//! The character on the ground: the ported `VIWalkCamera` (crate `vi`, on the
//! console's float model), driven as CLIENT's `VIClient::UpdateControls`
//! drives it in each camera mode (`controls`), then `VIWalkCamera::Process`,
//! which steps everything in 32 ms ticks. The walk keeps the console's
//! frames whatever the display's rate, and the screen shows it at the frame
//! drawn (`Kept`).
//!
//! The scene it collides with is the game's own: `vi::world::World`, the
//! world and zone BSPs, rooms, collision buffers and placed actors, loaded
//! from the world file (`VIScene::Collide` and `GetEnvFlags`, ported).

use glam::{Mat4, Vec3};
use vi::camera::{Eye, WalkCamera};
use vi::coll::CollSphere;
use vi::fpu::{Ee, F};
use vi::scene::{Filter, FilterKind};
use vi::vect::Vect3;
use vi::walk::WalkPhysics;
use vi::walk::flags::{NO_TURN, ON_FLOOR, SUBMERGED};
use vi::character::{AnimCache, load_csprite};
use vi::csprite::{CSprite, Rand};
use vi::geom::Matrix44;
use vi::hsprite::AnimScene;
use vi::world::World;

/// One frame's controls from the keyboard or a script, each in -1..1: a
/// pad's two sticks, as `UpdateControls` reads them (`axes`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    pub fwd: f32,
    pub strafe: f32,
    pub orbit: f32,
    pub pitch: f32,
}

impl Controls {
    /// The sticks (rx, ry, lx, ly, +y down): forward is the left stick up.
    pub fn axes(&self) -> [f32; 4] {
        [self.orbit, self.pitch, self.strafe, -self.fwd]
    }
}

/// How a walk's first place is stood on: as the message that gave it stands
/// the character there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placing {
    /// No height to go by (a walk with no server): the topmost floor under
    /// (x, z), probed from high above (`ground`).
    Ground,
    /// The 0x000D dump's (a login, a respawn): `Msg000D`'s `SetLocation` at
    /// the dump's place as it is (viclient.cpp:9826), no floor looked for.
    AsGiven,
    /// 0x00B2's: `Respawn`'s casts round the server's place
    /// (`arrival_floor`).
    Respawn,
    /// The GM's Goto (`VIClient::MoveTo`): a cast from 300 above the place
    /// to 300 below (`move_to_floor`), the place a grid square's centre at
    /// the world's own height.
    MoveTo,
}

/// Where a walk begins: the place, the heading, and how the place is stood
/// on (a `Placing::Ground` place's height is not read).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spawn {
    pub pos: [f32; 3],
    pub heading: f32,
    pub placing: Placing,
}

impl Spawn {
    /// A place with no height to go by: the ground under (x, z).
    pub fn ground(x: f32, z: f32, heading: f32) -> Self {
        Spawn { pos: [x, 0.0, z], heading, placing: Placing::Ground }
    }
}

pub struct Walker {
    pub cam: WalkCamera<Ee>,
    pub scene: World<Ee>,
    pub eye: Eye<Ee>,
    /// The character, animated: the game's VICSprite over the clips it names.
    pub sprite: Option<CSprite<Ee>>,
    pub anims: AnimScene<Ee>,
    rand: Rand,
    /// The camera shake's sample the last of the game's frames applied
    /// (`frame_with`).
    shake: i32,
    /// The follow orbit's distance the player chose, and the way back out to
    /// it after a pull (`camreach`, the client's own).
    pub reach: crate::camreach::Reach,
    /// The walk on the console's frames (`frame_with`); None before the
    /// first frame.
    kept: Option<Kept>,
}

/// The console's frame: two fields of NTSC video (59.94 Hz), 33.37 ms. The
/// walk keeps only these, at any display rate (`Kept`).
const CONSOLE_FRAME_MS: f64 = 2.0 * 1001.0 / 60.0;

/// The walk on the console's frames. `VIWalkControl::Process` steps 32 ms
/// ticks and then the rest of the frame, with its accelerations set for the
/// tick (`InitIntermediate`), so the top speed it reaches grows as the rest
/// gets shorter: stepped once a frame drawn, a run on flat ground went 8.85
/// m/s at 30 frames a second, 9.11 at 60 and 9.30 at 165, and a turn 1.575,
/// 1.628 and 1.669 radians a second (and swimming, pitch and falls the same
/// way); out at Tunaria's places, where each frame's add is cut to the
/// floats' step as well, the run changed again with the rate and the
/// heading (east at x 22000: 9.06 m/s at 60, 8.80 at 165, 8.83 at 240). So
/// each frame drawn puts the walk back where the last console frame left it,
/// steps every console frame come since as the console steps one (a tick
/// and the rest) and keeps that: the walk keeps what a console at 30 frames
/// a second keeps.
///
/// Each frame drawn shows the kept frame on its way to the next (`ahead`:
/// Process on a copy, on this frame's controls, so a press shows at once):
/// its place, height offset, heading and pitch as far along as the frame
/// drawn is, in doubles and rounded once, the rest of the walk as the kept
/// frame has it (the floor, the water, the locomotion change on the
/// console's frames, as they do on the console). Process stepped the rest
/// of the way to the frame drawn instead had put the place a float step or
/// two either side of the way (a chopped add onto a kept place that moves a
/// console frame at a time: 4 mm at Freeport's 22000), where the stepping it
/// replaced, adding the same step every frame, moved as evenly as the floats
/// allow; and Process then ran every frame drawn, where now it runs twice a
/// console frame (the frame and the one ahead), whatever the display's
/// rate.
#[derive(Clone, Copy)]
struct Kept {
    /// The console frames' first time (ms), and the frames counted from it.
    origin: i64,
    frames: i64,
    /// The walk (`phys`, `committed`) as the last console frame left it, no
    /// frame's forces in it.
    at: (WalkPhysics<Ee>, WalkPhysics<Ee>),
    /// The walk as the last frame drawn left it.
    left: (WalkPhysics<Ee>, WalkPhysics<Ee>),
    /// The next console frame, the frame it follows and the walk (its
    /// controls given) it was stepped from.
    ahead: Option<(i64, WalkPhysics<Ee>, WalkPhysics<Ee>)>,
}

impl Kept {
    /// Console frame `n`'s time: 33 or 34 ms apart, as the console's clock
    /// counts them.
    fn time(&self, n: i64) -> i64 {
        self.origin + (n as f64 * CONSOLE_FRAME_MS) as i64
    }
}

/// The same state, bit for bit.
fn same(a: &WalkPhysics<Ee>, b: &WalkPhysics<Ee>) -> bool {
    let v = |v: &Vect3<Ee>| [v.x.bits(), v.y.bits(), v.z.bits()];
    let all = |p: &WalkPhysics<Ee>| {
        let vs = [v(&p.position), v(&p.angles), v(&p.velocity), v(&p.angular_vel), v(&p.force), v(&p.torque), v(&p.landscape_normal)];
        (p.time, vs, [p.flags, p.floor.bits(), p.loco_state as u32, p.height_offset.bits(), p.target_heading.bits()])
    };
    all(a) == all(b)
}

impl Walker {
    /// `VICSprite::SetAction` on the character: its handle, or None with
    /// no character.
    pub fn set_action(&mut self, id: i32) -> Option<i32> {
        let sc = &self.anims;
        self.sprite.as_mut().map(|s| s.set_action(id, sc, &mut self.rand))
    }

    /// Stand the character at (x, z) on the terrain, facing `heading`, with
    /// the camera behind it.
    pub fn new(scene: World<Ee>, x: f32, z: f32, heading: f32) -> Self {
        Self::arrive(scene, Spawn::ground(x, z, heading))
    }

    /// Stand the character at `spawn`, stood on as its `placing` says, with
    /// the camera behind it. A login stands at the dump's height, as the
    /// console does: a character that logged out in a house or a cave comes
    /// back in it, where the topmost floor (every arrival's until 2026-10-23)
    /// put it on the roof or on the ground above the cave.
    pub fn arrive(scene: World<Ee>, spawn: Spawn) -> Self {
        let mut scene = scene;
        let [x, y, z] = spawn.pos;
        let y = match spawn.placing {
            Placing::Ground => ground(&mut scene, x, z).unwrap_or(0.0),
            Placing::AsGiven => y,
            Placing::Respawn => arrival_floor(&mut scene, spawn.pos, -1).unwrap_or(y),
            Placing::MoveTo => move_to_floor(&mut scene, spawn.pos, -1).unwrap_or(y),
        };
        let mut cam = WalkCamera::<Ee>::default();
        let pos = Vect3::of(x, y, z);
        let angles = Vect3::of(spawn.heading, 0.0, 0.0);
        cam.init(pos, angles, &mut scene);
        // What a camera preset does (`VIClient::ApplyCameraPreset`): mode,
        // pitch, distance, angle. The preset's own numbers are not read yet;
        // these are the walk camera's defaults.
        cam.set_camera_mode(3);
        let pitch = cam.def_orbit_pitch;
        cam.set_orbit_pitch_angle(pitch);
        let dist = cam.def_orbit_distance;
        cam.set_orbit_distance(dist);
        cam.set_orbit_angle_behind();
        let eye = cam.process(&mut scene);
        Walker { cam, scene, eye, sprite: None, anims: AnimScene::default(), rand: Rand(1), shake: 0, reach: Default::default(), kept: None }
    }

