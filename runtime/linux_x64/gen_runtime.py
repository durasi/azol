#!/usr/bin/env python3
"""Compiles runtime.c and writes runtime.bin and runtime.facts.

runtime.facts says where everything is in runtime.bin, in the notation of
the first compiler's tables. compiler/gen_runtime_azl.py makes
compiler/runtime_x64.azl from it.

Needs clang and lld for x86-64 (for example `pip install ziglang`) and the
`lief` Python package. Run it after editing runtime.c or runtime.ld.
"""
import os, subprocess, sys, tempfile
import lief

HERE = os.path.dirname(os.path.abspath(__file__))
BASE = 0x400000
TEXT_OFFSET = 0x100
MEMORY = 0x40000000
# In the order of the entry ids, which the code generators share.
ENTRIES = [
    "print_i64", "print_bool", "print_str", "div_zero", "index_fail", "pop_empty", "alloc", "free", "list_push",
    "list_clone", "drop", "str_concat", "str_eq", "str_from_i64", "str_from_bool", "eprint_str", "exit", "str_slice",
    "chr", "args", "read_file", "write_file", "str_append", "set_executable", "remove_file", "rename_file", "clock",
    "str_is_utf8",
]
FLAGS = [
    # General-purpose registers only: no vector instructions, so the code
    # makes no demands on how the stack is aligned.
    "-target", "x86_64-freestanding", "-mcpu=x86_64-sse-sse2-mmx+soft_float", "-O2", "-ffreestanding", "-nostdlib",
    "-fno-builtin", "-mno-red-zone", "-fno-stack-protector", "-fno-pic", "-fno-asynchronous-unwind-tables",
    "-fno-unwind-tables", "-fno-sanitize=all", "-Wall", "-Wextra", "-Werror",
]
ZIG = [sys.executable, "-m", "ziglang"]

with tempfile.TemporaryDirectory() as tmp:
    obj, elf = os.path.join(tmp, "runtime.o"), os.path.join(tmp, "runtime.elf")
    subprocess.run(ZIG + ["cc"] + FLAGS + ["-c", os.path.join(HERE, "runtime.c"), "-o", obj], check=True)
    subprocess.run(
        ZIG + ["ld.lld", "-static", "--no-dynamic-linker", "--build-id=none", "-z", "norelro",
               "-T", os.path.join(HERE, "runtime.ld"), obj, "-o", elf],
        check=True,
    )
    binary = lief.ELF.parse(elf)
    text = [s for s in binary.sections if s.name == ".text"][0]
    memory = [s for s in binary.sections if s.name == ".bss"]
    symbols = {s.name: s.value for s in binary.symbols if s.name}
    code = bytes(text.content)
    for section in binary.sections:
        # Everything the runtime needs at run time is in these two.
        loaded = section.has(lief.ELF.Section.FLAGS.ALLOC)
        assert not loaded or section.name in (".text", ".bss"), "unexpected section " + section.name
    assert text.virtual_address == BASE + TEXT_OFFSET
    assert symbols["azol_program"] == BASE + TEXT_OFFSET + len(code) and len(code) % 16 == 0
    if memory:
        assert memory[0].type == lief.ELF.Section.TYPE.NOBITS, "the runtime must not have initialised writable data"
        assert memory[0].virtual_address == MEMORY
    memory_size = memory[0].size if memory else 0
    assert memory_size <= 4096

open(os.path.join(HERE, "runtime.bin"), "wb").write(code)
offset = lambda name: symbols[name] - (BASE + TEXT_OFFSET)
out = """// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! The Linux x86-64 runtime: the routines every compiled program calls,
//! and the code that starts it. This file and `runtime.bin` are generated
//! from `runtime/linux_x64/runtime.c` by `runtime/linux_x64/gen_runtime.py`;
//! edit those, not this.
//!
//! The runtime is machine code linked for fixed addresses: it sits at
//! `BASE + TEXT_OFFSET` in every executable, the compiled program follows
//! it directly, and its writable memory is at `MEMORY`. Its routines use
//! the System V calling convention.

#![allow(dead_code)]

/// Ids of the routines, shared with the code generator.
%s
/// Number of routines.
pub const ENTRY_COUNT: usize = %d;

/// Address the executable file is mapped at.
pub const BASE: u64 = 0x%X;
/// Where in the file, and so after `BASE`, the runtime starts.
pub const TEXT_OFFSET: u64 = 0x%X;
/// The runtime's code and constant data. The program's code goes right
/// after it, starting with a jump to the program's `main`.
pub const TEXT: &[u8; %d] = include_bytes!("../runtime/linux_x64/runtime.bin");
/// Offset in `TEXT` of each routine, indexed by its id.
pub const ENTRIES: [u32; ENTRY_COUNT] = [%s];
/// Offset in `TEXT` of the code the system starts the process at.
pub const START: u32 = %d;
/// Address and size of the runtime's writable memory, which must start
/// out as zeros.
pub const MEMORY: u64 = 0x%X;
pub const MEMORY_SIZE: u64 = %d;
""" % (
    "\n".join("pub const %s: u32 = %d;" % (name.upper(), i) for i, name in enumerate(ENTRIES)),
    len(ENTRIES), BASE, TEXT_OFFSET, len(code),
    ", ".join(str(offset("rt_" + name)) for name in ENTRIES),
    offset("_start"), MEMORY, memory_size,
)
open(os.path.join(HERE, "runtime.facts"), "w").write(out)
print("wrote runtime.bin (%d bytes) and runtime.facts; writable memory %d bytes; now run compiler/gen_runtime_azl.py" % (len(code), memory_size))
