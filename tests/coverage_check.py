#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Checks tests/COVERAGE.md against the tests of the first compiler.

Every `#[test]` function in src/*.rs and tests/*.rs must have a row in the
section of its file in tests/COVERAGE.md, and every case file a row names
must exist. Run it while the Rust sources are still in the repository:

    python3 tests/coverage_check.py

Once they are gone the table is history and this script only checks that
the case files it names exist.
"""
import glob, os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)


def rust_tests(path):
    text = open(path, encoding="utf-8").read()
    return re.findall(r"#\[test\]\s*(?:#\[[^\]]*\]\s*)*fn\s+([a-z0-9_]+)", text)


def main():
    table = open(os.path.join(HERE, "COVERAGE.md"), encoding="utf-8").read()
    sections = {}
    for part in re.split(r"^## ", table, flags=re.M)[1:]:
        name = part.split(" ", 1)[0].strip()
        sections[name] = re.findall(r"^\| `([a-z0-9_]+)` \|", part, re.M)
    problems = []
    sources = sorted(glob.glob(os.path.join(ROOT, "src", "*.rs"))) + sorted(glob.glob(os.path.join(ROOT, "tests", "*.rs")))
    counted = 0
    for path in sources:
        shown = os.path.relpath(path, ROOT)
        tests = rust_tests(path)
        if not tests:
            continue
        counted += len(tests)
        rows = sections.get(shown)
        if rows is None:
            problems.append("%s: no section in COVERAGE.md for its %d tests" % (shown, len(tests)))
            continue
        for test in tests:
            if test not in rows:
                problems.append("%s: `%s` has no row" % (shown, test))
        for row in rows:
            if row not in tests:
                problems.append("%s: the row `%s` names a test that does not exist" % (shown, row))
    files = set(re.findall(r"`(tests/[A-Za-z0-9_./-]+\.(?:cases|azl|py|txt))`", table))
    for name in sorted(files):
        if not os.path.exists(os.path.join(ROOT, name)):
            problems.append("COVERAGE.md names %s, which does not exist" % name)
    for problem in problems:
        print(problem)
    if problems:
        print("%d problems" % len(problems))
        return 1
    if sources:
        print("COVERAGE.md has a row for each of the %d tests of the first compiler; the %d files it names exist" % (counted, len(files)))
    else:
        print("the %d files COVERAGE.md names exist (the first compiler's sources are gone)" % len(files))
    return 0


if __name__ == "__main__":
    sys.exit(main())
