//! `VIScene`'s collision world: `VIWorld` and its zones (`viworld.cpp`,
//! `vizone.cpp`), the rooms, sprites and actors in them (`viscene.cpp`), loaded
//! from the world file the way `VIESFParse` loads it (`ParseWorldBase`,
//! `ParseZoneBaseObj`, `ParseZoneRoom`, `ParseZoneRoomActorsObj`, the
//! `Replace*Proxy*` pair and `UpdateActor`), and the three walks over them:
//! the swept-sphere collide, the environment ray, and actor insertion.
//!
//! Not yet: `VIHSprite` and `VICSprite` collide as nothing here (the first
//! needs the pose evaluator, the second is a capsule); the ray collide.

use crate::character::{AnimCache, load_hsprite};
use crate::coll::{CollSphere, EnvRay};
use crate::hsprite::{AnimScene, HSprite};
use crate::collide::{Collide, CollBuffer, cull_sphere};
use crate::fpu::{F, Model, clamp, f};
use crate::geom::{BBox, Matrix44, Plane, Sphere};
use crate::scene::{Filter, FilterKind, Scene};
use crate::vect::Vect3;
use esf::{Container, Library, NodeId, Ref};
use std::collections::HashMap;

/// A resolved attachment: (scene sprite, node; -1 a skin).
pub type SpriteAttachment = (usize, i32);

#[derive(Clone, Debug, Default)]
pub struct Node<M: Model> {
    pub plane: Plane<M>,
    pub front: u32,
    pub back: u32,
}

#[derive(Clone, Debug, Default)]
pub struct Room {
    /// The DictID of the room's own sprite (`m_p28`), and what it resolved to.
    pub sprite_id: u32,
    pub sprite: Option<usize>,
    /// Pass stamps: `m_p20` (per world walk), `m_p24` (per scene pass).
    pub stamp20: i32,
    pub stamp24: i32,
    /// The actors that occupy it, in insertion order (`m_p68` list).
    pub actors: Vec<usize>,
    /// The zone's own actor placements, as parsed (`m_a3C`).
    pub placements: Vec<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Zone<M: Model> {
    pub nodes: Vec<Node<M>>,
    /// (room, environment flags) per leaf.
    pub leaves: Vec<(i32, u32)>,
    pub pretrans: Vec<Vect3<M>>,
    pub rooms: Vec<Room>,
}

#[derive(Clone, Debug)]
pub enum SpriteKind {
    /// SimpleSprite / SubSprite: a collision buffer, if it has one.
    Simple(Option<usize>),
    /// (member sprite, angles, scale, translation), as `VIGroupMember` holds
    /// them: `m_loc` is handed to `Transform` as the ANGLES and `m_hpr` as the
    /// translation, and the data agrees (every member's first triple is 0).
    Group(Vec<(usize, [f32; 3], f32, [f32; 3])>),
    Lod(Vec<usize>),
    /// An animated HSprite: index into `World::hsprites`.
    H(usize),
    /// `VISoundSprite` (0xB100): a positional sound, its wave or XM record,
    /// the record's volume, and its attenuation and pan settings (the parser
    /// leaves them at the constructor's 0 and true).
    Sound { sound: Option<Ref>, volume: f32, attenuation: i32, pan: bool },
    /// `VIStreamAudioSprite` (0x2E00): a box the listener stands in, the BGM
    /// it names (relative to `Music\`), whether it loops and its priority.
    Stream { path: String, looping: bool, priority: i32 },
    /// Collides as nothing (flora, sounds, lights, emitters; and for now
    /// CSprites, whose collision is a capsule).
    None,
}

#[derive(Clone, Debug)]
pub struct SceneSprite<M: Model> {
    pub kind: SpriteKind,
    pub bbox: BBox<M>,
    pub bsphere: Sphere<M>,
    /// `VISpriteType` 5 is VICSprite, which the walk filters skip.
    pub sprite_type: i32,
}

#[derive(Clone, Debug)]
pub struct Actor<M: Model> {
    pub location: Vect3<M>,
    pub hpr: Vect3<M>,
    pub scale: F<M>,
    pub sprite_id: u32,
    pub sprite: Option<usize>,
    pub object_to_world: Matrix44<M>,
    pub bound: Sphere<M>,
    /// `m_p40`: 4 proxy (unresolved), 8 static, 0x10 hidden.
    pub flags: u32,
    /// `m_p80`, the scene pass that last visited it.
    pub stamp80: i32,
}

pub struct World<M: Model> {
    pub nodes: Vec<Node<M>>,
    /// The zone each world leaf is.
    pub leaves: Vec<i32>,
    pub zones: Vec<Zone<M>>,
    /// `VIWorldTree::m_state`, the walk stamp.
    pub state: i32,
    /// `VIScene::m_stamp`, the pass stamp.
    pub stamp: i32,
    pub sprites: Vec<SceneSprite<M>>,
    sprite_of: HashMap<Ref, usize>,
    /// The record each scene sprite was built from.
    pub sprite_refs: Vec<Ref>,
    pub buffers: Vec<CollBuffer<M>>,
    pub actors: Vec<Actor<M>>,
    pub collide: Collide<M>,
    /// `VIScene::GetTime`, ms; the caller advances it.
    pub time: i64,
    /// The animated HSprites, one per distinct sprite (`VIHSprite` does not
    /// override `GetInstType`, so placements share it), with their
    /// attachments as (world sprite, node; -1 a skin).
    pub hsprites: Vec<(HSprite<M>, Vec<SpriteAttachment>)>,
    pub anims: AnimScene<M>,
    anim_cache: AnimCache,
    /// The terrain-profile grid (`VIWorld::m_regions`), if the file has one.
    pub grid: Option<Grid<M>>,
    /// Each zone's proxy bounds (record 0x8210), what streaming measures.
    pub zone_bounds: Vec<BBox<M>>,
}

/// `VIScene::Init`'s streaming radii (`m_p2A48[1]`, `[0]`): a zone loads
/// when its bounds come within the inner one and is let go past the outer.
pub const STREAM_INNER: f32 = 900.0;
pub const STREAM_OUTER: f32 = 950.0;

/// The world header's grid (record 0x8220) and the region bytes over it
/// (0x8240, version 2 up): one terrain-profile index per sub-cell.
#[derive(Clone, Debug)]
pub struct Grid<M: Model> {
    pub rows: i32,
    pub cols: i32,
    pub subdiv: i32,
    pub cell_size: F<M>,
    pub origin: Vect3<M>,
    pub regions: Vec<u8>,
}

/// `VITerrainProfileBlend`: four profile indices and their weights.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProfileBlend<M: Model> {
    pub index: [i32; 4],
    pub weight: [F<M>; 4],
}

