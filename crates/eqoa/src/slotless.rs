//! Armour on a body whose CSprite has no text slots (a prototype: the user,
//! 2026-11-08, "Lay armour textures on", to be judged by its pictures).
//!
//! A body is dressed by its text slots (`vi::cust`): `SetArmorSlot` and
//! `SetRobe` put the armour's art and tint on the palette entry and layer the
//! CSprite's TSlotList (0x2930) names for each slot. Nine of the bodies
//! go-eqoa-server gives races of its own have no TSlotList at all (the kappa,
//! brownie, pixie, fairy, minotaur, dryad, skeleton, fungusman and gargoyle;
//! the hill giant too), so neither a PS2 nor this client drew anything they
//! wear. The disc has no armoured variant of them, and their materials are
//! atlases (one texture over the whole body, or a few that cut across the
//! slots), so no palette entry of theirs stands for a chest or a pair of legs.
//!
//! Here such a body wears the armour as a human does. Its torso, arms, hands,
//! legs and feet are found from its skeleton: the hands are the parents of
//! the item nodes its NodeIDList names (slots 0 and 1), the feet are where
//! the chains down to its footstep nodes (5 and 6) end, the head is the node
//! it names for the head (7); the chains between them and the root are the
//! arms, the legs and the spine, a leg's knee and ankle the joints nearest a
//! human's (so a hoofed leg's long bones fall as a human's do), the neck (the
//! spine above where the arms hang) the head's, as a human's is. Nodes off the
//! spine whose skin hugs the torso (a belly, a chest, a kilt's flaps) are the
//! torso's; ones that reach away from it (wings, a tail) are not. Each
//! triangle on the torso and the limbs is carried onto a human body
//! (HumanMale, HumanFemale for a female body) segment by segment (the spine,
//! the upper arm, the forearm, the hand, the thigh, the shin, the foot: along
//! the bone, and round it at the human's girth), and takes the text slot and
//! texture coordinates of the human's surface where it lands: the surface a
//! human shows in the same look (the human's mesh also carries a robe's skirt
//! and cuffs, which armour hides with see-through texels), and within one of
//! its UV islands per triangle, so no triangle spans two pieces of an armour
//! texture. The armour is then drawn over the body's own skin on those
//! triangles, a batch per slot with the human's slot layer (a human's armour
//! is a layer over its skin: a texel the human shows its skin through shows
//! the body's). The head, what reaches away from the torso, and translucent
//! or untextured parts keep the body's own look.
//!
//! Only a body with no text slot at all changes; one with a TSlotList is
//! dressed by `vi::cust` exactly as before. `set_enabled(false)` turns it off
//! (for an Extended row: a puppet is made anew when its look is applied).

use esf::{Era, Library};
use glam::Vec3;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use vi::csprite::{CSprite, Rand};
use vi::cust::{LayerChange, Look, Tables};
use vi::fpu::Ee;

/// The palette entries the five made-up slots name: 0x5100 + slot. A
/// palette has a few dozen entries at most, so none is a body's own.
pub const ENTRY: i32 = 0x5100;

/// The worn slots (chest, bracer, gloves, legs, boots).
const SLOTS: usize = 5;

/// A human's knee and ankle, as fractions of the way from the hip joint
/// down to the end of the leg (HumanMale's: 1.02, 0.58, 0.12, 0).
const KNEE: f32 = 0.43;
const ANKLE: f32 = 0.88;

/// A node off the spine is the torso's when nine in ten of the vertices on
/// it are no farther from the spine than this times the torso's own skin at
/// that height (a chest's, a belly's and a kilt's are within it; a bee's
/// abdomen, a tail and wings reach past it).
const HUG: f32 = 1.2;

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Whether slotless bodies are dressed (on unless turned off).
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Turns the dressing of slotless bodies on or off, for the puppets made
/// from then on.
#[allow(dead_code)]
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// A body with no text slot at all: no TSlotList, or one that names none.
pub fn is_slotless(cs: &CSprite<Ee>) -> bool {
    cs.text_slots.iter().all(|s| s.0 == -1)
}

/// The text slots `vi::cust::apply_look_slots` dresses a slotless body by:
/// the five worn slots on the made-up entries, layer 0, nothing else (no
/// hair, no face).
pub fn slots() -> [(i32, i32); 19] {
    let mut s = [(-1, -1); 19];
    for (i, slot) in s.iter_mut().enumerate().take(SLOTS) {
        *slot = (ENTRY + i as i32, 0);
    }
    s
}

/// Each made-up slot's art after `changes` (the last texture given to it,
/// as on a body's own layers), and its colour.
fn worn(changes: &[LayerChange]) -> [(Option<u32>, Option<[u8; 4]>); SLOTS] {
    let mut out = [(None, None); SLOTS];
    for ch in changes {
        if let Some(o) = usize::try_from(ch.entry - ENTRY).ok().filter(|&i| i < SLOTS && ch.layer == 0).map(|i| &mut out[i]) {
            o.0 = ch.texture.or(o.0);
            o.1 = ch.color.or(o.1);
        }
    }
    out
}

/// Where the look puts the armour on a slotless body: `model` (the body as
/// `build_model` made it, `id` its DictID) gains a batch for each slot the
/// look gives art to, drawn over the body's own triangles there. False when
/// nothing was added (no art worn, a body this cannot fit, or turned off).
/// The look's textures resolve as `has` says, by the tables `t`, as
/// `vi::cust::apply_look_with` dresses a body with slots.
#[allow(clippy::too_many_arguments)]
pub fn dress(lib: &Library, id: i32, model: &mut esf::Model, cs: &CSprite<Ee>, look: &Look, helm_worn: bool, has: impl Fn(u32) -> bool, t: &Tables) -> bool {
    if !enabled() || !is_slotless(cs) {
        return false;
    }
    let changes = vi::cust::apply_look_slots(&slots(), cs.race, cs.sex, look, helm_worn, &has, t);
    let art = worn(&changes).map(|(t, _)| t);
    if art.iter().all(Option::is_none) {
        return false;
    }
    // the legs in a robe's lower half (the same look less the robe dresses
    // them otherwise)
    let unrobed = vi::cust::apply_look_slots(&slots(), cs.race, cs.sex, &Look { robe: -1, ..*look }, helm_worn, &has, t);
    let robed = art[3].is_some() && worn(&unrobed)[3].0 != art[3];
    let Some(fit) = fit_of(lib, id, model, cs, art, robed) else { return false };
    apply(model, &fit, &changes)
}

/// The layer changes on the made-up slots put on `model` as batches: per
/// slot with art, its triangles again (their own vertices, the human's
/// texture coordinates) with the human's slot layer in the slot's texture
/// and colour.
pub fn apply(model: &mut esf::Model, fit: &Fit, changes: &[LayerChange]) -> bool {
    let mut added = false;
    for (slot, (tex, color)) in worn(changes).into_iter().enumerate() {
        // no art for the slot (no set, or a set whose art is not there): the
        // body's own skin, as a human's slot keeps its own texture
        let Some(tex) = tex else { continue };
        let tris = &fit.tris[slot];
        if tris.is_empty() {
            continue;
        }
        let mut layer = fit.layers[slot].clone();
        layer.texture = tex as i32;
        if let Some(c) = color {
            layer.color = c;
        }
        let mut lods: Vec<Option<u8>> = tris.iter().map(|t| t.lod).collect();
        lods.sort();
        lods.dedup();
        for lod in lods {
            let mut indices = Vec::new();
            for t in tris.iter().filter(|t| t.lod == lod) {
                for k in 0..3 {
                    let v = t.v[k] as usize;
                    indices.push(model.pos.len() as u32);
                    model.pos.push(model.pos[v]);
                    model.uv.push(t.uv[k]);
                    model.normal.push(model.normal[v]);
                    model.color.push(model.color[v]);
                    model.bones.push(model.bones[v]);
                    model.weights.push(model.weights[v]);
                }
            }
            model.triangles += indices.len() as u32 / 3;
            model.batches.push(esf::Batch { material: ENTRY + slot as i32, layers: vec![layer.clone()], indices, lod });
        }
        added = true;
    }
    added
}

/// One of a body's triangles in a slot: its three vertices (the model's
/// indices), the texture coordinates the human has where they land, and the
/// level of detail its batch is drawn at.
#[derive(Clone, Debug, PartialEq)]
pub struct Tri {
    pub v: [u32; 3],
    pub uv: [[f32; 2]; 3],
    pub lod: Option<u8>,
}

