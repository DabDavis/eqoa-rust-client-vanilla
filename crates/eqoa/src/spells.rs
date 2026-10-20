//! Spell effects in the client: SPELLFX.CSF's effects (0xC200) loaded as
//! the scene loads them, played as `VIClient` plays them (`CastingCombat`
//! for 0x00B1, `Msg0776`, and the player's own swing in `AutoAttackTick`),
//! run each frame by `vi::spell` over the creatures as the scene's actors,
//! and drawn: their particles as billboards, their lightning as strips; the
//! sound sprites they create heard as `VIScene::UpdateSoundEmitters`' second
//! half hears a sound sprite.
//!
//! The actors: an entity's slot is its actor, the player is [`SELF`], and an
//! actor an effect makes is numbered from [`MADE`]. Not here: the weapon
//! trail (`SetWeaponTrail` is taken and dropped), a hand's projectile sprite
//! (no item carries one yet, so no arrow flies), the animated sprites an
//! effect can create (three events on the disc; tracked, not drawn), the fade
//! (no event on the disc uses it). The vibrate events of the player's own
//! effects go to the pad (`rumbles`: `VIClient`'s `Rumble`, which the client
//! always hands the effect system). The effects run on a particle system of
//! their own, not the world's.

use esf::{Library, Ref};
use render::billboard::{Batch, Strips};
use std::collections::{BTreeMap, HashMap};
use vi::camera::{CameraEffect, Eye};
use vi::csprite::{CSprite, Rand};
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::hsprite::AnimScene;
use vi::particle::{Color, Definition, System};
use vi::spell::{SpellEffect, SpellScene, SpellSystem};
use vi::vect::Vect3;

/// The player's actor.
pub const SELF: i32 = 1000;
/// The first actor an effect makes.
pub const MADE: i32 = 2000;

/// A creature as an effect sees it: its sprite (a CSprite, or none), the
/// clock it animates on, its transform, the centre transform it was last
/// drawn with (`CalcTransAboutCenter`), and the attack types of the items
/// in its two hands.
pub struct Body<'a> {
    pub actor: i32,
    pub sprite: Option<(&'a mut CSprite<Ee>, &'a AnimScene<Ee>, &'a mut Rand)>,
    pub transform: Matrix44<Ee>,
    pub centre: Matrix44<Ee>,
    pub attack: [i32; 2],
}

/// The attack types a creature's sprite holds for its two hands
/// (`VICSprite::m_items[0..2].m_attackType`, what `SetAttackAction` reads):
/// `VICreatureEntity::ApplyItems` hands the look each hand's item off the
/// channel (`m_itemOver` before `m_item`) with its type, and the look's
/// `Apply` attaches an item with that type (`AttachItem`); a hand that holds
/// nothing keeps the 11, hand to hand, that `Clear` and `DetachItem` leave.
pub fn hand_attack_types(c: &net::entity::Creature) -> [i32; 2] {
    std::array::from_fn(|i| {
        let model = if c.items_over[i] != 0 { c.items_over[i] } else { c.items[i] };
        if model != 0 { c.item_types[i] as i32 } else { 11 }
    })
}

/// A sprite an effect names: its type (9 a sound, 4 an animated sprite),
/// and a sound's radius, volume, wave, pan and attenuation (`VISoundSprite`).
#[derive(Clone)]
struct Sprite {
    ty: i32,
    radius: F<Ee>,
    volume: F<Ee>,
    wave: Option<Ref>,
    pan: bool,
    attenuation: i32,
}

/// An actor an effect made.
struct Made {
    sprite: i32,
    at: Vect3<Ee>,
    hpr: Vect3<Ee>,
    scale: F<Ee>,
}

/// A cast to play: the effect's DictID, the cast time, the caster, the
/// target and the area's targets as entity ids.
#[derive(Clone, Debug)]
pub struct Cast {
    pub effect: u32,
    pub time: i32,
    pub caster: u32,
    pub target: u32,
    pub targets: Vec<u32>,
}

