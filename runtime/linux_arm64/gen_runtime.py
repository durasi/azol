#!/usr/bin/env python3
"""Compiles runtime.c and writes runtime.bin and ../../src/runtime_linux_arm64.rs.

Needs clang and lld for ARM64 (for example `pip install ziglang`) and the
`lief` Python package. Run it after editing runtime.c, runtime.ld or the
files in ../linux_common.
"""
import os, re, subprocess, sys, tempfile
import lief

HERE = os.path.dirname(os.path.abspath(__file__))
BASE = 0x400000
TEXT_OFFSET = 0x100
MEMORY = 0x40000000
# The function that stands in for each import of the ARM64 code, in the
# order of `IMPORTS` in src/runtime.rs.
IMPORTS = [
    ("_write", "az_write"), ("_exit", "az_exit"), ("_malloc", "az_malloc"), ("_realloc", "az_realloc"),
    ("_free", "az_free"), ("_memcpy", "memcpy"), ("_memcmp", "az_memcmp"), ("_open", "az_open"),
    ("_read", "az_read"), ("_close", "az_close"), ("___error", "az_error"), ("_strerror", "az_strerror"),
    ("_strlen", "az_strlen"), ("__NSGetArgc", "az_argc"), ("__NSGetArgv", "az_argv"), ("_chmod", "az_chmod"),
    # Used only by the start of a macOS program; see `macos_only`.
    ("_sigaltstack", "az_sigaltstack"), ("_sigaction", "az_sigaction"), ("__exit", "az_exit_now"),
]
FLAGS = [
    # General-purpose registers only: the code never touches the
    # floating-point registers, and it reads only whole, aligned words.
    "-target", "aarch64-freestanding", "-mcpu=generic", "-mgeneral-regs-only", "-mstrict-align", "-O2",
    "-ffreestanding", "-nostdlib", "-fno-builtin", "-fno-stack-protector", "-fno-pic",
    "-fno-asynchronous-unwind-tables", "-fno-unwind-tables", "-fno-sanitize=all", "-Wall", "-Wextra", "-Werror",
]
ZIG = [sys.executable, "-m", "ziglang"]

declared = re.search(r"pub const IMPORTS: \[&str; \d+\] = \[(.*?)\];", open(os.path.join(HERE, "..", "..", "src", "runtime.rs")).read(), re.S)
assert re.findall(r'"([^"]+)"', declared.group(1)) == [name for name, _ in IMPORTS], "IMPORTS in src/runtime.rs changed"

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
        # Everything the library needs at run time is in these two.
        loaded = section.has(lief.ELF.Section.FLAGS.ALLOC)
        assert not loaded or section.name in (".text", ".bss"), "unexpected section " + section.name
    assert text.virtual_address == BASE + TEXT_OFFSET
    assert symbols["azol_program"] == BASE + TEXT_OFFSET + len(code) and len(code) % 16 == 0
    if memory:
        assert memory[0].type == lief.ELF.Section.TYPE.NOBITS, "the library must not have initialised writable data"
        assert memory[0].virtual_address == MEMORY
    memory_size = memory[0].size if memory else 0
    assert memory_size <= 4096

open(os.path.join(HERE, "runtime.bin"), "wb").write(code)
offset = lambda name: symbols[name] - (BASE + TEXT_OFFSET)
for _, function in IMPORTS:
    assert offset(function) % 4 == 0
out = """// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! The system library of Linux ARM64 programs: the functions the ARM64
//! code imports from the system library on macOS, written for Linux, and
//! the code that starts the program. This file and `runtime.bin` are
//! generated from `runtime/linux_arm64/runtime.c` by
//! `runtime/linux_arm64/gen_runtime.py`; edit those, not this.
//!
//! The library is machine code linked for fixed addresses: it sits at
//! `BASE + TEXT_OFFSET` in every executable, the compiled program follows
//! it directly, and its writable memory is at `MEMORY`. Its functions use
//! the standard ARM64 calling convention.

/// The imports the library provides, in the order of their offsets below.
/// It must be `IMPORTS` in `runtime.rs`, which a test checks.
#[allow(dead_code)]
pub const IMPORT_NAMES: [&str; %d] = [%s];

/// Address the executable file is mapped at.
pub const BASE: u64 = 0x%X;
/// Where in the file, and so after `BASE`, the library starts.
pub const TEXT_OFFSET: u64 = 0x%X;
/// The library's code and constant data. The program's code goes right
/// after it, starting with a jump to the program's `main`.
pub const TEXT: &[u8; %d] = include_bytes!("../runtime/linux_arm64/runtime.bin");
/// Offset in `TEXT` of the function that stands in for each import.
pub const IMPORT_OFFSETS: [u32; %d] = [%s];
/// Offset in `TEXT` of the code the system starts the process at.
pub const START: u32 = %d;
/// Address and size of the library's writable memory, which must start
/// out as zeros.
pub const MEMORY: u64 = 0x%X;
pub const MEMORY_SIZE: u64 = %d;
""" % (
    len(IMPORTS), ", ".join('"%s"' % name for name, _ in IMPORTS), BASE, TEXT_OFFSET, len(code),
    len(IMPORTS), ", ".join(str(offset(function)) for _, function in IMPORTS),
    offset("_start"), MEMORY, memory_size,
)
open(os.path.join(HERE, "..", "..", "src", "runtime_linux_arm64.rs"), "w").write(out)
print("wrote runtime.bin (%d bytes) and src/runtime_linux_arm64.rs; writable memory %d bytes" % (len(code), memory_size))
