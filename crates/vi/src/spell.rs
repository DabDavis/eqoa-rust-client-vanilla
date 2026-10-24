//! Spell effects: `VISpellEvent`/`VISpellEventList`/`VISpellEffect`,
//! `VISpellActiveEvent`, `VISpellEventDriver`, `VISpellEffectDriver` and
//! `VISpellEffectSystem`, line by line from the decomp (`vispell*.cpp`), and
//! the 0xC200 record as `VIESFParse::ParseSpellEffectObj` reads it.
//!
//! An effect is three event lists (the caster's, the target's, the area's).
//! A cast plays the caster's list on a driver; its events start particle
//! emitters at the actors' nodes, blend their motifs, set actions, wait on
//! them or on the clock, launch projectiles (four flight modes) and lightning
//! at the target (which then plays the target's list), create, move and fade
//! sprites and sounds, and shake the camera. One new event is dispatched a
//! frame; a projectile blocks its list until it lands.
//!
//! Everything the effect does to the world goes through [`SpellScene`] (the
//! actors and their sprites, the camera effect) and [`System`] (the particle
//! system, shared with the world's emitters); the random numbers are newlib's
//! `rand()`, passed in. Generic over the float model.

use crate::csprite::Rand;
use crate::fpu::{F, Model};
use crate::geom::{Matrix44, Vect4};
use crate::lightning::{BoltDraw, Lightning};
use crate::particle::{Color, Matrix33, System};
use crate::vect::Vect3;

/// `VISpellEvent`, 44 bytes in the file. `id` is resolved at load: a particle
/// definition (types 3 and 8) or a scene sprite (9 and 11), -1 for none.
#[derive(Clone, Copy, Debug)]
pub struct SpellEvent<M: Model> {
    pub ty: i32,
    pub p04: i32,
    pub id: i32,
    pub p0c: i32,
    pub p10: i32,
    /// `m_f14` .. `m_f28`
    pub f: [F<M>; 6],
}

/// `VISpellEffect`: the caster's, the target's and the area's event lists.
#[derive(Clone, Debug, Default)]
pub struct SpellEffect<M: Model> {
    pub id: u32,
    pub lists: [Vec<SpellEvent<M>>; 3],
}

pub const CASTER: usize = 0;
pub const TARGET: usize = 1;
pub const AREA: usize = 2;

impl<M: Model> SpellEffect<M> {
    /// `ParseSpellEffectObj` over a 0xC200 record: the id (0xC210), then the
    /// three lists (0xC220). `resolve(kind, dict_id)` names a particle
    /// definition (kind 3) or a sprite (kind 9) as an index, -1 when unknown;
    /// version 0/1 waits get 5000 ms.
    pub fn parse(c: &esf::Container, n: esf::NodeId, resolve: &mut dyn FnMut(i32, u32) -> i32) -> Option<Self> {
        let ver = c.node(n).version;
        let id = c.child(n, 0xC210).and_then(|h| Some(u32::from_le_bytes(c.body(h).get(..4)?.try_into().ok()?))).unwrap_or(0);
        let d = c.body(c.child(n, 0xC220)?);
        let i = |o: usize| -> Option<i32> { Some(i32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?)) };
        let mut e = SpellEffect { id, lists: Default::default() };
        let mut p = 0usize;
        for k in 0..3 {
            let count = i(p)?;
            p += 4;
            for _ in 0..count.max(0) {
                let fl = |o: usize| -> Option<F<M>> { Some(F::new(f32::from_bits(i(p + o)? as u32))) };
                let mut ev = SpellEvent {
                    ty: i(p)?,
                    p04: i(p + 4)?,
                    id: i(p + 8)?,
                    p0c: i(p + 12)?,
                    p10: i(p + 16)?,
                    f: [fl(20)?, fl(24)?, fl(28)?, fl(32)?, fl(36)?, fl(40)?],
                };
                p += 44;
                if ver < 2 && ev.ty == 1 {
                    ev.p0c = 5000;
                }
                match ev.ty {
                    3 | 8 => ev.id = if ev.id != -1 && ev.id != 0 { resolve(3, ev.id as u32) } else { -1 },
                    9 | 11 => ev.id = resolve(9, ev.id as u32),
                    _ => {}
                }
                e.lists[k].push(ev);
            }
        }
        Some(e)
    }
}

/// The world as a spell effect sees it: `VIScene`'s actors and sprites, the
/// `VICSprite` methods it calls on them, the camera effect and the vibrator.
/// An actor is the scene's index; -1 is none.
pub trait SpellScene<M: Model> {
    /// `VIScene::GetTime`, ms
    fn time(&self) -> i64;
    /// `GetActorSpritePtr(actor)->m_type`, `None` for no sprite (or actor -1)
    fn sprite_type(&self, actor: i32) -> Option<i32>;
    fn actor_transform(&self, actor: i32) -> Matrix44<M>;
    fn actor_location(&self, actor: i32) -> Vect3<M>;
    /// `VICSprite::CalcSlotWorldTransform(node, actor)`
    fn slot_world_transform(&self, actor: i32, node: i32) -> Matrix44<M>;
    /// `VICSprite::SetAction`: the action's handle
    fn set_action(&mut self, actor: i32, id: i32) -> i32;
    /// `SetAttackAction` (alt false) / `SetAttackAltAction` on a hand
    fn set_attack_action(&mut self, actor: i32, slot: i32, alt: bool) -> i32;
    fn set_weapon_trail(&mut self, actor: i32, id: i32, color: Color<M>);
    /// `FindTrigger(hash)`: the trigger's index
    fn find_trigger(&self, actor: i32, hash: u32) -> Option<i32>;
    fn reset_trigger(&mut self, actor: i32, i: i32);
    fn has_tripped(&self, actor: i32, i: i32) -> bool;
    fn is_action_complete(&self, actor: i32, action: i32) -> bool;
    /// `m_items[slot].m_projectile`, -1 for none
    fn item_projectile(&self, actor: i32, slot: usize) -> i32;
    fn create_actor(&mut self, at: Vect3<M>, hpr: Vect3<M>, scale: F<M>, sprite: i32) -> i32;
    fn release_actor(&mut self, actor: i32);
    fn set_actor_collideable(&mut self, actor: i32, on: bool);
    fn set_actor_location(&mut self, actor: i32, at: Vect3<M>);
    fn set_actor_hpr(&mut self, actor: i32, hpr: Vect3<M>);
    fn set_actor_transform(&mut self, actor: i32, hpr: Vect3<M>, scale: F<M>, at: Vect3<M>);
    /// `Sprite(sprite)->m_type`
    fn sprite_kind(&self, sprite: i32) -> i32;
    /// `CopySprite(sprite, 0)`
    fn copy_sprite(&mut self, sprite: i32) -> i32;
    /// An HSprite's plays erased and its default animation added: mode 1
    /// once, 2 loop, 0 its own playback type
    fn restart_hsprite(&mut self, sprite: i32, mode: i32);
    /// A created sound actor's `EnablePan` and `SetAttenuationMode`
    fn sound_settings(&mut self, actor: i32, pan: bool, attenuation: i32);
    /// `NewFadeSprite`'s clone of the actor's sprite (or its HSprite's first
    /// attachment) and material palette: the material layer's alpha, `None`
    /// when there is nothing to fade
    fn fade_begin(&mut self, actor: i32) -> Option<u8>;
    /// `ProcessFadeSprite`: the clone's material layer alpha
    fn fade_set(&mut self, actor: i32, alpha: u8);
    /// `VICameraEffect::Shake(speed, amp)`: 0 none .. 3 fast
    fn shake(&mut self, speed: i32, amp: F<M>);
    fn shake_terp(&mut self, mag: F<M>, time: F<M>);
    fn camera_reset(&mut self);
    fn vibrate(&mut self, power: u8, time: u8, mode: i32);
}

/// A motif blender an event owns: on an emitter, linked (`idx`) or not.
#[derive(Clone, Copy, Debug)]
struct Blend<M: Model> {
    emitter: usize,
    idx: Option<usize>,
    factor: F<M>,
}

