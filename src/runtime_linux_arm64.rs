// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

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
pub const IMPORT_NAMES: [&str; 19] = ["_write", "_exit", "_malloc", "_realloc", "_free", "_memcpy", "_memcmp", "_open", "_read", "_close", "___error", "_strerror", "_strlen", "__NSGetArgc", "__NSGetArgv", "_chmod", "_sigaltstack", "_sigaction", "__exit"];

/// Address the executable file is mapped at.
pub const BASE: u64 = 0x400000;
/// Where in the file, and so after `BASE`, the library starts.
pub const TEXT_OFFSET: u64 = 0x100;
/// The library's code and constant data. The program's code goes right
/// after it, starting with a jump to the program's `main`.
pub const TEXT: &[u8; 6000] = include_bytes!("../runtime/linux_arm64/runtime.bin");
/// Offset in `TEXT` of the function that stands in for each import.
pub const IMPORT_OFFSETS: [u32; 19] = [548, 596, 648, 936, 2076, 2156, 3036, 0, 3208, 3256, 3312, 3324, 4556, 4580, 4592, 4604, 4664, 4696, 4708];
/// Offset in `TEXT` of the code the system starts the process at.
pub const START: u32 = 16;
/// Address and size of the library's writable memory, which must start
/// out as zeros.
pub const MEMORY: u64 = 0x40000000;
pub const MEMORY_SIZE: u64 = 176;
