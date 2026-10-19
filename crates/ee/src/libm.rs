//! The newlib maths the engine calls, as the PS2's SUPPORT has it compiled:
//! `sinf`, `cosf`, `powf`, `atanf`, `atan2f`, `acosf`, `sqrtf`, `fmodf`.
//!
//! Ported from `go-eqoa-vi/vimath` (`trig.go`, `pow.go`, `atan.go`,
//! `acos.go`, `fmod.go`), where each was checked against the running console
//! through the tracer; the comments there are the long form of every quirk
//! kept here. The arithmetic is the EE's (crate root: chop, flush, saturate),
//! and the constants are the guest's bit patterns, read out of its code and
//! data, not decimal literals:
//!
//! - `pi` in atan2/acos is one ULP BELOW the nearest float, the remainder
//!   carried in `pi_lo`; `rem_pio2f` uses newlib's FLOAT split of pi/2.
//! - `_LIB_VERSION` is `_XOPEN_`, so the `powf` and `atan2f` WRAPPERS change
//!   values: `powf(0, y<0)` is -HUGE_VAL, an underflow is +0.0, and
//!   `atan2f(±0, ±0)` is +0.0.
//! - `powf`'s |y| > 2^27 overflow returns a constant the host compiler folded,
//!   0x7F800000: an infinity no EE operation could compute.
//! - `sqrtf` is the integer algorithm and rounds to NEAREST, unlike `sqrt.s`.
//!
//! Not ported (as in the Go): `sinf`/`cosf`'s huge-argument reduction
//! (|x| > 2^7·π/2, `__kernel_rem_pio2f`), where these fall back to the host's
//! functions; `acosf`'s |x| > 1 wrapper path; the wrappers' NaN arms.

// newlib writes the overflow and underflow tests as separate branches with the
// same return; they stay separate here so they read against the source.
#![allow(clippy::if_same_then_else)]

use crate::{add, div, mul, operand, sub};

fn bits(f: f32) -> i32 {
    f.to_bits() as i32
}
fn from(b: i32) -> f32 {
    f32::from_bits(b as u32)
}
fn c(b: u32) -> f32 {
    f32::from_bits(b)
}
/// `neg.s` / a sign-bit flip.
fn neg(f: f32) -> f32 {
    f32::from_bits(f.to_bits() ^ 0x8000_0000)
}
fn absf(f: f32) -> f32 {
    f32::from_bits(f.to_bits() & 0x7FFF_FFFF)
}
/// `c.lt.s` / `c.eq.s`, operands conditioned.
fn lt(a: f32, b: f32) -> bool {
    crate::lt(a, b)
}
fn le(a: f32, b: f32) -> bool {
    crate::le(a, b)
}
fn eq(a: f32, b: f32) -> bool {
    crate::eq(a, b)
}

// ---------------------------------------------------------------- sinf / cosf

const K_HALF: u32 = 0x3F00_0000;
const K_S: [u32; 6] = [0xBE2A_AAAB, 0x3C08_8889, 0xB950_0D01, 0x3638_EF1B, 0xB2D7_2F34, 0x2F2E_C9D3];
const K_ONE: u32 = 0x3F80_0000;
const K_C: [u32; 6] = [0x3D2A_AAAB, 0xBAB6_0B61, 0x37D0_0D01, 0xB493_F27C, 0x310F_74F6, 0xAD47_D74E];
const R_INV_PIO2: u32 = 0x3F22_F984;
const R_PIO2_1: u32 = 0x3FC9_0F80;
const R_PIO2_1T: u32 = 0x3735_4443;
const R_PIO2_2: u32 = 0x3735_4400;
const R_PIO2_2T: u32 = 0x2E85_A308;
const R_PIO2_3: u32 = 0x2E85_A300;
const R_PIO2_3T: u32 = 0x248D_3132;

