//! PCSX2's INTERPRETER's `add.s` and `sub.s`, and newlib's functions over
//! them: the arithmetic the tracer's `call` runs guest code in
//! (`intRunGuestCall`), so the arithmetic every go-eqoa-vi golden was
//! measured in. Its `ADD_S` is the host's float addition under the EE's
//! rounding (the exact sum toward zero), where the console's adder, as
//! PCSX2's recompiler has it (`FPU_ADD_SUB`), drops the smaller operand's
//! bits past one guard bit first (the crate root's `add`). The two differ
//! only on such bits: `1.0 + -5e-17` is one step below 1.0 here and 1.0 on
//! the console, as is `cosf(pi)`'s `-1`.
//!
//! The game runs on the crate root's. These stay to check the ports that were
//! checked against the interpreter (`libm` against go-eqoa-vi's corpus).

use crate::{chop, neg, operand};

/// The interpreter's `ADD_S`: the exact sum, rounded toward zero.
///
/// The f64 sum `s` misses the exact one by less than half an f64 step, and
/// every f32 is an f64, so unless `s` lands on an f32 the exact sum lies
/// between the same two f32s and chops as `s` does. An inexact `s` lands on
/// one only when the smaller operand is at most half an f64 step of the
/// larger, 53 or more exponents below it (`1.0 + -5e-17` rounds to 1.0 in
/// f64, and the interpreter's answer is one step below 1.0): `add_far`
/// settles those by the error of the sum. Nearer than that, or with a zero,
/// `s` will do.
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

/// The interpreter's `SUB_S`: `add` with the second operand's sign flipped.
#[inline(always)]
pub fn sub(a: f32, b: f32) -> f32 {
    add(a, neg(b))
}

/// What `libm` computes with.
mod arith {
    pub use super::{add, sub};
    pub use crate::{div, mul, operand};
}

/// newlib's functions over the interpreter's adder: go-eqoa-vi's ports as
/// they were checked (`tests/libm.rs`). The same file as `crate::libm`, on
/// purpose: one source, two adders.
#[allow(clippy::duplicate_mod)]
#[path = "libm.rs"]
pub mod libm;
