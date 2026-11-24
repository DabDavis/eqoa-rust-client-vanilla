//! go-eqoa-server's own, no retail message: the real offence and defence
//! figures for the Stat Window (petition 2925: "Would it be possible to also
//! include OFF MODS, DEF MODS"). The stat channel (0x42) carries neither; the
//! server's rolls are its own, so it sends what they use, as its GMs' `#stats`
//! states them.
//!
//! The server offers them once the player is in the world ([`OP_OFFER`], a u8,
//! the layout, 1), except where Character > Summary's stat caps are the
//! game's (the Stat Window is off there too); a client answers 1 while its
//! Stat Window is open ([`answer`]) and 0 when it closes, and is sent the
//! figures ([`read`], [`OP_FIGURES`]) at once and whenever they change. A PS2
//! ignores the offer.

use crate::stream::{Reader, Writer};

/// The offer and the answer, both ways: a u8 (the server's layout, 1; the
/// client's 1 to be sent the figures, 0 to stop).
pub const OP_OFFER: u16 = 0x7E84;
/// The figures, server to a client that answered 1: [`read`].
pub const OP_FIGURES: u16 = 0x7E85;
/// The layout this client reads.
pub const LAYOUT: u8 = 1;

/// The figures' ids, each a hit's damage on a creature of the player's own
/// level: its melee hit (main hand) low and high, its ranged weapon's arrow
/// (instead, while one is worn), and such a creature's hit on it after its AC
/// and AGI.
pub const MELEE_LOW: u8 = 1;
pub const MELEE_HIGH: u8 = 2;
pub const RANGED_LOW: u8 = 3;
pub const RANGED_HIGH: u8 = 4;
pub const DEFENCE_LOW: u8 = 5;
pub const DEFENCE_HIGH: u8 = 6;

/// The figures as the client keeps them: each a range (low, high), none when
/// the server did not send it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Figures {
    pub melee: Option<[i32; 2]>,
    pub ranged: Option<[i32; 2]>,
    pub defence: Option<[i32; 2]>,
}

impl Figures {
    /// Whether any came.
    pub fn any(&self) -> bool {
        self.melee.is_some() || self.ranged.is_some() || self.defence.is_some()
    }
}

/// [`OP_FIGURES`]'s body: a u8 count, then each a u8 id and an i32 (LE). An id
/// not known is skipped; a range is kept only with both its ends. None when
/// short.
pub fn read(data: &[u8]) -> Option<Figures> {
    let mut r = Reader::new(data);
    let n = r.u8() as usize;
    let mut v = [None::<i32>; 7];
    for _ in 0..n {
        let id = r.u8() as usize;
        let value = r.u32() as i32;
        if let Some(slot) = v.get_mut(id) {
            *slot = Some(value);
        }
    }
    if r.error {
        return None;
    }
    let pair = |lo: u8, hi: u8| Some([v[lo as usize]?, v[hi as usize]?]);
    Some(Figures { melee: pair(MELEE_LOW, MELEE_HIGH), ranged: pair(RANGED_LOW, RANGED_HIGH), defence: pair(DEFENCE_LOW, DEFENCE_HIGH) })
}

/// [`OP_OFFER`] from the client: the whole message, its opcode first.
pub fn answer(want: bool) -> Vec<u8> {
    let mut w = Writer::op(OP_OFFER);
    w.u8(want as u8);
    w.data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(pairs: &[(u8, i32)]) -> Vec<u8> {
        let mut b = vec![pairs.len() as u8];
        for &(id, v) in pairs {
            b.push(id);
            b.extend(v.to_le_bytes());
        }
        b
    }

    /// The server's EncodeStatFigures, byte for byte: melee and defence; a
    /// ranged set; an id not known skipped; half a range dropped; short, none.
    #[test]
    fn figures_read_and_the_answer_written() {
        let f = read(&body(&[(1, 12), (2, 31), (5, 4), (6, 17)])).expect("read");
        assert_eq!(f, Figures { melee: Some([12, 31]), ranged: None, defence: Some([4, 17]) });
        assert!(f.any());
        let f = read(&body(&[(3, 9), (4, 22), (5, 1), (6, 2)])).unwrap();
        assert_eq!((f.melee, f.ranged), (None, Some([9, 22])));
        let f = read(&body(&[(1, 5), (2, 6), (99, 7), (0, 1)])).unwrap();
        assert_eq!(f, Figures { melee: Some([5, 6]), ..Default::default() }, "an unknown id skipped");
        assert_eq!(read(&body(&[(1, 5), (5, 3), (6, -1)])).unwrap().melee, None, "no high: no range");
        assert_eq!(read(&body(&[(5, 3), (6, -1)])).unwrap().defence, Some([3, -1]), "signed");
        let b = body(&[(1, 5), (2, 6)]);
        assert_eq!(read(&b[..b.len() - 1]), None, "cut short");
        assert_eq!(read(&[0]), Some(Figures::default()));
        assert!(!Figures::default().any());
        assert_eq!(answer(true), [0x84, 0x7E, 1]);
        assert_eq!(answer(false), [0x84, 0x7E, 0]);
    }
}