impl<M: Model> Grid<M> {
    /// Out of the world base record (0x8200).
    pub fn read(c: &Container, base: NodeId) -> Option<Self> {
        let h = c.child(base, 0x8220)?;
        let d = c.body(h);
        let v = c.node(h).version;
        let mut p = if v != 0 { 4 } else { 0 };
        p += 24; // the bbox
        let (rows, cols) = (rd_i(d, p), rd_i(d, p + 4));
        p += 8;
        let subdiv = if v != 0 {
            p += 4;
            rd_i(d, p - 4)
        } else {
            4
        };
        let cell_size = F::new(rd_f(d, p));
        let origin = v3(d, p + 4);
        let n = (rows * cols * subdiv * subdiv).max(0) as usize;
        let mut regions = vec![0u8; n];
        if c.node(base).version >= 2
            && let Some(r) = c.child(base, 0x8240)
        {
            let b = c.body(r);
            let k = n.min(b.len());
            regions[..k].copy_from_slice(&b[..k]);
        }
        Some(Grid { rows, cols, subdiv, cell_size, origin, regions })
    }

    /// `VIWorld::CalcProfileBlend`.
    pub fn profile_blend(&self, at: &Vect3<M>) -> ProfileBlend<M> {
        let half = f(0.5);
        let mut v = *at;
        v -= self.origin;
        v.x = v.x + self.cell_size * half;
        v.z = v.z + self.cell_size * half;
        let s = F::new(self.subdiv as f32) / self.cell_size;
        let c = f(1.0) / s;
        let x = v.x - c * half;
        let z = v.z - c * half;
        let (wide, high) = (self.cols * self.subdiv, self.rows * self.subdiv);
        let col = ((x * s).0 as i32).max(0).min(wide - 1);
        let row = ((z * s).0 as i32).max(0).min(high - 1);
        let fx = clamp((x - F::new(col as f32) * c) * s, F::ZERO, f(1.0));
        let fz = clamp((z - F::new(row as f32) * c) * s, F::ZERO, f(1.0));
        let i0 = row * wide + col;
        let i1 = if col != wide - 1 { i0 + 1 } else { i0 };
        let (i2, i3) = if row != high - 1 { (i0 + wide, i1 + wide) } else { (i0, i1) };
        let at = |i: i32| self.regions.get(i as usize).copied().unwrap_or(0) as i32;
        let one = f(1.0);
        ProfileBlend {
            index: [at(i0), at(i1), at(i2), at(i3)],
            weight: [(one - fx) * (one - fz), fx * (one - fz), (one - fx) * fz, fx * fz],
        }
    }
}

