# Closing the run-time gap with optimized Rust

## Summary

- On x86-64 Linux, azoL already runs fewer instructions than `rustc -O` in two of the eight benchmarks (`tree`, `words`) and 9% to 47% more in the other six.
- Six concrete causes explain those gaps. Five of them can be fixed inside today's single-pass generator, with help from the checker; none needs an optimizer.
- After those fixes the measured and computed instruction counts put seven of eight benchmarks at or under 1.05 times `rustc -O`; `loops` stays at 1.15 until the compiler can do strength reduction.
- `sieve` is slow because a `bool` in a list takes 8 bytes, not because of its code: the same program on a byte buffer ran 2.8 times faster here.
- Matching LLVM everywhere is not realistic. Code that LLVM vectorizes or unrolls will stay several times slower; none of the eight benchmarks is such code.

Everything below was measured on x86-64 Linux (the sandbox, 2 shared cores) at commit `8355cb6`, with `rustc 1.97.0 -O`. Instruction counts are exact (callgrind). Wall times are best of 7 on a loaded machine and only indicate direction. **Nothing here was measured on ARM64 hardware**: the Apple M5 ratios quoted are the user's earlier measurements, and the causes found on x86-64 are assumed, not shown, to be the causes there.

## 1. Where the instructions go

| Benchmark | azoL (Ir) | `rustc -O` (Ir) | Ratio | Wall here, azoL / Rust | M5 ratio (user) |
|---|---|---|---|---|---|
| `fib40` | 4,470,664,007 | 3,248,800,071 | 1.38 | 1.17 s / 0.54 s | 1.6 |
| `loops` | 483,692,460 | 420,128,470 | 1.15 | 0.060 / 0.043 | 1.1 |
| `sort` | 679,209,442 | 568,302,479 | 1.20 | 0.44 / 0.52 | 1.3 |
| `sieve` | 714,103,862 | 657,422,457 | 1.09 | 1.16 / 0.28 | 2.4 |
| `particles` | 2,208,302,670 | 1,504,682,472 | 1.47 | 0.21 / 0.18 | 2.0 |
| `tree` | 1,023,407,869 | 1,203,590,175 | 0.85 | 0.44 / 0.43 | 1.8 |
| `append` | 469,460,443 | 333,871,038 | 1.41 | 0.124 / 0.116 | 1.4 |
| `words` | 477,937,553 | 552,979,288 | 0.86 | 0.19 / 0.17 | 1.5 |

### Causes, with their share of each gap

| Benchmark | Cause | Share of the gap |
|---|---|---|
| `fib40` | `rustc` turns the second recursive call into a loop, so it makes 165.6 M calls where azoL makes 331.2 M. | about 80% |
| `fib40` | The x86-64 generator builds the frame before testing `n < 2`; the base case costs 11 instructions instead of 4. | about 20% |
| `loops` | `rustc` replaces `i * j` by an addition and unrolls the loop twice: 16.8 instructions per round against 19.3. | 100% |
| `sort` | The last `quicksort(...)` call is a real call in azoL; `rustc` loops. Per-call code: 121 M against 67 M. | 54 M of 93 M |
| `sort` | The swap reloads both elements and checks three bounds: 11 instructions against 7. | 43 M of 93 M |
| `sieve` | `List[bool]` stores 8 bytes per flag: 160 MB against 20 MB. D1 misses 38.1 M against 19.5 M; last-level misses 2.50 M against 0.77 M. | all of the time gap |
| `particles` | Bounds check and list header reloaded from memory on every round. | 3 of 7 extra instructions |
| `particles` | `x < 0 \|\| x > WIDTH` is two compares and two jumps per coordinate; `rustc` uses one unsigned compare. | 4 of 7 extra instructions |
| `append` | `rt_str_append` copies byte by byte and saves six registers: 82 instructions per call, 3.6 M calls, 63% of the run. | all |
| `tree`, `words` | azoL runs fewer instructions than Rust here. The M5 gap is not explained by anything measured on x86-64. | unknown |

### Evidence

`fib`: execution counts of the entry instruction.

```text
azoL   331160281  4024e0: push rbp          11 instructions on the base case
rustc  165580141  13d10:  push r14          second call became `ja 13d20`
```

