// mandelbrot: escape-time iteration.
// Algorithm: for every point of a square grid over [-2, 0.5] x [-1.25, 1.25],
// iterate z = z * z + c up to 200 times.
// Sizes: 1,400 x 1,400; small: 200 x 200.
// Output: the points that never escaped and the total number of iterations.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn main() {
    let side = size(1400, 200);
    let mut inside = 0;
    let mut total = 0;
    for row in 0..side {
        let ci = -1.25 + 2.5 * row as f64 / side as f64;
        for column in 0..side {
            let cr = -2.0 + 2.5 * column as f64 / side as f64;
            let (mut zr, mut zi) = (0.0f64, 0.0f64);
            let mut n = 0;
            while n < 200 && zr * zr + zi * zi <= 4.0 {
                let next = zr * zr - zi * zi + cr;
                zi = 2.0 * zr * zi + ci;
                zr = next;
                n += 1;
            }
            if n == 200 {
                inside += 1;
            }
            total += n;
        }
    }
    println!("{} {}", inside, total);
}
