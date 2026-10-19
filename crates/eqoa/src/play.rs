//! Playing on a server: the game session (crate `net`) pumped every frame
//! beside the walker, the player's state sent on channel 0x40 every
//! `gEntityUpdateRate` (100 ms; the library sends nothing when it has not
//! changed), and the entities on channels 0x00-0x17 drawn where
//! `VIClient::Extrapolate` puts them, each as its CSprite.
//!
//! Not yet: the creatures' own walk physics between updates
//! (`VICreatureEntity::Walk`/`ExtrapolateMove`/`ExtrapolateGround`), their
//! animation from it, items, armour and names.

use crate::walker::Walker;
use esf::Library;
use net::entity::{Entities, SLOTS};
use net::session::{Event, Player, Session};
use render::{GpuModel, Renderer, wgpu};
use vi::character::{AnimCache, load_csprite};
use vi::csprite::{CSprite, Rand};
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::hsprite::AnimScene;
use vi::vect::Vect3;
use vi::walk::{WalkControl, WalkPhysics};

/// `gEntityUpdateRate`.
pub const UPDATE_RATE: u32 = 100;

/// One drawn entity: its model as a CSprite, animated on its own clock.
struct Puppet {
    model: u32,
    look: Option<vi::cust::Look>,
    sprite: Option<CSprite<Ee>>,
    anims: AnimScene<Ee>,
    rand: Rand,
    gpu: GpuModel,
    /// Items and the helm, each on its node (`Attach`).
    attached: Vec<(i32, GpuModel)>,
}

pub struct Play {
    pub session: Session,
    pub player: Player,
    pub entities: Entities,
    last_send: u32,
    puppets: Vec<Option<Puppet>>,
    /// Each creature's `m_walk`, made with it (`CreateEntity`: `Init` at the
    /// origin), keyed by the entity's id.
    walks: Vec<Option<(u32, WalkControl<Ee>)>>,
    /// The player's model as the server last named it on its channel.
    pub self_model: Option<u32>,
    /// The player dressed as its own channel says (look, items, helm),
    /// placed with the walker's pose.
    self_puppet: Option<Puppet>,
}

/// A creature's model as `Apply` dresses it: the look on its text slots, the
/// items and helm on their nodes.
fn build_puppet(lib: &Library, g: &mut Gpu, e: &net::entity::Entity, look: Option<vi::cust::Look>, slot: usize, game_ms: i64) -> Option<Puppet> {
    let textures = render::library_textures(lib);
    lib.lookup(e.model as i32).and_then(|r| {
                    let mut model = lib.build_model(r)?;
                    let mut anims = AnimScene { time: game_ms, ..Default::default() };
                    let mut rand = Rand(slot as u64 + 1);
                    let sprite = load_csprite(lib, r, &mut anims, &mut AnimCache::new(), &mut rand);
                    // Attach: the items in hand (ApplyItems: the override if
                    // any) and the helm (SetHelm, at node 8, in the tint).
                    let mut attached = Vec::new();
                    let mut helm_worn = false;
                    if let (Some(cs), Some(c)) = (&sprite, &e.creature) {
                        for i in 0..3 {
                            let id = if c.items_over[i] != 0 { c.items_over[i] } else { c.items[i] };
                            let node = cs.item_nodes[i];
                            if id == 0 || node < 0 {
                                continue;
                            }
                            if let Some(m) = lib.lookup(id as i32).and_then(|r| lib.build_model(r)) {
                                attached.push((node, g.renderer.upload(g.device, g.queue, &m, &[render::IDENTITY], &textures)));
                            }
                        }
                        if cs.node_ids[8] >= 0
                            && let Some(h) = vi::cust::helm(c.armor[5])
                            && let Some(mut m) = lib.lookup(h as i32).and_then(|r| lib.build_model(r))
                        {
                            let tint = vi::cust::TINTS[(c.armor_tint[5] as usize).min(14)];
                            for b in &mut m.batches {
                                for l in &mut b.layers {
                                    l.color = tint;
                                }
                            }
                            helm_worn = true;
                            attached.push((cs.node_ids[8], g.renderer.upload(g.device, g.queue, &m, &[render::IDENTITY], &textures)));
                        }
                    }
                    // ApplyLook: the look's textures and tints on the layers the
                    // sprite's text slots name.
                    if let (Some(cs), Some(look)) = (&sprite, &look) {
                        dress(&mut model, &vi::cust::apply_look(cs, look, helm_worn, |id| lib.surface(id as i32).is_some()));
                    }
                    let gpu = g.renderer.upload(g.device, g.queue, &model, &[render::IDENTITY], &textures);
                    Some(Puppet { model: e.model, look, sprite, anims, rand, gpu, attached })
                })
}

/// Puts layer changes onto the batches of the palette entries they name.
fn dress(model: &mut esf::Model, changes: &[vi::cust::LayerChange]) {
    for ch in changes {
        for b in model.batches.iter_mut().filter(|b| b.material == ch.entry) {
            if let Some(l) = b.layers.get_mut(ch.layer as usize) {
                if let Some(t) = ch.texture {
                    l.texture = t as i32;
                }
                if let Some(c) = ch.color {
                    l.color = c;
                }
            }
        }
    }
}

