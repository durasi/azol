// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Writes a macOS executable for Apple Silicon (Mach-O, arm64) directly,
//! including the ad-hoc code signature the system requires. No external
//! linker or `codesign` run is involved.

use crate::arm64::{Code, Reloc};
use crate::sha256::sha256;

const PAGE: u64 = 0x4000;
const TEXT_VMADDR: u64 = 0x1_0000_0000;
const SIGN_PAGE_SHIFT: u8 = 12;

const LC_SEGMENT_64: u32 = 0x19;
const LC_SYMTAB: u32 = 0x2;
const LC_DYSYMTAB: u32 = 0xB;
const LC_LOAD_DYLIB: u32 = 0xC;
const LC_LOAD_DYLINKER: u32 = 0xE;
const LC_CODE_SIGNATURE: u32 = 0x1D;
const LC_DYLD_INFO_ONLY: u32 = 0x8000_0022;
const LC_MAIN: u32 = 0x8000_0028;
const LC_BUILD_VERSION: u32 = 0x32;

struct Buf(Vec<u8>);

impl Buf {
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn name16(&mut self, name: &str) {
        let mut bytes = [0u8; 16];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        self.0.extend_from_slice(&bytes);
    }
    /// A NUL-terminated string padded so the buffer length is a multiple of 8.
    fn padded_str(&mut self, text: &str, total: usize) {
        let start = self.0.len();
        self.0.extend_from_slice(text.as_bytes());
        self.0.resize(start + total, 0);
    }
    #[allow(clippy::too_many_arguments)]
    fn segment(&mut self, name: &str, vmaddr: u64, vmsize: u64, fileoff: u64, filesize: u64, prot: u32, nsects: u32, flags: u32) {
        self.u32(LC_SEGMENT_64);
        self.u32(72 + 80 * nsects);
        self.name16(name);
        self.u64(vmaddr);
        self.u64(vmsize);
        self.u64(fileoff);
        self.u64(filesize);
        self.u32(prot);
        self.u32(prot);
        self.u32(nsects);
        self.u32(flags);
    }
    #[allow(clippy::too_many_arguments)]
    fn section(&mut self, name: &str, segment: &str, addr: u64, size: u64, offset: u64, align: u32, flags: u32) {
        self.name16(name);
        self.name16(segment);
        self.u64(addr);
        self.u64(size);
        self.u32(offset as u32);
        self.u32(align);
        self.u32(0); // relocations
        self.u32(0);
        self.u32(flags);
        self.u32(0); // reserved1: index into the indirect symbol table
        self.u32(0);
        self.u32(0);
    }
}

fn align_up(value: u64, to: u64) -> u64 {
    value.div_ceil(to) * to
}

/// Size in bytes of the signature blob for a file whose signed part is
/// `code_limit` bytes long.
fn signature_size(code_limit: u64, ident: &str) -> usize {
    let pages = code_limit.div_ceil(1 << SIGN_PAGE_SHIFT) as usize;
    20 + 88 + ident.len() + 1 + pages * 32
}

/// Writes the SHA-256 of every `page_size` chunk of `data` into `out`,
/// 32 bytes each. With the processor's own SHA-256 instructions this is
/// quick enough to do on one thread: starting more would cost more
/// processor time than it saves.
fn hash_pages(data: &[u8], page_size: usize, out: &mut [u8]) {
    for (page, slot) in data.chunks(page_size).zip(out.chunks_mut(32)) {
        slot.copy_from_slice(&sha256(page));
    }
}

