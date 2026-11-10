//! `Library::record` and `answers_still`: what a build asked of the library
//! is answered the same until a file that answered it is another's, or a
//! file comes that answers what none did; and a file that changes nothing
//! asked leaves every answer. Each check has a control.

use esf::{Container, Era, Library};

fn record(ty: u16, children: u32, body: &[u8]) -> Vec<u8> {
    let mut r = ty.to_le_bytes().to_vec();
    r.extend(0i16.to_le_bytes());
    r.extend((body.len() as i32).to_le_bytes());
    r.extend(children.to_le_bytes());
    r.extend(body);
    r
}

/// A container of sprites (0x2700, a 0x2710 header each) of these ids.
fn sprites(ids: &[i32], fill: u8) -> Container {
    let mut bytes = Vec::new();
    for &id in ids {
        let mut hdr = id.to_le_bytes().to_vec();
        hdr.extend([fill; 8]);
        bytes.extend(record(0x2700, 1, &record(0x2710, 0, &hdr)));
    }
    Container::parse_records(bytes)
}

#[test]
fn answers_hold_until_what_answered_changes() {
    let mut lib = Library::new();
    let a = lib.add("A", sprites(&[1, 2], 0));
    assert!(lib.lookup(1).is_some() && lib.lookup(3).is_none(), "the made files register");
    // nothing is kept outside a record
    let ((), none) = lib.record(|| ());
    assert!(none.is_empty());
    let (got, asked) = lib.record(|| (lib.lookup(1), lib.lookup(3), lib.lookup(1), lib.view_for(2)));
    assert!(got.0.is_some() && got.1.is_none());
    // each question once (the view is Frontiers: nothing is asked of 2002)
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert!(lib.answers_still(&asked));
    // a file that answers nothing asked: still
    lib.add("B", sprites(&[7], 0));
    assert!(lib.answers_still(&asked));
    // a file that answers what none did: not
    let c = lib.add("C", sprites(&[3], 0));
    assert!(!lib.answers_still(&asked));
    let (_, asked) = lib.record(|| (lib.lookup(1), lib.lookup(3)));
    assert!(lib.answers_still(&asked));
    // the file that answered put in place of, even by the same records: not
    lib.replace(c, "C", sprites(&[3], 0));
    assert!(!lib.answers_still(&asked));
    let (_, asked) = lib.record(|| (lib.lookup(1), lib.lookup(3)));
    lib.replace(c, "C", sprites(&[8], 0));
    assert!(!lib.answers_still(&asked), "3 is gone");
    // another file's replacement leaves a question of A's
    let (_, of_a) = lib.record(|| lib.lookup(2));
    lib.replace(c, "C", sprites(&[3], 1));
    assert!(lib.answers_still(&of_a));
    lib.replace(a, "A", sprites(&[1, 2], 0));
    assert!(!lib.answers_still(&of_a));
}

#[test]
fn a_2002_copy_standing_in_changes_the_2002_answers() {
    let mut lib = Library::new();
    let a = lib.add("A", sprites(&[1, 2], 0));
    let (_, asked) = lib.record(|| lib.with_view(Era::Y2002, || (lib.view_for(1), lib.lookup(1))));
    let (_, frontiers) = lib.record(|| lib.lookup_in(Era::Frontiers, 1));
    assert!(lib.answers_still(&asked) && lib.answers_still(&frontiers));
    // its 2002 copy, other bytes, stands in
    lib.stand_in(a, sprites(&[1, 2], 9));
    assert!(!lib.answers_still(&asked), "the 2002 view names the copy's record now");
    assert!(lib.answers_still(&frontiers), "the control: Frontiers' view is as it was");
    // one record inside another hears the inner one's questions
    let ((_, inner), outer) = lib.record(|| lib.record(|| lib.lookup(2)));
    assert_eq!((inner.len(), outer.len()), (1, 1));
}
