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


## Branches on data (x86-64, after `ifconv`)

Measured in the sandbox (x86-64 Linux, two cores shared with other
work; CPU seconds, best of 15, full sizes): `sort_lib` 0.92 s before
and 0.59 s after, with 24 percent MORE instructions (93.8 M against
75.4 M at the small size: 2.30 of `rustc -O` where it was 1.85). The
merge executes more instructions and no longer guesses. On the
instruction table this is a loss and on the clock a gain of a third;
the gate is the clock on the M5, where it has not been run.

Instructions over the 28 programs: geometric mean 1.088 before, 1.098
after, all of the difference `sort_lib`. Without `sort_lib` the mean
went down: `dp_lcs` 1.79 to 1.72, `tokenizer` 1.53 to 1.50, `fannkuch`
1.18 to 1.16 (all three from `ra_next_need`), `words` 1.80 to 1.83 and
`wordfreq` 1.47 to 1.48 (the merge reads the chosen item of a list of
strings once more).

Compile cost of the 44,004-line program through `azol-opt`: 1.18 s of
CPU before, 1.14 s after (best of three; the difference is noise).

Looked at and not done, with what was found:

- `dp_lcs`: the three bounds checks of a round are behind the `if` on
  the two letters, so none of them is the first thing a round does and
  rule B2 cannot take them into the limit: a round that does not reach
  a check does not fail at it. The program indexes one table with two
  offsets where the Rust version has two vectors. Written with two
  lists and `let held = above; above = row; row = held`, azoL clones
  both lists in every round of the outer loop (`list_clone` twice):
  81 M instructions against 49 M. The checker does not see that
  `above` is overwritten before it is read again, so the `let` copies.
  This is the "swap of two lists copies" cost; it wants either a
  `swap` in the library or `copyelide`.
- `map_int`: 27 percent of its instructions are `0.filled(n)`: a loop
  of `push` whose length and capacity live in frame memory because
  the slow way of `push` is given the header's address. Reading the
  header back after the slow way only (a 20-line change in `sroa`,
  kept as a patch, not committed) brought `map_int` down 3 percent but
  `words` up 8 and `wordfreq` up 4: the values then live in registers
  across the loop, and the allocator puts the loop's block parameters
  in slots and writes them on every back edge. That allocator
  behaviour (block parameters without a register at a loop top) is
  the next thing to fix; `tokenizer`'s moves at the bottom of its loop
  are the same thing.
- A layout that sorts the chains left over (blocks of a loop next to
  it, rarely run blocks last) was tried and made `dp_lcs` 13 percent,
  `dijkstra` 25 percent and `tokenizer` 11 percent worse; not kept,
  not understood.
- `ivsr`, `unroll`, `strbuild`: not started.
## The optimising path on the Apple M5, in time (13 November 2026)

