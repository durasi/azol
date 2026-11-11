# azoL

*The rust remover.*

azoL is an experimental, memory-safe systems programming language designed for very fast compilation.

**Status:** early stage. A small part of the language compiles to native executables on Apple Silicon Macs. It is not ready for real use.

## What works today

The compiler is written in Rust with no dependencies. It reads an `.azl` file and writes a signed macOS executable directly: no LLVM, no assembler, no system linker and no `codesign` run. The small runtime (printing, growing lists, error exits) is written in assembly; its machine code is embedded in the compiler.

Supported so far:

- `i64`, `bool` and `str` values (text literals)
- structs, including structs inside structs
- `List[T]`: growable arrays with `push`, `pop`, `len` and indexing
- functions, including recursion
- read-only parameters and `mut` parameters
- `let` (fixed) and `var` (changeable) variables
- `if` / `else`, `while`, `for ... in`, `break`, `continue`
- `print` for integers, booleans and text
- memory is freed automatically, with no garbage collector
- division by zero, a bad index or `pop` on an empty list stop the program with a message instead of continuing

### Memory safety so far

Values have value semantics. Assigning a struct or a list copies it, and a function can only change an argument the caller passes as `mut`. Large values are still passed by reference for speed, so the compiler makes sure a reference can never be told apart from a private copy: nothing can be changed while something else still refers to it.

Two rules do that, both checked inside one function at a time and without any annotations:

- One call cannot receive the same data twice when one of the uses is `mut`.
- A list cannot be changed while a loop is going through it.

      error: cannot change `xs` while a loop is going through it
       --> loop.azl:4:9
        |
      4 |         xs.push(x)
        |         ^

Every list buffer has exactly one owner, a variable or a temporary. A temporary hands its buffer over when it is stored (a move); a variable is duplicated when it is stored again (a copy); and the compiler frees each buffer when its owner goes out of scope. The test suite runs every test program on an emulated CPU with a checking heap, which confirms that each buffer is freed exactly once and never used afterwards.

Not covered yet: lists whose elements own memory (a list of lists), growable text, and integer overflow, which currently wraps.

The parser already accepts more of the planned language (enums, `match`, generics). The compiler reports these as not supported yet.

## Try it

Requires a Mac with Apple Silicon and a Rust toolchain.

    git clone https://github.com/durasi/azol
    cd azol
    cargo build --release
    ./target/release/azol build examples/lists.azl
    ./examples/lists

Other commands:

    azol lex <file.azl>      print the tokens
    azol parse <file.azl>    print the syntax tree
    cargo test               run the test suite

## Example

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

    fn add_round(player: mut Player, score: i64) {
        player.scores.push(score)
    }

    fn main() {
        var ada = Player { name: "Ada", scores: [12, 30] }
        add_round(mut ada, 25)

        var grace = ada
        grace.name = "Grace"
        add_round(mut grace, 40)

        print(total(ada.scores))
        print(total(grace.scores))
    }

This prints 67 and 107: `grace` is a copy with its own list, so adding to it leaves `ada` alone.

## First measurements

Building `examples/fib.azl` and the same program in Rust (`bench/fib.rs`, plain `rustc`), ten times each on an Apple Silicon Mac:

| | Per build | Executable size |
|---|---|---|
| azoL | about 0.01 s | 33 KB |
| Rust | about 0.23 s | 472 KB |

Read this with care:

- The program is tiny. At this size most of Rust's time is fixed cost, such as starting the compiler and linking the standard library. Larger programs have not been measured yet.
- azoL does far less work: no optimization, no standard library, a small subset of the language.
- The code azoL generates is deliberately simple and runs slower than optimized Rust.

On the front end alone, the lexer and parser handle about 13 million lines per second on the same machine.

Run time, computing `fib(40)` (`bench/fib40.azl` and `bench/fib40.rs`):

| | Run time |
|---|---|
| azoL | 0.64 s |
| Rust, no optimization | 0.44 s |
| Rust, optimized (`-O`) | 0.16 s |

azoL's code is about 1.5 times slower than unoptimized Rust and about 4 times slower than optimized Rust. Closing that gap is the job of the planned release mode.

## Goals

- Compile at least 4x faster than Rust on equivalent programs
- Memory safety without lifetime annotations
- A small language that is easy to learn

## Design

- Value semantics by default; references are only passed down as parameters
- Automatic reference counting only where static analysis cannot prove safety
- Custom fast backend for development builds, LLVM planned for release builds
- No macros, context-free grammar, function-local type checking

## Roadmap

1. Lists of lists, growable text and `match`
2. Enums and generics
3. Avoiding copies the compiler can prove unnecessary
4. An optimizing release mode
5. Linux and x86-64 targets
6. A compiler written in azoL itself, so that building azoL no longer needs Rust

## Name

Rust is named after rust fungi. Azoles are the fungicides used against them.

## License

Licensed under either of MIT or Apache-2.0, at your option.
