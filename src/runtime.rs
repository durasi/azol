// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! The runtime: a few hand-written ARM64 routines appended to every
//! program. This file is generated from `runtime/runtime.s` by
//! `runtime/gen_runtime.py`; edit those, not this. Places that call an
//! imported function or take the address of runtime text are markers the
//! linker rewrites.

/// Functions imported from the system library, in GOT order.
pub const IMPORTS: [&str; 16] = ["_write", "_exit", "_malloc", "_realloc", "_free", "_memcpy", "_memcmp", "_open", "_read", "_close", "___error", "_strerror", "_strlen", "__NSGetArgc", "__NSGetArgv", "_chmod"];

/// Text the runtime prints. `RUNTIME_DATA_OFFSETS[n]` is where item `n`
/// starts; program string literals are placed after it.
pub const RUNTIME_DATA: &[u8] = b"\ntruefalseazol: division by zero\nazol: index out of range\nazol: pop from an empty list\nazol: out of memory\nazol: slice out of range\nazol: chr needs a number from 0 to 255\n\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\n\x0b\x0c\x0d\x0e\x0f\x10\x11\x12\x13\x14\x15\x16\x17\x18\x19\x1a\x1b\x1c\x1d\x1e\x1f !\x22#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\x5c]^_`abcdefghijklmnopqrstuvwxyz{|}~\x7f\x80\x81\x82\x83\x84\x85\x86\x87\x88\x89\x8a\x8b\x8c\x8d\x8e\x8f\x90\x91\x92\x93\x94\x95\x96\x97\x98\x99\x9a\x9b\x9c\x9d\x9e\x9f\xa0\xa1\xa2\xa3\xa4\xa5\xa6\xa7\xa8\xa9\xaa\xab\xac\xad\xae\xaf\xb0\xb1\xb2\xb3\xb4\xb5\xb6\xb7\xb8\xb9\xba\xbb\xbc\xbd\xbe\xbf\xc0\xc1\xc2\xc3\xc4\xc5\xc6\xc7\xc8\xc9\xca\xcb\xcc\xcd\xce\xcf\xd0\xd1\xd2\xd3\xd4\xd5\xd6\xd7\xd8\xd9\xda\xdb\xdc\xdd\xde\xdf\xe0\xe1\xe2\xe3\xe4\xe5\xe6\xe7\xe8\xe9\xea\xeb\xec\xed\xee\xef\xf0\xf1\xf2\xf3\xf4\xf5\xf6\xf7\xf8\xf9\xfa\xfb\xfc\xfd\xfe\xff";
pub const RUNTIME_DATA_OFFSETS: [u32; 10] = [0, 1, 5, 10, 33, 58, 87, 107, 132, 171];

/// Entry points, as word offsets into `RUNTIME`.
pub const PRINT_I64: u32 = 0;
pub const PRINT_BOOL: u32 = 26;
pub const PRINT_STR: u32 = 35;
pub const DIV_ZERO: u32 = 66;
pub const INDEX_FAIL: u32 = 70;
pub const POP_EMPTY: u32 = 74;
pub const ALLOC: u32 = 98;
pub const FREE: u32 = 106;
pub const LIST_PUSH: u32 = 113;
pub const LIST_CLONE: u32 = 150;
pub const DROP: u32 = 180;
pub const STR_CONCAT: u32 = 185;
pub const STR_EQ: u32 = 222;
pub const STR_FROM_I64: u32 = 239;
pub const STR_FROM_BOOL: u32 = 277;
pub const EPRINT_STR: u32 = 58;
pub const EXIT: u32 = 62;
pub const STR_SLICE: u32 = 291;
pub const CHR: u32 = 323;
pub const ARGS: u32 = 333;
pub const READ_FILE: u32 = 436;
pub const WRITE_FILE: u32 = 523;
pub const STR_APPEND: u32 = 583;
pub const SET_EXECUTABLE: u32 = 645;

pub enum RuntimeReloc {
    /// Three words: load the import's address and call it.
    Import { at: u32, import: u32 },
    /// Two words: put the address of a runtime text item in `reg`.
    Data { at: u32, data: u32, reg: u32 },
}

