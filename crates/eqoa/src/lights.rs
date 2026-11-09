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
//!   and each spell bolt ([`from_strips`]); held from frame to frame
//!   ([`Moment`]), so that none flashes: each frame's own jumped with the
//!   few particles it was made of, where it was, how bright and what
//!   colour, up to 18 times a second over metres of floor and wall at night
//!   (petition 1321), where photosensitivity guidance (WCAG 2.2's 2.3.1)
//!   allows 3.
//!
//! Where more are in view than a frame takes, the choice among them is held
//! from frame to frame too ([`Chooser`]): a light another takes the place of
//! dims out, and the one taking it brightens in, over `BUDGET_FADE`.
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
        let mut keyed = Vec::new();
        self.keyed(world, lib, eye, now, &mut keyed);
        out.extend(keyed.into_iter().map(|(_, l)| l));
    }

    /// `lights`, each with its key (its place in the list, which a zone
    /// coming in only adds to, from 1: 0 is no key's, `render::PointLight::key`):
    /// a `Chooser`'s.
    pub fn keyed(&mut self, world: &World<Ee>, lib: &Library, eye: Vec3, now: i64, out: &mut Vec<(u64, PointLight)>) {
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
        for (key, l) in self.lamps.iter().enumerate() {
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
                out.push((key as u64 + 1, PointLight { at: l.at, range: l.range, color: l.color * k, fixed: true, key: key as u64 + 1 }));
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
        out.push(PointLight { at, range: (8.0 + 3.0 * spread).clamp(6.0, 20.0), color: hue / peak * strength, fixed: false, key: 0 });
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
        out.push(PointLight { at: at / n, range: 15.0, color: hue / peak * 0.8, fixed: false, key: 0 });
    }
}

/// How near the lights of the moment are to be one light ([`Moment`]): a
/// body's effects (at its hands, its head, its feet) one, a caster's and its
/// target's (melee reach is 4.5) two.
pub const MOMENT_MERGE: f32 = 3.0;

/// How far a light of the moment's middle may move between two frames and
/// still be the same light ([`Moment`]), past its own motion.
pub const MOMENT_MATCH: f32 = 4.0;

/// The time constant, in seconds, of each of the two stages a light of the
/// moment's colour and reach go through ([`Moment`]), and of each of the two
/// its middle goes through: what kept every visual on the disc at 3 flashes
/// a second or fewer on a bright floor, by night and in a cellar, at 60 and
/// at 165 frames a second (`survey_spell_flashes`). The middle's is the one
/// that counts, a sparse cloud's moving a metre a frame: one stage of 0.06
/// left four visuals at 4 to 6 flashes a second, one of 0.15 one at 4 at 165
/// frames a second. A light that moves steadily (a bolt's, a projectile's)
/// is followed 0.2 s behind.
pub const MOMENT_TAU: f32 = 0.2;
pub const MOMENT_TAU_AT: f32 = 0.1;

/// A light of the moment fainter than this (its brightest channel) is gone.
const MOMENT_GONE: f32 = 1.0 / 512.0;

/// How bright a light of the moment may be ([`Moment`]): its brightest
/// channel as it is up to the knee (no one emitter's passes 0.9, no bolt's
/// 0.8, and a body's few emitters together come to 2 to 3.5), and past it
/// eased toward the most. A lightning's dozen bolt drawings summed to 7.75:
/// everything within 12 of it lit full, and held, it stayed so for most of a
/// second after its bolts went (the fade shows only once it comes down under
/// about 1), the target's body lit full to its edges while it played its hit
/// reaction. Eased to 2 the strongly lit area came to 57-64% of master's;
/// past 2 toward 4 a body's lights stay as they were.
pub const MOMENT_KNEE: f32 = 2.0;
pub const MOMENT_MOST: f32 = 4.0;

/// A cluster's colour with its brightest channel eased toward `MOMENT_MOST`
/// past `MOMENT_KNEE` (its hue kept).
fn eased(c: Vec3) -> Vec3 {
    let m = c.max_element();
    if m <= MOMENT_KNEE {
        return c;
    }
    let room = MOMENT_MOST - MOMENT_KNEE;
    c * ((MOMENT_KNEE + room * (1.0 - (-(m - MOMENT_KNEE) / room).exp())) / m)
}

/// One light of the moment held from frame to frame: where it is, its reach
/// and its colour, each through the two stages, and this frame's cluster.
#[derive(Clone, Copy, Debug)]
struct Track {
    key: u32,
    at: [Vec3; 2],
    range: [f32; 2],
    color: [Vec3; 2],
    target: Option<PointLight>,
}

/// The lights of the moment (`from_batches`, `from_strips`: what a frame's
/// spells, held items and fiery creatures draw) held from frame to frame, so
/// that none comes, goes or changes in a frame (petition 1321: "the lighting
/// on some spells rapidly flashes colors in a radius"). A frame's particles
/// come and go a few at a time, and each frame's lights out of them jumped
/// with them, many times a second: where they are (a burst of a few large
/// particles, its middle a metre off the next frame, under the floor the one
/// after), how bright and what colour (a spark born white in a blue cloud).
/// So those within `MOMENT_MERGE` of one another are one (their colours
/// summed and eased past `MOMENT_KNEE`, at their middle by strength, the
/// mean reach), each is followed
/// from frame to frame by where it is, its colour and reach eased toward the
/// frame's through two stages of `MOMENT_TAU` and its middle through two of
/// `MOMENT_TAU_AT`; a new one comes up from nothing, one whose particles are
/// gone goes down to nothing.
#[derive(Default)]
pub struct Moment {
    tracks: Vec<Track>,
    next: u32,
    last: Option<i64>,
}

