# Shorter azoL

The user's instruction: find a way to shorten long code that does not create
problems, in a language written mostly by AI agents, that depends on nothing
outside itself. This document measures where azoL's lines go, decides which
features remove them, specifies those features, shows the gain on real
programs, and says how each one could go wrong and how that would be caught.

Sections 3.1 to 3.8 and the library functions of 3.9 that need neither
function values nor `Map[K, V]` are implemented; section 7 says where the
build differs from this text. The rewritten programs in section 4 were
written by hand before that and use library functions that still wait for
closures, so they have not been compiled as they stand.

## Summary

- azoL is not longer than Rust in tokens. It is longer in lines: 19% more
  code lines on the small tasks and 14% more on the hard ones, with 1 to 2%
  more tokens. The excess is vertical: things Rust says in one line take
  three to five in azoL.
- Five patterns account for almost all of it: a default followed by a
  `match` to read an `Option`; counting loops written with `while`; guards
  of three lines; `x = x + n`; text built with `+` and `str()`.
- Seven small features and about thirty library functions remove them. All
  but one are rewritten by the parser, the checker or the formatter into
  what exists today, so the code generators and the coming optimiser are not
  touched; ranges in `for` need about 30 lines in each generator.
- Rewritten by hand, ten programs of the corpus go from 669 code lines to
  499; the same ten in Rust are 541. Tokens go from 4,213 to 3,556; Rust has
  4,108. Three stretches of the compiler go from 225 lines to 158.
- Six of the ten programs end at or below Rust's line count; four stay above
  it by 2 to 17 lines. The total is 8% below Rust.

## OPEN: decisions for the user

Each changes how azoL code looks. Each has a recommendation, and the rest of
the document assumes it.

- **OPEN C1. Text with values in it: `"total: {sum}"`.** A `{` inside a
  string starts a value; a literal brace is written `\{`. This breaks the few
  existing strings that contain `{` (18 in this repository, none of which
  would change meaning silently: each becomes an error with the fix). It
  replaces OPEN A1 of [agents.md](agents.md), which made `"{x}"` an error.
  Recommendation: yes.
- **OPEN C2. `if` and `match` may give a value**, only as the whole right
  side of `let`, `var`, an assignment or `return`, and only when every
  branch is one expression: `let goal = if n > 0 { "up" } else { "down" }`.
  Recommendation: yes.
- **OPEN C3. A short `if` stays on one line.** `azol fmt` lays out an `if`
  without `else`, whose body is one simple statement and which fits in 100
  columns, as `if at < 0 { return }`. Today it always uses three lines.
  This alone removes 6.7% of the compiler's lines. Recommendation: yes.
- **OPEN C4. Ranges in `for`:** `for i in 0..n`, `for i in 1..=12`,
  `for j in i * i..limit step i`, counting down with `step -1`. A range is
  not a value and exists nowhere else. Recommendation: yes.
- **OPEN C5. `+=` and the other compound assignments.** Recommendation: yes.
- **OPEN C6. What stays out:** `if let`, `let ... else`, a last expression
  that returns without `return`, tuples, default field values, and `unwrap`.
  Each is an error that shows the azoL form. `match` stays the only way to
  look inside a value. Recommendation: keep them out.

## 1. Where the lines go

Measured by `/tmp/az/concise/measure.py` on code lines (blank lines and
comment lines removed). Two corpora: the 34 programs written by models that
had read only the documentation, next to the same tasks in Rust, and the
compiler written in azoL (`compiler/*.azl` without generated files, 16,709
code lines).

### 1.1 Against Rust, task by task

Averages over the models that wrote each task.

| Task | azoL lines | Rust lines | ratio | azoL tokens | Rust tokens | ratio |
|---|---|---|---|---|---|---|
| `bank` | 56.5 | 60.0 | 0.94 | 482 | 511 | 0.94 |
| `inventory` | 30.0 | 26.0 | 1.15 | 201 | 188 | 1.07 |
| `matrix` | 40.5 | 30.0 | 1.35 | 261 | 270 | 0.97 |
| `parse_kv` | 34.0 | 23.0 | 1.48 | 204 | 192 | 1.06 |
| `primes` | 30.0 | 23.0 | 1.30 | 149 | 154 | 0.96 |
| `shapes` | 33.0 | 34.0 | 0.97 | 236 | 215 | 1.10 |
| `stack_generic` | 58.0 | 52.0 | 1.12 | 390 | 372 | 1.05 |
| `wordfreq` | 28.0 | 13.0 | 2.15 | 169 | 145 | 1.17 |
| `editor` | 114.7 | 114.0 | 1.01 | 751 | 821 | 0.92 |
| `graph` | 78.3 | 93.7 | 0.84 | 549 | 680 | 0.81 |
| `ledger` | 97.7 | 84.0 | 1.16 | 661 | 548 | 1.21 |
| `lru` | 76.3 | 80.0 | 0.95 | 471 | 572 | 0.82 |
| `sim` | 125.3 | 83.3 | 1.50 | 837 | 753 | 1.11 |
| `tree_edit` | 131.0 | 91.3 | 1.43 | 810 | 664 | 1.22 |

| Set | azoL lines | Rust lines | ratio | azoL tokens | Rust tokens | ratio |
|---|---|---|---|---|---|---|
| 8 small tasks, per model | 310 | 261 | 1.19 | 2,092 | 2,047 | 1.02 |
| 6 hard tasks, per model | 623 | 546 | 1.14 | 4,080 | 4,038 | 1.01 |

