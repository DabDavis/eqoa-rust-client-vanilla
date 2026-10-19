//! The 0x40 encoder against the console: every self-state the PS2 sent in
//! pcaps/ps2_sandstorm.pcap (full values, the XOR deltas undone through
//! their references), each field turned back into a value inside its
//! quantisation bucket and re-encoded by `vi::selfstate::encode`, must give
//! the PS2's own 41 bytes. That holds the field order, widths, bounds, byte
//! order and rounding to the console. One limit: a 3-byte position bucket
//! is narrower than an f32 ulp at Freeport's coordinates, so for those axes
//! the value is found by searching floats with the encoder itself, which
//! shows only that one exists. SKIPs without the capture.
// The decomp's own literal (WriteQuantized's target-heading range).
#![allow(clippy::field_reassign_with_default, clippy::needless_range_loop, clippy::excessive_precision)]

use std::collections::BTreeMap;
use vi::fpu::{F, Ieee};
use vi::vect::Vect3;
use vi::walk::WalkPhysics;

fn read_pcap(path: &std::path::Path) -> Option<Vec<(u16, u16, Vec<u8>)>> {
    let d = std::fs::read(path).ok()?;
    let mut p = 24;
    let mut out = Vec::new();
    while p + 16 <= d.len() {
        let cap = u32::from_le_bytes(d[p + 8..p + 12].try_into().unwrap()) as usize;
        let b = &d[p + 16..p + 16 + cap];
        p += 16 + cap;
        if b.len() < 42 || b[12..14] != [8, 0] || b[23] != 17 {
            continue;
        }
        let ip = &b[14..];
        let udp = &ip[((ip[0] & 15) * 4) as usize..];
        let len = u16::from_be_bytes([udp[4], udp[5]]) as usize;
        out.push((u16::from_be_bytes([udp[0], udp[1]]), u16::from_be_bytes([udp[2], udp[3]]), udp[8..len.min(udp.len())].to_vec()));
    }
    Some(out)
}

/// Every full 0x40 value the client sent, in order of first appearance.
fn client_states(cap: &[(u16, u16, Vec<u8>)]) -> Vec<Vec<u8>> {
    use drdp::buffer::{Buffer, rle_decode};
    let mut hist: BTreeMap<u16, Vec<u8>> = BTreeMap::new();
    let mut out = Vec::new();
    for (_, dport, payload) in cap {
        if *dport != 10070 {
            continue;
        }
        let mut b = Buffer::reader(&payload[..payload.len() - 4]);
        b.u16();
        b.u16();
        while b.remaining() > 0 && !b.overflow {
            let h = drdp::read_segment_header(&mut b, 0, 0);
            let mut s = Buffer::reader(&b.rest()[..(h.size as usize).min(b.rest().len())]);
            b.skip(h.size as usize);
            if h.flags & 0x30000 != 0 || s.max == 0 {
                continue;
            }
            let f = s.u8();
            if f & 0x40 != 0 {
                s.u32();
            }
            s.u16();
            if f & 1 != 0 {
                s.u16();
                if f & 4 != 0 {
                    s.packed();
                }
            }
            if f & 2 != 0 {
                s.u16();
                if f & 8 != 0 {
                    s.packed();
                }
            }
            if f & 0x10 != 0 {
                while s.u8() != 248 && !s.overflow {
                    s.u16();
                }
            }
            while s.remaining() > 0 && !s.overflow {
                let ty = s.u8();
                let size = s.size() as usize;
                if ty.wrapping_add(7) < 3 {
                    s.u16();
                    s.skip(size);
                } else if ty == 252 {
                    s.skip(size);
                } else if ty < 248 {
                    let seq = s.u16();
                    let delta = s.u8();
                    let mut d = vec![0; size];
                    rle_decode(&mut s, &mut d);
                    if ty != 0x40 {
                        continue;
                    }
                    if delta != 0
                        && let Some(r) = hist.get(&seq.wrapping_sub(delta as u16))
                    {
                        for (x, y) in d.iter_mut().zip(r) {
                            *x ^= y;
                        }
                    }
                    if let std::collections::btree_map::Entry::Vacant(e) = hist.entry(seq) {
                        e.insert(d.clone());
                        out.push(d);
                    }
                }
            }
        }
    }
    out
}

