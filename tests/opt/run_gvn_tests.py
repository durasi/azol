#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Tests of the passes `effects` and `gvn` (compiler/opt/pass_effects.azl,
pass_gvn.azl).

    python3 tests/opt/run_gvn_tests.py [--azol PATH] [--random N] [--seed N] [--keep]
                                       [--only cases|random|programs|compiler]

Needs only Python 3 and an azoL compiler (default: ./azol). It builds
the tools of run_pass_tests.py and then:

1. cases     tests/opt/gvn and tests/opt/effects: every NAME.ir must
             come out of `opt-tool --passes <the folder's pass>` as
             NAME.expected.ir, byte for byte, run the same in the
             interpreter on every line of NAME.runs, and come out of
             the pass unchanged a second time. A case whose name starts
             with `no_` is one where a rule must not fire; its first
             comment line says why. (`gvn` alone takes what a callee
             does from the words in the callee's own first line.)
2. random    random modules made here: functions that read and write
             two lists through their headers, with and without bounds
             checks, keep their variables in frame memory, call each
             other, push, branch and loop. Each module must print the
             same before the passes and after `gvn`, after
             `effects,gvn` and after the whole order with `--fixpoint`,
             every time with `--verify-each`; a part of them also as
             native code, and a part built for ARM64 and run in the
             emulators of run_back_arm64_tests.py (skipped where those
             are missing).
3. programs  every program the lowering proof uses, through the whole
             order: interpreted, and built by `azol-opt` with all
             registers and with three, like its default build
             (run_pass_tests.py, whose order holds these passes).
4. compiler  the compiler built by `azol-opt` builds examples/hello.azl
             and itself into the bytes `azol` writes.

run_opt_tests.py runs parts 1 and 2; parts 3 and 4 are there already,
in run_pass_tests.py.
"""
import os
import random
import shutil
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import run_opt_tests as base
import run_pass_tests as pass_tests
import run_back_arm64_tests as arm64

FOLDERS = ["gvn", "effects"]
ALL = "read write writebuf resize free"
OPS = ["add", "sub", "mul", "xor", "and", "or"]
SLOTS = 6


class Function:
    """One random function: `p0` is a list it may change, `p1` one it
    may only read, `p2` a number. Its variables are the cells of frame
    object 0."""

    def __init__(self, rng, index):
        self.rng = rng
        self.index = index
        self.values = 0
        self.blocks = [["b0", []]]
        self.frames = [8 * SLOTS, 8]
        self.trap = False
        self.cell = None
        self.push_slot = None

    def value(self):
        self.values += 1
        return "v%d" % (self.values - 1)

    def emit(self, text):
        self.blocks[-1][1].append("    " + text)

    def block(self):
        self.blocks.append(["b%d" % len(self.blocks), []])
        return self.blocks[-1][0]

    def label(self):
        return "b%d" % len(self.blocks)

    def read(self, slot):
        v = self.value()
        self.emit("%s = load i64, %s, %d  [frame 0]" % (v, self.cell, 8 * slot))
        return v

    def write(self, slot, v):
        self.emit("store i64, %s, %d, %s  [frame 0]" % (self.cell, 8 * slot, v))

    def element(self, which, index, checked):
        """The address of element `index & 7` of list `which`."""
        masked = self.value()
        self.emit("%s = and %s, 7" % (masked, index))
        if checked:
            length = self.value()
            self.emit("%s = load i64, p%d, 8  [param %d]" % (length, which, which))
            fine = self.value()
            self.emit("%s = ult %s, %s" % (fine, masked, length))
            self.trap = True
            self.emit("br %s, %s, btrap" % (fine, self.label()))
            self.block()
        buffer = self.value()
        self.emit("%s = load ptr, p%d, 0  [param %d]" % (buffer, which, which))
        at = self.value()
        self.emit("%s = gep %s, %s, 8, 0" % (at, buffer, masked))
        return at

    def field(self, which, place):
        buffer = self.value()
        self.emit("%s = load ptr, p%d, 0  [param %d]" % (buffer, which, which))
        v = self.value()
        self.emit("%s = load i64, %s, %d  [buf(param %d)]" % (v, buffer, place, which))
        return v

    def expression(self, depth):
        rng = self.rng
        kind = rng.randrange(10)
        if kind < 3:
            a = self.read(rng.randrange(SLOTS))
            b = self.read(rng.randrange(SLOTS)) if rng.random() < 0.7 else str(rng.randrange(-9, 99))
            v = self.value()
            self.emit("%s = %s %s, %s" % (v, rng.choice(OPS), a, b))
            return v
        if kind < 7:
            which = rng.randrange(2)
            at = self.element(which, self.read(rng.randrange(SLOTS)), rng.random() < 0.6)
            v = self.value()
            self.emit("%s = load i64, %s, 0  [buf(param %d)]" % (v, at, which))
            return v
        if kind == 7:
            which = rng.randrange(2)
            v = self.value()
            self.emit("%s = load i64, p%d, 8  [param %d]" % (v, which, which))
            return v
        if kind == 8:
            # A field at a fixed place: element 0 to 7 without a `gep`.
            return self.field(rng.randrange(2), 8 * rng.randrange(8))
        if self.index > 0 and depth < 1:
            a = self.read(rng.randrange(SLOTS))
            v = self.value()
            self.emit("%s = call f%d(p0, p1, %s) -> i64" % (v, rng.randrange(self.index), a))
            return v
        return self.read(rng.randrange(SLOTS))

    def statement(self, depth):
        rng = self.rng
        kind = rng.randrange(20)
        if kind < 9:
            v = self.expression(depth)
            self.write(rng.randrange(SLOTS), v)
            if rng.random() < 0.3:
                # The same again: something for the pass to find.
                self.write(rng.randrange(SLOTS), self.expression(depth))
        elif kind < 14:
            value = self.read(rng.randrange(SLOTS))
            at = self.element(0, self.read(rng.randrange(SLOTS)), rng.random() < 0.6)
            self.emit("store i64, %s, 0, %s  [buf(param 0)]" % (at, value))
        elif kind == 14:
            value = self.read(rng.randrange(SLOTS))
            self.emit("store i64, %s, 0, %s  [frame 1]" % (self.push_slot, value))
            self.emit("callrt list_push(p0, %s, 8)" % self.push_slot)
        elif kind == 15:
            # The same field read on both sides of something that may
            # or may not write it: a call, a push, a store.
            which = rng.randrange(2)
            place = 8 * rng.randrange(8)
            first = self.field(which, place)
            between = rng.randrange(4)
            if between == 0 and self.index > 0:
                given = self.read(rng.randrange(SLOTS))
                got = self.value()
                self.emit("%s = call f%d(p0, p1, %s) -> i64" % (got, rng.randrange(self.index), given))
                self.write(rng.randrange(SLOTS), got)
            elif between == 1:
                self.emit("store i64, %s, 0, %s  [frame 1]" % (self.push_slot, first))
                self.emit("callrt list_push(p0, %s, 8)" % self.push_slot)
            elif between == 2:
                at = self.element(0, self.read(rng.randrange(SLOTS)), False)
                self.emit("store i64, %s, 0, %d  [buf(param 0)]" % (at, rng.randrange(1000)))
            else:
                buffer = self.value()
                self.emit("%s = load ptr, p0, 0  [param 0]" % buffer)
                self.emit("store i64, %s, %d, %d  [buf(param 0)]" % (buffer, 8 * rng.randrange(8), rng.randrange(1000)))
            second = self.field(which, place)
            both = self.value()
            self.emit("%s = sub %s, %s" % (both, first, second))
            self.write(rng.randrange(SLOTS), both)
            self.write(rng.randrange(SLOTS), second)
        elif kind < 18 and depth < 3:
            test = self.read(rng.randrange(SLOTS))
            low = self.value()
            self.emit("%s = and %s, 1" % (low, test))
            cond = self.value()
            self.emit("%s = eq %s, 0" % (cond, low))
            branch = len(self.blocks) - 1
            first = self.block()
            for _ in range(rng.randrange(1, 4)):
                self.statement(depth + 1)
            first_end = len(self.blocks) - 1
            second = self.block()
            for _ in range(rng.randrange(0, 3)):
                self.statement(depth + 1)
            after = self.label()
            self.emit("jump %s" % after)
            self.blocks[first_end][1].append("    jump %s" % after)
            self.blocks[branch][1].append("    br %s, %s, %s" % (cond, first, second))
            self.block()
        elif depth < 2:
            self.frames.append(8)
            counter = self.value()
            self.emit("%s = frame %d" % (counter, len(self.frames) - 1))
            region = "[frame %d]" % (len(self.frames) - 1)
            self.emit("store i64, %s, 0, 0  %s" % (counter, region))
            top = self.label()
            self.emit("jump %s" % top)
            self.block()
            n = self.value()
            self.emit("%s = load i64, %s, 0  %s" % (n, counter, region))
            more = self.value()
            self.emit("%s = slt %s, %d" % (more, n, rng.randrange(1, 4)))
            head = len(self.blocks) - 1
            body = self.block()
            for _ in range(rng.randrange(1, 4)):
                self.statement(depth + 1)
            n = self.value()
            self.emit("%s = load i64, %s, 0  %s" % (n, counter, region))
            up = self.value()
            self.emit("%s = add %s, 1" % (up, n))
            self.emit("store i64, %s, 0, %s  %s" % (counter, up, region))
            self.emit("jump %s" % top)
            after = self.label()
            self.blocks[head][1].append("    br %s, %s, %s" % (more, body, after))
            self.block()
        else:
            self.write(rng.randrange(SLOTS), self.expression(depth))

    def text(self):
        rng = self.rng
        self.cell = self.value()
        self.emit("%s = frame 0" % self.cell)
        self.push_slot = self.value()
        self.emit("%s = frame 1" % self.push_slot)
        self.write(0, "p2")
        for slot in range(1, SLOTS):
            self.write(slot, str(rng.randrange(-5, 50)))
        for _ in range(rng.randrange(3, 9)):
            self.statement(0)
        total = self.read(0)
        for slot in range(1, SLOTS):
            more = self.value()
            self.emit("%s = add %s, %s" % (more, total, self.read(slot)))
            total = more
        self.emit("ret %s" % total)
        out = ["fn f%d(p0: ptr [param 0 %s], p1: ptr [param 1 read], p2: i64) -> i64 {" % (self.index, ALL)]
        out.append("    frames " + ", ".join(str(size) for size in self.frames))
        for label, rows in self.blocks:
            out.append(label + ":")
            out.extend(rows)
        if self.trap:
            out.append("btrap:")
            out.append("    trap index")
        out.append("}")
        text = "\n".join(out)
        # The trap block gets a number like the others.
        return text.replace("btrap", "b%d" % len(self.blocks))


def random_module(rng):
    """A module of random functions and a `main` that makes the two
    lists, calls the functions and prints what they give and what the
    first list holds afterwards. Gives (text, number of functions)."""
    count = rng.randrange(2, 7)
    parts = [Function(rng, index).text() for index in range(count)]
    rows = ["fn main() {", "    frames 24, 24", "b0:"]
    n = 0
    for which in range(2):
        rows.append("    v%d = frame %d" % (n, which))
        rows.append("    v%d = callrt alloc(64)" % (n + 1))
        rows.append("    store ptr, v%d, 0, v%d  [frame %d]" % (n, n + 1, which))
        rows.append("    store i64, v%d, 8, 8  [frame %d]" % (n, which))
        rows.append("    store i64, v%d, 16, 8  [frame %d]" % (n, which))
        for k in range(8):
            rows.append("    store i64, v%d, %d, %d  [buf(frame %d)]" % (n + 1, 8 * k, rng.randrange(-20, 200), which))
        n += 2
    for _ in range(rng.randrange(2, 5)):
        rows.append("    v%d = call f%d(v0, v2, %d) -> i64" % (n, rng.randrange(count), rng.randrange(-9, 99)))
        rows.append("    callrt print_i64(v%d)" % n)
        n += 1
    rows.append("    v%d = load ptr, v0, 0  [frame 0]" % n)
    rows.append("    v%d = load i64, v0, 8  [frame 0]" % (n + 1))
    rows.append("    callrt print_i64(v%d)" % (n + 1))
    for k in range(8):
        rows.append("    v%d = load i64, v%d, %d  [buf(frame 0)]" % (n + 2 + k, n, 8 * k))
        rows.append("    callrt print_i64(v%d)" % (n + 2 + k))
    rows += ["    callrt drop(v0)", "    callrt drop(v2)", "    ret", "}", ""]
    return "\n\n".join(parts) + "\n\n" + "\n".join(rows), count


ORDERS = [("gvn", ["--passes", "gvn"]), ("effects,gvn", ["--passes", "effects,gvn"]), ("the whole order", ["--fixpoint"])]


def random_tests(tools, total, seed, folder, problems):
    rng = random.Random(seed)
    made = 0
    files = 0
    natives = 0
    arm = 0
    loads = [0, 0]
    checks = [0, 0]
    have_arm = arm64.emulators() is not None
    while made < total and len(problems) < 5:
        text, count = random_module(rng)
        files += 1
        made += count
        path = os.path.join(folder, "gvn%d.ir" % files)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        before = base.tool(tools["ir"], "run", path, "main", timeout=120)
        if before[0] not in (0, 1):
            problems.append("random file %d (seed %d): the interpreter gives exit %r, %r before any pass; kept as %s" % (files, seed, before[0], base.lines(before[2])[:2], path))
            continue
        wrong = False
        after = path
        for label, arguments in ORDERS:
            after = os.path.join(folder, "gvn%d.after.ir" % files)
            code, out, err = base.tool(tools["opt"], path, "-o", after, "--verify-each", *arguments, timeout=300)
            if code != 0:
                problems.append("random file %d (seed %d): %s fails (exit %r, %r); kept as %s" % (files, seed, label, code, base.lines(err)[:3], path))
                wrong = True
                break
            result = base.tool(tools["ir"], "run", after, "main", timeout=120)
            if result != before:
                problems.append("random file %d (seed %d): after %s `run main` gives %r, before %r; kept as %s" % (files, seed, label, result, before, path))
                wrong = True
                break
            if label == "effects,gvn":
                with open(after, encoding="utf-8") as handle:
                    left = handle.read()
                loads[0] += text.count(" = load ")
                loads[1] += left.count(" = load ")
                checks[0] += text.count(" = ult ")
                checks[1] += left.count(" = ult ")
        if wrong:
            continue
        if pass_tests.native() and files % 4 == 0 and before[0] == 0:
            one = pass_tests.run_native(tools, path, folder, "ga", "main", [])
            two = pass_tests.run_native(tools, after, folder, "gb", "main", [])
            natives += 1
            if one != two:
                problems.append("random file %d (seed %d): as native code `main` gives %r before the passes and %r after; kept as %s" % (files, seed, one, two, path))
                continue
        if have_arm and files % 25 == 0:
            found = len(problems)
            arm += arm64.compare(tools["ir"], tools["back"], after, ["main"], os.path.join(folder, "gvn-arm64"), problems, arm64.MODES[:1])
            if len(problems) != found:
                continue
        os.unlink(path)
        os.unlink(after)
    return made, files, natives, arm, loads, checks


def run(azol, folder, total, seed, problems, only=None, tools=None):
    if tools is None:
        tools = pass_tests.build_tools(azol, folder)
    if only in (None, "cases"):
        kept = pass_tests.FOLDERS
        pass_tests.FOLDERS = FOLDERS
        try:
            count, quiet = pass_tests.case_tests(tools, problems)
        finally:
            pass_tests.FOLDERS = kept
        print("gvn: %d hand-written cases of `gvn` and `effects`, %d of them where a rule must not fire" % (count, quiet))
    if only in (None, "random") and total > 0:
        made, files, natives, arm, loads, checks = random_tests(tools, total, seed, folder, problems)
        print("gvn: %d random functions with memory and calls in %d files print the same before and after (`gvn`, `effects,gvn`, the whole order); %d files also as native code, %d programs for ARM64 in the emulators; `effects,gvn` leaves %d of %d loads and %d of %d bounds comparisons" %
              (made, files, natives, arm, loads[1], loads[0], checks[1], checks[0]))
    return tools


def main():
    argv = sys.argv[1:]
    azol = base.find_azol(argv)
    total = int(argv[argv.index("--random") + 1]) if "--random" in argv else 3000
    seed = int(argv[argv.index("--seed") + 1]) if "--seed" in argv else 20261112
    only = argv[argv.index("--only") + 1] if "--only" in argv else None
    folder = tempfile.mkdtemp(prefix="azol-gvn-")
    problems = []
    tools = run(azol, folder, total, seed, problems, only)
    if only in (None, "programs"):
        compared = pass_tests.program_tests(azol, tools, folder, problems)
        print("gvn: %d whole programs through the whole order: interpreted, and built by azol-opt (also with three registers), like their default builds" % compared)
    if only in (None, "compiler"):
        if pass_tests.compiler_test(azol, tools, folder, problems):
            print("gvn: the compiler, optimised and built by azol-opt, builds examples/hello.azl and itself as azol does")
    for problem in problems:
        print("FAIL: " + problem)
    if problems:
        print("%d problem(s); files kept in %s" % (len(problems), folder))
        return 1
    if "--keep" not in argv:
        shutil.rmtree(folder)
    print("ALL GVN TESTS PASSED")
    return 0


if __name__ == "__main__":
    sys.exit(main())
