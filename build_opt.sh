#!/bin/sh
# Copyright (c) 2026 Seçkin Sefa Durası, github.com/durasi
#
# Builds `./azol-opt`, the optimising path as a tool of its own
# (compiler/opt/opt_build_main.azl), with `./azol`:
#
#     sh build.sh && sh build_opt.sh [-o PATH]
#
# `azol-opt build <file.azl>... -o <out> [--target TARGET]` then builds a
# program through the optimiser, for this machine when no target is named.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cd "$here"
out=azol-opt
if [ "${1:-}" = "-o" ] && [ -n "${2:-}" ]; then out=$2; fi
case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) host=aarch64_macos ;;
    Linux-x86_64) host=x86_64_linux ;;
    Linux-aarch64 | Linux-arm64) host=aarch64_linux ;;
    *) echo "build_opt.sh: no azoL target for $(uname -s) $(uname -m)" >&2; exit 1 ;;
esac
if [ ! -x ./azol ]; then echo "build_opt.sh: there is no ./azol; run sh build.sh first" >&2; exit 1; fi
list=
while read -r name; do
    case "$name" in
        "" | "#"*) ;;
        *) list="$list compiler/$name" ;;
    esac
done < compiler/opt/OPT_SOURCES
# shellcheck disable=SC2086
./azol build $list "compiler/host_$host.azl" -o "$out"
echo "built $out"
