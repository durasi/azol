// fib40: function calls and recursion.
// Algorithm: the naive doubly recursive Fibonacci function.
// Sizes: fib(40); small: fib(30).
// Output: the number (102334155; small 46368).
// `SMALL` selects the small size; the gate changes that one line.
const SMALL: bool = false;

fn size(full: i64, small: i64) -> i64 {
    std::hint::black_box(if SMALL { small } else { full })
}

fn fib(n: i64) -> i64 {
    if n < 2 {
        return n;
    }
    fib(n - 1) + fib(n - 2)
}

fn main() {
    println!("{}", fib(size(40, 30)));
}
