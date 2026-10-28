//! The spell-visual preview the GM panel's spell editor frames (go-eqoa-server
//! `internal/gmweb`; the user, 2026-10-25: "a default human casting the spell
//! fx ... so we can change without going in game", "for the castingcombat
//! animation"). A human casts a visual at a second one standing a few units
//! off, again and again, as a CastingCombat (0x00B1) plays it in the game:
//! the message's own bytes read by `spells::cast_of`, played by `spells.rs`
//! on the two characters' own skeletons (the caster's casting action, the
//! target's reaction, the particles, the lightning and the projectiles), on
//! an empty stage under an orbit camera. Nothing else of the game runs: no
//! world, no session, no sound. With [`Aim::Both`] the target is a second
//! human and a third, behind the caster, casts the same visual at itself in
//! the same moment, so what it does to its caster and to a target are seen
//! side by side.
//!
//! Natively `eqoa fxview [visual] [cast seconds]` opens a window (drag to
//! orbit, scroll to zoom, ←/→ the previous or next visual on the disc, Space
//! casts again, R puts the camera back), and `eqoa fxview <visual> <out.png>
//! [ms,ms,...] [cast seconds]` draws PNGs at those times, with no window. In a
//! browser it is `web/fxview.html`, which the panel frames: the panel hands
//! it [`FILES`] off the GM's own disc and the visual to play (`web::fxview`).

use crate::spells::{Body as SpellBody, Cast, SELF, Spells};
use esf::Library;
use glam::{Mat4, Vec3};
use render::{GpuModel, Renderer, View, wgpu};
use std::collections::HashMap;
use vi::camera::Eye;
use vi::character::{AnimCache, load_csprite};
use vi::csprite::{CSprite, Rand};
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::hsprite::AnimScene;
use vi::vect::Vect3;

/// The disc's files the stage reads, by their names on the disc and in the
/// order the game registers them (the dictionary's first registration wins):
/// the character art the game keeps resident, the bodies, the spell effects
/// and their textures, the items (a hand's projectile).
pub const FILES: &[&str] = &["CHARCUST.CSF", "CHARFACE.CSF", "CHAR.ESF", "SPELLFX.CSF", "FX.CSF", "ITEM.ESF"];

/// A visual played with no cast time given is cast for 2 s (the panel's
/// `play_visual` default); at most 30 s (`entity.MaxVisualCast`).
pub const CAST_MS: i32 = 2000;
pub const MAX_CAST_MS: i32 = 30_000;
/// How far apart the two stand at first, and the range a page may set.
pub const DISTANCE: f32 = 6.0;
pub const DISTANCES: std::ops::RangeInclusive<f32> = 1.0..=40.0;

/// The caster's id and the target's in the 0x00B1 the stage builds (the
/// spells' actors are `SELF` and [`TARGET`]: the caster is "the player", as
/// the GM's own character is to the GM who plays a visual in the game).
const CASTER_ID: u32 = 0x0100;
const TARGET_ID: u32 = 0x0101;
const TARGET: i32 = 1;
/// The ids of [`Aim::Both`]'s human casting at itself (its cast names it
/// twice, as a cast at oneself does), where it stands (behind the caster,
/// away from the target, so a projectile never passes it) and its heading
/// (turned to the side camera, as the caster alone is seen from in front).
const TWIN_ID: u32 = 0x0102;
const TWIN: i32 = 2;
const TWIN_GAP: f32 = 4.0;
const TWIN_HEADING: f32 = -1.25;

/// The training dummies' body (every "Training Dummy", "Regular Dummy" and
/// "Sparring Dummy" row of eqoaclassic's `npcs` has it): the target of a
/// harmful visual unless the page names another.
pub const DUMMY: i32 = 0x6626_2F1Cu32 as i32;
/// The Training Dummy rows' size (the entity's ModelSize, the actor's
/// scale; the Regular Dummy rows' is 0.45).
pub const DUMMY_SIZE: f32 = 0.25;

/// What a cast lands on (the user: "if offensive spell ... add a second
/// human or training dummy for the target of the spell"): the caster itself
/// (a beneficial spell's, a Self or Group line's), a second human, or a
/// training dummy (a harmful spell's, and a bare visual's unless the GM
/// picks another). The page decides by the spell (the panel asks the server
/// whether its line is harmful); the stage only stands the figure there.
/// `Both` is the caster and a human at once (the community page's players,
/// 2026-10-26: "an option so the spell casts on a target and self ... so you
/// can see if its a buff, debuff or dps spell without having to toggle"): the
/// cast at a second human as `Human` plays it and, beside it, a third human's
/// at itself as `Caster` plays it, the two cast together. A human, not the
/// dummy (the user: "the hitbox makes the spellfx smaller harder to see,
/// while human target is accurate": a landing scales with its actor, and the
/// dummy's rows are a quarter size). Not one cast landing on both: a cast's
/// area targets get only the events flagged for them (`PlayTargetInstant`'s
/// bit 2), so the caster would not be shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aim {
    Caster,
    Human,
    Dummy,
    Both,
}

impl Aim {
    pub fn parse(s: &str) -> Option<Aim> {
        match s.trim().to_ascii_lowercase().as_str() {
            "caster" | "self" => Some(Aim::Caster),
            "human" => Some(Aim::Human),
            "dummy" => Some(Aim::Dummy),
            "both" => Some(Aim::Both),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Aim::Caster => "the caster",
            Aim::Human | Aim::Both => "a human",
            Aim::Dummy => "a training dummy",
        }
    }

    /// Whether a second figure stands in front of the caster, and whether
    /// a human casting at itself stands behind it.
    fn target_on(self) -> bool {
        self != Aim::Caster
    }

