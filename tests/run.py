#!/usr/bin/env python3
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
"""The azoL test suite. It needs an `azol` executable and Python 3, nothing else.

    python3 tests/run.py [--azol PATH] [--jobs N] [--update-golden] [section ...]

Sections (all of them when none is named):

    programs  builds every program in tests/programs for this machine, runs it
              and compares what it prints and how it ends with its `// expect-`
              comments; several files as one program; `check`, `--json`,
              `explain`; SHA-256 and division by constants
    errors    the case files under tests/errors and tests/golden: wrong programs
              with the exact report they must get, the tokens and syntax trees
              of right ones, the command line; damaged input; the error codes
    format    `azol fmt`: the pairs in tests/format, the case files in
              tests/golden/format and its guarantees
    cross     every program built for all three targets; each executable is
              checked for its structure (and a Mach-O for its signature) and its
              SHA-256 is compared with tests/golden/executables.txt
    self      the compiler builds itself to a fixed point (build.sh)
              and the compiler that comes out passes the four sections above

`--update-golden` rewrites what the case files and tests/golden/executables.txt
expect from what the given compiler does now: the `stdout`, `stderr`, `after`
and `executables` texts. Everything else in a case file is never rewritten:
exit codes, and the `asserted`, `contains`, `reports`, `fix-at`, `no-fix` and
`after-has` lines, which are what the tests of the first compiler required
word for word. They must keep holding, or be changed by hand.

A case file (`*.cases`) is a list of cases. Lines before the first case that
start with `#` are comments.

    ==== a title
    ---- file a.azl                  a source file, exactly the lines that follow
    ---- stdin                       standard input of the next run
    ---- run check --json a.azl      runs azol with these arguments, in a folder
                                     that holds the files of the case
    ---- exit 1                      its exit code
    ---- stdout                      what it must write to standard output
    ---- stderr                      and to standard error (times are left out)
    ---- asserted 3:17 a message     standard error must name this place and text
    ---- contains some text          standard error must contain this text
    ---- reports E0401 3:5, E0201 7:1  the errors of the last `--json` run, in
                                     order: code and place (or the code alone), optionally with the
                                     file before and `-line:column` of the end
                                     after; `*` for any code; `none` for no error
    ---- no-fix                      its only error offers no fix
    ---- fix-at a.azl 2:5-2:8        where the fix of its only error goes
    ---- apply-fix                   applies the `fix` of the only error of the
                                     last `--json` run to the files
    ---- after a.azl                 what a file must hold after the last run
    ---- after-has a.azl             text that the file must contain
    ---- exists NAME / missing NAME  a file the last run must (not) have written
    ---- note some words             a remark, not a check
    ---- run build-all a.azl         builds for all three targets
    ---- executables                 one `target sha256` line for each

Text ends with a line break unless the header line says `(no newline at end)`;
`(empty)` is no text at all; `(escaped)` gives the text on one line with `\\n`,
`\\t`, `\\xNN` escapes, for bytes that would not survive in a text file.
"""

import concurrent.futures
import hashlib
import json
import os
import platform
import re
import shlex
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
TARGETS = ["aarch64-macos", "x86_64-linux", "aarch64-linux"]
SECTIONS = ["programs", "errors", "format", "cross", "self"]
TIMEOUT = 120


def host_target():
    system, machine = platform.system(), platform.machine().lower()
    if system == "Darwin" and machine in ("arm64", "aarch64"):
        return "aarch64-macos"
    if system == "Linux" and machine in ("x86_64", "amd64"):
        return "x86_64-linux"
    if system == "Linux" and machine in ("aarch64", "arm64"):
        return "aarch64-linux"
    return None


HOST = host_target()


class Suite:
    """What one run of the suite knows: the compiler and the options."""

    def __init__(self, azol, jobs, update):
        self.azol = os.path.abspath(azol)
        self.jobs = jobs
        self.update = update
        self.tmp = tempfile.mkdtemp(prefix="azol-test-")

    def folder(self, name="t"):
        return tempfile.mkdtemp(prefix=name + "-", dir=self.tmp)

    def run(self, args, cwd=None, stdin=None):
        """Runs the compiler; returns (exit code, stdout, stderr) as bytes."""
        try:
            done = subprocess.run([self.azol] + list(args), cwd=cwd, input=stdin if stdin is not None else b"", capture_output=True, timeout=TIMEOUT)
        except subprocess.TimeoutExpired:
            return -999, b"", b"(no answer within %d seconds)" % TIMEOUT
        return done.returncode, done.stdout, done.stderr

    def close(self):
        shutil.rmtree(self.tmp, ignore_errors=True)


class Section:
    def __init__(self, name):
        self.name = name
        self.checks = 0
        self.failures = []
        self.started = time.time()
        self.lock = threading.Lock()

    def check(self, good, label, detail=""):
        with self.lock:
            self.checks += 1
            if not good:
                self.failures.append((label, detail))
        return bool(good)

    def fail(self, label, detail=""):
        self.check(False, label, detail)

    def summary(self):
        seconds = getattr(self, "seconds", time.time() - self.started)
        if self.failures:
            return "%-9s %d of %d checks FAILED (%.1f s)" % (self.name + ":", len(self.failures), self.checks, seconds)
        return "%-9s %d checks passed (%.1f s)" % (self.name + ":", self.checks, seconds)


def text_of(data):
    return data.decode("utf-8", errors="replace")


def read(path):
    with open(path, "rb") as file:
        return file.read()


def write(path, data):
    folder = os.path.dirname(path)
    if folder:
        os.makedirs(folder, exist_ok=True)
    with open(path, "wb") as file:
        file.write(data)


def sources_in(folder, deep=True):
    """The .azl files of a folder of the repository, sorted."""
    found = []
    top = os.path.join(ROOT, folder)
    if not os.path.isdir(top):
        return found
    for base, folders, files in os.walk(top):
        folders.sort()
        if not deep:
            del folders[:]
        for name in sorted(files):
            if name.endswith(".azl"):
                found.append(os.path.join(base, name))
    return sorted(found)


TIME_LINE = re.compile(rb"^((?:lexed|parsed) .*?) in [0-9.]+ ms \(.*\)$")
BUILT_LINE = re.compile(rb"^(built .* \(\d+ bytes of code\)) in [0-9.]+ ms \(.*\)$")
CHECKED_LINE = re.compile(rb"^(checked .*: no errors) \([0-9.]+ ms\)$")


def without_times(data):
    """Standard error with the measured times taken out."""
    lines = data.split(b"\n")
    for at, line in enumerate(lines):
        for pattern in (TIME_LINE, BUILT_LINE, CHECKED_LINE):
            match = pattern.match(line)
            if match:
                lines[at] = match.group(1)
                break
    return b"\n".join(lines)


def difference(want, got):
    """A short account of where two texts differ."""
    if want == got:
        return ""
    at = 0
    while at < min(len(want), len(got)) and want[at] == got[at]:
        at += 1
    line = want[:at].count(b"\n") + 1
    start = max(want.rfind(b"\n", 0, at) + 1, at - 60)
    return "first difference in line %d\n      wanted: %r\n      got:    %r" % (line, text_of(want[start:at + 80]), text_of(got[start:at + 80]))


def in_parallel(suite, jobs_list, work):
    """Runs work(job) for every job; results in the order of the jobs."""
    if suite.jobs <= 1:
        return [work(job) for job in jobs_list]
    with concurrent.futures.ThreadPoolExecutor(suite.jobs) as pool:
        return list(pool.map(work, jobs_list))


