//! `VICollBuffer` and `VICollide`'s swept-sphere path (decomp `vicollbuffer.cpp`,
//! `vicollide.cpp`, `viesfparse.cpp`'s `ParseCollBuffer`).
//!
//! A collision buffer is triangle STRIPS in groups, each group and each strip
//! carrying a bounding box. Three packings: float vertices (V), 16-bit vertices
//! times a scale (PackV), and 16-bit vertices with a pre-translation group and
//! a flora byte (PackVGF), the terrain's.

use crate::coll::{CollRay, CollSphere};
use crate::fpu::{F, Model, f};
use crate::geom::{BBox, Matrix44, Plane, Sphere, Vect4};
use crate::vect::Vect3;

pub const TYPE_V: i32 = 0;
pub const TYPE_PACK_V: i32 = 1;
pub const TYPE_PACK_VGF: i32 = 3;

#[derive(Clone, Debug, Default)]
pub struct CollGroup<M: Model> {
    pub num_prims: i32,
    pub num_verts: i32,
    pub bbox: BBox<M>,
}

#[derive(Clone, Debug, Default)]
pub struct CollPrim<M: Model> {
    pub num_verts: i32,
    pub mat: i32,
    pub bbox: BBox<M>,
}

#[derive(Clone, Debug, Default)]
pub struct CollBuffer<M: Model> {
    pub kind: i32,
    pub scale: F<M>,
    pub bbox: BBox<M>,
    pub groups: Vec<CollGroup<M>>,
    pub prims: Vec<CollPrim<M>>,
    /// TYPE_V's vertices.
    pub verts_f: Vec<Vect3<M>>,
    /// The packed types' vertices, with (group, flora) for PackVGF.
    pub verts_s: Vec<[i16; 3]>,
    pub vgroup: Vec<i8>,
    pub flora: Vec<i8>,
}

fn rd<T: Copy>(d: &[u8], p: &mut usize, n: usize, conv: impl Fn(&[u8]) -> T) -> T {
    let v = conv(&d[*p..*p + n]);
    *p += n;
    v
}
fn ri(d: &[u8], p: &mut usize) -> i32 {
    rd(d, p, 4, |b| i32::from_le_bytes(b.try_into().unwrap()))
}
fn rf<M: Model>(d: &[u8], p: &mut usize) -> F<M> {
    F::new(rd(d, p, 4, |b| f32::from_le_bytes(b.try_into().unwrap())))
}
fn rs(d: &[u8], p: &mut usize) -> i16 {
    rd(d, p, 2, |b| i16::from_le_bytes(b.try_into().unwrap()))
}

impl<M: Model> CollBuffer<M> {
    /// `ParseCollBuffer` over a 0x4200 record's body. `pretrans` is the zone's
    /// table when the buffer belongs to a zone (its bounds include it).
    pub fn parse(body: &[u8], version: i16, pretrans: &[Vect3<M>]) -> Option<Self> {
        let d = body;
        let mut p = 0;
        let packing = if version >= 2 { ri(d, &mut p) } else { 0 };
        let num_groups = ri(d, &mut p);
        let num_prims = ri(d, &mut p);
        let _num_verts = ri(d, &mut p);
        let bits = if version >= 2 { ri(d, &mut p) } else { 0 };
        // Init: a PackVG (2) is stored as PackVGF (3).
        let kind = if packing == 2 { TYPE_PACK_VGF } else { packing };
        if !matches!(kind, TYPE_V | TYPE_PACK_V | TYPE_PACK_VGF) {
            return None;
        }
        let mut b = CollBuffer::<M> { kind, ..Default::default() };
        // SetPacking: 1 / powf(2, n).
        b.scale = F::ONE / f::<M>(2.0).powf(F::new(bits as f32));
        let mut cur_group = -1;
        for _ in 0..num_prims {
            let nv = ri(d, &mut p);
            let grp = ri(d, &mut p);
            let mat = ri(d, &mut p);
            if cur_group != grp {
                if cur_group != -1 {
                    // EndPrimGroup is implicit: the next push starts a group.
                }
                b.groups.push(CollGroup::default());
                cur_group = grp;
            }
            b.prims.push(CollPrim { num_verts: 0, mat, bbox: BBox::default() });
            for _ in 0..nv {
                match (packing, kind) {
                    (0, _) => {
                        let v = Vect3::new(rf(d, &mut p), rf(d, &mut p), rf(d, &mut p));
                        b.verts_f.push(v);
                    }
                    (1, _) => {
                        b.verts_s.push([rs(d, &mut p), rs(d, &mut p), rs(d, &mut p)]);
                    }
                    (2, TYPE_PACK_VGF) => {
                        b.verts_s.push([rs(d, &mut p), rs(d, &mut p), rs(d, &mut p)]);
                        let g = rs(d, &mut p);
                        b.vgroup.push(g as i8);
                        b.flora.push(-1);
                    }
                    (3, TYPE_PACK_VGF) => {
                        b.verts_s.push([rs(d, &mut p), rs(d, &mut p), rs(d, &mut p)]);
                        let g = d[p] as i8;
                        let fl = d[p + 1] as i8;
                        p += 2;
                        b.vgroup.push(g);
                        b.flora.push(fl);
                    }
                    _ => return None,
                }
                b.prims.last_mut().unwrap().num_verts += 1;
            }
            // End(): the group counts its strip and vertices.
            let g = b.groups.last_mut().unwrap();
            g.num_prims += 1;
            g.num_verts += nv;
        }
        let _ = num_groups;
        b.calc_bvolumes(pretrans);
        Some(b)
    }

