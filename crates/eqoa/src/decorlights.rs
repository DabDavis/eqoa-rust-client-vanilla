//! A server's decoration lights, as this client draws them (go-eqoa-server's
//! own; the server owner, 2026-11-16: Freeport's Christmas tree, "needs lights
//! on trees in different colors. the lights can be a template so we can put
//! christmas lights on fences buildings etc"; `net::decor` has the message, the
//! server's `entity/decorlights.go` the tables).
//!
//! The server sends strings of bulbs, each hung on every NPC of a name or placed
//! in a world, laid out by a template of its own (colours, spacing, how they
//! light). Each bulb is drawn as a glow, additive, its colour the string's
//! colour for it times how lit it is now ([`net::decor::lit`]: steady, a
//! twinkle, alternating, a chase), a soft round flare off SPELLFX.CSF
//! ([`BULB`]); a string's top bulb (a tree's star) as a sparkle ([`STAR`]). One
//! batch a colour a string, drawn with the particles after the world's: so
//! Dynamic Lighting takes each as a light of the moment (`lights::from_batches`)
//! and the tree casts its colours on the ground round it at night.
//!
//! The bulbs hang on a cord (the owner: "for the lights let there look like a
//! cord connecting them"): a thin wire in the string's cord colour from bulb to
//! bulb in order, hanging a little between each two ([`cords`]), a ribbon turned
//! to the eye drawn opaque with the world (`render::billboard::Strips`), lit as
//! the ground is by the sun and the ambient ([`light_cords`]), never a light of
//! its own. A ring's first bulb and a box's edge apart from the last break it
//! (`net::decor::BREAK`); a run that comes back round to its first bulb (a
//! ring) is closed. A string of single points has none.
//!
//! The art a decoration's NPC wears may be a sprite of a zone the player is not
//! near (Freeport's tree is a fir of Tunaria's north): the server names the
//! zone, and the walk's streamer reads it ([`art_zones`],
//! `stream::Streamer::want`), as it would were the player walking there, so
//! the sprite is in the library when the NPC is built.
//!
//! A server that sends none, and a PS2, draw nothing of it.

use net::decor::{Anchor, Decor};
use render::billboard::{Batch, Quad, Strips};
use std::cell::RefCell;

/// The bulb's glow: SPELLFX.CSF's soft round flare, white at its heart.
pub const BULB: i32 = 0x0BE1_EF02;
/// A string's top bulb (a tree's star): SPELLFX.CSF's six-pointed sparkle.
pub const STAR: i32 = 0xEDC2_9ADDu32 as i32;

pub use net::decor::TOP;

/// How far from the eye a bulb is drawn.
pub const REACH: f32 = 300.0;

thread_local! {
    static DECOR: RefCell<Decor> = RefCell::default();
}

/// The set the server sent (`net::session::OP_DECOR_LIGHTS`), the last one
/// standing.
pub fn set(d: Decor) {
    if std::env::var_os("EQOA_DECOR_TRACE").is_some() {
        eprintln!("decor: {} strings, art {:?}", d.strings.len(), d.art);
    }
    DECOR.with_borrow_mut(|x| *x = d);
}

/// Whether any string is lit.
pub fn any() -> bool {
    DECOR.with_borrow(|d| !d.strings.is_empty())
}

/// The zones of world `world` whose art a decoration wears: for the walk's
/// streamer to read.
pub fn art_zones(world: i64) -> Vec<usize> {
    DECOR.with_borrow(|d| d.art.iter().filter(|a| a.world as i64 == world).map(|a| a.zone as usize).collect())
}

/// How much lower a decoration's art is drawn than its NPC stands, in the
/// art's own units, if `model` is one (a fir's bare stem, so its boughs rest
/// on the ground): `play`'s puppets are built lowered by it.
pub fn sink(model: u32) -> Option<f32> {
    DECOR.with_borrow(|d| d.art.iter().find(|a| a.model == model && a.sink != 0.0).map(|a| a.sink))
}

