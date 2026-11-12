//! go-eqoa-server's own, no retail message: a ready check (the GM Jibrish's
//! petition 1711, 2026-11-12: "/readycheck ... pops up a window on their screen
//! with "Are you ready?" and Yes or No. It does not stop them from moving while
//! the pop up is up. It expires after 30 seconds"). A raid's leader or helper,
//! or anyone in a plain group, types `/readycheck`; the server asks the
//! group's other players and tells the asker who is not ready.
//!
//! The server offers the window once the player is in the world
//! ([`OP_READY_OFFER`]); a client that shows it answers 1 ([`OP_READY_ANSWER`])
//! and is then sent each check ([`read_check`], [`OP_READY_CHECK`]) and answers
//! one with [`reply`]. A PS2 ignores the offer and is told in a chat line to
//! type `/t ready` or `/t notready`. Every rule is the server's.

use crate::stream::{Reader, Writer};

/// The offer, server to client once the player is in the world: a u8, 1.
pub const OP_READY_OFFER: u16 = 0x7E49;
/// The answer to [`OP_READY_OFFER`], client to server: a u8, 1 to be sent the
/// window, 0 for the chat line a PS2 gets.
pub const OP_READY_ANSWER: u16 = 0x7E4A;
/// A check, server to client: [`read_check`].
pub const OP_READY_CHECK: u16 = 0x7E4B;
/// The window's answer, client to server: [`reply`].
pub const OP_READY_REPLY: u16 = 0x7E56;

/// One check as the server sent it: its id, the seconds left (0: over for
/// this client, its window goes) and who asked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Check {
    pub id: u32,
    pub seconds: u8,
    pub asker: String,
}

/// [`OP_READY_CHECK`]'s body: u32 id, u8 seconds left, the asker's name (u8
/// byte count and its bytes). None when short.
pub fn read_check(data: &[u8]) -> Option<Check> {
    let mut r = Reader::new(data);
    let id = r.u32();
    let seconds = r.u8();
    let n = r.u8() as usize;
    let asker = String::from_utf8_lossy(r.bytes(n)).into_owned();
    (!r.error).then_some(Check { id, seconds, asker })
}

/// [`OP_READY_ANSWER`]: the whole message, its opcode first.
pub fn answer(want: bool) -> Vec<u8> {
    let mut w = Writer::op(OP_READY_ANSWER);
    w.u8(want as u8);
    w.data
}

/// [`OP_READY_REPLY`]: u32 the check's id, u8 1 ready, 0 not; the whole
/// message, its opcode first.
pub fn reply(id: u32, ready: bool) -> Vec<u8> {
    let mut w = Writer::op(OP_READY_REPLY);
    w.u32(id).u8(ready as u8);
    w.data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_read_and_a_reply_written() {
        let body = [7, 0, 0, 0, 30, 3, b'B', b'o', b'b'];
        assert_eq!(read_check(&body), Some(Check { id: 7, seconds: 30, asker: "Bob".into() }));
        assert_eq!(read_check(&[7, 0, 0, 0, 0, 0]), Some(Check { id: 7, seconds: 0, asker: String::new() }));
        assert_eq!(read_check(&body[..7]), None, "a name cut short");
        assert_eq!(reply(7, true), [0x56, 0x7E, 7, 0, 0, 0, 1]);
        assert_eq!(reply(9, false), [0x56, 0x7E, 9, 0, 0, 0, 0]);
        assert_eq!(answer(true), [0x4A, 0x7E, 1]);
    }
}
