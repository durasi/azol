#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Checks that README.md and README.tr.md are the same document in two languages.

    python3 docs/check_readme_langs.py

`sh test.sh` runs it in its `format` section. It cannot read Turkish, so it
cannot say that a sentence is translated well, or at all; what it can say is
that the two files have the same shape, and it fails when they do not:

  - both begin with the language switch, each with its own tab filled in
  - the same headings, level by level, in the same order
  - the same lists: as many bullets and as many numbered items under each heading
  - the same tables: as many, each with as many rows and columns, the same
    cells in bold, and the same numbers in every row
  - the same fenced code blocks, character for character
  - the same numbers between the benchmark markers
  - the same links to files and the same pictures (the switch aside)
  - every link inside the page points at a heading that exists in that page
  - no em dash in either

A change to one README that is not made in the other fails one of these,
unless it is a change of words alone.
"""

import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
FILES = {"en": "README.md", "tr": "README.tr.md"}
# From the Turkish page, "English" goes to the repository's front page, which shows README.md.
HOME = "https://github.com/durasi/azol#readme"
TABS = {"en": ("assets/lang/en-on.svg", "assets/lang/tr-off.svg"), "tr": ("assets/lang/en-off.svg", "assets/lang/tr-on.svg")}
START, END = "<!-- bench:start -->", "<!-- bench:end -->"
NUMBER = re.compile(r"\d+(?:[.,]\d+)*")


def slug(heading):
    """The anchor GitHub gives a heading: lower case, punctuation dropped,
    spaces to hyphens. Letters of any alphabet are kept."""
    text = re.sub(r"<[^>]+>", "", heading)
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text).replace("`", "").strip().lower()
    text = "".join(ch for ch in text if ch.isalnum() or ch in " -_")
    return text.replace(" ", "-")


def parse(text):
    """What the checks compare, read from one README."""
    doc = {"headings": [], "code": [], "tables": [], "lists": [], "links": [], "images": [], "anchors": []}
    lines = text.split("\n")
    fence, block, table, counts = None, [], None, None
    for line in lines:
        if fence is not None:
            if line.strip() == "```":
                doc["code"].append((fence, "\n".join(block)))
                fence = None
            else:
                block.append(line)
            continue
        if line.startswith("```"):
            fence, block = line[3:].strip(), []
            continue
        if line.startswith("|"):
            cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
            if table is None:
                table = []
                doc["tables"].append(table)
            table.append(cells)
        else:
            table = None
        found = re.match(r"^(#{1,6}) +(.*)$", line)
        if found:
            doc["headings"].append((len(found.group(1)), found.group(2).strip()))
            counts = [found.group(2).strip(), 0, 0]
            doc["lists"].append(counts)
        elif counts is not None and re.match(r"^- ", line):
            counts[1] += 1
        elif counts is not None and re.match(r"^\d+\. ", line):
            counts[2] += 1
        for target in re.findall(r"\]\(([^)\s]+)\)", line) + re.findall(r'href="([^"]+)"', line):
            if target == HOME:
                target = "README.md"
            (doc["anchors"] if target.startswith("#") else doc["links"]).append(target)
        doc["images"].extend(re.findall(r'src="([^"]+)"', line))
    if fence is not None:
        doc["code"].append((fence, "\n".join(block) + "\n(never closed)"))
    return doc


def numbers(text):
    return sorted(NUMBER.findall(text))


def check():
    problems = []
    texts, docs = {}, {}
    for lang, name in FILES.items():
        path = os.path.join(ROOT, name)
        if not os.path.exists(path):
            return ["%s is missing: the README exists in English and in Turkish" % name]
        with open(path, encoding="utf-8") as handle:
            texts[lang] = handle.read()
        docs[lang] = parse(texts[lang])
    en, tr = docs["en"], docs["tr"]

    for lang, name in FILES.items():
        top = texts[lang].split("\n\n", 1)[0]
        on_off = TABS[lang]
        english = 'href="README.md"' if lang == "en" else 'href="%s"' % HOME
        if english not in top or 'href="README.tr.md"' not in top or any(src not in top for src in on_off):
            problems.append("%s must begin with the language switch: links to README.md and README.tr.md with %s and %s"
                            % (name, on_off[0], on_off[1]))
        if "—" in texts[lang]:
            problems.append("%s has an em dash; the project does not use it" % name)
        if START not in texts[lang] or END not in texts[lang]:
            problems.append("%s has no %s ... %s block for bench/compare.py" % (name, START, END))
        slugs = set()
        for _, heading in docs[lang]["headings"]:
            slugs.add(slug(heading))
        for anchor in docs[lang]["anchors"]:
            if anchor[1:] not in slugs:
                problems.append("%s links to %s, but no heading there has that anchor (the headings give: %s)"
                                % (name, anchor, ", ".join("#" + one for one in sorted(slugs))))

    if [level for level, _ in en["headings"]] != [level for level, _ in tr["headings"]]:
        problems.append("the headings differ: README.md has %d (levels %s), README.tr.md has %d (levels %s)"
                        % (len(en["headings"]), "".join(str(level) for level, _ in en["headings"]),
                           len(tr["headings"]), "".join(str(level) for level, _ in tr["headings"])))
    else:
        for (name_en, bullets_en, items_en), (name_tr, bullets_tr, items_tr) in zip(en["lists"], tr["lists"]):
            if (bullets_en, items_en) != (bullets_tr, items_tr):
                problems.append("under `%s` there are %d bullets and %d numbered items, under `%s` %d and %d"
                                % (name_en, bullets_en, items_en, name_tr, bullets_tr, items_tr))

    if len(en["code"]) != len(tr["code"]):
        problems.append("README.md has %d code blocks, README.tr.md has %d" % (len(en["code"]), len(tr["code"])))
    else:
        for at, (one, other) in enumerate(zip(en["code"], tr["code"]), 1):
            if one != other:
                first = one[1].split("\n")[0] if one[1] else ""
                problems.append("code block %d differs between the two files (it begins `%s`); code is never translated" % (at, first[:60]))

    if len(en["tables"]) != len(tr["tables"]):
        problems.append("README.md has %d tables, README.tr.md has %d" % (len(en["tables"]), len(tr["tables"])))
    else:
        for at, (one, other) in enumerate(zip(en["tables"], tr["tables"]), 1):
            if len(one) != len(other) or [len(cells) for cells in one] != [len(cells) for cells in other]:
                problems.append("table %d: %d rows in README.md, %d in README.tr.md, or rows of different widths"
                                % (at, len(one), len(other)))
                continue
            for row, (cells, others) in enumerate(zip(one, other), 1):
                if [cell.count("**") for cell in cells] != [cell.count("**") for cell in others]:
                    problems.append("table %d, row %d (`%s`): different cells are bold in the two files" % (at, row, cells[0][:50]))
                if numbers(" ".join(cells)) != numbers(" ".join(others)):
                    problems.append("table %d, row %d (`%s`): the numbers differ: %s against %s"
                                    % (at, row, cells[0][:50], numbers(" ".join(cells)), numbers(" ".join(others))))

    if all(START in texts[lang] and END in texts[lang] for lang in FILES):
        blocks = {lang: texts[lang].split(START, 1)[1].split(END, 1)[0] for lang in FILES}
        if numbers(blocks["en"]) != numbers(blocks["tr"]):
            only_en = [n for n in numbers(blocks["en"]) if n not in numbers(blocks["tr"])]
            only_tr = [n for n in numbers(blocks["tr"]) if n not in numbers(blocks["en"])]
            problems.append("the benchmark blocks hold different numbers (only in README.md: %s; only in README.tr.md: %s); "
                            "write both with `python3 bench/compare.py --write-readme` or `--from-results --write-readme`"
                            % (only_en[:8], only_tr[:8]))

    own = set(FILES.values())
    for what in ("links", "images"):
        one = sorted(item for item in en[what] if item not in own and not item.startswith("assets/lang/"))
        other = sorted(item for item in tr[what] if item not in own and not item.startswith("assets/lang/"))
        if one != other:
            missing = [item for item in set(one) | set(other) if one.count(item) != other.count(item)]
            problems.append("the two files do not have the same %s; these differ in number: %s" % (what, ", ".join(sorted(missing)[:10])))
    return problems


def main():
    problems = check()
    for problem in problems:
        print("check_readme_langs: " + problem)
    if problems:
        print("check_readme_langs: README.md and README.tr.md have drifted apart; a change to one must be made in the other")
        return 1
    print("check_readme_langs: README.md and README.tr.md have the same shape")
    return 0


if __name__ == "__main__":
    sys.exit(main())
