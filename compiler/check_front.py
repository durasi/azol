#!/usr/bin/env python3
"""Checks the lexer and parser written in azoL against the ones in the Rust compiler.

`azol-front`, built here from the azoL sources in this folder, must write
what `azol lex` and `azol parse` write: the same standard output, the same
standard error and the same exit code. Only the line of timings that the
Rust tool adds to standard error and the error code it puts before a
message are left out of the comparison.

It is run on every .azl file of tests/programs, tests/multi, examples,
this folder and src/prelude.azl, on bench/generated/big.azl, and on
deliberately broken texts: one or more for every message the lexer and
the parser can give, and a set of examples damaged at regular places.

    python3 compiler/check_front.py           everything
    python3 compiler/check_front.py --quick   without bench/generated/big.azl

`azol lex` finds the line of every token by reading the text from its
start, so listing the tokens of big.azl takes it minutes; `azol-front`
does the same in a fraction of a second. The timings printed at the end
are of the work alone (`--time`: nothing is printed), best of five runs.

`azol-front` is built for x86-64 Linux and run directly, so this script
needs such a machine. Build the Rust compiler first: cargo build --release
"""
import glob, os, platform, re, shutil, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
AZOL = os.path.join(ROOT, "target", "release", "azol")
BIG = os.path.join(ROOT, "bench", "generated", "big.azl")
SOURCES = ["front_main.azl", "driver.azl", "token.azl", "lexer.azl", "ast.azl", "parser.azl", "dump.azl"]
MODES = ["lex", "parse"]
TIMING = re.compile(rb"^(lexed|parsed) .*\n", re.M)

