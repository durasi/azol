#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Tests of `azol-opt build` (compiler/opt/opt_build_main.azl) as a whole,
for the two ARM64 targets, on a machine that is neither: source files in,
an executable out, through lowering, every pass of the order in
pipeline.azl, the ARM64 back end and the default compiler's runtime and
executable writers.

    python3 tests/opt/run_driver_tests.py [--azol PATH] [--count] [--keep]

1. Every program of tests/programs and examples that `azol build`
   accepts is built by `azol-opt build --target T --check` for
   aarch64-macos and aarch64-linux and run in the checking emulators
   (see run_back_arm64_tests.py). It must behave as the executable
   `azol build --target T` makes of it: the same standard output,
   standard error and exit code, no more blocks left unfreed, and
   nothing the emulator objects to. The programs that overflow the
   stack, index outside a list, divide by zero and so on are among
   them.

2. Every program of bench/suite, at its small size, the same way in the
   fast emulator of the repository (compiler/emu_arm64.py), which checks
   only what the program prints and how it ends (a default build that
   ends with another code than 0 is noted and not counted against the
   optimised one: `fannkuch` for aarch64-linux does, because a bare
   `return` in `main` frees the lists after setting the code). With `--count` the
   instructions each one runs are counted and printed as a table, the
   default build next to the optimised one (this takes much longer).

3. The same program built twice gives the same bytes, and on this
   machine `azol-opt build` with no `--target` builds for this machine.

Where the emulators are missing everything here is skipped with a note;
on a Mac, tests/opt/mac_check.sh runs natively what `azol-opt build`
makes.

