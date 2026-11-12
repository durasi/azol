// Run-time benchmark: the same computation as bench/tree.azl.
// The children are in a `Vec`, as they are in a `List` in azoL, so that
// both programs build the same structure.

enum Tree {
    Leaf(i64),
    Node(Vec<Tree>),
}

fn build(depth: i64, seed: i64) -> Tree {
    if depth == 0 {
        return Tree::Leaf(seed % 10);
    }
    Tree::Node(vec![
        build(depth - 1, seed * 3 + 1),
        build(depth - 1, seed * 5 + 2),
    ])
}

fn sum(tree: &Tree) -> i64 {
    match tree {
        Tree::Leaf(value) => *value,
        Tree::Node(children) => {
            let mut total = 1;
            for child in children {
                total += sum(child);
            }
            total
        }
    }
}

fn main() {
    let depth: i64 = std::hint::black_box(18);
    let rounds: i64 = std::hint::black_box(12);
    let mut total = 0;
    let mut round = 0;
    while round < rounds {
        let tree = build(depth, round);
        total += sum(&tree);
        round += 1;
    }
    println!("{}", total);
}
