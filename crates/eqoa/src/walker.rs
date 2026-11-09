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
//!
//! A GM's flight (`Client::flying`: go-eqoa-server's `#fly`; the client's own,
//! no such thing in the game; the user, 2026-11-07: "its like swimming you can
//! look up/down to go in that direction, but running, standing like normal") is
//! CLIENT's swim path, untouched in `vi`, with its answers changed at the seam:
//! gravity off (`m_parm60`, the walk's own switch), and each console frame stepped
//! in a world of water with no surface (`Aloft`), so that `Run` and `Walk` push
//! along the body's pitch (`m_facing`) at the same speed as on the ground (the
//! swim's damping and the ground's are both 0.02), the walls stop it as they stop a
//! swimmer, and nothing lets it fall. In the orbits the body takes its pitch from
//! the camera's tilt, the game's default for the camera counting as level (the
//! user, of the 15 degrees the follow orbit looks down: "yes make default level";
//! `fly_pitch`, `fly_view`); first person's and the follow camera's is the body's
//! own, which looks level at 0 and has no tilt to take off. What the rest of the
//! client and the server are shown is the walk as a body in the air is, drawn each
//! frame: the gait a land
//! walk's, never a swim's (`fly_gait`), and the flags the world's at its place
//! (`flags_at`), so its 0x40 runs no breath down but under real water. Off, the
//! flags are the world's again and the walk goes on from where it is.
//!
//! A jump (`ask_jump`, on a server that offers it: go-eqoa-server's `world.jump`,
//! `jump.rs`; the user, 2026-11-09: "lets add jump to the game"; no such thing in
//! the game either) is CLIENT's walk too, given one thing between console frames,
//! as a control: a body on a floor, out of any liquid, gets `JUMP_SPEED` upward,
//! and the floor it remembers (`m_floor`, which `Move` sticks a body to when one
//! is within its step-down tolerance, `m_parm28` 0.6, under its feet) is lifted
//! over the jump's top, so `Move` lets it leave the ground and takes it as a body
//! in the air: its own gravity and air damping, no push (Walk and Run push only on
//! a floor), its own landing (the feet come down to a floor, which sets `m_floor`
//! again), the fall's gait (6) on the way and the landing's (7) after, as a body
//! stepping off a ledge has them. The first tick of it goes at the ground's
//! damping, as it leaves from one, so the run's speed across is kept whole. With
//! no jump asked nothing of it runs: every frame of the walk is as it was.

use glam::{Mat4, Vec3};
use vi::camera::{Eye, WalkCamera};
use vi::coll::{CollSphere, EnvRay};
use vi::fpu::{Ee, F, vimax, vimin};
use vi::scene::{Filter, FilterKind, Scene};
use vi::vect::Vect3;
use vi::walk::{WalkControl, WalkPhysics};
use vi::walk::flags::{self, NO_TURN, ON_FLOOR, SUBMERGED};
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
    /// The walk put back this frame (`rewind`), not yet kept again
    /// (`keep_left`).
    rewound: bool,
    /// The camera's place as `rewind` found it, and how far the walk was
    /// put back (the last frame drawn's place less the console frame's, in
    /// doubles): a place given the camera after that was given round the
    /// console frame (`camera_as_drawn`).
    given: Option<(Vect3<Ee>, [f64; 3])>,
    /// The player's actor's scale: its entity's (`m_scale`, the state
    /// channel's), as `VIEntity::Changed`'s `SetActorScale` gives the player's
    /// own actor as any other's (play sets it, `place_self_actor`; 1 with no
    /// server). The body is drawn at it; the walk's collision is the walk
    /// control's own (`InitParms`' constants, which nothing in CLIENT scales).
    pub scale: F<Ee>,
    /// A GM's flight is on (the module's doc), and the orbit's pitch and its
    /// camera mode when it came on, its look put back when it ends.
    flying: bool,
    fly_tilt: Option<(i32, F<Ee>)>,
    /// This frame's length (s), as the camera's `Process` is told it.
    frame_s: f32,
    /// A jump asked (`ask_jump`): when, on the walk's clock.
    jump_asked: Option<i64>,
    /// In a jump, from its take-off until the body is on a floor again: its
    /// fall wholly in by its top (`jump::fall_by_top`), which a jump that
    /// reaches its top has already.
    in_jump: bool,
}

/// A jump's start speed upward (units a second): with the walk's own integrator,
/// its first tick at the ground's damping (0.02 a second) and the rest at the
/// air's (0.8), the feet rise `JUMP_TOP` (0.896) over the floor, at the top 0.29 s
/// in and back on flat ground 0.62 s after leaving it.
pub const JUMP_SPEED: f32 = 7.1;

/// The highest a jump takes the feet over the floor it left: half the walk's
/// body (`m_parm30`, 1.8). go-eqoa-server's movement check takes a jump to this
/// (its `JumpTop`), and no further.
pub const JUMP_TOP: f32 = 0.9;

/// How long a jump asked waits for the walk and the client to allow one (ms on
/// the walk's clock): a press as the body comes down, or before the report of
/// its landing has gone out, jumps once it may.
const JUMP_WAIT_MS: i64 = 250;

/// The console's frame: two fields of NTSC video (59.94 Hz), 33.37 ms. The
/// walk keeps only these, at any display rate (`Kept`).
const CONSOLE_FRAME_MS: f64 = 2.0 * 1001.0 / 60.0;

/// The game's clock, which the walk steps by (`VIScene::GetTime`: `Tick`
/// journals `VITimer::GetTime()` and sets the scene's time to it): SUPPORT's
/// VITimer, the EE's timer 0 at the bus clock over 16 (9.216 MHz)
/// interrupting every 18,750 counts, 491.52 times a second, and adding 2
/// each time. So it counts 983.04 a second, every count even, and a console
/// frame is 32.80 of them: the walk steps 32 or 34 (`console_step`).
const VITIMER_IRQ_PER_MS: f64 = 0.49152;

/// The game clock's step into console frame `n` of a walk's frames: VITimer's
/// count as the frame begins less its count as the frame before began, 32 or
/// 34, three frames in five 32. `Process` steps a 32 tick and then the rest,
/// and takes the gait from the last of those (`UpdateLocomotion`), so on
/// three frames in five the console reads the gait off the whole tick and on
/// the others off a 2 tick rest. Counted in milliseconds (33 or 34) every
/// frame had a rest of 1 or 2, and the gait was read off that alone: a
/// float step or two of way, which the EE's chopped add rounds toward zero,
/// up for a walk toward a lower coordinate. Walking toward -z at Tunaria's
/// z 31570 (one step 1.95 mm) the 1 rest read 3.9 a second, a run, and the 2
/// rest 2.9, a walk, and the body changed its gait two frames in three
/// (2026-11-03, Mesh walking toward Warlord Jurglash: "when walking slow
/// with controller it is still rapidly jitters animation"). On VITimer's
/// counts it walks throughout, and walks and runs at a PS2's pace on the
/// wall clock, whose game clock runs 1.7% slower than milliseconds (east on
/// Tunaria's open ground a run 8.62 a second where it was 8.81, a walk 2.88
/// where it was 2.91).
fn console_step(n: i64) -> i64 {
    let count = |n: i64| ((n as f64 * CONSOLE_FRAME_MS * VITIMER_IRQ_PER_MS) as i64) * 2;
    count(n) - count(n - 1)
}

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
/// steps every console frame come since as the console steps one (the game
/// clock's step, `console_step`: a tick, and on some frames a rest) and keeps
/// that: the walk keeps what a console at 30 frames a second keeps. Each
/// console frame is at its own place on the wall clock (the scene's time,
/// which the camera, the animation and the drawing go by) and steps the
/// walk by VITimer's count.
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
    /// Console frame `n`'s time on the wall clock: 33 or 34 ms apart.
    fn time(&self, n: i64) -> i64 {
        self.origin + (n as f64 * CONSOLE_FRAME_MS) as i64
    }

    /// The walk at console frame `n - 1` (its committed state) made to be
    /// stepped into frame `n` at `scene_time` (frame `n`'s `time`) by the
    /// game clock's step (`console_step`): `Process` steps the time since the
    /// committed state's.
    fn ready(w: &mut WalkPhysics<Ee>, n: i64, scene_time: i64) {
        w.time = scene_time - console_step(n);
    }
}

/// The world as the walk meets it in flight (the module's doc): its collision
/// the world's, and water everywhere with no surface, so `Process` takes the swim
/// path whatever is really there, and past a real surface freely.
struct Aloft<'a>(&'a mut World<Ee>);

impl Scene<Ee> for Aloft<'_> {
    fn time(&self) -> i64 {
        self.0.time
    }
    fn collide_sphere(&mut self, s: &mut CollSphere<Ee>, filter: &mut Filter) -> bool {
        self.0.collide_sphere(s, filter)
    }
    fn collide_env(&mut self, _: &mut EnvRay<Ee>) -> bool {
        false
    }
    fn env_flags(&self, _: &Vect3<Ee>) -> u32 {
        1
    }
}

/// `p`'s flags as a body in the air has them: what `Move` works out of the world
/// at its place in place of `Aloft`'s water (a place in no zone is dry here, where
/// `GetEnvFlags` says every liquid: a flyer may go past the world's edge), and the
/// floor as it stands (`stands`), the bump on a wall (B1) only with it (a wall
/// holds no one up).
fn flags_at(world: &World<Ee>, w: &WalkControl<Ee>, p: &WalkPhysics<Ee>) -> u32 {
    let liquids = |at: Vect3<Ee>| match world.env_flags(&at) {
        u32::MAX => 0,
        e => e,
    };
    let mut at = p.position;
    at.y += w.parm38;
    let (above, below) = (Vect3::new(at.x, at.y + w.parm54, at.z), Vect3::new(at.x, at.y - w.parm54, at.z));
    let (mid, up, dn) = (liquids(at), liquids(above), liquids(below));
    let mut fl = p.flags & !(flags::LIQUID | flags::SLOW_RUN | ON_FLOOR | flags::B1);
    let mut set = |on: bool, bit: u32| {
        if on {
            fl |= bit;
        }
    };
    set(mid & 1 != 0, flags::IN_WATER);
    set(mid & 4 != 0, flags::IN_LAVA);
    set(mid & 2 != 0, flags::IN_SLIME);
    set(mid & 8 != 0, flags::IN_MUD);
    set(up & 1 == 0 && dn & 1 != 0, flags::AT_WATER_SURFACE);
    set(up & 4 == 0 && dn & 4 != 0, flags::AT_LAVA_SURFACE);
    set(up & 2 == 0 && dn & 2 != 0, flags::AT_SLIME_SURFACE);
    set(up & 8 == 0 && dn & 8 != 0, flags::AT_MUD_SURFACE);
    set(liquids(p.position) & 8 != 0, flags::SLOW_RUN);
    if stands(p) {
        fl |= ON_FLOOR | (p.flags & flags::B1);
    }
    fl
}

/// A body in flight stands on a floor: it just met one (the swim path clears
/// ON_FLOOR the frame after, level on it), or it is still at the height the last
/// floor put it at with a floor within the probe's reach under it.
fn stands(p: &WalkPhysics<Ee>) -> bool {
    p.flags & ON_FLOOR != 0 || (p.landscape_normal.y != F::ZERO && (p.position.y - p.floor).abs() < F::new(0.01))
}

/// `UpdateLocomotion` for a console frame in flight: its land chain, whatever the
/// frame's flags, on the way the frame went from `from` over `ms`, the way along
/// the body's axes taken whole (up and down are forward here: a body run straight
/// up is running) and backward only when it went back across the ground. So a
/// body moving runs (walks, sidesteps, backs up) and one still stands or turns.
/// `was` is the gait before it (the swim path's own left the frame's 12 or 13).
fn fly_gait(w: &mut WalkControl<Ee>, from: &Vect3<Ee>, was: i32, ms: i64) {
    let f = F::<Ee>::new;
    let mut d = w.committed.position;
    d -= *from;
    let dt = f(ms.max(1) as i32 as f32) * f(0.001);
    let across = d.dot(&w.right);
    let along = vimax(d.dot(&d) - across * across, F::ZERO).sqrt() / dt;
    let v_f = if d.dot(&w.forward) < F::ZERO { -along } else { along };
    let v_r = across / dt;
    let st = w.speed_threshold;
    let spin = w.committed.angular_vel.x;
    let not_walking = was != 1 && was != 2;
    let gait = if w.walk_speed + (w.run_speed - w.walk_speed) * f(0.1) < v_f {
        3
    } else if st < v_r && v_r.abs() > v_f.abs() && not_walking {
        11
    } else if v_r < -st && v_r.abs() > v_f.abs() && not_walking {
        10
    } else if st < v_r && v_f.abs() * f(1.5) < v_r.abs() {
        11
    } else if v_r < -st && v_f.abs() * f(1.5) < v_r.abs() {
        10
    } else if st < v_f {
        1
    } else if v_f < -st {
        2
    } else if w.turn_rate * f(0.5) < spin {
        4
    } else if spin < -(w.turn_rate * f(0.5)) {
        5
    } else {
        0
    };
    w.phys.loco_state = gait;
    w.committed.loco_state = gait;
}

/// How slow a flyer let go is when it stops, squared (0.05 units a second): the
/// walk's damping alone never quite brings a body to rest (`process_aloft`).
const FLY_STOPPED_SQ: f32 = 0.0025;