    /// `Respawn`'s placing: the character stood at `pos` (on the ground
    /// when the zone there is loaded, else at the height given), facing
    /// `heading`, the camera set behind.
    pub fn relocate(&mut self, pos: [f32; 3], heading: f32) {
        let y = arrival_floor(&mut self.scene, pos, self.cam.walk.parm64).unwrap_or(pos[1]);
        self.place([pos[0], y, pos[2]], heading);
    }

    /// `MoveTo`'s placing: the character stood on the ground its cast finds
    /// within 300 of `pos` (else at the height given, the zones there not
    /// read yet), facing `heading`, the camera set behind.
    pub fn move_to(&mut self, pos: [f32; 3], heading: f32) {
        let y = move_to_floor(&mut self.scene, pos, self.cam.walk.parm64).unwrap_or(pos[1]);
        self.place([pos[0], y, pos[2]], heading);
    }

    /// `Msg000D`'s placing in play: the character stood at the dump's place
    /// as it is, facing `heading`, the camera set behind (viclient.cpp:9826).
    pub fn place(&mut self, pos: [f32; 3], heading: f32) {
        let p = Vect3::of(pos[0], pos[1], pos[2]);
        // SetLocation (not Init: the camera keeps its mode and distance),
        // then SetOrbitAngleBehind.
        self.cam.set_location(p, Vect3::of(heading, 0.0, 0.0), &mut self.scene);
        self.cam.set_orbit_angle_behind();
        self.eye = self.cam.process(&mut self.scene);
    }

    /// The character put back at `at` (no floor looked for), facing
    /// `heading`, the camera behind, when it has moved off it at all: what
    /// the loading screen holds the walk at while the game's frame would
    /// stop (`loading.rs`).
    pub fn hold(&mut self, at: [f32; 3], heading: f32) {
        if self.cam.walk.phys.position.to_array() == at {
            return;
        }
        self.cam.set_location(Vect3::of(at[0], at[1], at[2]), Vect3::of(heading, 0.0, 0.0), &mut self.scene);
        self.cam.set_orbit_angle_behind();
        self.eye = self.cam.process(&mut self.scene);
    }

    /// Loads the character as a VICSprite from `r` (a 0x2700 record).
    pub fn set_character(&mut self, lib: &esf::Library, r: esf::Ref) {
        // the clips and ref maps the character before brought go with it:
        // each load adds its own, and a form taken and dropped grew the
        // table without end (a human's 80 clips each time)
        self.anims = AnimScene { time: self.scene.time, ..Default::default() };
        let mut cache = AnimCache::new();
        self.sprite = load_csprite(lib, r, &mut self.anims, &mut cache, &mut self.rand);
        match &self.sprite {
            Some(s) => eprintln!("character: {} nodes, {} clips, skeleton type {}", s.h.nodes.len(), self.anims.anims.len(), s.skel_type),
            None => eprintln!("character: not a CSprite with a play list; drawn in its bind pose"),
        }
    }

    /// The actor's transform: the body's render position, turned to its
    /// heading (`VIMatrix44::Transform(hpr, 1, pos)`).
    fn actor(&self) -> Matrix44<Ee> {
        let p = self.cam.walk.phys.smooth_location();
        let hpr = Vect3::new(self.cam.walk.phys.angles.x, F::ZERO, F::ZERO);
        Matrix44::transform(&hpr, F::ONE, &p)
    }

    /// The character's model matrix and skin palette for this frame:
    /// `UpdateSprite`, then `Process` at the LOD for its distance, then the
    /// centre transform and `SetBlendMatrices`.
    pub fn pose(&mut self) -> ([[f32; 4]; 4], Vec<[[f32; 4]; 4]>) {
        self.pose_sounding(None)
    }

    /// `pose`, with the sounds its animation's triggers fire played between
    /// `UpdateSprite` and the pose, as `VIClient` calls `VICSprite::Process`.
    pub fn pose_sounding(&mut self, sound: Option<&mut dyn vi::csprite::SoundSink>) -> ([[f32; 4]; 4], Vec<[[f32; 4]; 4]>) {
        let actor = self.actor();
        let Some(cs) = self.sprite.as_mut() else { return (actor.to_cols(), Vec::new()) };
        self.anims.time = self.scene.time;
        self.cam.walk.update_sprite(cs, &self.anims, &mut self.rand);
        if sound.is_some() {
            let at = self.cam.walk.phys.smooth_location();
            cs.process_sounds(&self.eye.position, &self.eye.direction, &at, sound);
        }
        // the skeleton's level from no farther than the game's camera could
        // be, as the mesh's (`lod::eye`): zoomed out, the camera only moves
        // back; every node with Level of Detail off
        let eye = crate::lod::eye(&self.cam, &self.eye.position);
        let lod = crate::lod::anim_level(&cs.h, &actor, &eye, crate::settings::client().lod);
        cs.h.process(lod, &self.anims);
        let model = cs.h.calc_trans_about_center(&actor, &self.anims);
        cs.h.set_blend_matrices();
        (model.to_cols(), cs.h.blend.iter().map(|m| m.to_cols()).collect())
    }

    /// One frame: the controls, then the clock moved to `now_ms` and stepped.
    pub fn frame(&mut self, c: Controls, now_ms: i64) {
        self.frame_with(c.axes(), now_ms, &crate::controls::Client::default(), true);
    }

    /// One frame on the sticks `keys` (rx, ry, lx, ly) for the client
    /// `c`: `UpdateGame`'s run speed, `UpdateControls`, then the clock moved
    /// to `now_ms` and `Process`. What `UpdateControls` changed of the client
    /// comes back (autorun, the follow). `game_frame`: one of the game's
    /// frames (`frames.rs`). Everything here steps by the clock but the
    /// camera shake (`VICameraEffect::Apply` takes the next of its samples a
    /// call) and the manual camera's look (`UpdateControls`, a step a frame),
    /// which move on only then: between them the eye is shaken by the sample
    /// the last of the game's frames took. The camera's `Process` goes
    /// through `reach` (the follow orbit's way back out). The walk keeps the
    /// console's frames (`Kept`): `UpdateControls` reads it as the last of
    /// them left it, and it is drawn on the way to the next, where the
    /// camera's `Process` finds it.
    pub fn frame_with(&mut self, keys: [f32; 4], now_ms: i64, c: &crate::controls::Client, game_frame: bool) -> crate::controls::Changed {
        self.rewind();
        // UpdateGame sets the run speed and calls InitIntermediate every
        // frame (SetSpeed the walk speed, when one comes); with the speeds
        // unchanged that changes nothing the next `Process` reads, so it is
        // done when one moves.
        let (run, walk) = (F::<Ee>::new(c.run_speed), F::<Ee>::new(c.walk_speed));
        if self.cam.walk.run_speed.bits() != run.bits() || self.cam.walk.walk_speed.bits() != walk.bits() {
            self.cam.walk.run_speed = run;
            self.cam.walk.walk_speed = walk;
            self.cam.walk.init_intermediate();
        }
        let c = crate::controls::Client { step: game_frame, ..*c };
        let changed = crate::controls::update_controls(self, keys, &c);
        let dt_s = (now_ms - self.scene.time).clamp(0, 100) as f32 / 1000.0;
        self.step_kept(now_ms);
        let ahead = self.ahead();
        self.scene.time = now_ms;
        self.draw_between(&ahead, now_ms);
        let next = self.cam.effect.sample;
        if !game_frame {
            self.cam.effect.sample = self.shake;
        }
        self.eye = self.reach.process(&mut self.cam, &mut self.scene, dt_s);
        if game_frame {
            self.shake = next;
        } else {
            self.cam.effect.sample = next;
        }
        self.keep_left();
        changed
    }