# ---- case files ---------------------------------------------------------

WITH_TEXT = ("file", "stdin", "stdout", "stderr", "after", "after-has", "executables")


def escape(data):
    return data.decode("latin-1").encode("unicode_escape").decode("ascii")


def unescape(text):
    return text.encode("ascii").decode("unicode_escape").encode("latin-1")


def plain(data):
    """Whether text can stand in a case file as it is."""
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return False
    if any((ord(c) < 32 and c not in "\n\t") or ord(c) == 127 for c in text):
        return False
    return not any(line.startswith("---- ") or line.startswith("==== ") for line in text.split("\n"))


class Item:
    def __init__(self, kind, arg="", data=b""):
        self.kind = kind
        self.arg = arg
        self.data = data

    def lines(self):
        head = "---- " + self.kind + (" " + self.arg if self.arg else "")
        if self.kind not in WITH_TEXT:
            return [head.encode()]
        if self.data == b"":
            return [(head + " (empty)").encode()]
        if not plain(self.data):
            return [(head + " (escaped)").encode(), escape(self.data).encode()]
        if self.data.endswith(b"\n"):
            return [head.encode()] + self.data[:-1].split(b"\n")
        return [(head + " (no newline at end)").encode()] + self.data.split(b"\n")


class Case:
    def __init__(self, title):
        self.title = title
        self.items = []


def read_cases(path):
    comments, cases = [], []
    item, body, flag = None, [], ""

    def close():
        nonlocal item, body, flag
        if item is not None and item.kind in WITH_TEXT:
            if flag == "empty":
                item.data = b""
            elif flag == "escaped":
                item.data = unescape(b"\n".join(body).decode("ascii"))
            elif flag == "no newline at end":
                item.data = b"\n".join(body)
            else:
                item.data = b"\n".join(body) + b"\n"
        item, body, flag = None, [], ""

    for line in read(path).split(b"\n")[:-1]:
        if line.startswith(b"==== "):
            close()
            cases.append(Case(line[5:].decode()))
        elif line.startswith(b"---- ") and cases:
            close()
            head = line[5:].decode()
            for known in ("empty", "escaped", "no newline at end"):
                if head.endswith(" (" + known + ")"):
                    flag = known
                    head = head[:-len(known) - 3]
            kind, _, arg = head.partition(" ")
            item = Item(kind, arg)
            cases[-1].items.append(item)
        elif not cases:
            comments.append(line)
        else:
            body.append(line)
    close()
    return comments, cases


def write_cases(path, comments, cases):
    lines = list(comments)
    for case in cases:
        lines.append(("==== " + case.title).encode())
        for item in case.items:
            lines.extend(item.lines())
    write(path, b"\n".join(lines) + b"\n")


def apply_edit(data, edit):
    """Replaces the place a `fix` names; lines and columns count characters from 1."""
    lines = data.decode("utf-8").split("\n")

    def offset(line, column):
        return sum(len(text) + 1 for text in lines[:line - 1]) + column - 1

    text = "\n".join(lines)
    start, end = offset(edit["line"], edit["column"]), offset(edit["end_line"], edit["end_column"])
    return (text[:start] + edit["replacement"] + text[end:]).encode("utf-8")


def check_json(section, label, folder, code, out):
    """What must hold for every `--json` report, whatever the program."""
    lines = out.decode("utf-8", errors="replace").split("\n")
    if not section.check(lines[-1] == "" and len(lines) >= 2, label + ": JSON ends with a line break"):
        return []
    found = []
    for line in lines[:-1]:
        try:
            found.append(json.loads(line))
        except ValueError as problem:
            section.fail(label + ": every line is JSON", "%s in %r" % (problem, line))
            return []
    summary, found = found[-1], found[:-1]
    section.check(list(summary) == ["summary"] and summary["summary"].get("errors") == len(found), label + ": the summary counts the errors", repr(summary))
    section.check((code == 1) == bool(found) and code in (0, 1), label + ": exit code 1 exactly when there are errors", "exit %d, %d errors" % (code, len(found)))
    for one in found:
        wanted = ["severity", "code", "message", "file", "line", "column", "end_line", "end_column", "source", "rule"]
        section.check(list(one)[:10] == wanted and re.fullmatch(r"E0\d\d\d", one["code"]) and one["code"] != "E0000", label + ": an error has every field and a code", repr(list(one)))
        path = os.path.join(folder, one.get("file", ""))
        if os.path.isfile(path):
            source = read(path).decode("utf-8", errors="replace").split("\n")
            line = source[one["line"] - 1] if one["line"] <= len(source) else ""
            section.check(one["source"] == line.rstrip("\r") or one["source"] == line, label + ": `source` is the line of the file", "%r is not %r" % (one["source"], line))
            section.check((one["end_line"], one["end_column"]) >= (one["line"], one["column"]) and one["column"] >= 1, label + ": the place does not end before it starts")
    return found


