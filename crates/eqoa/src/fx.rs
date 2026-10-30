//! The effect volumes placed in the world (`VIEffectVolumeSprite`, ESF
//! 0xC300): boxes of ground fog, dust, light shafts, falling leaves or a
//! flock of birds, whose game logic is `vi::effect` (held to the decomp frame
//! by frame).
//!
//! `VIESFParse::ParseEffectVolumeSpriteObj` reads the record: a header (the
//! DictID and the box), then an optional 0xC320 whose sub-objects (the
//! surface, the flock's CSprite) register in the dictionary first, then
//! 0xC330, the volume's own parameters. Zone actors place it like any
//! sprite, and its `GetInstType` is 1, so each placement gets its own copy
//! (`VIEffectVolumeSprite::Copy`) and its own volume state.

use esf::{Container, NodeId};

/// `VIEffectVolumeSprite`'s record.
pub const RECORD: u16 = 0xC300;

/// 0xC330, `ParseEffectVolumeSpriteObj`'s second half.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// `VIEffectVolumeType`: 1 leaves, 2 shafts, 3 fog, 4 dust, 5 flock
    pub ty: i32,
    /// the hours it shows, from and to (`m_p34`, `m_p38`)
    pub p34: f32,
    pub p38: f32,
    /// how far it is seen (`m_p30`): faded over the last quarter
    pub p30: f32,
    /// its strength (`m_p28`)
    pub p28: f32,
    pub density: f32,
    /// the surface's DictID and the flock's CSprite's (-1 none)
    pub surface: i32,
    pub sprite: i32,
    /// from version 2 (bit 1: shafts that follow the sun)
    pub flags: u32,
}

/// An effect volume record as the parser reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Record {
    pub id: i32,
    /// min x y z, max x y z
    pub bbox: [f32; 6],
    /// None without a 0xC330: the volume keeps `Init`'s (no type)
    pub params: Option<Params>,
}

fn f32_at(d: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}

fn i32_at(d: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}

impl Record {
    /// `ParseEffectVolumeSpriteObj`: the first child is the header (whatever
    /// its type), then 0xC320 if there is one, then 0xC330 if there is one.
    pub fn read(c: &Container, node: NodeId) -> Option<Record> {
        let n = c.node(node);
        if n.ty != RECORD {
            return None;
        }
        let mut kids = n.children.iter().copied();
        let d = c.body(kids.next()?);
        if d.len() < 28 {
            return None;
        }
        let bbox = [1, 2, 3, 4, 5, 6].map(|k| f32_at(d, k * 4));
        let mut next = kids.next();
        if next.is_some_and(|k| c.node(k).ty == 0xC320) {
            next = kids.next();
        }
        let params = next.filter(|&k| c.node(k).ty == 0xC330).and_then(|k| {
            let d = c.body(k);
            (d.len() >= 32).then(|| Params {
                ty: i32_at(d, 0),
                p34: f32_at(d, 4),
                p38: f32_at(d, 8),
                p30: f32_at(d, 12),
                p28: f32_at(d, 16),
                density: f32_at(d, 20),
                surface: i32_at(d, 24),
                sprite: i32_at(d, 28),
                flags: if c.node(k).version >= 2 && d.len() >= 36 { i32_at(d, 32) as u32 } else { 0 },
            })
        });
        Some(Record { id: i32_at(d, 0), bbox, params })
    }
}

/// A type's name.
pub fn type_name(ty: i32) -> &'static str {
    match ty {
        0 => "none",
        1 => "leaves",
        2 => "shafts",
        3 => "fog",
        4 => "dust",
        5 => "flock",
        _ => "?",
    }
}

/// One placement of a volume, for `eqoa volumes`.
#[derive(Clone, Debug)]
pub struct Placement {
    pub zone: usize,
    pub actor: esf::zone::Actor,
    pub record: Record,
    /// the surface and the sprite resolve in the zone's own records
    pub surface_found: bool,
    pub sprite_found: bool,
}

/// Every effect volume a zone's records place (`zone` is the zone record in
/// `c`, a zone's container or a world read whole). An actor's sprite is the
/// zone's own (every actor names a sprite its own zone holds: `vi::world`).
pub fn placements(c: &Container, zone: NodeId, index: usize) -> Vec<Placement> {
    let found = |id: i32, ty: u16| c.lookup(id).is_some_and(|n| c.node(n).ty == ty);
    let mut out = Vec::new();
    for a in c.actors(zone) {
        let Some(n) = c.lookup(a.sprite) else { continue };
        let Some(record) = Record::read(c, n) else { continue };
        let (s, p) = record.params.map_or((false, false), |p| (found(p.surface, esf::types::SURFACE), found(p.sprite, esf::types::CSPRITE)));
        out.push(Placement { zone: index, actor: a, record, surface_found: s, sprite_found: p });
    }
    out
}

// ------------------------------------------------------------- in the world --

use esf::{Library, Ref};
use glam::{Mat4, Vec3};
use render::fans::{Quad, Run};
use render::wgpu;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use vi::character::{AnimCache, load_csprite};
use vi::csprite::{CSprite, Rand};
use vi::effect::{Camera, Cards, Draw, Host, Particles, VolumeSprite};
use vi::fpu::{Ee, F};
use vi::geom::{BBox, Matrix44};
use vi::hsprite::AnimScene;
use vi::vect::Vect3;
use vi::world::World;

/// `VISprite::VISprite`'s off distance, which the volume's sprite keeps:
/// `VIScene::RasterActor` passes over an actor no nearer the camera.
pub const OFF_DIST: f32 = 500.0;

/// Slots handed out by index, a freed one taken again first: the materials
/// and sprites the volumes hold handles to.
struct Slab<T> {
    slots: Vec<Option<T>>,
    free: Vec<usize>,
}

impl<T> Default for Slab<T> {
    fn default() -> Self {
        Slab { slots: Vec::new(), free: Vec::new() }
    }
}

impl<T> Slab<T> {
    fn insert(&mut self, v: T) -> i32 {
        match self.free.pop() {
            Some(k) => {
                self.slots[k] = Some(v);
                k as i32
            }
            None => {
                self.slots.push(Some(v));
                self.slots.len() as i32 - 1
            }
        }
    }
    fn remove(&mut self, k: i32) -> Option<T> {
        let v = usize::try_from(k).ok().and_then(|k| self.slots.get_mut(k)).and_then(Option::take);
        if v.is_some() {
            self.free.push(k as usize);
        }
        v
    }
    fn get_mut(&mut self, k: i32) -> Option<&mut T> {
        usize::try_from(k).ok().and_then(|k| self.slots.get_mut(k)).and_then(Option::as_mut)
    }
    fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }
}

/// A boid's sprite: a copy of its flock's CSprite (`VIScene::CopySprite`),
/// animated on its own, and where it was rastered this frame.
pub struct Boid {
    base: Ref,
    cs: Option<CSprite<Ee>>,
    rand: Rand,
    gpu: Option<render::GpuModel>,
    /// this frame's transform (`VIBoid::Render`'s model-view, in the world)
    drawn: Option<Matrix44<Ee>>,
    /// its centre transform and skin palette this frame (`VIHSprite::Raster`)
    pose: Option<Pose>,
}

/// A centre transform, and a skin palette column-major.
type Pose = (Matrix44<Ee>, Vec<[[f32; 4]; 4]>);

enum Sprite {
    /// a flock's CSprite as its record names it (`AttachSprite`'s index)
    Base(Ref),
    Boid(Box<Boid>),
}

