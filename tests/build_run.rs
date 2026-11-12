// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Compiles every program in `tests/programs` with the real `azol`
//! binary. On an Apple Silicon Mac and on x86-64 Linux the result is also
//! executed and its output and exit code are compared with the `expect-`
//! comments at the top of each file. Elsewhere only the compilation is
//! checked.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Expectation {
    exit: i32,
    output: String,
    /// Text the program must write to standard error, if any.
    stderr: Option<String>,
}

/// Reads the `// expect-...:` comments of a test program. The output is
/// given either as `expect-line` comments, one per printed line, or as
/// one `expect-output` comment listing the printed values.
fn expectation(source: &str, path: &Path) -> Expectation {
    let values = |key: &str| -> Vec<&str> {
        let prefix = format!("// {key}:");
        source
            .lines()
            .filter_map(|line| line.strip_prefix(prefix.as_str()))
            .map(|rest| rest.strip_prefix(' ').unwrap_or(rest))
            .collect()
    };
    let exit = values("expect-exit");
    let lines = values("expect-line");
    let output = if lines.is_empty() {
        let listed = values("expect-output");
        assert_eq!(listed.len(), 1, "{} needs `expect-line` or one `expect-output` comment", path.display());
        listed[0].split_whitespace().map(|part| format!("{part}\n")).collect()
    } else {
        lines.iter().map(|line| format!("{line}\n")).collect()
    };
    Expectation {
        exit: exit.first().unwrap_or_else(|| panic!("{} has no `// expect-exit:` comment", path.display())).trim().parse().expect("expect-exit must be a number"),
        output,
        stderr: values("expect-stderr").first().map(|line| format!("{line}\n")),
    }
}

/// The compiler builds for the machine it runs on when it can; on those
/// machines the programs can be run as well.
const NATIVE: bool =
    cfg!(all(target_os = "macos", target_arch = "aarch64")) || cfg!(all(target_os = "linux", target_arch = "x86_64"));

fn programs() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("tests/programs exists")
        .map(|entry| entry.expect("readable entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "azl"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no test programs found");
    paths
}

#[test]
fn programs_compile_and_run() {
    let out_dir = std::env::temp_dir().join(format!("azol-test-{}", std::process::id()));
    fs::create_dir_all(&out_dir).expect("temp dir can be created");

    for path in programs() {
        let source = fs::read_to_string(&path).expect("program is readable");
        let expected = expectation(&source, &path);
        let exe = out_dir.join(path.file_stem().expect("file has a name"));

        let build = Command::new(env!("CARGO_BIN_EXE_azol"))
            .arg("build")
            .arg(&path)
            .arg("-o")
            .arg(&exe)
            .output()
            .expect("azol can be started");
        assert!(
            build.status.success(),
            "{} failed to compile:\n{}",
            path.display(),
            String::from_utf8_lossy(&build.stderr)
        );

        if NATIVE {
            let run = Command::new(&exe).output().expect("compiled program can be started");
            assert_eq!(
                String::from_utf8_lossy(&run.stdout),
                expected.output,
                "{}: wrong output",
                path.display()
            );
            assert_eq!(run.status.code(), Some(expected.exit), "{}: wrong exit code", path.display());
            if let Some(stderr) = &expected.stderr {
                assert_eq!(&String::from_utf8_lossy(&run.stderr), stderr, "{}: wrong error output", path.display());
            }
        }
    }
    let _ = fs::remove_dir_all(&out_dir);
}

/// Runs `azol build` on files of `tests/multi`, in the order given.
fn build_multi(files: &[&str], exe: &Path) -> std::process::Output {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/multi");
    let mut command = Command::new(env!("CARGO_BIN_EXE_azol"));
    command.arg("build");
    for file in files {
        command.arg(dir.join(file));
    }
    command.arg("-o").arg(exe).output().expect("azol can be started")
}