impl Moment {
    /// How many lights it holds now, coming, going or lit.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// The frame at `now` (ms): `raw`, its lights of the moment, merged and
    /// followed, every one held stepped by the time since the last frame;
    /// those lit put into `out`, each with its own key (`Chooser`).
    pub fn frame(&mut self, raw: &[PointLight], now: i64, out: &mut Vec<(u64, PointLight)>) {
        let dt = self.last.map_or(0.0, |t| ((now - t) as f32 / 1000.0).clamp(0.0, 0.25));
        self.last = Some(now);
        // the frame's clusters, the strongest first
        let mut order: Vec<&PointLight> = raw.iter().filter(|l| l.range > 0.0 && l.color.max_element() > 0.0).collect();
        order.sort_by(|a, b| b.color.max_element().total_cmp(&a.color.max_element()));
        let mut clusters: Vec<(PointLight, f32)> = Vec::new();
        for l in order {
            let w = l.color.max_element();
            match clusters.iter_mut().find(|(c, _)| (c.at - l.at).length() < MOMENT_MERGE) {
                Some((c, cw)) => {
                    let all = *cw + w;
                    c.at = (c.at * *cw + l.at * w) / all;
                    c.range = (c.range * *cw + l.range * w) / all;
                    c.color += l.color;
                    *cw = all;
                }
                None => clusters.push((PointLight { fixed: false, ..*l }, w)),
            }
        }
        for (c, _) in &mut clusters {
            c.color = eased(c.color);
        }
        // each to the nearest held light not taken, within reach of it
        for t in &mut self.tracks {
            t.target = None;
        }
        for (c, _) in clusters {
            let near = self
                .tracks
                .iter()
                .enumerate()
                .filter(|(_, t)| t.target.is_none())
                .map(|(i, t)| (i, (t.at[0] - c.at).length()))
                .filter(|&(_, d)| d < MOMENT_MATCH)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            match near {
                Some((i, _)) => self.tracks[i].target = Some(c),
                None => {
                    self.next = self.next.wrapping_add(1);
                    self.tracks.push(Track { key: self.next, at: [c.at; 2], range: [c.range; 2], color: [Vec3::ZERO; 2], target: Some(c) });
                }
            }
        }
        // eased toward the frame's
        let (a, b) = (1.0 - (-dt / MOMENT_TAU).exp(), 1.0 - (-dt / MOMENT_TAU_AT).exp());
        for t in &mut self.tracks {
            let (at, range, color) = t.target.map_or((t.at[0], t.range[1], Vec3::ZERO), |c| (c.at, c.range, c.color));
            t.color[0] += (color - t.color[0]) * a;
            t.color[1] += (t.color[0] - t.color[1]) * a;
            t.range[0] += (range - t.range[0]) * a;
            t.range[1] += (t.range[0] - t.range[1]) * a;
            t.at[0] += (at - t.at[0]) * b;
            t.at[1] += (t.at[0] - t.at[1]) * b;
        }
        self.tracks.retain(|t| t.target.is_some() || t.color[0].max_element() > MOMENT_GONE || t.color[1].max_element() > MOMENT_GONE);
        for t in &self.tracks {
            if t.color[1].max_element() > MOMENT_GONE {
                out.push((MOMENT_KEY | t.key as u64, PointLight { at: t.at[1], range: t.range[1], color: t.color[1], fixed: false, key: MOMENT_KEY | t.key as u64 }));
            }
        }
    }
}

/// The keys of the lights of the moment (`Moment`), apart from the fixed
/// lights' (`Fixed::keyed`, their places in the list).
pub const MOMENT_KEY: u64 = 1 << 40;

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
    let chosen: Vec<PointLight> = ranked(candidates.iter().map(|&l| (0, l)), eye, view_proj).into_iter().take(budget()).map(|(_, l, share)| PointLight { color: l.color * share, ..l }).collect();
    render::Lights::new(&chosen)
}

/// How long, in seconds, a light takes to come into the frame's lights or
/// to go out of them when the budget changes hands ([`Chooser`]).
pub const BUDGET_FADE: f32 = 0.25;

/// The frame's choice of lights ([`choose`]) held from frame to frame: when
/// another light takes a light's place in the budget (a spell's comes, the
/// camera turns), the one leaving dims out over `BUDGET_FADE` and the one
/// coming brightens in over as long once its place is free, instead of each
/// going or coming in a frame; a light newly in view, which lit nothing in
/// view a frame ago, comes as it is when there is a place for it. The
/// budget is never passed: a light comes only into a free place.
#[derive(Default)]
pub struct Chooser {
    /// every light in view last frame by its key, and how much of it was drawn
    held: HashMap<u64, f32>,
    last: Option<i64>,
}

