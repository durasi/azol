// sort: a hand-written quicksort.
// Algorithm: Hoare partition around the middle element, recursing on both
// halves; then a sampled checksum and a check that the list is ordered.
// Sizes: 3,000,000 numbers below 1,000,000; small: 200,000.
// Output: checksum + 1 when ordered.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn quicksort(xs: &mut [i64], low: i64, high: i64) {
    if low >= high {
        return;
    }
    let pivot = xs[((low + high) / 2) as usize];
    let mut i = low;
    let mut j = high;
    while i <= j {
        while xs[i as usize] < pivot {
            i += 1;
        }
        while xs[j as usize] > pivot {
            j -= 1;
        }
        if i <= j {
            xs.swap(i as usize, j as usize);
            i += 1;
            j -= 1;
        }
    }
    quicksort(xs, low, j);
    quicksort(xs, i, high);
}

fn main() {
    let count = size(3000000, 200000);
    let mut xs: Vec<i64> = Vec::new();
    let mut seed: i64 = 42;
    for _ in 0..count {
        seed = seed * 48271 % 2147483647;
        xs.push(seed % 1000000);
    }
    quicksort(&mut xs, 0, count - 1);
    let checksum: i64 = xs.iter().enumerate().step_by(1000).map(|(k, x)| x * (k as i64 % 7)).sum();
    let ordered = if xs.windows(2).all(|pair| pair[0] <= pair[1]) { 1 } else { 0 };
    println!("{}", checksum + ordered);
}
