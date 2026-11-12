#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Tests of the loop passes `licm` and `bce` and of the range facts they
use (compiler/opt/pass_licm.azl, pass_bce.azl, pass_range.azl).

    python3 tests/opt/run_loop_tests.py [--azol PATH] [--random N] [--seed N] [--keep]
                                        [--only cases|random|programs]

Needs only Python 3 and an azoL compiler (default: ./azol). It builds the
tools of run_pass_tests.py and then:

1. cases     tests/opt/licm, tests/opt/bce, tests/opt/range: every
             NAME.ir must come out of `opt-tool --passes <pass>` as
             NAME.expected.ir, byte for byte (the folder `range` is run
             through `bce`, which is how a range fact is seen); the two
             must do the same in the interpreter on every line of
             NAME.runs, a stop with `trap index` included (same output
             before it, same message, same exit code); and the pass
             must leave its own result alone. A case whose name starts
             with `no_` is one where a rule must not fire; its first
             comment line says why.
2. random    random functions with loops over lists: counters that go
             up, down, by a step from outside, from a start that may be
             negative; tests against the length, a number, the smaller
             of two; one or two lists; rounds that print, store, call or
             leave early; lists that are too short. Before and after
             `licm,bce,simplify,dce` (twice) they do the same in the
             interpreter for several arguments, and as native code for
             a part of them.
3. programs  small azoL programs with the loop shapes the rules are for
             and the ones they must leave alone, some of which stop with
             an index out of range after printing: built by `azol-opt`
             with the full pipeline (also with three registers) they do
             what the default build does.