impl Chooser {
    /// The frame at `now` (ms): `candidates` (each with its own key)
    /// chosen as [`choose`] chooses them, each drawn as much as it was last
    /// frame, moved toward what `choose` would give it by a `BUDGET_FADE`'s
    /// share of the time since.
    pub fn choose(&mut self, candidates: &[(u64, PointLight)], eye: Vec3, view_proj: Mat4, now: i64) -> render::Lights {
        let dt = self.last.map_or(0.0, |t| ((now - t) as f32 / 1000.0).clamp(0.0, 0.25));
        self.last = Some(now);
        let step = dt / BUDGET_FADE;
        let ranked = ranked(candidates.iter().copied(), eye, view_proj);
        let most = budget();
        let mut drawn: HashMap<u64, f32> = HashMap::new();
        // those drawn last frame keep their places, eased toward their share
        for &(key, _, share) in &ranked {
            if let Some(&w) = self.held.get(&key).filter(|w| **w > 0.0) {
                drawn.insert(key, approach(w, share, step));
            }
        }
        // the rest by key into the places left: one in view last frame eased
        // in from nothing, one newly in view as it is
        let mut places = drawn.len();
        for &(key, _, share) in &ranked {
            if places >= most || share <= 0.0 || drawn.contains_key(&key) {
                continue;
            }
            let w = if self.held.contains_key(&key) { approach(0.0, share, step) } else { share };
            if w > 0.0 {
                drawn.insert(key, w);
                places += 1;
            }
        }
        self.held = ranked.iter().map(|&(key, _, _)| (key, drawn.get(&key).copied().unwrap_or(0.0))).collect();
        let lights: Vec<PointLight> = ranked
            .iter()
            .filter_map(|&(key, l, _)| drawn.get(&key).filter(|w| **w > 0.0).map(|w| PointLight { color: l.color * *w, ..l }))
            .collect();
        render::Lights::new(&lights)
    }
}

/// `x` moved toward `to` by at most `by`.
fn approach(x: f32, to: f32, by: f32) -> f32 {
    if x < to { (x + by).min(to) } else { (x - by).max(to) }
}

/// The candidates (each with its key) in view and within reach, the most
/// by radius over distance first, each with its share as [`choose`] gives
/// it: whole well inside the budget, dimmed toward the first left out, none
/// past it.
fn ranked(candidates: impl Iterator<Item = (u64, PointLight)>, eye: Vec3, view_proj: Mat4) -> Vec<(u64, PointLight, f32)> {
    let planes = crate::particles::frustum(view_proj);
    let mut keyed: Vec<(f32, u64, PointLight)> = candidates
        .filter(|(_, l)| l.range > 0.0 && (l.at - eye).length() < QUERY_RADIUS + l.range && planes.iter().all(|p| p.dot(l.at.extend(1.0)) >= -l.range))
        .map(|(key, l)| (l.range / (l.at - eye).length().max(0.01), key, l))
        .collect();
    keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
    let most = budget();
    let cut = keyed.get(most).map(|k| k.0);
    keyed
        .iter()
        .enumerate()
        .map(|(i, &(k, key, l))| {
            let share = match cut {
                _ if i >= most => 0.0,
                Some(c) => {
                    let x = ((k - c) / (0.25 * c)).clamp(0.0, 1.0);
                    x * x * (3.0 - 2.0 * x)
                }
                None => 1.0,
            };
            (key, l, share)
        })
        .collect()
}

/// Dynamic Lighting's lights from frame to frame (the window's): the lights
/// of the moment held ([`Moment`]) and the choice among them and the fixed
/// lights ([`Chooser`]).
#[derive(Default)]
pub struct Lighting {
    pub moment: Moment,
    pub chooser: Chooser,
}

impl Lighting {
    /// The frame's lights at `now`: `fixed` (`Fixed::keyed`), and the lights
    /// of the moment of `batches` (the spells', the held items', the
    /// creatures') and `strips` (the spells' bolts), for a camera at `eye`
    /// seeing through `view_proj`.
    pub fn frame(&mut self, mut fixed: Vec<(u64, PointLight)>, batches: &[Batch], strips: &[Strips], eye: Vec3, view_proj: Mat4, now: i64) -> render::Lights {
        let mut raw = Vec::new();
        from_batches(batches, &mut raw);
        from_strips(strips, &mut raw);
        self.moment.frame(&raw, now, &mut fixed);
        self.chooser.choose(&fixed, eye, view_proj, now)
    }
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
        PointLight { at, range, color: Vec3::ONE, fixed: true, key: 0 }
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

    // ---- How much a frame sequence flashes, as WCAG 2.2 counts it ----

    /// A displayed channel's light: sRGB's transfer undone (WCAG's relative
    /// luminance's).
    fn linear(v: f32) -> f32 {
        let v = v.clamp(0.0, 1.0);
        if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    }

    /// Where a series turns: each change of at least `by` from the last turn
    /// the other way (a transition) that `counts` lets count, given the two
    /// samples' places (the turn's and this one's); the frames they end on.
    fn transitions(series: &[f32], by: f32, counts: impl Fn(usize, usize) -> bool) -> Vec<usize> {
        let mut out = Vec::new();
        let (mut lo, mut hi, mut ext, mut trend) = (0, 0, 0, 0i32);
        for (i, &v) in series.iter().enumerate() {
            let turned = match trend {
                0 => {
                    if v < series[lo] {
                        lo = i;
                    }
                    if v > series[hi] {
                        hi = i;
                    }
                    if v - series[lo] >= by && counts(lo, i) {
                        Some(1)
                    } else if series[hi] - v >= by && counts(hi, i) {
                        Some(-1)
                    } else {
                        None
                    }
                }
                1 if v > series[ext] => {
                    ext = i;
                    None
                }
                1 => (series[ext] - v >= by && counts(ext, i)).then_some(-1),
                _ if v < series[ext] => {
                    ext = i;
                    None
                }
                _ => (v - series[ext] >= by && counts(ext, i)).then_some(1),
            };
            if let Some(t) = turned {
                (trend, ext) = (t, i);
                out.push(i);
            }
        }
        out
    }

