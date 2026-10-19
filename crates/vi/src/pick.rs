//! Picking: `VIPickSegment` and `VIRaster`'s pick family (virasterpick.cpp,
//! vipicksegment.cpp). A segment is taken into a prim buffer's object space
//! through the inverse model-view, culled against the buffer's and then each
//! strip's box, and walked triangle by triangle (the winding alternating on
//! odd ones) through `Pick`; a hit records the plane normal, the parametric
//! time and the material, and `CalcHitColor` interpolates the three vertex
//! colours at the hit point. The outdoors probe reads the hit colour's alpha.

use crate::fpu::{F, Model};
use crate::geom::{BBox, Matrix44, Plane, Sphere, Vect4};
use crate::particle::Matrix33;
use crate::vect::Vect3;

/// `VIColor32F`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color32F<M: Model> {
    pub r: F<M>,
    pub g: F<M>,
    pub b: F<M>,
    pub a: F<M>,
}

/// `VIPickSegment`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PickSegment<M: Model> {
    pub start: Vect3<M>,
    pub end: Vect3<M>,
    pub delta: Vect3<M>,
    pub dir: Vect3<M>,
    pub length: F<M>,
    pub inv_length: F<M>,
    pub hit_time: F<M>,
    /// -1: none
    pub hit_material: i32,
    pub hit_normal: Vect3<M>,
    pub hit_color: Color32F<M>,
}

impl<M: Model> PickSegment<M> {
    fn precalc(&mut self) {
        self.delta = self.end;
        self.delta -= self.start;
        self.dir = self.delta;
        let l = self.dir.magnitude();
        self.length = l;
        if l != F::ZERO {
            let inv = F::ONE / l;
            self.inv_length = inv;
            self.dir *= inv;
        } else {
            self.inv_length = F::ZERO;
        }
        self.hit_time = F::ONE;
        self.hit_material = -1;
    }

    /// `Init(start, end)`.
    pub fn new(start: Vect3<M>, end: Vect3<M>) -> Self {
        let mut s = PickSegment { start, end, hit_color: Color32F { a: F::ONE, ..Default::default() }, ..Default::default() };
        s.precalc();
        s
    }

    /// `Init(src, mat)`: `src` under `mat`, its hit carried over.
    pub fn transformed(src: &Self, mat: &Matrix44<M>) -> Self {
        let mut s = PickSegment { start: mat.mul_point(&src.start), end: mat.mul_point(&src.end), ..Default::default() };
        let n = &src.hit_normal;
        s.hit_normal = mat.apply(&Vect4::new(n.x, n.y, n.z, F::ZERO)).xyz();
        s.hit_normal.normalize();
        s.hit_color = src.hit_color;
        s.precalc();
        s.hit_time = src.hit_time;
        s.hit_material = src.hit_material;
        s
    }

    /// `GetHitPoint`.
    pub fn hit_point(&self) -> Vect3<M> {
        let p = self.delta;
        Vect3::new(p.x * self.hit_time + self.start.x, p.y * self.hit_time + self.start.y, p.z * self.hit_time + self.start.z)
    }
}

/// `VIRaster::Cull(seg, VISphere &)`: 1 when the segment cannot reach the
/// sphere before the current hit.
pub fn cull_sphere<M: Model>(seg: &PickSegment<M>, s: &Sphere<M>) -> bool {
    let mut d = seg.start;
    d -= s.c;
    let c = d.x * d.x + d.y * d.y + d.z * d.z - s.r * s.r;
    if c > F::ZERO {
        let dir = &seg.dir;
        let b = d.x * dir.x + d.y * dir.y + d.z * dir.z;
        if b < F::ZERO {
            let disc = b * b - c;
            if disc > F::ZERO && -(b + disc.sqrt()) * seg.inv_length < seg.hit_time {
                return false;
            }
        }
        true
    } else {
        false
    }
}

