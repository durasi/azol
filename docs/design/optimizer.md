# The optimising back end: `azol build -O`

Design only; nothing here is implemented. The default build stays the
single fast pass. `-O` adds our own optimiser, written in azoL, with no
LLVM and no outside dependency. The goal the user set: azoL programs
compiled with `-O` run as fast as `rustc -O`.

## Summary

- The intermediate form is SSA with block parameters, one function at a
  time, with a memory model that carries what the checker already proves
  (no aliasing between variables, unique ownership of buffers, the four
  exclusivity rules).
- Twenty-one passes, each its own file with its own tests in a text form
  of the IR; linear-scan register allocation on SSA; one instruction
  selector per architecture on top of the existing encoders, linkers and
  runtimes.
- Experiments (section 2) hand-wrote the code these passes are specified
  to produce for the five benchmarks named in the task. Measured against
  `rustc -O` on x86-64, in instructions: `loops` 0.77, `fib40` 0.70,
  `sort` 0.95, `sieve` 0.67, `particles` 1.002 without unrolling and 0.90
  with the loop unrolled twice. All five outputs are identical to Rust's.
- "As fast as `rustc -O`" is defined by a gate (section 8): about 30
  programs, geometric mean at most 1.00 and no program above 1.10, on the
  Apple M5 and on x86-64. The claim is made in public only when the gate
  passes on both.
- Size: about 26,500 lines of azoL in six waves of parallel agents
  (section 11). Line counts and the `-O` compile speed are estimates.

## OPEN: decisions for the user

Each has a recommendation, and the design is written as if it is taken.

- **OPEN O1 (section 3.5).** May a program built with `-O` stop with a
  different run-time message than the default build when two different
  stops are both certain and nothing is printed between them?
  Recommendation: no. `-O` and the default build always give the same
  output, the same message and the same exit code. It costs a little
  freedom in moving checks and makes differential testing exact.
- **OPEN O2 (section 5.9).** On x86-64, vector loops get two versions:
  SSE2, which every x86-64 processor has, and AVX2, chosen at run time by
  one test of a flag set at start-up. Recommendation: yes. The executable
  still runs everywhere and matches `rustc -C target-cpu=native` on
  recent machines for the loops we vectorise.
- **OPEN O3 (section 7.2).** Ordinary structs get packed layout under both
  modes: a `bool` or an enum tag in a struct takes one byte, fields are
  ordered by size. Only structs declared for C (`extern`, see
  [legacy.md](legacy.md)) keep declaration order. Recommendation: yes;
  nothing in the language can observe the layout.
- **OPEN O4 (section 8).** The gate: geometric mean at most 1.00 and no
  single program above 1.10 against `rustc -O` (opt-level 3, default
  target CPU), with results against `-C target-cpu=native` published next
  to it. Recommendation: accept these two numbers as the public
  definition.
- **OPEN O5 (section 9.4).** Once the compiler built with `-O` compiles
  itself to the same output as the unoptimised compiler, the prebuilt
  compilers in `bootstrap/` are built with `-O`. Recommendation: yes; it
  is how the compiler written in azoL closes its own 2.5 times gap to the
  Rust-hosted one.

## 1. What the default generator cannot do, and why an IR

[speed.md](speed.md) measured the eight benchmarks and lists seven local
fixes (A1 to A7) that the single-pass generators can take. They stay
planned for the default mode. Three things need more than a single pass
over the tree:

| Need | Example | Why a tree walk cannot do it |
|---|---|---|
| A value's whole life in one register | loop counters, list headers, struct fields across a loop | The generator decides where a value lives before it has seen its uses. |
| Facts that hold across statements | `len == limit + 1` after a fill loop; `i < len` from the loop condition | No place to keep a fact and no way to ask whether a store in between killed it. |
| Rewrites of control flow | recursion into a loop, a multiplication into an addition per round, unrolling | The tree is the control flow. |

The IR is the place where those three become ordinary data-flow problems.

## 2. Experiments: does the plan reach `rustc -O`?

Method. For each benchmark the hot code was written by hand in x86-64
assembly exactly as the passes of section 5 are specified to produce it:
no trick was used that is not a named pass. A small C driver builds the
same input and prints the same result; the kernels are in the appendix.
Instructions were counted with callgrind, the kernel alone
(`--toggle-collect`), and added to the instructions the present azoL
build spends outside the kernel (measured by building the benchmark with
the kernel call removed). So the totals below still contain today's
unoptimised code for everything except the kernel. Rust is `rustc 1.97.0
-O`, whole program. Machine: x86-64 Linux, the sandbox. Every hand
program prints the same number as the azoL and the Rust build.

| Benchmark | azoL today | Hand kernel | Rest (azoL today) | Total | `rustc -O` | Ratio | Reached 1.00? |
|---|---|---|---|---|---|---|---|
| `loops` | 483,692,460 | 321,446,442 | about 0 | 321,446,442 | 420,128,470 | **0.77** | yes |
| `fib40` | 4,470,664,007 | 2,269,806,340 | about 0 | 2,269,806,340 | 3,248,800,071 | **0.70** | yes |
| `sort` | 679,209,442 | 445,430,713 | 94,405,415 | 539,836,128 | 568,302,479 | **0.95** | yes |
| `sieve` | 714,103,862 | 437,215,131 | about 0 | 437,215,131 | 657,422,457 | **0.67** | yes |
| `particles`, no unrolling | 2,208,302,670 | 1,501,138,760 | 6,629,246 | 1,507,768,006 | 1,504,682,536 | **1.002** | no, by 0.2% |
| `particles`, unrolled twice | | 1,351,538,760 | 6,629,246 | 1,358,168,006 | 1,504,682,536 | **0.90** | yes |

Which pass produced what:

| Benchmark | Passes that made the difference | Instructions per round, ours / Rust |
|---|---|---|
| `loops` | Strength reduction (`i * j` becomes `p += i`); range analysis (`0 <= p < 2^32`, so division by 7 is one multiplication and a shift); remainder-compare peephole (`p % 7 == 3` as `7 * (p / 7) == p - 3`); cold path moved out of line. No unrolling was needed. | 12 on the common path / 16.8 |
| `fib40` | Accumulator recursion (section 5.3): 165.6 M calls instead of 331.2 M, as in Rust; base case tested before the frame; two saved registers instead of three. | 13.7 per call / 19.6 |
| `sort` | Tail self call as a jump; list pointer and length loaded once per round (the callee is proved not to resize the list); compare against memory in the scan loops; bounds checks kept, as Rust keeps them. | scan step 5 / 6 |
| `sieve` | One byte per flag (representation, section 7.1); final value of an induction variable (`len == limit + 1` after the fill loop) lets every bounds check go. | inner loop 4 / 7 |
| `particles` | Header hoisting; pointer induction variable; `x < 0 \|\| x > W` as one unsigned compare; unrolling by two for the last 10%. | 15, or 13.5 unrolled / 15 |

Cache behaviour of the packed sieve, from cachegrind: D1 misses
19,480,916 against Rust's 19,482,091; last-level misses 774,176 against
775,415. Today's `List[bool]` has 38.1 M and 2.50 M
([speed.md](speed.md)).

