# azoL

*The rust remover.*

**A memory-safe systems language that compiles 132,000 lines of code in 18 milliseconds.**

azoL gives you what Rust is known for (no garbage collector, no dangling pointers, no double frees, no data left unfreed) without a single lifetime annotation, and with a compiler so fast that waiting for a build stops being part of programming.

| | azoL | Rust |
|---|---|---|
| Build a 132,000-line program | **0.018 s** | 3.82 s |
| Lifetime annotations | never | wherever the compiler cannot infer them |
| Garbage collector | none | none |
| Needed to produce an executable | `azol` alone | `rustc`, LLVM and the system linker |

The compiler reads your source and writes a signed, ready-to-run executable by itself. No LLVM, no assembler, no linker, no `codesign`, no dependencies.

## A taste

    struct Player {
        name: str
        scores: List[i64]
    }

    fn total(scores: List[i64]) -> i64 {
        var sum = 0
        for score in scores {
            sum = sum + score
        }
        return sum
    }

    fn describe(p: Player) -> str {
        return p.name + ": " + str(total(p.scores)) + " points in " + str(p.scores.len()) + " rounds"
    }

    fn main() {
        var ada = Player { name: "Ada", scores: [12, 30] }
        ada.scores.push(25)

        var grace = ada
        grace.name = "Grace"
        grace.scores.push(40)

        for player in [ada, grace] {
            print(describe(player))
        }
    }

    Ada: 67 points in 3 rounds
    Grace: 107 points in 4 rounds

`grace` is a copy with her own list, so adding to it leaves `ada` alone. The strings and lists built along the way are freed the moment nothing owns them. Nothing in this program says how.

There is no null and there are no exceptions. A value that may be missing is an `Option`, something that may fail returns a `Result`, and `match` makes you deal with every case:

    struct Account {
        owner: str
        balance: i64
    }

    enum Failure {
        NoSuchAccount(str)
        NotEnough(i64)
    }

    fn find(accounts: List[Account], owner: str) -> Option[i64] {
        for i, account in accounts {
            if account.owner == owner {
                return Some(i)
            }
        }
        return None
    }

    fn withdraw(accounts: mut List[Account], owner: str, amount: i64) -> Result[i64, Failure] {
        match find(accounts, owner) {
            None => return Err(NoSuchAccount(owner))
            Some(i) => {
                let balance = accounts[i].balance
                if balance < amount {
                    return Err(NotEnough(amount - balance))
                }
                accounts[i].balance = balance - amount
                return Ok(balance - amount)
            }
        }
    }

Leave out the `None` arm and the program does not compile:

    error: this `match` does not handle `None`; add an arm for it or a final `_ =>` arm

Code that works for any type is written once. The compiler works out the types at each call and builds a version for each:

    struct Stack[T] {
        items: List[T]
    }

    fn pop[T](stack: mut Stack[T]) -> Option[T] {
        if stack.items.len() == 0 {
            return None
        }
        return Some(stack.items.pop())
    }

## Why azoL

**Builds that feel instant.** About 7 million lines per second, from source text to signed executable. A change is running before your finger leaves the key.

**Safety without ceremony.** No `&`, no `&mut`, no `'a`, no borrow checker fights. You write values; the compiler works out who owns what.

**Small enough to hold in your head.** No macros, no hidden control flow, one way to do things. Each stage is a few thousand lines at most, and the whole compiler, with both machine targets, is about 12,000.

**Nothing underneath.** azoL does not sit on top of another compiler. It generates machine code and writes the executable file itself, down to the code signature.

## How the safety works

Values have value semantics: assigning a struct, a string or a list gives you an independent copy, and a function can change an argument only when the caller writes `mut`. That is the entire mental model.

Underneath, large values are passed by reference for speed, and the compiler guarantees you can never notice. A reference behaves exactly like a private copy because nothing can be changed while something else still refers to it. Four rules enforce that, each checked inside one function at a time, which is what keeps checking fast:

- One call cannot receive the same data twice when one of the uses is `mut`.
- A list cannot be changed while a loop is going through it.
- A value cannot be changed while a `match` is looking inside it.
- A string cannot be changed while an operator is still reading it.

When a rule is broken you get a plain sentence, not a puzzle:

    error[E0408]: cannot change `xs` while a loop is going through it
     --> loop.azl:4:9
      |
    4 |         xs.push(x)
      |         ^

