//! The pick family against the decomp, bit for bit (`Ieee`):
//! VIPickSegment's transformed Init, VIRaster's two Culls, Pick (one
//! triangle), CalcHitColor, and PickPrimBufferPackVUNC over strips built
//! from random terrain-like vertices, with the vertex colours and with a
//! colour buffer's. The vertex block and the colour tags go to both sides as
//! the same bytes, so `decode_pack_vunc` and `decode_color_tags` are held
//! too. A planted difference must fail.
//!
//! Needs the oracle (see tests/common).

mod common;

use common::*;
use vi::fpu::{F, Ieee};
use vi::geom::{BBox, Matrix44, Sphere};
use vi::pick::{PickBuffer, PickSegment, calc_hit_color, cull_bbox, cull_sphere, decode_color_tags, decode_pack_vunc, pick_strips, pick_tri};
use vi::vect::Vect3;

type Seg = PickSegment<Ieee>;

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn same<T: Copy>(ours: &T, theirs: &T) -> Result<(), String> {
    match first_diff(ours, theirs) {
        Some((off, a, b)) => Err(format!("+0x{off:02X} ours {a:08X} theirs {b:08X}")),
        None => Ok(()),
    }
}

/// A segment from above the origin's neighbourhood, mostly downward.
fn segment(r: &mut Rng) -> Seg {
    let a = Vect3::of(r.range(-3.0, 3.0), r.range(1.0, 6.0), r.range(-3.0, 3.0));
    let b = Vect3::of(a.x.0 + r.range(-3.0, 3.0), a.y.0 - r.range(2.0, 12.0), a.z.0 + r.range(-3.0, 3.0));
    let mut s = Seg::new(a, b);
    if r.next() % 4 == 0 {
        s.hit_time = F::new(r.range(0.1, 0.9));
    }
    s
}

fn color(r: &mut Rng) -> [u8; 4] {
    let v = r.next();
    [v as u8, (v >> 8) as u8, (v >> 16) as u8, (v >> 24) as u8]
}

