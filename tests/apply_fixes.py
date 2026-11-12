# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Applies the fixes `azol check --json` offers, until none is left.

    python3 tests/apply_fixes.py [--azol PATH] <files that make one program...>

The files are checked together as one program. Each round applies the
first fix of every file and checks again, so a file with several mistakes
takes several rounds. An error without a fix is left and reported at the
end. Used to move sources to a newer azoL, for example to write a brace
in a text as `\\{`.
"""
import json, os, subprocess, sys


def main():
    arguments = sys.argv[1:]
    azol = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "azol")
    if "--azol" in arguments:
        at = arguments.index("--azol")
        azol = arguments[at + 1]
        del arguments[at:at + 2]
    if not arguments:
        sys.exit(__doc__)
    applied = 0
    for _ in range(100000):
        done = subprocess.run([azol, "check", "--json"] + arguments, capture_output=True, text=True)
        reports = [json.loads(line) for line in done.stdout.splitlines() if line.startswith('{"severity"')]
        fixes = {}
        for report in reports:
            fix = report.get("fix")
            if fix and fix["file"] not in fixes:
                fixes[fix["file"]] = fix
        if not fixes:
            for report in reports:
                print("left: %s:%d:%d %s %s" % (report["file"], report["line"], report["column"], report["code"], report["message"]))
            print("%d fixes applied, %d errors left" % (applied, len(reports)))
            sys.exit(1 if reports else 0)
        for path, fix in fixes.items():
            with open(path, encoding="utf-8") as source:
                lines = source.read().split("\n")
            first, last = fix["line"] - 1, fix["end_line"] - 1
            head = lines[first][:fix["column"] - 1]
            tail = lines[last][fix["end_column"] - 1:]
            lines[first:last + 1] = (head + fix["replacement"] + tail).split("\n")
            with open(path, "w", encoding="utf-8") as source:
                source.write("\n".join(lines))
            applied += 1
    sys.exit("too many rounds")


if __name__ == "__main__":
    main()
