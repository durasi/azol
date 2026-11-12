// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

mod arm64;
mod ast;
mod diag;
mod dump;
mod lexer;
mod macho;
mod parser;
mod runtime;
mod sema;
mod sha256;
mod token;
mod elf;
mod format;
mod sema_dump;
mod runtime_x64;
mod x64;
mod elf_arm64;
mod runtime_linux_arm64;

use diag::{Diagnostic, File as SourceFile, Problem};
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
    azol build --target <target> ... Compile for `aarch64-macos`, `x86_64-linux` or
                                     `aarch64-linux`
                                     instead of the machine the compiler runs on
    azol check <a.azl> <b.azl> ...   Report every independent error of a program
                                     without writing an executable
    azol check --json ...            Report errors as one JSON object per line on
    azol build --json ...            standard output, with exact positions, the
                                     rule that was broken and, when the repair is
                                     mechanical, the edit that makes it
    azol explain                     List the error codes with their rules
    azol explain <code>              Explain one error code, as in `azol explain E0401`
    azol fmt <files...>              Rewrite source files in the canonical layout
    azol fmt --check <files...>      Change nothing; list the files that are not
                                     formatted and exit with 1 if there are any
    azol fmt -                       Format standard input to standard output
    azol --version                   Print the version";

/// What kind of machine an executable is for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    /// Apple Silicon Macs: ARM64 code in a signed Mach-O file.
    Aarch64Macos,
    /// Linux on Intel and AMD processors: x86-64 code in a static ELF file.
    X86_64Linux,
    /// Linux on ARM64 processors: the ARM64 code in a static ELF file.
    Aarch64Linux,
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
            "aarch64-linux" => Some(Target::Aarch64Linux),
            _ => None,
        }
    }
}

/// Reads the arguments of `azol build`: the source files, and `-o` and
/// `--target` with their values, in any order.
/// What `azol build` was asked to do.
struct BuildArguments<'a> {
    paths: Vec<&'a str>,
    out: Option<&'a str>,
    target: Target,
    /// Where to also write the code generator's result as text, for the
    /// linker written in azoL (`compiler/`). Not part of the usage text.
    emit_code: Option<&'a str>,
    /// Report errors as JSON on standard output.
    json: bool,
}

