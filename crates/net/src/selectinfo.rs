//! go-eqoa-server's own, no retail message: more of each character at select
//! ([`crate::session::OP_SELECT_INFO`]; the server owner, 2026-11-12: "in char sel
//! screen where it says Level 1 / Alchemist, let it be Level 1 / 0 CMs / Ratman /
//! Alchemist", and a Wisp's colour at select's Customize). The character list
//! (0x002C) carries a character's wire race (the game's race a server's own race
//! passes as) and nothing of its masteries; a server that has more offers it after
//! the session's first list, and the client answers with its supported version.
//! The reply ([`crate::session::OP_SELECT_INFO_DATA`]) carries each character's
//! race, mastery points, appearance rows and, in version 2, saved choices.

use crate::stream::Reader;

/// The version that adds the character's saved sex and appearance values.
pub const APPEARANCE_VERSION: u8 = 2;

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
    /// The stored sex, which can differ from the sex inferred from a shared model.
    pub sex: Option<u8>,
    /// Saved custom appearance values by row key, including parts hidden by the current body.
    pub values: Vec<(String, u32)>,
}

fn text(r: &mut Reader) -> String {
    let n = r.u8() as usize;
    String::from_utf8_lossy(r.bytes(n)).into_owned()
}

/// The message's body (after the opcode): u8 version, u16 count, then each
/// character's u32 server id, u8 race, u8-counted race name, u16 mastery points,
/// u8 count of row keys and each u8-counted. Version 2 then has a u8 sex, a u8
/// value count, and each u8-counted key and u32 value.
pub fn read(data: &[u8]) -> Option<Vec<(u32, Info)>> {
    let mut r = Reader::new(data);
    let appearance = match r.u8() {
        1 => false,
        APPEARANCE_VERSION => true,
        _ => return None,
    };
    let n = r.u16() as usize;
    let mut out = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        let id = r.u32();
        let race = r.u8();
        let race_name = text(&mut r);
        let cms = r.u16();
        let k = r.u8() as usize;
        let rows = (0..k).map(|_| text(&mut r)).collect();
        let (sex, values) = if appearance {
            let sex = r.u8();
            if sex > 1 {
                return None;
            }
            let count = r.u8();
            (Some(sex), (0..count).map(|_| (text(&mut r), r.u32())).collect())
        } else {
            (None, Vec::new())
        };
        if r.error {
            return None;
        }
        out.push((id, Info { race, race_name, cms, rows, sex, values }));
    }
    (!r.error && r.pos == data.len()).then_some(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The bytes go-eqoa-server's `encodeSelectInfo` writes (its test's three
    /// characters): a human, a Ratman of 900 points, a Wisp with its colour row.
    pub fn sample() -> Vec<u8> {
        let mut b = vec![1, 3, 0];
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
        assert_eq!(got[1], (8, Info { race: 24, race_name: "Ratman".into(), cms: 900, ..Info::default() }));
        assert_eq!(got[2], (9, Info { race: 31, race_name: "Wisp".into(), rows: vec!["wispcolor".into()], ..Info::default() }));
        assert!(read(&b[..b.len() - 1]).is_none(), "cut short");
        let mut more = b.clone();
        more.push(0);
        assert!(read(&more).is_none(), "a byte too many");
        let mut unknown = b.clone();
        unknown[0] = 3;
        assert!(read(&unknown).is_none(), "another version");
    }

    #[test]
    fn reads_saved_appearance_and_sex() {
        let mut b = vec![APPEARANCE_VERSION, 1, 0];
        b.extend(27u32.to_le_bytes());
        b.extend(b"\x1c\x09Fungusman\x00\x00\x02\x04body\x03bag");
        b.extend(b"\x01\x01\x03bag");
        b.extend(0x0120_998Fu32.to_le_bytes());
        let got = read(&b).unwrap();
        assert_eq!(got, [(27, Info {
            race: 28,
            race_name: "Fungusman".into(),
            rows: vec!["body".into(), "bag".into()],
            sex: Some(1),
            values: vec![("bag".into(), 0x0120_998F)],
            ..Info::default()
        })]);
        assert!(read(&b[..b.len() - 1]).is_none());
    }
}