/// 0x00B1 (`CastingCombat`): caster, target, a count and that many targets,
/// the cast time, the effect; 0x0776 (`Msg0776`): caster, target, effect.
pub fn cast_of(op: u16, data: &[u8]) -> Option<Cast> {
    let mut r = net::stream::Reader::new(data);
    let c = match op {
        0x00B1 => {
            let caster = r.u32();
            let target = r.u32();
            let n = r.u8();
            let targets = (0..n).map(|_| r.u32()).collect();
            let time = r.u32() as i32;
            Cast { caster, target, targets, time, effect: r.u32() }
        }
        0x0776 => Cast { caster: r.u32(), target: r.u32(), effect: r.u32(), time: 0, targets: Vec::new() },
        _ => return None,
    };
    (!r.error).then_some(c)
}

pub struct Spells {
    sys: SpellSystem<Ee>,
    ps: System<Ee>,
    rand: Rand,
    by_dict: HashMap<u32, usize>,
    sprites: Vec<Sprite>,
    made: BTreeMap<i32, Made>,
    next: i32,
    /// `VILightningFxInit`'s texture
    lightning: i32,
    /// the sound sprites playing: sprite -> channel, and each sound's wave on
    /// the device
    playing: BTreeMap<i32, i32>,
    waves: HashMap<i32, i32>,
    /// this frame's drawing
    pub batches: Vec<Batch>,
    pub strips: Vec<Strips>,
    pub played: usize,
    /// bolts drawn, all frames
    pub bolts: usize,
    /// the vibrate events, as the callback gets them (power, time, mode:
    /// `Rumble`'s motor, level and ms)
    pub rumbles: Vec<(u8, u8, i32)>,
}

impl Spells {
    /// Every 0xC200 record in the library (SPELLFX.CSF), with the particle
    /// definitions and sprites its events name.
    pub fn load(lib: &Library, time: i64) -> Spells {
        let mut s = Spells {
            sys: SpellSystem::default(),
            ps: System::new(time, 2000),
            rand: Rand(1),
            by_dict: HashMap::new(),
            sprites: Vec::new(),
            made: BTreeMap::new(),
            next: MADE,
            lightning: vi::hash::hash_resource_id("lightning.tga") as i32,
            playing: BTreeMap::new(),
            waves: HashMap::new(),
            batches: Vec::new(),
            strips: Vec::new(),
            played: 0,
            bolts: 0,
            rumbles: Vec::new(),
        };
        let mut defs: HashMap<Ref, usize> = HashMap::new();
        let mut sprites: HashMap<Ref, usize> = HashMap::new();
        for f in 0..lib.files.len() {
            let c = lib.file(f);
            for n in (0..c.nodes.len()).filter(|&n| c.nodes[n].ty == 0xC200) {
                let (ps, list) = (&mut s.ps, &mut s.sprites);
                let e = SpellEffect::<Ee>::parse(c, n, &mut |kind, id| {
                    let Some(r) = lib.lookup(id as i32) else { return -1 };
                    if kind == 3 {
                        if let Some(&k) = defs.get(&r) {
                            return k as i32;
                        }
                        let rc = lib.file(r.file);
                        let Some(d) = rc.child(r.node, 0xC020).and_then(|b| Definition::<Ee>::parse(rc.body(b), rc.node(b).version as i32)) else { return -1 };
                        let k = ps.add_definition(d);
                        defs.insert(r, k);
                        k as i32
                    } else {
                        if let Some(&k) = sprites.get(&r) {
                            return k as i32;
                        }
                        list.push(sprite(lib, r));
                        sprites.insert(r, list.len() - 1);
                        (list.len() - 1) as i32
                    }
                });
                if let Some(e) = e {
                    // the first registered wins
                    if let std::collections::hash_map::Entry::Vacant(v) = s.by_dict.entry(e.id) {
                        v.insert(s.sys.effects.len());
                        s.sys.effects.push(e);
                    }
                }
            }
        }
        s
    }

