//! The 2002 view (`Era::Y2002`): a file off the 2002 disc standing in for its
//! namesake. Each test pairs what the 2002 view answers with what Frontiers'
//! answers for the same id, the control: the Frontiers view never sees a
//! stand-in.

use esf::{Container, Era, Library, Ref};

/// A container of root records, each `(type, body)`.
fn container(records: &[(u16, Vec<u8>)]) -> Container {
    let mut d = b"FJBO".to_vec();
    d.resize(24, 0);
    d[16..24].copy_from_slice(&24i64.to_le_bytes());
    for (ty, body) in records {
        d.extend_from_slice(&ty.to_le_bytes());
        d.extend_from_slice(&0i16.to_le_bytes());
        d.extend_from_slice(&(body.len() as i32).to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(body);
    }
    Container::parse(d).unwrap()
}

/// A surface record (its DictID the body's first word), its pixels `fill`.
fn surface(id: u32, fill: u8) -> (u16, Vec<u8>) {
    let mut b = id.to_le_bytes().to_vec();
    b.extend([fill; 16]);
    (0x1000, b)
}

const A: i32 = 0x0A;
const B: i32 = 0x0B;
const C: i32 = 0x0C;
const D: i32 = 0x0D;

/// CHAR.ESF with A, B and C; the 2002 disc's with A changed, B as it is, no
/// C, and a D of its own.
fn char_esf() -> Library {
    let mut lib = Library::new();
    lib.add("CHAR.ESF", container(&[surface(A as u32, 1), surface(B as u32, 2), surface(C as u32, 3)]));
    lib.stand_in(0, container(&[surface(A as u32, 9), surface(B as u32, 2), surface(D as u32, 4)]));
    lib
}

#[test]
fn a_stand_in_answers_in_the_2002_view_only() {
    let lib = char_esf();
    let f = |id| lib.lookup_in(Era::Frontiers, id).map(|r| r.file);
    let y = |id| lib.lookup_in(Era::Y2002, id).map(|r| r.file);
    // Frontiers' view: the disc the game runs off, the stand-in unseen
    assert_eq!((f(A), f(B), f(C), f(D)), (Some(0), Some(0), Some(0), None));
    // 2002's: the changed A and the new D off the 2002 disc; B, the same
    // bytes, is Frontiers' record; C, which the 2002 disc lacks, Frontiers'
    assert_eq!((y(A), y(B), y(C), y(D)), (Some(1), Some(0), Some(0), Some(1)));
    // the view picks the dictionary
    assert_eq!(lib.view(), Era::Frontiers);
    assert_eq!(lib.lookup(A).unwrap().file, 0);
    lib.set_view(Era::Y2002);
    assert_eq!(lib.lookup(A).unwrap().file, 1);
    assert_eq!(lib.with_view(Era::Frontiers, || lib.lookup(A).unwrap().file), 0);
    assert_eq!(lib.view(), Era::Y2002, "with_view puts the view back");
}

#[test]
fn a_texture_key_tells_only_changed_art_apart() {
    let lib = char_esf();
    let key = |era, id| lib.with_view(era, || lib.texture_key(id));
    assert_ne!(key(Era::Frontiers, A), key(Era::Y2002, A), "A changed: two textures");
    assert_eq!(key(Era::Frontiers, B), key(Era::Y2002, B), "B is the same bytes: one texture");
    assert_eq!(key(Era::Frontiers, C), key(Era::Y2002, C), "C is Frontiers' in both");
    assert_eq!(key(Era::Frontiers, A) as u32, A as u32, "the low word is the id");
}

#[test]
fn a_model_the_2002_disc_has_is_built_in_its_view() {
    let lib = char_esf();
    assert!(lib.in_2002(A) && lib.in_2002(B) && lib.in_2002(D));
    assert!(!lib.in_2002(C), "C is only on Frontiers' disc");
    // in Frontiers' view everything is Frontiers'
    assert_eq!([A, B, C, D].map(|id| lib.view_for(id)), [Era::Frontiers; 4]);
    lib.set_view(Era::Y2002);
    // B's record is Frontiers' bytes, but the 2002 disc has the model: its
    // parts are looked up as 2002's
    assert_eq!([A, B, C, D].map(|id| lib.view_for(id)), [Era::Y2002, Era::Y2002, Era::Frontiers, Era::Y2002]);
}

#[test]
fn a_stand_in_ranks_where_its_namesake_does() {
    let mut lib = Library::new();
    lib.add("TUNARIA.ESF", container(&[surface(A as u32, 5)]));
    lib.add("CHAR.ESF", container(&[surface(A as u32, 1), surface(B as u32, 2)]));
    lib.add("ITEM.ESF", container(&[surface(B as u32, 7), surface(C as u32, 3)]));
    let s = lib.stand_in(1, container(&[surface(A as u32, 9), surface(B as u32, 8), surface(C as u32, 6)]));
    assert_eq!(s, 3);
    let y = |id| lib.lookup_in(Era::Y2002, id).map(|r| r.file);
    // the world, registered first, still has A; the stand-in comes ahead of
    // CHAR.ESF and so of ITEM.ESF after it (B, C)
    assert_eq!((y(A), y(B), y(C)), (Some(0), Some(3), Some(3)));
    // the control: in Frontiers' view ITEM.ESF keeps C and CHAR.ESF B
    let f = |id| lib.lookup_in(Era::Frontiers, id).map(|r| r.file);
    assert_eq!((f(A), f(B), f(C)), (Some(0), Some(1), Some(2)));
    // a file added after the stand-in does not take its ids
    lib.add("LATER.CSF", container(&[surface(B as u32, 4), surface(D as u32, 4)]));
    let y = |id| lib.lookup_in(Era::Y2002, id).map(|r| r.file);
    assert_eq!((y(B), y(D)), (Some(3), Some(4)));
}

#[test]
fn a_kept_id_is_frontiers_in_both_views() {
    let mut lib = char_esf();
    lib.keep_frontiers(&[A]);
    let y = |id| lib.lookup_in(Era::Y2002, id).map(|r| r.file);
    // A kept: Frontiers' record; the control, D, the 2002 file's still
    assert_eq!((y(A), y(D)), (Some(0), Some(1)));
    assert!(!lib.in_2002(A), "a kept id is not the 2002 disc's model");
    // kept before the stand-in comes, the same
    let mut lib = Library::new();
    lib.add("CHAR.ESF", container(&[surface(A as u32, 1)]));
    lib.keep_frontiers(&[A]);
    lib.stand_in(0, container(&[surface(A as u32, 9)]));
    assert_eq!(lib.lookup_in(Era::Y2002, A).map(|r| r.file), Some(0));
}

#[test]
fn files_by_name_as_the_view_sees_them() {
    let lib = char_esf();
    assert_eq!(lib.file_named("char.esf"), Some(0));
    assert_eq!(lib.in_view(), vec![0]);
    lib.set_view(Era::Y2002);
    assert_eq!(lib.file_named("char.esf"), Some(1));
    assert_eq!(lib.in_view(), vec![1, 0]);
    assert_eq!((lib.era_of(0), lib.era_of(1)), (Era::Frontiers, Era::Y2002));
    assert_eq!((lib.stands_for(1), lib.stand_in_of(0)), (Some(0), Some(1)));
}

#[test]
fn a_world_change_keeps_the_stand_ins_and_the_aliases() {
    let mut lib = Library::new();
    lib.add("TUNARIA.ESF", container(&[surface(D as u32, 5)]));
    lib.add("CHAR.ESF", container(&[surface(A as u32, 1)]));
    lib.stand_in(1, container(&[surface(A as u32, 9)]));
    assert!(lib.alias(B, Ref { file: 1, node: 0 }));
    lib.replace_from(0, "TUNARIA.ESF", container(&[surface(C as u32, 5)]), Era::Y2002);
    let y = |id| lib.lookup_in(Era::Y2002, id).map(|r| r.file);
    assert_eq!((y(A), y(B), y(C), y(D)), (Some(2), Some(1), Some(0), None));
    assert_eq!(lib.lookup_in(Era::Frontiers, A).map(|r| r.file), Some(1));
    assert_eq!(lib.era_of(0), Era::Y2002, "the world is the 2002 disc's now");
    // a second stand-in for CHAR.ESF takes the first's place
    lib.stand_in(1, container(&[surface(A as u32, 1)]));
    let y = |id| lib.lookup_in(Era::Y2002, id).map(|r| r.file);
    assert_eq!(y(A), Some(1), "the same bytes as Frontiers': Frontiers' record");
    assert_eq!(lib.files.len(), 3);
}

#[test]
fn without_a_stand_in_both_views_are_one() {
    let mut lib = Library::new();
    lib.add("CHAR.ESF", container(&[surface(A as u32, 1)]));
    assert!(!lib.has_2002());
    assert_eq!(lib.lookup_in(Era::Y2002, A), lib.lookup_in(Era::Frontiers, A));
    assert_eq!(lib.with_view(Era::Y2002, || lib.texture_key(A)), lib.texture_key(A));
}

/// The two discs: `$EQOA_ISO` (default `run/eqoa.iso`) and `$EQOA_2002_ISO`
/// (default the USA disc under `../decomp-builds/discs/eqoa-usa/`).
fn discs() -> Option<(iso::Iso, iso::Iso)> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let frontiers = std::env::var_os("EQOA_ISO").map(std::path::PathBuf::from).unwrap_or_else(|| root.join("run/eqoa.iso"));
    let y2002 = std::env::var_os("EQOA_2002_ISO").map(std::path::PathBuf::from).or_else(|| {
        let dir = root.join("../decomp-builds/discs/eqoa-usa");
        std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")))
    });
    let (Some(y2002), true) = (y2002, frontiers.exists()) else {
        eprintln!("SKIP: no Frontiers disc or no 2002 disc (set EQOA_ISO and EQOA_2002_ISO)");
        return None;
    };
    Some((iso::Iso::open(frontiers).ok()?, iso::Iso::open(y2002).ok()?))
}

