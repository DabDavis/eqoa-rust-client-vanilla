//! The atmosphere against the decomp, frame by frame: eqoa-port's
//! `build/oracle_atmos` runs SUPPORT's VIAtmosphere (with VIMaterial and the
//! particle system under it) over a recording raster, sound device and scene
//! with newlib's rand(), and every request here is run on it and on
//! `vi::atmosphere` under `Ieee`. Each frame compares the lightning flash,
//! every material run's billboards (stars, the shooting star, lightning
//! patches, rain, snow, splashes, and the fireballs' particles), every sky
//! sprite rastered and its model-view, the fog switches, every sound call,
//! and the state after: the transition, the sky's material layers, the
//! thunder's clock, all 200 weather particles and the fireballs.
//!
//! The run goes through clear night skies (stars, a shooting star), a rain
//! fade, a thunderstorm queued behind it (lightning, thunder, the rain's two
//! voices), a default-sky change under weather, snow, an instant sandstorm, a
//! Lavastorm (two SPELLFX definitions as its head and impact, when the disc
//! is there), clearing, both planes of sky, and a camera that walks over a
//! liquid surface and jumps far enough to re-drop the whole field.
//!
//! SKIPs without the oracle (`make oracle` in eqoa-port).

mod common;
use common::Oracle;
use std::collections::HashMap;
use vi::atmosphere::{Atmosphere, Camera, Layer, Material, Raster, Scene, Sound};
use vi::coll::{CollRay, EnvRay};
use vi::csprite::Rand;
use vi::fpu::{F, Ieee};
use vi::geom::{Matrix44, Plane, Vect4};
use vi::particle::{Definition, System};
use vi::vect::Vect3;

type M = Ieee;

/// The oracle's ground and liquid: y = 0.1 x - 0.05 z + 3, and west of x = 0
/// a surface at y = 0.1 x + 5.5.
struct Ground;

impl Scene<M> for Ground {
    fn collide_ray(&mut self, r: &mut CollRay<M>) -> bool {
        let g = r.start.x * F::new(0.1) - r.start.z * F::new(0.05) + F::new(3.0);
        let t = (r.start.y - g) / (r.start.y - r.end.y);
        if t < F::ZERO || F::ONE < t {
            return false;
        }
        r.hit_time = t;
        r.hit_idx = 0;
        true
    }
    fn collide_env(&mut self, r: &mut EnvRay<M>) -> bool {
        if F::ZERO <= r.start.x {
            return false;
        }
        let w = r.start.x * F::new(0.1) + F::new(5.5);
        let t = (r.start.y - w) / (r.start.y - r.end.y);
        if t < F::ZERO || F::ONE < t {
            return false;
        }
        r.hit_time = t;
        r.hits = 1;
        true
    }
}

/// Records as the oracle does: (op, handle or sound, argument bits, result).
#[derive(Default)]
struct Log {
    next: i32,
    calls: Vec<[u32; 4]>,
}

impl Sound for Log {
    fn play(&mut self, sound: i32) -> i32 {
        self.next += 1;
        self.calls.push([1, sound as u32, 0, self.next as u32]);
        self.next
    }
    fn stop(&mut self, h: i32) {
        self.calls.push([2, h as u32, 0, 0]);
    }
    fn set_volume(&mut self, h: i32, v: f32) {
        self.calls.push([3, h as u32, v.to_bits(), 0]);
    }
    fn set_pan(&mut self, h: i32, p: f32) {
        self.calls.push([4, h as u32, p.to_bits(), 0]);
    }
    fn set_pitch(&mut self, h: i32, p: i32) {
        self.calls.push([5, h as u32, p as u32, 0]);
    }
}

fn layer(texture: i32, blend: i32, color: [u8; 4]) -> Layer {
    Layer { texture, blend, color }
}

/// The five sky sprites' palettes: sun, corona, moon, sky, sky360.
fn palettes() -> Vec<Vec<Vec<Layer>>> {
    vec![
        vec![vec![layer(20, 1, [255, 240, 200, 255])]],
        vec![vec![layer(21, 1, [255, 255, 255, 255])]],
        vec![vec![layer(22, 1, [200, 200, 255, 255])], vec![layer(23, 2, [180, 180, 255, 255])]],
        vec![vec![layer(30, 0, [200, 210, 220, 255]), layer(31, 3, [255, 255, 255, 64])]],
        vec![vec![layer(32, 0, [128, 128, 128, 255]), layer(33, 3, [255, 255, 255, 64])]],
    ]
}

