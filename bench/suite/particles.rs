// particles: an array of structs updated in place.
// Algorithm: every round moves each particle by its velocity and bounces
// it off the walls; an energy sum is sampled every 500 rounds.
// Sizes: 2,000 particles, 200,000 rounds; small: 200 particles, 20,000 rounds.
// Output: the sampled energies plus the final energy.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

struct Particle {
    x: i64,
    y: i64,
    vx: i64,
    vy: i64,
}

const WIDTH: i64 = 100000;
const HEIGHT: i64 = 60000;

fn step(ps: &mut [Particle]) {
    for p in ps.iter_mut() {
        let mut x = p.x + p.vx;
        let mut y = p.y + p.vy;
        if x < 0 || x > WIDTH {
            p.vx = -p.vx;
            x += 2 * p.vx;
        }
        if y < 0 || y > HEIGHT {
            p.vy = -p.vy;
            y += 2 * p.vy;
        }
        p.x = x;
        p.y = y;
    }
}

fn energy(ps: &[Particle]) -> i64 {
    ps.iter().map(|p| p.x * 3 + p.y * 5 + p.vx * p.vx + p.vy * p.vy).sum()
}

fn next(seed: &mut i64) -> i64 {
    *seed = *seed * 48271 % 2147483647;
    *seed
}

fn main() {
    let count = size(2000, 200);
    let rounds = size(200000, 20000);
    let mut ps: Vec<Particle> = Vec::new();
    let mut seed: i64 = 7;
    for _ in 0..count {
        let x = next(&mut seed) % WIDTH;
        let y = next(&mut seed) % HEIGHT;
        let vx = next(&mut seed) % 201 - 100;
        let vy = next(&mut seed) % 201 - 100;
        ps.push(Particle { x, y, vx, vy });
    }
    let mut total = 0;
    for round in 0..rounds {
        step(&mut ps);
        if round % 500 == 0 {
            total += energy(&ps) % 1000003;
        }
    }
    println!("{}", total + energy(&ps));
}
