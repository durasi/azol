// The same program as examples/fib.azl, used to compare build times.

fn fib(n: i64) -> i64 {
    if n < 2 {
        return n;
    }
    fib(n - 1) + fib(n - 2)
}

fn main() {
    let mut i = 0;
    while i <= 30 {
        println!("{}", fib(i));
        i += 5;
    }
}
