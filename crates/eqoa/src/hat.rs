//! A hat another body has built in, worn on a character's head (go-eqoa-server's
//! aura kind hat, `net::npctint::Aura::HAT`, the helm's aura alone; the owner,
//! 2026-11-16, of Hoofbite: "for his monk hat lets use the kappas built in hat
//! that is wide brimmed for his model").
//!
//! No helm on the disc is a hat: the kappa's wide-brimmed straw hat, with its
//! steel spike, is part of a kappa's own mesh (CHAR.ESF 0xF0FAB933), skinned to
//! its head. [`cut`] takes it out of the source body: the triangles every
//! vertex of which rides the head (its head node, `node_ids[7]`, or a node
//! under it) that are not of the face's texture (the texture most of the
//! head's triangles are in: the face, the eyes and the mouth), in the bind
//! pose, as a rigid model. [`hat_on`] seats it on the wearer: from the source
//! body's head to the wearer's, as each wears armour set 4's helm (the measure
//! the circlet takes, `aura::CIRCLET_MEASURE_SET`, seated as worn:
//! `slotless::helm_on`), its middle on the helm's middle and scaled by the
//! helms' widths, then into the frame of the node the wearer's helm hangs on.
//! Drawn in place of the helm (`play::attach`): a hat counts as a helm worn,
//! so the hair is dressed as under one.
//!
//! Everything is measured in the bind pose's model space, upright and facing
//! the same way for every body, never in a head node's own frame, whose axes
//! differ from skeleton to skeleton.

use esf::Library;
use glam::{Mat4, Vec3};
use vi::csprite::CSprite;
use vi::fpu::Ee;

/// The kappa whose straw hat go-eqoa-server's named look "kappa hat" wears.
pub const KAPPA_HAT: u32 = 0xF0FA_B933;

/// The efreeti lord (Efreeti Lord Djarn's body) whose turban Dust's Turban of
/// Twisted Virtue is (the owner, 2026-11-16: "lets also cut the turban from
/// the efreeti lord as a turban for dust in his inventory").
pub const EFREETI_LORD: u32 = 0xAA48_9CBB;

/// The bodies whose hats are known, and the textures their hats are in: of
/// these the hat is the head's triangles in those textures alone, where the
/// face rule ([`cut`]) does not hold (the efreeti lord's turban has more of
/// his head's triangles than his face has, so the rule would take his face,
/// eyes and beard for the hat). The kappa's straw (E81D726E) and steel spike
/// (3E30193D, the atlas its bracers share); the efreeti lord's turban
/// (0808CC05, the atlas his shorts and arm bands share; his face is FB130901).
pub const HATS: &[(u32, &[u32])] = &[(KAPPA_HAT, &[0xE81D_726E, 0x3E30_193D]), (EFREETI_LORD, &[0x0808_CC05])];

/// The textures body `src`'s hat is in, when [`HATS`] knows it.
pub fn hat_textures(src: u32) -> Option<&'static [u32]> {
    HATS.iter().find(|(b, _)| *b == src).map(|(_, t)| *t)
}

/// A node's bind-pose world matrix (model space), glam's.
fn bind_world(cs: &CSprite<Ee>, node: i32) -> Option<Mat4> {
    let n = cs.h.nodes.get(usize::try_from(node).ok()?)?;
    Some(Mat4::from_cols_array_2d(&n.inv_bind_world.to_cols()).inverse())
}

/// Whether `node` is `head` or a node under it.
fn under(cs: &CSprite<Ee>, mut node: i32, head: i32) -> bool {
    for _ in 0..cs.h.nodes.len() {
        if node == head {
            return true;
        }
        match usize::try_from(node).ok().and_then(|i| cs.h.nodes.get(i)) {
            Some(n) => node = n.parent,
            None => return false,
        }
    }
    false
}

