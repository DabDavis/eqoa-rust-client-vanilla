//! `vi::shadow` against the decomp's `VIScene::RenderShadow`, bit for bit
//! (`Ieee`): eqoa-port's `build/oracle_shadow` compiles viscene.cpp over a
//! scene in raw memory and a recording raster, so the decomp's own
//! RenderShadow runs end to end: the actor's transform and its inverse, the
//! CSprite's six slots through libsupport's `CalcSlotWorldTransform` (node
//! world matrices and a centre matrix the caller gives, stamped with the
//! scene's time), the box, `m_queryTris` (Init(200) as `VIScene::Init` makes
//! it, Reset, Intersect, the clipper), `QueryIntersection`'s own setup over a
//! world walk that hands the list the caller's triangle soup, and every
//! raster call in order (SetMaterial, SetZBias, BeginTriFan, Color, UV,
//! Vertex, EndTriFan). Compared: the return, the box the world walk got, the
//! list's triangles after, the calls with every float's bits.
//!
//! Cases: an upright character on flat ground cut by the box; tilted and
//! scaled; every slot missing (the actor's own transform); a step and a
//! wall (the y planes cut, the 80 degrees reject); ground above the feet and
//! below them (the alpha's clamps); wound away; past the list's 200; no
//! material; no sprite; a singular transform (scale 0: `Invert`'s partial
//! rows); then seeded random characters over random ground. The control: our
//! side with one node moved a hundredth must not compare equal.
//!
//! SKIPs without the oracle (`make oracle` in eqoa-port).

mod common;
use common::{Oracle, bytes};
use vi::flora::QueryTriList;
use vi::fpu::{F, Ieee};
use vi::geom::{BBox, Matrix44};
use vi::shadow::{ShadowScene, Shadow, render_shadow};
use vi::vect::Vect3;

type M = Ieee;

/// One RenderShadow call, on both sides.
#[derive(Clone)]
struct Case {
    name: String,
    time: i64,
    material: i32,
    sprite: bool,
    loc: Vect3<M>,
    hpr: Vect3<M>,
    scale: f32,
    node: [i32; 9],
    nodes: Vec<Matrix44<M>>,
    centre: Matrix44<M>,
    soup: Vec<(i32, [Vect3<M>; 3])>,
}

