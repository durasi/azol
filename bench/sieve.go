// Run-time benchmark: the same computation as bench/sieve.azl.
// One `bool` per number in a slice made at its full size, which is how Go
// writes the list of `false` the other versions build.
package main

import "fmt"

var input int64 = 20000000

func main() {
	limit := input
	composite := make([]bool, limit+1)
	var count, last int64
	for i := int64(2); i <= limit; i++ {
		if !composite[i] {
			count++
			last = i
			for j := i * i; j <= limit; j += i {
				composite[j] = true
			}
		}
	}
	fmt.Println(count*100000000 + last)
}
