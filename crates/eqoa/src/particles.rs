//! The world's particles: every particle sprite placed in the world file
//! (`VIParticleSprite`, 0xC100) is an emitter of its own per placement (its
//! `GetInstType` is 1), placed and updated as `VIParticleSprite::Raster` does
//! each frame it is drawn (aimed along its actor's axes, or in its actor's
//! space when its flags say so; purged rather than updated past 100), and the
//! whole system rendered once a frame (`VIParticleSystem::Render`) into
//! billboards for `render::billboard`.
//!
//! "Drawn" is the camera's view: an emitter whose actor's sphere is outside
//! the frustum is not updated (its particles still live out their lives). The
//! game's own draw distances (`m_offDist`) are not applied.

use esf::{Library, Ref};
use glam::{Mat4, Vec3, Vec4};
use render::billboard::{Batch, Quad};
use std::collections::HashMap;
use vi::csprite::Rand;
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::particle::{Definition, Matrix33, System};
use vi::vect::Vect3;
use vi::world::{SpriteKind, World};

/// One placed emitter: its actor, its sprite, the sprite's transform within
/// the actor (a group member's), its definition record and flags.
struct Instance {
    actor: usize,
    member: Option<Matrix44<Ee>>,
    def: Ref,
    flags: i32,
    emitter: Option<usize>,
}

pub struct Particles {
    sys: System<Ee>,
    rand: Rand,
    defs: HashMap<Ref, usize>,
    instances: Vec<Instance>,
}

impl Particles {
    /// Every particle sprite placed in `world`, directly or as a group's
    /// member.
    pub fn new(world: &World<Ee>, time: i64) -> Particles {
        let mut p = Particles { sys: System::new(time, 2000), rand: Rand(1), defs: HashMap::new(), instances: Vec::new() };
        p.add_from(world, 0);
        p
    }