    fn twin_on(self) -> bool {
        self == Aim::Both
    }
}

/// After a cast, the next one comes once nothing of the last is left (no
/// playback, particle or flight) for `REST_MS`, but not before the cast
/// time and `MIN_MS` have gone by, and at the latest `MAX_MS` after the
/// cast time ends (a visual that never lets go).
const REST_MS: i64 = 800;
const MIN_MS: i64 = 1000;
const MAX_MS: i64 = 12_000;

/// The look the humans wear: the first robe, untinted, over the first
/// armour, brown hair of full length and the second face (`ApplyLook`'s
/// arguments, as a creature update carries them).
fn look() -> vi::cust::Look {
    vi::cust::Look { armor: [1, 1, 1, 1, 1, 0], armor_tint: [0; 6], hair: [3, 3, 2], face: 1, robe: 1, robe_tint: 0 }
}

/// The 0x00B1 a server sends for a cast of `effect` taking `cast_ms`, from
/// `caster` onto `target` (entity ids; a cast at oneself names the caster
/// twice, as a server's does), read as the client reads it.
pub fn casting_combat(effect: u32, cast_ms: i32, caster: u32, target: u32) -> Option<Cast> {
    let mut b = Vec::with_capacity(17);
    b.extend(caster.to_le_bytes());
    b.extend(target.to_le_bytes());
    b.push(0);
    b.extend((cast_ms.max(0) as u32).to_le_bytes());
    b.extend(effect.to_le_bytes());
    crate::spells::cast_of(0x00B1, &b)
}

/// A visual as a page or the command line names one: hex with or without
/// `0x`, or a decimal number.
pub fn parse_visual(s: &str) -> Option<u32> {
    let s = s.trim();
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None if s.len() == 8 && s.chars().all(|c| c.is_ascii_hexdigit()) => u32::from_str_radix(s, 16).ok(),
        None => s.parse().ok(),
    }
}

/// One of the two: its CSprite on its own clock, its body dressed in the
/// look, and where it stands.
struct Actor {
    model: i32,
    /// The actor's scale (its entity's ModelSize).
    scale: f32,
    sprite: Option<CSprite<Ee>>,
    anims: AnimScene<Ee>,
    rand: Rand,
    mesh: Option<esf::Model>,
    gpu: Option<GpuModel>,
    transform: Matrix44<Ee>,
    centre: Matrix44<Ee>,
}

impl Actor {
    fn new(lib: &Library, model: i32, scale: f32, seed: u64) -> Result<Actor, String> {
        let r = lib.lookup(model).ok_or_else(|| format!("no body {:#010x} in CHAR.ESF", model as u32))?;
        let mut anims = AnimScene::default();
        let mut rand = Rand(seed);
        let sprite = load_csprite(lib, r, &mut anims, &mut AnimCache::new(), &mut rand);
        let mut mesh = lib.build_model(r);
        // the look is a human's: another body stays as the disc has it
        if let (Some(cs), Some(m), true) = (&sprite, mesh.as_mut(), model == crate::viewer::HUMAN) {
            crate::play::dress(m, &vi::cust::apply_look(cs, &look(), false, |id| lib.surface(id as i32).is_some()));
        }
        let m = Matrix44::identity();
        Ok(Actor { model, scale: if scale > 0.0 { scale } else { 1.0 }, sprite, anims, rand, mesh, gpu: None, transform: m, centre: m })
    }

    fn place(&mut self, at: Vect3<Ee>, heading: f32) {
        self.transform = Matrix44::transform(&Vect3::new(F::new(heading), F::ZERO, F::ZERO), F::new(self.scale), &at);
        self.centre = self.transform;
    }

    /// `UpdateCreatures` for a creature that stands (`SetLocomotion(0)`),
    /// then `VIHSprite::Process` at its full level and its centre.
    fn pose(&mut self, now: i64) {
        self.anims.time = now;
        if let Some(cs) = self.sprite.as_mut() {
            cs.set_locomotion(0, &self.anims, &mut self.rand);
            cs.h.process(0, &self.anims);
            self.centre = cs.h.calc_trans_about_center(&self.transform, &self.anims);
        }
    }

    fn body(&mut self, actor: i32) -> SpellBody<'_> {
        SpellBody { actor, sprite: self.sprite.as_mut().map(|cs| (cs, &self.anims, &mut self.rand)), transform: self.transform, centre: self.centre, attack: [11, 11], projectile: [None; 2] }
    }

    /// The animations its sprite's two layers play now (`cur[i].anim`), for
    /// a test or a log.
    fn action(&self) -> Option<[i32; 2]> {
        self.sprite.as_ref().map(|cs| [cs.cur[0].anim, cs.cur[1].anim])
    }
}

/// The orbit camera: round the middle of the two, `yaw` and `pitch` in
/// radians, `zoom` a multiple of the distance that shows both.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
}

impl Default for Orbit {
    /// From the side, the caster on the left and the target on the right, a
    /// little above the heads.
    fn default() -> Self {
        Orbit { yaw: -1.25, pitch: 0.2, zoom: 1.0 }
    }
}

impl Orbit {
    /// The caster alone, from in front and to its right.
    pub fn at_caster() -> Self {
        Orbit { yaw: -0.85, pitch: 0.2, zoom: 1.0 }
    }

    pub fn for_aim(aim: Aim) -> Self {
        if aim == Aim::Caster { Orbit::at_caster() } else { Orbit::default() }
    }

    pub fn drag(&mut self, dx: f32, dy: f32) {
        self.yaw -= dx * 0.01;
        self.pitch = (self.pitch + dy * 0.01).clamp(-0.6, 1.45);
    }

