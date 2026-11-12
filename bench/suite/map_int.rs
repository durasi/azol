// map_int: a map with integer keys: insert, look up, remove.
// Algorithm: insert pseudo-random keys, look up present and absent keys,
// remove every second key, look up again.
// Rust uses its standard `HashMap`; azoL has no map with integer keys yet
// (`Map[K, V]` is planned), so the azoL version carries its own table.
// Sizes: 1,000,000 keys; small: 100,000.
// Output: keys left, keys removed, and the two look-up totals.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

use std::collections::HashMap;

fn lookups(m: &HashMap<i64, i64>, count: i64) -> i64 {
    let mut total = 0;
    let mut hit: i64 = 17;
    let mut miss: i64 = 23;
    for _ in 0..count {
        hit = hit * 48271 % 2147483647;
        if let Some(value) = m.get(&hit) {
            total += value;
        }
        miss = miss * 16807 % 2147483647;
        if m.contains_key(&(miss + 2147483647)) {
            total += 1;
        }
    }
    total
}

fn main() {
    let count = size(1000000, 100000);
    let mut m: HashMap<i64, i64> = HashMap::new();
    let mut seed: i64 = 17;
    for n in 0..count {
        seed = seed * 48271 % 2147483647;
        m.insert(seed, n);
    }
    let first = lookups(&m, count);
    let mut removed = 0;
    seed = 17;
    for n in 0..count {
        seed = seed * 48271 % 2147483647;
        if n % 2 == 0 && m.remove(&seed).is_some() {
            removed += 1;
        }
    }
    let second = lookups(&m, count);
    println!("{} {} {} {}", m.len(), removed, first, second);
}
