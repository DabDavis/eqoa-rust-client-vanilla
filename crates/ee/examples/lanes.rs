//! Four lanes against one at a time: `mul4` and `add4` (SSE2 on x86_64, NEON
//! on aarch64, SIMD128 on wasm32 built with it) against four calls of `mul`
//! and `add`, which is what they are on any other machine, alone and in the two shapes the game uses them in
//! (`vi`'s `Matrix44::mul` and `Matrix44::apply`), on matrices shaped like the
//! game's: a rotation with zeros in it, a translation row, w's column 0 0 0 1.
//! Both give the same bits (checked here on the data timed; the proof is
//! `tests/reference.rs`).
//!
//!   cargo run --release -p ee --example lanes
//!
//! (For wasm, build it for wasm32-wasip1, with `-C target-feature=+simd128`
//! and without, and run it under a WASI runtime or a page's.)
//!
//! Each figure is the best of seven runs, in nanoseconds a call.

use std::hint::black_box;
use std::time::Instant;

type L = [f32; 4];
type M = [L; 4];

/// A pair of four-lane functions.
trait Lanes {
    fn mul4(a: L, b: L) -> L;
    fn add4(a: L, b: L) -> L;
}

/// The crate's: SSE2 or NEON where there are, else four calls.
struct Four;
impl Lanes for Four {
    #[inline(always)]
    fn mul4(a: L, b: L) -> L {
        ee::mul4(a, b)
    }
    #[inline(always)]
    fn add4(a: L, b: L) -> L {
        ee::add4(a, b)
    }
}

/// Four calls, as `mul4` and `add4` are on a machine without four-lane code.
struct One;
impl Lanes for One {
    #[inline(always)]
    fn mul4(a: L, b: L) -> L {
        [ee::mul(a[0], b[0]), ee::mul(a[1], b[1]), ee::mul(a[2], b[2]), ee::mul(a[3], b[3])]
    }
    #[inline(always)]
    fn add4(a: L, b: L) -> L {
        [ee::add(a[0], b[0]), ee::add(a[1], b[1]), ee::add(a[2], b[2]), ee::add(a[3], b[3])]
    }
}

/// `Matrix44::mul` (crates/vi/src/geom.rs): each lane ((b0·x + b1·y) + b2·z)
/// + b3·w of a row of `a`.
#[inline(never)]
fn mat_mul<K: Lanes>(a: &M, b: &M) -> M {
    let row = |r: &L| {
        let s = K::mul4(b[0], [r[0]; 4]);
        let s = K::add4(s, K::mul4(b[1], [r[1]; 4]));
        let s = K::add4(s, K::mul4(b[2], [r[2]; 4]));
        K::add4(s, K::mul4(b[3], [r[3]; 4]))
    };
    [row(&a[0]), row(&a[1]), row(&a[2]), row(&a[3])]
}

/// `Matrix44::apply`: `v * m`, row vectors.
#[inline(never)]
fn apply<K: Lanes>(m: &M, v: &L) -> L {
    let s = K::mul4(m[0], [v[0]; 4]);
    let s = K::add4(s, K::mul4(m[1], [v[1]; 4]));
    let s = K::add4(s, K::mul4(m[2], [v[2]; 4]));
    K::add4(s, K::mul4(m[3], [v[3]; 4]))
}

const N: usize = 1024;
const REPS: usize = 2000;

fn other(i: usize) -> usize {
    (i * 7 + 3) % N
}

fn bits(v: L) -> u32 {
    v.iter().fold(0, |h, f| h.rotate_left(5) ^ f.to_bits())
}

// The index goes through `black_box`, not the operands: the operands load
// straight out of the arrays, as a matrix's rows do, and the loop cannot be
// computed ahead or several iterations at once.
#[inline(never)]
fn time_mul4<K: Lanes>(ms: &[M], vs: &[L]) -> u32 {
    let mut h = 0;
    for r in 0..REPS {
        for i in 0..N {
            let i = black_box(i);
            h ^= bits(K::mul4(ms[i][r & 3], [vs[i][r & 3]; 4]));
        }
    }
    h
}

#[inline(never)]
fn time_add4<K: Lanes>(ms: &[M], _: &[L]) -> u32 {
    let mut h = 0;
    for r in 0..REPS {
        for i in 0..N {
            let i = black_box(i);
            h ^= bits(K::add4(ms[i][r & 3], ms[other(i)][r & 3]));
        }
    }
    h
}

