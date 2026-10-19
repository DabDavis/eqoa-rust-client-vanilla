//! The collision leaf against the decomp compiled for the host, bit for bit
//! (`Ieee`): VIMatrix44's Transform/InvertAffine/Mul, VIPlane, VICollSphere's
//! transformed Init, the triangle/edge/vertex sweeps, and VICollide::CollideObj
//! over REAL collision buffers out of TUNARIA.ESF (zone 84's terrain and the
//! props it places), built on both sides from the same 0x4200 bodies.
//!
//! Needs the oracle (see tests/common) and the disc at run/eqoa.iso.
#![allow(clippy::field_reassign_with_default)]

mod common;

use common::*;
use std::mem::size_of;
use vi::coll::CollSphere;
use vi::collide::{CollBuffer, Collide, collide_edge, collide_point, collide_tri, transform_sphere};
use vi::fpu::{F, Ieee};
use vi::geom::{Matrix44, Plane};
use vi::vect::Vect3;

type S = CollSphere<Ieee>;
type Mx = Matrix44<Ieee>;

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn sweep(r: &mut Rng, near: Vect3<Ieee>, extent: f32) -> S {
    let a = Vect3::of(
        near.x.0 + r.range(-extent, extent),
        near.y.0 + r.range(-extent, extent),
        near.z.0 + r.range(-extent, extent),
    );
    let b = Vect3::of(a.x.0 + r.range(-4.0, 4.0), a.y.0 + r.range(-4.0, 4.0), a.z.0 + r.range(-4.0, 4.0));
    let rad = [0.001, 0.5, 0.6, r.range(0.0, 2.0)][(r.next() % 4) as usize];
    let mut s = S::default();
    s.init(a, b, F::new(rad));
    s
}

fn matrix(r: &mut Rng) -> Mx {
    if r.next().is_multiple_of(3) {
        return Mx::identity();
    }
    let hpr = Vect3::of(r.range(-7.0, 7.0), r.range(-0.4, 0.4), r.range(-0.4, 0.4));
    let t = Vect3::of(r.range(-50.0, 50.0), r.range(-10.0, 10.0), r.range(-50.0, 50.0));
    Mx::transform(&hpr, F::new(r.range(0.5, 2.0)), &t)
}

fn check<T: Copy>(name: &str, i: usize, ours: &T, theirs: &T) {
    if let Some((off, a, b)) = first_diff(ours, theirs) {
        panic!("{name} case {i}: +0x{off:02X} ours {a:08X} theirs {b:08X}");
    }
}

#[test]
fn matrices_and_primitives_match() {
    let Some(mut o) = Oracle::open() else { return };
    let mut r = Rng(0x0BA5_EBA11);
    for i in 0..3000 {
        // Transform, InvertAffine, Mul.
        let hpr = r.vec();
        let sc = r.range(0.1, 3.0);
        let t = r.vec();
        let m = Mx::transform(&hpr, F::new(sc), &t);
        let res = o.call(401, &f32s(&[hpr.x.0, hpr.y.0, hpr.z.0, sc, t.x.0, t.y.0, t.z.0]));
        check("Transform", i, &m, &from_bytes::<Mx>(&res[4..]));
        let inv = Mx::invert_affine(&m);
        let res = o.call(402, bytes(&m));
        check("InvertAffine", i, &inv, &from_bytes::<Mx>(&res[4..]));
        let m2 = matrix(&mut r);
        let p = Mx::mul(&m, &m2);
        let mut payload = bytes(&m).to_vec();
        payload.extend_from_slice(bytes(&m2));
        let res = o.call(403, &payload);
        check("Mul", i, &p, &from_bytes::<Mx>(&res[4..]));
        // VICollSphere::Init(src, m).
        let src = sweep(&mut r, Vect3::default(), 5.0);
        let ours = transform_sphere(&src, &m);
        let mut payload = bytes(&src).to_vec();
        payload.extend_from_slice(bytes(&m));
        let res = o.call(404, &payload);
        check("CollSphere::Init(src, m)", i, &ours, &from_bytes::<S>(&res[4..]));
    }
}

