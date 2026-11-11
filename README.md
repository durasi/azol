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

## Why azoL

**Builds that feel instant.** About 7 million lines per second, from source text to signed executable. A change is running before your finger leaves the key.

**Safety without ceremony.** No `&`, no `&mut`, no `'a`, no borrow checker fights. You write values; the compiler works out who owns what.

**Small enough to hold in your head.** No macros, no hidden control flow, one way to do things. The whole compiler is about 8,000 lines.

**Nothing underneath.** azoL does not sit on top of another compiler. It generates machine code and writes the executable file itself, down to the code signature.

## How the safety works

Values have value semantics: assigning a struct, a string or a list gives you an independent copy, and a function can change an argument only when the caller writes `mut`. That is the entire mental model.

Underneath, large values are passed by reference for speed, and the compiler guarantees you can never notice. A reference behaves exactly like a private copy because nothing can be changed while something else still refers to it. Four rules enforce that, each checked inside one function at a time, which is what keeps checking fast:

- One call cannot receive the same data twice when one of the uses is `mut`.
- A list cannot be changed while a loop is going through it.
- A value cannot be changed while a `match` is looking inside it.
- A string cannot be changed while an operator is still reading it.

When a rule is broken you get a plain sentence, not a puzzle:

    error: cannot change `xs` while a loop is going through it
     --> loop.azl:4:9
      |
    4 |         xs.push(x)
      |         ^

Memory is managed by ownership. Every buffer has exactly one owner, a variable or a temporary. A temporary hands its buffer over when it is stored; a variable is duplicated when it is stored again; and each buffer is freed when its owner goes out of scope. This works at any depth: a tree of nodes that each own a list of nodes is copied and freed correctly with nothing written by hand. An enum owns whatever its current variant holds, and only that.

Out-of-range indexes and slices, `pop` on an empty list and division by zero stop the program with a message. They never read or write memory they should not.

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

**Run time.** This is where Rust is ahead, and it is the next thing to fix. Computing `fib(40)` takes 0.64 s in azoL, 0.44 s in unoptimized Rust and 0.16 s in optimized Rust. azoL generates simple code today; an optimizing release mode is planned.

**Keeping the comparison fair.** The benchmark program is generated and repetitive, and azoL does less work than Rust: it has no optimizer, no standard library and no generics or traits yet. Some of this lead will be spent as those arrive. The design goal is to stay at least 4 times faster than Rust with the language complete, and today's margin leaves a great deal of room.

## What works today

- `i64`, `bool` and `str`
- text: `+`, `==`, `!=`, `len`, bytes with `text[i]`, `slice`, `find`, `contains`, `starts_with`, `ends_with`, `trim`, `split`, `lines`, `join`, `to_i64`, and `str(...)` and `chr(...)` to make text from numbers
- structs, including structs inside structs
- `List[T]` for any `T`, including lists of lists and lists of structs: `push`, `pop`, `len`, indexing
- enums whose variants carry values, with type parameters if you want them, and recursive ones through a list
- `Option[T]` and `Result[T, E]` built in: `Some`, `None`, `Ok`, `Err`
- `match` on enums, numbers, booleans and text, checked to cover every case
- functions and recursion, read-only parameters and `mut` parameters
- `let` (fixed) and `var` (changeable) variables
- `if` / `else`, `while`, `for ... in`, `match`, `break`, `continue`
- `print` for integers, booleans and text, `eprint` for standard error
- files with `read_file` and `write_file`, which return a `Result` instead of failing silently
- command-line arguments with `args()`, and `exit(code)`
- automatic memory management with no garbage collector
- compile errors that point at the exact place and say what to do

It is young. azoL targets Apple Silicon Macs only, has only a first, small standard library, and is missing generic functions and structs, patterns inside patterns, floating-point numbers and modules. Integer overflow currently wraps. Do not build anything you depend on with it yet.

## Try it

You need a Mac with Apple Silicon and a Rust toolchain to build the compiler.

    git clone https://github.com/durasi/azol
    cd azol
    cargo build --release
    ./target/release/azol build examples/players.azl
    ./examples/players

A real command-line tool, written in azoL:

    ./target/release/azol build examples/wc.azl
    ./examples/wc README.md src/main.rs

More to run: `examples/bank.azl`, `examples/tree.azl`, `examples/lists.azl`, `examples/structs.azl`, `examples/fib.azl`.

    azol build <file.azl>    compile to an executable
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
4. **Code generator.** Syntax tree straight to ARM64 machine code in one pass.
5. **Linker.** Lays out the Mach-O file, binds the system library imports, and computes the code signature macOS requires, hashing pages on several threads.

The compiler is written in Rust with no dependencies, SHA-256 included. A small runtime for printing, files, growing lists and joining strings is written in assembly and embedded as machine code. The text methods of the standard library (`find`, `split`, `trim` and the rest) are written in azoL itself, in `src/prelude.azl`, and compiled with every program.

**How it is tested.** Besides unit tests for every stage, each test program is executed on an emulated ARM64 CPU with a checking heap. The heap reports any buffer that is leaked, freed twice, used after being freed or overrun by a single byte, and it moves a buffer every time it grows so that stale pointers cannot hide. Every test program must come out clean. On a Mac the same programs also run natively against the real system allocator.

## Roadmap

1. Generic functions and structs
2. Skipping copies the compiler can prove unnecessary
3. An optimizing release mode, to close the run-time gap
4. Linux and x86-64
5. **The azoL compiler written in azoL.** Rust is scaffolding; the goal is to remove it.

## Name

Rust is named after rust fungi. Azoles are the fungicides used against them.

## License

Licensed under either of MIT or Apache-2.0, at your option.
