// bench/fib40 with one mistake, the same in every language: the text "40" is
// passed where a number is expected. bench/compare.py runs each language's
// checker on its file and reports what the checker says and in what form.
fn fib(n: i64) -> i64 {
    if n < 2 {
        return n;
    }
    fib(n - 1) + fib(n - 2)
}

fn main() {
    println!("{}", fib("40"));
}