/// `VIRaster::Cull(seg, VIBBox &)`: both ends past one face.
pub fn cull_bbox<M: Model>(seg: &PickSegment<M>, b: &BBox<M>) -> bool {
    let (s, e) = (&seg.start, &seg.end);
    (s.x > b.max.x && e.x > b.max.x)
        || (s.y > b.max.y && e.y > b.max.y)
        || (s.z > b.max.z && e.z > b.max.z)
        || (s.x < b.min.x && e.x < b.min.x)
        || (s.y < b.min.y && e.y < b.min.y)
        || (s.z < b.min.z && e.z < b.min.z)
}

/// `VIRaster::PointInTriangle`.
pub fn point_in_triangle<M: Model>(p: &Vect3<M>, pl: &Plane<M>, a: &Vect3<M>, b: &Vect3<M>, c: &Vect3<M>) -> bool {
    let edge = |u: &Vect3<M>, v: &Vect3<M>| {
        let e = Vect3::new(v.x - u.x, v.y - u.y, v.z - u.z);
        let w = Vect3::new(p.x - v.x, p.y - v.y, p.z - v.z);
        let x = Vect3::new(e.y * w.z - e.z * w.y, -e.x * w.z + e.z * w.x, e.x * w.y - e.y * w.x);
        x.x * pl.n.x + x.y * pl.n.y + x.z * pl.n.z > F::ZERO
    };
    !(edge(a, b) || edge(b, c) || edge(c, a))
}

/// `VIRaster::Pick(seg, mat, a, b, c)`: a front-facing crossing nearer than
/// the current hit.
pub fn pick_tri<M: Model>(seg: &mut PickSegment<M>, mat: i32, a: &Vect3<M>, b: &Vect3<M>, c: &Vect3<M>) -> bool {
    let mut pl = Plane::<M>::default();
    if pl.from_tri(a, b, c) == 0 {
        return false;
    }
    let d0 = pl.distance(&seg.start);
    if d0 > F::ZERO {
        let d1 = pl.distance(&seg.end);
        if d1 < F::ZERO {
            let t = d0 / (d0 - d1);
            if t < seg.hit_time {
                let mut p = seg.delta;
                p *= t;
                p += seg.start;
                if point_in_triangle(&p, &pl, a, b, c) {
                    seg.hit_time = t;
                    seg.hit_material = mat;
                    seg.hit_normal = pl.n;
                    return true;
                }
            }
        }
    }
    false
}

/// `VIMatrix33::Invert`: Gauss-Jordan with partial pivoting; `None` when
/// singular.
pub fn invert33<M: Model>(a: &Matrix33<M>) -> Option<Matrix33<M>> {
    let mut w = a.m.map(|r| [r.x, r.y, r.z]);
    let mut o = Matrix33::<M>::identity().m.map(|r| [r.x, r.y, r.z]);
    for i in 0..3 {
        let mut mag = w[i][i].abs();
        let mut p = w[i][i];
        let mut row = i;
        for (k, wk) in w.iter().enumerate().skip(i + 1) {
            let t = wk[i].abs();
            if mag < t {
                p = wk[i];
                mag = t;
                row = k;
            }
        }
        if p == F::ZERO {
            return None;
        }
        w.swap(i, row);
        o.swap(i, row);
        let q = F::ONE / p;
        w[i] = w[i].map(|x| x * q);
        o[i] = o[i].map(|x| x * q);
        for k in 0..3 {
            if k != i {
                let q = -w[k][i];
                let (wi, oi) = (w[i], o[i]);
                for (d, s) in w[k].iter_mut().zip(wi) {
                    *d = *d + s * q;
                }
                for (d, s) in o[k].iter_mut().zip(oi) {
                    *d = *d + s * q;
                }
            }
        }
    }
    Some(Matrix33 { m: o.map(|r| Vect3::new(r[0], r[1], r[2])) })
}

fn to_f<M: Model>(c: [u8; 4]) -> Color32F<M> {
    let k = F::new(1.0 / 255.0);
    Color32F { r: F::from_int(c[0] as i32) * k, g: F::from_int(c[1] as i32) * k, b: F::from_int(c[2] as i32) * k, a: F::from_int(c[3] as i32) * k }
}