#[test]
fn triangle_edge_vertex_match() {
    let Some(mut o) = Oracle::open() else { return };
    let mut r = Rng(0x7E1A_0001);
    let mut hits = [0; 3];
    for i in 0..20_000 {
        // A triangle near the origin and a sweep through its neighbourhood.
        let v: Vec<Vect3<Ieee>> =
            (0..3).map(|_| Vect3::of(r.range(-3.0, 3.0), r.range(-1.0, 1.0), r.range(-3.0, 3.0))).collect();
        let s = sweep(&mut r, Vect3::of(0.0, 1.5, 0.0), 3.0);
        let mut payload = bytes(&s).to_vec();
        payload.extend(f32s(&v.iter().flat_map(|p| p.to_array()).collect::<Vec<_>>()));
        let mut pl = Plane::<Ieee>::default();
        let ret = pl.from_tri(&v[0], &v[1], &v[2]);
        let res = o.call(405, &payload);
        assert_eq!(ret, i32::from_le_bytes(res[0..4].try_into().unwrap()), "plane case {i}");
        check("VIPlane()", i, &pl, &from_bytes::<Plane<Ieee>>(&res[4..]));
        for (k, op) in [406u32, 407, 408].into_iter().enumerate() {
            let mut ours = s;
            let hit = match op {
                406 => collide_tri(&mut ours, 7, &v[0], &v[1], &v[2]),
                407 => collide_edge(&mut ours, &v[0], &v[1]),
                _ => collide_point(&mut ours, &v[0]),
            };
            let res = o.call(op, &payload);
            assert_eq!(hit as i32, i32::from_le_bytes(res[0..4].try_into().unwrap()), "op {op} case {i}: hit");
            check(["triangle", "edge", "vertex"][k], i, &ours, &from_bytes::<S>(&res[4..]));
            hits[k] += hit as usize;
        }
    }
    eprintln!("hits over 20,000 sweeps: triangle {}, edge {}, vertex {}", hits[0], hits[1], hits[2]);
    assert!(hits.iter().all(|&h| h > 100), "too few hits to mean anything: {hits:?}");
}

/// Real buffers: zone 84's terrain SubSprites (PackVGF, with the zone's
/// pre-translations) and the SimpleSprite props in TUNARIA (other packings).
/// A 0x4200 body, its record version, and the pre-translations it uses.
type RawBuffer = (Vec<u8>, i16, Vec<Vect3<Ieee>>);

fn buffers() -> Option<Vec<RawBuffer>> {
    let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !iso_path.exists() {
        eprintln!("SKIP: no disc at {}", iso_path.display());
        return None;
    }
    let mut iso = iso::Iso::open(iso_path).ok()?;
    let c = esf::Container::open(iso.read("TUNARIA.ESF").ok()?).ok()?;
    let zone = c.zones()[84];
    let pre: Vec<Vect3<Ieee>> = c.pre_translations(zone).iter().map(|p| Vect3::of(p[0], p[1], p[2])).collect();
    let mut out = Vec::new();
    for &ss in c.terrain_patches(zone).iter().step_by(7) {
        if let Some(b) = c.child(ss, 0x4200) {
            out.push((c.body(b).to_vec(), c.node(b).version, pre.clone()));
        }
    }
    let mut props = 0;
    for (i, n) in c.nodes.iter().enumerate() {
        if n.ty == 0x2000
            && props < 60
            && let Some(b) = c.child(i, 0x4200)
        {
            out.push((c.body(b).to_vec(), c.node(b).version, Vec::new()));
            props += 1;
        }
    }
    Some(out)
}

#[test]
fn collide_obj_on_real_buffers_matches() {
    let Some(mut o) = Oracle::open() else { return };
    let Some(bufs) = buffers() else { return };
    let mut r = Rng(0xB0FF_E125);
    let (mut cases, mut hits) = (0, 0);
    let mut kinds = std::collections::BTreeMap::new();
    for (bi, (body, version, pre)) in bufs.iter().enumerate() {
        let buf = CollBuffer::<Ieee>::parse(body, *version, pre).expect("parse");
        *kinds.entry(buf.kind).or_insert(0) += 1;
        let mut c = Collide::<Ieee>::default();
        c.set_pretranslations(pre);
        let centre = {
            let mut m = buf.bbox.min;
            m += buf.bbox.max;
            m *= F::new(0.5);
            m
        };
        let extent = ((buf.bbox.max.x.0 - buf.bbox.min.x.0).abs().max((buf.bbox.max.z.0 - buf.bbox.min.z.0).abs()) * 0.5).max(1.0);
        for _ in 0..60 {
            let m = matrix(&mut r);
            // A sweep near the buffer in its own space, carried into the world.
            let local = sweep(&mut r, centre, extent);
            let mut s = S::default();
            s.init(m.mul_point(&local.ray.start), m.mul_point(&local.ray.end), local.radius);
            let mut payload = bytes(&s).to_vec();
            payload.extend_from_slice(bytes(&m));
            payload.extend_from_slice(&(pre.len() as i32).to_le_bytes());
            payload.extend(f32s(&pre.iter().flat_map(|p| p.to_array()).collect::<Vec<_>>()));
            payload.extend_from_slice(&(*version as i32).to_le_bytes());
            payload.extend_from_slice(body);
            let mut ours = s;
            c.set_world(&m);
            let hit = c.collide_obj(&mut ours, &buf);
            let res = o.call(400, &payload);
            assert_eq!(hit as i32, i32::from_le_bytes(res[0..4].try_into().unwrap()), "buffer {bi}: hit");
            check("CollideObj", cases, &ours, &from_bytes::<S>(&res[4..4 + size_of::<S>()]));
            cases += 1;
            hits += hit as usize;
        }
    }
    eprintln!("CollideObj: {cases} sweeps over {} real buffers (by packing {kinds:?}), {hits} hits", bufs.len());
    assert!(hits > cases / 10, "too few hits to mean anything");
}
