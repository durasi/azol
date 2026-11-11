// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

mod arm64;
mod ast;
mod dump;
mod lexer;
mod macho;
mod parser;
mod runtime;
mod sema;
mod sha256;
mod token;
mod elf;
mod runtime_x64;
mod x64;

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
    azol build <file.azl>            Compile to an executable next to the source file
    azol build <file.azl> -o <out>   Compile to the given output path
    azol build <a.azl> <b.azl> ...   Compile several files as one program; the
                                     executable is named after the first
    azol build --target <target> ... Compile for `aarch64-macos` or `x86_64-linux`
                                     instead of the machine the compiler runs on
    azol --version                   Print the version";

/// What kind of machine an executable is for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    /// Apple Silicon Macs: ARM64 code in a signed Mach-O file.
    Aarch64Macos,
    /// Linux on Intel and AMD processors: x86-64 code in a static ELF file.
    X86_64Linux,
}

impl Target {
    /// The machine the compiler itself runs on, when it can compile for
    /// it; otherwise the first target it learned.
    fn host() -> Target {
        if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Target::X86_64Linux
        } else {
            Target::Aarch64Macos
        }
    }

    fn parse(name: &str) -> Option<Target> {
        match name {
            "aarch64-macos" => Some(Target::Aarch64Macos),
            "x86_64-linux" => Some(Target::X86_64Linux),
            _ => None,
        }
    }
}

/// Reads the arguments of `azol build`: the source files, and `-o` and
/// `--target` with their values, in any order.
fn build_arguments<'a>(args: &[&'a str]) -> Option<(Vec<&'a str>, Option<&'a str>, Target)> {
    let (mut paths, mut out, mut target) = (Vec::new(), None, Target::host());
    let mut rest = args.iter();
    while let Some(&arg) = rest.next() {
        match arg {
            "-o" => out = Some(*rest.next()?),
            "--target" => {
                let name = rest.next()?;
                let Some(known) = Target::parse(name) else {
                    eprintln!("error: unknown target '{name}'; the targets are `aarch64-macos` and `x86_64-linux`");
                    return None;
                };
                target = known;
            }
            _ if !arg.starts_with('-') => paths.push(arg),
            _ => return None,
        }
    }
    if paths.is_empty() {
        return None;
    }
    Some((paths, out, target))
}

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
        ["build", rest @ ..] => match build_arguments(rest) {
            Some((paths, out, target)) => run_build(&paths, out, target),
            None => {
                eprintln!("{USAGE}");
                ExitCode::from(2)
            }
        },
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
/// A very long line is cut down to the part around the problem.
fn report(path: &str, src: &str, offset: u32, message: &str) {
    const WIDTH: usize = 100;
    let (line, col) = lexer::line_col(src, offset);
    let text = src.lines().nth(line - 1).unwrap_or("");
    let chars: Vec<char> = text.chars().collect();
    let (shown, caret) = if chars.len() <= WIDTH {
        (text.to_string(), col - 1)
    } else {
        let start = (col - 1).saturating_sub(WIDTH / 2).min(chars.len().saturating_sub(WIDTH));
        let end = (start + WIDTH).min(chars.len());
        let mut shown: String = chars[start..end].iter().collect();
        let mut caret = col - 1 - start;
        if start > 0 {
            shown.insert_str(0, "...");
            caret += 3;
        }
        if end < chars.len() {
            shown.push_str("...");
        }
        (shown, caret)
    };
    let gutter = line.to_string().len();
    eprintln!("error: {message}");
    eprintln!("{:gutter$}--> {path}:{line}:{col}", "");
    eprintln!("{:gutter$} |", "");
    eprintln!("{line} | {shown}");
    eprintln!("{:gutter$} | {:caret$}^", "", "");
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

/// The part of the standard library that is written in azoL.
const PRELUDE: &str = include_str!("prelude.azl");

/// One source file of the program being built: its path, and where its
/// text starts and how long it is in the one text that holds them all.
struct SourceFile<'a> {
    path: &'a str,
    start: usize,
    len: usize,
}

/// The file that position `offset` of the whole text falls in, and the
/// position inside that file. A position right after a file's last
/// character (where "end of file" is reported) still belongs to it.
fn locate<'f, 'a>(files: &'f [SourceFile<'a>], offset: u32) -> (&'f SourceFile<'a>, u32) {
    let offset = offset as usize;
    let file = files.iter().find(|file| offset <= file.start + file.len).unwrap_or(&files[files.len() - 1]);
    (file, (offset.max(file.start) - file.start) as u32)
}

/// Prints an error at `offset` of the whole text, naming the file it
/// falls in and the line inside that file.
fn report_in(files: &[SourceFile], src: &str, offset: u32, message: &str) {
    let (file, at) = locate(files, offset);
    report(file.path, &src[file.start..file.start + file.len], at, message);
}

