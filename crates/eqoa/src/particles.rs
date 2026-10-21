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
