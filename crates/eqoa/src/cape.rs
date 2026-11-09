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
//! Then it is settled on its wearer, so that what the wearer wears (plate,
//! a robe's skirt, broad shoulders) does not show through it as it stands,
//! walks and runs (`settle`; the owner, trying them: "they may need to sit
//! back off the armor a tad. the armor is clipping through the cape"). Where
//! it lies close over the back, the neck and the tops of the shoulders it is
//! skinned as the body right under it is, and below the hips it swings as the
//! robe's tail under it does (`follow`), so the two move as one; then the
//! wearer's own stand, walk and run are played and wherever the body still
//! comes through it, or within a little of it (a broad back, an arm swung
//! back), it is moved out from the body that far (`sway`): only there, and
//! never more than 8% of the body's height to the shoulders, so where nothing
//! comes near it it rests where it did.
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
    /// The bodies' skin palettes in the poses a cape is fitted against
    /// (`palettes`), by wearer and art: one walk and run played a body.
    static POSED: std::cell::RefCell<HashMap<(u32, Era), Rc<Vec<Vec<glam::Mat4>>>>> = Default::default();
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
/// and art (a body's mesh is built here, for its measures, only when a cape
/// is first fitted to it, and its walk and run played once a body).
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
            let poses = match POSED.with(|c| c.borrow().get(&(wearer, era)).cloned()) {
                Some(p) => p,
                None => {
                    let p = Rc::new(lib.with_view(era, || palettes(lib, wearer)));
                    POSED.with(|c| c.borrow_mut().insert((wearer, era), p.clone()));
                    p
                }
            };
            let f = fit(&src, &Bones::of(cs), &body, &poses).map(Rc::new);
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
/// broader than the cape's own a little wider and further back (`spread`);
/// then settled on the body, posed as `poses` (skin palettes: `palettes`), so
/// no part of it shows through (`settle`). `EQOA_CAPE_DEBUG=1` says which way
/// and by what measures.
fn fit(src: &Source, to: &Bones, body: &esf::Model, poses: &[Vec<glam::Mat4>]) -> Option<esf::Model> {
    let debug = std::env::var_os("EQOA_CAPE_DEBUG").is_some();
    let into = Frame::of(to, body);
    let mut m = placed(src, to, into.as_ref(), debug)?;
    if let Some(f) = &into {
        settle(&mut m, to, f, body, poses, debug);
    }
    Some(m)
}

/// The poses a cape is fitted against (`settle`): standing, and the walk and
/// the run (`SetLocomotion` 0, `SetAction` 1 and 3) a frame every 80
/// milliseconds through their first second, as (action, milliseconds in).
const POSES: &[(Option<i32>, &[i64])] = &[
    (None, &[1500]),
    (Some(1), &[80, 160, 240, 320, 400, 480, 560, 640, 720, 800, 880, 960, 1040]),
    (Some(3), &[80, 160, 240, 320, 400, 480, 560, 640, 720, 800, 880, 960, 1040]),
];

/// The body `wearer`'s skin palettes (a matrix a node, bind pose to posed)
/// in `POSES`, played on its own animations; none when its sprite does not
/// load.
fn palettes(lib: &Library, wearer: u32) -> Vec<Vec<glam::Mat4>> {
    let mut out = Vec::new();
    for &(action, times) in POSES {
        let Some(r) = lib.lookup(wearer as i32) else { return out };
        let (mut anims, mut rand) = (AnimScene::<Ee>::default(), Rand(1));
        let Some(mut cs) = load_csprite::<Ee>(lib, r, &mut anims, &mut AnimCache::new(), &mut rand) else { return out };
        let mut t = 0;
        for &ms in times {
            while t <= ms {
                anims.time = t;
                match action {
                    Some(a) if t == 0 => {
                        cs.set_action(a, &anims, &mut rand);
                    }
                    Some(_) => {}
                    None => cs.set_locomotion(0, &anims, &mut rand),
                }
                cs.h.process(0, &anims);
                t += 16;
            }
            cs.h.set_blend_matrices();
            out.push(cs.h.blend.iter().map(|b| glam::Mat4::from_cols_array_2d(&b.to_cols())).collect());
        }
    }
    out
}

