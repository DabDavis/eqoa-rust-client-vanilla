//! `vi::flora` against the decomp's radial flora, bit for bit (`Ieee`):
//! eqoa-port's `build/oracle_flora` compiles viradialflorasystem.cpp over a
//! scene in raw memory (VIScene's grid helpers, VIQueryTriList, VIClipper and
//! VIFrustum are libsupport's own; the query hands the list a triangle soup in
//! order; the leaf lookup is a zone per grid cell; the clock is the caller's)
//! and a recording raster, with newlib's rand(). The same script runs on both
//! sides: Precalc over three zones' tables, a walk across sub-cells and a cell
//! boundary into another zone (each Update's location and swatches, and every
//! plant's transform, sphere, prim and palette), Render along it (every plant
//! drawn, in order: model view, prim, palette, fade), a jump away and back, a
//! Reset, and a world one row high (no wrap, no clamp). The soup's triangles
//! are cut by the swatch boxes, tilted past the 30 degrees, wound away, and
//! more than a list holds; one zone's plants are dense enough for Render's
//! 700; a scale jitter past 1 clamps plants to 0. A planted difference (one
//! few triangles' flora sets) must not compare equal.
//!
//! SKIPs without the oracle (`make oracle` in eqoa-port).

mod common;
use common::{Oracle, bytes};
use vi::flora::{Dist, FloraModel, FloraScene, Frustum, QueryTriList, Set, System, ZoneFlora};
use vi::fpu::{F, Ieee};
use vi::geom::{BBox, Matrix44, Plane, Sphere};
use vi::vect::Vect3;
use vi::world::Grid;

type M = Ieee;

/// The oracle's scene, on our side.
struct Fake {
    grid: Grid<M>,
    map: Vec<i32>,
    zones: Vec<ZoneFlora<M>>,
    soup: Vec<(i32, [Vect3<M>; 3])>,
    now: i64,
}

impl FloraScene<M> for Fake {
    fn grid_rows(&self) -> i32 {
        self.grid.rows
    }
    fn grid_cols(&self) -> i32 {
        self.grid.cols
    }
    fn grid_center(&self, a: i32, b: i32) -> Vect3<M> {
        self.grid.center(a, b)
    }
    fn grid_offset(&self, v: &Vect3<M>) -> (i32, i32, Vect3<M>) {
        self.grid.offset(v)
    }
    fn world_bbox(&self) -> BBox<M> {
        self.grid.bbox
    }
    fn find_zone(&self, v: &Vect3<M>) -> i32 {
        let (col, row, _) = self.grid.offset(v);
        let col = col.clamp(0, self.grid.cols - 1);
        let row = row.clamp(0, self.grid.rows - 1);
        self.map[(row * self.grid.cols + col) as usize]
    }
    fn flora(&self, zone: i32) -> Option<&ZoneFlora<M>> {
        usize::try_from(zone).ok().and_then(|z| self.zones.get(z))
    }
    fn query(&mut self, _: &BBox<M>, l: &mut QueryTriList<M>) {
        for (idx, v) in &self.soup {
            l.intersect(*idx, &v[0], &v[1], &v[2]);
        }
    }
    fn time(&self) -> i64 {
        self.now
    }
}

#[derive(Default)]
struct Wire(Vec<u8>);

impl Wire {
    fn i(&mut self, v: i32) {
        self.0.extend(v.to_le_bytes());
    }
    fn f(&mut self, v: f32) {
        self.0.extend(v.to_le_bytes());
    }
    fn v(&mut self, v: &Vect3<M>) {
        for x in [v.x, v.y, v.z] {
            self.f(x.0);
        }
    }
}

struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl Rd<'_> {
    fn take(&mut self, n: usize) -> &[u8] {
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        s
    }
    fn i(&mut self) -> i32 {
        i32::from_le_bytes(self.take(4).try_into().unwrap())
    }
    fn f(&mut self) -> u32 {
        u32::from_le_bytes(self.take(4).try_into().unwrap())
    }
}