fn kernel_sinf(x: f32, y: f32, iy: i32) -> f32 {
    let ix = x.to_bits() & 0x7FFF_FFFF;
    if ix < 0x3200_0000 && x as i32 == 0 {
        return x;
    }
    let s = K_S.map(c);
    let z = mul(x, x);
    let v = mul(z, x);
    let r = add(s[1], mul(z, add(s[2], mul(z, add(s[3], mul(z, add(s[4], mul(z, s[5]))))))));
    if iy == 0 {
        return add(x, mul(v, add(s[0], mul(z, r))));
    }
    sub(x, sub(sub(mul(z, sub(mul(c(K_HALF), y), mul(v, r))), y), mul(v, s[0])))
}

fn kernel_cosf(x: f32, y: f32) -> f32 {
    let ix = x.to_bits() & 0x7FFF_FFFF;
    if ix < 0x3200_0000 && x as i32 == 0 {
        return c(K_ONE);
    }
    let k = K_C.map(c);
    let z = mul(x, x);
    let r = mul(z, add(k[0], mul(z, add(k[1], mul(z, add(k[2], mul(z, add(k[3], mul(z, add(k[4], mul(z, k[5])))))))))));
    if ix < 0x3E99_999A {
        return sub(c(K_ONE), sub(mul(c(K_HALF), z), sub(mul(z, r), mul(x, y))));
    }
    let qx = if ix > 0x3F48_0000 { c(0x3E90_0000) } else { f32::from_bits(ix - 0x0100_0000) };
    let hz = sub(mul(c(K_HALF), z), qx);
    let a = sub(c(K_ONE), qx);
    sub(a, sub(hz, sub(mul(z, r), mul(x, y))))
}

/// `__ieee754_rem_pio2f`, fast and medium paths; `None` for the huge one.
fn rem_pio2f(x: f32) -> Option<(i32, f32, f32)> {
    let hx = x.to_bits();
    let ix = hx & 0x7FFF_FFFF;
    let neg_x = hx & 0x8000_0000 != 0;
    if ix <= 0x3F49_0FD8 {
        return Some((0, x, 0.0));
    }
    if ix < 0x4016_CBE4 {
        let (y0, y1);
        if !neg_x {
            let mut z = sub(x, c(R_PIO2_1));
            if ix & 0xFFFF_FFF0 != 0x3FC9_0FD0 {
                y0 = sub(z, c(R_PIO2_1T));
                y1 = sub(sub(z, y0), c(R_PIO2_1T));
            } else {
                z = sub(z, c(R_PIO2_2));
                y0 = sub(z, c(R_PIO2_2T));
                y1 = sub(sub(z, y0), c(R_PIO2_2T));
            }
            return Some((1, y0, y1));
        }
        let mut z = add(x, c(R_PIO2_1));
        if ix & 0xFFFF_FFF0 != 0x3FC9_0FD0 {
            y0 = add(z, c(R_PIO2_1T));
            y1 = add(sub(z, y0), c(R_PIO2_1T));
        } else {
            z = add(z, c(R_PIO2_2));
            y0 = add(z, c(R_PIO2_2T));
            y1 = add(sub(z, y0), c(R_PIO2_2T));
        }
        return Some((-1, y0, y1));
    }
    if ix > 0x4349_0F80 {
        return None;
    }
    let t = f32::from_bits(ix);
    let n = add(mul(t, c(R_INV_PIO2)), c(K_HALF)) as i32;
    let fnn = n as f32;
    let mut r = sub(t, mul(fnn, c(R_PIO2_1)));
    let mut w = mul(fnn, c(R_PIO2_1T));
    // No npio2_hw shortcut in this build: always refine (245/245 on console).
    let j = (ix >> 23) as i32;
    let mut y0 = sub(r, w);
    let mut i = j - ((y0.to_bits() >> 23) & 0xFF) as i32;
    if i > 8 {
        let tt = r;
        w = mul(fnn, c(R_PIO2_2));
        r = sub(tt, w);
        w = sub(mul(fnn, c(R_PIO2_2T)), sub(sub(tt, r), w));
        y0 = sub(r, w);
        i = j - ((y0.to_bits() >> 23) & 0xFF) as i32;
        if i > 25 {
            let tt = r;
            w = mul(fnn, c(R_PIO2_3));
            r = sub(tt, w);
            w = sub(mul(fnn, c(R_PIO2_3T)), sub(sub(tt, r), w));
            y0 = sub(r, w);
        }
    }
    let y1 = sub(sub(r, y0), w);
    Some(if neg_x { (-n, neg(y0), neg(y1)) } else { (n, y0, y1) })
}

