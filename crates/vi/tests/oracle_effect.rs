//! The effect volumes against the decomp, frame by frame: eqoa-port's
//! `build/oracle_effect` runs SUPPORT's VIEffectVolume, VIBoid and
//! VIEffectVolumeSprite over a recording raster and scene with newlib's
//! rand() and a clock of the test's, and every request here is run on it and
//! on `vi::effect` under `Ieee`. `GlobalInit`'s cards are compared once;
//! each frame, every fan (material, uv, normal, colour, position of all six
//! vertices), every boid sprite rastered with its model-view, every sprite
//! copied, released or set walking, and each volume's state with all its
//! particles.
//!
//! Five volumes, one of each kind, with hour windows that wrap midnight,
//! ramp, span the day or are empty, watched by a camera that circles in and
//! out of their fade distance over a day; the clock stutters and hitches,
//! and one box is moved far enough to re-seed its volume.
//!
//! SKIPs without the oracle (`make oracle` in eqoa-port).

mod common;
use common::Oracle;
use vi::csprite::Rand;
use vi::effect::{self, Camera, Cards, Draw, Host, Particles, Volume, VolumeSprite};
use vi::fpu::{F, Ieee};
use vi::geom::{BBox, Matrix44, Vect4};
use vi::vect::Vect3;

type M = Ieee;

#[derive(Default)]
struct Scene {
    next_material: i32,
    next_sprite: i32,
    calls: Vec<[i32; 3]>,
    hour: f32,
    blends: Vec<(i32, i32)>,
}

impl Host<M> for Scene {
    fn create_material(&mut self, surface: i32, blend: i32) -> i32 {
        let k = self.next_material;
        self.next_material += 1;
        self.blends.push((blend, surface));
        k
    }
    fn release_material(&mut self, _m: i32) {}
    fn copy_sprite(&mut self, sprite: i32) -> i32 {
        let k = self.next_sprite;
        self.next_sprite += 1;
        self.calls.push([1, sprite, k]);
        k
    }
    fn release_sprite(&mut self, sprite: i32) {
        self.calls.push([2, sprite, 0]);
    }
    fn set_locomotion(&mut self, sprite: i32, anim: i32) {
        self.calls.push([3, sprite, anim]);
    }
    fn hour(&self) -> F<M> {
        F::new(self.hour)
    }
}

fn w(v: &mut Vec<u8>, x: F<M>) {
    v.extend(x.0.to_bits().to_le_bytes());
}
fn i(v: &mut Vec<u8>, x: i32) {
    v.extend(x.to_le_bytes());
}
fn v3(v: &mut Vec<u8>, x: &Vect3<M>) {
    w(v, x.x);
    w(v, x.y);
    w(v, x.z);
}
fn m44(v: &mut Vec<u8>, m: &Matrix44<M>) {
    for r in &m.m {
        for x in [r.x, r.y, r.z, r.w] {
            w(v, x);
        }
    }
}

/// The volume and its particles, laid out as the oracle's `Dump`.
fn dump(vol: &Volume<M>) -> Vec<u8> {
    let mut v = Vec::new();
    v.extend(vol.time.to_le_bytes());
    i(&mut v, vol.reseed as i32);
    i(&mut v, vol.ty);
    for x in [vol.p28, vol.density, vol.p30, vol.p34, vol.p38] {
        w(&mut v, x);
    }
    i(&mut v, vol.flags as i32);
    i(&mut v, vol.surface);
    i(&mut v, vol.sprite);
    i(&mut v, vol.dirty as i32);
    i(&mut v, vol.material);
    let n = match &vol.particles {
        Particles::None => 0,
        Particles::Leaves(p) => p.len(),
        Particles::Shafts(p) => p.len(),
        Particles::Fog(p) => p.len(),
        Particles::Dust(p) => p.len(),
        Particles::Flock(p) => p.len(),
    };
    i(&mut v, n as i32);
    w(&mut v, vol.day_night);
    w(&mut v, vol.fade);
    v3(&mut v, &vol.wind);
    w(&mut v, vol.p6c);
    match &vol.particles {
        Particles::None => {}
        Particles::Leaves(p) => {
            for l in p {
                i(&mut v, l.state);
                i(&mut v, l.side);
                w(&mut v, l.wobble);
                v3(&mut v, &l.pos);
                for x in [l.floor, l.scale, l.life] {
                    w(&mut v, x);
                }
            }
        }
        Particles::Shafts(p) => {
            for s in p {
                i(&mut v, s.state);
                v3(&mut v, &s.pos);
                for x in [s.width, s.height, s.age, s.life] {
                    w(&mut v, x);
                }
            }
        }
        Particles::Fog(p) => {
            for s in p {
                i(&mut v, s.state);
                v3(&mut v, &s.pos);
                i(&mut v, s.card);
                for x in [s.size, s.height, s.age, s.life] {
                    w(&mut v, x);
                }
            }
        }
        Particles::Dust(p) => {
            for s in p {
                i(&mut v, s.state);
                v3(&mut v, &s.pos);
                i(&mut v, s.frame);
                i(&mut v, s.base);
                for x in [s.size_x, s.size_y, s.age, s.rate, s.life, s.inv_life] {
                    w(&mut v, x);
                }
                v3(&mut v, &s.end);
            }
        }
        Particles::Flock(p) => {
            for b in p {
                i(&mut v, b.sprite);
                v3(&mut v, &b.hpr);
                w(&mut v, b.p10);
                for x in [b.dir, b.pos, b.sep, b.align, b.coh, b.goal] {
                    v3(&mut v, &x);
                }
                i(&mut v, b.count);
                m44(&mut v, &b.xform);
                i(&mut v, b.pa0);
                w(&mut v, b.pa4);
                w(&mut v, b.pa8);
            }
        }
    }
    v
}

