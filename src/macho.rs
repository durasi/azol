// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Writes a macOS executable for Apple Silicon (Mach-O, arm64) directly,
//! including the ad-hoc code signature the system requires. No external
//! linker or `codesign` run is involved.

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
    fn segment(&mut self, name: &str, vmaddr: u64, vmsize: u64, fileoff: u64, filesize: u64, prot: u32, nsects: u32) {
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
    for page in signed.chunks(page_size) {
        out.extend_from_slice(&sha256(page));
    }
    debug_assert_eq!(out.len(), total as usize);
    out
}

/// Lays out and signs an executable whose code is `code` and whose entry
/// point is the instruction at index `entry_word`.
pub fn build_executable(code: &[u32], entry_word: u32, ident: &str) -> Vec<u8> {
    const NCMDS: u32 = 11;
    const SIZEOFCMDS: u32 = 72 + 152 + 72 + 48 + 24 + 80 + 32 + 24 + 24 + 56 + 16;
    let code_off = align_up(32 + SIZEOFCMDS as u64, 16);
    let code_size = code.len() as u64 * 4;
    let text_size = align_up(code_off + code_size, PAGE);
    let linkedit_off = text_size;
    let strtab_size = 16u64;
    let sig_off = linkedit_off + strtab_size;
    let sig_size = align_up(signature_size(sig_off, ident) as u64, 16);
    let linkedit_size = strtab_size + sig_size;

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

    b.segment("__PAGEZERO", 0, TEXT_VMADDR, 0, 0, 0, 0);

    b.segment("__TEXT", TEXT_VMADDR, text_size, 0, text_size, 5, 1);
    b.name16("__text");
    b.name16("__TEXT");
    b.u64(TEXT_VMADDR + code_off);
    b.u64(code_size);
    b.u32(code_off as u32);
    b.u32(2); // 4-byte alignment
    b.u32(0);
    b.u32(0);
    b.u32(0x8000_0400); // pure instructions
    b.u32(0);
    b.u32(0);
    b.u32(0);

    b.segment("__LINKEDIT", TEXT_VMADDR + linkedit_off, align_up(linkedit_size, PAGE), linkedit_off, linkedit_size, 1, 0);

    b.u32(LC_DYLD_INFO_ONLY);
    b.u32(48);
    for _ in 0..10 {
        b.u32(0);
    }

    b.u32(LC_SYMTAB);
    b.u32(24);
    b.u32(linkedit_off as u32);
    b.u32(0);
    b.u32(linkedit_off as u32);
    b.u32(strtab_size as u32);

    b.u32(LC_DYSYMTAB);
    b.u32(80);
    for _ in 0..18 {
        b.u32(0);
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
    b.u64(code_off + entry_word as u64 * 4);
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
    b.0.resize(code_off as usize, 0);
    for word in code {
        b.u32(*word);
    }
    b.0.resize(linkedit_off as usize, 0);
    b.0.extend_from_slice(b" \0");
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
    fn layout_is_consistent() {
        let code = [0xD280_0540, 0xD65F_03C0]; // mov x0, #42 ; ret
        let file = build_executable(&code, 0, "t");
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
        // The code sits where LC_MAIN says and the file ends with the signature.
        let code_off = 640;
        assert_eq!(le32(&file, code_off), 0xD280_0540);
        assert_eq!(file.len(), 0x4000 + 16 + 272);
        assert_eq!(&file[0x4010..0x4014], &[0xFA, 0xDE, 0x0C, 0xC0]);
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
