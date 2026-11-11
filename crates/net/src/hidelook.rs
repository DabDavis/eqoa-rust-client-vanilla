//! go-eqoa-server's own, no retail message: a worn item's look hidden (the
//! server owner, 2026-11-11: "lets add a hide display option to equipped
//! armor/weapon, still equipped, but no model displayed"). The server offers it
//! once the player is in the world with the equip slots its character hides
//! ([`OP_OFFER`]); the item menu's Hide Look and Show Look on a worn item set
//! one ([`set`], [`OP_SET`]) and the server answers the slots after it
//! ([`OP_STATE`]). What is worn in a hidden slot stays worn, its stats and its
//! swing kept; the server sends every client (this one's own figure too, which
//! the game dresses off its channel) the slot as bare. A PS2 never answers and
//! sees the hidden looks as everyone does; on a server that makes no offer
//! nothing is shown or sent.

use crate::stream::{Reader, Writer};

/// The offer, server to client at world entry: a u8 layout ([`LAYOUT`]), then
/// a u32, the hidden equip slots (bit n the equip slot n).
pub const OP_OFFER: u16 = 0x7E6C;
/// A slot's look hidden or shown, client to server: a u8 equip slot, a u8 1
/// to hide, 0 to show.
pub const OP_SET: u16 = 0x7E6D;
/// The server's answer to [`OP_SET`]: a u32, the hidden equip slots after it.
pub const OP_STATE: u16 = 0x7E6E;
/// The offer's layout this client reads.
pub const LAYOUT: u8 = 1;

/// The equip slots whose look the server can hide: what shows on a body, the
/// head (0), the hands (1), the chest (5), both forearms (8), the legs (12),
/// the feet (13), the weapons and held items (14-20), the robe (21) and the
/// cape (22). A ring, an earring or a belt shows nothing to hide.
pub const SHOWN_SLOTS: [i32; 15] = [0, 1, 5, 8, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22];

/// Whether an item worn in `equip_slot` has a look to hide.
pub fn shown(equip_slot: i32) -> bool {
    SHOWN_SLOTS.contains(&equip_slot)
}

/// What the server said of the hidden looks on this connection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// The server offered them (in [`LAYOUT`]).
    pub offered: bool,
    /// The hidden equip slots, a bit each.
    pub hidden: u32,
}

impl State {
    /// Whether the look of what is worn in `equip_slot` is hidden.
    pub fn is_hidden(&self, equip_slot: i32) -> bool {
        (0..32).contains(&equip_slot) && self.hidden & (1 << equip_slot) != 0
    }

    /// Takes [`OP_OFFER`] or [`OP_STATE`] (`op`, its body after the opcode);
    /// whether it was one of them. An offer in another layout, a short one or
    /// a state with no offer is left alone.
    pub fn read(&mut self, op: u16, body: &[u8]) -> bool {
        let mut r = Reader::new(body);
        match op {
            OP_OFFER => {
                let layout = r.u8();
                let hidden = r.u32();
                if !r.error && layout == LAYOUT {
                    *self = State { offered: true, hidden };
                }
                true
            }
            OP_STATE => {
                let hidden = r.u32();
                if !r.error && self.offered {
                    self.hidden = hidden;
                }
                true
            }
            _ => false,
        }
    }
}

/// [`OP_SET`]: the look of what is worn in `equip_slot` hidden or shown.
pub fn set(equip_slot: i32, hide: bool) -> Vec<u8> {
    let mut w = Writer::op(OP_SET);
    w.u8(equip_slot as u8).u8(hide as u8);
    w.data
}

#[cfg(test)]
mod tests {
    use super::*;

    // The offer is taken in its layout with the slots it names, a state only
    // after it; the set is the slot and the choice.
    #[test]
    fn the_offer_and_the_answers() {
        let mut s = State::default();
        assert!(s.read(OP_STATE, &[1, 0, 0, 0]));
        assert_eq!(s, State::default(), "no state before the offer");
        assert!(s.read(OP_OFFER, &[2, 1, 0, 0, 0]));
        assert!(!s.offered, "another layout is not taken");
        assert!(s.read(OP_OFFER, &[1, 0x21, 0, 0]));
        assert!(!s.offered, "a short offer is not taken");
        s.read(OP_OFFER, &[1, 0x21, 0, 0, 0]);
        assert!(s.offered && s.is_hidden(0) && s.is_hidden(5) && !s.is_hidden(14) && !s.is_hidden(-1) && !s.is_hidden(40));
        s.read(OP_STATE, &[0, 0x40, 0, 0]);
        assert!(s.is_hidden(14) && !s.is_hidden(0));
        assert!(!s.read(0x7E60, &[1]));
        assert_eq!(set(5, true), vec![0x6D, 0x7E, 5, 1]);
        assert_eq!(set(22, false), vec![0x6D, 0x7E, 22, 0]);
        assert!(shown(0) && shown(22) && !shown(2) && !shown(9) && !shown(-1));
    }
}
