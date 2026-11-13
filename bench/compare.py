#!/usr/bin/env python3
"""Compares azoL with Rust and JavaScript (Node.js) on this machine.

    sh build.sh
    python3 bench/compare.py              everything, best of 5 runs
    python3 bench/compare.py --runs 9     more runs per measurement
    python3 bench/compare.py fib40 sort   only the named benchmarks
    python3 bench/compare.py --write-readme
                                          a full run that also replaces the
                                          tables in README.md, between the
                                          `<!-- bench:start -->` and
                                          `<!-- bench:end -->` lines
    python3 bench/compare.py --from-results [--write-readme]
                                          no measuring: the tables again from
                                          the bench/results.json of the last run

It prints Markdown tables and writes the raw numbers to bench/results.json.

What is compared
  Run time     every benchmark in this folder that exists as .azl, .rs and
               .js: the same algorithm, written plainly in each language,
               printing one number. All versions must print the same number;
               a benchmark whose outputs differ is reported as broken and
               left out of the tables.
  Build time   one generated program of about 130,000 lines (bench/gen.py),
               the same in the three languages.
  Size         the executables of a small program.

What to keep in mind when reading the numbers
  - Run times are whole processes, start to exit, so they include start-up:
    a few milliseconds for azoL and Rust, tens of milliseconds for Node.
  - JavaScript is compiled while it runs (JIT) and collects garbage; short
    runs favour the others, long hot loops favour its optimizing compiler.
  - Rust is measured twice: plain `rustc` (no optimization, what a normal
    development build is) and `rustc -O`.
  - azoL has one mode. It has no optimizer passes beyond what its one-pass
    code generator does, and it checks every index and division.
  - The best of several runs is reported, which hides noise but also hides
    variation. These are small programs, not a benchmark suite.
"""
import json
import os
import platform
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
BUILD = os.path.join(HERE, "build")
GENERATED = os.path.join(HERE, "generated")
AZOL = os.environ.get("AZOL") or os.path.join(ROOT, "azol")
UNITS = 3000
README_START = "<!-- bench:start -->"
README_END = "<!-- bench:end -->"

# What each benchmark mostly exercises, for the table.
ABOUT = {
    "fib40": "function calls (recursive `fib(40)`)",
    "loops": "nested loops and arithmetic",
    "sort": "quicksort of 2 million numbers",
    "sieve": "prime sieve up to 20 million",
    "particles": "list of structs updated in place",
    "tree": "building and walking trees",
    "append": "building long strings",
    "words": "splitting and joining text",
}


def run(cmd, cwd=None):
    """Runs a command; returns (seconds, exit code, stdout, stderr)."""
    start = time.perf_counter()
    done = subprocess.run(cmd, cwd=cwd, capture_output=True)
    return time.perf_counter() - start, done.returncode, done.stdout.decode(errors="replace"), done.stderr.decode(errors="replace")


def best(cmd, runs, cwd=None):
    """Best time of `runs` runs, and the output of the last one. None if it fails."""
    times, out = [], ""
    for _ in range(runs):
        seconds, code, out, err = run(cmd, cwd)
        if code != 0:
            return None, (out + err).strip()[-300:]
        times.append(seconds)
    return min(times), out


def first_line(cmd):
    try:
        _, code, out, err = run(cmd)
        text = (out or err).strip()
        return text.splitlines()[0] if code == 0 and text else None
    except OSError:
        return None


def machine():
    cpu = None
    if sys.platform == "darwin":
        cpu = first_line(["sysctl", "-n", "machdep.cpu.brand_string"])
    elif os.path.exists("/proc/cpuinfo"):
        for line in open("/proc/cpuinfo"):
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    return {
        "cpu": cpu or platform.processor() or "unknown",
        "os": "%s %s (%s)" % (platform.system(), platform.release(), platform.machine()),
        "azol": first_line([AZOL, "--version"]),
        "rustc": first_line(["rustc", "--version"]) if shutil.which("rustc") else None,
        "node": first_line(["node", "--version"]) if shutil.which("node") else None,
    }


def seconds(value):
    if value is None:
        return "n/a"
    if value < 0.1:
        return "%.3f s" % value
    return "%.2f s" % value