/// A slotless body fitted to the human for one look: its triangles per
/// slot, and the human's layer per slot (the one its TSlotList names, its
/// texture coordinates' transform folded into `Tri::uv`).
#[derive(Clone, Debug)]
pub struct Fit {
    pub tris: [Vec<Tri>; SLOTS],
    pub layers: [esf::Layer; SLOTS],
}

/// The art per slot a fit is made for (None: the human's own there).
type Art = [Option<u32>; SLOTS];

/// What was made, by what it was made from (None: it could not be).
type Cache<K, V> = std::cell::RefCell<HashMap<K, Option<Rc<V>>>>;

thread_local! {
    /// The human fitted to, by art and sex.
    static REFS: Cache<(Era, bool), Reference> = std::cell::RefCell::new(HashMap::new());
    /// A body's triangles carried onto the human, by body and art.
    static CARRIED: Cache<(i32, Era), Carried> = std::cell::RefCell::new(HashMap::new());
    /// Fits made, by body, art, what each slot wears and whether the legs
    /// are in a robe.
    static FITS: Cache<(i32, Era, Art, bool), Fit> = std::cell::RefCell::new(HashMap::new());
    /// Textures' see-through texels, by DictID and art.
    static ALPHAS: Cache<(i32, Era), Alpha> = std::cell::RefCell::new(HashMap::new());
}

/// The fit of body `id` (`model` and `cs` built from it in the library's
/// view now) wearing `art`, its legs in a robe's lower half when `robed`,
/// made once per body, art and look.
pub fn fit_of(lib: &Library, id: i32, model: &esf::Model, cs: &CSprite<Ee>, art: Art, robed: bool) -> Option<Rc<Fit>> {
    let era = lib.view();
    let key = (id, era, art, robed);
    if let Some(f) = FITS.with(|f| f.borrow().get(&key).cloned()) {
        return f;
    }
    let female = cs.sex != 0;
    let r = REFS.with(|r| r.borrow().get(&(era, female)).cloned()).unwrap_or_else(|| {
        let made = Human::load(lib, female).and_then(|h| Reference::of(lib, &h)).map(Rc::new);
        REFS.with(|r| r.borrow_mut().insert((era, female), made.clone()));
        made
    });
    let fit = r.and_then(|r| {
        let c = CARRIED.with(|c| c.borrow().get(&(id, era)).cloned()).unwrap_or_else(|| {
            let made = Carried::of(&r, model, cs).map(Rc::new);
            CARRIED.with(|c| c.borrow_mut().insert((id, era), made.clone()));
            made
        })?;
        let alpha: [Option<Rc<Alpha>>; SLOTS] = std::array::from_fn(|i| alpha_of(lib, art[i].map_or(r.own[i], |t| t as i32)));
        Some(Rc::new(fit(&r, &c, &alpha, robed)))
    });
    FITS.with(|f| f.borrow_mut().insert(key, fit.clone()));
    fit
}

/// A texture's see-through texels (the alpha `Surface::decode` gives as
/// stored: 0x80 opaque; the renderer's alpha test passes from 0x40).
struct Alpha {
    w: usize,
    h: usize,
    a: Vec<u8>,
}

impl Alpha {
    /// Whether the texel at `uv` shows (the texture repeated).
    fn at(&self, uv: [f32; 2]) -> bool {
        let x = (uv[0].rem_euclid(1.0) * self.w as f32) as usize % self.w;
        let y = (uv[1].rem_euclid(1.0) * self.h as f32) as usize % self.h;
        self.a[y * self.w + x] >= 0x40
    }

    /// Whether a triangle shows: four of its corners, its edges' middles and
    /// its middle.
    fn shows(&self, uv: &[[f32; 2]; 3]) -> bool {
        let mid = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let c = [(uv[0][0] + uv[1][0] + uv[2][0]) / 3.0, (uv[0][1] + uv[1][1] + uv[2][1]) / 3.0];
        let points = [uv[0], uv[1], uv[2], mid(uv[0], uv[1]), mid(uv[1], uv[2]), mid(uv[2], uv[0]), c];
        points.iter().filter(|&&p| self.at(p)).count() >= 4
    }
}

/// Texture `id`'s see-through texels in the library's view now; None when
/// it does not resolve or decode (then it is taken to show everywhere).
fn alpha_of(lib: &Library, id: i32) -> Option<Rc<Alpha>> {
    let key = (id, lib.view());
    if let Some(a) = ALPHAS.with(|a| a.borrow().get(&key).cloned()) {
        return a;
    }
    let made = lib.surface(id).and_then(|(c, s)| s.decode(&c.data)).map(|img| {
        Rc::new(Alpha { w: img.width as usize, h: img.height as usize, a: img.rgba.chunks_exact(4).map(|p| p[3]).collect() })
    });
    ALPHAS.with(|a| a.borrow_mut().insert(key, made.clone()));
    made
}

fn shows(a: &Option<Rc<Alpha>>, uv: &[[f32; 2]; 3]) -> bool {
    a.as_ref().is_none_or(|a| a.shows(uv))
}

/// The human body a slotless one is fitted to.
struct Human {
    model: esf::Model,
    cs: CSprite<Ee>,
}

impl Human {
    fn load(lib: &Library, female: bool) -> Option<Human> {
        let names: &[&str] = if female { &["HumanFemale", "HumanMale"] } else { &["HumanMale"] };
        names.iter().find_map(|n| {
            let r = lib.lookup(vi::hash::hash_resource_id(n) as i32)?;
            let model = lib.build_model(r)?;
            let mut anims = vi::hsprite::AnimScene::<Ee>::default();
            let cs = vi::character::load_csprite(lib, r, &mut anims, &mut vi::character::AnimCache::new(), &mut Rand(1))?;
            (0..SLOTS).all(|i| cs.text_slots[i].0 != -1).then_some(Human { model, cs })
        })
    }
}

/// Where a node of a skeleton is: on the spine (the root up to the head's
/// parent, the bones between it and a limb, and what hugs the torso), on a
/// limb (and which of its three segments: upper arm, forearm, hand; thigh,
/// shin, foot), on the head, or on something else hanging off the spine (a
/// wing, a tail).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Part {
    Body,
    Limb(Limb, u8),
    Head,
    Other,
}

/// The four limbs: the arm whose hand holds the main item (NodeIDList slot
/// 0), the other's (1), the leg of footstep node 5 and of 6.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Limb {
    Arm(u8),
    Leg(u8),
}

/// What a triangle is fitted within: the body, or one limb.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Group {
    Body,
    Limb(Limb),
}

impl Part {
    fn group(self) -> Option<Group> {
        match self {
            Part::Body => Some(Group::Body),
            Part::Limb(l, _) => Some(Group::Limb(l)),
            _ => None,
        }
    }

