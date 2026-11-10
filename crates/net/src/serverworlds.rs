//! go-eqoa-server's own, no retail message: the worlds a server has past the
//! disc's, each made of the disc's own zones where they stand on the disc
//! (its `internal/worlds`, `cmd/worldbuild`), told to a client that can build
//! the same file from its player's disc ([`crate::session::OP_SERVER_WORLDS`]).
//! Nothing of the disc comes over the wire: a world is named by its zones (a
//! world file of the disc and a zone index), and its CRC and each zone's are how
//! the client knows it built the server's world. The client says which it holds
//! ([`encode_ready`]), and the server sends it into those alone.

use crate::stream::Reader;

/// The list's version without extras: a world named by its zones.
pub const VERSION: u8 = 1;
/// The list's version with extras: each zone followed by the disc's art the
/// server placed in it (its `internal/worlds/extras.go`), sent only when a
/// world has any. This client reads both.
pub const VERSION_EXTRAS: u8 = 2;

/// A world a server has past the disc's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct World {
    /// Its slot (the world id a MemoryDump and 0x00B2 name).
    pub id: u8,
    /// What players are told it is.
    pub name: String,
    /// The built file's size and CRC-32 (IEEE).
    pub size: u32,
    pub crc: u32,
    /// Its zones, in its file's order.
    pub parts: Vec<Part>,
}

/// A zone of a server's world: zone `zone` of the disc's world file `from`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Part {
    pub from: String,
    pub zone: u16,
    /// Its record on the disc (header and body): size and CRC-32.
    pub size: u32,
    pub crc: u32,
    /// The cell of the world's grid it stands in.
    pub col: u8,
    pub row: u8,
    /// Its name, shown where the game names a zone.
    pub name: String,
    /// The race whose home it is (a server's race by its id, 0 none): where a
    /// new character of it starts, which the create pages name.
    pub race: u8,
    /// The disc's art the server placed in it, each an actor of its own
    /// (none from a version 1 list).
    pub extras: Vec<Extra>,
}

/// A piece of the disc's art placed in a server's zone: a ZoneActor of its
/// own, its art copied into the zone off the player's disc when the zone does
/// not hold it (`eqoa`'s `serverworlds::extras`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Extra {
    /// The DictID of the sprite or model placed.
    pub sprite: u32,
    /// The disc file its records are in when the zone does not register the
    /// sprite itself (empty: the zone's own), and the zone of it when that
    /// file is a world.
    pub from: String,
    pub zone: u16,
    /// Where it stands, its heading, pitch and roll (radians), its scale, and
    /// its static light (RGBA), as its ZoneActor has them.
    pub at: [f32; 3],
    pub hpr: [f32; 3],
    pub scale: f32,
    pub color: [u8; 4],
}

impl Eq for Extra {}

fn str8(r: &mut Reader) -> String {
    let n = r.u8() as usize;
    let b = r.bytes(n);
    String::from_utf8_lossy(&b).into_owned()
}

/// The server's list (the message's body after its opcode); `None` when it
/// is not one this client reads.
pub fn read(b: &[u8]) -> Option<Vec<World>> {
    let mut r = Reader::new(b);
    let version = r.u8();
    if !(version == VERSION || version == VERSION_EXTRAS) || r.error {
        return None;
    }
    let n = r.u8();
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let id = r.u8();
        let name = str8(&mut r);
        let (size, crc) = (r.u32(), r.u32());
        let k = r.u8();
        let mut parts = Vec::with_capacity(k as usize);
        for _ in 0..k {
            let from = str8(&mut r);
            let zone = r.u16();
            let (size, crc) = (r.u32(), r.u32());
            let (col, row) = (r.u8(), r.u8());
            let name = str8(&mut r);
            let race = r.u8();
            let extras = if version >= VERSION_EXTRAS { read_extras(&mut r) } else { Vec::new() };
            parts.push(Part { from, zone, size, crc, col, row, name, race, extras });
        }
        out.push(World { id, name, size, crc, parts });
    }
    (!r.error).then_some(out)
}