    /// The walk put back where the last console frame left it (`Kept`),
    /// with the directions it steers by made again from it. A walk moved
    /// since the last frame drawn (a place given, the loading screen's hold,
    /// the first frame) is not put back: the console frames begin where it
    /// stands.
    fn rewind(&mut self) {
        let w = &mut self.cam.walk;
        match &self.kept {
            Some(k) if same(&w.phys, &k.left.0) && same(&w.committed, &k.left.1) => {
                w.phys = k.at.0;
                w.committed = k.at.1;
                w.update_directions();
            }
            _ => {
                let mut at = w.phys;
                at.force = Vect3::default();
                at.torque = Vect3::default();
                self.kept = Some(Kept { origin: w.committed.time, frames: 0, at: (at, w.committed), left: (w.phys, w.committed), ahead: None });
            }
        }
    }

    /// `Process` for every console frame come by `now`, each on this frame's
    /// controls (the forces `UpdateControls` gave, given again to each, as
    /// Process clears them), and the walk kept as the last of them left it.
    /// A gap past the walk's own 200 ms (a stall, a hidden window) is one
    /// frame, as the console takes a long one: Process steps its last 200 ms
    /// and skips the rest, and the console frames begin again there.
    fn step_kept(&mut self, now: i64) {
        let Walker { cam, scene, kept, .. } = self;
        let Some(k) = kept.as_mut() else { return };
        let w = &mut cam.walk;
        let (force, torque) = (w.phys.force, w.phys.torque);
        if now - k.time(k.frames) > w.tick_alt_ms as i64 {
            scene.time = now;
            w.process(0, scene);
            (k.origin, k.frames, k.ahead) = (now, 0, None);
        }
        while k.time(k.frames + 1) <= now {
            k.frames += 1;
            w.phys.force = force;
            w.phys.torque = torque;
            scene.time = k.time(k.frames);
            w.process(0, scene);
        }
        let mut at = w.phys;
        at.force = Vect3::default();
        at.torque = Vect3::default();
        k.at = (at, w.committed);
        // the forces again, for the frame ahead
        w.phys.force = force;
        w.phys.torque = torque;
    }

    /// Where the next console frame will keep the walk if this frame's
    /// controls hold: `Process` on a copy, stepped again only when the frame
    /// kept or the controls (all the walk they write) have changed.
    fn ahead(&mut self) -> WalkPhysics<Ee> {
        let Walker { cam, scene, kept, .. } = self;
        let Some(k) = kept.as_mut() else { return cam.walk.phys };
        if let Some((n, from, next)) = &k.ahead
            && *n == k.frames
            && same(from, &cam.walk.phys)
        {
            return *next;
        }
        let mut w = cam.walk;
        let now = scene.time;
        scene.time = k.time(k.frames + 1);
        w.process(0, scene);
        scene.time = now;
        k.ahead = Some((k.frames, cam.walk.phys, w.phys));
        w.phys
    }

    /// The walk drawn: the console frame kept on its way to the next
    /// (`ahead`), as far along as `now` is between them, its place, height
    /// offset, heading (the short way round) and pitch in doubles and
    /// rounded once (`Kept`'s doc says why), the rest of it the kept frame's,
    /// at `now` (so the camera's `Process` steps it no further). The next
    /// frame drawn puts the kept frame back.
    fn draw_between(&mut self, ahead: &WalkPhysics<Ee>, now: i64) {
        let Some(k) = &self.kept else { return };
        let (t0, t1) = (k.time(k.frames), k.time(k.frames + 1));
        let a = ((now - t0) as f64 / (t1 - t0) as f64).clamp(0.0, 1.0);
        let (from, to) = (&k.at.0, ahead);
        let on = |u: F<Ee>, v: F<Ee>| F::new((u.0 as f64 + (v.0 as f64 - u.0 as f64) * a) as f32);
        let mut p = *from;
        p.time = now;
        p.position = Vect3::new(on(from.position.x, to.position.x), on(from.position.y, to.position.y), on(from.position.z, to.position.z));
        p.height_offset = on(from.height_offset, to.height_offset);
        let tau = std::f64::consts::TAU;
        let turn = (to.angles.x.0 as f64 - from.angles.x.0 as f64 + std::f64::consts::PI).rem_euclid(tau) - std::f64::consts::PI;
        p.angles.x = F::new(((from.angles.x.0 as f64 + turn * a) % tau) as f32);
        p.angles.y = on(from.angles.y, to.angles.y);
        let w = &mut self.cam.walk;
        (w.phys, w.committed) = (p, p);
    }

    /// After the camera's `Process`, what is kept of the frame drawn. The
    /// camera sets the walk's NO_TURN as it swings round what is in its way
    /// and clears it once round, and nothing else touches it: the kept walk
    /// takes it as the camera left it. The landing (`Process`'s 16 in
    /// `parm118`, the sprite's locomotion 7) is told between two frames
    /// drawn, as one Process a frame drawn told it: on the first frame that
    /// stands, once (the console frame's own call tells it, and the camera's
    /// call, with no time to step, clears it again).
    fn keep_left(&mut self) {
        let Some(k) = self.kept.as_mut() else { return };
        let w = &mut self.cam.walk;
        k.at.0.flags = (k.at.0.flags & !NO_TURN) | (w.phys.flags & NO_TURN);
        let (old, new) = (k.left.0.flags, w.phys.flags);
        let landed = old & ON_FLOOR == 0 && new & ON_FLOOR != 0 && new & SUBMERGED == 0 && old & SUBMERGED == 0;
        w.parm118 = if landed { 16 } else { 0 };
        k.left = (w.phys, w.committed);
    }


    /// The projection and the view matrix through the game's camera: its far
    /// plane the terrain profiles' at the eye (`CalcFarPlane`, through the
    /// Extended page's Fog, `fog::far_plane`).
    pub fn view(&self, aspect: f32) -> (Mat4, Mat4) {
        let eye = Vec3::from(self.eye.position.to_array());
        let dir = Vec3::from(self.eye.direction.to_array());
        let far = crate::fog::far_plane(&self.scene, eye, crate::settings::fog());
        (projection_to(aspect, far), glam::camera::rh::view::look_to_mat4(eye, dir.normalize_or(Vec3::Z), Vec3::Y))
    }

    /// Where the camera is.
    pub fn eye_position(&self) -> Vec3 {
        Vec3::from(self.eye.position.to_array())
    }

    pub fn describe(&self) -> String {
        let env = self.scene.env_flags(&self.cam.walk.phys.position);
        let p = &self.cam.walk.phys;
        format!(
            "at ({:.1}, {:.1}, {:.1}) heading {:.2} loco {} flags {:#06x} env {env:#x} cam mode {} dist {:.2} pitch {:.3} (default {:.3}) anim {:?}",
            p.position.x.0,
            p.position.y.0,
            p.position.z.0,
            p.angles.x.0,
            p.loco_state,
            p.flags,
            self.cam.camera_mode,
            self.cam.cur.dist.0,
            self.cam.cur.orbit.y.0,
            self.cam.def_orbit_pitch.0,
            self.sprite.as_ref().map(|s| [s.cur[0].anim, s.cur[1].anim])
        )
    }
}

/// `m_camera` in play (`VIClient`'s `Init`, viclient.cpp): 55 degrees of the
/// picture's height (`VICamera::Init`'s field of view is vertical) and the
/// frame's own shape across it, 640 by 448: 10/7.
pub const FOV: f32 = 55.0;
pub const FRAME_ASPECT: f32 = 10.0 / 7.0;

/// The world's projection for a window `aspect` wide: the game's camera as a
/// TV shows it, the frame's 10/7 across a 4:3 picture, carried on in step
/// past a 4:3 picture's sides in a wider window (the height's view the
/// game's, more of the world at the sides), and past its top and bottom in a
/// narrower one (two windows side by side, a phone held upright: the width's
/// view the game's, as the HUD's frame fills the width, more of the world
/// above and below); near and far the client's. Until 2026-10-23 it was 45
/// degrees at the window's own shape, the world a quarter larger than the
/// game draws it: the start menu's view of the character took its head off
/// (F3 20261023-091325); a narrower window kept the height's 55 degrees and
/// so drew the world larger again, by the window's height over the frame's.
#[cfg(test)]
pub fn projection(aspect: f32) -> Mat4 {
    projection_to(aspect, 6000.0)
}

/// `projection` ending at `far` (the game's far plane, `fog::far_plane`).
pub fn projection_to(aspect: f32, far: f32) -> Mat4 {
    glam::camera::rh::proj::directx::perspective(fov_y(aspect), FRAME_ASPECT * aspect * 0.75, 0.3, far)
}