The 7 to 11% reported earlier counted every line of the files; on code lines
the gap is 14 to 19%. In tokens there is almost no gap. So the cure is not
fewer words but fewer lines per thought, and anything that only shortens
words (shorter keywords, sigils) would miss.

Where azoL already wins (`graph`, `lru`, `bank`) the reason is the same in
each: no `&`, `&mut`, `.to_string()`, `.iter()`, `.collect()`, `use` lines.
Where it loses badly (`wordfreq`, `sim`, `tree_edit`, `parse_kv`) Rust has
`entry().or_insert`, `sort_by` with a closure, `retain`, `if let`, `match`
as a value and `?` on `Option`.

### 1.2 The patterns

Lines that belong to each pattern, and their share of all code lines.

| Pattern | Small (620) | Hard (1,870) | Compiler (16,709) |
|---|---|---|---|
| A line holding only `}` | 148, 23.9% | 410, 21.9% | 3,266, 19.5% |
| Guard of three lines: `if c { return ... }` and the like that fit one line | 27, 4.4% | 90, 4.8% | 1,671, 10.0% |
| of which `if c.failed { return }` / `if p.failed { return NO_NODE }` | 0 | 0 | 636, 3.8% |
| Default, then `match` on an `Option` to overwrite it | 0 | 65, 3.5% | 14, 0.1% |
| `var x = a`, then `if c { x = b }` | 0 | 8, 0.4% | 240, 1.4% |
| Counting loop: the `var i` line and the `i = i + 1` line | 22, 3.5% | 50, 2.7% | 340, 2.0% |
| `x = x + n` and other self-updates | 23, 3.7% | 32, 1.7% | 694, 4.2% |
| Lines that build text with `+` and `str()` | 49, 7.9% | 64, 3.4% | 455, 2.7% |
| Loops that are a map, filter, sum or find | 21, 3.4% | 28, 1.5% | 79, 0.5% |
| `else {` holding only an `if` | 0 | 10 | 11 |
| `== false`, `== true` | 0 | 16 | 0 |
| Arm that does nothing: `None => {}` | 2 | 24, 1.3% | 53 |
| `let t = a.b` copies before a change | 6 | 72, 3.9% | 330, 2.0% |

Notes from reading the files by hand, which the script cannot see:

- The models avoided `else if` and `!` because the briefing did not show
  them (both exist). `AGENTS.md` now shows them; this needs no feature.
- Most `let t = a.b` copies in the hard tasks were defensive: the models
  were unsure of the read-and-change rule. `AGENTS.md` now states the exact
  boundary. About a third were needed.
- In the compiler the guard is the single largest pattern, and it comes from
  one style choice: every stage reports a failure by setting a flag, so every
  call is followed by a test. That is a property of that program, but short
  guards of other kinds are 4 to 5% of the model-written programs too.
- Accessor functions and struct construction are not a measurable cost: 45
  `field: field` repeats in the compiler, 9 in the corpus.

### 1.3 A floor: mechanical rewriting

`/tmp/az/concise/transform.py` rewrites only what a script can prove safe:
compound assignment, counting loops whose variable is not used afterwards,
`.or(default)` for the default-then-match shape, one-line guards, `else if`,
interpolation of plain `+` chains, field shorthand. It uses no closures and
no new library.

| Set | Lines | Tokens | Against Rust, lines | tokens |
|---|---|---|---|---|
| Small, per model | 310 → 295, −4.8% | 2,092 → 1,899, −9.2% | 1.13 | 0.93 |
| Hard, per model | 623 → 573, −8.1% | 4,080 → 3,807, −6.7% | 1.05 | 0.94 |
| Compiler | 16,709 → 15,295, −8.5% | 119,569 → 116,292, −2.7% | | |

In the compiler: 557 guards on one line, 534 compound assignments, 150
ranges, 127 interpolations. So syntax alone halves the gap to Rust and puts
tokens below Rust; the rest needs values from `if`/`match` and the library,
which section 4 measures by hand.

## 2. Decisions

Judged by the criteria of [agents.md](agents.md): one way to write each
thing; a line means what it says without reading other files; no hidden
control flow or conversion; habits from other languages work when there is
no reason to differ, and are a precise error with a fix when there is.

