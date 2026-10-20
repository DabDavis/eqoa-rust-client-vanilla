//! The crate's arithmetic against its first implementation, kept here verbatim
//! as the reference: the one that was checked against the console's goldens
//! (`go-eqoa-vi/vimath/ee.go`, ported line by line), written for clarity with
//! a function call and several branches per operation. The crate's own is the
//! same function computed another way (truncation of the f64 bits instead of a
//! rounded conversion stepped back), and has to agree on EVERY input:
//!
//! - `sqrt` and `operand` over all 2^32 bit patterns;
//! - `mul`, `div`, `add`, `sub` and the compares over every pair of 1,592
//!   chosen values (zeros, denormals, the normal range's edges, infinities and
//!   NaNs, powers of two and their neighbours, 1.0's neighbours), and over
//!   random pairs in the regimes the shortcuts branch on: any bit patterns,
//!   exponents a fixed distance apart (the f64 sum exact or not, up to 60
//!   apart), a power of two half an f64 step below the other operand (where
//!   an inexact sum ties onto an f32), nearly cancelling sums, sums that land
//!   on an f32, and products and quotients at the underflow and overflow
//!   edges.
//!
//! `mul4`/`add4` (the same, four lanes at once) against four scalar calls.
//!
//! The adder (2026-10-20): the first implementation's `add` and `sub` are the
//! exact sum chopped, which is PCSX2's interpreter, where the tracer measured
//! (kept here verbatim as `interp_add`/`interp_sub`, the reference for
//! `ee::interp`). The console's (`ee::add`) aligns the smaller operand with one
//! guard bit first, as PCSX2's recompiler models it; its reference is that
//! code, `FPU_ADD_SUB` in `pcsx2/x86/iFPU.cpp`, written out step for step.
//!
//! `EE_PAIRS=n` sets the random pairs per regime (default 2,000,000; the
//! release build checks 100,000,000 in about a minute).

// ---- the reference, as first written (crates/ee/src/lib.rs before 2026-10-20)

mod reference {
    fn chop(x: f64) -> f32 {
        if x.is_nan() {
            return f32::NAN;
        }
        let mut f = x as f32;
        if (f as f64).abs() > x.abs() {
            f = next_toward_zero(f);
        }
        ftz(f)
    }

    fn next_toward_zero(f: f32) -> f32 {
        let b = f.to_bits();
        if b & 0x7FFF_FFFF == 0 {
            return f;
        }
        f32::from_bits(b - 1)
    }

    fn ftz(f: f32) -> f32 {
        let b = f.to_bits();
        if b & 0x7F80_0000 == 0 {
            return f32::from_bits(b & 0x8000_0000);
        }
        f
    }

    pub fn operand(f: f32) -> f32 {
        let b = f.to_bits();
        if b & 0x7F80_0000 == 0x7F80_0000 {
            return if b >> 31 != 0 { -f32::MAX } else { f32::MAX };
        }
        ftz(f)
    }

    fn two_sum(x: f64, y: f64) -> (f64, f64) {
        let s = x + y;
        let b = s - x;
        let e = (x - (s - b)) + (y - b);
        (s, e)
    }

    fn chop_sum(s: f64, e: f64) -> f32 {
        let r = chop(s);
        if e == 0.0 || r == 0.0 {
            return r;
        }
        let rem = (s - r as f64) + e;
        if (r > 0.0 && rem < 0.0) || (r < 0.0 && rem > 0.0) {
            return ftz(next_toward_zero(r));
        }
        r
    }

    pub fn mul(a: f32, b: f32) -> f32 {
        chop(operand(a) as f64 * operand(b) as f64)
    }

    pub fn interp_add(a: f32, b: f32) -> f32 {
        let (s, e) = two_sum(operand(a) as f64, operand(b) as f64);
        chop_sum(s, e)
    }

    pub fn interp_sub(a: f32, b: f32) -> f32 {
        let (s, e) = two_sum(operand(a) as f64, -(operand(b) as f64));
        chop_sum(s, e)
    }

    /// PCSX2's `FPU_ADD_SUB` (`iFPU.cpp`), on the conditioned operands: the
    /// exponent difference; 25 or more, the smaller keeps its sign only; 1 to
    /// 24, its bits masked by `0xffffffff << (diff - 1)`; then the operation,
    /// SSE's under the EE's rounding (the exact result toward zero).
    fn fpu_add_sub(a: f32, b: f32, sub: bool) -> f32 {
        let mut d = operand(a).to_bits();
        let mut t = operand(b).to_bits();
        let diff = ((d >> 23) & 0xff) as i32 - ((t >> 23) & 0xff) as i32;
        if diff >= 25 {
            t &= 0x8000_0000;
        } else if diff > 0 {
            t &= 0xffff_ffff << (diff - 1);
        } else if diff <= -25 {
            d &= 0x8000_0000;
        } else if diff < 0 {
            d &= 0xffff_ffff << (-diff - 1);
        }
        let (x, y) = (f32::from_bits(d) as f64, f32::from_bits(t) as f64);
        let (s, e) = two_sum(x, if sub { -y } else { y });
        chop_sum(s, e)
    }

