#!/bin/sh
# Builds the azoL compiler, which is written in azoL, into `./azol`.
#
#     sh build.sh                            the compiler for this machine
#     sh build.sh --target <target> -o <out> a compiler that runs on another machine
#     sh build.sh --with <azol>              start from that compiler, not the seed
#     sh build.sh --keep <folder>            leave the three stages in that folder
#
# Nothing else is needed: no other compiler, no linker, no package manager.
# The first build is made by the seed, a compiler for this machine that is
# kept in `bootstrap/`. The result builds the same sources again, and that
# result once more. The last two must be the same file, byte for byte (a
# fixed point): then the compiler in `./azol` is the one its own sources
# describe, whatever built it first. When the sources have not changed
# since the seed was made, all of them equal the seed too; a note says so
# when they do not.
#
# Targets: aarch64-macos, x86_64-linux, aarch64-linux.
set -e
here=$(cd "$(dirname "$0")" && pwd)

case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) host=aarch64-macos ;;
    Linux-x86_64) host=x86_64-linux ;;
    Linux-aarch64 | Linux-arm64) host=aarch64-linux ;;
    *) host=unknown ;;
esac

target=$host
out=
with=
keep=
while [ $# -gt 0 ]; do
    case "$1" in
        --target) target=$2; shift 2 ;;
        -o) out=$2; shift 2 ;;
        --with) with=$2; shift 2 ;;
        --keep) keep=$2; shift 2 ;;
        *) echo "usage: sh build.sh [--target <target>] [-o <out>] [--with <azol>] [--keep <folder>]" >&2; exit 2 ;;
    esac
done
if [ "$host" = unknown ]; then
    echo "build.sh: there is no azoL compiler for this machine ($(uname -s) on $(uname -m)) yet" >&2
    exit 1
fi
case "$target" in
    aarch64-macos | x86_64-linux | aarch64-linux) ;;
    *) echo "build.sh: unknown target '$target': aarch64-macos, x86_64-linux or aarch64-linux" >&2; exit 2 ;;
esac
seed=$here/bootstrap/$host/azol
if [ -z "$with" ]; then
    with=$seed
fi
if [ ! -x "$with" ]; then
    echo "build.sh: no compiler to start from at $with" >&2
    exit 1
fi
if [ -z "$out" ]; then
    if [ "$target" != "$host" ]; then
        echo "build.sh: say where the compiler for $target goes: -o <out>" >&2
        exit 2
    fi
    out=$here/azol
fi

# From here on the work happens in the repository's folder, with the
# source files named relative to it, so that a folder with a space in its
# name is no trouble.
absolute() {
    case "$1" in
        /*) echo "$1" ;;
        *) echo "$PWD/$1" ;;
    esac
}
out=$(absolute "$out")
with=$(absolute "$with")
if [ -n "$keep" ]; then
    keep=$(absolute "$keep")
fi
cd "$here"

# The compiler's source files: those listed in compiler/SOURCES and the
# one file that says which machine the compiler runs on.
sources_for() {
    list=
    while read -r name; do
        case "$name" in
            "" | "#"*) ;;
            *) list="$list compiler/$name" ;;
        esac
    done < compiler/SOURCES
    echo "$list compiler/host_$(echo "$1" | tr - _).azl"
}

if [ -n "$keep" ]; then
    work=$keep
else
    work=$(mktemp -d "${TMPDIR:-/tmp}/azol-build.XXXXXX")
    trap 'rm -rf "$work"' EXIT
fi
# Every stage is written under the same file name, `azol`, in a folder of
# its own: on macOS the name is part of the signature, so two otherwise
# equal executables with different names are different files.
mkdir -p "$work/1" "$work/2" "$work/3" "$work/out"
rm -f "$work/1/azol" "$work/2/azol" "$work/3/azol" "$work/out/azol"

sources=$(sources_for "$host")
"$with" build $sources -o "$work/1/azol" 2> "$work/log" || { cat "$work/log" >&2; exit 1; }
"$work/1/azol" build $sources -o "$work/2/azol" 2> "$work/log" || { cat "$work/log" >&2; exit 1; }
"$work/2/azol" build $sources -o "$work/3/azol" 2> "$work/log" || { cat "$work/log" >&2; exit 1; }
if ! cmp -s "$work/2/azol" "$work/3/azol"; then
    echo "DIFFERENT: the compiler does not reproduce itself (stages 2 and 3 are not the same file)" >&2
    exit 1
fi
size=$(wc -c < "$work/3/azol" | tr -d ' ')
if [ "$target" = "$host" ]; then
    cp "$work/3/azol" "$work/out/azol"
else
    "$work/3/azol" build $(sources_for "$target") --target "$target" -o "$work/out/azol" 2> "$work/log" || { cat "$work/log" >&2; exit 1; }
fi
# A fresh file, not one written over: macOS remembers the signature of a
# file it has run.
rm -f "$out"
cp "$work/out/azol" "$out"
chmod 755 "$out"

echo "FIXED POINT: stage 2 and stage 3 are the same $size bytes"
if [ "$target" != "$host" ]; then
    echo "built $out for $target with it ($(wc -c < "$out" | tr -d ' ') bytes)"
else
    echo "built $out"
fi
if ! cmp -s "$work/1/azol" "$work/2/azol"; then
    echo "note: the compiler this started from generates different code than the sources describe now (stage 1 differs)"
fi
if [ -x "$here/bootstrap/$target/azol" ] && ! cmp -s "$work/out/azol" "$here/bootstrap/$target/azol"; then
    echo "note: the result differs from the seed in bootstrap/$target: the sources have moved on since it was made"
fi
