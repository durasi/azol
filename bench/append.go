// Run-time benchmark: the same computation as bench/append.azl.
// `strings.Builder` is Go's text that grows; `strings.Join` is its join.
package main

import (
	"fmt"
	"strconv"
	"strings"
)

var (
	inputPieces int64 = 3000000
	inputCount  int64 = 300000
)

func main() {
	pieces, count := inputPieces, inputCount
	var text strings.Builder
	for i := int64(0); i < pieces; i++ {
		text.WriteString("0123456789")
	}
	var lines []string
	for n := int64(0); n < count; n++ {
		lines = append(lines, "line "+strconv.FormatInt(n, 10))
	}
	joined := strings.Join(lines, "\n")
	fmt.Println(text.Len() + len(joined))
}
