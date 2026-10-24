//! This port against the decomp compiled for the host (eqoa-port's
//! `build/oracle`), bit for bit, under the [`Ieee`] model: the same object
//! bytes and arguments go to both, and every byte that comes back must agree.
//!
//! The oracle is found at `$EQOA_ORACLE`, else `../eqoa-port/build/oracle`
//! beside this repository (`make oracle` in eqoa-port). Without it the tests
//! say so and pass.

#![allow(clippy::approx_constant, clippy::field_reassign_with_default)]

mod common;

use common::*;
use std::mem::{offset_of, size_of};
use vi::camera::WalkCamera;
use vi::coll::{CollRay, CollSphere};
use vi::fpu::{F, Ieee};
use vi::vect::Vect3;
use vi::walk::{WalkControl, WalkPhysics};

type W = WalkControl<Ieee>;
type C = WalkCamera<Ieee>;
type S = CollSphere<Ieee>;

fn physics(r: &mut Rng) -> WalkPhysics<Ieee> {
    WalkPhysics {
        time: (r.next() % 100_000) as i64,
        position: Vect3::of(r.range(-2000.0, 26000.0), r.range(-50.0, 300.0), r.range(-2000.0, 32000.0)),
        angles: r.vec(),
        velocity: r.vec(),
        angular_vel: r.vec(),
        force: r.vec(),
        torque: r.vec(),
        flags: r.flags(),
        floor: F::new(r.any()),
        loco_state: (r.next() % 14) as i32,
        height_offset: F::new(if r.next().is_multiple_of(2) { 0.0 } else { r.range(-0.8, 0.8) }),
        target_heading: F::new(r.any()),
        landscape_normal: {
            let mut n = Vect3::of(r.range(-0.6, 0.6), r.range(0.3, 1.0), r.range(-0.6, 0.6));
            if !r.next().is_multiple_of(4) {
                n.normalize();
            }
            n
        },
    }
}

/// A walk control with the stock parameters (sometimes perturbed) and two
/// random body states.
fn walk(r: &mut Rng) -> W {
    let mut w = W::default();
    w.init_parms();
    if r.next().is_multiple_of(4) {
        w.walk_speed = F::new(r.range(0.5, 10.0));
        w.turn_rate = F::new(r.range(0.2, 4.0));
        w.min_pitch = F::new(r.range(-1.6, 0.0));
        w.max_pitch = F::new(r.range(0.0, 1.6));
    }
    w.phys = physics(r);
    w.committed = physics(r);
    w.committed.position.x = w.phys.position.x + F::new(r.range(-0.5, 0.5));
    w.committed.position.z = w.phys.position.z + F::new(r.range(-0.5, 0.5));
    w.init_intermediate();
    if r.next().is_multiple_of(3) {
        w.update_directions();
    }
    w
}

#[test]
fn layout_matches() {
    let Some(mut o) = Oracle::open() else { return };
    let r = o.call(0, &[]);
    let v: Vec<usize> = r.chunks(4).map(|c| u32::from_le_bytes(c.try_into().unwrap()) as usize).collect();
    let ours = [
        0,
        size_of::<WalkPhysics<Ieee>>(),
        size_of::<W>(),
        size_of::<C>(),
        size_of::<CollRay<Ieee>>(),
        size_of::<S>(),
        size_of::<vi::camera::CameraEffect<Ieee>>(),
        offset_of!(W, phys),
        offset_of!(W, parm118),
        offset_of!(W, committed),
        offset_of!(C, cur),
        offset_of!(C, last),
        offset_of!(C, auto_orbit),
        offset_of!(C, effect),
        offset_of!(WalkPhysics<Ieee>, flags),
        offset_of!(WalkPhysics<Ieee>, landscape_normal),
    ];
    assert_eq!(v, ours, "sizes and offsets (the oracle's first)");
}