/// `src` on the skeleton `to` (frame `into`), before it is settled on the
/// body (`fit`).
fn placed(src: &Source, to: &Bones, into: Option<&Frame>, debug: bool) -> Option<esf::Model> {
    if src.bones.same(to) {
        if debug {
            eprintln!("cape: on its own skeleton");
        }
        return Some(src.model.clone());
    }
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
            let (scale, dz) = match (from, into) {
                (Some(f), Some(i)) => spread(f, i, &src.bones, to, &map, true),
                _ => (Vec3::ONE, 0.0),
            };
            (map, anchor, scale, dz)
        }
        None => {
            let (f, i) = (from?, into?);
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

/// How near the body a cape's place moves wholly as the body under it does
/// (`follow`), and how far off it still does in part, of the body's height to
/// the shoulders.
const FOLLOW: (f32, f32) = (0.15, 0.35);
/// How far off the body a cape sits at the least where the body would come
/// near it (`sway`), how far outside the cape a part of the body may lie and
/// still push it out (further, a tail or a hand swung back, which no cape
/// covers), and the most it is pushed, of the same height.
const SWAY: (f32, f32, f32) = (0.02, 0.12, 0.08);

/// A cape fitted to the body `body` (skeleton `to`, frame `f`) put where the
/// body does not come through it: where it lies close over the back, the
/// neck and the shoulders skinned as the body under it is, and below the
/// hips swinging as a robe's tail does (`follow`), so the two move as one;
/// then moved out from the body as far as the body, posed as `poses`, still
/// comes through it (`sway`): as far as a broad back's shoulders or an arm
/// swung back reach, and only there. Neither looks at the head nor at the
/// arms from the elbows down, which swing free of a cape.
fn settle(m: &mut esf::Model, to: &Bones, f: &Frame, body: &esf::Model, poses: &[Vec<glam::Mat4>], debug: bool) {
    let tall = f.shoulder_y - f.ground;
    let weld = Weld::of(m);
    let b = |i: usize| to.bind[i];
    let axis = [Vec3::new(b(f.root).x, f.ground, b(f.root).z), b(f.root), b(f.chest), b(f.head)];
    let dir: Vec<Vec3> = weld.members.iter().map(|vs| outward(&axis, f.half_width, Vec3::from(m.pos[vs[0]]))).collect();
    let under = surface(body, &loose(f, to, true, 3));
    // the art's own hold on the arms and legs is kept: a place it hangs on
    // an arm rides it, and the legs carry the hem as they swing
    let kept = below(to, &f.arms.iter().map(|a| a[a.len() - 3]).chain(f.legs.iter().filter_map(|l| l.first().copied())).collect::<Vec<_>>());
    // a robe's tail: the nodes off the spine that are no limb's nor the head's
    // (the two back flaps every player skeleton has)
    let mut limbs: Vec<usize> = vec![f.head];
    limbs.extend(f.arms.iter().map(|a| a[0]).chain(f.legs.iter().filter_map(|l| l.first().copied())));
    let off = below(to, &limbs);
    let mut spine = vec![false; to.parents.len()];
    let mut k = f.head;
    while let Some(p) = usize::try_from(to.parents[k]).ok().filter(|&p| p < spine.len()) {
        spine[p] = true;
        k = p;
    }
    let tail: Vec<bool> = (0..spine.len()).map(|i| !off[i] && !spine[i]).collect();
    let followed = follow(m, &weld, &dir, &under, &kept, &tail, b(f.root).y, SWAY.1 * tall, FOLLOW.0 * tall, FOLLOW.1 * tall);
    let skip = loose(f, to, false, 2);
    let pts: Vec<(Vec3, [u8; 4], [u8; 4])> = used(body)
        .filter(|&v| !skip.get(heaviest(body, v)).copied().unwrap_or(true))
        .map(|v| (Vec3::from(body.pos[v]), body.bones[v], body.weights[v]))
        .collect();
    let nodes = [f.root, f.chest, f.head].map(|i| (i, to.bind[i]));
    let moved = sway(m, &weld, &dir, &pts, nodes, b(f.root).y - f.ground, f.half_width, poses, SWAY.0 * tall, SWAY.1 * tall, SWAY.2 * tall);
    if debug {
        eprintln!("cape: {followed} of {} places skinned as the body under them; moved out by up to {moved:.3} over {} poses", weld.members.len(), poses.len());
    }
}

/// The vertices of the body's nearest level of detail.
fn used(body: &esf::Model) -> impl Iterator<Item = usize> + '_ {
    let mut on = vec![false; body.pos.len()];
    for bt in body.batches.iter().filter(|bt| bt.lod.unwrap_or(0) == 0) {
        for &v in &bt.indices {
            on[v as usize] = true;
        }
    }
    (0..body.pos.len()).filter(move |&v| on[v] && v < body.bones.len() && v < body.weights.len())
}

/// The way out from the body at `c`: from the nearest point of the line
/// `axis` runs along (under the hips, the hips, the chest, the head), that
/// line widened to `half` either side as a torso is, so straight back behind
/// the back and round the sides beside it.
fn outward(axis: &[Vec3; 4], half: f32, c: Vec3) -> Vec3 {
    let mut best = (f32::MAX, Vec3::NEG_Z);
    for w in axis.windows(2) {
        let (a, d) = (w[0], w[1] - w[0]);
        let t = ((c - a).dot(d) / d.length_squared().max(1e-9)).clamp(0.0, 1.0);
        let mut r = c - (a + d * t);
        r.x = r.x.signum() * (r.x.abs() - half).max(0.0);
        if r.length() < best.0 {
            best = (r.length(), r);
        }
    }
    best.1.try_normalize().unwrap_or(Vec3::NEG_Z)
}