def run_case(suite, section, path, case):
    label = "%s [%s]" % (os.path.relpath(path, ROOT), case.title)
    folder = suite.folder("case")
    stdin = None
    last = None  # (args, exit code, stdout, stderr, diagnostics)
    changed = False
    items = case.items
    for at, item in enumerate(items):
        kind = item.kind
        if kind == "file":
            write(os.path.join(folder, item.arg), item.data)
        elif kind == "stdin":
            stdin = item.data
        elif kind == "note":
            pass
        elif kind == "run":
            args = shlex.split(item.arg)
            if args and args[0] == "build-all":
                hashes = []
                for target in TARGETS:
                    out = os.path.join(folder, "program")
                    if os.path.exists(out):
                        os.remove(out)
                    code, _, err = suite.run(["build", "--target", target] + args[1:] + ["-o", "program"], cwd=folder)
                    if section.check(code == 0 and os.path.isfile(out), label + ": builds for " + target, text_of(err)):
                        data = read(out)
                        problem = structure_problem(data, target, "program")
                        section.check(not problem, label + ": a sound executable for " + target, problem)
                        hashes.append("%s %s" % (target, hashlib.sha256(data).hexdigest()))
                    if os.path.exists(out):
                        os.remove(out)
                last = (args, 0, b"", ("\n".join(hashes) + "\n").encode(), [])
            else:
                code, out, err = suite.run(args, cwd=folder, stdin=stdin)
                stdin = None
                section.check(code >= 0, label + ": `azol %s` ends by itself" % item.arg, "ended by signal %d\n%s" % (-code, text_of(err)))
                found = check_json(section, label, folder, code, out) if "--json" in args and code in (0, 1) else []
                last = (args, code, out, without_times(err), found)
        elif last is None:
            section.fail(label + ": `%s` comes before any run" % kind)
        elif kind == "exit":
            section.check(str(last[1]) == item.arg.strip(), label + ": exit code of `%s`" % " ".join(last[0]), "wanted %s, got %d\n%s" % (item.arg, last[1], text_of(last[3])))
        elif kind in ("stdout", "stderr", "executables"):
            got = last[2] if kind == "stdout" else last[3]
            if suite.update:
                changed |= item.data != got
                item.data = got
            else:
                section.check(item.data == got, label + ": %s of `%s`" % (kind, " ".join(last[0])), difference(item.data, got))
        elif kind == "asserted":
            place, _, message = item.arg.partition(" ")
            lines = text_of(last[3]).split("\n")
            section.check(lines[0].startswith("error[E0") and lines[0].split("]: ", 1)[-1] == message, label + ": the first error is the one the first compiler's tests required", "wanted: %s\n      got:    %s" % (message, lines[0]))
            if place != "-":
                section.check(len(lines) > 1 and lines[1].lstrip().startswith("--> ") and lines[1].endswith(":" + place), label + ": at %s" % place, "\n".join(lines[:2]))
        elif kind == "contains":
            section.check(item.arg in text_of(last[3]) or item.arg in text_of(last[2]), label + ": the output contains `%s`" % item.arg, text_of(last[3])[:400])
        elif kind == "reports":
            wanted = [] if item.arg.strip() == "none" else [part.strip() for part in item.arg.split(",")]
            got = []
            for one, want in zip(last[4], wanted + [""] * len(last[4])):
                words = want.split()
                if len(words) == 1:
                    got.append(one["code"])
                    continue
                text = "%s %d:%d" % (one["code"] if not words or words[-2:-1] != ["*"] else "*", one["line"], one["column"])
                if "-" in want:
                    text += "-%d:%d" % (one["end_line"], one["end_column"])
                if len(words) == 3:
                    text = one["file"] + " " + text
                got.append(text)
            section.check(got == wanted, label + ": the errors reported, in order", "wanted: %s\n      got:    %s" % (", ".join(wanted) or "none", ", ".join(got) or "none"))
        elif kind == "no-fix":
            section.check(len(last[4]) == 1 and "fix" not in last[4][0], label + ": one error, and no fix is offered", repr(last[4])[:300])
        elif kind == "fix-at":
            fix = last[4][0].get("fix") if len(last[4]) == 1 else None
            got = fix and "%s %d:%d-%d:%d" % (fix["file"], fix["line"], fix["column"], fix["end_line"], fix["end_column"])
            section.check(got == item.arg.strip(), label + ": where the fix goes", "wanted %s, got %s" % (item.arg, got))
        elif kind == "apply-fix":
            found = last[4]
            if section.check(len(found) == 1 and "fix" in found[0], label + ": one error, with a fix", repr(found)[:300]):
                fix = found[0]["fix"]
                target = os.path.join(folder, fix["file"])
                write(target, apply_edit(read(target), fix))
        elif kind == "after":
            target = os.path.join(folder, item.arg)
            got = read(target) if os.path.isfile(target) else b""
            if suite.update:
                changed |= item.data != got
                item.data = got
            else:
                section.check(item.data == got, label + ": %s after `%s`" % (item.arg, " ".join(last[0])), difference(item.data, got))
        elif kind == "after-has":
            name, target = item.arg, os.path.join(folder, item.arg)
            got = read(target) if os.path.isfile(target) else b""
            section.check(item.data in got, label + ": %s holds the required text after `%s`" % (name, " ".join(last[0])), "wanted: %r\n      in:     %r" % (text_of(item.data), text_of(got)))
        elif kind == "exists":
            section.check(os.path.isfile(os.path.join(folder, item.arg)), label + ": %s was written" % item.arg)
        elif kind == "missing":
            section.check(not os.path.exists(os.path.join(folder, item.arg)), label + ": %s was not written" % item.arg)
        else:
            section.fail(label + ": unknown line `---- %s`" % kind)
    shutil.rmtree(folder, ignore_errors=True)
    return changed


def run_case_files(suite, section, folders):
    files = []
    for folder in folders:
        top = os.path.join(HERE, folder)
        if os.path.isdir(top):
            files += [os.path.join(top, name) for name in sorted(os.listdir(top)) if name.endswith(".cases")]
    loaded = [(path,) + read_cases(path) for path in files]
    jobs_list = [(path, case) for path, _, cases in loaded for case in cases]
    changed = in_parallel(suite, jobs_list, lambda job: run_case(suite, section, job[0], job[1]))
    if suite.update:
        rewritten = set(path for (path, _), one in zip(jobs_list, changed) if one)
        for path, comments, cases in loaded:
            if path in rewritten:
                write_cases(path, comments, cases)
                print("rewrote " + os.path.relpath(path, ROOT))
    return len(jobs_list)


# ---- the structure of executables ---------------------------------------


def elf_problem(data, machine):
    if len(data) < 64 or data[:4] != b"\x7fELF":
        return "not an ELF file"
    if data[4] != 2 or data[5] != 1:
        return "not a 64-bit little-endian ELF file"
    kind, arch = struct.unpack_from("<HH", data, 16)
    entry, phoff = struct.unpack_from("<QQ", data, 24)
    phentsize, phnum = struct.unpack_from("<HH", data, 54)
    if kind != 2:
        return "not a static executable (type %d)" % kind
    if arch != machine:
        return "made for machine %d, not %d" % (arch, machine)
    if phentsize != 56 or phoff + phnum * 56 > len(data) or phnum == 0:
        return "the program headers do not fit in the file"
    loads, entry_ok = [], False
    for at in range(phnum):
        p_type, flags, offset, vaddr, _, filesz, memsz, align = struct.unpack_from("<IIQQQQQQ", data, phoff + at * 56)
        if p_type == 3:
            return "asks for a dynamic loader"
        if p_type != 1:
            continue
        if filesz > memsz or offset + filesz > len(data):
            return "a segment does not fit in the file"
        if align > 1 and offset % align != vaddr % align:
            return "a segment is not placed on its alignment"
        if flags & 1 and vaddr <= entry < vaddr + filesz:
            entry_ok = True
        if flags & 1 and flags & 2:
            return "a segment is both writable and executable"
        loads.append((vaddr, vaddr + memsz))
    loads.sort()
    if any(a[1] > b[0] for a, b in zip(loads, loads[1:])):
        return "segments overlap in memory"
    if not entry_ok:
        return "the entry point is not inside an executable segment"
    return ""


def macho_problem(data, name):
    if len(data) < 32 or struct.unpack_from("<I", data, 0)[0] != 0xFEEDFACF:
        return "not a 64-bit Mach-O file"
    cputype, _, filetype, ncmds, sizeofcmds = struct.unpack_from("<IIIII", data, 4)
    if cputype != 0x0100000C or filetype != 2:
        return "not an ARM64 executable"
    at, end = 32, 32 + sizeofcmds
    if end > len(data):
        return "the load commands do not fit in the file"
    signature, has_main, text = None, False, None
    for _ in range(ncmds):
        cmd, size = struct.unpack_from("<II", data, at)
        if size < 8 or at + size > end:
            return "a load command has a wrong size"
        if cmd == 0x19:
            segname = data[at + 8:at + 24].rstrip(b"\0")
            vmaddr, vmsize, fileoff, filesize = struct.unpack_from("<QQQQ", data, at + 24)
            if fileoff + filesize > len(data):
                return "segment %s does not fit in the file" % segname.decode()
            if segname == b"__TEXT":
                text = (fileoff, filesize)
        elif cmd == 0x80000028:
            entryoff = struct.unpack_from("<Q", data, at + 8)[0]
            has_main = True
            if text is None or not text[0] <= entryoff < text[0] + text[1]:
                return "the entry point is not inside __TEXT"
        elif cmd == 0x1D:
            signature = struct.unpack_from("<II", data, at + 8)
        at += size
    if not has_main:
        return "no entry point"
    if signature is None:
        return "not signed"
    offset, size = signature
    if offset + size > len(data):
        return "the signature does not fit in the file"
    magic, _, count = struct.unpack_from(">III", data, offset)
    if magic != 0xFADE0CC0 or count < 1:
        return "the signature is not a list of blobs"
    directory = None
    for index in range(count):
        kind, where = struct.unpack_from(">II", data, offset + 12 + index * 8)
        if kind == 0:
            directory = offset + where
    if directory is None or struct.unpack_from(">I", data, directory)[0] != 0xFADE0C02:
        return "the signature has no code directory"
    _, _, _, flags, hash_offset, ident_offset, _, slots, limit = struct.unpack_from(">IIIIIIIII", data, directory)
    hash_size, hash_type, _, page_shift = struct.unpack_from(">BBBB", data, directory + 36)
    if hash_size != 32 or hash_type != 2:
        return "the signature does not use SHA-256"
    if not flags & 2:
        return "the signature is not marked ad hoc"
    if limit != offset:
        return "the signature covers %d bytes, not the %d before it" % (limit, offset)
    page = 1 << page_shift
    if slots != (limit + page - 1) // page:
        return "the signature has %d page hashes for %d bytes" % (slots, limit)
    ident = data[directory + ident_offset:data.index(b"\0", directory + ident_offset)]
    if name is not None and ident != name.encode():
        return "signed for the name %r, not %r" % (ident.decode(errors="replace"), name)
    for slot in range(slots):
        chunk = data[slot * page:min((slot + 1) * page, limit)]
        start = directory + hash_offset + slot * 32
        if hashlib.sha256(chunk).digest() != data[start:start + 32]:
            return "the hash of page %d is wrong" % slot
    return ""


