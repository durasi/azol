// Run-time benchmark: the same computation as bench/words.azl.

fn main() {
    let count: i64 = std::hint::black_box(400000);
    let names = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"];
    let mut text = String::new();
    let mut seed: i64 = 11;
    let mut n = 0;
    while n < count {
        seed = seed * 48271 % 2147483647;
        if n > 0 {
            text.push_str(" ");
        }
        text.push_str(names[(seed % 8) as usize]);
        n += 1;
    }
    let words: Vec<String> = text.split(" ").map(|word| word.to_string()).collect();
    let mut long = 0;
    let mut with_t = 0;
    for word in &words {
        if word.len() > 4 {
            long += 1;
        }
        if word.contains("t") {
            with_t += 1;
        }
    }
    let joined = words.join(",");
    let parts: Vec<String> = joined.split(",").map(|part| part.to_string()).collect();
    let mut starts = 0;
    for part in &parts {
        if part.starts_with("e") {
            starts += 1;
        }
    }
    println!("{}", words.len() + long * 3 + with_t * 5 + starts * 7 + joined.len());
}
