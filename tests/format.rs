// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Tests of `azol fmt` with the real `azol` binary: the input and
//! expected pairs in `tests/format`, the command's three forms, and the
//! promise that formatting never changes what a program compiles to.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const AZOL: &str = env!("CARGO_BIN_EXE_azol");

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// A fresh, empty directory for one test.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("azol-fmt-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("temporary directory");
    dir
}

fn azol(args: &[&str]) -> Output {
    Command::new(AZOL).args(args).output().expect("azol runs")
}

/// Runs `azol fmt -` on `src`.
fn fmt_stdin(src: &str) -> Output {
    let mut child = Command::new(AZOL)
        .args(["fmt", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("azol runs");
    child.stdin.take().expect("piped").write_all(src.as_bytes()).expect("written");
    child.wait_with_output().expect("azol finishes")
}

fn formatted(src: &str) -> String {
    let output = fmt_stdin(src);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).expect("UTF-8")
}

/// The `.azl` files of a directory, and of the directories below it when
/// `deep` is set, sorted.
fn sources(dir: &Path, deep: bool) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).expect("readable directory") {
        let path = entry.expect("readable entry").path();
        if path.is_dir() {
            if deep {
                found.extend(sources(&path, true));
            }
        } else if path.extension().is_some_and(|ext| ext == "azl") {
            found.push(path);
        }
    }
    found.sort();
    found
}

fn text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

fn utf8(path: &Path) -> &str {
    path.to_str().expect("UTF-8 path")
}

#[test]
fn fixtures_format_to_their_expected_text() {
    let dir = root().join("tests/format");
    let mut pairs = 0;
    for path in sources(&dir, false) {
        let name = path.file_name().and_then(|n| n.to_str()).expect("file name");
        if let Some(stem) = name.strip_suffix(".input.azl") {
            let expected = text(&dir.join(format!("{stem}.expected.azl")));
            assert_eq!(formatted(&text(&path)), expected, "{name}");
            assert_eq!(formatted(&expected), expected, "{stem}.expected.azl is not a fixed point");
            pairs += 1;
        } else {
            let stem = name.strip_suffix(".expected.azl").unwrap_or_else(|| panic!("unexpected file {name}"));
            assert!(dir.join(format!("{stem}.input.azl")).exists(), "{name} has no input");
        }
    }
    assert!(pairs >= 9, "only {pairs} fixture pairs");
}

