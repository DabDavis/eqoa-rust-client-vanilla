//! `VIFrustum` (`vifrustum.cpp`: the camera's eight corners and the six
//! planes `CalcPlanes` makes of them) and `VIClipStack` (`viclipstack.cpp`):
//! the stack of clip volumes `VIScene::Render`'s walk pushes as it goes
//! through the portals, each a run of planes (normals outward) and a run of
//! points, the top alone tested (`CullByTop`, `ClipByTop`). Held to the decomp
//! by eqoa-port's oracle (ops 1700-1703).

use crate::fpu::{F, Model};
use crate::geom::{BBox, Plane, Sphere};
use crate::vect::Vect3;

/// `VIFrustum`: the view volume's eight corners (`VICamera::CalcFrustum`:
/// near top-left, top-right, bottom-right, bottom-left, then the same at the
/// far plane) and the six planes `CalcPlanes` makes of them, normals outward:
/// 0 near, 1 far, 2 left, 3 right, 4 top, 5 bottom.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Frustum<M: Model> {
    pub verts: [Vect3<M>; 8],
    pub planes: [Plane<M>; 6],
}

impl<M: Model> Frustum<M> {
    /// The frustum of these eight corners (`CalcFrustum`'s order).
    pub fn of_corners(verts: [Vect3<M>; 8]) -> Self {
        let mut f = Frustum { verts, planes: [Plane::default(); 6] };
        f.calc_planes();
        f
    }

    /// `CalcPlanes`: each face's plane through three of its corners, as the
    /// four are copied into `v` and the plane made of `v[2], v[1], v[0]`.
    pub fn calc_planes(&mut self) {
        let m = self.verts;
        let faces: [[usize; 3]; 6] = [[1, 2, 3], [4, 7, 6], [0, 3, 7], [5, 6, 2], [5, 1, 0], [2, 6, 7]];
        for (pl, f) in self.planes.iter_mut().zip(faces) {
            pl.from_tri(&m[f[0]], &m[f[1]], &m[f[2]]);
        }
    }
}

/// What `VIClipStack` reads of the camera (`VICamera`): its frustum, place,
/// view direction and far distance.
#[derive(Clone, Copy, Debug, Default)]
pub struct Camera<M: Model> {
    pub frustum: Frustum<M>,
    pub position: Vect3<M>,
    pub dir: Vect3<M>,
    pub far: F<M>,
}

/// `VIClipEntry`: where a volume's runs start and how long they are.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Entry {
    plane_index: usize,
    vert_index: usize,
    num_planes: usize,
    num_verts: usize,
}

/// `VIClipStack`: `Init`'s arrays (500 entries, 2000 points, 2000 planes),
/// grown past them where the console would write past the end; `ClipByTop`'s
/// two scratch rings (32 points each there) are its own.
#[derive(Clone, Debug)]
pub struct ClipStack<M: Model> {
    entries: Vec<Entry>,
    verts: Vec<Vect3<M>>,
    planes: Vec<Plane<M>>,
    num_entries: usize,
    num_planes: usize,
    num_verts: usize,
}

impl<M: Model> Default for ClipStack<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Model> ClipStack<M> {
    /// `Init`, then `Reset`.
    pub fn new() -> Self {
        ClipStack { entries: vec![Entry::default(); 500], verts: vec![Vect3::default(); 2000], planes: vec![Plane::default(); 2000], num_entries: 0, num_planes: 0, num_verts: 0 }
    }

    /// `Reset`: the three counts back to zero; nothing is freed.
    pub fn reset(&mut self) {
        self.num_entries = 0;
        self.num_planes = 0;
        self.num_verts = 0;
    }

    /// How many volumes are on the stack.
    pub fn depth(&self) -> usize {
        self.num_entries
    }

    fn entry(&mut self, i: usize) -> &mut Entry {
        if i >= self.entries.len() {
            self.entries.resize(i + 1, Entry::default());
        }
        &mut self.entries[i]
    }

    fn put_plane(&mut self, p: Plane<M>) {
        if self.num_planes >= self.planes.len() {
            self.planes.resize(self.num_planes + 1, Plane::default());
        }
        self.planes[self.num_planes] = p;
        self.num_planes += 1;
    }

    fn put_vert(&mut self, v: Vect3<M>) {
        if self.num_verts >= self.verts.len() {
            self.verts.resize(self.num_verts + 1, Vect3::default());
        }
        self.verts[self.num_verts] = v;
        self.num_verts += 1;
    }

    /// A volume begun: its runs start where the counts stand.
    fn begin(&mut self) {
        let (p, v, k) = (self.num_planes, self.num_verts, self.num_entries);
        let e = self.entry(k);
        e.plane_index = p;
        e.vert_index = v;
    }

    /// A volume ended: its counts derived from the indices written at its
    /// start, and the stack one deeper. Returns the volume's index.
    fn end(&mut self) -> usize {
        let (p, v, k) = (self.num_planes, self.num_verts, self.num_entries);
        let e = self.entry(k);
        e.num_planes = p - e.plane_index;
        e.num_verts = v - e.vert_index;
        self.num_entries = k + 1;
        k
    }