/// The nodes a cape is not fitted against: the head's, and each arm's from
/// its `arm`th node above the hand (3 the shoulder, 2 the elbow) down; with
/// `legs`, each leg's from the thigh down too.
fn loose(f: &Frame, b: &Bones, legs: bool, arm: usize) -> Vec<bool> {
    let mut roots = vec![f.head];
    roots.extend(f.arms.iter().map(|a| a[a.len() - arm]));
    if legs {
        roots.extend(f.legs.iter().filter_map(|l| l.first().copied()));
    }
    below(b, &roots)
}

/// The nodes `roots` and every node below them.
fn below(b: &Bones, roots: &[usize]) -> Vec<bool> {
    let n = b.parents.len();
    let mut out = vec![false; n];
    let mut stack = roots.to_vec();
    while let Some(i) = stack.pop() {
        if i < n && !out[i] {
            out[i] = true;
            stack.extend((0..n).filter(|&c| b.parents[c] == i as i32));
        }
    }
    out
}

/// The node a body's vertex is skinned to most (255 for none).
fn heaviest(body: &esf::Model, v: usize) -> usize {
    let (b, w) = (body.bones.get(v).copied().unwrap_or([255; 4]), body.weights.get(v).copied().unwrap_or([0; 4]));
    (0..4).filter(|&k| b[k] != 255 && w[k] > 0).max_by_key(|&k| w[k]).map_or(255, |k| b[k] as usize)
}

/// A skin as a list of (node, share), the shares summing to one.
type Skin = Vec<(u8, f32)>;

fn skin_of(b: [u8; 4], w: [u8; 4]) -> Skin {
    let sum: f32 = (0..4).filter(|&k| b[k] != 255).map(|k| w[k] as f32).sum();
    (0..4).filter(|&k| b[k] != 255 && w[k] > 0 && sum > 0.0).map(|k| (b[k], w[k] as f32 / sum)).collect()
}

/// `a` and `b`, each counting as its share of `parts`.
fn mix(parts: &[(&Skin, f32)]) -> Skin {
    let mut out: Skin = Vec::new();
    for (s, a) in parts {
        for &(b, w) in s.iter() {
            match out.iter_mut().find(|x| x.0 == b) {
                Some(x) => x.1 += w * a,
                None => out.push((b, w * a)),
            }
        }
    }
    out
}

/// A skin as a vertex's four nodes and weights: its heaviest four, summing
/// to 127.
fn pack(mut s: Skin) -> ([u8; 4], [u8; 4]) {
    s.sort_by(|a, b| b.1.total_cmp(&a.1));
    s.truncate(4);
    let kept: f32 = s.iter().map(|x| x.1).sum();
    let (mut bones, mut weights) = ([255u8; 4], [0u8; 4]);
    for (k, &(b, w)) in s.iter().enumerate() {
        bones[k] = b;
        weights[k] = (w / kept.max(1e-9) * 127.0).round() as u8;
    }
    // the rounding's remainder on the heaviest, so they sum to 127
    let got: i32 = weights.iter().map(|&w| w as i32).sum();
    weights[0] = (weights[0] as i32 + 127 - got).clamp(0, 127) as u8;
    (bones, weights)
}

/// The body's triangles of its nearest level of detail that a cape lies on
/// (two of their corners skinned most to none of `skip`'s nodes), with
/// their corners' skins.
fn surface(body: &esf::Model, skip: &[bool]) -> Vec<([Vec3; 3], [Skin; 3])> {
    body.batches
        .iter()
        .filter(|bt| bt.lod.unwrap_or(0) == 0)
        .flat_map(|bt| bt.indices.chunks_exact(3))
        .filter(|t| t.iter().filter(|&&v| !skip.get(heaviest(body, v as usize)).copied().unwrap_or(true)).count() >= 2)
        .map(|t| {
            let v = [0, 1, 2].map(|k| t[k] as usize);
            let at = v.map(|i| Vec3::from(body.pos[i]));
            let skin = v.map(|i| skin_of(body.bones.get(i).copied().unwrap_or([255; 4]), body.weights.get(i).copied().unwrap_or([0; 4])));
            (at, skin)
        })
        .collect()
}

/// A cape's vertices by place: those at one place (a seam's, the two sides
/// of a cape drawn both ways) are one, so all of them move alike; and which
/// places a triangle joins.
struct Weld {
    /// Each place's vertices.
    members: Vec<Vec<usize>>,
    /// Each place's neighbours.
    next: Vec<Vec<usize>>,
    /// The triangles, by place (none whose corners share a place).
    tris: Vec<[usize; 3]>,
}