Wall time, best of 7 to 15 runs, same machine, load average about 1.4.
Indicative only; the hand programs' set-up code is C, not azoL.

| | Hand | `rustc -O` | azoL today |
|---|---|---|---|
| `fib40` | 0.235 s | 0.245 s | 0.412 s |
| `loops` | 0.015 s | 0.026 s | 0.024 s |
| `sieve` | 0.110 s | 0.113 s | 0.286 s |
| `sort` | 0.158 s | 0.154 s | 0.201 s |
| `particles` (unrolled) | 0.069 s | 0.085 s | 0.098 s |

What the experiments do not show:

- Nothing was measured on ARM64. The same passes apply; the M5 numbers
  come only when the code exists and the user runs the gate.
- `sort` is ahead in instructions and level in time (0.158 s against
  0.154 s, inside the noise). Its time is dominated by branch
  mispredictions in the scan loops, which both compilers have.
- The hand code shows that the specified output is fast enough. It does
  not show that the passes will produce exactly that output; section 9
  is how that is checked.
- `tree`, `words` and `append` were not hand-written. On x86-64 azoL
  already runs fewer instructions than Rust in `tree` and `words`
  ([speed.md](speed.md)); their gap on the M5 is unexplained and is the
  first thing to profile on the Mac (section 11, wave 0).

## 3. The intermediate form

### 3.1 Choice

**SSA with block parameters; a control-flow graph of basic blocks; one
function in memory at a time, plus the bodies of functions small enough to
inline.**

- Block parameters instead of phi nodes: one rule ("a jump passes
  arguments") covers joins and loops, the text form reads like code, and
  there is no phi placement invariant to break. The cost is nothing.
- Not a linear three-address form with virtual registers: every pass
  would need its own reaching-definitions analysis.
- Not a sea of nodes: scheduling is a second compiler, and the fuzzer's
  failing cases would be unreadable.

### 3.2 Types

| Type | Meaning | When |
|---|---|---|
| `i64` | 64-bit integer, wrapping arithmetic | now |
| `bool` | 0 or 1, held in a full register | now |
| `ptr` | address of a frame object or inside a buffer | now |
| `i8`, `i32`, `u8`, `u32`, `u64` | only as the width of a `load`, `store`, `ext` or `trunc`; arithmetic happens in `i64` | with the narrow integers of [legacy.md](legacy.md); `u8` already for strings and packed `bool` |
| `f64` | IEEE double in a vector register | with [floats.md](floats.md) |
| `v128`, `v256` | vector of 2 or 4 `i64`, 16 or 32 bytes | created only by the vectoriser |

There are no aggregate values. A struct, an enum, a list header or a
string header is its slots, each an SSA value (section 3.6).

### 3.3 Instructions

```text
constants    iconst N            bconst 0|1          addr @data+N
arithmetic   add sub mul sdiv srem neg and or xor shl sar shr
             umulh smulh                 (high half, for division by constants)
compare      eq ne slt sle sgt sge ult ule ugt uge      -> bool
select       select c, a, b
convert      ext8u ext8s ext32u ext32s trunc8 trunc32
memory       load T, p, off  [region]
             store T, p, off, v  [region]
             frame N                     (address of frame object N)
             gep p, index, scale, off    (p + index * scale + off)
calls        call f(args) -> results     (internal convention, section 6.2)
             callrt NAME(args) -> results (platform convention; effects from a table)
vector       vload vstore vadd vsub vand vor vxor vcmpeq vcmpgt vsplat vextract
             vreduce_add vreduce_or vmovmask
terminators  jump B(args)
             br c, B1(args), B2(args)
             switch v, [k -> B(args)]..., default B(args)
             ret values
             trap KIND                   (index, divide, pop, slice, chr, memory)
```

`sdiv` and `srem` do not trap: the check for zero is an explicit `br` to a
`trap divide` block before them, so that the check can be removed or
moved like any other branch. The same holds for every bounds check.

### 3.4 The memory model

Every `load` and `store` names a **region**. Two operations can touch the
same memory only if their regions can overlap. Regions are assigned at
lowering from facts the checker has already proved; the optimiser never
has to rediscover them.

| Region | What it is |
|---|---|
| `frame N` | frame object N of this function (a local whose address is taken) |
| `buf(R)` | the buffer owned by the header in region R (a list's elements, a string's bytes) |
| `param K` | the value a by-reference parameter K points to |
| `ret` | the caller's space for a returned aggregate |
| `data` | read-only data |
| `rt` | the runtime's own state (allocator); only `callrt` touches it |

The facts, each with the rule of the language that proves it:

| # | Fact the optimiser may use | Why it is true |
|---|---|---|
| M1 | Two different frame objects never overlap. | They are different variables; azoL has no references. |
| M2 | A buffer has exactly one owner. `buf(R1)` and `buf(R2)` overlap only if R1 and R2 are the same region. | Unique ownership: assignment copies or moves, nothing shares. |
| M3 | A buffer never contains a pointer into a frame, into another variable's buffer, or to itself. Values form trees. | No reference types; every value is self-contained. |
| M4 | A `mut` parameter does not overlap any other parameter, nor anything else the function can name. | Exclusivity rule 1: a value passed `mut` cannot be passed again in the same call. |
| M5 | A read-only by-reference parameter is not written by anyone for the whole call, including by callees. | Parameters without `mut` are read-only; rule 1 excludes a simultaneous `mut` use. Two read-only parameters may be the same value, which is harmless because neither is written. |
| M6 | During a `for` over a list, the list's header and buffer are not written. | Exclusivity rule 2 (the loop holds the list). |
| M7 | Inside a `match` arm, the matched value is not written. | Exclusivity rule 3. |
| M8 | Within one expression, an operand read earlier is not changed by a later `mut` call. | Exclusivity rule 4. |
| M9 | A call writes only the regions of its `mut` arguments, the space for its result, and `rt`. | There are no globals other than constants, and M3. |
| M10 | A value after its last use is dead: its buffer may be reused or freed without a copy. | Last-use information from the checker (moves). |
| M11 | `data` is never written. | Constants only. |

Each function also gets an **effect summary**, computed bottom-up over the
call graph (iterated to a fixed point inside recursive groups): for every
parameter, whether the function may read it, write its header, write its
buffer, change its length or capacity ("resize"), or free it. `quicksort`
writes its list's buffer but never resizes it; that single bit is what
lets the list pointer and length stay in registers across its calls.

### 3.5 Stops

A run-time stop is a `trap` terminator in a block of its own, reached by
an ordinary conditional branch. Trap blocks are cold and are laid out
after the function's last ordinary block.

The rule for moving checks (OPEN O1): a transformed program must print
the same output, stop with the same message and return the same exit code
as the original, on every input. So a check may be

- removed, when its condition is proved false;
- merged with or moved before another, only if no instruction with a
  visible effect (a `callrt` that prints or writes a file, a `store` that
  survives the stop in a way a later `print` would show, a different
  trap kind) lies between its old and its new place. Stores do not
  survive a stop, so in practice only output, file writes and other trap
  kinds block the move.

All index checks share one message, so checks on the same list inside a
loop body with no output in it can be merged freely.

### 3.6 Aggregates

Scalar replacement happens at lowering, not as a later repair:

- A local of struct, enum, `Option`, `Result`, list-header or
  string-header type becomes one SSA value per slot.
- It gets a frame object only if its address is needed: it is passed by
  reference to a call that is not inlined, or it is larger than 16 slots.
  A later pass (section 5.4) removes frame objects whose address stops
  being needed after inlining.
- Element access `xs[i].field` is `gep` plus `load` with region
  `buf(...)`; nothing is copied to the frame first.
- Enum tags are an `i64` slot value; `match` is a `switch` on it.

### 3.7 Runtime calls

`callrt` names a runtime routine (`alloc`, `realloc`, `free`, `write`,
`str_append`, `list_push_slow`, `clone`, `drop`, ...). A table in
`ir_rt.azl` gives each routine its effects in the terms of section 3.4,
whether it can stop the program, and whether it has visible output. The
runtimes themselves do not change for the IR; they change for section 7.

### 3.8 Lowering from `Program`

One function instance at a time, the same walk the generators do today
(`x64_gen.azl`, `arm64_gen.azl`), reading the same tables: resolved types
per node, call targets, method resolutions, the move/copy/append flags,
constant values, `PatInfo`. The slot layout and register choices the
checker made for the single-pass generators (`slots`, `reg_slots`,
`zero_slots`, `scratch_slot`) are ignored: the optimiser decides
placement itself.

Lowering also records, from the checker's borrow stack, the facts M4 to
M8 as region tags, and marks every copy the value semantics require as an
explicit `callrt clone` or slot-wise copy, with the last-use flag, so
that copy elision (section 5.11) sees them.

Variables are lowered with the standard "sealed block" SSA construction
(Braun et al.): no dominance frontiers, one pass.

### 3.9 Text form

A printer and a parser, for unit tests and for reading failures. `azol
build -O --emit-ir <stage> <path>` writes it after any pass.

```text
fn sum(p0: ptr [param 0 read]) -> i64 {
b0:
    v0 = load ptr, p0, 0  [param 0]
    v1 = load i64, p0, 8  [param 0]
    jump b1(0, 0)
b1(v2: i64, v3: i64):                 ; i, total
    v4 = slt v2, v1
    br v4, b2, b3
b2:
    v5 = gep v0, v2, 8, 0
    v6 = load i64, v5, 0  [buf(param 0)]
    v7 = add v3, v6
    v8 = add v2, 1
    jump b1(v8, v7)
b3:
    ret v3
}
```

A verifier (`ir_verify.azl`) checks after every pass in debug runs: each
value defined once and before use on every path, block arguments match
parameters in number and type, every block ends in one terminator,
region tags are well formed, no critical edge is left where the back end
forbids one, loads and stores have a region.

## 4. The pipeline

```text
Program (checked tree)
  -> lower (SSA, scalarised aggregates, regions)             ir_lower
  -> early: simplify, sroa, gvn, dce                          per function, callees first
  -> inline, accumulator recursion, tail calls                 call graph order
  -> effects (summaries)                                       bottom-up
  -> middle: range, gvn, licm, bce, ivsr, unroll, unswitch,
             ifconv, copy elision, string building, dse, dce   per function
  -> vectorise                                                 per loop
  -> late: simplify, layout
  -> select instructions (x86-64 | ARM64)
  -> allocate registers, place frame, emit bytes               existing encoders
  -> existing linkers (ELF, Mach-O), existing runtimes
```

The order is fixed; there is no pass manager and no iteration to a fixed
point except where a pass says so. Each pass states what it needs
(dominators, loops, ranges) and those are recomputed on demand and cached
until a pass that changes the graph runs.

## 5. The passes

Each is one file `compiler/opt/<name>.azl` with tests
`tests/opt/<name>/*.ir` (input and expected output in the text form).

### 5.1 `simplify`: folding, algebra, branches

Constant folding with azoL's exact rules (wrapping, shift counts masked to
six bits, truncating division); algebraic identities (`x + 0`, `x * 1`,
`x - x`, `x * 2^k` to a shift, `0 - x` to `neg`); copy propagation falls
out of SSA; a branch on a constant becomes a jump; a jump to a block with
one predecessor merges the blocks; a block parameter that receives the
same value on every edge is replaced by it. Division and remainder by a
constant become multiply-high and shifts (the default generator already
has the magic numbers). Runs several times; linear each time.

