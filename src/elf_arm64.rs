// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Writes Linux ARM64 executables: static ELF files that need no
//! interpreter and no libraries.
//!
//! The code comes from the same generator as on macOS, unchanged. There
//! it calls sixteen functions of the system library through addresses the
//! loader fills in. Here those functions are part of the executable (see
//! `runtime/linux_arm64/runtime.c`), so every such call becomes a direct
//! one.
//!
//! The file is mapped as it is:
//!
//! ```text
//! file offset   address             contents
//! 0             BASE                ELF header and program headers
//! TEXT_OFFSET   BASE + TEXT_OFFSET  the system library (fixed size)
//!               ...                 a jump to the program's `main`
//!               ...                 the program's code
//!               ...                 the program's constant data
//! (not in file) MEMORY              the library's writable memory
//! ```
//!
//! The file is readable and executable. The library's memory is a second
//! piece, readable and writable, that takes no room in the file: the
//! system provides it filled with zeros. The library was linked for
//! exactly these addresses (see `runtime/linux_arm64/runtime.ld`), so it
//! is copied in unchanged; only the program's references to it and to its
//! own data are filled in here.

use crate::arm64::{Code, Reloc};
use crate::runtime_linux_arm64::{BASE, IMPORT_OFFSETS, MEMORY, MEMORY_SIZE, START, TEXT, TEXT_OFFSET};

/// ARM64 Linux systems use pages of 4, 16 or 64 KiB. Pieces aligned for
/// the largest load on all of them.
const PAGE: u64 = 0x1_0000;
const HEADER_SIZE: u64 = 64;
const PROGRAM_HEADER_SIZE: u64 = 56;
const PROGRAM_HEADERS: u64 = 3;
/// The jump to `main` in front of the program's code: one instruction.
const JUMP_SIZE: u64 = 4;

const PT_LOAD: u32 = 1;
const PT_GNU_STACK: u32 = 0x6474_E551;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

const NOP: u32 = 0xD503_201F;

/// Where each part of an executable sits in the file. Addresses are
/// `BASE` plus the offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layout {
    /// The jump to `main`, right after the library.
    jump: u64,
    code: u64,
    data: u64,
    /// End of the file, and of what is mapped readable and executable.
    file_size: u64,
}

fn layout(code_len: u64, data_len: u64) -> Layout {
    let jump = TEXT_OFFSET + TEXT.len() as u64;
    let code = jump + JUMP_SIZE;
    let data = code + code_len;
    Layout { jump, code, data, file_size: data + data_len }
}

struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn program_header(&mut self, kind: u32, flags: u32, offset: u64, address: u64, file_size: u64, memory_size: u64, align: u64) {
        self.u32(kind);
        self.u32(flags);
        self.u64(offset);
        self.u64(address); // virtual address
        self.u64(address); // physical address, unused
        self.u64(file_size);
        self.u64(memory_size);
        self.u64(align);
    }
}

/// The 26-bit field of `b` and `bl`: the distance from the instruction at
/// file offset `from` to the one at `to`, in instructions.
fn branch_distance(from: u64, to: u64) -> u32 {
    let distance = to as i64 - from as i64;
    debug_assert!(distance % 4 == 0);
    assert!((-(1 << 27)..1 << 27).contains(&distance), "the program is too large for a direct call");
    ((distance >> 2) as u32) & 0x03FF_FFFF
}

/// `adrp rd, target`: the 4 KiB page of `target`, relative to `pc`.
fn adrp(rd: u32, pc: u64, target: u64) -> u32 {
    let delta = ((target >> 12) as i64 - (pc >> 12) as i64) as u64;
    0x9000_0000 | ((delta & 3) as u32) << 29 | (((delta >> 2) & 0x7_FFFF) as u32) << 5 | rd
}

