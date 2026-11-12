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
//! What each character chose comes by its name ([`take_looks`], [`look_of`]) and
//! is drawn only on the body it was chosen on ([`dress`]: a player's character
//! in the world, character select's figures): under an illusion, a GM's form or a
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

/// The look of the character of this name made `look` here (select's
/// Customize re-chose its skin): drawn so until the server's next list, which
/// replaces every look whole. None takes its look away.
pub fn set_look_of(name: &str, look: Option<Look>) {
    let changed = LOOKS.with_borrow_mut(|l| match look {
        Some(look) if l.get(name).map(|o| **o != look).unwrap_or(true) => {
            l.insert(name.to_string(), Rc::new(look));
            true
        }
        None => l.remove(name).is_some(),
        _ => false,
    });
    if changed {
        bump();
    }
}

/// The look a character's server look `base` (on `body`) becomes with `rows`
/// re-chosen at the steps `steps` (select's Customize): its parts of those rows
/// (their kind on their parts of the body) taken out and the steps' laid in their
/// place, every other part kept (a Wisp's wisp, laid always, beside its colour
/// re-chosen; a game race's skin beside nothing). None when nothing is laid.
pub fn rechosen(base: Option<&Look>, rows: &[Row], sex: i32, steps: &[usize], body: u32) -> Option<Look> {
    let mine = |k: Kind, p: &Part| rows.iter().any(|r| r.kind == k && r.parts_of(body).any(|q| q == p));
    let mut parts: Vec<(Kind, u32, Part)> = base.filter(|l| l.body == body).map(|l| l.parts.iter().filter(|(k, _, p)| !mine(*k, p)).cloned().collect()).unwrap_or_default();
    parts.extend(look_of_steps(rows, sex, steps, body).parts);
    (!parts.is_empty()).then_some(Look { body, parts })
}

/// [`rechosen`] for the choices Accept hands up (each a row's key and step) of
/// race `race`'s rows, on the look the server told of the character `name`.
pub fn rechosen_by_name(name: &str, race: i32, sex: i32, body: u32, choices: &[(String, u8)]) -> Option<Look> {
    let o = options()?;
    let rows: Vec<Row> = o.rows(race).iter().filter(|r| choices.iter().any(|(k, _)| *k == r.key)).cloned().collect();
    let steps: Vec<usize> = rows.iter().map(|r| choices.iter().find(|(k, _)| *k == r.key).map_or(0, |(_, s)| *s as usize)).collect();
    rechosen(look_of(name).as_deref(), &rows, sex, &steps, body)
}

/// `model` drawn in colour `value` (a race option's colour, `render::recolor`)
/// on every one of its batches: a Wisp's wisp in its colour (`wisp`). Whether
/// anything changed (a colour that changes nothing does not).
pub fn colour_all(lib: &esf::Library, model: &mut esf::Model, value: u32) -> bool {
    let Some(change @ Change::Colour(_)) = Change::of(lib, Kind::Colour, value) else { return false };
    for b in &mut model.batches {
        change.lay(lib, b);
    }
    !model.batches.is_empty()
}

