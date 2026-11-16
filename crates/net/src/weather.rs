//! go-eqoa-server's own, no retail message: the regional weather's timelines
//! ([`crate::session::OP_WEATHER_DATA`], 0x7E91; board entries #1549 and
//! #1550). The server picks each 500x500 square's weather from the disc's
//! terrain profile under it (snow on the icy profiles, sand on the sandy,
//! rain elsewhere, none where the Lavastorm's fireballs fall) and runs it on a
//! clock; a GM may call any weather anywhere. Rain and snow still reach every
//! client as the retail bytes (0x00C5/0x00C7); this carries what a PS2 cannot
//! draw (sandstorms, whiteouts) to a client that asked
//! ([`crate::session::OP_WEATHER`]).
//!
//! Little-endian: a u8 zone count (at most [`MAX_ZONES`]: the player's, then
//! any other within 100 units of it), each: u8 world, u16 col, u16 row (the
//! 2000x2000 zone col*2000.., row*2000.. on x and z), u8 timelines (at most
//! [`MAX_TIMELINES`]), each: u16 squares (bit (sz%4)*4 + (sx%4) for each
//! 500x500 square it covers, sx = floor(x/500), sz = floor(z/500); 0xFFFF a
//! GM's override), u32 revision, u32 seed, f32 wind x, f32 wind z, u8 n (2 to
//! 12), then n phases: i32 at (ms from this message's receipt; negative is
//! past), u8 kind ([`CLEAR`]..[`WHITEOUT`]), u8 strength (0-255 for 0 to 1).
//! A square in no timeline is clear. The whole message is refused on any bad
//! field: phases not 2 to 12, times not increasing, a last phase not clear, a
//! wind over 50, a kind past 4, two timelines sharing a square.

use crate::stream::Reader;

pub const CLEAR: u8 = 0;
pub const RAIN: u8 = 1;
pub const SNOW: u8 = 2;
pub const SAND: u8 = 3;
pub const WHITEOUT: u8 = 4;

pub const MAX_ZONES: usize = 4;
pub const MAX_TIMELINES: usize = 3;
/// The most a wind may blow, units a second (either way, as a vector).
pub const MAX_WIND: f32 = 50.0;

/// One step of a timeline: at `at_ms` (from the message's receipt) the
/// weather is `kind` at `strength` of 255; between two the weather eases from
/// one to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Phase {
    pub at_ms: i32,
    pub kind: u8,
    pub strength: u8,
}

/// The weather of some of a zone's squares (`squares`, a bit a square).
#[derive(Clone, Debug, PartialEq)]
pub struct Timeline {
    pub squares: u16,
    pub revision: u32,
    pub seed: u32,
    pub wind: [f32; 2],
    pub phases: Vec<Phase>,
}

/// A 2000x2000 zone's timelines.
#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub world: u8,
    pub col: u16,
    pub row: u16,
    pub timelines: Vec<Timeline>,
}

/// The square's bit in its zone's masks: (sz%4)*4 + (sx%4).
pub fn square_bit(sx: i32, sz: i32) -> u16 {
    1 << (sz.rem_euclid(4) * 4 + sx.rem_euclid(4))
}

/// The message's zones (after the opcode), or None for any bad field.
pub fn read(d: &[u8]) -> Option<Vec<Zone>> {
    let mut r = Reader::new(d);
    let nz = r.u8() as usize;
    if r.error || nz > MAX_ZONES {
        return None;
    }
    let mut zones = Vec::with_capacity(nz);
    for _ in 0..nz {
        let (world, col, row) = (r.u8(), r.u16(), r.u16());
        let nt = r.u8() as usize;
        if r.error || nt > MAX_TIMELINES {
            return None;
        }
        let mut timelines: Vec<Timeline> = Vec::with_capacity(nt);
        let mut taken = 0u16;
        for _ in 0..nt {
            let squares = r.u16();
            let (revision, seed) = (r.u32(), r.u32());
            let wind = [r.f32(), r.f32()];
            let n = r.u8() as usize;
            if r.error || !(2..=12).contains(&n) || squares & taken != 0 {
                return None;
            }
            taken |= squares;
            if !(wind[0].is_finite() && wind[1].is_finite()) || wind[0].hypot(wind[1]) > MAX_WIND {
                return None;
            }
            let mut phases = Vec::with_capacity(n);
            for _ in 0..n {
                let p = Phase { at_ms: r.u32() as i32, kind: r.u8(), strength: r.u8() };
                if r.error || p.kind > WHITEOUT || phases.last().is_some_and(|q: &Phase| q.at_ms >= p.at_ms) {
                    return None;
                }
                phases.push(p);
            }
            if phases.last().is_none_or(|p| p.kind != CLEAR) {
                return None;
            }
            timelines.push(Timeline { squares, revision, seed, wind, phases });
        }
        zones.push(Zone { world, col, row, timelines });
    }
    Some(zones)
}