/// The hat `src` (the source body's model, skinned, as `build_model` gives it;
/// `cs` its sprite) has built in, in its bind pose's model space, as a rigid
/// model: see the module; with `textures` (a body [`HATS`] knows), the head's
/// triangles in those textures alone. None when the body has no head node or
/// nothing on its head but its face. Of a body with levels of detail, the
/// nearest level.
pub fn cut(src: &esf::Model, cs: &CSprite<Ee>, textures: Option<&[u32]>) -> Option<esf::Model> {
    let head = cs.node_ids[7];
    if head < 0 || src.bones.len() != src.pos.len() || src.weights.len() != src.pos.len() {
        return None;
    }
    // a vertex rides the head when most of its weight is on the head's nodes
    let rides = |v: usize| {
        let (b, w) = (src.bones[v], src.weights[v]);
        let on: u32 = (0..4).filter(|&k| b[k] != 255 && under(cs, b[k] as i32, head)).map(|k| w[k] as u32).sum();
        let all: u32 = w.iter().map(|&x| x as u32).sum();
        all > 0 && on * 2 > all
    };
    let near = |b: &esf::Batch| b.lod.is_none_or(|l| l == 0);
    let on_head = |b: &esf::Batch| b.indices.chunks_exact(3).filter(|t| t.iter().all(|&v| rides(v as usize))).count();
    // the face's texture: the one most of the head's triangles are in
    let mut by_tex: Vec<(i32, usize)> = Vec::new();
    for b in src.batches.iter().filter(|b| near(b)) {
        let n = on_head(b);
        match by_tex.iter_mut().find(|(t, _)| *t == b.texture()) {
            Some(e) => e.1 += n,
            None => by_tex.push((b.texture(), n)),
        }
    }
    let face = by_tex.iter().max_by_key(|e| e.1).map(|e| e.0)?;
    let mut m = esf::Model { skinned: false, parts: 1, ..Default::default() };
    let mut map = std::collections::HashMap::new();
    let hat = |b: &esf::Batch| textures.map_or(b.texture() != face, |t| t.contains(&(b.texture() as u32)));
    for b in src.batches.iter().filter(|b| near(b) && hat(b)) {
        let mut out = esf::Batch { lod: None, indices: Vec::new(), ..b.clone() };
        for t in b.indices.chunks_exact(3).filter(|t| t.iter().all(|&v| rides(v as usize))) {
            for &v in t {
                let o = v as usize;
                let i = *map.entry(o).or_insert_with(|| {
                    m.pos.push(src.pos[o]);
                    if let Some(uv) = src.uv.get(o) {
                        m.uv.push(*uv);
                    }
                    if let Some(n) = src.normal.get(o) {
                        m.normal.push(*n);
                    }
                    if let Some(c) = src.color.get(o) {
                        m.color.push(*c);
                    }
                    m.pos.len() as u32 - 1
                });
                out.indices.push(i);
            }
        }
        if !out.indices.is_empty() {
            m.batches.push(out);
        }
    }
    m.triangles = m.batches.iter().map(|b| b.indices.len() as u32 / 3).sum();
    (m.triangles > 0).then_some(m)
}

/// The bounds of a helm (`slotless::helm_on`'s, in its node's frame) in the
/// bind pose's model space: its lowest and highest corners.
fn helm_bounds(cs: &CSprite<Ee>, node: i32, helm: &esf::Model) -> Option<(Vec3, Vec3)> {
    let w = bind_world(cs, node)?;
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for p in &helm.pos {
        let q = w.transform_point3(Vec3::from(*p));
        lo = lo.min(q);
        hi = hi.max(q);
    }
    (lo.x < hi.x).then_some((lo, hi))
}

/// The bounds, in the bind pose's model space, of a body's head: the
/// triangles every vertex of which rides its head node or a node under it
/// (the face, the hair and what else the head carries), but those in the
/// textures `skip` (its hat's); the nearest level of detail.
pub fn head_bounds(m: &esf::Model, cs: &CSprite<Ee>, skip: &[u32]) -> Option<(Vec3, Vec3)> {
    let head = cs.node_ids[7];
    if head < 0 || m.bones.len() != m.pos.len() || m.weights.len() != m.pos.len() {
        return None;
    }
    let rides = |v: usize| {
        let (b, w) = (m.bones[v], m.weights[v]);
        let on: u32 = (0..4).filter(|&k| b[k] != 255 && under(cs, b[k] as i32, head)).map(|k| w[k] as u32).sum();
        let all: u32 = w.iter().map(|&x| x as u32).sum();
        all > 0 && on * 2 > all
    };
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for b in m.batches.iter().filter(|b| b.lod.is_none_or(|l| l == 0) && !skip.contains(&(b.texture() as u32))) {
        for t in b.indices.chunks_exact(3).filter(|t| t.iter().all(|&v| rides(v as usize))) {
            for &v in t {
                lo = lo.min(Vec3::from(m.pos[v as usize]));
                hi = hi.max(Vec3::from(m.pos[v as usize]));
            }
        }
    }
    (lo.x < hi.x).then_some((lo, hi))
}

