// Run-time benchmark: the same computation as bench/particles.azl.
// An array of structs updated in place.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct {
    int64_t x, y, vx, vy;
} Particle;

#define WIDTH 100000
#define HEIGHT 60000

static void step(Particle *ps, int64_t count) {
    for (int64_t i = 0; i < count; i++) {
        int64_t x = ps[i].x + ps[i].vx;
        int64_t y = ps[i].y + ps[i].vy;
        if (x < 0 || x > WIDTH) {
            ps[i].vx = 0 - ps[i].vx;
            x = x + 2 * ps[i].vx;
        }
        if (y < 0 || y > HEIGHT) {
            ps[i].vy = 0 - ps[i].vy;
            y = y + 2 * ps[i].vy;
        }
        ps[i].x = x;
        ps[i].y = y;
    }
}

static int64_t energy(const Particle *ps, int64_t count) {
    int64_t total = 0;
    for (int64_t i = 0; i < count; i++) {
        total = total + ps[i].x * 3 + ps[i].y * 5 + ps[i].vx * ps[i].vx + ps[i].vy * ps[i].vy;
    }
    return total;
}

int main(void) {
    volatile int64_t input_count = 2000;
    volatile int64_t input_rounds = 50000;
    int64_t count = input_count;
    int64_t rounds = input_rounds;
    Particle *ps = malloc((size_t)count * sizeof *ps);
    if (ps == NULL) {
        return 1;
    }
    int64_t seed = 7;
    for (int64_t n = 0; n < count; n++) {
        seed = seed * 48271 % 2147483647;
        ps[n].x = seed % WIDTH;
        seed = seed * 48271 % 2147483647;
        ps[n].y = seed % HEIGHT;
        seed = seed * 48271 % 2147483647;
        ps[n].vx = seed % 201 - 100;
        seed = seed * 48271 % 2147483647;
        ps[n].vy = seed % 201 - 100;
    }
    int64_t total = 0;
    for (int64_t round = 0; round < rounds; round++) {
        step(ps, count);
        if (round % 500 == 0) {
            total += energy(ps, count) % 1000003;
        }
    }
    printf("%lld\n", (long long)(total + energy(ps, count)));
    free(ps);
    return 0;
}
