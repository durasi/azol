# Removing the first compiler

The Rust sources under `src/` are kept until `sh build.sh && sh test.sh`
is confirmed on a real Mac. Nothing uses them any more: `build.sh` starts
from the seeds in this folder. This file is the list for the commit that
deletes them; delete this file in the same commit.

## Before

On the Mac, in the repository:

    sh build.sh          FIXED POINT, and no note about the seed
    sh test.sh           ALL TESTS PASSED
    cmp azol bootstrap/aarch64-macos/azol && echo the seed is what the Mac builds

## The commands

    git rm -r src Cargo.toml Cargo.lock
    git rm tests/build_run.rs tests/format.rs
    git rm compiler/gen_tables_azl.py
    git rm bootstrap/REMOVE_RUST.md
    rm -rf target

`tests/format/` (the pairs) stays: `tests/run.py` reads it.
`compiler/gen_tables_azl.py` made `compiler/diag_table.azl` from
`src/diag.rs`; with that gone the `.azl` file is the table, and
`docs/errors.md` is written from it by `./azol explain --markdown`.

## Edits in the same commit

None of these changes what the compiler does, so the seeds stay valid
(`sh build.sh` must still print no note).

- `.gitignore`: remove the line `/target`.
- `README.md`: remove the "Temporary note" at the top of "Self-hosting".
- `compiler/diag_table.azl`: the comment at the top says "Generated from
  `src/diag.rs`, `src/main.rs` and `Cargo.toml` by
  `compiler/gen_tables_azl.py`; do not edit it by hand." Replace with:
  "This is the table: edit it here. `docs/errors.md` is written from it
  by `azol explain --markdown`; `sh test.sh` fails when that file is
  stale." (`VERSION` is then set here too.)
- `compiler/oracle.py`: the default oracle is then always the seed; drop
  the `target/release/azol` branch and the sentence about it, and the
  `src/diag.rs` test in `generators()`.
- `runtime/gen_runtime.py`, `runtime/linux_x64/gen_runtime.py`,
  `runtime/linux_arm64/gen_runtime.py`: each has three lines that also
  write the facts to `src/` "while that compiler's sources exist"; they
  do nothing once `src/` is gone and can be deleted with the sentence in
  the docstring.
- `compiler/check_self.py`, `tests/fuzz/fuzz.py`: `emulator_repo()` keeps
  working (it then always takes the facts from
  `runtime/linux_arm64/runtime.facts`).
- `tests/coverage_check.py` keeps working: it then only checks that the
  case files `tests/COVERAGE.md` names exist.

## What still names Rust afterwards, and why it stays

- `tests/COVERAGE.md` and the first line of the case files under
  `tests/errors/` and `tests/golden/` ("From the tests of the first
  compiler: src/sema.rs, ..."): the record of where each case came from.
- Comments in `compiler/*.azl` that say "as the Rust compiler" or "the
  Rust text" (`arm64.azl`, `x64.azl`, `sema.azl`, `code_file.azl`,
  `dump.azl`, `sema_main.azl`): history of the port; reword to "the first
  compiler" at leisure. Comments do not change the compiler's bytes.
- Comments in `runtime/runtime.s`, `runtime/linux_x64/runtime.ld`,
  `runtime/linux_arm64/runtime.c` and `runtime.ld` that say `src/runtime.rs`,
  `src/elf.rs` or `src/elf_arm64.rs` must agree: they now mean
  `runtime/*.facts`, `compiler/elf.azl` and `compiler/arm64_link.azl`.
- `bench/*.rs` and `bench/compare.py`: the Rust versions of the
  benchmark programs, for the comparison with `rustc`. They are not part
  of the compiler.
- `README.md`, `docs/design/*.md`, `AGENTS.md`: Rust as the language azoL
  is compared with.
- The reference emulators and runners outside the repository
  (`runall.py`, `runall_x64.py`, `runall_linux_arm64.py`,
  `x64harness/run.py`) build with `<repo>/target/release/azol`; they need
  `./azol` instead once `target/` is gone.

## After

    sh build.sh && sh test.sh
    python3 compiler/check_self.py --quick      (x86-64 Linux only)
    git commit -m "Remove the first compiler: azoL builds itself from the seeds"