#[test]
fn fmt_rewrites_files_in_place() {
    let dir = scratch("in-place");
    let (ugly, fine) = (dir.join("ugly.azl"), dir.join("fine.azl"));
    fs::write(&ugly, "fn main(){print( 1 )}").unwrap();
    fs::write(&fine, "fn main() {\n    print(1)\n}\n").unwrap();
    let output = azol(&["fmt", utf8(&ugly), utf8(&fine)]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    assert_eq!(text(&ugly), "fn main() {\n    print(1)\n}\n");
    assert_eq!(text(&fine), "fn main() {\n    print(1)\n}\n");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn check_lists_unformatted_files_and_changes_nothing() {
    let dir = scratch("check");
    let (ugly, fine) = (dir.join("ugly.azl"), dir.join("fine.azl"));
    fs::write(&ugly, "fn main(){print( 1 )}").unwrap();
    fs::write(&fine, "fn main() {\n    print(1)\n}\n").unwrap();
    let output = azol(&["fmt", "--check", utf8(&fine), utf8(&ugly)]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&output.stdout), format!("{}\n", utf8(&ugly)));
    assert_eq!(text(&ugly), "fn main(){print( 1 )}");
    let output = azol(&["fmt", "--check", utf8(&fine)]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn stdin_is_formatted_to_stdout() {
    assert_eq!(formatted("fn main(){print( 1 )}"), "fn main() {\n    print(1)\n}\n");
}

#[test]
fn fmt_without_files_prints_the_usage() {
    for args in [&["fmt"][..], &["fmt", "--check"], &["fmt", "--unknown", "a.azl"]] {
        let output = azol(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("azol fmt --check <files...>"));
    }
}

/// The first line of an error report, `error[E....]: ...`.
fn error_line(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).lines().next().unwrap_or("").to_string()
}

#[test]
fn a_file_that_does_not_parse_is_left_untouched() {
    let dir = scratch("refuse");
    let (broken, ugly) = (dir.join("broken.azl"), dir.join("ugly.azl"));
    let source = "fn main(){\n  let = 1\n     }";
    fs::write(&broken, source).unwrap();
    fs::write(&ugly, "fn main(){print( 1 )}").unwrap();
    let parser = azol(&["parse", utf8(&broken)]);
    assert!(error_line(&parser).contains("expected a variable name"), "{}", error_line(&parser));

    let output = azol(&["fmt", utf8(&broken), utf8(&ugly)]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(error_line(&output), error_line(&parser));
    assert!(String::from_utf8_lossy(&output.stderr).contains("broken.azl:2:"));
    assert_eq!(text(&broken), source);
    // The other file of the same run is still formatted.
    assert_eq!(text(&ugly), "fn main() {\n    print(1)\n}\n");

    let output = azol(&["fmt", "--check", utf8(&broken)]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(error_line(&output), error_line(&parser));
    assert!(output.stdout.is_empty());

    let output = fmt_stdin(source);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(error_line(&output), error_line(&parser));
    assert!(output.stdout.is_empty());
    fs::remove_dir_all(&dir).unwrap();
}

/// The repository's own files that `azol fmt` would change, each for a
/// reason that can be defended:
///
/// * `struct_stress.azl` declares a struct over several lines with six
///   fields on a line; members of a declaration that spans lines go one
///   on a line.
/// * `strings.azl` has a space at the end of a line of code.
/// * `matching.azl`, `moves.azl`, `strings.azl` and `text_methods.azl`
///   end an `// expect-line: ` comment in a space, for an empty line of
///   output; the space goes and the test runner reads the same
///   expectation without it.
/// * `text_methods.azl` has a space before a closing parenthesis.
const MAY_CHANGE: [&str; 5] = [
    "tests/programs/matching.azl",
    "tests/programs/moves.azl",
    "tests/programs/strings.azl",
    "tests/programs/struct_stress.azl",
    "tests/programs/text_methods.azl",
];

#[test]
fn repository_files_are_already_formatted() {
    let mut paths = Vec::new();
    for dir in ["tests/programs", "tests/multi", "examples", "compiler", "src"] {
        paths.extend(sources(&root().join(dir), true));
    }
    assert!(paths.len() > 50, "only {} source files found", paths.len());
    let mut args = vec!["fmt", "--check"];
    args.extend(paths.iter().map(|path| utf8(path)));
    let output = azol(&args);
    let listed = String::from_utf8(output.stdout).expect("UTF-8");
    let listed: BTreeSet<&str> = listed.lines().map(|line| line.strip_prefix(utf8(root())).unwrap_or(line).trim_start_matches('/')).collect();
    let allowed: BTreeSet<&str> = MAY_CHANGE.into_iter().collect();
    let unexpected: Vec<&&str> = listed.difference(&allowed).collect();
    assert!(unexpected.is_empty(), "azol fmt would change {unexpected:?}");
    // The one file that is refused is a test of a broken program.
    let errors = String::from_utf8_lossy(&output.stderr).lines().filter(|line| line.starts_with("error")).count();
    assert_eq!(errors, 1, "{}", String::from_utf8_lossy(&output.stderr));
    assert!(String::from_utf8_lossy(&output.stderr).contains("tests/multi/broken/unclosed.azl"));
}

/// The same program laid out badly: tabs and odd indentation, commas
/// with the space on the wrong side, braces without a space before them
/// and blank lines after them. Lines with a comment, a string or a
/// character literal keep their text, only their indentation changes.
fn uglier(source: &str) -> String {
    let mut out = String::new();
    for (number, line) in source.lines().enumerate() {
        let code = line.trim_start();
        if code.is_empty() {
            out.push_str("\n\n");
            continue;
        }
        out.push_str(["\t", "", "       ", "  \t "][number % 4]);
        if code.contains("//") || code.contains('"') || code.contains('\'') {
            out.push_str(code);
        } else {
            out.push_str(&code.replace(", ", " ,").replace(" {", "{").replace(" = ", "=  "));
            if code.ends_with('{') {
                out.push_str("\n\n");
            }
        }
        out.push('\n');
    }
    out
}

#[test]
fn formatting_does_not_change_the_executable() {
    let dir = scratch("same-exe");
    let (original_dir, formatted_dir) = (dir.join("original"), dir.join("formatted"));
    fs::create_dir_all(&original_dir).unwrap();
    fs::create_dir_all(&formatted_dir).unwrap();
    // The executable's name is part of a Mach-O signature, so both
    // builds write to the same path.
    let exe = dir.join("program");
    let build = |source: &Path, target: &str| -> Vec<u8> {
        let output = azol(&["build", "--target", target, utf8(source), "-o", utf8(&exe)]);
        assert!(output.status.success(), "{}: {}", source.display(), String::from_utf8_lossy(&output.stderr));
        let bytes = fs::read(&exe).expect("executable written");
        fs::remove_file(&exe).unwrap();
        bytes
    };
    let mut changed = 0;
    let programs = sources(&root().join("tests/programs"), false);
    assert!(programs.len() >= 30);
    for program in &programs {
        let name = program.file_name().expect("file name");
        let (original, pretty) = (original_dir.join(name), formatted_dir.join(name));
        let source = text(program);
        // An uglier spelling of the same program takes part as well, so
        // that the comparison is not only between equal texts.
        let ugly = uglier(&source);
        for (variant, input) in [("as written", &source), ("made ugly", &ugly)] {
            fs::write(&original, input).unwrap();
            let neat = formatted(input);
            changed += usize::from(&neat != input);
            if variant == "made ugly" {
                assert!(neat == formatted(&source), "{}: the ugly spelling formats differently", program.display());
            }
            fs::write(&pretty, &neat).unwrap();
            for target in ["aarch64-macos", "x86_64-linux"] {
                assert!(build(&original, target) == build(&pretty, target), "{} ({variant}) for {target}: the executable changed", program.display());
            }
        }
    }
    assert!(changed >= programs.len(), "the formatter changed only {changed} inputs");
    fs::remove_dir_all(&dir).unwrap();
}