`sieve`: the inner loops are the same length; only the store differs.

```text
azoL   46588036  cmp rbx,[rbp-0x70] / jb / mov r10,[rbp-0x78] / mov QWORD PTR [r10+rbx*8],1 / add / cmp / jle
rustc  46588036  cmp r11,rbx / jae / mov BYTE PTR [r14],1 / add / add / cmp / jle
```

`particles`: per round, azoL 22 instructions, `rustc` 15.

```text
azoL   cmp rbx,[r14+8] / jb / mov r8,[r14] / lea rcx,[rbx*4] / lea r10,[r8+rcx*8] ... test r12,r12 / jl / cmp r12,0x186a0 / jle
rustc  mov rbx,[rax-8] / mov rdx,[rax-0x18] / add rdx,rbx / cmp rdx,0x186a0 / jbe
```

`sort`: the scan loops are 6 instructions per step in both compilers. The difference is in the call and the swap.

```text
azoL   3360081 entries x 14 + 1680040 x 44 = 121 M      rustc  1680041 entries x about 40 = 67 M
```

`append`: the copy loop inside `rt_str_append`.

```text
6599900  movzx r9d,BYTE PTR [r14+rsi] / mov BYTE PTR [r8+rsi-3],r9b   (four bytes per round, then a tail loop)
```

## 2. Experiments

| Experiment | Result | Verdict |
|---|---|---|
| `sieve` on a byte string (`push`, `set`, `s[i]`) instead of `List[bool]` | wall 1.16 s to 0.42 s (Rust 0.28 s); D1 misses 38.1 M to 18.6 M (Rust 19.5 M) | Memory is the cause. It ran 1,506 M instructions because `str.set` is generated poorly (four `push`/`pop` pairs per call); a native packed list would keep today's 714 M. |
| `fib` written with one call as a loop, as `rustc` does it | 4,470 M to 3,477 M instructions (Rust 3,249 M: 1.07); wall 1.17 s to 0.69 s (Rust 0.54 s) | The transformation is most of the gap. |
| Runtime `copy` by 8-byte words (scratch copy of the repository) | `append` 469.5 M to 429.9 M; `words` 477.9 M to 469.0 M; same output | Helps, but the call itself is the larger cost. |

Not run as experiments, computed from the per-instruction counts above:

- `particles` with the header in registers, the implied bounds check dropped and the range test fused: 22 to 15 instructions per round, 2,208 M to about 1,508 M.
- `sort` with the last call as a jump and one reload less per scan step: 679 M to about 570 M.
- `fib` with the base case tested before the frame: 4,470 M to about 3,310 M.
- `append` with an inline fast path for a literal that fits: about 82 to about 12 instructions per append, 469 M to about 220 M.

## 3. The optimizations

### A. Within the single-pass generator

| # | Optimization | Gain | Needs | Compile cost | Risk |
|---|---|---|---|---|---|
| A1 | `List[bool]` stored as 1 byte per element | `sieve` time to about Rust's | Element size per list type in checker, both generators, three runtimes (`push`, clone, drop) | none | Medium: every list path takes an element size |
| A2 | Self call in tail position becomes a jump | `sort` −54 M | Checker marks the call; generator assigns parameters and jumps | none | Low; owned temporaries must be dropped first |
| A3 | List pointer and length kept in registers through a loop the checker proves does not resize the list; no check when the loop condition is `i < xs.len()` | `particles` −300 M, `sort` −55 M | A per-loop fact from the checker, which already tracks who may change a list | one extra walk of loop bodies in the checker | Medium: a wrong fact removes a bounds check |
| A4 | `x < 0 \|\| x > C` as one unsigned compare | `particles` −400 M | Pattern in the condition generator | none | Low |
| A5 | Base case tested before the frame on x86-64 (ARM64 has it) | `fib40` −1,160 M | Port of existing ARM64 logic | none | Low |
| A6 | Inline fast path for `s = s + literal`; word copies in the runtimes | `append` −250 M | Generator; runtimes | none | Low |
| A7 | Swap and repeated element reads reuse the loaded value | `sort` −43 M | Extension of the element tracking both generators have | none | Medium |

### B. Needing an intermediate form

