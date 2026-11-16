//! A pet's dyes as this client draws them (the server owner, 2026-11-16:
//! Zerkziee's drachnid pet should "have 2 different color areas that can be
//! changed like armor dye (will be a template for pet color dye)"; `net::petdye`
//! has the message, go-eqoa-server's `entity/petdyes.go` the tables and its
//! Pet Groomers the dyeing).
//!
//! The server says, by the id this client knows each by, which pets in view
//! are dyed: the body the dyes are for, a colour for each of its two areas and
//! which of the body's palette materials are area 2 (every other area 1). A
//! creature of that id in that body is built with each batch's layer colours
//! times its area's colour ([`apply`]), as an NPC's body tint multiplies the
//! whole body (`npctint::apply`, whose multipliers it uses): the colour the
//! strip programs modulate each texel by, so each area is darker or shifted in
//! hue under the game's own light and fog. An area of colour 0 keeps its own
//! colours; a creature not in the set, or in another body than the dyes' (a
//! pet summoned anew in another look), is drawn exactly as before.
//!
//! One set a thread, as `npctint`'s: a new set replaces the last whole, and a
//! creature made in other dyes than its id's now is made again (`play::apply`).

use std::cell::RefCell;
use std::collections::HashMap;

pub use net::petdye::PetDye;

thread_local! {
    static DYES: RefCell<HashMap<u32, PetDye>> = RefCell::default();
}

/// The dyed pets the server sent (`net::session::OP_PET_DYES`), by id: the
/// whole set, the last one standing.
pub fn set(dyes: Vec<(u32, PetDye)>) {
    DYES.with_borrow_mut(|d| *d = dyes.into_iter().filter(|(_, p)| p.area != [0, 0]).collect());
}

/// The dyes the creature of this id is drawn in while in body `look`, if any.
pub fn of(id: u32, look: u32) -> Option<PetDye> {
    DYES.with_borrow(|d| if d.is_empty() { None } else { d.get(&id).copied().filter(|p| p.look == look) })
}

/// `model` in dyes `d`: each batch's layer colours' red, green and blue times
/// its area's colour (`npctint::multipliers`: as much of it as its alpha
/// says), rounded, their alpha kept; an area of colour 0 (or one that changes
/// nothing) left as it is, and a batch with no layer too.
pub fn apply(model: &mut esf::Model, d: PetDye) {
    let m = d.area.map(|c| if c == 0 { [255; 3] } else { crate::npctint::multipliers(c) });
    if m == [[255; 3]; 2] {
        return;
    }
    for b in model.batches.iter_mut() {
        let m = m[d.area_of(b.material)];
        if m == [255; 3] {
            continue;
        }
        for l in b.layers.iter_mut() {
            for (v, m) in l.color.iter_mut().zip(m) {
                *v = ((*v as u32 * m + 127) / 255) as u8;
            }
        }
    }
}

/// What each material of a body is (`eqoa dress <body> out.png
/// petareas=1`, for a GM's petdyeareas row): a line per palette material, its
/// batches' triangles and the textures its layers draw.
pub fn areas_report(model: &esf::Model) -> String {
    let mut by: std::collections::BTreeMap<i32, (usize, Vec<i32>)> = Default::default();
    for b in &model.batches {
        let e = by.entry(b.material).or_default();
        e.0 += b.indices.len() / 3;
        for l in &b.layers {
            if !e.1.contains(&l.texture) {
                e.1.push(l.texture);
            }
        }
    }
    by.iter().map(|(m, (tris, tex))| format!("material {m}: {tris} triangles, textures {tex:?}\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(color: [u8; 4]) -> esf::Layer {
        esf::Layer { flags: 1, texture: 7, wrap: 0, blend: 0, color, uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0, 0.0], emissive: [10, 20, 30, 0] }
    }

    fn body() -> esf::Model {
        esf::Model {
            batches: vec![
                esf::Batch { material: 0, layers: vec![layer([255; 4])], indices: vec![0, 1, 2], lod: None },
                esf::Batch { material: 1, layers: vec![layer([128, 64, 200, 77])], indices: vec![0, 2, 3], lod: None },
                esf::Batch { material: -1, layers: vec![layer([255; 4])], indices: vec![0, 2, 3], lod: None },
            ],
            ..Default::default()
        }
    }

    /// Area 1's colour is on every batch of a material not in the mask (none,
    /// -1, too), area 2's on those in it; each a multiply of the layer
    /// colours' red, green and blue, alpha, emissive and texture kept. An
    /// area of 0 keeps its own; dyes of 0 and 0, or white, change nothing (the
    /// control).
    #[test]
    fn each_area_takes_its_own_colour() {
        let mut m = body();
        apply(&mut m, PetDye { look: 1, area: [0xFF00_00FF, 0x0000_FFFF], mask: 0b10 });
        assert_eq!(m.batches[0].layers[0].color, [255, 0, 0, 255]);
        assert_eq!(m.batches[1].layers[0].color, [0, 0, 200, 77], "area 2: blue times its own, alpha kept");
        assert_eq!(m.batches[2].layers[0].color, [255, 0, 0, 255], "no material: area 1");
        assert_eq!((m.batches[1].layers[0].emissive, m.batches[1].layers[0].texture), ([10, 20, 30, 0], 7));
        let mut one = body();
        apply(&mut one, PetDye { look: 1, area: [0, 0x0000_FFFF], mask: 0b10 });
        assert_eq!((one.batches[0].layers[0].color, one.batches[1].layers[0].color), ([255; 4], [0, 0, 200, 77]), "area 1 undyed keeps its own");
        for area in [[0, 0], [0xFFFF_FFFF, 0xFFFF_FFFF], [0xFF00_0000, 0]] {
            let mut same = body();
            apply(&mut same, PetDye { look: 1, area, mask: 0b10 });
            let colours = |m: &esf::Model| m.batches.iter().flat_map(|b| b.layers.iter().map(|l| l.color)).collect::<Vec<_>>();
            assert_eq!(colours(&same), colours(&body()), "{area:08X?} changed something");
        }
        let mut whole = body();
        apply(&mut whole, PetDye { look: 1, area: [0x8080_80FF, 0x0000_FFFF], mask: 0 });
        assert_eq!(whole.batches[1].layers[0].color, [64, 32, 100, 77], "no mask: the whole body area 1's");
    }

    /// The set is by id and body: a pet in another body than the dyes' is
    /// drawn as it is (the control), and a new set replaces the last whole.
    #[test]
    fn dyes_hold_for_their_id_and_body_alone() {
        set(vec![(5, PetDye { look: 0x1576_4DA8, area: [0xFF00_00FF, 0], mask: 0 }), (6, PetDye { look: 1, area: [0, 0], mask: 0 })]);
        assert_eq!(of(5, 0x1576_4DA8).map(|d| d.area), Some([0xFF00_00FF, 0]));
        assert_eq!(of(5, 0x9CF7_2381), None, "another body: undyed");
        assert_eq!((of(6, 1), of(7, 1)), (None, None), "an entry dyeing nothing, and an id not in the set");
        set(Vec::new());
        assert_eq!(of(5, 0x1576_4DA8), None);
    }
}
