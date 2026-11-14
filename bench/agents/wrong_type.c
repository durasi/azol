// bench/fib40 with one mistake, the same in every language: the text "40" is
// passed where a number is expected. bench/compare.py runs each language's
// checker on its file and reports what the checker says and in what form.
#include <stdint.h>
#include <stdio.h>

static int64_t fib(int64_t n) {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

int main(void) {
    printf("%lld\n", (long long)fib("40"));
    return 0;
}
