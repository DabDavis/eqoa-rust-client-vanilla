//! `VIScene`'s collision world: `VIWorld` and its zones (`viworld.cpp`,
//! `vizone.cpp`), the rooms, sprites and actors in them (`viscene.cpp`), loaded
//! from the world file the way `VIESFParse` loads it (`ParseWorldBase`,
//! `ParseZoneBaseObj`, `ParseZoneRoom`, `ParseZoneRoomActorsObj`, the
//! `Replace*Proxy*` pair and `UpdateActor`), and the three walks over them:
//! the swept-sphere collide, the environment ray, and actor insertion.
//!
//! The actors made in play (`CreateActor`, `SetActorTransform`,
//! `ReleaseActor`: every entity's, the player's own among them) go into the
//! rooms they reach and out again as they move, and a `VICSprite` collides as
//! its capsule, so a walk stops at a creature as it does at a wall.

use crate::character::{AnimCache, load_hsprite};
use crate::clipstack::{Camera, ClipStack};
use crate::coll::{CollRay, CollSphere, EnvRay};
use crate::hsprite::{AnimScene, HSprite};
use crate::collide::{Capsule, Collide, CollBuffer, cull_ray_sphere, cull_sphere};
use crate::flora::{Dist, FloraModel, FloraScene, QueryTriList, Set, ZoneFlora};
use crate::fpu::{F, Model, clamp, f};
use crate::geom::{BBox, Matrix44, Plane, Sphere};
use crate::pick::{self, Color32F, PickBuffer, PickSegment};
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

