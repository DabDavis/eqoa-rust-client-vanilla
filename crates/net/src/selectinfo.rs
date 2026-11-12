//! go-eqoa-server's own, no retail message: more of each character at select
//! ([`crate::session::OP_SELECT_INFO`]; the server owner, 2026-11-12: "in char sel
//! screen where it says Level 1 / Alchemist, let it be Level 1 / 0 CMs / Ratman /
//! Alchemist", and a Wisp's colour at select's Customize). The character list
//! (0x002C) carries a character's wire race (the game's race a server's own race
//! passes as) and nothing of its masteries; a server that has more offers it after
//! the session's first list, and this client answers 1 and is sent, after every
//! list ([`crate::session::OP_SELECT_INFO_DATA`]), each character's own race, its
//! race's name where the game has no word for it, its mastery points and the race
//! option rows its Customize re-chooses. A server that never offers it (Sandstorm's,
//! eqoa.live's) gets no answer, and select shows the game's two lines.

use crate::stream::Reader;

/// The message's version, its first byte.
pub const VERSION: u8 = 1;

/// One character's, by its server id in the list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    /// its own race (a server's own past the game's ten)
    pub race: u8,
    /// its race's name where the server has its own (empty: one of the game's
    /// races, named in the game's words)
    pub race_name: String,
    /// its mastery points, spent and unspent: what it has earned
    pub cms: u16,
    /// the keys of its race's option rows select's Customize re-chooses
    pub rows: Vec<String>,
}

fn text(r: &mut Reader) -> String {
    let n = r.u8() as usize;
    String::from_utf8_lossy(r.bytes(n)).into_owned()
}

/// The message's body (after the opcode): u8 version, u16 count, then each
/// character's u32 server id, u8 race, u8-counted race name, u16 mastery points,
/// u8 count of row keys and each u8-counted. None for another version or a body
/// cut short or longer.
pub fn read(data: &[u8]) -> Option<Vec<(u32, Info)>> {
    let mut r = Reader::new(data);
    if r.u8() != VERSION {
        return None;
    }
    let n = r.u16() as usize;
    let mut out = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        let id = r.u32();
        let race = r.u8();
        let race_name = text(&mut r);
        let cms = r.u16();
        let k = r.u8() as usize;
        let rows = (0..k).map(|_| text(&mut r)).collect();
        if r.error {
            return None;
        }
        out.push((id, Info { race, race_name, cms, rows }));
    }
    (!r.error && r.pos == data.len()).then_some(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The bytes go-eqoa-server's `encodeSelectInfo` writes (its test's three
    /// characters): a human, a Ratman of 900 points, a Wisp with its colour row.
    pub fn sample() -> Vec<u8> {
        let mut b = vec![VERSION, 3, 0];
        let mut one = |id: u32, race: u8, name: &str, cms: u16, rows: &[&str]| {
            b.extend_from_slice(&id.to_le_bytes());
            b.push(race);
            b.push(name.len() as u8);
            b.extend_from_slice(name.as_bytes());
            b.extend_from_slice(&cms.to_le_bytes());
            b.push(rows.len() as u8);
            for k in rows {
                b.push(k.len() as u8);
                b.extend_from_slice(k.as_bytes());
            }
        };
        one(7, 0, "", 0, &[]);
        one(8, 24, "Ratman", 900, &[]);
        one(9, 31, "Wisp", 0, &["wispcolor"]);
        b
    }

    /// It reads to each character's; the controls: cut short, a byte too many
    /// and another version read to nothing.
    #[test]
    fn reads_each_characters() {
        let b = sample();
        let got = read(&b).expect("read");
        assert_eq!(got.len(), 3);
        assert_eq!(got[0], (7, Info::default()));
        assert_eq!(got[1], (8, Info { race: 24, race_name: "Ratman".into(), cms: 900, rows: vec![] }));
        assert_eq!(got[2], (9, Info { race: 31, race_name: "Wisp".into(), cms: 0, rows: vec!["wispcolor".into()] }));
        assert!(read(&b[..b.len() - 1]).is_none(), "cut short");
        let mut more = b.clone();
        more.push(0);
        assert!(read(&more).is_none(), "a byte too many");
        let mut v2 = b.clone();
        v2[0] = 2;
        assert!(read(&v2).is_none(), "another version");
    }
}
