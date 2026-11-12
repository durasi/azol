// json: parse and re-serialise a document.
// Algorithm: build a JSON text (an array of objects with numbers, strings,
// arrays, booleans and nulls), parse it into a tree by recursive descent,
// sum the numbers by walking the tree, write the tree back out and compare
// with the original.
// Sizes: 120,000 objects (about 11 MB); small: 6,000 objects.
// Output: the length written, the sum of numbers, and whether the text came back unchanged.
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

use std::fmt::Write;

enum Json {
    Null,
    Flag(bool),
    Number(i64),
    Text(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn skip_spaces(&mut self) {
        while self.at < self.text.len() && (self.text[self.at] == b' ' || self.text[self.at] == b'\n') {
            self.at += 1;
        }
    }

    fn parse_text(&mut self) -> String {
        let start = self.at + 1;
        self.at = start;
        while self.text[self.at] != b'"' {
            self.at += 1;
        }
        self.at += 1;
        String::from_utf8_lossy(&self.text[start..self.at - 1]).into_owned()
    }

    fn parse_number(&mut self) -> i64 {
        let negative = self.text[self.at] == b'-';
        if negative {
            self.at += 1;
        }
        let mut n = 0;
        while self.at < self.text.len() && self.text[self.at].is_ascii_digit() {
            n = n * 10 + (self.text[self.at] - b'0') as i64;
            self.at += 1;
        }
        if negative { -n } else { n }
    }

    fn parse_value(&mut self) -> Json {
        self.skip_spaces();
        match self.text[self.at] {
            b'{' => {
                let mut members = Vec::new();
                self.at += 1;
                self.skip_spaces();
                while self.text[self.at] != b'}' {
                    if self.text[self.at] == b',' {
                        self.at += 1;
                        self.skip_spaces();
                    }
                    let key = self.parse_text();
                    self.skip_spaces();
                    self.at += 1;
                    let value = self.parse_value();
                    members.push((key, value));
                    self.skip_spaces();
                }
                self.at += 1;
                Json::Object(members)
            }
            b'[' => {
                let mut items = Vec::new();
                self.at += 1;
                self.skip_spaces();
                while self.text[self.at] != b']' {
                    if self.text[self.at] == b',' {
                        self.at += 1;
                    }
                    items.push(self.parse_value());
                    self.skip_spaces();
                }
                self.at += 1;
                Json::Array(items)
            }
            b'"' => Json::Text(self.parse_text()),
            b't' => {
                self.at += 4;
                Json::Flag(true)
            }
            b'f' => {
                self.at += 5;
                Json::Flag(false)
            }
            b'n' => {
                self.at += 4;
                Json::Null
            }
            _ => Json::Number(self.parse_number()),
        }
    }
}

fn total(value: &Json) -> i64 {
    match value {
        Json::Number(n) => *n,
        Json::Array(items) => items.iter().map(total).sum(),
        Json::Object(members) => members.iter().map(|(_, value)| total(value)).sum(),
        _ => 0,
    }
}

fn write(value: &Json, out: &mut String) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Flag(on) => out.push_str(if *on { "true" } else { "false" }),
        Json::Number(n) => write!(out, "{}", n).unwrap(),
        Json::Text(text) => {
            out.push('"');
            out.push_str(text);
            out.push('"');
        }
        Json::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(item, out);
            }
            out.push(']');
        }
        Json::Object(members) => {
            out.push('{');
            for (i, (key, value)) in members.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('"');
                out.push_str(key);
                out.push_str("\":");
                write(value, out);
            }
            out.push('}');
        }
    }
}

fn document(objects: i64) -> String {
    let mut text = String::from("[");
    let mut seed: i64 = 83;
    for n in 0..objects {
        seed = seed * 48271 % 2147483647;
        if n > 0 {
            text.push(',');
        }
        write!(
            text,
            "{{\"id\":{},\"name\":\"item{}\",\"tags\":[\"red\",\"green\",{}],\"active\":{},\"score\":{},\"extra\":null}}",
            n,
            seed % 100000,
            seed % 7,
            seed % 3 == 0,
            seed % 2001 - 1000
        )
        .unwrap();
    }
    text.push(']');
    text
}

fn main() {
    let text = document(size(120000, 6000));
    let mut parser = Parser { text: text.as_bytes(), at: 0 };
    let tree = parser.parse_value();
    let mut out = String::new();
    write(&tree, &mut out);
    println!("{} {} {}", out.len(), total(&tree), out == text);
}
