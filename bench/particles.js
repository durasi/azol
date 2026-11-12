// Run-time benchmark: the same computation as bench/particles.azl.

const WIDTH = 100000;
const HEIGHT = 60000;

function step(ps) {
    let i = 0;
    while (i < ps.length) {
        let x = ps[i].x + ps[i].vx;
        let y = ps[i].y + ps[i].vy;
        if (x < 0 || x > WIDTH) {
            ps[i].vx = 0 - ps[i].vx;
            x = x + 2 * ps[i].vx;
        }
        if (y < 0 || y > HEIGHT) {
            ps[i].vy = 0 - ps[i].vy;
            y = y + 2 * ps[i].vy;
        }
        ps[i].x = x;
        ps[i].y = y;
        i += 1;
    }
}

function energy(ps) {
    let total = 0;
    for (const p of ps) {
        total = total + p.x * 3 + p.y * 5 + p.vx * p.vx + p.vy * p.vy;
    }
    return total;
}

const ps = [];
let seed = 7;
let n = 0;
while (n < 2000) {
    seed = seed * 48271 % 2147483647;
    const x = seed % WIDTH;
    seed = seed * 48271 % 2147483647;
    const y = seed % HEIGHT;
    seed = seed * 48271 % 2147483647;
    const vx = seed % 201 - 100;
    seed = seed * 48271 % 2147483647;
    const vy = seed % 201 - 100;
    ps.push({ x: x, y: y, vx: vx, vy: vy });
    n += 1;
}
let round = 0;
let total = 0;
while (round < 50000) {
    step(ps);
    if (round % 500 === 0) {
        total += energy(ps) % 1000003;
    }
    round += 1;
}
console.log(total + energy(ps));