    /// The segment a vertex on this part is carried along.
    fn seg(self) -> Option<Seg> {
        match self {
            Part::Body => Some(Seg::Body),
            Part::Limb(l, k) => Some(Seg::Limb(l, k)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Seg {
    Body,
    Limb(Limb, u8),
}

impl Seg {
    fn group(self) -> Group {
        match self {
            Seg::Body => Group::Body,
            Seg::Limb(l, _) => Group::Limb(l),
        }
    }
}

/// A skeleton read for the fit: each node's part, each node's parent, and
/// each segment's two ends in the bind pose (model space).
#[derive(Debug)]
struct Rig {
    part: Vec<Part>,
    parent: Vec<i32>,
    segs: HashMap<Seg, (Vec3, Vec3)>,
    head: usize,
    /// The spine nodes the arms hang from.
    shoulders: Vec<usize>,
}

impl Rig {
    /// From each node's parent and bind position and the NodeIDList; None
    /// when the list does not name the hands, the feet and the head, or the
    /// chains between them are not arms, legs and a spine.
    fn of(parent: &[i32], at: &[Vec3], ids: &[i32; 10]) -> Option<Rig> {
        let n = parent.len();
        let node = |i: i32| usize::try_from(i).ok().filter(|&i| i < n);
        let up = |i: usize| node(parent[i]);
        let children = |i: usize| (0..n).filter(move |&k| parent[k] == i as i32);
        let mut part = vec![None::<Part>; n];
        fn mark(part: &mut [Option<Part>], parent: &[i32], i: usize, p: Part) {
            part[i] = Some(p);
            for k in 0..parent.len() {
                if parent[k] == i as i32 {
                    mark(part, parent, k, p);
                }
            }
        }
        // the node of a subtree farthest from `i`
        let farthest = |i: usize| {
            let mut best = (0.0f32, i);
            let mut stack = vec![i];
            while let Some(k) = stack.pop() {
                let d = at[k].distance(at[i]);
                if d > best.0 {
                    best = (d, k);
                }
                stack.extend(children(k));
            }
            best.1
        };
        let head = node(ids[7])?;
        mark(&mut part, parent, head, Part::Head);
        let mut k = up(head)?;
        loop {
            part[k] = Some(Part::Body);
            match up(k) {
                Some(p) => k = p,
                None => break,
            }
        }
        let root = k;
        let mut segs = HashMap::new();
        let mut shoulders = Vec::new();
        // the arms: a hand is the parent of its item node
        for s in 0..2u8 {
            let item = node(ids[s as usize])?;
            let hand = up(item)?;
            let fore = up(hand)?;
            let upper = up(fore)?;
            if [hand, fore, upper].iter().any(|&i| part[i].is_some()) {
                return None;
            }
            let l = Limb::Arm(s);
            mark(&mut part, parent, upper, Part::Limb(l, 0));
            mark(&mut part, parent, fore, Part::Limb(l, 1));
            mark(&mut part, parent, hand, Part::Limb(l, 2));
            // the bones between the upper arm and the spine (a collarbone)
            let mut k = up(upper)?;
            while part[k].is_none() {
                part[k] = Some(Part::Body);
                k = up(k)?;
            }
            shoulders.push(k);
            segs.insert(Seg::Limb(l, 0), (at[upper], at[fore]));
            segs.insert(Seg::Limb(l, 1), (at[fore], at[hand]));
            segs.insert(Seg::Limb(l, 2), (at[hand], at[farthest(hand)]));
        }
        // the legs: the chain from the root down to the footstep node's
        // farthest end; the hip is its first joint off the middle, the knee
        // and the ankle the joints below it nearest a human's
        for s in 0..2u8 {
            let end = farthest(node(ids[5 + s as usize])?);
            let mut chain = vec![end];
            while let Some(p) = up(*chain.last()?) {
                chain.push(p);
            }
            chain.reverse();
            if chain[0] != root {
                return None;
            }
            let drop = (at[root].y - at[end].y).max(1e-3);
            let hip = (1..chain.len()).find(|&i| (at[chain[i]].x - at[root].x).abs() > 0.03 * drop)?;
            let height = (at[chain[hip]].y - at[end].y).max(1e-3);
            let frac = |i: usize| (at[chain[hip]].y - at[chain[i]].y) / height;
            let nearest = |from: usize, f: f32| (from..chain.len()).min_by(|&a, &b| (frac(a) - f).abs().total_cmp(&(frac(b) - f).abs()));
            let knee = nearest(hip + 1, KNEE).filter(|&k| k + 1 < chain.len())?;
            let ankle = nearest(knee + 1, ANKLE)?;
            let (hip, knee, ankle) = (chain[hip], chain[knee], chain[ankle]);
            if [hip, knee, ankle].iter().any(|&i| part[i].is_some()) {
                return None;
            }
            // a foot with no bone past the ankle: a stub forward and down
            let toe = match farthest(ankle) {
                t if t != ankle => at[t],
                _ => at[ankle] + Vec3::new(0.0, -0.25, 0.25) * at[knee].distance(at[ankle]),
            };
            let l = Limb::Leg(s);
            mark(&mut part, parent, hip, Part::Limb(l, 0));
            mark(&mut part, parent, knee, Part::Limb(l, 1));
            mark(&mut part, parent, ankle, Part::Limb(l, 2));
            let mut k = up(hip)?;
            while part[k].is_none() {
                part[k] = Some(Part::Body);
                k = up(k)?;
            }
            segs.insert(Seg::Limb(l, 0), (at[hip], at[knee]));
            segs.insert(Seg::Limb(l, 1), (at[knee], at[ankle]));
            segs.insert(Seg::Limb(l, 2), (at[ankle], toe));
        }
        segs.insert(Seg::Body, (at[root], at[head]));
        // a body that stands on two legs: its hands well off the ground (a
        // four-legged body's "hands" are its forefeet)
        let ground = [Seg::Limb(Limb::Leg(0), 2), Seg::Limb(Limb::Leg(1), 2)].iter().map(|s| segs[s].1.y.min(segs[s].0.y)).fold(f32::MAX, f32::min);
        let tall = at[head].y - ground;
        if [Seg::Limb(Limb::Arm(0), 2), Seg::Limb(Limb::Arm(1), 2)].iter().any(|s| segs[s].0.y - ground < 0.25 * tall) {
            return None;
        }
        Some(Rig { part: part.into_iter().map(|p| p.unwrap_or(Part::Other)).collect(), parent: parent.to_vec(), segs, head, shoulders })
    }

    /// The neck made the head's: the spine between the head and the node
    /// the arms hang from (a human's neck is its head's material, and its
    /// armour stops below it).
    fn neck(&mut self) {
        let mut k = self.parent[self.head];
        while let Ok(i) = usize::try_from(k) {
            if self.shoulders.contains(&i) || self.part[i] != Part::Body {
                break;
            }
            self.part[i] = Part::Head;
            k = self.parent[i];
        }
    }

    fn of_sprite(cs: &CSprite<Ee>) -> Option<Rig> {
        let parent: Vec<i32> = cs.h.nodes.iter().map(|n| n.parent).collect();
        let at: Vec<Vec3> = cs.h.nodes.iter().map(|n| Vec3::new(n.bind.trans.x.0, n.bind.trans.y.0, n.bind.trans.z.0)).collect();
        Rig::of(&parent, &at, &cs.node_ids)
    }

    /// The nodes off the spine whose skin hugs the torso made the torso's:
    /// each subtree of `Other` nodes hanging off a `Body` one, when nine in
    /// ten of the vertices on it (`on`: a vertex, the node it is on) are no
    /// farther from the spine (`body`, the torso's segment) than `HUG` times
    /// the farthest of the torso's own at their height (in eight bands from
    /// its lowest vertex to its highest; past them, the nearest band).
    fn fold(&mut self, body: &Frame, on: &[(Vec3, usize)]) {
        let n = self.part.len();
        let round = |p: Vec3| {
            let q = p - body.a;
            (q.dot(body.d), (q.dot(body.e1).powi(2) + q.dot(body.e2).powi(2)).sqrt())
        };
        let torso: Vec<(f32, f32)> = on.iter().filter(|(_, b)| self.part.get(*b) == Some(&Part::Body)).map(|(p, _)| round(*p)).collect();
        if torso.is_empty() {
            return;
        }
        const BANDS: usize = 8;
        let (lo, hi) = torso.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &(t, _)| (lo.min(t), hi.max(t)));
        let band = |t: f32| (((t - lo) / (hi - lo).max(1e-6) * BANDS as f32) as isize).clamp(0, BANDS as isize - 1) as usize;
        let mut skin = [0.0f32; BANDS];
        for &(t, r) in &torso {
            skin[band(t)] = skin[band(t)].max(r);
        }
        // a band with no torso skin takes the nearest band's that has some
        let had = skin;
        for (i, s) in skin.iter_mut().enumerate() {
            if *s == 0.0 {
                *s = (0..BANDS).filter(|&j| had[j] > 0.0).min_by_key(|&j| j.abs_diff(i)).map_or(0.0, |j| had[j]);
            }
        }
        for k in 0..n {
            let off_spine = self.part[k] == Part::Other && usize::try_from(self.parent[k]).ok().is_some_and(|p| self.part[p] == Part::Body);
            if !off_spine {
                continue;
            }
            let mut tree = vec![false; n];
            let mut stack = vec![k];
            while let Some(i) = stack.pop() {
                tree[i] = true;
                stack.extend((0..n).filter(|&c| self.parent[c] == i as i32));
            }
            let mut d: Vec<f32> = on
                .iter()
                .filter(|(_, b)| tree.get(*b) == Some(&true))
                .map(|(p, _)| {
                    let (t, r) = round(*p);
                    r / skin[band(t)].max(1e-4)
                })
                .collect();
            if d.is_empty() {
                continue;
            }
            d.sort_by(f32::total_cmp);
            if d[(d.len() * 9 / 10).min(d.len() - 1)] <= HUG {
                for (i, t) in tree.iter().enumerate() {
                    if *t {
                        self.part[i] = Part::Body;
                    }
                }
            }
        }
    }
}

/// A segment's frame: from its first end along it (`d`, `len` long), and
/// round it (`e1` toward the front, or up for a bone that points forward;
/// `e2` across), with the girth of the vertices on it along each.
#[derive(Clone, Copy, Debug)]
struct Frame {
    a: Vec3,
    d: Vec3,
    len: f32,
    e1: Vec3,
    e2: Vec3,
    r: [f32; 2],
}

impl Frame {
    fn new(a: Vec3, b: Vec3) -> Frame {
        let len = a.distance(b).max(1e-3);
        let d = if a.distance(b) > 1e-6 { (b - a) / a.distance(b) } else { Vec3::NEG_Y };
        let side = |r: Vec3| r - d * r.dot(d);
        let f = side(Vec3::Z);
        let e1 = if f.length() > 0.35 { f.normalize() } else { side(Vec3::Y).normalize() };
        Frame { a, d, len, e1, e2: d.cross(e1), r: [1.0, 1.0] }
    }

    /// Along, and round in girths.
    fn local(&self, p: Vec3) -> Vec3 {
        let q = p - self.a;
        Vec3::new(q.dot(self.d) / self.len, q.dot(self.e1) / self.r[0], q.dot(self.e2) / self.r[1])
    }

    fn world(&self, l: Vec3) -> Vec3 {
        self.a + self.d * (l.x * self.len) + self.e1 * (l.y * self.r[0]) + self.e2 * (l.z * self.r[1])
    }
}

/// Each segment's frame, its girth measured over the vertices on it.
fn frames(rig: &Rig, verts: impl Iterator<Item = (Vec3, Seg)>) -> HashMap<Seg, Frame> {
    let mut f: HashMap<Seg, Frame> = rig.segs.iter().map(|(&s, &(a, b))| (s, Frame::new(a, b))).collect();
    let mut sums: HashMap<Seg, (f32, f32, u32)> = HashMap::new();
    for (p, s) in verts {
        if let Some(fr) = f.get(&s) {
            let q = p - fr.a;
            let e = sums.entry(s).or_default();
            e.0 += q.dot(fr.e1).powi(2);
            e.1 += q.dot(fr.e2).powi(2);
            e.2 += 1;
        }
    }
    for (s, fr) in f.iter_mut() {
        if let Some(&(a, b, n)) = sums.get(s).filter(|v| v.2 > 0) {
            fr.r = [(a / n as f32).sqrt().max(0.005), (b / n as f32).sqrt().max(0.005)];
        }
    }
    f
}

/// A vertex's heaviest bone (none for a vertex on no bone).
fn bone_of(model: &esf::Model, v: usize) -> Option<usize> {
    let (b, w) = (model.bones[v], model.weights[v]);
    (0..4).filter(|&k| b[k] != 255 && w[k] > 0).max_by_key(|&k| (w[k], std::cmp::Reverse(k))).map(|k| b[k] as usize)
}

/// A vertex's part: its heaviest bone's.
fn part_of(model: &esf::Model, rig: &Rig, v: usize) -> Part {
    bone_of(model, v).and_then(|b| rig.part.get(b).copied()).unwrap_or(Part::Other)
}

/// A triangle's group: where most of its three vertices' weight is, None
/// when at least as much is on the head or on something off the spine.
fn group_of(model: &esf::Model, rig: &Rig, t: [usize; 3]) -> Option<Group> {
    let mut by: HashMap<Option<Group>, u32> = HashMap::new();
    for &v in &t {
        let (b, w) = (model.bones[v], model.weights[v]);
        for k in 0..4 {
            if b[k] != 255 && w[k] > 0 {
                let g = rig.part.get(b[k] as usize).and_then(|p| p.group());
                *by.entry(g).or_default() += w[k] as u32;
            }
        }
    }
    let off = by.get(&None).copied().unwrap_or(0);
    let (_, g, w) = by.into_iter().filter_map(|(g, w)| g.map(|g| (matches!(g, Group::Body), g, w))).max_by_key(|&(body, _, w)| (w, body))?;
    (w > off).then_some(g)
}

/// The closest point of triangle `p` to `q`, as barycentric weights, and
/// the squared distance to it.
fn closest(q: Vec3, p: &[Vec3; 3]) -> (f32, [f32; 3]) {
    // Ericson, Real-Time Collision Detection, 5.1.5
    let (a, b, c) = (p[0], p[1], p[2]);
    let (ab, ac, ap) = (b - a, c - a, q - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    let w = if d1 <= 0.0 && d2 <= 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        let bp = q - b;
        let (d3, d4) = (ab.dot(bp), ac.dot(bp));
        if d3 >= 0.0 && d4 <= d3 {
            [0.0, 1.0, 0.0]
        } else {
            let vc = d1 * d4 - d3 * d2;
            if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
                let v = d1 / (d1 - d3);
                [1.0 - v, v, 0.0]
            } else {
                let cp = q - c;
                let (d5, d6) = (ab.dot(cp), ac.dot(cp));
                if d6 >= 0.0 && d5 <= d6 {
                    [0.0, 0.0, 1.0]
                } else {
                    let vb = d5 * d2 - d1 * d6;
                    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
                        let w = d2 / (d2 - d6);
                        [1.0 - w, 0.0, w]
                    } else {
                        let va = d3 * d6 - d5 * d4;
                        if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
                            let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
                            [0.0, 1.0 - w, w]
                        } else {
                            let denom = 1.0 / (va + vb + vc);
                            let (v, w) = (vb * denom, vc * denom);
                            [1.0 - v - w, v, w]
                        }
                    }
                }
            }
        }
    };
    let x = a * w[0] + b * w[1] + c * w[2];
    (x.distance_squared(q), w)
}

