# The differential fuzzer

`fuzz.py` writes random azoL programs and checks that the compiler
gives every one of them the meaning the language promises. It needs only
Python 3 and a built compiler.

## How it works

1. **A generator** builds a random program as a tree, from a seed. The
   program is valid (it follows the checker's rules), always ends, and
   always behaves the same way. It prints many intermediate values, and at
   the end of `main` it prints everything every variable holds.
2. **An oracle**, a small interpreter inside `fuzz.py`, runs the same tree
   and works out the exact standard output, standard error and exit code.
   It shares no code with the compiler: 64-bit wrapping arithmetic,
   division that truncates toward zero, a remainder with the sign of the
   dividend, shift counts masked to six bits, arithmetic right shift,
   strings as bytes, and a deep copy on every store.
3. **The builds.** The program is built for `x86_64-linux` and run
   natively. When the checking emulator is available (`emu.py`, looked for
   in `/home/claude/ref` or in the folder named by `AZOL_EMU_DIR`), it is
   also built for `aarch64-macos` and run there; the emulator reports
   leaks, double frees, accesses outside a block and an unbalanced stack.
   Every eighth program (`--linux-arm-every N`) is built for
   `aarch64-linux` as well and run under that target's emulator,
   `emu_linux_arm64.py`, when it is in the same folder.
4. **A finding** is any of: a believed-valid program the compiler rejects,
   a compiler crash or hang, a program that crashes or hangs, output or an
   exit code that differs from the oracle, or a heap rule broken under the
   emulator.
5. **A reducer** shrinks the failing program while the same kind of
   failure remains: it drops functions, constants and types, deletes runs
   of statements, replaces a compound statement by its body, and replaces
   expressions by smaller ones. The result is saved under `findings/`.

## Running it

    sh build.sh
    python3 tests/fuzz/fuzz.py --count 3000 --minutes 25

    python3 tests/fuzz/fuzz.py --show 1234      print the program of seed 1234
    python3 tests/fuzz/fuzz.py --only 1234      run that one program and report
    python3 tests/fuzz/fuzz.py --seed 50000 --count 500 --no-arm

Program `i` of a run uses seed `--seed + i`, so a finding is reproduced
with `--only <seed>`. The exit code is 1 when there were findings. Other
options: `--azol PATH` (the compiler to test), `--no-reduce`,
`--findings DIR`, `--max-findings N`, `--linux-arm-every N`, `--quiet`.

One process, no threads; a few hundred programs a minute on one core.

Each finding gets a folder `findings/<kind>_<seed>/` with

- `original.azl` and `reduced.azl`,
- `original.results.txt` and `reduced.results.txt`: the seed and the
  results (oracle, x86-64, arm64 macOS, arm64 Linux) and any compiler
  message,
- `info.txt`: the seed, the kind of failure and how to reproduce it.

## What the programs contain

- `i64` arithmetic and bit operators on edge values (0, 1, -1, the
  smallest and largest numbers, powers of two), division and remainder by
  many constants and by values, shifts by 0, 63, 64, 65 and negative
  counts, comparisons, unary `-` and `!`, numbers written in decimal, hex,
  binary, with `_`, and as characters;
- `&&` and `||` around calls that print, so the order of evaluation shows;
- `let` and `var`, reassignment, shadowing in inner blocks, many live
  variables at once, and now and then a function with hundreds of locals;
- functions with 0 to 8 parameters, `mut` parameters given variables,
  fields and elements, recursion with a depth counter, early returns,
  functions without a result;
- structs (nested, with strings and lists), enums with values, `Option`,
  `Result`, a recursive enum, the generic `Box[T]`, `Pair[A, B]` and
  `Either[A, B]`, and generic functions used at many types;
- `match` on enums (in place, on fields, on temporaries), numbers, `bool`
  and strings, with variant, literal, constant, `_` and binding arms, as
  one-line arms and as blocks;
- `for` over variables, fields and temporaries, with and without an
  index; `while` with `break` and `continue`;
- lists of numbers, strings, structs, enums and lists: literals, `push`,
  `pop`, indexing, element assignment, `len`, `contains`, `first`, `last`,
  `reversed`, `join`, and loops that grow a list or a string far past its
  first buffer;
- strings: `+`, `s = s + x` and `s = s + s`, `==`, bytes, `slice`, `find`,
  `contains`, `starts_with`, `ends_with`, `trim`, `split`, `lines`,
  `to_i64`, `str()`, `chr()`, the empty string, escapes;
- constants built from constant expressions, used in code and as `match`
  patterns;
- lines that continue after an operator;
- deliberate run-time stops (division by zero, an index or a slice out of
  range, `pop` from an empty list, a bad `chr`, `insert` and `remove_at`
  outside the list), `exit` and `eprint`; the oracle predicts the message,
  the output before it and the exit code;
- `value?` on `Option` and `Result`, in functions that return one: as a
  statement, in a `let`, inside larger expressions, in conditions, call
  arguments, loop sources and `match` subjects, between two reads of the
  same list element, taken out of variables (a copy), calls and
  temporaries (a move), and in the first parts of struct literals and
  variants; never where the checker refuses it (inside a list literal, or
  after a part of a literal that owns memory);
- the library methods that change their receiver, `extend`, `truncate`,
  `insert` and `remove_at`, on variables and on fields, with the receiver
  read inside its own call for a number (`xs.insert(clampi(e, xs.len() +
  1), item)`);
- `Map[V]` with values that are numbers, strings, lists, lists of lists,
  `Option`, structs and enums: `new_map`, `set`, `get`, `has`, `remove`,
  `keys`, `len`, copies of a map, and loops that set and remove over a
  hundred keys so that the table is rebuilt;
- the rest of the library: `replace`, `repeat`, `to_upper`, `to_lower`,
  `pad_left`, `pad_right`, `compare`, `index_of_from`, `last_index_of`,
  `bytes` and `to_str`, `parse_hex`, `to_hex`, `hash`, `is_empty`,
  `sorted`, `sum`, `min`, `max`, `take`, `skip`, `index_of`, `count`, and
  `abs`, `min`, `max`, `clamp`, `pow`, `compare` of numbers, also called on
  literals;
- the shapes the code generators treat specially: straight-line runs of
  reads and writes of `xs[i]`, `xs[i].field` and `xs[i][j]` with one index,
  between calls, `push`, `pop`, `insert`, `truncate`, a call that takes the
  list as `mut`, new values for the index and for the whole list, inside
  `if` branches and loops; functions that start with `if a < b { return c
  }` on their parameters; loops that read one parameter many times;
- string constants joined with `+`, the smallest `i64` written in decimal,
  functions whose last path ends in `exit`, and now and then a recursion
  that never ends, which must stop with `azol: stack overflow`.

One program in five is mostly numbers in deep expressions.

## What it does not cover

- Programs that must be rejected: every program here is meant to compile,
  so error messages and the checker's refusals are not tested.
- `args`, `read_file`, `write_file`, `set_executable`, programs in several
  files, and `main` without a result.
- Generic structs and functions other than the fixed helpers at the top
  of each program; closures of types three levels deep at most.
- Patterns of ownership the generator avoids to stay valid: it never
  changes a variable that a `for` or `match` is looking at, never passes
  overlapping places to one call, and only calls functions with `mut`
  arguments as whole statements. The checker may allow more than this.
- Heap rules on x86-64: leaks and double frees are only seen under the
  arm64 emulator. Without the emulator only x86-64 output is compared.
- Programs that need much work: the oracle gives up after a fixed number
  of steps and such programs are skipped, so long-running loops, deep
  recursion and very large values are not tested.
- A stack overflow while the frames hold heap values runs natively only:
  a hundred thousand live blocks are too many for the emulators' checking
  heap.
- `xs.extend(...)`, `m.set(...)` and the like never read their own
  receiver for anything but a number, because the checker refuses that.
- Bytes outside ASCII are compared loosely on arm64, because the emulator
  decodes what a program writes piece by piece.

## Limits of the oracle

The oracle fixes an order of evaluation: operands and arguments from left
to right, the right side of an assignment before the index of its target,
and the argument of `push` before its receiver. This matches what the
compiler does today and was learnt from it, so a deliberate change to that
order would show up as findings. The older text methods of the oracle
(`find`, `split`, `lines`, `trim`, `to_i64`) were written from the
behaviour of the library source in `lib/prelude.azl`; an error both share
would not be found.

The newer library methods, `?`, `Map` and the methods that change their
receiver are modelled from their documented meaning in
`docs/language.md`, with Python's own operations (`sorted`, `bytes.replace`,
`rfind`, `pow` with a modulus), not from the library's loops. The source
was read to settle what the manual leaves open, and the oracle follows it
there:

- `keys()` after a `remove`: the last key moves into the freed place (the
  manual only promises the order up to the first removal);
- `pad_left` and `pad_right` with a fill of several bytes add whole copies
  and may pass the width; an empty fill changes nothing;
- `index_of_from` treats a start below zero as zero and gives `None` for a
  start past the end, also for an empty part; `find` and `last_index_of`
  of an empty part give 0 and the length;
- `min` and `max` give the first of several equal items;
- `to_str` stops with the message of `chr`, and `insert` and `remove_at`
  with `azol: index out of range`;
- `parse_hex` accepts both cases, and sixteen digits fill all 64 bits.

A `?` leaves its function by raising an exception inside the oracle, which
the call catches: the same deep copy and the same scopes as a `return`.
