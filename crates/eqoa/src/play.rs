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

/// `gEntityUpdateRate`.
pub const UPDATE_RATE: u32 = 100;

/// One drawn entity: its model as a CSprite, animated on its own clock.
struct Puppet {
    model: u32,
    sprite: Option<CSprite<Ee>>,
    anims: AnimScene<Ee>,
    rand: Rand,
    gpu: GpuModel,
}

pub struct Play {
    pub session: Session,
    pub player: Player,
    pub entities: Entities,
    last_send: u32,
    puppets: Vec<Option<Puppet>>,
    /// The player's model as the server last named it on its channel.
    pub self_model: Option<u32>,
}

pub struct Gpu<'a> {
    pub renderer: &'a mut Renderer,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
}

impl Play {
    pub fn new(session: Session, player: Player) -> Self {
        Play { session, player, entities: Entities::default(), last_send: 0, puppets: (0..SLOTS).map(|_| None).collect(), self_model: None }
    }

    fn self_id32(&self) -> Option<u32> {
        self.session.self_id.map(|s| s as u32)
    }

    /// One frame. Returns the player's model when the server has just named
    /// a different one (the caller re-dresses the walker).
    pub fn frame(&mut self, walker: &mut Walker, lib: &Library, g: Gpu, game_ms: i64) -> Option<u32> {
        let now = self.session.now();
        let mut new_model = None;
        for e in self.session.pump() {
            match e {
                Event::State { channel, data } if (channel as usize) < SLOTS => {
                    let self_id = self.session.self_id;
                    self.entities.channel(channel as usize, &data, self_id, now, UPDATE_RATE);
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
        self.entities.extrapolate(self_id, now, UPDATE_RATE);
        // SendPosition, at the update rate.
        if self_id.is_some() && now.wrapping_sub(self.last_send) >= UPDATE_RATE {
            self.last_send = now;
            let msg = vi::selfstate::encode(self.player.world as u8, &walker.cam.walk.committed, 0, u32::MAX);
            self.session.send_state(0x40, &msg);
        }
        let me = self.self_id32();
        let textures = render::library_textures(lib);
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
            let Some(e) = want else {
                self.puppets[slot] = None;
                continue;
            };
            if self.puppets[slot].as_ref().map(|p| p.model) != Some(e.model) {
                self.puppets[slot] = lib.lookup(e.model as i32).and_then(|r| {
                    let model = lib.build_model(r)?;
                    let mut anims = AnimScene { time: game_ms, ..Default::default() };
                    let mut rand = Rand(slot as u64 + 1);
                    let sprite = load_csprite(lib, r, &mut anims, &mut AnimCache::new(), &mut rand);
                    let gpu = g.renderer.upload(g.device, g.queue, &model, &[render::IDENTITY], &textures);
                    Some(Puppet { model: e.model, sprite, anims, rand, gpu })
                });
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
                    let _ = &mut p.rand;
                    cs.h.process(0, &p.anims);
                    let m = cs.h.calc_trans_about_center(&actor, &p.anims);
                    cs.h.set_blend_matrices();
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

    pub fn models(&self) -> impl Iterator<Item = &GpuModel> {
        self.puppets.iter().flatten().map(|p| &p.gpu)
    }

    pub fn describe(&self) -> String {
        let n = self.entities.slots.iter().flatten().count();
        format!("{n} entities, {} packets sent", self.session.packets_sent)
    }
}
