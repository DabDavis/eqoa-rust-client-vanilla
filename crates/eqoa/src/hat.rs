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

/// The dressed gnome man (CHAR.ESF #139, unused as a body) whose red pointed
/// cap the Santa Hat is (go-eqoa-server's `GnomeCap`; the owner, 2026-11-17:
/// "for santa clause lets cut the gnome cap to be usuable for all and make it
/// red and white with a white ball on the end").
pub const GNOME_CAP: u32 = 0xA205_90E3;

/// The bodies whose hats are their materials, not their textures: the gnome's
/// cap is its material 10, the head's own texture (0xD499494B: the hair's and
/// face's too) in a red layer colour.
pub const HAT_MATERIALS: &[(u32, &[i32])] = &[(GNOME_CAP, &[10])];

/// The materials body `src`'s hat is, when [`HAT_MATERIALS`] knows it.
pub fn hat_materials(src: u32) -> Option<&'static [i32]> {
    HAT_MATERIALS.iter().find(|(b, _)| *b == src).map(|(_, m)| *m)
}

/// A hat aura's nodes byte that dresses the hat as Santa's (go-eqoa-server's
/// `HatSanta`): a white band round its base and a white ball at its tip, the
/// rest red ([`SANTA_RED`]) or the helm's dye ([`dress`]). 0 is the hat as cut.
pub const STYLE_SANTA: u8 = 1;

/// The Santa Hat's red, undyed (the record's colour, go-eqoa-server's Santa
/// Hat: 0xC01818).
pub const SANTA_RED: [u8; 4] = [0xC0, 0x18, 0x18, 0xFF];

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
    cut_where(src, cs, textures, None)
}