The whole programs of the lowering proof and the compiler itself go
through these passes in tests/opt/run_pass_tests.py, since `licm` and
`bce` are in the pipeline's order.
"""
import os
import platform
import random
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import run_opt_tests as base
import run_pass_tests as passes

HERE = base.HERE
# The folder of cases and the passes its cases go through.
FOLDERS = [("licm", "licm"), ("bce", "bce"), ("range", "bce")]
LOOP_PASSES = "licm,bce,simplify,dce,licm,bce,simplify,dce"


def case_tests(tools, problems):
    count = 0
    kept = 0
    for part, names in FOLDERS:
        cases = os.path.join(HERE, part)
        for name in sorted(os.listdir(cases)):
            if not name.endswith(".ir") or name.endswith(".expected.ir"):
                continue
            count += 1
            path = os.path.join(cases, name)
            want_path = path[:-3] + ".expected.ir"
            with open(path, encoding="utf-8") as handle:
                first = handle.readline()
            if not first.startswith(";") or len(first) < 12:
                problems.append("case %s/%s: the first line must be a comment that says what the case shows" % (part, name))
            code, out, err = base.tool(tools["opt"], path, "--passes", names, "--verify-each")
            with open(want_path, "rb") as handle:
                want = handle.read()
            if code != 0 or out != want:
                problems.append("case %s/%s: the result is not the expected text (exit %r, %r)\n%s" % (part, name, code, base.lines(err), out.decode(errors="replace")))
                continue
            again = base.tool(tools["opt"], want_path, "--passes", names)
            if again[0] != 0 or again[1] != want:
                problems.append("case %s/%s: the pass changes its own result" % (part, name))
            if name.startswith("no_"):
                kept += 1
            with open(path[:-3] + ".runs", encoding="utf-8") as handle:
                runs = [row.strip() for row in handle if row.strip()]
            for row in runs:
                before = base.tool(tools["ir"], "run", path, *row.split())
                after = base.tool(tools["ir"], "run", want_path, *row.split())
                if before != after or before[0] not in (0, 1):
                    problems.append("case %s/%s: `run %s` differs before and after: %r, %r" % (part, name, row, before, after))
    return count, kept


# ---- 2. random loops -------------------------------------------------------------

MAIN = """fn main(p0: i64, p1: i64, p2: i64) -> i64 {
    frames 24, 24
b0:
    v0 = frame 0
    v1 = callrt alloc(64)
    store i64, v1, 0, 3  [buf(frame 0)]
    store i64, v1, 8, -7  [buf(frame 0)]
    store i64, v1, 16, 11  [buf(frame 0)]
    store i64, v1, 24, 20  [buf(frame 0)]
    store i64, v1, 32, -1  [buf(frame 0)]
    store i64, v1, 40, 8  [buf(frame 0)]
    store i64, v1, 48, 2  [buf(frame 0)]
    store i64, v1, 56, 5  [buf(frame 0)]
    store ptr, v0, 0, v1  [frame 0]
    store i64, v0, 8, %d  [frame 0]
    store i64, v0, 16, 8  [frame 0]
    v3 = frame 1
    v4 = callrt alloc(32)
    store i64, v4, 0, 1  [buf(frame 1)]
    store i64, v4, 8, 2  [buf(frame 1)]
    store i64, v4, 16, 4  [buf(frame 1)]
    store i64, v4, 24, 9  [buf(frame 1)]
    store ptr, v3, 0, v4  [frame 1]
    store i64, v3, 8, %d  [frame 1]
    store i64, v3, 16, 4  [frame 1]
    v2 = call w(v0, v3, p0, p1, p2) -> i64
    callrt free(v1)
    callrt free(v4)
    ret v2
}
"""


def random_loop(rng):
    """One module: `w` walks one or two lists in a loop (sometimes with a
    loop inside), `main` makes the lists. Gives the text and argument
    rows. Every loop ends: its counter moves by a step that is not 0
    towards a test, or a second counter counts the rounds."""
    out = []
    n = [20]

    def fresh():
        n[0] += 1
        return "v%d" % n[0]

    writes = rng.random() < 0.3
    fx = "read writebuf" if writes else "read"
    out.append("fn w(p0: ptr [param 0 %s], p1: ptr [param 1 read], p2: i64, p3: i64, p4: i64) -> i64 {" % fx)
    out.append("    frames 8")
    out.append("b0:")
    out.append("    v0 = load i64, p0, 8  [param 0]")
    out.append("    v1 = load ptr, p0, 0  [param 0]")
    out.append("    v10 = load i64, p1, 8  [param 1]")
    out.append("    v11 = load ptr, p1, 0  [param 1]")
    out.append("    v12 = frame 0")
    out.append("    store i64, v12, 0, p4  [frame 0]")
    out.append("    v13 = slt v0, v10")
    out.append("    v14 = select v13, v0, v10")
    out.append("    v15 = sub v0, 1")
    down = rng.random() < 0.2
    start = rng.choice(["0", "0", "0", "1", "p3", "v15"]) if not down else rng.choice(["v15", "p3", "3"])
    bound = rng.choice(["v0", "v0", "p2", "p2", "v14", "v15", "v10", "4", "8"])
    step = rng.choice(["1", "1", "1", "2", "3", "p4", "4611686018427387904"])
    if down:
        guard = rng.choice(["sgt v2, -1", "sge v2, 0", "sgt v2, p2"])
        move = "sub v2, %s" % rng.choice(["1", "1", "2"])
    else:
        guard = "%s v2, %s" % (rng.choice(["slt", "slt", "slt", "sle"]), bound)
        if rng.random() < 0.15:
            # Unsigned, as the loop of `for x in xs` is tested: only
            # ever against a length.
            guard = "ult v2, %s" % rng.choice(["v0", "v10", "v14", "4"])
        move = "add v2, %s" % step
    out.append("    jump b1(%s, 0, 0)" % start)
    # v2: the counter; v3: the sum; v4: the rounds, which end every loop.
    out.append("b1(v2: i64, v3: i64, v4: i64):")
    if rng.random() < 0.3:
        out.append("    %s = load i64, p0, 8  [param 0]" % fresh())
    out.append("    v5 = %s" % guard)
    if rng.random() < 0.2:
        out.append("    v6 = slt v4, 12")
        out.append("    v7 = and v5, v6")
        out.append("    br v7, b2, b9")
    else:
        out.append("    br v5, b2, b9")
    out.append("b2:")
    total = "v3"
    # What a round does before its first check.
    lead = rng.random()
    if lead < 0.12:
        out.append("    callrt print_i64(v2)")
    elif lead < 0.2:
        out.append("    store i64, v12, 0, v2  [frame 0]")
    elif lead < 0.3:
        out.append("    v16 = sgt v3, 30")
        out.append("    br v16, b9, b3")
        out.append("b3:")
    index = "v2"
    shape = rng.random()
    if shape < 0.2:
        index = fresh()
        out.append("    %s = add v2, %s" % (index, rng.choice(["1", "p3", "-1", "2"])))
    elif shape < 0.28:
        index = "p3"
    elif shape < 0.34:
        index = fresh()
        out.append("    %s = and v2, 7" % index)
    elif shape < 0.4:
        index = fresh()
        out.append("    %s = mul v2, 2" % index)
    length = "v0"
    if rng.random() < 0.15:
        length = fresh()
        out.append("    %s = load i64, p0, 8  [param 0]" % length)
    check = fresh()
    out.append("    %s = ult %s, %s" % (check, index, length))
    out.append("    br %s, b4, b8" % check)
    out.append("b4:")
    address = fresh()
    value = fresh()
    out.append("    %s = gep v1, %s, 8, 0" % (address, index))
    out.append("    %s = load i64, %s, 0  [buf(param 0)]" % (value, address))
    total2 = fresh()
    out.append("    %s = add %s, %s" % (total2, total, value))
    total = total2
    if writes and rng.random() < 0.6:
        out.append("    store i64, %s, 0, %s  [buf(param 0)]" % (address, total))
    if rng.random() < 0.2:
        out.append("    callrt print_i64(%s)" % total)
    if rng.random() < 0.45:
        # The second list, at the counter or at a fixed place.
        other = rng.choice(["v2", "v2", index, "p3"])
        check = fresh()
        out.append("    %s = ult %s, v10" % (check, other))
        out.append("    br %s, b5, b8" % check)
        out.append("b5:")
        address = fresh()
        value = fresh()
        out.append("    %s = gep v11, %s, 8, 0" % (address, other))
        out.append("    %s = load i64, %s, 0  [buf(param 1)]" % (value, address))
        total2 = fresh()
        out.append("    %s = mul %s, %s" % (total2, total, value))
        total = total2
    if rng.random() < 0.3:
        # A loop inside, over the first list, with its own check.
        which = rng.choice(["v17", "v17", "v2"])
        inner_bound = rng.choice(["v0", "v2", "p2", "v14"]) if which == "v17" else rng.choice(["v0", "v14"])
        out.append("    jump b6(0, %s)" % total)
        out.append("b6(v17: i64, v18: i64):")
        out.append("    v19 = slt v17, %s" % inner_bound)
        out.append("    br v19, b7, b10(v18)")
        out.append("b7:")
        check = fresh()
        out.append("    %s = ult %s, v0" % (check, which))
        out.append("    br %s, b11, b8" % check)
        out.append("b11:")
        address = fresh()
        value = fresh()
        out.append("    %s = gep v1, %s, 8, 0" % (address, which))
        out.append("    %s = load i64, %s, 0  [buf(param 0)]" % (value, address))
        scratch = fresh()
        out.append("    %s = load i64, v12, 0  [frame 0]" % scratch)
        inner_total = fresh()
        out.append("    %s = add v18, %s" % (inner_total, value))
        mixed = fresh()
        out.append("    %s = xor %s, %s" % (mixed, inner_total, scratch))
        out.append("    v20 = add v17, 1")
        out.append("    jump b6(v20, %s)" % mixed)
        out.append("b10(%s: i64):" % (fresh()))
        total = "v%d" % n[0]
    moved = fresh()
    out.append("    %s = %s" % (moved, move))
    rounds = fresh()
    out.append("    %s = add v4, 1" % rounds)
    # At most twelve rounds, whatever the counter does.
    more = fresh()
    out.append("    %s = slt %s, 12" % (more, rounds))
    out.append("    br %s, b1(%s, %s, %s), b9" % (more, moved, total, rounds))
    out.append("b8:")
    out.append("    trap index")
    out.append("b9:")
    out.append("    ret v3")
    out.append("}")
    out.append("")
    first = rng.choice([8, 8, 5, 3, 1, 0])
    second = rng.choice([4, 4, 2, 0])
    out.append(MAIN % (first, second))
    rows = []
    for _ in range(6):
        rows.append([rng.choice([0, 1, 2, 3, 5, 8, 9, -1, 100]), rng.choice([0, 0, 1, 2, 3, 7, -1, -2, 8]), rng.choice([1, 1, 2, 3, 0, -1, 5])])
    rows.append([(1 << 63) - 1, 0, 1 << 62])
    return "\n".join(out), rows


def random_tests(tools, total, seed, folder, problems):
    rng = random.Random(seed)
    made = 0
    natives = 0
    fired = 0
    stops = 0
    while made < total and len(problems) < 5:
        text, rows = random_loop(rng)
        made += 1
        path = os.path.join(folder, "loop%d.ir" % made)
        after = os.path.join(folder, "loop%d.after.ir" % made)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        code, out, err = base.tool(tools["opt"], path, "-o", after, "--passes", LOOP_PASSES, "--verify-each", timeout=300)
        if code != 0:
            problems.append("random loop %d (seed %d): the passes fail (exit %r, %r); kept as %s" % (made, seed, code, base.lines(err)[:3], path))
            continue
        with open(after, encoding="utf-8") as handle:
            if handle.read().count(" = ult ") < text.count(" = ult "):
                fired += 1
        wrong = False
        for at, arguments in enumerate(rows):
            arguments = [str(x) for x in arguments]
            before = base.tool(tools["ir"], "run", path, "main", *arguments)
            result = base.tool(tools["ir"], "run", after, "main", *arguments)
            if before != result or before[0] is None:
                problems.append("random loop %d (seed %d): `run main %s` gives %r before the passes and %r after; kept as %s" % (made, seed, " ".join(arguments), before, result, path))
                wrong = True
                break
            if before[0] == 1:
                stops += 1
            if passes.native() and at < 2 and made % 4 == 0:
                one = passes.run_native(tools, path, folder, "a", "main", arguments)
                two = passes.run_native(tools, after, folder, "b", "main", arguments)
                natives += 1
                if one != two:
                    problems.append("random loop %d (seed %d): as native code `main %s` gives %r before the passes and %r after; kept as %s" % (made, seed, " ".join(arguments), one, two, path))
                    wrong = True
                    break
        if not wrong:
            os.unlink(path)
            os.unlink(after)
    return made, natives, fired, stops


# ---- 3. small programs ---------------------------------------------------------------

PROGRAM = """fn sum(xs: List[i64]) -> i64 {
    var total = 0
    var i = 0
    while i < xs.len() {
        total += xs[i]
        i += 1
    }
    return total
}

