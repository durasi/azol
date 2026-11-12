#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Tests of the optimising back end for ARM64 (compiler/opt/sel_arm64.azl,
emit_arm64.azl, back_arm64.azl), for both of its targets, aarch64-macos
and aarch64-linux, on a machine that is neither: every program runs in
an emulator.

    python3 tests/opt/run_back_arm64_tests.py [--azol PATH] [--random N] [--seed S]
                                              [--kernels] [--keep]

It builds `azol-back` and `ir-tool` and then:

1. Every `; run:` case of every file here goes through `azol-back build
   --target T --check` for both targets, with all registers and with
   three (`--stress`), and through the interpreter. The program must
   print the same results, write the same to standard error and exit
   with the same code. `--check` verifies every register allocation.

   Two functions made here are large enough for their conditional
   jumps not to reach (more than 32 KiB for `tbz`, more than 1 MiB for
   `b.cond`), so that the longer forms are run too.

2. Random modules (the generator of run_back_tests.py), N functions in
   all (30000 when not given, about ten minutes), compared in the same
   way on both targets; every fourth module also with three registers.

3. Every program of tests/programs and examples that `azol build`
   accepts goes through `azol-lower emit` and `azol-back build
   --program`, for both targets, with all registers and with three, and
   must behave as the executable `azol build --target T` makes of it:
   the same standard output, standard error and exit code.

4. The compiler itself, lowered and built by this back end for each
   target, runs in compiler/emu_arm64.py and builds examples/hello.azl
   and itself; what it writes must be, byte for byte, what `azol build
   --target T` writes. The same once more for macOS with the pass
   `sroa` between lowering and the back end.

5. With `--kernels` only, because it takes half an hour: the four
   benchmark kernels (tests/opt/kernels) built for both targets print
   their benchmarks' numbers, and for macOS the instructions they run
   are counted, next to those of `azol build --target aarch64-macos` of
   the benchmark itself.

The emulators of the first three parts are not in the repository: they
are the checking ones the ARM64 code generator was developed with,
`emu.py` (macOS: a Mach-O loader, the system library's functions with a
heap that reports every access outside a live block, and a check that
x19 to x28 and the stack pointer come back as they were) and
`emu_linux_arm64.py`, looked for in the folder $AZOL_REF
(/home/claude/ref when not set). They need the Python package
`unicorn`. Where they are missing, everything here is skipped with a
note; on a Mac, tests/opt/mac_check.sh runs the real thing instead.
Parts 4 and 5 use the fast emulator of the repository,
compiler/emu_arm64.py, which checks nothing but runs and counts the
same instructions.