/// A column-major matrix back into the row-vector `Matrix44`.
fn glam_to_matrix(m: [[f32; 4]; 4]) -> Matrix44<Ee> {
    let mut out = Matrix44::<Ee>::identity();
    for (r, row) in out.m.iter_mut().enumerate() {
        row.x = F::new(m[r][0]);
        row.y = F::new(m[r][1]);
        row.z = F::new(m[r][2]);
        row.w = F::new(m[r][3]);
    }
    out
}

fn look_of(c: &net::entity::Creature) -> vi::cust::Look {
    vi::cust::Look { armor: c.armor, armor_tint: c.armor_tint, hair: c.hair, face: c.face, robe: c.robe as i8, robe_tint: c.robe_tint }
}

fn v3(a: [f32; 3]) -> Vect3<Ee> {
    Vect3::of(a[0], a[1], a[2])
}

pub struct Gpu<'a> {
    pub renderer: &'a mut Renderer,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
}

impl Play {
    pub fn new(session: Session, player: Player) -> Self {
        Play { session, player, entities: Entities::default(), last_send: 0, puppets: (0..SLOTS).map(|_| None).collect(), walks: (0..SLOTS).map(|_| None).collect(), self_model: None, self_puppet: None }
    }

    fn self_id32(&self) -> Option<u32> {
        self.session.self_id.map(|s| s as u32)
    }

    /// One frame. Returns the player's model when the server has just named
    /// a different one (the caller re-dresses the walker).
    pub fn frame(&mut self, walker: &mut Walker, lib: &Library, mut g: Gpu, game_ms: i64) -> Option<u32> {
        let now = self.session.now();
        let mut new_model = None;
        for e in self.session.pump() {
            match e {
                Event::State { channel, data } if (channel as usize) < SLOTS => {
                    let self_id = self.session.self_id;
                    let skel = self.puppets[channel as usize].as_ref().and_then(|p| p.sprite.as_ref()).map_or(0, |s| s.skel_type);
                    let scene = &mut walker.scene;
                    let walk = &mut self.walks[channel as usize];
                    self.entities.channel(channel as usize, &data, self_id, now, UPDATE_RATE, |e| {
                        if walk.as_ref().map(|w| w.0) != Some(e.id) {
                            let mut w = WalkControl::<Ee>::default();
                            w.init(Vect3::default(), Vect3::default(), scene);
                            *walk = Some((e.id, w));
                        }
                        // VICreatureEntity::Sync: the update becomes the walk
                        // control's physics; a creature that does not walk is
                        // put on the ground at once.
                        if let (Some(c), Some((_, w))) = (&e.creature, walk.as_mut()) {
                            let p = WalkPhysics::<Ee> {
                                position: v3(e.pos),
                                angles: v3(e.hpr),
                                velocity: v3(c.velocity),
                                angular_vel: v3(c.angular_vel),
                                force: v3(c.accel),
                                torque: v3(c.angular_accel),
                                floor: F::new(c.floor),
                                loco_state: c.state as i32,
                                target_heading: F::new(c.target_heading),
                                flags: c.walk_flags,
                                ..Default::default()
                            };
                            w.set_update_physics(&p, scene);
                        }
                        if e.creature.as_ref().is_some_and(|c| c.p255 == 0) {
                            let (mut pos, mut hpr) = (v3(e.pos), v3(e.hpr));
                            vi::creature::extrapolate_ground(scene, -1, skel, None, &mut pos, &mut hpr);
                            e.pos = pos.to_array();
                            e.hpr = hpr.to_array();
                        }
                        e.extrapolated = game_ms;
                    });
                }
                Event::Disconnected(r) => eprintln!("play: disconnected (reason {r})"),
                Event::WorldEntry { self_id, player } => {
                    eprintln!("play: world entry again as {self_id:#x} at {:?}", player.pos);
                    self.player = player;
                }
                _ => {}
            }
        }
        let self_id = self.session.self_id;
        // UpdateCreatures / VICreatureEntity::Walk: a creature with its own
        // walk physics (a player, +0x255) is stepped by it; any other is
        // carried along its velocity since it was last moved (ExtrapolateMove).
        let me = self.self_id32();
        for (slot, e) in self.entities.slots.iter_mut().enumerate() {
            let Some(e) = e else { continue };
            let Some(c) = &e.creature else { continue };
            if Some(e.id) == me {
                continue;
            }
            if c.p255 != 0
                && let Some((_, w)) = self.walks[slot].as_mut()
            {
                w.process(1, &mut walker.scene);
                e.pos = w.committed.position.to_array();
                e.hpr = [w.committed.angles.x.0, 0.0, 0.0];
                continue;
            }
            let skel = self.puppets[slot].as_ref().and_then(|p| p.sprite.as_ref()).map_or(0, |s| s.skel_type);
            let dt = F::<Ee>::new((game_ms - e.extrapolated) as i32 as f32) * F::new(0.001);
            let (mut pos, mut hpr) = (v3(e.pos), v3(e.hpr));
            vi::creature::extrapolate_move(&mut walker.scene, -1, skel, None, &v3(c.velocity), dt, &mut pos, &mut hpr);
            e.pos = pos.to_array();
            e.hpr = hpr.to_array();
            e.extrapolated = game_ms;
        }
        self.entities.extrapolate(self_id, now, UPDATE_RATE);
        // SendPosition, at the update rate.
        if self_id.is_some() && now.wrapping_sub(self.last_send) >= UPDATE_RATE {
            self.last_send = now;
            let msg = vi::selfstate::encode(self.player.world as u8, &walker.cam.walk.committed, 0, u32::MAX);
            self.session.send_state(0x40, &msg);
        }
        let me = self.self_id32();
        for (slot, entity) in self.entities.slots.iter().enumerate() {
            let want = entity.as_ref().filter(|e| Some(e.id) != me && e.model != 0);
            if let Some(e) = entity
                && Some(e.id) == me
                && e.model != 0
                && self.self_model != Some(e.model)
            {
                self.self_model = Some(e.model);
                new_model = Some(e.model);
            }
            if let Some(e) = entity
                && Some(e.id) == me
                && e.model != 0
            {
                let look = e.creature.as_ref().map(look_of);
                if self.self_puppet.as_ref().map(|p| (p.model, p.look)) != Some((e.model, look)) {
                    self.self_puppet = build_puppet(lib, &mut g, e, look, slot, game_ms);
                }
            }
            let Some(e) = want else {
                self.puppets[slot] = None;
                continue;
            };
            let look = e.creature.as_ref().map(look_of);
            if self.puppets[slot].as_ref().map(|p| (p.model, p.look)) != Some((e.model, look)) {
                self.puppets[slot] = build_puppet(lib, &mut g, e, look, slot, game_ms);
            }
            let Some(p) = self.puppets[slot].as_mut() else { continue };
            let f = F::<Ee>::new;
            let actor = Matrix44::transform(
                &Vect3::new(f(e.actor_hpr[0]), f(e.actor_hpr[1]), F::ZERO),
                f(if e.scale > 0.0 { e.scale } else { 1.0 }),
                &Vect3::new(f(e.actor_pos[0]), f(e.actor_pos[1]), f(e.actor_pos[2])),
            );
            let (matrix, palette) = match p.sprite.as_mut() {
                Some(cs) => {
                    p.anims.time = game_ms;
                    // UpdateCreatures: a walking creature's sprite follows its
                    // walk control; any other takes SetLocomotion(state).
                    if let Some(c) = &e.creature {
                        match self.walks[slot].as_ref() {
                            Some((_, w)) if c.p255 != 0 => {
                                w.update_sprite(cs, &p.anims, &mut p.rand);
                            }
                            _ => cs.set_locomotion(c.state as i32, &p.anims, &mut p.rand),
                        }
                    }
                    cs.h.process(0, &p.anims);
                    let m = cs.h.calc_trans_about_center(&actor, &p.anims);
                    cs.h.set_blend_matrices();
                    for (node, gpu) in &p.attached {
                        if let Some(n) = cs.h.nodes.get(*node as usize) {
                            g.renderer.set_instances(g.queue, gpu, &[Matrix44::mul(&n.world, &m).to_cols()]);
                        }
                    }
                    (m.to_cols(), cs.h.blend.iter().map(|m| m.to_cols()).collect::<Vec<_>>())
                }
                None => (actor.to_cols(), Vec::new()),
            };
            g.renderer.set_instances(g.queue, &p.gpu, &[matrix]);
            if !palette.is_empty() {
                g.renderer.set_palette(g.queue, &p.gpu, &palette);
            }
        }
        new_model
    }