fn build_arguments<'a>(args: &[&'a str]) -> Option<BuildArguments<'a>> {
    let (mut paths, mut out, mut target, mut emit_code, mut json) = (Vec::new(), None, Target::host(), None, false);
    let mut rest = args.iter();
    while let Some(&arg) = rest.next() {
        match arg {
            "-o" => out = Some(*rest.next()?),
            "--emit-code" => emit_code = Some(*rest.next()?),
            "--json" => json = true,
            "--target" => {
                let name = rest.next()?;
                let Some(known) = Target::parse(name) else {
                    eprintln!("error: unknown target '{name}'; the targets are `aarch64-macos`, `x86_64-linux` and `aarch64-linux`");
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
    Some(BuildArguments { paths, out, target, emit_code, json })
}

/// Bytes as lines of hexadecimal digits, 32 bytes to a line.
fn hex_lines(text: &mut String, bytes: &[u8]) {
    use std::fmt::Write;
    for line in bytes.chunks(32) {
        for byte in line {
            let _ = write!(text, "{byte:02x}");
        }
        text.push('\n');
    }
}

/// The text form of what the ARM64 code generator hands to the Mach-O
/// writer. The format is described in `compiler/code_file.azl`.
fn code_text_macos(code: &arm64::Code, imports: &[&str], ident: &str) -> String {
    use std::fmt::Write;
    let mut text = format!("azol-code 1\ntarget aarch64-macos\nident {ident}\nentry {}\nimports {}\n", code.entry, imports.len());
    for name in imports {
        let _ = writeln!(text, "{name}");
    }
    let bytes: Vec<u8> = code.words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let _ = writeln!(text, "code {}", bytes.len());
    hex_lines(&mut text, &bytes);
    let _ = writeln!(text, "data {}", code.data.len());
    hex_lines(&mut text, &code.data);
    let _ = writeln!(text, "relocs {}", code.relocs.len());
    for reloc in &code.relocs {
        let _ = match *reloc {
            arm64::Reloc::Import { at, import } => writeln!(text, "i {at} {import}"),
            arm64::Reloc::Data { at, offset, reg } => writeln!(text, "d {at} {offset} {reg}"),
        };
    }
    text.push_str("end\n");
    text
}

/// The text form of what the x86-64 code generator hands to the ELF
/// writer, with the facts about the runtime that writer needs.
fn code_text_linux(code: &x64::Code) -> String {
    use std::fmt::Write;
    let mut text = format!(
        "azol-code 1\ntarget x86_64-linux\nentry {}\nruntime {} {} {} {} {}\nentries {}\n",
        code.entry,
        runtime_x64::BASE,
        runtime_x64::TEXT_OFFSET,
        runtime_x64::START,
        runtime_x64::MEMORY,
        runtime_x64::MEMORY_SIZE,
        runtime_x64::ENTRIES.len()
    );
    for offset in runtime_x64::ENTRIES {
        let _ = writeln!(text, "{offset}");
    }
    let _ = writeln!(text, "code {}", code.bytes.len());
    hex_lines(&mut text, &code.bytes);
    let _ = writeln!(text, "data {}", code.data.len());
    hex_lines(&mut text, &code.data);
    let _ = writeln!(text, "calls {}", code.runtime_calls.len());
    for &(field, routine) in &code.runtime_calls {
        let _ = writeln!(text, "{field} {routine}");
    }
    let _ = writeln!(text, "refs {}", code.data_refs.len());
    for &(field, offset) in &code.data_refs {
        let _ = writeln!(text, "{field} {offset}");
    }
    text.push_str("end\n");
    text
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
        ["fmt", rest @ ..] => format::run(rest),
        // For comparing with the checker written in azoL; see `sema_dump.rs`.
        ["sema-dump", rest @ ..] => sema_dump::run(rest),
        [command @ ("build" | "check"), rest @ ..] => match build_arguments(rest) {
            Some(arguments) => run_build(&arguments, *command == "check"),
            None => {
                eprintln!("{USAGE}");
                ExitCode::from(2)
            }
        },
        ["explain"] => {
            print!("{}", diag::list());
            ExitCode::SUCCESS
        }
        // How `docs/errors.md` is made; not part of the usage text.
        ["explain", "--markdown"] => {
            print!("{}", diag::errors_md());
            ExitCode::SUCCESS
        }
        ["explain", code] => match diag::explain(code) {
            Some(text) => {
                print!("{text}");
                ExitCode::SUCCESS
            }
            None => {
                eprintln!("error: there is no error code '{code}'; `azol explain` lists them all");
                ExitCode::FAILURE
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
    eprintln!("error[{}]: {message}", diag::classify(message).code);
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
const PRELUDE: &str = include_str!("../lib/prelude.azl");

/// Reads the files of a program into one text. All source files, and
/// after them the standard library's source, follow each other in it,
/// so that every position in any of them is a plain offset. Returns the
/// text, where each file is in it, and where the standard library starts.
fn load<'a>(paths: &[&'a str], read: &dyn Fn(&str) -> Option<String>) -> Option<(String, Vec<SourceFile<'a>>, usize)> {
    let mut src = read(paths[0])?;
    let mut files = vec![SourceFile { path: paths[0], start: 0, len: src.len() }];
    for &path in &paths[1..] {
        let more = read(path)?;
        src.push('\n');
        files.push(SourceFile { path, start: src.len(), len: more.len() });
        src.push_str(&more);
    }
    let prelude_start = src.len() + 1;
    src.push('\n');
    src.push_str(PRELUDE);
    Some((src, files, prelude_start))
}

/// Lexes and parses the program. Each file is lexed and parsed on its
/// own: a mistake at the end of one file is never reported inside the
/// next, and every file with a mistake reports its first one.
fn parse_program(src: &str, files: &[SourceFile], prelude_start: usize) -> Result<ast::Ast, Vec<Problem>> {
    let mut ast = None;
    let mut problems = Vec::new();
    for file in files {
        let text = &src[file.start..file.start + file.len];
        let mut tokens = match lexer::lex(text) {
            Ok(tokens) => tokens,
            Err(err) => {
                problems.push(Problem { offset: err.offset + file.start as u32, message: err.message.to_string(), expr: None, fix: None });
                continue;
            }
        };
        // Positions are offsets in the whole text. The first file starts
        // at zero, so a program of one file pays nothing for this.
        if file.start > 0 {
            for token in &mut tokens {
                token.start += file.start as u32;
            }
        }
        // After a mistake the tree is of no use any more; the files that
        // follow are still parsed, for the mistakes of their own.
        let parsed = match ast.take() {
            Some(ast) if problems.is_empty() => parser::parse_into(ast, src, &tokens),
            _ => parser::parse(src, &tokens),
        };
        match parsed {
            Ok(parsed) => ast = Some(parsed),
            Err(err) => problems.push(Problem { offset: err.offset, message: err.message, expr: None, fix: None }),
        }
    }
    if !problems.is_empty() {
        return Err(problems);
    }
    let ast = ast.expect("there is at least one source file");
    let mut prelude_tokens = lexer::lex(&src[prelude_start..]).expect("the standard library has valid tokens");
    for token in &mut prelude_tokens {
        token.start += prelude_start as u32;
    }
    Ok(parser::parse_into(ast, src, &prelude_tokens).expect("the standard library parses"))
}

/// What the checker found, in the form every stage reports problems in.
fn sema_problems(errors: Vec<sema::SemaError>) -> Vec<Problem> {
    errors.into_iter().map(|err| Problem { offset: err.offset, message: err.message, expr: err.expr, fix: err.fix }).collect()
}

/// Shows the problems of a program: for a person on standard error, or
/// with `json` as one JSON object per line on standard output.
fn show(diagnostics: &[Diagnostic], files: &[SourceFile], src: &str, json: bool) {
    if json {
        for diagnostic in diagnostics {
            println!("{}", diag::json_line(diagnostic));
        }
        println!("{}", diag::json_summary(diagnostics.len(), files.len()));
        return;
    }
    for (index, diagnostic) in diagnostics.iter().enumerate() {
        if index > 0 {
            eprintln!();
        }
        let file = &files[diagnostic.file_index];
        report(file.path, &src[file.start..file.start + file.len], diagnostic.at, &diagnostic.message);
    }
    if diagnostics.len() > 1 {
        eprintln!("\nfound {} errors", diagnostics.len());
    }
}

fn run_build(arguments: &BuildArguments, check_only: bool) -> ExitCode {
    let (paths, out, target, json) = (&arguments.paths[..], arguments.out, arguments.target, arguments.json);
    let started = Instant::now();
    let path = paths[0];
    let Some((src, files, prelude_start)) = load(paths, &read_source) else {
        return ExitCode::FAILURE;
    };
    if src.len() > u32::MAX as usize {
        eprintln!("error: the program is larger than 4 GB of source text");
        return ExitCode::FAILURE;
    }
    let read = started.elapsed().as_secs_f64();

    let ast = match parse_program(&src, &files, prelude_start) {
        Ok(ast) => ast,
        Err(problems) => {
            show(&diag::diagnose(&problems, &files, &src, None), &files, &src, json);
            return ExitCode::FAILURE;
        }
    };
    let parsed = started.elapsed().as_secs_f64();

    // Where each file starts, for `embed`, which reads next to its source.
    let file_starts: Vec<(u32, &str)> = files.iter().map(|file| (file.start as u32, file.path)).collect();
    let program = match sema::check_with_files(&ast, &src, prelude_start as u32, &file_starts) {
        Ok(program) => program,
        Err(errors) => {
            if let Some(err) = errors.iter().find(|err| err.offset as usize >= prelude_start) {
                eprintln!("error: {} (inside the standard library; this is a bug in azoL)", err.message);
                return ExitCode::FAILURE;
            }
            show(&diag::diagnose(&sema_problems(errors), &files, &src, Some(&ast)), &files, &src, json);
            return ExitCode::FAILURE;
        }
    };
    let checked = started.elapsed().as_secs_f64();
    if check_only {
        if json {
            println!("{}", diag::json_summary(0, files.len()));
        } else {
            let lines = files.iter().map(|file| src[file.start..file.start + file.len].lines().count()).sum::<usize>();
            eprintln!("checked {} lines in {} file{}: no errors ({:.3} ms)", lines, files.len(), if files.len() == 1 { "" } else { "s" }, checked * 1000.0);
        }
        return ExitCode::SUCCESS;
    }

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
            let mut code = arm64::generate(&ast, &program);
            macho::start_through_runtime(&mut code);
            let generated = started.elapsed().as_secs_f64();
            let ident = out_path.file_name().and_then(|name| name.to_str()).unwrap_or("azol-program");
            if let Some(path) = arguments.emit_code {
                if let Err(err) = fs::write(path, code_text_macos(&code, &runtime::IMPORTS, ident)) {
                    eprintln!("error: cannot write '{path}': {err}");
                    return ExitCode::FAILURE;
                }
            }
            (macho::build_executable(&code, &runtime::IMPORTS, ident), code.words.len() * 4, generated)
        }
        Target::X86_64Linux => {
            let code = x64::generate(&ast, &program);
            let generated = started.elapsed().as_secs_f64();
            if let Some(path) = arguments.emit_code {
                if let Err(err) = fs::write(path, code_text_linux(&code)) {
                    eprintln!("error: cannot write '{path}': {err}");
                    return ExitCode::FAILURE;
                }
            }
            (elf::build_executable(&code), code.bytes.len(), generated)
        }
        Target::Aarch64Linux => {
            if arguments.emit_code.is_some() {
                eprintln!("error: `--emit-code` is not available for the target `aarch64-linux`");
                return ExitCode::FAILURE;
            }
            let code = arm64::generate(&ast, &program);
            let generated = started.elapsed().as_secs_f64();
            (elf_arm64::build_executable(&code), code.words.len() * 4, generated)
        }
    };
    let linked = started.elapsed().as_secs_f64();

    if let Err(err) = write_executable(&out_path, &image) {
        eprintln!("error: cannot write '{}': {err}", out_path.display());
        return ExitCode::FAILURE;
    }
    let total = started.elapsed().as_secs_f64();
    if json {
        println!("{}", diag::json_summary(0, files.len()));
    }

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

    /// Checks a program given as (path, text) pairs, as `azol check` does.
    fn check(sources: &[(&str, &str)]) -> Vec<Diagnostic> {
        let paths: Vec<&str> = sources.iter().map(|(path, _)| *path).collect();
        let read = |path: &str| sources.iter().find(|(name, _)| *name == path).map(|(_, text)| text.to_string());
        let (src, files, prelude_start) = load(&paths, &read).unwrap();
        let ast = match parse_program(&src, &files, prelude_start) {
            Ok(ast) => ast,
            Err(problems) => return diag::diagnose(&problems, &files, &src, None),
        };
        match sema::check_with_prelude(&ast, &src, prelude_start as u32) {
            Ok(_) => Vec::new(),
            Err(errors) => diag::diagnose(&sema_problems(errors), &files, &src, Some(&ast)),
        }
    }

    /// (code, line, column) of every problem of a one-file program.
    fn found(text: &str) -> Vec<(&'static str, usize, usize)> {
        check(&[("a.azl", text)]).iter().map(|d| (d.code, d.line, d.column)).collect()
    }

    #[test]
    fn every_function_reports_its_first_error() {
        let text = "fn one() -> i64 {\n    return true\n}\n\nfn two() {\n    let a = 1\n    a = 2\n    missing()\n}\n\nfn three() {\n    print(nothing)\n}\n\nfn main() {\n}\n";
        // `two` has a second mistake, which may be a result of the first and is left for the next run.
        assert_eq!(found(text), [("E0304", 2, 12), ("E0401", 7, 5), ("E0201", 12, 11)]);
        assert!(found("fn main() {\n    print(1)\n}\n").is_empty());
    }

    #[test]
    fn a_missing_main_is_reported_with_the_other_errors() {
        assert_eq!(found("fn f() {\n    g()\n}\n"), [("E0315", 1, 1), ("E0202", 2, 5)]);
    }

    #[test]
    fn an_error_in_a_declaration_is_reported_alone() {
        // What functions mean depends on the declarations, so their bodies are not checked yet.
        assert_eq!(found("struct P {\n    x: Nope\n}\n\nfn main() {\n    print(nothing)\n}\n"), [("E0203", 2, 8)]);
    }

    #[test]
    fn a_generic_function_reports_a_mistake_once() {
        let text = "fn show[T](x: T) {\n    print(nothing)\n}\n\nfn main() {\n    show(1)\n    show(true)\n}\n";
        assert_eq!(found(text), [("E0201", 2, 11)]);
    }

    #[test]
    fn every_file_reports_its_first_syntax_error() {
        let found = check(&[("a.azl", "fn main() {\n    let x = \n}\n"), ("b.azl", "fn ok() {\n}\n"), ("c.azl", "fn f() {\n    let s = \"open\n}\n"), ("d.azl", "fn g( {\n}\nfn h( {\n}\n")]);
        let seen: Vec<(&str, &str, usize, usize)> = found.iter().map(|d| (d.file.as_str(), d.code, d.line, d.column)).collect();
        assert_eq!(seen, [("a.azl", "E0106", 3, 1), ("c.azl", "E0102", 2, 13), ("d.azl", "E0106", 1, 7)]);
        // Mistakes in the meaning of a program wait until it can be read at all.
        assert_eq!(check(&[("a.azl", "fn main() {\n    print(nothing)\n}\n"), ("b.azl", "fn f( {\n}\n")]).len(), 1);
    }

    #[test]
    fn errors_are_sorted_by_file_and_position() {
        let found = check(&[("a.azl", "fn main() {\n    b()\n    print(x)\n}\n\nfn last() {\n    print(y)\n}\n"), ("b.azl", "fn b() {\n    print(z)\n}\n")]);
        let seen: Vec<(&str, usize)> = found.iter().map(|d| (d.file.as_str(), d.line)).collect();
        assert_eq!(seen, [("a.azl", 3), ("a.azl", 7), ("b.azl", 2)]);
    }

    #[test]
    fn a_report_covers_the_whole_expression_or_name() {
        let span = |text: &str| {
            let found = check(&[("a.azl", text)]);
            assert_eq!(found.len(), 1, "{text}");
            (found[0].line, found[0].column, found[0].end_line, found[0].end_column)
        };
        // A name.
        assert_eq!(span("fn main() {\n    print(çay)\n}\n"), (2, 11, 2, 14));
        // A whole operand, brackets and call included.
        assert_eq!(span("fn f(a: i64) -> bool {\n    return true\n}\n\nfn main() {\n    print(1 + f(2 * (3 + 4)))\n}\n"), (6, 15, 6, 29));
        // Parentheses around the whole of it are not part of the tree.
        assert_eq!(span("fn main() {\n    let a: i64 = (1 < 2)\n}\n"), (2, 19, 2, 24));
        assert_eq!(span("fn main() {\n    let a: i64 = [1, 2].len() == 2\n}\n"), (2, 18, 2, 35));
        // An expression that goes on on the next line.
        assert_eq!(span("fn main() {\n    let a: bool = 1 +\n        2\n}\n"), (2, 19, 3, 10));
        // Where nothing better is known: one character.
        assert_eq!(span("fn main() {\n    let a = @\n}\n"), (2, 13, 2, 14));
        assert_eq!(span("fn main() {\n"), (2, 1, 2, 1));
    }

    /// Checks that `text` has exactly one problem with code `code` and
    /// a fix, and that applying the fix leaves a correct program.
    /// Returns the fixed text.
    fn fixed(code: &str, text: &str) -> String {
        let found = check(&[("a.azl", text)]);
        assert_eq!(found.len(), 1, "{text}");
        assert_eq!(found[0].code, code, "{}", found[0].message);
        let fix = found[0].fix.as_ref().unwrap_or_else(|| panic!("no fix for: {}", found[0].message));
        let after = diag::apply(text, fix);
        let left = check(&[("a.azl", &after)]);
        assert!(left.is_empty(), "the fix left an error: {}\n{after}", left[0].message);
        after
    }

    #[test]
    fn fixes_resolve_their_errors() {
        const GROW: &str = "fn grow(items: mut List[i64]) {\n    items.push(1)\n}\n\nfn size(items: List[i64]) -> i64 {\n    return items.len()\n}\n\n";
        // 1. `let` must be `var`: assigned, grown, passed as `mut`.
        let after = fixed("E0401", "fn main() {\n    let total = 0\n    total = total + 1\n}\n");
        assert!(after.contains("    var total = 0\n"));
        fixed("E0401", "fn main() {\n    let çay: List[i64] = [1]\n    çay.push(2)\n}\n");
        fixed("E0401", &format!("{GROW}fn main() {{\n    let xs = [1]\n    grow(mut xs)\n}}\n"));
        // 2. A `mut` parameter needs `mut` at the call.
        let after = fixed("E0405", &format!("{GROW}fn main() {{\n    var xs = [1]\n    grow(xs)\n}}\n"));
        assert!(after.contains("    grow(mut xs)\n"));
        // 3. `mut` on an argument whose parameter is not `mut`.
        let after = fixed("E0406", &format!("{GROW}fn main() {{\n    var xs = [1]\n    print(size(mut xs))\n}}\n"));
        assert!(after.contains("    print(size(xs))\n"));
        // 4. `mut` on something that is not an argument.
        let after = fixed("E0406", "fn main() {\n    var a = 1\n    let b = Some(mut a)\n    match b {\n        Some(v) => print(v)\n        None => {}\n    }\n}\n");
        assert!(after.contains("    let b = Some(a)\n"));
        fixed("E0406", "fn main() {\n    var a = 1\n    print(mut a)\n}\n");
        // 5. Arms missing for variants of an enum.
        let after = fixed("E0501", "enum Shape {\n    Circle(i64)\n    Square(i64, i64)\n    Empty\n}\n\nfn main() {\n    let s = Empty\n    match s {\n        Circle(r) => print(r)\n    }\n}\n");
        assert!(after.contains("        Circle(r) => print(r)\n        Square(_, _) => {}\n        Empty => {}\n    }\n"), "{after}");
        // 6. An arm missing for a `bool`.
        let after = fixed("E0501", "fn main() {\n    match 1 < 2 {\n        true => print(1)\n    }\n}\n");
        assert!(after.contains("        false => {}\n    }\n"));
        // 7. The final `_` arm for a number or a string.
        let after = fixed("E0502", "fn main() {\n    match args().len() {\n        0 => print(0)\n    }\n}\n");
        assert!(after.contains("        _ => {}\n    }\n"));
        fixed("E0502", "fn main() {\n\tmatch \"a\" {\n\t\t\"a\" => print(1)\n\t}\n}\n");
        // 8. A variant pattern without its values.
        let after = fixed("E0506", "fn main() {\n    let o: Option[i64] = None\n    match o {\n        Some => print(1)\n        None => print(0)\n    }\n}\n");
        assert!(after.contains("        Some(_) => print(1)\n"));
        // 9. `()` after a variant that holds nothing.
        let after = fixed("E0312", "fn main() {\n    let o: Option[i64] = None()\n    match o {\n        Some(v) => print(v)\n        None => print(0)\n    }\n}\n");
        assert!(after.contains("    let o: Option[i64] = None\n"));
    }

    #[test]
    fn no_fix_is_offered_when_it_would_not_be_the_whole_repair() {
        let fix_of = |text: &str| {
            let found = check(&[("a.azl", text)]);
            assert_eq!(found.len(), 1, "{text}");
            found[0].fix.clone()
        };
        // A parameter: whether to copy it or to declare it `mut` is the author's choice.
        assert_eq!(fix_of("fn f(a: i64) {\n    a = 1\n}\n\nfn main() {\n}\n"), None);
        // A loop variable.
        assert_eq!(fix_of("fn main() {\n    for x in [1] {\n        x = 2\n    }\n}\n"), None);
        // An argument that is not a changeable variable: `mut` alone would not do.
        assert_eq!(fix_of("fn grow(items: mut List[i64]) {\n}\n\nfn main() {\n    let xs = [1]\n    grow(xs)\n}\n"), None);
        assert_eq!(fix_of("fn grow(items: mut List[i64]) {\n}\n\nfn main() {\n    grow([1])\n}\n"), None);
        // A `match` written on one line has no line to add an arm on.
        assert_eq!(fix_of("fn main() {\n    match true { true => print(1) }\n}\n"), None);
    }

    #[test]
    fn json_output_has_every_field() {
        let found = check(&[("dir/\"a\".azl", "fn main() {\n\tlet s = \"a\\tb\"\n\ts = \"c\"\n}\n")]);
        assert_eq!(found.len(), 1);
        assert_eq!(
            diag::json_line(&found[0]),
            concat!(
                r#"{"severity":"error","code":"E0401","message":"cannot change `s`: declare it with `var` to make it changeable","#,
                r#""file":"dir/\"a\".azl","line":3,"column":2,"end_line":3,"end_column":3,"source":"\ts = \"c\"","#,
                r#""rule":"A variable declared with `let` never changes; only one declared with `var` can be assigned, grown or passed as `mut`.","#,
                r#""help":"Declare the variable with `var` instead of `let`.","#,
                r#""fix":{"file":"dir/\"a\".azl","line":2,"column":2,"end_line":2,"end_column":5,"replacement":"var"}}"#
            )
        );
        // Without help and fix when there is nothing certain to say.
        let found = check(&[("a.azl", "fn main() {\n    print(x)\n}\n")]);
        assert_eq!(
            diag::json_line(&found[0]),
            concat!(
                r#"{"severity":"error","code":"E0201","message":"unknown variable `x`","file":"a.azl","line":2,"column":11,"end_line":2,"end_column":12,"#,
                r#""source":"    print(x)","rule":"A name must be declared before it is used: as a variable, a parameter, a constant or a variant."}"#
            )
        );
    }

    #[test]
    fn a_fix_in_a_later_file_is_placed_inside_that_file() {
        let found = check(&[("a.azl", "fn main() {\n    count()\n}\n"), ("b.azl", "fn count() {\n    let n = 0\n    n = n + 1\n}\n")]);
        assert_eq!(found.len(), 1);
        let fix = found[0].fix.as_ref().unwrap();
        assert_eq!((found[0].file.as_str(), found[0].line, fix.line, fix.column, fix.end_column), ("b.azl", 3, 2, 5, 8));
    }

    #[test]
    fn build_arguments_accept_several_files_in_any_order() {
        let parsed = build_arguments(&["a.azl", "-o", "out", "b.azl", "--target", "x86_64-linux", "c.azl"]).unwrap();
        assert_eq!(parsed.paths, ["a.azl", "b.azl", "c.azl"]);
        assert_eq!(parsed.out, Some("out"));
        assert!(parsed.target == Target::X86_64Linux);
        assert_eq!(parsed.emit_code, None);
        assert!(!parsed.json);
        assert!(build_arguments(&["--json", "a.azl"]).unwrap().json);
        let parsed = build_arguments(&["one.azl", "--emit-code", "one.code"]).unwrap();
        assert_eq!((parsed.paths, parsed.out, parsed.emit_code), (vec!["one.azl"], None, Some("one.code")));
        assert!(build_arguments(&["a.azl", "--emit-code"]).is_none());
        assert!(build_arguments(&[]).is_none());
        assert!(build_arguments(&["-o", "out"]).is_none());
        assert!(build_arguments(&["a.azl", "-o"]).is_none());
        assert!(build_arguments(&["a.azl", "--fast"]).is_none());
    }
}
