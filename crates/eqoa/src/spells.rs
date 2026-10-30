//! Spell effects in the client: SPELLFX.CSF's effects (0xC200) loaded as
//! the scene loads them, played as `VIClient` plays them (`CastingCombat`
//! for 0x00B1, `Msg0776`, and the player's own swing in `AutoAttackTick`),
//! run each frame by `vi::spell` over the creatures as the scene's actors,
//! and drawn: their particles as billboards, their lightning as strips; the
//! sound sprites they create heard as `VIScene::UpdateSoundEmitters`' second
//! half hears a sound sprite.
//!
//! The actors: an entity's slot is its actor, the player is [`SELF`], and an
//! actor an effect makes is numbered from [`MADE`]. A ranged attack's arrow,
//! bolt or thrown weapon is its hand's projectile (`hand_projectiles`), which
//! the effect's projectile event flies as an actor of its own, drawn where it
//! is each frame (`Spells::flying`). Not here: the weapon trail
//! (`SetWeaponTrail` is taken and dropped), the animated sprites an effect
//! can create (three events on the disc; tracked, not drawn), the fade (no
//! event on the disc uses it). The vibrate events of the player's own
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
/// drawn with (`CalcTransAboutCenter`), and the attack types and projectiles
/// of the items in its two hands (`hand_projectiles`).
pub struct Body<'a> {
    pub actor: i32,
    pub sprite: Option<(&'a mut CSprite<Ee>, &'a AnimScene<Ee>, &'a mut Rand)>,
    pub transform: Matrix44<Ee>,
    pub centre: Matrix44<Ee>,
    pub attack: [i32; 2],
    pub projectile: [Option<u32>; 2],
}

/// The attack types a creature's sprite holds for its two hands
/// (`VICSprite::m_items[0..2].m_attackType`, what `SetAttackAction` reads):
/// `VICreatureEntity::ApplyItems` hands the look each hand's item off the
/// channel (`m_itemOver` before `m_item`) with its type, and the look's
/// `Apply` attaches an item with that type (`AttachItem`); a hand that holds
/// nothing keeps the 11, hand to hand, that `Clear` and `DetachItem` leave,
/// and so does every hand of a sprite that holds no items (`holds_items`
/// false: `AttachItem` returns before the type).
pub fn hand_attack_types(c: &net::entity::Creature, holds_items: bool) -> [i32; 2] {
    std::array::from_fn(|i| {
        let model = if c.items_over[i] != 0 { c.items_over[i] } else { c.items[i] };
        if model != 0 && holds_items { c.item_types[i] as i32 } else { 11 }
    })
}

/// The spells' bolts as the game draws them. `VILightning::Render` puts its
/// strips through `VIRaster::BeginTriStrip`, the lit strip program
/// (`VIRStrip`), while `VIScene::Render` runs the spell effects, before it
/// turns the lights off for the weapons' trails (`EnableAllLights(0)`,
/// which leaves the trails their own colour). So each vertex is lit as the
/// program lights it, min(colour + ambient + sun · max(−N·S, 0) + point ·
/// (1 − min(|L|/range, 1)) · max(L·N/|L|, 0), 1), alpha the colour's own;
/// a bolt's colour is one that fades a bright day's ambient toward white.
/// The bolt sets no normal: N is the raster's, which only the light shafts
/// and falling leaves set ((0, 1, 0), `VIEffectVolume`), zero before any
/// (`normal`, `fx::Fx::raster_normal`).
pub fn light_bolts(strips: &mut [Strips], view: &render::View, normal: glam::Vec3) {
    let sun = view.sun * (-normal.dot(view.sun_dir)).max(0.0);
    for s in strips {
        let colors: Vec<Vec<[f32; 4]>> = s
            .strips
            .iter()
            .enumerate()
            .map(|(k, strip)| {
                strip
                    .iter()
                    .enumerate()
                    .map(|(i, (_, p))| {
                        let c = s.colors.get(k).and_then(|c| c.get(i)).copied().unwrap_or(s.color);
                        let mut point = glam::Vec3::ZERO;
                        if let Some((at, range, col)) = view.point.filter(|&(_, r, _)| r > 0.0) {
                            let l = at - glam::Vec3::from(*p);
                            let d = l.length();
                            if d > 0.0 {
                                point = col * (1.0 - (d / range).min(1.0)) * (l.dot(normal) / d).max(0.0);
                            }
                        }
                        let rgb = (glam::Vec3::new(c[0], c[1], c[2]) + view.ambient + sun + point).clamp(glam::Vec3::ZERO, glam::Vec3::ONE);
                        [rgb.x, rgb.y, rgb.z, c[3]]
                    })
                    .collect()
            })
            .collect();
        s.colors = colors;
    }
}

/// `SecondModel`: the projectile `VICreatureEntity::ApplyItems` gives the
/// second hand's item by its attack type (a bow's arrow, a crossbow's bolt,
/// a thrown weapon itself), 0 for none.
pub fn second_model(ty: i32, model: u32) -> u32 {
    match ty {
        7 => 0x845F_ECDD,
        9 => 0x08F5_4A95,
        10 => model,
        _ => 0,
    }
}

/// The projectiles a creature's sprite keeps for its two hands
/// (`VICSprite::m_items[0..2].m_projectile`), as the hands' items are
/// attached: `ApplyItems` hands each item a second model, the second hand's
/// `second_model` and the first hand's 0, and `AttachItem` keeps it for a
/// hand that holds an item on a sprite that holds items (else -1, `None`
/// here). A second model of 0 is the scene's null sprite
/// (`VIClientPart06D00::LoadRes`), which is not -1: a projectile event
/// shoots the first hand's projectile before the second's, so a creature
/// with anything in its first hand shoots the null sprite and nothing is
/// seen to fly.
pub fn hand_projectiles(c: &net::entity::Creature, holds_items: bool) -> [Option<u32>; 2] {
    std::array::from_fn(|i| {
        let model = if c.items_over[i] != 0 { c.items_over[i] } else { c.items[i] };
        (model != 0 && holds_items).then(|| if i == 1 { second_model(c.item_types[1] as i32, model) } else { 0 })
    })
}

/// `VISpriteType`, by the record that makes the sprite
/// (`VIESFParse::ParseObject`'s dispatch).
fn kind(ty: u16) -> i32 {
    match ty {
        0x2D00 => 2,
        0x2200 => 4,
        0x2700 => 5,
        0x2A00 => 7,
        0x2B00 => 8,
        0xB100 => 9,
        0xC100 => 10,
        0x2C00 => 11,
        0x2E00 => 12,
        0x2F00 => 13,
        0xC300 => 14,
        _ => 1,
    }
}