/// The height's field of view for a window `aspect` wide, in radians: the
/// game's 55 degrees in a 4:3 window or a wider one; in a narrower one what
/// keeps the 4:3 picture's view across the width (the frame fits the width
/// there, `render::ui`), 55 degrees over the frame's height in the middle.
fn fov_y(aspect: f32) -> f32 {
    let fov = FOV.to_radians();
    if aspect >= 4.0 / 3.0 {
        return fov;
    }
    2.0 * ((fov * 0.5).tan() * (4.0 / 3.0) / aspect).atan()
}

#[cfg(test)]
mod view_tests {
    use super::*;

    /// Where a point `tan` up and `tan` across (from the eye, looking down
    /// -Z) lands on the screen, in normalised device coordinates.
    fn ndc(proj: Mat4, across: f32, up: f32) -> (f32, f32) {
        let p = proj * glam::Vec4::new(across, up, -1.0, 1.0);
        (p.x / p.w, p.y / p.w)
    }

    /// The world is framed as the game frames it: in a 4:3 window the top of
    /// `VICamera::Init`'s frustum (tan 27.5 degrees) is the picture's top and
    /// its side (that times 10/7) the picture's side; in a 16:9 window the
    /// height the same and the side a third further out. The control: the
    /// 45 degrees it had put the game's top a quarter past the picture's.
    #[test]
    fn the_world_is_framed_as_the_games_camera_frames_it() {
        let t = (FOV.to_radians() * 0.5).tan();
        let tv = projection(4.0 / 3.0);
        let (x, y) = ndc(tv, t * FRAME_ASPECT, t);
        assert!((x - 1.0).abs() < 1e-5 && (y - 1.0).abs() < 1e-5, "4:3: ({x}, {y})");
        let wide = projection(16.0 / 9.0);
        let (x, y) = ndc(wide, t * FRAME_ASPECT * 4.0 / 3.0, t);
        assert!((x - 1.0).abs() < 1e-5 && (y - 1.0).abs() < 1e-5, "16:9: ({x}, {y})");
        let (_, y) = ndc(wide, 0.0, t * 0.5);
        assert!((y - 0.5).abs() < 1e-5, "the height's view unchanged by the width");
        let old = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 4.0 / 3.0, 0.3, 6000.0);
        let (_, y) = ndc(old, 0.0, t);
        assert!(y > 1.2, "the control: 45 degrees puts the game's top at {y}");
    }

    /// In a window narrower than 4:3 (two windows side by side at 798x828,
    /// a phone held upright at 9:16) the frame fills the width, in the middle
    /// of the height: the side of `VICamera::Init`'s frustum is the window's
    /// side, its top the frame's top (the window's height times the frame's
    /// share of it), so the frame shows what a TV shows. The control: the
    /// height's 55 degrees kept puts the game's side well inside the window,
    /// the world drawn larger.
    #[test]
    fn a_narrower_window_keeps_the_width_the_games() {
        let t = (FOV.to_radians() * 0.5).tan();
        for (w, h) in [(798.0f32, 828.0f32), (540.0, 960.0), (640.0, 900.0)] {
            let aspect = w / h;
            let tall = projection(aspect);
            let (x, y) = ndc(tall, t * FRAME_ASPECT, t);
            // the frame's share of the height: its 4:3 picture fits the width
            let share = w * 0.75 / h;
            assert!((x - 1.0).abs() < 1e-5, "{w}x{h}: the side at {x}");
            assert!((y - share).abs() < 1e-5, "{w}x{h}: the top at {y}, the frame's at {share}");
            let kept = glam::camera::rh::proj::directx::perspective(FOV.to_radians(), FRAME_ASPECT * aspect * 0.75, 0.3, 6000.0);
            let (x, _) = ndc(kept, t * FRAME_ASPECT, t);
            assert!(x > 1.2, "the control: the height's 55 degrees put the game's side at {x}");
        }
        // continuous at 4:3, and a 4:3 window is the game's
        assert_eq!(fov_y(4.0 / 3.0), FOV.to_radians());
        assert!((fov_y(4.0 / 3.0 - 1e-6) - FOV.to_radians()).abs() < 1e-5);
        assert_eq!(projection(640.0 / 480.0), glam::camera::rh::proj::directx::perspective(FOV.to_radians(), FRAME_ASPECT * (640.0 / 480.0) * 0.75, 0.3, 6000.0));
    }
}

impl crate::controls::Walk for Walker {
    fn camera_mode(&self) -> i32 {
        self.cam.camera_mode
    }
    fn heading(&self) -> F<Ee> {
        self.cam.walk.committed.angles.x
    }
    fn pitch(&self) -> F<Ee> {
        self.cam.walk.committed.angles.y
    }
    fn orbit_angle(&self) -> F<Ee> {
        self.cam.last.orbit.x
    }
    fn camera_dir(&self) -> Vect3<Ee> {
        self.eye.direction
    }
    fn under_liquid(&self) -> bool {
        self.cam.walk.is_under_liquid()
    }
    fn at_liquid_surface(&self) -> bool {
        self.cam.walk.is_at_liquid_surface()
    }
    fn move_stick(&mut self, x: F<Ee>, y: F<Ee>) {
        self.cam.walk.move_stick(x, y);
    }
    fn walk(&mut self, x: F<Ee>, y: F<Ee>) {
        self.cam.walk.walk(x, y);
    }
    fn turn_left(&mut self, t: F<Ee>) {
        self.cam.walk.turn_left(t);
    }
    fn pitch_down(&mut self, t: F<Ee>) {
        self.cam.walk.pitch_down(t);
    }
    fn swim_up(&mut self, t: F<Ee>) {
        self.cam.walk.swim_up(t);
    }
    fn view_relative_move(&mut self, r: &Vect3<Ee>, x: F<Ee>, y: F<Ee>) {
        self.cam.walk.view_relative_move(r, x, y);
    }
    fn view_relative_walk(&mut self, r: &Vect3<Ee>, x: F<Ee>, y: F<Ee>) {
        self.cam.walk.view_relative_walk(r, x, y);
    }
    fn set_target_heading(&mut self, h: F<Ee>) {
        self.cam.walk.set_target_heading(h);
    }
    fn orbit_in(&mut self, t: F<Ee>) {
        self.cam.orbit_in(t);
    }
    fn orbit_right(&mut self, t: F<Ee>) {
        self.cam.orbit_right(t);
    }
    fn manual_terp(&self) -> F<Ee> {
        self.cam.effect.manual_terp
    }
    fn manual_hpr(&self) -> Vect3<Ee> {
        self.cam.effect.manual_hpr
    }
    fn set_manual_hpr(&mut self, v: Vect3<Ee>) {
        self.cam.effect.manual_hpr = v;
    }
}

/// The ground under (x, z): a floor probe from high above, as CalcFloor drops
/// one.
/// `VIClient::Respawn`'s floor for a move (CLIENT 0x00BD5628, 0x00B2's
/// teleport and a dump in play): a 0.1 sphere cast from 3 above the server's
/// place to 3 below (`VISceneFilter` on the player's own actor, `actor`: the
/// other creatures are in it), the place put 0.2 under where it hits.
/// Its second cast, "20 above the first hit", collides the same sphere
/// again, so the first hit stands. The topmost floor within 20 (the client's
/// rule until 2026-10-21) put a teleport on a stall's roof over the place.
/// Nothing within 3 (the zones not read yet, on a far move) is None here,
/// and the caller keeps the server's height; the game would go to the cast's
/// end, 3.2 under it, safe only because its zones are always in.
pub fn arrival_floor(w: &mut World<Ee>, pos: [f32; 3], actor: i32) -> Option<f32> {
    let y = F::<Ee>::new(pos[1]);
    let mut s = CollSphere::<Ee>::default();
    s.init(Vect3::new(F::new(pos[0]), y + F::new(3.0), F::new(pos[2])), Vect3::new(F::new(pos[0]), y - F::new(3.0), F::new(pos[2])), F::new(0.1));
    let mut f = Filter::new(FilterKind::Scene, actor);
    w.collide_sphere(&mut s, &mut f).then(|| (s.hit_point().y - F::new(0.2)).0)
}

