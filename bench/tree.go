// Run-time benchmark: the same computation as bench/tree.azl.
// A node holds a slice of its children, as the other versions hold them
// in a list; a leaf has none. The garbage collector frees the trees.
package main

import "fmt"

type Tree struct {
	value    int64
	children []Tree
}

var (
	inputDepth  int64 = 18
	inputRounds int64 = 12
)

func build(depth, seed int64) Tree {
	if depth == 0 {
		return Tree{value: seed % 10}
	}
	return Tree{children: []Tree{build(depth-1, seed*3+1), build(depth-1, seed*5+2)}}
}

func sum(tree *Tree) int64 {
	if tree.children == nil {
		return tree.value
	}
	total := int64(1)
	for i := range tree.children {
		total += sum(&tree.children[i])
	}
	return total
}

func main() {
	depth, rounds := inputDepth, inputRounds
	var total int64
	for round := int64(0); round < rounds; round++ {
		tree := build(depth, round)
		total += sum(&tree)
	}
	fmt.Println(total)
}
