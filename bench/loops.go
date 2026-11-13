// Run-time benchmark: the same computation as bench/loops.azl.
package main

import "fmt"

var input int64 = 5000

func main() {
	size := input
	var total int64
	for i := int64(0); i < size; i++ {
		for j := int64(0); j < size; j++ {
			product := i * j
			if product%7 == 3 {
				total += product % 11
			} else {
				total++
			}
		}
	}
	fmt.Println(total)
}