    fn vertex(&self, i: usize, pretrans: &[Vect3<M>]) -> Vect3<M> {
        match self.kind {
            TYPE_V => self.verts_f[i],
            _ => {
                let s = self.verts_s[i];
                let mut v = Vect3::new(F::new(s[0] as f32) * self.scale, F::new(s[1] as f32) * self.scale, F::new(s[2] as f32) * self.scale);
                if self.kind == TYPE_PACK_VGF
                    && let Some(t) = usize::try_from(self.vgroup[i] as i32).ok().and_then(|g| pretrans.get(g))
                {
                    v += *t;
                }
                v
            }
        }
    }

    /// `CalcBVolumes`: each strip's box, each group's, and the buffer's.
    fn calc_bvolumes(&mut self, pretrans: &[Vect3<M>]) {
        let (mut pi, mut vi) = (0usize, 0usize);
        for gi in 0..self.groups.len() {
            let mut gb = BBox::default();
            let mut fg = true;
            for _ in 0..self.groups[gi].num_prims {
                let mut pb = BBox::default();
                let mut fp = true;
                for _ in 0..self.prims[pi].num_verts {
                    let v = self.vertex(vi, pretrans);
                    gb.bound(fg, &v);
                    pb.bound(fp, &v);
                    fp = false;
                    fg = false;
                    vi += 1;
                }
                self.prims[pi].bbox = pb;
                pi += 1;
            }
            self.groups[gi].bbox = gb;
            self.bbox = if gi == 0 { gb } else { BBox::union(&self.bbox, &gb) };
        }
    }
}

/// `VICollSphere::Init(const VICollSphere &src, const VIMatrix44 &m)`: the
/// sweep carried into another space; the radius scaled by row 0's length.
pub fn transform_sphere<M: Model>(src: &CollSphere<M>, m: &Matrix44<M>) -> CollSphere<M> {
    let mut s = CollSphere::<M>::default();
    let r = &mut s.ray;
    r.start = m.mul_point(&src.ray.start);
    r.end = m.mul_point(&src.ray.end);
    let r0 = m.m[0];
    s.radius = src.radius * (r0.x * r0.x + r0.y * r0.y + r0.z * r0.z).sqrt();
    let mut b = m.apply(&Vect4::new(src.ray.dir2.x, src.ray.dir2.y, src.ray.dir2.z, F::ZERO));
    b.normalize();
    s.ray.dir2 = b.xyz();
    // Precalc (the sphere's: delta, dir, length, and the radius' reciprocal).
    let start = s.ray.start;
    let end = s.ray.end;
    let dir2 = s.ray.dir2;
    let radius = s.radius;
    s.init(start, end, radius);
    s.ray.dir2 = dir2;
    s.ray.hit_time = src.ray.hit_time;
    s.ray.hit_idx = src.ray.hit_idx;
    s.ray.hit_normal = src.ray.hit_normal;
    s
}

