// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Writes Linux x86-64 executables: static ELF files that need no
//! interpreter and no libraries.
//!
//! The file is mapped as it is, in two pieces:
//!
//! ```text
//! file offset   address             contents
//! 0             BASE                ELF header and program headers
//! TEXT_OFFSET   BASE + TEXT_OFFSET  the runtime (fixed size)
//!               ...                 a jump to the program's `main`
//!               ...                 the program's code
//!               ...                 the program's string literals
//! next page     MEMORY              one page of zeros: the runtime's
//!                                   writable memory
//! ```
//!
//! The first piece is readable and executable, the second readable and
//! writable. The runtime was linked for exactly these addresses (see
//! `runtime/linux_x64/runtime.ld`), so it is copied in unchanged; only
//! the program's references to it and to its own data are filled in here.

use crate::runtime_x64::{BASE, ENTRIES, MEMORY, MEMORY_SIZE, START, TEXT, TEXT_OFFSET};
use crate::x64::Code;

const PAGE: u64 = 4096;
const HEADER_SIZE: u64 = 64;
const PROGRAM_HEADER_SIZE: u64 = 56;
const PROGRAM_HEADERS: u64 = 3;
/// Room for the jump to `main` in front of the program's code.
const JUMP_SIZE: u64 = 16;

const PT_LOAD: u32 = 1;
const PT_GNU_STACK: u32 = 0x6474_E551;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

fn align_up(value: u64, to: u64) -> u64 {
    value.div_ceil(to) * to
}

/// Where each part of an executable sits in the file. Addresses are
/// `BASE` plus the offset, except for the runtime's memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layout {
    /// The jump to `main`, right after the runtime.
    jump: u64,
    code: u64,
    data: u64,
    /// End of everything that is mapped readable and executable.
    text_end: u64,
    /// The page of zeros that becomes the runtime's memory.
    memory: u64,
    file_size: u64,
}

fn layout(code_len: u64, data_len: u64) -> Layout {
    let jump = TEXT_OFFSET + TEXT.len() as u64;
    let code = jump + JUMP_SIZE;
    let data = align_up(code + code_len, 16);
    let text_end = data + data_len;
    let memory = align_up(text_end, PAGE);
    Layout { jump, code, data, text_end, memory, file_size: memory + align_up(MEMORY_SIZE.max(1), PAGE) }
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

    fn pad_to(&mut self, offset: u64, byte: u8) {
        debug_assert!(self.bytes.len() as u64 <= offset);
        self.bytes.resize(offset as usize, byte);
    }

    fn program_header(&mut self, kind: u32, flags: u32, offset: u64, address: u64, size: u64, align: u64) {
        self.u32(kind);
        self.u32(flags);
        self.u64(offset);
        self.u64(address); // virtual address
        self.u64(address); // physical address, unused
        self.u64(size); // size in the file
        self.u64(size); // size in memory
        self.u64(align);
    }
}

/// The 4-byte distance an instruction stores to reach `target`, when the
/// field holding it is at `field` and the instruction ends right after.
fn relative(target: u64, field: u64) -> [u8; 4] {
    let distance = target as i64 - (field as i64 + 4);
    i32::try_from(distance).expect("the executable is far smaller than 2 GB").to_le_bytes()
}

