#!/usr/bin/env python3
"""Measures what one `azol build` costs: wall time and CPU time.

    python3 bench/cpu_time.py [runs] [source.azl]

CPU time is user plus system time of the compiler process, all of its
threads together, so it can be larger than the wall time. Run from the
repository root after `cargo build --release`. The source defaults to
bench/generated/big.azl (write it with `python3 bench/gen.py 3000`).
"""
import os, statistics, sys, tempfile, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
AZOL = os.path.join(ROOT, "target", "release", "azol")
runs = int(sys.argv[1]) if len(sys.argv) > 1 else 20
source = sys.argv[2] if len(sys.argv) > 2 else os.path.join(ROOT, "bench", "generated", "big.azl")

def build(target, out):
    """Runs one build and returns (wall seconds, CPU seconds)."""
    started = time.perf_counter()
    pid = os.fork()
    if pid == 0:
        null = os.open(os.devnull, os.O_WRONLY)
        os.dup2(null, 1)
        os.dup2(null, 2)
        os.execv(AZOL, [AZOL, "build", "--target", target, source, "-o", out])
    _, status, usage = os.wait4(pid, 0)
    wall = time.perf_counter() - started
    if status != 0:
        sys.exit(f"azol build failed for {target}")
    return wall, usage.ru_utime + usage.ru_stime

for path in (AZOL, source):
    if not os.path.exists(path):
        sys.exit(f"missing {path}")
print(f"{runs} builds of {os.path.relpath(source, ROOT)} per target; times in ms")
print(f"{'target':16} {'wall best':>10} {'wall median':>12} {'CPU best':>10} {'CPU median':>11}")
with tempfile.TemporaryDirectory() as tmp:
    for target in ("aarch64-macos", "x86_64-linux"):
        results = [build(target, os.path.join(tmp, "out")) for _ in range(runs)]
        wall = [w * 1000 for w, _ in results]
        cpu = [c * 1000 for _, c in results]
        print(f"{target:16} {min(wall):10.1f} {statistics.median(wall):12.1f} {min(cpu):10.1f} {statistics.median(cpu):11.1f}")