type Tables = Vec<(Vec<i32>, Vec<(u32, i32, f32, f32, f32)>, Vec<(i32, i32)>)>;

/// Per zone: the models' sprites, the dists (id, model, weight, scale,
/// jitter) and the sets (first dist, count).
fn tables() -> Tables {
    vec![
        // Odus's numbers (weight 0.02..0.05, scale 1, jitter 0.2), one tilted;
        // a second set of other scales
        (vec![0, 1, 2, 3], vec![(0, 0, 0.05, 1.0, 0.2), (1, 1, 0.02, 1.0, 0.2), (0, 2, 0.04, 1.0, 0.2), (1, 3, 0.1, 0.7, 0.0), (3, 0, 0.3, 1.5, 0.5)], vec![(0, 3), (3, 2)]),
        // dense (Render's 700), a dist of weight 0, a jitter past 1
        (vec![4, 5, 6], vec![(1, 0, 0.6, 2.0, 0.3), (0, 1, 0.0, 1.0, 0.1), (2, 2, 0.4, 0.5, 1.2)], vec![(0, 2), (2, 1)]),
        // no sets: no plants, the swatch's seed all it draws
        (vec![7], vec![], vec![]),
    ]
}

/// A height field over x 1680..2300, z 860..1300 on a 7.3 grid that the
/// swatch boxes cut, steep in places, every 13th triangle wound away; each
/// triangle's flora set -1, 0 or 1.
fn soup(plant: bool) -> Vec<(i32, [Vect3<M>; 3])> {
    let h = |x: f32, z: f32| 30.0 + 6.0 * (x * 0.05).sin() * (z * 0.07).cos() + 1.2 * (x * 0.31 + z * 0.17).sin();
    let p = |x: f32, z: f32| Vect3::of(x, h(x, z), z);
    let (s, x0, z0) = (7.3f32, 1680.37f32, 860.21f32);
    let mut out = Vec::new();
    let mut k = 0u32;
    for i in 0..85 {
        for j in 0..60 {
            let (xa, xb) = (x0 + i as f32 * s, x0 + (i + 1) as f32 * s);
            let (za, zb) = (z0 + j as f32 * s, z0 + (j + 1) as f32 * s);
            for tri in [[p(xa, za), p(xb, za), p(xa, zb)], [p(xb, za), p(xb, zb), p(xa, zb)]] {
                k += 1;
                let set = match k.wrapping_mul(2_654_435_761) >> 29 {
                    0 | 1 => -1,
                    2..=4 => 0,
                    _ => 1,
                };
                let tri = if k % 13 == 0 { [tri[0], tri[2], tri[1]] } else { tri };
                out.push((set, tri));
            }
        }
    }
    // under the walk's first swatches (x 1900, z 1000): some face up
    if plant {
        for k in (3600..3720).step_by(5) {
            out[k].0 = if out[k].0 == 0 { 1 } else { 0 };
        }
    }
    out
}

/// A camera at `eye` facing heading `h`: a 90-degree frustum (outward
/// normals, 0.5 to 300) and some view matrix.
fn camera(eye: Vect3<M>, h: f32) -> (Frustum<M>, Matrix44<M>) {
    let fw = [h.sin(), 0.0, h.cos()];
    let rt = [h.cos(), 0.0, -h.sin()];
    let up = [0.0f32, 1.0, 0.0];
    let e = [eye.x.0, eye.y.0, eye.z.0];
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let side = |a: [f32; 3], s: f32| {
        let k = std::f32::consts::FRAC_1_SQRT_2;
        let n = [(a[0] * s - fw[0]) * k, (a[1] * s - fw[1]) * k, (a[2] * s - fw[2]) * k];
        (n, -dot(n, e))
    };
    let planes = [side(rt, 1.0), side(rt, -1.0), side(up, 1.0), side(up, -1.0), ([-fw[0], -fw[1], -fw[2]], dot(fw, e) + 0.5), (fw, -dot(fw, e) - 300.0)];
    let mut f = Frustum::<M>::default();
    for (k, (n, d)) in planes.into_iter().enumerate() {
        f.planes[k] = Plane { n: Vect3::of(n[0], n[1], n[2]), d: F::new(d) };
    }
    let view = Matrix44::transform(&Vect3::of(h, 0.2, 0.0), F::new(1.0), &Vect3::of(-e[0], -e[1], -e[2]));
    (f, view)
}

