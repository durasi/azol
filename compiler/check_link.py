#!/usr/bin/env python3
"""Checks the linkers written in azoL against the ones in the Rust compiler.

For every test program and example, and for both targets, the Rust
compiler builds the executable and also writes the code generator's
result to a code file. `azol-link`, built here from the azoL sources in
this folder, turns that code file into an executable of its own. The two
files must be the same, byte for byte.

    python3 compiler/check_link.py          every program in tests/programs and examples
    python3 compiler/check_link.py --big    also bench/generated/big.azl, with timings

`azol-link` is built for x86-64 Linux and run directly, so this script
needs such a machine. The compiler it compares with is the oracle: `--oracle <path>`, or see
compiler/oracle.py for the one taken when none is given.
"""
import glob, os, platform, shutil, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import oracle  # noqa: E402

AZOL = oracle.path()
RUNTIME = os.path.join(ROOT, "runtime", "linux_x64", "runtime.bin")
SOURCES = ["link_main.azl", "code_file.azl", "macho.azl", "elf.azl", "sha256.azl", "bytes.azl"]
TARGETS = ["aarch64-macos", "x86_64-linux"]


def run(command):
    return subprocess.run(command, capture_output=True, text=True)


def first_difference(a, b):
    for at, (x, y) in enumerate(zip(a, b)):
        if x != y:
            return at
    return min(len(a), len(b))


def main():
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        sys.exit("check_link.py runs azol-link directly and so needs x86-64 Linux")
    if not os.path.exists(AZOL):
        sys.exit(oracle.missing())
    work = tempfile.mkdtemp(prefix="azol-check-link-")
    try:
        return check(work)
    finally:
        shutil.rmtree(work, ignore_errors=True)


def check(work):
    link = os.path.join(work, "azol-link")
    built = run([AZOL, "build", "--target", "x86_64-linux", "-o", link] + [os.path.join(HERE, name) for name in SOURCES])
    if built.returncode != 0:
        sys.exit("azol-link does not build:\n" + built.stderr)

    # The hashing the signature rests on, checked on its own first.
    sha_test = os.path.join(work, "sha256-test")
    built = run([AZOL, "build", "--target", "x86_64-linux", "-o", sha_test]
                + [os.path.join(HERE, name) for name in ["sha256_test.azl", "sha256.azl", "bytes.azl"]])
    tested = run([sha_test]) if built.returncode == 0 else built
    if tested.returncode != 0 or tested.stdout != "ok\n":
        sys.exit("sha256_test fails:\n" + tested.stdout + tested.stderr)
    print("sha256             known hashes ok")

    programs = sorted(glob.glob(os.path.join(ROOT, "tests", "programs", "*.azl")))
    programs += sorted(p for p in glob.glob(os.path.join(ROOT, "examples", "*.azl")) if os.path.basename(p) != "point.azl")
    big = os.path.join(ROOT, "bench", "generated", "big.azl")
    if "--big" in sys.argv[1:]:
        if not os.path.exists(big):
            sys.exit("bench/generated/big.azl is missing: python3 bench/gen.py 3000")
        programs.append(big)

    failures = 0
    for path in programs:
        name = os.path.splitext(os.path.basename(path))[0]
        for target in TARGETS:
            # Both files get the same name, in different folders: the
            # Mach-O signature contains the name.
            theirs_dir = os.path.join(work, "rust", target)
            ours_dir = os.path.join(work, "azol", target)
            os.makedirs(theirs_dir, exist_ok=True)
            os.makedirs(ours_dir, exist_ok=True)
            theirs, ours = os.path.join(theirs_dir, name), os.path.join(ours_dir, name)
            code_file = os.path.join(work, name + "." + target + ".code")
            built = run([AZOL, "build", "--target", target, path, "-o", theirs, "--emit-code", code_file])
            if built.returncode != 0:
                sys.exit(f"{path} does not build for {target}:\n{built.stderr}")
            started = time.time()
            linked = run([link, code_file, ours] + ([RUNTIME] if target == "x86_64-linux" else []))
            seconds = time.time() - started
            label = f"{name:18} {target:14}"
            if linked.returncode != 0:
                print(f"{label} azol-link failed: {linked.stderr.strip()}")
                failures += 1
                continue
            a, b = open(theirs, "rb").read(), open(ours, "rb").read()
            runnable = os.access(ours, os.X_OK)
            if a == b and runnable:
                timing = f"  ({seconds:.2f} s)" if path == big else ""
                print(f"{label} identical, {len(a)} bytes{timing}")
            elif a == b:
                print(f"{label} identical but not marked executable")
                failures += 1
            else:
                print(f"{label} DIFFERENT: first at byte {first_difference(a, b)}, sizes {len(a)} and {len(b)}")
                failures += 1
            os.remove(code_file)
    if failures:
        print(f"{failures} DIFFERENT")
        return 1
    print("ALL IDENTICAL")
    return 0


if __name__ == "__main__":
    sys.exit(main())
