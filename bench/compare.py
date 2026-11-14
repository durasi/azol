#!/usr/bin/env python3
"""Compares azoL with Rust, C, Go and JavaScript (Node.js) on this machine.

    sh build.sh
    python3 bench/compare.py              everything, best of 5 runs
    python3 bench/compare.py --runs 9     more runs per measurement
    python3 bench/compare.py fib40 sort   only the named benchmarks
    python3 bench/compare.py --write-readme
                                          a full run that also replaces the
                                          table in README.md, between the
                                          `<!-- bench:start -->` and
                                          `<!-- bench:end -->` lines
    python3 bench/compare.py --from-results [--write-readme]
                                          no measuring: the table again from
                                          the bench/results.json of the last run

It prints one Markdown table and writes the raw numbers to bench/results.json.

What is compared, one column for each
  azoL         `azol build`, the default build: one fast pass
  azoL -O      `azol build -O`, the optimiser (experimental)
  Rust         `rustc -O`
  C            `cc -O2` (clang where there is one, else cc or gcc)
  Go           `go build`
  JavaScript   Node.js
A column whose tools are not installed is left out, and the report says so.

The rows
  Build time   one generated program (bench/gen.py), the same in every
               language. Rust's unoptimized build and its type check alone
               are measured too and reported in a sentence under the table.
  Size         the executable of the smallest benchmark.
  Run time     every benchmark in this folder that exists in all the
               languages: the same algorithm, written plainly in each,
               printing one number. All versions must print the same
               number; a benchmark whose outputs differ is reported as
               broken and left out.

  For AI agents
               what an agent meets when it writes a language, as far as a
               script can measure it: how long a check of the generated
               program takes without building it (`azol check`,
               `rustc --emit=metadata`, `cc -fsyntax-only`, `go vet`,
               `node --check`); how much source the benchmarks take in each
               language, counted by one rule for all of them; and what each
               checker reports for the same mistake (bench/agents/), and in
               what form.

In every row the best value is bold, and nothing else is.

What to keep in mind when reading the numbers
  - Run times are whole processes, start to exit, so they include start-up:
    a few milliseconds for the compiled languages, tens for Node.
  - JavaScript is compiled while it runs (JIT) and collects garbage; Go
    collects garbage too. Short runs favour the others, long hot loops
    favour Node's optimizing compiler.
  - azoL checks every index and every division in both of its modes; so do
    Rust and Go for indexes; C checks nothing.
  - Go caches what it has compiled. Each timed build here is given a
    source file that differs in its last line, so the program is compiled
    again while the standard library stays cached, as it would be for the
    others.
  - The best of several runs is reported, which hides noise but also hides
    variation. These are small programs, not a benchmark suite.
"""
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
BUILD = os.path.join(HERE, "build")
GENERATED = os.path.join(HERE, "generated")
AZOL = os.environ.get("AZOL") or os.path.join(ROOT, "azol")
UNITS = 1000
README_START = "<!-- bench:start -->"
README_END = "<!-- bench:end -->"

# The columns, in order: key, heading, the tool each needs (None: azol itself).
COLUMNS = [
    ("azol", "azoL", None),
    ("azol_opt", "azoL `-O`", None),
    ("rust_opt", "Rust `-O`", "rustc"),
    ("c", "C `-O2`", "cc"),
    ("go", "Go", "go"),
    ("node", "JavaScript (Node.js)", "node"),
]
# The heading cell of each column in the README: a logo or a name tile, and
# under it the mode that was measured. The Rust logo is black, so dark pages
# get the name tile instead. Paths are relative to README.md.
def _img(src, alt, more=""):
    return '<img src="%s%s" alt="%s" height="28">' % (src, more, alt)


HEADINGS = {
    "azol": _img("assets/compare/azol.svg", "azoL") + "<br><sub>default</sub>",
    "azol_opt": _img("assets/compare/azol.svg", "azoL") + "<br><sub><code>-O</code></sub>",
    "rust_opt": (_img("assets/third-party/rust-logo-blk.svg", "Rust", "#gh-light-mode-only")
                 + _img("assets/compare/rust.svg", "Rust", "#gh-dark-mode-only") + "<br><sub>Rust&nbsp;<code>-O</code></sub>"),
    "c": _img("assets/compare/c.svg", "C") + "<br><sub><code>-O2</code></sub>",
    "go": _img("assets/compare/go.svg", "Go") + "<br><sub>go&nbsp;build</sub>",
    "node": _img("assets/third-party/js.png", "JavaScript") + "<br><sub>Node.js</sub>",
}
SHORT = {
    "fib40": "recursive calls",
    "loops": "nested loops",
    "sort": "quicksort, 2 million numbers",
    "sieve": "primes up to 20 million",
    "particles": "structs updated in place",
    "tree": "building and walking trees",
    "append": "building long strings",
    "words": "splitting and joining text",
}


