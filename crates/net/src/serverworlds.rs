//! go-eqoa-server's own, no retail message: the worlds a server has past the
//! disc's, each made of the disc's own zones where they stand on the disc
//! (its `internal/worlds`, `cmd/worldbuild`), told to a client that can build
//! the same file from its player's disc ([`crate::session::OP_SERVER_WORLDS`]).
//! Nothing of the disc comes over the wire: a world is named by its zones (a
//! world file of the disc and a zone index), and its CRC and each zone's are how
//! the client knows it built the server's world. The client says which it holds
//! ([`encode_ready`]), and the server sends it into those alone.

use crate::stream::Reader;

/// The one version this client reads.
pub const VERSION: u8 = 1;

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
}

fn str8(r: &mut Reader) -> String {
    let n = r.u8() as usize;
    let b = r.bytes(n);
    String::from_utf8_lossy(&b).into_owned()
}

/// The server's list (the message's body after its opcode); `None` when it
/// is not one this client reads.
pub fn read(b: &[u8]) -> Option<Vec<World>> {
    let mut r = Reader::new(b);
    if r.u8() != VERSION || r.error {
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
            parts.push(Part { from, zone, size, crc, col, row, name, race });
        }
        out.push(World { id, name, size, crc, parts });
    }
    (!r.error).then_some(out)
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
    let mut b = vec![VERSION, worlds.len() as u8];
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
            parts: vec![Part { from: "TUNARIA.ESF".into(), zone: 115, size: 5_879_151, crc: 0x7257_AE33, col: 4, row: 10, name: "Aviak Village".into(), race: 14 }],
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
}