#[test]
fn libm_matches() {
    // The comparison rests on the host's libm agreeing with the oracle's
    // (32-bit glibc). Count, per function, how often it does not.
    let Some(mut o) = Oracle::open() else { return };
    let mut r = Rng(0x5EED_0001);
    let mut bad = std::collections::BTreeMap::new();
    for _ in 0..20_000 {
        let (a, b) = (r.range(-10.0, 10.0), r.range(0.001, 2.0));
        let cases: [(u32, &str, f32); 6] = [
            (900, "sinf", a.sin()),
            (901, "cosf", a.cos()),
            (902, "powf", b.powf(r.range(0.001, 0.2))),
            (903, "atan2f", a.atan2(b)),
            (904, "fmodf", a % 6.283_185_5),
            (905, "sqrtf", b.sqrt()),
        ];
        for (op, name, _) in cases {
            let args: Vec<f32> = match op {
                902 => vec![b, 0.032],
                903 => vec![a, b],
                904 => vec![a, 6.283_185_5],
                905 => vec![b],
                _ => vec![a],
            };
            let ours = match op {
                900 => a.sin(),
                901 => a.cos(),
                902 => b.powf(0.032),
                903 => a.atan2(b),
                904 => a % 6.283_185_5,
                _ => b.sqrt(),
            };
            let mut p = Vec::new();
            for x in &args {
                p.extend_from_slice(&x.to_le_bytes());
            }
            let res = o.call(op, &p);
            let theirs = f32::from_le_bytes(res[4..8].try_into().unwrap());
            if theirs.to_bits() != ours.to_bits() {
                *bad.entry(name).or_insert(0) += 1;
            }
        }
    }
    eprintln!("libm disagreements in 20,000 samples each: {bad:?}");
    assert!(bad.is_empty(), "the host libm differs from the oracle's: {bad:?}");
}

/// Runs `n` random cases of one walk-control op on both sides.
fn check_walk(o: &mut Oracle, op: u32, name: &str, n: usize, mut act: impl FnMut(&mut W, &mut Rng) -> (Vec<f32>, Vec<f32>)) {
    let mut r = Rng(0xC0FFEE ^ op as u64);
    for i in 0..n {
        let before = walk(&mut r);
        let mut ours = before;
        let (args, outs) = act(&mut ours, &mut r);
        let (_, theirs, their_outs) = o.run(op, &before, &args);
        if let Some((off, a, b)) = first_diff(&ours, &theirs) {
            panic!("{name} case {i}: word +0x{off:03X} ours {a:08X} theirs {b:08X} (args {args:?})");
        }
        let ob: Vec<u32> = outs.iter().map(|x| x.to_bits()).collect();
        let tb: Vec<u32> = their_outs.iter().map(|x| x.to_bits()).collect();
        assert_eq!(ob, tb, "{name} case {i}: outputs");
    }
}