run_opt_tests.py runs parts 1 to 3 at its end.
"""
import concurrent.futures
import os
import re
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, HERE)
import run_back_arm64_tests as arm64  # noqa: E402
import run_opt_tests as base  # noqa: E402
import run_pass_tests as passes  # noqa: E402

TARGETS = arm64.TARGETS
SUITE = os.path.join(ROOT, "bench", "suite")


def build_driver(azol, folder):
    tool = os.path.join(folder, "azol-opt-driver")
    base.build(azol, passes.opt_build_sources(), tool)
    return tool


def one_program(azol, driver, work, where, name):
    """One program on both targets. Gives (executables compared, problems)."""
    found = []
    compared = 0
    path = os.path.join(ROOT, where, name)
    stem = name[:-4]
    for target in TARGETS:
        plain = os.path.join(work, "plain-" + stem)
        if subprocess.run([azol, "build", "--target", target, path, "-o", plain], capture_output=True).returncode != 0:
            return 0, found
        label = "%s/%s for %s" % (where, name, target)
        exe = os.path.join(work, "opt-" + stem)
        built = subprocess.run([driver, "build", path, "-o", exe, "--target", target, "--check"], capture_output=True)
        if built.returncode != 0:
            found.append("%s: azol-opt refuses it: %s" % (label, built.stderr.decode(errors="replace").strip()[:300]))
            continue
        want = arm64.run_exe(target, plain, [plain])
        got = arm64.run_exe(target, exe, [plain])
        if got[4] and got[4] != want[4]:
            found.append("%s: %s" % (label, got[4]))
        elif got[:3] != want[:3]:
            found.append("%s: exit %r (default build %r), output %r (default build %r), errors %r (default build %r)" % (
                label, got[0], want[0], got[1][-80:], want[1][-80:], got[2][-80:], want[2][-80:]))
        elif got[3] > want[3]:
            found.append("%s: %d blocks are never freed (default build %d)" % (label, got[3], want[3]))
        else:
            compared += 1
    return compared, found


def program_tests(azol, driver, folder, problems):
    work = os.path.join(folder, "driver-programs")
    os.makedirs(work, exist_ok=True)
    jobs = []
    compared = 0
    sources = 0
    with concurrent.futures.ProcessPoolExecutor(max_workers=2) as pool:
        for where in ("tests/programs", "examples"):
            for name in sorted(os.listdir(os.path.join(ROOT, where))):
                if name.endswith(".azl"):
                    jobs.append(pool.submit(one_program, azol, driver, work, where, name))
        for job in jobs:
            done, found = job.result()
            compared += done
            sources += 1 if done else 0
            problems.extend(found)
    return compared, sources


def small_source(text):
    new, count = re.subn(r"^const SMALL = false$", "const SMALL = true", text, flags=re.M)
    if count != 1:
        raise ValueError("the line that sets SMALL to false is missing")
    return new


def one_benchmark(azol, driver, work, name, counted):
    """One program of the suite on both targets in the fast emulator.
    Gives (executables compared, problems, {target: (default, optimised)})."""
    found = []
    compared = 0
    counts = {}
    path = os.path.join(work, name + ".azl")
    with open(os.path.join(SUITE, name + ".azl"), encoding="utf-8") as handle:
        text = small_source(handle.read())
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text)
    for target in TARGETS:
        label = "bench/suite/%s.azl for %s" % (name, target)
        plain = os.path.join(work, "%s-plain-%s" % (name, target))
        exe = os.path.join(work, "%s-opt-%s" % (name, target))
        done = subprocess.run([azol, "build", "--target", target, path, "-o", plain], capture_output=True)
        if done.returncode != 0:
            found.append("%s: azol refuses it: %s" % (label, done.stderr.decode(errors="replace").strip()[:300]))
            continue
        done = subprocess.run([driver, "build", path, "-o", exe, "--target", target, "--check"], capture_output=True)
        if done.returncode != 0:
            found.append("%s: azol-opt refuses it: %s" % (label, done.stderr.decode(errors="replace").strip()[:300]))
            continue
        want = arm64.fast_run(plain, counted)
        got = arm64.fast_run(exe, counted)
        if got[3]:
            found.append("%s: %s" % (label, got[3]))
        elif got[1] != want[1] or got[2] != 0:
            found.append("%s: exit %r (default build %r), output %r (default build %r)" % (label, got[2], want[2], got[1][-80:], want[1][-80:]))
        else:
            # Every program of the suite ends with 0. Where the default
            # build does not, the mistake is the default generator's
            # and is only noted.
            if want[2] != 0:
                found.append("NOTE %s: the default build exits with %r, the optimised one with 0" % (label, want[2]))
            compared += 1
            counts[target] = (want[0], got[0])
    return compared, found, counts


def benchmark_tests(azol, driver, folder, problems, counted):
    work = os.path.join(folder, "driver-suite")
    os.makedirs(work, exist_ok=True)
    names = sorted(name[:-4] for name in os.listdir(SUITE) if name.endswith(".azl"))
    compared = 0
    table = {}
    with concurrent.futures.ProcessPoolExecutor(max_workers=2) as pool:
        jobs = [(name, pool.submit(one_benchmark, azol, driver, work, name, counted)) for name in names]
        for name, job in jobs:
            done, found, counts = job.result()
            compared += done
            for line in found:
                if line.startswith("NOTE "):
                    print("azol-opt for ARM64: note: " + line[5:])
                else:
                    problems.append(line)
            table[name] = counts
    return compared, len(names), table


def count_table(table):
    import math
    lines = ["| Program | default, macOS | azol-opt, macOS | ratio | default, Linux | azol-opt, Linux | ratio |", "|---|---|---|---|---|---|---|"]
    ratios = {target: [] for target in TARGETS}
    for name in sorted(table):
        cells = []
        for target in TARGETS:
            if target in table[name] and table[name][target][0]:
                plain, opt = table[name][target]
                ratios[target].append(opt / plain)
                cells += ["{:,}".format(plain), "{:,}".format(opt), "%.2f" % (opt / plain)]
            else:
                cells += ["n/a", "n/a", "n/a"]
        lines.append("| `%s` | %s |" % (name, " | ".join(cells)))
    means = []
    for target in TARGETS:
        values = ratios[target]
        means.append("%s %.2f" % (target, math.exp(sum(math.log(v) for v in values) / len(values))) if values else "%s n/a" % target)
    lines.append("")
    lines.append("Geometric means of azol-opt against the default build: " + ", ".join(means) + ".")
    return lines


def same_twice(azol, driver, folder, problems):
    """Two builds of one program are the same bytes, for each target, and
    the build with no target is the host's."""
    source = os.path.join(ROOT, "examples", "hello.azl")
    built = 0
    for target in TARGETS + ("x86_64-linux",):
        images = []
        for turn in ("a", "b"):
            where = os.path.join(folder, "twice-" + turn)
            os.makedirs(where, exist_ok=True)
            exe = os.path.join(where, "hello")
            done = subprocess.run([driver, "build", source, "-o", exe, "--target", target], capture_output=True)
            if done.returncode != 0:
                problems.append("azol-opt build --target %s examples/hello.azl: %s" % (target, done.stderr.decode(errors="replace").strip()[:200]))
                break
            with open(exe, "rb") as handle:
                images.append(handle.read())
        if len(images) == 2:
            built += 1
            if images[0] != images[1]:
                problems.append("azol-opt build --target %s: two builds of examples/hello.azl differ" % target)
        if images:
            # `azol build -O` is the same path inside the compiler.
            exe = os.path.join(folder, "twice-b", "hello")
            done = subprocess.run([azol, "build", "-O", source, "-o", exe, "--target", target], capture_output=True)
            with open(exe, "rb") as handle:
                if done.returncode != 0 or handle.read() != images[0]:
                    problems.append("azol build -O --target %s does not give the file azol-opt build gives" % target)
        host = os.path.basename(passes.host_file() or "")[5:-4].replace("_linux", "-linux").replace("_macos", "-macos")
        if images and host == target:
            exe = os.path.join(folder, "twice-a", "hello")
            done = subprocess.run([driver, "build", source, "-o", exe], capture_output=True)
            with open(exe, "rb") as handle:
                if done.returncode != 0 or handle.read() != images[0]:
                    problems.append("azol-opt build with no --target does not build for this machine (%s)" % target)
    wrong = subprocess.run([driver, "build", source, "-o", os.path.join(folder, "never"), "--target", "riscv"], capture_output=True)
    if wrong.returncode != 2 or b"unknown target" not in wrong.stderr:
        problems.append("azol-opt build --target riscv: exit %d, %r" % (wrong.returncode, wrong.stderr[:100]))
    return built