fn i32s(v: &mut Vec<u8>, x: i32) {
    v.extend(x.to_le_bytes());
}
fn f32s(v: &mut Vec<u8>, x: F<M>) {
    v.extend(x.0.to_bits().to_le_bytes());
}
fn v3(v: &mut Vec<u8>, x: &Vect3<M>) {
    f32s(v, x.x);
    f32s(v, x.y);
    f32s(v, x.z);
}
fn v4(v: &mut Vec<u8>, x: &Vect4<M>) {
    f32s(v, x.x);
    f32s(v, x.y);
    f32s(v, x.z);
    f32s(v, x.w);
}
fn m44(v: &mut Vec<u8>, m: &Matrix44<M>) {
    for r in &m.m {
        v4(v, r);
    }
}

/// Our side of a frame reply, labelled section by section.
fn ours(a: &Atmosphere<M>, lit: Option<F<M>>, r: &Raster<M>, snd: &Log) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut v = Vec::new();
    i32s(&mut v, lit.is_some() as i32);
    f32s(&mut v, lit.unwrap_or(F::ZERO));
    out.push(("lightning".into(), std::mem::take(&mut v)));
    i32s(&mut v, r.runs.len() as i32);
    out.push(("runs".into(), std::mem::take(&mut v)));
    for (k, (mat, bbs)) in r.runs.iter().enumerate() {
        i32s(&mut v, *mat);
        i32s(&mut v, bbs.len() as i32);
        out.push((format!("run {k} (material {mat})"), std::mem::take(&mut v)));
        for (j, b) in bbs.iter().enumerate() {
            v3(&mut v, &b.location);
            i32s(&mut v, b.frame);
            f32s(&mut v, b.width);
            f32s(&mut v, b.height);
            for c in [b.color.r, b.color.g, b.color.b, b.color.a] {
                f32s(&mut v, c);
            }
            out.push((format!("run {k} billboard {j}"), std::mem::take(&mut v)));
        }
    }
    i32s(&mut v, r.sprites.len() as i32);
    for (id, m) in &r.sprites {
        i32s(&mut v, *id);
        m44(&mut v, m);
    }
    i32s(&mut v, r.fog_off as i32);
    out.push(("sprites, fog".into(), std::mem::take(&mut v)));
    i32s(&mut v, snd.calls.len() as i32);
    for c in &snd.calls {
        for x in c {
            v.extend(x.to_le_bytes());
        }
    }
    out.push(("sound calls".into(), std::mem::take(&mut v)));
    for x in [a.intensity, a.lavastorm, a.effect_mix, a.p84, a.p4a8, a.sky_scale] {
        f32s(&mut v, x);
    }
    i32s(&mut v, a.overcast);
    for x in [a.p5ec, a.p5f0, a.p5f4, a.p5f8, a.p5fc, a.p600, a.p604] {
        i32s(&mut v, x);
    }
    f32s(&mut v, a.p608);
    for x in [a.p60c, a.p610, a.p614, a.p618] {
        i32s(&mut v, x);
    }
    f32s(&mut v, a.p61c);
    out.push(("transition".into(), std::mem::take(&mut v)));
    for x in [a.pf24, a.pf28, a.pf2c] {
        f32s(&mut v, x);
    }
    v3(&mut v, &a.pf30);
    v3(&mut v, &a.pf3c);
    f32s(&mut v, a.pf48);
    out.push(("stars".into(), std::mem::take(&mut v)));
    i32s(&mut v, a.p1190);
    i32s(&mut v, a.p1194);
    for x in [a.p1198, a.p119c, a.p11a0, a.p11a4, a.p11a8] {
        f32s(&mut v, x);
    }
    out.push(("thunder".into(), std::mem::take(&mut v)));
    v3(&mut v, &a.camera);
    i32s(&mut v, a.num_splashes() as i32);
    i32s(&mut v, a.fireballs().count() as i32);
    f32s(&mut v, a.overcast_interp());
    out.push(("camera, counts, overcast".into(), std::mem::take(&mut v)));
    m44(&mut v, &a.sky_rot);
    m44(&mut v, &a.corona_rot);
    m44(&mut v, &a.moon_rot);
    out.push(("sky rotations".into(), std::mem::take(&mut v)));
    for p in &a.patches[..14] {
        v4(&mut v, p);
    }
    out.push(("lightning patches".into(), std::mem::take(&mut v)));
    for (i, p) in a.particles.iter().enumerate() {
        v3(&mut v, &p.location);
        for c in [p.color.r, p.color.g, p.color.b, p.color.a] {
            f32s(&mut v, c);
        }
        f32s(&mut v, p.ground);
        i32s(&mut v, p.liquid);
        out.push((format!("weather particle {i}"), std::mem::take(&mut v)));
    }
    for s in 10..15 {
        let pal = a.pals.get(&s);
        i32s(&mut v, pal.map_or(0, |p| p.len()) as i32);
        for m in pal.into_iter().flatten() {
            i32s(&mut v, m.n as i32);
            for l in m.used() {
                i32s(&mut v, l.texture);
                i32s(&mut v, l.blend);
                v.extend(l.color);
            }
        }
        out.push((format!("sprite {s}'s palette"), std::mem::take(&mut v)));
    }
    out
}

