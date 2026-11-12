// crc32: a table-driven checksum over a large buffer.
// Algorithm: the standard reflected CRC-32 (polynomial 0xEDB88320), one
// table look-up per byte.
// Sizes: 64 MB of pseudo-random bytes; small: 2 MB.
// Output: the checksum as a number.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn make_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    for (n, entry) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 == 1 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        *entry = c;
    }
    table
}

fn crc32(table: &[u32; 256], data: &[u8]) -> u32 {
    let mut crc = 0xFFFFFFFFu32;
    for &byte in data {
        crc = table[((crc ^ byte as u32) & 255) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFFFFFF
}

fn main() {
    let length = size(67108864, 2097152);
    let mut data: Vec<u8> = Vec::new();
    let mut seed: i64 = 41;
    for _ in 0..length {
        seed = seed * 48271 % 2147483647;
        data.push((seed >> 8) as u8);
    }
    println!("{}", crc32(&make_table(), &data));
}
