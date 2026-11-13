#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Draws the two pictures of the README's Numbers section from its own tables.

    python3 bench/chart.py            write assets/bench/*.svg from README.md

It reads the block between `<!-- bench:start -->` and `<!-- bench:end -->`,
so the pictures can never say anything the tables do not. `compare.py
--write-readme` calls it after it has written the block. Each picture is
written twice, for light and for dark pages.
"""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
OUT = os.path.join(ROOT, "assets", "bench")
FONT = "ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Helvetica, Arial, sans-serif"
TEAL, RUST = "#0B9BB0", "#CE422B"
INK = {"light": ("#1B1F24", "#59636E", "#D1D9E0"), "dark": ("#E6EDF3", "#9198A1", "#3D444D")}
WIDTH, RIGHT, BAR, STEP = 760, 96, 16, 30


def cell_seconds(text):
    found = re.search(r"([0-9.]+) s", text)
    return float(found.group(1)) if found else None


def read_block():
    with open(os.path.join(ROOT, "README.md"), encoding="utf-8") as handle:
        text = handle.read()
    start, end = text.find("<!-- bench:start -->"), text.find("<!-- bench:end -->")
    if start < 0 or end < 0:
        sys.exit("chart.py: README.md has no bench block")
    runs, builds, where = [], [], ""
    for line in text[start:end].split("\n"):
        if line.startswith("**Run time**"):
            where = "run"
        elif line.startswith("**Build time**"):
            where = "build"
        elif line.startswith("**Executable size**"):
            where = ""
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if not line.startswith("|") or len(cells) < 2 or set(cells[1]) <= set("-: "):
            continue
        if where == "run" and len(cells) >= 3:
            azol, rust = cell_seconds(cells[1]), cell_seconds(cells[2])
            name = re.search(r"`([^`]+)`", cells[0])
            if azol and rust and name:
                runs.append((name.group(1), azol / rust))
        elif where == "build":
            seconds = cell_seconds(cells[1])
            label = re.sub(r"\s*\(.*\)", "", cells[0].replace("`", ""))
            if seconds and not label.startswith("JavaScript"):
                builds.append((label, seconds))
    return runs, builds


def number(value, small=3):
    if value >= 10:
        return "%.1f" % value
    if value >= 1:
        return "%.2f" % value
    return "%.*f" % (small, value)


def svg(title, subtitle, rows, colours, labels, limit, mode, left, mark=None):
    """Horizontal bars from one baseline. `rows` is (name, value); `mark` is a
    value to draw a labelled line at."""
    primary, secondary, grid = INK[mode]
    top = 62 if mark is None else 80
    LEFT = left
    height = top + STEP * len(rows) + 18
    plot = WIDTH - LEFT - RIGHT
    out = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d" width="%d" height="%d" font-family="%s">'
           % (WIDTH, height, WIDTH, height, FONT),
           "<title>%s</title>" % title,
           '<text x="0" y="18" font-size="15" font-weight="600" fill="%s">%s</text>' % (primary, title),
           '<text x="0" y="38" font-size="12.5" fill="%s">%s</text>' % (secondary, subtitle),
           '<path d="M%d %dV%d" stroke="%s" stroke-width="1"/>' % (LEFT, top - 8, height - 14, grid)]
    if mark is not None:
        x = LEFT + plot * mark / limit
        out.append('<path d="M%.1f %dV%d" stroke="%s" stroke-width="1"/>' % (x, top - 8, height - 14, secondary))
        out.append('<text x="%.1f" y="%d" font-size="11.5" fill="%s" text-anchor="middle">%s</text>'
                   % (x, top - 13, secondary, labels["mark"]))
    for index, (name, value) in enumerate(rows):
        y = top + STEP * index
        length = max(2.0, plot * min(value, limit) / limit)
        radius = min(4.0, length / 2)
        out.append('<text x="%d" y="%.1f" font-size="12.5" fill="%s" text-anchor="end">%s</text>'
                   % (LEFT - 10, y + BAR / 2 + 4.5, primary, name))
        out.append('<path d="M%d %dh%.1fa%.1f %.1f 0 0 1 %.1f %.1fv%.1fa%.1f %.1f 0 0 1 -%.1f %.1fh-%.1fz" fill="%s"/>'
                   % (LEFT, y, length - radius, radius, radius, radius, radius, BAR - 2 * radius,
                      radius, radius, radius, radius, length - radius, colours[index]))
        text = labels["value"](value)
        line = None if mark is None else plot * mark / limit
        if line is not None and length < line and length + 14 + 7.5 * len(text) > line:
            out.append('<text x="%.1f" y="%.1f" font-size="12" font-weight="600" fill="#FFFFFF" text-anchor="end">%s</text>'
                       % (LEFT + length - 7, y + BAR / 2 + 4.5, text))
        else:
            out.append('<text x="%.1f" y="%.1f" font-size="12.5" font-weight="600" fill="%s">%s</text>'
                       % (LEFT + length + 8, y + BAR / 2 + 4.5, primary, text))
    out.append("</svg>")
    return "\n".join(out) + "\n"


def write(name, text):
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, name), "w", encoding="utf-8") as handle:
        handle.write(text)


def main():
    runs, builds = read_block()
    if not runs or not builds:
        sys.exit("chart.py: the bench block has no run-time or build-time table to draw")
    for mode in ("light", "dark"):
        limit = max(value for _, value in builds)
        colours = [TEAL if label.startswith("azoL") else RUST for label, _ in builds]
        write("build-%s.svg" % mode, svg(
            "Build time", "Seconds from source to result for the same generated program. Shorter is better.",
            builds, colours, {"value": lambda value: number(value) + " s"}, limit, mode, 232))
        limit = max(1.25, max(value for _, value in runs)) * 1.02
        write("run-%s.svg" % mode, svg(
            "Run time against optimized Rust", "azoL's default build; 1.0 is the same program built with rustc -O. Shorter is better.",
            runs, [TEAL] * len(runs), {"value": lambda value: number(value, 2) + "×", "mark": "Rust -O = 1.0"},
            limit, mode, 84, mark=1.0))
    print("assets/bench: build and run pictures written for light and dark pages")


if __name__ == "__main__":
    main()