/// `VISpellActiveEvent`
#[derive(Clone, Debug)]
struct Active<M: Model> {
    ev: SpellEvent<M>,
    v14: Vect3<M>,
    v20: Vect3<M>,
    a: [Vect3<M>; 2],
    b: [Vect3<M>; 2],
    c: [Vect3<M>; 2],
    emitter0: Option<usize>,
    emitter1: Option<usize>,
    blender: Option<Blend<M>>,
    p84: i32,
    f88: F<M>,
    actor0: i32,
    actor1: i32,
    f9c: F<M>,
    fa0: F<M>,
    time: i64,
}

impl<M: Model> Active<M> {
    fn new(ev: &SpellEvent<M>) -> Self {
        Active {
            ev: *ev,
            v14: Vect3::default(),
            v20: Vect3::default(),
            a: Default::default(),
            b: Default::default(),
            c: Default::default(),
            emitter0: None,
            emitter1: None,
            blender: None,
            p84: -1,
            f88: F::ZERO,
            actor0: -1,
            actor1: -1,
            f9c: F::ZERO,
            fa0: F::ZERO,
            time: 0,
        }
    }

    fn particle_count(&self, ps: &System<M>) -> usize {
        self.emitter0.map_or(0, |e| ps.particle_count(e)) + self.emitter1.map_or(0, |e| ps.particle_count(e))
    }

    /// `VIMotifBlender::Motif() == 0`: never linked, or its emitter gone.
    fn blender_motif(&self, ps: &System<M>) -> bool {
        self.blender.is_some_and(|b| b.idx.is_some() && ps.get(b.emitter).is_some())
    }

    fn blender_factor(&self, ps: &System<M>) -> F<M> {
        match self.blender {
            Some(Blend { emitter, idx: Some(i), factor }) => ps.get(emitter).and_then(|e| e.blender_factor(i)).unwrap_or(factor),
            Some(b) => b.factor,
            None => F::ZERO,
        }
    }

    fn set_blender_factor(&mut self, ps: &mut System<M>, f: F<M>) {
        if let Some(b) = self.blender.as_mut() {
            if b.factor != f {
                b.factor = f;
            }
            if let (Some(i), Some(e)) = (b.idx, ps.get_mut(b.emitter)) {
                e.set_blender_factor(i, f);
            }
        }
    }
}

/// `~VISpellActiveEvent`: its actors back to the scene, its emitters
/// destroyed, its blender unlinked.
fn delete<M: Model>(e: Active<M>, scene: &mut dyn SpellScene<M>, ps: &mut System<M>) {
    if e.actor0 != -1 {
        scene.release_actor(e.actor0);
    }
    if e.actor1 != -1 {
        scene.release_actor(e.actor1);
    }
    if let Some(x) = e.emitter0 {
        ps.destroy_emitter(x);
    }
    if let Some(x) = e.emitter1 {
        ps.destroy_emitter(x);
    }
    if let Some(Blend { emitter, idx: Some(i), .. }) = e.blender
        && let Some(em) = ps.get_mut(emitter)
    {
        em.unlink_blender(i);
    }
}

/// What an event driver reaches of its effect driver and the world.
struct Ctx<'a, M: Model> {
    scene: &'a mut dyn SpellScene<M>,
    ps: &'a mut System<M>,
    rand: &'a mut Rand,
    effect: &'a SpellEffect<M>,
    caster: i32,
    target: i32,
    secondary: &'a [i32],
    trigger_time: i64,
    arg0: i32,
    arg1: i32,
    player_actor: i32,
    camera: bool,
    camera_on: bool,
    vibrate: bool,
    /// drivers added this step (`AddSpellEventDriver`), appended after it
    added: Vec<EventDriver<M>>,
    bolts: &'a mut Vec<BoltDraw<M>>,
}

impl<M: Model> Ctx<'_, M> {
    fn add_driver(&mut self, list: usize, actor: i32) -> usize {
        self.added.push(EventDriver::new(list, actor));
        self.added.len() - 1
    }
}

const TWO_PI: f32 = 6.2831855;

/// `VISpellEventDriver`: one event list running.
#[derive(Clone, Debug)]
pub struct EventDriver<M: Model> {
    list: usize,
    pub actor: i32,
    index: usize,
    wait: Option<Active<M>>,
    projectile: Option<Active<M>>,
    bolt0: Option<Lightning<M>>,
    bolt1: Option<Lightning<M>>,
    lightning: Option<Active<M>>,
    action: i32,
    anim: [i32; 4],
    active: Vec<Active<M>>,
    inactive: Vec<Active<M>>,
}

fn v3<M: Model>(m: &Vect4<M>) -> Vect3<M> {
    Vect3::new(m.x, m.y, m.z)
}