/// Whether `x` is in the unported huge-argument range of `sinf`/`cosf`.
pub fn huge_angle(x: f32) -> bool {
    let ix = x.to_bits() & 0x7FFF_FFFF;
    ix > 0x4349_0F80 && ix < 0x7F80_0000
}

/// newlib `sinf` (SUPPORT 0x00126470).
pub fn sinf(x: f32) -> f32 {
    let ix = x.to_bits() & 0x7FFF_FFFF;
    if ix <= 0x3F49_0FD8 {
        return kernel_sinf(x, 0.0, 0);
    }
    if ix >= 0x7F80_0000 {
        return sub(x, x);
    }
    let Some((n, y0, y1)) = rem_pio2f(x) else { return operand(x).sin() };
    match n & 3 {
        0 => kernel_sinf(y0, y1, 1),
        1 => kernel_cosf(y0, y1),
        2 => neg(kernel_sinf(y0, y1, 1)),
        _ => neg(kernel_cosf(y0, y1)),
    }
}

/// newlib `cosf` (SUPPORT 0x00126368).
pub fn cosf(x: f32) -> f32 {
    let ix = x.to_bits() & 0x7FFF_FFFF;
    if ix <= 0x3F49_0FD8 {
        return kernel_cosf(x, 0.0);
    }
    if ix >= 0x7F80_0000 {
        return sub(x, x);
    }
    let Some((n, y0, y1)) = rem_pio2f(x) else { return operand(x).cos() };
    match n & 3 {
        0 => kernel_cosf(y0, y1),
        1 => neg(kernel_sinf(y0, y1, 1)),
        2 => neg(kernel_cosf(y0, y1)),
        _ => kernel_sinf(y0, y1, 1),
    }
}

// ----------------------------------------------------------------- sqrtf

/// newlib `__ieee754_sqrtf` (0x0012B3F0): rounds to NEAREST (an integer
/// algorithm), unlike `sqrt.s`. Exponent 255 is `x*x + x` and a negative is
/// `(x-x)/(x-x)`, both EE arithmetic, so both saturate.
pub fn sqrtf(x: f32) -> f32 {
    let b = x.to_bits();
    if b & 0x7F80_0000 == 0x7F80_0000 {
        return add(mul(x, x), x);
    }
    if b & 0x7FFF_FFFF == 0 {
        return x;
    }
    if (b as i32) < 0 {
        let d = sub(x, x);
        return div(d, d);
    }
    (x as f64).sqrt() as f32
}

/// newlib `fmodf` (0x001271E8). Exact, so the f64 route is the same number.
pub fn fmodf(x: f32, y: f32) -> f32 {
    ((x as f64) % (y as f64)) as f32
}

// ------------------------------------------------------------- atanf / atan2f

const ATAN_HI: [u32; 4] = [0x3EED_6338, 0x3F49_0FDA, 0x3F7B_985E, 0x3FC9_0FDA];
const ATAN_LO: [u32; 4] = [0x31AC_3769, 0x3322_2168, 0x3314_0FB4, 0x33A2_2168];
const AT: [u32; 11] = [
    0x3EAA_AAAB, 0xBE4C_CCCD, 0x3E12_4925, 0xBDE3_8E38, 0x3DBA_2E6E, 0xBD9D_8795, 0x3D88_6B35, 0xBD6E_F16B, 0x3D4B_DA59,
    0xBD15_A221, 0x3C85_69D7,
];
const ATAN_HUGE: u32 = 0x7149_F2CA;
const ATAN2_PIO4: u32 = 0x3F49_0FDB;
const ATAN2_PIO2: u32 = 0x3FC9_0FDB;
const ATAN2_PI3O4: u32 = 0x4016_CBE4;
const ATAN2_PI: u32 = 0x4049_0FDA;
const ATAN2_PI_LO: u32 = 0x3422_2168;
const ATAN2_PIO2_HI: u32 = 0x3FC9_0FDC;