fn sprites() -> Vec<(i32, i32, Sphere<M>)> {
    (0..8).map(|i| (100 + i, 200 + i, Sphere { c: Vect3::of(0.0, 0.4 + 0.1 * i as f32, 0.05 * i as f32), r: F::new(0.5 + 0.1 * i as f32) })).collect()
}

/// Init on both sides: the grid, the zones' tables, Precalc's results
/// compared. Returns our scene and system.
fn init(o: &mut Oracle, rows: i32, cols: i32, map: Vec<i32>, plant: bool) -> Result<(Fake, System<M>), String> {
    let origin = Vect3::of(1000.0, 0.0, 1000.0);
    let bbox = BBox { min: Vect3::of(0.0, -150.0, 0.0), max: Vect3::of(2000.0 * cols as f32, 420.0, 2000.0 * rows as f32) };
    let grid = Grid { bbox, rows, cols, subdiv: 4, cell_size: F::new(2000.0), origin, regions: Vec::new() };
    let mut w = Wire::default();
    w.i(rows);
    w.i(cols);
    w.i(4);
    w.f(2000.0);
    w.v(&origin);
    w.v(&bbox.min);
    w.v(&bbox.max);
    let t = tables();
    w.i(t.len() as i32);
    for &z in &map {
        w.i(z);
    }
    let sp = sprites();
    w.i(sp.len() as i32);
    for (prim, pal, s) in &sp {
        w.i(*prim);
        w.i(*pal);
        w.0.extend_from_slice(bytes(s));
    }
    let mut zones = Vec::new();
    for (models, dists, sets) in &t {
        w.i(models.len() as i32);
        let mut zf = ZoneFlora::<M>::default();
        for &m in models {
            w.i(m);
            zf.models.push(FloraModel { sprite: m, ..Default::default() });
        }
        w.i(dists.len() as i32);
        for &(id, model, weight, scale, jitter) in dists {
            w.i(id as i32);
            w.i(model);
            w.f(weight);
            w.f(scale);
            w.f(jitter);
            zf.dists.push(Dist { id, model, weight: F::new(weight), scale: F::new(scale), jitter: F::new(jitter), ..Default::default() });
        }
        w.i(sets.len() as i32);
        for &(first, count) in sets {
            w.i(first);
            w.i(count);
            zf.sets.push(Set { first, count, ..Default::default() });
        }
        zf.precalc(|s| sp.get(s as usize).copied());
        zones.push(zf);
    }
    let res = o.call(1400, &w.0);
    let mut r = Rd { b: &res, p: 0 };
    for (z, zf) in zones.iter().enumerate() {
        for (k, m) in zf.models.iter().enumerate() {
            let theirs = (r.i(), r.i(), r.i(), [r.f(), r.f(), r.f(), r.f()]);
            let ours = (m.sprite, m.prim, m.pal, [m.sphere.c.x.bits(), m.sphere.c.y.bits(), m.sphere.c.z.bits(), m.sphere.r.bits()]);
            if theirs != ours {
                return Err(format!("zone {z} model {k}: oracle {theirs:?} ours {ours:?}"));
            }
        }
        for (k, d) in zf.dists.iter().enumerate() {
            let theirs = (r.f(), r.f(), r.f(), r.i(), r.f(), r.f(), r.f());
            let ours = (d.id, d.start.bits(), d.end.bits(), d.model, d.weight.bits(), d.scale.bits(), d.jitter.bits());
            if theirs != ours {
                return Err(format!("zone {z} dist {k}: oracle {theirs:?} ours {ours:?}"));
            }
        }
        for (k, s) in zf.sets.iter().enumerate() {
            let theirs = (r.i(), r.i(), r.f());
            let ours = (s.first, s.count, s.weight.bits());
            if theirs != ours {
                return Err(format!("zone {z} set {k}: oracle {theirs:?} ours {ours:?}"));
            }
        }
    }
    // the oracle's soup is never planted: ours may be
    let mut w = Wire::default();
    let theirs = soup(false);
    w.i(theirs.len() as i32);
    for (idx, v) in &theirs {
        w.i(*idx);
        for p in v {
            w.v(p);
        }
    }
    o.call(1401, &w.0);
    Ok((Fake { grid, map, zones, soup: soup(plant), now: 0 }, System::default()))
}

