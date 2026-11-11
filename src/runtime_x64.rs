// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! The Linux x86-64 runtime: the routines every compiled program calls,
//! and the code that starts it. This file and `runtime.bin` are generated
//! from `runtime/linux_x64/runtime.c` by `runtime/linux_x64/gen_runtime.py`;
//! edit those, not this.
//!
//! The runtime is machine code linked for fixed addresses: it sits at
//! `BASE + TEXT_OFFSET` in every executable, the compiled program follows
//! it directly, and its writable memory is at `MEMORY`. Its routines use
//! the System V calling convention.

/// Ids of the routines, shared with the code generator.
pub const PRINT_I64: u32 = 0;
pub const PRINT_BOOL: u32 = 1;
pub const PRINT_STR: u32 = 2;
pub const DIV_ZERO: u32 = 3;
pub const INDEX_FAIL: u32 = 4;
pub const POP_EMPTY: u32 = 5;
pub const ALLOC: u32 = 6;
pub const FREE: u32 = 7;
pub const LIST_PUSH: u32 = 8;
pub const LIST_CLONE: u32 = 9;
pub const DROP: u32 = 10;
pub const STR_CONCAT: u32 = 11;
pub const STR_EQ: u32 = 12;
pub const STR_FROM_I64: u32 = 13;
pub const STR_FROM_BOOL: u32 = 14;
pub const EPRINT_STR: u32 = 15;
pub const EXIT: u32 = 16;
pub const STR_SLICE: u32 = 17;
pub const CHR: u32 = 18;
pub const ARGS: u32 = 19;
pub const READ_FILE: u32 = 20;
pub const WRITE_FILE: u32 = 21;
pub const STR_APPEND: u32 = 22;
/// Number of routines.
pub const ENTRY_COUNT: usize = 23;

/// Address the executable file is mapped at.
pub const BASE: u64 = 0x400000;
/// Where in the file, and so after `BASE`, the runtime starts.
pub const TEXT_OFFSET: u64 = 0x100;
/// The runtime's code and constant data. The program's code goes right
/// after it, starting with a jump to the program's `main`.
pub const TEXT: &[u8; 8576] = include_bytes!("../runtime/linux_x64/runtime.bin");
/// Offset in `TEXT` of each routine, indexed by its id.
pub const ENTRIES: [u32; ENTRY_COUNT] = [784, 1024, 1296, 1616, 1696, 1712, 368, 704, 1728, 2416, 2608, 2688, 3072, 3136, 3456, 1424, 1552, 3520, 3792, 4384, 4688, 6048, 3856];
/// Offset in `TEXT` of the code the system starts the process at.
pub const START: u32 = 0;
/// Address and size of the runtime's writable memory, which must start
/// out as zeros.
pub const MEMORY: u64 = 0x40000000;
pub const MEMORY_SIZE: u64 = 128;
