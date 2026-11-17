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
    DECOR.with_borrow_mut(|x| *x = d);
}

/// Whether any string is lit.
pub fn any() -> bool {
    DECOR.with_borrow(|d| !d.strings.is_empty() || !d.presents.is_empty())
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

/// Whether `model` is a decoration's art (`net::decor::Art`): scenery the
/// server places, which the client never moves.
pub fn is_art(model: u32) -> bool {
    model != 0 && DECOR.with_borrow(|d| d.art.iter().any(|a| a.model == model))
}

/// `model` lowered by `sink(id)`, if the art is a decoration's that is.
pub fn lower(id: u32, model: &mut esf::Model) {
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
        for strip in &mut s.colors {
            for c in strip {
                *c = [c[0] * light.x, c[1] * light.y, c[2] * light.z, c[3]];
            }
        }
    }
}

/// The presents of every set (`net::decor::Presents`) on the NPCs in
/// `holders` and in world `world`, within [`REACH`] of `eye`: each box its
/// paper's five faces (the bottom on the ground unseen), shaded by the face's
/// facing, its ribbon a cross of bands over the top and down the sides, and a
/// bow of two crossed loops on top; opaque world-space strips, a colour a
/// vertex, unlit (lit as the cords are: [`light_cords`]).
pub fn presents(holders: &[Holder], world: i64, eye: [f32; 3]) -> Vec<Strips> {
    DECOR.with_borrow(|d| presents_of(d, holders, world, eye))
}