impl Weld {
    fn of(m: &esf::Model) -> Weld {
        let mut at: HashMap<[i32; 3], usize> = HashMap::new();
        let mut members: Vec<Vec<usize>> = Vec::new();
        let of: Vec<usize> = m
            .pos
            .iter()
            .enumerate()
            .map(|(v, p)| {
                let key = p.map(|c| (c * 4096.0).round() as i32);
                let k = *at.entry(key).or_insert_with(|| {
                    members.push(Vec::new());
                    members.len() - 1
                });
                members[k].push(v);
                k
            })
            .collect();
        let mut next = vec![Vec::new(); members.len()];
        let mut tris = Vec::new();
        for t in m.batches.iter().flat_map(|b| b.indices.chunks_exact(3)) {
            let p = [0, 1, 2].map(|k| of[t[k] as usize]);
            if p[0] == p[1] || p[1] == p[2] || p[2] == p[0] {
                continue;
            }
            tris.push(p);
            for (x, y) in [(p[0], p[1]), (p[1], p[2]), (p[2], p[0]), (p[1], p[0]), (p[2], p[1]), (p[0], p[2])] {
                if !next[x].contains(&y) {
                    next[x].push(y);
                }
            }
        }
        Weld { members, next, tris }
    }
}

/// Each place of the cape `m` skinned as the body is right under it: where
/// the line it moves out along (`dir`) first meets the body's surface
/// (`under`), coming in from `reach` outside it, so the outermost skin there
/// (the back over the spine, the neck, the tops of the shoulders); wholly
/// where that is within `near` of it, in part out to `far`; its own share on
/// the nodes `kept` (the arms', the legs') kept as it was. Below the hips
/// (`hips`, a height) only its share on a robe's tail (`tail`'s nodes, the
/// back flaps) is shared among them as the body's under it is, so it swings
/// as the robe's hem does while the legs carry it as the art has them. A
/// place above the hips over none of the body (beside it) is blended toward
/// its neighbours' (twice over), so the cape does not tear where one gives
/// way to the other. How many places above the hips were over the body.
#[allow(clippy::too_many_arguments)]
fn follow(m: &mut esf::Model, weld: &Weld, dir: &[Vec3], under: &[([Vec3; 3], [Skin; 3])], kept: &[bool], tail: &[bool], hips: f32, reach: f32, near: f32, far: f32) -> usize {
    let own: Vec<Skin> = weld.members.iter().map(|vs| skin_of(m.bones[vs[0]], m.weights[vs[0]])).collect();
    let mut skins = own.clone();
    let mut over = vec![false; own.len()];
    for (k, vs) in weld.members.iter().enumerate() {
        let low = m.pos[vs[0]][1] < hips;
        let (o, d) = (Vec3::from(m.pos[vs[0]]) + dir[k] * reach, -dir[k]);
        // the nearest crossing of the line with a triangle (Moller-Trumbore)
        let mut hit: Option<(f32, usize, f32, f32)> = None;
        for (i, (c, _)) in under.iter().enumerate() {
            let (e1, e2) = (c[1] - c[0], c[2] - c[0]);
            let p = d.cross(e2);
            let det = e1.dot(p);
            if det.abs() < 1e-9 {
                continue;
            }
            let tv = o - c[0];
            let u = tv.dot(p) / det;
            let q = tv.cross(e1);
            let v = d.dot(q) / det;
            let t = e2.dot(q) / det;
            if u < 0.0 || v < 0.0 || u + v > 1.0 || t < 0.0 || t > reach + far {
                continue;
            }
            if hit.is_none_or(|h| t < h.0) {
                hit = Some((t, i, u, v));
            }
        }
        let Some((t, i, u, v)) = hit else { continue };
        let s = ((far - (t - reach).max(0.0)) / (far - near)).clamp(0.0, 1.0);
        let sk = &under[i].1;
        let at = mix(&[(&sk[0], 1.0 - u - v), (&sk[1], u), (&sk[2], v)]);
        if low {
            // the art's share on the tail, shared among its nodes as the
            // body's there is
            let on = |x: &(u8, f32)| tail.get(x.0 as usize).copied().unwrap_or(false);
            let (mine, theirs): (f32, f32) = (own[k].iter().filter(|x| on(x)).map(|x| x.1).sum(), at.iter().filter(|x| on(x)).map(|x| x.1).sum());
            if mine > 0.0 && theirs > 0.0 {
                let tailed: Skin = own[k].iter().filter(|x| !on(x)).copied().chain(at.iter().filter(|x| on(x)).map(|&(b, w)| (b, w / theirs * mine))).collect();
                skins[k] = mix(&[(&own[k], 1.0 - s), (&tailed, s)]);
            }
            continue;
        }
        skins[k] = keep(&own[k], &mix(&[(&own[k], 1.0 - s), (&at, s)]), kept);
        over[k] = true;
    }
    let followed = over.iter().filter(|&&o| o).count();
    // beside the body: toward the neighbours a ring at a time (below the
    // hips the art's own, as it hangs from the robe's tail and the legs)
    let mut done: Vec<bool> = over.iter().zip(&weld.members).map(|(&o, vs)| o || m.pos[vs[0]][1] < hips).collect();
    for share in [0.5, 0.25] {
        let was = skins.clone();
        let mut ring = Vec::new();
        for k in (0..own.len()).filter(|&k| !done[k]) {
            let by: Vec<usize> = weld.next[k].iter().copied().filter(|&j| done[j]).collect();
            if by.is_empty() {
                continue;
            }
            let parts: Vec<(&Skin, f32)> = by.iter().map(|&j| (&was[j], share / by.len() as f32)).chain(std::iter::once((&own[k], 1.0 - share))).collect();
            skins[k] = keep(&own[k], &mix(&parts), kept);
            ring.push(k);
        }
        for k in ring {
            done[k] = true;
        }
    }
    for (k, vs) in weld.members.iter().enumerate() {
        let (b, w) = pack(skins[k].clone());
        for &v in vs {
            m.bones[v] = b;
            m.weights[v] = w;
        }
    }
    followed
}