/// The value at the middle of bucket `q` of `n` bytes over [lo, hi].
fn centre(q: u32, n: u32, lo: f32, hi: f32) -> f32 {
    let scale = (1u64 << (n * 8)) as f64;
    (lo as f64 + (q as f64 + 0.5) * (hi as f64 - lo as f64) / scale) as f32
}

/// A float that quantises to bucket `q`: the midpoint, or the nearest float
/// to it that does (a 3-byte bucket at 25,000 is narrower than an f32 ulp
/// there, so the midpoint itself can fall in the neighbour).
fn in_bucket(q: u32, n: u32, lo: f32, hi: f32, want: &[u8]) -> f32 {
    let c = centre(q, n, lo, hi);
    for k in 0..64i32 {
        for d in [k, -k] {
            let v = f32::from_bits((c.to_bits() as i32 + if c < 0.0 { -d } else { d }) as u32);
            let mut out = Vec::new();
            vi::selfstate::write_quantized::<Ieee>(&mut out, n, F::new(v), lo, hi);
            if out == want {
                return v;
            }
        }
    }
    c
}

fn take(b: &[u8], at: &mut usize, n: usize) -> u32 {
    let v = b[*at..*at + n].iter().fold(0u32, |a, &x| (a << 8) | x as u32);
    *at += n;
    v
}

fn decode(b: &[u8]) -> (u8, WalkPhysics<Ieee>, u8, u32) {
    use vi::selfstate::*;
    let mut at = 1;
    let vect = |n: u32, bx: ([f32; 3], [f32; 3]), at: &mut usize| {
        let mut v = [0f32; 3];
        for i in 0..3 {
            if bx.0[i] != bx.1[i] {
                let start = *at;
                let q = take(b, at, n as usize);
                v[i] = in_bucket(q, n, bx.0[i], bx.1[i], &b[start..*at]);
            }
        }
        Vect3::<Ieee>::of(v[0], v[1], v[2])
    };
    let mut p = WalkPhysics::<Ieee>::default();
    p.position = vect(3, POS, &mut at);
    p.velocity = vect(2, VEL, &mut at);
    p.force = vect(2, ACCEL, &mut at);
    p.angles = vect(1, HPR, &mut at);
    p.angular_vel = vect(1, AVEL, &mut at);
    p.torque = vect(1, AACCEL, &mut at);
    p.target_heading = F::new(centre(take(b, &mut at, 1), 1, -3.14159265, 3.14159265));
    let above = centre(take(b, &mut at, 2), 2, -2000.0, 2000.0);
    p.floor = F::new((p.position.y.0 as f64 - above as f64) as f32);
    p.flags = u32::from_le_bytes(b[at..at + 4].try_into().unwrap());
    p.loco_state = b[at + 4] as i32;
    (b[0], p, b[at + 5], u32::from_le_bytes(b[at + 6..at + 10].try_into().unwrap()))
}

#[test]
fn the_ps2s_own_self_states_re_encode() {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../pcaps/ps2_sandstorm.pcap");
    let Some(cap) = read_pcap(&path) else {
        eprintln!("SKIP: no capture at {}", path.display());
        return;
    };
    let states = client_states(&cap);
    assert!(states.len() > 50, "only {} self-states in the capture", states.len());
    let mut bad = 0;
    for s in &states {
        assert_eq!(s.len(), 41, "a self-state is 41 bytes");
        let (world, p, aa, target) = decode(s);
        let again = vi::selfstate::encode(world, &p, aa, target);
        if &again != s {
            if bad < 3 {
                eprintln!("PS2  {s:02x?}\nrust {again:02x?}");
            }
            bad += 1;
        }
    }
    eprintln!("{} self-states, {} re-encode differently", states.len(), bad);
    assert_eq!(bad, 0);
    // Negative control: one box off by a little changes the bytes.
    let (world, mut p, aa, target) = decode(&states[states.len() / 2]);
    p.position.x += F::new(40.0);
    assert_ne!(vi::selfstate::encode(world, &p, aa, target), states[states.len() / 2]);
}