#[test]
fn several_files_make_one_program() {
    let out_dir = std::env::temp_dir().join(format!("azol-multi-{}", std::process::id()));
    fs::create_dir_all(&out_dir).expect("temp dir can be created");
    let main = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/multi/shop/main.azl");
    let expected = expectation(&fs::read_to_string(&main).expect("program is readable"), &main);

    // The files refer to each other in both directions, so every order
    // of them must give the same program.
    let orders = [
        ["shop/main.azl", "shop/stock.azl", "shop/numbers.azl"],
        ["shop/numbers.azl", "shop/stock.azl", "shop/main.azl"],
        ["shop/stock.azl", "shop/main.azl", "shop/numbers.azl"],
    ];
    for (i, order) in orders.iter().enumerate() {
        let exe = out_dir.join(format!("shop{i}"));
        let build = build_multi(order, &exe);
        assert!(build.status.success(), "{order:?} failed to compile:\n{}", String::from_utf8_lossy(&build.stderr));
        if NATIVE {
            let run = Command::new(&exe).output().expect("compiled program can be started");
            assert_eq!(String::from_utf8_lossy(&run.stdout), expected.output, "{order:?}: wrong output");
            assert_eq!(run.status.code(), Some(expected.exit), "{order:?}: wrong exit code");
        }
    }

    // Without `-o` the executable is named after the first file.
    let copy = out_dir.join("first.azl");
    fs::copy(&main, &copy).expect("file can be copied");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/multi/shop");
    let build = Command::new(env!("CARGO_BIN_EXE_azol"))
        .arg("build")
        .arg(&copy)
        .arg(dir.join("stock.azl"))
        .arg(dir.join("numbers.azl"))
        .output()
        .expect("azol can be started");
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stderr));
    assert!(out_dir.join("first").is_file(), "the executable is named after the first file");
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn errors_name_the_file_they_are_in() {
    let out_dir = std::env::temp_dir().join(format!("azol-multi-errors-{}", std::process::id()));
    fs::create_dir_all(&out_dir).expect("temp dir can be created");
    let exe = out_dir.join("never");
    let failure = |files: &[&str]| -> String {
        let build = build_multi(files, &exe);
        assert!(!build.status.success(), "{files:?} should not compile");
        assert!(!exe.exists(), "{files:?}: nothing must be written when the build fails");
        String::from_utf8_lossy(&build.stderr).into_owned()
    };

    // A type error on line 5 of the second file.
    let message = failure(&["broken/main.azl", "broken/helper.azl"]);
    assert!(message.contains("error[E0302]: `+` needs `i64` but this is `str`"), "{message}");
    assert!(message.contains("broken/helper.azl:5:16"), "{message}");
    assert!(message.contains("5 |     return n + text"), "{message}");
    assert!(!message.contains("main.azl"), "{message}");
    // The same mistake when its file comes first.
    let message = failure(&["broken/helper.azl", "broken/main.azl"]);
    assert!(message.contains("broken/helper.azl:5:16"), "{message}");

    // A block left open at the end of the middle file is reported there,
    // not at the start of the file that follows.
    let message = failure(&["broken/unclosed_main.azl", "broken/unclosed.azl", "broken/after.azl"]);
    assert!(message.contains("expected `}` to close the block but reached end of file"), "{message}");
    assert!(message.contains("broken/unclosed.azl:3:1"), "{message}");
    assert!(!message.contains("after.azl"), "{message}");

    // A name defined in two files is reported at the second definition.
    let message = failure(&["shop/main.azl", "shop/stock.azl", "shop/numbers.azl", "broken/after.azl", "broken/unclosed_main.azl"]);
    assert!(message.contains("function `main` is defined more than once"), "{message}");
    assert!(message.contains("broken/unclosed_main.azl:1:4"), "{message}");

    // A program without `main`, and a file that does not exist.
    let message = failure(&["shop/numbers.azl", "broken/after.azl"]);
    assert!(message.contains("the program has no `main` function"), "{message}");
    assert!(message.contains("shop/numbers.azl:1:1"), "{message}");
    let message = failure(&["shop/numbers.azl", "broken/missing.azl"]);
    assert!(message.contains("cannot read"), "{message}");
    assert!(message.contains("broken/missing.azl"), "{message}");
    let _ = fs::remove_dir_all(&out_dir);
}

/// Runs `azol` with the given arguments; returns its exit code and what
/// it wrote to standard output and to standard error.
fn azol(args: &[&str]) -> (i32, String, String) {
    let run = Command::new(env!("CARGO_BIN_EXE_azol")).args(args).output().expect("azol can be started");
    (run.status.code().unwrap_or(-1), String::from_utf8_lossy(&run.stdout).into_owned(), String::from_utf8_lossy(&run.stderr).into_owned())
}

