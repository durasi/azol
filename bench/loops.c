// Run-time benchmark: the same computation as bench/loops.azl.
#include <stdint.h>
#include <stdio.h>

int main(void) {
    volatile int64_t input = 5000;
    int64_t size = input;
    int64_t total = 0;
    for (int64_t i = 0; i < size; i++) {
        for (int64_t j = 0; j < size; j++) {
            int64_t product = i * j;
            if (product % 7 == 3) {
                total += product % 11;
            } else {
                total += 1;
            }
        }
    }
    printf("%lld\n", (long long)total);
    return 0;
}
