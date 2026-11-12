#!/usr/bin/env python3
"""Checks the checker written in azoL against the one in the Rust compiler.

`azol-sema`, built here from the azoL sources in this folder, must write
what `azol sema-dump` writes: every table the checker hands to the code
generators (types, layouts, functions, and what it found out about each
expression, statement and pattern), or the same problems at the same
places, and give the same exit code.

It is run on every .azl file of tests/programs, examples, bench, this
folder and src/prelude.azl, each as a program of its own; on the programs
of several files (tests/multi and the tools in this folder); on
bench/generated/big.azl; on deliberately wrong programs, one or more for
every message the checker can give; and on right programs damaged at
regular places.

    python3 compiler/check_sema.py           everything
    python3 compiler/check_sema.py --quick   without bench/generated/big.azl

The list of messages comes from `azol explain`, so a message added to the
Rust checker without a program here that reaches it fails this check.

`azol-sema` is built for x86-64 Linux and run directly, so this script
needs such a machine. Build the Rust compiler first: cargo build --release
"""
import glob, os, platform, re, shutil, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
AZOL = os.path.join(ROOT, "target", "release", "azol")
BIG = os.path.join(ROOT, "bench", "generated", "big.azl")
PRELUDE = os.path.join(ROOT, "src", "prelude.azl")
FRONT = ["driver.azl", "token.azl", "lexer.azl", "ast.azl", "parser.azl"]
SOURCES = ["sema_main.azl"] + FRONT + ["sema.azl", "sema_fn.azl", "sema_dump.azl"]

# Programs of several files, as lists of paths under the repository.
SETS = [
    ["tests/multi/shop/main.azl", "tests/multi/shop/numbers.azl", "tests/multi/shop/stock.azl"],
    ["tests/multi/shop/stock.azl", "tests/multi/shop/numbers.azl", "tests/multi/shop/main.azl"],
    ["tests/multi/broken/main.azl", "tests/multi/broken/helper.azl"],
    ["tests/multi/broken/main.azl", "tests/multi/broken/helper.azl", "tests/multi/broken/after.azl"],
    ["tests/multi/broken/unclosed_main.azl", "tests/multi/broken/unclosed.azl", "tests/multi/broken/after.azl"],
    ["compiler/" + name for name in SOURCES],
    ["compiler/" + name for name in ["front_main.azl"] + FRONT + ["dump.azl"]],
    ["compiler/" + name for name in ["link_main.azl", "bytes.azl", "sha256.azl", "code_file.azl", "macho.azl", "elf.azl"]],
    ["compiler/" + name for name in ["sha256_test.azl", "bytes.azl", "sha256.azl"]],
]


def M(body):
    return "fn main() {\n" + body + "\n}\n"


P = "struct P {\n    x: i64\n    y: i64\n}\n"
F = "fn nothing() {\n}\n"
TRY = "fn number(s: str) -> Result[i64, str] {\n    return Ok(1)\n}\nfn words(s: str) -> Result[List[str], str] {\n    return Ok([s])\n}\nfn maybe() -> Option[i64] {\n    return None\n}\nstruct PQ {\n    a: List[str]\n    n: i64\n}\n"
MANY = 2100

