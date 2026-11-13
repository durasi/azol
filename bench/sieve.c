// Run-time benchmark: the same computation as bench/sieve.azl.
// One byte per number, zeroed by calloc: what a C programmer would write
// where the other versions push `false` onto a list.
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

int main(void) {
    volatile int64_t input = 20000000;
    int64_t limit = input;
    bool *composite = calloc((size_t)limit + 1, sizeof *composite);
    if (composite == NULL) {
        return 1;
    }
    int64_t count = 0;
    int64_t last = 0;
    for (int64_t i = 2; i <= limit; i++) {
        if (!composite[i]) {
            count++;
            last = i;
            for (int64_t j = i * i; j <= limit; j += i) {
                composite[j] = true;
            }
        }
    }
    printf("%lld\n", (long long)(count * 100000000 + last));
    free(composite);
    return 0;
}
