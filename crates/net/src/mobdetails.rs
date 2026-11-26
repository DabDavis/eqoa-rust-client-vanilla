//! go-eqoa-server's own: a creature's details and loot chart (Heirloom petition
//! 3163: "view a mob's loot chart: right-click a mob to view its details like
//! stats and loot table"; the server's `world.mob_details`,
//! `handlers/mobdetails.go`). The server offers it at each world entry
//! ([`OP_OFFER`]); a right click on a creature asks ([`ask`]) and the answer
//! ([`OP_DETAILS`], [`read`]) is shown in the client's own window (`eqoa`'s
//! `mobwnd`). A PS2 ignores the offer: CLIENT has no such case.

use crate::stream::{Reader, Writer};

/// The offer, server to client once the player is in the world: a u8, the
/// layout of [`OP_DETAILS`] (1). Answered with nothing.
pub const OP_OFFER: u16 = 0x7EFA;
/// The ask, client to server: a u8 layout (1) and the u32 id the client knows
/// the creature by.
pub const OP_ASK: u16 = 0x7EFB;
/// The answer (the server's `OpcodeMobDetails`): a u8 layout (1), the u32 id
/// asked about, a u8 status ([`Status`]); shown (0), the name (u8-counted
/// ASCII), a u8 level, the class (u8-counted), a u8 consider colour (0x0010's:
/// 0 red .. 5 green), u32 hit points and u32 their most, a u8 count of info
/// lines (u8-counted each), a u16 count of loot rows and a u8 of flags (1 rows
/// left out), each row a u8 source ([`Source`]), a u16 chance a kill in
/// hundredths of a percent, a u32 item record id (0 for a category's row) and
/// its text (u8-counted).
pub const OP_DETAILS: u16 = 0x7EFC;
/// The layout this client reads.
pub const LAYOUT: u8 = 1;

/// An answer's status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Shown,
    /// no creature the client knows by that id, or not one (a player, a
    /// townsperson, a corpse, a pet)
    NotACreature,
    /// asked too often: again in a moment
    TooSoon,
    /// not on this server (off, or not for this character)
    Off,
    Other(u8),
}

impl Status {
    fn of(b: u8) -> Self {
        match b {
            0 => Status::Shown,
            1 => Status::NotACreature,
            2 => Status::TooSoon,
            3 => Status::Off,
            n => Status::Other(n),
        }
    }
}

/// Where a loot row comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// its own items: every kill
    Own,
    /// what it holds
    Held,
    /// its loot tables
    Table,
    /// its region's category (a row names the category and its pool's size)
    Category,
    /// a quest's item
    Quest,
    /// a table every creature rolls
    Every,
    Other(u8),
}

impl Source {
    fn of(b: u8) -> Self {
        match b {
            0 => Source::Own,
            1 => Source::Held,
            2 => Source::Table,
            3 => Source::Category,
            4 => Source::Quest,
            5 => Source::Every,
            n => Source::Other(n),
        }
    }
}

/// One loot row: its source, its chance a kill (hundredths of a percent,
/// 10000 every kill), the item record (0 for a category) and its text.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub source: Source,
    pub chance: u16,
    pub pattern: u32,
    pub text: String,
}

/// A creature's details, as an answer of status 0 carries them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Details {
    pub name: String,
    pub level: u8,
    pub class: String,
    pub con: u8,
    pub hp: u32,
    pub max_hp: u32,
    pub lines: Vec<String>,
    pub rows: Vec<Row>,
    /// the server left rows out past the most it sends
    pub left_out: bool,
}

/// An answer read: the id asked about, its status and, shown, the details.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub id: u32,
    pub status: Status,
    pub details: Option<Details>,
}

/// The ask about the creature the client knows by `id`.
pub fn ask(id: u32) -> Vec<u8> {
    let mut w = Writer::op(OP_ASK);
    w.u8(LAYOUT);
    w.u32(id);
    w.data
}