/// Builds the embedded signature: a SuperBlob holding one CodeDirectory
/// with a SHA-256 hash of every page of the file. All fields big-endian.
pub fn code_signature(signed: &[u8], ident: &str, text_file_size: u64, page_shift: u8) -> Vec<u8> {
    let page_size = 1usize << page_shift;
    let pages = signed.len().div_ceil(page_size);
    let ident_offset = 88u32;
    let hash_offset = ident_offset + ident.len() as u32 + 1;
    let dir_len = hash_offset + pages as u32 * 32;
    let total = 20 + dir_len;

    let mut out = Vec::with_capacity(total as usize);
    let be32 = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_be_bytes());
    // SuperBlob header and its single index entry.
    be32(&mut out, 0xFADE_0CC0);
    be32(&mut out, total);
    be32(&mut out, 1);
    be32(&mut out, 0);
    be32(&mut out, 20);
    // CodeDirectory, version 0x20400.
    be32(&mut out, 0xFADE_0C02);
    be32(&mut out, dir_len);
    be32(&mut out, 0x0002_0400);
    be32(&mut out, 0x0002_0002); // adhoc | linker-signed
    be32(&mut out, hash_offset);
    be32(&mut out, ident_offset);
    be32(&mut out, 0); // special slots
    be32(&mut out, pages as u32);
    be32(&mut out, signed.len() as u32);
    out.extend_from_slice(&[32, 2, 0, page_shift]); // hash size, SHA-256, platform, page size
    be32(&mut out, 0); // spare2
    be32(&mut out, 0); // scatter offset
    be32(&mut out, 0); // team offset
    be32(&mut out, 0); // spare3
    out.extend_from_slice(&0u64.to_be_bytes()); // code limit 64
    out.extend_from_slice(&0u64.to_be_bytes()); // exec segment base
    out.extend_from_slice(&text_file_size.to_be_bytes()); // exec segment limit
    out.extend_from_slice(&1u64.to_be_bytes()); // exec segment flags: main binary
    out.extend_from_slice(ident.as_bytes());
    out.push(0);
    let hashes_at = out.len();
    out.resize(total as usize, 0);
    hash_pages(signed, page_size, &mut out[hashes_at..]);
    out
}

/// `adrp rd, target`: the 4 KiB page of `target`, relative to `pc`.
fn adrp(rd: u32, pc: u64, target: u64) -> u32 {
    let delta = (target >> 12) as i64 - (pc >> 12) as i64;
    0x9000_0000 | ((delta & 3) as u32) << 29 | (((delta >> 2) & 0x7_FFFF) as u32) << 5 | rd
}

