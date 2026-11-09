//! The Wisp, a race of go-eqoa-server's own (race 31; the owner, 2026-11-09: "lets
//! add a wisp also as a playable race, the armor will form like a human and also
//! the weapons swing like humans. like when humans are invis, but the wisp is inside
//! the helm").
//!
//! A Wisp is a human underneath: its body is a human's of its sex (the character's
//! model, so a PS2 draws a human in its armour), which walks, swings its weapons and
//! emotes with the human's own animations. The server marks it with a race option
//! laid always (`raceopts`: a texture row "wisp" of one step, whose value is the will
//! o' wisp's body, CHAR.ESF 0x8143DC70, the one the world's wisps and orbs of light
//! wear), so every client that asked for the looks knows its Wisps by name, at
//! character select and in the world; a client that does not draw them lays nothing
//! (the value is no texture). Here such a body is drawn as its armour alone, the way
//! CLIENT's invisibility shows only what hangs on a body, but the worn slots kept:
//! the triangles of the palette entries its chest, bracer, gloves, legs and boots
//! slots name, where it wears something there (a robe its chest and legs), and
//! nothing of its skin, face or hair ([`strip`]). The wisp hangs where the helm does,
//! at the head's top node, inside any helm, sized to a head ([`mesh`]); the held
//! items and the helm are attachments and stay. With nothing worn it is the wisp
//! floating where a human's head would be, its hands' weapons still swinging.

use esf::Library;
use net::raceopts::{Kind, Look};
use vi::csprite::CSprite;
use vi::fpu::Ee;

/// The wisp's size, across its widest: about a human's head (HumanMale's skull is
/// about 0.3 from ear to ear in its bind pose).
pub const HEAD: f32 = 0.34;

/// The wisp a server's race look carries: the value of a texture part that names a
/// body (a CSprite record) rather than a texture; none for any other look.
pub fn of(lib: &Library, look: &Look) -> Option<u32> {
    look.parts.iter().find(|(k, v, _)| *k == Kind::Texture && is_body(lib, *v)).map(|(_, v, _)| *v)
}

/// Whether `id` is a CSprite record (0x2700) in the library.
fn is_body(lib: &Library, id: u32) -> bool {
    lib.lookup(id as i32).is_some_and(|r| lib.file(r.file).node(r.node).ty == 0x2700)
}

/// The palette entries of the worn slots `look` covers on `cs`: chest, bracer,
/// gloves, legs and boots with an armour set, a robe's chest and legs.
pub fn worn_entries(cs: &CSprite<Ee>, look: &vi::cust::Look) -> Vec<i32> {
    (0..5)
        .filter(|&i| look.armor[i] != 0 || (look.robe >= 0 && (i == 0 || i == 3)))
        .map(|i| cs.text_slots[i].0)
        .filter(|&e| e >= 0)
        .collect()
}

/// The body as a Wisp: only its worn slots' triangles (its armour, a robe), the rest
/// (its skin, face, hair, the bare slots) taken out. Whether any is left.
pub fn strip(model: &mut esf::Model, cs: &CSprite<Ee>, look: &vi::cust::Look) -> bool {
    let keep = worn_entries(cs, look);
    model.batches.retain(|b| keep.contains(&b.material));
    !model.batches.is_empty()
}

/// The node the wisp hangs on: the one a helm goes on (NodeIDList slot 8), else the
/// head's (7); -1 none.
pub fn node(cs: &CSprite<Ee>) -> i32 {
    if cs.node_ids[8] >= 0 { cs.node_ids[8] } else { cs.node_ids[7] }
}

/// The wisp's body as a rigid attachment: its mesh as built (the bind pose, in the
/// model's own space), centred on its middle and scaled to [`HEAD`], unskinned.
pub fn mesh(lib: &Library, id: u32) -> Option<esf::Model> {
    let mut m = lib.build_model(lib.lookup(id as i32)?)?;
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &m.pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    if m.pos.is_empty() || lo[0] > hi[0] {
        return None;
    }
    let mid: [f32; 3] = std::array::from_fn(|k| (lo[k] + hi[k]) * 0.5);
    let s = HEAD / (0..3).map(|k| hi[k] - lo[k]).fold(0f32, f32::max).max(1e-3);
    for p in &mut m.pos {
        for k in 0..3 {
            p[k] = (p[k] - mid[k]) * s;
        }
    }
    m.skinned = false;
    m.lod_dists.clear();
    for b in &mut m.batches {
        b.lod = None;
    }
    Some(m)
}

