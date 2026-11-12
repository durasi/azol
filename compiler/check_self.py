#!/usr/bin/env python3
"""Checks the azoL compiler written in azoL against the one written in Rust.

`azol-self` is the whole compiler in azoL: the lexer, the parser, the
checker, the code generators for x86-64 and ARM64, the writers of ELF and
Mach-O files, the diagnostics and the formatter in this folder, with
`self_main.azl` on top. This script proves four things about it;
`check_cli.py` compares the rest of the command line.

1. The same executables. For every program and each of the three targets
   (`x86_64-linux`, `aarch64-macos`, `aarch64-linux`), `azol-self build`
   must write the file `azol build` writes, byte for byte, on macOS its
   signature included: every .azl file of tests/programs, examples, bench
   and this folder, the programs of several files (tests/multi and the
   tools here), and bench/generated/big.azl. A file that is not a program
   on its own must be refused by both with the same words and the same
   exit code.

2. The compiler compiles itself. Stage 1 is `azol-self` built by the
   Rust compiler. Stage 2 is the same sources built by stage 1, stage 3
   by stage 2. All three must be the same file. Then every program of
   tests/programs is built by stage 2, run, and compared with the
   `// expect-...` lines at its top.

3. The compiler compiles itself on ARM64 too, as far as a machine that
   is not ARM64 can show. Stage 1 builds `azol-self` for `aarch64-macos`
   and for `aarch64-linux`; each must be the file the Rust compiler
   builds for that target. Then each of the two runs in an emulator
   (`emu_arm64.py`), where it builds a few programs and the compiler
   itself for all three targets; what it writes must be what stage 1
   writes. Where the checking emulators of the test programs are at
   hand, each also builds one program under those: no memory may be
   left behind or touched out of bounds. Without the Python package
   `unicorn` the runs are skipped, and the script says so.

4. The same errors. Wrong programs (the ones `check_front.py` and
   `check_sema.py` use, and right programs damaged at regular places)
   must be refused with the same text on standard error, error codes
   included, and the same exit code.

    python3 compiler/check_self.py           everything
    python3 compiler/check_self.py --quick   without bench/generated/big.azl

Which machine an `azol-self` runs on is said when it is built, by one of
the files `host_*.azl` (see `self_main.azl`); `sources(target)` below
names the right one. Stage 1 is built for x86-64 Linux and run directly,
so this script needs such a machine. The compiler it compares with is the oracle: `--oracle <path>`, or see
compiler/oracle.py for the one taken when none is given.
"""
import glob, os, platform, re, shutil, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import check_front, check_sema  # noqa: E402  (the wrong programs come from them)

import oracle  # noqa: E402

AZOL = oracle.path()
BIG = os.path.join(ROOT, "bench", "generated", "big.azl")
TARGETS = ["x86_64-linux", "aarch64-macos", "aarch64-linux"]
# The compiler's sources but for the one file that says where it runs.
def sources(target):
    """The files an `azol-self` that runs on `target` is built from."""
    return oracle.sources(target)


# For the machine this script runs on.
SOURCES = sources("x86_64-linux")
SETS = check_sema.SETS + [["compiler/" + name for name in SOURCES]]



def emulator_repo():
    """The folder the checking emulator for Linux on ARM64 reads the facts
    about the runtime from. It looks for them as a module of the first
    compiler, `src/runtime_linux_arm64.rs`; once that is gone, it gets the
    same text from `runtime/linux_arm64/runtime.facts` in a folder made
    for it."""
    if os.path.exists(os.path.join(ROOT, "src", "runtime_linux_arm64.rs")):
        return ROOT
    folder = tempfile.mkdtemp(prefix="azol-facts-")
    os.makedirs(os.path.join(folder, "src"))
    shutil.copy(os.path.join(ROOT, "runtime", "linux_arm64", "runtime.facts"), os.path.join(folder, "src", "runtime_linux_arm64.rs"))
    return folder


def run(command, **more):
    return subprocess.run(command, capture_output=True, **more)


def first_difference(a, b):
    for at, (x, y) in enumerate(zip(a, b)):
        if x != y:
            return at
    return min(len(a), len(b))


def rust_build(paths, out, target="x86_64-linux"):
    return run([AZOL, "build", "--target", target, "-o", out] + paths)


def self_build(tool, paths, out, target=None):
    """Without a target `azol-self` builds for the machine it runs on."""
    return run([tool, "build", "-o", out] + (["--target", target] if target else []) + paths)