# Wrong programs. Every message of the checker is reached by at least
# one of them (see `uncovered`).
BROKEN = {
    # ---- names ----
    "unknown_variable": M("print(x)"),
    "unknown_function": M("foo()"),
    "unknown_type": "fn f(a: Foo) {\n}\n" + M(""),
    "unknown_type_in_let": M("let a: Foo = 1"),
    "unknown_type_in_struct": "struct A {\n    a: Foo\n}\n" + M(""),
    "unknown_type_in_generic": "fn f[T](a: Foo[T]) {\n}\n" + M(""),
    "unknown_struct": M("let p = Foo { x: 1 }"),
    "unknown_variant": M("let o = Some(1)\nmatch o {\nFoo(x) => {}\n_ => {}\n}"),
    "no_field": P + M("let p = P { x: 1, y: 2 }\nprint(p.z)"),
    "no_field_list": M("let xs = [1]\nprint(xs.size)"),
    "no_field_enum": M("let o = Some(1)\nprint(o.x)"),
    "no_field_number": M("let n = 1\nprint(n.x)"),
    "no_method_list": M("var n = [1]\nn.foo()"),
    "no_method_struct": P + M("let p = P { x: 1, y: 2 }\np.area()"),
    "no_method_str": M("print(\"a\".foo())"),
    "no_method_generic_receiver": M("let n = 1\nprint(n.first())"),
    "fn_twice": "fn f() {\n}\nfn f() {\n}\n" + M(""),
    "fn_twice_generic": "fn f[T](a: T) {\n}\nfn f[T](a: T) {\n}\n" + M(""),
    "fn_twice_generic_then_plain": "fn f[T](a: T) {\n}\nfn f() {\n}\n" + M(""),
    "fn_twice_plain_then_generic": "fn f() {\n}\nfn f[T](a: T) {\n}\n" + M(""),
    "struct_twice": P + P + M(""),
    "enum_twice": "enum A {\n    X\n}\nenum A {\n    Y\n}\n" + M(""),
    "const_twice": "const K = 1\nconst K = 2\n" + M(""),
    "struct_already_enum": "enum A {\n    X\n}\nstruct A {\n    x: i64\n}\n" + M(""),
    "enum_already_struct": "struct A {\n    x: i64\n}\nenum A {\n    X\n}\n" + M(""),
    "variant_twice": "enum A {\n    X\n    X\n}\n" + M(""),
    "variant_of_other": "enum A {\n    X\n}\nenum B {\n    X\n}\n" + M(""),
    "variant_some": "enum A {\n    Some\n}\n" + M(""),
    "field_twice": "struct A {\n    x: i64\n    x: i64\n}\n" + M(""),
    "type_parameter_twice_struct": "struct A[T, T] {\n    x: T\n}\n" + M(""),
    "type_parameter_twice_enum": "enum A[T, T] {\n    X(T)\n}\n" + M(""),
    "type_parameter_twice_fn": "fn f[T, T](x: T) {\n}\n" + M(""),
    "fn_named_variant": "enum A {\n    X\n}\nfn X() {\n}\n" + M(""),
    "const_named_variant": "enum A {\n    X\n}\nconst X = 1\n" + M(""),
    "fn_named_constant": "const K = 1\nfn K() {\n}\n" + M(""),
    "builtin_type_struct": "struct i64 {\n    x: i64\n}\n" + M(""),
    "builtin_type_enum": "enum List {\n    A\n}\n" + M(""),
    "builtin_option": "struct Option {\n    x: i64\n}\n" + M(""),
    "builtin_result": "enum Result {\n    A\n}\n" + M(""),
    "builtin_fn": "fn print() {\n}\n" + M(""),
    "builtin_fn_str": "fn str() {\n}\n" + M(""),
    "builtin_const": "const exit = 1\n" + M(""),
    "function_as_value": M("let f = main"),
    "generic_function_as_value": "fn g[T](x: T) {\n}\n" + M("let f = g"),
    "builtin_as_value": M("let f = print"),
    "struct_as_value": P + M("let p = P"),
    "enum_as_value": "enum A {\n    X\n}\n" + M("let a = A"),
    "variable_called": M("let x = 1\nx()"),
    "index_called": M("let x = [1]\nx[0]()"),
    "struct_called": P + M("let p = P(1, 2)"),
    "variable_named_variant": M("let None = 1"),
    "variable_named_constant": "const K = 1\n" + M("let K = 2"),
    "loop_variable_named_variant": M("for Some in [1] {\n}"),
    "loop_index_named_constant": "const K = 1\n" + M("for K, x in [1] {\n}"),
    "parameter_named_variant": "fn f(Ok: i64) {\n}\n" + M(""),
    "enum_without_variants": "enum A {\n}\n" + M(""),
    # ---- types ----
    "let_type": M("let x: i64 = true"),
    "assign_type": M("var x = 1\nx = true"),
    "const_type": "const K: str = 1\n" + M(""),
    "operator_type": M("let x = 1 + true"),
    "operator_left_type": M("let x = true - 1"),
    "plus_left_type": M("let x = true + 1"),
    "plus_string_right": M("let x = \"a\" + 1"),
    "string_equal_right": M("let x = \"a\" == 1"),
    "compare_type": M("let x = 1 < true"),
    "and_type": M("let x = true && 1"),
    "equal_type": M("let x = 1 == true"),
    "negate_type": M("let x = -true"),
    "not_type": M("let x = !1"),
    "index_type": M("let xs = [1]\nprint(xs[true])"),
    "element_type": M("let xs = [1, true]"),
    "element_type_hinted": M("let xs: List[i64] = [true]"),
    "push_type": M("var xs = [1]\nxs.push(true)"),
    "argument_type": "fn f(a: i64) {\n}\n" + M("f(true)"),
    "argument_type_builtin": M("exit(true)"),
    "argument_type_second": M("write_file(\"a\", 1)"),
    "argument_type_slice": M("print(\"a\".slice(true, 1))"),
    "argument_type_slice_second": M("print(\"a\".slice(0, true))"),
    "argument_type_method": M("print(\"a\".find(1))"),
    "argument_type_variant": M("let o: Option[i64] = Some(true)"),
    "argument_type_variant_plain": "enum A {\n    X(i64)\n}\n" + M("let a = X(true)"),
    "field_type": P + M("let p = P { x: true, y: 1 }"),
    "field_type_generic_ready": "struct Pair[T] {\n    a: T\n    b: T\n}\n" + M("let p = Pair { a: 1, b: true }"),
    "field_type_generic_shape": "struct W[T] {\n    a: List[T]\n}\n" + M("let w = W { a: 1 }"),
    "field_type_generic_hinted": "struct W[T] {\n    a: T\n}\n" + M("let w: W[i64] = W { a: true }"),
    "argument_shape": "fn f[T](xs: List[T]) {\n}\n" + M("f(1)"),
    "argument_shape_ready": "fn f[T](a: T, b: T) {\n}\n" + M("f(1, true)"),
    "argument_shape_variant": "enum Tree[T] {\n    Leaf(List[T])\n}\n" + M("let t = Leaf(1)"),
    "argument_shape_variant_ready": "enum Two[T] {\n    Both(T, T)\n}\n" + M("let t = Both(1, true)"),
    "const_operator_type": "const K = 1 + true\n" + M(""),
    "const_operator_left_type": "const K = true + 1\n" + M(""),
    "const_negate_type": "const K = -true\n" + M(""),
    "const_not_type": "const K = !1\n" + M(""),
    "const_and_right_type": "const K = true && 1\n" + M(""),
    "const_and_left_type": "const K = 1 && true\n" + M(""),
    "const_equal_type": "const K = 1 == true\n" + M(""),
    "condition_if": M("if 1 {\n}"),
    "condition_while": M("while \"a\" {\n}"),
    "return_type": "fn f() -> i64 {\n    return true\n}\n" + M(""),
    "return_value_from_nothing": "fn f() {\n    return 1\n}\n" + M(""),
    "return_without_value": "fn f() -> i64 {\n    return\n}\n" + M(""),
    "return_missing": "fn f() -> i64 {\n}\n" + M(""),
    "return_missing_else": "fn f(a: bool) -> i64 {\n    if a {\n        return 1\n    }\n}\n" + M(""),
    "return_missing_else_if": "fn f(a: bool) -> i64 {\n    if a {\n        return 1\n    } else if !a {\n        return 2\n    }\n}\n" + M(""),
    "return_missing_arm": "fn f(a: bool) -> i64 {\n    match a {\n        true => return 1\n        false => {}\n    }\n}\n" + M(""),
    "return_missing_generic": "fn f[T](a: T) -> T {\n}\n" + M("print(f(1))"),
    "for_type": M("for x in 1 {\n}"),
    "indexed_type": M("let n = 1\nprint(n[0])"),
    "print_type": M("print([1])"),
    "str_type": M("let s = str(\"a\")"),
    "count_print": M("print(1, 2)"),
    "count_str": M("let s = str()"),
    "count_function": "fn f(a: i64) {\n}\n" + M("f()"),
    "count_function_one": "fn f(a: i64, b: i64) {\n}\n" + M("f(1)"),
    "count_len": M("print([1].len(1))"),
    "count_len_str": M("print(\"a\".len(1))"),
    "count_slice": M("print(\"a\".slice(1))"),
    "count_push": M("var xs = [1]\nxs.push()"),
    "count_pop": M("var xs = [1]\nxs.pop(1)"),
    "count_generic": "fn f[T](a: T) {\n}\n" + M("f()"),
    "count_method": M("print(\"a\".find())"),
    "count_generic_method": M("print([1].contains())"),
    "count_builtin": M("exit()"),
    "count_builtin_args": M("let a = args(1)"),
    "count_variant": M("let o: Option[i64] = Some(1, 2)"),
    "no_value_to_store": F + M("let x = nothing()"),
    "no_value_to_compare": F + M("if nothing() == 1 {\n}"),
    "no_value_to_match": F + M("match nothing() {\n_ => {}\n}"),
    "no_value_for_list": F + M("let xs = [nothing()]"),
    "no_value_to_pass": F + "fn g[T](x: T) {\n}\n" + M("g(nothing())"),
    "no_value_for_field": F + "struct W[T] {\n    a: T\n}\n" + M("let w = W { a: nothing() }"),
    "no_value_for_variant": F + M("let o = Some(nothing())"),
    "compare_structs": P + M("let p = P { x: 1, y: 2 }\nif p == p {\n}"),
    "compare_lists": M("let xs = [1]\nif xs != xs {\n}"),
    "compare_enums": M("let a = Some(1)\nif a == a {\n}"),
    "variant_needs_value": M("let o: Option[i64] = Some"),
    "variant_needs_values": "enum A {\n    X(i64, i64)\n}\n" + M("let a = X"),
    "variant_holds_nothing": M("let o: Option[i64] = None()"),
    "variant_holds_nothing_given": M("let o: Option[i64] = None(1)"),
    "field_given_twice": P + M("let p = P { x: 1, x: 2, y: 3 }"),
    "field_given_twice_generic": "struct W[T] {\n    a: T\n}\n" + M("let w = W { a: 1, a: 2 }"),
    "field_missing": P + M("let p = P { x: 1 }"),
    "field_missing_generic": "struct W[T] {\n    a: T\n    b: i64\n}\n" + M("let w = W { a: 1 }"),
    "field_unknown_generic": "struct W[T] {\n    a: T\n}\n" + M("let w = W { z: 1 }"),
    "no_main": "fn f() {\n}\n",
    "no_main_and_more": "fn f() {\n    print(x)\n}\nfn g() {\n    print(y)\n}\n",
    "main_parameters": "fn main(a: i64) {\n}\n",
    "main_return": "fn main() -> str {\n    return \"\"\n}\n",
    "main_generic": "fn main[T]() {\n}\n",
    "break_outside": M("break"),
    "continue_outside": M("if true {\n    continue\n}"),
    "number_too_large": M("let x = 9223372036854775808"),
    "number_too_large_hex": M("let x = 0x1FFFFFFFFFFFFFFFF"),
    "number_too_large_binary": M("let x = 0b1" + "0" * 64),
    "number_too_large_const": "const K = 99999999999999999999\n" + M(""),
    "number_too_large_const_hex": "const K = 0x1_0000_0000_0000_0000\n" + M(""),
    "pattern_too_large": M("match 1 {\n9223372036854775808 => {}\n_ => {}\n}"),
    "pattern_too_small": M("match 1 {\n-9223372036854775809 => {}\n_ => {}\n}"),
    "pattern_far_too_large": M("match 1 {\n99999999999999999999999 => {}\n_ => {}\n}"),
    "pattern_too_large_hex": M("match 1 {\n0x1_0000_0000_0000_0000 => {}\n_ => {}\n}"),
    "pattern_too_small_hex": M("match 1 {\n-0x8000_0000_0000_0000 => {}\n_ => {}\n}"),
    # ---- ownership and exclusivity ----
    "change_let": M("let x = 1\nx = 2"),
    "change_let_field": P + M("let p = P { x: 1, y: 2 }\np.x = 3"),
    "change_let_push": M("let xs = [1]\nxs.push(2)"),
    "pass_let": "fn f(a: mut i64) {\n}\n" + M("let x = 1\nf(mut x)"),
    "change_parameter": "fn f(a: i64) {\n    a = 1\n}\n" + M(""),
    "pass_parameter": "fn g(a: mut i64) {\n}\nfn f(a: i64) {\n    g(mut a)\n}\n" + M(""),
    "change_loop_variable": M("for x in [1] {\n    x = 2\n}"),
    "change_binding": M("let o = Some(1)\nmatch o {\nSome(x) => {\nx = 2\n}\nNone => {}\n}"),
    "pass_loop_variable": "fn g(a: mut i64) {\n}\n" + M("for x in [1] {\n    g(mut x)\n}"),
    "mut_missing": "fn f(a: mut i64) {\n}\n" + M("var x = 1\nf(x)"),
    "mut_missing_generic": "fn f[T](a: mut T) {\n}\n" + M("var x = 1\nf(x)"),
    "mut_not_wanted": "fn f(a: i64) {\n}\n" + M("var x = 1\nf(mut x)"),
    "mut_not_wanted_generic": "fn f[T](a: T) {\n}\n" + M("var x = 1\nf(mut x)"),
    "mut_outside_argument": M("var x = 1\nprint(mut x)"),
    "mut_needs_variable": "fn f(a: mut i64) {\n}\n" + M("f(mut 1)"),
    "push_needs_variable": M("[1].push(2)"),
    "pop_needs_variable": "fn f() -> List[i64] {\n    return [1]\n}\n" + M("f().pop()"),
    "assign_needs_variable": "fn f() -> List[i64] {\n    return [1]\n}\n" + M("f()[0] = 1"),
    "change_in_loop": M("var xs = [1]\nfor x in xs {\n    xs.push(x)\n}"),
    "assign_in_loop": M("var xs = [1]\nfor i, x in xs {\n    xs[i] = x\n}"),
    "pass_in_loop": "fn g(a: mut List[i64]) {\n}\n" + M("var xs = [1]\nfor x in xs {\n    g(mut xs)\n}"),
    "change_in_match": M("var o = Some(1)\nmatch o {\nSome(x) => {\no = None\n}\nNone => {}\n}"),
    "pass_in_match": "fn g(a: mut Option[i64]) {\n}\n" + M("var o = Some(1)\nmatch o {\nSome(x) => g(mut o)\nNone => {}\n}"),
    "passed_twice": "fn f(a: mut List[i64], b: List[i64]) {\n}\n" + M("var xs = [1]\nf(mut xs, xs)"),
    "passed_twice_other_order": "fn f(a: List[i64], b: mut List[i64]) {\n}\n" + M("var xs = [1]\nf(xs, mut xs)"),
    "passed_twice_part": "struct B {\n    xs: List[i64]\n}\nfn f(a: mut B, b: List[i64]) {\n}\n" + M("var b = B { xs: [1] }\nf(mut b, b.xs)"),
    "passed_twice_nested": "fn f(a: mut List[i64], b: i64) {\n}\nfn g(a: List[i64]) -> i64 {\n    return 1\n}\n" + M("var xs = [1]\nf(mut xs, g(xs))"),
    "changed_while_passed": "fn g(a: List[i64], b: i64) {\n}\n" + M("var xs = [1]\ng(xs, xs.pop())"),
    "changed_while_read": M("var xs = [\"a\"]\nlet t = xs[0] + xs.pop()"),
    "passed_while_read": "fn f(s: mut str) -> str {\n    return s\n}\n" + M("var s = \"a\"\nlet t = s + f(mut s)"),
    "string_changed_in_place": M("var s = \"ab\"\ns[0] = 1"),
    "struct_contains_itself": "struct A {\n    a: A\n}\n" + M(""),
    "structs_contain_each_other": "struct A {\n    b: B\n}\nstruct B {\n    a: A\n}\n" + M(""),
    "enum_contains_itself": "enum E {\n    X(E)\n}\n" + M(""),
    "generic_struct_contains_itself": "struct A[T] {\n    a: A[T]\n}\nfn f(a: A[i64]) {\n}\n" + M(""),
    "generic_struct_contains_itself_in_body": "struct A[T] {\n    a: A[T]\n}\n" + M("let a: List[A[i64]] = []"),
    # ---- patterns ----
    "match_misses_one": M("let o = Some(1)\nmatch o {\nSome(x) => {}\n}"),
    "match_misses_two": "enum C {\n    R\n    G(i64)\n    B(i64, i64)\n}\n" + M("let c = R\nmatch c {\nR => {}\n}"),
    "match_misses_three": "enum C {\n    R\n    G(i64)\n    B(i64, i64)\n    W\n}\n" + M("let c = R\nmatch c {\nR => {}\n}"),
    "match_misses_true": M("match true {\nfalse => {}\n}"),
    "match_misses_false": M("match true {\ntrue => {}\n}"),
    "match_number_needs_all": M("match 1 {\n1 => {}\n}"),
    "match_string_needs_all": M("match \"a\" {\n\"a\" => {}\n}"),
    "arm_after_all": M("match 1 {\n_ => {}\n1 => {}\n}"),
    "arm_after_name": M("match 1 {\nn => {}\n1 => {}\n}"),
    "arm_after_variants": M("let o = Some(1)\nmatch o {\nSome(x) => {}\nNone => {}\n_ => {}\n}"),
    "arm_after_bools": M("match true {\ntrue => {}\nfalse => {}\ntrue => {}\n}"),
    "arm_variant_again": M("let o = Some(1)\nmatch o {\nSome(x) => {}\nSome(y) => {}\nNone => {}\n}"),
    "arm_number_again": M("match 1 {\n1 => {}\n0x1 => {}\n_ => {}\n}"),
    "arm_negative_again": M("match 1 {\n-1 => {}\n-1 => {}\n_ => {}\n}"),
    "arm_bool_again": M("match true {\ntrue => {}\ntrue => {}\n}"),
    "arm_string_again": M("match \"a\" {\n\"a\\n\" => {}\n\"b\" => {}\n\"a\\n\" => {}\n_ => {}\n}"),
    "arm_constant_again": "const K = 1\n" + M("match 1 {\n1 => {}\nK => {}\n_ => {}\n}"),
    "arm_string_constant_again": "const K = \"a\"\n" + M("match \"a\" {\nK => {}\n\"a\" => {}\n_ => {}\n}"),
    "match_on_struct": P + M("let p = P { x: 1, y: 2 }\nmatch p {\n_ => {}\n}"),
    "match_on_list": M("match [1] {\n_ => {}\n}"),
    "pattern_number_on_string": M("match \"a\" {\n1 => {}\n_ => {}\n}"),
    "pattern_bool_on_number": M("match 1 {\ntrue => {}\n_ => {}\n}"),
    "pattern_string_on_number": M("match 1 {\n\"a\" => {}\n_ => {}\n}"),
    "pattern_constant_on_string": "const K = 1\n" + M("match \"a\" {\nK => {}\n_ => {}\n}"),
    "pattern_number_on_enum": M("let o = Some(1)\nmatch o {\n1 => {}\n_ => {}\n}"),
    "pattern_other_enum": "enum A {\n    X\n}\n" + M("let o = Some(1)\nmatch o {\nX => {}\n_ => {}\n}"),
    "pattern_variant_on_number": M("match 1 {\nNone => {}\n_ => {}\n}"),
    "pattern_variant_with_values_on_number": M("match 1 {\nSome(x) => {}\n_ => {}\n}"),
    "pattern_needs_values": M("let o = Some(1)\nmatch o {\nSome => {}\nNone => {}\n}"),
    "pattern_needs_two_values": "enum A {\n    X(i64, i64)\n}\n" + M("let a = X(1, 2)\nmatch a {\nX => {}\n}"),
    "pattern_holds_nothing": M("let o = Some(1)\nmatch o {\nSome(x) => {}\nNone(y) => {}\n}"),
    "pattern_wrong_count": "enum A {\n    X(i64, i64)\n}\n" + M("let a = X(1, 2)\nmatch a {\nX(b) => {}\n}"),
    "pattern_name_twice": "enum A {\n    X(i64, i64)\n}\n" + M("let a = X(1, 2)\nmatch a {\nX(b, b) => {}\n}"),
    "pattern_in_pattern": M("let o = Some(Some(1))\nmatch o {\nSome(Some(x)) => {}\n_ => {}\n}"),
    "pattern_number_in_pattern": M("let o = Some(1)\nmatch o {\nSome(1) => {}\n_ => {}\n}"),
    "pattern_variant_in_pattern": M("let o = Some(Some(1))\nmatch o {\nSome(None) => {}\n_ => {}\n}"),
    "pattern_constant_in_pattern": "const K = 1\n" + M("let o = Some(1)\nmatch o {\nSome(K) => {}\n_ => {}\n}"),
    # ---- generics ----
    "empty_list": M("let xs = []"),
    "empty_list_argument": "fn f[T](xs: List[T]) {\n}\n" + M("f([])"),
    "which_enum": M("let o = None"),
    "which_result": M("let r = Ok(1)"),
    "which_struct": "struct Ph[T] {\n    n: i64\n}\n" + M("let p = Ph { n: 1 }"),
    "which_type": "fn make[T]() -> List[T] {\n    let xs: List[T] = []\n    return xs\n}\n" + M("let xs = make()"),
    "which_type_wrong_hint": "fn make[T]() -> List[T] {\n    let xs: List[T] = []\n    return xs\n}\n" + M("let xs: i64 = make()"),
    "type_parameter_arguments": "struct W[T] {\n    a: T[i64]\n}\n" + M(""),
    "scalar_arguments": M("let a: i64[i64] = 1"),
    "list_no_arguments": M("let a: List = []"),
    "list_two_arguments": M("let a: List[i64, i64] = []"),
    "option_no_arguments": M("let a: Option = None"),
    "result_one_argument": M("let a: Result[i64] = Ok(1)"),
    "struct_arguments": P + M("let a: P[i64] = P { x: 1, y: 2 }"),
    "generic_struct_no_arguments": "struct W[T] {\n    a: T\n}\nfn f(w: W) {\n}\n" + M(""),
    "enum_arguments": "enum A {\n    X\n}\nfn f(a: A[i64]) {\n}\n" + M(""),
    "types_grow": "fn f[T](x: T) {\n    f([x])\n}\n" + M("f(1)"),
    "structs_nest": "struct S[T] {\n    a: List[S[List[T]]]\n}\nfn g(s: S[i64]) {\n}\n" + M(""),
    "enums_nest": "enum E[T] {\n    A(List[E[List[T]]])\n}\nfn g(e: E[i64]) {\n}\n" + M(""),
    "mistake_in_generic": "fn f[T](x: T) -> T {\n    return x + 1\n}\n" + M("print(f(1))\nprint(f(\"a\"))\nprint(f(true))"),
    "mistake_in_generic_long_name": "fn f[T](x: T) {\n    print(x)\n}\n" + M("let x: List[List[List[List[List[List[List[List[List[List[i64]]]]]]]]]] = []\nf(x)"),
    "mistake_in_generic_two": "fn f[A, B](x: A, y: B) {\n    print(x == y)\n}\n" + M("f(1, 2)\nf(1, true)"),
    "mistake_in_library": P + M("let ps = [P { x: 1, y: 2 }]\nprint(ps.contains(P { x: 1, y: 2 }))"),
    "mistake_in_library_from_generic": P + "fn has[T](xs: List[T], x: T) -> bool {\n    return xs.contains(x)\n}\n" + M("let ps = [P { x: 1, y: 2 }]\nprint(has(ps, P { x: 1, y: 2 }))"),
    # ---- constants ----
    "const_not_constant": "const A = B\n" + M(""),
    "const_call": "fn f() -> i64 {\n    return 1\n}\nconst A = f()\n" + M(""),
    "const_list": "const A = [1]\n" + M(""),
    "const_string_minus": "const A = \"a\" - \"b\"\n" + M(""),
    "const_string_plus_number": "const A = \"a\" + 1\n" + M(""),
    "const_number_plus_string": "const A = 1 + \"a\"\n" + M(""),
    "const_string_compared": "const A = \"a\" == \"a\"\n" + M(""),
    "const_circle": "const A = B\nconst B = A + 1\n" + M(""),
    "const_itself": "const A = A\n" + M(""),
    "const_changed": "const K = 1\n" + M("K = 2"),
    "const_passed": "const K = 1\nfn f(a: mut i64) {\n}\n" + M("f(mut K)"),
    "const_division": "const A = 1 / 0\n" + M(""),
    "const_remainder": "const A = 1 % (2 - 2)\n" + M(""),
    # ---- limits ----
    "too_many_variants": "enum A {\n" + "".join(f"    V{i}\n" for i in range(1001)) + "}\n" + M(""),
    "too_many_parameters": "fn f(" + ", ".join(f"a{i}: i64" for i in range(9)) + ") {\n}\n" + M(""),
    "too_many_slots": M("\n".join(f"let a{i} = {i}" for i in range(MANY))),
    "too_many_slots_strings": M("\n".join(f"print(\"{i}\")" for i in range(700))),
    "too_many_slots_in_match": M("\n".join(f"match {i} {{\n_ => {{}}\n}}" for i in range(MANY))),
    "struct_too_large": "struct A {\n" + "".join(f"    f{i}: i64\n" for i in range(501)) + "}\n" + M(""),
    "enum_too_large": "struct A {\n" + "".join(f"    f{i}: i64\n" for i in range(300)) + "}\nenum E {\n    X(A, A)\n}\n" + M(""),
    "float": M("let x = 1.5"),
    # ---- how numbers are written ----
    "number_written_0": M("let a = 1__0"),
    "number_written_1": M("let a = 1_"),
    "number_written_2": M("let a = 0x_F"),
    "number_written_3": M("let a = 0xF_"),
    "number_written_4": M("let a = 0xF__F"),
    "number_written_5": M("let a = 0b_1"),
    "number_written_6": M("let a = 0b1_"),
    "number_written_7": M("let a = 007"),
    "number_written_8": M("let a = 00"),
    "number_written_9": M("let a = 0_1"),
    "number_written_10": M("let a = 9223372036854775808"),
    "number_written_11": M("let a = 1 - 9223372036854775808"),
    "number_written_12": M("let a = -9223372036854775809"),
    "number_written_13": M("let a = 0x1_0000_0000_0000_0000"),
    "number_written_14": M("let a = 0x_1_0000_0000_0000_0000"),
    "number_written_pattern": M("match 7 {\n007 => {}\n_ => {}\n}"),
    "number_written_pattern_hex": M("match 7 {\n0x_7 => {}\n_ => {}\n}"),
    "number_written_pattern_underscore": M("match 7 {\n-7_ => {}\n_ => {}\n}"),
    "number_written_const": "const A = 1__0\n" + M(""),
    "number_written_const_min": "const A = -9223372036854775809\n" + M(""),
    # ---- `?` ----
    "try_number": TRY + "fn f() -> Result[i64, str] {\n    let n = 1?\n    return Ok(n)\n}\n" + M(""),
    "try_returns_number": TRY + "fn f() -> i64 {\n    return number(\"1\")?\n}\n" + M(""),
    "try_returns_nothing": TRY + "fn f() {\n    number(\"1\")?\n}\n" + M(""),
    "try_in_main": TRY + M("let n = maybe()?"),
    "try_other_error": TRY + "fn f() -> Result[i64, i64] {\n    return Ok(number(\"1\")?)\n}\n" + M(""),
    "try_option_in_result": TRY + "fn f() -> Result[i64, str] {\n    return Ok(maybe()?)\n}\n" + M(""),
    "try_result_in_option": TRY + "fn f() -> Option[i64] {\n    return Some(number(\"1\")?)\n}\n" + M(""),
    "try_in_list": TRY + "fn f() -> Result[List[i64], str] {\n    return Ok([number(\"1\")?])\n}\n" + M(""),
    "try_in_second_list": TRY + "fn f() -> Result[i64, str] {\n    let n = [1, 2].len() + [number(\"1\")?].len()\n    return Ok(n)\n}\n" + M(""),
    "try_after_owner": TRY + "fn f() -> Result[PQ, str] {\n    return Ok(PQ { a: words(\"a\")?, n: number(\"1\")? })\n}\n" + M(""),
    "try_after_owner_list": TRY + "fn f(s: str) -> Result[PQ, str] {\n    return Ok(PQ { a: [s], n: number(s)? })\n}\n" + M(""),
    "try_after_owner_variant": TRY + "enum Two {\n    Both(str, i64)\n}\nfn f(s: str) -> Result[Two, str] {\n    return Ok(Both(s + \"!\", number(s)?))\n}\n" + M(""),
    "try_generic": "fn first[T](r: Result[T, str]) -> T {\n    return r?\n}\n" + M("let r: Result[i64, str] = Ok(1)\nprint(first(r))"),
    "try_mut": TRY + "fn f() -> Result[i64, str] {\n    let n = mut number(\"1\")?\n    return Ok(n)\n}\n" + M(""),
    # ---- the library ----
    "library_type_struct": "struct Map {\n    n: i64\n}\n" + M(""),
    "library_type_enum": "enum Map {\n    A\n}\n" + M(""),
    "library_type_generic": "struct Map[K] {\n    n: K\n}\n" + M(""),
    "library_fn_untyped": M("let m = new_map()"),
    "library_method_as_function": M("var m: Map[i64] = new_map()\nmap_set(mut m, \"a\", 1)"),
    "receiver_let": M("let xs = [1]\nxs.extend([2])"),
    "receiver_temporary": M("[1].extend([2])"),
    "receiver_twice": M("var xs = [1]\nxs.extend(xs)"),
    "receiver_in_loop": M("var xs = [1]\nfor x in xs {\nxs.extend([x])\n}"),
    "receiver_in_match": M("var o = Some([1])\nmatch o {\nSome(xs) => xs.truncate(0)\nNone => {}\n}"),
    "receiver_matched": "struct W {\n    o: Option[i64]\n    xs: List[i64]\n}\n" + M("var w = W { o: Some(1), xs: [1] }\nmatch w {\n_ => w.xs.truncate(0)\n}"),
    "receiver_read_before": M("var ss = [\"a\"]\nlet x = ss[0] + ss.remove_at(0)"),
    "receiver_operator": "fn two(a: List[i64], b: i64) -> i64 {\n    return b\n}\n" + M("var xs = [1]\nprint(two(xs, xs.remove_at(0)))"),
    "receiver_mut_argument": M("var xs = [1]\nxs.extend(mut [2])"),
    "receiver_parameter": "fn f(xs: List[i64]) {\n    xs.truncate(0)\n}\n" + M(""),
    "receiver_constant": "const K = \"a\"\n" + M("var m: Map[i64] = new_map()\nK.extend([1])"),
    "receiver_wrong_type": M("let n = 5\nn.extend([1])"),
    "receiver_wrong_method": M("let a = true.is_empty()\nlet b = 5.replace(\"a\", \"b\")"),
    "receiver_wrong_count": M("var xs = [1]\nxs.extend()"),
    "receiver_wrong_argument": M("var xs = [1]\nxs.extend(\"a\")"),
    "exit_is_not_a_return": "fn f(n: i64) -> i64 {\n    if n > 0 {\n        return n\n    }\n    eprint(\"no\")\n}\n" + M(""),
    # ---- several at once ----
    "three_functions": "fn a() {\n    print(x)\n}\nfn b() -> i64 {\n}\n" + M("let y = 1\ny = 2"),
    "same_place_twice": "fn f[T](x: T) {\n    print(y)\n}\n" + M("f(1)\nf(true)\nprint(z)"),
}

