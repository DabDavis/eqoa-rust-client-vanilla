//! `VIVect4`, `VIMatrix44`, `VIPlane`, `VIBBox`, `VISphere`: the parts the
//! collision walk uses. The matrices are ROW-major with ROW vectors
//! (`p' = p * M`), as the engine's are. Where the guest uses VU0, this follows
//! eqoa-port's C spelling of it (`vimatrix44.cpp` under `VI_PORT`,
//! `platform/vu0.c`), operation for operation, so the `Ieee` model can be held
//! to the oracle; under `Ee` the same order runs on the console's FPU rules.

use crate::fpu::{F, Model, f};
use crate::vect::Vect3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Vect4<M: Model> {
    pub x: F<M>,
    pub y: F<M>,
    pub z: F<M>,
    pub w: F<M>,
}

impl<M: Model> Vect4<M> {
    pub fn new(x: F<M>, y: F<M>, z: F<M>, w: F<M>) -> Self {
        Vect4 { x, y, z, w }
    }

    pub fn xyz(&self) -> Vect3<M> {
        Vect3::new(self.x, self.y, self.z)
    }

    pub fn normalize(&mut self) -> i32 {
        let d = ((self.x * self.x + self.y * self.y) + self.z * self.z) + self.w * self.w;
        if d != F::ZERO {
            let r = F::ONE / d.sqrt();
            self.x = self.x * r;
            self.y = self.y * r;
            self.z = self.z * r;
            self.w = self.w * r;
            return 1;
        }
        0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Matrix44<M: Model> {
    pub m: [Vect4<M>; 4],
}

impl<M: Model> Matrix44<M> {
    pub fn identity() -> Self {
        let (o, z) = (F::ONE, F::ZERO);
        Matrix44 {
            m: [Vect4::new(o, z, z, z), Vect4::new(z, o, z, z), Vect4::new(z, z, o, z), Vect4::new(z, z, z, o)],
        }
    }

    /// `sceVu0ApplyMatrix`: `out = v * M`, row vectors. Each lane is
    /// ((r0·x + r1·y) + r2·z) + r3·w, the four computed side by side
    /// (`Model::mul4`, `add4`), as `mul` computes a row.
    #[inline]
    pub fn apply(&self, v: &Vect4<M>) -> Vect4<M> {
        let lanes = |r: &Vect4<M>| [r.x.0, r.y.0, r.z.0, r.w.0];
        let s = M::mul4(lanes(&self.m[0]), [v.x.0; 4]);
        let s = M::add4(s, M::mul4(lanes(&self.m[1]), [v.y.0; 4]));
        let s = M::add4(s, M::mul4(lanes(&self.m[2]), [v.z.0; 4]));
        let s = M::add4(s, M::mul4(lanes(&self.m[3]), [v.w.0; 4]));
        Vect4::new(F::new(s[0]), F::new(s[1]), F::new(s[2]), F::new(s[3]))
    }

    /// `Mul(const VIVect3 &, VIVect3 &)`: a point (w = 1).
    pub fn mul_point(&self, p: &Vect3<M>) -> Vect3<M> {
        self.apply(&Vect4::new(p.x, p.y, p.z, F::ONE)).xyz()
    }

    /// `this = a * b`.
    pub fn mul(a: &Self, b: &Self) -> Self {
        // Each element is ((b0·x + b1·y) + b2·z) + b3·w of its column of b's
        // rows and a's row: the four of a row computed side by side, each in
        // that order (`Model::mul4`, `add4`).
        let lanes = |v: &Vect4<M>| [v.x.0, v.y.0, v.z.0, v.w.0];
        let (b0, b1, b2, b3) = (lanes(&b.m[0]), lanes(&b.m[1]), lanes(&b.m[2]), lanes(&b.m[3]));
        let row = |r: &Vect4<M>| {
            let s = M::mul4(b0, [r.x.0; 4]);
            let s = M::add4(s, M::mul4(b1, [r.y.0; 4]));
            let s = M::add4(s, M::mul4(b2, [r.z.0; 4]));
            let s = M::add4(s, M::mul4(b3, [r.w.0; 4]));
            Vect4::new(F::new(s[0]), F::new(s[1]), F::new(s[2]), F::new(s[3]))
        };
        Matrix44 { m: [row(&a.m[0]), row(&a.m[1]), row(&a.m[2]), row(&a.m[3])] }
    }

    /// `Transform(const VIVect3 &hpr, float scale, const VIVect3 &t)`: the
    /// rotation from (heading, pitch, roll) = (v.x, v.y, v.z), scaled, then
    /// the translation row.
    pub fn transform(v: &Vect3<M>, scale: F<M>, t: &Vect3<M>) -> Self {
        let (si, ci) = (v.x.sinf(), v.x.cosf());
        let (sj, cj) = (v.y.sinf(), v.y.cosf());
        let (sh, ch) = (v.z.sinf(), v.z.cosf());
        let z = F::ZERO;
        Matrix44 {
            m: [
                Vect4::new((ch * ci + sh * sj * si) * scale, (sh * cj) * scale, (-ch * si + sh * sj * ci) * scale, z),
                Vect4::new((-sh * ci + ch * sj * si) * scale, (ch * cj) * scale, (sh * si + ch * sj * ci) * scale, z),
                Vect4::new((cj * si) * scale, (-sj) * scale, (cj * ci) * scale, z),
                Vect4::new(t.x, t.y, t.z, F::ONE),
            ],
        }
    }

    /// The affine inverse: cofactor rows, the determinant, the 3×3
    /// transposed and scaled, then the translation.
    pub fn invert_affine(a: &Self) -> Self {
        let (r0, r1, r2, t) = (a.m[0], a.m[1], a.m[2], a.m[3]);
        let c0x = r1.y * r2.z - r2.y * r1.z;
        let c0y = r1.z * r2.x - r2.z * r1.x;
        let c0z = r1.x * r2.y - r2.x * r1.y;
        let c1x = r0.y * r2.z - r2.y * r0.z;
        let c1y = r0.z * r2.x - r2.z * r0.x;
        let c1z = r0.x * r2.y - r2.x * r0.y;
        let c2x = r0.y * r1.z - r1.y * r0.z;
        let c2y = r0.z * r1.x - r1.z * r0.x;
        let c2z = r0.x * r1.y - r1.x * r0.y;
        let det = (r0.x * c0x + r0.y * c0y) + r0.z * c0z;
        let q = F::ONE / det;
        let z = F::ZERO;
        let o0 = Vect4::new((z + c0x) * q, (z - c1x) * q, (z + c2x) * q, z);
        let o1 = Vect4::new((z + c0y) * q, (z - c1y) * q, (z + c2y) * q, z);
        let o2 = Vect4::new((z + c0z) * q, (z - c1z) * q, (z + c2z) * q, z);
        let o3 = Vect4::new(
            ((z - o0.x * t.x) - o1.x * t.y) - o2.x * t.z,
            ((z - o0.y * t.x) - o1.y * t.y) - o2.y * t.z,
            ((z - o0.z * t.x) - o1.z * t.y) - o2.z * t.z,
            F::ONE,
        );
        Matrix44 { m: [o0, o1, o2, o3] }
    }

    /// Column-major `f32`s, for a renderer.
    pub fn to_cols(&self) -> [[f32; 4]; 4] {
        let r = &self.m;
        [
            [r[0].x.0, r[0].y.0, r[0].z.0, r[0].w.0],
            [r[1].x.0, r[1].y.0, r[1].z.0, r[1].w.0],
            [r[2].x.0, r[2].y.0, r[2].z.0, r[2].w.0],
            [r[3].x.0, r[3].y.0, r[3].z.0, r[3].w.0],
        ]
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Plane<M: Model> {
    pub n: Vect3<M>,
    pub d: F<M>,
}

impl<M: Model> Plane<M> {
    /// `v.x*n.x + v.y*n.y + v.z*n.z + d`, the point first.
    pub fn distance(&self, v: &Vect3<M>) -> F<M> {
        v.x * self.n.x + v.y * self.n.y + v.z * self.n.z + self.d
    }

    /// `operator()(a, b, c)`: the plane through a triangle; 0 when degenerate.
    pub fn from_tri(&mut self, a: &Vect3<M>, b: &Vect3<M>, c: &Vect3<M>) -> i32 {
        let u = Vect3::new(b.x - a.x, b.y - a.y, b.z - a.z);
        let v = Vect3::new(c.x - b.x, c.y - b.y, c.z - b.z);
        self.n = Vect3::new(v.y * u.z - v.z * u.y, -v.x * u.z + v.z * u.x, v.x * u.y - v.y * u.x);
        if self.n.normalize() != 0 {
            self.d = -((self.n.x * a.x + self.n.y * a.y) + self.n.z * a.z);
            return 1;
        }
        self.d = F::ZERO;
        0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct BBox<M: Model> {
    pub min: Vect3<M>,
    pub max: Vect3<M>,
}

fn bb_min<M: Model>(a: F<M>, b: F<M>) -> F<M> {
    if a > b { b } else { a }
}

impl<M: Model> BBox<M> {
    /// `VIBBox::DistanceSquared`: from `v` to the nearest point of the box.
    pub fn distance_squared(&self, v: &Vect3<M>) -> F<M> {
        let mut d = Vect3::<M>::default();
        if d.x < self.min.x - v.x {
            d.x = self.min.x - v.x;
        }
        if d.y < self.min.y - v.y {
            d.y = self.min.y - v.y;
        }
        if d.z < self.min.z - v.z {
            d.z = self.min.z - v.z;
        }
        if d.x < v.x - self.max.x {
            d.x = v.x - self.max.x;
        }
        if d.y < v.y - self.max.y {
            d.y = v.y - self.max.y;
        }
        if d.z < v.z - self.max.z {
            d.z = v.z - self.max.z;
        }
        d.x * d.x + d.y * d.y + d.z * d.z
    }

    pub fn bound(&mut self, first: bool, v: &Vect3<M>) {
        if !first {
            self.min.x = bb_min(self.min.x, v.x);
            self.min.y = bb_min(self.min.y, v.y);
            self.min.z = bb_min(self.min.z, v.z);
            let mx = &mut self.max;
            mx.x = if v.x < mx.x { mx.x } else { v.x };
            mx.y = if v.y < mx.y { mx.y } else { v.y };
            mx.z = if v.z < mx.z { mx.z } else { v.z };
        } else {
            self.min = *v;
            self.max = *v;
        }
    }

    /// `Union(a, b)`.
    pub fn union(a: &Self, b: &Self) -> Self {
        let pick_min = |bv: F<M>, av: F<M>| if bv < av { bv } else { av };
        let pick_max = |bv: F<M>, av: F<M>| if bv < av { av } else { bv };
        BBox {
            min: Vect3::new(pick_min(b.min.x, a.min.x), pick_min(b.min.y, a.min.y), pick_min(b.min.z, a.min.z)),
            max: Vect3::new(pick_max(b.max.x, a.max.x), pick_max(b.max.y, a.max.y), pick_max(b.max.z, a.max.z)),
        }
    }

    /// `Transform(m, VIVect3 *p)`: the eight corners, transformed.
    pub fn corners(&self, m: &Matrix44<M>) -> [Vect3<M>; 8] {
        let (a, b) = (self.min, self.max);
        [
            Vect3::new(a.x, a.y, a.z),
            Vect3::new(b.x, a.y, a.z),
            Vect3::new(b.x, a.y, b.z),
            Vect3::new(a.x, a.y, b.z),
            Vect3::new(a.x, b.y, a.z),
            Vect3::new(b.x, b.y, a.z),
            Vect3::new(b.x, b.y, b.z),
            Vect3::new(a.x, b.y, b.z),
        ]
        .map(|p| m.apply(&Vect4::new(p.x, p.y, p.z, F::ONE)).xyz())
    }

    /// `Store(VIVect3 *p)`: the eight corners, in `corners`' order.
    pub fn store(&self) -> [Vect3<M>; 8] {
        let (a, b) = (self.min, self.max);
        [
            Vect3::new(a.x, a.y, a.z),
            Vect3::new(b.x, a.y, a.z),
            Vect3::new(b.x, a.y, b.z),
            Vect3::new(a.x, a.y, b.z),
            Vect3::new(a.x, b.y, a.z),
            Vect3::new(b.x, b.y, a.z),
            Vect3::new(b.x, b.y, b.z),
            Vect3::new(a.x, b.y, b.z),
        ]
    }

    /// `Transform(m, VIBBox &)`: the box round the eight corners through `m`.
    pub fn transform(&self, m: &Matrix44<M>) -> BBox<M> {
        let mut b = BBox::default();
        for (k, p) in self.corners(m).iter().enumerate() {
            b.bound(k == 0, p);
        }
        b
    }

    /// `Intersects(const VIBBox &)`: no gap between the two on any axis.
    pub fn intersects(&self, b: &BBox<M>) -> bool {
        !(self.min.x > b.max.x || self.max.x < b.min.x || self.min.y > b.max.y || self.max.y < b.min.y || self.min.z > b.max.z || self.max.z < b.min.z)
    }

    /// `CullIntersect(const VISphere &)`: the sphere clear of the box past
    /// one of its six faces (the faces in the decomp's order).
    pub fn cull_intersect(&self, s: &Sphere<M>) -> bool {
        s.r < self.min.x - s.c.x || s.r < self.min.y - s.c.y || s.r < self.min.z - s.c.z || s.r < s.c.x - self.max.x || s.r < s.c.y - self.max.y || s.r < s.c.z - self.max.z
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Sphere<M: Model> {
    pub c: Vect3<M>,
    pub r: F<M>,
}

impl<M: Model> Sphere<M> {
    /// `Init(const VIBBox &)`: the box's centre and half-diagonal.
    pub fn from_bbox(b: &BBox<M>) -> Self {
        let mut c = b.min;
        c += b.max;
        c *= f(0.5);
        let mut d = b.max;
        d -= c;
        Sphere { c, r: d.magnitude() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpu::{Ee, Ieee};

    /// `apply` lane by lane, one scalar operation at a time in the order the
    /// guest computes it: what `apply` was before it went four lanes wide.
    fn scalar<M: Model>(m: &Matrix44<M>, v: &Vect4<M>) -> [f32; 4] {
        let r = &m.m;
        let c = |k: fn(&Vect4<M>) -> F<M>| ((k(&r[0]) * v.x + k(&r[1]) * v.y) + k(&r[2]) * v.z) + k(&r[3]) * v.w;
        [c(|q| q.x).0, c(|q| q.y).0, c(|q| q.z).0, c(|q| q.w).0]
    }

    /// Floats of every kind: zeros, denormals, the world's tens of
    /// thousands, saturating ones, exponent-255 patterns.
    fn floats(seed: &mut u64) -> f32 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let b = (*seed >> 32) as u32;
        match b % 8 {
            0 => 0.0,
            1 => f32::from_bits(b & 0x807F_FFFF),
            2 => f32::from_bits(b | 0x7F80_0000),
            3 => (b % 60000) as f32 - 30000.0 + (b >> 16) as f32 / 65536.0,
            _ => f32::from_bits((b & 0x80FF_FFFF) | (((b >> 24) % 80 + 88) << 23)),
        }
    }

    fn check<M: Model>(n: usize) -> usize {
        let mut seed = 7u64;
        let mut differ = 0;
        for _ in 0..n {
            let mut m = Matrix44::<M>::identity();
            for r in m.m.iter_mut() {
                *r = Vect4::new(F::new(floats(&mut seed)), F::new(floats(&mut seed)), F::new(floats(&mut seed)), F::new(floats(&mut seed)));
            }
            let v = Vect4::new(F::new(floats(&mut seed)), F::new(floats(&mut seed)), F::new(floats(&mut seed)), F::new(floats(&mut seed)));
            let a = m.apply(&v);
            let want = scalar(&m, &v);
            for (k, got) in [a.x.0, a.y.0, a.z.0, a.w.0].into_iter().enumerate() {
                assert!(got.to_bits() == want[k].to_bits() || (got.is_nan() && want[k].is_nan()), "lane {k}: {got:e} [{:08X}], want {:e} [{:08X}]", got.to_bits(), want[k], want[k].to_bits());
                // the control: the sum taken the other way round
                let r = &m.m;
                let g = |q: &Vect4<M>| [q.x, q.y, q.z, q.w][k];
                let other = g(&r[0]) * v.x + (g(&r[1]) * v.y + (g(&r[2]) * v.z + g(&r[3]) * v.w));
                differ += (other.0.to_bits() != want[k].to_bits()) as usize;
            }
        }
        differ
    }

    /// Four lanes at once are the scalar operations bit for bit, on the
    /// console's arithmetic and on IEEE's; the control (the same sum in
    /// another order) differs on many of the same inputs.
    #[test]
    fn apply_is_the_scalar_sum_lane_by_lane() {
        let n = 200_000;
        let (ee, ieee) = (check::<Ee>(n), check::<Ieee>(n));
        eprintln!("the other order differs on {ee} (Ee) and {ieee} (Ieee) of {} lanes", 4 * n);
        assert!(ee > 1000 && ieee > 1000);
    }
}