/// newlib `atanf` (0x001260C0).
pub fn atanf(x: f32) -> f32 {
    let hx = bits(x);
    let ix = hx & 0x7FFF_FFFF;
    if ix >= 0x5080_0000 {
        if ix > 0x7F80_0000 {
            return add(x, x);
        }
        return if hx > 0 { add(c(ATAN_HI[3]), c(ATAN_LO[3])) } else { sub(neg(c(ATAN_HI[3])), c(ATAN_LO[3])) };
    }
    let mut x = x;
    let id: i32;
    if ix < 0x3EE0_0000 {
        if ix < 0x3100_0000 && lt(1.0, add(c(ATAN_HUGE), x)) {
            return x;
        }
        id = -1;
    } else {
        x = absf(x);
        if ix < 0x3F30_0000 {
            id = 0;
            x = div(sub(add(x, x), 1.0), add(2.0, x));
        } else if ix < 0x3F98_0000 {
            id = 1;
            x = div(sub(x, 1.0), add(x, 1.0));
        } else if ix < 0x401C_0000 {
            id = 2;
            x = div(sub(x, 1.5), add(mul(x, 1.5), 1.0));
        } else {
            id = 3;
            x = div(-1.0, x);
        }
    }
    let a = AT.map(c);
    let z = mul(x, x);
    let w = mul(z, z);
    let mut even = add(a[8], mul(w, a[10]));
    let mut odd = add(a[7], mul(w, a[9]));
    even = add(a[6], mul(w, even));
    odd = add(a[5], mul(w, odd));
    even = add(a[4], mul(w, even));
    odd = add(a[3], mul(w, odd));
    even = add(a[2], mul(w, even));
    odd = add(a[1], mul(w, odd));
    even = add(a[0], mul(w, even));
    let s1 = mul(z, even);
    let s2 = mul(w, odd);
    if id < 0 {
        return sub(x, mul(x, add(s1, s2)));
    }
    let id = id as usize;
    let mut r = mul(x, add(s1, s2));
    r = sub(r, c(ATAN_LO[id]));
    r = sub(r, x);
    let z = sub(c(ATAN_HI[id]), r);
    if hx < 0 { neg(z) } else { z }
}

/// newlib `atan2f` (0x001270C0), the WRAPPER every caller reaches: the kernel's
/// answer, except that both arguments zero (an EE `c.eq.s`, so denormals count)
/// answers +0.0.
pub fn atan2f(y: f32, x: f32) -> f32 {
    let z = atan2f_ieee(y, x);
    if x.is_nan() || y.is_nan() {
        return z;
    }
    if eq(y, 0.0) && eq(x, 0.0) {
        return 0.0;
    }
    z
}