### 5.2 `inline`

Bottom-up over the call graph, so callees are optimised before they are
copied.

- Always: a callee with one call site; a callee of at most 12 IR
  instructions after its own early passes; library methods marked
  `inline` in the prelude (`len`, `is_empty`, the byte helpers).
- By cost: `benefit = 5 + 4 * (constant arguments) + 6 * (arguments whose
  address would stop escaping) + 10 if inside a loop`; `cost = callee
  instructions - 5`. Inline when `benefit >= cost` and the caller stays
  under 2,000 instructions.
- Never: across a recursive cycle, except one level of self-inlining when
  the callee is under 30 instructions (unrolls recursion once; not used
  for the gate numbers above).
- Generic instances are ordinary functions here.

Inlining is what turns by-reference parameters back into SSA values:
after it, `sroa` (5.4) runs again on the caller.

### 5.3 `accrec`: accumulator recursion, and `tailcall`

This is what `rustc -O` does to `fib`. In LLVM it is the
**TailRecursionElimination** pass, in its accumulator form
(`CanTransformAccumulatorRecursion`): a self call whose result is
combined with an associative and commutative operation just before the
return is treated as if it were a tail call, by carrying the rest of the
combination in an accumulator.

Ours, on the IR:

1. Find a block that ends `ret (op a, call self(args))` or `ret (op (call
   self(args)), a)`, where `op` is `add`, `mul`, `and`, `or` or `xor`, and
   nothing with a visible effect comes after the call.
2. Add a block parameter `acc` to the loop header that the function entry
   becomes, starting at the identity of `op` (0, 1, all ones, 0, 0).
3. Replace the pattern by `jump header(args, op acc a)`.
4. Every other `ret v` in the function becomes `ret (op acc v)`.

For `fib` the first call (`fib(n - 1)`) stays a call and the second
becomes the loop: half the calls, as measured in section 2. Owned
temporaries alive at the call are dropped before the jump; if any must
outlive it, the pattern does not match.

