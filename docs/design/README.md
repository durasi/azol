# Designs for the five missing features

Specifications for floating-point numbers, methods and traits, function
values, threads, and packages. Nothing here is implemented. Each document
states its decisions; this page lists what is left for the user to decide,
the order of work, and the names and numbers each feature reserves so that
five implementations do not collide.

| Document | Feature | Error codes |
|---|---|---|
| [floats.md](floats.md) | `f64` | E0901–E0915 |
| [traits.md](traits.md) | methods on your own types, `Equal`/`Ordered`/`Hashable`, traits and bounds, `Map[K, V]`, `Set[T]` | E1001–E1030 |
| [closures.md](closures.md) | function types, function literals with captures by value, `map`/`filter`/... | E1101–E1125 |
| [threads.md](threads.md) | `run_all`, `parallel_map`, `spawn`/`join` with `Thread[T]`, `Channel[T]`, `cores` | E1201–E1230 |
| [packages.md](packages.md) | `azol.toml`, `import`, `pub`, `deps/`, `azol new/add/test/...`, `test` blocks, `assert`, `run` | E1301–E1345 |
| [agents.md](agents.md) | `azol guide`, `azol fix`, an error of its own for each habit of another language (done: see "What exists" in that file) | E0120–E0133, E0220–E0224 |

## Decided by the user (12 November 2026)

- **Scope before the public announcement:** everything: the five features,
  the speed work, and the whole P0 list of [legacy.md](legacy.md) (calling C,
  the other integer types, processes, time, JSON and the rest).
- **OPEN-2:** yes. `Map[V]` becomes `Map[K, V]`, and `Set[T]` is added.
- **OPEN-3:** yes. One long form for function literals; the short forms are
  errors that carry the long form as their fix.
- **OPEN-4:** no. Threads get `spawn` with a handle, `join`, and channels
  that move values, in the first version, next to `run_all` and
  `parallel_map`. [threads.md](threads.md) has been extended accordingly
  (its two remaining questions are OPEN T1 and T2 below).
- **Speed:** parity with `rustc -O` is required, not optional. The default
  build stays single pass; `azol build -O` gets our own optimiser written in
  azoL (an intermediate form and passes), with no outside dependency. See
  [speed.md](speed.md); the optimiser's own design is `optimizer.md`.
- **Optimiser, OPEN O1–O5 of [optimizer.md](optimizer.md):** all as recommended.
  `-O` stops with the same message as the default build; x86-64 vector
  loops get SSE2 and AVX2 versions chosen at run time; ordinary structs are
  packed in both modes; the public definition of "as fast as `rustc -O`" is
  a geometric mean of at most 1.00 over the 31-program suite with no
  program above 1.10, on the Apple M5 and on x86-64; the seeds are built
  with `-O` once its bootstrap fixed point holds.
- **Shorter code, OPEN C1–C6 of [concise.md](concise.md):** all as
  recommended. Interpolation `"total: {sum}"` with `\{` for a literal
  brace; `if`/`match` may give a value in the restricted positions; a short
  `if` stays on one line in `azol fmt`; ranges in `for`; compound
  assignment; `if let`, `let ... else`, implicit return, tuples, default
  fields and `unwrap` stay out and are errors with fixes.
- **No outside packages:** the language does its own work. JSON, CSV,
  dates, hashing and the like are written in azoL in the standard library;
  only the operating system's own libraries are reached through `extern`.
- **Bootstrap:** prebuilt compilers for the three targets live in the
  repository; the Rust sources and Cargo are deleted.
- **Licence:** MIT or Apache-2.0.

The other OPEN items below and in [agents.md](agents.md) and
[legacy.md](legacy.md) stand as recommended until the user says otherwise.

## OPEN: decisions for the user

Each has a recommendation, and the specifications are written as if the
recommendation is taken.

- **OPEN-1 (floats.md F11).** Make `-5.abs()` and `-7.5.floor()` an error
  that asks for parentheses (`E0912`), because today they silently mean
  `-(5.abs())`. This rejects programs that compile now. Recommendation:
  yes.