/// A zone's extras in a version 2 list: a u16 count, the u8 count of the
/// places their art comes from and each (a file's name, a u16 zone), then
/// each extra (a u8 place, 0xFF the zone's own; the sprite's u32 DictID; seven
/// f32s, where it stands, heading, pitch, roll and scale; RGBA).
fn read_extras(r: &mut Reader) -> Vec<Extra> {
    let n = r.u16();
    let k = r.u8();
    let places: Vec<(String, u16)> = (0..k).map(|_| (str8(r), r.u16())).collect();
    let mut out = Vec::with_capacity(n.min(4096) as usize);
    for _ in 0..n {
        if r.error {
            break;
        }
        let at = r.u8();
        let sprite = r.u32();
        let mut f = [0f32; 7];
        for v in &mut f {
            *v = f32::from_bits(r.u32());
        }
        let color = [r.u8(), r.u8(), r.u8(), r.u8()];
        let (from, zone) = match at {
            0xFF => (String::new(), 0),
            i => match places.get(usize::from(i)) {
                Some(p) => p.clone(),
                None => {
                    r.error = true;
                    break;
                }
            },
        };
        out.push(Extra { sprite, from, zone, at: [f[0], f[1], f[2]], hpr: [f[3], f[4], f[5]], scale: f[6], color });
    }
    out
}

/// The version of [`read_chars`]'s list this client reads.
pub const CHARS_VERSION: u8 = 1;

/// Which of the session's characters a login needs the server's worlds for
/// ([`crate::session::OP_SERVER_WORLDS_CHARS`]'s body after its opcode): a u8
/// version (1), a u8 count, then each character's server id (a u32, as the
/// character list carries it), the whole set each time. `None` when it is not
/// one this client reads (another version, cut short): the session then keeps
/// what it knew, nothing from a server that never said.
pub fn read_chars(b: &[u8]) -> Option<Vec<u32>> {
    let mut r = Reader::new(b);
    if r.u8() != CHARS_VERSION || r.error {
        return None;
    }
    let n = r.u8();
    let ids: Vec<u32> = (0..n).map(|_| r.u32()).collect();
    (!r.error).then_some(ids)
}

/// The answer: the worlds this client holds now, by id (a u8 count, then each).
pub fn encode_ready(ids: &[u8]) -> Vec<u8> {
    let n = ids.len().min(255);
    let mut b = Vec::with_capacity(1 + n);
    b.push(n as u8);
    b.extend_from_slice(&ids[..n]);
    b
}

