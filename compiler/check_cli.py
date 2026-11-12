#!/usr/bin/env python3
"""Checks the command line of the azoL compiler written in azoL against the Rust one.

`azol-self <arguments>` must behave like `azol <arguments>`: the same
standard output, the same standard error, the same exit code and the
same files written. `check_self.py` proves that for `build` (the same
executables, the compiler compiling itself). This script compares the
rest of what a command line can ask for:

    check, check --json   every right and wrong program the other check
                          scripts use, programs of several files, and
                          programs with several independent mistakes in
                          several functions and files, with their fixes
    build --json, build --emit-code, build to a place that cannot be written
    explain               every code, the list, an unknown code, --markdown
    fmt --check, fmt -, fmt in place (on copies)
                          every .azl file of the repository as it is and
                          made ugly, the pairs of tests/format, files
                          that do not parse, files that cannot be read
    lex, parse            with and without --time
    usage errors, --version

Both tools are run in two folders with the same files under the same
names, so that every path they print is the same. Left out of the
comparison:

- the times in the lines `built ...`, `checked ...`, `lexed ...` and
  `parsed ...`: azol-self has no clock and prints zeros;
- a build that names no target on a machine that is Linux on ARM64,
  where the Rust compiler builds for macOS and azol-self for that
  machine (this script runs on x86-64 Linux, where both agree);
- `sema-dump` (the tool `azol-sema` does that) and azol-self's own
  `build --map`;
- source files that are not valid UTF-8, which only the Rust compiler
  refuses to read.

    python3 compiler/check_cli.py           everything
    python3 compiler/check_cli.py --quick   without bench/generated/big.azl and
                                            with a third of the damaged programs

`azol-self` is built for x86-64 Linux and run directly, so this script
needs such a machine. Build the Rust compiler first: cargo build --release
"""
import glob, os, platform, re, shutil, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
import check_front, check_sema, check_self  # noqa: E402  (the programs come from them)

AZOL = os.path.join(ROOT, "target", "release", "azol")
BIG = os.path.join(ROOT, "bench", "generated", "big.azl")

TIMES = [
    (re.compile(rb"^(built .* \(\d+ bytes of code\) in )[0-9.]+ ms \(read [0-9.]+, parse [0-9.]+, check [0-9.]+, "
                rb"codegen [0-9.]+, link [0-9.]+, write [0-9.]+\)$", re.M), rb"\1..."),
    (re.compile(rb"^(lexed \d+ bytes, \d+ lines, \d+ tokens in ).*$", re.M), rb"\1..."),
    (re.compile(rb"^(parsed \d+ lines into \d+ nodes in ).*$", re.M), rb"\1..."),
    (re.compile(rb"^(checked \d+ lines in \d+ files?: no errors \()[0-9.]+ ms\)$", re.M), rb"\1..."),
]

GROW = "fn grow(items: mut List[i64]) {\n    items.push(1)\n}\n\nfn size(items: List[i64]) -> i64 {\n    return items.len()\n}\n\n"
LONG = "    let total = " + " + ".join("value_%d" % n for n in range(40)) + "\n"