/// `VIZonePortal`: a polygon into another room of the zone (`m_destRoom`), its
/// plane (`CalcPortalPlane`, facing the side a camera must be on to see
/// through it), and `m_p1C`'s bit 0, set while the walk draws the room behind
/// it so a cycle of rooms cannot recurse.
#[derive(Clone, Debug, Default)]
pub struct Portal<M: Model> {
    pub dest: i32,
    pub verts: Vec<Vect3<M>>,
    pub plane: Plane<M>,
    pub open: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Room<M: Model> {
    /// `m_flags` as the record gives it: bit 0 a room open to the sky, which
    /// `VIWorld::RenderByPortals` draws by the frustum (the whole world), where
    /// it draws a closed room alone with what its portals show.
    pub flags: u32,
    /// How many portals lead out of it.
    pub portals: i32,
    /// `m_bbox`, the record's from version 2 (zero before).
    pub bbox: BBox<M>,
    /// `m_portals`, as `ParseZoneRoom` builds them (`DestRoom`, `Begin`,
    /// `Vertex`, `End`).
    pub portal_list: Vec<Portal<M>>,
    /// The DictID of the room's own sprite (`m_p28`), and what it resolved to.
    pub sprite_id: u32,
    pub sprite: Option<usize>,
    /// Pass stamps: `m_p20` (per world walk), `m_p24` (per scene pass).
    pub stamp20: i32,
    pub stamp24: i32,
    /// The actors that occupy it, in insertion order (`m_p68` list).
    pub actors: Vec<usize>,
    /// The actors made in play that occupy it (`World::create_actor`). The
    /// game keeps them on the same list, but `UpdateActor` takes an actor out
    /// of every room and adds it at the end of each room it reaches, and they
    /// are updated every frame, so they always come after the zone's own, as
    /// they are walked here.
    pub moving: Vec<usize>,
    /// The zone's own actor placements, as parsed (`m_a3C`).
    pub placements: Vec<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Zone<M: Model> {
    pub nodes: Vec<Node<M>>,
    /// (room, environment flags) per leaf.
    pub leaves: Vec<(i32, u32)>,
    pub pretrans: Vec<Vect3<M>>,
    pub rooms: Vec<Room<M>>,
    /// The radial flora's tables (`VIZoneRadialFlora`, `ParseZoneFlora`).
    pub flora: ZoneFlora<M>,
}

#[derive(Clone, Debug)]
pub enum SpriteKind {
    /// SimpleSprite / SubSprite: a collision buffer, if it has one.
    Simple(Option<usize>),
    /// (member sprite, angles, scale, translation), as `VIGroupMember` holds
    /// them: `m_loc` is handed to `Transform` as the ANGLES and `m_hpr` as the
    /// translation, and the data agrees (every member's first triple is 0).
    Group(Vec<(usize, [f32; 3], f32, [f32; 3])>),
    /// The levels: (sprite, switch distance).
    Lod(Vec<(usize, f32)>),
    /// An animated HSprite: index into `World::hsprites`.
    H(usize),
    /// `VISoundSprite` (0xB100): a positional sound, its wave or XM record,
    /// the record's volume, and its attenuation and pan settings (the parser
    /// leaves them at the constructor's 0 and true).
    Sound { sound: Option<Ref>, volume: f32, attenuation: i32, pan: bool },
    /// `VIStreamAudioSprite` (0x2E00): a box the listener stands in, the BGM
    /// it names (relative to `Music\`), whether it loops and its priority.
    Stream { path: String, looping: bool, priority: i32 },
    /// `VIParticleSprite` (0xC100): its definition record (0xC000) and the
    /// header's flags (`m_unk430`: 0 the emitter is placed and aimed in the
    /// world, else it sits at the origin of the sprite's own space).
    Particle { def: Option<Ref>, flags: i32 },
    /// `VICSprite` (0x2700), a character, a player's or an NPC's (a signpost
    /// is one): collides as its capsule ([`Capsule::csprite`]) in its actor's
    /// space, as nothing while its actor's copy is dead, never with a ray.
    C,
    /// `VIFloraSprite` (0x2F00): a plant the radial flora draws
    /// (`vi::flora`); collides as nothing.
    Flora,
    /// Collides as nothing (sounds, lights, emitters).
    None,
}

#[derive(Clone, Debug)]
pub struct SceneSprite<M: Model> {
    pub kind: SpriteKind,
    pub bbox: BBox<M>,
    pub bsphere: Sphere<M>,
    /// `VISpriteType` 5 is VICSprite, which the walk filters skip.
    pub sprite_type: i32,
    /// A simple sprite's prim buffer as the pick reads it (`World::picks`).
    pub pick: Option<usize>,
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
    /// `m_p70`: the static light colour (`CreateStaticProxyActor`'s, the
    /// zone actor record's bytes / 255).
    pub color: Color32F<M>,
    /// `m_p6C`: its static lighting (`World::lightings`), if any.
    pub lighting: Option<usize>,
    /// `m_dead` of its own copy of a CSprite (`CreateActor` copies a sprite
    /// whose `GetInstType` is 1, so each creature dies alone), with which
    /// `VICSprite::Collide` answers nothing.
    pub dead: bool,
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
    /// The simple sprites' prim buffers as the pick reads them.
    pub picks: Vec<PickBuffer<M>>,
    /// The static lightings: each a list of colour buffers (0x1220), the
    /// colours as the pick reads them, flat in vertex order.
    pub lightings: Vec<Vec<Vec<[u8; 4]>>>,
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
    /// Which zones are in (`World::add_zone`); an unloaded zone is a proxy
    /// with no tree, rooms or actors, which every walk passes over.
    pub loaded: Vec<bool>,
    /// Each actor's place in the whole-file load's insertion order (zone,
    /// room, placement), which every room's actor list is kept sorted by.
    actor_order: Vec<u64>,
    /// The rooms an insertion added to, while a zone loads: (zone, room).
    touched: Vec<(usize, usize)>,
    /// The actors made in play (`create_actor`) and the rooms each is in now,
    /// as (zone, room) (`m_p98`'s occupancy list), which `RemoveActor` walks;
    /// a zone's own actors are placed once and never taken out.
    moving: HashMap<usize, Vec<(usize, usize)>>,
    /// Released actors' places (`VIPool::Erase`), the next `create_actor`'s.
    free: Vec<usize>,
    /// Which of the scenes made (`World::load`) this is: an actor made in one
    /// is nothing in the next (a world change, F9's art).
    pub serial: u64,
}

/// The scenes made so far, for [`World::serial`].
static SCENES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// `VIScene::Init`'s streaming radii (`m_p2A48[1]`, `[0]`): a zone loads
/// when its bounds come within the inner one and is let go past the outer.
pub const STREAM_INNER: f32 = 900.0;
pub const STREAM_OUTER: f32 = 950.0;

/// The world header's grid (record 0x8220) and the region bytes over it
/// (0x8240, version 2 up): one terrain-profile index per sub-cell.
#[derive(Clone, Debug)]
pub struct Grid<M: Model> {
    /// The world's box (`VIWorld::m_bbox`): what a flora swatch spans in y.
    pub bbox: BBox<M>,
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
        let bbox = BBox { min: v3(d, p), max: v3(d, p + 12) };
        p += 24;
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
        Some(Grid { bbox, rows, cols, subdiv, cell_size, origin, regions })
    }

    /// `VIScene::GetWorldGridCenter(a, b)`: column a, row b; the origin's
    /// zero in a world of one cell.
    pub fn center(&self, a: i32, b: i32) -> Vect3<M> {
        if self.rows == 1 && self.cols == 1 {
            return Vect3::default();
        }
        let mut v = Vect3::new(F::from_int(a) * self.cell_size, F::ZERO, F::from_int(b) * self.cell_size);
        v += self.origin;
        v
    }

    /// `VIScene::CalcWorldGridOffset(v)`: the column and row `v` is in (its
    /// x and z over the cell size, truncated) and `v` less that cell's corner
    /// and the origin; (0, 0, v) in a world of one cell.
    pub fn offset(&self, v: &Vect3<M>) -> (i32, i32, Vect3<M>) {
        if self.rows == 1 && self.cols == 1 {
            return (0, 0, *v);
        }
        let a = (v.x / self.cell_size).to_int();
        let b = (v.z / self.cell_size).to_int();
        let mut out = *v;
        out.x -= F::from_int(a) * self.cell_size;
        out.z -= F::from_int(b) * self.cell_size;
        out -= self.origin;
        (a, b, out)
    }

    /// `VIScene::CalcFarPlane`: the camera's far plane at `at`, the terrain
    /// profiles' far there (`tod::clip_planes`), uncapped.
    pub fn far_plane(&self, at: &Vect3<M>) -> F<M> {
        crate::tod::clip_planes(&self.profile_blend(at)).1
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

/// `ParseZoneRoom`'s portals, after the room's portal count at `at` and its
/// vertex count: per portal its vertex count, the room it leads into (a word
/// more before version 2), then its vertices; each one's plane made as `End`
/// makes it. A record cut short keeps the portals read whole.
fn read_portals<M: Model>(d: &[u8], at: usize, version: i16) -> Vec<Portal<M>> {
    let mut out = Vec::new();
    if d.len() < at + 8 {
        return out;
    }
    let n = rd_i(d, at).max(0) as usize;
    let mut o = at + 8;
    for _ in 0..n {
        if d.len() < o + 8 {
            break;
        }
        let (k, dest) = (rd_i(d, o).max(0) as usize, rd_i(d, o + 4));
        o += if version < 2 { 12 } else { 8 };
        if d.len() < o + k * 12 {
            break;
        }
        let verts: Vec<Vect3<M>> = (0..k).map(|j| v3(d, o + j * 12)).collect();
        o += k * 12;
        let plane = portal_plane(&verts);
        out.push(Portal { dest, verts, plane, open: false });
    }
    out
}

/// `VIZoneRoom::CalcPortalPlane`: Newell's method over the ring, the
/// centroid summed in the same loop; the distance is the normal dotted into
/// the average vertex, negated. A degenerate ring (`Normalize` 0) gets d 0
/// and the normal `Normalize` left.
fn portal_plane<M: Model>(v: &[Vect3<M>]) -> Plane<M> {
    let mut p = Plane::<M>::default();
    let Some(&last_v) = v.last() else { return p };
    let mut sum = Vect3::<M>::default();
    let mut prev = last_v;
    for &c in v {
        let last = prev;
        prev = c;
        p.n.x = p.n.x + (prev.y - last.y) * (prev.z + last.z);
        p.n.y = p.n.y + (prev.z - last.z) * (prev.x + last.x);
        p.n.z = p.n.z + (prev.x - last.x) * (prev.y + last.y);
        sum.x = sum.x + prev.x;
        sum.y = sum.y + prev.y;
        sum.z = sum.z + prev.z;
    }
    if p.n.normalize() == 0 {
        p.d = F::ZERO;
    } else {
        let k = F::ONE / F::new(v.len() as f32);
        sum.x = sum.x * k;
        sum.y = sum.y * k;
        sum.z = sum.z * k;
        p.d = -(p.n.x * sum.x + p.n.y * sum.y + p.n.z * sum.z);
    }
    p
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
    /// Loads the world in `lib.file(file)`: its tree and every loaded zone's
    /// base, resources and actors (`ParseWorldObj` with i == 0 when the whole
    /// file is loaded). A world opened with its zones on the disc
    /// (`esf::Container::open_deferred`) has only the zones its library has
    /// loaded; the rest come later through [`World::add_zone`].
    pub fn load(lib: &Library, file: usize) -> Option<Self> {
        let c = lib.file(file);
        let root = *c.roots.first()?;
        let base = c.child(root, 0x8200)?;
        let tree = c.child(base, 0x8230)?;
        let (nodes, leaves) = read_tree::<M>(c, tree, 1);
        let n = lib.zone_count(file);
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
            picks: Vec::new(),
            lightings: Vec::new(),
            actors: Vec::new(),
            collide: Collide::default(),
            time: 0,
            hsprites: Vec::new(),
            anims: AnimScene::default(),
            anim_cache: AnimCache::new(),
            grid: Grid::read(c, base),
            zone_bounds: Self::proxies(c, base),
            loaded: vec![false; n],
            actor_order: Vec::new(),
            touched: Vec::new(),
            moving: HashMap::new(),
            free: Vec::new(),
            serial: SCENES.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        };
        for zi in 0..n {
            w.zones.push(lib.zone(file, zi).map_or_else(Zone::default, |(zf, z)| Self::zone_base(lib.file(zf), z)));
        }
        for zi in 0..n {
            if let Some((zf, z)) = lib.zone(file, zi) {
                w.load_zone(lib, file, zf, zi, z);
                w.loaded[zi] = true;
            }
        }
        w.touched.clear();
        Some(w)
    }

    /// Loads zone `zi` of the world in `lib.file(file)` once its library has
    /// it (`esf::Library::add_zone`): its base, actors and lightings as
    /// `load` does, then the actors already in that reach into it, inserted
    /// into its rooms only. Every room it changed is put back in the order
    /// the whole-file load inserts in (zone, room, placement), so a walk
    /// meets actors in the same order however the zones came. Zones are only
    /// added, never let go (no `ResetToProxy` yet). Every actor names a sprite
    /// its own zone holds (measured over all six worlds), so no proxy waits
    /// on another zone. Returns whether the zone was added.
    pub fn add_zone(&mut self, lib: &Library, file: usize, zi: usize) -> bool {
        if self.loaded.get(zi) != Some(&false) {
            return false;
        }
        let Some((zf, z)) = lib.zone(file, zi) else { return false };
        self.zones[zi] = Self::zone_base(lib.file(zf), z);
        self.touched.clear();
        let before = self.actors.len();
        self.load_zone(lib, file, zf, zi, z);
        self.loaded[zi] = true;
        // the actors loaded before it, into its rooms (`UpdateActor` walks
        // every zone; here only this one's leaves can be new to them)
        for a in 0..before {
            if self.actors[a].sprite.is_none() || self.actors[a].flags & 4 != 0 {
                continue;
            }
            let a_ = &self.actors[a];
            let bound = a_.bound;
            let corners = self.sprites[a_.sprite.unwrap()].bbox.corners(&a_.object_to_world);
            let stamp = self.state;
            let root = if self.nodes.is_empty() { LEAF } else { 0 };
            self.insert_world_zone(root, zi, a, &bound, &corners, stamp);
            self.state += 1;
            self.stamp += 1;
        }
        let mut touched = std::mem::take(&mut self.touched);
        touched.sort_unstable();
        touched.dedup();
        for &(tz, room) in &touched {
            let order = &self.actor_order;
            self.zones[tz].rooms[room].actors.sort_by_key(|&a| order[a]);
        }
        true
    }

    /// The zone actor `a` was placed by, once its zone has placed it.
    pub fn actor_zone(&self, a: usize) -> Option<usize> {
        self.actor_order.get(a).filter(|&&k| k != u64::MAX).map(|&k| (k >> 42) as usize)
    }

    /// `insert_world`, descending only into the leaves of zone `only`.
    fn insert_world_zone(&mut self, node: u32, only: usize, actor: usize, b: &Sphere<M>, corners: &[Vect3<M>; 8], stamp: i32) {
        let mut node = node;
        if node & 0xF000_0000 != 0 {
            if node & LEAF != 0 {
                let z = self.leaves[(node & 0x0FFF_FFFF) as usize] as usize;
                if z == only {
                    let root = if self.zones[z].nodes.is_empty() { LEAF } else { 0 };
                    self.insert_zone(z, root, actor, b, corners, stamp);
                }
                return;
            }
            node &= 0x0FFF_FFFF;
        }
        let n = self.nodes[node as usize].clone();
        let (front, back) = split(&n.plane, b, corners);
        if front {
            self.insert_world_zone(n.front, only, actor, b, corners, stamp);
        }
        if back {
            self.insert_world_zone(n.back, only, actor, b, corners, stamp);
        }
    }

    /// `ParseWorldZoneProxies`: 56 bytes a zone (two 8-byte offsets, a
    /// size, a centre, the bounds); a version 1 table (the 2002 disc's
    /// Tunaria) follows each with the zone's name, which the game reads into
    /// a stack buffer and throws away (`ReadString`: a 16-bit length, then
    /// the bytes).
    pub fn proxies(c: &Container, base: NodeId) -> Vec<BBox<M>> {
        let Some(p) = c.child(base, 0x8210) else { return Vec::new() };
        let d = c.body(p);
        let named = c.node(p).version == 1;
        let mut out = Vec::new();
        let mut at = 4;
        for _ in 0..rd_i(d, 0).max(0) {
            if at + 56 > d.len() {
                break;
            }
            out.push(BBox { min: v3(d, at + 32), max: v3(d, at + 44) });
            at += 56;
            if named {
                let Some(len) = d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize) else { break };
                at += 2 + len;
            }
        }
        out
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
                    // `ParseZoneRoom`: the sprite, a word, the flags, the box
                    // from version 2, then the portals' count
                    let d = c.body(r);
                    let version = c.node(r).version;
                    let at = if version >= 2 { 36 } else { 12 };
                    let portals = if d.len() >= at + 4 { rd_i(d, at) } else { 0 };
                    let flags = if d.len() >= 12 { rd_i(d, 8) as u32 } else { 0 };
                    let bbox = if version >= 2 && d.len() >= 36 { BBox { min: v3(d, 12), max: v3(d, 24) } } else { BBox::default() };
                    let portal_list = read_portals(d, at, version);
                    zone.rooms.push(Room { flags, portals, bbox, portal_list, sprite_id: rd_i(d, 0) as u32, stamp20: -1, stamp24: -1, ..Default::default() });
                }
            }
        }
        zone
    }

    /// The zone's actors (`ParseZoneActors`), then `ReplaceProxyRooms` and
    /// `ReplaceStaticProxyActors`, which resolve the DictIDs and insert.
    /// `file` holds the zone's records; `world` is its world's file, whose
    /// zone may have been given more by its twin (`Library::zone_extras`: the
    /// 2002 art's Tunaria keeps Frontiers' effect volumes and streamed
    /// audio), placed after the zone's own in each room.
    fn load_zone(&mut self, lib: &Library, world: usize, file: usize, zi: usize, z: NodeId) {
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
                    self.actor_order.push(u64::MAX);
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
                        color: {
                            let k = F::new(1.0 / 255.0);
                            let b = |i: usize| F::from_int(d[32 + i] as i32) * k;
                            Color32F { r: b(0), g: b(1), b: b(2), a: b(3) }
                        },
                        lighting: None,
                        dead: false,
                    });
                    if let Some(r) = self.zones[zi].rooms.get_mut(room) {
                        r.placements.push(id);
                    }
                }
            }
        }
        if let Some((_, extra)) = lib.zone_extras(world, zi) {
            for a in extra {
                let Some(room) = usize::try_from(a.room).ok().filter(|&r| r < self.zones[zi].rooms.len()) else { continue };
                let id = self.actors.len();
                self.actor_order.push(u64::MAX);
                let k = F::new(1.0 / 255.0);
                let b = |i: usize| F::from_int(a.color[i] as i32) * k;
                let v = |p: [f32; 3]| Vect3::new(F::new(p[0]), F::new(p[1]), F::new(p[2]));
                self.actors.push(Actor {
                    location: v(a.pos),
                    hpr: v(a.rot),
                    scale: F::new(a.scale),
                    sprite_id: a.sprite as u32,
                    sprite: None,
                    object_to_world: Matrix44::identity(),
                    bound: Sphere::default(),
                    flags: 12,
                    stamp80: 0,
                    color: Color32F { r: b(0), g: b(1), b: b(2), a: b(3) },
                    lighting: None,
                    dead: false,
                });
                self.zones[zi].rooms[room].placements.push(id);
            }
        }
        // ParseZoneStaticLightings: per room (0x3290: 0x32A0 the room, 0x6030
        // the list), each 0x6010 an actor index in the room (0x6020) and its
        // colour buffers (0x1220).
        if let Some(sl) = c.child(z, 0x32B0) {
            for &rl in &c.node(sl).children {
                let Some(room) = c.child(rl, 0x32A0).map(|r| rd_i(c.body(r), 0) as usize) else { continue };
                let Some(list) = c.child(rl, 0x6030) else { continue };
                for &l in &c.node(list).children {
                    let Some(obj) = c.child(l, 0x6020) else { continue };
                    let ai = rd_i(c.body(obj), 0);
                    let bufs: Vec<Vec<[u8; 4]>> = c
                        .node(l)
                        .children
                        .iter()
                        .filter(|&&b| c.node(b).ty == 0x1220)
                        .map(|&b| {
                            let d = c.body(b);
                            let n = rd_i(d, 4).max(0) as usize;
                            (0..n).filter_map(|k| d.get(8 + 4 * k..12 + 4 * k)).map(|q| pick::pack_color([q[0] >> 1, q[1] >> 1, q[2] >> 1, q[3] >> 1])).collect()
                        })
                        .collect();
                    let Some(&a) = usize::try_from(ai).ok().and_then(|ai| self.zones[zi].rooms.get(room)?.placements.get(ai)) else { continue };
                    self.lightings.push(bufs);
                    self.actors[a].lighting = Some(self.lightings.len() - 1);
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
                self.actor_order[a] = (zi as u64) << 42 | (ri as u64) << 21 | j as u64;
                if self.actors[a].flags & 4 != 0 {
                    let Some(r) = lib.lookup(self.actors[a].sprite_id as i32) else { continue };
                    let Some(s) = self.sprite(lib, r, &pretrans) else { continue };
                    self.actors[a].sprite = Some(s);
                    self.update_actor(a);
                    self.actors[a].flags &= !4;
                }
            }
        }
        // ParseZoneFlora (in the zone base 0x3200, of version 1 up), then
        // Precalc: the flora sprites (0x32E0, each a sprite record, the
        // models), the distributions (0x32F4: id, model, weight, scale,
        // jitter) and the sets (0x32F8: first distribution, count) under 0x32F0.
        if let Some(base) = c.child(z, 0x3200)
            && c.node(base).version != 0
            && let Some(fl) = c.child(base, 0x32D0)
        {
            let mut flora = ZoneFlora::default();
            if let Some(arr) = c.child(fl, 0x32E0) {
                for k in 0..c.node(arr).children.len() {
                    let node = c.node(arr).children[k];
                    let sprite = self.sprite(lib, Ref { file, node }, &pretrans).map_or(-1, |s| s as i32);
                    flora.models.push(FloraModel { sprite, ..Default::default() });
                }
            }
            if let Some(sets) = c.child(fl, 0x32F0) {
                if let Some(t) = c.child(sets, 0x32F4) {
                    let d = c.body(t);
                    for k in 0..rd_i(d, 0).max(0) as usize {
                        let o = 4 + k * 20;
                        if d.len() < o + 20 {
                            break;
                        }
                        let fl = |i: usize| F::new(rd_f(d, o + i));
                        flora.dists.push(Dist { id: rd_i(d, o) as u32, model: rd_i(d, o + 4), weight: fl(8), scale: fl(12), jitter: fl(16), ..Default::default() });
                    }
                }
                if let Some(t) = c.child(sets, 0x32F8) {
                    let d = c.body(t);
                    for k in 0..rd_i(d, 0).max(0) as usize {
                        let o = 4 + k * 8;
                        if d.len() < o + 8 {
                            break;
                        }
                        flora.sets.push(Set { first: rd_i(d, o), count: rd_i(d, o + 4), ..Default::default() });
                    }
                }
            }
            // a model's prim buffer and palette are its scene sprite here
            // (the renderer's model for it), its sphere the sprite's
            let sprites = &self.sprites;
            flora.precalc(|s| usize::try_from(s).ok().and_then(|i| sprites.get(i)).map(|sp| (s, s, sp.bsphere)));
            self.zones[zi].flora = flora;
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
                            levels.push((ls, rd_f(d, 8 + k * 8)));
                        }
                    }
                }
                (SpriteKind::Lod(levels), header_bbox(0x2A10, 4), 0)
            }
            // ParseCSpriteObj: the header's box (its sphere the box's)
            0x2700 => (SpriteKind::C, header_bbox(0x2710, 4), 5),
            // ParseFloraSpriteObj: the header's id and box (its sphere the box's)
            0x2F00 => (SpriteKind::Flora, header_bbox(0x2F01, 4), 13),
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
            // ParseParticleSpriteObj: the id and (from version 1) the flags;
            // the box and sphere are a unit radius, the emitter's own bounds
            // coming at its first update
            0xC100 => {
                let flags = c.child(r.node, 0xC101).filter(|&h| c.node(h).version != 0).map_or(0, |h| rd_i(c.body(h), 4));
                let def = c.child(r.node, 0xC000).map(|node| Ref { file: r.file, node });
                let o = F::ONE;
                (SpriteKind::Particle { def, flags }, BBox { min: Vect3::new(-o, -o, -o), max: Vect3::new(o, o, o) }, 10)
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
        let bsphere = if let SpriteKind::Sound { .. } | SpriteKind::Particle { .. } = kind { Sphere { c: Vect3::default(), r: bbox.max.x } } else { Sphere::from_bbox(&bbox) };
        // the prim buffer a simple sprite picks with
        let pick = match n.ty {
            0x2000 | 0x2310 => c.child(r.node, 0x1200).and_then(|b| pick::read_pick_buffer(c.body(b), c.node(b).version as u16, pretrans)).map(|pb| {
                self.picks.push(pb);
                self.picks.len() - 1
            }),
            _ => None,
        };
        self.sprites.push(SceneSprite { kind, bbox, bsphere, sprite_type, pick });
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

    /// `VIScene::UpdateActor`: the transform, the bound, out of the rooms it
    /// was in (`RemoveActor`), and into every room the world and zone trees
    /// put it in.
    fn update_actor(&mut self, i: usize) {
        let Some(s) = self.actors[i].sprite else { return };
        let a = &mut self.actors[i];
        a.object_to_world = Matrix44::transform(&a.hpr, a.scale, &a.location);
        let sp = &self.sprites[s];
        a.bound.c = a.object_to_world.mul_point(&sp.bsphere.c);
        a.bound.r = sp.bsphere.r * a.scale;
        let corners = sp.bbox.corners(&a.object_to_world);
        let bound = a.bound;
        self.remove_actor(i);
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
                    match self.moving.get_mut(&actor) {
                        Some(rooms) => {
                            r.moving.push(actor);
                            rooms.push((z, room as usize));
                        }
                        None => {
                            r.actors.push(actor);
                            self.touched.push((z, room as usize));
                        }
                    }
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

    /// `VIDictionary::Find` for an entity's model: the scene sprite record `r`
    /// is (a CSprite, for every creature on the disc), made once.
    pub fn find_sprite(&mut self, lib: &Library, r: Ref) -> Option<usize> {
        self.sprite(lib, r, &[])
    }

    /// `VIScene::CreateActor`: an actor of `sprite` at `at`, turned by `hpr`
    /// and scaled by `scale`, in every room it reaches (`UpdateActor`), its
    /// copy of a CSprite alive; its index, until `release_actor`.
    pub fn create_actor(&mut self, at: Vect3<M>, hpr: Vect3<M>, scale: F<M>, sprite: usize) -> usize {
        let actor = Actor {
            location: at,
            hpr,
            scale,
            sprite_id: 0,
            sprite: Some(sprite),
            object_to_world: Matrix44::identity(),
            bound: Sphere::default(),
            flags: 0,
            stamp80: 0,
            color: Color32F { a: F::ONE, ..Default::default() },
            lighting: None,
            dead: false,
        };
        let i = match self.free.pop() {
            Some(i) => {
                self.actors[i] = actor;
                i
            }
            None => {
                self.actors.push(actor);
                self.actor_order.push(u64::MAX);
                self.actors.len() - 1
            }
        };
        self.moving.insert(i, Vec::new());
        self.update_actor(i);
        i
    }

    /// `VIScene::SetActorTransform` on an actor made in play: `hpr`, `scale`
    /// and `at`, then `UpdateActor`.
    pub fn set_actor_transform(&mut self, i: usize, hpr: Vect3<M>, scale: F<M>, at: Vect3<M>) {
        let a = &mut self.actors[i];
        a.location = at;
        a.hpr = hpr;
        a.scale = scale;
        self.update_actor(i);
    }

    /// `VIScene::SetActorSprite` on an actor made in play: another sprite (of
    /// a CSprite a new copy, alive), then `UpdateActor`.
    pub fn set_actor_sprite(&mut self, i: usize, sprite: usize) {
        self.actors[i].sprite = Some(sprite);
        self.actors[i].dead = false;
        self.update_actor(i);
    }

    /// `VIScene::SetActorCollideable`: `m_p40`'s 0x10, which every collide
    /// passes over.
    pub fn set_actor_collideable(&mut self, i: usize, on: bool) {
        if on {
            self.actors[i].flags &= !0x10;
        } else {
            self.actors[i].flags |= 0x10;
        }
    }

    /// `VICSprite::SetDeath` on actor `i`'s own copy, as its collision sees
    /// it (`m_dead`).
    pub fn set_actor_dead(&mut self, i: usize, dead: bool) {
        self.actors[i].dead = dead;
    }

    /// `VIScene::ReleaseActor` on an actor made in play: out of every room,
    /// and its place the next `create_actor`'s.
    pub fn release_actor(&mut self, i: usize) {
        if self.moving.contains_key(&i) {
            self.remove_actor(i);
            self.moving.remove(&i);
            self.actors[i].sprite = None;
            self.free.push(i);
        }
    }

    /// `VIScene::RemoveActor`: an actor made in play out of every room it is
    /// in (the zones' own are never taken out).
    fn remove_actor(&mut self, i: usize) {
        let Some(rooms) = self.moving.get_mut(&i) else { return };
        for (z, r) in std::mem::take(rooms) {
            self.zones[z].rooms[r].moving.retain(|&a| a != i);
        }
    }

    /// The animated HSprite a dictionary entry is, if the world built one.
    pub fn hsprite_of(&self, r: Ref) -> Option<usize> {
        match self.sprites[*self.sprite_of.get(&r)?].kind {
            SpriteKind::H(i) => Some(i),
            _ => None,
        }
    }

    /// `VIScene::Pick(seg, VISceneOutdoorsFilter)`: the segment through the
    /// world and zone trees to the rooms, each room's own sprite and then its
    /// static actors (the filter rejects the rest), nearest hit kept; the hit
    /// colour is the baked colour at the hit plus the static light then set.
    pub fn pick_outdoors(&mut self, seg: &mut PickSegment<M>) -> bool {
        let mut v = PickVars { start: seg.start, end: seg.end, stamp: self.state, light: Color32F { a: F::ONE, ..Default::default() } };
        let hit = if self.nodes.is_empty() { self.pick_world(LEAF, &mut v, seg) } else { self.pick_world(0, &mut v, seg) };
        self.state += 1;
        self.stamp += 1;
        hit
    }

    fn pick_world(&mut self, node: u32, v: &mut PickVars<M>, seg: &mut PickSegment<M>) -> bool {
        let mut node = node;
        if node & 0xF000_0000 != 0 {
            if node & LEAF != 0 {
                let z = self.leaves[(node & 0x0FFF_FFFF) as usize] as usize;
                if self.zones[z].leaves.is_empty() {
                    return false;
                }
                let root = if self.zones[z].nodes.is_empty() { LEAF } else { 0 };
                return self.pick_zone(z, root, v, seg);
            }
            node &= 0x0FFF_FFFF;
        }
        let n = self.nodes[node as usize].clone();
        pick_descend(&n, v, |front, v| self.pick_world(if front { n.front } else { n.back }, v, seg))
    }

    fn pick_zone(&mut self, z: usize, node: u32, v: &mut PickVars<M>, seg: &mut PickSegment<M>) -> bool {
        let mut node = node;
        if node & 0xF000_0000 != 0 {
            if node & LEAF != 0 {
                // PickLeaf
                let room = self.zones[z].leaves[(node & 0x0FFF_FFFF) as usize].0 as usize;
                if self.zones[z].rooms[room].stamp20 != v.stamp {
                    self.zones[z].rooms[room].stamp20 = v.stamp;
                    if self.pick_room(z, room, v, seg) {
                        v.end = seg.hit_point();
                        return true;
                    }
                }
                return false;
            }
            node &= 0x0FFF_FFFF;
        }
        let n = self.zones[z].nodes[node as usize].clone();
        pick_descend(&n, v, |front, v| self.pick_zone(z, if front { n.front } else { n.back }, v, seg))
    }

    fn pick_room(&mut self, z: usize, room: usize, v: &mut PickVars<M>, seg: &mut PickSegment<M>) -> bool {
        let mut hit = false;
        if self.zones[z].rooms[room].stamp24 != self.stamp {
            self.zones[z].rooms[room].stamp24 = self.stamp;
            if let Some(sprite) = self.zones[z].rooms[room].sprite {
                // no lighting (m_p444 -1); the static light is what was last set
                hit = self.pick_sprite(sprite, &Matrix44::identity(), None, &mut -1, v, seg);
            }
        }
        for k in 0..self.zones[z].rooms[room].actors.len() {
            let a = self.zones[z].rooms[room].actors[k];
            if self.pick_actor(a, v, seg) {
                hit = true;
            }
        }
        hit
    }

    /// `VIScene::PickActor` under `VISceneOutdoorsFilter`.
    fn pick_actor(&mut self, i: usize, v: &mut PickVars<M>, seg: &mut PickSegment<M>) -> bool {
        let a = &self.actors[i];
        if a.stamp80 == self.stamp || a.flags & 5 != 0 {
            return false;
        }
        self.actors[i].stamp80 = self.stamp;
        let a = &self.actors[i];
        // FilterActor: only static actors
        if a.flags & 8 == 0 || pick::cull_sphere(seg, &a.bound) {
            return false;
        }
        let Some(sprite) = a.sprite else { return false };
        let (m, lighting) = (a.object_to_world, a.lighting);
        v.light = a.color;
        self.pick_sprite(sprite, &m, lighting, &mut -1, v, seg)
    }

    /// The sprite's own `Pick` under the model view `mv`; `lighting` and
    /// `cursor` are `m_p444`/`m_p448`, the colour buffer each simple sprite
    /// takes in turn (`GetNextColorBuffer`).
    fn pick_sprite(&mut self, sprite: usize, mv: &Matrix44<M>, lighting: Option<usize>, cursor: &mut i32, v: &mut PickVars<M>, seg: &mut PickSegment<M>) -> bool {
        match self.sprites[sprite].kind.clone() {
            SpriteKind::Simple(_) => {
                let cb = lighting.and_then(|l| {
                    *cursor += 1;
                    self.lightings[l].get(*cursor as usize)
                });
                let Some(p) = self.sprites[sprite].pick else { return false };
                let pb = &self.picks[p];
                let colors = cb.map(|flat| pick::split_colors(flat, pb));
                pick::pick_prim_buffer(seg, mv, pb, colors.as_deref(), &v.light)
            }
            SpriteKind::Group(members) => {
                for (ms, angles, scale, t) in members {
                    let mem = Matrix44::transform(&Vect3::of(angles[0], angles[1], angles[2]), F::new(scale), &Vect3::of(t[0], t[1], t[2]));
                    let out = Matrix44::mul(&mem, mv);
                    if self.pick_sprite(ms, &out, lighting, cursor, v, seg) {
                        return true;
                    }
                }
                false
            }
            SpriteKind::Lod(levels) => {
                let p = Vect3::new(mv.m[3].x - seg.start.x, mv.m[3].y - seg.start.y, mv.m[3].z - seg.start.z);
                let d = p.x * p.x + p.y * p.y + p.z * p.z;
                let mut i = 0;
                while i < levels.len() && F::<M>::new(levels[i].1) * F::new(levels[i].1) < d {
                    i += 1;
                }
                levels.get(i).is_some_and(|&(l, _)| self.pick_sprite(l, mv, lighting, cursor, v, seg))
            }
            // an animated HSprite's pick (posed, its attachments) is not
            // ported; the rest have none
            _ => false,
        }
    }

    /// `VIScene::Collide(VICollSphere &, VISceneFilter &)`.
    pub fn collide_sphere(&mut self, s: &mut CollSphere<M>, f: &mut Filter) -> bool {
        self.collide_swept(s, f)
    }

    /// `VIScene::Collide(VICollRay &, VISceneFilter &)`: the same walks with
    /// a radius of 0 (the ray's thresholds are the sphere's at 0 exactly).
    pub fn collide_ray(&mut self, r: &mut CollRay<M>, f: &mut Filter) -> bool {
        self.collide_swept(r, f)
    }

    fn collide_swept<S: Swept<M>>(&mut self, s: &mut S, f: &mut Filter) -> bool {
        let (start, end) = s.ends();
        let mut v = Vars { start, end, radius: s.radius(), stamp: self.state };
        let hit = if self.nodes.is_empty() { self.world_node(LEAF, &mut v, s, f) } else { self.world_node(0, &mut v, s, f) };
        self.state += 1;
        self.stamp += 1;
        hit
    }

    fn world_node<S: Swept<M>>(&mut self, node: u32, v: &mut Vars<M>, s: &mut S, f: &mut Filter) -> bool {
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

    fn zone_collide<S: Swept<M>>(&mut self, z: usize, v: &mut Vars<M>, s: &mut S, f: &mut Filter) -> bool {
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

    fn zone_node<S: Swept<M>>(&mut self, z: usize, node: u32, v: &mut Vars<M>, s: &mut S, f: &mut Filter) -> bool {
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

    fn collide_leaf<S: Swept<M>>(&mut self, z: usize, leaf: usize, v: &mut Vars<M>, s: &mut S, f: &mut Filter) -> bool {
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

    fn collide_room<S: Swept<M>>(&mut self, z: usize, room: usize, s: &mut S, f: &mut Filter) -> bool {
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
        for k in 0..self.zones[z].rooms[room].moving.len() {
            let a = self.zones[z].rooms[room].moving[k];
            if self.collide_actor(a, s, f) {
                hit = true;
            }
        }
        hit
    }

    /// `VIScene::CollideActor`.
    fn collide_actor<S: Swept<M>>(&mut self, i: usize, s: &mut S, f: &mut Filter) -> bool {
        let mut hit = false;
        // the ray's version tests the stamp, the flags and the filter first,
        // and culls after
        if S::RAY {
            let a = &self.actors[i];
            if a.stamp80 == self.stamp || a.flags & 0x14 != 0 {
                return false;
            }
            self.actors[i].stamp80 = self.stamp;
            let Some(sprite) = self.actors[i].sprite else { return false };
            if self.filtered(f, i, sprite) || s.cull(&self.actors[i].bound) {
                return false;
            }
            let m = self.actors[i].object_to_world;
            self.collide.set_world(&m);
            hit = self.collide_actor_sprite(i, sprite, s);
            if hit {
                f.hit_actor = i as i32;
                f.hit_sprite = sprite as i32;
            }
            return hit;
        }
        if !s.cull(&self.actors[i].bound) {
            let a = &self.actors[i];
            if a.stamp80 == self.stamp || a.flags & 0x14 != 0 {
                return false;
            }
            self.actors[i].stamp80 = self.stamp;
            let Some(sprite) = self.actors[i].sprite else { return false };
            if self.filtered(f, i, sprite) {
                return false;
            }
            let m = self.actors[i].object_to_world;
            self.collide.set_world(&m);
            hit = self.collide_actor_sprite(i, sprite, s);
            if hit {
                f.hit_actor = i as i32;
                f.hit_sprite = sprite as i32;
            }
        } else {
            self.actors[i].stamp80 = self.stamp;
        }
        hit
    }

    /// The actor's sprite's own `Collide` under its transform: a CSprite is
    /// the actor's own copy, which collides as nothing while it is dead.
    fn collide_actor_sprite<S: Swept<M>>(&mut self, i: usize, sprite: usize, s: &mut S) -> bool {
        if self.actors[i].dead && matches!(self.sprites[sprite].kind, SpriteKind::C) {
            return false;
        }
        self.collide_sprite(sprite, s)
    }

    /// `FilterActor`: the filter's own actor, or (the walk filters) a CSprite.
    fn filtered(&self, f: &Filter, i: usize, sprite: usize) -> bool {
        match f.kind {
            FilterKind::Scene => i as i32 == f.actor,
            FilterKind::WalkFloor => self.sprites[sprite].sprite_type == 5,
            FilterKind::WalkCamera => self.sprites[sprite].sprite_type == 5 || i as i32 == f.actor,
        }
    }

    /// The sprite's own `Collide` virtual, under the collider's current world.
    fn collide_sprite<S: Swept<M>>(&mut self, sprite: usize, s: &mut S) -> bool {
        match self.sprites[sprite].kind.clone() {
            SpriteKind::Simple(Some(b)) => {
                let buf = std::mem::take(&mut self.buffers[b]);
                let hit = s.collide_obj(&mut self.collide, &buf);
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
            SpriteKind::Lod(levels) => levels.first().is_some_and(|&(l, _)| self.collide_sprite(l, s)),
            // VICSprite::Collide: `m_capsule` (its death is the actor's copy's)
            SpriteKind::C => s.collide_capsule(&mut self.collide, &Capsule::csprite()),
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
    pub fn collide_sphere_brute<S: Swept<M>>(&mut self, s: &mut S, zones: &[usize]) -> bool {
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
                let room = &self.zones[z].rooms[room];
                let actors: Vec<usize> = room.actors.iter().chain(&room.moving).copied().collect();
                for a in actors {
                    if !seen.insert(a) || self.actors[a].flags & 0x14 != 0 {
                        continue;
                    }
                    let (Some(sprite), m) = (self.actors[a].sprite, self.actors[a].object_to_world) else { continue };
                    self.collide.set_world(&m);
                    hit |= self.collide_actor_sprite(a, sprite, s);
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

    /// `VIWorld::FindLeaf`'s zone: the world leaf `at` falls in, whatever the
    /// zone's own tree answers (-1 past the leaves).
    pub fn find_zone(&self, at: &Vect3<M>) -> i32 {
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
        self.leaves.get(node as usize).copied().unwrap_or(-1)
    }

    /// `VIScene::QueryIntersection(bb, VIRadialFloraFilter, tl)`: every
    /// triangle of the rooms' own geometry in the box that faces the list's
    /// direction, cut to the box (`VIQueryTriList`). The walk is the world's
    /// tree and each zone's (the query's bounding sphere while it is clear of
    /// a plane, else its eight corners), each room once (the tree's and the
    /// scene's stamps), its sprite under the identity. The flora's filter
    /// takes no actor (`VIRadialFloraFilter::FilterActor` is `return 1`), so
    /// the actors are left out: the game only stamps them, with the pass's
    /// stamp, which the pass then moves on.
    pub fn query_intersection(&mut self, bb: &BBox<M>, tl: &mut QueryTriList<M>) {
        let v = QueryVars { bbox: *bb, corners: bb.store(), sphere: Sphere::from_bbox(bb), stamp: self.state, actors: false };
        let root = if self.nodes.is_empty() { LEAF } else { 0 };
        self.query_world(root, &v, tl);
        self.state += 1;
        self.stamp += 1;
    }

    /// `VIScene::QueryIntersection` under a default `VIActorFilter`, which
    /// filters no actor (`FilterActor` is `actor == -1`): the rooms' own
    /// geometry and every actor's in the box (`QueryIntersectionActor`), as
    /// `RenderShadow` queries it. A character's sprite has none
    /// (`VICSprite::QueryIntersection`), so the placed props are what it adds.
    pub fn query_intersection_actors(&mut self, bb: &BBox<M>, tl: &mut QueryTriList<M>) {
        let v = QueryVars { bbox: *bb, corners: bb.store(), sphere: Sphere::from_bbox(bb), stamp: self.state, actors: true };
        let root = if self.nodes.is_empty() { LEAF } else { 0 };
        self.query_world(root, &v, tl);
        self.state += 1;
        self.stamp += 1;
    }

    fn query_world(&mut self, node: u32, v: &QueryVars<M>, tl: &mut QueryTriList<M>) {
        let mut node = node;
        if node & 0xF000_0000 != 0 {
            if node & LEAF != 0 {
                if let Some(z) = self.leaves.get((node & 0x0FFF_FFFF) as usize).and_then(|&z| usize::try_from(z).ok()) {
                    self.query_zone(z, v, tl);
                }
                return;
            }
            node &= 0x0FFF_FFFF;
        }
        let n = self.nodes[node as usize].clone();
        let (front, back) = split(&n.plane, &v.sphere, &v.corners);
        if front {
            self.query_world(n.front, v, tl);
        }
        if back {
            self.query_world(n.back, v, tl);
        }
    }

    /// `VIZone::QueryIntersection(v)`: the zone's pre-translations, then its
    /// tree (an unloaded zone, a proxy with no leaves, has nothing).
    fn query_zone(&mut self, z: usize, v: &QueryVars<M>, tl: &mut QueryTriList<M>) {
        if !self.zones[z].pretrans.is_empty() {
            let pt = self.zones[z].pretrans.clone();
            self.collide.set_pretranslations(&pt);
        }
        if self.zones[z].leaves.is_empty() {
            return;
        }
        let root = if self.zones[z].nodes.is_empty() { LEAF } else { 0 };
        self.query_zone_node(z, root, v, tl);
    }

    fn query_zone_node(&mut self, z: usize, node: u32, v: &QueryVars<M>, tl: &mut QueryTriList<M>) {
        let mut node = node;
        if node & 0xF000_0000 != 0 {
            if node & LEAF != 0 {
                // QueryIntersectionLeaf, then QueryIntersectionRoom
                let Some(room) = self.zones[z].leaves.get((node & 0x0FFF_FFFF) as usize).and_then(|l| usize::try_from(l.0).ok()) else { return };
                let Some(r) = self.zones[z].rooms.get_mut(room) else { return };
                if r.stamp20 != v.stamp {
                    r.stamp20 = v.stamp;
                    if r.stamp24 != self.stamp {
                        r.stamp24 = self.stamp;
                        if let Some(sprite) = r.sprite {
                            self.query_sprite(sprite, &Matrix44::identity(), &v.bbox, tl);
                        }
                    }
                    // QueryIntersectionRoom's actor list, each under its own
                    // stamp (the flora's filter takes none of them)
                    if v.actors {
                        for k in 0..self.zones[z].rooms[room].actors.len() {
                            let a = self.zones[z].rooms[room].actors[k];
                            self.query_actor(a, v, tl);
                        }
                        for k in 0..self.zones[z].rooms[room].moving.len() {
                            let a = self.zones[z].rooms[room].moving[k];
                            self.query_actor(a, v, tl);
                        }
                    }
                }
                return;
            }
            node &= 0x0FFF_FFFF;
        }
        let n = self.zones[z].nodes[node as usize].clone();
        let (front, back) = split(&n.plane, &v.sphere, &v.corners);
        if front {
            self.query_zone_node(z, n.front, v, tl);
        }
        if back {
            self.query_zone_node(z, n.back, v, tl);
        }
    }

    /// `VIScene::QueryIntersectionActor` under the default filter: an actor
    /// already met this pass or a proxy (`m_p40 & 4`; not the collision's
    /// 0x10) passed over, stamped, culled by the box (`CullIntersect` with its
    /// bound), then its sprite's own query under its transform.
    fn query_actor(&mut self, i: usize, v: &QueryVars<M>, tl: &mut QueryTriList<M>) {
        let a = &self.actors[i];
        if a.stamp80 == self.stamp || a.flags & 4 != 0 {
            return;
        }
        self.actors[i].stamp80 = self.stamp;
        let a = &self.actors[i];
        if v.bbox.cull_intersect(&a.bound) {
            return;
        }
        let Some(sprite) = a.sprite else { return };
        let m = a.object_to_world;
        self.query_sprite(sprite, &m, &v.bbox, tl);
    }

    /// Each sprite's `QueryIntersection(b, l, m, sc)`: a simple sprite its
    /// buffer under `m` (`SetWorld`, `QueryObj`), a group each member under its
    /// placement times `m`, a LOD sprite its first level, an HSprite (posed
    /// for now) each node attachment; the rest nothing.
    fn query_sprite(&mut self, sprite: usize, m: &Matrix44<M>, bb: &BBox<M>, tl: &mut QueryTriList<M>) {
        match self.sprites[sprite].kind.clone() {
            SpriteKind::Simple(Some(b)) => {
                self.collide.set_world(m);
                self.collide.query_obj(bb, tl, &self.buffers[b]);
            }
            SpriteKind::Group(members) => {
                for (ms, angles, scale, t) in members {
                    let mem = Matrix44::transform(&Vect3::of(angles[0], angles[1], angles[2]), F::new(scale), &Vect3::of(t[0], t[1], t[2]));
                    let out = Matrix44::mul(&mem, m);
                    self.query_sprite(ms, &out, bb, tl);
                }
            }
            SpriteKind::Lod(levels) => {
                if let Some(&(l, _)) = levels.first() {
                    self.query_sprite(l, m, bb, tl);
                }
            }
            SpriteKind::H(i) => {
                self.anims.time = self.time;
                let (h, attach) = &mut self.hsprites[i];
                h.process(-1, &self.anims);
                let u = h.calc_trans_about_center(m, &self.anims);
                let parts: Vec<(usize, Matrix44<M>)> = attach
                    .iter()
                    .filter(|&&(_, node)| node != -1)
                    .map(|&(sp, node)| (sp, Matrix44::mul(&h.nodes[node as usize].world, &u)))
                    .collect();
                for (sp, t) in parts {
                    self.query_sprite(sp, &t, bb, tl);
                }
            }
            _ => {}
        }
    }

    /// [`World::query_intersection`] by brute force: every room sprite of
    /// `zones` under the identity, no tree, no stamps. For testing the walk: a
    /// room it fails to reach shows up as triangles missing.
    pub fn query_brute(&mut self, bb: &BBox<M>, tl: &mut QueryTriList<M>, zones: &[usize]) {
        for &z in zones {
            if !self.zones[z].pretrans.is_empty() {
                let pt = self.zones[z].pretrans.clone();
                self.collide.set_pretranslations(&pt);
            }
            for room in 0..self.zones[z].rooms.len() {
                if let Some(sprite) = self.zones[z].rooms[room].sprite {
                    self.query_sprite(sprite, &Matrix44::identity(), bb, tl);
                }
            }
        }
    }

    /// [`World::query_intersection_actors`] by brute force: `query_brute`'s
    /// rooms, then every actor any room of `zones` holds, once each, through
    /// `QueryIntersectionActor`'s tests. For testing the walk, as `query_brute`.
    pub fn query_brute_actors(&mut self, bb: &BBox<M>, tl: &mut QueryTriList<M>, zones: &[usize]) {
        self.query_brute(bb, tl, zones);
        let v = QueryVars { bbox: *bb, corners: bb.store(), sphere: Sphere::from_bbox(bb), stamp: self.state, actors: true };
        self.stamp += 1;
        for &z in zones {
            if !self.zones[z].pretrans.is_empty() {
                let pt = self.zones[z].pretrans.clone();
                self.collide.set_pretranslations(&pt);
            }
            for room in 0..self.zones[z].rooms.len() {
                let r = &self.zones[z].rooms[room];
                let all: Vec<usize> = r.actors.iter().chain(r.moving.iter()).copied().collect();
                for a in all {
                    self.query_actor(a, &v, tl);
                }
            }
        }
        self.stamp += 1;
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
/// `VIScenePickVars`: the segment as the trees see it (its end pulled in to
/// each hit), the walk stamp, and the raster's static light as last set.
struct PickVars<M: Model> {
    start: Vect3<M>,
    end: Vect3<M>,
    stamp: i32,
    light: Color32F<M>,
}

/// `VIWorld::Pick(int, v)` / `VIZone::Pick(int, v)`'s descent: no margin;
/// a straddling segment takes its own side first and the other when, measured
/// again, it still reaches it.
fn pick_descend<M: Model>(n: &Node<M>, v: &mut PickVars<M>, mut go: impl FnMut(bool, &mut PickVars<M>) -> bool) -> bool {
    let ds = n.plane.distance(&v.start);
    let de = n.plane.distance(&v.end);
    if F::ZERO < ds && F::ZERO < de {
        go(true, v)
    } else if ds < F::ZERO && de < F::ZERO {
        go(false, v)
    } else if F::ZERO < ds {
        let mut hit = go(true, v);
        let (ds, de) = (n.plane.distance(&v.start), n.plane.distance(&v.end));
        if (ds < F::ZERO || de < F::ZERO) && go(false, v) {
            hit = true;
        }
        hit
    } else {
        let mut hit = go(false, v);
        let (ds, de) = (n.plane.distance(&v.start), n.plane.distance(&v.end));
        if (F::ZERO < ds || F::ZERO < de) && go(true, v) {
            hit = true;
        }
        hit
    }
}

struct Vars<M: Model> {
    start: Vect3<M>,
    end: Vect3<M>,
    radius: F<M>,
    stamp: i32,
}

const EPS: f32 = 0.03125;

/// What `VIScene::Collide` sweeps through the world: `VICollSphere` or
/// `VICollRay` (whose walks are the sphere's at radius 0, and whose actor
/// test culls last).
pub trait Swept<M: Model> {
    const RAY: bool;
    fn ends(&self) -> (Vect3<M>, Vect3<M>);
    fn radius(&self) -> F<M>;
    fn hit_point(&self) -> Vect3<M>;
    /// `Cull(.., VISphere &)` against an actor's bound.
    fn cull(&self, b: &Sphere<M>) -> bool;
    /// `CollideObj` under the collider's world.
    fn collide_obj(&mut self, c: &mut Collide<M>, buf: &CollBuffer<M>) -> bool;
    /// `VICSprite::Collide`'s capsule under the collider's world: a sphere's
    /// `CollideObj(VICollSphere &, VICapsule &)`; a ray's is nothing
    /// (`Collide(VICollRay &, ...)` answers 0).
    fn collide_capsule(&mut self, c: &mut Collide<M>, cap: &Capsule<M>) -> bool;
}

impl<M: Model> Swept<M> for CollSphere<M> {
    const RAY: bool = false;
    fn ends(&self) -> (Vect3<M>, Vect3<M>) {
        (self.ray.start, self.ray.end)
    }
    fn radius(&self) -> F<M> {
        self.radius
    }
    fn hit_point(&self) -> Vect3<M> {
        CollSphere::hit_point(self)
    }
    fn cull(&self, b: &Sphere<M>) -> bool {
        cull_sphere(self, b)
    }
    fn collide_obj(&mut self, c: &mut Collide<M>, buf: &CollBuffer<M>) -> bool {
        c.collide_obj(self, buf)
    }
    fn collide_capsule(&mut self, c: &mut Collide<M>, cap: &Capsule<M>) -> bool {
        c.collide_capsule_obj(self, cap)
    }
}

impl<M: Model> Swept<M> for CollRay<M> {
    const RAY: bool = true;
    fn ends(&self) -> (Vect3<M>, Vect3<M>) {
        (self.start, self.end)
    }
    fn radius(&self) -> F<M> {
        F::ZERO
    }
    fn hit_point(&self) -> Vect3<M> {
        CollRay::hit_point(self)
    }
    fn cull(&self, b: &Sphere<M>) -> bool {
        cull_ray_sphere(self, b)
    }
    fn collide_obj(&mut self, c: &mut Collide<M>, buf: &CollBuffer<M>) -> bool {
        c.collide_ray_obj(self, buf)
    }
    fn collide_capsule(&mut self, _: &mut Collide<M>, _: &Capsule<M>) -> bool {
        false
    }
}

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

/// `VISceneIntersectVars`, what the query walk carries: the box, its eight
/// corners (`Store`) and sphere (`VISphere::Init`), the tree's stamp.
struct QueryVars<M: Model> {
    bbox: BBox<M>,
    corners: [Vect3<M>; 8],
    sphere: Sphere<M>,
    stamp: i32,
    /// The filter takes the actors (the shadow's `VIActorFilter`), or none
    /// (the flora's `VIRadialFloraFilter`).
    actors: bool,
}

/// `InsertActor`'s choice at a node, and the query's: the bound sphere alone
/// when it is clear of the plane, else the eight corners.
/// `VISceneRendVars`, what `VIScene::Render`'s visibility walk carries as far
/// as `RasterActor`: the camera and the clip stack, the view distance
/// (`cam.m_far`, 500 at most), the walk stamp (`m_stamp`: the world tree's
/// state at `RenderByVolume`), `m_p28` (the world walked by the frustum this
/// pass already: a portal hands it to `RenderByFrustum` once), the eye the
/// distances go by, an actor's off distance (`RasterActor`'s
/// `m_sprites.Value(m_p60)->m_offDist`; None: not one asked for), and the
/// actors rastered so far, in order.
struct Rend<'a, M: Model> {
    cam: &'a Camera<M>,
    eye: Vect3<M>,
    clip: ClipStack<M>,
    view_dist: F<M>,
    stamp: i32,
    frustum_done: bool,
    off: &'a dyn Fn(usize) -> Option<F<M>>,
    out: Vec<usize>,
}

/// A BSP node's sides as the clip volume's points `verts` reach them: in
/// front where one is past the plane, behind where one is not.
fn volume_sides<M: Model>(pl: &Plane<M>, verts: &[Vect3<M>]) -> (bool, bool) {
    let (mut front, mut back) = (false, false);
    for p in verts {
        if F::ZERO < pl.distance(p) {
            front = true;
        } else {
            back = true;
        }
    }
    (front, back)
}

impl<M: Model> World<M> {
    /// `VIScene::Render`'s walk (`RenderByPortals`: the scene's own mode, not
    /// `RenderByFrustum`'s) as far as `RasterActor`: the actors it rasters this
    /// pass, in its order. From the camera's room: an outdoor room (bit 0)
    /// walks the world by the frustum (`RenderByFrustum`: the world BSP and each
    /// zone's by the clip volume's points, `RenderLeaf` taking only outdoor
    /// rooms), an indoor one draws itself under the frustum (`PushFrustum`);
    /// each room drawn (`RenderRoom`) is culled by its box against the clip
    /// stack's top, draws through each portal the camera is in front of
    /// (`RenderThroughPortals`: the portal clipped by the top, the room behind
    /// it under the volume the clipped portal makes, `PushPortal`; an outdoor
    /// room behind it the whole world by the frustum, once), then rasters its
    /// actors when its box is within the view distance of `eye`. `RasterActor`
    /// passes an actor rastered already this pass (`m_p80`), hidden or a proxy
    /// (`m_p40 & 5`), further than its off distance from `eye`, or outside the
    /// top (`CullByTop` of its bound), and stamps the rest. The camera's place
    /// decides the rooms and the portals; `eye` the distances (the client's
    /// `lod::eye`, the game's camera at its own ceiling).
    pub fn raster_actors(&mut self, cam: &Camera<M>, eye: &Vect3<M>, off: &dyn Fn(usize) -> Option<F<M>>) -> Vec<usize> {
        let d = cam.far;
        let view_dist = if f(500.0) < d { f(500.0) } else { d };
        let mut v = Rend { cam, eye: *eye, clip: ClipStack::new(), view_dist, stamp: 0, frustum_done: false, off, out: Vec::new() };
        self.render_by_portals(&mut v);
        // `Render`'s `m_stamp++` after the passes
        self.stamp += 1;
        v.out
    }

    /// `VIWorld::IsEmpty`.
    fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.leaves.is_empty()
    }

    /// `VIWorld::RenderByPortals`.
    fn render_by_portals(&mut self, v: &mut Rend<M>) {
        if self.is_empty() {
            return;
        }
        let Some((zone, leaf)) = self.find_leaf(&v.cam.position) else { return };
        let Some(room) = usize::try_from(self.zones[zone].leaves[leaf].0).ok().filter(|&r| r < self.zones[zone].rooms.len()) else { return };
        if self.zones[zone].rooms[room].flags & 1 != 0 {
            self.render_by_frustum(v);
        } else {
            v.clip.push_frustum(v.cam);
            self.render_room(zone, room, v);
            v.clip.pop();
        }
    }

    /// `VIWorld::RenderByFrustum`.
    fn render_by_frustum(&mut self, v: &mut Rend<M>) {
        v.frustum_done = true;
        if self.is_empty() {
            return;
        }
        v.clip.push_frustum(v.cam);
        v.stamp = self.state;
        let root = if self.nodes.is_empty() { LEAF } else { 0 };
        self.render_world_volume(root, v);
        self.state += 1;
        v.clip.pop();
    }

    /// `VIWorld::RenderByVolume(node)`: down the world BSP to the sides the
    /// clip volume's points reach, the side away from the camera first.
    fn render_world_volume(&mut self, node: u32, v: &mut Rend<M>) {
        let mut node = node;
        if node & 0xF000_0000 != 0 {
            if node & LEAF != 0 {
                let Some(&z) = self.leaves.get((node & 0x0FFF_FFFF) as usize) else { return };
                self.render_zone_volume(z as usize, v);
                return;
            }
            node &= 0x0FFF_FFFF;
        }
        let Some(n) = self.nodes.get(node as usize).cloned() else { return };
        let (front, back) = volume_sides(&n.plane, v.clip.top().0);
        if n.plane.distance(&v.cam.position) < F::ZERO {
            if front {
                self.render_world_volume(n.front, v);
            }
            if back {
                self.render_world_volume(n.back, v);
            }
        } else {
            if back {
                self.render_world_volume(n.back, v);
            }
            if front {
                self.render_world_volume(n.front, v);
            }
        }
    }

    /// `VIZone::RenderByVolume(v, zone)`: an unloaded zone (no tree, no
    /// rooms) has nothing to walk.
    fn render_zone_volume(&mut self, z: usize, v: &mut Rend<M>) {
        if z >= self.zones.len() || self.zones[z].leaves.is_empty() {
            return;
        }
        let root = if self.zones[z].nodes.is_empty() { LEAF } else { 0 };
        self.render_zone_node(z, root, v);
    }

    /// `VIZone::RenderByVolume(node)`.
    fn render_zone_node(&mut self, z: usize, node: u32, v: &mut Rend<M>) {
        let mut node = node;
        if node & 0xF000_0000 != 0 {
            if node & LEAF != 0 {
                self.render_leaf(z, (node & 0x0FFF_FFFF) as usize, v);
                return;
            }
            node &= 0x0FFF_FFFF;
        }
        let Some(n) = self.zones[z].nodes.get(node as usize).cloned() else { return };
        let (front, back) = volume_sides(&n.plane, v.clip.top().0);
        if n.plane.distance(&v.cam.position) < F::ZERO {
            if front {
                self.render_zone_node(z, n.front, v);
            }
            if back {
                self.render_zone_node(z, n.back, v);
            }
        } else {
            if back {
                self.render_zone_node(z, n.back, v);
            }
            if front {
                self.render_zone_node(z, n.front, v);
            }
        }
    }

    /// `VIZone::RenderLeaf`: the leaf's room, once a walk (`m_p20`), and in
    /// the portals' mode only an outdoor one (an indoor room is reached
    /// through the portals).
    fn render_leaf(&mut self, z: usize, leaf: usize, v: &mut Rend<M>) {
        let Some(&(room, _)) = self.zones[z].leaves.get(leaf) else { return };
        let Some(r) = usize::try_from(room).ok().filter(|&r| r < self.zones[z].rooms.len()) else { return };
        if self.zones[z].rooms[r].stamp20 != v.stamp {
            self.zones[z].rooms[r].stamp20 = v.stamp;
            if self.zones[z].rooms[r].flags & 1 != 0 {
                self.render_room(z, r, v);
            }
        }
    }

    /// `VIZone::RenderRoom`, as far as its actors (its own sprite is drawn
    /// only: nothing here).
    fn render_room(&mut self, z: usize, r: usize, v: &mut Rend<M>) {
        if v.clip.cull_bbox(&self.zones[z].rooms[r].bbox) {
            return;
        }
        self.render_through_portals(z, r, v);
        if self.zones[z].rooms[r].bbox.distance_squared(&v.eye) < v.view_dist * v.view_dist {
            let room = &self.zones[z].rooms[r];
            let list: Vec<usize> = room.actors.iter().chain(room.moving.iter()).copied().collect();
            for a in list {
                self.raster_actor(a, v);
            }
        }
    }

    /// `VIZone::RenderThroughPortals`.
    fn render_through_portals(&mut self, z: usize, r: usize, v: &mut Rend<M>) {
        for i in 0..self.zones[z].rooms[r].portal_list.len() {
            let p = &self.zones[z].rooms[r].portal_list[i];
            if p.open {
                continue;
            }
            let d = p.plane.distance(&v.cam.position);
            if d < F::ZERO {
                continue;
            }
            let inside = d < f(0.02);
            let dest = p.dest;
            let clipped = if inside {
                None
            } else {
                match v.clip.clip_by_top(&p.verts) {
                    Some(out) => Some(out),
                    None => continue,
                }
            };
            let Some(dr) = usize::try_from(dest).ok().filter(|&d| d < self.zones[z].rooms.len()) else { continue };
            if self.zones[z].rooms[dr].flags & 1 == 0 {
                if let Some(out) = &clipped {
                    v.clip.push_portal(v.cam, out);
                }
                self.zones[z].rooms[r].portal_list[i].open = true;
                self.render_room(z, dr, v);
                if clipped.is_some() {
                    v.clip.pop();
                }
                self.zones[z].rooms[r].portal_list[i].open = false;
            } else if !v.frustum_done {
                self.render_by_frustum(v);
            }
        }
    }

    /// `VIScene::RasterActor`'s tests, as far as its stamp.
    fn raster_actor(&mut self, i: usize, v: &mut Rend<M>) {
        let Some(a) = self.actors.get(i) else { return };
        if a.stamp80 == self.stamp || a.flags & 5 != 0 {
            return;
        }
        let Some(off) = (v.off)(i) else { return };
        let (dx, dy, dz) = (a.location.x - v.eye.x, a.location.y - v.eye.y, a.location.z - v.eye.z);
        let d2 = dx * dx + dy * dy + dz * dz;
        if !(d2 < off * off) {
            return;
        }
        if v.clip.cull_sphere(&a.bound) {
            return;
        }
        self.actors[i].stamp80 = self.stamp;
        v.out.push(i);
    }
}

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

/// What the radial flora reads of the scene. A world with no grid record
/// is `VIWorld`'s constructor's: one cell, 2000 a side, about the origin, in a
/// box of +/-1000.
impl<M: Model> FloraScene<M> for World<M> {
    fn grid_rows(&self) -> i32 {
        self.grid.as_ref().map_or(1, |g| g.rows)
    }
    fn grid_cols(&self) -> i32 {
        self.grid.as_ref().map_or(1, |g| g.cols)
    }
    fn grid_center(&self, a: i32, b: i32) -> Vect3<M> {
        self.grid.as_ref().map_or_else(Vect3::default, |g| g.center(a, b))
    }
    fn grid_offset(&self, v: &Vect3<M>) -> (i32, i32, Vect3<M>) {
        self.grid.as_ref().map_or((0, 0, *v), |g| g.offset(v))
    }
    fn world_bbox(&self) -> BBox<M> {
        self.grid.as_ref().map_or_else(
            || {
                let k = F::new(1000.0);
                BBox { min: Vect3::new(-k, -k, -k), max: Vect3::new(k, k, k) }
            },
            |g| g.bbox,
        )
    }
    fn find_zone(&self, v: &Vect3<M>) -> i32 {
        World::find_zone(self, v)
    }
    fn flora(&self, zone: i32) -> Option<&ZoneFlora<M>> {
        usize::try_from(zone).ok().and_then(|z| self.zones.get(z)).map(|z| &z.flora)
    }
    fn query(&mut self, b: &BBox<M>, l: &mut QueryTriList<M>) {
        self.query_intersection(b, l)
    }
    fn time(&self) -> i64 {
        self.time
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

/// `VIScene::CalcOutdoorsIntensity`'s state (`m_soundTrack.m_p28`, `m_p30`):
/// a 15-unit segment straight down from the eye, the baked colour's alpha
/// where it lands (sun visibility: 1 under the sky, low under a roof), eased
/// toward at 0.2 a second. A miss keeps the last value.
#[derive(Clone, Copy, Debug)]
pub struct Outdoors<M: Model> {
    last: i64,
    pub intensity: F<M>,
}

impl<M: Model> Default for Outdoors<M> {
    fn default() -> Self {
        Outdoors { last: 0, intensity: F::ONE }
    }
}

impl<M: Model> Outdoors<M> {
    pub fn update(&mut self, w: &mut World<M>, eye: &Vect3<M>, now: i64) -> F<M> {
        let b = Vect3::new(eye.x, eye.y - F::new(15.0), eye.z);
        let mut seg = PickSegment::new(*eye, b);
        if w.pick_outdoors(&mut seg) {
            if self.last != 0 {
                let lim = F::from_int((now - self.last) as i32) * F::new(0.001) * F::new(0.2);
                let d = seg.hit_color.a - self.intensity;
                let c = crate::fpu::vimin(lim, crate::fpu::vimax(-lim, d));
                self.intensity = self.intensity + c;
            } else {
                self.intensity = seg.hit_color.a;
            }
            self.last = now;
        }
        self.intensity
    }
}
