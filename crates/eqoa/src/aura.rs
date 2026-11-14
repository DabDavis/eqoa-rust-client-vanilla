//! A held item's own colour on its glow: go-eqoa-server sends each hand's
//! item's colour (`itempattern.color`, else its dye) in the live game's look
//! (`vi::cust::Look::item_color`, the channel's bytes 99-110 and the 0x002C
//! record's), which SOE's build read and drew nothing of. The owner,
//! 2026-11-13: "give dust a heirloom sword of twisted virtue (sword of virtue
//! with a evil aura instead of the virtuous aura)": the Sword of Virtue's
//! model in crimson (0xB0102AFF) where the disc's glows white-cyan.
//!
//! What glows on an item is three things, and each takes the colour's hue:
//! its materials' emissive colour (204, 255, 255 on the sword: F3 pass 26),
//! its additive layers (the sword's sheen, (204, 255, 255, 153)) and its
//! particle sprites (the sword's one emitter). Each keeps its own brightness
//! (`glow`), so a dark colour such as crimson still glows as brightly as the
//! cyan did; every other layer is tinted lightly (`TINT`), so a dyed or
//! coloured weapon with no glow shows its colour a little too.
//!
//! White (the server's "plain", 0xFFFFFFFF) and no colour at all (the disc's
//! look, a PS2's server) leave the item exactly as the disc draws it.
//!
//! An NPC's auras, a template any body can wear, chosen per NPC name by the
//! server (`npctint`, layout 3: `net::npctint::Aura`; the owner, 2026-11-13:
//! "the moogie aura should only be one aura that encompasses his whole body
//! form fitting similar to how anarchy online shows certain buffs", and
//! "moogies aura also can be a template to make any model have particle
//! effects"). Two, each chosen apart:
//!
//! - The shell ([`npc_shell`]): the body's own skinned mesh drawn again,
//!   pushed out along its normals, an additive rim in the glow's colour
//!   (`render::shell`). The default of a glow: Mr. Moogie's, and every glow a
//!   layout 2 server sends.
//! - The particles ([`npc_particles`]): an item's or model's particle sprite
//!   (its source, [`SWORD_OF_VIRTUE`]'s by default; [`source_emitter`]) on
//!   some of the body's nodes spread over it ([`body_nodes`], [`AURA_NODES`]
//!   unless the server names a count), each streaming up from its node in the
//!   world's up and following it, its particles in the glow's hue as the
//!   sword's are in its item's colour ([`recolor_quads`]), or in the source's
//!   own colours with no glow.
//!
//! No aura asked for and no glow: none of either, and nothing is looked up.
//!
//! The trim's aura ([`npc_trim`], [`piece_trims`]; the owner, 2026-11-13:
//! "lets also let armor trim glow also. and make dusts armor trim glow a
//! black aura"): kind 5 (`net::npctint::Aura::TRIM`) lights the trim of the
//! armour a body or piece wears (`vi::cust::TRIMMED`, found on each batch's
//! texture through its recolours, [`trim_batches`]) in the aura's colour, kind
//! 6 that and the shell (`render::trimglow`). A colour black or near black
//! (`net::npctint::dark`) makes any shell or trim a DARK aura: a smoky shadow
//! instead of a light, in the colour as sent (not times its alpha), with no
//! light of its own on the body ([`aura_rgb`]).
//!
//! The held items' auras ([`hand_aura`]; the owner, 2026-11-14: "give
//! zaeires equipped weapons the necro's tainted aura from the epic sickle"):
//! the channel's tenth and eleventh, the main hand's and the off hand's, on
//! whatever that hand holds. Kind epic (`net::npctint::Aura::EPIC`) lends the
//! held item an epic's own look, its source the epic's hand model
//! ([`borrow_epic`]: its emissive glow and additive sheen; [`hand_emitters`]:
//! its particle sprites, where the epic carries them), in the colour's hue
//! when it lights, else in its own; kind particles its particles alone.

use vi::fpu::{Ee, F};
use vi::geom::{Matrix44, Vect4};

/// How far a layer that does not glow goes toward the colour: its colour
/// times `1 - TINT + TINT * colour`.
pub const TINT: f32 = 0.25;

/// The colour a hand's item is drawn in, by its slot (0 main hand, 1 off
/// hand, 2 shield): none with the disc's look or a plain (white) item.
pub fn hand(look: &vi::cust::Look, slot: usize) -> Option<[u8; 3]> {
    let c = look.item_color?.get(slot).copied()?;
    let rgb = [c[0], c[1], c[2]];
    (rgb != [255; 3]).then_some(rgb)
}

/// Each hand's (`hand`), none without a look.
pub fn hands(look: Option<&vi::cust::Look>) -> [Option<[u8; 3]>; 3] {
    std::array::from_fn(|i| look.and_then(|l| hand(l, i)))
}

/// A glow `o` (any scale: 0-255 or 0-1) in the hue of `c`: `c`'s hue and
/// saturation at `o`'s brightness (its largest channel), times how much of
/// a colour `c` is: a full colour keeps all of it (crimson, 0xB0102A, keeps
/// 97%: the sword's (204, 255, 255) emissive goes to (248, 23, 59)), a grey
/// its lightness (black puts the glow out, white keeps it white). Black stays
/// black: there is nothing to recolour.
pub fn glow(o: [f32; 3], c: [u8; 3]) -> [f32; 3] {
    let v = o[0].max(o[1]).max(o[2]);
    if v <= 0.0 {
        return o;
    }
    let hi = c[0].max(c[1]).max(c[2]) as f32;
    if hi <= 0.0 {
        return [0.0; 3];
    }
    let lo = c[0].min(c[1]).min(c[2]) as f32;
    let sat = (hi - lo) / hi;
    let k = sat + (1.0 - sat) * hi / 255.0;
    let s = v * k / hi;
    c.map(|x| x as f32 * s)
}

/// `glow` on a colour of bytes, its alpha kept.
pub fn glow8(o: [u8; 4], c: [u8; 3]) -> [u8; 4] {
    let g = glow([o[0] as f32, o[1] as f32, o[2] as f32], c);
    [byte(g[0]), byte(g[1]), byte(g[2]), o[3]]
}

/// A layer that does not glow, lightly toward `c` (`TINT`), its alpha kept.
pub fn tint8(o: [u8; 4], c: [u8; 3]) -> [u8; 4] {
    let t = |k: usize| byte(o[k] as f32 * (1.0 - TINT + TINT * c[k] as f32 / 255.0));
    [t(0), t(1), t(2), o[3]]
}

fn byte(x: f32) -> u8 {
    x.round().clamp(0.0, 255.0) as u8
}

/// An item's model in colour `c`: every material's emissive and every
/// additive layer's colour in its hue (`glow`), every other layer tinted
/// (`tint8`).
pub fn recolor(m: &mut esf::Model, c: [u8; 3]) {
    for l in m.batches.iter_mut().flat_map(|b| b.layers.iter_mut()) {
        if l.emissive[..3] != [0, 0, 0] {
            l.emissive = glow8(l.emissive, c);
        }
        l.color = if l.blend == 1 { glow8(l.color, c) } else { tint8(l.color, c) };
    }
}

/// An item's particles' billboards in colour `c` (`glow`, alpha kept).
pub fn recolor_quads(b: &mut render::billboard::Batch, c: [u8; 3]) {
    for q in &mut b.quads {
        let g = glow([q.color[0], q.color[1], q.color[2]], c);
        q.color = [g[0], g[1], g[2], q.color[3]];
    }
}

/// The Sword of Virtue (ITEM.ESF): its one particle sprite, the white-cyan
/// sparkles that rise along its blade, is a glowing NPC's aura.
pub const SWORD_OF_VIRTUE: u32 = 0x88EC_092D;

/// How many of a body's nodes carry its particles unless the server names a
/// count: the sword's emitter keeps about 25 particles alive (4.1 a second,
/// 6 s each), so a body's aura is about 200.
pub const AURA_NODES: usize = 8;

/// The most nodes a body's particles take, whatever the server names.
pub const AURA_NODES_MOST: usize = 32;

/// The colour of an NPC's glow ([`crate::npctint::glow_rgb`]), none for a
/// tint that does not glow and none without a tint.
pub fn npc_glow(t: Option<crate::npctint::Tint>) -> Option<[u8; 3]> {
    t.filter(|t| net::npctint::glows(t.glow)).map(|t| crate::npctint::glow_rgb(t.glow))
}

/// The colour an aura of tint `t` is drawn in, and whether it is dark: a
/// dark glow's own red, green and blue (`net::npctint::dark`), any other the
/// glow's ([`crate::npctint::glow_rgb`], times its alpha).
pub fn aura_rgb(t: &crate::npctint::Tint) -> ([u8; 3], bool) {
    if net::npctint::dark(t.glow) {
        let g = t.glow;
        ([(g >> 24) as u8, (g >> 16) as u8, (g >> 8) as u8], true)
    } else {
        (crate::npctint::glow_rgb(t.glow), false)
    }
}