# The README exists in two languages, README.md and README.tr.md, and this
# script writes the block between the markers in both. Every word the block
# uses is here: English is the key, Turkish the value. `say` refuses a text
# that has no Turkish, so the two blocks cannot drift apart. Numbers are
# written the same way in both (a point for decimals, a comma for thousands).
READMES = (("README.md", "en"), ("README.tr.md", "tr"))
TURKISH = {
    "Measured on %s, %s. Best of %d runs.": "%s, %s üzerinde ölçüldü. %d çalıştırmanın en iyisi.",
    "Tools: %s.": "Araçlar: %s.",
    "Lower is better everywhere. In every row the best value is bold, and nothing else in the table is.":
        "Her yerde düşük olan daha iyidir. Her satırda en iyi değer kalın yazılmıştır; tabloda başka hiçbir şey kalın değildir.",
    "Lower is better everywhere. Cells that say *to be measured* have not been run on this machine yet, "
    "and nothing is bold until they have: bold will mean the best value of its row and nothing else.":
        "Her yerde düşük olan daha iyidir. *ölçülecek* yazan hücreler bu makinede henüz çalıştırılmadı ve onlar "
        "ölçülene kadar hiçbir şey kalın değildir: kalın, satırının en iyi değeri anlamına gelecek, başka bir anlama gelmeyecek.",
    "default": "varsayılan",
    "BUILD TIME": "DERLEME SÜRESİ",
    "EXECUTABLE SIZE": "ÇALIŞTIRILABİLİR DOSYA BOYUTU",
    "RUN TIME": "ÇALIŞMA SÜRESİ",
    "The same %s-line program": "Aynı %s satırlık program",
    "no build": "derleme yok",
    "needs Node.js": "Node.js gerekir",
    "to be measured": "ölçülecek",
    "n/a": "yok",
    "recursive calls": "özyinelemeli çağrılar",
    "nested loops": "iç içe döngüler",
    "quicksort, 2 million numbers": "quicksort, 2 milyon sayı",
    "primes up to 20 million": "20 milyona kadar asal sayılar",
    "structs updated in place": "yerinde güncellenen struct'lar",
    "building and walking trees": "ağaç kurma ve dolaşma",
    "building long strings": "uzun metinler oluşturma",
    "splitting and joining text": "metni bölme ve birleştirme",
    "The generated program is %s.": "Üretilen programın uzunluğu: %s.",
    "%s lines in %s": "%s satır (%s)",
    "%(ratio)s times faster than %(name)s (%(took)s)": "%(name)s (%(took)s) karşısında %(ratio)s kat hızlı",
    "%(ratio)s times slower than %(name)s (%(took)s)": "%(name)s (%(took)s) karşısında %(ratio)s kat yavaş",
    "`rustc -O`": "`rustc -O`",
    "an unoptimized `rustc` build": "optimizasyonsuz `rustc` derlemesi",
    "`rustc`'s type check alone (`--emit=metadata`)": "yalnızca `rustc` tür denetimi (`--emit=metadata`)",
    "`cc -O2`": "`cc -O2`",
    "`go build`": "`go build`",
    "Build: azoL's default build is %s.": "Derleme: azoL'un varsayılan derlemesi %s.",
    "Node.js has nothing to build; checking the syntax of the JavaScript version (`node --check`) takes %s.":
        "Node.js için derlenecek bir şey yoktur; JavaScript sürümünün sözdizimini denetlemek (`node --check`) %s sürer.",
    "azoL's default build": "azoL'un varsayılan derlemesinin",
    "`azol build -O`": "`azol build -O` derlemesinin",
    "%(name)s in %(ahead)d of %(rows)d": "%(name)s karşısında %(rows)d programdan %(ahead)d",
    " (level in %d)": " (%d programda eşit)",
    "Run: %s is ahead of %s.": "Çalışma: %s önde olduğu program sayısı: %s.",
    ", of ": ", ",
    "The %s column is left out: %s was not found on this machine.": "%s sütunu yok: %s bu makinede bulunamadı.",
    "Lower is better everywhere. In every row the best value is bold, and nothing else in the table is. "
    "Cells that say *to be measured* have not been run on this machine yet, and a row has no bold until all of it is measured.":
        "Her yerde düşük olan daha iyidir. Her satırda en iyi değer kalın yazılmıştır; tabloda başka hiçbir şey kalın değildir. "
        "*ölçülecek* yazan hücreler bu makinede henüz çalıştırılmadı; bir satırın tamamı ölçülene kadar o satırda kalın yoktur.",
    "FOR AI AGENTS": "AI AJANLARI İÇİN",
    "Check without building": "Derlemeden denetleme",
    "the same %s-line program": "aynı %s satırlık program",
    "Lines of code": "Kod satırı",
    "Characters": "Karakter",
    "Tokens, roughly": "Token, yaklaşık",
    "the %d programs": "%d program",
    "A wrong type passed": "Yanlış türde argüman",
    "what the checker reports": "denetleyicinin bildirdiği",
    "as default": "varsayılanla aynı",
    "JSON": "JSON",
    "span": "span",
    "fix": "fix",
    "text": "metin",
    "position": "konum",
    "not detected": "yakalanmadı",
    "warning only": "yalnızca uyarı",
    "Check: the commands are %s.": "Denetleme: komutlar şunlardır: %s.",
    "Source: the %d benchmark programs together, without comments, blank lines and indentation. Tokens are counted by "
    "one rule for every language (a name, a number or a text is one token; so is each bracket, comma or semicolon, "
    "and each run of operator characters); that approximates what a model reads and writes and is not any model's tokenizer.":
        "Kaynak: %d kıyaslama programının toplamı; yorumlar, boş satırlar ve girintiler sayılmaz. Token'lar her dil için "
        "tek bir kuralla sayılır (bir ad, bir sayı ya da bir metin tek token'dır; her parantez, virgül ya da noktalı virgül "
        "ve her operatör karakteri dizisi de öyle); bu, bir modelin okuyup yazdığına yaklaşık bir ölçüdür, herhangi bir modelin tokenizer'ı değildir.",
    "Mistake: the same in each language ([`bench/agents/`](bench/agents/)): the text `\"40\"` passed where a number is "
    "expected. *JSON* means the tool gives the message and the place as data when asked; *span*, the start and the end "
    "of the place; *fix*, an edit a tool can apply; *text, position*, a line to read with a line and a column in it. "
    "No tool offers a fix for this mistake.":
        "Hata: her dilde aynı ([`bench/agents/`](bench/agents/)): sayı beklenen yere `\"40\"` metni verilir. *JSON*, aracın "
        "istendiğinde iletiyi ve yeri veri olarak verdiği anlamına gelir; *span*, yerin başı ve sonu; *fix*, bir aracın "
        "uygulayabileceği bir düzenleme; *metin, konum*, içinde satır ve sütun geçen, okunacak bir satır. "
        "Bu hata için hiçbir araç fix sunmaz.",
    "Mistake: the same in each language ([`bench/agents/`](bench/agents/)): the text `\"40\"` passed where a number is "
    "expected. *JSON* means the tool gives the message and the place as data when asked; *span*, the start and the end "
    "of the place; *fix*, an edit a tool can apply; *text, position*, a line to read with a line and a column in it.":
        "Hata: her dilde aynı ([`bench/agents/`](bench/agents/)): sayı beklenen yere `\"40\"` metni verilir. *JSON*, aracın "
        "istendiğinde iletiyi ve yeri veri olarak verdiği anlamına gelir; *span*, yerin başı ve sonu; *fix*, bir aracın "
        "uygulayabileceği bir düzenleme; *metin, konum*, içinde satır ve sütun geçen, okunacak bir satır.",
    "Node.js does not look at types before it runs: `node --check` accepts the file, and the program runs and prints a number.":
        "Node.js çalıştırmadan önce türlere bakmaz: `node --check` dosyayı kabul eder, program da çalışıp bir sayı yazdırır.",
    "a C compiler (`clang`, `cc` or `gcc`)": "bir C derleyicisi (`clang`, `cc` ya da `gcc`)",
}


