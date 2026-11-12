#!/usr/bin/env python3
"""Writes compiler/diag_table.azl and compiler/embedded.azl.

The compiler written in azoL needs four things that the Rust compiler
carries inside itself. They are generated from where the Rust compiler
keeps them, so that they are never typed twice:

    compiler/diag_table.azl   the table of error codes (src/diag.rs), the
                              usage text (src/main.rs) and the version
                              (Cargo.toml)
    compiler/embedded.azl     the standard library's source
                              (src/prelude.azl) and the machine code of
                              the Linux x86-64 runtime
                              (runtime/linux_x64/runtime.bin)

    python3 compiler/gen_tables_azl.py           write the files
    python3 compiler/gen_tables_azl.py --check   fail if a file is stale
"""
import os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
COPYRIGHT = "// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi"
# Entries of the table that one function adds: a function's stack is limited.
PER_FUNCTION = 10


def read(*parts):
    with open(os.path.join(ROOT, *parts), encoding="utf-8", newline="") as source:
        return source.read()


def rust_strings(text):
    """The string literals of a piece of Rust source, unescaped, in order."""
    found = []
    at = 0
    while True:
        at = text.find('"', at)
        if at < 0:
            return found
        at += 1
        out = []
        while text[at] != '"':
            if text[at] == "\\":
                at += 1
                escaped = text[at]
                if escaped == "n":
                    out.append("\n")
                elif escaped in '"\\':
                    out.append(escaped)
                elif escaped == "\n":
                    # A backslash at the end of a line joins the next line, without its indentation.
                    while text[at + 1] in " \t":
                        at += 1
                else:
                    sys.exit("gen_tables_azl.py: an escape this script does not know: \\" + escaped)
            else:
                out.append(text[at])
            at += 1
        at += 1
        found.append("".join(out))


def azol_string(text):
    """`text` as an azoL string literal."""
    out = ['"']
    for c in text:
        if c == "\n":
            out.append("\\n")
        elif c == "\t":
            out.append("\\t")
        elif c == "\r":
            out.append("\\r")
        elif c in '"\\':
            out.append("\\" + c)
        elif ord(c) < 0x20:
            sys.exit("gen_tables_azl.py: a control character that an azoL string cannot hold")
        else:
            out.append(c)
    out.append('"')
    return "".join(out)


def diag_table():
    source = read("src", "diag.rs")
    start = source.index("pub static TABLE: &[Entry] = &[")
    table = source[start:source.index("\n];\n", start)]
    # Comments in the table would confuse the string scanner only if they held a quote.
    table = "\n".join(line for line in table.split("\n") if not line.strip().startswith("//"))
    entries = []
    for block in table.split("Entry {")[1:]:
        strings = rust_strings(block)
        if len(strings) < 5 or not re.fullmatch(r"E\d{4}", strings[0]):
            sys.exit("gen_tables_azl.py: an entry of the table in src/diag.rs is not in the form this script reads")
        entries.append((strings[0], strings[1:-3], strings[-3], strings[-2], strings[-1]))
    unclassified = rust_strings(source[source.index("const UNCLASSIFIED: Entry"):source.index("/// Every kind of error")])
    usage_at = source_main.index("const USAGE: &str = ")
    usage = rust_strings(source_main[usage_at:source_main.index('";', usage_at) + 1])[0]
    version = re.search(r'^version = "([^"]+)"', read("Cargo.toml"), re.M).group(1)
    md_at = source.index("pub fn errors_md()")
    md_head = "".join(rust_strings(source[source.index("String::from(", md_at):source.index(");", md_at)]))

    lines = [
        COPYRIGHT,
        "",
        "// The table of error codes, the usage text and the version, as the",
        "// compiler written in azoL needs them. Generated from `src/diag.rs`,",
        "// `src/main.rs` and `Cargo.toml` by `compiler/gen_tables_azl.py`; do",
        "// not edit it by hand.",
        "",
        "const VERSION = " + azol_string(version),
        "",
        "const USAGE = " + azol_string(usage),
        "",
        "// The first lines of `docs/errors.md`.",
        "const ERRORS_MD_HEAD = " + azol_string(md_head),
        "",
        "// One kind of error: its code, the wordings that belong to it with",
        "// `{}` for the parts that vary, the rule of the language behind it,",
        "// what to do about it when that is the same every time, and the",
        "// longer text `explain` prints.",
        "struct DiagEntry {",
        "    code: str",
        "    patterns: List[str]",
        "    rule: str",
        "    help: str",
        "    explain: str",
        "}",
        "",
        "// What a message that fits no pattern gets.",
        "fn diag_unclassified() -> DiagEntry {",
        "    return DiagEntry { code: %s, patterns: [], rule: %s, help: %s, explain: %s }" % (
            azol_string(unclassified[0]), azol_string(unclassified[1]), azol_string(unclassified[2]), azol_string(unclassified[3])),
        "}",
        "",
        "// Every kind of error, in the order they are tried: the first entry",
        "// with a pattern that fits a message wins.",
        "fn diag_table() -> List[DiagEntry] {",
        "    var t: List[DiagEntry] = []",
    ]
    groups = [entries[at:at + PER_FUNCTION] for at in range(0, len(entries), PER_FUNCTION)]
    for number in range(len(groups)):
        lines.append("    diag_table_%d(mut t)" % (number + 1))
    lines += ["    return t", "}"]
    for number, group in enumerate(groups):
        lines += ["", "fn diag_table_%d(t: mut List[DiagEntry]) {" % (number + 1)]
        for code, patterns, rule, help_text, explain in group:
            lines.append("    t.push(DiagEntry { code: %s, patterns: [%s], rule: %s, help: %s, explain: %s })" % (
                azol_string(code), ", ".join(azol_string(p) for p in patterns), azol_string(rule), azol_string(help_text),
                azol_string(explain)))
        lines.append("}")
    lines.append("")
    return "\n".join(lines)


