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

/// The layout of [`crate::session::OP_DECOR_LIGHTS`] this client reads: 2,
/// the walk-ons after the papers ([`WalkOn`]), and the server's rides
/// (`eqoa`'s `ride`: a vehicle's riders kept on its deck).
pub const LAYOUT: u8 = 2;

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
    /// How it looks past its glows (a server before the looks sends none:
    /// [`Look::default`])
    pub look: Look,
}

/// A string's look past its glows and plain cord (the owner, 2026-11-16:
/// "where dust is standing this model the magic globe can be used for bulbs
/// and made different colors"; "the rope hanging also can be used to
/// decorate the tree"): each bulb drawn as a model too, tinted its colour,
/// and the cord drawn with a texture, tinted the cord's colour.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Look {
    /// a model (DictID, one of the set's [`Asset`]s) drawn at each bulb, 0 none
    pub bulb_model: u32,
    /// its size across, in the string's units
    pub bulb_scale: f32,
    /// the cord's texture (DictID), 0 a plain wire
    pub cord_texture: u32,
    /// how long one repeat of it runs along the cord, in the string's units
    pub cord_repeat: f32,
    /// the cord drawn as tinsel (the owner, 2026-11-17: "the tinsel should be
    /// rotated per piece"): short pieces along it, each a pair of crossed
    /// quads at a twist of its own, not one band turned to the eye
    pub tinsel: bool,
}

/// Art a decoration draws that the player need not have loaded: the
/// textures and models `ids` of zone `zone` of world `world`'s file, read off
/// the player's own disc (a tent's cloth of Odus for a present in Freeport).
#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub world: u8,
    pub zone: u16,
    pub ids: Vec<u32>,
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
    /// The art the looks draw ([`Asset`])
    pub assets: Vec<Asset>,
    /// The NPCs a walk goes onto or through (layout 2)
    pub walk_ons: Vec<WalkOn>,
}

/// An NPC (by name) whose body a player's walk does not stop at (the server
/// owner, 2026-11-17: "let players run onto the sled without jumping"):
/// [`WALK_DECK`] walked into and stood on (Santa's sleigh: its sides pass the
/// walk, its top is a floor), [`WALK_PASS`] walked through (his team). The
/// server's movement check takes a deck as a floor `radius` across, up to
/// `height` over its feet.
#[derive(Clone, Debug, PartialEq)]
pub struct WalkOn {
    pub key: String,
    pub mode: u8,
    pub radius: f32,
    pub height: f32,
}

/// [`WalkOn::mode`]: walked into and stood on.
pub const WALK_DECK: u8 = 1;
/// [`WalkOn::mode`]: walked through.
pub const WALK_PASS: u8 = 2;

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
    /// its paper's texture (DictID, an [`Asset`]'s; 0 plain paper), tinted
    /// its colour times `paper_gain` (a dark cloth made as bright as plain
    /// paper)
    pub paper: u32,
    pub paper_gain: f32,
}

