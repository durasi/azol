// prefix: prefix sums and a running maximum.
// Algorithm: every round writes the prefix sums of one list into another,
// takes their maximum, and refills the first list from the sums.
// Sizes: 4,000,000 elements, 20 rounds; small: 200,000 elements, 5 rounds.
// Output: the sum of the maxima.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn main() {
    let count = size(4000000, 200000) as usize;
    let rounds = size(20, 5);
    let mut source: Vec<i64> = Vec::new();
    let mut seed: i64 = 31;
    for _ in 0..count {
        seed = seed * 48271 % 2147483647;
        source.push(seed % 1024);
    }
    let mut sums = vec![0i64; count];
    let mut total = 0;
    for _ in 0..rounds {
        let mut running = 0;
        for (sum, x) in sums.iter_mut().zip(source.iter()) {
            running += x;
            *sum = running;
        }
        let highest = sums.iter().copied().max().unwrap_or(0).max(0);
        total += highest;
        for (i, (x, sum)) in source.iter_mut().zip(sums.iter()).enumerate() {
            *x = (sum ^ i as i64) & 1023;
        }
    }
    println!("{}", total);
}
