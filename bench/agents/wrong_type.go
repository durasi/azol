// bench/fib40 with one mistake, the same in every language: the text "40" is
// passed where a number is expected. bench/compare.py runs each language's
// checker on its file and reports what the checker says and in what form.
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
	fmt.Println(fib("40"))
}
