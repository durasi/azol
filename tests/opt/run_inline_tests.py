#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Tests of the passes `inline`, `tailcall` and `accrec`
(compiler/opt/pass_inline.azl, pass_tailcall.azl, pass_accrec.azl).

    python3 tests/opt/run_inline_tests.py [--azol PATH] [--random N] [--seed N] [--only PART] [--keep]

1. The hand-written cases of tests/opt/inline, tailcall and accrec.
   A case is a file in the intermediate form whose header says what it
   shows and what it does:

       ; one line that says what the case shows
       ; calls: N          calls of functions left after the pass
       ; text: WORDS       the result holds these words
       ; text: no WORDS    the result does not
       ; run: main 5       as in tests/opt/run_*.ir, with `; out:`,
       ; out: => 88        `; err:` and `; exit:`

   The file must do what its header says before the pass, after the
   pass alone, and after the whole order of `-O` run to a fixed point;
   the pass must leave its own result alone; as native code the file
   must do the same before and after. A case whose name starts with
   `no_` is one the pass must not touch: its text stays what it was.

2. Random functions that call themselves, in every shape the two
   passes know and in shapes they must leave alone (an operation that
   is not associative, something printed after the call, a result used
   twice), with tests that show an end and tests that do not, up to
   3,000 calls deep: the same before and after `tailcall,accrec`, and
   after the whole order, interpreted and as native code.

3. The random modules of the other scripts, which call each other,
   through `inline` alone and through `tailcall,accrec` alone.

4. The programs of tests/opt/inline, tailcall and accrec (`.azl`, with
   `// expect-` comments): recursions without end that must still stop
   with `azol: stack overflow`, and recursions that end, millions of
   calls deep. Built by `azol-opt`, run as native code (also with
   three registers) and in the interpreter.

5. A sample for ARM64: programs of the examples and of tests/programs
   with calls, recursion and stops, optimised, built for macOS and
   Linux and run in the checking emulators like their default builds
   (skipped without the emulators).

