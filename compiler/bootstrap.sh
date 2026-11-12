#!/bin/sh
# Builds the compiler written in azoL with a given azoL compiler, then lets
# the result build itself twice. The three results must be the same file.
#
#     compiler/bootstrap.sh <azol> [target] [folder for the results]
#
# `target` is the machine the new compiler will run on: `aarch64-macos`
# (the default on a Mac), `x86_64-linux` or `aarch64-linux`.
set -e
here=$(cd "$(dirname "$0")" && pwd)
azol=$1
if [ -z "$azol" ]; then
    echo "usage: compiler/bootstrap.sh <azol> [target] [folder]" >&2
    exit 2
fi
case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) host=aarch64-macos ;;
    Linux-x86_64) host=x86_64-linux ;;
    Linux-aarch64) host=aarch64-linux ;;
    *) host=unknown ;;
esac
target=${2:-$host}
out=${3:-/tmp/azol-bootstrap}
mkdir -p "$out"
sources=""
for name in self_main driver token lexer ast parser dump sema sema_fn x64 x64_gen runtime_x64 code_file elf bytes diag diag_table embedded format arm64 arm64_gen arm64_link runtime_arm64 macho sha256 "host_$(echo "$target" | tr - _)"; do
    sources="$sources $here/$name.azl"
done
"$azol" build $sources --target "$target" -o "$out/stage1"
if [ "$target" != "$host" ]; then
    echo "built $out/stage1 for $target; it cannot run on this machine ($host)"
    exit 0
fi
"$out/stage1" build $sources -o "$out/stage2"
"$out/stage2" build $sources -o "$out/stage3"
if cmp -s "$out/stage1" "$out/stage2" && cmp -s "$out/stage2" "$out/stage3"; then
    echo "FIXED POINT: stage 1, 2 and 3 are the same $(wc -c < "$out/stage3" | tr -d ' ') bytes"
else
    echo "DIFFERENT: the stages are not the same file" >&2
    exit 1
fi
