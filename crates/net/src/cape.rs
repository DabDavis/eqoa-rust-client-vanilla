//! go-eqoa-server's own, no retail message: capes, a wear slot the game has
//! not got ([`crate::session::OP_CAPES`]). A server that turns them on (its
//! `world.capes`, the Heirloom Test Server's) offers them after the first
//! character list; a client that draws capes answers 1, and from then on it
//! is sent a cape item as one of wear slot [`WEAR_SLOT`], worn in equip slot
//! [`EQUIP_SLOT`] (every other client, a PS2's, gets the item as a plain one
//! and never the slot), the capes of the characters in its list
//! ([`crate::session::OP_CAPES_LIST`], by the character's server id) and, in
//! the world, the capes of the players it sees
//! ([`crate::session::OP_CAPES_IN_VIEW`], by the id it knows each by). Each
//! list is the whole of it: one left out wears none.

use crate::stream::Reader;

/// The cape's wear slot (an item's `+0x14`, `itempattern.equipslot`): past
/// CLIENT's 27, so no retail table has it.
pub const WEAR_SLOT: i64 = 28;
/// The equip slot a cape is worn in (`charinventory.equiploc`): past
/// CLIENT's 21.
pub const EQUIP_SLOT: i64 = 22;

/// A cape as worn: its style (which art: `eqoa`'s `cape::STYLES`) and its
/// colour, 0xRRGGBBAA, 0 the art's own.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Cape {
    pub style: u8,
    pub color: u32,
}

/// A list of capes: a u8 count, then each a u32 id, a u8 style and a u32
/// colour, little-endian. None when the message is short; an entry of style
/// 0 (none) is left out.
pub fn read(data: &[u8]) -> Option<Vec<(u32, Cape)>> {
    let mut r = Reader::new(data);
    let n = r.u8() as usize;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let id = r.u32();
        let style = r.u8();
        let color = r.u32();
        if style != 0 {
            out.push((id, Cape { style, color }));
        }
    }
    (!r.error).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: u32, style: u8, color: u32) -> Vec<u8> {
        let mut b = id.to_le_bytes().to_vec();
        b.push(style);
        b.extend_from_slice(&color.to_le_bytes());
        b
    }

    /// A list reads to its entries in order, a style-0 entry left out; a
    /// list cut short reads to nothing (the control), and so does none.
    #[test]
    fn a_list_reads_whole_or_not_at_all() {
        let mut m = vec![3u8];
        m.extend(entry(0x0101_0203, 2, 0));
        m.extend(entry(7, 0, 0xFF00_00FF));
        m.extend(entry(0x7F00_0005, 4, 0x8020_10FF));
        let got = read(&m).unwrap();
        assert_eq!(got, vec![(0x0101_0203, Cape { style: 2, color: 0 }), (0x7F00_0005, Cape { style: 4, color: 0x8020_10FF })]);
        assert_eq!(read(&[0]).unwrap(), vec![], "an empty list: nobody wears one");
        assert!(read(&m[..m.len() - 1]).is_none(), "the control: one byte short");
        assert!(read(&[]).is_none());
    }
}
