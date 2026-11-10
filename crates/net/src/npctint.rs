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

use crate::stream::Reader;

/// The tints: a u16 count, then each its NPC name (a u8 byte count and UTF-8,
/// as the server keys a name: [`key`]) and its u32 colour 0xRRGGBBAA,
/// little-endian. None when the message is short; a name that keys to nothing
/// is left out.
pub fn read(data: &[u8]) -> Option<Vec<(String, u32)>> {
    let mut r = Reader::new(data);
    let n = r.u16() as usize;
    let mut out = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        let len = r.u8() as usize;
        let name = key(&String::from_utf8_lossy(r.bytes(len)));
        let color = r.u32();
        if r.error {
            break;
        }
        if !name.is_empty() {
            out.push((name, color));
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

    /// A set reads to its tints in order, keyed (case, spaces and a trailing
    /// '$' as the server's key takes them; one keyed to nothing left out); the
    /// server's own bytes for Yhafiran read to his name and colour; a set cut
    /// short reads to nothing (the control), and so does none; an empty set is
    /// none tinted.
    #[test]
    fn a_set_reads_whole_or_not_at_all() {
        let yhafiran = [1, 0, 8, b'y', b'h', b'a', b'f', b'i', b'r', b'a', b'n', 0xFF, 0x50, 0x8C, 0x55];
        assert_eq!(read(&yhafiran).unwrap(), vec![("yhafiran".to_string(), 0x558C_50FF)]);
        let mut m = vec![3u8, 0];
        m.extend(tint("Lady Vox $", 0xFF00_00FF));
        m.extend(tint("  ", 0x1020_30FF));
        m.extend(tint("moss snake", 0x8080_8080));
        assert_eq!(read(&m).unwrap(), vec![("lady vox".to_string(), 0xFF00_00FF), ("moss snake".to_string(), 0x8080_8080)]);
        assert_eq!(read(&[0, 0]).unwrap(), vec![], "an empty set: none tinted");
        assert!(read(&m[..m.len() - 1]).is_none(), "the control: one byte short");
        assert!(read(&yhafiran[..5]).is_none(), "a name cut short");
        assert!(read(&[]).is_none());
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
