#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Tests of the optimiser's intermediate form (compiler/opt).

    python3 tests/opt/run_opt_tests.py [--azol PATH] [--random N] [--seed S] [--keep]

It builds `ir-tool` from compiler/opt with the given azoL compiler
(default: ./azol), and then:

1. Runs every `tests/opt/*.ir` file as its header comments say:

       ; verify: ok                 `ir-tool verify` prints `ok`
       ; verify: MESSAGE            it fails with `error: MESSAGE`
       ; parse: LINE:COL: MESSAGE   it fails with `error: FILE:LINE:COL: MESSAGE`
       ; cfg:                       `ir-tool cfg` prints the `; out:` lines
       ; run: [FUNCTION ARGS...]    `ir-tool run` prints the `; out:` lines,
       ; out: LINE                  writes the `; err:` lines to standard
       ; err: LINE                  error and exits with `; exit:` (0 when
       ; exit: N                    not given). A file can have several runs.

   Every file that parses must also come back from `ir-tool print`
   unchanged in meaning: printing the printed text gives the same text.

2. Runs compiler/opt/ir_builder_test_main.azl, which makes and changes
   functions through the builder, and compares what it prints with
   tests/opt/builder.expected. Then tests the example pass of
   docs/design/optimizer-notes.md against tests/opt/passes.

3. Writes random functions that are valid by construction (default
   5,000), in canonical text, and checks for each file that `print`
   gives the text back byte for byte, that `verify` says `ok`, and that
   `run` prints and returns exactly what this script's own evaluation of
   the same functions gives. Then damages the texts at random and checks
   that the tool never crashes or hangs on them.

4. Builds `azol-lower` (compiler/opt/lower_tool_main.azl: the checker
   and the lowering stage) and, for every program of tests/programs,
   examples, bench and bench/suite (at sizes an interpreter gets
   through), tests/opt/lower and the programs of several files in
   tests/multi, checks
   that `azol-lower run` prints, reports and exits exactly as the
   executable `azol build` makes of the same files; that the text
   `azol-lower emit` prints passes the verifier; and that `ir-tool
   print` gives that text back byte for byte. Programs with
   `// expect-` comments must also do what those say. Then the same
   for the compiler: its lowered form, interpreted, must build
   examples/hello.azl and the compiler itself into the very bytes the
   given `azol` writes (so give it a compiler built from this tree,
   as `sh build.sh` leaves in ./azol). `--no-lower`
   leaves this part out; `--lower-only` runs nothing else.

Only the Python standard library is used.
"""

import concurrent.futures
import os
import random
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
SOURCES = ["ir.azl", "ir_text.azl", "ir_cfg.azl", "ir_verify.azl", "ir_interp.azl", "ir_tool_main.azl"]
MASK = (1 << 64) - 1


def wrap(n):
    n &= MASK
    return n - (1 << 64) if n >> 63 else n


def find_azol(argv):
    if "--azol" in argv:
        return argv[argv.index("--azol") + 1]
    for candidate in ("azol",):
        path = os.path.join(ROOT, candidate)
        if os.path.exists(path):
            return path
    sys.exit("no azoL compiler found; pass --azol PATH")


def build(azol, sources, out):
    paths = [os.path.join(ROOT, "compiler", "opt", name) if os.sep not in name else name for name in sources]
    done = subprocess.run([azol, "build", *paths, "-o", out], capture_output=True, text=True)
    if done.returncode != 0:
        sys.exit("cannot build %s:\n%s" % (out, done.stderr))


def tool(binary, *arguments, timeout=60):
    try:
        done = subprocess.run([binary, *arguments], capture_output=True, timeout=timeout)
    except subprocess.TimeoutExpired:
        return None, b"", b""
    return done.returncode, done.stdout, done.stderr


# ---- the hand-written files ---------------------------------------------------


def read_cases(path):
    """The cases a file's header describes: (kind, argument, out, err, exit)."""
    cases = []
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if not line.startswith(";"):
                if line.strip():
                    break
                continue
            body = line[1:].strip("\n")
            body = body[1:] if body.startswith(" ") else body
            word, _, rest = body.partition(":")
            rest = rest[1:] if rest.startswith(" ") else rest
            if word in ("verify", "parse", "cfg", "run"):
                cases.append({"kind": word, "arg": rest, "out": [], "err": [], "exit": None})
            elif word == "out" and cases:
                cases[-1]["out"].append(rest)
            elif word == "err" and cases:
                cases[-1]["err"].append(rest)
            elif word == "exit" and cases:
                cases[-1]["exit"] = int(rest)
    return cases


def lines(data):
    text = data.decode("utf-8", errors="replace")
    return text.split("\n")[:-1] if text.endswith("\n") else (text.split("\n") if text else [])