    /// The most flashes (a flash is a pair of opposing transitions) in any
    /// `second` frames.
    fn most_a_second(turns: &[usize], second: usize) -> usize {
        let (mut most, mut j) = (0, 0);
        for i in 0..turns.len() {
            while turns[i] - turns[j] >= second {
                j += 1;
            }
            most = most.max(i - j + 1);
        }
        most / 2
    }

    /// What a run of frames' lights does to the fxview stage's floor (up, on
    /// a grid a unit apart, half a unit off the figures' feet) and a wall
    /// beside it (facing the two), each point's colour a texel of `texel`
    /// lit by the night's `base` and the lights as `lit.wgsl` lights it:
    /// the most general flashes a second at any point (WCAG 2.2: changes of
    /// a tenth of the relative luminance or more, the darker under 0.8),
    /// the most red flashes (changes of (R-G-B)·320 over 20 with a
    /// saturated red, R/(R+G+B) at least 0.8, at one end or the other), and
    /// how many points flash more than three times in a second.
    fn flashing(frames: &[Vec<PointLight>], base: Vec3, texel: f32, second: usize, every: usize) -> (usize, usize, usize) {
        let mut points: Vec<(Vec3, Vec3)> = Vec::new();
        for x in (-8..=8).step_by(every) {
            for z in (-6..=12).step_by(every) {
                points.push((Vec3::new(x as f32 + 0.5, 0.0, z as f32 + 0.5), Vec3::Y));
            }
        }
        for y in (0..=6).step_by(every) {
            for z in (-6..=12).step_by(every) {
                points.push((Vec3::new(3.0, y as f32 + 0.5, z as f32 + 0.5), -Vec3::X));
            }
        }
        let (mut general, mut red, mut over) = (0, 0, 0);
        for (p, n) in points {
            let (mut lum, mut red_of, mut sat) = (Vec::with_capacity(frames.len()), Vec::with_capacity(frames.len()), Vec::with_capacity(frames.len()));
            for lights in frames {
                let mut light = base;
                for l in lights {
                    let to = l.at - p;
                    let d = to.length();
                    if d < l.range && d > 0.0 {
                        light += l.color * (1.0 - d / l.range) * (to.dot(n) / d).max(0.0);
                    }
                }
                let c = light.clamp(Vec3::ZERO, Vec3::ONE) * texel;
                let (r, g, b) = (linear(c.x), linear(c.y), linear(c.z));
                lum.push(0.2126 * r + 0.7152 * g + 0.0722 * b);
                red_of.push(((r - g - b) * 320.0).max(0.0));
                sat.push(r + g + b > 0.0 && r / (r + g + b) >= 0.8);
            }
            let f = most_a_second(&transitions(&lum, 0.1, |a, b| lum[a].min(lum[b]) < 0.8), second);
            general = general.max(f);
            over += (f > 3) as usize;
            red = red.max(most_a_second(&transitions(&red_of, 20.0, |a, b| sat[a] || sat[b]), second));
        }
        (general, red, over)
    }

    /// The night's light on what faces up (`InitTimeOfDay`'s ambient at 22h,
    /// where nothing faces the sun): what a light of the moment adds to most.
    fn night() -> Vec3 {
        let (amb, _, _) = vi::tod::TimeOfDay::<Ee>::new().lights(F::new(22.0), F::ZERO);
        Vec3::new(amb[0].0, amb[1].0, amb[2].0)
    }

    /// A dark floor's light: a quarter of the night's (a cellar, a dungeon),
    /// where a red light's red is saturated (the night's grey-blue never lets
    /// it be).
    fn cellar() -> Vec3 {
        night() * 0.25
    }

    /// A camera that sees the stage: from in front of it and above.
    fn stage_view() -> (Vec3, Mat4) {
        let eye = Vec3::new(-9.0, 4.0, 3.0);
        let v = glam::camera::rh::view::look_at_mat4(eye, Vec3::new(0.0, 1.0, 3.0), Vec3::Y);
        (eye, glam::camera::rh::proj::directx::perspective(55f32.to_radians(), 16.0 / 9.0, 0.1, 800.0) * v)
    }

    /// newlib's `rand`'s step, for scripted particles.
    fn next(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
        (*seed >> 8) as f32 / (1u32 << 24) as f32
    }

    /// A spell's sparse cloud as the stage's caster shows it: four large
    /// additive particles a frame, born somewhere within 1.5 of its middle
    /// a unit up (under the floor some frames), white or blue by turns at
    /// random, as `ee9d396a`, `287654cc` and `d17b032f` draw them.
    fn sparse_cloud(seed: &mut u32, red: bool) -> Batch {
        let quads = (0..4)
            .map(|_| {
                let at = [next(seed) * 3.0 - 1.5, next(seed) * 3.0 - 0.5, next(seed) * 3.0 - 1.5];
                let white = next(seed) < 0.5;
                let color = if red { [1.0, 0.05, 0.05, next(seed)] } else if white { [1.0, 1.0, 1.0, 1.0] } else { [0.2, 0.3, 1.0, 1.0] };
                Quad { center: at, half: [0.8, 0.8], frame: 0, color }
            })
            .collect();
        Batch { texture: 0, blend: 1, ztest: true, zwrite: false, quads }
    }

