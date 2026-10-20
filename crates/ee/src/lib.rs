//! The Emotion Engine's FPU, as the game sees it.
//!
//! Ported from `go-eqoa-vi/vimath/ee.go`, where every rule below was measured
//! on a running game through the tracer, not read out of a manual. The EE
//! FPU is not IEEE-754:
//!
//! - it rounds toward zero, always (no rounding-mode control);
//! - it flushes denormals to a signed zero, on the way IN as well as out;
//! - it saturates to ±`f32::MAX` instead of producing an infinity, and an
//!   exponent-255 operand is clamped to ±`f32::MAX` before it is used;
//! - a comparison conditions its operands too, so `0 < 1.4e-45` is false;
//! - its adder aligns the smaller operand with one guard bit and no sticky
//!   bit: bits shifted past the guard are dropped before the add, and an
//!   operand 25 or more exponents below the other adds nothing but its sign
//!   (`add`).
//!
//! The last is PCSX2's recompiler's (`FPU_ADD_SUB`, `iFPU.cpp`), which the
//! game plays on; the tracer's `call` runs guest code in PCSX2's interpreter,
//! whose `ADD_S` is the host's addition (the exact sum toward zero), so
//! go-eqoa-vi measured that one. The two differ where the dropped bits would
//! have borrowed: `x + -tiny` is `x` on the console and one step toward zero
//! in the interpreter, which made a body with a little velocity left after a
//! stop creep a float step toward the world's origin every walk tick for the
//! twenty seconds the velocity took to underflow (2 mm a tick at Freeport's
//! x of 25,000; F3 20261020-194625, 0.3 m/s at 165 frames a second). The
//! interpreter's adder is kept as `interp`, for the ports checked against it.
//!
//! Host `f32` arithmetic rounds to nearest-even and disagrees by one step on
//! most inexact results. Game logic whose feel has to match the console
//! (walking, camera, collision, animation) goes through these functions;
//! rendering does not need to.
//!
//! Rust never contracts `a * b + c` into one fused rounding on its own, so the
//! order of operations written here is the order the guest executes.

pub mod interp;
pub mod libm;

/// What `libm` computes with (`interp::libm` has the interpreter's adder).
mod arith {
    pub use crate::{add, div, mul, operand, sub};
}

/// The f64 bit pattern of 2^-126, the least normal f32.
const MIN_NORMAL: u64 = 0x3810_0000_0000_0000;
/// The f64 bit pattern of 2^128, the least magnitude an f32 cannot hold.
const OVERFLOW: u64 = 0x47F0_0000_0000_0000;

// The game's logic makes about half a million of these calls a frame, and more
// than half of them have a zero operand (measured in play, 2026-10-20), so
// the usual path has no branch to mispredict: selects, not tests.

/// Reduces a finite `x` to `f32` rounding toward zero; a result below the
/// least normal flushes to a signed zero, and one at or past 2^128 lands on
/// ±`f32::MAX`, which is the EE's saturation.
///
/// Toward zero is a truncation: the f64 kept to the 24 significant bits an
/// f32 holds converts exactly, whatever the host's rounding mode.
#[inline(always)]
fn chop(x: f64) -> f32 {
    let b = x.to_bits();
    let mag = b & 0x7FFF_FFFF_FFFF_FFFF;
    let sign = ((b >> 32) as u32) & 0x8000_0000;
    let t = (f64::from_bits(b & !0x1FFF_FFFF) as f32).to_bits();
    let r = if mag < MIN_NORMAL { sign } else { t };
    f32::from_bits(if mag >= OVERFLOW { sign | 0x7F7F_FFFF } else { r })
}

