// Run-time benchmark: the same computation as bench/sort.azl.
// The numbers are appended to a slice, as the other versions push them
// onto a list.
package main

import "fmt"

var (
	input int64 = 2000000
	first int64 = 42
)

func quicksort(xs []int64, low, high int64) {
	if low >= high {
		return
	}
	pivot := xs[(low+high)/2]
	i, j := low, high
	for i <= j {
		for xs[i] < pivot {
			i++
		}
		for xs[j] > pivot {
			j--
		}
		if i <= j {
			xs[i], xs[j] = xs[j], xs[i]
			i++
			j--
		}
	}
	quicksort(xs, low, j)
	quicksort(xs, i, high)
}

func main() {
	count := input
	seed := first
	var xs []int64
	for n := int64(0); n < count; n++ {
		seed = seed * 48271 % 2147483647
		xs = append(xs, seed%1000000)
	}
	quicksort(xs, 0, count-1)
	var checksum int64
	for k := int64(0); k < count; k += 1000 {
		checksum += xs[k] * (k % 7)
	}
	ordered := int64(1)
	for m := int64(1); m < count; m++ {
		if xs[m-1] > xs[m] {
			ordered = 0
		}
	}
	fmt.Println(checksum + ordered)
}