def say(lang, text):
    """`text` in the language of one README. English is the text itself."""
    if lang == "en":
        return text
    if text not in TURKISH:
        sys.exit("compare.py: no Turkish for %r; add it to TURKISH" % text)
    return TURKISH[text]


def heading(lang, key, fallback):
    return HEADINGS.get(key, fallback).replace("<sub>default</sub>", "<sub>%s</sub>" % say(lang, "default"))


def section(name, columns):
    return "| <sub>%s</sub> |" % name + " |" * columns


NAMES = {"rust_opt": "Rust `-O`", "c": "C `-O2`", "go": "Go", "node": "Node.js"}
SOURCES = {"azol": ".azl", "rust_opt": ".rs", "c": ".c", "go": ".go", "node": ".js"}

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


def c_compiler():
    for name in ("clang", "cc", "gcc"):
        if shutil.which(name):
            return name
    return None


def tools():
    """The tool of each column that is installed: {"rustc": "rustc", "cc": "clang", ...}."""
    found = {}
    if shutil.which("rustc"):
        found["rustc"] = "rustc"
    if c_compiler():
        found["cc"] = c_compiler()
    if shutil.which("go"):
        found["go"] = "go"
    if shutil.which("node"):
        found["node"] = "node"
    return found


def machine(have):
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
        "rustc": first_line(["rustc", "--version"]) if "rustc" in have else None,
        "cc": first_line([have["cc"], "--version"]) if "cc" in have else None,
        "go": first_line(["go", "version"]) if "go" in have else None,
        "node": ("node " + (first_line(["node", "--version"]) or "")).strip() if "node" in have else None,
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
    """Every benchmark that exists in all the languages, whatever is installed."""
    names = []
    for file in sorted(os.listdir(HERE)):
        name, ext = os.path.splitext(file)
        if ext == ".azl" and all(os.path.exists(os.path.join(HERE, name + other)) for other in SOURCES.values()):
            if not only or name in only:
                names.append(name)
    order = list(ABOUT)
    return sorted(names, key=lambda name: (order.index(name) if name in order else len(order), name))


def compile_commands(source_of, exe_of, have):
    """{variant: command} that builds one program in every installed language.
    `source_of(ext)` is the source file, `exe_of(variant)` where the result goes."""
    commands = {
        "azol": [AZOL, "build", source_of(".azl"), "-o", exe_of("azol")],
        "azol_opt": [AZOL, "build", "-O", source_of(".azl"), "-o", exe_of("azol_opt")],
    }
    if "rustc" in have:
        commands["rust_opt"] = ["rustc", "-O", "-A", "warnings", source_of(".rs"), "-o", exe_of("rust_opt")]
    if "cc" in have:
        commands["c"] = [have["cc"], "-O2", source_of(".c"), "-o", exe_of("c")]
    if "go" in have:
        commands["go"] = ["go", "build", "-o", exe_of("go"), source_of(".go")]
    return commands