/// `VIRaster::CalcHitColor`: the three colours interpolated at the hit (by
/// the inverse of the basis of the triangle's edges and normal); `ca` alone
/// when the triangle is degenerate.
pub fn calc_hit_color<M: Model>(seg: &mut PickSegment<M>, a: &Vect3<M>, b: &Vect3<M>, c: &Vect3<M>, ca: [u8; 4], cb: [u8; 4], cc: [u8; 4]) -> bool {
    let hit = seg.hit_point();
    let mut m = Matrix33::<M>::identity();
    m.m[0].x = b.x - a.x;
    m.m[1].x = b.y - a.y;
    m.m[2].x = b.z - a.z;
    m.m[0].y = c.x - a.x;
    m.m[1].y = c.y - a.y;
    m.m[2].y = c.z - a.z;
    let mut pl = Plane::<M>::default();
    if pl.from_tri(a, b, c) != 0 {
        m.m[0].z = pl.n.x;
        m.m[1].z = pl.n.y;
        m.m[2].z = pl.n.z;
        if let Some(inv) = invert33(&m) {
            let (c0, c1, c2) = (to_f::<M>(ca), to_f::<M>(cb), to_f::<M>(cc));
            let k = |d1: F<M>, d2: F<M>| crate::particle::vm33(&Vect3::new(d1, d2, F::ZERO), &inv);
            let k0 = k(c1.r - c0.r, c2.r - c0.r);
            let k1 = k(c1.g - c0.g, c2.g - c0.g);
            let k2 = k(c1.b - c0.b, c2.b - c0.b);
            let k3 = k(c1.a - c0.a, c2.a - c0.a);
            let u = Vect3::new(hit.x - a.x, hit.y - a.y, hit.z - a.z);
            let dot = |k: &Vect3<M>| u.x * k.x + u.y * k.y + u.z * k.z;
            seg.hit_color = Color32F { r: c0.r + dot(&k0), g: c0.g + dot(&k1), b: c0.b + dot(&k2), a: c0.a + dot(&k3) };
            return true;
        }
    }
    seg.hit_color = to_f(ca);
    false
}

/// One strip of a prim buffer as the pick walks it: its box and its
/// vertices (object-space position, colour as `VIColor32`).
#[derive(Clone, Debug, Default)]
pub struct PickStrip<M: Model> {
    pub bbox: BBox<M>,
    pub material: i32,
    pub verts: Vec<(Vect3<M>, [u8; 4])>,
}

/// A prim buffer as the pick walks it.
#[derive(Clone, Debug, Default)]
pub struct PickBuffer<M: Model> {
    /// `VIPrimBufferType`: 1 VUNC (float), 2 PackVUNC, 3 PackVUNCG.
    pub kind: i32,
    pub bbox: BBox<M>,
    pub strips: Vec<PickStrip<M>>,
}

/// The strip walk every `PickPrimBuffer*` shares, over the decoded
/// vertices (`colors`, when given, replace each strip's own: the colour
/// buffer's, strip by strip).
pub fn pick_strips<M: Model>(seg: &mut PickSegment<M>, pb: &PickBuffer<M>, colors: Option<&[Vec<[u8; 4]>]>) -> bool {
    let mut hit = false;
    for (si, s) in pb.strips.iter().enumerate() {
        if cull_bbox(seg, &s.bbox) || s.verts.len() < 3 {
            continue;
        }
        let col = |k: usize| colors.and_then(|c| c.get(si)).and_then(|c| c.get(k).copied()).unwrap_or(s.verts[k].1);
        let mut flip = false;
        for k in 2..s.verts.len() {
            let (p0, p1, p2) = (&s.verts[k - 2].0, &s.verts[k - 1].0, &s.verts[k].0);
            let (c0, c1, c2) = (col(k - 2), col(k - 1), col(k));
            if !flip {
                if pick_tri(seg, s.material, p0, p1, p2) {
                    calc_hit_color(seg, p0, p1, p2, c0, c1, c2);
                    hit = true;
                }
            } else if pick_tri(seg, s.material, p1, p0, p2) {
                calc_hit_color(seg, p1, p0, p2, c1, c0, c2);
                hit = true;
            }
            flip = !flip;
        }
    }
    hit
}

