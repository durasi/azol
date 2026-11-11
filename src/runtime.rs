// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! The runtime: a few hand-written ARM64 routines appended to every
//! program. This file is generated from `runtime/runtime.s` by
//! `runtime/gen_runtime.py`; edit those, not this. Places that call an
//! imported function or take the address of runtime text are markers the
//! linker rewrites.

/// Functions imported from the system library, in GOT order.
pub const IMPORTS: [&str; 6] = ["_write", "_exit", "_malloc", "_realloc", "_free", "_memcpy"];

/// Text the runtime prints. `RUNTIME_DATA_OFFSETS[n]` is where item `n`
/// starts; program string literals are placed after it.
pub const RUNTIME_DATA: &[u8] = b"\ntruefalseazol: division by zero\nazol: index out of range\nazol: pop from an empty list\nazol: out of memory\n";
pub const RUNTIME_DATA_OFFSETS: [u32; 7] = [0, 1, 5, 10, 33, 58, 87];

/// Entry points, as word offsets into `RUNTIME`.
pub const PRINT_I64: u32 = 0;
pub const PRINT_BOOL: u32 = 26;
pub const PRINT_STR: u32 = 35;
pub const DIV_ZERO: u32 = 54;
pub const INDEX_FAIL: u32 = 58;
pub const POP_EMPTY: u32 = 62;
pub const ALLOC: u32 = 78;
pub const FREE: u32 = 86;
pub const LIST_PUSH: u32 = 93;
pub const LIST_CLONE: u32 = 130;

pub enum RuntimeReloc {
    /// Three words: load the import's address and call it.
    Import { at: u32, import: u32 },
    /// Two words: put the address of a runtime text item in `reg`.
    Data { at: u32, data: u32, reg: u32 },
}

pub const RUNTIME_RELOCS: [RuntimeReloc; 18] = [
    RuntimeReloc::Import { at: 21, import: 0 },
    RuntimeReloc::Data { at: 27, data: 1, reg: 0 },
    RuntimeReloc::Data { at: 31, data: 2, reg: 0 },
    RuntimeReloc::Import { at: 42, import: 0 },
    RuntimeReloc::Data { at: 46, data: 0, reg: 1 },
    RuntimeReloc::Import { at: 49, import: 0 },
    RuntimeReloc::Data { at: 54, data: 3, reg: 1 },
    RuntimeReloc::Data { at: 58, data: 4, reg: 1 },
    RuntimeReloc::Data { at: 62, data: 5, reg: 1 },
    RuntimeReloc::Data { at: 66, data: 6, reg: 1 },
    RuntimeReloc::Import { at: 70, import: 0 },
    RuntimeReloc::Import { at: 74, import: 1 },
    RuntimeReloc::Import { at: 80, import: 2 },
    RuntimeReloc::Import { at: 88, import: 4 },
    RuntimeReloc::Import { at: 111, import: 3 },
    RuntimeReloc::Import { at: 123, import: 5 },
    RuntimeReloc::Import { at: 141, import: 2 },
    RuntimeReloc::Import { at: 148, import: 5 },
];

