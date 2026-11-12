// Run-time benchmark: the same computation as bench/sort.azl.

fn quicksort(xs: &mut Vec<i64>, low: i64, high: i64) {
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
            let held = xs[i as usize];
            xs[i as usize] = xs[j as usize];
            xs[j as usize] = held;
            i += 1;
            j -= 1;
        }
    }
    quicksort(xs, low, j);
    quicksort(xs, i, high);
}

fn main() {
    let count: i64 = std::hint::black_box(2000000);
    let mut xs: Vec<i64> = Vec::new();
    let mut seed: i64 = std::hint::black_box(42);
    let mut n = 0;
    while n < count {
        seed = seed * 48271 % 2147483647;
        xs.push(seed % 1000000);
        n += 1;
    }
    quicksort(&mut xs, 0, count - 1);
    let mut checksum: i64 = 0;
    let mut k = 0;
    while k < count {
        checksum += xs[k as usize] * (k % 7);
        k += 1000;
    }
    let mut ordered = 1;
    let mut m = 1;
    while m < count {
        if xs[(m - 1) as usize] > xs[m as usize] {
            ordered = 0;
        }
        m += 1;
    }
    println!("{}", checksum + ordered);
}