def build_variants(name, have, problems):
    """Builds one benchmark; returns {variant: command to run it}."""
    variants = {}
    commands = compile_commands(lambda ext: os.path.join(HERE, name + ext),
                                lambda variant: os.path.join(BUILD, "%s-%s" % (name, variant)), have)
    for variant, cmd in commands.items():
        _, code, out, err = run(cmd)
        if code == 0:
            variants[variant] = [os.path.join(BUILD, "%s-%s" % (name, variant))]
        else:
            problems.append("%s: the %s build failed: %s" % (name, variant, (err or out).strip()[-300:]))
    if "node" in have:
        variants["node"] = ["node", os.path.join(HERE, name + ".js")]
    return variants


def measure_runs(names, runs, have, problems):
    results = {}
    for name in names:
        print("running %s ..." % name, file=sys.stderr)
        variants = build_variants(name, have, problems)
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


def count_lines(path):
    with open(path, encoding="utf-8") as source:
        return sum(1 for _ in source)


def go_build_time(big_go, exe, runs):
    """`go build` of the program, compiled anew each time: every run gets a
    copy whose last line differs, so Go's cache holds the standard library
    and not this program."""
    folder = os.path.join(BUILD, "go")
    os.makedirs(folder, exist_ok=True)
    with open(big_go, encoding="utf-8") as source:
        text = source.read()
    warm = os.path.join(folder, "warm.go")
    with open(warm, "w", encoding="utf-8") as out:
        out.write('package main\n\nimport "fmt"\n\nfunc main() {\n\tfmt.Println(1)\n}\n')
    if run(["go", "build", "-o", os.path.join(folder, "warm"), warm])[1] != 0:
        return None
    times = []
    for index in range(runs):
        path = os.path.join(folder, "big.go")
        with open(path, "w", encoding="utf-8") as out:
            out.write(text + "// build %d at %r\n" % (index, time.time()))
        took, code, _, _ = run(["go", "build", "-o", exe, path])
        if code != 0:
            return None
        times.append(took)
    return min(times)


def measure_build(runs, have, problems):
    """Build time of the generated program, and a check that all versions agree."""
    files = {ext: os.path.join(GENERATED, "big" + ext) for ext in (".azl", ".rs", ".c", ".go", ".js")}
    if not all(os.path.exists(file) for file in files.values()) or count_lines(files[".azl"]) != 44 * UNITS + 4:
        print("generating the large program ...", file=sys.stderr)
        _, code, out, err = run([sys.executable, os.path.join(HERE, "gen.py"), str(UNITS)])
        if code != 0:
            problems.append("bench/gen.py failed: " + err.strip()[-300:])
            return None
    result = {"lines": count_lines(files[".azl"]), "seconds": {}, "outputs": {},
              "all_lines": {ext: count_lines(path) for ext, path in files.items()}}
    print("building the large program ...", file=sys.stderr)
    slow = min(runs, 3)  # each of these takes seconds
    commands = compile_commands(lambda ext: files[ext], lambda variant: os.path.join(BUILD, "big-" + variant), have)
    for variant, cmd in commands.items():
        exe = os.path.join(BUILD, "big-" + variant)
        if variant == "go":
            result["seconds"]["go"] = go_build_time(files[".go"], exe, slow)
        elif variant in ("rust_opt", "c"):  # tens of seconds each: once
            result["seconds"][variant], _ = best(cmd, 1)
        else:
            result["seconds"][variant], _ = best(cmd, runs if variant == "azol" else slow)
        if result["seconds"][variant] is None:
            problems.append("the large program: the %s build failed" % variant)
            continue
        result["outputs"][variant] = run([exe])[2].strip()
    if "rustc" in have:
        result["seconds"]["rust_check"], _ = best(
            ["rustc", "--emit=metadata", files[".rs"], "--out-dir", BUILD, "-A", "warnings"], slow)
        result["seconds"]["rust_debug"], _ = best(
            ["rustc", files[".rs"], "-o", os.path.join(BUILD, "big-rust_debug"), "-A", "warnings"], slow)
    if "node" in have:
        result["seconds"]["node_check"], _ = best(["node", "--check", files[".js"]], runs)
        took, out = best(["node", files[".js"]], runs)
        if took is not None:
            result["outputs"]["node"] = out.strip()
    if len(set(result["outputs"].values())) > 1:
        problems.append("the large program prints different results: %s" % result["outputs"])
        return None
    return result


# ---- for AI agents: the check, the size of the source, the diagnostics ------------

CHECKS = {"azol": "`azol check`", "rust_opt": "`rustc --emit=metadata`", "c": "`cc -fsyntax-only`",
          "go": "`go vet`", "node": "`node --check`"}
BRACKETS = "()[]{},;"
OPERATORS = "+-*/%=<>!&|^~?:.@#$\\"


