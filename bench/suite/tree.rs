// tree: allocate and walk trees.
// Algorithm: build a full binary tree out of an enum whose inner nodes own
// a list of children, sum it, free it; repeated.
// Sizes: depth 18, 20 rounds; small: depth 14, 4 rounds.
// Output: the total of all sums.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

enum Tree {
    Leaf(i64),
    Node(Vec<Tree>),
}

fn build(depth: i64, seed: i64) -> Tree {
    if depth == 0 {
        return Tree::Leaf(seed % 10);
    }
    Tree::Node(vec![
        build(depth - 1, seed.wrapping_mul(3) + 1),
        build(depth - 1, seed.wrapping_mul(5) + 2),
    ])
}

fn sum(tree: &Tree) -> i64 {
    match tree {
        Tree::Leaf(value) => *value,
        Tree::Node(children) => 1 + children.iter().map(sum).sum::<i64>(),
    }
}

fn main() {
    let depth = size(18, 14);
    let rounds = size(20, 4);
    let mut total = 0;
    for round in 0..rounds {
        let tree = build(depth, round);
        total += sum(&tree);
    }
    println!("{}", total);
}
