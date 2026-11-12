#!/usr/bin/env python3
"""Writes compiler/runtime_x64.azl from src/runtime_x64.rs.

The compiler written in azoL needs the same facts about the Linux x86-64
runtime as the Rust one: the ids of its routines, where each starts, and
where the runtime sits in an executable. They are generated, so they are
taken from the generated Rust module instead of being typed again.

    python3 compiler/gen_runtime_azl.py           write the file
    python3 compiler/gen_runtime_azl.py --check   fail if the file is stale
"""
import os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
SOURCE = os.path.join(os.path.dirname(HERE), "src", "runtime_x64.rs")
OUT = os.path.join(HERE, "runtime_x64.azl")


def generate():
    text = open(SOURCE, encoding="utf-8").read()
    ids = re.findall(r"^pub const ([A-Z0-9_]+): u32 = (\d+);$", text, re.M)
    routines = [(name, int(value)) for name, value in ids if name != "START"]
    count = int(re.search(r"ENTRY_COUNT: usize = (\d+);", text).group(1))
    if [value for _, value in routines] != list(range(count)):
        sys.exit("gen_runtime_azl.py: the routine ids are not 0, 1, 2, ...")
    entries = [int(n) for n in re.search(r"ENTRIES: \[u32; ENTRY_COUNT\] = \[([^\]]*)\]", text).group(1).split(",")]
    if len(entries) != count:
        sys.exit("gen_runtime_azl.py: ENTRIES does not have ENTRY_COUNT numbers")

    def number(name, kind):
        return int(re.search(r"pub const %s: %s = (0x[0-9A-Fa-f]+|\d+);" % (name, kind), text).group(1), 0)

    size = int(re.search(r"TEXT: &\[u8; (\d+)\]", text).group(1))
    lines = [
        "// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi",
        "",
        "// The Linux x86-64 runtime as the compiler written in azoL sees it:",
        "// the ids of its routines, where each starts, and where the runtime",
        "// sits in an executable. Generated from `src/runtime_x64.rs` by",
        "// `compiler/gen_runtime_azl.py`; edit neither, but the runtime's",
        "// source in `runtime/linux_x64`.",
        "",
        "// Ids of the routines, shared with the code generator.",
    ]
    lines += ["const RT_%s = %d" % (name, value) for name, value in routines]
    lines += [
        "",
        "// Address the executable file is mapped at.",
        "const RT_BASE = %d" % number("BASE", "u64"),
        "// Where in the file, and so after `RT_BASE`, the runtime starts.",
        "const RT_TEXT_OFFSET = %d" % number("TEXT_OFFSET", "u64"),
        "// Size of the runtime's code and constant data: the file",
        "// `runtime/linux_x64/runtime.bin`.",
        "const RT_TEXT_SIZE = %d" % size,
        "// Offset in it of the code the system starts the process at.",
        "const RT_START = %d" % number("START", "u32"),
        "// Address and size of the runtime's writable memory.",
        "const RT_MEMORY = %d" % number("MEMORY", "u64"),
        "const RT_MEMORY_SIZE = %d" % number("MEMORY_SIZE", "u64"),
        "",
        "// Offset in the runtime of each routine, indexed by its id.",
        "fn rt_entries() -> List[i64] {",
    ]
    rows = [entries[at:at + 12] for at in range(0, len(entries), 12)]
    for at, row in enumerate(rows):
        start = "    return [" if at == 0 else "        "
        end = "]" if at == len(rows) - 1 else ","
        lines.append(start + ", ".join(str(n) for n in row) + end)
    lines += ["}", ""]
    return "\n".join(lines)


def main():
    text = generate()
    if "--check" in sys.argv[1:]:
        if not os.path.exists(OUT) or open(OUT, encoding="utf-8").read() != text:
            sys.exit("compiler/runtime_x64.azl is stale: python3 compiler/gen_runtime_azl.py")
        return
    with open(OUT, "w", encoding="utf-8", newline="") as out:
        out.write(text)


if __name__ == "__main__":
    main()
