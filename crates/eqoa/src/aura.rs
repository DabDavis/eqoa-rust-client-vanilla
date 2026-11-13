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
//! A glowing NPC's aura (the owner, 2026-11-13: "mr. moogie aura should have
//! particle effects like the effect of dust's sword of twisted virtue"): an
//! NPC whose body tint glows (`npctint`, layout 2) carries the Sword of
//! Virtue's own particle emitter ([`SWORD_OF_VIRTUE`]'s one particle sprite,
//! its definition as the disc has it) on [`AURA_NODES`] of its body's nodes
//! spread over it ([`body_nodes`]), each streaming up from its node in the
//! world's up and following it, its particles in the glow's hue as the
//! sword's are in its item's colour ([`recolor_quads`]). No glow: none.

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

/// How many of a glowing body's nodes carry the aura: the sword's emitter
/// keeps about 25 particles alive (4.1 a second, 6 s each), so a body's
/// aura is about 200.
pub const AURA_NODES: usize = 8;

/// The colour of an NPC's aura: its tint's glow's ([`crate::npctint::glow_rgb`]),
/// none for a tint that does not glow and none without a tint.
pub fn npc_glow(t: Option<crate::npctint::Tint>) -> Option<[u8; 3]> {
    t.filter(|t| net::npctint::glows(t.glow)).map(|t| crate::npctint::glow_rgb(t.glow))
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

/// The nodes of a body that carry its aura: [`AURA_NODES`] spread over its
/// bind pose (each node's model-space place, `SetNodeBindPose`).
pub fn body_nodes(cs: &vi::csprite::CSprite<Ee>) -> Vec<i32> {
    let at: Vec<[f32; 3]> = cs.h.nodes.iter().map(|n| [n.bind.trans.x.0, n.bind.trans.y.0, n.bind.trans.z.0]).collect();
    spread(&at, AURA_NODES).into_iter().map(|i| i as i32).collect()
}

/// An aura emitter's world transform at a node's (`w`, its world transform):
/// at the node's place, unscaled, its +z the world's up (+y), so the sword's
/// stream rises from the node whichever way the body turns or bends.
pub fn upright(w: &Matrix44<Ee>) -> Matrix44<Ee> {
    let (o, i) = (F::ZERO, F::ONE);
    Matrix44 { m: [Vect4::new(i, o, o, o), Vect4::new(o, o, -i, o), Vect4::new(o, i, o, o), Vect4::new(w.m[3].x, w.m[3].y, w.m[3].z, i)] }
}

/// A glowing NPC's aura this frame: the sword's emitter (`sword`, its
/// definition record) on each of its `nodes` (`body_nodes`) of sprite `cs`,
/// under the centre transform `centre`, in `glow`'s hue, run while the body
/// is drawn. Keyed by the holder, `particles::AURA` and the node's place in
/// the list.
pub fn body_emitters(holder: i32, cs: &vi::csprite::CSprite<Ee>, nodes: &[i32], centre: &Matrix44<Ee>, sword: esf::Ref, glow: [u8; 3], drawn: bool) -> Vec<crate::particles::InHand> {
    use crate::particles::{AURA, How, InHand};
    nodes
        .iter()
        .enumerate()
        .filter_map(|(k, &node)| {
            let n = cs.h.nodes.get(usize::try_from(node).ok()?)?;
            let world = upright(&Matrix44::mul(&n.world, centre));
            Some(InHand { key: (holder, AURA, SWORD_OF_VIRTUE, k), world, def: sword, how: How::Part { use_location: false, once: false }, drawn, tint: Some(glow) })
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
        assert_eq!(npc_glow(Some(Tint { color: 0xFF60_60FF, glow: 0xFF00_00FF })), Some([255, 0, 0]));
        assert_eq!(npc_glow(Some(Tint { color: 0xFFFF_FFFF, glow: 0xFF40_4080 })), Some([128, 32, 32]));
        assert_eq!(npc_glow(Some(Tint::color(0x558C_50FF))), None);
        assert_eq!(npc_glow(Some(Tint { color: 0xFFFF_FFFF, glow: 0x0000_00FF })), None);
        assert_eq!(npc_glow(Some(Tint { color: 0xFFFF_FFFF, glow: 0xFF00_0000 })), None);
        assert_eq!(npc_glow(None), None);
        let w = Matrix44::<Ee>::transform(&vi::vect::Vect3::of(0.3, 1.0, 0.2), F::new(2.0), &vi::vect::Vect3::of(5.0, 6.0, 7.0));
        let u = upright(&w);
        let up = u.mul_point(&vi::vect::Vect3::of(0.0, 0.0, 1.0));
        assert_eq!((up.x.0, up.y.0, up.z.0), (5.0, 7.0, 7.0), "+z is the world's up, unscaled, at the node");
    }

    /// Mr. Moogie's body (0x485AD952, the lion-like cat) in his glow: the
    /// sword's own emitter (one particle sprite of the Sword of Virtue,
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
        let nodes = body_nodes(&cs);
        assert_eq!(cs.h.nodes.len(), 39);
        assert_eq!(nodes.len(), AURA_NODES);
        eprintln!("aura nodes {nodes:?}");
        let mut sorted = nodes.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), AURA_NODES, "eight different nodes");
        let centre = Matrix44::transform(&vi::vect::Vect3::of(0.0, 0.0, 0.0), F::new(2.12), &vi::vect::Vect3::of(100.0, 0.0, 200.0));
        let red = [255, 0, 0];
        let now = body_emitters(9, &cs, &nodes, &centre, sword[0].def, red, true);
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
}
