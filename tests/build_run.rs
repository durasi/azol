// Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi

//! Compiles every program in `tests/programs` with the real `azol`
//! binary. On an Apple Silicon Mac the result is also executed and its
//! output and exit code are compared with the `expect-` comments at the
//! top of each file. Elsewhere only the compilation is checked.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Expectation {
    exit: i32,
    output: String,
}

fn expectation(source: &str, path: &Path) -> Expectation {
    let field = |key: &str| {
        source
            .lines()
            .find_map(|line| line.strip_prefix(&format!("// {key}: ")))
            .unwrap_or_else(|| panic!("{} has no `// {key}:` comment", path.display()))
    };
    Expectation {
        exit: field("expect-exit").trim().parse().expect("expect-exit must be a number"),
        output: field("expect-output").split_whitespace().map(|part| format!("{part}\n")).collect(),
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

        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            let run = Command::new(&exe).output().expect("compiled program can be started");
            assert_eq!(
                String::from_utf8_lossy(&run.stdout),
                expected.output,
                "{}: wrong output",
                path.display()
            );
            assert_eq!(run.status.code(), Some(expected.exit), "{}: wrong exit code", path.display());
        }
    }
    let _ = fs::remove_dir_all(&out_dir);
}