/// `VIRaster::PickPrimBuffer`: the segment into the buffer's space under the
/// inverse of `model_view`, the buffer culled, the strips walked, the hit
/// brought back out, and the static lighting `static_light` added (alpha
/// multiplied).
pub fn pick_prim_buffer<M: Model>(seg: &mut PickSegment<M>, model_view: &Matrix44<M>, pb: &PickBuffer<M>, colors: Option<&[Vec<[u8; 4]>]>, static_light: &Color32F<M>) -> bool {
    // The game does not check the result (a singular matrix leaves what the
    // elimination reached); no placement is singular, so none here is a miss.
    let Some(inv) = Matrix44::invert(model_view) else { return false };
    let mut local = PickSegment::transformed(seg, &inv);
    let mut hit = false;
    // the colour-buffer form's switch has only PackVUNC's case
    let walks = colors.is_none() || pb.kind == 2;
    if !cull_bbox(&local, &pb.bbox) && walks && pick_strips(&mut local, pb, colors) {
        *seg = PickSegment::transformed(&local, model_view);
        hit = true;
    }
    if hit {
        seg.hit_color.r = seg.hit_color.r + static_light.r;
        seg.hit_color.g = seg.hit_color.g + static_light.g;
        seg.hit_color.b = seg.hit_color.b + static_light.b;
        seg.hit_color.a = seg.hit_color.a * static_light.a;
    }
    hit
}

/// `PickColor`: a packed colour's four bytes doubled into a `VIColor32`'s
/// unsigned chars (so 0x80, a packed 1.0, wraps to 0).
pub fn pack_color(k: [u8; 4]) -> [u8; 4] {
    k.map(|c| c.wrapping_shl(1))
}

/// A PackVUNC buffer's vertex block as the pick reads it: per strip a
/// 32-byte header, 32-byte vertices (`VIVect3S16` position at +4 times the
/// position scale, colour at +0x1C), a 16-byte tail. `prims` are the strips'
/// vertex counts and boxes, `mats` each strip's palette material.
pub fn decode_pack_vunc<M: Model>(verts: &[u8], prims: &[(i32, BBox<M>)], scale: F<M>, mats: &[i32]) -> Vec<PickStrip<M>> {
    let mut o = 0usize;
    let mut out = Vec::with_capacity(prims.len());
    for (k, &(nv, bbox)) in prims.iter().enumerate() {
        o += 32;
        let mut vs = Vec::with_capacity(nv.max(0) as usize);
        for _ in 0..nv.max(0) {
            let Some(v) = verts.get(o..o + 32) else { break };
            let s = |i: usize| F::from_int(i16::from_le_bytes([v[i], v[i + 1]]) as i32) * scale;
            vs.push((Vect3::new(s(4), s(6), s(8)), pack_color([v[0x1C], v[0x1D], v[0x1E], v[0x1F]])));
            o += 32;
        }
        o += 16;
        out.push(PickStrip { bbox, material: mats.get(k).copied().unwrap_or(-1), verts: vs });
    }
    out
}

/// A colour buffer's colours as the baked-colour pick reads them: per strip
/// `ceil((n + 4) / 4)` quadwords, vertex k's colour at +8 + 4k.
pub fn decode_color_tags(tags: &[u8], counts: &[i32]) -> Vec<Vec<[u8; 4]>> {
    let mut o = 0usize;
    counts
        .iter()
        .map(|&nv| {
            let nv = nv.max(0) as usize;
            let c = (0..nv).map(|k| tags.get(o + 8 + 4 * k..o + 12 + 4 * k).map_or([0; 4], |b| pack_color([b[0], b[1], b[2], b[3]]))).collect();
            o += (nv + 4).div_ceil(4) * 16;
            c
        })
        .collect()
}

