// Writes the corpus crates/ee/tests/libm.rs checks the Rust port of newlib's
// maths against: inputs from the regimes each function branches on, and the
// answer of go-eqoa-vi/vimath, whose functions were checked against the
// running console. A record is a line: the op, the bit patterns of a, b and
// the result in hex, and ok (1 or 0). Text, so the corpus can be committed.
//
//	go run . > ../../crates/ee/tests/libm_corpus.txt
package main

import (
	"bufio"
	"fmt"
	"math"
	"os"

	"github.com/DabDavis/go-eqoa-vi/vimath"
)

const (
	opSin = iota
	opCos
	opPow
	opAtan
	opAtan2
	opAcos
	opSqrtf
	opFmod
)

var rng uint64 = 0x5EED_1EB1

func next() uint64 {
	rng ^= rng >> 12
	rng ^= rng << 25
	rng ^= rng >> 27
	return rng * 0x2545F4914F6CDD1D
}

func unit() float32 { return float32(next()>>40) / float32(1<<24) }

func rnd(lo, hi float32) float32 { return lo + (hi-lo)*unit() }

// special values every function sees: zeros of both signs, denormals, one,
// the smallest normal, and a few bit patterns from the code's own branches.
var specials = []uint32{
	0x00000000, 0x80000000, 0x00000001, 0x80000001, 0x007FFFFF, 0x00800000,
	0x3F800000, 0xBF800000, 0x3F000000, 0x40000000, 0x3FC90FD0, 0x3FC90FDB,
	0x3F490FD8, 0x3F490FD9, 0x4016CBE4, 0x40490FDB, 0xC0490FDB, 0x43490F80,
	0x31000000, 0x3EE00000, 0x3F300000, 0x3F980000, 0x401C0000, 0x50800000,
}

func main() {
	w := bufio.NewWriter(os.Stdout)
	defer w.Flush()
	rec := func(op byte, a, b, r float32, ok bool) {
		k := 0
		if ok {
			k = 1
		}
		fmt.Fprintf(w, "%d %08x %08x %08x %d\n", op, math.Float32bits(a), math.Float32bits(b), math.Float32bits(r), k)
	}
	pick := func() float32 {
		switch next() % 6 {
		case 0:
			return math.Float32frombits(specials[next()%uint64(len(specials))])
		case 1:
			return rnd(-1, 1)
		case 2:
			// near a multiple of pi/2
			return float32(int(next()%9)-4)*math.Pi/2 + rnd(-1e-4, 1e-4)
		case 3:
			return rnd(-200, 200)
		default:
			return rnd(-7, 7)
		}
	}
	for i := 0; i < 3000; i++ {
		x := pick()
		v, ok := vimath.Sinf(x)
		rec(opSin, x, 0, v, ok)
		v, ok = vimath.Cosf(x)
		rec(opCos, x, 0, v, ok)
	}
	for i := 0; i < 3000; i++ {
		var x, y float32
		switch next() % 5 {
		case 0: // the game's: powf(damping, dt)
			x, y = rnd(0.001, 0.9), rnd(0.001, 0.2)
		case 1:
			x, y = rnd(0.001, 100), rnd(-10, 10)
		case 2: // negative base, integer and half-integer exponents
			x, y = rnd(-5, -0.01), float32(int(next()%11)-5)*[]float32{1, 0.5}[next()%2]
		case 3:
			x, y = pick(), pick()
		default:
			x, y = rnd(0.999999, 1.000001), rnd(-3e8, 3e8)
		}
		v, ok := vimath.Powf(x, y)
		rec(opPow, x, y, v, ok)
	}
	for i := 0; i < 3000; i++ {
		x := pick()
		if next()%3 == 0 {
			x = rnd(-1e6, 1e6)
		}
		rec(opAtan, x, 0, vimath.Atanf(x), true)
		y, xx := pick(), pick()
		if next()%4 == 0 {
			y, xx = rnd(-1e-3, 1e-3), rnd(-1e3, 1e3)
		}
		rec(opAtan2, y, xx, vimath.Atan2f(y, xx), true)
		a := rnd(-1, 1)
		if next()%5 == 0 {
			a = pick()
		}
		v, ok := vimath.Acosf(a)
		rec(opAcos, a, 0, v, ok)
		s := rnd(0, 1000)
		if next()%4 == 0 {
			s = pick()
		}
		rec(opSqrtf, s, 0, vimath.Sqrtf(s), true)
		f := rnd(-50, 50)
		rec(opFmod, f, 6.28318548, vimath.Fmodf(f, 6.28318548), true)
	}
}