/// The colour of an NPC's shell, if it wears one (`Aura::shell`: asked for
/// or the default, and a glow that lights, or a dark one named): its glow's
/// ([`aura_rgb`]).
pub fn npc_shell(t: Option<crate::npctint::Tint>) -> Option<[u8; 3]> {
    t.filter(net::npctint::Aura::shell).map(|t| aura_rgb(&t).0)
}

/// An NPC's shell over its whole body ([`npc_shell`]), dark or not.
pub fn npc_shell_part(t: Option<crate::npctint::Tint>) -> Option<render::shell::PartOf> {
    let color = npc_shell(t)?;
    Some(render::shell::PartOf { color, batches: None, dark: t.is_some_and(|t| aura_rgb(&t).1) })
}

/// The batches of `model` (`only` of them; `None` all) whose texture has
/// trim (`vi::cust::has_trim`, the texture a layer's recolours
/// were made from: `render::recolor::root_of`), each with its trim's mask
/// (`render::recolor::TRIM_MASK` of that texture; `taken`, a library's
/// lookup, as `recolored`'s).
pub fn trim_batches(model: &esf::Model, only: Option<&[usize]>, taken: &dyn Fn(i32) -> bool) -> Vec<(usize, i32)> {
    let mut out = Vec::new();
    for (i, b) in model.batches.iter().enumerate() {
        if only.is_some_and(|o| !o.contains(&i)) {
            continue;
        }
        let root = b.layers.iter().map(|l| render::recolor::root_of(l.texture)).find(|&r| r != 0 && vi::cust::has_trim(r as u32));
        if let Some(root) = root {
            out.push((i, render::recolor::recolored(root, render::recolor::TRIM_MASK << 24, taken)));
        }
    }
    out
}

/// An NPC's trim aura over its whole body (`Aura::trim`: kind 5 or 6 and a
/// glow that lights or a dark one): the trim of every batch that has one
/// ([`trim_batches`]); none on a body with no trim.
pub fn npc_trim(model: &esf::Model, t: Option<crate::npctint::Tint>, taken: &dyn Fn(i32) -> bool) -> Option<render::trimglow::TrimOf> {
    let t = t.filter(net::npctint::Aura::trim)?;
    let batches = trim_batches(model, None, taken);
    let (color, dark) = aura_rgb(&t);
    (!batches.is_empty()).then_some(render::trimglow::TrimOf { color, dark, batches })
}

/// An NPC's particles: whose particle sprite, on how many nodes, in which
/// hue (the glow's; none, the source's own colours).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Particles {
    pub source: u32,
    pub nodes: usize,
    pub hue: Option<[u8; 3]>,
}

/// An NPC's particles, if it wears them (`Aura::particles`): the server's
/// source (0: [`SWORD_OF_VIRTUE`]) and node count (0: [`AURA_NODES`], at most
/// [`AURA_NODES_MOST`]), in the glow's hue.
pub fn npc_particles(t: Option<crate::npctint::Tint>) -> Option<Particles> {
    let t = t.filter(net::npctint::Aura::particles)?;
    let source = if t.aura.source == 0 { SWORD_OF_VIRTUE } else { t.aura.source };
    let nodes = if t.aura.nodes == 0 { AURA_NODES } else { (t.aura.nodes as usize).min(AURA_NODES_MOST) };
    Some(Particles { source, nodes, hue: npc_glow(Some(t)) })
}

/// The particle definition a source's particles are: an item's (or any
/// group sprite's) first particle sprite (`particles::item_emitters`), else a
/// character sprite's first own emitter (`particles::part_emitters`); none
/// for a source with neither, or not loaded.
pub fn source_emitter(lib: &esf::Library, source: u32) -> Option<esf::Ref> {
    crate::particles::item_emitters(lib, source).first().map(|e| e.def).or_else(|| crate::particles::part_emitters(lib, source).first().map(|e| e.def))
}

/// `n` of `points` spread over them, by index: the one nearest their middle,
/// then each the farthest from those taken (the lowest index of equals), so
/// a body's extremities (head, feet, tail) and its middle are covered. All of
/// them when there are no more than `n`.
pub fn spread(points: &[[f32; 3]], n: usize) -> Vec<usize> {
    if points.len() <= n {
        return (0..points.len()).collect();
    }
    let d2 = |a: &[f32; 3], b: &[f32; 3]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2);
    let k = points.len() as f32;
    let mid = [0, 1, 2].map(|i| points.iter().map(|p| p[i]).sum::<f32>() / k);
    let first = (0..points.len()).min_by(|&a, &b| d2(&points[a], &mid).total_cmp(&d2(&points[b], &mid)).then(a.cmp(&b))).unwrap_or(0);
    let mut taken = vec![first];
    let mut near: Vec<f32> = points.iter().map(|p| d2(p, &points[first])).collect();
    while taken.len() < n {
        let next = (0..points.len()).filter(|i| !taken.contains(i)).max_by(|&a, &b| near[a].total_cmp(&near[b]).then(b.cmp(&a))).unwrap_or(0);
        taken.push(next);
        for (i, p) in points.iter().enumerate() {
            near[i] = near[i].min(d2(p, &points[next]));
        }
    }
    taken
}

/// The `n` nodes of a body that carry its particles, spread over its bind
/// pose (each node's model-space place, `SetNodeBindPose`).
pub fn body_nodes(cs: &vi::csprite::CSprite<Ee>, n: usize) -> Vec<i32> {
    let at: Vec<[f32; 3]> = cs.h.nodes.iter().map(|n| [n.bind.trans.x.0, n.bind.trans.y.0, n.bind.trans.z.0]).collect();
    spread(&at, n).into_iter().map(|i| i as i32).collect()
}

/// An aura emitter's world transform at a node's (`w`, its world transform):
/// at the node's place, unscaled, its +z the world's up (+y), so the sword's
/// stream rises from the node whichever way the body turns or bends.
pub fn upright(w: &Matrix44<Ee>) -> Matrix44<Ee> {
    let (o, i) = (F::ZERO, F::ONE);
    Matrix44 { m: [Vect4::new(i, o, o, o), Vect4::new(o, o, -i, o), Vect4::new(o, i, o, o), Vect4::new(w.m[3].x, w.m[3].y, w.m[3].z, i)] }
}

/// An NPC's particles this frame: the source's emitter (`def`, its
/// definition record, [`source_emitter`] of `source`) on each of its `nodes`
/// (`body_nodes`) of sprite `cs`, under the centre transform `centre`, in
/// `hue` (none: its own colours), run while the body is drawn. Keyed by the
/// holder, `particles::AURA`, the source and the node's place in the list.
#[allow(clippy::too_many_arguments)]
pub fn body_emitters(holder: i32, cs: &vi::csprite::CSprite<Ee>, nodes: &[i32], centre: &Matrix44<Ee>, source: u32, def: esf::Ref, hue: Option<[u8; 3]>, drawn: bool) -> Vec<crate::particles::InHand> {
    use crate::particles::{AURA, How, InHand};
    nodes
        .iter()
        .enumerate()
        .filter_map(|(k, &node)| {
            let n = cs.h.nodes.get(usize::try_from(node).ok()?)?;
            let world = upright(&Matrix44::mul(&n.world, centre));
            Some(InHand { key: (holder, AURA, source, k), world, def, how: How::Part { use_location: false, once: false }, drawn, tint: hue })
        })
        .collect()
}

/// The worn pieces an aura can be on, as `net::entity::PieceAura`s come:
/// chest, bracer, gloves, legs, boots (the text slots 0 to 4), the helm and
/// the robe (the owner, 2026-11-13: "even pieces of armor like bracers only if
/// that piece has aura. or only a breastplate etc. and they can have their own
/// aura color each").
pub const HELM: usize = 5;
pub const ROBE: usize = 6;

/// A worn piece's aura read as an NPC's tint is (`npc_shell`,
/// `npc_particles`): its colour the glow, its kind, source and nodes the
/// aura; none for a piece with no aura (all zero).
pub fn piece_tint(p: &net::entity::PieceAura) -> Option<crate::npctint::Tint> {
    (*p != net::entity::PieceAura::default()).then(|| crate::npctint::Tint {
        color: 0xFFFF_FFFF,
        glow: u32::from_be_bytes(p.color),
        aura: net::npctint::Aura { kind: p.kind, source: p.source, nodes: p.nodes },
    })
}

