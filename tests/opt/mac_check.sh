#!/bin/sh
# Checks the optimising ARM64 back end on a real Mac (Apple silicon).
#
#     sh build.sh                  once, if ./azol is not there yet
#     sh tests/opt/mac_check.sh
#
# Needs nothing but ./azol. It builds two tools with it, `azol-lower`
# (a program as intermediate form) and `azol-back` (intermediate form to
# an executable through the new back end), and then:
#
#   1. builds a set of test programs twice, the default way (`azol
#      build`) and through `azol-lower emit` and `azol-back build
#      --target aarch64-macos`, runs both and compares what they print,
#      what they write to standard error and their exit codes;
#   2. does the same for the four benchmark kernels (tests/opt/kernels,
#      hand-written intermediate form of bench/fib40, loops, sieve and
#      sort) against `azol build` of the benchmarks, and prints how long
#      each takes in both ways, the best of five runs.
#
# One line for every program. The last line is `MAC CHECK PASSED`, or
# says how many programs failed; then the exit code is 1.
#
# Nothing is installed and nothing outside a temporary folder is
# written; the folder is removed at the end unless something failed.

here=$(cd "$(dirname "$0")/../.." && pwd)
cd "$here" || exit 2

if [ "$(uname -s)-$(uname -m)" != "Darwin-arm64" ]; then
    echo "mac_check.sh is for macOS on Apple silicon; this is $(uname -s) $(uname -m)."
    echo "On other machines the same back end is tested in emulators: python3 tests/opt/run_back_arm64_tests.py"
    exit 2
fi
if [ ! -x ./azol ]; then
    echo "There is no ./azol here. Run: sh build.sh"
    exit 2
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/azol-mac-check.XXXXXX") || exit 2
c=compiler
o=compiler/opt

echo "Building azol-lower and azol-back with ./azol ..."
if ! ./azol build $c/driver.azl $c/token.azl $c/lexer.azl $c/ast.azl $c/parser.azl $c/sema.azl \
    $c/sema_fn.azl $c/embedded.azl $o/ir.azl $o/ir_text.azl $o/ir_cfg.azl $o/ir_verify.azl \
    $o/ir_interp.azl $o/ir_lower.azl $o/ir_lower_stmt.azl $o/ir_lower_expr.azl \
    $o/lower_tool_main.azl -o "$work/azol-lower" > "$work/build.log" 2>&1; then
    cat "$work/build.log"
    echo "MAC CHECK FAILED: azol-lower does not build (messages above)"
    exit 1
fi
if ! ./azol build $c/bytes.azl $c/code_file.azl $c/elf.azl $c/runtime_x64.azl $c/runtime_arm64.azl \
    $c/macho.azl $c/sha256.azl $o/ir.azl $o/ir_text.azl $o/ir_cfg.azl $o/ir_verify.azl $o/mach.azl \
    $o/regalloc.azl $o/enc_x64.azl $o/sel_x64.azl $o/emit_x64.azl $o/back_x64.azl $o/enc_arm64.azl \
    $o/sel_arm64.azl $o/emit_arm64.azl $o/back_arm64.azl $o/back_tool_main.azl \
    -o "$work/azol-back" > "$work/build.log" 2>&1; then
    cat "$work/build.log"
    echo "MAC CHECK FAILED: azol-back does not build (messages above)"
    exit 1
fi

failed=0

# run_in <name> <executable>: runs it in the work folder and leaves its
# output, errors and exit code in <name>.out, <name>.err and <name>.code.
run_in() {
    (cd "$work" && "$2" > "$1.out" 2> "$1.err"; echo $? > "$1.code")
}

# same <a> <b>: whether two runs printed, complained and ended alike.
same() {
    cmp -s "$work/$1.out" "$work/$2.out" && cmp -s "$work/$1.err" "$work/$2.err" && cmp -s "$work/$1.code" "$work/$2.code"
}