/// `Process` in the world of water. A body let go comes to rest there and holds
/// its place to the bit, and one going level holds its height: `Move` adds the
/// body's height to the place and takes it off again, and the EE's chopped adds
/// lose a float step of height a frame or gain one (a millimetre a second at 300
/// up, nothing a floor does not put back on the ground), so a body in the air
/// would creep for good, and a 0x40 go out for each step of it where one standing
/// still sends none. A floor or anything else met keeps the height it gave.
fn process_aloft(w: &mut WalkControl<Ee>, scene: &mut World<Ee>) {
    let (from, pushed) = (w.committed.position, w.phys.force);
    w.process(0, &mut Aloft(scene));
    let (zero, stopped) = (F::<Ee>::ZERO, F::<Ee>::new(FLY_STOPPED_SQ));
    let v = w.phys.velocity;
    if pushed.x == zero && pushed.y == zero && pushed.z == zero && v.dot(&v) < stopped {
        for p in [&mut w.phys, &mut w.committed] {
            p.position = from;
            p.velocity = Vect3::default();
        }
    } else if pushed.y == zero && v.y * v.y < stopped && w.phys.flags & (ON_FLOOR | flags::B1) == 0 {
        for p in [&mut w.phys, &mut w.committed] {
            p.position.y = from.y;
            p.velocity.y = zero;
        }
    }
}

