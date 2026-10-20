//! `net::dump::read_effects` (`VIClient::Channel43`, state channel 0x43) by
//! the decomp's rules: 132-byte records after the count, at most eight, a
//! wide name of at most 31 characters, and `VIMemStream`'s reads (nothing
//! written past the end, nothing read after an error). The captures hold no
//! whole value of this channel (pcaps/ps2_sandstorm.pcap has one 4-byte
//! delta on a base the capture dropped), so these are built by hand; each
//! has a control a looser reader would fail.

use net::dump::{Effects, read_effects};

fn record(icon: u32, name: &str) -> Vec<u8> {
    let mut r = icon.to_le_bytes().to_vec();
    let w: Vec<u16> = name.encode_utf16().collect();
    r.extend((w.len() as u32).to_le_bytes());
    for c in w {
        r.extend(c.to_le_bytes());
    }
    r.resize(132, 0);
    r
}

fn value(count: i32, records: &[Vec<u8>]) -> Vec<u8> {
    let mut v = count.to_le_bytes().to_vec();
    for r in records {
        v.extend(r);
    }
    v
}

fn name(e: &Effects, i: usize) -> String {
    String::from_utf16_lossy(&e.name[i])
}

#[test]
fn records_are_132_bytes() {
    let mut e = Effects::default();
    read_effects(&mut e, &value(2, &[record(0xDE1B_C5EC, "Gypsy WereHunter: Rat"), record(0x1234, "Haste")]));
    assert_eq!((e.count, e.icon[0], e.icon[1]), (2, 0xDE1B_C5EC, 0x1234));
    assert_eq!((name(&e, 0), name(&e, 1)), ("Gypsy WereHunter: Rat".into(), "Haste".into()));
    // control: records packed 66 apart, as go-eqoa-server writes them, do not
    // give the second
    let mut packed = value(2, &[]);
    packed.extend(&record(0xDE1B_C5EC, "Gypsy WereHunter: Rat")[..66]);
    packed.extend(&record(0x1234, "Haste")[..66]);
    packed.resize(4 + 2 * 132, 0);
    let mut p = Effects::default();
    read_effects(&mut p, &packed);
    assert_eq!(name(&p, 0), "Gypsy WereHunter: Rat");
    assert_ne!((p.icon[1], name(&p, 1)), (0x1234, "Haste".into()));
}

#[test]
fn a_name_is_at_most_31_characters() {
    let mut e = Effects::default();
    let n31 = "abcdefghijklmnopqrstuvwxyz01234";
    read_effects(&mut e, &value(1, &[record(7, n31)]));
    assert_eq!(name(&e, 0), n31);
    // 32 characters: skipped, and shown empty
    read_effects(&mut e, &value(1, &[record(7, &format!("{n31}5"))]));
    assert_eq!((e.icon[0], name(&e, 0)), (7, String::new()));
}

#[test]
fn the_count_is_at_most_eight_and_records_not_there_keep_their_values() {
    let mut e = Effects::default();
    let eight: Vec<Vec<u8>> = (0..8).map(|i| record(100 + i, &format!("Effect {i}"))).collect();
    read_effects(&mut e, &value(8, &eight));
    read_effects(&mut e, &value(9, &[record(1, "One")]));
    assert_eq!(e.count, 8);
    assert_eq!((e.icon[0], name(&e, 0)), (1, "One".into()));
    assert_eq!((e.icon[1], name(&e, 1)), (101, "Effect 1".into()));
    assert_eq!((e.icon[7], name(&e, 7)), (107, "Effect 7".into()));
    // a negative count reads no record
    read_effects(&mut e, &value(-1, &[record(5, "Five")]));
    assert_eq!((e.count, e.icon[0]), (-1, 1));
    // under four bytes: the count is not read and stays
    read_effects(&mut e, &[3, 0]);
    assert_eq!(e.count, -1);
}

#[test]
fn nothing_is_read_after_an_error() {
    let mut e = Effects::default();
    read_effects(&mut e, &value(2, &[record(10, "Ten"), record(20, "Twenty")]));
    // the first name's length runs past the end: its skip fails, and the
    // second record, whole at 136, is not read though the position moves
    // there
    let mut bad = record(11, "");
    bad[4..8].copy_from_slice(&1000u32.to_le_bytes());
    read_effects(&mut e, &value(2, &[bad, record(21, "Twenty-one")]));
    assert_eq!((e.icon[0], name(&e, 0)), (11, String::new()));
    assert_eq!((e.icon[1], name(&e, 1)), (20, "Twenty".into()));
    // control: with the first record sound, the second is read
    read_effects(&mut e, &value(2, &[record(11, "Eleven"), record(21, "Twenty-one")]));
    assert_eq!((e.icon[1], name(&e, 1)), (21, "Twenty-one".into()));
    // cut inside a name: what was there is cut at the length given
    let v = value(1, &[record(30, "Thirty")]);
    read_effects(&mut e, &v[..4 + 8 + 5]);
    assert_eq!((e.icon[0], name(&e, 0)), (30, "Eleven".into()));
    read_effects(&mut e, &value(1, &[record(30, "Abc")])[..4 + 8 + 1]);
    assert_eq!(name(&e, 0), "Ele");
}