/// The leaves; `plant` nudges one hit colour on our side.
fn leaves(plant: bool) -> Option<Result<[usize; 3], String>> {
    let mut o = Oracle::open()?;
    let mut r = Rng(0x0D1C_4E55);
    let mut n = [0usize; 3];
    for i in 0..6000 {
        let seg = segment(&mut r);
        let mut tri = [0f32; 9];
        for (k, t) in tri.iter_mut().enumerate() {
            *t = if k % 3 == 1 { r.range(-0.5, 0.5) } else { r.range(-3.0, 3.0) };
        }
        // a degenerate triangle now and then
        if i % 50 == 0 {
            tri.copy_within(0..3, 3);
        }
        let p = |k: usize| Vect3::of(tri[3 * k], tri[3 * k + 1], tri[3 * k + 2]);
        let cs = [color(&mut r), color(&mut r), color(&mut r)];
        let mut payload = bytes(&seg).to_vec();
        payload.extend(f32s(&tri));
        payload.extend(cs.iter().flatten());
        // Pick
        let mut ours = seg;
        let hit = pick_tri(&mut ours, 5, &p(0), &p(1), &p(2));
        let res = o.call(432, &payload);
        if hit as i32 != i32::from_le_bytes(res[0..4].try_into().unwrap()) {
            return Some(Err(format!("case {i}: Pick")));
        }
        if let Err(e) = same(&ours, &from_bytes::<Seg>(&res[4..])) {
            return Some(Err(format!("case {i}: Pick {e}")));
        }
        n[0] += hit as usize;
        // CalcHitColor, at the hit when there was one, else wherever the
        // segment's time says
        let mut ours = if hit { ours } else { seg };
        let mut payload = bytes(&ours).to_vec();
        payload.extend(f32s(&tri));
        payload.extend(cs.iter().flatten());
        let ok = calc_hit_color(&mut ours, &p(0), &p(1), &p(2), cs[0], cs[1], cs[2]);
        if plant && i == 777 {
            ours.hit_color.a = F::new(ours.hit_color.a.0 + 1e-3);
        }
        let res = o.call(431, &payload);
        if ok as i32 != i32::from_le_bytes(res[0..4].try_into().unwrap()) {
            return Some(Err(format!("case {i}: CalcHitColor")));
        }
        if let Err(e) = same(&ours, &from_bytes::<Seg>(&res[4..])) {
            return Some(Err(format!("case {i}: CalcHitColor {e}")));
        }
        n[1] += ok as usize;
        // Init(seg, m) and the culls
        let m = if i % 3 == 0 {
            Matrix44::identity()
        } else {
            Matrix44::transform(&Vect3::of(r.range(-7.0, 7.0), r.range(-0.4, 0.4), r.range(-0.4, 0.4)), F::new(r.range(0.5, 2.0)), &Vect3::of(r.range(-5.0, 5.0), r.range(-2.0, 2.0), r.range(-5.0, 5.0)))
        };
        let sp = Sphere { c: Vect3::of(r.range(-4.0, 4.0), r.range(-4.0, 4.0), r.range(-4.0, 4.0)), r: F::new(r.range(0.2, 3.0)) };
        let lo = Vect3::of(r.range(-5.0, 2.0), r.range(-5.0, 2.0), r.range(-5.0, 2.0));
        let bb = BBox { min: lo, max: Vect3::of(lo.x.0 + r.range(0.1, 4.0), lo.y.0 + r.range(0.1, 4.0), lo.z.0 + r.range(0.1, 4.0)) };
        let mut payload = bytes(&ours).to_vec();
        payload.extend_from_slice(bytes(&m));
        payload.extend(f32s(&[sp.c.x.0, sp.c.y.0, sp.c.z.0, sp.r.0, bb.min.x.0, bb.min.y.0, bb.min.z.0, bb.max.x.0, bb.max.y.0, bb.max.z.0]));
        let t = Seg::transformed(&ours, &m);
        let res = o.call(433, &payload);
        let w = |k: usize| i32::from_le_bytes(res[k..k + 4].try_into().unwrap());
        if let Err(e) = same(&t, &from_bytes::<Seg>(&res[4..96])) {
            return Some(Err(format!("case {i}: Init(seg, m) {e}")));
        }
        if (cull_sphere(&t, &sp) as i32, cull_bbox(&t, &bb) as i32) != (w(96), w(100)) {
            return Some(Err(format!("case {i}: Cull")));
        }
        n[2] += (w(96) == 0) as usize;
    }
    Some(Ok(n))
}

#[test]
fn pick_leaves_match() {
    let Some(r) = leaves(false) else { return };
    let n = r.unwrap();
    eprintln!("6000 cases: {} triangle hits, {} hit colours, {} sphere culls passed", n[0], n[1], n[2]);
    assert!(n[0] > 100 && n[1] > 300 && n[2] > 300, "{n:?}");
}

#[test]
fn a_planted_hit_colour_fails() {
    let Some(r) = leaves(true) else { return };
    assert!(r.is_err(), "a nudged hit colour went unnoticed");
}

