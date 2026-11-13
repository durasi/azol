<p align="center">
  <img src="assets/logo.svg#gh-light-mode-only" alt="azoL" height="104">
  <img src="assets/logo-dark.svg#gh-dark-mode-only" alt="azoL" height="104">
</p>

<p align="center"><em>The rust remover.</em></p>

<p align="center">
  <strong>A memory-safe systems language with no garbage collector and no lifetime annotations,<br>
  whose compiler turns 44,000 lines into an executable in 24 milliseconds.</strong>
</p>

<p align="center">
  <a href="#numbers">Numbers</a> ·
  <a href="#azol-rust-c-go-and-javascript">Comparison</a> ·
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

One table, one machine, one run of [`bench/compare.py`](bench/compare.py), which writes this section itself (`python3 bench/compare.py --write-readme`). Every program is the same algorithm written plainly in each language ([`bench/`](bench/)), and all versions must print the same result.

<!-- bench:start -->
Measured on Apple M5, Darwin 27.2.0 (arm64). Best of 5 runs.
Tools: azol 0.1.0; rustc 1.99.1 (58cf8f9c3 2026-10-13); Apple clang version 21.0.0 (clang-2100.4.1.1); go version go1.27.1 darwin/arm64; node v22.17.0.

Lower is better everywhere. In every row the best value is bold, and nothing else in the table is.

|  | azoL | azoL `-O` | Rust `-O` | C `-O2` | Go | JavaScript (Node.js) |
|:---|---:|---:|---:|---:|---:|---:|
| Build time, the same 44,004-line program | **0.024 s** | 0.45 s | 9.51 s | 127.04 s | 0.67 s | nothing to build |
| Executable size, `fib40` | **33 KB** | **33 KB** | 460 KB | **33 KB** | 2373 KB | needs Node.js |
| Run time, `fib40`: function calls (recursive `fib(40)`) | 0.29 s | **0.15 s** | **0.15 s** | **0.15 s** | 0.23 s | 0.52 s |
| Run time, `loops`: nested loops and arithmetic | 0.017 s | **0.016 s** | 0.018 s | 0.018 s | 0.018 s | 0.054 s |
| Run time, `sort`: quicksort of 2 million numbers | 0.14 s | **0.11 s** | **0.11 s** | **0.11 s** | **0.11 s** | 0.27 s |
| Run time, `sieve`: prime sieve up to 20 million | 0.16 s | 0.15 s | 0.045 s | 0.041 s | **0.036 s** | 0.30 s |
| Run time, `particles`: list of structs updated in place | 0.096 s | 0.073 s | **0.039 s** | 0.050 s | 0.051 s | 0.17 s |
| Run time, `tree`: building and walking trees | 0.094 s | 0.085 s | **0.054 s** | 0.058 s | 0.065 s | 0.42 s |
| Run time, `append`: building long strings | 0.026 s | 0.025 s | 0.021 s | **0.019 s** | 0.027 s | 0.12 s |
| Run time, `words`: splitting and joining text | 0.034 s | 0.030 s | 0.025 s | 0.026 s | **0.018 s** | 0.10 s |

The generated program is 44,004 lines in azoL, 45,006 lines in Rust, 49,035 lines in C, 44,008 lines in Go, 38,002 lines in JavaScript.

Build: azoL's default build is 400 times faster than `rustc -O` (9.51 s), 54 times faster than an unoptimized `rustc` build (1.28 s), 29 times faster than `rustc`'s type check alone (`--emit=metadata`) (0.69 s), 5400 times faster than `cc -O2` (127.04 s), 29 times faster than `go build` (0.67 s).

Node.js has nothing to build; checking the syntax of the JavaScript version (`node --check`) takes 0.024 s.

Run: azoL's default build is ahead of Rust `-O` in 1 of 8, of C `-O2` in 1 of 8, of Go in 2 of 8, of Node.js in 8 of 8.