#[test]
fn walk_control_matches() {
    let Some(mut o) = Oracle::open() else { return };
    let n = 3000;
    let f = F::<Ieee>::new;
    check_walk(&mut o, 100, "InitParms", 50, |w, _| { w.init_parms(); (vec![], vec![]) });
    check_walk(&mut o, 101, "InitIntermediate", 200, |w, _| { w.init_intermediate(); (vec![], vec![]) });
    check_walk(&mut o, 102, "UpdateDirections", n, |w, _| { w.update_directions(); (vec![], vec![]) });
    check_walk(&mut o, 103, "UpdatePhysics", n, |w, r| {
        let dt = [0.032, 0.016, r.range(0.001, 0.2)][(r.next() % 3) as usize];
        w.update_physics(f(dt));
        (vec![dt], vec![])
    });
    check_walk(&mut o, 104, "UpdateLocomotion", n, |w, r| {
        let dt = [0.032, r.range(0.001, 0.2)][(r.next() % 2) as usize];
        w.update_locomotion(f(dt));
        (vec![dt], vec![])
    });
    check_walk(&mut o, 105, "AutoPitch", n, |w, r| {
        let dt = r.range(0.001, 0.2);
        w.auto_pitch(f(dt));
        (vec![dt], vec![])
    });
    for (op, name) in [(106u32, "Walk"), (107, "Run"), (108, "Move")] {
        check_walk(&mut o, op, name, n, |w, r| {
            let (x, y) = (r.any(), r.any());
            match op {
                106 => w.walk(f(x), f(y)),
                107 => w.run(f(x), f(y)),
                _ => w.move_stick(f(x), f(y)),
            };
            (vec![x, y], vec![])
        });
    }
    for (op, name) in [(109u32, "TurnLeft"), (110, "SwimUp"), (111, "PitchDown"), (112, "SetTargetHeading")] {
        check_walk(&mut o, op, name, n, |w, r| {
            let t = r.any();
            match op {
                109 => w.turn_left(f(t)),
                110 => w.swim_up(f(t)),
                111 => w.pitch_down(f(t)),
                _ => w.set_target_heading(f(t)),
            };
            (vec![t], vec![])
        });
    }
    check_walk(&mut o, 113, "SetPitchRange", n, |w, r| {
        let (lo, hi) = (r.any(), r.any());
        w.set_pitch_range(f(lo), f(hi));
        (vec![lo, hi], vec![])
    });
    for (op, name) in [(114u32, "ViewRelativeMove"), (115, "ViewRelativeWalk")] {
        check_walk(&mut o, op, name, n, |w, r| {
            let v = r.vec();
            let (x, y) = (r.any(), r.any());
            if op == 114 { w.view_relative_move(&v, f(x), f(y)) } else { w.view_relative_walk(&v, f(x), f(y)) };
            (vec![v.x.0, v.y.0, v.z.0, x, y], vec![])
        });
    }
    check_walk(&mut o, 116, "CalcPitchRollToLandscape", n, |w, r| {
        let h = r.any();
        let mut v = Vect3::new(f(h), f(r.any()), f(r.any()));
        let args = vec![v.x.0, v.y.0, v.z.0];
        w.calc_pitch_roll_to_landscape(&mut v);
        (args, v.to_array().to_vec())
    });
    check_walk(&mut o, 117, "ClearForces", 200, |w, _| { w.clear_forces(); (vec![], vec![]) });
}

fn camera(r: &mut Rng) -> C {
    let mut c = C::default();
    c.walk = walk(r);
    c.look_at_height = F::new(1.68);
    c.auto_orbit = 1;
    c.min_orbit_distance = F::new(1.5);
    c.angle_damping = F::new(0.01);
    c.camera_mode = [0, 1, 2, 3][(r.next() % 4) as usize];
    c.def_orbit_angle = F::new(1.570_796_4);
    c.max_orbit_distance = F::new(25.0);
    c.min_collision_dist = F::new(3.0);
    c.dist_snap_speed = F::new(23.5);
    c.collision_radius = F::new(0.5);
    c.def_orbit_distance = F::new(11.75);
    c.dist_damping = F::new(0.01);
    c.def_orbit_pitch = F::new(0.523_598_8);
    c.alt_orbit_pitch = F::new(1.047_197_6);
    c.target_height = F::new(1.1);
    c.look_at_height_above = F::new(1.68);
    c.snap_speed = F::new(std::f32::consts::PI);
    c.angle_accel = F::new(r.range(1.0, 300.0));
    c.dist_accel = F::new(r.range(1.0, 300.0));
    for s in [&mut c.cur, &mut c.last] {
        s.time = (r.next() % 100_000) as i64;
        s.flags = (r.next() % 16) as u32;
        s.orbit = r.vec();
        s.orbit_vel = r.vec();
        s.orbit_force = r.vec();
        s.dist = F::new(r.range(0.0, 30.0));
        s.dist_vel = F::new(r.any());
        s.dist_force = F::new(r.any());
        s.camera_pos = r.vec();
        s.snap_angle = F::new(r.any());
        s.snap_pitch = F::new(r.any());
    }
    c
}