def source_counts(text):
    """(lines, characters, tokens) of one source text, by one rule for every
    language here. Comments (`//` to the end of the line, `/* */`) are left
    out. A line counts if a token is on it; characters are those of each
    such line without its indentation and the space at its end. A token is
    a name, a number, a text in quotes (`"`, `'` or a backtick), one of
    `()[]{},;`, or a run of operator characters."""
    tokens, kept, line = 0, [], []
    at, end = 0, len(text)

    def close():
        joined = "".join(line).strip()
        if joined:
            kept.append(joined)
        del line[:]

    while at < end:
        ch = text[at]
        if ch == "\n":
            close()
            at += 1
        elif text.startswith("//", at):
            while at < end and text[at] != "\n":
                at += 1
        elif text.startswith("/*", at):
            stop = text.find("*/", at + 2)
            at = end if stop < 0 else stop + 2
        elif ch in "\"'`":
            stop = at + 1
            while stop < end and text[stop] != ch and (ch == "`" or text[stop] != "\n"):
                stop += 2 if text[stop] == "\\" and ch != "`" else 1
            if ch == "'" and (stop >= end or text[stop] != "'"):  # a lifetime or a lone quote, not a text
                tokens += 1
                line.append(ch)
                at += 1
                continue
            stop = min(stop + 1, end)
            tokens += 1
            line.append(text[at:stop].replace("\n", " "))
            at = stop
        elif ch.isalnum() or ch == "_":
            stop = at
            while stop < end and (text[stop].isalnum() or text[stop] == "_"):
                stop += 1
            tokens += 1
            line.append(text[at:stop])
            at = stop
        elif ch in BRACKETS:
            tokens += 1
            line.append(ch)
            at += 1
        elif ch in OPERATORS:
            stop = at
            while stop < end and text[stop] in OPERATORS and not text.startswith("//", stop) and not text.startswith("/*", stop):
                stop += 1
            tokens += 1
            line.append(text[at:stop])
            at = stop
        else:
            line.append(ch)
            at += 1
    close()
    return len(kept), sum(len(one) for one in kept), tokens


def measure_source(names):
    """{"lines": {column: n}, "characters": ..., "tokens": ..., "programs": n} over the benchmarks."""
    result = {"lines": {}, "characters": {}, "tokens": {}, "programs": len(names)}
    for key, ext in SOURCES.items():
        totals = [0, 0, 0]
        for name in names:
            with open(os.path.join(HERE, name + ext), encoding="utf-8") as source:
                for index, value in enumerate(source_counts(source.read())):
                    totals[index] += value
        result["lines"][key], result["characters"][key], result["tokens"][key] = totals
    return result


def go_check_time(big_go, runs):
    """`go vet` of the program, checked anew each time, as `go_build_time` builds it."""
    folder = os.path.join(BUILD, "govet")
    os.makedirs(folder, exist_ok=True)
    with open(big_go, encoding="utf-8") as source:
        text = source.read()
    times = []
    for index in range(runs):
        path = os.path.join(folder, "big.go")
        with open(path, "w", encoding="utf-8") as out:
            out.write(text + "// check %d at %r\n" % (index, time.time()))
        took, code, _, _ = run(["go", "vet", path])
        if code != 0:
            return None
        times.append(took)
    return min(times)


def measure_checks(runs, have, build_result, problems):
    """Seconds to check the generated program without building it."""
    files = {ext: os.path.join(GENERATED, "big" + ext) for ext in (".azl", ".rs", ".c", ".go", ".js")}
    slow = min(runs, 3)
    times = {}
    times["azol"], _ = best([AZOL, "check", files[".azl"]], runs)
    if "rustc" in have:
        times["rust_opt"] = (build_result or {}).get("seconds", {}).get("rust_check")
    if "cc" in have:
        times["c"], _ = best([have["cc"], "-fsyntax-only", files[".c"]], slow)
    if "go" in have:
        times["go"] = go_check_time(files[".go"], slow)
    if "node" in have:
        times["node"] = (build_result or {}).get("seconds", {}).get("node_check")
    for key, value in times.items():
        if value is None:
            problems.append("the large program: the %s check failed" % key)
    return times


def json_values(text):
    """Every JSON value found in `text`: the whole of it, or one to a line."""
    found = []
    try:
        found.append(json.loads(text))
    except ValueError:
        for line in text.splitlines():
            line = line.strip()
            if line[:1] in "{[":
                try:
                    found.append(json.loads(line))
                except ValueError:
                    pass
    return found


def walk(value):
    """Every dict inside a JSON value."""
    if isinstance(value, dict):
        yield value
        for inner in value.values():
            for one in walk(inner):
                yield one
    elif isinstance(value, list):
        for inner in value:
            for one in walk(inner):
                yield one