#[test]
fn check_reports_every_error_and_writes_nothing() {
    let dir = std::env::temp_dir().join(format!("azol-check-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir can be created");
    let write = |name: &str, text: &str| -> String {
        let path = dir.join(name);
        fs::write(&path, text).expect("a test program can be written");
        path.to_str().expect("the temp dir has a plain name").to_string()
    };
    let good = write("good.azl", "fn main() {\n    print(twice(2))\n}\n");
    let lib = write("lib.azl", "fn twice(n: i64) -> i64 {\n    return n * 2\n}\n");
    let bad = write("bad.azl", "fn one() {\n    let a = 1\n    a = 2\n}\n\nfn two() {\n    print(missing)\n}\n\nfn main() {\n}\n");

    // A correct program of two files: nothing on standard output, no executable.
    let (code, stdout, stderr) = azol(&["check", &good, &lib]);
    assert_eq!((code, stdout.as_str()), (0, ""), "{stderr}");
    assert!(stderr.contains("no errors"), "{stderr}");
    assert!(!dir.join("good").exists());
    let (code, stdout, _) = azol(&["check", "--json", &good, &lib]);
    assert_eq!((code, stdout.as_str()), (0, "{\"summary\":{\"errors\":0,\"files\":2}}\n"));

    // Two independent errors: both are shown, each in the usual form, and counted.
    let (code, stdout, stderr) = azol(&["check", &bad]);
    assert_eq!((code, stdout.as_str()), (1, ""));
    assert!(stderr.contains("error[E0401]: cannot change `a`: declare it with `var` to make it changeable"), "{stderr}");
    assert!(stderr.contains("error[E0201]: unknown variable `missing`"), "{stderr}");
    assert!(stderr.contains("bad.azl:3:5") && stderr.contains("bad.azl:7:11"), "{stderr}");
    assert!(stderr.ends_with("found 2 errors\n"), "{stderr}");

    // The same as JSON: one object per line on standard output and a summary.
    for command in ["check", "build"] {
        let (code, stdout, stderr) = azol(&[command, "--json", &bad]);
        assert_eq!(code, 1);
        assert!(!stderr.contains("error"), "{stderr}");
        let lines: Vec<&str> = stdout.lines().collect();
        assert_eq!(lines.len(), 3, "{stdout}");
        assert!(lines.iter().all(|line| line.starts_with('{') && line.ends_with('}')), "{stdout}");
        assert!(lines[0].starts_with("{\"severity\":\"error\",\"code\":\"E0401\","), "{stdout}");
        assert!(lines[0].ends_with("\"line\":2,\"column\":5,\"end_line\":2,\"end_column\":8,\"replacement\":\"var\"}}"), "{stdout}");
        assert!(lines[1].contains("\"code\":\"E0201\"") && lines[1].contains("\"line\":7,\"column\":11,\"end_line\":7,\"end_column\":18"), "{stdout}");
        assert!(!lines[1].contains("\"fix\""), "{stdout}");
        assert_eq!(lines[2], "{\"summary\":{\"errors\":2,\"files\":1}}");
        assert!(!dir.join("bad").exists());
    }

    // A build that succeeds says so in JSON too.
    let exe = dir.join("good-program");
    let (code, stdout, stderr) = azol(&["build", "--json", &good, &lib, "-o", exe.to_str().expect("a plain name")]);
    assert_eq!((code, stdout.as_str()), (0, "{\"summary\":{\"errors\":0,\"files\":2}}\n"), "{stderr}");
    assert!(exe.is_file());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn explain_describes_every_code() {
    let (code, listed, _) = azol(&["explain"]);
    assert_eq!(code, 0);
    assert!(listed.lines().count() > 50, "{listed}");
    for line in listed.lines() {
        let (code, text, _) = azol(&["explain", &line[..5]]);
        assert_eq!(code, 0, "{line}");
        assert!(text.starts_with(&line[..5]) && text.contains("Messages with this code:"), "{text}");
    }
    let (code, text, _) = azol(&["explain", "E0408"]);
    assert_eq!(code, 0);
    assert!(text.contains("A list cannot be changed while a `for` loop is going through it.") && text.contains("Wrong:") && text.contains("Right:"), "{text}");
    let (code, stdout, stderr) = azol(&["explain", "E9999"]);
    assert_eq!((code, stdout.as_str()), (1, ""));
    assert!(stderr.contains("there is no error code 'E9999'"), "{stderr}");
}