def size(path):
    return os.path.getsize(path) if path and os.path.exists(path) else None


def kilobytes(value):
    return "n/a" if value is None else "%d KB" % round(value / 1024)


def benchmarks(only):
    names = []
    for file in sorted(os.listdir(HERE)):
        name, ext = os.path.splitext(file)
        if ext == ".azl" and all(os.path.exists(os.path.join(HERE, name + other)) for other in (".rs", ".js")):
            if not only or name in only:
                names.append(name)
    order = list(ABOUT)
    return sorted(names, key=lambda name: (order.index(name) if name in order else len(order), name))


def build_variants(name, have_rust, have_node, problems):
    """Builds one benchmark; returns {variant: command to run it}."""
    variants = {}
    exe = os.path.join(BUILD, name + "-azol")
    _, code, out, err = run([AZOL, "build", os.path.join(HERE, name + ".azl"), "-o", exe])
    if code == 0:
        variants["azol"] = [exe]
    else:
        problems.append("%s: azoL build failed: %s" % (name, err.strip()[-300:]))
    if have_rust:
        for variant, flags in (("rust_opt", ["-O"]), ("rust_debug", [])):
            exe = os.path.join(BUILD, "%s-%s" % (name, variant))
            _, code, out, err = run(["rustc"] + flags + [os.path.join(HERE, name + ".rs"), "-o", exe])
            if code == 0:
                variants[variant] = [exe]
            else:
                problems.append("%s: rustc %s failed: %s" % (name, " ".join(flags), err.strip()[-300:]))
    if have_node:
        variants["node"] = ["node", os.path.join(HERE, name + ".js")]
    return variants


def measure_runs(names, runs, have_rust, have_node, problems):
    results = {}
    for name in names:
        print("running %s ..." % name, file=sys.stderr)
        variants = build_variants(name, have_rust, have_node, problems)
        times, outputs = {}, {}
        for variant, cmd in variants.items():
            took, out = best(cmd, runs)
            if took is None:
                problems.append("%s: the %s version failed to run: %s" % (name, variant, out))
                continue
            times[variant], outputs[variant] = took, out.strip()
        if len(set(outputs.values())) > 1:
            problems.append("%s: the versions print different results, so it is left out: %s" % (name, outputs))
            continue
        if "azol" not in times:
            continue
        results[name] = {"seconds": times, "output": next(iter(outputs.values()), "")}
    return results


def measure_build(runs, have_rust, have_node, problems):
    """Build time of the generated program, and a check that all versions agree."""
    files = [os.path.join(GENERATED, "big" + ext) for ext in (".azl", ".rs", ".js")]
    if not all(os.path.exists(file) for file in files):
        print("generating the large program ...", file=sys.stderr)
        _, code, out, err = run([sys.executable, os.path.join(HERE, "gen.py"), str(UNITS)])
        if code != 0:
            problems.append("bench/gen.py failed: " + err.strip()[-300:])
            return None
    big_azl, big_rs, big_js = files
    lines = sum(1 for _ in open(big_azl))
    result = {"lines": lines, "seconds": {}, "outputs": {}}
    print("building the large program ...", file=sys.stderr)

    exe = os.path.join(BUILD, "big-azol")
    result["seconds"]["azol"], _ = best([AZOL, "build", big_azl, "-o", exe], runs)
    if result["seconds"]["azol"] is not None:
        result["outputs"]["azol"] = run([exe])[2].strip()
    if have_rust:
        rust_runs = min(runs, 3)  # each takes seconds
        exe = os.path.join(BUILD, "big-rust")
        result["seconds"]["rust_check"], _ = best(
            ["rustc", "--emit=metadata", big_rs, "--out-dir", BUILD, "-A", "warnings"], rust_runs)
        result["seconds"]["rust_debug"], _ = best(["rustc", big_rs, "-o", exe, "-A", "warnings"], rust_runs)
        if result["seconds"]["rust_debug"] is not None:
            result["outputs"]["rust"] = run([exe])[2].strip()
        result["seconds"]["rust_opt"], _ = best(["rustc", "-O", big_rs, "-o", exe + "-opt", "-A", "warnings"], 1)
    if have_node:
        result["seconds"]["node_check"], _ = best(["node", "--check", big_js], runs)
        result["seconds"]["node_run"], out = best(["node", big_js], runs)
        if result["seconds"]["node_run"] is not None:
            result["outputs"]["node"] = out.strip()
    if len(set(result["outputs"].values())) > 1:
        problems.append("the large program prints different results: %s" % result["outputs"])
        return None
    return result


