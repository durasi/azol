// words: split, join and count.
// Algorithm: build a text of words, split it, count by length and content,
// join with commas, split again and count by first letter.
// Sizes: 4,000,000 words; small: 40,000.
// Output: a combination of the counts and the joined length.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn main() {
    let count = size(4000000, 40000);
    let names = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"];
    let mut text = String::new();
    let mut seed: i64 = 11;
    for n in 0..count {
        seed = seed * 48271 % 2147483647;
        if n > 0 {
            text.push(' ');
        }
        text.push_str(names[(seed % 8) as usize]);
    }
    let words: Vec<&str> = text.split(' ').collect();
    let long = words.iter().filter(|word| word.len() > 4).count();
    let with_t = words.iter().filter(|word| word.contains('t')).count();
    let joined = words.join(",");
    let parts: Vec<&str> = joined.split(',').collect();
    let starts = parts.iter().filter(|part| part.starts_with('e')).count();
    println!("{}", words.len() + long * 3 + with_t * 5 + starts * 7 + joined.len());
}
