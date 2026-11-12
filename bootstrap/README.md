# The seed compilers

The azoL compiler is written in azoL, so a compiler is needed to build
it. This folder holds one for each machine azoL runs on. `sh build.sh`
starts from the one for your machine and needs nothing else.

| File | Runs on | Bytes | SHA-256 |
|---|---|---|---|
| `aarch64-macos/azol` | Apple Silicon macOS | 645120 | `efdc4e249b15ff95fe396e6a3e055680bd602b9e383bcf954b535623463e7f94` |
| `x86_64-linux/azol` | x86-64 Linux | 778240 | `2b29f9826e5ddeb2c6decf53e3c59cb5a9f7de3b40fdb91ab798195c0cf2f395` |
| `aarch64-linux/azol` | ARM64 Linux | 622083 | `01631a13890215a342b37850b8d678479598965400e59c9557cd125876f7ac9d` |

Each file is named `azol` in a folder of its own, not `azol-<target>`:
on macOS the file's name is part of its signature, and only under this
name is a seed the same file as the compiler `build.sh` leaves.

The three files in this table were built from the sources as they are in
commit `2e7a418` (later commits up to the one that added this file
changed comments and scripts only, which the compiler's bytes do not
depend on).

## What they are

Each is the compiler in `compiler/`, `lib/prelude.azl` and the
runtimes in `runtime/`, as those files were when the seeds were made,
built for its target from a fixed point: the compiler built the sources,
the result built them again, and the two results were the same file.
The Linux and macOS ARM64 seeds were built on x86-64 Linux with
`--target`.

They were first made with the compiler written in Rust that this
repository held until then (the last commit with `src/` in it): that
compiler and the azoL-written one produced the same three files, byte for
byte. Since then every seed is made by the seed before it.

## Making them again

On any of the three machines:

    sh build.sh
    sh build.sh --target aarch64-macos -o bootstrap/aarch64-macos/azol
    sh build.sh --target x86_64-linux  -o bootstrap/x86_64-linux/azol
    sh build.sh --target aarch64-linux -o bootstrap/aarch64-linux/azol
    shasum -a 256 bootstrap/*/azol      (sha256sum on Linux)

With unchanged sources this writes the same files back and `build.sh`
prints no note. The three machines produce the same three files.

## When to refresh them

Only when it is needed: when the compiler's sources start to use a part
of the language that the seeds do not know, so that the seeds can no
longer build them. Changing how the compiler works, or adding a feature
it does not use itself, needs no new seeds: `build.sh` builds through
the old ones, reaches a fixed point and says in a note that the result
differs from the seed.

Always refresh from a fixed point, all three at once, with the commands
above, after `sh test.sh` passes; then update the table in this file and
commit the three files together with the change that needed them. A
feature the compiler is to use in its own sources therefore takes two
steps: first add it and refresh the seeds, then use it.
