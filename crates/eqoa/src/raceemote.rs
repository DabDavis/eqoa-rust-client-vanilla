//! Emotes on the bodies a server gives races of its own (vanilla only; the
//! user, 2026-11-08: "lets add emotes to the new playable races").
//!
//! An emote is an action on the character's sprite: CLIENT's `PlayEmote`
//! sends the bare opcode (0x001E wave, 0x0042 bow, 0x0043 point, 0x0044
//! cheer) and plays action 56, 57, 60 or 61 on the player's own sprite
//! (`VICSprite::SetAction`), and go-eqoa-server shows it to everyone who sees
//! the player as 0x0775 (the entity, the same action), which CLIENT's
//! dispatcher plays on that entity's sprite (`modules/client/src/viclient.cpp`;
//! go-eqoa-server `entity/emote.go`). Each client resolves the action against
//! the body's own play list (0x2910): `VICSprite::SetAnimation` plays
//! `m_entries[action]`. A player race's body has the four (HumanMale's play
//! list names them; every player race's does). Of the bodies the server's
//! races wear (racedefs 11 to 29 and the froglok), only the brownie's and the
//! skeleton's have them, and they are HumanMale's own clips (and not on the
//! 2002 disc, the 2002 art's); the other eighteen have none, and
//! `SetDefaultEntries` fills an empty 56, 57, 60 or 61 from the idle, played
//! once (`src/vicsprite.cpp`): on a PS2 and in this client alike, a hill
//! giant's wave was its idle starting over, nothing to see.
//!
//! Here such a body plays something for each, as `TABLE` says, body by body,
//! chosen by sight (`eqoa emotes <body> <out.png> [pose ...]`, `emoteview`):
//! one of the body's own animations, played once at the emote's priority, or
//! the human's emote carried onto the body's skeleton ([`retarget`]: the
//! bodies built on the human's bone names, with their own proportions). The
//! emote entry is written over, so the action stays the game's 56, 57, 60 or
//! 61 everywhere: the player's own (`PlayEmote`'s `SetAction` on the walker)
//! and another player's (0x0775 on its puppet) play the same, and the server
//! relays the emote as before.
//!
//! What a PS2 sees of it: its own play list, the idle once, unless the
//! server sends it one of the body's own slots instead (go-eqoa-server's
//! `entity/emote.go`, for a viewer that has not said it plays the emote
//! itself, `net::session::OP_RACE_EMOTES`).
//!
//! Only a player's character (name colour 2, the player's own) in a body
//! `TABLE` lists, on a server that has races of its own (it offered
//! creation's races, or sent race labels), changes; an NPC in the same body,
//! a body not listed, and every body on a server with none (Sandstorm,
//! eqoa.live) is the game as it was.
//!
//! The fall and the landing, the same way (the user, 2026-11-09, asked if
//! the new races' bodies should borrow the human's: "Yes, borrow the
//! human's"). A body in the air plays its play list's entry 6 (the fall,
//! `UpdateLocomotion`'s gait 6) and coming down entry 7 (the landing,
//! `UpdateSprite`'s), and of the bodies `TABLE` lists only the brownie's,
//! the dryad's, the orc's and the skeleton's have them; the others' are
//! `SetDefaultEntries`' idle (the fall the idle looping, the landing the idle
//! once), so a jump or a fall off a ledge stood them in the air. Here they
//! take the human's fall and landing carried onto their skeletons (`retarget`),
//! the fall looping and the landing once, at their entries' own priority:
//! but the pixie, the fairy and the fungusman, which share one bone with the
//! human (carried, they only lean), whose are their own nearest, as their
//! emotes are (`AIR_OWN`). Only a player's character in a body `TABLE`
//! lists, or another its race's options offer (the shadow gargoyle, a goblin
//! king: `race_of`, the body worn), on a server with jumps (`grant_air`): the
//! jumper's own and every other player's on this client, a jump and a fall off
//! a ledge alike; a PS2 still stands them in the air, as every body is
//! elsewhere.
//!
//! The swim, the same way (petition 1456, a gargoyle player's: "gargs fly in
//! the water instead of swim"). A body in liquid plays its play list's entry
//! 12 moving and 13 still (`UpdateLocomotion`), and of the races' bodies the
//! gargoyles' six and the Jal'Raeth's three have neither: `SetDefaultEntries`
//! fills them with the idle, so a gargoyle stood upright in the water, its
//! wings out, as if it flew, and a Jal'Raeth sat in it cross-legged. Here they
//! take the human's swim carried onto their skeletons, looping (`grant_swim`:
//! a gargoyle's wings, which the human lacks, held as its idle holds them, so
//! they stand up off its back as it lies along the water); the pixie and
//! the fairy, which have none either, keep their hover (`SWIM_NONE`: nothing
//! of theirs but flies, and the human's would not carry). Only a player's
//! character in such a body, on a server with races of its own, as the
//! emotes: the swimmer's own and every other player's on this client; a PS2
//! still shows them as before.

use esf::Library;
use glam::{Quat as GQuat, Vec3};
use std::collections::HashMap;
use vi::character::{parse_anim, parse_hierarchy, parse_ref_map};
use vi::csprite::CSprite;
use vi::fpu::{Ee, F};
use vi::hsprite::{Anim, AnimScene, Frame, Frames, HSprite, ONCE, unpack};
use vi::quat::Quat;
use vi::vect::Vect3;

/// The emotes' actions, as `PlayEmote` plays them: wave, bow, point, cheer.
pub const EMOTES: [i32; 4] = [56, 57, 60, 61];

/// The emotes' names, in `EMOTES`' order.
pub const NAMES: [&str; 4] = ["wave", "bow", "point", "cheer"];

/// The human whose emotes a body borrows (HumanMale; HumanFemale's four are
/// the same clips on the same skeleton).
pub const HUMAN: u32 = 0x70D8_98C6;

/// What an emote plays on a body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pose {
    /// One of the body's own play list slots, played once.
    Own(i32),
    /// The human's emote, carried onto the body's skeleton (`retarget`).
    Human,
}

const H: [Pose; 4] = [Pose::Human; 4];

/// Per body (CHAR.ESF DictID), what wave, bow, point and cheer play: the
/// bodies go-eqoa-server's races wore on 2026-11-08 (racedefs 11 to 29, the
/// froglok, and the hill giant's bodies for its armour, racearmorbody).
/// The brownie's and the skeleton's play lists have the four on the
/// Frontiers disc (the human's own clips), so `put` leaves them as they are;
/// the 2002 disc's (the 2002 art, F9) have none, and take the human's
/// carried. Chosen by sight (`eqoa emotes`). No body has a wave,
/// a bow or a point of its own, and every body built on the human's bone
/// names carries the human's emote cleanly (its arms, spine and head; the
/// gargoyle's, froglok's and aviak's spines bend by `retarget`'s rule for a
/// node the human lacks), where its own nearest (a cast's gesture for a
/// wave, a wind-up's raised arms for a cheer, which go on into the strike)
/// reads as the creature's attack. The pixie, the fairy and the fungusman
/// share one bone with the human: theirs are their own, the nearest each
/// has (`Own`'s slots, played once and silent, as the human's emotes are).
pub const TABLE: &[(u32, [Pose; 4])] = &[
    (0x141B_FE0C, H), // froglok
    (0xFFB1_EA2C, H), // hill giant
    (0xE617_303B, H), // hill giant in cloth (a sand giant's body)
    (0x72A8_FDBB, H), // hill giant in chain (a fire giant's)
    (0x2D68_6972, H), // hill giant in plate
    (0x2217_6258, H), // kerran
    (0xB749_BF57, H), // kerran, female
    (0x6C13_08E0, H), // goblin
    (0x3151_DF13, H), // aviak
    (0x8EFD_12EA, H), // aviak, female
    (0x9112_20E2, H), // lizardman
    (0x31A1_5F17, H), // kappa
    (0x4376_EB52, H), // brownie (its own on the Frontiers disc)
    // a hand raised high and a swoop; a dive forward (a flying bow); arms
    // spread and a lunge, an arm out; both arms overhead
    (0x94BA_77CD, [Pose::Own(25), Pose::Own(23), Pose::Own(22), Pose::Own(37)]), // pixie
    // the pixie's but the bow: wings and arms opened wide (it has no dive)
    (0x7A85_646C, [Pose::Own(25), Pose::Own(15), Pose::Own(22), Pose::Own(37)]), // fairy
    (0xCF41_5561, H), // gnoll
    (0x8214_A32D, H), // orc
    (0xCFC4_78EE, H), // kobold
    (0xC5CB_F4CF, H), // minotaur
    (0xBADC_25C9, H), // ratman
    (0xBE10_226F, H), // satyr
    (0x70EF_CE05, H), // dryad (HumanFemale's skeleton: the human's clips as they are)
    (0x6AF9_8A81, H), // skeleton (its own on the Frontiers disc)
    // an arm swung out to the side, twice; the cap tipped forward; an arm
    // thrust forward; arms flung wide
    (0x4A4D_88FB, [Pose::Own(15), Pose::Own(42), Pose::Own(30), Pose::Own(37)]), // fungusman
    (0xB4D2_D132, H), // gargoyle
    // the Jal'Raeth (2026-11-09: race 30), floating cross-legged: 22 of the
    // human's bone names (the spine, arms and head), so the human's emote moves
    // its upper body and its crossed legs keep their float (the channeler's and
    // knight's bodies take these by `poses_of`)
    (0xF51B_AB09, H),
];

/// What `model` plays for each emote, if `TABLE` lists it.
pub fn poses(model: u32) -> Option<[Pose; 4]> {
    TABLE.iter().find(|(b, _)| *b == model).map(|(_, p)| *p)
}

/// The fall's and the landing's entries: the gait `UpdateLocomotion` gives a
/// body in the air, and the one `UpdateSprite` gives it coming down.
pub const AIR: [i32; 2] = [6, 7];