/// newlib `__ieee754_atan2f` (0x0012A248).
pub fn atan2f_ieee(y: f32, x: f32) -> f32 {
    let hx = bits(x);
    let ix = hx & 0x7FFF_FFFF;
    let hy = bits(y);
    let iy = hy & 0x7FFF_FFFF;
    if ix > 0x7F80_0000 || iy > 0x7F80_0000 {
        return add(x, y);
    }
    if hx == 0x3F80_0000 {
        return atanf(y);
    }
    let m = ((hy >> 31) & 1) | ((hx >> 30) & 2);
    if iy == 0 {
        return match m {
            0 | 1 => y,
            2 => c(ATAN2_PI),
            _ => neg(c(ATAN2_PI)),
        };
    }
    if ix == 0 {
        return if hy < 0 { neg(c(ATAN2_PIO2)) } else { c(ATAN2_PIO2) };
    }
    if ix == 0x7F80_0000 {
        if iy == 0x7F80_0000 {
            return match m {
                0 => c(ATAN2_PIO4),
                1 => neg(c(ATAN2_PIO4)),
                2 => c(ATAN2_PI3O4),
                _ => neg(c(ATAN2_PI3O4)),
            };
        }
        return match m {
            0 => 0.0,
            1 => neg(0.0),
            2 => c(ATAN2_PI),
            _ => neg(c(ATAN2_PI)),
        };
    }
    if iy == 0x7F80_0000 {
        return if hy < 0 { neg(c(ATAN2_PIO2)) } else { c(ATAN2_PIO2) };
    }
    let k = (iy - ix) >> 23;
    let z = if k > 60 {
        c(ATAN2_PIO2_HI)
    } else if hx < 0 && k < -60 {
        0.0
    } else {
        atanf(absf(div(y, x)))
    };
    match m {
        0 => z,
        1 => neg(z),
        2 => sub(c(ATAN2_PI), sub(z, c(ATAN2_PI_LO))),
        _ => sub(sub(z, c(ATAN2_PI_LO)), c(ATAN2_PI)),
    }
}

// ------------------------------------------------------------------ acosf

const ACOS_PS: [u32; 6] = [0x3E2A_AAAB, 0xBEA6_B090, 0x3E4E_0AA8, 0xBD24_1146, 0x3A4F_7F04, 0x3811_EF08];
const ACOS_QS: [u32; 4] = [0xC019_D139, 0x4001_572D, 0xBF30_3361, 0x3D9D_C62E];
const ACOS_PIO2_HI: u32 = 0x3FC9_0FDA;
const ACOS_PIO2_LO: u32 = 0x33A2_2168;
const ACOS_PI: u32 = 0x4049_0FDA;
const ACOS_PI_EXACT: u32 = 0x4049_0FDB;
const ACOS_PIO2: u32 = 0x3FC9_0FDB;

fn acos_poly(z: f32) -> f32 {
    let ps = ACOS_PS.map(c);
    let qs = ACOS_QS.map(c);
    let mut p = mul(z, ps[5]);
    for k in (0..5).rev() {
        p = add(p, ps[k]);
        p = mul(z, p);
    }
    let mut q = mul(z, qs[3]);
    for k in (0..3).rev() {
        q = add(q, qs[k]);
        q = mul(z, q);
    }
    q = add(q, 1.0);
    div(p, q)
}

/// newlib `__ieee754_acosf` (0x00129E18); `None` for |x| > 1 (the wrapper's
/// domain path, not ported).
pub fn acosf(x: f32) -> Option<f32> {
    let ix = x.to_bits() & 0x7FFF_FFFF;
    if ix == 0x3F80_0000 {
        return Some(if x.to_bits() >> 31 == 0 { 0.0 } else { c(ACOS_PI_EXACT) });
    }
    if ix > 0x3F80_0000 {
        return None;
    }
    if ix < 0x3F00_0000 {
        if ix <= 0x2300_0000 {
            return Some(c(ACOS_PIO2));
        }
        let z = mul(x, x);
        let r = acos_poly(z);
        return Some(sub(c(ACOS_PIO2_HI), sub(x, sub(c(ACOS_PIO2_LO), mul(x, r)))));
    }
    if x.to_bits() >> 31 != 0 {
        let z = mul(add(x, 1.0), 0.5);
        let r = acos_poly(z);
        let s = sqrtf(z);
        let w = sub(mul(r, s), c(ACOS_PIO2_LO));
        let t = add(s, w);
        return Some(sub(c(ACOS_PI), add(t, t)));
    }
    let z = mul(sub(1.0, x), 0.5);
    let s = sqrtf(z);
    let df = f32::from_bits(s.to_bits() & 0xFFFF_F000);
    let cc = div(sub(z, mul(df, df)), add(s, df));
    let r = acos_poly(z);
    let w = add(mul(r, s), cc);
    let t = add(df, w);
    Some(add(t, t))
}

