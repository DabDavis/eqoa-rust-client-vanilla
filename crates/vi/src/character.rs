//! Characters out of the ESF: `ParseCSpriteObj` and the parsers under it
//! (`ParseHSpriteAnimObj`, `ParseHSpriteHierarchy`, `ParseRefMap`,
//! `ParseHSpriteTriggers`, `ParseCSpriteSkinList`, `ParseCSpritePlayList`),
//! then `SetDefaultEntries`, `SetAnimPriorities` and `SetDefaults`, as the
//! game builds a character.

use crate::csprite::{CSprite, Rand};
use crate::fpu::{F, Model};
use crate::hsprite::{Anim, AnimScene, Frame, Frames, HSprite, Trigger};
use crate::quat::Quat;
use crate::vect::Vect3;
use esf::{Container, Library, NodeId, Ref};
use std::collections::HashMap;

struct Rd<'a> {
    d: &'a [u8],
    p: usize,
}

impl Rd<'_> {
    fn i(&mut self) -> i32 {
        let v = i32::from_le_bytes(self.d[self.p..self.p + 4].try_into().unwrap());
        self.p += 4;
        v
    }
    fn f<M: Model>(&mut self) -> F<M> {
        F::new(f32::from_bits(self.i() as u32))
    }
    fn s(&mut self) -> i16 {
        let v = i16::from_le_bytes(self.d[self.p..self.p + 2].try_into().unwrap());
        self.p += 2;
        v
    }
}

/// A clip record (0x2600).
pub fn parse_anim<M: Model>(body: &[u8], version: i16) -> Anim<M> {
    let mut r = Rd { d: body, p: 0 };
    let _id = r.i();
    let format = if version >= 2 { r.i() } else { 0 };
    let num_nodes = r.i();
    let num_frames = r.i();
    let num_triggers = if version >= 3 { r.i() } else { 0 };
    let play_rate = r.f();
    let (play_speed, playback_type) = if version >= 1 { (r.f(), r.i()) } else { (F::ONE, 0) };
    let mut nodes = Vec::new();
    let mut full = Vec::new();
    let mut pack = Vec::new();
    for j in 0..num_nodes {
        nodes.push((r.i(), j * num_frames));
        for _ in 0..num_frames {
            if format == 0 {
                let rot = Quat { x: r.f(), y: r.f(), z: r.f(), w: r.f() };
                let scale = r.f();
                let trans = Vect3::new(r.f(), r.f(), r.f());
                full.push(Frame { rot, scale, trans });
            } else {
                pack.push([r.s(), r.s(), r.s(), r.s(), r.s(), r.s(), r.s(), r.s()]);
            }
        }
    }
    let triggers = (0..num_triggers).map(|_| (r.i(), r.f())).collect();
    Anim {
        num_nodes,
        num_frames,
        play_rate,
        play_speed,
        playback_type,
        nodes,
        frames: if format == 0 { Frames::Full(full) } else { Frames::Pack(pack) },
        triggers,
    }
}

/// A ref map (0x5000): ref id -> node; the first of a repeated key stays.
pub fn parse_ref_map(body: &[u8]) -> HashMap<i32, i32> {
    let mut r = Rd { d: body, p: 0 };
    let _id = r.i();
    let n = r.i();
    let mut m = HashMap::new();
    for _ in 0..n {
        let (k, v) = (r.i(), r.i());
        m.entry(k).or_insert(v);
    }
    m
}

/// The hierarchy (0x2400) into `h`: nodes, bind poses, LOD, fix scales.
pub fn parse_hierarchy<M: Model>(h: &mut HSprite<M>, body: &[u8], version: i16) {
    let mut r = Rd { d: body, p: 0 };
    if version != 0 {
        let n = r.i();
        h.lod_dist = (0..n).map(|_| r.f()).collect();
    }
    let num = r.i();
    for _ in 0..num {
        let parent = r.i();
        let j = if parent != -1 { h.add(parent) } else { h.add_root() };
        let q = Quat { x: r.f(), y: r.f(), z: r.f(), w: r.f() };
        let s = r.f();
        let t = Vect3::new(r.f(), r.f(), r.f());
        if j < 0 {
            continue;
        }
        let j = j as usize;
        h.set_node_bind_pose(j, q, s, t);
        if version != 0 {
            h.nodes[j].lod = r.i();
        }
        h.nodes[j].fix_scale = if version >= 2 { Vect3::new(r.f(), r.f(), r.f()) } else { Vect3::of(1.0, 1.0, 1.0) };
    }
}

/// Loads clips on demand into the scene, once per dictionary entry.
pub struct AnimCache {
    pub index: HashMap<Ref, usize>,
}