- **OPEN-2 (traits.md 3.5).** Change `Map[V]` (text keys only) to
  `Map[K, V]` with any `Hashable` key, and add `Set[T]`. Existing
  `Map[i64]` must be rewritten `Map[str, i64]` (the compiler offers the
  fix). Recommendation: yes, now, while few programs exist.
- **OPEN-3 (closures.md 2, 11).** No short form for function literals:
  `xs.map(fn(x: i64) -> i64 { return x * 2 })` is the only form, and
  `|x| x * 2` is an error that carries the long form as its fix.
  Recommendation: yes; revisit only if real use shows agents failing
  after the fix.
- **OPEN-4 (threads.md 2).** Threads are structured only (`run_all`,
  `parallel_map`): no `spawn` with a handle, no channels, no locks.
  Channels would be the next step if long-lived workers are needed.
  Recommendation: ship the structured form alone; decide on channels when
  sockets or timers exist.
- **OPEN T1 (threads.md 4.7).** No `select` over several channels; a
  thread that waits for several kinds of message receives them through
  one channel of an enum. Recommendation: leave `select` out.
- **OPEN T2 (threads.md 4.5).** A thread cannot be detached: every
  `Thread` must be joined, and forgetting to is a compile error that
  carries the fix. Servers use a fixed pool of workers fed by a channel.
  Recommendation: keep it.
- **OPEN-5 (packages.md 4.3).** Dependencies are copied into `deps/` and
  that folder is meant to be committed, so that a clone builds offline.
  The alternative is to ignore `deps/` in git and run `azol fetch` after
  cloning. Recommendation: commit it (both ways work with the same
  commands).
- **OPEN-6 (traits.md 3.2).** `==`, ordering, hashing and `str(x)` are
  worked out by the compiler from a type's fields and cannot be
  customised by a program. Field order then decides sort order.
  Recommendation: yes; it is what makes `a == b` readable without context.
- **OPEN-7 (traits.md 3.1).** No functions without `self` inside `impl`
  (no `Account.new()`); constructors are plain functions
  (`new_account()`). Recommendation: yes, one way to call a function.
- **OPEN-8 (packages.md 4).** The manifest is a strict subset of TOML
  named `azol.toml` rather than a file in azoL syntax. Recommendation:
  TOML subset; it is what models and editors expect.

## Order of implementation

Dependencies between the features:

- Floats depend on nothing. One use of a bound (`sorted` on `List[f64]`)
  waits for traits step A5 and works through the present mechanism until
  then.
- Traits depend on nothing.
- Closures depend on nothing for the language part; `sorted_by_key` needs
  bounds (traits A5).
- Threads depend on closures (steps 1-5 of closures.md) entirely for the
  language side. `receive_within` needs the monotonic clock (`clock()`).
  The handle rules for `Thread[T]` are the same analysis as the foreign
  `handle` types of legacy.md and should be built once.
- Packages depend on nothing for `assert`, `test`, the manifest, `run`;
  the coherence rule of traits refers to packages and is trivially true
  until packages exist (one package).

Who touches what (compiler written in azoL; file names as in
`compiler/`):

| Part | Floats | Traits | Closures | Threads | Packages |
|---|---|---|---|---|---|
| lexer, token | literal rule | 4 words | none | none | 3 words |
| parser, ast, dump, format | none | `impl`, `trait`, bounds | `fn` type, `fn` literal | none | `import`, `pub`, `test`, `a.B` in types and patterns |
| checker: types table | `f64` | `Param`, capabilities | `Fn` signatures | `Thread[T]`, `Channel[T]` | none |
| checker: operators | arithmetic, comparison | `==`, `<` on all types | none | none | none |
| checker: name and method lookup | none | **method lookup** | captures | channel methods (built in) | **package of every name** |
| checker: flow | none | none | none | **handle states, must-join (Th5-Th9)** | none |
| checker: generics | none | **bounds, opaque check** | function-typed parameters | none | none |
| code generators | float ops | equality/compare/hash/str glue | indirect call, environment, glue | `rt_parallel` call stub, `spawn`/`join`/channel calls, channel copy and drop glue | `assert`, `run` calls |
| runtimes | none | none | none | **threads, allocator, channels, deadlock stop** | `run`, assertion message |
| prelude | math, text | bounds, `Map[K,V]`, `Set` | `map`, `filter`, ... | `run_all`, `parallel_map` | none |
| parser | none | (above) | (above) | habit forms `go`, `chan`, `<-`, `select`, `async`, `await` | (above) |
| driver, command line | none | none | none | none | **everything** |

