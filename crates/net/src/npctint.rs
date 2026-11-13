//! go-eqoa-server's own, no retail message: a tint on the bodies of the NPCs of
//! a name ([`crate::session::OP_NPC_TINTS`]; the server owner, 2026-11-10:
//! Yhafiran, "the black and green oriental dragon", wears the disc's only green
//! oriental dragon, which the Plane of Disease's twins wear too, and "vanilla
//! draws Yhafiran tinted toward black and green"). The server offers them once
//! the player is in the world; a client that draws them answers 1 and is sent
//! the whole set by name, at once and whenever a GM changes it, so every spawn
//! of the name, a respawn and a GM's copy are tinted alike with nothing sent as
//! they come into view. A PS2 never answers: it draws each body in its own
//! colours, as this client does on every server that makes no offer.
//!
//! Layout 2 (the GM Jibrish's petition 1893: Mr. Moogie's body in a soft red
//! glow) puts a glow after each colour; this client answers an offer of 2 or
//! more with 2 ([`LAYOUT`]) and an older server's 1 with 1.
//!
//! Layout 3 (the owner, 2026-11-13: "moogies aura also can be a template to
//! make any model have particle effects") puts an aura after each glow
//! ([`Aura`]): which of the two auras the name's bodies wear (the shell, one
//! form-fitting glow round the whole body; the particles, a held item's
//! emitter on the body's nodes), the item or model whose particle sprite the
//! particles are, and on how many nodes. A layout 2 server's glow is the
//! shell alone ([`Aura::DEFAULT`]).

use crate::stream::Reader;

/// The newest layout of the tints this client reads: 3, each name's colour
/// followed by its glow and its aura.
pub const LAYOUT: u8 = 3;

/// The auras a name's bodies wear (layout 3: a u8 kind, a u32 source DictID
/// and a u8 node count after the glow), each in the glow's colour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Aura {
    /// [`Aura::DEFAULT`], [`Aura::NONE`], [`Aura::SHELL`],
    /// [`Aura::PARTICLES`] or [`Aura::BOTH`]; any other is the default.
    pub kind: u8,
    /// The DictID of the item or model whose particle sprite the particles
    /// are (an ITEM.ESF hand model, or any model with one); 0 the Sword of
    /// Virtue's (`eqoa`'s `aura::SWORD_OF_VIRTUE`).
    pub source: u32,
    /// How many of the body's nodes carry the particles; 0 the default (8).
    pub nodes: u8,
}

impl Aura {
    /// The default: the shell alone while the glow lights, nothing without
    /// (a layout 2 server's every glow).
    pub const DEFAULT: u8 = 0;
    /// No aura, whatever the glow.
    pub const NONE: u8 = 1;
    /// The shell alone (needs a glow: it is drawn in the glow's colour).
    pub const SHELL: u8 = 2;
    /// The particles alone (in the glow's hue; with no glow, the source's own
    /// colours).
    pub const PARTICLES: u8 = 3;
    /// The shell and the particles.
    pub const BOTH: u8 = 4;

    /// Whether a body in tint `t` wears the shell: asked for (the default
    /// included) and the glow lights.
    pub fn shell(t: &Tint) -> bool {
        glows(t.glow) && !matches!(t.aura.kind, Self::NONE | Self::PARTICLES)
    }

    /// Whether a body in tint `t` wears the particles: asked for alone.
    pub fn particles(t: &Tint) -> bool {
        matches!(t.aura.kind, Self::PARTICLES | Self::BOTH)
    }
}

/// A name's tint: its colour, which multiplies the body's layer colours, and
/// its glow, which lights it (each 0xRRGGBBAA; a glow of 0 is none, and
/// layout 1 has none).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tint {
    pub color: u32,
    pub glow: u32,
    /// Its aura (layout 3; the default before).
    pub aura: Aura,
}

impl Tint {
    /// A tint of a colour alone (layout 1's).
    pub const fn color(color: u32) -> Self {
        Tint { color, glow: 0, aura: Aura { kind: Aura::DEFAULT, source: 0, nodes: 0 } }
    }
}

