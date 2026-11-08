//! A custom race's own character options, as this client draws them (the user,
//! 2026-11-08: "lets add char customization to the new races, if any other faces
//! and hair add them, but if not add color swaps"; `net::raceopts` has the
//! messages, go-eqoa-server's `entity/raceoptions.go` the options themselves).
//!
//! A server's own race wears a creature's body, which has none of the text slots
//! the retail look's face, hair and armour go on (`vi::cust`). What such a body has
//! is told race by race: other bodies of the same creature (the character's
//! model: every client draws that, a PS2 too), and parts of a body a colour or a
//! texture is laid on, each the triangles of some palette entries within some
//! rectangles of the texture ([`apply`]). A colour is a recoloured copy of the
//! part's own texture (`render::recolor`), named apart from it, so the rest of the
//! body, and every other creature that shares the texture, keep theirs; a texture
//! is a player race's face or hair laid on a head that already wears one of them.
//!
//! What each character chose comes by its name ([`take_looks`]) and is drawn
//! only on the body it was chosen on ([`dress_named`]: a player's character in
//! the world, character select's figures): under an illusion, a GM's form or a
//! race's armour body ("racearmorbody") the body is another and wears its own.
//! The create pages build the look from the steps on the page ([`look_of_steps`]).
//! A body with no look, an NPC, and every body on a server that sends none are
//! drawn exactly as before.
//!
//! One list a thread, as `racelabel`'s: a new list replaces the last whole, and
//! [`generation`] counts the changes, so the creatures already made are made
//! again in the look (`play`).

use net::raceopts::{Kind, Look, Options, Part, Row};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

thread_local! {
    static LOOKS: RefCell<HashMap<String, Rc<Look>>> = RefCell::default();
    static OPTIONS: RefCell<Option<Rc<Options>>> = const { RefCell::new(None) };
    static GENERATION: Cell<u64> = const { Cell::new(0) };
}

/// How many times the looks or the options changed on this thread: a creature
/// made under another is made again.
pub fn generation() -> u64 {
    GENERATION.with(|g| g.get())
}

fn bump() {
    GENERATION.with(|g| g.set(g.get() + 1));
}

/// The looks the server sent ([`net::session::OP_RACE_LOOKS`]): the whole list,
/// the last one standing. One that does not read leaves the one before.
pub fn take_looks(data: &[u8]) {
    match net::raceopts::read_looks(data) {
        Some(list) => set_looks(list.into_iter().map(|(n, l)| (n, Rc::new(l))).collect()),
        None => eprintln!("race looks: a list of {} bytes not read: the last kept", data.len()),
    }
}

/// `looks` the list.
pub fn set_looks(looks: HashMap<String, Rc<Look>>) {
    let changed = LOOKS.with_borrow(|l| *l != looks);
    if changed {
        LOOKS.with_borrow_mut(|l| *l = looks);
        bump();
    }
}

/// The look the character of this name chose, if it chose one.
pub fn look_of(name: &str) -> Option<Rc<Look>> {
    LOOKS.with_borrow(|l| l.get(name).cloned())
}

/// The options the server sent, kept for whatever this thread draws (the
/// session's `race_options`); a change makes the creatures again.
pub fn set_options(o: Option<&Options>) {
    let changed = OPTIONS.with_borrow(|cur| cur.as_deref() != o);
    if changed {
        OPTIONS.with_borrow_mut(|cur| *cur = o.cloned().map(Rc::new));
        bump();
    }
}

/// The options this thread was last given.
pub fn options() -> Option<Rc<Options>> {
    OPTIONS.with_borrow(|o| o.clone())
}

/// Whether `model` is a body a race's options offer (a player chose it): it
/// holds a player's items as its race's own does (`racebody`).
pub fn offered_body(model: u32) -> bool {
    OPTIONS.with_borrow(|o| o.as_ref().is_some_and(|o| o.offers_body(model)))
}