| # | Optimization | Gain on the eight | Needs |
|---|---|---|---|
| B1 | Recursion with an accumulator turned into a loop (`fib`) | `fib40` 1.38 to 1.07 measured in the source-level experiment | A rewrite before code generation |
| B2 | Strength reduction and loop-invariant code motion | `loops` 1.15 to about 1.0 (estimate) | Linear IR, loops, liveness |
| B3 | Linear-scan register allocation, common subexpressions, general bounds-check elimination | A few percent each on `sort`, `particles`, `tree`; makes A3 and A7 general | The same IR |
| B4 | Small structs in registers | Not visible in these eight | The same IR |

### C. Only a heavy optimizer

Auto-vectorization, unrolling, aggressive whole-program inlining. `rustc` unrolled `loops` twice; none of the eight hot loops is vectorized. On array arithmetic that LLVM vectorizes, expect azoL to stay 2 to 8 times slower. That figure is general experience with scalar against SIMD code, not something measured here.

## 4. Recommendation

Do A first, in one mode. It keeps the 6 ms build and is enough for these benchmarks.

| Step | fib40 | loops | sort | sieve | particles | tree | append | words |
|---|---|---|---|---|---|---|---|---|
| Today (instruction ratio) | 1.38 | 1.15 | 1.20 | 1.09 | 1.47 | 0.85 | 1.41 | 0.86 |
| 0. Profile `tree`, `words`, `append` on the Mac | | | | | | | | |
| 1. A1 packed `bool` | | | | 1.09, memory equal | | | | |
| 2. A5 early base case | 1.02 | | | | | | | |
| 3. A2 tail self call | | | 1.10 | | | | | |
| 4. A3 + A4 list headers, range test | | | 1.00 | | 1.00 | | | |
| 5. A6 append fast path | | | | | | | about 0.7 | |
| 6. A7 reuse of loaded elements | | | 0.93 | | | | | |
| After A | 1.02 | 1.15 | 0.93 | 1.09 | 1.00 | 0.85 | 0.7 | 0.86 |
| After B1, B2 (optimizing mode) | 1.07 measured form | about 1.0 | | | | | | |

Notes on reading the table:

- Steps 2 to 6 are computed from instruction counts, not built. Step 1's time effect is measured; its instruction ratio does not change.
- Instruction ratio is not time. `fib40` at 1.02 still makes twice the calls: the looped form measured 1.29 times Rust's wall time here, so expect about 1.3 without B1.
- Step 0 comes first because the two benchmarks where azoL is ahead on x86-64 are 1.5 and 1.8 times behind on the M5. Until that is profiled there (`sample`, or a Rust build for ARM64 Linux in the emulator), the M5 end state for `tree`, `words` and `append` cannot be predicted.

An optimizing mode (`azol build -O`) is worth building only after A, and only if `loops`-like and `fib`-like code matters. Rough size, as an estimate: 6,000 to 8,000 lines of azoL (IR, liveness, linear scan, four or five passes, two lowerings), compiling perhaps 5 to 10 times slower than today's roughly 7 million lines per second. Neither number is measured.

Every step lands behind the existing checks: the three target suites, the differential fuzzer, and byte-identical output of the compiler written in azoL. A1 and A3 need new fuzzer shapes (lists of `bool` through every list operation; lists resized inside loops by calls).

## 5. Memory representation

| Change | Speed effect | Effect on the language |
|---|---|---|
| 1-byte `bool` in lists (A1) | `sieve` 2.8 times faster here | None visible; `List[bool]` keeps its meaning |
| 1-byte `bool` and enum tags in structs | Smaller `Tree` nodes (32 to 24 bytes would match Rust); `tree` D1 misses are 17.4 M against 9.6 M | None visible; changes struct layout in both generators |
| 32-bit fields | None without new integer types | Would add types; not recommended |
| Small strings stored inline | Unknown; not measured | None visible; complicates every string path |
| Size classes in the Linux allocator | `tree` spends 21% in `rt_alloc` and `rt_free` | None |

## 6. What this document does not show

- No ARM64 measurement. The M5 is the machine the user will quote.
- Steps 2 to 6 are arithmetic on counts, not working code.
- The wall times were taken with a load average above 5.
- The inlining another agent is adding is not in this worktree; it will change `fib40`, `tree` and `words`.
