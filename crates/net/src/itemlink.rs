//! go-eqoa-server's item links in chat (the GM Jibrish's petition 1463,
//! 2026-11-10: "shift click an item and if your chat is open it puts a link to
//! that item. If people click it, they can view the items stats, name, icon"),
//! on the wire. CLIENT has none of it; the server's own opcodes carry it, and
//! nothing goes to a server that made no offer.
//!
//! - [`OP_ITEM_LINKS`](crate::session::OP_ITEM_LINKS), both ways: the server's
//!   offer at every world entry (a u8, the layout, [`LAYOUT`]), and the client's
//!   answer (a u8, 1 while its Extended row Item Links is on, 0 off: the
//!   server then sends it the plain line, as a PS2 is sent).
//! - [`OP_ITEM_LINK_SAY`](crate::session::OP_ITEM_LINK_SAY), client to server, in
//!   place of the retail line when a typed line holds a link ([`say`]): how it
//!   goes (as 0x000B, a shout's 0x0C01 2, the guild's 0x0578 12), 0x000B's
//!   target, the line as typed with each item's name in its place, then each
//!   link: where its name lies (UTF-16 units) and which of the player's own
//!   records it is (its list and its key, `VIItemList::GetSlotId`, as 0x003E
//!   names one). The server snapshots the record as the player holds it.
//! - [`OP_ITEM_LINK_LINE`](crate::session::OP_ITEM_LINK_LINE), server to client:
//!   a retail chat line (0x0A7A, 0x0A7B, 0x069E, 0x0468 5) with its links: each
//!   one's place in the line's text and the server's link id, then the line's
//!   own message whole ([`read_line`]); the line is read as that message is.
//! - [`OP_ITEM_LINK_ASK`](crate::session::OP_ITEM_LINK_ASK) ([`ask`]) and
//!   [`OP_ITEM_LINK_RECORD`](crate::session::OP_ITEM_LINK_RECORD): a link opened,
//!   and the server's snapshot of its item as an inventory record (0x003B's
//!   body, its key the link id), or none (expired, never made: [`read_record`]).

use crate::dump::NetItem;
use crate::session::{OP_ITEM_LINK_ASK, OP_ITEM_LINK_SAY};
use crate::stream::{Reader, Writer};

/// The layout of [`read_line`]'s and [`read_record`]'s bodies this client
/// reads; an offer of another is not taken.
pub const LAYOUT: u8 = 1;

/// The most links a line holds (the server's `itemLinksPerLine`).
pub const LINKS_MOST: usize = 4;

/// How a typed line with links goes, [`say`]'s first byte: as the retail
/// message it stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// 0x000B: routed by the line (a `/t name` tell, a command), then by the
    /// chat mode the last 0x000E named (say, group)
    Say = 0,
    /// 0x0C01 2: a shout
    Shout = 1,
    /// 0x0578 12: the guild's chat
    Guild = 2,
}

/// One of the player's own items a typed line links: where its name lies in
/// the line (UTF-16 units), the list it is in (0 the pack, worn items among
/// them; 1 the bank) and its record's key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Linked {
    pub start: u16,
    pub len: u16,
    pub list: u8,
    pub key: u32,
}

/// A link in a line the server relayed: where its name lies in the line's text
/// (UTF-16 units) and the server's id for its snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatLink {
    pub start: u16,
    pub len: u16,
    pub id: u32,
}

/// [`OP_ITEM_LINK_SAY`]: the whole message, its opcode first. At most
/// [`LINKS_MOST`] links go; the text is the line as typed, the links' names in it.
pub fn say(route: Route, text: &str, target: u32, links: &[Linked]) -> Vec<u8> {
    let mut w = Writer::op(OP_ITEM_LINK_SAY);
    w.u8(route as u8).u32(target).wide(text);
    let links = &links[..links.len().min(LINKS_MOST)];
    w.u8(links.len() as u8);
    for l in links {
        w.u16(l.start).u16(l.len).u8(l.list).u32(l.key);
    }
    w.data
}

/// [`OP_ITEM_LINK_ASK`]: the whole message.
pub fn ask(id: u32) -> Vec<u8> {
    let mut w = Writer::op(OP_ITEM_LINK_ASK);
    w.u32(id);
    w.data
}