# Broken texts. Every message of src/lexer.rs and src/parser.rs is
# reached by at least one of them.
BROKEN = {
    # ---- lexer ----
    "hex_no_digits": "fn main() {\n  let a = 0x\n}\n",
    "hex_bad_digit": "fn main() {\n  let a = 0xZ1\n}\n",
    "hex_only_underscore": "fn main() {\n  let a = 0x_\n}\n",
    "binary_no_digits": "fn main() {\n  let a = 0b\n}\n",
    "binary_bad_digit": "fn main() {\n  let a = 0b2\n}\n",
    "string_open_at_line_end": "fn main() {\n  print(\"abc\n}\n",
    "string_open_at_file_end": "fn main() {\n  print(\"abc",
    "string_open_after_backslash": "fn main() {\n  print(\"abc\\",
    "string_bad_escape": "fn main() {\n  print(\"a\\qb\")\n}\n",
    "char_bad_escape": "fn main() {\n  let c = '\\q'\n}\n",
    "char_empty": "fn main() {\n  let c = ''\n}\n",
    "char_too_long": "fn main() {\n  let c = 'ab'\n}\n",
    "char_not_ascii": "fn main() {\n  let c = 'ş'\n}\n",
    "char_open_at_file_end": "fn main() {\n  let c = '",
    "char_open_after_one": "fn main() {\n  let c = 'a",
    "char_newline": "fn main() {\n  let c = '\n'\n}\n",
    "unexpected_character": "fn main() {\n  let a = 1 @ 2\n}\n",
    "unexpected_semicolon": "fn main() {\n  let a = 1;\n}\n",
    "unexpected_not_ascii": "fn main() {\n  let ğ = 1\n}\n",
    "unexpected_after_not_ascii": "fn main() {\n  print(\"şğü çöı\") $\n}\n",
    "unexpected_single_and": "fn main() {\n  let a = 1 ~ 2\n}\n",
    "unexpected_in_crlf": "fn main() {\r\n  let a = #\r\n}\r\n",
    "unexpected_tab_before": "fn main() {\n\t\tlet a = `\n}\n",
    "unexpected_long_line_start": "fn main() {\n  let a = ? + " + "1 + " * 60 + "1\n}\n",
    "unexpected_long_line_middle": "fn main() {\n  let a = " + "1 + " * 40 + "? + " + "1 + " * 40 + "1\n}\n",
    "unexpected_long_line_end": "fn main() {\n  let a = " + "1 + " * 60 + "?\n}\n",
    "unexpected_long_line_not_ascii": "fn main() {\n  print(\"" + "şğü" * 50 + "\") ? print(\"" + "çöı" * 50 + "\")\n}\n",
    "unexpected_long_line_exact": "fn main() {\n" + "a" * 99 + "?\n}\n",
    "unexpected_long_line_over": "fn main() {\n" + "a" * 100 + "?\n}\n",
    "unexpected_line_1000": "\n" * 999 + "?\n",
    # ---- parser ----
    "item_expected": "let a = 1\n",
    "item_expected_number": "42\n",
    "item_not_at_line_end": "struct A {\n}  struct B {\n}\n",
    "const_not_at_line_end": "const A = 1 const B = 2\n",
    "fn_name": "fn (a: i64) {\n}\n",
    "fn_name_keyword": "fn match() {\n}\n",
    "fn_no_paren": "fn main {\n}\n",
    "fn_no_body": "fn main()\n",
    "fn_body_at_file_end": "fn main()",
    "generic_name": "fn first[1](a: i64) {\n}\n",
    "generic_not_closed": "fn first[T(a: T) {\n}\n",
    "param_name": "fn f(1: i64) {\n}\n",
    "param_colon": "fn f(a i64) {\n}\n",
    "param_type": "fn f(a: ) {\n}\n",
    "param_not_closed": "fn f(a: i64 b: i64) {\n}\n",
    "return_type": "fn f() -> {\n}\n",
    "type_argument": "fn f(a: List[]) {\n}\n",
    "type_not_closed": "fn f(a: List[i64) {\n}\n",
    "struct_name": "struct {\n}\n",
    "struct_no_brace": "struct A\n",
    "struct_field_name": "struct A {\n  1: i64\n}\n",
    "struct_field_colon": "struct A {\n  a i64\n}\n",
    "struct_field_type": "struct A {\n  a: 5\n}\n",
    "struct_separator": "struct A {\n  a: i64 b: i64\n}\n",
    "struct_open_at_file_end": "struct A {\n  a: i64\n",
    "enum_name": "enum {\n}\n",
    "enum_no_brace": "enum A\n",
    "enum_variant_name": "enum A {\n  1\n}\n",
    "enum_variant_type": "enum A {\n  B()\n}\n",
    "enum_variant_not_closed": "enum A {\n  B(i64\n}\n",
    "enum_separator": "enum A {\n  B C\n}\n",
    "const_name": "const = 1\n",
    "const_type": "const A: = 1\n",
    "const_no_value": "const A\n",
    "const_value": "const A =\n",
    "const_inside_function": "fn main() {\n  const A = 1\n}\n",
    "block_open_at_file_end": "fn main() {\n  let a = 1\n",
    "block_open_at_file_end_nested": "fn main() {\n  if true {\n    while false {\n",
    "statement_not_at_line_end": "fn main() {\n  let a = 1 let b = 2\n}\n",
    "let_name": "fn main() {\n  let 1 = 2\n}\n",
    "let_type": "fn main() {\n  let a: = 2\n}\n",
    "let_no_value": "fn main() {\n  var a\n}\n",
    "let_value": "fn main() {\n  let a = \n\n}\n",
    "return_value": "fn main() {\n  return )\n}\n",
    "if_condition": "fn main() {\n  if {\n  }\n}\n",
    "if_no_block": "fn main() {\n  if true\n}\n",
    "if_struct_literal": "fn main() {\n  if a == P { x: 1 } {\n  }\n}\n",
    "else_no_block": "fn main() {\n  if true {\n  } else 5\n}\n",
    "while_no_block": "fn main() {\n  while true print(1)\n}\n",
    "for_name": "fn main() {\n  for 1 in a {\n  }\n}\n",
    "for_second_name": "fn main() {\n  for i, in a {\n  }\n}\n",
    "for_no_in": "fn main() {\n  for i a {\n  }\n}\n",
    "for_no_block": "fn main() {\n  for i in a\n}\n",
    "match_no_brace": "fn main() {\n  match a\n}\n",
    "match_pattern": "fn main() {\n  match a {\n    + => {}\n  }\n}\n",
    "match_pattern_minus": "fn main() {\n  match a {\n    -x => {}\n  }\n}\n",
    "match_pattern_inner": "fn main() {\n  match a {\n    Some() => {}\n  }\n}\n",
    "match_pattern_not_closed": "fn main() {\n  match a {\n    Some(x => {}\n  }\n}\n",
    "match_no_arrow": "fn main() {\n  match a {\n    1 {}\n  }\n}\n",
    "match_arm_statement": "fn main() {\n  match a {\n    1 => \n  }\n}\n",
    "match_separator": "fn main() {\n  match a {\n    1 => {} 2 => {}\n  }\n}\n",
    "match_open_at_file_end": "fn main() {\n  match a {\n    1 => {}\n",
    "assign_to_call": "fn main() {\n  f() = 1\n}\n",
    "assign_to_number": "fn main() {\n  1 = 2\n}\n",
    "assign_to_sum": "fn main() {\n  var a = 1\n      a + 1 = 2\n}\n",
    "assign_to_literal": "fn main() {\n  P { x: 1 } = 2\n}\n",
    "assign_value": "fn main() {\n  a = \n  }\n}\n",
    "expression_expected": "fn main() {\n  let a = 1 + \n\n  * 2\n}\n",
    "expression_keyword": "fn main() {\n  let a = let\n}\n",
    "unary_operand": "fn main() {\n  let a = -\n}\n",
    "call_not_closed": "fn main() {\n  f(1, 2\n}\n",
    "call_no_comma": "fn main() {\n  f(1 2)\n}\n",
    "call_mut_nothing": "fn main() {\n  f(mut )\n}\n",
    "index_not_closed": "fn main() {\n  let a = b[1\n}\n",
    "index_empty": "fn main() {\n  let a = b[]\n}\n",
    "field_name": "fn main() {\n  let a = b.1\n}\n",
    "field_name_next_line": "fn main() {\n  let a = b\n    .\n}\n",
    "paren_not_closed": "fn main() {\n  let a = (1 + 2\n}\n",
    "paren_empty": "fn main() {\n  let a = ()\n}\n",
    "array_not_closed": "fn main() {\n  let a = [1, 2\n}\n",
    "array_no_comma": "fn main() {\n  let a = [1 2]\n}\n",
    "literal_field_name": "fn main() {\n  let a = P { 1: 2 }\n}\n",
    "literal_field_colon": "fn main() {\n  let a = P { x 2 }\n}\n",
    "literal_field_value": "fn main() {\n  let a = P { x: }\n}\n",
    "literal_not_closed": "fn main() {\n  let a = P { x: 1 y: 2 }\n}\n",
    "nested_parens": "fn main() {\n  let a = " + "(" * 300 + "1" + ")" * 300 + "\n}\n",
    "nested_minus": "fn main() {\n  let a = " + "-" * 300 + "1\n}\n",
    "nested_brackets": "fn main() {\n  let a = " + "[" * 300 + "]" * 300 + "\n}\n",
    "nested_blocks": "fn main() {\n" + "if a {\n" * 300 + "}\n" * 300 + "}\n",
    "nested_else_if": "fn main() {\n  if a {\n  }" + " else if a {\n  }" * 300 + "\n}\n",
    "nested_types": "fn f(a: " + "List[" * 300 + "i64" + "]" * 300 + ") {\n}\n",
    "nested_patterns": "fn main() {\n  match a {\n    " + "Some(" * 300 + "x" + ")" * 300 + " => {}\n  }\n}\n",
    "nested_literals": "fn main() {\n  let a = " + "P { x: " * 300 + "1" + " }" * 300 + "\n}\n",
    # ---- texts that are right, at the edges ----
    "ok_empty": "",
    "ok_only_newlines": "\n\n\n",
    "ok_only_comment": "// nothing here",
    "ok_no_final_newline": "fn main() {\n}",
    "ok_crlf": "fn main() {\r\n  print(\"a\")\r\n}\r\n",
    "ok_nested_just_enough": "fn main() {\n  let a = " + "(" * 150 + "1" + ")" * 150 + "\n}\n",
    "ok_dot_on_next_line": "fn main() {\n  let a = b\n\n    .c\n    .d(1)\n}\n",
    "ok_not_ascii": "fn main() {\n  // şğü\n  print(\"çöı\")  // ığ\n  let a = 1\n}\n",
    # ---- a struct literal where a block is expected, and `?` ----
    "literal_in_if": "fn main() {\n  if a == P { x: 1 } {\n  }\n}\n",
    "literal_in_while_next_line": "fn main() {\n  while f(a) != P {\n    x: 1\n  } {\n  }\n}\n",
    "literal_in_match": "fn main() {\n  match P { x: 1 } {\n  }\n}\n",
    "literal_in_for": "fn main() {\n  for x in P { xs: [1] }.xs {\n  }\n}\n",
    "block_after_name_is_fine": "fn main() {\n  if a {\n    b.c(1)\n  }\n  if a == b {\n    x = 1\n  }\n}\n",
    "try_everywhere": "fn f() -> Option[i64] {\n  let a = g()? + h(1)?.x\n  g()?\n  return Some(a??[0]?)\n}\n",
    "try_alone": "fn main() {\n  ?\n}\n",
    # ---- numbers the checker looks at, not the lexer ----
    "numbers_of_every_form": "fn main() {\n  let a = 0 + 10 + 1_000 + 0x0F + 0xff_ff + 0b0_1 + 007 + 1__0 + 0x_F\n}\n",
}

