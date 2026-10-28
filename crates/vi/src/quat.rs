//! `VIQuat` and the `VIMatrix44` operations the animation code uses
//! (decomp `viquat.cpp`, `vimatrix44.cpp`; VU0 routines as eqoa-port spells
//! them under `VI_PORT`).

use crate::fpu::{F, Model, f};
use crate::geom::{Matrix44, Vect4};
use crate::vect::Vect3;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat<M: Model> {
    pub x: F<M>,
    pub y: F<M>,
    pub z: F<M>,
    pub w: F<M>,
}

impl<M: Model> Default for Quat<M> {
    /// The identity (`VIHSpriteFrame`'s constructor).
    fn default() -> Self {
        Quat { x: F::ZERO, y: F::ZERO, z: F::ZERO, w: F::ONE }
    }
}

impl<M: Model> Quat<M> {
    /// `ConvertFrom(const VIVect3 &hpr)`.
    pub fn from_hpr(v: &Vect3<M>) -> Self {
        let h = f(0.5);
        let (sx, cx) = ((v.x * h).sinf(), (v.x * h).cosf());
        let (sy, cy) = ((v.y * h).sinf(), (v.y * h).cosf());
        let (sz, cz) = ((v.z * h).sinf(), (v.z * h).cosf());
        Quat {
            x: (sx * cy) * sz + (cx * sy) * cz,
            y: (-cx * sy) * sz + (sx * cy) * cz,
            z: (-sx * sy) * cz + (cx * cy) * sz,
            w: (cx * cy) * cz + (sx * sy) * sz,
        }
    }

    /// `Slerp(a, b, t)`: the negative-dot half negates the dot and b's
    /// weight, never b itself.
    pub fn slerp(a: &Self, b: &Self, t: F<M>) -> Self {
        let (wa, wb);
        let mut dot = ((a.x * b.x + a.y * b.y) + a.z * b.z) + a.w * b.w;
        if F::ZERO < dot {
            if F::ONE - dot > f(1.0e-4) {
                let theta = dot.acosf();
                let inv = F::ONE / theta.sinf();
                wa = ((F::ONE - t) * theta).sinf() * inv;
                wb = (t * theta).sinf() * inv;
            } else {
                wa = F::ONE - t;
                wb = t;
            }
        } else {
            dot = -dot;
            if F::ONE - dot > f(1.0e-4) {
                let theta = dot.acosf();
                let inv = F::ONE / theta.sinf();
                wa = ((F::ONE - t) * theta).sinf() * inv;
                wb = -(t * theta).sinf() * inv;
            } else {
                wa = F::ONE - t;
                wb = -t;
            }
        }
        Quat {
            x: a.x * wa + b.x * wb,
            y: a.y * wa + b.y * wb,
            z: a.z * wa + b.z * wb,
            w: a.w * wa + b.w * wb,
        }
    }

    /// `ConvertTo(VIVect3 &hpr) const`: the long way round, through a
    /// matrix (`Rotate`, then `ConvertToEulerZXY`).
    pub fn to_hpr(&self) -> Vect3<M> {
        Matrix44::rotate(self).euler_zxy()
    }
}

fn comp<M: Model>(v: &Vect4<M>, k: usize) -> F<M> {
    [v.x, v.y, v.z, v.w][k]
}
fn scale_vector<M: Model>(v: &Vect4<M>, t: F<M>) -> Vect4<M> {
    Vect4::new(v.x * t, v.y * t, v.z * t, v.w * t)
}
fn add_scaled_vector<M: Model>(acc: &mut Vect4<M>, row: &Vect4<M>, t: F<M>) {
    let (x, y, z, w) = (row.x * t, row.y * t, row.z * t, row.w * t);
    acc.x = acc.x + x;
    acc.y = acc.y + y;
    acc.z = acc.z + z;
    acc.w = acc.w + w;
}

impl<M: Model> Matrix44<M> {
    fn quat_basis(q: &Quat<M>) -> [F<M>; 9] {
        let wx = (q.w + q.w) * q.x;
        let wy = (q.w + q.w) * q.y;
        let wz = (q.w + q.w) * q.z;
        let xx = (q.x + q.x) * q.x;
        let xy = (q.x + q.x) * q.y;
        let xz = (q.x + q.x) * q.z;
        let yy = (q.y + q.y) * q.y;
        let yz = (q.y + q.y) * q.z;
        let zz = (q.z + q.z) * q.z;
        [
            F::ONE - (yy + zz),
            xy + wz,
            xz - wy,
            xy - wz,
            F::ONE - (xx + zz),
            yz + wx,
            xz + wy,
            yz - wx,
            F::ONE - (xx + yy),
        ]
    }

    /// `Rotate(const VIQuat &)`: any quaternion's rotation (scaled by two
    /// over its norm, where `quat_basis` takes it as a unit one), the last
    /// row the identity's.
    pub fn rotate(q: &Quat<M>) -> Self {
        let s = f::<M>(2.0) / (((q.w * q.w + q.x * q.x) + q.y * q.y) + q.z * q.z);
        let (xs, ys, zs) = (q.x * s, q.y * s, q.z * s);
        let (wx, wy, wz) = (q.w * xs, q.w * ys, q.w * zs);
        let (xx, xy, xz) = (q.x * xs, q.x * ys, q.x * zs);
        let (yy, yz, zz) = (q.y * ys, q.y * zs, q.z * zs);
        let (o, z) = (F::ONE, F::ZERO);
        Matrix44 {
            m: [
                Vect4::new(o - (yy + zz), xy + wz, xz - wy, z),
                Vect4::new(xy - wz, o - (xx + zz), yz + wx, z),
                Vect4::new(xz + wy, yz - wx, o - (xx + yy), z),
                Vect4::new(z, z, z, o),
            ],
        }
    }

