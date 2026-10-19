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

/// Reduces `x` to `f32` rounding toward zero, then flushes a denormal result.
/// An overflowing `x` lands on ±`f32::MAX`, which is the EE's saturation.
fn chop(x: f64) -> f32 {
    if x.is_nan() {
        return f32::NAN;
    }
    let mut f = x as f32;
    // `as f32` rounds to nearest-even; if that moved away from zero, step back.
    if (f as f64).abs() > x.abs() {
        f = next_toward_zero(f);
    }
    ftz(f)
}

/// Steps one representable value toward zero. Decrementing the bit pattern
/// works for both signs, since the magnitude lives in the low bits.
fn next_toward_zero(f: f32) -> f32 {
    let b = f.to_bits();
    if b & 0x7FFF_FFFF == 0 {
        return f;
    }
    f32::from_bits(b - 1)
}

/// Flushes a denormal to a zero of the same sign.
fn ftz(f: f32) -> f32 {
    let b = f.to_bits();
    if b & 0x7F80_0000 == 0 {
        return f32::from_bits(b & 0x8000_0000);
    }
    f
}

/// Conditions a value the way the FPU does before computing with it: a
/// denormal becomes a signed zero and an exponent-255 pattern saturates to
/// ±`f32::MAX`.
///
/// The second half is measured: `VIWalkControl::InitIntermediate` divides by a
/// value built from `powf(0, -0.032)`, which returns `0xFF800000`, and the
/// console answers 31.25 exactly (`0x41FA0000`) only if that pattern is read
/// as -Fmax rather than -2^128.
pub fn operand(f: f32) -> f32 {
    let b = f.to_bits();
    if b & 0x7F80_0000 == 0x7F80_0000 {
        return if b >> 31 != 0 { -f32::MAX } else { f32::MAX };
    }
    ftz(f)
}

/// Returns `s = fl(x + y)` and the exact error `e`, so that `s + e == x + y`.
///
/// Needed because a float64 cannot always hold the exact sum of two float32s
/// when their exponents are far apart: `1.0 + -5e-17` rounds to 1.0 in f64,
/// and the console's answer is one step below 1.0.
fn two_sum(x: f64, y: f64) -> (f64, f64) {
    let s = x + y;
    let b = s - x;
    let e = (x - (s - b)) + (y - b);
    (s, e)
}

/// Reduces the exact value `s + e` to `f32`, rounding toward zero.
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

/// EE `mul.s`. The f64 product of two f32s is exact, so the chop is the only
/// rounding.
pub fn mul(a: f32, b: f32) -> f32 {
    chop(operand(a) as f64 * operand(b) as f64)
}

/// EE `add.s`.
pub fn add(a: f32, b: f32) -> f32 {
    let (s, e) = two_sum(operand(a) as f64, operand(b) as f64);
    chop_sum(s, e)
}

/// EE `sub.s`.
pub fn sub(a: f32, b: f32) -> f32 {
    let (s, e) = two_sum(operand(a) as f64, -(operand(b) as f64));
    chop_sum(s, e)
}

/// EE `div.s`, and VU0's `VDIV`, which agree.
///
/// Division by zero saturates to ±`f32::MAX` with the combined sign, and 0/0
/// does not produce a NaN (measured: `VIMatrix44::InvertAffine` on a singular
/// matrix divides by -0.0 and the console builds rows from -3.4028235e+38).
pub fn div(a: f32, b: f32) -> f32 {
    let (a, b) = (operand(a), operand(b));
    if b == 0.0 {
        return if a.is_sign_negative() != b.is_sign_negative() { -f32::MAX } else { f32::MAX };
    }
    chop(a as f64 / b as f64)
}

/// EE `sqrt.s`. The engine also calls newlib's `sqrtf`, which rounds to
/// nearest; the two are different functions and a port must use the one the
/// guest called.
pub fn sqrt(a: f32) -> f32 {
    chop((operand(a) as f64).sqrt())
}

/// Negation is a sign flip in a register (`neg.s`), with no conditioning.
pub fn neg(a: f32) -> f32 {
    f32::from_bits(a.to_bits() ^ 0x8000_0000)
}

/// EE `c.lt.s`: the operands are conditioned first.
pub fn lt(a: f32, b: f32) -> bool {
    operand(a) < operand(b)
}

/// EE `c.le.s`.
pub fn le(a: f32, b: f32) -> bool {
    operand(a) <= operand(b)
}

/// EE `c.eq.s`. There is no `c.ne.s`; the guest writes `!eq`.
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
