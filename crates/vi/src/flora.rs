//! The radial flora: the plants on the ground round the camera, as
//! `VIRadialFloraSystem` grows and draws them (decomp
//! `viradialflorasystem.cpp`, every function exact, phase 61), with what it
//! reads of the scene: each zone's flora tables (`VIZoneRadialFlora`, which
//! `ParseZoneFlora` reads and `Precalc` readies), the triangle list it plants
//! on (`VIQueryTriList`, `viquerytrilist.cpp`, clipped by `VIClipper`,
//! `viclipper.cpp`) and the frustum's two culls (`VIFrustum`, `vifrustum.cpp`).
//!
//! A 3 x 3 grid of swatches, each a 40-unit sub-cell of the world grid's
//! cells (50 to a cell side, clamped to [-25, 24]), follows the camera
//! (`update`). A new swatch asks the scene for the up-facing ground in its box
//! (`QueryIntersection` with a filter that takes no actor, so the rooms' own
//! geometry: the terrain), each triangle cut to the box. A triangle whose
//! flora set is not -1 grows the set's weight times its area in plants (at
//! most 500 a swatch), each one's model drawn by a binary search over the
//! set's cumulative weights, a random point, heading and scale, tilted to the
//! ground when its distribution's id has bit 0. The random numbers are
//! newlib's `rand()`, seeded off the sub-cell (`srand(sub.x * 100 + sub.y)`), so
//! a swatch always grows the same plants. `render` keeps the plants within 40
//! units that the frustum keeps (700 at most), sorted by distance and drawn
//! farthest first, each faded from 1 at 10 units to 0 at 40 (the vertex
//! alpha `VIRStrip6` draws them with).
//!
//! Where the game indexes past an array (a flora set the triangle's byte names
//! that the zone does not have), this skips the triangle: the PS2 reads
//! whatever lies there.

use crate::csprite::Rand;
use crate::fpu::{F, Model, vimax};
use crate::geom::{BBox, Matrix44, Plane, Sphere};
use crate::vect::Vect3;

/// `VIRadialFloraModel`: a flora sprite (`m_a`, the scene's sprite), its prim
/// buffer and material palette (`m_b`, `m_c`, which `Precalc` copies out of
/// the sprite) and its bounding sphere.
#[derive(Clone, Copy, Debug, Default)]
pub struct FloraModel<M: Model> {
    pub sprite: i32,
    pub prim: i32,
    pub pal: i32,
    pub sphere: Sphere<M>,
}

/// `VIRadialFloraDist`: one model of a set. The id's bit 0 tilts a plant to
/// the ground; `model` is the model's index (`m_set`); the weight, the scale
/// and its jitter are the file's (`m_p10`, `m_p14`, `m_p18`); `start` and
/// `end` are the normalised cumulative weight round it (`Precalc`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Dist<M: Model> {
    pub id: u32,
    pub start: F<M>,
    pub end: F<M>,
    pub model: i32,
    pub weight: F<M>,
    pub scale: F<M>,
    pub jitter: F<M>,
}

/// `VIRadialFloraSet`: its first distribution (`m_model`), how many
/// (`m_dist`), and their total weight (`m_c`, `Precalc`'s).
#[derive(Clone, Copy, Debug, Default)]
pub struct Set<M: Model> {
    pub first: i32,
    pub count: i32,
    pub weight: F<M>,
}

/// `VIZoneRadialFlora`: a zone's flora tables.
#[derive(Clone, Debug, Default)]
pub struct ZoneFlora<M: Model> {
    pub models: Vec<FloraModel<M>>,
    pub dists: Vec<Dist<M>>,
    pub sets: Vec<Set<M>>,
}

impl<M: Model> ZoneFlora<M> {
    /// `Precalc`: each model's prim buffer, palette and sphere, out of its
    /// sprite (`sprite(i)` answers them; none without a raster), then each
    /// set's total weight and each distribution's normalised [start, end).
    pub fn precalc(&mut self, sprite: impl Fn(i32) -> Option<(i32, i32, Sphere<M>)>) {
        for m in &mut self.models {
            if let Some((prim, pal, sphere)) = sprite(m.sprite) {
                m.prim = prim;
                m.pal = pal;
                m.sphere = sphere;
            }
        }
        for i in 0..self.sets.len() {
            let mut c = F::ZERO;
            for j in self.sets[i].first..self.sets[i].first + self.sets[i].count {
                if let Some(d) = self.dist(j) {
                    c += d.weight;
                }
            }
            self.sets[i].weight = c;
        }
        for i in 0..self.sets.len() {
            let inv = F::ONE / self.sets[i].weight;
            let mut acc = F::ZERO;
            for j in self.sets[i].first..self.sets[i].first + self.sets[i].count {
                let Some(d) = usize::try_from(j).ok().and_then(|j| self.dists.get_mut(j)) else { continue };
                d.start = acc * inv;
                acc += d.weight;
                d.end = acc * inv;
            }
        }
    }