/// `VICollide`: the current world transform and its inverse, and the 32
/// pre-translations a PackVGF buffer's groups index.
#[derive(Clone, Debug)]
pub struct Collide<M: Model> {
    pub world: Matrix44<M>,
    pub inv_world: Matrix44<M>,
    pub pretrans: [Vect3<M>; 32],
}

impl<M: Model> Default for Collide<M> {
    fn default() -> Self {
        Collide { world: Matrix44::identity(), inv_world: Matrix44::identity(), pretrans: [Vect3::default(); 32] }
    }
}

impl<M: Model> Collide<M> {
    pub fn set_world(&mut self, m: &Matrix44<M>) {
        self.world = *m;
        self.inv_world = Matrix44::invert_affine(&self.world);
    }

    /// Copies `v` over the first entries (the guest's `memcpy`).
    pub fn set_pretranslations(&mut self, v: &[Vect3<M>]) {
        for (d, s) in self.pretrans.iter_mut().zip(v) {
            *d = *s;
        }
    }

    /// `CollideObj(VICollSphere &, int)`: into the buffer's space, collide,
    /// and bring the contact normal back out through the world transform.
    pub fn collide_obj(&mut self, s: &mut CollSphere<M>, buf: &CollBuffer<M>) -> bool {
        let mut local = transform_sphere(s, &self.inv_world);
        if self.collide_buffer(&mut local, buf) {
            s.ray.hit_time = local.ray.hit_time;
            s.ray.hit_idx = local.ray.hit_idx;
            let n = local.ray.hit_normal;
            let mut wn = self.world.apply(&Vect4::new(n.x, n.y, n.z, F::ZERO));
            wn.normalize();
            s.ray.hit_normal = wn.xyz();
            return true;
        }
        false
    }

    /// `Collide(VICollSphere &, VICollBuffer &)`: the three walks are one body
    /// with the vertex fetch swapped, so one function here.
    pub fn collide_buffer(&self, s: &mut CollSphere<M>, b: &CollBuffer<M>) -> bool {
        let mut hit = false;
        if cull_bbox(s, &b.bbox) {
            return false;
        }
        if s.ray.length == F::ZERO {
            return false;
        }
        let pt = &self.pretrans[..];
        let (mut vert, mut prim) = (0usize, 0usize);
        for g in &b.groups {
            if !cull_bbox(s, &g.bbox) {
                for _ in 0..g.num_prims {
                    let pr = &b.prims[prim];
                    if !cull_bbox(s, &pr.bbox) {
                        let mut n = pr.num_verts - 2;
                        let mut c = b.vertex(vert, pt);
                        vert += 1;
                        let mut d = b.vertex(vert, pt);
                        vert += 1;
                        let mut flip = false;
                        while n != 0 {
                            let a = c;
                            c = d;
                            d = b.vertex(vert, pt);
                            let h = if !flip { collide_tri(s, pr.mat, &a, &c, &d) } else { collide_tri(s, pr.mat, &c, &a, &d) };
                            hit |= h;
                            flip = !flip;
                            vert += 1;
                            n -= 1;
                        }
                    } else {
                        vert += pr.num_verts as usize;
                    }
                    prim += 1;
                }
            } else {
                prim += g.num_prims as usize;
                vert += g.num_verts as usize;
            }
        }
        hit
    }
}

/// `Cull(VICollSphere &, VIBBox &)`: true when the sweep, both ends, lies
/// wholly outside one face of the box grown by the radius.
pub fn cull_bbox<M: Model>(s: &CollSphere<M>, b: &BBox<M>) -> bool {
    let mut mx = b.max;
    mx.x += s.radius;
    mx.y += s.radius;
    mx.z += s.radius;
    let mut mn = b.min;
    mn.x -= s.radius;
    mn.y -= s.radius;
    mn.z -= s.radius;
    let (a, e) = (s.ray.start, s.ray.end);
    (a.x > mx.x && e.x > mx.x)
        || (a.y > mx.y && e.y > mx.y)
        || (a.z > mx.z && e.z > mx.z)
        || (a.x < mn.x && e.x < mn.x)
        || (a.y < mn.y && e.y < mn.y)
        || (a.z < mn.z && e.z < mn.z)
}

