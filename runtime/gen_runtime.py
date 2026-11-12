#!/usr/bin/env python3
"""Assembles runtime.s and writes ../src/runtime.rs.

Needs clang for arm64 macOS (for example `pip install ziglang`) and the
`lief` and `capstone` Python packages. Run it after editing runtime.s.
"""
import os, struct, subprocess, sys, tempfile
import capstone, lief

HERE = os.path.dirname(os.path.abspath(__file__))
IMPORTS = [
    "_write", "_exit", "_malloc", "_realloc", "_free", "_memcpy", "_memcmp", "_open", "_read", "_close", "___error",
    "_strerror", "_strlen", "__NSGetArgc", "__NSGetArgv", "_chmod", "_sigaltstack", "_sigaction", "__exit",
]
TEXT = [
    (None, b"\n"),
    (None, b"true"),
    (None, b"false"),
    ("LEN_DIV_ZERO", b"azol: division by zero\n"),
    ("LEN_INDEX", b"azol: index out of range\n"),
    ("LEN_POP", b"azol: pop from an empty list\n"),
    ("LEN_MEMORY", b"azol: out of memory\n"),
    ("LEN_SLICE", b"azol: slice out of range\n"),
    ("LEN_CHR", b"azol: chr needs a number from 0 to 255\n"),
    (None, bytes(range(256))),
    ("LEN_STACK", b"azol: stack overflow\n"),
]
ENTRIES = [
    "print_i64", "print_bool", "print_str", "div_zero", "index_fail", "pop_empty", "alloc", "free", "list_push",
    "list_clone", "drop", "str_concat", "str_eq", "str_from_i64", "str_from_bool", "eprint_str", "exit", "str_slice",
    "chr", "args", "read_file", "write_file", "str_append", "set_executable",
]

source = open(os.path.join(HERE, "runtime.s")).read()
for name, text in TEXT:
    if name:
        source = source.replace("#" + name, "#%d" % len(text))
with tempfile.TemporaryDirectory() as tmp:
    asm, obj = os.path.join(tmp, "rt.s"), os.path.join(tmp, "rt.o")
    open(asm, "w").write(source)
    subprocess.run([sys.executable, "-m", "ziglang", "cc", "-target", "aarch64-macos.11.0", "-c", asm, "-o", obj], check=True)
    binary = lief.MachO.parse(obj).at(0)
    section = [s for s in binary.sections if s.name == "__text"][0]
    code = bytes(section.content)
    symbols = {s.name: s.value // 4 for s in binary.symbols if s.name.startswith("_rt_")}
    assert not list(section.relocations), "the runtime must not need relocations"

words = struct.unpack("<%dI" % (len(code) // 4), code)
disasm = list(capstone.Cs(capstone.CS_ARCH_ARM64, capstone.CS_MODE_ARM).disasm(code, 0))
assert len(disasm) == len(words)
relocs, lines = [], []
for i, (word, ins) in enumerate(zip(words, disasm)):
    if word & 0xFFE0001F == 0xD4200000:
        imm = (word >> 5) & 0xFFFF
        if imm == 0x800:
            assert i == symbols["_rt_start_main"]
        elif imm >= 0x2000:
            relocs.append("    RuntimeReloc::Data { at: %d, data: %d, reg: %d }," % (i, (imm - 0x2000) // 32, (imm - 0x2000) % 32))
        elif imm >= 0x1000:
            relocs.append("    RuntimeReloc::Import { at: %d, import: %d }," % (i, imm - 0x1000))
    lines.append("    0x%08X, // %3d: %s %s" % (word, i, ins.mnemonic, ins.op_str))

offsets, at = [], 0
for _, text in TEXT:
    offsets.append(at)
    at += len(text)
data = b"".join(text for _, text in TEXT)
escaped = "".join(chr(b) if 32 <= b < 127 and chr(b) not in '"\\' else "\\n" if b == 10 else "\\x%02x" % b for b in data)

out = """// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! The runtime: a few hand-written ARM64 routines appended to every
//! program. This file is generated from `runtime/runtime.s` by
//! `runtime/gen_runtime.py`; edit those, not this. Places that call an
//! imported function or take the address of runtime text are markers the
//! linker rewrites.

/// Functions imported from the system library, in GOT order.
pub const IMPORTS: [&str; %d] = [%s];

/// Text the runtime prints. `RUNTIME_DATA_OFFSETS[n]` is where item `n`
/// starts; program string literals are placed after it.
pub const RUNTIME_DATA: &[u8] = b"%s";
pub const RUNTIME_DATA_OFFSETS: [u32; %d] = [%s];

/// Entry points, as word offsets into `RUNTIME`.
%s

/// Where a macOS program starts, and the word in it that the linker turns
/// into the call of the program's `main`.
pub const START: u32 = %d;
pub const START_MAIN: u32 = %d;

pub enum RuntimeReloc {
    /// Three words: load the import's address and call it.
    Import { at: u32, import: u32 },
    /// Two words: put the address of a runtime text item in `reg`.
    Data { at: u32, data: u32, reg: u32 },
}

pub const RUNTIME_RELOCS: [RuntimeReloc; %d] = [
%s
];

pub const RUNTIME: [u32; %d] = [
%s
];
""" % (
    len(IMPORTS), ", ".join('"%s"' % name for name in IMPORTS),
    escaped, len(offsets), ", ".join(map(str, offsets)),
    "\n".join("pub const %s: u32 = %d;" % (name.upper(), symbols["_rt_" + name]) for name in ENTRIES),
    symbols["_rt_start"], symbols["_rt_start_main"],
    len(relocs), "\n".join(relocs), len(words), "\n".join(lines),
)
open(os.path.join(HERE, "..", "src", "runtime.rs"), "w").write(out)
print("wrote src/runtime.rs: %d words, %d relocations" % (len(words), len(relocs)))