pub const RUNTIME: [u32; 157] = [
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
    0x1400000C, //  57: b #0x114
    0xD4241020, //  58: brk #0x2081
    0xD503201F, //  59: nop 
    0xD2800322, //  60: mov x2, #0x19
    0x14000008, //  61: b #0x114
    0xD4241420, //  62: brk #0x20a1
    0xD503201F, //  63: nop 
    0xD28003A2, //  64: mov x2, #0x1d
    0x14000004, //  65: b #0x114
    0xD4241820, //  66: brk #0x20c1
    0xD503201F, //  67: nop 
    0xD2800282, //  68: mov x2, #0x14
    0xD2800040, //  69: mov x0, #2
    0xD4220000, //  70: brk #0x1000
    0xD503201F, //  71: nop 
    0xD503201F, //  72: nop 
    0xD2800020, //  73: mov x0, #1
    0xD4220020, //  74: brk #0x1001
    0xD503201F, //  75: nop 
    0xD503201F, //  76: nop 
    0xD4200020, //  77: brk #1
    0xA9BF7BFD, //  78: stp x29, x30, [sp, #-0x10]!
    0x910003FD, //  79: mov x29, sp
    0xD4220040, //  80: brk #0x1002
    0xD503201F, //  81: nop 
    0xD503201F, //  82: nop 
    0xB4FFFDE0, //  83: cbz x0, #0x108
    0xA8C17BFD, //  84: ldp x29, x30, [sp], #0x10
    0xD65F03C0, //  85: ret 
    0xA9BF7BFD, //  86: stp x29, x30, [sp, #-0x10]!
    0x910003FD, //  87: mov x29, sp
    0xD4220080, //  88: brk #0x1004
    0xD503201F, //  89: nop 
    0xD503201F, //  90: nop 
    0xA8C17BFD, //  91: ldp x29, x30, [sp], #0x10
    0xD65F03C0, //  92: ret 
    0xA9BD7BFD, //  93: stp x29, x30, [sp, #-0x30]!
    0x910003FD, //  94: mov x29, sp
    0xA90153F3, //  95: stp x19, x20, [sp, #0x10]
    0xF90013F5, //  96: str x21, [sp, #0x20]
    0xAA0003F3, //  97: mov x19, x0
    0xAA0103F4, //  98: mov x20, x1
    0xAA0203F5, //  99: mov x21, x2
    0xF9400669, // 100: ldr x9, [x19, #8]
    0xF9400A6A, // 101: ldr x10, [x19, #0x10]
    0xEB0A013F, // 102: cmp x9, x10
    0x540001A3, // 103: b.lo #0x1d0
    0xD37FF94A, // 104: lsl x10, x10, #1
    0xD280008B, // 105: mov x11, #4
    0xF100015F, // 106: cmp x10, #0
    0x9A8A016A, // 107: csel x10, x11, x10, eq
    0xF9000A6A, // 108: str x10, [x19, #0x10]
    0xF9400260, // 109: ldr x0, [x19]
    0x9B157D41, // 110: mul x1, x10, x21
    0xD4220060, // 111: brk #0x1003
    0xD503201F, // 112: nop 
    0xD503201F, // 113: nop 
    0xB4FFFA00, // 114: cbz x0, #0x108
    0xF9000260, // 115: str x0, [x19]
    0xF9400260, // 116: ldr x0, [x19]
    0xF9400669, // 117: ldr x9, [x19, #8]
    0x9B150120, // 118: madd x0, x9, x21, x0
    0x91000529, // 119: add x9, x9, #1
    0xF9000669, // 120: str x9, [x19, #8]
    0xAA1403E1, // 121: mov x1, x20
    0xAA1503E2, // 122: mov x2, x21
    0xD42200A0, // 123: brk #0x1005
    0xD503201F, // 124: nop 
    0xD503201F, // 125: nop 
    0xF94013F5, // 126: ldr x21, [sp, #0x20]
    0xA94153F3, // 127: ldp x19, x20, [sp, #0x10]
    0xA8C37BFD, // 128: ldp x29, x30, [sp], #0x30
    0xD65F03C0, // 129: ret 
    0xA9BD7BFD, // 130: stp x29, x30, [sp, #-0x30]!
    0x910003FD, // 131: mov x29, sp
    0xA90153F3, // 132: stp x19, x20, [sp, #0x10]
    0xF90013F5, // 133: str x21, [sp, #0x20]
    0xAA0003F3, // 134: mov x19, x0
    0xF9400669, // 135: ldr x9, [x19, #8]
    0xF9000A69, // 136: str x9, [x19, #0x10]
    0x9B017D34, // 137: mul x20, x9, x1
    0xF9400275, // 138: ldr x21, [x19]
    0xB40001B4, // 139: cbz x20, #0x260
    0xAA1403E0, // 140: mov x0, x20
    0xD4220040, // 141: brk #0x1002
    0xD503201F, // 142: nop 
    0xD503201F, // 143: nop 
    0xB4FFF640, // 144: cbz x0, #0x108
    0xF9000260, // 145: str x0, [x19]
    0xAA1503E1, // 146: mov x1, x21
    0xAA1403E2, // 147: mov x2, x20
    0xD42200A0, // 148: brk #0x1005
    0xD503201F, // 149: nop 
    0xD503201F, // 150: nop 
    0x14000002, // 151: b #0x264
    0xF900027F, // 152: str xzr, [x19]
    0xF94013F5, // 153: ldr x21, [sp, #0x20]
    0xA94153F3, // 154: ldp x19, x20, [sp, #0x10]
    0xA8C37BFD, // 155: ldp x29, x30, [sp], #0x30
    0xD65F03C0, // 156: ret 
];
