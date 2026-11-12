#!/usr/bin/env python3
"""Writes the same program in azoL, in Rust and in JavaScript, at any size.

    python3 bench/gen.py 3000

creates bench/generated/big.azl, big.rs and big.js, each made of that
many units. A unit is a struct that owns a list plus four functions that
build it, read it, change it through `mut` and copy it. The programs
print the same number, which shows they compute the same thing.
"""
import os
import sys

AZOL_UNIT = """struct Rec{i} {{
    id: i64
    weight: i64
    values: List[i64]
}}

fn make{i}(seed: i64) -> Rec{i} {{
    var values: List[i64] = []
    var k = 0
    while k < 4 {{
        values.push(seed * (k + 1) + {i})
        k = k + 1
    }}
    return Rec{i} {{ id: {i}, weight: seed % 7 + 1, values: values }}
}}

fn score{i}(r: Rec{i}) -> i64 {{
    var total = r.id * r.weight
    for v in r.values {{
        if v % 2 == 0 {{
            total = total + v
        }} else {{
            total = total - v / 3
        }}
    }}
    return total
}}

fn bump{i}(r: mut Rec{i}, by: i64) {{
    r.weight = r.weight + by
    r.values.push(by)
    r.values[0] = r.values[0] + r.id
}}

fn step{i}(seed: i64) -> i64 {{
    var r = make{i}(seed)
    bump{i}(mut r, seed % 5)
    let before = score{i}(r)
    var copy = r
    bump{i}(mut copy, 3)
    return before + score{i}(copy) - score{i}(r)
}}

"""

RUST_UNIT = """#[derive(Clone)]
struct Rec{i} {{
    id: i64,
    weight: i64,
    values: Vec<i64>,
}}

fn make{i}(seed: i64) -> Rec{i} {{
    let mut values: Vec<i64> = Vec::new();
    let mut k = 0;
    while k < 4 {{
        values.push(seed * (k + 1) + {i});
        k = k + 1;
    }}
    return Rec{i} {{ id: {i}, weight: seed % 7 + 1, values: values }};
}}

fn score{i}(r: &Rec{i}) -> i64 {{
    let mut total = r.id * r.weight;
    for &v in &r.values {{
        if v % 2 == 0 {{
            total = total + v;
        }} else {{
            total = total - v / 3;
        }}
    }}
    return total;
}}

fn bump{i}(r: &mut Rec{i}, by: i64) {{
    r.weight = r.weight + by;
    r.values.push(by);
    r.values[0] = r.values[0] + r.id;
}}

fn step{i}(seed: i64) -> i64 {{
    let mut r = make{i}(seed);
    bump{i}(&mut r, seed % 5);
    let before = score{i}(&r);
    let mut copy = r.clone();
    bump{i}(&mut copy, 3);
    return before + score{i}(&copy) - score{i}(&r);
}}

"""
JS_UNIT = """function make{i}(seed) {{
    const values = [];
    let k = 0;
    while (k < 4) {{
        values.push(seed * (k + 1) + {i});
        k = k + 1;
    }}
    return {{ id: {i}, weight: seed % 7 + 1, values: values }};
}}

function score{i}(r) {{
    let total = r.id * r.weight;
    for (const v of r.values) {{
        if (v % 2 === 0) {{
            total = total + v;
        }} else {{
            total = total - Math.trunc(v / 3);
        }}
    }}
    return total;
}}

function bump{i}(r, by) {{
    r.weight = r.weight + by;
    r.values.push(by);
    r.values[0] = r.values[0] + r.id;
}}

function step{i}(seed) {{
    const r = make{i}(seed);
    bump{i}(r, seed % 5);
    const before = score{i}(r);
    const copy = {{ id: r.id, weight: r.weight, values: r.values.slice() }};
    bump{i}(copy, 3);
    return before + score{i}(copy) - score{i}(r);
}}

"""


def main():
    units = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "generated")
    os.makedirs(out, exist_ok=True)

    azol = [AZOL_UNIT.format(i=i) for i in range(units)]
    azol.append("fn main() {\n    var total = 0\n")
    azol.extend("    total = total + step%d(%d)\n" % (i, i % 97) for i in range(units))
    azol.append("    print(total)\n}\n")

    rust = ["#![allow(dead_code)]\n\n"]
    rust.extend(RUST_UNIT.format(i=i) for i in range(units))
    rust.append("fn main() {\n    let mut total: i64 = 0;\n")
    rust.extend("    total = total + step%d(%d);\n" % (i, i % 97) for i in range(units))
    rust.append('    println!("{}", total);\n}\n')

    js = [JS_UNIT.format(i=i) for i in range(units)]
    js.append("let total = 0;\n")
    js.extend("total = total + step%d(%d);\n" % (i, i % 97) for i in range(units))
    js.append("console.log(total);\n")

    for name, parts in (("big.azl", azol), ("big.rs", rust), ("big.js", js)):
        text = "".join(parts)
        with open(os.path.join(out, name), "w") as f:
            f.write(text)
        print("%s: %d lines" % (name, text.count("\n")))


if __name__ == "__main__":
    main()