    pub fn describe(&self) -> String {
        format!("{} spell effects, {} played, {} particles, {} bolt frames drawn", self.sys.effects.len(), self.played, self.sys.particle_count(&self.ps), self.bolts)
    }

    /// An effect's DictID by its place in the files, for trying one out.
    pub fn nth(&self, i: usize) -> Option<u32> {
        self.sys.effects.get(i).map(|e| e.id)
    }

    /// `m_dict.FindTyped(effect, 21)`
    pub fn has(&self, effect: u32) -> bool {
        self.by_dict.contains_key(&effect)
    }

    /// One frame at scene time `time`: the casts played (each actor named,
    /// as `CastingCombat` names it), every playback run, its particles
    /// rendered and its lightning drawn; then the sound sprites heard from
    /// the camera at `eye`.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, bodies: &mut [Body], casts: &[(Cast, i32, i32, Vec<i32>)], player: i32, camera: Option<&mut CameraEffect<Ee>>, time: i64, eye: &Eye<Ee>, lib: &Library, sound: Option<&mut crate::sound::Sound>) {
        self.sys.player_actor = player;
        self.sys.camera = camera.is_some();
        // `SetVibrateCallback(Rumble, &m_console)`: the client always sets one
        self.sys.vibrate = true;
        let mut sc = Scene { bodies, made: &mut self.made, next: &mut self.next, sprites: &mut self.sprites, time, camera, rumbles: &mut self.rumbles };
        for (c, caster, target, targets) in casts {
            let Some(&i) = self.by_dict.get(&c.effect) else { continue };
            if self.sys.play(i, c.time, *caster, *target, targets, &mut sc) != -1 {
                self.played += 1;
            }
        }
        self.sys.process(&mut sc, &mut self.ps, &mut self.rand);
        self.batches = crate::particles::render(&mut self.ps, time);
        self.strips = self
            .sys
            .bolts
            .iter()
            .map(|b| Strips {
                texture: self.lightning,
                blend: 1,
                ztest: true,
                zwrite: false,
                color: [b.color.r.0, b.color.g.0, b.color.b.0, b.color.a.0],
                strips: b.strips.iter().map(|s| s.iter().map(|(uv, v)| ([uv[0].0, uv[1].0], v.to_array())).collect()).collect(),
                colors: Vec::new(),
            })
            .collect();
        self.bolts += self.strips.len();
        if let Some(s) = sound {
            self.hear(s, lib, eye);
        }
    }

    /// An entity went: `NotifyActorWasDestroyed`.
    pub fn actor_gone(&mut self, actor: i32, bodies: &mut [Body], time: i64) {
        let mut sc = Scene { bodies, made: &mut self.made, next: &mut self.next, sprites: &mut self.sprites, time, camera: None, rumbles: &mut self.rumbles };
        self.sys.notify_actor_destroyed(actor, &mut sc);
    }

    /// `StopIfThisWasCaster(actor)`: a death ends the spells it was casting.
    pub fn stop_caster(&mut self, actor: i32, bodies: &mut [Body], time: i64) {
        let mut sc = Scene { bodies, made: &mut self.made, next: &mut self.next, sprites: &mut self.sprites, time, camera: None, rumbles: &mut self.rumbles };
        self.sys.stop_if_this_was_caster(actor, &mut sc, &mut self.ps);
    }

    /// `UpdateSoundEmitters`' sound sprites, for the ones the effects made:
    /// each one whose sphere the camera is in plays (started when it was not
    /// heard last frame), at the volume its attenuation gives the distance
    /// and the pan of its side of the camera; one no longer heard is stopped.
    fn hear(&mut self, s: &mut crate::sound::Sound, lib: &Library, eye: &Eye<Ee>) {
        let cp = eye.position;
        let mut heard: BTreeMap<i32, (Vect3<Ee>, F<Ee>)> = BTreeMap::new();
        for m in self.made.values() {
            let Some(sp) = self.sprites.get(m.sprite as usize) else { continue };
            if sp.ty != 9 || sp.wave.is_none() {
                continue;
            }
            let r = m.scale * sp.radius;
            if cp.distance(&m.at) <= r {
                heard.insert(m.sprite, (m.at, m.scale));
            }
        }
        for (&k, &(at, scale)) in &heard {
            let sp = self.sprites[k as usize].clone();
            if !self.playing.contains_key(&k) {
                let wave = match self.waves.get(&k) {
                    Some(&w) => w,
                    None => {
                        let r = sp.wave.unwrap();
                        let c = lib.file(r.file);
                        let Some(w) = esf::sound::wave(c, r.node) else { continue };
                        let i = s.dev.create_wave(w.channels, w.blocks, w.rate, w.volume, w.pan, w.looped, &w.adpcm);
                        self.waves.insert(k, i);
                        i
                    }
                };
                let ch = s.dev.play(wave, 0, 0);
                if ch <= 0 {
                    continue;
                }
                self.playing.insert(k, ch);
            }
            let ch = self.playing[&k];
            let mut d = Vect3::new(cp.x - at.x, F::ZERO, cp.z - at.z);
            let mut right = Vect3::new(eye.direction.z, F::ZERO, -eye.direction.x);
            d.normalize();
            right.normalize();
            let pan = d.x * right.x + d.z * right.z;
            let mut t = cp.distance(&at) / (scale * sp.radius);
            match sp.attenuation {
                1 => t = t * (t * (t * t)),
                2 => {
                    t = F::ONE - t;
                    t = t * (t * (t * t));
                    t = F::ONE - t;
                }
                _ => {}
            }
            s.dev.set_volume(ch, ((F::ONE - t) * sp.volume).0);
            s.dev.set_pan(ch, if sp.pan { pan.0 } else { 0.0 });
        }
        let gone: Vec<i32> = self.playing.keys().copied().filter(|k| !heard.contains_key(k)).collect();
        for k in gone {
            if let Some(ch) = self.playing.remove(&k) {
                s.dev.stop(ch);
            }
        }
    }
}