    /// `Pop`: the counts rewound by the top entry's; the entry is left, and a
    /// push after overwrites it.
    pub fn pop(&mut self) {
        let i = self.num_entries - 1;
        let e = self.entries[i];
        self.num_verts -= e.num_verts;
        self.num_planes -= e.num_planes;
        self.num_entries = i;
    }

    /// `GetTop`: the top volume's points and planes.
    pub fn top(&self) -> (&[Vect3<M>], &[Plane<M>]) {
        let e = &self.entries[self.num_entries - 1];
        (&self.verts[e.vert_index..e.vert_index + e.num_verts], &self.planes[e.plane_index..e.plane_index + e.num_planes])
    }

    /// `CullByTop(const VISphere &)`: outside when the centre is further
    /// from any one plane than the radius.
    pub fn cull_sphere(&self, s: &Sphere<M>) -> bool {
        let (_, planes) = self.top();
        planes.iter().any(|p| s.r < s.c.x * p.n.x + s.c.y * p.n.y + s.c.z * p.n.z + p.d)
    }

    /// `CullByTop(const VIBBox &)`: per plane, the corner furthest along the
    /// normal against zero.
    pub fn cull_bbox(&self, b: &BBox<M>) -> bool {
        let (_, planes) = self.top();
        planes.iter().any(|p| {
            let x = if F::ZERO <= p.n.x { b.min.x } else { b.max.x };
            let y = if F::ZERO <= p.n.y { b.min.y } else { b.max.y };
            let z = if F::ZERO <= p.n.z { b.min.z } else { b.max.z };
            F::ZERO < x * p.n.x + y * p.n.y + z * p.n.z + p.d
        })
    }

    /// `PushFrustum`: the camera's frustum planes 1..5 (the near one dropped)
    /// and five points, the eye and the four far corners: a pyramid from the
    /// eye.
    pub fn push_frustum(&mut self, cam: &Camera<M>) -> usize {
        self.begin();
        for i in 1..6 {
            self.put_plane(cam.frustum.planes[i]);
        }
        self.put_vert(cam.position);
        for i in 4..8 {
            self.put_vert(cam.frustum.verts[i]);
        }
        self.end()
    }

    /// `PushPortal(VICamera &, n, p)`: a portal seen from the camera, its
    /// polygon `p` (clipped): the frustum's plane 1 (the far plane), then a
    /// plane through the eye along each edge, and the eye and each corner
    /// pushed out along its ray from the eye to the far distance, measured
    /// along the view direction.
    pub fn push_portal(&mut self, cam: &Camera<M>, p: &[Vect3<M>]) -> usize {
        self.begin();
        self.put_plane(cam.frustum.planes[1]);
        self.put_vert(cam.position);
        let mut prev = p[p.len() - 1];
        for &c in p {
            let a = prev;
            prev = c;
            let mut pl = Plane::default();
            pl.from_tri(&a, &c, &cam.position);
            self.put_plane(pl);
            let mut d = c;
            d -= cam.position;
            let t = d.x * cam.dir.x + d.y * cam.dir.y + d.z * cam.dir.z;
            let mut v = d;
            v *= cam.far / t;
            v += cam.position;
            self.put_vert(v);
        }
        self.end()
    }

    /// `ClipByTop`: the polygon `v` clipped by each plane of the top volume
    /// in turn (`VIClipper::Clip`'s body, but a point exactly on a plane is
    /// dropped); None when nothing is left.
    pub fn clip_by_top(&mut self, v: &[Vect3<M>]) -> Option<Vec<Vect3<M>>> {
        let (_, planes) = self.top();
        let planes = planes.to_vec();
        let mut src: Vec<Vect3<M>> = v.to_vec();
        let mut dst: Vec<Vect3<M>> = Vec::with_capacity(32);
        for pl in &planes {
            dst.clear();
            let mut prev = src[src.len() - 1];
            let mut d_prev = pl.distance(&prev);
            for &cur in &src {
                let d_cur = pl.distance(&cur);
                if d_cur < F::ZERO {
                    if F::ZERO < d_prev {
                        dst.push(cut(&prev, &cur, d_prev / (d_prev - d_cur)));
                    }
                    dst.push(cur);
                } else if F::ZERO < d_cur && d_prev < F::ZERO {
                    dst.push(cut(&prev, &cur, d_prev / (d_prev - d_cur)));
                }
                prev = cur;
                d_prev = d_cur;
            }
            if dst.is_empty() {
                return None;
            }
            std::mem::swap(&mut src, &mut dst);
        }
        Some(src)
    }
}

/// `*dst = *v; *dst -= *prev; *dst *= t; *dst += *prev`.
fn cut<M: Model>(prev: &Vect3<M>, v: &Vect3<M>, t: F<M>) -> Vect3<M> {
    let mut d = *v;
    d -= *prev;
    d *= t;
    d += *prev;
    d
}
