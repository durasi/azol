# Costs the first run of the gate found that no planned pass covers

From the baseline of the 28-program suite (default mode, x86-64, 12 November
2026: geometric mean 1.61, 22 programs above 1.10). These must be added to
the plan in [optimizer.md](optimizer.md).

1. **Owned substrings.** `slice` and `split` allocate where Rust borrows a
   `&str`. Seen in `tokenizer`, `words` (2.07) and `wordfreq` (1.93). Needs
   string views that the checker proves do not outlive their source, or
   stack promotion of slices that do not escape.
2. **Membership in a constant set of strings.** `keywords.contains(word)` is
   42% of `tokenizer`: up to 18 runtime compares per word where Rust's
   `match` is a switch on length. Needs `match` on strings compiled to a
   length switch, and `contains` on a constant list lowered the same way.
3. **No unsigned shift.** `>>` followed by a mask must be recognised as a
   logical shift, or the language needs one (the `u64` type of legacy.md
   brings it).
4. **Swapping two lists copies.** Forces a one-list table in `dp_lcs`
   (2.21). Needs a `swap` that exchanges the buffers, and the optimiser
   treating it as a move.
5. **Library gaps that cost speed:** the prelude merge sort (`sort_lib`
   8.22), no `popcount`, no queue, no heap, no boxed node.

## Baseline on the Apple M5 (12 November 2026, default mode, before `-O`)

`python3 bench/gate.py` on the user's Mac, best of 10 runs, load average
2.7 (so no verdict was given, but the picture is clear). Geometric mean of
azoL over `rustc -O`: **2.15** (x86-64 Linux gave 1.61). `target-cpu=native`
changes nothing on this machine.

| Program | Ratio | Program | Ratio | Program | Ratio | Program | Ratio |
|---|---|---|---|---|---|---|---|
| `sort_lib` | 9.33 | `soa` | 3.94 | `dijkstra` | 2.48 | `knapsack` | 1.62 |
| `tokenizer` | 5.58 | `sieve` | 3.52 | `particles` | 2.46 | `json` | 1.46 |
| `map_int` | 5.42 | `wordfreq` | 2.96 | `sha256` | 2.45 | `lru` | 1.46 |
| `dp_lcs` | 4.24 | `prefix` | 2.86 | `words` | 2.43 | `matmul` | 1.41 |
| `bits` | 4.11 | | | `bignum` | 1.92 | `matcher` | 1.36 |
| | | | | `fib40` | 1.68 | `fannkuch` | 1.17 |
| | | | | `bfs` | 1.67 | `sort` | 1.17 |
| | | | | `tree` | 1.67 | `binary_trees` | 1.15 |
| | | | | | | `crc32` | 1.15 |
| | | | | | | `append` | 1.05 |
| | | | | | | `loops` | 0.85 |

Things that differ from x86-64 and need an explanation from a profile on
the Mac: `map_int` (5.42), `dp_lcs` (4.24), `prefix` (2.86), `knapsack`
(1.62 here, 0.72 on x86-64), `tree` (1.67 here, 0.97 there). The default
ARM64 generator is weaker than the x86-64 one on list-heavy loops.

## Every pass in one order (x86-64, instructions)

Measured in the sandbox with `python3 bench/gate.py --instructions --no-time --azol-opt <azol-opt>` after `effects`, `gvn`, `licm`, `bce`, `inline`, `tailcall` and `accrec` were put in one order: over the 28 programs the optimising path runs **0.70** of the default build's instructions and **1.29** of `rustc -O`'s (the default build: 1.85). Twelve programs are at or under 1.00; the furthest are `tokenizer` 3.51, `soa` 3.13, `sort_lib` 2.81, `words` 2.35, `dijkstra` 2.09 and `dp_lcs` 2.00. `particles` is the one program that got worse than the default build (1.22 of it). These are instruction counts on x86-64, not times, and not the Apple M5: the optimising path has no end-to-end ARM64 driver yet.