    fn dist(&self, j: i32) -> Option<&Dist<M>> {
        usize::try_from(j).ok().and_then(|j| self.dists.get(j))
    }
}

/// `VIFrustum`'s six planes, normals outward.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frustum<M: Model> {
    pub planes: [Plane<M>; 6],
}

impl<M: Model> Frustum<M> {
    /// `Cull(const VISphere &)`: outside a plane by more than the radius.
    pub fn cull_sphere(&self, s: &Sphere<M>) -> bool {
        self.planes.iter().any(|pl| {
            let d = s.c.x * pl.n.x + s.c.y * pl.n.y + s.c.z * pl.n.z + pl.d;
            s.r < d
        })
    }

    /// `Cull(const VIBBox &, const VIMatrix44 &)`: every corner, transformed,
    /// on the outside of one plane.
    pub fn cull_bbox(&self, b: &BBox<M>, m: &Matrix44<M>) -> bool {
        let v = b.corners(m);
        self.planes.iter().any(|pl| v.iter().all(|p| !(p.x * pl.n.x + p.y * pl.n.y + p.z * pl.n.z + pl.d < F::ZERO)))
    }
}

/// `VIQueryTri`: a triangle the query kept, and the index it came with (the
/// flora set, for the terrain).
#[derive(Clone, Copy, Debug, Default)]
pub struct QueryTri<M: Model> {
    pub actor: i32,
    pub v0: Vect3<M>,
    pub v1: Vect3<M>,
    pub v2: Vect3<M>,
}

/// `VIQueryTriList`: at most `max` triangles, each facing within the angle of
/// `dir` and cut to the box's six outward planes.
#[derive(Clone, Debug)]
pub struct QueryTriList<M: Model> {
    pub tris: Vec<QueryTri<M>>,
    pub max: usize,
    pub dir: Vect3<M>,
    pub cos_angle: F<M>,
    pub planes: [Plane<M>; 6],
}

impl<M: Model> QueryTriList<M> {
    /// `Init(n, b, dir, angle)`: the box's planes (`StoreOutward`), the
    /// direction and the cosine of the angle in degrees.
    pub fn new(n: usize, b: &BBox<M>, dir: Vect3<M>, angle: F<M>) -> Self {
        QueryTriList { tris: Vec::with_capacity(n), max: n, dir, cos_angle: (angle * F::new(0.017453292)).cosf(), planes: store_outward(b) }
    }

    /// `Intersect(actor, a, b, c)`: the triangle, if its plane faces the
    /// direction closely enough, clipped to the box and fanned out.
    pub fn intersect(&mut self, actor: i32, a: &Vect3<M>, b: &Vect3<M>, c: &Vect3<M>) {
        if self.tris.len() == self.max {
            return;
        }
        let mut pl = Plane::default();
        pl.from_tri(a, b, c);
        if pl.n.dot(&self.dir) < self.cos_angle {
            return;
        }
        let Some(out) = clip(&[*a, *b, *c], &self.planes) else { return };
        for i in 2..out.len() {
            if self.tris.len() >= self.max {
                break;
            }
            self.tris.push(QueryTri { actor, v0: out[0], v1: out[i - 1], v2: out[i] });
        }
    }
}

/// `VIBBox::StoreOutward`: the six planes, normals out of the box.
pub fn store_outward<M: Model>(b: &BBox<M>) -> [Plane<M>; 6] {
    let (o, z) = (F::ONE, F::ZERO);
    let pl = |x: F<M>, y: F<M>, zz: F<M>, d: F<M>| Plane { n: Vect3::new(x, y, zz), d };
    [
        pl(-o, z, z, b.min.x),
        pl(o, z, z, -b.max.x),
        pl(z, -o, z, b.min.y),
        pl(z, o, z, -b.max.y),
        pl(z, z, -o, b.min.z),
        pl(z, z, o, -b.max.z),
    ]
}