# Pieces put into right programs to damage them.
PIECES = ["{", "}", "(", ")", "[", "]", ",", "=", "let ", "\"", "'", "\n", " 1 ", " else ", "=>", ".", "@", " match ", "-", ":"]


CODE = re.compile(rb"^error\[E\d{4}\]:")


def run(command):
    return subprocess.run(command, capture_output=True)


def result_of(tool, mode, path, rust):
    done = run([tool, mode, path])
    err = done.stderr
    if rust and done.returncode == 0:
        err = TIMING.sub(b"", err, count=1)
    if rust:
        # The Rust tool adds a code, `error[E0109]:`, from a table that the
        # azoL front end does not have yet; the message itself is compared.
        err = CODE.sub(b"error:", err, count=1)
    return done.returncode, done.stdout, err


def first_difference(a, b):
    for at, (x, y) in enumerate(zip(a, b)):
        if x != y:
            return at
    return min(len(a), len(b))


def compare(front, mode, path, shown):
    """Returns None when both tools agree, else a description."""
    want = result_of(AZOL, mode, path, True)
    got = result_of(front, mode, path, False)
    for name, a, b in [("exit code", want[0], got[0]), ("standard output", want[1], got[1]), ("standard error", want[2], got[2])]:
        if a == b:
            continue
        if name == "exit code":
            return f"{mode} {shown}: exit code {b}, expected {a}\n  azol: {want[2][:300]!r}\n  azol-front: {got[2][:300]!r}"
        at = first_difference(a, b)
        return (f"{mode} {shown}: {name} differs at byte {at} (azol {len(a)} bytes, azol-front {len(b)} bytes)\n"
                f"  azol:       {a[max(0, at - 60):at + 60]!r}\n  azol-front: {b[max(0, at - 60):at + 60]!r}")
    return None