/// Body `body`'s model (built from it, `model`) in the look of the character
/// named `name`, if it chose one on that body. Whether anything changed.
pub fn dress_named(lib: &esf::Library, body: u32, model: &mut esf::Model, name: &str) -> bool {
    look_of(name).is_some_and(|l| dress(lib, body, model, &l))
}

/// Body `body`'s model in `look`, when the look was chosen on it (on any other
/// body, nothing). Whether anything changed.
pub fn dress(lib: &esf::Library, body: u32, model: &mut esf::Model, look: &Look) -> bool {
    look.body == body && apply(lib, model, &look.parts)
}

/// The look the steps on a create page make (`steps`, one a row of `rows`, a
/// female's when `sex` is 1) on `body`: each colour or texture row's step on its
/// parts of the body, a first step (the body's own) laying nothing.
pub fn look_of_steps(rows: &[Row], sex: i32, steps: &[usize], body: u32) -> Look {
    let mut parts = Vec::new();
    for (row, &k) in rows.iter().zip(steps) {
        if row.kind == Kind::Body || k == 0 {
            continue;
        }
        let Some(step) = row.steps(sex).get(k) else { continue };
        for p in row.parts_of(body) {
            parts.push((row.kind, step.value, p.clone()));
        }
    }
    Look { body, parts }
}

/// The body the steps on a create page choose: the body row's step, else none
/// (the race's own).
pub fn body_of_steps(rows: &[Row], sex: i32, steps: &[usize]) -> Option<u32> {
    rows.iter().zip(steps).find(|(r, _)| r.kind == Kind::Body).and_then(|(r, &k)| r.steps(sex).get(k)).map(|s| s.value)
}

/// Each part laid on the model: a colour on every layer of the part's
/// triangles (a textured layer's texture recoloured, an untextured one's colour,
/// the material's emissive colour), a texture on its first layer (when the
/// library has it). A part that is only some of a batch's triangles takes them
/// into a batch of their own, the same palette entry's. Whether anything
/// changed.
pub fn apply(lib: &esf::Library, model: &mut esf::Model, parts: &[(Kind, u32, Part)]) -> bool {
    let mut changed = false;
    for (kind, value, part) in parts {
        let Some(change) = Change::of(lib, *kind, *value) else { continue };
        let n = model.batches.len();
        for bi in 0..n {
            if !part.entries.iter().any(|&e| e as i32 == model.batches[bi].material) {
                continue;
            }
            let picked = picked(model, bi, &part.rects);
            if !picked.contains(&true) {
                continue;
            }
            let bi = if picked.contains(&false) {
                // the part's triangles into a batch of their own
                let b = &mut model.batches[bi];
                let (mut mine, mut rest) = (Vec::new(), Vec::new());
                for (t, &p) in b.indices.chunks_exact(3).zip(&picked) {
                    if p { mine.extend_from_slice(t) } else { rest.extend_from_slice(t) }
                }
                b.indices = rest;
                let split = esf::Batch { indices: mine, ..b.clone() };
                model.batches.push(split);
                model.batches.len() - 1
            } else {
                bi
            };
            change.lay(lib, &mut model.batches[bi]);
            changed = true;
        }
    }
    changed
}

/// What a part takes.
enum Change {
    Colour(u32),
    Texture(i32),
}

impl Change {
    /// A colour that changes something, a texture the library has; else none.
    fn of(lib: &esf::Library, kind: Kind, value: u32) -> Option<Change> {
        match kind {
            Kind::Colour if render::recolor::changes(value) => Some(Change::Colour(value)),
            Kind::Texture if value != 0 && lib.surface(value as i32).is_some() => Some(Change::Texture(value as i32)),
            _ => None,
        }
    }

