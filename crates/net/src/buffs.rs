//! go-eqoa-server's own, no retail message: the buff window (the GM Jibrish's
//! petition 1761, 2026-11-12: "Give the buff window 2 horizontal bars for buffs
//! on the player, and a third for buffs on their pet. Allow someone to mouse
//! click off their buffs, and add a way for console to navigate to the buff bar
//! and then use arrow keys to click off buffs, or check their name and
//! remaining duration").
//!
//! CLIENT's effects window is state channel 0x43: at most eight icons and
//! names, no time left, nothing to take one off with. The server offers the
//! window once the player is in the world ([`OP_BUFF_WINDOW`], a u8 1); a client
//! that draws it answers 1 ([`answer`]) and is sent the set ([`read_set`],
//! [`OP_BUFF_WINDOW_DATA`]) at once and whenever it changes, and asks for a line
//! to be taken off with [`remove`] ([`OP_BUFF_REMOVE`]). The server takes off
//! only what is beneficial, tells the player in a chat line either way, and
//! sends the set again. A PS2 ignores the offer and keeps channel 0x43's window.

use crate::stream::{Reader, Writer};

/// The offer and the answer, both ways: a u8 (the server's 1; the client's 1
/// to be sent the set, 0 to stop).
pub const OP_BUFF_WINDOW: u16 = 0x7E57;
/// The set, server to a client that answered 1: [`read_set`].
pub const OP_BUFF_WINDOW_DATA: u16 = 0x7E58;
/// A line to take off, client to server: [`remove`].
pub const OP_BUFF_REMOVE: u16 = 0x7E59;

/// Whose a line is.
pub const ON_PLAYER: u8 = 0;
pub const ON_PET: u8 = 1;

/// A line's time left when the server keeps no end for it.
pub const NO_END: u32 = u32::MAX;

/// One line of the set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    pub whose: u8,
    /// the player may take it off (the server's word: beneficial)
    pub removable: bool,
    /// the effect's spell id, what a removal names with the icon (0: a slot
    /// no effect backs)
    pub id: u32,
    /// its icon's DictID (UI.CSF's 32x32 surfaces, as channel 0x43's)
    pub icon: u32,
    /// what was left of it when the server made the message, in ms
    /// ([`NO_END`]: no end kept)
    pub left_ms: u32,
    pub name: String,
}

/// [`OP_BUFF_WINDOW_DATA`]'s body: a u8 count, then each line's u8 whose, u8
/// flags (bit 0 removable), u32 id, u32 icon, u32 ms left and its name (a u8
/// byte count and UTF-8). The player's lines first, then the pet's. None when
/// short.
pub fn read_set(data: &[u8]) -> Option<Vec<Line>> {
    let mut r = Reader::new(data);
    let n = r.u8() as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let whose = r.u8();
        let flags = r.u8();
        let (id, icon, left_ms) = (r.u32(), r.u32(), r.u32());
        let len = r.u8() as usize;
        let name = String::from_utf8_lossy(r.bytes(len)).into_owned();
        out.push(Line { whose, removable: flags & 1 != 0, id, icon, left_ms, name });
    }
    (!r.error).then_some(out)
}

/// [`OP_BUFF_WINDOW`] from the client: the whole message, its opcode first.
pub fn answer(want: bool) -> Vec<u8> {
    let mut w = Writer::op(OP_BUFF_WINDOW);
    w.u8(want as u8);
    w.data
}

/// [`OP_BUFF_REMOVE`]: u8 whose, u32 id, u32 icon; the whole message, its
/// opcode first.
pub fn remove(l: &Line) -> Vec<u8> {
    let mut w = Writer::op(OP_BUFF_REMOVE);
    w.u8(l.whose).u32(l.id).u32(l.icon);
    w.data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_set_read_and_a_removal_written() {
        // the server's EncodeBuffWindow of two lines: the player's Dark Ward
        // (removable), the pet's Plague (not), byte for byte
        let mut body = vec![2, 0, 1];
        body.extend(9304u32.to_le_bytes());
        body.extend(0xBDD60F04u32.to_le_bytes());
        body.extend(592_400u32.to_le_bytes());
        body.push(9);
        body.extend(b"Dark Ward");
        body.extend([1, 0]);
        body.extend(0u32.to_le_bytes());
        body.extend(77u32.to_le_bytes());
        body.extend(NO_END.to_le_bytes());
        body.push(6);
        body.extend(b"Plague");
        let set = read_set(&body).expect("read");
        assert_eq!(set.len(), 2);
        assert_eq!(set[0], Line { whose: 0, removable: true, id: 9304, icon: 0xBDD60F04, left_ms: 592_400, name: "Dark Ward".into() });
        assert_eq!((set[1].whose, set[1].removable, set[1].left_ms, set[1].name.as_str()), (ON_PET, false, NO_END, "Plague"));
        assert_eq!(read_set(&body[..body.len() - 1]), None, "a name cut short");
        assert_eq!(read_set(&[0]), Some(Vec::new()), "none");
        assert_eq!(remove(&set[0]), [0x59, 0x7E, 0, 0x58, 0x24, 0, 0, 0x04, 0x0F, 0xD6, 0xBD]);
        assert_eq!(answer(true), [0x57, 0x7E, 1]);
        assert_eq!(answer(false), [0x57, 0x7E, 0]);
    }
}