    pub fn wheel(&mut self, steps: f32) {
        self.zoom = (self.zoom * 0.9f32.powf(steps)).clamp(0.25, 4.0);
    }
}

/// The stage: the two (and [`Aim::Both`]'s human casting at itself, made
/// the first time it is asked for), the effects, the visual and its cast
/// time, and when the next cast comes.
pub struct Stage {
    pub lib: Library,
    caster: Actor,
    target: Actor,
    twin: Option<Actor>,
    spells: Spells,
    effect: u32,
    cast_ms: i32,
    distance: f32,
    /// What the cast lands on, and the body a training dummy stands in and
    /// its size.
    aim: Aim,
    dummy: (i32, f32),
    /// Cast at the next step.
    due: bool,
    cast_at: Option<i64>,
    quiet_since: Option<i64>,
    /// The casts made, and the clock of the last step.
    pub casts: usize,
    now: i64,
    pub camera: Orbit,
    projectiles: HashMap<u32, (Option<esf::Model>, Vec<GpuModel>)>,
}

impl Stage {
    /// A human casting the disc's first visual at a training dummy
    /// `DISTANCE` in front of it.
    pub fn new(lib: Library) -> Result<Stage, String> {
        let caster = Actor::new(&lib, crate::viewer::HUMAN, 1.0, 1)?;
        let target = Actor::new(&lib, DUMMY, DUMMY_SIZE, 2).or_else(|_| Actor::new(&lib, crate::viewer::HUMAN, 1.0, 2))?;
        let spells = Spells::load(&lib, 0);
        let effect = spells.nth(0).ok_or("no spell visuals in SPELLFX.CSF")?;
        let mut s = Stage {
            lib,
            caster,
            target,
            twin: None,
            spells,
            effect,
            cast_ms: CAST_MS,
            distance: DISTANCE,
            aim: Aim::Dummy,
            dummy: (DUMMY, DUMMY_SIZE),
            due: true,
            cast_at: None,
            quiet_since: None,
            casts: 0,
            now: 0,
            camera: Orbit::default(),
            projectiles: HashMap::new(),
        };
        s.place();
        Ok(s)
    }

    /// The caster at the origin facing the target, the target `distance`
    /// ahead (+z) facing back, `Both`'s self-caster `TWIN_GAP` behind.
    fn place(&mut self) {
        self.caster.place(Vect3::default(), 0.0);
        self.target.place(Vect3::of(0.0, 0.0, self.distance), std::f32::consts::PI);
        if let Some(t) = self.twin.as_mut() {
            t.place(Vect3::of(0.0, 0.0, -TWIN_GAP), TWIN_HEADING);
        }
    }

    /// The figures standing now: the caster, the target unless the cast is
    /// at the caster, the self-caster with `Both`.
    fn shown(&mut self) -> impl Iterator<Item = &mut Actor> {
        let (target_on, twin_on) = (self.aim.target_on(), self.aim.twin_on());
        [Some(&mut self.caster), target_on.then_some(&mut self.target), self.twin.as_mut().filter(|_| twin_on)].into_iter().flatten()
    }

    /// Every visual on the disc, in its order (SPELLFX.CSF's records): what
    /// the community page lists to be tried.
    pub fn visual_list(&self) -> Vec<u32> {
        (0..self.spells.count()).filter_map(|i| self.spells.nth(i)).collect()
    }

    /// A visual as the command line names one: a DictID, or a place in the
    /// disc's list when no visual has that DictID and the list is that long.
    pub fn visual_or_index(&self, v: u32) -> u32 {
        match self.spells.nth(v as usize) {
            Some(e) if !self.spells.has(v) => e,
            _ => v,
        }
    }

    /// The visual played and its cast time: what was playing is let go (its
    /// particles too: the effects start over) and the new one is cast at the
    /// next step. False when the disc has no visual by that DictID.
    pub fn set_visual(&mut self, effect: u32, cast_ms: i32) -> bool {
        if !self.spells.has(effect) {
            return false;
        }
        self.effect = effect;
        self.cast_ms = cast_ms.clamp(0, MAX_CAST_MS);
        self.again();
        true
    }

    /// Everything playing let go, and a cast at the next step.
    pub fn again(&mut self) {
        self.spells = Spells::load(&self.lib, self.now);
        self.projectiles.values_mut().for_each(|(_, pool)| pool.iter_mut().for_each(|g| g.visible = false));
        self.due = true;
    }

    /// The visual `by` places on from the one playing, round the disc's list.
    pub fn step_visual(&mut self, by: isize) {
        let n = self.spells.count() as isize;
        if n == 0 {
            return;
        }
        let i = self.spells.index_of(self.effect).unwrap_or(0) as isize;
        if let Some(e) = self.spells.nth((i + by).rem_euclid(n) as usize) {
            self.set_visual(e, self.cast_ms);
        }
    }

    pub fn set_distance(&mut self, d: f32) {
        self.distance = d.clamp(*DISTANCES.start(), *DISTANCES.end());
        self.place();
        self.again();
    }

    /// What the cast lands on: the caster, or a second figure in front of
    /// it (a human, or the training dummy's body), or a human in front and a
    /// third casting at itself behind.
    pub fn set_aim(&mut self, aim: Aim) -> Result<(), String> {
        let body = match aim {
            Aim::Caster => None,
            Aim::Human | Aim::Both => Some((crate::viewer::HUMAN, 1.0)),
            Aim::Dummy => Some(self.dummy),
        };
        if let Some((m, size)) = body.filter(|&(m, size)| m != self.target.model || size != self.target.scale) {
            self.target = Actor::new(&self.lib, m, size, 2)?;
        }
        if aim.twin_on() && self.twin.is_none() {
            self.twin = Some(Actor::new(&self.lib, crate::viewer::HUMAN, 1.0, 3)?);
        }
        if aim != self.aim {
            self.camera = Orbit::for_aim(aim);
        }
        self.aim = aim;
        self.place();
        self.again();
        Ok(())
    }