/// One Update on both sides, everything after it compared; the plants made.
fn update(o: &mut Oracle, fake: &mut Fake, sys: &mut System<M>, now: i64, at: Vect3<M>, what: &str) -> Result<usize, String> {
    let mut w = Wire::default();
    w.0.extend(now.to_le_bytes());
    w.v(&at);
    let res = o.call(1402, &w.0);
    fake.now = now;
    sys.update(fake, &at);
    let mut r = Rd { b: &res, p: 0 };
    if r.i() != 0 {
        return Err(format!("{what}: Update failed"));
    }
    let loc = |r: &mut Rd| (r.i(), [r.i(), r.i()], [r.i(), r.i()]);
    let theirs = loc(&mut r);
    if theirs != (sys.loc.zone, sys.loc.cell, sys.loc.sub) {
        return Err(format!("{what}: loc oracle {theirs:?} ours {:?}", sys.loc));
    }
    let mut plants = 0;
    for i in 0..9 {
        let present = r.i() != 0;
        let ours = sys.swatches[i].as_ref();
        if present != ours.is_some() {
            return Err(format!("{what}: swatch {i} oracle {present} ours {}", ours.is_some()));
        }
        let Some(s) = ours else { continue };
        if r.take(24) != bytes(&s.bbox) {
            return Err(format!("{what}: swatch {i} box: ours {:?}", s.bbox));
        }
        let tl = loc(&mut r);
        if tl != (s.loc.zone, s.loc.cell, s.loc.sub) {
            return Err(format!("{what}: swatch {i} loc oracle {tl:?} ours {:?}", s.loc));
        }
        let n = r.i() as usize;
        if n != s.insts.len() {
            return Err(format!("{what}: swatch {i} ({:?}) plants: oracle {n} ours {}", s.loc, s.insts.len()));
        }
        for (k, p) in s.insts.iter().enumerate() {
            let (world, sphere) = (r.take(64).to_vec(), r.take(16).to_vec());
            let (prim, pal) = (r.i(), r.i());
            if world != bytes(&p.world) || sphere != bytes(&p.sphere) || (prim, pal) != (p.prim, p.pal) {
                return Err(format!("{what}: swatch {i} plant {k}: ours {:?} {:?} {} {}", p.world, p.sphere, p.prim, p.pal));
            }
        }
        plants += n;
    }
    Ok(plants)
}

/// One Render on both sides, every plant drawn compared in order. A
/// heading of NaN is a frustum that keeps everything.
fn render(o: &mut Oracle, sys: &mut System<M>, eye: Vect3<M>, h: f32, what: &str) -> Result<usize, String> {
    let (mut fr, view) = camera(eye, if h.is_nan() { 0.0 } else { h });
    if h.is_nan() {
        for pl in &mut fr.planes {
            *pl = Plane { n: Vect3::of(0.0, 1.0, 0.0), d: F::new(-1.0e6) };
        }
    }
    let mut w = Wire::default();
    for pl in &fr.planes {
        w.0.extend_from_slice(bytes(pl));
    }
    w.0.extend_from_slice(bytes(&view));
    w.v(&eye);
    let res = o.call(1403, &w.0);
    let drawn = sys.render(&fr, &eye);
    let mut r = Rd { b: &res, p: 0 };
    let n = r.i() as usize;
    if n != drawn.len() {
        return Err(format!("{what}: drawn oracle {n} ours {}", drawn.len()));
    }
    for (k, d) in drawn.iter().enumerate() {
        let mv = Matrix44::mul(&d.world, &view);
        let (theirs_mv, prim, pal, alpha, flags) = (r.take(64).to_vec(), r.i(), r.i(), r.f(), r.i());
        if theirs_mv != bytes(&mv) || (prim, pal, alpha, flags) != (d.prim, d.pal, d.alpha.bits(), 3) {
            return Err(format!("{what}: draw {k}: ours {mv:?} {} {} {:?}", d.prim, d.pal, d.alpha));
        }
    }
    Ok(n)
}