/// A rigid transform as the engine writes one: rotation about y by `yaw`,
/// then the translation row.
fn rigid(yaw: f32, t: [f32; 3]) -> [f32; 16] {
    let (s, c) = yaw.sin_cos();
    [c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, t[0], t[1], t[2], 1.0]
}

/// Its inverse (rotation transposed, translation rotated back).
fn inverse(m: &[f32; 16]) -> [f32; 16] {
    let r = |i: usize, j: usize| m[i * 4 + j];
    let mut o = [0f32; 16];
    for a in 0..3 {
        for b in 0..3 {
            o[a * 4 + b] = r(b, a);
        }
    }
    for b in 0..3 {
        o[12 + b] = -(m[12] * r(b, 0) + m[13] * r(b, 1) + m[14] * r(b, 2));
    }
    o[15] = 1.0;
    o
}

fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut o = [0f32; 16];
    for r in 0..4 {
        for c in 0..4 {
            o[r * 4 + c] = (0..4).map(|k| a[r * 4 + k] * b[k * 4 + c]).sum();
        }
    }
    o
}

fn mat(a: &[f32; 16]) -> Matrix44<M> {
    let row = |r: usize| Vect4::new(F::new(a[r * 4]), F::new(a[r * 4 + 1]), F::new(a[r * 4 + 2]), F::new(a[r * 4 + 3]));
    Matrix44 { m: [row(0), row(1), row(2), row(3)] }
}

fn words(b: &[u8]) -> String {
    b.chunks(4).map(|c| format!("{:08x}", u32::from_le_bytes(c.try_into().unwrap_or([0; 4])))).collect::<Vec<_>>().join(" ")
}