// ------------------------------------------------------------------- powf

const POW_DPH: [u32; 2] = [0, 0x3F15_C000];
const POW_DPL: [u32; 2] = [0, 0x35D1_CFDC];
const POW_L: [u32; 6] = [0x3F19_999A, 0x3EDB_6DB7, 0x3EAA_AAAB, 0x3E8B_A305, 0x3E6C_3255, 0x3E53_F142];
const POW_P: [u32; 5] = [0x3E2A_AAAB, 0xBB36_0B61, 0x388A_B355, 0xB5DD_EA0E, 0x3331_BB4C];
const POW_LG2: u32 = 0x3F31_7218;
const POW_LG2_H: u32 = 0x3F31_7200;
const POW_LG2_L: u32 = 0x35BF_BE8C;
const POW_OVT: u32 = 0x3338_AA3C;
const POW_CP: u32 = 0x3F76_384F;
const POW_CP_H: u32 = 0x3F76_3800;
const POW_CP_L: u32 = 0x369D_C3A0;
const POW_IVLN2: u32 = 0x3FB8_AA3B;
const POW_IVLN2_H: u32 = 0x3FB8_AA00;
const POW_IVLN2_L: u32 = 0x36EC_A570;
const POW_HUGE: u32 = 0x7149_F2CA;
const POW_TINY: u32 = 0x0DA2_4260;
const POW_TWO24: u32 = 0x4B80_0000;
const POW_TWO25: u32 = 0x4C00_0000;
const POW_TWOM25: u32 = 0x3300_0000;
/// The compiler's fold of sn*huge*huge: an INFINITY, where the runtime
/// multiply saturates.
const POW_FOLDED_INF: u32 = 0x7F80_0000;

/// newlib `powf` (0x00127328): the `_XOPEN_` wrapper and the kernel. `None`
/// for an exponent-255 argument (the wrapper's NaN arms are not decoded).
pub fn powf(x: f32, y: f32) -> Option<f32> {
    if bits(x) & 0x7F80_0000 == 0x7F80_0000 || bits(y) & 0x7F80_0000 == 0x7F80_0000 {
        return None;
    }
    let z = powf_ieee(x, y);
    if eq(x, 0.0) {
        if eq(y, 0.0) {
            return Some(1.0);
        }
        if lt(y, 0.0) {
            return Some(neg(c(POW_FOLDED_INF)));
        }
        return Some(z);
    }
    if bits(z) & 0x7F80_0000 == 0x7F80_0000 {
        let mut h = c(POW_FOLDED_INF);
        let yh = y as f64 * 0.5;
        if x < 0.0 && yh.round_ties_even() != yh {
            h = neg(h);
        }
        return Some(h);
    }
    if eq(z, 0.0) {
        return Some(0.0);
    }
    Some(z)
}

