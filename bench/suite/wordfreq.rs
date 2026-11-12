// wordfreq: a map from text to a count.
// Algorithm: build a text of words drawn from 5,000 different ones, split
// it, count every word in a map, then report the number of different words,
// the highest count and the sum of count times word length.
// Sizes: 4,000,000 words; small: 100,000.
// Output: the three numbers on one line.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

use std::collections::HashMap;

fn main() {
    let count = size(4000000, 100000);
    let mut text = String::new();
    let mut seed: i64 = 5;
    for n in 0..count {
        seed = seed * 48271 % 2147483647;
        if n > 0 {
            text.push(' ');
        }
        text.push('w');
        text.push_str(&(seed % 5000).to_string());
    }
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for word in text.split(' ') {
        *counts.entry(word).or_insert(0) += 1;
    }
    let most = counts.values().copied().max().unwrap_or(0);
    let weighted: i64 = counts.iter().map(|(word, times)| times * word.len() as i64).sum();
    println!("{} {} {}", counts.len(), most, weighted);
}
