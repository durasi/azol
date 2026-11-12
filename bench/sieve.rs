// Run-time benchmark: the same computation as bench/sieve.azl.

fn main() {
    let limit: i64 = std::hint::black_box(20000000);
    let mut composite: Vec<bool> = Vec::new();
    let mut n = 0;
    while n <= limit {
        composite.push(false);
        n += 1;
    }
    let mut count: i64 = 0;
    let mut last: i64 = 0;
    let mut i: i64 = 2;
    while i <= limit {
        if !composite[i as usize] {
            count += 1;
            last = i;
            let mut j = i * i;
            while j <= limit {
                composite[j as usize] = true;
                j += i;
            }
        }
        i += 1;
    }
    println!("{}", count * 100000000 + last);
}