#[test]
fn walk_camera_matches() {
    let Some(mut o) = Oracle::open() else { return };
    let f = F::<Ieee>::new;
    let ops: [(u32, &str); 11] = [
        (200, "UpdateCameraPhysics"),
        (201, "OrbitRight"),
        (202, "OrbitIn"),
        (203, "SetOrbitDistance"),
        (204, "SetOrbitPitchAngle"),
        (205, "SetDefOrbitPitchAngle"),
        (206, "Halt"),
        (207, "ClearCameraForces"),
        (208, "SetCameraMode"),
        (209, "SetOrbitAngle"),
        (210, "SetOrbitAngleBehind"),
    ];
    for (op, name) in ops {
        let mut r = Rng(0xCA3E_0000 ^ op as u64);
        for i in 0..2000 {
            let before = camera(&mut r);
            let mut ours = before;
            let a = match op {
                200 => [0.032, r.range(0.001, 0.2)][(r.next() % 2) as usize],
                208 => (r.next() % 4) as f32,
                _ => r.any(),
            };
            match op {
                200 => ours.update_camera_physics(f(a)),
                201 => ours.orbit_right(f(a)),
                202 => ours.orbit_in(f(a)),
                203 => ours.set_orbit_distance(f(a)),
                204 => ours.set_orbit_pitch_angle(f(a)),
                205 => ours.set_def_orbit_pitch_angle(f(a)),
                206 => ours.halt(),
                207 => ours.clear_camera_forces(),
                208 => ours.set_camera_mode(a as i32),
                209 => ours.set_orbit_angle(f(a)),
                _ => ours.set_orbit_angle_behind(),
            };
            let (_, theirs, _) = o.run(op, &before, &[a]);
            if let Some((off, x, y)) = first_diff(&ours, &theirs) {
                panic!("{name} case {i}: word +0x{off:03X} ours {x:08X} theirs {y:08X} (arg {a})");
            }
        }
    }
}

type E = vi::camera::CameraEffect<Ieee>;

/// A camera effect: a manual camera off, blending in or out, or all the way
/// in, at a place and a heading and pitch as `BeginManualCamera` gives one
/// (now and then a roll), half the time with a shake running.
fn effect(r: &mut Rng) -> E {
    let mut e = E::default();
    if r.next().is_multiple_of(2) {
        e.speed = 1 + (r.next() % 3) as i32;
        e.sample = (r.next() % 6) as i32;
        e.amplitude = F::new(r.range(0.0, 3.0));
        e.amplitude_end = F::new(r.range(0.0, 3.0));
        e.amplitude_rate = F::new([0.0, r.range(-4.0, 4.0)][(r.next() % 2) as usize]);
    }
    e.manual_xyz = Vect3::of(r.range(-2000.0, 26000.0), r.range(-50.0, 300.0), r.range(-2000.0, 32000.0));
    let roll = if r.next().is_multiple_of(4) { r.range(-0.3, 0.3) } else { 0.0 };
    e.manual_hpr = Vect3::of(r.range(-7.0, 7.0), r.range(-1.5607964, 1.5607964), roll);
    e.manual_terp = F::new(match r.next() % 8 {
        0 => 0.0,
        1 => 1.0,
        2 => 1.0e-7,
        3 => 0.99999994,
        4 => r.range(-0.2, 0.0),
        _ => r.unit(),
    });
    e.manual_terp_rate = F::new([0.0, 5.0, -5.0, r.range(-20.0, 20.0)][(r.next() % 4) as usize]);
    e
}

