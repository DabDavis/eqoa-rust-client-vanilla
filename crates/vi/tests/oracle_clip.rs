//! `vi::clipstack` against the decomp, bit for bit (`Ieee`): eqoa-port's
//! oracle runs libsupport's own `VIFrustum::CalcPlanes` (op 1700) and a script
//! of `VIClipStack` commands on one stack (op 1701): over cameras like the
//! game's (an eye round Freeport's places, a view 55 degrees high at any
//! window's shape, near 0.3, far 50 to 900), the frustum pushed, spheres and
//! boxes culled by it, portal polygons clipped by it (some wholly outside,
//! some straddling, one with a corner exactly on a plane, which `ClipByTop`
//! drops), a clipped portal pushed and tested under, the top read back and
//! popped. Every answer and every float of every plane and point must match.
//! The controls: a corner moved one float step changes a plane, and a script
//! answered for another camera does not compare equal.
//!
//! SKIPs without the oracle (`make oracle` in eqoa-port).

mod common;
use common::{Oracle, Rng};
use vi::clipstack::{Camera, ClipStack, Frustum};
use vi::fpu::{F, Ieee};
use vi::geom::{BBox, Plane, Sphere};
use vi::vect::Vect3;

type M = Ieee;
type V = Vect3<M>;

/// A camera like the game's: the eight corners in `CalcFrustum`'s order
/// (near top-left, top-right, bottom-right, bottom-left, then far), its
/// place, view direction and far distance.
fn camera(rng: &mut Rng) -> ([[f32; 3]; 8], [f32; 3], [f32; 3], f32) {
    let eye = [rng.range(24000.0, 26000.0), rng.range(40.0, 120.0), rng.range(15000.0, 17000.0)];
    let (yaw, pitch) = (rng.range(-3.2, 3.2), rng.range(-0.9, 0.9));
    let f = [yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos()];
    let (r, u) = basis(f);
    let (near, far) = (0.3f32, rng.range(50.0, 900.0));
    let aspect = rng.range(1.0, 2.4);
    let th = (55.0f32.to_radians() * 0.5).tan();
    let mut c = [[0.0f32; 3]; 8];
    for (k, &(sx, sy, d)) in [(-1.0, 1.0, near), (1.0, 1.0, near), (1.0, -1.0, near), (-1.0, -1.0, near), (-1.0, 1.0, far), (1.0, 1.0, far), (1.0, -1.0, far), (-1.0, -1.0, far)].iter().enumerate() {
        let (hw, hh) = (d * th * aspect, d * th);
        for i in 0..3 {
            c[k][i] = eye[i] + f[i] * d + r[i] * (sx * hw) + u[i] * (sy * hh);
        }
    }
    (c, eye, f, far)
}

/// The screen's right and up for a view along `f` with y up, as the game's
/// world is drawn (looking along +z, +x is on the left): right = f x y, up =
/// right x f.
fn basis(f: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let l = (f[2] * f[2] + f[0] * f[0]).sqrt().max(1e-6);
    let r = [-f[2] / l, 0.0, f[0] / l];
    let u = [r[1] * f[2] - r[2] * f[1], r[2] * f[0] - r[0] * f[2], r[0] * f[1] - r[1] * f[0]];
    (r, u)
}

fn v3(a: [f32; 3]) -> V {
    Vect3::of(a[0], a[1], a[2])
}

/// One command of the script.
#[derive(Clone, Debug)]
enum Cmd {
    PushFrustum,
    PushPortal(Vec<[f32; 3]>),
    Pop,
    Sphere([f32; 4]),
    Box([f32; 6]),
    Clip(Vec<[f32; 3]>),
    Top,
}

fn put_f(b: &mut Vec<u8>, v: f32) {
    b.extend(v.to_le_bytes());
}
fn put_i(b: &mut Vec<u8>, v: i32) {
    b.extend(v.to_le_bytes());
}
fn put_v(b: &mut Vec<u8>, v: &V) {
    for x in [v.x.0, v.y.0, v.z.0] {
        put_f(b, x);
    }
}
fn put_p(b: &mut Vec<u8>, p: &Plane<M>) {
    put_v(b, &p.n);
    put_f(b, p.d.0);
}