    /// The body a training dummy stands in (a CHAR.ESF DictID) and its size
    /// (the page may name its server's dummies' row).
    pub fn set_dummy(&mut self, model: i32, size: f32) -> Result<(), String> {
        self.lib.lookup(model).ok_or_else(|| format!("no body {:#010x} in CHAR.ESF", model as u32))?;
        self.dummy = (model, if size > 0.0 { size } else { DUMMY_SIZE });
        if self.aim == Aim::Dummy {
            self.set_aim(Aim::Dummy)?;
        }
        Ok(())
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn aim(&self) -> Aim {
        self.aim
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn effect(&self) -> u32 {
        self.effect
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn cast_ms(&self) -> i32 {
        self.cast_ms
    }

    /// What is playing, for the page's status line and the log.
    pub fn describe(&self) -> String {
        let n = self.spells.count();
        let i = self.spells.index_of(self.effect).map_or(0, |i| i + 1);
        let at = match self.aim {
            Aim::Caster => "at the caster".to_string(),
            Aim::Both => format!("at {} {:.0} off and, by another human, at itself", Aim::Both.name(), self.distance),
            a => format!("at {} {:.0} off", a.name(), self.distance),
        };
        format!("visual {:#010x} ({i} of {n}), cast {:.1} s, {at}", self.effect, self.cast_ms as f32 / 1000.0)
    }

    /// One frame at `now` ms: the figures posed standing, a cast when one is
    /// due (the 0x00B1 the server would send; with `Both` the self-caster's
    /// at itself too, in the same frame), every effect run.
    pub fn step(&mut self, now: i64) {
        self.now = now;
        if let Some(at) = self.cast_at {
            if self.spells.quiet() {
                self.quiet_since.get_or_insert(now);
            } else {
                self.quiet_since = None;
            }
            let rested = self.quiet_since.is_some_and(|q| now - q >= REST_MS) && now - at >= self.cast_ms as i64 + MIN_MS;
            if rested || now - at >= self.cast_ms as i64 + MAX_MS {
                self.due = true;
            }
        }
        let (target_on, twin_on) = (self.aim.target_on(), self.aim.twin_on());
        for a in self.shown() {
            a.pose(now);
        }
        let mut casts = Vec::new();
        if std::mem::take(&mut self.due) {
            let (to_id, to) = if target_on { (TARGET_ID, TARGET) } else { (CASTER_ID, SELF) };
            casts.extend(casting_combat(self.effect, self.cast_ms, CASTER_ID, to_id).map(|c| (c, SELF, to, Vec::new())));
            if twin_on {
                casts.extend(casting_combat(self.effect, self.cast_ms, TWIN_ID, TWIN_ID).map(|c| (c, TWIN, TWIN, Vec::new())));
            }
            self.cast_at = Some(now);
            self.quiet_since = None;
            self.casts += 1;
        }
        let mut bodies = vec![self.caster.body(SELF)];
        if target_on {
            bodies.push(self.target.body(TARGET));
        }
        if let Some(t) = self.twin.as_mut().filter(|_| twin_on) {
            bodies.push(t.body(TWIN));
        }
        self.spells.frame(&mut bodies, &casts, SELF, None, now, &Eye::default(), &self.lib, None);
        drop(bodies);
        for a in [&mut self.caster, &mut self.target].into_iter().chain(self.twin.as_mut()) {
            if let Some(cs) = a.sprite.as_mut() {
                cs.h.set_blend_matrices();
            }
        }
    }

    /// The camera: its eye, the point it looks at, and the frame's view
    /// (fixed daylight, no fog) at `aspect`.
    pub fn view(&self, aspect: f32) -> (View, Mat4, Mat4) {
        let c = &self.camera;
        // round the middle of the two, or the caster alone; of the three (the
        // self-caster behind the caster), nearer the target, whose end of the
        // row the side camera stands nearer, so the row's two ends are as far
        // in from the picture's sides, and closer in than two are framed (a
        // row a third longer, its figures not a third smaller)
        let (centre, reach) = match self.aim {
            Aim::Caster => (Vec3::new(0.0, 1.0, 0.0), 3.4),
            Aim::Both => (Vec3::new(0.0, 1.2, (self.distance - TWIN_GAP) * 0.5 + 0.9), (self.distance + TWIN_GAP) * 0.55 + 2.3),
            _ => (Vec3::new(0.0, 1.2, self.distance * 0.5), self.distance * 0.62 + 2.3),
        };
        let dir = Vec3::new(c.yaw.sin() * c.pitch.cos(), c.pitch.sin(), c.yaw.cos() * c.pitch.cos());
        let eye = centre + dir * reach * c.zoom;
        let v = glam::camera::rh::view::look_at_mat4(eye, centre, Vec3::Y);
        let p = glam::camera::rh::proj::directx::perspective(50f32.to_radians(), aspect.max(0.1), 0.1, 800.0);
        let mut view = View::daylight(p * v);
        view.eye = eye;
        (view, v, p)
    }

    /// The figures on the GPU (each once: a new target's, or the
    /// self-caster's when it is first made, at the next frame).
    pub fn upload(&mut self, r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue) {
        let textures = render::library_textures(&self.lib);
        for a in [&mut self.caster, &mut self.target].into_iter().chain(self.twin.as_mut()) {
            if a.gpu.is_none() {
                a.gpu = a.mesh.as_ref().map(|m| r.upload(device, queue, m, &[render::IDENTITY], &textures));
            }
        }
    }

    /// The frame's models placed (the two posed, the projectiles in flight)
    /// and its particles and lightning handed to the renderer; what to draw
    /// comes back through `draws`.
    pub fn prepare(&mut self, r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, eye: Vec3) {
        self.upload(r, device, queue);
        let eye = Vect3::of(eye.x, eye.y, eye.z);
        for a in [&mut self.caster, &mut self.target].into_iter().chain(self.twin.as_mut()) {
            if let Some(g) = a.gpu.as_mut() {
                g.visible = false;
            }
        }
        for a in self.shown() {
            let Some(g) = a.gpu.as_mut() else { continue };
            r.set_instances(queue, g, &[a.centre.to_cols()]);
            if let Some(cs) = &a.sprite {
                let palette: Vec<[[f32; 4]; 4]> = cs.h.blend.iter().map(|m| m.to_cols()).collect();
                if !palette.is_empty() {
                    r.set_palette(queue, g, &palette);
                }
            }
            crate::lod::place(g, &a.centre, &eye);
            g.visible = true;
        }
        // the projectiles as `RasterActor` draws an actor (play's
        // `draw_projectiles`): a GPU model for each flying at once, kept
        let textures = render::library_textures(&self.lib);
        let mut used: HashMap<u32, usize> = HashMap::new();
        for (id, m) in &self.spells.flying {
            let (model, pool) = self.projectiles.entry(*id).or_insert_with(|| (self.lib.lookup(*id as i32).and_then(|r| self.lib.build_model(r)), Vec::new()));
            let Some(model) = model.as_ref() else { continue };
            let n = used.entry(*id).or_default();
            if *n == pool.len() {
                pool.push(r.upload(device, queue, model, &[m.to_cols()], &textures));
            }
            let g = &mut pool[*n];
            r.set_instances(queue, g, &[m.to_cols()]);
            g.visible = true;
            *n += 1;
        }
        for (id, (_, pool)) in &mut self.projectiles {
            for g in pool.iter_mut().skip(used.get(id).copied().unwrap_or(0)) {
                g.visible = false;
            }
        }
        r.strips = std::mem::take(&mut self.spells.strips);
    }

    pub fn draws(&self) -> Vec<render::Draw<'_>> {
        let bodies = [Some(&self.caster), Some(&self.target), self.twin.as_ref()].into_iter().flatten().filter_map(|a| a.gpu.as_ref()).filter(|g| g.visible);
        let flying = self.projectiles.values().flat_map(|(_, pool)| pool.iter()).filter(|g| g.visible);
        bodies.chain(flying).map(render::Draw::Model).collect()
    }

    pub fn batches(&self) -> &[render::billboard::Batch] {
        &self.spells.batches
    }

    /// The frame drawn into `target`: the two and the projectiles, then the
    /// particles and the lightning over them (`VIScene::Render`'s
    /// particle system, then the spell effects).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(&mut self, r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, enc: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, aspect: f32) {
        let (view, v, p) = self.view(aspect);
        self.prepare(r, device, queue, view.eye);
        r.draw(queue, enc, target, depth, &view, &self.draws());
        let textures = render::library_textures(&self.lib);
        r.draw_billboards(device, queue, enc, target, depth, v, p, &self.spells.batches, &textures);
        r.draw_queued_strips(device, queue, enc, target, depth, v, p, &textures);
    }
}

/// The stage's library out of the files' bytes, by their names on the disc,
/// in [`FILES`]' order whatever order they came in; ITEM.ESF may be left out
/// (only a hand's projectile is in it).
pub fn library(mut files: HashMap<String, Vec<u8>>) -> Result<Library, String> {
    let mut lib = Library::new();
    for name in FILES {
        let key = files.keys().find(|k| crate::pregame::res::name_of(k).eq_ignore_ascii_case(name)).cloned();
        match key.and_then(|k| files.remove(&k)) {
            Some(b) => {
                let c = esf::Container::open(b).map_err(|e| format!("{name}: {e:?}"))?;
                lib.add(*name, c);
            }
            None if *name == "ITEM.ESF" => {}
            None => return Err(format!("{name} was not handed over")),
        }
    }
    Ok(lib)
}

/// The stage's library off a disc image (natively).
#[cfg(not(target_arch = "wasm32"))]
pub fn library_off(disc: &mut iso::Iso) -> Result<Library, String> {
    let mut files = HashMap::new();
    for name in FILES {
        files.insert(name.to_string(), disc.read(name).map_err(|e| format!("{name}: {e}"))?);
    }
    library(files)
}

/// What a page asks of a running preview (`web::fxview_*`), taken at the
/// next frame.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub enum Command {
    /// A visual and its cast time, and what it lands on when given.
    Play { effect: u32, cast_ms: i32, aim: Option<Aim> },
    Again,
    Distance(f32),
    Aim(Aim),
    /// The training dummy's body and size.
    Dummy(i32, f32),
    Camera(Orbit),
}