run_opt_tests.py runs parts 1 to 4 at its end, with its own number of
random functions.
"""
import concurrent.futures
import os
import random
import shutil
import subprocess
import sys
import tempfile

import run_opt_tests as base
import run_back_tests as x64

HERE = base.HERE
ROOT = base.ROOT
REF = os.environ.get("AZOL_REF", "/home/claude/ref")
TARGETS = ("aarch64-macos", "aarch64-linux")
MODES = ([], ["--stress"])
LIMIT = 2_000_000_000


def emulators():
    """The checking emulators, or None where they cannot be used."""
    if not os.path.exists(os.path.join(REF, "emu.py")):
        return None
    if REF not in sys.path:
        sys.path.insert(0, REF)
    try:
        import emu
        import emu_linux_arm64
    except Exception:
        return None
    return emu, emu_linux_arm64


def run_exe(target, exe, argv=None, limit=LIMIT):
    """Runs an executable for `target` in its emulator. Gives (exit code,
    standard output, standard error, leaks, complaint): the complaint is
    what the emulator found wrong, or None."""
    emu, emu_linux = emulators()
    try:
        if target == "aarch64-macos":
            out, code, trap, balanced, err, heap = emu.run(exe, limit, argv or [exe])
        else:
            out, code, trap, balanced, err, heap = emu_linux.run(exe, limit, argv or [exe], ROOT)
    except Exception as problem:
        return None, b"", b"", 0, "the emulator gave up: %r" % (problem,)
    complaint = None
    if trap is not None:
        complaint = "it stopped with %r" % (trap,)
    elif not balanced:
        complaint = "a saved register or the stack pointer came back changed"
    elif heap["violations"]:
        complaint = "memory was used wrongly: %r" % (heap["violations"][:3],)
    return code, out.encode(), err.encode(), heap["leaks"], complaint


def compare(ir_tool, back, path, arguments, exe, problems, modes=MODES):
    """Builds one case for both targets in the given modes and runs it.
    Gives how many programs were compared (0 when the interpreter stopped
    the case itself)."""
    code, out, err = base.tool(ir_tool, "run", path, *arguments)
    if code is None or code == 3:
        return 0
    name = os.path.basename(path) + " " + " ".join(arguments)
    want = (code, x64.expected_lines(code, out), err)
    done = 0
    for target in TARGETS:
        for mode in modes:
            command = [back, "build", path, "-o", exe, "--target", target, "--check"] + mode
            if arguments:
                command += ["--run"] + list(arguments)
            built = subprocess.run(command, capture_output=True)
            label = "%s for %s%s" % (name, target, " (three registers)" if mode else "")
            if built.returncode != 0:
                problems.append("%s: azol-back: %s" % (label, built.stderr.decode(errors="replace").strip()[:400]))
                continue
            got_code, got_out, got_err, _, complaint = run_exe(target, exe)
            got = (got_code, base.lines(got_out), got_err)
            if complaint:
                problems.append("%s: %s" % (label, complaint))
            elif got != want:
                problems.append("%s: the program gives %r, the interpreter %r" % (label, got, want))
            done += 1
    return done


def file_tests(ir_tool, back, folder, problems):
    exe = os.path.join(folder, "case-arm64")
    cases = 0
    programs = 0
    for name in sorted(os.listdir(HERE)):
        if not name.endswith(".ir"):
            continue
        path = os.path.join(HERE, name)
        for case in base.read_cases(path):
            if case["kind"] != "run":
                continue
            done = compare(ir_tool, back, path, case["arg"].split(), exe, problems)
            if done:
                cases += 1
                programs += done
    return cases, programs


def far_module(count):
    """The text of a function with two conditional jumps over `count`
    instructions each: one on a bit, one on a comparison."""
    lines = ["fn main(p0: i64, p1: i64) -> i64, i64 {", "b0:", "    v0 = and p0, 4", "    v1 = eq v0, 0", "    br v1, b1, b2(p1)", "b1:"]
    last = "p1"
    value = 2
    for k in range(count):
        lines.append("    v%d = %s %s, %s" % (value, "xor" if k % 2 else "add", last, "p0" if k % 2 else str(k % 7 + 1)))
        last = "v%d" % value
        value += 1
    first = value
    lines += ["    jump b2(%s)" % last, "b2(v%d: i64):" % first, "    v%d = slt p0, p1" % (first + 1), "    br v%d, b3, b4(v%d)" % (first + 1, first), "b3:"]
    last = "v%d" % first
    value += 2
    for k in range(count):
        lines.append("    v%d = %s %s, %s" % (value, "xor" if k % 2 else "sub", last, "p1" if k % 2 else str(k % 5 + 1)))
        last = "v%d" % value
        value += 1
    lines += ["    jump b4(%s)" % last, "b4(v%d: i64):" % value, "    ret v%d, v%d" % (first, value), "}"]
    return "\n".join(lines) + "\n"


def far_tests(ir_tool, back, folder, problems):
    """Functions too large for the short conditional jumps. Gives how
    many programs were compared."""
    exe = os.path.join(folder, "far-arm64")
    programs = 0
    for count in (9000, 270000):
        path = os.path.join(folder, "far_%d.ir" % count)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(far_module(count))
        for arguments in ("0 5", "4 5", "9 2", "12 1"):
            programs += compare(ir_tool, back, path, ["main"] + arguments.split(), exe, problems, MODES[:1])
    return programs


def random_tests(ir_tool, back, total, seed, folder, problems):
    """Random modules until `total` functions were compared. Gives
    (functions, modules, programs, programs with three registers)."""
    rng = random.Random(seed)
    made = 0
    files = 0
    compared = 0
    stressed = 0
    exe = os.path.join(folder, "random-arm64")
    while made < total and len(problems) < 10:
        text, arguments, count = x64.random_back_module(rng)
        path = os.path.join(folder, "arm64_%05d.ir" % files)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        stress = files % 4 == 0
        before = len(problems)
        done = compare(ir_tool, back, path, ["main"] + arguments, exe, problems, MODES if stress else MODES[:1])
        if done:
            compared += done
            made += count
            if stress:
                stressed += done // 2
        files += 1
        if len(problems) == before:
            os.unlink(path)
    return made, files, compared, stressed


def whole_program(azol, back, lower, work, where, name):
    """One program on both targets. Gives (programs compared, problems)."""
    found = []
    compared = 0
    path = os.path.join(ROOT, where, name)
    stem = name[:-4]
    text = os.path.join(work, stem + ".ir")
    emitted = None
    for target in TARGETS:
        plain = os.path.join(work, "plain-" + stem)
        if subprocess.run([azol, "build", "--target", target, path, "-o", plain], capture_output=True).returncode != 0:
            return 0, found
        if emitted is None:
            emitted = subprocess.run([lower, "emit", path], capture_output=True)
            if emitted.returncode != 0:
                return 0, ["%s/%s: azol-lower refuses it: %s" % (where, name, emitted.stderr.decode(errors="replace").strip()[:200])]
            with open(text, "wb") as handle:
                handle.write(emitted.stdout)
        want = run_exe(target, plain, [plain])
        for mode in MODES:
            label = "%s/%s for %s%s" % (where, name, target, " (three registers)" if mode else "")
            exe = os.path.join(work, "opt-" + stem)
            built = subprocess.run([back, "build", text, "-o", exe, "--target", target, "--check", "--program"] + mode, capture_output=True)
            if built.returncode != 0:
                found.append("%s: the back end refuses it: %s" % (label, built.stderr.decode(errors="replace").strip()[:200]))
                continue
            got = run_exe(target, exe, [plain])
            if got[4] and got[4] != want[4]:
                found.append("%s: %s" % (label, got[4]))
            elif got[:3] != want[:3]:
                found.append("%s: exit %r (default mode %r), output %r (default mode %r), errors %r (default mode %r)" % (
                    label, got[0], want[0], got[1][-80:], want[1][-80:], got[2][-80:], want[2][-80:]))
            elif got[3] != want[3]:
                found.append("%s: %d blocks are never freed (default mode %d)" % (label, got[3], want[3]))
            else:
                compared += 1
    return compared, found


def build_lower(azol, folder, problems):
    """`azol-lower`, or None where the lowering stage is not in the tree."""
    front = getattr(base, "FRONT", None)
    lower = getattr(base, "LOWER", None)
    opt = os.path.join(ROOT, "compiler", "opt")
    if front is None or lower is None or not os.path.exists(os.path.join(opt, "lower_tool_main.azl")):
        return None
    tool = os.path.join(folder, "azol-lower-for-arm64")
    sources = [os.path.join(ROOT, "compiler", name) for name in front] + [os.path.join(opt, name) for name in lower]
    done = subprocess.run([azol, "build"] + sources + ["-o", tool], capture_output=True)
    if done.returncode != 0:
        problems.append("azol-lower does not build: " + done.stderr.decode(errors="replace").strip()[:300])
        return None
    return tool


def whole_program_tests(azol, back, lower, folder, problems):
    """Every program of tests/programs and examples. Gives (programs
    built by the back end and compared, source programs)."""
    work = os.path.join(folder, "whole-arm64")
    os.makedirs(work, exist_ok=True)
    jobs = []
    compared = 0
    sources = 0
    with concurrent.futures.ProcessPoolExecutor(max_workers=2) as pool:
        for where in ("tests/programs", "examples"):
            for name in sorted(os.listdir(os.path.join(ROOT, where))):
                if name.endswith(".azl"):
                    jobs.append(pool.submit(whole_program, azol, back, lower, work, where, name))
        for job in jobs:
            done, found = job.result()
            compared += done
            sources += 1 if done else 0
            problems.extend(found)
    return compared, sources


def fast_run(path, counted):
    """Runs an executable in compiler/emu_arm64.py. Gives (instructions
    run, or 0 when not `counted`; standard output; exit code; what went
    wrong, or None)."""
    folder = os.path.join(ROOT, "compiler")
    if folder not in sys.path:
        sys.path.insert(0, folder)
    import emu_arm64
    from unicorn import UC_HOOK_BLOCK
    ran = [0]
    plain = emu_arm64.Uc

    def counting(*arguments, **named):
        mu = plain(*arguments, **named)
        mu.hook_add(UC_HOOK_BLOCK, lambda uc, address, size, _: ran.__setitem__(0, ran[0] + size // 4))
        return mu

    if counted:
        emu_arm64.Uc = counting
    try:
        out, err, code, problem = emu_arm64.run(path, (), 7200)
    finally:
        emu_arm64.Uc = plain
    return ran[0], out, code, problem


def kernel_tests(azol, back, folder, problems):
    """The four kernels. Gives the lines of a table: for each, the
    instructions the default build of the benchmark and the kernel built
    by the back end run on macOS."""
    table = []
    for name, want in sorted(x64.KERNELS.items()):
        stem = name[:-3]
        plain = os.path.join(folder, "kernel-plain-" + stem)
        source = os.path.join(ROOT, "bench", stem + ".azl")
        counts = []
        if os.path.exists(source) and subprocess.run([azol, "build", "--target", "aarch64-macos", source, "-o", plain], capture_output=True).returncode == 0:
            ran, out, code, problem = fast_run(plain, True)
            if out.decode().strip() != want or code != 0:
                problems.append("kernel %s: the default build of the benchmark prints %r" % (name, out[-80:]))
            counts.append(ran)
        for target in TARGETS:
            exe = os.path.join(folder, "kernel-" + stem)
            done = subprocess.run([back, "build", os.path.join(HERE, "kernels", name), "-o", exe, "--target", target, "--check", "--program"], capture_output=True)
            if done.returncode != 0:
                problems.append("kernel %s for %s does not build: %s" % (name, target, done.stderr.decode(errors="replace").strip()))
                continue
            ran, out, code, problem = fast_run(exe, target == "aarch64-macos")
            if target == "aarch64-macos":
                counts.append(ran)
            if problem or code != 0 or out.decode().strip() != want:
                problems.append("kernel %s for %s: exit %r, printed %r, wanted %r; %s" % (name, target, code, out[-80:], want, problem or ""))
        if len(counts) == 2:
            table.append("    %-6s %9.1f M default, %9.1f M this back end (%.2f)" % (stem, counts[0] / 1e6, counts[1] / 1e6, counts[1] / counts[0]))
    return table


def build_sroa(azol, folder):
    """`sroa-tool`, or None where the pass is not in the tree or does
    not build."""
    opt = os.path.join(ROOT, "compiler", "opt")
    if not os.path.exists(os.path.join(opt, "sroa_tool_main.azl")):
        return None
    tool = os.path.join(folder, "sroa-tool-for-arm64")
    names = base.LOWER[:-1] + ["pass_sroa.azl", "sroa_tool_main.azl"]
    sources = [os.path.join(ROOT, "compiler", name) for name in base.FRONT] + [os.path.join(opt, name) for name in names]
    if subprocess.run([azol, "build"] + sources + ["-o", tool], capture_output=True).returncode != 0:
        return None
    return tool


def compiler_tests(azol, back, lower, folder, problems):
    """The compiler, lowered and built by the back end for each target,
    in the fast emulator: it must build examples/hello.azl and itself as
    `azol` does. Once more for macOS with the pass `sroa` after
    lowering, where that pass is in the tree. Gives how many builds
    were compared."""
    emulator = os.path.join(ROOT, "compiler", "emu_arm64.py")
    sroa = build_sroa(azol, folder)
    with open(os.path.join(ROOT, "compiler", "SOURCES"), encoding="utf-8") as handle:
        common = [os.path.join(ROOT, "compiler", line.strip()) for line in handle if line.strip() and not line.startswith("#")]
    hello = os.path.join(ROOT, "examples", "hello.azl")
    compared = 0
    forms = [(target, lower) for target in TARGETS]
    if sroa is not None:
        forms.append((TARGETS[0], sroa))
    for target, emitter in forms:
        sources = common + [os.path.join(ROOT, "compiler", "host_" + target.replace("-", "_") + ".azl")]
        code, text, err = base.tool(emitter, "emit", *sources)
        if code != 0:
            problems.append("the compiler for %s: %s exits with %r: %r" % (target, os.path.basename(emitter), code, err[-300:]))
            continue
        emitted = os.path.join(folder, "compiler-" + target + ".ir")
        with open(emitted, "wb") as handle:
            handle.write(text)
        built = os.path.join(folder, "compiler-" + target)
        done = subprocess.run([back, "build", emitted, "-o", built, "--target", target, "--check", "--program"], capture_output=True)
        if done.returncode != 0:
            problems.append("the compiler for %s: the back end refuses it: %s" % (target, done.stderr.decode(errors="replace").strip()[:300]))
            continue
        for label, files in (("examples/hello.azl", [hello]), ("itself", sources)):
            real = os.path.join(folder, "real", "out-" + target)
            ours = os.path.join(folder, "ours", "out-" + target)
            os.makedirs(os.path.dirname(real), exist_ok=True)
            os.makedirs(os.path.dirname(ours), exist_ok=True)
            first = subprocess.run([azol, "build", "--target", target, *files, "-o", real], capture_output=True)
            try:
                second = subprocess.run([sys.executable, emulator, built, "build", *files, "-o", ours], capture_output=True, timeout=900)
            except subprocess.TimeoutExpired:
                problems.append("the compiler built by the back end for %s does not finish building %s" % (target, label))
                continue
            if first.returncode != 0 or second.returncode != 0:
                problems.append("the compiler built by the back end for %s, building %s: exit %r (azol: %r), %r" % (target, label, second.returncode, first.returncode, second.stderr[-300:]))
                continue
            with open(real, "rb") as one, open(ours, "rb") as two:
                if one.read() != two.read():
                    problems.append("the compiler built by the back end for %s builds %s differently from %s" % (target, label, azol))
                else:
                    compared += 1
    return compared


def run(azol, ir_tool, folder, total, seed, problems, kernels=False):
    """Everything; prints what was done. `ir_tool` is a built ir-tool."""
    if emulators() is None:
        print("ARM64 back end: skipped (needs the Python package unicorn and the checking emulators in %s; on a Mac run tests/opt/mac_check.sh)" % REF)
        return
    back = os.path.join(folder, "azol-back-arm64")
    base.build(azol, x64.BACK_SOURCES, back)
    cases, programs = file_tests(ir_tool, back, folder, problems)
    print("ARM64 back end: %d cases of the files, %d programs built and run in the emulators (macOS and Linux, all registers and three)" % (cases, programs))
    print("ARM64 back end: %d programs with jumps too far for the short forms" % far_tests(ir_tool, back, folder, problems))
    lower = build_lower(azol, folder, problems)
    if lower is None:
        print("ARM64 back end: whole programs skipped (the lowering stage is not in this tree)")
    else:
        compared, sources = whole_program_tests(azol, back, lower, folder, problems)
        print("ARM64 back end: %d whole programs lowered, %d executables built by the back end and run like their default builds (both targets, all registers and three)" % (sources, compared))
        built = compiler_tests(azol, back, lower, folder, problems)
        print("ARM64 back end: the lowered compiler built for both targets, and with `sroa` for macOS; in the emulator each built examples/hello.azl and itself byte for byte as azol does (%d builds compared)" % built)
    if total > 0:
        made, files, compared, stressed = random_tests(ir_tool, back, total, seed, folder, problems)
        print("ARM64 back end: %d random functions in %d modules, %d programs built and run (%d of them with three registers)" % (made, files, compared, stressed))
    if kernels:
        table = kernel_tests(azol, back, folder, problems)
        print("ARM64 back end: the four benchmark kernels on both targets; instructions on macOS:")
        for line in table:
            print(line)


def main():
    argv = sys.argv[1:]
    azol = base.find_azol(argv)
    total = int(argv[argv.index("--random") + 1]) if "--random" in argv else 30000
    seed = int(argv[argv.index("--seed") + 1]) if "--seed" in argv else 20261112
    folder = tempfile.mkdtemp(prefix="azol-back-arm64-")
    ir_tool = os.path.join(folder, "ir-tool")
    base.build(azol, base.SOURCES, ir_tool)
    problems = []
    run(azol, ir_tool, folder, total, seed, problems, "--kernels" in argv)
    for problem in problems:
        print("FAIL " + problem)
    if problems:
        print("%d FAILED; files are in %s" % (len(problems), folder))
        sys.exit(1)
    if "--keep" not in argv:
        shutil.rmtree(folder, ignore_errors=True)
    print("ALL ARM64 BACK END TESTS PASSED")


if __name__ == "__main__":
    main()