/// What the volumes ask of the scene and the raster (`vi::effect::Host`):
/// the materials `AttachSurface` makes, the boids' sprites, the hour.
struct Scene<'a> {
    lib: &'a Library,
    materials: &'a mut Slab<(i32, i32)>,
    sprites: &'a mut Slab<Sprite>,
    spare: &'a mut HashMap<Ref, Vec<Box<Boid>>>,
    anims: &'a mut AnimScene<Ee>,
    cache: &'a mut AnimCache,
    seed: &'a mut u64,
    hour: F<Ee>,
}

impl Host<Ee> for Scene<'_> {
    /// the surface handle is its DictID
    fn create_material(&mut self, surface: i32, blend: i32) -> i32 {
        self.materials.insert((surface, blend))
    }
    fn release_material(&mut self, m: i32) {
        self.materials.remove(m);
    }
    fn copy_sprite(&mut self, sprite: i32) -> i32 {
        let Some(Sprite::Base(base)) = self.sprites.get_mut(sprite).map(|s| match s {
            Sprite::Base(r) => Sprite::Base(*r),
            Sprite::Boid(b) => Sprite::Base(b.base),
        }) else {
            return -1;
        };
        // a boid let go with its flock serves again (its clips and model are
        // the same), else a new copy, idling as `SetDefaults` leaves it
        let boid = match self.spare.get_mut(&base).and_then(Vec::pop) {
            Some(b) => b,
            None => {
                *self.seed += 1;
                let mut rand = Rand(*self.seed);
                let cs = load_csprite(self.lib, base, self.anims, self.cache, &mut rand);
                Box::new(Boid { base, cs, rand, gpu: None, drawn: None, pose: None })
            }
        };
        self.sprites.insert(Sprite::Boid(boid))
    }
    fn release_sprite(&mut self, sprite: i32) {
        if let Some(Sprite::Boid(mut b)) = self.sprites.remove(sprite) {
            b.drawn = None;
            if let Some(g) = b.gpu.as_mut() {
                g.visible = false;
            }
            self.spare.entry(b.base).or_default().push(b);
        }
    }
    fn set_locomotion(&mut self, sprite: i32, anim: i32) {
        if let Some(Sprite::Boid(b)) = self.sprites.get_mut(sprite)
            && let Some(cs) = b.cs.as_mut()
        {
            cs.set_locomotion(anim, self.anims, &mut b.rand);
        }
    }
    fn hour(&self) -> F<Ee> {
        self.hour
    }
}

/// One placement: its actor, its own copy of the volume, the actor's
/// transform and its bound in the world (centre, radius).
struct Instance {
    actor: usize,
    vs: VolumeSprite<Ee>,
    m: Matrix44<Ee>,
    bound: (Vec3, f32),
}

/// A camera as the volumes read it (`VICamera::Update`): the view with the
/// axes as columns, the world with them as rows, and the forward vector.
pub fn camera(position: Vect3<Ee>, direction: Vect3<Ee>) -> Camera<Ee> {
    let (view, dir) = vi::emit::camera_view(&position, &direction);
    let mut world = Matrix44::identity();
    for (k, row) in world.m.iter_mut().take(3).enumerate() {
        let c = |r: usize| [view.m[r].x, view.m[r].y, view.m[r].z][k];
        row.x = c(0);
        row.y = c(1);
        row.z = c(2);
    }
    world.m[3].x = position.x;
    world.m[3].y = position.y;
    world.m[3].z = position.z;
    Camera { position, dir, view, world }
}

/// A fan as the renderer draws it (`render::fans`): its front face, the
/// first four vertices, taken from the volume's space into the world by the
/// actor's transform `m` and placed relative to `eye` (in doubles: the world
/// is tens of thousands out); the normal turned with them.
pub fn quad(fan: &vi::effect::Fan<Ee>, m: &Matrix44<Ee>, eye: glam::DVec3) -> Quad {
    let [r0, r1, r2, t] = m.m.map(|r| glam::DVec3::new(r.x.0 as f64, r.y.0 as f64, r.z.0 as f64));
    let t = t - eye;
    let at = |v: &Vect3<Ee>| r0 * v.x.0 as f64 + r1 * v.y.0 as f64 + r2 * v.z.0 as f64;
    Quad {
        corners: [0, 1, 2, 3].map(|k| (at(&fan.verts[k].1) + t).as_vec3().to_array()),
        uvs: [0, 1, 2, 3].map(|k| [fan.verts[k].0[0].0, fan.verts[k].0[1].0]),
        color: [fan.color.r.0, fan.color.g.0, fan.color.b.0, fan.color.a.0],
        normal: at(&fan.normal).normalize_or_zero().as_vec3().to_array(),
    }
}

/// `VIScene::RasterActor`'s tests: an actor neither hidden nor a proxy
/// (`m_p40 & 5`), nearer the camera at `eye` than its sprite's off
/// distance, and its bound (centre, radius) in view (`CullByTop`: here the
/// view's planes, `particles::frustum`'s).
pub fn rastered(flags: u32, at: &Vect3<Ee>, bound: (Vec3, f32), eye: &Vect3<Ee>, planes: &[glam::Vec4; 6]) -> bool {
    let (c, r) = bound;
    flags & 5 == 0 && crate::lod::within(at, eye, OFF_DIST) && planes.iter().all(|p| p.dot(c.extend(1.0)) >= -r)
}

/// This frame's count of what the volumes did (`describe`, `EQOA_PERF`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub volumes: usize,
    pub rastered: usize,
    pub fans: usize,
    pub boids: usize,
}

/// `Fx::scene`'s: the host, the zones' volumes, the cards, the rand, the
/// raster's output (its normal carried from fan to fan).
type Parts<'a> = (Scene<'a>, &'a mut BTreeMap<usize, Vec<Instance>>, &'a Cards<Ee>, &'a mut Rand, &'a mut Draw<Ee>);

/// The effect volumes of the zones streamed in round the camera.
pub struct Fx {
    cards: Cards<Ee>,
    rand: Rand,
    zones: BTreeMap<usize, Vec<Instance>>,
    materials: Slab<(i32, i32)>,
    sprites: Slab<Sprite>,
    bases: HashMap<Ref, i32>,
    spare: HashMap<Ref, Vec<Box<Boid>>>,
    anims: AnimScene<Ee>,
    cache: AnimCache,
    seed: u64,
    /// this frame's fans, a run per material
    pub runs: Vec<Run>,
    /// the flocks' models, by their CSprite
    models: HashMap<Ref, Option<esf::Model>>,
    /// where this frame's flocks' levels were measured from (`lod_eye`, else
    /// the camera), and the eye its fans are placed relative to (the view's)
    eye: Vect3<Ee>,
    fans_eye: Vec3,
    /// where the flocks' levels are measured from, as the characters' are:
    /// no farther than the game's camera could be (`lod::eye`), set by the
    /// caller before each frame; None, the camera itself
    pub lod_eye: Option<Vect3<Ee>>,
    /// where the zones were last streamed for, and how many the world had in
    streamed: Option<(Vec3, usize)>,
    /// the raster's immediate-mode output, kept for its room
    draw: Draw<Ee>,
    pub stats: Stats,
}

impl Default for Fx {
    fn default() -> Self {
        Self::new()
    }
}

