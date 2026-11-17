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
    /// The dyers' colour picker's preview (`dyeplay`): a pet's dyes while
    /// its colour is picked, over the server's.
    static PREVIEW: RefCell<Option<(u32, PetDye)>> = const { RefCell::new(None) };
}

/// The picker's preview of a pet's dyes (by its id), or none.
pub fn preview(p: Option<(u32, PetDye)>) {
    PREVIEW.with_borrow_mut(|v| *v = p);
}

/// The dyed pets the server sent (`net::session::OP_PET_DYES`), by id: the
/// whole set, the last one standing.
pub fn set(dyes: Vec<(u32, PetDye)>) {
    DYES.with_borrow_mut(|d| *d = dyes.into_iter().filter(|(_, p)| p.area != [0, 0]).collect());
}

/// The dyes the creature of this id is drawn in while in body `look`, if any.
pub fn of(id: u32, look: u32) -> Option<PetDye> {
    if let Some(p) = PREVIEW.with_borrow(|v| v.filter(|(i, p)| *i == id && p.look == look)) {
        return Some(p.1);
    }
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

/// How much of texture `tex` is clothing, by the rule (go-eqoa-server's
/// layout 2, [`PetDye::rule`]): an armour set's or robe's texture all of it
/// (`vi::cust::is_garment`), a pet texture's as `render::petart` has it; none
/// for the body's own (skin, fur, hide, bone).
pub fn cloth_of(tex: i32) -> Option<render::petart::Cloth> {
    if vi::cust::is_garment(tex as u32) {
        return Some(render::petart::Cloth::Whole);
    }
    render::petart::cloth_of(tex as u32)
}

/// Whether a body wears clothing by the rule: some of its layers' textures
/// are clothing ([`cloth_of`]) and some of it is its own. A body all of
/// clothing (an animation, all plate) or of none is dyed whole, as its body.
pub fn clothed(model: &esf::Model) -> bool {
    let (mut cloth, mut bare) = (false, false);
    for l in model.batches.iter().flat_map(|b| b.layers.iter()) {
        match cloth_of(l.texture) {
            Some(render::petart::Cloth::Whole) => cloth = true,
            Some(render::petart::Cloth::Parts(_)) => (cloth, bare) = (true, true),
            None => bare = true,
        }
    }
    cloth && bare
}

/// `model` in dyes `d` by the rule (area 1 its clothing, area 2 the rest;
/// [`clothed`]: a body of no clothing all area 2): a layer of a texture all
/// clothing takes area 1's colour on its layer colour, one of none area 2's;
/// one of a texture part clothing draws a recoloured texture
/// (`render::recolor::CLOTH` and `BARE`, the parts `render::petart`'s), its
/// layer colour kept. `taken` is what a recoloured name must not be (a
/// library's lookup).
pub fn apply_rule(model: &mut esf::Model, d: PetDye, taken: &dyn Fn(i32) -> bool) {
    let m = d.area.map(|c| if c == 0 { [255; 3] } else { crate::npctint::multipliers(c) });
    if m == [[255; 3]; 2] {
        return;
    }
    let clothed = clothed(model);
    let rgb = |m: [u32; 3]| m[0] << 16 | m[1] << 8 | m[2];
    for l in model.batches.iter_mut().flat_map(|b| b.layers.iter_mut()) {
        let cloth = if clothed { cloth_of(l.texture) } else { None };
        match cloth {
            Some(render::petart::Cloth::Parts(_)) => {
                let mut t = l.texture;
                if m[0] != [255; 3] {
                    t = render::recolor::recolored(t, render::recolor::CLOTH << 24 | rgb(m[0]), taken);
                }
                if m[1] != [255; 3] {
                    t = render::recolor::recolored(t, render::recolor::BARE << 24 | rgb(m[1]), taken);
                }
                l.texture = t;
            }
            whole => {
                let m = m[usize::from(whole.is_none())];
                for (v, m) in l.color.iter_mut().zip(m) {
                    *v = ((*v as u32 * m + 127) / 255) as u8;
                }
            }
        }
    }
}

/// go-eqoa-server's `internal/entity/petclothes.go`: the bodies that wear
/// clothing by the rule (`eqoa petclothes`), so its Pet Groomers ask after a
/// clothed pet's clothing and its body, and dye an unclothed one whole.
pub fn go_list(ids: &[u32]) -> String {
    let mut out = String::from("// Code generated by vanilla's `eqoa petclothes` (eqoa-rust-client-vanilla\n// crates/eqoa/src/petdye.rs, render's petart.rs) off the Frontiers disc's\n// CHAR.ESF. DO NOT EDIT: run it again when the rule changes.\n\npackage entity\n\n");
    out += &format!("// petClothedBodies is every CHAR.ESF body ({}) that wears clothing by the\n// client's pet dye rule: some of it clothing, some of it its own.\nvar petClothedBodies = map[uint32]bool{{\n", ids.len());
    for c in ids.chunks(5) {
        out += "\t";
        out += &c.iter().map(|id| format!("0x{id:08X}: true,")).collect::<Vec<_>>().join(" ");
        out += "\n";
    }
    out += "}";
    out
}

/// What each material of a body is (`eqoa dress <body> out.png
/// petareas=1`): a line per palette material, its batches' triangles, the
/// textures its layers draw (each with the file it lives in, `garment` for an
/// armour set's or robe's, and the rule's `clothing` or `part clothing`), the
/// text slots that drive it; and whether the body is clothed by the rule.
pub fn areas_report(lib: &esf::Library, model: &esf::Model, slots: Option<&[(i32, i32); 19]>) -> String {
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
    let name = |t: i32| {
        let file = lib.lookup(t).map_or("-".to_string(), |r| lib.file_name(r.file).to_string());
        let rule = match cloth_of(t) {
            Some(render::petart::Cloth::Whole) => " clothing",
            Some(render::petart::Cloth::Parts(_)) => " part clothing",
            None => "",
        };
        format!("{:08X} {file}{}{rule}", t as u32, if vi::cust::is_garment(t as u32) { " garment" } else { "" })
    };
    let mut out: String = by
        .iter()
        .map(|(m, (tris, tex))| {
            let driven: Vec<usize> = slots.map(|s| (0..19).filter(|&i| s[i].0 == *m).collect()).unwrap_or_default();
            format!("material {m}: {tris} triangles, slots {driven:?}, textures [{}]\n", tex.iter().map(|&t| name(t)).collect::<Vec<_>>().join(", "))
        })
        .collect();
    out += &format!("clothed by the rule: {}\n", clothed(model));
    out
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
        apply(&mut m, PetDye { look: 1, area: [0xFF00_00FF, 0x0000_FFFF], mask: 0b10, rule: false });
        assert_eq!(m.batches[0].layers[0].color, [255, 0, 0, 255]);
        assert_eq!(m.batches[1].layers[0].color, [0, 0, 200, 77], "area 2: blue times its own, alpha kept");
        assert_eq!(m.batches[2].layers[0].color, [255, 0, 0, 255], "no material: area 1");
        assert_eq!((m.batches[1].layers[0].emissive, m.batches[1].layers[0].texture), ([10, 20, 30, 0], 7));
        let mut one = body();
        apply(&mut one, PetDye { look: 1, area: [0, 0x0000_FFFF], mask: 0b10, rule: false });
        assert_eq!((one.batches[0].layers[0].color, one.batches[1].layers[0].color), ([255; 4], [0, 0, 200, 77]), "area 1 undyed keeps its own");
        for area in [[0, 0], [0xFFFF_FFFF, 0xFFFF_FFFF], [0xFF00_0000, 0]] {
            let mut same = body();
            apply(&mut same, PetDye { look: 1, area, mask: 0b10, rule: false });
            let colours = |m: &esf::Model| m.batches.iter().flat_map(|b| b.layers.iter().map(|l| l.color)).collect::<Vec<_>>();
            assert_eq!(colours(&same), colours(&body()), "{area:08X?} changed something");
        }
        let mut whole = body();
        apply(&mut whole, PetDye { look: 1, area: [0x8080_80FF, 0x0000_FFFF], mask: 0, rule: false });
        assert_eq!(whole.batches[1].layers[0].color, [64, 32, 100, 77], "no mask: the whole body area 1's");
    }

    /// By the rule (layout 2): a texture all clothing takes area 1's colour on
    /// its layer, the body's own area 2's, one part clothing draws a texture
    /// recoloured by both (`render::recolor::CLOTH`, then `BARE`) on its layer
    /// colour as it was. A body all of clothing, or of none, is dyed whole as
    /// its body (the controls).
    #[test]
    fn the_rule_dyes_clothing_and_body_apart() {
        let tex = |t: u32, c: [u8; 4]| esf::Layer { texture: t as i32, ..layer(c) };
        let model = |ts: &[u32]| esf::Model {
            batches: ts.iter().enumerate().map(|(i, &t)| esf::Batch { material: i as i32, layers: vec![tex(t, [255; 4])], indices: vec![0, 1, 2], lod: None }).collect(),
            ..Default::default()
        };
        let d = PetDye { look: 0x1576_4DA8, area: [0xFF00_00FF, 0x0000_FFFF], mask: 0, rule: true };
        // the drachnid: plating (clothing), the spider (its own), a djinn's sash texture (part)
        let mut m = model(&[0xFF88_DE6B, 0xAC3E_6DF0, 0x4F83_8CC3]);
        assert!(clothed(&m));
        apply_rule(&mut m, d, &|_| false);
        assert_eq!((m.batches[0].layers[0].color, m.batches[1].layers[0].color), ([255, 0, 0, 255], [0, 0, 255, 255]));
        let part = &m.batches[2].layers[0];
        assert_eq!(part.color, [255; 4], "a part's layer colour kept");
        let (src, r) = render::recolor::source_of(part.texture).expect("a recoloured texture");
        assert_eq!(r, render::recolor::BARE << 24 | 0x0000FF);
        assert_eq!(render::recolor::source_of(src), Some((0x4F83_8CC3u32 as i32, render::recolor::CLOTH << 24 | 0xFF0000)));
        // all clothing (an animation's plate) and none (a fire elemental): whole, the body's colour
        for ts in [&[0x0638_5212u32, 0x40B0_DDA7][..], &[0x91C0_13E3][..]] {
            let mut m = model(ts);
            assert!(!clothed(&m));
            apply_rule(&mut m, d, &|_| false);
            assert!(m.batches.iter().all(|b| b.layers[0].color == [0, 0, 255, 255]), "{ts:08X?}");
        }
        assert_eq!(cloth_of(0x40B0_DDA7u32 as i32), Some(render::petart::Cloth::Whole), "set 4's plate: a garment");
    }

    /// The set is by id and body: a pet in another body than the dyes' is
    /// drawn as it is (the control), and a new set replaces the last whole.
    #[test]
    fn dyes_hold_for_their_id_and_body_alone() {
        set(vec![(5, PetDye { look: 0x1576_4DA8, area: [0xFF00_00FF, 0], mask: 0, rule: false }), (6, PetDye { look: 1, area: [0, 0], mask: 0, rule: false })]);
        assert_eq!(of(5, 0x1576_4DA8).map(|d| d.area), Some([0xFF00_00FF, 0]));
        assert_eq!(of(5, 0x9CF7_2381), None, "another body: undyed");
        assert_eq!((of(6, 1), of(7, 1)), (None, None), "an entry dyeing nothing, and an id not in the set");
        set(Vec::new());
        assert_eq!(of(5, 0x1576_4DA8), None);
    }
}