thread_local! {
    static INBOX: std::cell::RefCell<Vec<Command>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The stage's last status line, for the page to read.
    static STATUS: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    /// The disc's visuals (`Stage::visual_list`), kept as the stage starts.
    static VISUALS: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The disc's visuals, in its order, once a stage has started (empty before).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn visuals() -> Vec<u32> {
    VISUALS.with(|v| v.borrow().clone())
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn send(c: Command) {
    INBOX.with(|b| b.borrow_mut().push(c));
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn status() -> String {
    STATUS.with(|s| s.borrow().clone())
}

fn say(s: String) {
    #[cfg(target_arch = "wasm32")]
    crate::web::status(&s);
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("fxview: {s}");
    STATUS.with(|b| *b.borrow_mut() = s);
}

impl Stage {
    /// The commands a page sent since the last frame.
    fn take_commands(&mut self) {
        let list = INBOX.with(|b| std::mem::take(&mut *b.borrow_mut()));
        for c in list {
            match c {
                Command::Play { effect, cast_ms, aim } => {
                    if let Some(a) = aim
                        && let Err(e) = self.set_aim(a)
                    {
                        say(e);
                    }
                    if self.set_visual(effect, cast_ms) {
                        say(self.describe());
                    } else {
                        say(format!("visual {effect:#010x} is not on this disc"));
                    }
                }
                Command::Again => self.again(),
                Command::Distance(d) => self.set_distance(d),
                Command::Aim(a) => match self.set_aim(a) {
                    Ok(()) => say(self.describe()),
                    Err(e) => say(e),
                },
                Command::Dummy(m, size) => {
                    if let Err(e) = self.set_dummy(m, size) {
                        say(e);
                    }
                }
                Command::Camera(o) => self.camera = o,
            }
        }
    }
}

/// `eqoa fxview <visual> <out.png> [ms,ms,...] [cast seconds]`: the stage
/// drawn at each time (game ms from the first cast, stepped 16 ms a frame),
/// to `out.png`, or with several times to `out-<ms>.png`, 960x540.
#[cfg(not(target_arch = "wasm32"))]
pub fn shots(lib: Library, effect: Option<u32>, cast_ms: i32, aim: Option<Aim>, times: &[i64], out: &str, camera: Option<Orbit>) -> Result<(), String> {
    let mut stage = Stage::new(lib)?;
    if let Some(a) = aim {
        stage.set_aim(a)?;
    }
    if let Some(e) = effect.map(|e| stage.visual_or_index(e))
        && !stage.set_visual(e, cast_ms)
    {
        return Err(format!("visual {e:#010x} is not on this disc"));
    }
    stage.cast_ms = cast_ms.clamp(0, MAX_CAST_MS);
    if let Some(c) = camera {
        stage.camera = c;
    }
    eprintln!("{}", stage.describe());
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).map_err(|e| e.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).map_err(|e| e.to_string())?;
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let (w, h) = (960u32, 540u32);
    let mut times = times.to_vec();
    times.sort_unstable();
    let mut t = 0i64;
    for &at in &times {
        while t <= at {
            stage.step(t);
            t += 16;
        }
        let (view, v, p) = stage.view(w as f32 / h as f32);
        stage.prepare(&mut r, &device, &queue, view.eye);
        let textures = render::library_textures(&stage.lib);
        let draws = stage.draws();
        let rgba = render::capture(&device, &queue, &mut r, w, h, &view, &draws, Some((v, p, stage.batches(), &textures as &render::Textures)));
        let path = if times.len() == 1 { out.to_string() } else { format!("{}-{at}.png", out.trim_end_matches(".png")) };
        let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(&path).map_err(|e| e.to_string())?), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().and_then(|mut wr| wr.write_image_data(&rgba)).map_err(|e| e.to_string())?;
        let twin = stage.twin.as_ref().filter(|_| stage.aim.twin_on()).map_or(String::new(), |t| format!(", the self-caster's {:?}", t.action()));
        eprintln!("{path}: at {at} ms, the caster's action {:?}, the target's {:?}{twin}, {}", stage.caster.action(), stage.target.action(), stage.spells.describe());
    }
    Ok(())
}

