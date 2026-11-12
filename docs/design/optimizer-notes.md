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