/// newlib `__ieee754_powf` (0x0012A780).
pub fn powf_ieee(x: f32, y: f32) -> f32 {
    let hx = bits(x);
    let hy = bits(y);
    let mut ix = hx & 0x7FFF_FFFF;
    let iy = hy & 0x7FFF_FFFF;
    if iy == 0 {
        return 1.0;
    }
    if ix > 0x7F80_0000 || iy > 0x7F80_0000 {
        return add(x, y);
    }
    let mut yisint = 0i32;
    if hx < 0 {
        if iy >= 0x4B80_0000 {
            yisint = 2;
        } else if iy >= 0x3F80_0000 {
            let k = (iy >> 23) - 0x7F;
            let j = iy >> (23 - k);
            if j << (23 - k) == iy {
                yisint = 2 - (j & 1);
            }
        }
    }
    if iy == 0x7F80_0000 {
        if ix == 0x3F80_0000 {
            return sub(y, y);
        } else if ix > 0x3F80_0000 {
            return if hy >= 0 { y } else { 0.0 };
        }
        return if hy < 0 { neg(y) } else { 0.0 };
    }
    if iy == 0x3F80_0000 {
        return if hy < 0 { div(1.0, x) } else { x };
    }
    if hy == 0x4000_0000 {
        return mul(x, x);
    }
    if hy == 0x3F00_0000 && hx >= 0 {
        return sqrtf(x);
    }
    let mut ax = absf(x);
    if ix == 0x7F80_0000 || ix == 0 || ix == 0x3F80_0000 {
        let mut z = ax;
        if hy < 0 {
            z = div(1.0, z);
        }
        if hx < 0 {
            if (ix - 0x3F80_0000) | yisint == 0 {
                let d = sub(z, z);
                z = div(d, d);
            } else if yisint == 1 {
                z = neg(z);
            }
        }
        return z;
    }
    let mut n = ((hx as u32) >> 31) as i32 - 1;
    if n | yisint == 0 {
        let d = sub(x, x);
        return div(d, d);
    }
    let sn = if n | (yisint - 1) == 0 { -1.0f32 } else { 1.0 };
    let (t1, t2);
    if iy > 0x4D00_0000 {
        if ix < 0x3F7F_FFF8 {
            return if hy < 0 { c(POW_FOLDED_INF) } else { 0.0 };
        }
        if ix > 0x3F80_0007 {
            return if hy > 0 { c(POW_FOLDED_INF) } else { 0.0 };
        }
        // x, not ax: newlib's, and what the console does.
        let t = sub(x, 1.0);
        let w = mul(mul(t, t), sub(0.5, mul(t, sub(c(POW_L[2]), mul(t, 0.25)))));
        let u = mul(t, c(POW_IVLN2_H));
        let v = sub(mul(t, c(POW_IVLN2_L)), mul(w, c(POW_IVLN2)));
        let tt1 = from(bits(add(u, v)) & -0x1000);
        t1 = tt1;
        t2 = sub(v, sub(tt1, u));
    } else {
        n = 0;
        if ix < 0x0080_0000 {
            ax = mul(ax, c(POW_TWO24));
            n -= 24;
            ix = bits(ax);
        }
        n += (ix >> 23) - 0x7F;
        let j = ix & 0x007F_FFFF;
        ix = j | 0x3F80_0000;
        let k: i32 = if j <= 0x1C_C471 {
            0
        } else if j < 0x5D_B3D7 {
            1
        } else {
            n += 1;
            ix -= 0x0080_0000;
            0
        };
        ax = from(ix);
        let bp = [1.0f32, 1.5][k as usize];
        let u = sub(ax, bp);
        let v = div(1.0, add(ax, bp));
        let ss = mul(u, v);
        let s_h = from(bits(ss) & -0x1000);
        let mut t_h = from(((ix >> 1) | 0x2000_0000) + 0x0004_0000 + (k << 21));
        let mut t_l = sub(ax, sub(t_h, bp));
        let s_l = mul(v, sub(sub(u, mul(s_h, t_h)), mul(s_h, t_l)));
        let l = POW_L.map(c);
        let mut s2 = mul(ss, ss);
        let mut r = mul(mul(s2, s2), add(l[0], mul(s2, add(l[1], mul(s2, add(l[2], mul(s2, add(l[3], mul(s2, add(l[4], mul(s2, l[5])))))))))));
        r = add(r, mul(s_l, add(s_h, ss)));
        s2 = mul(s_h, s_h);
        t_h = add(add(s2, 3.0), r);
        t_h = from(bits(t_h) & -0x1000);
        t_l = sub(r, sub(sub(t_h, 3.0), s2));
        let u = mul(s_h, t_h);
        let v = add(mul(s_l, t_h), mul(t_l, ss));
        let mut p_h = add(u, v);
        p_h = from(bits(p_h) & -0x1000);
        let p_l = sub(v, sub(p_h, u));
        let z_h = mul(c(POW_CP_H), p_h);
        let z_l = add(add(mul(c(POW_CP_L), p_h), mul(p_l, c(POW_CP))), c(POW_DPL[k as usize]));
        let t = n as f32;
        let mut tt1 = add(add(add(z_h, z_l), c(POW_DPH[k as usize])), t);
        tt1 = from(bits(tt1) & -0x1000);
        t1 = tt1;
        t2 = sub(z_l, sub(sub(sub(tt1, t), c(POW_DPH[k as usize])), z_h));
    }
    let y1 = from(hy & -0x1000);
    let p_l = add(mul(sub(y, y1), t1), mul(y, t2));
    let mut p_h = mul(y1, t1);
    let mut z = add(p_l, p_h);
    let mut j = bits(z);
    if j > 0x4300_0000 {
        return mul(mul(sn, c(POW_HUGE)), c(POW_HUGE));
    } else if j == 0x4300_0000 {
        if lt(sub(z, p_h), add(p_l, c(POW_OVT))) {
            return mul(mul(sn, c(POW_HUGE)), c(POW_HUGE));
        }
    } else if j & 0x7FFF_FFFF > 0x4316_0000 {
        return mul(mul(sn, c(POW_TINY)), c(POW_TINY));
    } else if j as u32 == 0xC316_0000 && le(p_l, sub(z, p_h)) {
        return mul(mul(sn, c(POW_TINY)), c(POW_TINY));
    }
    let i = j & 0x7FFF_FFFF;
    let mut k = (i >> 23) - 0x7F;
    n = 0;
    if i > 0x3F00_0000 {
        n = j + (0x0080_0000 >> (k + 1));
        k = ((n & 0x7FFF_FFFF) >> 23) - 0x7F;
        let t = from(n & !(0x007F_FFFF >> k));
        n = ((n & 0x007F_FFFF) | 0x0080_0000) >> (23 - k);
        if j < 0 {
            n = -n;
        }
        p_h = sub(p_h, t);
    }
    let mut t = add(p_l, p_h);
    t = from(bits(t) & -0x1000);
    let u = mul(t, c(POW_LG2_H));
    let v = add(mul(sub(p_l, sub(t, p_h)), c(POW_LG2)), mul(t, c(POW_LG2_L)));
    z = add(u, v);
    let w = sub(v, sub(z, u));
    t = mul(z, z);
    let p = POW_P.map(c);
    let t1 = sub(z, mul(t, add(p[0], mul(t, add(p[1], mul(t, add(p[2], mul(t, add(p[3], mul(t, p[4]))))))))));
    let r = sub(div(mul(z, t1), sub(t1, 2.0)), add(w, mul(z, w)));
    z = sub(1.0, sub(r, z));
    j = bits(z) + (n << 23);
    z = if j >> 23 <= 0 { scalbnf(z, n) } else { from(j) };
    mul(sn, z)
}