Run: `azol build -O` is ahead of Rust `-O` in 1 of 8 (level in 2), of C `-O2` in 1 of 8 (level in 2), of Go in 3 of 8 (level in 1), of Node.js in 8 of 8.
<!-- bench:end -->

**Reading these fairly.**

- azoL has two columns. The first is its default build, one fast pass. The second is `azol build -O`, the optimiser: new, experimental, and slower to build. Both check every index and every division.
- Rust and Go check indexes too; C checks nothing. Go and JavaScript collect garbage; azoL, Rust and C free memory at fixed points.
- The build-time program is generated and repetitive ([`bench/gen.py`](bench/gen.py)), and it is one file. That flatters azoL, which does less work than the others (no traits, a small library), and is hard on C, whose projects are split into many files compiled side by side. Go's build cache is kept for its standard library and bypassed for the program.
- Run times are whole processes, so they include start-up, which is tens of milliseconds for Node.js.
- These are eight small programs, not a benchmark suite. The gate azoL holds itself to is larger: 28 programs against `rustc -O` ([`bench/suite/`](bench/suite/), see the roadmap).

## azoL, Rust, C, Go and JavaScript

In each row, bold marks the languages that are ahead on that row, where that can be said plainly. Rows without bold are trade-offs.

| | <img src="assets/compare/azol.svg" alt="azoL" height="30"> | <img src="assets/third-party/rust-logo-blk.svg#gh-light-mode-only" alt="" height="30"><img src="assets/compare/rust.svg#gh-dark-mode-only" alt="" height="30"> <strong>Rust</strong> | <img src="assets/compare/c.svg" alt="C" height="30"> | <img src="assets/compare/go.svg" alt="Go" height="30"> | <img src="assets/third-party/js.png" alt="" height="30"> <strong>JavaScript</strong> |
|:---|:---|:---|:---|:---|:---|
| <sub>SAFETY</sub> | | | | | |
| Memory safety without a garbage collector | **Yes, checked when compiling** | **Yes, checked when compiling; `unsafe` opts out** | No | Safe, with a garbage collector | Safe, with a garbage collector |
| Lifetime annotations | Never | Where the compiler cannot infer them | None | None | None |
| References in your code | None. Values only; a call says `mut` to let a function change its argument | `&T`, `&mut T`, smart pointers | Pointers | Pointers | Every object is a shared reference |
| Data races | No threads yet | **Ruled out when compiling** | Possible | Possible; a race detector for tests | No shared memory by default |
| Null | **None. `Option[T]`** | **None in safe code. `Option<T>`** | `NULL` | `nil` | `null` and `undefined` |
| Errors | `Result[T, E]`, `?`, exhaustive `match` | `Result<T, E>`, `?`, exhaustive `match`, panics | Return codes | Error values, `panic` | Exceptions |
| <sub>LANGUAGE</sub> | | | | | |
| Types | Static, inferred inside functions | Static, inferred inside functions | Static | Static, inferred inside functions | Dynamic |
| Generics | Functions, structs and enums. No bounds yet | **With traits and bounds** | None; macros | **With constraints** | Not needed |
| Numbers | `i64` only. No floating point | **Integers of every size, floats** | **Integers of every size, floats** | **Integers of every size, floats** | 64-bit floats, `BigInt` |
| Closures | No | **Yes** | No; function pointers | **Yes** | **Yes** |
| Threads | No | **Yes** | Yes, through a library | **Yes: goroutines and channels** | Workers and an event loop |
| Packages | None. Several files, one namespace | **Cargo, crates.io** | No standard one | **Go modules** | **npm** |
| <sub>BUILDING AND SHIPPING</sub> | | | | | |
| From source to running | Compiled ahead of time | Compiled ahead of time | Compiled ahead of time | Compiled ahead of time | Compiled while it runs |
| What you ship | One executable | One executable | One executable | One executable | Source; the user needs a runtime |
| Building for another system | **Built in: `--target`** | A target to install, and a linker for it | A cross compiler | **Built in: `GOOS`, `GOARCH`** | Not applicable |
| Targets | Apple Silicon macOS, x86-64 Linux, ARM64 Linux | Dozens | **Nearly every processor** | Dozens | Wherever an engine runs |
| <sub>TOOLS</sub> | | | | | |
| Errors for tools and AI agents | `check --json`: code, exact span, the rule that was broken, the edit when the repair is mechanical | `--error-format=json`, with suggested edits marked by how safe they are | Text, with fix-it hints | Text | No compiler; errors appear when the code runs |
| Formatter | **`azol fmt`, one layout** | **`rustfmt`, with the toolchain** | Separate tools | **`gofmt`, one layout** | Separate tools |
| <sub>PROJECT</sub> | | | | | |
| Written in | azoL: the compiler compiles itself | Rust | Compilers are written in C and C++ | Go | Engines are written in C++ |
| Maturity | Experimental. Do not depend on it | **Stable since 2015** | **A standard since 1989** | **Stable since 2012** | **In use since 1995** |

