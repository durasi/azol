// Run-time benchmark: the same computation as bench/append.azl.
// C has no string type that grows, so this file carries the usual one: a
// buffer with a length and a capacity that doubles when it is full.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    char *data;
    size_t len, cap;
} Text;

static void text_push(Text *text, const char *piece, size_t n) {
    if (text->len + n > text->cap) {
        size_t cap = text->cap ? text->cap : 16;
        while (text->len + n > cap) {
            cap *= 2;
        }
        text->data = realloc(text->data, cap);
        if (text->data == NULL) {
            exit(1);
        }
        text->cap = cap;
    }
    memcpy(text->data + text->len, piece, n);
    text->len += n;
}

int main(void) {
    volatile int64_t input_pieces = 3000000;
    volatile int64_t input_count = 300000;
    int64_t pieces = input_pieces;
    int64_t count = input_count;
    Text text = {NULL, 0, 0};
    for (int64_t i = 0; i < pieces; i++) {
        text_push(&text, "0123456789", 10);
    }
    char **lines = malloc((size_t)count * sizeof *lines);
    if (lines == NULL) {
        return 1;
    }
    for (int64_t n = 0; n < count; n++) {
        char line[32];
        int len = snprintf(line, sizeof line, "line %lld", (long long)n);
        lines[n] = malloc((size_t)len + 1);
        if (lines[n] == NULL) {
            return 1;
        }
        memcpy(lines[n], line, (size_t)len + 1);
    }
    Text joined = {NULL, 0, 0};
    for (int64_t n = 0; n < count; n++) {
        if (n > 0) {
            text_push(&joined, "\n", 1);
        }
        text_push(&joined, lines[n], strlen(lines[n]));
    }
    printf("%lld\n", (long long)(text.len + joined.len));
    for (int64_t n = 0; n < count; n++) {
        free(lines[n]);
    }
    free(lines);
    free(joined.data);
    free(text.data);
    return 0;
}
