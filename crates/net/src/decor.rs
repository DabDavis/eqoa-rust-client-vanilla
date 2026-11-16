//! go-eqoa-server's own, no retail message: strings of decoration lights and
//! the art its decorations wear ([`crate::session::OP_DECOR_LIGHTS`]; the
//! server owner, 2026-11-16: a Christmas tree in Freeport's plaza, "lights on
//! trees in different colors. the lights can be a template so we can put
//! christmas lights on fences buildings etc"). The server offers them once the
//! player is in the world; a client that draws them answers the layout it reads
//! (1) and is sent the whole set, at once and whenever a GM turns a season or
//! edits a string. A PS2 never answers: it draws none, and is sent the NPCs'
//! own bodies (the server's `decorart`).
//!
//! A string hangs on every NPC of a name (its bulbs in the NPC's own frame at
//! its size 1: x right, y up, z ahead) or in a world (world coordinates). Each
//! bulb has a colour of the string's and a phase in its effect's cycle.

use crate::stream::Reader;

/// The layout of [`crate::session::OP_DECOR_LIGHTS`] this client reads.
pub const LAYOUT: u8 = 1;

/// How a string's bulbs light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// always lit
    Steady,
    /// each bulb brightens and dims on its own
    Twinkle,
    /// the odd bulbs and the even ones in turn
    Alternate,
    /// a run of light along the string
    Chase,
}

impl Effect {
    fn of(b: u8) -> Effect {
        match b {
            1 => Effect::Twinkle,
            2 => Effect::Alternate,
            3 => Effect::Chase,
            _ => Effect::Steady,
        }
    }
}

/// Where a string hangs.
#[derive(Clone, Debug, PartialEq)]
pub enum Anchor {
    /// on every NPC whose name keys this (`crate::npctint::key`)
    Npc(String),
    /// in this world, at world coordinates
    World(u8),
}

/// One bulb: its place, its colour (an index into the string's) and its
/// phase (of 256 in its effect's cycle).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bulb {
    pub at: [f32; 3],
    /// the colour's index ([`COLOR`] of it), the string's top ([`TOP`]) and
    /// the cord broken before it ([`BREAK`])
    pub color: u8,
    pub phase: u8,
}

/// The bits of [`Bulb::color`] that are the colour's index.
pub const COLOR: u8 = 0x3F;
/// The bit of [`Bulb::color`] that marks the string's top (a tree's star).
pub const TOP: u8 = 0x80;
/// The bit of [`Bulb::color`] that breaks the cord before the bulb (a ring's
/// first after the ring below).
pub const BREAK: u8 = 0x40;

/// A string of lights.
#[derive(Clone, Debug, PartialEq)]
pub struct LightString {
    pub anchor: Anchor,
    pub effect: Effect,
    /// seconds of one cycle
    pub period: f32,
    /// a bulb's glow across
    pub size: f32,
    pub bright: f32,
    /// The cord the bulbs hang on, bulb to bulb (the owner, 2026-11-16: "for
    /// the lights let there look like a cord connecting them"): 0xRRGGBBAA, 0
    /// none (a string of single points)
    pub cord: u32,
    /// its thickness, in the string's units
    pub cord_width: f32,
    /// how far it hangs between two bulbs, of their distance
    pub cord_sag: f32,
    /// 0xRRGGBBAA
    pub colors: Vec<u32>,
    pub bulbs: Vec<Bulb>,
}

/// Art a decoration's NPC wears that lives in a zone of a world's file: the
/// sprite's DictID, the world and the zone's index, which a client reads to
/// have the sprite, and how much lower it is drawn than the NPC stands (in
/// its own units: a fir's bare stem below its boughs, so they rest on the
/// ground: a creature stands on the floor, so the art is lowered).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Art {
    pub model: u32,
    pub world: u8,
    pub zone: u16,
    pub sink: f32,
}

/// The whole set the server sent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decor {
    pub strings: Vec<LightString>,
    pub art: Vec<Art>,
    /// Presents under a tree (the owner, 2026-11-16: "we need some different
    /// color presents under the tree"), each set hung as a string is
    pub presents: Vec<Presents>,
}

/// A set of gift boxes, hung on an NPC's name or placed in a world as a
/// string is.
#[derive(Clone, Debug, PartialEq)]
pub struct Presents {
    pub anchor: Anchor,
    pub boxes: Vec<GiftBox>,
}