def run(azol, folder, problems, counted=False):
    """Everything; prints what was done."""
    if arm64.emulators() is None:
        print("azol-opt for ARM64: skipped (needs the Python package unicorn and the checking emulators in %s; on a Mac run tests/opt/mac_check.sh)" % arm64.REF)
        return
    driver = build_driver(azol, folder)
    compared, sources = program_tests(azol, driver, folder, problems)
    print("azol-opt for ARM64: %d programs of tests/programs and examples, %d executables built from source through every pass and run like their default builds in the checking emulators (macOS and Linux)" % (sources, compared))
    compared, names, table = benchmark_tests(azol, driver, folder, problems, counted)
    print("azol-opt for ARM64: %d programs of bench/suite at their small size, %d executables run like their default builds (macOS and Linux)" % (names, compared))
    if counted:
        for line in count_table(table):
            print(line)
    print("azol-opt: the same bytes from two builds on %d targets, and from `azol build -O`; no target means this machine; an unknown target is refused" % same_twice(azol, driver, folder, problems))


def main():
    argv = sys.argv[1:]
    azol = base.find_azol(argv)
    folder = tempfile.mkdtemp(prefix="azol-driver-")
    problems = []
    run(azol, folder, problems, "--count" in argv)
    for problem in problems:
        print("FAIL " + problem)
    if problems:
        print("%d FAILED; files are in %s" % (len(problems), folder))
        sys.exit(1)
    if "--keep" not in argv:
        shutil.rmtree(folder, ignore_errors=True)
    print("ALL DRIVER TESTS PASSED")


if __name__ == "__main__":
    main()
