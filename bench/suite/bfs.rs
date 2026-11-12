// bfs: breadth-first search on a grid.
// Algorithm: a square grid with about 30% of the cells blocked; from every
// open cell not seen yet, search through the four neighbours with a queue.
// Rust uses `VecDeque`; azoL has no queue type, so the queue is a list as
// long as the grid with a read position.
// Sizes: 4,000 x 4,000; small: 400 x 400.
// Output: the number of separate regions, cells reached, the largest
// distance, and the sum of distances modulo 1,000,000,007.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

use std::collections::VecDeque;

fn main() {
    let side = size(4000, 400) as usize;
    let cells = side * side;
    let mut blocked: Vec<bool> = Vec::new();
    let mut seed: i64 = 73;
    for _ in 0..cells {
        seed = seed * 48271 % 2147483647;
        blocked.push(seed % 100 < 30);
    }
    let mut distance = vec![-1i64; cells];
    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut parts = 0;
    let mut reached = 0;
    let mut farthest = 0;
    let mut total = 0;
    for start in 0..cells {
        if blocked[start] || distance[start] >= 0 {
            continue;
        }
        parts += 1;
        distance[start] = 0;
        queue.push_back(start);
        while let Some(cell) = queue.pop_front() {
            let d = distance[cell];
            reached += 1;
            farthest = farthest.max(d);
            total = (total + d) % 1000000007;
            let x = cell % side;
            let mut visit = |next: usize| {
                if !blocked[next] && distance[next] < 0 {
                    distance[next] = d + 1;
                    queue.push_back(next);
                }
            };
            if x > 0 {
                visit(cell - 1);
            }
            if x < side - 1 {
                visit(cell + 1);
            }
            if cell >= side {
                visit(cell - side);
            }
            if cell + side < cells {
                visit(cell + side);
            }
        }
    }
    println!("{} {} {} {}", parts, reached, farthest, total);
}