def compare(tool, paths, shown, work, counts, target=None):
    """Returns None when both compilers agree on the program, else a description."""
    # The same file name for both: a macOS signature holds it.
    theirs, ours = os.path.join(work, "theirs", "program"), os.path.join(work, "ours", "program")
    for path in (theirs, ours):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        if os.path.exists(path):
            os.remove(path)
    want = rust_build(paths, theirs, target or "x86_64-linux")
    got = self_build(tool, paths, ours, target)
    if want.returncode != got.returncode:
        return (f"{shown}: exit code {got.returncode}, expected {want.returncode}\n  azol:      {want.stderr[:300]!r}\n"
                f"  azol-self: {got.stderr[:300]!r}")
    if want.returncode != 0:
        a, b = want.stderr, got.stderr
        if a != b:
            at = first_difference(a, b)
            return (f"{shown}: standard error differs at byte {at}\n  azol:      {a[max(0, at - 60):at + 100]!r}\n"
                    f"  azol-self: {b[max(0, at - 60):at + 100]!r}")
        if os.path.exists(ours):
            return f"{shown}: azol-self refused the program and still wrote a file"
        counts["refused"] += 1
        return None
    a, b = open(theirs, "rb").read(), open(ours, "rb").read()
    if a != b:
        return f"{shown}: the executables differ, first at byte {first_difference(a, b)}, sizes {len(a)} and {len(b)}"
    if not os.access(ours, os.X_OK):
        return f"{shown}: the executable is not marked executable"
    counts[target or "built"] += 1
    return None


def compare_all(tool, paths, shown, work, counts):
    """`compare` for the machine this runs on, and for a program that builds, for the other targets too."""
    before = counts["built"]
    problem = compare(tool, paths, shown, work, counts)
    if problem or counts["built"] == before:
        return problem
    for target in TARGETS[1:]:
        problem = compare(tool, paths, f"{shown} for {target}", work, counts, target)
        if problem:
            return problem
    return None


def checked_run(host, tool, program, stage1, work):
    """Runs the compiler for `host` on one program under the checking
    emulator the test programs run under, when that is at hand (in the
    folder `AZOL_EMU_DIR` names, or /home/claude/ref): it must leave no
    block of memory behind, touch none it does not own and hand every
    register back. Returns a description of what is wrong, or None."""
    folder = os.environ.get("AZOL_EMU_DIR", "/home/claude/ref")
    name = "emu" if host == "aarch64-macos" else "emu_linux_arm64"
    if not os.path.exists(os.path.join(folder, name + ".py")):
        print(f"SKIPPED the {host} compiler under the checking emulator: {name}.py is not in {folder}", flush=True)
        return None
    sys.path.insert(0, folder)
    try:
        checking = __import__(name)
    except ImportError as error:
        print(f"SKIPPED the {host} compiler under the checking emulator: {error}", flush=True)
        return None
    finally:
        sys.path.remove(folder)
    ours = os.path.join(work, "checked", host, "program")
    os.makedirs(os.path.dirname(ours), exist_ok=True)
    arguments = ["build", "--target", host, "-o", ours, program]
    if host == "aarch64-macos":
        out, code, trap, balanced, errs, heap = checking.run(tool, max_insn=4_000_000_000, argv=arguments)
    else:
        out, code, trap, balanced, errs, heap = checking.run(tool, max_insn=4_000_000_000, argv=arguments, repo=emulator_repo())
    if code != 0 or trap is not None or not balanced or heap["leaks"] or heap["violations"]:
        return f"the compiler for {host} under the checking emulator: exit {code}, trap {trap}, balanced {balanced}, heap {heap}, {errs[:200]!r}"
    want_path = os.path.join(work, "checked", "native", "program")
    os.makedirs(os.path.dirname(want_path), exist_ok=True)
    self_build(stage1, [program], want_path, host)
    if open(ours, "rb").read() != open(want_path, "rb").read():
        return f"the compiler for {host} under the checking emulator builds {os.path.relpath(program, ROOT)} differently from stage 1"
    print(f"clean {os.path.relpath(program, ROOT)} built by the {host} compiler under the checking emulator: "
          f"{heap['allocs']} blocks of memory, all freed", flush=True)
    return None


