// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

mod ast;
mod dump;
mod lexer;
mod parser;
mod token;

use std::env;
use std::fs;
use std::process::ExitCode;
use std::time::Instant;

const USAGE: &str = "azoL compiler (early development)

Usage:
    azol lex <file.azl>              Print the tokens of a source file
    azol lex --time <file.azl>       Lex the file and report the speed only
    azol parse <file.azl>            Print the syntax tree of a source file
    azol parse --time <file.azl>     Lex and parse the file and report the speed only
    azol --version                   Print the version";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    match args.as_slice() {
        ["--version"] => {
            println!("azol {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        ["lex", path] => run_lex(path, false),
        ["lex", "--time", path] => run_lex(path, true),
        ["parse", path] => run_parse(path, false),
        ["parse", "--time", path] => run_parse(path, true),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn read_source(path: &str) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(src) => Some(src),
        Err(err) => {
            eprintln!("error: cannot read '{path}': {err}");
            None
        }
    }
}

/// Prints an error with the offending source line and a caret under it.
fn report(path: &str, src: &str, offset: u32, message: &str) {
    let (line, col) = lexer::line_col(src, offset);
    let text = src.lines().nth(line - 1).unwrap_or("");
    let gutter = line.to_string().len();
    eprintln!("error: {message}");
    eprintln!("{:gutter$}--> {path}:{line}:{col}", "");
    eprintln!("{:gutter$} |", "");
    eprintln!("{line} | {text}");
    eprintln!("{:gutter$} | {:pad$}^", "", "", pad = col - 1);
}

fn speed(bytes: usize, lines: usize, secs: f64) -> String {
    if secs <= 0.0 {
        return String::from("too fast to measure");
    }
    let mb = bytes as f64 / (1024.0 * 1024.0);
    format!("{:.0} MB/s, {:.1} million lines/s", mb / secs, lines as f64 / secs / 1_000_000.0)
}

fn run_lex(path: &str, time_only: bool) -> ExitCode {
    let Some(src) = read_source(path) else {
        return ExitCode::FAILURE;
    };

    let started = Instant::now();
    let result = lexer::lex(&src);
    let elapsed = started.elapsed().as_secs_f64();

    let tokens = match result {
        Ok(tokens) => tokens,
        Err(err) => {
            report(path, &src, err.offset, err.message);
            return ExitCode::FAILURE;
        }
    };

    if !time_only {
        for token in &tokens {
            let (line, col) = lexer::line_col(&src, token.start);
            let text = match token.kind {
                token::TokenKind::Newline => "\\n",
                token::TokenKind::Eof => "",
                _ => token.text(&src),
            };
            println!("{line:>4}:{col:<4} {:<10} {text}", format!("{:?}", token.kind));
        }
    }

    let lines = src.lines().count();
    eprintln!(
        "lexed {} bytes, {} lines, {} tokens in {:.3} ms ({})",
        src.len(),
        lines,
        tokens.len(),
        elapsed * 1000.0,
        speed(src.len(), lines, elapsed),
    );
    ExitCode::SUCCESS
}

fn run_parse(path: &str, time_only: bool) -> ExitCode {
    let Some(src) = read_source(path) else {
        return ExitCode::FAILURE;
    };

    let started = Instant::now();
    let tokens = match lexer::lex(&src) {
        Ok(tokens) => tokens,
        Err(err) => {
            report(path, &src, err.offset, err.message);
            return ExitCode::FAILURE;
        }
    };
    let lexed = started.elapsed().as_secs_f64();

    let ast = match parser::parse(&src, &tokens) {
        Ok(ast) => ast,
        Err(err) => {
            report(path, &src, err.offset, &err.message);
            return ExitCode::FAILURE;
        }
    };
    let total = started.elapsed().as_secs_f64();

    if !time_only {
        print!("{}", dump::dump(&ast, &src));
    }

    let lines = src.lines().count();
    eprintln!(
        "parsed {} lines into {} nodes in {:.3} ms (lex {:.3} ms + parse {:.3} ms; {})",
        lines,
        ast.node_count(),
        total * 1000.0,
        lexed * 1000.0,
        (total - lexed) * 1000.0,
        speed(src.len(), lines, total),
    );
    ExitCode::SUCCESS
}