def check_file(binary, path, problems):
    name = os.path.basename(path)
    cases = read_cases(path)
    if not cases:
        problems.append("%s: no test in its header" % name)
        return 0
    count = 0
    parses = True
    for case in cases:
        count += 1
        kind = case["kind"]
        if kind == "parse":
            parses = False
            code, out, err = tool(binary, "verify", path)
            want = ["error: %s:%s" % (path, case["arg"])]
            if code != 1 or lines(err) != want or out:
                problems.append("%s: parse: wanted %r, got exit %r and %r" % (name, want, code, lines(err)))
        elif kind == "verify":
            code, out, err = tool(binary, "verify", path)
            if case["arg"] == "ok":
                if code != 0 or lines(out) != ["ok"] or err:
                    problems.append("%s: verify: wanted ok, got exit %r and %r" % (name, code, lines(err)))
            else:
                want = ["error: " + case["arg"]]
                if code != 1 or lines(err) != want or out:
                    problems.append("%s: verify: wanted %r, got exit %r and %r" % (name, want, code, lines(err)))
        elif kind == "cfg":
            code, out, err = tool(binary, "cfg", path)
            if code != 0 or lines(out) != case["out"] or err:
                problems.append("%s: cfg: wanted\n    %s\n  got exit %r\n    %s\n    %s" % (
                    name, "\n    ".join(case["out"]), code, "\n    ".join(lines(out)), "\n    ".join(lines(err))))
        elif kind == "run":
            code, out, err = tool(binary, "run", path, *case["arg"].split())
            want_exit = case["exit"] or 0
            if code != want_exit or lines(out) != case["out"] or lines(err) != case["err"]:
                problems.append("%s: run %s: wanted exit %d, out %r, err %r; got exit %r, out %r, err %r" % (
                    name, case["arg"], want_exit, case["out"], case["err"], code, lines(out), lines(err)))
    if parses:
        count += 1
        code, first, err = tool(binary, "print", path)
        if code != 0:
            problems.append("%s: print fails: %r" % (name, lines(err)))
        else:
            with tempfile.NamedTemporaryFile("wb", suffix=".ir", delete=False) as handle:
                handle.write(first)
            code, second, err = tool(binary, "print", handle.name)
            os.unlink(handle.name)
            if code != 0 or first != second:
                problems.append("%s: printing the printed text gives something else" % name)
    return count


# ---- random functions -----------------------------------------------------------
#
# A function is built as a list of blocks, each a list of instruction
# tuples, and printed in canonical text. The same tuples are evaluated
# here, which gives the expected output without reading the text back.

BINARY = ["add", "sub", "mul", "and", "or", "xor", "shl", "sar", "shr", "umulh", "smulh"]
COMPARE = ["eq", "ne", "slt", "sle", "sgt", "sge", "ult", "ule", "ugt", "uge"]
UNARY = ["neg", "ext8u", "ext8s", "ext32u", "ext32s", "trunc8", "trunc32"]
EDGE = [0, 1, -1, 2, 7, 63, 64, 65, 255, 256, -128, (1 << 31) - 1, 1 << 31, (1 << 32) - 1, 1 << 32,
        (1 << 63) - 1, -(1 << 63), -(1 << 63) + 1, 1234567891011, -987654321]


def evaluate(op, a, b=0):
    ua, ub = a & MASK, b & MASK
    if op == "add":
        return wrap(a + b)
    if op == "sub":
        return wrap(a - b)
    if op == "mul":
        return wrap(a * b)
    if op == "and":
        return a & b
    if op == "or":
        return a | b
    if op == "xor":
        return a ^ b
    if op == "shl":
        return wrap(a << (b & 63))
    if op == "sar":
        return a >> (b & 63)
    if op == "shr":
        return wrap(ua >> (b & 63))
    if op == "umulh":
        return wrap((ua * ub) >> 64)
    if op == "smulh":
        return wrap((a * b) >> 64)
    if op == "sdiv":
        q = abs(a) // abs(b)
        return wrap(q if (a < 0) == (b < 0) else -q)
    if op == "srem":
        r = abs(a) % abs(b)
        return wrap(-r if a < 0 else r)
    if op == "eq":
        return int(a == b)
    if op == "ne":
        return int(a != b)
    if op == "slt":
        return int(a < b)
    if op == "sle":
        return int(a <= b)
    if op == "sgt":
        return int(a > b)
    if op == "sge":
        return int(a >= b)
    if op == "ult":
        return int(ua < ub)
    if op == "ule":
        return int(ua <= ub)
    if op == "ugt":
        return int(ua > ub)
    if op == "uge":
        return int(ua >= ub)
    if op == "neg":
        return wrap(-a)
    if op in ("ext8u", "trunc8"):
        return a & 255
    if op == "ext8s":
        return ((a & 255) ^ 128) - 128
    if op in ("ext32u", "trunc32"):
        return a & 0xFFFFFFFF
    if op == "ext32s":
        return ((a & 0xFFFFFFFF) ^ (1 << 31)) - (1 << 31)
    raise ValueError(op)


class Stop(Exception):
    """The generated program stops with a division by zero."""