/// The eye and direction `VIWalkCamera::SetCamera` hands `Apply`: near the
/// manual place, looking along a unit direction (first person, the fixed
/// follow) or at the body from the orbit (any length); now and then straight
/// up or down, and the test's regimes (zeros, signed zeros, large).
fn eye(r: &mut Rng, e: &E) -> (Vect3<Ieee>, Vect3<Ieee>) {
    let m = e.manual_xyz;
    let pos = Vect3::of(m.x.0 + r.range(-20.0, 20.0), m.y.0 + r.range(-5.0, 15.0), m.z.0 + r.range(-20.0, 20.0));
    let dir = match r.next() % 5 {
        0 => {
            let mut d = Vect3::of(r.range(-1.0, 1.0), r.range(-1.0, 1.0), r.range(-1.0, 1.0));
            d.normalize();
            d
        }
        1 => Vect3::of(r.range(-25.0, 25.0), r.range(-12.0, 2.0), r.range(-25.0, 25.0)),
        2 => Vect3::of(0.0, [1.0, -1.0, 3.5][(r.next() % 3) as usize], 0.0),
        _ => r.vec(),
    };
    (pos, dir)
}

/// The blend as it would be with the ease left out (k = terp): the planted
/// bug the comparison must see. The rest is `apply`'s, step for step.
fn apply_linear(e: &E, pos: &mut Vect3<Ieee>, dir: &mut Vect3<Ieee>) {
    use vi::geom::Matrix44;
    use vi::quat::Quat;
    let k = e.manual_terp;
    let mut m = Matrix44::<Ieee>::identity();
    let fwd = *dir;
    let mut up = Vect3::of(0.0, 1.0, 0.0);
    let mut right = Vect3::cross(&up, &fwd);
    up = Vect3::cross(&fwd, &right);
    right.normalize();
    up.normalize();
    (m.m[0].x, m.m[0].y, m.m[0].z) = (right.x, right.y, right.z);
    (m.m[1].x, m.m[1].y, m.m[1].z) = (up.x, up.y, up.z);
    (m.m[2].x, m.m[2].y, m.m[2].z) = (fwd.x, fwd.y, fwd.z);
    let hpr = Quat::slerp(&Quat::from_hpr(&m.euler_zxy()), &Quat::from_hpr(&e.manual_hpr), k).to_hpr();
    let x = e.manual_xyz;
    *pos = Vect3::new(pos.x + (x.x - pos.x) * k, pos.y + (x.y - pos.y) * k, pos.z + (x.z - pos.z) * k);
    dir.x = hpr.x.sinf() * hpr.y.cosf();
    dir.y = -hpr.y.sinf();
    dir.z = hpr.x.cosf() * hpr.y.cosf();
}

