# The benchmark suite and the gate

This folder defines, in a form anyone can rerun, what "azoL runs as fast as
`rustc -O`" means. It is the suite of
[docs/design/optimizer.md](../../docs/design/optimizer.md), section 8.

    python3 bench/gate.py

builds every program with `azol` and with `rustc`, checks that all builds
of a program print the same output, times them, and ends with one line:

    GATE: PASS (28 programs, geometric mean 0.97, none above 1.10)
    GATE: FAIL (22 programs above 1.10, geometric mean 1.61)

## The rule

- Time is the best wall time of the whole process over 10 runs, after one
  warm-up run.
- Ratio = azoL time / Rust time, with `rustc -O` (edition 2021).
- The gate passes when the geometric mean of the ratios is at most 1.00
  and no program is above 1.10, on the Apple M5 and on an x86-64 Linux
  machine.
- A second column gives the same against
  `rustc -C opt-level=3 -C target-cpu=native`. It is published with the
  first, pass or fail.
- Builds of one program that print different output make the run void.
- When the machine is busy the gate says so and gives no verdict
  (`--force` overrides and marks the result).
- Until the compiler has `azol build -O`, the gate measures the default
  build and says so in its report. It detects `-O` by itself.

Results are written to `results-<machine>-<system>.json` here.

## The programs

Each program is `<name>.azl` and `<name>.rs`. Both start with a comment
that states the task, the algorithm, the sizes and the output.

| Program | What it stresses |
|---|---|
| `fib40` | calls, recursion |
| `loops` | integer arithmetic in nested loops |
| `sort` | a hand-written quicksort |
| `sort_lib` | the library sort: `sorted()` against `sort_unstable()` |
| `sieve` | flags in a list |
| `particles` | an array of structs updated in place |
| `soa` | a struct of arrays, element-wise arithmetic |
| `tree` | allocating and walking trees |
| `binary_trees` | the classic allocation benchmark |
| `append` | building long strings |
| `words` | split, join, count |
| `wordfreq` | a map from text to a count |
| `map_int` | a map with integer keys |
| `lru` | a cache: a map and an order list |
| `matmul` | integer matrix multiply |
| `prefix` | prefix sums and a running maximum |
| `bits` | population count, bit reversal, xorshift |
| `crc32` | a table-driven checksum over 64 MB |
| `sha256` | block hashing |
| `dp_lcs` | a dynamic-programming table |
| `knapsack` | one-dimensional dynamic programming |
| `bfs` | breadth-first search on a grid |
| `dijkstra` | a binary-heap priority queue |
| `fannkuch` | permutations and reversals |
| `bignum` | a large factorial with a list of limbs |
| `json` | parsing and writing a document |
| `matcher` | glob matching with backtracking |
| `tokenizer` | a lexer over 2.4 million lines |
| `nbody`, `spectral`, `mandelbrot` | floating point; waiting for `f64` |

The three floating-point programs have their Rust version and an
`.azl.todo` file: the azoL program as it will be written once
[floats.md](../../docs/design/floats.md) is implemented. The gate lists
them as waiting and leaves them out; the public statement must say
"28 programs" until they join.

## Rules of fairness

These are what a critic will check, so every program keeps to them.

1. **Same work.** Both versions use the same algorithm and the same sizes
   and print the same output: a checksum or the final values, so that the
   work cannot be optimised away and a wrong result shows.
2. **Idiomatic Rust.** The Rust version is what a competent Rust programmer
   writes: iterators, slices, `Vec`, `&str`, the standard collections and
   the standard sort. It is not a transliteration of the azoL version.
   Safe Rust only, standard library only.
3. **The faster Rust counts.** Where a program uses `HashMap`, there is a
   second file `<name>.fx.rs`, the same program with an FNV-1a hasher
   written inline in place of the standard SipHash. The gate times both
   and compares azoL with the faster one, so the comparison cannot lean on
   Rust's slow default hasher.
4. **Idiomatic azoL.** The azoL version is written the way the
   documentation teaches, not bent around today's code generator.
5. **No input, no output in the timed part.** Inputs come from a small
   generator in the program (`seed = seed * 48271 % 2147483647`); output is
   a line or a few.
6. **Sizes.** The full size runs 0.2 to 2 seconds under `rustc -O`. A
   small size, for instruction counts under callgrind and for emulators,
   is chosen by one line that both versions share in meaning:
   `const SMALL = false` in azoL and `const SMALL: bool = false;` in Rust.
   The gate rewrites that line in a copy; nobody edits it by hand. In Rust
   the size goes through `std::hint::black_box`, so the compiler cannot
   fold the whole program into its answer.
7. **Differences that could not be avoided are stated** in the header
   comment of both files:

| Program | Difference |
|---|---|
| `map_int` | azoL's library map takes text keys only, so the azoL version carries its own open-addressing table; Rust uses `HashMap`. Replace it by `Map[i64, i64]` when `Map[K, V]` exists. |
| `bits` | azoL has no built-in for counting or reversing bits and no unsigned shift; it writes them with masks. Rust uses `count_ones` and `reverse_bits`. |
| `sha256`, `crc32` | azoL has only `i64`; 32-bit values are masked. Rust uses `u32` and `u8`. |
| `bfs`, `dijkstra` | azoL has no queue or priority queue; it uses a list with a read position and a heap written out. Rust uses `VecDeque` and `BinaryHeap`. |
| `binary_trees`, `tree` | azoL has no boxed pointer; an inner node owns a list of children. In `binary_trees` Rust uses two `Box`es. |
| `dp_lcs` | Swapping two lists copies them in azoL, so its two rows live in one list; Rust swaps two `Vec`s. |
| `tokenizer` | A compact lexer written for the suite, not the compiler's own; azoL checks keywords with `contains` on a list of strings, Rust with a `match` on bytes. |

## Running it

    python3 bench/gate.py                   the whole gate
    python3 bench/gate.py --list            the programs
    python3 bench/gate.py --only sieve,sort a part (no verdict)
    python3 bench/gate.py --instructions    also count instructions (valgrind; small sizes)
    python3 bench/gate.py --instructions --no-time
    python3 bench/gate.py --azol path/to/azol
    python3 bench/gate.py --readme          only the block between the gate markers

It needs Python 3, `rustc` and an `azol` executable (`./azol`, else
`./target/release/azol`, else `--azol`). It runs unchanged on macOS and
Linux. Exit code: 0 pass, 1 fail, 2 no verdict.

## Adding a program

1. Write `<name>.rs` first, the way you would for a Rust project, then
   `<name>.azl`, starting both from an existing pair: the header comment,
   the `SMALL` line and the `size` function are the same everywhere.
2. Choose the full size for 0.2 to 2 seconds under `rustc -O` and the
   small size for 20 to 200 million instructions.
3. Make both print the same output. `python3 bench/gate.py --only <name>`
   refuses the pair otherwise.
4. Run `azol fmt` on the `.azl` file.
5. If the two versions differ in more than syntax, say so in both headers
   and in the table above.
6. Add the program to the table of programs here.

A program is added because real programs do that kind of work, never
because azoL wins it; a program is never removed because azoL loses it.