    /// The frames' lights of the moment of `cloud`, a frame each 1000/`hz`
    /// ms for `secs`: master's way (each frame's own, `choose`) and the
    /// client's (`Lighting`).
    fn both_ways(hz: u32, secs: f32, mut cloud: impl FnMut(usize) -> Vec<Batch>) -> (Vec<Vec<PointLight>>, Vec<Vec<PointLight>>) {
        let (eye, vp) = stage_view();
        let mut lighting = Lighting::default();
        let (mut each, mut held) = (Vec::new(), Vec::new());
        for k in 0..(secs * hz as f32) as usize {
            let now = (k as u64 * 1000 / hz as u64) as i64;
            let batches = cloud(k);
            let mut found = Vec::new();
            from_batches(&batches, &mut found);
            each.push(choose(&found, eye, vp).as_slice().to_vec());
            held.push(lighting.frame(Vec::new(), &batches, &[], eye, vp, now).as_slice().to_vec());
        }
        (each, held)
    }

    /// The petition's flashing (petition 1321: "the lighting on some spells
    /// rapidly flashes colors in a radius"), as the disc's worst visuals
    /// make it: a sparse cloud's light, each frame's own, jumps where its
    /// few particles are, under the floor and out, white and blue, 4 to 14
    /// times a second over metres of floor and wall at night. Held from
    /// frame to frame (`Moment`), at 60 and at 165 frames a second, it
    /// flashes 3 times a second at most and lights the floor still; a red
    /// cloud's red flashes too. The control: each frame's own, as master
    /// gathered them, flashes more than 3 times a second.
    #[test]
    fn a_sparse_clouds_light_does_not_flash() {
        for hz in [60, 165] {
            let mut seed = 7u32;
            let (each, held) = both_ways(hz, 3.0, |_| vec![sparse_cloud(&mut seed, false)]);
            let (general, _, over) = flashing(&each, night(), 0.6, hz as usize, 1);
            assert!(general > 3 && over > 10, "the control at {hz} Hz: each frame's own light flashes, {general} a second at {over} points");
            let (general, red, over) = flashing(&held, night(), 0.6, hz as usize, 1);
            assert!(general <= 3 && red <= 3 && over == 0, "held at {hz} Hz: {general} flashes a second, {red} red, {over} points over 3");
            // still lit: what reaches the floor under the cloud, the last two seconds
            let under = |frames: &[Vec<PointLight>]| -> f32 {
                let p = Vec3::new(0.5, 0.0, 0.5);
                let tail = &frames[frames.len() / 3..];
                tail.iter().map(|ls| ls.iter().map(|l| { let to = l.at - p; let d = to.length(); if d < l.range && d > 0.0 { l.color.max_element() * (1.0 - d / l.range) * (to.y / d).max(0.0) } else { 0.0 } }).sum::<f32>()).sum::<f32>() / tail.len() as f32
            };
            assert!(under(&held) > 0.5 * under(&each), "held at {hz} Hz the floor is lit still: {} against {}", under(&held), under(&each));
            // a red cloud whose particles come and go
            let mut seed = 11u32;
            let (each, held) = both_ways(hz, 3.0, |k| if (k / (hz as usize / 20).max(1)) % 2 == 0 { vec![sparse_cloud(&mut seed, true)] } else { Vec::new() });
            let (_, red, _) = flashing(&each, cellar(), 0.6, hz as usize, 1);
            assert!(red > 3, "the control at {hz} Hz: a red cloud coming and going 10 times a second, {red} red flashes");
            let (general, red, _) = flashing(&held, cellar(), 0.6, hz as usize, 1);
            assert!(general <= 3 && red <= 3, "held at {hz} Hz: {general} flashes, {red} red");
        }
    }

    /// A steady light of the moment comes to be its own (a cloud that does
    /// not change looks as master drew it), from nothing over a quarter of a
    /// second or so, at any frame rate alike, and goes when its particles
    /// do, to nothing over a second or two, then is let go.
    #[test]
    fn a_light_of_the_moment_comes_and_goes_smoothly() {
        let l = PointLight { at: Vec3::new(1.0, 1.0, 2.0), range: 12.0, color: Vec3::new(0.2, 0.5, 0.9), fixed: false, key: 0 };
        let at = |hz: i64, secs: f32, on: f32| {
            let mut m = Moment::default();
            let mut out = Vec::new();
            for k in 0..=(secs * hz as f32) as i64 {
                out.clear();
                let now = k * 1000 / hz;
                m.frame(if (now as f32) < on * 1000.0 { std::slice::from_ref(&l) } else { &[] }, now, &mut out);
            }
            (out.first().map(|o| o.1), m.len())
        };
        let (first, _) = at(60, 0.0, 9.0);
        assert!(first.is_none_or(|f| f.color.max_element() < 0.01), "it comes up from nothing: {first:?}");
        for hz in [30, 60, 165] {
            let (quarter, _) = at(hz, 0.25, 9.0);
            let q = quarter.expect("coming").color.z / l.color.z;
            assert!((0.2..0.6).contains(&q), "a quarter of a second in at {hz} Hz: {q}");
            let (whole, _) = at(hz, 1.5, 9.0);
            let w = whole.expect("lit");
            assert!((w.color - l.color).abs().max_element() < 0.01 && (w.at - l.at).length() < 1e-3 && (w.range - l.range).abs() < 0.05, "its own at {hz} Hz: {w:?}");
            let (going, _) = at(hz, 1.8, 1.5);
            let g = going.expect("going").color.z / l.color.z;
            assert!((0.3..0.9).contains(&g), "0.3 s after its particles at {hz} Hz: {g}");
            let (gone, held) = at(hz, 4.0, 1.5);
            assert!(gone.is_none() && held == 0, "let go 2.5 s after at {hz} Hz: {gone:?}, {held} held");
        }
        // the control: a light whose particles stay is held as long
        assert_eq!(at(60, 4.0, 9.0).1, 1);
    }