/// `Cull(VICollSphere &, VISphere &)`: 1 when the sweep cannot reach the
/// sphere before its current hit.
pub fn cull_sphere<M: Model>(s: &CollSphere<M>, v: &Sphere<M>) -> bool {
    let r = s.radius + v.r;
    let mut l = s.ray.start;
    l -= v.c;
    let mut cull = true;
    let c = l.x * l.x + l.y * l.y + l.z * l.z - r * r;
    if F::ZERO < c {
        let d = &s.ray.dir;
        let b = l.x * d.x + l.y * d.y + l.z * d.z;
        if b < F::ZERO {
            let e = b * b - c;
            if F::ZERO < e {
                let t = -(b + e.sqrt()) * s.ray.inv_length;
                if t < s.ray.hit_time {
                    cull = false;
                }
            }
        }
    } else {
        cull = false;
    }
    cull
}

fn point_in_triangle<M: Model>(p: &Vect3<M>, pl: &Plane<M>, v0: &Vect3<M>, v1: &Vect3<M>, v2: &Vect3<M>) -> bool {
    for (a, b) in [(v0, v1), (v1, v2), (v2, v0)] {
        let e = Vect3::new(b.x - a.x, b.y - a.y, b.z - a.z);
        let d = Vect3::new(p.x - b.x, p.y - b.y, p.z - b.z);
        let c = Vect3::new(e.y * d.z - e.z * d.y, -e.x * d.z + e.z * d.x, e.x * d.y - e.y * d.x);
        if F::ZERO < c.x * pl.n.x + c.y * pl.n.y + c.z * pl.n.z {
            return false;
        }
    }
    true
}

/// A swept sphere against one triangle: the face, then the three edges (each
/// edge test falls through to its first vertex). Only a face hit writes the
/// hit index.
pub fn collide_tri<M: Model>(s: &mut CollSphere<M>, idx: i32, v0: &Vect3<M>, v1: &Vect3<M>, v2: &Vect3<M>) -> bool {
    let mut hit = false;
    let mut pl = Plane::default();
    if pl.from_tri(v0, v1, v2) != 0 {
        let dir = s.ray.dir;
        if pl.n.x * dir.x + pl.n.y * dir.y + pl.n.z * dir.z <= F::ZERO {
            let mut ds = pl.distance(&s.ray.start);
            if F::ZERO < ds {
                let de = pl.distance(&s.ray.end) - s.radius;
                ds = ds - s.radius;
                if de < F::ZERO {
                    if ds != de {
                        let mut t = ds / (ds - de);
                        #[allow(clippy::neg_cmp_op_on_partial_ord)]
                        if !(t < s.ray.hit_time) {
                            return hit;
                        }
                        let hp = if F::ZERO < t {
                            let dl = s.ray.delta;
                            Vect3::new(
                                dl.x * t + s.ray.start.x - pl.n.x * s.radius,
                                dl.y * t + s.ray.start.y - pl.n.y * s.radius,
                                dl.z * t + s.ray.start.z - pl.n.z * s.radius,
                            )
                        } else {
                            let k = s.radius - ds;
                            t = F::ZERO;
                            Vect3::new(s.ray.start.x - pl.n.x * k, s.ray.start.y - pl.n.y * k, s.ray.start.z - pl.n.z * k)
                        };
                        if point_in_triangle(&hp, &pl, v0, v1, v2) {
                            s.ray.hit_time = t;
                            s.ray.hit_normal = pl.n;
                            s.ray.hit_idx = idx;
                            return true;
                        }
                    }
                    hit |= collide_edge(s, v0, v1);
                    hit |= collide_edge(s, v1, v2);
                    hit |= collide_edge(s, v2, v0);
                }
            }
        }
    }
    hit
}

/// `Collide(VICollSphere &, VIVect3 &)`: the sweep against a vertex.
pub fn collide_point<M: Model>(s: &mut CollSphere<M>, p: &Vect3<M>) -> bool {
    let mut l = s.ray.start;
    l -= *p;
    let b = l.dot(&s.ray.dir);
    let mut hit = false;
    if b < F::ZERO {
        let c = l.x * l.x + l.y * l.y + l.z * l.z - s.radius * s.radius;
        let e = b * b - c;
        if F::ZERO < e {
            let tn = -(b + e.sqrt());
            let t = tn * s.ray.inv_length;
            if t < s.ray.hit_time {
                if F::ZERO < t {
                    s.ray.hit_time = t;
                    let mut n = s.ray.dir;
                    n *= tn;
                    n += s.ray.start;
                    n -= *p;
                    n *= s.inv_radius;
                    s.ray.hit_normal = n;
                } else {
                    s.ray.hit_time = F::ZERO;
                    let mut n = s.ray.start;
                    n -= *p;
                    n.normalize();
                    s.ray.hit_normal = n;
                }
                hit = true;
            }
        }
    }
    hit
}