class Builder:
    """Writes one function and evaluates it while writing.

    Every construct is generated together with the path a concrete run
    takes through it, for the concrete arguments the caller will pass,
    so the expected output is known without a second interpreter of the
    text. Values are (name, type, number); the number is the value on
    the path that is being followed, and None on a path that is not.
    """

    def __init__(self, rng, name, params, earlier):
        self.rng = rng
        self.name = name
        self.params = params
        self.earlier = earlier
        self.blocks = []
        self.values = 0
        self.frames = []
        self.output = []
        self.stopped = False
        self.calls = 0

    def new_block(self, params=()):
        self.blocks.append((len(self.blocks), list(params), []))
        return len(self.blocks) - 1

    def emit(self, block, text):
        self.blocks[block][2].append(text)

    def value(self):
        self.values += 1
        return "v%d" % (self.values - 1)

    def constant(self):
        rng = self.rng
        return rng.choice(EDGE) if rng.random() < 0.6 else rng.randint(-1000, 1000)

    def pick(self, scope, ty, live):
        """An operand of the type: a value in scope, or a constant."""
        rng = self.rng
        found = [v for v in scope if v[1] == ty]
        if found and rng.random() < 0.75:
            v = rng.choice(found)
            return v[0], v[2]
        if ty == "i64":
            n = self.constant()
            return str(n), (n if live else None)
        n = rng.randint(0, 1)
        return str(n), (n if live else None)

    def straight(self, block, scope, live, count):
        """`count` instructions with no control flow, added to `block`."""
        rng = self.rng
        for _ in range(count):
            kind = rng.random()
            name = self.value()
            if kind < 0.45:
                op = rng.choice(BINARY)
                (a, an), (b, bn) = self.pick(scope, "i64", live), self.pick(scope, "i64", live)
                self.emit(block, "    %s = %s %s, %s" % (name, op, a, b))
                scope.append((name, "i64", evaluate(op, an, bn) if live else None))
            elif kind < 0.6:
                op = rng.choice(COMPARE)
                (a, an), (b, bn) = self.pick(scope, "i64", live), self.pick(scope, "i64", live)
                self.emit(block, "    %s = %s %s, %s" % (name, op, a, b))
                scope.append((name, "bool", evaluate(op, an, bn) if live else None))
            elif kind < 0.7:
                op = rng.choice(UNARY)
                a, an = self.pick(scope, "i64", live)
                self.emit(block, "    %s = %s %s" % (name, op, a))
                scope.append((name, "i64", evaluate(op, an) if live else None))
            elif kind < 0.78:
                c, cn = self.pick(scope, "bool", live)
                (a, an), (b, bn) = self.pick(scope, "i64", live), self.pick(scope, "i64", live)
                self.emit(block, "    %s = select %s, %s, %s" % (name, c, a, b))
                scope.append((name, "i64", (an if cn else bn) if live else None))
            elif kind < 0.84:
                op = rng.choice(["and", "or", "xor"])
                bools = [v for v in scope if v[1] == "bool"]
                if not bools:
                    self.emit(block, "    %s = bconst %d" % (name, rng.randint(0, 1)))
                    scope.append((name, "bool", int(self.blocks[block][2][-1][-1]) if live else None))
                    continue
                x = rng.choice(bools)
                b, bn = self.pick(scope, "bool", live)
                self.emit(block, "    %s = %s %s, %s" % (name, op, x[0], b))
                scope.append((name, "bool", evaluate(op, x[2], bn) if live else None))
            elif kind < 0.9:
                # A frame object, stored and loaded again in a width.
                width, bytes_ = rng.choice([("i64", 8), ("u8", 1), ("i8", 1), ("u32", 4), ("i32", 4), ("u64", 8)])
                slot = len(self.frames)
                self.frames.append(16)
                offset = rng.choice([0, 8 - bytes_, 8, 16 - bytes_])
                a, an = self.pick(scope, "i64", live)
                self.emit(block, "    %s = frame %d" % (name, slot))
                self.emit(block, "    store %s, %s, %d, %s  [frame %d]" % (width, name, offset, a, slot))
                loaded = self.value()
                self.emit(block, "    %s = load %s, %s, %d  [frame %d]" % (loaded, width, name, offset, slot))
                number = None
                if live:
                    number = an & ((1 << (8 * bytes_)) - 1)
                    if width in ("i8", "i32") and number >> (8 * bytes_ - 1):
                        number -= 1 << (8 * bytes_)
                    number = wrap(number)
                scope.append((name, "ptr", 0))
                scope.append((loaded, "i64", number))
            elif kind < 0.95:
                n = self.constant()
                self.emit(block, "    %s = iconst %d" % (name, n))
                scope.append((name, "i64", n if live else None))
            elif self.earlier and self.calls < 3:
                self.calls += 1
                callee = rng.choice(self.earlier)
                operands = [self.pick(scope, "i64", live) for _ in range(callee.params)]
                self.emit(block, "    %s = call %s(%s) -> i64" % (name, callee.name, ", ".join(o[0] for o in operands)))
                number = None
                if live:
                    out, number = callee.call([o[1] for o in operands])
                    self.output.extend(out)
                scope.append((name, "i64", number))
            else:
                a, an = self.pick(scope, "i64", live)
                self.values -= 1
                self.emit(block, "    callrt print_i64(%s)" % a)
                if live:
                    self.output.append(str(an))

    def region(self, block, scope, live, depth):
        """A piece of code with one way in and one way out; gives the
        block it ends in (not yet terminated) and the scope there."""
        rng = self.rng
        self.straight(block, scope, live, rng.randint(1, 5))
        if depth <= 0:
            return block, scope
        kind = rng.random()
        if kind < 0.4:
            # if / else, joined through block parameters
            c, cn = self.pick(scope, "bool", live)
            then, other = self.new_block(), self.new_block()
            self.emit(block, "    br %s, b%d, b%d" % (c, then, other))
            then_end, then_scope = self.region(then, list(scope), live and bool(cn), depth - 1)
            other_end, other_scope = self.region(other, list(scope), live and not cn, depth - 1)
            joined = []
            arguments = ([], [])
            for _ in range(rng.randint(0, 3)):
                ty = rng.choice(["i64", "i64", "bool"])
                (a, an), (b, bn) = self.pick(then_scope, ty, live and bool(cn)), self.pick(other_scope, ty, live and not cn)
                arguments[0].append(a)
                arguments[1].append(b)
                number = None
                if live:
                    number = an if cn else bn
                joined.append((self.value(), ty, number))
            join = self.new_block(["%s: %s" % (v[0], v[1]) for v in joined])
            for end, passed in ((then_end, arguments[0]), (other_end, arguments[1])):
                self.emit(end, "    jump b%d%s" % (join, "(%s)" % ", ".join(passed) if passed else ""))
            return self.region(join, scope + joined, live, depth - 1)
        if kind < 0.7:
            # a counted loop: i from 0 to a small constant, one sum
            trips = rng.randint(0, 4)
            start, sn = self.pick(scope, "i64", live)
            i, total = self.value(), self.value()
            header = self.new_block(["%s: i64" % i, "%s: i64" % total])
            body, after = self.new_block(), self.new_block()
            self.emit(block, "    jump b%d(0, %s)" % (header, start))
            test = self.value()
            self.emit(header, "    %s = slt %s, %d" % (test, i, trips))
            self.emit(header, "    br %s, b%d, b%d" % (test, body, after))
            # The body is written once; its numbers are those of the last
            # trip that happens, and the sum is carried through all trips.
            step = rng.choice(BINARY[:6])
            operand, on = self.pick(scope, "i64", live)
            added = self.value()
            bumped = self.value()
            self.emit(body, "    %s = %s %s, %s" % (added, step, total, operand))
            self.emit(body, "    %s = add %s, 1" % (bumped, i))
            self.emit(body, "    jump b%d(%s, %s)" % (header, bumped, added))
            number = None
            if live:
                number = sn
                for _ in range(trips):
                    number = evaluate(step, number, on)
            return self.region(after, scope + [(i, "i64", trips if live else None), (total, "i64", number)], live, depth - 1)
        if kind < 0.78:
            # a checked division
            op = rng.choice(["sdiv", "srem"])
            (a, an), (d, dn) = self.pick(scope, "i64", live), self.pick(scope, "i64", live)
            test, result = self.value(), self.value()
            good, bad = self.new_block(), self.new_block()
            self.emit(block, "    %s = ne %s, 0" % (test, d))
            self.emit(block, "    br %s, b%d, b%d" % (test, good, bad))
            self.emit(bad, "    trap divide")
            self.emit(good, "    %s = %s %s, %s" % (result, op, a, d))
            number = None
            if live:
                if dn == 0:
                    raise Stop()
                number = wrap(-an) if dn == -1 and op == "sdiv" else (0 if dn == -1 else evaluate(op, an, dn))
            return self.region(good, scope + [(result, "i64", number)], live, depth - 1)
        # a switch whose arms all come together again
        v, vn = self.pick(scope, "i64", live)
        keys = rng.sample([-1, 0, 1, 2, 5, 64, 255], rng.randint(1, 3))
        arms = [self.new_block() for _ in keys]
        default = self.new_block()
        merged = self.value()
        join = self.new_block(["%s: i64" % merged])
        self.emit(block, "    switch %s, %s, default b%d" % (v, ", ".join("%d -> b%d" % (k, b) for k, b in zip(keys, arms)), default))
        number = None
        for key, arm in list(zip(keys, arms)) + [(None, default)]:
            taken = live and ((vn == key) if key is not None else (vn not in keys))
            end, arm_scope = self.region(arm, list(scope), taken, 0)
            a, an = self.pick(arm_scope, "i64", taken)
            self.emit(end, "    jump b%d(%s)" % (join, a))
            if taken:
                number = an
        return self.region(join, scope + [(merged, "i64", number)], live, depth - 1)