# Programs with several mistakes, or with a mistake that has a fix, as
# lists of (file name, text). The tests of src/main.rs use most of them.
EXTRA = {
    "every_function": [("a.azl", "fn one() -> i64 {\n    return true\n}\n\nfn two() {\n    let a = 1\n    a = 2\n    missing()\n}\n\nfn three() {\n    print(nothing)\n}\n\nfn main() {\n}\n")],
    "no_main_and_more": [("a.azl", "fn f() {\n    g()\n}\n")],
    "declaration_alone": [("a.azl", "struct P {\n    x: Nope\n}\n\nfn main() {\n    print(nothing)\n}\n")],
    "generic_once": [("a.azl", "fn show[T](x: T) {\n    print(nothing)\n}\n\nfn main() {\n    show(1)\n    show(true)\n}\n")],
    "generic_with_types": [("a.azl", "fn show[T](x: T) {\n    print(x)\n}\n\nfn main() {\n    show(1)\n    show([1, 2])\n    show([[true]])\n}\n")],
    "syntax_in_files": [("a.azl", "fn main() {\n    let x = \n}\n"), ("b.azl", "fn ok() {\n}\n"), ("c.azl", "fn f() {\n    let s = \"open\n}\n"),
                        ("d.azl", "fn g( {\n}\nfn h( {\n}\n")],
    "syntax_waits": [("a.azl", "fn main() {\n    print(nothing)\n}\n"), ("b.azl", "fn f( {\n}\n")],
    "sorted_by_file": [("a.azl", "fn main() {\n    b()\n    print(x)\n}\n\nfn last() {\n    print(y)\n}\n"), ("b.azl", "fn b() {\n    print(z)\n}\n")],
    "span_name": [("a.azl", "fn main() {\n    print(çay)\n}\n")],
    "span_operand": [("a.azl", "fn f(a: i64) -> bool {\n    return true\n}\n\nfn main() {\n    print(1 + f(2 * (3 + 4)))\n}\n")],
    "span_parens": [("a.azl", "fn main() {\n    let a: i64 = (1 < 2)\n}\n")],
    "span_method": [("a.azl", "fn main() {\n    let a: i64 = [1, 2].len() == 2\n}\n")],
    "span_two_lines": [("a.azl", "fn main() {\n    let a: bool = 1 +\n        2\n}\n")],
    "span_character": [("a.azl", "fn main() {\n    let a = @\n}\n")],
    "span_end_of_file": [("a.azl", "fn main() {\n")],
    "span_try": [("a.azl", "fn f() -> Option[i64] {\n    return None\n}\n\nfn main() {\n    let a: bool = f()?\n}\n")],
    "span_struct": [("a.azl", "struct P {\n    x: i64\n}\n\nfn main() {\n    let a: i64 = P { x: 1 }\n    let b: i64 = [P { x: 1 }][0].x == 2\n}\n")],
    "fix_var_assign": [("a.azl", "fn main() {\n    let total = 0\n    total = total + 1\n}\n")],
    "fix_var_push": [("a.azl", "fn main() {\n    let çay: List[i64] = [1]\n    çay.push(2)\n}\n")],
    "fix_var_mut": [("a.azl", GROW + "fn main() {\n    let xs = [1]\n    grow(mut xs)\n}\n")],
    "fix_var_field": [("a.azl", "struct P {\n    x: List[i64]\n}\n\nfn main() {\n    let p = P { x: [1] }\n    p.x[0] = 2\n}\n")],
    "fix_var_shadow": [("a.azl", GROW + "fn main() {\n    let xs = [1]\n    if size(xs) > 0 {\n        let xs = [2]\n        print(size(xs))\n    }\n    let n = size(xs)\n    xs.push(n)\n}\n")],
    "fix_var_same_name": [("a.azl", GROW + "fn pop(items: mut List[i64]) -> List[i64] {\n    return [items.pop()]\n}\n\nfn main() {\n    let xs = [1]\n    let xs = pop(mut xs)\n    print(size(xs))\n}\n")],
    "fix_var_in_let": [("a.azl", "fn main() {\n    let xs = [1]\n    let n = xs.pop()\n    print(n)\n}\n")],
    "fix_var_tab": [("a.azl", "fn main() {\n\tlet\txs = [1]\n\txs.push(2)\n}\n")],
    "fix_mut_missing": [("a.azl", GROW + "fn main() {\n    var xs = [1]\n    grow(xs)\n}\n")],
    "fix_mut_generic": [("a.azl", "fn add[T](items: mut List[T], item: T) {\n    items.push(item)\n}\n\nfn main() {\n    var xs = [1]\n    add(xs, 2)\n}\n")],
    "fix_mut_too_many": [("a.azl", GROW + "fn main() {\n    var xs = [1]\n    print(size(mut xs))\n}\n")],
    "fix_mut_variant": [("a.azl", "fn main() {\n    var a = 1\n    let b = Some(mut a)\n    match b {\n        Some(v) => print(v)\n        None => {}\n    }\n}\n")],
    "fix_mut_print": [("a.azl", "fn main() {\n    var a = 1\n    print(mut a)\n}\n")],
    "fix_arms_enum": [("a.azl", "enum Shape {\n    Circle(i64)\n    Square(i64, i64)\n    Empty\n}\n\nfn main() {\n    let s = Empty\n    match s {\n        Circle(r) => print(r)\n    }\n}\n")],
    "fix_arms_one": [("a.azl", "fn main() {\n    let o: Option[i64] = None\n    match o {\n        Some(v) => print(v)\n    }\n}\n")],
    "fix_arms_bool": [("a.azl", "fn main() {\n    match 1 < 2 {\n        true => print(1)\n    }\n}\n")],
    "fix_arms_number": [("a.azl", "fn main() {\n    match args().len() {\n        0 => print(0)\n    }\n}\n")],
    "fix_arms_text_tabs": [("a.azl", "fn main() {\n\tmatch \"a\" {\n\t\t\"a\" => print(1)\n\t}\n}\n")],
    "fix_arms_value_in_brackets": [("a.azl", "fn main() {\n    match [1, 2][(0)] {\n        0 => print(0)\n    }\n}\n")],
    "fix_pattern_values": [("a.azl", "fn main() {\n    let o: Option[i64] = None\n    match o {\n        Some => print(1)\n        None => print(0)\n    }\n}\n")],
    "fix_empty_parens": [("a.azl", "fn main() {\n    let o: Option[i64] = None()\n    match o {\n        Some(v) => print(v)\n        None => print(0)\n    }\n}\n")],
    "no_fix_parens_with_value": [("a.azl", "fn main() {\n    let o: Option[i64] = None(1)\n}\n")],
    "no_fix_parameter": [("a.azl", "fn f(a: i64) {\n    a = 1\n}\n\nfn main() {\n}\n")],
    "no_fix_loop_variable": [("a.azl", "fn main() {\n    for x in [1] {\n        x = 2\n    }\n}\n")],
    "no_fix_let_argument": [("a.azl", "fn grow(items: mut List[i64]) {\n}\n\nfn main() {\n    let xs = [1]\n    grow(xs)\n}\n")],
    "no_fix_literal_argument": [("a.azl", "fn grow(items: mut List[i64]) {\n}\n\nfn main() {\n    grow([1])\n}\n")],
    "no_fix_wrong_type": [("a.azl", "fn grow(items: mut List[i64]) {\n}\n\nfn main() {\n    var xs = [true]\n    grow(xs)\n}\n")],
    "no_fix_one_line_match": [("a.azl", "fn main() {\n    match true { true => print(1) }\n}\n")],
    "json_escapes": [("dir\"a\".azl", "fn main() {\n\tlet s = \"a\\tb\"\n\ts = \"c\"\n}\n")],
    "json_control": [("a.azl", "fn main() {\n    print(x) // \x01\x1f \\ \"q\"\n}\n")],
    "fix_in_later_file": [("a.azl", "fn main() {\n    count()\n}\n"), ("b.azl", "fn count() {\n    let n = 0\n    n = n + 1\n}\n")],
    "fixes_in_three_files": [("a.azl", GROW + "fn main() {\n    let xs = [1]\n    xs.push(2)\n    b()\n    c()\n}\n"),
                             ("b.azl", "fn b() {\n    var xs = [1]\n    grow(xs)\n    match xs.len() {\n        1 => {}\n    }\n}\n"),
                             ("c.azl", "fn c() {\n    let o = Some(1)\n    match o {\n        None => {}\n    }\n}\n\nfn d() {\n    print(size(mut nothing))\n}\n")],
    "long_line": [("a.azl", "fn main() {\n" + LONG + "}\n")],
    "long_line_late": [("a.azl", "fn main() {\n    let value_0 = 1\n    let value_1 = 2\n    let value_2 = 3\n" + LONG.replace("value_39", "çok_uzun") + "}\n")],
    "windows_lines": [("a.azl", "fn main() {\r\n    let a = 1\r\n    a = 2\r\n    print(b)\r\n}\r\n")],
    "many_errors": [("a.azl", "".join("fn f%d() {\n    print(missing_%d)\n}\n\n" % (n, n) for n in range(120)) + "fn main() {\n}\n")],
    "library_misuse": [("a.azl", "fn main() {\n    let m: Map[i64] = new_map()\n    m.set(\"a\", 1)\n    print([1, 2].sum(3))\n}\n")],
    "inside_library": [("a.azl", "struct P {\n    x: i64\n}\n\nfn main() {\n    let xs = [P { x: 1 }]\n    print(xs.contains(P { x: 1 }))\n}\n")],
    "empty": [("a.azl", "")],
    "only_a_comment": [("a.azl", "  // nothing here  ")],
    "several_right": [("a.azl", "fn main() {\n    helper()\n}\n"), ("b.azl", "fn helper() {\n    print(1)\n}"), ("c.azl", "\n\n")],
}