/// [`crate::session::OP_DECOR_LIGHTS`]'s body (after the opcode); none if it
/// is cut short.
pub fn read(data: &[u8]) -> Option<Decor> {
    let mut r = Reader::new(data);
    let n = r.u16() as usize;
    let mut out = Decor::default();
    // each string sent: where it is in `out.strings`, if it was kept
    let mut kept_strings: Vec<Option<usize>> = Vec::with_capacity(n);
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
            kept_strings.push(Some(out.strings.len()));
            out.strings.push(LightString { anchor, effect, period, size, bright, cord, cord_width, cord_sag, colors, bulbs, look: Look::default() });
        } else {
            kept_strings.push(None);
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
    // the presents (a server before them sends none): each set's boxes sent,
    // where each is in its set, if kept
    let mut kept_boxes: Vec<Vec<Option<usize>>> = Vec::new();
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
            let mut kept = Vec::with_capacity(nb.min(64));
            for _ in 0..nb {
                let at = [r.f32(), r.f32(), r.f32()];
                let size = [r.f32(), r.f32(), r.f32()];
                let yaw = r.f32();
                let (color, ribbon, bow) = (r.u32(), r.u32(), r.u8() != 0);
                if r.error {
                    return None;
                }
                if at.iter().chain(size.iter()).all(|v| v.is_finite()) && yaw.is_finite() {
                    kept.push(Some(boxes.len()));
                    boxes.push(GiftBox { at, size, yaw, color, ribbon, bow, paper: 0, paper_gain: 1.0 });
                } else {
                    kept.push(None);
                }
            }
            if r.error {
                return None;
            }
            out.presents.push(Presents { anchor, boxes });
            kept_boxes.push(kept);
        }
    }
    // the looks (a server before them sends none): the assets (u16 count,
    // each u8 world, u16 zone, u8 count of u32 DictIDs), the strings' (u16
    // count, each u16 string sent, u32 bulb model, f32 its scale, u32 cord
    // texture, f32 its repeat) and the papers (u16 count of sets, as sent,
    // each u16 count of boxes, each u32 paper, f32 its gain)
    if r.pos < data.len() {
        let na = r.u16() as usize;
        for _ in 0..na {
            let (world, zone) = (r.u8(), r.u16());
            let k = r.u8() as usize;
            let ids = (0..k).map(|_| r.u32()).collect();
            out.assets.push(Asset { world, zone, ids });
        }
        let nl = r.u16() as usize;
        for _ in 0..nl {
            let i = r.u16() as usize;
            let look = Look { bulb_model: r.u32(), bulb_scale: r.f32(), cord_texture: r.u32(), cord_repeat: r.f32(), tinsel: false };
            if r.error {
                return None;
            }
            let ok = look.bulb_scale.is_finite() && look.cord_repeat.is_finite();
            if let Some(Some(s)) = kept_strings.get(i)
                && ok
            {
                out.strings[*s].look = look;
            }
        }
        let ns = r.u16() as usize;
        for set in 0..ns {
            let nb = r.u16() as usize;
            for b in 0..nb {
                let (paper, gain) = (r.u32(), r.f32());
                if r.error {
                    return None;
                }
                if let Some(Some(k)) = kept_boxes.get(set).and_then(|v| v.get(b))
                    && gain.is_finite()
                {
                    let g = &mut out.presents[set].boxes[*k];
                    (g.paper, g.paper_gain) = (paper, gain);
                }
            }
        }
    }
    // the cords' styles (a server before them sends none): a u16 count, each
    // the u16 index of a string among those sent and a u8 style (1 tinsel)
    if r.pos < data.len() {
        let n = r.u16() as usize;
        for _ in 0..n {
            let (i, style) = (r.u16() as usize, r.u8());
            if r.error {
                return None;
            }
            if let Some(Some(s)) = kept_strings.get(i) {
                out.strings[*s].look.tinsel = style == 1;
            }
        }
    }
    // the walk-ons (layout 2; a server before them sends none): a u16 count,
    // each the NPC's name (u8 byte count), u8 mode, f32 radius, f32 height
    if r.pos < data.len() {
        let n = r.u16() as usize;
        for _ in 0..n {
            let len = r.u8() as usize;
            let key = crate::npctint::key(&String::from_utf8_lossy(r.bytes(len)));
            let (mode, radius, height) = (r.u8(), r.f32(), r.f32());
            if r.error {
                return None;
            }
            if (mode == WALK_DECK || mode == WALK_PASS) && radius.is_finite() && height.is_finite() {
                out.walk_ons.push(WalkOn { key, mode, radius, height });
            }
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

    /// The walk-ons after the looks (layout 2): the sleigh a deck, a horse
    /// passed through; a mode it does not know dropped.
    #[test]
    fn reads_the_walk_ons() {
        let mut b = body();
        b.extend(0u16.to_le_bytes()); // no art
        b.extend(0u16.to_le_bytes()); // no presents
        b.extend([0u8; 6]); // no assets, looks or papers
        b.extend(0u16.to_le_bytes()); // no tinsel cords: the server writes their section before the walk-ons
        b.extend(3u16.to_le_bytes());
        for (name, mode, r, h) in [("Santa's Sleigh", 1u8, 2.6f32, 1.0f32), ("rudolph", 2, 3.0, 1.0), ("odd", 9, 1.0, 1.0)] {
            b.push(name.len() as u8);
            b.extend(name.as_bytes());
            b.push(mode);
            b.extend(r.to_le_bytes());
            b.extend(h.to_le_bytes());
        }
        let d = read(&b).expect("read");
        assert_eq!(d.walk_ons, vec![WalkOn { key: crate::npctint::key("santa's sleigh"), mode: WALK_DECK, radius: 2.6, height: 1.0 }, WalkOn { key: crate::npctint::key("Rudolph"), mode: WALK_PASS, radius: 3.0, height: 1.0 }]);
        // a server before them: none
        b.truncate(b.len() - (2 + 3 * 10 + "Santa's Sleigh".len() + "rudolph".len() + "odd".len()));
        assert!(read(&b).expect("read").walk_ons.is_empty());
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
        assert_eq!(d.presents[0].boxes[0], GiftBox { at: [1.0, 0.0, 2.0], size: [1.0, 0.8, 1.0], yaw: 0.5, color: 0xC020_20FF, ribbon: 0, bow: true, paper: 0, paper_gain: 1.0 });
        assert_eq!(read(&p[..p.len() - 3]), None, "cut short");
        // with no art after them (none to read), and cut short: nothing
        assert_eq!(read(&body()).map(|d| d.art.len()), Some(0));
        let cut = &b[..b.len() - 7];
        assert_eq!(read(cut), None);
        assert_eq!(read(&body()[..30]), None);
        // and the looks after the presents: an asset, the first string's
        // globe bulbs and rope cord, the box's paper
        let mut l = p.clone();
        l.extend(1u16.to_le_bytes());
        l.push(2);
        l.extend(15u16.to_le_bytes());
        l.push(2);
        l.extend(0x4BC0_D7FAu32.to_le_bytes());
        l.extend(0x7B3D_8677u32.to_le_bytes());
        l.extend(2u16.to_le_bytes());
        l.extend(0u16.to_le_bytes());
        l.extend(0xF8E6_C045u32.to_le_bytes());
        l.extend(0.35f32.to_le_bytes());
        l.extend(0x7B3D_8677u32.to_le_bytes());
        l.extend(1.5f32.to_le_bytes());
        // one for a string never sent: let go
        l.extend(9u16.to_le_bytes());
        l.extend([0u8; 16]);
        l.extend(1u16.to_le_bytes());
        l.extend(1u16.to_le_bytes());
        l.extend(0x4BC0_D7FAu32.to_le_bytes());
        l.extend(2.0f32.to_le_bytes());
        let d = read(&l).expect("read with looks");
        assert_eq!(d.assets, vec![Asset { world: 2, zone: 15, ids: vec![0x4BC0_D7FA, 0x7B3D_8677] }]);
        assert_eq!(d.strings[0].look, Look { bulb_model: 0xF8E6_C045, bulb_scale: 0.35, cord_texture: 0x7B3D_8677, cord_repeat: 1.5, tinsel: false });
        assert_eq!(d.strings[1].look, Look::default());
        assert_eq!((d.presents[0].boxes[0].paper, d.presents[0].boxes[0].paper_gain), (0x4BC0_D7FA, 2.0));
        assert_eq!(read(&l[..l.len() - 2]), None, "cut short");
        // and the cords' styles after them: the first string's tinsel
        let mut t = l.clone();
        t.extend(1u16.to_le_bytes());
        t.extend(0u16.to_le_bytes());
        t.push(1);
        let d = read(&t).expect("read with styles");
        assert!(d.strings[0].look.tinsel && !d.strings[1].look.tinsel);
        assert_eq!(read(&t[..t.len() - 1]), None, "cut short");
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