/// The step of each of `rows` that `look` (a character's, as the server told
/// it) lays on `body` for `sex`: the step whose value one of its parts on the
/// body has; the first (the body's own) for a row the look has none of.
pub fn steps_of_look(rows: &[Row], sex: i32, body: u32, look: Option<&Look>) -> Vec<usize> {
    rows.iter()
        .map(|r| {
            let Some(l) = look.filter(|l| l.body == body) else { return 0 };
            let mine: Vec<&Part> = r.parts_of(body).collect();
            l.parts.iter().filter(|(k, _, p)| *k == r.kind && mine.contains(&p)).find_map(|(_, v, _)| r.steps(sex).iter().position(|s| s.value == *v)).unwrap_or(0)
        })
        .collect()
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

/// The scale a body a race's options offer is drawn at against its race's own
/// (go-eqoa-server sends it on the entity channel; the pregame's figures take
/// it here); 1 for any other body.
pub fn body_size(model: u32) -> f32 {
    OPTIONS.with_borrow(|o| {
        o.as_ref().map_or(1.0, |o| {
            o.races.iter().flat_map(|(_, rows)| rows).filter(|r| r.kind == Kind::Body).flat_map(|r| r.male.iter().chain(&r.female)).find(|s| s.value == model && s.size > 0).map_or(1.0, |s| s.size as f32 / 1000.0)
        })
    })
}

/// Whether `model` is a body a race's options offer (a player chose it): it
/// holds a player's items as its race's own does (`racebody`).
pub fn offered_body(model: u32) -> bool {
    OPTIONS.with_borrow(|o| o.as_ref().is_some_and(|o| o.offers_body(model)))
}

/// The race's own body of the race whose options offer `model` (another body
/// of the same creature: `net::raceopts::Options::race_body_of`), for what is
/// chosen per race's body (`raceemote`); None for any other model.
pub fn race_body_of(model: u32) -> Option<u32> {
    OPTIONS.with_borrow(|o| o.as_ref().and_then(|o| o.race_body_of(model)))
}

/// Body `body`'s model in `look`, when the look was chosen on it (on any other
/// body, nothing). Whether anything changed.
pub fn dress(lib: &esf::Library, body: u32, model: &mut esf::Model, look: &Look) -> bool {
    if look.body != body {
        return false;
    }
    // a skin shade only on the body's own skin: the first layers the body is
    // built with, and a face (`skin_layer`); never in the 2002 art, whose
    // bodies lay their entries out otherwise
    let built = skin_shades(&look.parts).then(|| {
        if lib.view_for(body as i32) == esf::Era::Y2002 {
            return std::collections::HashSet::new();
        }
        lib.lookup(body as i32).and_then(|r| lib.build_model(r)).map(|m| m.batches.iter().filter_map(|b| b.layers.first().map(|l| (b.material, l.texture))).collect()).unwrap_or_default()
    });
    // a barbarian's tattoo, never in the 2002 art either (its faces are
    // others'): the parts without it there
    let parts: Vec<(Kind, u32, Part)> = if lib.view_for(body as i32) == esf::Era::Y2002 {
        look.parts.iter().filter(|(k, v, _)| !(*k == Kind::Colour && v >> 24 == render::recolor::TATTOO)).cloned().collect()
    } else {
        look.parts.clone()
    };
    // a face a tattoo was laid on is still a face (the skin's shade goes over
    // the tattoo)
    apply_with(lib, model, &parts, &|entry, texture| {
        let root = render::recolor::root_of(texture);
        vi::cust::is_face(root as u32) || built.as_ref().is_some_and(|b| b.contains(&(entry, texture)))
    })
}

/// Whether any of `parts` is a skin shade (`render::recolor::SKIN`).
fn skin_shades(parts: &[(Kind, u32, Part)]) -> bool {
    parts.iter().any(|(k, v, _)| *k == Kind::Colour && v >> 24 == render::recolor::SKIN)
}

/// The look the steps on a create page make (`steps`, one a row of `rows`, a
/// female's when `sex` is 1) on `body`: each colour or texture row's step on its
/// parts of the body, a first step (the body's own) laying nothing, but a texture
/// row's of one step, which the server lays always (the Wisp's wisp: `wisp`).
pub fn look_of_steps(rows: &[Row], sex: i32, steps: &[usize], body: u32) -> Look {
    let mut parts = Vec::new();
    for (row, &k) in rows.iter().zip(steps) {
        let always = row.kind == Kind::Texture && row.steps(sex).len() == 1;
        if row.kind == Kind::Body || (k == 0 && !always) {
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
    apply_with(lib, model, parts, &|_, _| true)
}

/// [`apply`], a skin shade (and a tattoo) laid only on a batch's first layer
/// whose (palette entry, texture) `skin_layer` takes: the game race's own skin, never an
/// armour's or a robe's texture laid over it (on a body's single-layer limbs
/// and feet `SetArmorSlot` swaps the skin's own texture).
fn apply_with(lib: &esf::Library, model: &mut esf::Model, parts: &[(Kind, u32, Part)], skin_layer: &dyn Fn(i32, i32) -> bool) -> bool {
    let mut changed = false;
    for (kind, value, part) in parts {
        let Some(change) = Change::of(lib, *kind, *value) else { continue };
        let n = model.batches.len();
        for bi in 0..n {
            if !part.entries.iter().any(|&e| e as i32 == model.batches[bi].material) {
                continue;
            }
            // a shade on the body's own skin (`skin_layer`): never on an
            // armour's, a robe's or a helm's (a tattoo picks its layers itself)
            if matches!(change, Change::Skin(_))
                && !model.batches[bi].layers.first().is_some_and(|l| skin_layer(model.batches[bi].material, l.texture))
            {
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
    /// a game race's skin shade (`render::recolor::SKIN`): the first layer's
    /// texture moved to it, the layer's own colour (an erudite's or a
    /// barbarian's skin tint) white, as the shade is the skin's whole colour
    Skin(u32),
    /// a barbarian's tattoo (`render::recolor::TATTOO`): a face's layer with
    /// its woad taken off and the pattern's laid; the man's bare chest's, its
    /// tattoo taken off by none
    Tattoo(u32),
    Texture(i32),
}

impl Change {
    /// A colour that changes something, a texture the library has; else none.
    fn of(lib: &esf::Library, kind: Kind, value: u32) -> Option<Change> {
        match kind {
            Kind::Colour if value >> 24 == render::recolor::SKIN => Some(Change::Skin(value)),
            Kind::Colour if value >> 24 == render::recolor::TATTOO => Some(Change::Tattoo(value)),
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
            Change::Skin(r) => {
                if let Some(l) = b.layers.first_mut() {
                    if l.texture != 0 {
                        l.texture = render::recolor::recolored(l.texture, r, |id| lib.lookup(id).is_some());
                        l.color = [255, 255, 255, l.color[3]];
                    } else {
                        let c = render::recolor::recolor_rgb([l.color[0], l.color[1], l.color[2]], r);
                        l.color = [c[0], c[1], c[2], l.color[3]];
                    }
                }
            }
            Change::Tattoo(r) => {
                // a face's layer and the man's bare chest's: never an
                // armour's, a robe's or a helm's
                for l in &mut b.layers {
                    let root = render::recolor::root_of(l.texture) as u32;
                    if l.texture != 0 && (vi::cust::is_face(root) || render::tattoo::marks(root)) {
                        l.texture = render::recolor::recolored(l.texture, r, |id| lib.lookup(id).is_some());
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
pub(crate) mod tests {
    use super::*;
    use net::raceopts::{Options, Step};

    /// A fungusman's options as go-eqoa-server sends them, cut down: its body
    /// row (two bodies), the cap's colour (a part of the blue body only), the
    /// bag's (the blue body too), the skin's, and a row of one step (shown on
    /// no page).
    pub(crate) fn fungus_options() -> Options {
        let step = |name: &str, value: u32, size: u16| Step { name: name.into(), value, size };
        let colours = vec![step("Natural", 0, 0), step("Crimson", 0x0100_8C79, 0), step("Gold", 0x0120_998F, 0)];
        let blue = |entries: Vec<u8>| (0x4A4D_88FB, Part { entries, rects: vec![] });
        let row = |key: &str, kind: Kind, parts, male| Row { key: key.into(), label: key.to_uppercase(), kind, parts, male, female: vec![] };
        Options {
            races: vec![(
                28,
                vec![
                    row("body", Kind::Body, vec![], vec![step("Blue cap", 0x4A4D_88FB, 1000), step("Red cap", 0x5874_ACFC, 1000)]),
                    row("cap", Kind::Colour, vec![blue(vec![0]), (0x5874_ACFC, Part { entries: vec![0], rects: vec![[115, 115, 255, 255]] })], colours.clone()),
                    row("bag", Kind::Colour, vec![blue(vec![2])], colours.clone()),
                    row("skin", Kind::Colour, vec![blue(vec![1]), (0x5874_ACFC, Part { entries: vec![0], rects: vec![[0, 0, 100, 100]] })], colours.clone()),
                    row("one", Kind::Colour, vec![blue(vec![3])], vec![step("Natural", 0, 0)]),
                ],
            )],
        }
    }


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
        // a texture row of one step is laid at its first (the server lays it
        // always: the Wisp's wisp); the control, a colour row's first, is not
        let wisp = Row { key: "wisp".into(), label: "Wisp".into(), kind: Kind::Texture, parts: vec![(0x70D8_98C6, Part { entries: vec![2], rects: vec![] })], male: vec![step("Will o' wisp", 0x8143_DC70)], female: vec![] };
        let laid = look_of_steps(&[wisp.clone()], 0, &[0], 0x70D8_98C6);
        assert_eq!(laid.parts.len(), 1);
        assert_eq!((laid.parts[0].0, laid.parts[0].1), (Kind::Texture, 0x8143_DC70));
        assert!(look_of_steps(&[rows[1].clone()], 0, &[0], 0x4A4D_88FB).parts.is_empty(), "the control: a colour row's first");
        assert_eq!(look_of_steps(&rows, 0, &[0, 1], 0x4A4D_88FB).parts.len(), 1);
        assert!(look_of_steps(&rows, 0, &[0, 0], 0x4A4D_88FB).parts.is_empty(), "the first step: the body's own");
        assert!(look_of_steps(&rows, 0, &[1, 1], 0x5874_ACFC).parts.is_empty(), "a body the row has no part of");
    }

    /// Select's Customize re-chooses rows on the look the server told: a Wisp's
    /// colour taken out and laid again, its wisp (laid always, on the same part
    /// but a texture) kept; Natural leaves the wisp alone; Accept's choices by
    /// name do the same. The control: a look with nothing of the rows keeps all
    /// of it and adds the steps', and on another body the base is dropped.
    #[test]
    fn rechosen_keeps_what_customize_did_not_choose() {
        let step = |name: &str, value: u32| net::raceopts::Step { name: name.into(), value, size: 0 };
        let (human, at) = (0x70D8_98C6u32, Part { entries: vec![2], rects: vec![] });
        let (gold, rose) = (render::recolor::DYE << 24 | 0x22_D99A, render::recolor::DYE << 24 | 0xEA_B399);
        let colour = Row { key: "wispcolor".into(), label: "Color".into(), kind: Kind::Colour, parts: vec![(human, at.clone())], male: vec![step("Natural", 0), step("Gold", gold), step("Rose", rose)], female: vec![] };
        let wisp = (Kind::Texture, 0x8143_DC70, at.clone());
        let base = Look { body: human, parts: vec![wisp.clone(), (Kind::Colour, gold, at.clone())] };
        let got = rechosen(Some(&base), std::slice::from_ref(&colour), 0, &[2], human).unwrap();
        assert_eq!(got.parts, vec![wisp.clone(), (Kind::Colour, rose, at.clone())], "Rose, the wisp kept");
        let got = rechosen(Some(&base), std::slice::from_ref(&colour), 0, &[0], human).unwrap();
        assert_eq!(got.parts, vec![wisp.clone()], "Natural: the wisp alone");
        let other = Look { body: human, parts: vec![(Kind::Colour, gold, Part { entries: vec![5], rects: vec![] })] };
        assert_eq!(rechosen(Some(&other), std::slice::from_ref(&colour), 0, &[1], human).unwrap().parts.len(), 2, "the control: nothing of the row");
        assert_eq!(rechosen(Some(&Look { body: 0x1234, ..base.clone() }), std::slice::from_ref(&colour), 0, &[1], human).unwrap().parts.len(), 1, "another body's look");
        assert!(rechosen(None, std::slice::from_ref(&colour), 0, &[0], human).is_none());

        let o = net::raceopts::Options { races: vec![(31, vec![Row { key: "wisp".into(), label: "Wisp".into(), kind: Kind::Texture, parts: vec![(human, at.clone())], male: vec![step("Will o' wisp", 0x8143_DC70)], female: vec![] }, colour.clone()])] };
        set_options(Some(&o));
        set_look_of("Glim", Some(base.clone()));
        let got = rechosen_by_name("Glim", 31, 0, human, &[("wispcolor".into(), 2)]).unwrap();
        assert_eq!(got.parts, vec![wisp, (Kind::Colour, rose, at)], "by name, on the server's look");
        set_look_of("Glim", None);
        set_options(None);
    }

    /// The library of the disc's bodies and their textures, or None (no
    /// `run/eqoa.iso`).
    fn disc_lib() -> Option<esf::Library> {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let mut iso = iso::Iso::open(&p).ok()?;
        let mut lib = esf::Library::new();
        for f in crate::dressview::FILES {
            lib.add(*f, esf::Container::open(iso.read(f).ok()?).ok()?);
        }
        Some(lib)
    }

    /// On the disc: the fungusman's body in every row's first step is the body
    /// as built, batch for batch (no part takes anything); its cap in a colour
    /// recolours only the cap's triangles (split off the fungus's own entry,
    /// the cap's top, and its gills off the skin's), each under a texture of
    /// its own name, and every other triangle keeps its texture; the textures
    /// as the renderer reads them (`LibraryTextures`): the recoloured one is
    /// its source's levels recoloured under another key at the source's size,
    /// the source itself unchanged (the control). A body the look was not
    /// chosen on takes nothing (a human's). Needs `run/eqoa.iso`.
    #[test]
    fn a_look_on_the_discs_fungusman() {
        use render::TextureSource;
        let Some(lib) = disc_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let opts = fungus_options();
        let rows = opts.rows(28);
        let body = 0x4A4D_88FBu32;
        let plain = lib.build_model(lib.lookup(body as i32).unwrap()).unwrap();
        let mut natural = plain.clone();
        assert!(!dress(&lib, body, &mut natural, &look_of_steps(rows, 0, &[0, 0, 0, 0, 0], body)));
        assert_eq!(format!("{:?}", natural.batches), format!("{:?}", plain.batches), "every first step: the body as built");

        let mut red = plain.clone();
        assert!(dress(&lib, body, &mut red, &look_of_steps(rows, 0, &[0, 1, 0, 0, 0], body)));
        let tris = |m: &esf::Model| m.batches.iter().map(|b| b.indices.len() / 3).sum::<usize>();
        assert_eq!(tris(&red), tris(&plain), "no triangle lost or doubled");
        let cap = red.batches.iter().filter(|b| b.material == 0 && render::recolor::source_of(b.layers[0].texture).is_some()).count();
        assert_eq!(cap, 1, "the fungus's own entry: all of it the cap here");
        for b in &red.batches {
            let recoloured = render::recolor::source_of(b.layers[0].texture);
            match b.material {
                0 => assert!(recoloured.is_some(), "the fungus entry (the cap) recoloured"),
                _ => assert!(recoloured.is_none(), "entry {} kept", b.material),
            }
        }
        let id = red.batches.iter().find(|b| b.material == 0).unwrap().layers[0].texture;
        let (source, r) = render::recolor::source_of(id).unwrap();
        assert_eq!(source, 0x3721_EB55u32 as i32);
        let textures = render::library_textures(&lib);
        let (mine, theirs) = (textures.levels(id), textures.levels(source));
        assert_eq!(mine.len(), theirs.len());
        let mut want = theirs[0].rgba.clone();
        render::recolor::recolor_rgba(&mut want, r);
        assert_eq!(mine[0].rgba, want, "the source's texels recoloured");
        assert_ne!(mine[0].rgba, theirs[0].rgba);
        assert_ne!(textures.key(id), textures.key(source));
        assert_eq!(textures.native_size(id), textures.native_size(source));
        assert_eq!(textures.levels(source)[0].rgba, theirs[0].rgba, "the control: the source as it was");

        let human = 0x70D8_98C6u32;
        let as_built = lib.build_model(lib.lookup(human as i32).unwrap()).unwrap();
        let mut m = as_built.clone();
        assert!(!dress(&lib, human, &mut m, &look_of_steps(rows, 0, &[0, 1, 1, 1, 0], body)));
        assert_eq!(format!("{:?}", m.batches), format!("{:?}", as_built.batches), "a human: nothing");
    }

    /// The human's and the erudite's skin rows as go-eqoa-server sends them
    /// (`entity/skinshades.go`), cut to three shades: Natural, Light, Dark.
    pub(crate) fn skin_options() -> Options {
        let step = |name: &str, value: u32| Step { name: name.into(), value, size: 0 };
        let shades = vec![step("Natural", 0), step("Light", 0x030C_7AAB), step("Dark", 0x0313_6B3B)];
        let row = |parts: Vec<(u32, Vec<u8>)>| Row { key: "skin".into(), label: "Skin".into(), kind: Kind::Colour, parts: parts.into_iter().map(|(b, entries)| (b, Part { entries, rects: vec![] })).collect(), male: shades.clone(), female: vec![] };
        Options {
            races: vec![
                (0, vec![row(vec![(0x70D8_98C6, vec![0, 2, 3, 4, 5]), (0x0D53_7335, vec![3, 5, 6, 7, 9])])]),
                (3, vec![row(vec![(0xA99C_6B15, vec![2, 3, 4, 5, 6]), (0x4C40_84FC, vec![2, 5, 6, 7, 8])])]),
                (8, vec![row(vec![(0x320C_0B47, vec![0, 6, 8, 9, 10]), (0x4F1A_3CBE, vec![0, 4, 6, 8, 9])])]),
            ],
        }
    }

    /// A skin shade on the disc's bodies: on a human man the first layer of
    /// each skin entry takes a shaded copy of its texture and a white colour,
    /// the second (the armour's) is left as it is; an erudite's tint (its
    /// layer colour) goes white, the shade being the skin's whole colour. On a
    /// gnome's single-layer hands an armour's texture (gloves) is left as it
    /// is, its bare hands shaded; a face another face option laid is shaded.
    /// The controls: Natural changes nothing; a body the look was not chosen
    /// on, nothing; a colour of another mode on the same parts recolours every
    /// layer. Needs `run/eqoa.iso`.
    #[test]
    fn a_skin_shade_on_the_discs_bodies() {
        let Some(lib) = disc_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let opts = skin_options();
        let built = |body: u32| lib.build_model(lib.lookup(body as i32).unwrap()).unwrap();
        let human = 0x70D8_98C6u32;
        let plain = built(human);
        let mut dark = plain.clone();
        assert!(dress(&lib, human, &mut dark, &look_of_steps(opts.rows(0), 0, &[2], human)));
        for (a, b) in plain.batches.iter().zip(&dark.batches) {
            let skin = [0, 2, 3, 4, 5].contains(&a.material);
            let shaded = render::recolor::source_of(b.layers[0].texture).map(|(s, _)| s);
            assert_eq!(shaded, skin.then_some(a.layers[0].texture), "entry {}", a.material);
            if skin {
                assert_eq!(b.layers[0].color[..3], [255, 255, 255]);
            }
            for (la, lb) in a.layers.iter().zip(&b.layers).skip(1) {
                assert_eq!(la.texture, lb.texture, "entry {}: the armour's layer kept", a.material);
            }
        }
        let mut natural = plain.clone();
        assert!(!dress(&lib, human, &mut natural, &look_of_steps(opts.rows(0), 0, &[0], human)));
        assert_eq!(format!("{:?}", natural.batches), format!("{:?}", plain.batches), "the control: Natural");
        let ogre = built(0x8480_7490);
        let mut m = ogre.clone();
        assert!(!dress(&lib, 0x8480_7490, &mut m, &look_of_steps(opts.rows(0), 0, &[2], human)), "the control: another body");

        let erudite = 0x320C_0B47u32;
        let plain = built(erudite);
        assert_eq!(plain.batches.iter().find(|b| b.material == 6).unwrap().layers[0].color[..3], [109, 83, 77], "the erudite's tint");
        let mut light = plain.clone();
        assert!(dress(&lib, erudite, &mut light, &look_of_steps(opts.rows(8), 0, &[1], erudite)));
        let chest = light.batches.iter().find(|b| b.material == 6).unwrap();
        assert_eq!(chest.layers[0].color[..3], [255, 255, 255], "its tint white under the shade");
        assert!(render::recolor::source_of(chest.layers[0].texture).is_some());

        // a gnome man's hands (entry 6, one layer) in gloves: the gloves kept;
        // its face (entry 4) as another face option laid it: shaded
        let gnome = 0xA99C_6B15u32;
        let mut gloved = built(gnome);
        let gloves = 0x3857_E856;
        let other_face = 0xAB3E_7194u32 as i32;
        for b in gloved.batches.iter_mut() {
            match b.material {
                6 => b.layers[0].texture = gloves,
                4 => b.layers[0].texture = other_face,
                _ => {}
            }
        }
        assert!(dress(&lib, gnome, &mut gloved, &look_of_steps(opts.rows(3), 0, &[2], gnome)));
        for b in &gloved.batches {
            match b.material {
                6 => assert_eq!(b.layers[0].texture, gloves, "the gloves kept"),
                4 => assert_eq!(render::recolor::source_of(b.layers[0].texture).map(|(s, _)| s), Some(other_face), "the face shaded"),
                5 => assert!(render::recolor::source_of(b.layers[0].texture).is_some(), "the bare forearm shaded"),
                _ => {}
            }
        }
        // the control: a dye on the same parts is laid on every layer
        let mut dyed = built(human);
        let red = render::recolor::DYE << 24 | 0x00_C080;
        assert!(apply(&lib, &mut dyed, &[(Kind::Colour, red, Part { entries: vec![5], rects: vec![] })]));
        let chest = dyed.batches.iter().find(|b| b.material == 5).unwrap();
        assert!(chest.layers.iter().all(|l| render::recolor::source_of(l.texture).is_some()), "a dye: both layers");
    }

    /// The barbarian's rows as go-eqoa-server sends them to a client that
    /// answered 3 (`entity/tattoos.go`, `skinshades.go`): the tattoo, then the
    /// skin cut to Natural and Dark.
    pub(crate) fn barbarian_options() -> Options {
        let step = |name: &str, value: u32| Step { name: name.into(), value, size: 0 };
        let parts = |p: Vec<(u32, Vec<u8>)>| p.into_iter().map(|(b, entries)| (b, Part { entries, rects: vec![] })).collect::<Vec<_>>();
        let tattoo = Row {
            key: "tattoo".into(),
            label: "Tattoo".into(),
            kind: Kind::Colour,
            parts: parts(vec![(0x61CA_3EDF, vec![9, 5]), (0xC44A_D516, vec![6])]),
            male: vec![step("Face's own", 0), step("Off", 0x0400_0000), step("Bands", 0x0401_0000), step("Crest", 0x0402_0000)],
            female: vec![step("Off", 0), step("Bands", 0x0401_0000), step("Crest", 0x0402_0000)],
        };
        let skin = Row {
            key: "skin".into(),
            label: "Skin".into(),
            kind: Kind::Colour,
            parts: parts(vec![(0x61CA_3EDF, vec![0, 5, 7, 8, 9]), (0xC44A_D516, vec![0, 4, 6, 7, 8])]),
            male: vec![step("Natural", 0), step("Dark", 0x0313_6B3B)],
            female: vec![],
        };
        Options { races: vec![(6, vec![tattoo, skin])] }
    }

    /// A barbarian's tattoo on the disc's bodies, drawn through the library
    /// (`render::LibraryTextures`): a man in face 1 (the bands) with Off has
    /// his face's woad taken off (its brow lighter), the rest of the face byte
    /// for byte, and his bare chest's tattoo off; a woman in face 0 with the crest has it laid (her brow
    /// darker), her cheek byte for byte; the crest and a dark skin on a man: the
    /// shade laid over the tattooed face, his chest's tattoo kept. The
    /// controls: the face's own (his first step) changes nothing; a texture on
    /// the face's entry that is not a face (a helm's, say) and an armour's chest
    /// take no tattoo. Needs `run/eqoa.iso`.
    #[test]
    fn a_tattoo_on_the_discs_barbarians() {
        use render::TextureSource;
        let Some(lib) = disc_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let opts = barbarian_options();
        let rows = opts.rows(6);
        let (man, woman) = (0x61CA_3EDFu32, 0xC44A_D516u32);
        let with_face = |body: u32, entry: i32, face: u32| {
            let mut m = lib.build_model(lib.lookup(body as i32).unwrap()).unwrap();
            m.batches.iter_mut().filter(|b| b.material == entry).for_each(|b| b.layers[0].texture = face as i32);
            m
        };
        let face_of = |m: &esf::Model, entry: i32| m.batches.iter().find(|b| b.material == entry).unwrap().layers[0].texture;
        let tex = render::LibraryTextures(&lib);
        let texel = |id: i32, x: u32, y: u32| {
            let l = tex.levels(id);
            let im = &l[0];
            let (x, y) = (x * im.width / 64, y * im.height / 64);
            let o = ((y * im.width + x) * 4) as usize;
            [im.rgba[o] as u32, im.rgba[o + 1] as u32, im.rgba[o + 2] as u32]
        };
        let sum = |c: [u32; 3]| c.iter().sum::<u32>();

        let bands = render::tattoo::BANDS_FACE;
        let mut off = with_face(man, 9, bands);
        assert!(dress(&lib, man, &mut off, &look_of_steps(rows, 0, &[1, 0], man)));
        let t = face_of(&off, 9);
        assert_eq!(render::recolor::source_of(t), Some((bands as i32, 0x0400_0000)));
        assert!(sum(texel(t, 20, 5)) > sum(texel(bands as i32, 20, 5)) + 60, "his brow's woad off");
        assert_eq!(texel(t, 5, 60), texel(bands as i32, 5, 60), "his chin as it was");
        // Off: his bare chest's tattoo off too, the skin under it kept
        let chest = off.batches.iter().find(|b| b.material == 5).unwrap();
        let art = render::tattoo::CHEST_ART as i32;
        assert_eq!(render::recolor::source_of(chest.layers[1].texture), Some((art, 0x0400_0000)));
        assert_eq!(chest.layers[0].texture, 0xB037_9A45u32 as i32, "the skin's layer kept");
        let alpha = |id: i32, x: u32, y: u32| {
            let im = &tex.levels(id)[0];
            im.rgba[(((y * im.width / 128) * im.width + x * im.width / 128) * 4 + 3) as usize]
        };
        assert!(alpha(art, 10, 90) > 0, "the swirl on the disc");
        assert_eq!(alpha(chest.layers[1].texture, 10, 90), 0, "the swirl off");

        let her_face = 0x28F1_9AC8u32;
        let mut crest = with_face(woman, 6, her_face);
        assert!(dress(&lib, woman, &mut crest, &look_of_steps(rows, 1, &[2, 0], woman)));
        let t = face_of(&crest, 6);
        assert!(sum(texel(t, 30, 5)) + 60 < sum(texel(her_face as i32, 30, 5)), "her brow in woad");
        assert_eq!(texel(t, 5, 60), texel(her_face as i32, 5, 60), "her cheek as it was");

        let mut dark = with_face(man, 9, 0xD9EC_0301);
        assert!(dress(&lib, man, &mut dark, &look_of_steps(rows, 0, &[3, 1], man)));
        let chest = dark.batches.iter().find(|b| b.material == 5).unwrap();
        assert_eq!(alpha(chest.layers[1].texture, 10, 90), alpha(art, 10, 90), "a pattern keeps the swirl");
        let (under, shade) = render::recolor::source_of(face_of(&dark, 9)).unwrap();
        assert_eq!((render::recolor::source_of(under).map(|(s, r)| (s as u32, r)), shade), (Some((0xD9EC_0301, 0x0402_0000)), 0x0313_6B3B), "the shade over the tattoo");

        let mut own = with_face(man, 9, bands);
        assert!(!dress(&lib, man, &mut own, &look_of_steps(rows, 0, &[0, 0], man)), "the control: the face's own");
        assert_eq!(face_of(&own, 9), bands as i32);
        let helm = 0x0C6F_2FDAu32;
        let mut hidden = with_face(man, 9, helm);
        let plate = 0x39BC_F85Fu32 as i32;
        hidden.batches.iter_mut().filter(|b| b.material == 5).for_each(|b| b.layers[1].texture = plate);
        dress(&lib, man, &mut hidden, &look_of_steps(rows, 0, &[1, 0], man));
        assert_eq!(face_of(&hidden, 9), helm as i32, "the control: not a face");
        assert_eq!(hidden.batches.iter().find(|b| b.material == 5).unwrap().layers[1].texture, plate, "the control: an armour's chest");
    }

    /// Drawn (`eqoa dress`, the GPU): the fungusman in every row's first step
    /// is the plain body's picture byte for byte; its cap in a colour is
    /// another picture (the control). Needs `run/eqoa.iso` and a GPU.
    #[test]
    fn the_first_steps_draw_the_body_as_it_is() {
        let Some(lib) = disc_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        set_options(Some(&fungus_options()));
        let dir = std::env::temp_dir().join(format!("raceopts-shots-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let shot = |model: u32, words: &str, name: &str| -> Option<Vec<u8>> {
            let w = crate::dressview::parse(&words.split_whitespace().map(String::from).collect::<Vec<_>>()).unwrap();
            let out = dir.join(name);
            match crate::dressview::shot(&lib, model as i32, &w, out.to_str().unwrap()) {
                Ok(()) => std::fs::read(&out).ok(),
                Err(e) => {
                    eprintln!("SKIP the drawing: {e}");
                    None
                }
            }
        };
        let fungus = 0x4A4D_88FB;
        let Some(plain) = shot(fungus, "size=120x180", "plain.png") else { return };
        assert_eq!(shot(fungus, "size=120x180 opts=28:0,0,0,0,0", "natural.png").unwrap(), plain, "every first step");
        assert_ne!(shot(fungus, "size=120x180 opts=28:0,1,0,0,0", "crimson.png").unwrap(), plain, "the control: a crimson cap");
        let _ = std::fs::remove_dir_all(&dir);
        set_options(None);
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

    /// A body a race's options offer holds a player's items as the race's own
    /// does (`racebody`) and stands at its step's size; with no options (a
    /// server that sent none, the control) it is a creature's body like any
    /// other, at its own size; a body no step names never is.
    #[test]
    fn an_offered_body_holds_items() {
        set_options(None);
        assert!(!crate::racebody::worn_by_a_race(0x5874_ACFC, None), "the control: no options");
        assert_eq!(body_size(0x5874_ACFC), 1.0);
        let mut o = fungus_options();
        o.races[0].1[0].male[1].size = 910;
        set_options(Some(&o));
        assert!(crate::racebody::worn_by_a_race(0x5874_ACFC, None));
        assert!(crate::racebody::worn_by_a_race(0x4A4D_88FB, None));
        assert!(!crate::racebody::worn_by_a_race(0x1234_5678, None), "a body no step names");
        assert_eq!(body_size(0x5874_ACFC), 0.91);
        assert_eq!(body_size(0x1234_5678), 1.0);
        set_options(None);
        assert!(!crate::racebody::worn_by_a_race(0x5874_ACFC, None));
    }
}