impl Fx {
    pub fn new() -> Self {
        Fx {
            cards: Cards::new(),
            rand: Rand(1),
            zones: BTreeMap::new(),
            materials: Slab::default(),
            sprites: Slab::default(),
            bases: HashMap::new(),
            spare: HashMap::new(),
            anims: AnimScene::default(),
            cache: AnimCache::new(),
            seed: 0,
            runs: Vec::new(),
            models: HashMap::new(),
            eye: Vect3::default(),
            fans_eye: Vec3::ZERO,
            lod_eye: None,
            streamed: None,
            draw: Draw::default(),
            stats: Stats::default(),
        }
    }

    /// The host the volumes ask, beside the rest of what a raster takes.
    fn scene<'a>(&'a mut self, lib: &'a Library, hour: F<Ee>) -> Parts<'a> {
        let s = Scene { lib, materials: &mut self.materials, sprites: &mut self.sprites, spare: &mut self.spare, anims: &mut self.anims, cache: &mut self.cache, seed: &mut self.seed, hour };
        (s, &mut self.zones, &self.cards, &mut self.rand, &mut self.draw)
    }

    /// The zones in and out round `at` as `VIWorld::StreamLeaf` takes them
    /// (`World::stream`, the streamer's rule): a zone in the collision world
    /// gets its volumes, a copy each (`CreateActor`, `Copy`); a zone let go
    /// takes them with it (`StreamRoomStaticsOut`: `ReleaseActor`, `Free`).
    pub fn stream(&mut self, at: Vec3, world: &World<Ee>, lib: &Library) {
        // nothing to do until the place moves or another zone comes in
        let ready = world.loaded.iter().filter(|&&l| l).count();
        if self.streamed.is_some_and(|(p, n)| n == ready && (p - at).length_squared() < 1.0) {
            return;
        }
        self.streamed = Some((at, ready));
        let loaded: BTreeSet<usize> = self.zones.keys().copied().collect();
        let (load, release) = world.stream(&Vect3::of(at.x, at.y, at.z), &loaded);
        for z in release {
            self.release(z, lib);
        }
        for z in load.into_iter().filter(|&z| world.loaded.get(z).copied().unwrap_or(false)) {
            let list = self.instances(world, lib, z);
            self.zones.insert(z, list);
        }
    }

    fn release(&mut self, z: usize, lib: &Library) {
        let Some(list) = self.zones.remove(&z) else { return };
        let (mut host, ..) = self.scene(lib, F::ZERO);
        for mut i in list {
            i.vs.volume.free(&mut host);
        }
    }

    /// Zone `z`'s volumes, in its rooms' placement order.
    fn instances(&mut self, world: &World<Ee>, lib: &Library, z: usize) -> Vec<Instance> {
        let mut out = Vec::new();
        let Some(zone) = world.zones.get(z) else { return out };
        for room in &zone.rooms {
            for &a in &room.placements {
                let actor = &world.actors[a];
                let Some(s) = actor.sprite else { continue };
                let r = world.sprite_refs[s];
                let Some(rec) = Record::read(lib.file(r.file), r.node) else { continue };
                let Some(p) = rec.params else { continue };
                let f = |k: usize| F::<Ee>::new(rec.bbox[k]);
                let b = BBox { min: Vect3::new(f(0), f(1), f(2)), max: Vect3::new(f(3), f(4), f(5)) };
                // AttachSurface's surface and AttachSprite's sprite, as the
                // dictionary resolves them (-1 when it does not)
                let surface = if lib.surface(p.surface).is_some() { p.surface } else { -1 };
                let sprite = match lib.lookup(p.sprite).filter(|q| lib.file(q.file).node(q.node).ty == esf::types::CSPRITE) {
                    Some(q) => *self.bases.entry(q).or_insert_with(|| self.sprites.insert(Sprite::Base(q))),
                    None => -1,
                };
                let (mut host, ..) = self.scene(lib, F::ZERO);
                // VIEffectVolumeSprite::Copy
                let mut vs = VolumeSprite { bbox: b, local_bbox: b, volume: Default::default() };
                vs.volume.init(&b, &mut host);
                vs.volume.set_type(p.ty, F::new(p.density), &mut host);
                vs.volume.p34 = F::new(p.p34);
                vs.volume.p38 = F::new(p.p38);
                vs.volume.flags = p.flags;
                vs.volume.p30 = F::new(p.p30);
                vs.volume.p28 = F::new(p.p28);
                vs.volume.attach_surface(surface, &mut host);
                vs.volume.attach_sprite(sprite);
                let sphere = vi::geom::Sphere::from_bbox(&b);
                let c = actor.object_to_world.mul_point(&sphere.c);
                let bound = (Vec3::new(c.x.0, c.y.0, c.z.0), (sphere.r * actor.scale).0.abs());
                out.push(Instance { actor: a, vs, m: actor.object_to_world, bound });
            }
        }
        out
    }