impl ShadowScene<M> for Case {
    fn has_sprite(&self, _: i32) -> bool {
        self.sprite
    }
    fn actor_transform(&self, _: i32) -> Matrix44<M> {
        Matrix44::transform(&self.hpr, F::new(self.scale), &self.loc)
    }
    fn actor_location(&self, _: i32) -> Vect3<M> {
        self.loc
    }
    // CalcSlotWorldTransform: CalcNodeTransform's node world times
    // CalcTransAboutCenter's centre times the actor's transform
    fn slot_world_transform(&self, actor: i32, slot: i32) -> Matrix44<M> {
        let t = self.actor_transform(actor);
        match self.node[slot as usize] {
            -1 => t,
            n => Matrix44::mul(&self.nodes[n as usize], &Matrix44::mul(&self.centre, &t)),
        }
    }
    fn query(&mut self, _: &BBox<M>, l: &mut QueryTriList<M>) {
        for (idx, v) in &self.soup {
            l.intersect(*idx, &v[0], &v[1], &v[2]);
        }
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
}

/// The calls our shadow makes on the raster, as the oracle records them.
fn calls(s: &Option<Shadow<M>>) -> Vec<u8> {
    let mut w = Wire::default();
    let Some(s) = s else { return w.0 };
    w.i(1);
    w.i(s.material);
    w.i(2);
    w.i(15);
    for t in &s.tris {
        w.i(3);
        for v in t {
            w.i(4);
            w.0.extend_from_slice(bytes(&v.color));
            w.i(5);
            w.0.extend_from_slice(bytes(&v.uv));
            w.i(6);
            w.0.extend_from_slice(bytes(&v.at));
        }
        w.i(7);
    }
    w.i(2);
    w.i(0);
    w.0
}

fn count(s: &Option<Shadow<M>>) -> usize {
    s.as_ref().map_or(0, |s| 3 + s.tris.len() * 11)
}

/// Runs a case on both sides: what differs, or the triangles drawn.
fn run(o: &mut Oracle, c: &Case, ours: &Case) -> Result<usize, String> {
    let mut w = Wire::default();
    w.0.extend(c.time.to_le_bytes());
    w.i(c.material);
    w.i(c.sprite as i32);
    w.v(&c.loc);
    w.v(&c.hpr);
    w.f(c.scale);
    for n in c.node {
        w.i(n);
    }
    w.i(c.nodes.len() as i32);
    for m in &c.nodes {
        w.0.extend_from_slice(bytes(m));
    }
    w.0.extend_from_slice(bytes(&c.centre));
    w.i(c.soup.len() as i32);
    for (idx, v) in &c.soup {
        w.i(*idx);
        for p in v {
            w.v(p);
        }
    }
    let res = o.call(1600, &w.0);
    let mut sc = ours.clone();
    let s = render_shadow(&mut sc, ours.material, 1);
    let mut r = Rd { b: &res, p: 0 };
    let what = &c.name;
    let ret = r.i();
    if (ret == 0) != s.is_some() {
        return Err(format!("{what}: oracle returned {ret}, ours {}", if s.is_some() { "a shadow" } else { "none" }));
    }
    let queried = r.i() != 0;
    if queried != s.is_some() {
        return Err(format!("{what}: oracle queried {queried}"));
    }
    if let Some(s) = &s
        && r.take(24) != bytes(&s.query)
    {
        return Err(format!("{what}: the box: ours {:?} (local {:?})", s.query, s.local));
    }
    let n = r.i() as usize;
    let found = s.as_ref().map_or(&[][..], |s| &s.found[..]);
    if queried && n != found.len() {
        return Err(format!("{what}: the list: oracle {n} triangles, ours {}", found.len()));
    }
    for (k, t) in found.iter().enumerate() {
        let b = r.take(40);
        let mut want = t.actor.to_le_bytes().to_vec();
        for p in [t.v0, t.v1, t.v2] {
            want.extend_from_slice(bytes(&p));
        }
        if b != want {
            return Err(format!("{what}: list triangle {k}: ours {t:?}"));
        }
    }
    if !queried {
        // a shadow never drawn leaves the list as it was
        r.take(40 * n);
    }
    let nc = r.i() as usize;
    if nc != count(&s) {
        return Err(format!("{what}: oracle {nc} raster calls, ours {}", count(&s)));
    }
    let theirs = &res[r.p..];
    let mine = calls(&s);
    if theirs != &mine[..] {
        let at = theirs.iter().zip(&mine).position(|(a, b)| a != b).unwrap_or(theirs.len().min(mine.len()));
        return Err(format!("{what}: the raster calls differ at byte {at} of {}", mine.len()));
    }
    Ok(s.map_or(0, |s| s.tris.len()))
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * ((self.next() >> 40) as f32 / (1u64 << 24) as f32)
    }
}

fn at(x: f32, y: f32, z: f32) -> Matrix44<M> {
    let mut m = Matrix44::identity();
    m.m[3].x = F::new(x);
    m.m[3].y = F::new(y);
    m.m[3].z = F::new(z);
    m
}

/// A standing figure's nodes in the body's frame: feet, hands, chest, head
/// (slots 5 6, 0 1, 4, 7), the centre lifting it 0.05.
fn figure(loc: Vect3<M>, hpr: Vect3<M>, scale: f32, name: &str) -> Case {
    let nodes = vec![at(0.42, 1.02, 0.12), at(-0.44, 0.98, 0.15), at(0.0, 1.31, 0.02), at(0.0, 1.74, -0.03), at(0.16, 0.04, 0.11), at(-0.15, 0.06, -0.09)];
    Case {
        name: name.to_string(),
        time: 4321,
        material: 7,
        sprite: true,
        loc,
        hpr,
        scale,
        // m_node: 0 1 hands, 4 chest, 5 6 feet, 7 head; 2 3 8 elsewhere
        node: [0, 1, -1, 3, 2, 4, 5, 3, -1],
        nodes,
        centre: at(0.0, 0.05, 0.0),
        soup: Vec::new(),
    }
}