`tailcall`: a self call in tail position becomes a jump to the entry
block with the arguments as block arguments (`quicksort`'s second call).
A tail call to another function becomes a jump when the frames allow it
(no by-reference argument points into the caller's frame).

### 5.4 `sroa`: frame objects back to values

A frame object whose address is only used by `load` and `store` with
constant offsets is split into SSA values (Braun construction again). It
runs after lowering leaves an aggregate in the frame for a call that
`inline` later removes.

### 5.5 `gvn`: value numbering, loads included

Dominator-tree walk with a scoped hash table. Pure instructions with the
same operands get one value. A `load` is reused from an earlier `load` or
`store` of the same address when no operation that can write its region
lies between: this is where section 3.4 pays. In `quicksort` the swap
reuses `xs[i]` and `xs[j]` from the scan loops; a list's length loaded
before a call that does not resize the list is still valid after it.
Redundant checks disappear here too: a branch whose condition is already
known on this path (it dominates, or its negation led here) is folded.

### 5.6 `range`: what values can be

Interval analysis on `i64` values, with the loop conditions as sources
(`i < n` on the edge into the body), widening at loop headers, plus:

- induction variables: `i` starting at `a`, stepping by a constant, bounded
  by `i < n`, has range `[a, n - 1]` in the body and **final value**
  `max(a, n)` after the loop, exact when the step is 1;
- lengths: `len >= 0`; after `push`, `len` grows by one; a list that was
  empty before a loop that pushes once per round has length equal to the
  trip count afterwards (the `sieve` fill loop);
- `x % c` is in `(-c, c)`, or `[0, c)` when `x >= 0`; `x & m` is in `[0,
  m]` for `m >= 0`.

Ranges feed `bce`, the division peepholes (an operand known to be below
2^32 gets the short multiplication) and the choice of unsigned compares.

### 5.7 `licm`: loop-invariant code motion

Natural loops from dominators; a preheader is made for each. An
instruction moves to the preheader when its operands are defined outside
the loop and it cannot stop the program. A `load` moves when nothing in
the loop writes its region; with M2 to M9 that is decided by looking at
the regions of the loop's stores and the effect summaries of its calls,
not by pointer analysis. List pointer, length and capacity are the common
case (`particles`, `sort`). A check moves only under the rule of section
3.5.

### 5.8 `bce`: bounds checks

A check `ult i, len` is removed when `range` proves `0 <= i < len`. The
three shapes that matter:

- `while i < xs.len() { ... xs[i] ... }` with `i` not decreased and `xs`
  not resized in the loop;
- `for x in xs` and `for i, x in xs` (M6 makes this unconditional);
- a bound known from a final value, `j <= limit` with `len == limit + 1`.

Where the bound is a different variable (`while i < n { xs[i] }`), the
loop gets a single check `n <= len` in the preheader if the loop body has
no visible effect before the first access (so the stop happens at the
same point in the output), else the per-round check stays. This is the
place Rust's iterators win over indexed loops; `for` loops and the first
shape cover what idiomatic azoL writes.

### 5.9 `ivsr`, `unroll`, `unswitch`, `ifconv`, `vectorise`

**`ivsr`: induction variables.** `i * c` and `i * inv` with `i` stepping
by a constant become a second induction variable stepped by addition
(`loops`). An address `base + i * scale` used only for memory access
becomes a pointer that is stepped, and the loop test is rewritten to
compare that pointer with an end pointer when the counter has no other
use (`particles`).

**`unroll`.** Innermost loops only. Factor: the largest of 1, 2, 4 with
`body instructions * factor <= 48`; 8 for bodies of at most 4
instructions. A loop with a constant trip count of at most 8 is unrolled
completely. The remainder runs in the original loop. Loops containing a
call are not unrolled.

**`unswitch`.** A loop-invariant branch inside a loop of at most 32
instructions is moved out by duplicating the loop once. At most one
level.

**`ifconv`.** A diamond or triangle whose arms are at most three pure
instructions with no load or store becomes `select` (`cmov`/`csel`), when
the branch is not marked predictable (a loop exit or a trap edge is
predictable). `x < 0 || x > C` with `C >= 0` becomes `ugt x, C` in
`simplify`.

**`vectorise`.** Scope, stated exactly:

- Loop shape: innermost, counted, unit stride, no call, no stop other than
  bounds checks already removed, one exit.
- Element types: `i64` (2 lanes with SSE2 or NEON, 4 with AVX2), bytes of
  strings and packed `bool` (16 or 32 lanes), later `f64` (2 or 4 lanes)
  and the narrow integers.
- Bodies: element-wise `add sub and or xor`, compares, `select`, shifts by
  a loop-invariant amount. A 64-bit multiply has no vector instruction in
  SSE2, AVX2 or NEON, so a loop that multiplies `i64` elements is
  unrolled, not vectorised. Stores to one list and loads from others
  (M2 proves no overlap without a run-time test, which a C or Rust
  compiler often needs).
- Reductions: sum, bitwise and/or/xor, count of matches, any/all,
  position of the first match.
- Idioms replaced by runtime routines written with the same instructions:
  fill (`memset`), copy (`memcpy`), compare (`memcmp`), find a byte.
- Targets: x86-64 gets an SSE2 version and an AVX2 version of each
  vector loop; a flag set once at start-up by `cpuid` picks one with a
  single branch before the loop (OPEN O2). ARM64 uses NEON, which every
  ARM64 processor has.
- Left out of the first version: gathers and scatters, strides other than
  one, loops with early exits other than "first match", outer loops,
  SLP (straight-line) vectorisation, AVX-512 and SVE. Each is a later
  wave if a gate program needs it (section 8.4).

### 5.10 `dse`, `dce`

Dead stores: a store overwritten before any possible read of its region,
or to a frame object never read again. Dead code: unused pure
instructions, unreachable blocks, unused block parameters. Both linear.

### 5.11 `copyelide`: the copies value semantics imply

Where azoL can beat Rust rather than match it: Rust code clones
explicitly and the compiler must respect it; azoL copies are the
compiler's, so it may drop any copy nobody can observe.

| Case | Transformation |
|---|---|
| `var b = a` and `a` is not used again (M10) | Move: `b` takes `a`'s slots; no `clone`, no `drop`. The checker already marks these; the optimiser finds the ones that appear after inlining. |
| `var b = a`, then neither is written before one dies | Share until first write: both names use the same buffer; the copy is made only on a path that writes while the other is still alive. No reference counts: liveness decides at compile time. |
| A by-value argument the callee only reads | Passed by reference (the checker does this for aggregates already; here it extends to arguments that become read-only after optimisation). |
| A returned aggregate built in a local | Built directly in the caller's space (`ret` region). |
| `xs = f(xs)` where `f` returns its parameter changed | In-place: `f` gets `xs` as `mut`, when the effect summary shows the result is the parameter's buffer. |
| `clone` followed by `drop` of the original with no use between | Both removed. |
| A buffer that does not outlive the function, of size known and at most 256 bytes | Allocated in the frame (`alloca` object), no `alloc`/`free`. |

### 5.12 `strbuild`

`s = s + x` in a loop is already an in-place append. This pass adds: the
capacity check and the copy of a short literal inline (speed.md A6);
several appends in a row reserve once; `str(n)` appended directly writes
its digits into the destination instead of making a temporary string;
`a + b + c` allocates once for the total length.

### 5.13 `peephole`, on selected instructions

After instruction selection: compare with zero after an instruction that
set the flags; `lea` for add-and-shift; remainder compared with a
constant (`x % c == r` as `c * (x / c) == x - r`, used in `loops`);
load folded into a compare or an arithmetic instruction when the loaded
value has one use; redundant moves; a jump to the next block.

### 5.14 `layout`

Blocks in reverse post-order; loops rotated so the test is at the bottom;
the more likely successor falls through (loop back edges and non-trap
edges are likely; `if` arms are equal unless one ends in `return`, `trap`
or `continue`); trap blocks and blocks reached only from unlikely edges
go after the function's last ordinary block.

## 6. From IR to bytes

### 6.1 Register allocation

**Linear scan on SSA with interval splitting** (the Wimmer and Franz
form): live intervals with holes from a single backward walk; intervals
are split before a call or where no register is free, the split part
goes to a frame slot; moves for block arguments are resolved on edges
with parallel-move sequencing. Not graph colouring: it is several times
slower to build and the experiments' kernels need nothing it gives.
Allocation hints (same register for a block argument and its parameter,
for a call argument and its convention register) remove most moves.

### 6.2 The internal calling convention

All azoL code of a program is compiled together, so calls between azoL
functions need not follow the platform convention. Runtime calls and,
later, `extern` calls do.

- Arguments: the first 8 integer slots in registers, the rest on the
  stack. An aggregate of at most 4 slots is passed and returned in
  registers, slot by slot; larger ones by reference as today.
- Results: up to 4 slots in registers.
- Saved registers: decided per function. A leaf function uses only
  caller-saved registers and saves nothing. For other functions the
  allocator prefers caller-saved registers for values that do not live
  across a call. A bottom-up summary of which registers each function
  (and its callees) really changes lets a caller keep values in
  registers its callee does not touch; inside recursive groups the
  platform's split is used.
- No frame pointer unless the function has a variable-size frame. The
  stack pointer is aligned only before a `callrt`.
- The guard-page stack overflow detection keeps working: frames above
  one page touch each page as they grow.

### 6.3 Instruction selection

Greedy bottom-up matching over the SSA of each block, largest pattern
first, folding a value into its user when it has exactly one use in the
same block (so a `load` becomes a memory operand, a `gep` an addressing
mode, a compare the condition of the branch after it).

| | x86-64 | ARM64 |
|---|---|---|
| Addressing | `[base + index * {1,2,4,8} + disp32]` | `[base, index, lsl #k]`, `[base, #imm]`, pre- and post-index for stepped pointers |
| Flags | `add`/`sub`/`and` set flags for the next branch; `test` | `subs`/`adds`/`ands`; `cbz`/`cbnz`/`tbz`/`tbnz` |
| Select | `cmov`, `setcc` | `csel`, `cset`, `csinc`, `cneg` |
| Multiply-add | `lea` | `madd`, `msub`, shifted operands |
| Pairs | | `ldp`/`stp` for adjacent slots |
| Vectors | SSE2 and AVX2 (VEX) | NEON |

Branch relaxation: x86-64 jumps start short and are widened until
nothing changes; ARM64 conditional branches beyond 1 MB get an inverted
branch over an unconditional one.

### 6.4 What is reused unchanged

The byte-level encoders in `x64.azl` and `arm64.azl` (new encodings are
added: `cmov`, `setcc`, `umulh`, SSE2/AVX2/NEON), the relocation and
import machinery, `elf.azl`, `arm64_link.azl`, `macho.azl`, `sha256.azl`,
the three runtimes, the command line. The tree-walking generators
`x64_gen.azl` and `arm64_gen.azl` stay as the default mode and are not
touched by the optimiser.

## 7. Representation and runtime changes (both modes)

These change what both the default generator and the optimiser emit, so
they land first (wave 0) and go through the existing byte-identity and
emulator checks.

### 7.1 Element size per list type

`List[bool]` stores one byte per element; later `List[u8]`, `List[i32]`
and so on store their width. Measured effect on `sieve`: D1 misses 38.1 M
to 19.5 M, equal to Rust, and 2.8 times faster in the byte-string
experiment of [speed.md](speed.md). Needs an element size in the list
type in the checker, in both generators, and in the runtime routines that
grow, clone and drop lists.

### 7.2 Packed structs (OPEN O3)

`bool` fields and enum tags take one byte; fields are ordered by size.
`Tree` nodes in the `tree` benchmark shrink from 32 to 24 bytes, matching
Rust; its D1 misses are 17.4 M against 9.6 M today.

### 7.3 Allocator

`tree` spends 21% of its instructions in `rt_alloc` and `rt_free`
([speed.md](speed.md)). Plan: size classes of 16 bytes up to 256, one
free list per class, popped and pushed inline by the optimiser's code
(four instructions on the fast path); larger blocks as today. Growth of
lists and strings stays doubling. With threads
([threads.md](threads.md)) the free lists are per thread, which that
design already requires.

### 7.4 Not doing

Small strings stored inline: not measured, and every string path would
pay a branch. Revisit only if a gate program is lost to it.

## 8. The gate: what "as fast as `rustc -O`" means

The claim will be made in public, so it is defined by a suite and a rule
that anyone can rerun (`python3 bench/compare.py --gate`).

**Rule (OPEN O4).** For every program, time = best of 10 runs of the whole
process. Ratio = azoL `-O` time / `rustc -O` time. The gate passes when the
geometric mean of the ratios is at most 1.00 and no ratio exceeds 1.10,
on the Apple M5 (macOS) and on an x86-64 Linux machine. Results against
`rustc -C opt-level=3 -C target-cpu=native` are printed in a second
column and published with the first, pass or fail. Instruction counts
(callgrind) are printed for x86-64 as the noise-free check. Every pair of
programs must print the same output, or the run is void.

### 8.1 The suite

Each program exists as `bench/<name>.azl` and an idiomatic
`bench/<name>.rs` (iterators and slices where a Rust programmer would use
them, not a transliteration), reviewed so that neither side is
handicapped. Inputs are generated in the program; sizes are chosen for
0.1 to 1 second.

| # | Program | What it stresses | Needs |
|---|---|---|---|
| 1 | `fib40` | calls, recursion | `accrec` |
| 2 | `loops` | integer arithmetic in nested loops | `ivsr`, range, peepholes |
| 3 | `sort` | hand-written quicksort | `tailcall`, `gvn`, effects |
| 4 | `sort_lib` | the library's `sorted()` against `sort_unstable()` | a good library sort (pattern-defeating quicksort) |
| 5 | `sieve` | flags in a list | packed `bool`, `bce` |
| 6 | `particles` | array of structs updated in place | `licm`, `ivsr`, `unroll` |
| 7 | `soa` | struct of arrays, element-wise arithmetic | `vectorise` |
| 8 | `tree` | allocate and walk binary trees | allocator, packed structs |
| 9 | `binary_trees` | the classic allocation benchmark | allocator |
| 10 | `append` | building long strings | `strbuild` |
| 11 | `words` | split, join, count | library, `copyelide` |
| 12 | `wordfreq` | map from text to count | `Map` hashing and probing |
| 13 | `map_int` | map with integer keys, insert/lookup/remove | `Map[K, V]` ([traits.md](traits.md)) |
| 14 | `lru` | cache with a map and an order list | `copyelide`, map |
| 15 | `matmul` | 200 x 200 integer matrix multiply | `licm`, `ivsr`, `bce`, loop order kept |
| 16 | `prefix` | prefix sums, running maximum | `vectorise` (reductions), `unroll` |
| 17 | `bits` | population count, bit reversal, xorshift | peepholes, `popcnt`/`cnt` selection |
| 18 | `crc32` | table-driven checksum over 64 MB | byte loads, `bce` |
| 19 | `sha256` | block hashing | rotates, register pressure |
| 20 | `dp_lcs` | longest common subsequence table | `bce`, two-row reuse |
| 21 | `knapsack` | one-dimensional dynamic programming | `ifconv`, `vectorise` candidate |
| 22 | `bfs` | breadth-first search on a grid graph | queues, `bce` |
| 23 | `dijkstra` | binary-heap priority queue | sift loops, struct moves |
| 24 | `fannkuch` | permutations, reversals | small-array loops, `unroll` |
| 25 | `bignum` | 2,000-digit factorial with a list of limbs | carry loops, `umulh` |
| 26 | `json` | parse and re-serialise a 5 MB document | byte loops, `match`, recursion |
| 27 | `matcher` | glob/regex-like matching with backtracking | byte compares, calls |
| 28 | `tokenizer` | the azoL lexer on 132,000 lines | the compiler's own hot path |
| 29 | `nbody` | floating-point simulation | `f64` ([floats.md](floats.md)) |
| 30 | `spectral` | spectral norm | `f64`, `vectorise` |
| 31 | `mandelbrot` | escape-time iteration | `f64`, `ifconv` |

Programs 29 to 31 join the gate when `f64` exists; until then the gate is
the first 28 and the public statement says so.

### 8.2 Where azoL's rules help

| Property | How the design uses it |
|---|---|
| No aliasing between variables (M1 to M4) | Loads stay in registers across stores to other lists; vector loops need no overlap test. Rust has the same guarantee for `&mut`, but LLVM uses only part of it. |
| Precise call effects (M9) | A call does not force reloading anything it cannot write. |
| No unwinding | No landing pads, no cleanup paths; a stop ends the process. Every call is a plain call. |
| Whole program | Custom calling convention, per-function saved registers, effect summaries, inlining across what would be crate boundaries. |
| Copies are the compiler's | `copyelide` removes copies a Rust programmer would have to write out as moves or borrows. |
| Wrapping arithmetic | No overflow checks to remove. (`rustc -O` has none either.) |

### 8.3 Where they cost, and what removes the cost

| Cost | Remedy in this design |
|---|---|
| Indexed loops check bounds where Rust iterators do not | `range` + `bce` for the three common shapes; `for` loops need no check at all. |
| Value-semantics copies | `copyelide`; last-use moves; in-place update. |
| 8-byte `bool` and tags | Packed lists and structs (section 7). |
| Only 64-bit integers | Narrow element storage arrives with the narrow integer types; until then `crc32`, `sha256` use `i64` with masks, which `simplify` and the selectors reduce to 32-bit instructions when the range proves it. |
| Aggregates passed by reference | Up to 4 slots in registers (section 6.2). |
| Every allocation is a call | Size-class fast path inline (section 7.3); frame allocation of short-lived buffers. |

### 8.4 The hardest programs, and what winning each takes

| Program | Why it is hard | What it takes |
|---|---|---|
| `soa`, `prefix`, `knapsack` | LLVM vectorises them; scalar code loses 2 to 4 times | `vectorise` as specified, with AVX2 on x86-64. Against `target-cpu=native` on an AVX-512 machine the 1.10 bound may not hold; AVX-512 is then one more instruction set in the same pass. |
| `matmul` | LLVM unrolls and vectorises the inner product; a 64-bit multiply has no vector form below AVX-512, so both sides stay scalar and unrolled | `ivsr`, `unroll` by 4, `bce` from final values. |
| `sort_lib` | Rust's standard sort is heavily tuned | The library sort must be pattern-defeating quicksort with insertion sort for small ranges; this is library work in azoL, counted in wave 5. |
| `wordfreq`, `map_int`, `lru` | Rust's `HashMap` is SwissTable with SIMD probing | `Map[K, V]` with the same layout (control bytes, 16-wide probe written with the vector instructions), and a hash as cheap as Rust's default is slow: SipHash costs Rust here, so a good azoL hash can win. |
| `binary_trees`, `tree` | Allocation speed | Size-class allocator with an inline fast path; packed nodes. |
| `json`, `tokenizer`, `matcher` | Branchy byte code; register allocation and layout quality decide | Linear scan with hints, `layout`, load folding; these are the programs that expose a weak allocator, so they are in the gate on purpose. |
| `nbody`, `spectral`, `mandelbrot` | Floating-point code generation does not exist yet | The `f64` work of floats.md, plus `vectorise` for `spectral`. |
| `fib40` | Solved in the experiment (0.70). | |

If a gate program is lost to a capability this design leaves out (a loop
shape the vectoriser does not handle, AVX-512, an interprocedural
optimisation), that capability becomes the next wave. The suite decides
what is built; nothing is declared out of reach in advance.

## 9. Proof that `-O` is right

### 9.1 Differential testing

For every program P: `default(P)`, `-O(P)` and the fuzzer's own oracle
must agree on output, standard error and exit code, on all three targets
(x86-64 natively, both ARM64 targets in the checking emulators, which
also catch leaks, double frees and writes outside a block).

- The existing suites (`tests/programs`, `tests/errors`, the format and
  cross sections of `test.sh`) run a second time with `-O`.
- `tests/fuzz/fuzz.py --opt` generates programs shaped for the optimiser:
  loops with every kind of induction variable, checks that must stay and
  checks that may go, accumulator recursion with every operator, copies
  that may and may not be elided (a write after the copy on one path
  only), calls with every effect combination, stops in the middle of
  loops with output before them (the rule of section 3.5).
- A flag `--opt-stop-after <pass>` and `--opt-skip <pass>` lets the
  reducer find the pass that broke a program by bisection.

### 9.2 The verifier and per-pass tests

`ir_verify` after every pass in debug runs (`-O --verify`, always on in
the test suite). Each pass has input/expected pairs in the text form, and
for each pair a third file with concrete inputs: an IR interpreter
(`compiler/opt/ir_interp.azl`, about 500 lines) runs input and output
functions on them and compares results and memory effects. That is
translation validation at unit-test scale: cheap, and it catches a wrong
expected file as well as a wrong pass.

### 9.3 Checks that only the optimiser needs

- Every removed bounds check is recorded with the fact that removed it;
  `-O --keep-checks` keeps them as `trap internal` and the fuzzer runs in
  that mode: a removed check that could have fired stops the program
  with a distinct message.
- Every elided copy likewise: `-O --check-copies` keeps a hash of the
  source buffer and verifies at the copy's death that the other name was
  never written.

### 9.4 Bootstrap

Stage A: the default compiler builds the compiler. Stage B: stage A
builds the compiler with `-O`. Stage C: stage B builds the compiler with
`-O`. B and C must be the same file, and B's output for every test
program, in both modes, must be byte-identical to A's. Then the seed
compilers are built with `-O` (OPEN O5).

### 9.5 The benchmark gate

Section 8, run on the user's Mac and on x86-64 before any public
statement, and in `test.sh` afterwards with a looser bound so that a
regression is noticed.

## 10. Compile speed, memory, size

All three are estimates; none is measured.

- **Speed.** The default build does about 6.9 million lines per second on
  the M5 (44,004 lines in 6.4 ms, measured). `rustc -O` on the same
  program takes 9.5 s, about 4,600 lines per second (measured). `-O`
  walks each function's IR about 25 times (passes, selection,
  allocation), each walk comparable to the default generator's single
  walk, plus the range and value-numbering tables. Target: **at least
  300,000 lines per second**, about 20 times slower than the default
  build and about 60 times faster than `rustc -O`. The gate script
  prints it; under 150,000 is treated as a bug.
- **Memory.** One function's IR at a time, plus the stored bodies of
  inlinable functions (at most 12 instructions each) and the effect
  summaries: a few megabytes for the 132,000-line program.
- **Code size.** Smaller than the default build for scalar code (values
  in registers, shared trap blocks); larger where loops are unrolled or
  vectorised twice. Expect within 20% either way.

`-O` is deterministic: the same source gives the same executable, as in
the default mode.

## 11. Implementation plan

Six waves. Inside a wave every agent owns its files and no two agents
edit the same one. Line counts are estimates.

| Wave | Piece | Files | Lines |
|---|---|---|---|
| 0 | Profile `tree`, `words`, `append` on the Mac (`sample`), since their gap exists only there | none | |
| 0 | Packed lists (7.1), both modes | checker, both generators, runtimes | 900 |
| 0 | Packed structs (7.2), both modes | checker, both generators | 600 |
| 0 | Size-class allocator (7.3) | three runtimes | 700 |
| 0 | The suite: 28 programs in azoL and Rust, `compare.py --gate` | `bench/` | 3,500 azoL + Rust |
| 1 | IR data structures, builder, printer, parser | `opt/ir.azl`, `ir_text.azl` | 1,400 |
| 1 | Verifier, interpreter | `opt/ir_verify.azl`, `ir_interp.azl` | 900 |
| 1 | Lowering from `Program` | `opt/ir_lower.azl` | 2,600 |
| 1 | Dominators, loops, liveness | `opt/ir_cfg.azl` | 700 |
| 1 | Linear scan | `opt/regalloc.azl` | 1,500 |
| 1 | Selector and emitter, x86-64 | `opt/sel_x64.azl` | 2,200 |
| 1 | Selector and emitter, ARM64 | `opt/sel_arm64.azl` | 2,200 |
| 1 | `-O` in the driver, `--emit-ir`, fuzzer `--opt` | `self_main.azl`, `tests/fuzz` | 500 |
| 2 | `simplify`, `dce`, `dse` | one file each | 1,100 |
| 2 | `sroa`, `gvn` | one file each | 900 |
| 2 | `inline`, `effects` | one file each | 900 |
| 2 | `accrec`, `tailcall` | one file | 350 |
| 2 | `layout`, `peephole` | one file each | 600 |
| 3 | `range` | one file | 900 |
| 3 | `licm`, `bce` | one file each | 800 |
| 3 | `ivsr`, `unroll`, `unswitch`, `ifconv` | one file each | 1,300 |
| 4 | `copyelide`, `strbuild` | one file each | 1,000 |
| 4 | Internal calling convention, saved-register summaries | `regalloc`, selectors | 600 |
| 4 | Inline allocator fast path | selectors, `ir_rt.azl` | 300 |
| 5 | `vectorise` with SSE2, AVX2, NEON encodings and the `cpuid` flag | `opt/vectorise.azl`, encoders, runtimes | 2,400 |
| 5 | Library work the gate needs: sort, `Map` layout, hash | `prelude` | 1,200 |
| 6 | Gate on both machines; whatever a lost program needs | | open |

Total: about 26,500 lines of azoL plus the benchmark suite, against
20,400 lines for the whole compiler today.

Wave 1 is the first point where `-O` produces a program; with no
optimisation passes it should already match the default mode's speed or
beat it, because values live in registers. Waves 2 to 5 each run the
whole proof plan before merging.

### Predicted instruction ratios to `rustc -O` on x86-64, per wave

Bold numbers are measured in the experiments of section 2; the others are
computed from them or from [speed.md](speed.md) and are predictions.

| After | `fib40` | `loops` | `sort` | `sieve` | `particles` | `tree` | `append` | `words` |
|---|---|---|---|---|---|---|---|---|
| Today (default mode) | 1.38 | 1.15 | 1.20 | 1.09 | 1.47 | 0.85 | 1.41 | 0.86 |
| Wave 0 | 1.38 | 1.15 | 1.20 | 1.09, memory equal | 1.47 | about 0.75 | 1.41 | 0.86 |
| Wave 1 (registers only) | about 1.2 | about 1.05 | about 1.1 | about 1.0 | about 1.2 | | | |
| Wave 2 | **0.70** | about 1.0 | about 1.0 | about 1.0 | about 1.1 | | | |
| Wave 3 | 0.70 | **0.77** | **0.95** | **0.67** | **1.00, 0.90 unrolled** | | | |
| Wave 4 | | | | | | about 0.7 | about 0.7 | about 0.8 |

Instruction ratio is not time. Section 2's wall times agree with the
direction for all five; the gate is defined on time.

## 12. Risks

| Risk | Consequence | Guard |
|---|---|---|
| A wrong region tag or effect summary | A load is reused across a store that changed it: silently wrong results | Regions come only from lowering, with one test per fact M1 to M11; `--keep-checks` and `--check-copies` modes in the fuzzer; the interpreter-based pass tests. |
| A removed bounds check that could fire | Memory safety lost | Every removal is recorded with its proof; `--keep-checks`; the checking emulators. This is the one failure that would undo the language's promise, so `bce` is the pass with the most tests. |
| `copyelide` share-until-write misses a write | Two names see one buffer change | The pass is conservative (any call that might write counts as a write); `--check-copies`. |
| Lowering is 2,600 lines duplicating what two generators do | Third copy of the language's semantics | Differential testing against the default mode on every program in the repository and every fuzzer program, from wave 1. |
| Linear scan quality on branchy code | `json`, `tokenizer`, `matcher` miss the 1.10 bound | They are in the gate from the start; fallback is interval splitting at loop boundaries and, if needed, a second-chance pass. |
| The ARM64 results differ from x86-64 | The gate passes on one machine only | The experiments are x86-64 only; wave 0 profiles the Mac first, and every wave's numbers are taken on both. |
| The unexplained M5 gap in `tree` and `words` is not code generation | Passes do not close it | Wave 0 profiling decides before any optimiser code is written; the allocator and packed structs are the suspects. |
| `-O` compile speed | Under the 300,000 lines per second target | Measured from wave 1; passes are linear by design. |
| Size: 26,500 lines | Long work, many agents | Waves keep everything green; `-O` is optional until the gate passes, so the default compiler is never at risk. |

## Appendix: the hand-written kernels

x86-64, GNU assembler, Intel syntax. These are the measured programs of
section 2. Scratch copies were built in `/tmp/az/optdesign` (`k3.s`, driver
`d.c`: `gcc -O1 -o hand d.c k3.s`, then `valgrind --tool=callgrind
--toggle-collect=run_<name> ./hand <name>`).

```asm
# loops: ivsr (p += i), range (p < 2^32), remainder-compare peephole, layout
loops_kernel:                       # rdi = size
    xor r8d, r8d                    # total
    xor esi, esi                    # i
    movabs r10, 4908534053          # ceil(2^35 / 7)
    movabs r11, 3123612579          # ceil(2^35 / 11)
    test rdi, rdi
    jle .Ldone
.Li:
    xor ecx, ecx                    # j
    xor r9d, r9d                    # p = i * j
.Lj:
    mov rax, r9
    imul rax, r10
    shr rax, 35                     # p / 7
    imul rax, rax, 7
    lea rdx, [r9 - 3]
    cmp rax, rdx                    # p % 7 == 3
    je .Lrare
    inc r8
.Lnext:
    add r9, rsi
    inc rcx
    cmp rcx, rdi
    jl .Lj
    inc rsi
    cmp rsi, rdi
    jl .Li
.Ldone:
    mov rax, r8
    ret
.Lrare:
    mov rax, r9
    imul rax, r11
    shr rax, 35
    imul rax, rax, 11
    mov rdx, r9
    sub rdx, rax
    add r8, rdx
    jmp .Lnext

# fib: accrec, base case before the frame
fib_kernel:                         # rdi = n
    cmp rdi, 2
    jl .Lleaf
    push rbx
    push r14
    xor ebx, ebx                    # acc
    mov r14, rdi
.Lloop:
    lea rdi, [r14 - 1]
    call fib_kernel
    add rbx, rax
    sub r14, 2
    cmp r14, 2
    jge .Lloop
    lea rax, [rbx + r14]
    pop r14
    pop rbx
    ret
.Lleaf:
    mov rax, rdi
    ret

# quicksort: tailcall, effects (no resize) + licm of the header, load folding
qs_kernel:                          # rdi = header {ptr, len}, rsi = low, rdx = high
    cmp rsi, rdx
    jge .Lret
    push rbx
    push rbp
    push r12
    push r13
    push r14
    push r15
    sub rsp, 8
    mov r12, rdi
    mov r13, rdx                    # high
.Ltop:
    mov r15, [r12]                  # base
    mov rbp, [r12 + 8]              # len
    lea rax, [rsi + r13]
    mov rcx, rax
    shr rcx, 63
    add rax, rcx
    sar rax, 1
    cmp rax, rbp
    jae .Ltrap
    mov r8, [r15 + rax*8]           # pivot
    mov rbx, rsi                    # i
    mov r14, r13                    # j
    mov r9, rsi                     # low
.Louter:
    cmp rbx, r14
    jg .Lsplit
    jmp .Lscan_i
.Linc:
    inc rbx
.Lscan_i:
    cmp rbx, rbp
    jae .Ltrap
    cmp [r15 + rbx*8], r8
    jl .Linc
    jmp .Lscan_j
.Ldec:
    dec r14
.Lscan_j:
    cmp r14, rbp
    jae .Ltrap
    cmp [r15 + r14*8], r8
    jg .Ldec
    cmp rbx, r14
    jg .Lsplit
    mov rax, [r15 + rbx*8]
    mov rcx, [r15 + r14*8]
    mov [r15 + rbx*8], rcx
    mov [r15 + r14*8], rax
    inc rbx
    dec r14
    jmp .Louter
.Lsplit:
    mov rdi, r12
    mov rsi, r9
    mov rdx, r14
    call qs_kernel
    mov rsi, rbx                    # low = i, high unchanged: the tail call
    cmp rsi, r13
    jl .Ltop
    add rsp, 8
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbp
    pop rbx
.Lret:
    ret
.Ltrap:
    ud2

# particles step, not unrolled: licm, ivsr (pointer), unsigned range test
step_kernel:                        # rdi = header {ptr, len}
    mov rcx, [rdi + 8]
    mov r8, [rdi]
    test rcx, rcx
    jle .Lsret
    shl rcx, 5
    add rcx, r8                     # end
.Lstep:
    mov rax, [r8]
    mov r9, [r8 + 16]
    add rax, r9                     # x
    mov rdx, [r8 + 8]
    mov r10, [r8 + 24]
    add rdx, r10                    # y
    cmp rax, 100000
    ja .Lfix_x
.Lx:
    cmp rdx, 60000
    ja .Lfix_y
.Ly:
    mov [r8], rax
    mov [r8 + 8], rdx
    add r8, 32
    cmp r8, rcx
    jb .Lstep
.Lsret:
    ret
.Lfix_x:
    neg r9
    mov [r8 + 16], r9
    lea rax, [rax + r9*2]
    jmp .Lx
.Lfix_y:
    neg r10
    mov [r8 + 24], r10
    lea rdx, [rdx + r10*2]
    jmp .Ly
# The unrolled form repeats the body for [r8 + 32 ...] and steps r8 by 64,
# with one round of the loop above for an odd count.

# sieve: packed bool, final value of len, bce
sieve_kernel:                       # rdi = limit
    push rbx
    push r12
    push r13
    push r14
    push r15
    mov r12, rdi                    # limit
    xor r13d, r13d                  # base
    xor r14d, r14d                  # len
    xor r15d, r15d                  # cap
    xor ebx, ebx                    # n
.Lfill:
    cmp r14, r15
    je .Lgrow
.Lstore:
    mov byte ptr [r13 + r14], 0
    inc r14
    inc rbx
    cmp rbx, r12
    jle .Lfill
    xor eax, eax                    # count
    xor edx, edx                    # last
    mov ecx, 2                      # i
    cmp rcx, r12
    jg .Lout
.Lsi:
    cmp byte ptr [r13 + rcx], 0
    jne .Lsnext
    inc rax
    mov rdx, rcx
    mov rsi, rcx
    imul rsi, rcx                   # j = i * i
    cmp rsi, r12
    jg .Lsnext
.Lsj:
    mov byte ptr [r13 + rsi], 1
    add rsi, rcx
    cmp rsi, r12
    jle .Lsj
.Lsnext:
    inc rcx
    cmp rcx, r12
    jle .Lsi
.Lout:
    imul rax, rax, 100000000
    add rax, rdx
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    ret
.Lgrow:                             # capacity doubles, from 16
    lea rsi, [r15 + r15]
    mov eax, 16
    cmp rsi, 16
    cmovb rsi, rax
    mov r15, rsi
    mov rdi, r13
    call realloc
    mov r13, rax
    jmp .Lstore
```
