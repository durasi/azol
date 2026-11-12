// dijkstra: shortest paths with a binary-heap priority queue.
// Algorithm: a directed graph with four random out-edges per node and
// weights from 1 to 100; Dijkstra's algorithm with a binary heap and stale
// entries skipped.
// Rust uses `BinaryHeap`; azoL has no priority queue, so the heap is
// written out over two lists.
// Sizes: 400,000 nodes; small: 30,000.
// Output: nodes reached, and the sum of their distances modulo 1,000,000,007.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

use std::cmp::Reverse;
use std::collections::BinaryHeap;

fn main() {
    let nodes = size(400000, 30000) as usize;
    let mut edges: Vec<(usize, i64)> = Vec::new();
    let mut seed: i64 = 79;
    for _ in 0..nodes * 4 {
        seed = seed * 48271 % 2147483647;
        let target = (seed % nodes as i64) as usize;
        seed = seed * 48271 % 2147483647;
        edges.push((target, 1 + seed % 100));
    }
    let mut best = vec![-1i64; nodes];
    let mut heap = BinaryHeap::new();
    heap.push(Reverse((0i64, 0usize)));
    let mut reached = 0;
    let mut total = 0;
    while let Some(Reverse((d, at))) = heap.pop() {
        if best[at] >= 0 {
            continue;
        }
        best[at] = d;
        reached += 1;
        total = (total + d) % 1000000007;
        for &(target, weight) in &edges[at * 4..at * 4 + 4] {
            if best[target] < 0 {
                heap.push(Reverse((d + weight, target)));
            }
        }
    }
    println!("{} {}", reached, total);
}
