#!/usr/bin/env python3
"""Checks the azoL compiler written in azoL against the one written in Rust.

`azol-self` is the whole compiler in azoL: the lexer, the parser, the
checker, the x86-64 code generator, the ELF writer, the diagnostics and
the formatter in this folder, with `self_main.azl` on top. This script
proves three things about it; `check_cli.py` compares the rest of the
command line.

1. The same executables. For every program, `azol-self build` must write
   the file `azol build --target x86_64-linux` writes, byte for byte:
   every .azl file of tests/programs, examples, bench and this folder,
   the programs of several files (tests/multi and the tools here), and
   bench/generated/big.azl. A file that is not a program on its own must
   be refused by both with the same words and the same exit code.

2. The compiler compiles itself. Stage 1 is `azol-self` built by the
   Rust compiler. Stage 2 is the same sources built by stage 1, stage 3
   by stage 2. All three must be the same file. Then every program of
   tests/programs is built by stage 2, run, and compared with the
   `// expect-...` lines at its top.

3. The same errors. Wrong programs (the ones `check_front.py` and
   `check_sema.py` use, and right programs damaged at regular places)
   must be refused with the same text on standard error, error codes
   included, and the same exit code.

    python3 compiler/check_self.py           everything
    python3 compiler/check_self.py --quick   without bench/generated/big.azl

`azol-self` is built for x86-64 Linux and run directly, so this script
needs such a machine. Build the Rust compiler first: cargo build --release
"""
import glob, os, platform, re, shutil, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import check_front, check_sema  # noqa: E402  (the wrong programs come from them)

AZOL = os.path.join(ROOT, "target", "release", "azol")
BIG = os.path.join(ROOT, "bench", "generated", "big.azl")
SOURCES = ["self_main.azl", "driver.azl", "token.azl", "lexer.azl", "ast.azl", "parser.azl", "dump.azl", "sema.azl", "sema_fn.azl",
           "x64.azl", "x64_gen.azl", "runtime_x64.azl", "code_file.azl", "elf.azl", "bytes.azl", "diag.azl", "diag_table.azl",
           "embedded.azl", "format.azl"]
SETS = check_sema.SETS + [["compiler/" + name for name in SOURCES]]


def run(command, **more):
    return subprocess.run(command, capture_output=True, **more)


def first_difference(a, b):
    for at, (x, y) in enumerate(zip(a, b)):
        if x != y:
            return at
    return min(len(a), len(b))


def rust_build(paths, out):
    return run([AZOL, "build", "--target", "x86_64-linux", "-o", out] + paths)


def self_build(tool, paths, out):
    return run([tool, "build", "-o", out] + paths)


def compare(tool, paths, shown, work, counts):
    """Returns None when both compilers agree on the program, else a description."""
    theirs, ours = os.path.join(work, "theirs"), os.path.join(work, "ours")
    for path in (theirs, ours):
        if os.path.exists(path):
            os.remove(path)
    want = rust_build(paths, theirs)
    got = self_build(tool, paths, ours)
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
    counts["built"] += 1
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
        sys.exit("build the compiler first: cargo build --release")
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
    for generator in ["gen_runtime_azl.py", "gen_tables_azl.py"]:
        stale = run([sys.executable, os.path.join(HERE, generator), "--check"])
        if stale.returncode != 0:
            sys.exit(stale.stderr.decode().strip())
    sources = [os.path.join(HERE, name) for name in SOURCES]

    # The compiler compiles itself.
    stage1 = os.path.join(work, "azol-self")
    built = rust_build(sources, stage1)
    if built.returncode != 0:
        sys.exit("azol-self does not build:\n" + built.stderr.decode(errors="replace"))
    stage2, stage3 = os.path.join(work, "stage2"), os.path.join(work, "stage3")
    for tool, out in [(stage1, stage2), (stage2, stage3)]:
        built = self_build(tool, sources, out)
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

    # The same executables, and the same errors.
    groups = []
    files = []
    for pattern in ["tests/programs/*.azl", "tests/multi/*/*.azl", "examples/*.azl", "bench/*.azl", "compiler/*.azl", "src/prelude.azl"]:
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
    counts = {"built": 0, "refused": 0}
    for group, entries in groups:
        for paths, shown in entries:
            problem = compare(stage2, paths, group + shown, work, counts)
            if problem:
                print("DIFFERENT " + problem)
                return 1
            print(f"same  {group}{shown}")
            count += 1

    if not quick:
        if not os.path.exists(BIG):
            sys.exit("bench/generated/big.azl is missing: python3 bench/gen.py")
        problem = compare(stage2, [BIG], "bench/generated/big.azl", work, counts)
        if problem:
            print("DIFFERENT " + problem)
            return 1
        print("same  bench/generated/big.azl", flush=True)
        count += 1

        out = os.path.join(work, "big")
        ours = [stage2, "build", "-o", out, BIG]
        theirs = [AZOL, "build", "--target", "x86_64-linux", "-o", out, BIG]
        print()
        print("big.azl, source to executable, best of 7, whole process:")
        print(f"  azol-self build   {best_time(ours):7.1f} ms")
        print(f"  azol build        {best_time(theirs):7.1f} ms")
        a, b = instructions(ours), instructions(theirs)
        if a and b:
            print(f"  instructions executed: azol-self {a / 1e6:.0f} million, azol {b / 1e6:.0f} million ({a / b:.1f} times)")
        inner = run(theirs).stderr.decode().strip()
        print(f"  azol's own clock: {inner}")

    print()
    print(f"{count} comparisons: {counts['built']} programs built to the same executable, {counts['refused']} refused with the same errors")
    print("ALL IDENTICAL")
    return 0


if __name__ == "__main__":
    sys.exit(main())
