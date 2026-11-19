//! go-eqoa-server's own, no retail message: Hardcore characters (the server
//! owner, 2026-11-18: "lets add [HC] tag for characters. Hardcore mode chars stay
//! dead and cannot respawn"). Chosen at creation, for good; a Hardcore
//! character's first real death is its last: no respawn, no resurrection, and it
//! stays on the character list as a memorial, fallen, until its player deletes it.
//!
//! - [`crate::session::OP_HARDCORE`]: the server's offer, a u32 (1), once a
//!   session after its first character list, only where the server has Hardcore;
//!   this client answers 1 ([`crate::session::Session::hardcore`]) and from then
//!   on writes [`block`] in every creation and is sent
//!   [`crate::session::OP_HARDCORE_CHARS`] after every list. A PS2 ignores the
//!   offer (its players choose in the world, in the game's question box).
//! - [`crate::session::OP_HARDCORE_CHARS`] ([`read_chars`]): each listed
//!   character's plain name (a PS2's list carries "[HC] " or a fallen mark in
//!   the name itself), whether it is Hardcore, fallen (when, its level and
//!   masteries then, by what and where) or still to choose.
//! - [`crate::session::OP_HARDCORE_FALL`] ([`read_fall`]): the player's own
//!   Hardcore character has just fallen, and in how many seconds the server ends
//!   the connection (0x0777, `Msg0777`: back to the server list).
//!
//! [OG] characters ride the same offer (the server owner, 2026-11-19: "lets make
//! [OG] status a char tag like [HC]"): the offer's u32 is bits, [`OFFER_HARDCORE`]
//! and [`OFFER_OG`] (a server of before sends 1); this client answers the bits it
//! takes, writes [`og_block`] after [`block`] when [OG] was offered, and reads
//! [`OG`] and [`OG_OPEN`] in the list. An [OG] character has the basic upgrades
//! only on that server: no map, minimap or item links, no mouse or touch.

use crate::stream::Reader;

/// [`read_chars`]'s version, its first byte.
pub const VERSION: u8 = 1;
/// A creation's block's first byte, before the race options' 'R'.
pub const MARK: u8 = b'H';
/// The [OG] block's first byte, after the Hardcore block, before 'R'.
pub const OG_MARK: u8 = b'O';

/// The offer's bits: what the server's creation takes.
pub const OFFER_HARDCORE: u32 = 1;
pub const OFFER_OG: u32 = 2;

/// The flags of a character's entry.
pub const HARDCORE: u8 = 1;
pub const FALLEN: u8 = 2;
/// A character made by a client that did not choose (a PS2's): it is asked in
/// the world.
pub const OPEN: u8 = 4;
/// An [OG] character, and one whose [OG] choice is still to be asked in the world.
pub const OG: u8 = 8;
pub const OG_OPEN: u8 = 16;

/// A fallen character's death, as the server took it down at the moment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fall {
    /// Unix seconds, UTC
    pub at: u32,
    pub level: u8,
    pub cms: u16,
    pub killer: String,
    pub place: String,
}

/// One listed character's, by its server id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Char {
    /// its own name, untagged
    pub name: String,
    pub hardcore: bool,
    /// its choice still to make (in the world)
    pub open: bool,
    pub fall: Option<Fall>,
    /// an [OG] character, and its [OG] choice still to make (in the world)
    pub og: bool,
    pub og_open: bool,
}

fn text(r: &mut Reader) -> String {
    let n = r.u8() as usize;
    String::from_utf8_lossy(r.bytes(n)).into_owned()
}

/// [`crate::session::OP_HARDCORE_CHARS`]'s body (after the opcode): u8 version,
/// u16 count, then each u32 server id, u8-counted name, u8 flags and, fallen, u32
/// Unix seconds, u8 level, u16 masteries, u8-counted killer and place. None for
/// another version or a body cut short or longer.
pub fn read_chars(data: &[u8]) -> Option<Vec<(u32, Char)>> {
    let mut r = Reader::new(data);
    if r.u8() != VERSION {
        return None;
    }
    let n = r.u16() as usize;
    let mut out = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        let id = r.u32();
        let name = text(&mut r);
        let flags = r.u8();
        let fall = (flags & FALLEN != 0).then(|| {
            let (at, level, cms) = (r.u32(), r.u8(), r.u16());
            let killer = text(&mut r);
            let place = text(&mut r);
            Fall { at, level, cms, killer, place }
        });
        if r.error {
            return None;
        }
        out.push((id, Char { name, hardcore: flags & HARDCORE != 0, open: flags & OPEN != 0, fall, og: flags & OG != 0, og_open: flags & OG_OPEN != 0 }));
    }
    (!r.error && r.pos == data.len()).then_some(out)
}

/// The fall told its own client ([`crate::session::OP_HARDCORE_FALL`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FallNotice {
    pub at: u32,
    pub killer: String,
    pub place: String,
    /// seconds until the server ends the connection
    pub secs: u16,
}