fn scenario(planted: bool, mutated: bool) -> Option<(usize, usize)> {
    let mut o = Oracle::open_at("oracle_effect")?;
    let mut cards = Cards::<M>::new();
    let want = o.call(1300, &[]);
    let mut ours = Vec::new();
    for side in &cards.fall {
        for c in side {
            for v in c {
                v3(&mut ours, v);
            }
        }
    }
    for c in cards.fog.iter().chain(cards.dust.iter()) {
        for v in c {
            v3(&mut ours, v);
        }
    }
    // the 48 bytes past sDustCard, which a mote born on frame 60 reads
    let (want, past) = want.split_at(want.len() - 48);
    for (k, c) in past.chunks(12).enumerate() {
        let g = |j: usize| F::new(f32::from_le_bytes(c[j * 4..j * 4 + 4].try_into().unwrap()));
        cards.past[k] = Vect3::new(g(0), g(1), g(2));
    }
    eprintln!("past sDustCard in this build: {}", words(past));
    assert_eq!(want.len(), ours.len());
    for (k, (a, b)) in want.chunks(4).zip(ours.chunks(4)).enumerate() {
        assert_eq!(a, b, "GlobalInit's cards, word {k}");
    }
    o.call(1301, &99u32.to_le_bytes());
    let mut rand = Rand(99);
    let mut scene = Scene { next_sprite: 100, ..Default::default() };

    // (type, density, p28, p30, p34, p38, flags, surface, sprite, box, yaw, at)
    #[allow(clippy::type_complexity)]
    let specs: [(i32, f32, f32, f32, f32, f32, u32, i32, i32, [f32; 6], f32, [f32; 3]); 5] = [
        (effect::LEAVES, 0.5, 0.7, 60.0, 0.0, 0.0, 0, 11, -1, [-5.0, -4.0, -5.0, 5.0, 4.0, 5.0], 0.3, [10.0, 2.0, -4.0]),
        (effect::SHAFTS, 1.0, 0.4, 60.0, 6.0, 18.0, 2, 12, -1, [-6.0, -5.0, -6.0, 6.0, 5.0, 6.0], -0.8, [-12.0, 6.0, 3.0]),
        (effect::FOG, 0.3, 0.9, 50.0, 20.0, 6.0, 0, 13, -1, [-3.0, -1.0, -8.0, 3.0, 1.0, 8.0], 1.2, [0.0, 0.0, 20.0]),
        (effect::DUST, 0.2, 0.5, 70.0, 9.5, 14.25, 0, 14, -1, [-2.5, -3.0, -4.0, 2.5, 3.0, 4.0], 2.0, [25.0, 1.0, 10.0]),
        (effect::FLOCK, 1.0, 1.0, 60.0, 0.0, 24.0, 0, -1, 7, [-10.0, -6.0, -10.0, 10.0, 6.0, 10.0], 0.0, [-5.0, 15.0, -20.0]),
    ];
    let mut vols: Vec<VolumeSprite<M>> = Vec::new();
    let mut places = Vec::new();
    for (k, s) in specs.iter().enumerate() {
        let mut p = Vec::new();
        i(&mut p, s.0);
        for x in [s.1, s.2, s.3, s.4, s.5] {
            p.extend(x.to_le_bytes());
        }
        p.extend(s.6.to_le_bytes());
        i(&mut p, s.7);
        i(&mut p, s.8);
        for x in s.9 {
            p.extend(x.to_le_bytes());
        }
        let r = o.call(1302, &p);
        let ri = |j: usize| i32::from_le_bytes(r[j * 4..j * 4 + 4].try_into().unwrap());
        assert_eq!(ri(0), k as i32);
        let b = BBox { min: Vect3::of(s.9[0], s.9[1], s.9[2]), max: Vect3::of(s.9[3], s.9[4], s.9[5]) };
        let mut vs = VolumeSprite { bbox: b, local_bbox: b, volume: Volume::default() };
        vs.volume.init(&b, &mut scene);
        vs.volume.set_type(s.0, F::new(s.1), &mut scene);
        vs.volume.p28 = F::new(s.2);
        vs.volume.p34 = F::new(s.4);
        vs.volume.p38 = F::new(s.5);
        vs.volume.p30 = F::new(s.3);
        vs.volume.attach_surface(s.7, &mut scene);
        vs.volume.attach_sprite(s.8);
        vs.volume.flags = s.6;
        assert_eq!(ri(1), vs.volume.material, "volume {k}'s material");
        if ri(1) != -1 {
            let (blend, surf) = *scene.blends.last().unwrap();
            assert_eq!((ri(2), ri(3)), (blend, surf), "volume {k}'s material: blend, texture");
        }
        vols.push(vs);
        places.push(rigid(s.10, s.11));
    }
    if mutated {
        // a code mutation: one corner of one of GlobalInit's fog cards
        cards.fog[3][1].y = cards.fog[3][1].y + F::new(1e-6);
    }

    let mut now = 5000i64;
    let mut normal = Vect3::default();
    let (mut fans, mut boids) = (0usize, 0usize);
    let mut per = [0usize; 5];
    let mut ramps = [(false, false, false); 5];
    for frame in 0..900 {
        now += match frame {
            400 => 700,
            f => 10 + (f * 13 % 31) as i64,
        };
        let hour = (4.0 + frame as f32 * 0.03) % 24.0;
        scene.hour = hour;
        // circle the field, sometimes far enough to fade it all out
        let a = frame as f32 * 0.011;
        let r = 20.0 + 60.0 * (frame as f32 * 0.004).sin().abs();
        let eye = [a.cos() * r, 4.0 + (frame as f32 * 0.02).sin() * 3.0, a.sin() * r];
        let dir = [-a.cos() * 0.97, -0.24, -a.sin() * 0.97];
        let cam_world = rigid(a + 1.3, eye);
        let view = inverse(&cam_world);
        if frame == 500 {
            let b = [-9.0f32, -4.0, -5.0, 9.0, 4.0, 5.0];
            let mut p = Vec::new();
            i(&mut p, 0);
            for x in b {
                p.extend(x.to_le_bytes());
            }
            o.call(1304, &p);
            vols[0].bbox = BBox { min: Vect3::of(b[0], b[1], b[2]), max: Vect3::of(b[3], b[4], b[5]) };
        }
        for (k, vs) in vols.iter_mut().enumerate() {
            let mv = mul(&places[k], &view);
            let mut p = Vec::new();
            i(&mut p, k as i32);
            p.extend(now.to_le_bytes());
            p.extend(hour.to_le_bytes());
            for x in eye.iter().chain(&dir).chain(&view).chain(&cam_world).chain(&mv) {
                p.extend(x.to_le_bytes());
            }
            let want = o.call(1303, &p);

            let cam = Camera { position: Vect3::of(eye[0], eye[1], eye[2]), dir: Vect3::of(dir[0], dir[1], dir[2]), view: mat(&view), world: mat(&cam_world) };
            let mut draw = Draw { normal, ..Default::default() };
            scene.calls.clear();
            vs.raster(&cam, &mat(&mv), now, &cards, &mut rand, &mut scene, &mut draw);
            normal = draw.normal;
            if planted && frame == 600 && k == 3 {
                vs.volume.wind.x = vs.volume.wind.x + F::new(1e-4);
                if let Particles::Dust(p) = &mut vs.volume.particles {
                    p[0].age = p[0].age + F::new(1e-6);
                }
            }

            let mut v = Vec::new();
            i(&mut v, draw.fans.len() as i32);
            for f in &draw.fans {
                i(&mut v, f.material);
                for (uv, p) in &f.verts {
                    w(&mut v, uv[0]);
                    w(&mut v, uv[1]);
                    v3(&mut v, &f.normal);
                    for x in [f.color.r, f.color.g, f.color.b, f.color.a] {
                        w(&mut v, x);
                    }
                    v3(&mut v, p);
                }
            }
            let at_fans = v.len();
            i(&mut v, draw.sprites.len() as i32);
            for (id, m) in &draw.sprites {
                i(&mut v, *id);
                m44(&mut v, m);
            }
            let at_sprites = v.len();
            i(&mut v, scene.calls.len() as i32);
            for c in &scene.calls {
                for x in c {
                    i(&mut v, *x);
                }
            }
            let at_calls = v.len();
            v.extend(dump(&vs.volume));
            if v != want {
                let at = v.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(v.len().min(want.len())) / 4 * 4;
                let part = if at < at_fans {
                    "fans"
                } else if at < at_sprites {
                    "sprites"
                } else if at < at_calls {
                    "scene calls"
                } else {
                    "state"
                };
                let lo = at.saturating_sub(16);
                panic!(
                    "frame {frame}, volume {k}: {part} differ at byte {at} (lengths {} / {})\n  oracle {}\n  ours   {}",
                    want.len(),
                    v.len(),
                    words(&want[lo..(at + 32).min(want.len())]),
                    words(&v[lo..(at + 32).min(v.len())])
                );
            }
            fans += draw.fans.len();
            boids += draw.sprites.len();
            per[k] += draw.fans.len() + draw.sprites.len();
            let (fd, dn) = (vs.volume.fade.0, vs.volume.day_night.0);
            ramps[k].0 |= fd > 0.0 && fd < 1.0;
            ramps[k].1 |= fd == 0.0;
            ramps[k].2 |= dn > 0.0 && dn < 1.0;
        }
    }
    eprintln!("drawn per volume {per:?}; (fading, faded, hour ramp) {ramps:?}");
    assert!(per.iter().all(|&n| n > 0), "every volume drew: {per:?}");
    assert!(ramps.iter().all(|r| r.0 && r.1), "every volume faded in and out: {ramps:?}");
    assert!(ramps[2].2 && ramps[3].2, "the hour ramps ran: {ramps:?}");
    Some((fans, boids))
}

#[test]
fn effect_volumes_match_the_decomp() {
    let Some((f, b)) = scenario(false, false) else { return };
    eprintln!("900 frames of 5 volumes equal: {f} fans, {b} boid sprites");
    assert!(f > 10_000 && b > 1_000, "a real load: {f} fans, {b} boid sprites");
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
