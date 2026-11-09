//! Capes: a wear slot of go-eqoa-server's own, on a server that turns it on
//! (the Heirloom Test Server; its owner, 2026-11-08: "lets add a new equip slot
//! for heirloom server only. capes for all. there are many capes we can borrow
//! from for various styles."). The game has no cape slot and no cape art, so a
//! PS2 is sent a cape as a plain item and never sees one worn; this client
//! answers the server's offer (`net::session::OP_CAPES`) and draws them.
//!
//! The art is borrowed from the capes the disc's own characters wear
//! ([`STYLES`]): a cape is the triangles of its body's cape materials, cut out
//! (`Source`), and put on a player's character skinned to its skeleton so it
//! moves with the body:
//!
//! - On the skeleton the cape was made on, as it is. Every player race's body
//!   is built on one of two skeletons, each the same bone for bone in every
//!   body that has it (forty nodes: the human, dark elf, gnome, halfling and
//!   dwarf of either sex; forty-three: the elf, troll, barbarian, erudite,
//!   ogre's), and the capes' bodies are built on them too (the knight's, the
//!   vampire's and the Dawn Maiden's on the forty, Dread's on the forty-three).
//!   Their capes hang from the shoulders, the spine and the two back flap
//!   bones every player skeleton has for a robe's tail, which the race's own
//!   walk and run swing: a cape sways as a robe's tail does.
//! - On the other player skeleton, each bone on the one of the same role
//!   (`ON_40`, `ON_43`), each vertex kept where it was about its bones, put
//!   about theirs on the wearer (every skeleton on the disc binds
//!   world-aligned, so a bone's bind is a place: `transfer`).
//! - On any other body that stands on two legs (a server's own races'
//!   bodies), likewise by its parts (`Frame::roles`): its spine by height, its
//!   head, its arms from the hands up and its legs from the thighs down; a
//!   bone with no counterpart (a back flap) on its nearest parent's, so a cape
//!   whose corners ride the arms (the vampire's) still folds with them.
//!   Bodies with wings wear none (`NO_CAPE`).
//!
//! On a body broader than the cape's own the cape goes wider and further
//! back, so the body does not show through it; on a body not a player's it is
//! sized to it, up and across (`spread`).
//!
//! A cape fitted is in its wearer's bind pose and names its wearer's nodes, so
//! it is drawn with the body's own skin palette. With none worn nothing is
//! made and nothing more is drawn: the frame is the game's.

use esf::{Era, Library};
use glam::Vec3;
use std::collections::HashMap;
use std::rc::Rc;
use vi::character::{AnimCache, load_csprite};
use vi::csprite::{CSprite, Rand};
use vi::fpu::Ee;
use vi::hsprite::AnimScene;

pub use net::cape::{Cape, EQUIP_SLOT, WEAR_SLOT};

/// A cape's art: the body (CHAR.ESF DictID) whose cape it is and the palette
/// entries the cape is drawn in on it.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// What the server's items name it by (`itempattern.model`).
    pub id: u8,
    pub body: u32,
    pub entries: &'static [i32],
    /// What it is, for the log and the shots.
    pub what: &'static str,
}

/// The capes this client draws, by the style the server names. Every one is
/// a cape of its own material on its body (its triangles and none of the
/// body's): the vampire's is two (its outside and its lining, the same
/// triangles turned); Dread's is worn alone or with the fur over its
/// shoulders (a golem's, as wide on anyone).
pub const STYLES: &[Style] = &[
    Style { id: 1, body: 0xD130_0BD3, entries: &[6], what: "the silver knight's long blue cape" },
    Style { id: 2, body: 0xCE1B_8083, entries: &[7, 9], what: "Magnus Jaeger's high-collared black cape" },
    Style { id: 3, body: 0xD754_0FDA, entries: &[7], what: "the Dawn Maiden's pleated violet cape" },
    Style { id: 4, body: 0xDC3B_3450, entries: &[3], what: "Dread's maroon cape" },
    Style { id: 5, body: 0xDC3B_3450, entries: &[3, 1], what: "Dread's maroon cape and its fur mantle" },
];

/// The style of this id, if this client has its art.
pub fn style(id: u8) -> Option<&'static Style> {
    STYLES.iter().find(|s| s.id == id)
}

/// The bodies of a server's races that wear no cape: wings where it would
/// hang (the pixie's and the fairy's, 0x94BA77CD and 0x7A85646C, and the
/// gargoyle's, 0xB4D2D132), which it would cut through.
pub const NO_CAPE: &[u32] = &[0x94BA_77CD, 0x7A85_646C, 0xB4D2_D132];

