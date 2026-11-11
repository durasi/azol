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
    assert!(message.contains("error: `+` needs `i64` but this is `str`"), "{message}");
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