/// `hat` moved from a head bounded `from` to one bounded `to` ([`head_bounds`]):
/// scaled by the heads' widths (across, x), every axis alike, its middle across
/// on the head's, and as high over (or under) the head's top and as far
/// behind (or before) its front (the face) as it sat, scaled: a head measured
/// by its face alone (the efreeti lord's, whose skull is the turban) has its
/// middle front to back far forward of one with its skull and hair.
pub fn moved_on_top(hat: &mut esf::Model, from: (Vec3, Vec3), to: (Vec3, Vec3)) {
    let s = (to.1.x - to.0.x) / (from.1.x - from.0.x).max(1e-4);
    let fc = Vec3::new((from.0.x + from.1.x) * 0.5, from.1.y, from.1.z);
    let tc = Vec3::new((to.0.x + to.1.x) * 0.5, to.1.y, to.1.z);
    for p in &mut hat.pos {
        *p = (tc + (Vec3::from(*p) - fc) * s).to_array();
    }
}

/// `hat` (a [`cut`], in the source body's bind-pose model space) moved from a
/// head measured `from` to one measured `to` (each the bounds of the same
/// helm as worn, [`helm_bounds`]): its middle on the helm's, scaled by the
/// helms' widths (across, x), every axis alike.
pub fn moved(hat: &mut esf::Model, from: (Vec3, Vec3), to: (Vec3, Vec3)) {
    let (fc, tc) = ((from.0 + from.1) * 0.5, (to.0 + to.1) * 0.5);
    let s = (to.1.x - to.0.x) / (from.1.x - from.0.x).max(1e-4);
    for p in &mut hat.pos {
        *p = (tc + (Vec3::from(*p) - fc) * s).to_array();
    }
}

/// The hat of body `src` on body `id` (`cs` its sprite, `player` a player's
/// character): the node it hangs on (the helm's) and the hat in that node's
/// frame, seated as the module says; `hue` recolours it (`aura::recolor`), none
/// its own colours. None when either body wears no helm (no measure), the
/// source is not a loaded body, or it has no hat.
pub fn hat_on(lib: &Library, id: u32, cs: &CSprite<Ee>, player: bool, src: u32, hue: Option<[u8; 3]>) -> Option<(i32, esf::Model)> {
    let r = lib.lookup(src as i32)?;
    if lib.file(r.file).node(r.node).ty != 0x2700 {
        return None;
    }
    let scs: CSprite<Ee> = vi::character::load_csprite(lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut vi::csprite::Rand(1))?;
    let mut hat = cut(&lib.build_model(r)?, &scs, hat_textures(src))?;
    let t = crate::s2::get().cust();
    let set = crate::aura::CIRCLET_MEASURE_SET;
    let (node, helm) = crate::slotless::helm_on(lib, id, cs, player, set, [255; 4], &t, true)?;
    match crate::slotless::helm_on(lib, src, &scs, true, set, [255; 4], &t, true) {
        // both heads measured by the same helm as each wears it
        Some((snode, shelm)) => moved(&mut hat, helm_bounds(&scs, snode, &shelm)?, helm_bounds(cs, node, &helm)?),
        // a source that wears no helm (the efreeti lord): the source's head by its
        // own triangles but the hat's (its face, ears and beard), the hat's
        // top-of-the-head place kept; the wearer's by its own triangles, but its
        // width the helm's as it wears one (its skull's: a dark elf's ears, a
        // head across and half again, are no measure of what a hat sits on)
        None => {
            let skip = hat_textures(src).unwrap_or(&[]);
            let from = head_bounds(&lib.build_model(r)?, &scs, skip)?;
            let (mut lo, mut hi) = head_bounds(&lib.build_model(lib.lookup(id as i32)?)?, cs, &[])?;
            let (hlo, hhi) = helm_bounds(cs, node, &helm)?;
            (lo.x, hi.x) = (hlo.x, hhi.x);
            moved_on_top(&mut hat, from, (lo, hi));
        }
    }
    // into the frame of the node it hangs on
    let local = bind_world(cs, node)?.inverse();
    for p in &mut hat.pos {
        *p = local.transform_point3(Vec3::from(*p)).to_array();
    }
    for n in &mut hat.normal {
        *n = local.transform_vector3(Vec3::from(*n)).normalize_or_zero().to_array();
    }
    if let Some(c) = hue {
        crate::aura::recolor(&mut hat, c);
    }
    Some((node, hat))
}