#[cfg(not(test))]
static OFFERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(test)]
thread_local! {
    /// A test's own, so tests running at once never see each other's.
    static OFFERED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the server offered capes and this client took them (the session's
/// `capes`): a cape item then goes in its slot. One session a program.
pub fn offered() -> bool {
    #[cfg(not(test))]
    return OFFERED.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    return OFFERED.with(|o| o.get());
}

/// Set from the session as it stands (`Play`, each frame).
pub fn set_offered(on: bool) {
    #[cfg(not(test))]
    OFFERED.store(on, std::sync::atomic::Ordering::Relaxed);
    #[cfg(test)]
    OFFERED.with(|o| o.set(on));
}

/// `f` run with capes offered or not on this thread (a test's), put back after.
#[cfg(test)]
pub fn with_offered<T>(on: bool, f: impl FnOnce() -> T) -> T {
    let old = offered();
    set_offered(on);
    let out = f();
    set_offered(old);
    out
}

/// The word an item's description names the cape's slot by (WORDS.TXT has
/// none: no retail slot is a cape's).
pub const SLOT_WORD: &str = "Cape";

/// The forty-node player skeleton's parents (the human male's, `HumanMale`).
const P40_PARENTS: [i32; 40] = [-1, 0, 1, 2, 3, 4, 4, 6, 4, 8, 2, 2, 11, 12, 13, 14, 13, 16, 13, 12, 2, 20, 21, 22, 23, 22, 25, 22, 21, 0, 29, 30, 31, 0, 33, 34, 35, 0, 0, 38];
/// The forty-three-node player skeleton's parents (the elf's, Dread's).
const P43_PARENTS: [i32; 43] = [-1, 0, 1, 2, 3, 4, 5, 5, 7, 5, 3, 10, 11, 12, 13, 14, 13, 16, 13, 12, 3, 3, 21, 22, 23, 24, 25, 24, 27, 24, 23, 1, 0, 32, 33, 34, 0, 36, 37, 38, 0, 0, 41];

/// Each of the forty-three's nodes on the forty's of the same role: the
/// pelvis, the spine (both of its lower bones on the forty's one), the chest,
/// neck and head; the collarbones and upper arms on the forty's upper arms
/// (its arms hang from the spine), the arms, hands and fingers; the chest's
/// and belly's front on the forty's chest front and front flap; the legs; the
/// back flap's two bones on the forty's two.
const ON_40: [u8; 43] = [
    0, 1, 1, 2, 3, 4, 5, 6, 7, 8, //
    11, 11, 12, 13, 14, 15, 16, 17, 18, 19, //
    10, //
    20, 20, 21, 22, 23, 24, 25, 26, 27, 28, //
    37, //
    29, 30, 31, 32, //
    33, 34, 35, 36, //
    37, 38, 39,
];

/// Each of the forty's nodes on the forty-three's of the same role: as
/// `ON_40` the other way, the upper arms on the collarbones (a cape's corner
/// skinned to the shoulder stays on it as the arm swings), the spine on the
/// forty-three's upper spine bone.
const ON_43: [u8; 40] = [
    0, 2, 3, 4, 5, 6, 7, 8, 9, 9, //
    20, 10, 12, 13, 14, 15, 16, 17, 18, 19, //
    21, 23, 24, 25, 26, 27, 28, 29, 30, //
    32, 33, 34, 35, //
    36, 37, 38, 39, //
    40, 41, 42,
];

/// A skeleton's layout when it is one of the two player skeletons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layout {
    P40,
    P43,
}

fn layout_of(parents: &[i32]) -> Option<Layout> {
    if parents == P40_PARENTS {
        Some(Layout::P40)
    } else if parents == P43_PARENTS {
        Some(Layout::P43)
    } else {
        None
    }
}

/// A skeleton's nodes' parents and bind places (model space: every rig on
/// the disc binds world-aligned), and its NodeIDList.
#[derive(Clone, Debug)]
struct Bones {
    parents: Vec<i32>,
    bind: Vec<Vec3>,
    ids: [i32; 10],
}

impl Bones {
    fn of(cs: &CSprite<Ee>) -> Bones {
        Bones {
            parents: cs.h.nodes.iter().map(|n| n.parent).collect(),
            bind: cs.h.nodes.iter().map(|n| Vec3::new(n.bind.trans.x.0, n.bind.trans.y.0, n.bind.trans.z.0)).collect(),
            ids: cs.node_ids,
        }
    }

    /// The same skeleton: the same parents and every bind within a millimetre.
    fn same(&self, o: &Bones) -> bool {
        self.parents == o.parents && self.bind.iter().zip(&o.bind).all(|(a, b)| a.distance(*b) < 1e-3)
    }
}

/// A style's cape cut out of its body: its batches alone, their vertices
/// gathered, skinned to the body's skeleton; with the body's skeleton and,
/// for fitting it to other bodies, the body's measures.
struct Source {
    model: esf::Model,
    bones: Bones,
    frame: Option<Frame>,
}

/// The triangles of `entries` out of `model`, every other batch and every
/// vertex they do not use let go.
fn cut(model: &esf::Model, entries: &[i32]) -> esf::Model {
    let mut out = esf::Model { skinned: model.skinned, ..Default::default() };
    let mut map: HashMap<u32, u32> = HashMap::new();
    for b in model.batches.iter().filter(|b| entries.contains(&b.material)) {
        let mut indices = Vec::with_capacity(b.indices.len());
        for &v in &b.indices {
            let k = *map.entry(v).or_insert_with(|| {
                let i = v as usize;
                out.pos.push(model.pos[i]);
                out.uv.push(model.uv.get(i).copied().unwrap_or_default());
                out.normal.push(model.normal.get(i).copied().unwrap_or([0.0, 1.0, 0.0]));
                out.color.push(model.color.get(i).copied().unwrap_or([1.0; 4]));
                out.bones.push(model.bones.get(i).copied().unwrap_or([255; 4]));
                out.weights.push(model.weights.get(i).copied().unwrap_or([0; 4]));
                out.pos.len() as u32 - 1
            });
            indices.push(k);
        }
        out.batches.push(esf::Batch { material: b.material, layers: b.layers.clone(), indices, lod: None });
    }
    out.parts = out.batches.len() as u32;
    out.triangles = out.batches.iter().map(|b| b.indices.len() as u32 / 3).sum();
    out
}

fn load_source(lib: &Library, s: &Style) -> Option<Source> {
    let r = lib.lookup(s.body as i32)?;
    let body = lib.build_model(r)?;
    let cs = load_csprite::<Ee>(lib, r, &mut AnimScene::default(), &mut AnimCache::new(), &mut Rand(1))?;
    let model = cut(&body, s.entries);
    if model.batches.is_empty() || !model.skinned {
        return None;
    }
    let bones = Bones::of(&cs);
    // measured without the cape, which hangs off its back
    let mut rest = body;
    rest.batches.retain(|b| !s.entries.contains(&b.material));
    let frame = Frame::of(&bones, &rest);
    Some(Source { model, bones, frame })
}

thread_local! {
    /// The styles' capes as cut, by style (`load_source`'s answer, kept).
    static SOURCES: std::cell::RefCell<HashMap<u8, Option<Rc<Source>>>> = Default::default();
    /// The capes fitted, by style, wearer and the art its skeleton is in.
    static FITTED: std::cell::RefCell<HashMap<(u8, u32, Era), Option<Rc<esf::Model>>>> = Default::default();
}

/// A style's cape as cut, in Frontiers' art (capes are the server's, not
/// the 2002 disc's), kept once its body is in the library.
fn source(lib: &Library, s: &Style) -> Option<Rc<Source>> {
    if let Some(Some(src)) = SOURCES.with(|c| c.borrow().get(&s.id).cloned()) {
        return Some(src);
    }
    let got = lib.with_view(Era::Frontiers, || load_source(lib, s)).map(Rc::new);
    // a body not read yet (the pregame's library before CHAR.ESF's bodies
    // are in) is asked for again; one that is there and has no cape is not
    if got.is_some() || lib.lookup(s.body as i32).is_some() {
        SOURCES.with(|c| c.borrow_mut().insert(s.id, got.clone()));
    }
    got
}

/// The cape bodies a list of capes needs (the pregame reads them out of
/// CHAR.ESF, `res::Res::want_bodies`).
pub fn bodies<'a>(capes: impl IntoIterator<Item = &'a Cape>) -> Vec<u32> {
    let mut out: Vec<u32> = capes.into_iter().filter_map(|c| style(c.style)).map(|s| s.body).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Whether the body `wearer` can wear a cape: not one with wings (`NO_CAPE`).
pub fn wears(wearer: u32) -> bool {
    !NO_CAPE.contains(&wearer)
}

/// The cape `cape` on the body `wearer` (CHAR.ESF's, or the pregame's), whose
/// sprite is `cs`, in `cape`'s colour: in the wearer's bind pose and naming
/// its nodes, so the body's skin palette draws it. None for style 0 or one
/// this client has no art for, a body that wears none or does not stand on
/// two legs, or a library without the style's body. Kept per style, wearer
/// and art (a body's mesh is built here for its measures only when a cape is
/// first fitted to it).
pub fn model(lib: &Library, cape: Cape, wearer: u32, cs: &CSprite<Ee>) -> Option<esf::Model> {
    let s = style(cape.style)?;
    if !wears(wearer) {
        return None;
    }
    let era = lib.view_for(wearer as i32);
    let key = (s.id, wearer, era);
    let fitted = match FITTED.with(|c| c.borrow().get(&key).cloned()) {
        Some(f) => f,
        None => {
            let src = source(lib, s)?;
            let body = lib.with_view(era, || lib.lookup(wearer as i32).and_then(|r| lib.build_model(r)))?;
            let f = fit(&src, &Bones::of(cs), &body).map(Rc::new);
            FITTED.with(|c| c.borrow_mut().insert(key, f.clone()));
            f
        }
    }?;
    let mut m = (*fitted).clone();
    tint(&mut m, cape.color);
    Some(m)
}

/// The cape coloured: every layer's colour times `color` (0xRRGGBBAA; its
/// alpha left out, so a cape is never made see-through), as a dye colours a
/// slot's layer. 0 leaves it as its art is.
fn tint(m: &mut esf::Model, color: u32) {
    if color == 0 {
        return;
    }
    let c = [(color >> 24) as u8, (color >> 16) as u8, (color >> 8) as u8];
    for b in &mut m.batches {
        for l in &mut b.layers {
            for k in 0..3 {
                l.color[k] = ((l.color[k] as u32 * c[k] as u32 + 127) / 255) as u8;
            }
        }
    }
}

/// `src` on the skeleton `to` (the wearer's, whose mesh is `body`): as it
/// is on the skeleton it was made on; on another, each of its bones on the
/// wearer's of the same role (`transfer`), the other player skeleton's by the
/// role tables, any other's by its parts (`Frame::roles`), and on a body
/// broader than the cape's own a little wider and further back (`spread`).
/// `EQOA_CAPE_DEBUG=1` says which way and by what measures.
fn fit(src: &Source, to: &Bones, body: &esf::Model) -> Option<esf::Model> {
    let debug = std::env::var_os("EQOA_CAPE_DEBUG").is_some();
    if src.bones.same(to) {
        if debug {
            eprintln!("cape: on its own skeleton");
        }
        return Some(src.model.clone());
    }
    let into = Frame::of(to, body);
    let from = src.frame.as_ref();
    let n = src.bones.parents.len();
    let tables: Option<Vec<usize>> = match (layout_of(&src.bones.parents), layout_of(&to.parents)) {
        (Some(Layout::P40), Some(Layout::P43)) => Some(ON_43.iter().map(|&j| j as usize).collect()),
        (Some(Layout::P43), Some(Layout::P40)) => Some(ON_40.iter().map(|&j| j as usize).collect()),
        // the same skeleton with a bind that differs somewhere (a head's top
        // a little higher): each bone on its own
        (Some(a), Some(b)) if a == b => Some((0..n).collect()),
        _ => None,
    };
    let (map, anchor, scale, dz) = match tables {
        Some(map) => {
            // a player skeleton: every bone has its counterpart
            let anchor: Vec<usize> = (0..n).collect();
            let (scale, dz) = match (from, &into) {
                (Some(f), Some(i)) => spread(f, i, &src.bones, to, &map, true),
                _ => (Vec3::ONE, 0.0),
            };
            (map, anchor, scale, dz)
        }
        None => {
            let (f, i) = (from?, into.as_ref()?);
            let (map, anchor) = Frame::roles(f, &src.bones, i, to);
            let (scale, dz) = spread(f, i, &src.bones, to, &map, false);
            (map, anchor, scale, dz)
        }
    };
    if debug {
        eprintln!("cape: onto {} nodes, scale {scale:?}, back {dz:+.3}; from {from:?}\n  into {into:?}", to.parents.len());
    }
    Some(transfer(src, to, &map, &anchor, scale, dz))
}

/// How much wider (and deeper, and on a body not a player's taller) a cape
/// goes on a body of frame `into` than on its own of frame `from`, and how
/// far further back: by the torsos' widths at the shoulders (on a player's
/// body only ever wider, so a cape stays as made on a slimmer one), the
/// shoulders' height over the ground, and the backs (the source's back line
/// as the transfer would carry it off the chest, against the wearer's; on a
/// player's body only ever back).
fn spread(from: &Frame, into: &Frame, sb: &Bones, tb: &Bones, map: &[usize], player: bool) -> (Vec3, f32) {
    let w = into.half_width / from.half_width;
    let h = (into.shoulder_y - into.ground) / (from.shoulder_y - from.ground);
    let (sx, sy) = if player { (w.clamp(1.0, 2.0), 1.0) } else { (w.clamp(0.4, 3.0), h.clamp(0.3, 3.0)) };
    let carried = sx * (from.back - sb.bind[from.chest].z) + tb.bind[map[from.chest]].z;
    let dz = into.back - carried;
    (Vec3::new(sx, sy, sx), if player { dz.min(0.0) } else { dz })
}

/// Each vertex of `src` on the wearer's nodes: of each of its bones, the
/// node `map` gives the bone's `anchor` (itself where it has a counterpart,
/// else the nearest of its parents that has), where it was about the anchor
/// put about the wearer's node, scaled by `s`; weighted as it was, then moved
/// back `dz`.
fn transfer(src: &Source, to: &Bones, map: &[usize], anchor: &[usize], s: Vec3, dz: f32) -> esf::Model {
    let mut m = src.model.clone();
    for v in 0..m.pos.len() {
        let (b, w) = (m.bones[v], m.weights[v]);
        let p = Vec3::from(m.pos[v]);
        let mut at = Vec3::ZERO;
        let mut sum = 0.0;
        let mut bones = [255u8; 4];
        for k in 0..4 {
            let i = b[k] as usize;
            if b[k] == 255 || w[k] == 0 || i >= anchor.len() {
                continue;
            }
            let (a, j) = (anchor[i], map[anchor[i]]);
            let (Some(from), Some(onto)) = (src.bones.bind.get(a), to.bind.get(j)) else { continue };
            at += (s * (p - *from) + *onto) * w[k] as f32;
            sum += w[k] as f32;
            bones[k] = j as u8;
        }
        if sum > 0.0 {
            m.pos[v] = (at / sum + Vec3::new(0.0, 0.0, dz)).to_array();
        }
        m.bones[v] = bones;
    }
    m
}

/// What a body is measured by for a cape: its skeleton's root (the pelvis),
/// chest (the spine's node the arms hang from), head and the spine between;
/// each arm's chain from the spine to the hand and each leg's from the thigh
/// to the foot; where its shoulders are and the ground; how wide its torso is
/// at the shoulders, where its back is, and its middle.
#[derive(Clone, Debug)]
struct Frame {
    root: usize,
    chest: usize,
    head: usize,
    /// From the chest down to the root.
    spine: Vec<usize>,
    /// From the spine (not it) down to the hand: a collarbone if it has one,
    /// the upper arm, the forearm, the hand.
    arms: [Vec<usize>; 2],
    /// From the thigh down to the footstep node.
    legs: [Vec<usize>; 2],
    shoulder_y: f32,
    ground: f32,
    half_width: f32,
    back: f32,
    mid_x: f32,
}

impl Frame {
    /// The frame of a skeleton and its mesh; None when the skeleton's
    /// NodeIDList does not name two hands, two feet and a head joined by a
    /// spine (not a body standing on two legs), or its torso has no skin.
    fn of(b: &Bones, body: &esf::Model) -> Option<Frame> {
        let n = b.parents.len();
        let node = |i: i32| usize::try_from(i).ok().filter(|&i| i < n);
        let up = |i: usize| node(b.parents[i]);
        let head = node(b.ids[7])?;
        // the spine: the head's parents up to the root
        let mut chain = Vec::new();
        let mut k = up(head)?;
        loop {
            chain.push(k);
            match up(k) {
                Some(p) => k = p,
                None => break,
            }
        }
        let root = *chain.last()?;
        // the arms: a hand is its item node's parent; the chest is where the
        // arm's chain meets the spine
        let mut arms: [Vec<usize>; 2] = Default::default();
        let mut chests = [0usize; 2];
        for s in 0..2 {
            let hand = up(node(b.ids[s])?)?;
            let mut arm = vec![hand];
            let mut c = up(hand)?;
            while !chain.contains(&c) {
                arm.push(c);
                c = up(c)?;
            }
            if arm.len() < 3 {
                return None;
            }
            arm.reverse();
            (arms[s], chests[s]) = (arm, c);
        }
        let chest = if b.bind[chests[0]].y >= b.bind[chests[1]].y { chests[0] } else { chests[1] };
        let spine: Vec<usize> = chain.iter().copied().skip_while(|&i| i != chest).collect();
        // the legs: the chain from the root down to each footstep node, from
        // its first node off the middle (the thigh)
        let mut legs: [Vec<usize>; 2] = Default::default();
        let mut ground = f32::MAX;
        for s in 0..2 {
            let foot = node(b.ids[5 + s])?;
            let mut leg = vec![foot];
            while let Some(p) = up(*leg.last()?) {
                leg.push(p);
            }
            leg.reverse();
            if leg.first() != Some(&root) {
                return None;
            }
            let drop = (b.bind[root].y - b.bind[foot].y).max(1e-3);
            let thigh = leg.iter().skip(1).position(|&i| (b.bind[i].x - b.bind[root].x).abs() > 0.03 * drop)? + 1;
            legs[s] = leg[thigh..].to_vec();
            // the foot's lowest node, the toes too
            let mut stack = vec![foot];
            while let Some(i) = stack.pop() {
                ground = ground.min(b.bind[i].y);
                stack.extend((0..n).filter(|&c| b.parents[c] == i as i32));
            }
        }
        // an arm's upper bone: the third from its hand
        let upper = |s: usize| arms[s][arms[s].len() - 3];
        let shoulder_y = (b.bind[upper(0)].y + b.bind[upper(1)].y) * 0.5;
        if shoulder_y - ground < 0.2 * (b.bind[head].y - ground).max(1e-3) {
            return None;
        }
        let mid_x = b.bind[root].x;
        // the torso's skin: the vertices skinned most to the spine, on the
        // body's nearest level of detail
        let mut used = vec![false; body.pos.len()];
        for bt in body.batches.iter().filter(|bt| bt.lod.unwrap_or(0) == 0) {
            for &v in &bt.indices {
                used[v as usize] = true;
            }
        }
        let torso: Vec<Vec3> = (0..body.pos.len())
            .filter(|&v| used[v])
            .filter(|&v| {
                let (bb, w) = (body.bones.get(v).copied().unwrap_or([255; 4]), body.weights.get(v).copied().unwrap_or([0; 4]));
                let k = (0..4).max_by_key(|&k| w[k]).unwrap_or(0);
                spine.contains(&(bb[k] as usize))
            })
            .map(|v| Vec3::from(body.pos[v]))
            .collect();
        let tall = shoulder_y - ground;
        // its width over the top fifth below the shoulders, its back over the
        // middle of the torso (each the twentieth from the edge, so one stray
        // vertex does not set it)
        let edge = |mut v: Vec<f32>, top: bool| -> Option<f32> {
            if v.is_empty() {
                return None;
            }
            v.sort_by(f32::total_cmp);
            let k = v.len() / 20;
            Some(if top { v[v.len() - 1 - k] } else { v[k] })
        };
        let half_width = edge(torso.iter().filter(|p| (shoulder_y - 0.2 * tall..=shoulder_y).contains(&p.y)).map(|p| (p.x - mid_x).abs()).collect(), true)?;
        let (lo, hi) = (b.bind[root].y, b.bind[chest].y);
        let back = edge(torso.iter().filter(|p| (lo..=hi.max(lo + 1e-3)).contains(&p.y)).map(|p| p.z).collect(), false)?;
        (half_width > 1e-3).then_some(Frame { root, chest, head, spine, arms, legs, shoulder_y, ground, half_width, back, mid_x })
    }

    /// The source skeleton's nodes on another body's (frame `g`, skeleton
    /// `tb`) by their parts: the spine's by where they are between the root and
    /// the chest, the head's, each arm's from the hand up (the wearer's arm on
    /// the same side) and each leg's from the thigh down; every other node
    /// (a back flap, a finger, a chest's front) anchored on the nearest of its
    /// parents that has a counterpart. For each node its anchor's node, and
    /// its anchor.
    fn roles(f: &Frame, sb: &Bones, g: &Frame, tb: &Bones) -> (Vec<usize>, Vec<usize>) {
        let n = sb.parents.len();
        let mut role: Vec<Option<usize>> = vec![None; n];
        let frac = |fr: &Frame, b: &Bones, i: usize| (b.bind[i].y - b.bind[fr.root].y) / (b.bind[fr.chest].y - b.bind[fr.root].y).max(1e-4);
        for &i in &f.spine {
            let t = frac(f, sb, i);
            role[i] = g.spine.iter().copied().min_by(|&a, &b| (frac(g, tb, a) - t).abs().total_cmp(&(frac(g, tb, b) - t).abs()));
        }
        role[f.root] = Some(g.root);
        role[f.chest] = Some(g.chest);
        role[f.head] = Some(g.head);
        // a limb of the source on the wearer's limb on the same side
        let side = |fr: &Frame, b: &Bones, x: usize| (b.bind[x].x - fr.mid_x).signum();
        for s in 0..2 {
            let a = &f.arms[s];
            let t = &g.arms[(0..2).find(|&k| side(g, tb, g.arms[k][0]) == side(f, sb, a[0])).unwrap_or(s)];
            for k in 0..a.len() {
                role[a[a.len() - 1 - k]] = Some(t[t.len().saturating_sub(1 + k)]);
            }
            let a = &f.legs[s];
            let t = &g.legs[(0..2).find(|&k| side(g, tb, g.legs[k][0]) == side(f, sb, a[0])).unwrap_or(s)];
            for k in 0..a.len() {
                role[a[k]] = Some(t[k.min(t.len() - 1)]);
            }
        }
        let anchor: Vec<usize> = (0..n)
            .map(|i| {
                let mut k = i;
                while role[k].is_none() {
                    match usize::try_from(sb.parents[k]).ok().filter(|&p| p < n) {
                        Some(p) => k = p,
                        None => return f.root,
                    }
                }
                k
            })
            .collect();
        let map = (0..n).map(|i| role[i].unwrap_or(g.root)).collect();
        (map, anchor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The disc's CHAR.ESF and CHARCUST.CSF in a library, or None (SKIP)
    /// without `run/eqoa.iso`.
    pub(crate) fn char_lib() -> Option<Library> {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let mut iso = iso::Iso::open(&path).ok()?;
        let mut lib = Library::new();
        for f in ["CHARCUST.CSF", "CHARFACE.CSF", "CHAR.ESF", "ITEM.ESF"] {
            lib.add(f, esf::Container::open(iso.read(f).ok()?).ok()?);
        }
        Some(lib)
    }

    fn sprite(lib: &Library, id: u32) -> (CSprite<Ee>, esf::Model) {
        let r = lib.lookup(id as i32).unwrap_or_else(|| panic!("{id:08X} in CHAR.ESF"));
        let cs = load_csprite::<Ee>(lib, r, &mut AnimScene::default(), &mut AnimCache::new(), &mut Rand(1)).unwrap();
        (cs, lib.build_model(r).unwrap())
    }

    /// The player bodies, male and female (the 0x002C model ids of the races
    /// 0-9 and the froglok's), and a server's races' bodies.
    const PLAYERS: &[u32] = &[
        0x70D8_98C6, 0x0D53_7335, 0xD8D5_6FF8, 0xE189_19F7, 0x7ED8_9DAA, 0x8C9B_4B39, 0xA99C_6B15, 0x4C40_84FC, 0xAA5B_0447, 0x350B_FDBE, 0x4C6F_A532, 0x61CA_3EDF, 0xC44A_D516,
        0xA3DB_3FE2, 0x8A27_A731, 0x320C_0B47, 0x4F1A_3CBE, 0x8480_7490,
    ];
    const RACES: &[u32] = &[
        0xFFB1_EA2C, 0x2217_6258, 0xB749_BF57, 0x6C13_08E0, 0x3151_DF13, 0x8EFD_12EA, 0x9112_20E2, 0x31A1_5F17, 0x4376_EB52, 0xCF41_5561, 0x8214_A32D, 0xCFC4_78EE, 0xC5CB_F4CF, 0xBADC_25C9,
        0xBE10_226F, 0x70EF_CE05, 0x6AF9_8A81, 0x4A4D_88FB, 0x141B_FE0C,
    ];

    /// Every style is a cape cut out of its body: its own materials' triangles
    /// alone, behind the body (its middle behind the spine) and long (from
    /// above the waist to below the knees); the control: a style of no art,
    /// and the body's other materials, make none.
    #[test]
    fn every_style_is_a_cape_of_its_body() {
        let Some(lib) = char_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        for s in STYLES {
            let src = source(&lib, s).unwrap_or_else(|| panic!("style {}: {}", s.id, s.what));
            let m = &src.model;
            assert!(m.triangles >= 60 && m.batches.iter().all(|b| s.entries.contains(&b.material)), "style {}: {} triangles", s.id, m.triangles);
            let (lo, hi) = m.pos.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p))));
            let mid_z = m.pos.iter().map(|p| p[2]).sum::<f32>() / m.pos.len() as f32;
            assert!(mid_z < -0.05, "style {}: behind ({mid_z:.2})", s.id);
            assert!(hi.y > 1.3 && lo.y < 0.5, "style {}: from the shoulders down ({:.2}..{:.2})", s.id, lo.y, hi.y);
        }
        assert!(style(0).is_none() && style(200).is_none(), "the control: no art");
        let (_, knight) = sprite(&lib, 0xD130_0BD3);
        assert_eq!(cut(&knight, &[99]).triangles, 0, "the control: no such material");
    }

    /// A cape on the skeleton it was made on is the cape as it is (the
    /// human's skeleton is the knight's, vampire's and Dawn Maiden's; the
    /// elf's Dread's); on any other player body or a server's race's body it
    /// names only the wearer's nodes, every vertex skinned, its top at the
    /// shoulders, and what hangs below the hips hangs behind the back to below
    /// them. The control: on a body with wings, none.
    #[test]
    fn a_cape_fits_every_player_body_and_the_races() {
        let Some(lib) = char_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        for s in STYLES {
            let src = source(&lib, s).unwrap();
            for &id in PLAYERS.iter().chain(RACES) {
                let (cs, body) = sprite(&lib, id);
                let Some(m) = model(&lib, Cape { style: s.id, color: 0 }, id, &cs) else {
                    panic!("style {} on {id:08X}: none", s.id);
                };
                let n = cs.h.nodes.len();
                assert!(m.bones.iter().flatten().all(|&b| b == 255 || (b as usize) < n), "style {} on {id:08X}: its nodes alone", s.id);
                assert!(m.weights.iter().zip(&m.bones).all(|(w, b)| (0..4).any(|k| b[k] != 255 && w[k] > 0)), "style {} on {id:08X}: every vertex skinned", s.id);
                if Bones::of(&cs).same(&src.bones) {
                    assert_eq!(m.pos, src.model.pos, "style {} on {id:08X}: as it is", s.id);
                    continue;
                }
                let f = Frame::of(&Bones::of(&cs), &body).unwrap_or_else(|| panic!("{id:08X}: a frame"));
                let tall = f.shoulder_y - f.ground;
                let (lo, hi) = m.pos.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p))));
                // what hangs between the hips and the ankles is behind the back
                // (the collar and a vampire's sides wrap round to the front)
                let hips = Bones::of(&cs).bind[f.root].y;
                let low: Vec<f32> = m.pos.iter().filter(|p| p[1] < hips && p[1] > f.ground + 0.1 * tall).map(|p| p[2]).collect();
                let mid_z = low.iter().sum::<f32>() / low.len().max(1) as f32;
                assert!(!low.is_empty() && mid_z < f.back + 0.02 * tall, "style {} on {id:08X}: hanging behind its back ({mid_z:.2} against {:.2})", s.id, f.back);
                assert!(hi.y > f.shoulder_y - 0.15 * tall && hi.y < f.shoulder_y + 0.5 * tall, "style {} on {id:08X}: its top at the shoulders ({:.2} against {:.2})", s.id, hi.y, f.shoulder_y);
                assert!(lo.y < f.ground + 0.55 * tall, "style {} on {id:08X}: below the hips ({:.2})", s.id, lo.y);
            }
            for &id in NO_CAPE {
                let (cs, _) = sprite(&lib, id);
                assert!(model(&lib, Cape { style: s.id, color: 0 }, id, &cs).is_none(), "the control: {id:08X} has wings");
            }
        }
    }

    /// The same cape fitted twice is the same (it is kept), and a colour
    /// multiplies its layers but their alpha; the control: colour 0 leaves the
    /// art as it is.
    #[test]
    fn a_colour_tints_the_cape_and_none_leaves_it() {
        let Some(lib) = char_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let (cs, _) = sprite(&lib, 0x70D8_98C6);
        let plain = model(&lib, Cape { style: 1, color: 0 }, 0x70D8_98C6, &cs).unwrap();
        let again = model(&lib, Cape { style: 1, color: 0 }, 0x70D8_98C6, &cs).unwrap();
        assert_eq!(plain.pos, again.pos);
        let red = model(&lib, Cape { style: 1, color: 0xFF00_0000 }, 0x70D8_98C6, &cs).unwrap();
        for (a, b) in plain.batches.iter().zip(&red.batches) {
            for (la, lb) in a.layers.iter().zip(&b.layers) {
                assert_eq!(lb.color, [la.color[0], 0, 0, la.color[3]], "red over its colour, its alpha kept");
            }
        }
        assert_eq!(plain.batches[0].layers[0].color, again.batches[0].layers[0].color, "the control: no colour, the art's");
    }

    /// The two role maps send each player skeleton's nodes to nodes of the
    /// other, the cape's bones (pelvis, spine, chest, shoulders, back flaps,
    /// thighs and shins) to the same places' bones within 0.3 (the forty-three's
    /// hips sit lower);
    /// the control: the layouts are told apart by their parents alone.
    #[test]
    fn the_role_maps_join_like_bones() {
        let Some(lib) = char_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let (h, _) = sprite(&lib, 0x70D8_98C6);
        let (e, _) = sprite(&lib, 0xA99C_6B15);
        let (h, e) = (Bones::of(&h), Bones::of(&e));
        assert_eq!((layout_of(&h.parents), layout_of(&e.parents)), (Some(Layout::P40), Some(Layout::P43)));
        assert_eq!(layout_of(&h.parents[..39]), None, "the control");
        for (from, to, map, bones) in [(&h, &e, &ON_43[..], &[0usize, 1, 2, 11, 20, 29, 30, 33, 34, 38, 39][..]), (&e, &h, &ON_40[..], &[0, 2, 3, 10, 21, 32, 33, 36, 37, 41, 42][..])] {
            assert!(map.iter().all(|&j| (j as usize) < to.bind.len()));
            for &b in bones {
                let d = from.bind[b].distance(to.bind[map[b] as usize]);
                assert!(d < 0.3, "{b} -> {}: {d:.2} apart", map[b]);
            }
        }
    }
}