/// On the discs: the 2002 CHAR.ESF stands in for Frontiers'. The human
/// (0x70D898C6) is other art in 2002 and the ogre (0x6A82352C) the same
/// bytes; what the 2002 disc lacks (Frontiers' new creatures) stays
/// Frontiers' in both views. Counts as `claude-notes/era-2002-art-20261024.md`
/// measured them: 770 of the 2002 file's 3,661 ids are other art (or only
/// on its disc), 2,891 are Frontiers' bytes.
#[test]
fn the_2002_disc_stands_in_for_char_esf() {
    let Some((mut f, mut y)) = discs() else { return };
    let mut lib = Library::new();
    let char = lib.add("CHAR.ESF", Container::open(f.read("DATA/CHAR.ESF").unwrap()).unwrap());
    let s = lib.stand_in(char, Container::open(y.read("DATA/CHAR.ESF").unwrap()).unwrap());
    let (human, ogre) = (0x70D898C6u32 as i32, 0x6A82352Cu32 as i32);
    assert_eq!(lib.lookup_in(Era::Y2002, human).unwrap().file, s);
    assert_eq!(lib.lookup_in(Era::Frontiers, human).unwrap().file, char);
    assert_eq!(lib.lookup_in(Era::Y2002, ogre).unwrap().file, char, "the ogre did not change");
    let ids = &lib.file(s).dict;
    let off_2002 = ids.keys().filter(|&&id| lib.lookup_in(Era::Y2002, id).unwrap().file == s).count();
    assert_eq!((ids.len(), off_2002), (3661, 770));
    // a model only Frontiers has: built from Frontiers' records in 2002's view
    let frontiers_only = lib.file(char).dict.keys().filter(|id| !ids.contains_key(id)).count();
    assert_eq!(frontiers_only, 1762);
    lib.set_view(Era::Y2002);
    assert!(lib.file(char).dict.keys().filter(|id| !ids.contains_key(id)).all(|&id| lib.view_for(id) == Era::Frontiers));
    assert_eq!(lib.view_for(human), Era::Y2002);
}

