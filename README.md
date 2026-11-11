# azoL

*The rust remover.*

azoL is an experimental, memory-safe systems programming language designed for very fast compilation.

**Status:** early stage. Nothing here is usable yet.

## Goals

- Compile at least 4x faster than Rust on equivalent programs
- Memory safety without lifetime annotations
- A small language that is easy to learn

## Design

- Value semantics by default; references are only passed down as parameters
- Automatic reference counting only where static analysis cannot prove safety
- Custom fast backend for development builds, LLVM planned for release builds
- No macros, context-free grammar, function-local type checking

## Example

    struct Point {
        x: f64
        y: f64
    }

    fn shift(p: mut Point, dx: f64) {
        p.x = p.x + dx
    }

    fn main() {
        var p = Point { x: 1.0, y: 2.0 }
        shift(mut p, 3.0)
        print(p.x)
    }

## Name

Rust is named after rust fungi. Azoles are the fungicides used against them.

## License

Licensed under either of MIT or Apache-2.0, at your option.