def best_time(command, runs=5):
    best = None
    for _ in range(runs):
        started = time.perf_counter()
        done = run(command)
        took = time.perf_counter() - started
        if done.returncode != 0:
            sys.exit("cannot time " + " ".join(command) + ":\n" + done.stderr.decode(errors="replace"))
        best = took if best is None else min(best, took)
    return best * 1000


def main():
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        sys.exit("check_front.py runs azol-front directly and so needs x86-64 Linux")
    if not os.path.exists(AZOL):
        sys.exit("build the compiler first: cargo build --release")
    work = tempfile.mkdtemp(prefix="azol-check-front-")
    try:
        return check(work, "--quick" in sys.argv[1:])
    finally:
        shutil.rmtree(work, ignore_errors=True)


def check(work, quick):
    front = os.path.join(work, "azol-front")
    built = run([AZOL, "build", "--target", "x86_64-linux", "-o", front] + [os.path.join(HERE, name) for name in SOURCES])
    if built.returncode != 0:
        sys.exit("azol-front does not build:\n" + built.stderr.decode(errors="replace"))

    files = []
    for pattern in ["tests/programs/*.azl", "tests/multi/*/*.azl", "examples/*.azl", "compiler/*.azl", "src/prelude.azl"]:
        files += sorted(glob.glob(os.path.join(ROOT, pattern)))
    if not files:
        sys.exit("no programs found")

    broken = os.path.join(work, "broken")
    os.mkdir(broken)
    made = []
    for name, text in BROKEN.items():
        path = os.path.join(broken, name + ".azl")
        with open(path, "w", encoding="utf-8", newline="") as out:
            out.write(text)
        made.append(path)

    # Right programs damaged at regular places: a piece put in, or the
    # text cut off.
    damaged = []
    seeds = sorted(glob.glob(os.path.join(ROOT, "examples", "*.azl")))[:6] + [os.path.join(HERE, "dump.azl")]
    for seed_at, seed in enumerate(seeds):
        text = open(seed, encoding="utf-8").read()
        step = max(1, len(text) // 40)
        for round_at, at in enumerate(range(step // 2, len(text), step)):
            piece = PIECES[(round_at + seed_at) % len(PIECES)]
            kinds = [("put", text[:at] + piece + text[at:])]
            if round_at % 4 == 0:
                kinds.append(("cut", text[:at]))
            for kind, changed in kinds:
                path = os.path.join(broken, f"damaged_{os.path.basename(seed)[:-4]}_{kind}_{at}.azl")
                with open(path, "w", encoding="utf-8", newline="") as out:
                    out.write(changed)
                damaged.append(path)

    count = 0
    messages = set()
    for group, paths in [("", files), ("broken ", made), ("damaged ", damaged)]:
        for path in paths:
            shown = group + (os.path.relpath(path, ROOT) if not group else os.path.basename(path))
            if group:
                # Paths in the temporary folder are shown as given, so
                # both tools are handed the same one.
                first = run([AZOL, "parse", path]).stderr.split(b"\n")[0]
                if first.startswith(b"error: "):
                    messages.add(first)
            for mode in MODES:
                problem = compare(front, mode, path, shown)
                if problem:
                    print("DIFFERENT " + problem)
                    return 1
                print(f"same  {mode:<5} {shown}")
                count += 1

    if not quick:
        if not os.path.exists(BIG):
            sys.exit("bench/generated/big.azl is missing: python3 bench/gen.py")
        for mode in ["parse", "lex"]:
            if mode == "lex":
                print("      (azol lex takes minutes on this file)", flush=True)
            problem = compare(front, mode, BIG, "bench/generated/big.azl")
            if problem:
                print("DIFFERENT " + problem)
                return 1
            print(f"same  {mode:<5} bench/generated/big.azl", flush=True)
            count += 1
        print()
        print("big.azl, the work alone, best of 5 (whole process: start, read the file, work):")
        for mode in MODES:
            rust = best_time([AZOL, mode, "--time", BIG])
            mine = best_time([front, mode, "--time", BIG])
            inner = run([AZOL, mode, "--time", BIG]).stderr.decode().strip()
            print(f"  {mode:<5} azol {rust:7.1f} ms   azol-front {mine:7.1f} ms   ({mine / rust:.2f}x)")
            print(f"        azol's own clock: {inner}")

    print()
    print(f"{count} comparisons, {len(messages)} different error messages among the broken texts")
    print("ALL IDENTICAL")
    return 0


if __name__ == "__main__":
    sys.exit(main())
