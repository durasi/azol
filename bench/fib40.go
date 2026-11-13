// Run-time benchmark: the same computation as bench/fib40.azl.
// The argument is a package variable, so the compiler cannot work the
// answer out, as the Rust version reads it through `black_box`.
package main

import "fmt"

var input int64 = 40

func fib(n int64) int64 {
	if n < 2 {
		return n
	}
	return fib(n-1) + fib(n-2)
}

func main() {
	fmt.Println(fib(input))
}