/// `VIClipper::Clip`: Sutherland-Hodgman against each plane in turn, a
/// negative distance inside and a point on a plane kept; None when nothing
/// is left.
pub fn clip<M: Model>(v: &[Vect3<M>], planes: &[Plane<M>]) -> Option<Vec<Vect3<M>>> {
    let mut cur: Vec<Vect3<M>> = v.to_vec();
    for p in planes {
        let mut out = Vec::with_capacity(cur.len() + 1);
        let mut prev = *cur.last()?;
        let mut d_prev = p.distance(&prev);
        for &c in &cur {
            let d_cur = p.distance(&c);
            let cross = |out: &mut Vec<Vect3<M>>| {
                let t = d_prev / (d_prev - d_cur);
                let mut x = c;
                x -= prev;
                x *= t;
                x += prev;
                out.push(x);
            };
            if d_cur < F::ZERO {
                if F::ZERO < d_prev {
                    cross(&mut out);
                }
                out.push(c);
            } else if F::ZERO < d_cur {
                if d_prev < F::ZERO {
                    cross(&mut out);
                }
            } else {
                out.push(c);
            }
            prev = c;
            d_prev = d_cur;
        }
        if out.is_empty() {
            return None;
        }
        cur = out;
    }
    Some(cur)
}

/// `VIRadialFloraLoc`: a world-grid cell (row, column), a 40-unit sub-cell
/// of it (row, column, each in [-25, 24]), and the zone the cell's centre is
/// in (-1 off the grid).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loc {
    pub zone: i32,
    pub cell: [i32; 2],
    pub sub: [i32; 2],
}

impl Default for Loc {
    fn default() -> Self {
        Loc { zone: -1, cell: [0; 2], sub: [0; 2] }
    }
}

/// `VIRadialFloraInst`: one plant, its world transform, its bounding sphere
/// and its model's prim buffer and palette.
#[derive(Clone, Copy, Debug, Default)]
pub struct Inst<M: Model> {
    pub world: Matrix44<M>,
    pub sphere: Sphere<M>,
    pub prim: i32,
    pub pal: i32,
}

/// `VIRadialFloraSwatch`.
#[derive(Clone, Debug, Default)]
pub struct Swatch<M: Model> {
    pub bbox: BBox<M>,
    pub loc: Loc,
    pub insts: Vec<Inst<M>>,
}

/// `VIRadialFloraSort`: a plant to draw, its distance, swatch and index.
#[derive(Clone, Copy, Debug, Default)]
pub struct Sort<M: Model> {
    pub dist: F<M>,
    pub swatch: i32,
    pub inst: i32,
}

/// What the flora reads of the scene.
pub trait FloraScene<M: Model> {
    fn grid_rows(&self) -> i32;
    fn grid_cols(&self) -> i32;
    /// `GetWorldGridCenter(a, b)`: column a, row b.
    fn grid_center(&self, a: i32, b: i32) -> Vect3<M>;
    /// `CalcWorldGridOffset(v)`: the column, the row and the offset in it.
    fn grid_offset(&self, v: &Vect3<M>) -> (i32, i32, Vect3<M>);
    fn world_bbox(&self) -> BBox<M>;
    /// `m_world.FindLeaf(v)`'s zone.
    fn find_zone(&self, v: &Vect3<M>) -> i32;
    fn flora(&self, zone: i32) -> Option<&ZoneFlora<M>>;
    /// `QueryIntersection(box, VIRadialFloraFilter, list)`: the rooms' own
    /// geometry in the box (the filter takes no actor).
    fn query(&mut self, b: &BBox<M>, l: &mut QueryTriList<M>);
    /// `GetTime`, ms.
    fn time(&self) -> i64;
}

/// One plant `render` draws: its model view's world half (the caller
/// multiplies by the view), prim buffer, palette and the fade.
#[derive(Clone, Copy, Debug)]
pub struct Drawn<M: Model> {
    pub world: Matrix44<M>,
    pub prim: i32,
    pub pal: i32,
    pub alpha: F<M>,
}

