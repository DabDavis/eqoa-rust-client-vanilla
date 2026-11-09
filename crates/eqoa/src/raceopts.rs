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
    look.body == body && apply(lib, model, &look.parts)
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