fn rd_f(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
fn rd_i(d: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
fn v3<M: Model>(d: &[u8], o: usize) -> Vect3<M> {
    Vect3::of(rd_f(d, o), rd_f(d, o + 4), rd_f(d, o + 8))
}

fn read_tree<M: Model>(c: &Container, id: NodeId, leaf_words: usize) -> (Vec<Node<M>>, Vec<(i32, u32)>) {
    let d = c.body(id);
    let (nn, nl) = (rd_i(d, 0) as usize, rd_i(d, 4) as usize);
    let mut p = 8;
    let mut nodes = Vec::with_capacity(nn);
    for _ in 0..nn {
        nodes.push(Node {
            plane: Plane { n: v3(d, p), d: F::new(rd_f(d, p + 12)) },
            front: rd_i(d, p + 16) as u32,
            back: rd_i(d, p + 20) as u32,
        });
        p += 24;
    }
    let mut leaves = Vec::with_capacity(nl);
    for _ in 0..nl {
        let a = rd_i(d, p);
        let b = if leaf_words == 2 { rd_i(d, p + 4) as u32 } else { 0 };
        leaves.push((a, b));
        p += 4 * leaf_words;
    }
    (nodes, leaves)
}

const LEAF: u32 = 0x4000_0000;

impl<M: Model> World<M> {
    /// Loads the world in `lib.file(file)`: its tree and every zone's base,
    /// resources and actors (`ParseWorldObj` with i == 0).
    pub fn load(lib: &Library, file: usize) -> Option<Self> {
        let c = lib.file(file);
        let root = *c.roots.first()?;
        let base = c.child(root, 0x8200)?;
        let tree = c.child(base, 0x8230)?;
        let (nodes, leaves) = read_tree::<M>(c, tree, 1);
        let mut w = World {
            nodes,
            leaves: leaves.into_iter().map(|l| l.0).collect(),
            zones: Vec::new(),
            state: 0,
            stamp: 0,
            sprites: Vec::new(),
            sprite_of: HashMap::new(),
            sprite_refs: Vec::new(),
            buffers: Vec::new(),
            actors: Vec::new(),
            collide: Collide::default(),
            time: 0,
            hsprites: Vec::new(),
            anims: AnimScene::default(),
            anim_cache: AnimCache::new(),
            grid: Grid::read(c, base),
            zone_bounds: Self::proxies(c, base),
        };
        let zones = c.zones();
        for &z in &zones {
            w.zones.push(Self::zone_base(c, z));
        }
        for (zi, &z) in zones.iter().enumerate() {
            w.load_zone(lib, file, zi, z);
        }
        Some(w)
    }

    /// `ParseWorldZoneProxies`: 56 bytes a zone (two 8-byte offsets, a
    /// size, a centre, the bounds), plus a name in version 1.
    pub fn proxies(c: &Container, base: NodeId) -> Vec<BBox<M>> {
        let Some(p) = c.child(base, 0x8210) else { return Vec::new() };
        let d = c.body(p);
        if c.node(p).version == 1 {
            return Vec::new();
        }
        (0..rd_i(d, 0).max(0) as usize)
            .filter(|k| 4 + k * 56 + 56 <= d.len())
            .map(|k| BBox { min: v3(d, 4 + k * 56 + 32), max: v3(d, 4 + k * 56 + 44) })
            .collect()
    }

    /// The zone whose proxy bounds hold (x, z), out of the world base record
    /// (the nearest by bounds distance when none does).
    pub fn zone_at(c: &Container, x: f32, z: f32) -> Option<usize> {
        let base = c.child(*c.roots.first()?, 0x8200)?;
        let b = Self::proxies(c, base);
        let at = Vect3::<M>::of(x, 0.0, z);
        (0..b.len()).min_by(|&i, &j| {
            let flat = |k: usize| {
                let mut bb = b[k];
                bb.min.y = F::new(-1e9);
                bb.max.y = F::new(1e9);
                bb.distance_squared(&at).0
            };
            flat(i).total_cmp(&flat(j))
        })
    }

    /// `VIWorld::StreamLeaf`'s radial rule for the zones at `at`: which of
    /// those not in `loaded` to load (inside the inner radius) and which of
    /// those in it to let go (past the outer). Every zone is measured, where
    /// the game measures those its BSP walk reaches within 1,050.
    pub fn stream(&self, at: &Vect3<M>, loaded: &std::collections::BTreeSet<usize>) -> (Vec<usize>, Vec<usize>) {
        let (inner, outer) = (F::<M>::new(STREAM_INNER), F::<M>::new(STREAM_OUTER));
        let mut load = Vec::new();
        let mut release = Vec::new();
        for (z, b) in self.zone_bounds.iter().enumerate() {
            let d = b.distance_squared(at);
            if loaded.contains(&z) {
                if outer * outer < d {
                    release.push(z);
                }
            } else if d < inner * inner {
                load.push(z);
            }
        }
        // Nearest first, so a budgeted caller fills in around the player.
        load.sort_by(|&a, &b| self.zone_bounds[a].distance_squared(at).0.total_cmp(&self.zone_bounds[b].distance_squared(at).0));
        (load, release)
    }

    fn zone_base(c: &Container, z: NodeId) -> Zone<M> {
        let mut zone = Zone::default();
        let Some(base) = c.child(z, 0x3200) else { return zone };
        if let Some(t) = c.child(base, 0x3220) {
            let (n, l) = read_tree(c, t, 2);
            zone.nodes = n;
            zone.leaves = l;
        }
        if let Some(pt) = c.child(base, 0x3250) {
            let d = c.body(pt);
            zone.pretrans = (0..rd_i(d, 0).max(0) as usize).map(|i| v3(d, 4 + i * 12)).collect();
        }
        if let Some(rooms) = c.child(base, 0x3230) {
            for &r in &c.node(rooms).children {
                if c.node(r).ty == 0x3240 {
                    let d = c.body(r);
                    zone.rooms.push(Room { sprite_id: rd_i(d, 0) as u32, stamp20: -1, stamp24: -1, ..Default::default() });
                }
            }
        }
        zone
    }

    /// The zone's actors (`ParseZoneActors`), then `ReplaceProxyRooms` and
    /// `ReplaceStaticProxyActors`, which resolve the DictIDs and insert.
    fn load_zone(&mut self, lib: &Library, file: usize, zi: usize, z: NodeId) {
        let c = lib.file(file);
        if let Some(acts) = c.child(z, 0x3290) {
            for &ra in &c.node(acts).children {
                if c.node(ra).ty != 0x3270 {
                    continue;
                }
                let Some(room) = c.child(ra, 0x3280).map(|r| rd_i(c.body(r), 0) as usize) else { continue };
                let Some(list) = c.child(ra, 0x6040) else { continue };
                for &a in &c.node(list).children {
                    let d = c.body(a);
                    if c.node(a).ty != 0x6000 || d.len() < 36 {
                        continue;
                    }
                    let id = self.actors.len();
                    self.actors.push(Actor {
                        location: v3(d, 4),
                        hpr: v3(d, 16),
                        scale: F::new(rd_f(d, 28)),
                        sprite_id: rd_i(d, 0) as u32,
                        sprite: None,
                        object_to_world: Matrix44::identity(),
                        bound: Sphere::default(),
                        flags: 12,
                        stamp80: 0,
                    });
                    if let Some(r) = self.zones[zi].rooms.get_mut(room) {
                        r.placements.push(id);
                    }
                }
            }
        }
        let pretrans = self.zones[zi].pretrans.clone();
        for ri in 0..self.zones[zi].rooms.len() {
            let sid = self.zones[zi].rooms[ri].sprite_id;
            if let Some(r) = lib.lookup(sid as i32) {
                self.zones[zi].rooms[ri].sprite = self.sprite(lib, r, &pretrans);
            }
        }
        for ri in 0..self.zones[zi].rooms.len() {
            for j in 0..self.zones[zi].rooms[ri].placements.len() {
                let a = self.zones[zi].rooms[ri].placements[j];
                if self.actors[a].flags & 4 != 0 {
                    let Some(r) = lib.lookup(self.actors[a].sprite_id as i32) else { continue };
                    let Some(s) = self.sprite(lib, r, &pretrans) else { continue };
                    self.actors[a].sprite = Some(s);
                    self.update_actor(a);
                    self.actors[a].flags &= !4;
                }
            }
        }
    }

    /// The scene sprite a dictionary entry is, built once.
    fn sprite(&mut self, lib: &Library, r: Ref, pretrans: &[Vect3<M>]) -> Option<usize> {
        if let Some(&s) = self.sprite_of.get(&r) {
            return Some(s);
        }
        let c = lib.file(r.file);
        let n = c.node(r.node);
        let header_bbox = |hdr: u16, off: usize| -> BBox<M> {
            c.child(r.node, hdr).map_or(BBox::default(), |h| {
                let d = c.body(h);
                BBox { min: v3(d, off), max: v3(d, off + 12) }
            })
        };
        let (kind, bbox, sprite_type) = match n.ty {
            // SimpleSprite: the buffer only when the record's version says so.
            0x2000 => {
                let coll = if n.version != 0 { self.buffer(c, r.node, pretrans) } else { None };
                (SpriteKind::Simple(coll), header_bbox(0x2001, 4), 1)
            }
            0x2310 => (SpriteKind::Simple(self.buffer(c, r.node, pretrans)), header_bbox(0x2311, 8), 1),
            0x2C00 => {
                let mut members = Vec::new();
                if let Some(m) = c.child(r.node, 0x2C30) {
                    let d = c.body(m);
                    for k in 0..rd_i(d, 0).max(0) as usize {
                        let o = 4 + k * 32;
                        let Some(mr) = lib.lookup(rd_i(d, o)) else { continue };
                        let Some(ms) = self.sprite(lib, mr, pretrans) else { continue };
                        let t = |i: usize| rd_f(d, o + 4 + i * 4);
                        members.push((ms, [t(0), t(1), t(2)], t(3), [t(4), t(5), t(6)]));
                    }
                }
                (SpriteKind::Group(members), header_bbox(0x2C10, 4), 11)
            }
            0x2A00 => {
                let mut levels = Vec::new();
                if let Some(l) = c.child(r.node, 0x2A30) {
                    let d = c.body(l);
                    for k in 0..rd_i(d, 0).max(0) as usize {
                        let Some(lr) = lib.lookup(rd_i(d, 4 + k * 8)) else { continue };
                        if let Some(ls) = self.sprite(lib, lr, pretrans) {
                            levels.push(ls);
                        }
                    }
                }
                (SpriteKind::Lod(levels), header_bbox(0x2A10, 4), 0)
            }
            0x2700 => (SpriteKind::None, header_bbox(0x2710, 4), 5),
            // ParseSoundSpriteObj: radius and volume (1 and 1 before version
            // 1); the box and sphere are the radius about the origin
            0xB100 => {
                let (mut radius, mut volume) = (1.0, 1.0);
                if let Some(h) = c.child(r.node, 0xB110).or_else(|| c.node(r.node).children.first().copied()) {
                    let d = c.body(h);
                    if c.node(h).version != 0 && d.len() >= 12 {
                        radius = rd_f(d, 4);
                        volume = rd_f(d, 8);
                    }
                }
                let sound = c.node(r.node).children.iter().copied().find(|&k| matches!(c.node(k).ty, 0xB000 | 0xB030)).map(|node| Ref { file: r.file, node });
                let rr = F::new(radius);
                let bbox = BBox { min: Vect3::new(-rr, -rr, -rr), max: Vect3::new(rr, rr, rr) };
                (SpriteKind::Sound { sound, volume, attenuation: 0, pan: true }, bbox, 9)
            }
            // ParseStreamAudioSpriteObj: id, the box, the pathname (u16
            // length), looping, priority
            0x2E00 => {
                let d = c.body(r.node);
                if d.len() < 30 {
                    (SpriteKind::None, BBox::default(), 0)
                } else {
                    let bbox = BBox { min: v3(d, 4), max: v3(d, 16) };
                    let n = u16::from_le_bytes([d[28], d[29]]) as usize;
                    let end = (30 + n).min(d.len());
                    let path = String::from_utf8_lossy(&d[30..end].iter().take(127).copied().collect::<Vec<u8>>()).into_owned();
                    let looping = d.get(end..end + 4).is_some_and(|b| i32::from_le_bytes(b.try_into().unwrap()) != 0);
                    let priority = d.get(end + 4..end + 8).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()));
                    (SpriteKind::Stream { path, looping, priority }, bbox, 12)
                }
            }
            0x2200 => match load_hsprite(lib, r, &mut self.anims, &mut self.anim_cache) {
                Some((h, attach)) => {
                    let mut resolved = Vec::new();
                    for (sid, node) in attach {
                        if let Some(ar) = lib.lookup(sid)
                            && let Some(s) = self.sprite(lib, ar, pretrans)
                        {
                            resolved.push((s, node));
                        }
                    }
                    self.hsprites.push((h, resolved));
                    (SpriteKind::H(self.hsprites.len() - 1), header_bbox(0x2210, 4), 4)
                }
                None => (SpriteKind::None, header_bbox(0x2210, 4), 4),
            },
            _ => (SpriteKind::None, BBox::default(), 0),
        };
        let id = self.sprites.len();
        // a sound sprite's sphere is its radius, not its box's half diagonal
        let bsphere = if let SpriteKind::Sound { .. } = kind { Sphere { c: Vect3::default(), r: bbox.max.x } } else { Sphere::from_bbox(&bbox) };
        self.sprites.push(SceneSprite { kind, bbox, bsphere, sprite_type });
        self.sprite_refs.push(r);
        self.sprite_of.insert(r, id);
        Some(id)
    }

    fn buffer(&mut self, c: &Container, sprite: NodeId, pretrans: &[Vect3<M>]) -> Option<usize> {
        let b = c.child(sprite, 0x4200)?;
        let buf = CollBuffer::parse(c.body(b), c.node(b).version, pretrans)?;
        self.buffers.push(buf);
        Some(self.buffers.len() - 1)
    }

    /// `VIScene::UpdateActor`: the transform, the bound, and insertion into
    /// every room the world and zone trees put it in.
    fn update_actor(&mut self, i: usize) {
        let Some(s) = self.actors[i].sprite else { return };
        let a = &mut self.actors[i];
        a.object_to_world = Matrix44::transform(&a.hpr, a.scale, &a.location);
        let sp = &self.sprites[s];
        a.bound.c = a.object_to_world.mul_point(&sp.bsphere.c);
        a.bound.r = sp.bsphere.r * a.scale;
        let corners = sp.bbox.corners(&a.object_to_world);
        let bound = a.bound;
        let stamp = self.state;
        if self.nodes.is_empty() {
            self.insert_world(LEAF, i, &bound, &corners, stamp);
        } else {
            self.insert_world(0, i, &bound, &corners, stamp);
        }
        self.state += 1;
        self.stamp += 1;
    }

    fn insert_world(&mut self, node: u32, actor: usize, b: &Sphere<M>, corners: &[Vect3<M>; 8], stamp: i32) {
        let mut node = node;
        if node & 0xF000_0000 != 0 {
            if node & LEAF != 0 {
                let z = self.leaves[(node & 0x0FFF_FFFF) as usize] as usize;
                let root = if self.zones[z].nodes.is_empty() { LEAF } else { 0 };
                self.insert_zone(z, root, actor, b, corners, stamp);
                return;
            }
            node &= 0x0FFF_FFFF;
        }
        let n = self.nodes[node as usize].clone();
        let (front, back) = split(&n.plane, b, corners);
        if front {
            self.insert_world(n.front, actor, b, corners, stamp);
        }
        if back {
            self.insert_world(n.back, actor, b, corners, stamp);
        }
    }

    fn insert_zone(&mut self, z: usize, node: u32, actor: usize, b: &Sphere<M>, corners: &[Vect3<M>; 8], stamp: i32) {
        let mut node = node;
        if node & 0xF000_0000 != 0 {
            if node & LEAF != 0 {
                let Some(&(room, _)) = self.zones[z].leaves.get((node & 0x0FFF_FFFF) as usize) else { return };
                let Some(r) = self.zones[z].rooms.get_mut(room as usize) else { return };
                if r.stamp20 != stamp {
                    r.stamp20 = stamp;
                    r.actors.push(actor);
                }
                return;
            }
            node &= 0x0FFF_FFFF;
        }
        let n = self.zones[z].nodes[node as usize].clone();
        let (front, back) = split(&n.plane, b, corners);
        if front {
            self.insert_zone(z, n.front, actor, b, corners, stamp);
        }
        if back {
            self.insert_zone(z, n.back, actor, b, corners, stamp);
        }
    }

    /// The animated HSprite a dictionary entry is, if the world built one.
    pub fn hsprite_of(&self, r: Ref) -> Option<usize> {
        match self.sprites[*self.sprite_of.get(&r)?].kind {
            SpriteKind::H(i) => Some(i),
            _ => None,
        }
    }

    /// `VIScene::Collide(VICollSphere &, VISceneFilter &)`.
    pub fn collide_sphere(&mut self, s: &mut CollSphere<M>, f: &mut Filter) -> bool {
        let mut v = Vars { start: s.ray.start, end: s.ray.end, radius: s.radius, stamp: self.state };
        let hit = if self.nodes.is_empty() { self.world_node(LEAF, &mut v, s, f) } else { self.world_node(0, &mut v, s, f) };
        self.state += 1;
        self.stamp += 1;
        hit
    }

    fn world_node(&mut self, node: u32, v: &mut Vars<M>, s: &mut CollSphere<M>, f: &mut Filter) -> bool {
        let mut node = node;
        if node & 0xF000_0000 != 0 {
            if node & LEAF != 0 {
                let z = self.leaves[(node & 0x0FFF_FFFF) as usize] as usize;
                return self.zone_collide(z, v, s, f);
            }
            node &= 0x0FFF_FFFF;
        }
        let n = self.nodes[node as usize].clone();
        descend(&n, v, |front, v| self.world_node(if front { n.front } else { n.back }, v, s, f))
    }

    fn zone_collide(&mut self, z: usize, v: &mut Vars<M>, s: &mut CollSphere<M>, f: &mut Filter) -> bool {
        if !self.zones[z].pretrans.is_empty() {
            let pt = self.zones[z].pretrans.clone();
            self.collide.set_pretranslations(&pt);
        }
        if self.zones[z].leaves.is_empty() {
            return false;
        }
        let root = if self.zones[z].nodes.is_empty() { LEAF } else { 0 };
        self.zone_node(z, root, v, s, f)
    }

    fn zone_node(&mut self, z: usize, node: u32, v: &mut Vars<M>, s: &mut CollSphere<M>, f: &mut Filter) -> bool {
        let mut node = node;
        if node & 0xF000_0000 != 0 {
            if node & LEAF != 0 {
                return self.collide_leaf(z, (node & 0x0FFF_FFFF) as usize, v, s, f);
            }
            node &= 0x0FFF_FFFF;
        }
        let n = self.zones[z].nodes[node as usize].clone();
        descend(&n, v, |front, v| self.zone_node(z, if front { n.front } else { n.back }, v, s, f))
    }

    fn collide_leaf(&mut self, z: usize, leaf: usize, v: &mut Vars<M>, s: &mut CollSphere<M>, f: &mut Filter) -> bool {
        let room = self.zones[z].leaves[leaf].0 as usize;
        let mut hit = false;
        if self.zones[z].rooms[room].stamp20 != v.stamp {
            self.zones[z].rooms[room].stamp20 = v.stamp;
            if self.collide_room(z, room, s, f) {
                hit = true;
                v.end = s.hit_point();
            }
        }
        hit
    }

    fn collide_room(&mut self, z: usize, room: usize, s: &mut CollSphere<M>, f: &mut Filter) -> bool {
        let mut hit = false;
        if self.zones[z].rooms[room].stamp24 != self.stamp {
            self.zones[z].rooms[room].stamp24 = self.stamp;
            if let Some(sprite) = self.zones[z].rooms[room].sprite {
                self.collide.set_world(&Matrix44::identity());
                if self.collide_sprite(sprite, s) {
                    f.hit_sprite = sprite as i32;
                    f.hit_actor = -1;
                    hit = true;
                }
            }
        }
        for k in 0..self.zones[z].rooms[room].actors.len() {
            let a = self.zones[z].rooms[room].actors[k];
            if self.collide_actor(a, s, f) {
                hit = true;
            }
        }
        hit
    }

    /// `VIScene::CollideActor`.
    fn collide_actor(&mut self, i: usize, s: &mut CollSphere<M>, f: &mut Filter) -> bool {
        let mut hit = false;
        if !cull_sphere(s, &self.actors[i].bound) {
            let a = &self.actors[i];
            if a.stamp80 == self.stamp || a.flags & 0x14 != 0 {
                return false;
            }
            self.actors[i].stamp80 = self.stamp;
            let Some(sprite) = self.actors[i].sprite else { return false };
            let filtered = match f.kind {
                FilterKind::Scene => i as i32 == f.actor,
                FilterKind::WalkFloor => self.sprites[sprite].sprite_type == 5,
                FilterKind::WalkCamera => self.sprites[sprite].sprite_type == 5 || i as i32 == f.actor,
            };
            if filtered {
                return false;
            }
            let m = self.actors[i].object_to_world;
            self.collide.set_world(&m);
            hit = self.collide_sprite(sprite, s);
            if hit {
                f.hit_actor = i as i32;
                f.hit_sprite = sprite as i32;
            }
        } else {
            self.actors[i].stamp80 = self.stamp;
        }
        hit
    }

    /// The sprite's own `Collide` virtual, under the collider's current world.
    fn collide_sprite(&mut self, sprite: usize, s: &mut CollSphere<M>) -> bool {
        match self.sprites[sprite].kind.clone() {
            SpriteKind::Simple(Some(b)) => {
                let buf = std::mem::take(&mut self.buffers[b]);
                let hit = self.collide.collide_obj(s, &buf);
                self.buffers[b] = buf;
                hit
            }
            SpriteKind::Group(members) => {
                let base = self.collide.world;
                for (ms, angles, scale, t) in members {
                    let mem = Matrix44::transform(&Vect3::of(angles[0], angles[1], angles[2]), F::new(scale), &Vect3::of(t[0], t[1], t[2]));
                    let out = Matrix44::mul(&mem, &base);
                    self.collide.set_world(&out);
                    if self.collide_sprite(ms, s) {
                        return true;
                    }
                }
                false
            }
            SpriteKind::Lod(levels) => levels.first().is_some_and(|&l| self.collide_sprite(l, s)),
            // VIHSprite::Collide: the pose for now, the centre transform on the
            // collider's world, and each node attachment under its node.
            SpriteKind::H(i) => {
                self.anims.time = self.time;
                let world = self.collide.world;
                let (h, attach) = &mut self.hsprites[i];
                h.process(-1, &self.anims);
                let m = h.calc_trans_about_center(&world, &self.anims);
                let parts: Vec<(usize, Matrix44<M>)> = attach
                    .iter()
                    .filter(|&&(_, node)| node != -1)
                    .map(|&(sp, node)| (sp, Matrix44::mul(&h.nodes[node as usize].world, &m)))
                    .collect();
                let mut hit = false;
                for (sp, t) in parts {
                    self.collide.set_world(&t);
                    if self.collide_sprite(sp, s) {
                        hit = true;
                    }
                }
                hit
            }
            _ => false,
        }
    }

    /// The same answer as [`World::collide_sphere`] by brute force: every
    /// room sprite and every placed actor in `zones`, no BSP, no culling. For
    /// testing the walk: anything it fails to visit shows up as a difference.
    pub fn collide_sphere_brute(&mut self, s: &mut CollSphere<M>, zones: &[usize]) -> bool {
        let mut hit = false;
        let mut seen = std::collections::HashSet::new();
        for &z in zones {
            let pt = self.zones[z].pretrans.clone();
            for room in 0..self.zones[z].rooms.len() {
                if let Some(sprite) = self.zones[z].rooms[room].sprite {
                    if !pt.is_empty() {
                        self.collide.set_pretranslations(&pt);
                    }
                    self.collide.set_world(&Matrix44::identity());
                    hit |= self.collide_sprite(sprite, s);
                }
                for k in 0..self.zones[z].rooms[room].actors.len() {
                    let a = self.zones[z].rooms[room].actors[k];
                    if !seen.insert(a) {
                        continue;
                    }
                    let (Some(sprite), m) = (self.actors[a].sprite, self.actors[a].object_to_world) else { continue };
                    self.collide.set_world(&m);
                    hit |= self.collide_sprite(sprite, s);
                }
            }
        }
        hit
    }

    /// `VIWorld::FindLeaf`: the zone and the zone leaf a point is in.
    pub fn find_leaf(&self, at: &Vect3<M>) -> Option<(usize, usize)> {
        let mut node = 0u32;
        if !self.nodes.is_empty() {
            loop {
                let n = &self.nodes[(node & 0x0FFF_FFFF) as usize];
                node = if F::ZERO <= n.plane.distance(at) { n.front } else { n.back };
                if node & LEAF != 0 {
                    break;
                }
            }
            node &= 0x0FFF_FFFF;
        }
        let &z = self.leaves.get(node as usize)?;
        let zone = &self.zones[z as usize];
        let mut leaf = 0u32;
        if !zone.nodes.is_empty() {
            loop {
                let n = &zone.nodes[(leaf & 0x0FFF_FFFF) as usize];
                leaf = if F::ZERO <= n.plane.distance(at) { n.front } else { n.back };
                if leaf & LEAF != 0 {
                    break;
                }
            }
            leaf &= 0x0FFF_FFFF;
        }
        ((leaf as usize) < zone.leaves.len()).then_some((z as usize, leaf as usize))
    }

    /// `VIWorld::FindLeaf` then the zone leaf's flags (`GetEnvFlags`).
    pub fn env_flags(&self, at: &Vect3<M>) -> u32 {
        self.find_leaf(at).map_or(!0, |(z, leaf)| self.zones[z].leaves[leaf].1)
    }

    /// `VIScene::Collide(VIEnvRay &)` with `VITransEnvRay`'s rule: a hit is
    /// the first leaf whose liquid mask differs from the one before it.
    pub fn collide_env(&mut self, r: &mut EnvRay<M>) -> bool {
        r.first_collide = true;
        let (a, b) = (r.start, r.end);
        let hit = if self.nodes.is_empty() { self.env_world(LEAF, r, a, b) } else { self.env_world(0, r, a, b) };
        self.stamp += 1;
        hit
    }

    fn env_world(&self, node: u32, r: &mut EnvRay<M>, a: Vect3<M>, b: Vect3<M>) -> bool {
        let mut node = node;
        if node & 0xF000_0000 != 0 {
            if node & LEAF != 0 {
                let z = self.leaves[(node & 0x0FFF_FFFF) as usize] as usize;
                let zone = &self.zones[z];
                if zone.leaves.is_empty() {
                    return false;
                }
                let root = if zone.nodes.is_empty() { LEAF } else { 0 };
                return env_zone(zone, root, r, a, b);
            }
            node &= 0x0FFF_FFFF;
        }
        let n = &self.nodes[node as usize];
        env_split(&n.plane, n.front, n.back, r, a, b, &mut |child, r, a, b| self.env_world(child, r, a, b))
    }
}

