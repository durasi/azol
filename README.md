<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.svg">
    <img src="assets/logo.svg" alt="azoL" height="104">
  </picture>
</p>

<p align="center"><em>The rust remover.</em></p>

<p align="center">
  <strong>A memory-safe systems language with no garbage collector and no lifetime annotations,<br>
  whose compiler turns 44,000 lines into an executable in 10 milliseconds.</strong>
</p>

<p align="center">
  <a href="#numbers">Numbers</a> ·
  <a href="#azol-rust-and-javascript">Comparison</a> ·
  <a href="#quick-start">Quick start</a> ·
  <a href="docs/language.md">Language reference</a> ·
  <a href="AGENTS.md">For AI agents</a> ·
  <a href="docs/for-agents.md">Briefing</a>
</p>

---

azoL is for the kind of program you would write in Rust: no garbage collector, no dangling pointers, no double frees, nothing left unfreed. It gets there with values instead of references, so there is nothing to annotate, and with a compiler that reads your source and writes a ready-to-run executable by itself: no LLVM, no assembler, no linker, no `codesign`, no dependencies.

It is young and small. Read [what it does not have](#what-azol-does-not-have) before you plan anything on it.

## A taste

```
struct Player {
    name: str
    scores: List[i64]
}

fn total(p: Player) -> i64 {
    var sum = 0
    for score in p.scores {
        sum = sum + score
    }
    return sum
}

fn main() {
    var ada = Player { name: "Ada", scores: [12, 30] }
    ada.scores.push(25)

    var grace = ada
    grace.name = "Grace"
    grace.scores.push(40)

    for player in [ada, grace] {
        print(player.name + ": " + str(total(player)) + " points")
    }
}
```

```text
Ada: 67 points
Grace: 107 points
```

`grace` is a copy with her own list, so adding to it leaves `ada` alone. The strings and lists built along the way are freed the moment nothing owns them. Nothing in this program says how.

## Numbers

One machine, one run of [`bench/compare.py`](bench/compare.py), which writes this section itself (`python3 bench/compare.py --write-readme`). Each benchmark is the same algorithm written plainly in the three languages, and all three versions must print the same result.

<!-- bench:start -->
Measured on Apple M5, Darwin 27.2.0 (arm64). Best of 5 runs.
Tools: azol 0.1.0, rustc 1.99.1 (58cf8f9c3 2026-10-13), v22.17.0.

**Run time** (whole process, lower is better)

|  | azoL | Rust (`-O`) | Rust (unoptimized) | JavaScript (Node.js) |
|---|---|---|---|---|
| `fib40`: function calls (recursive `fib(40)`) | **0.24 s** | 0.14 s | 0.38 s | 0.46 s |
| `loops`: nested loops and arithmetic | **0.016 s** | 0.018 s | 0.064 s | 0.042 s |
| `sort`: quicksort of 2 million numbers | **0.13 s** | 0.11 s | 0.29 s | 0.27 s |
| `sieve`: prime sieve up to 20 million | **0.14 s** | 0.045 s | 0.20 s | 0.31 s |
| `particles`: list of structs updated in place | **0.23 s** | 0.040 s | 0.93 s | 0.17 s |
| `tree`: building and walking trees | **0.10 s** | 0.055 s | 0.11 s | 0.38 s |
| `append`: building long strings | **0.025 s** | 0.020 s | 0.064 s | 0.11 s |
| `words`: splitting and joining text | **0.040 s** | 0.026 s | 0.087 s | 0.11 s |

**Build time** of the same 44,004-line program

|  | Time |
|---|---|
| azoL, source to executable | **0.010 s** |
| Rust, type checking only (`--emit=metadata`) | 0.70 s |
| Rust, unoptimized build | 1.32 s |
| Rust, optimized build (`-O`) | 9.50 s |
| JavaScript, syntax check only (`node --check`) | 0.024 s |
| JavaScript, load and run (nothing to build) | 0.049 s |

**Executable size** of a small program (`fib40`)

|  | Size |
|---|---|
| azoL | **33 KB** |
| Rust (`-O`) | 460 KB |
| Rust (unoptimized) | 461 KB |
| JavaScript | needs Node.js installed |

Build: azoL is 70 times faster than Rust's type checking alone, 130 times faster than an unoptimized Rust build and 950 times faster than an optimized one.

Node.js checks the syntax of the JavaScript version in 2.4 times azoL's whole build time.

Run: azoL is faster than unoptimized Rust in 8 of 8 and faster than Node.js in 7 of 8. It is slower than optimized Rust in 7 of 8; the largest gaps are `particles` (5.8 times) and `sieve` (3.1 times).
<!-- bench:end -->

**Reading these fairly.**

- azoL has one build mode. There is no slow debug build and no optimizer to wait for; the code generator makes one pass and checks every index and every division.
- The build-time program is generated and repetitive ([`bench/gen.py`](bench/gen.py)), and azoL does less work than `rustc`: no traits, a small standard library, no optimizer passes.
- Run times are whole processes, so they include start-up, which is tens of milliseconds for Node.js.
- These are eight small programs, not a benchmark suite. Where azoL trails optimized Rust, the reasons are known: Rust inlines calls and removes index checks it can prove unnecessary; azoL does neither yet.

## azoL, Rust and JavaScript

| | <img src="assets/compare/azol.svg" alt="azoL" height="30"> | <img src="assets/compare/rust.svg" alt="Rust" height="30"> | <img src="assets/compare/javascript.svg" alt="JavaScript" height="30"> |
|---|---|---|---|
| **Memory safety** | Yes, checked when compiling | Yes, checked when compiling | Yes, by the runtime |
| **Garbage collector** | None | None | Yes |
| **Lifetime annotations** | Never | Wherever the compiler cannot infer them | Not applicable |
| **References in your code** | None. Values only; a function changes an argument only if the call says `mut` | `&T`, `&mut T`, smart pointers | Every object is a shared reference |
| **What stops two parts of a program changing the same data** | Four exclusivity rules, checked one function at a time | The borrow checker | Nothing |
| **Null** | None. `Option[T]` | None in safe code. `Option<T>` | `null` and `undefined` |
| **Errors** | `Result[T, E]`, `?`, exhaustive `match` | `Result<T, E>`, `?`, exhaustive `match`, panics | Exceptions |
| **Types** | Static, inferred inside functions | Static, inferred inside functions | Dynamic |
| **Generics** | Functions, structs and enums, checked for each set of types used. No traits | With traits and bounds | Not needed |
| **Numbers** | `i64` only. No floating point | Integers of every size, floats | 64-bit floats, `BigInt` |
| **Closures, functions as values** | No | Yes | Yes |
| **Threads** | No | Yes, with data races ruled out when compiling | Workers and an event loop |
| **Modules and packages** | Several files, one namespace. No package manager | Modules, crates, Cargo, crates.io | Modules, npm |
| **From source to running** | Compiled ahead of time, in milliseconds | Compiled ahead of time, in seconds to minutes | Compiled while it runs |
| **What you ship** | One small executable | One executable | Source, and the user needs a runtime |
| **Needed to build** | `azol` alone | `rustc`, LLVM, and the system linker | Nothing; Node.js or a browser to run |
| **Building for another system** | Built in: `--target`, nothing to install | A target to install and a linker for it | Not applicable |
| **Compiler output for tools and AI agents** | `check --json`: code, exact span, the rule that was broken, and the edit when the repair is mechanical | `--error-format=json`, with suggested edits marked by how safe they are to apply | No compiler; errors appear when the code runs, linters are separate tools |
| **Formatter** | `azol fmt`, one layout, no options | `rustfmt`, ships with the toolchain | Separate tools |
| **Written in** | azoL: the compiler compiles itself. The first compiler was written in Rust | Rust | Engines are written in C++ |
| **Targets** | Apple Silicon macOS, x86-64 Linux, ARM64 Linux | Dozens | Wherever an engine runs |
| **Maturity** | Experimental. Do not depend on it | Stable since 2015 | In use since 1995 |

Rust and JavaScript can do far more than azoL can today. What azoL offers is the compile-time memory safety of the first with a full build that takes less time than Node.js needs to check the same program's syntax, in a language whose whole reference is one document.

## Quick start

You need a shell and nothing else: no Rust, no C compiler, no linker, no package manager. It works on an Apple Silicon Mac and on x86-64 Linux; ARM64 Linux is built the same way but has only been run in an emulator.

```sh
git clone https://github.com/durasi/azol
cd azol
sh build.sh
./azol build examples/players.azl
./examples/players
```

`build.sh` takes the small compiler for your machine that is kept in [`bootstrap/`](bootstrap/), lets it build the compiler's azoL sources in [`compiler/`](compiler/), lets the result build them twice more, and checks that the last two are the same file. That takes well under a second and leaves `./azol`.

A real command-line tool, written in azoL:

```sh
./azol build examples/wc.azl
./examples/wc README.md lib/prelude.azl
```

More to run: `examples/generics.azl`, `examples/bank.azl`, `examples/tree.azl`, `examples/lists.azl`, `examples/structs.azl`, `examples/fib.azl`. To run the tests: `sh test.sh` (needs Python 3). To repeat the measurements: `python3 bench/compare.py` (it uses `rustc` and `node` if they are installed).

## The language in five minutes

The whole language is in [docs/language.md](docs/language.md), with every example checked against the compiler. This is the short version.

**No null, no exceptions.** A value that may be missing is an `Option`, something that may fail returns a `Result`, `?` passes a failure up, and `match` makes you deal with every case.

```
struct Account {
    owner: str
    balance: i64
}

enum Failure {
    NoSuchAccount(str)
    NotEnough(i64)
}

fn find(accounts: List[Account], owner: str) -> Result[i64, Failure] {
    for i, account in accounts {
        if account.owner == owner {
            return Ok(i)
        }
    }
    return Err(NoSuchAccount(owner))
}

fn withdraw(accounts: mut List[Account], owner: str, amount: i64) -> Result[i64, Failure] {
    let i = find(accounts, owner)?
    let balance = accounts[i].balance
    if balance < amount {
        return Err(NotEnough(amount - balance))
    }
    accounts[i].balance = balance - amount
    return Ok(balance - amount)
}
```

`withdraw(mut accounts, "Ada", 70)` returns `Ok(30)` the first time and `Err(NotEnough(40))` the second. Leave an arm out of a `match` and the program does not compile:

```text
error[E0501]: this `match` does not handle `None`; add an arm for it or a final `_ =>` arm
```

**Generics without ceremony.** Code that works for any type is written once. The compiler works out the types at each call and builds a version for each.

```
struct Stack[T] {
    items: List[T]
}

fn pop[T](stack: mut Stack[T]) -> Option[T] {
    if stack.items.len() == 0 {
        return None
    }
    return Some(stack.items.pop())
}
```

**A standard library written in azoL.** Text (`split`, `trim`, `replace`, `find`, `to_i64`, ...), lists (`sorted`, `reversed`, `contains`, `extend`, ...), numbers (`abs`, `pow`, `clamp`, `to_hex`, ...) and a map with text keys live in [`lib/prelude.azl`](lib/prelude.azl). Only what a program uses is compiled into it.

```
fn main() {
    var seen: Map[i64] = new_map()
    for word in "the cat and the hat and the bat".split(" ") {
        match seen.get(word) {
            Some(count) => seen.set(word, count + 1)
            None => seen.set(word, 1)
        }
    }
    for word in ["the", "and", "dog"] {
        match seen.get(word) {
            Some(count) => print(word + ": " + str(count))
            None => print(word + ": never")
        }
    }
}
```

```text
the: 3
and: 2
dog: never
```

**Also there:** structs inside structs, enums whose variants carry values, `List[T]` of anything, `const`, bit operators, hex, binary and character literals, `for` with an index, programs spread over several files, `read_file` / `write_file` returning a `Result`, `args()` and `exit()`.

### What azoL does not have

Floating-point numbers, integer types other than `i64`, closures, traits, methods on your own types, threads, modules or a package manager, patterns inside patterns, `if` as an expression. Integer overflow wraps. [The reference lists all of it](docs/language.md#19-what-azol-does-not-have), each with what to write instead.

## How the safety works

Values have value semantics: assigning a struct, a string or a list gives you an independent copy, and a function can change an argument only when the caller writes `mut`. That is the entire mental model.

Underneath, large values are passed by reference for speed, and the compiler guarantees you can never notice. A reference behaves exactly like a private copy because nothing can be changed while something else still refers to it. Four rules enforce that, each checked inside one function at a time, which is what keeps checking fast:

- One call cannot receive the same data twice when one of the uses is `mut`.
- A list cannot be changed while a loop is going through it.
- A value cannot be changed while a `match` is looking inside it.
- A string cannot be changed while an operator is still reading it.

When a rule is broken you get a plain sentence, not a puzzle:

```text
error[E0408]: cannot change `xs` while a loop is going through it
 --> loop.azl:4:9
  |
4 |         xs.push(x)
  |         ^
```

Memory is managed by ownership. Every buffer has exactly one owner, a variable or a temporary. A temporary hands its buffer over when it is stored; a variable is duplicated when it is stored again, unless that is the last time it is used, in which case it hands its buffer over too; and each buffer is freed when its owner goes out of scope. Copies exist in the meaning of the program, not necessarily in the machine: `text = text + more` adds to `text` in place. This works at any depth: a tree of nodes that each own a list of nodes is copied and freed correctly with nothing written by hand.

Out-of-range indexes and slices, `pop` on an empty list, division by zero and recursion that never ends stop the program with a message such as `azol: stack overflow`. They never read or write memory they should not.

## Built for AI agents

No model has seen azoL, so everything an agent needs is in the repository and in the compiler's output.

- [docs/for-agents.md](docs/for-agents.md) is a short briefing for a model that must write azoL correctly on the first try: what differs from Rust, Go, Swift and Python, the idioms, and how to read the errors.
- `azol check a.azl b.azl` checks a whole program without writing an executable.
- One run reports every independent error: the first one of each function, or the first syntax error of each file. An error that might only be a result of an earlier one is left for the next run, so nothing reported is noise.
- Every kind of error has a stable code, and `azol explain E0408` states the rule behind it with a wrong and a right example. All codes are listed in [docs/errors.md](docs/errors.md).
- `--json` (for `check` and `build`) prints one JSON object per error on standard output and a summary on the last line: the exact start and end of the offending text, the source line, the rule that was broken, and, when the repair is mechanical, the edit that makes it.
- `azol fmt` gives every file the one canonical layout, so an agent never spends effort on spacing and diffs stay small.

For this program

```
fn main() {
    let total = 0
    total = total + 1
    print(total)
}
```

`azol check --json total.azl` prints (wrapped here; lines and columns start at 1, the end is exclusive):

```text
{"severity":"error","code":"E0401",
 "message":"cannot change `total`: declare it with `var` to make it changeable",
 "file":"total.azl","line":3,"column":5,"end_line":3,"end_column":10,
 "source":"    total = total + 1",
 "rule":"A variable declared with `let` never changes; only one declared with `var` can be assigned, grown or passed as `mut`.",
 "help":"Declare the variable with `var` instead of `let`.",
 "fix":{"file":"total.azl","line":2,"column":5,"end_line":2,"end_column":8,"replacement":"var"}}
{"summary":{"errors":1,"files":1}}
```

A `fix` is only given when applying exactly that edit resolves the error; the tests apply each kind and check the result. Today that covers `let` that must be `var`, a missing or misplaced `mut` on an argument, missing `match` arms, a variant pattern without its values and `()` after a variant that holds none. Other errors come with the rule and no edit.

## Tools

| Command | What it does |
|---|---|
| `azol build <files...> [-o out]` | Compile to an executable. Several files make one program |
| `azol build --target <target> ...` | Compile for another system |
| `azol check <files...>` | Report every independent error and write nothing. `--json` for tools |
| `azol explain [code]` | Explain an error code, or list them all |
| `azol fmt <files...>` | Rewrite files in the canonical layout. `--check` only lists them, `-` formats standard input |
| `azol parse <file>`, `azol lex <file>` | Print the syntax tree or the tokens |

Syntax highlighting for VS Code is in [`editors/vscode`](editors/).

## Targets

| Target | Output | Tested |
|---|---|---|
| `aarch64-macos` | A signed Mach-O executable for Apple Silicon | On a Mac, and on an emulated CPU with a checking heap |
| `x86_64-linux` | A static ELF executable, no libc | Natively, and under valgrind |
| `aarch64-linux` | A static ELF executable, no libc | Only on an emulated CPU so far, never on real hardware |

The default is the machine the compiler runs on, and every target can be built from any of them.

## How it is built

Five stages, each a plain pass over the previous one's output:

1. **Lexer.** Bytes to tokens.
2. **Parser.** Tokens to a flat syntax tree with no per-node allocation. It never backtracks.
3. **Checker.** Names, types, and the ownership and exclusivity rules, one function at a time.
4. **Code generator.** Syntax tree straight to machine code in one pass: ARM64 or x86-64.
5. **Linker.** For macOS it lays out the Mach-O file, binds the system library imports and computes the code signature macOS requires. For Linux it writes a static ELF file that needs nothing from the system but the kernel.

The compiler has no dependencies, SHA-256 included. A small runtime for printing, files, growing lists and joining strings is embedded as machine code: hand-written assembly on macOS; on Linux, C without a C library, with its own memory allocator, talking to the kernel directly.

**How it is tested.** `sh test.sh` builds and runs every test program and compares what it prints, checks several thousand wrong programs for the exact report each must get, pins the bytes of every executable for all three targets, and lets the compiler build itself and pass the same tests again. Beyond that, each test program is executed on an emulated ARM64 CPU with a checking heap, which reports any buffer that is leaked, freed twice, used after being freed or overrun by a single byte. The x86-64 output is run natively on Linux and, linked against a checking allocator, under valgrind. A [fuzzer](tests/fuzz/) writes random programs, works out what each must print with an interpreter of its own, and compares that with what the compiled program prints on both processors.

## Self-hosting

The compiler is written in azoL: about 20,000 lines in [`compiler/`](compiler/) for the lexer, parser, checker, the ARM64 and x86-64 code generators, the three linkers, the diagnostics, the formatter and the command line, plus the part of the standard library that is azoL in [`lib/prelude.azl`](lib/prelude.azl). It builds itself.

- **Where it starts.** A compiler cannot build itself from nothing. [`bootstrap/`](bootstrap/) holds one ready-made compiler for each target, about 0.7 MB each, built from these same sources; [`bootstrap/README.md`](bootstrap/README.md) gives their hashes and how to make them again.
- **The proof it is the same compiler.** Built by the seed, the sources produce a compiler; that compiler produces another from the same sources; the two are the same file, byte for byte, and equal to the seed. `build.sh` checks this on every build.
- **Where that has been seen.** On x86-64 Linux natively. On macOS on an Apple Silicon Mac, with an earlier state of the sources (611,936 bytes then). For ARM64 Linux only on an emulated CPU, never on real hardware.
- **Where it came from.** The first compiler was written in Rust. Every part of the azoL one was accepted only when its output equalled the Rust one's byte for byte, over the repository's programs and thousands of deliberately broken ones: tokens, syntax trees, the checker's tables, error reports, and whole executables for all three targets. The scripts that did that, [`compiler/check_*.py`](compiler/), now compare a changed compiler with the one before it.
- **What it costs today.** The azoL-written compiler runs about twice as many instructions as the Rust one did for the same build (2.0 times for x86-64 Linux, 1.9 times for macOS, on the 132,000-line benchmark program, counted on x86-64 Linux). The build-time numbers above were measured with the Rust-hosted compiler; they will be measured again.

## Roadmap

What is not done, in the order planned. Each links to its design.

1. **Speed.** Programs: seven local improvements to the code generators that bring the benchmarks close to optimized Rust without an optimizer, then inlining ([docs/design/speed.md](docs/design/speed.md)). The compiler itself: back to the build speed of the Rust-hosted one.
2. **Floating-point numbers**: `f64` ([docs/design/floats.md](docs/design/floats.md)).
3. **Methods and traits**: methods on your own types, `==`, ordering and hashing for any type, bounded generics, `Map[K, V]` and `Set[T]` ([docs/design/traits.md](docs/design/traits.md)).
4. **Function values and closures**, and the library functions that take them ([docs/design/closures.md](docs/design/closures.md)).
5. **Threads and channels** without data races ([docs/design/threads.md](docs/design/threads.md)).
6. **Packages**: modules, `import`, dependencies without a central server, `test` blocks ([docs/design/packages.md](docs/design/packages.md)).
7. **More for AI agents**: `azol guide`, a named error with a fix for every habit from another language, `azol fix` ([docs/design/agents.md](docs/design/agents.md)).
8. **What replacing an old system needs**: calling C, more integer types, standard input, processes, directories, dates, JSON, CSV, stack traces with source lines ([docs/design/legacy.md](docs/design/legacy.md)).
9. **ARM64 Linux on real hardware.**

## Name

Rust is named after rust fungi. Azoles are the fungicides used against them. The capital L in the logo is a scraper lifting a layer of rust; the L alone ([`assets/mark.svg`](assets/mark.svg)) is the icon.

## License

Licensed under either of the [MIT license](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your option.
