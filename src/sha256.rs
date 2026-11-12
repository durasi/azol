// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! SHA-256, needed for the code signature macOS requires on every
//! executable. Kept in-tree so the compiler has no dependencies.
//!
//! Processors made for the last several years compute SHA-256 with
//! instructions of their own, many times faster than ordinary code can.
//! They are used when the processor has them; the portable version below
//! is the fallback and the reference the others are tested against.

use std::sync::OnceLock;

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// The state every message starts from.
const INITIAL: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];

/// Mixes whole 64-byte blocks into the state.
type Compress = fn(&mut [u32; 8], &[u8]);

/// The portable version: plain arithmetic, any processor.
fn compress_portable(state: &mut [u32; 8], blocks: &[u8]) {
    for block in blocks.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, chunk) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }
}

/// The ARM64 version, for processors with the SHA2 instructions (every
/// Apple Silicon Mac). One instruction pair does four rounds; two more
/// instructions extend the message schedule by four words.
///
/// # Safety
/// The processor must support `sha2`: `hardware` checks that before
/// handing this function out. All memory access goes through `state`
/// (eight words: two loads and two stores of four) and through 64-byte
/// chunks of `blocks` (four loads of 16 bytes each), so it stays inside
/// what the references cover.
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "sha2")]
unsafe fn compress_arm(state: &mut [u32; 8], blocks: &[u8]) {
    // The safety conditions are those in the comment above.
    unsafe {
        use std::arch::aarch64::*;
        // abcd holds a in its lowest lane, efgh holds e there.
        let mut abcd = vld1q_u32(state.as_ptr());
        let mut efgh = vld1q_u32(state.as_ptr().add(4));
        for block in blocks.chunks_exact(64) {
            let (abcd_before, efgh_before) = (abcd, efgh);
            // The sixteen message words, read as big-endian numbers. From
            // here on `m` always holds the latest sixteen schedule words;
            // `m[i % 4]` is the oldest group once i reaches 4.
            let mut m = [
                vreinterpretq_u32_u8(vrev32q_u8(vld1q_u8(block.as_ptr()))),
                vreinterpretq_u32_u8(vrev32q_u8(vld1q_u8(block.as_ptr().add(16)))),
                vreinterpretq_u32_u8(vrev32q_u8(vld1q_u8(block.as_ptr().add(32)))),
                vreinterpretq_u32_u8(vrev32q_u8(vld1q_u8(block.as_ptr().add(48)))),
            ];
            // Sixteen groups of four rounds.
            for i in 0..16 {
                let w = if i < 4 {
                    m[i]
                } else {
                    // w[t] for t = 4i..4i+3 from the groups 16, 12, 8 and 4 words back.
                    let next = vsha256su1q_u32(vsha256su0q_u32(m[i % 4], m[(i + 1) % 4]), m[(i + 2) % 4], m[(i + 3) % 4]);
                    m[i % 4] = next;
                    next
                };
                let wk = vaddq_u32(w, vld1q_u32(K.as_ptr().add(4 * i)));
                let abcd_old = abcd;
                abcd = vsha256hq_u32(abcd, efgh, wk);
                efgh = vsha256h2q_u32(efgh, abcd_old, wk);
            }
            abcd = vaddq_u32(abcd, abcd_before);
            efgh = vaddq_u32(efgh, efgh_before);
        }
        vst1q_u32(state.as_mut_ptr(), abcd);
        vst1q_u32(state.as_mut_ptr().add(4), efgh);
    }
}