fn env_zone<M: Model>(zone: &Zone<M>, node: u32, r: &mut EnvRay<M>, a: Vect3<M>, b: Vect3<M>) -> bool {
    let mut node = node;
    if node & 0xF000_0000 != 0 {
        if node & LEAF != 0 {
            // CollideLeaf: report the mask; the ray's Collide decides.
            let mask = zone.leaves[(node & 0x0FFF_FFFF) as usize].1 & r.mask;
            let mut hit = false;
            if r.first_collide || r.hits != mask {
                let d = r.delta;
                let mut t = (a.x * d.x + a.y * d.y + a.z * d.z) - (r.start.x * d.x + r.start.y * d.y + r.start.z * d.z);
                t *= r.inv_length;
                hit = !r.first_collide;
                if hit {
                    r.hit_time = t;
                }
                r.hits = mask;
                r.first_collide = false;
            }
            return hit;
        }
        node &= 0x0FFF_FFFF;
    }
    let n = &zone.nodes[node as usize];
    env_split(&n.plane, n.front, n.back, r, a, b, &mut |child, r, a, b| env_zone(zone, child, r, a, b))
}

/// Visits one child of an env-ray split: (node, ray, from, to) -> hit.
type EnvVisit<'a, M> = dyn FnMut(u32, &mut EnvRay<M>, Vect3<M>, Vect3<M>) -> bool + 'a;