/// A u8-counted string, as printable ASCII (anything else '?').
fn text(r: &mut Reader) -> String {
    let n = r.u8() as usize;
    r.bytes(n).iter().map(|&c| if (0x20..0x7F).contains(&c) { c as char } else { '?' }).collect()
}

/// An [`OP_DETAILS`] (its bytes after the opcode); none for another layout or
/// a short one.
pub fn read(data: &[u8]) -> Option<Answer> {
    let mut r = Reader::new(data);
    let (layout, id, status) = (r.u8(), r.u32(), Status::of(r.u8()));
    if r.error || layout != LAYOUT {
        return None;
    }
    if status != Status::Shown {
        return Some(Answer { id, status, details: None });
    }
    let mut d = Details { name: text(&mut r), level: r.u8(), ..Default::default() };
    d.class = text(&mut r);
    d.con = r.u8();
    d.hp = r.u32();
    d.max_hp = r.u32();
    for _ in 0..r.u8() {
        d.lines.push(text(&mut r));
    }
    let rows = r.u16();
    d.left_out = r.u8() & 1 != 0;
    for _ in 0..rows {
        let (source, chance, pattern) = (Source::of(r.u8()), r.u16(), r.u32());
        d.rows.push(Row { source, chance, pattern, text: text(&mut r) });
        if r.error {
            return None;
        }
    }
    (!r.error).then_some(Answer { id, status, details: Some(d) })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// go-eqoa-server's TestMobDetailsWire's shape: a snake of level 3, green,
    /// 30 of 40, a line, two rows (a table's fang at 25%, a category); read
    /// back whole. The control: a short answer and another layout are none, a
    /// status past 0 carries nothing.
    #[test]
    fn an_answer_reads_back() {
        let mut b = vec![1u8];
        b.extend_from_slice(&0x186A1u32.to_le_bytes());
        b.push(0);
        let s = |b: &mut Vec<u8>, t: &str| {
            b.push(t.len() as u8);
            b.extend_from_slice(t.as_bytes());
        };
        s(&mut b, "moss snake");
        b.push(3);
        s(&mut b, "Warrior");
        b.push(5);
        b.extend_from_slice(&30u32.to_le_bytes());
        b.extend_from_slice(&40u32.to_le_bytes());
        b.push(1);
        s(&mut b, "a line");
        b.extend_from_slice(&2u16.to_le_bytes());
        b.push(1);
        b.push(2);
        b.extend_from_slice(&2500u16.to_le_bytes());
        b.extend_from_slice(&7u32.to_le_bytes());
        s(&mut b, "fang");
        b.push(3);
        b.extend_from_slice(&3000u16.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        s(&mut b, "gems: one of 12 items");
        let a = read(&b).expect("read");
        assert_eq!((a.id, a.status), (0x186A1, Status::Shown));
        let d = a.details.unwrap();
        assert_eq!((d.name.as_str(), d.level, d.class.as_str(), d.con, d.hp, d.max_hp), ("moss snake", 3, "Warrior", 5, 30, 40));
        assert_eq!(d.lines, ["a line"]);
        assert!(d.left_out);
        assert_eq!(d.rows, [
            Row { source: Source::Table, chance: 2500, pattern: 7, text: "fang".into() },
            Row { source: Source::Category, chance: 3000, pattern: 0, text: "gems: one of 12 items".into() },
        ]);
        assert!(read(&b[..b.len() - 1]).is_none(), "the control: one byte short");
        let mut other = b.clone();
        other[0] = 2;
        assert!(read(&other).is_none(), "another layout");
        assert_eq!(read(&[1, 9, 0, 0, 0, 2]), Some(Answer { id: 9, status: Status::TooSoon, details: None }));
        assert_eq!(ask(0x186A1), vec![0xFB, 0x7E, 1, 0xA1, 0x86, 0x01, 0x00]);
    }
}