# Command lines that are not understood, or that ask for something that is not there.
USAGE = [
    [], ["help"], ["--help"], ["--version", "x"], ["version"], ["lex"], ["lex", "a", "b"], ["lex", "--time"], ["lex", "--time", "a", "b"],
    ["parse"], ["parse", "--fast", "a.azl"], ["build"], ["check"], ["build", "-o"], ["build", "-o", "out"], ["build", "a.azl", "-o"],
    ["build", "a.azl", "--fast"], ["check", "--json"], ["check", "-"], ["build", "a.azl", "--target"],
    ["build", "a.azl", "--target", "riscv"], ["build", "--target", "riscv", "a.azl"], ["build", "a.azl", "--emit-code"],
    ["fmt"], ["fmt", "--check"], ["fmt", "-", "a.azl"], ["fmt", "a.azl", "--check"], ["fmt", "--check", "-"], ["fmt", "-x"],
    ["explain", "E0401", "E0402"], ["explain", "e0401"], ["explain", "E9999"], ["explain", ""], ["explain", "--json"],
    ["check", "missing.azl"], ["check", "a.azl", "missing.azl"], ["check", "missing.azl", "a.azl"], ["build", "missing.azl"],
    ["lex", "missing.azl"], ["parse", "missing.azl"], ["parse", "--time", "missing.azl"], ["fmt", "missing.azl"],
    ["fmt", "--check", "missing.azl", "a.azl"], ["fmt", "missing.azl", "ugly.azl", "wrong.azl", "folder"], ["check", "folder"],
    ["lex", "folder"], ["fmt", "folder"], ["build", "a.azl", "-o", "folder"], ["build", "a.azl", "-o", "nowhere/out"],
    ["build", "a.azl", "-o", "a.azl"], ["build", "a"], ["build", "a.azl", "--emit-code", "nowhere/code"], ["check", ""],
    ["build", "a.azl", "-o", "-out"], ["check", "--json", "wrong.azl", "a.azl"], ["build", "--json", "a.azl", "-o", "made"],
    ["build", "--json", "wrong.azl"], ["build", "a.azl", "ugly.azl"], ["build", "noext"], ["build", ".hidden"],
    ["build", "folder.d/in.azl"], ["build", "two.dots.azl"], ["lex", "--time", "a.azl"], ["parse", "--time", "a.azl"],
    ["lex", "--time", "wrong.azl"], ["parse", "--time", "wrong.azl"], ["--version"],
]
USAGE_FILES = [("a.azl", "fn main() {\n    print(1)\n}\n"), ("ugly.azl", "fn  helper( ){print( 2 )}\n"), ("wrong.azl", "fn main() {\n    let = 1\n}\n"),
               ("noext", "fn main() {\n}\n"), (".hidden", "fn main() {\n}\n"), ("folder.d/in.azl", "fn main() {\n}\n"),
               ("two.dots.azl", "fn main() {\n}\n"), ("folder/keep", "")]