    pub fn add(a: f32, b: f32) -> f32 {
        fpu_add_sub(a, b, false)
    }

    pub fn sub(a: f32, b: f32) -> f32 {
        fpu_add_sub(a, b, true)
    }

    pub fn div(a: f32, b: f32) -> f32 {
        let (a, b) = (operand(a), operand(b));
        if b == 0.0 {
            return if a.is_sign_negative() != b.is_sign_negative() { -f32::MAX } else { f32::MAX };
        }
        chop(a as f64 / b as f64)
    }

    pub fn sqrt(a: f32) -> f32 {
        chop((operand(a) as f64).sqrt())
    }

    pub fn lt(a: f32, b: f32) -> bool {
        operand(a) < operand(b)
    }

    pub fn le(a: f32, b: f32) -> bool {
        operand(a) <= operand(b)
    }

    pub fn eq(a: f32, b: f32) -> bool {
        operand(a) == operand(b)
    }
}

// ---- the comparison

/// The same answer: the same bits, or both NaN (`sqrt` of a negative, which
/// the caller retests; its payload is not the EE's to choose).
fn same(x: f32, y: f32) -> bool {
    x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan())
}

type Op = fn(f32, f32) -> f32;
type Cmp = fn(f32, f32) -> bool;
type Pair = fn(&mut Rng) -> (u32, u32);

/// Every binary operation on one pair; the first difference, described.
fn check_pair(a: f32, b: f32) -> Result<(), String> {
    let ops: [(&str, Op, Op); 6] = [
        ("mul", ee::mul, reference::mul),
        ("div", ee::div, reference::div),
        ("add", ee::add, reference::add),
        ("sub", ee::sub, reference::sub),
        ("interp add", ee::interp::add, reference::interp_add),
        ("interp sub", ee::interp::sub, reference::interp_sub),
    ];
    for (name, fast, slow) in ops {
        let (f, s) = (fast(a, b), slow(a, b));
        if !same(f, s) {
            return Err(format!(
                "{name}({a:e} [{:08X}], {b:e} [{:08X}]) = {f:e} [{:08X}], reference {s:e} [{:08X}]",
                a.to_bits(),
                b.to_bits(),
                f.to_bits(),
                s.to_bits()
            ));
        }
    }
    let cmps: [(&str, Cmp, Cmp); 3] =
        [("lt", ee::lt, reference::lt), ("le", ee::le, reference::le), ("eq", ee::eq, reference::eq)];
    for (name, fast, slow) in cmps {
        if fast(a, b) != slow(a, b) {
            return Err(format!("{name}({a:e} [{:08X}], {b:e} [{:08X}]) = {}", a.to_bits(), b.to_bits(), fast(a, b)));
        }
    }
    Ok(())
}

/// A small fast generator (xorshift64*), seeded per regime and thread.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn u32(&mut self) -> u32 {
        (self.next() >> 32) as u32
    }
}

fn pairs() -> u64 {
    std::env::var("EE_PAIRS").ok().and_then(|v| v.parse().ok()).unwrap_or(2_000_000)
}

/// Runs `pair` for `n` pairs spread over the machine's threads.
fn sweep(regime: &str, n: u64, pair: Pair) {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()) as u64;
    let failure = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                s.spawn(move || {
                    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (t + 1).wrapping_mul(0xD1B5_4A32_D192_ED03) ^ regime.len() as u64);
                    for _ in 0..n / threads {
                        let (a, b) = pair(&mut rng);
                        check_pair(f32::from_bits(a), f32::from_bits(b))?;
                    }
                    Ok::<(), String>(())
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).find_map(Result::err)
    });
    if let Some(e) = failure {
        panic!("{regime}: {e}");
    }
}

