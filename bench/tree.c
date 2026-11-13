// Run-time benchmark: the same computation as bench/tree.azl.
// A node owns a malloc'd array of its two children, as the other versions
// keep them in a list, so that all of them build the same structure and
// free it again.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct Tree {
    int64_t value;         // of a leaf
    int64_t count;         // children; 0 for a leaf
    struct Tree *children; // NULL for a leaf
} Tree;

static Tree build(int64_t depth, int64_t seed) {
    Tree tree = {0, 0, NULL};
    if (depth == 0) {
        tree.value = seed % 10;
        return tree;
    }
    tree.count = 2;
    tree.children = malloc(2 * sizeof *tree.children);
    if (tree.children == NULL) {
        exit(1);
    }
    tree.children[0] = build(depth - 1, seed * 3 + 1);
    tree.children[1] = build(depth - 1, seed * 5 + 2);
    return tree;
}

static int64_t sum(const Tree *tree) {
    if (tree->children == NULL) {
        return tree->value;
    }
    int64_t total = 1;
    for (int64_t i = 0; i < tree->count; i++) {
        total += sum(&tree->children[i]);
    }
    return total;
}

static void release(Tree *tree) {
    for (int64_t i = 0; i < tree->count; i++) {
        release(&tree->children[i]);
    }
    free(tree->children);
}

int main(void) {
    volatile int64_t input_depth = 18;
    volatile int64_t input_rounds = 12;
    int64_t depth = input_depth;
    int64_t rounds = input_rounds;
    int64_t total = 0;
    for (int64_t round = 0; round < rounds; round++) {
        Tree tree = build(depth, round);
        total += sum(&tree);
        release(&tree);
    }
    printf("%lld\n", (long long)total);
    return 0;
}