/// The oracle's request for `cmds` on the camera.
fn request(cam: &([[f32; 3]; 8], [f32; 3], [f32; 3], f32), cmds: &[Cmd]) -> Vec<u8> {
    let mut b = Vec::new();
    for c in cam.0 {
        c.iter().for_each(|&x| put_f(&mut b, x));
    }
    cam.1.iter().chain(cam.2.iter()).for_each(|&x| put_f(&mut b, x));
    put_f(&mut b, cam.3);
    put_i(&mut b, cmds.len() as i32);
    for c in cmds {
        match c {
            Cmd::PushFrustum => put_i(&mut b, 1),
            Cmd::PushPortal(p) | Cmd::Clip(p) => {
                put_i(&mut b, if matches!(c, Cmd::PushPortal(_)) { 2 } else { 6 });
                put_i(&mut b, p.len() as i32);
                p.iter().flatten().for_each(|&x| put_f(&mut b, x));
            }
            Cmd::Pop => put_i(&mut b, 3),
            Cmd::Sphere(s) => {
                put_i(&mut b, 4);
                s.iter().for_each(|&x| put_f(&mut b, x));
            }
            Cmd::Box(s) => {
                put_i(&mut b, 5);
                s.iter().for_each(|&x| put_f(&mut b, x));
            }
            Cmd::Top => put_i(&mut b, 7),
        }
    }
    b
}

/// The same script on our side, answered as the oracle answers it.
fn ours(cam: &([[f32; 3]; 8], [f32; 3], [f32; 3], f32), cmds: &[Cmd]) -> Vec<u8> {
    let frustum = Frustum::<M>::of_corners(cam.0.map(v3));
    let c = Camera { frustum, position: v3(cam.1), dir: v3(cam.2), far: F::new(cam.3) };
    let mut cs = ClipStack::<M>::new();
    let mut b = Vec::new();
    put_i(&mut b, 0);
    for cmd in cmds {
        match cmd {
            Cmd::PushFrustum => put_i(&mut b, cs.push_frustum(&c) as i32),
            Cmd::PushPortal(p) => put_i(&mut b, cs.push_portal(&c, &p.iter().map(|&a| v3(a)).collect::<Vec<_>>()) as i32),
            Cmd::Pop => {
                cs.pop();
                put_i(&mut b, 0);
            }
            Cmd::Sphere(s) => put_i(&mut b, cs.cull_sphere(&Sphere { c: Vect3::of(s[0], s[1], s[2]), r: F::new(s[3]) }) as i32),
            Cmd::Box(s) => put_i(&mut b, cs.cull_bbox(&BBox { min: Vect3::of(s[0], s[1], s[2]), max: Vect3::of(s[3], s[4], s[5]) }) as i32),
            Cmd::Clip(p) => match cs.clip_by_top(&p.iter().map(|&a| v3(a)).collect::<Vec<_>>()) {
                Some(out) => {
                    put_i(&mut b, 1);
                    put_i(&mut b, out.len() as i32);
                    out.iter().for_each(|v| put_v(&mut b, v));
                }
                None => {
                    put_i(&mut b, 0);
                    put_i(&mut b, 0);
                }
            },
            Cmd::Top => {
                let (verts, planes) = cs.top();
                put_i(&mut b, verts.len() as i32);
                verts.iter().for_each(|v| put_v(&mut b, v));
                put_i(&mut b, planes.len() as i32);
                planes.iter().for_each(|p| put_p(&mut b, p));
            }
        }
    }
    b
}

/// A quad `d` ahead of the camera, `w` wide, off the view's middle by
/// (sx, sy) of its half-sizes there.
fn quad(cam: &([[f32; 3]; 8], [f32; 3], [f32; 3], f32), d: f32, w: f32, sx: f32, sy: f32) -> Vec<[f32; 3]> {
    let (eye, f) = (cam.1, cam.2);
    let (r, u) = basis(f);
    let at = |x: f32, y: f32| [0, 1, 2].map(|i| eye[i] + f[i] * d + r[i] * (x + sx * d * 0.5) + u[i] * (y + sy * d * 0.4));
    vec![at(-w, -w), at(w, -w), at(w, w), at(-w, w)]
}