    /// `ConvertToEulerZXY(VIVect3 &) const`: (heading, pitch, roll); with
    /// no heading to read (the third row straight up or down) the heading is
    /// 0 and the roll is taken off the first two rows.
    pub fn euler_zxy(&self) -> Vect3<M> {
        let m = &self.m;
        let cy = (m[2].z * m[2].z + m[2].x * m[2].x).sqrt();
        if cy > F::ZERO {
            Vect3::new(m[2].x.atan2f(m[2].z), (-m[2].y).atan2f(cy), m[0].y.atan2f(m[1].y))
        } else {
            Vect3::new(F::ZERO, (-m[2].y).atan2f(cy), (-m[1].x).atan2f(m[0].x))
        }
    }

    /// `Transform(const VIQuat &, float, const VIVect3 &)`.
    pub fn transform_quat(q: &Quat<M>, s: F<M>, t: &Vect3<M>) -> Self {
        Self::transform_quat3(q, &Vect3::new(s, s, s), t)
    }

    /// `Transform(const VIQuat &, const VIVect3 &, const VIVect3 &)`: row i
    /// scaled by s's component i.
    pub fn transform_quat3(q: &Quat<M>, s: &Vect3<M>, t: &Vect3<M>) -> Self {
        let b = Self::quat_basis(q);
        let z = F::ZERO;
        Matrix44 {
            m: [
                Vect4::new(b[0] * s.x, b[1] * s.x, b[2] * s.x, z),
                Vect4::new(b[3] * s.y, b[4] * s.y, b[5] * s.y, z),
                Vect4::new(b[6] * s.z, b[7] * s.z, b[8] * s.z, z),
                Vect4::new(t.x, t.y, t.z, F::ONE),
            ],
        }
    }

    pub fn scale(s: F<M>) -> Self {
        let mut m = Self::identity();
        m.m[0].x = s;
        m.m[1].y = s;
        m.m[2].z = s;
        m
    }

    pub fn translate(v: &Vect3<M>) -> Self {
        let mut m = Self::identity();
        m.m[3].x = v.x;
        m.m[3].y = v.y;
        m.m[3].z = v.z;
        m
    }

    /// `Invert`: Gauss-Jordan with partial pivoting; `None` when singular.
    pub fn invert(a: &Self) -> Option<Self> {
        let (o, ok) = Self::invert_partial(a);
        ok.then_some(o)
    }

    /// `Invert` as it leaves `this`: the inverse and true, or at a zero pivot
    /// false and the rows as far as they got (a caller that ignores the
    /// return, `VIScene::RenderShadow`, goes on with them).
    pub fn invert_partial(a: &Self) -> (Self, bool) {
        let mut w = *a;
        let mut o = Self::identity();
        for i in 0..4 {
            let mut p = comp(&w.m[i], i);
            let mut mag = p.abs();
            let mut row = i;
            for k in i + 1..4 {
                let t = comp(&w.m[k], i).abs();
                if mag < t {
                    mag = t;
                    p = comp(&w.m[k], i);
                    row = k;
                }
            }
            if p == F::ZERO {
                return (o, false);
            }
            w.m.swap(i, row);
            o.m.swap(i, row);
            let q = F::ONE / p;
            w.m[i] = scale_vector(&w.m[i], q);
            o.m[i] = scale_vector(&o.m[i], q);
            for k in 0..4 {
                if k != i {
                    let fk = -comp(&w.m[k], i);
                    let (wi, oi) = (w.m[i], o.m[i]);
                    add_scaled_vector(&mut w.m[k], &wi, fk);
                    add_scaled_vector(&mut o.m[k], &oi, fk);
                }
            }
        }
        (o, true)
    }

    /// `ConvertTo(VIQuat &, float &scale, VIVect3 &t)`.
    pub fn convert_to(&self) -> (Quat<M>, F<M>, Vect3<M>) {
        let m = &self.m;
        let nxt = [1usize, 2, 0];
        let mut inv = F::ONE;
        let t = Vect3::new(m[3].x, m[3].y, m[3].z);
        let mut scale = m[0].x * m[0].x + m[0].y * m[0].y + m[0].z * m[0].z;
        if scale > F::ZERO {
            scale = scale * (F::ONE / scale.sqrt());
            inv = F::ONE / scale;
        }
        let tr = (m[0].x + m[1].y + m[2].z) * inv;
        let mut q = Quat::default();
        if tr > F::ZERO {
            let mut s = (tr + F::ONE).sqrt();
            q.w = s * f(0.5);
            s = f(0.5) / s;
            q.x = (m[1].z - m[2].y) * s * inv;
            q.y = (m[2].x - m[0].z) * s * inv;
            q.z = (m[0].y - m[1].x) * s * inv;
        } else {
            let mut i = 0;
            if m[1].y > m[0].x {
                i = 1;
            }
            if m[2].z > comp(&m[i], i) {
                i = 2;
            }
            let j = nxt[i];
            let k = nxt[j];
            let (mi, mj, mk) = (&m[i], &m[j], &m[k]);
            let mut s = ((comp(mi, i) - (comp(mj, j) + comp(mk, k))) * inv + F::ONE).sqrt();
            let mut qq = [F::ZERO; 4];
            qq[i] = s * f(0.5);
            if s != F::ZERO {
                s = f(0.5) / s;
            }
            qq[3] = (comp(mj, k) - comp(mk, j)) * s * inv;
            qq[j] = (comp(mi, j) + comp(mj, i)) * s * inv;
            qq[k] = (comp(mi, k) + comp(mk, i)) * s * inv;
            q = Quat { x: qq[0], y: qq[1], z: qq[2], w: qq[3] };
        }
        (q, scale, t)
    }
}