/// `model` lowered by `sink(id)`, if the art is a decoration's that is.
pub fn lower(id: u32, model: &mut esf::Model) {
    if std::env::var_os("EQOA_DECOR_TRACE").is_some() {
        eprintln!("decor: build {id:#010x}: sink {:?}, {} verts", sink(id), model.pos.len());
    }
    if let Some(s) = sink(id) {
        for p in &mut model.pos {
            p[1] -= s;
        }
    }
}

/// An NPC a string may hang on: its name as its channel carries it, where it
/// is drawn, its heading and its scale.
pub struct Holder<'a> {
    pub name: &'a str,
    pub at: [f32; 3],
    pub heading: f32,
    pub scale: f32,
}

/// The bulbs of every string, as particle batches: those on the NPCs in
/// `holders`, and those placed in world `world`, within [`REACH`] of `eye`, lit
/// at `ms` (the game's time).
pub fn batches(holders: &[Holder], world: i64, eye: [f32; 3], ms: i64) -> Vec<Batch> {
    DECOR.with_borrow(|d| batches_of(d, holders, world, eye, ms))
}

fn batches_of(d: &Decor, holders: &[Holder], world: i64, eye: [f32; 3], ms: i64) -> Vec<Batch> {
    let t = ms as f64 / 1000.0;
    let mut out: Vec<Batch> = Vec::new();
    // each batch's colour key (usize::MAX the star's), beside it
    let mut keys: Vec<usize> = Vec::new();
    let near = |p: [f32; 3]| {
        let (dx, dy, dz) = (p[0] - eye[0], p[1] - eye[1], p[2] - eye[2]);
        dx * dx + dy * dy + dz * dz < REACH * REACH
    };
    for s in &d.strings {
        // where it hangs: each NPC of its name's frame, or the world's own
        let frames: Vec<([f32; 3], f32, f32)> = match &s.anchor {
            Anchor::Npc(key) => holders.iter().filter(|h| net::npctint::key(h.name) == *key).map(|h| (h.at, h.heading, h.scale)).collect(),
            Anchor::World(w) if *w as i64 == world => vec![([0.0; 3], 0.0, 1.0)],
            Anchor::World(_) => Vec::new(),
        };
        if frames.is_empty() {
            continue;
        }
        let first = out.len();
        for (at, heading, scale) in frames {
            if matches!(s.anchor, Anchor::Npc(_)) && !near(at) {
                continue;
            }
            let (sin, cos) = heading.sin_cos();
            for b in &s.bulbs {
                // the NPC's frame as a zone actor's (`esf::zone::Actor::matrix`)
                let [x, y, z] = b.at;
                let p = [at[0] + scale * (z * sin + x * cos), at[1] + scale * y, at[2] + scale * (z * cos - x * sin)];
                if matches!(s.anchor, Anchor::World(_)) && !near(p) {
                    continue;
                }
                let top = b.color & TOP != 0;
                let ci = (b.color & net::decor::COLOR) as usize;
                let c = s.colors.get(ci).copied().unwrap_or(0xFFFF_FFFF);
                let rgb = [(c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8];
                let lit = if top { 0.85 + 0.15 * net::decor::lit(net::decor::Effect::Twinkle, 3.0, 0, t) } else { net::decor::lit(s.effect, s.period, b.phase, t) };
                let k = (lit * s.bright).clamp(0.0, 2.0);
                let half = 0.5 * s.size * scale.max(0.1) * if top { 4.0 } else { 1.0 } * (0.85 + 0.15 * lit);
                let color = [rgb[0] as f32 / 255.0 * k, rgb[1] as f32 / 255.0 * k, rgb[2] as f32 / 255.0 * k, 1.0];
                let texture = if top { STAR } else { BULB };
                // a batch a colour (and the star its own), so Dynamic Lighting
                // casts each colour from where its bulbs are
                let key = if top { usize::MAX } else { ci };
                let i = match keys[first..].iter().position(|&k| k == key) {
                    Some(i) => first + i,
                    None => {
                        out.push(Batch { texture, blend: 1, ztest: true, zwrite: false, quads: Vec::new() });
                        keys.push(key);
                        out.len() - 1
                    }
                };
                out[i].quads.push(Quad { center: p, half: [half, half], frame: 0, color });
            }
        }
    }
    out
}

/// How many pieces a cord's span between two bulbs is drawn in (its sag).
pub const CORD_PIECES: usize = 4;

/// The cords of every string with one, as world-space strips: the same strings
/// and frames as [`batches`], each run of bulbs (to a break) one strip, a
/// ribbon `cord_width` across turned to `eye`, sagging `cord_sag` of each span.
/// Unlit: [`light_cords`] lights them.
pub fn cords(holders: &[Holder], world: i64, eye: [f32; 3]) -> Vec<Strips> {
    DECOR.with_borrow(|d| cords_of(d, holders, world, eye))
}

fn cords_of(d: &Decor, holders: &[Holder], world: i64, eye: [f32; 3]) -> Vec<Strips> {
    use glam::Vec3;
    let e = Vec3::from(eye);
    let mut out = Vec::new();
    for s in &d.strings {
        if s.cord & 0xFF == 0 || s.cord_width <= 0.0 {
            continue;
        }
        let frames: Vec<([f32; 3], f32, f32)> = match &s.anchor {
            Anchor::Npc(key) => holders.iter().filter(|h| net::npctint::key(h.name) == *key).map(|h| (h.at, h.heading, h.scale)).collect(),
            Anchor::World(w) if *w as i64 == world => vec![([0.0; 3], 0.0, 1.0)],
            Anchor::World(_) => Vec::new(),
        };
        let color = [(s.cord >> 24) as f32 / 255.0, ((s.cord >> 16) & 0xFF) as f32 / 255.0, ((s.cord >> 8) & 0xFF) as f32 / 255.0, 1.0];
        let mut strips = Vec::new();
        for (at, heading, scale) in frames {
            let (sin, cos) = heading.sin_cos();
            let place = |b: &net::decor::Bulb| {
                let [x, y, z] = b.at;
                Vec3::new(at[0] + scale * (z * sin + x * cos), at[1] + scale * y, at[2] + scale * (z * cos - x * sin))
            };
            // the runs: bulbs in order, broken where a bulb says, the star never on one
            let mut runs: Vec<Vec<Vec3>> = Vec::new();
            for b in s.bulbs.iter().filter(|b| b.color & TOP == 0) {
                let p = place(b);
                match runs.last_mut() {
                    Some(r) if b.color & net::decor::BREAK == 0 => r.push(p),
                    _ => runs.push(vec![p]),
                }
            }
            let half = 0.5 * s.cord_width * scale.max(0.1);
            for mut run in runs {
                if run.len() < 2 || (run[0] - e).length() > REACH {
                    continue;
                }
                // a ring that comes back round: closed
                if run.len() >= 3 {
                    let step = (run[1] - run[0]).length();
                    if (run[run.len() - 1] - run[0]).length() < 1.6 * step {
                        run.push(run[0]);
                    }
                }
                // the wire: each span in CORD_PIECES, hanging cord_sag of its length at its middle
                let mut line = vec![run[0]];
                for w in run.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    let sag = s.cord_sag * (b - a).length();
                    for k in 1..=CORD_PIECES {
                        let f = k as f32 / CORD_PIECES as f32;
                        line.push(a.lerp(b, f) - Vec3::Y * (sag * 4.0 * f * (1.0 - f)));
                    }
                }
                // a ribbon turned to the eye
                let mut strip = Vec::with_capacity(line.len() * 2);
                for (i, &p) in line.iter().enumerate() {
                    let dir = if i + 1 < line.len() { line[i + 1] - p } else { p - line[i - 1] };
                    let side = dir.cross(e - p).normalize_or_zero() * half;
                    strip.push(([0.0, 0.0], (p - side).to_array()));
                    strip.push(([0.0, 1.0], (p + side).to_array()));
                }
                strips.push(strip);
            }
        }
        if !strips.is_empty() {
            out.push(Strips { texture: 0, blend: 0, ztest: true, zwrite: true, color, strips, colors: Vec::new() });
        }
    }
    out
}

