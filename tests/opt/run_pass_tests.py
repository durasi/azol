#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Tests of the passes `simplify`, `dce` and `dse` and of the pipeline
(compiler/opt/pass_*.azl, pipeline.azl, opt_tool_main.azl,
opt_build_main.azl).

    python3 tests/opt/run_pass_tests.py [--azol PATH] [--random N] [--seed N] [--keep]
                                        [--only cases|rules|random|programs|compiler]

Needs only Python 3 and an azoL compiler (default: ./azol). It builds
`opt-tool`, `ir-tool` and `azol-opt` and then:

1. cases     tests/opt/simplify, tests/opt/dce, tests/opt/dse: every
             NAME.ir must come out of `opt-tool --passes <the folder's
             pass>` as NAME.expected.ir, byte for byte; the two must run
             the same in the interpreter on every line of NAME.runs; and
             the expected text must come out of the pass unchanged. A
             case whose name starts with `no_` is one where a rule must
             not fire; its first comment line says why.
2. rules     every operation with two operands, for every pair from a
             set of edge numbers, with the second, the first or both as
             constants: the function before and after the passes gives
             what the script's own arithmetic gives.
3. random    random functions (the generators of run_opt_tests.py and
             run_back_tests.py): the same answers before and after the
             passes, in the interpreter and as native code, and a second
             run of the passes changes nothing.
4. programs  every program the lowering proof uses: interpreted after
             the passes, and built by `azol-opt` and run, it does what
             its default build does.
5. compiler  the compiler itself, lowered, optimised and built by the
             optimising back end, builds examples/hello.azl and itself
             into the bytes `azol` writes.

