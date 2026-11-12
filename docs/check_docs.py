#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""Checks every azoL example in docs/language.md and docs/for-agents.md.

Each example is a fenced code block. The word after the opening fence says
how the block is checked:

    ```azol
        A complete program. It must compile and run. If the next fenced
        block is ```output, the program's standard output must equal it
        exactly. If a ```stderr block follows (directly, or after the
        ```output block), standard error must equal it exactly.
        Options after the word:
            exit=N          the exit code the program must end with
                            (default 0)
            args="a b"      command-line arguments for the run
            group=G file=F  one file of a program made of several files;
                            blocks with the same group are built together,
                            in the order they appear, and the options and
                            the expected output are taken from the last one

    ```azol-error
        A complete program that must NOT compile. The next fenced block
        must be ```error; every line of it must appear in what the
        compiler prints.

    ```azol-fragment
        A piece of a program, shown for its shape only. Not checked.

Blocks with any other word (text, sh, json, ...) are ignored.

Programs are built for x86_64-linux and run on this machine, so the
script needs x86-64 Linux. Run it from anywhere:

    cargo build --release
    python3 docs/check_docs.py
"""

import os
import re
import shlex
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
DOCUMENTS = ["language.md", "for-agents.md"]
FENCE = re.compile(r"^```(.*)$")


def find_compiler():
    path = os.environ.get("AZOL")
    if path:
        return path
    path = os.path.join(ROOT, "target", "release", "azol")
    if not os.path.exists(path):
        sys.exit("check_docs: build the compiler first: cargo build --release")
    return path


def blocks_of(path):
    """The fenced blocks of a Markdown file: (line, info, text)."""
    found = []
    info = None
    start = 0
    lines = []
    with open(path, encoding="utf-8") as handle:
        for number, line in enumerate(handle, 1):
            line = line.rstrip("\n")
            match = FENCE.match(line)
            if info is None:
                if match:
                    info = match.group(1).strip()
                    start = number
                    lines = []
            elif line == "```":
                found.append((start, info, "\n".join(lines) + "\n"))
                info = None
            else:
                lines.append(line)
    if info is not None:
        sys.exit(f"check_docs: {path}:{start}: code block is never closed")
    return found


def options_of(info):
    words = shlex.split(info)
    options = {}
    for word in words[1:]:
        key, _, value = word.partition("=")
        options[key] = value
    return words[0] if words else "", options


def examples_of(name):
    """Groups the blocks of one document into examples to check."""
    path = os.path.join(HERE, name)
    blocks = blocks_of(path)
    examples = []
    groups = {}
    fragments = 0
    i = 0
    while i < len(blocks):
        line, info, text = blocks[i]
        kind, options = options_of(info)
        i += 1
        if kind == "azol-fragment":
            fragments += 1
            continue
        if kind not in ("azol", "azol-error"):
            if kind in ("output", "stderr", "error"):
                sys.exit(f"check_docs: {name}:{line}: a ```{kind} block must follow a program")
            continue
        example = {
            "where": f"{name}:{line}",
            "kind": kind,
            "files": [(options.get("file", "example.azl"), text)],
            "options": options,
            "output": None,
            "stderr": None,
            "error": None,
        }
        while i < len(blocks):
            next_kind, _ = options_of(blocks[i][1])
            if kind == "azol" and next_kind in ("output", "stderr") and example[next_kind] is None:
                example[next_kind] = blocks[i][2]
            elif kind == "azol-error" and next_kind == "error" and example["error"] is None:
                example["error"] = blocks[i][2]
            else:
                break
            i += 1
        if kind == "azol-error" and example["error"] is None:
            sys.exit(f"check_docs: {name}:{line}: an ```azol-error block needs an ```error block after it")
        group = options.get("group")
        if group is None:
            examples.append(example)
        elif group in groups:
            # A later file of a group: it carries the expectations.
            first = groups[group]
            first["files"].append(example["files"][0])
            for key in ("options", "output", "stderr", "where"):
                first[key] = example[key]
        else:
            groups[group] = example
            examples.append(example)
    return examples, fragments


def check(compiler, example, folder):
    """Returns a list of complaints; empty when the example is right."""
    paths = []
    for file_name, text in example["files"]:
        path = os.path.join(folder, file_name)
        with open(path, "w", encoding="utf-8") as handle:
            handle.write(text)
        paths.append(path)
    binary = os.path.join(folder, "program")
    build = subprocess.run(
        [compiler, "build", *paths, "-o", binary, "--target", "x86_64-linux"],
        capture_output=True, text=True, errors="replace",
    )
    if example["kind"] == "azol-error":
        if build.returncode == 0:
            return ["compiled, but it must not"]
        missing = [
            line.strip() for line in example["error"].splitlines()
            if line.strip() and line.strip() not in build.stderr
        ]
        return [f"the compiler did not print: {line}\n--- it printed:\n{build.stderr}" for line in missing[:1]]
    if build.returncode != 0:
        return [f"does not compile:\n{build.stderr}"]
    options = example["options"]
    arguments = shlex.split(options.get("args", ""))
    try:
        run = subprocess.run(
            [binary, *arguments], capture_output=True, text=True, errors="replace",
            timeout=20, cwd=folder,
        )
    except subprocess.TimeoutExpired:
        return ["still running after 20 seconds"]
    complaints = []
    wanted_exit = int(options.get("exit", "0"))
    if run.returncode != wanted_exit:
        complaints.append(f"ended with exit code {run.returncode}, expected {wanted_exit}\n--- stderr:\n{run.stderr}")
    if example["output"] is not None and run.stdout != example["output"]:
        complaints.append(f"printed\n{run.stdout}--- expected\n{example['output']}")
    if example["stderr"] is not None and run.stderr != example["stderr"]:
        complaints.append(f"printed on standard error\n{run.stderr}--- expected\n{example['stderr']}")
    return complaints


def main():
    compiler = find_compiler()
    failed = 0
    total = 0
    for name in DOCUMENTS:
        examples, fragments = examples_of(name)
        programs = sum(1 for e in examples if e["kind"] == "azol")
        errors = len(examples) - programs
        bad = 0
        for example in examples:
            with tempfile.TemporaryDirectory(prefix="azol_docs_") as folder:
                complaints = check(compiler, example, folder)
            for complaint in complaints:
                print(f"FAIL {example['where']}: {complaint}")
            if complaints:
                bad += 1
        total += len(examples)
        failed += bad
        print(
            f"{name}: {len(examples)} examples checked "
            f"({programs} must run, {errors} must fail to compile), "
            f"{fragments} fragments not checked, {bad} failed"
        )
    if failed:
        print(f"check_docs: {failed} of {total} examples are wrong")
        return 1
    print(f"check_docs: all {total} examples are right")
    return 0


if __name__ == "__main__":
    sys.exit(main())
