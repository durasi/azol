// append: building long strings.
// Algorithm: append a ten-byte piece many times; build many short lines
// from numbers and join them.
// Sizes: 12,000,000 pieces and 1,200,000 lines; small: 300,000 and 30,000.
// Output: the two lengths added.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn main() {
    let pieces = size(12000000, 300000);
    let count = size(1200000, 30000);
    let mut text = String::new();
    for _ in 0..pieces {
        text.push_str("0123456789");
    }
    let lines: Vec<String> = (0..count).map(|n| format!("line {}", n)).collect();
    let joined = lines.join("\n");
    println!("{}", text.len() + joined.len());
}