Sections 3 to 5 need x86-64 Linux for the native half.
"""
import concurrent.futures
import os
import platform
import random
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import run_opt_tests as base
import run_back_tests as back_tests

HERE = base.HERE
ROOT = base.ROOT
PASSES = ["pass_simplify_util.azl", "pass_simplify.azl", "pass_dce.azl", "pass_dse.azl", "pass_sroa.azl", "pipeline.azl"]
OPT_TOOL = ["ir.azl", "ir_text.azl", "ir_cfg.azl", "ir_verify.azl"] + PASSES + ["opt_tool_main.azl"]
COMPILER = ["bytes.azl", "code_file.azl", "elf.azl", "runtime_x64.azl"]
OPT_BUILD = (["ir.azl", "ir_text.azl", "ir_cfg.azl", "ir_verify.azl", "ir_interp.azl", "ir_lower.azl", "ir_lower_stmt.azl", "ir_lower_expr.azl"] + PASSES +
             ["mach.azl", "regalloc.azl", "enc_x64.azl", "sel_x64.azl", "emit_x64.azl", "back_x64.azl", "opt_build_main.azl"])
FOLDERS = ["simplify", "dce", "dse"]


def native():
    return platform.system() == "Linux" and platform.machine() == "x86_64"


def build_tools(azol, folder):
    tools = {"opt": os.path.join(folder, "opt-tool"), "ir": os.path.join(folder, "ir-tool"), "build": os.path.join(folder, "azol-opt"),
             "back": os.path.join(folder, "azol-back")}
    base.build(azol, OPT_TOOL, tools["opt"])
    base.build(azol, base.SOURCES, tools["ir"])
    front = [os.path.join(ROOT, "compiler", name) for name in base.FRONT + COMPILER]
    base.build(azol, front + OPT_BUILD, tools["build"])
    base.build(azol, back_tests.BACK_SOURCES, tools["back"])
    return tools


# ---- 1. the hand-written cases ---------------------------------------------------


def case_tests(tools, problems):
    count = 0
    kept = 0
    for part in FOLDERS:
        cases = os.path.join(HERE, part)
        if not os.path.isdir(cases):
            continue
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
            if not os.path.exists(want_path):
                problems.append("case %s/%s: there is no %s" % (part, name, os.path.basename(want_path)))
                continue
            code, out, err = base.tool(tools["opt"], path, "--passes", part, "--verify-each")
            with open(want_path, "rb") as handle:
                want = handle.read()
            if code != 0 or out != want:
                problems.append("case %s/%s: the result is not the expected text (exit %r, %r)\n%s" % (part, name, code, base.lines(err), out.decode(errors="replace")))
                continue
            again = base.tool(tools["opt"], want_path, "--passes", part)
            if again[0] != 0 or again[1] != want:
                problems.append("case %s/%s: the pass changes its own result" % (part, name))
            if name.startswith("no_"):
                kept += 1
            runs_path = path[:-3] + ".runs"
            if not os.path.exists(runs_path):
                problems.append("case %s/%s: there is no %s" % (part, name, os.path.basename(runs_path)))
                continue
            with open(runs_path, encoding="utf-8") as handle:
                runs = [row.strip() for row in handle if row.strip()]
            for row in runs:
                before = base.tool(tools["ir"], "run", path, *row.split())
                after = base.tool(tools["ir"], "run", want_path, *row.split())
                if before != after or before[0] not in (0, 1):
                    problems.append("case %s/%s: `run %s` differs before and after: %r, %r" % (part, name, row, before, after))
    return count, kept


# ---- 2. every rule on edge numbers -------------------------------------------------

EDGE = [0, 1, -1, 2, -2, 3, 7, 8, 62, 63, 64, 65, 127, 128, 255, 256, (1 << 31) - 1, 1 << 31, (1 << 32) - 1, 1 << 32, (1 << 32) + 1,
        1 << 62, (1 << 62) - 1, (1 << 62) + 1, (1 << 63) - 1, -(1 << 63), -(1 << 63) + 1, -(1 << 31), -(1 << 31) - 1, -255, -256, -64, -63, -8, -7]
DIVIDE = ["sdiv", "srem"]


def expected_value(op, a, b):
    if op == "sdiv":
        if b == -1:
            return base.wrap(-a)
        quotient = abs(a) // abs(b)
        return base.wrap(quotient if (a < 0) == (b < 0) else -quotient)
    if op == "srem":
        if b == -1:
            return 0
        rest = abs(a) % abs(b)
        return base.wrap(rest if a >= 0 else -rest)
    return base.evaluate(op, a, b)


def rule_tests(tools, folder, problems):
    """One file per operation: a function for every pair of edge numbers
    and every way of making them constants; `main` calls them all and
    prints what each gives."""
    checked = 0
    for op in base.BINARY + base.COMPARE + DIVIDE:
        result = "bool" if op in base.COMPARE else "i64"
        text = []
        calls = []
        want = []
        number = 0
        for a in EDGE:
            for b in EDGE:
                if op in DIVIDE and b == 0:
                    continue
                value = expected_value(op, a, b)
                for shape in ("vc", "cv", "cc"):
                    name = "f%d" % number
                    number += 1
                    if shape == "vc":
                        text.append("fn %s(p0: i64) -> %s {\nb0:\n    v0 = %s p0, %d\n    ret v0\n}\n" % (name, result, op, b))
                        calls.append((name, [a]))
                    elif shape == "cv":
                        text.append("fn %s(p0: i64) -> %s {\nb0:\n    v0 = %s %d, p0\n    ret v0\n}\n" % (name, result, op, a))
                        calls.append((name, [b]))
                    else:
                        text.append("fn %s() -> %s {\nb0:\n    v0 = %s %d, %d\n    ret v0\n}\n" % (name, result, op, a, b))
                        calls.append((name, []))
                    want.append(value)
        # The same value on both sides.
        for a in EDGE:
            if op in DIVIDE and a == 0:
                continue
            text.append("fn f%d(p0: i64) -> %s {\nb0:\n    v0 = %s p0, p0\n    ret v0\n}\n" % (number, result, op))
            calls.append(("f%d" % number, [a]))
            want.append(expected_value(op, a, a))
            number += 1
        main = ["fn main() {", "b0:"]
        for at, (name, arguments) in enumerate(calls):
            main.append("    v%d = call %s(%s) -> %s" % (at, name, ", ".join(str(x) for x in arguments), result))
            main.append("    callrt print_%s(v%d)" % ("bool" if result == "bool" else "i64", at))
        main += ["    ret", "}", ""]
        path = os.path.join(folder, "rule_%s.ir" % op)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write("\n".join(text) + "\n".join(main))
        after = os.path.join(folder, "rule_%s.after.ir" % op)
        code, out, err = base.tool(tools["opt"], path, "-o", after, "--fixpoint", "--verify-each", timeout=300)
        if code != 0:
            problems.append("rules: `%s`: the passes fail (exit %r, %r)" % (op, code, base.lines(err)[:3]))
            continue
        if result == "bool":
            lines = ["true" if value else "false" for value in want]
        else:
            lines = [str(value) for value in want]
        for label, file in (("before", path), ("after", after)):
            code, out, err = base.tool(tools["ir"], "run", file, "main", timeout=600)
            got = base.lines(out)
            if code != 0 or got[:len(lines)] != lines:
                bad = [at for at in range(min(len(got), len(lines))) if got[at] != lines[at]][:1]
                where = calls[bad[0]] if bad else "?"
                problems.append("rules: `%s` %s the passes: exit %r, first wrong case %r (%r)" % (op, label, code, where, base.lines(err)[:2]))
        # No function of constants or of one value and a constant should
        # still compute anything the rules could have decided.
        with open(after, encoding="utf-8") as handle:
            left = handle.read()
        if op not in ("umulh", "smulh") and op not in DIVIDE:
            folded = left.count(" = %s " % op)
            # One instruction per function with a value is the most that
            # can stay; the functions of two constants keep none.
            if folded > 2 * len(EDGE) * len(EDGE):
                problems.append("rules: `%s`: %d instructions are left, more than one per function with a value" % (op, folded))
        checked += len(calls)
        if not problems:
            os.unlink(path)
            os.unlink(after)
    return checked


# ---- 3. random functions --------------------------------------------------------------


def run_native(tools, path, folder, tag, name, arguments):
    exe = os.path.join(folder, "native-%s" % tag)
    done = subprocess.run([tools["back"], "build", path, "-o", exe, "--run", name, *[str(x) for x in arguments], "--check"], capture_output=True)
    if done.returncode != 0:
        return ("build", done.returncode, done.stderr[-300:])
    try:
        ran = subprocess.run([exe], capture_output=True, timeout=60)
    except subprocess.TimeoutExpired:
        return ("run", None, b"")
    os.unlink(exe)
    return (ran.returncode, ran.stdout, ran.stderr)


def random_tests(tools, total, seed, folder, problems):
    """The random modules of the two other scripts through the passes."""
    rng = random.Random(seed)
    made = 0
    files = 0
    natives = 0
    smaller = 0
    while made < total and len(problems) < 5:
        if files % 2 == 0:
            count = rng.randint(2, 10)
            text = base.random_module(rng, count)[0]
            rows = [("main", [])]
        else:
            text, arguments, count = back_tests.random_back_module(rng)
            rows = [("main", arguments)]
        made += count
        files += 1
        path = os.path.join(folder, "random%d.ir" % files)
        after = os.path.join(folder, "random%d.after.ir" % files)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        code, out, err = base.tool(tools["opt"], path, "-o", after, "--fixpoint", "--verify-each", timeout=300)
        if code != 0:
            problems.append("random file %d (seed %d): the passes fail (exit %r, %r); kept as %s" % (files, seed, code, base.lines(err)[:3], path))
            continue
        again = base.tool(tools["opt"], after, "--fixpoint")
        with open(after, "rb") as handle:
            optimised = handle.read()
        if again[0] != 0 or again[1] != optimised:
            problems.append("random file %d (seed %d): a second run of the passes changes the result; kept as %s" % (files, seed, after))
            continue
        if len(optimised) < len(text.encode()):
            smaller += 1
        wrong = False
        for at, (name, arguments) in enumerate(rows):
            arguments = [str(x) for x in arguments]
            before = base.tool(tools["ir"], "run", path, name, *arguments)
            result = base.tool(tools["ir"], "run", after, name, *arguments)
            if before != result:
                problems.append("random file %d (seed %d): `run %s %s` gives %r before the passes and %r after; kept as %s" % (files, seed, name, " ".join(arguments), before, result, path))
                wrong = True
                break
            # Natively, for a part of the runs: the back end has its own
            # test for the rest.
            if native() and at < 2 and before[0] == 0:
                one = run_native(tools, path, folder, "a", name, arguments)
                two = run_native(tools, after, folder, "b", name, arguments)
                natives += 1
                if one != two:
                    problems.append("random file %d (seed %d): as native code `%s %s` gives %r before the passes and %r after; kept as %s" % (files, seed, name, " ".join(arguments), one, two, path))
                    wrong = True
                    break
        if not wrong:
            os.unlink(path)
            os.unlink(after)
    return made, files, natives, smaller


# ---- 4. whole programs -----------------------------------------------------------------


def program_one(azol, tools, folder, at, label, files, want):
    exe = os.path.join(folder, "p%d-default" % at)
    done = subprocess.run([azol, "build", *files, "-o", exe], capture_output=True)
    if done.returncode != 0:
        return None, []
    found = []
    opt = os.path.join(folder, "p%d-opt" % at)
    stress = os.path.join(folder, "p%d-stress" % at)
    commands = [("default build", [exe])]
    commands.append(("passes, interpreted", ["sh", "-c", 'ulimit -s unlimited 2>/dev/null; exec "$@"', "sh", tools["build"], "run", *files, "--fixpoint", "--verify-each", "--argv0", exe]))
    if native():
        for target, more in ((opt, []), (stress, ["--stress"])):
            built = subprocess.run([tools["build"], "build", *files, "-o", target, "--fixpoint", "--check", *more], capture_output=True)
            if built.returncode != 0:
                found.append("program %s: `azol-opt build %s` fails (exit %r, %r)" % (label, " ".join(more), built.returncode, built.stderr[-300:]))
                return "run", found
        # The executables print their own name in some programs: run
        # each under the name of the default build.
        commands.append(("passes, native", [opt]))
        commands.append(("passes, native, three registers", [stress]))
    work = os.path.join(folder, "pw%d" % at)
    results = []
    for kind, command in commands:
        if os.path.isdir(work):
            shutil.rmtree(work)
        os.mkdir(work)
        run = command
        if kind.startswith("passes, native"):
            # Under the same name as the default build, in its own folder.
            alias = os.path.join(work, os.path.basename(exe))
            shutil.copy(command[0], alias)
            run = [alias]
        try:
            ran = subprocess.run(run, capture_output=True, timeout=900, cwd=work)
            results.append((ran.returncode, ran.stdout, ran.stderr))
        except subprocess.TimeoutExpired:
            results.append((None, b"", b"(no answer in time)"))
    shutil.rmtree(work)
    real = results[0]
    for (kind, _), got in zip(commands[1:], results[1:]):
        same = got == real
        if not same and kind.startswith("passes, native") and got[0] == real[0] and got[1] == real[1]:
            # A program that prints the path it was started by.
            same = got[2].replace(os.path.join(work, os.path.basename(exe)).encode(), exe.encode()) == real[2]
        if not same and real[0] is not None and real[0] < 0 and got[0] == real[0] and got[1] == real[1]:
            same = True
        if not same:
            found.append("program %s (%s): exit %r, %r, %r; the default build gives exit %r, %r, %r" %
                         (label, kind, got[0], got[1][-200:], got[2][-300:], real[0], real[1][-200:], real[2][-200:]))
    if want is not None and (real[0] != want[0] or real[1] != want[1]):
        found.append("program %s: the default build does not do what the `// expect-` comments say" % label)
    for path in (exe, opt, stress):
        if os.path.exists(path):
            os.unlink(path)
    return "run", found


def program_tests(azol, tools, folder, problems):
    programs = base.lower_programs(folder, problems)
    compared = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=os.cpu_count() or 2) as pool:
        jobs = [pool.submit(program_one, azol, tools, folder, at, label, files, want) for at, (label, files, want) in enumerate(programs)]
        for job in jobs:
            what, found = job.result()
            problems.extend(found)
            if what is not None:
                compared += 1
    return compared


# ---- 5. the compiler ---------------------------------------------------------------------


def compiler_test(azol, tools, folder, problems):
    if not native():
        return False
    with open(os.path.join(ROOT, "compiler", "SOURCES"), encoding="utf-8") as handle:
        sources = [os.path.join(ROOT, "compiler", line.strip()) for line in handle if line.strip() and not line.startswith("#")]
    sources.append(os.path.join(ROOT, "compiler", "host_x86_64_linux.azl"))
    optimised = os.path.join(folder, "azol-optimised")
    built = subprocess.run([tools["build"], "build", *sources, "-o", optimised, "--fixpoint", "--verify-each", "--check"], capture_output=True)
    if built.returncode != 0:
        problems.append("the compiler: `azol-opt build` fails (exit %r, %r)" % (built.returncode, built.stderr[-400:]))
        return False
    hello = os.path.join(ROOT, "examples", "hello.azl")
    for label, files in (("examples/hello.azl", [hello]), ("itself", sources)):
        real = os.path.join(folder, "built-real")
        other = os.path.join(folder, "built-optimised")
        one = subprocess.run([azol, "build", *files, "-o", real], capture_output=True)
        two = subprocess.run([optimised, "build", *files, "-o", other], capture_output=True)
        if one.returncode != 0 or two.returncode != 0:
            problems.append("the optimised compiler building %s: exit %r (azol: %r), %r" % (label, two.returncode, one.returncode, two.stderr[-300:]))
            continue
        with open(real, "rb") as first, open(other, "rb") as second:
            if first.read() != second.read():
                problems.append("the optimised compiler builds %s differently from %s" % (label, azol))
    return True


def run(azol, folder, total, seed, problems, only=None):
    tools = build_tools(azol, folder)
    if only in (None, "cases"):
        count, kept = case_tests(tools, problems)
        print("passes: %d hand-written cases, %d of them where a rule must not fire" % (count, kept))
    if only in (None, "rules"):
        checked = rule_tests(tools, folder, problems)
        print("passes: %d functions of edge numbers: every operation, constants on either side" % checked)
    if only in (None, "random") and total > 0:
        made, files, natives, smaller = random_tests(tools, total, seed, folder, problems)
        print("passes: %d random functions in %d files the same before and after (%d runs also as native code); %d files became smaller" % (made, files, natives, smaller))
    if only in (None, "programs"):
        compared = program_tests(azol, tools, folder, problems)
        print("passes: %d whole programs through the passes: interpreted, and built by azol-opt (also with three registers), like their default builds" % compared)
    if only in (None, "compiler"):
        if compiler_test(azol, tools, folder, problems):
            print("passes: the compiler, optimised and built by azol-opt, builds examples/hello.azl and itself as azol does")


def main():
    argv = sys.argv[1:]
    azol = base.find_azol(argv)
    total = int(argv[argv.index("--random") + 1]) if "--random" in argv else 3000
    seed = int(argv[argv.index("--seed") + 1]) if "--seed" in argv else 20261112
    only = argv[argv.index("--only") + 1] if "--only" in argv else None
    folder = tempfile.mkdtemp(prefix="azol-pass-")
    problems = []
    run(azol, folder, total, seed, problems, only)
    for problem in problems:
        print("FAIL " + problem)
    if problems:
        print("%d FAILED; files are in %s" % (len(problems), folder))
        sys.exit(1)
    if "--keep" not in argv:
        shutil.rmtree(folder, ignore_errors=True)
    print("ALL PASS TESTS PASSED")


if __name__ == "__main__":
    main()
