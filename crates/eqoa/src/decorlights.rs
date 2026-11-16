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
//! The art a decoration's NPC wears may be a sprite of a zone the player is not
//! near (Freeport's tree is a fir of Tunaria's north): the server names the
//! zone, and the walk's streamer reads it ([`art_zones`],
//! `stream::Streamer::want`), as it would were the player walking there, so
//! the sprite is in the library when the NPC is built.
//!
//! A server that sends none, and a PS2, draw nothing of it.

use net::decor::{Anchor, Decor};
use render::billboard::{Batch, Quad};
use std::cell::RefCell;

/// The bulb's glow: SPELLFX.CSF's soft round flare, white at its heart.
pub const BULB: i32 = 0x0BE1_EF02;
/// A string's top bulb (a tree's star): SPELLFX.CSF's six-pointed sparkle.
pub const STAR: i32 = 0xEDC2_9ADDu32 as i32;

/// The bit of a bulb's colour byte that marks the string's top (its star).
pub const TOP: u8 = 0x80;

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
                let ci = (b.color & !TOP) as usize;
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