impl AnimCache {
    pub fn new() -> Self {
        AnimCache { index: HashMap::new() }
    }

    pub fn get<M: Model>(&mut self, lib: &Library, dict_id: i32, sc: &mut AnimScene<M>) -> Option<usize> {
        let r = lib.lookup(dict_id)?;
        let c = lib.file(r.file);
        if c.node(r.node).ty != 0x2600 {
            return None;
        }
        if let Some(&i) = self.index.get(&r) {
            return Some(i);
        }
        sc.anims.push(parse_anim(c.body(r.node), c.node(r.node).version));
        let i = sc.anims.len() - 1;
        self.index.insert(r, i);
        Some(i)
    }
}

impl Default for AnimCache {
    fn default() -> Self {
        Self::new()
    }
}

fn i32_at(d: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}

/// `ParseCSpriteObj` and the set-up after it. `sprite` is the 0x2700 record.
pub fn load_csprite<M: Model>(
    lib: &Library,
    sprite: Ref,
    sc: &mut AnimScene<M>,
    cache: &mut AnimCache,
    rand: &mut Rand,
) -> Option<CSprite<M>> {
    let c: &Container = lib.file(sprite.file);
    let id: NodeId = sprite.node;
    if c.node(id).ty != 0x2700 {
        return None;
    }
    let version = c.node(id).version;
    let hdr = c.child(id, 0x2710)?;
    let (hv, hd) = (c.node(hdr).version, c.body(hdr));
    let mut r = Rd { d: hd, p: 4 + 24 };
    let skel = if hv >= 1 { r.i() } else { 0 };
    let scale: F<M> = if hv >= 2 { r.f() } else { F::ONE };
    let (race, sex) = if hv >= 3 { (r.i(), r.i()) } else { (-1, 0) };

    let children = &c.node(id).children;
    // The skeleton's ref map is the 0x5000 AFTER the hierarchy (the one
    // before it is the CSprite's own, for its materials).
    let hier = c.child(id, 0x2400)?;
    let pos = children.iter().position(|&k| k == hier)?;
    let refmap = children[pos..].iter().copied().find(|&k| c.node(k).ty == 0x5000)?;
    sc.ref_maps.push(parse_ref_map(c.body(refmap)));
    let mut h = HSprite::new(sc.ref_maps.len() - 1);
    parse_hierarchy(&mut h, c.body(hier), c.node(hier).version);
    if version >= 4
        && let Some(t) = c.child(id, 0x2450)
    {
        let d = c.body(t);
        h.triggers = (0..i32_at(d, 0).max(0) as usize).map(|k| Trigger { id: i32_at(d, 4 + k * 4), ..Default::default() }).collect();
    }
    // The clips the file carries (0x2610) are registered first, as the
    // parse order does; the play list then names them by DictID.
    if let Some(arr) = c.child(id, 0x2610) {
        for &k in &c.node(arr).children {
            let d = c.body(k);
            if c.node(k).ty == 0x2600 && d.len() >= 4 {
                cache.get(lib, i32_at(d, 0), sc);
            }
        }
    }
    let mut cs = CSprite::new(h, skel, race, sex, scale);
    if let Some(sl) = c.child(id, 0x2900) {
        let d = c.body(sl);
        for k in 0..i32_at(d, 0).max(0) as usize {
            let (sid, slot) = (i32_at(d, 4 + k * 8), i32_at(d, 8 + k * 8));
            if slot == 0 && sid != 0 {
                cs.body = lib.lookup(sid).map_or(-1, |r| r.node as i32);
            }
        }
    }
    let pl = c.child(id, 0x2910)?;
    let (pv, pd) = (c.node(pl).version, c.body(pl));
    let mut r = Rd { d: pd, p: 0 };
    for _ in 0..r.i() {
        let a0 = r.i();
        let a1 = if pv != 0 { r.i() } else { 0 };
        let slot = r.i();
        let weight = r.f();
        let sync = r.i();
        if pv >= 2 {
            let n = if pv >= 3 { 3 } else { 2 };
            for _ in 0..2 * n {
                r.i();
            }
        }
        if !(0..66).contains(&slot) {
            continue;
        }
        let anim = |a: i32, sc: &mut AnimScene<M>, cache: &mut AnimCache| {
            if a == 0 { -1 } else { cache.get(lib, a, sc).map_or(-1, |i| i as i32) }
        };
        let e = &mut cs.entries[slot as usize];
        e.animation = [anim(a0, sc, cache), anim(a1, sc, cache)];
        e.weight = weight;
        e.sync = sync;
    }
    cs.set_default_entries();
    cs.set_anim_priorities();
    if cs.entries[0].animation[0] < 0 {
        return None;
    }
    cs.set_defaults(sc, rand);
    Some(cs)
}
