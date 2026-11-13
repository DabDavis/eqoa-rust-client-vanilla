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

use crate::stream::Reader;

/// The newest layout of the tints this client reads: 2, each name's colour
/// followed by its glow.
pub const LAYOUT: u8 = 2;

/// A name's tint: its colour, which multiplies the body's layer colours, and
/// its glow, which lights it (each 0xRRGGBBAA; a glow of 0 is none, and
/// layout 1 has none).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tint {
    pub color: u32,
    pub glow: u32,
}

impl Tint {
    /// A tint of a colour alone (layout 1's).
    pub const fn color(color: u32) -> Self {
        Tint { color, glow: 0 }
    }
}

/// Whether a glow lights anything: one of alpha 0, or black, is none
/// (go-eqoa-server's `NPCGlowShows`).
pub fn glows(glow: u32) -> bool {
    glow & 0xFF != 0 && glow >> 8 != 0
}

/// The tints in `layout` (1, or 2 and more as 2): a u16 count, then each its
/// NPC name (a u8 byte count and UTF-8, as the server keys a name: [`key`]),
/// its u32 colour 0xRRGGBBAA and, in layout 2, its u32 glow 0xRRGGBBAA, each
/// little-endian. None when the message is short; a name that keys to nothing
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
        if r.error {
            break;
        }
        if !name.is_empty() {
            out.push((name, Tint { color, glow }));
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
            vec![("mr. moogie".to_string(), Tint { color: 0xFFFF_FFFF, glow: 0xFF40_4080 }), ("yhafiran".to_string(), Tint::color(0x558C_50FF))]
        );
        assert_ne!(read(&m, 1), read(&m, 2), "the control: layout 1 reads no glow");
        assert!(read(&m[..m.len() - 1], 2).is_none(), "a glow cut short");
        assert!(glows(0xFF40_4080) && !glows(0xFF40_4000) && !glows(0x0000_00FF) && !glows(0));
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