/// `VIClient::MoveTo`'s floor (CLIENT 0x00BD53A0, the GM's Goto): a 0.1
/// sphere cast from 300 above the place to 300 below, the place put 0.2 under
/// where it hits (its second `Collide` repeats the first: its sphere is not
/// set again). The console casts after `EnterWorld` has streamed the zones
/// round the place in; nothing there (a far move's zones not read yet) is
/// None, and the caller keeps the place's height and casts again once they
/// are (`Play::loading_step`).
pub fn move_to_floor(w: &mut World<Ee>, pos: [f32; 3], actor: i32) -> Option<f32> {
    let y = F::<Ee>::new(pos[1]);
    let mut s = CollSphere::<Ee>::default();
    s.init(Vect3::new(F::new(pos[0]), y + F::new(300.0), F::new(pos[2])), Vect3::new(F::new(pos[0]), y - F::new(300.0), F::new(pos[2])), F::new(0.1));
    let mut f = Filter::new(FilterKind::Scene, actor);
    w.collide_sphere(&mut s, &mut f).then(|| (s.hit_point().y - F::new(0.2)).0)
}

pub fn ground(w: &mut World<Ee>, x: f32, z: f32) -> Option<f32> {
    let mut s = CollSphere::<Ee>::default();
    s.init(Vect3::of(x, 2000.0, z), Vect3::of(x, -2000.0, z), F::new(0.001));
    let mut f = Filter::new(FilterKind::WalkFloor, -1);
    w.collide_sphere(&mut s, &mut f).then(|| s.hit_point().y.0)
}

/// HSprite placements drawn posed: one group per (skeleton, node attachment),
/// instanced over its placements. The game shares one skeleton between every
/// placement of a sprite, so each is evaluated once a frame.
pub struct AnimatedProps {
    pub groups: Vec<PropGroup>,
}

pub struct PropGroup {
    pub hsprite: usize,
    pub node: usize,
    pub model: esf::Model,
    pub actors: Vec<Matrix44<Ee>>,
}

impl AnimatedProps {
    pub fn new(world: &World<Ee>, lib: &esf::Library, placements: &[(esf::Ref, Matrix44<Ee>)]) -> Self {
        let mut groups: Vec<PropGroup> = Vec::new();
        for (r, m) in placements {
            let Some(h) = world.hsprite_of(*r) else { continue };
            for &(sp, node) in &world.hsprites[h].1 {
                if node < 0 {
                    continue;
                }
                let node = node as usize;
                if let Some(g) = groups.iter_mut().find(|g| g.hsprite == h && g.node == node) {
                    g.actors.push(*m);
                } else if let Some(model) = lib.build_model(world.sprite_refs[sp]) {
                    groups.push(PropGroup { hsprite: h, node, model, actors: vec![*m] });
                }
            }
        }
        AnimatedProps { groups }
    }

    /// This frame's instance matrices per group: `node.world × centre × actor`
    /// (`VIHSprite::Raster`).
    pub fn matrices(&self, world: &mut World<Ee>) -> Vec<Vec<[[f32; 4]; 4]>> {
        world.anims.time = world.time;
        let mut posed: std::collections::HashMap<usize, Matrix44<Ee>> = std::collections::HashMap::new();
        for g in &self.groups {
            if let std::collections::hash_map::Entry::Vacant(e) = posed.entry(g.hsprite) {
                let h = &mut world.hsprites[g.hsprite].0;
                h.process(-1, &world.anims);
                e.insert(h.calc_trans_about_center(&Matrix44::identity(), &world.anims));
            }
        }
        self.groups
            .iter()
            .map(|g| {
                let centre = posed[&g.hsprite];
                let node = Matrix44::mul(&world.hsprites[g.hsprite].0.nodes[g.node].world, &centre);
                g.actors.iter().map(|a| Matrix44::mul(&node, a).to_cols()).collect()
            })
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The disc's TUNARIA.ESF with the zones round (x, z) read and the
    /// collision world made; None without `run/eqoa.iso`.
    pub(crate) fn tunaria_round(x: f32, z: f32) -> Option<World<Ee>> {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        }
        let mut lib = esf::Library::new();
        let w = lib.add("TUNARIA.ESF", crate::stream::open_file(&iso, "TUNARIA.ESF").unwrap());
        let mut src = crate::stream::FileZones::open(&iso, "TUNARIA.ESF").unwrap();
        crate::stream::fill(&mut lib, w, x, z, &mut src);
        World::load(&lib, w)
    }

    /// The view's far plane is the game's at the eye (`CalcFarPlane`: the
    /// terrain profiles' far, 800 on Freeport's shore), through Fog: off, the
    /// sky's; thickest, half. The control: the far plane the client had,
    /// 6000, which drew the world past its fog.
    #[test]
    fn the_view_ends_at_the_games_far_plane() {
        let Some(world) = tunaria_round(23508.0, 18925.0) else { return };
        let w = Walker::arrive(world, Spawn::ground(23508.0, 18925.0, 2.74));
        // f = b / (1 + a) for glam's [0, 1]-depth right-handed perspective
        let far = |level: u8| {
            crate::settings::set_client(crate::settings::ClientOptions { fog: level, ..Default::default() });
            let (p, _) = w.view(16.0 / 9.0);
            p.w_axis.z / (1.0 + p.z_axis.z)
        };
        let eye = Vec3::from(w.eye.position.to_array());
        let game = w.scene.grid.as_ref().unwrap().far_plane(&vi::vect::Vect3::of(eye.x, eye.y, eye.z)).0;
        assert!((game - 800.0).abs() < 0.01, "Freeport's shore's profiles: {game}");
        let at = far(crate::fog::GAME);
        assert!((at - game).abs() < 0.05, "{at}");
        // (read back off an f32 matrix, a far plane that far is good to 1%)
        assert!((far(0) - crate::fog::SKY_FAR).abs() < crate::fog::SKY_FAR * 0.01, "{}", far(0));
        assert!((far(crate::fog::STEPS) - game * 0.5).abs() < 0.05);
        assert!((at - 6000.0).abs() > 1000.0, "the old far plane");
        crate::settings::set_client(crate::settings::ClientOptions::default());
    }

    /// A move lands on the floor under the server's place, as `Respawn`
    /// casts: in Freeport's market, by Merchant Gilgash (54.4), on the
    /// stall's floor, where the topmost floor within 20 (the control, the
    /// old rule) is the stall's roof at 70; on open ground both agree; and
    /// 50 over the ground there is nothing within 3 (the caller keeps the
    /// server's height). F3 2026-10-21: a coach ride's arrival on a roof.
    #[test]
    fn a_move_lands_on_the_floor_under_it_not_a_roof_over_it() {
        let (x, y, z) = (25476.0f32, 54.398438f32, 15707.797f32);
        let Some(mut world) = tunaria_round(x, z) else { return };
        let floor = arrival_floor(&mut world, [x, y, z], -1).expect("a floor within 3");
        assert!((floor - y).abs() < 0.5, "the stall's floor: {floor}");
        let top = ground(&mut world, x, z).unwrap();
        assert!(top > y + 10.0 && (top - y).abs() < 20.0, "the control: the old rule took the roof at {top}");
        let (px, pz) = (25275.115f32, 15766.627f32);
        let open = arrival_floor(&mut world, [px, 54.125996, pz], -1).unwrap();
        assert!((open - ground(&mut world, px, pz).unwrap()).abs() < 0.2, "open ground: {open}");
        assert_eq!(arrival_floor(&mut world, [px, 104.0, pz], -1), None, "nothing within 3");
    }