def diagnostic_of(key, have):
    """What the checker of one language reports for bench/agents/wrong_type:
    {"detected", "warning", "json", "span", "fix", "position"}, each true or
    false, worked out from what the tool really prints here."""
    path = os.path.join(HERE, "agents", "wrong_type" + SOURCES[key])
    seen = {"detected": False, "warning": False, "json": False, "span": False, "fix": False, "position": False}

    def text_form(output, code):
        seen["detected"] = code != 0 or "warning" in output
        seen["warning"] = code == 0 and seen["detected"]
        seen["position"] = bool(re.search(r":\d+:\d+", output))

    if key == "azol":
        _, code, out, err = run([AZOL, "check", "--json", path])
        for item in json_values(out + err):
            if isinstance(item, dict) and item.get("severity") == "error":
                seen.update(detected=True, json=True, span="end_column" in item, fix="fix" in item)
        if not seen["json"]:
            text_form(out + err, code)
    elif key == "rust_opt":
        _, code, out, err = run(["rustc", "--error-format=json", "--emit=metadata", "-o", os.path.join(BUILD, "wrong_type.rmeta"), path])
        for item in json_values(out + err):
            if isinstance(item, dict) and item.get("level") == "error" and item.get("spans"):
                spans = item["spans"] + [span for child in item.get("children", []) for span in child.get("spans", [])]
                seen.update(detected=True, json=True,
                            span=any("column_end" in span for span in item["spans"]),
                            fix=any(span.get("suggestion_applicability") == "MachineApplicable" for span in spans))
        if not seen["json"]:
            text_form(out + err, code)
    elif key == "c":
        for flags in (["-fdiagnostics-format=sarif", "-Wno-sarif-format-unstable"], ["-fdiagnostics-format=sarif-stderr"],
                      ["-fdiagnostics-format=json"]):
            _, code, out, err = run([have["cc"], "-fsyntax-only"] + flags + [path])
            for item in json_values(out + err):
                for one in walk(item):
                    if "region" in one and ("message" in str(item)):   # SARIF
                        seen.update(detected=True, json=True)
                        seen["span"] = seen["span"] or "endColumn" in one["region"]
                    if "fixes" in one or "fixits" in one:
                        seen["fix"] = True
                    if "caret" in one:                                  # gcc's own JSON
                        seen.update(detected=True, json=True)
                        seen["span"] = seen["span"] or "finish" in one
            if seen["json"]:
                seen["warning"] = code == 0
                break
        if not seen["json"]:
            _, code, out, err = run([have["cc"], "-fsyntax-only", path])
            text_form(out + err, code)
    elif key == "go":
        _, code, out, err = run(["go", "vet", "-json", path])
        for item in json_values(out + err):
            for one in walk(item):
                if "posn" in one and "message" in one:
                    seen.update(detected=True, json=True, span="end" in one, fix=bool(one.get("suggested_fixes")))
        if not seen["json"]:
            text_form(out + err, code)
    elif key == "node":
        _, code, out, err = run(["node", "--check", path])
        text_form(out + err, code)
    return seen


def measure_agents(names, runs, have, build_result, problems):
    print("the rows for AI agents ...", file=sys.stderr)
    tool_of = {"azol": None, "rust_opt": "rustc", "c": "cc", "go": "go", "node": "node"}
    return {
        "check": measure_checks(runs, have, build_result, problems),
        "source": measure_source(names),
        "diagnostics": {key: diagnostic_of(key, have) for key, tool in tool_of.items() if tool is None or tool in have},
    }


def diagnostic_cell(seen, lang):
    if seen is None:
        return None
    if not seen["detected"]:
        return say(lang, "not detected")
    if seen["json"]:
        parts = ["JSON"] + [name for name in ("span", "fix") if seen[name]]
    else:
        parts = ["text"] + (["position"] if seen["position"] else [])
    if seen["warning"]:
        parts.append("warning only")
    return ", ".join(say(lang, part) for part in parts)


def words_row(label, cells, keys, pending, lang):
    """A row of words: never bold."""
    out = [label]
    for key in keys:
        cell = cells.get(key)
        out.append(cell if cell is not None else say(lang, "to be measured" if pending else "n/a"))
    return "| " + " | ".join([out[0]] + [cell.replace(" ", "&nbsp;") for cell in out[1:]]) + " |"


def agent_rows(agents, keys, build_result, pending, lang):
    out = [section(say(lang, "FOR AI AGENTS"), len(keys))]
    same = {"azol_opt": "as default"}
    lines = format(build_result["lines"], ",") if build_result else "?"
    check = agents.get("check", {})
    out.append(row("%s <sub>%s</sub>" % (say(lang, "Check without building"), say(lang, "the same %s-line program") % lines),
                   {key: check.get(key) for key in keys}, seconds, pending, same, lang))
    source = agents.get("source", {})
    programs = say(lang, "the %d programs") % source.get("programs", 8)
    for name, label in (("lines", "Lines of code"), ("characters", "Characters"), ("tokens", "Tokens, roughly")):
        out.append(row("%s <sub>%s</sub>" % (say(lang, label), programs),
                       {key: source.get(name, {}).get(key) for key in keys}, lambda value: format(value, ","), pending, same, lang))
    seen = agents.get("diagnostics", {})
    cells = {key: diagnostic_cell(seen.get(key), lang) for key in keys}
    cells["azol_opt"] = say(lang, "as default")
    out.append(words_row("%s <sub>%s</sub>" % (say(lang, "A wrong type passed"), say(lang, "what the checker reports")),
                         cells, keys, pending, lang))
    return out