Every program of tests/programs and of the examples through the whole
order (interpreted, native, three registers), and the compiler built
by `azol-opt` building itself, are parts 4 and 5 of
tests/opt/run_pass_tests.py: the three passes are in the order they
run.
"""

import concurrent.futures
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import run_opt_tests as base
import run_pass_tests as passes

HERE = base.HERE
ROOT = base.ROOT
PARTS = ["inline", "tailcall", "accrec"]
DEEP = ["sh", "-c", 'ulimit -s unlimited 2>/dev/null; exec "$@"', "sh"]


def calls_in(text):
    return len(re.findall(r"(?m)^\s+(?:[^\n=]*= )?call ", text))


# ---- 1. the hand-written cases ---------------------------------------------------


def header(path):
    wants = {"calls": None, "text": [], "first": ""}
    with open(path, encoding="utf-8") as handle:
        for at, line in enumerate(handle):
            if not line.startswith(";"):
                break
            if at == 0:
                wants["first"] = line
            body = line[1:].strip()
            if body.startswith("calls:"):
                wants["calls"] = int(body[6:])
            elif body.startswith("text:"):
                wants["text"].append(body[5:].strip())
    return wants


def run_rows(tools, path, cases, label, problems):
    for case in cases:
        if case["kind"] != "run":
            continue
        code, out, err = base.tool(tools["ir"], "run", path, *case["arg"].split())
        want_exit = case["exit"] or 0
        if code != want_exit or base.lines(out) != case["out"] or base.lines(err) != case["err"]:
            problems.append("%s: run %s: wanted exit %d, out %r, err %r; got exit %r, out %r, err %r" % (
                label, case["arg"], want_exit, case["out"], case["err"], code, base.lines(out), base.lines(err)))


def case_tests(tools, folder, problems):
    count = 0
    kept = 0
    natives = 0
    for part in PARTS:
        cases_dir = os.path.join(HERE, part)
        for name in sorted(os.listdir(cases_dir)):
            if not name.endswith(".ir"):
                continue
            count += 1
            path = os.path.join(cases_dir, name)
            label = "case %s/%s" % (part, name)
            wants = header(path)
            if len(wants["first"]) < 12:
                problems.append("%s: the first line must be a comment that says what the case shows" % label)
            if wants["calls"] is None:
                problems.append("%s: no `; calls: N` in its header" % label)
                continue
            cases = base.read_cases(path)
            if not any(case["kind"] == "run" for case in cases):
                problems.append("%s: no `; run:` in its header" % label)
            before = len(problems)
            base.check_file(tools["ir"], path, problems)
            if len(problems) > before:
                continue
            after = os.path.join(folder, "%s-%s" % (part, name))
            code, out, err = base.tool(tools["opt"], path, "-o", after, "--passes", part, "--verify-each")
            if code != 0:
                problems.append("%s: the pass fails (exit %r, %r)" % (label, code, base.lines(err)[:3]))
                continue
            with open(after, encoding="utf-8") as handle:
                text = handle.read()
            if calls_in(text) != wants["calls"]:
                problems.append("%s: %d calls are left, not %d\n%s" % (label, calls_in(text), wants["calls"], text))
            for words in wants["text"]:
                if words.startswith("no "):
                    if words[3:] in text:
                        problems.append("%s: the result still holds `%s`\n%s" % (label, words[3:], text))
                elif words not in text:
                    problems.append("%s: the result does not hold `%s`\n%s" % (label, words, text))
            if name.startswith("no_"):
                kept += 1
                plain = base.tool(tools["opt"], path, "--passes", "")
                if plain[0] != 0 or plain[1].decode() != text:
                    problems.append("%s: the pass must leave this alone, and it gives\n%s" % (label, text))
            again = base.tool(tools["opt"], after, "--passes", part)
            if again[0] != 0 or again[1].decode() != text:
                problems.append("%s: the pass changes its own result" % label)
            run_rows(tools, after, cases, label + " after the pass", problems)
            whole = os.path.join(folder, "%s-whole-%s" % (part, name))
            code, out, err = base.tool(tools["opt"], path, "-o", whole, "--fixpoint", "--verify-each")
            if code != 0:
                problems.append("%s: the whole order fails (exit %r, %r)" % (label, code, base.lines(err)[:3]))
                continue
            run_rows(tools, whole, cases, label + " after the whole order", problems)
            if passes.native():
                for case in cases:
                    if case["kind"] != "run" or not case["arg"]:
                        continue
                    row = case["arg"].split()
                    one = passes.run_native(tools, path, folder, "ia", row[0], row[1:])
                    for other in (after, whole):
                        two = passes.run_native(tools, other, folder, "ib", row[0], row[1:])
                        natives += 1
                        if one != two:
                            problems.append("%s: as native code `%s` gives %r before and %r in %s" % (label, case["arg"], one, two, other))
    return count, kept, natives


# ---- 2. random functions that call themselves ------------------------------------------

FOLD = ["add", "mul", "and", "or", "xor"]
OTHER = ["sub", "shl", "sar", "sdiv", "smulh"]
MIX = ["add", "sub", "mul", "xor", "and", "or"]


def recursive_function(rng):
    """One function `f(n, bound, value)` that calls itself, and how to
    call it so that it ends: (text, rows of arguments)."""
    shape = rng.choice(["tail", "tail", "fold", "fold", "fold", "fold", "both", "two", "after", "twice", "nothing"])
    up = rng.random() < 0.5
    step = rng.choice([1, 1, 1, 2, 3])
    fixed_bound = rng.random() < 0.5
    limit = rng.randint(-40, 40)
    bound = str(limit) if fixed_bound else "p1"
    # The test that leaves the recursion, and whether it is taken when true.
    exact = rng.random() < 0.3
    if exact:
        step = 1
        test, leave_when = rng.choice([("eq", True), ("ne", False)])
        operands = "p0, %s" % bound if rng.random() < 0.7 else "%s, p0" % bound
    else:
        if up:
            test, leave_when = rng.choice([("sge", True), ("sgt", True), ("slt", False), ("sle", False)])
        else:
            test, leave_when = rng.choice([("sle", True), ("slt", True), ("sgt", False), ("sge", False)])
        operands = "p0, %s" % bound
        if rng.random() < 0.3:
            # The same test with the operands the other way round.
            test = {"sge": "sle", "sgt": "slt", "slt": "sgt", "sle": "sge"}[test]
            operands = "%s, p0" % bound
    move = step if up else -step
    op = rng.choice(FOLD) if rng.random() < 0.75 else rng.choice(OTHER)
    lines = ["fn f(p0: i64, p1: i64, p2: i64) -> i64 {", "b0:", "    v0 = %s %s" % (test, operands)]
    lines.append("    br v0, b1, b2" if leave_when else "    br v0, b2, b1")
    lines += ["b1:", "    v1 = %s p2, %d" % (rng.choice(MIX), rng.randint(-3, 9)), "    ret v1", "b2:"]
    if rng.random() < 0.2:
        lines.append("    callrt print_i64(p0)")
    lines.append("    v2 = add p0, %d" % move)
    lines.append("    v3 = %s p2, %s" % (rng.choice(MIX), rng.choice(["p0", "p1", "3", "-7"])))
    lines.append("    v4 = %s p0, %s" % (rng.choice(MIX), rng.choice(["p2", "5", "-1"])))
    call = "    v5 = call f(v2, p1, v3) -> i64"
    left = rng.random() < 0.5
    fold = "    v6 = %s v5, v4" % op if left else "    v6 = %s v4, v5" % op
    if shape == "tail":
        lines += [call, "    ret v5"]
    elif shape == "fold":
        lines += [call, fold, "    ret v6"]
    elif shape == "both":
        lines += ["    v7 = and v4, 1", "    v8 = eq v7, 0", "    br v8, b3, b4", "b3:", call, "    ret v5", "b4:",
                  "    v9 = call f(v2, p1, v4) -> i64", "    v6 = %s v9, v3" % op, "    ret v6"]
    elif shape == "two":
        lines += [call, "    v7 = add p0, %d" % (2 * move), "    v8 = call f(v7, p1, v4) -> i64", "    v6 = %s v5, v8" % op, "    ret v6"]
    elif shape == "after":
        lines += [call, "    callrt print_i64(v4)", fold, "    ret v6"]
    elif shape == "twice":
        lines += [call, "    v6 = %s v5, v5" % op, "    ret v6"]
    else:
        lines += [call, fold, "    v7 = add v6, 1", "    ret v7"]
    lines.append("}")
    text = "\n".join(lines) + "\n\nfn main(p0: i64, p1: i64, p2: i64) -> i64 {\nb0:\n    v0 = call f(p0, p1, p2) -> i64\n    ret v0\n}\n"
    rows = []
    for _ in range(3):
        deepest = 14 if shape == "two" else rng.choice([0, 1, 5, 40, 255, 256, 257, 700, 3000])
        depth = rng.randint(0, deepest)
        edge = limit if fixed_bound else rng.randint(-1000, 1000)
        # Where to start so that the test is met after `depth` calls.
        start = edge - move * depth
        if not exact and rng.random() < 0.5:
            start -= rng.randint(0, step - 1) * (1 if up else -1)
        rows.append([str(start), str(edge), str(rng.choice([0, 1, -1, 7, 1 << 40, rng.randint(-99, 99)]))])
    return text, rows


def recursive_tests(tools, total, seed, folder, problems):
    rng = random.Random(seed)
    made = 0
    changed = 0
    counted = 0
    natives = 0
    path = os.path.join(folder, "recursive.ir")
    while made < total and len(problems) < 5:
        text, rows = recursive_function(rng)
        made += 1
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        code, out, err = base.tool(tools["ir"], "verify", path)
        if code != 0:
            problems.append("recursive function %d (seed %d): the generator made a wrong function: %r\n%s" % (made, seed, base.lines(err), text))
            break
        results = []
        for label, more in (("tailcall,accrec", ["--passes", "tailcall,accrec"]), ("the whole order", ["--fixpoint"])):
            after = os.path.join(folder, "recursive-%d.ir" % len(results))
            code, out, err = base.tool(tools["opt"], path, "-o", after, "--verify-each", *more)
            if code != 0:
                problems.append("recursive function %d (seed %d): %s fails (exit %r, %r)\n%s" % (made, seed, label, code, base.lines(err)[:3], text))
                break
            results.append((label, after))
        if len(results) != 2:
            continue
        with open(results[0][1], encoding="utf-8") as handle:
            first = handle.read()
        if calls_in(first) < calls_in(text) or " ne " in first and " ne " not in text or "jump b0" in first:
            changed += 1
        if "= ne v" in first and "= ne v" not in text:
            counted += 1
        for row in rows:
            before = base.tool(tools["ir"], "run", path, "main", *row)
            if before[0] not in (0, 1):
                # Deeper than the interpreter goes, for instance.
                continue
            for label, after in results:
                got = base.tool(tools["ir"], "run", after, "main", *row)
                if got != before:
                    problems.append("recursive function %d (seed %d): `main %s` gives %r, and after %s %r\n%s" % (made, seed, " ".join(row), before, label, got, text))
            if passes.native() and made % 4 == 0:
                one = passes.run_native(tools, path, folder, "ra", "main", row)
                two = passes.run_native(tools, results[1][1], folder, "rb", "main", row)
                natives += 1
                if one != two:
                    problems.append("recursive function %d (seed %d): as native code `main %s` gives %r before and %r after\n%s" % (made, seed, " ".join(row), one, two, text))
    return made, changed, counted, natives


# ---- 3. the random modules of the other scripts ----------------------------------------


def module_tests(tools, total, seed, folder, problems):
    import run_back_tests as back_tests
    rng = random.Random(seed)
    made = 0
    files = 0
    fewer = 0
    while made < total and len(problems) < 5:
        if files % 2 == 0:
            count = rng.randint(2, 10)
            text = base.random_module(rng, count)[0]
            arguments = []
        else:
            text, arguments, count = back_tests.random_back_module(rng)
        made += count
        files += 1
        path = os.path.join(folder, "module%d.ir" % files)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        before = base.tool(tools["ir"], "run", path, "main", *[str(x) for x in arguments])
        for order in ("inline", "tailcall,accrec", "inline,tailcall,accrec,inline"):
            after = os.path.join(folder, "module%d.after.ir" % files)
            code, out, err = base.tool(tools["opt"], path, "-o", after, "--passes", order, "--verify-each", timeout=300)
            if code != 0:
                problems.append("random module %d (seed %d): `%s` fails (exit %r, %r); kept as %s" % (files, seed, order, code, base.lines(err)[:3], path))
                break
            result = base.tool(tools["ir"], "run", after, "main", *[str(x) for x in arguments])
            if result != before:
                problems.append("random module %d (seed %d): `main` gives %r, and after `%s` %r; kept as %s" % (files, seed, before, order, result, path))
                break
            if order == "inline":
                with open(after, encoding="utf-8") as handle:
                    if calls_in(handle.read()) < calls_in(text):
                        fewer += 1
        else:
            os.unlink(path)
    return made, files, fewer


# ---- 4. programs: recursions with and without end --------------------------------------


def expectation(path):
    want = {"exit": 0, "lines": [], "stderr": []}
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            if line.startswith("// expect-exit:"):
                want["exit"] = int(line.split(":", 1)[1])
            elif line.startswith("// expect-line:"):
                want["lines"].append(line.split(":", 1)[1].strip())
            elif line.startswith("// expect-stderr:"):
                want["stderr"].append(line.split(":", 1)[1].strip())
    return want


def programs():
    found = []
    for part in PARTS:
        for name in sorted(os.listdir(os.path.join(HERE, part))):
            if name.endswith(".azl"):
                found.append(os.path.join(HERE, part, name))
    return found


def program_tests(tools, folder, problems):
    runs = 0
    for path in programs():
        label = "program %s" % os.path.relpath(path, HERE)
        want = expectation(path)
        commands = [("interpreted", DEEP + [tools["build"], "run", path, "--fixpoint", "--verify-each"])]
        if passes.native():
            for kind, more in (("native", []), ("native, three registers", ["--stress"])):
                exe = os.path.join(folder, "program" + ("-stress" if more else ""))
                built = subprocess.run([tools["build"], "build", path, "-o", exe, "--fixpoint", "--check", *more], capture_output=True)
                if built.returncode != 0:
                    problems.append("%s: `azol-opt build %s` fails: %r" % (label, " ".join(more), built.stderr[-300:]))
                    continue
                commands.append((kind, [exe]))
        for kind, command in commands:
            try:
                ran = subprocess.run(command, capture_output=True, timeout=600)
            except subprocess.TimeoutExpired:
                problems.append("%s (%s): no answer in time" % (label, kind))
                continue
            runs += 1
            got = (ran.returncode, base.lines(ran.stdout), base.lines(ran.stderr))
            if got != (want["exit"], want["lines"], want["stderr"]):
                problems.append("%s (%s): exit %r, %r, %r; wanted exit %r, %r, %r" % (label, kind, got[0], got[1][-3:], got[2][-3:], want["exit"], want["lines"], want["stderr"]))
    return len(programs()), runs


# ---- 5. ARM64 ---------------------------------------------------------------------------

ARM_SAMPLE = ["examples/hello.azl", "examples/fib.azl", "examples/tree.azl", "examples/structs.azl", "examples/bank.azl", "examples/lists.azl",
              "tests/programs/functions.azl", "tests/programs/mut_params.azl", "tests/programs/deep_recursion.azl", "tests/programs/struct_stress.azl",
              "tests/programs/lean_trap_callee.azl", "tests/programs/stack_overflow.azl", "tests/programs/try_operator.azl", "tests/programs/matching.azl"]


def arm_one(azol, tools, back, work, at, path):
    import run_back_arm64_tests as arm
    found = []
    compared = 0
    text = os.path.join(work, "arm%d.ir" % at)
    scrap = os.path.join(work, "arm%d.x64" % at)
    built = subprocess.run([tools["build"], "build", path, "-o", scrap, "--fixpoint", "--emit-ir", text], capture_output=True)
    if built.returncode != 0:
        return 0, ["ARM64 %s: `azol-opt build --emit-ir` fails: %r" % (path, built.stderr[-200:])]
    for target in arm.TARGETS:
        plain = os.path.join(work, "arm%d-plain" % at)
        if subprocess.run([azol, "build", "--target", target, path, "-o", plain], capture_output=True).returncode != 0:
            return 0, ["ARM64 %s: `azol build --target %s` fails" % (path, target)]
        want = arm.run_exe(target, plain, [plain])
        for mode in arm.MODES:
            label = "ARM64 %s for %s%s" % (os.path.relpath(path, ROOT), target, " (three registers)" if mode else "")
            exe = os.path.join(work, "arm%d-opt" % at)
            done = subprocess.run([back, "build", text, "-o", exe, "--target", target, "--check", "--program"] + mode, capture_output=True)
            if done.returncode != 0:
                found.append("%s: the back end refuses it: %r" % (label, done.stderr[-200:]))
                continue
            got = arm.run_exe(target, exe, [plain])
            if got[4] and got[4] != want[4]:
                found.append("%s: %s" % (label, got[4]))
            elif got[:3] != want[:3]:
                found.append("%s: exit %r (default build %r), output %r (default build %r), errors %r (default build %r)" % (
                    label, got[0], want[0], got[1][-80:], want[1][-80:], got[2][-80:], want[2][-80:]))
            else:
                compared += 1
    return compared, found


def arm_tests(azol, tools, folder, problems):
    import run_back_arm64_tests as arm
    import run_back_tests as x64
    if arm.emulators() is None:
        return None
    back = os.path.join(folder, "azol-back-arm64-inline")
    base.build(azol, x64.BACK_SOURCES, back)
    work = os.path.join(folder, "arm64-inline")
    os.makedirs(work, exist_ok=True)
    # Not the programs of part 4: the emulators would take them through
    # millions of rounds. `stack_overflow.azl` and `deep_recursion.azl`
    # of tests/programs stand for them.
    sample = [os.path.join(ROOT, name) for name in ARM_SAMPLE]
    compared = 0
    with concurrent.futures.ProcessPoolExecutor(max_workers=2) as pool:
        jobs = [pool.submit(arm_one, azol, tools, back, work, at, path) for at, path in enumerate(sample)]
        for job in jobs:
            done, found = job.result()
            compared += done
            problems.extend(found)
    return len(sample), compared


def run(azol, folder, total, seed, problems, only=None, tools=None):
    if tools is None:
        tools = passes.build_tools(azol, folder)
    if only in (None, "cases"):
        count, kept, natives = case_tests(tools, folder, problems)
        print("inline, tailcall, accrec: %d hand-written cases, %d of them where a pass must do nothing (%d runs also as native code)" % (count, kept, natives))
    if only in (None, "recursive") and total > 0:
        made, changed, counted, natives = recursive_tests(tools, total, seed, folder, problems)
        print("inline, tailcall, accrec: %d random functions that call themselves the same before and after (%d became loops, %d of those counted; %d runs also as native code)" % (made, changed, counted, natives))
    if only in (None, "modules") and total > 0:
        made, files, fewer = module_tests(tools, total, seed, folder, problems)
        print("inline, tailcall, accrec: %d random functions in %d modules the same after `inline`, after `tailcall,accrec` and after both (%d modules lost calls)" % (made, files, fewer))
    if only in (None, "programs"):
        count, runs = program_tests(tools, folder, problems)
        print("inline, tailcall, accrec: %d programs with recursions that end and that do not: %d runs, interpreted and native, do what their comments say" % (count, runs))
    if only in (None, "arm64"):
        done = arm_tests(azol, tools, folder, problems)
        if done is None:
            print("inline, tailcall, accrec: ARM64 skipped (needs the checking emulators)")
        else:
            print("inline, tailcall, accrec: %d optimised programs built for ARM64, %d executables run in the emulators like their default builds (macOS and Linux, all registers and three)" % done)


def main():
    argv = sys.argv[1:]
    azol = base.find_azol(argv)
    total = int(argv[argv.index("--random") + 1]) if "--random" in argv else 1500
    seed = int(argv[argv.index("--seed") + 1]) if "--seed" in argv else 20261112
    only = argv[argv.index("--only") + 1] if "--only" in argv else None
    folder = tempfile.mkdtemp(prefix="azol-inline-")
    problems = []
    run(azol, folder, total, seed, problems, only)
    for problem in problems:
        print("FAIL " + problem)
    if problems:
        print("%d FAILED; files are in %s" % (len(problems), folder))
        sys.exit(1)
    if "--keep" not in argv:
        shutil.rmtree(folder, ignore_errors=True)
    print("ALL INLINE TESTS PASSED")


if __name__ == "__main__":
    main()