fn run_build(paths: &[&str], out: Option<&str>, target: Target) -> ExitCode {
    let started = Instant::now();
    let path = paths[0];
    let Some(mut src) = read_source(path) else {
        return ExitCode::FAILURE;
    };
    // All source files, and after them the standard library's source,
    // follow each other in one text, so that every position in any of
    // them is a plain offset. Each is lexed and parsed on its own: a
    // mistake at the end of one file is never reported inside the next.
    let mut files = vec![SourceFile { path, start: 0, len: src.len() }];
    for &path in &paths[1..] {
        let Some(more) = read_source(path) else {
            return ExitCode::FAILURE;
        };
        src.push('\n');
        files.push(SourceFile { path, start: src.len(), len: more.len() });
        src.push_str(&more);
    }
    let prelude_start = src.len() + 1;
    src.push('\n');
    src.push_str(PRELUDE);
    if src.len() > u32::MAX as usize {
        eprintln!("error: the program is larger than 4 GB of source text");
        return ExitCode::FAILURE;
    }
    let read = started.elapsed().as_secs_f64();

    let mut ast = None;
    for file in &files {
        let text = &src[file.start..file.start + file.len];
        let mut tokens = match lexer::lex(text) {
            Ok(tokens) => tokens,
            Err(err) => {
                report(file.path, text, err.offset, err.message);
                return ExitCode::FAILURE;
            }
        };
        // Positions are offsets in the whole text. The first file starts
        // at zero, so a program of one file pays nothing for this.
        if file.start > 0 {
            for token in &mut tokens {
                token.start += file.start as u32;
            }
        }
        let parsed = match ast.take() {
            None => parser::parse(&src, &tokens),
            Some(ast) => parser::parse_into(ast, &src, &tokens),
        };
        match parsed {
            Ok(parsed) => ast = Some(parsed),
            Err(err) => {
                report_in(&files, &src, err.offset, &err.message);
                return ExitCode::FAILURE;
            }
        }
    }
    let ast = ast.expect("there is at least one source file");
    let mut prelude_tokens = lexer::lex(&src[prelude_start..]).expect("the standard library has valid tokens");
    for token in &mut prelude_tokens {
        token.start += prelude_start as u32;
    }
    let ast = parser::parse_into(ast, &src, &prelude_tokens).expect("the standard library parses");
    let parsed = started.elapsed().as_secs_f64();

    let program = match sema::check_with_prelude(&ast, &src, prelude_start as u32) {
        Ok(program) => program,
        Err(err) if err.offset as usize >= prelude_start => {
            eprintln!("error: {} (inside the standard library; this is a bug in azoL)", err.message);
            return ExitCode::FAILURE;
        }
        Err(err) => {
            report_in(&files, &src, err.offset, &err.message);
            return ExitCode::FAILURE;
        }
    };
    let checked = started.elapsed().as_secs_f64();

    let source_path = std::path::Path::new(path);
    let out_path = match out {
        Some(out) => std::path::PathBuf::from(out),
        None => source_path.with_extension(""),
    };
    if out_path == source_path {
        eprintln!("error: the output would overwrite the source file; pass `-o <out>`");
        return ExitCode::FAILURE;
    }
    // From here on the two targets differ: machine code, then the file
    // format the system loads.
    let (image, code_bytes, generated) = match target {
        Target::Aarch64Macos => {
            let code = arm64::generate(&ast, &program);
            let generated = started.elapsed().as_secs_f64();
            let ident = out_path.file_name().and_then(|name| name.to_str()).unwrap_or("azol-program");
            (macho::build_executable(&code, &runtime::IMPORTS, ident), code.words.len() * 4, generated)
        }
        Target::X86_64Linux => {
            let code = x64::generate(&ast, &program);
            let generated = started.elapsed().as_secs_f64();
            (elf::build_executable(&code), code.bytes.len(), generated)
        }
    };
    let linked = started.elapsed().as_secs_f64();

    if let Err(err) = write_executable(&out_path, &image) {
        eprintln!("error: cannot write '{}': {err}", out_path.display());
        return ExitCode::FAILURE;
    }
    let total = started.elapsed().as_secs_f64();

    let ms = |from: f64, to: f64| (to - from) * 1000.0;
    eprintln!(
        "built {} ({} bytes of code) in {:.3} ms (read {:.3}, parse {:.3}, check {:.3}, codegen {:.3}, link {:.3}, write {:.3})",
        out_path.display(),
        code_bytes,
        total * 1000.0,
        read * 1000.0,
        ms(read, parsed),
        ms(parsed, checked),
        ms(checked, generated),
        ms(generated, linked),
        ms(linked, total),
    );
    ExitCode::SUCCESS
}

/// Writes a fresh file. An existing executable is removed first: macOS
/// caches code signatures per file and rejects one rewritten in place.
fn write_executable(path: &std::path::Path, image: &[u8]) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    fs::write(path, image)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_found_in_the_right_file() {
        // "ab", "cde" and "" joined by newlines: a b \n c d e \n (empty)
        let files = [
            SourceFile { path: "one", start: 0, len: 2 },
            SourceFile { path: "two", start: 3, len: 3 },
            SourceFile { path: "three", start: 7, len: 0 },
        ];
        let at = |offset: u32| {
            let (file, inside) = locate(&files, offset);
            (file.path, inside)
        };
        assert_eq!(at(0), ("one", 0));
        assert_eq!(at(1), ("one", 1));
        // Right after the last character: the end of that file.
        assert_eq!(at(2), ("one", 2));
        assert_eq!(at(3), ("two", 0));
        assert_eq!(at(5), ("two", 2));
        assert_eq!(at(6), ("two", 3));
        assert_eq!(at(7), ("three", 0));
        // Past everything (never produced for a program's own text): the last file.
        assert_eq!(at(40), ("three", 33));
    }

    #[test]
    fn build_arguments_accept_several_files_in_any_order() {
        let (paths, out, target) = build_arguments(&["a.azl", "-o", "out", "b.azl", "--target", "x86_64-linux", "c.azl"]).unwrap();
        assert_eq!(paths, ["a.azl", "b.azl", "c.azl"]);
        assert_eq!(out, Some("out"));
        assert!(target == Target::X86_64Linux);
        let (paths, out, _) = build_arguments(&["one.azl"]).unwrap();
        assert_eq!((paths, out), (vec!["one.azl"], None));
        assert!(build_arguments(&[]).is_none());
        assert!(build_arguments(&["-o", "out"]).is_none());
        assert!(build_arguments(&["a.azl", "-o"]).is_none());
        assert!(build_arguments(&["a.azl", "--fast"]).is_none());
    }
}
