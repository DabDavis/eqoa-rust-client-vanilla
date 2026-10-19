//! The dictionary across files.
//!
//! CHAR.ESF and CHARCUST.CSF register 16 of the same DictIDs, 8 of them with
//! different bytes (the shadowed armor of `claude-notes/charesf-shadowed-
//! armor.md`), so which file wins changes what is drawn. The game's rule is
//! that the first registration wins (`ParseSurfaceObj` finds before it adds;
//! `VIMap::Insert` keeps an existing key); this holds the library to it in both
//! load orders.

mod common;

#[test]
fn first_loaded_file_wins() {
    let Some((mut disc, _)) = common::inputs("expected.js") else { return };
    let char_esf = common::container(&mut disc, "CHAR.ESF");
    let charcust = common::container(&mut disc, "CHARCUST.CSF");
    let shared: Vec<i32> = char_esf.dict.keys().filter(|k| charcust.dict.contains_key(k)).copied().collect();
    assert!(!shared.is_empty(), "the premise changed: CHAR and CHARCUST share no DictID");

    for first_is_char in [true, false] {
        let mut lib = esf::Library::new();
        let (a, b) = if first_is_char { (&char_esf, &charcust) } else { (&charcust, &char_esf) };
        lib.add("first", esf::Container::parse(a.data.clone()).unwrap());
        lib.add("second", esf::Container::parse(b.data.clone()).unwrap());
        for &id in &shared {
            let r = lib.lookup(id).unwrap();
            assert_eq!(r.file, 0, "0x{:08X} resolved into the later file", id as u32);
            assert_eq!(r.node, a.dict[&id]);
        }
    }
    eprintln!("{} shared DictIDs resolve to the first-loaded file in both orders", shared.len());
}

#[test]
fn group_members_resolve() {
    // ITEM.ESF's GroupSprites name 362 point sprites (0x2D00, id in the body),
    // which a table copied from the Go tools did not register.
    let Some((mut disc, _)) = common::inputs("expected.js") else { return };
    let c = common::container(&mut disc, "ITEM.ESF");
    let missing: u32 = c.models().iter().filter_map(|r| c.build_model(r.node)).map(|m| m.missing).sum();
    assert_eq!(missing, 0, "GroupSprite members that resolve to nothing");
}