/// `VIRadialFloraSystem`.
#[derive(Clone, Debug)]
pub struct System<M: Model> {
    pub loc: Loc,
    pub swatches: [Option<Swatch<M>>; 9],
    pub sorted: Vec<Sort<M>>,
    pub rebuild: bool,
    /// newlib's `rand()` (`srand`'s state), as the flora seeds and draws it.
    pub rand: Rand,
}

impl<M: Model> Default for System<M> {
    fn default() -> Self {
        System { loc: Loc::default(), swatches: Default::default(), sorted: Vec::new(), rebuild: true, rand: Rand(1) }
    }
}

/// `VIRAND`: ten bits of `rand()`, a fraction in [0, 1].
fn virand<M: Model>(r: &mut Rand) -> F<M> {
    F::from_int(r.rand() & 0x3FF) * F::new(1.0 / 1023.0)
}


impl<M: Model> System<M> {
    /// `Reset`: every swatch let go and the grid built again at the next
    /// update.
    pub fn reset(&mut self) {
        self.swatches = Default::default();
        self.sorted.clear();
        self.rebuild = true;
    }

    /// `srand(seed)`: newlib's state is the seed.
    fn srand(&mut self, seed: i32) {
        self.rand = Rand(seed as u32 as u64);
    }

    /// `Update(at)`: the 3 x 3 grid round the viewer's sub-cell. When the
    /// location moved the swatches still inside slide to their new slot and
    /// the rest go; every empty slot whose cell is on the world grid gets a
    /// new swatch. The sub-cell walk wraps at +/-25 into the next cell
    /// unless the world is one row or one column.
    pub fn update(&mut self, sc: &mut impl FloraScene<M>, v: &Vect3<M>) {
        let single = !(sc.grid_rows() != 1 && sc.grid_cols() != 1);
        let loc = self.calc_radial_loc(sc, *v);
        if self.rebuild || loc.zone != self.loc.zone || loc.sub != self.loc.sub || loc.cell != self.loc.cell {
            self.rebuild = false;
            let mut grid: [Option<Swatch<M>>; 9] = Default::default();
            let d0 = (self.loc.sub[0] - loc.sub[0]) + (self.loc.cell[0] - loc.cell[0]) * 50;
            let d1 = (self.loc.sub[1] - loc.sub[1]) + (self.loc.cell[1] - loc.cell[1]) * 50;
            let k = d0 * 3 + d1;
            let mut i = 0i32;
            for r in 0..3 {
                for c in 0..3 {
                    if let Some(s) = self.swatches[i as usize].take()
                        && r + d0 >= 0
                        && r + d0 < 3
                        && c + d1 >= 0
                        && c + d1 < 3
                    {
                        grid[(k + i) as usize] = Some(s);
                    }
                    i += 1;
                }
            }
            self.swatches = grid;
        }
        let mut i = 0usize;
        let mut l = Loc { zone: -1, cell: [loc.cell[0], 0], sub: [loc.sub[0] - 1, 0] };
        if !single && l.sub[0] < -25 {
            l.cell[0] -= 1;
            l.sub[0] += 50;
        }
        for _ in 0..3 {
            l.sub[1] = loc.sub[1] - 1;
            l.cell[1] = loc.cell[1];
            if !single && l.sub[1] < -25 {
                l.cell[1] -= 1;
                l.sub[1] += 50;
            }
            for _ in 0..3 {
                if self.swatches[i].is_none() && l.cell[0] >= 0 && l.cell[0] < sc.grid_rows() && l.cell[1] >= 0 && l.cell[1] < sc.grid_cols() {
                    self.calc_zone_index(sc, &mut l);
                    self.create_swatch(sc, i, l);
                }
                l.sub[1] += 1;
                if !single && l.sub[1] >= 25 {
                    l.sub[1] = -25;
                    l.cell[1] += 1;
                }
                i += 1;
            }
            l.sub[0] += 1;
            if !single && l.sub[0] >= 25 {
                l.sub[0] = -25;
                l.cell[0] += 1;
            }
        }
        self.loc = loc;
    }