Bold marks the heavy, conflict-prone work. The plan in waves; inside a
wave the agents work at the same time:

**Wave 1** (four agents, disjoint parts)
- F-a: floats steps 1-4 (lexer rule, text-to-bits, type, code generators).
- T-a: traits A1-A2 (methods on own types).
- P-a: packages steps 1-3 and 6 (`assert`, `test` parsing, manifest,
  `azol new`, project `build`/`run`, the `run` built-in). Driver and
  runtimes; the checker only for `assert` and `test`.
- R-a: threads steps 2-4 and 6 (runtime thread primitives, Linux
  allocator, per-thread overflow handling, the channel runtime and the
  deadlock stop), tested from C drivers. No compiler changes. Large
  enough for two agents: one on threads and the allocator, one on
  channels.
- E-a: threads step 10, first half: cooperative, seeded threads in the
  two emulators (outside the repository's compiler; touches nothing else).

**Wave 2** (four agents)
- F-b: floats steps 5-7 (prelude only: printing, parsing, math).
- T-b: traits A3 (capabilities and structural `==`/`compare`/`hash`/`str`;
  operators and glue, both code generators).
- C-a: closures steps 1-4 and 7 (function types, values without captures,
  indirect calls, library functions).
- P-b: packages step 4 (packages in name lookup) — after T-a has merged,
  because both rewrite lookup.

**Wave 3** (three agents)
- T-c: traits A4-A5 (traits, bounds at uses, bounds in the prelude).
- C-b: closures steps 5-6 (captures, glue, habit errors).
- P-c: packages steps 5, 7-9 (path dependencies, `azol test`, lock,
  `deps/`, `add`/`fetch`).

**Wave 4** (three agents)
- T-d: traits B1-B2 (opaque check of generic bodies, `Map[K, V]`,
  `Set[T]`).
- H-a: threads steps 1, 5 and 7 (`cores`, `run_all`, `parallel_map`,
  `Thread[T]` with the handle rules, `spawn`, `join`, `join_all`), now
  that captures exist.
- H-b: threads steps 8 and 9 (`Channel[T]`, its methods, `for value in
  ch`, the habit errors). Disjoint from H-a except for the list of
  built-ins; H-a lands first.
- D-a: documents, formatter and grammar passes for all five, the fuzzer
  extensions, one test per error code.

After every wave: all comparison scripts and the bootstrap fixed point
must pass on the three targets before the next wave starts.

Rough size: floats 3,400 lines, traits 3,900, closures 2,700, threads
5,750 (of which 900 are emulator and fuzzer work), packages 3,700; about
19,450 lines, more than the compiler written in azoL today.

## Reserved names and numbers

So that parallel work cannot collide. Nothing outside a feature's row may
be claimed by it.

| | Floats | Traits | Closures | Threads | Packages |
|---|---|---|---|---|---|
| New keywords | none | `impl`, `trait`, `self`, `Self` | none | none | `import`, `pub`, `test` |
| New tokens | float literal (the token exists; rule tightened) | none (`:` and `+` in type parameters already exist as tokens) | none | none | none |
| Token kind numbers (after the last one in `token.azl` at the time work starts, call it N) | none | N+1 … N+4 | none | none | N+5 … N+7 |
| New type names | `f64` | `Equal`, `Ordered`, `Hashable`, `Set`; `Map` gets two parameters | `fn(...) -> ...` | `Thread`, `Channel` | none |
| Built-in functions | `f64_from_bits` | none | none | `cores`, `spawn`, `join_all`, `channel`, internal `rt_parallel` | `assert`, `run` |
| Built-in methods (compiler-made) | `bits`, `to_f64`, `to_i64` (on `f64`), `sqrt`, `abs`, `is_nan`, `is_finite` | `compare`, `hash` on every capable type | calling a value | `join` on `Thread[T]`; `send`, `try_send`, `receive`, `try_receive`, `receive_within`, `close`, `is_finished` on `Channel[T]` | none |
| Built-in numbers in the checker (`FIRST_BUILTIN` is `MAX - k`; with B the lowest number in use when work starts) | B-1 … B-12 | B-13 … B-16 | B-17 … B-20 | B-21 … B-24, and B-31 … B-42 (after the packages block) | B-25 … B-30 |
| Prelude names (functions and constants) | `PI`, `E`, `NAN`, `INFINITY`, `f64_*`, `str_to_f64`, `list_sum` for `f64` | `new_set`, `set_*`, `map_*` rewritten | `list_map`, `list_filter`, `list_fold`, `list_any`, `list_all`, `list_first_where`, `list_index_where`, `list_count_where`, `list_sorted_by`, `list_sorted_by_key`, `option_map`, `option_or` | `run_all`, `list_parallel_map` | `Finished` (struct) |
| Error codes | E0901–E0915 | E1001–E1030 | E1101–E1125 | E1201–E1230 | E1301–E1345 |
| Runtime imports/system calls | none | none | none | macOS: `pthread_create`, `pthread_join`, `pthread_attr_*`, `sysconf`, `pthread_mutex_init`/`lock`/`unlock`/`destroy`, `pthread_cond_init`/`wait`/`signal`/`broadcast`/`destroy`, `pthread_cond_timedwait_relative_np`; Linux: `clone`, `futex` (wait, wake, timed wait), `mmap`/`munmap` (in use), `sched_getaffinity`, `exit` | macOS: `posix_spawnp`, `pipe`, `waitpid`, `read`/`close` (in use); Linux: `clone` (vfork form), `execve`, `pipe2`, `wait4`, `dup3` |
| New files in `compiler/` | `float_text.azl` | `traits.azl` (capabilities, bounds) | `closures.azl` (captures, environments) | none | `manifest.azl`, `project.azl`, `fetch.azl` |
| Test folders | `tests/programs/float_*.azl`, `tests/float_vectors/` | `tests/programs/method_*.azl`, `trait_*.azl`, `equal_*.azl` | `tests/programs/fn_*.azl` | `tests/programs/thread_*.azl`, `tests/programs/channel_*.azl` | `tests/packages/` |

Names that existing programs may already use and that become reserved:
`impl`, `trait`, `self`, `Self`, `import`, `pub`, `test`, `f64`, `Equal`,
`Ordered`, `Hashable`, `Set`, `assert`, `run`, `cores`, `run_all`, `spawn`,
`join_all`, `channel`, `Thread`, `Channel`, `PI`,
`E`, `NAN`, `INFINITY`. The single-letter constant `E` is the likeliest to
collide with existing code (a type parameter named `E`, as in
`Result[T, E]`, is a different kind of name and is unaffected; a user
constant `E` would clash). If that is a concern, name it `EULER`: decide
when floats step 3 lands.

## Common rules for all five implementations

- Every error message says the rule in one sentence and, where the
  repair is mechanical, carries the fix; every code gets an entry in the
  table that `azol explain` prints, with one wrong and one right example.
- Every habit form from Rust, Swift, Go, TypeScript or Python that a
  specification names gets its own error rather than a generic syntax
  error.
- Every example in these documents becomes a test, verbatim, before the
  feature is called done; where an example and the implementation
  disagree, the specification is corrected by a commit that says why.
- The formatter, the VS Code grammar, `docs/language.md`,
  `docs/for-agents.md` and the fuzzer are part of each feature, not
  follow-ups.
- No feature may make a program that does not use it bigger or slower:
  hello stays 16,384 bytes on Linux x86-64, and the build of
  `bench/generated/big.azl` stays within 2% in instruction count.