/// Conditions a value the way the FPU does before computing with it: a
/// denormal becomes a signed zero and an exponent-255 pattern saturates to
/// ±`f32::MAX`.
///
/// The second half is measured: `VIWalkControl::InitIntermediate` divides by a
/// value built from `powf(0, -0.032)`, which returns `0xFF800000`, and the
/// console answers 31.25 exactly (`0x41FA0000`) only if that pattern is read
/// as -Fmax rather than -2^128.
#[inline(always)]
pub fn operand(f: f32) -> f32 {
    let b = f.to_bits();
    let sign = b & 0x8000_0000;
    let e = b & 0x7F80_0000;
    let r = if e == 0 { sign } else { b };
    f32::from_bits(if e == 0x7F80_0000 { sign | 0x7F7F_FFFF } else { r })
}

/// EE `mul.s`. The f64 product of two f32s is exact, so the chop is the only
/// rounding.
#[inline(always)]
pub fn mul(a: f32, b: f32) -> f32 {
    chop(operand(a) as f64 * operand(b) as f64)
}

/// EE `add.s`: the smaller operand aligned as the adder aligns it (`align`),
/// then the sum rounded toward zero.
///
/// After the alignment the smaller operand's lowest bit is half a step of the
/// larger's at least (or it is a zero), so the f64 sum is exact (27 bits at
/// most) and its chop is the answer.
#[inline(always)]
pub fn add(a: f32, b: f32) -> f32 {
    let (a, b) = align(operand(a).to_bits(), operand(b).to_bits());
    chop(f32::from_bits(a) as f64 + f32::from_bits(b) as f64)
}

/// The adder's alignment, as PCSX2's recompiler models the EE's
/// (`FPU_ADD_SUB`, "the PS2 uses a single guard bit"): of the operand `d`
/// exponents below the other, the bits that shifting it `d` places would
/// take past the one guard bit are cleared, and 25 or more below, all but
/// its sign. The operands are conditioned already.
#[inline(always)]
fn align(a: u32, b: u32) -> (u32, u32) {
    let d = ((a >> 23) & 0xFF) as i32 - ((b >> 23) & 0xFF) as i32;
    (a & keep(-d), b & keep(d))
}

/// The mask `align` keeps of an operand `d` exponents below the other: all of
/// it within one exponent, the bits above the lowest `d - 1` up to 24 below,
/// the sign alone past that.
#[inline(always)]
fn keep(d: i32) -> u32 {
    let s = if d > 24 { 31 } else if d > 1 { d - 1 } else { 0 };
    u32::MAX << s
}

/// EE `sub.s`: `add` with the second operand's sign flipped (conditioning
/// and the alignment keep the sign, so the two orders agree).
#[inline(always)]
pub fn sub(a: f32, b: f32) -> f32 {
    add(a, neg(b))
}

/// EE `div.s`, and VU0's `VDIV`, which agree.
///
/// Division by zero saturates to ±`f32::MAX` with the combined sign, and 0/0
/// does not produce a NaN (measured: `VIMatrix44::InvertAffine` on a singular
/// matrix divides by -0.0 and the console builds rows from -3.4028235e+38).
/// The f64 quotient is within half an f64 step of the exact one and can sit on
/// an f32 only when it is exact, so its chop is the exact quotient's.
#[inline]
pub fn div(a: f32, b: f32) -> f32 {
    let (a, b) = (operand(a), operand(b));
    if b == 0.0 {
        return if a.is_sign_negative() != b.is_sign_negative() { -f32::MAX } else { f32::MAX };
    }
    chop(a as f64 / b as f64)
}

/// EE `sqrt.s`. The engine also calls newlib's `sqrtf`, which rounds to
/// nearest; the two are different functions and a port must use the one the
/// guest called. A negative operand answers NaN here, for the caller's retest.
#[inline]
pub fn sqrt(a: f32) -> f32 {
    let a = operand(a);
    if a < 0.0 {
        return f32::NAN;
    }
    chop((a as f64).sqrt())
}

/// Four [`mul`]s at once, lane by lane: the same four answers, with SSE2 on
/// x86_64 (two f64 lanes a register) and one at a time elsewhere.
#[inline(always)]
pub fn mul4(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: SSE2 is part of the x86_64 baseline.
    return unsafe { simd::mul4(a, b) };
    #[cfg(not(target_arch = "x86_64"))]
    [mul(a[0], b[0]), mul(a[1], b[1]), mul(a[2], b[2]), mul(a[3], b[3])]
}

