// spectral: the spectral norm of an infinite matrix's corner.
// Algorithm: ten rounds of the power method with the matrix
// A[i][j] = 1 / ((i + j)(i + j + 1) / 2 + i + 1), multiplying by A and by
// its transpose without storing it.
// Sizes: n = 2,200; small: n = 250.
// Output: the norm times 1,000,000,000 and rounded.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn entry(i: usize, j: usize) -> f64 {
    1.0 / ((i + j) * (i + j + 1) / 2 + i + 1) as f64
}

fn times(v: &[f64], out: &mut [f64], transposed: bool) {
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = v
            .iter()
            .enumerate()
            .map(|(j, x)| if transposed { entry(j, i) } else { entry(i, j) } * x)
            .sum();
    }
}

fn main() {
    let n = size(2200, 250) as usize;
    let mut u = vec![1.0f64; n];
    let mut v = vec![0.0f64; n];
    let mut held = vec![0.0f64; n];
    for _ in 0..10 {
        times(&u, &mut held, false);
        times(&held, &mut v, true);
        times(&v, &mut held, false);
        times(&held, &mut u, true);
    }
    let uv: f64 = u.iter().zip(&v).map(|(a, b)| a * b).sum();
    let vv: f64 = v.iter().map(|b| b * b).sum();
    println!("{}", ((uv / vv).sqrt() * 1000000000.0).round() as i64);
}