The first run of `python3 bench/gate.py --azol-opt ./azol-opt` on real hardware, at commit `369d9f8`: Apple M5, Darwin 27.2.0, best of 10 runs of the whole process. The machine was busy (the gate's own calibration loop took between 58 and 85 ms and it gave no verdict), so single ratios can be off by a few hundredths; the picture is not.

Geometric means of the times: the default build **2.13** of `rustc -O`, `azol-opt` **1.60** of `rustc -O` and **0.75** of the default build; 22 of the 28 programs are above 1.10. On x86-64 the same sources run 1.09 of `rustc -O` and 0.62 of the default build in instructions, so the ARM64 side is behind the x86-64 side.

| Program | default | azol-opt | Rust `-O` | azol-opt / default | azol-opt / Rust |
|---|---|---|---|---|---|
| `append` | 0.080 s | 0.081 s | 0.077 s | 1.01 | 1.06 |
| `bfs` | 0.358 s | 0.307 s | 0.216 s | 0.86 | 1.42 |
| `bignum` | 0.320 s | 0.320 s | 0.166 s | 1.00 | 1.92 |
| `binary_trees` | 0.336 s | 0.315 s | 0.273 s | 0.94 | 1.15 |
| `bits` | 0.475 s | 0.181 s | 0.113 s | 0.38 | 1.60 |
| `crc32` | 0.362 s | 0.360 s | 0.317 s | 0.99 | 1.14 |
| `dijkstra` | 0.187 s | 0.122 s | 0.073 s | 0.65 | 1.66 |
| `dp_lcs` | 0.367 s | 0.258 s | 0.092 s | 0.71 | 2.81 |
| `fannkuch` | 0.151 s | 0.117 s | 0.131 s | 0.77 | 0.89 |
| `fib40` | 0.246 s | 0.146 s | 0.148 s | 0.59 | 0.99 |
| `json` | 0.083 s | 0.073 s | 0.058 s | 0.87 | 1.26 |
| `knapsack` | 0.205 s | 0.151 s | 0.127 s | 0.74 | 1.19 |
| `loops` | 0.216 s | 0.215 s | 0.253 s | 0.99 | 0.85 |
| `lru` | 0.179 s | 0.162 s | 0.127 s | 0.90 | 1.27 |
| `map_int` | 0.218 s | 0.132 s | 0.043 s | 0.60 | 3.07 |
| `matcher` | 0.190 s | 0.162 s | 0.140 s | 0.85 | 1.16 |
| `matmul` | 0.178 s | 0.127 s | 0.128 s | 0.71 | 0.99 |
| `particles` | 0.379 s | 0.286 s | 0.155 s | 0.76 | 1.84 |
| `prefix` | 0.170 s | 0.192 s | 0.058 s | 1.13 | 3.28 |
| `sha256` | 0.459 s | 0.267 s | 0.183 s | 0.58 | 1.47 |
| `sieve` | 0.247 s | 0.236 s | 0.068 s | 0.95 | 3.48 |
| `soa` | 0.475 s | 0.182 s | 0.121 s | 0.38 | 1.50 |
| `sort` | 0.190 s | 0.161 s | 0.160 s | 0.85 | 1.00 |
| `sort_lib` | 0.681 s | 0.398 s | 0.086 s | 0.58 | 4.61 |
| `tokenizer` | 0.392 s | 0.144 s | 0.070 s | 0.37 | 2.07 |
| `tree` | 0.148 s | 0.143 s | 0.088 s | 0.97 | 1.64 |
| `wordfreq` | 0.266 s | 0.232 s | 0.092 s | 0.87 | 2.51 |
| `words` | 0.312 s | 0.270 s | 0.135 s | 0.87 | 2.00 |

### Counts on ARM64 against times on the M5

`python3 tests/opt/run_driver_tests.py --count` (the emulator, small sizes, the same commit) next to the x86-64 counts of the gate and the M5 times above, all as `azol-opt` against the default build:

| Program | x86-64, instructions | ARM64 macOS, instructions | ARM64 Linux, instructions | M5, time |
|---|---|---|---|---|
| `append` | 0.97 | 0.94 | 0.97 | 1.01 |
| `bfs` | 0.60 | 0.67 | 0.69 | 0.86 |
| `bignum` | 0.54 | 0.50 | 0.51 | 1.00 |
| `binary_trees` | 0.84 | 0.87 | 0.89 | 0.94 |
| `bits` | 0.29 | 0.29 | 0.29 | 0.38 |
| `crc32` | 0.61 | 0.66 | 0.66 | 0.99 |
| `dijkstra` | 0.40 | 0.41 | 0.42 | 0.65 |
| `dp_lcs` | 0.48 | 0.52 | 0.52 | 0.71 |
| `fannkuch` | 0.77 | 0.71 | 0.71 | 0.77 |
| `fib40` | 0.63 | 0.84 | 0.84 | 0.59 |
| `json` | 0.76 | 0.76 | 0.82 | 0.87 |
| `knapsack` | 0.77 | 0.87 | 0.87 | 0.74 |
| `loops` | 0.65 | 0.89 | 0.89 | 0.99 |
| `lru` | 0.74 | 0.74 | 0.77 | 0.90 |
| `map_int` | 0.65 | 0.62 | 0.66 | 0.60 |
| `matcher` | 0.75 | 0.79 | 0.79 | 0.85 |
| `matmul` | 0.53 | 0.50 | 0.51 | 0.71 |
| `particles` | 0.81 | 0.55 | 0.56 | 0.76 |
| `prefix` | 0.60 | 0.59 | 0.59 | 1.13 |
| `sha256` | 0.37 | 0.33 | 0.33 | 0.58 |
| `sieve` | 0.70 | 0.66 | 0.66 | 0.95 |
| `soa` | 0.48 | 0.43 | 0.43 | 0.38 |
| `sort` | 0.75 | 0.72 | 0.72 | 0.85 |
| `sort_lib` | 0.47 | 0.50 | 0.55 | 0.58 |
| `tokenizer` | 0.38 | 0.44 | 0.44 | 0.37 |
| `tree` | 0.81 | 0.86 | 0.89 | 0.97 |
| `wordfreq` | 0.77 | 0.78 | 0.78 | 0.87 |
| `words` | 0.85 | 0.80 | 0.86 | 0.87 |

Geometric means: x86-64 0.62, ARM64 macOS 0.62, ARM64 Linux 0.64, M5 time 0.75. In instructions the ARM64 back end removes what the x86-64 one removes; no program is clearly behind. What the M5 does not give back is time that instructions do not measure:

- **Division.** `bignum` (0.50 in instructions, 1.00 in time), `loops` (0.89, 0.99), and the line `seed * 48271 % 2147483647` that makes the data of 18 of the 28 programs (`bfs`, `crc32`, `dijkstra`, `dp_lcs`, `json`, `knapsack`, `lru`, `map_int`, `matcher`, `matmul`, `particles`, `prefix`, `sha256`, `soa`, `sort`, `sort_lib`, `wordfreq`, `words`): the time is the processor's division, one instruction either way. `rustc` multiplies by a reciprocal. The pass `divconst` (below) does the same.
- **A value read back from memory one turn after it was written.** `prefix` (0.59 in instructions, 1.13 in time): `sums[i] = sums[i - 1] + source[i]` loads `sums[i - 1]`, which the turn before stored; each turn waits for the store to reach the load, and the checks the optimiser removed were never what the loop waited for. `rustc` keeps the running sum in a register. Closing it needs a pass that carries a stored value to the load of the next turn (not written; `dp_lcs` has the same shape behind its branches).
- **Memory.** `sieve` (0.66, 0.95): a `List[bool]` has eight bytes for each element, 240 MB where `rustc` uses 30 MB, and `false.filled(n)` is a loop of `push` that grows the buffer by doubling; the two inner loops are already seven and four instructions. Closing it needs elements of one byte (packed lists) and `filled` as one allocation.
- **Branches that cannot be predicted.** `sort_lib` (4.61 of Rust): the merge compares two elements and jumps, on data the processor cannot predict, with three bounds checks in each turn. `dp_lcs` chooses among three ways for each cell. They need `ifconv` (a choice of value without a jump) and more of `bce`.
- **Allocation and copies of structs.** `tree`, `binary_trees`, `append`, `words`, `wordfreq` gain little: their time is in the system library's `malloc`, `free` and `realloc`, which `rustc`'s programs call too, and in structs copied a word at a time (`ldr`, `str` for each word where `ldp`, `stp` would take two). Not looked at further here.

The macOS runtime (`runtime/runtime.s`) needed no port of the faster copy: it has no copy loop of its own, it calls the system library's `memcpy`, `memcmp` and `realloc`. The two Linux runtimes have the same `copy`, line for line.

### `divconst`, and right shifts as ARM64 operands (13 November 2026)

- `compiler/opt/pass_divconst.azl`, last in the order before `layout`: `n / d` and `n % d` for a known `d` of 3 or more that is no power of two become the high half of a product, a shift and the sign of `n`; a division and a remainder of the same number share the quotient. It raises the instruction count (one division becomes four or five instructions) and lowers the time. On this x86-64 machine, full sizes, best of three, while other work was running: `bignum` 1.55 s to 0.42 s, `loops` 2.28 s to 0.76 s, `crc32` 1.12 s to 0.83 s, `knapsack` 0.51 s to 0.54 s (within the noise). Not measured on the M5 yet; Apple's division is quicker than Intel's, so expect less there.
- `sel_arm64.azl`, `emit_arm64.azl`: `add`, `sub`, `and`, `orr`, `eor` take an operand shifted right by a known distance (`eor x15, x15, x14, asr #8`), as they took one shifted left. One instruction less for each: the table step of `crc32`, the hashes, the sign of `divconst`.
- `sel_arm64.azl`: the words of a struct copy go two at a time (`ldp`, `stp`) when the store between two loads cannot write what the second reads. `tree` at its small size on ARM64 macOS: 12.45 M instructions to 11.40 M.
- `compiler/opt/pass_carry.azl`: the running sum of `prefix` stays in a register (`ldr, add, str, add, cmp, b.lt`, what `rustc` writes for that loop). On this x86-64 machine `prefix` went from 0.93 s to 0.80 s; the M5, where the store reaching the load was the whole wait, should gain more. Not measured there yet.

### ARM64 instruction counts before and after (emulator, macOS target, small sizes)

| Program | default build | azol-opt at `369d9f8` | azol-opt now | ratio then | ratio now |
|---|---|---|---|---|---|
| `append` | 21,624,592 | 20,424,603 | 20,304,603 | 0.94 | 0.94 |
| `bfs` | 20,955,163 | 13,975,382 | 16,550,257 | 0.67 | 0.79 |
| `bignum` | 10,701,926 | 5,378,886 | 6,577,363 | 0.50 | 0.61 |
| `binary_trees` | 65,538,324 | 56,797,286 | 51,423,294 | 0.87 | 0.78 |
| `bits` | 200,000,443 | 58,000,420 | 55,000,420 | 0.29 | 0.28 |
| `crc32` | 86,009,274 | 56,644,787 | 60,841,138 | 0.66 | 0.71 |
| `dijkstra` | 123,788,300 | 50,885,133 | 53,087,123 | 0.41 | 0.43 |
| `dp_lcs` | 94,637,804 | 49,063,831 | 49,069,843 | 0.52 | 0.52 |
| `fannkuch` | 162,867,921 | 115,549,372 | 115,549,364 | 0.71 | 0.71 |
| `fib40` | 25,579,207 | 21,540,404 | 21,540,404 | 0.84 | 0.84 |
| `json` | 43,553,327 | 33,280,901 | 32,740,892 | 0.76 | 0.75 |
| `knapsack` | 62,474,620 | 54,499,066 | 54,507,465 | 0.87 | 0.87 |
| `loops` | 23,611,446 | 21,086,228 | 26,136,644 | 0.89 | 1.11 |
| `lru` | 50,378,515 | 37,497,117 | 37,648,400 | 0.74 | 0.75 |
| `map_int` | 75,717,465 | 47,178,871 | 48,447,387 | 0.62 | 0.64 |
| `matcher` | 84,898,160 | 67,044,866 | 67,135,691 | 0.79 | 0.79 |
| `matmul` | 61,584,008 | 31,015,482 | 34,216,087 | 0.50 | 0.56 |
| `particles` | 108,741,288 | 60,343,203 | 72,471,286 | 0.55 | 0.67 |
| `prefix` | 47,400,651 | 27,802,660 | 26,202,675 | 0.59 | 0.55 |
| `sha256` | 350,294,061 | 114,118,397 | 115,691,262 | 0.33 | 0.33 |
| `sieve` | 73,676,836 | 48,434,675 | 48,434,675 | 0.66 | 0.66 |
| `soa` | 105,125,661 | 45,090,729 | 45,116,743 | 0.43 | 0.43 |
| `sort` | 64,527,879 | 46,477,151 | 47,701,494 | 0.72 | 0.74 |
| `sort_lib` | 143,734,948 | 71,234,390 | 71,836,393 | 0.50 | 0.50 |
| `tokenizer` | 30,113,583 | 13,326,264 | 13,410,268 | 0.44 | 0.45 |
| `tree` | 13,958,713 | 12,058,219 | 11,402,907 | 0.86 | 0.82 |
| `wordfreq` | 80,996,480 | 63,554,700 | 63,944,705 | 0.78 | 0.79 |
| `words` | 29,335,536 | 23,455,525 | 23,295,524 | 0.80 | 0.79 |

Geometric means against the default build: 0.62 before, 0.64 after.

The mean went up, and that is `divconst`: a division that was two instructions (`sdiv`, `msub`) is five or six that do not divide (`loops` 0.89 to 1.11, `bfs` 0.67 to 0.79, `particles` 0.55 to 0.67, `bignum` 0.50 to 0.61; 18 of the 28 programs make their data with `% 2147483647`). The instruction ratio against `rustc -O` on x86-64 will rise for the same reason, while the times fall: where the two disagree, the gate is defined in time. `binary_trees`, `tree`, `prefix`, `bits`, `json`, `words` and `append` run fewer instructions (paired copies, `carry`, shifted operands).