def agent_notes(agents, keys, lang):
    out = [say(lang, "Check: the commands are %s.") % ", ".join(CHECKS[key] for key in keys if key in CHECKS)]
    out.append(say(lang, "Source: the %d benchmark programs together, without comments, blank lines and indentation. Tokens are counted by "
                         "one rule for every language (a name, a number or a text is one token; so is each bracket, comma or semicolon, "
                         "and each run of operator characters); that approximates what a model reads and writes and is not any model's tokenizer.")
               % agents.get("source", {}).get("programs", 8))
    seen = agents.get("diagnostics", {})
    mistake = ("Mistake: the same in each language ([`bench/agents/`](bench/agents/)): the text `\"40\"` passed where a number is "
               "expected. *JSON* means the tool gives the message and the place as data when asked; *span*, the start and the end "
               "of the place; *fix*, an edit a tool can apply; *text, position*, a line to read with a line and a column in it.")
    if seen and not any(one["fix"] for one in seen.values()):
        mistake += " No tool offers a fix for this mistake."
    out.append(say(lang, mistake))
    if "node" in seen and not seen["node"]["detected"]:
        out.append(say(lang, "Node.js does not look at types before it runs: `node --check` accepts the file, and the program runs and prints a number."))
    return out


def measure_sizes():
    """Size of the executables of the smallest benchmark."""
    sizes = {}
    for variant in ("azol", "azol_opt", "rust_opt", "c", "go"):
        sizes[variant] = size(os.path.join(BUILD, "fib40-" + variant))
    return sizes


def ratio(value):
    """A ratio for prose: `5.8`, `70`, `950`."""
    if value >= 100:
        return "%d" % (round(value / 10) * 10)
    if value >= 10:
        return "%d" % round(value)
    return "%.1f" % value


def against(lang, mine, theirs, name, took):
    """`3.2 times faster than X (0.5 s)` or `1.4 times slower than X (0.5 s)`."""
    if mine <= theirs:
        return say(lang, "%(ratio)s times faster than %(name)s (%(took)s)") % {
            "ratio": ratio(theirs / mine), "name": say(lang, name), "took": took}
    return say(lang, "%(ratio)s times slower than %(name)s (%(took)s)") % {
        "ratio": ratio(mine / theirs), "name": say(lang, name), "took": took}


def row(label, values, show, pending, never=(), lang="en"):
    """One row of the table. `values` is {column: number or None}; the best
    (smallest) is bold, with every cell that reads the same. `never` holds
    {column: text} for cells that are words and are never bold."""
    never = dict(never)
    numbers = [value for key, value in values.items() if value is not None and key not in never]
    missing = any(value is None for key, value in values.items() if key not in never)
    winner = show(min(numbers)) if numbers and not (pending and missing) else None
    out = [label]
    for key, value in values.items():
        if key in never:
            out.append(say(lang, never[key]))
        elif value is None:
            out.append(say(lang, "to be measured" if pending else "n/a"))
        elif show(value) == winner:
            out.append("**%s**" % show(value))
        else:
            out.append(show(value))
    return "| " + " | ".join([out[0]] + [cell.replace(" ", "&nbsp;") for cell in out[1:]]) + " |"


def counts(run_results, mine, theirs):
    """(ahead, level, rows) of `mine` against `theirs` over the run rows, as printed."""
    ahead = level = rows = 0
    for result in run_results.values():
        a, b = result["seconds"].get(mine), result["seconds"].get(theirs)
        if a is None or b is None:
            continue
        rows += 1
        if seconds(a) == seconds(b):
            level += 1
        elif a < b:
            ahead += 1
    return ahead, level, rows


def summary(run_results, build_result, shown, lang="en"):
    """Sentences worked out from the numbers, so that they stay true."""
    out = []
    if build_result:
        times, lines = build_result["seconds"], build_result.get("all_lines", {})
        if lines:
            names = ((".azl", "azoL"), (".rs", "Rust"), (".c", "C"), (".go", "Go"), (".js", "JavaScript"))
            out.append(say(lang, "The generated program is %s.")
                       % ", ".join(say(lang, "%s lines in %s") % (format(lines[ext], ","), name) for ext, name in names if ext in lines))
        azol = times.get("azol")
        if azol:
            parts = []
            for key, name in (("rust_opt", "`rustc -O`"), ("rust_debug", "an unoptimized `rustc` build"),
                              ("rust_check", "`rustc`'s type check alone (`--emit=metadata`)"),
                              ("c", "`cc -O2`"), ("go", "`go build`")):
                if times.get(key):
                    parts.append(against(lang, azol, times[key], name, seconds(times[key])))
            if parts:
                out.append(say(lang, "Build: azoL's default build is %s.") % ", ".join(parts))
            if times.get("node_check"):
                out.append(say(lang, "Node.js has nothing to build; checking the syntax of the JavaScript version "
                                     "(`node --check`) takes %s.") % seconds(times["node_check"]))
    for mine, name in (("azol", "azoL's default build"), ("azol_opt", "`azol build -O`")):
        parts = []
        for theirs in ("rust_opt", "c", "go", "node"):
            if theirs not in shown:
                continue
            ahead, level, rows = counts(run_results, mine, theirs)
            if rows:
                text = say(lang, "%(name)s in %(ahead)d of %(rows)d") % {"name": NAMES[theirs], "ahead": ahead, "rows": rows}
                if level:
                    text += say(lang, " (level in %d)") % level
                parts.append(text)
        if parts:
            out.append(say(lang, "Run: %s is ahead of %s.") % (say(lang, name), say(lang, ", of ").join(parts)))
    return out


