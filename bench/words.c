// Run-time benchmark: the same computation as bench/words.azl.
// The text is a growing buffer and `split` gives every word its own
// malloc'd string, as the other versions give each word its own value.
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct {
    char *data;
    size_t len, cap;
} Text;

static void text_push(Text *text, const char *piece, size_t n) {
    if (text->len + n + 1 > text->cap) {
        size_t cap = text->cap ? text->cap : 16;
        while (text->len + n + 1 > cap) {
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
    text->data[text->len] = '\0';
}

typedef struct {
    char **items;
    size_t len, cap;
} Words;

static void words_push(Words *words, const char *from, size_t n) {
    if (words->len == words->cap) {
        words->cap = words->cap ? words->cap * 2 : 16;
        words->items = realloc(words->items, words->cap * sizeof *words->items);
        if (words->items == NULL) {
            exit(1);
        }
    }
    char *word = malloc(n + 1);
    if (word == NULL) {
        exit(1);
    }
    memcpy(word, from, n);
    word[n] = '\0';
    words->items[words->len++] = word;
}

static Words split(const char *text, char separator) {
    Words words = {NULL, 0, 0};
    const char *start = text;
    for (;;) {
        const char *end = strchr(start, separator);
        if (end == NULL) {
            words_push(&words, start, strlen(start));
            return words;
        }
        words_push(&words, start, (size_t)(end - start));
        start = end + 1;
    }
}

static void words_free(Words *words) {
    for (size_t i = 0; i < words->len; i++) {
        free(words->items[i]);
    }
    free(words->items);
}

int main(void) {
    volatile int64_t input = 400000;
    int64_t count = input;
    const char *names[] = {"alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"};
    Text text = {NULL, 0, 0};
    text_push(&text, "", 0);
    int64_t seed = 11;
    for (int64_t n = 0; n < count; n++) {
        seed = seed * 48271 % 2147483647;
        if (n > 0) {
            text_push(&text, " ", 1);
        }
        const char *name = names[seed % 8];
        text_push(&text, name, strlen(name));
    }
    Words words = split(text.data, ' ');
    int64_t long_words = 0;
    int64_t with_t = 0;
    for (size_t i = 0; i < words.len; i++) {
        if (strlen(words.items[i]) > 4) {
            long_words++;
        }
        if (strstr(words.items[i], "t") != NULL) {
            with_t++;
        }
    }
    Text joined = {NULL, 0, 0};
    text_push(&joined, "", 0);
    for (size_t i = 0; i < words.len; i++) {
        if (i > 0) {
            text_push(&joined, ",", 1);
        }
        text_push(&joined, words.items[i], strlen(words.items[i]));
    }
    Words parts = split(joined.data, ',');
    int64_t starts = 0;
    for (size_t i = 0; i < parts.len; i++) {
        if (strncmp(parts.items[i], "e", 1) == 0) {
            starts++;
        }
    }
    printf("%lld\n", (long long)((int64_t)words.len + long_words * 3 + with_t * 5 + starts * 7 + (int64_t)joined.len));
    words_free(&parts);
    words_free(&words);
    free(joined.data);
    free(text.data);
    return 0;
}
