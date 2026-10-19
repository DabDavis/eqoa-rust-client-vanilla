//! The float model the game logic is written against.
//!
//! Every function here is written ONCE, generic over a [`Model`]:
//!
//! - [`Ee`] is the console: the `ee` crate's chop/flush/saturate arithmetic and
//!   conditioned compares. It is what the client runs.
//! - [`Ieee`] is eqoa-port: host single precision (SSE, no contraction) and the
//!   host's libm. It exists so a test can compare this port against the decomp
//!   compiled for the host BIT FOR BIT (`tests/oracle.rs`), which proves the
//!   control flow, the constants and the operation order; the `ee` crate's own
//!   goldens prove the arithmetic.
//!
//! [`F`] wraps an `f32` so that ported code reads like the C it came from:
//! `a * b + c` evaluates left to right, one rounding per operator, and `a < b`
//! is the model's compare. Rust never fuses `a * b + c` on its own.
//!
//! Not yet: the EE's own transcendental functions. [`Ee`] takes `sinf`, `cosf`,
//! `powf`, `atan2f` and `acosf` from the host's libm on conditioned operands;
//! newlib's are ported and console-checked in `go-eqoa-vi/vimath` (trig, pow,
//! atan, acos) and are the next thing to bring over.

use std::fmt;
use std::marker::PhantomData;
use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

pub trait Model: Copy + Default + fmt::Debug + PartialEq + 'static {
    fn add(a: f32, b: f32) -> f32;
    fn sub(a: f32, b: f32) -> f32;
    fn mul(a: f32, b: f32) -> f32;
    fn div(a: f32, b: f32) -> f32;
    /// The inline square root (`sqrt.s` on the EE).
    fn sqrt(a: f32) -> f32;
    fn lt(a: f32, b: f32) -> bool;
    fn le(a: f32, b: f32) -> bool;
    fn eq(a: f32, b: f32) -> bool;
    fn sinf(a: f32) -> f32;
    fn cosf(a: f32) -> f32;
    fn powf(a: f32, b: f32) -> f32;
    fn atan2f(a: f32, b: f32) -> f32;
    fn fmodf(a: f32, b: f32) -> f32;
}

/// eqoa-port's arithmetic: IEEE single, round to nearest, the host's libm.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ieee;

impl Model for Ieee {
    fn add(a: f32, b: f32) -> f32 {
        a + b
    }
    fn sub(a: f32, b: f32) -> f32 {
        a - b
    }
    fn mul(a: f32, b: f32) -> f32 {
        a * b
    }
    fn div(a: f32, b: f32) -> f32 {
        a / b
    }
    fn sqrt(a: f32) -> f32 {
        a.sqrt()
    }
    fn lt(a: f32, b: f32) -> bool {
        a < b
    }
    fn le(a: f32, b: f32) -> bool {
        a <= b
    }
    fn eq(a: f32, b: f32) -> bool {
        a == b
    }
    fn sinf(a: f32) -> f32 {
        a.sin()
    }
    fn cosf(a: f32) -> f32 {
        a.cos()
    }
    fn powf(a: f32, b: f32) -> f32 {
        a.powf(b)
    }
    fn atan2f(a: f32, b: f32) -> f32 {
        a.atan2(b)
    }
    fn fmodf(a: f32, b: f32) -> f32 {
        a % b
    }
}

/// The console's arithmetic (crate `ee`). Transcendentals: see the module docs.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Ee;

impl Model for Ee {
    fn add(a: f32, b: f32) -> f32 {
        ee::add(a, b)
    }
    fn sub(a: f32, b: f32) -> f32 {
        ee::sub(a, b)
    }
    fn mul(a: f32, b: f32) -> f32 {
        ee::mul(a, b)
    }
    fn div(a: f32, b: f32) -> f32 {
        ee::div(a, b)
    }
    fn sqrt(a: f32) -> f32 {
        ee::sqrt(a)
    }
    fn lt(a: f32, b: f32) -> bool {
        ee::lt(a, b)
    }
    fn le(a: f32, b: f32) -> bool {
        ee::le(a, b)
    }
    fn eq(a: f32, b: f32) -> bool {
        ee::eq(a, b)
    }
    fn sinf(a: f32) -> f32 {
        ee::operand(a).sin()
    }
    fn cosf(a: f32) -> f32 {
        ee::operand(a).cos()
    }
    fn powf(a: f32, b: f32) -> f32 {
        ee::operand(a).powf(ee::operand(b))
    }
    fn atan2f(a: f32, b: f32) -> f32 {
        ee::operand(a).atan2(ee::operand(b))
    }
    fn fmodf(a: f32, b: f32) -> f32 {
        // fmod is exact, so only the conditioning differs from the host's.
        ee::operand(a) % ee::operand(b)
    }
}