def report(results, lang="en"):
    """The Markdown report, as a list of lines, from what `results.json` holds,
    in the language of one README."""
    info, runs = results["machine"], results["runs"]
    run_results, build_result, sizes = results["run_time"], results["build_time"], results["sizes"]
    pending = bool(results.get("pending"))
    shown = [(key, heading) for key, heading, tool in COLUMNS if tool is None or info.get(tool)]
    keys = [key for key, _ in shown]
    out = []
    out.append(say(lang, "Measured on %s, %s. Best of %d runs.") % (info["cpu"], info["os"], runs))
    out.append(say(lang, "Tools: %s.") % "; ".join(str(info[tool]) for tool in ("azol", "rustc", "cc", "go", "node") if info.get(tool)))
    out.append("")
    if pending:
        out.append(say(lang, "Lower is better everywhere. In every row the best value is bold, and nothing else in the table is. "
                             "Cells that say *to be measured* have not been run on this machine yet, and a row has no bold until all of it is measured."))
    else:
        out.append(say(lang, "Lower is better everywhere. In every row the best value is bold, and nothing else in the table is."))
    out.append("")
    out.append("|  | " + " | ".join(heading(lang, key, fallback) for key, fallback in shown) + " |")
    out.append("|:---|" + "---:|" * len(shown))
    if build_result:
        times = build_result["seconds"]
        out.append(section(say(lang, "BUILD TIME"), len(shown)))
        out.append(row(say(lang, "The same %s-line program") % format(build_result["lines"], ","),
                       {key: times.get(key) for key in keys}, seconds, pending, {"node": "no build"}, lang))
    if sizes.get("azol"):
        out.append(section(say(lang, "EXECUTABLE SIZE"), len(shown)))
        out.append(row("`fib40`", {key: sizes.get(key) for key in keys}, kilobytes, pending,
                       {"node": "needs Node.js"}, lang))
    if run_results:
        out.append(section(say(lang, "RUN TIME"), len(shown)))
    for name, result in run_results.items():
        about = SHORT.get(name)
        out.append(row("`%s` <sub>%s</sub>" % (name, say(lang, about) if about else name),
                       {key: result["seconds"].get(key) for key in keys}, seconds, pending, (), lang))
    agents = results.get("agents") or ({} if pending else None)
    if agents is not None:
        out.extend(agent_rows(agents, keys, build_result, pending, lang))
    out.append("")
    for tool in results.get("left_out_tools", []):
        out.append(left_out_note(lang, tool))
        out.append("")
    for line in summary(run_results, build_result, keys, lang) + (agent_notes(agents, keys, lang) if agents is not None else []):
        out.append(line)
        out.append("")
    return out


LEFT_OUT = {"rustc": "Rust", "cc": "C", "go": "Go", "node": "JavaScript"}


def left_out_note(lang, tool):
    name = say(lang, "a C compiler (`clang`, `cc` or `gcc`)") if tool == "cc" else "`%s`" % tool
    return say(lang, "The %s column is left out: %s was not found on this machine.") % (LEFT_OUT[tool], name)


def write_readme(results):
    """Replaces what is between the two markers in README.md and in
    README.tr.md, each in its own language, from the same results."""
    for name, lang in READMES:
        path = os.path.join(ROOT, name)
        text = open(path, encoding="utf-8").read()
        if README_START not in text or README_END not in text:
            sys.exit("%s has no %s ... %s block" % (name, README_START, README_END))
        head, rest = text.split(README_START, 1)
        tail = rest.split(README_END, 1)[1]
        block = "\n".join(report(results, lang)).rstrip("\n")
        with open(path, "w", encoding="utf-8") as out:
            out.write(head + README_START + "\n" + block + "\n" + README_END + tail)
        print("%s: the benchmark block is updated" % name, file=sys.stderr)


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
            write_readme(results)
        return

    if not os.path.exists(AZOL):
        sys.exit("%s is missing; run `sh build.sh` first" % AZOL)
    os.makedirs(BUILD, exist_ok=True)
    have = tools()
    left_out, problems = [], []
    for tool, what, how in (("rustc", "Rust", "install Rust from https://rustup.rs"),
                            ("cc", "C", "install a C compiler (on macOS: `xcode-select --install`)"),
                            ("go", "Go", "install Go from https://go.dev/dl"),
                            ("node", "JavaScript", "install Node.js from https://nodejs.org")):
        if tool not in have:
            name = "a C compiler (`clang`, `cc` or `gcc`)" if tool == "cc" else "`%s`" % tool
            left_out.append(tool)
            print("note: no %s column: %s was not found; to have it, %s" % (what, name, how), file=sys.stderr)

    info = machine(have)
    names = benchmarks(only)
    run_results = measure_runs(names, runs, have, problems)
    build_result = None if only else measure_build(runs, have, problems)
    sizes = measure_sizes()
    agents = None if only or not build_result else measure_agents(benchmarks(None), runs, have, build_result, problems)
    results = {"machine": info, "runs": runs, "run_time": run_results, "build_time": build_result,
               "sizes": sizes, "agents": agents, "left_out_tools": left_out, "problems": problems}

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
            sys.exit("README.md is left alone: it takes a full run with no problems")
        if "rustc" not in have or "node" not in have:
            sys.exit("README.md is left alone: the table there needs at least rustc and node")
        write_readme(results)


if __name__ == "__main__":
    main()
