# Built for agents: what it has to mean, and the plan

Status: design. Nothing here is implemented unless a line says "exists today".
Every statement about today's compiler was checked against the compiler in
this repository at the commit this file was written on (`azol 0.1.0`,
74 error codes, `docs/for-agents.md` 18,898 bytes).

The goal, in the user's words: when an AI agent is asked to replace an old
system and would today rewrite it in Rust, it should choose azoL, because
Rust is workable for agents but was not built for them, and azoL is.

That is a claim about measured behaviour of models, not about taste. So this
document turns it into requirements that a script can check, and a number
that can be published. "100% compatible" is a direction: nobody can promise
that every model writes every program right. What can be promised, after
measuring, is a sentence of this form: *"Given only `azol guide`, model M
built N of 40 tasks on the first try and solved all 40 within K compiler
rounds; the same model on the same tasks in Rust: N', K'."* Section 6 defines
it. Until that run exists, no public text may say "100%".

## What exists (13 November 2026)

The first part of this plan is built; the rest of the file is the design as
it was written, and where the two differ this section is the one that
describes the compiler.

- **`azol guide`** (R1): prints `docs/for-agents.md`, which is embedded in
  the compiler when it is built. `azol guide <topic>` prints one `##`
  section of `docs/language.md`; the topics are the nineteen sections of
  that file (`lexical`, `literals`, `types`, `declarations`, `statements`,
  `expressions`, `calls`, `methods`, `builtins`, `library`, `values`,
  `ownership`, `exclusivity`, `runtime-stops`, `files`, `cli`, `targets`,
  `limits`, `missing`), not the finer list of 1.1, because the reference is
  divided that way and a topic that is not a section could drift from it.
  `--list` and `--json` as designed. The test suite fails when the briefing
  passes 32,000 bytes, when a topic passes 16,000, or when what the command
  prints is not the file byte for byte, and when `AGENTS.md` or the briefing
  names an `azol` command that the usage text does not have.
- **`azol fix`** (R6): as in section 2, with three choices. It works on the
  texts in memory and writes at the end, so a file is either repaired or
  untouched: one in which an error without a fix remains is not written
  unless `--partial` is given. The limit is 40 rounds, not 8: every round
  makes every edit that does not overlap another, and a file full of habits
  needs about four, so the limit is only a guard against an edit that keeps
  uncovering new work. A written file is laid out by `azol fmt`.
- **The foreign-habit rule** (R4, 1.5): `compiler/habits.azl`. It is asked
  in one place, where a mistake becomes a report (`diagnose`), with what the
  lexer, the parser or the checker said and the place it is about; it reads
  the source text around the place and answers with the habit's own message
  and, when the repair is the same every time, the edit. Nothing in the
  lexer, the parser or the checker changed, so the path of a program
  without mistakes is what it was. The codes are by family, as planned:
  E0120 statement ends, E0121 declarations, E0122 `< >` and lifetimes,
  E0123 references and pointers, E0124 macros, E0125 `::` and paths, E0126
  declaration words, E0127 blocks and loops, E0128 operators, E0129
  comments and texts, E0130 the spelling of types, parameters and struct
  values, E0131 function values, E0132 exceptions and the like, E0133
  `match` arms; E0220 type names, E0221 function names, E0222 value names,
  E0223 method and field names, E0224 text and numbers.
