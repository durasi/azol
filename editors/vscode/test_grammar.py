#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Tests the VS Code files for azoL without VS Code.

1. The three JSON files parse and have the fields VS Code needs.
2. Every regular expression of the grammar compiles.
3. A small TextMate-style tokenizer runs the grammar over sample lines and
   checks which scope each piece of text gets.

The grammar only uses regex features that mean the same in Python's `re`
and in Oniguruma (which VS Code uses), so the result carries over.

    python3 editors/vscode/test_grammar.py
"""

import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))


def load(name):
    with open(os.path.join(HERE, name), encoding="utf-8") as handle:
        return json.load(handle)


class Grammar:
    def __init__(self, data):
        self.repository = data["repository"]
        self.rules = self.flatten(data["patterns"])

    def flatten(self, patterns):
        """Rules in priority order, with includes followed."""
        rules = []
        for pattern in patterns:
            if "include" in pattern:
                key = pattern["include"]
                assert key.startswith("#"), key
                rules += self.flatten(self.repository[key[1:]]["patterns"])
            elif "match" in pattern:
                rules.append(("match", re.compile(pattern["match"]), pattern))
            else:
                inner = self.flatten(pattern.get("patterns", []))
                rules.append(("begin", re.compile(pattern["begin"]), pattern, re.compile(pattern["end"]), inner))
        return rules

    def tokenize(self, line):
        """A list of (text, scope) for the scoped pieces of one line."""
        tokens = []
        self.scan(line, 0, self.rules, None, tokens)
        return tokens

    def earliest(self, line, pos, rules):
        best = None
        for rule in rules:
            found = rule[1].search(line, pos)
            if found and (best is None or found.start() < best[1].start()):
                best = (rule, found)
        return best

    def emit(self, tokens, found, pattern, key):
        captures = pattern.get(key, {})
        inner = [int(index) for index in captures if index != "0" and found.group(int(index)) is not None]
        if inner:
            for index in sorted(inner):
                tokens.append((found.group(index), captures[str(index)]["name"]))
        name = captures.get("0", {}).get("name") or (pattern.get("name") if key == "captures" else None)
        if name and (not inner or key != "captures" or "name" in pattern):
            tokens.append((found.group(0), name))

    def scan(self, line, pos, rules, end, tokens):
        """Scans until the line or the enclosing begin/end rule ends."""
        while pos <= len(line):
            best = self.earliest(line, pos, rules)
            closing = end.search(line, pos) if end else None
            if closing and (best is None or closing.start() <= best[1].start()):
                return closing
            if best is None:
                return None
            rule, found = best
            if rule[0] == "match":
                self.emit(tokens, found, rule[2], "captures")
                pos = found.end() if found.end() > found.start() else found.end() + 1
            else:
                pattern, end_regex, inner = rule[2], rule[3], rule[4]
                closing = self.scan(line, found.end(), inner, end_regex, tokens)
                stop = closing.end() if closing else len(line)
                tokens.append((line[found.start():stop], pattern["name"]))
                if closing is None:
                    return None
                pos = stop
        return None


# (line, [(text, the scope it must get)], [text that must get no scope starting with ...])
SAMPLES = [
    ("// a comment with \"quotes\" and fn", [("// a comment with \"quotes\" and fn", "comment.line")], []),
    ("let x = 42 // trailing", [("let", "keyword.declaration"), ("42", "constant.numeric.decimal"), ("// trailing", "comment.line"), ("=", "keyword.operator.assignment")], []),
    ("var big = 1_000_000", [("var", "keyword.declaration"), ("1_000_000", "constant.numeric.decimal")], []),
    ("let mask = 0x1F & 0xff_FF", [("0x1F", "constant.numeric.hex"), ("0xff_FF", "constant.numeric.hex"), ("&", "keyword.operator.bitwise")], []),
    ("let bits = 0b1010 | 0b1", [("0b1010", "constant.numeric.binary"), ("0b1", "constant.numeric.binary"), ("|", "keyword.operator.bitwise")], []),
    ("let half = 3.25", [("3.25", "invalid.illegal.float")], []),
    ("let name1 = x2 + y_3", [], [("1", "constant.numeric"), ("2", "constant.numeric"), ("3", "constant.numeric")]),
    ("let c = 'a' + '\\n' + '\\'' + '\"'", [("'a'", "constant.character"), ("'\\n'", "constant.character"), ("\\n", "constant.character.escape"), ("'\\''", "constant.character"), ("'\"'", "constant.character")], []),
    ("print(\"tab\\there \\\"q\\\" \\\\ end\")", [("print", "support.function.builtin"), ("\"tab\\there \\\"q\\\" \\\\ end\"", "string.quoted.double"), ("\\t", "constant.character.escape"), ("\\\"", "constant.character.escape"), ("\\\\", "constant.character.escape")], []),
    ("let s = \"bad \\q escape\"", [("\\q", "invalid.illegal.escape")], []),
    ("let s = \"not // a comment\"", [("\"not // a comment\"", "string.quoted.double")], [("// a comment\"", "comment")]),
    ("let s = \"a\" + \"b\"", [("\"a\"", "string.quoted.double"), ("\"b\"", "string.quoted.double"), ("+", "keyword.operator.arithmetic")], []),
    ("fn main() -> i64 {", [("fn", "keyword.declaration.function"), ("main", "entity.name.function"), ("->", "keyword.operator.arrow"), ("i64", "support.type.primitive"), ("{", "punctuation.brackets")], []),
    ("fn pop[T](stack: mut Stack[T]) -> Option[T] {", [("pop", "entity.name.function"), ("mut", "storage.modifier.mut"), ("Stack", "entity.name.type"), ("Option", "support.type"), ("T", "entity.name.type")], []),
    ("struct Pair[A, B] { first: A, second: B }", [("struct", "keyword.declaration.type"), ("Pair", "entity.name.type"), ("A", "entity.name.type"), (":", "punctuation.separator")], [("first", "entity"), ("second", "entity")]),
    ("enum Shape {", [("enum", "keyword.declaration.type"), ("Shape", "entity.name.type")], []),
    ("const MASK = (1 << 4) - 1", [("const", "keyword.declaration.constant"), ("MASK", "variable.other.constant"), ("<<", "keyword.operator.bitwise"), ("-", "keyword.operator.arithmetic")], []),
    ("if a <= b && !done || c != d {", [("if", "keyword.control"), ("<=", "keyword.operator.comparison"), ("&&", "keyword.operator.logical"), ("!", "keyword.operator.logical"), ("||", "keyword.operator.logical"), ("!=", "keyword.operator.comparison")], []),
    ("} else if x >> 2 > 1 {", [("else", "keyword.control"), ("if", "keyword.control"), (">>", "keyword.operator.bitwise"), (">", "keyword.operator.comparison")], []),
    ("let n = parse(a)? + 1", [("let", "keyword.declaration"), ("?", "keyword.operator.try"), ("+", "keyword.operator.arithmetic")], []),
    ("for i, item in items {", [("for", "keyword.control"), ("in", "keyword.control"), (",", "punctuation.separator")], [("item", "keyword"), ("items", "keyword")]),
    ("while true { break }", [("while", "keyword.control"), ("true", "constant.language.boolean"), ("break", "keyword.control")], []),
    ("match found {", [("match", "keyword.control")], [("found", "keyword")]),
    ("Some(value) => return value", [("Some", "support.function.variant"), ("=>", "keyword.operator.arrow"), ("return", "keyword.control")], []),
    ("None => continue", [("None", "constant.language.none"), ("continue", "keyword.control")], []),
    ("Err(why) => eprint(why)", [("Err", "support.function.variant"), ("eprint", "support.function.builtin")], []),
    ("let r: Result[i64, str] = Ok(1)", [("Result", "support.type"), ("i64", "support.type.primitive"), ("str", "support.type.primitive"), ("Ok", "support.function.variant")], []),
    ("let text = str(n) + chr(65)", [("str", "support.function.builtin"), ("chr", "support.function.builtin"), ("65", "constant.numeric.decimal")], []),
    ("let xs: List[bool] = [false]", [("List", "support.type"), ("bool", "support.type.primitive"), ("false", "constant.language.boolean"), ("[", "punctuation.brackets")], []),
    ("total = add(total, xs.len())", [("add", "entity.name.function.call"), ("len", "entity.name.function.method"), (".", "punctuation.accessor")], [("total", "entity"), ("xs", "entity")]),
    ("let p = Point { x: 1, y: -2 }", [("Point", "entity.name.type"), ("1", "constant.numeric.decimal"), ("-", "keyword.operator.arithmetic"), ("2", "constant.numeric.decimal")], []),
    ("let format = information + printer", [], [("for", "keyword"), ("in", "keyword"), ("print", "support"), ("format", "keyword"), ("information", "keyword")]),
    ("let naïve = 1", [("1", "constant.numeric.decimal")], [("ve", "keyword"), ("na", "keyword")]),
    ("record(mut c, \"ann\")", [("record", "entity.name.function.call"), ("mut", "storage.modifier.mut"), ("\"ann\"", "string.quoted.double")], []),
    ("x = a * b / c % d ^ e", [("*", "keyword.operator.arithmetic"), ("/", "keyword.operator.arithmetic"), ("%", "keyword.operator.arithmetic"), ("^", "keyword.operator.bitwise")], []),
]


def check_json():
    package = load("package.json")
    contributes = package["contributes"]
    language = contributes["languages"][0]
    grammar_entry = contributes["grammars"][0]
    assert language["id"] == "azol" and ".azl" in language["extensions"]
    assert grammar_entry["language"] == "azol" and grammar_entry["scopeName"] == "source.azol"
    for key in ("name", "version", "publisher", "engines"):
        assert key in package, key
    for path in (language["configuration"], grammar_entry["path"]):
        assert os.path.exists(os.path.join(HERE, path)), path
    configuration = load("language-configuration.json")
    assert configuration["comments"]["lineComment"] == "//"
    assert ["{", "}"] in configuration["brackets"]
    assert len(configuration["autoClosingPairs"]) >= 4
    for rule in configuration["indentationRules"].values():
        re.compile(rule)
    grammar = load(os.path.join("syntaxes", "azol.tmLanguage.json"))
    assert grammar["scopeName"] == "source.azol"
    for pattern in grammar["patterns"]:
        assert pattern["include"][1:] in grammar["repository"], pattern
    return grammar


def main():
    grammar = Grammar(check_json())
    failures = 0
    checks = 0
    for line, wanted, unwanted in SAMPLES:
        tokens = grammar.tokenize(line)
        for text, scope in wanted:
            checks += 1
            if not any(t == text and s.startswith(scope) for t, s in tokens):
                failures += 1
                print(f"FAIL {line!r}: {text!r} should be {scope}; got {[(t, s) for t, s in tokens if t == text]}")
        for text, scope in unwanted:
            checks += 1
            wrong = [(t, s) for t, s in tokens if t == text and s.startswith(scope)]
            if wrong:
                failures += 1
                print(f"FAIL {line!r}: {text!r} should not be {scope}; got {wrong}")
    if failures:
        print(f"test_grammar: {failures} of {checks} checks failed")
        return 1
    print(f"test_grammar: JSON files are valid; {checks} checks on {len(SAMPLES)} lines passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
