// knapsack: one-dimensional dynamic programming.
// Algorithm: the 0/1 knapsack over a single row, walked from the capacity
// down for every item.
// Sizes: 4,000 items, capacity 100,000; small: 400 items, capacity 10,000.
// Output: the best value.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn main() {
    let items = size(4000, 400);
    let capacity = size(100000, 10000) as usize;
    let mut best = vec![0i64; capacity + 1];
    let mut seed: i64 = 71;
    for _ in 0..items {
        seed = seed * 48271 % 2147483647;
        let weight = (1 + seed % 97) as usize;
        seed = seed * 48271 % 2147483647;
        let value = 1 + seed % 1000;
        for c in (weight..=capacity).rev() {
            best[c] = best[c].max(best[c - weight] + value);
        }
    }
    println!("{}", best[capacity]);
}
