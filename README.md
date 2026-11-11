# azoL

*The rust remover.*

azoL is an experimental, memory-safe systems programming language designed for very fast compilation.

**Status:** early stage. A small part of the language compiles to native executables on Apple Silicon Macs. It is not ready for real use.

## What works today

The compiler is written in Rust with no dependencies. It reads an `.azl` file and writes a signed macOS executable directly: no LLVM, no assembler, no system linker and no `codesign` run.

Supported so far:

- `i64` and `bool` values
- structs, including structs inside structs
- functions, including recursion
- read-only parameters and `mut` parameters
- `let` (fixed) and `var` (changeable) variables
- `if` / `else`, `while`, `break`, `continue`
- `print` for integers

Values have value semantics. Assigning a struct copies it, and a function can only change an argument the caller passes as `mut`. Large values are still passed by reference for speed; the compiler rejects any call that would let two arguments refer to the same data while one of them is `mut`, so a reference can never be told apart from a private copy. This is the first rule of the memory safety analysis:

    error: `p` is passed twice in one call and one of the uses is `mut`; copy it into a variable first
      --> alias.azl:14:21
       |
    14 |     swap(mut p, mut p)
       |                     ^

The parser already accepts more of the planned language (enums, `match`, `for`, generics, strings, arrays). The compiler reports these as not supported yet.

## Try it

Requires a Mac with Apple Silicon and a Rust toolchain.

    git clone https://github.com/durasi/azol
    cd azol
    cargo build --release
    ./target/release/azol build examples/structs.azl
    ./examples/structs

Other commands:

    azol lex <file.azl>      print the tokens
    azol parse <file.azl>    print the syntax tree
    cargo test               run the test suite

## Example

    struct Point {
        x: i64
        y: i64
    }

    fn shift(p: mut Point, dx: i64) {
        p.x = p.x + dx
    }

    fn main() {
        let a = Point { x: 1, y: 2 }
        var b = a
        shift(mut b, 3)
        print(a.x)
        print(b.x)
    }

This prints 1 and 4: `b` is a copy, so changing it leaves `a` alone.

## First measurements

Building `examples/fib.azl` and the same program in Rust (`bench/fib.rs`, plain `rustc`), ten times each on an Apple Silicon Mac:

| | Per build | Executable size |
|---|---|---|
| azoL | about 0.01 s | 16.7 KB |
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

1. Strings, arrays, `for` and `match`
2. Enums and generics
3. Heap values and the rest of the memory safety analysis
4. An optimizing release mode
5. Linux and x86-64 targets
6. A compiler written in azoL itself, so that building azoL no longer needs Rust

## Name

Rust is named after rust fungi. Azoles are the fungicides used against them.

## License

Licensed under either of MIT or Apache-2.0, at your option.