/// Values every operation has an edge at, and their neighbours.
fn chosen() -> Vec<u32> {
    let mut v = Vec::new();
    let near = |v: &mut Vec<u32>, b: u32| {
        for d in 0..4u32 {
            v.push(b.wrapping_add(d));
            v.push(b.wrapping_sub(d));
        }
    };
    for sign in [0, 0x8000_0000u32] {
        near(&mut v, sign); // zeros, the least denormals
        near(&mut v, sign | 0x007F_FFFF); // the greatest denormal, the least normal
        near(&mut v, sign | 0x7F7F_FFFF); // Fmax, the infinity
        near(&mut v, sign | 0x7FC0_0000); // the quiet NaN
        near(&mut v, sign | 0x3F80_0000); // 1.0
        near(&mut v, sign | 0x3F00_0000); // 0.5
        v.push(sign | 0x7FFF_FFFF);
        v.push(sign | 0x7F80_0001);
        v.push(sign | 0x0040_0000);
        // every power of two, and the all-ones significand under it
        for e in 1..255u32 {
            v.push(sign | e << 23);
            v.push(sign | e << 23 | 0x007F_FFFF);
            v.push(sign | e << 23 | 1);
        }
    }
    v.sort_unstable();
    v.dedup();
    v
}

#[test]
fn chosen_pairs() {
    let v = chosen();
    let failure = std::thread::scope(|s| {
        let v = &v;
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                s.spawn(move || {
                    for &a in v.iter().skip(t).step_by(threads) {
                        for &b in v {
                            check_pair(f32::from_bits(a), f32::from_bits(b))?;
                        }
                    }
                    Ok::<(), String>(())
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).find_map(Result::err)
    });
    if let Some(e) = failure {
        panic!("chosen pairs: {e}");
    }
    eprintln!("{} chosen values, {} pairs", v.len(), v.len() * v.len());
}

#[test]
fn any_bit_patterns() {
    sweep("any bit patterns", pairs(), |r| (r.u32(), r.u32()));
}

#[test]
fn exponents_a_fixed_distance_apart() {
    // the f64 sum is exact up to 28 apart, may be inexact past it, and an
    // inexact sum may land on an f32 from 53 apart
    sweep("exponents a fixed distance apart", pairs(), |r| {
        let a = r.u32();
        let gap = (r.u32() % 121) as i32 - 60;
        let ea = ((a >> 23) & 0xFF) as i32;
        let eb = (ea + gap).clamp(0, 255) as u32;
        let b = (r.u32() & 0x807F_FFFF) | eb << 23;
        (a, b)
    });
}

#[test]
fn powers_of_two_at_half_an_f64_step() {
    // b a power of two 50 to 56 exponents below a: around 53 apart it is half
    // an f64 step of a, the f64 sum ties, and an inexact sum lands on an f32
    sweep("powers of two at half an f64 step", pairs(), |r| {
        let a = r.u32() | 0x0080_0000;
        let ea = ((a >> 23) & 0xFF) as i32;
        let eb = (ea - 50 - (r.u32() % 7) as i32).clamp(0, 255) as u32;
        (a, (r.u32() & 0x8000_0000) | eb << 23)
    });
}

#[test]
fn nearly_cancelling() {
    // b is -a (or a, for sub) a few steps off: sums that lose most bits
    sweep("nearly cancelling", pairs(), |r| {
        let a = r.u32();
        let d = (r.u32() % 64) as i32 - 32;
        let mut b = (a as i32).wrapping_add(d) as u32;
        if r.u32() & 1 == 0 {
            b ^= 0x8000_0000;
        }
        (a, b)
    });
}

#[test]
fn small_integers_and_simple_fractions() {
    // sums that land exactly on an f32 (the error decides)
    sweep("small integers and simple fractions", pairs(), |r| {
        let pick = |r: &mut Rng| {
            let n = (r.u32() % 4096) as f32 - 2048.0;
            let scale = [1.0f32, 0.5, 0.25, 0.125, 1024.0, 1.0 / 1024.0][(r.u32() % 6) as usize];
            (n * scale).to_bits()
        };
        (pick(r), pick(r))
    });
}

#[test]
fn products_and_quotients_at_the_edges() {
    // a times b near 2^-126 and 2^128, and a over b likewise
    sweep("products and quotients at the edges", pairs(), |r| {
        let a = r.u32() & 0x7FFF_FFFF;
        let ea = ((a >> 23) & 0xFF) as i32;
        let target = if r.u32() & 1 == 0 { -126 } else { 127 };
        let e = if r.u32() & 1 == 0 { target - (ea - 127) } else { target + (ea - 127) };
        let eb = (e + 127 + (r.u32() % 3) as i32 - 1).clamp(0, 255) as u32;
        let b = (r.u32() & 0x807F_FFFF) | eb << 23;
        (a | (r.u32() & 0x8000_0000), b)
    });
}

#[test]
fn unary_over_every_bit_pattern() {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()) as u64;
    let failure = std::thread::scope(|s| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                s.spawn(move || {
                    let (lo, hi) = ((1u64 << 32) * t / threads, (1u64 << 32) * (t + 1) / threads);
                    for b in lo..hi {
                        let x = f32::from_bits(b as u32);
                        if !same(ee::sqrt(x), reference::sqrt(x)) {
                            return Err(format!("sqrt({x:e} [{b:08X}]) = {:e}, reference {:e}", ee::sqrt(x), reference::sqrt(x)));
                        }
                        if ee::operand(x).to_bits() != reference::operand(x).to_bits() {
                            return Err(format!("operand([{b:08X}])"));
                        }
                    }
                    Ok(())
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).find_map(Result::err)
    });
    if let Some(e) = failure {
        panic!("{e}");
    }
}

