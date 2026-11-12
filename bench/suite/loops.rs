// loops: integer arithmetic in nested loops.
// Algorithm: for every pair (i, j) below the size, add a term chosen by
// the remainder of i * j.
// Sizes: 20000 x 20000; small: 1500 x 1500.
// Output: the total.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn main() {
    let n = size(20000, 1500);
    let mut total: i64 = 0;
    for i in 0..n {
        for j in 0..n {
            let product = i * j;
            if product % 7 == 3 {
                total += product % 11;
            } else {
                total += 1;
            }
        }
    }
    println!("{}", total);
}