/// The x86-64 version, for processors with the SHA extensions. One
/// instruction does two rounds; the state is kept in the order those
/// instructions want (a, b, e, f in one register and c, d, g, h in the
/// other).
///
/// # Safety
/// The processor must support `sha`, `sse2`, `ssse3` and `sse4.1`:
/// `hardware` checks that before handing this function out. All memory
/// access goes through `state` (two unaligned loads and stores of four
/// words), 64-byte chunks of `blocks` (four unaligned loads of 16 bytes)
/// and `K` (sixteen unaligned loads of four words), so it stays inside
/// what the references cover.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sha,sse2,ssse3,sse4.1")]
unsafe fn compress_x86(state: &mut [u32; 8], blocks: &[u8]) {
    // The safety conditions are those in the comment above.
    unsafe {
        use std::arch::x86_64::*;
        // Turns four big-endian words into the processor's order.
        let big_endian = _mm_set_epi64x(0x0c0d0e0f_08090a0b, 0x04050607_00010203);
        let dcba = _mm_loadu_si128(state.as_ptr() as *const __m128i);
        let hgfe = _mm_loadu_si128(state.as_ptr().add(4) as *const __m128i);
        let cdab = _mm_shuffle_epi32(dcba, 0xB1);
        let efgh = _mm_shuffle_epi32(hgfe, 0x1B);
        let mut abef = _mm_alignr_epi8(cdab, efgh, 8);
        let mut cdgh = _mm_blend_epi16(efgh, cdab, 0xF0);
        for block in blocks.chunks_exact(64) {
            let (abef_before, cdgh_before) = (abef, cdgh);
            // The latest sixteen schedule words; `m[i % 4]` is the oldest
            // group once i reaches 4.
            let mut m = [
                _mm_shuffle_epi8(_mm_loadu_si128(block.as_ptr() as *const __m128i), big_endian),
                _mm_shuffle_epi8(_mm_loadu_si128(block.as_ptr().add(16) as *const __m128i), big_endian),
                _mm_shuffle_epi8(_mm_loadu_si128(block.as_ptr().add(32) as *const __m128i), big_endian),
                _mm_shuffle_epi8(_mm_loadu_si128(block.as_ptr().add(48) as *const __m128i), big_endian),
            ];
            // Sixteen groups of four rounds.
            for i in 0..16 {
                let w = if i < 4 {
                    m[i]
                } else {
                    // w[t] for t = 4i..4i+3: the group 16 words back, mixed
                    // with the one after it, plus w[t-7], then with w[t-2].
                    let (oldest, second, third, newest) = (m[i % 4], m[(i + 1) % 4], m[(i + 2) % 4], m[(i + 3) % 4]);
                    let partial = _mm_add_epi32(_mm_sha256msg1_epu32(oldest, second), _mm_alignr_epi8(newest, third, 4));
                    let next = _mm_sha256msg2_epu32(partial, newest);
                    m[i % 4] = next;
                    next
                };
                let wk = _mm_add_epi32(w, _mm_loadu_si128(K.as_ptr().add(4 * i) as *const __m128i));
                cdgh = _mm_sha256rnds2_epu32(cdgh, abef, wk);
                abef = _mm_sha256rnds2_epu32(abef, cdgh, _mm_shuffle_epi32(wk, 0x0E));
            }
            abef = _mm_add_epi32(abef, abef_before);
            cdgh = _mm_add_epi32(cdgh, cdgh_before);
        }
        let feba = _mm_shuffle_epi32(abef, 0x1B);
        let dchg = _mm_shuffle_epi32(cdgh, 0xB1);
        _mm_storeu_si128(state.as_mut_ptr() as *mut __m128i, _mm_blend_epi16(feba, dchg, 0xF0));
        _mm_storeu_si128(state.as_mut_ptr().add(4) as *mut __m128i, _mm_alignr_epi8(dchg, feba, 8));
    }
}

/// The version written for this processor's SHA-256 instructions, when
/// it has them.
fn processor_version() -> Option<Compress> {
    #[cfg(target_arch = "aarch64")]
    if std::arch::is_aarch64_feature_detected!("sha2") {
        // Safe to call: the processor has the instructions it uses.
        return Some(|state, blocks| unsafe { compress_arm(state, blocks) });
    }
    #[cfg(target_arch = "x86_64")]
    if std::arch::is_x86_feature_detected!("sha")
        && std::arch::is_x86_feature_detected!("sse2")
        && std::arch::is_x86_feature_detected!("ssse3")
        && std::arch::is_x86_feature_detected!("sse4.1")
    {
        // Safe to call: the processor has the instructions it uses.
        return Some(|state, blocks| unsafe { compress_x86(state, blocks) });
    }
    None
}

/// The fast version to use, if there is one and it can be trusted. It is
/// tried once on a message with a known digest that is long enough to
/// need two blocks; if the answer were ever wrong, the portable version
/// would be used instead of signing executables with bad hashes.
fn hardware() -> Option<Compress> {
    static CHOICE: OnceLock<Option<Compress>> = OnceLock::new();
    *CHOICE.get_or_init(|| {
        processor_version().filter(|&compress| digest(SELF_CHECK.0, compress) == SELF_CHECK.1)
    })
}

/// The 448-bit message of the standard and its digest.
const SELF_CHECK: (&[u8], [u8; 32]) = (
    b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
    [
        0x24, 0x8d, 0x6a, 0x61, 0xd2, 0x06, 0x38, 0xb8, 0xe5, 0xc0, 0x26, 0x93, 0x0c, 0x3e, 0x60, 0x39, 0xa3, 0x3c, 0xe4,
        0x59, 0x64, 0xff, 0x21, 0x67, 0xf6, 0xec, 0xed, 0xd4, 0x19, 0xdb, 0x06, 0xc1,
    ],
);