fn first(xs: List[i64], n: i64) -> i64 {
    var total = 0
    for i in 0..n {
        total += xs[i]
        print(total)
    }
    return total
}

fn pairs(xs: List[i64]) -> i64 {
    var t = 0
    var i = 0
    let n = xs.len()
    while i < n - 1 {
        t += xs[i] * xs[i + 1]
        i += 1
    }
    return t
}

fn down(xs: List[i64]) -> i64 {
    var t = 0
    var i = xs.len() - 1
    while i >= 0 {
        t = t * 3 + xs[i]
        i -= 1
    }
    return t
}

fn dot(a: List[i64], b: List[i64]) -> i64 {
    var n = a.len()
    if b.len() < n { n = b.len() }
    var t = 0
    for i in 0..n {
        t += a[i] * b[i]
    }
    return t
}

fn zip(a: List[i64], b: List[i64]) -> i64 {
    var t = 0
    for i, x in a {
        t += x * b[i]
    }
    return t
}

fn rows(m: List[List[i64]]) -> i64 {
    var t = 0
    for i in 0..m.len() {
        for j in 0..m[i].len() {
            t += m[i][j]
        }
    }
    return t
}

fn flat(a: List[i64], n: i64) -> i64 {
    var t = 0
    for i in 0..n {
        for k in 0..n {
            t += a[i * n + k]
        }
    }
    return t
}

