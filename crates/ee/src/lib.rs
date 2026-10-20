//! The Emotion Engine's FPU, as the game sees it.
//!
//! Ported from `go-eqoa-vi/vimath/ee.go`, where every rule below was measured
//! on a running console through the tracer, not read out of a manual. The EE
//! FPU is not IEEE-754:
//!
//! - it rounds toward zero, always (no rounding-mode control);
//! - it flushes denormals to a signed zero, on the way IN as well as out;
//! - it saturates to ±`f32::MAX` instead of producing an infinity, and an
//!   exponent-255 operand is clamped to ±`f32::MAX` before it is used;
//! - a comparison conditions its operands too, so `0 < 1.4e-45` is false.
//!
//! Host `f32` arithmetic rounds to nearest-even and disagrees by one step on
//! most inexact results. Game logic whose feel has to match the console
//! (walking, camera, collision, animation) goes through these functions;
//! rendering does not need to.
//!
//! Rust never contracts `a * b + c` into one fused rounding on its own, so the
//! order of operations written here is the order the guest executes.

pub mod libm;

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

/// EE `add.s`: the exact sum, rounded toward zero.
///
/// The f64 sum `s` misses the exact one by less than half an f64 step, and
/// every f32 is an f64, so unless `s` lands on an f32 the exact sum lies
/// between the same two f32s and chops as `s` does. An inexact `s` lands on
/// one only when the smaller operand is at most half an f64 step of the
/// larger, 53 or more exponents below it (`1.0 + -5e-17` rounds to 1.0 in
/// f64, and the console's answer is one step below 1.0): `add_far` settles
/// those by the error of the sum. Nearer than that, or with a zero, `s` will do.
#[inline(always)]
pub fn add(a: f32, b: f32) -> f32 {
    let (a, b) = (operand(a), operand(b));
    let (ea, eb) = ((a.to_bits() >> 23) & 0xFF, (b.to_bits() >> 23) & 0xFF);
    if ea.abs_diff(eb) > 52 && ea != 0 && eb != 0 {
        return add_far(a, b);
    }
    chop(a as f64 + b as f64)
}

/// `add` of conditioned operands 53 or more exponents apart: when the f64 sum
/// lands on an f32, the error of the sum (`two_sum`) tells which side of it the
/// exact sum was.
#[inline(never)]
fn add_far(a: f32, b: f32) -> f32 {
    let (x, y) = (a as f64, b as f64);
    let s = x + y;
    let bb = s - x;
    let e = (x - (s - bb)) + (y - bb);
    let r = chop(s);
    if e == 0.0 || r == 0.0 {
        return r;
    }
    let rem = (s - r as f64) + e;
    if (r > 0.0 && rem < 0.0) || (r < 0.0 && rem > 0.0) {
        // one step toward zero, which in the bit pattern is down for both
        // signs; flushed if that left the normal range
        let b = r.to_bits() - 1;
        return f32::from_bits(if b & 0x7F80_0000 == 0 { b & 0x8000_0000 } else { b });
    }
    r
}

/// EE `sub.s`: `add` with the second operand's sign flipped (conditioning
/// keeps the sign, so the two orders agree).
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

/// Four [`add`]s at once, as [`mul4`]; a lane whose operands are 53 or more
/// exponents apart sends all four through `add`.
#[inline(always)]
pub fn add4(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: SSE2 is part of the x86_64 baseline.
    if let Some(r) = unsafe { simd::add4(a, b) } {
        return r;
    }
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

    /// `None` when a lane's operands are 53 or more exponents apart, both
    /// nonzero (`add_far`'s).
    #[inline(always)]
    pub unsafe fn add4(a: [f32; 4], b: [f32; 4]) -> Option<[f32; 4]> {
        unsafe {
            let (a, b) = (operand4(a), operand4(b));
            let exp = |v| _mm_srli_epi32(_mm_and_si128(v, _mm_set1_epi32(0x7F80_0000)), 23);
            let (ea, eb) = (exp(a), exp(b));
            let far = _mm_or_si128(
                _mm_cmpgt_epi32(_mm_sub_epi32(ea, eb), _mm_set1_epi32(52)),
                _mm_cmpgt_epi32(_mm_sub_epi32(eb, ea), _mm_set1_epi32(52)),
            );
            let zero = _mm_or_si128(_mm_cmpeq_epi32(ea, _mm_setzero_si128()), _mm_cmpeq_epi32(eb, _mm_setzero_si128()));
            if _mm_movemask_epi8(_mm_andnot_si128(zero, far)) != 0 {
                return None;
            }
            let ((a0, a1), (b0, b1)) = (widen(a), widen(b));
            Some(store(chop2(_mm_add_pd(a0, b0)), chop2(_mm_add_pd(a1, b1))))
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

    #[test]
    fn sum_needs_the_error_term() {
        // 1.0 + -5e-17 is one step below 1.0 on the console.
        assert_eq!(add(1.0, -5e-17).to_bits(), 0x3F7F_FFFF);
    }
}