/// The recolour ([`render::recolor::DYE`]) a hat takes in the helm's dye `c`
/// (the owner, 2026-11-16: "also turban should be abled to be dyed"): the
/// dye's hue and saturation over the hat's own shading, its lightness raised
/// with the dye's (a hat's art is dark: the efreeti lord's turban is a deep
/// teal, which a plain multiply, an armour piece's MUL, turns black). None for
/// white or near it (no dye: the hat's own colours) or no colour.
pub fn dye_of(c: [u8; 4]) -> Option<u32> {
    let [r, g, b, a] = c.map(|v| v as f32 / 255.0);
    if a == 0.0 || (r > 0.94 && g > 0.94 && b > 0.94) {
        return None;
    }
    let (hi, lo) = (r.max(g).max(b), r.min(g).min(b));
    let l = (hi + lo) / 2.0;
    let d = hi - lo;
    let s = if d <= 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6) };
    let h = if d <= 0.0 {
        0.0
    } else if hi == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if hi == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    let byte = |x: f32| (x.round().clamp(0.0, 255.0)) as u32;
    // the hat's lightness times 1 + 2 L (128 keeps it): a dark dye darkens less
    // than it brightens a light one, black (L 0) keeps the hat's own shading
    Some(render::recolor::DYE << 24 | byte(h * 256.0).min(255) << 16 | byte(s.clamp(0.0, 1.0) * 255.0) << 8 | byte(128.0 * (1.0 + 2.0 * l)).min(255))
}

/// `hat` dyed `c` ([`dye_of`]): every layer's texture recoloured, a texture
/// `taken` names never reused; none when the colour is no dye.
pub fn dyed(hat: &mut esf::Model, c: [u8; 4], taken: impl Fn(i32) -> bool) {
    let Some(r) = dye_of(c) else { return };
    for l in hat.batches.iter_mut().flat_map(|b| b.layers.iter_mut()) {
        if l.texture != 0 {
            l.texture = render::recolor::recolored(l.texture, r, &taken);
        }
    }
}

