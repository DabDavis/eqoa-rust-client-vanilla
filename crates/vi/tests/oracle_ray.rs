//! The ray half of the collision leaf against the decomp, bit for bit
//! (`Ieee`): VICollRay's transformed Init, the ray against a triangle, a
//! sphere and a cylinder's side, the two ray culls, and VICollide::CollideObj
//! with a ray over REAL collision buffers out of TUNARIA.ESF (zone 84's
//! terrain and the props it places). A planted difference must fail.
//!
//! Needs the oracle (see tests/common) and the disc at run/eqoa.iso.

mod common;

use common::*;
use std::mem::size_of;
use vi::coll::CollRay;
use vi::collide::{CollBuffer, Collide, Cylinder, collide_ray_no_caps, collide_ray_sphere, collide_ray_tri, cull_ray_bbox, cull_ray_sphere, transform_ray};
use vi::fpu::{F, Ieee};
use vi::geom::{BBox, Matrix44, Sphere};
use vi::vect::Vect3;

type R = CollRay<Ieee>;
type Mx = Matrix44<Ieee>;

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn ray(r: &mut Rng, near: Vect3<Ieee>, extent: f32, reach: f32) -> R {
    let a = Vect3::of(near.x.0 + r.range(-extent, extent), near.y.0 + r.range(-extent, extent), near.z.0 + r.range(-extent, extent));
    let b = Vect3::of(a.x.0 + r.range(-reach, reach), a.y.0 + r.range(-reach, reach), a.z.0 + r.range(-reach, reach));
    let mut s = R::default();
    s.init(a, b);
    // a ray part-way through a sweep: an earlier hit to beat
    if r.next() % 4 == 0 {
        s.hit_time = F::new(r.range(0.1, 0.9));
    }
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

fn same<T: Copy>(ours: &T, theirs: &T) -> Result<(), String> {
    match first_diff(ours, theirs) {
        Some((off, a, b)) => Err(format!("+0x{off:02X} ours {a:08X} theirs {b:08X}")),
        None => Ok(()),
    }
}

/// The leaf cases; `plant` changes one hit's time on our side.
fn leaves(plant: bool) -> Option<Result<[usize; 5], String>> {
    let mut o = Oracle::open()?;
    let mut r = Rng(0x0AA7_5EED);
    let mut hits = [0usize; 5];
    for i in 0..3000 {
        let src = ray(&mut r, Vect3::default(), 5.0, 8.0);
        let m = matrix(&mut r);
        let ours = transform_ray(&src, &m);
        let mut payload = bytes(&src).to_vec();
        payload.extend_from_slice(bytes(&m));
        let res = o.call(414, &payload);
        if let Err(e) = same(&ours, &from_bytes::<R>(&res[4..])) {
            return Some(Err(format!("CollRay::Init(src, m) case {i}: {e}")));
        }
    }
    for i in 0..20_000 {
        let v: Vec<Vect3<Ieee>> = (0..3).map(|_| Vect3::of(r.range(-3.0, 3.0), r.range(-1.0, 1.0), r.range(-3.0, 3.0))).collect();
        let s = ray(&mut r, Vect3::of(0.0, 1.5, 0.0), 3.0, 6.0);
        // the cylinder's direction is unit; its length and radius in v[2]
        let mut cdir = v[1];
        cdir.normalize();
        let leaf = [v[0], cdir, Vect3::of(r.range(0.5, 4.0), r.range(0.2, 1.5), 0.0)];
        for op in [416u32, 417, 418, 419, 420] {
            let w = match op {
                416 => [v[0], v[1], v[2]],
                417 | 420 => [v[0], Vect3::of(r.range(0.1, 2.0), 0.0, 0.0), v[2]],
                418 => leaf,
                _ => {
                    let (mn, mx) = (Vect3::of(v[0].x.0.min(v[1].x.0), v[0].y.0.min(v[1].y.0), v[0].z.0.min(v[1].z.0)), Vect3::of(v[0].x.0.max(v[1].x.0), v[0].y.0.max(v[1].y.0), v[0].z.0.max(v[1].z.0)));
                    [mn, mx, v[2]]
                }
            };
            let mut payload = bytes(&s).to_vec();
            payload.extend(f32s(&w.iter().flat_map(|p| p.to_array()).collect::<Vec<_>>()));
            let mut ours = s;
            let sphere = Sphere { c: w[0], r: w[1].x };
            let hit = match op {
                416 => collide_ray_tri(&mut ours, 7, &w[0], &w[1], &w[2]),
                417 => collide_ray_sphere(&mut ours, &sphere),
                418 => collide_ray_no_caps(&mut ours, &Cylinder { base: w[0], dir: w[1], length: w[2].x, radius: w[2].y }),
                419 => cull_ray_bbox(&ours, &BBox { min: w[0], max: w[1] }),
                _ => cull_ray_sphere(&ours, &sphere),
            };
            if plant && hit && op == 417 && i > 1000 {
                ours.hit_time = ours.hit_time + F::new(1e-6);
            }
            let res = o.call(op, &payload);
            let theirs = i32::from_le_bytes(res[0..4].try_into().unwrap());
            if hit as i32 != theirs {
                return Some(Err(format!("op {op} case {i}: hit ours {hit} theirs {theirs}")));
            }
            if let Err(e) = same(&ours, &from_bytes::<R>(&res[4..])) {
                return Some(Err(format!("op {op} case {i}: {e}")));
            }
            hits[(op - 416) as usize] += hit as usize;
        }
    }
    Some(Ok(hits))
}

#[test]
fn ray_leaves_match() {
    let Some(res) = leaves(false) else { return };
    let hits = res.unwrap();
    eprintln!("hits over 20,000 rays: triangle {}, sphere {}, cylinder {}, culled by box {}, by sphere {}", hits[0], hits[1], hits[2], hits[3], hits[4]);
    assert!(hits.iter().all(|&h| h > 100), "too few hits to mean anything: {hits:?}");
}

#[test]
fn a_planted_difference_fails() {
    if let Some(res) = leaves(true) {
        assert!(res.is_err(), "a planted hit time went unnoticed");
    }
}

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
fn collide_ray_obj_on_real_buffers_matches() {
    let Some(mut o) = Oracle::open() else { return };
    let Some(bufs) = buffers() else { return };
    let mut r = Rng(0x7A15_B0FF);
    let (mut cases, mut hits) = (0, 0);
    for (bi, (body, version, pre)) in bufs.iter().enumerate() {
        let buf = CollBuffer::<Ieee>::parse(body, *version, pre).expect("parse");
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
            // a ray near the buffer in its own space, carried into the world
            let local = ray(&mut r, centre, extent, extent * 2.0);
            let mut s = R::default();
            s.init(m.mul_point(&local.start), m.mul_point(&local.end));
            let mut payload = bytes(&s).to_vec();
            payload.extend_from_slice(bytes(&m));
            payload.extend_from_slice(&(pre.len() as i32).to_le_bytes());
            payload.extend(f32s(&pre.iter().flat_map(|p| p.to_array()).collect::<Vec<_>>()));
            payload.extend_from_slice(&(*version as i32).to_le_bytes());
            payload.extend_from_slice(body);
            let mut ours = s;
            c.set_world(&m);
            let hit = c.collide_ray_obj(&mut ours, &buf);
            let res = o.call(410, &payload);
            assert_eq!(hit as i32, i32::from_le_bytes(res[0..4].try_into().unwrap()), "buffer {bi}: hit");
            if let Err(e) = same(&ours, &from_bytes::<R>(&res[4..4 + size_of::<R>()])) {
                panic!("CollideObj(ray) case {cases}: {e}");
            }
            cases += 1;
            hits += hit as usize;
        }
    }
    eprintln!("CollideObj(ray): {cases} rays over {} real buffers, {hits} hits", bufs.len());
    assert!(hits > cases / 10, "too few hits to mean anything");
}