/// A record: type, version 0, its children packed at the start of its body.
fn rec(ty: u16, children: &[Vec<u8>], tail: &[u8]) -> Vec<u8> {
    let mut body: Vec<u8> = children.concat();
    body.extend_from_slice(tail);
    let mut d = ty.to_le_bytes().to_vec();
    d.extend_from_slice(&0i16.to_le_bytes());
    d.extend_from_slice(&(body.len() as i32).to_le_bytes());
    d.extend_from_slice(&(children.len() as u32).to_le_bytes());
    d.extend_from_slice(&body);
    d
}

fn words(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// An actor record (0x6000) placing `sprite` at x.
fn actor(sprite: u32, x: f32) -> Vec<u8> {
    let mut b = words(&[sprite]);
    for v in [x, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&[255, 255, 255, 255]);
    rec(0x6000, &[], &b)
}

/// A zone's additions (`Container::additions`): the actors placing records of
/// the kinds asked for and those records whole, with what they name; the
/// control, the zone's other actors and records left out.
#[test]
fn a_zone_gives_only_the_additions_asked_for() {
    let (stream, volume, prop, art) = (0x5E, 0x70, 0x9A, 0xA7);
    let header = |id: u32| rec(0xC310, &[], &words(&[id, 0, 0, 0, 0, 0, 0]));
    let resources = rec(
        0x3100,
        &[
            rec(0x2E00, &[], &[words(&[stream, 0, 0, 0, 0, 0, 0]), vec![3, 0], b"amb".to_vec(), words(&[1, 5])].concat()),
            rec(0xC300, &[header(volume)], &words(&[art])),
            rec(0x2000, &[rec(0x2001, &[], &words(&[prop]))], &[]),
            rec(0x1000, &[], &words(&[art, 0, 0, 0, 0])),
        ],
        &[],
    );
    let room = rec(0x3270, &[rec(0x3280, &[], &words(&[2])), rec(0x6040, &[actor(stream, 1.0), actor(prop, 2.0), actor(volume, 3.0)], &[])], &[]);
    let zone = rec(0x3000, &[resources, rec(0x3290, &[room], &[])], &[]);
    let c = Container::parse_records(zone);
    let z = c.roots[0];
    // the volume names the surface `art` in its tail (as its parameters would)
    let needs = |c: &Container, n: usize| if c.node(n).ty == 0xC300 { vec![art as i32] } else { Vec::new() };
    let (extra, actors) = c.additions(z, &[0xC300, 0x2E00], needs).unwrap();
    assert_eq!(actors.iter().map(|a| (a.sprite as u32, a.room, a.pos[0])).collect::<Vec<_>>(), [(stream, 2, 1.0), (volume, 2, 3.0)]);
    for id in [stream, volume, art] {
        assert!(extra.lookup(id as i32).is_some(), "{id:#x} comes with them");
    }
    assert!(extra.lookup(prop as i32).is_none(), "the control: the prop stays the zone's own");
    assert!(c.additions(z, &[0x2B00], |_, _| Vec::new()).is_none(), "none placed: none");
}