/// Four [`add`]s at once, as [`mul4`].
#[inline(always)]
pub fn add4(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: SSE2 is part of the x86_64 baseline.
    return unsafe { simd::add4(a, b) };
    #[cfg(not(target_arch = "x86_64"))]
    [add(a[0], b[0]), add(a[1], b[1]), add(a[2], b[2]), add(a[3], b[3])]
}

/// `mul`, `add` and their helpers on SSE2 registers, step for step.
#[cfg(target_arch = "x86_64")]
mod simd {
    use super::{MIN_NORMAL, OVERFLOW};
    use std::arch::x86_64::*;

    /// `m ? x : y`, lane by lane (each lane of `m` all ones or all zeros).
    #[inline(always)]
    unsafe fn select(m: __m128i, x: __m128i, y: __m128i) -> __m128i {
        unsafe { _mm_or_si128(_mm_and_si128(m, x), _mm_andnot_si128(m, y)) }
    }

    /// `operand` on four lanes.
    #[inline(always)]
    unsafe fn operand4(v: [f32; 4]) -> __m128i {
        unsafe {
            let b = _mm_castps_si128(_mm_loadu_ps(v.as_ptr()));
            let e = _mm_and_si128(b, _mm_set1_epi32(0x7F80_0000));
            let sign = _mm_and_si128(b, _mm_set1_epi32(i32::MIN));
            let r = select(_mm_cmpeq_epi32(e, _mm_setzero_si128()), sign, b);
            select(_mm_cmpeq_epi32(e, _mm_set1_epi32(0x7F80_0000)), _mm_or_si128(sign, _mm_set1_epi32(0x7F7F_FFFF)), r)
        }
    }

    /// `chop` on two f64 lanes (never NaN), the two f32 in the low half.
    #[inline(always)]
    unsafe fn chop2(x: __m128d) -> __m128i {
        unsafe {
            let abs = _mm_and_pd(x, _mm_castsi128_pd(_mm_set1_epi64x(0x7FFF_FFFF_FFFF_FFFF)));
            let tiny = _mm_castpd_si128(_mm_cmplt_pd(abs, _mm_set1_pd(f64::from_bits(MIN_NORMAL))));
            let huge = _mm_castpd_si128(_mm_cmpge_pd(abs, _mm_set1_pd(f64::from_bits(OVERFLOW))));
            let t = _mm_castps_si128(_mm_cvtpd_ps(_mm_and_pd(x, _mm_castsi128_pd(_mm_set1_epi64x(!0x1FFF_FFFF)))));
            // each 64-bit mask's low dword, packed below like t's two floats
            let tiny = _mm_shuffle_epi32(tiny, 0b00_00_10_00);
            let huge = _mm_shuffle_epi32(huge, 0b00_00_10_00);
            let sign = _mm_and_si128(t, _mm_set1_epi32(i32::MIN));
            let r = select(tiny, sign, t);
            select(huge, _mm_or_si128(sign, _mm_set1_epi32(0x7F7F_FFFF)), r)
        }
    }

    /// The low two lanes and the high two of `v` as f64 pairs.
    #[inline(always)]
    unsafe fn widen(v: __m128i) -> (__m128d, __m128d) {
        unsafe {
            let f = _mm_castsi128_ps(v);
            (_mm_cvtps_pd(f), _mm_cvtps_pd(_mm_movehl_ps(f, f)))
        }
    }

    #[inline(always)]
    unsafe fn store(lo: __m128i, hi: __m128i) -> [f32; 4] {
        let mut out = [0f32; 4];
        unsafe { _mm_storeu_ps(out.as_mut_ptr(), _mm_castsi128_ps(_mm_unpacklo_epi64(lo, hi))) };
        out
    }