class Function:
    def __init__(self, rng, name, params, earlier, depth):
        self.rng = rng
        self.name = name
        self.params = params
        self.earlier = earlier
        self.depth = depth
        self.seed = rng.getrandbits(48)
        self.text = self.build(None)[0]

    def build(self, arguments):
        """Writes the function; with arguments, also what a call with
        them prints and returns. The same seed gives the same text."""
        builder = Builder(random.Random(self.seed), self.name, self.params, self.earlier)
        live = arguments is not None
        scope = [("p%d" % k, "i64", arguments[k] if live else None) for k in range(self.params)]
        entry = builder.new_block()
        result = None
        try:
            end, scope = builder.region(entry, scope, live, self.depth)
            a, result = builder.pick(scope, "i64", live)
            builder.emit(end, "    ret %s" % a)
        except Stop:
            builder.stopped = True
        head = "fn %s(%s) -> i64 {" % (self.name, ", ".join("p%d: i64" % k for k in range(self.params)))
        out = [head]
        if builder.frames:
            out.append("    frames " + ", ".join(str(n) for n in builder.frames))
        for number, params, body in builder.blocks:
            out.append("b%d%s:" % (number, "(%s)" % ", ".join(params) if params else ""))
            out.extend(body)
        out.append("}")
        return "\n".join(out) + "\n", builder.output, result, builder.stopped

    def call(self, arguments):
        _, output, result, stopped = self.build(arguments)
        if stopped:
            raise Stop()
        return output, result