/// A height field round (x, z) on a `step` grid, `h` its height, a
/// triangle's index its count; `away` every n-th wound away.
fn ground(x: f32, z: f32, step: f32, cells: i32, h: impl Fn(f32, f32) -> f32, away: u32) -> Vec<(i32, [Vect3<M>; 3])> {
    let p = |x: f32, z: f32| Vect3::of(x, h(x, z), z);
    let mut out = Vec::new();
    let mut k = 0u32;
    let x0 = x - step * cells as f32 / 2.0;
    let z0 = z - step * cells as f32 / 2.0;
    for i in 0..cells {
        for j in 0..cells {
            let (xa, xb) = (x0 + i as f32 * step, x0 + (i + 1) as f32 * step);
            let (za, zb) = (z0 + j as f32 * step, z0 + (j + 1) as f32 * step);
            // facing up (`VIPlane`'s normal of (a, b, c) is (c - b) x (b - a))
            for tri in [[p(xa, za), p(xb, za), p(xa, zb)], [p(xb, za), p(xb, zb), p(xa, zb)]] {
                k += 1;
                let tri = if away != 0 && k.is_multiple_of(away) { [tri[0], tri[2], tri[1]] } else { tri };
                out.push((k as i32, tri));
            }
        }
    }
    out
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let loc = Vect3::of(24812.37, 31.25, 14702.81);
    // upright on flat ground a little under the feet, the grid's cells
    // larger and smaller than the box
    for (k, step) in [0.31f32, 0.9, 2.7].into_iter().enumerate() {
        let mut c = figure(loc, Vect3::of(0.7 + k as f32, 0.0, 0.0), 1.0, &format!("flat {step}"));
        c.soup = ground(loc.x.0, loc.z.0, step, 12, |_, _| 31.17, 0);
        out.push(c);
    }
    // tilted and scaled: toLocal's off-axis terms reach the UVs
    let mut c = figure(loc, Vect3::of(-2.1, 0.31, -0.22), 1.37, "tilted");
    c.soup = ground(loc.x.0, loc.z.0, 0.43, 14, |x, z| 31.0 + 0.2 * (x * 1.3).sin() + 0.15 * (z * 0.7).cos(), 0);
    out.push(c);
    // every slot missing: the six points are the actor's origin
    let mut c = figure(loc, Vect3::of(1.0, 0.0, 0.0), 1.0, "no slots");
    c.node = [-1; 9];
    c.soup = ground(loc.x.0, loc.z.0, 0.5, 8, |_, _| 31.2, 0);
    out.push(c);
    // a step up behind and a wall: the y planes cut the risers, the 80
    // degrees reject the wall, the alpha clamps above the feet (a step above
    // them) and fades below
    let mut c = figure(loc, Vect3::of(0.2, 0.0, 0.0), 1.0, "step and wall");
    let step = |x: f32, z: f32| if x > loc.x.0 + 0.1 { 31.45 } else if z < loc.z.0 - 0.2 { 30.6 } else { 31.2 };
    c.soup = ground(loc.x.0, loc.z.0, 0.17, 20, step, 0);
    let (wx, wz) = (loc.x.0 - 0.3, loc.z.0);
    for k in 0..6 {
        let y0 = 30.4 + k as f32 * 0.3;
        c.soup.push((900 + k, [Vect3::of(wx, y0, wz - 1.0), Vect3::of(wx, y0 + 0.3, wz - 1.0), Vect3::of(wx, y0, wz + 1.0)]));
        // a slope at 75 degrees passes, at 85 does not
        let (dx75, dx85) = (0.3 / 75f32.to_radians().tan(), 0.3 / 85f32.to_radians().tan());
        for (n, x, dx) in [(950, wx + 0.5, dx75), (960, wx + 0.6, dx85)] {
            let (a, b, d) = (Vect3::of(x, y0, wz - 1.0), Vect3::of(x + dx, y0 + 0.3, wz + 1.0), Vect3::of(x + dx, y0 + 0.3, wz - 1.0));
            c.soup.push((n + k, [a, d, b]));
            c.soup.push((n + 20 + k, [a, b, d]));
        }
    }
    out.push(c);
    // the ground above the feet and far below them
    for (name, y) in [("ground above", 31.6f32), ("ground 0.9 below", 30.35), ("ground at the bottom", 30.25)] {
        let mut c = figure(loc, Vect3::of(2.5, 0.0, 0.0), 1.0, name);
        c.soup = ground(loc.x.0, loc.z.0, 0.6, 6, |_, _| y, 0);
        out.push(c);
    }
    // every third wound away
    let mut c = figure(loc, Vect3::of(-0.4, 0.0, 0.0), 1.0, "wound away");
    c.soup = ground(loc.x.0, loc.z.0, 0.4, 10, |_, _| 31.2, 3);
    out.push(c);
    // past the list's 200
    let mut c = figure(loc, Vect3::of(0.9, 0.0, 0.0), 1.2, "past 200");
    c.soup = ground(loc.x.0, loc.z.0, 0.07, 40, |x, z| 31.1 + 0.03 * (x * 9.0).sin() * (z * 7.0).cos(), 0);
    out.push(c);
    // no material, no sprite: nothing drawn
    let mut c = figure(loc, Vect3::of(0.0, 0.0, 0.0), 1.0, "no material");
    c.material = -1;
    c.soup = ground(loc.x.0, loc.z.0, 0.5, 6, |_, _| 31.2, 0);
    out.push(c.clone());
    c.material = 3;
    c.sprite = false;
    c.name = "no sprite".to_string();
    out.push(c);
    // a singular transform: Invert stops at its zero pivot
    let mut c = figure(loc, Vect3::of(0.3, 0.0, 0.0), 0.0, "scale 0");
    c.soup = ground(loc.x.0, loc.z.0, 0.5, 6, |_, _| 31.2, 0);
    out.push(c);
    // seeded: characters anywhere, of any size and pose, over rough ground
    let mut rng = Rng(0x5AD0_0B10_B5EE_D123);
    for k in 0..300 {
        let loc = Vect3::of(rng.range(-30000.0, 30000.0), rng.range(-200.0, 600.0), rng.range(-30000.0, 30000.0));
        let tilt = if k % 3 == 0 { 0.4 } else { 0.0 };
        let hpr = Vect3::of(rng.range(-3.2, 3.2), rng.range(-tilt, tilt), rng.range(-tilt, tilt));
        let mut c = figure(loc, hpr, rng.range(0.3, 3.0), &format!("seeded {k}"));
        for n in c.nodes.iter_mut() {
            n.m[3].x = n.m[3].x + F::new(rng.range(-0.3, 0.3));
            n.m[3].y = n.m[3].y + F::new(rng.range(-0.3, 0.3));
            n.m[3].z = n.m[3].z + F::new(rng.range(-0.3, 0.3));
        }
        c.centre = Matrix44::transform(&Vect3::of(rng.range(-0.2, 0.2), 0.0, 0.0), F::new(rng.range(0.8, 1.2)), &Vect3::of(0.0, rng.range(0.0, 0.3), 0.0));
        if k % 7 == 0 {
            c.node[rng.next() as usize % 9] = -1;
        }
        let (a, b, ph) = (rng.range(0.0, 0.6), rng.range(0.0, 0.6), rng.range(0.0, 6.0));
        let y = loc.y.0 + rng.range(-0.8, 0.3);
        c.soup = ground(loc.x.0, loc.z.0, rng.range(0.1, 1.5), 1 + (rng.next() % 12) as i32, move |x, z| y + a * (x * 2.1 + ph).sin() + b * (z * 1.7).cos(), (rng.next() % 9) as u32);
        out.push(c);
    }
    out
}

#[test]
fn shadows_match_the_decomp() {
    let Some(mut o) = Oracle::open_at("oracle_shadow") else { return };
    let (mut drawn, mut shadows, mut most) = (0, 0, 0);
    for c in cases() {
        let n = run(&mut o, &c, &c).unwrap();
        drawn += n;
        shadows += (n > 0) as usize;
        most = most.max(n);
    }
    eprintln!("shadows: {shadows} cases drew {drawn} triangles, at most {most} a shadow");
    assert!(shadows > 200 && drawn > 5000, "too little drawn to mean anything");
    assert_eq!(most, 200, "the list's 200 never met");
    // the control: one node a hundredth off on our side
    let c = &cases()[0];
    let mut planted = c.clone();
    planted.nodes[4].m[3].x = planted.nodes[4].m[3].x + F::new(0.01);
    assert!(run(&mut o, c, &planted).is_err(), "a planted difference compared equal");
}
