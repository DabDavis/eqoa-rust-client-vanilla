//! Dynamic Lighting's point lights (the options' Extended > Lighting:
//! `render::lit` draws them): the lights a frame is lit by past the personal
//! light, which the console never draws. `SetupTimeOfDayLights` would light
//! with them were `VIRaster::MaxPointLights` more than its 1: it asks the
//! world for the point-light sprites (`VIPointLight`, type 8) within 500 of
//! the camera (`VIWorld::QueryEmitters`), keyed as `VISprite::QueryEmitters`
//! keys an emitter, its radius over its distance, and keeps the most keys.
//! So does [`choose`], of every light here:
//!
//! - **Placed point lights** (0x2B00, `ParsePointLightObj`: a radius and a
//!   colour), a zone actor's sprite or a group's member. None of the disc's
//!   worlds places one (ARENA.CSF, which no world loads, holds the disc's
//!   ten): the editor's lights were baked into the zones' colour buffers.
//! - **Fires**: every additive particle emitter a zone places (a torch's,
//!   a brazier's, a candle's, a lamp's glow: 161 definitions placed 10,568
//!   times over the six worlds, most of them orange), lit as its own flame:
//!   the colour of its gradient over a particle's life, by alpha, its reach
//!   and strength by its particles' size ([`fire`]), a little above its
//!   nozzle, flickering. Smoke (a grey additive) and the alpha-blended
//!   emitters cast none. A fire past the particle system's 100 (its flame
//!   purged, `particles`) casts none either, fading out from 85.
//! - **Lights of the moment**, out of what a frame draws: each additive
//!   emitter of the spell effects, the items in hands (an epic weapon's
//!   glow) and the creatures' own (a fire creature's flames), at its
//!   particles' centre and as bright as they are many ([`from_batches`]),
//!   and each spell bolt ([`from_strips`]).
//!
//! Fires and placed lights are fixed (`render::PointLight::fixed`): a
//! zone's colour buffers were baked with the scene's fixed lights (measured
//! over Tunaria's baked actors: red 80 of 255 within 4 of a fire, 17 past 40,
//! blue 36 to 37 at both), so a baked instance is not lit by them again.

use esf::{Library, Ref};
use glam::{Mat4, Vec3};
use render::PointLight;
use render::billboard::{Batch, Strips};
use std::collections::HashMap;
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::particle::Definition;
use vi::vect::Vect3;
use vi::world::{SpriteKind, World};

/// How far from the camera `SetupTimeOfDayLights` asks for point lights
/// (`v.m_sphere.r`).
pub const QUERY_RADIUS: f32 = 500.0;

/// A fire casts light while its flame is drawn: the particle system purges
/// an emitter past 100 of the eye (`particles::Particles::frame`). Its light
/// fades out over the last `FIRE_FADE` before it.
pub const FIRE_REACH: f32 = 100.0;
pub const FIRE_FADE: f32 = 15.0;

/// How near an actor's flames are to be one light (`Fixed::lights`).
pub const MERGE: f32 = 4.0;

/// The light an emitter's flame casts (`fire`): its colour at the centre,
/// its range, and how far above the nozzle the flame's middle is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fire {
    pub color: Vec3,
    pub range: f32,
    pub rise: f32,
}

/// A particle definition's light, if it casts one: only an additive one
/// (blend 1), whose gradient is bright (the grey additive smoke is not).
/// Its colour is the gradient's over a particle's life, each entry by its
/// alpha, with the brightest channel at the light's strength. A flame (its
/// particles rise) reaches farther and burns stronger the larger its
/// particles are: a candle's 0.2 reaches 16 at 0.45, a torch's 0.36 19 at
/// 0.49, a brazier's 0.65 25 at 0.56. A glow that does not rise (a street
/// lamp's halo, 1 to 2.6 across, Freeport's 0x3F67600E on its post) is a
/// lamp's: 18 at 0.45, whatever its size. The light sits half the way a
/// particle rises in its life above the nozzle.
pub fn fire(d: &Definition<Ee>) -> Option<Fire> {
    if d.blend != 1 {
        return None;
    }
    let a = &d.attrs;
    let (mut rgb, mut w) = (Vec3::ZERO, 0.0);
    for c in &a.gradient {
        let alpha = c.a.0.max(0.0);
        rgb += Vec3::new(c.r.0, c.g.0, c.b.0).max(Vec3::ZERO) * alpha;
        w += alpha;
    }
    if w <= 0.0 {
        return None;
    }
    let hue = rgb / w;
    let peak = hue.max_element();
    if peak < 0.35 {
        return None;
    }
    let size = a.start_size.0.max(a.end_size.0).max(0.0);
    let rises = a.velocity.0 > 0.0;
    let (range, strength) = if rises { ((12.0 + 20.0 * size).clamp(14.0, 26.0), (0.4 + 0.25 * size).clamp(0.4, 0.6)) } else { (18.0, 0.45) };
    let rise = (0.5 * a.velocity.0.max(0.0) * a.lifespan.0.max(0.0)).min(1.0);
    Some(Fire { color: hue / peak * strength * peak.min(1.0), range, rise })
}