/// The batches of `model` a body slot (0 chest .. 4 boots) draws: its text
/// slot's palette entry's (`cs.text_slots`) and the made-up batches the
/// slotless bodies dress it with (`slotless::ENTRY`).
pub fn slot_batches(model: &esf::Model, cs: &vi::csprite::CSprite<Ee>, slot: usize) -> Vec<usize> {
    let entry = cs.text_slots.get(slot).map_or(-1, |s| s.0);
    let made = crate::slotless::ENTRY + slot as i32;
    model.batches.iter().enumerate().filter(|(_, b)| (entry != -1 && b.material == entry) || b.material == made).map(|(i, _)| i).collect()
}

/// The nodes `batches` of `model` are skinned to: each vertex's heaviest
/// bone (a palette index is a node's), in order, once each.
pub fn batch_nodes(model: &esf::Model, batches: &[usize]) -> Vec<i32> {
    let mut out: Vec<i32> = Vec::new();
    for b in batches.iter().filter_map(|&i| model.batches.get(i)) {
        for &v in &b.indices {
            let (Some(bones), Some(w)) = (model.bones.get(v as usize), model.weights.get(v as usize)) else { continue };
            let k = (0..4).max_by_key(|&k| (w[k], std::cmp::Reverse(k))).unwrap_or(0);
            if w[k] > 0 && bones[k] != 255 && !out.contains(&(bones[k] as i32)) {
                out.push(bones[k] as i32);
            }
        }
    }
    out
}

/// `n` of the `among` nodes of a body spread over its bind pose (`spread`).
pub fn nodes_among(cs: &vi::csprite::CSprite<Ee>, among: &[i32], n: usize) -> Vec<i32> {
    let among: Vec<i32> = among.iter().copied().filter(|&k| usize::try_from(k).is_ok_and(|k| k < cs.h.nodes.len())).collect();
    let at: Vec<[f32; 3]> = among.iter().map(|&k| cs.h.nodes[k as usize].bind.trans).map(|t| [t.x.0, t.y.0, t.z.0]).collect();
    spread(&at, n).into_iter().map(|i| among[i]).collect()
}

/// One aura's particles a body carries: on `nodes`, `particles`' source in
/// its hue, keyed from `first` (each piece's apart).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worn {
    pub nodes: Vec<i32>,
    pub particles: Particles,
    pub first: usize,
}

/// The body's auras from its worn pieces (`auras`, the channel's, by
/// [`HELM`]/[`ROBE`] and the body slots), the helm's aside (it is an
/// attachment: [`helm_aura`]): each piece's shell over its slot's batches in
/// its colour, and its particles on the nodes those batches are skinned to.
/// A robe (`robed`) covers the chest and the legs: their aura is the robe's.
/// Nothing for pieces with none.
pub fn pieces(model: &esf::Model, cs: &vi::csprite::CSprite<Ee>, auras: &[net::entity::PieceAura; net::entity::AURAS], robed: bool) -> (Vec<render::shell::PartOf>, Vec<Worn>) {
    let (mut shells, mut worn) = (Vec::new(), Vec::new());
    for slot in 0..5 {
        let a = if robed && (slot == 0 || slot == 3) { &auras[ROBE] } else { &auras[slot] };
        let Some(t) = piece_tint(a) else { continue };
        let (shell, particles) = (npc_shell(Some(t)), npc_particles(Some(t)));
        if shell.is_none() && particles.is_none() {
            continue;
        }
        let batches = slot_batches(model, cs, slot);
        if batches.is_empty() {
            continue;
        }
        if let Some(pt) = particles {
            let nodes = nodes_among(cs, &batch_nodes(model, &batches), pt.nodes);
            if !nodes.is_empty() {
                worn.push(Worn { nodes, particles: pt, first: 64 * (slot + 1) });
            }
        }
        if let Some(color) = shell {
            shells.push(render::shell::PartOf { color, batches: Some(batches), dark: aura_rgb(&t).1 });
        }
    }
    (shells, worn)
}

/// The worn pieces' trim auras (`auras`, as [`pieces`] reads them; a robe's
/// in the chest's and the legs' place): each piece of kind 5 or 6 lights the
/// trim of its slot's batches ([`trim_batches`]); a piece with no trim (the
/// gauntlets, any set but the plate) shows none.
pub fn piece_trims(model: &esf::Model, cs: &vi::csprite::CSprite<Ee>, auras: &[net::entity::PieceAura; net::entity::AURAS], robed: bool, taken: &dyn Fn(i32) -> bool) -> Vec<render::trimglow::TrimOf> {
    let mut out = Vec::new();
    for slot in 0..5 {
        let a = if robed && (slot == 0 || slot == 3) { &auras[ROBE] } else { &auras[slot] };
        let Some(t) = piece_tint(a).filter(net::npctint::Aura::trim) else { continue };
        let batches = trim_batches(model, Some(&slot_batches(model, cs, slot)), taken);
        if !batches.is_empty() {
            let (color, dark) = aura_rgb(&t);
            out.push(render::trimglow::TrimOf { color, dark, batches });
        }
    }
    out
}

/// The body's aura (`auras[BODY]`, go-eqoa-server's charaura slot 7: the
/// owner, 2026-11-13: "a wholebody just applies it to body and underclothes,
/// armor will cover it up, but can be /aura separately").
pub const BODY: usize = net::entity::BODY_AURA;

/// The cape's aura (`auras[CAPE]`, the channel's ninth: [`cape_aura`]).
pub const CAPE: usize = net::entity::CAPE_AURA;

/// Which body slots (chest .. boots) a look covers with armour: a set worn
/// there (not 0, the bare body or its underclothes), or a robe over the chest
/// and the legs.
pub fn covered(look: &vi::cust::Look) -> [bool; 5] {
    std::array::from_fn(|i| look.armor[i] != 0 || (look.robe >= 0 && (i == 0 || i == 3)))
}

/// The batches of `model` the body's aura covers: every batch but those of
/// the slots armour covers (`covered`, [`slot_batches`]), so a piece worn
/// hides the body's aura under it and the skin, the face and the
/// underclothes keep it.
pub fn body_batches(model: &esf::Model, cs: &vi::csprite::CSprite<Ee>, covered: [bool; 5]) -> Vec<usize> {
    let under: Vec<usize> = (0..5).filter(|&s| covered[s]).flat_map(|s| slot_batches(model, cs, s)).collect();
    (0..model.batches.len()).filter(|b| !under.contains(b)).collect()
}

/// The body's aura (`auras[BODY]`): its shell over [`body_batches`] (dark or
/// not), its particles on the nodes those are skinned to; a trim kind lights
/// nothing there (the body has no plate), its shell drawn with trimshell.
pub fn body_aura(model: &esf::Model, cs: &vi::csprite::CSprite<Ee>, auras: &[net::entity::PieceAura; net::entity::AURAS], covered: [bool; 5]) -> (Option<render::shell::PartOf>, Option<Worn>) {
    let Some(t) = piece_tint(&auras[BODY]) else { return (None, None) };
    let (shell, particles) = (npc_shell(Some(t)), npc_particles(Some(t)));
    if shell.is_none() && particles.is_none() {
        return (None, None);
    }
    let batches = body_batches(model, cs, covered);
    if batches.is_empty() {
        return (None, None);
    }
    let worn = particles.and_then(|pt| {
        let nodes = nodes_among(cs, &batch_nodes(model, &batches), pt.nodes);
        (!nodes.is_empty()).then_some(Worn { nodes, particles: pt, first: 64 * (BODY + 1) })
    });
    (shell.map(|color| render::shell::PartOf { color, batches: Some(batches), dark: aura_rgb(&t).1 }), worn)
}

/// The helm's aura (`auras[HELM]`): its shell over the whole helm, and its
/// particles on the helm's node `node`. (The helm the plate wears, set 4's,
/// is plain steel: no trim.)
pub fn helm_aura(auras: &[net::entity::PieceAura; net::entity::AURAS], node: i32) -> (Option<render::shell::PartOf>, Option<Worn>) {
    let t = piece_tint(&auras[HELM]);
    (npc_shell_part(t), npc_particles(t).map(|particles| Worn { nodes: vec![node], particles, first: 64 * (HELM + 1) }))
}

/// The helm's trim aura (`auras[HELM]` of kind 5 or 6): the trim of the
/// helm's own batches (`helm`, the attached model, [`trim_batches`]); none on
/// a helm with no trim (set 4's plain steel). The helm is drawn apart from
/// the body, so [`piece_trims`] never reaches it (the owner, 2026-11-14:
/// "oof's helm isnt showing aura for trim").
pub fn helm_trim(helm: &esf::Model, auras: &[net::entity::PieceAura; net::entity::AURAS], taken: &dyn Fn(i32) -> bool) -> Option<render::trimglow::TrimOf> {
    let t = piece_tint(&auras[HELM]).filter(net::npctint::Aura::trim)?;
    let batches = trim_batches(helm, None, taken);
    let (color, dark) = aura_rgb(&t);
    (!batches.is_empty()).then_some(render::trimglow::TrimOf { color, dark, batches })
}