    /// A body's lights of the moment within `MOMENT_MERGE` are one light (the
    /// colours summed, at their middle by strength), a target's 6 off
    /// another; each followed as it moves (a bolt's middle across the
    /// stage at 20 a second, its two stages' 0.2 s behind), not made anew.
    #[test]
    fn a_bodys_lights_are_one_and_followed() {
        let light = |x: f32, z: f32, c: Vec3| PointLight { at: Vec3::new(x, 1.0, z), range: 10.0, color: c, fixed: false, key: 0 };
        let mut m = Moment::default();
        let mut out = Vec::new();
        for k in 0..120 {
            out.clear();
            let x = k as f32 * 20.0 / 60.0;
            m.frame(&[light(0.0, 0.0, Vec3::X * 0.3), light(0.0, 1.0, Vec3::Y * 0.6), light(0.0, 6.0, Vec3::Z * 0.5), light(x, 3.0, Vec3::ONE * 0.4)], k * 1000 / 60, &mut out);
        }
        assert_eq!(out.len(), 3, "the caster's two as one, the target's, the bolt's: {out:?}");
        let caster = out.iter().find(|o| o.1.at.z < 1.0).expect("the caster's").1;
        assert!((caster.color - Vec3::new(0.3, 0.6, 0.0)).abs().max_element() < 0.01 && (caster.at.z - 2.0 / 3.0).abs() < 0.01, "{caster:?}");
        let bolt = out.iter().find(|o| (o.1.at.z - 3.0).abs() < 0.1).expect("the bolt's").1;
        let behind = 119.0 * 20.0 / 60.0 - bolt.at.x;
        assert!((behind - 20.0 * 2.0 * MOMENT_TAU_AT).abs() < 0.5 && (bolt.color - Vec3::ONE * 0.4).abs().max_element() < 0.01, "followed {behind} behind, its colour its own: {bolt:?}");
        assert!(m.len() == 3, "not made anew as it moves: {} held", m.len());
    }

    /// A cluster's brightness eased past `MOMENT_KNEE` toward `MOMENT_MOST`:
    /// one emitter's light (0.9 at most), one bolt drawing's (0.8) or a
    /// body's few together (2 at the knee) as it is; a lightning's dozen
    /// drawings summed (7.75, 0x58f4da33's) under 4, its hue kept; brighter
    /// always brighter. The control: summed as master summed them in the
    /// shader, 7.75.
    #[test]
    fn a_clusters_light_is_eased_not_blown_out() {
        let one = Vec3::new(0.9, 0.6, 0.1);
        assert_eq!(eased(one), one, "one emitter's light as it is");
        let body = Vec3::new(2.0, 1.4, 0.2);
        assert_eq!(eased(body), body, "a body's few at the knee as they are");
        let dozen = Vec3::new(0.8, 0.65, 0.0) * 9.7;
        let e = eased(dozen);
        assert!(dozen.max_element() > 7.7, "the control: summed, {dozen:?}");
        assert!(e.max_element() < MOMENT_MOST && e.max_element() > 3.8, "eased under the most: {e:?}");
        assert!((e.y / e.x - dozen.y / dozen.x).abs() < 1e-5 && e.z == 0.0, "its hue kept: {e:?}");
        let mut last = 0.0;
        for k in 1..120 {
            let m = eased(Vec3::splat(k as f32 * 0.1)).x;
            assert!(m > last, "brighter always brighter at {k}: {m} after {last}");
            last = m;
        }
        // a lightning's held through `Moment` comes no brighter than the most
        let mut m = Moment::default();
        let mut out = Vec::new();
        for k in 0..120 {
            out.clear();
            let draws: Vec<PointLight> = (0..12).map(|i| PointLight { at: Vec3::new(0.0, 1.0, 1.0 + i as f32 * 0.3), range: 15.0, color: Vec3::new(0.8, 0.65, 0.0), fixed: false, key: 0 }).collect();
            m.frame(&draws, k * 1000 / 60, &mut out);
        }
        assert!(out.len() == 1 && out[0].1.color.max_element() < MOMENT_MOST, "{out:?}");
    }

