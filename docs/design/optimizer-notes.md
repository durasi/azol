# Notes on the optimiser as built

[optimizer.md](optimizer.md) is the design. This page records what
exists, where it differs from the design and why, and how to write a
pass on top of it. Keep it current: a pass that changes a rule below
changes this page in the same commit.

## What exists (wave 1, first half)

| File | What it is |
|---|---|
| `compiler/opt/ir.azl` | The intermediate form: data layout, operations, regions, the table of runtime routines, the builder, operands by position, uses, replacing a value, removing instructions, blocks and block parameters, splitting an edge. |
| `compiler/opt/ir_text.azl` | The text form: printer and parser. |
| `compiler/opt/ir_cfg.azl` | Predecessors, reverse postorder, dominators, loops, liveness. |
| `compiler/opt/ir_verify.azl` | The verifier. |
| `compiler/opt/ir_interp.azl` | The interpreter, the reference a pass is tested against. |
| `compiler/opt/ir_tool_main.azl` | `ir-tool print|verify|run|cfg|time <file.ir>`. |
| `compiler/opt/example_pass.azl`, `example_pass_main.azl` | The example pass of this page, and a tool that runs it. |
| `compiler/opt/ir_builder_test_main.azl` | A test of the builder and of the changes passes make. |
| `tests/opt/` | 114 hand-written `.ir` files, `passes/`, `builder.expected`, and `run_opt_tests.py`. |

Not yet: lowering from `Program`, the register allocator, the
instruction selectors, `-O` in the driver, any real pass.

Run the tests with `python3 tests/opt/run_opt_tests.py` (about 40
seconds; `--random 0` skips the random functions and takes 3).

## Where this differs from optimizer.md

1. **Constants are operands.** The design's text form writes `add v2, 1`
   and `jump b1(0, 0)`, so an operand is a value or a constant. In
   memory a value is its number (0 or more) and a constant is
   `-(k + 2)` for entry `k` of the function's `consts` list; -1 is "no
   operand". `iconst` and `bconst` still exist, for a constant that
   must be a value of its own.
2. **Additions to the text form** (section 3.9 has no example that
   needs them):
   - `data "..."`, once per file: the read-only data that
     `addr @data+N` points into. Escapes: `\"`, `\\`, `\xHH`.
   - `    frames 16, 8` as the first line of a function: the sizes in
     bytes of frame objects 0, 1, ...
   - A call names the types of its results, because the callee may be
     in another file: `v1, v2 = call f(v0) -> i64, ptr`. A `callrt`
     does not: the table of routines has them.
   - `switch v0, 1 -> b1, 2 -> b2(v3), default b3`.
   - The note of a `ptr` parameter lists its effects as words:
     `p0: ptr [param 0 read write writebuf resize free]`.
   - A comment runs from `;` to the end of the line. The printer writes
     none.
3. **No vector instructions, `f64`, `v128` or `v256` yet.** They arrive
   with the vectoriser and with floats; nothing in waves 1 to 4 needs
   them, and adding an operation is one constant, one name and one row
   in the verifier and the interpreter.
4. **The table of runtime routines is in `ir.azl`**, not in a file
   `ir_rt.azl`, and has eleven routines: `alloc`, `realloc`, `free`,
   `memcpy`, `print_i64`, `print_bool`, `print_str`, `eprint_str`,
   `str_eq`, `str_append`, `exit`. `list_push_slow`, `clone` and `drop`
   are not there: what they take depends on how the lowering stage
   describes an element type, which is that stage's decision. To add a
   routine: a constant `IR_RT_*`, its name, its argument and result
   letters, its three properties, and its case in `irm_callrt`.
   `str_append(p, len, cap, q, n) -> ptr, i64, i64` works on a string's
   three slots, since a header is scalarised; `cap` 0 means the bytes
   are not the string's own (a literal).
5. **`bool` in memory is eight bytes**, like the default generators
   store it today. A packed `bool` (design section 7.1) is a `load u8`
   or `store u8`.
6. **Type rules the design leaves open.** `and`, `or`, `xor` take two
   `i64` or two `bool` and give what they take. `eq` and `ne` take two
   of the same type, `ptr` and `bool` included. The unsigned orderings
   also take two `ptr`. Addresses move only by `gep`; `add` on a `ptr`
   is an error. A constant fits `i64`; 0 and 1 fit `bool`; 0 fits `ptr`
   (the address of nothing).
7. **A `trap internal`** exists for section 9.3 (`-O --keep-checks`).
8. **The verifier does not check critical edges.** The design says "no
   critical edge is left where the back end forbids one"; no back end
   has stated a rule yet. `ir_split_edge` is ready for it. It also adds
   two rules the design implies: a `trap` block has no parameters, and
   a parameter's effects limit what is done through it (`read` to load,
   `write` to store to what it points to, `writebuf` to store to a
   buffer it owns). `resize` and `free` are kept and printed but not
   checked: they speak about runtime calls, which is the `effects`
   pass's subject.
9. **Region overlap is "the same region".** `ir_regions_overlap` gives
   true only for one and the same region, which is facts M1, M2, M3 and
   M11 as they stand. M4 to M8 are the lowering stage's to encode by
   its choice of regions.
10. **The interpreter is about 720 lines**, not 500, because it is
    strict (see below) and knows the runtime routines. Calls of the
    function being interpreted nest in the interpreter itself, so it
    stops at 4,000 nested calls; `tests/programs/deep_recursion.azl`
    cannot be run through it.
11. **A module** (`IrModule`) holds several functions and the data, for
    the tool and for calls between functions in tests. A pass still
    takes one `IrFunc`.

## The data layout in one page

Everything is a number that indexes parallel lists of an `IrFunc`.

- **Values.** Parameters of the function are values `0` to
  `param_count - 1` (`p0`, `p1` in text); every other value
  `param_count + N` is `vN`. `val_ty[v]` is its type or -1 when the
  number is not in use. `val_ins[v]` is the instruction that defines
  it, `val_blk[v]` the block it is a parameter of; both -1 for a
  parameter of the function.
- **Instructions.** `ins_op`, `ins_ty` (result type; the width for
  `load`/`store`), fixed operands `ins_a`, `ins_b`, `ins_c`, a number
  `ins_imm` (constant, offset, frame object, callee, routine, trap
  kind), `ins_imm2` (the scale of a `gep`), `ins_reg` (region),
  `ins_res` (first result or -1). A block's instructions are chained by
  `ins_prev`/`ins_next` from `blk_first[b]` to `blk_last[b]`.
- **Blocks.** `order` lists the blocks that exist, entry first.
  `blk_ps`/`blk_pn` give a block's parameters in `bparams`.
- **Edges.** A terminator's ways out are `ins_en[ins]` edges from
  `ins_es[ins]`: `edge_blk` (target), `edge_key` (the case of a
  `switch`), and its arguments, `edge_an[e]` entries of `args` from
  `edge_as[e]`. `br`: taken first. `switch`: default last.

Read the comment at the top of each file before using it.

## How to write a pass

A pass is a function `fn ir_NAME(f: mut IrFunc) -> i64` in its own file
`compiler/opt/NAME.azl`, giving how many things it changed. Here is a
whole one, `compiler/opt/example_pass.azl`: it removes every `add` of
two constants and every `add` of 0.

```azol-fragment
fn ir_example_fold_add(f: mut IrFunc) -> i64 {
    var removed = 0
    var at = 0
    while at < f.order.len() {
        var ins = f.blk_first[f.order[at]]
        while ins >= 0 {
            // Read the next one first: this one may be removed.
            let next = f.ins_next[ins]
            if f.ins_op[ins] == IR_ADD {
                let a = f.ins_a[ins]
                let b = f.ins_b[ins]
                // What can stand for the result: -1 when nothing can.
                var instead = -1
                if ir_is_const(a) && ir_is_const(b) {
                    let sum = ir_const_value(f, a) + ir_const_value(f, b)
                    instead = ir_const(mut f, sum)
                } else if ir_is_const(b) && ir_const_value(f, b) == 0 {
                    instead = a
                } else if ir_is_const(a) && ir_const_value(f, a) == 0 {
                    instead = b
                }
                if instead != -1 {
                    ir_replace_value(mut f, f.ins_res[ins], instead)
                    ir_remove(mut f, ins)
                    removed = removed + 1
                }
            }
            ins = next
        }
        at = at + 1
    }
    return removed
}
```

What it shows:

1. **Walk blocks by position in `f.order`, with `while`.** A
   `for b in f.order` holds the list, and the compiler then refuses
   every change to `f` inside the loop (E0408).
2. **Read `ins_next` before you remove.** `ir_remove` takes the
   instruction out of the chain.
3. **An operand is a value or a constant.** Test with `ir_is_const`,
   read with `ir_const_value`, make with `ir_const`.
4. **Replace, then remove.** `ir_replace_value(f, old, new)` changes
   every user in every block, edge arguments included; only then is the
   definition removed. A removed instruction's results stop existing.