/// `Collide(VICollSphere &, VIVect3 &v0, VIVect3 &v1)`: the sweep against an
/// edge (a cylinder), then against `v0`.
pub fn collide_edge<M: Model>(s: &mut CollSphere<M>, v0: &Vect3<M>, v1: &Vect3<M>) -> bool {
    let mut hit = false;
    let mut over = false;
    let mut e = *v1;
    e -= *v0;
    let mut pl = Plane::<M>::default();
    pl.n = Vect3::cross(&s.ray.dir, &e);
    let lsq = pl.n.dot(&pl.n);
    if F::ZERO < lsq {
        let inv_c = F::ONE / lsq.sqrt();
        pl.n.x = pl.n.x * inv_c;
        pl.n.y = pl.n.y * inv_c;
        pl.n.z = pl.n.z * inv_c;
        pl.d = -pl.n.dot(v0);
        let dist = pl.distance(&s.ray.start);
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(-s.radius < dist) || !(dist < s.radius) {
            return hit;
        }
        let q = Vect3::new(s.ray.start.x - v0.x, s.ray.start.y - v0.y, s.ray.start.z - v0.z);
        let cp = Vect3::cross(&q, &e);
        let mut fdist = -cp.dot(&pl.n) * inv_c;
        'edge: {
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(F::ZERO < fdist) {
                break 'edge;
            }
            if e.normalize() == 0 {
                break 'edge;
            }
            let q = Vect3::cross(&pl.n, &e);
            let chord = (s.radius * s.radius - dist * dist).sqrt();
            fdist = fdist + chord / s.ray.dir.dot(&q);
            if fdist < F::ZERO {
                over = true;
                fdist = F::ZERO;
            }
            let t = fdist * s.ray.inv_length;
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(t < s.ray.hit_time) {
                break 'edge;
            }
            let mut q = s.ray.delta;
            q *= t;
            q += s.ray.start;
            let k = e.x * q.x + e.y * q.y + e.z * q.z;
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(F::ZERO < k - v0.dot(&e)) {
                break 'edge;
            }
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !(k - v1.dot(&e) < F::ZERO) {
                break 'edge;
            }
            let mut n = q;
            n -= *v0;
            let k = n.x * e.x + n.y * e.y + n.z * e.z;
            n.sub_scaled(&e, k);
            if !over {
                n *= s.inv_radius;
            } else {
                n.normalize();
            }
            s.ray.hit_normal = n;
            s.ray.hit_time = t;
            hit = true;
        }
        hit |= collide_point(s, v0);
    } else {
        hit = collide_point(s, v0);
    }
    hit
}

// ------------------------------------------------------------ the ray half
//
// The decomp writes each ray function out beside its sphere twin (the bodies
// repeated, not shared); so here.

/// `VICylinder`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Cylinder<M: Model> {
    pub base: Vect3<M>,
    /// Unit.
    pub dir: Vect3<M>,
    pub length: F<M>,
    pub radius: F<M>,
}

/// `VICollRay::Init(const VICollRay &src, const VIMatrix44 &mat)`: the ray
/// carried into another space, its hit state kept.
pub fn transform_ray<M: Model>(src: &CollRay<M>, m: &Matrix44<M>) -> CollRay<M> {
    let mut r = CollRay::<M>::default();
    r.start = m.mul_point(&src.start);
    r.end = m.mul_point(&src.end);
    let mut b = m.apply(&Vect4::new(src.dir2.x, src.dir2.y, src.dir2.z, F::ZERO));
    b.normalize();
    r.dir2 = b.xyz();
    r.precalc();
    r.hit_time = src.hit_time;
    r.hit_idx = src.hit_idx;
    r.hit_normal = src.hit_normal;
    r
}