def structure_problem(data, target, name):
    if target == "aarch64-macos":
        return macho_problem(data, name)
    return elf_problem(data, 62 if target == "x86_64-linux" else 183)


# ---- programs -----------------------------------------------------------


def expectation(source, path):
    """The `// expect-...:` comments of a test program."""
    def values(key):
        prefix = "// %s:" % key
        return [line[len(prefix):][1:] if line[len(prefix):].startswith(" ") else line[len(prefix):] for line in source.split("\n") if line.startswith(prefix)]

    lines = values("expect-line")
    if lines:
        output = "".join(line + "\n" for line in lines)
    else:
        listed = values("expect-output")
        if len(listed) != 1:
            raise ValueError("%s needs `expect-line` or one `expect-output` comment" % path)
        output = "".join(part + "\n" for part in listed[0].split())
    exits = values("expect-exit")
    if not exits:
        raise ValueError("%s has no `// expect-exit:` comment" % path)
    stderr = values("expect-stderr")
    return int(exits[0].strip()), output, (stderr[0] + "\n") if stderr else None


def run_program(exe, stdin=b""):
    try:
        done = subprocess.run([exe], input=stdin, capture_output=True, timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        return -999, b"", b"(no answer in time)"
    return done.returncode, done.stdout, done.stderr


def build_and_run(suite, section, path, folder):
    name = os.path.basename(path)[:-4]
    source = read(path).decode("utf-8")
    try:
        want_exit, want_out, want_err = expectation(source, path)
    except ValueError as problem:
        section.fail(str(problem))
        return
    exe = os.path.join(folder, name)
    code, _, err = suite.run(["build", path, "-o", exe])
    if not section.check(code == 0 and os.path.isfile(exe), name + ".azl compiles", text_of(err)):
        return
    if HOST is None:
        return
    code, out, err = run_program(exe)
    section.check(text_of(out) == want_out, name + ".azl: output", difference(want_out.encode(), out))
    section.check(code == want_exit, name + ".azl: exit code", "wanted %d, got %d" % (want_exit, code))
    if want_err is not None:
        section.check(text_of(err) == want_err, name + ".azl: error output", difference(want_err.encode(), err))


def division_program():
    """A program that divides by constants, and what it must print.

    The compiler turns a division by a constant into a multiplication. The
    program compares that with a division by the same number held in a
    variable, and prints a checksum that is worked out here as well. The
    divisors are named constants: that is the spelling that takes the
    multiplication also for negative ones."""
    divisors = [2, 3, 5, 7, 10, 11, 16, 60, 100, 255, 256, 1000, 4096, 65537, 1000000007]
    top, low = 2 ** 63 - 1, -2 ** 63
    divisors += [top, top - 1, 1 << 62, (1 << 62) + 1, 3000000000, 0x123456789ABCDEF1] + list(range(2, 131))
    divisors += [-by for by in divisors]
    dividends = [0, 1, -1, 2, -2, 6, 7, 8, -7, -8, 99, 100, 101, top, low, top - 1, low + 1]
    state, mask = 0x9E3779B97F4A7C15, 2 ** 64 - 1

    def signed(value):
        value &= mask
        return value - 2 ** 64 if value >= 2 ** 63 else value

    for _ in range(120):
        state ^= (state << 13) & mask
        state ^= state >> 7
        state ^= (state << 17) & mask
        dividends += [signed(state), state >> 40, -(state >> 52)]

    def divide(n, by):
        quotient = abs(n) // abs(by)
        if (n < 0) != (by < 0):
            quotient = -quotient
        return quotient, n - quotient * by

    lines = ["fn values() -> List[i64] {", "    var xs: List[i64] = []"]
    lines += ["    xs.push(%d)" % n for n in dividends]
    lines += ["    return xs", "}", ""]
    expected, calls = [], []
    for index, by in enumerate(divisors):
        near = list(dividends)
        for k in (1, 2, 3, 1000, -1, -2, -1000):
            multiple = signed(by * k)
            near += [signed(multiple - 1), multiple, signed(multiple + 1)]
        extra = near[len(dividends):]
        total = 0
        for n in near:
            if n == low and by == -1:
                continue
            quotient, rest = divide(n, by)
            total = signed(total * 31 + signed(quotient ^ rest))
        expected.append(total)
        lines += ["const D%d = %d" % (index, by), "", "fn divide_%d(xs: List[i64], by: i64) -> i64 {" % index, "    var total = 0", "    var wrong = 0", "    var all = xs"]
        lines += ["    all.push(%d)" % n for n in extra]
        lines += [
            "    for n in all {",
            "        let q = n / D%d" % index,
            "        let r = n %% D%d" % index,
            "        if q != n / by || r != n % by {",
            "            wrong = wrong + 1",
            "        }",
            "        total = total * 31 + (q ^ r)",
            "    }",
            "    if wrong != 0 {",
            "        print(\"division by %d differs for \" + str(wrong) + \" numbers\")" % by,
            "    }",
            "    return total",
            "}",
            "",
        ]
        calls.append("    print(divide_%d(xs, %d))" % (index, by) if by > 0 else "    print(divide_%d(xs, 0 - %d))" % (index, -by))
    lines += ["fn main() {", "    let xs = values()"] + calls + ["}", ""]
    return "\n".join(lines), "".join("%d\n" % total for total in expected)


def section_programs(suite):
    section = Section("programs")
    folder = suite.folder("programs")
    programs = sources_in("tests/programs", deep=False)
    section.check(len(programs) >= 30, "tests/programs holds the test programs", "%d found" % len(programs))
    in_parallel(suite, programs, lambda path: build_and_run(suite, section, path, folder))

    # Several files make one program, in any order.
    multi = os.path.join(ROOT, "tests", "multi")
    main = os.path.join(multi, "shop", "main.azl")
    want_exit, want_out, _ = expectation(read(main).decode(), main)
    orders = [["shop/main.azl", "shop/stock.azl", "shop/numbers.azl"], ["shop/numbers.azl", "shop/stock.azl", "shop/main.azl"], ["shop/stock.azl", "shop/main.azl", "shop/numbers.azl"]]
    for at, order in enumerate(orders):
        exe = os.path.join(folder, "shop%d" % at)
        code, _, err = suite.run(["build"] + [os.path.join(multi, name) for name in order] + ["-o", exe])
        if section.check(code == 0, "shop builds in the order %s" % order, text_of(err)) and HOST:
            code, out, _ = run_program(exe)
            section.check(text_of(out) == want_out and code == want_exit, "shop in the order %s: output and exit code" % order, difference(want_out.encode(), out))
    # Without `-o` the executable is named after the first file.
    named = suite.folder("named")
    shutil.copy(main, os.path.join(named, "first.azl"))
    code, _, err = suite.run(["build", os.path.join(named, "first.azl"), os.path.join(multi, "shop", "stock.azl"), os.path.join(multi, "shop", "numbers.azl")])
    section.check(code == 0 and os.path.isfile(os.path.join(named, "first")), "without `-o` the executable is named after the first file", text_of(err))

    # Embedded files are found next to their source, wherever the compiler
    # is started, and the program carries them: it runs where they are not.
    embed = os.path.join(multi, "embed")
    main = os.path.join(embed, "main.azl")
    want_exit, want_out, _ = expectation(read(main).decode(), main)
    elsewhere = suite.folder("elsewhere")
    exe = os.path.join(elsewhere, "embed")
    code, _, err = suite.run(["build", main, os.path.join(embed, "parts", "words.azl"), "-o", exe], cwd=elsewhere)
    if section.check(code == 0, "the program with embedded files builds from another folder", text_of(err)) and HOST:
        code, out, _ = run_program(exe)
        section.check(text_of(out) == want_out and code == want_exit, "embedded files: output and exit code", difference(want_out.encode(), out))
    broken = os.path.join(elsewhere, "broken.azl")
    write(broken, b"const TEXT = embed(\"missing.txt\")\n\nfn main() {\n    print(TEXT)\n}\n")
    code, _, err = suite.run(["check", broken])
    section.check(code == 1 and b"error[E0706]: cannot embed `missing.txt`: No such file or directory" in err and b"broken.azl:1:14" in err,
                  "a file to embed that is missing is an error of the program, with its place", text_of(err))

    # Errors name the file they are in.
    never = os.path.join(folder, "never")

    def failure(files):
        code, _, err = suite.run(["build"] + [os.path.join(multi, name) for name in files] + ["-o", never])
        section.check(code == 1, "%s does not compile" % files, "exit %d" % code)
        section.check(not os.path.exists(never), "%s: nothing is written when the build fails" % files)
        return text_of(err)

    def has(message, files, *parts):
        for part in parts:
            section.check(part in message, "%s: the report contains `%s`" % (files, part), message[:500])

    def lacks(message, files, part):
        section.check(part not in message, "%s: the report does not mention `%s`" % (files, part), message[:500])

    files = ["broken/main.azl", "broken/helper.azl"]
    message = failure(files)
    has(message, files, "error[E0302]: `+` needs `i64` but this is `str`", "broken/helper.azl:5:16", "5 |     return n + text")
    lacks(message, files, "main.azl")
    files = ["broken/helper.azl", "broken/main.azl"]
    has(failure(files), files, "broken/helper.azl:5:16")
    files = ["broken/unclosed_main.azl", "broken/unclosed.azl", "broken/after.azl"]
    message = failure(files)
    has(message, files, "expected `}` to close the block but reached end of file", "broken/unclosed.azl:3:1")
    lacks(message, files, "after.azl")
    files = ["shop/main.azl", "shop/stock.azl", "shop/numbers.azl", "broken/after.azl", "broken/unclosed_main.azl"]
    has(failure(files), files, "function `main` is defined more than once", "broken/unclosed_main.azl:1:4")
    files = ["shop/numbers.azl", "broken/after.azl"]
    has(failure(files), files, "the program has no `main` function", "shop/numbers.azl:1:1")
    files = ["shop/numbers.azl", "broken/missing.azl"]
    has(failure(files), files, "cannot read", "broken/missing.azl")

    # `check` reports every error and writes nothing.
    work = suite.folder("check")
    good, lib, bad = (os.path.join(work, name) for name in ("good.azl", "lib.azl", "bad.azl"))
    write(good, b"fn main() {\n    print(twice(2))\n}\n")
    write(lib, b"fn twice(n: i64) -> i64 {\n    return n * 2\n}\n")
    write(bad, b"fn one() {\n    let a = 1\n    a = 2\n}\n\nfn two() {\n    print(missing)\n}\n\nfn main() {\n}\n")
    code, out, err = suite.run(["check", good, lib])
    section.check((code, out) == (0, b"") and b"no errors" in err, "`check` of a right program says `no errors` on standard error", text_of(err))
    section.check(not os.path.exists(os.path.join(work, "good")), "`check` writes no executable")
    code, out, _ = suite.run(["check", "--json", good, lib])
    section.check((code, out) == (0, b'{"summary":{"errors":0,"files":2}}\n'), "`check --json` of a right program is the summary alone", text_of(out))
    code, out, err = suite.run(["check", bad])
    err = text_of(err)
    section.check((code, out) == (1, b""), "`check` of a wrong program exits with 1 and writes nothing to standard output")
    for part in ("error[E0401]: cannot change `a`: declare it with `var` to make it changeable", "error[E0201]: unknown variable `missing`", "bad.azl:3:5", "bad.azl:7:11"):
        section.check(part in err, "`check` shows both independent errors: `%s`" % part, err)
    section.check(err.endswith("found 2 errors\n"), "`check` counts the errors", err[-200:])
    for command in ("check", "build"):
        code, out, err = suite.run([command, "--json", bad])
        lines = text_of(out).split("\n")[:-1]
        good_shape = code == 1 and b"error" not in err and len(lines) == 3 and all(line.startswith("{") and line.endswith("}") for line in lines)
        if section.check(good_shape, "`%s --json`: one object per line and a summary" % command, text_of(out) + text_of(err)):
            section.check(lines[0].startswith('{"severity":"error","code":"E0401",'), "`%s --json`: the first error" % command, lines[0])
            section.check(lines[0].endswith('"line":2,"column":5,"end_line":2,"end_column":8,"replacement":"var"}}'), "`%s --json`: the fix of the first error" % command, lines[0])
            section.check('"code":"E0201"' in lines[1] and '"line":7,"column":11,"end_line":7,"end_column":18' in lines[1] and '"fix"' not in lines[1], "`%s --json`: the second error, without a fix" % command, lines[1])
            section.check(lines[2] == '{"summary":{"errors":2,"files":1}}', "`%s --json`: the summary" % command, lines[2])
        section.check(not os.path.exists(os.path.join(work, "bad")), "`%s --json` of a wrong program writes no executable" % command)
    exe = os.path.join(work, "good-program")
    code, out, err = suite.run(["build", "--json", good, lib, "-o", exe])
    section.check((code, out) == (0, b'{"summary":{"errors":0,"files":2}}\n') and os.path.isfile(exe), "a build that succeeds says so in JSON and writes the executable", text_of(out) + text_of(err))

    # SHA-256, as the compiler written in azoL computes it for signatures.
    parts = [os.path.join(ROOT, "compiler", name) for name in ("sha256_test.azl", "sha256.azl", "bytes.azl")]
    if all(os.path.isfile(part) for part in parts):
        exe = os.path.join(folder, "sha256_vectors")
        code, _, err = suite.run(["build"] + parts + ["-o", exe])
        if section.check(code == 0, "compiler/sha256_test.azl compiles", text_of(err)) and HOST:
            code, out, _ = run_program(exe)
            section.check((code, out) == (0, b"ok\n"), "SHA-256 gives the known hashes", text_of(out))

    # Division by a constant against real division.
    source, wanted = division_program()
    path = os.path.join(folder, "divide_by_constants.azl")
    write(path, source.encode())
    exe = os.path.join(folder, "divide_by_constants")
    code, _, err = suite.run(["build", path, "-o", exe])
    if section.check(code == 0, "the division program compiles", text_of(err)[:600]) and HOST:
        code, out, _ = run_program(exe)
        section.check(code == 0 and text_of(out) == wanted, "division by a constant equals real division for every divisor", difference(wanted.encode(), out))
    return section


# ---- errors -------------------------------------------------------------

WRONG_AND_RIGHT = ["E0401", "E0402", "E0403", "E0405", "E0407", "E0408", "E0409", "E0410", "E0411", "E0412", "E0413", "E0501", "E0502", "E0503", "E0505", "E0506", "E0508"]


def damaged_inputs():
    """Every beginning of a real program and many small corruptions of it."""
    source = read(os.path.join(ROOT, "examples", "point.azl"))
    text = source.decode("utf-8")
    damaged = [text[:end] for end in range(len(text) + 1)]
    junk = ["{", "}", "(", ")", "[", "]", ",", ":", "=>", "=", "mut", "match", "if", "\n", ".", "-", "fn", "else"]
    seed, mask = 0x9E3779B97F4A7C15, 2 ** 64 - 1
    for _ in range(4000):
        seed ^= (seed << 13) & mask
        seed ^= seed >> 7
        seed ^= (seed << 17) & mask
        at = seed % len(text)
        damaged.append(text[:at] + junk[(seed >> 32) % len(junk)] + text[at:])
    return damaged


def section_errors(suite):
    section = Section("errors")
    count = run_case_files(suite, section, ["errors", os.path.join("golden", "lex"), os.path.join("golden", "parse"), os.path.join("golden", "check")])
    section.check(count >= 400, "the case files are there", "%d cases found" % count)

    # Code nested far too deeply is an error, not a crash.
    folder = suite.folder("deep")
    for name, text in (("parens", "fn main() {\n  let a = %s1%s\n}" % ("(" * 5000, ")" * 5000)), ("minus", "fn main() {\n  let a = %s1\n}" % ("-" * 5000))):
        path = os.path.join(folder, name + ".azl")
        write(path, text.encode())
        for command in ("parse", "check"):
            code, _, err = suite.run([command, path])
            section.check(code == 1 and b"error[E0802]: code is nested too deeply" in err, "5000 nested %s: `%s` reports an error" % (name, command), "exit %d: %s" % (code, text_of(err)[:200]))

    # Damaged input gives a tree or an error, never a crash. `fmt --check`
    # reads and parses every file it is given, many in one run.
    folder = suite.folder("damaged")
    paths = []
    for at, text in enumerate(damaged_inputs()):
        path = os.path.join(folder, "%04d.azl" % at)
        write(path, text.encode("utf-8"))
        paths.append(path)
    batches = [paths[at:at + 250] for at in range(0, len(paths), 250)]

    def survive(batch):
        code, _, err = suite.run(["fmt", "--check"] + batch)
        if code in (0, 1):
            return []
        bad = []
        for path in batch:
            code, _, err = suite.run(["fmt", "--check", path])
            if code not in (0, 1):
                bad.append((os.path.basename(path), code))
        return bad or [("a batch of files", code)]

    crashed = [one for group in in_parallel(suite, batches, survive) for one in group]
    section.check(not crashed, "%d damaged programs are read without a crash" % len(paths), repr(crashed[:5]))
    sample = paths[::40]

    def survive_check(path):
        code, _, _ = suite.run(["check", path])
        return [] if code in (0, 1) else [(os.path.basename(path), code)]

    crashed = [one for group in in_parallel(suite, sample, survive_check) for one in group]
    section.check(not crashed, "%d of them are checked without a crash" % len(sample), repr(crashed[:5]))

    # The error codes.
    code, listed, _ = suite.run(["explain"])
    lines = text_of(listed).split("\n")[:-1]
    section.check(code == 0 and len(lines) > 50, "`explain` lists the codes", "%d lines" % len(lines))
    codes = [line[:5] for line in lines]
    section.check(all(re.fullmatch(r"E0\d\d\d", one) for one in codes), "every code is `E0` and three digits")
    section.check(codes == sorted(set(codes)), "the codes are listed once each, in order")
    section.check(all(line[5:7] == "  " and line.rstrip().endswith(".") for line in lines), "every code has a rule that ends in a full stop")

    def explained(one):
        code, text, _ = suite.run(["explain", one])
        text = text_of(text)
        good = code == 0 and text.startswith(one + ": ") and "Messages with this code:" in text
        if one in WRONG_AND_RIGHT:
            good = good and "Wrong" in text and "Right" in text
        return [] if good else [one]

    unexplained = [one for group in in_parallel(suite, codes, explained) for one in group]
    section.check(not unexplained, "`explain` describes every code", repr(unexplained))
    section.check(all(one in codes for one in WRONG_AND_RIGHT), "the codes of the ownership and `match` rules exist")
    code, text, _ = suite.run(["explain", "E0408"])
    text = text_of(text)
    section.check(code == 0 and "A list cannot be changed while a `for` loop is going through it." in text and "Wrong:" in text and "Right:" in text, "`explain E0408` gives the rule and both examples", text[:300])
    code, out, err = suite.run(["explain", "E9999"])
    section.check((code, out) == (1, b"") and b"there is no error code 'E9999'" in err, "`explain` of an unknown code is an error", text_of(err))
    reference = os.path.join(ROOT, "docs", "errors.md")
    if os.path.isfile(reference):
        code, out, _ = suite.run(["explain", "--markdown"])
        section.check(code == 0 and out == read(reference), "docs/errors.md is what `explain --markdown` prints", difference(read(reference), out))
    return section


# ---- format -------------------------------------------------------------

MAY_CHANGE = ["tests/programs/matching.azl", "tests/programs/moves.azl", "tests/programs/strings.azl", "tests/programs/struct_stress.azl", "tests/programs/text_methods.azl"]
UGLY = b"fn main(){print( 1 )}"
NEAT = b"fn main() {\n    print(1)\n}\n"


def uglier(source):
    """The same program laid out badly."""
    out = []
    for number, line in enumerate(source.split("\n")[:-1] if source.endswith("\n") else source.split("\n")):
        code = line.lstrip(" \t\r\x0b\x0c")
        if code.strip() == "":
            out.append("\n\n")
            continue
        out.append(["\t", "", "       ", "  \t "][number % 4])
        if "//" in code or '"' in code or "'" in code:
            out.append(code)
        else:
            out.append(code.replace(", ", " ,").replace(" {", "{").replace(" = ", "=  "))
            if code.endswith("{"):
                out.append("\n\n")
        out.append("\n")
    return "".join(out)


def comments_of(source):
    """The comments of a source text, in order."""
    found = []
    for line in source.split("\n"):
        at = 0
        while at < len(line):
            c = line[at]
            if c == '"':
                at += 1
                while at < len(line) and line[at] != '"':
                    at += 2 if line[at] == "\\" else 1
            elif c == "'":
                at += 3 if line[at + 1:at + 2] == "\\" else 2
            elif line.startswith("//", at):
                found.append(line[at:].rstrip())
                break
            at += 1
    return found


def tokens_of(listing):
    """Kinds and texts from `azol lex`, without positions and line ends."""
    found = []
    for line in listing.split("\n"):
        parts = line.split(None, 2)
        if len(parts) >= 2 and parts[1] not in ("Newline", "Eof"):
            found.append((parts[1], parts[2] if len(parts) > 2 else ""))
    return found


def section_format(suite):
    section = Section("format")

    def formatted(source):
        return suite.run(["fmt", "-"], stdin=source)

    def first_line(err):
        return text_of(err).split("\n")[0]

    # The pairs of tests/format.
    folder = os.path.join(HERE, "format")
    names = sorted(os.listdir(folder))
    pairs = 0
    for name in names:
        if name.endswith(".input.azl"):
            stem = name[:-len(".input.azl")]
            expected = read(os.path.join(folder, stem + ".expected.azl"))
            code, out, err = formatted(read(os.path.join(folder, name)))
            section.check(code == 0 and out == expected, name + " formats to its expected text", difference(expected, out) + text_of(err))
            code, out, _ = formatted(expected)
            section.check(code == 0 and out == expected, stem + ".expected.azl is a fixed point", difference(expected, out))
            pairs += 1
        elif name.endswith(".expected.azl"):
            section.check(name[:-len(".expected.azl")] + ".input.azl" in names, name + " has an input")
    section.check(pairs >= 9, "tests/format holds the pairs", "%d found" % pairs)
    run_case_files(suite, section, [os.path.join("golden", "format")])

    # In place, `--check`, standard input, usage.
    work = suite.folder("fmt")
    ugly, fine = os.path.join(work, "ugly.azl"), os.path.join(work, "fine.azl")
    write(ugly, UGLY)
    write(fine, NEAT)
    code, out, err = suite.run(["fmt", "--check", fine, ugly])
    section.check(code == 1 and text_of(out) == ugly + "\n" and read(ugly) == UGLY, "`fmt --check` lists the unformatted file and changes nothing", text_of(out) + text_of(err))
    code, out, _ = suite.run(["fmt", "--check", fine])
    section.check((code, out) == (0, b""), "`fmt --check` of formatted files says nothing")
    code, out, err = suite.run(["fmt", ugly, fine])
    section.check((code, out, err) == (0, b"", b"") and read(ugly) == NEAT and read(fine) == NEAT, "`fmt` rewrites files in place, silently", text_of(err))
    code, out, _ = formatted(UGLY)
    section.check((code, out) == (0, NEAT), "`fmt -` formats standard input to standard output")
    for args in (["fmt"], ["fmt", "--check"], ["fmt", "--unknown", "a.azl"]):
        code, _, err = suite.run(args, cwd=work)
        section.check(code == 2 and b"azol fmt --check <files...>" in err, "`azol %s` prints the usage" % " ".join(args), "exit %d" % code)

    # A file that does not parse is left untouched.
    broken = os.path.join(work, "broken.azl")
    source = b"fn main(){\n  let = 1\n     }"
    write(broken, source)
    write(ugly, UGLY)
    _, _, parser = suite.run(["parse", broken])
    section.check("expected a variable name" in first_line(parser), "the broken file is refused by the parser", first_line(parser))
    code, _, err = suite.run(["fmt", broken, ugly])
    section.check(code == 1 and first_line(err) == first_line(parser) and b"broken.azl:2:" in err, "`fmt` reports the parser's error", text_of(err))
    section.check(read(broken) == source, "`fmt` leaves the broken file untouched")
    section.check(read(ugly) == NEAT, "the other file of the same run is still formatted")
    code, out, err = suite.run(["fmt", "--check", broken])
    section.check(code == 1 and first_line(err) == first_line(parser) and out == b"", "`fmt --check` reports the parser's error", text_of(err))
    code, out, err = formatted(source)
    section.check(code == 1 and first_line(err) == first_line(parser) and out == b"", "`fmt -` reports the parser's error and prints nothing", text_of(err))

    # The repository's own files.
    paths = []
    for one in ("tests/programs", "tests/multi", "examples", "compiler", "src"):
        paths.extend(sources_in(one))
    section.check(len(paths) > 50, "the repository's source files are found", "%d found" % len(paths))
    code, out, err = suite.run(["fmt", "--check"] + paths)
    listed = set(os.path.relpath(line, ROOT).replace(os.sep, "/") for line in text_of(out).split("\n") if line)
    section.check(listed <= set(MAY_CHANGE), "the repository's files are already formatted", "azol fmt would change %s" % sorted(listed - set(MAY_CHANGE)))
    errors = [line for line in text_of(err).split("\n") if line.startswith("error")]
    section.check(len(errors) == 1 and "tests/multi/broken/unclosed.azl" in text_of(err), "the one file that is refused is the test of a broken program", text_of(err)[:400])

    def guarantees(path):
        """Formatting keeps tokens, comments and tree; twice is once; no stray space."""
        problems = []
        name = os.path.relpath(path, ROOT)
        source = read(path)
        code, neat, _ = formatted(source)
        if code != 0:
            return [] if code == 1 else [(name + ": `fmt -` ends with %d" % code, "")]
        code, again, _ = formatted(neat)
        if again != neat:
            problems.append((name + ": formatting twice is formatting once", difference(neat, again)))
        text = neat.decode("utf-8")
        stray = [line for line in text.split("\n") if line != line.rstrip()]
        if stray or (text and not text.endswith("\n")) or text.endswith("\n\n") or text.startswith("\n") or "\n\n\n" in text or "\t" in "".join(line[:len(line) - len(line.lstrip())] for line in text.split("\n")):
            problems.append((name + ": the output has no stray whitespace", repr(stray[:2])))
        work = suite.folder("keep")
        before, after = os.path.join(work, "before.azl"), os.path.join(work, "after.azl")
        write(before, source)
        write(after, neat)
        for command in ("lex", "parse"):
            _, one, _ = suite.run([command, before])
            _, two, _ = suite.run([command, after])
            if command == "lex":
                one, two = tokens_of(text_of(one)), tokens_of(text_of(two))
            if one != two:
                problems.append((name + ": formatting keeps the %s" % ("tokens" if command == "lex" else "syntax tree"), ""))
        if comments_of(source.decode("utf-8")) != comments_of(text):
            problems.append((name + ": formatting keeps the comments", ""))
        shutil.rmtree(work, ignore_errors=True)
        return problems

    everything = paths + sources_in("bench", deep=False) + [os.path.join(folder, name) for name in names if name.endswith(".azl")]
    for group in in_parallel(suite, everything, guarantees):
        for label, detail in group:
            section.fail(label, detail)
    section.checks += 4 * len(everything)

    # Formatting does not change the executable.
    programs = sources_in("tests/programs", deep=False)

    def same_executable(program):
        problems, changed = [], 0
        name = os.path.basename(program)
        work = suite.folder("exe")
        source = read(program).decode("utf-8")
        _, neat_source, _ = formatted(source.encode())
        for variant, text in (("as written", source), ("made ugly", uglier(source))):
            code, neat, err = formatted(text.encode())
            if code != 0:
                problems.append(("%s (%s) formats" % (name, variant), text_of(err)[:300]))
                continue
            changed += neat != text.encode()
            if variant == "made ugly" and neat != neat_source:
                problems.append(("%s: the ugly spelling formats like the original" % name, difference(neat_source, neat)))
            builds = []
            for data in (text.encode(), neat):
                write(os.path.join(work, name), data)
                for target in ("aarch64-macos", "x86_64-linux"):
                    exe = os.path.join(work, "program")
                    code, _, err = suite.run(["build", "--target", target, os.path.join(work, name), "-o", exe])
                    builds.append(read(exe) if code == 0 and os.path.isfile(exe) else text_of(err).encode())
                    if os.path.exists(exe):
                        os.remove(exe)
            if builds[:2] != builds[2:]:
                problems.append(("%s (%s): formatting does not change the executable" % (name, variant), ""))
        shutil.rmtree(work, ignore_errors=True)
        return problems, changed

    total = 0
    for problems, changed in in_parallel(suite, programs, same_executable):
        total += changed
        for label, detail in problems:
            section.fail(label, detail)
    section.checks += 2 * len(programs)
    section.check(total >= len(programs), "the formatter had something to change in the ugly spellings", "%d inputs changed" % total)
    return section


# ---- cross --------------------------------------------------------------

GOLDEN = os.path.join(HERE, "golden", "executables.txt")


def cross_programs(suite):
    """(name, source files) of every program built for all targets."""
    found = []
    for folder in ("tests/programs", "examples", "bench"):
        for path in sources_in(folder, deep=False):
            # An example may show syntax that is not built yet (examples/point.azl
            # is there for `azol parse`). A program that stops passing `check`
            # is still noticed: its lines in executables.txt are left over.
            if folder == "examples" and suite.run(["check", path])[0] != 0:
                continue
            found.append((os.path.relpath(path, ROOT).replace(os.sep, "/"), [path]))
    shop = os.path.join(ROOT, "tests", "multi", "shop")
    found.append(("tests/multi/shop", [os.path.join(shop, name) for name in ("main.azl", "stock.azl", "numbers.azl")]))
    embed = os.path.join(ROOT, "tests", "multi", "embed")
    found.append(("tests/multi/embed", [os.path.join(embed, "main.azl"), os.path.join(embed, "parts", "words.azl")]))
    return found


def section_cross(suite):
    section = Section("cross")
    wanted = {}
    if os.path.isfile(GOLDEN):
        for line in read(GOLDEN).decode().split("\n"):
            parts = line.split()
            if len(parts) == 3 and not line.startswith("#"):
                wanted[(parts[1], parts[2])] = parts[0]
    jobs = [(name, files, target) for name, files in cross_programs(suite) for target in TARGETS]

    def build(job):
        name, files, target = job
        work = suite.folder("cross")
        exe = os.path.join(work, "program")
        code, _, err = suite.run(["build", "--target", target] + files + ["-o", exe])
        if code != 0 or not os.path.isfile(exe):
            shutil.rmtree(work, ignore_errors=True)
            return None, text_of(err)[:400]
        data = read(exe)
        shutil.rmtree(work, ignore_errors=True)
        return hashlib.sha256(data).hexdigest(), structure_problem(data, target, "program")

    results = in_parallel(suite, jobs, build)
    lines = ["# SHA-256 of the executable of every program for every target, each built as `program`.", "# Rewritten by `python3 tests/run.py --update-golden cross`."]
    for (name, files, target), (digest, problem) in zip(jobs, results):
        label = "%s for %s" % (name, target)
        if not section.check(digest is not None, label + " builds", problem):
            continue
        section.check(not problem, label + ": a sound executable", problem)
        lines.append("%s %s %s" % (digest, target, name))
        if not suite.update:
            known = wanted.get((target, name))
            section.check(known == digest, label + ": the executable is the expected one", "no line for it in tests/golden/executables.txt" if known is None else "the generated code changed; if that is intended, run `python3 tests/run.py --update-golden cross`")
    if suite.update:
        write(GOLDEN, ("\n".join(lines) + "\n").encode())
        print("rewrote " + os.path.relpath(GOLDEN, ROOT))
    else:
        built = set((target, name) for name, _, target in jobs)
        section.check(set(wanted) <= built, "every line of executables.txt names a program that still builds", repr(sorted(set(wanted) - built)[:3]))
    return section


# ---- self ---------------------------------------------------------------


def section_self(suite):
    section = Section("self")
    script = os.path.join(ROOT, "build.sh")
    if HOST is None or not os.path.isfile(script):
        section.fail("the compiler can build itself on this machine", "no build.sh, or an unknown machine")
        return section, []
    out = os.path.join(suite.tmp, "bootstrap")
    os.makedirs(out)
    done = subprocess.run(["sh", script, "--with", suite.azol, "--keep", out, "-o", os.path.join(out, "azol")], capture_output=True, timeout=1800)
    answer = text_of(done.stdout) + text_of(done.stderr)
    stage = os.path.join(out, "2", "azol")
    if not section.check(done.returncode == 0 and "FIXED POINT" in answer and os.path.isfile(stage), "the compiler builds itself, and the result builds itself to the same file", answer[-600:]):
        return section, []
    inner = Suite(stage, suite.jobs, False)
    sections = []
    try:
        for name in ("programs", "errors", "format", "cross"):
            one = RUN[name](inner)
            one.seconds = time.time() - one.started
            one.name = "self/" + name
            sections.append(one)
            section.check(not one.failures, "the compiler built by itself passes `%s`" % name, "%d checks failed" % len(one.failures))
    finally:
        inner.close()
    return section, sections


RUN = {"programs": section_programs, "errors": section_errors, "format": section_format, "cross": section_cross}


def main(args):
    azol, jobs, update, chosen = "./azol", os.cpu_count() or 1, False, []
    at = 0
    while at < len(args):
        arg = args[at]
        if arg == "--azol" and at + 1 < len(args):
            azol = args[at + 1]
            at += 1
        elif arg == "--jobs" and at + 1 < len(args):
            jobs = max(1, int(args[at + 1]))
            at += 1
        elif arg == "--update-golden":
            update = True
        elif arg in SECTIONS:
            chosen.append(arg)
        else:
            sys.exit("usage: python3 tests/run.py [--azol PATH] [--jobs N] [--update-golden] [%s ...]" % " | ".join(SECTIONS))
        at += 1
    if not os.path.isfile(azol):
        sys.exit("there is no compiler at '%s'; give one with --azol PATH" % azol)
    chosen = chosen or (["errors", "format", "cross"] if update else SECTIONS)
    suite = Suite(azol, jobs, update)
    done = []
    try:
        for name in SECTIONS:
            if name not in chosen:
                continue
            if name == "self":
                section, inner = section_self(suite)
                done.extend(inner)
                done.append(section)
            else:
                done.append(RUN[name](suite))
            done[-1].seconds = time.time() - done[-1].started
    finally:
        suite.close()
    failed = False
    for section in done:
        for label, detail in section.failures[:40]:
            print("FAILED  %s: %s" % (section.name, label))
            for line in str(detail).rstrip().split("\n")[:12]:
                if line:
                    print("        " + line)
        if len(section.failures) > 40:
            print("        ... and %d more in `%s`" % (len(section.failures) - 40, section.name))
        failed |= bool(section.failures)
    for section in done:
        print(section.summary())
    if update:
        print("the expected results were rewritten; look at `git diff` before committing them")
    print("SOME TESTS FAILED" if failed else "ALL TESTS PASSED")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