/// A sprite an effect names: its type (9 a sound, 4 an animated sprite),
/// and a sound's radius, volume and wave (`VISoundSprite`); or a hand's
/// projectile, its model's DictID (`model`). A sound's pan and attenuation
/// are its actors' own ([`Made`]). An animated sprite (`VIHSprite`) keeps
/// its record (`hsprite`), and the copy an effect makes of it the play it
/// was restarted with and when (`restart`: `NewCreateSprite`'s mode, 1
/// once, 2 looped, 0 its clip's own).
#[derive(Clone)]
struct Sprite {
    ty: i32,
    radius: F<Ee>,
    volume: F<Ee>,
    wave: Option<Ref>,
    model: Option<u32>,
    hsprite: Option<Ref>,
    restart: Option<(i32, i64)>,
}

/// An animated sprite an effect made (`NewCreateSprite`'s `CopySprite` of an
/// HSprite: its own skeleton and plays), and what hangs on it: a mesh on a
/// node, or a skin over the skeleton (node -1).
#[derive(Clone)]
struct Posed {
    h: vi::hsprite::HSprite<Ee>,
    attach: Vec<(i32, i32)>,
}

/// An actor an effect made. One of a sound sprite shows its own copy of it
/// (`VIScene::CreateActor` copies a sprite whose `GetInstType` is 1, as
/// `VISoundSprite`'s is), so the copy's pan and attenuation are the actor's:
/// the constructor's (on, 0) until the effect sets them (`NewCreateSound`).
struct Made {
    sprite: i32,
    at: Vect3<Ee>,
    hpr: Vect3<Ee>,
    scale: F<Ee>,
    pan: bool,
    attenuation: i32,
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
    /// `VIScene`'s null sprite (`Init`: `CreateSprite(VISpriteTypePoint, 1)`),
    /// which draws nothing
    null: i32,
    /// the hands' projectiles found: DictID -> sprite (-1 for none on the disc)
    models: HashMap<u32, i32>,
    made: BTreeMap<i32, Made>,
    next: i32,
    /// `VILightningFxInit`'s texture
    lightning: i32,
    /// the sound actors playing: actor -> channel (each actor's own copy of
    /// its sprite, its own voice), and each sound sprite's wave on the device
    playing: BTreeMap<i32, i32>,
    waves: HashMap<i32, i32>,
    /// this frame's drawing
    pub batches: Vec<Batch>,
    pub strips: Vec<Strips>,
    /// the actors made of a model (the projectiles in flight): its DictID and
    /// the actor's transform, in actor order
    pub flying: Vec<(u32, Matrix44<Ee>)>,
    /// the animated sprites the effects made, posed on the spells' clock (by
    /// the copy each shows), each record read once (its skeleton, what hangs
    /// on it and its default clip), with their clips and ref maps
    posed: HashMap<i32, Posed>,
    templates: HashMap<Ref, Option<(Posed, usize)>>,
    anims: AnimScene<Ee>,
    anim_cache: vi::character::AnimCache,
    /// this frame's meshes the posed sprites carry (`VIHSprite::Raster`):
    /// each attachment's DictID, its transform (a node's world under the
    /// centre transform, or a skin's centre transform) and a skin's palette
    /// (empty for a mesh on a node)
    pub meshes: Vec<(u32, Matrix44<Ee>, Vec<[[f32; 4]; 4]>)>,
    pub played: usize,
    /// bolts drawn, all frames
    pub bolts: usize,
    /// the vibrate events, as the callback gets them (power, time, mode:
    /// `Rumble`'s motor, level and ms)
    pub rumbles: Vec<(u8, u8, i32)>,
}

impl Spells {
    /// No effects, sprites or actors yet.
    fn empty(time: i64) -> Spells {
        Spells {
            sys: SpellSystem::default(),
            ps: System::new(time, 2000),
            rand: Rand(1),
            by_dict: HashMap::new(),
            sprites: Vec::new(),
            null: -1,
            models: HashMap::new(),
            made: BTreeMap::new(),
            next: MADE,
            lightning: vi::hash::hash_resource_id("lightning.tga") as i32,
            playing: BTreeMap::new(),
            waves: HashMap::new(),
            batches: Vec::new(),
            strips: Vec::new(),
            flying: Vec::new(),
            posed: HashMap::new(),
            templates: HashMap::new(),
            anims: AnimScene::default(),
            anim_cache: vi::character::AnimCache::new(),
            meshes: Vec::new(),
            played: 0,
            bolts: 0,
            rumbles: Vec::new(),
        }
    }

    /// Every 0xC200 record in the library (SPELLFX.CSF), with the particle
    /// definitions and sprites its events name, in the order the library's
    /// view registers its files (the 2002 disc's SPELLFX.CSF ahead of
    /// Frontiers' while its art is drawn, `era`).
    pub fn load(lib: &Library, time: i64) -> Spells {
        let mut s = Spells::empty(time);
        let mut defs: HashMap<Ref, usize> = HashMap::new();
        let mut sprites: HashMap<Ref, usize> = HashMap::new();
        for f in lib.in_view() {
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
        s.sprites.push(Sprite { ty: 2, ..Sprite::new() });
        s.null = (s.sprites.len() - 1) as i32;
        s
    }

    /// A hand's projectile as `LoadRes` finds it in the dictionary: its
    /// record a sprite of the record's type, or none (-1: the disc has no
    /// `"!Item"`, the stand-in it would fall back to).
    fn find_projectile(&mut self, lib: &Library, id: u32) {
        if self.models.contains_key(&id) {
            return;
        }
        let k = match lib.lookup(id as i32) {
            Some(r) => {
                self.sprites.push(Sprite { ty: kind(lib.file(r.file).node(r.node).ty), model: Some(id), ..Sprite::new() });
                (self.sprites.len() - 1) as i32
            }
            None => -1,
        };
        self.models.insert(id, k);
    }

    pub fn describe(&self) -> String {
        format!("{} spell effects, {} played, {} particles, {} bolt frames drawn, {} projectiles in flight, {} meshes on {} animated sprites", self.sys.effects.len(), self.played, self.sys.particle_count(&self.ps), self.bolts, self.flying.len(), self.meshes.len(), self.posed.len())
    }

    /// An effect's DictID by its place in the files, for trying one out.
    pub fn nth(&self, i: usize) -> Option<u32> {
        self.sys.effects.get(i).map(|e| e.id)
    }

    /// How many effects there are, and an effect's place by its DictID.
    pub fn count(&self) -> usize {
        self.sys.effects.len()
    }

    pub fn index_of(&self, effect: u32) -> Option<usize> {
        self.by_dict.get(&effect).copied()
    }

    /// The playbacks running (a cast is one until its effect ends).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn playing(&self) -> usize {
        self.sys.playing()
    }

    /// Nothing of any effect is left: no playback running, no particle
    /// alive, nothing flying (the preview casts again then, `fxview`).
    pub fn quiet(&self) -> bool {
        self.sys.playing() == 0 && self.sys.particle_count(&self.ps) == 0 && self.flying.is_empty() && self.made.is_empty()
    }