def measure_sizes(have_rust, problems):
    """Size of the executables of the smallest benchmark."""
    sizes = {}
    for variant in ("azol", "rust_opt", "rust_debug"):
        sizes[variant] = size(os.path.join(BUILD, "fib40-" + variant))
    return sizes


def table(header, rows):
    """A Markdown table whose first column is text and whose others are numbers."""
    out = ["| " + " | ".join(header) + " |", "|:---|" + "---:|" * (len(header) - 1)]
    out.extend("| " + " | ".join(row) + " |" for row in rows)
    return "\n".join(out)


def picture(name, about):
    """The two drawings of bench/chart.py, one for light pages and one for dark."""
    return ('<p><img src="assets/bench/%s-light.svg#gh-light-mode-only" alt="%s" width="760">'
            '<img src="assets/bench/%s-dark.svg#gh-dark-mode-only" alt="%s" width="760"></p>\n' % (name, about, name, about))


def ratio(value):
    """A ratio for prose: `5.8`, `70`, `950`."""
    if value >= 100:
        return "%d" % (round(value / 10) * 10)
    if value >= 10:
        return "%d" % round(value)
    return "%.1f" % value


def summary(run_results, build_result):
    """Sentences worked out from the numbers, so that they stay true."""
    out = []
    if build_result:
        times = build_result["seconds"]
        azol = times.get("azol")
        rust = [times.get(key) for key in ("rust_check", "rust_debug", "rust_opt")]
        if azol and all(rust):
            out.append("Build: azoL is %s times faster than Rust's type checking alone, %s times faster than an "
                       "unoptimized Rust build and %s times faster than an optimized one."
                       % tuple(ratio(value / azol) for value in rust))
        if azol and times.get("node_check"):
            out.append("Node.js checks the syntax of the JavaScript version in %s times azoL's whole build time."
                       % ratio(times["node_check"] / azol))
    full = {name: result["seconds"] for name, result in run_results.items()
            if all(result["seconds"].get(key) for key in ("azol", "rust_opt", "rust_debug", "node"))}
    if full:
        count = len(full)
        behind = sorted(((times["azol"] / times["rust_opt"], name) for name, times in full.items()), reverse=True)
        slower = [item for item in behind if item[0] > 1.0]
        line = ("Run: azoL is faster than unoptimized Rust in %d of %d and faster than Node.js in %d of %d. "
                % (sum(times["azol"] < times["rust_debug"] for times in full.values()), count,
                   sum(times["azol"] < times["node"] for times in full.values()), count))
        if slower:
            line += "It is slower than optimized Rust in %d of %d" % (len(slower), count)
            line += "; the largest gaps are %s." % " and ".join(
                "`%s` (%s times)" % (name, ratio(value)) for value, name in slower[:2])
        else:
            line += "It is at least as fast as optimized Rust in all of them."
        out.append(line)
    return out