/// One present: its bottom's middle and its size (in its anchor's frame), its
/// turn (radians), its paper's colour 0xRRGGBBAA, its ribbon's (0 none) and
/// whether a bow sits on top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GiftBox {
    pub at: [f32; 3],
    pub size: [f32; 3],
    pub yaw: f32,
    pub color: u32,
    pub ribbon: u32,
    pub bow: bool,
}

/// [`crate::session::OP_DECOR_LIGHTS`]'s body (after the opcode); none if it
/// is cut short.
pub fn read(data: &[u8]) -> Option<Decor> {
    let mut r = Reader::new(data);
    let n = r.u16() as usize;
    let mut out = Decor::default();
    for _ in 0..n {
        let anchor = match r.u8() {
            1 => {
                let len = r.u8() as usize;
                Anchor::Npc(crate::npctint::key(&String::from_utf8_lossy(r.bytes(len))))
            }
            _ => Anchor::World(r.u8()),
        };
        let effect = Effect::of(r.u8());
        let (period, size, bright) = (r.f32(), r.f32(), r.f32());
        let (cord, cord_width, cord_sag) = (r.u32(), r.f32(), r.f32());
        let nc = r.u8() as usize;
        let colors: Vec<u32> = (0..nc).map(|_| r.u32()).collect();
        let nb = r.u16() as usize;
        let mut bulbs = Vec::with_capacity(nb.min(2048));
        for _ in 0..nb {
            let at = [r.f32(), r.f32(), r.f32()];
            let (color, phase) = (r.u8(), r.u8());
            bulbs.push(Bulb { at, color, phase });
        }
        if r.error {
            return None;
        }
        let ok = |v: f32| v.is_finite();
        if ok(period) && ok(size) && ok(bright) && ok(cord_width) && ok(cord_sag) && bulbs.iter().all(|b| b.at.iter().all(|v| v.is_finite())) && !colors.is_empty() {
            out.strings.push(LightString { anchor, effect, period, size, bright, cord, cord_width, cord_sag, colors, bulbs });
        }
    }
    // the art (a server before it sends none)
    if r.pos < data.len() {
        let na = r.u16() as usize;
        for _ in 0..na {
            let a = Art { model: r.u32(), world: r.u8(), zone: r.u16(), sink: r.f32() };
            if r.error || !a.sink.is_finite() {
                return None;
            }
            out.art.push(a);
        }
    }
    // the presents (a server before them sends none)
    if r.pos < data.len() {
        let ns = r.u16() as usize;
        for _ in 0..ns {
            let anchor = match r.u8() {
                1 => {
                    let len = r.u8() as usize;
                    Anchor::Npc(crate::npctint::key(&String::from_utf8_lossy(r.bytes(len))))
                }
                _ => Anchor::World(r.u8()),
            };
            let nb = r.u16() as usize;
            let mut boxes = Vec::with_capacity(nb.min(64));
            for _ in 0..nb {
                let at = [r.f32(), r.f32(), r.f32()];
                let size = [r.f32(), r.f32(), r.f32()];
                let yaw = r.f32();
                let (color, ribbon, bow) = (r.u32(), r.u32(), r.u8() != 0);
                if r.error {
                    return None;
                }
                if at.iter().chain(size.iter()).all(|v| v.is_finite()) && yaw.is_finite() {
                    boxes.push(GiftBox { at, size, yaw, color, ribbon, bow });
                }
            }
            if r.error {
                return None;
            }
            out.presents.push(Presents { anchor, boxes });
        }
    }
    (!r.error).then_some(out)
}