/// The stage in a window (natively) or on the page's canvas (`pending`, in
/// a browser, where the loop is handed to the page and this returns at once).
pub fn run(lib: Library, effect: Option<u32>, cast_ms: i32, aim: Option<Aim>, pending: Option<crate::viewer::PendingGfx>) -> Result<(), String> {
    let mut stage = Stage::new(lib)?;
    let list = stage.visual_list();
    VISUALS.with(|v| *v.borrow_mut() = list);
    if let Some(a) = aim {
        stage.set_aim(a)?;
    }
    if let Some(e) = effect.map(|e| stage.visual_or_index(e))
        && !stage.set_visual(e, cast_ms)
    {
        say(format!("visual {e:#010x} is not on this disc: the first one plays"));
    }
    stage.cast_ms = cast_ms.clamp(0, MAX_CAST_MS);
    say(stage.describe());
    let event_loop = winit::event_loop::EventLoop::new().map_err(|e| e.to_string())?;
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut app = App { stage, pending, gfx: None, start: web_time::Instant::now(), drag: None, cursor: (0.0, 0.0), failed: None };
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(app);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
        if let Some(e) = app.failed.take() {
            return Err(e);
        }
    }
    Ok(())
}

struct Gfx {
    window: std::sync::Arc<winit::window::Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    depth: wgpu::TextureView,
    renderer: Renderer,
}

struct App {
    stage: Stage,
    pending: Option<crate::viewer::PendingGfx>,
    gfx: Option<Gfx>,
    start: web_time::Instant,
    /// Where a drag began (the cursor then), while the button is down.
    drag: Option<(f64, f64)>,
    cursor: (f64, f64),
    failed: Option<String>,
}

impl App {
    fn attach(&mut self, window: std::sync::Arc<winit::window::Window>, surface: wgpu::Surface<'static>, config: wgpu::SurfaceConfiguration, device: wgpu::Device, queue: wgpu::Queue) {
        surface.configure(&device, &config);
        let depth = render::depth_view(&device, config.width.max(1), config.height.max(1));
        let mut renderer = Renderer::new(&device, &queue, config.format);
        self.stage.upload(&mut renderer, &device, &queue);
        window.request_redraw();
        self.gfx = Some(Gfx { window, surface, config, device, queue, depth, renderer });
    }