fn run(plant: bool) -> Option<Result<(usize, usize, usize), String>> {
    let mut o = Oracle::open_at("oracle_flora")?;
    Some((|| {
        let (mut fake, mut sys) = init(&mut o, 2, 3, vec![0, 1, 2, 1, 0, 2], plant)?;
        let (mut plants, mut drawn, mut most) = (0, 0, 0);
        let mut now = 5000i64;
        // across x = 2000: column 0's last sub-cells (zone 0) into column 1's
        // first (zone 1), some steps inside one sub-cell
        for k in 0..70 {
            let t = k as f32 / 69.0;
            let at = Vect3::of(1880.0 + 280.0 * t, 35.0, 1000.0 + 50.0 * t);
            now += 33;
            plants += update(&mut o, &mut fake, &mut sys, now, at, &format!("walk {k}"))?;
            if k % 5 == 0 {
                let n = render(&mut o, &mut sys, Vect3::of(at.x.0, at.y.0 + 1.7, at.z.0), k as f32 * 0.37, &format!("render {k}"))?;
                drawn += n;
                most = most.max(n);
            }
        }
        // everything within 40 of the dense end, past the 700
        let n = render(&mut o, &mut sys, Vect3::of(2160.0, 37.0, 1050.0), f32::NAN, "all round")?;
        drawn += n;
        most = most.max(n);
        // a jump to row 1 (every swatch let go) and back; a Reset; again
        for (k, at) in [Vect3::of(2050.0, 35.0, 3010.0), Vect3::of(2050.0, 35.0, 1040.0)].into_iter().enumerate() {
            now += 33;
            plants += update(&mut o, &mut fake, &mut sys, now, at, &format!("jump {k}"))?;
        }
        o.call(1404, &[]);
        sys.reset();
        now += 33;
        plants += update(&mut o, &mut fake, &mut sys, now, Vect3::of(2050.0, 35.0, 1040.0), "after reset")?;
        drawn += render(&mut o, &mut sys, Vect3::of(2050.0, 37.0, 1040.0), 2.0, "after reset")?;
        // one row: no clamp, no wrap into the next cell
        let (mut fake, mut sys) = init(&mut o, 1, 3, vec![1, 0, 2], plant)?;
        for k in 0..40 {
            let at = Vect3::of(1940.0 + 3.0 * k as f32, 32.0, 1100.0 + 2.0 * k as f32);
            now += 33;
            plants += update(&mut o, &mut fake, &mut sys, now, at, &format!("one row {k}"))?;
            if k % 8 == 0 {
                drawn += render(&mut o, &mut sys, at, 4.0 + k as f32 * 0.2, &format!("one row render {k}"))?;
            }
        }
        Ok((plants, drawn, most))
    })())
}

#[test]
fn flora_matches_the_decomp() {
    let Some(res) = run(false) else { return };
    let (plants, drawn, most) = res.unwrap();
    eprintln!("flora: {plants} plants made over the script, {drawn} drawn, at most {most} in a frame");
    assert!(plants > 10_000 && drawn > 2000, "too little grown to mean anything");
    assert_eq!(most, 700, "Render never met its 700");
    let planted = run(true).unwrap();
    assert!(planted.is_err(), "a planted difference compared equal");
}
