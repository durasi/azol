// sort_lib: the library's sort.
// Algorithm: `sorted()` in azoL against `sort_unstable()` in Rust on the
// same numbers; then a sampled checksum and a check that the list is ordered.
// Sizes: 6,000,000 numbers; small: 200,000.
// Output: checksum + 1 when ordered.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn main() {
    let count = size(6000000, 200000);
    let mut xs: Vec<i64> = Vec::new();
    let mut seed: i64 = 99;
    for _ in 0..count {
        seed = seed * 48271 % 2147483647;
        xs.push(seed);
    }
    let mut ys = xs.clone();
    ys.sort_unstable();
    let checksum: i64 = ys.iter().enumerate().step_by(1000).map(|(k, y)| y % 1000 * (k as i64 % 7)).sum();
    let ordered = if ys.windows(2).all(|pair| pair[0] <= pair[1]) { 1 } else { 0 };
    println!("{}", checksum + ordered);
}