    /// `Render(cam)` (with `DrawSwatch`): every swatch the frustum keeps
    /// queues its plants within 40 units that the frustum keeps (700 at
    /// most), sorted by distance (`QSortCompare`); they are drawn from the
    /// last, the farthest, faded out past 10 units.
    pub fn render(&mut self, frustum: &Frustum<M>, eye: &Vect3<M>) -> Vec<Drawn<M>> {
        self.sorted.clear();
        let id = Matrix44::identity();
        for i in 0..9 {
            if let Some(s) = &self.swatches[i]
                && !frustum.cull_bbox(&s.bbox, &id)
            {
                Self::draw_swatch(&mut self.sorted, i as i32, s, frustum, eye);
            }
        }
        // qsort with QSortCompare: below, equal, above (not stable; distances
        // that tie are as rare as two equal floats)
        self.sorted.sort_by(|a, b| {
            if a.dist < b.dist {
                std::cmp::Ordering::Less
            } else if a.dist == b.dist {
                std::cmp::Ordering::Equal
            } else {
                std::cmp::Ordering::Greater
            }
        });
        let mut out = Vec::with_capacity(self.sorted.len());
        for s in self.sorted.iter().rev() {
            let Some(inst) = self.swatches[s.swatch as usize].as_ref().and_then(|w| w.insts.get(s.inst as usize)) else { continue };
            let mut a = F::ONE;
            if F::new(10.0) < s.dist {
                a = F::ONE - (s.dist - F::new(10.0)) * F::new(1.0 / 30.0);
            }
            out.push(Drawn { world: inst.world, prim: inst.prim, pal: inst.pal, alpha: a });
        }
        out
    }

    /// `DrawSwatch(i, cam)`.
    fn draw_swatch(sorted: &mut Vec<Sort<M>>, i: i32, s: &Swatch<M>, frustum: &Frustum<M>, eye: &Vect3<M>) {
        for (j, inst) in s.insts.iter().enumerate() {
            if sorted.len() >= 700 {
                break;
            }
            let p = inst.world.m[3].xyz();
            let (cx, cy, cz) = (eye.x, eye.y, eye.z);
            let d = (cx - p.x) * (cx - p.x) + (cy - p.y) * (cy - p.y) + (cz - p.z) * (cz - p.z);
            if d < F::new(1600.0) && !frustum.cull_sphere(&inst.sphere) {
                sorted.push(Sort { dist: d.sqrt(), swatch: i, inst: j as i32 });
            }
        }
    }

    /// `CreateSwatch(i, loc)`: its box spans the world's height over its
    /// 40-unit sub-cell; then its plants.
    fn create_swatch(&mut self, sc: &mut impl FloraScene<M>, i: usize, loc: Loc) {
        let wb = sc.world_bbox();
        let c = sc.grid_center(loc.cell[1], loc.cell[0]);
        let forty = F::new(40.0);
        let mut b = BBox::default();
        b.min.x = c.x + F::from_int(loc.sub[1]) * forty;
        b.min.y = wb.min.y;
        b.min.z = c.z + F::from_int(loc.sub[0]) * forty;
        b.max = b.min;
        b.max.y = wb.max.y;
        b.max.x += forty;
        b.max.z += forty;
        self.swatches[i] = Some(Swatch { bbox: b, loc, insts: Vec::new() });
        self.create_flora_insts(sc, i);
    }

