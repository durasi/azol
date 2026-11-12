// bits: population count, bit reversal and a xorshift generator.
// Algorithm: step a 64-bit xorshift generator; add the number of set bits
// and fold in the reversed low 32 bits.
// azoL has no built-in for counting or reversing bits and its `>>` keeps
// the sign, so the azoL version writes them out with masks; Rust uses
// `count_ones` and `reverse_bits`.
// Sizes: 80,000,000 steps; small: 1,000,000.
// Output: the bit total and the folded value.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn main() {
    let steps = size(80000000, 1000000);
    let mut x: u64 = 88172645463325252;
    let mut ones: u64 = 0;
    let mut folded: u32 = 0;
    for _ in 0..steps {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        ones += x.count_ones() as u64;
        folded ^= (x as u32).reverse_bits();
    }
    println!("{} {}", ones, folded);
}