/// How lit a bulb is at `t` seconds (0 to 1): its effect at its phase.
pub fn lit(effect: Effect, period: f32, phase: u8, t: f64) -> f32 {
    let period = if period > 0.0 { period as f64 } else { 1.5 };
    let f = (t / period + phase as f64 / 256.0).rem_euclid(1.0) as f32;
    match effect {
        Effect::Steady => 1.0,
        // a slow swell and a quick dip, never quite out
        Effect::Twinkle => {
            let s = (f * std::f32::consts::TAU).sin();
            0.55 + 0.45 * s * s.abs().sqrt()
        }
        // on for half the cycle, off (a glow) for the other
        Effect::Alternate => {
            if f < 0.5 {
                1.0
            } else {
                0.18
            }
        }
        // a run of light: bright where the phase is, fading behind it
        Effect::Chase => 0.15 + 0.85 * (1.0 - f).powi(3),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body() -> Vec<u8> {
        let mut b = vec![2, 0];
        // on the NPC "Christmas Tree": twinkle, 1.5 s, 0.3 across, 1.0, two colours, one bulb
        b.extend([1, 14]);
        b.extend(b"christmas tree");
        b.push(1);
        for v in [1.5f32, 0.3, 1.0] {
            b.extend(v.to_le_bytes());
        }
        b.extend(0x1E3F_1EFFu32.to_le_bytes());
        for v in [0.06f32, 0.06] {
            b.extend(v.to_le_bytes());
        }
        b.push(2);
        b.extend(0xFF20_20FFu32.to_le_bytes());
        b.extend(0x20D0_40FFu32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        for v in [1.0f32, 2.0, 3.0] {
            b.extend(v.to_le_bytes());
        }
        b.extend([1, 7]);
        // in world 3: chase, no bulbs
        b.extend([0, 3, 3]);
        for v in [2.0f32, 0.5, 2.0] {
            b.extend(v.to_le_bytes());
        }
        b.extend([0u8; 12]);
        b.push(1);
        b.extend(0xFFFF_FFFFu32.to_le_bytes());
        b.extend(0u16.to_le_bytes());
        b
    }

    #[test]
    fn reads_the_strings_and_the_art() {
        let mut b = body();
        b.extend(1u16.to_le_bytes());
        b.extend(0x0E31_8233u32.to_le_bytes());
        b.push(0);
        b.extend(32u16.to_le_bytes());
        b.extend(9.5f32.to_le_bytes());
        let d = read(&b).expect("read");
        assert_eq!(d.strings.len(), 2);
        let s = &d.strings[0];
        assert_eq!(s.anchor, Anchor::Npc("christmas tree".into()));
        assert_eq!((s.effect, s.period, s.size, s.bright), (Effect::Twinkle, 1.5, 0.3, 1.0));
        assert_eq!((s.cord, s.cord_width, s.cord_sag), (0x1E3F_1EFF, 0.06, 0.06));
        assert_eq!(d.strings[1].cord, 0, "the second has none");
        assert_eq!(s.colors, vec![0xFF20_20FF, 0x20D0_40FF]);
        assert_eq!(s.bulbs, vec![Bulb { at: [1.0, 2.0, 3.0], color: 1, phase: 7 }]);
        assert_eq!(d.strings[1].anchor, Anchor::World(3));
        assert_eq!(d.strings[1].effect, Effect::Chase);
        assert_eq!(d.art, vec![Art { model: 0x0E31_8233, world: 0, zone: 32, sink: 9.5 }]);
        assert!(d.presents.is_empty(), "a server before the presents sends none");
        // and a set of presents after the art
        let mut p = b.clone();
        p.extend(1u16.to_le_bytes());
        p.extend([1, 3]);
        p.extend(b"elf");
        p.extend(1u16.to_le_bytes());
        for v in [1.0f32, 0.0, 2.0, 1.0, 0.8, 1.0, 0.5] {
            p.extend(v.to_le_bytes());
        }
        p.extend(0xC020_20FFu32.to_le_bytes());
        p.extend(0u32.to_le_bytes());
        p.push(1);
        let d = read(&p).expect("read with presents");
        assert_eq!(d.presents.len(), 1);
        assert_eq!(d.presents[0].anchor, Anchor::Npc("elf".into()));
        assert_eq!(d.presents[0].boxes[0], GiftBox { at: [1.0, 0.0, 2.0], size: [1.0, 0.8, 1.0], yaw: 0.5, color: 0xC020_20FF, ribbon: 0, bow: true });
        assert_eq!(read(&p[..p.len() - 3]), None, "cut short");
        // with no art after them (none to read), and cut short: nothing
        assert_eq!(read(&body()).map(|d| d.art.len()), Some(0));
        let cut = &b[..b.len() - 7];
        assert_eq!(read(cut), None);
        assert_eq!(read(&body()[..30]), None);
    }

    #[test]
    fn the_effects_light_within_bounds() {
        for e in [Effect::Steady, Effect::Twinkle, Effect::Alternate, Effect::Chase] {
            for i in 0..200 {
                let v = lit(e, 1.5, (i * 37 % 256) as u8, i as f64 * 0.071);
                assert!((0.0..=1.0).contains(&v), "{e:?}: {v}");
            }
        }
        // alternate: odd and even half a cycle apart, one lit while the other is not
        assert!(lit(Effect::Alternate, 2.0, 0, 0.2) > 0.9 && lit(Effect::Alternate, 2.0, 128, 0.2) < 0.3);
        // twinkle: two bulbs out of step differ
        assert_ne!(lit(Effect::Twinkle, 1.5, 0, 0.3), lit(Effect::Twinkle, 1.5, 90, 0.3));
    }
}