Memory is managed by ownership. Every buffer has exactly one owner, a variable or a temporary. A temporary hands its buffer over when it is stored; a variable is duplicated when it is stored again, unless that is the last time it is used, in which case it hands its buffer over too; and each buffer is freed when its owner goes out of scope. Copies exist in the meaning of the program, not necessarily in the machine: `text = text + more` adds to `text` in place. This works at any depth: a tree of nodes that each own a list of nodes is copied and freed correctly with nothing written by hand. An enum owns whatever its current variant holds, and only that.

Out-of-range indexes and slices, `pop` on an empty list and division by zero stop the program with a message. They never read or write memory they should not.

## Built for AI agents

An agent that edits azoL code should not have to guess from prose what went wrong. The compiler tells it everything it knows, in a form it can act on:

- `azol check a.azl b.azl` checks a whole program without writing an executable.
- One run reports every independent error: the first one of each function, or the first syntax error of each file. An error that might only be a result of an earlier one is left for the next run, so nothing reported is noise.
- Every kind of error has a stable code, and `azol explain E0408` states the rule behind it with a wrong and a right example. All codes are listed in [docs/errors.md](docs/errors.md).
- `--json` (for `check` and `build`) prints one JSON object per error on standard output and a summary on the last line: the exact start and end of the offending text, the source line, the rule that was broken, and, when the repair is mechanical, the edit that makes it.

For this program

    fn main() {
        let total = 0
        total = total + 1
        print(total)
    }

`azol check --json total.azl` prints (wrapped here; lines and columns start at 1, the end is exclusive):

    {"severity":"error","code":"E0401",
     "message":"cannot change `total`: declare it with `var` to make it changeable",
     "file":"total.azl","line":3,"column":5,"end_line":3,"end_column":10,
     "source":"    total = total + 1",
     "rule":"A variable declared with `let` never changes; only one declared with `var` can be assigned, grown or passed as `mut`.",
     "help":"Declare the variable with `var` instead of `let`.",
     "fix":{"file":"total.azl","line":2,"column":5,"end_line":2,"end_column":8,"replacement":"var"}}
    {"summary":{"errors":1,"files":1}}

A `fix` is only given when applying exactly that edit resolves the error; the tests apply each kind and check the result. Today that covers `let` that must be `var`, a missing or misplaced `mut` on an argument, missing `match` arms, a variant pattern without its values and `()` after a variant that holds none. Other errors come with the rule and no edit.

## Numbers

Measured on one Apple Silicon Mac. Rust is plain `rustc` with no optimization flags.

**Compile time.** `bench/gen.py 3000` writes the same program in both languages: 3,000 structs that each own a list, and 12,000 functions that build, read, change and copy them, about 132,000 lines each. Both executables print the same result.

| | Time |
|---|---|
| azoL, source to signed executable | 0.018 s |
| Rust, type checking only (`--emit=metadata`) | 2.05 s |
| Rust, full build | 3.82 s |

That is roughly 210 times faster than the full Rust build, and 110 times faster than Rust's type checking alone.

**Executable size.** A small program is 33 KB in azoL and 472 KB in Rust.

**Run time.** The same build that takes milliseconds also runs fast. There is no separate slow debug mode.

| | azoL | Rust, optimized (`-O`) | Rust, unoptimized |
|---|---|---|---|
| Loops and arithmetic (`bench/loops`, 25 million rounds) | 0.022 s | 0.024 s | 0.085 s |
| Function calls (`bench/fib40`, `fib(40)`) | 0.39 s | 0.21 s | 0.56 s |

In loops azoL is level with optimized Rust; in code dominated by small function calls it is faster than unoptimized Rust and about 1.9 times slower than optimized Rust, which inlines calls and azoL does not yet.

**Keeping the comparison fair.** The compile-time benchmark is generated and repetitive, and azoL does less work than Rust: its optimizer is small, its standard library is small, and it has no traits. The run-time numbers come from two small programs, not from a benchmark suite. Some of this lead will be spent as those arrive. The design goal is to stay at least 4 times faster than Rust with the language complete, and today's margin leaves a great deal of room.

## What works today