| Candidate | Decision | Reason |
|---|---|---|
| `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` `<<=` `>>=` | accept | Every model writes them; one meaning: `a op= b` is `a = a op b` with `a` located once. |
| `++`, `--` | reject, error with fix `x += 1` | Two ways to add one; statement or expression is unclear. |
| `for i in a..b`, `a..=b`, `step k` | accept, in `for` only | The counting loop is the most common loop; both forms come from habit and differ only in the end. |
| Ranges as values, `(0..n).rev()`, slices `xs[a..b]` | reject, error with fix | A second kind of sequence beside lists; `step -1` and `slice`/`take`/`skip` cover it. |
| `if`/`match` as a value | accept, restricted | Removes the default-then-match shape without a new construct; the restriction keeps it one line of thought. |
| Blocks with statements that end in a value | reject | That is implicit return in small; the last line of a block would mean two things. |
| Last expression returns without `return` | reject | The models named "`return` is always written" as a help; two ways to return. |
| `o.or(default)`, `m.get_or(key, default)` | accept (library) | The commonest read of an optional value in one call. |
| `o.expect("message")` | accept (library) | Stops with `azol: message`; the honest form of "this cannot be absent". |
| `o.unwrap()` | reject, error with fix `.expect("...")` or `.or(...)` | A stop with no message helps nobody; the fix forces the choice. |
| `o.or_else(fn)` | reject | A value `match` says the same in the same space without a closure. |
| `let Some(x) = e else { ... }` | reject, error with fix | A value `match` with a branch that returns does it: `let x = match e { Some(x) => x  None => return }`. |
| `if let Some(x) = e { ... }` | reject, error with fix | `match` with `None => {}` is one line longer and is the one way. |
| Interpolation `"{expr}"` | accept (OPEN C1) | The largest single token cost; every model tries it; today it silently prints the braces. |
| Format mini-language `{x:>5}`, `{:08x}` | reject, error with fix | Methods already do it: `{n.pad_left(5)}`, `{n.to_hex()}`. |
| Nested and literal patterns | accept | `Some(Put(key, value))`, `Ok(0)`; literals already work at the top level. |
| Tuples, several results | reject, error with fix | A small struct names its parts; one way to group values. |
| Field shorthand `Pair { key, value }` | accept | Rust and JavaScript habit, no ambiguity, removes `key: key`. |
| Default field values, `..base` update | reject | Hidden values at the place of use; `var b = a` then `b.x = 1` is already short under value semantics. |
| Method chains on temporaries | already works | Document it; `counts.keys().sorted()` is in the corpus. |
| Type of `[]` from later use | reject | Not local; `var xs: List[i64] = []` stays. |
| One-line `if` with one simple statement | accept (OPEN C3), as a layout rule | The parser accepts it today; only `azol fmt` expands it. |
| A function body as one expression, `fn f() -> T = e` | reject | A second way to write a function. |
| `x = if ...` inside a larger expression, `f(match ...)` | reject, error with fix | Keeps control flow at the start of a line, where it can be seen. |

## 3. Specifications

New error codes take E1401 to E1425, after packages. Every feature states
where it is lowered. "Parser" and "checker" mean the generators and the
optimiser see only forms that exist today.

### 3.1 Compound assignment

```text
assign := place ("=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=") expr
```

- `a op= b` means `a = a op b`, with every index and call inside `a`
  evaluated once, before `b`. `xs[next()] += 1` calls `next` once.
- Types as for `a op b`. `s += t` on text is the in-place append that
  `s = s + t` already is.
- The place follows today's rules for an assignment target (`var`, `mut`
  parameter, not a loop variable). The read-and-change rule applies to `b`:
  `xs[0] += xs.len()` is fine (a number is copied first), `s += f(mut s)` is
  rejected as today.
- Errors: E1401 for `x++`, `++x`, `x--`: "azoL has no `++`; write `x += 1`",
  with the fix.
- Layout: one space on each side.
- Lowering: parser, when the place has no index or only names, numbers and
  fields in its indexes; otherwise the checker keeps the index in a hidden
  slot. Generators untouched.

### 3.2 Ranges in `for`

```text
for := "for" name "in" expr (".." | "..=") expr ("step" expr)? block
```

- `for i in a..b` runs `i = a, a + 1, ...` while `i < b`; `a..=b` while
  `i <= b`. `a` and `b` are `i64`, evaluated once, `a` first.
- `step k`: `k` is an `i64` constant expression other than zero. With a
  negative `k` the loop runs while `i > b` (`..`) or `i >= b` (`..=`):
  `for i in n - 1..=0 step -1` visits `n - 1` down to `0`.
- `i` is read-only in the body, like every loop variable. `break` and
  `continue` work; `continue` goes to the step.
- The sum `i + k` wraps like all arithmetic; a loop whose end is the largest
  `i64` with `..=` stops after that value instead of wrapping (the test is
  made before the step).