    /// The budget changing hands: 20 fires down the view, each a fifth
    /// farther than the last, and the most a frame takes; a spell's light
    /// (the nearest of all) comes up from nothing at 0.5 s, as a `Moment`
    /// brings one, and goes down to nothing at 1.5 s. Chosen frame by frame
    /// as master chose (`choose`), the last fire taken goes dark the frame
    /// the spell's comes and lights again the frame it goes (the control);
    /// through the `Chooser` no fire's drawn colour changes by more than a
    /// `BUDGET_FADE`'s share of a frame, the spell's comes once the fire's
    /// place is free, the budget is never passed, and with nothing coming
    /// or going the choice is `choose`'s exactly.
    #[test]
    fn the_budget_changes_hands_gradually() {
        let (eye, vp) = (Vec3::ZERO, camera());
        let fire = Vec3::new(0.5, 0.34, 0.0);
        let fires: Vec<(u64, PointLight)> = (0..20).map(|k| (k, PointLight { at: Vec3::new(0.0, 0.0, -5.0 * 1.2f32.powi(k as i32)), range: 18.0, color: fire, fixed: true, key: k })).collect();
        let spell = |k: i64| {
            let up = ((k - 30) as f32 / 20.0).clamp(0.0, 1.0) * ((90 - k) as f32 / 20.0).clamp(0.0, 1.0);
            (MOMENT_KEY | 1, PointLight { at: Vec3::new(0.0, 0.0, -3.0), range: 12.0, color: Vec3::new(0.2, 0.4, 0.9) * up, fixed: false, key: MOMENT_KEY | 1 })
        };
        let lit = |ls: &[PointLight], at: Vec3| ls.iter().find(|l| l.at == at).map_or(Vec3::ZERO, |l| l.color);
        let mut chooser = Chooser::default();
        let (mut last_each, mut last_held): (Vec<PointLight>, Vec<PointLight>) = (Vec::new(), Vec::new());
        let (mut jump_each, mut jump_held, mut spell_drawn) = (0.0f32, 0.0f32, 0.0f32);
        for k in 0..150i64 {
            let now = k * 1000 / 60;
            let mut all = fires.clone();
            if (30..90).contains(&k) {
                all.push(spell(k));
            }
            let plain: Vec<PointLight> = all.iter().map(|a| a.1).collect();
            let each = choose(&plain, eye, vp).as_slice().to_vec();
            let held = chooser.choose(&all, eye, vp, now).as_slice().to_vec();
            assert!(held.len() <= budget(), "the budget passed at frame {k}: {}", held.len());
            if k < 30 {
                let shown: Vec<PointLight> = each.iter().copied().filter(|l| l.color.max_element() > 0.0).collect();
                assert_eq!(held, shown, "with nothing coming or going, choose's own at frame {k}");
            }
            if k > 0 {
                for (_, f) in &fires {
                    jump_each = jump_each.max((lit(&each, f.at) - lit(&last_each, f.at)).abs().max_element());
                    jump_held = jump_held.max((lit(&held, f.at) - lit(&last_held, f.at)).abs().max_element());
                }
            }
            spell_drawn = spell_drawn.max(lit(&held, Vec3::new(0.0, 0.0, -3.0)).z);
            (last_each, last_held) = (each, held);
        }
        assert!(jump_each > 0.3, "the control: the last fire taken jumps {jump_each} in a frame");
        // a frame is 16 or 17 ms (`now` in whole ms)
        assert!(jump_held <= fire.x * 0.017 / BUDGET_FADE * 1.01, "held: no fire moves more than its fade's step a frame, {jump_held}");
        assert!(spell_drawn > 0.5, "the spell's light comes once its place is free: {spell_drawn}");
    }

    /// The fxview stage off the disc (`None` without run/eqoa.iso): a human
    /// casting at another 6 off, as the GM panel's preview stands them.
    fn stage() -> Option<crate::fxview::Stage> {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let mut disc = iso::Iso::open(&iso).ok()?;
        let mut stage = crate::fxview::Stage::new(crate::fxview::library_off(&mut disc).ok()?).ok()?;
        stage.set_aim(crate::fxview::Aim::Human).ok()?;
        Some(stage)
    }

    /// One visual cast on the stage (the stage's clock at `clock`, moved on),
    /// a frame each `step` ms for `secs`: its lights of the moment each
    /// frame, master's way (each frame's own, `choose`) and the client's
    /// (`Lighting`, as the window's frames are lit).
    fn cast(stage: &mut crate::fxview::Stage, effect: u32, clock: &mut i64, step: i64, secs: f32) -> (Vec<Vec<PointLight>>, Vec<Vec<PointLight>>) {
        *clock += step;
        stage.step(*clock);
        assert!(stage.set_visual(effect, 2000));
        let mut lighting = Lighting::default();
        let (mut each, mut held) = (Vec::new(), Vec::new());
        for _ in 0..(secs * 1000.0 / step as f32) as usize {
            *clock += step;
            stage.step(*clock);
            let (view, _, _) = stage.view(16.0 / 9.0);
            let mut found = Vec::new();
            from_batches(stage.batches(), &mut found);
            from_strips(stage.strips(), &mut found);
            each.push(choose(&found, view.eye, view.view_proj).as_slice().to_vec());
            held.push(lighting.frame(Vec::new(), stage.batches(), stage.strips(), view.eye, view.view_proj, *clock).as_slice().to_vec());
        }
        (each, held)
    }