# Right programs for corners that tests/programs does not reach.
CORNERS = {
    "numbers": "const BIG = 0xFFFF_FFFF_FFFF_FFFF\nconst SMALL = -9223372036854775807 - 1\nconst WRAP = SMALL / -1\nconst REST = SMALL % -1\nconst SH = 1 << 65\nconst SR = SMALL >> 70\n"
               + M("print(BIG)\nprint(SMALL)\nprint(WRAP + REST + SH + SR)\nmatch SMALL {\n-9223372036854775808 => print(1)\n9223372036854775807 => print(2)\n0x7FFF_FFFF_FFFF_FFFF => print(3)\n-0x7FFF_FFFF_FFFF_FFFF => print(4)\n'a' => print(5)\n'\\n' => print(6)\n0b101 => print(7)\n_ => print(8)\n}\nprint(0x8000_0000_0000_0000)\nprint('\\\\' + '\\'' + '\\0' + '\\t' + '\\r')"),
    "constants": "const A = !true || false && true\nconst B = 3 * 4 - 5 / 2 % 7\nconst C = (1 | 2) ^ 7 & 5\nconst D = 1 < 2 == true\nconst E = \"x\\ty\\\\\\\"z\\0\"\nconst F: bool = 1 >= 2 != (3 <= 4)\nconst G = B\n"
                 + M("print(A)\nprint(B + C + G)\nprint(D)\nprint(E)\nprint(F)\nmatch E {\nE => print(1)\n_ => print(2)\n}\nmatch true {\nA => print(3)\nF => print(4)\n}\nvar s = E\ns = s + E\nprint(s)"),
    "registers": M("\n".join(f"var v{i} = {i}" for i in range(14)) + "\nvar n = 0\nwhile n < 3 {\n" + "\n".join(f"v{i} = v{i} + v{(i * 5 + 3) % 14} * {i % 4}" for i in range(14)) + "\nn = n + 1\n}\nprint(v0 + v13)"),
    "generic_chain": "struct Box[T] {\n    v: T\n}\nenum Tree[T] {\n    Leaf(T)\n    Node(List[Tree[T]])\n}\nfn wrap[T](x: T) -> Box[T] {\n    return Box { v: x }\n}\nfn twice[T](x: T) -> Box[Box[T]] {\n    return wrap(wrap(x))\n}\nfn count[T](t: Tree[T]) -> i64 {\n    match t {\n        Leaf(_) => return 1\n        Node(kids) => {\n            var n = 0\n            for k in kids {\n                n = n + count(k)\n            }\n            return n\n        }\n    }\n}\n"
                     + M("let a = twice(1)\nlet b = twice(\"s\")\nlet c = twice([Some(1)])\nprint(a.v.v)\nprint(b.v.v)\nprint(c.v.v.len())\nlet t = Node([Leaf(1), Node([Leaf(2), Leaf(3)])])\nprint(count(t))\nlet u: Tree[str] = Node([])\nprint(count(u))\nlet r: Result[Box[i64], str] = Ok(wrap(2))\nmatch r {\nOk(bx) => print(bx.v)\nErr(e) => print(e)\n}"),
    "moves": "struct S {\n    name: str\n    xs: List[str]\n}\nfn make(n: str) -> S {\n    var xs: List[str] = []\n    xs.push(n)\n    let s = S { name: n, xs: xs }\n    return s\n}\n"
             + M("var a = make(\"a\")\nlet b = a\nvar c = b\nc.name = c.name + \"!\"\nc.xs[0] = c.name\na = c\nvar list = [a, make(\"z\")]\nlist[1] = list[0]\nlet last = list.pop()\nprint(last.name)\nfor s in list {\nprint(s.xs[0])\n}\nvar t = \"\"\nvar i = 0\nwhile i < 3 {\nt = t + last.name\ni = i + 1\n}\nprint(t)"),
}