    /// The particle sprites placed by actors `first..`: a zone streamed in.
    pub fn add_from(&mut self, world: &World<Ee>, first: usize) {
        let instances = &mut self.instances;
        for (a, act) in world.actors.iter().enumerate().skip(first) {
            let Some(s) = act.sprite else { continue };
            match &world.sprites[s].kind {
                SpriteKind::Particle { def: Some(def), flags } => instances.push(Instance { actor: a, member: None, def: *def, flags: *flags, emitter: None }),
                SpriteKind::Group(members) => {
                    for &(ms, angles, scale, t) in members {
                        if let SpriteKind::Particle { def: Some(def), flags } = &world.sprites[ms].kind {
                            let m = Matrix44::transform(&Vect3::of(angles[0], angles[1], angles[2]), F::new(scale), &Vect3::of(t[0], t[1], t[2]));
                            instances.push(Instance { actor: a, member: Some(m), def: *def, flags: *flags, emitter: None });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    pub fn describe(&self) -> String {
        let live = self.instances.iter().filter(|i| i.emitter.is_some()).count();
        format!("{} particle emitters placed, {live} made", self.instances.len())
    }

    fn definition(&mut self, lib: &Library, r: Ref) -> Option<usize> {
        if let Some(&d) = self.defs.get(&r) {
            return Some(d);
        }
        let c = lib.file(r.file);
        let body = c.child(r.node, 0xC020)?;
        let d = Definition::<Ee>::parse(c.body(body), c.node(body).version as i32)?;
        let k = self.sys.add_definition(d);
        self.defs.insert(r, k);
        Some(k)
    }

    /// One frame at scene time `time` (ms), for a camera at `eye` seeing
    /// through `view_proj`: the emitters drawn are placed and updated, then
    /// everything is rendered. Returns each emitter's billboards with its
    /// material.
    pub fn frame(&mut self, world: &World<Ee>, lib: &Library, eye: Vec3, view_proj: Mat4, time: i64) -> Vec<Batch> {
        let planes = frustum(view_proj);
        for k in 0..self.instances.len() {
            let inst = &self.instances[k];
            let act = &world.actors[inst.actor];
            let c = match &inst.member {
                Some(m) => Matrix44::mul(m, &act.object_to_world),
                None => act.object_to_world,
            };
            let at = Vec3::new(c.m[3].x.0, c.m[3].y.0, c.m[3].z.0);
            let r = act.bound.r.0.max(1.0);
            let center = Vec3::new(act.bound.c.x.0, act.bound.c.y.0, act.bound.c.z.0);
            if !planes.iter().all(|p| p.dot(center.extend(1.0)) >= -r) {
                continue;
            }
            let (def, flags) = (inst.def, inst.flags);
            let emitter = match inst.emitter {
                Some(e) => e,
                None => {
                    let Some(d) = self.definition(lib, def) else { continue };
                    let e = self.sys.create_emitter(d);
                    self.instances[k].emitter = Some(e);
                    e
                }
            };
            let em = self.sys.emitter(emitter);
            if flags == 0 {
                em.set_location(Vect3::new(c.m[3].x, c.m[3].y, c.m[3].z));
                em.direction = Matrix33 { m: [c.m[0].xyz(), c.m[1].xyz(), c.m[2].xyz()] };
            } else {
                em.set_location(Vect3::default());
                em.set_direction_vec(&Vect3::of(0.0, 0.0, 1.0));
                em.space = c;
            }
            if (eye - at).length() > 100.0 {
                self.sys.purge(emitter);
            } else {
                self.sys.update(emitter, &mut self.rand);
            }
        }
        render(&mut self.sys, time)
    }
}

/// `VIParticleSystem::Render`: each emitter's billboards with its
/// definition's material.
pub fn render(sys: &mut System<Ee>, time: i64) -> Vec<Batch> {
    let mut out = Vec::new();
    for (id, bbs) in sys.render(time) {
        let def = sys.emitter(id).def;
        let Some(d) = sys.defs[def].as_ref() else { continue };
        out.push(Batch {
            texture: d.texture as i32,
            blend: d.blend.clamp(0, 4) as usize,
            ztest: d.ztest,
            zwrite: d.zwrite,
            quads: bbs
                .iter()
                .map(|b| Quad {
                    center: [b.location.x.0, b.location.y.0, b.location.z.0],
                    half: [b.width.0, b.height.0],
                    frame: b.frame,
                    color: [b.color.r.0, b.color.g.0, b.color.b.0, b.color.a.0],
                })
                .collect(),
        });
    }
    out
}

/// The six planes of a view-projection (`dot(plane, (p, 1)) >= 0` inside),
/// each scaled to unit normal length so a sphere's radius compares directly.
pub(crate) fn frustum(m: Mat4) -> [Vec4; 6] {
    let r = |i: usize| m.row(i);
    let mut p = [r(3) + r(0), r(3) - r(0), r(3) + r(1), r(3) - r(1), r(2), r(3) - r(2)];
    for q in p.iter_mut() {
        let n = q.truncate().length();
        if n > 0.0 {
            *q /= n;
        }
    }
    p
}

/// One particle sprite inside an item's sprite: its transform within the
/// item (a group member's `Transform(angles, scale, translation)`, the
/// members' composed when groups nest), its definition record and flags.
#[derive(Clone, Copy, Debug)]
pub struct ItemEmitter {
    pub member: Matrix44<Ee>,
    pub def: Ref,
    pub flags: i32,
}

/// The particle sprites (0xC100) an item's sprite carries as a group's
/// members, as `VIGroupSprite::Raster` rasters them: the glow of an epic
/// weapon (22 of ITEM.ESF's group sprites carry 27, beside the mesh and the
/// two point sprites a weapon's trail runs between). None for any other item.
pub fn item_emitters(lib: &Library, item: u32) -> Vec<ItemEmitter> {
    let mut out = Vec::new();
    if let Some(r) = lib.lookup(item as i32) {
        members(lib, r, &Matrix44::identity(), 0, &mut out);
    }
    out
}

fn members(lib: &Library, r: Ref, base: &Matrix44<Ee>, depth: u32, out: &mut Vec<ItemEmitter>) {
    let c = lib.file(r.file);
    match c.node(r.node).ty {
        esf::types::GROUPSPRITE if depth < 4 => {
            let Some(list) = c.child(r.node, esf::types::GROUPSPRITE_MEMBERS) else { return };
            let d = c.body(list);
            let f = |p: usize| d.get(p..p + 4).map_or(0.0, |b| f32::from_le_bytes(b.try_into().unwrap()));
            let n = d.get(..4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap())).clamp(0, 4096) as usize;
            for k in 0..n {
                let p = 4 + k * 32;
                let Some(id) = d.get(p..p + 4).map(|b| i32::from_le_bytes(b.try_into().unwrap())) else { break };
                let Some(m) = lib.lookup(id) else { continue };
                let mem = Matrix44::transform(&Vect3::of(f(p + 4), f(p + 8), f(p + 12)), F::new(f(p + 16)), &Vect3::of(f(p + 20), f(p + 24), f(p + 28)));
                members(lib, m, &Matrix44::mul(&mem, base), depth + 1, out);
            }
        }
        // ParseParticleSpriteObj: the definition nested in it and (from
        // version 1) the flags
        0xC100 => {
            let Some(def) = c.child(r.node, 0xC000) else { return };
            let flags = c.child(r.node, 0xC101).filter(|&h| c.node(h).version != 0).and_then(|h| c.body(h).get(4..8)).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()));
            out.push(ItemEmitter { member: *base, def: Ref { file: r.file, node: def }, flags });
        }
        _ => {}
    }
}

/// One of the emitters in creatures' hands this frame: which (the holder,
/// the hand, the item, its place among the item's), its world transform
/// (the member's under the hand's node under the creature's centre
/// transform), and whether its holder was drawn.
pub struct InHand {
    pub key: (i32, usize, u32, usize),
    pub world: Matrix44<Ee>,
    pub def: Ref,
    pub flags: i32,
    pub drawn: bool,
}

/// The particles of the items creatures hold, on a particle system of their
/// own. `AttachItem` attaches a COPY of an item whose sprite carries a
/// particle sprite (its `GetInstType` is 1), so every hand has its own
/// emitters; `DetachItem` releases the copy and its particles with it. Each
/// is placed and updated as `VIParticleSprite::Raster` does under the
/// attachment's model-view, while its holder is drawn.
pub struct Held {
    sys: System<Ee>,
    rand: Rand,
    defs: HashMap<Ref, usize>,
    emitters: HashMap<(i32, usize, u32, usize), usize>,
}

impl Held {
    pub fn new(time: i64) -> Held {
        Held { sys: System::new(time, 2000), rand: Rand(1), defs: HashMap::new(), emitters: HashMap::new() }
    }

    /// One frame at scene time `time`, the camera at `eye`, with `now` every
    /// emitter in a hand: one new is made, one gone is destroyed, each whose
    /// holder was drawn is placed and updated; then every particle rendered.
    pub fn frame(&mut self, lib: &Library, now: &[InHand], eye: &Vect3<Ee>, time: i64) -> Vec<Batch> {
        let gone: Vec<_> = self.emitters.keys().filter(|k| !now.iter().any(|h| h.key == **k)).copied().collect();
        for k in gone {
            if let Some(id) = self.emitters.remove(&k) {
                self.sys.destroy_emitter(id);
            }
        }
        for h in now {
            let id = match self.emitters.get(&h.key) {
                Some(&id) => id,
                None => {
                    let def = match self.defs.get(&h.def) {
                        Some(&d) => d,
                        None => {
                            let c = lib.file(h.def.file);
                            let Some(d) = c.child(h.def.node, 0xC020).and_then(|b| Definition::<Ee>::parse(c.body(b), c.node(b).version as i32)) else { continue };
                            let d = self.sys.add_definition(d);
                            self.defs.insert(h.def, d);
                            d
                        }
                    };
                    let id = self.sys.create_emitter(def);
                    self.emitters.insert(h.key, id);
                    id
                }
            };
            if h.drawn {
                raster_sprite(&mut self.sys, id, h.flags, &h.world, eye, &mut self.rand);
            }
        }
        render(&mut self.sys, time)
    }

    /// How many emitters are in hands, and their particles.
    pub fn describe(&self) -> String {
        let n: usize = self.emitters.values().map(|&id| self.sys.particle_count(id)).sum();
        format!("{} held-item emitters, {n} particles", self.emitters.len())
    }
}

/// `VIParticleSprite::Raster` for a particle sprite whose world transform is
/// `c` (the model-view with the camera's view taken off): a free emitter
/// (flags 0) placed at its translation and aimed along its axes, scale and
/// all; a local one pinned at the origin, aimed down +z, in `c`'s space.
/// Past 100 from the camera at `eye` it is purged rather than updated.
fn raster_sprite(sys: &mut System<Ee>, id: usize, flags: i32, c: &Matrix44<Ee>, eye: &Vect3<Ee>, rand: &mut Rand) {
    let em = sys.emitter(id);
    if flags == 0 {
        em.set_location(Vect3::new(c.m[3].x, c.m[3].y, c.m[3].z));
        em.direction = Matrix33 { m: [c.m[0].xyz(), c.m[1].xyz(), c.m[2].xyz()] };
    } else {
        em.set_location(Vect3::default());
        em.set_direction_vec(&Vect3::of(0.0, 0.0, 1.0));
        em.space = *c;
    }
    let p = Vect3::new(eye.x - c.m[3].x, eye.y - c.m[3].y, eye.z - c.m[3].z);
    if F::new(100.0) < p.magnitude() {
        sys.purge(id);
    } else {
        sys.update(id, rand);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ITEM.ESF off the disc, or None (the test SKIPs).
    fn items() -> Option<Library> {
        let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&iso_path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        };
        let mut lib = Library::new();
        lib.add("ITEM.ESF", esf::Container::open(disc.read("ITEM.ESF").ok()?).ok()?);
        Some(lib)
    }

    /// The particle sprites the disc's items carry: 27 in 22 group sprites
    /// (the Sword of Virtue's one, the Ancient Crystal Staff's two), each
    /// under its member transform; a weapon with only a mesh and the trail's
    /// two point sprites (Selo's Jeweled Rapier) carries none.
    #[test]
    fn the_items_particle_sprites_off_the_disc() {
        let Some(lib) = items() else { return };
        let virtue = item_emitters(&lib, 0x88EC092D);
        assert_eq!(virtue.len(), 1);
        let e = virtue[0];
        assert_eq!(e.flags, 1, "the virtue's glow is a local emitter");
        assert_eq!(lib.file(e.def.file).node(e.def.node).ty, 0xC000);
        let t = e.member.m[3];
        assert!((t.x.0 - 0.001).abs() < 0.001 && t.y.0.abs() < 0.001 && (t.z.0 + 0.456).abs() < 0.001, "at {:?}", (t.x.0, t.y.0, t.z.0));
        assert_eq!(item_emitters(&lib, 0x63DCEFAB).len(), 2);
        assert!(item_emitters(&lib, 0xF4DD991A).is_empty(), "Selo's Jeweled Rapier");
        assert!(item_emitters(&lib, 0x12345678).is_empty(), "no such item");
        let c = lib.file(0);
        let groups: Vec<u32> = c.models().iter().filter(|m| m.ty == esf::types::GROUPSPRITE).map(|m| m.dict_id as u32).collect();
        let with: Vec<usize> = groups.iter().map(|&g| item_emitters(&lib, g).len()).filter(|&n| n > 0).collect();
        assert_eq!((with.len(), with.iter().sum::<usize>()), (22, 27));
    }

    /// One emitter in a hand, run for `ms` at the console's frame: its
    /// billboards' centres. `world` is where it is held, `drawn` whether its
    /// holder is drawn, `eye` the camera.
    fn run(held: &mut Held, lib: &Library, h: &[InHand], eye: [f32; 3], from: i64, ms: i64) -> Vec<[f32; 3]> {
        let eye = Vect3::of(eye[0], eye[1], eye[2]);
        let mut out = Vec::new();
        let mut t = from;
        while t < from + ms {
            t += 33;
            out = held.frame(lib, h, &eye, t).iter().flat_map(|b| b.quads.iter().map(|q| q.center)).collect();
        }
        out
    }

    /// `VIParticleSprite::Raster` in a hand: a local emitter (the blade of
    /// Blood Iron Longblade, Nitefyre Scimitar and five more) streams up its
    /// item's +z in the item's space, so its particles follow the hand; not
    /// updated while its holder is not drawn, purged past 100 from the
    /// camera, and gone with the item.
    #[test]
    fn a_held_items_particles_follow_the_hand() {
        let Some(lib) = items() else { return };
        let e = item_emitters(&lib, 0x8A82B605)[0];
        let at = |x: f32, y: f32, z: f32| Matrix44::transform(&Vect3::of(0.0, 0.0, 0.0), F::ONE, &Vect3::of(x, y, z));
        let hand = |world, drawn| [InHand { key: (3, 0, 0x8A82B605, 0), world, def: e.def, flags: e.flags, drawn }];
        let mut held = Held::new(0);
        let near = [100.0, 52.0, 195.0];
        let pts = run(&mut held, &lib, &hand(at(100.0, 50.0, 200.0), true), near, 0, 1000);
        assert!(pts.len() > 5, "{} particles", pts.len());
        // up the blade (+z) from the grip, no farther than speed x life,
        // spread a little by the nozzle's variation
        assert!(pts.iter().all(|p| (p[0] - 100.0).abs() < 0.05 && (p[1] - 50.0).abs() < 0.05 && p[2] >= 200.0 && p[2] < 201.6), "{pts:?}");
        // the hand moves: the particles with it, in its space
        let moved = run(&mut held, &lib, &hand(at(110.0, 50.0, 200.0), true), near, 1000, 66);
        assert!(!moved.is_empty() && moved.iter().all(|p| (p[0] - 110.0).abs() < 0.05), "{moved:?}");
        // the item put away: its emitter and its particles gone
        assert!(run(&mut held, &lib, &[], near, 1066, 66).is_empty());
        assert_eq!(held.describe(), "0 held-item emitters, 0 particles");
        // the controls: a holder not drawn emits nothing, nor one past 100
        assert!(run(&mut Held::new(0), &lib, &hand(at(100.0, 50.0, 200.0), false), near, 0, 1000).is_empty());
        assert!(run(&mut Held::new(0), &lib, &hand(at(100.0, 50.0, 200.0), true), [100.0, 50.0, 320.0], 0, 1000).is_empty());
    }
}