    #[inline(always)]
    pub unsafe fn mul4(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        unsafe {
            let ((a0, a1), (b0, b1)) = (widen(operand4(a)), widen(operand4(b)));
            store(chop2(_mm_mul_pd(a0, b0)), chop2(_mm_mul_pd(a1, b1)))
        }
    }

    /// `keep` on four lanes: `u32::MAX << s` as `-(2^s)`, 2^s converted
    /// from the float whose exponent is s (2^31 is past i32's range and
    /// converts to 0x80000000, which negates to itself: the sign alone).
    #[inline(always)]
    unsafe fn keep4(d: __m128i) -> __m128i {
        unsafe {
            let one = _mm_set1_epi32(1);
            let s = _mm_and_si128(_mm_sub_epi32(d, one), _mm_cmpgt_epi32(d, one));
            let s = select(_mm_cmpgt_epi32(d, _mm_set1_epi32(24)), _mm_set1_epi32(31), s);
            let p = _mm_cvttps_epi32(_mm_castsi128_ps(_mm_slli_epi32(_mm_add_epi32(s, _mm_set1_epi32(127)), 23)));
            _mm_sub_epi32(_mm_setzero_si128(), p)
        }
    }

    /// `add` on four lanes: `align`, then the exact sum chopped.
    #[inline(always)]
    pub unsafe fn add4(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        unsafe {
            let (a, b) = (operand4(a), operand4(b));
            let exp = |v| _mm_srli_epi32(_mm_and_si128(v, _mm_set1_epi32(0x7F80_0000)), 23);
            let d = _mm_sub_epi32(exp(a), exp(b));
            let a = _mm_and_si128(a, keep4(_mm_sub_epi32(_mm_setzero_si128(), d)));
            let b = _mm_and_si128(b, keep4(d));
            let ((a0, a1), (b0, b1)) = (widen(a), widen(b));
            store(chop2(_mm_add_pd(a0, b0)), chop2(_mm_add_pd(a1, b1)))
        }
    }
}

/// Negation is a sign flip in a register (`neg.s`), with no conditioning.
#[inline(always)]
pub fn neg(a: f32) -> f32 {
    f32::from_bits(a.to_bits() ^ 0x8000_0000)
}

/// EE `c.lt.s`: the operands are conditioned first.
#[inline(always)]
pub fn lt(a: f32, b: f32) -> bool {
    operand(a) < operand(b)
}

/// EE `c.le.s`.
#[inline(always)]
pub fn le(a: f32, b: f32) -> bool {
    operand(a) <= operand(b)
}

/// EE `c.eq.s`. There is no `c.ne.s`; the guest writes `!eq`.
#[inline(always)]
pub fn eq(a: f32, b: f32) -> bool {
    operand(a) == operand(b)
}

#[cfg(test)]
mod tests {
    //! Values read off a live console (`go-eqoa-vi/vimath/golden_test.go`).
    //! If someone "simplifies" this crate back to host f32, these fail.
    use super::*;

    /// `VIVect3::Normalize` (SUPPORT 0x003FA928), written out with the guest's
    /// operation order.
    fn normalize(v: [f32; 3]) -> ([f32; 3], i32) {
        let len_sq = add(add(mul(v[0], v[0]), mul(v[1], v[1])), mul(v[2], v[2]));
        if len_sq == 0.0 {
            return (v, 0);
        }
        let inv = div(1.0, sqrt(len_sq));
        ([mul(v[0], inv), mul(v[1], inv), mul(v[2], inv)], 1)
    }

    fn bits(v: [f32; 3]) -> [u32; 3] {
        v.map(f32::to_bits)
    }

    #[test]
    fn normalize_matches_console() {
        // Nearest-even gives 3F19999A / 3F4CCCCD, one step higher on both.
        let (v, r) = normalize([3.0, 4.0, 0.0]);
        assert_eq!(r, 1);
        assert_eq!(bits(v), [0x3F19_9999, 0x3F4C_CCCC, 0]);
        assert_eq!(normalize([0.0; 3]).1, 0);
    }