/// The cape's aura (`auras[CAPE]`, go-eqoa-server's charaura slot 8 or the
/// worn cape item's itemaura; the owner, 2026-11-14: "also cannot apply aura
/// to capes yet"), on the cape's own model (`cape`, `crate::cape::model`:
/// skinned to the wearer's nodes, drawn apart from the body): its shell over
/// the whole cape, its particles on the wearer's nodes the cape is skinned to
/// (`cs`), and a trim kind's glow over the cape's trim ([`trim_batches`]).
/// No cape's art has trim (`vi::cust::TRIMMED` names none of theirs), so a
/// trim kind there lights the whole cape as a shell instead of nothing: a
/// player who put a trim aura on every piece sees the cape glow with them.
pub fn cape_aura(cape: &esf::Model, cs: &vi::csprite::CSprite<Ee>, auras: &[net::entity::PieceAura; net::entity::AURAS], taken: &dyn Fn(i32) -> bool) -> (Option<render::shell::PartOf>, Option<Worn>, Option<render::trimglow::TrimOf>) {
    let Some(t) = piece_tint(&auras[CAPE]) else { return (None, None, None) };
    let worn = npc_particles(Some(t)).and_then(|pt| {
        let all: Vec<usize> = (0..cape.batches.len()).collect();
        let nodes = nodes_among(cs, &batch_nodes(cape, &all), pt.nodes);
        (!nodes.is_empty()).then_some(Worn { nodes, particles: pt, first: 64 * (CAPE + 1) })
    });
    let (color, dark) = aura_rgb(&t);
    let trim = Some(t).filter(net::npctint::Aura::trim).map(|_| trim_batches(cape, None, taken));
    let shell = match &trim {
        // a trim with no trim to light: the shell in its colour
        Some(b) if b.is_empty() => Some(render::shell::PartOf { color, batches: None, dark }),
        _ => npc_shell_part(Some(t)),
    };
    let trim = trim.filter(|b| !b.is_empty()).map(|batches| render::trimglow::TrimOf { color, dark, batches });
    (shell, worn, trim)
}

/// Every aura's particles a body carries this frame (`body_emitters` for
/// each of `worn`, its source looked up: one with no particle sprite, none).
pub fn worn_emitters(lib: &esf::Library, holder: i32, cs: &vi::csprite::CSprite<Ee>, worn: &[Worn], centre: &Matrix44<Ee>, drawn: bool) -> Vec<crate::particles::InHand> {
    let mut out = Vec::new();
    for w in worn {
        let Some(def) = source_emitter(lib, w.particles.source) else { continue };
        out.extend(body_emitters(holder, cs, &w.nodes, centre, w.particles.source, def, w.particles.hue, drawn).into_iter().map(|mut h| {
            h.key.3 += w.first;
            h
        }));
    }
    out
}

/// The held items' auras (go-eqoa-server's charaura slots 9 and 10, the
/// channel's tenth and eleventh: `net::entity::MAIN_HAND_AURA`,
/// `OFF_HAND_AURA`; the owner, 2026-11-14: "give zaeires equipped weapons the
/// necro's tainted aura from the epic sickle"): drawn on whatever the hand
/// holds, the off hand's on a shield held there too, nothing on an empty hand.
pub const MAIN_HAND: usize = net::entity::MAIN_HAND_AURA;
pub const OFF_HAND: usize = net::entity::OFF_HAND_AURA;

/// The emitter index (the `particles::InHand` key's fourth) a hand's borrowed
/// particles are counted from, apart from the held item's own.
pub const HAND_AURA_FIRST: usize = 1 << 16;

/// The Sickle of Avhi Escron, the necromancer's epic (ITEM.ESF's
/// Necromancer_Sickle): its "tainted aura" is its five materials' olive
/// emissive (60, 90, 0), which keeps it lit green by night, and its one
/// particle sprite, a green mist rising off its blade. Its other layer (a
/// rust texture alpha-blended over the blade and grip) is its own art, not
/// a glow, and is not lent.
pub const SICKLE_OF_AVHI_ESCRON: u32 = 0x5FD4_AE7C;

/// A held item's aura, by its hand (0 the main hand, 1 the off hand, 2 a
/// shield: the off hand's): one of kind epic or particles, none for any
/// other (a server never sends another there).
pub fn hand_aura(auras: &[net::entity::PieceAura; net::entity::AURAS], hand: usize) -> Option<net::entity::PieceAura> {
    let a = auras[if hand == 0 { MAIN_HAND } else { OFF_HAND }];
    matches!(a.kind, net::npctint::Aura::EPIC | net::npctint::Aura::PARTICLES).then_some(a)
}

/// The model a hand's aura borrows from: its source, 0 the Sword of Virtue.
pub fn hand_source(a: &net::entity::PieceAura) -> u32 {
    if a.source == 0 { SWORD_OF_VIRTUE } else { a.source }
}

/// The hue a hand's aura is drawn in: its colour when it lights, none (the
/// source's own colours) without.
pub fn hand_hue(a: &net::entity::PieceAura) -> Option<[u8; 3]> {
    net::npctint::glows(u32::from_be_bytes(a.color)).then_some([a.color[0], a.color[1], a.color[2]])
}

/// An epic's own look lent to a held item (kind epic): `src`, the epic's
/// model, lends `m` its glow: the brightest emissive colour of its materials
/// on every layer of `m` (a material's emissive is its light by night, F3
/// pass 26: the item stays lit as the epic is), and its first additive layer
/// (the Sword of Virtue's sheen: a material's own texture added over itself)
/// laid over each of `m`'s batches the same way, `m`'s own texture in the
/// epic's sheen colour. In `hue` when one is named (`glow8`, as a held item's
/// own colour recolours its glow), else in the epic's own colours. An epic
/// with neither lends nothing here; its particles are apart
/// ([`hand_emitters`]). The epic itself in hand (`same`) keeps its own look,
/// recoloured only.
pub fn borrow_epic(m: &mut esf::Model, src: &esf::Model, hue: Option<[u8; 3]>, same: bool) {
    if same {
        if let Some(c) = hue {
            recolor(m, c);
        }
        return;
    }
    let layers = || src.batches.iter().flat_map(|b| b.layers.iter());
    let shade = |c: [u8; 4]| hue.map_or(c, |h| glow8(c, h));
    let emissive = layers().map(|l| l.emissive).filter(|e| e[..3] != [0, 0, 0]).max_by_key(|e| e[0] as u32 + e[1] as u32 + e[2] as u32).map(shade);
    let sheen = layers().find(|l| l.blend == 1).map(|l| shade(l.color));
    for b in &mut m.batches {
        if let Some(e) = emissive {
            b.layers.iter_mut().for_each(|l| l.emissive = e);
        }
        if let (Some(c), Some(base)) = (sheen, b.layers.first().cloned()) {
            b.layers.push(esf::Layer { blend: 1, color: c, ..base });
        }
    }
}

/// How far along a held item its trail runs: the farther of its two point
/// sprites (`trails::weapon_points`) from the hand; none for an item without
/// them (or with them at the hand).
pub fn reach(lib: &esf::Library, item: u32) -> Option<f32> {
    let p = crate::trails::weapon_points(lib, item)?;
    let len = |m: &Matrix44<Ee>| (m.m[3].x.0 * m.m[3].x.0 + m.m[3].y.0 * m.m[3].y.0 + m.m[3].z.0 * m.m[3].z.0).sqrt();
    Some(len(&p.a).max(len(&p.b))).filter(|&r| r > 0.01)
}

