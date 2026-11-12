#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Tests of the optimising back end for x86-64 Linux (compiler/opt:
regalloc.azl, sel_x64.azl, emit_x64.azl), through `azol-back`.

    python3 tests/opt/run_back_tests.py [--azol PATH] [--random N] [--seed S] [--keep]

`run_opt_tests.py` runs these too, after its own. The measure of right
is the interpreter of the intermediate form (`ir-tool run`): a program
built by the back end must print the same lines, write the same to
standard error and exit with the same code.

1. Every `; run:` case of every `tests/opt/*.ir` file that the
   interpreter runs to an end (a case it stops for breaking a rule
   cannot be compared: the processor does not notice).

2. Random modules (default: until 5,000 functions): several functions
   that call each other, with up to eleven parameters and six results,
   addresses as parameters, loads and stores of every width in frame
   objects, in parameters and in allocated blocks, `gep`, loops, `if`s
   with and without `else`, `switch`, values that stay alive across all
   of them, prints and traps.

3. The hand-written kernels of four benchmarks (tests/opt/kernels).

4. When the lowering stage is in the tree: every program of
   tests/programs and examples, lowered (`azol-lower emit`), built by the
   back end as a whole program and compared with its default build.

Every program is built twice: as it is, and with the allocator limited
to three registers, so that everything is spilled, split and moved. The
allocation of every function is verified by the allocator's checker
(`--check`) both times. A sample of the programs also runs under
valgrind's memcheck, when valgrind is there.

