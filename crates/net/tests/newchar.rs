//! 0x002B as `SendNewChar` writes it, read back in go-eqoa-server's order
//! (`HandleCreateCharacter`): the name, nine zigzags, seven raw ints.

use net::session::{NewChar, new_char_message};
use net::stream::Reader;

fn read(b: &[u8]) -> (u16, Vec<u8>, Vec<i64>, Vec<u32>, usize) {
    let mut r = Reader::new(b);
    let op = r.u16();
    let name = r.string(24);
    let f = (0..9).map(|_| r.signed()).collect();
    let s = (0..7).map(|_| r.u32()).collect();
    (op, name, f, s, r.pos)
}

#[test]
fn the_creation_form() {
    let c = NewChar { name: "Arwen".into(), race: 1, class: 13, sex: 1, hair_color: 7, hair_length: 2, hair_style: 3, face: 0, city: 0, stats: [5, 0, 3, 2, 4, 6, 0] };
    let b = new_char_message(&c);
    let (op, name, f, s, end) = read(&b);
    assert_eq!(op, 0x002B);
    assert_eq!(name, b"Arwen");
    // m_p18 (1), race, class, sex, hair colour, length, style, face, city
    assert_eq!(f, [1, 1, 13, 1, 7, 2, 3, 0, 0]);
    assert_eq!(s, [5, 0, 3, 2, 4, 6, 0]);
    assert_eq!(end, b.len(), "nothing after the stats");
    // the length is a u32, not a varint: a varint reader sees 5, then zeros
    assert_eq!(&b[2..6], &[5, 0, 0, 0]);
}

#[test]
fn the_name_fits_the_forms_buffer() {
    let c = NewChar { name: "Abcdefghijklmnopqrstuvwxyz".into(), face: -1, ..Default::default() };
    let (_, name, f, _, _) = read(&new_char_message(&c));
    assert_eq!(name.len(), 23, "24 bytes with the terminator");
    assert_eq!(f[7], -1, "a negative zigzags and comes back");
    // control: a 24-byte name would be read as empty by ReadString(.., 24)
    let mut long = new_char_message(&NewChar { name: "x".repeat(23), ..Default::default() });
    long[2] = 24;
    long.insert(6 + 23, b'y');
    assert!(read(&long).1.is_empty());
}
