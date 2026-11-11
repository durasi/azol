# azoL

*The rust remover.*

azoL is an experimental, memory-safe systems programming language designed for very fast compilation.

**Status:** early stage. A small part of the language compiles to native executables on Apple Silicon Macs. It is not ready for real use.

## What works today

The compiler is written in Rust with no dependencies. It reads an `.azl` file and writes a signed macOS executable directly: no LLVM, no assembler, no system linker and no `codesign` run.

Supported so far:

- `i64` and `bool` values
- functions, including recursion
- `let` (fixed) and `var` (changeable) variables
- `if` / `else`, `while`, `break`, `continue`
- `print` for integers

The parser already accepts more of the planned language (structs, enums, `match`, `for`, generics). The compiler reports these as not supported yet.

## Try it

Requires a Mac with Apple Silicon and a Rust toolchain.

    git clone https://github.com/durasi/azol
    cd azol
    cargo build --release
    ./target/release/azol build examples/fib.azl
    ./examples/fib

Other commands:

    azol lex <file.azl>      print the tokens
    azol parse <file.azl>    print the syntax tree
    cargo test               run the test suite

## Example

    fn fib(n: i64) -> i64 {
        if n < 2 {
            return n
        }
        return fib(n - 1) + fib(n - 2)
    }

    fn main() {
        var i = 0
        while i <= 30 {
            print(fib(i))
            i = i + 5
        }
    }

## First measurement

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

1. Structs with value semantics and `mut` parameters
2. Strings, arrays, `for` and `match`
3. Enums and generics
4. The memory safety analysis
5. An optimizing release mode
6. Linux and x86-64 targets

## Name

Rust is named after rust fungi. Azoles are the fungicides used against them.

## License

Licensed under either of MIT or Apache-2.0, at your option.
