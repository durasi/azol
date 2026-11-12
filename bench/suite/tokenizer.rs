// tokenizer: a lexer over source text.
// Algorithm: split azoL-like source into identifiers, keywords, numbers
// (decimal and hexadecimal), strings, operators of one or two characters
// and line ends; comments are skipped. A compact lexer written for the
// suite, not the compiler's own.
// Sizes: 2,400,000 lines; small: 14,000 lines.
// Output: counts of identifiers, keywords, numbers, strings, operators and
// lines, and the sum of the numbers modulo 1,000,000,007.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn is_letter(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn source(lines: i64) -> String {
    let snippet = [
        "// Adds the weights of every edge that leaves a node.",
        "fn total_weight(graph: Graph, node: i64) -> i64 {",
        "    var sum = 0",
        "    for edge in graph.edges {",
        "        if edge.source == node && edge.weight >= 10 {",
        "            sum = sum + edge.weight * 3 - 0x1F",
        "        } else if edge.label != \"skip me\" || node << 2 > 4096 {",
        "            sum = sum - 1",
        "        }",
        "    }",
        "    let message = \"total: \" + str(sum)   // shown to the user",
        "    return sum % 1000007",
        "}",
        "",
    ];
    let mut text = String::new();
    for n in 0..lines as usize {
        text.push_str(snippet[n % snippet.len()]);
        text.push('\n');
    }
    text
}

fn is_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"fn" | b"let"
            | b"var"
            | b"mut"
            | b"const"
            | b"struct"
            | b"enum"
            | b"if"
            | b"else"
            | b"match"
            | b"while"
            | b"for"
            | b"in"
            | b"break"
            | b"continue"
            | b"return"
            | b"true"
            | b"false"
    )
}

fn main() {
    let source = source(size(2400000, 14000));
    let text = source.as_bytes();
    let (mut identifiers, mut keywords, mut numbers, mut strings, mut operators, mut lines) = (0, 0, 0, 0, 0, 0);
    let mut sum: i64 = 0;
    let mut at = 0;
    let end = text.len();
    while at < end {
        let c = text[at];
        if c == b' ' {
            at += 1;
        } else if c == b'\n' {
            lines += 1;
            at += 1;
        } else if is_letter(c) {
            let start = at;
            while at < end && (is_letter(text[at]) || text[at].is_ascii_digit()) {
                at += 1;
            }
            if is_keyword(&text[start..at]) {
                keywords += 1;
            } else {
                identifiers += 1;
            }
        } else if c.is_ascii_digit() {
            let mut value: i64 = 0;
            if c == b'0' && at + 1 < end && text[at + 1] == b'x' {
                at += 2;
                while at < end {
                    match (text[at] as char).to_digit(16) {
                        Some(digit) => value = value * 16 + digit as i64,
                        None => break,
                    }
                    at += 1;
                }
            } else {
                while at < end && text[at].is_ascii_digit() {
                    value = value * 10 + (text[at] - b'0') as i64;
                    at += 1;
                }
            }
            numbers += 1;
            sum = (sum + value) % 1000000007;
        } else if c == b'"' {
            at += 1;
            while at < end && text[at] != b'"' {
                at += 1;
            }
            at += 1;
            strings += 1;
        } else if c == b'/' && at + 1 < end && text[at + 1] == b'/' {
            while at < end && text[at] != b'\n' {
                at += 1;
            }
        } else {
            let mut width = 1;
            if at + 1 < end {
                let d = text[at + 1];
                if (d == b'=' && matches!(c, b'=' | b'!' | b'<' | b'>'))
                    || (c == d && matches!(c, b'&' | b'|' | b'<' | b'>'))
                    || (d == b'>' && matches!(c, b'-' | b'='))
                {
                    width = 2;
                }
            }
            operators += 1;
            at += width;
        }
    }
    println!("{} {} {} {} {} {} {}", identifiers, keywords, numbers, strings, operators, lines, sum);
}