/// The hat a body's helm aura names (kind hat), if it names one: its source
/// body (0: the kappa's, [`KAPPA_HAT`]) and its hue (`aura::hand_hue`: the
/// colour when it lights, else none, its own colours).
///
/// The hat is the helm's hat ([`net::entity::HELM_HAT_AURA`], a server that
/// sends it apart: the helm's place then holds the helm's other aura, a
/// shell, particles or a trim, drawn on the hat: [`crate::aura::helm_aura`]),
/// else the helm's own aura when that is a hat (an older server's, or a
/// channel with no place for it).
pub fn hat_of(auras: &[net::entity::PieceAura; net::entity::AURAS]) -> Option<(u32, Option<[u8; 3]>)> {
    let hat = |a: net::entity::PieceAura| (a.kind == net::npctint::Aura::HAT).then(|| (if a.source == 0 { KAPPA_HAT } else { a.source }, crate::aura::hand_hue(&a)));
    hat(auras[net::entity::HELM_HAT_AURA]).or_else(|| hat(auras[crate::aura::HELM]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vi::character::{AnimCache, load_csprite};
    use vi::csprite::Rand;

    const HALFLING: u32 = 0xA3DB_3FE2;

    fn disc_lib() -> Option<Library> {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let mut iso = iso::Iso::open(&p).ok()?;
        let mut lib = Library::new();
        for f in crate::dressview::FILES {
            lib.add(*f, esf::Container::open(iso.read(f).ok()?).ok()?);
        }
        Some(lib)
    }

    fn body(lib: &Library, id: u32) -> (esf::Model, CSprite<Ee>) {
        let r = lib.lookup(id as i32).unwrap();
        let cs = load_csprite(lib, r, &mut vi::hsprite::AnimScene::default(), &mut AnimCache::new(), &mut Rand(1)).unwrap();
        (lib.build_model(r).unwrap(), cs)
    }

    fn bounds(m: &esf::Model) -> (Vec3, Vec3) {
        m.pos.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p))))
    }

    /// On the disc: the kappa's hat cut out is the straw hat (44 triangles of
    /// its body's second material) and its steel spike (4 of the bracers'
    /// and belt's): none of the face's texture, wide and flat, over the
    /// head's root. (A body with hair of its own texture would give its hair:
    /// a hat's source is named by a GM, a body known to wear one.)
    #[test]
    fn the_kappas_hat_cut_out() {
        let Some(lib) = disc_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let (m, cs) = body(&lib, KAPPA_HAT);
        let hat = cut(&m, &cs, None).expect("the kappa's hat");
        let (lo, hi) = bounds(&hat);
        eprintln!("hat: {} triangles in {} batches, {lo} .. {hi}", hat.triangles, hat.batches.len());
        for b in &hat.batches {
            let pm = esf::Model { pos: b.indices.iter().map(|&v| hat.pos[v as usize]).collect(), ..Default::default() };
            let (l, h) = bounds(&pm);
            eprintln!("  batch tex {:#010X} mat {}: {} triangles, {l} .. {h}", b.texture() as u32, b.material, b.indices.len() / 3);
        }
        assert!(!hat.skinned && hat.batches.iter().all(|b| b.texture() != m.batches[2].texture()), "no face");
        assert!(hi.x - lo.x > 0.4 && hi.y - lo.y < 0.3, "wide and flat: {lo} .. {hi}");
        let head = bind_world(&cs, cs.node_ids[7]).unwrap().w_axis.truncate();
        assert!(hi.y > head.y + 0.1 && (lo.y + hi.y) / 2.0 > head.y, "on top of the head's root ({head}): {lo} .. {hi}");
        assert_eq!(hat.batches.iter().map(|b| b.indices.len() / 3).collect::<Vec<_>>(), [44, 4], "the straw and the spike");
        assert!(hat.triangles < m.triangles / 4, "the hat alone: {} of {}", hat.triangles, m.triangles);
    }

    /// On the disc: the kappa's hat on a halfling hangs on the node a helm
    /// hangs on, a halfling's head across and more (a brim), its middle over
    /// the helm's; moved and scaled as the helms are; a hue recolours it.
    #[test]
    fn the_hat_on_a_halfling() {
        let Some(lib) = disc_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let (_, cs) = body(&lib, HALFLING);
        let (node, hat) = hat_on(&lib, HALFLING, &cs, true, KAPPA_HAT, None).expect("a hat");
        let t = crate::s2::get().cust();
        let (hn, helm) = crate::slotless::helm_on(&lib, HALFLING, &cs, true, crate::aura::CIRCLET_MEASURE_SET, [255; 4], &t, true).unwrap();
        assert_eq!(node, hn, "the helm's node");
        let w = bind_world(&cs, node).unwrap();
        let world = esf::Model { pos: hat.pos.iter().map(|p| w.transform_point3(Vec3::from(*p)).to_array()).collect(), ..Default::default() };
        let (lo, hi) = bounds(&world);
        let (hlo, hhi) = helm_bounds(&cs, node, &helm).unwrap();
        eprintln!("hat on the halfling: {lo} .. {hi}; helm {hlo} .. {hhi}");
        assert!(hi.x - lo.x > 1.5 * (hhi.x - hlo.x), "a brim past the head");
        assert!(((lo.x + hi.x) / 2.0 - (hlo.x + hhi.x) / 2.0).abs() < 0.02, "centred across");
        assert!(hi.y > hhi.y && lo.y > hlo.y, "on top of the head: {lo} .. {hi} over {hlo} .. {hhi}");
        let (_, red) = hat_on(&lib, HALFLING, &cs, true, KAPPA_HAT, Some([0xFF, 0x20, 0x20])).unwrap();
        assert_eq!(red.pos, hat.pos);
        assert_ne!(format!("{:?}", red.batches[0].layers[0].color), format!("{:?}", hat.batches[0].layers[0].color));
        // the controls: no hat on an item, none of a body with none
        assert!(hat_on(&lib, HALFLING, &cs, true, 0x88EC_092D, None).is_none());
    }

    /// On the disc: the efreeti lord's turban (his body's 0808CC05 triangles on
    /// his head: its dome and the tail down his back) on a dark elf, whose
    /// source wears no helm: on the helm's node, about the turban's own size
    /// on the efreeti's head scaled to the dark elf's skull (his helm's width,
    /// not his ears'), its dome over the head's top. The face rule alone would
    /// take the efreeti's face for his hat.
    #[test]
    fn the_efreeti_lords_turban_on_a_dark_elf() {
        let Some(lib) = disc_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        const DARK_ELF: u32 = 0x7ED8_9DAA;
        let (m, ecs) = body(&lib, EFREETI_LORD);
        let turban = cut(&m, &ecs, hat_textures(EFREETI_LORD)).expect("the turban");
        let (tlo, thi) = bounds(&turban);
        assert!(turban.batches.iter().all(|b| b.texture() as u32 == 0x0808_CC05), "the turban's atlas alone");
        assert!(thi.y > 1.85 && tlo.z < -0.15, "its dome on top and its tail down the back: {tlo} .. {thi}");
        let face = cut(&m, &ecs, None).unwrap();
        assert!(face.batches.iter().any(|b| b.texture() as u32 == 0xFB13_0901), "the face rule takes his face");
        let (_, cs) = body(&lib, DARK_ELF);
        let (node, hat) = hat_on(&lib, DARK_ELF, &cs, true, EFREETI_LORD, None).expect("a turban");
        let t = crate::s2::get().cust();
        let (hn, helm) = crate::slotless::helm_on(&lib, DARK_ELF, &cs, true, crate::aura::CIRCLET_MEASURE_SET, [255; 4], &t, true).unwrap();
        assert_eq!(node, hn);
        let w = bind_world(&cs, node).unwrap();
        let world = esf::Model { pos: hat.pos.iter().map(|p| w.transform_point3(Vec3::from(*p)).to_array()).collect(), ..Default::default() };
        let (lo, hi) = bounds(&world);
        let (hlo, hhi) = helm_bounds(&cs, node, &helm).unwrap();
        eprintln!("turban on the dark elf: {lo} .. {hi}; helm {hlo} .. {hhi}");
        let across = (hi.x - lo.x) / (thi.x - tlo.x);
        assert!((0.8..1.0).contains(&across), "scaled to the skull, not the ears: {across}");
        assert!(hi.y > hhi.y - 0.05 && ((lo.x + hi.x) / 2.0).abs() < 0.01, "on top of the head, centred: {lo} .. {hi}");
        assert!((hi.z - hhi.z).abs() < 0.05 && lo.z < hlo.z && lo.y < hlo.y + 0.05, "its front at the brow, its tail down the back: {lo} .. {hi}");
    }

    /// `moved_on_top`: the tops, the fronts and the middles across, scaled by the widths.
    #[test]
    fn moved_on_top_of_the_head() {
        let mut m = esf::Model { pos: vec![[0.0, 2.0, 1.0], [2.0, 3.0, 0.0]], ..Default::default() };
        moved_on_top(&mut m, (Vec3::new(-1.0, 0.0, -1.0), Vec3::new(1.0, 2.0, 1.0)), (Vec3::new(9.5, 5.0, 9.5), Vec3::new(10.5, 6.0, 10.5)));
        assert_eq!(m.pos, vec![[10.0, 6.0, 10.5], [11.0, 6.5, 10.0]]);
    }

    /// `dye_of`: white and near it, or no colour, no dye; crimson a red hue,
    /// full, its lightness raised; dark blue a blue hue; black keeps the
    /// shading. `dyed` names a recoloured texture for every layer with one.
    #[test]
    fn a_hat_dyed() {
        assert_eq!(dye_of([255, 255, 255, 255]), None);
        assert_eq!(dye_of([250, 250, 245, 255]), None);
        assert_eq!(dye_of([0xD1, 0, 0, 0]), None, "alpha 0: none");
        let red = dye_of([0xD1, 0, 0, 0xFF]).unwrap();
        assert_eq!((red >> 24, (red >> 16) & 0xFF, (red >> 8) & 0xFF), (render::recolor::DYE, 0, 255));
        assert!((red & 0xFF) > 128, "a crimson brightens a dark hat");
        let blue = dye_of([0x10, 0x20, 0x80, 0xFF]).unwrap();
        assert!(((blue >> 16) & 0xFF).abs_diff(160) < 8, "a blue hue: {:#x}", blue);
        assert_eq!(dye_of([0, 0, 0, 0xFF]).map(|r| r & 0xFF), Some(128), "black keeps the shading");
        let layer = |texture: i32| esf::Layer { flags: 1, texture, wrap: 0, blend: 0, color: [255; 4], uv_transform: [0.0; 9], lod_bias: 0.0, uv_rate: [0.0; 2], emissive: [0; 4] };
        let mut m = esf::Model { batches: vec![esf::Batch { material: 0, layers: vec![layer(0x0808_CC05), layer(0)], indices: Vec::new(), lod: None }], ..Default::default() };
        dyed(&mut m, [255; 4], |_| false);
        assert_eq!(m.batches[0].layers[0].texture, 0x0808_CC05, "white: as it was");
        dyed(&mut m, [0xD1, 0, 0, 0xFF], |_| false);
        let t = m.batches[0].layers[0].texture;
        assert_eq!(render::recolor::source_of(t), Some((0x0808_CC05, red)));
        assert_eq!(m.batches[0].layers[1].texture, 0, "a layer with no texture keeps none");
    }

    /// `moved`: the middle to the middle, scaled by the widths.
    #[test]
    fn moved_by_the_helms() {
        let mut m = esf::Model { pos: vec![[0.0, 1.0, 0.0], [2.0, 3.0, 0.0]], ..Default::default() };
        moved(&mut m, (Vec3::new(0.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0)), (Vec3::new(10.0, 10.0, 10.0), Vec3::new(11.0, 11.0, 11.0)));
        assert_eq!(m.pos, vec![[10.0, 10.5, 10.0], [11.0, 11.5, 10.0]]);
    }

    /// `hat_of` reads kind hat alone: its source (0 the kappa's) and hue.
    #[test]
    fn hat_of_reads_the_helms_aura() {
        use net::entity::PieceAura;
        let mut auras = [PieceAura::default(); net::entity::AURAS];
        assert_eq!(hat_of(&auras), None);
        auras[crate::aura::HELM] = PieceAura { kind: net::npctint::Aura::HAT, ..Default::default() };
        assert_eq!(hat_of(&auras), Some((KAPPA_HAT, None)));
        auras[crate::aura::HELM] = PieceAura { kind: net::npctint::Aura::HAT, source: 0x1234, color: [0x20, 0xC0, 0x20, 0xFF], ..Default::default() };
        assert_eq!(hat_of(&auras), Some((0x1234, Some([0x20, 0xC0, 0x20]))));
        auras[crate::aura::HELM].kind = net::npctint::Aura::CIRCLET;
        assert_eq!(hat_of(&auras), None);
        // the helm's hat apart (a 365-byte channel): the hat there, a shell in the helm's place
        auras[crate::aura::HELM] = PieceAura { kind: net::npctint::Aura::SHELL, color: [0xFF, 0x20, 0x20, 0xFF], ..Default::default() };
        auras[net::entity::HELM_HAT_AURA] = PieceAura { kind: net::npctint::Aura::HAT, source: EFREETI_LORD, ..Default::default() };
        assert_eq!(hat_of(&auras), Some((EFREETI_LORD, None)), "the hat's own colours: the shell's colour is the shell's");
    }
}