fn presents_of(d: &Decor, holders: &[Holder], world: i64, eye: [f32; 3]) -> Vec<Strips> {
    use glam::Vec3;
    let e = Vec3::from(eye);
    let mut out = Vec::new();
    for set in &d.presents {
        let frames: Vec<([f32; 3], f32, f32)> = match &set.anchor {
            Anchor::Npc(key) => holders.iter().filter(|h| net::npctint::key(h.name) == *key).map(|h| (h.at, h.heading, h.scale)).collect(),
            Anchor::World(w) if *w as i64 == world => vec![([0.0; 3], 0.0, 1.0)],
            Anchor::World(_) => Vec::new(),
        };
        let mut strips: Vec<Vec<([f32; 2], [f32; 3])>> = Vec::new();
        let mut colors: Vec<Vec<[f32; 4]>> = Vec::new();
        for (at, heading, scale) in frames {
            let (sin, cos) = heading.sin_cos();
            let frame = |p: Vec3| Vec3::new(at[0] + scale * (p.z * sin + p.x * cos), at[1] + scale * p.y, at[2] + scale * (p.z * cos - p.x * sin));
            for b in &set.boxes {
                let centre = frame(Vec3::from(b.at));
                if (centre - e).length() > REACH {
                    continue;
                }
                let (ys, yc) = b.yaw.sin_cos();
                // the box's own corner (x, y, z) in its anchor's frame, then the world's
                let place = |x: f32, y: f32, z: f32| frame(Vec3::new(b.at[0] + z * ys + x * yc, b.at[1] + y, b.at[2] + z * yc - x * ys));
                let rgba = |c: u32, k: f32| [((c >> 24) & 0xFF) as f32 / 255.0 * k, ((c >> 16) & 0xFF) as f32 / 255.0 * k, ((c >> 8) & 0xFF) as f32 / 255.0 * k, 1.0];
                let (w, h, dp) = (b.size[0] / 2.0, b.size[1], b.size[2] / 2.0);
                let mut quad = |q: [Vec3; 4], c: [f32; 4]| {
                    // a quad as a strip: its corners in order round it, the strip 0 1 3 2
                    strips.push(vec![([0.0, 0.0], q[0].to_array()), ([0.0, 1.0], q[1].to_array()), ([1.0, 0.0], q[3].to_array()), ([1.0, 1.0], q[2].to_array())]);
                    colors.push(vec![c; 4]);
                };
                // the paper: the top, then the four sides, each shaded by its facing
                let shade = [1.0, 0.82, 0.68, 0.74, 0.88];
                let faces: [[(f32, f32, f32); 4]; 5] = [
                    [(-w, h, -dp), (w, h, -dp), (w, h, dp), (-w, h, dp)],
                    [(-w, 0.0, dp), (w, 0.0, dp), (w, h, dp), (-w, h, dp)],
                    [(w, 0.0, -dp), (w, 0.0, dp), (w, h, dp), (w, h, -dp)],
                    [(-w, 0.0, -dp), (w, 0.0, -dp), (w, h, -dp), (-w, h, -dp)],
                    [(-w, 0.0, -dp), (-w, 0.0, dp), (-w, h, dp), (-w, h, -dp)],
                ];
                for (f, k) in faces.iter().zip(shade) {
                    quad(f.map(|(x, y, z)| place(x, y, z)), rgba(b.color, k));
                }
                if b.ribbon & 0xFF == 0 {
                    continue;
                }
                // the ribbon: a band across the top and down two sides each way, a little proud of the paper
                let rw = 0.09 * b.size[0].min(b.size[2]);
                let o = 0.012;
                let (wo, ho, dpo) = (w + o, h + o, dp + o);
                for (f, k) in [
                    // along x: over the top and down the +x and -x sides
                    ([(-wo, ho, -rw), (wo, ho, -rw), (wo, ho, rw), (-wo, ho, rw)], 1.0),
                    ([(wo, 0.0, -rw), (wo, 0.0, rw), (wo, ho, rw), (wo, ho, -rw)], 0.68),
                    ([(-wo, 0.0, -rw), (-wo, 0.0, rw), (-wo, ho, rw), (-wo, ho, -rw)], 0.88),
                    // along z: over the top and down the +z and -z sides
                    ([(-rw, ho + 0.002, -dpo), (rw, ho + 0.002, -dpo), (rw, ho + 0.002, dpo), (-rw, ho + 0.002, dpo)], 1.0),
                    ([(-rw, 0.0, dpo), (rw, 0.0, dpo), (rw, ho, dpo), (-rw, ho, dpo)], 0.82),
                    ([(-rw, 0.0, -dpo), (rw, 0.0, -dpo), (rw, ho, -dpo), (-rw, ho, -dpo)], 0.74),
                ] {
                    quad(f.map(|(x, y, z)| place(x, y, z)), rgba(b.ribbon, k));
                }
                if !b.bow {
                    continue;
                }
                // the bow: two loops crossed on top, each a pair of tilted petals
                let bw = 0.32 * b.size[0].min(b.size[2]);
                let bh = 0.55 * bw;
                for (dx, dz) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                    let tip = (dx * bw, ho + bh, dz * bw);
                    let side = (dz * bw * 0.35, 0.0, dx * bw * 0.35);
                    let q = [(0.0, ho, 0.0), (tip.0 + side.0, tip.1, tip.2 + side.2), (tip.0 * 1.15, ho + bh * 0.4, tip.2 * 1.15), (tip.0 - side.0, tip.1, tip.2 - side.2)];
                    quad(q.map(|(x, y, z)| place(x, y, z)), rgba(b.ribbon, 0.95));
                }
            }
        }
        if !strips.is_empty() {
            out.push(Strips { texture: 0, blend: 0, ztest: true, zwrite: true, color: [1.0; 4], strips, colors });
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
            presents: Vec::new(),
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

    /// Presents: five faces a box, six ribbon bands with a ribbon and four bow
    /// petals with a bow, placed in the holder's frame; none past the reach.
    #[test]
    fn the_presents_are_wrapped() {
        let mut d = tree(Effect::Steady);
        d.presents.push(net::decor::Presents {
            anchor: Anchor::Npc("christmas tree".into()),
            boxes: vec![
                net::decor::GiftBox { at: [2.0, 0.0, 0.0], size: [1.0, 0.8, 1.0], yaw: 0.0, color: 0xC020_20FF, ribbon: 0xFFD0_40FF, bow: true },
                net::decor::GiftBox { at: [-2.0, 0.0, 0.0], size: [0.6, 0.6, 0.6], yaw: 0.3, color: 0x2050_C0FF, ribbon: 0, bow: false },
            ],
        });
        let h = [Holder { name: "Christmas Tree", at: [100.0, 50.0, 200.0], heading: 0.0, scale: 2.0 }];
        let p = presents_of(&d, &h, 0, [100.0, 52.0, 190.0]);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].strips.len(), (5 + 6 + 4) + 5, "the red box wrapped and bowed, the blue plain");
        assert_eq!(p[0].colors.len(), p[0].strips.len());
        // the red box's top at 50 + 2 x 0.8, its middle at x 100 + 2 x 2
        let top = &p[0].strips[0];
        assert!(top.iter().all(|(_, v)| (v[1] - 51.6).abs() < 1e-4), "{top:?}");
        let mid_x = top.iter().map(|(_, v)| v[0]).sum::<f32>() / 4.0;
        assert!((mid_x - 104.0).abs() < 1e-4);
        assert!(p[0].colors[0][0][0] > 0.7, "red paper");
        assert!(presents_of(&d, &h, 0, [100.0 + REACH + 10.0, 50.0, 200.0]).is_empty());
    }

    /// A decoration's art is known by its model, nothing else is.
    #[test]
    fn the_art_is_scenery() {
        let mut d = tree(Effect::Steady);
        d.art.push(net::decor::Art { model: 0x0B3B_A520, world: 0, zone: 22, sink: 0.0 });
        set(d);
        assert!(is_art(0x0B3B_A520));
        assert!(!is_art(0x84807490) && !is_art(0));
        set(Decor::default());
        assert!(!is_art(0x0B3B_A520));
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