/// A placed point light's record (0x2B00: an id, the radius, the colour's
/// four floats): its range and colour.
pub fn point_light_record(c: &esf::Container, node: esf::NodeId) -> Option<(f32, Vec3)> {
    let d = c.body(node);
    let f = |o: usize| d.get(o..o + 4).map(|b| f32::from_le_bytes(b.try_into().unwrap()));
    let (r, red, green, blue) = (f(4)?, f(8)?, f(12)?, f(16)?);
    (r > 0.0).then(|| (r, Vec3::new(red, green, blue).clamp(Vec3::ZERO, Vec3::ONE)))
}

/// A sprite a zone places that may be a light: an actor's own, or a group
/// member's at its transform within the actor.
struct Placed {
    actor: usize,
    member: Option<Matrix44<Ee>>,
    sprite: usize,
}

/// A fixed light where it is in the world.
#[derive(Clone, Copy, Debug)]
struct Lamp {
    /// the actor that places it
    actor: usize,
    at: Vec3,
    range: f32,
    color: Vec3,
    /// A fire's (it flickers, and fades out as its flame is purged); a
    /// placed light's is none.
    fire: Option<f32>,
}

/// The fixed lights of the zones the world has in: every placed point light
/// and fire, each found as its zone comes in (`add_from`, beside the
/// particles' emitters) and made the first time a frame asks (`lights`).
#[derive(Default)]
pub struct Fixed {
    placed: Vec<Placed>,
    made: usize,
    lamps: Vec<Lamp>,
    defs: HashMap<Ref, Option<Fire>>,
}

impl Fixed {
    /// The sprites the actors `first..` place that may be lights: particle
    /// sprites (fires) and sprites with no kind of their own (a point light
    /// is one), an actor's own or a group member's, as `Particles::add_from`
    /// finds the emitters.
    pub fn add_from(&mut self, world: &World<Ee>, first: usize) {
        let maybe = |k: &SpriteKind| matches!(k, SpriteKind::Particle { def: Some(_), .. } | SpriteKind::None);
        for (a, act) in world.actors.iter().enumerate().skip(first) {
            let Some(s) = act.sprite else { continue };
            match &world.sprites[s].kind {
                SpriteKind::Group(members) => {
                    for &(ms, angles, scale, t) in members {
                        if maybe(&world.sprites[ms].kind) {
                            let m = Matrix44::transform(&Vect3::of(angles[0], angles[1], angles[2]), F::new(scale), &Vect3::of(t[0], t[1], t[2]));
                            self.placed.push(Placed { actor: a, member: Some(m), sprite: ms });
                        }
                    }
                }
                k if maybe(k) => self.placed.push(Placed { actor: a, member: None, sprite: s }),
                _ => {}
            }
        }
    }

    /// How many fixed lights are made (`lights` makes them).
    pub fn len(&self) -> usize {
        self.lamps.len()
    }