/// The cords lit as the world's own matte geometry is: their colour times the
/// ambient and the sun on a wire facing up, never brighter than it, never a
/// glow (a bolt's light, `spells::light_bolts`, adds the light to its colour).
pub fn light_cords(strips: &mut [Strips], view: &render::View) {
    matte(strips, view.ambient, view.sun, view.sun_dir);
}

fn matte(strips: &mut [Strips], ambient: glam::Vec3, sun: glam::Vec3, sun_dir: glam::Vec3) {
    let light = (ambient + sun * (-glam::Vec3::Y.dot(sun_dir)).max(0.0)).clamp(glam::Vec3::ZERO, glam::Vec3::ONE);
    for s in strips {
        let c = s.color;
        s.color = [c[0] * light.x, c[1] * light.y, c[2] * light.z, c[3]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::decor::{Bulb, Effect, LightString};

    fn tree(effect: Effect) -> Decor {
        Decor {
            strings: vec![LightString {
                anchor: Anchor::Npc("christmas tree".into()),
                effect,
                period: 1.5,
                size: 0.4,
                bright: 1.0,
                cord: 0x1E3F_1EFF,
                cord_width: 0.1,
                cord_sag: 0.06,
                colors: vec![0xFF20_20FF, 0x20D0_40FF, 0xFFD0_40FF],
                bulbs: vec![
                    Bulb { at: [1.0, 2.0, 0.0], color: 0, phase: 0 },
                    Bulb { at: [0.0, 4.0, 1.0], color: 1, phase: 64 },
                    Bulb { at: [-1.0, 6.0, 0.0], color: 0, phase: 128 },
                    Bulb { at: [0.0, 9.0, 0.0], color: 2 | TOP, phase: 0 },
                ],
            }],
            art: Vec::new(),
        }
    }

    /// Hung on the tree's frame: placed by its place, heading and scale, a
    /// batch a colour and the star its own, additive, none for another name or
    /// out of reach.
    #[test]
    fn the_bulbs_hang_on_the_tree() {
        let d = tree(Effect::Steady);
        let h = [Holder { name: "Christmas Tree", at: [100.0, 50.0, 200.0], heading: 0.0, scale: 2.0 }];
        let b = batches_of(&d, &h, 0, [100.0, 55.0, 190.0], 0);
        let quads: usize = b.iter().map(|b| b.quads.len()).sum();
        assert_eq!(quads, 4);
        assert!(b.iter().all(|b| b.blend == 1 && !b.zwrite && b.ztest));
        let mut centres: Vec<[f32; 3]> = b.iter().flat_map(|b| b.quads.iter().map(|q| q.center)).collect();
        centres.sort_by(|a, b| a[1].total_cmp(&b[1]));
        assert_eq!(centres[0], [102.0, 54.0, 200.0], "x 1 at scale 2, y 2 up");
        assert_eq!(centres[3], [100.0, 68.0, 200.0], "the star at the top");
        let star: Vec<&Batch> = b.iter().filter(|b| b.texture == STAR).collect();
        assert_eq!(star.len(), 1);
        assert!(star[0].quads[0].half[0] > b.iter().find(|b| b.texture == BULB).unwrap().quads[0].half[0], "the star is the larger");
        // the red bulbs share a batch, the green its own
        let red = b.iter().find(|b| b.texture == BULB && b.quads[0].color[0] > 0.9).unwrap();
        assert_eq!(red.quads.len(), 2);
        // turned half round: x 1 goes to x -1
        let h2 = [Holder { name: "christmas tree", at: [0.0; 3], heading: std::f32::consts::PI, scale: 1.0 }];
        let b2 = batches_of(&d, &h2, 0, [0.0; 3], 0);
        let low = b2.iter().flat_map(|b| b.quads.iter()).find(|q| (q.center[1] - 2.0).abs() < 1e-4).unwrap();
        assert!((low.center[0] + 1.0).abs() < 1e-4, "{:?}", low.center);
        // another name, or past the reach: nothing
        assert!(batches_of(&d, &[Holder { name: "Christmas Bush", at: [0.0; 3], heading: 0.0, scale: 1.0 }], 0, [0.0; 3], 0).is_empty());
        assert!(batches_of(&d, &h, 0, [100.0 + REACH + 10.0, 50.0, 200.0], 0).is_empty());
    }

    /// The cord runs bulb to bulb, the star off it, a break starting a new
    /// run, each span sagging below the straight line between its bulbs; a
    /// string with no cord has none.
    #[test]
    fn the_cord_hangs_from_bulb_to_bulb() {
        let mut d = tree(Effect::Steady);
        let h = [Holder { name: "christmas tree", at: [0.0; 3], heading: 0.0, scale: 1.0 }];
        let c = cords_of(&d, &h, 0, [0.0, 3.0, -20.0]);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].strips.len(), 1, "three bulbs, the star off it: one run");
        let s = &c[0].strips[0];
        assert_eq!(s.len(), 2 * (1 + 2 * CORD_PIECES), "two spans of {CORD_PIECES} pieces, two sides a point");
        // the middle of the first span hangs below the straight line
        let mid = |i: usize| (glam::Vec3::from(s[2 * i].1) + glam::Vec3::from(s[2 * i + 1].1)) / 2.0;
        let straight = (glam::Vec3::new(1.0, 2.0, 0.0) + glam::Vec3::new(0.0, 4.0, 1.0)) / 2.0;
        assert!(mid(CORD_PIECES / 2).y < straight.y - 0.01, "{:?} vs {straight:?}", mid(CORD_PIECES / 2));
        assert!((mid(0) - glam::Vec3::new(1.0, 2.0, 0.0)).length() < 1e-4, "it starts at the first bulb");
        // a break: two runs, the second too short for a wire
        d.strings[0].bulbs[1].color |= net::decor::BREAK;
        assert_eq!(cords_of(&d, &h, 0, [0.0; 3])[0].strips.len(), 1, "a run of one bulb has no wire");
        d.strings[0].bulbs[2].color |= net::decor::BREAK;
        assert!(cords_of(&d, &h, 0, [0.0; 3]).is_empty(), "every bulb alone: no wire");
        // no cord
        let mut none = tree(Effect::Steady);
        none.strings[0].cord = 0;
        assert!(cords_of(&none, &h, 0, [0.0; 3]).is_empty());
        // lit: darker than its colour at night, never brighter
        let mut lit = cords_of(&tree(Effect::Steady), &h, 0, [0.0; 3]);
        matte(&mut lit, glam::Vec3::splat(0.3), glam::Vec3::ZERO, glam::Vec3::NEG_Y);
        assert!(lit[0].color[1] < 0x3F as f32 / 255.0 && lit[0].color[1] > 0.0);
    }

    /// A twinkle changes each bulb's light over time, out of step.
    #[test]
    fn the_bulbs_twinkle() {
        let d = tree(Effect::Twinkle);
        let h = [Holder { name: "christmas tree", at: [0.0; 3], heading: 0.0, scale: 1.0 }];
        let light = |ms| -> Vec<f32> {
            let mut v: Vec<(f32, f32)> = batches_of(&d, &h, 0, [0.0; 3], ms).iter().filter(|b| b.texture == BULB).flat_map(|b| b.quads.iter().map(|q| (q.center[1], q.color[0] + q.color[1] + q.color[2]))).collect();
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            v.into_iter().map(|x| x.1).collect()
        };
        let (a, b) = (light(300), light(700));
        assert_ne!(a, b, "lit differently 0.4 s on");
        assert_ne!(a[0], a[2], "two red bulbs out of step");
    }
}
