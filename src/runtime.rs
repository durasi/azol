// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! The runtime: a few hand-written ARM64 routines appended to every
//! program. They were assembled with clang from `runtime/runtime.s`; the
//! words below are its output. Places that call an imported function or
//! take the address of runtime data are markers the linker rewrites.

/// Functions imported from the system library, in GOT order.
pub const IMPORTS: [&str; 2] = ["_write", "_exit"];

/// Text the runtime prints. `RUNTIME_DATA_OFFSETS[n]` is where item `n`
/// starts; program string literals are placed after it.
pub const RUNTIME_DATA: &[u8] = b"\ntruefalseazol: division by zero\n";
pub const RUNTIME_DATA_OFFSETS: [u32; 4] = [0, 1, 5, 10];

/// Entry points, as word offsets into `RUNTIME`.
pub const PRINT_I64: u32 = 0;
pub const PRINT_BOOL: u32 = 26;
pub const PRINT_STR: u32 = 35;
pub const DIV_ZERO: u32 = 54;

pub enum RuntimeReloc {
    /// Three words: load the import's address and call it.
    Import { at: u32, import: u32 },
    /// Two words: put the address of a runtime data item in `reg`.
    Data { at: u32, data: u32, reg: u32 },
}

pub const RUNTIME_RELOCS: [RuntimeReloc; 9] = [
    RuntimeReloc::Import { at: 21, import: 0 },
    RuntimeReloc::Data { at: 27, data: 1, reg: 0 },
    RuntimeReloc::Data { at: 31, data: 2, reg: 0 },
    RuntimeReloc::Import { at: 42, import: 0 },
    RuntimeReloc::Data { at: 46, data: 0, reg: 1 },
    RuntimeReloc::Import { at: 49, import: 0 },
    RuntimeReloc::Data { at: 54, data: 3, reg: 1 },
    RuntimeReloc::Import { at: 58, import: 0 },
    RuntimeReloc::Import { at: 62, import: 1 },
];

pub const RUNTIME: [u32; 66] = [
    0xA9BC7BFD, //   0: stp x29, x30, [sp, #-0x40]!
    0x910003FD, //   1: mov x29, sp
    0x9100FFE9, //   2: add x9, sp, #0x3f
    0x5280014A, //   3: mov w10, #0xa
    0x3900012A, //   4: strb w10, [x9]
    0xF100001F, //   5: cmp x0, #0
    0xDA80A40B, //   6: cneg x11, x0, lt
    0xD280014D, //   7: mov x13, #0xa
    0x9ACD096E, //   8: udiv x14, x11, x13
    0x9B0DADCF, //   9: msub x15, x14, x13, x11
    0x1100C1EF, //  10: add w15, w15, #0x30
    0x381FFD2F, //  11: strb w15, [x9, #-1]!
    0xAA0E03EB, //  12: mov x11, x14
    0xB5FFFF6B, //  13: cbnz x11, #0x20
    0xB6F80060, //  14: tbz x0, #0x3f, #0x44
    0x528005AF, //  15: mov w15, #0x2d
    0x381FFD2F, //  16: strb w15, [x9, #-1]!
    0xD2800020, //  17: mov x0, #1
    0xAA0903E1, //  18: mov x1, x9
    0x910103E2, //  19: add x2, sp, #0x40
    0xCB090042, //  20: sub x2, x2, x9
    0xD4220000, //  21: brk #0x1000
    0xD503201F, //  22: nop 
    0xD503201F, //  23: nop 
    0xA8C47BFD, //  24: ldp x29, x30, [sp], #0x40
    0xD65F03C0, //  25: ret 
    0xB40000A0, //  26: cbz x0, #0x7c
    0xD4240400, //  27: brk #0x2020
    0xD503201F, //  28: nop 
    0xD2800081, //  29: mov x1, #4
    0x14000007, //  30: b #0x94
    0xD4240800, //  31: brk #0x2040
    0xD503201F, //  32: nop 
    0xD28000A1, //  33: mov x1, #5
    0x14000003, //  34: b #0x94
    0xF9400401, //  35: ldr x1, [x0, #8]
    0xF9400000, //  36: ldr x0, [x0]
    0xA9BF7BFD, //  37: stp x29, x30, [sp, #-0x10]!
    0x910003FD, //  38: mov x29, sp
    0xAA0103E2, //  39: mov x2, x1
    0xAA0003E1, //  40: mov x1, x0
    0xD2800020, //  41: mov x0, #1
    0xD4220000, //  42: brk #0x1000
    0xD503201F, //  43: nop 
    0xD503201F, //  44: nop 
    0xD2800020, //  45: mov x0, #1
    0xD4240020, //  46: brk #0x2001
    0xD503201F, //  47: nop 
    0xD2800022, //  48: mov x2, #1
    0xD4220000, //  49: brk #0x1000
    0xD503201F, //  50: nop 
    0xD503201F, //  51: nop 
    0xA8C17BFD, //  52: ldp x29, x30, [sp], #0x10
    0xD65F03C0, //  53: ret 
    0xD4240C20, //  54: brk #0x2061
    0xD503201F, //  55: nop 
    0xD28002E2, //  56: mov x2, #0x17
    0xD2800040, //  57: mov x0, #2
    0xD4220000, //  58: brk #0x1000
    0xD503201F, //  59: nop 
    0xD503201F, //  60: nop 
    0xD2800020, //  61: mov x0, #1
    0xD4220020, //  62: brk #0x1001
    0xD503201F, //  63: nop 
    0xD503201F, //  64: nop 
    0xD4200020, //  65: brk #1
];