def cross(stage1, work):
    """Part 3 of the text at the top. Returns a description of what is wrong, or None."""
    import emu_arm64
    native, emulated = os.path.join(work, "native"), os.path.join(work, "emulated")
    # What stage 1 builds for each target, with the file name every copy below gets.
    made = {}
    for target in TARGETS:
        made[target] = os.path.join(native, target, "azol-self")
        os.makedirs(os.path.dirname(made[target]))
        paths = [os.path.join(HERE, name) for name in sources(target)]
        built = self_build(stage1, paths, made[target], target)
        if built.returncode != 0:
            return f"stage 1 does not build the compiler for {target}: {built.stderr[:400]!r}"
        theirs = os.path.join(work, "theirs", "azol-self")
        os.makedirs(os.path.dirname(theirs), exist_ok=True)
        if rust_build(paths, theirs, target).returncode != 0:
            return f"azol does not build the compiler for {target}"
        if open(theirs, "rb").read() != open(made[target], "rb").read():
            return f"the compiler for {target}: stage 1 and azol write different files"
        print(f"same  azol-self for {target}, built by stage 1 and by azol: {os.path.getsize(made[target])} bytes", flush=True)
    if not emu_arm64.available():
        print("SKIPPED running the ARM64 compilers: the Python package `unicorn` is not installed", flush=True)
        return None
    programs = [os.path.join(ROOT, path) for path in ["examples/hello.azl", "examples/wc.azl", "tests/programs/map.azl", "tests/programs/try_places.azl"]]
    programs = [path for path in programs if os.path.exists(path)]
    for host in TARGETS[1:]:
        tool = made[host]
        out, err, code, problem = emu_arm64.run(tool, ["--version"])
        want = run([stage1, "--version"])
        if problem or code != 0 or out != want.stdout:
            return f"the compiler for {host}, run in the emulator, does not say its version: {problem or err[:200]!r}"
        # Without a target it builds for where it runs.
        for path in programs[:1]:
            ours, theirs = os.path.join(emulated, "default", "program"), os.path.join(native, "default", "program")
            for folder in (ours, theirs):
                os.makedirs(os.path.dirname(folder), exist_ok=True)
            out, err, code, problem = emu_arm64.run(tool, ["build", "-o", ours, path])
            self_build(stage1, [path], theirs, host)
            if problem or code != 0 or open(ours, "rb").read() != open(theirs, "rb").read():
                return f"the compiler for {host}, run in the emulator, does not build for {host} when no target is named: {problem or err[:200]!r}"
        for target in TARGETS:
            jobs = [([path], os.path.join(native, "p", target, "program")) for path in programs]
            jobs.append(([os.path.join(HERE, name) for name in sources(target)], made[target]))
            for paths, want_path in jobs:
                if len(paths) == 1:
                    os.makedirs(os.path.dirname(want_path), exist_ok=True)
                    self_build(stage1, paths, want_path, target)
                ours = os.path.join(emulated, host, target, os.path.basename(want_path))
                os.makedirs(os.path.dirname(ours), exist_ok=True)
                if os.path.exists(ours):
                    os.remove(ours)
                out, err, code, problem = emu_arm64.run(tool, ["build", "--target", target, "-o", ours] + paths)
                shown = "the compiler" if len(paths) > 1 else os.path.relpath(paths[0], ROOT)
                if problem or code != 0:
                    return f"the compiler for {host}, run in the emulator, does not build {shown} for {target}: {problem or err[:300]!r}"
                if open(ours, "rb").read() != open(want_path, "rb").read():
                    return f"the compiler for {host}, run in the emulator, builds {shown} for {target} differently from stage 1"
                if not os.access(ours, os.X_OK):
                    return f"the compiler for {host}, run in the emulator: {shown} for {target} is not marked executable"
            print(f"same  {len(programs)} programs and the compiler for {target}, built by the {host} compiler in the emulator", flush=True)
        problem = checked_run(host, tool, programs[0], stage1, work)
        if problem:
            return problem
        # A wrong program is refused with the same words.
        wrong = os.path.join(work, "wrong.azl")
        with open(wrong, "w") as out_file:
            out_file.write("fn main() {\n    let x = 1\n    x = 2\n}\n")
        want = run([stage1, "check", wrong])
        out, err, code, problem = emu_arm64.run(tool, ["check", wrong])
        if problem or code != want.returncode or err != want.stderr or out != want.stdout:
            return f"the compiler for {host}, run in the emulator, reports a wrong program differently: {problem or err[:300]!r}"
    return None


