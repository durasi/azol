#!/usr/bin/env python3
"""Writes compiler/runtime_x64.azl from runtime/linux_x64/runtime.facts,
and compiler/runtime_arm64.azl from runtime/runtime.facts and
runtime/linux_arm64/runtime.facts.

The compiler needs facts about each runtime: the ids of its routines,
where each starts, and where the runtime sits in an executable. The
`gen_runtime.py` scripts in runtime/ write them down next to the machine
code, as `runtime.facts`; this script turns those into azoL.

    python3 compiler/gen_runtime_azl.py           write the files
    python3 compiler/gen_runtime_azl.py --check   fail if a file is stale
"""
import os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
SOURCE = os.path.join(os.path.dirname(HERE), "runtime", "linux_x64", "runtime.facts")
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
        "// sits in an executable. Generated from `runtime/linux_x64/runtime.facts` by",
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


def number_rows(numbers, per, first, form="%d"):
    """A list literal of numbers, `per` to a line; `first` starts it."""
    rows = [numbers[at:at + per] for at in range(0, len(numbers), per)]
    lines = []
    for at, row in enumerate(rows):
        start = first if at == 0 else "        "
        end = "]" if at == len(rows) - 1 else ","
        lines.append(start + ", ".join(form % n for n in row) + end)
    return lines


def generate_arm64():
    """The macOS runtime (machine code the generator appends to every
    program) and the facts about the Linux ARM64 library."""
    root = os.path.dirname(HERE)
    text = open(os.path.join(root, "runtime", "runtime.facts"), encoding="utf-8").read()
    imports = re.findall(r'"([^"]+)"', re.search(r"pub const IMPORTS: \[&str; \d+\] = \[([^\]]*)\];", text).group(1))
    offsets = [int(n) for n in re.search(r"RUNTIME_DATA_OFFSETS: \[u32; \d+\] = \[([^\]]*)\];", text).group(1).split(",")]
    data = re.search(r'pub const RUNTIME_DATA: &\[u8\] = b"(.*)";', text).group(1)
    data_size = len(data.encode("latin-1").decode("unicode_escape").encode("latin-1"))
    ids = re.findall(r"^pub const ([A-Z0-9_]+): u32 = (\d+);$", text, re.M)
    relocs = []
    for kind, a, b, c in re.findall(r"RuntimeReloc::(Import|Data) \{ at: (\d+), (?:import|data): (\d+)(?:, reg: (\d+))? \}", text):
        relocs += [0 if kind == "Import" else 1, int(a), int(b), int(c or 0)]
    count = int(re.search(r"RUNTIME_RELOCS: \[RuntimeReloc; (\d+)\]", text).group(1))
    if len(relocs) != 4 * count:
        sys.exit("gen_runtime_azl.py: RUNTIME_RELOCS was not read completely")
    body = text[text.index("pub const RUNTIME: [u32;"):]
    words = [int(word, 16) for word in re.findall(r"^\s+0x([0-9A-Fa-f]{8}),", body, re.M)]
    if len(words) != int(re.search(r"pub const RUNTIME: \[u32; (\d+)\]", text).group(1)):
        sys.exit("gen_runtime_azl.py: RUNTIME was not read completely")

    linux = open(os.path.join(root, "runtime", "linux_arm64", "runtime.facts"), encoding="utf-8").read()

    def number(name, kind):
        return int(re.search(r"pub const %s: %s = (0x[0-9A-Fa-f]+|\d+);" % (name, kind), linux).group(1), 0)

    import_offsets = [int(n) for n in re.search(r"IMPORT_OFFSETS: \[u32; \d+\] = \[([^\]]*)\];", linux).group(1).split(",")]
    names = re.findall(r'"([^"]+)"', re.search(r"pub const IMPORT_NAMES: \[&str; \d+\] = \[([^\]]*)\];", linux).group(1))
    if names != imports:
        sys.exit("gen_runtime_azl.py: the Linux ARM64 library does not provide the imports of the macOS runtime")
    size = int(re.search(r"TEXT: &\[u8; (\d+)\]", linux).group(1))

    lines = [
        "// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi",
        "",
        "// The ARM64 runtime as the compiler written in azoL sees it: the",
        "// machine code that follows every program's own, where each of its",
        "// routines starts, what the linker completes in it, and the facts",
        "// about the library that stands in for the system's on Linux.",
        "// Generated from `runtime/runtime.facts` and `runtime/linux_arm64/runtime.facts` by",
        "// `compiler/gen_runtime_azl.py`; edit neither, but the sources in",
        "// `runtime/`.",
        "",
        "// Where each routine starts in the runtime, in instructions.",
    ]
    lines += ["const ART_%s = %s" % (name, value) for name, value in ids]
    lines += [
        "",
        "// How many instructions the runtime is.",
        "const ART_WORDS = %d" % len(words),
        "// How many bytes its text is; the text itself is in `embedded.azl`.",
        "const ART_DATA_SIZE = %d" % data_size,
        "",
        "// The functions imported from the system library, in the order of",
        "// their slots.",
        "fn art_imports() -> List[str] {",
    ]
    lines += number_rows(imports, 6, "    return [", '"%s"')
    lines += [
        "}",
        "",
        "// Where each item of the runtime's text starts in it.",
        "fn art_data_offsets() -> List[i64] {",
    ]
    lines += number_rows(offsets, 12, "    return [")
    lines += [
        "}",
        "",
        "// What the linker completes in the runtime, four numbers each: 0 and",
        "// the instruction, the import and nothing for a call of an imported",
        "// function; 1 and the instruction, the text item and the register for",
        "// the address of a text item.",
        "fn art_relocs() -> List[i64] {",
    ]
    lines += number_rows(relocs, 16, "    return [")
    lines += [
        "}",
        "",
        "// The runtime's instructions.",
        "fn art_words() -> List[i64] {",
    ]
    lines += number_rows(words, 8, "    return [", "0x%08X")
    lines += [
        "}",
        "",
        "// Linux: address the executable file is mapped at.",
        "const ALR_BASE = %d" % number("BASE", "u64"),
        "// Where in the file, and so after `ALR_BASE`, the library starts.",
        "const ALR_TEXT_OFFSET = %d" % number("TEXT_OFFSET", "u64"),
        "// Size of the library: the file `runtime/linux_arm64/runtime.bin`.",
        "const ALR_TEXT_SIZE = %d" % size,
        "// Offset in it of the code the system starts the process at.",
        "const ALR_START = %d" % number("START", "u32"),
        "// Address and size of the library's writable memory.",
        "const ALR_MEMORY = %d" % number("MEMORY", "u64"),
        "const ALR_MEMORY_SIZE = %d" % number("MEMORY_SIZE", "u64"),
        "",
        "// Offset in the library of each imported function, in the order of",
        "// `art_imports`.",
        "fn alr_import_offsets() -> List[i64] {",
    ]
    lines += number_rows(import_offsets, 12, "    return [")
    lines += ["}", ""]
    return "\n".join(lines)


def main():
    made = [(OUT, "runtime_x64.azl", generate()), (os.path.join(HERE, "runtime_arm64.azl"), "runtime_arm64.azl", generate_arm64())]
    for path, name, text in made:
        if "--check" in sys.argv[1:]:
            if not os.path.exists(path) or open(path, encoding="utf-8").read() != text:
                sys.exit("compiler/%s is stale: python3 compiler/gen_runtime_azl.py" % name)
        else:
            with open(path, "w", encoding="utf-8", newline="") as out:
                out.write(text)


if __name__ == "__main__":
    main()
