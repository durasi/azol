// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! The runtime: a few hand-written ARM64 routines appended to every
//! program. This file is generated from `runtime/runtime.s` by
//! `runtime/gen_runtime.py`; edit those, not this. Places that call an
//! imported function or take the address of runtime text are markers the
//! linker rewrites.

/// Functions imported from the system library, in GOT order.
pub const IMPORTS: [&str; 7] = ["_write", "_exit", "_malloc", "_realloc", "_free", "_memcpy", "_memcmp"];

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
pub const DROP: u32 = 160;
pub const STR_CONCAT: u32 = 165;
pub const STR_EQ: u32 = 202;
pub const STR_FROM_I64: u32 = 219;
pub const STR_FROM_BOOL: u32 = 257;

pub enum RuntimeReloc {
    /// Three words: load the import's address and call it.
    Import { at: u32, import: u32 },
    /// Two words: put the address of a runtime text item in `reg`.
    Data { at: u32, data: u32, reg: u32 },
}

pub const RUNTIME_RELOCS: [RuntimeReloc; 26] = [
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
    RuntimeReloc::Import { at: 143, import: 2 },
    RuntimeReloc::Import { at: 150, import: 5 },
    RuntimeReloc::Import { at: 180, import: 2 },
    RuntimeReloc::Import { at: 187, import: 5 },
    RuntimeReloc::Import { at: 195, import: 5 },
    RuntimeReloc::Import { at: 210, import: 6 },
    RuntimeReloc::Import { at: 241, import: 2 },
    RuntimeReloc::Import { at: 250, import: 5 },
    RuntimeReloc::Data { at: 259, data: 1, reg: 9 },
    RuntimeReloc::Data { at: 265, data: 2, reg: 9 },
];