/// The bodies `TABLE` lists whose fall and landing are their own nearest,
/// as their emotes are (they share one bone with the human): chosen by sight
/// (`eqoa emotes <body> <out.png> 22 23 37 42`). The pixie's and the fairy's
/// fall both arms flung overhead (their cheers), the landing arms spread and
/// the knees drawn up (their points); the fungusman's fall arms flung wide (its
/// cheer), the landing the cap tipped forward (its bow).
pub const AIR_OWN: &[(u32, [Pose; 2])] = &[
    (0x94BA_77CD, [Pose::Own(37), Pose::Own(22)]), // pixie
    (0x7A85_646C, [Pose::Own(37), Pose::Own(22)]), // fairy
    (0x4A4D_88FB, [Pose::Own(37), Pose::Own(42)]), // fungusman
];

/// What `model` plays for the fall and the landing, if it is one of a race's
/// bodies (`race_of`: `TABLE`'s, or another its race's options offer): its
/// race's own nearest (`AIR_OWN`), else the human's. An offered body was
/// looked up as itself, in `TABLE` alone, so a player in one (the shadow
/// gargoyle, a goblin king) stood in the air: its fall and landing the idle.
pub fn air_poses(model: u32) -> Option<[Pose; 2]> {
    let race = race_of(model)?;
    Some(AIR_OWN.iter().find(|(b, _)| *b == race).map_or([Pose::Human; 2], |(_, p)| *p))
}

/// The body `TABLE` lists that `model`, the body worn, is or stands for: itself,
/// or, for another body of a race's creature its options offer a player, its
/// race's own (`raceopts::race_body_of`: the shadow gargoyle's the granite one,
/// a goblin king's the grey goblin); none for any other body.
fn race_of(model: u32) -> Option<u32> {
    poses(model).map(|_| model).or_else(|| crate::raceopts::race_body_of(model).filter(|&b| poses(b).is_some()))
}

/// What `model` plays for each emote on this server: `TABLE`'s, else, for
/// another body of a race's creature its options offer a player (the ice
/// giant's, a goblin king's, the faun's: `raceopts::race_body_of`), its race's
/// own body's. Every such body is its race's creature on its race's skeleton
/// or one of its bone names (the great skeletons' the troll's, 37 of the
/// human's), and the fungusmen's three share the slots `Own` names.
pub fn poses_of(model: u32) -> Option<[Pose; 4]> {
    race_of(model).and_then(poses)
}

/// One body's skeleton as its 0x2700 record has it: each node's parent,
/// its bind pose in its parent's space (`rest`: the hierarchy's frames are
/// the model's, `SetNodeBindPose`, and `rest` the parent's taken off), and
/// its ref id.
pub struct Rig {
    pub parent: Vec<i32>,
    /// The bind pose in the parent's space.
    pub rest: Vec<Frame<Ee>>,
    /// The node a ref id names.
    pub by_ref: HashMap<i32, usize>,
    /// Each node's ref id (the least the map names it by).
    pub refs: Vec<Option<i32>>,
}

impl Rig {
    fn new(h: &HSprite<Ee>, map: &HashMap<i32, i32>) -> Rig {
        let mut refs = vec![None; h.nodes.len()];
        let mut by_ref = HashMap::new();
        let mut keys: Vec<(&i32, &i32)> = map.iter().collect();
        keys.sort();
        for (&k, &n) in keys {
            if let Some(r) = refs.get_mut(n as usize) {
                r.get_or_insert(k);
                by_ref.insert(k, n as usize);
            }
        }
        Rig { parent: h.nodes.iter().map(|n| n.parent).collect(), rest: h.nodes.iter().map(|n| n.rest).collect(), by_ref, refs }
    }

    /// A sprite's, with its ref map.
    pub fn of(cs: &CSprite<Ee>, map: &HashMap<i32, i32>) -> Rig {
        Rig::new(&cs.h, map)
    }

    /// Body `id`'s, off the library: its hierarchy (0x2400) and the ref map
    /// after it (the skeleton's).
    pub fn load(lib: &Library, id: u32) -> Option<Rig> {
        let r = lib.lookup(id as i32)?;
        let c = lib.file(r.file);
        let hier = c.child(r.node, 0x2400)?;
        let kids = &c.node(r.node).children;
        let at = kids.iter().position(|&k| k == hier)?;
        let map = kids[at..].iter().copied().find(|&k| c.node(k).ty == 0x5000)?;
        let mut h = HSprite::<Ee>::new(0);
        parse_hierarchy(&mut h, c.body(hier), c.node(hier).version);
        Some(Rig::new(&h, &parse_ref_map(c.body(map))))
    }

    /// Each node's rotation in the model, its local ones `local` taken down
    /// from the root (`ProcessHierarchy`: a node's world is its parent's
    /// times its own, as column vectors).
    fn world(&self, local: &[GQuat]) -> Vec<GQuat> {
        let mut out: Vec<Option<GQuat>> = vec![None; self.parent.len()];
        fn get(i: usize, rig: &Rig, local: &[GQuat], out: &mut Vec<Option<GQuat>>, depth: u32) -> GQuat {
            if let Some(q) = out[i] {
                return q;
            }
            let p = rig.parent[i];
            let q = if p >= 0 && (p as usize) < rig.parent.len() && p as usize != i && depth < 256 { get(p as usize, rig, local, out, depth + 1) * local[i] } else { local[i] };
            out[i] = Some(q);
            q
        }
        (0..self.parent.len()).map(|i| get(i, self, local, &mut out, 0)).collect()
    }

    /// How many parents up node `i`'s root is.
    fn depth(&self, mut i: usize) -> usize {
        let mut d = 0;
        while self.parent[i] >= 0 && (self.parent[i] as usize) != i && d < 256 {
            i = self.parent[i] as usize;
            d += 1;
        }
        d
    }

    /// Parents before children.
    fn order(&self) -> Vec<usize> {
        let mut o: Vec<usize> = (0..self.parent.len()).collect();
        o.sort_by_key(|&i| self.depth(i));
        o
    }
}

fn gq(q: &Quat<Ee>) -> GQuat {
    GQuat::from_xyzw(q.x.0, q.y.0, q.z.0, q.w.0).normalize()
}

fn eq(q: GQuat) -> Quat<Ee> {
    Quat { x: F::new(q.x), y: F::new(q.y), z: F::new(q.z), w: F::new(q.w) }
}

fn gv(v: &Vect3<Ee>) -> Vec3 {
    Vec3::new(v.x.0, v.y.0, v.z.0)
}

fn ev(v: Vec3) -> Vect3<Ee> {
    Vect3::new(F::new(v.x), F::new(v.y), F::new(v.z))
}

/// Frame `f` (clamped to the clip) of the clip node whose frames begin at
/// `first`.
fn frame_of(a: &Anim<Ee>, first: i32, f: i32) -> Frame<Ee> {
    let i = (first + f.clamp(0, (a.num_frames - 1).max(0))).max(0) as usize;
    match &a.frames {
        Frames::Full(v) => v.get(i).copied().unwrap_or_default(),
        Frames::Pack(v) => v.get(i).map(unpack).unwrap_or_default(),
    }
}

/// A play list row: its two clips' DictIDs and its play speed.
#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub clips: [i32; 2],
    pub weight: F<Ee>,
}

/// Body `id`'s play list row for `slot`, off the library (0x2910).
pub fn row(lib: &Library, id: u32, slot: i32) -> Option<Row> {
    let r = lib.lookup(id as i32)?;
    let c = lib.file(r.file);
    let pl = c.child(r.node, 0x2910)?;
    let (v, d) = (c.node(pl).version, c.body(pl));
    let at = |o: usize| d.get(o..o + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap()));
    let mut p = 4;
    for _ in 0..at(0)? {
        let a0 = at(p)?;
        p += 4;
        let a1 = if v != 0 {
            p += 4;
            at(p - 4)?
        } else {
            0
        };
        let s = at(p)?;
        let weight = F::new(f32::from_bits(at(p + 4)? as u32));
        p += 12;
        if v >= 2 {
            p += if v >= 3 { 24 } else { 16 };
        }
        if s == slot {
            return Some(Row { clips: [a0, a1], weight });
        }
    }
    None
}

/// A clip off the library by DictID.
pub fn clip(lib: &Library, id: i32) -> Option<Anim<Ee>> {
    if id == 0 {
        return None;
    }
    let r = lib.lookup(id)?;
    let c = lib.file(r.file);
    (c.node(r.node).ty == 0x2600).then(|| parse_anim(c.body(r.node), c.node(r.node).version))
}

/// What the table's carried emotes are measured from.
pub const BASE: Base = Base::Bind;

/// What a carried emote's turns are measured from: the two skeletons' bind
/// poses, or the poses their idles stand in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Base {
    Bind,
    Idle,
}

/// The local frames of the first frame of each node `clips` drive, else
/// `rig`'s rest.
fn first_frames(rig: &Rig, clips: &[&Anim<Ee>]) -> Vec<Frame<Ee>> {
    let mut out = rig.rest.clone();
    let mut set = vec![false; out.len()];
    for a in clips {
        for &(rid, first) in &a.nodes {
            if let Some(&n) = rig.by_ref.get(&rid)
                && !set[n]
            {
                out[n] = frame_of(a, first, 0);
                set[n] = true;
            }
        }
    }
    out
}