/// Whether a glow lights anything: one of alpha 0, or black, is none
/// (go-eqoa-server's `NPCGlowShows`).
pub fn glows(glow: u32) -> bool {
    glow & 0xFF != 0 && glow >> 8 != 0
}

/// The tints in `layout` (1, 2, or 3 and more as 3): a u16 count, then each
/// its NPC name (a u8 byte count and UTF-8, as the server keys a name:
/// [`key`]), its u32 colour 0xRRGGBBAA, in layout 2 and on its u32 glow
/// 0xRRGGBBAA, and in layout 3 its aura ([`Aura`]: a u8 kind, a u32 source
/// DictID, a u8 node count), each little-endian. None when the message is short; a name that keys to nothing
/// is left out.
pub fn read(data: &[u8], layout: u8) -> Option<Vec<(String, Tint)>> {
    let mut r = Reader::new(data);
    let n = r.u16() as usize;
    let mut out = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        let len = r.u8() as usize;
        let name = key(&String::from_utf8_lossy(r.bytes(len)));
        let color = r.u32();
        let glow = if layout >= 2 { r.u32() } else { 0 };
        let aura = if layout >= 3 { Aura { kind: r.u8(), source: r.u32(), nodes: r.u8() } } else { Aura::default() };
        if r.error {
            break;
        }
        if !name.is_empty() {
            out.push((name, Tint { color, glow, aura }));
        }
    }
    (!r.error).then_some(out)
}