/// [`cut`], the hat a body's materials (`materials`: [`hat_materials`]) when
/// it names them, before its textures.
pub fn cut_where(src: &esf::Model, cs: &CSprite<Ee>, textures: Option<&[u32]>, materials: Option<&[i32]>) -> Option<esf::Model> {
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
    let hat = |b: &esf::Batch| match (materials, textures) {
        (Some(m), _) => m.contains(&b.material),
        (None, Some(t)) => t.contains(&(b.texture() as u32)),
        (None, None) => b.texture() != face,
    };
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
/// character): the node it hangs on (the helm's, or the head's on a body that
/// wears no helm) and the hat in that node's frame; `hue` recolours it
/// (`aura::recolor`), none its own colours. `id` is the body DRAWN (an
/// illusion's, a pill's, a were form's, a custom race's: `play::attach` hangs
/// it on the drawn body, made again whenever that changes): the hat is
/// measured on [`REFERENCE`] ([`measured_hat`]) and moved to this body's helm
/// as it wears one, so it sits as a helm would. None when the source is no
/// loaded body or has no hat, or this body has neither a helm nor a head.
pub fn hat_on(lib: &Library, id: u32, cs: &CSprite<Ee>, player: bool, src: u32, hue: Option<[u8; 3]>) -> Option<(i32, esf::Model)> {
    hat_on_styled(lib, id, cs, player, src, hue, 0)
}

/// [`hat_on`] in a style: [`STYLE_SANTA`] Santa's ([`santa`], its band and
/// ball made in the bind pose, before it goes into the node's frame).
#[allow(clippy::too_many_arguments)]
pub fn hat_on_styled(lib: &Library, id: u32, cs: &CSprite<Ee>, player: bool, src: u32, hue: Option<[u8; 3]>, style: u8) -> Option<(i32, esf::Model)> {
    let (mut hat, from) = measured_hat(lib, src)?;
    let t = crate::s2::get().cust();
    let set = crate::aura::CIRCLET_MEASURE_SET;
    let node = match crate::slotless::helm_on(lib, id, cs, player, set, [255; 4], &t, true) {
        // the body drawn (an illusion's, a were's, a pill's, a custom race's)
        // wears a helm: the hat moved from the helm it was measured by to that
        // helm as this body wears it (its seat, its stretch: a giant's 4.2), as
        // the helm itself would be
        Some((node, helm)) => {
            moved(&mut hat, from, helm_bounds(cs, node, &helm)?);
            node
        }
        // a body that wears no helm (a were's beast): a helm stood in for, on
        // its head node where the reference's helm sits from the reference's,
        // scaled by the heads' widths ([`unhelmed_seat`])
        None => {
            let node = cs.node_ids[7];
            let to = unhelmed_seat(lib, id, cs, from)?;
            moved(&mut hat, from, to);
            node
        }
    };
    if style == STYLE_SANTA {
        santa(&mut hat);
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

/// The body a hat whose source wears no helm (the efreeti lord's turban) is
/// first seated on, by the heads ([`moved_on_top`]), to measure it by a helm:
/// the dark elf male (Dust's, the seat the owner saw first). Every other body
/// then takes it from that helm to its own, as a helm moves, so a body's
/// jaw, tusks, ears, hair or cap never moves it (on an ogre the turban stood
/// forward of the brow, by the jaw; on a troll and a gnome above the head, by
/// the ears and the cap: the owner, 2026-11-17, "dust used an ogre pill and
/// the turban is not sitting on the ogres head correctly").
pub const REFERENCE: u32 = 0x7ED8_9DAA;

/// Body `src`'s hat ([`cut`]) seated on [`REFERENCE`], in its bind-pose model
/// space, and the bounds of the reference's helm (armour set 4's,
/// `aura::CIRCLET_MEASURE_SET`, as worn) it is measured by: from the source's
/// own helm to the reference's when the source wears one (the kappa), else by
/// the heads (the source's head by its own triangles but the hat's; the
/// reference's by its own, its width its helm's). None when `src` is no loaded body or has no
/// hat, or the reference is not loaded.
pub fn measured_hat(lib: &Library, src: u32) -> Option<(esf::Model, (Vec3, Vec3))> {
    let r = lib.lookup(src as i32)?;
    if lib.file(r.file).node(r.node).ty != 0x2700 {
        return None;
    }
    let load = |r| vi::character::load_csprite::<Ee>(lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut vi::csprite::Rand(1));
    let scs = load(r)?;
    let mut hat = cut_where(&lib.build_model(r)?, &scs, hat_textures(src), hat_materials(src))?;
    let t = crate::s2::get().cust();
    let set = crate::aura::CIRCLET_MEASURE_SET;
    let rr = lib.lookup(REFERENCE as i32)?;
    let rcs = load(rr)?;
    let (rnode, rhelm) = crate::slotless::helm_on(lib, REFERENCE, &rcs, true, set, [255; 4], &t, true)?;
    let helm = helm_bounds(&rcs, rnode, &rhelm)?;
    // a hat that is a material of its body (the gnome's cap,
    // [`HAT_MATERIALS`]): seated by its own rim ([`rim_seat`]): the gnome's
    // helm sits high on its small face (moved by it the cap came down over the
    // wearer's face) and its scalp is under the cap (moved by the heads it
    // stood off the head)
    if hat_materials(src).is_some() {
        rim_seat(&mut hat, helm);
        return Some((hat, helm));
    }
    // a source that wears a helm: from its helm to the reference's, so every
    // hat is measured on the reference (a body with no helm, `unhelmed_seat`,
    // is measured from the reference's head)
    if let Some((snode, shelm)) = crate::slotless::helm_on(lib, src, &scs, true, set, [255; 4], &t, true) {
        moved(&mut hat, helm_bounds(&scs, snode, &shelm)?, helm);
        return Some((hat, helm));
    }
    let skip = hat_textures(src).unwrap_or(&[]);
    let from = head_bounds(&lib.build_model(r)?, &scs, skip)?;
    let (mut lo, mut hi) = head_bounds(&lib.build_model(rr)?, &rcs, &[])?;
    (lo.x, hi.x) = (helm.0.x, helm.1.x);
    moved_on_top(&mut hat, from, (lo, hi));
    Some((hat, helm))
}

/// How high on a helm (its share from the chin of set 4's great helm) a cap's
/// rim sits ([`rim_seat`]): the brow, where a circlet sits too.
pub const RIM_HEIGHT: f32 = 0.6;

/// How wide a cap's rim is against the helm it is seated by ([`rim_seat`]):
/// round the head, the head inside it as in a helm.
pub const RIM_WIDTH: f32 = 1.05;

/// `hat` (a cap, in its body's bind-pose model space) seated on a head by its
/// own rim, the head measured by `helm` (set 4's as worn, its bounds): the
/// rim (its lowest fifth of height) scaled to [`RIM_WIDTH`] of the helm's
/// width, every axis alike, its middle over the helm's across and front to
/// back, its bottom at [`RIM_HEIGHT`] of the helm's height.
pub fn rim_seat(hat: &mut esf::Model, helm: (Vec3, Vec3)) {
    let (lo, hi) = hat.pos.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
    if lo >= hi {
        return;
    }
    let line = lo + 0.2 * (hi - lo);
    let rim: Vec<Vec3> = hat.pos.iter().map(|p| Vec3::from(*p)).filter(|p| p.y <= line).collect();
    let (rlo, rhi) = rim.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), p| (a.min(*p), b.max(*p)));
    let s = RIM_WIDTH * (helm.1.x - helm.0.x) / (rhi.x - rlo.x).max(1e-4);
    let from = Vec3::new((rlo.x + rhi.x) * 0.5, lo, (rlo.z + rhi.z) * 0.5);
    let to = Vec3::new((helm.0.x + helm.1.x) * 0.5, helm.0.y + RIM_HEIGHT * (helm.1.y - helm.0.y), (helm.0.z + helm.1.z) * 0.5);
    for p in &mut hat.pos {
        *p = (to + (Vec3::from(*p) - from) * s).to_array();
    }
}

/// The helm a body that wears none would wear (a were's beast: the
/// werewolf), stood in for in its bind-pose model space: `helm` (the bounds
/// [`measured_hat`] measured by, on [`REFERENCE`]) moved from the reference's
/// head node to this body's (`node_ids[7]`), its place past the node and its
/// size scaled by the heads' widths ([`head_bounds`]). None when either body
/// has no head node or no head.
pub fn unhelmed_seat(lib: &Library, id: u32, cs: &CSprite<Ee>, helm: (Vec3, Vec3)) -> Option<(Vec3, Vec3)> {
    let rr = lib.lookup(REFERENCE as i32)?;
    let rcs = vi::character::load_csprite::<Ee>(lib, rr, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut vi::csprite::Rand(1))?;
    let rhead = head_bounds(&lib.build_model(rr)?, &rcs, &[])?;
    let head = head_bounds(&lib.build_model(lib.lookup(id as i32)?)?, cs, &[])?;
    let s = (head.1.x - head.0.x) / (rhead.1.x - rhead.0.x).max(1e-4);
    let (rn, n) = (bind_world(&rcs, rcs.node_ids[7])?.w_axis.truncate(), bind_world(cs, cs.node_ids[7])?.w_axis.truncate());
    Some((n + (helm.0 - rn) * s, n + (helm.1 - rn) * s))
}

/// The recolour ([`render::recolor::DYE`]) a hat's texture takes in the
/// helm's dye `c` (the owner, 2026-11-16: "also turban should be abled to be
/// dyed"): the dye's hue and saturation over the texture's own shading, and
/// its lightness moved so the texture's mean (`mean`, 0-1: [`mean_lightness`])
/// lands on the dye's (lightening by the square root of that): a dark dye
/// darkens it, a black one (no hue) to near black, a bright one lightens a dark texture (the efreeti lord's turban, a
/// deep teal, which a plain multiply, an armour piece's MUL, turns black).
/// Before, the lightness was raised with the dye's whatever the texture, so a
/// black dye (Hoofbite's 101010) left the pale straw of the kappa's hat a pale
/// grey: it seemed undyed (the owner, 2026-11-17: "the kappa hat that hoofbie
/// has isnt being dyed yet"). The scale is at most twice (the DYE byte's 255).
/// None for white or near it (no dye: the hat's own colours) or no colour.
pub fn dye_code(c: [u8; 4], mean: f32) -> Option<u32> {
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
    // the texture's lightness times the dye's over the texture's mean (128
    // keeps it): darkening in full, lightening by its square root (the full
    // ratio washed a red over pale straw to pink, its light streaks white)
    let ratio = l / mean.max(0.02);
    let scale = 128.0 * if ratio < 1.0 { ratio } else { ratio.sqrt() };
    Some(render::recolor::DYE << 24 | byte(h * 256.0).min(255) << 16 | byte(s.clamp(0.0, 1.0) * 255.0) << 8 | byte(scale).clamp(1, 255))
}

/// The mean HSL lightness (0-1) of a texture's opaque texels (alpha over
/// half; every texel when none is), its base level decoded; none for a
/// texture the library cannot decode.
pub fn mean_lightness(lib: &Library, texture: i32) -> Option<f32> {
    let img = lib.surface(texture).and_then(|(c, s)| s.decode(&c.data))?;
    let px: Vec<&[u8]> = img.rgba.chunks_exact(4).collect();
    let opaque: Vec<&&[u8]> = px.iter().filter(|p| p[3] > 64).collect();
    let pick: Vec<&[u8]> = if opaque.is_empty() { px.clone() } else { opaque.into_iter().copied().collect() };
    if pick.is_empty() {
        return None;
    }
    let sum: f32 = pick.iter().map(|p| (p[0].max(p[1]).max(p[2]) as f32 + p[0].min(p[1]).min(p[2]) as f32) / 510.0).sum();
    Some(sum / pick.len() as f32)
}

/// `hat` dyed `c` ([`dye_code`]): every layer's texture recoloured by its own
/// mean lightness (a character's own hat, charaura's, and a worn hat item's
/// alike: both are the helm's hat, the colour the helm's dye); a texture the
/// library names never reused; nothing when the colour is no dye.
pub fn dyed(hat: &mut esf::Model, c: [u8; 4], lib: &Library) {
    if dye_code(c, 0.5).is_none() {
        return;
    }
    let taken = |id: i32| lib.lookup(id).is_some();
    for l in hat.batches.iter_mut().flat_map(|b| b.layers.iter_mut()) {
        if l.texture != 0
            && let Some(r) = dye_code(c, mean_lightness(lib, l.texture).unwrap_or(0.5))
        {
            l.texture = render::recolor::recolored(l.texture, r, taken);
        }
    }
}

/// `hat` (in a bind-pose model space, up +y) made Santa's: the triangles of
/// its lowest [`SANTA_BAND`] of height moved into a batch of their own with no
/// texture (white; a triangle by its middle), and a white ball ([`SANTA_BALL`] of its height across) at
/// its tip, the vertex farthest from the middle of its base (the gnome's cap
/// bends over at the top, so not the highest). [`dress`] colours the rest.
pub fn santa(hat: &mut esf::Model) {
    let (lo, hi) = hat.pos.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1])));
    if lo >= hi || hat.batches.is_empty() {
        return;
    }
    let line = lo + SANTA_BAND * (hi - lo);
    let mut white = esf::Batch { indices: Vec::new(), ..hat.batches[0].clone() };
    for l in &mut white.layers {
        l.texture = 0;
        l.color = [255; 4];
    }
    let mut base = Vec3::ZERO;
    let mut n = 0.0;
    for b in &mut hat.batches {
        let mut keep = Vec::new();
        for t in b.indices.chunks_exact(3) {
            if t.iter().map(|&v| hat.pos[v as usize][1]).sum::<f32>() / 3.0 <= line {
                white.indices.extend_from_slice(t);
                for &v in t {
                    base += Vec3::from(hat.pos[v as usize]);
                    n += 1.0;
                }
            } else {
                keep.extend_from_slice(t);
            }
        }
        b.indices = keep;
    }
    let base = if n > 0.0 { base / n } else { Vec3::new(0.0, lo, 0.0) };
    let tip = hat.pos.iter().map(|p| Vec3::from(*p)).max_by(|a, b| (*a - base).length().total_cmp(&(*b - base).length())).unwrap_or(base);
    // the ball's vertex colour: the hat's own mean (the share of the light
    // its vertices carry), so the ball is lit as the cap is; white (1.0) lit
    // it full by night, a lamp on the tip
    let shade = if hat.color.is_empty() {
        [1.0; 4]
    } else {
        let n = hat.color.len() as f32;
        hat.color.iter().fold([0.0f32; 4], |a, c| std::array::from_fn(|k| a[k] + c[k])).map(|x| x / n)
    };
    // the ball: a UV sphere of 8 rings by 10 segments
    let r = SANTA_BALL * (hi - lo) * 0.5;
    let (rings, segs) = (8u32, 10u32);
    let first = hat.pos.len() as u32;
    for i in 0..=rings {
        let th = std::f32::consts::PI * i as f32 / rings as f32;
        for j in 0..=segs {
            let ph = std::f32::consts::TAU * j as f32 / segs as f32;
            let d = Vec3::new(th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin());
            hat.pos.push((tip + d * r).to_array());
            if !hat.uv.is_empty() {
                hat.uv.push([0.0, 0.0]);
            }
            if !hat.normal.is_empty() {
                hat.normal.push(d.to_array());
            }
            if !hat.color.is_empty() {
                hat.color.push(shade);
            }
        }
    }
    for i in 0..rings {
        for j in 0..segs {
            let a = first + i * (segs + 1) + j;
            let b = a + segs + 1;
            // wound to face out (wound the other way the ball showed its far
            // side's inside, flat and unlit: the owner's "the ball is a white
            // disc" before two_sided turned that inside grey)
            white.indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    hat.batches.retain(|b| !b.indices.is_empty());
    hat.batches.push(white);
    hat.triangles = hat.batches.iter().map(|b| b.indices.len() as u32 / 3).sum();
}

/// How much of a Santa hat's height, from its base, is its white band.
pub const SANTA_BAND: f32 = 0.2;

/// A Santa hat's ball across, as a share of its height.
pub const SANTA_BALL: f32 = 0.2;

/// A hat dressed in the helm's colour `c` (`look.helm_color`): Santa's
/// ([`STYLE_SANTA`]) its textured layers in the dye's colour, else in
/// [`SANTA_RED`] (no dye: white), their layer colours made white (the gnome's
/// cap is red by its layer colour over the head's own texture), its band and
/// ball left white; any other hat [`dyed`].
pub fn dress(hat: &mut esf::Model, style: u8, c: [u8; 4], lib: &Library) {
    if style != STYLE_SANTA {
        dyed(hat, c, lib);
        return;
    }
    let colour = if dye_code(c, 0.5).is_some() { c } else { SANTA_RED };
    let taken = |id: i32| lib.lookup(id).is_some();
    for l in hat.batches.iter_mut().flat_map(|b| b.layers.iter_mut()) {
        if l.texture != 0
            && let Some(r) = dye_code(colour, mean_lightness(lib, l.texture).unwrap_or(0.5))
        {
            l.texture = render::recolor::recolored(l.texture, r, taken);
            l.color = [255; 4];
        }
    }
}

/// The hat an NPC's tint names by its name (`npctint`, go-eqoa-server's
/// npctint aura of kind hat: Santa Claus's Santa Hat): its source (0 the
/// kappa's) and its style; none for a tint with no hat.
pub fn npc_hat(t: &net::npctint::Tint) -> Option<(u32, u8)> {
    (t.aura.kind == net::npctint::Aura::HAT).then(|| (if t.aura.source == 0 { KAPPA_HAT } else { t.aura.source }, t.aura.nodes))
}

/// How dark a hat's inside is against its outside ([`two_sided`]): the same
/// art in shade, as the inside of a cap is.
pub const INSIDE_SHADE: f32 = 0.6;

/// `hat` drawn from both sides (the owner, 2026-11-17: "when you look under
/// santas hat up the backface culling makes it look bad"): a cut hat is the
/// outside of a cap alone, its inside culled, so from below it was not
/// there. Every triangle again, wound the other way on its own copies of
/// the vertices with the normals turned in (lit as an inside is), each batch's
/// layers' colours times [`INSIDE_SHADE`] (the Santa Hat's white band a light
/// grey inside, its red a deeper red). The white ball is closed and comes out
/// the same from any side. Called once a hat is dressed (`dress`), so the shade
/// is over its dye.
pub fn two_sided(hat: &mut esf::Model) {
    let n = hat.pos.len() as u32;
    let k = hat.pos.len();
    hat.pos.extend_from_within(..k);
    if hat.uv.len() == k {
        hat.uv.extend_from_within(..k);
    }
    if hat.normal.len() == k {
        let inward: Vec<[f32; 3]> = hat.normal[..k].iter().map(|v| [-v[0], -v[1], -v[2]]).collect();
        hat.normal.extend(inward);
    }
    if hat.color.len() == k {
        hat.color.extend_from_within(..k);
    }
    if hat.bones.len() == k {
        hat.bones.extend_from_within(..k);
    }
    if hat.weights.len() == k {
        hat.weights.extend_from_within(..k);
    }
    let shade = |c: u8| (c as f32 * INSIDE_SHADE).round() as u8;
    let insides: Vec<esf::Batch> = hat
        .batches
        .iter()
        .map(|b| {
            let mut inside = b.clone();
            inside.indices = b.indices.chunks_exact(3).flat_map(|t| [t[0] + n, t[2] + n, t[1] + n]).collect();
            for l in &mut inside.layers {
                l.color = [shade(l.color[0]), shade(l.color[1]), shade(l.color[2]), l.color[3]];
            }
            inside
        })
        .collect();
    hat.batches.extend(insides);
    hat.triangles = hat.batches.iter().map(|b| b.indices.len() as u32 / 3).sum();
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
pub fn hat_of(auras: &[net::entity::PieceAura; net::entity::AURAS]) -> Option<(u32, Option<[u8; 3]>, u8)> {
    let hat = |a: net::entity::PieceAura| (a.kind == net::npctint::Aura::HAT).then(|| (if a.source == 0 { KAPPA_HAT } else { a.source }, crate::aura::hand_hue(&a), a.nodes));
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

    /// `dye_code`: white and near it, or no colour, no dye; crimson a red
    /// hue, full; dark blue a blue hue; the lightness moved to the dye's from
    /// the texture's mean: a black dye (Hoofbite's 101010) on the pale straw
    /// (mean 0.3) darkens it to a fifth, a crimson on the dark turban (0.15)
    /// lightens it (at most twice). On the disc: the kappa's straw dyed black
    /// is near black, red is red, and white leaves the texture as it was.
    #[test]
    fn a_hat_dyed() {
        assert_eq!(dye_code([255, 255, 255, 255], 0.5), None);
        assert_eq!(dye_code([250, 250, 245, 255], 0.5), None);
        assert_eq!(dye_code([0xD1, 0, 0, 0], 0.5), None, "alpha 0: none");
        let red = dye_code([0xD1, 0, 0, 0xFF], 0.15).unwrap();
        assert_eq!((red >> 24, (red >> 16) & 0xFF, (red >> 8) & 0xFF), (render::recolor::DYE, 0, 255));
        assert!((200..=215).contains(&(red & 0xFF)), "a crimson lightens a dark hat by the root: {}", red & 0xFF);
        let blue = dye_code([0x10, 0x20, 0x80, 0xFF], 0.6).unwrap();
        assert!(((blue >> 16) & 0xFF).abs_diff(160) < 8, "a blue hue: {:#x}", blue);
        let black = dye_code([0x10, 0x10, 0x10, 0xFF], 0.6).unwrap();
        assert!((black >> 8) & 0xFF == 0 && (black & 0xFF) < 16, "black darkens the straw, no hue: {black:#x}");
        let Some(lib) = disc_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let (m, cs) = body(&lib, KAPPA_HAT);
        let straw = cut(&m, &cs, hat_textures(KAPPA_HAT)).unwrap();
        let tex = straw.batches[0].texture();
        let mean = mean_lightness(&lib, tex).unwrap();
        assert!(mean > 0.2, "the straw is lighter than the dye: {mean}");
        let lightness_of = |m: &esf::Model| {
            let t = m.batches[0].texture();
            let (src, r) = render::recolor::source_of(t).unwrap_or((t, 0));
            let img = lib.surface(src).and_then(|(c, s)| s.decode(&c.data)).unwrap();
            let mut px = img.rgba.clone();
            render::recolor::recolor_rgba(&mut px, r);
            let n = px.len() / 4;
            (px.chunks_exact(4).map(|p| (p[0].max(p[1]).max(p[2]) as f32 + p[0].min(p[1]).min(p[2]) as f32) / 510.0).sum::<f32>() / n as f32,
             px.chunks_exact(4).map(|p| p[0] as f32 - p[2] as f32).sum::<f32>() / n as f32)
        };
        let mut white = straw.clone();
        dyed(&mut white, [255; 4], &lib);
        assert_eq!(white.batches[0].texture(), tex, "white: as it was");
        let mut dark = straw.clone();
        dyed(&mut dark, [0x10, 0x10, 0x10, 0xFF], &lib);
        let (l, _) = lightness_of(&dark);
        assert!(l < 0.12, "Hoofbite's black darkens the straw: {l}");
        let mut redden = straw.clone();
        dyed(&mut redden, [0xD1, 0, 0, 0xFF], &lib);
        let (l, rb) = lightness_of(&redden);
        assert!(rb > 60.0 && l > 0.2, "red is red: lightness {l}, red over blue {rb}");
    }

    /// The hat fits the body drawn (the owner, 2026-11-17: "dust used an ogre
    /// pill and the turban is not sitting on the ogres head correctly"): on
    /// the ogres, a troll, a barbarian, a gnome, a halfling and a hill giant
    /// the turban and the kappa's hat sit as a helm would, centred across over
    /// the helm, their lowest point below the helm's top (on the head, not over
    /// it: before, the turban floated over the troll's ears and the gnome's
    /// cap) and their middle front to back over the helm's (before, on the
    /// ogre, the turban stood forward by the jaw); on the werewolf, which wears
    /// no helm, on its head node, centred, above its brow and not below its
    /// chest (the kappa's hat once hung at its chest). The dark elf, the
    /// reference, wears the turban as it did.
    #[test]
    fn the_hat_fits_the_body_drawn() {
        let Some(lib) = disc_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let t = crate::s2::get().cust();
        let world = |cs: &CSprite<Ee>, node: i32, m: &esf::Model| {
            let w = bind_world(cs, node).unwrap();
            bounds(&esf::Model { pos: m.pos.iter().map(|p| w.transform_point3(Vec3::from(*p)).to_array()).collect(), ..Default::default() })
        };
        for id in [0x8480_7490u32, 0xACC0_0B4F, 0x4C6F_A532, 0x61CA_3EDF, 0xA99C_6B15, 0xA3DB_3FE2, 0xFFB1_EA2C, REFERENCE] {
            let (_, cs) = body(&lib, id);
            let (hn, helm) = crate::slotless::helm_on(&lib, id, &cs, true, crate::aura::CIRCLET_MEASURE_SET, [255; 4], &t, true).unwrap();
            let (hlo, hhi) = helm_bounds(&cs, hn, &helm).unwrap();
            for src in [EFREETI_LORD, KAPPA_HAT] {
                let (node, hat) = hat_on(&lib, id, &cs, true, src, None).unwrap_or_else(|| panic!("{id:#010X} {src:#010X}"));
                assert_eq!(node, hn, "{id:#010X}: the helm's node");
                let (lo, hi) = world(&cs, node, &hat);
                let mid = (lo + hi) * 0.5;
                let across = hhi.x - hlo.x;
                assert!((mid.x - (hlo.x + hhi.x) * 0.5).abs() < 0.02 * across.max(0.2), "{id:#010X} {src:#010X}: centred: {lo} .. {hi}");
                assert!(lo.y < hhi.y, "{id:#010X} {src:#010X}: on the head, not over it: {lo} .. {hi}, helm {hlo} .. {hhi}");
                assert!((mid.z - (hlo.z + hhi.z) * 0.5).abs() < 0.35 * across, "{id:#010X} {src:#010X}: over the helm front to back, not forward by the jaw: {lo} .. {hi}, helm {hlo} .. {hhi}");
            }
        }
        // the reference wears the turban as it was first seated (by the heads)
        let (m, ecs) = body(&lib, EFREETI_LORD);
        let mut seated = cut(&m, &ecs, hat_textures(EFREETI_LORD)).unwrap();
        let (_, rcs) = body(&lib, REFERENCE);
        let (rn, rh) = crate::slotless::helm_on(&lib, REFERENCE, &rcs, true, crate::aura::CIRCLET_MEASURE_SET, [255; 4], &t, true).unwrap();
        let rhelm = helm_bounds(&rcs, rn, &rh).unwrap();
        let (mut lo, mut hi) = head_bounds(&lib.build_model(lib.lookup(REFERENCE as i32).unwrap()).unwrap(), &rcs, &[]).unwrap();
        (lo.x, hi.x) = (rhelm.0.x, rhelm.1.x);
        moved_on_top(&mut seated, head_bounds(&m, &ecs, hat_textures(EFREETI_LORD).unwrap()).unwrap(), (lo, hi));
        let (_, on_ref) = hat_on(&lib, REFERENCE, &rcs, true, EFREETI_LORD, None).unwrap();
        let (a, b) = (bounds(&seated), world(&rcs, rn, &on_ref));
        assert!((a.0 - b.0).length() < 1e-3 && (a.1 - b.1).length() < 1e-3, "the reference: {a:?} vs {b:?}");
        // the werewolf: no helm, on its head node
        const WEREWOLF: u32 = 0xDBED_A323;
        let (wm, wcs) = body(&lib, WEREWOLF);
        assert!(crate::slotless::helm_on(&lib, WEREWOLF, &wcs, true, crate::aura::CIRCLET_MEASURE_SET, [255; 4], &t, true).is_none());
        let head = head_bounds(&wm, &wcs, &[]).unwrap();
        let hnode = bind_world(&wcs, wcs.node_ids[7]).unwrap().w_axis.truncate();
        for src in [EFREETI_LORD, KAPPA_HAT] {
            let (node, hat) = hat_on(&lib, WEREWOLF, &wcs, true, src, None).expect("a hat on the werewolf");
            assert_eq!(node, wcs.node_ids[7]);
            let (lo, hi) = world(&wcs, node, &hat);
            eprintln!("werewolf {src:#010X}: {lo} .. {hi}; head {head:?}; node {hnode}");
            assert!(((lo.x + hi.x) * 0.5).abs() < 0.02, "centred");
            assert!(hi.y > head.1.y - 0.05 && lo.y > hnode.y - 0.25, "{src:#010X}: on the head, not at the chest: {lo} .. {hi}");
        }
    }

    /// Santa's ball takes the cap's own vertex colour (the share of the light
    /// its vertices carry), so by night it is no brighter than the cap: with
    /// white (1.0) it was drawn full bright, a lamp on the tip.
    #[test]
    fn santas_ball_is_lit_as_the_cap() {
        // a cone of 8 sides, 1 high, its vertices carrying half the light (as
        // a body's do), in one textured batch: the ball's vertices take the
        // cap's own colour, so by night it is no brighter than the cap; the
        // control: a hat with no vertex colours gains none
        let mut hat = esf::Model::default();
        let layer = esf::Layer { flags: 0, texture: 77, wrap: 0, blend: 0, color: [128; 4], uv_transform: [0.0; 9], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
        let mut idx = Vec::new();
        for k in 0..8u32 {
            let a = k as f32 * std::f32::consts::TAU / 8.0;
            hat.pos.push([a.cos() * 0.3, 0.0, a.sin() * 0.3]);
            hat.color.push([0.5, 0.4, 0.3, 0.5]);
        }
        hat.pos.push([0.0, 1.0, 0.0]);
        hat.color.push([0.5, 0.4, 0.3, 0.5]);
        for k in 0..8u32 {
            idx.extend_from_slice(&[k, (k + 1) % 8, 8]);
        }
        hat.batches.push(esf::Batch { material: 10, layers: vec![layer], indices: idx, lod: None });
        let mut bare = hat.clone();
        let n = hat.pos.len();
        santa(&mut hat);
        assert!(hat.pos.len() > n + 50, "a ball added");
        assert_eq!(hat.color.len(), hat.pos.len());
        assert!(hat.color[n..].iter().all(|c| c.iter().zip([0.5, 0.4, 0.3, 0.5]).all(|(a, b)| (a - b).abs() < 1e-6)), "the ball lit as the cap: {:?}", hat.color[n]);
        bare.color.clear();
        santa(&mut bare);
        assert!(bare.color.is_empty(), "no colours made up");
    }

    /// On the disc: the gnome's cap (the dressed gnome man's material 10, the
    /// head's own texture in a red layer colour) cut out alone, its point well
    /// above the head; made Santa's on a dark elf and an ogre: a white batch
    /// (no texture, white) of its band and a ball at its tip, the rest red;
    /// dressed in a dye, the red part takes the dye and the white stays white.
    /// The NPC's hat by name (npctint kind hat) reads its source and style.
    #[test]
    fn the_santa_hat() {
        let Some(lib) = disc_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let (m, cs) = body(&lib, GNOME_CAP);
        let cap = cut_where(&m, &cs, hat_textures(GNOME_CAP), hat_materials(GNOME_CAP)).expect("the gnome's cap");
        assert!(cap.batches.iter().all(|b| b.material == 10), "material 10 alone");
        assert_eq!(cap.batches[0].layers[0].color[..3], [128, 0, 1], "red by its layer colour");
        let (lo, hi) = bounds(&cap);
        assert!(hi.y - lo.y > 0.4, "a tall cap: {lo} .. {hi}");
        for body_id in [REFERENCE, 0x8480_7490] {
            let (_, wcs) = body(&lib, body_id);
            let (_, plain) = hat_on(&lib, body_id, &wcs, true, GNOME_CAP, None).unwrap();
            let (_, mut hat) = hat_on_styled(&lib, body_id, &wcs, true, GNOME_CAP, None, STYLE_SANTA).unwrap();
            let white = hat.batches.last().unwrap();
            assert!(white.layers.iter().all(|l| l.texture == 0 && l.color == [255; 4]), "the band and ball: white");
            assert!(hat.triangles > plain.triangles + 100, "a ball added: {} vs {}", hat.triangles, plain.triangles);
            assert_eq!(hat.batches.iter().map(|b| b.indices.len()).sum::<usize>() / 3, hat.triangles as usize);
            dress(&mut hat, STYLE_SANTA, [255; 4], &lib);
            let red = hat.batches[0].layers[0].clone();
            assert_eq!(red.color, [255; 4], "its red by its texture now");
            let (src, r) = render::recolor::source_of(red.texture).unwrap();
            assert_eq!((src as u32, (r >> 16) & 0xFF, (r >> 8) & 0xFF), (0xD499_494B, 0, 198), "undyed: Santa's red");
            let (_, mut blue) = hat_on_styled(&lib, body_id, &wcs, true, GNOME_CAP, None, STYLE_SANTA).unwrap();
            dress(&mut blue, STYLE_SANTA, [0x20, 0x30, 0xC0, 0xFF], &lib);
            let (_, rb) = render::recolor::source_of(blue.batches[0].layers[0].texture).unwrap();
            assert!(((rb >> 16) & 0xFF).abs_diff(163) < 8, "dyed blue: {rb:#x}");
            assert!(blue.batches.last().unwrap().layers.iter().all(|l| l.texture == 0), "the white stays white");
        }
        let t = net::npctint::Tint { color: 0, glow: 0, aura: net::npctint::Aura { kind: net::npctint::Aura::HAT, source: GNOME_CAP, nodes: STYLE_SANTA } };
        assert_eq!(npc_hat(&t), Some((GNOME_CAP, STYLE_SANTA)));
        assert_eq!(npc_hat(&net::npctint::Tint::color(0xFF0000FF)), None);
    }

    /// `two_sided`: every triangle again, wound the other way on copies of its
    /// vertices, normals turned in, layers in shade; on the disc, each hat
    /// (the Santa Hat, the kappa's, the turban) doubled, the same bounds.
    #[test]
    fn a_hat_has_an_inside() {
        let layer = |texture: i32| esf::Layer { flags: 1, texture, wrap: 0, blend: 0, color: [255, 200, 100, 255], uv_transform: [0.0; 9], lod_bias: 0.0, uv_rate: [0.0; 2], emissive: [0; 4] };
        let mut m = esf::Model {
            pos: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            uv: vec![[0.0; 2]; 3],
            normal: vec![[0.0, 0.0, 1.0]; 3],
            batches: vec![esf::Batch { material: 0, layers: vec![layer(7)], indices: vec![0, 1, 2], lod: None }],
            triangles: 1,
            ..Default::default()
        };
        two_sided(&mut m);
        assert_eq!((m.pos.len(), m.uv.len(), m.normal.len(), m.triangles), (6, 6, 6, 2));
        assert_eq!(m.batches[1].indices, [3, 5, 4], "the other winding, on the copies");
        assert_eq!(m.normal[3], [0.0, 0.0, -1.0], "turned in");
        assert_eq!(m.batches[1].layers[0].color, [153, 120, 60, 255], "in shade, alpha kept");
        assert_eq!(m.batches[0].layers[0].color, [255, 200, 100, 255], "the outside as it was");
        let Some(lib) = disc_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let (_, cs) = body(&lib, REFERENCE);
        for (src, style) in [(GNOME_CAP, STYLE_SANTA), (KAPPA_HAT, 0), (EFREETI_LORD, 0)] {
            let (_, mut hat) = hat_on_styled(&lib, REFERENCE, &cs, true, src, None, style).unwrap();
            dress(&mut hat, style, [255; 4], &lib);
            let (t, b) = (hat.triangles, bounds(&hat));
            two_sided(&mut hat);
            assert_eq!(hat.triangles, 2 * t, "{src:#010X}");
            assert_eq!(bounds(&hat), b, "{src:#010X}: the same place");
        }
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
        assert_eq!(hat_of(&auras), Some((KAPPA_HAT, None, 0)));
        auras[crate::aura::HELM] = PieceAura { kind: net::npctint::Aura::HAT, source: 0x1234, color: [0x20, 0xC0, 0x20, 0xFF], ..Default::default() };
        assert_eq!(hat_of(&auras), Some((0x1234, Some([0x20, 0xC0, 0x20]), 0)));
        auras[crate::aura::HELM].kind = net::npctint::Aura::CIRCLET;
        assert_eq!(hat_of(&auras), None);
        // the helm's hat apart (a 365-byte channel): the hat there, a shell in the helm's place
        auras[crate::aura::HELM] = PieceAura { kind: net::npctint::Aura::SHELL, color: [0xFF, 0x20, 0x20, 0xFF], ..Default::default() };
        auras[net::entity::HELM_HAT_AURA] = PieceAura { kind: net::npctint::Aura::HAT, source: EFREETI_LORD, ..Default::default() };
        assert_eq!(hat_of(&auras), Some((EFREETI_LORD, None, 0)), "the hat's own colours: the shell's colour is the shell's");
    }
}
