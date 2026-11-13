// Run-time benchmark: the same computation as bench/sort.azl.
// The numbers are in one malloc'd array, the natural C counterpart of the
// list the other versions push onto.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

static void quicksort(int64_t *xs, int64_t low, int64_t high) {
    if (low >= high) {
        return;
    }
    int64_t pivot = xs[(low + high) / 2];
    int64_t i = low;
    int64_t j = high;
    while (i <= j) {
        while (xs[i] < pivot) {
            i++;
        }
        while (xs[j] > pivot) {
            j--;
        }
        if (i <= j) {
            int64_t held = xs[i];
            xs[i] = xs[j];
            xs[j] = held;
            i++;
            j--;
        }
    }
    quicksort(xs, low, j);
    quicksort(xs, i, high);
}

int main(void) {
    volatile int64_t input = 2000000;
    volatile int64_t first = 42;
    int64_t count = input;
    int64_t seed = first;
    int64_t *xs = malloc((size_t)count * sizeof *xs);
    if (xs == NULL) {
        return 1;
    }
    for (int64_t n = 0; n < count; n++) {
        seed = seed * 48271 % 2147483647;
        xs[n] = seed % 1000000;
    }
    quicksort(xs, 0, count - 1);
    int64_t checksum = 0;
    for (int64_t k = 0; k < count; k += 1000) {
        checksum += xs[k] * (k % 7);
    }
    int64_t ordered = 1;
    for (int64_t m = 1; m < count; m++) {
        if (xs[m - 1] > xs[m]) {
            ordered = 0;
        }
    }
    printf("%lld\n", (long long)(checksum + ordered));
    free(xs);
    return 0;
}