/// `VICameraEffect`'s manual camera (BeginManual, EndManual, Update, and
/// Apply with its blend: the ease, the basis under world up read as euler
/// angles, the quaternion slerp and `VIQuat::ConvertTo`'s way back through a
/// matrix, the place's lerp), bit for bit on the object and the eye. The
/// control: the blend without its ease differs from the oracle's in nearly
/// every blended case.
#[test]
fn camera_effect_matches() {
    let Some(mut o) = Oracle::open() else { return };
    let mut r = Rng(0x03A1_1EC7);
    let (mut blended, mut linear_differs) = (0, 0);
    for i in 0..20_000 {
        let before = effect(&mut r);
        let (pos, dir) = eye(&mut r, &before);
        // Apply
        let (mut p, mut d) = (pos, dir);
        let mut ours = before;
        ours.apply(&mut p, &mut d);
        let args = [pos.x.0, pos.y.0, pos.z.0, dir.x.0, dir.y.0, dir.z.0];
        let (_, theirs, outs) = o.run(250, &before, &args);
        if let Some((off, x, y)) = first_diff(&ours, &theirs) {
            panic!("Apply case {i}: +0x{off:02X} ours {x:08X} theirs {y:08X} ({before:?}, {args:?})");
        }
        let ob: Vec<u32> = [p.x, p.y, p.z, d.x, d.y, d.z].iter().map(|v| v.0.to_bits()).collect();
        let tb: Vec<u32> = outs.iter().map(|v| v.to_bits()).collect();
        assert_eq!(ob, tb, "Apply case {i}: the eye ({before:?}, {args:?})");
        let t = before.manual_terp.0;
        if t > 0.0 && t < 1.0 && before.speed == 0 {
            blended += 1;
            let (mut lp, mut ld) = (pos, dir);
            apply_linear(&before, &mut lp, &mut ld);
            let lb: Vec<u32> = [lp.x, lp.y, lp.z, ld.x, ld.y, ld.z].iter().map(|v| v.0.to_bits()).collect();
            if lb != tb {
                linear_differs += 1;
            }
        }
        // BeginManual
        let time = [0.0, 0.2, r.range(0.01, 1.0)][(r.next() % 3) as usize];
        let (xyz, hpr) = (r.vec(), r.vec());
        let mut ours = before;
        ours.begin_manual(xyz, hpr, F::new(time));
        let (_, theirs, _) = o.run(251, &before, &[xyz.x.0, xyz.y.0, xyz.z.0, hpr.x.0, hpr.y.0, hpr.z.0, time]);
        if let Some((off, x, y)) = first_diff(&ours, &theirs) {
            panic!("BeginManual case {i}: +0x{off:02X} ours {x:08X} theirs {y:08X} (time {time})");
        }
        // EndManual
        let time = [0.0, 0.2, r.range(0.01, 1.0)][(r.next() % 3) as usize];
        let mut ours = before;
        ours.end_manual(F::new(time));
        let (_, theirs, _) = o.run(252, &before, &[time]);
        if let Some((off, x, y)) = first_diff(&ours, &theirs) {
            panic!("EndManual case {i}: +0x{off:02X} ours {x:08X} theirs {y:08X} (time {time})");
        }
        // Update
        let dt = [0.0, 0.016, 0.033, r.range(0.0, 0.2)][(r.next() % 4) as usize];
        let mut ours = before;
        ours.update(F::new(dt));
        let (_, theirs, _) = o.run(253, &before, &[dt]);
        if let Some((off, x, y)) = first_diff(&ours, &theirs) {
            panic!("Update case {i}: +0x{off:02X} ours {x:08X} theirs {y:08X} (dt {dt})");
        }
    }
    eprintln!("camera effect: {blended} blended cases, the ease-less blend differs in {linear_differs}");
    assert!(blended > 3000, "blended cases: {blended}");
    assert!(linear_differs * 100 >= blended * 95, "the control: the blend without its ease differs in {linear_differs} of {blended}");
}

#[test]
fn coll_sphere_matches() {
    let Some(mut o) = Oracle::open() else { return };
    let f = F::<Ieee>::new;
    let mut r = Rng(0x5F_E2E);
    for i in 0..5000 {
        let (a, b) = (r.vec(), r.vec());
        let rad = [0.0, 0.001, 0.5, r.range(0.0, 2.0)][(r.next() % 4) as usize];
        let mut ours = S::default();
        ours.init(a, b, f(rad));
        let (_, theirs, _) = o.run(300, &S::default(), &[a.x.0, a.y.0, a.z.0, b.x.0, b.y.0, b.z.0, rad]);
        if let Some((off, x, y)) = first_diff(&ours, &theirs) {
            panic!("CollSphere::Init case {i}: +0x{off:02X} ours {x:08X} theirs {y:08X}");
        }
        // A hit somewhere along it, then Skitter off a random normal.
        let mut hit = ours;
        hit.ray.hit_time = f([0.0, 0.5, 1.0, r.unit()][(r.next() % 4) as usize]);
        hit.ray.hit_normal = r.vec();
        hit.ray.hit_normal.normalize();
        let v = if r.next().is_multiple_of(2) { Vect3::of(0.0, 1.0, 0.0) } else { r.vec() };
        let mut ours = hit;
        ours.skitter(&v);
        let (_, theirs, _) = o.run(301, &hit, &[v.x.0, v.y.0, v.z.0]);
        if let Some((off, x, y)) = first_diff(&ours, &theirs) {
            panic!("Skitter case {i}: +0x{off:02X} ours {x:08X} theirs {y:08X}");
        }
    }
}