pub const RUNTIME_RELOCS: [RuntimeReloc; 58] = [
    RuntimeReloc::Import { at: 21, import: 0 },
    RuntimeReloc::Data { at: 27, data: 1, reg: 0 },
    RuntimeReloc::Data { at: 31, data: 2, reg: 0 },
    RuntimeReloc::Import { at: 45, import: 0 },
    RuntimeReloc::Data { at: 49, data: 0, reg: 1 },
    RuntimeReloc::Import { at: 52, import: 0 },
    RuntimeReloc::Import { at: 62, import: 1 },
    RuntimeReloc::Data { at: 66, data: 3, reg: 1 },
    RuntimeReloc::Data { at: 70, data: 4, reg: 1 },
    RuntimeReloc::Data { at: 74, data: 5, reg: 1 },
    RuntimeReloc::Data { at: 78, data: 7, reg: 1 },
    RuntimeReloc::Data { at: 82, data: 8, reg: 1 },
    RuntimeReloc::Data { at: 86, data: 6, reg: 1 },
    RuntimeReloc::Import { at: 90, import: 0 },
    RuntimeReloc::Import { at: 94, import: 1 },
    RuntimeReloc::Import { at: 100, import: 2 },
    RuntimeReloc::Import { at: 108, import: 4 },
    RuntimeReloc::Import { at: 131, import: 3 },
    RuntimeReloc::Import { at: 143, import: 5 },
    RuntimeReloc::Import { at: 163, import: 2 },
    RuntimeReloc::Import { at: 170, import: 5 },
    RuntimeReloc::Import { at: 200, import: 2 },
    RuntimeReloc::Import { at: 207, import: 5 },
    RuntimeReloc::Import { at: 215, import: 5 },
    RuntimeReloc::Import { at: 230, import: 6 },
    RuntimeReloc::Import { at: 261, import: 2 },
    RuntimeReloc::Import { at: 270, import: 5 },
    RuntimeReloc::Data { at: 279, data: 1, reg: 9 },
    RuntimeReloc::Data { at: 285, data: 2, reg: 9 },
    RuntimeReloc::Import { at: 309, import: 2 },
    RuntimeReloc::Import { at: 316, import: 5 },
    RuntimeReloc::Data { at: 325, data: 9, reg: 9 },
    RuntimeReloc::Import { at: 339, import: 13 },
    RuntimeReloc::Import { at: 343, import: 14 },
    RuntimeReloc::Import { at: 354, import: 2 },
    RuntimeReloc::Import { at: 363, import: 12 },
    RuntimeReloc::Import { at: 383, import: 2 },
    RuntimeReloc::Import { at: 391, import: 5 },
    RuntimeReloc::Import { at: 405, import: 10 },
    RuntimeReloc::Import { at: 409, import: 11 },
    RuntimeReloc::Import { at: 413, import: 12 },
    RuntimeReloc::Import { at: 422, import: 2 },
    RuntimeReloc::Import { at: 429, import: 5 },
    RuntimeReloc::Import { at: 445, import: 7 },
    RuntimeReloc::Import { at: 470, import: 3 },
    RuntimeReloc::Import { at: 478, import: 8 },
    RuntimeReloc::Import { at: 491, import: 9 },
    RuntimeReloc::Import { at: 497, import: 9 },
    RuntimeReloc::Import { at: 510, import: 3 },
    RuntimeReloc::Import { at: 537, import: 7 },
    RuntimeReloc::Import { at: 557, import: 0 },
    RuntimeReloc::Import { at: 567, import: 9 },
    RuntimeReloc::Import { at: 573, import: 9 },
    RuntimeReloc::Import { at: 610, import: 3 },
    RuntimeReloc::Import { at: 619, import: 2 },
    RuntimeReloc::Import { at: 627, import: 5 },
    RuntimeReloc::Import { at: 636, import: 5 },
    RuntimeReloc::Import { at: 652, import: 15 },
];