/// A PrimBuffer record (0x1200) as the parser leaves it for the pick: each
/// face record a strip, its box bounded as `VIPrimBuffer::Unlock` does, and
/// the buffer's the union. Version 0 is float (VUNC: the colour through
/// `/255` and back, `(int)(f*255 + 0.5)`); later versions are packed, the
/// position scaled by 2^-p1, a colour stored `>> 1` and read back `<< 1`, and
/// type 4 (PackVUNCG) offset by `pretrans[group]`. Type 5 (PackVUNBW) picks
/// nothing, and neither does an unknown type.
pub fn read_pick_buffer<M: Model>(d: &[u8], version: u16, pretrans: &[Vect3<M>]) -> Option<PickBuffer<M>> {
    let rd = |o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
    let mut p = 0usize;
    let (kind, nfaces, scale) = if version == 0 {
        let n = rd(4)?;
        p += 12;
        (0, n, F::ONE)
    } else {
        if version > 1 {
            p += 4;
        }
        let (kind, nfaces, p1) = (rd(p)?, rd(p + 8)?, rd(p + 16)?);
        p += 28;
        (kind, nfaces, F::new((2.0f64.powi(-p1)) as f32))
    };
    let stride = match (version, kind) {
        (0, _) => 36,
        (_, 2) => 17,
        (_, 4) => 19,
        _ => return None,
    };
    if !(0..=100_000).contains(&nfaces) {
        return None;
    }
    let mut pb = PickBuffer::<M> { kind: if version == 0 { 1 } else if kind == 4 { 3 } else { 2 }, ..Default::default() };
    for f in 0..nfaces {
        let (count, material) = (rd(p)?, rd(p + 4)?);
        p += 8;
        if !(0..=200_000).contains(&count) || p + count as usize * stride > d.len() {
            return None;
        }
        let mut s = PickStrip { material, ..Default::default() };
        for v in 0..count as usize {
            let r = &d[p + v * stride..p + (v + 1) * stride];
            let (pos, col) = if version == 0 {
                let g = |k: usize| F::<M>::new(f32::from_le_bytes(r[k * 4..k * 4 + 4].try_into().unwrap()));
                let k = F::<M>::new(1.0 / 255.0);
                let back = |b: u8| ((F::<M>::from_int(b as i32) * k) * F::new(255.0) + F::new(0.5)).0 as i32 as u8;
                (Vect3::new(g(0), g(1), g(2)), [back(r[32]), back(r[33]), back(r[34]), back(r[35])])
            } else {
                let sv = |k: usize| F::<M>::from_int(i16::from_le_bytes([r[k * 2], r[k * 2 + 1]]) as i32) * scale;
                let mut pos = Vect3::new(sv(0), sv(1), sv(2));
                if kind == 4 {
                    let g = i16::from_le_bytes([r[17], r[18]]);
                    if let Some(t) = usize::try_from(g).ok().and_then(|g| pretrans.get(g)) {
                        pos += *t;
                    }
                }
                (pos, pack_color([r[13] >> 1, r[14] >> 1, r[15] >> 1, r[16] >> 1]))
            };
            s.bbox.bound(v == 0, &pos);
            s.verts.push((pos, col));
        }
        p += count as usize * stride;
        if f == 0 {
            pb.bbox = s.bbox;
        } else {
            pb.bbox = BBox::union(&pb.bbox, &s.bbox);
        }
        pb.strips.push(s);
    }
    Some(pb)
}

/// A colour buffer's colours (flat, in vertex order) split over a buffer's
/// strips; past its end, zero.
pub fn split_colors<M: Model>(flat: &[[u8; 4]], pb: &PickBuffer<M>) -> Vec<Vec<[u8; 4]>> {
    let mut k = 0usize;
    pb.strips
        .iter()
        .map(|s| {
            (0..s.verts.len())
                .map(|_| {
                    k += 1;
                    flat.get(k - 1).copied().unwrap_or([0; 4])
                })
                .collect()
        })
        .collect()
}