/// A float computed under model `M`. Same size and bits as an `f32`.
#[repr(transparent)]
pub struct F<M: Model>(pub f32, PhantomData<M>);

impl<M: Model> F<M> {
    pub const ZERO: Self = F(0.0, PhantomData);
    pub const ONE: Self = F(1.0, PhantomData);

    pub const fn new(v: f32) -> Self {
        F(v, PhantomData)
    }
    pub fn bits(self) -> u32 {
        self.0.to_bits()
    }
    pub fn sqrt(self) -> Self {
        F::new(M::sqrt(self.0))
    }
    /// `fabsf`: a sign clear, no arithmetic.
    pub fn abs(self) -> Self {
        F::new(f32::from_bits(self.0.to_bits() & 0x7FFF_FFFF))
    }
    pub fn sinf(self) -> Self {
        F::new(M::sinf(self.0))
    }
    pub fn cosf(self) -> Self {
        F::new(M::cosf(self.0))
    }
    pub fn powf(self, e: Self) -> Self {
        F::new(M::powf(self.0, e.0))
    }
    pub fn atan2f(self, x: Self) -> Self {
        F::new(M::atan2f(self.0, x.0))
    }
    pub fn fmodf(self, d: Self) -> Self {
        F::new(M::fmodf(self.0, d.0))
    }
}

/// `VIMIN(a, b)`: `a < b ? a : b`.
pub fn vimin<M: Model>(a: F<M>, b: F<M>) -> F<M> {
    if a < b { a } else { b }
}

/// `VIMAX(a, b)`: `a > b ? a : b`.
pub fn vimax<M: Model>(a: F<M>, b: F<M>) -> F<M> {
    if a > b { a } else { b }
}

/// `VIMIN(hi, VIMAX(lo, v))`, the clamp every input method opens with.
pub fn clamp<M: Model>(v: F<M>, lo: F<M>, hi: F<M>) -> F<M> {
    vimin(hi, vimax(lo, v))
}

impl<M: Model> Clone for F<M> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<M: Model> Copy for F<M> {}
impl<M: Model> Default for F<M> {
    fn default() -> Self {
        Self::ZERO
    }
}
impl<M: Model> fmt::Debug for F<M> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl<M: Model> PartialEq for F<M> {
    fn eq(&self, o: &Self) -> bool {
        <M as Model>::eq(self.0, o.0)
    }
}
impl<M: Model> PartialOrd for F<M> {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        use std::cmp::Ordering::*;
        if <M as Model>::lt(self.0, o.0) {
            Some(Less)
        } else if <M as Model>::eq(self.0, o.0) {
            Some(Equal)
        } else if <M as Model>::lt(o.0, self.0) {
            Some(Greater)
        } else {
            None
        }
    }
    fn lt(&self, o: &Self) -> bool {
        <M as Model>::lt(self.0, o.0)
    }
    fn le(&self, o: &Self) -> bool {
        <M as Model>::le(self.0, o.0)
    }
    fn gt(&self, o: &Self) -> bool {
        <M as Model>::lt(o.0, self.0)
    }
    fn ge(&self, o: &Self) -> bool {
        <M as Model>::le(o.0, self.0)
    }
}

macro_rules! binop {
    ($tr:ident, $f:ident, $m:ident, $tra:ident, $fa:ident) => {
        impl<M: Model> $tr for F<M> {
            type Output = Self;
            fn $f(self, o: Self) -> Self {
                F::new(M::$m(self.0, o.0))
            }
        }
        impl<M: Model> $tra for F<M> {
            fn $fa(&mut self, o: Self) {
                *self = F::new(M::$m(self.0, o.0));
            }
        }
    };
}
binop!(Add, add, add, AddAssign, add_assign);
binop!(Sub, sub, sub, SubAssign, sub_assign);
binop!(Mul, mul, mul, MulAssign, mul_assign);
impl<M: Model> Div for F<M> {
    type Output = Self;
    fn div(self, o: Self) -> Self {
        F::new(M::div(self.0, o.0))
    }
}

/// `neg.s`: a sign flip, no conditioning.
impl<M: Model> Neg for F<M> {
    type Output = Self;
    fn neg(self) -> Self {
        F::new(f32::from_bits(self.0.to_bits() ^ 0x8000_0000))
    }
}

/// Shorthand for a literal.
pub const fn f<M: Model>(v: f32) -> F<M> {
    F::new(v)
}