/// A sprite an effect names, as `ParseSoundSpriteObj` reads a sound (radius
/// and volume, 1 and 1 before version 1; the wave its child) and anything
/// else by its type.
fn sprite(lib: &Library, r: Ref) -> Sprite {
    let c = lib.file(r.file);
    let mut s = Sprite { ty: 0, radius: F::ONE, volume: F::ONE, wave: None, pan: true, attenuation: 0 };
    match c.node(r.node).ty {
        0xB100 => {
            s.ty = 9;
            if let Some(h) = c.child(r.node, 0xB110).or_else(|| c.node(r.node).children.first().copied()) {
                let d = c.body(h);
                if c.node(h).version != 0 && d.len() >= 12 {
                    s.radius = F::new(f32::from_le_bytes(d[4..8].try_into().unwrap()));
                    s.volume = F::new(f32::from_le_bytes(d[8..12].try_into().unwrap()));
                }
            }
            s.wave = c.node(r.node).children.iter().copied().find(|&k| c.node(k).ty == esf::sound::WAVE).map(|node| Ref { file: r.file, node });
        }
        0x2200 => s.ty = 4,
        _ => s.ty = 1,
    }
    s
}

/// The scene as the effects see it.
struct Scene<'a, 'b, 'c> {
    bodies: &'a mut [Body<'b>],
    made: &'a mut BTreeMap<i32, Made>,
    next: &'a mut i32,
    sprites: &'a mut Vec<Sprite>,
    time: i64,
    camera: Option<&'c mut CameraEffect<Ee>>,
    rumbles: &'a mut Vec<(u8, u8, i32)>,
}