5. **Say how much changed.** The caller repeats a pass that can enable
   itself until it gives 0.

The calls a pass needs most:

```text
ir_operand_count(f, ins)  ir_operand(f, ins, k)  ir_set_operand(f, ins, k, x)   every operand, of any kind
ir_make + ir_add_result + ir_link_before / ir_link_end                          a new instruction, anywhere
ir_replace_value(f, old, new)   ir_remove(f, ins)   ir_uses(f)                    rewriting
ir_split_edge(f, e)  ir_add_block_param  ir_remove_block_param  ir_remove_block   the graph
ir_cfg(f)  ir_dominates(cfg, a, b)  cfg.loop_*  ir_liveness(f, cfg)               analyses, recomputed after a change to the graph
```

To add a new instruction at the end of a block use the builder
(`ir_binary`, `ir_load`, `ir_jump`, ...). To put one in the middle:
`ir_make`, then `ir_add_result`, then `ir_link_before`.

### Testing a pass

Three files per case in `tests/opt/passes/`, as section 9.2 of the
design asks:

- `NAME.ir`: the input.
- `NAME.expected.ir`: the canonical text the pass must produce.
- `NAME.runs`: one line of `function arguments...` per run. The runner
  runs input and expected output through the interpreter with each line
  and compares standard output, standard error and the exit code. That
  catches a wrong expected file as well as a wrong pass.

`example_pass_main.azl` is the pattern for the tool that applies a
pass: parse, verify, run the pass, verify again, print. When there are
several passes this becomes one tool with the pass name as an argument
(`--opt-stop-after`, `--opt-skip` of section 9.1).

### Things azoL does that will slow a pass down or stop it compiling

- `out = out + a + b` copies the whole of `out`: only `out = out + a`,
  one piece at a time, appends in place. The printer was quadratic
  until every line was written that way (57 seconds for 1 MB of text,
  then 9 milliseconds).
- `let f = m.funcs[i]` copies the function. Pass `m.funcs[i]` straight
  to a function instead: a read-only argument is not copied, and
  `mut m.funcs[i]` changes it in place.
- `list.push(x)` copies `x` unless this is the last use of a local
  `x`; pushing a parameter always copies.
- `g(mut f, h(f))` is refused when `h(f)` gives a list, a string or a
  struct (E0410); a number is fine. Put the inner call in a `let`.
- All files of a program share one namespace, and these files will be
  compiled into the compiler: every name here starts with `ir_`, `irp_`
  (parser), `irv_` (verifier), `irm_` (interpreter) or `IR_`/`IRM_`.

## The interpreter is strict

`ir-tool run` ends with exit code 3 and a message that starts with
`ir-interp:` when a function does something the intermediate form
allows the machine to get away with: a load or store outside its
object, through the address of nothing, or in freed memory (a frame
object is freed when its function returns); a load of bytes that were
never stored; a store to the data; an access that is not in the region
it names; `free` of anything but the start of a live block; `sdiv` or
`srem` by zero (the check before it is gone); a `bool` that is not 0
or 1; blocks still allocated when the entry function returns. A pass
that drops a `free`, moves a load above its bounds check, or gives a
load the wrong region fails its test for that reason, even if the
numbers come out right.

## The test files

`tests/opt/*.ir` say in header comments what must happen:

```text
; verify: ok                 `ir-tool verify` prints `ok`
; verify: MESSAGE            it fails with `error: MESSAGE`
; parse: LINE:COL: MESSAGE   it fails with `error: FILE:LINE:COL: MESSAGE`
; cfg:                       `ir-tool cfg` prints the `; out:` lines
; run: [FUNCTION ARGS...]    `ir-tool run` prints the `; out:` lines, writes the
; out: LINE                  `; err:` lines to standard error and exits with
; err: LINE                  `; exit:`. A file can have several runs.
; exit: N
```

Every file that parses is also printed, and the printed text printed
again; the two must be equal.

`run_opt_tests.py` also writes random functions (5,000 by default) that
are valid by construction, in canonical text, and requires that `print`
gives each file back byte for byte, that `verify` says `ok`, and that
`run` prints and returns what the script's own evaluation of the same
functions gives; the script computes each value as it writes the
function, in Python integers, and shares nothing with the interpreter.
Then it damages the texts about 2,000 times and requires that the tool
ends with exit code 0, 1 or 3 every time: no crash, no hang.

## Speed

`ir-tool time` on 20,000 generated functions (703,847 instructions,
19 MB of text), three runs on a loaded two-core x86-64 machine:

| Step | Time | Functions per second |
|---|---|---|
| parse | 0.58 to 0.64 s | about 33,000 |
| verify (includes dominators) | 0.18 to 0.23 s | about 100,000 |
| predecessors, order, dominators, loops, liveness | 0.11 to 0.13 s | about 170,000 |
| print | 0.16 to 0.18 s | about 115,000 |

Parsing is for tests only; the optimiser never reads text.

## For the next stages

- **Lowering** builds with the builder and never with text. It decides
  the regions (facts M4 to M8), adds the runtime routines it needs to
  the table, and sets `param_fx` to all effects (`IR_FX_ALL`) until the
  `effects` pass computes better.
- **The selectors** need a rule for critical edges; when they state
  one, add it to the verifier.
- **Block parameters of one block are contiguous in `bparams`.**
  `ir_add_block_param` moves a block's list to the end when another
  block got a parameter in between, so parameter positions held across
  such a call are stale; values are not.

## Lowering (wave 1, second half)

`compiler/opt/ir_lower.azl` (state, frame, copies, drop and clone
helpers, the function driver), `ir_lower_stmt.azl` (statements, `for`,
`match`) and `ir_lower_expr.azl` (places, expressions, calls, methods,
`?`) turn what the checker hands out into IR for the whole language.
`lw_program(mut lw, c, p)` fills `lw.m`. The walk follows
`compiler/x64_gen.azl` construct by construct, including the order in
which operands, indexes, receivers and assignment targets are
evaluated.

`compiler/opt/lower_tool_main.azl` is the tool `azol-lower` (built from
the front end of the compiler and `compiler/opt`; the build line is at
the top of the file): `emit` prints the IR of the whole program,
standard library included; `run` interprets it with the program's own
output, messages and exit code; `count`, `time` and `check` are for
measuring.

### How values are represented: through frame memory

This is the largest difference from optimizer.md sections 3.6 and 3.8,
which ask for scalar replacement and SSA construction at lowering.
Lowering does **not** do that:

- Every variable and every temporary, scalars included, lives in frame
  memory at the slot the checker gave it (`fns[i].slots`, `expr_tmp`,
  `stmt_slot`, `pat_slot`). The checker's layout is used, not ignored:
  its tables of what to free and what to zero are all in slots.
- One `frame` instruction per slot is made in the entry block (`b0`,
  which holds nothing else and jumps to the body). At the end of each
  function `lw_finish_frame` merges slots whose ranges overlap into one
  frame object, turns the other slots into a `gep` from the object's
  start, and gives every access the region `frame N` of its object. Two
  different frame objects are therefore never reached through one
  another, and `ir_regions_overlap` stays exact.
- A parameter passed by reference (aggregates, `mut`) is an IR `ptr`
  parameter used directly, with region `param K`; read-only ones have
  effect `read`, `mut` ones all effects. A function that returns an
  aggregate gets one more `ptr` parameter, the last, with region `ret`.
- A name a pattern binds to a part of a matched value is the address of
  that part, kept as a value; nothing is copied.
- A list element is `gep buffer, index, bytes` with region
  `buf(header's region)`; nothing is copied to the frame first, as
  section 3.6 asks.
- An enum value is its tag (variant + 1; 0 is "moved out or empty") in
  slot 0 and the fields in the slots the checker laid out. Slots the
  variant does not use are stored as zero when the value is built, so a
  slot-wise copy never reads a byte that was not stored. `match` is a
  chain of `eq` and `br` in the order of the arms, not a `switch`.
- Loads and stores use the type of the slot: `ptr`, `i64`, `i64` for a
  string or list header, `bool` for a `bool`, and `i64` for every slot
  of an enum (its fields have no one type).
- Facts M6 to M8 are not written as tags. They hold by construction in
  what lowering emits.

So the first pass the optimiser needs is promotion of frame slots to
values (`sroa` of section 5.4, run first and on everything), and it is
not optional: until it runs, loops keep their counters in memory. A
scalar slot whose `frame` value is only ever used as the address of a
load or store of one type promotes directly; the enum slots typed
`i64` are the cases that need care.

### Additions to `ir.azl` and `ir_interp.azl`

Nothing that existed changed meaning. Added, each in a marked block:

- `ir.azl`: runtime routines 11 to 30 (`IR_RT_COUNT` is 31):
  `list_push`, `list_clone`, `drop`, `str_concat`, `str_eq_buf`,
  `str_from_i64`, `str_from_bool`, `str_slice`, `chr`,
  `str_append_buf`, `args`, `read_file`, `write_file`,
  `set_executable`, `remove_file`, `rename_file`, `clock`,
  `str_is_utf8`, `print_buf`, `eprint_buf`. They take what the real
  runtime takes: the address of a three-slot header (pointer, length,
  capacity; capacity 0 owns nothing). `ir_rt_real(id)` is the runtime's
  own number of a routine, `ir_trap_real(kind)` that of a stop, and
  `ir_rt_arg_fx(id, k)` what a routine does through argument `k`.
  `ir_clear_func` empties an `IrFunc` and keeps the room of its lists.
- `ir_interp.azl`: the 20 routines; the program's arguments
  (`mach.arguments`); a modelled stack (`stack_limit`, 48 bytes per
  call plus its frame objects; with `overflow_stops` running out is the
  program's own `azol: stack overflow`, exit 1); `reuse_frames`, which
  uses the frame objects of finished calls again so that long runs fit
  in memory (a pointer into a dead frame is then not always caught).

Helpers lowering emits as IR functions: `drop.<type>` and
`clone.<type>` for types with nested owned parts, and `str.cmp(a, b)`
(-1, 0, 1 by bytes) for `<` and the like on strings. A function is
named `<name>.<index in the checker's list>`; the program starts at
`main`. Stops are `br` to one shared block per kind that holds the
`trap` alone. Blocks no path reaches are removed.

### The proof

`python3 tests/opt/run_opt_tests.py` (part 4):

- 139 programs (all of tests/programs, tests/opt/lower, examples, bench
  and bench/suite at small sizes, tests/multi in two orders and with
  embedded files): `azol-lower run` gives the output, the messages and
  the exit code of the executable, stops included; the emitted text
  verifies; `ir-tool print` gives it back byte for byte. One more file
  (examples/point.azl) has no `main`; both tools refuse it.
- Stack overflow is modelled, not reproduced: the three programs that
  run out of stack do so in the interpreter too, and 100,000 nested
  calls still fit; the exact depth at which a program runs out differs.
- The compiler itself (20,428 lines) lowers to 739 functions, verifies
  and reads back. Interpreted, it builds examples/hello.azl (0.24 s)
  and itself (7.5 s) into the same bytes `./azol` writes.
- `python3 tests/fuzz/fuzz.py --lower <azol-lower>`: 15,400 programs,
  no difference. (The first 50 seeds found one mistake, a constant
  under `!` that became a number; it is tests/opt/lower/bool_constants.azl.)

### Size and speed

| Program | Lines | Functions | Instructions | Per line |
|---|---|---|---|---|
| the compiler | 20,428 | 739 | 105,689 | 5.2 |
| bench/generated/big.azl | 44,004 | 6,001 | 205,006 | 4.7 |

Lowering alone (`azol-lower time` less `azol-lower check`), best of 25:

| Program | callgrind Ir | Ir per line | Wall | Lines per second |
|---|---|---|---|---|
| the compiler | 227 M | 11,100 | 32 ms | 640,000 |
| big.azl | 479 M | 10,900 | 77 ms | 570,000 |

Lexing, parsing and checking the same files take 133 M and 203 M. About
40% of lowering is not lowering: `ir_add_func` copies the finished
function (59 M for the compiler, byte by byte in the runtime's `copy`)
and `truncate` empties the working lists one `pop` at a time (38 M).
Before `ir_clear_func` the lists of every function grew from nothing
and the cost was 413 M. A runtime `copy` that moves words, and a list
`clear`, would each take a visible part of what is left.

### What is redundant in the IR, and the pass that removes it

Of the compiler's 105,689 instructions 29,696 are loads and 28,409
stores; 20,256 and 22,097 of those are on frame objects.

| Pattern | Pass |
|---|---|
| A scalar variable is a frame slot: `load`, compute, `store` on every use (loop counters, accumulators) | `sroa` (promotion to values, block parameters at joins) |
| `x * x` loads `x` twice; a list's pointer and length are loaded again for each access | `gvn` with loads |
| A struct literal is built in a temporary and copied slot by slot into the variable; a returned aggregate is built in a temporary and copied to `ret` | `sroa`, then `copyelide`; `dse` for the dead temporary |
| The unused slots of an enum value are stored as zero and never read | `dse` |
| An argument passed by reference is first copied to a temporary when it is not a place | `sroa` after `inline` |
| `clone` of a value whose original is dropped right after | `copyelide` |
| Bounds check before every element access, the length loaded again each time | `gvn`, `range`, `bce` |
| `b0` holds only `frame`s and a jump; `if`, `while` and `match` leave blocks that only jump (981 in the compiler) | `simplify` |
| `match` on a tag as a chain of `eq` and `br` | `simplify` (chain to `switch`) |
| `!x` as `xor x, 1` feeding a `br`; `&&` and `\|\|` as a block parameter that feeds a `br` | `simplify` (branch threading) |
| The inline fast path of `push` repeats per call site with its own loads of length and capacity | `gvn`, `licm` |
| `gep p, 0, 1, K` followed by an access at offset 0 | `simplify` (fold into the offset) |
| `drop.<type>` and `clone.<type>` called for small types | `inline` |

### What the back end must know

- Calls: arguments in order; an aggregate argument is a `ptr`; a
  returned aggregate is written through the last `ptr` argument, and
  such a function has no results. Scalars return as one result. `main`
  returns its exit code or nothing (exit 0).
- `callrt` routines 11 to 30 follow the real runtime's convention;
  `ir_rt_real` maps them. `print_buf`, `eprint_buf`, `str_eq_buf` and
  `str_append_buf` are the header-taking forms next to the first
  half's pointer-and-length ones.
- The file routines return 0 or 1 and write the payload themselves;
  lowering stores the tag (result + 1) after the call.
- `bool` in memory takes 8 bytes. String bytes are `u8` loads and
  stores in a `buf` region.
- Shifts use the low 6 bits of the count; `sdiv` and `srem` by -1 must
  not fault (lowering emits no guard for -1, only the zero check).
- A frame object can be large (the checker's limit on slots applies)
  and the entry block has one `frame` per object plus `gep`s.

### Doubts

- Promotion is owed (above). Enum slots typed `i64` hold pointers and
  `bool`s too; a pass that types values strictly must treat an enum
  slot as raw bits.
- Regions of `param K` for two by-reference parameters are distinct,
  which is right only because the checker forbids passing overlapping
  places when one is `mut` (M4/M5). Pattern-bound addresses carry the
  region of the value they point into.
- The interpreter with `reuse_frames` no longer catches every use of a
  dead frame; the 139 programs also pass without it only as far as
  memory allows, and that was not measured.
- docs/language.md was not compared line by line; where the generator
  and the text could differ, the generator was followed, and no
  difference was noticed while reading it.

## The back end for x86-64 (wave 1, second half)

From verified IR to a running x86-64 Linux executable, with no
optimisation pass. All of it is new files; the default compiler is
untouched.

| File | Lines | What |
|---|---|---|
| `mach.azl` | 219 | The machine-level view of a function, the same for every processor |
| `regalloc.azl` | 1,685 | Linear scan with holes and splitting, parallel moves on edges, `ra_check` |
| `sel_x64.azl` | 1,397 | Selection for x86-64, the calling convention, runtime calls |
| `enc_x64.azl` | 530 | Encodings the default generator's `x64.azl` does not have (it is not touched) |
| `emit_x64.azl` | 767 | The frame, the allocator's moves, bytes, short and long jumps |
| `back_x64.azl` | 346 | A module to an ELF image through `../elf.azl` and the unchanged runtime |
| `back_tool_main.azl` | 164 | `azol-back build <file.ir> -o <out> [--program] [--run NAME n...] [--dump] [--check] [--stress] [--time]` |

`tests/opt/run_back_tests.py` (also run by `run_opt_tests.py`) is the
proof: every runnable `.ir` case and 5,000 random functions, built as
they are and with three registers only, give what the interpreter
gives; every allocation passes `ra_check`; a sample runs under
memcheck; the four kernels print their benchmarks' numbers; and, once
the lowering stage is in the same tree, every program of
`tests/programs` and `examples` goes `azol-lower emit` then `azol-back
build --program` and behaves as its default build does.

### What a selector for another processor must provide

`regalloc.azl` and `mach.azl` are used unchanged. A selector for ARM64
is one file like `sel_x64.azl` and one like `emit_x64.azl`:

1. `<p>_target() -> RaTarget`: the registers the allocator may hand
   out, most wanted first (caller-saved before callee-saved), and one
   scratch register outside that list for cycles of moves. A stress
   target with three registers, for tests.
2. `<p>_function(f: mut IrFunc, targets, ...) -> SxFunc`: fills an
   `MFunc` with `mf_block`, `mf_param`, `mf_ins`, `mf_operand`,
   `mf_succ`, `mf_edge_arg`, `mf_edge_const`, `mf_vreg`, `mf_hint`.
   Blocks in layout order, block 0 the entry, critical edges split
   first (a block of one `jmp` on each).
3. Then `ra_allocate(mut mf, target) -> RaResult`, and in tests
   `ra_check(mf, result)` must give "".
4. `<e>_function(sx, result, frames, trap_at) -> EmFunc`: the bytes of
   one function, with the calls, runtime calls and data references the
   linker fills in. It does the moves of `result.mv_*` before each
   instruction, in the order given.
5. A driver like `bk_build` that lays the functions out and hands code
   and data to the right writer (`../arm64_link.azl`, `../macho.azl`).

Checklist of what the operands must say, because the allocator knows
nothing else:

- a register an instruction needs is named in the operand (`RAX = copy
  v`, then the instruction with `RAX` as a use and a definition);
- `MF_LATE` on a use that is read after a definition of the same
  instruction is written (x86-64: the second operand of a two-address
  operation that is not also the first; ARM64 needs it rarely);
- `MF_ANY` on a use the emitter can take from a spill slot;
- `clob` on calls: every caller-saved register;
- hints between a block parameter and its arguments, and between a
  value and the fixed register it is copied to or from;
- arguments of an edge are virtual registers or constants (`-(k + 2)`),
  never processor registers.

On ARM64 there are no two-address constraints, no fixed registers for
division or shifts, and compare-and-branch needs no flags between
instructions, so the selector is simpler; the things to decide are the
scratch register (x16 or x17), `ldp`/`stp` in the prologue, and
immediates that do not fit (materialised in the scratch register by the
emitter, or as a `movi` by the selector).

### The calling convention as built

Between functions of the module: arguments 1 to 8 in rdi, rsi, rdx,
rcx, r8, r9, rax, r10, the rest in the caller's frame; results 1 to 4
in registers, the rest in the caller's frame. rbx, rbp and r12 to r15
are saved by the function that uses them, in the prologue; there is no
frame pointer. The stack pointer is kept a multiple of 16 only in
functions that call the runtime or call a function that does. A frame
above one page is made a page at a time, each touched.

Runtime routines: the first eleven of `ir.azl` (which take strings as
separate values) are written out by the selector; every other routine
is found by its name (`sx_rt_by_name`) and called with its arguments
in the platform's registers in order, its result from rax. That is the
convention of the routines the lowering stage added, so lowered
programs link without a wrapper. A routine the selector does not know
is an error, not silence.

Division: `sdiv` and `srem` by a value that is not a constant compare
it with -1 first and negate instead of dividing, so the smallest number
divided by -1 wraps as the language says and the processor does not
stop the program (`tests/opt/run_div.ir`).

### Layout

The emitter keeps the selector's block order, with three exceptions it
makes itself: a jump back to a short block that ends in a comparison
(a loop's test) is replaced by a copy of that block, so a loop has one
jump a round; a jump to a block that only jumps on goes straight to
where that leads; and such blocks are not emitted. The last two took
`sieve` from 752.8 M to 714.1 M instructions: every `push` paid for a
jump over a jump.

### Measured: registers alone

Hand-written IR of four benchmarks' kernels as naive lowering *with
values in SSA* would give them (`tests/opt/kernels`), x86-64,
instructions by callgrind, whole process:

| Kernel | default mode | this back end | `rustc -O` | ratio to Rust: was, is, predicted |
|---|---|---|---|---|
| `fib40` | 4,470.7 M | 4,139.5 M | 3,248.8 M | 1.38, 1.27, about 1.2 |
| `loops` | 483.7 M | 315.3 M | 420.1 M | 1.15, 0.75, about 1.05 |
| `sort` | 679.2 M | 679.6 M | 568.3 M | 1.20, 1.20, about 1.1 |
| `sieve` | 714.1 M | 714.1 M | 657.4 M | 1.09, 1.09, about 1.0 |

`loops` is better than predicted: everything the loops use stays in
registers.
`fib40` is short of the prediction because the saved registers are
pushed before the base case is tested (shrink-wrapping, wave 4).
`sort` and `sieve` are level with the default mode and not better,
because the kernels read the list's header from the frame at every
access, exactly as lowering writes it, and the default generator
already keeps an index it has checked; what is left there is `licm`,
`bce` and `gvn`, not registers.

The whole compiler, lowered (memory form, no promotion) and built by
this back end, builds itself byte-identically to `./azol` and does so
in 171.1 M instructions where the default-built compiler takes 185.6 M.

### Speed of the back end

Selection, allocation and emission, by the tool's own clock, on a
loaded machine:

| Input | Functions | Select | Allocate | Emit | Functions/s | Source lines/s |
|---|---|---|---|---|---|---|
| the compiler (20,428 lines) | 742 | 35 ms | 46 ms | 11 ms | 8,000 | 220,000 |
| `big.azl` (132,004 lines) | 18,004 | 238 ms | 338 ms | 69 ms | 27,900 | 205,000 |

That is the back end alone, on memory-form IR, against a budget of
300,000 lines a second for the whole `-O` pipeline: the allocator is
the largest part and the first thing to make faster, and promotion will
shrink its input.

### Not done here

- No per-function saved-register summaries and no shrink-wrapping
  (wave 4); the code is arranged for both (`SxFunc.calls`, the saved
  set is computed per function already).
- `load`-operate-`store` on the same address is three instructions,
  not one `add [mem], imm`.
- `switch` is a chain of comparisons.
- No vector or `f64` operations: the IR has none yet.
- ARM64: nothing.

## Promotion of frame memory: `sroa` (wave 2)

`compiler/opt/pass_sroa.azl`, with the driver `sroa_tool_main.azl`
(`sroa-tool <in.ir> [-o out.ir] [--stats]`, and `emit`, `stats [--each]`,
`time`, `lower`, `run` on `.azl` files) and the proof
`tests/opt/run_sroa_tests.py`.

    ir_sroa(f: mut IrFunc, effects: SroaEffects, stats: mut SroaStats) -> i64
    ir_sroa_module(m: mut IrModule, stats: mut SroaStats) -> i64
    sroa_effects(m) -> SroaEffects      // every function's `param_fx`, by name

### The algorithm

Pruned SSA construction in the classic form: dominance frontiers
(Cooper, Harvey and Kennedy's two-finger walk over `ir_cfg`'s `idom`),
block parameters at the iterated frontier of the stores, pruned by a
liveness problem over the cells, then one walk over the dominator tree
with an undo log. Braun's on-the-fly construction was the alternative;
it needs the whole function sealed before it is better than this, and
the lowered IR is already a finished CFG, so the textbook form is the
shorter and the more predictable one. The bit tables have one bit per
cell that is *read in a block before that block writes it*; on the
compiler that is a small share of the cells, and the others cost
nothing per block.

A cell is eight bytes of a frame object. There are three classes:

| class | when | what happens |
|---|---|---|
| variable | the address never leaves the function | loads become values, stores go |
| shadowed | the address is only given to calls and runtime calls | stores stay; a load takes the value that reaches it, until a call that may write the cell, after which the next load is real and gives the value |
| kept | the address is stored in memory, indexed by a value, returned, compared, selected or sent along an edge; or the cell is read and written with different types, or with a narrow or unaligned access | untouched |

Shadowing rests on two facts of the language (M3 and M9 of
`optimizer.md`): a callee does not keep an address, and writes only
what its `mut` arguments and its result reach. So after
`call f(addr)` where `f` only reads its parameter, the cell still holds
what was stored. Whether a call writes comes from `sroa_arg_writes`
(the callee's `param_fx`; `ir_rt_arg_fx` for a runtime call; an
unknown callee writes).

An escape exposes the object from that offset to its end, not before
it: lowering only ever passes the address of a whole value, which a
callee reads forward.

What is left of an object is the range from its first cell that stays
to its last; accesses are rewritten to `frame + (offset - start)`, and
an object with nothing left has size 0 and no `frame` instruction. The
objects keep their numbers so region names stay valid. This is the one
change to a shared file: the text form (`ir_text.azl`) now accepts a
frame object of size 0.

### Loads behind `ptr` parameters

Keyed by (parameter, offset, type). Behind a parameter the function
only reads, a load that another load of the same bytes dominates takes
its value. Behind a parameter it may write, what is known ends at a
store over the same bytes and at a call that is given the parameter's
address to write through, or a `ptr` the pass cannot name; it is
carried between blocks by a forward problem over single values, in
which a join keeps a value only when every way in brings the same one.
No load is moved: a loop reads `xs.len` once in its header and not
again in its body, but the header load stays in the loop (that is
`licm`'s). Two hooks for the `effects` pass: `sroa_param_is_stable`
and `sroa_arg_writes`.

### Three rules that the verifier forces

- A `ptr` variable never takes a constant: `gep`, `load` and `store`
  need a value as address. A store of 0 to such a cell stays and is
  read back by a `load` put right after it (235 on the compiler).
- A `bool` variable takes a `bconst` put before the store, not a bare
  constant, which would type `xor 0, 1` as `i64`. Some of these are
  never used; they are `dce`'s.
- A load that no store reaches reads 0 (the interpreter refuses such a
  load, so no program that runs can tell), except for `ptr`, where the
  load stays.

### What it does to the compiler and the benchmarks

The compiler (739 functions, 106,337 instructions before, 71,622
after):

| | before | removed |
|---|---|---|
| loads | 29,696 | 17,872 (60.1%), 1,298 of them behind parameters |
| stores | 28,409 | 10,260 (36.1%) |
| frame objects | 8,297 | 4,244 gone; 191,448 bytes -> 118,184 |
| block parameters | | 1,179 added |

Of 21,911 cells, 9,393 (42.8%) are variables, 12,321 (56.2%) shadowed,
and 197 kept: 183 because the address is stored in memory, 14 because
they are read and written with different types. No cell is kept for
any other reason.

The 44 programs of `bench/` and `bench/suite/` together: 4,277 loads,
68.5% removed (48% for `json`, 100% for `loops`, `fib40`, `bits`);
3,579 stores, 44.1% removed (20% to 100%); 462 block parameters.

Hot loops (whole program, loads / stores / instructions):

| program | before | after | what is left in the inner loop |
|---|---|---|---|
| `loops` | 13 / 9 / 50 | 0 / 0 / 23 | nothing: `mul`, `srem`, `eq`, `add`, two parameters |
| `fib40` | 4 / 1 / 21 | 0 / 0 / 15 | nothing |
| `sieve` | 29 / 21 / 99 | 7 / 8 / 59 | marking loop: `ult` (bounds), `gep`, one `store bool` to the buffer; length and buffer address come from before the loop |
| `sort` | 80 / 34 / 231 | 18 / 9 / 131 | scan loops: `ult`, `gep`, one buffer load, compare |
| `particles` | 104 / 52 / 304 | 20 / 17 / 167 | `step`: two header loads per turn (`len`, buffer), then only the fields' loads and stores |

The first scan loop of `quicksort`, `while xs[i] < pivot { i = i + 1 }`,
before:

    b10:
        v22 = load i64, v16, 0  [frame 3]
        v23 = load i64, p0, 8  [param 0]
        v24 = ult v22, v23
        br v24, b13, b5
    b13:
        v25 = load ptr, p0, 0  [param 0]
        v26 = gep v25, v22, 8, 0
        v27 = load i64, v26, 0  [buf(param 0)]
        v28 = load i64, v14, 0  [frame 2]
        v29 = slt v27, v28
        br v29, b11, b12
    b11:
        v30 = load i64, v16, 0  [frame 3]
        v31 = add v30, 1
        store i64, v16, 0, v31  [frame 3]
        jump b10

and after (`v9`, `v11` and `v13` are loaded once, before the loops):

    b10(v79: i64):
        v24 = ult v79, v9
        br v24, b13, b5
    b13:
        v26 = gep v11, v79, 8, 0
        v27 = load i64, v26, 0  [buf(param 0)]
        v29 = slt v27, v13
        br v29, b11, b12
    b11:
        v31 = add v79, 1
        jump b10(v31)

### The proof

`python3 tests/opt/run_sroa_tests.py` (`--quick`, `--only-small`,
`--random N`, `--seed`, `--update`):

- 15 hand-written cases in `tests/opt/sroa/` with their expected text
  and 42 runs, each run before and after: two latches, a read before
  any write, a store on one branch, an object of which part escapes, a
  reader and a writer, a loop that never runs, an irreducible graph,
  the cells of an enum, a stored address, null and bool constants,
  loads behind parameters within and across blocks, trimming, mixed
  types, values that live across calls inside a loop.
- 3,000 random functions with frame objects, loops, branches, calls
  that read or write a cell, and two address parameters that are read,
  stored to and given to callees; every one interpreted before and
  after with the same result and output.
- The 139 programs of the lowering proof: lowered, promoted,
  interpreted, the same exit code and output as their executables.
- The compiler lowered, promoted and interpreted builds
  `examples/hello.azl` and itself, byte for byte.
- `tests/fuzz/fuzz.py --sroa TOOL`: 3,600 programs, 0 findings.

Each rule was also broken on purpose (a callee's writes not seen, a
block parameter dropped, a join that keeps a value that differs, a
store that ends nothing, a load after a call that gives nothing new,
an unknown address that ends nothing); the tests refuse every one.
The rule for a parameter whose address is put in memory (`leaky`) has
no test that runs: the interpreter's region check refuses the program
that would need it.

### Speed

`sroa-tool time` against `sroa-tool lower`, callgrind and wall clock:

| | IR instructions | Ir of the pass | per instruction | wall |
|---|---|---|---|---|
| the compiler (20,428 lines) | 106,337 | 314.6 M | 2,960 | about 37 ms, 2.9 M instructions/s |
| `bench/generated/big.azl` (44,004 lines) | 209,007 | 587.9 M | 2,810 | about 70 ms, 3.0 M instructions/s |

At 300,000 lines a second the compiler has 68 ms and `big.azl` 147 ms,
so the pass alone takes about half of the `-O` budget. That is too
much, and a third of it is not the pass: 105.5 M of the 314.6 M are the
runtime's `realloc` copying byte by byte (the loop at the start of the
executable), behind `filled(n)` growing its list by doubling and behind
the first `push` on each of a function's eighteen instruction lists
when the pass adds one instruction. A `realloc` that copies words and a
`filled` that allocates once would take about 90 M off this pass and a
tenth off lowering (42 M of its 360 M) without a line changed here. The
rest: the walk itself 48 M, the parameter problem about 45 M, analysis
15 M, `sroa_ins_var` 12 M, shrinking 11 M, `ir_cfg` 8 M. Reusing the
tables from one function to the next is the next thing to do inside
the pass.

### In real code: lowering, the pass, the x86-64 back end

Measured once in a scratch clone with master merged in (the back end is
not in this branch): every program of the lowering proof went through
`azol-lower emit` or `sroa-tool emit` and then `azol-back build
--program --check`. All 139 behave as their default builds, with and
without the pass. Instructions run (callgrind, small sizes) on the 28
programs of `bench/suite`, as a ratio to `./azol build`:

| | geometric mean |
|---|---|
| lowering alone, then the back end | 1.14 |
| lowering, `sroa`, then the back end | 0.87 |
| the pass against lowering alone | 0.76 |

Best after the pass: `sha256` 0.61, `bits` 0.63, `dp_lcs` 0.65, `loops`
0.70, `dijkstra` 0.73. Still above the default mode: `particles` 1.89,
`soa` 1.32, `sort_lib` 1.13, `fannkuch` 1.02. The first two read a list
header and check a bound for every field of every element, which the
default generator's element tracking avoids and which here waits for
`gvn`, `licm` and `bce`. `sort_lib` is the one program the pass makes
slower than lowering alone (193.7 M against 186.9 M instructions); the
cause was not looked for and is worth a look by whoever tunes the
allocator: more values live in registers across its calls.

The four kernels (whole small program): `fib40` 2.03 M default, 2.10 M
lowered, 1.88 M with the pass; `loops` 1.74 M, 1.85 M, 1.22 M; `sort`
5.88 M, 8.12 M, 5.04 M; `sieve` 7.06 M, 10.59 M, 6.91 M.

The merge itself: one conflict, in this file (both sides append a
section; keep both).

### What the next passes and the back end should know

- Memory traffic that remains is of three kinds: shadow stores (a store
  to a cell whose address a call gets; most of the 18,149 left; `dse`
  can remove those that no call reads before the next store),
  accesses behind parameters and into buffers, and the 197 kept cells.
- A block parameter of type `ptr` can get the constant 0 as an
  argument (a path on which nothing was stored).
- `gep x, 0, 1, 0` and repeated identical `gep`s and bounds checks
  (`ult i, len` four times in `step`) are left for `simplify`.
- A `bconst` with no use, and the `load` right after a `store ptr, x,
  0, 0`, are left for `dce` and `simplify`.
- Frame objects of size 0 exist and have no `frame` instruction; the
  back end gives them no slot. `b0` still holds only `frame`, `gep`
  and the jump.
- Enum payload cells are `i64` whatever they hold; a cell read with
  two types is kept in memory, not converted.
- Loads behind parameters are not hoisted; a loop header still loads
  `len` once per turn.
- Critical edges are not split; arguments were added to existing
  edges, including the two edges of a `br`.

### Doubts

- The cost, above.
- Shadowing and the parameter rules rest on M3 and M9 being true of
  everything lowering emits. If a later feature lets a callee keep an
  address (a closure that captures by reference and outlives the
  call), both must be revisited; `leaky` only covers what this
  function can see.
- One fuzz finding that is not this pass's: seed 2200256 leaks one
  block in `azol-lower run` without the pass too (a `?` inside the
  index of an assignment target whose right side is a temporary
  string).

## The passes simplify, dce, dse and the pipeline (wave 2, first part)

Files: `pass_simplify.azl` with `pass_simplify_util.azl`, `pass_dce.azl`,
`pass_dse.azl`, `pipeline.azl`, the tools `opt_tool_main.azl`
(`opt-tool <in.ir> -o <out.ir> [--passes a,b,c] [--fixpoint]
[--verify-each] [--stats]`) and `opt_build_main.azl` (`azol-opt build
<files...> -o <out>`: front end, lowering, the passes, the x86-64 back end;
this is what becomes `azol build -O`), and `tests/opt/run_pass_tests.py`
with the cases in `tests/opt/simplify`, `dce` and `dse`.

### What each pass does

- **simplify.** Constant folding with azoL's arithmetic; identities and one
  canonical form (constants to the right, `x - c` as `x + (-c)`,
  comparisons normalised, `x * 2^k` as a shift, `x / 2^k` and `x % 2^k`
  with the sign added in first, an arithmetic shift whose sign copies are
  masked away as `shr`, shift counts reduced to six bits); `select` and
  `bool` rules; a branch on a constant, on equal edges, or on a condition a
  dominating branch already decided; jumps through blocks that only pass
  on; trap blocks of one kind merged into one; `switch` from a chain of
  `eq`/`br` on one value; block parameters whose arguments are all one
  value; blocks merged with their only successor. A replaced value is
  noted and looked up when its users are next visited, which is the copy
  propagation.
- **dce.** Instructions without effect whose results nobody uses, blocks
  nothing reaches, block parameters nobody reads, calls of runtime routines
  the IR marks as pure, allocations that are only written and freed. A loop
  goes only when it has one counted exit the pass can bound and holds
  nothing with an effect; any other loop stays, because it might not end.
- **dse.** Frame objects whose address stays in the function: liveness of
  every eight bytes, backwards over the whole function, so a store nothing
  reads again goes, also across blocks and before a stop. Everything else:
  inside one block, a store written again before anything can read it.

### Deliberately not done

- A division or remainder by a constant zero stays (the check before it
  stops the program). Nothing observable moves.
- `dse` across blocks for memory that is not a private frame object
  (parameters, buffers): it needs the effect summaries of a later pass.
- A store before a stop is removed only for a private frame object.
- A trap block is never merged into another block: `no_merge_into_trap`.

### Adding a pass

A function pass `fn ir_NAME(f: mut IrFunc) -> i64`: one line in
`pl_run_pass`, and its name in `PL_ORDER`. A pass on the whole module: one
line in `pl_run_module_pass`, its name with a comma in `PL_MODULE_PASSES`,
and its name in `PL_ORDER`. For `sroa` (branch `sroa`, entry
`ir_sroa_module(m: mut IrModule, stats: mut SroaStats) -> i64`):

    // in pl_run_module_pass
    if name == "sroa" {
        var numbers = sroa_new_stats()
        return ir_sroa_module(mut m, mut numbers)
    }
    const PL_MODULE_PASSES = "sroa,"
    const PL_ORDER = "simplify,sroa,simplify,dce,dse,dce,simplify"

and `pass_sroa.azl` in the source lists `PASSES` of
`tests/opt/run_pass_tests.py`. None of the three passes assumes `sroa` has
run; they are correct on memory-form IR and only find little there.

### Proof (x86-64 Linux, 12 November 2026)

`python3 tests/opt/run_pass_tests.py`: 175 hand-written cases (56 where a
rule must not fire, each with its reason); 85,118 functions of edge
numbers, every operation with constants on either side, against the
script's own arithmetic; 3,007 random functions the same before and after
in the interpreter (530 also as native code), and a second run of the
passes changes nothing; 139 whole programs interpreted and built by
`azol-opt`, also with three registers, like their default builds; the
compiler, optimised and built by `azol-opt`, builds `hello` and itself into
the bytes `azol` writes. Fuzzer with `--lower --opt`: 3,200 programs, 0
findings, on the final sources.

### Measurements

IR instructions, lowered and after the default order
`simplify,dce,dse,dce,simplify`:

| | before | after | removed |
|---|---|---|---|
| the compiler | 105,688 | 100,594 | 4.8% |
| the 28 programs of `bench/suite` | 11,977 | 11,510 | 3.9% |

Native instructions (callgrind), full sizes:

| | `./azol build` | back end, no passes | back end, these passes |
|---|---|---|---|
| `loops` | 483.7 M | 512.3 M | 512.3 M |
| `fib40` | 4,470.7 M | 4,636.2 M | 4,636.2 M |
| `sort` | 679.2 M | 1,022.2 M | 1,025.6 M |
| `sieve` | 714.1 M | 1,097.2 M | 1,097.2 M |

So on lowered IR as it is, with every variable in frame memory, the
optimising path is slower than the default generator and these three
passes change almost nothing: there is nothing for them to see until
`sroa` has turned memory into values. (The hand-written kernels of the back
end's section were SSA already; these are whole programs from lowering.)
`sort` is 0.3% worse with the passes; not looked into.

Speed, on the compiler (20,400 lines with the library): the passes take
768 M instructions, 7,270 for each IR instruction, about 77 ms alone and
115 ms inside `azol-opt`, where the front end with lowering takes 91 ms
and the back end 163 ms. The target for the whole `-O` pipeline is 300,000
lines a second, which is 68 ms for this program: the three passes alone
use more than that, and the whole path runs at about 55,000 lines a
second. Most of the passes' time is tables built again for every function
on every run (`su_preds`, use counts, the liveness bits of `dse`); that is
the first thing to take out.

## Integration

The whole path (lowering, `simplify,sroa,simplify,dce,dse,dce,simplify`,
the x86-64 back end) as one thing: what broke when the parts met, what
was done, and what was measured.

### A definition laid out after its reader

`azol-opt build --stress` refused 11 programs with "an interval moves to
a register in the middle of an instruction". After promotion a value can
be live in a block that is laid out before the block of its definition
(the lowering's block order is not a reverse postorder once a loop's
value no longer goes through frame memory). Its interval then has a
range that starts at the definition, which is an odd position. When the
interval is in a hole and another value takes its register, `ra_take`
cuts it at the start of that range; the part that follows got a register
and `ra_resolve` refused it, though nothing has to move there: the
instruction at that position writes the value. `ra_resolve` now lets a
part start at an odd position exactly when that position is the write of
the value's definition. `ra_check` is unchanged and accepts the result.
`tests/opt/run_def_after_reader.ir` is the case from `dijkstra`; the
random functions of `run_back_tests.py` now lay their blocks out in a
shuffled order in a third of the functions, which the old allocator
refused in 243 of 400 modules with three registers.

### Running the passes twice

With `sroa` in the order a second run changed five random files.
`sroa` ran once even with `--fixpoint`, and `dse` and `dce` after it
remove the last readers and stores of some objects, so only a second
`sroa` could promote or narrow them; and only `sroa` emptied a frame
object nothing names. A second iteration is needed by nature, so the
definition is: with `--fixpoint` the whole list, module passes included,
runs again until a round changes nothing (at most 8 rounds). To make
"changes nothing" true, `dce` sets the size of a frame object no `frame`
instruction names to 0, and the bytes `sroa` narrows count as a change.
Without `--fixpoint` the list runs once, as before.

### A value lost when `?` leaves in the target of an assignment

Fuzz seed 2200256: `xs[fail(n)?] = "123".pad_left(40, "a")` leaked the
new text when `?` returned, because the lowering had the value parked in
a slot the early-return path does not know. `lw_assign` now registers
the slot as live while the target's address is worked out
(`tests/opt/lower/try_in_assignment_target.azl`). **The default
generators have the same leak** (aarch64-macos build under the checking
emulator: 11 allocations, 10 frees); that is outside `compiler/opt` and
still open.

### Values that live across a call in a loop

`sort_lib` was slower with `sroa` (188 M instructions) than without
(185 M). The IR after promotion is better; the loss was in the
allocator. A promoted value that is live across the call to the
comparison got a caller-saved register that is free only up to the call,
was cut there, reloaded after it, and moved back to where the top of the
loop expects it, every round, while callee-saved registers held values
that are read again only after the loop. `ra_try_free` now asks
`ra_quiet_register` first when the free register does not last and the
value is read again after the cut: a register whose holders are not
needed in a register before the new value ends is taken, and the holders
go to their slots. `sort_lib` 188.3 M to 134.6 M (default build
171.7 M), `sha256` 255.4 M to 248.5 M, nothing else moved.

### The three columns

`bench/gate.py --instructions --azol-opt <azol-opt>`, small sizes,
callgrind. Geometric means: default against `rustc -O` 1.85, `azol-opt`
against default **0.85**, `azol-opt` against `rustc -O` **1.57**.

Worse than the default build:

| Program | ratio | cause | pass that removes it |
|---|---|---|---|
| `particles` | 1.89 | the default generator works out the checked address of `ps[i]` once for a run of accesses; the lowering checks and scales for each of the 12 | `gvn` (same check, same `gep`), then `bce` |
| `soa` | 1.32 | the same, and the length and pointer of each list read again for each access | `gvn`, `licm` for the headers |
| `fannkuch` | 1.02 | not looked into; presumably the same on the lists of the flip loop | `gvn` |

Furthest from `rustc -O`: `soa` 5.34, `tokenizer` 3.62, `sort_lib` 3.30,
`dijkstra` 3.10, `bits` 2.93: repeated bounds checks and list headers
(`soa`, `dijkstra`, `sort_lib`), calls that are not inlined
(`tokenizer`, `sort_lib`'s comparison, `bits`), and no strength reduction
of the index arithmetic. Better than `rustc -O`: `binary_trees` 0.47,
`lru` 0.64, `tree` 0.64, `loops` 0.79, `knapsack` 0.84.

### Compile speed

Instructions for the whole of `azol-opt build`, the tool built by the
default compiler: the compiler's sources (20,428 lines) 2,485 M before
and 2,276 M after; a generated program of 13,204 lines 1,390 M and
1,298 M. The default compiler takes 186 M and 99 M for the same inputs.
Wall time, best of five on a loaded machine: `bench/generated/big.azl`
(132,004 lines) 1.96 s to 1.71 s, about 77,000 lines a second, a quarter
of the 300,000 target.

`azol-opt build --map <path>` writes where each function's code starts,
so that a callgrind profile of an `azol-opt` built by itself can be read
by function. The three largest costs:

1. Growing lists, 34% to 38% of all instructions: `rt_list_push` copies
   the buffer byte by byte when it doubles, and `x.filled(n)` in the
   library is `n` pushes, so every per-function table (`ir_sroa`,
   `su_preds`, `ir_cfg`, use counts, `ra_finish`, `ra_build`) costs about
   30 instructions an element. Not fixed: the runtime and `lib/prelude`
   are outside this directory. A `filled` that allocates once, or tables
   kept between functions, removes most of it.
2. Asking for operands, about 14%: `ir_fixed_operands`, `ir_is_binary`,
   `ir_is_unary`, `ir_operand_count`, `ir_operand_place`, `ir_operand`.
   `ir_fixed_operands` now tests its ranges itself.
3. Copies of whole functions, 5% to 6%: `bk_build` copied each function
   before selection (fixed: it takes the module `mut`), and
   `ir_add_func` copies each one into the module (open).

## The back end for ARM64 (macOS and Linux)

From verified IR to a running ARM64 executable for `aarch64-macos` and
`aarch64-linux`, with no optimisation pass: the second selector for
the allocator of wave 1. The machine code is the same for both
targets; only the file around it differs. All of it is new files; the
default compiler is untouched and byte-identical.

| File | Lines | What |
|---|---|---|
| `sel_arm64.azl` | 1,709 | Selection, what is folded, the calling convention, runtime calls |
| `enc_arm64.azl` | 435 | The instructions as numbers; constants (`movz`/`movk`/`movn`, logical immediates) |
| `emit_arm64.azl` | 1,147 | The frame, the allocator's moves, instructions, jumps and their longer forms |
| `back_arm64.azl` | 364 | A module to a Mach-O image through `../macho.azl`, or to an ELF image |
| `back_tool_main.azl` | +24 | `azol-back build ... --target aarch64-macos\|aarch64-linux` (x86-64 when not given) |

`tests/opt/run_back_arm64_tests.py` (also run by `run_opt_tests.py`)
is the proof, in emulators on a machine that is not ARM64, always for
both targets: every runnable `.ir` case with all registers and with
three; random functions (30,000 when run alone, every fourth module
also with three registers); two functions large enough to need the
long forms of `tbz` and `b.cond`; every program of `tests/programs` and
`examples` through `azol-lower emit` and `azol-back build --program`
against its default build for the same target; and the compiler
itself, lowered (and, for macOS, lowered and promoted by `sroa`) and
built by this back end, which in the emulator builds
`examples/hello.azl` and itself byte for byte as `./azol` does. Every
allocation passes `ra_check`. The checking emulators (a heap that
reports every access outside a live block, and a check that x19 to x28
and sp come back unchanged) are not in the repository; without them
the script says so and skips. `tests/opt/mac_check.sh` is the same
question asked of a real Mac, with nothing but `./azol`.

### What differs from the x86-64 selector

- **Nothing is two-address and nothing needs a fixed register**, so no
  operand is ever `MF_LATE`, and division, the high half of a product
  and shifts by a register are plain three-register instructions.
  `srem` is `sdiv` and `msub`; dividing by -1 needs no guard, the
  processor gives what the language says.
- **Constants cost instructions here.** An immediate exists for `add`
  and `sub` (12 bits, or 12 bits shifted by 12; a negative one turns
  the operation round), for `and`/`or`/`xor` (a repeated run of ones,
  `ax_logical_imm`), for `cmp` (-4095 to 4095, the negative ones as
  `cmn`) and for shifts. Everything else is a `movi` into a virtual
  register. That alone made the `loops` kernel slower than the default
  generator (which had the bound in a variable, so in a register), so:
  the first eight different constants that instructions *inside loops*
  need in a register are made once, in the entry block, and live in
  registers like any value (`sa_pool`; a definition no loop turned out
  to use becomes `SA_NOP`). `mul` by a power of two is a shift; a
  stored zero, and a zero operand of `select`, is the zero register.
- **Folds the x86-64 selector does not have**: `mul` into `madd`/`msub`;
  `shl` by a constant into the shifted operand of
  `add`/`sub`/`and`/`or`/`xor`; `and` whose only use is a comparison
  with zero into `tst`, or into `tbz`/`tbnz` when it tests one bit;
  comparison with zero into `cbz`/`cbnz`, a sign test into
  `tbz`/`tbnz` on bit 63; and the flags of the last `add`, `sub` or
  `and` of a block reused by its `br` (`adds`/`subs`/`ands` then
  `b.eq`/`b.ne`/`b.mi`/`b.pl`, only for those four conditions, which
  do not depend on overflow, and only when no call, `select` or
  materialised comparison comes in between). The allocator's moves
  never touch the flags on ARM64, which is what makes the last one
  safe.
- **A fold the x86-64 selector has and this one does not**: a `load`
  as the memory operand of `cmp`. There is no such instruction.
- **Addresses.** `[base, #offset]` and `[base, index, lsl #log2(width)]`
  (or unscaled index) are free; anything else costs an `add`. So a
  `gep` is folded into its loads and stores when every use is free, or
  when it has one use; otherwise it is computed once. The emitter
  finishes what does not fit (a far offset, a frame object with an
  index) in x17 and x16.
- **Pairs.** Two `load`s or two `store`s of eight bytes that follow
  each other, from the same address operand, eight bytes apart, are
  one `ldp` or `stp`; so are the first two words of the string headers
  for runtime calls, and the saved registers.
- **Operands in slots.** Only `copy` reads a slot directly. But the
  emitter owns two registers inside an instruction, so the operands
  beyond the third (`select`'s two values, `madd`'s addend) are
  `MF_ANY` and are fetched into x17/x16; that is also what lets every
  instruction be allocated with three registers.
- **Jumps** all have one size, so nothing is relaxed in place. A
  function is emitted with every jump in its short form; if a `tbz`
  (±32 KiB) does not reach it is emitted again with those as the
  opposite test over a `b`, and if a `b.cond`/`cbz` (±1 MiB) does not
  reach, once more with all conditional jumps in that form. Jump
  threading and the copied loop test are as on x86-64.
- **The test before the frame.** When the first block of a function
  is nothing but a comparison of arguments that arrived in registers,
  and one of its two ways out only returns (an argument, a constant or
  nothing), the emitter does that test before the prologue, from the
  argument registers themselves, and returns there
  (`ea_find_early`); the block's own test then leads one way. It is
  the one case of shrink-wrapping the default ARM64 generator has
  too, and without it `fib` was a fifth slower than the default
  build: the base case paid for two saved pairs.
- **Layout.** The x86-64 selector lays the blocks out in the IR's
  order. Here they are laid out in chains: a block is followed by the
  block it jumps to, or by one of the two successors of its `br` (the
  one deeper in loops; else the first, or the second when the test is
  for equality, which rarely holds), while that block is still free; a new chain starts at the first free block in reverse
  postorder; traps still come last. After promotion the IR's order
  had left an unconditional jump on most paths through a loop body.
  A block is always laid out after the block that dominates it.

### Registers, and why

| Registers | Use |
|---|---|
| x9–x15, x8, x0–x7 | allocatable, destroyed by calls; in this order, so that short-lived values stay out of the argument registers |
| x19–x28 | allocatable, saved by the function that uses them (in pairs) |
| x16 | the allocator's scratch register for cycles of moves; free inside an instruction |
| x17 | the emitter's: far offsets, the quotient of `srem`, slot-to-slot moves, constants on their way to a slot |
| x18 | never touched: it belongs to the system on macOS |
| x29 | never allocated and never used as a frame pointer; saved only to make the number of saved registers even |
| x30 | saved by every function that calls anything |

x16 and x17 are the two registers the platform lets a linker's call
stub destroy (the import stubs of `macho.azl` use x16), so nothing may
live in them across a call anyway: they are the natural scratch pair.
Slot-to-slot moves use x17 and not x16, because x16 may hold a value
in the middle of a parallel move.

The convention between functions of the module: arguments 1 to 8 in x0
to x7, results 1 to 4 in x0 to x3, the rest of both in the caller's
frame at `[sp]`; x19 to x28 survive. `main` therefore keeps the
platform's rules towards the runtime that calls it. It is fixed, but
it is in one place (`sa_call`, the entry block and `ret` in
`sa_function`, `SA_CALLER_SAVED`); summaries of which registers a
callee really destroys would go into `mf.clob` of `SA_CALL`.

The frame, from sp up: outgoing stack arguments, 48 bytes of scratch
for string headers, spill slots, frame objects, saved registers. sp is
a multiple of 16 always and does not move in the body. A frame of at
most 504 bytes with no outgoing stack arguments has the saved pairs at
the bottom, so one `stp ..., [sp, #-n]!` makes it and one `ldp ...,
[sp], #n` removes it; other frames save first and then lower sp. A
frame above a page is lowered a page at a time with a store to each
(`ea_grow_stack`): the runtime calls a fault within 64 KiB of sp a
stack overflow, and the default generator's promise that no frame
exceeds 16 KiB does not hold for IR with large frame objects.

### The runtime

The ARM64 runtime is the default generator's, appended after the
program's code exactly as `a_generate` does, with its relocations. Its
routines take the address for what they give in **x8**, not as a last
argument: `sa_rt_has_out` says which. The first eleven routines of the
IR: `alloc`, `free`, `print_i64`, `print_bool`, `exit` directly;
`print_str`, `eprint_str`, `str_eq`, `str_append` through string
headers in the scratch area; `memcpy` is the `__memcpy` of
`BK_HELPERS`; `realloc` is the system's (import 3 of `art_imports`,
`SA_CALLIMP`), followed by a test for null that stops with "out of
memory" — the `__realloc` of `BK_HELPERS` reads the Linux x86-64
allocator's block header and is skipped here. Traps 0 to 2 are one
`bl` to the runtime's own routines; the others print their message
through `eprint_str` and exit with 1.

Linking: for macOS a `CodeFile` goes to `build_macho`, unchanged, which
also signs. For Linux, `bka_link_linux` writes what `a_build_linux`
writes; it is a copy for a list of words, because `arm64_link.azl`
takes the default generator's `AGen` and with it the whole front end
(`hex_lines` lives in `self_main.azl`). When the back end moves into
the compiler proper, call `a_build_linux` and delete the copy.

### Measured

Instructions in the emulator, whole process, macOS:

| Kernel (`tests/opt/kernels`) | `azol build` of the benchmark | the kernel through this back end | ratio |
|---|---|---|---|
| `fib40` | 3,146.0 M | 2,814.9 M | 0.89 |
| `loops` | 262.3 M | 234.2 M | 0.89 |
| `sieve` | 760.3 M | 759.1 M | 1.00 |
| `sort` | 698.6 M | 806.2 M | 1.15 |

`fib40` is 17 instructions for a call and its base-case call together
where the default generator needs 19; `loops` is nine instructions a
turn against ten. `sieve` and `sort` are the kernels that read the
list's header and check the index at every access, exactly as lowering
writes it; the default ARM64 generator remembers an index it has
checked and a header it has read, and reads address and length with
one `ldp`, and the back end cannot know any of that. In `sort` the
scanning loops are eight instructions here against seven there, and
the swap is twice as long. It is `gvn`, `licm` and `bce` that are
missing, not registers: for what the IR says, the code is short.

The compiler building itself (742 functions), the build for macOS run
in the emulator:

| Compiler built by | Instructions | Code |
|---|---|---|
| the default generator | 188.7 M | 536,196 bytes |
| lowering + this back end | 194.7 M | 480,368 bytes |
| lowering + `sroa` + this back end | 133.7 M (0.71) | 399,356 bytes |

Lowering alone leaves every variable in frame memory, which the
default ARM64 generator does not do (it keeps the variables the
checker chose in x19 and up); that is why the middle row loses here
while it won on x86-64, whose default generator is weaker. With
promotion the back end is ahead by more than a quarter. Of this, the
layout in chains gave 6 M instructions in both rows, the constants for
loops and `mf_hint_first` less than 1 M together.

### Speed of the back end

Selection, allocation and emission of the lowered compiler (742
functions, 122,457 lines of IR), by the tool's own clock, the best of
eight runs on a loaded machine, both back ends in the same runs:

| Target | Select | Allocate | Emit | Functions/s | Source lines/s |
|---|---|---|---|---|---|
| aarch64-macos | 31 ms | 57 ms | 7 ms | 7,800 | 215,000 |
| x86_64-linux | 34 ms | 45 ms | 10 ms | 8,300 | 229,000 |

Selection with its extra folds and its layout, and emission with its
second attempt for far jumps, cost nothing measurable. Allocation is
a quarter slower because
there are 26 registers to hand out instead of 14: `ra_try_free` and
`ra_take` walk the target's list for every interval. If that matters
later, the fix is in the allocator (a set of free registers instead
of a walk), not here.

### Additions to shared files

- `regalloc.azl`: one case in `ra_resolve`. A part of an interval that
  starts exactly at the value's definition needs no move (the
  definition writes the register); it used to be refused as "an
  interval moves to a register in the middle of an instruction". It
  happens when a block that uses a value is laid out before the block
  that defines it and the scan splits the interval at the hole. The
  x86-64 back end hit the same error on the promoted compiler
  (`ck_match_stmt`); with the case added both build it, pass
  `ra_check`, and the results build the compiler byte for byte.
- `mach.azl`: `mf_hint_first`, a wish that goes in front of earlier
  ones. A value copied to a fixed register (an argument, a result)
  wishes for that register before it wishes for its operand's; without
  it `fib` had two more `mov`s per call.
- `back_tool_main.azl`: `--target`, and two more embedded files.
- `tests/opt/run_back_tests.py`: the list of sources of `azol-back`
  (the tool now holds both back ends). `run_opt_tests.py`: the call of
  the ARM64 tests.

### What later waves must know

- The x86-64 and ARM64 selectors share no code but have the same
  shape, function for function (`sx_`/`sa_`, `em_`/`ea_`,
  `bk_`/`bka_`); a change to one usually has a twin.
- `SaFunc.calls` means "x30 must be saved": any call, of a function or
  of the runtime. A trap is not a call. The test before the frame is
  shrink-wrapping for one shape only; the general thing (save where
  the first call is, or split the interval of an argument so that it
  stays in its register until then) is still to do, and needs the
  allocator.
- The constant pool is a stand-in for `licm` on constants, done blind:
  it does not know how hot a loop is, and a constant that lives across
  a call takes a callee-saved register. Once the allocator can
  rematerialise (make a constant again instead of loading it from a
  slot), the pool should define its constants as rematerialisable and
  the limit of eight can go.
- Division by a constant is still `sdiv`; rustc multiplies. That
  belongs in a pass (`simplify`), not here, but `umulh`/`smulh` are
  selected and ready for it.
- The layout in chains (`sa_follower`) knows nothing about how often
  a way out is taken beyond "traps are cold" and "deeper in loops is
  hotter". It is twenty lines and would serve `sel_x64.azl` unchanged,
  which still has the IR's order.
- Flags are reused only inside a block and only for a comparison with
  zero. `ccmp` chains for `a && b`, `csinc`/`cneg`/`csetm` for selects
  of 0/1/-1, and `adds`/`subs` for signed comparisons other than
  with zero are not done.
- Loads and stores are paired only when they are adjacent in the IR;
  the allocator's own spill moves are not paired.
- A function with more than 32,760 bytes of outgoing arguments,
  scratch and spill slots is refused (the slots must be reachable with
  one instruction, because a move may not use a second register).

### Doubts

- Everything was run in Unicorn, not on a processor. The emulators
  follow the manual, so an encoding that is wrong in a way the manual
  calls unpredictable would show there too, but two things only a Mac
  can say: that the system's signal handling sees the stack overflow
  of these frames as the runtime expects (the emulator models the
  handler's rule, not the kernel), and how fast the code is.
  `tests/opt/mac_check.sh` includes `stack_overflow` for the first.
- `realloc(p, 0)` is the system's to define. Lowering never emits
  `realloc`, and the random generator calls it with 40, 200 and 70,000
  bytes only, so a size of 0 is untested.
- The kernels' instruction counts are from compiler/emu_arm64.py with a
  counting hook, which gives the same numbers as the checking emulator
  (compared on `loops`), not from hardware counters. An instruction
  count says nothing about how a real processor likes the code.