CORNERS.update({
    "tries_everywhere": 'struct Bad {\n    why: str\n    codes: List[i64]\n}\nstruct Item {\n    name: str\n    n: i64\n}\nfn num(n: i64) -> Result[i64, Bad] {\n    if n < 0 {\n        return Err(Bad { why: "negative " + str(n), codes: [n] })\n    }\n    return Ok(n)\n}\nfn item(n: i64) -> Result[Item, Bad] {\n    return Ok(Item { name: "i" + str(num(n)?), n: n })\n}\nfn items(n: i64) -> Result[List[Item], Bad] {\n    var out: List[Item] = []\n    var i = 0\n    while i < n {\n        out.push(item(i - 1 + num(1)?)?)\n        i = i + 1\n    }\n    return Ok(out)\n}\nfn opt(n: i64) -> Option[str] {\n    if n == 0 {\n        return None\n    }\n    return Some("s" + str(n))\n}\nfn uses(n: i64, kept: Result[Item, Bad]) -> Result[i64, Bad] {\n    var total = 0\n    let label = "t" + str(n)\n    if num(n)? > 3 && num(n - 1)? < 100 {\n        total = total + 1\n    }\n    let list = [1, 2, 3]\n    total = total + list[num(n)? % 3]\n    for it in items(n)? {\n        total = total + it.n + it.name.len()\n    }\n    match num(n)? {\n        0 => total = total + 100\n        _ => total = total + num(num(n)?)?\n    }\n    let a = kept?\n    let b = kept?.name\n    var j = 0\n    while j < 3 {\n        let c = item(j)?\n        total = total + c.n + a.n + b.len() + label.len()\n        j = j + 1\n    }\n    return Ok(total + num(0 - n)?)\n}\nfn words(n: i64) -> Option[i64] {\n    let w = opt(n)?\n    var copy = Some(w)\n    let again = copy?\n    return Some(again.len() + opt(n - 1)?.len())\n}\nfn main() {\n    for n in [0, 4, -2] {\n        match uses(n, item(n)) {\n            Ok(v) => print(v)\n            Err(bad) => print(bad.why + " " + str(bad.codes.len()))\n        }\n        match words(n + 2) {\n            Some(v) => print(v)\n            None => print("none")\n        }\n    }\n}\n',
    "smallest": "const LOW = -9223372036854775808\nconst NEXT = LOW + 1\nconst JOINED = \"a\" + \"b\"\nconst MORE = JOINED + JOINED + \"!\"\n"
                + M("let a = -9223372036854775808\nlet b = -9_223_372_036_854_775_808\nlet c = - 9223372036854775808\nprint(a + b + c + LOW + NEXT)\nprint(MORE)\nprint(0 + 10 + 1_000 + 0x0F + 0xff_ff + 0b0_1 + 'a' + '_')\nmatch a {\n-9_223_372_036_854_775_808 => print(1)\n_ => print(2)\n}"),
    "tries": TRY + "struct QP {\n    n: i64\n    a: List[str]\n}\nfn f() -> Result[i64, str] {\n    let n = number(\"1\")? + number(\"2\")?\n    number(\"3\")?\n    return Ok(n)\n}\nfn g() -> Result[str, str] {\n    let w = words(\"a\")?\n    return Ok(w[0])\n}\nfn h() -> Option[str] {\n    let n = maybe()?\n    return Some(str(n))\n}\nfn k(r: Result[List[i64], str]) -> Result[i64, str] {\n    return Ok(r?.len())\n}\nfn q() -> Result[QP, str] {\n    return Ok(QP { n: number(\"1\")?, a: words(\"a\")? })\n}\nfn pq() -> Result[PQ, str] {\n    return Ok(PQ { a: words(\"a\")?, n: 1 })\n}\nfn l(s: str) -> Result[i64, str] {\n    let n = [1, 2].len() + number(s)?\n    let v = QP { n: 1, a: [s] }\n    let w = QP { n: number(s)?, a: v.a }\n    return Ok(n + w.n)\n}\nfn o(x: Option[List[str]]) -> Option[i64] {\n    var n = x?.len()\n    while n < 3 {\n        n = n + maybe()?\n    }\n    return Some(n)\n}\n"
             + M("match f() {\nOk(n) => print(n)\nErr(e) => print(e)\n}\nmatch g() {\nOk(n) => print(n)\nErr(e) => print(e)\n}\nmatch h() {\nSome(n) => print(n)\nNone => print(0)\n}\nmatch k(Ok([1])) {\nOk(n) => print(n)\nErr(e) => print(e)\n}\nmatch q() {\nOk(v) => print(v.n)\nErr(e) => print(e)\n}\nmatch pq() {\nOk(v) => print(v.n)\nErr(e) => print(e)\n}\nmatch l(\"s\") {\nOk(n) => print(n)\nErr(e) => print(e)\n}\nmatch o(Some([\"a\"])) {\nSome(n) => print(n)\nNone => print(0)\n}"),
    "library": "fn new_map() -> i64 {\n    return 7\n}\nfn f(xs: mut List[i64]) {\n    xs.truncate(0)\n}\nstruct H {\n    xs: List[i64]\n}\nfn leave(n: i64) -> i64 {\n    if n > 0 {\n        return n\n    }\n    exit(2)\n}\n"
               + M("let n: i64 = new_map()\nvar xs = [1]\nxs.extend([2, 3])\nxs.insert(0, 9)\nxs.truncate(2)\nlet r = xs.remove_at(0)\nf(mut xs)\nvar h = H { xs: [1] }\nh.xs.extend(xs)\nvar hs = [h]\nhs[0].xs.insert(0, r)\nprint(n + r + hs[0].xs.len() + leave(1))\nlet a = 3.min(4)\nlet b = [3, 4].min()\nlet c = \"\".is_empty()\nlet d = [1].is_empty()\nprint(a)\nprint(c == d)\nlet e = xs.len() + xs.remove_at(0)\nlet ys = [xs.first(), Some(xs.remove_at(0))]\nprint(e + ys.len())"),
    "library_map": "fn new_map[T](x: T) -> T {\n    return x\n}\n" + M("let n: i64 = new_map(1)\nprint(n)"),
    "map": M("var m: Map[i64] = new_map()\nm.set(\"a\", 1)\nm.set(\"b\", 2)\nlet gone = m.remove(\"a\")\nprint(m.len())\nprint(m.has(\"b\"))\nmatch m.get(\"b\") {\nSome(v) => print(v)\nNone => print(0)\n}\nfor k in m.keys() {\nprint(k)\n}\nvar w: Map[List[str]] = new_map()\nw.set(\"k\", [\"v\"])\nprint(w.len())"),
})