Only the Python standard library is used.
"""

import os
import random
import re
import shutil
import subprocess
import sys
import tempfile

import run_opt_tests as base

HERE = base.HERE
ROOT = base.ROOT
BACK_SOURCES = [
    os.path.join(ROOT, "compiler", name) for name in ("bytes.azl", "code_file.azl", "elf.azl", "runtime_x64.azl", "runtime_arm64.azl", "macho.azl", "sha256.azl")
] + [
    "ir.azl", "ir_text.azl", "ir_cfg.azl", "ir_verify.azl", "mach.azl", "regalloc.azl", "enc_x64.azl",
    "sel_x64.azl", "emit_x64.azl", "back_x64.azl", "enc_arm64.azl", "sel_arm64.azl", "emit_arm64.azl", "back_arm64.azl", "back_tool_main.azl",
]
MODES = ([], ["--stress"])


def native_supported():
    return sys.platform.startswith("linux") and os.uname().machine in ("x86_64", "amd64")


def expected_lines(code, out):
    """What the built program prints for what the interpreter printed:
    the results one on a line."""
    found = base.lines(out)
    if code == 0 and found and found[-1].startswith("=>"):
        rest = found.pop()[2:].strip()
        found.append("=>")
        if rest:
            found += [part.strip() for part in rest.split(",")]
    return found


def compare(ir_tool, back, path, arguments, exe, problems, valgrind=False):
    """Builds and runs one case in both modes. Gives how many programs
    were compared (0 when the interpreter stopped the case itself)."""
    code, out, err = base.tool(ir_tool, "run", path, *arguments)
    if code is None or code == 3:
        return 0
    name = os.path.basename(path) + " " + " ".join(arguments)
    want = (code, expected_lines(code, out), err)
    done = 0
    for mode in MODES:
        command = [back, "build", path, "-o", exe, "--check"] + mode
        if arguments:
            command += ["--run"] + list(arguments)
        built = subprocess.run(command, capture_output=True)
        label = name + (" (three registers)" if mode else "")
        if built.returncode != 0:
            problems.append("%s: azol-back: %s" % (label, built.stderr.decode(errors="replace").strip()[:400]))
            continue
        got_code, got_out, got_err = base.tool(exe, timeout=60)
        got = (got_code, base.lines(got_out), got_err)
        if got != want:
            problems.append("%s: the program gives %r, the interpreter %r" % (label, got, want))
        elif valgrind:
            checked = subprocess.run(["valgrind", "-q", "--error-exitcode=99", exe], capture_output=True)
            if checked.returncode == 99 or b"== " in checked.stderr:
                problems.append("%s: valgrind: %s" % (label, checked.stderr.decode(errors="replace")[:600]))
        done += 1
    return done


# ---- random modules -----------------------------------------------------------

BIG = [0, 1, -1, 2, 7, -8, 255, 256, 65535, 2147483647, -2147483648, 2147483648, 4294967295, 4294967296,
       9223372036854775807, -9223372036854775808, 1311768467463790320, -81985529216486896]
BINARY = ["add", "sub", "mul", "and", "or", "xor", "shl", "sar", "shr", "umulh", "smulh"]
COMPARE = ["eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge"]
UNARY = ["neg", "ext8u", "ext8s", "ext32u", "ext32s", "trunc8", "trunc32"]
WIDTHS = [("i8", 1), ("u8", 1), ("i32", 4), ("u32", 4), ("i64", 8), ("u64", 8)]
BUDGET = 300000


BLOCK_NAME = re.compile(r"\bb(\d+)\b")


class Maker:
    """One random function, made block by block. The values in `ints`,
    `bools` and `ptrs` are those that can be used at the point where
    text is being added: all of them are defined on every path there."""

    def __init__(self, rng, index, earlier, is_main):
        self.rng = rng
        self.earlier = earlier
        self.name = "main" if is_main else "f%d" % index
        self.blocks = []
        self.values = 0
        self.cost = 0
        self.ints = []
        self.bools = []
        # (value, region, bytes)
        self.ptrs = []
        count = rng.choice([1, 2, 3]) if is_main else rng.choice([0, 1, 2, 2, 3, 3, 4, 5, 9, 10, 11])
        self.params = []
        for k in range(count):
            kind = "i64" if is_main else rng.choice(["i64"] * 6 + ["bool", "bool", "ptr", "ptr"])
            self.params.append(kind)
            if kind == "i64":
                self.ints.append("p%d" % k)
            elif kind == "bool":
                self.bools.append("p%d" % k)
            else:
                self.ptrs.append(("p%d" % k, "param %d" % k, 32))
        self.rets = [rng.choice(["i64", "i64", "i64", "bool"]) for _ in range(rng.choice([0, 1, 1, 1, 2, 2, 3, 5, 6]))]
        self.frames = [rng.choice([32, 32, 48, 64]) for _ in range(rng.randint(1, 3))]
        if rng.random() < 0.04:
            self.frames.append(rng.choice([4096, 20000, 70000]))
        self.cur = self.block([])

    # -- text --

    def block(self, params):
        self.blocks.append({"params": params, "lines": []})
        return len(self.blocks) - 1

    def add(self, line):
        self.blocks[self.cur]["lines"].append(line)
        self.cost += self.scale

    def new(self):
        self.values += 1
        return "v%d" % (self.values - 1)

    def define(self, text):
        v = self.new()
        self.add("%s = %s" % (v, text))
        return v

    def text(self):
        params = []
        for k, kind in enumerate(self.params):
            params.append("p%d: %s%s" % (k, kind, " [param %d read write]" % k if kind == "ptr" else ""))
        head = "fn %s(%s)" % (self.name, ", ".join(params))
        if self.rets:
            head += " -> " + ", ".join(self.rets)
        out = [head + " {", "    frames " + ", ".join(str(n) for n in self.frames)]
        # In a third of the functions the blocks after the first are laid
        # out in another order than they were made in: then a block that
        # reads a value can come before the block that defines it, and
        # the allocator meets intervals whose definition is not in their
        # first range.
        order = list(range(len(self.blocks)))
        if len(order) > 2 and self.rng.random() < 0.34:
            rest = order[1:]
            self.rng.shuffle(rest)
            order = [0] + rest
        name = {old: "b%d" % new for new, old in enumerate(order)}

        def renamed(line):
            return BLOCK_NAME.sub(lambda match: name[int(match.group(1))], line)

        for old in order:
            block = self.blocks[old]
            label = name[old]
            if block["params"]:
                label += "(" + ", ".join("%s: %s" % pair for pair in block["params"]) + ")"
            out.append(label + ":")
            out += ["    " + renamed(line) for line in block["lines"]]
        out.append("}")
        return "\n".join(out) + "\n"

    # -- operands --

    def int(self):
        rng = self.rng
        if not self.ints or rng.random() < 0.2:
            if rng.random() < 0.5:
                return str(rng.randint(-20, 20))
            return str(rng.choice(BIG))
        if rng.random() < 0.5:
            return rng.choice(self.ints[-8:])
        return rng.choice(self.ints)

    def int_value(self):
        """An i64 that is a value, not a constant."""
        if not self.ints:
            self.ints.append(self.define("add %s, %s" % (self.rng.randint(-9, 9), self.rng.choice(BIG))))
        return self.rng.choice(self.ints)

    def bool(self):
        if not self.bools or self.rng.random() < 0.1:
            if self.rng.random() < 0.3:
                self.bools.append(self.define("bconst %d" % self.rng.randint(0, 1)))
            else:
                self.bools.append(self.define("%s %s, %s" % (self.rng.choice(COMPARE), self.int_value(), self.int())))
        return self.rng.choice(self.bools)

    def of(self, kind):
        return self.bool() if kind == "bool" else self.int()

    def keep(self, kind, value):
        (self.bools if kind == "bool" else self.ints).append(value)

    # -- statements --

    def start(self):
        self.scale = 1
        for k, size in enumerate(self.frames):
            v = self.define("frame %d" % k)
            words = min(size, 64)
            for at in range(0, words, 8):
                self.add("store i64, %s, %d, %s  [frame %d]" % (v, at, self.int(), k))
            self.ptrs.append((v, "frame %d" % k, words))

    def memory(self):
        rng = self.rng
        p, region, size = rng.choice(self.ptrs)
        width, n = rng.choice(WIDTHS)
        how = rng.random()
        if how < 0.5:
            base_value, at = p, rng.randrange(0, size - n + 1)
        elif how < 0.8:
            index = self.define("and %s, %d" % (self.int_value(), size // 8 - 1))
            at = rng.choice([0, 0, 8 - n])
            base_value = self.define("gep %s, %s, 8, 0" % (p, index))
            if rng.random() < 0.3:
                # The address is also a value on its own.
                same = self.define("eq %s, %s" % (base_value, p))
                self.bools.append(same)
        else:
            scale = rng.choice([1, 2, 3, 4, 8, 16])
            index = rng.randint(0, (size - 8) // scale)
            disp = rng.randint(0, size - 8 - index * scale)
            base_value, at = self.define("gep %s, %d, %d, %d" % (p, index, scale, disp)), rng.randint(0, 8 - n)
        if rng.random() < 0.5:
            self.ints.append(self.define("load %s, %s, %d  [%s]" % (width, base_value, at, region)))
        else:
            self.add("store %s, %s, %d, %s  [%s]" % (width, base_value, at, self.int(), region))

    def heap(self):
        rng = self.rng
        p = self.define("callrt alloc(24)")
        for at in (0, 8, 16):
            self.add("store i64, %s, %d, %s  [buf(frame 0)]" % (p, at, self.int()))
        size = 24
        if rng.random() < 0.5:
            p = self.define("callrt realloc(%s, %d)" % (p, rng.choice([40, 200, 70000])))
            self.add("store i64, %s, 24, %s  [buf(frame 0)]" % (p, self.int()))
            self.add("store i64, %s, 32, %s  [buf(frame 0)]" % (p, self.int()))
            size = 40
        if rng.random() < 0.5:
            other, region, _ = rng.choice(self.ptrs)
            if rng.random() < 0.5:
                self.add("callrt memcpy(%s, %s, %d)" % (p, other, rng.randint(0, 24)))
            else:
                self.add("callrt memcpy(%s, %s, %d)" % (other, p, rng.randint(0, 24)))
        if rng.random() < 0.3:
            # Inside one block, both ways round.
            a, b, n = rng.randint(0, 8), rng.randint(0, 8), rng.randint(0, 16)
            src = self.define("gep %s, %d, 1, 0" % (p, a))
            dst = self.define("gep %s, %d, 1, 0" % (p, b))
            self.add("callrt memcpy(%s, %s, %d)" % (dst, src, n))
        for _ in range(rng.randint(1, 3)):
            width, n = rng.choice(WIDTHS)
            self.ints.append(self.define("load %s, %s, %d  [buf(frame 0)]" % (width, p, rng.randrange(0, size - n + 1))))
        self.add("callrt free(%s)" % p)

    def call(self):
        rng = self.rng
        usable = [f for f in self.earlier if self.cost + self.scale * (f.cost + 5) < BUDGET]
        if not usable:
            return
        f = rng.choice(usable[-4:]) if rng.random() < 0.7 else rng.choice(usable)
        arguments = []
        for kind in f.params:
            if kind == "ptr":
                arguments.append(rng.choice(self.ptrs)[0])
            else:
                arguments.append(self.of(kind))
        results = [self.new() for _ in f.rets]
        line = "call %s(%s)" % (f.name, ", ".join(arguments))
        if results:
            line = "%s = %s -> %s" % (", ".join(results), line, ", ".join(f.rets))
        self.add(line)
        self.cost += self.scale * f.cost
        for kind, value in zip(f.rets, results):
            self.keep(kind, value)

    def simple(self):
        rng = self.rng
        pick = rng.random()
        if pick < 0.30:
            self.ints.append(self.define("%s %s, %s" % (rng.choice(BINARY), self.int(), self.int())))
        elif pick < 0.36:
            if rng.random() < 0.6:
                by = self.define("or %s, 1" % self.int())
            else:
                by = str(rng.choice([-1, 1, 2, 3, -7, 10, 1000000007, -9223372036854775808]))
            self.ints.append(self.define("%s %s, %s" % (rng.choice(["sdiv", "srem"]), self.int(), by)))
        elif pick < 0.42:
            self.ints.append(self.define("%s %s" % (rng.choice(UNARY), self.int())))
        elif pick < 0.52:
            self.bools.append(self.define("%s %s, %s" % (rng.choice(COMPARE), self.int(), self.int())))
        elif pick < 0.56:
            self.bools.append(self.define("%s %s, %s" % (rng.choice(["and", "or", "xor"]), self.bool(), self.bool())))
        elif pick < 0.63:
            kind = rng.choice(["i64", "i64", "bool"])
            if rng.random() < 0.5:
                # A comparison used only here: it is done in the select.
                cond = self.define("%s %s, %s" % (rng.choice(COMPARE), self.int(), self.int()))
            else:
                cond = self.bool()
            self.keep(kind, self.define("select %s, %s, %s" % (cond, self.of(kind), self.of(kind))))
        elif pick < 0.80:
            self.memory()
        elif pick < 0.83:
            self.heap()
        elif pick < 0.87:
            if rng.random() < 0.7:
                self.add("callrt print_i64(%s)" % self.int())
            else:
                self.add("callrt print_bool(%s)" % self.bool())
        elif pick < 0.90:
            # Many values at once: all of them are alive together.
            total = self.int()
            for _ in range(rng.randint(3, 9)):
                total = self.define("add %s, %s" % (total, self.int()))
            self.ints.append(total)
        else:
            self.call()

    def saved(self):
        return list(self.ints), list(self.bools)

    def restore(self, saved):
        self.ints, self.bools = list(saved[0]), list(saved[1])

    def merged(self):
        """The kinds of some values that come together at a join."""
        return [self.rng.choice(["i64", "i64", "bool"]) for _ in range(self.rng.randint(0, 3))]

    def edge(self, target, kinds):
        if not kinds:
            return "b%d" % target
        return "b%d(%s)" % (target, ", ".join(self.of(kind) for kind in kinds))

    def join(self, kinds):
        params = [(self.new(), kind) for kind in kinds]
        return self.block(params), params

    def enter(self, block, saved, params=()):
        self.cur = block
        self.restore(saved)
        for value, kind in params:
            self.keep(kind, value)

    def condition(self):
        rng = self.rng
        if rng.random() < 0.3:
            # A value from memory compared right away: the back end
            # compares with the memory itself, unless a store comes
            # between.
            p, region, size = rng.choice(self.ptrs)
            at = rng.randrange(0, size - 7)
            if rng.random() < 0.5:
                index = self.define("and %s, %d" % (self.int_value(), size // 8 - 1))
                p, at = self.define("gep %s, %s, 8, 0" % (p, index)), 0
            loaded = self.define("load %s, %s, %d  [%s]" % (rng.choice(["i64", "u64"]), p, at, region))
            if rng.random() < 0.25:
                self.add("store i64, %s, %d, %s  [%s]" % (p, at, self.int(), region))
            if rng.random() < 0.5:
                return self.define("%s %s, %s" % (rng.choice(COMPARE), loaded, self.int()))
            return self.define("%s %s, %s" % (rng.choice(COMPARE), self.int(), loaded))
        if self.rng.random() < 0.6:
            return self.define("%s %s, %s" % (self.rng.choice(COMPARE), self.int(), self.int()))
        return self.bool()

    def branch(self, depth):
        rng = self.rng
        kinds = self.merged()
        cond = self.condition()
        saved = self.saved()
        then = self.block([])
        after, params = self.join(kinds)
        if rng.random() < 0.5:
            other = self.block([])
            if rng.random() < 0.5:
                self.add("br %s, b%d, b%d" % (cond, then, other))
            else:
                self.add("br %s, b%d, b%d" % (cond, other, then))
            self.enter(other, saved)
            self.statements(depth + 1, rng.randint(0, 4))
            self.add("jump " + self.edge(after, kinds))
        elif rng.random() < 0.5:
            # No `else`: the edge to the join is critical.
            self.add("br %s, b%d, %s" % (cond, then, self.edge(after, kinds)))
        else:
            self.add("br %s, %s, b%d" % (cond, self.edge(after, kinds), then))
        self.enter(then, saved)
        self.statements(depth + 1, rng.randint(0, 4))
        self.add("jump " + self.edge(after, kinds))
        self.enter(after, saved, params)

    def loop(self, depth):
        rng = self.rng
        kinds = ["i64"] + self.merged()
        if rng.random() < 0.5:
            limit = str(rng.randint(0, 5))
            rounds = 5
        else:
            limit = self.define("and %s, 3" % self.int_value())
            rounds = 3
        saved = self.saved()
        outer = self.scale
        if rng.random() < 0.5:
            # The test at the top.
            head, params = self.join(kinds)
            self.add("jump b%d(0%s)" % (head, "".join(", " + self.of(kind) for kind in kinds[1:])))
            self.enter(head, saved, params)
            counter = params[0][0]
            cond = self.define("slt %s, %s" % (counter, limit))
            body = self.block([])
            done = self.block([])
            self.add("br %s, b%d, b%d" % (cond, body, done))
            in_head = self.saved()
            self.enter(body, in_head)
            self.scale = outer * rounds
            self.statements(depth + 1, rng.randint(1, 5))
            step = self.define("add %s, 1" % counter)
            self.add("jump b%d(%s%s)" % (head, step, "".join(", " + self.of(kind) for kind in kinds[1:])))
            self.scale = outer
            self.enter(done, in_head)
        else:
            # The test at the bottom: the edge back is critical, and so
            # is the edge out when it carries values.
            body, params = self.join(kinds)
            self.add("jump b%d(0%s)" % (body, "".join(", " + self.of(kind) for kind in kinds[1:])))
            self.enter(body, saved, params)
            self.scale = outer * (rounds + 1)
            self.statements(depth + 1, rng.randint(1, 5))
            step = self.define("add %s, 1" % params[0][0])
            cond = self.define("slt %s, %s" % (step, limit))
            out_kinds = self.merged()
            done, out_params = self.join(out_kinds)
            back = "b%d(%s%s)" % (body, step, "".join(", " + self.of(kind) for kind in kinds[1:]))
            self.add("br %s, %s, %s" % (cond, back, self.edge(done, out_kinds)))
            self.scale = outer
            self.enter(done, saved, out_params)

    def switch(self, depth):
        rng = self.rng
        kinds = self.merged()
        on = self.define("and %s, 7" % self.int_value()) if rng.random() < 0.7 else self.int_value()
        saved = self.saved()
        after, params = self.join(kinds)
        keys = rng.sample([0, 1, 2, 3, 4, 5, -1, 9223372036854775807, 4294967296], rng.randint(1, 5))
        arms = []
        ways = []
        for key in keys + [None]:
            if rng.random() < 0.3:
                way = self.edge(after, kinds)
            else:
                arm = self.block([])
                arms.append(arm)
                way = "b%d" % arm
            ways.append("default " + way if key is None else "%d -> %s" % (key, way))
        self.add("switch %s, %s" % (on, ", ".join(ways)))
        for arm in arms:
            self.enter(arm, saved)
            self.statements(depth + 1, rng.randint(0, 3))
            self.add("jump " + self.edge(after, kinds))
        self.enter(after, saved, params)

    def trap(self):
        rng = self.rng
        low = self.define("and %s, 63" % self.int_value())
        cond = self.define("eq %s, %d" % (low, rng.randint(0, 63)))
        stop = self.block([])
        on = self.block([])
        self.add("br %s, b%d, b%d" % (cond, stop, on))
        saved = self.saved()
        self.cur = stop
        if rng.random() < 0.2:
            self.add("callrt exit(%s)" % self.int())
            self.add("jump b%d" % on)
        else:
            self.add("trap " + rng.choice(["index", "divide", "pop", "slice", "chr", "memory", "internal"]))
        self.enter(on, saved)

    def statements(self, depth, count):
        rng = self.rng
        for _ in range(count):
            pick = rng.random()
            if depth < 3 and pick < 0.07:
                self.branch(depth)
            elif depth < 3 and pick < 0.12:
                self.loop(depth)
            elif depth < 3 and pick < 0.15:
                self.switch(depth)
            elif pick < 0.165:
                self.trap()
            else:
                self.simple()

    def finish(self):
        self.start()
        self.statements(0, self.rng.randint(3, 30))
        self.add("ret" + (" " if self.rets else "") + ", ".join(self.of(kind) for kind in self.rets))


def random_back_module(rng):
    functions = []
    count = rng.randint(2, 7)
    for index in range(count):
        maker = Maker(rng, index, functions, index == count - 1)
        maker.finish()
        functions.append(maker)
    arguments = [str(rng.choice(BIG) if rng.random() < 0.5 else rng.randint(-50, 50)) for _ in functions[-1].params]
    return "\n".join(f.text() for f in functions), arguments, count


def random_back_tests(ir_tool, back, total, seed, folder, problems):
    rng = random.Random(seed)
    made = 0
    files = 0
    compared = 0
    stopped = 0
    skipped = 0
    checked = 0
    valgrind = shutil.which("valgrind") is not None
    exe = os.path.join(folder, "random.exe")
    while made < total and len(problems) < 10:
        text, arguments, count = random_back_module(rng)
        path = os.path.join(folder, "back_%05d.ir" % files)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        code, out, err = base.tool(ir_tool, "verify", path)
        if code != 0:
            problems.append("%s: the generator made a wrong module: %s" % (path, err.decode(errors="replace").strip()))
            break
        sample = valgrind and files % 40 == 0
        before = len(problems)
        done = compare(ir_tool, back, path, ["main"] + arguments, exe, problems, sample)
        if done == 0:
            skipped += 1
        else:
            compared += done
            made += count
            if sample:
                checked += done
            run_code, _, _ = base.tool(ir_tool, "run", path, "main", *arguments)
            if run_code != 0:
                stopped += 1
        files += 1
        if len(problems) == before:
            os.unlink(path)
    return made, files, compared, stopped, skipped, checked


def file_tests(ir_tool, back, folder, problems):
    exe = os.path.join(folder, "case.exe")
    cases = 0
    programs = 0
    valgrind = shutil.which("valgrind") is not None
    for name in sorted(os.listdir(HERE)):
        if not name.endswith(".ir"):
            continue
        path = os.path.join(HERE, name)
        for case in base.read_cases(path):
            if case["kind"] != "run":
                continue
            done = compare(ir_tool, back, path, case["arg"].split(), exe, problems, valgrind)
            if done:
                cases += 1
                programs += done
    return cases, programs


KERNELS = {
    "fib40.ir": "102334155",
    "loops.ir": "35846572",
    "sieve.ir": "127060719999999",
    "sort.ir": "3003623302",
}


def kernel_tests(back, folder, problems):
    """The hand-written kernels of four benchmarks (tests/opt/kernels):
    built, also with three registers, and run; each prints the number its
    benchmark prints. Gives how many programs ran."""
    ran = 0
    for name, want in sorted(KERNELS.items()):
        for mode in MODES:
            exe = os.path.join(folder, "kernel-" + name[:-3])
            done = subprocess.run([back, "build", os.path.join(HERE, "kernels", name), "-o", exe, "--check", "--program"] + mode, capture_output=True)
            if done.returncode != 0:
                problems.append("kernel %s %s does not build: %s" % (name, " ".join(mode), done.stderr.decode(errors="replace").strip()))
                continue
            got = subprocess.run([exe], capture_output=True, timeout=600)
            ran += 1
            if got.returncode != 0 or got.stdout.decode().strip() != want or got.stderr:
                problems.append("kernel %s %s: exit %d, printed %r, wanted %r" % (name, " ".join(mode), got.returncode, got.stdout[-80:], want))
    return ran


def whole_program_tests(azol, back, folder, problems):
    """Once the lowering stage is in the tree (compiler/opt/lower_tool_main.azl):
    every program of tests/programs and examples goes through `azol-lower
    emit` and `azol-back build --program`, also with three registers, and
    must behave as the executable the default mode builds: the same
    standard output, standard error and exit code. Gives (compared, None)
    or (0, why it was skipped)."""
    front = getattr(base, "FRONT", None)
    lower = getattr(base, "LOWER", None)
    opt = os.path.join(ROOT, "compiler", "opt")
    if front is None or lower is None or not os.path.exists(os.path.join(opt, "lower_tool_main.azl")):
        return 0, "the lowering stage is not in this tree"
    tool = os.path.join(folder, "azol-lower-for-back")
    sources = [os.path.join(ROOT, "compiler", name) for name in front] + [os.path.join(opt, name) for name in lower]
    done = subprocess.run([azol, "build"] + sources + ["-o", tool], capture_output=True)
    if done.returncode != 0:
        problems.append("azol-lower does not build: " + done.stderr.decode(errors="replace").strip()[:300])
        return 0, None
    work = os.path.join(folder, "whole")
    os.makedirs(work, exist_ok=True)
    compared = 0
    for where in ("tests/programs", "examples"):
        for name in sorted(os.listdir(os.path.join(ROOT, where))):
            if not name.endswith(".azl"):
                continue
            path = os.path.join(ROOT, where, name)
            plain = os.path.join(work, "plain-" + name[:-4])
            if subprocess.run([azol, "build", path, "-o", plain], capture_output=True).returncode != 0:
                continue
            want = subprocess.run([plain], capture_output=True, cwd=work, timeout=600)
            emitted = subprocess.run([tool, "emit", path], capture_output=True)
            if emitted.returncode != 0:
                problems.append("%s/%s: azol-lower refuses it: %s" % (where, name, emitted.stderr.decode(errors="replace").strip()[:200]))
                continue
            text = os.path.join(work, name[:-4] + ".ir")
            with open(text, "wb") as handle:
                handle.write(emitted.stdout)
            for mode in MODES:
                exe = os.path.join(work, "opt-" + name[:-4])
                built = subprocess.run([back, "build", text, "-o", exe, "--check", "--program"] + mode, capture_output=True)
                if built.returncode != 0:
                    problems.append("%s/%s %s: the back end refuses it: %s" % (where, name, " ".join(mode), built.stderr.decode(errors="replace").strip()[:200]))
                    break
                got = subprocess.run([exe], capture_output=True, cwd=work, timeout=600)
                if (got.returncode, got.stdout, got.stderr) != (want.returncode, want.stdout, want.stderr):
                    problems.append("%s/%s %s: exit %d (default mode %d), output %r (default mode %r), errors %r (default mode %r)" % (
                        where, name, " ".join(mode), got.returncode, want.returncode, got.stdout[-80:], want.stdout[-80:], got.stderr[-80:], want.stderr[-80:]))
                    break
            else:
                compared += 1
    return compared, None


def run(azol, ir_tool, folder, total, seed, problems):
    """Everything; prints what was done. `ir_tool` is a built ir-tool."""
    if not native_supported():
        print("the back end's tests need x86-64 Linux: skipped")
        return
    back = os.path.join(folder, "azol-back")
    base.build(azol, BACK_SOURCES, back)
    cases, programs = file_tests(ir_tool, back, folder, problems)
    print("back end: %d cases of the files, %d programs built and run" % (cases, programs))
    print("back end: %d runs of the four benchmark kernels" % kernel_tests(back, folder, problems))
    compared, skipped = whole_program_tests(azol, back, folder, problems)
    if skipped:
        print("back end: whole programs skipped (%s)" % skipped)
    else:
        print("back end: %d whole programs lowered, built by the back end and run like their default builds, also with three registers" % compared)
    if total > 0:
        made, files, compared, stopped, skipped, checked = random_back_tests(ir_tool, back, total, seed, folder, problems)
        print("back end: %d random functions in %d modules, %d programs built and run (%d modules end in a trap or an exit, "
              "%d not compared), %d under valgrind" % (made, files, compared, stopped, skipped, checked))


def main():
    argv = sys.argv[1:]
    azol = base.find_azol(argv)
    total = int(argv[argv.index("--random") + 1]) if "--random" in argv else 5000
    seed = int(argv[argv.index("--seed") + 1]) if "--seed" in argv else 20261112
    folder = tempfile.mkdtemp(prefix="azol-back-")
    ir_tool = os.path.join(folder, "ir-tool")
    base.build(azol, base.SOURCES, ir_tool)
    problems = []
    run(azol, ir_tool, folder, total, seed, problems)
    for problem in problems:
        print("FAIL " + problem)
    if problems:
        print("%d FAILED; files are in %s" % (len(problems), folder))
        sys.exit(1)
    if "--keep" not in argv:
        shutil.rmtree(folder)
    print("ALL BACK END TESTS PASSED")


if __name__ == "__main__":
    main()