fn sieve(n: i64) -> i64 {
    var flags: List[bool] = []
    var i = 0
    while i < n {
        flags.push(true)
        i += 1
    }
    var count = 0
    i = 2
    while i < n {
        if flags[i] {
            count += 1
            var j = i * 2
            while j < n {
                flags[j] = false
                j += i
            }
        }
        i += 1
    }
    return count
}

fn grow(n: i64) -> i64 {
    var xs = [1]
    var i = 0
    var t = 0
    while i < n {
        t += xs[i]
        xs.push(t)
        i += 1
    }
    return t
}

fn stride(xs: List[i64], from: i64, by: i64) -> i64 {
    var t = 0
    var i = from
    while i < xs.len() {
        t += xs[i]
        i += by
    }
    return t
}

fn main() {
    var xs = [1, 2, 3, 4, 5]
    var ys = [2, 3, 4]
    var m = [[1, 2], [3, 4, 5], []]
    let empty: List[i64] = []
    print(sum(xs) + sum(empty))
    print(pairs(xs) + pairs(empty) + pairs(ys))
    print(down(xs) + down(empty))
    print(dot(xs, ys) + dot(ys, xs) + dot(empty, xs))
    print(zip(ys, xs))
    print(rows(m))
    print(flat(xs, 2) + flat(xs, 0))
    print(sieve(100) + sieve(0) + sieve(3))
    print(grow(10))
    print(stride(xs, 0, 2) + stride(xs, 1, 3) + stride(xs, 9, 1))
    print(first(xs, 3))
    WHAT
}
"""

# What the last line of `main` is: each stops the program with an index
# out of range after everything above has been printed, and some after
# prints of their own.
ENDS = ["print(first(xs, 5))", "print(first(xs, 9))", "print(zip(xs, ys))", "print(flat(xs, 3))", "print(stride(xs, -1, 1))", "print(first(empty, 1))",
        "print(flat(ys, 2))", "xs.pop()\n    print(pairs(xs) + first(xs, 5))"]


def program_tests(azol, tools, folder, problems):
    if not passes.native():
        return 0
    compared = 0
    for at, end in enumerate(ENDS):
        source = os.path.join(folder, "loops%d.azl" % at)
        with open(source, "w", encoding="utf-8") as handle:
            handle.write(PROGRAM.replace("WHAT", end))
        plain = os.path.join(folder, "loops%d-default" % at)
        built = subprocess.run([azol, "build", source, "-o", plain], capture_output=True)
        if built.returncode != 0:
            problems.append("loop program %d: `azol build` fails: %r" % (at, built.stderr[-300:]))
            continue
        want = subprocess.run([plain], capture_output=True, timeout=60)
        want = (want.returncode, want.stdout, want.stderr)
        if at > 0 and want[0] == 0:
            problems.append("loop program %d: was to stop with an index out of range and did not" % at)
        for label, more in (("the pipeline", []), ("the pipeline to a fixed point", ["--fixpoint"]), ("three registers", ["--fixpoint", "--stress"])):
            exe = os.path.join(folder, "loops%d-opt" % at)
            built = subprocess.run([tools["build"], "build", source, "-o", exe, "--verify-each", "--check", *more], capture_output=True)
            if built.returncode != 0:
                problems.append("loop program %d, %s: `azol-opt build` fails: %r" % (at, label, built.stderr[-300:]))
                continue
            got = subprocess.run([exe], capture_output=True, timeout=60)
            if (got.returncode, got.stdout, got.stderr) != want:
                problems.append("loop program %d, %s: gives %r, the default build %r" % (at, label, (got.returncode, got.stdout[-200:], got.stderr), want))
        interpreted = subprocess.run([tools["build"], "run", source, "--fixpoint", "--argv0", plain], capture_output=True, timeout=120)
        if (interpreted.returncode, interpreted.stdout, interpreted.stderr) != want:
            problems.append("loop program %d, interpreted after the passes: gives %r, the default build %r" % (at, (interpreted.returncode, interpreted.stdout[-200:], interpreted.stderr), want))
        compared += 1
    return compared


def run(azol, folder, total, seed, problems, only=None, tools=None):
    if tools is None:
        tools = passes.build_tools(azol, folder)
    if only in (None, "cases"):
        count, kept = case_tests(tools, problems)
        print("loops: %d hand-written cases of licm, bce and the range facts, %d of them where a rule must not fire" % (count, kept))
    if only in (None, "random") and total > 0:
        made, natives, fired, stops = random_tests(tools, total, seed, folder, problems)
        print("loops: %d random loops over lists the same before and after (%d runs also as native code); a check went in %d of them; %d runs stop with an index out of range, the same way" % (made, natives, fired, stops))
    if only in (None, "programs"):
        compared = program_tests(azol, tools, folder, problems)
        print("loops: %d programs of loop shapes, %d of which stop with an index out of range after printing: by azol-opt (also to a fixed point, with three registers, and interpreted) like their default builds" % (compared, max(compared - 1, 0)))


def main():
    argv = sys.argv[1:]
    azol = base.find_azol(argv)
    total = int(argv[argv.index("--random") + 1]) if "--random" in argv else 1500
    seed = int(argv[argv.index("--seed") + 1]) if "--seed" in argv else 20261112
    only = argv[argv.index("--only") + 1] if "--only" in argv else None
    folder = tempfile.mkdtemp(prefix="azol-loop-")
    problems = []
    run(azol, folder, total, seed, problems, only)
    for problem in problems:
        print("FAIL " + problem)
    if problems:
        print("%d FAILED; files are in %s" % (len(problems), folder))
        sys.exit(1)
    if "--keep" not in argv:
        shutil.rmtree(folder, ignore_errors=True)
    print("ALL LOOP TESTS PASSED")


if __name__ == "__main__":
    main()