    /// The visuals master's lights flashed most (`survey_spell_flashes`):
    /// what a debug build plays (a release build plays every one).
    const WORST: [u32; 8] = [0x2876_54CC, 0xD17B_032F, 0x0290_C733, 0xFABE_2193, 0xEF8D_60A7, 0xCC78_9A46, 0x1AF4_AF0C, 0x58F4_DA33];

    /// Every spell visual on the disc (SKIPs without run/eqoa.iso), cast at a
    /// human at night, a frame each 16 ms for 6 s, its lights held from frame
    /// to frame as the window's are: none flashes more than 3 times in any
    /// second at any point of the stage's floor and wall (a texel of 0.9, a
    /// bright floor's), nor flashes red. The control: each frame's own
    /// lights, as master gathered them, flash more than 3 times a second for
    /// some (0x287654cc's, the worst, 18 times at 212 points). A debug build
    /// plays only `WORST` (every visual takes a release build a minute).
    #[test]
    fn no_spell_visual_flashes() {
        let Some(mut stage) = stage() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let list: Vec<u32> = if cfg!(debug_assertions) { WORST.to_vec() } else { stage.visual_list() };
        let (mut clock, mut flashed) = (0, Vec::new());
        stage.step(0);
        for e in list {
            let (each, held) = cast(&mut stage, e, &mut clock, 16, 6.0);
            let (general, red, over) = flashing(&held, night(), 0.9, 60, 1);
            assert!(general <= 3 && red <= 3, "visual {e:#010x}: {general} flashes a second ({over} points over 3), {red} red");
            let (general, _, over) = flashing(&each, night(), 0.9, 60, 1);
            if general > 3 {
                flashed.push((e, general, over));
            }
        }
        assert!(!flashed.is_empty(), "the control: some visual's frame-by-frame lights flash");
        eprintln!("frame by frame, {} visuals flashed more than 3 times a second: {flashed:x?}", flashed.len());
    }

    /// The survey the petition's fix was measured by (`--ignored`): every
    /// visual's flashes a second at night, each frame's own lights (master's
    /// way) and held (the client's); `SURVEY_TEXEL` (0.9), `SURVEY_BASE` (the
    /// night's light times this, 1: 0.25 a cellar), `SURVEY_STEP` (ms a frame,
    /// 16; 6 is 165 a second), `SURVEY_ONLY` (a visual's place: its lights
    /// frame by frame).
    #[test]
    #[ignore]
    fn survey_spell_flashes() {
        let Some(mut stage) = stage() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let env = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let (texel, step) = (env("SURVEY_TEXEL", 0.9), env("SURVEY_STEP", 16.0) as i64);
        let base = night() * env("SURVEY_BASE", 1.0);
        let only = std::env::var("SURVEY_ONLY").ok().and_then(|v| v.parse::<usize>().ok());
        let (mut clock, mut rows) = (0, Vec::new());
        stage.step(0);
        for (i, e) in stage.visual_list().into_iter().enumerate() {
            if only.is_some_and(|o| o != i) {
                continue;
            }
            let (each, held) = cast(&mut stage, e, &mut clock, step, 6.0);
            if only.is_some() {
                let show = |ls: &[PointLight]| ls.iter().map(|l| format!("[{:.1},{:.1},{:.1} r{:.0} {:.2},{:.2},{:.2}]", l.at.x, l.at.y, l.at.z, l.range, l.color.x, l.color.y, l.color.z)).collect::<Vec<_>>().join(" ");
                for (k, (a, b)) in each.iter().zip(&held).enumerate() {
                    eprintln!("{:5} ms each {} | held {}", k as i64 * step, show(a), show(b));
                }
            }
            let second = (1000 / step) as usize;
            rows.push((i, e, flashing(&each, base, texel, second, 1), flashing(&held, base, texel, second, 1)));
        }
        let over = |f: &dyn Fn(&(usize, u32, (usize, usize, usize), (usize, usize, usize))) -> (usize, usize, usize)| rows.iter().filter(|r| f(r).0 > 3 || f(r).1 > 3).count();
        eprintln!("{} visuals over 3 flashes a second: frame by frame {}, held {}", rows.len(), over(&|r| r.2), over(&|r| r.3));
        rows.sort_by(|a, b| (b.2.0, b.2.2).cmp(&(a.2.0, a.2.2)));
        for (i, e, a, b) in rows.iter().take(30) {
            eprintln!("#{i:3} {e:#010x}: frame by frame {:2}/s ({:3} points over 3, red {}) | held {:2}/s ({:3} over 3, red {})", a.0, a.2, a.1, b.0, b.2, b.1);
        }
        let worst = |f: &dyn Fn(&(usize, u32, (usize, usize, usize), (usize, usize, usize))) -> usize| rows.iter().map(f).max().unwrap_or(0);
        eprintln!("worst: frame by frame {}/s, red {}/s; held {}/s, red {}/s", worst(&|r| r.2.0), worst(&|r| r.2.1), worst(&|r| r.3.0), worst(&|r| r.3.1));
        let mut by_red = rows.clone();
        by_red.sort_by(|a, b| b.2.1.max(b.3.1).cmp(&a.2.1.max(a.3.1)));
        for (i, e, a, b) in by_red.iter().take(6) {
            eprintln!("red #{i:3} {e:#010x}: frame by frame {}/s | held {}/s", a.1, b.1);
        }
    }
}