    /// Makes the lights of the sprites found since the last call, then puts
    /// those that reach the eye at `eye` into `out`, a fire flickering by
    /// the time `now` (ms).
    pub fn lights(&mut self, world: &World<Ee>, lib: &Library, eye: Vec3, now: i64, out: &mut Vec<PointLight>) {
        while self.made < self.placed.len() {
            let p = &self.placed[self.made];
            self.made += 1;
            let act = &world.actors[p.actor];
            let m = match &p.member {
                Some(mem) => Matrix44::mul(mem, &act.object_to_world),
                None => act.object_to_world,
            };
            let nozzle = Vec3::new(m.m[3].x.0, m.m[3].y.0, m.m[3].z.0);
            let lamp = match &world.sprites[p.sprite].kind {
                SpriteKind::Particle { def: Some(def), .. } => {
                    let def = *def;
                    let fire = *self.defs.entry(def).or_insert_with(|| {
                        let c = lib.file(def.file);
                        let body = c.child(def.node, 0xC020)?;
                        fire(&Definition::<Ee>::parse(c.body(body), c.node(body).version as i32)?)
                    });
                    fire.map(|f| Lamp { actor: p.actor, at: nozzle + Vec3::Y * f.rise, range: f.range, color: f.color, fire: Some(seed(nozzle)) })
                }
                _ => {
                    let r = world.sprite_refs[p.sprite];
                    let c = lib.file(r.file);
                    // VIPointLight: at the query's matrix, its own radius
                    (c.node(r.node).ty == 0x2B00).then(|| point_light_record(c, r.node)).flatten().map(|(range, color)| Lamp { actor: p.actor, at: nozzle, range, color, fire: None })
                }
            };
            // one light for an actor's flames close together (a brazier's
            // several, a lamp's flame and glow): the brightest of each colour
            // and the farthest reach, between them
            match (lamp, self.lamps.last_mut()) {
                (Some(l), Some(last)) if last.actor == l.actor && last.fire.is_some() == l.fire.is_some() && (last.at - l.at).length() < MERGE => {
                    last.at = (last.at + l.at) * 0.5;
                    last.color = last.color.max(l.color);
                    last.range = last.range.max(l.range);
                }
                (Some(l), _) => self.lamps.push(l),
                (None, _) => {}
            }
        }
        let t = now as f32 / 1000.0;
        for l in &self.lamps {
            let d = (l.at - eye).length();
            let k = match l.fire {
                Some(s) => {
                    let fade = ((FIRE_REACH - d) / FIRE_FADE).clamp(0.0, 1.0);
                    fade * flicker(t, s)
                }
                None if d < QUERY_RADIUS + l.range => 1.0,
                None => 0.0,
            };
            if k > 0.0 {
                out.push(PointLight { at: l.at, range: l.range, color: l.color * k, fixed: true });
            }
        }
    }
}

/// A fire's own phase, off where it stands.
fn seed(p: Vec3) -> f32 {
    let h = (p.x * 12.9898 + p.y * 78.233 + p.z * 37.719).sin() * 43758.547;
    (h - h.floor()) * std::f32::consts::TAU
}

/// A flame's light from moment to moment: within 8% of its own, two waves
/// at a fire's own phase.
pub fn flicker(t: f32, seed: f32) -> f32 {
    1.0 + 0.08 * ((t * 7.3 + seed).sin() * 0.6 + (t * 17.9 + 2.3 * seed).sin() * 0.4)
}

/// The lights the additive emitters of `batches` cast this frame (a spell's
/// particles, a held item's glow, a fire creature's flames): each emitter's
/// at its particles' centre (each particle by its alpha and size), its
/// colour theirs with the brightest channel as strong as they are many (a
/// few sparks dim, a spell's cloud to 0.9), reaching farther the wider they
/// are spread.
pub fn from_batches(batches: &[Batch], out: &mut Vec<PointLight>) {
    for b in batches.iter().filter(|b| b.blend == 1 && !b.quads.is_empty()) {
        let (mut w, mut at, mut rgb) = (0.0, Vec3::ZERO, Vec3::ZERO);
        for q in &b.quads {
            let k = q.color[3].max(0.0) * (4.0 * q.half[0].abs() * q.half[1].abs());
            w += k;
            at += Vec3::from(q.center) * k;
            rgb += Vec3::new(q.color[0], q.color[1], q.color[2]).max(Vec3::ZERO) * k;
        }
        if w <= 0.0 {
            continue;
        }
        let (at, hue) = (at / w, rgb / w);
        let peak = hue.max_element();
        if peak < 0.2 {
            continue;
        }
        let spread = (b.quads.iter().map(|q| q.color[3].max(0.0) * (4.0 * q.half[0].abs() * q.half[1].abs()) * (Vec3::from(q.center) - at).length_squared()).sum::<f32>() / w).sqrt();
        let strength = 0.9 * (1.0 - (-(w * peak.min(1.0)) / 2.0).exp());
        out.push(PointLight { at, range: (8.0 + 3.0 * spread).clamp(6.0, 20.0), color: hue / peak * strength, fixed: false });
    }
}