    #[test]
    fn chop_is_not_nearest() {
        assert_eq!(div(1.0, 5.0).to_bits(), 0x3E4C_CCCC);
        assert_eq!((1.0f32 / 5.0).to_bits(), 0x3E4C_CCCD, "host premise changed");
    }

    #[test]
    fn denormals_flush() {
        let den = f32::from_bits(1);
        assert_eq!(mul(den, 1.0).to_bits(), 0);
        assert_eq!(mul(-den, 1.0).to_bits(), 0x8000_0000);
    }

    #[test]
    fn denormal_operand_flush() {
        // Flushing only the result leaves z = 1.4012986e-37; the console has +0.
        let (v, r) = normalize([f32::from_bits(0x0080_0000), 1e-8, f32::from_bits(1)]);
        assert_eq!(r, 1);
        assert_eq!(bits(v), [0x0DBE_BC21, 0x3F80_0000, 0]);
    }

    #[test]
    fn saturates_instead_of_inf() {
        assert_eq!(mul(f32::MAX, 2.0), f32::MAX);
        assert_eq!(div(1.0, -0.0), -f32::MAX);
    }

    #[test]
    fn compare_conditions_operands() {
        assert!(!lt(0.0, f32::from_bits(1)));
        assert!(eq(f32::from_bits(1), 0.0));
    }

    #[test]
    fn exponent_255_operand_is_fmax() {
        // powf(0, -0.032) hands back 0xFF800000; the console reads it as -Fmax.
        assert_eq!(operand(f32::from_bits(0xFF80_0000)), -f32::MAX);
    }

    /// The adder's guard bit, against PCSX2's interpreter (`interp`, where
    /// the tracer measured): an operand 25 or more exponents below adds only
    /// its sign; nearer, its bits past the guard are dropped before the add.
    #[test]
    fn the_adder_has_one_guard_bit() {
        // 1.0 + -5e-17: the interpreter one step below 1.0, the console 1.0
        assert_eq!(interp::add(1.0, -5e-17).to_bits(), 0x3F7F_FFFF);
        assert_eq!(add(1.0, -5e-17).to_bits(), 0x3F80_0000);
        // the slide (F3 20261020-194625): Freeport's x less a walk tick's
        // trace of velocity stays put; the interpreter's crept 2 mm a tick
        let x = 25280.5f32;
        assert_eq!(add(x, -1.0e-5), x);
        assert_eq!(sub(x, 1.0e-5), x);
        assert_eq!(interp::add(x, -1.0e-5).to_bits(), x.to_bits() - 1);
        // half a step of x or more still takes it a step (the guard bit)
        let half = f32::from_bits((x.to_bits() & 0x7F80_0000) - (24 << 23));
        assert_eq!(add(x, -half).to_bits(), x.to_bits() - 1);
        assert_eq!(add(x, -half * 0.75), x, "under half a step: 25 exponents below");
        // nearer: 1 - (1/8 + 2^-26) drops 2^-26 (3 below: two bits past the
        // guard cleared) and is 0.875; the interpreter's is a step under
        let b = -(0.125f32 + f32::from_bits(0x3280_0000));
        assert_eq!(b.to_bits() & 3, 1, "the bit to drop is there");
        assert_eq!(add(1.0, b).to_bits(), 0x3F60_0000);
        assert_eq!(interp::add(1.0, b).to_bits(), 0x3F5F_FFFF);
        // a sum that only grows never borrows: the two agree
        assert_eq!(add(1.0, -b).to_bits(), interp::add(1.0, -b).to_bits());
        // and the four-lane form is the same adder
        let s = add4([x, 1.0, 1.0, 3.0], [-1.0e-5, -5e-17, b, 0.25]);
        assert_eq!(s.map(f32::to_bits), [x.to_bits(), 0x3F80_0000, 0x3F60_0000, 3.25f32.to_bits()]);
    }
}