    /// Whether the player is drawn dressed (the caller hides its plain body).
    pub fn has_self(&self) -> bool {
        self.self_puppet.is_some()
    }

    /// Places the dressed player with the walker's pose: the body at its
    /// model matrix and palette, the attachments on the walker's nodes.
    pub fn place_self(&self, walker: &Walker, matrix: [[f32; 4]; 4], palette: &[[[f32; 4]; 4]], r: &Renderer, queue: &wgpu::Queue) {
        let Some(p) = &self.self_puppet else { return };
        r.set_instances(queue, &p.gpu, &[matrix]);
        if !palette.is_empty() {
            r.set_palette(queue, &p.gpu, palette);
        }
        let Some(cs) = walker.sprite.as_ref() else { return };
        let centre = glam_to_matrix(matrix);
        for (node, gpu) in &p.attached {
            if let Some(n) = cs.h.nodes.get(*node as usize) {
                r.set_instances(queue, gpu, &[Matrix44::mul(&n.world, &centre).to_cols()]);
            }
        }
    }

    pub fn models(&self) -> impl Iterator<Item = &GpuModel> {
        self.puppets.iter().chain(std::iter::once(&self.self_puppet)).flatten().flat_map(|p| std::iter::once(&p.gpu).chain(p.attached.iter().map(|a| &a.1)))
    }

    pub fn describe(&self) -> String {
        let n = self.entities.slots.iter().flatten().count();
        format!("{n} entities, {} packets sent", self.session.packets_sent)
    }
}
