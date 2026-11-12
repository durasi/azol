// binary_trees: the classic allocation benchmark.
// Algorithm: build and count a stretch tree, keep one long-lived tree, and
// for every second depth build and count many short-lived trees.
// azoL has no boxed pointer: an inner node owns a list of its two children.
// Sizes: maximum depth 17; small: 12.
// Output: one line per step: depth, number of trees, total count.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

enum Tree {
    Leaf,
    Node(Box<Tree>, Box<Tree>),
}

fn build(depth: i64) -> Tree {
    if depth == 0 {
        return Tree::Leaf;
    }
    Tree::Node(Box::new(build(depth - 1)), Box::new(build(depth - 1)))
}

fn check(tree: &Tree) -> i64 {
    match tree {
        Tree::Leaf => 1,
        Tree::Node(left, right) => 1 + check(left) + check(right),
    }
}

fn main() {
    let max_depth = size(17, 12);
    let stretch = build(max_depth + 1);
    println!("{} 1 {}", max_depth + 1, check(&stretch));
    let long_lived = build(max_depth);
    for depth in (4..=max_depth).step_by(2) {
        let trees = 1i64 << (max_depth - depth + 4);
        let total: i64 = (0..trees).map(|_| check(&build(depth))).sum();
        println!("{} {} {}", depth, trees, total);
    }
    println!("{} 1 {}", max_depth, check(&long_lived));
}
