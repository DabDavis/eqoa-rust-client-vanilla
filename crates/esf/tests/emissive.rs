//! A material's emissive colour (`ParseMaterial`, version 3 on: four bytes
//! after the tessellate flags, `SetEmissiveColor`), carried on each of its
//! layers. The Sword of Virtue's four materials are all (204, 255, 255): its
//! blade, guard and pommel stay lit at night (F3 pass 26, against PCSX2). The
//! control: the layers after the emissive read as before (the sword's sheen
//! layer: blend 1, (204, 255, 255, 153)); a version-2 material reading black
//! is `mesh`'s own test. Needs `run/eqoa.iso`; without it, SKIPs.

use std::path::PathBuf;

fn item_esf() -> Option<esf::Container> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    let Ok(mut disc) = iso::Iso::open(&p) else {
        eprintln!("SKIP: no run/eqoa.iso");
        return None;
    };
    esf::Container::open(disc.read("ITEM.ESF").ok()?).ok()
}

#[test]
fn the_sword_of_virtues_materials_are_emissive() {
    let Some(c) = item_esf() else { return };
    let r = c.models().into_iter().find(|r| r.dict_id as u32 == 0x88EC_092D).expect("the Sword of Virtue");
    let m = c.build_model(r.node).expect("its model");
    assert_eq!(m.batches.len(), 4);
    for b in &m.batches {
        assert!(b.layers.iter().all(|l| l.emissive == [204, 255, 255, 255]), "material {}: {:?}", b.material, b.layers);
    }
    // the layers after it read as before: the blade's sheen, added in
    let blade = &m.batches[3].layers;
    assert_eq!((blade.len(), blade[1].blend, blade[1].color), (2, 1, [204, 255, 255, 153]));
    assert_eq!(blade[1].texture, blade[0].texture);
}