/// The triangles of `model` drawn at its nearest level (or at every level).
fn near_triangles(model: &esf::Model) -> impl Iterator<Item = (&esf::Batch, [usize; 3])> {
    model.batches.iter().filter(|b| b.lod.is_none_or(|l| l == 0)).flat_map(|b| b.indices.chunks_exact(3).map(move |t| (b, [t[0] as usize, t[1] as usize, t[2] as usize])))
}

fn pos(model: &esf::Model, v: usize) -> Vec3 {
    Vec3::from(model.pos[v])
}

/// A triangle of the human's slot materials: in model space, with its
/// texture coordinates (as the slot's layer samples them), slot, UV island,
/// and whether another layer of its material (a human's skin) shows on it.
struct Target {
    p: [Vec3; 3],
    uv: [[f32; 2]; 3],
    slot: usize,
    island: usize,
    skin: bool,
}

/// The human: its slot triangles per group, the frames of its segments,
/// each slot's layer and the texture that layer has of its own.
struct Reference {
    frames: HashMap<Seg, Frame>,
    targets: HashMap<Group, Vec<Target>>,
    layers: [esf::Layer; SLOTS],
    own: [i32; SLOTS],
}

impl Reference {
    fn of(lib: &Library, h: &Human) -> Option<Reference> {
        let mut rig = Rig::of_sprite(&h.cs)?;
        // on the human, what hangs off the spine (a robe's flaps, the chest
        // node) is the body's: its armour is there
        for p in rig.part.iter_mut() {
            if *p == Part::Other {
                *p = Part::Body;
            }
        }
        let m = &h.model;
        let slot_of = |b: &esf::Batch| (0..SLOTS).find(|&i| h.cs.text_slots[i].0 == b.material);
        let mut layers: Vec<esf::Layer> = Vec::new();
        for i in 0..SLOTS {
            let (entry, layer) = h.cs.text_slots[i];
            let l = m.batches.iter().find(|b| b.material == entry).and_then(|b| b.layers.get(layer as usize)).cloned()?;
            layers.push(l);
        }
        // the UV islands: triangles of one material sharing an edge whose
        // ends have one place and one texture coordinate
        let mut weld: HashMap<([u32; 3], [u32; 2], i32), usize> = HashMap::new();
        let mut tris: Vec<(&esf::Batch, usize, [usize; 3], [usize; 3])> = Vec::new();
        for (b, t) in near_triangles(m) {
            let Some(slot) = slot_of(b) else { continue };
            let mut key = |v: usize| {
                let n = weld.len();
                *weld.entry((m.pos[v].map(f32::to_bits), m.uv[v].map(f32::to_bits), b.material)).or_insert(n)
            };
            let w = [key(t[0]), key(t[1]), key(t[2])];
            tris.push((b, slot, t, w));
        }
        let mut island: Vec<usize> = (0..tris.len()).collect();
        fn find(x: &mut [usize], mut i: usize) -> usize {
            while x[i] != i {
                x[i] = x[x[i]];
                i = x[i];
            }
            i
        }
        let mut edges: HashMap<(usize, usize), usize> = HashMap::new();
        for (k, (_, _, _, w)) in tris.iter().enumerate() {
            for e in 0..3 {
                let (a, b) = (w[e].min(w[(e + 1) % 3]), w[e].max(w[(e + 1) % 3]));
                match edges.get(&(a, b)) {
                    Some(&o) => {
                        let (x, y) = (find(&mut island, o), find(&mut island, k));
                        island[x] = y;
                    }
                    None => {
                        edges.insert((a, b), k);
                    }
                }
            }
        }
        let mut targets: HashMap<Group, Vec<Target>> = HashMap::new();
        let mut on: Vec<(Vec3, Seg)> = Vec::new();
        for (k, &(b, slot, t, _)) in tris.iter().enumerate() {
            let Some(g) = group_of(m, &rig, t) else { continue };
            for &v in &t {
                if let Some(s) = part_of(m, &rig, v).seg() {
                    on.push((pos(m, v), s));
                }
            }
            let sl = h.cs.text_slots[slot].1 as usize;
            // the skin: an opaque layer of the material other than the slot's
            let skin = b.layers.iter().enumerate().any(|(j, l)| j != sl && l.blend == 0 && l.texture != 0 && alpha_of(lib, l.texture).is_some_and(|a| a.shows(&t.map(|v| l.apply_uv(m.uv[v])))));
            let l = &layers[slot];
            targets.entry(g).or_default().push(Target { p: t.map(|v| pos(m, v)), uv: t.map(|v| l.apply_uv(m.uv[v])), slot, island: find(&mut island, k), skin });
        }
        let frames = frames(&rig, on.into_iter());
        let own: [i32; SLOTS] = std::array::from_fn(|i| layers[i].texture);
        let layers: [esf::Layer; SLOTS] = layers.try_into().ok()?;
        Some(Reference { frames, targets, layers, own })
    }
}