impl<M: Model> Collide<M> {
    /// `CollideObj(VICollRay &, int)`.
    pub fn collide_ray_obj(&mut self, r: &mut CollRay<M>, buf: &CollBuffer<M>) -> bool {
        let mut local = transform_ray(r, &self.inv_world);
        if self.collide_ray_buffer(&mut local, buf) {
            r.hit_time = local.hit_time;
            r.hit_idx = local.hit_idx;
            let n = local.hit_normal;
            let mut wn = self.world.apply(&Vect4::new(n.x, n.y, n.z, F::ZERO));
            wn.normalize();
            r.hit_normal = wn.xyz();
            return true;
        }
        false
    }

    /// `Collide(VICollRay &, VICollBuffer &)`: the three ray walks, one body.
    pub fn collide_ray_buffer(&self, r: &mut CollRay<M>, b: &CollBuffer<M>) -> bool {
        let mut hit = false;
        if cull_ray_bbox(r, &b.bbox) {
            return false;
        }
        if r.length == F::ZERO {
            return false;
        }
        let pt = &self.pretrans[..];
        let (mut vert, mut prim) = (0usize, 0usize);
        for g in &b.groups {
            if !cull_ray_bbox(r, &g.bbox) {
                for _ in 0..g.num_prims {
                    let pr = &b.prims[prim];
                    if !cull_ray_bbox(r, &pr.bbox) {
                        let mut n = pr.num_verts - 2;
                        let mut c = b.vertex(vert, pt);
                        vert += 1;
                        let mut d = b.vertex(vert, pt);
                        vert += 1;
                        let mut flip = false;
                        while n != 0 {
                            let a = c;
                            c = d;
                            d = b.vertex(vert, pt);
                            let h = if !flip { collide_ray_tri(r, pr.mat, &a, &c, &d) } else { collide_ray_tri(r, pr.mat, &c, &a, &d) };
                            hit |= h;
                            flip = !flip;
                            vert += 1;
                            n -= 1;
                        }
                    } else {
                        vert += pr.num_verts as usize;
                    }
                    prim += 1;
                }
            } else {
                prim += g.num_prims as usize;
                vert += g.num_verts as usize;
            }
        }
        hit
    }
}

/// `Cull(VICollRay &, VIBBox &)`: both ends outside one face.
pub fn cull_ray_bbox<M: Model>(r: &CollRay<M>, b: &BBox<M>) -> bool {
    let (a, e, mx, mn) = (r.start, r.end, b.max, b.min);
    (a.x > mx.x && e.x > mx.x)
        || (a.y > mx.y && e.y > mx.y)
        || (a.z > mx.z && e.z > mx.z)
        || (a.x < mn.x && e.x < mn.x)
        || (a.y < mn.y && e.y < mn.y)
        || (a.z < mn.z && e.z < mn.z)
}

/// `Cull(VICollRay &, VISphere &)`: 1 unless the ray starts inside or
/// reaches the sphere before its current hit.
pub fn cull_ray_sphere<M: Model>(r: &CollRay<M>, v: &Sphere<M>) -> bool {
    let mut l = r.start;
    l -= v.c;
    let mut cull = true;
    let c = l.x * l.x + l.y * l.y + l.z * l.z - v.r * v.r;
    if F::ZERO < c {
        let d = &r.dir;
        let b = l.x * d.x + l.y * d.y + l.z * d.z;
        if b < F::ZERO {
            let e = b * b - c;
            if F::ZERO < e {
                let t = -(b + e.sqrt()) * r.inv_length;
                if t < r.hit_time {
                    cull = false;
                }
            }
        }
    } else {
        cull = false;
    }
    cull
}