fn uleb(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// Lays out, links and signs an executable.
///
/// The file has three mapped parts: `__TEXT` (headers, code, read-only
/// data), `__DATA_CONST` (one pointer per imported function, filled in by
/// the system loader) and `__LINKEDIT` (the loader's instructions for
/// those pointers, the symbol table and the signature).
/// Makes the program start in the runtime's `_rt_start` instead of its
/// `main`: the routine prepares for a stack that runs out and then calls
/// `main`, through a word that is filled in here. The runtime is the last
/// thing in the code, which is how it is found.
pub fn start_through_runtime(code: &mut Code) {
    use crate::runtime::{RUNTIME, START, START_MAIN};
    let runtime_start = code.words.len() as u32 - RUNTIME.len() as u32;
    let call = runtime_start + START_MAIN;
    debug_assert_eq!(code.words[call as usize], 0xD420_0000 | 0x800 << 5, "the marker `brk #0x800`");
    let distance = code.entry as i64 - call as i64;
    code.words[call as usize] = 0x9400_0000 | (distance as u32 & 0x03FF_FFFF); // bl main
    code.entry = runtime_start + START;
}

pub fn build_executable(code: &Code, imports: &[&str], ident: &str) -> Vec<u8> {
    const NCMDS: u32 = 12;
    const SIZEOFCMDS: u32 = 72 + 232 + 152 + 72 + 48 + 24 + 80 + 32 + 24 + 24 + 56 + 16;
    const DATA_CONST_SEGMENT: u8 = 2;
    let n_imports = imports.len() as u64;

    let code_off = align_up(32 + SIZEOFCMDS as u64, 16);
    let code_size = code.words.len() as u64 * 4;
    let const_off = code_off + code_size;
    let const_size = code.data.len() as u64;
    let text_size = align_up(const_off + const_size, PAGE);
    let got_off = text_size;
    let got_addr = TEXT_VMADDR + got_off;
    let got_size = n_imports * 8;
    let linkedit_off = got_off + PAGE;

    // Loader instructions: bind each GOT slot to its symbol in libSystem.
    let mut bind = Vec::new();
    for (i, name) in imports.iter().enumerate() {
        bind.push(0x70 | DATA_CONST_SEGMENT); // set segment and offset
        uleb(&mut bind, i as u64 * 8);
        bind.push(0x40); // symbol name follows
        bind.extend_from_slice(name.as_bytes());
        bind.push(0);
        bind.push(0x51); // type: pointer
        bind.push(0x11); // library ordinal 1
        bind.push(0x90); // bind
    }
    bind.push(0x00); // done
    bind.resize(align_up(bind.len() as u64, 8) as usize, 0);

    let mut strtab = vec![b' ', 0];
    let mut name_offsets = Vec::with_capacity(imports.len());
    for name in imports {
        name_offsets.push(strtab.len() as u32);
        strtab.extend_from_slice(name.as_bytes());
        strtab.push(0);
    }

    let bind_off = linkedit_off;
    let symtab_off = bind_off + bind.len() as u64;
    let indirect_off = symtab_off + n_imports * 16;
    let strtab_off = align_up(indirect_off + n_imports * 4, 8);
    let sig_off = align_up(strtab_off + strtab.len() as u64, 16);
    let strtab_size = sig_off - strtab_off;
    let sig_size = align_up(signature_size(sig_off, ident) as u64, 16);
    let linkedit_size = sig_off + sig_size - linkedit_off;

    let mut b = Buf(Vec::with_capacity((linkedit_off + linkedit_size) as usize));
    // mach_header_64
    b.u32(0xFEED_FACF);
    b.u32(0x0100_000C); // CPU_TYPE_ARM64
    b.u32(0);
    b.u32(2); // MH_EXECUTE
    b.u32(NCMDS);
    b.u32(SIZEOFCMDS);
    b.u32(0x0020_0085); // NOUNDEFS | DYLDLINK | TWOLEVEL | PIE
    b.u32(0);

    b.segment("__PAGEZERO", 0, TEXT_VMADDR, 0, 0, 0, 0, 0);

    b.segment("__TEXT", TEXT_VMADDR, text_size, 0, text_size, 5, 2, 0);
    b.section("__text", "__TEXT", TEXT_VMADDR + code_off, code_size, code_off, 2, 0x8000_0400); // pure instructions
    b.section("__const", "__TEXT", TEXT_VMADDR + const_off, const_size, const_off, 0, 0);

    b.segment("__DATA_CONST", got_addr, PAGE, got_off, PAGE, 3, 1, 0x10); // read-only after loading
    b.section("__got", "__DATA_CONST", got_addr, got_size, got_off, 3, 0x6); // non-lazy symbol pointers

    b.segment("__LINKEDIT", TEXT_VMADDR + linkedit_off, align_up(linkedit_size, PAGE), linkedit_off, linkedit_size, 1, 0, 0);

    b.u32(LC_DYLD_INFO_ONLY);
    b.u32(48);
    b.u32(0); // rebase
    b.u32(0);
    b.u32(bind_off as u32);
    b.u32(bind.len() as u32);
    for _ in 0..6 {
        b.u32(0); // weak bind, lazy bind, export
    }

    b.u32(LC_SYMTAB);
    b.u32(24);
    b.u32(symtab_off as u32);
    b.u32(n_imports as u32);
    b.u32(strtab_off as u32);
    b.u32(strtab_size as u32);

    b.u32(LC_DYSYMTAB);
    b.u32(80);
    b.u32(0); // local symbols
    b.u32(0);
    b.u32(0); // defined external symbols
    b.u32(0);
    b.u32(0); // undefined symbols
    b.u32(n_imports as u32);
    for _ in 0..6 {
        b.u32(0); // table of contents, modules, external references
    }
    b.u32(indirect_off as u32);
    b.u32(n_imports as u32);
    for _ in 0..4 {
        b.u32(0); // relocations
    }

    b.u32(LC_LOAD_DYLINKER);
    b.u32(32);
    b.u32(12);
    b.padded_str("/usr/lib/dyld", 20);

    b.u32(LC_BUILD_VERSION);
    b.u32(24);
    b.u32(1); // macOS
    b.u32(0x000B_0000); // minimum OS 11.0
    b.u32(0x000B_0000); // SDK 11.0
    b.u32(0);

    b.u32(LC_MAIN);
    b.u32(24);
    b.u64(code_off + code.entry as u64 * 4);
    b.u64(0);

    b.u32(LC_LOAD_DYLIB);
    b.u32(56);
    b.u32(24);
    b.u32(2);
    b.u32(0x0001_0000);
    b.u32(0x0001_0000);
    b.padded_str("/usr/lib/libSystem.B.dylib", 32);

    b.u32(LC_CODE_SIGNATURE);
    b.u32(16);
    b.u32(sig_off as u32);
    b.u32(sig_size as u32);

    debug_assert_eq!(b.0.len() as u32, 32 + SIZEOFCMDS);

    // Code: the words go in as they are, in one pass over the buffer,
    // and then the few instructions that depend on final addresses are
    // filled in where they landed. No second copy of the code is made.
    b.0.resize((code_off + code_size) as usize, 0);
    let code_bytes = &mut b.0[code_off as usize..];
    for (bytes, word) in code_bytes.chunks_exact_mut(4).zip(&code.words) {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    let mut put = |at: u32, word: u32| {
        let start = at as usize * 4;
        code_bytes[start..start + 4].copy_from_slice(&word.to_le_bytes());
    };
    for reloc in &code.relocs {
        match *reloc {
            Reloc::Import { at, import } => {
                let pc = TEXT_VMADDR + code_off + at as u64 * 4;
                let target = got_addr + import as u64 * 8;
                put(at, adrp(16, pc, target));
                put(at + 1, 0xF940_0210 | (((target & 0xFFF) / 8) as u32) << 10); // ldr x16, [x16, #lo12]
                put(at + 2, 0xD63F_0200); // blr x16
            }
            Reloc::Data { at, offset, reg } => {
                let pc = TEXT_VMADDR + code_off + at as u64 * 4;
                let target = TEXT_VMADDR + const_off + offset as u64;
                put(at, adrp(reg, pc, target));
                put(at + 1, 0x9100_0000 | ((target & 0xFFF) as u32) << 10 | reg << 5 | reg); // add reg, reg, #lo12
            }
        }
    }
    b.0.extend_from_slice(&code.data);

    // The GOT page is all zeros in the file; the loader fills it.
    b.0.resize(linkedit_off as usize, 0);
    b.0.extend_from_slice(&bind);
    for &name_offset in &name_offsets {
        b.u32(name_offset);
        b.0.push(0x01); // undefined, external
        b.0.push(0);
        b.0.extend_from_slice(&0x0100u16.to_le_bytes()); // from library 1
        b.u64(0);
    }
    for index in 0..n_imports as u32 {
        b.u32(index);
    }
    b.0.resize(strtab_off as usize, 0);
    b.0.extend_from_slice(&strtab);
    b.0.resize(sig_off as usize, 0);

    let signature = code_signature(&b.0, ident, text_size, SIGN_PAGE_SHIFT);
    b.0.extend_from_slice(&signature);
    b.0.resize((linkedit_off + linkedit_size) as usize, 0);
    b.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn le32(data: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(data[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn a_program_starts_in_the_runtime_which_calls_main() {
        use crate::runtime::{RUNTIME, START, START_MAIN};
        // Three words of program, `main` at the second, then the runtime.
        let mut words = vec![0xD503_201F; 3];
        words.extend_from_slice(&RUNTIME);
        let mut code = Code { words, entry: 1, data: Vec::new(), relocs: Vec::new() };
        start_through_runtime(&mut code);
        assert_eq!(code.entry, 3 + START);
        // The marker became `bl main`: a call backwards over the runtime
        // so far and two words of program.
        let call = (3 + START_MAIN) as usize;
        let back = (1 - call as i64) as u32 & 0x03FF_FFFF;
        assert_eq!(code.words[call], 0x9400_0000 | back);
        // Nothing else changed.
        assert!(code.words[..3].iter().all(|&word| word == 0xD503_201F));
        assert_eq!(code.words[3..call], RUNTIME[..START_MAIN as usize]);
        assert_eq!(code.words[call + 1..], RUNTIME[START_MAIN as usize + 1..]);
        assert_eq!(RUNTIME[START_MAIN as usize], 0xD420_0000 | 0x800 << 5);
        assert!(START < START_MAIN);
    }

    #[test]
    fn layout_is_consistent() {
        // mov x0, #42 ; ret, then a call through GOT slot 1 and a data address.
        let code = Code {
            words: vec![0xD280_0540, 0xD65F_03C0, 0, 0, 0, 0, 0],
            entry: 0,
            data: b"hello".to_vec(),
            relocs: vec![Reloc::Import { at: 2, import: 1 }, Reloc::Data { at: 5, offset: 1, reg: 3 }],
        };
        let file = build_executable(&code, &["_write", "_exit"], "t");
        assert_eq!(le32(&file, 0), 0xFEED_FACF);
        let ncmds = le32(&file, 16);
        let sizeofcmds = le32(&file, 20) as usize;
        // Walk the load commands: sizes must add up exactly.
        let mut at = 32;
        let mut seen = Vec::new();
        for _ in 0..ncmds {
            seen.push(le32(&file, at));
            at += le32(&file, at + 4) as usize;
        }
        assert_eq!(at, 32 + sizeofcmds);
        assert_eq!(seen.last(), Some(&LC_CODE_SIGNATURE));

        let code_off = 864;
        assert_eq!(le32(&file, code_off), 0xD280_0540);
        // Text is one page, the GOT is at 0x4000: adrp x16, +4 pages (of 4 KiB).
        assert_eq!(le32(&file, code_off + 8), 0x9000_0030);
        assert_eq!(le32(&file, code_off + 12), 0xF940_0610); // ldr x16, [x16, #8]
        assert_eq!(le32(&file, code_off + 16), 0xD63F_0200); // blr x16
        // The data starts right after the 7 words of code.
        let data_off = code_off + 28;
        assert_eq!(&file[data_off..data_off + 5], b"hello");
        assert_eq!(le32(&file, code_off + 20), 0x9000_0003); // adrp x3, same page
        assert_eq!(le32(&file, code_off + 24), 0x9100_0063 | ((data_off as u32 + 1) << 10)); // add x3, x3, #lo12

        // Loader instructions name both imports and the file ends with the signature.
        let linkedit = 0x8000;
        let bind = &file[linkedit..linkedit + 27];
        assert_eq!(bind, b"\x72\x00\x40_write\x00\x51\x11\x90\x72\x08\x40_exit\x00\x51\x11\x90\x00\x00");
        let sig_off = le32(&file, 32 + sizeofcmds - 8) as usize;
        let sig_size = le32(&file, 32 + sizeofcmds - 4) as usize;
        assert_eq!(&file[sig_off..sig_off + 4], &[0xFA, 0xDE, 0x0C, 0xC0]);
        assert_eq!(sig_off % 16, 0);
        assert_eq!(file.len(), sig_off + sig_size);
    }

    #[test]
    fn adrp_reaches_other_pages_in_both_directions() {
        // Expected encodings from clang's assembler.
        assert_eq!(adrp(16, 0x1_0000_1588, 0x1_0000_8000), 0xF000_0030);
        assert_eq!(adrp(1, 0x1_0000_1590, 0x1_0000_160C), 0x9000_0001);
        assert_eq!(adrp(0, 0x1_0000_5000, 0x1_0000_1000), 0x90FF_FFE0);
    }

    #[test]
    fn every_page_gets_its_own_hash() {
        // 1000 pages and a partial one.
        let data: Vec<u8> = (0..4096 * 1000 + 123).map(|i| (i * 31 % 251) as u8).collect();
        let mut hashes = vec![0u8; 1001 * 32];
        hash_pages(&data, 4096, &mut hashes);
        for (number, page) in data.chunks(4096).enumerate() {
            assert_eq!(&hashes[number * 32..number * 32 + 32], &sha256(page), "page {number}");
        }
        // The last, short page is hashed as it is, not padded.
        assert_eq!(&hashes[1000 * 32..], &sha256(&data[4096 * 1000..]));
    }

    #[test]
    fn signature_hashes_every_page() {
        let data = vec![7u8; 4096 * 2 + 10];
        let blob = code_signature(&data, "id", 4096, 12);
        assert_eq!(blob.len(), 20 + 88 + 3 + 3 * 32);
        assert_eq!(&blob[blob.len() - 32..], &sha256(&data[8192..]));
        assert_eq!(&blob[20 + 88..20 + 91], b"id\0");
    }
}