    /// One frame at wall-clock `now` (ms) and `hour`, for `cam` seeing
    /// through `view_proj` from `eye` (the view's own, which the fans are
    /// placed relative to): each volume `VIScene::RasterActor` would draw (in
    /// the world, nearer than its off distance, in view) steps and renders
    /// as `VIEffectVolumeSprite::Raster` does, into this frame's fans and
    /// boids.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, world: &World<Ee>, lib: &Library, cam: &Camera<Ee>, view_proj: Mat4, eye: Vec3, now: i64, hour: f32) {
        self.anims.time = now;
        for s in self.sprites.slots.iter_mut().flatten() {
            if let Sprite::Boid(b) = s {
                b.drawn = None;
            }
        }
        let planes = crate::particles::frustum(view_proj);
        let mut runs: Vec<Run> = Vec::new();
        let mut stats = Stats::default();
        let (mut host, zones, cards, rand, draw) = self.scene(lib, F::new(hour));
        let eye_d = eye.as_dvec3();
        for inst in zones.values_mut().flatten() {
            stats.volumes += 1;
            let actor = &world.actors[inst.actor];
            if !rastered(actor.flags, &actor.location, inst.bound, &cam.position, &planes) {
                continue;
            }
            stats.rastered += 1;
            let modelview = Matrix44::mul(&inst.m, &cam.view);
            // the raster's normal carries on from the last fan drawn
            draw.fans.clear();
            draw.sprites.clear();
            inst.vs.raster(cam, &modelview, now, cards, rand, &mut host, draw);
            for fan in &draw.fans {
                // the material, or the raster's solid fill (`SetSolidFillUIAVGMaterial`:
                // untextured, blended, Z written)
                let (texture, blend, zwrite) = match host.materials.get_mut(fan.material) {
                    Some(&mut (s, b)) => (s, b.clamp(0, 4) as usize, false),
                    None => (0, 3, true),
                };
                let q = quad(fan, &inst.m, eye_d);
                match runs.last_mut().filter(|r| (r.texture, r.blend, r.zwrite) == (texture, blend, zwrite)) {
                    Some(r) => r.quads.push(q),
                    None => runs.push(Run { texture, blend, zwrite, quads: vec![q] }),
                }
                stats.fans += 1;
            }
            // VIBoid::Render: each boid's sprite under `m_xform` in the
            // volume's space
            if !draw.sprites.is_empty()
                && let Particles::Flock(boids) = &inst.vs.volume.particles
            {
                for b in boids.iter().filter(|b| b.sprite != -1) {
                    if let Some(Sprite::Boid(s)) = host.sprites.get_mut(b.sprite) {
                        s.drawn = Some(Matrix44::mul(&b.xform, &inst.m));
                        stats.boids += 1;
                    }
                }
            }
        }
        // VIHSprite::Raster for each boid drawn: its level for its
        // distance (`lod::anim_level`, from where the levels go by; every
        // node with Level of Detail off), the pose, the centre transform and
        // the skin palette
        let anims = &self.anims;
        let lod_eye = self.lod_eye.unwrap_or(cam.position);
        let lod_on = crate::settings::client().lod;
        for s in self.sprites.slots.iter_mut().flatten() {
            let Sprite::Boid(b) = s else { continue };
            b.pose = match (b.drawn, b.cs.as_mut()) {
                (Some(m), Some(cs)) => {
                    let lod = crate::lod::anim_level(&cs.h, &m, &lod_eye, lod_on);
                    cs.h.process(lod, anims);
                    let centre = cs.h.calc_trans_about_center(&m, anims);
                    cs.h.set_blend_matrices();
                    Some((centre, cs.h.blend.iter().map(|m| m.to_cols()).collect()))
                }
                (Some(m), None) => Some((m, Vec::new())),
                _ => None,
            };
        }
        self.runs = runs;
        self.stats = stats;
        self.eye = lod_eye;
        self.fans_eye = eye;
    }

    /// This frame's fans to the renderer, and the boids' models posed (each
    /// uploaded the first time it is drawn).
    pub fn upload(&mut self, renderer: &mut render::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, lib: &Library) {
        let textures = render::library_textures(lib);
        renderer.set_fans(device, queue, &self.runs, self.fans_eye, &textures);
        for s in self.sprites.slots.iter_mut().flatten() {
            let Sprite::Boid(b) = s else { continue };
            let Some((centre, palette)) = &b.pose else {
                if let Some(g) = b.gpu.as_mut() {
                    g.visible = false;
                }
                continue;
            };
            if b.gpu.is_none() {
                let model = self.models.entry(b.base).or_insert_with(|| lib.build_model(b.base));
                let Some(model) = model.as_ref() else { continue };
                b.gpu = Some(renderer.upload(device, queue, model, &[centre.to_cols()], &textures));
            }
            let Some(g) = b.gpu.as_mut() else { continue };
            renderer.set_instances(queue, g, &[centre.to_cols()]);
            if !palette.is_empty() {
                renderer.set_palette(queue, g, palette);
            }
            // of an LOD body the level its distance picks (`VILODSprite::Raster`)
            crate::lod::place(g, centre, &self.eye);
            g.visible = true;
        }
    }

    /// Nothing of the volumes drawn (the options' Environment off): no fans,
    /// every boid hidden. They take up where they are when it is on again.
    pub fn hide(&mut self, renderer: &mut render::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, lib: &Library) {
        renderer.set_fans(device, queue, &[], self.fans_eye, &render::library_textures(lib));
        for s in self.sprites.slots.iter_mut().flatten() {
            if let Sprite::Boid(b) = s
                && let Some(g) = b.gpu.as_mut()
            {
                g.visible = false;
            }
        }
    }

    /// The boids' models, for `Renderer::draw` (those not drawn this frame
    /// are not visible).
    pub fn models(&self) -> impl Iterator<Item = &render::GpuModel> {
        self.sprites.slots.iter().flatten().filter_map(|s| match s {
            Sprite::Boid(b) => b.gpu.as_ref(),
            Sprite::Base(_) => None,
        })
    }

    /// Where each boid drawn this frame is (its centre transform's place),
    /// and the length of its skin palette.
    #[cfg(test)]
    fn boids(&self) -> Vec<([f32; 3], usize)> {
        self.sprites
            .slots
            .iter()
            .flatten()
            .filter_map(|s| match s {
                Sprite::Boid(b) => b.pose.as_ref().map(|(m, p)| ([m.m[3].x.0, m.m[3].y.0, m.m[3].z.0], p.len())),
                Sprite::Base(_) => None,
            })
            .collect()
    }

    /// The raster's normal as the volumes leave it (`VIRaster::Normal`):
    /// (0, 1, 0) once a light shaft or a falling leaf has been drawn, zero
    /// before; what an immediate-mode strip that sets none is lit by
    /// (`spells::light_bolts`).
    pub fn raster_normal(&self) -> Vec3 {
        let n = self.draw.normal;
        Vec3::new(n.x.0, n.y.0, n.z.0)
    }

    pub fn describe(&self) -> String {
        let s = self.stats;
        format!(
            "effect volumes: {} in {} zones, {} rastered, {} fans in {} runs, {} boids ({} materials, {} sprites held)",
            s.volumes,
            self.zones.len(),
            s.rastered,
            s.fans,
            self.runs.len(),
            s.boids,
            self.materials.len(),
            self.sprites.len()
        )
    }
}