def embedded():
    prelude = read("src", "prelude.azl")
    with open(os.path.join(ROOT, "runtime", "linux_x64", "runtime.bin"), "rb") as source:
        runtime = source.read()
    lines = [
        COPYRIGHT,
        "",
        "// Two files the compiler carries inside itself: the source of the",
        "// part of the standard library that is written in azoL",
        "// (`src/prelude.azl`), and the machine code of the Linux x86-64",
        "// runtime (`runtime/linux_x64/runtime.bin`) as hexadecimal digits.",
        "// Generated by `compiler/gen_tables_azl.py`; do not edit it by hand.",
        "",
        "const PRELUDE = " + azol_string(prelude),
        "",
        "// The bytes of the runtime. Text grows by whole strings only, so every",
        "// byte value is made into a string once and added from a table.",
        "fn embedded_runtime() -> str {",
        "    var singles: List[str] = []",
        "    var b = 0",
        "    while b < 256 {",
        "        singles.push(chr(b))",
        "        b = b + 1",
        "    }",
        "    let digits = RUNTIME_HEX",
        "    var out = \"\"",
        "    var at = 0",
        "    let end = digits.len()",
        "    while at < end {",
        "        var high = digits[at] - '0'",
        "        if high > 9 {",
        "            high = high - 39",
        "        }",
        "        var low = digits[at + 1] - '0'",
        "        if low > 9 {",
        "            low = low - 39",
        "        }",
        "        out = out + singles[high * 16 + low]",
        "        at = at + 2",
        "    }",
        "    return out",
        "}",
        "",
    ]
    # One literal: a constant joined from pieces with `+` would leave every
    # intermediate text in the executable.
    lines.append('const RUNTIME_HEX = "' + runtime.hex() + '"')
    lines.append("")
    return "\n".join(lines)


source_main = read("src", "main.rs")


def main():
    made = [("diag_table.azl", diag_table()), ("embedded.azl", embedded())]
    for name, text in made:
        path = os.path.join(HERE, name)
        if "--check" in sys.argv[1:]:
            if not os.path.exists(path) or open(path, encoding="utf-8", newline="").read() != text:
                sys.exit("compiler/%s is stale: python3 compiler/gen_tables_azl.py" % name)
        else:
            with open(path, "w", encoding="utf-8", newline="") as out:
                out.write(text)


if __name__ == "__main__":
    main()