#[inline(never)]
fn time_mat_mul<K: Lanes>(ms: &[M], _: &[L]) -> u32 {
    let mut h = 0;
    for _ in 0..REPS {
        for i in 0..N {
            h ^= bits(mat_mul::<K>(black_box(&ms[i]), black_box(&ms[other(i)]))[1]);
        }
    }
    h
}

#[inline(never)]
fn time_apply<K: Lanes>(ms: &[M], vs: &[L]) -> u32 {
    let mut h = 0;
    for _ in 0..REPS {
        for i in 0..N {
            h ^= bits(apply::<K>(black_box(&ms[i]), black_box(&vs[i])));
        }
    }
    h
}

/// The best of seven runs of `f` (`REPS * N` calls), a call.
fn best(f: fn(&[M], &[L]) -> u32, ms: &[M], vs: &[L]) -> f64 {
    let mut least = f64::MAX;
    for _ in 0..7 {
        let t = Instant::now();
        black_box(f(ms, vs));
        least = least.min(t.elapsed().as_secs_f64());
    }
    least * 1e9 / (REPS * N) as f64
}

fn main() {
    // xorshift64*, fixed: the same data every run and on every machine
    let mut s = 0x9E37_79B9_7F4A_7C15u64;
    let mut rnd = move || {
        s ^= s >> 12;
        s ^= s << 25;
        s ^= s >> 27;
        (s.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
    };
    let ms: Vec<M> = (0..N)
        .map(|i| {
            let mut m = [[0f32; 4]; 4];
            for (r, row) in m.iter_mut().take(3).enumerate() {
                for (c, x) in row.iter_mut().take(3).enumerate() {
                    *x = if (i + r * 3 + c) % 3 == 0 { 0.0 } else { rnd() };
                }
            }
            m[3] = [rnd() * 3000.0, rnd() * 50.0, rnd() * 3000.0, 1.0];
            m
        })
        .collect();
    let vs: Vec<L> = (0..N).map(|_| [rnd() * 10.0, rnd() * 10.0, rnd() * 10.0, 1.0]).collect();

    // the same answers, on the data timed
    for i in 0..N {
        let (a, b, v) = (&ms[i], &ms[other(i)], &vs[i]);
        assert_eq!(mat_mul::<Four>(a, b).map(bits), mat_mul::<One>(a, b).map(bits), "Matrix44::mul, matrix {i}");
        assert_eq!(bits(apply::<Four>(a, v)), bits(apply::<One>(a, v)), "Matrix44::apply, matrix {i}");
        for r in 0..4 {
            let x = [v[r]; 4];
            assert_eq!(bits(Four::mul4(a[r], x)), bits(One::mul4(a[r], x)), "mul4, matrix {i} row {r}");
            assert_eq!(bits(Four::add4(a[r], b[r])), bits(One::add4(a[r], b[r])), "add4, matrix {i} row {r}");
        }
    }

    let path = if cfg!(target_arch = "x86_64") {
        "SSE2"
    } else if cfg!(all(target_arch = "aarch64", target_feature = "neon")) {
        "NEON"
    } else if cfg!(all(target_arch = "wasm32", target_feature = "simd128")) {
        "SIMD128"
    } else {
        "one lane at a time (no four-lane code for this machine)"
    };
    let os = std::env::consts::OS;
    println!("{}{}: four lanes are {path}", std::env::consts::ARCH, if os.is_empty() { String::new() } else { format!(" ({os})") });
    let rows: [(&str, fn(&[M], &[L]) -> u32, fn(&[M], &[L]) -> u32); 4] = [
        ("mul4", time_mul4::<Four>, time_mul4::<One>),
        ("add4", time_add4::<Four>, time_add4::<One>),
        ("Matrix44::mul", time_mat_mul::<Four>, time_mat_mul::<One>),
        ("Matrix44::apply", time_apply::<Four>, time_apply::<One>),
    ];
    for (name, four, one) in rows {
        let (fast, slow) = (best(four, &ms, &vs), best(one, &ms, &vs));
        println!("{name:<16} four lanes {fast:7.2} ns   one at a time {slow:7.2} ns   {:.2}x", slow / fast);
    }
}