    /// The animated sprites the effects made, posed for scene time `time`
    /// (`VIScene::RasterActor`'s `VIHSprite::Raster` for each actor showing
    /// one): each copy made the first time its actor is drawn, its skeleton
    /// read off the record (`load_hsprite`) and its plays restarted as its
    /// effect asked, when it asked (`restart`); then every frame `Process`,
    /// the centre transform at the actor's transform, and each attachment:
    /// a mesh at its node's world, a skin at the centre with the skeleton's
    /// blend matrices. A copy goes with its actor (`ReleaseActor`).
    fn pose(&mut self, lib: &Library, time: i64) {
        self.meshes.clear();
        let shown: std::collections::BTreeSet<i32> = self.made.values().map(|m| m.sprite).collect();
        self.posed.retain(|k, _| shown.contains(k));
        for m in self.made.values() {
            let Some(sp) = self.sprites.get(m.sprite as usize) else { continue };
            let (Some(r), Some((mode, at))) = (sp.hsprite, sp.restart) else { continue };
            if !self.posed.contains_key(&m.sprite) {
                // the record read once (`ParseHSpriteObj`): its skeleton, a
                // skin attached as `ParseHSpriteAttachments` attaches it
                // (`AttachSkin`, which `SetBlendMatrices` looks for), its
                // default clip
                let (anims, cache) = (&mut self.anims, &mut self.anim_cache);
                let template = self.templates.entry(r).or_insert_with(|| {
                    let (mut h, attach) = vi::character::load_hsprite(lib, r, anims, cache)?;
                    for &(id, node) in &attach {
                        if node == -1 {
                            h.attach_skin(id);
                        }
                    }
                    let first = h.plays.iter().next()?;
                    let anim = h.plays.get(first).anim;
                    Some((Posed { h, attach }, anim))
                });
                let Some((t, anim)) = template.as_ref() else { continue };
                // the copy's plays as `NewCreateSprite` restarts them, from
                // when it did: `EraseAllPlay`, then the default clip once,
                // looped or as the clip says
                let mut p = t.clone();
                let plays: Vec<i32> = p.h.plays.iter().collect();
                for i in plays {
                    p.h.erase_play(i);
                }
                self.anims.time = at;
                let speed = self.anims.anims[*anim].play_speed;
                match mode {
                    1 => p.h.add_play(*anim, 1, speed, &self.anims),
                    2 => p.h.add_play(*anim, 0, speed, &self.anims),
                    _ => p.h.add_play_default(*anim, &self.anims),
                };
                self.posed.insert(m.sprite, p);
            }
            let Some(p) = self.posed.get_mut(&m.sprite) else { continue };
            self.anims.time = time;
            p.h.process(-1, &self.anims);
            let centre = p.h.calc_trans_about_center(&Matrix44::transform(&m.hpr, m.scale, &m.at), &self.anims);
            let mut palette: Option<Vec<[[f32; 4]; 4]>> = None;
            for &(id, node) in &p.attach {
                if node >= 0 {
                    if let Some(n) = p.h.nodes.get(node as usize) {
                        self.meshes.push((id as u32, Matrix44::mul(&n.world, &centre), Vec::new()));
                    }
                } else {
                    let pal = palette.get_or_insert_with(|| {
                        p.h.set_blend_matrices();
                        p.h.blend.iter().map(|b| b.to_cols()).collect()
                    });
                    self.meshes.push((id as u32, centre, pal.clone()));
                }
            }
        }
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
        for id in bodies.iter().flat_map(|b| b.projectile).flatten().filter(|&id| id != 0) {
            self.find_projectile(lib, id);
        }
        let mut sc = Scene { bodies, made: &mut self.made, next: &mut self.next, sprites: &mut self.sprites, null: self.null, models: &self.models, time, camera, rumbles: &mut self.rumbles };
        for (c, caster, target, targets) in casts {
            let Some(&i) = self.by_dict.get(&c.effect) else { continue };
            if self.sys.play(i, c.time, *caster, *target, targets, &mut sc) != -1 {
                self.played += 1;
            }
        }
        self.sys.process(&mut sc, &mut self.ps, &mut self.rand);
        // `RasterActor` draws an actor as its sprite at its transform
        self.flying = self.made.values().filter_map(|m| Some((self.sprites.get(m.sprite as usize)?.model?, Matrix44::transform(&m.hpr, m.scale, &m.at)))).collect();
        self.pose(lib, time);
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
            let mut waves = std::mem::take(&mut self.waves);
            self.hear(eye, &mut Device { s, lib, waves: &mut waves });
            self.waves = waves;
        }
    }

    /// An entity went: `NotifyActorWasDestroyed`.
    pub fn actor_gone(&mut self, actor: i32, bodies: &mut [Body], time: i64) {
        let mut sc = Scene { bodies, made: &mut self.made, next: &mut self.next, sprites: &mut self.sprites, null: self.null, models: &self.models, time, camera: None, rumbles: &mut self.rumbles };
        self.sys.notify_actor_destroyed(actor, &mut sc);
    }

    /// `StopIfThisWasCaster(actor)`: a death ends the spells it was casting.
    pub fn stop_caster(&mut self, actor: i32, bodies: &mut [Body], time: i64) {
        let mut sc = Scene { bodies, made: &mut self.made, next: &mut self.next, sprites: &mut self.sprites, null: self.null, models: &self.models, time, camera: None, rumbles: &mut self.rumbles };
        self.sys.stop_if_this_was_caster(actor, &mut sc, &mut self.ps);
    }

    /// `UpdateSoundEmitters`' sound sprites, for the ones the effects made.
    /// Each sound actor shows its own copy of its sprite ([`Made`]), and the
    /// copy keeps the frame it was heard and its voice, so they are the
    /// actor's: one whose sphere the camera is in starts its sound the first
    /// frame it is heard and never again while it is (a quad attack's four
    /// strikes, each its own actor, are four sounds, overlapping or not),
    /// and takes the volume its attenuation gives the distance and the pan
    /// of its side of the camera; one no longer heard is stopped, as the
    /// sound of an actor the effect released (`VISoundSprite::Release`).
    fn hear(&mut self, eye: &Eye<Ee>, v: &mut dyn Voices) {
        let found = query_emitters(&self.made, &self.sprites, eye);
        let cp = eye.position;
        for &actor in &found {
            let m = &self.made[&actor];
            let sp = &self.sprites[m.sprite as usize];
            let ch = match self.playing.get(&actor) {
                Some(&ch) => ch,
                None => {
                    let Some(wave) = sp.wave else { continue };
                    let ch = v.play(m.sprite, wave);
                    // not started: asked again next frame
                    if ch <= 0 {
                        continue;
                    }
                    self.playing.insert(actor, ch);
                    ch
                }
            };
            let mut d = Vect3::new(cp.x - m.at.x, F::ZERO, cp.z - m.at.z);
            let mut right = Vect3::new(eye.direction.z, F::ZERO, -eye.direction.x);
            d.normalize();
            right.normalize();
            let pan = d.x * right.x + d.z * right.z;
            let mut t = cp.distance(&m.at) / (m.scale * sp.radius);
            match m.attenuation {
                1 => t = t * (t * (t * t)),
                2 => {
                    t = F::ONE - t;
                    t = t * (t * (t * t));
                    t = F::ONE - t;
                }
                _ => {}
            }
            v.set(ch, ((F::ONE - t) * sp.volume).0, if m.pan { pan.0 } else { 0.0 });
        }
        let gone: Vec<i32> = self.playing.keys().copied().filter(|k| !found.contains(k)).collect();
        for k in gone {
            if let Some(ch) = self.playing.remove(&k) {
                v.stop(ch);
            }
        }
    }
}