/// The message's body for `zones` (the server's layout; the tests' and a
/// preview's).
pub fn write(zones: &[Zone]) -> Vec<u8> {
    let mut out = vec![zones.len() as u8];
    for z in zones {
        out.push(z.world);
        out.extend_from_slice(&z.col.to_le_bytes());
        out.extend_from_slice(&z.row.to_le_bytes());
        out.push(z.timelines.len() as u8);
        for t in &z.timelines {
            out.extend_from_slice(&t.squares.to_le_bytes());
            out.extend_from_slice(&t.revision.to_le_bytes());
            out.extend_from_slice(&t.seed.to_le_bytes());
            out.extend_from_slice(&t.wind[0].to_le_bytes());
            out.extend_from_slice(&t.wind[1].to_le_bytes());
            out.push(t.phases.len() as u8);
            for p in &t.phases {
                out.extend_from_slice(&p.at_ms.to_le_bytes());
                out.push(p.kind);
                out.push(p.strength);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone() -> Zone {
        Zone {
            world: 0,
            col: 3,
            row: 12,
            timelines: vec![
                Timeline {
                    squares: 0x00FF,
                    revision: 7,
                    seed: 99,
                    wind: [4.0, -3.0],
                    phases: vec![
                        Phase { at_ms: -60_000, kind: CLEAR, strength: 0 },
                        Phase { at_ms: -30_000, kind: SAND, strength: 38 },
                        Phase { at_ms: 30_000, kind: SAND, strength: 255 },
                        Phase { at_ms: 90_000, kind: CLEAR, strength: 0 },
                    ],
                },
                Timeline {
                    squares: 0x0F00,
                    revision: 8,
                    seed: 5,
                    wind: [0.0, 2.0],
                    phases: vec![Phase { at_ms: 0, kind: RAIN, strength: 128 }, Phase { at_ms: 1000, kind: CLEAR, strength: 0 }],
                },
            ],
        }
    }

    /// A good message reads back as written; with no zones too.
    #[test]
    fn reads_what_was_written() {
        let z = vec![zone(), Zone { world: 1, col: 0, row: 0, timelines: Vec::new() }];
        assert_eq!(read(&write(&z)), Some(z));
        assert_eq!(read(&[0]), Some(Vec::new()));
    }

    /// Cut anywhere short of its end, it is refused (the control: whole, it
    /// reads).
    #[test]
    fn a_truncated_message_is_refused() {
        let b = write(&[zone()]);
        assert!(read(&b).is_some(), "the control");
        for n in 0..b.len() {
            assert_eq!(read(&b[..n]), None, "cut at {n}");
        }
    }

    /// Each bad field refuses the whole message.
    #[test]
    fn bad_fields_are_refused() {
        let bad = |f: &dyn Fn(&mut Zone)| {
            let mut z = zone();
            f(&mut z);
            read(&write(&[z]))
        };
        assert!(bad(&|_| {}).is_some(), "the control");
        assert_eq!(bad(&|z| z.timelines[0].phases[1].kind = 5), None, "a kind past whiteout");
        assert_eq!(bad(&|z| z.timelines[0].phases[2].at_ms = -30_000), None, "a time not after the last");
        assert_eq!(bad(&|z| z.timelines[0].phases.swap(1, 2)), None, "times out of order");
        assert_eq!(bad(&|z| z.timelines[0].phases[3].kind = SAND), None, "the last phase not clear");
        assert_eq!(bad(&|z| z.timelines[1].squares = 0x0180), None, "two timelines sharing a square");
        assert_eq!(bad(&|z| z.timelines[0].wind = [40.0, 40.0]), None, "a wind over 50");
        assert_eq!(bad(&|z| z.timelines[0].wind = [f32::NAN, 0.0]), None, "a wind not a number");
        assert_eq!(bad(&|z| z.timelines[1].phases.truncate(1)), None, "one phase");
        assert_eq!(bad(&|z| z.timelines[1].phases = (0..13).map(|k| Phase { at_ms: k, kind: CLEAR, strength: 0 }).collect()), None, "13 phases");
        assert_eq!(bad(&|z| z.timelines.extend(std::iter::repeat_n(Timeline { squares: 0, ..z.timelines[1].clone() }, 2))), None, "four timelines");
        assert_eq!(read(&write(&vec![zone(); 5])), None, "five zones");
    }

    /// A square's bit: its place in its zone's 4x4.
    #[test]
    fn square_bits() {
        assert_eq!(square_bit(0, 0), 1);
        assert_eq!(square_bit(5, 0), 2);
        assert_eq!(square_bit(4, 7), 1 << 12);
        assert_eq!(square_bit(-1, -1), 1 << 15);
    }
}