- **Counted against the table of 1.5** (137 rows, each tried again with
  today's compiler, 14 November 2026): 119 rows are answered by one of
  these codes or by a code of the short forms (E14xx) that names the habit.
  11 rows are accepted or have since become azoL (`+=`, ranges, `"{x}"`,
  field shorthand, `if` as a value, and the six that were always
  accepted). 6 rows already had a message that states the rule (E0111,
  E0303, E0311, E0413, E0601, E0804). 1 row is still answered generically:
  `f64` (row 42, which floats.md makes a type). **98 of the 137 rows carry
  an edit, 72%**, against the 70% that R4 asks for. The 23 named rows
  without one are those whose repair needs a decision: `null` and `nil`,
  `impl`, `class`, `trait`, closures and `.map(...)`, `switch`, a guard in
  a `match` arm, tuples, `unwrap` and `x!`, a map literal, `defer`,
  `throw`, two names declared at once, a text over two lines, default
  arguments, `Box`.
  Beyond the table, about sixty more names and forms are recognised
  (`fmt.Println`, `xs.size()`, `s.toUpperCase()`, `max(a, b)`, `Vec::new()`,
  `pub` fields, `for (i, x) in xs.iter().enumerate()`, `not (x)`, `'text'`).
- **Edits that read more than the place** (`compiler/habits_more.azl`):
  `for (i = 0; i < n; i++)` to `for i in 0..n`; `range(a, b, step)`; a
  block that Python closes by its depth, with its `else:`; `if (x) f()`
  without braces; `if let` without `else` to a `match`; `c ? a : b` as the
  whole right side; `1 | 2 => f()` to an arm each; `printf("%d\n", n)` and
  `print(a, b)` to one text; `xs[a..b]` to `.slice`; `m[k] = v` to
  `m.set`; `HashMap[str, V]`; `new P(1, 2)` with the fields of `P`;
  `panic("why")` and `assert(test)`; `while 1`; `const` in a function;
  `~x`; `"\x41"` and `"\u{e9}"`; `'a` between brackets; a text between
  backquotes. Each is made only for the one plain shape it knows, and its
  cases (`tests/errors/habits__edits_that_read_more.cases`) build the
  repaired program and run it, so an edit is known to keep what the
  program does; shapes it must leave alone have cases of their own.
- **Not built:** `azol new`, `llms.txt`, `azol example`, `explain-line`,
  `explain --json`, the schema file and `"schema"` in `check --json`,
  several errors per function, `help:` and `fix:` lines in the text form
  of an error, `azol test`. They stay as designed below.

## Open questions for the user

Three here, four in [legacy.md](legacy.md). Each has a recommendation; work
proceeds on the recommendation unless the user says otherwise.

- `OPEN A1:` Text such as `"x = {x}"` compiles today and prints the braces.
  Agents coming from Rust, Python and JavaScript write it constantly.
  Recommendation: make it an error (new code) when the text between `{` and
  `}` is exactly a name in scope, with the fix `"x = " + str(x)`; a literal
  brace before a name is then written `\{`. No string literal in any `.azl`
  file of this repository has that shape (searched), so nothing existing
  breaks.
- `OPEN A2:` Should randomness and wall-clock time be ambient built-ins?
  Recommendation: no ambient randomness at all (a seeded `Rng` value passed
  explicitly); a monotonic `clock()` stays a plain built-in because it cannot
  change a program's results, only its timing output; calendar time comes
  through an explicit `now()` that tests can replace (section 5.4).
- `OPEN A3:` Contracts in the language (`requires`, `ensures`) or only
  `assert` in the library? Recommendation: `assert` as a built-in statement
  now; `requires`/`ensures` as two more statements at the top of a function
  body, always checked, in the same release as `azol test` (section 5.3).

## The requirements, in one table

| # | Requirement | Test that decides it | Today |
|---|---|---|---|
| R1 | The compiler teaches the language: `azol guide` prints a complete briefing that fits in 8,000 tokens | `azol guide \| wc -c` ≤ 32,000 bytes; every example in it compiles (`check_docs.py`) | Briefing exists as a file (18,898 bytes, about 5,000 tokens), not as a command |
| R2 | Every project carries the briefing pointer: `azol new` writes `AGENTS.md` and `llms.txt` | files exist and name the installed compiler's version | No `azol new` |
| R3 | Examples are retrievable by task: `azol example <topic>` | each topic prints a program that compiles and runs with the shown output | No command |
| R4 | A construct borrowed from another language gets an error that names the habit, shows the azoL form, and carries a fix | the habit table below: every row has a specific message; ≥ 70% carry a fix | 137 habits tried: 80 get a generic grammar or "unexpected character" message, 8 are accepted, **0 carry a fix** |
| R5 | One round is enough: all independent errors are reported, each with code, rule, help and, where mechanical, fix | fixture programs with k planted errors report k | One error per function; fixes on 7 of 74 codes |
| R6 | `azol fix` applies every safe fix and reports what is left | fixtures: fixed file equals expected file; second run changes nothing | No command; `fix` objects exist in `--json` |
| R7 | Every command has `--json`, a schema version, fixed exit codes, deterministic order, no colour, no width dependence | schema file in the repository; golden outputs | `check`/`build` have `--json`, no version field |
| R8 | Meaning is local: what a line does is decidable from the line and the declarations it names; where the compiler chooses, a tool shows the choice | `azol explain-line` golden outputs; the list in section 4 has no "hidden" row left | Six non-local places, listed in section 4 |
| R9 | An agent can prove its port: `test` declarations, `assert`, golden tests against recorded input and output, `azol test --json` | the 40-task suite is itself written with them | None of it exists |
| R10 | The claim is measured: the benchmark of section 6 is run on every release and azoL beats the Rust control on every metric | `bench/agents/` results file in the release | A first informal measurement is in progress outside this document |

## 1. Cold start: the toolchain teaches the language

No model has seen azoL. Everything a model needs must arrive in its context
at the moment it starts writing, from a place it will look without being
told: the compiler binary and the project folder.

### 1.1 `azol guide`

Decision: the briefing is embedded in the compiler and printed by
`azol guide`. Reason: the binary is the one thing that is always present and
always the right version; a web page or a file in another repository is
neither.

- `azol guide` prints `docs/for-agents.md` as plain Markdown to standard
  output. Nothing else, no pager, no colour.
- `azol guide <topic>` prints one section of the reference
  (`docs/language.md`) by its stable topic name: `types`, `functions`,
  `generics`, `structs`, `enums`, `match`, `loops`, `strings`, `lists`,
  `map`, `errors` (the `?` and `Result` section), `ownership`,
  `exclusivity`, `files`, `runtime-stops`, `cli`, `limits`, `missing`, and
  one per new feature when the five designs land. `azol guide --list`
  prints the names with one line each. An unknown topic exits 2 and prints
  the list.
- `azol guide --json` prints `{"schema":1,"version":"0.1.0","topics":[{"name","title","bytes"}]}`.
- Budget: the briefing is at most **8,000 tokens**, enforced as **32,000
  bytes** (4 bytes per token is the conservative ratio for English with code).
  A test fails the build when `docs/for-agents.md` is larger. Today it is
  18,898 bytes, so there are about 13,000 bytes left for floats, closures,
  traits, threads and packages: roughly 2,600 bytes each. Rule for keeping
  it under: the briefing holds rules and one example per idiom; anything
  that is reference (full method lists with signatures, every error code)
  lives in topics. Each topic is at most 4,000 tokens (16,000 bytes), so an
  agent can pull two or three without crowding its context.
- The briefing's first lines tell the agent what to run next:
  `azol check --json <files>` after every edit, `azol fix`, `azol explain
  <code>`, `azol guide <topic>`, `azol example <task>`.
- How it gets into the binary: the `embed("...")` constant that the
  built-ins work adds; `check_docs.py` already compiles every example, and a
  new test compares `azol guide` with the file byte for byte.

### 1.2 `azol explain`

Exists today for the 74 codes (rule, what to do, wrong and right example).
Decisions:

- Keep one code per rule; add `azol explain --json <code>`.
- Add the section-4 tool `azol explain-line` (it explains a line of the
  user's program, not a code).
- The human text form gets the help line too. Today help and fix appear only
  in `--json`; an agent that reads standard error of a plain `azol build`
  (many do) never sees them. Decision: the text form prints
  `help: ...` and, when there is a fix, `fix: replace line:col-line:col with
  ...` after the source excerpt. Reason: agents must not need to know a flag
  to get the useful half of a message.

### 1.3 `AGENTS.md` and `llms.txt`

Decision: `azol new <name>` (specified in packages.md) writes both files
into every project, and `azol guide --agents-file` prints the same content
for existing projects.

- `AGENTS.md` (the convention coding agents already read at the root of a
  repository), about 30 lines: this project is written in azoL; you have not
  seen this language, do not guess from Rust; run `azol guide` once before
  writing; after every edit run `azol check --json`; apply `azol fix`; run
  `azol test --json`; format with `azol fmt`; the compiler version this
  project was created with.
- `llms.txt` at the root of the azoL repository and of the documentation
  site: the briefing's location and the topic list, in the plain format that
  convention uses. It contains links only; the content stays in `azol
  guide`, so there is one source.
- Neither file contains the briefing itself. Reason: a copied briefing goes
  stale; a command cannot.

### 1.4 `azol example <task>`

Decision: a small set of complete, tested programs embedded in the compiler,
retrievable by task word. Reason: models copy the shape of a working program
far more reliably than they apply a rule list.

- `azol example --list` prints the names. First set, each under 60 lines:
  `hello`, `args`, `files` (read, handle the error, write), `lines`
  (process a file line by line), `parse` (text to numbers with `Result`),
  `struct`, `enum` (with `match`), `generic`, `map` (count words), `sort`,
  `state-machine`, `cli` (a tool with subcommands), `bytes` (binary data in
  a `str`), `errors` (`?` through three functions), `test` (when `azol test`
  exists), and, as they land, `float`, `closure`, `trait`, `thread`,
  `package`, `http`, `json`, `c-call`.
- Each is a file under `examples/agents/` checked by the same script that
  checks the documents, with its expected output.
- `azol example <word>` that matches no name prints the list and exits 2.
  No fuzzy matching. Reason: determinism.

### 1.5 The foreign-habit rule

Rule (R4): **when the source contains a construct that is valid in Rust, Go,
Swift, TypeScript, Python or C and is not azoL, the error names that
construct, shows the azoL form, and carries a fix whenever the rewrite is
mechanical.** A generic "expected X but found Y" for such a construct is a
bug in the compiler, tracked like any other.

Measured today, by writing each habit into a small program and running
`azol check --json` (the script and the 137 programs are reproducible; the
table below is its output plus the two right-hand columns, which are the
specification):

- 137 habits tried.
- 80 are answered with the generic grammar message (E0106, E0107) or
  "unexpected character" (E0101).
- 8 are accepted. Six of those are harmless (`match` arm commas, a typed
  constant, parentheses around a condition, struct member commas,
  `fn main() -> i64`, a parameter named `self`). **Two are silently wrong:
  `"x = {x}"` and `"x = ${x}"` print the braces** (OPEN A1).
- 0 carry a fix. 95 of the 137 have a mechanical fix by the right-hand
  column, which is 69%; the target of R4 is at least 70% of rows, reached by
  adding the `for`/range rewrites.
- One message is out of date: "the built-in types so far are `i64`, `bool`,
  `str`, `List`, `Option` and `Result`" omits `Map`.

How to implement without slowing the compiler: all of it is on the error
path. The parser already stops at the first error of a file; at that point a
recogniser looks at the tokens around the failure (and, for unknown names
and methods, the checker looks the name up in two fixed tables: foreign
type names and foreign method/function names) and replaces the generic
message. New codes: one per family, in the range reserved for this work,
E0120–E0149 (syntax habits) and E0220–E0239 (name habits); the range must be
entered in the table in docs/design/README.md. Families: statement
terminator, `let mut`, angle-bracket generics, references and pointers,
macros, path and dotted variants, foreign declaration keywords (`def`,
`func`, `function`, `class`, `impl`, `trait`, `pub`, `use`, `import`,
`static`), compound assignment, foreign loops, foreign conditionals
(`elif`, `:` blocks, ternary, `if let`, `switch`), foreign operators
(`and`, `or`, `not`, `===`, `<>`, `**`, `~`), foreign literals (`null`,
`nil`, `True`, single-quoted and raw strings, interpolation), foreign types
(`int`, `i32`, `String`, `Vec`, `HashMap`, `string`, `boolean`, `T[]`,
`[]T`, `T?`, tuples), foreign methods (`clone`, `to_string`, `unwrap`,
`iter`, `append`, `length`, `push_str`, `charAt`, slicing with ranges),
foreign calls (`println`, `printf`, `console.log`, `len(x)`, `panic`,
`range`), casts, closures, comments and attributes.

#### The table

Columns three and four are facts (what was written, what the compiler in
this repository answered). Columns five and six are the requirement.
"none" in the last column means the repair needs a decision an agent must
make; the message must then say what to decide.

| # | From | What the agent writes | What `azol check` says today | What it must say | Fix it must carry |
|---|---|---|---|---|---|
| 1 | C/Rust/JS | `let x = 1;` | E0101: unexpected character | `;` does not end a statement in azoL; a new line does | delete the `;` |
| 2 | Rust | `let mut x = 1` | E0106: expected a variable name but found `mut` | `let mut` is Rust; a variable that changes is declared with `var` | replace `let mut` with `var` |
| 3 | Rust/TS | `fn first<T>(xs: List<T>)` | E0106: expected `(` but found `<` | type parameters are written in `[ ]`, not `< >` | replace `<T>` with `[T]` |
| 4 | Rust/TS | `let xs: List<i64> = []` | E0106: expected `=` but found `<` | type arguments are written in `[ ]`, not `< >` | replace `<i64>` with `[i64]` |
| 5 | Rust | `let xs: Vec<i64> = []` | E0203: unknown type `Vec`; the built-in types so far are … | `Vec` is Rust; the growable list is `List[T]` | replace `Vec` with `List` |
| 6 | Rust | `vec![1, 2]` | E0107: expected end of line but found `!` | there are no macros; a list literal is `[1, 2]` | replace `vec![1, 2]` with `[1, 2]` |
| 7 | Rust | `fn f(s: String)` | E0203: unknown type `String`; the built-in types so far are … | `String` is Rust; text is `str`, owned and growable | replace `String` with `str` |
| 8 | Rust | `fn f(s: &str)` | E0106: expected a type but found `&` | there are no references; a parameter is `s: str` and is read-only | delete the `&` |
| 9 | Rust/C | `f(&x)` | E0106: expected an expression but found `&` | there are no references; pass the value itself | delete the `&` |
| 10 | Rust | `f(&mut xs)` | E0106: expected an expression but found `&` | `&mut` is Rust; an argument the function changes is written `mut xs` | replace `&mut xs` with `mut xs` |
| 11 | Rust | `fn f(xs: &mut List[i64])` | E0106: expected a type but found `&` | `&mut` is Rust; a parameter the function changes is `xs: mut List[i64]` | replace `&mut` with `mut` |
| 12 | Rust/C | `*p = 3` | E0106: expected an expression but found `*` | there are no pointers, so nothing to dereference; assign to the variable | delete the `*` |
| 13 | Rust | `xs.clone()` | E0207: `List[i64]` has no method `clone` | assignment already copies; `.clone()` is not needed | delete `.clone()` |
| 14 | Rust | `n.to_string()` | E0207: `i64` has no method `to_string` | a number becomes text with `str(n)` | replace `n.to_string()` with `str(n)` |
| 15 | Rust | `String::from("a")` | E0107: expected end of line but found `:` | a string literal is already an owned `str` | replace `String::from("a")` with `"a"` |
| 16 | Rust | `Shape::Circle(2)` | E0107: expected end of line but found `:` | variants are written bare; there is no `::` | replace `Shape::Circle(2)` with `Circle(2)` |
| 17 | Swift/TS | `Shape.Circle(2)` | E0210: `Shape` is an enum; create a value with one of its variants | variants are written bare, without the enum's name | replace `Shape.Circle(2)` with `Circle(2)` |
| 18 | Rust | `impl Point { fn ... }` | E0106: expected `fn`, `struct`, `enum` or `const` but found a name | there is no `impl`; write `fn get(p: Point)` at the top level (see traits.md for what replaces it) | none: needs moving and renaming `self` |
| 19 | Rust/Swift | `p.area()` | E0207: `P` has no method `area` | `area` is a function, not a method; call it as `area(p)` | replace `p.area()` with `area(p)` (function of that name taking `P` exists) |
| 20 | Rust | `println!("{}", x)` | E0107: expected end of line but found `!` | there are no macros and no formatting; `print` takes one value | for the one-placeholder form: `print(x)` |
| 21 | Go/Java/Swift | `println(x)` | E0202: unknown function `println` | the function is `print`; it adds the newline | replace `println` with `print` |
| 22 | JS | `console.log(x)` | E0201: unknown variable `console` | the function is `print` | replace `console.log` with `print` |
| 23 | C | `printf("%d\n", x)` | E0202: unknown function `printf` | there is no formatting; build one string with `+` and `str(...)` | none: the format string must be rewritten |
| 24 | Python | `print("x", x)` | E0309: `print` takes 1 argument but 2 were given | `print` takes one value; join the parts with `+` and `str(...)` | replace with `print("x" + " " + str(x))` |
| 25 | Rust | `format!("{}", x)` | E0107: expected end of line but found `!` | there is no `format!`; join with `+` and `str(...)` | for the one-placeholder form: `str(x)` |
| 26 | Rust/Python | `"x = {x}"` | **accepted, no error** | ACCEPTED TODAY and prints the braces. Should be an error: `{x}` inside a string is not replaced; a literal `{` must be allowed, so this is a warning-level error only when `x` is a name in scope | replace `"x = {x}"` with `"x = " + str(x)` |
| 27 | JS/Kotlin | `"x = ${x}"` | **accepted, no error** | ACCEPTED TODAY and prints `${x}`. Same rule as above | replace with `"x = " + str(x)` |
| 28 | JS/Java | `"n = " + n` | E0302: `+` needs `str` but this is `i64` | `+` does not convert; wrap the number in `str(...)` | replace `n` with `str(n)` |
| 29 | JS/Java/C | `let x = null` | E0201: unknown variable `null` | there is no `null`; an absent value is `None` of an `Option[T]` | none: the variable needs an `Option` type |
| 30 | Go/Swift | `let x = nil` | E0201: unknown variable `nil` | there is no `nil`; an absent value is `None` of an `Option[T]` | none: as above |
| 31 | Python | `return None for i64` | E0601: cannot tell which `Option` this is; give the variable a type, as in `let x: Option[i64] = ...` | `None` is a value of `Option[T]`, but `f` returns `i64`; return `Option[i64]` or a number | none: a design choice for the author |
| 32 | C/JS | `for (i = 0; i < n; i++)` | E0101: unexpected character | there is no C-style `for`; count with `while` | rewrite to `var i = 0` / `while i < 3 { ... i = i + 1 }` (mechanical for the canonical three-part form) |
| 33 | Rust | `for i in 0..n` | E0106: expected a field name but found `.` | there are no ranges; count with `while` | rewrite to the `while` form (mechanical) |
| 34 | Python | `for i in range(3)` | E0202: unknown function `range` | there is no `range`; count with `while` | rewrite to the `while` form (mechanical) |
| 35 | C/Rust/Go | `x += 1` | E0106: expected an expression but found `=` | there is no `+=`; write `x = x + 1` | replace `x += 1` with `x = x + 1` |
| 36 | C/JS | `x++` | E0106: expected an expression but found `+` | there is no `++`; write `x = x + 1` | replace `x++` with `x = x + 1` |
| 37 | Go | `x := 1` | E0107: expected end of line but found `:` | `:=` is Go; declare with `let` or `var` | replace `x := 1` with `let x = 1` |
| 38 | Go | `var x int = 1` | E0106: expected `=` but found a name | a type follows a colon: `var x: i64 = 1` | insert `:` (and see the next row for `int`) |
| 39 | Go/C | `let x: int = 1` | E0203: unknown type `int`; the built-in types so far are … | `int` is not a type; the integer type is `i64` | replace `int` with `i64` |
| 40 | Rust | `let x: i32 = 1` | E0203: unknown type `i32`; the built-in types so far are … | the only integer type is `i64` | replace `i32` with `i64` |
| 41 | Rust | `let n: usize = 1` | E0203: unknown type `usize`; the built-in types so far are … | the only integer type is `i64`; lengths and indexes are `i64` | replace `usize` with `i64` |
| 42 | Rust | `let x: f64 = 1.5` | E0203: unknown type `f64`; the built-in types so far are … | there are no floating-point numbers yet (floats.md) | none |
| 43 | all | `let x = 1.5` | E0804: there are no floating-point numbers yet; the only number type is `i64` | already specific (E0804) | none |
| 44 | TS | `let s: string = "a"` | E0203: unknown type `string`; the built-in types so far are … | the text type is `str` | replace `string` with `str` |
| 45 | TS/Java | `let b: boolean = true` | E0203: unknown type `boolean`; the built-in types so far are … | the type is `bool` | replace `boolean` with `bool` |
| 46 | TS | `let xs: i64[] = [1]` | E0106: expected a type but found `]` | a list type is `List[i64]` | replace `i64[]` with `List[i64]` |
| 47 | Go | `let xs: []i64 = [1]` | E0106: expected a type but found `[` | a list type is `List[i64]` | replace `[]i64` with `List[i64]` |
| 48 | Rust | `Option<i64>` | E0106: expected `{` but found `<` | type arguments are written in `[ ]`: `Option[i64]` | replace `<i64>` with `[i64]` |
| 49 | Swift/TS | `let x: i64? = None` | E0106: expected `=` but found `?` | an optional type is `Option[i64]` | replace `i64?` with `Option[i64]` |
| 50 | Rust | `opt.unwrap()` | E0207: `Option[i64]` has no method `unwrap` | there is no `unwrap`; use `match`, or `?` in a function that returns an `Option` | none: needs a decision about the `None` case |
| 51 | Rust/Swift | `if let Some(x) = opt` | E0106: expected an expression but found `let` | there is no `if let`; use `match` with a `_ => {}` arm | rewrite to `match o { Some(x) => { ... } None => {} }` (mechanical) |
| 52 | Rust | `arm => expr,` | **accepted, no error** | accepted today (commas are allowed between arms); `azol fmt` removes them. Keep | — |
| 53 | Rust | `1 \| 2 =>` | E0106: expected `=>` but found `\|` | one pattern per arm; repeat the arm | duplicate the arm for each alternative (mechanical) |
| 54 | Rust | `x if x > 0 =>` | E0106: expected `=>` but found `if` | there are no guards; test with `if` inside the arm | none: later arms change meaning |
| 55 | C/JS/Go | `switch n { case 1: }` | E0107: expected end of line but found a name | there is no `switch`; use `match n { 1 => ... _ => ... }` | none |
| 56 | Rust | `let x = if c { 1 } else { 2 }` | E0106: expected an expression but found `if` | `if` is a statement and has no value; assign in each branch | rewrite to `var x = 1` ... when both branches are literals; otherwise none |
| 57 | C/JS | `c ? 1 : 2` | E0107: expected end of line but found a number | there is no `?:`; use `if`/`else` and assign in each branch | none |
| 58 | Rust | `fn f() -> i64 { 1 }` | E0305: function `f` must return a value of type `i64` on every path | the last expression is not returned; write `return 1` | insert `return ` before the final expression statement |
| 59 | C/JS | `if (x > 0) {` | **accepted, no error** | accepted today (parentheses are just an expression). Keep; `azol fmt` should remove them | — |
| 60 | C | `if (x > 0) print(x)` | E0106: expected `{` but found a name | a block needs braces: `if x > 0 { ... }` | wrap the statement in `{ }` |
| 61 | Python | `elif` | E0107: expected end of line but found a name | write `else if` | replace `elif` with `else if` |
| 62 | Python | `if x > 0:` | E0106: expected `{` but found `:` | blocks use braces, not `:` and indentation | none: block end is not known |
| 63 | Python | `a and b` | E0106: expected `{` but found a name | the operators are `&&` and `\|\|` | replace `and` with `&&` |
| 64 | Python | `not a` | E0106: expected `{` but found a name | the operator is `!` | replace `not ` with `!` |
| 65 | Python | `True` | E0201: unknown variable `True` | the literals are `true` and `false` | replace `True` with `true` |
| 66 | Python | `def f():` | E0106: expected `fn`, `struct`, `enum` or `const` but found a name | functions are declared with `fn` | replace `def` with `fn` |
| 67 | Go/Swift | `func f() {` | E0106: expected `fn`, `struct`, `enum` or `const` but found a name | functions are declared with `fn` | replace `func` with `fn` |
| 68 | JS | `function f() {` | E0106: expected `fn`, `struct`, `enum` or `const` but found a name | functions are declared with `fn` | replace `function` with `fn` |
| 69 | Rust | `pub fn f()` | E0106: expected `fn`, `struct`, `enum` or `const` but found a name | there is no `pub` yet; every declaration is visible to the whole program (packages.md) | delete `pub ` |
| 70 | Rust | `use std::fs` | E0106: expected `fn`, `struct`, `enum` or `const` but found a name | there are no imports yet; list the files on the command line (packages.md) | delete the line |
| 71 | Go/Python/TS | `import "fmt"` | E0106: expected `fn`, `struct`, `enum` or `const` but found a name | there are no imports yet; list the files on the command line (packages.md) | delete the line |
| 72 | Go | `fn f() i64 {` | E0106: expected `{` but found a name | a result type follows `->`: `fn f() -> i64 {` | insert `-> ` |
| 73 | TS/Python | `fn f(): i64 {` | E0106: expected `{` but found `:` | a result type follows `->`, not `:` | replace `:` with ` ->` |
| 74 | C/Java | `fn f(i64 x)` | E0106: expected `:` but found a name | a parameter is `name: type` | swap to `x: i64` |
| 75 | Rust | `\|x\| x + 1` | E0106: expected an expression but found `\|` | there are no closures yet (closures.md); write a named function | none |
| 76 | JS/TS | `(x) => x + 1` | E0107: expected end of line but found `=>` | there are no closures yet (closures.md); write a named function | none |
| 77 | Python | `lambda x: x + 1` | E0107: expected end of line but found a name | there are no closures yet (closures.md); write a named function | none |
| 78 | Rust/JS | `xs.map(...)` | E0207: `List[i64]` has no method `map` | lists have no `map` yet (closures.md); write the loop | none until closures exist |
| 79 | Rust | `xs.iter().sum()` | E0207: `List[i64]` has no method `iter` | there are no iterators; lists have their methods directly: `xs.sum()` | delete `.iter()` |
| 80 | Python/Go | `len(xs)` | E0202: unknown function `len` | length is a method: `xs.len()` | replace `len(xs)` with `xs.len()` |
| 81 | JS/Java | `xs.length` | E0206: `List[i64]` has no field `length`; its methods are `len()`, `push(...)` and `pop()` | already close; the message lists only three methods and should say `xs.len()` | replace `.length` with `.len()` |
| 82 | Go/Python | `xs.append(1)` | E0207: `List[i64]` has no method `append` | the method is `push` | replace `append` with `push` |
| 83 | Rust | `s.push_str("a")` | E0207: `str` has no method `push_str` | strings are joined with `+`: `s = s + "b"` appends in place | replace `s.push_str("b")` with `s = s + "b"` |
| 84 | Java/JS | `s.charAt(0)` | E0207: `str` has no method `charAt` | a byte is read with `s[0]` and is an `i64` | replace `s.charAt(0)` with `s[0]` |
| 85 | Rust/Python | `s[0..2] / s[0:2]` | E0106: expected a field name but found `.` | a part of a string is `s.slice(0, 2)` | replace `s[0..2]` with `s.slice(0, 2)` |
| 86 | Python/Go | `s[0:2]` | E0106: expected `]` but found `:` | a part of a string is `s.slice(0, 2)` | replace `s[0:2]` with `s.slice(0, 2)` |
| 87 | Rust/JS | `Point { x, y }` | E0106: expected `:` but found `}` | there is no field shorthand; write `P { x: x }` | replace `x` with `x: x` |
| 88 | Rust | `struct P { x: i64, y: i64, } multi-line` | **accepted, no error** | accepted today. Keep | — |
| 89 | Swift/Kotlin | `P(x: 1)` | E0106: expected `)` but found `:` | a struct value is written with braces: `P { x: 1 }` | replace `(`…`)` with ` {`…`}` |
| 90 | Java/JS | `new P(1)` | E0107: expected end of line but found a name | there is no `new`; a struct value is `P { x: 1 }` | none: positional arguments have no field names |
| 91 | Java/Python/TS | `class P {` | E0106: expected `fn`, `struct`, `enum` or `const` but found a name | there are no classes; data is a `struct`, behaviour is functions | replace `class` with `struct` when the body holds only fields |
| 92 | Rust/Python | `let t = (1, 2)` | E0106: expected `)` but found `,` | there are no tuples; declare a struct | none |
| 93 | Rust/Go | `-> (i64, i64)` | E0106: expected a type but found `(` | there are no tuples; declare a struct for the result | none |
| 94 | Rust/JS | `let (a, b) = f()` | E0106: expected a variable name but found `(` | there is no destructuring; read the fields one by one | none |
| 95 | Rust | `x as i64` | E0106: expected `)` but found a name | there are no casts; the only number type is `i64` | delete ` as i64` |
| 96 | Rust/C | `let c: char = 'a'` | E0203: unknown type `char`; the built-in types so far are … | there is no `char`; `'a'` is the `i64` 97 and a one-byte string is `chr(97)` | replace `char` with `i64` |
| 97 | Python/JS | `'hello'` | E0104: a character literal holds one ASCII character, as in `'a'` | strings use double quotes; single quotes are for one byte | replace the quotes with `"` |
| 98 | C/Rust | `/* ... */` | E0106: expected an expression but found `/` | comments are `//` only | rewrite each line with `//` |
| 99 | Python | `# note` | E0101: unexpected character | comments start with `//` | replace `#` with `//` |
| 100 | Rust | `#[derive(Debug)]` | E0101: unexpected character | there are no attributes; nothing needs deriving | delete the line |
| 101 | Rust derive | `p == q on structs` | E0311: structs cannot be compared with `==` yet; compare their fields | already specific; after traits.md `==` on structs exists | none today |
| 102 | JS | `a === b` | E0106: expected an expression but found `=` | equality is `==` | replace `===` with `==` |
| 103 | Swift/TS | `o!` | E0106: expected `)` but found `!` | there is no force-unwrap; use `match` or `?` | none |
| 104 | Swift/Zig | `try f()` | E0107: expected end of line but found a name | the early-return form is a postfix `?`: `f()?` | replace `try f()` with `f()?` |
| 105 | Rust/Go | `panic!("x") / panic("x")` | E0202: unknown function `panic` | there is no `panic`; return a `Result`, or `eprint(...)` then `exit(1)` | replace `panic(m)` with `eprint(m)` + newline + `exit(1)` |
| 106 | all | `assert(x == 1)` | E0202: unknown function `assert` | there is no `assert` yet (see Verification below) | none today |
| 107 | Rust | `loop { }` | E0106: expected a field name but found `break` | there is no `loop`; write `while true {` | replace `loop` with `while true` |
| 108 | Python | `while True:` | E0303: a condition must be `bool` but this is `i64` | already specific (a condition must be `bool`) | replace `1` with `true` |
| 109 | JS/Rust | `const X = 1 inside fn` | E0111: a constant is declared outside functions; use `let` for a value inside one | already specific and correct | replace `const` with `let` |
| 110 | Rust/C | `static X: i64 = 1` | E0106: expected `fn`, `struct`, `enum` or `const` but found a name | there is no `static`; a program-wide value is `const X = 1` | replace `static X: i64 = 1` with `const X = 1` |
| 111 | Rust | `const X: i64 = 1` | **accepted, no error** | accepted today. Keep | — |
| 112 | C/TS | `-> void` | E0203: unknown type `void`; the built-in types so far are … | a function that returns nothing has no `->` part | delete `-> void` |
| 113 | Rust | `-> ()` | E0106: expected a type but found `(` | a function that returns nothing has no `->` part | delete `-> ()` |
| 114 | Rust/C | `fn main() -> i64` | **accepted, no error** | accepted today (the exit code). Keep | — |
| 115 | Rust/Python | `self.x` | **accepted, no error** | accepted today: `self` is an ordinary parameter name. Keep until traits.md decides methods | — |
| 116 | Rust | `HashMap<String, i64>` | E0203: unknown type `HashMap`; the built-in types so far are … | the map type is `Map[V]`; keys are always `str` | replace `HashMap[str, i64]` with `Map[i64]` |
| 117 | Go/Python/JS | `m["k"] = 1` | E0307: only a list or a string can be indexed but this is `Map[i64]` | a `Map` is read with `m.get(key)` and written with `m.set(key, value)` | replace `m["k"] = 1` with `m.set("k", 1)` |
| 118 | Python/JS | `{"a": 1}` | E0106: expected an expression but found `{` | there are no map literals; `new_map()` then `m.set(...)` | none |
| 119 | C | `~x` | E0101: unexpected character | there is no `~`; write `x ^ -1` | replace `~x` with `(x ^ -1)` |
| 120 | Python | `x ** 2` | E0106: expected an expression but found `*` | there is no `**`; write `x.pow(2)` | replace `x ** 2` with `x.pow(2)` |
| 121 | SQL/Pascal | `a <> b` | E0106: expected an expression but found `>` | inequality is `!=` | replace `<>` with `!=` |
| 122 | Rust | `trait Shape { }` | E0106: expected `fn`, `struct`, `enum` or `const` but found a name | there are no traits yet (traits.md) | none |
| 123 | Rust | `fn f[T: Ord](x: T)` | E0106: expected `]` but found `:` | type parameters have no bounds yet (traits.md); delete the bound | delete `: Ord` |
| 124 | Rust | `parse::<i64>()` | E0107: expected end of line but found `:` | type arguments are never written at a call; text to number is `"1".to_i64()` | none in general |
| 125 | TS/Go | `f[i64](x)` | E0210: only functions can be called | type arguments are never written at a call; delete `[i64]` | delete `[i64]` |
| 126 | Go/Swift | `defer f()` | E0107: expected end of line but found a name | there is no `defer`; values are freed automatically, run the call before each `return` | none |
| 127 | Go | `x, err := f()` | E0106: expected `=` but found `,` | one declaration per line; a function returns a `Result`, not a pair | none |
| 128 | JS/Java | `throw / try-catch` | E0106: expected `:` but found `(` | there are no exceptions; return a `Result` and `match` on it | none |
| 129 | Python/Rust | `string over two lines` | E0102: unterminated string literal | a string ends on its own line; write `\n` for a line break | none |
| 130 | Rust/Go | `r".." / `..`` | E0101: unexpected character | there are no raw strings; use `"..."` with escapes | none |
| 131 | all | `"\u{41}"` | E0103: unknown escape sequence in string literal | the escapes are `\n \t \r \0 \\ \"`; other bytes come from `chr(n)` | none today; `\x41` should be added to the language |
| 132 | C | `"\x41"` | E0103: unknown escape sequence in string literal | as above; decision: add `\xNN` to the language | — |
| 133 | Swift | `f(x: 1) named args` | E0106: expected `)` but found `:` | arguments are positional; there are no named arguments | delete `x: ` |
| 134 | Python/TS | `fn f(x: i64 = 1)` | E0106: expected `)` but found `=` | parameters have no default values; pass every argument | none |
| 135 | Rust | `fn f<'a>(s: &'a str)` | E0104: a character literal holds one ASCII character, as in `'a'` | there are no lifetimes; delete `'a` | delete the lifetime |
| 136 | Rust | `Box<Node>` | E0203: unknown type `Box`; the built-in types so far are … | there is no `Box`; a type that contains itself does so through `List[Node]` | none |
| 137 | all | `struct Node { next: Option[Node] }` | E0413: struct `Node` contains itself, so it would have no finite size | already specific; should add: use `List[Node]` with zero or one item | none |

## 2. The feedback loop: converge in one round

What exists today: `azol check [--json]` reports the first error of every
function and of every file's syntax, with a stable code, exact positions
(1-based, columns in characters, end exclusive), the rule, a help sentence
and, for nine repairs on seven codes (E0312, E0401, E0405, E0406, E0501,
E0502, E0506), a `fix` with the exact replacement. `azol explain <code>`
gives the long form. What is missing, with a decision for each:

| Gap | Decision | Reason |
|---|---|---|
| One error per function | Report **every independent error in a function**: after an error in a statement, the checker marks the names that statement would have declared as "unknown type, do not report", and goes on with the next statement. An error is suppressed only when it mentions a poisoned name. | Three mistakes in one function cost three rounds today. Poisoning, not counting, is what prevents cascades. |
| One syntax error per file | Keep one per file for now, but recover at the next top-level `fn`, `struct`, `enum`, `const` so that later declarations are still parsed and checked. | Statement-level recovery in a newline-terminated grammar produces false errors; declaration-level recovery does not. |
| Checking stops after syntax errors | After declaration-level recovery, check the declarations that did parse. | An agent that broke one function should still learn about the type error in another. |
| Span is one token for most errors | Every error carries the span of the whole offending expression or statement. Target: 100% of codes; a test asserts no span has zero width. | A fix needs an exact range; an agent applying an edit needs one too. |
| Fix coverage: 7 of 74 codes | Target: **every code is classified** in the table of codes as `fix: always`, `fix: when <condition>`, or `fix: never, because <reason>`, and `azol explain --json` exposes that field. Planned classification is in section 2.1. | "Which errors can be repaired mechanically" must be a published property, not something an agent discovers. |
| No command applies fixes | `azol fix <files...>`: applies every fix, re-checks, repeats until nothing changes or 8 passes; prints what it changed and what is left; exits 0 when the program is clean, 1 otherwise. `azol fix --check` changes nothing. `--json` lists each applied edit. Only fixes marked *safe* are applied: a safe fix never changes the meaning of code that already compiled, and the fixed program must pass the check that the original failed. | One command is one tool call. Agents apply JSON edits unreliably when offsets shift after the first edit; the compiler does not. |
| No schema version | Every JSON line gains `"schema":1`. The schema is a file, `docs/schema/diagnostic.json` (JSON Schema), and a test validates real output against it. A field is never removed or retyped within a schema number. | Agents and harnesses are written once and run for years. |
| Summary line shape | Keep `{"summary":{"errors":N,"files":M}}` as the last line, add `"schema":1,"fixable":K`. | The agent learns in one line whether `azol fix` is worth calling. |
| Exit codes | 0 success; 1 the program has errors (or `fmt --check`/`fix --check` found work); 2 the command line is wrong; 3 the compiler itself failed (a bug) and says so on standard error. Tests fix these. | Today 0, 1, 2 exist; a compiler bug must not look like a user error. |
| `--json` only on `check` and `build` | `--json` on every command: `fmt` (files changed), `fix`, `test`, `explain`, `guide --list`, `example --list`, and the package commands. One object per line, last line a summary. | No scraping of human text anywhere. |
| Order | Errors sorted by file (command-line order), then line, then column, then code. Exists today for `check`; becomes a tested rule for all commands. | Two runs give identical bytes, so a harness can diff. |
| Colour, progress, width | Never colour; never progress output; never wrap or truncate by terminal width; the source excerpt line is the full source line. Exists today; becomes a tested rule (run under `TERM=dumb` and `COLUMNS=20`, output identical). | Bytes must not depend on where the tool runs. |
| Help and fix invisible without `--json` | Text form prints `help:` and `fix:` lines (section 1.2). | See 1.2. |
| Errors in generic instances | Keep the existing `(in f with T = X)` suffix and add the call site that caused the instance as a second location in JSON: `"related":[{"file","line","column","note"}]`. | The fix is almost always at the call. When traits.md lands this case mostly disappears. |
| Run-time stops | A trap prints `azol: <what>` and exits 1 today, with no location. Decision: print `  at <file>:<line> in <function>` for the trap and each caller (legacy.md, tooling P0), and with `AZOL_JSON=1` in the environment print one JSON line instead. | An agent debugging a port needs the line, not a guess. |

### 2.1 Fix classification of the codes

Carry a fix today: E0312 (`None()`), E0401 (`let` to `var`), E0405 (add
`mut`), E0406 (remove `mut`), E0501 (add the missing arms), E0502 (add
`_ =>`), E0506 (`(_)` on a variant pattern).

Decision for the rest:

- `fix: always` to add: E0101 for `;` at the end of a line, `#` comments,
  `~`; E0103 never (the intended byte is unknown); E0107 for a second
  statement on a line (insert a line break); E0108 (insert the newline);
  E0111 (`const` to `let`); E0112 (wrap the struct literal in parentheses);
  E0113 (drop leading zeros and doubled `_`); E0305 when the function's last
  statement is an expression of the result type (insert `return `); E0303
  when the condition is `i64` (append ` != 0`); E0308 and E0302 when a
  `str` and an `i64` meet at `+` (wrap in `str(...)`); E0309 for `print`
  with several arguments (join with `+`); E0310 never; E0313 for a missing
  field never, for a field given twice (delete the second); E0403 (copy into
  a `var` first) when the binding is assigned exactly once; E0408 never
  (needs a `while` rewrite: say so); E0409 (insert `let current = x` and
  match on it); E0410 (copy the second use into a `let` before the call);
  E0411 (hoist the first read into a `let`); E0412 (`s[i] = b` to
  `s.set(i, b)`); E0414 (hoist the `?` expression into a `let`); E0503
  (delete the unreachable arm); E0601 for an empty list or bare `None`
  never (the type is the author's choice, but the message already shows the
  form); E0703 (`const` cannot change: never).
- `fix: when a close name exists` (edit distance 1 or 2, exactly one
  candidate): E0201, E0202, E0203, E0204, E0205, E0206, E0207. Plus the two
  fixed tables of foreign names from section 1.5.
- `fix: never`: the E03xx type mismatches not listed above, E0208–E0212,
  E0315–E0318, E0413, E0504–E0508 except E0506, E0602, E0603, E07xx, E08xx.
  Each gets a help sentence that states the decision the author must make.

Target after this work: a fix on 35 or more of the 74 existing codes, and on
at least 70% of the habit rows. The published number is whatever the fixture
suite measures: `fixable errors / all errors` over the 137 habit programs
and the wrong-program sets already used by `compiler/check_sema.py`.

## 3. One way to write each thing

Exists today and is the language's main advantage for agents; stated here so
that the five new features keep it: no optional syntax (no semicolons at
all, no optional parentheses that change meaning, no optional `return`), one
loop over a collection, one conditional, one number type, one text type, one
list type, `azol fmt` with no options. Rule for every design in
docs/design/: a feature that adds a second way to write something that can
already be written is rejected, or the old way is removed in the same
change with a fix in the compiler for the old form.

Three places where today's compiler accepts two forms; decisions:

- Commas between `match` arms and struct members are accepted, `fmt` removes
  them. Keep (it removes a whole class of first-try failures for Rust-shaped
  output at no cost in meaning).
- Parentheses around a condition are accepted. Keep; `fmt` should remove
  them when the whole condition is parenthesised and holds no struct literal.
- `const X: i64 = 1` and `const X = 1`. Keep both; `fmt` leaves them.

## 4. Local reasoning: where today's azoL needs knowledge that is not on the line

Found by reading docs/language.md and the briefing against the rule "what a
line means is decidable from the line and the declarations it names".

| # | Place | Why it is not local | Decision |
|---|---|---|---|
| L1 | Variant names are global and bare (`Circle(2)`, `Red`) | A reader cannot tell a variant from a function call or a constant without knowing every enum of the program; two enums cannot share a variant name, so adding an enum in one file can break another file. With packages this stops scaling. | **Change.** Keep bare names inside the file that declares the enum and wherever the expected type is known (argument, typed variable, `match` on a value of that enum, `return`). Add the qualified form `Shape.Circle(2)` and require it only when the bare name is ambiguous. Uniqueness is then per enum, not per program. E0210's message for `Shape.Circle` disappears because the form becomes valid. Coordinate with packages.md, which owns namespaces. |
| L2 | Library methods are functions named `type_method` in the prelude, chosen by receiver type | Locally fine (the receiver's type decides), but a user cannot do the same for their own types (`p.area()` is an error) and nothing on the line says a method is library code. | **Change, in traits.md.** One rule for everyone: a function whose first parameter is named `self` is callable as a method on that type. Until then keep, and make E0207 name the function when one exists ("`area` is a function; write `area(p)`"). |
| L3 | Last-use moves and skipped copies | Whether `let b = a` copies depends on whether `a` is used later. Meaning is unaffected (the program behaves as if every assignment copied), only cost. | **Keep, and surface.** `azol explain-line file:line` prints, for each expression on the line: copied, moved (last use), appended in place, or dropped here. JSON form for tools. Reason: meaning is local already; cost should be inspectable, not annotated. |
| L4 | Implicit deep copies on assignment and on passing to `List.push` | An innocent line can copy a megabyte. | **Keep (it is the safety model), surface with the same tool,** and add an optional report `azol check --copies` that lists every copy of a value that owns memory inside a loop. No warnings in normal output: the compiler reports errors only. |
| L5 | Generic functions are checked per use | An error inside a generic body depends on a call somewhere else. | **Change, in traits.md:** a generic function states what it needs, so its body is checked once and a bad call is reported at the call. Until then add the related location (section 2). |
| L6 | An empty list literal `[]`, a bare `None`, `new_map()` take their type from context | The line alone does not say the type when the context is far (a `return` in a long function). | **Keep.** The context is always a declaration named on the line (the variable's type, the parameter, the result type of the enclosing function), which is inside the rule. |
| L7 | All files of a build share one namespace | A name on a line may be declared in any file given on the command line. | **Change, in packages.md** (`import`, public names). |
| L8 | `mut` receivers: `xs.push(x)`, `m.set(k, v)`, `s.set(i, b)` change the receiver without `mut` at the call, while `f(mut xs)` requires it | Two conventions for "this call changes that variable". | **Keep the method form without `mut`,** because the receiver position is itself the mark, and state the rule in one sentence in the briefing: "a call changes a variable only if the variable is the receiver of a method or is written `mut x`". `explain-line` lists the variables a line can change. |
| L9 | Operator precedence differs from C for `&`, `|`, `^` against comparisons, and shifts bind looser than `+` | An agent's C habit gives the right answer for `a & MASK == 0` only in azoL; `1 << n + 1` surprises both ways. | **Keep,** and make `azol fmt` add parentheses to any expression that mixes a shift or a bit operator with arithmetic or a comparison. Reason: the formatter can make precedence visible at no cost to anyone. |
| L10 | Integer overflow wraps silently; division truncates | Not non-local, but silent. A port from Python or Java `BigInteger`/checked contexts changes behaviour without an error. | **Keep wrapping as the meaning of `+ - *`;** add library methods `checked_add`, `checked_sub`, `checked_mul` returning `Option[i64]` (legacy.md P0, small). |

`azol explain-line <file>:<line> [--json]`, specified: for the given line,
print each statement on it with (a) the type of every sub-expression that is
not a literal, (b) for every value that owns memory: copied / moved / built
in place / appended in place, (c) what is dropped at the end of the
statement, (d) the variables the line can change, (e) for a call, the
resolved function with its file and line, and the type arguments of a
generic call. It reads the checker's existing tables (types, move flags,
call targets); it adds no analysis. Golden tests fix its output.

## 5. Verification an agent can run

An agent replacing a system must show that the new code does what the old
did. Decisions on where each piece lives:

| Piece | Where | Form | Reason |
|---|---|---|---|
| `assert` | Language (a statement) | `assert condition` and `assert condition, "message"`. On failure prints `azol: assertion failed: <source text of the condition>` plus the message and the location, exits 1. Never compiled out. | It needs the source text and the location, which only the compiler has. One form, always on: no debug/release difference in behaviour. |
| Tests | Language (a declaration) + tool | `test "adds two numbers" { ... }` at the top level of any file. Ignored by `azol build`. `azol test [files or project]` builds one executable per file's tests and runs every test in its own process. | Tests next to the code is the one layout; a process per test means a trap in one test cannot hide the others, and needs no unwinding in the language. |
| Test results | Tool | `azol test --json`: one line per test `{"schema":1,"test","file","line","result":"pass"\|"fail"\|"stopped","message","duration_ns"}`, then a summary. Exit 0 only if all pass. Order: file, then line. | Machine-readable, deterministic. |
| Comparing values in tests | Library, after traits.md | `assert a == b` works on any comparable type once `==` exists for structs and lists; on failure both values are printed with the library's text form. | Depends on traits (built-in equality and text form). Until then compare fields. |
| Golden tests (recorded input and output) | Tool | `azol test --golden <folder>`: every `name.in` is fed to the program's `main` as standard input, `name.args` as arguments; standard output, standard error and the exit code are compared with `name.out`, `name.err`, `name.code`. `--update` rewrites them. `--json` as above. | This is the porting workflow: record the old system's behaviour as files once, then drive the new one against them. It needs standard input in the language (legacy.md P0). |
| Contracts | Language (OPEN A3) | `requires condition` and `ensures condition` as the first statements of a function body; `ensures` may use the name `result`. Checked at run time, always. Failure message names the function and the contract text. | They let an agent write down what it learnt about the old system's function where the next agent will see it, and they turn a wrong port into a located failure instead of a wrong number downstream. Always-on keeps one behaviour. |
| Property and fuzz helpers | Library, after closures.md | `check_property(runs, seed, fn(rng) -> bool)` and generators on an explicit `Rng`. Failing seed printed, so the failure is reproducible with one number. | Needs function values. Seeded, never ambient. |
| Determinism | Language rule + library shape | A program's output is a function of its arguments, its standard input, the files it reads and, if it asks, the clock. No ambient randomness: `Rng` is a value created from a seed and passed explicitly (OPEN A2). `Map.keys()` order is already specified and deterministic (insertion order; a `remove` moves the last key into the gap). Thread results (threads.md) are joined in a specified order. Same source and compiler give the same executable bytes (exists today and is tested by the fixed point). | Golden tests are only meaningful when reruns agree. |
| Coverage | Tool, later (P2) | `azol test --coverage --json`: lines executed. | Useful to show a port's tests reach the code; not needed for the claim. |

## 6. "Agent compatible", defined and measured

### 6.1 The benchmark

- **Tasks:** the 40 below, fixed. Each has a statement in plain English that
  names no language, and a hidden set of golden tests (arguments, standard
  input, files in; standard output, exit code, files out). Tasks that need a
  feature azoL lacks are still in the list from day one and count as
  failures until the feature lands; the suite does not change to flatter
  the language.
- **Subjects:** at least three current models from at least two vendors,
  each with a fresh context per task. The azoL arm gets the output of
  `azol guide`, the tools `azol check --json`, `azol fix`, `azol build`,
  `azol guide <topic>`, `azol example <task>`, and nothing else about the
  language. The Rust control arm gets `rustc`/`cargo check
  --message-format=json`, no documentation (the model knows Rust), the
  standard library only (no crates), same tasks, same tests.
- **Protocol:** the model writes a file; the harness records every compiler
  invocation and its result; at most 10 compiler rounds per task; 5 runs per
  task per model with different seeds; the harness, not the model, runs the
  tests.
- **Metrics:**
  1. first-try build rate: share of runs whose first compiler invocation
     reports no error;
  2. first-try correct rate: share of runs whose first submitted program
     passes every hidden test;
  3. rounds to green: median and 90th percentile of compiler invocations
     until the tests pass (failures count as 11);
  4. solved rate within 10 rounds;
  5. context cost: tokens of documentation the model was given or fetched
     (the Rust arm is 0 by construction; this metric is reported, and is the
     one azoL cannot win; it must stay under 12,000 per task);
  6. output cost: tokens the model wrote in total until green;
  7. wall time until green, including builds (this is where the compile
     speed shows).
- **Release rule:** the public sentence "agents write azoL more reliably
  than Rust" may be made only when, for every model in the run, azoL is at
  least as good as the Rust control on metrics 1 to 4, 6 and 7, and strictly
  better on at least three of them, with the raw logs published in
  `bench/agents/`. Metric 5 is published beside it. If one model fails the
  rule, the claim names the models it holds for.
- **What can be claimed, honestly, in words:** not "100% compatible". The
  form is: "On 40 tasks, <model> built azoL correctly on the first try in X%
  of runs (Rust: Y%) and reached passing tests in a median of K rounds
  (Rust: K')". The aim for the first public number: metric 1 at or above
  90%, metric 4 at 100% on the tasks whose features exist.
- **Regression use:** the same harness runs on every change to the briefing
  or to an error message. A change that lowers metric 1 or 3 does not land.
  This is the only honest way to tune `azol guide`.

### 6.2 The 40 tasks

Text and parsing

1. Count lines, words and bytes of a file given as an argument.
2. Word frequency of a file, sorted by count then word.
3. Parse `key=value` configuration lines, report bad lines with line numbers, print the values in key order.
4. Split CSV lines with quoted fields containing commas and doubled quotes.
5. Evaluate integer arithmetic expressions with `+ - * /`, parentheses and precedence; report division by zero.
6. Tokenise a small language (names, numbers, strings, operators) and print the tokens.
7. Wrap text to a given width without breaking words.
8. Convert Roman numerals to numbers and back, rejecting malformed input.
9. Parse and validate ISO dates (`YYYY-MM-DD`), print the day of the week.
10. Render a table of records as aligned columns.

Data structures

11. A generic stack and queue with their operations and empty cases.
12. A binary search tree of numbers: insert, contains, in-order listing, height.
13. A priority queue (binary heap) used to merge k sorted lists.
14. A sparse matrix as a map from position to value: add, multiply two.
15. An LRU cache with a fixed capacity.
16. A graph from an edge list: breadth-first shortest path lengths.
17. Topological sort with cycle detection and the cycle printed.
18. Union-find to count connected components.
19. A trie: insert words, list all words with a prefix.
20. Interval merging and total covered length.

Files and bytes

21. Hex dump of a file in the classic 16-bytes-per-line form.
22. Find duplicate files in a list of paths by content hash.
23. Read a little-endian binary record file (fixed layout), print the records, write them back sorted.
24. Apply a line-based patch (insert, delete, replace by line number) to a file.
25. Tail: print the last n lines of a file given `-n`.

State machines and simulation

26. A vending machine driven by a script of events, with every invalid transition reported.
27. A traffic-light controller stepping through timed states.
28. Conway's Life for n generations on a fixed grid.
29. A bank ledger: deposits, withdrawals, transfers, rejected operations, final balances.
30. A stack virtual machine executing a program of a dozen instructions with error states.

Error handling

31. A pipeline of three fallible steps where each error must be reported with its step and cause.
32. Validate a batch of records against five rules, collecting every violation rather than stopping at the first.
33. Retry a failing operation (simulated by a script) with a bound, reporting attempts.
34. A command-line tool with subcommands, flags, usage text and exit codes 0, 1, 2.

Numbers

35. Prime sieve and factorisation up to a bound.
36. Arbitrary-precision addition and multiplication of decimal strings.
37. Statistics of a column of decimal numbers: mean, median, standard deviation to two places (needs floats).
38. A fixed-point money calculation with rounding rules stated in the task.

Concurrency and system (count as failures until the features exist)

39. Sum the sizes of many files in parallel with a bounded number of workers; the result must equal the sequential one (needs threads).
40. A line-based TCP echo server and a client that talks to it (needs sockets, legacy.md).

### 6.3 Where the harness lives

`bench/agents/tasks/NN-name/` (statement, hidden tests), `bench/agents/run.py`
(the protocol; model access is whatever the operator has: the harness only
needs a command that turns a prompt into a completion, so it is not tied to
one vendor or to an API), `bench/agents/results/<date>-<model>.json` (raw
logs). The suite's own reference solutions in azoL are compiled and tested
in the normal test run, which also proves every non-failing task is solvable.

## 7. Order of work

1. Habit recognisers and fixes (section 1.5) and `azol fix` (R4, R6):
   independent of the five features; touches only error paths. First,
   because the measured gap is largest here (0 fixes on 137 habits).
2. `azol guide`, `azol example`, text-form `help:`/`fix:` lines, schema
   version, exit code 3 (R1, R3, R7).
3. All independent errors per function; full spans (R5).
4. `assert`, `test`, `azol test --json`, standard input, golden tests (R9).
5. `azol explain-line` and the `fmt` precedence parentheses (R8).
6. The benchmark harness and its first published run (R10), then again after
   each of the five features.
7. L1, L2, L5, L7 land with traits.md and packages.md.

## 8. Doubts

- The 4-bytes-per-token ratio is a rule of thumb; the budget test should be
  re-based on a real tokenizer count once one model family is fixed for the
  benchmark.
- The fix list on seven codes is taken from the diagnostics work's report
  and one probe (`let` to `var`); it was not re-derived from the source for
  this document.
- Declaration-level syntax recovery can still mis-synchronise inside a
  string that contains `fn `; the recogniser must work on tokens, not text.
- The Rust control without crates disadvantages Rust on tasks 4, 22 and 40
  less than it seems (the standard library covers them), but a reviewer may
  ask for a second control arm with crates allowed. Worth running both.
