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

## The costs of the furthest programs (x86-64, instructions, after wave 3)

Where the instructions of the ten furthest programs went was measured
with callgrind on the small builds, instruction by instruction, and the
causes were removed in order of gain. Over the 28 programs the
optimising path went from **1.29** to **1.09** of `rustc -O`
(`python3 bench/gate.py --instructions --no-time --azol-opt <azol-opt>`,
sandbox, x86-64 Linux; instruction counts, not times, and not the M5).
Thirteen programs are at or under 1.00 and none is above 2.00.

| Program | azol-opt | Rust `-O` | ratio before | ratio now |
|---|---|---|---|---|
| `append` | 31,040,955 | 28,876,748 | 1.57 | 1.07 |
| `bfs` | 16,295,534 | 19,536,737 | 0.94 | 0.83 |
| `bignum` | 8,970,792 | 9,138,146 | 0.99 | 0.98 |
| `binary_trees` | 82,666,952 | 171,954,449 | 0.48 | 0.48 |
| `bits` | 64,000,846 | 47,442,104 | 1.37 | 1.35 |
| `crc32` | 63,060,160 | 60,740,975 | 1.08 | 1.04 |
| `dijkstra` | 52,655,863 | 31,869,069 | 2.09 | 1.65 |
| `dp_lcs` | 51,420,140 | 28,651,371 | 2.00 | 1.79 |
| `fannkuch` | 126,177,278 | 106,700,735 | 1.35 | 1.18 |
| `fib40` | 22,886,746 | 26,854,973 | 0.87 | 0.85 |
| `json` | 48,574,387 | 67,909,124 | 0.96 | 0.72 |
| `knapsack` | 54,619,595 | 76,174,857 | 0.76 | 0.72 |
| `loops` | 28,386,714 | 38,222,825 | 0.79 | 0.74 |
| `lru` | 55,208,915 | 100,614,838 | 0.60 | 0.55 |
| `map_int` | 52,704,221 | 34,312,308 | 1.82 | 1.54 |
| `matcher` | 74,756,657 | 80,962,388 | 1.00 | 0.92 |
| `matmul` | 37,535,248 | 37,856,938 | 1.17 | 0.99 |
| `particles` | 72,438,564 | 60,815,270 | 1.78 | 1.19 |
| `prefix` | 28,835,250 | 21,530,608 | 1.38 | 1.34 |
| `sha256` | 152,330,977 | 95,152,751 | 1.78 | 1.60 |
| `sieve` | 48,849,278 | 49,554,538 | 1.04 | 0.99 |
| `soa` | 45,173,937 | 23,085,052 | 3.13 | 1.96 |
| `sort` | 48,201,563 | 50,939,112 | 0.97 | 0.95 |
| `sort_lib` | 75,425,889 | 40,767,411 | 2.81 | 1.85 |
| `tokenizer` | 15,297,937 | 10,023,257 | 3.51 | 1.53 |
| `tree` | 17,235,377 | 26,699,739 | 0.65 | 0.65 |
| `wordfreq` | 86,670,279 | 58,934,598 | 1.78 | 1.47 |
| `words` | 33,172,099 | 18,479,005 | 2.35 | 1.80 |

What was done, with what each gave where it mattered most:

- **Block layout, branches passed by, one address for a `gep` with
  several users; string equality without the runtime; frame fields out
  of loops; checks past stores; the runtime copies eight bytes at a
  time; the library's sort merges between two lists; search filters by
  the first byte** (three commits; together 1.29 to 1.15). `soa` 3.13
  to 1.96, `sort_lib` 2.81 to 1.85, `tokenizer` 3.51 to 2.08.
- **`view`** (new pass, `compiler/opt/pass_view.azl`): a part of a
  string that is only read is not copied. `tokenizer` 2.08 to 1.60.
  It is the first of the four costs this document listed as uncovered
  (owned substrings); it covers the temporary that is compared or
  searched, not a part that is returned or kept (`json`'s
  `parse_string` returns one: unchanged).
