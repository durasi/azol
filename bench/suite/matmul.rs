// matmul: integer matrix multiply.
// Algorithm: the plain triple loop (row, column, inner product) on square
// matrices kept in flat lists; the product feeds the next round.
// Sizes: 200 x 200, 40 rounds; small: 100 x 100, 3 rounds.
// Output: the sum of the last product.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn multiply(a: &[i64], b: &[i64], c: &mut [i64], n: usize) {
    for i in 0..n {
        for j in 0..n {
            let mut total = 0;
            for k in 0..n {
                total += a[i * n + k] * b[k * n + j];
            }
            c[i * n + j] = total;
        }
    }
}

fn main() {
    let n = size(200, 100) as usize;
    let rounds = size(40, 3);
    let mut a: Vec<i64> = Vec::new();
    let mut b: Vec<i64> = Vec::new();
    let mut seed: i64 = 13;
    for _ in 0..n * n {
        seed = seed * 48271 % 2147483647;
        a.push(seed % 1000);
        b.push(seed / 1000 % 1000);
    }
    let mut c = vec![0i64; n * n];
    for _ in 0..rounds {
        multiply(&a, &b, &mut c, n);
        for (x, y) in a.iter_mut().zip(c.iter()) {
            *x = y % 1000;
        }
    }
    println!("{}", c.iter().sum::<i64>());
}