/// newlib `__scalbnf` (0x0012DC80).
pub fn scalbnf(x: f32, n: i32) -> f32 {
    let mut x = x;
    let mut ix = bits(x);
    let mut k = (ix & 0x7F80_0000) >> 23;
    if k == 0 {
        if ix & 0x7FFF_FFFF == 0 {
            return x;
        }
        x = mul(x, c(POW_TWO25));
        ix = bits(x);
        k = ((ix & 0x7F80_0000) >> 23) - 25;
        if n < -50000 {
            return mul(c(POW_TINY), x);
        }
    }
    if k == 0xFF {
        return add(x, x);
    }
    k += n;
    let copysign = |a: f32, b: f32| from((bits(a) & 0x7FFF_FFFF) | (bits(b) & i32::MIN));
    if k > 0xFE {
        return mul(c(POW_HUGE), copysign(c(POW_HUGE), x));
    }
    if k > 0 {
        return from((ix & -0x7F80_0001) | (k << 23));
    }
    if k <= -25 {
        if n > 50000 {
            return mul(c(POW_HUGE), copysign(c(POW_HUGE), x));
        }
        return mul(c(POW_TINY), copysign(c(POW_TINY), x));
    }
    k += 25;
    mul(from((ix & -0x7F80_0001) | (k << 23)), c(POW_TWOM25))
}
