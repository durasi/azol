#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""The allocator of the ARM64 runtime when the system runs out of memory.

    python3 tests/opt/run_heap_tests.py [--azol PATH]

tests/programs/allocator_stress.azl exercises the allocator on every
target, natively and in the checking emulators, with every other test
program. What no program can reach cheaply on a real machine is the end
of memory, so this is done in the macOS emulator, whose stand-in for the
system's `malloc` has 64 MB to give: tests/opt/heap/out_of_memory.azl,
built with and without `-O`, must stop with `azol: out of memory` and
exit code 1.

Needs the checking emulators (see run_back_arm64_tests.py); without them
it says so and checks nothing.
"""
import os
import subprocess
import sys
import tempfile

import run_back_arm64_tests as arm64

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))


def main():
    azol = os.path.join(ROOT, "azol")
    if "--azol" in sys.argv:
        azol = os.path.abspath(sys.argv[sys.argv.index("--azol") + 1])
    if arm64.emulators() is None:
        print("heap: the checking emulators are not here; nothing checked")
        return 0
    source = os.path.join(HERE, "heap", "out_of_memory.azl")
    problems = []
    done = 0
    with tempfile.TemporaryDirectory(prefix="azol-heap-") as folder:
        for mode in ([], ["-O"]):
            exe = os.path.join(folder, "oom" + "".join(mode))
            built = subprocess.run([azol, "build"] + mode + ["--target", "aarch64-macos", source, "-o", exe], capture_output=True)
            label = "out_of_memory.azl %s" % (" ".join(mode) or "default build")
            if built.returncode != 0:
                problems.append("%s: does not build: %s" % (label, built.stderr.decode(errors="replace")[:200]))
                continue
            code, out, err, leaks, complaint = arm64.run_exe("aarch64-macos", exe)
            if complaint is not None:
                problems.append("%s: %s" % (label, complaint))
            elif (code, out, err) != (1, b"", b"azol: out of memory\n"):
                problems.append("%s: exit %r, output %r, error output %r; expected exit 1 and `azol: out of memory`" % (label, code, out[:60], err[:80]))
            done += 1
    for problem in problems:
        print("FAILED  heap: " + problem)
    if problems:
        return 1
    print("heap: %d builds of a program that uses up memory stop with `azol: out of memory` (macOS emulator)" % done)
    return 0


if __name__ == "__main__":
    sys.exit(main())