/// A Wisp's body dressed: `model` (the human's, its look laid) stripped to its armour
/// and the wisp to hang on [`node`], when `race_look` (on `body`, the drawn one) has
/// a wisp; else nothing changes and none.
pub fn dress(lib: &Library, body: u32, model: &mut esf::Model, cs: &CSprite<Ee>, look: &vi::cust::Look, race_look: Option<&Look>) -> Option<(i32, esf::Model)> {
    let rl = race_look.filter(|l| l.body == body)?;
    let w = of(lib, rl)?;
    strip(model, cs, look);
    let n = node(cs);
    if n < 0 {
        return None;
    }
    mesh(lib, w).map(|m| (n, m))
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::raceopts::Part;
    use vi::character::{AnimCache, load_csprite};
    use vi::csprite::Rand;

    const HUMAN: u32 = 0x70D8_98C6;
    const WISP: u32 = 0x8143_DC70;

    fn disc_lib() -> Option<Library> {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let mut iso = iso::Iso::open(&p).ok()?;
        let mut lib = Library::new();
        for f in crate::dressview::FILES {
            lib.add(*f, esf::Container::open(iso.read(f).ok()?).ok()?);
        }
        Some(lib)
    }

    fn human(lib: &Library) -> (esf::Model, CSprite<Ee>) {
        let r = lib.lookup(HUMAN as i32).unwrap();
        let cs = load_csprite(lib, r, &mut vi::hsprite::AnimScene::default(), &mut AnimCache::new(), &mut Rand(1)).unwrap();
        (lib.build_model(r).unwrap(), cs)
    }

    fn look(body: u32, value: u32) -> Look {
        Look { body, parts: vec![(Kind::Texture, value, Part { entries: vec![2], rects: vec![] })] }
    }

    /// On the disc: a look whose texture part names the will o' wisp's body is a
    /// Wisp's, one naming a texture (a hair) is not; a Wisp in plate keeps only its
    /// five worn slots' triangles (none of its head, its hair or its bare skin), in a
    /// chest alone only the chest's, bare nothing; the wisp is a rigid model a head
    /// across, centred, on the helm's node. The control: a human's own look (no wisp)
    /// leaves the body as it was.
    #[test]
    fn a_wisp_is_its_armour_and_the_wisp_in_the_helm() {
        let Some(lib) = disc_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        assert_eq!(of(&lib, &look(HUMAN, WISP)), Some(WISP));
        assert_eq!(of(&lib, &look(HUMAN, 0x98F8_6489)), None, "a hair's texture is no wisp");
        let (built, cs) = human(&lib);
        let plate = vi::cust::Look { armor: [4; 6], robe: -1, ..Default::default() };
        let mut m = built.clone();
        let (n, w) = dress(&lib, HUMAN, &mut m, &cs, &plate, Some(&look(HUMAN, WISP))).expect("a wisp");
        assert_eq!(n, cs.node_ids[8], "the helm's node");
        let slots: Vec<i32> = (0..5).map(|i| cs.text_slots[i].0).collect();
        assert!(!m.batches.is_empty() && m.batches.iter().all(|b| slots.contains(&b.material)), "the worn slots only");
        assert!(m.batches.len() < built.batches.len(), "the head, hair and skin taken out");
        let mut chest = built.clone();
        strip(&mut chest, &cs, &vi::cust::Look { armor: [4, 0, 0, 0, 0, 0], robe: -1, ..Default::default() });
        assert!(chest.batches.iter().all(|b| b.material == cs.text_slots[0].0));
        let mut bare = built.clone();
        assert!(!strip(&mut bare, &cs, &vi::cust::Look { robe: -1, ..Default::default() }), "bare: nothing but the wisp");
        assert!(!w.skinned && !w.pos.is_empty());
        let (lo, hi) = w.pos.iter().fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), p| (std::array::from_fn(|k| lo[k].min(p[k])), std::array::from_fn(|k| hi[k].max(p[k]))));
        let across = (0..3).map(|k| hi[k] - lo[k]).fold(0f32, f32::max);
        assert!((across - HEAD).abs() < 1e-3 && (0..3).all(|k| (lo[k] + hi[k]).abs() < 1e-3), "{across} across, centred {lo:?} {hi:?}");
        // the control: no wisp in the look, the body as it was
        let mut plain = built.clone();
        assert!(dress(&lib, HUMAN, &mut plain, &cs, &plate, Some(&look(HUMAN, 0x98F8_6489))).is_none());
        assert_eq!(format!("{:?}", plain.batches), format!("{:?}", built.batches));
        assert!(dress(&lib, HUMAN, &mut plain, &cs, &plate, Some(&look(0x1234, WISP))).is_none(), "a look on another body");
    }
}