pub const RUNTIME: [u32; 271] = [
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
    0xF9400809, // 130: ldr x9, [x0, #0x10]
    0xB4000389, // 131: cbz x9, #0x27c
    0xA9BD7BFD, // 132: stp x29, x30, [sp, #-0x30]!
    0x910003FD, // 133: mov x29, sp
    0xA90153F3, // 134: stp x19, x20, [sp, #0x10]
    0xF90013F5, // 135: str x21, [sp, #0x20]
    0xAA0003F3, // 136: mov x19, x0
    0xF9400669, // 137: ldr x9, [x19, #8]
    0xF9000A69, // 138: str x9, [x19, #0x10]
    0x9B017D34, // 139: mul x20, x9, x1
    0xF9400275, // 140: ldr x21, [x19]
    0xB40001B4, // 141: cbz x20, #0x268
    0xAA1403E0, // 142: mov x0, x20
    0xD4220040, // 143: brk #0x1002
    0xD503201F, // 144: nop 
    0xD503201F, // 145: nop 
    0xB4FFF600, // 146: cbz x0, #0x108
    0xF9000260, // 147: str x0, [x19]
    0xAA1503E1, // 148: mov x1, x21
    0xAA1403E2, // 149: mov x2, x20
    0xD42200A0, // 150: brk #0x1005
    0xD503201F, // 151: nop 
    0xD503201F, // 152: nop 
    0x14000002, // 153: b #0x26c
    0xF900027F, // 154: str xzr, [x19]
    0xF94013F5, // 155: ldr x21, [sp, #0x20]
    0xA94153F3, // 156: ldp x19, x20, [sp, #0x10]
    0xA8C37BFD, // 157: ldp x29, x30, [sp], #0x30
    0xD65F03C0, // 158: ret 
    0xD65F03C0, // 159: ret 
    0xF9400809, // 160: ldr x9, [x0, #0x10]
    0xB4000069, // 161: cbz x9, #0x290
    0xF9400000, // 162: ldr x0, [x0]
    0x17FFFFB3, // 163: b #0x158
    0xD65F03C0, // 164: ret 
    0xA9BD7BFD, // 165: stp x29, x30, [sp, #-0x30]!
    0x910003FD, // 166: mov x29, sp
    0xA90153F3, // 167: stp x19, x20, [sp, #0x10]
    0xA9025BF5, // 168: stp x21, x22, [sp, #0x20]
    0xAA0003F3, // 169: mov x19, x0
    0xAA0103F4, // 170: mov x20, x1
    0xAA0803F5, // 171: mov x21, x8
    0xF9400669, // 172: ldr x9, [x19, #8]
    0xF940068A, // 173: ldr x10, [x20, #8]
    0x8B0A0136, // 174: add x22, x9, x10
    0xF90006B6, // 175: str x22, [x21, #8]
    0xF9000AB6, // 176: str x22, [x21, #0x10]
    0xF90002BF, // 177: str xzr, [x21]
    0xB4000296, // 178: cbz x22, #0x318
    0xAA1603E0, // 179: mov x0, x22
    0xD4220040, // 180: brk #0x1002
    0xD503201F, // 181: nop 
    0xD503201F, // 182: nop 
    0xB4FFF160, // 183: cbz x0, #0x108
    0xF90002A0, // 184: str x0, [x21]
    0xF9400261, // 185: ldr x1, [x19]
    0xF9400662, // 186: ldr x2, [x19, #8]
    0xD42200A0, // 187: brk #0x1005
    0xD503201F, // 188: nop 
    0xD503201F, // 189: nop 
    0xF94002A0, // 190: ldr x0, [x21]
    0xF9400669, // 191: ldr x9, [x19, #8]
    0x8B090000, // 192: add x0, x0, x9
    0xF9400281, // 193: ldr x1, [x20]
    0xF9400682, // 194: ldr x2, [x20, #8]
    0xD42200A0, // 195: brk #0x1005
    0xD503201F, // 196: nop 
    0xD503201F, // 197: nop 
    0xA9425BF5, // 198: ldp x21, x22, [sp, #0x20]
    0xA94153F3, // 199: ldp x19, x20, [sp, #0x10]
    0xA8C37BFD, // 200: ldp x29, x30, [sp], #0x30
    0xD65F03C0, // 201: ret 
    0xF9400402, // 202: ldr x2, [x0, #8]
    0xF9400429, // 203: ldr x9, [x1, #8]
    0xEB09005F, // 204: cmp x2, x9
    0x54000181, // 205: b.ne #0x364
    0xA9BF7BFD, // 206: stp x29, x30, [sp, #-0x10]!
    0x910003FD, // 207: mov x29, sp
    0xF9400000, // 208: ldr x0, [x0]
    0xF9400021, // 209: ldr x1, [x1]
    0xD42200C0, // 210: brk #0x1006
    0xD503201F, // 211: nop 
    0xD503201F, // 212: nop 
    0x7100001F, // 213: cmp w0, #0
    0x9A9F17E0, // 214: cset x0, eq
    0xA8C17BFD, // 215: ldp x29, x30, [sp], #0x10
    0xD65F03C0, // 216: ret 
    0xD2800000, // 217: mov x0, #0
    0xD65F03C0, // 218: ret 
    0xA9BB7BFD, // 219: stp x29, x30, [sp, #-0x50]!
    0x910003FD, // 220: mov x29, sp
    0xA90153F3, // 221: stp x19, x20, [sp, #0x10]
    0xF90013F5, // 222: str x21, [sp, #0x20]
    0xAA0803F5, // 223: mov x21, x8
    0x910143E9, // 224: add x9, sp, #0x50
    0xF100001F, // 225: cmp x0, #0
    0xDA80A40B, // 226: cneg x11, x0, lt
    0xD280014D, // 227: mov x13, #0xa
    0x9ACD096E, // 228: udiv x14, x11, x13
    0x9B0DADCF, // 229: msub x15, x14, x13, x11
    0x1100C1EF, // 230: add w15, w15, #0x30
    0x381FFD2F, // 231: strb w15, [x9, #-1]!
    0xAA0E03EB, // 232: mov x11, x14
    0xB5FFFF6B, // 233: cbnz x11, #0x390
    0xB6F80060, // 234: tbz x0, #0x3f, #0x3b4
    0x528005AF, // 235: mov w15, #0x2d
    0x381FFD2F, // 236: strb w15, [x9, #-1]!
    0xAA0903F3, // 237: mov x19, x9
    0x910143F4, // 238: add x20, sp, #0x50
    0xCB090294, // 239: sub x20, x20, x9
    0xAA1403E0, // 240: mov x0, x20
    0xD4220040, // 241: brk #0x1002
    0xD503201F, // 242: nop 
    0xD503201F, // 243: nop 
    0xB4FFE9C0, // 244: cbz x0, #0x108
    0xF90002A0, // 245: str x0, [x21]
    0xF90006B4, // 246: str x20, [x21, #8]
    0xF9000AB4, // 247: str x20, [x21, #0x10]
    0xAA1303E1, // 248: mov x1, x19
    0xAA1403E2, // 249: mov x2, x20
    0xD42200A0, // 250: brk #0x1005
    0xD503201F, // 251: nop 
    0xD503201F, // 252: nop 
    0xF94013F5, // 253: ldr x21, [sp, #0x20]
    0xA94153F3, // 254: ldp x19, x20, [sp, #0x10]
    0xA8C57BFD, // 255: ldp x29, x30, [sp], #0x50
    0xD65F03C0, // 256: ret 
    0xF900091F, // 257: str xzr, [x8, #0x10]
    0xB40000E0, // 258: cbz x0, #0x424
    0xD4240520, // 259: brk #0x2029
    0xD503201F, // 260: nop 
    0xF9000109, // 261: str x9, [x8]
    0xD2800089, // 262: mov x9, #4
    0xF9000509, // 263: str x9, [x8, #8]
    0xD65F03C0, // 264: ret 
    0xD4240920, // 265: brk #0x2049
    0xD503201F, // 266: nop 
    0xF9000109, // 267: str x9, [x8]
    0xD28000A9, // 268: mov x9, #5
    0xF9000509, // 269: str x9, [x8, #8]
    0xD65F03C0, // 270: ret 
];