/// The human's emote carried onto a body's skeleton (`clips`, the human's
/// two layers for it; `human_idle`, the human's idle's): one clip for each
/// of the body's two layers, driving the nodes the body's own idle drives on
/// that layer (`idle`), at the human clip's frames and rate.
///
/// Each node of the body that the human has (by ref id: the bodies built on
/// the human's bone names) turns in the model as the human's turns from
/// `base` (its bind pose, or the pose its idle stands in), taken onto the
/// body's own, so an arm raised in a wave is raised whatever the body's own
/// bind or stance; its bone keeps the body's length, a moved bone (the hips
/// in a bow) moving by the human's move scaled to the body's size. A node
/// the human lacks turns as the human node does that holds everything of
/// the human's below it (a spine of other names bends as the human's), and
/// one with nothing of the human's below it (a tail, a wing, a third finger)
/// keeps the body's idle's first frame.
pub fn retarget(human: &Rig, human_idle: &[Anim<Ee>], clips: &[Anim<Ee>], body: &Rig, idle: [&Anim<Ee>; 2], base: Base) -> [Anim<Ee>; 2] {
    let n = clips.iter().map(|a| a.num_frames).max().unwrap_or(1).max(1);
    // which clip node drives each human node
    let mut drives: HashMap<usize, (usize, i32)> = HashMap::new();
    for (k, a) in clips.iter().enumerate() {
        for &(rid, first) in &a.nodes {
            if let Some(&h) = human.by_ref.get(&rid) {
                drives.entry(h).or_insert((k, first));
            }
        }
    }
    // each skeleton's base, local and in the model
    let (hbase_l, bbase_l) = match base {
        Base::Bind => (human.rest.clone(), body.rest.clone()),
        Base::Idle => (first_frames(human, &human_idle.iter().collect::<Vec<_>>()), first_frames(body, &idle)),
    };
    let bidle_l = first_frames(body, &idle);
    let hbase = human.world(&hbase_l.iter().map(|f| gq(&f.rot)).collect::<Vec<_>>());
    let bbase = body.world(&bbase_l.iter().map(|f| gq(&f.rot)).collect::<Vec<_>>());
    // the human node each body node turns as: its own, or the one holding
    // every human node mapped below it
    let mine: Vec<Option<usize>> = body.refs.iter().map(|r| r.and_then(|r| human.by_ref.get(&r).copied())).collect();
    // the lowest human node over both (`a`'s root when they share none)
    let up = |i: usize| usize::try_from(human.parent[i]).ok().filter(|&p| p != i && p < human.parent.len());
    let lca = |a: usize, b: usize| {
        let mut over = vec![a];
        while let Some(p) = up(*over.last().unwrap()).filter(|_| over.len() < 256) {
            over.push(p);
        }
        let mut b = b;
        for _ in 0..256 {
            if over.contains(&b) {
                return b;
            }
            match up(b) {
                Some(p) => b = p,
                None => break,
            }
        }
        *over.last().unwrap()
    };
    let order = body.order();
    // the lowest human node over every mapped one below each body node
    let mut below: Vec<Option<usize>> = mine.clone();
    for &b in order.iter().rev() {
        let p = body.parent[b];
        if let (Some(h), true) = (below[b], p >= 0 && (p as usize) != b) {
            let p = p as usize;
            below[p] = Some(match below[p] {
                Some(q) if mine[p].is_none() => lca(q, h),
                Some(q) => q,
                None => h,
            });
        }
    }
    // an unmapped node turns as that one, or as its parent when it is
    // itself mapped below (a thigh of another name over a mapped shin turns
    // as the human's thigh)
    let mapped: std::collections::HashSet<usize> = mine.iter().flatten().copied().collect();
    let held: Vec<Option<usize>> = (0..body.parent.len())
        .map(|b| match (mine[b], below[b]) {
            (Some(h), _) => Some(h),
            (None, Some(h)) if mapped.contains(&h) && human.parent[h] >= 0 => Some(human.parent[h] as usize),
            (None, h) => h,
        })
        .collect();
    let k = size_ratio(human, body);
    let mut frames: Vec<Vec<Frame<Ee>>> = vec![Vec::with_capacity(n as usize); body.parent.len()];
    for f in 0..n {
        let hl: Vec<Frame<Ee>> = (0..human.parent.len()).map(|h| drives.get(&h).map_or(hbase_l[h], |&(c, first)| frame_of(&clips[c], first, f))).collect();
        let hg = human.world(&hl.iter().map(|x| gq(&x.rot)).collect::<Vec<_>>());
        let mut bg = vec![GQuat::IDENTITY; body.parent.len()];
        for &b in &order {
            let p = body.parent[b];
            let pg = if p >= 0 && (p as usize) != b { bg[p as usize] } else { GQuat::IDENTITY };
            let own = bidle_l[b];
            let (local, g) = match (mine[b], held[b]) {
                (Some(h), _) => {
                    let g = (hg[h] * hbase[h].inverse()) * bbase[b];
                    // moved as the human's moves from its base, in the model,
                    // scaled to the body
                    let hp = human.parent[h];
                    let hpg = if hp >= 0 && (hp as usize) != h { hg[hp as usize] } else { GQuat::IDENTITY };
                    let moved = hpg * (gv(&hl[h].trans) - gv(&hbase_l[h].trans));
                    let t = gv(&bbase_l[b].trans) + pg.inverse() * moved * k;
                    (Frame { rot: eq(pg.inverse() * g), scale: own.scale, trans: ev(t) }, g)
                }
                (None, Some(h)) => {
                    let g = (hg[h] * hbase[h].inverse()) * bbase[b];
                    (Frame { rot: eq(pg.inverse() * g), ..own }, g)
                }
                (None, None) => (own, pg * gq(&own.rot)),
            };
            bg[b] = g;
            frames[b].push(local);
        }
    }
    let rate = clips.first().map_or(F::new(10.0), |a| a.play_rate);
    std::array::from_fn(|layer| {
        let nodes: Vec<usize> = idle[layer].nodes.iter().filter_map(|(rid, _)| body.by_ref.get(rid).copied()).collect();
        let mut full = Vec::with_capacity(nodes.len() * n as usize);
        let mut list = Vec::with_capacity(nodes.len());
        for (j, &b) in nodes.iter().enumerate() {
            list.push((body.refs[b].unwrap_or(0), j as i32 * n));
            full.extend_from_slice(&frames[b]);
        }
        Anim { num_nodes: list.len() as i32, num_frames: n, play_rate: rate, play_speed: F::ONE, playback_type: ONCE, nodes: list, frames: Frames::Full(full), triggers: Vec::new() }
    })
}

/// How much bigger the body's skeleton is than the human's: the median of
/// the length ratios of the bones both have (a hill giant's 2.8, a goblin's
/// 0.84).
fn size_ratio(human: &Rig, body: &Rig) -> f32 {
    let mut r: Vec<f32> = body
        .refs
        .iter()
        .enumerate()
        .filter_map(|(b, rid)| {
            let h = *human.by_ref.get(&(*rid)?)?;
            let (hl, bl) = (gv(&human.rest[h].trans).length(), gv(&body.rest[b].trans).length());
            (hl > 0.01).then_some(bl / hl)
        })
        .collect();
    if r.is_empty() {
        return 1.0;
    }
    r.sort_by(|a, b| a.total_cmp(b));
    r[r.len() / 2]
}

/// The human whose emotes a body borrows: its skeleton and its idle's
/// clips (what `Base::Idle` measures from).
pub struct Human {
    pub rig: Rig,
    pub idle: Vec<Anim<Ee>>,
}

impl Human {
    pub fn load(lib: &Library) -> Option<Human> {
        let rig = Rig::load(lib, HUMAN)?;
        let idle = row(lib, HUMAN, 0)?.clips.iter().filter_map(|&c| clip(lib, c)).collect();
        Some(Human { rig, idle })
    }

    /// The human's `action` carried onto `cs` (its clips in `sc`): the two
    /// layers' clips and the play speed, or `None` where the human or the
    /// sprite lacks what it takes.
    pub fn carry(&self, lib: &Library, action: i32, cs: &CSprite<Ee>, sc: &AnimScene<Ee>, base: Base) -> Option<([Anim<Ee>; 2], F<Ee>)> {
        let row = row(lib, HUMAN, action)?;
        let clips: Vec<Anim<Ee>> = row.clips.iter().filter_map(|&c| clip(lib, c)).collect();
        let idle = cs.entries[0].animation;
        let i0 = usize::try_from(idle[0]).ok().filter(|&i| i < sc.anims.len())?;
        let i1 = usize::try_from(idle[1]).ok().filter(|&i| i < sc.anims.len()).unwrap_or(i0);
        if clips.is_empty() {
            return None;
        }
        let body = Rig::of(cs, &sc.ref_maps[cs.h.ref_map]);
        Some((retarget(&self.rig, &self.idle, &clips, &body, [&sc.anims[i0], &sc.anims[i1]], base), row.weight))
    }
}

/// Puts `poses` on `cs` (its clips in `sc`): for each emote an own slot's
/// clips and speed over the emote's entry, or the human's emote carried onto
/// it (its clips added to `sc`), played once and silent, at the emote's own
/// priority and channel. Whether it did: not when the sprite's emotes are no
/// longer `SetDefaultEntries`' idle (put already, or its own).
pub fn put(lib: &Library, cs: &mut CSprite<Ee>, sc: &mut AnimScene<Ee>, poses: [Pose; 4]) -> bool {
    // done already, or the body has emotes of its own: an empty emote slot
    // is the idle's (`SetDefaultEntries`)
    if EMOTES.iter().any(|&e| cs.entries[e as usize].animation != cs.entries[0].animation) {
        return false;
    }
    let mut human: Option<Option<Human>> = None;
    for (k, &e) in EMOTES.iter().enumerate() {
        let e = e as usize;
        match poses[k] {
            Pose::Own(s) => {
                let src = cs.entries[s as usize];
                let dst = &mut cs.entries[e];
                dst.animation = src.animation;
                dst.weight = src.weight;
                dst.sound = [0, 0];
                dst.sync = ONCE;
            }
            Pose::Human => {
                let human = human.get_or_insert_with(|| Human::load(lib));
                let Some(([a, b], weight)) = human.as_ref().and_then(|h| h.carry(lib, e as i32, cs, sc, BASE)) else { continue };
                sc.anims.push(a);
                let ia = sc.anims.len() as i32 - 1;
                let ib = if cs.skel_type == 1 {
                    sc.anims.push(b);
                    sc.anims.len() as i32 - 1
                } else {
                    -1
                };
                let dst = &mut cs.entries[e];
                dst.animation = [ia, ib];
                dst.weight = weight;
                dst.sound = [0, 0];
                dst.sync = ONCE;
            }
        }
    }
    true
}