    /// The library with TUNARIA's zones round (x, z) and CHAR.ESF (the
    /// characters) in it, and TUNARIA's place in it; None without the disc.
    fn tunaria_and_chars(x: f32, z: f32) -> Option<(esf::Library, usize)> {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        }
        let mut lib = esf::Library::new();
        let w = lib.add("TUNARIA.ESF", crate::stream::open_file(&iso, "TUNARIA.ESF").unwrap());
        lib.add("CHAR.ESF", crate::stream::open_file(&iso, "CHAR.ESF").unwrap());
        let mut src = crate::stream::FileZones::open(&iso, "TUNARIA.ESF").unwrap();
        crate::stream::fill(&mut lib, w, x, z, &mut src);
        Some((lib, w))
    }

    /// The player's own skeleton zoomed out (F3 pass 11: "the LoD off setting
    /// only disable the mid level LoD, not when zoomed far out"): a human
    /// posed with the camera 50 back is posed exactly as at the game's 8, its
    /// level taken from `lod::eye`; with Level of Detail off at 50, as up
    /// close (every node moving). The control: up close it is posed
    /// otherwise than at 8 (one node fewer at its rest frame), so the pose
    /// shows the level; from the camera itself at 50 (as the client had it)
    /// 22 of its 40 nodes rest and this fails.
    #[test]
    fn zoomed_out_the_players_body_moves_as_at_the_games_camera() {
        let (x, z) = (25275.115f32, 15766.627f32);
        let Some((lib, file)) = tunaria_and_chars(x, z) else { return };
        let human = lib.lookup(0x70D898C6).unwrap();
        let pose_from = |d: f32, lod: bool| {
            crate::settings::testing::reset(false);
            crate::settings::set_client(crate::settings::ClientOptions { lod, ..Default::default() });
            let mut w = Walker::arrive(World::load(&lib, file).unwrap(), Spawn::ground(x, z, 0.0));
            w.set_character(&lib, human);
            w.cam.camera_mode = 3;
            let p = w.cam.walk.phys.position;
            let pivot_y = p.y + w.cam.target_height + w.cam.walk.phys.height_offset;
            w.eye.position = Vect3::new(p.x, pivot_y, p.z - F::new(d));
            w.scene.time = 1234;
            w.pose().1
        };
        let at8 = pose_from(8.0, true);
        assert_eq!(pose_from(50.0, true), at8, "50 back, posed as at 8");
        let near = pose_from(2.0, true);
        assert_ne!(near, at8, "the control: up close, posed otherwise");
        assert_eq!(pose_from(50.0, false), near, "Level of Detail off: as up close, at any zoom");
        crate::settings::testing::reset(false);
    }

    /// The character loaded again (a form taken and dropped, the art
    /// turned) brings only its own clips: the table is what one load makes,
    /// and the sprite still poses. The control: a load that kept the clips
    /// and ref maps before beside its own doubled them each time.
    #[test]
    fn a_character_loaded_again_keeps_one_set_of_clips() {
        let (x, z) = (25275.115f32, 15766.627f32);
        let Some((lib, file)) = tunaria_and_chars(x, z) else { return };
        let mut w = Walker::arrive(World::load(&lib, file).unwrap(), Spawn::ground(x, z, 0.0));
        let human = lib.lookup(0x70D898C6).unwrap();
        w.set_character(&lib, human);
        let once = (w.anims.anims.len(), w.anims.ref_maps.len());
        assert!(once.0 > 0 && once.1 > 0, "a human has clips: {once:?}");
        for _ in 0..3 {
            w.set_character(&lib, human);
        }
        assert_eq!((w.anims.anims.len(), w.anims.ref_maps.len()), once);
        assert!(!w.pose().1.is_empty(), "it poses");
    }

    /// The player runs at a creature in its way and stops at its capsule (the
    /// walk's `VISceneFilter` passes over only the player's own actor), or
    /// slides round it when it is not dead ahead, never nearer its axis than
    /// the capsule grown by the walk's sphere (0.5 + 0.6): on open ground in
    /// Freeport, a signpost (an NPC, CHAR.ESF 0x6A82352C) 4 ahead. The
    /// controls: the same run with no signpost goes on past where it stood;
    /// the player's own actor at its feet, moved with it as `UpdateGame`
    /// moves it, changes nothing of its run.
    #[test]
    fn the_player_stops_at_a_creature_in_its_way() {
        let (x, z) = (25275.115f32, 15766.627f32);
        let Some((lib, file)) = tunaria_and_chars(x, z) else { return };
        let sprite_of = |w: &mut World<Ee>, id: i32| w.find_sprite(&lib, lib.lookup(id).unwrap()).unwrap();
        // 1.5 s forward; (its place each frame, the place it began)
        let run = |post: Option<(f32, f32)>, own: bool| {
            let mut w = Walker::arrive(World::load(&lib, file).unwrap(), Spawn::ground(x, z, 0.0));
            let p0 = w.cam.walk.phys.position;
            if let Some((px, pz)) = post {
                let y = ground(&mut w.scene, px, pz).unwrap();
                let s = sprite_of(&mut w.scene, 0x6A82352C);
                w.scene.create_actor(Vect3::of(px, y, pz), Vect3::default(), F::ONE, s);
            }
            let me = own.then(|| {
                let s = sprite_of(&mut w.scene, 0x70D898C6);
                let a = w.scene.create_actor(w.cam.walk.committed.smooth_location(), Vect3::default(), F::ONE, s);
                w.cam.walk.parm64 = a as i32;
                a
            });
            let t0 = w.scene.time;
            let mut at = Vec::new();
            for i in 1..=45 {
                w.frame(Controls { fwd: 1.0, ..Default::default() }, t0 + i * 33);
                if let Some(a) = me {
                    let c = &w.cam.walk.committed;
                    w.scene.set_actor_transform(a, Vect3::new(c.angles.x, F::ZERO, F::ZERO), F::ONE, c.smooth_location());
                }
                at.push(w.cam.walk.phys.position.to_array());
            }
            (at, p0.to_array())
        };
        let (free, p0) = run(None, false);
        let end = free[free.len() - 1];
        let (dx, dz) = (end[0] - p0[0], end[2] - p0[2]);
        let len = (dx * dx + dz * dz).sqrt();
        let (ux, uz) = (dx / len, dz / len);
        assert!(len > 8.0, "the control runs on: {len}");
        let ahead = |p: [f32; 3]| (p[0] - p0[0]) * ux + (p[2] - p0[2]) * uz;
        for side in [0.0f32, 0.6] {
            let post = (p0[0] + ux * 4.0 - uz * side, p0[2] + uz * 4.0 + ux * side);
            let (blocked, _) = run(Some(post), false);
            let near = blocked.iter().map(|p| ((p[0] - post.0).powi(2) + (p[2] - post.1).powi(2)).sqrt()).fold(f32::MAX, f32::min);
            let got = ahead(blocked[blocked.len() - 1]);
            eprintln!("a post {side} off the line: nearest {near:.3} to its axis, {got:.2} ahead of 4 (free: {:.2})", ahead(end));
            assert!(near > 1.05, "{side} off: within the capsule, {near}");
            if side == 0.0 {
                assert!(got < 4.0 - 1.0, "dead ahead: stopped before it, at {got}");
            } else {
                assert!(got > 4.0 + 1.1, "{side} off: slid round it, at {got}");
            }
            assert!(ahead(end) > 4.0 + 1.1, "the control passes where it stood");
        }
        let (mine, _) = run(None, true);
        assert_eq!(mine, free, "the player's own actor changes nothing of its run");
    }

    /// A login stands where the dump puts it, as `Msg000D`'s `SetLocation`
    /// does, and stays on that floor as the walk goes on: in Gilgash's
    /// stall (its roof at 70) and on Neriak's plaza (underground). A world
    /// change stands the same way, a dump's as given and a teleport's on
    /// `Respawn`'s floor. The control is `Placing::Ground`, the topmost
    /// floor every login took until 2026-10-23: the roof, the land over the
    /// cave (the user: "if you are logged underground or inside a building
    /// you will log in on top of whatever you were in").
    #[test]
    fn a_login_stands_where_it_logged_out_not_on_what_is_over_it() {
        for (x, y, z, what) in [(25476.0f32, 54.398438f32, 15707.797f32, "Gilgash's stall"), (25087.0, 0.5, 8786.0, "Neriak's plaza")] {
            for placing in [Placing::AsGiven, Placing::Respawn, Placing::Ground] {
                let Some(world) = tunaria_round(x, z) else { return };
                let mut w = Walker::arrive(world, Spawn { pos: [x, y, z], heading: 0.0, placing });
                let t0 = w.scene.time;
                for i in 1..=90 {
                    w.frame(Controls::default(), t0 + i * 33);
                }
                let at = w.cam.walk.phys.position.y.0;
                eprintln!("{what}, {placing:?}: logged out at {y}, stands at {at} after 3 s");
                if placing == Placing::Ground {
                    assert!(at > y + 10.0, "{what}: the control stands over the place, at {at}");
                } else {
                    assert!((at - y).abs() < 1.0, "{what}, {placing:?}: stands at {at}, logged out at {y}");
                }
            }
        }
    }

    /// The GM's Goto (`MoveTo`) onto Tunaria's plains west of Freeport: grid
    /// square (11, 8)'s centre, (23000, 17000), is at the grid's own height
    /// (0, the ground there at 53.6), and `MoveTo`'s cast from
    /// 300 above it finds the ground, where the walk then stands. The control
    /// is `Respawn`'s cast (3 above to 3 below), the Goto's arrival until
    /// 2026-10-28: nothing within 3, the walk left under the ground, where it
    /// fell (to -70 in 3 s; the user: "the goto zone aways spawns too low and
    /// you fall").
    #[test]
    fn goto_stands_on_the_ground_of_the_square() {
        let (x, z) = (23000.0f32, 17000.0f32);
        let Some(mut probe) = tunaria_round(x, z) else { return };
        let c = probe.grid.as_ref().expect("Tunaria's grid").center(11, 8);
        let pos = [c.x.0, c.y.0, c.z.0];
        assert_eq!((pos[0], pos[2]), (x, z), "the square's centre");
        let floor = move_to_floor(&mut probe, pos, -1).expect("MoveTo's cast finds the ground");
        assert!(floor > pos[1] + 10.0, "the ground {floor} is well over the centre's height {}", pos[1]);
        assert_eq!(arrival_floor(&mut probe, pos, -1), None, "the control's cast finds nothing within 3");
        for placing in [Placing::MoveTo, Placing::Respawn] {
            let Some(world) = tunaria_round(x, z) else { return };
            let mut w = Walker::arrive(world, Spawn { pos, heading: 0.0, placing });
            let t0 = w.scene.time;
            for i in 1..=90 {
                w.frame(Controls::default(), t0 + i * 33);
            }
            let at = w.cam.walk.phys.position.y.0;
            eprintln!("{placing:?}: the square's centre at {}, the ground at {floor}, stands at {at} after 3 s", pos[1]);
            if placing == Placing::MoveTo {
                assert!((at - floor).abs() < 1.0, "MoveTo stands on the ground: {at}, the ground {floor}");
            } else {
                assert!(at < floor - 10.0, "the control is left under the ground: {at}");
            }
        }
    }

    /// Tunaria's open ground west of Freeport: from (22000, 18000) a run at
    /// heading pi/2 stays on the flat at 54.1 for 68 m. In camera mode 2 the
    /// body steers by its own axes (`UpdateControls`' modes 0 and 2), so the
    /// walk reads nothing of the camera.
    fn open_ground() -> Option<Walker> {
        let (x, z) = (22000.0f32, 18000.0f32);
        let mut w = Walker::arrive(tunaria_round(x, z)?, Spawn::ground(x, z, std::f32::consts::FRAC_PI_2));
        w.cam.set_camera_mode(2);
        Some(w)
    }

    /// The control: the walk stepped once a frame drawn, as the client
    /// stepped it until 2026-10-27 (`UpdateControls`, then the camera's
    /// `Process` to the frame).
    fn frame_drawn(w: &mut Walker, c: Controls, now: i64) {
        crate::controls::update_controls(w, c.axes(), &crate::controls::Client::default());
        w.scene.time = now;
        w.eye = w.cam.process(&mut w.scene);
    }

    /// Frame `k`'s time at `hz` from `t0`, up to `jitter` ms early or late
    /// (a display's frames are not exact).
    fn drawn_at(t0: i64, hz: f64, jitter: f64, k: u64) -> i64 {
        let j = if jitter == 0.0 { 0.0 } else { (((k.wrapping_mul(2654435761) >> 7) % 31) as f64 / 15.0 - 1.0) * jitter };
        t0 + (k as f64 * 1000.0 / hz + j).floor() as i64
    }

    /// The ground covered between frames drawn, over the time they span.
    fn speed(at: &[(i64, Vect3<Ee>)]) -> f32 {
        let d: f32 = at.windows(2).map(|p| ((p[1].1.x.0 - p[0].1.x.0).powi(2) + (p[1].1.z.0 - p[0].1.z.0).powi(2)).sqrt()).sum();
        d / ((at[at.len() - 1].0 - at[0].0) as f32 / 1000.0)
    }

    /// The walk keeps the console's frames at any display rate: a run with a
    /// turn drawn at 60, 165 and 240 frames a second, and at 144 with each
    /// frame up to 1.5 ms early or late, keeps bit for bit the states a
    /// console at 29.97 frames a second keeps (one `Process` a frame), so it
    /// runs and turns as fast at every rate, and the run drawn at 165 goes as
    /// fast as the console's. The control: stepped once a frame drawn, as
    /// until now, it is the console's at the console's rate and 4% faster at
    /// 165.
    #[test]
    fn the_walk_keeps_the_consoles_frames_at_any_display_rate() {
        let go = Controls { fwd: 1.0, orbit: -0.25, ..Default::default() };
        let Some(mut c) = open_ground() else { return };
        let t0 = c.scene.time;
        let end = t0 + 6000;
        let console = |n: i64| t0 + (n as f64 * CONSOLE_FRAME_MS) as i64;
        let (mut want, mut path) = (Vec::new(), vec![(t0, c.cam.walk.phys.position)]);
        let mut n = 1;
        while console(n) <= end {
            c.frame(go, console(n));
            let k = c.kept.unwrap();
            assert_eq!(k.frames, n, "one console frame a frame drawn");
            want.push(k.at.0);
            path.push((console(n), c.cam.walk.phys.position));
            n += 1;
        }
        let console_speed = speed(&path);
        for (hz, jitter) in [(60.0, 0.0), (165.0, 0.0), (240.0, 0.0), (144.0, 1.5)] {
            let mut w = open_ground().unwrap();
            assert_eq!(w.scene.time, t0);
            let (mut seen, mut path) = (0, vec![(t0, w.cam.walk.phys.position)]);
            for k in 1.. {
                let now = drawn_at(t0, hz, jitter, k);
                if now > end {
                    break;
                }
                w.frame(go, now);
                path.push((now, w.cam.walk.phys.position));
                let kept = w.kept.unwrap();
                if kept.frames > seen {
                    assert_eq!(kept.frames, seen + 1, "{hz} Hz: one console frame at a time");
                    seen = kept.frames;
                    let (a, b) = (kept.at.0, want[seen as usize - 1]);
                    assert!(same(&a, &b), "{hz} Hz: console frame {seen} kept at {:?} heading {:?}, the console's at {:?} heading {:?}", a.position, a.angles.x, b.position, b.angles.x);
                }
            }
            assert_eq!(seen as usize, want.len(), "{hz} Hz: every console frame kept");
            let drawn = speed(&path);
            eprintln!("{hz} Hz (jitter {jitter}): {drawn:.4} m/s drawn, the console's {console_speed:.4}");
            assert!((drawn / console_speed - 1.0).abs() < 0.003, "{hz} Hz: drawn at {drawn}, the console's {console_speed}");
        }
        // the control
        let mut a = open_ground().unwrap();
        for n in 1..=want.len() as i64 {
            frame_drawn(&mut a, go, console(n));
        }
        assert!(same(&a.cam.walk.phys, want.last().unwrap()), "the control: once a frame at the console's rate is the console's");
        let mut b = open_ground().unwrap();
        let mut path = vec![(t0, b.cam.walk.phys.position)];
        for k in 1.. {
            let now = drawn_at(t0, 165.0, 0.0, k);
            if now > end {
                break;
            }
            frame_drawn(&mut b, go, now);
            path.push((now, b.cam.walk.phys.position));
        }
        let fast = speed(&path);
        eprintln!("the control at 165 Hz: {fast:.4} m/s");
        assert!(fast > console_speed * 1.03, "the control: once a frame drawn at 165 runs {fast}, the console {console_speed}");
    }

    /// The farthest and the root mean square (mm) the places drawn are off
    /// the straight line through them that fits best, over time.
    fn off_line(at: &[(i64, Vect3<Ee>)]) -> (f64, f64) {
        let n = at.len() as f64;
        let m = |f: &dyn Fn(&(i64, Vect3<Ee>)) -> f64| at.iter().map(f).sum::<f64>() / n;
        let (mt, mx, mz) = (m(&|p| p.0 as f64), m(&|p| p.1.x.0 as f64), m(&|p| p.1.z.0 as f64));
        let tt: f64 = at.iter().map(|p| (p.0 as f64 - mt).powi(2)).sum();
        let vx = at.iter().map(|p| (p.0 as f64 - mt) * (p.1.x.0 as f64 - mx)).sum::<f64>() / tt;
        let vz = at.iter().map(|p| (p.0 as f64 - mt) * (p.1.z.0 as f64 - mz)).sum::<f64>() / tt;
        let off: Vec<f64> = at.iter().map(|p| {
            let dt = p.0 as f64 - mt;
            ((p.1.x.0 as f64 - mx - vx * dt).powi(2) + (p.1.z.0 as f64 - mz - vz * dt).powi(2)).sqrt() * 1000.0
        }).collect();
        (off.iter().cloned().fold(0.0, f64::max), (off.iter().map(|e| e * e).sum::<f64>() / n).sqrt())
    }

    /// Drawn at 165 and 240 frames a second a run moves on every frame, at
    /// the console's top speed there, and is as straight as the console's own
    /// frames to within the floats' own step (1.95 mm at x 22000): over 3 s
    /// those are 3.4 mm off a line at worst (the decomp's adds chop), the
    /// frames drawn 3.1 at 165 and 3.6 at 240. (The stepping it replaced was
    /// within 0.5 mm, adding a step cut to the same number of float steps
    /// every frame, which is what made its speed change with the rate and
    /// the heading.) The control: what a lock to the console's frames would
    /// draw (the walk as the last console frame kept it) stands still on
    /// most frames and is a stride off the line.
    #[test]
    fn drawn_at_any_rate_the_walk_moves_every_frame() {
        let go = Controls { fwd: 1.0, ..Default::default() };
        let Some(mut c) = open_ground() else { return };
        let t0 = c.scene.time;
        let mut console = Vec::new();
        for n in 1..=150 {
            let now = t0 + (n as f64 * CONSOLE_FRAME_MS) as i64;
            c.frame(go, now);
            if now - t0 >= 2000 {
                console.push((now, c.cam.walk.phys.position));
            }
        }
        let theirs = speed(&console);
        let (cw, cr) = off_line(&console);
        eprintln!("the console's own frames: off a line {cw:.2} mm at worst, {cr:.2} mm rms");
        let step = 1.95;
        for hz in [165.0, 240.0] {
            let mut w = open_ground().unwrap();
            let (mut live, mut locked) = (Vec::new(), Vec::new());
            for k in 1..=(hz * 5.0) as u64 {
                let now = drawn_at(t0, hz, 0.0, k);
                w.frame(go, now);
                if now - t0 >= 2000 {
                    live.push((now, w.cam.walk.phys.position));
                    locked.push((now, w.kept.unwrap().at.0.position));
                }
            }
            let top = speed(&live);
            let (worst, rms) = off_line(&live);
            let moved = live.windows(2).all(|p| p[1].1.x != p[0].1.x || p[1].1.z != p[0].1.z);
            eprintln!("{hz} Hz: {top:.4} m/s (the console's {theirs:.4}), off a line {worst:.2} mm at worst, {rms:.2} mm rms");
            assert!((top / theirs - 1.0).abs() < 0.003, "{hz} Hz: drawn at {top}, the console's top speed {theirs}");
            assert!(moved, "{hz} Hz: a frame drawn stood still");
            assert!(worst < cw + step && rms < cr + step / 4.0, "{hz} Hz: {worst} mm off the line at worst, {rms} rms; the console's {cw}, {cr}");
            let still = locked.windows(2).filter(|p| p[1].1.x == p[0].1.x && p[1].1.z == p[0].1.z).count();
            let (lw, _) = off_line(&locked);
            assert!(still * 2 > locked.len() && lw > 100.0, "the control: locked, {still} of {} frames stand still, {lw} mm off the line", locked.len());
        }
    }

    /// A place given between frames (a teleport, a dump in play, the loading
    /// screen's hold) stands: the next frame steps on from it, not from the
    /// console frame kept before it. The control: had the walk looked as the
    /// last frame left it, the frame would have put it back.
    #[test]
    fn a_place_given_between_frames_stands() {
        let run = |control: bool| {
            let mut w = open_ground()?;
            let t0 = w.scene.time;
            let go = Controls { fwd: 1.0, ..Default::default() };
            for k in 1..=100 {
                w.frame(go, drawn_at(t0, 165.0, 0.0, k));
            }
            let p = w.cam.walk.phys.position;
            let to = [p.x.0 + 30.0, p.y.0, p.z.0 + 30.0];
            w.place(to, 0.0);
            if control {
                let k = w.kept.as_mut().unwrap();
                k.left = (w.cam.walk.phys, w.cam.walk.committed);
            }
            w.frame(go, drawn_at(t0, 165.0, 0.0, 101));
            let q = w.cam.walk.phys.position;
            Some(((q.x.0 - to[0]).powi(2) + (q.z.0 - to[2]).powi(2)).sqrt())
        };
        let Some(d) = run(false) else { return };
        assert!(d < 0.2, "the next frame steps on from the place: {d} from it");
        let c = run(true).unwrap();
        assert!(c > 30.0, "the control: put back to the frame kept before, {c} from the place");
    }

    /// A landing is told once (`parm118`'s 16, the sprite's locomotion 7),
    /// on the first frame drawn that stands, at any rate: a drop of 3 onto
    /// the open ground. The control: `Process`'s own flag, set by the
    /// console frame's call and cleared again by the camera's (which finds
    /// no time left to step), tells it on no frame.
    #[test]
    fn a_landing_is_told_once() {
        for hz in [1000.0 / CONSOLE_FRAME_MS, 60.0, 165.0] {
            let drop = || {
                let mut w = open_ground()?;
                let p = w.cam.walk.phys.position;
                w.place([p.x.0, p.y.0 + 3.0, p.z.0], 0.0);
                Some(w)
            };
            let Some(mut w) = drop() else { return };
            let t0 = w.scene.time;
            let (mut told, mut was, mut at) = (0, w.cam.walk.phys.flags, None);
            for k in 1..=(hz * 3.0) as u64 {
                let now = drawn_at(t0, hz, 0.0, k);
                w.frame(Controls::default(), now);
                let flags = w.cam.walk.phys.flags;
                if w.cam.walk.parm118 == 16 {
                    told += 1;
                    assert!(flags & ON_FLOOR != 0 && was & ON_FLOOR == 0, "{hz} Hz: told on a frame that stood before");
                    at.get_or_insert(now - t0);
                }
                was = flags;
            }
            // the control: `frame_with`'s steps, the flag read before `keep_left`
            let mut c = drop().unwrap();
            let mut own = 0;
            for k in 1..=(hz * 3.0) as u64 {
                let now = drawn_at(t0, hz, 0.0, k);
                c.rewind();
                c.step_kept(now);
                let ahead = c.ahead();
                c.scene.time = now;
                c.draw_between(&ahead, now);
                c.eye = c.cam.process(&mut c.scene);
                own += (c.cam.walk.parm118 == 16) as u32;
                c.keep_left();
            }
            eprintln!("{hz:.2} Hz: landed {at:?} ms in, told {told}, by Process's own flag {own}");
            assert_eq!(told, 1, "{hz} Hz: told once");
            assert_eq!(own, 0, "the control: {hz} Hz, Process's own flag told it {own} times");
        }
    }

    /// The camera's NO_TURN (set during its `Process` as it swings round what
    /// is in its way, cleared there once round) is kept with the console
    /// frame, as the console keeps it in the body's state till the next
    /// step. The control: another bit written there the same way is put
    /// back with the frame.
    #[test]
    fn the_cameras_no_turn_is_kept_with_the_frame() {
        let Some(mut w) = open_ground() else { return };
        let t0 = w.scene.time;
        for k in 1..=10 {
            w.frame(Controls::default(), drawn_at(t0, 165.0, 0.0, k));
        }
        // frame 11 as `frame_with` steps it, the camera's write inside it
        let now = drawn_at(t0, 165.0, 0.0, 11);
        w.rewind();
        w.step_kept(now);
        w.scene.time = now;
        w.eye = w.cam.process(&mut w.scene);
        w.cam.walk.phys.flags |= NO_TURN | vi::walk::flags::SLOW_RUN;
        w.keep_left();
        w.rewind();
        let f = w.cam.walk.phys.flags;
        assert!(f & NO_TURN != 0, "NO_TURN kept with the frame");
        assert!(f & vi::walk::flags::SLOW_RUN == 0, "the control: another bit written so is put back");
    }

    /// A stall (the next frame 2 s on) moves the walk as the console's one
    /// long frame does: `Process` steps its last 200 ms and skips the rest,
    /// so a run goes on under 2 m. The control: drawn through the same 2 s,
    /// the run goes on 17.
    #[test]
    fn a_stall_is_one_long_frame() {
        let go = Controls { fwd: 1.0, ..Default::default() };
        let run = |stall: bool| {
            let mut w = open_ground()?;
            let t0 = w.scene.time;
            for k in 1..=120 {
                w.frame(go, drawn_at(t0, 60.0, 0.0, k));
            }
            let (t, p) = (drawn_at(t0, 60.0, 0.0, 120), w.cam.walk.phys.position);
            if stall {
                w.frame(go, t + 2000);
            } else {
                for k in 121..=240 {
                    w.frame(go, drawn_at(t0, 60.0, 0.0, k));
                }
            }
            let q = w.cam.walk.phys.position;
            Some(((q.x.0 - p.x.0).powi(2) + (q.z.0 - p.z.0).powi(2)).sqrt())
        };
        let Some(d) = run(true) else { return };
        assert!(d < 2.2, "the stall goes on {d}");
        let c = run(false).unwrap();
        assert!(c > 15.0, "the control: drawn through, {c}");
    }
}
