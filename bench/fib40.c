// Run-time benchmark: the same computation as bench/fib40.azl.
// The argument is read through a `volatile`, as the Rust version reads it
// through `black_box`, so that the compiler cannot work the answer out.
#include <stdint.h>
#include <stdio.h>

static int64_t fib(int64_t n) {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

int main(void) {
    volatile int64_t n = 40;
    printf("%lld\n", (long long)fib(n));
    return 0;
}