impl<'b> Scene<'_, 'b, '_> {
    fn body(&self, actor: i32) -> Option<&Body<'b>> {
        self.bodies.iter().find(|b| b.actor == actor)
    }

    fn body_mut(&mut self, actor: i32) -> Option<&mut Body<'b>> {
        self.bodies.iter_mut().find(|b| b.actor == actor)
    }
}

impl SpellScene<Ee> for Scene<'_, '_, '_> {
    fn time(&self) -> i64 {
        self.time
    }

    fn sprite_type(&self, actor: i32) -> Option<i32> {
        if let Some(b) = self.body(actor) {
            return Some(if b.sprite.is_some() { 5 } else { 1 });
        }
        let m = self.made.get(&actor)?;
        self.sprites.get(m.sprite as usize).map(|s| s.ty).filter(|&t| t != 0)
    }

    fn actor_transform(&self, actor: i32) -> Matrix44<Ee> {
        if let Some(b) = self.body(actor) {
            return b.transform;
        }
        match self.made.get(&actor) {
            Some(m) => Matrix44::transform(&m.hpr, m.scale, &m.at),
            None => Matrix44::identity(),
        }
    }

    fn actor_location(&self, actor: i32) -> Vect3<Ee> {
        self.actor_transform(actor).m[3].xyz()
    }

    /// `CalcSlotWorldTransform`: the node's world transform under the
    /// actor's centre transform, as last posed.
    fn slot_world_transform(&self, actor: i32, node: i32) -> Matrix44<Ee> {
        let Some(b) = self.body(actor) else { return self.actor_transform(actor) };
        let Some((cs, _, _)) = &b.sprite else { return b.transform };
        let n = cs.node_ids.get(node as usize).copied().unwrap_or(-1);
        match cs.h.nodes.get(n.max(0) as usize).filter(|_| n >= 0) {
            Some(nd) => Matrix44::mul(&nd.world, &b.centre),
            None => b.transform,
        }
    }

    fn set_action(&mut self, actor: i32, id: i32) -> i32 {
        match self.body_mut(actor).and_then(|b| b.sprite.as_mut()) {
            Some((cs, anims, rand)) => cs.set_action(id, anims, rand),
            None => -1,
        }
    }

    fn set_attack_action(&mut self, actor: i32, slot: i32, alt: bool) -> i32 {
        let Some(b) = self.body_mut(actor) else { return -1 };
        let Some(id) = vi::spell::attack_action(b.attack, slot, alt) else { return -1 };
        match b.sprite.as_mut() {
            Some((cs, anims, rand)) => cs.set_action(id, anims, rand),
            None => -1,
        }
    }

    fn set_weapon_trail(&mut self, _actor: i32, _id: i32, _color: Color<Ee>) {}

    fn find_trigger(&self, actor: i32, hash: u32) -> Option<i32> {
        let (cs, _, _) = self.body(actor)?.sprite.as_ref()?;
        cs.h.find_trigger(hash as i32).map(|i| i as i32)
    }

    fn reset_trigger(&mut self, actor: i32, i: i32) {
        if let Some((cs, _, _)) = self.body_mut(actor).and_then(|b| b.sprite.as_mut()) {
            cs.h.reset_trigger(i as usize);
        }
    }

    fn has_tripped(&self, actor: i32, i: i32) -> bool {
        self.body(actor).and_then(|b| b.sprite.as_ref()).is_some_and(|(cs, _, _)| cs.h.triggers.get(i as usize).is_some_and(|t| t.tripped != 0))
    }

    fn is_action_complete(&self, actor: i32, action: i32) -> bool {
        self.body(actor).and_then(|b| b.sprite.as_ref()).is_none_or(|(cs, _, _)| cs.is_action_complete(action))
    }

    fn item_projectile(&self, _actor: i32, _slot: usize) -> i32 {
        -1
    }

    fn create_actor(&mut self, at: Vect3<Ee>, hpr: Vect3<Ee>, scale: F<Ee>, sprite: i32) -> i32 {
        let id = *self.next;
        *self.next += 1;
        self.made.insert(id, Made { sprite, at, hpr, scale });
        id
    }

    fn release_actor(&mut self, actor: i32) {
        self.made.remove(&actor);
    }

    fn set_actor_collideable(&mut self, _actor: i32, _on: bool) {}

    fn set_actor_location(&mut self, actor: i32, at: Vect3<Ee>) {
        if let Some(m) = self.made.get_mut(&actor) {
            m.at = at;
        }
    }

    fn set_actor_hpr(&mut self, actor: i32, hpr: Vect3<Ee>) {
        if let Some(m) = self.made.get_mut(&actor) {
            m.hpr = hpr;
        }
    }

    fn set_actor_transform(&mut self, actor: i32, hpr: Vect3<Ee>, scale: F<Ee>, at: Vect3<Ee>) {
        if let Some(m) = self.made.get_mut(&actor) {
            m.hpr = hpr;
            m.scale = scale;
            m.at = at;
        }
    }

    fn sprite_kind(&self, sprite: i32) -> i32 {
        self.sprites.get(sprite as usize).map_or(0, |s| s.ty)
    }

    fn copy_sprite(&mut self, sprite: i32) -> i32 {
        let Some(s) = self.sprites.get(sprite as usize).cloned() else { return -1 };
        self.sprites.push(s);
        (self.sprites.len() - 1) as i32
    }

    fn restart_hsprite(&mut self, _sprite: i32, _mode: i32) {}

    fn sound_settings(&mut self, actor: i32, pan: bool, attenuation: i32) {
        let Some(m) = self.made.get(&actor) else { return };
        if let Some(s) = self.sprites.get_mut(m.sprite as usize) {
            s.pan = pan;
            s.attenuation = attenuation;
        }
    }

    fn fade_begin(&mut self, _actor: i32) -> Option<u8> {
        None
    }

    fn fade_set(&mut self, _actor: i32, _alpha: u8) {}

    fn shake(&mut self, speed: i32, amp: F<Ee>) {
        if let Some(c) = self.camera.as_mut() {
            c.shake(speed, amp);
        }
    }

    fn shake_terp(&mut self, mag: F<Ee>, time: F<Ee>) {
        if let Some(c) = self.camera.as_mut() {
            c.shake_terp(mag, time);
        }
    }

    fn camera_reset(&mut self) {
        if let Some(c) = self.camera.as_mut() {
            c.reset();
        }
    }

    fn vibrate(&mut self, power: u8, time: u8, mode: i32) {
        self.rumbles.push((power, time, mode));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty hand is 11 (hand to hand), which the swing's play-attack
    /// event (type 15) plays as action 23 or 22; read as 0, as it was, it
    /// plays nothing. A held item takes the channel's type, the override's
    /// model counting as the item.
    #[test]
    fn an_empty_hand_swings_hand_to_hand() {
        use vi::spell::attack_action;
        let bare = net::entity::Creature::default();
        assert_eq!(hand_attack_types(&bare), [11, 11]);
        assert_eq!(attack_action(hand_attack_types(&bare), 0, false), Some(23));
        assert_eq!(attack_action(hand_attack_types(&bare), 1, false), Some(22));
        // the negative control: the type an empty hand used to get
        assert_eq!(attack_action([0, 0], 0, false), None);
        let mut c = net::entity::Creature { item_types: [1, 5], ..Default::default() };
        c.items[0] = 0x1234;
        assert_eq!(hand_attack_types(&c), [1, 11]);
        c.items_over[1] = 0x5678;
        assert_eq!(hand_attack_types(&c), [1, 5]);
        assert_eq!(attack_action(hand_attack_types(&c), 1, false), Some(27));
        // an item whose type the channel leaves 0 swings nothing, as in the game
        c.item_types = [0, 0];
        assert_eq!(attack_action(hand_attack_types(&c), 0, false), None);
    }
}