/// Strips of a little height field: rows of vertices zig-zagging across.
fn strips(r: &mut Rng) -> (f32, Vec<(i32, BBox<Ieee>)>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let scale = [1.0 / 256.0, 1.0 / 64.0, 1.0 / 1024.0][r.next() as usize % 3];
    let ns = 1 + r.next() as usize % 6;
    let (mut prims, mut payload, mut verts, mut tags) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for s in 0..ns {
        let nv = 3 + (r.next() % 12) as i32;
        let z0 = s as f32 - ns as f32 / 2.0;
        let mut pos = Vec::new();
        for k in 0..nv {
            let x = (k / 2) as f32 - 3.0 + r.range(-0.2, 0.2);
            let z = z0 + 1.0 - (k % 2) as f32 + r.range(-0.2, 0.2);
            let y = r.range(-0.6, 0.6);
            pos.push([(x / scale) as i16, (y / scale) as i16, (z / scale) as i16]);
        }
        let mut bb = BBox::<Ieee>::default();
        for (k, p) in pos.iter().enumerate() {
            let v = Vect3::of(p[0] as f32 * scale, p[1] as f32 * scale, p[2] as f32 * scale);
            bb.bound(k == 0, &v);
        }
        // now and then a box the segment misses (the strip culled)
        if r.next() % 7 == 0 {
            bb.min.x = F::new(50.0);
            bb.max.x = F::new(60.0);
        }
        payload.extend(nv.to_le_bytes());
        payload.extend(f32s(&[bb.min.x.0, bb.min.y.0, bb.min.z.0, bb.max.x.0, bb.max.y.0, bb.max.z.0]));
        verts.extend([0u8; 32]);
        let base = tags.len();
        tags.resize(base + (nv as usize + 4).div_ceil(4) * 16, 0);
        for (k, p) in pos.iter().enumerate() {
            let vc = color(r);
            let cc = color(r);
            payload.extend(p.iter().flat_map(|c| c.to_le_bytes()));
            payload.extend(vc);
            payload.extend(cc);
            let mut v = [0u8; 32];
            for (j, c) in p.iter().enumerate() {
                v[4 + 2 * j..6 + 2 * j].copy_from_slice(&c.to_le_bytes());
            }
            v[0x1C..0x20].copy_from_slice(&vc);
            verts.extend(v);
            tags[base + 8 + 4 * k..base + 12 + 4 * k].copy_from_slice(&cc);
        }
        verts.extend([0u8; 16]);
        prims.push((nv, bb));
    }
    let mut head = (ns as i32).to_le_bytes().to_vec();
    head.extend(payload);
    (scale, prims, head, verts, tags)
}

#[test]
fn pick_pack_vunc_matches() {
    let Some(mut o) = Oracle::open() else { return };
    let mut r = Rng(0x57A1_9C0D);
    let mut hits = [0usize; 2];
    for i in 0..4000 {
        let mode = i % 2;
        let (scale, prims, body, verts, tags) = strips(&mut r);
        let seg = {
            let a = Vect3::of(r.range(-3.0, 3.0), r.range(0.5, 4.0), r.range(-3.0, 3.0));
            let b = Vect3::of(a.x.0 + r.range(-1.0, 1.0), a.y.0 - r.range(1.0, 8.0), a.z.0 + r.range(-1.0, 1.0));
            Seg::new(a, b)
        };
        let mut payload = (mode as i32).to_le_bytes().to_vec();
        payload.extend(f32s(&[seg.start.x.0, seg.start.y.0, seg.start.z.0, seg.end.x.0, seg.end.y.0, seg.end.z.0, scale]));
        payload.extend(body);
        let pb = PickBuffer { bbox: BBox::default(), strips: decode_pack_vunc(&verts, &prims, F::new(scale), &vec![7; prims.len()]) };
        let cb = decode_color_tags(&tags, &prims.iter().map(|p| p.0).collect::<Vec<_>>());
        let mut ours = seg;
        let hit = pick_strips(&mut ours, &pb, (mode == 1).then_some(&cb[..]));
        let res = o.call(430, &payload);
        assert_eq!(hit as i32, i32::from_le_bytes(res[0..4].try_into().unwrap()), "case {i}: hit");
        if let Err(e) = same(&ours, &from_bytes::<Seg>(&res[4..])) {
            panic!("case {i} (mode {mode}): {e}");
        }
        hits[mode] += hit as usize;
    }
    eprintln!("PickPrimBufferPackVUNC: {} hits on vertex colours, {} on a colour buffer, of 2000 each", hits[0], hits[1]);
    assert!(hits[0] > 300 && hits[1] > 300, "{hits:?}");
}