/// What the run went through, so a pass means something.
#[derive(Debug, Default)]
struct Seen {
    fireballs: usize,
    fire_bb: usize,
    splashes: usize,
    liquid: usize,
    patches: usize,
    stars: usize,
}

fn scenario(planted: bool, mutated: bool) -> Option<(usize, usize, usize)> {
    let mut o = Oracle::open_at("oracle_atmos")?;
    let mut rand = Rand(1);
    let t0 = 1000i64;
    o.call(1200, &t0.to_le_bytes());
    let mut sys = System::<M>::new(t0, 2000);
    let mut a = Atmosphere::<M>::new(t0, &mut rand);
    if mutated {
        // a code mutation: one of Init's rain constants
        a.p68[0].y = F::new(-0.2104);
    }
    o.call(1201, &77u32.to_le_bytes());
    rand.0 = 77;

    // the Lavastorm's two definitions, when the disc is there
    let mut lava = (-1, -1);
    let mut lava_ours = (-1, -1);
    let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if let Ok(mut disc) = iso::Iso::open(&iso) {
        let fx = esf::Container::open(disc.read("DATA2/SPELLFX.CSF").unwrap()).unwrap();
        let defs: Vec<(i32, Vec<u8>)> = (0..fx.nodes.len())
            .filter(|&i| fx.nodes[i].ty == 0xC000)
            .filter_map(|n| fx.child(n, 0xC020).map(|d| (fx.node(d).version as i32, fx.body(d).to_vec())))
            .take(40)
            .collect();
        let mut ks = Vec::new();
        for (k, (ver, body)) in [&defs[7], &defs[31]].into_iter().enumerate() {
            let mut p = ver.to_le_bytes().to_vec();
            p.extend(body);
            let pk = i32::from_le_bytes(o.call(1202, &p)[..4].try_into().unwrap());
            let mut d = Definition::<M>::parse(body, *ver).expect("a definition");
            if k == 0 {
                Atmosphere::tune_fireball_head(&mut d);
            }
            ks.push((pk, sys.add_definition(d) as i32));
        }
        lava = (ks[0].0, ks[1].0);
        lava_ours = (ks[0].1, ks[1].1);
    } else {
        eprintln!("no disc: the Lavastorm is not exercised");
    }

    let pals = palettes();
    let mut p = Vec::new();
    for pal in &pals {
        i32s(&mut p, pal.len() as i32);
        for e in pal {
            i32s(&mut p, e.len() as i32);
            for l in e {
                i32s(&mut p, l.texture);
                i32s(&mut p, l.blend);
                p.extend(l.color);
            }
        }
    }
    let sky_tex = [40, 41, 42];
    for x in sky_tex {
        i32s(&mut p, x);
    }
    for x in [300, 301, lava.0, lava.1] {
        i32s(&mut p, x);
    }
    o.call(1203, &p);
    a.set_resources(10, 11, 12, [13, 14], sky_tex, 300, 301, lava_ours);
    a.pals = HashMap::new();
    for (k, pal) in pals.iter().enumerate() {
        a.pals.insert(10 + k as i32, pal.iter().map(|e| Material::new(e)).collect());
    }

    let mut ground = Ground;
    let mut snd = Log::default();
    let mut t = t0;
    let mut pt = t0;
    let (mut billboards, mut sounds, mut flashes) = (0usize, 0usize, 0usize);
    let frames = 4200;
    let mut seen = Seen::default();
    for frame in 0..frames {
        let req = |o: &mut Oracle, op: u32, x: i32, y: i32| {
            let mut p = x.to_le_bytes().to_vec();
            p.extend(y.to_le_bytes());
            i32::from_le_bytes(o.call(op, &p)[..4].try_into().unwrap())
        };
        match frame {
            20 => assert_eq!(req(&mut o, 1204, 1, 0), a.set_weather(1, false)),
            400 => assert_eq!(req(&mut o, 1204, 6, 0), a.set_weather(6, false)),
            1500 => assert_eq!(req(&mut o, 1205, 2, 0), a.set_default_sky(2, false)),
            2000 => assert_eq!(req(&mut o, 1204, 2, 0), a.set_weather(2, false)),
            2600 => assert_eq!(req(&mut o, 1204, 4, 1), a.set_weather(4, true)),
            2700 => assert_eq!(req(&mut o, 1204, 5, 0), a.set_weather(5, false)),
            3300 => assert_eq!(req(&mut o, 1204, 0, 0), a.set_weather(0, false)),
            3400 | 3500 => {
                let i = (frame == 3400) as i32;
                assert_eq!(i32::from_le_bytes(o.call(1206, &i.to_le_bytes())[..4].try_into().unwrap()), 0);
                a.set_plane_of_sky(i);
            }
            3800 => assert_eq!(req(&mut o, 1205, 0, 1), a.set_default_sky(0, true)),
            3900 => assert_eq!(req(&mut o, 1205, 1, 0), a.set_default_sky(1, false)),
            _ => {}
        }
        t += match frame {
            1234 => 900,
            f => 30 + (f * 7 % 23) as i64,
        };
        pt += 33;
        let hour = (20.0 + frame as f32 * 0.004) % 24.0;
        let vol = 0.8f32;
        // walk east over the liquid edge and back; one jump far away
        let x = -20.0 + ((frame % 800) as f32 - 400.0).abs() * 0.1 + if frame >= 3000 { 500.0 } else { 0.0 };
        let pos = [x, 4.0 + (frame as f32 * 0.01).sin(), 2.0 + (frame as f32 * 0.003).cos() * 3.0];
        let ang = frame as f32 * 0.01;
        let dir = [ang.sin() * 0.9, -0.2f32, ang.cos() * 0.9];
        let mut view = [0f32; 16];
        view[0] = ang.cos();
        view[2] = ang.sin();
        view[5] = 1.0;
        view[8] = -ang.sin();
        view[10] = ang.cos();
        view[12] = -pos[0];
        view[13] = -pos[1];
        view[14] = -pos[2];
        view[15] = 1.0;
        let near = [dir[0], dir[1], dir[2], -(dir[0] * pos[0] + dir[1] * pos[1] + dir[2] * pos[2]) - 0.5];
        let sky_rgb = [(frame % 256) as u8, 100, 200];
        let lava_i = if (2700..3200).contains(&frame) { 0.5f32 } else { 0.0 };

        let mut p = t.to_le_bytes().to_vec();
        for x in [hour, vol].iter().chain(&pos).chain(&dir).chain(&view).chain(&near) {
            p.extend(x.to_le_bytes());
        }
        p.extend(sky_rgb);
        p.extend(lava_i.to_le_bytes());
        p.extend(pt.to_le_bytes());
        let want = o.call(1207, &p);

        let vv = |k: usize| Vect3::<M>::of(view[k], view[k + 1], view[k + 2]);
        let mut vm = Matrix44::<M>::identity();
        for i in 0..4 {
            let r = vv(i * 4);
            vm.m[i] = Vect4::new(r.x, r.y, r.z, F::new(view[i * 4 + 3]));
        }
        let cam = Camera { position: Vect3::of(pos[0], pos[1], pos[2]), dir: Vect3::of(dir[0], dir[1], dir[2]), view: vm, near: Plane { n: Vect3::of(near[0], near[1], near[2]), d: F::new(near[3]) } };
        snd.calls.clear();
        let mut r = Raster::default();
        a.update(&mut ground, &mut snd, &mut sys, &cam, t, F::new(hour), F::new(vol), &mut rand);
        let lit = a.get_lightning(&mut rand);
        a.set_sky_color([sky_rgb[0], sky_rgb[1], sky_rgb[2]]);
        a.set_lavastorm_intensity(F::new(lava_i), &mut sys);
        a.render_sky(&cam, &mut r, &mut rand);
        for (_, bbs) in sys.render(pt) {
            r.runs.push((-2, bbs));
        }
        a.render_weather(&cam, &mut r);
        if planted && frame == 3000 {
            a.particles[150].location.x += F::new(1e-5);
        }

        let mine = ours(&a, lit, &r, &snd);
        let mut at = 0usize;
        for (label, bytes) in &mine {
            let theirs = &want[at.min(want.len())..(at + bytes.len()).min(want.len())];
            if theirs != &bytes[..] {
                let w = |b: &[u8]| b.chunks(4).map(|c| format!("{:08x}", u32::from_le_bytes(c.try_into().unwrap_or([0; 4])))).collect::<Vec<_>>().join(" ");
                panic!("frame {frame}: {label} differs\n  oracle {}\n  ours   {}", w(theirs), w(bytes));
            }
            at += bytes.len();
        }
        // the fireballs (ours have no layout to compare; their fields do)
        let fb: Vec<_> = a.fireballs().collect();
        for (k, f) in fb.iter().enumerate() {
            let mut v = Vec::new();
            v3(&mut v, &f.from);
            v3(&mut v, &f.to);
            f32s(&mut v, f.t);
            f32s(&mut v, f.speed);
            i32s(&mut v, f.landed);
            i32s(&mut v, sys.emitter(f.e0).particles.len() as i32);
            i32s(&mut v, sys.emitter(f.e1).particles.len() as i32);
            assert_eq!(&want[at..at + v.len()], &v[..], "frame {frame}: fireball {k}");
            at += v.len();
        }
        assert_eq!(at, want.len(), "frame {frame}: the oracle sent more");
        billboards += r.runs.iter().map(|x| x.1.len()).sum::<usize>();
        sounds += snd.calls.len();
        flashes += lit.is_some_and(|x| x > F::ZERO) as usize;
        seen.fireballs = seen.fireballs.max(fb.len());
        seen.fire_bb += r.runs.iter().filter(|x| x.0 == -2).map(|x| x.1.len()).sum::<usize>();
        seen.splashes = seen.splashes.max(a.num_splashes());
        seen.liquid = seen.liquid.max(a.particles.iter().filter(|p| p.liquid == 1).count());
        seen.patches += r.runs.iter().filter(|x| x.0 == 4).map(|x| x.1.len()).sum::<usize>();
        seen.stars += r.runs.iter().filter(|x| x.0 == 3).map(|x| x.1.len()).sum::<usize>();
    }
    eprintln!("{seen:?}");
    assert!(seen.splashes > 0 && seen.liquid > 0 && seen.patches > 0 && seen.stars > 0, "{seen:?}");
    assert!(lava.0 == -1 || (seen.fireballs > 0 && seen.fire_bb > 0), "the Lavastorm ran: {seen:?}");
    Some((billboards, sounds, flashes))
}

#[test]
fn the_atmosphere_matches_the_decomp() {
    let Some((b, s, l)) = scenario(false, false) else { return };
    eprintln!("4200 frames equal: {b} billboards, {s} sound calls, {l} lightning frames");
    assert!(b > 200_000 && s > 100 && l > 10, "a real load: {b} billboards, {s} sound calls, {l} flashes");
}

#[test]
fn a_planted_difference_fails() {
    match std::panic::catch_unwind(|| scenario(true, false)) {
        Ok(None) => {}
        Ok(Some(_)) => panic!("a planted difference passed"),
        Err(_) => {}
    }
}

#[test]
fn a_mutated_constant_fails() {
    match std::panic::catch_unwind(|| scenario(false, true)) {
        Ok(None) => {}
        Ok(Some(_)) => panic!("a mutated constant passed"),
        Err(_) => {}
    }
}