# How right programs are damaged: a text is replaced where it first
# occurs, where it occurs a second time, and a line is taken out.
SWAPS = [("var ", "let "), ("mut ", ""), (": i64", ": str"), ("true", "1"), ("return ", "return !"), (" + ", " == "),
         ("[", "[["), (".len()", ".size"), ("Some(", "Ok("), ("None", "Nothing"), ("print(", "eprint("), ("i64", "bool"),
         (" -> ", " -> List[i64] -> "), ("\"", "\"x"), ("_ =>", "0 =>"), ("for ", "for i, ")]


def run(command):
    return subprocess.run(command, capture_output=True)


def first_difference(a, b):
    for at, (x, y) in enumerate(zip(a, b)):
        if x != y:
            return at
    return min(len(a), len(b))


def compare(sema, paths, shown, messages):
    """Returns None when both tools agree, else a description."""
    want = run([AZOL, "sema-dump"] + paths)
    got = run([sema, PRELUDE] + paths)
    for line in want.stdout.split(b"\n"):
        if line.startswith(b"error "):
            messages.add(line.split(b" ", 3)[3].decode(errors="replace"))
    if want.returncode != got.returncode:
        return (f"{shown}: exit code {got.returncode}, expected {want.returncode}\n  azol:      {want.stdout[:300]!r}\n"
                f"  azol-sema: {got.stdout[:300]!r} {got.stderr[:300]!r}")
    if want.stdout != got.stdout:
        a, b = want.stdout, got.stdout
        at = first_difference(a, b)
        line = a.count(b"\n", 0, at) + 1
        return (f"{shown}: output differs at byte {at}, line {line} (azol {len(a)} bytes, azol-sema {len(b)} bytes)\n"
                f"  azol:      {a[max(0, at - 80):at + 80]!r}\n  azol-sema: {b[max(0, at - 80):at + 80]!r}")
    return None