- `step` is a word only in this position; it stays a free name elsewhere.
- Errors: E1402 a range outside `for` ("a range exists only in `for`; for
  part of a list use `xs.slice(a, b)`"); E1403 a bound that is not `i64`;
  E1404 `step` that is zero or not a constant; E1405 `(a..b).rev()` and
  `.step_by(k)`, with the `step` form as the fix; `for (i = 0; ...)`,
  `for i in range(n)` and `for i := 0` keep their rows in the habit table
  of agents.md and now get this form as their fix.
- Layout: no spaces around `..`, as written above.
- Lowering: not a pure rewrite. `continue` must reach the step, which a
  plain `while` cannot express. The loop node gains an optional step
  statement; each generator needs about 30 lines, the optimiser's lowering
  the same. The index loop over a list (`for i, x in xs`) is unchanged.

### 3.3 `if` and `match` as values

```text
value_if    := "if" cond "{" expr "}" ("else" "if" cond "{" expr "}")* "else" "{" expr "}"
value_match := "match" expr "{" (pattern "=>" (expr | jump))+ "}"
jump        := "return" expr? | "break" | "continue"
```

- Allowed in exactly four places: the whole value of `let`, of `var`, the
  whole right side of an assignment (plain or compound), and the whole
  operand of `return`.
- Every branch is one expression, and all have the same type; that is the
  type of the whole. In a value `match` a branch may instead be a jump; in a
  value `if` a branch may be a jump only as `{ return ... }`, `{ break }` or
  `{ continue }`. A call to `exit` counts as a jump.
- A value `if` must have `else`. A value `match` must be exhaustive, as
  every `match` is.
- A value `if` is written on one line; if it does not fit in 100 columns,
  `azol fmt` breaks it as the statement form with each expression on its own
  line. A value `match` has one arm per line.
- Ownership: the chosen branch's value is moved when it is a temporary and
  copied when it is a variable that is used again, exactly as the right side
  of `let` is today. Bindings of a pattern are views, as today; a branch that
  gives a binding copies it.
- The statement forms are unchanged and stay the only forms with blocks.
- Errors: E1406 value `if` without `else`; E1407 a branch with statements
  ("a branch that gives a value is one expression; use the statement form
  and assign inside it"), with no fix; E1408 branches of different types,
  naming both; E1409 a value `if`/`match` inside a larger expression
  ("give it a name with `let` first"), with the fix when the expression is
  an argument; the ternary `c ? a : b` keeps its habit-table row and gets
  this form as its fix.
- Lowering: checker. `let x = if c { a } else { b }` becomes a slot for `x`
  and the statement `if` assigning in each branch; a jump branch is the jump.
  The slot starts unfilled, which the checker already tracks for results of
  calls. Generators untouched; to be confirmed by the implementer for values
  that own memory.

### 3.4 Text with values: `"{expr}"`

```text
string := '"' (char | escape | "{" expr "}")* '"'      escape adds "\{"
```

- `"{e}"` puts the text of `e` there, as `str(e)` would. `e` may be of any
  type `str()` accepts: `i64`, `bool`, `str` today; `f64` and, by
  [traits.md](traits.md), everything else later.
- `e` is any expression that contains no string literal and no `{`. It may
  contain calls with arguments: `"{name.pad_left(8)} {n.to_hex()}"`.
- A literal brace is `\{`. `}` outside a value needs no escape.
- Evaluation is left to right, each `e` once.
- No format language: width, padding, hex, sign are methods.
- Errors: E1410 a name inside `{}` that is not in scope, or a value that has
  no text form; E1411 `{` with no matching `}` or an empty `{}` ("write `\{`
  for a brace"), with the fix; E1412 a string literal or a brace inside
  `{}` ("give the value a name with `let` first"); E1413 `${x}`, `%d`, `%s`
  with arguments, `{:>5}`, `{0}`, `f"..."`, with the azoL form as the fix.
- Constants: a `const` string may not contain values (constants are folded
  before any variable exists): E1410 says so.
- Layout: untouched; `azol fmt` never rewrites the inside of a string.
- Migration: an existing literal with `{` becomes E1410 or E1411, never a
  silent change, because no name could be in scope by accident and print
  differently without the author having written it in braces: the 18
  literals with `{` in this repository were checked, and none holds a name
  in braces. `azol fix` writes `\{`.
- Lowering: lexer and parser, to the `+` and `str()` chain written today,
  which the generators already append in place.

### 3.5 Nested and literal patterns

```text
pattern := "_" | name | literal | Variant ("(" pattern ("," pattern)* ")")?
```

- A pattern may hold patterns: `Some(Put(key, value))`, `Ok(0)`,
  `Pair(_, Some(n))`. Literals are numbers, `true`/`false`, strings and
  character literals, as at the top level today.
- A name binds; a name that is a variant matches that variant (variants are
  global names, so there is no ambiguity).
- Exhaustiveness is checked over the whole nest; the error names a missing
  case in full (`Some(Get(_))`) and the fix adds the arm.
- Bindings are views into the matched value, as today.
- Errors: E1418 the same name bound twice in one pattern. Struct patterns
  (`Point { x, y }`) stay out: fields are read with `.`.
- Lowering: checker, to nested `match` statements with shared arm bodies.
  The exhaustiveness algorithm is new.

### 3.6 Field shorthand

- `Pair { key, value }` means `Pair { key: key, value: value }`; a field may
  be given either way in the same literal.
- Errors: E1419 no variable with the field's name ("write `key: <value>`").
- `azol fmt` leaves the author's choice; `azol fix` does not rewrite.
- Lowering: parser.

### 3.7 One-line `if`

Not a language change: `if c { return }` parses today. The rule for
`azol fmt`: an `if` with no `else`, whose body is exactly one statement that
is a `return`, `break`, `continue`, an assignment or a call, is laid out on
one line when the line fits in 100 columns; otherwise in three lines, as
today. Every other block keeps one statement per line. This is the one
layout, not a second one: `azol fmt` joins as well as splits.

### 3.8 What stays out, with its error

| Written | Code | Message and fix |
|---|---|---|
| `if let Some(x) = e { ... }` | E1414 | "azoL has no `if let`; use `match`", fix: the `match` with `None => {}` |
| `while let Some(x) = xs.pop() { ... }` | E1414 | fix: `while xs.len() > 0 { let x = xs.pop() ... }` |
| `let Some(x) = e else { return }` | E1415 | fix: `let x = match e { Some(x) => x` / `None => return }` |
| a last line that is a bare expression in a function with a result | E1416 | "write `return`", fix: insert it |
| `(a, b)`, `let (a, b) = f()`, `-> (i64, str)` | E1417 | "azoL has no tuples; declare a struct with named parts" |
| `x.unwrap()` | E1420 | fix: `.expect("...")`; help mentions `.or(default)` |
| `Point { x: 1, ..base }`, `x: i64 = 0` in a struct | E1422 | "copy the value and change the field" / "give every field a value" |

### 3.9 Library

All in the library written in azoL, compiled only when used. Those marked C
need function values ([closures.md](closures.md)); those marked T need
`Map[K, V]` or bounds ([traits.md](traits.md)). The rest can be written
today.

| On | Function | Result | Needs |
|---|---|---|---|
| `Option[T]` | `o.or(default: T)` | `T` | |
| | `o.expect(message: str)` | `T`, or stops with `azol: <message>` | |
| | `o.is_some()`, `o.is_none()` | `bool` | |
| | `o.map(f: fn(T) -> U)` | `Option[U]` | C |
| `Result[T, E]` | `r.or(default: T)` | `T` | |
| | `r.expect(message: str)` | `T`, or stops with the message and the error's text | |
| | `r.is_ok()`, `r.is_err()` | `bool` | |
| | `r.ok()` | `Option[T]` | |
| `Map[K, V]` | `m.get_or(key: K, default: V)` | `V` | T |
| | `m.add(key: K, amount: i64)` on `Map[K, i64]`: a missing key counts as 0 | nothing | T |
| | `m.append(key: K, item: T)` on `Map[K, List[T]]`: a missing key is an empty list | nothing | T |
| | `m.values()` | `List[V]`, in the order of `keys()` | T |
| | `m.is_empty()` | `bool` | |
| `List[T]` | `xs.join(separator: str)` for any `T` that `str()` accepts | `str` | T |
| | `xs.swap(i: i64, j: i64)` | nothing | |
| | `xs.reverse()` in place; `xs.reversed()` exists | nothing | |
| | `xs.remove(item)`: removes the first equal one, says whether there was one | `bool` | |
| | `xs.slice(from: i64, to: i64)` | `List[T]` | |
| | `xs.map(f)`, `xs.filter(keep)`, `xs.fold(start, f)`, `xs.any(test)`, `xs.all(test)`, `xs.first_where(test)`, `xs.index_where(test)`, `xs.count_where(test)`, `xs.sorted_by(before)`, `xs.sorted_by_key(key)` | as in closures.md 4.4 | C |
| | `xs.sum_by(f: fn(T) -> i64)` | `i64` | C |
| | `xs.min_by(key: fn(T) -> K)`, `xs.max_by(key)` | `Option[T]` | C, T |
| | `xs.group_by(key: fn(T) -> K)` | `Map[K, List[T]]` | C, T |
| | `xs.keep(test: fn(T) -> bool)` in place | nothing | C |
| `i64` | `n.sign()` | -1, 0 or 1 | |
| `str` | `s.chars()` | `List[str]`, one per character | |

Decided against, so the question is not asked again: `s.join(parts)`
(`parts.join(s)` is the one way), `n.times(text)` (`text.repeat(n)`
exists), `xs.last_index()` (`xs.len() - 1`). There is no `zip` and no
`enumerate`: `for i, x in xs`
gives the index, and a second list is read with `ys[i]`. There is no `each`
(closures.md): `for` acts on every item.

Every one of these gets its signature and one example in the list of
built-ins in `docs/for-agents.md`, with the return type, because three
models wrote their own search loops rather than trust `index_of`.

## 4. The gain

### 4.1 Ten programs, by hand

Five small tasks and five hard ones, rewritten by hand in the language of
section 3 with the library of 3.9 and the closures and `Map[K, V]` of the
other designs. Files: `/tmp/az/concise/hand/`. They keep the same behaviour
and output. They have not been compiled. Code lines and tokens:

| Program | azoL today | proposed | Rust | tokens today | proposed | Rust |
|---|---|---|---|---|---|---|
| `wordfreq` | 28 | 15 | 13 | 169 | 114 | 145 |
| `primes` | 28 | 21 | 23 | 139 | 118 | 154 |
| `parse_kv` | 34 | 26 | 23 | 203 | 167 | 192 |
| `matrix` | 40 | 28 | 30 | 259 | 208 | 270 |
| `inventory` | 30 | 23 | 26 | 201 | 160 | 188 |
| `ledger` | 96 | 64 | 83 | 654 | 494 | 535 |
| `lru` | 76 | 65 | 81 | 471 | 389 | 570 |
| `sim` | 127 | 86 | 79 | 798 | 671 | 733 |
| `tree_edit` | 134 | 106 | 89 | 770 | 726 | 644 |
| `graph` | 76 | 65 | 94 | 549 | 509 | 677 |
| Total | 669 | 499 | 541 | 4,213 | 3,556 | 4,108 |

Lines fall 25%; the total is 8% below Rust in lines and 13% below in
tokens. The target, at or below Rust's line count on the corpus, is reached
in total and for six programs. It is not reached for four:

- `wordfreq` (15 against 13): Rust sorts pairs; azoL has no tuples, so the
  sort looks counts up again. A `Map` method `m.sorted_keys_by_value()`
  would close it; that is too narrow to add.
- `parse_kv` (26 against 23): the `Pair` struct, where Rust returns a tuple.
- `sim` (86 against 79): two value `match`es with `continue`, where Rust has
  `let Some(..) = .. else { continue }` on one line each.
- `tree_edit` (106 against 89): the Rust version is a recursive
  `Option<Box<Node>>` tree; the models wrote azoL's as nodes in a list with
  indexes. This is a difference of data structure, not of syntax, and was
  not examined here.

Closing the first three means tuples and `let ... else`, both rejected in
section 2 for adding a second way to do something. The recommendation is to
accept being two or three lines longer on such programs.

How often each proposed form appears in the ten rewrites (counted in the
files, not an attribution of the 170 saved lines, which overlap): one-line
`if` 13 times, each replacing three lines by one; ranges 9, each saving two
lines; value `if`/`match` 9; `or`, `get_or`, `add`, `append`, `is_none` 11
calls; function literals 5 (`filter` twice, `index_where`, `map`,
`sorted_by`); compound assignment 8 and interpolation on 19 lines, which
save tokens and not lines. The rest of the saving needs no feature: `else
if` and `!`, which the models avoided, and defensive copies they did not
need.

### 4.2 Three stretches of the compiler

| Stretch | Lines today | proposed | tokens today | proposed |
|---|---|---|---|---|
| `parser.azl` 630–691 (`parse_let`, `parse_if`) | 61 | 33 | 304 | 283 |
| `lexer.azl` 354–386 (`count_characters`, `line_column`) | 30 | 22 | 174 | 147 |
| `sema_fn.azl` 1142–1285 (`ck_let`, `ck_return`, `ck_for`) | 134 | 103 | 908 | 899 |
| Total | 225 | 158 | 1,386 | 1,329 |

Lines fall 30%: the one-line `if` appears 23 times, value `if` 3 times,
ranges twice, compound assignment 7 times. No closures and no new library
were used, apart from `offset.min(...)`, which exists. Over the whole compiler the
mechanical floor of 1.3 is 8.5%; on these three stretches the hand result is 30%, but they were chosen
because they are typical of guard-heavy code, so the whole compiler will
land between the two figures. Most of it comes from OPEN C3.

### 4.3 Two rewrites in full

`ledger`, 96 lines today, 64 proposed, 83 in Rust:

```text
struct Transfer {
    src: str
    dst: str
    amount: i64
}

fn apply_all(ledger: mut Map[str, i64], transfers: List[Transfer]) -> List[str] {
    var rejected: List[str] = []
    for t in transfers {
        let label = "{t.src} -> {t.dst} {t.amount}"
        let have = match ledger.get(t.src) {
            Some(n) => n
            None => {
                rejected.push("rejected {label}: unknown account {t.src}")
                continue
            }
        }
        if t.amount > have {
            rejected.push("rejected {label}: balance is {have}")
            continue
        }
        ledger.add(t.src, -t.amount)
        ledger.add(t.dst, t.amount)
    }
    return rejected
}

fn bracket(balance: i64) -> str {
    if balance < 100 { return "0-99" }
    if balance < 1000 { return "100-999" }
    return "1000+"
}

fn group(ledger: Map[str, i64]) -> Map[str, List[str]] {
    var groups: Map[str, List[str]] = new_map()
    for name in ledger.keys().sorted() {
        groups.append(bracket(ledger.get_or(name, 0)), name)
    }
    return groups
}

fn main() {
    var ledger: Map[str, i64] = new_map()
    ledger.set("alice", 1200)
    ledger.set("bob", 150)
    ledger.set("carol", 40)
    ledger.set("dave", 0)
    ledger.set("erin", 990)
    let transfers = [
        Transfer { src: "alice", dst: "bob", amount: 300 },
        Transfer { src: "carol", dst: "dave", amount: 50 },
        Transfer { src: "bob", dst: "carol", amount: 400 },
        Transfer { src: "erin", dst: "alice", amount: 100 },
        Transfer { src: "dave", dst: "erin", amount: 10 },
        Transfer { src: "carol", dst: "dave", amount: 440 },
        Transfer { src: "bob", dst: "alice", amount: 51 },
        Transfer { src: "erin", dst: "alice", amount: 200 },
        Transfer { src: "zed", dst: "bob", amount: 5 }
    ]
    let rejected = apply_all(mut ledger, transfers)
    let groups = group(ledger)
    for b in groups.keys().sorted() {
        print("{b}: " + groups.get_or(b, []).join(", "))
    }
    for line in rejected {
        print(line)
    }
}
```

`sim`, 127 lines today, 86 proposed, 79 in Rust:

```text
struct Entity {
    id: i64
    x: i64
    y: i64
    hp: i64
    target: Option[i64]
}

fn find_index(es: List[Entity], id: i64) -> Option[i64] {
    return es.index_where(fn(e: Entity) -> bool { return e.id == id })
}

fn sign(n: i64) -> i64 {
    if n > 0 { return 1 }
    if n < 0 { return -1 }
    return 0
}

fn nearest(es: List[Entity], k: i64) -> Option[i64] {
    var best: Option[i64] = None
    var best_d = -1
    for j, other in es {
        if j == k { continue }
        let d = (other.x - es[k].x).abs() + (other.y - es[k].y).abs()
        if best_d < 0 || d < best_d {
            best_d = d
            best = Some(other.id)
        }
    }
    return best
}

fn tick(es: mut List[Entity]) {
    for i in 0..es.len() {
        let cur = es[i]
        if cur.hp <= 0 { continue }
        let tid = match cur.target {
            Some(tid) => tid
            None => continue
        }
        let j = match find_index(es, tid) {
            Some(j) => j
            None => continue
        }
        let dx = es[j].x - cur.x
        let dy = es[j].y - cur.y
        if dx.abs() <= 1 && dy.abs() <= 1 {
            es[j].hp = (es[j].hp - 3).max(0)
        } else {
            es[i].x += sign(dx)
            es[i].y += sign(dy)
        }
    }
    es = es.filter(fn(e: Entity) -> bool { return e.hp > 0 })
    for k in 0..es.len() {
        let lost = match es[k].target {
            Some(tid) => find_index(es, tid).is_none()
            None => true
        }
        if lost {
            let pick = nearest(es, k)
            es[k].target = pick
        }
    }
}

fn show(es: List[Entity]) {
    for e in es {
        let goal = match e.target {
            Some(tid) => "#{tid}"
            None => "none"
        }
        print("  #{e.id} at ({e.x},{e.y}) hp {e.hp} -> {goal}")
    }
}

fn main() {
    var es = [
        Entity { id: 1, x: 0, y: 0, hp: 10, target: Some(2) },
        Entity { id: 2, x: 5, y: 0, hp: 9, target: Some(3) },
        Entity { id: 3, x: 5, y: 5, hp: 12, target: Some(1) },
        Entity { id: 4, x: 0, y: 5, hp: 6, target: Some(1) },
        Entity { id: 5, x: 2, y: 2, hp: 7, target: Some(4) }
    ]
    print("start")
    show(es)
    for t in 1..=12 {
        tick(mut es)
        print("tick {t}")
        show(es)
    }
}
```

And a stretch of the parser, 61 lines today, 33 proposed:

```text
fn parse_let(p: mut Parser, pos: i64) -> i64 {
    let mutable = p.kind == TOK_VAR
    bump(mut p)
    let name = expect_name(mut p, "a variable name")
    if p.failed { return NO_NODE }
    let ty = if eat(mut p, TOK_COLON) { parse_type(mut p) } else { NO_NODE }
    if p.failed { return NO_NODE }
    expect(mut p, TOK_ASSIGN)
    if p.failed { return NO_NODE }
    skip_newlines(mut p)
    let value = parse_expr(mut p)
    if p.failed { return NO_NODE }
    return push_stmt(mut p, StmtLet(mutable, name, ty, value), pos)
}

fn parse_if(p: mut Parser) -> i64 {
    enter(mut p)
    if p.failed { return NO_NODE }
    let pos = here(p)
    bump(mut p)
    let cond = parse_condition(mut p)
    if p.failed { return NO_NODE }
    let then_block = parse_block(mut p)
    if p.failed { return NO_NODE }
    var else_branch = ElseNone
    if kind_after_newlines(p) == TOK_ELSE {
        skip_newlines(mut p)
        bump(mut p)
        else_branch = if p.kind == TOK_IF { ElseIf(parse_if(mut p)) } else { ElseBlock(parse_block(mut p)) }
        if p.failed { return NO_NODE }
    }
    p.depth -= 1
    return push_stmt(mut p, StmtIf(cond, then_block, else_branch), pos)
}
```

## 5. Does it create problems?

### 5.1 Per feature

| Feature | How it could mislead | Guard |
|---|---|---|
| Compound assignment | `a[f()] += 1` calling `f` twice if lowered naively. | The rule in 3.1 and a test with a counting function. |
| | `x =+ 1` typed for `x += 1` assigns `+1` silently. | E1401 also covers `=+`, `=-` written without a space before the sign: error with fix. |
| Ranges | Off by one between `..` and `..=`. | Same meaning as Rust and Swift (`..<` is rejected with the fix `..`); the fuzzer compares every range loop with its `while` form. |
| | `a..b step -1` with `a < b` runs zero times, which may surprise. | Documented with an example; when both bounds are constants and the loop cannot run, error E1404 ("this loop never runs"). |
| | `step` as a variable name today. | It stays a free name; it is a word only after the second bound. A program with `step` there today cannot exist. |
| | Precedence: `i + 1..n`. | `..` binds looser than every operator; both bounds are full expressions. Same as Rust. |
| Value `if`/`match` | A branch with a side effect and a value: `{ log(x) x }`. | Not expressible: one expression per branch (E1407). |
| | Hidden copy of a large value chosen by a branch. | Same rule as `let x = y` today; `azol explain-line` (agents.md R8) shows it. |
| | Models write the statement `if` and expect a value from the last line. | E1416 with the fix. |
| | Deep nesting on one line. | Not allowed inside expressions (E1409); `azol fmt` breaks lines over 100 columns. |
| Interpolation | A literal `{` in JSON-like output. | E1411 with the fix `\{`; never silent. |
| | Evaluation order with calls that change things: `"{pop(mut xs)} {xs.len()}"`. | Left to right, stated; the read-and-change rule applies across the parts as across `+` today. |
| | Hidden allocation. | The same in-place append as the `+` chain; no new cost. |
| | Injection-like surprises when a value contains `{`. | Values are inserted as text, never read again as a template. |
| Nested patterns | An arm shadowed by an earlier, wider arm. | Unreachable arm is an error, as today. |
| | A lower-case name that the author meant as a variant. | Variants are capitalised by convention only; a pattern name that equals no variant but differs from one only by case gets a help line. |
| Field shorthand | A field named like an unrelated variable picks it up. | Only when the author writes the bare name; types still must match. |
| One-line `if` | A long condition hides the jump at the right edge. | Only when the whole line fits in 100 columns; `return` at the end of a line is still the last word. |
| | Two layouts for the same code in old files. | `azol fmt` joins as well as splits; `fmt --check` in the test suite. |
| `.or`, `.expect`, `get_or` | `.or(expensive())` evaluates the default always. | Stated in the signature's text; a value `match` is the form when the default is costly. |
| | `.expect` used where a `Result` should travel. | The message is required, so the choice is visible in review. |
| `add`, `append` on maps | Reads as arithmetic on the map. | Named in the briefing next to `set`; they exist only for `i64` and list values, otherwise the error lists the right method. |

### 5.2 What must not get worse

- **Error messages.** Each rejected foreign form has its own code and fix
  (3.8), so widening the language does not turn precise errors into generic
  ones. The habit table of agents.md gains these rows and its test (every
  row tried against the compiler) covers them.
- **One way.** After this change the old forms still compile: `x = x + 1`,
  `while` counting loops, default-then-match, `+` chains. That is two ways
  for a time. The rule: the documentation and every example show only the
  new form; `azol fix` rewrites the old to the new where the rewrite is
  provably the same (the cases of the mechanical script in 1.3); nothing old
  is ever removed or made a warning, because old code must keep building.
- **Local reasoning.** No feature needs another file or another function to
  be read: a range's bounds, a value branch's type, a field shorthand and an
  interpolated name are all visible on the line.

### 5.3 The measurement that decides

The first-try measurement is repeated before the features are called done,
with fresh models that have seen nothing:

1. The same 14 tasks, the updated `AGENTS.md` and `docs/for-agents.md`,
   three models per round as before, plus the same Rust control.
2. Pass rule: the first-build-and-correct rate is not lower than today's
   (33 of 34), and the mean code lines per task fall by at least 15%.
3. A third round on six new tasks nobody has tuned for, so the result is
   not fitted to the corpus.
4. Every compiler error a model meets is recorded with the habit behind it;
   an error without a fix that a fix could have had is a defect to repair
   before release.
5. If the first-try rate drops, the feature whose errors caused it is
   withdrawn or narrowed. Shorter code that fails more often is not the
   goal.

The differential fuzzer gains each form (`+=` against `=`, ranges against
`while`, value `if`/`match` against the statement form, interpolation
against `+`), each compared with its long form in the same program.

## 6. Implementation plan

Pieces that can be built in parallel, ordered by lines saved per line of
compiler written. "Saves" counts sites in the ten programs of 4.1 (669 lines) and in
the compiler (16,709 lines, from the mechanical rewrite of 1.3). Sizes are
estimates.

| # | Piece | Where | Size | Saves, corpus | Saves, compiler |
|---|---|---|---|---|---|
| 1 | One-line `if` in the formatter | `format.azl` only | 80 | 13 sites, 26 lines | 557 sites, 1,114 lines, 6.7% |
| 2 | Compound assignment | lexer, parser; checker for indexed places | 150 | tokens | 534 sites, tokens |
| 3 | Field shorthand | parser | 30 | 9 sites | 45 sites |
| 4 | `or`, `expect`, `is_some`, `is_none`, `ok`, `swap`, `reverse`, `sign`, `remove`, `slice`, `chars` | library only | 150 | 11 calls with piece 9 | small |
| 5 | Ranges in `for` | lexer, parser, checker, 30 lines in each generator | 350 | 9 sites, 18 lines | 150 sites, 300 lines, 1.8% |
| 6 | Value `if`/`match` | parser, checker | 450 | 9 sites, with pieces 4 and 9 the largest saving | 60 `var`-then-`if` sites, about 180 lines |
| 7 | Interpolation | lexer, parser | 300 | tokens, 15% of them | 127 sites |
| 8 | Rejected forms and their fixes (3.8), habit rows | parser, `diag` table | 300 | none; keeps errors exact | none |
| 9 | `get_or`, `add`, `append`, `values`, `join` on any list | library, after `Map[K, V]` | 120 | see piece 4 | none |
| 10 | Library with function values (3.9, marked C) | library, after closures | 350 | 5 sites | few |
| 11 | Nested patterns | checker | 500 | few | few |
| 12 | `azol fix` rewrites of old forms | `diag`, driver | 300 | applies 1–7 to existing code | applies them |

Pieces 1, 3 and 4 touch disjoint files and can start at once. Pieces 2, 5,
6 and 7 all change the lexer and parser: one agent builds them in that
order, or four agents build them on branches that are merged one after
another. Piece 8 follows them. Pieces 9 and 10 wait for traits and closures.
Piece 11 is independent of all but the checker. About 3,100 lines in all.

Documentation and the fuzzer move with each piece: `AGENTS.md`,
`docs/for-agents.md` and `docs/language.md` show the new form the day it
lands, and `docs/check_docs.py` compiles every example.

## Doubts

- The hand rewrites were made by the author of the design and not compiled;
  a model that has only the documentation may write longer code than these.
  Section 5.3 measures that.
- The Rust programs were taken as written; a Rust programmer aiming at
  brevity could shorten them too.
- Line counts depend on layout rules. OPEN C3 changes azoL's layout; Rust's
  formatter keeps `if x { return; }` on three lines, so part of the gain
  against Rust is a layout choice, not expressiveness. Tokens, where azoL
  ends 13% below Rust, do not depend on layout.
- The pattern counts come from a script with regular expressions, checked by
  reading about a fifth of the corpus; individual counts may be off by a few
  percent.
- The claim that value `if`/`match` needs no generator change rests on how
  the checker tracks unfilled slots, and was not verified in the code.
- `tree_edit` was not analysed for a data-structure fix.

## 7. As built: the second half

What was built differs from the sections above in these points.

- **Values from `if` and `match` (3.3) are lowered in the parser**, not in
  the checker: to a `var` without a value, the statement form assigning in
  each branch, and a read. The checker gives the hidden variable the type of
  the first branch. A branch may leave instead of giving a value: `return`,
  `break`, `continue` or `exit(...)`.
- **A name bound twice in one pattern (3.5) is E0507**, the code that
  already said it for flat patterns; E1418 was not used.
- **A missing arm in a value `match` gets no fix.** The statement form's fix
  adds `Variant(_) => {}`, which is not a value; only the author knows the
  value, so `check --json` reports the missing case without an edit.
- **Library (3.9):** every function not marked C or T is in
  `lib/prelude.azl`. Of those marked T, `get_or`, `add`, `append`, `values`
  and the general `join` were written for today's `Map[V]` with text keys
  and for the element types `str()` accepts; they keep their names when
  `Map[K, V]` arrives. Those marked C wait for function values.
- **The compiler's own sources do not use the second-half forms yet**: the
  seeds in `bootstrap/` were built before them. They can once new seeds are
  checked in.