def run(command, **more):
    return subprocess.run(command, capture_output=True, **more)


def without_times(err):
    for pattern, kept in TIMES:
        err = pattern.sub(kept, err)
    return err


def fill(folder, files):
    """Makes `folder` hold exactly `files`: (relative path, text or bytes) pairs."""
    shutil.rmtree(folder, ignore_errors=True)
    os.makedirs(folder)
    for name, text in files:
        path = os.path.join(folder, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        data = text if isinstance(text, bytes) else text.encode("utf-8")
        with open(path, "wb") as out:
            out.write(data)


def snapshot(folder):
    """Every file below `folder`: its bytes and whether it can be executed."""
    found = {}
    for where, _, names in os.walk(folder):
        for name in names:
            path = os.path.join(where, name)
            with open(path, "rb") as source:
                found[os.path.relpath(path, folder)] = (source.read(), os.access(path, os.X_OK))
    return found


def first_difference(a, b):
    for at, (x, y) in enumerate(zip(a, b)):
        if x != y:
            return at
    return min(len(a), len(b))


class Checker:
    def __init__(self, tool, work):
        self.tool = tool
        self.theirs = os.path.join(work, "theirs")
        self.ours = os.path.join(work, "ours")
        self.count = 0
        self.kinds = {}

    def case(self, shown, files, commands, stdin=None):
        """Runs every command with both tools on the same files. Returns a description of the first difference, or None."""
        fill(self.theirs, files)
        fill(self.ours, files)
        for command in commands:
            given = stdin if command == ["fmt", "-"] else None
            want = run([AZOL] + command, cwd=self.theirs, input=given)
            got = run([self.tool] + command, cwd=self.ours, input=given)
            said = f"{shown}: `{' '.join(command)}`"
            if want.returncode != got.returncode:
                return (f"{said}: exit code {got.returncode}, expected {want.returncode}\n  azol:      {want.stderr[:300]!r}\n"
                        f"  azol-self: {got.stderr[:300]!r}")
            for what, a, b in [("standard output", want.stdout, got.stdout), ("standard error", without_times(want.stderr), without_times(got.stderr))]:
                if a != b:
                    at = first_difference(a, b)
                    return (f"{said}: {what} differs at byte {at} (azol {len(a)} bytes, azol-self {len(b)} bytes)\n"
                            f"  azol:      {a[max(0, at - 80):at + 120]!r}\n  azol-self: {b[max(0, at - 80):at + 120]!r}")
            a, b = snapshot(self.theirs), snapshot(self.ours)
            if a != b:
                names = sorted(name for name in set(a) | set(b) if a.get(name) != b.get(name))
                return f"{said}: the files afterwards differ: {', '.join(names[:5])}"
            self.count += 1
            kind = command[0] if command else "(nothing)"
            if kind in ("check", "build") and "--json" in command:
                kind += " --json"
            if kind == "fmt":
                kind += " --check" if "--check" in command else (" -" if command == ["fmt", "-"] else " in place")
            self.kinds[kind] = self.kinds.get(kind, 0) + 1
        return None


def uglier(text):
    """The same program laid out badly: no indentation, blanks at line ends, Windows line ends, blank lines doubled."""
    lines = []
    for line in text.split("\n"):
        code = line.lstrip(" \t")
        lines.append(code + ("  " if code else ""))
        if not code:
            lines.append("")
    return "\r\n".join(lines)


def damaged_programs(quick):
    """Right programs, damaged as in check_self.py."""
    made = []
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
            made.append((f"{stem} {kind}", new_text))
    return made[::3] if quick else made


def main():
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        sys.exit("check_cli.py runs azol-self directly and so needs x86-64 Linux")
    if not os.path.exists(AZOL):
        sys.exit("build the compiler first: cargo build --release")
    work = tempfile.mkdtemp(prefix="azol-check-cli-")
    try:
        return check(work, "--quick" in sys.argv[1:])
    finally:
        shutil.rmtree(work, ignore_errors=True)


def check(work, quick):
    for generator in ["gen_runtime_azl.py", "gen_tables_azl.py"]:
        stale = run([sys.executable, os.path.join(HERE, generator), "--check"])
        if stale.returncode != 0:
            sys.exit(stale.stderr.decode().strip())
    tool = os.path.join(work, "azol-self")
    built = run([AZOL, "build", "--target", "x86_64-linux", "-o", tool] + [os.path.join(HERE, name) for name in check_self.SOURCES])
    if built.returncode != 0:
        sys.exit("azol-self does not build:\n" + built.stderr.decode(errors="replace"))
    checker = Checker(tool, work)

    def expect(shown, files, commands, stdin=None):
        problem = checker.case(shown, files, commands, stdin)
        if problem:
            print("DIFFERENT " + problem)
            sys.exit(1)
        print(f"same  {shown}", flush=True)

    # The command line itself.
    for command in USAGE:
        expect("usage", USAGE_FILES, [command])
    listed = run([AZOL, "explain"]).stdout.decode().split("\n")
    codes = [line[:5] for line in listed if line]
    if len(codes) < 60:
        sys.exit("`azol explain` lists only %d codes" % len(codes))
    expect("explain", [], [["explain"], ["explain", "--markdown"]] + [["explain", code] for code in codes])
    made = run([tool, "explain", "--markdown"]).stdout
    if made != open(os.path.join(ROOT, "docs", "errors.md"), "rb").read():
        print("DIFFERENT `azol-self explain --markdown` is not docs/errors.md")
        return 1

    # The other targets: `--emit-code` exists for macOS and is refused
    # for Linux on ARM64, by both in the same words.
    for target in ["aarch64-macos", "aarch64-linux"]:
        expect(f"build for {target}", USAGE_FILES,
               [["build", "a.azl", "--target", target], ["build", "--target", target, "--json", "a.azl", "-o", "out"],
                ["build", "a.azl", "--target", target, "--emit-code", "code.txt"], ["build", "--emit-code", "code.txt", "--target", target, "a.azl", "-o", "folder/../out2"]])
        expect(f"wrong program for {target}", USAGE_FILES, [["check", "--target", target, "a.azl"], ["build", "--target", target, "wrong.azl"],
                                                               ["check", "--json", "--target", target, "wrong.azl"]])

    # Every source file of the repository: as it is, and made ugly.
    files = []
    for pattern in ["tests/programs/*.azl", "tests/multi/*/*.azl", "tests/format/*.azl", "examples/*.azl", "bench/*.azl", "compiler/*.azl",
                    "src/prelude.azl"]:
        files += sorted(glob.glob(os.path.join(ROOT, pattern)))
    if len(files) < 100:
        sys.exit("only %d source files found" % len(files))
    for path in files:
        shown = os.path.relpath(path, ROOT)
        text = open(path, "rb").read()
        commands = [["check", "in.azl"], ["check", "--json", "in.azl"], ["parse", "in.azl"], ["fmt", "--check", "in.azl", "ugly.azl"],
                    ["fmt", "-"], ["fmt", "in.azl", "ugly.azl"], ["fmt", "--check", "in.azl", "ugly.azl"]]
        if len(text) < 200000:
            commands.insert(2, ["lex", "in.azl"])
        if "/programs/" in shown or "/examples/" in shown or shown.startswith("examples"):
            commands += [["build", "--json", "in.azl"], ["build", "in.azl", "--emit-code", "code.txt", "-o", "out"],
                         ["build", "--target", "aarch64-macos", "in.azl", "--emit-code", "code_macos.txt", "-o", "out_macos"],
                         ["build", "--target", "aarch64-linux", "--json", "in.azl", "-o", "out_linux"]]
        ugly = uglier(text.decode("utf-8"))
        expect(shown, [("in.azl", text), ("ugly.azl", ugly)], commands, stdin=ugly.encode("utf-8"))

    # Programs of several files.
    for paths in check_self.SETS:
        names = ["%d_%s" % (at, os.path.basename(path)) for at, path in enumerate(paths)]
        texts = [open(os.path.join(ROOT, path), "rb").read() for path in paths]
        expect(" + ".join(paths), list(zip(names, texts)), [["check"] + names, ["check", "--json"] + names, ["build", "--json", "-o", "out"] + names,
                                                           ["fmt", "--check"] + names])

    # Programs with several mistakes, and mistakes with fixes.
    for name, sources in EXTRA.items():
        names = [source[0] for source in sources]
        first = sources[0][1].encode("utf-8")
        expect("extra " + name, sources, [["check"] + names, ["check", "--json"] + names, ["build"] + names, ["build", "--json"] + names,
                                          ["lex", names[0]], ["parse", names[0]], ["fmt", "--check"] + names, ["fmt", "-"], ["fmt"] + names],
               stdin=first)

    # Wrong programs.
    wrong = [("syntax " + name, text) for name, text in check_front.BROKEN.items()]
    wrong += [("wrong " + name, text) for name, text in check_sema.BROKEN.items()]
    wrong += [("corner " + name, text) for name, text in check_sema.CORNERS.items()]
    wrong += [("damaged " + name, text) for name, text in damaged_programs(quick)]
    for shown, text in wrong:
        commands = [["check", "in.azl"], ["check", "--json", "in.azl"], ["parse", "in.azl"], ["fmt", "--check", "in.azl"], ["fmt", "-"],
                    ["fmt", "in.azl"]]
        if len(text) < 200000:
            commands.insert(2, ["lex", "in.azl"])
        expect(shown, [("in.azl", text)], commands, stdin=text.encode("utf-8"))

    if not quick:
        if not os.path.exists(BIG):
            sys.exit("bench/generated/big.azl is missing: python3 bench/gen.py")
        expect("bench/generated/big.azl", [("big.azl", open(BIG, "rb").read())],
               [["check", "big.azl"], ["check", "--json", "big.azl"], ["parse", "--time", "big.azl"], ["lex", "--time", "big.azl"],
                ["fmt", "--check", "big.azl"]])

    print()
    print(f"{checker.count} comparisons: " + ", ".join(f"{count} {kind}" for kind, count in sorted(checker.kinds.items())))
    print("ALL IDENTICAL")
    return 0


if __name__ == "__main__":
    sys.exit(main())