def random_module(rng, functions):
    """Text of a module and what running its `main` gives."""
    made = []
    for k in range(functions):
        made.append(Function(rng, "f%d" % k, rng.randint(0, 3), made[-4:], rng.randint(0, 3)))
    lines_ = ["fn main() -> i64 {", "b0:"]
    expected, code, err = [], 0, []
    count = 0
    alive = True
    for function in made:
        arguments = [rng.choice(EDGE) if rng.random() < 0.5 else rng.randint(-50, 50) for _ in range(function.params)]
        lines_.append("    v%d = call %s(%s) -> i64" % (count, function.name, ", ".join(str(a) for a in arguments)))
        lines_.append("    callrt print_i64(v%d)" % count)
        count += 1
        if alive:
            try:
                output, result = function.call(arguments)
                expected.extend(output)
                expected.append(str(result))
            except Stop:
                # What the stopped function printed before stopping is
                # not known here; the comparison then ends at the stop.
                alive = False
                code, err = 1, ["azol: division by zero"]
    lines_.append("    ret %d" % len(made))
    lines_.append("}")
    if alive:
        expected.append("=> %d" % len(made))
    text = "\n".join(f.text for f in made) + "\n" + "\n".join(lines_) + "\n"
    return text, expected, err, code, alive


def damage(rng, text):
    kind = rng.random()
    if kind < 0.3:
        at = rng.randrange(len(text))
        return text[:at] + text[at + 1:]
    if kind < 0.5:
        at = rng.randrange(len(text))
        return text[:at] + rng.choice("vbp0123456789,()[]:=-> \n{}@+;x") + text[at:]
    rows = text.split("\n")
    at = rng.randrange(len(rows))
    if kind < 0.7:
        del rows[at]
    elif kind < 0.85:
        other = rng.randrange(len(rows))
        rows[at], rows[other] = rows[other], rows[at]
    else:
        words = rows[at].split(" ")
        if words:
            words[rng.randrange(len(words))] = rng.choice(["v0", "p9", "b99", "ptr", "i64", "u8", "trap", "ret", "-1", "bool", "frame", "9999999999999999999999"])
        rows[at] = " ".join(words)
    return "\n".join(rows)


