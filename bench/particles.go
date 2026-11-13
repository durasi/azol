// Run-time benchmark: the same computation as bench/particles.azl.
// A slice of structs updated in place.
package main

import "fmt"

type Particle struct {
	x, y, vx, vy int64
}

const (
	width  = 100000
	height = 60000
)

var (
	inputCount  int64 = 2000
	inputRounds int64 = 50000
)

func step(ps []Particle) {
	for i := range ps {
		x := ps[i].x + ps[i].vx
		y := ps[i].y + ps[i].vy
		if x < 0 || x > width {
			ps[i].vx = 0 - ps[i].vx
			x = x + 2*ps[i].vx
		}
		if y < 0 || y > height {
			ps[i].vy = 0 - ps[i].vy
			y = y + 2*ps[i].vy
		}
		ps[i].x = x
		ps[i].y = y
	}
}

func energy(ps []Particle) int64 {
	var total int64
	for _, p := range ps {
		total = total + p.x*3 + p.y*5 + p.vx*p.vx + p.vy*p.vy
	}
	return total
}

func main() {
	count, rounds := inputCount, inputRounds
	var ps []Particle
	seed := int64(7)
	for n := int64(0); n < count; n++ {
		seed = seed * 48271 % 2147483647
		x := seed % width
		seed = seed * 48271 % 2147483647
		y := seed % height
		seed = seed * 48271 % 2147483647
		vx := seed%201 - 100
		seed = seed * 48271 % 2147483647
		vy := seed%201 - 100
		ps = append(ps, Particle{x, y, vx, vy})
	}
	var total int64
	for round := int64(0); round < rounds; round++ {
		step(ps)
		if round%500 == 0 {
			total += energy(ps) % 1000003
		}
	}
	fmt.Println(total + energy(ps))
}