    fn frame(&mut self) {
        self.stage.take_commands();
        let now = self.start.elapsed().as_millis() as i64;
        self.stage.step(now);
        let Some(g) = self.gfx.as_mut() else { return };
        let frame = match g.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            // nothing to draw on: what was written goes with an empty
            // submission (wgpu keeps queued writes until one), the surface
            // set up again
            _ => {
                g.queue.submit(std::iter::empty());
                g.surface.configure(&g.device, &g.config);
                g.window.request_redraw();
                return;
            }
        };
        let tv = frame.texture.create_view(&Default::default());
        let mut enc = g.device.create_command_encoder(&Default::default());
        let aspect = g.config.width as f32 / g.config.height.max(1) as f32;
        self.stage.draw(&mut g.renderer, &g.device, &g.queue, &mut enc, &tv, &g.depth, aspect);
        g.queue.submit([enc.finish()]);
        g.queue.present(frame);
        g.window.request_redraw();
    }
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        if let Some(p) = self.pending.take() {
            match el.create_window(p.attrs) {
                Ok(w) => self.attach(std::sync::Arc::new(w), p.surface, p.config, p.device, p.queue),
                Err(e) => say(format!("no window: {e}")),
            }
            return;
        }
        let attrs = winit::window::Window::default_attributes().with_title("EQOA spell visual").with_inner_size(winit::dpi::LogicalSize::new(960.0, 540.0));
        #[cfg(not(target_arch = "wasm32"))]
        let attrs = crate::icon::attributes(attrs);
        let window = match el.create_window(attrs) {
            Ok(w) => std::sync::Arc::new(w),
            Err(e) => {
                self.failed = Some(format!("no window: {e}"));
                el.exit();
                return;
            }
        };
        let r: Result<(), String> = (|| {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(el.owned_display_handle())));
            let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
            let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })).map_err(|e| e.to_string())?;
            let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).map_err(|e| e.to_string())?;
            let size = window.inner_size();
            let mut config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).ok_or("the surface cannot be configured")?;
            config.format = config.format.remove_srgb_suffix();
            self.attach(window.clone(), surface, config, device, queue);
            Ok(())
        })();
        if let Err(e) = r {
            self.failed = Some(e);
            el.exit();
        }
    }

    fn window_event(&mut self, el: &winit::event_loop::ActiveEventLoop, _id: winit::window::WindowId, event: winit::event::WindowEvent) {
        use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
        use winit::keyboard::{Key, NamedKey};
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => {
                if let Some(g) = self.gfx.as_mut() {
                    g.config.width = size.width.max(1);
                    g.config.height = size.height.max(1);
                    g.surface.configure(&g.device, &g.config);
                    g.depth = render::depth_view(&g.device, g.config.width, g.config.height);
                    g.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.frame(),
            WindowEvent::CursorMoved { position, .. } => {
                let (x, y) = (position.x, position.y);
                if self.drag.is_some() {
                    let (dx, dy) = (x - self.cursor.0, y - self.cursor.1);
                    self.stage.camera.drag(dx as f32, dy as f32);
                }
                self.cursor = (x, y);
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left | MouseButton::Right, .. } => {
                self.drag = (state == ElementState::Pressed).then_some(self.cursor);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let steps = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 60.0,
                };
                self.stage.camera.wheel(steps);
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && !event.repeat => match event.logical_key {
                Key::Named(NamedKey::ArrowRight) => {
                    self.stage.step_visual(1);
                    say(self.stage.describe());
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    self.stage.step_visual(-1);
                    say(self.stage.describe());
                }
                Key::Named(NamedKey::Space) => self.stage.again(),
                Key::Named(NamedKey::Escape) => {
                    #[cfg(not(target_arch = "wasm32"))]
                    el.exit();
                }
                Key::Character(ref c) if c.eq_ignore_ascii_case("r") => self.stage.camera = Orbit::for_aim(self.stage.aim()),
                _ => {}
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 0x00B1 the stage builds is read back as the cast it means, and
    /// visuals parse as the panel writes them.
    #[test]
    fn the_cast_is_a_casting_combat() {
        let c = casting_combat(0x1DAE_B5B6, 2500, CASTER_ID, TARGET_ID).expect("read back");
        assert_eq!((c.caster, c.target, c.time, c.effect), (CASTER_ID, TARGET_ID, 2500, 0x1DAE_B5B6));
        assert!(c.targets.is_empty());
        // a cast at oneself names the caster twice, as a server's does
        let me = casting_combat(0x1DAE_B5B6, 0, TWIN_ID, TWIN_ID).expect("read back");
        assert_eq!((me.caster, me.target), (TWIN_ID, TWIN_ID));
        assert_eq!(
            [Aim::parse("self"), Aim::parse("Human"), Aim::parse("dummy"), Aim::parse(" both"), Aim::parse("x")],
            [Some(Aim::Caster), Some(Aim::Human), Some(Aim::Dummy), Some(Aim::Both), None]
        );
        assert_eq!(parse_visual("0x1DAEB5B6"), Some(0x1DAE_B5B6));
        assert_eq!(parse_visual("1DAEB5B6"), Some(0x1DAE_B5B6));
        assert_eq!(parse_visual("12"), Some(12));
        assert_eq!(parse_visual("nope"), None);
        // the control: a payload cut short is no cast
        let mut b = Vec::new();
        b.extend(CASTER_ID.to_le_bytes());
        assert!(crate::spells::cast_of(0x00B1, &b).is_none(), "the control must fail");
    }

    /// The stage off the disc (SKIPs without run/eqoa.iso): a visual with a
    /// casting part plays on the caster (its action changes from standing),
    /// its particles appear, and once nothing of it is left it is cast again;
    /// a DictID the disc lacks is refused and changes nothing.
    #[test]
    fn a_visual_plays_and_plays_again() {
        let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&iso_path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let lib = library_off(&mut disc).expect("the files");
        let mut stage = Stage::new(lib).expect("a stage");
        // a visual whose first event list moves the caster: the first one
        // that changes the caster's action within its cast time
        let standing = {
            stage.step(0);
            stage.caster.action()
        };
        let n = stage.spells.count();
        assert!(n > 200, "SPELLFX.CSF's visuals: {n}");
        let mut found = None;
        for i in 0..n {
            let e = stage.spells.nth(i).unwrap();
            assert!(stage.set_visual(e, 2000));
            let (mut moved, mut parts) = (false, 0usize);
            for t in (16..3000).step_by(16) {
                stage.step(t);
                moved |= stage.caster.action() != standing;
                parts = parts.max(stage.batches().iter().map(|b| b.quads.len()).sum());
            }
            if moved && parts > 0 {
                found = Some(e);
                break;
            }
        }
        let e = found.expect("a visual that moves its caster and shows particles");
        assert!(stage.set_visual(e, 1000));
        let before = stage.casts;
        let mut t = 3000;
        while stage.casts < before + 2 && t < 3000 + 1000 + MAX_MS + 2000 {
            stage.step(t);
            t += 16;
        }
        assert_eq!(stage.casts, before + 2, "cast again once its last one is over ({})", stage.describe());
        // the control: a visual not on the disc is refused, the one playing stays
        assert!(!stage.set_visual(0x0BAD_F00D, 2000));
        assert_eq!(stage.effect(), e);
    }

    /// The community page's list of visuals (SKIPs without run/eqoa.iso):
    /// every visual on the disc once, in its order, each one the stage plays.
    #[test]
    fn the_visual_list_is_every_visual_the_stage_plays() {
        let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&iso_path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut stage = Stage::new(library_off(&mut disc).expect("the files")).expect("a stage");
        let list = stage.visual_list();
        assert_eq!(list.len(), stage.spells.count());
        assert!(list.len() > 200, "SPELLFX.CSF's visuals: {}", list.len());
        assert_eq!(list[0], stage.spells.nth(0).unwrap(), "in the disc's order");
        let mut seen = std::collections::HashSet::new();
        assert!(list.iter().all(|e| seen.insert(*e)), "each visual once");
        assert!(list.iter().all(|&e| stage.set_visual(e, 1000)), "each one plays");
        // the control: a DictID the list lacks is one the stage refuses
        assert!(!list.contains(&0x0BAD_F00D) && !stage.set_visual(0x0BAD_F00D, 1000));
    }

    /// `Both` (SKIPs without run/eqoa.iso): one step casts twice, the
    /// caster's at the target and the self-caster's at itself, each its own
    /// playback; the target is a human at its full size, not the dummy; the
    /// three stand in a row, apart, and all three are in the picture, where
    /// the one-target framing leaves the self-caster at its edge, half cut
    /// (the control); back to `Human`, one cast at two figures.
    #[test]
    fn both_casts_at_a_human_and_at_itself() {
        let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&iso_path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut stage = Stage::new(library_off(&mut disc).expect("the files")).expect("a stage");
        let e = stage.spells.nth(0).unwrap();
        stage.set_aim(Aim::Both).expect("the self-caster's body");
        assert!(stage.set_visual(e, 2000));
        stage.step(0);
        assert_eq!(stage.spells.playing(), 2, "two casts in one step ({})", stage.describe());
        assert_eq!(stage.shown().count(), 3);
        assert_eq!((stage.target.model, stage.target.scale), (crate::viewer::HUMAN, 1.0), "a human at full size");
        // where they stand (mid-body), and where the camera puts them at 16:9
        let at = |a: &Actor| Mat4::from_cols_array_2d(&a.transform.to_cols()).transform_point3(Vec3::Y);
        let (twin, caster, target) = (at(stage.twin.as_ref().unwrap()), at(&stage.caster), at(&stage.target));
        assert!(twin.z < caster.z - 3.0 && target.z > caster.z + 3.0, "in a row, apart: {twin} {caster} {target}");
        let across = |stage: &Stage, p: Vec3| {
            let (_, v, proj) = stage.view(16.0 / 9.0);
            let c = proj * v * p.extend(1.0);
            (c.x / c.w, c.y / c.w, c.w > 0.0)
        };
        for (who, p) in [("self-caster", twin), ("caster", caster), ("target", target)] {
            let (x, y, ahead) = across(&stage, p);
            assert!(ahead && x.abs() < 0.8 && y.abs() < 0.8, "the {who} in the picture: {x:.2}, {y:.2}");
        }
        let (left, _, _) = across(&stage, twin);
        let (right, _, _) = across(&stage, target);
        assert!((left + right).abs() < 0.1, "the row's ends as far in from the sides: {left:.2} {right:.2}");
        // the control: one cast at a second human, the self-caster not shown
        // and, where it stood, at the one-target picture's edge, half cut
        stage.set_aim(Aim::Human).expect("a human");
        stage.step(16);
        assert_eq!((stage.spells.playing(), stage.shown().count()), (1, 2), "{}", stage.describe());
        let (x, _, _) = across(&stage, twin);
        assert!(x < -0.9, "the self-caster's place at the one-target picture's edge: {x:.2}");
    }
}