#[test]
fn calc_planes_as_the_decomp() {
    let Some(mut o) = Oracle::open() else { return };
    let mut rng = Rng(0x00C1_1F05);
    for case in 0..200 {
        let cam = camera(&mut rng);
        let mut req = Vec::new();
        cam.0.iter().flatten().for_each(|&x| put_f(&mut req, x));
        let got = o.call(1700, &req);
        let f = Frustum::<M>::of_corners(cam.0.map(v3));
        let mut want = Vec::new();
        put_i(&mut want, 0);
        f.planes.iter().for_each(|p| put_p(&mut want, p));
        assert_eq!(got, want, "case {case}");
        // the corners' order as the game's world is drawn: a point straight
        // ahead is inside every plane, one behind the eye outside the near one
        let at = |d: f32| v3([0, 1, 2].map(|i| cam.1[i] + cam.2[i] * d));
        assert!(f.planes.iter().all(|p| p.distance(&at(cam.3 * 0.5)).0 < 0.0), "case {case}: ahead is inside");
        assert!(f.planes[0].distance(&at(-1.0)).0 > 0.0, "case {case}: behind is outside the near plane");
        // the control: a corner one float step over changes a plane
        let mut moved = cam.0;
        moved[5][0] = f32::from_bits(moved[5][0].to_bits() + 1);
        let g = Frustum::<M>::of_corners(moved.map(v3));
        let mut other = Vec::new();
        put_i(&mut other, 0);
        g.planes.iter().for_each(|p| put_p(&mut other, p));
        assert_ne!(other, want, "case {case}: the control");
    }
}

#[test]
fn the_clip_stack_as_the_decomp() {
    let Some(mut o) = Oracle::open() else { return };
    let mut rng = Rng(0x0C11_95AC);
    let mut pushed = 0;
    for case in 0..150 {
        let cam = camera(&mut rng);
        let (eye, far) = (cam.1, cam.3);
        let mut cmds = vec![Cmd::PushFrustum, Cmd::Top];
        for _ in 0..12 {
            let s = [eye[0] + rng.range(-far, far), eye[1] + rng.range(-40.0, 40.0), eye[2] + rng.range(-far, far), rng.range(0.0, 30.0)];
            cmds.push(Cmd::Sphere(s));
            let (x, y, z) = (eye[0] + rng.range(-far, far), eye[1] + rng.range(-40.0, 40.0), eye[2] + rng.range(-far, far));
            let e = [rng.range(0.0, 40.0), rng.range(0.0, 20.0), rng.range(0.0, 40.0)];
            cmds.push(Cmd::Box([x, y, z, x + e[0], y + e[1], z + e[2]]));
        }
        // portals: in view, straddling an edge, behind the camera
        let near = rng.range(2.0, 40.0);
        let portal = quad(&cam, near, rng.range(0.5, 6.0), rng.range(-0.8, 0.8), rng.range(-0.8, 0.8));
        cmds.push(Cmd::Clip(portal.clone()));
        cmds.push(Cmd::Clip(quad(&cam, near, 3.0, 1.6, 0.0)));
        cmds.push(Cmd::Clip(quad(&cam, -near, 3.0, 0.0, 0.0)));
        // a corner exactly on the frustum's far plane
        let f = Frustum::<M>::of_corners(cam.0.map(v3));
        let mut on = quad(&cam, near, 2.0, 0.0, 0.0);
        on[0] = cam.0[4];
        cmds.push(Cmd::Clip(on));
        // the portal clipped on our side and pushed: its volume tested
        let c = Camera { frustum: f, position: v3(cam.1), dir: v3(cam.2), far: F::new(far) };
        let mut cs = ClipStack::<M>::new();
        cs.push_frustum(&c);
        if let Some(clipped) = cs.clip_by_top(&portal.iter().map(|&a| v3(a)).collect::<Vec<_>>()) {
            let pts: Vec<[f32; 3]> = clipped.iter().map(|v| [v.x.0, v.y.0, v.z.0]).collect();
            cmds.push(Cmd::PushPortal(pts));
            cmds.push(Cmd::Top);
            for _ in 0..8 {
                let s = [portal[0][0] + rng.range(-8.0, 8.0), portal[0][1] + rng.range(-8.0, 8.0), portal[0][2] + rng.range(-8.0, 8.0), rng.range(0.0, 2.0)];
                cmds.push(Cmd::Sphere(s));
            }
            cmds.push(Cmd::Clip(quad(&cam, near * 2.0, 1.0, rng.range(-0.5, 0.5), rng.range(-0.5, 0.5))));
            cmds.push(Cmd::Pop);
            pushed += 1;
        }
        cmds.push(Cmd::Top);
        let got = o.call(1701, &request(&cam, &cmds));
        let want = ours(&cam, &cmds);
        assert_eq!(got.len(), want.len(), "case {case}: {cmds:?}");
        assert_eq!(got, want, "case {case}");
        // the control: the script answered for another camera differs
        let other = camera(&mut Rng(case as u64 + 7));
        assert_ne!(ours(&other, &cmds), got, "case {case}: the control");
    }
    assert!(pushed > 50, "portals pushed: {pushed}");
}
