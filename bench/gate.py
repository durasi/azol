#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""The gate: is azoL as fast as `rustc -O` on the benchmark suite?

    python3 bench/gate.py                  build, check, time, print the table and the verdict
    python3 bench/gate.py --list           the programs of the suite
    python3 bench/gate.py --only sieve,sort
    python3 bench/gate.py --instructions   also count instructions on the small sizes (needs valgrind)
    python3 bench/gate.py --readme         print only the block for the README
    python3 bench/gate.py --force          give a verdict even when the machine is busy

Every program in bench/suite exists as `<name>.azl` and `<name>.rs`
(sometimes also `<name>.fx.rs`, the same Rust program with a cheaper
hasher; the faster Rust variant counts). All builds of a program must
print the same output, or the gate fails. Time is the best wall time of
the whole process over several runs. The rule:

    geometric mean of (azoL time / Rust -O time) <= 1.00
    and no program above 1.10

Results against `rustc -C opt-level=3 -C target-cpu=native` are printed
beside them. Needs Python 3, an `azol` executable and `rustc`; nothing
else. Runs unchanged on macOS and Linux.
"""

import argparse
import datetime
import json
import math
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SUITE = os.path.join(HERE, "suite")
BOUND = 1.10
MEAN = 1.00
START = "<!-- gate:start -->"
END = "<!-- gate:end -->"


def find_azol(given):
    for path in [given, os.path.join(ROOT, "azol")]:
        if path and os.path.isfile(path) and os.access(path, os.X_OK):
            return os.path.abspath(path)
    sys.exit("gate: no azol executable found; build one with `sh build.sh` or pass --azol <path>")


def programs(only):
    names, pending = [], []
    for file in sorted(os.listdir(SUITE)):
        if file.endswith(".azl.todo"):
            pending.append(file[: -len(".azl.todo")])
        elif file.endswith(".azl") and os.path.exists(os.path.join(SUITE, file[:-4] + ".rs")):
            names.append(file[:-4])
    if only:
        unknown = [name for name in only if name not in names]
        if unknown:
            sys.exit("gate: not in the suite: " + ", ".join(unknown))
        names = [name for name in names if name in only]
    return names, pending


def run(command, **more):
    return subprocess.run(command, capture_output=True, **more)


def version_of(command):
    try:
        done = run(command)
        return (done.stdout or done.stderr).decode(errors="replace").strip().splitlines()[0]
    except (OSError, IndexError):
        return "not found"


def small_source(text, rust):
    if rust:
        new, count = re.subn(r"^const SMALL: bool = false;$", "const SMALL: bool = true;", text, flags=re.M)
    else:
        new, count = re.subn(r"^const SMALL = false$", "const SMALL = true", text, flags=re.M)
    if count != 1:
        raise ValueError("the line that sets SMALL to false is missing")
    return new


def has_optimiser(azol, work):
    """True when `azol build -O` exists."""
    probe = os.path.join(work, "probe.azl")
    with open(probe, "w") as handle:
        handle.write("fn main() {\n    print(1)\n}\n")
    return run([azol, "build", "-O", probe, "-o", os.path.join(work, "probe")]).returncode == 0


def build_all(azol, name, work, optimised, small):
    """Builds every variant of one program. Returns {label: executable} or raises."""
    tag = "small" if small else "full"
    built = {}
    source = open(os.path.join(SUITE, name + ".azl"), encoding="utf-8").read()
    if small:
        source = small_source(source, False)
    path = os.path.join(work, name + ".azl")
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(source)
    exe = os.path.join(work, "%s_azol_%s" % (name, tag))
    done = run([azol, "build"] + (["-O"] if optimised else []) + [path, "-o", exe])
    if done.returncode != 0:
        raise RuntimeError("azol build fails for %s:\n%s" % (name, done.stderr.decode(errors="replace")))
    built["azol"] = exe
    for variant in ("rs", "fx.rs"):
        file = os.path.join(SUITE, "%s.%s" % (name, variant))
        if not os.path.exists(file):
            continue
        source = open(file, encoding="utf-8").read()
        if small:
            source = small_source(source, True)
        path = os.path.join(work, "%s_%s.rs" % (name, variant.replace(".", "_")))
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(source)
        for mode, flags in (("rust", ["-O"]), ("native", ["-C", "opt-level=3", "-C", "target-cpu=native"])):
            if small and mode == "native":
                continue
            exe = os.path.join(work, "%s_%s_%s_%s" % (name, mode, variant.replace(".", "_"), tag))
            done = run(["rustc", "--edition", "2021", "--crate-name", "bench"] + flags + [path, "-o", exe])
            if done.returncode != 0:
                raise RuntimeError("rustc fails for %s.%s:\n%s" % (name, variant, done.stderr.decode(errors="replace")))
            built["%s:%s" % (mode, variant)] = exe
    return built


def output_of(exe):
    done = run([exe])
    return done.returncode, done.stdout


def best_time(exe, runs):
    run([exe])
    best = None
    for _ in range(runs):
        start = time.perf_counter()
        subprocess.run([exe], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        spent = time.perf_counter() - start
        best = spent if best is None else min(best, spent)
    return best


def instructions(exe):
    done = run(["valgrind", "--tool=callgrind", "--callgrind-out-file=/dev/null", exe])
    found = re.search(rb"Collected : (\d+)", done.stderr)
    return int(found.group(1)) if found else None


def calibrate():
    """Seconds for a fixed piece of work."""
    start = time.perf_counter()
    total = 0
    for n in range(1500000):
        total += n * n % 7
    return time.perf_counter() - start


def busy():
    """A reason the machine cannot give a fair timing, or None."""
    times = [calibrate() for _ in range(6)]
    spread = max(times) / min(times)
    if spread > 1.20:
        return "the same loop took between %.0f and %.0f ms" % (min(times) * 1000, max(times) * 1000)
    try:
        load = os.getloadavg()[0]
    except (OSError, AttributeError):
        return None
    limit = max(1.0, (os.cpu_count() or 1) / 4)
    if load > limit:
        return "the load average is %.1f (limit for a verdict here: %.1f)" % (load, limit)
    return None


def seconds(value):
    if value is None:
        return "n/a"
    return "%.3f s" % value if value < 1 else "%.2f s" % value


def ratio(value):
    return "n/a" if value is None else "%.2f" % value


def geometric_mean(values):
    return math.exp(sum(math.log(value) for value in values) / len(values)) if values else None


def render(results):
    info = results["machine"]
    lines = [START, ""]
    lines.append("Measured on %s, %s %s (%s), %s. Best of %d runs of the whole process." % (
        info["cpu"], info["system"], info["release"], info["arch"], results["date"], results["runs"]))
    lines.append("Tools: %s; %s. azoL mode: %s." % (info["azol"], info["rustc"], results["mode"]))
    lines.append("")
    lines.append("| Program | azoL | Rust `-O` | Ratio | Rust native | Ratio | Within %.2f |" % BOUND)
    lines.append("|---|---|---|---|---|---|---|")
    for row in results["programs"]:
        if not row["same_output"]:
            verdict = "void: outputs differ"
        elif row["ratio"] is None:
            verdict = "not timed"
        else:
            verdict = "yes" if row["ratio"] <= BOUND else "**no**"
        lines.append("| `%s` | %s | %s | %s | %s | %s | %s |" % (
            row["name"], seconds(row["azol"]), seconds(row["rust"]), ratio(row["ratio"]),
            seconds(row["native"]), ratio(row["native_ratio"]), verdict))
    lines.append("")
    lines.append("Geometric mean of the ratios: **%s** against `rustc -O`, %s against `-C opt-level=3 -C target-cpu=native`." % (
        ratio(results["mean"]), ratio(results["native_mean"])))
    if results["pending"]:
        lines.append("Not in the gate yet (they need `f64`): %s." % ", ".join("`%s`" % name for name in results["pending"]))
    if any("instructions" in row for row in results["programs"]):
        lines.append("")
        lines.append("Instructions executed on the small sizes (callgrind):")
        lines.append("")
        lines.append("| Program | azoL | Rust `-O` | Ratio |")
        lines.append("|---|---|---|---|")
        for row in results["programs"]:
            if "instructions" in row:
                az, rs = row["instructions"]["azol"], row["instructions"]["rust"]
                lines.append("| `%s` | %s | %s | %s |" % (row["name"], "{:,}".format(az), "{:,}".format(rs), ratio(az / rs if az and rs else None)))
        lines.append("")
        lines.append("Geometric mean of the instruction ratios: **%s**." % ratio(results.get("instruction_mean")))
    lines.append("")
    lines.append(results["verdict"])
    lines.append("")
    lines.append(END)
    return "\n".join(lines)


def cpu_name():
    if platform.system() == "Darwin":
        return version_of(["sysctl", "-n", "machdep.cpu.brand_string"])
    try:
        for line in open("/proc/cpuinfo", encoding="utf-8", errors="replace"):
            if line.lower().startswith(("model name", "hardware")):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or platform.machine()


def main():
    parser = argparse.ArgumentParser(description="The gate: azoL against rustc -O on the benchmark suite.")
    parser.add_argument("--azol", help="the azol executable (default: ./azol)")
    parser.add_argument("--only", help="comma-separated program names")
    parser.add_argument("--list", action="store_true", help="list the programs and stop")
    parser.add_argument("--runs", type=int, default=10, help="timed runs per executable (default 10)")
    parser.add_argument("--instructions", action="store_true", help="count instructions on the small sizes with valgrind")
    parser.add_argument("--no-time", action="store_true", help="skip wall-clock timing (with --instructions)")
    parser.add_argument("--force", action="store_true", help="give a verdict even when the machine is busy")
    parser.add_argument("--readme", action="store_true", help="print only the block for the README")
    parser.add_argument("--keep", help="keep the built files in this folder")
    args = parser.parse_args()

    names, pending = programs(args.only.split(",") if args.only else None)
    if args.list:
        for name in names:
            variants = "azl, rs" + (", fx.rs" if os.path.exists(os.path.join(SUITE, name + ".fx.rs")) else "")
            print("%-14s %s" % (name, variants))
        for name in pending:
            print("%-14s waits for f64 (azl.todo, rs)" % name)
        return 0
    if shutil.which("rustc") is None:
        sys.exit("gate: rustc is not installed; the gate compares against it")
    if args.instructions and shutil.which("valgrind") is None:
        sys.exit("gate: --instructions needs valgrind")
    azol = find_azol(args.azol)
    say = (lambda *words: None) if args.readme else (lambda *words: print(*words, file=sys.stderr, flush=True))

    reason = None if args.no_time else busy()
    if reason and not args.force:
        say("gate: this machine is busy: %s." % reason)
        say("gate: timings would not be fair. Close other programs and run again, or pass --force.")
    work = args.keep or tempfile.mkdtemp(prefix="azol-gate-")
    os.makedirs(work, exist_ok=True)
    optimised = has_optimiser(azol, work)
    mode = "`azol build -O`" if optimised else "default build (this compiler has no `-O` yet)"
    rows, failures = [], []
    for name in names:
        say("gate: %s ..." % name)
        row = {"name": name, "same_output": True, "azol": None, "rust": None, "native": None, "ratio": None, "native_ratio": None}
        try:
            full = build_all(azol, name, work, optimised, False)
        except (RuntimeError, ValueError) as problem:
            failures.append(str(problem))
            row["same_output"] = False
            rows.append(row)
            continue
        outputs = {label: output_of(exe) for label, exe in full.items()}
        if len(set(outputs.values())) != 1 or outputs["azol"][0] != 0:
            row["same_output"] = False
            failures.append("%s: the builds do not print the same output: %s" % (
                name, {label: (code, out[:80]) for label, (code, out) in outputs.items()}))
        if not args.no_time:
            times = {label: best_time(exe, args.runs) for label, exe in full.items()}
            row["azol"] = times["azol"]
            row["rust"] = min(value for label, value in times.items() if label.startswith("rust:"))
            row["native"] = min(value for label, value in times.items() if label.startswith("native:"))
            row["rust_variant"] = min((value, label) for label, value in times.items() if label.startswith("rust:"))[1].split(":")[1]
            row["ratio"] = row["azol"] / row["rust"]
            row["native_ratio"] = row["azol"] / row["native"]
        if args.instructions:
            try:
                small = build_all(azol, name, work, optimised, True)
                small_outputs = {label: output_of(exe) for label, exe in small.items()}
                if len(set(small_outputs.values())) != 1:
                    row["same_output"] = False
                    failures.append("%s: the small builds do not print the same output" % name)
                counts = {label: instructions(exe) for label, exe in small.items()}
                rust = [value for label, value in counts.items() if label.startswith("rust:") and value]
                row["instructions"] = {"azol": counts["azol"], "rust": min(rust) if rust else None}
            except (RuntimeError, ValueError) as problem:
                failures.append(str(problem))
        rows.append(row)
    if not args.keep:
        shutil.rmtree(work, ignore_errors=True)

    timed = [row for row in rows if row["ratio"] is not None and row["same_output"]]
    mean = geometric_mean([row["ratio"] for row in timed])
    native_mean = geometric_mean([row["native_ratio"] for row in timed])
    counted = [row["instructions"]["azol"] / row["instructions"]["rust"] for row in rows
               if "instructions" in row and row["instructions"]["azol"] and row["instructions"]["rust"]]
    above = [row["name"] for row in timed if row["ratio"] > BOUND]
    void = [row["name"] for row in rows if not row["same_output"]]
    if void:
        verdict, code = "GATE: FAIL (void: %s)" % ", ".join(void), 1
    elif args.no_time:
        verdict, code = "GATE: NO VERDICT (no timing was asked for)", 2
    elif reason and not args.force:
        verdict, code = "GATE: NO VERDICT (the machine is busy: %s)" % reason, 2
    elif args.only:
        verdict, code = "GATE: NO VERDICT (only part of the suite was run)", 2
    elif not above and mean is not None and mean <= MEAN:
        verdict, code = "GATE: PASS (%d programs, geometric mean %.2f, none above %.2f)" % (len(timed), mean, BOUND), 0
    else:
        verdict, code = "GATE: FAIL (%d programs above %.2f, geometric mean %.2f)" % (len(above), BOUND, mean), 1
    if reason and args.force:
        verdict += " [measured on a busy machine: %s]" % reason
    system = platform.system()
    results = {
        "date": datetime.date.today().isoformat(),
        "runs": args.runs,
        "mode": mode,
        "machine": {"cpu": cpu_name(), "system": system, "release": platform.release(), "arch": platform.machine(),
                    "azol": version_of([azol, "--version"]), "rustc": version_of(["rustc", "--version"])},
        "programs": rows, "pending": pending, "mean": mean, "native_mean": native_mean,
        "instruction_mean": geometric_mean(counted), "bound": BOUND, "verdict": verdict, "failures": failures,
    }
    out = os.path.join(SUITE, "results-%s-%s.json" % (platform.machine().lower(), system.lower()))
    if not args.only:
        with open(out, "w", encoding="utf-8") as handle:
            json.dump(results, handle, indent=2)
            handle.write("\n")
    print(render(results))
    for failure in failures:
        print(failure, file=sys.stderr)
    return code


if __name__ == "__main__":
    sys.exit(main())