def templates():
    """Every message the checker can give, as `azol explain` lists them."""
    found = []
    for line in run([AZOL, "explain"]).stdout.decode().splitlines():
        code = line.split()[0]
        if not re.fullmatch(r"E0[2-8]\d\d", code):
            continue
        listing = False
        for row in run([AZOL, "explain", code]).stdout.decode().splitlines():
            if listing and row.strip():
                found.append((code, row.strip()))
            if row.startswith("Messages with this code"):
                listing = True
    return found


# Messages of the lexer and the parser, which `check_front.py` covers.
NOT_OF_THE_CHECKER = ["source file is larger than 4 GiB", "code is nested too deeply"]


def uncovered(messages):
    missing = []
    for code, template in templates():
        if template in NOT_OF_THE_CHECKER:
            continue
        pattern = re.compile(".*".join(re.escape(part) for part in template.split("...")) + r"( \(in `.*)?", re.S)
        if not any(pattern.fullmatch(message) for message in messages):
            missing.append(f"{code} {template}")
    return missing


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


def main():
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        sys.exit("check_sema.py runs azol-sema directly and so needs x86-64 Linux")
    if not os.path.exists(AZOL):
        sys.exit("build the compiler first: cargo build --release")
    work = tempfile.mkdtemp(prefix="azol-check-sema-")
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
    sema = os.path.join(work, "azol-sema")
    built = run([AZOL, "build", "--target", "x86_64-linux", "-o", sema] + [os.path.join(HERE, name) for name in SOURCES])
    if built.returncode != 0:
        sys.exit("azol-sema does not build:\n" + built.stderr.decode(errors="replace"))

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
    groups.append(("wrong ", [([write(folder, name, text)], name) for name, text in BROKEN.items()]))
    groups.append(("corner ", [([write(folder, "corner_" + name, text)], name) for name, text in CORNERS.items()]))

    # Right programs, damaged.
    damaged = []
    seeds = sorted(glob.glob(os.path.join(ROOT, "tests", "programs", "*.azl"))) + sorted(glob.glob(os.path.join(ROOT, "examples", "*.azl")))
    for seed_at, seed in enumerate(seeds):
        text = open(seed, encoding="utf-8").read()
        stem = os.path.basename(seed)[:-4]
        changed = []
        for swap_at, (old, new) in enumerate(SWAPS):
            first = text.find(old)
            if first < 0 or (swap_at + seed_at) % 3 == 2:
                continue
            changed.append((f"swap{swap_at}", text[:first] + new + text[first + len(old):]))
            second = text.find(old, first + len(old))
            if second >= 0 and (swap_at + seed_at) % 2 == 0:
                changed.append((f"swap{swap_at}b", text[:second] + new + text[second + len(old):]))
        lines = text.split("\n")
        step = max(1, len(lines) // 12)
        for at in range(seed_at % step, len(lines), step):
            if lines[at].strip() and not lines[at].lstrip().startswith("//"):
                changed.append((f"line{at}", "\n".join(lines[:at] + lines[at + 1:])))
        for kind, new_text in changed:
            damaged.append(([write(folder, f"damaged_{stem}_{kind}", new_text)], f"{stem} {kind}"))
    groups.append(("damaged ", damaged))

    count = 0
    messages = set()
    for group, entries in groups:
        for paths, shown in entries:
            problem = compare(sema, paths, group + shown, messages)
            if problem:
                print("DIFFERENT " + problem)
                return 1
            print(f"same  {group}{shown}")
            count += 1
            if group == "wrong " and run([AZOL, "sema-dump"] + paths).returncode == 0:
                print(f"NOT WRONG: the program `{shown}` was meant to have a mistake")
                return 1

    if not quick:
        if not os.path.exists(BIG):
            sys.exit("bench/generated/big.azl is missing: python3 bench/gen.py")
        problem = compare(sema, [BIG], "bench/generated/big.azl", messages)
        if problem:
            print("DIFFERENT " + problem)
            return 1
        print("same  bench/generated/big.azl", flush=True)
        count += 1

    missing = uncovered(messages)
    if missing:
        print("NOT REACHED: no program here makes the checker say")
        for line in missing:
            print("  " + line)
        return 1

    if not quick:
        front = os.path.join(work, "azol-front")
        run([AZOL, "build", "--target", "x86_64-linux", "-o", front] + [os.path.join(HERE, name) for name in ["front_main.azl"] + FRONT + ["dump.azl"]])
        print()
        print("big.azl, best of 7, whole process (start, read the files, work), nothing printed:")
        whole = best_time([sema, "--time", PRELUDE, BIG])
        parse = best_time([front, "parse", "--time", BIG])
        rust_check = best_time([AZOL, "check", BIG])
        rust_parse = best_time([AZOL, "parse", "--time", BIG])
        print(f"  azol-sema (lex, parse, check)    {whole:7.1f} ms")
        print(f"  azol-front parse (lex, parse)    {parse:7.1f} ms")
        print(f"  so the check written in azoL    ~{whole - parse:7.1f} ms   (it also lexes and parses the standard library)")
        print(f"  azol check (lex, parse, check)   {rust_check:7.1f} ms")
        print(f"  azol parse (lex, parse)          {rust_parse:7.1f} ms")
        print(f"  so the check written in Rust    ~{rust_check - rust_parse:7.1f} ms")
        inner = run([AZOL, "build", BIG, "-o", os.path.join(work, "big")]).stderr.decode().strip()
        print(f"  azol's own clock: {inner}")

    print()
    print(f"{count} comparisons, {len(messages)} different messages of the checker reached, every one `azol explain` lists among them")
    print("ALL IDENTICAL")
    return 0


if __name__ == "__main__":
    sys.exit(main())