def random_tests(binary, total, seed, folder, problems):
    rng = random.Random(seed)
    made = 0
    stops = 0
    files = 0
    texts = []
    while made < total:
        functions = rng.randint(5, 30)
        text, expected, err, code, alive = random_module(rng, functions)
        made += functions
        files += 1
        path = os.path.join(folder, "random%d.ir" % files)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        texts.append(text)
        got, out, error = tool(binary, "print", path)
        if got != 0 or out.decode() != text:
            problems.append("random file %d (seed %d): print does not give the text back (exit %r, %r); kept as %s" % (
                files, seed, got, lines(error)[:1], path))
            continue
        got, out, error = tool(binary, "verify", path)
        if got != 0 or lines(out) != ["ok"]:
            problems.append("random file %d (seed %d): verify: %r; kept as %s" % (files, seed, lines(error), path))
            continue
        got, out, error = tool(binary, "run", path)
        if alive:
            right = got == 0 and lines(out) == expected and not error
        else:
            stops += 1
            right = got == code and lines(error) == err and lines(out)[:len(expected)] == expected
        if not right:
            problems.append("random file %d (seed %d): run gives exit %r, %r, %r; wanted %r, %r; kept as %s" % (
                files, seed, got, lines(out)[-3:], lines(error), expected[-3:], err, path))
            continue
        os.unlink(path)
    damaged = 0
    path = os.path.join(folder, "damaged.ir")
    for text in texts:
        for _ in range(max(1, 2000 // len(texts))):
            broken = damage(rng, text)
            with open(path, "w", encoding="utf-8") as handle:
                handle.write(broken)
            damaged += 1
            for command in (("verify", path), ("run", path, "--steps", "200000"), ("cfg", path)):
                got, out, error = tool(binary, *command, timeout=30)
                if got is None or got < 0 or got not in (0, 1, 3):
                    keep = os.path.join(folder, "crash%d.ir" % damaged)
                    with open(keep, "w", encoding="utf-8") as handle:
                        handle.write(broken)
                    problems.append("damaged text: `%s` ends with %r; kept as %s" % (command[0], got, keep))
                    break
    if os.path.exists(path):
        os.unlink(path)
    return made, files, stops, damaged


# ---- the example pass ---------------------------------------------------------------


def pass_tests(azol, binary, folder, problems):
    """tests/opt/passes/NAME.ir, NAME.expected.ir and NAME.runs: the
    example pass must turn the first into the second, and both must run
    the same on every line of arguments in the third."""
    source = os.path.join(ROOT, "compiler", "opt", "example_pass_main.azl")
    cases = os.path.join(HERE, "passes")
    if not os.path.exists(source) or not os.path.isdir(cases):
        return 0
    # The page that teaches the pass must show the pass that is tested.
    with open(os.path.join(ROOT, "compiler", "opt", "example_pass.azl"), encoding="utf-8") as handle:
        function = handle.read()
    function = function[function.index("fn ir_example_fold_add"):]
    with open(os.path.join(ROOT, "docs", "design", "optimizer-notes.md"), encoding="utf-8") as handle:
        if function not in handle.read():
            problems.append("docs/design/optimizer-notes.md does not show compiler/opt/example_pass.azl as it is")
    example = os.path.join(folder, "example-pass")
    build(azol, SOURCES[:5] + ["example_pass.azl", "example_pass_main.azl"], example)
    count = 0
    for name in sorted(os.listdir(cases)):
        if not name.endswith(".ir") or name.endswith(".expected.ir"):
            continue
        count += 1
        path = os.path.join(cases, name)
        want_path = path[:-3] + ".expected.ir"
        code, out, err = tool(example, path)
        with open(want_path, "rb") as handle:
            want = handle.read()
        if code != 0 or out != want:
            problems.append("pass %s: the result is not the expected text (exit %r, %r)\n%s" % (name, code, lines(err), out.decode(errors="replace")))
            continue
        runs_path = path[:-3] + ".runs"
        runs = [""]
        if os.path.exists(runs_path):
            with open(runs_path, encoding="utf-8") as handle:
                runs = [row.strip() for row in handle if row.strip()]
        for row in runs:
            before = tool(binary, "run", path, *row.split())
            after = tool(binary, "run", want_path, *row.split())
            if before != after or before[0] not in (0, 1):
                problems.append("pass %s: `run %s` differs before and after: %r, %r" % (name, row, before, after))
    return count


def builder_test(azol, folder, problems):
    """compiler/opt/ir_builder_test_main.azl builds and changes functions
    through the builder and prints each step; the output must be
    tests/opt/builder.expected."""
    binary = os.path.join(folder, "builder-test")
    build(azol, SOURCES[:5] + ["ir_builder_test_main.azl"], binary)
    code, out, err = tool(binary)
    with open(os.path.join(HERE, "builder.expected"), "rb") as handle:
        want = handle.read()
    if code != 0 or out != want or err:
        got = os.path.join(folder, "builder.got")
        with open(got, "wb") as handle:
            handle.write(out)
        problems.append("the builder test prints something else than tests/opt/builder.expected (exit %r); see %s" % (code, got))


# ---- lowering whole programs --------------------------------------------------

FRONT = ["driver.azl", "token.azl", "lexer.azl", "ast.azl", "parser.azl", "sema.azl", "sema_fn.azl", "embedded.azl"]
LOWER = ["ir.azl", "ir_text.azl", "ir_cfg.azl", "ir_verify.azl", "ir_interp.azl", "ir_lower.azl", "ir_lower_stmt.azl", "ir_lower_expr.azl", "lower_tool_main.azl"]

# The benchmarks without a `SMALL` switch, and the edits that make them
# small enough to interpret: (old text, new text), each found once.
SHRINK = {
    "append.azl": [("i < 3000000", "i < 30000"), ("n < 300000", "n < 3000")],
    "fib40.azl": [("fib(40)", "fib(24)")],
    "loops.azl": [("let size = 5000", "let size = 300")],
    "particles.azl": [("round < 50000", "round < 40")],
    "sieve.azl": [("let limit = 20000000", "let limit = 200000")],
    "sort.azl": [("let count = 2000000", "let count = 20000")],
    "tree.azl": [("build(18, round)", "build(9, round)")],
    "words.azl": [("n < 400000", "n < 6000")],
}


def lower_programs(folder, problems):
    """(label, files, expectation or None) for every program to compare."""
    programs = []

    def expected(path):
        with open(path, encoding="utf-8") as handle:
            source = handle.read()

        def values(key):
            prefix = "// %s:" % key
            found = [line[len(prefix):] for line in source.split("\n") if line.startswith(prefix)]
            return [text[1:] if text.startswith(" ") else text for text in found]

        if not values("expect-exit"):
            return None
        rows = values("expect-line")
        output = "".join(row + "\n" for row in rows) if rows else "".join(part + "\n" for part in " ".join(values("expect-output")).split())
        errors = values("expect-stderr")
        return int(values("expect-exit")[0]), output.encode(), (errors[0] + "\n").encode() if errors else None

    for part in ("tests/programs", "tests/opt/lower", "examples"):
        for name in sorted(os.listdir(os.path.join(ROOT, part))):
            if name.endswith(".azl"):
                path = os.path.join(ROOT, part, name)
                programs.append((part + "/" + name, [path], expected(path)))
    for part, switch in (("bench", False), ("bench/suite", True)):
        for name in sorted(os.listdir(os.path.join(ROOT, part))):
            if not name.endswith(".azl"):
                continue
            with open(os.path.join(ROOT, part, name), encoding="utf-8") as handle:
                source = handle.read()
            edits = [("const SMALL = false", "const SMALL = true")] if switch else SHRINK.get(name, [])
            for old, new in edits:
                if source.count(old) != 1:
                    problems.append("lowering: %s/%s no longer holds `%s` once; correct the test's list of sizes" % (part, name, old))
                source = source.replace(old, new)
            small = os.path.join(folder, part.replace("/", "-") + "-" + name)
            with open(small, "w", encoding="utf-8") as handle:
                handle.write(source)
            programs.append((part + "/" + name, [small], None))
    multi = os.path.join(ROOT, "tests", "multi")
    for label, order in (("shop", ["shop/main.azl", "shop/stock.azl", "shop/numbers.azl"]), ("shop, another order", ["shop/numbers.azl", "shop/stock.azl", "shop/main.azl"]),
                         ("embed", ["embed/main.azl", "embed/parts/words.azl"])):
        files = [os.path.join(multi, name) for name in order]
        main = [path for path in files if path.endswith("main.azl")][0]
        programs.append(("tests/multi: " + label, files, expected(main)))
    return programs


def lower_one(azol, lower, binary, folder, at, label, files, want):
    """Compares one program as an executable and as interpreted IR.
    Returns (what was compared or None, problems)."""
    exe = os.path.join(folder, "exe%d" % at)
    done = subprocess.run([azol, "build", *files, "-o", exe], capture_output=True)
    if done.returncode != 0:
        # Not a whole program (no `main`): the tool must refuse it too.
        code, _, _ = tool(lower, "emit", *files)
        return None, [] if code == 1 else ["lowering %s: `azol build` refuses it but `azol-lower emit` exits with %r" % (label, code)]
    found = []
    work = os.path.join(folder, "run%d" % at)
    results = []
    for kind, command in (("exe", [exe]), ("ir", ["sh", "-c", 'ulimit -s unlimited 2>/dev/null; exec "$@"', "sh", lower, "run", "--argv0", exe, *files])):
        # Each run starts in a folder of its own: programs write files.
        if os.path.isdir(work):
            shutil.rmtree(work)
        os.mkdir(work)
        try:
            ran = subprocess.run(command, capture_output=True, timeout=900, cwd=work)
            results.append((ran.returncode, ran.stdout, ran.stderr))
        except subprocess.TimeoutExpired:
            results.append((None, b"", b"(no answer in time)"))
    shutil.rmtree(work)
    real, lowered = results
    if real != lowered:
        found.append("lowering %s: the executable gives exit %r, %r, %r; the interpreted IR gives exit %r, %r, %r" %
                     (label, real[0], real[1][-200:], real[2][-200:], lowered[0], lowered[1][-200:], lowered[2][-300:]))
    if want is not None and (lowered[0] != want[0] or lowered[1] != want[1] or (want[2] is not None and lowered[2] != want[2])):
        found.append("lowering %s: the interpreted IR does not do what the `// expect-` comments say (exit %r, %r)" % (label, lowered[0], lowered[2][-300:]))
    code, text, err = tool(lower, "emit", *files)
    if code != 0:
        found.append("lowering %s: `emit` exits with %r: %r" % (label, code, err[-300:]))
        return "run", found
    emitted = os.path.join(folder, "emit%d.ir" % at)
    with open(emitted, "wb") as handle:
        handle.write(text)
    code, out, err = tool(binary, "verify", emitted)
    if code != 0 or out != b"ok\n":
        found.append("lowering %s: the emitted text does not verify: %r" % (label, err[-300:]))
    code, out, err = tool(binary, "print", emitted)
    if code != 0 or out != text:
        found.append("lowering %s: `ir-tool print` does not give the emitted text back (exit %r, %r)" % (label, code, err[-300:]))
    os.unlink(emitted)
    os.unlink(exe)
    return "run", found


def lower_tests(azol, binary, problems):
    folder = tempfile.mkdtemp(prefix="azol-lower-")
    lower = os.path.join(folder, "azol-lower")
    build(azol, [os.path.join(ROOT, "compiler", name) for name in FRONT] + LOWER, lower)
    programs = lower_programs(folder, problems)
    compared = 0
    refused = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=os.cpu_count() or 2) as pool:
        jobs = [pool.submit(lower_one, azol, lower, binary, folder, at, label, files, want) for at, (label, files, want) in enumerate(programs)]
        for job in jobs:
            what, found = job.result()
            problems.extend(found)
            if what is None:
                refused += 1
            else:
                compared += 1
    lower_compiler(azol, lower, binary, folder, problems)
    if not problems:
        shutil.rmtree(folder)
    return compared, refused


def lower_compiler(azol, lower, binary, folder, problems):
    """The compiler itself: lowered, verified, read back, and then run in
    the interpreter to build a small program and to build itself. What
    it writes must be, byte for byte, what `azol` writes."""
    import platform
    host = {("Linux", "x86_64"): "host_x86_64_linux.azl", ("Linux", "aarch64"): "host_aarch64_linux.azl", ("Darwin", "arm64"): "host_aarch64_macos.azl"}.get((platform.system(), platform.machine()))
    if host is None:
        return
    with open(os.path.join(ROOT, "compiler", "SOURCES"), encoding="utf-8") as handle:
        sources = [os.path.join(ROOT, "compiler", line.strip()) for line in handle if line.strip() and not line.startswith("#")]
    sources.append(os.path.join(ROOT, "compiler", host))
    code, text, err = tool(lower, "emit", *sources)
    if code != 0:
        problems.append("lowering the compiler: `emit` exits with %r: %r" % (code, err[-300:]))
        return
    emitted = os.path.join(folder, "compiler.ir")
    with open(emitted, "wb") as handle:
        handle.write(text)
    code, out, err = tool(binary, "verify", emitted)
    if code != 0:
        problems.append("lowering the compiler: the emitted text does not verify: %r" % err[-300:])
    code, out, err = tool(binary, "print", emitted)
    if code != 0 or out != text:
        problems.append("lowering the compiler: `ir-tool print` does not give the emitted text back")
    hello = os.path.join(ROOT, "examples", "hello.azl")
    for label, files in (("examples/hello.azl", [hello]), ("itself", sources)):
        real = os.path.join(folder, "built-real")
        interpreted = os.path.join(folder, "built-interpreted")
        done = subprocess.run([azol, "build", *files, "-o", real], capture_output=True)
        try:
            ran = subprocess.run(["sh", "-c", 'ulimit -s unlimited 2>/dev/null; exec "$@"', "sh", lower, "run", *sources, "--", "build", *files, "-o", interpreted], capture_output=True, timeout=900)
        except subprocess.TimeoutExpired:
            problems.append("the lowered compiler, interpreted, does not finish building %s" % label)
            continue
        if done.returncode != 0 or ran.returncode != 0:
            problems.append("the lowered compiler, interpreted, building %s: exit %r (azol: %r), %r" % (label, ran.returncode, done.returncode, ran.stderr[-300:]))
            continue
        with open(real, "rb") as first, open(interpreted, "rb") as second:
            if first.read() != second.read():
                problems.append("the lowered compiler, interpreted, builds %s differently from %s" % (label, azol))


def main():
    argv = sys.argv[1:]
    azol = find_azol(argv)
    total = int(argv[argv.index("--random") + 1]) if "--random" in argv else 5000
    seed = int(argv[argv.index("--seed") + 1]) if "--seed" in argv else 20261112
    folder = tempfile.mkdtemp(prefix="azol-opt-")
    binary = os.path.join(folder, "ir-tool")
    build(azol, SOURCES, binary)
    problems = []
    checks = 0
    if "--lower-only" not in argv:
        files = sorted(name for name in os.listdir(HERE) if name.endswith(".ir"))
        for name in files:
            checks += check_file(binary, os.path.join(HERE, name), problems)
        print("%d files, %d checks" % (len(files), checks))
        builder_test(azol, folder, problems)
        print("the builder test")
        passes = pass_tests(azol, binary, folder, problems)
        if passes:
            print("%d cases of the example pass" % passes)
        if total > 0:
            made, count, stops, damaged = random_tests(binary, total, seed, folder, problems)
            print("%d random functions in %d files (%d stop by a division by zero), %d damaged texts" % (made, count, stops, damaged))
    if "--no-lower" not in argv:
        compared, refused = lower_tests(azol, binary, problems)
        print("%d whole programs lowered: interpreted like their executables, verified, printed and read back (%d more are not programs)" % (compared, refused))
        print("the compiler lowered: verified, read back, and interpreted it builds examples/hello.azl and itself as azol does")
    for problem in problems:
        print("FAIL " + problem)
    if problems:
        print("%d FAILED; files are in %s" % (len(problems), folder))
        sys.exit(1)
    if "--keep" not in argv:
        for name in os.listdir(folder):
            os.unlink(os.path.join(folder, name))
        os.rmdir(folder)
    print("ALL OPT TESTS PASSED")


if __name__ == "__main__":
    main()