/// A body's triangles that can wear armour.
struct Carried {
    tris: Vec<CarriedTri>,
}

/// A triangle (its vertices), its group, its level, and its three corners
/// carried onto the human.
struct CarriedTri {
    v: [usize; 3],
    group: Group,
    lod: Option<u8>,
    q: [Vec3; 3],
}

impl Carried {
    fn of(r: &Reference, model: &esf::Model, cs: &CSprite<Ee>) -> Option<Carried> {
        let mut rig = Rig::of_sprite(cs)?;
        rig.neck();
        // the triangles that can wear armour: on an opaque textured material
        // (not a wing's translucent membrane, not a painted eye)
        let wears = |b: &esf::Batch| b.layers.first().is_some_and(|l| l.blend == 0 && l.texture != 0);
        let worn: Vec<([usize; 3], Option<u8>)> = model.batches.iter().filter(|b| wears(b)).flat_map(|b| b.indices.chunks_exact(3).map(|t| ([t[0] as usize, t[1] as usize, t[2] as usize], b.lod))).collect();
        let verts: Vec<usize> = {
            let mut v: Vec<usize> = worn.iter().flat_map(|(t, _)| *t).collect();
            v.sort_unstable();
            v.dedup();
            v
        };
        // what hugs the torso is the torso's (its girth first, from the
        // vertices on the spine)
        let body = frames(&rig, verts.iter().filter(|&&v| part_of(model, &rig, v) == Part::Body).map(|&v| (pos(model, v), Seg::Body)))[&Seg::Body];
        let on: Vec<(Vec3, usize)> = verts.iter().filter_map(|&v| bone_of(model, v).map(|b| (pos(model, v), b))).collect();
        rig.fold(&body, &on);
        let frames = frames(&rig, verts.iter().filter_map(|&v| part_of(model, &rig, v).seg().map(|s| (pos(model, v), s))));
        // a corner carried onto the human: along its own segment when that
        // is in the triangle's group, else along the group's segment nearest
        let carry = |v: usize, g: Group| -> Option<Vec3> {
            let p = pos(model, v);
            let s = part_of(model, &rig, v).seg().filter(|s| s.group() == g).or_else(|| {
                let d = |s: &Seg| frames.get(s).map_or(f32::MAX, |f| seg_distance(f, p));
                rig.segs.keys().copied().filter(|s| s.group() == g).min_by(|a, b| d(a).total_cmp(&d(b)))
            })?;
            Some(r.frames.get(&s)?.world(frames.get(&s)?.local(p)))
        };
        let tris = worn
            .into_iter()
            .filter_map(|(t, lod)| {
                let g = group_of(model, &rig, t)?;
                Some(CarriedTri { v: t, group: g, lod, q: [carry(t[0], g)?, carry(t[1], g)?, carry(t[2], g)?] })
            })
            .collect();
        Some(Carried { tris })
    }
}

/// A body's carried triangles fitted to the human's surface as it shows in
/// a look (`alpha`: the see-through texels of each slot's art there): each
/// takes the slot of the shown triangle nearest its middle, and texture
/// coordinates from the nearest shown triangles of that one's UV island.
/// With `robed`, a leg's triangles are fitted to what a human in the robe
/// shows round its legs: above the skirt's hem the skirt (the legs slot's
/// triangles no skin shows on: the legs are inside it), below it the legs
/// and feet a knee-length robe leaves out, or the skirt.
fn fit(r: &Reference, c: &Carried, alpha: &[Option<Rc<Alpha>>; SLOTS], robed: bool) -> Fit {
    let shown: HashMap<Group, Vec<&Target>> = r.targets.iter().map(|(&g, ts)| (g, ts.iter().filter(|t| t.skin || shows(&alpha[t.slot], &t.uv)).collect())).collect();
    let skirt: Vec<&Target> = if robed { r.targets.values().flatten().filter(|t| t.slot == 3 && !t.skin && shows(&alpha[3], &t.uv)).collect() } else { Vec::new() };
    let hem = hem(&skirt);
    let mid = |t: &Target| (t.p[0] + t.p[1] + t.p[2]) / 3.0;
    // under the hem, per leg: what shows there, and the skirt
    let below: HashMap<Group, Vec<&Target>> = shown
        .iter()
        .filter(|(g, _)| matches!(g, Group::Limb(Limb::Leg(_))))
        .map(|(&g, ts)| (g, ts.iter().copied().filter(|t| mid(t).y < hem).chain(skirt.iter().copied()).collect()))
        .collect();
    let mut out: [Vec<Tri>; SLOTS] = Default::default();
    for CarriedTri { v: t, group: g, lod, q } in &c.tris {
        let c = (q[0] + q[1] + q[2]) / 3.0;
        let targets = match g {
            Group::Limb(Limb::Leg(_)) if !skirt.is_empty() => {
                if c.y >= hem {
                    &skirt
                } else {
                    &below[g]
                }
            }
            _ => match shown.get(g).filter(|ts| !ts.is_empty()) {
                Some(ts) => ts,
                None => continue,
            },
        };
        let nearest = |p: Vec3, island: Option<usize>| {
            targets
                .iter()
                .filter(|x| island.is_none_or(|i| x.island == i))
                .map(|x| {
                    let (d, w) = closest(p, &x.p);
                    (d, w, *x)
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))
        };
        let Some((_, _, best)) = nearest(c, None) else { continue };
        let mut uv = [[0.0; 2]; 3];
        for k in 0..3 {
            let (_, w, x) = nearest(q[k], Some(best.island)).expect("the best target's island holds it");
            uv[k] = [0, 1].map(|j| x.uv[0][j] * w[0] + x.uv[1][j] * w[1] + x.uv[2][j] * w[2]);
        }
        out[best.slot].push(Tri { v: t.map(|v| v as u32), uv, lod: *lod });
    }
    Fit { tris: out, layers: r.layers.clone() }
}

/// Where a skirt stops hiding the legs: going down from its top, the first
/// height at which less than half the way round the body (in sixteenths,
/// round the human's middle) has a shown triangle of it (a knee-length robe's
/// panels hang lower, but a human's legs show between them).
fn hem(skirt: &[&Target]) -> f32 {
    let (lo, hi) = skirt.iter().flat_map(|t| t.p.iter().map(|p| p.y)).fold((f32::MAX, f32::MIN), |(lo, hi), y| (lo.min(y), hi.max(y)));
    if skirt.is_empty() {
        return lo;
    }
    let spans: Vec<(f32, f32, usize)> = skirt
        .iter()
        .map(|t| {
            let m = (t.p[0] + t.p[1] + t.p[2]) / 3.0;
            let a = (m.x.atan2(m.z) / std::f32::consts::TAU + 0.5) * 16.0;
            let (y0, y1) = t.p.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.y), b.max(p.y)));
            (y0, y1, (a as usize).min(15))
        })
        .collect();
    let mut h = hi;
    while h > lo {
        let mut round = [false; 16];
        for &(y0, y1, a) in &spans {
            if y0 <= h && h <= y1 {
                round[a] = true;
            }
        }
        if round.iter().filter(|&&b| b).count() < 8 {
            return h;
        }
        h -= 0.01;
    }
    lo
}

