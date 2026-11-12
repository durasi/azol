// dp_lcs: the longest common subsequence of two byte strings.
// Algorithm: the classic table, keeping only two rows.
// Sizes: two strings of 12,000 bytes over four letters; small: 1,500 bytes.
// Output: the length of the longest common subsequence.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn letters(count: i64, start: i64) -> Vec<u8> {
    let mut seed = start;
    (0..count)
        .map(|_| {
            seed = seed * 48271 % 2147483647;
            97 + (seed % 4) as u8
        })
        .collect()
}

fn main() {
    let count = size(12000, 1500);
    let a = letters(count, 51);
    let b = letters(count, 67);
    let mut above = vec![0i64; b.len() + 1];
    let mut row = vec![0i64; b.len() + 1];
    for &x in &a {
        for (j, &y) in b.iter().enumerate() {
            row[j + 1] = if x == y { above[j] + 1 } else { above[j + 1].max(row[j]) };
        }
        std::mem::swap(&mut above, &mut row);
    }
    println!("{}", above[b.len()]);
}
