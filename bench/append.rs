// Run-time benchmark: the same computation as bench/append.azl.

fn main() {
    let pieces: i64 = std::hint::black_box(3000000);
    let count: i64 = std::hint::black_box(300000);
    let mut text = String::new();
    let mut i = 0;
    while i < pieces {
        text.push_str("0123456789");
        i += 1;
    }
    let mut lines: Vec<String> = Vec::new();
    let mut n = 0;
    while n < count {
        let line = format!("line {}", n);
        lines.push(line);
        n += 1;
    }
    let joined = lines.join("\n");
    println!("{}", text.len() + joined.len());
}