fn sub3<M: Model>(a: &Vect3<M>, b: &Vect3<M>) -> Vect3<M> {
    Vect3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn add3<M: Model>(a: &Vect3<M>, b: &Vect3<M>) -> Vect3<M> {
    Vect3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

fn scale3<M: Model>(a: &Vect3<M>, k: F<M>) -> Vect3<M> {
    Vect3::new(a.x * k, a.y * k, a.z * k)
}

fn cross<M: Model>(a: &Vect3<M>, b: &Vect3<M>) -> Vect3<M> {
    Vect3::new(a.y * b.z - a.z * b.y, -a.x * b.z + a.z * b.x, a.x * b.y - a.y * b.x)
}

/// `VIMatrix44::RotateHPR`
fn rotate_hpr<M: Model>(v: &Vect3<M>) -> Matrix44<M> {
    let (si, ci) = (v.x.sinf(), v.x.cosf());
    let (sj, cj) = (v.y.sinf(), v.y.cosf());
    let (sh, ch) = (v.z.sinf(), v.z.cosf());
    let z = F::ZERO;
    Matrix44 {
        m: [
            Vect4::new(ch * ci + sh * sj * si, sh * cj, -ch * si + sh * sj * ci, z),
            Vect4::new(-sh * ci + ch * sj * si, ch * cj, sh * si + ch * sj * ci, z),
            Vect4::new(cj * si, -sj, cj * ci, z),
            Vect4::new(z, z, z, F::ONE),
        ],
    }
}

/// `VICSprite::SetAttackAction` (alt false) / `SetAttackAltAction`: the
/// action a hand's attack plays, from the attack types of the two hands'
/// items; a two-handed weapon in the first hand decides for both. `None`
/// for a slot past the hands or a weapon with no swing.
pub fn attack_action(attack: [i32; 2], slot: i32, alt: bool) -> Option<i32> {
    if !(0..2).contains(&slot) {
        return None;
    }
    let first = slot == 0;
    let id = match (alt, attack[0]) {
        (false, 2) => 26,
        (false, 4) => 42,
        (false, 6) => 29,
        (true, 2) => 42,
        (true, 4) => 29,
        (true, 6) => 26,
        (true, 9) => 33,
        (false, _) => match attack[slot as usize] {
            11 => if first { 23 } else { 22 },
            1 => if first { 25 } else { 24 },
            3 => if first { 41 } else { 40 },
            5 => if first { 28 } else { 27 },
            7 => 31,
            9 => 33,
            8 => if first { 35 } else { 32 },
            10 => 36,
            _ => return None,
        },
        (true, _) => match attack[slot as usize] {
            1 => if first { 41 } else { 40 },
            3 => if first { 28 } else { 27 },
            5 | 11 => if first { 25 } else { 24 },
            7 => 31,
            8 => if first { 35 } else { 32 },
            10 => 36,
            _ => return None,
        },
    };
    Some(id)
}

/// `VIMatrix44::ConvertToEulerZXY` (`Matrix44::euler_zxy`).
pub fn to_euler_zxy<M: Model>(m: &Matrix44<M>) -> Vect3<M> {
    m.euler_zxy()
}

/// The two-draw signed unit random the projectile jitter uses.
fn jitter<M: Model>(rand: &mut Rand, k: f32) -> F<M> {
    let a = F::<M>::from_int(rand.rand());
    let b = F::<M>::from_int(rand.rand());
    (a - b) * F::new(4.656613e-10) * F::new(k)
}

/// Which actor an event's flags name: the caster (0x10000), the target
/// (0x20000), else the driver's own.
fn pick_actor<M: Model>(flags: i32, own: i32, cx: &Ctx<M>) -> i32 {
    let mut a = if flags & 0x10000 != 0 { cx.caster } else { own };
    if flags & 0x20000 != 0 {
        a = cx.target;
    }
    a
}

/// The actor's transform, or its node's when it is a CSprite and the flags
/// name one (`& 0xFFFF != 0xFFFF`).
fn flagged_transform<M: Model>(cx: &Ctx<M>, actor: i32, ty: i32, flags: i32) -> Matrix44<M> {
    if ty != 5 || flags & 0xFFFF == 0xFFFF { cx.scene.actor_transform(actor) } else { cx.scene.slot_world_transform(actor, flags & 0xFFFF) }
}

impl<M: Model> EventDriver<M> {
    fn new(list: usize, actor: i32) -> Self {
        EventDriver {
            list,
            actor,
            index: 0,
            wait: None,
            projectile: None,
            bolt0: None,
            bolt1: None,
            lightning: None,
            action: -1,
            anim: [-1; 4],
            active: Vec::new(),
            inactive: Vec::new(),
        }
    }

    /// `~VISpellEventDriver`: the active events, the inactive, the wait, the
    /// projectile and the bolts (the lightning event is not freed).
    fn destroy(self, scene: &mut dyn SpellScene<M>, ps: &mut System<M>) {
        for e in self.active {
            delete(e, scene, ps);
        }
        for e in self.inactive {
            delete(e, scene, ps);
        }
        if let Some(e) = self.wait {
            delete(e, scene, ps);
        }
        if let Some(e) = self.projectile {
            delete(e, scene, ps);
        }
    }

    fn find(&self, ty: i32, id: i32) -> Option<usize> {
        self.active.iter().position(|e| e.ev.p04 == id && e.ev.ty == ty)
    }

    /// `GetNodeTransform`: `None` (m untouched) for no actor or no sprite.
    fn node_transform(cx: &Ctx<M>, actor: i32, node: i32) -> Option<Matrix44<M>> {
        if actor == -1 {
            return None;
        }
        let ty = cx.scene.sprite_type(actor)?;
        Some(if ty != 5 { cx.scene.actor_transform(actor) } else { cx.scene.slot_world_transform(actor, node) })
    }

    pub fn particle_count(&self, ps: &System<M>) -> usize {
        self.active.iter().chain(&self.inactive).map(|e| e.particle_count(ps)).sum::<usize>() + self.projectile.as_ref().map_or(0, |e| e.particle_count(ps))
    }

    // ---- the new events -------------------------------------------------------

    fn new_wait(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) {
        let mut e = Active::new(ev);
        e.time = cx.scene.time();
        self.wait = Some(e);
    }

    fn new_vibrate(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) {
        if cx.player_actor != self.actor || !cx.vibrate {
            return;
        }
        let (mut power, mut time, mut mode) = (1u8, 100u8, 0);
        match ev.p04 {
            0 => {
                power = ev.id as u8;
                time = ev.p0c as u8;
                mode = ev.p10;
            }
            1 => {
                time = 80;
                mode = 200;
            }
            2 => {
                time = 220;
                mode = 140;
            }
            _ => {}
        }
        cx.scene.vibrate(power, time, mode);
    }

    fn new_create_emitter(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) {
        let mut e = Active::new(ev);
        e.time = cx.scene.time();
        if ev.id >= 0 {
            e.emitter0 = Some(cx.ps.create_emitter(ev.id as usize));
        }
        e.f9c = F::ZERO;
        e.fa0 = F::ZERO;
        self.active.push(e);
    }

    fn new_set_emitter_attractor(&mut self, ev: &SpellEvent<M>) {
        if let Some(k) = self.find(3, ev.p04) {
            self.active[k].p84 = ev.id;
            self.active[k].f88 = ev.f[0];
        }
    }

    fn new_destroy_emitter(&mut self, ev: &SpellEvent<M>) {
        if let Some(k) = self.find(3, ev.p04) {
            let e = self.active.remove(k);
            self.inactive.push(e);
        }
    }

    fn new_destroy(&mut self, ty: i32, ev: &SpellEvent<M>, cx: &mut Ctx<M>) {
        if let Some(k) = self.find(ty, ev.p04) {
            let e = self.active.remove(k);
            delete(e, cx.scene, cx.ps);
        }
    }

    fn reset_anims(&mut self, cx: &mut Ctx<M>) {
        self.anim = [-1; 4];
        for (k, name) in ["Spell FX 1", "Spell FX 2", "Spell FX 3", "Spell FX 4"].iter().enumerate() {
            if let Some(i) = cx.scene.find_trigger(self.actor, crate::hash::hash_resource_id(name)) {
                self.anim[k] = i;
            }
        }
        for k in 0..4 {
            if self.anim[k] != -1 {
                cx.scene.reset_trigger(self.actor, self.anim[k]);
            }
        }
    }

    fn new_set_action(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) {
        if self.actor == -1 {
            return;
        }
        let Some(ty) = cx.scene.sprite_type(self.actor) else { return };
        if ty != 5 {
            self.action = -1;
            self.anim = [-1; 4];
            return;
        }
        self.action = cx.scene.set_action(self.actor, ev.p04);
        self.reset_anims(cx);
    }

    fn new_play_attack(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) {
        if self.actor == -1 {
            return;
        }
        let Some(ty) = cx.scene.sprite_type(self.actor) else { return };
        if ty != 5 {
            self.action = -1;
            self.anim = [-1; 4];
            return;
        }
        match ev.p04 {
            0 => self.action = cx.scene.set_attack_action(self.actor, 0, false),
            1 => self.action = cx.scene.set_attack_action(self.actor, 1, false),
            2 => self.action = cx.scene.set_attack_action(self.actor, 0, true),
            3 => self.action = cx.scene.set_attack_action(self.actor, 1, true),
            _ => {}
        }
        let c = Color { r: ev.f[0], g: ev.f[1], b: ev.f[2], a: F::ONE };
        cx.scene.set_weapon_trail(self.actor, ev.id, c);
        self.reset_anims(cx);
    }

    fn new_shake_camera(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) {
        if cx.player_actor != self.actor {
            return;
        }
        if cx.camera {
            cx.camera_on = true;
            let shake = ev.p04 & 0x10000 != 0;
            let terp = ev.p04 & 0x20000 != 0;
            let speed = ev.id as u8;
            let (amp, mag, time) = (ev.f[0], ev.f[1], ev.f[2]);
            if shake {
                match speed {
                    0 => cx.scene.shake(0, F::ZERO),
                    1..=3 => cx.scene.shake(speed as i32, amp),
                    _ => {}
                }
            }
            if terp {
                cx.scene.shake_terp(mag, time);
            }
        }
    }

    fn new_play_target_instant(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) {
        if ev.p04 & 1 != 0 {
            cx.add_driver(TARGET, cx.target);
        }
        if ev.p04 & 2 != 0 {
            for i in 0..cx.secondary.len() {
                cx.add_driver(AREA, cx.secondary[i]);
            }
        }
    }

    fn new_set_emitter_motif(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) {
        let Some(ce) = self.find(3, ev.p04) else { return };
        let mut e = Active::new(ev);
        e.time = cx.scene.time();
        let old = self.active.iter().position(|x| {
            x.ev.ty == 6
                && x.ev.p04 == ev.p04
                && x.blender.is_some_and(|b| b.idx.is_some_and(|i| cx.ps.get(b.emitter).is_some_and(|em| em.blenders.get(i).and_then(|o| o.as_ref()).is_some_and(|bl| cx.ps.defs[em.def].as_ref().is_some_and(|d| d.motifs[bl.motif].name_hash == ev.id as u32)))))
        });
        match old {
            Some(k) => {
                // the blend in flight is taken over at the factor it reached
                let mut o = self.active.remove(k);
                e.f9c = o.blender_factor(cx.ps);
                e.blender = o.blender.take();
                self.inactive.push(o);
            }
            None => {
                let em = self.active[ce].emitter0.unwrap_or(usize::MAX);
                let idx = cx.ps.link_blender(em, ev.id as u32);
                e.blender = Some(Blend { emitter: em, idx, factor: F::ZERO });
                e.f9c = F::ZERO;
            }
        }
        self.active.push(e);
    }

    fn new_play_target_projectile(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) -> i32 {
        for i in 0..cx.secondary.len() + 1 {
            let mut d = None;
            if ev.p04 & 1 != 0 && i == 0 {
                d = Some(cx.add_driver(TARGET, cx.target));
            }
            if ev.p04 & 2 != 0 && i != 0 {
                d = Some(cx.add_driver(AREA, cx.secondary[i - 1]));
            }
            let Some(d) = d else { continue };
            if self.actor == -1 {
                continue;
            }
            let mut e = Active::new(ev);
            e.time = cx.scene.time();
            let ty = cx.scene.sprite_type(self.actor);
            let item = |cx: &Ctx<M>| -> i32 {
                if cx.scene.item_projectile(self.actor, 0) != -1 {
                    cx.scene.item_projectile(self.actor, 0)
                } else if cx.scene.item_projectile(self.actor, 1) != -1 {
                    cx.scene.item_projectile(self.actor, 1)
                } else {
                    -1
                }
            };
            if ty == Some(5) && (ev.p0c >> 12) & 1 != 0 {
                let s = item(cx);
                if s >= 0 {
                    e.actor0 = cx.scene.create_actor(Vect3::of(0.0, -8000.0, 0.0), Vect3::default(), F::ONE, s);
                    cx.scene.set_actor_collideable(e.actor0, false);
                }
            }
            if ev.id >= 0 {
                e.emitter0 = Some(cx.ps.create_emitter(ev.id as usize));
            }
            match (ev.p0c >> 8) & 0xF {
                1 => {
                    e.f9c = if ev.p0c & 0xFF == 3 { F::ZERO } else { F::<M>::from_int(cx.rand.rand()) / F::new(5347.0) };
                    e.fa0 = F::ZERO;
                }
                2 => e.f9c = F::ONE,
                _ => {}
            }
            if ty != Some(5) {
                e.v14 = cx.scene.actor_location(self.actor);
                e.c[0] = Vect3::default();
            } else {
                let node = |cx: &Ctx<M>, n: i32| v3(&Self::node_transform(cx, self.actor, n).unwrap_or(Matrix44 { m: Default::default() }).m[3]);
                match ev.p0c as u8 {
                    0 => {
                        e.v14 = node(cx, 0);
                        e.c[0] = Vect3::default();
                    }
                    1 => {
                        e.v14 = node(cx, 1);
                        e.c[0] = Vect3::default();
                    }
                    2 => {
                        let a = node(cx, 0);
                        let b = node(cx, 1);
                        e.v14 = scale3(&add3(&a, &b), F::new(0.5));
                        e.c[0] = Vect3::default();
                    }
                    3 => {
                        let a = node(cx, 0);
                        let b = node(cx, 1);
                        e.v14 = scale3(&add3(&a, &b), F::new(0.5));
                        let m = cx.scene.actor_transform(self.actor);
                        let mut m2 = transpose(&m);
                        let w = (m2.m[0].w, m2.m[1].w, m2.m[2].w);
                        m2.m[3].x = -(w.0 * m2.m[0].x + w.1 * m2.m[1].x + w.2 * m2.m[2].x);
                        m2.m[3].y = -(w.0 * m2.m[0].y + w.1 * m2.m[1].y + w.2 * m2.m[2].y);
                        m2.m[3].z = -(w.0 * m2.m[0].z + w.1 * m2.m[1].z + w.2 * m2.m[2].z);
                        m2.m[0].w = F::ZERO;
                        m2.m[1].w = F::ZERO;
                        m2.m[2].w = F::ZERO;
                        e.a[0] = m2.mul_point(&a);
                        e.a[1] = m2.mul_point(&b);
                        let c = m2.mul_point(&e.v14);
                        e.a[0] = sub3(&e.a[0], &c);
                        e.a[1] = sub3(&e.a[1], &c);
                        if (ev.p0c >> 12) & 1 != 0 {
                            let s = item(cx);
                            if s >= 0 {
                                e.actor1 = cx.scene.create_actor(Vect3::of(0.0, -8000.0, 0.0), Vect3::default(), F::ONE, s);
                                cx.scene.set_actor_collideable(e.actor1, false);
                            }
                        }
                        if ev.id >= 0 {
                            e.emitter1 = Some(cx.ps.create_emitter(ev.id as usize));
                        }
                        e.c = e.a;
                    }
                    _ => {}
                }
            }
            e.v20 = e.v14;
            cx.added[d].projectile = Some(e);
        }
        0
    }

    fn new_create_sound(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) {
        if self.actor == -1 {
            return;
        }
        let mut e = Active::new(ev);
        e.time = cx.scene.time();
        e.actor0 = cx.scene.create_actor(Vect3::default(), Vect3::default(), ev.f[0], ev.id);
        let att = if ev.p0c & 0x80000 != 0 {
            1
        } else if ev.p0c & 0x100000 != 0 {
            2
        } else {
            0
        };
        cx.scene.sound_settings(e.actor0, ev.p0c & 0x40000 != 0, att);
        self.active.push(e);
        let k = self.active.len() - 1;
        self.process_create_sound(k, true, cx);
    }

    fn new_create_sprite(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) -> i32 {
        if self.actor == -1 {
            return 0;
        }
        let mut e = Active::new(ev);
        e.time = cx.scene.time();
        let sprite = if cx.scene.sprite_kind(ev.id) == 4 {
            let s = cx.scene.copy_sprite(ev.id);
            if s < 0 {
                return -1;
            }
            s
        } else {
            ev.id
        };
        if cx.scene.sprite_kind(sprite) == 4 {
            let mode = if ev.p0c & 0x40000 != 0 {
                1
            } else if ev.p0c & 0x80000 != 0 {
                2
            } else {
                0
            };
            cx.scene.restart_hsprite(sprite, mode);
        }
        e.actor0 = cx.scene.create_actor(Vect3::default(), Vect3::default(), F::ONE, sprite);
        cx.scene.set_actor_collideable(e.actor0, false);
        self.active.push(e);
        let k = self.active.len() - 1;
        self.process_create_sprite(k, true, cx);
        0
    }

    fn new_play_target_lightning(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) {
        for i in 0..cx.secondary.len() + 1 {
            let mut d = None;
            if ev.p04 & 1 != 0 && i == 0 {
                d = Some(cx.add_driver(TARGET, cx.target));
            }
            if ev.p04 & 2 != 0 && i != 0 {
                d = Some(cx.add_driver(AREA, cx.secondary[i - 1]));
            }
            if let Some(d) = d
                && self.actor != -1
                && cx.added[d].lightning.is_none()
            {
                let mut e = Active::new(ev);
                e.actor0 = self.actor;
                e.time = cx.scene.time();
                cx.added[d].lightning = Some(e);
            }
        }
    }

    fn new_fade_sprite(&mut self, ev: &SpellEvent<M>, cx: &mut Ctx<M>) {
        let Some(ce) = self.find(9, ev.p04) else { return };
        let mut e = Active::new(ev);
        e.time = cx.scene.time();
        let actor = self.active[ce].actor0;
        if let Some(a) = cx.scene.fade_begin(actor) {
            e.f9c = F::<M>::from_int(a as i32) / F::new(255.0);
            self.active.push(e);
        }
    }

    // ---- the running events -----------------------------------------------------

    fn elapsed(cx: &Ctx<M>, since: i64) -> i32 {
        cx.scene.time().wrapping_sub(since) as i32
    }

    /// `ProcessCreateEmitter`: placed at its actor (or node, or an orbit
    /// round it), aimed along it, updated; the attractor; expired into the
    /// inactive list. Returns whether it is still where it was.
    fn process_create_emitter(e: &mut Active<M>, own: i32, active: bool, cx: &mut Ctx<M>) -> bool {
        let ev = e.ev;
        if own != -1 {
            if active {
                let actor = pick_actor(ev.p0c, own, cx);
                if actor == -1 {
                    return Self::expire(e, active, cx);
                }
                if let Some(ty) = cx.scene.sprite_type(actor) {
                    let m2 = flagged_transform(cx, actor, ty, ev.p0c);
                    let shape = (ev.p0c >> 20) & 3;
                    let pos = if shape != 0 {
                        let angle = e.f9c;
                        let t = F::<M>::new((e.fa0.0 as f64).sin() as f32) * F::new(0.5) + F::new(0.5);
                        let r = ev.f[0] + (ev.f[1] - ev.f[0]) * t;
                        let (m, v) = match shape {
                            1 => (rotate_hpr(&Vect3::new(F::ZERO, angle, F::ZERO)), Vect3::new(r, ev.f[4], F::ZERO)),
                            2 => (rotate_hpr(&Vect3::new(angle, F::ZERO, F::ZERO)), Vect3::new(F::ZERO, r, ev.f[4])),
                            _ => (rotate_hpr(&Vect3::new(F::ZERO, F::ZERO, angle)), Vect3::new(ev.f[4], F::ZERO, r)),
                        };
                        let off = m.mul_point(&v);
                        let dt = cx.ps.delta_time;
                        e.f9c = e.f9c + ev.f[3] * F::new(2.0) * dt;
                        e.fa0 = e.fa0 + ev.f[2] * dt;
                        let tp = F::new(TWO_PI);
                        if tp < e.f9c {
                            e.f9c = e.f9c - tp;
                        } else if e.f9c < -tp {
                            e.f9c = e.f9c + tp;
                        }
                        if tp < e.fa0 {
                            e.fa0 = e.fa0 - tp;
                        } else if e.fa0 < -tp {
                            e.fa0 = e.fa0 + tp;
                        }
                        m2.mul_point(&off)
                    } else {
                        v3(&m2.m[3])
                    };
                    let r = Matrix33 { m: [v3(&m2.m[0]), v3(&m2.m[1]), v3(&m2.m[2])] };
                    if let Some(id) = e.emitter0 {
                        if let Some(em) = cx.ps.get_mut(id) {
                            em.set_location(pos);
                            em.direction = r;
                        }
                        cx.ps.update(id, cx.rand);
                    }
                }
            }
            if e.p84 != -1 {
                let actor = pick_actor(e.p84, own, cx);
                if let Some(ty) = cx.scene.sprite_type(actor) {
                    let m3 = flagged_transform(cx, actor, ty, e.p84);
                    if let Some(em) = e.emitter0.and_then(|id| cx.ps.get_mut(id)) {
                        em.attractor = Some((v3(&m3.m[3]), e.f88));
                    }
                }
            } else if let Some(em) = e.emitter0.and_then(|id| cx.ps.get_mut(id)) {
                em.attractor = None;
            }
        }
        Self::expire(e, active, cx)
    }

    /// The common tail: past its duration an event is done (true).
    fn expire(e: &Active<M>, active: bool, cx: &Ctx<M>) -> bool {
        active && e.ev.p10 != 0 && e.ev.p10 < Self::elapsed(cx, e.time)
    }

    fn process_create_sound(&mut self, k: usize, active: bool, cx: &mut Ctx<M>) -> bool {
        let ev = self.active[k].ev;
        if self.actor != -1 && active {
            let actor = pick_actor(ev.p0c, self.actor, cx);
            if actor != -1
                && let Some(ty) = cx.scene.sprite_type(actor)
            {
                let m = flagged_transform(cx, actor, ty, ev.p0c);
                cx.scene.set_actor_location(self.active[k].actor0, v3(&m.m[3]));
            }
        }
        if Self::expire(&self.active[k], active, cx) {
            let e = self.active.remove(k);
            delete(e, cx.scene, cx.ps);
            return true;
        }
        false
    }

    fn process_create_sprite(&mut self, k: usize, active: bool, cx: &mut Ctx<M>) -> bool {
        let ev = self.active[k].ev;
        if self.actor != -1 && active {
            let actor = pick_actor(ev.p0c, self.actor, cx);
            if actor != -1
                && let Some(ty) = cx.scene.sprite_type(actor)
            {
                let m = flagged_transform(cx, actor, ty, ev.p0c);
                let hpr = to_euler_zxy(&m);
                let a0 = self.active[k].actor0;
                cx.scene.set_actor_location(a0, v3(&m.m[3]));
                cx.scene.set_actor_hpr(a0, hpr);
            }
        }
        if Self::expire(&self.active[k], active, cx) {
            let e = self.active.remove(k);
            delete(e, cx.scene, cx.ps);
            return true;
        }
        false
    }

    fn process_set_emitter_motif(&mut self, k: usize, cx: &mut Ctx<M>) -> bool {
        let ev = self.active[k].ev;
        let el = Self::elapsed(cx, self.active[k].time);
        let e = &mut self.active[k];
        if ev.p0c < el {
            if e.blender_factor(cx.ps) != ev.f[0] {
                e.set_blender_factor(cx.ps, ev.f[0]);
            }
        } else {
            let mut f = F::<M>::from_int(el) / F::from_int(ev.p0c);
            f = e.f9c + (ev.f[0] - e.f9c) * f;
            let f = if F::ZERO > f { F::ZERO } else { f };
            let f = if f < F::ONE { f } else { F::ONE };
            e.set_blender_factor(cx.ps, f);
        }
        if !e.blender_motif(cx.ps) {
            let e = self.active.remove(k);
            delete(e, cx.scene, cx.ps);
            return true;
        }
        false
    }

    fn process_fade_sprite(&mut self, k: usize, cx: &mut Ctx<M>) -> bool {
        let ev = self.active[k].ev;
        let el = Self::elapsed(cx, self.active[k].time);
        let f = if ev.id < el {
            ev.f[0]
        } else {
            let e = &self.active[k];
            let mut f = F::<M>::from_int(el) / F::from_int(ev.id);
            f = e.f9c + (ev.f[0] - e.f9c) * f;
            let f = if F::ZERO > f { F::ZERO } else { f };
            if f < F::ONE { f } else { F::ONE }
        };
        if let Some(ce) = self.find(9, ev.p04) {
            let a = (f * F::new(255.0)).0 as u8;
            cx.scene.fade_set(self.active[ce].actor0, a);
        }
        if ev.id < Self::elapsed(cx, self.active[k].time) {
            let e = self.active.remove(k);
            delete(e, cx.scene, cx.ps);
            return true;
        }
        false
    }

    fn process_inactive(&mut self, cx: &mut Ctx<M>) {
        let mut k = 0;
        while k < self.inactive.len() {
            let e = &self.inactive[k];
            if e.emitter0.is_some_and(|x| cx.ps.particle_count(x) > 0) {
                if e.ev.ty == 3 {
                    let mut e = self.inactive[k].clone();
                    Self::process_create_emitter(&mut e, self.actor, false, cx);
                    self.inactive[k] = e;
                }
                k += 1;
            } else {
                let e = self.inactive.remove(k);
                delete(e, cx.scene, cx.ps);
            }
        }
    }

    /// `ProcessWaitEvent`
    fn process_wait(&mut self, cx: &mut Ctx<M>) {
        let Some(w) = &self.wait else { return };
        let ev = w.ev;
        let wt = w.time;
        let done = match ev.p04 {
            0 => !(self.actor != -1 && cx.scene.sprite_type(self.actor) == Some(5) && self.action != -1 && !cx.scene.is_action_complete(self.actor, self.action)),
            1 => ev.id < Self::elapsed(cx, wt),
            2 => cx.arg0 < Self::elapsed(cx, cx.trigger_time),
            3 => {
                if self.actor == -1 {
                    true
                } else {
                    match cx.scene.sprite_type(self.actor) {
                        None => true,
                        Some(ty) => {
                            let mut gone = false;
                            if ty != 5 {
                                gone = true;
                            } else {
                                let n = self.anim.get(ev.id as usize).copied().unwrap_or(-1);
                                if n != -1 && cx.scene.has_tripped(self.actor, n) {
                                    gone = true;
                                }
                            }
                            gone || ev.p0c < Self::elapsed(cx, wt)
                        }
                    }
                }
            }
            4 => cx.arg1 < Self::elapsed(cx, cx.trigger_time),
            _ => false,
        };
        if done {
            self.wait = None;
        }
    }

    /// `ProcessLightningEvent`: the bolt (or bolt pair) from the source actor's
    /// node(s) to the midpoint of this driver's actor's two nodes, recharged
    /// while the event lasts; it goes once a bolt has faded past its time.
    fn process_lightning(&mut self, cx: &mut Ctx<M>) -> i32 {
        let Some(l) = &self.lightning else { return 0 };
        let ev = l.ev;
        let actor = l.actor0;
        let lt = l.time;
        let mut alive = [true, true];
        if self.actor == -1 || actor == -1 {
            alive = [false, false];
        } else {
            if cx.scene.sprite_type(self.actor).is_none() {
                return -1;
            }
            let mut m = Matrix44 { m: Default::default() };
            let mut node = |cx: &Ctx<M>, a: i32, n: i32| {
                if let Some(x) = Self::node_transform(cx, a, n) {
                    m = x;
                }
                v3(&m.m[3])
            };
            let mut src = [Vect3::default(); 2];
            match ev.id as u8 {
                0 => src[0] = node(cx, actor, 0),
                1 => src[0] = node(cx, actor, 1),
                2 => {
                    let a = node(cx, actor, 0);
                    let b = node(cx, actor, 1);
                    let h = F::new(0.5);
                    src[0] = Vect3::new((a.x + b.x) * h, (a.y + b.y) * h, (a.z + b.z) * h);
                }
                3 => {
                    src[0] = node(cx, actor, 0);
                    src[1] = node(cx, actor, 1);
                }
                _ => {}
            }
            let a = node(cx, self.actor, 0);
            let b = node(cx, self.actor, 1);
            let h = F::new(0.5);
            let dst = Vect3::new((a.x + b.x) * h, (a.y + b.y) * h, (a.z + b.z) * h);
            if self.bolt0.is_none() {
                self.bolt0 = Some(Lightning::new(cx.rand));
            }
            if ev.id as u8 == 3 && self.bolt1.is_none() {
                self.bolt1 = Some(Lightning::new(cx.rand));
            }
            if Self::elapsed(cx, lt) <= ev.p0c {
                self.bolt0.as_mut().unwrap().set_charge(F::ONE);
                if let Some(b) = self.bolt1.as_mut() {
                    b.set_charge(F::ONE);
                }
            }
            let dt = (cx.ps.delta_time * F::new(1000.0)).0 as i32;
            let c = Color { r: ev.f[0], g: ev.f[1], b: ev.f[2], a: F::ONE };
            let b0 = self.bolt0.as_mut().unwrap();
            b0.set_color(c);
            b0.set_location(&src[0], &dst);
            b0.set_max_branch_count(ev.p10);
            b0.update(dt, cx.rand);
            cx.bolts.extend(b0.render());
            alive[0] = F::ZERO < b0.charge();
            if let Some(b1) = self.bolt1.as_mut() {
                b1.set_color(c);
                b1.set_location(&src[1], &dst);
                b1.set_max_branch_count(ev.p10);
                b1.update(dt, cx.rand);
                cx.bolts.extend(b1.render());
                alive[1] = F::ZERO < b1.charge();
            }
        }
        if ev.p0c < Self::elapsed(cx, lt) && (!alive[0] || !alive[1]) {
            self.lightning = None;
            self.bolt0 = None;
            self.bolt1 = None;
        }
        0
    }

    /// `ProcessProjectileEvent`: one frame of flight toward this driver's
    /// actor's node 4.
    fn process_projectile(&mut self, cx: &mut Ctx<M>) -> i32 {
        let Some(mut e) = self.projectile.take() else { return 0 };
        let mut done = false;
        let mut dist = F::ZERO;
        let mut dist1 = F::ZERO;
        if self.actor == -1 {
            done = true;
        } else {
            if cx.scene.sprite_type(self.actor).is_none() {
                self.projectile = Some(e);
                return -1;
            }
            let m = Self::node_transform(cx, self.actor, 4).unwrap_or(Matrix44 { m: Default::default() });
            let tgt = v3(&m.m[3]);
            let mut p0 = tgt;
            let mut p1 = Vect3::default();
            let a = sub3(&e.v14, &tgt);
            let total = sub3(&tgt, &e.v20).magnitude();
            dist = a.magnitude();
            let mut d = sub3(&tgt, &e.v14);
            let mut fwd = d;
            let mut up = Vect3::of(0.0, 1.0, 0.0);
            let mut right = cross(&up, &fwd);
            up = cross(&fwd, &right);
            right.normalize();
            up.normalize();
            fwd.normalize();
            let z = F::ZERO;
            let m = Matrix44 {
                m: [Vect4::new(right.x, right.y, right.z, z), Vect4::new(up.x, up.y, up.z, z), Vect4::new(fwd.x, fwd.y, fwd.z, z), Vect4::new(e.v14.x, e.v14.y, e.v14.z, F::ONE)],
            };
            let zz = dist * F::new(0.6);
            let scale = if zz < F::ONE { zz } else { F::ONE };
            let dts = cx.ps.delta_time;
            let two = e.emitter1.is_some() || e.actor1 >= 0;
            match (e.ev.p0c >> 8) & 0xF {
                0 => {
                    if two {
                        p0 = m.mul_point(&scale3(&e.a[0], scale));
                        p1 = m.mul_point(&scale3(&e.a[1], scale));
                    } else {
                        p0 = e.v14;
                    }
                }
                1 => {
                    let m2 = rotate_hpr(&Vect3::new(F::ZERO, F::ZERO, e.f9c));
                    e.f9c = e.f9c + e.ev.f[2] * F::new(5.0) * dts;
                    if two {
                        let q0 = m2.mul_point(&scale3(&e.a[0], scale));
                        let q1 = m2.mul_point(&scale3(&e.a[1], scale));
                        p0 = m.mul_point(&q0);
                        p1 = m.mul_point(&q1);
                    } else {
                        e.fa0 = e.fa0 + dts * F::new(3.0);
                        e.fa0 = if e.fa0 < e.ev.f[1] { e.fa0 } else { e.ev.f[1] };
                        let q = Vect3::new(e.fa0 * scale, F::ZERO, F::ZERO);
                        p1 = m2.mul_point(&q);
                        p0 = m.mul_point(&p1);
                    }
                }
                2 => {
                    let lerp = |c: &Vect3<M>, b: &Vect3<M>, t: F<M>| {
                        let q = sub3(c, b);
                        let q = scale3(&q, t);
                        add3(&q, b)
                    };
                    if two {
                        if F::ONE <= e.f9c {
                            e.b = e.c;
                            let k = e.ev.f[1];
                            for s in 0..2 {
                                let x = e.a[s].x + jitter(cx.rand, 0.2);
                                let y = e.a[s].y + jitter(cx.rand, 0.2);
                                let zq = e.a[s].z + jitter(cx.rand, 0.05);
                                e.c[s] = Vect3::new(x, y, zq);
                            }
                            e.c[0] = scale3(&e.c[0], k);
                            e.c[1] = scale3(&e.c[1], k);
                            e.f9c = F::ZERO;
                        }
                        let q0 = scale3(&lerp(&e.c[0], &e.b[0], e.f9c), scale);
                        let q1 = scale3(&lerp(&e.c[1], &e.b[1], e.f9c), scale);
                        e.f9c = e.f9c + dts * F::new(15.0);
                        p0 = m.mul_point(&q0);
                        p1 = m.mul_point(&q1);
                    } else {
                        if F::ONE <= e.f9c {
                            e.b[0] = e.c[0];
                            let x = jitter(cx.rand, 0.2);
                            let y = jitter(cx.rand, 0.2);
                            let zq = jitter(cx.rand, 0.05);
                            e.c[0] = scale3(&Vect3::new(x, y, zq), e.ev.f[1]);
                            e.f9c = F::ZERO;
                        }
                        p1 = scale3(&lerp(&e.c[0], &e.b[0], e.f9c), scale);
                        e.f9c = e.f9c + dts * F::new(15.0);
                        p0 = m.mul_point(&p1);
                    }
                }
                3 => {
                    if two {
                        p0 = m.mul_point(&scale3(&e.a[0], scale));
                        p1 = m.mul_point(&scale3(&e.a[1], scale));
                    } else {
                        p0 = e.v14;
                    }
                    let inv = F::ONE / e.ev.f[0];
                    let h = (sub3(&e.v14, &e.v20).magnitude() / total * F::new(3.1415927)).sinf() * inv * inv * total * total;
                    p0.y = p0.y + h;
                    p1.y = p1.y + h;
                }
                _ => {}
            }
            if let Some(id) = e.emitter0 {
                if let Some(em) = cx.ps.get_mut(id) {
                    em.set_location(p0);
                    em.set_direction_vec(&d);
                }
                cx.ps.update(id, cx.rand);
            }
            if let Some(id) = e.emitter1 {
                if let Some(em) = cx.ps.get_mut(id) {
                    em.set_location(p1);
                    em.set_direction_vec(&d);
                }
                cx.ps.update(id, cx.rand);
            }
            let mut hpr = Vect3::default();
            if e.actor0 != -1 || e.actor1 != -1 {
                let mut f2 = if (e.ev.p0c >> 8) & 0xF == 3 {
                    let mut f2 = d;
                    f2.normalize();
                    f2 = scale3(&f2, e.ev.f[0]);
                    f2 = scale3(&f2, dts);
                    f2 = scale3(&f2, F::new(2.0));
                    let mut next = add3(&e.v14, &f2);
                    let inv = F::ONE / e.ev.f[0];
                    let h = (sub3(&next, &e.v20).magnitude() / total * F::new(3.1415927)).sinf() * inv * inv * total * total;
                    next.y = next.y + h;
                    sub3(&next, &p0)
                } else {
                    d
                };
                let mut u2 = Vect3::of(0.0, 1.0, 0.0);
                let mut r2 = cross(&u2, &f2);
                u2 = cross(&f2, &r2);
                r2.normalize();
                u2.normalize();
                f2.normalize();
                let mut m3 = Matrix44::identity();
                m3.m[0] = Vect4::new(r2.x, r2.y, r2.z, m3.m[0].w);
                m3.m[1] = Vect4::new(u2.x, u2.y, u2.z, m3.m[1].w);
                m3.m[2] = Vect4::new(f2.x, f2.y, f2.z, m3.m[2].w);
                hpr = to_euler_zxy(&m3);
            }
            if e.actor0 >= 0 {
                cx.scene.set_actor_transform(e.actor0, hpr, F::ONE, p0);
            }
            if e.actor1 >= 0 {
                cx.scene.set_actor_transform(e.actor1, hpr, F::ONE, p1);
            }
            d.normalize();
            d = scale3(&d, e.ev.f[0]);
            d = scale3(&d, dts);
            e.v14 = add3(&e.v14, &d);
            dist1 = sub3(&e.v14, &tgt).magnitude();
        }
        if done || (dist <= dist1 && dist1 < F::new(3.0)) {
            self.inactive.push(e);
        } else {
            self.projectile = Some(e);
        }
        0
    }

    /// `VISpellEventDriver::Process`: false once the list is done.
    fn process(&mut self, cx: &mut Ctx<M>) -> bool {
        if self.projectile.is_some() {
            self.process_projectile(cx);
            if self.projectile.is_some() {
                return true;
            }
        }
        if self.lightning.is_some() {
            self.process_lightning(cx);
        }
        if self.wait.is_some() {
            self.process_wait(cx);
        }
        self.process_inactive(cx);
        let mut k = 0;
        while k < self.active.len() {
            let gone = match self.active[k].ev.ty {
                3 => {
                    let mut e = self.active[k].clone();
                    let done = Self::process_create_emitter(&mut e, self.actor, true, cx);
                    if done {
                        self.active.remove(k);
                        self.inactive.push(e);
                    } else {
                        self.active[k] = e;
                    }
                    done
                }
                11 => self.process_create_sound(k, true, cx),
                9 => self.process_create_sprite(k, true, cx),
                6 => self.process_set_emitter_motif(k, cx),
                14 => self.process_fade_sprite(k, cx),
                _ => false,
            };
            if !gone {
                k += 1;
            }
        }
        let list = &cx.effect.lists[self.list];
        if self.wait.is_none() && self.index < list.len() {
            let ev = list[self.index];
            match ev.ty {
                1 => self.new_wait(&ev, cx),
                2 => self.new_set_action(&ev, cx),
                3 => self.new_create_emitter(&ev, cx),
                6 => self.new_set_emitter_motif(&ev, cx),
                5 => self.new_set_emitter_attractor(&ev),
                4 => self.new_destroy_emitter(&ev),
                7 => self.new_play_target_instant(&ev, cx),
                8 => {
                    self.new_play_target_projectile(&ev, cx);
                }
                11 => self.new_create_sound(&ev, cx),
                12 => self.new_destroy(11, &ev, cx),
                9 => {
                    self.new_create_sprite(&ev, cx);
                }
                10 => self.new_destroy(9, &ev, cx),
                13 => self.new_play_target_lightning(&ev, cx),
                14 => self.new_fade_sprite(&ev, cx),
                15 => self.new_play_attack(&ev, cx),
                16 => self.new_vibrate(&ev, cx),
                17 => self.new_shake_camera(&ev, cx),
                _ => {}
            }
            self.index += 1;
        }
        !(self.index == cx.effect.lists[self.list].len() && self.active.is_empty() && self.inactive.is_empty() && self.wait.is_none() && self.lightning.is_none())
    }
}

fn transpose<M: Model>(a: &Matrix44<M>) -> Matrix44<M> {
    let r = |i: usize| [a.m[i].x, a.m[i].y, a.m[i].z, a.m[i].w];
    let rows = [r(0), r(1), r(2), r(3)];
    let c = |j: usize| Vect4::new(rows[0][j], rows[1][j], rows[2][j], rows[3][j]);
    Matrix44 { m: [c(0), c(1), c(2), c(3)] }
}

/// `VISpellEffectDriver`: one cast in flight.
#[derive(Clone, Debug)]
pub struct EffectDriver<M: Model> {
    pub caster: i32,
    pub target: i32,
    secondary: Vec<i32>,
    trigger_time: i64,
    arg0: i32,
    arg1: i32,
    drivers: Vec<EventDriver<M>>,
    player_actor: i32,
    camera_on: bool,
}

impl<M: Model> EffectDriver<M> {
    fn notify_actor_destroyed(&mut self, actor: i32, scene: &mut dyn SpellScene<M>, camera: bool) {
        if actor == self.player_actor && camera {
            scene.camera_reset();
        }
        if self.caster == actor {
            self.caster = -1;
        }
        if self.target == actor {
            self.target = -1;
        }
        for s in &mut self.secondary {
            if *s == actor {
                *s = -1;
            }
        }
        for d in &mut self.drivers {
            if d.actor == actor {
                d.actor = -1;
            }
        }
    }

    /// `Destroy`: the camera effect reset if a list was still shaking it,
    /// then every list freed.
    fn destroy(self, scene: &mut dyn SpellScene<M>, ps: &mut System<M>, camera: bool) {
        if !self.drivers.is_empty() && self.camera_on && camera {
            scene.camera_reset();
        }
        for d in self.drivers {
            d.destroy(scene, ps);
        }
    }

    pub fn particle_count(&self, ps: &System<M>) -> usize {
        self.drivers.iter().map(|d| d.particle_count(ps)).sum()
    }
}

struct Pbi<M: Model> {
    effect: usize,
    handle: i32,
    driver: Option<EffectDriver<M>>,
    start: i64,
}

/// `VISpellEffectSystem`: the loaded effects and twenty-four playbacks.
pub struct SpellSystem<M: Model> {
    pub effects: Vec<SpellEffect<M>>,
    slots: [Option<usize>; 24],
    pbis: Vec<Pbi<M>>,
    /// playbacks in list order (`m_active`), and the free ones
    active: Vec<usize>,
    free: std::collections::VecDeque<usize>,
    serial: i32,
    pub player_actor: i32,
    /// a vibrate callback and a camera effect are set
    pub vibrate: bool,
    pub camera: bool,
    /// this frame's lightning, as `VILightning::Render` drew it
    pub bolts: Vec<BoltDraw<M>>,
}

impl<M: Model> Default for SpellSystem<M> {
    fn default() -> Self {
        SpellSystem {
            effects: Vec::new(),
            slots: [None; 24],
            pbis: (0..24).map(|_| Pbi { effect: 0, handle: 0, driver: None, start: 0 }).collect(),
            active: Vec::new(),
            free: (0..24).collect(),
            serial: 0x16062,
            player_actor: -1,
            vibrate: false,
            camera: false,
            bolts: Vec::new(),
        }
    }
}

impl<M: Model> SpellSystem<M> {
    fn playback(&self, handle: i32) -> Option<usize> {
        let p = self.slots[((handle & 0xff) % 24) as usize]?;
        (self.pbis[p].handle == handle).then_some(p)
    }

    pub fn is_playing(&self, handle: i32) -> bool {
        self.playback(handle).is_some()
    }

    /// `Play(effect, a, caster, target, secondary)`: a handle, -1 when it
    /// cannot. `a` is the cast time the wait kind 2 measures against.
    pub fn play(&mut self, effect: usize, a: i32, caster: i32, target: i32, secondary: &[i32], scene: &mut dyn SpellScene<M>) -> i32 {
        if caster == -1 || target == -1 || effect >= self.effects.len() {
            return -1;
        }
        let mut handle = -1;
        let mut pb = 0;
        for i in 0..24 {
            if self.slots[i].is_none() {
                handle = i as i32 | (self.serial.wrapping_shl(8) & 0x7fffff00);
                self.serial = self.serial.wrapping_add(1);
                let Some(p) = self.free.pop_front() else { return -1 };
                self.active.push(p);
                self.slots[i] = Some(p);
                pb = p;
                break;
            }
        }
        if handle == -1 {
            return -1;
        }
        let mut sec: Vec<i32> = Vec::new();
        for &s in secondary.iter().take(8) {
            if s == -1 {
                break;
            }
            sec.push(s);
        }
        let now = scene.time();
        let mut d = EffectDriver { caster, target, secondary: sec, trigger_time: 0, arg0: 0, arg1: 0, drivers: Vec::new(), player_actor: self.player_actor, camera_on: false };
        // Trigger(a, 10000)
        if d.drivers.is_empty() && d.caster != -1 {
            d.trigger_time = now;
            d.arg0 = a;
            d.arg1 = 10000;
            d.drivers.push(EventDriver::new(CASTER, caster));
        }
        self.pbis[pb] = Pbi { effect, handle, driver: Some(d), start: now };
        handle
    }

    pub fn stop(&mut self, handle: i32, scene: &mut dyn SpellScene<M>, ps: &mut System<M>) {
        let Some(p) = self.playback(handle) else { return };
        if let Some(d) = self.pbis[p].driver.take() {
            d.destroy(scene, ps, self.camera);
        }
        self.slots[((self.pbis[p].handle & 0xff) % 24) as usize] = None;
        self.active.retain(|&x| x != p);
        self.free.push_back(p);
    }

    pub fn notify_actor_destroyed(&mut self, actor: i32, scene: &mut dyn SpellScene<M>) {
        let camera = self.camera;
        for &p in &self.active {
            if let Some(d) = self.pbis[p].driver.as_mut() {
                d.notify_actor_destroyed(actor, scene, camera);
            }
        }
    }

    pub fn stop_if_this_was_caster(&mut self, actor: i32, scene: &mut dyn SpellScene<M>, ps: &mut System<M>) {
        let list = self.active.clone();
        for p in list {
            if self.pbis[p].driver.as_ref().is_some_and(|d| d.caster == actor) {
                let h = self.pbis[p].handle;
                self.stop(h, scene, ps);
            }
        }
    }

    pub fn particle_count(&self, ps: &System<M>) -> usize {
        self.active.iter().filter_map(|&p| self.pbis[p].driver.as_ref()).map(|d| d.particle_count(ps)).sum()
    }

    /// `Process`: every playback one frame; one that is done, or has run for a
    /// minute, is stopped. The frame's lightning is in `bolts` after it.
    pub fn process(&mut self, scene: &mut dyn SpellScene<M>, ps: &mut System<M>, rand: &mut Rand) {
        self.bolts.clear();
        let mut k = 0;
        while k < self.active.len() {
            let p = self.active[k];
            let next = self.active.get(k + 1).copied();
            let Some(mut d) = self.pbis[p].driver.take() else {
                k += 1;
                continue;
            };
            let live = Self::drive(&mut d, &self.effects[self.pbis[p].effect], scene, ps, rand, self.vibrate, self.camera, &mut self.bolts);
            self.pbis[p].driver = Some(d);
            let h = self.pbis[p].handle;
            if !live {
                self.stop(h, scene, ps);
            } else {
                let el = scene.time().wrapping_sub(self.pbis[p].start) as i32;
                if el > 60000 {
                    self.stop(h, scene, ps);
                }
            }
            k = match next {
                Some(n) => self.active.iter().position(|&a| a == n).unwrap_or(self.active.len()),
                None => self.active.len(),
            };
        }
    }

    /// `VISpellEffectDriver::Process`: each list in order (a list added by
    /// one runs this same frame unless its adder was the last), a finished
    /// one freed. True while any is left.
    #[allow(clippy::too_many_arguments)]
    fn drive(d: &mut EffectDriver<M>, effect: &SpellEffect<M>, scene: &mut dyn SpellScene<M>, ps: &mut System<M>, rand: &mut Rand, vibrate: bool, camera: bool, bolts: &mut Vec<BoltDraw<M>>) -> bool {
        let mut i = 0;
        while i < d.drivers.len() {
            let last = i + 1 == d.drivers.len();
            let secondary = d.secondary.clone();
            let mut cx = Ctx {
                scene: &mut *scene,
                ps: &mut *ps,
                rand: &mut *rand,
                effect,
                caster: d.caster,
                target: d.target,
                secondary: &secondary,
                trigger_time: d.trigger_time,
                arg0: d.arg0,
                arg1: d.arg1,
                player_actor: d.player_actor,
                camera,
                camera_on: d.camera_on,
                vibrate,
                added: Vec::new(),
                bolts: &mut *bolts,
            };
            let live = d.drivers[i].process(&mut cx);
            d.camera_on = cx.camera_on;
            let added = std::mem::take(&mut cx.added);
            drop(cx);
            d.drivers.extend(added);
            if !live {
                let x = d.drivers.remove(i);
                x.destroy(scene, ps);
            } else {
                i += 1;
            }
            if last {
                break;
            }
        }
        !d.drivers.is_empty()
    }
}
