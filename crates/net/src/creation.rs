//! go-eqoa-server's own, no retail message: what character creation takes past
//! the game's own races and classes ([`crate::session::OP_CREATION_RACES`]),
//! told before the world to a client that answered its offer
//! ([`crate::session::OP_CREATION_OFFER`]). A server that opens it (its
//! `world.open_races` and `world.all_classes`) takes the races listed, each
//! one passing on the wire as one of the game's, and every class for every race
//! when the flag says so; the create pages offer them (`pregame::client::create`).
//! A server that never offers it is the game's alone.

use crate::stream::Reader;

/// The one version this client reads.
pub const VERSION: u8 = 1;

/// What creation takes past the game's own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Creation {
    /// Every class for every race.
    pub all_classes: bool,
    /// The races past the game's, by id.
    pub races: Vec<Race>,
}

/// A race the server has and the game does not (or the froglok, race 10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Race {
    /// As a character's record holds it (`characters.race`): what 0x002B sends.
    pub id: i32,
    /// The game's race whose place on the create pages' stage, cameras, music
    /// and starting city it takes (it passes as that race on the wire).
    pub like: i32,
    /// The human city it starts in when `like` is a human's (1 east, 2 west).
    pub city: i32,
    /// STR STA AGI DEX WIS INT CHA
    pub bonus: [i32; 7],
    /// The classes it may be when every class is not open: bit N class N.
    pub class_mask: i32,
    /// Its body by sex: CHAR.ESF DictIDs (0 none: the male's).
    pub body: [u32; 2],
    pub name: String,
    pub short: String,
    pub desc: String,
}

impl Creation {
    /// The race of this id, if the server listed it.
    pub fn race(&self, id: i32) -> Option<&Race> {
        self.races.iter().find(|r| r.id == id)
    }
}

impl Race {
    /// Its body for `sex` (1 female): the female's, else the male's.
    pub fn body_for(&self, sex: i32) -> u32 {
        if sex == 1 && self.body[1] != 0 { self.body[1] } else { self.body[0] }
    }

    /// Whether `class` may be made of it with every class not open.
    pub fn has_class(&self, class: i32) -> bool {
        (0..15).contains(&class) && (self.class_mask >> class) & 1 != 0
    }
}

/// The message's body (after the opcode): u8 version, u8 flags (bit 0 every
/// class for every race), u8 races, then each: u8 id, u8 like, u8 city, seven
/// s8 bonuses, u16 classes, u32 male and female bodies, its name and short name
/// (a u8 byte count and UTF-8 each) and its description (a u16 byte count and
/// UTF-8). None when it is short or of another version.
pub fn read(data: &[u8]) -> Option<Creation> {
    let mut r = Reader::new(data);
    if r.u8() != VERSION {
        return None;
    }
    let flags = r.u8();
    let n = r.u8() as usize;
    let text = |r: &mut Reader, len: usize| String::from_utf8_lossy(r.bytes(len)).into_owned();
    let mut races = Vec::with_capacity(n);
    for _ in 0..n {
        let (id, like, city) = (r.u8() as i32, r.u8() as i32, r.u8() as i32);
        let bonus = std::array::from_fn(|_| r.u8() as i8 as i32);
        let class_mask = r.u16() as i32;
        let body = [r.u32(), r.u32()];
        let len = r.u8() as usize;
        let name = text(&mut r, len);
        let len = r.u8() as usize;
        let short = text(&mut r, len);
        let len = r.u16() as usize;
        let desc = text(&mut r, len);
        races.push(Race { id, like, city, bonus, class_mask, body, name, short, desc });
    }
    (!r.error).then_some(Creation { all_classes: flags & 1 != 0, races })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The server's own bytes for the froglok and a hill giant (go-eqoa-server's
    /// `TestCreationRacesFormat`), every class closed.
    const SERVERS: &[u8] = &[
        1, 0, 2, //
        10, 0, 1, 5, 5, 5, 5, 5, 5, 5, 0x69, 0x7E, 0x0C, 0xFE, 0x1B, 0x14, 0x0C, 0xFE, 0x1B, 0x14, //
        7, b'F', b'r', b'o', b'g', b'l', b'o', b'k', 3, b'F', b'R', b'G', 6, 0, b'F', b'r', b'o', b'g', b's', b'.', //
        11, 9, 0, 25, 20, 0xFB, 0xFB, 0, 0, 0, 0x09, 0x09, 0x2C, 0xEA, 0xB1, 0xFF, 0x2C, 0xEA, 0xB1, 0xFF, //
        10, b'H', b'i', b'l', b'l', b' ', b'G', b'i', b'a', b'n', b't', 3, b'H', b'G', b'T', 0, 0,
    ];

    #[test]
    fn the_servers_races_read() {
        let c = read(SERVERS).expect("read");
        assert!(!c.all_classes);
        assert_eq!(c.races.len(), 2);
        let f = &c.races[0];
        assert_eq!((f.id, f.like, f.city, f.class_mask, f.body), (10, 0, 1, 0x7E69, [0x141B_FE0C, 0x141B_FE0C]));
        assert_eq!((f.name.as_str(), f.short.as_str(), f.desc.as_str()), ("Froglok", "FRG", "Frogs."));
        let g = c.race(11).expect("the giant");
        assert_eq!((g.like, g.bonus, g.class_mask, g.body_for(1)), (9, [25, 20, -5, -5, 0, 0, 0], 0x909, 0xFFB1_EA2C));
        assert!(g.has_class(3) && !g.has_class(13) && !g.has_class(15));
        assert_eq!(g.desc, "");
    }

    /// Every cut of the message is refused, never half read; another version
    /// too; the flag reads (the control: the whole one reads).
    #[test]
    fn a_short_or_other_message_is_none() {
        for n in 0..SERVERS.len() {
            assert_eq!(read(&SERVERS[..n]), None, "cut at {n}");
        }
        let mut other = SERVERS.to_vec();
        other[0] = 2;
        assert_eq!(read(&other), None);
        let mut all = SERVERS.to_vec();
        all[1] = 1;
        assert!(read(&all).expect("read").all_classes);
        assert_eq!(read(&[1, 1, 0]), Some(Creation { all_classes: true, races: vec![] }));
    }

    #[test]
    fn a_female_with_no_body_wears_the_males() {
        let r = Race { id: 12, like: 0, city: 2, bonus: [0; 7], class_mask: 3, body: [7, 0], name: "K".into(), short: "K".into(), desc: String::new() };
        assert_eq!((r.body_for(0), r.body_for(1)), (7, 7));
        let r = Race { body: [7, 9], ..r };
        assert_eq!(r.body_for(1), 9, "the control: its own");
    }
}