def run_tests(tool, work):
    """Builds every test program with `tool`, runs it and checks what it prints. Returns how many failed."""
    failed = 0
    exe = os.path.join(work, "test-program")
    for path in sorted(glob.glob(os.path.join(ROOT, "tests", "programs", "*.azl"))):
        name = os.path.basename(path)
        src = open(path, encoding="utf-8").read()
        want_exit = int(re.search(r"expect-exit: (\d+)", src).group(1))
        lines = re.findall(r"^// expect-line: ?(.*)$", src, re.M)
        if lines:
            want_out = "".join(line + "\n" for line in lines)
        else:
            want_out = "".join(word + "\n" for word in re.search(r"expect-output: (.*)", src).group(1).split())
        want_err = re.search(r"^// expect-stderr: (.*)$", src, re.M)
        want_err = want_err.group(1) + "\n" if want_err else ""
        built = self_build(tool, [path], exe)
        if built.returncode != 0:
            print(f"FAIL  {name}: does not build: {built.stderr[:200]!r}")
            failed += 1
            continue
        try:
            done = run([exe], timeout=120, cwd=work)
        except subprocess.TimeoutExpired:
            print(f"FAIL  {name}: still running after two minutes")
            failed += 1
            continue
        out, err = done.stdout.decode(errors="replace"), done.stderr.decode(errors="replace")
        if out != want_out or err != want_err or done.returncode != want_exit:
            print(f"FAIL  {name}: exit {done.returncode} (expected {want_exit})\n  printed  {out[:300]!r}\n  expected {want_out[:300]!r}\n  stderr   {err[:200]!r}")
            failed += 1
        else:
            print(f"ok    run  {name}")
    return failed


def best_time(command, runs=7):
    best = None
    for _ in range(runs):
        started = time.perf_counter()
        done = run(command)
        took = time.perf_counter() - started
        if done.returncode != 0:
            sys.exit("cannot time " + " ".join(command) + ":\n" + done.stderr.decode(errors="replace"))
        best = took if best is None else min(best, took)
    return best * 1000


def instructions(command):
    """Instructions a command executes, counted by valgrind, or None without it."""
    if not shutil.which("valgrind"):
        return None
    done = run(["valgrind", "--tool=callgrind", "--callgrind-out-file=/dev/null"] + command)
    found = re.search(rb"Collected : (\d+)", done.stderr)
    return int(found.group(1)) if found and done.returncode == 0 else None


def main():
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        sys.exit("check_self.py runs azol-self directly and so needs x86-64 Linux")
    if not os.path.exists(AZOL):
        sys.exit(oracle.missing())
    work = tempfile.mkdtemp(prefix="azol-check-self-")
    try:
        return check(work, "--quick" in sys.argv[1:])
    finally:
        shutil.rmtree(work, ignore_errors=True)


def write(folder, name, text):
    path = os.path.join(folder, name + ".azl")
    with open(path, "w", encoding="utf-8", newline="") as out:
        out.write(text)
    return path


