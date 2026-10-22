//! Which characters hold items, on the disc: `ParseCSpriteObj` reads the
//! 0x2710 header's `npc` (version 4 up, 1 below) into `VICSprite::Init`,
//! which keeps it as `m_flags & 1`, and `AttachItem` attaches nothing while
//! it is clear (decomp `viesfparse.cpp` `ParseCSpriteObj`, `vicsprite.cpp`
//! `Init` and `AttachItem`). Every playable race's model holds items; the
//! creatures Sandstorm turned a bard into hold none, though Sandstorm's
//! channel still named both of the bard's weapons and both creatures name
//! item nodes (`pcaps/ps2_main_capture.pcap`: Mouse Form, 0x8150E6AF;
//! `pcaps/ps2_sandstorm.pcap`: 0x82A69570).

use vi::character::{AnimCache, load_csprite};
use vi::csprite::{CSprite, Rand};
use vi::fpu::Ee;
use vi::hsprite::AnimScene;

#[test]
fn forms_hold_no_items() {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    let Ok(mut iso) = iso::Iso::open(&p) else {
        eprintln!("SKIP: no disc at run/eqoa.iso");
        return;
    };
    let mut lib = esf::Library::new();
    let file = lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
    let (mut sc, mut cache) = (AnimScene::<Ee>::default(), AnimCache::new());
    let mut load = |r: esf::Ref| load_csprite(&lib, r, &mut sc, &mut cache, &mut Rand(1));
    let held = |cs: &CSprite<Ee>| [cs.item_node(0), cs.item_node(1), cs.item_node(2)];

    let human = load(lib.lookup(0x70D898C6u32 as i32).unwrap()).expect("the human");
    assert!(human.holds_items);
    assert_eq!(held(&human), [Some(18), Some(27), Some(28)], "the human's hands and back");
    for form in [0x8150E6AFu32, 0x82A69570] {
        let cs = load(lib.lookup(form as i32).unwrap()).expect("the form");
        // the negative control: a test of the node alone attaches the weapons
        assert!(cs.item_nodes[0] >= 0 && cs.item_nodes[1] >= 0, "{form:#010x} names item nodes");
        assert!(!cs.holds_items, "{form:#010x}: the header's npc is 0");
        assert_eq!(held(&cs), [None; 3], "{form:#010x} holds nothing");
    }

    // every CSprite: a playable race's (race 0-8, 10; 9 is VICSpriteRaceNone)
    // holds items, and most of the rest, which name item nodes, hold none
    let (mut races, mut creatures, mut bare, mut named) = (0, 0, 0, 0);
    let c = lib.file(file);
    for node in (0..c.nodes.len()).filter(|&n| c.node(n).ty == 0x2700) {
        let Some(cs) = load(esf::Ref { file, node }) else { continue };
        if cs.race != 9 {
            races += 1;
            assert!(cs.holds_items, "a race's model (race {}) holds items", cs.race);
        } else {
            creatures += 1;
            if !cs.holds_items {
                bare += 1;
                named += cs.item_nodes[..3].iter().any(|&n| n >= 0) as usize;
            }
        }
    }
    eprintln!("{races} race models, {creatures} creatures: {bare} hold no items, {named} of them naming item nodes");
    assert!(races >= 20 && bare > 100 && named > 100);
}
