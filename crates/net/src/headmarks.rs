//! go-eqoa-server's own, no retail message: a yellow "!" or "?" over a quest
//! NPC's head (the GM Jibrish's petition 1813, 2026-11-12: "Add the ability for
//! us to add yellow ! and ? over NPCs in game, visible to the player, if they
//! toggle it on or we turn it on by default. The exclamation point over their
//! head should only happen if there's a quest they can log right then, and the
//! question mark if there's a quest they can progress right then such as via a
//! turn in or completion").
//!
//! The server offers the marks once the player is in the world ([`OP_OFFER`],
//! a u8, the layout: 1); a client that draws them answers 1 ([`answer`]) and is
//! sent the marks of the NPCs in its view ([`read`], [`OP_MARKS`]) at once and
//! whenever that set changes; 0 stops them. Every rule (which quests, who may
//! take one, what can be done where) is the server's. A PS2 ignores the offer
//! and shows none.

use crate::stream::{Reader, Writer};

/// The offer, server to client once the player is in the world: a u8, the
/// layout of [`OP_MARKS`] (this client reads 1).
pub const OP_OFFER: u16 = 0x7E61;
/// The answer, client to server: a u8, 1 to be sent the marks, 0 to stop.
pub const OP_ANSWER: u16 = 0x7E62;
/// The marks, server to a client that answered 1: [`read`].
pub const OP_MARKS: u16 = 0x7E63;

/// The layout of [`OP_MARKS`] this client reads.
pub const LAYOUT: u8 = 1;

/// A mark: a quest the player may take from the NPC now.
pub const TAKE: u8 = 1;
/// A mark: a quest's step the player can do at the NPC now (a turn-in).
pub const STEP: u8 = 2;

/// [`OP_MARKS`]'s body: a u16 count, then each NPC's u32 id (the entity's, as
/// the entity channel names it) and u8 mark ([`TAKE`] or [`STEP`]),
/// little-endian. The whole set in view: an NPC not in it has none. None when
/// short; a mark of another value is left out.
pub fn read(data: &[u8]) -> Option<Vec<(u32, u8)>> {
    let mut r = Reader::new(data);
    let n = r.u16() as usize;
    let mut out = Vec::with_capacity(n.min(512));
    for _ in 0..n {
        let id = r.u32();
        let mark = r.u8();
        if r.error {
            break;
        }
        if mark == TAKE || mark == STEP {
            out.push((id, mark));
        }
    }
    (!r.error).then_some(out)
}

/// [`OP_ANSWER`]: the whole message, its opcode first.
pub fn answer(want: bool) -> Vec<u8> {
    let mut w = Writer::op(OP_ANSWER);
    w.u8(want as u8);
    w.data
}

/// What a mark is drawn as.
pub fn sign(mark: u8) -> Option<&'static str> {
    match mark {
        TAKE => Some("!"),
        STEP => Some("?"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A set as go-eqoa-server's `EncodeQuestHeadMarks` writes it (its
    /// TestQuestHeadMarksGoByTheView's bytes: object 100001, "!") reads to its
    /// marks; a mark of another value is left out; a set cut short reads to
    /// nothing (the control); the empty set is none; the answer's bytes.
    #[test]
    fn a_set_read_and_the_answer_written() {
        assert_eq!(read(&[1, 0, 0xA1, 0x86, 0x01, 0x00, 1]), Some(vec![(100_001, TAKE)]));
        let m = [3, 0, 1, 0, 0, 0, 2, 2, 0, 0, 0, 9, 3, 0, 0, 0, 1];
        assert_eq!(read(&m), Some(vec![(1, STEP), (3, TAKE)]), "mark 9 left out");
        assert_eq!(read(&m[..m.len() - 1]), None, "the control: one byte short");
        assert_eq!(read(&[0, 0]), Some(vec![]));
        assert_eq!(read(&[]), None);
        assert_eq!(answer(true), [0x62, 0x7E, 1]);
        assert_eq!(answer(false), [0x62, 0x7E, 0]);
        assert_eq!((sign(TAKE), sign(STEP), sign(0)), (Some("!"), Some("?"), None));
    }
}
