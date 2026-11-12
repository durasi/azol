// Run-time benchmark: the same computation as bench/tree.azl.

function build(depth, seed) {
    if (depth === 0) {
        return { leaf: true, value: seed % 10 };
    }
    return { leaf: false, children: [build(depth - 1, seed * 3 + 1), build(depth - 1, seed * 5 + 2)] };
}

function sum(tree) {
    if (tree.leaf) {
        return tree.value;
    }
    let total = 1;
    for (const child of tree.children) {
        total += sum(child);
    }
    return total;
}

let total = 0;
let round = 0;
while (round < 12) {
    const tree = build(18, round);
    total += sum(tree);
    round += 1;
}
console.log(total);