/// How far `p` is from a segment's bone (its ends included).
fn seg_distance(f: &Frame, p: Vec3) -> f32 {
    let t = (p - f.a).dot(f.d).clamp(0.0, f.len);
    p.distance(f.a + f.d * t)
}

/// For a look at the fit (`dressview`'s pictures): each triangle of each of
/// `model`'s batches, where it is fitted: 0 the body, 1 and 2 the arms, 3
/// and 4 the legs, 5 the head, 6 a part that wears nothing (translucent,
/// untextured), 7 something off the spine. None for a body this cannot fit.
pub fn debug_groups(model: &esf::Model, cs: &CSprite<Ee>) -> Option<Vec<Vec<u8>>> {
    let mut rig = Rig::of_sprite(cs)?;
    rig.neck();
    let wears = |b: &esf::Batch| b.layers.first().is_some_and(|l| l.blend == 0 && l.texture != 0);
    let verts: Vec<usize> = model.batches.iter().filter(|b| wears(b)).flat_map(|b| b.indices.iter().map(|&v| v as usize)).collect();
    let body = frames(&rig, verts.iter().filter(|&&v| part_of(model, &rig, v) == Part::Body).map(|&v| (pos(model, v), Seg::Body)))[&Seg::Body];
    let on: Vec<(Vec3, usize)> = verts.iter().filter_map(|&v| bone_of(model, v).map(|b| (pos(model, v), b))).collect();
    rig.fold(&body, &on);
    Some(
        model
            .batches
            .iter()
            .map(|b| {
                b.indices
                    .chunks_exact(3)
                    .map(|t| {
                        if !wears(b) {
                            return 6;
                        }
                        let t = [t[0] as usize, t[1] as usize, t[2] as usize];
                        match group_of(model, &rig, t) {
                            Some(Group::Body) => 0,
                            Some(Group::Limb(Limb::Arm(s))) => 1 + s,
                            Some(Group::Limb(Limb::Leg(s))) => 3 + s,
                            None if t.iter().filter(|&&v| part_of(model, &rig, v) == Part::Head).count() >= 2 => 5,
                            None => 7,
                        }
                    })
                    .collect()
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A human-shaped skeleton (the root, a spine of three, the head; two
    /// arms of collarbone, upper arm, forearm, hand and item node; two legs
    /// of thigh, shin, ankle and toe) and a tail off the root.
    fn skeleton() -> (Vec<i32>, Vec<Vec3>, [i32; 10]) {
        let mut parent = Vec::new();
        let mut at = Vec::new();
        let mut add = |p: i32, x: f32, y: f32, z: f32| {
            parent.push(p);
            at.push(Vec3::new(x, y, z));
            parent.len() as i32 - 1
        };
        let root = add(-1, 0.0, 1.0, 0.0);
        let s1 = add(root, 0.0, 1.2, 0.0);
        let chest = add(s1, 0.0, 1.4, 0.0);
        let head = add(chest, 0.0, 1.6, 0.0);
        let mut hands = [0; 2];
        for (k, sx) in [-1.0f32, 1.0].into_iter().enumerate() {
            let col = add(chest, 0.05 * sx, 1.4, 0.0);
            let upper = add(col, 0.15 * sx, 1.4, 0.0);
            let fore = add(upper, 0.4 * sx, 1.4, 0.0);
            let hand = add(fore, 0.65 * sx, 1.4, 0.0);
            hands[k] = add(hand, 0.7 * sx, 1.4, 0.0);
        }
        let mut toes = [0; 2];
        for (k, sx) in [-1.0f32, 1.0].into_iter().enumerate() {
            let thigh = add(root, 0.1 * sx, 0.95, 0.0);
            let shin = add(thigh, 0.1 * sx, 0.55, 0.0);
            let ankle = add(shin, 0.1 * sx, 0.1, 0.0);
            toes[k] = add(ankle, 0.1 * sx, 0.0, 0.12);
        }
        let tail = add(root, 0.0, 0.95, -0.2);
        add(tail, 0.0, 0.9, -0.6);
        (parent, at, [hands[0], hands[1], -1, chest, -1, toes[0], toes[1], head, -1, -1])
    }

    /// The parts as the NodeIDList's anchors find them: the collarbones are
    /// the body's, the toes the feet's, the tail nobody's; and a list that
    /// does not name a hand finds nothing (the control).
    #[test]
    fn a_skeleton_reads_as_a_body() {
        let (parent, at, ids) = skeleton();
        let rig = Rig::of(&parent, &at, &ids).expect("a rig");
        assert_eq!(&rig.part[..4], &[Part::Body, Part::Body, Part::Body, Part::Head]);
        assert_eq!(rig.part[4], Part::Body, "a collarbone");
        assert_eq!(rig.part[5], Part::Limb(Limb::Arm(0), 0));
        assert_eq!(rig.part[6], Part::Limb(Limb::Arm(0), 1));
        assert_eq!(rig.part[7], Part::Limb(Limb::Arm(0), 2));
        assert_eq!(rig.part[8], Part::Limb(Limb::Arm(0), 2), "the item node is the hand's");
        let thigh = 14;
        assert_eq!(rig.part[thigh..thigh + 4], [Part::Limb(Limb::Leg(0), 0), Part::Limb(Limb::Leg(0), 1), Part::Limb(Limb::Leg(0), 2), Part::Limb(Limb::Leg(0), 2)]);
        assert_eq!(rig.part[22..], [Part::Other, Part::Other], "the tail");
        assert_eq!(rig.segs[&Seg::Body], (at[0], at[3]));
        assert_eq!(rig.segs[&Seg::Limb(Limb::Leg(0), 2)], (at[16], at[17]), "ankle to toe");
        let mut no_hand = ids;
        no_hand[1] = -1;
        assert!(Rig::of(&parent, &at, &no_hand).is_none());
        // the arms hang from the chest, right under the head: no neck
        let mut rig = rig;
        rig.neck();
        assert_eq!(rig.part[2], Part::Body);
    }

    /// A body on four legs is not read as one that wears armour: its
    /// "hands" (the forefeet) are on the ground (the control: the same
    /// skeleton with its hands raised is read).
    #[test]
    fn four_legs_wear_nothing() {
        let (parent, mut at, ids) = skeleton();
        // the arms hung down to the ground, the hands at the feet
        for (k, sx) in [(4usize, -1.0f32), (9, 1.0)] {
            for (j, y) in [1.3f32, 1.0, 0.5, 0.05, 0.0].into_iter().enumerate() {
                at[k + j] = Vec3::new(0.2 * sx, y, 0.5);
            }
        }
        assert!(Rig::of(&parent, &at, &ids).is_none());
        let (parent, at, ids) = skeleton();
        assert!(Rig::of(&parent, &at, &ids).is_some());
    }

    /// A neck between the shoulders and the head is the head's.
    #[test]
    fn a_neck_is_the_heads() {
        let (mut parent, mut at, mut ids) = skeleton();
        // a neck node between the chest (2) and the head (3)
        let neck = parent.len();
        parent.push(2);
        at.push(Vec3::new(0.0, 1.5, 0.0));
        parent[3] = neck as i32;
        ids[7] = 3;
        let mut rig = Rig::of(&parent, &at, &ids).expect("a rig");
        assert_eq!(rig.part[neck], Part::Body, "read from the skeleton alone, the spine's");
        rig.neck();
        assert_eq!(rig.part[neck], Part::Head);
        assert_eq!(rig.part[2], Part::Body, "the arms hang from the chest");
    }

    /// A hoofed leg (hip, knee forward, hock back, hoof) falls as a human's:
    /// the knee is the joint at 43% of the way down, the shin runs from it
    /// down to the hoof, past the hock; read from the bottom up instead, the
    /// hock would be the knee (the control: the hock is the shin's).
    #[test]
    fn a_hoofed_leg_falls_as_a_humans() {
        let (mut parent, mut at, mut ids) = skeleton();
        for s in 0..2usize {
            let sx = if s == 0 { -1.0 } else { 1.0 };
            let hip = parent.len();
            parent.extend([0, hip as i32, hip as i32 + 1, hip as i32 + 2]);
            at.extend([Vec3::new(0.13 * sx, 0.92, 0.0), Vec3::new(0.13 * sx, 0.56, 0.17), Vec3::new(0.13 * sx, 0.31, -0.07), Vec3::new(0.13 * sx, 0.06, -0.05)]);
            ids[5 + s] = hip as i32 + 3;
        }
        // the human legs left in place but not named: unassigned, off the root
        let rig = Rig::of(&parent, &at, &ids).expect("a rig");
        let first = parent.len() - 8;
        assert_eq!(rig.part[first], Part::Limb(Limb::Leg(0), 0), "the hip's bone is the thigh");
        assert_eq!(rig.part[first + 1], Part::Limb(Limb::Leg(0), 1), "the knee's the shin");
        assert_eq!(rig.part[first + 2], Part::Limb(Limb::Leg(0), 1), "the hock is the shin's");
        assert_eq!(rig.part[first + 3], Part::Limb(Limb::Leg(0), 2), "the hoof the foot");
    }

    /// Off the spine, a node whose skin hugs the torso becomes the torso's,
    /// and one whose skin reaches away (a tail) stays nobody's.
    #[test]
    fn what_hugs_the_torso_is_the_torsos() {
        let (parent, at, ids) = skeleton();
        let mut rig = Rig::of(&parent, &at, &ids).expect("a rig");
        let body = Frame::new(at[0], at[3]);
        // the torso's skin: 0.15 round the spine (on the root and the chest)
        let torso: Vec<(Vec3, usize)> = (0..12).map(|k| (Vec3::new(0.15 * (k as f32).cos(), 0.95 + 0.05 * k as f32, 0.15 * (k as f32).sin()), if k < 6 { 0 } else { 2 })).collect();
        // a tail's skin 0.2 to 0.56 behind the spine, down past the torso
        let tail: Vec<(Vec3, usize)> = (0..10).map(|k| (Vec3::new(0.0, 0.95 - 0.02 * k as f32, -0.2 - 0.04 * k as f32), 22 + (k >= 5) as usize)).collect();
        rig.fold(&body, &[torso.clone(), tail].concat());
        assert_eq!(rig.part[22], Part::Other);
        // the same node with its skin on the belly, under the torso's
        let belly: Vec<(Vec3, usize)> = (0..10).map(|k| (Vec3::new(0.01 * k as f32, 1.1, 0.14), 22)).collect();
        rig.fold(&body, &[torso, belly].concat());
        assert_eq!(rig.part[22..], [Part::Body, Part::Body]);
    }

    /// A frame carries a point to the same place on a segment twice as long
    /// and twice as thick; a point on the bone stays on it.
    #[test]
    fn frames_carry_along_and_round() {
        let mut a = Frame::new(Vec3::ZERO, Vec3::new(0.0, 1.0, 0.0));
        let mut b = Frame::new(Vec3::new(1.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 0.0));
        a.r = [0.1, 0.1];
        b.r = [0.2, 0.2];
        assert_eq!(a.e1, Vec3::Z, "toward the front");
        let p = Vec3::new(0.05, 0.5, 0.1);
        let q = b.world(a.local(p));
        assert!(q.distance(Vec3::new(1.1, 1.0, 0.2)) < 1e-5, "{q}");
        assert!(b.world(a.local(Vec3::new(0.0, 0.25, 0.0))).distance(Vec3::new(1.0, 0.5, 0.0)) < 1e-5);
        // a bone pointing forward is measured round from up
        assert_eq!(Frame::new(Vec3::ZERO, Vec3::Z).e1, Vec3::Y);
    }

    /// The closest point of a triangle, inside it and off a corner.
    #[test]
    fn closest_points() {
        let t = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let (d, w) = closest(Vec3::new(0.25, 0.25, 1.0), &t);
        assert!((d - 1.0).abs() < 1e-6 && (w[1] - 0.25).abs() < 1e-6 && (w[2] - 0.25).abs() < 1e-6);
        let (d, w) = closest(Vec3::new(2.0, -1.0, 0.0), &t);
        assert!((d - 2.0).abs() < 1e-6 && w == [0.0, 1.0, 0.0], "{d} {w:?}");
    }

    /// A triangle shows when most of its points are on opaque texels.
    #[test]
    fn see_through_texels() {
        // the left half opaque, the right see-through
        let a = Alpha { w: 4, h: 4, a: (0..16).map(|i| if i % 4 < 2 { 0x80 } else { 0 }).collect() };
        assert!(a.shows(&[[0.0, 0.0], [0.4, 0.0], [0.0, 0.9]]));
        assert!(!a.shows(&[[0.6, 0.0], [0.9, 0.0], [0.6, 0.9]]));
        // repeated past 1
        assert!(a.shows(&[[1.0, 1.0], [1.4, 1.0], [1.0, 1.9]]));
    }

    /// The bodies of the server's races with no text slot (the kappa,
    /// brownie, pixie, fairy, minotaur, dryad, skeleton, fungusman,
    /// gargoyle) and the hill giant.
    const NINE: [u32; 10] = [0x31A15F17, 0x4376EB52, 0x94BA77CD, 0x7A85646C, 0xC5CBF4CF, 0x70EFCE05, 0x6AF98A81, 0x4A4D88FB, 0xB4D2D132, 0xFFB1EA2C];

    /// The files a body needs off the disc, when it is there.
    fn disc() -> Option<Library> {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        }
        let mut d = iso::Iso::open(&iso).expect("the disc");
        let mut lib = Library::new();
        for f in ["CHARCUST.CSF", "CHARFACE.CSF", "CHAR.ESF"] {
            lib.add(f, esf::Container::open(d.read(f).expect(f)).expect(f));
        }
        Some(lib)
    }

    /// Two models alike to the bit: vertices, batches, layers, counts.
    fn same(a: &esf::Model, b: &esf::Model) -> bool {
        let bits2 = |v: &[[f32; 2]]| v.iter().flat_map(|p| p.map(f32::to_bits)).collect::<Vec<_>>();
        let bits3 = |v: &[[f32; 3]]| v.iter().flat_map(|p| p.map(f32::to_bits)).collect::<Vec<_>>();
        let bits4 = |v: &[[f32; 4]]| v.iter().flat_map(|p| p.map(f32::to_bits)).collect::<Vec<_>>();
        let batches = |m: &esf::Model| m.batches.iter().map(|b| (b.material, b.layers.clone(), b.indices.clone(), b.lod)).collect::<Vec<_>>();
        bits3(&a.pos) == bits3(&b.pos)
            && bits2(&a.uv) == bits2(&b.uv)
            && bits3(&a.normal) == bits3(&b.normal)
            && bits4(&a.color) == bits4(&b.color)
            && a.bones == b.bones
            && a.weights == b.weights
            && batches(a) == batches(b)
            && (a.parts, a.triangles, a.skinned, a.missing) == (b.parts, b.triangles, b.skinned, b.missing)
            && a.lod_dists.iter().map(|d| d.to_bits()).eq(b.lod_dists.iter().map(|d| d.to_bits()))
    }

    /// Every CSprite on the disc dressed by `play::dress_look` against the
    /// path before it (`apply_look_with`'s changes put on by `play::dress`),
    /// in looks of every kind: a body with any text slot comes out byte for
    /// byte as before in all of them (it is never fitted); a slotless one as
    /// before in a look with no art, and in one with art it only gains the
    /// made-up batches after its own, its own batches and vertices as they
    /// were. The nine and the hill giant are fitted in every look, and each
    /// gains armour in a set; the other slotless bodies in plate. The control:
    /// with the dressing off, a slotless body in plate is as before.
    #[test]
    fn only_slotless_bodies_change() {
        let Some(lib) = disc() else { return };
        let c = lib.file(lib.file_named("CHAR.ESF").expect("CHAR.ESF"));
        let mut ids: Vec<i32> = c.dict.iter().filter(|(_, n)| c.node(**n).ty == 0x2700).map(|(&k, _)| k).collect();
        ids.sort_unstable();
        let mut looks: Vec<Look> = vec![Look { robe: -1, ..Default::default() }];
        looks.extend((1..9u8).map(|s| Look { armor: [s; 6], armor_tint: [s; 6], hair: [s % 8, 2, 3], face: s % 4, robe: -1, ..Default::default() }));
        looks.extend((0..4i8).map(|r| Look { armor: [4, 3, 2, 1, 5, 6], robe: r, robe_tint: 3, ..Default::default() }));
        looks.push(Look { armor: [4; 6], armor_color: Some([[0x30, 0x70, 0xD0, 255]; 6]), robe: -1, ..Default::default() });
        let t = Tables::retail();
        let has = |id: u32| lib.surface(id as i32).is_some();
        let (mut slotted, mut slotless, mut dressed) = (0, 0, Vec::new());
        for id in ids {
            let r = lib.lookup(id).expect("a CSprite");
            let Some(model) = lib.build_model(r) else { continue };
            let mut anims = vi::hsprite::AnimScene::default();
            let Some(cs) = vi::character::load_csprite(&lib, r, &mut anims, &mut vi::character::AnimCache::new(), &mut Rand(1)) else { continue };
            let nine = NINE.contains(&(id as u32));
            let tried: Vec<&Look> = if !is_slotless(&cs) || nine { looks.iter().collect() } else { vec![&looks[0], &looks[4]] };
            let mut gained = false;
            for (k, look) in tried.iter().enumerate() {
                let helm = k % 3 == 1;
                let mut before = model.clone();
                crate::play::dress(&mut before, &vi::cust::apply_look_with(&cs, look, helm, has, &t));
                let mut after = model.clone();
                crate::play::dress_look(&lib, id, &mut after, &cs, look, helm, &t);
                if !is_slotless(&cs) {
                    assert!(same(&before, &after), "{id:#010x} has text slots and changed in {look:?}");
                    continue;
                }
                assert!(same(&before, &model), "a slotless body has nothing of its own to dress");
                let n = before.batches.len();
                assert_eq!(after.batches[..n].len(), n);
                let mut own = after.clone();
                own.batches.truncate(n);
                own.pos.truncate(before.pos.len());
                own.uv.truncate(before.uv.len());
                own.normal.truncate(before.normal.len());
                own.color.truncate(before.color.len());
                own.bones.truncate(before.bones.len());
                own.weights.truncate(before.weights.len());
                own.triangles = before.triangles;
                assert!(same(&before, &own), "{id:#010x}: its own batches and vertices as they were");
                assert!(after.batches[n..].iter().all(|b| (ENTRY..ENTRY + SLOTS as i32).contains(&b.material)));
                if look.armor[..5].iter().all(|&s| s == 0) && look.robe < 0 {
                    assert!(same(&before, &after), "{id:#010x}: no art, no change");
                }
                gained |= after.batches.len() > n;
            }
            if is_slotless(&cs) {
                slotless += 1;
                if gained {
                    dressed.push(id as u32);
                }
                if nine {
                    assert!(gained, "{id:#010x} wears nothing");
                }
            } else {
                slotted += 1;
            }
        }
        eprintln!("{slotted} bodies with text slots unchanged; {slotless} without, {} of them dressed: {:08X?}", dressed.len(), dressed);
        assert!(slotted > 100 && dressed.len() >= NINE.len());
        // the control: turned off, the kappa in plate is as it was
        set_enabled(false);
        let r = lib.lookup(NINE[0] as i32).unwrap();
        let model = lib.build_model(r).unwrap();
        let cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
        let mut off = model.clone();
        crate::play::dress_look(&lib, NINE[0] as i32, &mut off, &cs, &looks[4], false, &t);
        set_enabled(true);
        assert!(same(&model, &off));
    }

    /// The kappa fitted to the human: its torso, arms, legs and feet take
    /// the chest, bracer, gloves, legs and boots slots, its head none; in
    /// plate the chest is the slot with the most triangles. The fit is made
    /// once per look (the same `Rc` again).
    #[test]
    fn the_kappa_wears_each_slot() {
        let Some(lib) = disc() else { return };
        let id = NINE[0] as i32;
        let r = lib.lookup(id).unwrap();
        let model = lib.build_model(r).unwrap();
        let cs = vi::character::load_csprite(&lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::character::AnimCache::new(), &mut Rand(1)).unwrap();
        let t = Tables::retail();
        let has = |id: u32| lib.surface(id as i32).is_some();
        let look = Look { armor: [4; 6], robe: -1, ..Default::default() };
        let art = worn(&vi::cust::apply_look_slots(&slots(), cs.race, cs.sex, &look, false, has, &t)).map(|(t, _)| t);
        let fit = fit_of(&lib, id, &model, &cs, art, false).expect("a fit");
        let n: Vec<usize> = fit.tris.iter().map(Vec::len).collect();
        assert!(n.iter().all(|&k| k > 0), "{n:?}");
        assert!(n[0] > n[1] && n[0] > n[2] && n[0] > n[4], "{n:?}");
        assert!(Rc::ptr_eq(&fit, &fit_of(&lib, id, &model, &cs, art, false).unwrap()));
        // its head and neck keep their own look (no fitted triangle has two
        // corners on them: the six at the collar, in the head's material, sit
        // on the spine), and its torso's palette entry wears armour
        let mut rig = Rig::of_sprite(&cs).unwrap();
        rig.neck();
        let on_head = |v: u32| bone_of(&model, v as usize).is_some_and(|b| rig.part[b] == Part::Head);
        assert!(fit.tris.iter().flatten().all(|t| t.v.iter().filter(|&&v| on_head(v)).count() < 2));
        let of = |e: i32| model.batches.iter().filter(|b| b.material == e).flat_map(|b| b.indices.iter().copied()).collect::<std::collections::HashSet<u32>>();
        let (head, torso) = ([of(2), of(3)].concat_sets(), of(0));
        let in_head = fit.tris.iter().flatten().filter(|t| t.v.iter().all(|v| head.contains(v))).count();
        assert!(in_head <= 6, "{in_head} of the head's triangles fitted");
        assert!(fit.tris.iter().flatten().any(|t| t.v.iter().all(|v| torso.contains(v))));
        // the control: read without the neck rule, the head's material wears more
        assert!(Rig::of_sprite(&cs).unwrap().part[4] == Part::Body && rig.part[4] == Part::Head);
    }

    trait ConcatSets {
        fn concat_sets(self) -> std::collections::HashSet<u32>;
    }

    impl<const N: usize> ConcatSets for [std::collections::HashSet<u32>; N] {
        fn concat_sets(self) -> std::collections::HashSet<u32> {
            self.into_iter().flatten().collect()
        }
    }

    /// The made-up slots dress five entries of their own, nothing else, and
    /// a slot without art adds no batch (the control: with art it does).
    #[test]
    fn made_up_slots() {
        let s = slots();
        assert_eq!(&s[..5], &[(ENTRY, 0), (ENTRY + 1, 0), (ENTRY + 2, 0), (ENTRY + 3, 0), (ENTRY + 4, 0)]);
        assert!(s[5..].iter().all(|&e| e == (-1, -1)));
        let mut m = esf::Model::default();
        for k in 0..3 {
            m.pos.push([k as f32, 0.0, 0.0]);
            m.uv.push([0.0; 2]);
            m.normal.push([0.0, 1.0, 0.0]);
            m.color.push([0.0, 0.0, 0.0, 1.0]);
            m.bones.push([0, 255, 255, 255]);
            m.weights.push([127, 0, 0, 0]);
        }
        let layer = esf::Layer { flags: 0, texture: 7, wrap: 0, blend: 0, color: [255; 4], uv_transform: [0.0; 9], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
        let mut tris: [Vec<Tri>; SLOTS] = Default::default();
        tris[3].push(Tri { v: [0, 1, 2], uv: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]], lod: None });
        let fit = Fit { tris, layers: std::array::from_fn(|_| layer.clone()) };
        let none = [LayerChange { entry: ENTRY + 3, layer: 0, texture: None, color: Some([1, 2, 3, 255]) }];
        assert!(!apply(&mut m, &fit, &none));
        assert!(m.batches.is_empty() && m.pos.len() == 3);
        let art = [LayerChange { entry: ENTRY + 3, layer: 0, texture: Some(0xABCD), color: Some([1, 2, 3, 255]) }];
        assert!(apply(&mut m, &fit, &art));
        assert_eq!(m.batches.len(), 1);
        let b = &m.batches[0];
        assert_eq!((b.material, b.layers[0].texture, b.layers[0].color, b.indices.clone()), (ENTRY + 3, 0xABCD, [1, 2, 3, 255], vec![3, 4, 5]));
        assert_eq!(m.uv[3..], [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
        assert_eq!(m.pos[4], m.pos[1]);
    }
}
