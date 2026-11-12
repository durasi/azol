#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Tests of the pass `sroa` (compiler/opt/pass_sroa.azl).

    python3 tests/opt/run_sroa_tests.py [--azol PATH] [--random N] [--seed N] [--quick]

Builds `ir-tool` and `sroa-tool` with the azoL compiler (default:
./azol, then ./target/release/azol) and then:

1. tests/opt/sroa/NAME.ir, NAME.expected.ir, NAME.runs: the pass must
   turn the first into exactly the second, and both must do the same in
   the interpreter for every line of NAME.runs (`function arguments`).
2. Random functions that keep their variables in frame objects, with
   loops, branches, constant offsets, partly escaping objects and calls
   that write through a pointer (`--random N`, default 3000): the text
   before and after the pass must give the same output and exit code.
3. Every program the lowering proof uses (the list comes from
   run_opt_tests.py): lowered, through the pass, verified and
   interpreted, it gives the output, the messages and the exit code of
   its executable; the text after the pass reads back byte for byte.
4. The compiler itself: lowered, through the pass and interpreted, it
   builds examples/hello.azl, and itself, into the bytes `azol` writes.

`--quick` leaves out the compiler building itself.
Prints `ALL SROA TESTS PASSED` or the list of what failed; exit 0 or 1.
"""

import concurrent.futures
import os
import random
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import run_opt_tests as base  # noqa: E402

ROOT = base.ROOT
SROA = ["ir.azl", "ir_text.azl", "ir_cfg.azl", "ir_verify.azl", "ir_interp.azl", "ir_lower.azl", "ir_lower_stmt.azl", "ir_lower_expr.azl",
        "pass_sroa.azl", "sroa_tool_main.azl"]
UNLIMITED = ["sh", "-c", 'ulimit -s unlimited 2>/dev/null; exec "$@"', "sh"]


# ---- 1: the hand-written cases -------------------------------------------------

def hand_cases(binary, sroa, problems):
    folder = os.path.join(HERE, "sroa")
    names = sorted(name[:-3] for name in os.listdir(folder) if name.endswith(".ir") and not name.endswith(".expected.ir"))
    work = tempfile.mkdtemp(prefix="azol-sroa-hand-")
    runs = 0
    for name in names:
        source = os.path.join(folder, name + ".ir")
        expected = os.path.join(folder, name + ".expected.ir")
        out = os.path.join(work, name + ".out.ir")
        code, _, err = base.tool(sroa, source, "-o", out)
        if code != 0:
            problems.append("sroa/%s: the tool exits with %r: %r" % (name, code, err[-300:]))
            continue
        with open(out, "rb") as handle:
            got = handle.read()
        if "--update" in sys.argv:
            with open(expected, "wb") as handle:
                handle.write(got)
        if not os.path.exists(expected):
            problems.append("sroa/%s: there is no %s.expected.ir" % (name, name))
            continue
        with open(expected, "rb") as handle:
            want = handle.read()
        if got != want:
            problems.append("sroa/%s: the pass does not give the expected text; see %s" % (name, out))
        code, text, _ = base.tool(binary, "print", expected)
        if code != 0 or text != want:
            problems.append("sroa/%s: the expected text is not in canonical form" % name)
        lines = []
        if os.path.exists(os.path.join(folder, name + ".runs")):
            with open(os.path.join(folder, name + ".runs"), encoding="utf-8") as handle:
                lines = [line.split() for line in handle if line.strip() and not line.startswith("#")]
        if not lines:
            problems.append("sroa/%s: there is no line in %s.runs" % (name, name))
        for line in lines:
            before = base.tool(binary, "run", source, *line)
            after = base.tool(binary, "run", expected, *line)
            runs += 1
            if before != after:
                problems.append("sroa/%s: `%s` gives %r before the pass and %r after" % (name, " ".join(line), before, after))
            elif before[0] == 3:
                problems.append("sroa/%s: `%s` is refused by the interpreter: %r" % (name, " ".join(line), before[2][-200:]))
    shutil.rmtree(work)
    return len(names), runs


# ---- 2: random functions with frame objects ----------------------------------------

class Writer:
    """One random function. Variables are cells of frame objects; every
    statement loads what it reads and stores what it writes, as lowering
    does. All cells are stored once at the start, so no load reads bytes
    that were never stored."""

    def __init__(self, rng, name, helpers):
        self.rng = rng
        self.name = name
        self.helpers = helpers
        self.lines = []
        self.value = 0
        self.block = 0
        # (object, offset, type) of every cell, and the object sizes.
        self.sizes = []
        self.cells = []
        for obj in range(rng.randint(1, 5)):
            count = rng.randint(1, 4)
            self.sizes.append(count * 8)
            for c in range(count):
                self.cells.append((obj, c * 8, "bool" if rng.random() < 0.15 else "i64"))
        self.ints = [cell for cell in self.cells if cell[2] == "i64"]
        if not self.ints:
            self.sizes.append(8)
            self.cells.append((len(self.sizes) - 1, 0, "i64"))
            self.ints = [self.cells[-1]]
        self.bools = [cell for cell in self.cells if cell[2] == "bool"]
        self.frames = {}
        self.geps = {}
        self.fuel = rng.randint(8, 40)
        self.late_frames = []

    def new(self):
        self.value += 1
        return "v%d" % (self.value - 1)

    def emit(self, text):
        self.lines.append("    " + text)

    def label(self):
        self.block += 1
        return "b%d" % self.block

    def start(self, label):
        self.lines.append(label + ":")

    def address(self, cell):
        """An address value and the offset to use with it."""
        obj, off, _ = cell
        choices = [(self.frames[obj], off)]
        for (gobj, goff), value in self.geps.items():
            if gobj == obj and goff <= off:
                choices.append((value, off - goff))
        return self.rng.choice(choices)

    def load(self, cell):
        p, off = self.address(cell)
        v = self.new()
        self.emit("%s = load %s, %s, %d  [frame %d]" % (v, cell[2], p, off, cell[0]))
        return v

    def store(self, cell, value):
        p, off = self.address(cell)
        self.emit("store %s, %s, %d, %s  [frame %d]" % (cell[2], p, off, value, cell[0]))

    def number(self):
        """An `i64` operand: a constant, a parameter or a loaded cell."""
        roll = self.rng.random()
        if roll < 0.25:
            return str(self.rng.choice([0, 1, 2, 3, 7, -1, -5, 100, 1 << 40]))
        if roll < 0.4:
            return self.rng.choice(["p0", "p1"])
        if roll < 0.58:
            return self.behind()
        return self.load(self.rng.choice(self.ints))

    def behind(self):
        """A load of one of the four cells behind the address parameters."""
        k, off = self.rng.choice([(2, 0), (2, 8), (3, 0), (3, 8)])
        v = self.new()
        if k == 3 and off == 8 and self.rng.random() < 0.4:
            # The same bytes read as one byte: another entry of the
            # pass's table, which a store of eight bytes ends too.
            w = self.new()
            self.emit("%s = load u8, p3, 8  [param 3]" % w)
            self.emit("%s = add %s, 1" % (v, w))
            return v
        self.emit("%s = load i64, p%d, %d  [param %d]" % (v, k, off, k))
        return v

    def condition(self):
        if self.bools and self.rng.random() < 0.3:
            return self.load(self.rng.choice(self.bools))
        v = self.new()
        self.emit("%s = %s %s, %s" % (v, self.rng.choice(["slt", "sle", "eq", "ne", "sgt", "ult"]), self.number(), self.number()))
        return v

    def statement(self, depth):
        self.fuel -= 1
        roll = self.rng.random()
        if roll < 0.4 or depth > 3 or self.fuel <= 0:
            v = self.new()
            self.emit("%s = %s %s, %s" % (v, self.rng.choice(["add", "sub", "mul", "xor", "and", "or"]), self.number(), self.number()))
            self.store(self.rng.choice(self.ints), v)
        elif roll < 0.46 and self.bools:
            self.store(self.rng.choice(self.bools), self.condition())
        elif roll < 0.5:
            self.emit("callrt print_i64(%s)" % self.number())
        elif roll < 0.62:
            # Behind an address parameter: a store, or a call that
            # writes there or only reads.
            k, off = self.rng.choice([(2, 0), (2, 8), (3, 0), (3, 8)])
            kind = self.rng.random()
            if kind < 0.4:
                self.emit("store i64, p%d, %d, %s  [param %d]" % (k, off, self.number(), k))
            else:
                p = "p%d" % k
                if off:
                    p = self.new()
                    self.emit("%s = gep p%d, 0, 1, %d" % (p, k, off))
                self.emit("call %s(%s, %s)" % ("bump" if kind < 0.8 else "peek", p, self.number()))
            if self.rng.random() < 0.6:
                # Read it back at once: what the next blocks know of
                # this cell starts here.
                v = self.new()
                self.emit("%s = load i64, p%d, %d  [param %d]" % (v, k, off, k))
                self.store(self.rng.choice(self.ints), v)
        elif roll < 0.7 and self.helpers:
            # A call that reads and writes through an address: the cell
            # and everything after it in its object stays in memory.
            cell = self.rng.choice(self.ints)
            p, off = self.address(cell)
            if off:
                q = self.new()
                self.emit("%s = gep %s, 0, 1, %d" % (q, p, off))
                p = q
            self.emit("call %s(%s, %s)" % (self.rng.choice(self.helpers), p, self.number()))
        elif roll < 0.85:
            then, other, join = self.label(), self.label(), self.label()
            self.emit("br %s, %s, %s" % (self.condition(), then, other))
            self.start(then)
            for _ in range(self.rng.randint(1, 3)):
                self.statement(depth + 1)
            self.emit("jump %s" % join)
            self.start(other)
            for _ in range(self.rng.randint(0, 2)):
                self.statement(depth + 1)
            self.emit("jump %s" % join)
            self.start(join)
        else:
            # A loop that runs a fixed number of times; its counter is a
            # cell of its own object, so it is always a variable.
            self.sizes.append(8)
            counter = (len(self.sizes) - 1, 0, "i64")
            frame = self.new()
            self.late_frames.append("%s = frame %d" % (frame, counter[0]))
            self.frames[counter[0]] = frame
            head, body, done = self.label(), self.label(), self.label()
            self.store(counter, "0")
            self.emit("jump %s" % head)
            self.start(head)
            c = self.load(counter)
            test = self.new()
            self.emit("%s = slt %s, %d" % (test, c, self.rng.randint(0, 4)))
            self.emit("br %s, %s, %s" % (test, body, done))
            self.start(body)
            for _ in range(self.rng.randint(1, 3)):
                self.statement(depth + 1)
            c = self.load(counter)
            more = self.new()
            self.emit("%s = add %s, 1" % (more, c))
            self.store(counter, more)
            self.emit("jump %s" % head)
            self.start(done)

    def text(self):
        head = []
        for obj in range(len(self.sizes)):
            v = self.new()
            self.frames[obj] = v
            head.append("    %s = frame %d" % (v, obj))
        for obj, off, _ in self.cells:
            if off and self.rng.random() < 0.3 and (obj, off) not in self.geps:
                v = self.new()
                self.geps[(obj, off)] = v
                head.append("    %s = gep %s, 0, 1, %d" % (v, self.frames[obj], off))
        self.start("b1")
        self.block = 1
        for cell in self.cells:
            self.store(cell, "p0" if cell[2] == "i64" and self.rng.random() < 0.3 else "0")
        for _ in range(self.rng.randint(2, 6)):
            self.statement(0)
        total = "0"
        for cell in self.ints:
            v = self.new()
            self.emit("%s = add %s, %s" % (v, total, self.load(cell)))
            total = v
        for cell in self.bools:
            flag = self.load(cell)
            v = self.new()
            self.emit("%s = select %s, 1, 2" % (v, flag))
            w = self.new()
            self.emit("%s = add %s, %s" % (w, total, v))
            total = w
        for _ in range(2):
            v = self.new()
            self.emit("%s = add %s, %s" % (v, total, self.behind()))
            total = v
        self.emit("ret %s" % total)
        inner = self.name + ".in"
        out = ["fn %s(p0: i64, p1: i64, p2: ptr [param 2 read write], p3: ptr [param 3 read write]) -> i64 {" % inner,
               "    frames " + ", ".join(str(size) for size in self.sizes), "b0:"]
        out += head + ["    " + line for line in self.late_frames] + ["    jump b1"] + self.lines + ["}", ""]
        # The function the test runs: two objects for the inner one to
        # work behind, and what they hold afterwards in the result.
        out += ["fn %s(p0: i64, p1: i64) -> i64 {" % self.name, "    frames 16, 16", "b0:", "    v0 = frame 0", "    v1 = frame 1",
                "    store i64, v0, 0, p0  [frame 0]", "    store i64, v0, 8, 3  [frame 0]",
                "    store i64, v1, 0, p1  [frame 1]", "    store i64, v1, 8, 11  [frame 1]",
                "    v2 = call %s(p0, p1, v0, v1) -> i64" % inner,
                "    v3 = load i64, v0, 0  [frame 0]", "    v4 = load i64, v0, 8  [frame 0]",
                "    v5 = load i64, v1, 0  [frame 1]", "    v6 = load i64, v1, 8  [frame 1]",
                "    v7 = mul v3, 3", "    v8 = mul v4, 5", "    v9 = mul v5, 7", "    v10 = mul v6, 11",
                "    v11 = add v2, v7", "    v12 = add v11, v8", "    v13 = add v12, v9", "    v14 = add v13, v10",
                "    ret v14", "}"]
        return "\n".join(out)


HELPERS = """fn bump(p0: ptr [param 0 read write], p1: i64) {
b0:
    v0 = load i64, p0, 0  [param 0]
    v1 = add v0, p1
    store i64, p0, 0, v1  [param 0]
    ret
}