/// A name as the tints are keyed: trimmed, a trailing '$' and the space before
/// it taken off, in ASCII lower case (go-eqoa-server's `NPCClassKey`), so an
/// entity channel's name and a tint's meet whatever the case.
pub fn key(name: &str) -> String {
    let n = name.trim();
    let n = n.strip_suffix('$').unwrap_or(n).trim();
    n.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tint(name: &str, color: u32) -> Vec<u8> {
        let mut b = vec![name.len() as u8];
        b.extend_from_slice(name.as_bytes());
        b.extend_from_slice(&color.to_le_bytes());
        b
    }

    /// Layout 2 reads a glow after each colour: the server's own bytes for Mr.
    /// Moogie (white, a soft red glow) read to both; the same bytes as layout 1
    /// misread (the control: the layout matters); a glow cut short is nothing.
    #[test]
    fn layout_2_reads_a_glow_after_each_colour() {
        let mut m = vec![2u8, 0];
        m.extend(tint("Mr. Moogie", 0xFFFF_FFFF));
        m.extend_from_slice(&0xFF40_4080u32.to_le_bytes());
        m.extend(tint("Yhafiran", 0x558C_50FF));
        m.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read(&m, 2).unwrap(),
            vec![("mr. moogie".to_string(), Tint { color: 0xFFFF_FFFF, glow: 0xFF40_4080, ..Default::default() }), ("yhafiran".to_string(), Tint::color(0x558C_50FF))]
        );
        assert_ne!(read(&m, 1), read(&m, 2), "the control: layout 1 reads no glow");
        assert!(read(&m[..m.len() - 1], 2).is_none(), "a glow cut short");
        assert!(glows(0xFF40_4080) && !glows(0xFF40_4000) && !glows(0x0000_00FF) && !glows(0));
    }

    /// Layout 3 reads an aura after each glow: a skeleton in a green glow
    /// wearing both auras, the Sword of Virtue's on 12 nodes; Mr. Moogie in
    /// the default (his shell); the same bytes as layout 2 misread (the
    /// control); an aura cut short is nothing. Which aura each wears.
    #[test]
    fn layout_3_reads_an_aura_after_each_glow() {
        let mut m = vec![2u8, 0];
        m.extend(tint("a skeleton", 0xFFFF_FFFF));
        m.extend_from_slice(&0x40FF_40FFu32.to_le_bytes());
        m.push(Aura::BOTH);
        m.extend_from_slice(&0x88EC_092Du32.to_le_bytes());
        m.push(12);
        m.extend(tint("Mr. Moogie", 0xFF60_60FF));
        m.extend_from_slice(&0xFF00_00FFu32.to_le_bytes());
        m.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        let got = read(&m, 3).unwrap();
        let skel = Tint { color: 0xFFFF_FFFF, glow: 0x40FF_40FF, aura: Aura { kind: Aura::BOTH, source: 0x88EC_092D, nodes: 12 } };
        let moogie = Tint { color: 0xFF60_60FF, glow: 0xFF00_00FF, aura: Aura::default() };
        assert_eq!(got, vec![("a skeleton".to_string(), skel), ("mr. moogie".to_string(), moogie)]);
        assert_ne!(read(&m, 2), Some(got), "the control: layout 2 reads no aura");
        assert!(read(&m[..m.len() - 1], 3).is_none(), "an aura cut short");
        assert_eq!(read(&m, 4), read(&m, 3), "a newer layout read as 3");
        // which each wears
        assert!(Aura::shell(&skel) && Aura::particles(&skel));
        assert!(Aura::shell(&moogie) && !Aura::particles(&moogie), "the default: the shell alone");
        let with = |kind: u8, glow: u32| Tint { color: 0xFFFF_FFFF, glow, aura: Aura { kind, source: 0, nodes: 0 } };
        assert_eq!((Aura::shell(&with(Aura::SHELL, 0xFF00_00FF)), Aura::particles(&with(Aura::SHELL, 0xFF00_00FF))), (true, false));
        assert_eq!((Aura::shell(&with(Aura::PARTICLES, 0xFF00_00FF)), Aura::particles(&with(Aura::PARTICLES, 0xFF00_00FF))), (false, true));
        assert_eq!((Aura::shell(&with(Aura::NONE, 0xFF00_00FF)), Aura::particles(&with(Aura::NONE, 0xFF00_00FF))), (false, false));
        assert!(!Aura::shell(&with(Aura::DEFAULT, 0)) && !Aura::shell(&with(Aura::BOTH, 0)), "no glow: no shell");
        assert!(Aura::particles(&with(Aura::PARTICLES, 0)), "particles need no glow");
        assert!(!Aura::shell(&Tint::color(0x558C_50FF)) && !Aura::particles(&Tint::color(0x558C_50FF)), "a colour alone: nothing");
    }

    /// A set reads to its tints in order, keyed (case, spaces and a trailing
    /// '$' as the server's key takes them; one keyed to nothing left out); the
    /// server's own bytes for Yhafiran read to his name and colour; a set cut
    /// short reads to nothing (the control), and so does none; an empty set is
    /// none tinted.
    #[test]
    fn a_set_reads_whole_or_not_at_all() {
        let yhafiran = [1, 0, 8, b'y', b'h', b'a', b'f', b'i', b'r', b'a', b'n', 0xFF, 0x50, 0x8C, 0x55];
        assert_eq!(read(&yhafiran, 1).unwrap(), vec![("yhafiran".to_string(), Tint::color(0x558C_50FF))]);
        let mut m = vec![3u8, 0];
        m.extend(tint("Lady Vox $", 0xFF00_00FF));
        m.extend(tint("  ", 0x1020_30FF));
        m.extend(tint("moss snake", 0x8080_8080));
        assert_eq!(read(&m, 1).unwrap(), vec![("lady vox".to_string(), Tint::color(0xFF00_00FF)), ("moss snake".to_string(), Tint::color(0x8080_8080))]);
        assert_eq!(read(&[0, 0], 1).unwrap(), vec![], "an empty set: none tinted");
        assert!(read(&m[..m.len() - 1], 1).is_none(), "the control: one byte short");
        assert!(read(&yhafiran[..5], 1).is_none(), "a name cut short");
        assert!(read(&[], 1).is_none());
    }

    /// The key: the server's `NPCClassKey`.
    #[test]
    fn names_key_as_the_server_keys_them() {
        assert_eq!(key("Yhafiran"), "yhafiran");
        assert_eq!(key("  Uzrathixius  "), "uzrathixius");
        assert_eq!(key("Lady Vox$"), "lady vox");
        assert_eq!(key("Lady Vox $ "), "lady vox");
        assert_eq!(key("$"), "");
        assert_ne!(key("a moss snake"), key("moss snake"));
    }
}
