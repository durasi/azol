// sieve: flags in a list.
// Algorithm: the sieve of Eratosthenes over a list of booleans.
// Sizes: primes up to 30,000,000; small: 2,000,000.
// Output: count * 100000000 + the largest prime.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn main() {
    let limit = size(30000000, 2000000) as usize;
    let mut composite = vec![false; limit + 1];
    let mut count: i64 = 0;
    let mut last: i64 = 0;
    for i in 2..=limit {
        if !composite[i] {
            count += 1;
            last = i as i64;
            let mut j = i * i;
            while j <= limit {
                composite[j] = true;
                j += i;
            }
        }
    }
    println!("{}", count * 100000000 + last);
}