/// Puts `poses` on `cs`'s fall and landing (`AIR`; its clips in `sc`): an
/// own slot's clips and speed, or the human's carried onto it (its clips
/// added to `sc`), silent, the fall looping and the landing played once, at
/// their entries' own priority and channel. Whether it did: not when the body
/// has a fall or a landing that is no longer `SetDefaultEntries`' idle (its
/// own, or put already).
pub fn put_air(lib: &Library, cs: &mut CSprite<Ee>, sc: &mut AnimScene<Ee>, poses: [Pose; 2]) -> bool {
    // the fall loops while the body is in the air; the landing plays once
    put_over(lib, cs, sc, &AIR, &poses, &[0, ONCE])
}

/// Puts `poses` over `cs`'s `entries` (its clips in `sc`), each entry's
/// `syncs` its own (0 looping, `ONCE`): an own slot's clips and speed, or the
/// human's for that entry carried onto it (its clips added to `sc`, once for
/// entries whose human rows are the same clips), silent, at each entry's own
/// priority and channel. Whether it did: not when one of `entries` is no
/// longer `SetDefaultEntries`' idle (the body's own, or put already).
fn put_over(lib: &Library, cs: &mut CSprite<Ee>, sc: &mut AnimScene<Ee>, entries: &[i32], poses: &[Pose], syncs: &[i32]) -> bool {
    if entries.iter().any(|&e| cs.entries[e as usize].animation != cs.entries[0].animation) {
        return false;
    }
    let mut human: Option<Option<Human>> = None;
    // the human's rows carried so far (their clips), to what they made
    let mut carried: Vec<([i32; 2], [i32; 2], F<Ee>)> = Vec::new();
    for ((&e, &pose), &sync) in entries.iter().zip(poses).zip(syncs) {
        let e = e as usize;
        match pose {
            Pose::Own(s) => {
                let src = cs.entries[s as usize];
                let dst = &mut cs.entries[e];
                dst.animation = src.animation;
                dst.weight = src.weight;
                dst.sound = [0, 0];
                dst.sync = sync;
            }
            Pose::Human => {
                let clips = row(lib, HUMAN, e as i32).map(|r| r.clips);
                let (animation, weight) = match carried.iter().find(|c| Some(c.0) == clips) {
                    Some(&(_, animation, weight)) => (animation, weight),
                    None => {
                        let human = human.get_or_insert_with(|| Human::load(lib));
                        let Some(([a, b], weight)) = human.as_ref().and_then(|h| h.carry(lib, e as i32, cs, sc, BASE)) else { continue };
                        sc.anims.push(a);
                        let ia = sc.anims.len() as i32 - 1;
                        let ib = if cs.skel_type == 1 {
                            sc.anims.push(b);
                            sc.anims.len() as i32 - 1
                        } else {
                            -1
                        };
                        if let Some(c) = clips {
                            carried.push((c, [ia, ib], weight));
                        }
                        ([ia, ib], weight)
                    }
                };
                let dst = &mut cs.entries[e];
                dst.animation = animation;
                dst.weight = weight;
                dst.sound = [0, 0];
                dst.sync = sync;
            }
        }
    }
    true
}

/// The swim's entries: the gait `UpdateLocomotion` gives a body in liquid
/// moving or turning (12), and still (13, treading water).
pub const SWIM: [i32; 2] = [12, 13];

/// What `model` plays swimming and treading water, if it is one of a race's
/// bodies (`TABLE`'s, or another its race's options offer, `poses_of`): the
/// human's swim carried onto it (`retarget`), as its emotes are. Of the bodies
/// the races wear, only the gargoyles' six (25 of the human's bone names) and
/// the Jal'Raeth's three (22) have no swim of their own (`put_swim` leaves
/// every other's as it is); the pixie's and the fairy's have none either, and
/// share one bone with the human, so a carried swim would only nod its head,
/// and every motion of their own flies (their walk and run the same flight):
/// they are left hovering, as a PS2 draws them (`SWIM_NONE`).
pub fn swim_poses(model: u32) -> Option<[Pose; 2]> {
    let race = race_of(model)?;
    (!SWIM_NONE.contains(&race)).then_some([Pose::Human; 2])
}

/// The bodies `TABLE` lists that keep the game's swim, the idle (they have no
/// swim, the human's would not carry, and nothing of their own but flight):
/// the pixie's and the fairy's.
pub const SWIM_NONE: &[u32] = &[0x94BA_77CD, 0x7A85_646C];

/// Puts `poses` on `cs`'s swim (`SWIM`; its clips in `sc`), both looping, at
/// their entries' own priority and channel (`put_over`). Whether it did: not
/// when the body swims of its own (its 12 and 13 no longer `SetDefaultEntries`'
/// idle), or it was put already.
pub fn put_swim(lib: &Library, cs: &mut CSprite<Ee>, sc: &mut AnimScene<Ee>, poses: [Pose; 2]) -> bool {
    put_over(lib, cs, sc, &SWIM, &poses, &[0, 0])
}

/// The swim (`swim_poses`) on `cs` (made of `model`, its clips in `sc`) when
/// it is a player's character (`player`) on a server with races of its own
/// (`races`, `server_has_races`) in a body that has none (petition 1456,
/// a gargoyle's: "gargs fly in the water instead of swim": `SetDefaultEntries`
/// fills an empty swim with the idle, which stands a gargoyle upright in the
/// water, its wings out); anything else is left as the game made it. Whether
/// its swim is the table's.
pub fn grant_swim(lib: &Library, cs: &mut CSprite<Ee>, sc: &mut AnimScene<Ee>, model: u32, player: bool, races: Option<&net::creation::Creation>) -> bool {
    let Some(p) = swim_poses(model).filter(|_| player && server_has_races(races)) else { return false };
    put_swim(lib, cs, sc, p);
    true
}

/// The fall and the landing (`air_poses`) on `cs` (made of `model`, its clips
/// in `sc`) when it is a player's character (`player`) on a server with jumps
/// (`jumps`) in a body `TABLE` lists or another its race's options offer
/// (`race_of`); anything else is left as the game made it, and a body with a
/// fall and a landing of its own keeps them (`put_air`). Whether its fall and
/// landing are the table's.
pub fn grant_air(lib: &Library, cs: &mut CSprite<Ee>, sc: &mut AnimScene<Ee>, model: u32, player: bool, jumps: bool) -> bool {
    let Some(p) = air_poses(model).filter(|_| player && jumps) else { return false };
    put_air(lib, cs, sc, p);
    true
}

/// Whether the server has races of its own: it listed creation's races
/// (`net::creation`), or sent race labels (`racelabel`). Sandstorm and
/// eqoa.live do neither.
pub fn server_has_races(races: Option<&net::creation::Creation>) -> bool {
    races.is_some_and(|c| !c.races.is_empty()) || crate::racelabel::any()
}

