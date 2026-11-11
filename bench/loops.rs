// Run-time benchmark: the same computation as bench/loops.azl.

fn main() {
    let size: i64 = std::hint::black_box(5000);
    let mut total: i64 = 0;
    let mut i = 0;
    while i < size {
        let mut j = 0;
        while j < size {
            let product = i * j;
            if product % 7 == 3 {
                total += product % 11;
            } else {
                total += 1;
            }
            j += 1;
        }
        i += 1;
    }
    println!("{}", total);
}