/// What `hear` asks of the sound device.
trait Voices {
    /// A voice of the sound sprite's wave, started: its channel, or 0 or
    /// less for none.
    fn play(&mut self, sprite: i32, wave: Ref) -> i32;
    fn stop(&mut self, channel: i32);
    fn set(&mut self, channel: i32, volume: f32, pan: f32);
}

/// The sound device as `Voices`: a sprite's wave made on it the first time
/// one of its actors is heard (shared by them all, as `VISoundSprite::Copy`
/// shares the sound), then played.
struct Device<'a> {
    s: &'a mut crate::sound::Sound,
    lib: &'a Library,
    waves: &'a mut HashMap<i32, i32>,
}

impl Voices for Device<'_> {
    fn play(&mut self, sprite: i32, r: Ref) -> i32 {
        let wave = match self.waves.get(&sprite) {
            Some(&w) => w,
            None => {
                let c = self.lib.file(r.file);
                let Some(w) = esf::sound::wave(c, r.node) else { return -1 };
                let i = self.s.dev.create_wave(w.channels, w.blocks, w.rate, w.volume, w.pan, w.looped, &w.adpcm);
                self.waves.insert(sprite, i);
                i
            }
        };
        self.s.dev.play(wave, 0, 0)
    }

    fn stop(&mut self, channel: i32) {
        self.s.dev.stop(channel);
    }

    fn set(&mut self, channel: i32, volume: f32, pan: f32) {
        self.s.dev.set_volume(channel, volume);
        self.s.dev.set_pan(channel, pan);
    }
}

/// `VIEmitterQuery`'s size for the sound sprites (`Init(32, 9)`).
const EMITTERS: usize = 32;

/// `QueryEmittersAtLocation` for the sound sprites (type 9), over the
/// actors the effects made: each whose sphere holds the camera
/// (`VISphere::PointInSolid`: the actor's place, its sprite's radius at its
/// scale), keyed as `VISprite::QueryEmitters` keys it (that radius over its
/// distance from the camera), in the multimap's order (ascending, an equal
/// key after the ones before it) and at most 32, the smallest dropped.
fn query_emitters(made: &BTreeMap<i32, Made>, sprites: &[Sprite], eye: &Eye<Ee>) -> Vec<i32> {
    use std::cmp::Ordering::{Equal, Greater, Less};
    let cp = eye.position;
    let mut found: Vec<(F<Ee>, i32)> = Vec::new();
    for (&actor, m) in made {
        let Some(sp) = sprites.get(m.sprite as usize) else { continue };
        if sp.ty != 9 || sp.wave.is_none() {
            continue;
        }
        let r = m.scale * sp.radius;
        let (dx, dy, dz) = (cp.x - m.at.x, cp.y - m.at.y, cp.z - m.at.z);
        if dx * dx + dy * dy + dz * dz <= r * r {
            found.push((r * (F::ONE / cp.distance(&m.at)), actor));
        }
    }
    // stable: an equal key stays after the ones inserted before it
    found.sort_by(|a, b| if a.0 < b.0 { Less } else if b.0 < a.0 { Greater } else { Equal });
    let full = found.len().saturating_sub(EMITTERS);
    found.drain(..full);
    found.into_iter().map(|(_, a)| a).collect()
}

impl Sprite {
    fn new() -> Sprite {
        Sprite { ty: 0, radius: F::ONE, volume: F::ONE, wave: None, model: None, hsprite: None, restart: None }
    }
}

/// A sprite an effect names, as `ParseSoundSpriteObj` reads a sound (radius
/// and volume, 1 and 1 before version 1; the wave its child) and anything
/// else by its type.
fn sprite(lib: &Library, r: Ref) -> Sprite {
    let c = lib.file(r.file);
    let mut s = Sprite::new();
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
        ty => {
            s.ty = kind(ty);
            if ty == esf::types::HSPRITE {
                s.hsprite = Some(r);
            }
        }
    }
    s
}