/// Builds the complete executable for a program.
pub fn build_executable(code: &Code) -> Vec<u8> {
    let at = layout(code.bytes.len() as u64, code.data.len() as u64);
    // The runtime's memory is at a fixed address above the program.
    assert!(BASE + at.text_end <= MEMORY, "the program is too large for this executable layout");
    assert!(MEMORY_SIZE <= PAGE);
    debug_assert!(HEADER_SIZE + PROGRAM_HEADERS * PROGRAM_HEADER_SIZE <= TEXT_OFFSET);
    debug_assert!(at.jump % 16 == 0 && MEMORY % PAGE == 0 && BASE % PAGE == 0);

    let mut w = Writer { bytes: Vec::with_capacity(at.file_size as usize) };

    // ELF header.
    w.bytes.extend_from_slice(&[0x7F, b'E', b'L', b'F', 2, 1, 1, 0]); // 64-bit, little-endian, version 1, System V
    w.bytes.extend_from_slice(&[0; 8]);
    w.u16(2); // an executable at fixed addresses
    w.u16(62); // x86-64
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

    // Code and constant data, then the runtime's memory, then a note
    // that the stack holds no code.
    w.program_header(PT_LOAD, PF_R | PF_X, 0, BASE, at.text_end, PAGE);
    w.program_header(PT_LOAD, PF_R | PF_W, at.memory, MEMORY, at.file_size - at.memory, PAGE);
    w.program_header(PT_GNU_STACK, PF_R | PF_W, 0, 0, 0, 16);

    w.pad_to(TEXT_OFFSET, 0);
    w.bytes.extend_from_slice(TEXT);

    // The runtime calls this spot; from here the program's `main` is a
    // fixed distance away.
    debug_assert_eq!(w.bytes.len() as u64, at.jump);
    w.bytes.push(0xE9); // jmp rel32
    w.bytes.extend_from_slice(&relative(at.code + code.entry as u64, at.jump + 1));
    w.pad_to(at.code, 0xCC);

    w.bytes.extend_from_slice(&code.bytes);
    for &(field, routine) in &code.runtime_calls {
        let target = TEXT_OFFSET + ENTRIES[routine as usize] as u64;
        let field = at.code + field as u64;
        w.bytes[field as usize..field as usize + 4].copy_from_slice(&relative(target, field));
    }
    for &(field, offset) in &code.data_refs {
        let field = at.code + field as u64;
        w.bytes[field as usize..field as usize + 4].copy_from_slice(&relative(at.data + offset as u64, field));
    }
    w.pad_to(at.data, 0xCC);
    w.bytes.extend_from_slice(&code.data);

    w.pad_to(at.file_size, 0);
    w.bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_x64::{ENTRY_COUNT, PRINT_I64, STR_APPEND};

    fn u16_at(bytes: &[u8], at: usize) -> u16 {
        u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn u64_at(bytes: &[u8], at: usize) -> u64 {
        u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
    }

    fn i32_at(bytes: &[u8], at: usize) -> i64 {
        i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as i64
    }

    /// 21 bytes of stand-in code with a call field at 1, a data
    /// reference field at 8 and another call field at 13; `main` at 5.
    fn sample() -> Code {
        Code {
            bytes: vec![0xE8, 0, 0, 0, 0, 0x48, 0x8D, 0x3D, 0, 0, 0, 0, 0xE8, 0, 0, 0, 0, 0x31, 0xC0, 0xC3, 0x90],
            entry: 5,
            data: b"hello".to_vec(),
            runtime_calls: vec![(1, PRINT_I64), (13, STR_APPEND)],
            data_refs: vec![(8, 2)],
        }
    }

    #[test]
    fn the_runtime_matches_the_layout() {
        // The runtime's size keeps the program's code aligned, its start
        // routine and every entry lie inside it, and the headers fit in
        // front of it.
        assert_eq!(TEXT.len() % 16, 0);
        assert!((START as usize) < TEXT.len());
        assert_eq!(ENTRIES.len(), ENTRY_COUNT);
        assert!(ENTRIES.iter().all(|&entry| (entry as usize) < TEXT.len()));
        let mut sorted = ENTRIES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ENTRY_COUNT, "every routine has its own address");
        assert!(HEADER_SIZE + PROGRAM_HEADERS * PROGRAM_HEADER_SIZE <= TEXT_OFFSET);
        assert!(MEMORY_SIZE > 0 && MEMORY_SIZE <= PAGE);
    }

    #[test]
    fn layout_places_the_parts_in_order() {
        let at = layout(21, 5);
        assert_eq!(at.jump, TEXT_OFFSET + TEXT.len() as u64);
        assert_eq!(at.code, at.jump + 16);
        assert_eq!(at.data, at.code + 32); // 21 bytes of code, rounded up to 16
        assert_eq!(at.text_end, at.data + 5);
        assert_eq!(at.memory % PAGE, 0);
        assert!(at.memory >= at.text_end && at.memory - at.text_end < PAGE);
        assert_eq!(at.file_size, at.memory + PAGE);
        // Nothing at all still gives a valid file.
        let empty = layout(0, 0);
        assert_eq!(empty.data, empty.code);
        assert_eq!(empty.file_size, empty.memory + PAGE);
    }

    #[test]
    fn header_describes_a_static_x86_64_executable() {
        let file = build_executable(&sample());
        let at = layout(21, 5);
        assert_eq!(file.len() as u64, at.file_size);
        assert_eq!(&file[..8], &[0x7F, b'E', b'L', b'F', 2, 1, 1, 0]);
        assert_eq!(u16_at(&file, 16), 2); // executable
        assert_eq!(u16_at(&file, 18), 62); // x86-64
        assert_eq!(u64_at(&file, 24), BASE + TEXT_OFFSET + START as u64); // entry point
        assert_eq!(u64_at(&file, 32), 64); // program headers
        assert_eq!(u64_at(&file, 40), 0); // no section headers
        assert_eq!((u16_at(&file, 52), u16_at(&file, 54), u16_at(&file, 56)), (64, 56, 3));
        assert_eq!(u16_at(&file, 60), 0);

        // Readable and executable: the whole file up to the end of the data.
        let rx = 64;
        assert_eq!((u32_at(&file, rx), u32_at(&file, rx + 4)), (PT_LOAD, PF_R | PF_X));
        assert_eq!((u64_at(&file, rx + 8), u64_at(&file, rx + 16)), (0, BASE));
        assert_eq!((u64_at(&file, rx + 32), u64_at(&file, rx + 40)), (at.text_end, at.text_end));
        assert_eq!(u64_at(&file, rx + 48), PAGE);
        // Readable and writable: one page of zeros at the runtime's address.
        let rw = 64 + 56;
        assert_eq!((u32_at(&file, rw), u32_at(&file, rw + 4)), (PT_LOAD, PF_R | PF_W));
        assert_eq!((u64_at(&file, rw + 8), u64_at(&file, rw + 16)), (at.memory, MEMORY));
        assert_eq!((u64_at(&file, rw + 32), u64_at(&file, rw + 40)), (PAGE, PAGE));
        // A file offset and its address must agree within a page.
        assert_eq!(u64_at(&file, rw + 8) % PAGE, u64_at(&file, rw + 16) % PAGE);
        assert!(file[at.memory as usize..].iter().all(|&byte| byte == 0));
        // The stack is not executable.
        let stack = 64 + 2 * 56;
        assert_eq!((u32_at(&file, stack), u32_at(&file, stack + 4)), (PT_GNU_STACK, PF_R | PF_W));
        // The two mapped pieces do not overlap in memory.
        assert!(BASE + at.text_end <= MEMORY);
    }

    #[test]
    fn contents_and_references_are_resolved() {
        let code = sample();
        let file = build_executable(&code);
        let at = layout(21, 5);
        let text = TEXT_OFFSET as usize;
        assert_eq!(&file[text..text + TEXT.len()], &TEXT[..]);

        // The jump lands on `main`.
        let jump = at.jump as usize;
        assert_eq!(file[jump], 0xE9);
        assert_eq!(jump as i64 + 5 + i32_at(&file, jump + 1), (at.code + 5) as i64);
        assert!(file[jump + 5..at.code as usize].iter().all(|&byte| byte == 0xCC));

        // The code is copied, with only the recorded fields changed.
        let start = at.code as usize;
        for (i, &byte) in code.bytes.iter().enumerate() {
            let patched = (1..5).contains(&i) || (8..12).contains(&i) || (13..17).contains(&i);
            if !patched {
                assert_eq!(file[start + i], byte, "byte {i}");
            }
        }
        // Each call reaches its routine, each data reference its byte.
        let reach = |field: usize| (start + field + 4) as i64 + i32_at(&file, start + field);
        assert_eq!(reach(1), (TEXT_OFFSET + ENTRIES[PRINT_I64 as usize] as u64) as i64);
        assert_eq!(reach(13), (TEXT_OFFSET + ENTRIES[STR_APPEND as usize] as u64) as i64);
        assert_eq!(reach(8), (at.data + 2) as i64);
        assert_eq!(&file[at.data as usize..at.data as usize + 5], b"hello");
    }

    #[test]
    fn relative_distances() {
        assert_eq!(relative(0x110, 0x100), 12i32.to_le_bytes());
        assert_eq!(relative(0x100, 0x100), (-4i32).to_le_bytes());
        assert_eq!(relative(0x0F0, 0x100), (-20i32).to_le_bytes());
    }
}
