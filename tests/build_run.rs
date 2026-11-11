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

        // The compiler builds for the machine it runs on when it can; on
        // those machines the program can be run as well.
        let native = cfg!(all(target_os = "macos", target_arch = "aarch64")) || cfg!(all(target_os = "linux", target_arch = "x86_64"));
        if native {
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
