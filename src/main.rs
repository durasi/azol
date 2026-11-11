// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

mod lexer;
mod token;

use std::env;
use std::fs;
use std::process::ExitCode;
use std::time::Instant;

const USAGE: &str = "azoL compiler (early development)

Usage:
    azol lex <file.azl>            Print the tokens of a source file
    azol lex --time <file.azl>     Lex the file and report the speed only
    azol --version                 Print the version";

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
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run_lex(path: &str, time_only: bool) -> ExitCode {
    let src = match fs::read_to_string(path) {
        Ok(src) => src,
        Err(err) => {
            eprintln!("error: cannot read '{path}': {err}");
            return ExitCode::FAILURE;
        }
    };

    let started = Instant::now();
    let result = lexer::lex(&src);
    let elapsed = started.elapsed();

    let tokens = match result {
        Ok(tokens) => tokens,
        Err(err) => {
            let (line, col) = lexer::line_col(&src, err.offset);
            eprintln!("error: {}\n  --> {path}:{line}:{col}", err.message);
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

    let secs = elapsed.as_secs_f64();
    let mb = src.len() as f64 / (1024.0 * 1024.0);
    let lines = src.lines().count();
    eprintln!(
        "lexed {} bytes, {} lines, {} tokens in {:.3} ms ({:.0} MB/s, {:.1} million lines/s)",
        src.len(),
        lines,
        tokens.len(),
        secs * 1000.0,
        if secs > 0.0 { mb / secs } else { 0.0 },
        if secs > 0.0 { lines as f64 / secs / 1_000_000.0 } else { 0.0 },
    );
    ExitCode::SUCCESS
}