/// The scene as the effects see it.
struct Scene<'a, 'b, 'c> {
    bodies: &'a mut [Body<'b>],
    made: &'a mut BTreeMap<i32, Made>,
    next: &'a mut i32,
    sprites: &'a mut Vec<Sprite>,
    null: i32,
    models: &'a HashMap<u32, i32>,
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

    fn item_projectile(&self, actor: i32, slot: usize) -> i32 {
        match self.body(actor).and_then(|b| b.projectile.get(slot).copied().flatten()) {
            None => -1,
            Some(0) => self.null,
            Some(id) => self.models.get(&id).copied().unwrap_or(-1),
        }
    }

    fn create_actor(&mut self, at: Vect3<Ee>, hpr: Vect3<Ee>, scale: F<Ee>, sprite: i32) -> i32 {
        let id = *self.next;
        *self.next += 1;
        self.made.insert(id, Made { sprite, at, hpr, scale, pan: true, attenuation: 0 });
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

    /// `NewCreateSprite` on its copy of an HSprite: `EraseAllPlay`, then its
    /// default clip played once (mode 1), looped (2) or as the clip says (0),
    /// from now; the copy is posed from it ([`Spells::pose`]).
    fn restart_hsprite(&mut self, sprite: i32, mode: i32) {
        if let Some(s) = self.sprites.get_mut(sprite as usize) {
            s.restart = Some((mode, self.time));
        }
    }

    /// On the actor's own copy of its sprite (`GetActorSpritePtr`).
    fn sound_settings(&mut self, actor: i32, pan: bool, attenuation: i32) {
        if let Some(m) = self.made.get_mut(&actor) {
            m.pan = pan;
            m.attenuation = attenuation;
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

    /// A bolt (`VILightning::Render`'s strip) lit as the strip program
    /// lights a vertex: its colour plus the ambient, plus the sun by the
    /// raster's normal against the sun's way, plus the point light by its
    /// falloff and the normal, clamped at one, alpha its own. The controls:
    /// with no normal (no light shaft or leaf drawn yet) the sun and the
    /// point light add nothing, and a bolt left as it is keeps its colour.
    #[test]
    fn a_bolt_is_lit_as_virstrip_lights_a_vertex() {
        let bolt = || vec![Strips {
            texture: 0,
            blend: 1,
            ztest: true,
            zwrite: false,
            color: [0.0, 0.41, 1.0, 1.0],
            strips: vec![vec![([0.0, 1.0], [0.0, 0.0, 0.0]), ([0.0, 0.0], [0.0, 1.0, 0.0]), ([1.0, 1.0], [4.0, 0.0, 0.0])]],
            colors: Vec::new(),
        }];
        let view = render::View {
            ambient: glam::Vec3::new(0.3, 0.3, 0.35),
            sun: glam::Vec3::new(0.5, 0.45, 0.4),
            // the sun's light travels down and across, 0.6 of it down
            sun_dir: glam::Vec3::new(0.8, -0.6, 0.0),
            // a lantern 2 above the first vertex, reaching 8
            point: Some((glam::Vec3::new(0.0, 2.0, 0.0), 8.0, glam::Vec3::new(0.4, 0.3, 0.2))),
            ..render::View::daylight(glam::Mat4::IDENTITY)
        };
        let up = glam::Vec3::Y;
        let mut lit = bolt();
        light_bolts(&mut lit, &view, up);
        let at = |p: glam::Vec3| {
            let l = glam::Vec3::new(0.0, 2.0, 0.0) - p;
            let point = glam::Vec3::new(0.4, 0.3, 0.2) * (1.0 - (l.length() / 8.0).min(1.0)) * (l.dot(up) / l.length()).max(0.0);
            (glam::Vec3::new(0.0, 0.41, 1.0) + view.ambient + view.sun * 0.6 + point).min(glam::Vec3::ONE)
        };
        let c = &lit[0].colors[0];
        for (i, p) in [glam::Vec3::ZERO, glam::Vec3::Y, glam::Vec3::new(4.0, 0.0, 0.0)].into_iter().enumerate() {
            let want = at(p);
            assert!((0..3).all(|k| (c[i][k] - want[k]).abs() < 1e-5), "vertex {i}: {:?}, want {want:?}", c[i]);
            assert_eq!(c[i][3], 1.0, "the alpha is the colour's");
        }
        // the blue is clamped at one; the green reached past its 0.41
        assert!(c[0][2] == 1.0 && c[0][1] > 0.9, "{:?}", c[0]);
        // the controls: no normal, the ambient alone; a bolt not lit keeps
        // its colour (the trails)
        let mut flat = bolt();
        light_bolts(&mut flat, &view, glam::Vec3::ZERO);
        let want = (glam::Vec3::new(0.0, 0.41, 1.0) + view.ambient).min(glam::Vec3::ONE);
        assert!(flat[0].colors[0].iter().all(|c| (0..3).all(|k| (c[k] - want[k]).abs() < 1e-5)), "{:?}", flat[0].colors[0]);
        assert!(bolt()[0].colors.is_empty() && lit[0].colors[0][0] != bolt()[0].color, "the control must differ");
    }

    /// An empty hand is 11 (hand to hand), which the swing's play-attack
    /// event (type 15) plays as action 23 or 22; read as 0, as it was, it
    /// plays nothing. A held item takes the channel's type, the override's
    /// model counting as the item.
    #[test]
    fn an_empty_hand_swings_hand_to_hand() {
        use vi::spell::attack_action;
        let bare = net::entity::Creature::default();
        assert_eq!(hand_attack_types(&bare, true), [11, 11]);
        assert_eq!(attack_action(hand_attack_types(&bare, true), 0, false), Some(23));
        assert_eq!(attack_action(hand_attack_types(&bare, true), 1, false), Some(22));
        // the negative control: the type an empty hand used to get
        assert_eq!(attack_action([0, 0], 0, false), None);
        let mut c = net::entity::Creature { item_types: [1, 5], ..Default::default() };
        c.items[0] = 0x1234;
        assert_eq!(hand_attack_types(&c, true), [1, 11]);
        c.items_over[1] = 0x5678;
        assert_eq!(hand_attack_types(&c, true), [1, 5]);
        assert_eq!(attack_action(hand_attack_types(&c, true), 1, false), Some(27));
        // a sprite that holds no items (a form) swings both hands bare
        assert_eq!(hand_attack_types(&c, false), [11, 11]);
        // an item whose type the channel leaves 0 swings nothing, as in the game
        c.item_types = [0, 0];
        assert_eq!(attack_action(hand_attack_types(&c, true), 0, false), None);
    }

    /// `SecondModel`: a bow shoots the arrow, a crossbow the bolt, a thrown
    /// weapon itself, anything else nothing. The controls: no projectile at
    /// all (the client before), and the bow's and the crossbow's swapped.
    #[test]
    fn a_ranged_weapon_shoots_its_own_ammunition() {
        let check = |f: &dyn Fn(i32, u32) -> u32| {
            f(7, 0x1234) == 0x845F_ECDD && f(9, 0x1234) == 0x08F5_4A95 && f(10, 0x1234) == 0x1234 && [0, 1, 2, 3, 4, 5, 6, 8, 11].iter().all(|&t| f(t, 0x1234) == 0)
        };
        assert!(check(&second_model));
        assert!(!check(&|_, _| 0), "the control must fail");
        let swapped = |t: i32, m: u32| second_model(match t {
            7 => 9,
            9 => 7,
            t => t,
        }, m);
        assert!(!check(&swapped), "the control must fail");
    }

    /// What `ApplyItems` and `AttachItem` leave each hand: a bow in the
    /// second hand its arrow and the first hand none; a weapon in the first
    /// hand the null sprite (a second model of 0, which is not -1), shot
    /// before the second hand's arrow; the override's model counting as the
    /// item's; nothing on a sprite that holds no items. The controls: every
    /// hand its own second model (a thrown weapon in the first hand would
    /// fly), and a second model of 0 taken as none (the arrow would fly past
    /// a weapon in the first hand).
    #[test]
    fn only_the_second_hand_shoots() {
        let bare = net::entity::Creature::default();
        assert_eq!(hand_projectiles(&bare, true), [None, None]);
        let mut bow = net::entity::Creature { item_types: [0, 7], ..Default::default() };
        bow.items[1] = 0xDEBA_8F46;
        assert_eq!(hand_projectiles(&bow, true), [None, Some(0x845F_ECDD)]);
        assert_eq!(hand_projectiles(&bow, false), [None, None], "a form holds no items");
        let mut armed = bow.clone();
        armed.items[0] = 0xF4DD_991A;
        armed.item_types[0] = 5;
        assert_eq!(hand_projectiles(&armed, true), [Some(0), Some(0x845F_ECDD)]);
        let mut thrown = net::entity::Creature { item_types: [10, 10], ..Default::default() };
        thrown.items = [0x1111, 0x2222, 0];
        thrown.items_over[1] = 0xB0A9_CBD2;
        assert_eq!(hand_projectiles(&thrown, true), [Some(0), Some(0xB0A9_CBD2)]);
        let mut melee = net::entity::Creature { item_types: [0, 5], ..Default::default() };
        melee.items[1] = 0xF4DD_991A;
        assert_eq!(hand_projectiles(&melee, true), [None, Some(0)]);
        // the controls
        let own = |c: &net::entity::Creature| -> [Option<u32>; 2] {
            std::array::from_fn(|i| {
                let model = if c.items_over[i] != 0 { c.items_over[i] } else { c.items[i] };
                (model != 0).then(|| second_model(c.item_types[i] as i32, model))
            })
        };
        assert_ne!(own(&thrown), hand_projectiles(&thrown, true), "the control must fail");
        let zero_none = |c: &net::entity::Creature| hand_projectiles(c, true).map(|p| p.filter(|&id| id != 0));
        assert_ne!(zero_none(&armed), hand_projectiles(&armed, true), "the control must fail");
    }

    /// Range1, the swing a bow makes, cast by a human whose second hand holds
    /// a bow at a creature ten off, to the side of its heading (SKIPs without
    /// the disc): once the shot's wait is over, its projectile event makes
    /// the arrow (ITEM.ESF's GroupSprite 0x845FECDD) an actor of its own and
    /// flies it to the target from the next frame (made at 8000 under the
    /// origin, `NewPlayTargetProjectile`'s place for it), nearer each frame,
    /// pointing at it, and lets it go there; `flying` is what is drawn. The
    /// controls: a weapon in the first hand, whose projectile (the null
    /// sprite) is shot first, so nothing drawn flies; and no projectile at
    /// all, as before.
    #[test]
    fn a_bows_arrow_flies_to_its_target() {
        let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&iso_path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut lib = Library::new();
        for f in ["CHAR.ESF", "ITEM.ESF", "SPELLFX.CSF"] {
            lib.add(f, esf::Container::open(disc.read(f).unwrap()).unwrap());
        }
        let range1 = vi::hash::hash_resource_id("Range1");
        let target_at = Vect3::<Ee>::of(6.0, 0.0, 8.0);
        let fly = |projectile: [Option<u32>; 2]| -> Vec<Vec<(u32, Matrix44<Ee>)>> {
            let mut sp = Spells::load(&lib, 0);
            let mut anims = AnimScene::<Ee>::default();
            let mut rand = Rand(1);
            let human = lib.lookup(0x70D8_98C6u32 as i32).expect("the human's CSprite");
            let mut cs = vi::character::load_csprite(&lib, human, &mut anims, &mut vi::character::AnimCache::new(), &mut rand).expect("a CSprite");
            let actor = Matrix44::<Ee>::identity();
            let target = Matrix44::transform(&Vect3::default(), F::ONE, &target_at);
            let mut frames = Vec::new();
            for t in (0..4000i64).step_by(33) {
                anims.time = t;
                cs.h.process(0, &anims);
                let centre = cs.h.calc_trans_about_center(&actor, &anims);
                let mut bodies = vec![
                    Body { actor: SELF, sprite: Some((&mut cs, &anims, &mut rand)), transform: actor, centre, attack: [11, 7], projectile },
                    Body { actor: 1, sprite: None, transform: target, centre: target, attack: [11, 11], projectile: [None; 2] },
                ];
                let casts = if t == 0 { vec![(Cast { effect: range1, time: 0, caster: 0, target: 1, targets: Vec::new() }, SELF, 1, Vec::new())] } else { Vec::new() };
                sp.frame(&mut bodies, &casts, SELF, None, t, &Eye::default(), &lib, None);
                frames.push(sp.flying.clone());
            }
            frames
        };
        let bow = fly([None, Some(0x845F_ECDD)]);
        let up: Vec<usize> = (0..bow.len()).filter(|&i| !bow[i].is_empty()).collect();
        assert!(up.len() > 3, "the arrow flies: {up:?}");
        assert!(up.windows(2).all(|w| w[1] == w[0] + 1), "one flight: {up:?}");
        let d: Vec<f32> = up
            .iter()
            .map(|&i| {
                assert_eq!(bow[i].len(), 1);
                let (id, m) = bow[i][0];
                assert_eq!(id, 0x845F_ECDD);
                let at = m.m[3].xyz();
                let mut ahead = m.m[2].xyz();
                let mut to = Vect3::new(target_at.x - at.x, target_at.y - at.y, target_at.z - at.z);
                ahead.normalize();
                to.normalize();
                let cos = ahead.x * to.x + ahead.y * to.y + ahead.z * to.z;
                let made = at.y.0 == -8000.0;
                assert!(made == (i == up[0]), "frame {i}: made at {at:?}");
                assert!(made || at.distance(&target_at).0 < 3.0 || cos.0 > 0.95, "frame {i}: the arrow points {ahead:?}, the target is {to:?}");
                at.distance(&target_at).0
            })
            .collect();
        assert!(d.windows(2).all(|w| w[1] < w[0]), "nearer each frame: {d:?}");
        assert!(d[1] > 8.0 && *d.last().unwrap() < 4.0, "from the bow to the target: {d:?}");
        assert!(bow.last().unwrap().is_empty(), "let go at the target");
        // the controls
        assert!(fly([Some(0), Some(0x845F_ECDD)]).iter().all(Vec::is_empty), "the control must fail");
        assert!(fly([None, None]).iter().all(Vec::is_empty), "the control must fail");
    }

    /// The animated sprites two effects make (SKIPs without the disc), cast by
    /// a human at itself: the warrior's Furious Defense (0x1DAEB5B7) makes
    /// HSprite 0xB72087E1, whose mesh 0x09F69042 hangs on its node 1 and
    /// grows from a quarter of its size; 0x4DB80385 makes 0xA09C0316 and
    /// then 0x3BC88851, each a skin over a 45-node skeleton. Each is posed
    /// while its actor lives, about the caster, and gone with it. The
    /// controls: a skin's palette is the skeleton's blend matrices (all zero
    /// unless the skin is attached as `AttachSkin` attaches it), and nothing
    /// before the effect's sprite event or after its end.
    #[test]
    fn an_effects_animated_sprites_are_posed_and_drawn() {
        let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&iso_path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut lib = Library::new();
        for f in ["CHAR.ESF", "ITEM.ESF", "SPELLFX.CSF"] {
            lib.add(f, esf::Container::open(disc.read(f).unwrap()).unwrap());
        }
        let run = |effect: u32| -> Vec<Vec<(u32, Matrix44<Ee>, Vec<[[f32; 4]; 4]>)>> {
            let mut sp = Spells::load(&lib, 0);
            let mut anims = AnimScene::<Ee>::default();
            let mut rand = Rand(1);
            let human = lib.lookup(0x70D8_98C6u32 as i32).expect("the human's CSprite");
            let mut cs = vi::character::load_csprite(&lib, human, &mut anims, &mut vi::character::AnimCache::new(), &mut rand).expect("a CSprite");
            let actor = Matrix44::transform(&Vect3::default(), F::ONE, &Vect3::of(100.0, 20.0, 200.0));
            let mut frames = Vec::new();
            for t in (0..9000i64).step_by(33) {
                anims.time = t;
                cs.h.process(0, &anims);
                let centre = cs.h.calc_trans_about_center(&actor, &anims);
                let mut bodies = vec![Body { actor: SELF, sprite: Some((&mut cs, &anims, &mut rand)), transform: actor, centre, attack: [11, 11], projectile: [None; 2] }];
                let casts = if t == 0 { vec![(Cast { effect, time: 0, caster: 0, target: 0, targets: Vec::new() }, SELF, SELF, Vec::new())] } else { Vec::new() };
                sp.frame(&mut bodies, &casts, SELF, None, t, &Eye::default(), &lib, None);
                frames.push(sp.meshes.clone());
            }
            assert!(sp.posed.is_empty(), "gone with their actors");
            frames
        };
        let near = |m: &Matrix44<Ee>| (m.m[3].x.0 - 100.0).abs() < 3.0 && (m.m[3].y.0 - 20.0).abs() < 4.0 && (m.m[3].z.0 - 200.0).abs() < 3.0;
        let scale = |m: &Matrix44<Ee>| (m.m[0].x.0 * m.m[0].x.0 + m.m[0].y.0 * m.m[0].y.0 + m.m[0].z.0 * m.m[0].z.0).sqrt();
        // Furious Defense: one mesh on a node, no palette, about the caster,
        // growing while it shows
        let guard = run(0x1DAE_B5B7);
        let up: Vec<usize> = (0..guard.len()).filter(|&i| !guard[i].is_empty()).collect();
        assert!(up.len() > 10 && up[0] > 0, "the guard's column shows, after its event: {up:?}");
        assert!(guard.last().unwrap().is_empty(), "and goes");
        for &i in &up {
            assert_eq!(guard[i].len(), 1);
            let (id, m, palette) = &guard[i][0];
            assert!(*id == 0x09F6_9042 && palette.is_empty() && near(m), "frame {i}: {id:#x} at {:?}", m.m[3]);
        }
        let first = scale(&guard[up[0]][0].1);
        let most = up.iter().map(|&i| scale(&guard[i][0].1)).fold(0.0, f32::max);
        assert!(first < 0.5 && most > 0.9, "it grows: {first} to {most}");
        // 0x4DB80385: the two skins in turn, each over 45 nodes, posed
        let spirit = run(0x4DB8_0385);
        let skins: std::collections::BTreeSet<u32> = spirit.iter().flatten().map(|m| m.0).collect();
        assert_eq!(skins, [0xEEB8_1B03, 0xF6B0_C0AA].into());
        for (id, m, palette) in spirit.iter().flatten() {
            assert!(palette.len() == 45 && near(m), "{id:#x}: {} matrices at {:?}", palette.len(), m.m[3]);
            // the control: a skin not attached leaves the blend matrices zero
            assert!(palette.iter().any(|b| b[0][0] != 0.0 || b[1][1] != 0.0), "{id:#x}: a palette of zeros");
        }
        assert!(spirit.first().unwrap().is_empty() && spirit.last().unwrap().is_empty(), "the controls: none before, none after");
    }

    /// What `hear` asked of the device, and when: each play's (time, sprite)
    /// and channel (its place in the list, from 1), each stop's (time,
    /// channel), each volume and pan set; `refuse` plays fail first, as with
    /// no channel free.
    #[derive(Default)]
    struct Rec {
        plays: Vec<(f64, i32)>,
        stops: Vec<(f64, i32)>,
        sets: Vec<(i32, f32, f32)>,
        now: f64,
        refuse: usize,
    }

    impl Voices for Rec {
        fn play(&mut self, sprite: i32, _wave: Ref) -> i32 {
            if self.refuse > 0 {
                self.refuse -= 1;
                return -1;
            }
            self.plays.push((self.now, sprite));
            self.plays.len() as i32
        }
        fn stop(&mut self, channel: i32) {
            self.stops.push((self.now, channel));
        }
        fn set(&mut self, channel: i32, volume: f32, pan: f32) {
            self.sets.push((channel, volume, pan));
        }
    }

    fn hit_sprite(radius: f32) -> Sprite {
        Sprite { ty: 9, radius: F::new(radius), volume: F::ONE, wave: Some(Ref { file: 0, node: 0 }), model: None, hsprite: None, restart: None }
    }

    fn sound_actor(sprite: i32, x: f32, z: f32) -> Made {
        Made { sprite, at: Vect3::of(x, 0.0, z), hpr: Vect3::default(), scale: F::ONE, pan: true, attenuation: 0 }
    }

    /// The camera at the origin, looking down +z.
    fn eye() -> Eye<Ee> {
        Eye { position: Vect3::default(), direction: Vect3::of(0.0, 0.0, 1.0) }
    }

    /// A quad attack's four hit sounds as its target lists make them: a
    /// sound actor of SPELLFX's hit sprite at each strike (0, 0.5, 1.2 and
    /// 1.9 s, a bard's Attack4 on Sandstorm), each made for 759 ms (the
    /// effect's CreateSound), so each overlaps the one before.
    const STRIKES: [f64; 4] = [0.0, 500.0, 1200.0, 1900.0];
    const LIFE: f64 = 759.0;

    /// The sound actors alive at `t`, one a strike, all of one sprite.
    fn strikes_at(s: &mut Spells, t: f64) {
        for (i, &at) in STRIKES.iter().enumerate() {
            let id = MADE + i as i32;
            if at <= t && t < at + LIFE {
                s.made.entry(id).or_insert_with(|| sound_actor(0, 2.0, 2.0));
            } else {
                s.made.remove(&id);
            }
        }
    }

    /// Frames drawn at `hz` for three seconds of a quad attack.
    fn quad(hz: f64) -> Rec {
        let mut s = Spells::empty(0);
        s.sprites.push(hit_sprite(10.0));
        let mut rec = Rec::default();
        let mut k = 0.0;
        while k * 1000.0 / hz < 3000.0 {
            rec.now = k * 1000.0 / hz;
            strikes_at(&mut s, rec.now);
            s.hear(&eye(), &mut rec);
            k += 1.0;
        }
        rec
    }

    /// Each of a quad attack's four strikes is heard once, on the frame its
    /// sound actor first is, whether or not the one before is still heard,
    /// and stopped as its actor goes: each actor shows its own copy of the
    /// hit sprite (`VIScene::CreateActor`), so the frame heard and the voice
    /// are its own (F3: "I only hear my weapon hitting the mob once"); the
    /// same at any frame rate. The controls, on the same strikes: one voice
    /// a sprite while any of its actors is heard (the client's rule until
    /// now) starts the first strike alone; the sprite's own mark of the
    /// frame heard shared by its actors (the reverted 5b47b1b) starts a
    /// second actor's again on every frame of the game's they overlap.
    #[test]
    fn each_strike_of_a_quad_is_heard_once() {
        for hz in [30.0, 60.0, 165.0] {
            let rec = quad(hz);
            let frame = 1000.0 / hz;
            assert_eq!(rec.plays.len(), 4, "{hz} Hz: {:?}", rec.plays);
            for (p, at) in rec.plays.iter().zip(STRIKES) {
                assert!(at <= p.0 && p.0 < at + frame, "{hz} Hz: the strike at {at} ms started at {}", p.0);
            }
            assert_eq!(rec.stops.len(), 4, "{hz} Hz: {:?}", rec.stops);
            for (k, (st, at)) in rec.stops.iter().zip(STRIKES).enumerate() {
                assert_eq!(st.1, k as i32 + 1, "{hz} Hz: each strike's own voice stopped");
                assert!(at + LIFE <= st.0 && st.0 < at + LIFE + frame, "{hz} Hz: stopped as its actor went: {st:?}");
            }
        }
        // the controls, frame by frame at 30 Hz
        let frame = 1000.0 / 30.0;
        let alive = |t: f64| STRIKES.iter().filter(|&&at| at <= t && t < at + LIFE).count();
        let (mut one_voice, mut playing) = (0, false);
        let (mut marked, mut last) = (0, -2i64);
        let mut k = 0i64;
        while (k as f64) * frame < 3000.0 {
            let n = alive(k as f64 * frame);
            if n > 0 && !playing {
                one_voice += 1;
            }
            playing = n > 0;
            // the shared mark: the first actor starts it when the sprite was
            // not heard last frame, each one after it on every frame
            if n > 0 {
                marked += if last == k - 1 { n - 1 } else { n };
                last = k;
            }
            k += 1;
        }
        assert_eq!(one_voice, 1, "the control: one voice a sprite hears the first strike alone");
        assert!(marked > 4, "the control: a shared mark starts the overlaps again ({marked})");
    }

    /// Two sound actors of one sprite keep their own pan and attenuation
    /// (`NewCreateSound` sets them on the actor's copy): at 10 of a radius
    /// of 20 (t 0.5) the linear one is at half, the quartic at 1 - t^4, and
    /// only the one with pan on is panned (-0.6: its side of the camera).
    /// The control: set on the sprite they share, as the client did, the
    /// second's settings would be both's.
    #[test]
    fn a_sound_actor_keeps_its_own_pan_and_attenuation() {
        let mut s = Spells::empty(0);
        s.sprites.push(hit_sprite(20.0));
        let (a, b) = {
            let mut sc = Scene { bodies: &mut [], made: &mut s.made, next: &mut s.next, sprites: &mut s.sprites, null: -1, models: &s.models, time: 0, camera: None, rumbles: &mut s.rumbles };
            let a = sc.create_actor(Vect3::of(6.0, 0.0, 8.0), Vect3::default(), F::ONE, 0);
            sc.sound_settings(a, true, 1);
            let b = sc.create_actor(Vect3::of(6.0, 0.0, 8.0), Vect3::default(), F::ONE, 0);
            sc.sound_settings(b, false, 0);
            (a, b)
        };
        let mut rec = Rec::default();
        s.hear(&eye(), &mut rec);
        assert_eq!(rec.plays.len(), 2);
        let set_of = |actor: i32| {
            let ch = s.playing[&actor];
            rec.sets.iter().find(|x| x.0 == ch).copied().unwrap()
        };
        let (va, vb) = (set_of(a), set_of(b));
        assert!((va.1 - 0.9375).abs() < 1e-6 && (va.2 + 0.6).abs() < 1e-6, "quartic, panned: {va:?}");
        assert!((vb.1 - 0.5).abs() < 1e-6 && vb.2 == 0.0, "linear, centred: {vb:?}");
        assert_ne!((va.1, va.2), (vb.1, vb.2), "the control: one shared setting would make them one");
    }

    /// A sound the device could not start is asked for again the next frame
    /// (`m_flags` is set only past a channel), and plays once.
    #[test]
    fn a_sound_not_started_is_asked_again() {
        let mut s = Spells::empty(0);
        s.sprites.push(hit_sprite(10.0));
        s.made.insert(MADE, sound_actor(0, 2.0, 2.0));
        let mut rec = Rec { refuse: 2, ..Default::default() };
        for f in 0..4 {
            rec.now = f as f64;
            s.hear(&eye(), &mut rec);
        }
        assert_eq!(rec.plays, vec![(2.0, 0)], "refused twice, then started once");
        assert!(rec.stops.is_empty());
        // the control: refused every time, never started
        let mut never = Rec { refuse: 10, ..Default::default() };
        s.playing.clear();
        for _ in 0..4 {
            s.hear(&eye(), &mut never);
        }
        assert!(never.plays.is_empty());
    }

    /// The query holds 32 at most (`Init(32, 9)`), the smallest keys dropped:
    /// of 40 actors in reach the 32 nearest are heard. A sphere holds the
    /// camera on its edge (`PointInSolid`'s d² <= r²) and not past it.
    #[test]
    fn at_most_32_sounds_are_heard() {
        let mut s = Spells::empty(0);
        for i in 0..40 {
            s.sprites.push(hit_sprite(100.0));
            s.made.insert(MADE + i, sound_actor(i, 0.0, 1.0 + i as f32));
        }
        let mut rec = Rec::default();
        s.hear(&eye(), &mut rec);
        let mut heard: Vec<i32> = rec.plays.iter().map(|p| p.1).collect();
        heard.sort();
        assert_eq!(heard, (0..32).collect::<Vec<_>>(), "the 32 nearest");
        assert!(rec.plays.len() < s.made.len(), "the control: 40 in reach");
        // the sphere's edge
        let mut edge = Spells::empty(0);
        edge.sprites.push(hit_sprite(10.0));
        edge.made.insert(MADE, sound_actor(0, 0.0, 10.0));
        edge.made.insert(MADE + 1, sound_actor(0, 0.0, 10.01));
        let mut rec = Rec::default();
        edge.hear(&eye(), &mut rec);
        assert_eq!(rec.plays.len(), 1, "on the edge heard, past it not");
        assert!(edge.playing.contains_key(&MADE) && !edge.playing.contains_key(&(MADE + 1)));
    }
}