    /// `CreateFloraInsts(i)`.
    fn create_flora_insts(&mut self, sc: &mut impl FloraScene<M>, i: usize) {
        let (loc, bbox) = {
            let s = self.swatches[i].as_ref().unwrap();
            (s.loc, s.bbox)
        };
        self.srand(loc.sub[0] * 100 + loc.sub[1]);
        if sc.flora(loc.zone).is_none_or(|f| f.sets.is_empty()) {
            return;
        }
        let mut q = QueryTriList::new(100, &bbox, Vect3::new(F::ZERO, F::ONE, F::ZERO), F::new(30.0));
        sc.query(&bbox, &mut q);
        let flora = sc.flora(loc.zone).unwrap().clone();
        let mut insts: Vec<Inst<M>> = Vec::with_capacity(500);
        let mut mid = 0i32;
        for t in &q.tris {
            if insts.len() >= 500 {
                break;
            }
            let set = t.actor;
            if set == -1 {
                continue;
            }
            let Some(fs) = usize::try_from(set).ok().and_then(|s| flora.sets.get(s)).copied() else { continue };
            let mut e1 = t.v1;
            e1 -= t.v0;
            let mut e2 = t.v2;
            e2 -= t.v0;
            let c = Vect3::cross(&e2, &e1);
            let mut n = c;
            n.normalize();
            let num = (c.dot(&n) * F::new(0.5) * fs.weight).to_int();
            for _ in 0..num.max(0) {
                if insts.len() >= 500 {
                    break;
                }
                let r = virand::<M>(&mut self.rand);
                let mut lo = fs.first;
                let mut hi = lo + fs.count - 1;
                while lo <= hi {
                    mid = (lo + hi) >> 1;
                    let Some(d) = flora.dist(mid) else { break };
                    if r >= d.start && r < d.end {
                        break;
                    }
                    if r >= d.end {
                        lo = mid + 1;
                    } else {
                        hi = mid - 1;
                    }
                }
                let Some(d) = flora.dist(mid).copied() else { continue };
                let Some(model) = usize::try_from(d.model).ok().and_then(|m| flora.models.get(m)).copied() else { continue };
                // VIRAND without its parentheses: p18 times the raw ten bits
                // first, the scale after, then the 2
                let raw = F::from_int(self.rand.rand() & 0x3FF);
                let jitter = d.jitter + F::ONE - d.jitter * raw * F::new(1.0 / 1023.0) * F::new(2.0);
                let mut scale = d.scale * jitter;
                scale = vimax(F::ZERO, scale);
                let raw = F::from_int(self.rand.rand() & 0x3FF);
                let mut hpr = Vect3::new(F::new(6.2831855) * raw * F::new(1.0 / 1023.0), F::ZERO, F::ZERO);
                if d.id & 1 != 0 {
                    let mut e1 = Vect3::default();
                    e1.hpr(hpr.x, F::ZERO, F::ZERO);
                    let mut e2 = Vect3::cross(&n, &e1);
                    e2.normalize();
                    let e1 = Vect3::cross(&e2, &n);
                    hpr.y = -e1.y.atan2f((F::ONE - e1.y * e1.y).sqrt());
                    hpr.z = e2.y.atan2f((F::ONE - e2.y * e2.y).sqrt());
                }
                let mut u = virand::<M>(&mut self.rand);
                let mut v = virand::<M>(&mut self.rand);
                if u + v > F::ONE {
                    u = F::ONE - u;
                    v = F::ONE - v;
                }
                let w = F::ONE - u - v;
                let mut pos = Vect3::new(t.v0.x * w, t.v0.y * w, t.v0.z * w);
                pos.add_scaled(&t.v1, u);
                pos.add_scaled(&t.v2, v);
                let world = Matrix44::transform(&hpr, scale, &pos);
                let sphere = Sphere { c: world.mul_point(&model.sphere.c), r: scale * model.sphere.r };
                insts.push(Inst { world, sphere, prim: model.prim, pal: model.pal });
            }
        }
        self.swatches[i].as_mut().unwrap().insts = insts;
        self.srand(sc.time() as i32);
    }

    /// `CalcRadialLoc(p, loc)`: the cell the point is in and its 40-unit
    /// sub-cell: the offset over 40 truncated, less one where the offset is
    /// negative (so a negative whole multiple of 40 lands a sub-cell below its
    /// floor), clamped to [-25, 24] unless the world is one row or column; no
    /// zone off the grid.
    pub fn calc_radial_loc(&self, sc: &impl FloraScene<M>, p: Vect3<M>) -> Loc {
        let (col, row, off) = sc.grid_offset(&p);
        let mut loc = Loc { zone: -1, cell: [row, col], sub: [0; 2] };
        let forty = F::new(40.0);
        loc.sub[0] = (off.z / forty).to_int();
        if off.z < F::ZERO {
            loc.sub[0] -= 1;
        }
        loc.sub[1] = (off.x / forty).to_int();
        if off.x < F::ZERO {
            loc.sub[1] -= 1;
        }
        if sc.grid_rows() != 1 && sc.grid_cols() != 1 {
            loc.sub[0] = loc.sub[0].clamp(-25, 24);
            loc.sub[1] = loc.sub[1].clamp(-25, 24);
        }
        if col >= 0 && col < sc.grid_cols() && row >= 0 && row < sc.grid_rows() {
            self.calc_zone_index(sc, &mut loc);
        } else {
            loc.zone = -1;
        }
        loc
    }

    /// `CalcZoneIndex(loc)`: the zone the cell's centre is in.
    fn calc_zone_index(&self, sc: &impl FloraScene<M>, loc: &mut Loc) {
        let v = sc.grid_center(loc.cell[1], loc.cell[0]);
        loc.zone = sc.find_zone(&v);
    }
}