Rust, C, Go and JavaScript can each do far more than azoL can today. What azoL offers is memory safety checked when compiling, with no garbage collector and no lifetime annotations, a build measured in milliseconds, and a language whose whole reference is one document.

<sub>The Rust logo belongs to the Rust Foundation and the JS logo to its community; they only label the columns. C and Go are labelled by name. azoL is not affiliated with or endorsed by any of them. Sources and terms: [assets/third-party](assets/third-party/README.md).</sub>

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
| `azol build -O ...` | Compile through the optimiser: a faster program, a slower build. Experimental |
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

What is not done, in the order planned. Finished work is not listed here; it is in the sections above and in the history.

**Speed: `azol build -O`, as fast as optimized Rust** ([design](docs/design/optimizer.md), [findings](docs/design/optimizer-findings.md))

1. `azol build -O` exists and is experimental. Still to do: the compiler itself built with it (new seeds), so that the default build gets its speed back.
2. The programs still far from `rustc -O`: lists of structs, the library sort, maps, building text.
3. The remaining passes: unrolling, copy elision, string building, vectorising.
4. Packed lists and structs, a faster allocator, an internal calling convention.
5. `-O` compile speed.
6. The gate that defines the claim: the 28 programs of [`bench/suite/`](bench/suite/) at a geometric mean of 1.00 or better against `rustc -O`, none worse than 1.10, on Apple M5 and on x86-64. Today: 1.60 in time on the Apple M5 (the default build: 2.13), with 6 of the 28 inside 1.10; 1.09 in instructions on x86-64.

**Language**

7. Floating-point numbers: `f64` ([design](docs/design/floats.md)).
8. Methods and traits: methods on your own types, `==`, ordering and hashing for any type, bounded generics, `Map[K, V]` and `Set[T]` ([design](docs/design/traits.md)).
9. Function values and closures, and the library functions that take them ([design](docs/design/closures.md)).
10. Threads and channels without data races ([design](docs/design/threads.md)).
11. Packages: modules, `import`, dependencies without a central server, `test` blocks ([design](docs/design/packages.md)).

**For AI agents and for replacing old systems**

12. `azol guide`, a named error with a fix for every habit from another language, `azol fix` ([design](docs/design/agents.md)).
13. Calling C, more integer types, standard input, processes, directories, dates, JSON, CSV, stack traces with source lines ([design](docs/design/legacy.md)).

**Before the first release**

14. ARM64 Linux on real hardware.
15. The numbers above measured again on an idle machine, with `-O`.

## Name

Rust is named after rust fungi. Azoles are the fungicides used against them. The capital L in the logo is a scraper lifting a layer of rust; the L alone ([`assets/mark.svg`](assets/mark.svg)) is the icon.

## License

Licensed under either of the [MIT license](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your option.