/// A console frame of the walk: `Process`, in flight in the world of water, the
/// gait then the land's.
fn step_frame(w: &mut WalkControl<Ee>, scene: &mut World<Ee>, flying: bool) {
    if !flying {
        w.process(0, scene);
        return;
    }
    let (from, was, ms) = (w.committed.position, w.committed.loco_state, scene.time - w.committed.time);
    process_aloft(w, scene);
    fly_gait(w, &from, was, ms.min(w.tick_alt_ms as i64));
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
        Walker { cam, scene, eye, sprite: None, anims: AnimScene::default(), rand: Rand(1), shake: 0, reach: Default::default(), kept: None, rewound: false, given: None, scale: F::ONE, flying: false, fly_tilt: None, frame_s: 0.0, jump_asked: None, in_jump: false }
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
    /// heading, at the actor's scale (`VIMatrix44::Transform(hpr, scale,
    /// pos)`).
    pub fn actor(&self) -> Matrix44<Ee> {
        let p = self.cam.walk.phys.smooth_location();
        let hpr = Vect3::new(self.cam.walk.phys.angles.x, F::ZERO, F::ZERO);
        Matrix44::transform(&hpr, self.scale, &p)
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
        if self.flying {
            // standing like normal, under real water too: `UpdateSprite` pitches
            // a body under liquid with the walk, and sinks one at a surface
            let mut w = self.cam.walk;
            w.phys.flags &= !flags::LIQUID;
            w.update_sprite(cs, &self.anims, &mut self.rand);
        } else {
            self.cam.walk.update_sprite(cs, &self.anims, &mut self.rand);
            // a jump's fall in by its top, till the feet are on a floor
            let p = &self.cam.walk.phys;
            if p.flags & ON_FLOOR != 0 && p.velocity.y <= F::ZERO {
                self.in_jump = false;
            } else if self.in_jump {
                crate::jump::fall_by_top(cs, p.velocity.y, &self.anims);
            }
        }
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
    /// camera's `Process` finds it. A place given the camera on the walk put
    /// back is moved onto the walk as last drawn (`camera_as_drawn`).
    pub fn frame_with(&mut self, keys: [f32; 4], now_ms: i64, c: &crate::controls::Client, game_frame: bool) -> crate::controls::Changed {
        self.rewind();
        if c.flying != self.flying {
            self.set_flying(c.flying);
        }
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
        let dt_s = (now_ms - self.scene.time).clamp(0, 100) as f32 / 1000.0;
        self.frame_s = dt_s;
        if self.flying {
            self.fly_pitch();
        }
        let changed = crate::controls::update_controls(self, keys, &c);
        if self.flying {
            self.fly_on_the_floor();
        }
        self.jump_if_asked(&c, now_ms);
        self.step_kept(now_ms);
        let ahead = self.ahead();
        self.scene.time = now_ms;
        self.draw_between(&ahead, now_ms);
        self.camera_as_drawn();
        let next = self.cam.effect.sample;
        if !game_frame {
            self.cam.effect.sample = self.shake;
        }
        // steered, the camera kept behind the body: no swing round; and the
        // camera's sweeps go on past what is thin (`camreach`)
        self.reach.steered = c.steer;
        self.eye = self.reach.process(&mut self.cam, &mut crate::camreach::PastThin { scene: &mut self.scene }, dt_s);
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
    /// stands. Once a frame: the mouse's look calls it before it turns the
    /// body (`mouse::set_heading`, `pitch_body`), so the turn is made on the
    /// kept frame, as a control, and the console frames step on from it; a
    /// turn made on the walk as drawn would read as a walk moved, and with
    /// the console frames begun again every frame drawn none was ever
    /// stepped (F3 20261028-130331: the run slid on at its speed with the
    /// keys let go, and from a stand never got up to it).
    pub fn rewind(&mut self) {
        if std::mem::replace(&mut self.rewound, true) {
            return;
        }
        let w = &mut self.cam.walk;
        match &self.kept {
            Some(k) if same(&w.phys, &k.left.0) && same(&w.committed, &k.left.1) => {
                let (l, a) = (&k.left.0, &k.at.0);
                let d = |u: F<Ee>, v: F<Ee>| u.0 as f64 - v.0 as f64;
                let back = [d(l.position.x, a.position.x), d(l.position.y, a.position.y) + d(l.height_offset, a.height_offset), d(l.position.z, a.position.z)];
                self.given = Some((self.cam.cur.camera_pos, back));
                w.phys = k.at.0;
                w.committed = k.at.1;
                w.update_directions();
            }
            _ => {
                self.given = None;
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
        let Walker { cam, scene, kept, flying, .. } = self;
        let flying = *flying;
        let Some(k) = kept.as_mut() else { return };
        let w = &mut cam.walk;
        let (force, torque) = (w.phys.force, w.phys.torque);
        if now - k.time(k.frames) > w.tick_alt_ms as i64 {
            scene.time = now;
            step_frame(w, scene, flying);
            (k.origin, k.frames, k.ahead) = (now, 0, None);
        }
        while k.time(k.frames + 1) <= now {
            k.frames += 1;
            w.phys.force = force;
            w.phys.torque = torque;
            scene.time = k.time(k.frames);
            Kept::ready(&mut w.committed, k.frames, scene.time);
            step_frame(w, scene, flying);
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
        let Walker { cam, scene, kept, flying, .. } = self;
        let flying = *flying;
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
        Kept::ready(&mut w.committed, k.frames + 1, scene.time);
        if flying {
            process_aloft(&mut w, scene);
        } else {
            w.process(0, scene);
        }
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
        if self.flying {
            p.flags = flags_at(&self.scene, &self.cam.walk, &p);
        }
        let w = &mut self.cam.walk;
        (w.phys, w.committed) = (p, p);
    }

    /// The jump key (`Play::jump_key`): a jump asked of the walk, taken at its
    /// next frame that allows one (`jump_if_asked`), within `JUMP_WAIT_MS`.
    pub fn ask_jump(&mut self) {
        self.jump_asked = Some(self.scene.time);
    }

    /// The jump asked, taken now if the client allows one
    /// (`controls::Client::may_jump`) and the walk does: on the walk put back
    /// (the console frame kept, given the jump as a control, as `UpdateControls`
    /// gives its forces), on a floor, out of any liquid, not flying, not already
    /// on its way up. Let go once taken, or `JUMP_WAIT_MS` after it was asked.
    fn jump_if_asked(&mut self, c: &crate::controls::Client, now: i64) {
        let Some(at) = self.jump_asked else { return };
        if now - at > JUMP_WAIT_MS {
            self.jump_asked = None;
            return;
        }
        let p = &self.cam.walk.phys;
        let stands = p.flags & ON_FLOOR != 0 && p.flags & flags::LIQUID == 0 && p.velocity.y <= F::ZERO;
        if self.flying || !stands || !c.may_jump() {
            return;
        }
        self.jump_asked = None;
        self.take_off();
    }

    /// The jump (the module's doc): `JUMP_SPEED` up, and the floor the walk
    /// remembers lifted over the jump's top by the step-down tolerance and a
    /// little, so no floor under the feet on the way up or down holds the body
    /// before its feet come down to one.
    fn take_off(&mut self) {
        let w = &mut self.cam.walk;
        let lift = w.phys.position.y + F::new(JUMP_TOP) + w.parm28 + F::new(0.1);
        w.phys.floor = lift;
        w.committed.floor = lift;
        w.phys.velocity.y = F::new(JUMP_SPEED);
        self.in_jump = true;
        eprintln!("walker: jump from {:?}", w.phys.position.to_array());
    }

    /// A GM's flight on or off (the module's doc). Off, on the walk put back: the
    /// body goes on from where it is, as a body in the air or on the floor there
    /// (the world's flags, its pitch level again in the orbits, where it was the
    /// camera's; no way up left in it, as nothing lifts a body in the air), and the
    /// orbit's look as it was before the flight.
    fn set_flying(&mut self, on: bool) {
        eprintln!("walker: flying {on} at {:?}", self.cam.walk.phys.position.to_array());
        self.flying = on;
        let cam = &mut self.cam;
        cam.walk.parm60 = if on { 0 } else { 1 };
        if on {
            self.fly_tilt = Some((cam.camera_mode, if cam.camera_mode == 3 { cam.def_orbit_pitch } else { cam.cur.orbit.y }));
            return;
        }
        let orbit = matches!(cam.camera_mode, 1 | 3);
        let w = &mut cam.walk;
        let (mut phys, mut committed) = (w.phys, w.committed);
        for p in [&mut phys, &mut committed] {
            p.flags = flags_at(&self.scene, w, p);
            if orbit {
                p.angles.y = F::ZERO;
            }
            p.velocity.y = vimin(p.velocity.y, F::ZERO);
        }
        (w.phys, w.committed) = (phys, committed);
        w.update_directions();
        match self.fly_tilt.take() {
            Some((3, pitch)) if cam.camera_mode == 3 => cam.def_orbit_pitch = pitch,
            Some((1, pitch)) if cam.camera_mode == 1 => {
                cam.set_orbit_pitch_angle(pitch);
            }
            _ => {}
        }
    }

    /// In flight, in the orbits: the body's pitch the tilt the player chose less
    /// the game's own for the camera (the module's doc), which `Run` and `Walk` then
    /// push along (`m_facing`, on the body's own axes: the orbit of mode 1, mode 3
    /// steered) and `fly_view` too. So a camera at its default flies level, tilted
    /// up from there climbs and down dives, at any frame rate, the eye's own
    /// direction left out: it looks down by the tilt, and by a race's look-at
    /// height with the zoom; and in mode 3 the tilt read is the default the
    /// player's look moves (`mouse::tilt`), not the pitch the camera's own snaps
    /// swing it to when it meets a wall. Before `UpdateControls`, on the walk put
    /// back; the body is drawn upright whatever its pitch (`pose_sounding`).
    fn fly_pitch(&mut self) {
        let cam = &mut self.cam;
        let pitch = match cam.camera_mode {
            3 => cam.def_orbit_pitch - F::new(crate::camreach::GAME_PITCH),
            1 => cam.cur.orbit.y - F::new(crate::camstyle::ORBIT_PITCH),
            _ => return,
        };
        let w = &mut cam.walk;
        w.phys.angles.y = pitch;
        w.committed.angles.y = pitch;
        w.update_directions();
    }

    /// In flight, `ViewRelativeMove` and `ViewRelativeWalk` (`walking`) go up and
    /// down too: the stick's forward along the camera's way `r` across the ground at
    /// the body's pitch (`fly_pitch`), its sideways level, both at the walk's own
    /// push (so the speed is the ground's in any direction), the body turned to the
    /// way the stick points across the ground, as ever. `NO_TURN` and the rest as the
    /// walk's: the game's body axes.
    fn fly_view(&mut self, r: &Vect3<Ee>, x: F<Ee>, y: F<Ee>, walking: bool) {
        let w = &mut self.cam.walk;
        if w.phys.flags & NO_TURN != 0 {
            if walking {
                w.walk(x, F::ZERO);
            } else {
                w.move_stick(x, F::ZERO);
            }
            return;
        }
        let mut v = vi::vect::Vect2::new(x, y);
        let mag = v.magnitude();
        v.normalize();
        let mut level = Vect3::new(r.x, F::ZERO, r.z);
        level.normalize();
        let mut side = Vect3::cross(&level, &Vect3::of(0.0, 1.0, 0.0));
        side.normalize();
        let h = if w.move_deadzone < mag {
            let a = if !walking && w.run_threshold < mag { w.run_accel } else { w.walk_accel };
            let pitch = w.phys.angles.y;
            let mut look = level;
            look *= pitch.cosf();
            look.y = -pitch.sinf();
            w.phys.force.add_scaled(&look, a * v.x);
            w.phys.force.add_scaled(&side, a * v.y);
            let mut way = level;
            way *= v.x;
            way.add_scaled(&side, v.y);
            way.get_hpr().0
        } else {
            w.phys.target_heading
        };
        w.set_target_heading(h);
    }

    /// In flight on a floor: a push down into it goes across it, its whole
    /// strength, so a run along the ground is the run's speed with the camera
    /// looking down at it (the game's 15 degrees is a look down). Its way up, as
    /// it was: that is how a body takes off.
    fn fly_on_the_floor(&mut self) {
        let p = &mut self.cam.walk.phys;
        if !stands(p) || F::ZERO <= p.force.y {
            return;
        }
        let across = (p.force.x * p.force.x + p.force.z * p.force.z).sqrt();
        if F::ZERO < across {
            let k = p.force.magnitude() / across;
            p.force.x *= k;
            p.force.z *= k;
            p.force.y = F::ZERO;
        }
    }

    /// A place given the follow orbit's camera on the walk put back
    /// (`set_orbit_angle`, which puts the camera at once at its angle round
    /// the walk as it stands: the mouse's steer and look, a finger's, the
    /// wheel's style), moved by however far the walk was put back: onto the
    /// walk as last drawn, as every such place was given before the walk kept
    /// the console's frames. The camera's `Process` takes the orbit's angle
    /// from the way the camera is from the body as drawn now; given round the
    /// console frame kept, a place was off the body's line by however far the
    /// body had been drawn on from that frame across it, a part of a console
    /// frame's way or more, so with a sidestep (A or D steered by the right
    /// button, or the left on the move) the angle went 0.7 to 4 degrees off
    /// the body's back and back again at the console's 30 frames a second, the
    /// eye jumping as much as 0.2 a frame drawn at 144 (F3 20261029-024411:
    /// "the camera becomes very jittery and as such blurry"), and a finger's
    /// look while running jumped too. Given round the walk as last drawn,
    /// `Process` finds the body one frame drawn on, as with the keys alone.
    fn camera_as_drawn(&mut self) {
        let Some((found, back)) = self.given.take() else { return };
        let cam = &mut self.cam;
        let bits = |v: &Vect3<Ee>| [v.x.bits(), v.y.bits(), v.z.bits()];
        if cam.camera_mode != 3 || bits(&found) == bits(&cam.cur.camera_pos) {
            return;
        }
        let p = cam.cur.camera_pos;
        let on = |u: F<Ee>, d: f64| F::new((u.0 as f64 + d) as f32);
        let moved = Vect3::new(on(p.x, back[0]), on(p.y, back[1]), on(p.z, back[2]));
        cam.cur.camera_pos = moved;
        cam.last.camera_pos = moved;
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
        let landed = !self.flying && old & ON_FLOOR == 0 && new & ON_FLOOR != 0 && new & SUBMERGED == 0 && old & SUBMERGED == 0;
        w.parm118 = if landed { 16 } else { 0 };
        k.left = (w.phys, w.committed);
        self.rewound = false;
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
        !self.flying && self.cam.walk.is_under_liquid()
    }
    fn at_liquid_surface(&self) -> bool {
        !self.flying && self.cam.walk.is_at_liquid_surface()
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
        if self.flying {
            self.fly_view(r, x, y, false);
        } else {
            self.cam.walk.view_relative_move(r, x, y);
        }
    }
    fn view_relative_walk(&mut self, r: &Vect3<Ee>, x: F<Ee>, y: F<Ee>) {
        if self.flying {
            self.fly_view(r, x, y, true);
        } else {
            self.cam.walk.view_relative_walk(r, x, y);
        }
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
    /// The orbit's look by the stick: the mouse's tilt (`mouse::tilt`: the camera
    /// higher looks down) at the rate the body pitches in the other cameras
    /// (`m_pitchRate`, a quarter turn in two seconds), by the clock.
    fn fly_look(&mut self, t: F<Ee>) {
        let rate = self.cam.walk.pitch_rate;
        crate::mouse::tilt(&mut self.cam, t * rate * F::new(self.frame_s));
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

    /// The console's frame `n` from `t0` (`t0` the walk's own time when it
    /// began): `UpdateControls`, then the camera's `Process` with the walk
    /// stepped by the game clock's step (`console_step`), at the frame's
    /// place on the wall clock: one `Process` a frame, as the PS2 runs it.
    fn console_frame(w: &mut Walker, c: Controls, t0: i64, n: i64) {
        crate::controls::update_controls(w, c.axes(), &crate::controls::Client::default());
        let now = t0 + (n as f64 * CONSOLE_FRAME_MS) as i64;
        w.cam.walk.committed.time = now - console_step(n);
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
    /// console at 29.97 frames a second keeps (one `Process` a frame, the walk
    /// stepped by VITimer's 32 or 34: `console_frame`), so it runs and turns
    /// as fast at every rate, and the run drawn at 165 goes as fast as the
    /// console's. The controls: one `Process` a frame on the milliseconds
    /// (33 or 34, the walk's frames until 2026-11-03) is not the console's;
    /// stepped once a frame drawn, as until 2026-10-27, it is 4% faster at
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
        // the console's own frames, one Process each
        let mut a = open_ground().unwrap();
        for n in 1..=want.len() as i64 {
            console_frame(&mut a, go, t0, n);
        }
        assert!(same(&a.cam.walk.phys, want.last().unwrap()), "one Process a console frame, on VITimer's steps, is the console's");
        // the control: the same frames stepped on the milliseconds
        let mut m = open_ground().unwrap();
        for n in 1..=want.len() as i64 {
            frame_drawn(&mut m, go, console(n));
        }
        assert!(!same(&m.cam.walk.phys, want.last().unwrap()), "the control: one Process a frame on the milliseconds is not the console's");
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

    /// The game clock's step into each console frame is VITimer's: 32 or
    /// 34, three in five 32, an even count summing to 983.04 a second.
    #[test]
    fn a_console_frame_steps_the_walk_by_vitimers_count() {
        let steps: Vec<i64> = (1..=3000).map(console_step).collect();
        assert!(steps.iter().all(|&s| s == 32 || s == 34), "{:?}", &steps[..10]);
        let short = steps.iter().filter(|&&s| s == 32).count() as f64 / steps.len() as f64;
        assert!((short - 0.6).abs() < 0.01, "{short}");
        let total: i64 = steps.iter().sum();
        let wall = 3000.0 * CONSOLE_FRAME_MS;
        assert!((total as f64 / wall - 0.98304).abs() < 1e-4, "{total} counts in {wall} ms");
    }

    /// Walking slowly where a walk's way rounds up: toward -z from Mesh's
    /// place at Tunaria's z 31573 (one float step 1.95 mm) toward Warlord
    /// Jurglash, on a stick held at a walk (2026-11-03: "when walking slow
    /// with controller it is still rapidly jitters animation"). Drawn at 165
    /// frames a second, the body walks on every frame once it is under way.
    /// The control: the console's frames stepped on the milliseconds (33 or
    /// 34, the walk's until then), whose 1 ms rest reads a walk's float step
    /// or two of way, rounded up by the EE's chopped add, as a run (3.9 a
    /// second): the body ran one frame and walked the next.
    #[test]
    fn walking_slowly_toward_a_lower_coordinate_keeps_its_gait() {
        let (x, z, heading) = (25393.32f32, 31573.49f32, 3.0929f32);
        let walk = Controls { fwd: 0.42, ..Default::default() };
        let start = || {
            let mut w = Walker::arrive(tunaria_round(x, z)?, Spawn::ground(x, z, heading));
            w.cam.set_camera_mode(2);
            Some(w)
        };
        let Some(mut w) = start() else { return };
        let t0 = w.scene.time;
        let mut gaits = Vec::new();
        for k in 1..=(165 * 6) {
            w.frame(walk, drawn_at(t0, 165.0, 0.0, k));
            if k > 165 {
                gaits.push(w.cam.walk.committed.loco_state);
            }
        }
        assert!(w.cam.walk.phys.position.z.0 < z - 10.0, "it walked toward -z: {:?}", w.cam.walk.phys.position);
        assert!(gaits.iter().all(|&g| g == 1), "a walk on every frame drawn: {} not", gaits.iter().filter(|&&g| g != 1).count());
        let mut m = start().unwrap();
        let mut ran = 0;
        for n in 1..=180 {
            frame_drawn(&mut m, walk, t0 + (n as f64 * CONSOLE_FRAME_MS) as i64);
            ran += (n > 30 && m.cam.walk.committed.loco_state == 3) as usize;
        }
        assert!(ran > 50, "the control: on the milliseconds it reads a run on {ran} of 150 frames");
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
    /// The right button steering a run with the mouse on the move (camera
    /// mode 3: the look turns the body every frame drawn,
    /// `mouse::look_held`, as `Viewer::sticks` calls it), at 144 frames a
    /// second: from a stand the run gets up to the stick's pace, and let go
    /// of it stops as the stick's run stops. The look turns the walk as the
    /// last console frame left it (`rewind` first). The control: the look
    /// made on the walk as drawn, as until 2026-10-28, read as a walk moved
    /// whenever the mouse moved, so the console frames began again every
    /// frame drawn and none was stepped: the velocity stayed as it was when
    /// the mouse began to move, the run from a stand crept (F3
    /// 20261028-130331, 022421) and the run let go of slid on at its pace
    /// (054552). With the mouse still the look changes nothing and both
    /// step alike.
    #[test]
    fn a_run_steered_by_the_mouse_keeps_the_consoles_frames() {
        let go = Controls { fwd: 1.0, ..Default::default() }.axes();
        let Some(base) = open_ground() else { return };
        let t0 = base.scene.time;
        let moved = |a: Vect3<Ee>, b: Vect3<Ee>| ((b.x.0 - a.x.0).powi(2) + (b.z.0 - a.z.0).powi(2)).sqrt();
        // 144 frames a second: the stick on from frame `on` to frame 288 and
        // the button held from frame `held` on (`None`: never), the mouse a
        // pixel a frame; the pace over the run's last second, and the ground
        // covered from frame 288 to the end (one second on)
        let run = |on: u64, held: Option<u64>, fixed: bool| -> (f32, f32) {
            let mut w = open_ground().unwrap();
            crate::camstyle::set_style(&mut w.cam, 3, None, &mut w.scene);
            let mut path = Vec::new();
            for k in 1..=432 {
                let now = drawn_at(t0, 144.0, 0.0, k);
                let steer = held.is_some_and(|h| k >= h);
                if steer {
                    if fixed {
                        w.rewind();
                    }
                    let dx = if k % 2 == 0 { 1.0 } else { -1.0 };
                    crate::mouse::look_held(&mut w.cam, [false, true], dx, 0.0, false, 1.0 / 144.0, held.is_some_and(|h| k > h));
                }
                let c = crate::controls::Client { steer, ..Default::default() };
                w.frame_with(if (on..=288).contains(&k) { go } else { [0.0; 4] }, now, &c, true);
                path.push((now, w.cam.walk.phys.position));
            }
            (speed(&path[144..288]), moved(path[287].1, path[431].1))
        };
        let stick = run(1, None, true);
        let from_a_stand = (run(11, Some(1), true), run(11, Some(1), false));
        let while_running = (run(1, Some(216), true), run(1, Some(216), false));
        eprintln!("(pace, stop): the stick alone {stick:?}; the button from a stand {from_a_stand:?}; the button while running {while_running:?}");
        assert!(stick.0 > 8.0 && stick.1 < 3.0, "the stick's run: {stick:?}");
        for (case, (fixed, control)) in [("from a stand", from_a_stand), ("while running", while_running)] {
            assert!((fixed.0 / stick.0 - 1.0).abs() < 0.005, "{case}: the pace {fixed:?}, the stick's {stick:?}");
            assert!((fixed.1 - stick.1).abs() < 0.1, "{case}: the stop {fixed:?}, the stick's {stick:?}");
            if case == "from a stand" {
                assert!(control.0 < 0.7 * stick.0, "the control, {case}: it creeps, {control:?}");
            } else {
                assert!(control.1 > 6.0, "the control, {case}: it slides on, {control:?}");
            }
        }
    }

    /// The floor the walk's own probe finds under (x, z) from `y` down
    /// `reach` (a 0.001 sphere, `VIWalkFloorFilter`): the topmost.
    fn floor_under(w: &mut World<Ee>, x: f32, z: f32, y: f32, reach: f32) -> Option<f32> {
        let mut s = CollSphere::<Ee>::default();
        s.init(Vect3::of(x, y, z), Vect3::of(x, y - reach, z), F::new(0.001));
        let mut f = Filter::new(FilterKind::WalkFloor, -1);
        w.collide_sphere(&mut s, &mut f).then(|| s.hit_point().y.0)
    }

    /// A run steered by the mouse over Freeport's slopes (camera mode 3, the
    /// right button held, the mouse a pixel a frame either way, at 75 frames
    /// a second, jib's rate) is drawn on the ground under it all the way, as
    /// the console's own frames stand on it, and runs: up jib's slope
    /// south-west of the docks from a stand (F3 20261028-150758: "it phases
    /// characters into slopes, forces them to walk"), and down the side of
    /// the mound by the harbour with the button taken while running (022842:
    /// "you sink through it sometimes into the ground partially or float in
    /// the air as you move around it"). Drawn between two console frames on
    /// a straight line, the feet are within a few centimetres of the ground
    /// (2 cm here; 3 mm in 99 frames of 100 round Freeport). The control: the
    /// look made on the walk as drawn, as until c752913: the console frames
    /// begun again every frame drawn, only the place, the facing and the
    /// height offset were drawn on, and the rest of the walk stayed as it
    /// was when the mouse began to move: its speed, its locomotion and the
    /// floor it stood on. Each look-ahead's `Move` took the whole climb since
    /// that floor as one step and smoothed it (`m_heightOffset -= rise`,
    /// held at -0.6): the body ran 0.6 under the slope, standing (locomotion
    /// 0); going down, the step-down test failed against that floor and each
    /// look-ahead fell from rest: it floated over the slope, more than a
    /// metre by the mound's foot.
    #[test]
    fn a_run_steered_by_the_mouse_keeps_to_the_ground_of_a_slope() {
        let go = Controls { fwd: 1.0, ..Default::default() }.axes();
        // From 12 back of (x, z) along `heading`, 3.5 s, the button held from
        // frame `held` on, drawn at `hz`: the feet against the floor under
        // them at worst both ways (none within 3 under: 3 over) and the frames
        // drawn running (locomotion 3), from the hold's second second on.
        let run = |(x, y, z): (f32, f32, f32), heading: f32, held: u64, hz: f64, fixed: bool| -> Option<(f32, f32, usize, usize)> {
            let mut fw = Vect3::<Ee>::default();
            fw.hpr(F::new(heading), F::ZERO, F::ZERO);
            let (sx, sz) = (x - fw.x.0 * 12.0, z - fw.z.0 * 12.0);
            let mut world = tunaria_round(sx, sz)?;
            let sy = floor_under(&mut world, sx, sz, y + 2.0, 8.0)?;
            let mut w = Walker::arrive(world, Spawn { pos: [sx, sy, sz], heading, placing: Placing::AsGiven });
            crate::camstyle::set_style(&mut w.cam, 3, None, &mut w.scene);
            let t0 = w.scene.time;
            let (mut sunk, mut over, mut running, mut late) = (0.0f32, 0.0f32, 0, 0);
            for k in 1.. {
                let now = drawn_at(t0, hz, 0.0, k);
                if now - t0 > 3500 {
                    break;
                }
                let steer = k >= held;
                if steer {
                    if fixed {
                        w.rewind();
                    }
                    let dx = if k % 2 == 0 { 1.0 } else { -1.0 };
                    crate::mouse::look_held(&mut w.cam, [false, true], dx, 0.0, false, 1.0 / hz as f32, k > held);
                }
                w.frame_with(go, now, &crate::controls::Client { steer, ..Default::default() }, true);
                if !steer {
                    continue;
                }
                let feet = w.cam.walk.phys.smooth_location();
                let e = floor_under(&mut w.scene, feet.x.0, feet.z.0, feet.y.0 + 1.2, 4.2).map_or(3.0, |g| feet.y.0 - g);
                (sunk, over) = (sunk.min(e), over.max(e));
                if drawn_at(t0, hz, 0.0, held) + 1000 <= now {
                    late += 1;
                    running += (w.cam.walk.phys.loco_state == 3) as usize;
                }
            }
            Some((sunk, over, running, late))
        };
        let up = ((25291.62f32, 55.656654f32, 15822.053f32), 3.0 * std::f32::consts::FRAC_PI_2, 1);
        let down = ((25429.738f32, 58.026684f32, 15730.957f32), std::f32::consts::PI, 75);
        let Some(console) = run(up.0, up.1, up.2, 1000.0 / CONSOLE_FRAME_MS, true) else { return };
        for (case, (at, heading, held)) in [("up jib's slope from a stand", up), ("down the mound while running", down)] {
            let fixed = run(at, heading, held, 75.0, true).unwrap();
            let control = run(at, heading, held, 75.0, false).unwrap();
            eprintln!("{case}: (most sunk, most over, frames running of): {fixed:?}, the control {control:?}");
            assert!(fixed.0 > -0.05 && fixed.1 < 0.05, "{case}: the feet off the ground by {fixed:?}");
            assert!(fixed.2 * 10 >= fixed.3 * 9, "{case}: running on {} of {} frames", fixed.2, fixed.3);
            if held == 1 {
                assert!(control.0 < -0.5 && control.2 == 0, "the control, {case}: under the slope and standing, {control:?}");
            } else {
                assert!(control.1 > 1.0, "the control, {case}: over the slope, {control:?}");
            }
        }
        // the console's own frames stand on that ground (drawn at its rate,
        // each frame drawn is one)
        eprintln!("up jib's slope at the console's rate: {console:?}");
        assert!(console.0 > -0.002 && console.1 < 0.002, "the console's frames off the ground by {console:?}");
    }

    /// `frame_with`'s steps as they were until 2026-10-29 (the control): a
    /// place given the camera on the walk put back left round the console
    /// frame kept.
    fn frame_given_round_the_kept_frame(w: &mut Walker, keys: [f32; 4], now: i64, c: &crate::controls::Client) {
        w.rewind();
        crate::controls::update_controls(w, keys, c);
        let dt_s = (now - w.scene.time).clamp(0, 100) as f32 / 1000.0;
        w.step_kept(now);
        let ahead = w.ahead();
        w.scene.time = now;
        w.draw_between(&ahead, now);
        w.given = None;
        w.eye = w.reach.process(&mut w.cam, &mut w.scene, dt_s);
        w.keep_left();
    }

    /// A run with a sidestep steered by the mouse (W and A with the left
    /// button held on the move, which steers as the right does: the walk on
    /// the body's own axes and the camera put behind the body by the look
    /// every frame drawn, `mouse::look_held` after `rewind`, as
    /// `Viewer::sticks` calls them), at 144 frames a second and jib's 91, the
    /// mouse still or a pixel a frame: the eye moves as evenly as with the
    /// keys alone (its largest change of pace from one frame drawn to the
    /// next no larger, the floats' own steps at x 22000 apart) and the camera
    /// keeps one angle off the body's back (the frame drawn's sidestep, as
    /// before the walk kept the console's frames); a finger's look while
    /// running too. The control: the place the look gives left round the
    /// console frame kept (`frame_with` without `camera_as_drawn`, as until
    /// 2026-10-29), where the camera's `Process` found the body drawn on
    /// across it: the angle off the back swung 0.7 to 4 degrees at the
    /// console's 30 frames a second and the eye jumped 0.2 (F3
    /// 20261029-024411: "the camera becomes very jittery and as such
    /// blurry"). The walk keeps the same console frames either way, bit for
    /// bit, and with the keys alone the camera is the same too.
    #[test]
    fn a_sidestep_steered_by_the_mouse_keeps_the_camera_smooth() {
        use std::f64::consts::{PI, TAU};
        #[derive(Clone, Copy, Debug)]
        enum Look {
            Keys,
            /// the left button held, the mouse this many pixels a frame
            Steer(f32),
            /// a finger's drag in the follow orbit, a pixel a frame
            Finger,
        }
        struct Run {
            /// the eye's largest change of pace (mm a frame drawn), over the last 2 s
            jump: f64,
            /// how far its angle off the body's back swings (degrees), over the last 2 s
            swing: f64,
            kept: Vec<WalkPhysics<Ee>>,
            eyes: Vec<[u32; 3]>,
        }
        let go = Controls { fwd: 1.0, strafe: -1.0, ..Default::default() }.axes();
        let Some(base) = open_ground() else { return };
        let t0 = base.scene.time;
        let run = |hz: f64, look: Look, fixed: bool| -> Run {
            let mut w = open_ground().unwrap();
            crate::camstyle::set_style(&mut w.cam, 3, None, &mut w.scene);
            let (mut at, mut off, mut kept, mut eyes) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
            for k in 1..=(hz * 3.0) as u64 {
                let now = drawn_at(t0, hz, 0.0, k);
                w.rewind();
                match look {
                    Look::Steer(dx) => {
                        crate::mouse::look_held(&mut w.cam, [true, true], dx, 0.0, false, 1.0 / hz as f32, k > 1);
                    }
                    Look::Finger => {
                        crate::mouse::look_cam(&mut w.cam, 1.0, 0.0, 0.0, false, 1.0 / hz as f32);
                    }
                    Look::Keys => {}
                }
                let c = crate::controls::Client { steer: matches!(look, Look::Steer(_)), ..Default::default() };
                if fixed {
                    w.frame_with(go, now, &c, true);
                } else {
                    frame_given_round_the_kept_frame(&mut w, go, now, &c);
                }
                let k = w.kept.unwrap();
                if (kept.len() as i64) < k.frames {
                    kept.push(k.at.0);
                }
                let (e, b) = (w.eye.position, w.cam.walk.phys.position);
                eyes.push([e.x.bits(), e.y.bits(), e.z.bits()]);
                if now - t0 >= 1000 {
                    at.push((e.x.0 as f64, e.z.0 as f64));
                    let way = (e.x.0 as f64 - b.x.0 as f64).atan2(e.z.0 as f64 - b.z.0 as f64);
                    let back = w.cam.walk.phys.angles.x.0 as f64 + PI;
                    off.push(((way - back + PI).rem_euclid(TAU) - PI).to_degrees());
                }
            }
            let jump = at.windows(3).map(|p| (p[2].0 - 2.0 * p[1].0 + p[0].0).hypot(p[2].1 - 2.0 * p[1].1 + p[0].1) * 1000.0).fold(0.0, f64::max);
            let swing = off.iter().cloned().fold(f64::MIN, f64::max) - off.iter().cloned().fold(f64::MAX, f64::min);
            Run { jump, swing, kept, eyes }
        };
        let same_frames = |a: &Run, b: &Run| a.kept.len() == b.kept.len() && a.kept.iter().zip(&b.kept).all(|(x, y)| same(x, y));
        let (keys, keys_then) = (run(144.0, Look::Keys, true), run(144.0, Look::Keys, false));
        assert!(same_frames(&keys, &keys_then) && keys.eyes == keys_then.eyes, "the keys alone: the walk and the camera as before, bit for bit");
        assert!(keys.kept.len() >= 89, "{} console frames", keys.kept.len());
        let keys91 = run(91.0, Look::Keys, true);
        eprintln!("the keys alone: the eye's largest jump {:.2} mm at 144 Hz, {:.2} at 91", keys.jump, keys91.jump);
        for (hz, look, smooth) in [(144.0, Look::Steer(0.0), keys.jump), (144.0, Look::Steer(1.0), keys.jump), (91.0, Look::Steer(0.0), keys91.jump), (144.0, Look::Finger, keys.jump)] {
            let (now, then) = (run(hz, look, true), run(hz, look, false));
            eprintln!("{look:?} at {hz} Hz: the eye's largest jump {:.2} mm, its angle off the back swinging {:.3} degrees; the control {:.2} mm, {:.3} degrees", now.jump, now.swing, then.jump, then.swing);
            assert!(now.jump < smooth * 1.25, "{look:?} at {hz} Hz: the eye jumps {} mm, with the keys alone {smooth}", now.jump);
            if let Look::Steer(_) = look {
                assert!(now.swing < 0.5, "{look:?} at {hz} Hz: the camera swings {} degrees round the body", now.swing);
                assert!(then.jump > 100.0 && then.swing > 2.5, "the control, {look:?} at {hz} Hz: {} mm, {} degrees", then.jump, then.swing);
                assert!(same_frames(&now, &then), "{look:?} at {hz} Hz: the walk keeps the same console frames");
            } else {
                assert!(then.jump > 50.0, "the control, {look:?}: {} mm", then.jump);
            }
        }
    }

    // A GM's flight (the module's doc). Open air over Tunaria's flat at
    // (22000, 18000): the ground at 54.1, nothing over it.

    fn flying() -> crate::controls::Client {
        crate::controls::Client { flying: true, ..Default::default() }
    }

    /// `secs` of the frames drawn at `hz` from the walk's clock on, at `client`
    /// and the sticks `c`; the time it ends at.
    fn hold(w: &mut Walker, c: Controls, client: &crate::controls::Client, hz: f64, secs: f64) -> i64 {
        let t0 = w.scene.time;
        let mut now = t0;
        for k in 1..=(hz * secs) as u64 {
            now = drawn_at(t0, hz, 0.0, k);
            w.frame_with(c.axes(), now, client, true);
        }
        now
    }

    /// The way covered between frames, over the time they span: up and down too.
    fn speed3(a: &[(i64, Vect3<Ee>)]) -> f32 {
        let d: f32 = a
            .windows(2)
            .map(|p| ((p[1].1.x.0 - p[0].1.x.0).powi(2) + (p[1].1.y.0 - p[0].1.y.0).powi(2) + (p[1].1.z.0 - p[0].1.z.0).powi(2)).sqrt())
            .sum();
        d / ((a[a.len() - 1].0 - a[0].0) as f32 / 1000.0)
    }

    /// A run of `secs` at `hz` with the walk under way, its place each frame.
    fn track(w: &mut Walker, c: Controls, client: &crate::controls::Client, hz: f64, secs: f64) -> Vec<(i64, Vect3<Ee>)> {
        hold(w, c, client, hz, 1.0);
        let t0 = w.scene.time;
        let mut at = vec![(t0, w.cam.walk.phys.position)];
        for k in 1..=(hz * secs) as u64 {
            let now = drawn_at(t0, hz, 0.0, k);
            w.frame_with(c.axes(), now, client, true);
            at.push((now, w.cam.walk.phys.position));
        }
        at
    }

    /// In the follow orbit (the game's default) W goes where the camera looks, its
    /// default tilt counting as level. At the default it goes across at the height
    /// it stands at; tilted up from there it climbs along the tilt, level again it
    /// holds the height it reached, tilted down from there it comes down along the
    /// tilt, each at the ground's run, up and down counted. It then runs along the
    /// ground at the run's whole speed with the camera still looking down. The
    /// controls: on the ground with no flight the same tilt runs across at that
    /// speed and rises not at all.
    #[test]
    fn in_flight_the_body_goes_where_the_camera_looks() {
        let (x, z) = (22000.0f32, 18000.0f32);
        let start = || {
            let mut w = Walker::arrive(tunaria_round(x, z)?, Spawn::ground(x, z, std::f32::consts::FRAC_PI_2));
            // the game's own camera: its 15 degrees, a look down at the head
            crate::camstyle::set_style(&mut w.cam, 3, None, &mut w.scene);
            Some(w)
        };
        let up_by = |w: &mut Walker, d: f32| crate::mouse::tilt(&mut w.cam, F::new(-d));
        let Some(mut ground) = start() else { return };
        let go = Controls { fwd: 1.0, ..Default::default() };
        let plain = crate::controls::Client::default();
        let on_foot = speed(&track(&mut ground, go, &plain, 60.0, 2.0));
        eprintln!("on the ground: {on_foot:.3} a second");
        let floor = ground.cam.walk.phys.position.y.0;
        assert!((floor - 54.1).abs() < 0.2, "the flat: {floor}");

        let mut w = start().unwrap();
        // at the game's own camera, which looks down 15 degrees, it goes straight
        // along: a run on the flat at the run's speed
        let at = track(&mut w, go, &flying(), 60.0, 1.0);
        assert!((at[at.len() - 1].1.y.0 - at[0].1.y.0).abs() < 0.3 && (speed(&at) / on_foot - 1.0).abs() < 0.01, "the game's camera: {} a second, height {} to {}", speed(&at), at[0].1.y.0, at[at.len() - 1].1.y.0);

        up_by(&mut w, 0.5); // 0.5 up from the default
        hold(&mut w, go, &flying(), 60.0, 0.2);
        let eye = w.eye.direction.get_hpr().1.0;
        assert!((eye - (crate::camreach::GAME_PITCH - 0.5)).abs() < 0.03, "the eye looks up by what is left of its 15 degrees: {eye}");
        let at = track(&mut w, go, &flying(), 60.0, 3.0);
        let (a, b) = (at[0].1, at[at.len() - 1].1);
        let (up, across) = (b.y.0 - a.y.0, ((b.x.0 - a.x.0).powi(2) + (b.z.0 - a.z.0).powi(2)).sqrt());
        eprintln!("0.5 up from the default: {up:.2} up and {across:.2} across in 3 s, {:.3} a second", speed3(&at));
        assert!((up / across - 0.5f32.tan()).abs() < 0.03, "climbs along its tilt: {up} up, {across} across");
        assert!((speed3(&at) / on_foot - 1.0).abs() < 0.01, "at the run's speed: {} against {on_foot}", speed3(&at));
        assert!(w.cam.walk.phys.position.y.0 > floor + 15.0);

        // back at the default, level at the height it reached
        up_by(&mut w, -0.5);
        hold(&mut w, go, &flying(), 60.0, 1.0);
        let at = track(&mut w, go, &flying(), 60.0, 1.0);
        let (a, b) = (at[0].1, at[at.len() - 1].1);
        assert!((b.y.0 - a.y.0).abs() < 0.5, "level: {} to {}", a.y.0, b.y.0);
        assert!((speed3(&at) / on_foot - 1.0).abs() < 0.01, "level at the run's speed: {}", speed3(&at));

        // down along a look down, to the ground, and on along it at the run
        up_by(&mut w, -0.5);
        let top = w.cam.walk.phys.position.y.0;
        hold(&mut w, go, &flying(), 60.0, 1.0);
        assert!(w.cam.walk.phys.position.y.0 < top - 2.0, "comes down: {top} to {}", w.cam.walk.phys.position.y.0);
        hold(&mut w, go, &flying(), 60.0, 12.0);
        let at = track(&mut w, go, &flying(), 60.0, 1.0);
        let y = at[at.len() - 1].1.y.0;
        assert!((y - crate::walker::ground(&mut w.scene, at[at.len() - 1].1.x.0, at[at.len() - 1].1.z.0).unwrap()).abs() < 0.3, "on the ground: {y}");
        assert!((speed(&at) / on_foot - 1.0).abs() < 0.01, "along the ground, looking down at it: {} against {on_foot}", speed(&at));

        // the control: the same tilt, no flight
        let mut g = start().unwrap();
        up_by(&mut g, 0.5);
        hold(&mut g, go, &plain, 60.0, 0.2);
        let at = track(&mut g, go, &plain, 60.0, 2.0);
        assert!((at[at.len() - 1].1.y.0 - at[0].1.y.0).abs() < 0.3, "the control rose: {} to {}", at[0].1.y.0, at[at.len() - 1].1.y.0);
        assert!((speed(&at) / on_foot - 1.0).abs() < 0.01);
    }

    /// A camera at its default flies level, whichever it is and at any rate: W
    /// for 6 s high over Tunaria (at 300.5, where `Move`'s add of the body's height
    /// and back loses a float step a tick: `process_aloft`) holds the height to the
    /// bit, at the run's speed across the ground, in the follow orbit (the eye looking
    /// down its 15 degrees: the control, a look taken literally would sink at 2.2 a
    /// second), the orbit (its
    /// 25) and first person and the follow camera (no tilt of theirs: their pitch 0
    /// looks level). So does the follow orbit with a race's look-at height and
    /// zoomed, which turn the eye and not the tilt, and with its pitch swung by a
    /// snap, as near a wall. The other controls: a small tilt from the default climbs
    /// and dives by its sine, in each orbit.
    #[test]
    fn a_camera_at_its_default_flies_level_in_every_camera_at_any_rate() {
        let (x, z) = (22000.0f32, 18000.0f32);
        let y0 = 300.5f32;
        let start = |style: i32, rc: Option<crate::camstyle::RaceCamera>| {
            let mut w = Walker::arrive(tunaria_round(x, z)?, Spawn { pos: [x, y0, z], heading: std::f32::consts::FRAC_PI_2, placing: Placing::AsGiven });
            crate::camstyle::set_style(&mut w.cam, style, rc, &mut w.scene);
            Some(w)
        };
        let Some(mut ground) = tunaria_round(x, z).map(|s| Walker::arrive(s, Spawn::ground(x, z, std::f32::consts::FRAC_PI_2))) else { return };
        let go = Controls { fwd: 1.0, ..Default::default() };
        let on_foot = speed(&track(&mut ground, go, &crate::controls::Client::default(), 60.0, 2.0));
        // W for 6 s at `hz`: the height each frame drawn held to the bit
        let level = |w: &mut Walker, hz: f64, jitter: f64, what: &str| {
            hold(w, go, &flying(), hz, 0.5);
            let t0 = w.scene.time;
            let mut at = vec![(t0, w.cam.walk.phys.position)];
            for k in 1..=(hz * 6.0) as u64 {
                let now = drawn_at(t0, hz, jitter, k);
                w.frame_with(go.axes(), now, &flying(), true);
                let p = w.cam.walk.phys.position;
                assert_eq!(p.y.0, y0, "{what} at {hz} Hz: frame {k}");
                assert_eq!(vi::selfstate::encode(0, &w.cam.walk.committed, 0, 0)[35], 3, "{what} at {hz} Hz: frame {k} runs");
                at.push((now, p));
            }
            speed(&at)
        };
        for (style, name) in [(3, "the follow orbit"), (1, "the orbit"), (0, "first person"), (2, "the follow camera")] {
            for (hz, jitter) in [(30.0, 0.0), (60.0, 0.0), (165.0, 0.0), (240.0, 0.0), (144.0, 1.5)] {
                let mut w = start(style, None).unwrap();
                let v = level(&mut w, hz, jitter, name);
                if hz == 60.0 {
                    eprintln!("{name}: the eye's pitch {:.4}, {v:.3} a second across, the ground's {on_foot:.3}", w.eye.direction.get_hpr().1.0);
                }
                assert!((v / on_foot - 1.0).abs() < 0.01, "{name} at {hz} Hz: {v} a second, the ground's {on_foot}");
                let eye = w.eye.direction.get_hpr().1.0;
                match style {
                    3 => assert!((eye - crate::camreach::GAME_PITCH).abs() < 0.02, "the control: the eye looks down its 15 degrees: {eye}"),
                    1 => assert!(eye > 0.3, "the control: the eye looks down its 25 degrees: {eye}"),
                    _ => assert!(eye.abs() < 0.001, "{name}: its eye looks level: {eye}"),
                }
            }
        }
        // a race's look-at height, a zoom: the eye turns, the flight does not
        let short = crate::camstyle::RaceCamera { race: 3, target_height: 1.1, look_at_height_above: 1.1, pitch: 0.2, distance: 3.0, p34: 0.9, review: [0.0; 4], cust: [0.0; 4] };
        let mut w = start(3, Some(short)).unwrap();
        w.cam.set_orbit_distance(F::new(2.0));
        hold(&mut w, go, &flying(), 60.0, 0.5);
        let eye = w.eye.direction.get_hpr().1.0;
        assert!(eye > crate::camreach::GAME_PITCH + 0.05, "the control: the race's look-at height turns the eye: {eye}");
        level(&mut w, 60.0, 0.0, "a short race zoomed in");
        // the pitch swung by a snap, as near a wall: the tilt chosen is unchanged
        let mut w = start(3, None).unwrap();
        hold(&mut w, go, &flying(), 60.0, 0.5);
        let t0 = w.scene.time;
        w.cam.set_orbit_pitch_angle(w.cam.alt_orbit_pitch);
        for k in 1..=6 {
            w.frame_with(go.axes(), t0 + k * 16, &flying(), true);
            assert_eq!(w.cam.walk.phys.position.y.0, y0, "swung to its alternate pitch: frame {k}");
        }
        // a small tilt from the default, up and down, each orbit its own default
        for style in [3, 1] {
            for tilt in [0.1f32, -0.1] {
                let mut w = start(style, None).unwrap();
                crate::mouse::tilt(&mut w.cam, F::new(tilt));
                hold(&mut w, go, &flying(), 60.0, 1.0);
                let a = w.cam.walk.phys.position.y.0;
                hold(&mut w, go, &flying(), 60.0, 3.0);
                let climbed = (w.cam.walk.phys.position.y.0 - a) / 3.0;
                let want = -tilt.sin() * on_foot;
                assert!((climbed - want).abs() < 0.1, "style {style}, {tilt} from the default: {climbed} a second up, want {want}");
            }
        }
    }

    /// The tilts' limits: the follow orbit's 45 degrees under the head climbs at
    /// 60 degrees from its default (and the orbit at 70), all the run's speed along
    /// it, more of it up than the eye's own look gave. Its 75 overhead dives at 60.
    #[test]
    fn at_the_tilts_limits_the_flight_goes_at_the_run() {
        let (x, z) = (22000.0f32, 18000.0f32);
        let y0 = 400.0f32;
        let go = Controls { fwd: 1.0, ..Default::default() };
        let Some(mut ground) = tunaria_round(x, z).map(|s| Walker::arrive(s, Spawn::ground(x, z, std::f32::consts::FRAC_PI_2))) else { return };
        let on_foot = speed(&track(&mut ground, go, &crate::controls::Client::default(), 60.0, 2.0));
        for (style, name, limit, want) in [
            (3, "the follow orbit", crate::mouse::MIN_TILT, -60.0f32),
            (3, "the follow orbit", crate::mouse::MAX_TILT, 60.0),
            (1, "the orbit", crate::mouse::MIN_TILT, -70.0),
            (1, "the orbit", crate::mouse::MAX_TILT, 50.0),
        ] {
            let mut w = Walker::arrive(tunaria_round(x, z).unwrap(), Spawn { pos: [x, y0, z], heading: std::f32::consts::FRAC_PI_2, placing: Placing::AsGiven });
            crate::camstyle::set_style(&mut w.cam, style, None, &mut w.scene);
            crate::mouse::tilt(&mut w.cam, F::new(limit * 3.0));
            let at = track(&mut w, go, &flying(), 60.0, 2.0);
            let (a, b) = (at[0].1, at[at.len() - 1].1);
            let (up, all) = ((b.y.0 - a.y.0) / 2.0, speed3(&at));
            eprintln!("{name} at its tilt limit {limit:.3}: {up:.3} a second up, {all:.3} in all (the ground's {on_foot:.3})");
            assert!((up / all + want.to_radians().sin()).abs() < 0.02, "{name}: {up} up of {all}, want {want} degrees");
            assert!((all / on_foot - 1.0).abs() < 0.015, "{name}: {all} a second, the ground's {on_foot}");
        }
    }

    /// First person is the swim's own way: the stick pitches the body (past the
    /// walk's range for a body on land, as under liquid) and W goes along it.
    /// Near straight up (`PITCH_LIMIT`, 89.4 degrees) nearly all the run goes
    /// up. The control: looking up on the ground runs across.
    #[test]
    fn looking_straight_up_in_first_person_goes_up_at_the_run() {
        let (x, z) = (22000.0f32, 18000.0f32);
        let start = || {
            let mut w = Walker::arrive(tunaria_round(x, z)?, Spawn::ground(x, z, std::f32::consts::FRAC_PI_2));
            w.cam.set_camera_mode(0);
            Some(w)
        };
        let Some(mut w) = start() else { return };
        let plain = crate::controls::Client::default();
        let go = Controls { fwd: 1.0, ..Default::default() };
        hold(&mut w, Controls { pitch: -1.0, ..Default::default() }, &flying(), 60.0, 3.0);
        assert!(w.cam.walk.phys.angles.y.0 < -1.5, "the stick pitched it up: {:?}", w.cam.walk.phys.angles.y);
        crate::mouse::pitch_body(&mut w.cam, F::new(-2.0));
        assert!((w.cam.walk.phys.angles.y.0 + 1.5607964).abs() < 1e-5, "the mouse's to the limit: {:?}", w.cam.walk.phys.angles.y);
        let at = track(&mut w, go, &flying(), 60.0, 3.0);
        let (a, b) = (at[0].1, at[at.len() - 1].1);
        let up = (b.y.0 - a.y.0) / 3.0;
        eprintln!("looking straight up: {up:.3} a second up, {:.3} a second in all", speed3(&at));
        assert!(up > 8.0, "up at the run's speed: {up}");

        let mut g = start().unwrap();
        crate::mouse::pitch_body(&mut g.cam, F::new(-2.0));
        assert!(g.cam.walk.phys.angles.y.0 >= g.cam.walk.min_pitch.0, "the control's mouse keeps to the walk's range");
        let at = track(&mut g, go, &plain, 60.0, 2.0);
        assert!((at[at.len() - 1].1.y.0 - at[0].1.y.0).abs() < 0.3, "the control ran across: {:?}", at[at.len() - 1].1);
    }

    /// The flight keeps the console's frames at any rate, as the walk does: a
    /// climb and a turn on the sticks drawn at 60, 165 and 240 frames a second,
    /// and at 144 up to 1.5 ms off, keeps bit for bit what one console frame a
    /// frame keeps, and goes as fast.
    #[test]
    fn the_flight_keeps_the_consoles_frames_at_any_display_rate() {
        let go = Controls { fwd: 1.0, orbit: -0.25, pitch: -0.15, ..Default::default() };
        let Some(mut c) = open_ground() else { return };
        let t0 = c.scene.time;
        let end = t0 + 5000;
        let console = |n: i64| t0 + (n as f64 * CONSOLE_FRAME_MS) as i64;
        let (mut want, mut path) = (Vec::new(), vec![(t0, c.cam.walk.phys.position)]);
        let mut n = 1;
        while console(n) <= end {
            c.frame_with(go.axes(), console(n), &flying(), true);
            let k = c.kept.unwrap();
            assert_eq!(k.frames, n, "one console frame a frame drawn");
            want.push(k.at.0);
            path.push((console(n), c.cam.walk.phys.position));
            n += 1;
        }
        let console_speed = speed3(&path);
        assert!(c.cam.walk.phys.position.y.0 > 60.0, "it climbed: {:?}", c.cam.walk.phys.position);
        for (hz, jitter) in [(60.0, 0.0), (165.0, 0.0), (240.0, 0.0), (144.0, 1.5)] {
            let mut w = open_ground().unwrap();
            let (mut seen, mut path) = (0, vec![(t0, w.cam.walk.phys.position)]);
            for k in 1.. {
                let now = drawn_at(t0, hz, jitter, k);
                if now > end {
                    break;
                }
                w.frame_with(go.axes(), now, &flying(), true);
                path.push((now, w.cam.walk.phys.position));
                let kept = w.kept.unwrap();
                if kept.frames > seen {
                    seen = kept.frames;
                    let (a, b) = (kept.at.0, want[seen as usize - 1]);
                    assert!(same(&a, &b), "{hz} Hz: console frame {seen} kept at {:?}, the console's at {:?}", a.position, b.position);
                }
            }
            assert_eq!(seen as usize, want.len(), "{hz} Hz: every console frame kept");
            let drawn = speed3(&path);
            eprintln!("in flight at {hz} Hz (jitter {jitter}): {drawn:.4} a second drawn, the console's {console_speed:.4}");
            assert!((drawn / console_speed - 1.0).abs() < 0.003, "{hz} Hz: drawn at {drawn}, the console's {console_speed}");
        }
    }

    /// Let go, it holds where it is: no gravity, no drift, in the follow orbit
    /// and first person, standing (the gait 0). Off, it falls from there and
    /// lands, the fall's own gait (6) on the way down and the ground's after, and
    /// a walk from there is the walk. The control: the gravity back at once, the
    /// fall begun within a console frame or two.
    #[test]
    fn let_go_it_holds_and_off_it_falls_from_where_it_is() {
        let (x, z) = (22000.0f32, 18000.0f32);
        for mode in [3, 0, 2, 1] {
            let Some(mut w) = tunaria_round(x, z).map(|s| Walker::arrive(s, Spawn::ground(x, z, std::f32::consts::FRAC_PI_2))) else { return };
            w.cam.set_camera_mode(mode);
            let floor = w.cam.walk.phys.position.y.0;
            if mode == 3 || mode == 1 {
                crate::mouse::tilt(&mut w.cam, F::new(-0.5235988 - 0.5));
                hold(&mut w, Controls::default(), &flying(), 30.0, 0.2);
            } else {
                crate::mouse::pitch_body(&mut w.cam, F::new(-0.6));
            }
            let go = Controls { fwd: 1.0, ..Default::default() };
            hold(&mut w, go, &flying(), 30.0, 4.0);
            let y = w.cam.walk.phys.position.y.0;
            assert!(y > floor + 10.0, "mode {mode}: it climbed from {floor} to {y}");
            hold(&mut w, Controls::default(), &flying(), 30.0, 2.0);
            let (still, p) = (w.cam.walk.phys.position, w.cam.walk.committed.loco_state);
            hold(&mut w, Controls::default(), &flying(), 30.0, 4.0);
            let held = w.cam.walk.phys.position;
            assert!(held.y == still.y && held.x == still.x && held.z == still.z, "mode {mode}: {still:?} to {held:?} with nothing held");
            assert_eq!(p, 0, "mode {mode}: stands");
            assert_eq!(w.cam.walk.committed.loco_state, 0, "mode {mode}: stands");

            // off
            let plain = crate::controls::Client::default();
            let top = held.y.0;
            hold(&mut w, Controls::default(), &plain, 30.0, 0.5);
            assert!(w.cam.walk.phys.position.y.0 < top - 0.5, "mode {mode}: no fall: {top} to {:?}", w.cam.walk.phys.position.y);
            assert_eq!(w.cam.walk.parm60, 1);
            assert_eq!(w.cam.walk.committed.loco_state, 6, "mode {mode}: the fall's own gait");
            let mut landed = false;
            for _ in 0..12 {
                hold(&mut w, Controls::default(), &plain, 30.0, 0.5);
                landed = (w.cam.walk.phys.position.y.0 - crate::walker::ground(&mut w.scene, held.x.0, held.z.0).unwrap()).abs() < 0.2;
                if landed {
                    break;
                }
            }
            assert!(landed, "mode {mode}: still falling at {:?}", w.cam.walk.phys.position.y);
            hold(&mut w, Controls::default(), &plain, 30.0, 1.0);
            assert_eq!(w.cam.walk.committed.loco_state, 0, "mode {mode}: stands on the ground");
            assert!(w.cam.walk.phys.flags & ON_FLOOR != 0, "mode {mode}: on the floor");
        }
    }

    /// The gaits are the land's: a run wherever it runs (up and down too), a walk
    /// on a stick held low, the sidesteps and a back-up on the body's own axes,
    /// a stand when still, and never a swim's. The control: the walk's own gait
    /// in the world of water is a swim's, so the land's is what `fly_gait` gives.
    #[test]
    fn in_flight_the_gaits_are_the_lands() {
        let Some(mut w) = open_ground() else { return };
        let gait = |w: &Walker| w.cam.walk.committed.loco_state;
        let mut all = Vec::new();
        let mut watch = |w: &mut Walker, c: Controls, secs: f64, want: i32, why: &str| {
            hold(w, c, &flying(), 60.0, 1.0); // the walk's own time to change its speed
            let t0 = w.scene.time;
            for k in 1..=(60.0 * secs) as u64 {
                w.frame_with(c.axes(), t0 + (k as f64 * 1000.0 / 60.0) as i64, &flying(), true);
                let (loco, wire) = (gait(w), vi::selfstate::encode(0, &w.cam.walk.committed, 0, 0)[35] as i32);
                assert_eq!(loco, wire, "{why}: the 0x40 says the gait");
                all.push(loco);
                assert_eq!(loco, want, "{why}: frame {k}");
            }
        };
        // (mode 2: the body's own axes)
        watch(&mut w, Controls { fwd: 1.0, ..Default::default() }, 1.0, 3, "a level run");
        watch(&mut w, Controls::default(), 1.0, 0, "let go");
        watch(&mut w, Controls { fwd: 1.0, pitch: -0.8, ..Default::default() }, 1.0, 3, "a climb");
        watch(&mut w, Controls { fwd: 1.0, pitch: 0.8, ..Default::default() }, 1.0, 3, "a dive");
        watch(&mut w, Controls { fwd: 0.4, ..Default::default() }, 1.0, 1, "a walk");
        watch(&mut w, Controls { strafe: 1.0, ..Default::default() }, 1.0, 11, "a sidestep");
        watch(&mut w, Controls { strafe: -1.0, ..Default::default() }, 1.0, 10, "the other way");
        watch(&mut w, Controls { fwd: -1.0, ..Default::default() }, 1.0, 2, "backing up");
        watch(&mut w, Controls::default(), 1.0, 0, "let go again");
        assert!(all.iter().all(|&g| g != 12 && g != 13));
        // the control
        let mut copy = w.cam.walk;
        copy.committed.time = w.scene.time - 33;
        copy.process(0, &mut Aloft(&mut w.scene));
        assert!(matches!(copy.committed.loco_state, 12 | 13), "the control: {}", copy.committed.loco_state);
    }

    /// What the 0x40 says of a flyer is what is really there: no liquid in the
    /// air, though the walk it stepped is in its world of water (the control: its
    /// kept frame is); over the bay in the air none; under the bay's surface it
    /// is in the water, as anyone, and the breath gauge's test says so; past the
    /// surface it went freely. The body is drawn upright under it all the same.
    #[test]
    fn a_flyers_walk_flags_are_the_places() {
        let (x, z) = (25390.0f32, 15860.0f32);
        let Some(world) = tunaria_round(x, z) else { return };
        let surface = 51.32f32;
        assert_eq!(world.env_flags(&Vect3::of(x, surface - 3.0, z)) & 1, 1, "the bay's water");
        assert_eq!(world.env_flags(&Vect3::of(x, surface + 3.0, z)) & 1, 0, "the air over it");
        let mut w = Walker::arrive(world, Spawn { pos: [x, surface + 20.0, z], heading: 0.0, placing: Placing::AsGiven });
        w.cam.set_camera_mode(0);
        hold(&mut w, Controls::default(), &flying(), 30.0, 0.5);
        let wire = |w: &Walker| {
            let b = vi::selfstate::encode(0, &w.cam.walk.committed, 0, 0);
            u32::from_le_bytes([b[31], b[32], b[33], b[34]])
        };
        assert_eq!(wire(&w) & flags::LIQUID, 0, "in the air: {:#x}", wire(&w));
        assert!(!w.cam.walk.is_under_liquid());
        assert_ne!(w.kept.unwrap().at.0.flags & flags::IN_WATER, 0, "the control: the frame kept is in the world of water");
        assert_eq!(w.cam.walk.parm60, 0);
        // straight down, through the surface, 10 under it
        crate::mouse::pitch_body(&mut w.cam, F::new(1.5));
        let go = Controls { fwd: 1.0, ..Default::default() };
        let t0 = w.scene.time;
        let mut k = 0;
        while w.cam.walk.phys.position.y.0 > surface - 12.0 {
            k += 1;
            assert!(k < 60 * 8, "stuck at {:?}", w.cam.walk.phys.position);
            w.frame_with(go.axes(), t0 + (k as f64 * 1000.0 / 60.0) as i64, &flying(), true);
        }
        hold(&mut w, Controls::default(), &flying(), 30.0, 2.0); // the run dies away
        assert_ne!(wire(&w) & flags::IN_WATER, 0, "under the bay: {:#x}", wire(&w));
        assert_eq!(wire(&w) & flags::AT_SURFACE, 0, "under the bay: {:#x}", wire(&w));
        assert!(w.cam.walk.is_under_liquid(), "the gauge's test");
        let y = w.cam.walk.phys.position.y.0;
        hold(&mut w, Controls::default(), &flying(), 30.0, 2.0);
        assert_eq!(w.cam.walk.phys.position.y.0, y, "no sinking");
        assert!(!matches!(w.cam.walk.committed.loco_state, 12 | 13), "no swim");
    }

    /// Collision is the swimmer's: a flyer stops under a roof (Gilgash's stall's,
    /// at 70) with its body's height to spare and slides no way through it, and
    /// rises past the height a step aside in the open. The control is that step.
    #[test]
    fn a_flyer_stops_under_a_roof() {
        let (x, y, z) = (25476.0f32, 54.398438f32, 15707.797f32);
        let Some(world) = tunaria_round(x, z) else { return };
        let roof = ground(&mut tunaria_round(x, z).unwrap(), x, z).unwrap();
        let mut w = Walker::arrive(world, Spawn { pos: [x, y, z], heading: 0.0, placing: Placing::AsGiven });
        w.cam.set_camera_mode(0);
        let up = Controls { fwd: 1.0, ..Default::default() };
        hold(&mut w, Controls::default(), &flying(), 30.0, 0.2);
        crate::mouse::pitch_body(&mut w.cam, F::new(-2.0));
        hold(&mut w, up, &flying(), 30.0, 5.0);
        let top = w.cam.walk.phys.position.y.0;
        eprintln!("under the roof at {roof}: stopped at {top}");
        assert!(top < roof - 1.0, "through the roof at {roof}: {top}");
        assert!(top > y + 1.0, "it rose: {y} to {top}");
    }

    /// A dump (a respawn, a restart of the server's) puts the body where it
    /// says, with no floor looked for, and it goes on flying there: no fall, with
    /// the same client's word. A walker made anew (a world change) flies from
    /// its first frame. The control: no flight, the same place falls.
    #[test]
    fn a_dump_and_a_new_world_leave_it_flying() {
        let (x, z) = (22000.0f32, 18000.0f32);
        let Some(mut w) = tunaria_round(x, z).map(|s| Walker::arrive(s, Spawn::ground(x, z, 1.5))) else { return };
        hold(&mut w, Controls::default(), &flying(), 30.0, 0.2);
        let high = [x + 5.0, 300.5, z];
        w.place(high, 1.5);
        hold(&mut w, Controls::default(), &flying(), 30.0, 2.0);
        assert_eq!(w.cam.walk.phys.position.to_array(), high, "the dump's place, held");
        assert_eq!(w.cam.walk.parm60, 0);

        let Some(mut n) = tunaria_round(x, z).map(|s| Walker::arrive(s, Spawn { pos: high, heading: 1.5, placing: Placing::AsGiven })) else { return };
        assert_eq!(n.cam.walk.parm60, 1, "a new walker's gravity is the walk's own");
        hold(&mut n, Controls::default(), &flying(), 30.0, 2.0);
        assert_eq!(n.cam.walk.parm60, 0);
        assert_eq!(n.cam.walk.phys.position.to_array(), high, "a new world's first frames");

        let Some(mut f) = tunaria_round(x, z).map(|s| Walker::arrive(s, Spawn { pos: high, heading: 1.5, placing: Placing::AsGiven })) else { return };
        hold(&mut f, Controls::default(), &crate::controls::Client::default(), 30.0, 2.0);
        assert!(f.cam.walk.phys.position.y.0 < high[1] - 5.0, "the control falls: {:?}", f.cam.walk.phys.position.y);
    }

    /// Turned on and off with no time flown, the walk goes on bit for bit as one
    /// that never flew (with fly off nothing changes); on in a fall, the fall
    /// stops within a second. The control: a fall with no flight goes on.
    #[test]
    fn a_flight_of_no_length_changes_nothing_and_one_begun_in_a_fall_stops_it() {
        let go = Controls { fwd: 1.0, orbit: -0.25, ..Default::default() };
        let plain = crate::controls::Client::default();
        let (Some(mut a), Some(mut b)) = (open_ground(), open_ground()) else { return };
        let t0 = a.scene.time;
        let console = |n: i64| t0 + (n as f64 * CONSOLE_FRAME_MS) as i64;
        b.frame_with(Controls::default().axes(), t0, &flying(), true);
        b.frame_with(Controls::default().axes(), t0, &plain, true);
        a.frame_with(Controls::default().axes(), t0, &plain, true);
        for n in 1..=90 {
            a.frame_with(go.axes(), console(n), &plain, true);
            b.frame_with(go.axes(), console(n), &plain, true);
            assert!(same(&a.cam.walk.phys, &b.cam.walk.phys), "console frame {n}: {:?}, {:?}", a.cam.walk.phys.position, b.cam.walk.phys.position);
        }

        let (x, z) = (22000.0f32, 18000.0f32);
        let high = Spawn { pos: [x, 400.0, z], heading: 1.5, placing: Placing::AsGiven };
        let Some(mut f) = tunaria_round(x, z).map(|s| Walker::arrive(s, high)) else { return };
        hold(&mut f, Controls::default(), &plain, 30.0, 2.0);
        let v = f.cam.walk.phys.velocity.y.0;
        assert!(v < -15.0, "falling: {v}");
        hold(&mut f, Controls::default(), &flying(), 30.0, 1.0);
        assert!(f.cam.walk.phys.velocity.y.0.abs() < 1.0, "the fall stopped: {:?}", f.cam.walk.phys.velocity.y);
        let y = f.cam.walk.phys.position.y.0;
        hold(&mut f, Controls::default(), &flying(), 30.0, 2.0);
        assert!((f.cam.walk.phys.position.y.0 - y).abs() < 0.2, "and holds: {y} to {:?}", f.cam.walk.phys.position.y);
    }

    /// Flown down onto the ground a body runs on with no landing's gait (the
    /// game's locomotion 7, told from the walk's own flags): it did not fall. Off
    /// in the air, it falls and is told the landing, once. The body is drawn
    /// upright under real water, its pitch left out of its centre (`UpdateSprite`
    /// pitches a swimmer). The control: the same place and pitch with no flight is
    /// pitched.
    #[test]
    fn a_flyer_lands_with_no_landing_and_is_drawn_upright_under_water() {
        let (x, z) = (22000.0f32, 18000.0f32);
        let Some(mut w) = tunaria_round(x, z).map(|s| Walker::arrive(s, Spawn { pos: [x, 60.0, z], heading: 1.5, placing: Placing::AsGiven })) else { return };
        w.cam.set_camera_mode(0);
        hold(&mut w, Controls::default(), &flying(), 30.0, 0.2);
        crate::mouse::pitch_body(&mut w.cam, F::new(1.0));
        let t0 = w.scene.time;
        let go = Controls { fwd: 1.0, ..Default::default() };
        for k in 1..=(60 * 4) {
            w.frame_with(go.axes(), t0 + (k as f64 * 1000.0 / 60.0) as i64, &flying(), true);
            assert_eq!(w.cam.walk.parm118, 0, "frame {k}: a landing told in flight at {:?}", w.cam.walk.phys.position);
        }
        assert!((w.cam.walk.phys.position.y.0 - 54.1).abs() < 0.2, "on the ground: {:?}", w.cam.walk.phys.position.y);

        let plain = crate::controls::Client::default();
        let mut told = 0;
        let mut f = Walker::arrive(tunaria_round(x, z).unwrap(), Spawn { pos: [x, 60.0, z], heading: 1.5, placing: Placing::AsGiven });
        hold(&mut f, Controls::default(), &flying(), 30.0, 0.5);
        let t0 = f.scene.time;
        for k in 1..=(60 * 4) {
            f.frame_with(Controls::default().axes(), t0 + (k as f64 * 1000.0 / 60.0) as i64, &plain, true);
            told += (f.cam.walk.parm118 == 16) as usize;
        }
        assert!(told >= 1, "the control: off in the air, no landing told");

        // under the bay, pitched
        let (x, z) = (25390.0f32, 15860.0f32);
        let Some((lib, file)) = tunaria_and_chars(x, z) else { return };
        let human = lib.lookup(0x70D898C6).unwrap();
        let at = Spawn { pos: [x, 40.0, z], heading: 0.0, placing: Placing::AsGiven };
        let rot = |client: &crate::controls::Client| {
            let mut w = Walker::arrive(World::load(&lib, file).unwrap(), at);
            w.set_character(&lib, human);
            w.cam.set_camera_mode(0);
            hold(&mut w, Controls::default(), client, 30.0, 0.1);
            let t0 = w.scene.time;
            for k in 1..=30 {
                crate::mouse::pitch_body(&mut w.cam, F::new(0.05));
                w.frame_with(Controls::default().axes(), t0 + k * 33, client, true);
                w.pose();
            }
            let cs = w.sprite.as_ref().unwrap();
            assert!(w.cam.walk.is_under_liquid(), "under the bay: {:#x}", w.cam.walk.phys.flags);
            let q = cs.h.transform[1].rot;
            [q.x.0, q.y.0, q.z.0, q.w.0]
        };
        let upright = vi::quat::Quat::<Ee>::from_hpr(&Vect3::default());
        assert_eq!(rot(&flying()), [upright.x.0, upright.y.0, upright.z.0, upright.w.0], "drawn upright in flight");
        assert_ne!(rot(&plain), [upright.x.0, upright.y.0, upright.z.0, upright.w.0], "the control: a swimmer is pitched");
    }

    /// The right stick's look in the follow orbit, at the body's pitching rate
    /// at any frame rate, up, then back: the camera's tilt, as the mouse's, and it
    /// goes back to what it was when the flight ends. The control: not flying, it
    /// zooms and the tilt is the same.
    #[test]
    fn the_right_stick_tilts_the_orbit_in_flight_and_the_tilt_goes_back() {
        let (x, z) = (22000.0f32, 18000.0f32);
        for hz in [30.0, 165.0] {
            let Some(mut w) = tunaria_round(x, z).map(|s| Walker::arrive(s, Spawn::ground(x, z, 1.5))) else { return };
            let was = w.cam.def_orbit_pitch.0;
            hold(&mut w, Controls { pitch: -1.0, ..Default::default() }, &flying(), hz, 1.0);
            let tilted = w.cam.def_orbit_pitch.0;
            eprintln!("{hz} Hz: from {was} to {tilted} in a second of the stick");
            assert!((was - tilted - std::f32::consts::FRAC_PI_4).abs() < 0.03, "{hz} Hz: tilted {was} to {tilted}");
            hold(&mut w, Controls::default(), &crate::controls::Client::default(), hz, 0.1);
            assert_eq!(w.cam.def_orbit_pitch.0, was, "{hz} Hz: back as the flight ends");

            let dist = w.cam.cur.dist.0;
            hold(&mut w, Controls { pitch: -1.0, ..Default::default() }, &crate::controls::Client::default(), hz, 0.5);
            assert_eq!(w.cam.def_orbit_pitch.0, was, "{hz} Hz: the control keeps the tilt");
            assert!((w.cam.cur.dist.0 - dist).abs() > 0.1, "{hz} Hz: the control zooms: {dist} to {:?}", w.cam.cur.dist);
        }
    }

    // A jump (the module's doc), on Tunaria's flat at (22000, 18000), the ground
    // at 54.1, in camera mode 2 (`open_ground`).

    /// A client that may jump: the server offered it, a report standing sent.
    fn jumper() -> crate::controls::Client {
        crate::controls::Client { jumps: true, stood_sent: true, ..Default::default() }
    }

    /// `secs` of frames drawn at `hz` at the sticks `go` for `client`, a jump
    /// asked on the frame drawn at or after `ask_ms` from the walk's clock on
    /// (none for -1): each frame's walk as drawn, and the console frames kept
    /// (their number and the walk as each left it).
    fn jumping(w: &mut Walker, go: Controls, client: &crate::controls::Client, hz: f64, secs: f64, ask_ms: i64) -> (Vec<WalkPhysics<Ee>>, Vec<(i64, WalkPhysics<Ee>)>) {
        let t0 = w.scene.time;
        let (mut drawn, mut kept) = (Vec::new(), Vec::new());
        let mut asked = ask_ms < 0;
        for k in 1..=(hz * secs) as u64 {
            let now = drawn_at(t0, hz, 0.0, k);
            if !asked && now - t0 >= ask_ms {
                w.ask_jump();
                asked = true;
            }
            w.frame_with(go.axes(), now, client, true);
            drawn.push(w.cam.walk.phys);
            let k = w.kept.unwrap();
            if kept.last().is_none_or(|l: &(i64, WalkPhysics<Ee>)| l.0 != k.frames) {
                kept.push((k.frames, k.at.0));
            }
        }
        (drawn, kept)
    }

    /// The highest the feet went over `floor`, and how long the walk was off a
    /// floor (ms, between the first frame drawn in the air and the first on a
    /// floor again).
    fn arc(drawn: &[WalkPhysics<Ee>], floor: f32) -> (f32, i64) {
        let top = drawn.iter().map(|p| p.position.y.0 - floor).fold(f32::MIN, f32::max);
        let off = drawn.iter().position(|p| p.flags & ON_FLOOR == 0);
        let on = off.and_then(|o| drawn[o..].iter().position(|p| p.flags & ON_FLOOR != 0).map(|n| o + n));
        let air = match (off, on) {
            (Some(a), Some(b)) => drawn[b].time - drawn[a].time,
            _ => 0,
        };
        (top, air)
    }

    /// A jump in place rises to `JUMP_TOP` (0.896 with the walk's integrator) and
    /// comes down onto the floor it left through `Move`'s own floor test, at the
    /// console's 30 frames a second and drawn at 60 and 165: in the air for 0.6
    /// s, the air's gait (6, the fall's clip) all the way, the landing told once
    /// (`parm118` 16, the landing's clip) and standing (0) after, at the very
    /// height it left. The controls: the same start speed given with the floor
    /// left alone, `Move`'s step-down holds the body on the ground (it never
    /// leaves it, the reason for the lift); and with no offer the key does
    /// nothing.
    #[test]
    fn a_jump_rises_to_its_top_and_lands_on_the_floor_it_left() {
        for hz in [1000.0 / CONSOLE_FRAME_MS, 60.0, 165.0] {
            let Some(mut w) = open_ground() else { return };
            hold(&mut w, Controls::default(), &jumper(), hz, 0.5);
            let (floor, bits) = (w.cam.walk.phys.position.y.0, w.cam.walk.phys.position.y.bits());
            let t0 = w.scene.time;
            let mut told = 0;
            let mut drawn = Vec::new();
            for k in 1..=(hz * 1.5) as u64 {
                if k == 1 {
                    w.ask_jump();
                }
                w.frame_with(Controls::default().axes(), drawn_at(t0, hz, 0.0, k), &jumper(), true);
                told += (w.cam.walk.parm118 == 16) as u32;
                drawn.push(w.cam.walk.phys);
            }
            let (top, air) = arc(&drawn, floor);
            eprintln!("{hz:.2} Hz: top {top:.4}, {air} ms in the air");
            assert!((top - 0.896).abs() < 0.01 && top <= JUMP_TOP + 0.005, "{hz} Hz: the top {top}");
            assert!((560..=680).contains(&air), "{hz} Hz: {air} ms in the air");
            for p in drawn.iter().filter(|p| p.flags & ON_FLOOR == 0) {
                assert_eq!(p.loco_state, 6, "{hz} Hz: in the air the air's gait");
            }
            assert_eq!(told, 1, "{hz} Hz: the landing told once");
            let last = drawn.last().unwrap();
            assert!(last.flags & ON_FLOOR != 0 && last.loco_state == 0, "{hz} Hz: standing after");
            assert_eq!(last.position.y.bits(), bits, "{hz} Hz: at the height it left ({} for {floor})", last.position.y.0);
            assert_eq!(w.jump_asked, None, "{hz} Hz: the jump taken");
        }

        // the controls: the start speed with the floor left alone; no offer
        let Some(mut w) = open_ground() else { return };
        hold(&mut w, Controls::default(), &jumper(), 60.0, 0.5);
        let floor = w.cam.walk.phys.position.y.0;
        w.rewind();
        w.cam.walk.phys.velocity.y = F::new(JUMP_SPEED);
        let (drawn, _) = jumping(&mut w, Controls::default(), &jumper(), 60.0, 1.0, -1);
        let (top, air) = arc(&drawn, floor);
        assert!(air == 0 && top < 0.05, "the control: the speed alone, off the floor for {air} ms, {top} up");
        let Some(mut w) = open_ground() else { return };
        hold(&mut w, Controls::default(), &crate::controls::Client::default(), 60.0, 0.5);
        let (drawn, _) = jumping(&mut w, Controls::default(), &crate::controls::Client { stood_sent: true, ..Default::default() }, 60.0, 1.0, 0);
        assert_eq!(arc(&drawn, floor), (0.0, 0), "the control: no offer, no jump");
    }

    /// A jump keeps the console's frames at any display rate: drawn at 60, 165
    /// and 240 frames a second, a jump that takes off in the same console frame
    /// is that frame on bit for bit (its height, its speed up or down, its
    /// flags, the frame it lands in), standing and at a run. The frame it takes
    /// off in is the next one stepped after the press, as for any control given
    /// between frames, so a press can land a frame apart at two rates (one in a
    /// frame of 32 of VITimer's counts, the other in one of 34, whose arcs differ
    /// in their last bits, as a PS2's would): never more than one. The control:
    /// the same jump stepped once a frame drawn at 165 (the walk's stepping until
    /// 2026-10-27) goes higher, as its first step at the ground's damping is
    /// shorter.
    #[test]
    fn a_jump_keeps_the_consoles_frames_at_any_display_rate() {
        type Arc = Vec<[u32; 3]>;
        for go in [Controls::default(), Controls { fwd: 1.0, ..Default::default() }] {
            let frames = |hz: f64, ask: i64| -> Option<(i64, Arc)> {
                let mut w = open_ground()?;
                hold(&mut w, go, &jumper(), 165.0, 1.0);
                let (_, kept) = jumping(&mut w, go, &jumper(), hz, 1.5, ask);
                let up = kept.iter().position(|(_, p)| p.velocity.y.0 > 0.0)?;
                Some((kept[up].0, kept[up..].iter().map(|(_, p)| [p.position.y.bits(), p.velocity.y.bits(), p.flags]).collect()))
            };
            let mut all_one = 0;
            for ask in (500..=600).step_by(10) {
                let Some((n60, at60)) = frames(60.0, ask) else { return };
                assert!(at60.len() > 25 && at60.last().unwrap()[2] & ON_FLOOR != 0, "landed within the frames");
                let mut same = true;
                for hz in [165.0, 240.0] {
                    let (n, at) = frames(hz, ask).unwrap();
                    assert!((n - n60).abs() <= 1, "asked at {ask}, {hz} Hz took off in frame {n}, 60 Hz in {n60}");
                    if n == n60 {
                        let k = at60.len().min(at.len());
                        assert_eq!(at60[..k], at[..k], "asked at {ask}, {hz} Hz: the jump's console frames");
                    }
                    same &= n == n60;
                }
                all_one += same as u32;
            }
            assert!(all_one >= 3, "a press in the same frame at every rate, {all_one} times in 11");
        }

        // the control: stepped once a frame drawn at 165
        let Some(mut w) = open_ground() else { return };
        hold(&mut w, Controls::default(), &jumper(), 165.0, 0.5);
        let floor = w.cam.walk.phys.position.y.0;
        w.rewind();
        w.take_off();
        let t0 = w.scene.time;
        let mut top = 0.0f32;
        for k in 1..=165u64 {
            frame_drawn(&mut w, Controls::default(), drawn_at(t0, 165.0, 0.0, k));
            top = top.max(w.cam.walk.phys.position.y.0 - floor);
        }
        assert!(top > JUMP_TOP + 0.05, "the control: stepped a frame drawn at 165, the top {top}");
    }

    /// A running jump carries the run across: its take-off goes at the ground's
    /// damping, so the frame it leaves covers the ground as the run's frames do
    /// (each by VITimer's count of it, 32 or 34), and in the air nothing pushes it
    /// (Walk and Run need a floor), so it slows as the air's damping slows it
    /// alone, and lands about 5 ahead.
    #[test]
    fn a_running_jump_keeps_the_runs_speed_across() {
        let go = Controls { fwd: 1.0, ..Default::default() };
        let Some(mut w) = open_ground() else { return };
        hold(&mut w, go, &jumper(), 60.0, 1.5);
        let (_, kept) = jumping(&mut w, go, &jumper(), 60.0, 1.5, 300);
        let across = |i: usize| {
            let (a, b) = (&kept[i - 1].1, &kept[i].1);
            assert_eq!(kept[i].0, kept[i - 1].0 + 1, "kept frames one after another");
            ((b.position.x.0 - a.position.x.0).powi(2) + (b.position.z.0 - a.position.z.0).powi(2)).sqrt() / console_step(kept[i].0) as f32
        };
        let off = kept.iter().position(|(_, p)| p.flags & ON_FLOOR == 0).expect("it leaves the ground");
        let on = off + kept[off..].iter().position(|(_, p)| p.flags & ON_FLOOR != 0).expect("it lands");
        assert!(off >= 3, "run frames before the take-off");
        let (run, leave) = ((across(off - 1) + across(off - 2)) / 2.0, across(off));
        let far = ((kept[on].1.position.x.0 - kept[off - 1].1.position.x.0).powi(2) + (kept[on].1.position.z.0 - kept[off - 1].1.position.z.0).powi(2)).sqrt();
        eprintln!("a run's frame {run:.6} a count, the take-off's {leave:.6}, {far:.2} across in the air");
        assert!((leave / run - 1.0).abs() < 0.03, "the take-off's {leave} against the run's {run}");
        let mut was = leave;
        for i in off + 1..on {
            let d = across(i);
            assert!(d <= was * 1.03 && d > was * 0.96, "frame {i} in the air: {d} after {was}");
            was = d;
        }
        assert!((4.5..5.8).contains(&far), "landed {far} ahead");
    }

    /// No jump where the client or the walk says no: not offered, held in place
    /// (0x00FC, or no speed: a GM's Freeze, a root), casting, a window's lock,
    /// no report standing sent since the last move, flying, already in the air
    /// (a second ask on the way up adds nothing: the top is one jump's), and in
    /// the water. The control: the same frames from a client that may jump take
    /// off.
    #[test]
    fn no_jump_where_the_client_or_the_walk_says_no() {
        let off_ground = |client: &crate::controls::Client| {
            let mut w = open_ground()?;
            hold(&mut w, Controls::default(), client, 60.0, 0.3);
            let floor = w.cam.walk.phys.position.y.0;
            let (drawn, _) = jumping(&mut w, Controls::default(), client, 60.0, 0.6, 0);
            Some(arc(&drawn, floor).0)
        };
        let Some(control) = off_ground(&jumper()) else { return };
        assert!(control > 0.85, "the control: {control}");
        let j = jumper();
        for (why, c) in [
            ("not offered", crate::controls::Client { jumps: false, ..j }),
            ("held (0x00FC)", crate::controls::Client { rooted: true, ..j }),
            ("no speed", crate::controls::Client { run_speed: 0.0, ..j }),
            ("casting", crate::controls::Client { casting: true, ..j }),
            ("a window's lock", crate::controls::Client { move_ok: false, ..j }),
            ("the manual camera", crate::controls::Client { modal: true, ..j }),
            ("no report standing", crate::controls::Client { stood_sent: false, ..j }),
            ("timed out", crate::controls::Client { timed_out: true, ..j }),
        ] {
            let top = off_ground(&c).unwrap();
            assert!(top < 0.01, "{why}: up {top}");
        }

        // flying: the ask does nothing, and is let go
        let Some(mut w) = open_ground() else { return };
        let fly = crate::controls::Client { flying: true, ..j };
        hold(&mut w, Controls::default(), &fly, 60.0, 0.3);
        let y = w.cam.walk.phys.position.y.0;
        jumping(&mut w, Controls::default(), &fly, 60.0, 0.6, 0);
        assert!((w.cam.walk.phys.position.y.0 - y).abs() < 0.01 && w.jump_asked.is_none(), "flying");

        // in the air already: asked again on the way up, the top is one jump's
        let Some(mut w) = open_ground() else { return };
        hold(&mut w, Controls::default(), &j, 60.0, 0.3);
        let floor = w.cam.walk.phys.position.y.0;
        let t0 = w.scene.time;
        let mut drawn = Vec::new();
        for k in 1..=60u64 {
            if k == 1 || k == 8 || k == 14 {
                w.ask_jump();
            }
            w.frame_with(Controls::default().axes(), drawn_at(t0, 60.0, 0.0, k), &j, true);
            drawn.push(w.cam.walk.phys);
        }
        let (top, _) = arc(&drawn, floor);
        assert!((top - control).abs() < 0.001, "asked again in the air: {top} against one jump's {control}");

        // in the bay: swimming, no jump
        let (x, z) = (25390.0f32, 15860.0f32);
        let Some(world) = tunaria_round(x, z) else { return };
        let mut w = Walker::arrive(world, Spawn { pos: [x, 51.32 - 2.5, z], heading: 0.0, placing: Placing::AsGiven });
        w.cam.set_camera_mode(2);
        hold(&mut w, Controls::default(), &j, 60.0, 1.0);
        assert!(w.cam.walk.phys.flags & flags::LIQUID != 0, "in the water: {:#x}", w.cam.walk.phys.flags);
        let y = w.cam.walk.phys.position.y.0;
        jumping(&mut w, Controls::default(), &j, 60.0, 0.5, 0);
        assert!(w.cam.walk.phys.velocity.y.0 < 1.0 && w.cam.walk.phys.position.y.0 < y + 0.3, "swimming: up to {:?}", w.cam.walk.phys.position.y);
    }

    /// The key pressed as the body comes down (or before the report of its
    /// landing has gone out) jumps once it may, within `JUMP_WAIT_MS`: asked
    /// 150 ms before the landing, it takes off again as soon as it stands; asked
    /// 400 ms before, it is let go (the control) and the body stays down.
    #[test]
    fn a_jump_asked_as_the_body_comes_down_is_taken_once_it_stands() {
        let run = |second: Option<i64>| {
            let mut w = open_ground()?;
            hold(&mut w, Controls::default(), &jumper(), 60.0, 0.3);
            let t0 = w.scene.time;
            let (mut drawn, mut asked) = (Vec::new(), false);
            for k in 1..=120u64 {
                let now = drawn_at(t0, 60.0, 0.0, k);
                if k == 1 {
                    w.ask_jump();
                }
                if let Some(at) = second
                    && !asked
                    && now - t0 >= at
                {
                    w.ask_jump();
                    asked = true;
                }
                w.frame_with(Controls::default().axes(), now, &jumper(), true);
                drawn.push((now - t0, w.cam.walk.phys));
            }
            Some(drawn)
        };
        let leaves = |d: &[(i64, WalkPhysics<Ee>)]| d.windows(2).filter(|p| p[0].1.flags & ON_FLOOR != 0 && p[1].1.flags & ON_FLOOR == 0).count();
        let Some(once) = run(None) else { return };
        assert_eq!(leaves(&once), 1);
        let landed = once.windows(2).find(|p| p[0].1.flags & ON_FLOOR == 0 && p[1].1.flags & ON_FLOOR != 0).unwrap()[1].0;
        for (before, again) in [(150, true), (400, false)] {
            let d = run(Some(landed - before)).unwrap();
            assert_eq!(leaves(&d) == 2, again, "asked {before} ms before the landing at {landed}: left the ground {} times", leaves(&d));
        }
    }

    /// With nobody jumping the walk is as it was: a run with a turn, drawn at
    /// 165, from a client that may jump keeps every console frame bit for bit as
    /// one that may not (the game's walk, untouched). The control: a jump asked
    /// in the middle changes them.
    #[test]
    fn with_nobody_jumping_the_walk_is_as_it_was() {
        let go = Controls { fwd: 1.0, orbit: -0.25, ..Default::default() };
        let frames = |client: &crate::controls::Client, ask: i64| {
            let mut w = open_ground()?;
            let (drawn, kept) = jumping(&mut w, go, client, 165.0, 3.0, ask);
            let bits = |p: &WalkPhysics<Ee>| (p.time, [p.position.x.bits(), p.position.y.bits(), p.position.z.bits(), p.angles.x.bits(), p.velocity.x.bits(), p.velocity.y.bits(), p.floor.bits(), p.flags, p.loco_state as u32]);
            Some((drawn.iter().map(bits).collect::<Vec<_>>(), kept.iter().map(|(n, p)| (*n, bits(p))).collect::<Vec<_>>()))
        };
        let Some(plain) = frames(&crate::controls::Client::default(), -1) else { return };
        assert_eq!(frames(&jumper(), -1).unwrap(), plain, "a client that may jump, not jumping");
        assert_ne!(frames(&jumper(), 1500).unwrap().1, plain.1, "the control: a jump asked");
    }
}
