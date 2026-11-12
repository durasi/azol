// Run-time benchmark: the same computation as bench/particles.azl.

struct Particle {
    x: i64,
    y: i64,
    vx: i64,
    vy: i64,
}

const WIDTH: i64 = 100000;
const HEIGHT: i64 = 60000;

fn step(ps: &mut Vec<Particle>) {
    let mut i = 0;
    while i < ps.len() {
        let mut x = ps[i].x + ps[i].vx;
        let mut y = ps[i].y + ps[i].vy;
        if x < 0 || x > WIDTH {
            ps[i].vx = 0 - ps[i].vx;
            x = x + 2 * ps[i].vx;
        }
        if y < 0 || y > HEIGHT {
            ps[i].vy = 0 - ps[i].vy;
            y = y + 2 * ps[i].vy;
        }
        ps[i].x = x;
        ps[i].y = y;
        i += 1;
    }
}

fn energy(ps: &Vec<Particle>) -> i64 {
    let mut total = 0;
    for p in ps {
        total = total + p.x * 3 + p.y * 5 + p.vx * p.vx + p.vy * p.vy;
    }
    total
}

fn main() {
    let count: i64 = std::hint::black_box(2000);
    let rounds: i64 = std::hint::black_box(50000);
    let mut ps: Vec<Particle> = Vec::new();
    let mut seed: i64 = 7;
    let mut n = 0;
    while n < count {
        seed = seed * 48271 % 2147483647;
        let x = seed % WIDTH;
        seed = seed * 48271 % 2147483647;
        let y = seed % HEIGHT;
        seed = seed * 48271 % 2147483647;
        let vx = seed % 201 - 100;
        seed = seed * 48271 % 2147483647;
        let vy = seed % 201 - 100;
        ps.push(Particle { x, y, vx, vy });
        n += 1;
    }
    let mut round = 0;
    let mut total = 0;
    while round < rounds {
        step(&mut ps);
        if round % 500 == 0 {
            total += energy(&ps) % 1000003;
        }
        round += 1;
    }
    println!("{}", total + energy(&ps));
}