/// The lights the spells' bolts cast this frame: each bolt's at its middle,
/// in its colour.
pub fn from_strips(strips: &[Strips], out: &mut Vec<PointLight>) {
    for s in strips {
        let (mut n, mut at, mut rgb) = (0.0, Vec3::ZERO, Vec3::ZERO);
        for (k, strip) in s.strips.iter().enumerate() {
            for (j, (_, p)) in strip.iter().enumerate() {
                let c = s.colors.get(k).and_then(|c| c.get(j)).copied().unwrap_or(s.color);
                at += Vec3::from(*p);
                rgb += Vec3::new(c[0], c[1], c[2]).max(Vec3::ZERO);
                n += 1.0;
            }
        }
        if n == 0.0 {
            continue;
        }
        let hue = rgb / n;
        let peak = hue.max_element();
        if peak < 0.2 {
            continue;
        }
        out.push(PointLight { at: at / n, range: 15.0, color: hue / peak * 0.8, fixed: false });
    }
}

/// How many point lights a frame takes past the personal light, which
/// `render::lit` puts first: `render::MAX_LIGHTS` in all natively, half that
/// in a page and on Android, where a phone's GPU pays for every one at every
/// fragment it reaches (`render::lit`'s tiles leave out the rest; one that
/// reaches the whole of a 1920x1080 frame costs about 0.08 ms on an RX 580).
pub fn budget() -> usize {
    if cfg!(any(target_arch = "wasm32", target_os = "android")) { render::MAX_LIGHTS / 2 - 1 } else { render::MAX_LIGHTS - 1 }
}