fn peek(p0: ptr [param 0 read], p1: i64) {
b0:
    v0 = load i64, p0, 0  [param 0]
    v1 = load i64, p0, 0  [param 0]
    v2 = add v0, v1
    v3 = xor v2, p1
    callrt print_i64(v3)
    ret
}
"""


def random_cases(binary, sroa, total, seed, problems):
    rng = random.Random(seed)
    work = tempfile.mkdtemp(prefix="azol-sroa-random-")
    per_file = 20
    done = 0
    promoted = 0
    at = 0
    while done < total and len(problems) < 5:
        names = ["f%d" % k for k in range(per_file)]
        parts = [HELPERS.rstrip("\n")]
        for name in names:
            parts.append(Writer(rng, name, ["bump", "peek"]).text())
        source = os.path.join(work, "r%d.ir" % at)
        out = os.path.join(work, "r%d.out.ir" % at)
        at += 1
        with open(source, "w", encoding="utf-8") as handle:
            handle.write("\n\n".join(parts) + "\n")
        code, text, err = base.tool(binary, "verify", source)
        if code != 0:
            problems.append("random: the generator wrote a text that does not verify (%s): %r" % (source, err[-300:]))
            break
        code, _, err = base.tool(sroa, source, "-o", out, "--stats")
        if code != 0:
            problems.append("random: the tool exits with %r on %s: %r" % (code, source, err[-300:]))
            break
        for line in err.decode().split("\n"):
            if line.startswith("cells "):
                promoted += int(line.split("variables ")[1].split(" ")[0]) + int(line.split("shadowed ")[1].split(" ")[0])
        code, text, _ = base.tool(binary, "print", out)
        with open(out, "rb") as handle:
            if code != 0 or text != handle.read():
                problems.append("random: the text after the pass does not read back the same (%s)" % out)
        for name in names:
            arguments = [str(rng.choice([0, 1, -1, 5, 1000, -77])), str(rng.choice([0, 2, 9, -3]))]
            before = base.tool(binary, "run", source, name, *arguments)
            after = base.tool(binary, "run", out, name, *arguments)
            done += 1
            if before != after or before[0] not in range(0, 256) or before[0] == 3 and before[2].startswith(b"ir-interp"):
                problems.append("random: %s %s in %s gives %r before the pass and %r after" % (name, " ".join(arguments), source, before, after))
                break
        else:
            os.unlink(source)
            os.unlink(out)
    if not problems:
        shutil.rmtree(work)
    return done, promoted


# ---- 3 and 4: programs, and the compiler ----------------------------------------------

def program_one(azol, sroa, binary, folder, at, label, files, want):
    exe = os.path.join(folder, "exe%d" % at)
    done = subprocess.run([azol, "build", *files, "-o", exe], capture_output=True)
    if done.returncode != 0:
        code, _, _ = base.tool(sroa, "emit", *files)
        return None, [] if code == 1 else ["sroa %s: `azol build` refuses it but `sroa-tool emit` exits with %r" % (label, code)]
    found = []
    work = os.path.join(folder, "run%d" % at)
    results = []
    for command in ([exe], UNLIMITED + [sroa, "run", "--argv0", exe, *files]):
        if os.path.isdir(work):
            shutil.rmtree(work)
        os.mkdir(work)
        try:
            ran = subprocess.run(command, capture_output=True, timeout=900, cwd=work)
            results.append((ran.returncode, ran.stdout, ran.stderr))
        except subprocess.TimeoutExpired:
            results.append((None, b"", b"(no answer in time)"))
    shutil.rmtree(work)
    real, promoted = results
    if real != promoted:
        found.append("sroa %s: the executable gives exit %r, %r, %r; the interpreted IR after the pass gives exit %r, %r, %r" %
                     (label, real[0], real[1][-200:], real[2][-200:], promoted[0], promoted[1][-200:], promoted[2][-300:]))
    if want is not None and (promoted[0] != want[0] or promoted[1] != want[1] or (want[2] is not None and promoted[2] != want[2])):
        found.append("sroa %s: the interpreted IR after the pass does not do what the `// expect-` comments say (exit %r, %r)" % (label, promoted[0], promoted[2][-300:]))
    code, text, err = base.tool(sroa, "emit", *files)
    if code != 0:
        found.append("sroa %s: `emit` exits with %r: %r" % (label, code, err[-300:]))
        return "run", found
    emitted = os.path.join(folder, "emit%d.ir" % at)
    with open(emitted, "wb") as handle:
        handle.write(text)
    code, out, err = base.tool(binary, "verify", emitted)
    if code != 0 or out != b"ok\n":
        found.append("sroa %s: the text after the pass does not verify: %r" % (label, err[-300:]))
    code, out, err = base.tool(binary, "print", emitted)
    if code != 0 or out != text:
        found.append("sroa %s: `ir-tool print` does not give the text after the pass back (exit %r, %r)" % (label, code, err[-300:]))
    os.unlink(emitted)
    os.unlink(exe)
    return "run", found


def programs(azol, sroa, binary, problems):
    folder = tempfile.mkdtemp(prefix="azol-sroa-programs-")
    listed = base.lower_programs(folder, problems)
    compared = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=os.cpu_count() or 2) as pool:
        jobs = [pool.submit(program_one, azol, sroa, binary, folder, at, label, files, want) for at, (label, files, want) in enumerate(listed)]
        for job in jobs:
            what, found = job.result()
            problems.extend(found)
            if what is not None:
                compared += 1
    if not problems:
        shutil.rmtree(folder)
    return compared


def compiler_sources():
    import platform
    host = {("Linux", "x86_64"): "host_x86_64_linux.azl", ("Linux", "aarch64"): "host_aarch64_linux.azl", ("Darwin", "arm64"): "host_aarch64_macos.azl"}.get((platform.system(), platform.machine()))
    if host is None:
        return None
    with open(os.path.join(ROOT, "compiler", "SOURCES"), encoding="utf-8") as handle:
        sources = [os.path.join(ROOT, "compiler", line.strip()) for line in handle if line.strip() and not line.startswith("#")]
    sources.append(os.path.join(ROOT, "compiler", host))
    return sources


def compiler(azol, sroa, binary, itself, problems):
    sources = compiler_sources()
    if sources is None:
        return []
    folder = tempfile.mkdtemp(prefix="azol-sroa-compiler-")
    code, text, err = base.tool(sroa, "emit", *sources, timeout=300)
    if code != 0:
        problems.append("sroa on the compiler: `emit` exits with %r: %r" % (code, err[-300:]))
        return []
    emitted = os.path.join(folder, "compiler.ir")
    with open(emitted, "wb") as handle:
        handle.write(text)
    code, out, err = base.tool(binary, "print", emitted, timeout=300)
    if code != 0 or out != text:
        problems.append("sroa on the compiler: `ir-tool print` does not give the text back")
    built = []
    hello = os.path.join(ROOT, "examples", "hello.azl")
    for label, files in [("examples/hello.azl", [hello])] + ([("itself", sources)] if itself else []):
        real = os.path.join(folder, "built-real")
        interpreted = os.path.join(folder, "built-interpreted")
        done = subprocess.run([azol, "build", *files, "-o", real], capture_output=True)
        try:
            ran = subprocess.run(UNLIMITED + [sroa, "run", *sources, "--", "build", *files, "-o", interpreted], capture_output=True, timeout=1800)
        except subprocess.TimeoutExpired:
            problems.append("the compiler after the pass, interpreted, does not finish building %s" % label)
            continue
        if done.returncode != 0 or ran.returncode != 0:
            problems.append("the compiler after the pass, interpreted, building %s: exit %r (azol: %r), %r" % (label, ran.returncode, done.returncode, ran.stderr[-300:]))
            continue
        with open(real, "rb") as first, open(interpreted, "rb") as second:
            if first.read() != second.read():
                problems.append("the compiler after the pass, interpreted, builds %s differently from %s" % (label, azol))
            else:
                built.append(label)
    if not problems:
        shutil.rmtree(folder)
    return built


def main():
    argv = sys.argv[1:]
    azol = base.find_azol(argv)
    count = int(argv[argv.index("--random") + 1]) if "--random" in argv else 3000
    seed = int(argv[argv.index("--seed") + 1]) if "--seed" in argv else 20261112
    tools = tempfile.mkdtemp(prefix="azol-sroa-tools-")
    binary = os.path.join(tools, "ir-tool")
    sroa = os.path.join(tools, "sroa-tool")
    base.build(azol, base.SOURCES, binary)
    base.build(azol, [os.path.join(ROOT, "compiler", name) for name in base.FRONT] + SROA, sroa)
    problems = []
    cases, runs = hand_cases(binary, sroa, problems)
    print("%d hand-written cases, %d runs before and after" % (cases, runs))
    done, promoted = random_cases(binary, sroa, count, seed, problems)
    print("%d random functions with frame objects (%d cells became values or shadowed), each run before and after" % (done, promoted))
    if "--only-small" not in argv:
        compared = programs(azol, sroa, binary, problems)
        print("%d programs: lowered, through the pass, interpreted like their executables" % compared)
        built = compiler(azol, sroa, binary, "--quick" not in argv, problems)
        print("the compiler through the pass, interpreted, builds: %s" % (", ".join(built) if built else "nothing"))
    shutil.rmtree(tools, ignore_errors=True)
    if problems:
        for problem in problems[:40]:
            print("FAILED  " + problem)
        print("%d SROA TESTS FAILED" % len(problems))
        sys.exit(1)
    print("ALL SROA TESTS PASSED")


if __name__ == "__main__":
    main()