/// `mul4` and `add4` against four calls of `mul` and `add` (the reference's
/// equals, checked above), lane by lane, over every regime's pairs and the
/// chosen values mixed across lanes, so a lane that falls back takes the
/// others with it.
#[test]
fn four_lanes_agree() {
    let regimes: [Pair; 3] = [
        |r| (r.u32(), r.u32()),
        |r| {
            let a = r.u32();
            let ea = ((a >> 23) & 0xFF) as i32;
            let eb = (ea + (r.u32() % 121) as i32 - 60).clamp(0, 255) as u32;
            (a, (r.u32() & 0x807F_FFFF) | eb << 23)
        },
        |r| {
            let n = |r: &mut Rng| ((r.u32() % 64) as f32 - 32.0).to_bits();
            (n(r), n(r))
        },
    ];
    let v = chosen();
    let mut rng = Rng(0x5DEE_CE66_D1CE_4E5B);
    for k in 0..pairs() {
        let mut a = [0f32; 4];
        let mut b = [0f32; 4];
        for lane in 0..4 {
            let (x, y) = if k % 3 == 0 {
                (v[rng.u32() as usize % v.len()], v[rng.u32() as usize % v.len()])
            } else {
                regimes[rng.u32() as usize % 3](&mut rng)
            };
            (a[lane], b[lane]) = (f32::from_bits(x), f32::from_bits(y));
        }
        let (m, s) = (ee::mul4(a, b), ee::add4(a, b));
        for lane in 0..4 {
            let (x, y) = (a[lane], b[lane]);
            assert!(same(m[lane], ee::mul(x, y)), "mul4 lane {lane}: {x:e} [{:08X}] × {y:e} [{:08X}]", x.to_bits(), y.to_bits());
            assert!(same(s[lane], ee::add(x, y)), "add4 lane {lane}: {x:e} [{:08X}] + {y:e} [{:08X}]", x.to_bits(), y.to_bits());
        }
    }
}

/// The alignment is not idle: over random pairs a fixed distance apart, the
/// console's adder and the interpreter's differ on many (both references
/// passing above means `ee` has each), and an alignment with its edge one
/// exponent off (sign only from 26 apart) differs from the console's too.
#[test]
fn the_guard_bit_shows() {
    let off_by_one = |a: f32, b: f32| -> f32 {
        let (x, y) = (ee::operand(a).to_bits(), ee::operand(b).to_bits());
        let diff = ((x >> 23) & 0xff) as i32 - ((y >> 23) & 0xff) as i32;
        let y = if diff >= 26 { y & 0x8000_0000 } else if diff > 0 { y & (0xffff_ffff << (diff - 1).min(31)) } else { y };
        ee::interp::add(f32::from_bits(x), f32::from_bits(y))
    };
    let mut rng = Rng(0x0BAD_5EED_1234_5678);
    let (mut interp, mut edge) = (0, 0);
    for _ in 0..200_000 {
        let a = rng.u32() & 0x7FFF_FFFF | 0x0080_0000;
        let ea = ((a >> 23) & 0xFF) as i32;
        let eb = (ea - (rng.u32() % 30) as i32).clamp(1, 254) as u32;
        let b = (rng.u32() & 0x807F_FFFF) | eb << 23;
        let (a, b) = (f32::from_bits(a), f32::from_bits(b));
        if (a.abs() as f64) < 1e30 {
            interp += (!same(ee::add(a, b), ee::interp::add(a, b))) as usize;
            edge += (!same(ee::add(a, b), off_by_one(a, b))) as usize;
        }
    }
    assert!(interp > 10_000, "the two adders differ on {interp}");
    assert!(edge > 1_000, "the edge one off differs on {edge}");
}

/// The comparison catches a difference: a reference one step off on one
/// input fails `check_pair`'s test (here, by hand, against the crate).
#[test]
fn a_one_step_difference_is_seen() {
    let (a, b) = (1.0f32, 5.0f32);
    let wrong = f32::from_bits(ee::div(a, b).to_bits() + 1);
    assert!(!same(ee::div(a, b), wrong));
    // nearest-even, the host's own, differs from the EE's on this pair
    assert!(!same(ee::div(a, b), a / b));
    assert!(same(ee::div(a, b), reference::div(a, b)));
}