/// `Collide(VICollRay &, int, v0, v1, v2)`: a front face crossed from
/// above to below, inside the triangle, before the current hit.
pub fn collide_ray_tri<M: Model>(r: &mut CollRay<M>, idx: i32, v0: &Vect3<M>, v1: &Vect3<M>, v2: &Vect3<M>) -> bool {
    let mut pl = Plane::default();
    if pl.from_tri(v0, v1, v2) != 0 {
        let dir = r.dir;
        if pl.n.x * dir.x + pl.n.y * dir.y + pl.n.z * dir.z <= F::ZERO {
            let ds = pl.distance(&r.start);
            if F::ZERO < ds {
                let de = pl.distance(&r.end);
                if de < F::ZERO {
                    let t = ds / (ds - de);
                    if t < r.hit_time {
                        let dl = r.delta;
                        let hp = Vect3::new(dl.x * t + r.start.x, dl.y * t + r.start.y, dl.z * t + r.start.z);
                        if point_in_triangle(&hp, &pl, v0, v1, v2) {
                            r.hit_time = t;
                            r.hit_normal = pl.n;
                            r.hit_idx = idx;
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// `Collide(VICollRay &, const VISphere &)`: the point/sphere quadratic
/// once more; a ray starting inside hits at 0 with the outward normal.
pub fn collide_ray_sphere<M: Model>(r: &mut CollRay<M>, v: &Sphere<M>) -> bool {
    let mut l = r.start;
    l -= v.c;
    let b = l.dot(&r.dir);
    if b < F::ZERO {
        let c = l.x * l.x + l.y * l.y + l.z * l.z - v.r * v.r;
        let e = b * b - c;
        if F::ZERO < e {
            let tn = -(b + e.sqrt());
            let t = tn * r.inv_length;
            if t < r.hit_time {
                if F::ZERO < t {
                    r.hit_time = t;
                    let mut n = r.dir;
                    n *= tn;
                    n += r.start;
                    n -= v.c;
                    n *= F::ONE / v.r;
                    r.hit_normal = n;
                } else {
                    r.hit_time = F::ZERO;
                    let mut n = r.start;
                    n -= v.c;
                    n.normalize();
                    r.hit_normal = n;
                }
                return true;
            }
        }
    }
    false
}

/// `CollideNoCaps(VICollRay &, const VICylinder &)`: the ray against a
/// cylinder's side between its ends.
pub fn collide_ray_no_caps<M: Model>(r: &mut CollRay<M>, c: &Cylinder<M>) -> bool {
    let cross = |a: &Vect3<M>, b: &Vect3<M>| Vect3::new(a.y * b.z - a.z * b.y, a.z * b.x - a.x * b.z, a.x * b.y - a.y * b.x);
    let mut pl = Plane::<M>::default();
    pl.n = cross(&r.dir, &c.dir);
    let k = pl.n.dot(&pl.n);
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(F::ZERO < k) {
        return false;
    }
    let k = F::ONE / k.sqrt();
    pl.n = Vect3::new(pl.n.x * k, pl.n.y * k, pl.n.z * k);
    pl.d = -pl.n.dot(&c.base);
    let d = pl.distance(&r.start);
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(-c.radius < d) || !(d < c.radius) {
        return false;
    }
    let v = Vect3::new(r.start.x - c.base.x, r.start.y - c.base.y, r.start.z - c.base.z);
    let h = cross(&v, &c.dir);
    let t = -h.dot(&pl.n) * k;
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(F::ZERO < t) {
        return false;
    }
    let v = cross(&pl.n, &c.dir);
    let half = (c.radius * c.radius - d * d).sqrt();
    let mut t = t + half / r.dir.dot(&v);
    if t < F::ZERO {
        t = F::ZERO;
    }
    let t = t * r.inv_length;
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(t < r.hit_time) {
        return false;
    }
    let mut v = r.delta;
    v *= t;
    v += r.start;
    let u = v.dot(&c.dir) - c.base.dot(&c.dir);
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(F::ZERO <= u) || !(u <= c.length) {
        return false;
    }
    let mut n = v;
    n -= c.base;
    let along = n.dot(&c.dir);
    n = Vect3::new(n.x - c.dir.x * along, n.y - c.dir.y * along, n.z - c.dir.z * along);
    n.normalize();
    r.hit_normal = n;
    r.hit_time = t;
    true
}

/// `VICapsule`: a cylinder from `base` along the unit `dir`, `length` long,
/// with a sphere at each end.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Capsule<M: Model> {
    pub base: Vect3<M>,
    pub dir: Vect3<M>,
    pub length: F<M>,
    pub radius: F<M>,
}

fn sub_scaled<M: Model>(a: &mut Vect3<M>, b: &Vect3<M>, k: F<M>) {
    a.x = a.x - b.x * k;
    a.y = a.y - b.y * k;
    a.z = a.z - b.z * k;
}

impl<M: Model> Cylinder<M> {
    /// `VICylinder::PointInSolid`: within the radius of the axis, between
    /// the ends.
    pub fn point_in_solid(&self, p: &Vect3<M>) -> bool {
        let mut d = *p;
        d -= self.base;
        let t = d.dot(&self.dir);
        sub_scaled(&mut d, &self.dir, t);
        if d.dot(&d) <= self.radius * self.radius {
            let s = self.dir.dot(p) - self.base.dot(&self.dir);
            return F::ZERO <= s && s <= self.length;
        }
        false
    }
}

impl<M: Model> Capsule<M> {
    /// `VICSprite::SetDefaults`' `m_capsule` (which `Copy` hands on and
    /// nothing changes): every character collides as it, in its actor's
    /// space, from the feet 2 up and 0.5 round.
    pub fn csprite() -> Self {
        Capsule { base: Vect3::default(), dir: Vect3::of(0.0, 1.0, 0.0), length: f(2.0), radius: f(0.5) }
    }

    /// `VICapsule::PointInSolid`: the cylinder, or past an end within its
    /// sphere.
    pub fn point_in_solid(&self, p: &Vect3<M>) -> bool {
        let mut d = *p;
        d -= self.base;
        let t = d.dot(&self.dir);
        sub_scaled(&mut d, &self.dir, t);
        if d.dot(&d) <= self.radius * self.radius {
            let s = self.dir.dot(p) - self.base.dot(&self.dir);
            if F::ZERO <= s && s <= self.length {
                return true;
            }
            let r2 = self.radius * self.radius;
            let mut d = *p;
            d -= self.base;
            if d.dot(&d) <= r2 {
                return true;
            }
            let mut d = self.base;
            d.x = d.x + self.dir.x * self.length;
            d.y = d.y + self.dir.y * self.length;
            d.z = d.z + self.dir.z * self.length;
            d -= *p;
            return d.dot(&d) <= r2;
        }
        false
    }
}

/// `Collide(VICollSphere &, VICapsule &)`: the capsule grown by the
/// sweep's radius and the sweep run as its bare ray. Starting outside: the
/// tube and both caps, the nearest winning. Starting inside the grown
/// capsule: nothing when inside the true cylinder, the tube when inside the
/// grown one, else the near cap.
pub fn collide_capsule<M: Model>(s: &mut CollSphere<M>, c: &Capsule<M>) -> bool {
    let start = s.ray.start;
    let rad = s.radius;
    let ex = Capsule { radius: c.radius + rad, ..*c };
    let cyl = |r: F<M>| Cylinder { base: c.base, dir: c.dir, length: c.length, radius: r };
    let ray = &mut s.ray;
    if ex.point_in_solid(&start) {
        if cyl(c.radius).point_in_solid(&start) {
            return false;
        }
        let grown = cyl(c.radius + rad);
        if grown.point_in_solid(&start) {
            return collide_ray_no_caps(ray, &grown);
        }
        let ds = start.dot(&c.dir);
        let db = c.base.dot(&c.dir);
        let mut end = Sphere { c: c.base, r: c.radius + rad };
        #[allow(clippy::neg_cmp_op_on_partial_ord)]
        if !(ds - db < F::ZERO) {
            end.c.x = end.c.x + c.dir.x * c.length;
            end.c.y = end.c.y + c.dir.y * c.length;
            end.c.z = end.c.z + c.dir.z * c.length;
        }
        return collide_ray_sphere(ray, &end);
    }
    let mut hit = collide_ray_no_caps(ray, &cyl(c.radius + rad));
    let mut end = Sphere { c: c.base, r: c.radius + rad };
    hit |= collide_ray_sphere(ray, &end);
    end.c.x = end.c.x + c.dir.x * c.length;
    end.c.y = end.c.y + c.dir.y * c.length;
    end.c.z = end.c.z + c.dir.z * c.length;
    hit |= collide_ray_sphere(ray, &end);
    hit
}

impl<M: Model> Collide<M> {
    /// `CollideObj(VICollSphere &, VICapsule &)`: as the buffer form.
    pub fn collide_capsule_obj(&mut self, s: &mut CollSphere<M>, c: &Capsule<M>) -> bool {
        let mut local = transform_sphere(s, &self.inv_world);
        if collide_capsule(&mut local, c) {
            s.ray.hit_time = local.ray.hit_time;
            s.ray.hit_idx = local.ray.hit_idx;
            let n = local.ray.hit_normal;
            let mut wn = self.world.apply(&Vect4::new(n.x, n.y, n.z, F::ZERO));
            wn.normalize();
            s.ray.hit_normal = wn.xyz();
            return true;
        }
        false
    }
}