/// The env-ray descent shared by both trees: split the segment at the plane
/// and visit the near side first, stopping at its first hit.
fn env_split<M: Model>(
    pl: &Plane<M>,
    front: u32,
    back: u32,
    r: &mut EnvRay<M>,
    a: Vect3<M>,
    b: Vect3<M>,
    go: &mut EnvVisit<'_, M>,
) -> bool {
    let da = pl.distance(&a);
    let db = pl.distance(&b);
    if F::ZERO <= da && F::ZERO <= db {
        return go(front, r, a, b);
    }
    if da <= F::ZERO && db <= F::ZERO {
        return go(back, r, a, b);
    }
    let t = da / (da - db);
    let mut mid = b;
    mid -= a;
    mid *= t;
    mid += a;
    let (first, second) = if F::ZERO < da { (front, back) } else { (back, front) };
    if go(first, r, a, mid) {
        return true;
    }
    go(second, r, mid, b)
}

/// What one world walk carries (`VISceneCollVars`): the sweep's ends (the
/// end shrinks to each hit), its radius and the walk's stamp.
struct Vars<M: Model> {
    start: Vect3<M>,
    end: Vect3<M>,
    radius: F<M>,
    stamp: i32,
}

const EPS: f32 = 0.03125;

/// The BSP descent both `Collide(int node, ...)` bodies share: a side that
/// both ends clear by the radius plus 1/32 is the only one visited; otherwise
/// the near side first, then the far one if the (possibly shortened) sweep
/// still reaches it.
fn descend<M: Model>(n: &Node<M>, v: &mut Vars<M>, mut go: impl FnMut(bool, &mut Vars<M>) -> bool) -> bool {
    let e = F::new(EPS);
    let ds = n.plane.distance(&v.start);
    let de = n.plane.distance(&v.end);
    if v.radius + e < ds && v.radius + e < de {
        go(true, v)
    } else if ds < -v.radius - e && de < -v.radius - e {
        go(false, v)
    } else if F::ZERO < ds {
        let mut hit = go(true, v);
        let ds = n.plane.distance(&v.start);
        let de = n.plane.distance(&v.end);
        if (ds < v.radius + e || de < v.radius + e) && go(false, v) {
            hit = true;
        }
        hit
    } else {
        let mut hit = go(false, v);
        let ds = n.plane.distance(&v.start);
        let de = n.plane.distance(&v.end);
        if (-v.radius - e < ds || -v.radius - e < de) && go(true, v) {
            hit = true;
        }
        hit
    }
}

/// `InsertActor`'s choice at a node: the bound sphere alone when it is clear
/// of the plane, else the eight corners.
fn split<M: Model>(pl: &Plane<M>, b: &Sphere<M>, corners: &[Vect3<M>; 8]) -> (bool, bool) {
    let d = pl.distance(&b.c);
    if b.r < d {
        (true, false)
    } else if d < -b.r {
        (false, true)
    } else {
        let (mut front, mut back) = (false, false);
        for c in corners {
            if F::ZERO < pl.distance(c) {
                front = true;
            } else {
                back = true;
            }
        }
        (front, back)
    }
}

impl<M: Model> Scene<M> for World<M> {
    fn time(&self) -> i64 {
        self.time
    }
    fn collide_sphere(&mut self, s: &mut CollSphere<M>, filter: &mut Filter) -> bool {
        World::collide_sphere(self, s, filter)
    }
    fn collide_env(&mut self, ray: &mut EnvRay<M>) -> bool {
        World::collide_env(self, ray)
    }
    fn env_flags(&self, at: &Vect3<M>) -> u32 {
        World::env_flags(self, at)
    }
}