/// The server's list as it sends it ([`read`] reads it back): a page hands
/// its worker the world to build this way.
pub fn encode(worlds: &[World]) -> Vec<u8> {
    let s8 = |b: &mut Vec<u8>, s: &str| {
        let s = &s.as_bytes()[..s.len().min(255)];
        b.push(s.len() as u8);
        b.extend_from_slice(s);
    };
    let extras = worlds.iter().any(|w| w.parts.iter().any(|p| !p.extras.is_empty()));
    let mut b = vec![if extras { VERSION_EXTRAS } else { VERSION }, worlds.len() as u8];
    for w in worlds {
        b.push(w.id);
        s8(&mut b, &w.name);
        b.extend(w.size.to_le_bytes());
        b.extend(w.crc.to_le_bytes());
        b.push(w.parts.len() as u8);
        for p in &w.parts {
            s8(&mut b, &p.from);
            b.extend(p.zone.to_le_bytes());
            b.extend(p.size.to_le_bytes());
            b.extend(p.crc.to_le_bytes());
            b.push(p.col);
            b.push(p.row);
            s8(&mut b, &p.name);
            b.push(p.race);
            if extras {
                let mut places: Vec<(String, u16)> = Vec::new();
                for e in p.extras.iter().filter(|e| !e.from.is_empty()) {
                    if !places.iter().any(|q| q.0 == e.from && q.1 == e.zone) {
                        places.push((e.from.clone(), e.zone));
                    }
                }
                b.extend((p.extras.len() as u16).to_le_bytes());
                b.push(places.len() as u8);
                for (from, zone) in &places {
                    s8(&mut b, from);
                    b.extend(zone.to_le_bytes());
                }
                for e in &p.extras {
                    b.push(if e.from.is_empty() { 0xFF } else { places.iter().position(|q| q.0 == e.from && q.1 == e.zone).unwrap() as u8 });
                    b.extend(e.sprite.to_le_bytes());
                    for v in e.at.iter().chain(&e.hpr).chain(std::iter::once(&e.scale)) {
                        b.extend(v.to_bits().to_le_bytes());
                    }
                    b.extend(e.color);
                }
            }
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<World> {
        vec![World {
            id: 10,
            name: "the Homelands".into(),
            size: 5_880_303,
            crc: 0xB6D9_4C34,
            parts: vec![Part { from: "TUNARIA.ESF".into(), zone: 115, size: 5_879_151, crc: 0x7257_AE33, col: 4, row: 10, name: "Aviak Village".into(), race: 14, extras: vec![] }],
        }]
    }

    /// The list read back as it was written; every cut of it is refused
    /// whole, and another version is not read. The control: the whole message
    /// reads.
    #[test]
    fn the_list_reads_whole_or_not_at_all() {
        let b = encode(&sample());
        assert_eq!(read(&b), Some(sample()));
        for cut in 0..b.len() {
            assert_eq!(read(&b[..cut]), None, "cut at {cut}");
        }
        let mut v2 = b.clone();
        v2[0] = 2;
        assert_eq!(read(&v2), None);
        assert_eq!(encode_ready(&[10, 12]), vec![2, 10, 12]);
        assert_eq!(encode_ready(&[]), vec![0]);
    }

    /// Which characters need the server's worlds, as go-eqoa-server writes it
    /// (its TestServerWorldsCharsFormat: 630 and 0x01020304): read whole or
    /// not at all, and none is a set too (no character waits), which is not
    /// "never said". The controls: every cut, and another version.
    #[test]
    fn the_characters_read_whole_or_not_at_all() {
        let b = [1, 2, 0x76, 0x02, 0, 0, 4, 3, 2, 1];
        assert_eq!(read_chars(&b), Some(vec![630, 0x0102_0304]));
        assert_eq!(read_chars(&[1, 0]), Some(vec![]));
        for cut in 0..b.len() {
            assert_eq!(read_chars(&b[..cut]), None, "cut at {cut}");
        }
        let mut v2 = b;
        v2[0] = 2;
        assert_eq!(read_chars(&v2), None);
    }

    /// A list with extras (version 2) read back whole, each zone's extras with
    /// their places: the server's bytes for the same (go-eqoa-server's
    /// TestServerWorldsFormatWithExtras: the zone's own, ODUS.ESF zone 22 named
    /// once for two). Every cut is refused, as is an extra naming a place the
    /// zone has not.
    #[test]
    fn a_list_with_extras_reads_whole_or_not_at_all() {
        let mut w = sample();
        let e = |sprite: u32, from: &str, zone: u16, at: [f32; 3], hpr: [f32; 3], scale: f32, color: [u8; 4]| Extra { sprite, from: from.into(), zone, at, hpr, scale, color };
        w[0].parts.push(Part {
            from: "ODUS.ESF".into(),
            zone: 19,
            size: 8_409_752,
            crc: 0x9104_7E6F,
            col: 2,
            row: 3,
            name: "Kerra Isle".into(),
            race: 12,
            extras: vec![
                e(0xF017_8AAC, "ODUS.ESF", 22, [1.0, 2.0, 3.0], [0.5, 0.0, 0.0], 2.0, [0, 0, 0, 255]),
                e(0x194F_A7CB, "", 0, [4.0, 5.0, 6.0], [0.0; 3], 1.0, [9, 8, 7, 6]),
                e(0x5698_614C, "ODUS.ESF", 22, [7.0, 8.0, 9.0], [0.0; 3], 1.0, [0, 0, 0, 255]),
            ],
        });
        let b = encode(&w);
        assert_eq!(b[0], VERSION_EXTRAS);
        assert_eq!(read(&b), Some(w.clone()));
        // the second zone's places: one, named once
        let at = b.windows(8).position(|x| x == b"ODUS.ESF").unwrap();
        let second = b[at + 8..].windows(8).position(|x| x == b"ODUS.ESF").unwrap();
        assert_eq!(&b[at + 8 + second - 4..at + 8 + second], &[3, 0, 1, 8], "three extras, one place");
        for cut in 0..b.len() {
            assert_eq!(read(&b[..cut]), None, "cut at {cut}");
        }
        let mut stray = b.clone();
        let place = b.len() - 37;
        assert_eq!(stray[place], 0, "the last extra's place");
        stray[place] = 1;
        assert_eq!(read(&stray), None, "a place the zone has not");
        assert_eq!(read(&encode(&sample())).map(|w| w[0].parts[0].extras.len()), Some(0), "the control: version 1 has none");
    }
}