/// [`OP_ITEM_LINK_LINE`]'s body (after its opcode): the links, then the line's
/// own message (its opcode first). None when it is short, holds no link, or
/// wraps another of itself.
pub fn read_line(data: &[u8]) -> Option<(Vec<ChatLink>, &[u8])> {
    let mut r = Reader::new(data);
    let n = r.u8() as usize;
    let links: Vec<ChatLink> = (0..n).map(|_| ChatLink { start: r.u16(), len: r.u16(), id: r.u32() }).collect();
    let inner = &data[r.pos.min(data.len())..];
    if r.error || links.is_empty() || inner.len() < 2 || u16::from_le_bytes([inner[0], inner[1]]) == crate::session::OP_ITEM_LINK_LINE {
        return None;
    }
    Some((links, inner))
}

/// [`OP_ITEM_LINK_RECORD`](crate::session::OP_ITEM_LINK_RECORD)'s body: the link
/// id, and its item when the server still holds it (an inventory record).
/// None when it is short.
pub fn read_record(data: &[u8]) -> Option<(u32, Option<NetItem>)> {
    let mut r = Reader::new(data);
    let id = r.u32();
    let status = r.u8();
    if r.error {
        return None;
    }
    if status == 0 {
        return Some((id, None));
    }
    let item = crate::dump::read_net_item(&mut r);
    (!r.error).then_some((id, Some(item)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A typed line goes as its route, its target, the text and each link's
    /// place, list and key, little-endian; the control: no more than
    /// [`LINKS_MOST`] links go.
    #[test]
    fn a_linked_line_on_the_wire() {
        let l = Linked { start: 8, len: 5, list: 1, key: 0x0102_0304 };
        let m = say(Route::Shout, "look at Sword", 0xFFFF_FFFF, &[l]);
        let mut want = vec![0x50, 0x7E, 1, 0xFF, 0xFF, 0xFF, 0xFF, 13, 0, 0, 0];
        for c in "look at Sword".encode_utf16() {
            want.extend(c.to_le_bytes());
        }
        want.extend([1, 8, 0, 5, 0, 1, 4, 3, 2, 1]);
        assert_eq!(m, want);
        let six = say(Route::Say, "x", 7, &[l; 6]);
        assert_eq!(six[2 + 1 + 4 + 4 + 2], LINKS_MOST as u8, "four links at most");
        assert_eq!(six.len(), 2 + 1 + 4 + 4 + 2 + 1 + LINKS_MOST * 9);
        assert_eq!(ask(0xAABB_CCDD), vec![0x52, 0x7E, 0xDD, 0xCC, 0xBB, 0xAA]);
    }

    /// The server's line: its links, then the retail message whole. The
    /// controls: none, a short one, and one wrapping itself are not read.
    #[test]
    fn a_relayed_line_is_its_links_and_its_message() {
        let mut b = vec![2, 3, 0, 4, 0, 9, 0, 0, 0, 10, 0, 2, 0, 0x34, 0x12, 0, 0];
        b.extend([0x7A, 0x0A, 1, 0, 0, 0, b'a', 0]);
        let (links, inner) = read_line(&b).unwrap();
        assert_eq!(links, vec![ChatLink { start: 3, len: 4, id: 9 }, ChatLink { start: 10, len: 2, id: 0x1234 }]);
        assert_eq!(inner, &[0x7A, 0x0A, 1, 0, 0, 0, b'a', 0]);
        assert!(read_line(&[0, 0x7A, 0x0A]).is_none(), "no link");
        assert!(read_line(&[1, 3, 0, 4]).is_none(), "short");
        assert!(read_line(&[1, 3, 0, 4, 0, 9, 0, 0, 0, 0x51, 0x7E, 0]).is_none(), "itself inside");
    }

    /// A record: the id, 1 and an inventory record; 0 alone is a link gone.
    #[test]
    fn a_links_record() {
        assert_eq!(read_record(&[5, 0, 0, 0, 0]).map(|(id, it)| (id, it.is_none())), Some((5, true)));
        assert!(read_record(&[5, 0, 0]).is_none(), "short");
        let mut w = Writer::default();
        w.u32(77).u8(1);
        // the five zigzags, the key, the item's 23 zigzags, its name, its text, no stats
        for v in [2i64, 30, 0, -1, -1] {
            w.signed(v);
        }
        w.u32(77);
        for _ in 0..23 {
            w.signed(0);
        }
        w.wide("Rusty Sword").wide("old").signed(0);
        let (id, it) = read_record(&w.data).unwrap();
        let it = it.unwrap();
        assert_eq!((id, it.quantity, it.cur_hp, it.inv_index, it.item.name.as_str()), (77, 2, 30, 77, "Rusty Sword"));
        w.data.truncate(w.data.len() - 3);
        assert!(read_record(&w.data).is_none(), "a record cut short");
    }
}
