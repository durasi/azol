// matcher: glob matching with backtracking.
// Algorithm: patterns with `?` (any one byte) and `*` (any run of bytes),
// matched by trying every length for each star; every pattern against
// every text.
// Sizes: 200,000 texts of 12 to 27 letters over `abcd`, 12 patterns; small: 6,000 texts.
// Output: the number of matches.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn matches(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => (0..=text.len()).any(|k| matches(rest, &text[k..])),
        Some((&c, rest)) => match text.split_first() {
            Some((&d, more)) if c == b'?' || c == d => matches(rest, more),
            _ => false,
        },
    }
}

fn main() {
    let count = size(200000, 6000);
    let patterns = ["a*b*c*d", "*abc*", "?b*d?", "a*a*a*a*b", "*d", "c???*", "*a?c*d*", "b*", "*cc*dd*", "??*??*??", "*abcd*abcd*", "d*c*b*a"];
    let mut texts: Vec<Vec<u8>> = Vec::new();
    let mut seed: i64 = 89;
    for _ in 0..count {
        seed = seed * 48271 % 2147483647;
        let length = 12 + seed % 16;
        let mut text = Vec::new();
        for _ in 0..length {
            seed = seed * 48271 % 2147483647;
            text.push(97 + (seed % 4) as u8);
        }
        texts.push(text);
    }
    let mut found = 0;
    for pattern in patterns {
        for text in &texts {
            if matches(pattern.as_bytes(), text) {
                found += 1;
            }
        }
    }
    println!("{}", found);
}