    fn lay(&self, lib: &esf::Library, b: &mut esf::Batch) {
        match *self {
            Change::Colour(r) => {
                for l in &mut b.layers {
                    if l.texture != 0 {
                        l.texture = render::recolor::recolored(l.texture, r, |id| lib.lookup(id).is_some());
                    } else {
                        let c = render::recolor::recolor_rgb([l.color[0], l.color[1], l.color[2]], r);
                        l.color = [c[0], c[1], c[2], l.color[3]];
                    }
                    if l.emissive[..3] != [0, 0, 0] {
                        let c = render::recolor::recolor_rgb([l.emissive[0], l.emissive[1], l.emissive[2]], r);
                        l.emissive = [c[0], c[1], c[2], l.emissive[3]];
                    }
                }
            }
            Change::Texture(t) => {
                if let Some(l) = b.layers.first_mut() {
                    l.texture = t;
                }
            }
        }
    }
}

/// Which of batch `bi`'s triangles a part's rects take: the centre of each
/// one's texture coordinates (the first layer's, after its transform, the
/// texture repeated) within a rect; every one with no rects.
fn picked(model: &esf::Model, bi: usize, rects: &[[u8; 4]]) -> Vec<bool> {
    let b = &model.batches[bi];
    if rects.is_empty() {
        return vec![true; b.indices.len() / 3];
    }
    let layer = b.layers.first();
    b.indices
        .chunks_exact(3)
        .map(|t| {
            let uv = t.iter().fold([0.0f32; 2], |a, &v| {
                let p = model.uv.get(v as usize).copied().unwrap_or([0.0; 2]);
                let p = layer.map_or(p, |l| l.apply_uv(p));
                [a[0] + p[0] / 3.0, a[1] + p[1] / 3.0]
            });
            let (u, v) = (uv[0].rem_euclid(1.0) * 255.0, uv[1].rem_euclid(1.0) * 255.0);
            rects.iter().any(|r| u >= r[0] as f32 && u <= r[2] as f32 && v >= r[1] as f32 && v <= r[3] as f32)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A model of two palette entries: entry 0 two triangles, one at the left
    /// of the texture and one at its right; entry 1 one triangle; textures 7
    /// and 8, entry 1 an emissive one.
    fn model() -> esf::Model {
        let layer = |texture: i32, emissive: [u8; 4]| esf::Layer { flags: 0, texture, wrap: 0, blend: 0, color: [255; 4], uv_transform: [0.0; 9], lod_bias: 0.8, uv_rate: [0.0; 2], emissive };
        let mut m = esf::Model::default();
        m.pos = vec![[0.0; 3]; 9];
        m.uv = vec![[0.1, 0.1], [0.2, 0.1], [0.1, 0.2], [0.8, 0.8], [0.9, 0.8], [0.8, 0.9], [0.5, 0.5], [0.6, 0.5], [0.5, 0.6]];
        m.batches = vec![
            esf::Batch { material: 0, layers: vec![layer(7, [0; 4])], indices: vec![0, 1, 2, 3, 4, 5], lod: None },
            esf::Batch { material: 1, layers: vec![layer(8, [0, 102, 102, 255])], indices: vec![6, 7, 8], lod: None },
        ];
        m
    }

    fn lib() -> esf::Library {
        esf::Library::new()
    }

    /// A colour on a part that is all of an entry recolours its texture (a new
    /// name for it, the other entry's left alone) and its emissive colour; on a
    /// part within a rect it takes the triangles there into a batch of their
    /// own, the rest keeping the texture. Nothing is laid by a colour that
    /// changes nothing, by a texture the library has not, or on an entry the
    /// model has not (the controls: the model as it was).
    #[test]
    fn parts_take_their_colours() {
        let red = render::recolor::DYE << 24 | 0x00_C080;
        let mut m = model();
        assert!(apply(&lib(), &mut m, &[(Kind::Colour, red, Part { entries: vec![1], rects: vec![] })]));
        assert_eq!(m.batches[0].layers[0].texture, 7, "entry 0 its own");
        let t = m.batches[1].layers[0].texture;
        assert_ne!(t, 8);
        assert_eq!(render::recolor::source_of(t), Some((8, red)));
        assert_ne!(m.batches[1].layers[0].emissive, [0, 102, 102, 255], "the glow recoloured too");
        assert_eq!(m.batches[1].layers[0].emissive[3], 255);

        let mut m = model();
        assert!(apply(&lib(), &mut m, &[(Kind::Colour, red, Part { entries: vec![0], rects: vec![[128, 128, 255, 255]] })]));
        assert_eq!(m.batches.len(), 3, "the right triangle split off");
        assert_eq!((m.batches[0].indices.clone(), m.batches[0].layers[0].texture), (vec![0, 1, 2], 7), "the left one kept");
        assert_eq!((m.batches[2].material, m.batches[2].indices.clone()), (0, vec![3, 4, 5]), "the same entry's");
        assert_eq!(render::recolor::source_of(m.batches[2].layers[0].texture), Some((7, red)));

        let before = model();
        for (kind, value, entries) in [(Kind::Colour, 0, vec![0]), (Kind::Texture, 0x9999, vec![0]), (Kind::Colour, red, vec![5])] {
            let mut m = model();
            assert!(!apply(&lib(), &mut m, &[(kind, value, Part { entries, rects: vec![] })]));
            assert_eq!(format!("{:?}", m.batches), format!("{:?}", before.batches), "the control");
        }
    }

    /// A look is drawn on the body it was chosen on, never on another (an
    /// illusion's, an armour body's); the steps on a page make the look of
    /// their colour and texture rows, a first step none; the body row chooses
    /// the body.
    #[test]
    fn looks_go_on_their_own_body() {
        let red = render::recolor::DYE << 24 | 0x00_C080;
        let look = Look { body: 0x4A4D_88FB, parts: vec![(Kind::Colour, red, Part { entries: vec![0], rects: vec![] })] };
        let mut m = model();
        assert!(!dress(&lib(), 0x1234, &mut m, &look), "another body: nothing");
        assert_eq!(format!("{:?}", m.batches), format!("{:?}", model().batches));
        assert!(dress(&lib(), 0x4A4D_88FB, &mut m, &look));

        let step = |name: &str, value: u32| net::raceopts::Step { name: name.into(), value, size: 0 };
        let rows = vec![
            Row { key: "body".into(), label: "Fungus".into(), kind: Kind::Body, parts: vec![], male: vec![step("Blue", 0x4A4D_88FB), step("Red", 0x5874_ACFC)], female: vec![] },
            Row { key: "cap".into(), label: "Cap".into(), kind: Kind::Colour, parts: vec![(0x4A4D_88FB, Part { entries: vec![0], rects: vec![] })], male: vec![step("Natural", 0), step("Red", red)], female: vec![] },
        ];
        assert_eq!(body_of_steps(&rows, 0, &[1, 0]), Some(0x5874_ACFC));
        assert_eq!(look_of_steps(&rows, 0, &[0, 1], 0x4A4D_88FB).parts.len(), 1);
        assert!(look_of_steps(&rows, 0, &[0, 0], 0x4A4D_88FB).parts.is_empty(), "the first step: the body's own");
        assert!(look_of_steps(&rows, 0, &[1, 1], 0x5874_ACFC).parts.is_empty(), "a body the row has no part of");
    }

    /// The looks by name, replaced whole, each change counted; one that does
    /// not read leaves the list (the control).
    #[test]
    fn looks_by_name() {
        let g = generation();
        let mut b = vec![1, 1, 0, 5];
        b.extend(b"Spore");
        b.extend(0x4A4D_88FBu32.to_le_bytes());
        b.push(0);
        take_looks(&b);
        assert_eq!(look_of("Spore").map(|l| l.body), Some(0x4A4D_88FB));
        assert_eq!(generation(), g + 1);
        take_looks(&b);
        assert_eq!(generation(), g + 1, "the same list: no change");
        take_looks(&b[..5]);
        assert!(look_of("Spore").is_some(), "the control: a list cut short leaves the last");
        take_looks(&[1, 0, 0]);
        assert!(look_of("Spore").is_none());
    }
}