/// `new` with `own`'s share on the nodes `kept` put back: `own`'s on them,
/// and `new`'s on the rest scaled to fill what is left.
fn keep(own: &Skin, new: &Skin, kept: &[bool]) -> Skin {
    let on = |x: &(u8, f32)| kept.get(x.0 as usize).copied().unwrap_or(false);
    let held: f32 = own.iter().filter(|x| on(x)).map(|x| x.1).sum();
    let rest: f32 = new.iter().filter(|x| !on(x)).map(|x| x.1).sum();
    if held <= 0.0 || rest <= 0.0 {
        return if rest > 0.0 { new.iter().filter(|x| !on(x)).copied().collect() } else { own.clone() };
    }
    own.iter().filter(|x| on(x)).copied().chain(new.iter().filter(|x| !on(x)).map(|&(b, w)| (b, w / rest * (1.0 - held)))).collect()
}

/// Each place of the cape `m` moved straight out from the body (`dir`) as far
/// as needed for it to lie at least `gap` outside every point of the body
/// (`pts`, with their skins) that comes under it in any of the poses `poses`
/// (skin palettes): each point under a triangle of the posed cape (on the
/// body's side of it, or outside it by no more than `reach`) moves the
/// triangle's corners out until it is `gap` inside it. Each place then moved
/// at least half as far as its neighbours (twice over), so the cape bends
/// where it moves, never creases; none further than `cap`. A posed
/// triangle's face is turned away from the posed spine (`nodes`: the root,
/// the chest and the head, with their binds; the root `hips` over the
/// ground), `half` wide (`outward`). The most any place moved.
#[allow(clippy::too_many_arguments)]
fn sway(m: &mut esf::Model, weld: &Weld, dir: &[Vec3], pts: &[(Vec3, [u8; 4], [u8; 4])], nodes: [(usize, Vec3); 3], hips: f32, half: f32, poses: &[Vec<glam::Mat4>], gap: f32, reach: f32, cap: f32) -> f32 {
    let at: Vec<Vec3> = weld.members.iter().map(|vs| Vec3::from(m.pos[vs[0]])).collect();
    let skin: Vec<([u8; 4], [u8; 4])> = weld.members.iter().map(|vs| (m.bones[vs[0]], m.weights[vs[0]])).collect();
    let mut push = vec![0.0f32; at.len()];
    for pal in poses {
        let posed = |p: Vec3, b: [u8; 4], w: [u8; 4]| {
            let (mut q, mut sum) = (Vec3::ZERO, 0.0);
            for k in 0..4 {
                if b[k] != 255 && w[k] > 0 && (b[k] as usize) < pal.len() {
                    q += pal[b[k] as usize].transform_point3(p) * w[k] as f32;
                    sum += w[k] as f32;
                }
            }
            if sum > 0.0 { q / sum } else { p }
        };
        let node = |(i, bind): (usize, Vec3)| pal.get(i).map_or(bind, |x| x.transform_point3(bind));
        let [root, chest, head] = nodes.map(node);
        let axis = [root - Vec3::Y * hips, root, chest, head];
        let c: Vec<Vec3> = at.iter().zip(&skin).map(|(&p, &(b, w))| posed(p, b, w)).collect();
        // each place's way out, posed
        let d: Vec<Vec3> = at.iter().zip(&skin).zip(&c).zip(dir).map(|(((&p, &(b, w)), &q), &o)| ((posed(p + o * 0.01, b, w) - q) * 100.0).try_normalize().unwrap_or(o)).collect();
        // by height, so a triangle looks only at the points level with it
        let mut body: Vec<Vec3> = pts.iter().map(|&(p, b, w)| posed(p, b, w)).collect();
        body.sort_by(|a, b| a.y.total_cmp(&b.y));
        for t in &weld.tris {
            let k3 = t.map(|k| c[k]);
            let Some(n) = (k3[1] - k3[0]).cross(k3[2] - k3[0]).try_normalize() else { continue };
            let n = if n.dot(outward(&axis, half, (k3[0] + k3[1] + k3[2]) / 3.0)) >= 0.0 { n } else { -n };
            let (lo, hi) = (k3[0].min(k3[1]).min(k3[2]) - Vec3::splat(reach), k3[0].max(k3[1]).max(k3[2]) + Vec3::splat(reach));
            let (e0, e1) = (k3[1] - k3[0], k3[2] - k3[0]);
            let (d00, d01, d11) = (e0.dot(e0), e0.dot(e1), e1.dot(e1));
            let den = d00 * d11 - d01 * d01;
            if den.abs() < 1e-12 {
                continue;
            }
            let mut need = 0.0f32;
            let level = &body[body.partition_point(|p| p.y < lo.y)..body.partition_point(|p| p.y <= hi.y)];
            for &p in level.iter().filter(|p| p.cmpge(lo).all() && p.cmple(hi).all()) {
                let h = (p - k3[0]).dot(n);
                if h <= -gap || h >= reach {
                    continue;
                }
                // under the triangle: its foot on the plane inside it
                let e2 = p - n * h - k3[0];
                let (d20, d21) = (e2.dot(e0), e2.dot(e1));
                let v = (d11 * d20 - d01 * d21) / den;
                let w = (d00 * d21 - d01 * d20) / den;
                if v >= 0.0 && w >= 0.0 && v + w <= 1.0 {
                    need = need.max(h + gap);
                }
            }
            if need > 0.0 {
                for &k in t {
                    push[k] = push[k].max(need / d[k].dot(n).max(0.5));
                }
            }
        }
    }
    for _ in 0..2 {
        let was = push.clone();
        for (k, p) in push.iter_mut().enumerate() {
            *p = weld.next[k].iter().map(|&j| 0.5 * was[j]).fold(*p, f32::max);
        }
    }
    for (k, vs) in weld.members.iter().enumerate() {
        push[k] = push[k].min(cap);
        let to = at[k] + dir[k] * push[k];
        for &v in vs {
            m.pos[v] = to.to_array();
        }
    }
    push.into_iter().fold(0.0, f32::max)
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

/// How far the body `wearer`, dressed in `look`, shows through the cape `m`
/// fitted to it, posed as `action` (None standing) `ms` in: the furthest the
/// outer end of an edge of the body lies outside a cape triangle the edge goes
/// through, along the triangle's way out from the spine, and where (its
/// height of the body's to the shoulders, 0 the ground, and its node); 0
/// when none does. Of the body's triangles that show in the look (a robe's
/// skirt only under a robe), those of its torso, legs and shoulders: not its
/// head's, not its arms' from the elbows down, which swing free of a cape.
#[cfg(test)]
fn poke(lib: &Library, m: &esf::Model, wearer: u32, look: &vi::cust::Look, action: Option<i32>, ms: i64) -> (f32, String) {
    let r = lib.lookup(wearer as i32).unwrap();
    let mut body = lib.build_model(r).unwrap();
    let mut anims = AnimScene::<Ee>::default();
    let mut rand = Rand(1);
    let mut cs = load_csprite::<Ee>(lib, r, &mut anims, &mut AnimCache::new(), &mut rand).unwrap();
    crate::play::dress_look(lib, wearer as i32, &mut body, &cs, look, false, &vi::cust::Tables::retail(), true);
    let frame = Frame::of(&Bones::of(&cs), &body).unwrap();
    let mut t = 0;
    while t <= ms {
        anims.time = t;
        match action {
            Some(a) if t == 0 => {
                cs.set_action(a, &anims, &mut rand);
            }
            Some(_) => {}
            None => cs.set_locomotion(0, &anims, &mut rand),
        }
        cs.h.process(0, &anims);
        t += 16;
    }
    cs.h.set_blend_matrices();
    let pal: Vec<glam::Mat4> = cs.h.blend.iter().map(|b| glam::Mat4::from_cols_array_2d(&b.to_cols())).collect();
    let skin = |p: [f32; 3], b: [u8; 4], w: [u8; 4]| {
        let (mut q, mut sum) = (Vec3::ZERO, 0.0);
        for k in 0..4 {
            if b[k] != 255 && (b[k] as usize) < pal.len() && w[k] > 0 {
                q += pal[b[k] as usize].transform_point3(Vec3::from(p)) * w[k] as f32;
                sum += w[k] as f32;
            }
        }
        if sum > 0.0 { q / sum } else { Vec3::from(p) }
    };
    let node = |i: usize| {
        let w = &cs.h.nodes[i].world;
        Vec3::new(w.m[3].x.0, w.m[3].y.0, w.m[3].z.0)
    };
    let (root, chest) = (node(frame.root), node(frame.chest));
    let skip = loose(&frame, &Bones::of(&cs), false, 2);
    let ax = chest - root;
    // each cape triangle's corners, its plane's normal turned away from the
    // spine (the root to the chest), and its bounds
    let planes: Vec<([Vec3; 3], Vec3, Vec3, Vec3)> = m
        .batches
        .iter()
        .flat_map(|b| b.indices.chunks_exact(3))
        .filter_map(|t| {
            let c = [0, 1, 2].map(|k| skin(m.pos[t[k] as usize], m.bones[t[k] as usize], m.weights[t[k] as usize]));
            let n = (c[1] - c[0]).cross(c[2] - c[0]).try_normalize()?;
            let mid = (c[0] + c[1] + c[2]) / 3.0;
            let tt = ((mid - root).dot(ax) / ax.length_squared()).clamp(0.0, 1.0);
            let n = if n.dot(mid - (root + ax * tt)) >= 0.0 { n } else { -n };
            Some((c, n, c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2])))
        })
        .collect();
    let mut worst = (0.0f32, String::new());
    for b in body.batches.iter().filter(|b| b.lod.unwrap_or(0) == 0) {
        for t in b.indices.chunks_exact(3) {
            let uv = |l: &esf::Layer| [0, 1, 2].map(|k| l.apply_uv(body.uv[t[k] as usize]));
            if !b.layers.iter().filter(|l| l.blend == 0 && l.texture != 0).any(|l| crate::slotless::texture_shows(lib, l.texture, &uv(l))) {
                continue;
            }
            if t.iter().any(|&v| skip.get(heaviest(&body, v as usize)).copied().unwrap_or(true)) {
                continue;
            }
            let p = [0, 1, 2].map(|k| skin(body.pos[t[k] as usize], body.bones[t[k] as usize], body.weights[t[k] as usize]));
            for (i, j) in [(0, 1), (1, 2), (2, 0)] {
                let (a, e) = (p[i], p[j]);
                let (lo, hi) = (a.min(e), a.max(e));
                for (c, n, clo, chi) in &planes {
                    if hi.cmplt(*clo).any() || lo.cmpgt(*chi).any() {
                        continue;
                    }
                    let (da, de) = ((a - c[0]).dot(*n), (e - c[0]).dot(*n));
                    if da * de > 0.0 {
                        continue;
                    }
                    // where the edge meets the plane, inside the triangle
                    let f = a + (e - a) * (da / (da - de));
                    let (e0, e1, e2) = (c[1] - c[0], c[2] - c[0], f - c[0]);
                    let (d00, d01, d11, d20, d21) = (e0.dot(e0), e0.dot(e1), e1.dot(e1), e2.dot(e0), e2.dot(e1));
                    let den = d00 * d11 - d01 * d01;
                    if den.abs() < 1e-12 {
                        continue;
                    }
                    let (vv, ww) = ((d11 * d20 - d01 * d21) / den, (d00 * d21 - d01 * d20) / den);
                    if vv < 0.0 || ww < 0.0 || vv + ww > 1.0 {
                        continue;
                    }
                    // the edge goes through the cape: its end outside, how far
                    let (d, q, v) = if da > de { (da, a, t[i]) } else { (de, e, t[j]) };
                    if d > worst.0 {
                        let h = (q.y - frame.ground) / (frame.shoulder_y - frame.ground);
                        worst = (d, format!("h{h:.2}n{}", heaviest(&body, v as usize)));
                    }
                }
            }
        }
    }
    worst
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

    /// A cape on the skeleton it was made on is the cape as it is, or moved
    /// out off the body a little where the body comes through it (the human's
    /// skeleton is the knight's, vampire's and Dawn Maiden's; the elf's
    /// Dread's); on any other player body or a server's race's body it names
    /// only the wearer's nodes, every vertex skinned, its top at the
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
                let f = Frame::of(&Bones::of(&cs), &body).unwrap_or_else(|| panic!("{id:08X}: a frame"));
                let tall = f.shoulder_y - f.ground;
                if Bones::of(&cs).same(&src.bones) {
                    let most = m.pos.iter().zip(&src.model.pos).map(|(a, b)| Vec3::from(*a).distance(Vec3::from(*b))).fold(0.0f32, f32::max);
                    assert!(most <= SWAY.2 * tall + 1e-4, "style {} on {id:08X}: as it is, or moved out a little ({most:.3})", s.id);
                    continue;
                }
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

    /// The capes `no_part_of_the_body_shows_through_a_settled_cape` checks,
    /// as (body, style, under a robe): the barbarian's in Dread's cape (its
    /// plate shoulders through the cape as it ran), the human's in the Dawn
    /// Maiden's (its arms through the sides), the elf's robe under Dread's
    /// (the hem kicking back through it as it walked) and AA5B0447 in the
    /// vampire's.
    const CASES: &[(u32, u8, bool)] = &[(0x8480_7490, 4, false), (0x70D8_98C6, 3, false), (0xA99C_6B15, 4, true), (0xAA5B_0447, 2, false)];
    /// The most of the body's height to the shoulders a checked case may show
    /// through the cape settled.
    const AFTER: f32 = 0.02;

    /// The frames a cape is checked in, none of them a frame it is fitted
    /// in (`POSES`): standing a second in, and the walk and the run between
    /// the frames fitted.
    fn checked() -> Vec<(&'static str, Option<i32>, i64)> {
        let mut v = vec![("idle", None, 1000)];
        for ms in [100, 325, 575, 825] {
            v.push(("walk", Some(1), ms));
            v.push(("run", Some(3), ms));
        }
        v
    }

    /// A cape settled on its wearer (`settle`) shows no part of the body
    /// through it in frames of the walk and the run it was not fitted in
    /// (`CASES`: a barbarian's plate shoulders, a human's arms, an elf's
    /// robe's hem) by more than a fiftieth of the body's height to the
    /// shoulders; the control: the same capes as placed, before they are
    /// settled, show each by more than a twentieth.
    #[test]
    fn no_part_of_the_body_shows_through_a_settled_cape() {
        let Some(lib) = char_lib() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let plate = vi::cust::Look { armor: [4; 6], robe: -1, ..Default::default() };
        let robe = vi::cust::Look { armor: [4; 6], robe: 1, ..Default::default() };
        for (id, st, look) in CASES.iter().map(|&(id, st, r)| (id, st, if r { &robe } else { &plate })) {
            let (cs, body) = sprite(&lib, id);
            let to = Bones::of(&cs);
            let f = Frame::of(&to, &body).unwrap();
            let tall = f.shoulder_y - f.ground;
            let placed = placed(&source(&lib, style(st).unwrap()).unwrap(), &to, Some(&f), false).unwrap();
            let settled = model(&lib, Cape { style: st, color: 0 }, id, &cs).unwrap();
            let worst = |m: &esf::Model| checked().into_iter().map(|(p, a, ms)| { let (d, at) = poke(&lib, m, id, look, a, ms); (d, format!("{p}{ms} {at}")) }).fold((0.0f32, String::new()), |w, x| if x.0 > w.0 { x } else { w });
            let (before, after) = (worst(&placed), worst(&settled));
            assert!(before.0 > 0.05 * tall, "the control: style {st} on {id:08X} shows through as placed ({:.3}, {})", before.0, before.1);
            assert!(after.0 < AFTER * tall, "style {st} on {id:08X}: {:.3} through it ({}), {:.3} as placed", after.0, after.1, before.0);
        }
    }

    /// SURVEY (ignored, a few minutes): every player body and race body in
    /// every style, plate and plate under a robe, how far it shows through
    /// the cape as placed and as settled at worst over `checked`, and how far
    /// the settle moved the cape.
    #[test]
    #[ignore]
    fn survey_what_shows_through() {
        let Some(lib) = char_lib() else { return };
        let looks = [("plate", vi::cust::Look { armor: [4; 6], robe: -1, ..Default::default() }), ("robe", vi::cust::Look { armor: [4; 6], robe: 1, ..Default::default() })];
        for &id in PLAYERS.iter().chain(RACES) {
            let (cs, body) = sprite(&lib, id);
            let to = Bones::of(&cs);
            let Some(f) = Frame::of(&to, &body) else { continue };
            for s in STYLES {
                let Some(placed) = placed(&source(&lib, s).unwrap(), &to, Some(&f), false) else { continue };
                let Some(settled) = model(&lib, Cape { style: s.id, color: 0 }, id, &cs) else { continue };
                let moved = placed.pos.iter().zip(&settled.pos).map(|(a, b)| Vec3::from(*a).distance(Vec3::from(*b))).fold(0.0f32, f32::max);
                let mut line = format!("{id:08X} c{} moved {moved:.3} of {:.2}", s.id, f.shoulder_y - f.ground);
                for (name, look) in &looks {
                    for m in [&placed, &settled] {
                        let w = checked().into_iter().map(|(p, a, ms)| { let (d, at) = poke(&lib, m, id, look, a, ms); (d, format!("{p}{ms}@{at}")) }).fold((0.0f32, String::new()), |w, x| if x.0 > w.0 { x } else { w });
                        line += &format!("  {name} {:.3} {:18}", w.0, w.1);
                    }
                }
                println!("{line}");
            }
        }
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