def check(work, quick):
    for generator in oracle.generators():
        stale = run([sys.executable, generator, "--check"])
        if stale.returncode != 0:
            sys.exit(stale.stderr.decode().strip())
    own = [os.path.join(HERE, name) for name in SOURCES]

    # The compiler compiles itself.
    stage1 = os.path.join(work, "azol-self")
    built = rust_build(own, stage1)
    if built.returncode != 0:
        sys.exit("azol-self does not build:\n" + built.stderr.decode(errors="replace"))
    stage2, stage3 = os.path.join(work, "stage2"), os.path.join(work, "stage3")
    for tool, out in [(stage1, stage2), (stage2, stage3)]:
        built = self_build(tool, own, out)
        if built.returncode != 0:
            print(f"DIFFERENT {os.path.basename(tool)} does not build the compiler: {built.stderr[:400]!r}")
            return 1
    one, two, three = (open(path, "rb").read() for path in (stage1, stage2, stage3))
    if one != two or two != three:
        print(f"DIFFERENT the stages are not the same file: sizes {len(one)}, {len(two)}, {len(three)}; "
              f"stage 1 and 2 first differ at byte {first_difference(one, two)}, stage 2 and 3 at byte {first_difference(two, three)}")
        return 1
    print(f"same  stage 1 (built by azol), stage 2 (built by stage 1), stage 3 (built by stage 2): {len(one)} bytes", flush=True)

    failed = run_tests(stage2, work)
    if failed:
        print(f"{failed} test programs built by stage 2 FAILED")
        return 1

    # The compiler for ARM64, built and then run in an emulator.
    problem = cross(stage1, work)
    if problem:
        print("DIFFERENT " + problem)
        return 1

    # The same executables, and the same errors.
    groups = []
    files = []
    for pattern in ["tests/programs/*.azl", "tests/multi/*/*.azl", "examples/*.azl", "bench/*.azl", "compiler/*.azl", "lib/prelude.azl"]:
        files += sorted(glob.glob(os.path.join(ROOT, pattern)))
    if not files:
        sys.exit("no programs found")
    groups.append(("", [([path], os.path.relpath(path, ROOT)) for path in files]))
    groups.append(("", [([os.path.join(ROOT, path) for path in paths], " + ".join(paths)) for paths in SETS]))

    folder = os.path.join(work, "made")
    os.mkdir(folder)
    groups.append(("syntax ", [([write(folder, "syntax_" + name, text)], name) for name, text in check_front.BROKEN.items()]))
    groups.append(("wrong ", [([write(folder, name, text)], name) for name, text in check_sema.BROKEN.items()]))
    groups.append(("corner ", [([write(folder, "corner_" + name, text)], name) for name, text in check_sema.CORNERS.items()]))

    # Right programs, damaged: a text replaced or a line taken out (as in
    # check_sema.py), a piece put in or the text cut off (as in
    # check_front.py).
    damaged = []
    seeds = sorted(glob.glob(os.path.join(ROOT, "tests", "programs", "*.azl"))) + sorted(glob.glob(os.path.join(ROOT, "examples", "*.azl")))
    for seed_at, seed in enumerate(seeds):
        text = open(seed, encoding="utf-8").read()
        stem = os.path.basename(seed)[:-4]
        changed = []
        for swap_at, (old, new) in enumerate(check_sema.SWAPS):
            first = text.find(old)
            if first < 0 or (swap_at + seed_at) % 3 == 2:
                continue
            changed.append((f"swap{swap_at}", text[:first] + new + text[first + len(old):]))
        lines = text.split("\n")
        step = max(1, len(lines) // 8)
        for at in range(seed_at % step, len(lines), step):
            if lines[at].strip() and not lines[at].lstrip().startswith("//"):
                changed.append((f"line{at}", "\n".join(lines[:at] + lines[at + 1:])))
        step = max(1, len(text) // 6)
        for round_at, at in enumerate(range(step // 2, len(text), step)):
            piece = check_front.PIECES[(round_at + seed_at) % len(check_front.PIECES)]
            changed.append((f"put{at}", text[:at] + piece + text[at:]))
            if round_at % 3 == 0:
                changed.append((f"cut{at}", text[:at]))
        for kind, new_text in changed:
            damaged.append(([write(folder, f"damaged_{stem}_{kind}", new_text)], f"{stem} {kind}"))
    groups.append(("damaged ", damaged))

    count = 0
    counts = {"built": 0, "refused": 0, "aarch64-macos": 0, "aarch64-linux": 0}
    for group, entries in groups:
        for paths, shown in entries:
            problem = compare_all(stage2, paths, group + shown, work, counts)
            if problem:
                print("DIFFERENT " + problem)
                return 1
            print(f"same  {group}{shown}")
            count += 1

    if not quick:
        if not os.path.exists(BIG):
            sys.exit("bench/generated/big.azl is missing: python3 bench/gen.py")
        problem = compare_all(stage2, [BIG], "bench/generated/big.azl", work, counts)
        if problem:
            print("DIFFERENT " + problem)
            return 1
        print("same  bench/generated/big.azl", flush=True)
        count += 1

        out = os.path.join(work, "big")
        print()
        print("big.azl, source to executable, best of 7, whole process:")
        for target in TARGETS:
            ours = [stage2, "build", "--target", target, "-o", out, BIG]
            theirs = [AZOL, "build", "--target", target, "-o", out, BIG]
            print(f"  for {target}:")
            print(f"    azol-self build   {best_time(ours):7.1f} ms")
            print(f"    azol build        {best_time(theirs):7.1f} ms")
            a, b = instructions(ours), instructions(theirs)
            if a and b:
                print(f"    instructions executed: azol-self {a / 1e6:.0f} million, azol {b / 1e6:.0f} million ({a / b:.1f} times)")
            inner = run(theirs).stderr.decode().strip()
            print(f"    azol's own clock: {inner}")

    print()
    print(f"{count} comparisons: {counts['built']} programs built to the same executable for x86_64-linux, {counts['aarch64-macos']} for "
          f"aarch64-macos and {counts['aarch64-linux']} for aarch64-linux, {counts['refused']} refused with the same errors")
    print("ALL IDENTICAL")
    return 0


if __name__ == "__main__":
    sys.exit(main())