/// The particles a hand's aura `a` puts on the item `item` held at `at` (its
/// node's world transform), by holder `holder` and hand `hand`: each of its
/// source's particle sprites (`particles::item_emitters`) under the member
/// transform it has in the source, its place moved along to the same share of
/// the item's reach as of the source's ([`reach`]: the druid's staff's
/// sparkles, 1.25 up a 1.61 staff, at 0.78 of a scimitar's blade, not past
/// its tip; the place kept when either has no trail), in the aura's hue
/// (none: the source's own colours), keyed from [`HAND_AURA_FIRST`]. None
/// when the source is the item itself (its own particles are already in
/// hand) or has none.
pub fn hand_emitters(lib: &esf::Library, holder: i32, hand: usize, item: u32, at: &Matrix44<Ee>, a: &net::entity::PieceAura, drawn: bool) -> Vec<crate::particles::InHand> {
    let src = hand_source(a);
    if src == item {
        return Vec::new();
    }
    let tint = hand_hue(a);
    let k = reach(lib, item).zip(reach(lib, src)).map_or(1.0, |(i, s)| i / s);
    crate::particles::item_emitters(lib, src)
        .into_iter()
        .enumerate()
        .map(|(n, e)| {
            let mut member = e.member;
            if k != 1.0 {
                let t = member.m[3];
                member.m[3] = Vect4::new(F::new(t.x.0 * k), F::new(t.y.0 * k), F::new(t.z.0 * k), t.w);
            }
            crate::particles::InHand { key: (holder, hand, src, HAND_AURA_FIRST + n), world: Matrix44::mul(&member, at), def: e.def, how: crate::particles::How::Sprite(e.flags), drawn, tint }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRIMSON: [u8; 3] = [0xB0, 0x10, 0x2A];

    /// The look's colours by slot: white and the disc's look are none.
    #[test]
    fn a_hands_colour_off_the_look() {
        let mut l = vi::cust::Look::default();
        assert_eq!(hands(Some(&l)), [None; 3], "the disc's look: none");
        assert_eq!(hands(None), [None; 3]);
        l.item_color = Some([[0xB0, 0x10, 0x2A, 0xFF], [0xFF; 4], [0, 0, 0xFF, 0xFF]]);
        assert_eq!(hands(Some(&l)), [Some(CRIMSON), None, Some([0, 0, 0xFF])], "white is plain");
    }

    /// The sword's glow goes crimson at the cyan's brightness; a grey keeps
    /// its lightness of it; black stays black.
    #[test]
    fn the_glow_takes_the_hue_and_keeps_the_brightness() {
        assert_eq!(glow8([204, 255, 255, 255], CRIMSON), [248, 23, 59, 255]);
        assert_eq!(glow8([204, 255, 255, 153], CRIMSON), [248, 23, 59, 153], "the sheen layer's alpha kept");
        assert_eq!(glow8([204, 255, 255, 255], [255, 255, 255]), [255, 255, 255, 255]);
        assert_eq!(glow8([204, 255, 255, 255], [0x80, 0x80, 0x80]), [128, 128, 128, 255]);
        assert_eq!(glow8([204, 255, 255, 255], [0, 0, 0]), [0, 0, 0, 255], "black puts it out");
        assert_eq!(glow8([0, 0, 0, 255], CRIMSON), [0, 0, 0, 255], "no glow: none made");
        let f = glow([0.8, 1.0, 1.0], CRIMSON);
        assert!((f[0] - 248.0 / 255.0).abs() < 0.005 && f[1] < 0.1, "{f:?}: any scale");
        assert_eq!(tint8([128, 128, 128, 255], CRIMSON), [118, 98, 101, 255]);
    }

    /// `recolor` on a model like the sword's: emissive and sheen glow,
    /// the base tinted; the control, a white look, leaves it as it was.
    #[test]
    fn a_model_recoloured() {
        let layer = |blend: i32, color: [u8; 4], emissive: [u8; 4]| esf::Layer { flags: 1, texture: 7, wrap: 0, blend, color, uv_transform: [0.0; 9], lod_bias: 0.8, uv_rate: [0.0; 2], emissive };
        let e = [204, 255, 255, 255];
        let mut m = esf::Model::default();
        m.batches.push(esf::Batch { material: 0, layers: vec![layer(0, [128; 4], e), layer(1, [204, 255, 255, 153], e)], indices: vec![], lod: None });
        let before = m.batches.clone();
        recolor(&mut m, CRIMSON);
        let l = &m.batches[0].layers;
        assert_eq!((l[0].emissive, l[1].emissive), ([248, 23, 59, 255], [248, 23, 59, 255]));
        assert_eq!((l[0].color, l[1].color), ([118, 98, 101, 128], [248, 23, 59, 153]));
        assert_ne!(m.batches[0].layers, before[0].layers);
    }

    /// The Sword of Virtue off the disc in the Heirloom Sword of Twisted
    /// Virtue's crimson: all four materials' emissive and the blade's sheen
    /// red at the cyan's brightness, alphas and textures kept. Needs
    /// `run/eqoa.iso`; without it, SKIPs.
    #[test]
    fn the_sword_of_twisted_virtue() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let c = esf::Container::open(disc.read("ITEM.ESF").unwrap()).unwrap();
        let r = c.models().into_iter().find(|r| r.dict_id as u32 == 0x88EC_092D).expect("the Sword of Virtue");
        let virtue = c.build_model(r.node).expect("its model");
        let mut twisted = virtue.clone();
        recolor(&mut twisted, CRIMSON);
        for (a, b) in virtue.batches.iter().zip(&twisted.batches) {
            for (la, lb) in a.layers.iter().zip(&b.layers) {
                assert_eq!((la.emissive, lb.emissive), ([204, 255, 255, 255], [248, 23, 59, 255]));
                assert_eq!((lb.texture, lb.blend, lb.color[3]), (la.texture, la.blend, la.color[3]));
            }
        }
        assert_eq!(twisted.batches[3].layers[1].color, [248, 23, 59, 153], "the sheen");
    }

    /// The spread: the middle first, then each the farthest from those
    /// taken; few points are all taken; the same points the same pick.
    #[test]
    fn nodes_spread_over_a_body() {
        let line: Vec<[f32; 3]> = (0..9).map(|i| [i as f32, 0.0, 0.0]).collect();
        assert_eq!(spread(&line, 3), [4, 0, 8], "the middle, then the ends");
        assert_eq!(spread(&line, 5), [4, 0, 8, 2, 6]);
        assert_eq!(spread(&line[..3], 8), [0, 1, 2], "fewer than asked: all");
        assert_eq!(spread(&line, 5), spread(&line, 5));
        assert!(spread(&[], 8).is_empty());
    }

    /// Only a glow that lights makes an aura, in the glow's colour times its
    /// alpha (Mr. Moogie's 0xFF0000FF: pure red); a tint's colour alone, a
    /// black glow, one of alpha 0, and no tint make none.
    #[test]
    fn only_a_glow_makes_an_aura() {
        use crate::npctint::Tint;
        assert_eq!(npc_glow(Some(Tint { color: 0xFF60_60FF, glow: 0xFF00_00FF, ..Default::default() })), Some([255, 0, 0]));
        assert_eq!(npc_glow(Some(Tint { color: 0xFFFF_FFFF, glow: 0xFF40_4080, ..Default::default() })), Some([128, 32, 32]));
        assert_eq!(npc_glow(Some(Tint::color(0x558C_50FF))), None);
        assert_eq!(npc_glow(Some(Tint { color: 0xFFFF_FFFF, glow: 0x0000_00FF, ..Default::default() })), None);
        assert_eq!(npc_glow(Some(Tint { color: 0xFFFF_FFFF, glow: 0xFF00_0000, ..Default::default() })), None);
        assert_eq!(npc_glow(None), None);
        let w = Matrix44::<Ee>::transform(&vi::vect::Vect3::of(0.3, 1.0, 0.2), F::new(2.0), &vi::vect::Vect3::of(5.0, 6.0, 7.0));
        let u = upright(&w);
        let up = u.mul_point(&vi::vect::Vect3::of(0.0, 0.0, 1.0));
        assert_eq!((up.x.0, up.y.0, up.z.0), (5.0, 7.0, 7.0), "+z is the world's up, unscaled, at the node");
    }

    /// Which aura a tint gives: a glow with no aura named (a layout 2
    /// server's, Mr. Moogie's) the shell alone, in the glow; particles alone
    /// or both as named, the source and count the server's or the defaults
    /// (the Sword of Virtue, 8 nodes, at most 32), in the glow's hue or, with
    /// no glow, the source's own colours; none named, nothing; a colour alone
    /// or no tint, nothing at all.
    #[test]
    fn each_aura_as_the_server_names_it() {
        use crate::npctint::Tint;
        use net::npctint::Aura;
        let t = |glow: u32, kind: u8, source: u32, nodes: u8| Some(Tint { color: 0xFFFF_FFFF, glow, aura: Aura { kind, source, nodes } });
        let moogie = Some(Tint { color: 0xFF60_60FF, glow: 0xFF00_00FF, ..Default::default() });
        assert_eq!((npc_shell(moogie), npc_particles(moogie)), (Some([255, 0, 0]), None), "the default: the shell");
        assert_eq!(npc_shell(t(0x40FF_40FF, Aura::SHELL, 0, 0)), Some([64, 255, 64]));
        let sword = Particles { source: SWORD_OF_VIRTUE, nodes: AURA_NODES, hue: Some([64, 255, 64]) };
        assert_eq!((npc_shell(t(0x40FF_40FF, Aura::PARTICLES, 0, 0)), npc_particles(t(0x40FF_40FF, Aura::PARTICLES, 0, 0))), (None, Some(sword)));
        assert_eq!((npc_shell(t(0x40FF_40FF, Aura::BOTH, 0, 0)), npc_particles(t(0x40FF_40FF, Aura::BOTH, 0, 0))), (Some([64, 255, 64]), Some(sword)));
        assert_eq!(npc_particles(t(0x40FF_40FF, Aura::BOTH, 0x1234_5678, 12)), Some(Particles { source: 0x1234_5678, nodes: 12, hue: Some([64, 255, 64]) }));
        assert_eq!(npc_particles(t(0x40FF_40FF, Aura::BOTH, 0, 200)).map(|p| p.nodes), Some(AURA_NODES_MOST));
        assert_eq!(npc_particles(t(0, Aura::PARTICLES, 0, 0)), Some(Particles { hue: None, ..sword }), "no glow: its own colours");
        assert_eq!(npc_shell(t(0, Aura::BOTH, 0, 0)), None, "no glow: no shell");
        assert_eq!((npc_shell(t(0xFF00_00FF, Aura::NONE, 0, 0)), npc_particles(t(0xFF00_00FF, Aura::NONE, 0, 0))), (None, None));
        let plain = Some(Tint::color(0x558C_50FF));
        assert_eq!((npc_shell(plain), npc_particles(plain), npc_shell(None), npc_particles(None)), (None, None, None, None));
    }

    /// Worn pieces' auras on a human in plate (set 4): red bracers and a
    /// blue breastplate are two shell parts, each over its own slot's batches
    /// alone (none shared), in its own colour; bracers' particles on 3 of the
    /// nodes the bracers are skinned to (the forearms', not the chest's); a
    /// robe's aura takes the chest's and the legs' place; no piece with an
    /// aura, nothing. A helm's: its shell, its particles on its node. Needs
    /// `run/eqoa.iso`; without it, SKIPs.
    #[test]
    fn each_worn_piece_its_own_aura() {
        use net::entity::PieceAura;
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", esf::Container::open(disc.read("CHAR.ESF").unwrap()).unwrap());
        lib.add("CHARCUST.CSF", esf::Container::open(disc.read("CHARCUST.CSF").unwrap()).unwrap());
        let human = 0x70D8_98C6u32 as i32;
        let r = lib.lookup(human).expect("the human man");
        let mut model = lib.build_model(r).unwrap();
        let mut anims = vi::hsprite::AnimScene::<Ee>::default();
        let cs = vi::character::load_csprite(&lib, r, &mut anims, &mut vi::character::AnimCache::new(), &mut vi::csprite::Rand(1)).unwrap();
        let look = vi::cust::Look { armor: [4; 6], robe: -1, ..Default::default() };
        crate::play::dress_look(&lib, human, &mut model, &cs, &look, false, &vi::cust::Tables::retail(), true);
        let piece = |c: u32, kind: u8, nodes: u8| PieceAura { color: c.to_be_bytes(), kind, source: 0, nodes };
        let mut auras = [PieceAura::default(); net::entity::AURAS];
        assert_eq!(pieces(&model, &cs, &auras, false), (Vec::new(), Vec::new()), "no aura: nothing");
        auras[0] = piece(0x3060_FFFF, 0, 0);
        auras[1] = piece(0xFF20_20FF, net::npctint::Aura::BOTH, 3);
        let (shells, worn) = pieces(&model, &cs, &auras, false);
        assert_eq!(shells.iter().map(|s| s.color).collect::<Vec<_>>(), vec![[48, 96, 255], [255, 32, 32]]);
        let (chest, bracer) = (shells[0].batches.clone().unwrap(), shells[1].batches.clone().unwrap());
        assert_eq!(chest, slot_batches(&model, &cs, 0));
        assert_eq!(bracer, slot_batches(&model, &cs, 1));
        assert!(!chest.is_empty() && !bracer.is_empty() && chest.iter().all(|b| !bracer.contains(b)), "{chest:?} {bracer:?}");
        assert_eq!(worn.len(), 1, "the bracers' particles alone");
        let on = batch_nodes(&model, &bracer);
        assert_eq!(worn[0].nodes.len(), 3.min(on.len()));
        assert!(worn[0].nodes.iter().all(|n| on.contains(n)), "on the bracers' nodes {on:?}: {:?}", worn[0].nodes);
        assert_eq!((worn[0].particles.hue, worn[0].first), (Some([255, 32, 32]), 128));
        // a robe's aura in the chest's and the legs' place
        auras[ROBE] = piece(0x20FF_20FF, net::npctint::Aura::SHELL, 0);
        let (robed, _) = pieces(&model, &cs, &auras, true);
        assert_eq!(robed.iter().map(|s| s.color).collect::<Vec<_>>(), vec![[32, 255, 32], [255, 32, 32], [32, 255, 32]]);
        assert_eq!(robed[2].batches, Some(slot_batches(&model, &cs, 3)));
        // the helm's
        auras[HELM] = piece(0xFFFF_00FF, net::npctint::Aura::BOTH, 0);
        let (shell, worn) = helm_aura(&auras, 5);
        assert_eq!(shell.map(|s| (s.color, s.dark)), Some(([255, 255, 0], false)));
        assert_eq!(worn.map(|w| (w.nodes, w.first)), Some((vec![5], 64 * 6)));
        assert_eq!(helm_aura(&[PieceAura::default(); net::entity::AURAS], 5), (None, None));
        // the body's: over what no armour covers; a piece worn covers it
        let mut bare = [PieceAura::default(); net::entity::AURAS];
        bare[BODY] = piece(0x0000_00FF, net::npctint::Aura::SHELL, 0);
        let all = [false; 5];
        let (whole, _) = body_aura(&model, &cs, &bare, all);
        let whole = whole.expect("a body aura");
        assert!(whole.dark && whole.color == [0, 0, 0], "black: dark");
        assert_eq!(whole.batches.as_ref().map(Vec::len), Some(model.batches.len()), "nothing covered: every batch");
        let chest_on = [true, false, false, false, false];
        let (under, _) = body_aura(&model, &cs, &bare, chest_on);
        let under = under.unwrap().batches.unwrap();
        let chest = slot_batches(&model, &cs, 0);
        assert!(!chest.is_empty() && chest.iter().all(|b| !under.contains(b)), "the chest piece covers its part");
        assert_eq!(under.len(), model.batches.len() - chest.len());
        assert_eq!(covered(&look), [true; 5], "set 4 on every slot covers all five");
        assert_eq!(covered(&vi::cust::Look { robe: 1, ..Default::default() }), [true, false, false, true, false], "a robe: chest and legs");
        assert_eq!(body_aura(&model, &cs, &[PieceAura::default(); net::entity::AURAS], all), (None, None), "no body aura: nothing");
        bare[BODY] = piece(0xFF20_20FF, net::npctint::Aura::TRIM, 0);
        assert_eq!(body_aura(&model, &cs, &bare, all), (None, None), "a trim on the body: nothing");
    }

    /// The helm's and the cape's auras, which are drawn on models of their
    /// own (attached, and fitted to the body), not the body's: a trim aura on
    /// the cloth helm (set 1's, whose cream bands are trim) lights its trim,
    /// on the plate helm (set 4's plain steel) nothing, and a shell kind no
    /// trim (the owner, 2026-11-14: "oof's helm isnt showing aura for trim").
    /// The cape's (the silver knight's, style 1): a shell over it whole; a
    /// trim, its art having none, the shell instead; particles on the
    /// wearer's nodes it is skinned to, keyed apart; none, nothing (the
    /// owner, 2026-11-14: "also cannot apply aura to capes yet"). Needs
    /// `run/eqoa.iso`; without it, SKIPs.
    #[test]
    fn the_helm_and_the_cape_their_own_auras() {
        use net::entity::PieceAura;
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", esf::Container::open(disc.read("CHAR.ESF").unwrap()).unwrap());
        lib.add("CHARCUST.CSF", esf::Container::open(disc.read("CHARCUST.CSF").unwrap()).unwrap());
        let human = 0x70D8_98C6u32;
        let r = lib.lookup(human as i32).expect("the human man");
        let mut anims = vi::hsprite::AnimScene::<Ee>::default();
        let cs = vi::character::load_csprite(&lib, r, &mut anims, &mut vi::character::AnimCache::new(), &mut vi::csprite::Rand(1)).unwrap();
        let t = vi::cust::Tables::retail();
        let taken = |id: i32| lib.lookup(id).is_some();
        let piece = |c: u32, kind: u8| PieceAura { color: c.to_be_bytes(), kind, source: 0, nodes: 0 };
        let mut auras = [PieceAura::default(); net::entity::AURAS];
        let helm = |set: u8| crate::slotless::helm_on(&lib, human, &cs, true, set, [128, 128, 128, 255], &t, true).expect("a helm").1;
        let (cloth, plate) = (helm(1), helm(4));
        assert_eq!(helm_trim(&cloth, &auras, &taken), None, "no aura: nothing");
        auras[HELM] = piece(0xFF20_20FF, net::npctint::Aura::TRIM);
        let lit = helm_trim(&cloth, &auras, &taken).expect("the cloth helm's trim lit");
        assert_eq!((lit.color, lit.dark), ([255, 32, 32], false));
        assert!(!lit.batches.is_empty());
        assert_eq!(helm_trim(&plate, &auras, &taken), None, "the plate helm: no trim");
        assert_eq!(helm_aura(&auras, 5).0, None, "a trim alone: no shell");
        auras[HELM] = piece(0xFF20_20FF, net::npctint::Aura::SHELL);
        assert_eq!(helm_trim(&cloth, &auras, &taken), None, "a shell: no trim");
        // the cape
        let cape = crate::cape::model(&lib, net::cape::Cape { style: 1, color: 0 }, human, &cs).expect("the silver knight's cape on a human");
        assert_eq!(cape_aura(&cape, &cs, &auras, &taken), (None, None, None), "no cape aura: nothing");
        auras[CAPE] = piece(0x3060_FFFF, net::npctint::Aura::SHELL);
        let (shell, worn, trim) = cape_aura(&cape, &cs, &auras, &taken);
        assert_eq!(shell.map(|s| (s.color, s.batches, s.dark)), Some(([48, 96, 255], None, false)));
        assert_eq!((worn, trim), (None, None));
        auras[CAPE] = piece(0xFF20_20FF, net::npctint::Aura::TRIM);
        let (shell, _, trim) = cape_aura(&cape, &cs, &auras, &taken);
        assert_eq!((shell.map(|s| s.color), trim), (Some([255, 32, 32]), None), "a trim on a cape with none: its shell");
        auras[CAPE] = piece(0x3060_FFFF, net::npctint::Aura::PARTICLES);
        let (shell, worn, _) = cape_aura(&cape, &cs, &auras, &taken);
        let worn = worn.expect("the cape's particles");
        assert_eq!((shell, worn.first, worn.nodes.len()), (None, 64 * (CAPE + 1), AURA_NODES.min(worn.nodes.len())));
        let all: Vec<usize> = (0..cape.batches.len()).collect();
        let under = batch_nodes(&cape, &all);
        assert!(!worn.nodes.is_empty() && worn.nodes.iter().all(|n| under.contains(n)), "on the nodes the cape hangs from {under:?}: {:?}", worn.nodes);
    }

    /// The particles template on Mr. Moogie's body (0x485AD952, the lion-like
    /// cat) in a red glow: the sword's own emitter (one particle sprite of the Sword of Virtue,
    /// 4.1 a second) on eight of his 39 nodes spread from his back to his
    /// paws, jaw and tail; its particles red, rising from the nodes, about
    /// 25 an emitter and never more than 30; gone with the creature. Needs
    /// `run/eqoa.iso`; without it, SKIPs.
    #[test]
    fn mr_moogies_aura() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", esf::Container::open(disc.read("CHAR.ESF").unwrap()).unwrap());
        lib.add("ITEM.ESF", esf::Container::open(disc.read("ITEM.ESF").unwrap()).unwrap());
        let sword = crate::particles::item_emitters(&lib, SWORD_OF_VIRTUE);
        assert_eq!(sword.len(), 1, "the sword's one particle sprite");
        let r = lib.lookup(0x485A_D952u32 as i32).expect("Mr. Moogie's body");
        let mut anims = vi::hsprite::AnimScene::<Ee>::default();
        let mut rand = vi::csprite::Rand(1);
        let mut cs = vi::character::load_csprite(&lib, r, &mut anims, &mut vi::character::AnimCache::new(), &mut rand).unwrap();
        cs.set_locomotion(0, &anims, &mut rand);
        cs.h.process(0, &anims);
        let nodes = body_nodes(&cs, AURA_NODES);
        assert_eq!(cs.h.nodes.len(), 39);
        assert_eq!(nodes.len(), AURA_NODES);
        eprintln!("aura nodes {nodes:?}");
        let mut sorted = nodes.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), AURA_NODES, "eight different nodes");
        let centre = Matrix44::transform(&vi::vect::Vect3::of(0.0, 0.0, 0.0), F::new(2.12), &vi::vect::Vect3::of(100.0, 0.0, 200.0));
        let red = [255, 0, 0];
        assert_eq!(source_emitter(&lib, SWORD_OF_VIRTUE), Some(sword[0].def));
        let now = body_emitters(9, &cs, &nodes, &centre, SWORD_OF_VIRTUE, sword[0].def, Some(red), true);
        assert_eq!(now.len(), AURA_NODES);
        let mut held = crate::particles::Held::new(0);
        let eye = vi::vect::Vect3::of(100.0, 3.0, 190.0);
        let mut quads = Vec::new();
        let mut t = 0;
        while t < 8000 {
            t += 33;
            quads = held.frame(&lib, &now, &eye, t).into_iter().flat_map(|b| b.quads).collect::<Vec<_>>();
            assert!(quads.len() <= AURA_NODES * 30, "{} particles at {t}", quads.len());
        }
        assert!(quads.len() > AURA_NODES * 15, "{} particles", quads.len());
        eprintln!("{}", held.describe());
        assert!(held.describe().contains(&format!("{AURA_NODES} aura")));
        assert!(quads.iter().all(|q| q.color[1] <= q.color[0] * 0.1 && q.color[2] <= q.color[0] * 0.1), "red");
        let base = now.iter().map(|h| h.world.m[3].y.0).fold(f32::MAX, f32::min);
        assert!(quads.iter().all(|q| q.center[1] >= base - 0.01 && q.center[1] < base + 6.0), "rising from the nodes");
        assert!(quads.iter().all(|q| (q.center[0] - 100.0).abs() < 6.0 && (q.center[2] - 200.0).abs() < 6.0), "about the body");
        // the control: the creature gone (or never glowing), nothing
        assert!(held.frame(&lib, &[], &eye, t + 33).is_empty());
        assert_eq!(held.describe(), "0 held-item emitters, 0 creatures' own, 0 particles");
    }

    /// A hand's aura (the channel's tenth and eleventh): epic or particles
    /// alone, the main hand's on hand 0, the off hand's on a weapon or a
    /// shield there; its source 0 the Sword of Virtue; its hue only a colour
    /// that lights. The controls: a shell or trim on a hand, nothing.
    #[test]
    fn a_hands_aura_off_the_channel() {
        use net::entity::PieceAura;
        use net::npctint::Aura;
        let mut auras = [PieceAura::default(); net::entity::AURAS];
        auras[MAIN_HAND] = PieceAura { color: [0; 4], kind: Aura::EPIC, source: SICKLE_OF_AVHI_ESCRON, nodes: 0 };
        auras[OFF_HAND] = PieceAura { color: [0xFF, 0x20, 0x20, 0xFF], kind: Aura::PARTICLES, source: 0, nodes: 0 };
        assert_eq!(hand_aura(&auras, 0), Some(auras[MAIN_HAND]));
        assert_eq!((hand_aura(&auras, 1), hand_aura(&auras, 2)), (Some(auras[OFF_HAND]), Some(auras[OFF_HAND])), "a shield takes the off hand's");
        assert_eq!((hand_source(&auras[MAIN_HAND]), hand_source(&auras[OFF_HAND])), (SICKLE_OF_AVHI_ESCRON, SWORD_OF_VIRTUE));
        assert_eq!((hand_hue(&auras[MAIN_HAND]), hand_hue(&auras[OFF_HAND])), (None, Some([0xFF, 0x20, 0x20])));
        for kind in [Aura::DEFAULT, Aura::NONE, Aura::SHELL, Aura::BOTH, Aura::TRIM, Aura::TRIM_SHELL, 8] {
            auras[MAIN_HAND].kind = kind;
            assert_eq!(hand_aura(&auras, 0), None, "kind {kind} on a hand");
        }
        assert_eq!(hand_aura(&[PieceAura::default(); net::entity::AURAS], 1), None, "none sent");
    }

    /// `borrow_epic` on models like the disc's: the epic's brightest
    /// emissive on every layer, its additive sheen laid over each batch on
    /// the item's own texture, in a hue when named; an epic with neither
    /// lends nothing; the epic itself keeps its look (recoloured only).
    #[test]
    fn an_epics_look_lent() {
        let layer = |blend: i32, texture: i32, color: [u8; 4], emissive: [u8; 4]| esf::Layer { flags: 1, texture, wrap: 0, blend, color, uv_transform: [0.0; 9], lod_bias: 0.8, uv_rate: [0.0; 2], emissive };
        let model = |batches: Vec<Vec<esf::Layer>>| esf::Model { batches: batches.into_iter().map(|layers| esf::Batch { material: 0, layers, indices: vec![], lod: None }).collect(), ..Default::default() };
        let dagger = model(vec![vec![layer(0, 11, [128; 4], [90, 90, 90, 255])], vec![layer(0, 12, [255; 4], [90, 90, 90, 255])]]);
        let sickle = model(vec![vec![layer(0, 1, [229, 204, 178, 255], [60, 90, 0, 255]), layer(3, 2, [255; 4], [60, 90, 0, 255])]]);
        let mut m = dagger.clone();
        borrow_epic(&mut m, &sickle, None, false);
        for (b, was) in m.batches.iter().zip(&dagger.batches) {
            assert_eq!(b.layers.len(), 1, "no sheen: no layer added (the sickle's rust is alpha, not a glow)");
            assert_eq!((b.layers[0].emissive, b.layers[0].texture, b.layers[0].color), ([60, 90, 0, 255], was.layers[0].texture, was.layers[0].color));
        }
        let virtue = model(vec![vec![layer(0, 1, [255; 4], [204, 255, 255, 255]), layer(1, 1, [204, 255, 255, 153], [204, 255, 255, 255])], vec![layer(0, 3, [255; 4], [100, 100, 100, 255])]]);
        let mut m = dagger.clone();
        borrow_epic(&mut m, &virtue, Some(CRIMSON), false);
        for (b, was) in m.batches.iter().zip(&dagger.batches) {
            assert_eq!(b.layers.len(), 2, "the sheen laid over");
            assert_eq!(b.layers[0].emissive, [248, 23, 59, 255], "the brightest emissive, in the hue");
            assert_eq!((b.layers[1].blend, b.layers[1].texture, b.layers[1].color), (1, was.layers[0].texture, [248, 23, 59, 153]), "on its own texture");
        }
        // the controls: an epic of no glow lends nothing; the epic itself in hand
        let plain = model(vec![vec![layer(0, 5, [255; 4], [0, 0, 0, 255])]]);
        let mut m = dagger.clone();
        borrow_epic(&mut m, &plain, Some(CRIMSON), false);
        let layers = |m: &esf::Model| m.batches.iter().map(|b| b.layers.clone()).collect::<Vec<_>>();
        assert_eq!(layers(&m), layers(&dagger));
        let mut m = virtue.clone();
        borrow_epic(&mut m, &virtue, None, true);
        assert_eq!(layers(&m), layers(&virtue), "the epic itself: its own look");
        borrow_epic(&mut m, &virtue, Some(CRIMSON), true);
        assert_eq!(m.batches[0].layers.len(), 2, "recoloured, no sheen added");
        assert_eq!(m.batches[0].layers[1].color, [248, 23, 59, 153]);
    }

    /// Off the disc: the Sickle of Avhi Escron's taint on the Heirloom Dagger
    /// of Bloodletting's model (Rogue_Dirk, 0xBD3C3DC8): every material's
    /// olive emissive (60, 90, 0), no layer added; its one particle sprite
    /// put on the held dagger under the sickle's member transform, in its own
    /// colours, keyed apart from the dagger's own. The controls: the dagger
    /// plain has a grey emissive and no particles; the sickle in hand borrows
    /// no second emitter. Needs `run/eqoa.iso`; without it, SKIPs.
    #[test]
    fn the_sickles_taint_on_a_dagger() {
        use net::entity::PieceAura;
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("ITEM.ESF", esf::Container::open(disc.read("ITEM.ESF").unwrap()).unwrap());
        const DAGGER: u32 = 0xBD3C_3DC8;
        let build = |id: u32| lib.lookup(id as i32).and_then(|r| lib.build_model(r)).unwrap();
        let (dagger, sickle) = (build(DAGGER), build(SICKLE_OF_AVHI_ESCRON));
        assert!(sickle.batches.iter().flat_map(|b| &b.layers).all(|l| l.emissive == [60, 90, 0, 255]), "the sickle's five materials");
        assert!(dagger.batches.iter().flat_map(|b| &b.layers).all(|l| l.emissive == [90, 90, 90, 255]), "the control: the dagger's own grey");
        let mut m = dagger.clone();
        borrow_epic(&mut m, &sickle, None, false);
        assert_eq!(m.batches.len(), dagger.batches.len());
        for (b, was) in m.batches.iter().zip(&dagger.batches) {
            assert_eq!(b.layers.len(), was.layers.len());
            assert!(b.layers.iter().all(|l| l.emissive == [60, 90, 0, 255]));
        }
        let a = PieceAura { color: [0; 4], kind: net::npctint::Aura::EPIC, source: SICKLE_OF_AVHI_ESCRON, nodes: 0 };
        let at = Matrix44::<Ee>::identity();
        assert!(crate::particles::item_emitters(&lib, DAGGER).is_empty(), "the control: the dagger has none of its own");
        let own = crate::particles::item_emitters(&lib, SICKLE_OF_AVHI_ESCRON);
        assert_eq!(own.len(), 1, "the sickle's one green mist");
        let lent = hand_emitters(&lib, 3, 1, DAGGER, &at, &a, true);
        assert_eq!(lent.len(), 1);
        assert_eq!(lent[0].key, (3, 1, SICKLE_OF_AVHI_ESCRON, HAND_AURA_FIRST));
        assert_eq!((lent[0].def, lent[0].tint), (own[0].def, None));
        // where the sickle carries it, at the same share of the dagger's reach
        let k = reach(&lib, DAGGER).unwrap() / reach(&lib, SICKLE_OF_AVHI_ESCRON).unwrap();
        let (w, o) = (lent[0].world.m[3], own[0].member.m[3]);
        assert!([(w.x.0, o.x.0), (w.y.0, o.y.0), (w.z.0, o.z.0)].iter().all(|(w, o)| (w - o * k).abs() < 1e-5), "{:?} vs {:?} x {k}", (w.x.0, w.y.0, w.z.0), (o.x.0, o.y.0, o.z.0));
        assert!(hand_emitters(&lib, 3, 0, SICKLE_OF_AVHI_ESCRON, &at, &a, true).is_empty(), "the sickle itself: its own alone");
        let red = PieceAura { color: [0xFF, 0x20, 0x20, 0xFF], ..a };
        assert_eq!(hand_emitters(&lib, 3, 1, DAGGER, &at, &red, true)[0].tint, Some([0xFF, 0x20, 0x20]));
    }

    /// The druid staff glow (the Oak Staff of Eilerawdl, 0xCBF4135A) on
    /// Zerkziee's weapons: its sparkles 1.25 up the 1.61 staff go at that
    /// share of each held item's reach, inside the Triad Scimitar's (0.999)
    /// and the ranger's axe's (0.625) tips, not past them; its amber
    /// emissive (120, 80, 30) and gold sheen (255, 204, 0) lent. The
    /// control: an item without a trail keeps the staff's place.
    #[test]
    fn the_druid_staff_glow_on_a_scimitar_and_an_axe() {
        use net::entity::PieceAura;
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("ITEM.ESF", esf::Container::open(disc.read("ITEM.ESF").unwrap()).unwrap());
        const STAFF: u32 = 0xCBF4_135A;
        let a = PieceAura { color: [0; 4], kind: net::npctint::Aura::EPIC, source: STAFF, nodes: 0 };
        let at = Matrix44::<Ee>::identity();
        let staff = reach(&lib, STAFF).unwrap();
        assert!((staff - 1.606).abs() < 0.01, "{staff}");
        for (item, tip) in [(0xD039_42DEu32, 0.999f32), (0x1166_6A69, 0.6806)] {
            let r = reach(&lib, item).unwrap();
            assert!((r - tip).abs() < 0.01, "{item:#X}: {r}");
            let lent = hand_emitters(&lib, 1, 0, item, &at, &a, true);
            assert_eq!(lent.len(), 1, "the staff's one sparkle sprite");
            let t = lent[0].world.m[3];
            let d = (t.x.0 * t.x.0 + t.y.0 * t.y.0 + t.z.0 * t.z.0).sqrt();
            assert!(d < r && (d - r * 1.25 / staff).abs() < 0.01, "{item:#X}: at {d}, reach {r}");
        }
        assert_eq!(reach(&lib, 0x1234_5678), None, "the control: no such item, no reach");
        let build = |id: u32| lib.lookup(id as i32).and_then(|r| lib.build_model(r)).unwrap();
        let mut m = build(0xD039_42DE);
        borrow_epic(&mut m, &build(STAFF), None, false);
        assert!(m.batches.iter().all(|b| b.layers[0].emissive == [120, 80, 30, 255] && b.layers.last().unwrap().color == [255, 204, 0, 255]));
    }
}