/// SHA-256 of `data`, computed with the given block function.
fn digest(data: &[u8], compress: Compress) -> [u8; 32] {
    let mut state = INITIAL;
    let whole = data.len() - data.len() % 64;
    compress(&mut state, &data[..whole]);
    // The rest, the end marker and the length in bits fill one or two
    // more blocks.
    let rest = &data[whole..];
    let mut tail = [0u8; 128];
    tail[..rest.len()].copy_from_slice(rest);
    tail[rest.len()] = 0x80;
    let tail_len = if rest.len() < 56 { 64 } else { 128 };
    tail[tail_len - 8..tail_len].copy_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    compress(&mut state, &tail[..tail_len]);
    let mut out = [0u8; 32];
    for (chunk, word) in out.chunks_exact_mut(4).zip(state) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    out
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    digest(data, hardware().unwrap_or(compress_portable))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: [u8; 32]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Messages with published digests: the empty one, "abc", the 448-bit
    /// and 896-bit messages of the standard, a thousand and a million 'a'.
    fn vectors() -> Vec<(Vec<u8>, &'static str)> {
        vec![
            (Vec::new(), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),
            (b"abc".to_vec(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),
            (
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq".to_vec(),
                "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            ),
            (
                b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"
                    .to_vec(),
                "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
            ),
            (vec![b'a'; 1000], "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"),
            (vec![b'a'; 1_000_000], "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"),
        ]
    }

    #[test]
    fn known_vectors() {
        for (message, expected) in vectors() {
            assert_eq!(hex(sha256(&message)), expected, "sha256 of {} bytes", message.len());
        }
    }

    #[test]
    fn portable_version_matches_known_vectors() {
        for (message, expected) in vectors() {
            assert_eq!(hex(digest(&message, compress_portable)), expected, "portable sha256 of {} bytes", message.len());
        }
    }

    /// The judge of the processor-specific code. On a machine without the
    /// instructions there is nothing to judge and the test says so.
    #[test]
    fn hardware_version_matches_known_vectors() {
        let Some(compress) = processor_version() else {
            eprintln!("this processor has no SHA-256 instructions; the portable version is in use");
            return;
        };
        for (message, expected) in vectors() {
            assert_eq!(hex(digest(&message, compress)), expected, "hardware sha256 of {} bytes", message.len());
        }
    }

    /// Every length from nothing to several blocks, so that each way the
    /// last blocks can be filled is covered, with bytes that differ at
    /// every position.
    #[test]
    fn hardware_version_matches_portable_version_at_every_length() {
        let Some(compress) = processor_version() else {
            eprintln!("this processor has no SHA-256 instructions; the portable version is in use");
            return;
        };
        let data: Vec<u8> = (0..300u32).map(|i| (i.wrapping_mul(2654435761) >> 24) as u8).collect();
        for len in 0..=data.len() {
            assert_eq!(
                digest(&data[..len], compress),
                digest(&data[..len], compress_portable),
                "hardware and portable sha256 differ for {len} bytes"
            );
        }
        // And a size like the pages the code signature hashes.
        let page: Vec<u8> = (0..16384u32).map(|i| (i * 31 % 251) as u8).collect();
        assert_eq!(digest(&page, compress), digest(&page, compress_portable));
    }

    /// The block functions themselves, fed several blocks at once from a
    /// state that is not the initial one.
    #[test]
    fn hardware_version_matches_portable_version_block_by_block() {
        let Some(compress) = processor_version() else {
            eprintln!("this processor has no SHA-256 instructions; the portable version is in use");
            return;
        };
        let blocks: Vec<u8> = (0..64 * 5u32).map(|i| (i.wrapping_mul(40503) >> 3) as u8).collect();
        let start = [0x01234567, 0x89abcdef, 0xfedcba98, 0x76543210, 0xdeadbeef, 0x0badf00d, 0xffffffff, 0];
        for count in 0..=5 {
            let (mut fast, mut slow) = (start, start);
            compress(&mut fast, &blocks[..64 * count]);
            compress_portable(&mut slow, &blocks[..64 * count]);
            assert_eq!(fast, slow, "states differ after {count} blocks");
        }
    }

    /// The check made before the fast version is used agrees with the
    /// tests above: it is used exactly when the processor offers it.
    #[test]
    fn self_check_accepts_the_hardware_version() {
        assert_eq!(hex(SELF_CHECK.1), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
        assert_eq!(digest(SELF_CHECK.0, compress_portable), SELF_CHECK.1);
        assert_eq!(hardware().is_some(), processor_version().is_some());
    }

    /// On the machines azoL is developed for, the fast path must exist:
    /// if it silently went missing, builds would only get slower.
    #[test]
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    fn apple_silicon_uses_the_hardware_version() {
        assert!(processor_version().is_some(), "an Apple Silicon Mac should offer the SHA2 instructions");
    }
}