- **The register allocator cuts a value that loses its register inside
  a loop at the top of the loop** (`ra_loop_top`), so that it is not
  loaded again every time round. `fannkuch` 1.38 to 1.18, `matmul` 1.15
  to 0.99, `bfs`, `dijkstra` and `dp_lcs` 4 to 5 percent. `tokenizer`
  lost 2.6 percent by it.
- **`thread` T3**: two tests of one number against constants become one
  range test. `particles` 1.45 to 1.19; this was the program that had
  been worse than its default build, which it no longer is (0.81 of
  it). The loss against the default build came from the loop's four
  compare-and-branch pairs being laid out with jumps between them; the
  layout pass of the first commit removed that, T3 halved the tests.
- **`thread` T4**: a jump that carries a computed answer into a block
  that only tests it does the test itself. `dijkstra` 1.83 to 1.65.
- **The runtime's copy ends with one word that may overlap** instead of
  up to seven single bytes, and copies of four to seven bytes are two
  half words. `append` 1.23 to 1.07, `words` 1.94 to 1.80, `wordfreq`
  and `json` 5 percent. The default builds gain the same.
- **String equality compares a word at a time** when there are eight
  bytes. `json` 0.77 to 0.72, `tokenizer` 1.62 to 1.53; `lru` and
  `wordfreq`, whose keys are short, lose half a percent.

Compiling: the optimising path builds the generated 44,004-line program
in 0.93 s before and after (best of three, sandbox), and executes 6.59
thousand million instructions for it where the tool built from the
previous state executed 7.93: the passes added cost less than the
faster copy in the runtime gives back to the compiler itself.

### What is left, and why

- `soa` 1.96, `particles` 1.19, `bits` 1.35: the loops are as tight as
  scalar code gets (no check, no reload, no move); Rust's are
  vectorised, or use a pointer that steps instead of an index that is
  scaled every time (`ivsr`, `vectorise`: planned passes).
- `sha256` 1.60: every rotation is six instructions where Rust has one,
  and every sum is masked to 32 bits. No pass can do better without
  knowing that a number is below 2^32, and a number loaded from a list
  is not known to be. **Proposal (language-visible, not done here):**
  the `u32` type of the legacy list, and `rotate_left`/`rotate_right`
  on it; with them this program needs no pass at all. The same for the
  missing unsigned shift (`>>>` or a method): `bits` and `crc32` mask
  after every shift.
- `sort_lib` 1.85: the merge loop keeps three bounds checks per
  element. `bce` refuses a counter with a conditional step; here the
  loop's own test (`i < mid`, `mid <= len`) would prove two of the
  three without induction. Not done. Rust's `sort_unstable` is also a
  different algorithm.
- `dp_lcs` 1.79: three checks of `table[row + j]` forms remain, and the
  byte `a[i - 1]`, which does not change in the inner loop, is loaded
  every time: `licm` does not move a load from a buffer out of a loop
  that may run zero times. Needs a guarded preheader (loop rotation in
  the intermediate form).
- `dijkstra` 1.65: the heap's checks; Rust's `BinaryHeap` has none.
- `words` 1.80, `wordfreq` 1.47, `map_int` 1.54: half of `words` and
  `wordfreq` is the runtime: allocation and release of every piece,
  and `str_append_buf` with a full prologue for ten bytes (`strbuild`,
  the size-class allocator: planned). `map_int` was not looked at.
- `tokenizer` 1.53: moves at the bottom of its main loop that put eight
  values back where the top expects them (the allocator gives the two
  ends of a loop edge different registers; there is no coalescing of
  block parameters beyond hints), and the search of 18 keywords, which
  Rust does the same way.
- Unexplained: after the three commits of the first half `dijkstra`
  measured 10 percent above the last figure the work before had noted
  for it; that figure may have included a change that was never
  committed. It was not looked into; T4 more than made up for it.