# best_of_five <executable>: the shortest of five runs, in seconds.
best_of_five() {
    if [ ! -x /usr/bin/time ]; then
        echo "?"
        return
    fi
    best=
    for round in 1 2 3 4 5; do
        took=$( { /usr/bin/time -p "$1" > /dev/null; } 2>&1 | awk '$1 == "real" { print $2 }')
        if [ -z "$best" ] || awk -v a="$took" -v b="$best" 'BEGIN { exit !(a < b) }'; then
            best=$took
        fi
    done
    echo "$best"
}

echo
echo "Test programs: default build against lower + azol-back"
for name in arithmetic bits control enums functions lists map matching strings structs \
    text_methods try_operator numbers_edge index_fail div_zero stack_overflow; do
    source=tests/programs/$name.azl
    if [ ! -f "$source" ]; then
        echo "  skip  $name (no $source in this tree)"
        continue
    fi
    if ! ./azol build "$source" -o "$work/plain-$name" > "$work/build.log" 2>&1; then
        echo "  FAIL  $name: azol build refuses it: $(head -1 "$work/build.log")"
        failed=$((failed + 1))
        continue
    fi
    if ! "$work/azol-lower" emit "$source" > "$work/$name.ir" 2> "$work/build.log"; then
        echo "  FAIL  $name: azol-lower refuses it: $(head -1 "$work/build.log")"
        failed=$((failed + 1))
        continue
    fi
    if ! "$work/azol-back" build "$work/$name.ir" -o "$work/opt-$name" --target aarch64-macos --check --program > "$work/build.log" 2>&1; then
        echo "  FAIL  $name: azol-back refuses it: $(head -1 "$work/build.log")"
        failed=$((failed + 1))
        continue
    fi
    run_in "plain-$name" "$work/plain-$name"
    run_in "opt-$name" "$work/opt-$name"
    if same "plain-$name" "opt-$name"; then
        echo "  ok    $name (exit $(cat "$work/opt-$name.code"))"
    else
        echo "  FAIL  $name: default build exits $(cat "$work/plain-$name.code"), the back end's $(cat "$work/opt-$name.code"); compare $work/plain-$name.out and .err with $work/opt-$name.out and .err"
        failed=$((failed + 1))
    fi
done

echo
echo "Benchmark kernels: seconds, best of five (default build of bench/NAME.azl; the kernel through azol-back)"
for name in fib40 loops sieve sort; do
    if [ ! -f "bench/$name.azl" ] || [ ! -f "tests/opt/kernels/$name.ir" ]; then
        echo "  skip  $name (not in this tree)"
        continue
    fi
    if ! ./azol build "bench/$name.azl" -o "$work/plain-$name" > "$work/build.log" 2>&1; then
        echo "  FAIL  $name: azol build refuses the benchmark: $(head -1 "$work/build.log")"
        failed=$((failed + 1))
        continue
    fi
    if ! "$work/azol-back" build "tests/opt/kernels/$name.ir" -o "$work/opt-$name" --target aarch64-macos --check --program > "$work/build.log" 2>&1; then
        echo "  FAIL  $name: azol-back refuses the kernel: $(head -1 "$work/build.log")"
        failed=$((failed + 1))
        continue
    fi
    run_in "plain-$name" "$work/plain-$name"
    run_in "opt-$name" "$work/opt-$name"
    if same "plain-$name" "opt-$name"; then
        echo "  ok    $name: default $(best_of_five "$work/plain-$name") s, back end $(best_of_five "$work/opt-$name") s (both print $(head -1 "$work/opt-$name.out"))"
    else
        echo "  FAIL  $name: the default build prints $(head -1 "$work/plain-$name.out") and exits $(cat "$work/plain-$name.code"), the back end's prints $(head -1 "$work/opt-$name.out") and exits $(cat "$work/opt-$name.code")"
        failed=$((failed + 1))
    fi
done

echo
if [ "$failed" -eq 0 ]; then
    rm -rf "$work"
    echo "MAC CHECK PASSED"
    exit 0
fi
echo "MAC CHECK FAILED: $failed programs; everything is kept in $work"
exit 1
