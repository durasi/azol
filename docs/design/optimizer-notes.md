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

## The passes effects and gvn

`compiler/opt/pass_effects.azl` and `compiler/opt/pass_gvn.azl`, both
passes on the whole module, in the order as

    simplify, sroa, simplify, effects, gvn, simplify, dce, dse, dce, simplify

### What a summary holds

For every `ptr` parameter the five bits of `param_fx`, now worked out
from the body instead of taken from the declaration: the lowering
stage writes what the language allows (`read`, or everything for a
`mut` parameter and for the space of a result), and `effects` takes
away what the function and the functions it calls never do. A bit is
never added. The bits mean: `read` and `write`, the bytes the
parameter points to; `writebuf`, a buffer those bytes own, at any
depth; `resize`, a buffer may be somewhere else or of another size
afterwards; `free`, one may be gone. For the function as a whole four
more bits, which have no place in the intermediate form and live in
the table `fx_summaries` gives: it may stop the program (a `trap`, a
routine that can run out of memory, `exit`), change the allocator's
state, print or write a file, or not come back (a loop, a cycle of
calls, `exit`).

Every function starts with nothing and bits are added round after
round until a round adds none: the smallest answer that agrees with
every body, which is what lets `quicksort`, calling itself, come out
as `read writebuf`. Where an address comes from is followed through
`gep`s to a parameter, a `frame`, new memory, or a `load` whose region
starts at a parameter (then it is inside a buffer the parameter owns,
M3, and a callee's `write` is a `writebuf` here). Anything else is an
address the pass cannot name, and what is done through it counts for
every parameter the language lets change. One rule is there only for
safety: a store to a buffer under a local's name through an address
that cannot be named counts as `writebuf` for every such parameter (a
buffer moved out of a parameter and back would be written that way;
the lowering stage copies instead, but nothing in the form says so).

On the compiler 400 of the 578 parameters that were declared with
every bit lose some: 650 parameters are `read`, 173 `read write
writebuf resize`, 97 `write` alone (results), 178 keep everything. On
`big.azl` all 4,000.

### Who uses them

- **sroa**: nothing changed. `sroa_param_is_stable` and
  `sroa_arg_writes` read `param_fx`, which is now the summary.
- **gvn**: below.
- **dce** (`pass_dce.azl`, one function added at the end,
  `ir_dce_calls`): a call whose results nothing uses goes when the
  callee only reads through its parameters and has none of the four
  bits.
- **dse** (`pass_dse.azl`): `ir_dse` is now `ir_dse_with(f,
  fx_no_calls())`; `ds_private_objects`, `ds_frames` and `ds_walk` take
  the callees' summaries as one more argument, and the new `ds_call`
  makes a call a reader of a frame object from the address it is given
  on, when the callee's parameter says `read`. So an object whose
  address only goes to calls is followed over the whole function
  instead of block by block. With no summaries the pass does exactly
  what it did.

The last two need the table, which a pass on one function does not
have, so `effects` runs them itself, on the functions that call
anything. On the compiler that removes 27 stores and no call (the
front end leaves no call without a use); it is there for what inlining
will leave.

### gvn: the rules

One walk over the dominator tree with a table that grows on the way
down and is undone on the way up.

1. An instruction that only computes, when the same operation with the
   same operands is above it: arithmetic, comparisons, `gep`, `frame`,
   constants. Operands that do not care about their order match either
   way; the instructions are left in the form `simplify` gave them.
2. A `load`, when what memory holds there is known from an earlier
   `load` or `store` of the same bytes (same address value, same
   constant offset through `gep`s with a constant index, same width).
   A stored constant reaches the load too.
3. A `br` on a condition that is known because the only way into the
   block, or into one that dominates it, is one side of a `br` on the
   same condition. With 1 and 2 this is the second bounds check of
   `xs[i]`: same comparison, same length, and the way to the `trap` is
   not taken. A check is removed, never moved, so the rule of section
   3.5 about what lies between two checks does not come into it.
4. A `br` on `eq x, K` or `ne x, K` below the true side of `eq x, C`:
   the arms of a `match`.

What ends what is known about memory:

- A store through the same address value: the bytes it overlaps, and
  nothing else, whatever the regions say.
- A store through another address: everything in the same region. For
  an entry in a buffer also a store to any other buffer, unless the
  entry's buffer belongs to a parameter; and a store to a region that
  owns the buffer.
- A call: `write`, `resize` or `free` through an address made from a
  `frame` or a parameter ends what is known of that object; any of the
  four ends what is known of its buffers and of every buffer of a
  local. A callee that only reads ends nothing. An address the pass
  cannot name, given to a callee that may write through it, ends
  everything except read-only data.
- Between a block and its dominator: the stores and calls of every
  block on a way from one to the other, looked for backwards from the
  block (256 blocks at most, then everything is forgotten).

Why buffers of locals are not told apart by region: `sroa` makes a
local list's header three values, and after that a buffer goes from
one local to another without a single store. `buf(frame 2)` and
`buf(frame 3)` can then be the same memory at different times, with
nothing in between to say so. A parameter's header stays in memory,
so a buffer cannot leave a parameter without a store to it, and that
store ends the entry. This is the reason for the one loss in `soa`
below. It is also why `dse` treats all buffers as one.

### Deliberately not done

- Nothing is moved: a load or a check that is the same on every trip
  round a loop stays in the loop (`licm`), and where two ways into a
  block bring two different values no block parameter is put in (no
  partial redundancy).
- No `store` is removed, also not one that stores what is there.
- A store narrower than eight bytes is not forwarded, and a load of
  another width than the entry never matches.
- Comparisons that imply each other (`slt i, n` and `ult i, n` for an
  `i` that is not negative) are `range`'s and `bce`'s.
- Only the last 64 things known about memory are looked at.

### Proof (x86-64 Linux, 12 November 2026)

`python3 tests/opt/run_gvn_tests.py`:

- 47 hand-written cases in `tests/opt/gvn` and `tests/opt/effects`, 21
  of them `no_` cases where a rule must not fire, each with its reason
  in the first line: a call that may write the header, `list_push`
  between two loads and between two checks, a store through another
  address into the same list, two locals' buffers, a parameter's header
  written in between, a store on one side of a branch, a store further
  down a loop, a one-byte store, a load of another width, an address
  that cannot be named, a condition with two ways in, the false side
  of `eq`, operands of `sub` the other way round, the same expression
  on two sides of a branch; for `effects` a callee that may trap, loop,
  call itself, print, write or allocate, and a callee that reads what
  a store wrote.
- 3,000 random functions in 742 modules made by the script: two lists
  read and written through their headers with and without bounds
  checks, variables in frame memory, calls, pushes, branches, loops,
  and the same field read on both sides of a call, a push or a store.
  Each module prints the same before the passes and after `gvn`, after
  `effects,gvn` and after the whole order with `--fixpoint`, always
  with `--verify-each`; 185 of them also as native code, 58 programs
  built for ARM64 and run in the two checking emulators.
  `effects,gvn` leaves 44,434 of 111,885 loads.
- The 145 programs of the lowering list through the whole order:
  interpreted, and built by `azol-opt` with all registers and with
  three (`--stress`), like their default builds.
- The compiler built by `azol-opt --fixpoint --verify-each` builds
  `examples/hello.azl` and itself into the bytes `azol` writes.
- `tests/fuzz/fuzz.py --lower --opt`, seeds 7001 to 11383: 4,200
  programs, 4,201 runs through the passes, interpreted and as native
  code (`--fixpoint --verify-each --check`, also `--stress`). One
  finding, which is not the passes': seed 9428, a program that ends
  in `azol: stack overflow`, gave something else once in the
  interpreted run while three other test runs were loading the two
  processors. Run again on its own (`--only 9428`), with the build
  before these passes and with this one, everything agrees. Nothing
  else was found.
- `python3 tests/opt/run_opt_tests.py`: ALL OPT TESTS PASSED;
  `sh build.sh`: the fixed point, no seed note; `sh test.sh`: ALL
  TESTS PASSED.

Twenty-six rules were broken on purpose, one at a time (a call that
ends nothing, a store through the same address that ends nothing,
regions trusted for locals' buffers, the walk between blocks left out,
a condition taken the wrong way or with two ways in, narrow stores
forwarded, the table not undone, the rounds of `effects` stopped after
one, traps, loops and recursion forgotten, and so on). The cases
refuse every one; three of them only after a case was added for
exactly that (`no_byte_then_word`, `no_header_written`, and reads
around calls in the random modules).

### Measured

`python3 bench/gate.py --instructions --azol-opt <azol-opt>`,
instructions on the small sizes, before (the order without the two
passes) and after:

| Program | azoL default | azol-opt before | azol-opt after | Rust `-O` | before / default | after / default | before / Rust | after / Rust |
|---|---|---|---|---|---|---|---|---|
| `append` | 46,499,388 | 45,509,389 | 45,509,389 | 28,876,748 | 0.98 | 0.98 | 1.58 | 1.58 |
| `bfs` | 27,824,558 | 21,150,997 | 20,175,212 | 19,536,738 | 0.76 | 0.73 | 1.08 | 1.03 |
| `bignum` | 16,720,320 | 12,544,672 | 11,364,906 | 9,138,149 | 0.75 | 0.68 | 1.37 | 1.24 |
| `binary_trees` | 98,131,889 | 81,318,924 | 81,318,925 | 171,954,443 | 0.83 | 0.83 | 0.47 | 0.47 |
| `bits` | 223,001,019 | 139,001,003 | 139,001,003 | 47,442,104 | 0.62 | 0.62 | 2.93 | 2.93 |
| `crc32` | 103,161,283 | 75,897,779 | 75,897,779 | 60,740,975 | 0.74 | 0.74 | 1.25 | 1.25 |
| `dijkstra` | 133,587,177 | 98,839,075 | 83,428,126 | 31,869,061 | 0.74 | 0.62 | 3.10 | 2.62 |
| `dp_lcs` | 107,259,354 | 70,057,524 | 62,214,510 | 28,651,371 | 0.65 | 0.58 | 2.45 | 2.17 |
| `fannkuch` | 164,840,557 | 167,334,500 | 143,857,362 | 106,700,727 | 1.02 | 0.87 | 1.57 | 1.35 |
| `fib40` | 36,349,435 | 33,656,893 | 33,656,893 | 26,854,973 | 0.93 | 0.93 | 1.25 | 1.25 |
| `json` | 76,057,789 | 67,463,748 | 66,112,503 | 67,909,124 | 0.89 | 0.87 | 0.99 | 0.97 |
| `knapsack` | 70,810,081 | 64,133,921 | 57,514,365 | 76,174,849 | 0.91 | 0.81 | 0.84 | 0.76 |
| `loops` | 43,536,458 | 30,363,016 | 30,363,016 | 38,222,825 | 0.70 | 0.70 | 0.79 | 0.79 |
| `lru` | 77,599,843 | 64,631,742 | 63,844,821 | 100,614,841 | 0.83 | 0.82 | 0.64 | 0.63 |
| `map_int` | 86,809,454 | 74,120,785 | 72,974,519 | 34,312,308 | 0.85 | 0.84 | 2.16 | 2.13 |
| `matcher` | 99,557,762 | 79,664,938 | 79,664,938 | 80,962,384 | 0.80 | 0.80 | 0.98 | 0.98 |
| `matmul` | 72,039,898 | 59,599,824 | 59,509,821 | 37,856,938 | 0.83 | 0.83 | 1.57 | 1.57 |
| `particles` | 89,014,861 | 168,651,963 | 124,606,587 | 60,815,270 | 1.89 | 1.40 | 2.77 | 2.05 |
| `prefix` | 48,525,007 | 43,723,094 | 42,724,045 | 21,530,608 | 0.90 | 0.88 | 2.03 | 1.98 |
| `sha256` | 416,214,736 | 248,540,656 | 253,406,697 | 95,152,751 | 0.60 | 0.61 | 2.61 | 2.66 |
| `sieve` | 69,691,579 | 63,790,802 | 63,790,802 | 49,554,538 | 0.92 | 0.92 | 1.29 | 1.29 |
| `soa` | 93,341,623 | 123,294,537 | 84,294,537 | 23,085,052 | 1.32 | 0.90 | 5.34 | 3.65 |
| `sort` | 64,098,134 | 55,347,623 | 51,792,283 | 50,939,112 | 0.86 | 0.81 | 1.09 | 1.02 |
| `sort_lib` | 171,701,091 | 134,621,991 | 133,092,201 | 40,767,407 | 0.78 | 0.78 | 3.30 | 3.26 |
| `tokenizer` | 41,699,672 | 36,255,223 | 36,466,223 | 10,023,257 | 0.87 | 0.87 | 3.62 | 3.64 |
| `tree` | 21,364,019 | 16,973,257 | 16,973,257 | 26,699,739 | 0.79 | 0.79 | 0.64 | 0.64 |
| `wordfreq` | 126,818,951 | 112,108,898 | 111,265,041 | 58,934,592 | 0.88 | 0.88 | 1.90 | 1.89 |
| `words` | 49,096,060 | 43,890,710 | 43,730,714 | 18,479,005 | 0.89 | 0.89 | 2.38 | 2.37 |

Geometric means: against the default build **0.853** before and **0.808** after; against `rustc -O` **1.574** before and **1.491** after (the default generator itself: 1.845).

Besides the four below: `fannkuch` 1.02 to 0.87, `dijkstra` 0.74 to
0.62, `dp_lcs`, `knapsack` and `bignum` by about a tenth each; the
generated code was not read for these, the passes' counts say it is
the second and third access to an element. `sha256` and `tokenizer`
get a little worse (2% and 0.6%), probably because values that are
shared live longer; that was not looked into.

The four that were asked about, by the instructions of the hot
function in the intermediate form (ratios are against the default
generator):

| | before | after | what is left |
|---|---|---|---|
| `particles`, `step` | 75 | 38 | The loop body was 12 bounds checks, 12 `gep`s, 8 loads of fields and 4 stores; it is one check, one `gep`, 4 loads and 4 stores. Left for `licm`: the length and the buffer address, read from the parameter on every trip (the parameter is `read writebuf` now, so nothing in the loop changes them). Left for `range`/`bce`: `ult i, len` under `slt i, len`. Against the default generator 1.89 before, 1.40 after; the rest is the back end's (the default generator keeps the checked element address in a register over a run of accesses, which is what this is now, and still wins). |
| `soa`, `step` | 55 | 39 | Each of the four lists is checked and addressed once per trip instead of two or three times. One load is read again that need not be: the four lists are fields of one parameter, so their buffers are all `buf(param 0)` and a store to one ends what is known of the others. Left for `licm`: four lengths and four addresses. 1.32 before, 0.90 after. |
| `sort`, `quicksort` | 58 | 44 | The swap was 4 checks, 4 `gep`s, 2 loads and 2 stores and is 2 stores: the two scans above it have the addresses and the values. `quicksort` is `read writebuf`, so its callers keep the list's address and length across it. Left for `bce`: the check in each scan loop. 0.86 before, 0.81 after; 1.02 of `rustc -O`. |
| `sieve`, whole program | 72 | 72 | Nothing to find: the marking loop was already one comparison, one `gep` and one store. What is left there is `bce`'s (the comparison) and the strength of the index. |

Cost, callgrind instructions of `opt-tool` less the same run with no
pass:

| | IR instructions in | `effects` | per instruction | `gvn` | per instruction | whole order before | after |
|---|---|---|---|---|---|---|---|
| the compiler (20,428 lines) | 71,934 | 117.8 M | 1,640 | 215.8 M | 3,000 | 921 M | 1,409 M |
| `bench/generated/big.azl` (44,004 lines) | 145,001 | 117.1 M | 810 | 298.7 M | 2,060 | 1,363 M | 2,103 M |

The whole order costs half as much again as it did (the two passes,
one more `simplify` after `gvn`, and `dce` and `dse` running again on
what changed). Wall clock, on a machine the other agents were loading at the time
(best of five, the passes alone as `azol-opt build --stats` prints
them): the compiler 289 ms before and 403 ms after, 71,000 and 51,000
source lines a second through the passes; `big.azl` 569 ms and 723 ms,
77,000 and 61,000. The ratios (1.39 and 1.27) are worth more than the
numbers.
Per instruction `gvn` is where `sroa` is (2,960), and for the same
reason a good part of it is not the pass: each function gets its own
tables from `filled`, which grows them by doubling. Before the tables
were a log that is undone on the way up the tree, every block copied
what was known about memory, and the pass cost 5,800 per instruction.
The walk between a block and its dominator is 42 M of the 216 M on the
compiler; `ir_cfg` and the callees' table are 32 M. The cost of
`effects` includes the second `dse` it runs; how much of it that is
was not measured.

### Edits to files that are not these passes'

- `pipeline.azl`: the two names in `pl_run_module_pass`,
  `PL_MODULE_PASSES` and `PL_ORDER`, with `simplify` after `gvn`.
- `pass_dce.azl`: `ir_dce_calls` added at the end; nothing else.
- `pass_dse.azl`: `ds_call` added, one more argument for
  `ds_private_objects`, `ds_frames` and `ds_walk`, `ir_dse_with`; the
  three lines in `ds_private_objects` that let a frame object given to
  a call be followed.
- `tests/opt/run_pass_tests.py`: the two files in `PASSES`.
  `tests/opt/run_opt_tests.py`: three lines that call
  `run_gvn_tests.run`.
- Nothing in `ir*.azl`.

### What the next passes should know

- After `effects`, `param_fx` is what the function does, not what it
  may. A pass that adds a store or a call through a parameter has to
  run `effects` again or put the bit back, or the verifier refuses the
  function (it checks `read`, `write` and `writebuf`).
- `fx_summaries(m)` gives the four whole-function bits; `fx_calls`
  turns the table into the view of one function's callees that
  `fx_arg_bits` and `fx_call_flags` read. `licm` needs exactly these
  to know what a call in a loop leaves alone.
- `gvn` leaves the header loads of a loop inside it, once. With the
  parameter no longer `write` or `resize` they are the same on every
  trip, and `gv_store_ends` and `gv_call` say what could change them.

### Doubts

- The rule for parameters' buffers rests on a buffer never leaving a
  `mut` parameter without a store to the parameter. The lowering stage
  copies on `var t = a` and writes the header on `a = t`; the tests
  with such programs pass, and the case `no_header_written` holds the
  rule from the pass's side, but the form itself does not state it.
- An address read from a frame cell is "cannot be named" both for
  `effects` and for `gvn`. That is safe and costs precision where a
  pattern binds a part of a value and the address is kept in memory.
- Removing a call of a function that reads memory assumes its loads
  cannot stop the program, which holds for what the lowering stage
  writes.
- The cost. Half as much again for the whole order is more than these
  two passes should take; reusing the tables from one function to the
  next, and one `dse` instead of two, are the first things to do.

## Loops: range facts, `licm`, `bce` (wave 3)

Files: `compiler/opt/pass_range.azl` (no pass: a set of facts others ask
for), `pass_licm.azl`, `pass_bce.azl`. Tests: `tests/opt/range`,
`tests/opt/licm`, `tests/opt/bce` and `tests/opt/run_loop_tests.py`
(called from `run_opt_tests.py`). In the pipeline's order they come
after the first clean-up:

    ..., simplify, licm, bce, simplify, dce, licm, bce, simplify, dce

The second round is there because each half feeds the other: `bce`
needs lengths that `licm` moved out of the loop, and `licm` can move a
header load once `simplify` has merged the blocks a removed check stood
between; the limits `bce` works out in front of an inner loop are moved
further out by the second `licm`.

### Range facts (`ir_range`, `rg_at`, `rg_less`)

For every value a span `lo..hi` that holds wherever the value exists,
and on top of it two questions answered for a block: `rg_at(x, blk)`,
the span of `x` when `blk` runs, and `rg_less(a, ka, b, kb, blk)`,
whether `a + ka < b + kb` is certain there as whole numbers.

- Spans come from constants, types (a `bool`, a narrow `load`),
  arithmetic that provably does not wrap (a result that may wrap has no
  span at all), `and` with a non-negative number, `srem`/`sdiv` by a
  positive one, `select` (the smaller arm's upper end for the shape of
  `min`, the larger's lower end for `max`).
- A length is the right side of an unsigned `<`: 0 to 2^56. This rests
  on an invariant of the lowering stage that nothing checks: **it
  compares unsigned only against lengths and capacities**. A pass that
  makes an unsigned comparison against anything else breaks `range`.
- A branch gives facts to the block only it leads to and to everything
  that block dominates, as relations `u < w + k`. They are stored per
  block once (`fact_s`, `fu`, `fw`, `fk`); a question walks up the
  dominators through the blocks that have any (`above`), at most
  `RG_WALK` = 16 of them, and not at all for a value no relation is
  about (`told`). Before these three the pass took minutes on the
  compiler's large functions.
- A relation about `x + c` also places `x` (adding a constant and
  wrapping loses nothing): the check of `xs[j - 1]` bounds `j`.
- Induction: a header parameter passed round as itself plus a step that
  is never negative never gets smaller, **if the addition cannot wrap
  where it stands**; that needs an upper end for the parameter at the
  addition (a `<` test, or a bounds check, above it) with room for the
  step. `i < n` gives room for a step of 1; a step `s` from outside
  needs a length-sized bound. The same downwards. A counter that could
  wrap gets nothing (case `bce/no_wrap_counter`: a step of 2^62 under a
  test against the largest number wraps below 0, where the check must
  still fail).
- `rg_less` chains: constants taken off both sides (when that cannot
  wrap), spans, the arms of a `select`, the values passed into a block
  that is no loop header (each where it is passed: this is how the
  smaller of two lengths chosen by an `if` is below both), the start of
  a counter (a counter going down is never above its start), and the
  relations of the branches above, to depth 3.

### `licm`

Rules L1 to L4 are in the file's first comment. What deserves a note:

- Preheaders are made only when something moves; an empty one would be
  folded away by `simplify` and make the pipeline's fixed point
  oscillate. A made preheader takes the header's parameters, because
  the blocks outside may pass different values.
- What the loop may write is decided by regions: a `store` writes its
  region; a call writes, through each address it is given, that region
  and everything below it (`lc_call_arg_fx` is the one place to plug
  summaries in: today a function is assumed to do everything through
  every address, a runtime routine what `ir_rt_arg_fx` says). A call
  that is given no address writes nothing a loop can load (fact M9).
  Two different buffers are taken as not overlapping (fact M2), which
  is more than `dse` assumes; the argument is that a moved buffer's old
  owner is dead, so a loop cannot read through both names.
- **A load is made early only when the bytes are certain to be stored
  and to mean what the type says.** The first version moved any load
  from a frame object or from behind a parameter; the fuzzer (seed
  880345) found a `bool` field of one variant loaded in front of a loop
  while the memory held another variant. The rule now: in the header
  block any load may move (it runs whenever the preheader does); from
  anywhere else only the slots of a list header (`lc_header_slot`).
- L4 (the preheader already has the same instruction) is a small piece
  of value numbering; it is what lets `rows[i][j]` lose its check
  before a `gvn` pass exists, and costs nothing when one does.

### `bce`

B1 removes a check that `0 <= index` and `index < len` prove. B2 is the
only way a check is moved, and its shape is forced by the rule of order
(O1: a program that stops must stop at the same point, with the same
message, after the same output):

    before the loop:   limit = min(n, len)          (or the forms below)
    the loop's test:   j < limit                    instead of j < n
    in the round:      no check
    on the way out:    if j < n { trap index }      the old test, made once

It applies when the check is the first thing a round does: between the
loop's test and the check there are only instructions that change
nothing (no store, no call, no other check, no way out), so stopping in
front of the round is the same as stopping at its check. All rounds
before the failing one run in full. The three index shapes (the counter;
a value from outside the loop; the counter plus a value from outside
and constants) and their limits are in the file's first comment.

What is deliberately not done:

- **No test of the whole range before the loop.** `for i in 0..n {
  print(xs[i]) }` with a list that is too short must print the first
  elements and then stop; a test in front would stop before the first
  print, or (as a version of the loop) cost code size. B2 gets the same
  inner loop without either.
- No move of a check across a call, a store, another check, or an
  earlier way out of the round (cases `bce/no_*`).
- No reasoning about a length that is loaded inside the loop: if `licm`
  could not move the load, something in the loop may write the header.

### Proof

    python3 tests/opt/run_loop_tests.py

- 58 hand-written cases (`.ir`, `.expected.ir`, `.runs`), 25 of them
  where a rule must not fire; each first line says what the case shows.
- 1,500 random loops over one or two lists (counters up and down, steps
  from outside, starts that may be negative, tests against a number, a
  length, the smaller of two; rounds that print, store or leave early;
  lists that are too short): the same before and after in the
  interpreter for seven argument rows each, 750 runs also as native
  code; about 4,700 of the runs stop with an index out of range, with
  the same output before it.
- 8 azoL programs of loop shapes, 7 of which stop with an index out of
  range after printing: default build against `azol-opt` (also to a
  fixed point, with three registers, and interpreted).
- Through `run_pass_tests.py`, which uses the pipeline's order: the 145
  whole programs (interpreted after the passes, native, three
  registers), random modules with a second run that must change
  nothing, and the compiler built by `azol-opt`, which builds
  `examples/hello.azl` and itself into the bytes `azol` writes.
- The fuzzer of tests/fuzz with `--lower` and `--opt` (every program
  also through the passes: interpreted, native, three registers):
  5,200 programs (seeds 880001 to 885437), 0 findings. Its one finding on the way (seed 880345, the early `bool`
  load above) is now the case `licm/no_bool_elsewhere`.
- ARM64: the 8 loop programs and the small `sieve` and `matmul`, their
  optimised IR built by `azol-back` for aarch64-macos and
  aarch64-linux, with all registers and with three: 40 runs in the
  checking emulators the same as `azol build --target`, the stops
  included.
- `sh build.sh` reaches its fixed point (823,296 bytes, no seed note)
  and `sh test.sh` passes: nothing outside `compiler/opt` changed.

### Measurements

Instructions executed on the small sizes (`python3 bench/gate.py
--instructions --no-time --azol-opt ...`, callgrind), `azol-opt` with
the pass list before this wave and with it:

| Program | default azoL | azol-opt before | azol-opt after | after / before | Rust `-O` | after / Rust |
|---|---|---|---|---|---|---|
| `append` | 46,499,388 | 45,509,389 | 45,479,392 | 1.00 | 28,876,752 | 1.57 |
| `bfs` | 27,824,558 | 21,150,997 | 20,760,442 | 0.98 | 19,536,751 | 1.06 |
| `bignum` | 16,720,320 | 12,544,672 | 9,004,098 | 0.72 | 9,138,150 | 0.99 |
| `binary_trees` | 98,131,889 | 81,318,924 | 82,667,880 | 1.02 | 171,954,449 | 0.48 |
| `bits` | 223,001,019 | 139,001,003 | 139,001,003 | 1.00 | 47,442,108 | 2.93 |
| `crc32` | 103,161,283 | 75,897,779 | 65,412,022 | 0.86 | 60,740,965 | 1.08 |
| `dijkstra` | 133,587,177 | 98,839,075 | 86,194,303 | 0.87 | 31,869,075 | 2.70 |
| `dp_lcs` | 107,259,354 | 70,057,524 | 66,145,174 | 0.94 | 28,651,375 | 2.31 |
| `fannkuch` | 164,840,557 | 167,334,500 | 146,751,461 | 0.88 | 106,700,741 | 1.38 |
| `fib40` | 36,349,435 | 33,656,893 | 33,656,893 | 1.00 | 26,854,963 | 1.25 |
| `json` | 76,057,789 | 67,463,748 | 67,056,926 | 0.99 | 67,909,128 | 0.99 |
| `knapsack` | 70,810,081 | 64,133,921 | 56,218,157 | 0.88 | 76,174,863 | 0.74 |
| `loops` | 43,536,458 | 30,363,016 | 30,363,016 | 1.00 | 38,222,815 | 0.79 |
| `lru` | 77,599,843 | 64,631,742 | 62,672,311 | 0.97 | 100,614,845 | 0.62 |
| `map_int` | 86,809,454 | 74,120,785 | 71,843,329 | 0.97 | 34,312,314 | 2.09 |
| `matcher` | 99,557,762 | 79,664,938 | 80,609,776 | 1.01 | 80,962,402 | 1.00 |
| `matmul` | 72,039,898 | 59,599,824 | 44,332,613 | 0.74 | 37,856,942 | 1.17 |
| `particles` | 89,014,861 | 168,651,963 | 116,631,677 | 0.69 | 60,815,262 | 1.92 |
| `prefix` | 48,525,007 | 43,723,094 | 30,726,061 | 0.70 | 21,530,612 | 1.43 |
| `sha256` | 416,214,736 | 248,540,656 | 243,740,131 | 0.98 | 95,152,755 | 2.56 |
| `sieve` | 69,691,579 | 63,790,802 | 51,839,925 | 0.81 | 49,554,528 | 1.05 |
| `soa` | 93,341,623 | 123,294,537 | 60,340,584 | 0.49 | 23,085,066 | 2.61 |
| `sort` | 64,098,134 | 55,347,623 | 47,242,426 | 0.85 | 50,939,116 | 0.93 |
| `sort_lib` | 171,701,091 | 134,621,991 | 123,288,236 | 0.92 | 40,767,417 | 3.02 |
| `tokenizer` | 41,699,672 | 36,255,223 | 34,922,223 | 0.96 | 10,023,249 | 3.48 |
| `tree` | 21,364,019 | 16,973,257 | 17,235,393 | 1.02 | 26,699,743 | 0.65 |
| `wordfreq` | 126,818,951 | 112,108,898 | 111,259,094 | 0.99 | 58,934,612 | 1.89 |
| `words` | 49,096,060 | 43,890,710 | 46,315,474 | 1.06 | 18,478,995 | 2.51 |

Geometric means over the 28 programs: after / before 0.89; azol-opt / Rust `-O` 1.57 before and 1.40 after; azol-opt / default azoL 0.85 before and 0.76 after.

The five programs looked at closely:

| Program | before | after | after / Rust `-O` (was) | what happened |
|---|---|---|---|---|
| `sieve` | 63.8 M | 51.8 M | 1.05 (1.29) | Both checks became limits. The marking loop is `store; add; cmp; jl`: 4 instructions a round, 6 before (`cmp; jae` gone). |
| `sort` | 55.3 M | 47.2 M | 0.93 (1.09) | 10 checks in rounds, 3 left: those behind a comparison of two elements. |
| `particles` | 168.7 M | 116.6 M | 1.92 (2.77) | All 12 checks gone (counted loops over lists of one length); the headers' lengths and addresses are loaded once. Still above the default build (1.31): that gap is older than this wave. |
| `soa` | 123.3 M | 60.3 M | 2.61 (5.34) | 11 checks, 2 left; the loads of six list headers left the inner loop. |
| `matmul` | 59.6 M | 44.3 M | 1.17 (1.57) | `a[i * n + k]` lost its check to a limit (third shape); `b[k * n + j]` keeps its own (see below); `c[i * n + j]` is checked once per row element, at the end of the round. |

Checks, counted in the optimised IR of the 28 programs as branches on an
unsigned comparison that lead to `trap index`: 272 before, 132 after
(the tests B2 puts on the way out of a loop are not counted: they run
once per loop, not once per round). How many checks a run executes was
not counted separately; the instruction counts above are the measure.

Cost, on the compiler itself (19,731 lines, 760 functions; best of 12
runs of `azol-opt build --stats` on a busy machine): all passes 222 ms
before, 396 ms after. Of the difference `bce` is about 90 ms in its two
runs (372 checks removed or made limits), `licm` 17 ms (975 instructions
moved), and the second round's `simplify` and `dce` about 45 ms. In
lines per second for the passes alone that is 89,000 before and 50,000
after. `bce` is expensive for what it is: three quarters of its time is
answering `rg_at` and `rg_less` for checks that turn out not to be
provable.

### For `ivsr` and `unroll`

- `r.iv_dir`, `r.iv_one`, `r.iv_init`, `r.iv_step` say which header
  parameters are counters, where they start and how large a step is;
  `lc_preheader` gives (or makes) the block to put a new value in;
  `bc_outside` and `lc_invariant` say what is the same in every round.
- After B2 a loop's test is `j < limit` with `limit` at most the length:
  an address `base + 8 * j` is in the buffer for every round, so a
  pointer that steps by 8 can replace the index without a check of its
  own, and the trip count is `(limit - start + step - 1) / step` when
  the step is a constant.
- What `bce` leaves for want of a derived counter: an index `k * n + j`
  (matmul's second factor). With `t = k * n` as a counter of its own
  (`t += n`), B2's third shape applies to `t + j` once a limit for `t`
  can be put in terms of the loop's own counter; that needs a division
  before the loop.
- What it leaves for want of a loop that is known to run a round: a
  check or a load behind another in the first round (`a[i][k]` in a
  third loop: the row's length is loaded from a buffer element, which
  `licm` may not load early). Turning the loop (test before it, body
  first) makes the first round certain; `licm`'s rule L3 then covers the
  body's first block.

### Doubts

- The unsigned-comparison invariant above is assumed, not verified.
- 2^56 as the largest length is an assumption about the allocator, not
  a checked fact; a list of elements of size 0 could in principle be
  longer.
- Facts M2 and M9 (buffers do not overlap; a call writes only through
  what it is given) are relied on for loads; `dse` is more careful.
- `RG_WALK` cuts off facts more than 16 fact-bearing dominators above;
  in the compiler it changed nothing, in other code it may.
- `bce` works the ranges out again after every loop it rewrites; a
  function with very many such loops pays that many times.

## Inlining, tail calls and accumulators (wave 3)

Files: `pass_inline.azl` (a pass on the whole module,
`ir_inline_module(m: mut IrModule, stats: mut InlStats) -> i64`),
`pass_tailcall.azl` (`ir_tailcall(f: mut IrFunc) -> i64`) and
`pass_accrec.azl` (`ir_accrec(f: mut IrFunc) -> i64`, built from the
helpers of `pass_tailcall.azl`); the tests `tests/opt/run_inline_tests.py`
with the cases in `tests/opt/inline`, `tailcall` and `accrec`. The order is
now

    simplify, sroa, simplify, tailcall, accrec, dce, inline, dse, dce, simplify

### inline

**Order.** Tarjan's components of the call graph, callees first. A
function that can reach itself (alone or through others) is never put
anywhere, so every cycle of calls a program has it keeps, and a recursion
that ran out of stack still does. As soon as a function has taken its
callees in, `inline` runs `sroa` and `simplify` on it, so that its own
callers see the size it really has; a function nothing calls any more is
skipped, and at the end everything `main` cannot reach is removed (a
module without `main` keeps all its functions).

**What replacing a call does.** The block of the call is cut at the call
and the rest becomes a continuation block whose parameters are the call's
results; the body's blocks are copied with new values, a parameter being
the argument itself (a `bool` parameter bound to a constant gets a `bconst`
first, since the verifier types `xor 1, 1` as `i64`; a `ptr` parameter
bound to a constant keeps the call); each `ret` jumps to the continuation;
a `trap` stays a `trap` in a block of its own. The callee's frame objects
become new frame objects of the caller and their `frame` instructions go to
the caller's entry block, which is where `sroa` looks for the root of an
object.

**Regions.** `frame N` of the body is the new object. `param K` and `ret`
are replaced by the region the *caller* uses for what the argument points
to. The argument is followed down its `gep`s to its root: a `frame`
instruction gives `frame N`; otherwise an access the caller already makes
through that root gives its region; otherwise a parameter of the caller
gives `param K` (or `ret` for the place of the result); a loaded address
gives `buf(region of the load)`; `addr` gives `data`. When nothing says,
the call stays (`unnamed` in `InlStats`). `buf(r)` maps to `buf` of what
`r` maps to. This is exact because lowering gives every pointer root one
region; two regions of the body that land on one region of the caller only
make later passes more careful. What the body does through a parameter of
the caller must be within the effects that parameter declares, or the call
stays. In the compiler (4,332 calls seen) and the suite (235) no call was
kept for want of a region.

**When.** `inl_wants`, with the numbers at the top of the file:

| | |
|---|---|
| a callee with one call | goes in when it has at most 400 instructions (it then disappears) |
| any other callee | goes in when it has at most 20 instructions, plus 4 for each constant argument, plus 8 for each argument that is the address of a frame object of the caller, plus 30 when the call is in a loop |
| a caller | stops growing at 800 instructions |
| the program | callees that stay may add 60% of its size in all (never reached in the measurements) |
| a caller that calls itself | all bodies put into it may bring 96 bytes of frame objects, no more |

Size is counted in IR instructions after `sroa` and `simplify`. The calls
a function keeps are looked at a second time, because a callee may have
one call left once the others are replaced.

The numbers were chosen from these measurements (instructions, callgrind;
"suite" is the geometric mean of the 28 programs against the order without
`inline`; "compiler" is the compiler built by `azol-opt` compiling
`bench/generated/big.azl`, 132,004 lines):

| plain limit / in a loop | suite | compiler: code | compiler: on `big.azl` |
|---|---|---|---|
| no inlining | 1.000 | 578,526 | 666.9 M |
| 14 / +12 | 0.935 | 604,382 | 628.3 M |
| 14 / +40 | 0.928 | 623,934 | 620.8 M |
| **20 / +30** | 0.927 | 651,038 | 612.8 M |
| 30 / +12 | 0.926 | 665,294 | 607.7 M |
| 50 / +12 | 0.922 | 805,854 | 590.8 M |
| 80 / +12 | 0.920 | 888,862 | 586.2 M |

(these with a caller limit of 2,500). Past 20/+30 the compiler grows much
faster than it gains. The single-call limit: 60 gives 657,646 bytes and
616.3 M, 400 gives 651,038 and 612.8 M, 2,000 the same as 400. The caller
limit, with 20/+30, and what `azol-opt` itself needs to build the compiler:

| caller limit | suite | compiler: code | on `big.azl` | building the compiler |
|---|---|---|---|---|
| 400 | 0.929 | 614,590 | 632.1 M | 3,085 M |
| **800** | 0.927 | 635,102 | 616.7 M | 3,355 M |
| 1,200 | 0.927 | 643,422 | 613.4 M | 3,465 M |
| 2,500 | 0.927 | 651,006 | 612.9 M | 3,615 M |

The passes are not linear in the size of a function (`sroa`'s flow of
loads behind parameters is blocks times cells), so large callers cost
compile time out of proportion; 800 keeps nearly all the gain. With the
frame cap for recursive callers counted in all (the last change) the final
numbers are 621,470 bytes and 618.8 M.

**Why `sroa` and `simplify` are not in the order a second time.** They run
inside `inline` on exactly the functions it changed. Running `simplify,
sroa, simplify, dce` on the whole module after `inline` as well gave the
same instruction counts for every program of the suite and for the
compiler, to the instruction, and cost another pass over everything.

### tailcall

The pattern is a block that ends `r... = call F(a...)` then `ret r...` in
function F itself, with nothing between. A value dropped after the call
keeps the call a call: the drop is not moved. An argument for a `ptr`
parameter must be that parameter itself, so no address of this round's
frame objects lives into the next round (which uses the same frame) and
every access through a parameter keeps its region. The old entry block
becomes the top of a loop; only parameters some site changes become block
parameters; the `frame` instructions move to the new entry block.

**Running out of stack.** A recursion without end must still stop with
`azol: stack overflow`, so the loop is counted: 255 rounds are jumps, the
256th is the call itself, kept in a block of its own. A recursion then uses
1/256 of the stack it used, and one without end still reaches the guard
page (67 million rounds for a small frame, a fraction of a second). The
count is left out when the recursion is seen to end: one `i64` parameter
moves by a constant in one direction at every site, and every site is
dominated by a branch that keeps that parameter on the far side of a bound:
a constant (bound and step within 2^62, so nothing wraps), or a parameter
no site changes when the comparison is strict and the step is 1 (`i < n`
with `i + 1`; `i <= n` could wrap at the largest number, and is counted).
`==` and `!=` prove nothing. This is the one place where `-O` changes what
a program does: a recursion that ends but is deeper than the stack now
runs. The slow path is recognised by its shape (`tc_is_slow`), so a second
run of the pass leaves it alone.

### accrec

The pattern is a block that ends `r = call F(a...)`, `v = OP x, r` or
`OP r, x`, `ret v`, with OP one of `add`, `mul`, `and`, `or`, `xor`, `r`
used once, `v` used once, and `x` not `r`; F gives one `i64` (`and`, `or`,
`xor` also one `bool`). In `F(a) + F(b)` that is the second call. The new
body computes G(a, acc) = acc OP F(a) and F(a) is G(a, E) with E the
neutral element (0, 1, all ones or `true`, 0, 0): every other `ret v`
becomes `ret (acc OP v)`, and the site goes round with `acc OP x`. The
proof obligation is that OP is associative and commutative on wrapping
64-bit numbers, which holds for these five and for nothing else the IR has;
`x` was computed before the call and only OP stood between the call and
the `ret`, so no effect moves. Calls that are simply returned in the same
function go round too. The loop, the count and the proof of an end are
`tailcall`'s.

### Proof (x86-64 Linux, 12 November 2026)

`python3 tests/opt/run_inline_tests.py`: 44 hand-written cases, 14 of them
where a pass must do nothing (mutual recursion, a callee that calls itself,
a callee over the limit, an argument whose region nothing names, `sub`,
`sdiv` and a shift, something printed or freed after the call, a result
used twice, the address of a frame object of the same call, two address
parameters that change places), each checked before the pass, after it,
after the whole order to a fixed point, and as native code (146 native
runs); 1,500 random functions that call themselves, up to 3,000 deep (895
became loops, 543 of them counted), the same before and after, 1,081 runs
also as native code; 1,502 random functions in 281 modules the same after
`inline` alone, `tailcall,accrec` alone and both; 7 programs with
recursions that end three million calls deep and recursions without end
(which must print `azol: stack overflow`), 21 runs interpreted, native and
with three registers; for ARM64, 14 optimised programs as 56 executables
in the emulators like their default builds. Through the whole order in
`run_pass_tests.py`: the programs of tests/programs and the examples
interpreted, native and with three registers like their default builds
(the three `stack_overflow` programs among them), and the compiler built by
`azol-opt` builds `hello` and itself into the bytes `azol` writes.
`python3 tests/opt/run_opt_tests.py` passes as a whole. Fuzzer with
`--lower --opt`: 5,200 programs (seeds 40000 to 45399) lowered, through
the whole order, interpreted and as native code with all registers and
with three, 0 findings, on the final sources. (An earlier run on the same
seeds reported program 40705, which recurses until the stack ends: the
interpreter was killed for lack of memory, with the tool from before these
passes as well; the machine was short of memory then, and the run on the
final sources has no finding.)

### Measurements

`python3 bench/gate.py --instructions --azol-opt <tool>`, small sizes:

| Program | default | `azol-opt` before | `azol-opt` now | Rust `-O` | now / Rust |
|---|---|---|---|---|---|
| `fib40` | 36.35 M | 33.66 M | 23.40 M | 26.85 M | 0.87 |
| `sha256` | 416.2 M | 248.5 M | 173.1 M | 95.2 M | 1.82 |
| `bits` | 223.0 M | 139.0 M | 65.0 M | 47.4 M | 1.37 |
| `tokenizer` | 41.70 M | 36.26 M | 36.16 M | 10.02 M | 3.61 |
| `dijkstra` | 133.6 M | 98.8 M | 78.8 M | 31.9 M | 2.47 |
| `sort` | 64.10 M | 55.35 M | 54.38 M | 50.94 M | 1.07 |
| `binary_trees` | 98.13 M | 81.32 M | 81.32 M | 171.95 M | 0.47 |

Geometric means over the 28 programs: `azol-opt` against the default build
0.85 before and **0.79** now; against `rustc -O` 1.57 before and **1.45**
now. Also changed by more than 2%: `map_int` 74.1 M to 63.3 M, `matmul`
59.6 M to 53.4 M, `bfs`, `crc32`, `lru`, `wordfreq`, `words`. `fib40` is
`accrec` (the second call of each level is a loop). `tokenizer` has every
function in `main` now and is no faster: its cost is not in calls.
`binary_trees` and `sort` have one recursive function each that stays.

The compiler: 760 functions, 4,332 calls, 1,583 replaced, 270 functions
gone. Built by `azol-opt` it compiles `big.azl` in 618.8 M instructions,
666.9 M before (-7.2%); the compiler `azol` builds by default takes
852.3 M. Its code grows from 578,526 to 621,470 bytes (+7.4%); the code of
the 28 programs of the suite shrinks from 59,972 to 57,972 bytes.

The cost: `azol-opt` building the compiler (19,731 lines) takes 3,301 M
instructions, 2,392 M before, so 38% more. Of the difference, the `inline`
pass is about two thirds (its `sroa` 240 M and `simplify` 225 M on the
functions it changed, its own work 140 M), the rest is the back end and the
later passes on code that is a tenth larger. `tailcall` and `accrec`
together are 3 M.

### What later passes should know

- After `inline` a function's entry block may hold `frame` instructions
  for objects that are used only in one branch or one loop, and an object
  that came from a callee in a loop is the same memory in every round.
- A module after `inline` has no function `main` cannot reach, and the
  list of names a function calls holds only names it still calls.
- A loop made by `tailcall` or `accrec` has a new entry block that only
  jumps; a counted one carries a count down from 256 as a block parameter.
  `licm` and `range` meet it like any other loop.

### Doubts

- Compile time. The order now costs 38% more instructions on the compiler.
  Every function that takes a callee in is run through `sroa` and
  `simplify` again; I found no cheaper way to keep both the sizes right and
  the result as good, only smaller limits.
- The bound on stack growth is on frame objects (96 bytes for a caller
  that calls itself). Values kept in registers across calls spill to the
  frame too, and that part is not bounded: a recursive function with
  bodies put into it can have a larger frame than before, so a recursion
  very close to the end of the stack could now pass it.
- The count of 256 is a choice, not a measurement: the two instructions it
  costs a round did not show in the suite, and any number above about 16
  makes the calls that remain invisible in the profile.
- `tc_is_slow` knows the slow path by its shape. If a later pass rewrites
  that branch and the order runs again (`--fixpoint`), the path would be
  wrapped a second time: still correct, larger. It did not happen in any
  test, which run with `--fixpoint`.
- `tokenizer` (3.61 of Rust), `sort_lib`, `soa` and `particles` did not
  move: what they need is `gvn`, `licm` and `bce`.
- Wall times were not usable on the machine these numbers come from (two
  other builds were running), so the cost is given in instructions only.