def report(results):
    """The Markdown report, as a list of lines, from what `results.json` holds."""
    info, runs = results["machine"], results["runs"]
    run_results, build_result, sizes = results["run_time"], results["build_time"], results["sizes"]
    have_rust, have_node = bool(info.get("rustc")), bool(info.get("node"))
    out = []
    out.append("Measured on %s, %s. Best of %d runs." % (info["cpu"], info["os"], runs))
    out.append("Tools: %s." % ", ".join(str(info[tool]) for tool in ("azol", "rustc", "node") if info[tool]))
    out.append("")
    if run_results:
        out.append(picture("run", "azoL's run time against optimized Rust, program by program"))
        out.append("**Run time** (whole process, lower is better)\n")
        rows = []
        for name, result in run_results.items():
            times = result["seconds"]
            rows.append([
                "`%s`: %s" % (name, ABOUT.get(name, "")),
                "**%s**" % seconds(times.get("azol")),
                seconds(times.get("rust_opt")),
                seconds(times.get("rust_debug")),
                seconds(times.get("node")),
            ])
        out.append(table(["", "azoL", "Rust (`-O`)", "Rust (unoptimized)", "JavaScript (Node.js)"], rows))
        out.append("")
    if build_result:
        times = build_result["seconds"]
        out.append(picture("build", "Build time of azoL and of Rust for the same program"))
        out.append("**Build time** of the same %s-line program\n" % format(build_result["lines"], ","))
        rows = [["azoL, source to executable", "**%s**" % seconds(times.get("azol"))]]
        if have_rust:
            rows.append(["Rust, type checking only (`--emit=metadata`)", seconds(times.get("rust_check"))])
            rows.append(["Rust, unoptimized build", seconds(times.get("rust_debug"))])
            rows.append(["Rust, optimized build (`-O`)", seconds(times.get("rust_opt"))])
        if have_node:
            rows.append(["JavaScript, syntax check only (`node --check`)", seconds(times.get("node_check"))])
            rows.append(["JavaScript, load and run (nothing to build)", seconds(times.get("node_run"))])
        out.append(table(["", "Time"], rows))
        out.append("")
    if sizes.get("azol"):
        out.append("**Executable size** of a small program (`fib40`)\n")
        rows = [["azoL", "**%s**" % kilobytes(sizes.get("azol"))]]
        if have_rust:
            rows.append(["Rust (`-O`)", kilobytes(sizes.get("rust_opt"))])
            rows.append(["Rust (unoptimized)", kilobytes(sizes.get("rust_debug"))])
        if have_node:
            rows.append(["JavaScript", "needs Node.js installed"])
        out.append(table(["", "Size"], rows))
        out.append("")
    for line in summary(run_results, build_result):
        out.append(line)
        out.append("")
    return out


def write_readme(lines):
    """Replaces what is between the two markers in README.md."""
    path = os.path.join(ROOT, "README.md")
    text = open(path, encoding="utf-8").read()
    if README_START not in text or README_END not in text:
        sys.exit("README.md has no %s ... %s block" % (README_START, README_END))
    head, rest = text.split(README_START, 1)
    tail = rest.split(README_END, 1)[1]
    block = "\n".join(lines).rstrip("\n")
    with open(path, "w", encoding="utf-8") as out:
        out.write(head + README_START + "\n" + block + "\n" + README_END + tail)
    print("README.md: the benchmark block is updated", file=sys.stderr)
    sys.path.insert(0, HERE)
    import chart
    chart.main()


def main():
    args = sys.argv[1:]
    runs = 5
    if "--runs" in args:
        at = args.index("--runs")
        runs = int(args[at + 1])
        del args[at:at + 2]
    readme = "--write-readme" in args
    stored = "--from-results" in args
    args = [arg for arg in args if arg not in ("--write-readme", "--from-results")]
    only = set(args)

    if stored:
        with open(os.path.join(HERE, "results.json")) as source:
            results = json.load(source)
        lines = report(results)
        print("\n".join(lines))
        if readme:
            write_readme(lines)
        return

    if not os.path.exists(AZOL):
        sys.exit("%s is missing; run `sh build.sh` first" % AZOL)
    os.makedirs(BUILD, exist_ok=True)
    have_rust, have_node = bool(shutil.which("rustc")), bool(shutil.which("node"))
    problems = []
    if not have_rust:
        problems.append("rustc was not found, so Rust is left out")
    if not have_node:
        problems.append("node was not found, so JavaScript is left out")

    info = machine()
    names = benchmarks(only)
    run_results = measure_runs(names, runs, have_rust, have_node, problems)
    build_result = None if only else measure_build(runs, have_rust, have_node, problems)
    sizes = measure_sizes(have_rust, problems)
    results = {"machine": info, "runs": runs, "run_time": run_results, "build_time": build_result,
               "sizes": sizes, "problems": problems}

    lines = report(results)
    print()
    print("\n".join(lines))
    for problem in problems:
        print("note: " + problem)

    with open(os.path.join(HERE, "results.json"), "w") as out:
        json.dump(results, out, indent=2)
        out.write("\n")
    if readme:
        if only or problems:
            sys.exit("README.md is left alone: it takes a full run with rustc and node and no problems")
        write_readme(lines)


if __name__ == "__main__":
    main()
