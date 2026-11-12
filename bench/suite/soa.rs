// soa: a struct of arrays with element-wise arithmetic.
// Algorithm: four equally long lists; every round adds velocities to
// positions and stirs one velocity; then a weighted sum.
// Sizes: 100,000 elements, 3,000 rounds; small: 1,000 elements, 3,000 rounds.
// Output: the weighted sum.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

struct World {
    x: Vec<i64>,
    y: Vec<i64>,
    vx: Vec<i64>,
    vy: Vec<i64>,
}

fn step(w: &mut World) {
    let positions = w.x.iter_mut().zip(w.y.iter_mut());
    let velocities = w.vx.iter_mut().zip(w.vy.iter());
    for ((x, y), (vx, vy)) in positions.zip(velocities) {
        *x += *vx;
        *y += *vy;
        *vx ^= *y & 3;
    }
}

fn main() {
    let count = size(100000, 1000);
    let rounds = size(3000, 3000);
    let mut w = World { x: Vec::new(), y: Vec::new(), vx: Vec::new(), vy: Vec::new() };
    let mut seed: i64 = 3;
    for _ in 0..count {
        seed = seed * 48271 % 2147483647;
        w.x.push(seed % 1000);
        w.y.push(seed % 777);
        w.vx.push(seed % 7 - 3);
        w.vy.push(seed % 5 - 2);
    }
    for _ in 0..rounds {
        step(&mut w);
    }
    let total: i64 = w.x.iter().zip(w.y.iter()).map(|(x, y)| x * 3 + y).sum();
    println!("{}", total);
}