/// `TABLE`'s emotes on `cs` (made of `model`, its clips in `sc`) when it is
/// a player's character (`player`) on a server with races of its own
/// (`races`, `server_has_races`) in a body the table lists, or another body a
/// race's options offer (`poses_of`: its race's own body's); anything else is
/// left as the game made it. Whether its emotes are the table's.
pub fn grant(lib: &Library, cs: &mut CSprite<Ee>, sc: &mut AnimScene<Ee>, model: u32, player: bool, races: Option<&net::creation::Creation>) -> bool {
    let Some(p) = poses_of(model).filter(|_| player && server_has_races(races)) else { return false };
    put(lib, cs, sc, p);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::creation::{Creation, Race};
    use vi::character::{AnimCache, load_csprite};
    use vi::csprite::Rand;

    const PIXIE: u32 = 0x94BA_77CD;
    const HILL_GIANT: u32 = 0xFFB1_EA2C;
    const DRYAD: u32 = 0x70EF_CE05;
    const BROWNIE: u32 = 0x4376_EB52;
    const SKELETON: u32 = 0x6AF9_8A81;

    fn races(bodies: &[u32]) -> Creation {
        let race = |(k, &b): (usize, &u32)| Race { id: 11 + k as i32, like: 9, city: 0, bonus: [0; 7], class_mask: 0x7FFF, body: [b, b], name: format!("R{k}"), short: "R".into(), desc: String::new() };
        Creation { all_classes: true, races: bodies.iter().enumerate().map(race).collect() }
    }

    /// A sprite as `load_csprite` leaves one whose play list names the idle
    /// (clips 0 and 1), 15 (2, 3), 22, 23, 25 and 37 (4.., two apiece, each
    /// with a sound), and no emote: `SetDefaultEntries` filled them from the
    /// idle.
    fn creature() -> CSprite<Ee> {
        let mut cs = CSprite::<Ee>::new(HSprite::new(0), 1, 9, 0, F::ONE, false);
        for (k, &s) in [0usize, 15, 22, 23, 25, 37].iter().enumerate() {
            let e = &mut cs.entries[s];
            e.animation = [2 * k as i32, 2 * k as i32 + 1];
            e.weight = F::new(1.0 + k as f32 / 10.0);
            e.sync = (s != 0) as i32;
            e.sound = [0x100 + s as u32, 0];
        }
        cs.set_default_entries();
        cs.set_anim_priorities();
        cs.set_anim_sound_channels();
        cs
    }

    fn entries(cs: &CSprite<Ee>) -> Vec<String> {
        cs.entries.iter().map(|e| format!("{:?} {:?} {} {:?} {:?} {}", e.animation, e.weight.0.to_bits(), e.sync, e.sound, e.priority, e.channel)).collect()
    }

    /// Another body a race's options offer plays its race's own body's emotes
    /// (the fungusman's red body its own slots, as the blue one; a goblin
    /// king's the human's carried, as the grey goblin), granted on a player's
    /// character; with no options (the control) such a body is not listed and
    /// keeps the game's, as does a body no row offers.
    #[test]
    fn an_offered_body_plays_its_races_emotes() {
        crate::raceopts::set_options(None);
        let red = 0x5874_ACFCu32;
        assert_eq!(poses_of(red), None, "the control: no options");
        crate::raceopts::set_options(Some(&crate::raceopts::tests::fungus_options()));
        assert_eq!(poses_of(red), poses(0x4A4D_88FB));
        assert!(matches!(poses_of(red), Some([Pose::Own(15), ..])));
        assert_eq!(poses_of(0x1234_5678), None);
        let listed = races(&[0x4A4D_88FB]);
        let mut cs = creature();
        assert!(grant(&Library::new(), &mut cs, &mut AnimScene::default(), red, true, Some(&listed)));
        assert_ne!(cs.entries[56].animation, cs.entries[0].animation, "the wave is the body's own slot now");
        let mut npc = creature();
        assert!(!grant(&Library::new(), &mut npc, &mut AnimScene::default(), red, false, Some(&listed)), "an NPC keeps the game's");
        crate::raceopts::set_options(None);
        let mut cs = creature();
        assert!(!grant(&Library::new(), &mut cs, &mut AnimScene::default(), red, true, Some(&listed)), "the control: no options");
    }

    /// An own slot goes over the emote's entry: its clips and speed, played
    /// once and silent, the emote's priority and channel kept; the emotes
    /// not asked for stay the idle. Put again, nothing changes; a sprite
    /// whose emotes are its own (the brownie's, the skeleton's) is left as
    /// it is. The control: the entry the emote had before, the idle's, is
    /// what the test tells it from.
    #[test]
    fn an_own_slot_over_the_emote() {
        let mut cs = creature();
        let before = cs.entries[56];
        assert_eq!(before.animation, cs.entries[0].animation, "the control: the emote was the idle's");
        assert!(put(&Library::new(), &mut cs, &mut AnimScene::default(), [Pose::Own(25), Pose::Own(15), Pose::Own(22), Pose::Own(37)]));
        for (e, s) in [(56, 25), (57, 15), (60, 22), (61, 37)] {
            let (got, src) = (cs.entries[e], cs.entries[s]);
            assert_eq!((got.animation, got.weight, got.sync, got.sound), (src.animation, src.weight, ONCE, [0, 0]), "emote {e} from {s}");
            assert_eq!((got.priority, got.channel), ([4, 4], 0), "emote {e}: the emote's priority and channel");
        }
        assert_ne!(cs.entries[56].animation, before.animation);
        let done = entries(&cs);
        assert!(!put(&Library::new(), &mut cs, &mut AnimScene::default(), [Pose::Own(23); 4]), "put already");
        assert_eq!(entries(&cs), done, "again: as it was");
        let mut own = creature();
        own.entries[56].animation = [40, 41];
        let had = entries(&own);
        assert!(!put(&Library::new(), &mut own, &mut AnimScene::default(), [Pose::Own(25); 4]), "emotes of its own");
        assert_eq!(entries(&own), had);
    }

    /// The table's emotes go on a player's character in a body it lists, on
    /// a server with races of its own (creation's list, or race labels), and
    /// nothing else changes: an NPC in that body, a body not listed, a
    /// server with neither. The control: the same sprite through the table
    /// by the body alone (player and server ignored) changes for the NPC.
    #[test]
    fn players_on_servers_with_races_only() {
        crate::racelabel::clear();
        let listed = races(&[PIXIE]);
        let lib = Library::new();
        let mut cs = creature();
        assert!(grant(&lib, &mut cs, &mut AnimScene::default(), PIXIE, true, Some(&listed)));
        assert_eq!(cs.entries[56].animation, cs.entries[25].animation, "the pixie's wave");
        for (model, player, list, why) in [(PIXIE, false, Some(&listed), "an NPC in the body"), (0x1234_5678, true, Some(&listed), "a body not listed"), (PIXIE, true, None, "a server with no races"), (PIXIE, true, Some(&races(&[])), "a server listing none")] {
            let mut cs = creature();
            let had = entries(&cs);
            assert!(!grant(&lib, &mut cs, &mut AnimScene::default(), model, player, list), "{why}");
            assert_eq!(entries(&cs), had, "{why}: as the game made it");
        }
        // race labels alone say the server has races of its own
        let mut labels = crate::racelabel::Labels::new();
        labels.insert("Lagerbottoms".into(), std::rc::Rc::new(crate::racelabel::Label { long: "Hill Giant".into(), short: "HGT".into() }));
        crate::racelabel::set(labels);
        let mut cs = creature();
        assert!(grant(&lib, &mut cs, &mut AnimScene::default(), PIXIE, true, None), "race labels");
        crate::racelabel::clear();
        // the control: by the body alone, the NPC would have changed
        let mut npc = creature();
        put(&lib, &mut npc, &mut AnimScene::default(), poses(PIXIE).unwrap());
        assert_ne!(npc.entries[56].animation, creature().entries[56].animation, "the control");
    }

    /// An own slot goes over the fall and the landing (`put_air`): its clips
    /// and speed, silent, the fall looping and the landing played once, at
    /// their entries' own priority and channel; put again, nothing changes;
    /// a body with a fall of its own is left as it is. The control: the
    /// entries were the idle's before.
    #[test]
    fn an_own_slot_over_the_fall_and_landing() {
        let mut cs = creature();
        assert_eq!((cs.entries[6].animation, cs.entries[7].animation), (cs.entries[0].animation, cs.entries[0].animation), "the control: the idle's");
        assert!(put_air(&Library::new(), &mut cs, &mut AnimScene::default(), [Pose::Own(37), Pose::Own(22)]));
        for (e, s, sync) in [(6, 37, 0), (7, 22, ONCE)] {
            let (got, src) = (cs.entries[e], cs.entries[s]);
            assert_eq!((got.animation, got.weight, got.sync, got.sound), (src.animation, src.weight, sync, [0, 0]), "entry {e} from {s}");
        }
        assert_eq!((cs.entries[6].priority, cs.entries[7].priority), ([2, 6], [3, 7]), "the fall's and the landing's own priority");
        let done = entries(&cs);
        assert!(!put_air(&Library::new(), &mut cs, &mut AnimScene::default(), [Pose::Own(23); 2]), "put already");
        assert_eq!(entries(&cs), done);
        let mut own = creature();
        own.entries[6].animation = [40, 41];
        let had = entries(&own);
        assert!(!put_air(&Library::new(), &mut own, &mut AnimScene::default(), [Pose::Own(25); 2]), "a fall of its own");
        assert_eq!(entries(&own), had);
    }

    /// The fall and the landing go on a player's character in a body the
    /// table lists, on a server with jumps; an NPC in it, a body not listed
    /// and a server without jumps are as the game made them. The control: by
    /// the body alone, the NPC's would have changed.
    #[test]
    fn the_fall_and_landing_on_players_on_servers_with_jumps() {
        let lib = Library::new();
        let mut cs = creature();
        assert!(grant_air(&lib, &mut cs, &mut AnimScene::default(), PIXIE, true, true));
        assert_eq!((cs.entries[6].animation, cs.entries[7].animation), (cs.entries[37].animation, cs.entries[22].animation), "the pixie's own nearest");
        for (model, player, jumps, why) in [(PIXIE, false, true, "an NPC in the body"), (0x1234_5678, true, true, "a body not listed"), (PIXIE, true, false, "a server without jumps")] {
            let mut cs = creature();
            let had = entries(&cs);
            assert!(!grant_air(&lib, &mut cs, &mut AnimScene::default(), model, player, jumps), "{why}");
            assert_eq!(entries(&cs), had, "{why}: as the game made it");
        }
        let mut npc = creature();
        put_air(&lib, &mut npc, &mut AnimScene::default(), air_poses(PIXIE).unwrap());
        assert_ne!(npc.entries[6].animation, creature().entries[6].animation, "the control");
    }

    fn disc() -> Option<Library> {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no disc at run/eqoa.iso");
            return None;
        };
        let mut lib = Library::new();
        lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").ok()?).ok()?);
        Some(lib)
    }

    fn load(lib: &Library, id: u32) -> (CSprite<Ee>, AnimScene<Ee>) {
        let mut sc = AnimScene::default();
        let cs = load_csprite(lib, lib.lookup(id as i32).expect("on the disc"), &mut sc, &mut AnimCache::new(), &mut Rand(1)).expect("a CSprite");
        (cs, sc)
    }

    /// Every body the table lists, on the disc: its play list has no emote
    /// (each is `SetDefaultEntries`' idle), or has the four, the human's own
    /// clips (the brownie's and the skeleton's), which `put` leaves as they
    /// are; each slot it borrows is one of its own (not a fill), and a body
    /// that takes the human's emotes has the human's arms (the four arm
    /// bones by ref id). Every body the server's races wore on 2026-11-08 is
    /// listed. The control: the human's play list has the four, and the
    /// pixie's arms are not the human's. Needs `run/eqoa.iso`.
    #[test]
    fn the_listed_bodies_on_the_disc() {
        let Some(lib) = disc() else { return };
        let human = Rig::load(&lib, HUMAN).unwrap();
        let arms: Vec<i32> = [12, 13, 21, 22].iter().map(|&n| human.refs[n].unwrap()).collect();
        let hrow = |e| row(&lib, HUMAN, e).unwrap().clips;
        let mut own = Vec::new();
        for &(id, p) in TABLE {
            let (mut cs, mut sc) = load(&lib, id);
            if row(&lib, id, 56).is_some() {
                for e in EMOTES {
                    assert_eq!(row(&lib, id, e).map(|r| r.clips), Some(hrow(e)), "{id:#010x}: the human's emote {e}");
                }
                let had = entries(&cs);
                assert!(!put(&lib, &mut cs, &mut sc, p), "{id:#010x}: its own, left as they are");
                assert_eq!(entries(&cs), had);
                own.push(id);
                continue;
            }
            for e in EMOTES {
                assert!(row(&lib, id, e).is_none(), "{id:#010x} has emote {e} but not 56");
                assert_eq!(cs.entries[e as usize].animation, cs.entries[0].animation, "{id:#010x}: emote {e} is the idle");
            }
            for pose in p {
                match pose {
                    Pose::Own(s) => assert!(row(&lib, id, s).is_some(), "{id:#010x} has no slot {s} of its own"),
                    Pose::Human => {
                        let map = &sc.ref_maps[cs.h.ref_map];
                        assert!(arms.iter().all(|r| map.contains_key(r)), "{id:#010x}: the human's arms");
                    }
                }
            }
        }
        own.sort_unstable();
        assert_eq!(own, vec![BROWNIE, SKELETON], "the bodies with emotes of their own");
        let servers = [
            0x141B_FE0C, 0xFFB1_EA2C, 0x2217_6258, 0xB749_BF57, 0x6C13_08E0, 0x3151_DF13, 0x8EFD_12EA, 0x9112_20E2, 0x31A1_5F17, BROWNIE, PIXIE, 0x7A85_646C, 0xCF41_5561, 0x8214_A32D, 0xCFC4_78EE, 0xC5CB_F4CF, 0xBADC_25C9, 0xBE10_226F, DRYAD, SKELETON, 0x4A4D_88FB, 0xB4D2_D132,
        ];
        for id in servers {
            assert!(poses(id).is_some(), "{id:#010x} not listed");
        }
        assert!(EMOTES.iter().all(|&e| row(&lib, HUMAN, e).is_some()), "the control: the human's");
        let (pixie, sc) = load(&lib, PIXIE);
        assert!(!arms.iter().all(|r| sc.ref_maps[pixie.h.ref_map].contains_key(r)), "the control: the pixie's arms are its own");
    }

    /// The 2002 disc's brownie and skeleton (the 2002 art) have no emotes,
    /// and take the 2002 human's, carried onto them; the Frontiers disc's
    /// keep their own (`the_listed_bodies_on_the_disc`). The control: the
    /// 2002 human's own play list has the four. Needs the 2002 disc
    /// (`$EQOA_2002_ISO`, default the USA disc under
    /// `../decomp-builds/discs/eqoa-usa/`).
    #[test]
    fn the_2002_brownie_and_skeleton() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let y2002 = std::env::var_os("EQOA_2002_ISO").map(std::path::PathBuf::from).or_else(|| {
            std::fs::read_dir(root.join("../decomp-builds/discs/eqoa-usa")).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")))
        });
        let Some(mut iso) = y2002.and_then(|p| iso::Iso::open(&p).ok()) else {
            eprintln!("SKIP: no 2002 disc (set EQOA_2002_ISO)");
            return;
        };
        let mut lib = Library::new();
        lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
        assert!(EMOTES.iter().all(|&e| row(&lib, HUMAN, e).is_some()), "the control: the 2002 human's");
        for id in [BROWNIE, SKELETON] {
            assert!(EMOTES.iter().all(|&e| row(&lib, id, e).is_none()), "{id:#010x}: none on the 2002 disc");
            let (mut cs, mut sc) = load(&lib, id);
            assert!(put(&lib, &mut cs, &mut sc, poses(id).unwrap()), "{id:#010x}");
            for e in EMOTES {
                assert_ne!(cs.entries[e as usize].animation, cs.entries[0].animation, "{id:#010x}: emote {e} carried");
            }
        }
    }

    /// The carried emote's frames against the human's clip for `against`,
    /// node by node (by ref id): whether every rotation is the clip's, and
    /// whether every translation is the clip's (`moved`: the body's own rest
    /// plus the clip's move off the human's rest), and how many frames were
    /// compared.
    fn against(lib: &Library, human: &Human, body: u32, action: i32, against: i32, moved: bool) -> (bool, bool, usize) {
        let (cs, sc) = load(lib, body);
        let rig = Rig::of(&cs, &sc.ref_maps[cs.h.ref_map]);
        let ([a, b], _) = human.carry(lib, action, &cs, &sc, Base::Bind).unwrap();
        let want: Vec<Anim<Ee>> = row(lib, HUMAN, against).unwrap().clips.iter().map(|&c| clip(lib, c).unwrap()).collect();
        let (mut rots, mut trans, mut n) = (true, true, 0);
        for carried in [&a, &b] {
            for &(rid, first) in &carried.nodes {
                let Some((w, wf)) = want.iter().find_map(|w| w.nodes.iter().find(|(r, _)| human.rig.by_ref.get(r) == human.rig.by_ref.get(&rid)).map(|&(_, f)| (w, f))) else { continue };
                let (h, bn) = (human.rig.by_ref[&rid], rig.by_ref[&rid]);
                for f in 0..carried.num_frames {
                    let (got, clip) = (frame_of(carried, first, f), frame_of(w, wf, f));
                    // the clip's quaternions are near unit, not unit (packed)
                    rots &= gq(&got.rot).dot(gq(&clip.rot)).abs() > 0.99999;
                    let t = if moved { gv(&rig.rest[bn].trans) + gv(&clip.trans) - gv(&human.rig.rest[h].trans) } else { gv(&clip.trans) };
                    trans &= (gv(&got.trans) - t).length() < 1e-4;
                    n += 1;
                }
            }
        }
        (rots, trans, n)
    }

    /// On the human's own skeleton (the brownie is built on HumanMale's: the
    /// same forty nodes, bones and bind pose), the carried emote is the
    /// human's clip, frame for frame, node for node. On HumanFemale's (the
    /// dryad's: the same bind turns, bones up to 1% longer or shorter) its
    /// turns are the clip's and its bones keep the dryad's lengths, moving
    /// as the clip moves them. The control: the human's bow is not its wave.
    #[test]
    fn carried_onto_the_humans_skeleton_it_is_the_humans() {
        let Some(lib) = disc() else { return };
        let human = Human::load(&lib).unwrap();
        for e in EMOTES {
            let (rots, trans, n) = against(&lib, &human, BROWNIE, e, e, false);
            assert!(rots && trans && n >= 300, "emote {e} on the brownie: the human's ({rots} {trans} {n})");
            let (rots, trans, n) = against(&lib, &human, DRYAD, e, e, true);
            assert!(rots && trans && n >= 300, "emote {e} on the dryad: the human's turns, its own bones ({rots} {trans} {n})");
        }
        let (rots, _, _) = against(&lib, &human, BROWNIE, 57, 56, false);
        assert!(!rots, "the control");
    }

    /// Of the human's arm nodes a body also has (`also`), the one that
    /// rises highest in the human's own wave; and the head's.
    fn wave_hand(lib: &Library, also: &HashMap<i32, i32>) -> (i32, i32) {
        let (mut cs, mut sc) = load(lib, HUMAN);
        let rig = Rig::of(&cs, &sc.ref_maps[cs.h.ref_map]);
        let mut rand = Rand(1);
        sc.time = 0;
        cs.set_locomotion(0, &sc, &mut rand);
        cs.h.process(0, &sc);
        sc.time = 16;
        cs.set_action(56, &sc, &mut rand);
        let mut t = 16;
        while t < 900 {
            t += 16;
            sc.time = t;
            cs.h.process(0, &sc);
        }
        let high = (11..29).filter(|&n| rig.refs[n].is_some_and(|r| also.contains_key(&r))).max_by(|&a, &b| cs.h.nodes[a].world.m[3].y.0.total_cmp(&cs.h.nodes[b].world.m[3].y.0)).unwrap();
        (rig.refs[high].unwrap(), rig.refs[4].unwrap())
    }

    /// Heights, `ms` into an emote played over the idle: of the nodes ref
    /// ids `a` and `b` name.
    fn heights(cs: &mut CSprite<Ee>, sc: &mut AnimScene<Ee>, action: i32, ms: i64, a: i32, b: i32) -> (f32, f32) {
        let map = sc.ref_maps[cs.h.ref_map].clone();
        let mut rand = Rand(1);
        sc.time = 0;
        cs.set_locomotion(0, sc, &mut rand);
        cs.h.process(0, sc);
        sc.time = 16;
        cs.set_action(action, sc, &mut rand);
        let mut t = 16;
        while t < 16 + ms {
            t += 16;
            sc.time = t;
            cs.set_locomotion(0, sc, &mut rand);
            cs.h.process(0, sc);
        }
        let y = |r: i32| cs.h.nodes[map[&r] as usize].world.m[3].y.0;
        (y(a), y(b))
    }

    /// The hill giant's wave, the human's carried onto it: the hand the
    /// human raises rises over its head, where standing it hangs below; and
    /// when the wave is over the idle is back (the hand below the head
    /// again: the emote played once). The control: the giant without the
    /// table plays its idle for the wave, the hand below its head.
    #[test]
    fn the_hill_giant_waves() {
        let Some(lib) = disc() else { return };
        let (mut cs, mut sc) = load(&lib, HILL_GIANT);
        let (hand, head) = wave_hand(&lib, &sc.ref_maps[cs.h.ref_map]);
        assert!(put(&lib, &mut cs, &mut sc, poses(HILL_GIANT).unwrap()));
        let (h0, top0) = heights(&mut cs, &mut sc, 56, 0, hand, head);
        assert!(h0 < top0, "standing: the hand below the head ({h0} vs {top0})");
        let (mut cs, mut sc) = load(&lib, HILL_GIANT);
        put(&lib, &mut cs, &mut sc, poses(HILL_GIANT).unwrap());
        let (h, top) = heights(&mut cs, &mut sc, 56, 700, hand, head);
        assert!(h > top, "mid-wave: the hand over the head ({h} vs {top})");
        let (mut cs, mut sc) = load(&lib, HILL_GIANT);
        put(&lib, &mut cs, &mut sc, poses(HILL_GIANT).unwrap());
        let (h, top) = heights(&mut cs, &mut sc, 56, 3000, hand, head);
        assert!(h < top, "over: the idle back ({h} vs {top})");
        let (mut cs, mut sc) = load(&lib, HILL_GIANT);
        let (h, top) = heights(&mut cs, &mut sc, 56, 700, hand, head);
        assert!(h < top, "the control: without the table the idle ({h} vs {top})");
    }

    /// Every body the table lists, on the disc, through `put_air`: the four
    /// with a fall and a landing of their own (the brownie's, the dryad's,
    /// the orc's and the skeleton's) are left as they are; each of the others
    /// had neither (its entries the idle's) and takes both, the human's carried
    /// onto a body with the human's arms, or its own nearest (each one of its
    /// own slots, not a fill). No player race's body is listed, and each has
    /// its own. The control: the human's play list has the two. Needs
    /// `run/eqoa.iso`.
    #[test]
    fn the_bodies_with_no_fall_take_the_humans() {
        let Some(lib) = disc() else { return };
        let human = Rig::load(&lib, HUMAN).unwrap();
        let arms: Vec<i32> = [12, 13, 21, 22].iter().map(|&n| human.refs[n].unwrap()).collect();
        let (mut own, mut took) = (Vec::new(), Vec::new());
        for &(id, _) in TABLE {
            let (mut cs, mut sc) = load(&lib, id);
            let p = air_poses(id).unwrap();
            if row(&lib, id, 6).is_some() {
                assert!(row(&lib, id, 7).is_some(), "{id:#010x}: a fall but no landing");
                let had = entries(&cs);
                assert!(!put_air(&lib, &mut cs, &mut sc, p), "{id:#010x}: its own, left as they are");
                assert_eq!(entries(&cs), had);
                own.push(id);
                continue;
            }
            for e in AIR {
                assert!(row(&lib, id, e).is_none(), "{id:#010x} has {e} but not 6");
                assert_eq!(cs.entries[e as usize].animation, cs.entries[0].animation, "{id:#010x}: {e} is the idle");
            }
            for pose in p {
                match pose {
                    Pose::Own(s) => assert!(row(&lib, id, s).is_some(), "{id:#010x} has no slot {s} of its own"),
                    Pose::Human => assert!(arms.iter().all(|r| sc.ref_maps[cs.h.ref_map].contains_key(r)), "{id:#010x}: the human's arms"),
                }
            }
            assert!(put_air(&lib, &mut cs, &mut sc, p), "{id:#010x}");
            for e in AIR {
                assert_ne!(cs.entries[e as usize].animation, cs.entries[0].animation, "{id:#010x}: {e} now the table's");
            }
            assert_eq!((cs.entries[6].sync, cs.entries[7].sync), (0, ONCE), "{id:#010x}");
            took.push(id);
        }
        own.sort_unstable();
        assert_eq!(own, vec![BROWNIE, SKELETON, DRYAD, 0x8214_A32D], "the bodies with a fall of their own (the orc's last)");
        assert_eq!(took.len(), TABLE.len() - 4);
        for r in 0..10 {
            for sex in 0..2 {
                let id = crate::pregame::client::races::get_race(r).model_id(sex);
                assert!(air_poses(id).is_none(), "race {r} sex {sex}: not listed");
                assert!(row(&lib, id, 6).is_some() && row(&lib, id, 7).is_some(), "race {r} sex {sex}: its own");
            }
        }
        assert!(AIR.iter().all(|&e| row(&lib, HUMAN, e).is_some()), "the control: the human's");
    }

    /// The arms' spread (the most two of the human's four arm bones are apart
    /// across) and the head's height, `ms` into gait `loco` from the idle.
    fn spread_and_head(lib: &Library, cs: &mut CSprite<Ee>, sc: &mut AnimScene<Ee>, loco: i32, ms: i64) -> (f32, f32) {
        let human = Rig::load(lib, HUMAN).unwrap();
        let map = sc.ref_maps[cs.h.ref_map].clone();
        let mut rand = Rand(1);
        sc.time = 0;
        cs.set_locomotion(0, sc, &mut rand);
        cs.h.process(0, sc);
        let mut t = 0;
        while t < ms {
            t += 16;
            sc.time = t;
            cs.set_locomotion(loco, sc, &mut rand);
            cs.h.process(0, sc);
        }
        let at = |n: usize| {
            let m = &cs.h.nodes[map[&human.refs[n].unwrap()] as usize].world.m[3];
            (m.x.0, m.y.0, m.z.0)
        };
        let arms: Vec<(f32, f32, f32)> = [12, 13, 21, 22].iter().map(|&n| at(n)).collect();
        let spread = arms.iter().flat_map(|a| arms.iter().map(move |b| (a.0 - b.0).hypot(a.2 - b.2))).fold(0.0, f32::max);
        (spread, at(4).1)
    }

    /// The hill giant (`TABLE`'s, its fall and landing the idle's) falls and
    /// lands as the human does: falling, its arms spread out from its sides;
    /// landing, its head comes down (a crouch). The control: without the table
    /// the giant falls and lands as it stands, its idle.
    #[test]
    fn the_hill_giant_falls_and_lands() {
        let Some(lib) = disc() else { return };
        let measure = |put: bool, loco: i32| {
            let (mut cs, mut sc) = load(&lib, HILL_GIANT);
            if put {
                assert!(put_air(&lib, &mut cs, &mut sc, air_poses(HILL_GIANT).unwrap()));
            }
            spread_and_head(&lib, &mut cs, &mut sc, loco, 300)
        };
        let idle = measure(true, 0);
        let (fall, land) = (measure(true, 6), measure(true, 7));
        assert!(fall.0 > idle.0 * 1.3, "falling, the arms out: {fall:?} against the idle's {idle:?}");
        assert!(land.1 < idle.1 * 0.92, "landing, the head down: {land:?} against the idle's {idle:?}");
        let (fall0, land0) = (measure(false, 6), measure(false, 7));
        assert!(fall0.0 < idle.0 * 1.1 && land0.1 > idle.1 * 0.97, "the control: as it stands, {fall0:?} {land0:?} against {idle:?}");
    }

    /// Every CSprite on the disc through `grant`: for an NPC, and on a
    /// server with no races, no sprite's entries change (byte for byte); for
    /// a player on a server with races, exactly the bodies the table lists
    /// change, but the two whose emotes are their own (the brownie's, the
    /// skeleton's), and every player race's body stays as it was. The control: the table's own poses put on every body that
    /// has the human's arms and no emote of its own change those too, so
    /// the sweep sees a change where there is one. Needs `run/eqoa.iso`.
    #[test]
    fn only_the_listed_bodies_change() {
        let Some(lib) = disc() else { return };
        let c = lib.file(0);
        let mut bodies: Vec<u32> = c.dict.iter().filter(|(_, n)| c.node(**n).ty == 0x2700).map(|(&k, _)| k as u32).collect();
        bodies.sort_unstable();
        let listed = races(&TABLE.iter().map(|(b, _)| *b).collect::<Vec<_>>());
        crate::racelabel::clear();
        let changed = |player: bool, list: Option<&Creation>, everyone: bool| {
            let mut out = Vec::new();
            for &id in &bodies {
                let mut sc = AnimScene::default();
                let Some(mut cs) = load_csprite(&lib, lib.lookup(id as i32).unwrap(), &mut sc, &mut AnimCache::new(), &mut Rand(1)) else { continue };
                let had = entries(&cs);
                if everyone {
                    put(&lib, &mut cs, &mut sc, [Pose::Human; 4]);
                } else {
                    grant(&lib, &mut cs, &mut sc, id, player, list);
                }
                if entries(&cs) != had {
                    out.push(id);
                }
            }
            out
        };
        assert_eq!(changed(false, Some(&listed), false), Vec::<u32>::new(), "an NPC's: none");
        assert_eq!(changed(true, None, false), Vec::<u32>::new(), "a server with no races: none");
        let mut want: Vec<u32> = TABLE.iter().map(|(b, _)| *b).filter(|&b| row(&lib, b, 56).is_none()).collect();
        want.sort_unstable();
        assert_eq!(want.len(), TABLE.len() - 2, "the brownie's and skeleton's are their own");
        assert_eq!(changed(true, Some(&listed), false), want, "a player's: the listed bodies with no emote of their own");
        let every = changed(true, None, true);
        assert!(every.len() > want.len() + 100, "the control: {} bodies change with the human's emotes on all", every.len());
    }

    /// The gargoyles' six bodies (the granite one the race's own, the shadow
    /// one petition 1456's) and the Jal'Raeth's three.
    const GARGOYLES: [u32; 6] = [0xB4D2_D132, 0x4E2A_945C, 0xAEC5_D9F6, 0x0013_55C8, 0x6D95_9FD9, 0x9304_BAB5];
    const JALRAETH: [u32; 3] = [0xF51B_AB09, 0x4B24_FF7E, 0x491F_6347];

    /// Every body a server's race wears (each one the table lists and every
    /// other its options offer, the helm seats' families with the fungusmen's
    /// and the Jal'Raeth's), on the disc, through `put_swim`: 129 have a swim
    /// and a treading of their own (entries 12 and 13), left as they are; the
    /// gargoyles' six, the Jal'Raeth's three, the pixie's and the fairy's have
    /// neither (each is the idle), and but the last two (`SWIM_NONE`) take the
    /// human's carried, one clip looping for both, the human's swim. The
    /// control: the human's play list has the two, the same clip. Needs
    /// `run/eqoa.iso`.
    #[test]
    fn the_bodies_with_no_swim_take_the_humans() {
        let Some(lib) = disc() else { return };
        let mut bodies: Vec<u32> = TABLE.iter().map(|(b, _)| *b).collect();
        for s in crate::slotless::HEAD_SEATS {
            bodies.extend_from_slice(s.bodies);
        }
        bodies.extend(crate::slotless::HEADLESS.iter().chain(&JALRAETH));
        bodies.sort_unstable();
        bodies.dedup();
        let (mut own, mut none) = (0, Vec::new());
        for id in bodies {
            let (mut cs, mut sc) = load(&lib, id);
            if row(&lib, id, 12).is_some() {
                assert!(row(&lib, id, 13).is_some(), "{id:#010x}: a swim but no treading");
                let had = entries(&cs);
                assert!(!put_swim(&lib, &mut cs, &mut sc, [Pose::Human; 2]), "{id:#010x}: its own, left as it is");
                assert_eq!(entries(&cs), had);
                own += 1;
                continue;
            }
            none.push(id);
            for e in SWIM {
                assert!(row(&lib, id, e).is_none() && cs.entries[e as usize].animation == cs.entries[0].animation, "{id:#010x}: {e} is the idle");
            }
            if SWIM_NONE.contains(&id) {
                assert!(swim_poses(id).is_none(), "{id:#010x}: left hovering");
                continue;
            }
            let anims = sc.anims.len();
            assert!(put_swim(&lib, &mut cs, &mut sc, [Pose::Human; 2]), "{id:#010x}");
            let (swim, tread) = (cs.entries[12], cs.entries[13]);
            assert!(swim.animation != cs.entries[0].animation && swim.animation == tread.animation, "{id:#010x}: the human's swim, both");
            assert_eq!((swim.sync, tread.sync, swim.sound, sc.anims.len()), (0, 0, [0, 0], anims + 1), "{id:#010x}: looping, silent, one clip (one layer)");
        }
        let mut want: Vec<u32> = GARGOYLES.iter().chain(&JALRAETH).chain(SWIM_NONE).copied().collect();
        want.sort_unstable();
        assert_eq!(none, want, "the bodies with no swim");
        assert_eq!(own, 129, "the bodies with their own");
        let (h12, h13) = (row(&lib, HUMAN, 12).unwrap(), row(&lib, HUMAN, 13).unwrap());
        assert_eq!(h12.clips, h13.clips, "the control: the human's swim and treading, one clip");
    }

    /// The model-space places of the human's head (4) and pelvis (0) on
    /// `cs`, by their ref ids, `ms` into gait `loco` from the idle.
    fn head_and_hips(lib: &Library, cs: &mut CSprite<Ee>, sc: &mut AnimScene<Ee>, loco: i32, ms: i64) -> (Vec3, Vec3) {
        let human = Rig::load(lib, HUMAN).unwrap();
        let map = sc.ref_maps[cs.h.ref_map].clone();
        let mut rand = Rand(1);
        sc.time = 0;
        cs.set_locomotion(0, sc, &mut rand);
        cs.h.process(0, sc);
        let mut t = 0;
        while t < ms {
            t += 16;
            sc.time = t;
            cs.set_locomotion(loco, sc, &mut rand);
            cs.h.process(0, sc);
        }
        let at = |n: usize| {
            let m = &cs.h.nodes[map[&human.refs[n].unwrap()] as usize].world.m[3];
            Vec3::new(m.x.0, m.y.0, m.z.0)
        };
        (at(4), at(0))
    }

    /// A gargoyle swims as the human does (petition 1456: "gargs fly in the
    /// water instead of swim"): through the swim and the treading, its body
    /// lies along the water, its head as far forward of its hips as the
    /// human's is (within a fifth) and no higher over them than a third of
    /// that; the granite gargoyle (the race's own) and the shadow one alike.
    /// The control: as the game has it (its swim the idle), it hangs upright
    /// in the water, its head over its hips, as it hovers.
    #[test]
    fn a_gargoyle_swims() {
        let Some(lib) = disc() else { return };
        let lying = |id: u32, put: bool, loco: i32| {
            let (mut cs, mut sc) = load(&lib, id);
            if put {
                assert!(put_swim(&lib, &mut cs, &mut sc, [Pose::Human; 2]), "{id:#010x}");
            }
            let mut worst = (f32::MAX, 0.0f32);
            for ms in [300, 600, 900, 1200] {
                let (head, hips) = head_and_hips(&lib, &mut cs, &mut sc, loco, ms);
                let d = head - hips;
                worst = (worst.0.min(d.with_y(0.0).length() / d.length()), worst.1.max(d.y.abs() / d.length()));
            }
            worst
        };
        let human = lying(HUMAN, false, 12);
        assert!(human.0 > 0.9, "the human's swim lies along the water: {human:?}");
        for id in [0xB4D2_D132u32, 0x6D95_9FD9] {
            for loco in [12, 13] {
                let swims = lying(id, true, loco);
                assert!(swims.0 > 0.9 && swims.1 < 0.35, "{id:#010x} gait {loco}: lying along the water, {swims:?} (the human's {human:?})");
                let hovers = lying(id, false, loco);
                assert!(hovers.1 > 0.9, "the control, {id:#010x} gait {loco}: upright, its head over its hips, {hovers:?}");
            }
        }
    }

    /// The swim goes on a player's character in a body the table lists or
    /// another its race's options offer (the shadow gargoyle, petition 1456's
    /// Ghastly's, its race's own the granite one), on a server with races of
    /// its own, and on nothing else: not on an NPC in it, not with no races,
    /// not on the shadow gargoyle with no options to say whose body it is, not
    /// on a body with a swim of its own (the hill giant's, left as it is) nor
    /// on the pixie (`SWIM_NONE`). The control: the shadow gargoyle's swim
    /// changed only through `grant_swim`'s first case. Needs `run/eqoa.iso`.
    #[test]
    fn the_swim_goes_on_a_races_players() {
        let Some(lib) = disc() else { return };
        const SHADOW: u32 = 0x6D95_9FD9;
        let step = |value: u32| net::raceopts::Step { name: format!("{value:08X}"), value, size: 1000 };
        let options = net::raceopts::Options {
            races: vec![(29, vec![net::raceopts::Row { key: "body".into(), label: "BODY".into(), kind: net::raceopts::Kind::Body, parts: vec![], male: GARGOYLES.iter().map(|&b| step(b)).collect(), female: vec![] }])],
        };
        let listed = races(&[GARGOYLES[0], HILL_GIANT, PIXIE]);
        crate::racelabel::clear();
        let swims = |id: u32, player: bool, list: Option<&Creation>| {
            let (mut cs, mut sc) = load(&lib, id);
            let had = cs.entries[12].animation;
            let granted = grant_swim(&lib, &mut cs, &mut sc, id, player, list);
            (granted, cs.entries[12].animation != had)
        };
        crate::raceopts::set_options(Some(&options));
        assert_eq!(swims(SHADOW, true, Some(&listed)), (true, true), "a player's shadow gargoyle swims");
        assert_eq!(swims(GARGOYLES[0], true, Some(&listed)), (true, true), "a player's granite gargoyle swims");
        assert_eq!(swims(SHADOW, false, Some(&listed)), (false, false), "an NPC's: as the game has it");
        assert_eq!(swims(SHADOW, true, None), (false, false), "a server with no races: as the game has it");
        assert_eq!(swims(HILL_GIANT, true, Some(&listed)), (true, false), "a body with its own swim, left as it is");
        assert_eq!(swims(PIXIE, true, Some(&listed)), (false, false), "the pixie keeps its hover");
        crate::raceopts::set_options(None);
        assert_eq!(swims(SHADOW, true, Some(&listed)), (false, false), "the control: with no options the shadow gargoyle is no race's body");
    }

    /// Another body a race's options offer falls and lands as its race's own
    /// body does, on a server with jumps (`air_poses` by `race_of`, the body
    /// worn): the shadow gargoyle (the gargoyles', petition 1456's Ghastly's)
    /// and a goblin king (the goblins'), whose fall and landing are their idle,
    /// take the human's carried, the fall looping and the landing once; the
    /// brownie elder (the brownies'), which falls and lands of its own, is
    /// granted and left as it is; an NPC in either keeps the game's, as does
    /// each with no options to say whose body it is. The control: by the body
    /// worn alone, in `TABLE` (the lookup before), neither is listed, and each
    /// stands in the air, its fall and landing the idle. Needs `run/eqoa.iso`.
    #[test]
    fn an_offered_body_falls_and_lands_as_its_race() {
        let Some(lib) = disc() else { return };
        const SHADOW: u32 = 0x6D95_9FD9;
        const KING: u32 = 0x2F34_B867;
        const ELDER: u32 = 0x4946_8331;
        let step = |value: u32| net::raceopts::Step { name: format!("{value:08X}"), value, size: 1000 };
        let body_row = |bodies: &[u32]| net::raceopts::Row { key: "body".into(), label: "BODY".into(), kind: net::raceopts::Kind::Body, parts: vec![], male: bodies.iter().map(|&b| step(b)).collect(), female: vec![] };
        let options = net::raceopts::Options { races: vec![(29, vec![body_row(&GARGOYLES)]), (13, vec![body_row(&[0x6C13_08E0, KING])]), (21, vec![body_row(&[BROWNIE, ELDER])])] };
        // granted, the fall and landing the idle as loaded, changed, their syncs
        let falls = |id: u32, player: bool| {
            let (mut cs, mut sc) = load(&lib, id);
            let idle = cs.entries[0].animation;
            let was = (cs.entries[6].animation, cs.entries[7].animation);
            let granted = grant_air(&lib, &mut cs, &mut sc, id, player, true);
            let now = (cs.entries[6].animation, cs.entries[7].animation);
            (granted, was == (idle, idle), now != was, (cs.entries[6].sync, cs.entries[7].sync))
        };
        crate::raceopts::set_options(None);
        for id in [SHADOW, KING] {
            assert!(poses(id).is_none(), "the control, {id:#010x}: by the body worn alone, not in the table");
            let (granted, idle, changed, _) = falls(id, true);
            assert!(!granted && idle && !changed, "the control, {id:#010x}: standing in the air, its fall and landing the idle");
        }
        crate::raceopts::set_options(Some(&options));
        for id in [SHADOW, KING] {
            assert_eq!(air_poses(id), Some([Pose::Human; 2]), "{id:#010x}: as its race's own body");
            let (granted, idle, changed, sync) = falls(id, true);
            assert!(granted && idle && changed, "{id:#010x}: the human's fall and landing over its idle");
            assert_eq!(sync, (0, ONCE), "{id:#010x}: the fall looping, the landing once");
            let (granted, _, changed, _) = falls(id, false);
            assert!(!granted && !changed, "{id:#010x}: an NPC in it keeps the game's");
        }
        assert!(row(&lib, ELDER, 6).is_some() && row(&lib, ELDER, 7).is_some(), "the brownie elder's own fall and landing");
        let (granted, idle, changed, _) = falls(ELDER, true);
        assert!(granted && !idle && !changed, "the brownie elder: its own fall and landing, left as they are");
        crate::raceopts::set_options(None);
    }
}
