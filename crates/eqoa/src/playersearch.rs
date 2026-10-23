//! A server's player search, as Sandstorm's S2 adds it (`s2`): three more
//! entries in the Group menu, Search (word 1673), Register (1815) and
//! Unregister (1816), whose messages (0xFF000001-3) S2's own code takes.
//! Register and Unregister are bare opcodes, 0x1503 and 0x1504 (list or
//! unlist this character in others' searches); Search is 0x1502 with four
//! bytes, race, class, minimum and maximum level (0xFF any), and the server
//! answers 0x1501 (no body: the list emptied) then a 0x1500 a player found.
//! S2 shows them in a window of its own (a title, a filter row, sortable
//! columns, a player's Location and Group lines, Tell and Invite); here the
//! game's who window shows them (`wholist`: name, race, class and level,
//! Tell and Invite by the player's id), the search asked for any race and
//! class from level 1 to 60, as S2's window opens.

use crate::wholist::WhoRow;
use net::stream::Writer;

/// One player found: 0x1500's body as S2's code reads it in place (at least
/// 3 bytes, each later field only when the body reaches it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Found {
    /// the low nibble indexes HUM .. OGR (10-15 print "?")
    pub race: u8,
    /// the low nibble indexes WAR .. ALC (15 prints "?")
    pub class: u8,
    pub level: u8,
    /// 12 bytes, to a NUL
    pub name: String,
    /// the player's id, what Invite names (19 bytes or more)
    pub id: Option<u32>,
    /// world, grid column and row (22 bytes or more: the Location line)
    pub place: Option<(u8, u8, u8)>,
    /// the group's size, 0 not grouped; 0xFF when absent
    pub group: u8,
    /// the player's note, up to 71 bytes, with no NUL check
    pub note: String,
}

impl Found {
    pub fn parse(data: &[u8]) -> Option<Found> {
        if data.len() < 3 {
            return None;
        }
        let mut f = Found { race: data[0], class: data[1], level: data[2], group: 0xFF, ..Default::default() };
        if data.len() > 3 {
            let n = &data[3..data.len().min(15)];
            let end = n.iter().position(|&b| b == 0).unwrap_or(n.len());
            f.name = String::from_utf8_lossy(&n[..end]).into_owned();
        }
        if data.len() >= 19 {
            f.id = Some(u32::from_le_bytes(data[15..19].try_into().unwrap()));
        }
        if data.len() >= 22 {
            f.place = Some((data[19], data[20], data[21]));
        }
        if data.len() >= 23 {
            f.group = data[22];
        }
        if data.len() > 23 {
            f.note = String::from_utf8_lossy(&data[23..data.len().min(23 + 71)]).into_owned();
        }
        Some(f)
    }

    /// As a row of the who window, which shows the results.
    pub fn row(&self) -> WhoRow {
        WhoRow { race: self.race & 15, class: self.class & 15, level: self.level, name: self.name.clone(), x1c: self.id.unwrap_or(0) }
    }
}

/// The search: 0x1502, race, class, minimum and maximum level (0xFF any).
pub fn search(race: u8, class: u8, min_level: u8, max_level: u8) -> Vec<u8> {
    let mut w = Writer::op(0x1502);
    w.u8(race).u8(class).u8(min_level).u8(max_level);
    w.data
}

/// What the Search entry asks: any race and class, levels 1 to 60, as S2's
/// window opens.
pub fn search_all() -> Vec<u8> {
    search(0xFF, 0xFF, 1, 60)
}

/// Register: this character in others' searches.
pub fn register() -> Vec<u8> {
    Writer::op(0x1503).data
}

/// Unregister.
pub fn unregister() -> Vec<u8> {
    Writer::op(0x1504).data
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0x1500 as Sandstorm sent it live (23 bytes: every field, no note), a
    /// shorter one that stops before the id, and a note; less than 3 bytes
    /// is nothing (the control).
    #[test]
    fn a_player_found() {
        let mut b = vec![0x13, 0x0C, 49];
        b.extend(b"Dolph\0\0\0\0\0\0\0");
        b.extend(0x0001_5C0Cu32.to_le_bytes());
        b.extend([0, 7, 9, 4]);
        let f = Found::parse(&b).unwrap();
        assert_eq!((f.race, f.class, f.level, f.name.as_str()), (0x13, 0x0C, 49, "Dolph"));
        assert_eq!((f.id, f.place, f.group), (Some(0x0001_5C0C), Some((0, 7, 9)), 4));
        let row = f.row();
        assert_eq!((row.race, row.class, row.level, row.name.as_str(), row.x1c), (3, 12, 49, "Dolph", 0x0001_5C0C));
        let short = Found::parse(&b[..10]).unwrap();
        assert_eq!((short.name.as_str(), short.id, short.place, short.group), ("Dolph", None, None, 0xFF));
        let mut noted = b.clone();
        noted.extend(b"lfg wiz");
        assert_eq!(Found::parse(&noted).unwrap().note, "lfg wiz");
        assert_eq!(Found::parse(&[1, 2]), None);
    }

    #[test]
    fn the_requests() {
        assert_eq!(search_all(), vec![0x02, 0x15, 0xFF, 0xFF, 1, 60]);
        assert_eq!(register(), vec![0x03, 0x15]);
        assert_eq!(unregister(), vec![0x04, 0x15]);
    }
}
