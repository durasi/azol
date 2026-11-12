// nbody: a floating-point simulation.
// Algorithm: five bodies under gravity, advanced with the symplectic Euler
// step of the classic "n-body" benchmark.
// Sizes: 5,000,000 steps; small: 20,000.
// Output: the energy before and after, times 1,000,000,000 and rounded.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

#[derive(Clone, Copy)]
struct Body {
    x: f64,
    y: f64,
    z: f64,
    vx: f64,
    vy: f64,
    vz: f64,
    mass: f64,
}

const PI: f64 = 3.141592653589793;
const SOLAR_MASS: f64 = 4.0 * PI * PI;
const DAYS: f64 = 365.24;

fn bodies() -> Vec<Body> {
    vec![
        Body { x: 0.0, y: 0.0, z: 0.0, vx: 0.0, vy: 0.0, vz: 0.0, mass: SOLAR_MASS },
        Body {
            x: 4.84143144246472090,
            y: -1.16032004402742839,
            z: -0.103622044471123109,
            vx: 0.00166007664274403694 * DAYS,
            vy: 0.00769901118419740425 * DAYS,
            vz: -0.0000690460016972063023 * DAYS,
            mass: 0.000954791938424326609 * SOLAR_MASS,
        },
        Body {
            x: 8.34336671824457987,
            y: 4.12479856412430479,
            z: -0.403523417114321381,
            vx: -0.00276742510726862411 * DAYS,
            vy: 0.00499852801234917238 * DAYS,
            vz: 0.0000230417297573763929 * DAYS,
            mass: 0.000285885980666130812 * SOLAR_MASS,
        },
        Body {
            x: 12.8943695621391310,
            y: -15.1111514016986312,
            z: -0.223307578892655734,
            vx: 0.00296460137564761618 * DAYS,
            vy: 0.00237847173959480950 * DAYS,
            vz: -0.0000296589568540237556 * DAYS,
            mass: 0.0000436624404335156298 * SOLAR_MASS,
        },
        Body {
            x: 15.3796971148509165,
            y: -25.9193146099879641,
            z: 0.179258772950371181,
            vx: 0.00268067772490389322 * DAYS,
            vy: 0.00162824170038242295 * DAYS,
            vz: -0.0000951592254519715870 * DAYS,
            mass: 0.0000515138902046611451 * SOLAR_MASS,
        },
    ]
}

fn offset_momentum(bs: &mut [Body]) {
    let (mut px, mut py, mut pz) = (0.0, 0.0, 0.0);
    for b in bs.iter() {
        px += b.vx * b.mass;
        py += b.vy * b.mass;
        pz += b.vz * b.mass;
    }
    bs[0].vx = 0.0 - px / SOLAR_MASS;
    bs[0].vy = 0.0 - py / SOLAR_MASS;
    bs[0].vz = 0.0 - pz / SOLAR_MASS;
}

fn energy(bs: &[Body]) -> f64 {
    let mut e = 0.0;
    for (i, b) in bs.iter().enumerate() {
        e += 0.5 * b.mass * (b.vx * b.vx + b.vy * b.vy + b.vz * b.vz);
        for other in &bs[i + 1..] {
            let dx = b.x - other.x;
            let dy = b.y - other.y;
            let dz = b.z - other.z;
            e -= b.mass * other.mass / (dx * dx + dy * dy + dz * dz).sqrt();
        }
    }
    e
}

fn advance(bs: &mut [Body], dt: f64) {
    for i in 0..bs.len() {
        let (head, tail) = bs.split_at_mut(i + 1);
        let a = &mut head[i];
        for b in tail.iter_mut() {
            let dx = a.x - b.x;
            let dy = a.y - b.y;
            let dz = a.z - b.z;
            let squared = dx * dx + dy * dy + dz * dz;
            let mag = dt / (squared * squared.sqrt());
            let mi = a.mass;
            let mj = b.mass;
            a.vx -= dx * mj * mag;
            a.vy -= dy * mj * mag;
            a.vz -= dz * mj * mag;
            b.vx += dx * mi * mag;
            b.vy += dy * mi * mag;
            b.vz += dz * mi * mag;
        }
    }
    for b in bs.iter_mut() {
        b.x += dt * b.vx;
        b.y += dt * b.vy;
        b.z += dt * b.vz;
    }
}

fn scaled(x: f64) -> i64 {
    (x * 1000000000.0).round() as i64
}

fn main() {
    let steps = size(5000000, 20000);
    let mut bs = bodies();
    offset_momentum(&mut bs);
    let before = scaled(energy(&bs));
    for _ in 0..steps {
        advance(&mut bs, 0.01);
    }
    println!("{} {}", before, scaled(energy(&bs)));
}
