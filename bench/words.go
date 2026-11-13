// Run-time benchmark: the same computation as bench/words.azl.
// Text work with the standard `strings` package.
package main

import (
	"fmt"
	"strings"
)

var input int64 = 400000

func main() {
	count := input
	names := []string{"alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"}
	var text strings.Builder
	seed := int64(11)
	for n := int64(0); n < count; n++ {
		seed = seed * 48271 % 2147483647
		if n > 0 {
			text.WriteString(" ")
		}
		text.WriteString(names[seed%8])
	}
	words := strings.Split(text.String(), " ")
	long, withT := 0, 0
	for _, word := range words {
		if len(word) > 4 {
			long++
		}
		if strings.Contains(word, "t") {
			withT++
		}
	}
	joined := strings.Join(words, ",")
	parts := strings.Split(joined, ",")
	starts := 0
	for _, part := range parts {
		if strings.HasPrefix(part, "e") {
			starts++
		}
	}
	fmt.Println(len(words) + long*3 + withT*5 + starts*7 + len(joined))
}