- `i64`, `bool` and `str`
- bit operators `&`, `|`, `^`, `<<`, `>>`, and numbers written as `0x1F`, `0b1010` or a character such as `'a'`
- text: `+`, `==`, `!=`, `len`, bytes with `text[i]`, `slice`, `find`, `contains`, `starts_with`, `ends_with`, `trim`, `split`, `lines`, `join`, `to_i64`, and `str(...)` and `chr(...)` to make text from numbers
- structs, including structs inside structs
- `List[T]` for any `T`, including lists of lists and lists of structs: `push`, `pop`, `len`, indexing
- enums whose variants carry values, with type parameters if you want them, and recursive ones through a list
- `==` and `!=` on enums whose variants hold no values
- generic functions and structs: `fn pop[T](stack: mut Stack[T]) -> Option[T]`, with the types worked out at each call
- list methods `contains`, `first`, `last` and `reversed`
- `Option[T]` and `Result[T, E]` built in: `Some`, `None`, `Ok`, `Err`
- `match` on enums, numbers, booleans and text, checked to cover every case
- functions and recursion, read-only parameters and `mut` parameters
- `let` (fixed) and `var` (changeable) variables
- constants computed while compiling: `const MASK = (1 << 4) - 1`, usable everywhere, also as `match` patterns
- `if` / `else`, `while`, `for ... in`, `match`, `break`, `continue`
- `print` for integers, booleans and text, `eprint` for standard error
- files with `read_file`, `write_file` and `set_executable`, which return a `Result` instead of failing silently
- command-line arguments with `args()`, and `exit(code)`
- automatic memory management with no garbage collector
- compile errors that point at the exact place and say what to do

It is young. azoL targets Apple Silicon Macs and x86-64 Linux only, has only a first, small standard library, and is missing interfaces (a generic function is simply checked for each set of types it is used with), patterns inside patterns, floating-point numbers and modules. Integer overflow currently wraps. Do not build anything you depend on with it yet.

## Try it

You need a Rust toolchain to build the compiler, on an Apple Silicon Mac or on x86-64 Linux. It builds for the machine it runs on.

    git clone https://github.com/durasi/azol
    cd azol
    cargo build --release
    ./target/release/azol build examples/players.azl
    ./examples/players

A real command-line tool, written in azoL:

    ./target/release/azol build examples/wc.azl
    ./examples/wc README.md src/main.rs

A program can be spread over several files: `azol build main.azl shapes.azl -o app` compiles them as one, and they can use each other's declarations in any order.

More to run: `examples/generics.azl`, `examples/bank.azl`, `examples/tree.azl`, `examples/lists.azl`, `examples/structs.azl`, `examples/fib.azl`.

    azol build <file.azl>    compile to an executable
    azol build --target x86_64-linux <file.azl>
                             compile for Linux from a Mac (also `aarch64-linux`,
                             and `aarch64-macos` the other way)
    azol fmt <files...>      rewrite source files in the one canonical layout (`--check` to only list)
    azol check <file.azl>    report every independent error, write nothing (`--json` for tools)
    azol explain <code>      explain an error code
    azol parse <file.azl>    print the syntax tree
    azol lex <file.azl>      print the tokens
    cargo test               run the test suite

To repeat the benchmark:

    python3 bench/gen.py 3000
    ./target/release/azol build bench/generated/big.azl
    time rustc bench/generated/big.rs

## How it is built

Five stages, each a plain pass over the previous one's output:

1. **Lexer.** Bytes to tokens, about 20 million lines per second.
2. **Parser.** Tokens to a flat syntax tree with no per-node allocation. It never backtracks.
3. **Checker.** Names, types, and the ownership and exclusivity rules, one function at a time.
4. **Code generator.** Syntax tree straight to machine code in one pass: ARM64 or x86-64.
5. **Linker.** For macOS it lays out the Mach-O file, binds the system library imports, and computes the code signature macOS requires, hashing pages on several threads. For Linux it writes a static ELF file that needs nothing from the system but the kernel.

The compiler is written in Rust with no dependencies, SHA-256 included. A small runtime for printing, files, growing lists and joining strings is embedded as machine code: hand-written assembly on macOS; on Linux, C without a C library, with its own memory allocator, talking to the kernel directly. The text and list methods of the standard library (`find`, `split`, `trim`, `reversed` and the rest) are written in azoL itself, in `src/prelude.azl`, and compiled with every program.

**How it is tested.** Besides unit tests for every stage, each test program is executed on an emulated ARM64 CPU with a checking heap. The heap reports any buffer that is leaked, freed twice, used after being freed or overrun by a single byte, and it moves a buffer every time it grows so that stale pointers cannot hide. Every test program must come out clean. On a Mac the same programs also run natively against the real system allocator. The x86-64 output is run natively on Linux and, linked against a checking allocator, under valgrind.

## Roadmap

1. Inlining small functions, to close the rest of the run-time gap
2. **The azoL compiler written in azoL.** Rust is scaffolding; the goal is to remove it. The lexer, the parser and both linkers are already written in azoL (`compiler/`) and produce output identical to the Rust ones; the checker and the code generators are next.

## Name

Rust is named after rust fungi. Azoles are the fungicides used against them.

## License

Licensed under either of MIT or Apache-2.0, at your option.