/// Whether the client draws the effect volumes: not under `EQOA_FX=0` (in a
/// page `?nofx`), for comparing.
pub fn enabled() -> bool {
    #[cfg(target_arch = "wasm32")]
    return !crate::web::page_flag("nofx");
    #[cfg(not(target_arch = "wasm32"))]
    return std::env::var("EQOA_FX").map_or(true, |v| v != "0");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record's header, then its body.
    fn rec(ty: u16, version: i16, n_sub: u32, body: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend(ty.to_le_bytes());
        v.extend(version.to_le_bytes());
        v.extend((body.len() as i32).to_le_bytes());
        v.extend(n_sub.to_le_bytes());
        v.extend(body);
        v
    }

    fn words(w: &[u32]) -> Vec<u8> {
        w.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// An effect volume record: the header (id, box), an art record, then
    /// the parameters at `version`.
    fn volume(art: bool, params: Option<i16>) -> Vec<u8> {
        let f = |x: f32| x.to_bits();
        let mut body = rec(0xC310, 0, 0, &words(&[0x1234, f(-5.0), f(-1.0), f(-6.0), f(5.0), f(1.0), f(6.0)]));
        let mut n = 1;
        if art {
            body.extend(rec(0xC320, 0, 0, &[]));
            n += 1;
        }
        if let Some(v) = params {
            let mut p = words(&[3, f(18.0), f(7.75), f(100.0), f(0.3), f(0.15), 0x68f646e0, 0]);
            if v >= 2 {
                p.extend(words(&[2]));
            }
            body.extend(rec(0xC330, v, 0, &p));
            n += 1;
        }
        rec(RECORD, 0, n, &body)
    }

    /// `ParseEffectVolumeSpriteObj`'s reading: the header, an optional
    /// 0xC320, the parameters in the order the parser reads them (type,
    /// hours, distance, strength, density, surface, sprite), the flags only
    /// from version 2; without 0xC330, no parameters. The control: a reader
    /// taking the header for the parameters fails.
    #[test]
    fn a_record_reads_as_the_parser_reads_it() {
        let want = Params { ty: 3, p34: 18.0, p38: 7.75, p30: 100.0, p28: 0.3, density: 0.15, surface: 0x68f646e0, sprite: 0, flags: 2 };
        let read = |b: Vec<u8>| {
            let c = Container::parse_records(b);
            Record::read(&c, c.roots[0]).unwrap()
        };
        let r = read(volume(true, Some(2)));
        assert_eq!((r.id, r.bbox), (0x1234, [-5.0, -1.0, -6.0, 5.0, 1.0, 6.0]));
        assert_eq!(r.params, Some(want));
        assert_eq!(read(volume(false, Some(2))).params, Some(want), "no art record");
        assert_eq!(read(volume(true, Some(1))).params, Some(Params { flags: 0, ..want }), "flags from version 2");
        assert_eq!(read(volume(true, None)).params, None);
        // control: the parameters read off the record after the header,
        // whatever its type, take the art record for them
        let c = Container::parse_records(volume(true, Some(2)));
        let second = c.node(c.roots[0]).children[1];
        assert_ne!(c.node(second).ty, 0xC330, "the control must read the wrong record");
    }

    /// A fan's front face in the world: the volume's space through the
    /// actor's transform (turned, scaled, out at Freeport's x), relative to
    /// the eye, the normal turned and unit. The control: the transform
    /// without its turn puts the corners elsewhere.
    #[test]
    fn a_fan_goes_into_the_world_relative_to_the_eye() {
        let f = F::<Ee>::new;
        let v = |x: f32, y: f32, z: f32| Vect3::<Ee>::of(x, y, z);
        let uv = |u: f32, w: f32| [f(u), f(w)];
        let fan = vi::effect::Fan {
            material: 0,
            color: vi::particle::Color { r: f(0.0), g: f(0.0), b: f(0.0), a: f(0.25) },
            normal: v(0.0, 1.0, 0.0),
            verts: [(uv(0.0, 0.0), v(1.0, 0.0, 0.0)), (uv(1.0, 0.0), v(0.0, 0.0, 1.0)), (uv(1.0, 1.0), v(0.0, 1.0, 0.0)), (uv(0.0, 1.0), v(1.0, 1.0, 1.0)), (uv(1.0, 1.0), v(0.0, 1.0, 0.0)), (uv(1.0, 0.0), v(0.0, 0.0, 1.0))],
        };
        // a quarter turn (heading pi/2: x goes to -z, z to x), scale 2, at
        // (25300, 10, 8000)
        let m = Matrix44::<Ee>::transform(&v(std::f32::consts::FRAC_PI_2, 0.0, 0.0), f(2.0), &v(25300.0, 10.0, 8000.0));
        let eye = glam::DVec3::new(25290.0, 12.0, 8003.0);
        let q = quad(&fan, &m, eye);
        let want = [[10.0, -2.0, -5.0], [12.0, -2.0, -3.0], [10.0, 0.0, -3.0], [12.0, 0.0, -5.0]];
        for (k, (c, w)) in q.corners.iter().zip(want).enumerate() {
            assert!((0..3).all(|i| (c[i] - w[i]).abs() < 1e-3), "corner {k}: {c:?}, want {w:?}");
        }
        assert_eq!(q.uvs, [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        assert_eq!(q.color, [0.0, 0.0, 0.0, 0.25]);
        assert!((glam::Vec3::from(q.normal) - glam::Vec3::Y).length() < 1e-6);
        // control
        let flat = Matrix44::<Ee>::transform(&v(0.0, 0.0, 0.0), f(2.0), &v(25300.0, 10.0, 8000.0));
        let wrong = quad(&fan, &flat, eye);
        assert!((0..3).any(|i| (wrong.corners[0][i] - want[0][i]).abs() > 1.0), "the control must fail");
    }

    /// `RasterActor`'s tests: in view and near, drawn; hidden, a proxy,
    /// at the off distance or wholly outside a plane, not; a bound that
    /// reaches into the view counts. The control: a test dropped passes
    /// one of them.
    #[test]
    fn what_the_scene_rasters() {
        let eye = Vect3::<Ee>::of(0.0, 0.0, 0.0);
        let proj = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 1.0, 0.3, 6000.0);
        let planes = crate::particles::frustum(proj * glam::camera::rh::view::look_to_mat4(Vec3::ZERO, Vec3::Z, Vec3::Y));
        let at = |z: f32| (Vect3::<Ee>::of(0.0, 0.0, z), (Vec3::new(0.0, 0.0, z), 5.0));
        let ((a, b), (far, fb), (behind, bb)) = (at(20.0), at(500.0), at(-20.0));
        assert!(rastered(8, &a, b, &eye, &planes), "in front");
        assert!(!rastered(8 | 1, &a, b, &eye, &planes) && !rastered(4, &a, b, &eye, &planes), "hidden, a proxy");
        assert!(!rastered(8, &far, fb, &eye, &planes), "at the off distance");
        assert!(rastered(8, &Vect3::of(0.0, 0.0, 499.9), (Vec3::new(0.0, 0.0, 499.9), 5.0), &eye, &planes), "inside it");
        assert!(!rastered(8, &behind, bb, &eye, &planes), "behind");
        assert!(rastered(8, &behind, (bb.0, 25.0), &eye, &planes), "a bound reaching into the view");
        // control: without the planes, behind would draw
        let no_planes = |f: u32, p: &Vect3<Ee>| f & 5 == 0 && crate::lod::within(p, &eye, OFF_DIST);
        assert_ne!(no_planes(8, &behind), rastered(8, &behind, bb, &eye, &planes));
    }

    /// The camera as `VICamera::Update` makes it: the view takes the eye to
    /// the origin and the direction to +z (left-handed: right is forward x
    /// up), the world is its inverse, `dir` the unit direction. The control:
    /// the world's rows read off the view's rows, not its columns, fail.
    #[test]
    fn the_camera_is_vicameras() {
        let (p, d) = (Vect3::<Ee>::of(25087.0, 3.0, 8775.0), Vect3::<Ee>::of(0.3, -0.2, 1.0));
        let c = camera(p, d);
        let o = c.view.mul_point(&p);
        assert!(o.magnitude().0 < 0.01, "{o:?}");
        let ahead = Vect3::<Ee>::of(p.x.0 + 0.3, p.y.0 - 0.2, p.z.0 + 1.0);
        let a = c.view.mul_point(&ahead);
        assert!(a.x.0.abs() < 0.01 && a.y.0.abs() < 0.01 && a.z.0 > 1.0, "{a:?}");
        let i = Matrix44::mul(&c.world, &c.view);
        for (r, row) in i.m.iter().enumerate() {
            let e = |k: usize| if k == r { 1.0 } else { 0.0 };
            let v = [row.x.0, row.y.0, row.z.0, row.w.0];
            assert!((0..3).all(|k| (v[k] - e(k)).abs() < 1e-2), "world x view row {r}: {v:?}");
        }
        assert!((c.dir.magnitude().0 - 1.0).abs() < 1e-6);
        assert!((c.world.m[2].x.0 - c.dir.x.0).abs() < 1e-6, "forward is the world's third row");
        // right = forward x up: looking along +z, right is -x
        let r = camera(p, Vect3::of(0.0, 0.0, 1.0)).world.m[0];
        assert_eq!([r.x.0, r.y.0, r.z.0], [-1.0, 0.0, 0.0]);
        // control: rows for columns
        let wrong = c.view.m[0];
        assert!((wrong.y.0 - c.world.m[0].y.0).abs() > 1e-3 || (wrong.z.0 - c.world.m[0].z.0).abs() > 1e-3, "the control must fail");
    }

    /// The disc's TUNARIA.ESF round Neriak, with what a walk there needs:
    /// the zones round (x, z) read and the collision world made.
    pub(super) fn neriak(x: f32, z: f32) -> Option<(Library, World<Ee>)> {
        world_at("TUNARIA.ESF", x, z)
    }

    /// A world off the disc with the zones round (x, z) read and its
    /// collision world made.
    pub(super) fn world_at(file: &str, x: f32, z: f32) -> Option<(Library, World<Ee>)> {
        let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso_path.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        }
        let mut lib = Library::new();
        let w = lib.add(file, crate::stream::open_file(&iso_path, file).unwrap());
        let mut src = crate::stream::FileZones::open(&iso_path, file).unwrap();
        crate::stream::fill(&mut lib, w, x, z, &mut src);
        let world = World::load(&lib, w).unwrap();
        Some((lib, world))
    }

    /// Frames at `hz` from 0 to `ms` for a camera at `eye` looking along
    /// `dir`, at `hour`; `each` sees the volumes after each.
    #[allow(clippy::too_many_arguments)]
    fn run_with(fx: &mut Fx, lib: &Library, world: &World<Ee>, eye: Vec3, dir: Vec3, hour: f32, hz: i64, ms: i64, mut each: impl FnMut(&Fx)) {
        let proj = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 16.0 / 9.0, 0.3, 6000.0);
        let view = glam::camera::rh::view::look_to_mat4(eye, dir, Vec3::Y);
        let cam = camera(Vect3::of(eye.x, eye.y, eye.z), Vect3::of(dir.x, dir.y, dir.z));
        fx.stream(eye, world, lib);
        let mut t = 0;
        while t <= ms {
            fx.frame(world, lib, &cam, proj * view, eye, t, hour);
            each(fx);
            t += 1000 / hz;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run(fx: &mut Fx, lib: &Library, world: &World<Ee>, eye: Vec3, dir: Vec3, hour: f32, hz: i64, ms: i64) {
        run_with(fx, lib, world, eye, dir, hour, hz, ms, |_| {});
    }

    /// The animations the boids' sprites are playing (`m_cur[0].m_anim`).
    fn boid_anims(fx: &Fx) -> Vec<i32> {
        fx.sprites.slots.iter().flatten().filter_map(|s| match s {
            Sprite::Boid(b) => b.cs.as_ref().map(|cs| cs.cur[0].anim),
            Sprite::Base(_) => None,
        }).collect()
    }

    /// Neriak's plaza (dab's F3 of 2026-10-21): its ground fog, always on,
    /// is drawn round the camera: blended (mode 3) puffs of its two
    /// textures, each corner within reach of a volume the scene rastered.
    /// The controls: the same frames from 600 off (past every volume's
    /// fade) draw none, and a zone let go takes its volumes' materials.
    #[test]
    fn neriak_fog_draws_round_the_plaza() {
        let Some((lib, world)) = neriak(25087.0, 8786.0) else { return };
        let mut fx = Fx::new();
        let eye = Vec3::new(25087.0, 3.0, 8775.0);
        run(&mut fx, &lib, &world, eye, Vec3::new(0.0, -0.2, 1.0), 12.0, 30, 3000);
        eprintln!("{}", fx.describe());
        assert!(fx.stats.fans > 50, "{}", fx.describe());
        let textures: BTreeSet<i32> = fx.runs.iter().map(|r| r.texture).collect();
        assert!(textures.is_subset(&[0x68f646e0, 0x544fd983].into()), "{textures:x?}");
        assert!(fx.runs.iter().all(|r| r.blend == 3 && !r.zwrite));
        // each corner near a volume within its seen distance
        let near: Vec<(Vec3, f32)> = fx.zones.values().flatten().filter(|i| (i.bound.0 - eye).length() < 140.0).map(|i| i.bound).collect();
        for q in fx.runs.iter().flat_map(|r| &r.quads) {
            for c in q.corners {
                let p = Vec3::from(c) + eye;
                assert!(near.iter().any(|(at, r)| (p - *at).length() < r + 12.0), "a corner at {p} near no volume");
            }
        }
        // control: from 600 off, nothing
        let mut far = Fx::new();
        run(&mut far, &lib, &world, eye + Vec3::new(600.0, 0.0, 0.0), Vec3::new(0.0, -0.2, 1.0), 12.0, 30, 1000);
        assert_eq!(far.stats.fans, 0, "{}", far.describe());
        // a zone let go takes its volumes' materials with it
        assert!(fx.materials.len() > 0);
        fx.stream(eye + Vec3::new(5000.0, 0.0, 5000.0), &world, &lib);
        assert_eq!(fx.zones.len(), 0);
        assert_eq!(fx.materials.len(), 0, "materials held past their zone");
    }

    /// The hours a volume shows: the fog over Neriak's hill (18:00 to 07:45)
    /// is drawn at 22:00 and not at noon.
    #[test]
    fn night_fog_shows_at_night() {
        let Some((lib, world)) = neriak(25200.0, 8571.0) else { return };
        let fans = |hour: f32| {
            let mut fx = Fx::new();
            run(&mut fx, &lib, &world, Vec3::new(25200.0, 58.0, 8560.0), Vec3::new(0.0, -0.3, 1.0), hour, 30, 3000);
            fx.runs.iter().filter(|r| r.texture == 0x68f646e0).map(|r| r.quads.len()).sum::<usize>()
        };
        let (night, noon) = (fans(22.0), fans(12.0));
        eprintln!("night fog puffs: {night} at 22:00, {noon} at noon");
        assert!(night > 20 && noon == 0);
    }

    /// The flock over Neriak's hill: its five boids drawn as copies of its
    /// CSprite, flapping and gliding as the flock tells them (`SetLocomotion`
    /// 1 and 0), posed and skinned (a palette each), in its box; stepped at
    /// 165 frames a second as at 30 (the volume steps on the clock, at most
    /// 60 times a second). The control: from 200 off, past its fade, none.
    #[test]
    fn the_flock_flies_as_boids() {
        let Some((lib, world)) = neriak(25065.0, 8928.0) else { return };
        let at = Vec3::new(25065.162, 76.06539, 8928.152);
        for hz in [30, 165] {
            let mut fx = Fx::new();
            let mut seen = BTreeSet::new();
            run_with(&mut fx, &lib, &world, at + Vec3::new(0.0, 2.0, -30.0), Vec3::new(0.0, 0.0, 1.0), 12.0, hz, 4000, |fx| seen.extend(boid_anims(fx)));
            eprintln!("{hz} Hz: the boids played {seen:?}");
            // ProcessFlock's SetLocomotion: 1 diving, 0 climbing (control:
            // a sprite never told stays at its idle, 0)
            assert!(seen.contains(&1) && seen.contains(&0), "{seen:?}");
            let boids = fx.boids();
            eprintln!("{hz} Hz: {}; boids {boids:?}", fx.describe());
            assert_eq!(boids.len(), 5);
            for (p, palette) in boids {
                assert!(palette > 0, "a skinned body");
                let d = Vec3::from(p) - at;
                assert!(d.x.abs() < 30.0 && d.y.abs() < 12.0 && d.z.abs() < 30.0, "a boid at {p:?}");
            }
        }
        let mut far = Fx::new();
        run(&mut far, &lib, &world, at + Vec3::new(0.0, 2.0, -200.0), Vec3::new(0.0, 0.0, 1.0), 12.0, 30, 2000);
        assert_eq!(far.boids().len(), 0);
    }
}

#[cfg(test)]
mod bench {
    use super::*;

    /// `cargo test --release -p eqoa fx::bench -- --ignored --nocapture`:
    /// the volumes' CPU a frame where they cost most (a dust storm, 1,300
    /// fans) and in Neriak's plaza, the best of five runs of 200 frames.
    #[test]
    #[ignore]
    fn volumes_cost() {
        for (x, y, z, dir) in [(19150.0, 57.0, 4640.0, Vec3::new(0.3, -0.3, 1.0)), (25087.0, 3.0, 8775.0, Vec3::new(0.0, -0.2, 1.0))] {
            let Some((lib, world)) = tests::neriak(x, z) else { return };
            let eye = Vec3::new(x, y, z);
            let proj = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 16.0 / 9.0, 0.3, 6000.0);
            let view = glam::camera::rh::view::look_to_mat4(eye, dir, Vec3::Y);
            let cam = camera(Vect3::of(eye.x, eye.y, eye.z), Vect3::of(dir.x, dir.y, dir.z));
            let mut fx = Fx::new();
            fx.stream(eye, &world, &lib);
            let mut t = 0;
            for _ in 0..200 {
                fx.frame(&world, &lib, &cam, proj * view, eye, t, 12.0);
                t += 16;
            }
            let mut best = f64::MAX;
            for _ in 0..5 {
                let start = std::time::Instant::now();
                for _ in 0..200 {
                    fx.frame(&world, &lib, &cam, proj * view, eye, t, 12.0);
                    t += 16;
                }
                best = best.min(start.elapsed().as_secs_f64() * 1000.0 / 200.0);
            }
            eprintln!("at ({x}, {z}): {:.3} ms a frame; {}", best, fx.describe());
        }
    }

    /// Sutherland-Hodgman: the part of a convex polygon where `d >= 0`.
    fn clip<P: Copy>(poly: &[P], d: impl Fn(&P) -> f32, lerp: impl Fn(&P, &P, f32) -> P) -> Vec<P> {
        let mut out = Vec::with_capacity(poly.len() + 2);
        for (i, a) in poly.iter().enumerate() {
            let b = &poly[(i + 1) % poly.len()];
            let (da, db) = (d(a), d(b));
            if da >= 0.0 {
                out.push(*a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                out.push(lerp(a, b, da / (da - db)));
            }
        }
        out
    }

    /// What fans put on a `w` x `h` screen through `rel` (the view-projection
    /// with the eye at the origin, as `draw_fans` draws them), the ground
    /// (`ground` below the eye) hiding what is under it: the fragments (each
    /// quad's area, its part over the ground, on the screen), and how many
    /// quads cover each sample of a grid every `step` pixels.
    pub(super) fn cover<'a>(quads: impl Iterator<Item = &'a Quad>, rel: Mat4, ground: f32, (w, h): (f32, f32), step: usize) -> (f64, Vec<u16>) {
        let (gw, gh) = ((w as usize).div_ceil(step), (h as usize).div_ceil(step));
        let mut depth = vec![0u16; gw * gh];
        let mut fragments = 0.0;
        for q in quads {
            let poly: Vec<Vec3> = q.corners.iter().map(|&c| Vec3::from(c)).collect();
            let poly = clip(&poly, |p| p.y - ground, |a, b, t| a.lerp(*b, t));
            let poly: Vec<glam::Vec4> = poly.iter().map(|p| rel * p.extend(1.0)).collect();
            // DirectX depth: the near plane is z = 0
            let poly = clip(&poly, |c| c.z, |a, b, t| a.lerp(*b, t));
            let mut poly: Vec<glam::Vec2> = poly.iter().map(|c| glam::Vec2::new((c.x / c.w * 0.5 + 0.5) * w, (0.5 - c.y / c.w * 0.5) * h)).collect();
            for (axis, lo, hi) in [(0, 0.0, w), (1, 0.0, h)] {
                poly = clip(&poly, |p| p[axis] - lo, |a, b, t| a.lerp(*b, t));
                poly = clip(&poly, |p| hi - p[axis], |a, b, t| a.lerp(*b, t));
            }
            if poly.len() < 3 {
                continue;
            }
            let n = poly.len();
            let twice: f32 = (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum();
            fragments += (twice * 0.5).abs() as f64;
            let sign = twice.signum();
            let lo = poly.iter().fold(glam::Vec2::splat(f32::MAX), |a, p| a.min(*p));
            let hi = poly.iter().fold(glam::Vec2::splat(f32::MIN), |a, p| a.max(*p));
            let (x0, x1) = ((lo.x as usize) / step, ((hi.x as usize) / step).min(gw - 1));
            let (y0, y1) = ((lo.y as usize) / step, ((hi.y as usize) / step).min(gh - 1));
            for gy in y0..=y1 {
                for gx in x0..=x1 {
                    let s = glam::Vec2::new((gx * step) as f32 + step as f32 * 0.5, (gy * step) as f32 + step as f32 * 0.5);
                    if (0..n).all(|i| (poly[(i + 1) % n] - poly[i]).perp_dot(s - poly[i]) * sign >= 0.0) {
                        depth[gy * gw + gx] += 1;
                    }
                }
            }
        }
        (fragments, depth)
    }

    /// The cover's own check: one quad filling the view's lower half (the
    /// ground hiding none of it), seen from in front, is half the screen's
    /// fragments and one deep on the samples there; behind the eye, none.
    /// The control: the same quad under the ground puts nothing on it.
    #[test]
    fn the_cover_counts_what_is_seen() {
        let proj = glam::camera::rh::proj::directx::perspective(90f32.to_radians(), 1.0, 0.3, 1000.0);
        let rel = proj * glam::camera::rh::view::look_to_mat4(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y);
        let at = |z: f32, y0: f32, y1: f32| Quad { corners: [[-z.abs(), y1, z], [z.abs(), y1, z], [z.abs(), y0, z], [-z.abs(), y0, z]], ..Default::default() };
        let (f, d) = cover([at(-10.0, -10.0, 0.0)].iter(), rel, -100.0, (100.0, 100.0), 10);
        assert!((f - 5000.0).abs() < 1.0, "{f}");
        assert_eq!(d.iter().filter(|&&k| k == 1).count(), 50);
        let (f, _) = cover([at(10.0, -10.0, 0.0)].iter(), rel, -100.0, (100.0, 100.0), 10);
        assert_eq!(f, 0.0, "behind the eye");
        let (f, d) = cover([at(-10.0, -10.0, 0.0)].iter(), rel, 0.5, (100.0, 100.0), 10);
        assert_eq!((f, d.iter().map(|&k| k as u32).sum::<u32>()), (0.0, 0), "the control: under the ground");
    }

    /// `cargo test --release -p eqoa fx::bench::fans_cover -- --ignored
    /// --nocapture` (`EQOA_FX_SPOT=<world>,<x>,<y>,<z>,<hour>`, else the
    /// two F3 spots of 2026-10-26 with 298 fans, Klick'Anon's and Odus'):
    /// the volumes drawn round a player standing there, and what their fans
    /// put on a 1000x1000 screen for a camera close behind (3.3 off at 15
    /// degrees, the Klick'Anon report's) looking each of eight ways, the
    /// ground hiding what is under it: the fragments, as screens, and the
    /// deepest sample. Then the fans pass on this machine's GPU over that
    /// screen, the ground in its depth, against a whole screen of one
    /// blended layer.
    #[test]
    #[ignore]
    fn fans_cover() {
        let spots: Vec<(String, [f32; 3], f32)> = match std::env::var("EQOA_FX_SPOT") {
            Ok(s) => {
                let v: Vec<&str> = s.split(',').collect();
                let n = |i: usize| v[i].trim().parse::<f32>().unwrap();
                vec![(v[0].to_string(), [n(1), n(2), n(3)], n(4))]
            }
            Err(_) => vec![("TUNARIA.ESF".into(), [23331.03, 54.13, 6503.17], 0.04), ("ODUS.ESF".into(), [8927.07, 48.07, 7123.74], 12.73)],
        };
        let (w, h) = (1000.0, 1000.0);
        for (file, at, hour) in spots {
            let Some((lib, world)) = tests::world_at(&file, at[0], at[2]) else { return };
            let spot = Vec3::from(at);
            let mut worst: Option<(f64, Vec<Run>, Mat4, Vec3)> = None;
            for k in 0..8 {
                let yaw = k as f32 * std::f32::consts::FRAC_PI_4;
                let fwd = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                let (pitch, dist) = (0.2618f32, 3.33);
                let target = spot + Vec3::Y * 1.5;
                let eye = target - fwd * dist * pitch.cos() + Vec3::Y * dist * pitch.sin();
                let dir = (target - eye).normalize();
                let proj = crate::walker::projection_to(w / h, 3000.0);
                let view = glam::camera::rh::view::look_to_mat4(eye, dir, Vec3::Y);
                let cam = camera(Vect3::of(eye.x, eye.y, eye.z), Vect3::of(dir.x, dir.y, dir.z));
                let mut fx = Fx::new();
                fx.stream(eye, &world, &lib);
                let mut t = 0;
                // three seconds at 60 a second: every puff alive
                while t < 3000 {
                    fx.frame(&world, &lib, &cam, proj * view, eye, t, hour);
                    t += 16;
                }
                if k == 0 {
                    eprintln!("{file} at {at:?}, hour {hour}: {}", fx.describe());
                    let (mut host, zones, cards, rand, draw) = fx.scene(&lib, F::new(hour));
                    let planes = crate::particles::frustum(proj * view);
                    for inst in zones.values_mut().flatten() {
                        let actor = &world.actors[inst.actor];
                        let d = (inst.bound.0 - eye).length();
                        if d > 160.0 || !rastered(actor.flags, &actor.location, inst.bound, &cam.position, &planes) {
                            continue;
                        }
                        draw.fans.clear();
                        draw.sprites.clear();
                        inst.vs.raster(&cam, &Matrix44::mul(&inst.m, &cam.view), t, cards, rand, &mut host, draw);
                        let v = &inst.vs.volume;
                        let s = [v.bbox.max.x.0 - v.bbox.min.x.0, v.bbox.max.y.0 - v.bbox.min.y.0, v.bbox.max.z.0 - v.bbox.min.z.0];
                        eprintln!(
                            "  {:6} {:4.1}x{:4.1}x{:4.1} density {:.2} seen {:5.1} hours {:5.2}-{:5.2}: {:5.1} off, fade {:.2}, hours {:.2}, {} fans",
                            type_name(v.ty), s[0], s[1], s[2], v.density.0, v.p30.0, v.p34.0, v.p38.0, d, v.fade.0, v.day_night.0, draw.fans.len()
                        );
                    }
                }
                let rel = (proj * view) * Mat4::from_translation(eye);
                let ground = spot.y - eye.y;
                let (fragments, depth) = cover(fx.runs.iter().flat_map(|r| &r.quads), rel, ground, (w, h), 4);
                let screens = fragments / (w * h) as f64;
                let deep = depth.iter().copied().max().unwrap_or(0);
                let mut sorted = depth.clone();
                sorted.sort_unstable();
                let p = |q: f64| sorted[((sorted.len() - 1) as f64 * q) as usize];
                eprintln!("  looking {:3.0} deg: {} fans, {:.2} screens of fragments; samples covered {:.0}%, median {}, 90% {}, deepest {}", yaw.to_degrees(), fx.stats.fans, screens, 100.0 * depth.iter().filter(|&&d| d > 0).count() as f64 / depth.len() as f64, p(0.5), p(0.9), deep);
                if worst.as_ref().is_none_or(|w| fragments > w.0) {
                    worst = Some((fragments, fx.runs.clone(), proj * view, eye));
                }
            }
            if let Some((fragments, runs, vp, eye)) = worst {
                gpu_time(&lib, &runs, vp, eye, spot.y - eye.y, fragments, (w as u32, h as u32));
            }
        }
    }

    /// The fans pass timed on this machine's GPU: `runs` drawn over a
    /// `size` target whose depth holds the ground (`ground` below the eye),
    /// 200 passes in one submission, against 200 passes of one whole-screen
    /// blended quad.
    fn gpu_time(lib: &Library, runs: &[Run], view_proj: Mat4, eye: Vec3, ground: f32, fragments: f64, (w, h): (u32, u32)) {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&Default::default())) else {
            eprintln!("SKIP the GPU: no adapter");
            return;
        };
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut renderer = render::Renderer::new(&device, &queue, render::TARGET_FORMAT);
        let tex = |format, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let target = tex(render::TARGET_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let depth = tex(render::DEPTH_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT);
        // the ground into the depth: a plane under the eye
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(
                "@group(0) @binding(0) var<uniform> m: mat4x4<f32>;\n@vertex fn vs(@location(0) p: vec3<f32>) -> @builtin(position) vec4<f32> { return m * vec4<f32>(p, 1.0); }\n@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(0.1, 0.1, 0.1, 1.0); }".into(),
            ),
        });
        let rel = view_proj * Mat4::from_translation(eye);
        use wgpu::util::DeviceExt;
        let bytes = |f: &[f32]| f.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let ubuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: &bytes(&rel.to_cols_array()), usage: wgpu::BufferUsages::UNIFORM });
        let r = 4000.0f32;
        let plane: Vec<f32> = [[-r, -r], [r, -r], [r, r], [-r, -r], [r, r], [-r, r]].iter().flat_map(|[x, z]| [*x, ground, *z]).collect();
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: &bytes(&plane), usage: wgpu::BufferUsages::VERTEX });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: 12, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![0 => Float32x3] })],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: render::DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: render::TARGET_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() }],
        });
        let view = render::View { eye, fog: Some((20.0, 200.0, Vec3::splat(0.2))), ..render::View::daylight(view_proj) };
        let textures = render::library_textures(lib);
        let ground_pass = |enc: &mut wgpu::CommandEncoder| {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.set_vertex_buffer(0, vbuf.slice(..));
            pass.draw(0..6, 0..1);
        };
        let time = |renderer: &mut render::Renderer, runs: &[Run]| {
            renderer.set_fans(&device, &queue, runs, eye, &textures);
            let mut best = f64::MAX;
            for _ in 0..4 {
                let mut enc = device.create_command_encoder(&Default::default());
                ground_pass(&mut enc);
                queue.submit([enc.finish()]);
                device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).unwrap();
                let mut enc = device.create_command_encoder(&Default::default());
                for _ in 0..200 {
                    renderer.draw_fans(&queue, &mut enc, &target, &depth, &view);
                }
                let start = std::time::Instant::now();
                queue.submit([enc.finish()]);
                device.poll(wgpu::PollType::Wait { submission_index: None, timeout: None }).unwrap();
                best = best.min(start.elapsed().as_secs_f64() * 1000.0 / 200.0);
            }
            best
        };
        let fans = time(&mut renderer, runs);
        // one blended quad over the whole screen, just past the near plane
        let inv = rel.inverse();
        let corner = |x: f32, y: f32| {
            let p = inv * glam::Vec4::new(x, y, 0.01, 1.0);
            (p.truncate() / p.w).to_array()
        };
        let screen = Quad { corners: [corner(-1.0, 1.0), corner(1.0, 1.0), corner(1.0, -1.0), corner(-1.0, -1.0)], uvs: [[0.0; 2]; 4], color: [1.0, 1.0, 1.0, 0.5], normal: [0.0, 1.0, 0.0] };
        let one = time(&mut renderer, &[Run { texture: runs.first().map_or(0, |r| r.texture), blend: 3, zwrite: false, quads: vec![screen] }]);
        eprintln!(
            "  the GPU ({}): the fans pass {:.3} ms ({:.2} screens of fragments counted), one whole-screen blended layer {:.3} ms: the pass is {:.1} such layers",
            adapter.get_info().name,
            fans,
            fragments / (w * h) as f64,
            one,
            fans / one
        );
    }
}