pub const RUNTIME: [u32; 669] = [
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
    0xD2800022, //  37: mov x2, #1
    0xA9BE7BFD, //  38: stp x29, x30, [sp, #-0x20]!
    0x910003FD, //  39: mov x29, sp
    0xF9000BF3, //  40: str x19, [sp, #0x10]
    0xAA0203F3, //  41: mov x19, x2
    0xAA0103E2, //  42: mov x2, x1
    0xAA0003E1, //  43: mov x1, x0
    0xAA1303E0, //  44: mov x0, x19
    0xD4220000, //  45: brk #0x1000
    0xD503201F, //  46: nop 
    0xD503201F, //  47: nop 
    0xAA1303E0, //  48: mov x0, x19
    0xD4240020, //  49: brk #0x2001
    0xD503201F, //  50: nop 
    0xD2800022, //  51: mov x2, #1
    0xD4220000, //  52: brk #0x1000
    0xD503201F, //  53: nop 
    0xD503201F, //  54: nop 
    0xF9400BF3, //  55: ldr x19, [sp, #0x10]
    0xA8C27BFD, //  56: ldp x29, x30, [sp], #0x20
    0xD65F03C0, //  57: ret 
    0xF9400401, //  58: ldr x1, [x0, #8]
    0xF9400000, //  59: ldr x0, [x0]
    0xD2800042, //  60: mov x2, #2
    0x17FFFFE9, //  61: b #0x98
    0xD4220020, //  62: brk #0x1001
    0xD503201F, //  63: nop 
    0xD503201F, //  64: nop 
    0xD4200020, //  65: brk #1
    0xD4240C20, //  66: brk #0x2061
    0xD503201F, //  67: nop 
    0xD28002E2, //  68: mov x2, #0x17
    0x14000014, //  69: b #0x164
    0xD4241020, //  70: brk #0x2081
    0xD503201F, //  71: nop 
    0xD2800322, //  72: mov x2, #0x19
    0x14000010, //  73: b #0x164
    0xD4241420, //  74: brk #0x20a1
    0xD503201F, //  75: nop 
    0xD28003A2, //  76: mov x2, #0x1d
    0x1400000C, //  77: b #0x164
    0xD4241C20, //  78: brk #0x20e1
    0xD503201F, //  79: nop 
    0xD2800322, //  80: mov x2, #0x19
    0x14000008, //  81: b #0x164
    0xD4242020, //  82: brk #0x2101
    0xD503201F, //  83: nop 
    0xD28004E2, //  84: mov x2, #0x27
    0x14000004, //  85: b #0x164
    0xD4241820, //  86: brk #0x20c1
    0xD503201F, //  87: nop 
    0xD2800282, //  88: mov x2, #0x14
    0xD2800040, //  89: mov x0, #2
    0xD4220000, //  90: brk #0x1000
    0xD503201F, //  91: nop 
    0xD503201F, //  92: nop 
    0xD2800020, //  93: mov x0, #1
    0xD4220020, //  94: brk #0x1001
    0xD503201F, //  95: nop 
    0xD503201F, //  96: nop 
    0xD4200020, //  97: brk #1
    0xA9BF7BFD, //  98: stp x29, x30, [sp, #-0x10]!
    0x910003FD, //  99: mov x29, sp
    0xD4220040, // 100: brk #0x1002
    0xD503201F, // 101: nop 
    0xD503201F, // 102: nop 
    0xB4FFFDE0, // 103: cbz x0, #0x158
    0xA8C17BFD, // 104: ldp x29, x30, [sp], #0x10
    0xD65F03C0, // 105: ret 
    0xA9BF7BFD, // 106: stp x29, x30, [sp, #-0x10]!
    0x910003FD, // 107: mov x29, sp
    0xD4220080, // 108: brk #0x1004
    0xD503201F, // 109: nop 
    0xD503201F, // 110: nop 
    0xA8C17BFD, // 111: ldp x29, x30, [sp], #0x10
    0xD65F03C0, // 112: ret 
    0xA9BD7BFD, // 113: stp x29, x30, [sp, #-0x30]!
    0x910003FD, // 114: mov x29, sp
    0xA90153F3, // 115: stp x19, x20, [sp, #0x10]
    0xF90013F5, // 116: str x21, [sp, #0x20]
    0xAA0003F3, // 117: mov x19, x0
    0xAA0103F4, // 118: mov x20, x1
    0xAA0203F5, // 119: mov x21, x2
    0xF9400669, // 120: ldr x9, [x19, #8]
    0xF9400A6A, // 121: ldr x10, [x19, #0x10]
    0xEB0A013F, // 122: cmp x9, x10
    0x540001A3, // 123: b.lo #0x220
    0xD37FF94A, // 124: lsl x10, x10, #1
    0xD280008B, // 125: mov x11, #4
    0xF100015F, // 126: cmp x10, #0
    0x9A8A016A, // 127: csel x10, x11, x10, eq
    0xF9000A6A, // 128: str x10, [x19, #0x10]
    0xF9400260, // 129: ldr x0, [x19]
    0x9B157D41, // 130: mul x1, x10, x21
    0xD4220060, // 131: brk #0x1003
    0xD503201F, // 132: nop 
    0xD503201F, // 133: nop 
    0xB4FFFA00, // 134: cbz x0, #0x158
    0xF9000260, // 135: str x0, [x19]
    0xF9400260, // 136: ldr x0, [x19]
    0xF9400669, // 137: ldr x9, [x19, #8]
    0x9B150120, // 138: madd x0, x9, x21, x0
    0x91000529, // 139: add x9, x9, #1
    0xF9000669, // 140: str x9, [x19, #8]
    0xAA1403E1, // 141: mov x1, x20
    0xAA1503E2, // 142: mov x2, x21
    0xD42200A0, // 143: brk #0x1005
    0xD503201F, // 144: nop 
    0xD503201F, // 145: nop 
    0xF94013F5, // 146: ldr x21, [sp, #0x20]
    0xA94153F3, // 147: ldp x19, x20, [sp, #0x10]
    0xA8C37BFD, // 148: ldp x29, x30, [sp], #0x30
    0xD65F03C0, // 149: ret 
    0xF9400809, // 150: ldr x9, [x0, #0x10]
    0xB4000389, // 151: cbz x9, #0x2cc
    0xA9BD7BFD, // 152: stp x29, x30, [sp, #-0x30]!
    0x910003FD, // 153: mov x29, sp
    0xA90153F3, // 154: stp x19, x20, [sp, #0x10]
    0xF90013F5, // 155: str x21, [sp, #0x20]
    0xAA0003F3, // 156: mov x19, x0
    0xF9400669, // 157: ldr x9, [x19, #8]
    0xF9000A69, // 158: str x9, [x19, #0x10]
    0x9B017D34, // 159: mul x20, x9, x1
    0xF9400275, // 160: ldr x21, [x19]
    0xB40001B4, // 161: cbz x20, #0x2b8
    0xAA1403E0, // 162: mov x0, x20
    0xD4220040, // 163: brk #0x1002
    0xD503201F, // 164: nop 
    0xD503201F, // 165: nop 
    0xB4FFF600, // 166: cbz x0, #0x158
    0xF9000260, // 167: str x0, [x19]
    0xAA1503E1, // 168: mov x1, x21
    0xAA1403E2, // 169: mov x2, x20
    0xD42200A0, // 170: brk #0x1005
    0xD503201F, // 171: nop 
    0xD503201F, // 172: nop 
    0x14000002, // 173: b #0x2bc
    0xF900027F, // 174: str xzr, [x19]
    0xF94013F5, // 175: ldr x21, [sp, #0x20]
    0xA94153F3, // 176: ldp x19, x20, [sp, #0x10]
    0xA8C37BFD, // 177: ldp x29, x30, [sp], #0x30
    0xD65F03C0, // 178: ret 
    0xD65F03C0, // 179: ret 
    0xF9400809, // 180: ldr x9, [x0, #0x10]
    0xB4000069, // 181: cbz x9, #0x2e0
    0xF9400000, // 182: ldr x0, [x0]
    0x17FFFFB3, // 183: b #0x1a8
    0xD65F03C0, // 184: ret 
    0xA9BD7BFD, // 185: stp x29, x30, [sp, #-0x30]!
    0x910003FD, // 186: mov x29, sp
    0xA90153F3, // 187: stp x19, x20, [sp, #0x10]
    0xA9025BF5, // 188: stp x21, x22, [sp, #0x20]
    0xAA0003F3, // 189: mov x19, x0
    0xAA0103F4, // 190: mov x20, x1
    0xAA0803F5, // 191: mov x21, x8
    0xF9400669, // 192: ldr x9, [x19, #8]
    0xF940068A, // 193: ldr x10, [x20, #8]
    0x8B0A0136, // 194: add x22, x9, x10
    0xF90006B6, // 195: str x22, [x21, #8]
    0xF9000AB6, // 196: str x22, [x21, #0x10]
    0xF90002BF, // 197: str xzr, [x21]
    0xB4000296, // 198: cbz x22, #0x368
    0xAA1603E0, // 199: mov x0, x22
    0xD4220040, // 200: brk #0x1002
    0xD503201F, // 201: nop 
    0xD503201F, // 202: nop 
    0xB4FFF160, // 203: cbz x0, #0x158
    0xF90002A0, // 204: str x0, [x21]
    0xF9400261, // 205: ldr x1, [x19]
    0xF9400662, // 206: ldr x2, [x19, #8]
    0xD42200A0, // 207: brk #0x1005
    0xD503201F, // 208: nop 
    0xD503201F, // 209: nop 
    0xF94002A0, // 210: ldr x0, [x21]
    0xF9400669, // 211: ldr x9, [x19, #8]
    0x8B090000, // 212: add x0, x0, x9
    0xF9400281, // 213: ldr x1, [x20]
    0xF9400682, // 214: ldr x2, [x20, #8]
    0xD42200A0, // 215: brk #0x1005
    0xD503201F, // 216: nop 
    0xD503201F, // 217: nop 
    0xA9425BF5, // 218: ldp x21, x22, [sp, #0x20]
    0xA94153F3, // 219: ldp x19, x20, [sp, #0x10]
    0xA8C37BFD, // 220: ldp x29, x30, [sp], #0x30
    0xD65F03C0, // 221: ret 
    0xF9400402, // 222: ldr x2, [x0, #8]
    0xF9400429, // 223: ldr x9, [x1, #8]
    0xEB09005F, // 224: cmp x2, x9
    0x54000181, // 225: b.ne #0x3b4
    0xA9BF7BFD, // 226: stp x29, x30, [sp, #-0x10]!
    0x910003FD, // 227: mov x29, sp
    0xF9400000, // 228: ldr x0, [x0]
    0xF9400021, // 229: ldr x1, [x1]
    0xD42200C0, // 230: brk #0x1006
    0xD503201F, // 231: nop 
    0xD503201F, // 232: nop 
    0x7100001F, // 233: cmp w0, #0
    0x9A9F17E0, // 234: cset x0, eq
    0xA8C17BFD, // 235: ldp x29, x30, [sp], #0x10
    0xD65F03C0, // 236: ret 
    0xD2800000, // 237: mov x0, #0
    0xD65F03C0, // 238: ret 
    0xA9BB7BFD, // 239: stp x29, x30, [sp, #-0x50]!
    0x910003FD, // 240: mov x29, sp
    0xA90153F3, // 241: stp x19, x20, [sp, #0x10]
    0xF90013F5, // 242: str x21, [sp, #0x20]
    0xAA0803F5, // 243: mov x21, x8
    0x910143E9, // 244: add x9, sp, #0x50
    0xF100001F, // 245: cmp x0, #0
    0xDA80A40B, // 246: cneg x11, x0, lt
    0xD280014D, // 247: mov x13, #0xa
    0x9ACD096E, // 248: udiv x14, x11, x13
    0x9B0DADCF, // 249: msub x15, x14, x13, x11
    0x1100C1EF, // 250: add w15, w15, #0x30
    0x381FFD2F, // 251: strb w15, [x9, #-1]!
    0xAA0E03EB, // 252: mov x11, x14
    0xB5FFFF6B, // 253: cbnz x11, #0x3e0
    0xB6F80060, // 254: tbz x0, #0x3f, #0x404
    0x528005AF, // 255: mov w15, #0x2d
    0x381FFD2F, // 256: strb w15, [x9, #-1]!
    0xAA0903F3, // 257: mov x19, x9
    0x910143F4, // 258: add x20, sp, #0x50
    0xCB090294, // 259: sub x20, x20, x9
    0xAA1403E0, // 260: mov x0, x20
    0xD4220040, // 261: brk #0x1002
    0xD503201F, // 262: nop 
    0xD503201F, // 263: nop 
    0xB4FFE9C0, // 264: cbz x0, #0x158
    0xF90002A0, // 265: str x0, [x21]
    0xF90006B4, // 266: str x20, [x21, #8]
    0xF9000AB4, // 267: str x20, [x21, #0x10]
    0xAA1303E1, // 268: mov x1, x19
    0xAA1403E2, // 269: mov x2, x20
    0xD42200A0, // 270: brk #0x1005
    0xD503201F, // 271: nop 
    0xD503201F, // 272: nop 
    0xF94013F5, // 273: ldr x21, [sp, #0x20]
    0xA94153F3, // 274: ldp x19, x20, [sp, #0x10]
    0xA8C57BFD, // 275: ldp x29, x30, [sp], #0x50
    0xD65F03C0, // 276: ret 
    0xF900091F, // 277: str xzr, [x8, #0x10]
    0xB40000E0, // 278: cbz x0, #0x474
    0xD4240520, // 279: brk #0x2029
    0xD503201F, // 280: nop 
    0xF9000109, // 281: str x9, [x8]
    0xD2800089, // 282: mov x9, #4
    0xF9000509, // 283: str x9, [x8, #8]
    0xD65F03C0, // 284: ret 
    0xD4240920, // 285: brk #0x2049
    0xD503201F, // 286: nop 
    0xF9000109, // 287: str x9, [x8]
    0xD28000A9, // 288: mov x9, #5
    0xF9000509, // 289: str x9, [x8, #8]
    0xD65F03C0, // 290: ret 
    0xF9400409, // 291: ldr x9, [x0, #8]
    0xEB09005F, // 292: cmp x2, x9
    0x54FFE528, // 293: b.hi #0x138
    0xEB02003F, // 294: cmp x1, x2
    0x54FFE4E8, // 295: b.hi #0x138
    0xA9BD7BFD, // 296: stp x29, x30, [sp, #-0x30]!
    0x910003FD, // 297: mov x29, sp
    0xA90153F3, // 298: stp x19, x20, [sp, #0x10]
    0xF90013F5, // 299: str x21, [sp, #0x20]
    0xAA0803F3, // 300: mov x19, x8
    0xF9400014, // 301: ldr x20, [x0]
    0x8B010294, // 302: add x20, x20, x1
    0xCB010055, // 303: sub x21, x2, x1
    0xF900027F, // 304: str xzr, [x19]
    0xF9000675, // 305: str x21, [x19, #8]
    0xF9000A75, // 306: str x21, [x19, #0x10]
    0xB4000195, // 307: cbz x21, #0x4fc
    0xAA1503E0, // 308: mov x0, x21
    0xD4220040, // 309: brk #0x1002
    0xD503201F, // 310: nop 
    0xD503201F, // 311: nop 
    0xB4FFE3C0, // 312: cbz x0, #0x158
    0xF9000260, // 313: str x0, [x19]
    0xAA1403E1, // 314: mov x1, x20
    0xAA1503E2, // 315: mov x2, x21
    0xD42200A0, // 316: brk #0x1005
    0xD503201F, // 317: nop 
    0xD503201F, // 318: nop 
    0xF94013F5, // 319: ldr x21, [sp, #0x20]
    0xA94153F3, // 320: ldp x19, x20, [sp, #0x10]
    0xA8C37BFD, // 321: ldp x29, x30, [sp], #0x30
    0xD65F03C0, // 322: ret 
    0xF103FC1F, // 323: cmp x0, #0xff
    0x54FFE1C8, // 324: b.hi #0x148
    0xD4242520, // 325: brk #0x2129
    0xD503201F, // 326: nop 
    0x8B000129, // 327: add x9, x9, x0
    0xF9000109, // 328: str x9, [x8]
    0xD2800029, // 329: mov x9, #1
    0xF9000509, // 330: str x9, [x8, #8]
    0xF900091F, // 331: str xzr, [x8, #0x10]
    0xD65F03C0, // 332: ret 
    0xA9BC7BFD, // 333: stp x29, x30, [sp, #-0x40]!
    0x910003FD, // 334: mov x29, sp
    0xA90153F3, // 335: stp x19, x20, [sp, #0x10]
    0xA9025BF5, // 336: stp x21, x22, [sp, #0x20]
    0xF9001BF7, // 337: str x23, [sp, #0x30]
    0xAA0803F3, // 338: mov x19, x8
    0xD42201A0, // 339: brk #0x100d
    0xD503201F, // 340: nop 
    0xD503201F, // 341: nop 
    0xB9800014, // 342: ldrsw x20, [x0]
    0xD42201C0, // 343: brk #0x100e
    0xD503201F, // 344: nop 
    0xD503201F, // 345: nop 
    0xF9400015, // 346: ldr x21, [x0]
    0xF900027F, // 347: str xzr, [x19]
    0xF9000674, // 348: str x20, [x19, #8]
    0xF9000A74, // 349: str x20, [x19, #0x10]
    0xF100029F, // 350: cmp x20, #0
    0x540002AD, // 351: b.le #0x5d0
    0xD2800309, // 352: mov x9, #0x18
    0x9B097E80, // 353: mul x0, x20, x9
    0xD4220040, // 354: brk #0x1002
    0xD503201F, // 355: nop 
    0xD503201F, // 356: nop 
    0xB4FFDE20, // 357: cbz x0, #0x158
    0xF9000260, // 358: str x0, [x19]
    0xD2800016, // 359: mov x22, #0
    0xAA0003F7, // 360: mov x23, x0
    0xF8767AA0, // 361: ldr x0, [x21, x22, lsl #3]
    0xF90002E0, // 362: str x0, [x23]
    0xD4220180, // 363: brk #0x100c
    0xD503201F, // 364: nop 
    0xD503201F, // 365: nop 
    0xF90006E0, // 366: str x0, [x23, #8]
    0xF9000AFF, // 367: str xzr, [x23, #0x10]
    0x910062F7, // 368: add x23, x23, #0x18
    0x910006D6, // 369: add x22, x22, #1
    0xEB1402DF, // 370: cmp x22, x20
    0x54FFFEC3, // 371: b.lo #0x5a4
    0xF9401BF7, // 372: ldr x23, [sp, #0x30]
    0xA9425BF5, // 373: ldp x21, x22, [sp, #0x20]
    0xA94153F3, // 374: ldp x19, x20, [sp, #0x10]
    0xA8C47BFD, // 375: ldp x29, x30, [sp], #0x40
    0xD65F03C0, // 376: ret 
    0xA9BE7BFD, // 377: stp x29, x30, [sp, #-0x20]!
    0x910003FD, // 378: mov x29, sp
    0xA90153F3, // 379: stp x19, x20, [sp, #0x10]
    0xAA0003F3, // 380: mov x19, x0
    0xF9400660, // 381: ldr x0, [x19, #8]
    0x91000400, // 382: add x0, x0, #1
    0xD4220040, // 383: brk #0x1002
    0xD503201F, // 384: nop 
    0xD503201F, // 385: nop 
    0xB4FFDA80, // 386: cbz x0, #0x158
    0xAA0003F4, // 387: mov x20, x0
    0xF9400662, // 388: ldr x2, [x19, #8]
    0xB40000A2, // 389: cbz x2, #0x628
    0xF9400261, // 390: ldr x1, [x19]
    0xD42200A0, // 391: brk #0x1005
    0xD503201F, // 392: nop 
    0xD503201F, // 393: nop 
    0xF9400669, // 394: ldr x9, [x19, #8]
    0x38296A9F, // 395: strb wzr, [x20, x9]
    0xAA1403E0, // 396: mov x0, x20
    0xA94153F3, // 397: ldp x19, x20, [sp, #0x10]
    0xA8C27BFD, // 398: ldp x29, x30, [sp], #0x20
    0xD65F03C0, // 399: ret 
    0xA9BD7BFD, // 400: stp x29, x30, [sp, #-0x30]!
    0x910003FD, // 401: mov x29, sp
    0xA90153F3, // 402: stp x19, x20, [sp, #0x10]
    0xF90013F5, // 403: str x21, [sp, #0x20]
    0xAA0003F3, // 404: mov x19, x0
    0xD4220140, // 405: brk #0x100a
    0xD503201F, // 406: nop 
    0xD503201F, // 407: nop 
    0xB9400000, // 408: ldr w0, [x0]
    0xD4220160, // 409: brk #0x100b
    0xD503201F, // 410: nop 
    0xD503201F, // 411: nop 
    0xAA0003F4, // 412: mov x20, x0
    0xD4220180, // 413: brk #0x100c
    0xD503201F, // 414: nop 
    0xD503201F, // 415: nop 
    0xAA0003F5, // 416: mov x21, x0
    0xF900027F, // 417: str xzr, [x19]
    0xF9000675, // 418: str x21, [x19, #8]
    0xF9000A75, // 419: str x21, [x19, #0x10]
    0xB4000195, // 420: cbz x21, #0x6c0
    0xAA1503E0, // 421: mov x0, x21
    0xD4220040, // 422: brk #0x1002
    0xD503201F, // 423: nop 
    0xD503201F, // 424: nop 
    0xB4FFD5A0, // 425: cbz x0, #0x158
    0xF9000260, // 426: str x0, [x19]
    0xAA1403E1, // 427: mov x1, x20
    0xAA1503E2, // 428: mov x2, x21
    0xD42200A0, // 429: brk #0x1005
    0xD503201F, // 430: nop 
    0xD503201F, // 431: nop 
    0xF94013F5, // 432: ldr x21, [sp, #0x20]
    0xA94153F3, // 433: ldp x19, x20, [sp, #0x10]
    0xA8C37BFD, // 434: ldp x29, x30, [sp], #0x30
    0xD65F03C0, // 435: ret 
    0xA9BC7BFD, // 436: stp x29, x30, [sp, #-0x40]!
    0x910003FD, // 437: mov x29, sp
    0xA90153F3, // 438: stp x19, x20, [sp, #0x10]
    0xA9025BF5, // 439: stp x21, x22, [sp, #0x20]
    0xA90363F7, // 440: stp x23, x24, [sp, #0x30]
    0xAA0803F3, // 441: mov x19, x8
    0x97FFFFBF, // 442: bl #0x5e4
    0xAA0003F4, // 443: mov x20, x0
    0xD2800001, // 444: mov x1, #0
    0xD42200E0, // 445: brk #0x1007
    0xD503201F, // 446: nop 
    0xD503201F, // 447: nop 
    0x93407C15, // 448: sxtw x21, w0
    0xB6F800F5, // 449: tbz x21, #0x3f, #0x720
    0xAA1303E0, // 450: mov x0, x19
    0x97FFFFCD, // 451: bl #0x640
    0xAA1403E0, // 452: mov x0, x20
    0x97FFFEA5, // 453: bl #0x1a8
    0xD2800020, // 454: mov x0, #1
    0x1400003F, // 455: b #0x818
    0xAA1403E0, // 456: mov x0, x20
    0x97FFFEA1, // 457: bl #0x1a8
    0xD2800016, // 458: mov x22, #0
    0xD2800017, // 459: mov x23, #0
    0xD2800018, // 460: mov x24, #0
    0xCB170309, // 461: sub x9, x24, x23
    0xF140053F, // 462: cmp x9, #1, lsl #12
    0x54000182, // 463: b.hs #0x76c
    0xD37FFB18, // 464: lsl x24, x24, #1
    0xD2A00029, // 465: mov x9, #0x10000
    0xEB09031F, // 466: cmp x24, x9
    0x9A983138, // 467: csel x24, x9, x24, lo
    0xAA1603E0, // 468: mov x0, x22
    0xAA1803E1, // 469: mov x1, x24
    0xD4220060, // 470: brk #0x1003
    0xD503201F, // 471: nop 
    0xD503201F, // 472: nop 
    0xB4FFCFA0, // 473: cbz x0, #0x158
    0xAA0003F6, // 474: mov x22, x0
    0xAA1503E0, // 475: mov x0, x21
    0x8B1702C1, // 476: add x1, x22, x23
    0xCB170302, // 477: sub x2, x24, x23
    0xD4220100, // 478: brk #0x1008
    0xD503201F, // 479: nop 
    0xD503201F, // 480: nop 
    0xF100001F, // 481: cmp x0, #0
    0x540001C0, // 482: b.eq #0x7c0
    0x5400006B, // 483: b.lt #0x798
    0x8B0002F7, // 484: add x23, x23, x0
    0x17FFFFE8, // 485: b #0x734
    0xAA1303E0, // 486: mov x0, x19
    0x97FFFFA9, // 487: bl #0x640
    0xAA1603E0, // 488: mov x0, x22
    0x97FFFE81, // 489: bl #0x1a8
    0xAA1503E0, // 490: mov x0, x21
    0xD4220120, // 491: brk #0x1009
    0xD503201F, // 492: nop 
    0xD503201F, // 493: nop 
    0xD2800020, // 494: mov x0, #1
    0x14000017, // 495: b #0x818
    0xAA1503E0, // 496: mov x0, x21
    0xD4220120, // 497: brk #0x1009
    0xD503201F, // 498: nop 
    0xD503201F, // 499: nop 
    0xB5000117, // 500: cbnz x23, #0x7f0
    0xAA1603E0, // 501: mov x0, x22
    0x97FFFE74, // 502: bl #0x1a8
    0xF900027F, // 503: str xzr, [x19]
    0xF900067F, // 504: str xzr, [x19, #8]
    0xF9000A7F, // 505: str xzr, [x19, #0x10]
    0xD2800000, // 506: mov x0, #0
    0x1400000B, // 507: b #0x818
    0xAA1603E0, // 508: mov x0, x22
    0xAA1703E1, // 509: mov x1, x23
    0xD4220060, // 510: brk #0x1003
    0xD503201F, // 511: nop 
    0xD503201F, // 512: nop 
    0xB4FFCAA0, // 513: cbz x0, #0x158
    0xF9000260, // 514: str x0, [x19]
    0xF9000677, // 515: str x23, [x19, #8]
    0xF9000A77, // 516: str x23, [x19, #0x10]
    0xD2800000, // 517: mov x0, #0
    0xA94363F7, // 518: ldp x23, x24, [sp, #0x30]
    0xA9425BF5, // 519: ldp x21, x22, [sp, #0x20]
    0xA94153F3, // 520: ldp x19, x20, [sp, #0x10]
    0xA8C47BFD, // 521: ldp x29, x30, [sp], #0x40
    0xD65F03C0, // 522: ret 
    0xA9BC7BFD, // 523: stp x29, x30, [sp, #-0x40]!
    0x910003FD, // 524: mov x29, sp
    0xA90153F3, // 525: stp x19, x20, [sp, #0x10]
    0xA9025BF5, // 526: stp x21, x22, [sp, #0x20]
    0xA90363F7, // 527: stp x23, x24, [sp, #0x30]
    0xAA0803F3, // 528: mov x19, x8
    0xF9400037, // 529: ldr x23, [x1]
    0xF9400438, // 530: ldr x24, [x1, #8]
    0x97FFFF66, // 531: bl #0x5e4
    0xAA0003F4, // 532: mov x20, x0
    0xD280C021, // 533: mov x1, #0x601
    0xD2803482, // 534: mov x2, #0x1a4
    0xD10043FF, // 535: sub sp, sp, #0x10
    0xF90003E2, // 536: str x2, [sp]
    0xD42200E0, // 537: brk #0x1007
    0xD503201F, // 538: nop 
    0xD503201F, // 539: nop 
    0x910043FF, // 540: add sp, sp, #0x10
    0x93407C15, // 541: sxtw x21, w0
    0xB6F800F5, // 542: tbz x21, #0x3f, #0x894
    0xAA1303E0, // 543: mov x0, x19
    0x97FFFF70, // 544: bl #0x640
    0xAA1403E0, // 545: mov x0, x20
    0x97FFFE48, // 546: bl #0x1a8
    0xD2800020, // 547: mov x0, #1
    0x1400001E, // 548: b #0x908
    0xAA1403E0, // 549: mov x0, x20
    0x97FFFE44, // 550: bl #0x1a8
    0xD2800016, // 551: mov x22, #0
    0xEB1802DF, // 552: cmp x22, x24
    0x54000262, // 553: b.hs #0x8f0
    0xAA1503E0, // 554: mov x0, x21
    0x8B1602E1, // 555: add x1, x23, x22
    0xCB160302, // 556: sub x2, x24, x22
    0xD4220000, // 557: brk #0x1000
    0xD503201F, // 558: nop 
    0xD503201F, // 559: nop 
    0xF100001F, // 560: cmp x0, #0
    0x5400006D, // 561: b.le #0x8d0
    0x8B0002D6, // 562: add x22, x22, x0
    0x17FFFFF5, // 563: b #0x8a0
    0xAA1303E0, // 564: mov x0, x19
    0x97FFFF5B, // 565: bl #0x640
    0xAA1503E0, // 566: mov x0, x21
    0xD4220120, // 567: brk #0x1009
    0xD503201F, // 568: nop 
    0xD503201F, // 569: nop 
    0xD2800020, // 570: mov x0, #1
    0x14000007, // 571: b #0x908
    0xAA1503E0, // 572: mov x0, x21
    0xD4220120, // 573: brk #0x1009
    0xD503201F, // 574: nop 
    0xD503201F, // 575: nop 
    0xF9000278, // 576: str x24, [x19]
    0xD2800000, // 577: mov x0, #0
    0xA94363F7, // 578: ldp x23, x24, [sp, #0x30]
    0xA9425BF5, // 579: ldp x21, x22, [sp, #0x20]
    0xA94153F3, // 580: ldp x19, x20, [sp, #0x10]
    0xA8C47BFD, // 581: ldp x29, x30, [sp], #0x40
    0xD65F03C0, // 582: ret 
    0xF9400429, // 583: ldr x9, [x1, #8]
    0xB4000789, // 584: cbz x9, #0xa10
    0xA9BC7BFD, // 585: stp x29, x30, [sp, #-0x40]!
    0x910003FD, // 586: mov x29, sp
    0xA90153F3, // 587: stp x19, x20, [sp, #0x10]
    0xA9025BF5, // 588: stp x21, x22, [sp, #0x20]
    0xA90363F7, // 589: stp x23, x24, [sp, #0x30]
    0xAA0003F3, // 590: mov x19, x0
    0xF9400034, // 591: ldr x20, [x1]
    0xAA0903F5, // 592: mov x21, x9
    0xF940026A, // 593: ldr x10, [x19]
    0xEB0A029F, // 594: cmp x20, x10
    0x9A9F17F7, // 595: cset x23, eq
    0xF9400669, // 596: ldr x9, [x19, #8]
    0x8B150136, // 597: add x22, x9, x21
    0xF9400A6A, // 598: ldr x10, [x19, #0x10]
    0xEB0A02DF, // 599: cmp x22, x10
    0x540003E9, // 600: b.ls #0x9dc
    0xD37FF958, // 601: lsl x24, x10, #1
    0xEB16031F, // 602: cmp x24, x22
    0x9A9832D8, // 603: csel x24, x22, x24, lo
    0xD280020B, // 604: mov x11, #0x10
    0xEB0B031F, // 605: cmp x24, x11
    0x9A983178, // 606: csel x24, x11, x24, lo
    0xB400016A, // 607: cbz x10, #0x9a8
    0xF9400260, // 608: ldr x0, [x19]
    0xAA1803E1, // 609: mov x1, x24
    0xD4220060, // 610: brk #0x1003
    0xD503201F, // 611: nop 
    0xD503201F, // 612: nop 
    0xB4FFBE20, // 613: cbz x0, #0x158
    0xF9000260, // 614: str x0, [x19]
    0xB40001F7, // 615: cbz x23, #0x9d8
    0xAA0003F4, // 616: mov x20, x0
    0x1400000D, // 617: b #0x9d8
    0xAA1803E0, // 618: mov x0, x24
    0xD4220040, // 619: brk #0x1002
    0xD503201F, // 620: nop 
    0xD503201F, // 621: nop 
    0xB4FFBD00, // 622: cbz x0, #0x158
    0xF9400261, // 623: ldr x1, [x19]
    0xF9000260, // 624: str x0, [x19]
    0xF9400662, // 625: ldr x2, [x19, #8]
    0xB4000082, // 626: cbz x2, #0x9d8
    0xD42200A0, // 627: brk #0x1005
    0xD503201F, // 628: nop 
    0xD503201F, // 629: nop 
    0xF9000A78, // 630: str x24, [x19, #0x10]
    0xF9400260, // 631: ldr x0, [x19]
    0xF9400669, // 632: ldr x9, [x19, #8]
    0x8B090000, // 633: add x0, x0, x9
    0xAA1403E1, // 634: mov x1, x20
    0xAA1503E2, // 635: mov x2, x21
    0xD42200A0, // 636: brk #0x1005
    0xD503201F, // 637: nop 
    0xD503201F, // 638: nop 
    0xF9000676, // 639: str x22, [x19, #8]
    0xA94363F7, // 640: ldp x23, x24, [sp, #0x30]
    0xA9425BF5, // 641: ldp x21, x22, [sp, #0x20]
    0xA94153F3, // 642: ldp x19, x20, [sp, #0x10]
    0xA8C47BFD, // 643: ldp x29, x30, [sp], #0x40
    0xD65F03C0, // 644: ret 
    0xA9BE7BFD, // 645: stp x29, x30, [sp, #-0x20]!
    0x910003FD, // 646: mov x29, sp
    0xA90153F3, // 647: stp x19, x20, [sp, #0x10]
    0xAA0803F3, // 648: mov x19, x8
    0x97FFFEF0, // 649: bl #0x5e4
    0xAA0003F4, // 650: mov x20, x0
    0xD2803DA1, // 651: mov x1, #0x1ed
    0xD42201E0, // 652: brk #0x100f
    0xD503201F, // 653: nop 
    0xD503201F, // 654: nop 
    0x340000E0, // 655: cbz w0, #0xa58
    0xAA1303E0, // 656: mov x0, x19
    0x97FFFEFF, // 657: bl #0x640
    0xAA1403E0, // 658: mov x0, x20
    0x97FFFDD7, // 659: bl #0x1a8
    0xD2800020, // 660: mov x0, #1
    0x14000005, // 661: b #0xa68
    0xAA1403E0, // 662: mov x0, x20
    0x97FFFDD3, // 663: bl #0x1a8
    0xF900027F, // 664: str xzr, [x19]
    0xD2800000, // 665: mov x0, #0
    0xA94153F3, // 666: ldp x19, x20, [sp, #0x10]
    0xA8C27BFD, // 667: ldp x29, x30, [sp], #0x20
    0xD65F03C0, // 668: ret 
];