/// Builds the complete executable for a program.
pub fn build_executable(code: &Code) -> Vec<u8> {
    let at = layout(code.words.len() as u64 * 4, code.data.len() as u64);
    // The library's memory is at a fixed address above the program.
    assert!(BASE + at.file_size <= MEMORY, "the program is too large for this executable layout");
    debug_assert!(HEADER_SIZE + PROGRAM_HEADERS * PROGRAM_HEADER_SIZE <= TEXT_OFFSET);
    debug_assert!(at.jump % 4 == 0 && MEMORY % PAGE == 0 && BASE % PAGE == 0);

    let mut w = Writer { bytes: Vec::with_capacity(at.file_size as usize) };

    // ELF header.
    w.bytes.extend_from_slice(&[0x7F, b'E', b'L', b'F', 2, 1, 1, 0]); // 64-bit, little-endian, version 1, System V
    w.bytes.extend_from_slice(&[0; 8]);
    w.u16(2); // an executable at fixed addresses
    w.u16(183); // ARM64
    w.u32(1);
    w.u64(BASE + TEXT_OFFSET + START as u64); // where the process starts
    w.u64(HEADER_SIZE); // program headers follow this header
    w.u64(0); // no section headers
    w.u32(0);
    w.u16(HEADER_SIZE as u16);
    w.u16(PROGRAM_HEADER_SIZE as u16);
    w.u16(PROGRAM_HEADERS as u16);
    w.u16(64); // size a section header would have
    w.u16(0);
    w.u16(0);

    // Code and constant data, then the library's memory, then a note
    // that the stack holds no code. The memory has nothing in the file;
    // its offset only has to agree with its address within a page.
    w.program_header(PT_LOAD, PF_R | PF_X, 0, BASE, at.file_size, at.file_size, PAGE);
    w.program_header(PT_LOAD, PF_R | PF_W, 0, MEMORY, 0, MEMORY_SIZE, PAGE);
    w.program_header(PT_GNU_STACK, PF_R | PF_W, 0, 0, 0, 0, 16);

    w.bytes.resize(TEXT_OFFSET as usize, 0);
    w.bytes.extend_from_slice(TEXT);

    // The library calls this spot; from here the program's `main` is a
    // fixed distance away.
    debug_assert_eq!(w.bytes.len() as u64, at.jump);
    w.u32(0x1400_0000 | branch_distance(at.jump, at.code + code.entry as u64 * 4)); // b main

    // The words go in as they are, and then the few instructions that
    // depend on final addresses are filled in where they landed.
    for &word in &code.words {
        w.u32(word);
    }
    let mut put = |at_word: u32, word: u32| {
        let start = at.code as usize + at_word as usize * 4;
        w.bytes[start..start + 4].copy_from_slice(&word.to_le_bytes());
    };
    for reloc in &code.relocs {
        match *reloc {
            Reloc::Import { at: word, import } => {
                // The generator left room for the three instructions macOS
                // needs. A direct call takes one.
                let from = at.code + word as u64 * 4;
                let target = TEXT_OFFSET + IMPORT_OFFSETS[import as usize] as u64;
                put(word, 0x9400_0000 | branch_distance(from, target)); // bl
                put(word + 1, NOP);
                put(word + 2, NOP);
            }
            Reloc::Data { at: word, offset, reg } => {
                let pc = BASE + at.code + word as u64 * 4;
                let target = BASE + at.data + offset as u64;
                put(word, adrp(reg, pc, target));
                put(word + 1, 0x9100_0000 | ((target & 0xFFF) as u32) << 10 | reg << 5 | reg); // add reg, reg, #lo12
            }
        }
    }
    w.bytes.extend_from_slice(&code.data);
    debug_assert_eq!(w.bytes.len() as u64, at.file_size);
    w.bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_linux_arm64::IMPORT_NAMES;

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn u64_at(bytes: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
    }

    /// The file offset a `b` or `bl` at file offset `at` goes to.
    fn branch_target(bytes: &[u8], at: usize) -> i64 {
        let field = (u32_at(bytes, at) & 0x03FF_FFFF) as i64;
        let distance = if field >= 1 << 25 { field - (1 << 26) } else { field };
        at as i64 + distance * 4
    }

    /// Nine words of stand-in code: `main` at word 1, a call of import 7
    /// at word 2, the address of `data[3]` into x9 at word 5, a call of
    /// import 0 at word 6 that overlaps nothing.
    fn sample() -> Code {
        Code {
            words: vec![0xD65F_03C0, 0xD280_0540, 0xAAAA_AAAA, 0xBBBB_BBBB, 0xCCCC_CCCC, 0xDDDD_DDDD, 0xEEEE_EEEE, 0xD65F_03C0, 0x1234_5678],
            entry: 1,
            data: b"hello".to_vec(),
            relocs: vec![Reloc::Import { at: 2, import: 7 }, Reloc::Data { at: 5, offset: 3, reg: 9 }],
        }
    }

    #[test]
    fn the_library_matches_the_layout_and_the_imports() {
        // The library provides exactly the imports the ARM64 code uses, in
        // the same order; each has its own address inside the library, at
        // an instruction boundary; and the headers fit in front of it.
        assert_eq!(IMPORT_NAMES, crate::runtime::IMPORTS);
        assert_eq!(TEXT.len() % 4, 0);
        assert!((START as usize) < TEXT.len() && START % 4 == 0);
        assert!(IMPORT_OFFSETS.iter().all(|&offset| (offset as usize) < TEXT.len() && offset % 4 == 0));
        let mut sorted = IMPORT_OFFSETS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), IMPORT_OFFSETS.len(), "every function has its own address");
        assert!(HEADER_SIZE + PROGRAM_HEADERS * PROGRAM_HEADER_SIZE <= TEXT_OFFSET);
        assert!(MEMORY_SIZE > 0 && MEMORY_SIZE <= 4096);
        assert_eq!(BASE % PAGE, 0);
        assert_eq!(MEMORY % PAGE, 0);
    }

    #[test]
    fn layout_places_the_parts_in_order() {
        let at = layout(36, 5);
        assert_eq!(at.jump, TEXT_OFFSET + TEXT.len() as u64);
        assert_eq!(at.code, at.jump + 4);
        assert_eq!(at.data, at.code + 36);
        assert_eq!(at.file_size, at.data + 5);
        // Nothing at all still gives a valid file.
        let empty = layout(0, 0);
        assert_eq!(empty.data, empty.code);
        assert_eq!(empty.file_size, empty.code);
    }

    #[test]
    fn header_describes_a_static_arm64_executable() {
        let file = build_executable(&sample());
        let at = layout(36, 5);
        assert_eq!(file.len() as u64, at.file_size);
        assert_eq!(&file[..8], &[0x7F, b'E', b'L', b'F', 2, 1, 1, 0]);
        assert!(file[8..16].iter().all(|&byte| byte == 0));
        assert_eq!(u16_at(&file, 16), 2); // executable
        assert_eq!(u16_at(&file, 18), 183); // ARM64
        assert_eq!(u32_at(&file, 20), 1);
        assert_eq!(u64_at(&file, 24), BASE + TEXT_OFFSET + START as u64); // entry point
        assert_eq!(u64_at(&file, 32), 64); // program headers
        assert_eq!(u64_at(&file, 40), 0); // no section headers
        assert_eq!(u32_at(&file, 48), 0); // no flags
        assert_eq!((u16_at(&file, 52), u16_at(&file, 54), u16_at(&file, 56)), (64, 56, 3));
        assert_eq!((u16_at(&file, 58), u16_at(&file, 60), u16_at(&file, 62)), (64, 0, 0));

        // Readable and executable: the whole file.
        let rx = 64;
        assert_eq!((u32_at(&file, rx), u32_at(&file, rx + 4)), (PT_LOAD, PF_R | PF_X));
        assert_eq!((u64_at(&file, rx + 8), u64_at(&file, rx + 16), u64_at(&file, rx + 24)), (0, BASE, BASE));
        assert_eq!((u64_at(&file, rx + 32), u64_at(&file, rx + 40)), (at.file_size, at.file_size));
        assert_eq!(u64_at(&file, rx + 48), 0x1_0000);
        // Readable and writable: the library's memory, with nothing in
        // the file behind it.
        let rw = 64 + 56;
        assert_eq!((u32_at(&file, rw), u32_at(&file, rw + 4)), (PT_LOAD, PF_R | PF_W));
        assert_eq!((u64_at(&file, rw + 16), u64_at(&file, rw + 24)), (MEMORY, MEMORY));
        assert_eq!((u64_at(&file, rw + 32), u64_at(&file, rw + 40)), (0, MEMORY_SIZE));
        assert_eq!(u64_at(&file, rw + 48), 0x1_0000);
        // A file offset and its address must agree within a page, for
        // every page size the system may use.
        for header in [rx, rw] {
            assert_eq!(u64_at(&file, header + 8) % 0x1_0000, u64_at(&file, header + 16) % 0x1_0000);
        }
        // The stack is not executable.
        let stack = 64 + 2 * 56;
        assert_eq!((u32_at(&file, stack), u32_at(&file, stack + 4)), (PT_GNU_STACK, PF_R | PF_W));
        assert_eq!((u64_at(&file, stack + 32), u64_at(&file, stack + 40)), (0, 0));
        // The headers end before the library, with zeros between.
        assert!(file[64 + 3 * 56..TEXT_OFFSET as usize].iter().all(|&byte| byte == 0));
        // The two mapped pieces do not overlap in memory, on any page size.
        assert!((BASE + at.file_size).div_ceil(0x1_0000) * 0x1_0000 <= MEMORY);
    }

    #[test]
    fn contents_and_references_are_resolved() {
        let code = sample();
        let file = build_executable(&code);
        let at = layout(36, 5);
        let text = TEXT_OFFSET as usize;
        assert_eq!(&file[text..text + TEXT.len()], &TEXT[..]);

        // The jump lands on `main`.
        let jump = at.jump as usize;
        assert_eq!(u32_at(&file, jump) >> 26, 0b000101); // b
        assert_eq!(branch_target(&file, jump), at.code as i64 + 4);

        // The code is copied, with only the recorded instructions changed.
        let start = at.code as usize;
        for (i, &word) in code.words.iter().enumerate() {
            if !(2..7).contains(&i) {
                assert_eq!(u32_at(&file, start + i * 4), word, "word {i}");
            }
        }
        // The call goes straight to the library's function, backwards,
        // and the two instructions macOS needs besides do nothing.
        assert_eq!(u32_at(&file, start + 8) >> 26, 0b100101); // bl
        assert_eq!(branch_target(&file, start + 8), (TEXT_OFFSET + IMPORT_OFFSETS[7] as u64) as i64);
        assert_eq!(u32_at(&file, start + 12), NOP);
        assert_eq!(u32_at(&file, start + 16), NOP);
        // The data reference: adrp x9 and add x9, x9, #lo12 give the
        // address of the fourth byte of the data.
        let target = BASE + at.data + 3;
        let pc = BASE + at.code + 20;
        let first = u32_at(&file, start + 20);
        let second = u32_at(&file, start + 24);
        assert_eq!(first & 0x9F00_001F, 0x9000_0009);
        let pages = (((first >> 5) & 0x7_FFFF) << 2 | (first >> 29) & 3) as u64;
        assert_eq!((pc >> 12) + pages, target >> 12);
        assert_eq!(second, 0x9100_0129 | ((target & 0xFFF) as u32) << 10);
        assert_eq!(&file[at.data as usize..], b"hello");
    }

    #[test]
    fn every_import_gets_a_direct_call() {
        for import in 0..IMPORT_OFFSETS.len() as u32 {
            let code = Code { words: vec![0; 4], entry: 3, data: Vec::new(), relocs: vec![Reloc::Import { at: 0, import }] };
            let file = build_executable(&code);
            let start = layout(16, 0).code as usize;
            assert_eq!(branch_target(&file, start), (TEXT_OFFSET + IMPORT_OFFSETS[import as usize] as u64) as i64);
            assert_eq!(branch_target(&file, start - 4), start as i64 + 12);
        }
    }

    #[test]
    fn branch_distances() {
        assert_eq!(branch_distance(0x100, 0x110), 4);
        assert_eq!(branch_distance(0x100, 0x100), 0);
        assert_eq!(branch_distance(0x100, 0xF0), 0x03FF_FFFC);
        assert_eq!(0x9400_0000 | branch_distance(0x2000, 0x1000), 0x97FF_FC00); // bl .-0x1000
    }

    #[test]
    fn adrp_reaches_other_pages_in_both_directions() {
        assert_eq!(adrp(16, 0x40_1588, 0x40_8000), 0xF000_0030);
        assert_eq!(adrp(1, 0x40_1590, 0x40_160C), 0x9000_0001);
        assert_eq!(adrp(0, 0x40_5000, 0x40_1000), 0x90FF_FFE0);
    }
}