/// The frame's lights out of `candidates`, as `SetupTimeOfDayLights` would
/// choose them with more lights to give (`VIEmitterQuery`): of those whose
/// sphere is in the view (`view_proj`) and within `QUERY_RADIUS` of the eye,
/// the most by radius over distance from it, `budget()` at most.
/// Where more are in view than are taken, each one taken dims to nothing as
/// its key comes down to the first one's left out, so a light comes and goes
/// without a jump as the camera moves.
pub fn choose(candidates: &[PointLight], eye: Vec3, view_proj: Mat4) -> render::Lights {
    let planes = crate::particles::frustum(view_proj);
    let mut keyed: Vec<(f32, PointLight)> = candidates
        .iter()
        .filter(|l| l.range > 0.0 && (l.at - eye).length() < QUERY_RADIUS + l.range && planes.iter().all(|p| p.dot(l.at.extend(1.0)) >= -l.range))
        .map(|l| (l.range / (l.at - eye).length().max(0.01), *l))
        .collect();
    keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
    let most = budget();
    let cut = keyed.get(most).map(|k| k.0);
    let chosen: Vec<PointLight> = keyed
        .iter()
        .take(most)
        .map(|&(k, l)| match cut {
            Some(c) => {
                let x = ((k - c) / (0.25 * c)).clamp(0.0, 1.0);
                PointLight { color: l.color * (x * x * (3.0 - 2.0 * x)), ..l }
            }
            None => l,
        })
        .collect();
    render::Lights::new(&chosen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use render::billboard::Quad;

    fn definition(blend: i32, rgba: [f32; 4], size: f32) -> Definition<Ee> {
        let mut d = Definition::<Ee>::default();
        d.blend = blend;
        for g in d.attrs.gradient.iter_mut() {
            *g = vi::particle::Color { r: F::new(rgba[0]), g: F::new(rgba[1]), b: F::new(rgba[2]), a: F::new(rgba[3]) };
        }
        d.attrs.start_size = F::new(size);
        d.attrs.end_size = F::new(size * 0.5);
        d.attrs.velocity = F::new(2.0);
        d.attrs.lifespan = F::new(0.4);
        d
    }

    /// A torch's flame (the disc's most placed: orange, 0.36): a warm light
    /// reaching 19, the larger flames farther and stronger; a glow that does
    /// not rise a lamp's whatever its size; smoke and an alpha-blended
    /// emitter cast none.
    #[test]
    fn a_flame_casts_its_own_colour() {
        let torch = fire(&definition(1, [1.0, 0.68, 0.0, 0.5], 0.36)).expect("a torch's light");
        assert!((torch.range - 19.2).abs() < 1e-3, "{torch:?}");
        assert!((torch.color.x - 0.49).abs() < 1e-3 && (torch.color.y / torch.color.x - 0.68).abs() < 1e-3 && torch.color.z == 0.0, "its hue: {torch:?}");
        assert!((torch.rise - 0.4).abs() < 1e-3, "half its rise above the nozzle: {torch:?}");
        let candle = fire(&definition(1, [1.0, 0.68, 0.0, 0.5], 0.2)).unwrap();
        let brazier = fire(&definition(1, [1.0, 0.68, 0.0, 0.5], 0.65)).unwrap();
        assert!(candle.range < torch.range && torch.range < brazier.range && brazier.range < 26.0);
        assert!(candle.color.x < torch.color.x && torch.color.x < brazier.color.x);
        let mut halo = definition(1, [1.0, 0.92, 0.0, 0.5], 2.27);
        halo.attrs.velocity = F::ZERO;
        let lamp = fire(&halo).unwrap();
        assert_eq!((lamp.range, lamp.rise), (18.0, 0.0), "a lamp's halo: {lamp:?}");
        assert!((lamp.color.x - 0.45).abs() < 1e-3);
        assert_eq!(fire(&definition(1, [0.2, 0.2, 0.2, 0.5], 0.5)), None, "grey additive smoke");
        assert_eq!(fire(&definition(3, [1.0, 0.68, 0.0, 0.5], 0.36)), None, "an alpha-blended emitter");
        assert_eq!(fire(&definition(1, [1.0, 0.68, 0.0, 0.0], 0.36)), None, "a gradient with no alpha");
    }

    fn light(at: Vec3, range: f32) -> PointLight {
        PointLight { at, range, color: Vec3::ONE, fixed: true }
    }

    /// A camera at the origin looking down -z, far plane 1000.
    fn camera() -> Mat4 {
        glam::camera::rh::proj::directx::perspective(90f32.to_radians(), 1.0, 0.1, 1000.0)
    }

    /// The most by radius over distance, the ones out of view (sphere and
    /// all) and past the query's 500 left out, and the ones near the cut
    /// dimmed toward it.
    #[test]
    fn the_frame_takes_the_most_by_radius_over_distance() {
        let eye = Vec3::ZERO;
        let mut c: Vec<PointLight> = (1..=20).map(|k| light(Vec3::new(0.0, 0.0, -10.0 * k as f32), 10.0)).collect();
        c.push(light(Vec3::new(0.0, 0.0, 40.0), 10.0)); // behind the camera, out of reach of the view
        c.push(light(Vec3::new(0.0, 0.0, 5.0), 10.0)); // behind, but its sphere reaches in front
        c.push(light(Vec3::new(0.0, 0.0, -600.0), 50.0)); // past the query
        let l = choose(&c, eye, camera());
        let got = l.as_slice();
        assert_eq!(got.len(), budget());
        assert_eq!(got[0].at, Vec3::new(0.0, 0.0, 5.0), "the one behind whose sphere reaches into the view, nearest");
        assert!(got.iter().all(|p| p.at.z != 40.0 && p.at.z != -600.0));
        // the budget taken of 21 in view (15 natively: 16 with the personal
        // light), the last's key against the first left out's
        let last = got.last().unwrap();
        assert_eq!(last.at.z, -10.0 * (budget() - 1) as f32);
        assert!(last.color.x < 0.5, "dimmed near the cut: {last:?}");
        assert_eq!(got[1].color, Vec3::ONE, "well above it: whole");
        // fewer than can be taken: none dimmed
        let few = choose(&c[..5], eye, camera());
        assert!(few.as_slice().iter().all(|p| p.color == Vec3::ONE));
    }

    /// Freeport's torches off the disc (SKIPs without it): the row on the
    /// wall east of the river, each 0x1CCA51EE's flame at its member's place
    /// and half its rise above, orange, a torch's reach and fixed; none past
    /// the flames' 100 from the eye. The control: from 150 away, none of them.
    #[test]
    fn freeports_torches_light_their_wall() {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let (x, z) = (25352.0, 15590.0);
        let mut lib = Library::new();
        let w = lib.add("TUNARIA.ESF", crate::stream::open_file(&iso, "TUNARIA.ESF").unwrap());
        let mut src = crate::stream::FileZones::open(&iso, "TUNARIA.ESF").unwrap();
        crate::stream::fill(&mut lib, w, x, z, &mut src);
        let world = World::<Ee>::load(&lib, w).unwrap();
        let mut f = Fixed::default();
        f.add_from(&world, 0);
        let eye = Vec3::new(x, 60.0, z);
        let mut near = Vec::new();
        f.lights(&world, &lib, eye, 0, &mut near);
        let torches: Vec<&PointLight> = near.iter().filter(|l| (l.at.x - 25364.7).abs() < 1.5 && (15520.0..15650.0).contains(&l.at.z) && (l.at.y - 64.4).abs() < 0.3).collect();
        assert!(torches.len() >= 8, "the wall's torches: {torches:?}");
        for t in &torches {
            assert!(t.fixed && t.color.z == 0.0 && (t.color.y / t.color.x - 0.68).abs() < 0.01, "a torch's light: {t:?}");
            assert!((t.range - 19.2).abs() < 1e-3, "a torch's reach: {t:?}");
        }
        assert!(near.iter().all(|l| (l.at - eye).length() < FIRE_REACH), "none past the flames' reach");
        assert!(f.len() > torches.len(), "the zones' other flames made too");
        let mut far = Vec::new();
        f.lights(&world, &lib, eye + Vec3::new(150.0, 0.0, 0.0), 0, &mut far);
        assert!(far.iter().all(|l| (l.at.x - 25364.7).abs() > 1.5 || !(15520.0..15650.0).contains(&l.at.z)), "the control: the wall's torches from 150 away");
    }

    /// A placed point light's record (0x2B00) read as `ParsePointLightObj`
    /// reads it, on the disc's only ones, ARENA.CSF's ten (SKIPs without the
    /// disc): a radius of 5, 10 or 25 and a colour of full or half channels.
    #[test]
    fn the_discs_point_light_records() {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let c = esf::Container::open(iso::Iso::open(&iso).unwrap().read("ARENA.CSF").unwrap()).unwrap();
        let lights: Vec<(f32, Vec3)> = (0..c.nodes.len()).filter(|&n| c.node(n).ty == 0x2B00).filter_map(|n| point_light_record(&c, n)).collect();
        assert_eq!(lights.len(), 10);
        for (r, col) in &lights {
            assert!([5.0, 10.0, 25.0].contains(r), "{r}");
            assert!(col.to_array().iter().all(|v| [0.0, 1.0].contains(v) || (v - 0.502).abs() < 1e-3), "{col:?}");
        }
    }

    #[test]
    fn a_spell_cloud_casts_light_at_its_middle() {
        let q = |x: f32, a: f32| Quad { center: [x, 1.0, 0.0], half: [0.5, 0.5], frame: 0, color: [0.2, 0.6, 1.0, a] };
        let cloud = Batch { texture: 0, blend: 1, ztest: true, zwrite: false, quads: (0..40).map(|k| q(k as f32 * 0.1 - 2.0, 1.0)).collect() };
        let smoke = Batch { blend: 3, ..cloud.clone() };
        let mut out = Vec::new();
        from_batches(&[cloud.clone(), smoke], &mut out);
        assert_eq!(out.len(), 1, "an alpha-blended emitter casts none");
        let l = out[0];
        assert!((l.at - Vec3::new(-0.05, 1.0, 0.0)).length() < 1e-3, "its middle: {l:?}");
        assert!((l.color.z - 0.9 * (1.0 - (-20.0f32).exp())).abs() < 1e-3 && (l.color.x / l.color.z - 0.2).abs() < 1e-3, "its colour, near the most: {l:?}");
        assert!(!l.fixed, "a light of the moment lights a baked instance too");
        let mut few = Vec::new();
        from_batches(&[Batch { quads: cloud.quads[..1].to_vec(), ..cloud }], &mut few);
        assert!(few[0].color.z < 0.4, "one spark is dim: {:?}", few[0]);
    }
}