/// [`crate::session::OP_HARDCORE_FALL`]'s body: u32 Unix seconds, u8-counted
/// killer and place, u16 seconds to the disconnect. None when cut short.
pub fn read_fall(data: &[u8]) -> Option<FallNotice> {
    let mut r = Reader::new(data);
    let at = r.u32();
    let killer = text(&mut r);
    let place = text(&mut r);
    let secs = r.u16();
    (!r.error).then_some(FallNotice { at, killer, place, secs })
}

/// The creation's choice, written after 0x002B's own fields and before the race
/// options' block by a client that took the offer: 'H', version 1, 1 Hardcore or
/// 0 not.
pub fn block(hardcore: bool) -> [u8; 3] {
    [MARK, VERSION, hardcore as u8]
}

/// The creation's [OG] choice, after [`block`] (or 0x002B's own fields) and
/// before the race options' block, by a client the server offered [OG]: 'O',
/// version 1, 1 [OG] or 0 not.
pub fn og_block(og: bool) -> [u8; 3] {
    [OG_MARK, VERSION, og as u8]
}

/// Unix seconds as "2026-11-18 08:45 UTC".
pub fn stamp(at: u32) -> String {
    let secs = at as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // days to the civil date (Howard Hinnant's)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + (m <= 2) as i64;
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rest / 3600, rest % 3600 / 60)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn str8(b: &mut Vec<u8>, s: &str) {
        b.push(s.len() as u8);
        b.extend_from_slice(s.as_bytes());
    }

    /// Two characters as the server writes them: a living Hardcore one and a
    /// fallen one.
    pub fn sample() -> Vec<u8> {
        let mut b = vec![VERSION, 2, 0];
        b.extend_from_slice(&7u32.to_le_bytes());
        str8(&mut b, "Meshuggah");
        b.push(HARDCORE);
        b.extend_from_slice(&9u32.to_le_bytes());
        str8(&mut b, "Brakk");
        b.push(HARDCORE | FALLEN);
        b.extend_from_slice(&1_795_423_500u32.to_le_bytes());
        b.push(52);
        b.extend_from_slice(&140u16.to_le_bytes());
        str8(&mut b, "a Zek sentry");
        str8(&mut b, "Tallon's fortress");
        b
    }

    #[test]
    fn the_chars_are_read() {
        let l = read_chars(&sample()).unwrap();
        assert_eq!(l.len(), 2);
        assert_eq!(l[0], (7, Char { name: "Meshuggah".into(), hardcore: true, open: false, fall: None, og: false, og_open: false }));
        let f = l[1].1.fall.clone().unwrap();
        assert_eq!((l[1].0, l[1].1.name.as_str(), f.level, f.cms, f.killer.as_str(), f.place.as_str()), (9, "Brakk", 52, 140, "a Zek sentry", "Tallon's fortress"));
        assert_eq!(stamp(f.at), "2026-11-23 08:45 UTC");
        let mut other = sample();
        other[0] = 2;
        assert_eq!(read_chars(&other), None, "another version is not read");
        let s = sample();
        assert_eq!(read_chars(&s[..s.len() - 1]), None, "nor one cut short");
        let mut longer = sample();
        longer.push(0);
        assert_eq!(read_chars(&longer), None, "nor one longer");
        assert_eq!(read_chars(&[VERSION, 0, 0]), Some(vec![]), "the control: none");
    }

    #[test]
    fn a_fall_is_read() {
        let mut b = 1_795_423_500u32.to_le_bytes().to_vec();
        str8(&mut b, "a Zek sentry");
        str8(&mut b, "Tallon's fortress");
        b.extend_from_slice(&12u16.to_le_bytes());
        let f = read_fall(&b).unwrap();
        assert_eq!((f.killer.as_str(), f.place.as_str(), f.secs), ("a Zek sentry", "Tallon's fortress", 12));
        assert_eq!(read_fall(&b[..b.len() - 1]), None);
    }

    #[test]
    fn the_stamps() {
        assert_eq!(stamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(stamp(951_782_400), "2000-02-29 00:00 UTC", "a leap day");
        assert_eq!(stamp(1_794_991_500), "2026-11-18 08:45 UTC");
        assert_eq!(block(true), [b'H', 1, 1]);
        assert_eq!(block(false), [b'H', 1, 0]);
        assert_eq!(og_block(true), [b'O', 1, 1]);
        assert_eq!(og_block(false), [b'O', 1, 0]);
    }

    /// [OG]'s flags (8, and 16 its choice open) beside Hardcore's; the control:
    /// a server of before sends neither.
    #[test]
    fn og_flags_are_read() {
        let mut b = vec![VERSION, 2, 0];
        b.extend_from_slice(&7u32.to_le_bytes());
        str8(&mut b, "Meshuggah");
        b.push(HARDCORE | OG);
        b.extend_from_slice(&8u32.to_le_bytes());
        str8(&mut b, "Newbie");
        b.push(OPEN | OG_OPEN);
        let l = read_chars(&b).unwrap();
        assert!(l[0].1.og && l[0].1.hardcore && !l[0].1.og_open);
        assert!(l[1].1.og_open && l[1].1.open && !l[1].1.og);
        let old = read_chars(&sample()).unwrap();
        assert!(old.iter().all(|(_, c)| !c.og && !c.og_open), "the control");
    }
}
