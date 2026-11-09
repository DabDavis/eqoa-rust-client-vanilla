//! The follow orbit's reach, the client's own: the distance the player chose,
//! and the way back out to it after something pulled the camera in.
//!
//! The game's follow orbit (mode 3, `VIWalkCamera::MoveCamera`) sweeps from
//! the head out to the camera at its default pitch every tick; blocked, it
//! zooms in (23.5 a second) to `m_minCollisionDist`, 3.5, then tries the
//! alternate pitch and the other angles, and at last comes to its closest
//! (1.5). It keeps that distance: nothing in `UpdateCameraPhysics` or
//! `MoveCamera` grows it again, only the stick's zoom (`OrbitIn`, the
//! camera's distance spring) and `SetOrbitDistance` do (decomp
//! viwalkcamera.cpp, `MoveCamera` at 0x00482AF0 and the step before it). So
//! a camera a wall had pulled to 3.5 stayed there once the wall was behind
//! it (F3 20261021-002052: "distance 3.50 of 11.00").
//!
//! Here the distance the player chose is kept (`Reach::chosen`): set by the
//! wheel (`mouse::zoom`: out from the choice, so a notch out while a wall
//! holds the camera in waits for the room), by the stick's zoom (the spring
//! moving while no pull is under way) and by whatever sets the distance
//! between two frames (a style, a touch screen's pinch); back from the start
//! menu's framing
//! (mode 1, `ApplyCameraPreset`) the choice it had stands.
//!
//! Something in the way brings the camera in only as far as it must (F3
//! 20261025-060553, the user's: "only zoom what is needed then zoom back out
//! to where it was"): the game's pull goes on at its own 23.5 a second, and
//! is held (`hold`) where the sphere the game sweeps from the head first
//! meets what is in the way, `SHORT` this side of it, when that is past the
//! game's closest collision distance (3.5); nearer, the pull to 3.5, the
//! alternate pitch, the hunt for another angle and the way in to the closest
//! are the game's, as before. With the camera at rest (no snap, no pull, no
//! zoom under way) it eases back out toward the distance chosen as far as
//! that sweep is clear, `SHORT` short of anything still in the way (`ease`):
//! its speed grows from nothing, 30 a second, to at most 12 a second, and
//! slows over the last 2: 3.5 to 11 in about 1.3 seconds. So a wall behind a
//! player standing still holds the camera just in front of it, not at 3.5,
//! and a camera a tree or a slope pulled in follows it back out as it goes;
//! with nothing in the way the camera is the game's, bit for bit.
//!
//! The mouse's tilt (`mouse::tilt`) can lower the orbit's default pitch under
//! the game's 15 degrees (`GAME_PITCH`, `SetCameraStyle(3)`'s), even level
//! with the head. Blocked at its closest collision distance and with no
//! steep slope underfoot for the alternate pitch, the game's camera swings
//! round to the first clear of eight angles (`FindOrbitAngle`), and since the
//! follow orbit moves the player the way the camera looks (`UpdateControls`:
//! `ViewRelativeMove` along `m_camera.m_dir`), a run turns with it, up to a
//! half turn. At the game's pitch a slope behind, however steep, only brings
//! the camera in (to 3.5; past 26 degrees the alternate pitch takes it up), so
//! on a PS2 it takes a wall or broken ground; tilted level with the head, a
//! slope of 18 to 25 degrees behind a run downhill swung it 45 to 90 degrees
//! (F3 20261024-100428), and tilted lower, gentler ones did (from 8 degrees,
//! the camera 11 degrees under the head). Under the game's pitch, and while
//! the right button steers (the camera kept behind the body), the swing is
//! not begun: it ends where it is (`mouse::end_swing`) and the camera comes
//! in instead, only as far as what is in the way needs (`come_in`): `SHORT`
//! this side of where the game's sweep meets it, at least the game's
//! closest (1.5), and the way back out above brings it out again once that
//! is behind. It had come in to its closest every time (F3 2026-11-09, the
//! user, steering round in Freeport's well: "it hits the poles and zooms all
//! the way in to char, but shouldnt zoom at all"): from the well's middle
//! the posts and the roof's eaves meet the sweep 3.3 to 4.7 out, nearer
//! than the closest collision distance only toward the eaves' middles,
//! where the game itself swings round 45 degrees to a clear angle without
//! coming in at all; ours now comes in from the game's own 3.38 to about
//! 3.25 there.
//! The game's own way in to its closest, when no angle of the eight is clear
//! (flag 2), stays the game's. A frame longer than the camera's 32 ms tick
//! can let a swing move before it is stopped: under a degree at 30 frames a
//! second.
//!
//! Something thin in the way moves the camera not at all (`PastThin`, the
//! client's own; F3 2026-11-09, the user, zoomed out in Freeport's well: "we
//! rotate cam the posts collision make the cam zoom"): every sweep the
//! camera makes, the game's and ours, goes on past a post, a pole or a beam
//! narrower than its sphere is across, as modern third-person cameras let
//! poles stand in front of them. Thin is read off the geometry where the
//! sweep met it: nothing within `THIN_SIDE` to either side of the contact,
//! from just before it to just past where the sweep goes on, and a far side
//! within `THIN_DEEP` behind it. A wall, a roof's underside, the well's
//! stone ring, a broad trunk: the game's as before. The well's four posts
//! are 0.28 across; turning a camera zoomed out round its middle, each had
//! pulled it in four times a turn, toward 3.5-4.1 where the turn rested on
//! one (tilted under 10 degrees they are all that is in the way there; at the
//! game's 15 its roof's underside is too, past them, and still stops it).

use vi::camera::{Eye, WalkCamera};
use vi::coll::{CollSphere, EnvRay};
use vi::fpu::{Ee, F, clamp, vimax, vimin};
use vi::scene::{Filter, FilterKind, Scene};
use vi::vect::Vect3;

/// The way back out: its speed grows `ACCEL` a second from rest, to at most
/// `SPEED`, and is at most `RATE` times the distance left (m/s); within
/// `THERE` of where it goes it is there.
const ACCEL: f32 = 30.0;
const SPEED: f32 = 12.0;
const RATE: f32 = 6.0;
const THERE: f32 = 0.05;

/// How far this side of what is in the way the camera is held, and eased
/// out to: the sweep's sphere (the camera's 0.5) already keeps the eye that
/// far off it; this keeps the game's next sweep from touching it again.
const SHORT: f32 = 0.1;

/// The distance spring moving faster than this (a second): a zoom under way.
const MOVING: f32 = 0.001;

/// The follow orbit's default pitch as the game sets it
/// (`SetCameraStyle(3)`: `SetDefOrbitPitchAngle(0.2617994)`, 15 degrees).
pub const GAME_PITCH: f32 = 0.261_799_4;

/// What is thin to the camera (`thin`): the side probes run this far to
/// either side of where the sweep met it (half the sphere across: a post
/// 0.28 wide is clear of both wherever it was met), along lines this far
/// into what was met, from this far before it; what was met has its far
/// side no deeper than `THIN_DEEP` along the sweep. The probes are this
/// thin, and a sweep goes past at most `THIN_LEGS` thin things.
const THIN_SIDE: f32 = 0.5;
const THIN_INTO: f32 = 0.05;
const THIN_BEFORE: f32 = 0.6;
const THIN_DEEP: f32 = 0.6;
const THIN_PROBE: f32 = 0.01;
const THIN_LEGS: usize = 4;

#[derive(Clone, Copy, Debug, Default)]
pub struct Reach {
    /// the follow orbit's distance the player chose; none before the orbit
    /// is first seen
    pub chosen: Option<F<Ee>>,
    /// the camera's mode and distance as the last frame left them
    end: Option<(i32, F<Ee>)>,
    /// the way back out's speed now, a second
    speed: f32,
    /// the mouse steers this frame (`controls::Client::steer`, set by
    /// `Walker::frame_with`): the camera is kept behind the body, so no
    /// swing round is begun (`come_in`)
    pub steered: bool,
}

impl Reach {
    /// The distance chosen, within the orbit's range (before one is, the
    /// camera's own).
    pub fn chosen(&self, cam: &WalkCamera<Ee>) -> F<Ee> {
        clamp(self.chosen.unwrap_or(cam.cur.dist), cam.min_orbit_distance, cam.max_orbit_distance)
    }

    /// The camera's frame (`Process`), `dt_s` seconds after the last: the
    /// choice kept, the way back out before it, and after it a swing round
    /// ended (steered, or tilted under the game's pitch) with the camera come
    /// in only as far as it must (`come_in`), or else the game's pull held
    /// short of what is in the way.
    pub fn process<S: Scene<Ee>>(&mut self, cam: &mut WalkCamera<Ee>, scene: &mut S, dt_s: f32) -> Eye<Ee> {
        let mode = cam.camera_mode;
        if mode == 3 {
            match self.end {
                // set since the last frame (the wheel, a pinch, a preset)
                Some((3, d)) if d != cam.cur.dist => self.chosen = Some(cam.cur.dist),
                // into the orbit by a style (P, the wheel out of first
                // person): its distance; back from the start menu's framing
                // (mode 1) the choice it had
                Some((m, _)) if m != 3 && m != 1 => self.chosen = Some(cam.cur.dist),
                None => self.chosen = Some(cam.cur.dist),
                _ => {}
            }
            self.ease(cam, scene, dt_s);
        } else {
            self.speed = 0.0;
        }
        let (d0, f0, spring) = (cam.cur.dist, cam.cur.flags, zooming(cam));
        // the shake's sample as `Process` found it, for the eye made again
        let sample = cam.effect.sample;
        let mut eye = cam.process(scene);
        // steered, or tilted under the game's pitch, no swing round: the
        // camera comes in instead, as far as it must (the module's doc)
        let came_in = cam.camera_mode == 3 && (self.steered || cam.def_orbit_pitch < F::new(GAME_PITCH)) && come_in(cam, scene);
        if came_in {
            cam.effect.sample = sample;
            eye = cam.set_camera();
        }
        let pull = (f0 | cam.cur.flags) & 6 != 0;
        // the stick's zoom (the spring) is a choice; a pull (flags 2, 4) is
        // not, nor is coming in for what is in the way
        if mode == 3 && cam.camera_mode == 3 && spring && !pull && !came_in && cam.cur.dist != d0 {
            self.chosen = Some(cam.cur.dist);
        }
        // the game's pull (or one begun and ended inside this frame's ticks:
        // the distance down with no zoom of the stick's) held short of what
        // is in the way, when that is past its closest collision distance
        if mode == 3 && cam.camera_mode == 3 && !came_in && (pull || (!spring && cam.cur.dist < d0))
            && let Some(d) = hold(cam, scene, d0, self.chosen(cam))
        {
            place(cam, d);
            cam.effect.sample = sample;
            eye = cam.set_camera();
        }
        self.end = Some((cam.camera_mode, cam.cur.dist));
        eye
    }

    /// A step back out toward the distance chosen, as far as the game's
    /// sweep is clear (`SHORT` short of what is in the way), when the
    /// camera is at rest and that is past its closest collision distance.
    fn ease<S: Scene<Ee>>(&mut self, cam: &mut WalkCamera<Ee>, scene: &mut S, dt_s: f32) {
        let want = self.chosen(cam);
        let resting = cam.cur.flags & (1 | 2 | 4 | 8) == 0 && !zooming(cam);
        let to = if resting && cam.cur.dist < want {
            match reach(cam, scene, want) {
                None => Some(want),
                Some(r) => Some(r - F::new(SHORT)).filter(|&to| cam.min_collision_dist < to),
            }
        } else {
            None
        };
        let Some(to) = to.filter(|&to| cam.cur.dist < to) else {
            self.speed = 0.0;
            return;
        };
        let left = (to - cam.cur.dist).0;
        self.speed = (self.speed + ACCEL * dt_s).min(SPEED).min(left * RATE);
        let step = self.speed * dt_s;
        let d = if step >= left - THERE { to } else { cam.cur.dist + F::new(step) };
        cam.cur.dist = d;
        cam.last.dist = d;
    }
}

/// The distance spring moving or pushed: the stick's zoom under way.
fn zooming(cam: &WalkCamera<Ee>) -> bool {
    cam.cur.dist_force != F::ZERO || F::new(MOVING) < cam.cur.dist_vel.abs()
}

/// Where the game's own sweep (`MoveCamera`'s first: the camera's sphere
/// from the head out along the orbit at the default pitch) toward `dist`
/// first meets something: how far out along the orbit, or `None` when it
/// reaches `dist` clear.
pub fn reach<S: Scene<Ee>>(cam: &WalkCamera<Ee>, scene: &mut S, dist: F<Ee>) -> Option<F<Ee>> {
    let p = &cam.walk.phys;
    let mut from = p.position;
    from.y += cam.target_height;
    let mut dir = Vect3::<Ee>::default();
    dir.hpr(cam.cur.orbit.x, -cam.def_orbit_pitch, F::ZERO);
    let mut to = p.position;
    to.y += cam.target_height;
    to.add_scaled(&dir, dist);
    from.y += p.height_offset;
    to.y += p.height_offset;
    let mut s = CollSphere::<Ee>::default();
    s.init(from, to, cam.collision_radius);
    let mut f = Filter::new(FilterKind::WalkCamera, cam.walk.parm64);
    if !scene.collide_sphere(&mut s, &mut f) {
        return None;
    }
    Some(dist * s.ray.hit_time)
}

/// Where a pull this frame stops instead of the game's: `SHORT` this side
/// of what is in the way out toward the distance chosen (`want`), at most
/// `from` (where the frame began: out again is the way back out's), but
/// only when that is past the closest collision distance and out from where
/// the pull has brought the camera; `None` leaves the game's. Swept to the
/// choice, not to `from`, so the camera stops at the same place whatever the
/// frames' lengths: a pull's last frame can begin clear of the wall.
fn hold<S: Scene<Ee>>(cam: &WalkCamera<Ee>, scene: &mut S, from: F<Ee>, want: F<Ee>) -> Option<F<Ee>> {
    let to = match reach(cam, scene, vimax(from, want)) {
        None => from,
        Some(r) => vimin(from, r - F::new(SHORT)),
    };
    let to = vimin(to, cam.max_orbit_distance);
    (cam.min_collision_dist < to && cam.cur.dist < to).then_some(to)
}

/// A swing round the game began (`MoveCamera`'s `FindOrbitAngle` found a
/// clear angle: flag 1) ended where it is (`mouse::end_swing`), and the
/// camera come in instead, only as far as what is in the way needs: `SHORT`
/// this side of where the game's own sweep at its default pitch meets it,
/// at most where it is and at least the game's closest. Nothing in the way
/// now, it stays. Whether a swing was under way.
fn come_in<S: Scene<Ee>>(cam: &mut WalkCamera<Ee>, scene: &mut S) -> bool {
    if !crate::mouse::end_swing(cam) {
        return false;
    }
    if let Some(r) = reach(cam, scene, cam.cur.dist) {
        let d = clamp(r - F::new(SHORT), cam.min_orbit_distance, cam.cur.dist);
        place(cam, d);
    }
    true
}

/// The scene as the camera sees it (the module's doc): the camera's sweeps
/// (`VIWalkCameraFilter`'s) go on past what is thin, the first broad thing
/// they meet answered with its hit time on the whole sweep; every other
/// query is the scene's own, so the walk is untouched. With nothing thin in
/// the way it answers as the scene does, bit for bit.
pub struct PastThin<'a, S> {
    pub scene: &'a mut S,
}

impl<S: Scene<Ee>> Scene<Ee> for PastThin<'_, S> {
    fn time(&self) -> i64 {
        self.scene.time()
    }
    fn collide_sphere(&mut self, s: &mut CollSphere<Ee>, f: &mut Filter) -> bool {
        if f.kind != FilterKind::WalkCamera {
            return self.scene.collide_sphere(s, f);
        }
        let (from, to, radius, len, dir) = (s.ray.start, s.ray.end, s.radius, s.ray.length, s.ray.dir);
        let (actor, sprite) = (f.hit_actor, f.hit_sprite);
        // the first leg is the sweep itself; each after it begins past a
        // thin thing, its sphere clear of it
        let mut leg = *s;
        let mut at = F::ZERO;
        for n in 0..THIN_LEGS {
            if !self.scene.collide_sphere(&mut leg, f) {
                // the first leg's answer is the scene's own, as it left it
                if n == 0 {
                    *s = leg;
                } else {
                    (f.hit_actor, f.hit_sprite) = (actor, sprite);
                }
                return false;
            }
            let hit = at + leg.ray.hit_distance();
            let past = if n + 1 < THIN_LEGS { thin(&mut *self.scene, &leg, f.actor) } else { None };
            let Some(past) = past else {
                if n == 0 {
                    *s = leg;
                } else {
                    s.ray.hit_time = hit / len;
                    s.ray.hit_idx = leg.ray.hit_idx;
                    s.ray.hit_normal = leg.ray.hit_normal;
                }
                return true;
            };
            at = hit + past + radius;
            if !(at < len) {
                (f.hit_actor, f.hit_sprite) = (actor, sprite);
                return false;
            }
            let mut a = from;
            a.add_scaled(&dir, at);
            leg = CollSphere::default();
            leg.init(a, to, radius);
        }
        unreachable!("the last leg answers")
    }
    fn collide_env(&mut self, ray: &mut EnvRay<Ee>) -> bool {
        self.scene.collide_env(ray)
    }
    fn env_flags(&self, at: &Vect3<Ee>) -> u32 {
        self.scene.env_flags(at)
    }
}

/// Whether what the sweep `leg` met is thin (the module's doc): a far side
/// within `THIN_DEEP` behind the contact, along the sweep (a probe back from
/// past it), and nothing within `THIN_SIDE` to either side of the contact
/// from `THIN_BEFORE` before it to just past where the sweep goes on (so a
/// wall behind a post is still met). The distance along the sweep from the
/// contact to the thing's far side when thin, else None.
fn thin<S: Scene<Ee>>(scene: &mut S, leg: &CollSphere<Ee>, actor: i32) -> Option<F<Ee>> {
    let dir = leg.ray.dir;
    let mut side = Vect3::cross(&dir, &Vect3::of(0.0, 1.0, 0.0));
    if side.magnitude() < F::new(0.001) {
        return None;
    }
    side.normalize();
    // the sphere's centre at the contact, and a line just inside what it met
    let c = leg.hit_point();
    let mut q = c;
    q.sub_scaled(&leg.ray.hit_normal, leg.radius + F::new(THIN_INTO));
    let mut cq = q;
    cq -= c;
    let into = cq.dot(&dir);
    let mut back = q;
    back.add_scaled(&dir, F::new(THIN_DEEP));
    let mut far = probe(scene, back, q, actor)?;
    far -= c;
    let past = far.dot(&dir);
    if past + leg.radius < F::new(0.05) {
        return None;
    }
    // the probes reach past the front of the sphere the sweep goes on with
    let reach = past + leg.radius + leg.radius + F::new(SHORT) - into;
    for k in [-THIN_SIDE, THIN_SIDE] {
        let mut a = q;
        a.add_scaled(&side, F::new(k));
        let mut b = a;
        a.sub_scaled(&dir, F::new(THIN_BEFORE));
        b.add_scaled(&dir, reach);
        if probe(scene, a, b, actor).is_some() {
            return None;
        }
    }
    Some(past)
}

/// A thin probe of the camera's from `a` to `b`: where it met something.
fn probe<S: Scene<Ee>>(scene: &mut S, a: Vect3<Ee>, b: Vect3<Ee>, actor: i32) -> Option<Vect3<Ee>> {
    let mut s = CollSphere::<Ee>::default();
    s.init(a, b, F::new(THIN_PROBE));
    let mut f = Filter::new(FilterKind::WalkCamera, actor);
    scene.collide_sphere(&mut s, &mut f).then(|| s.hit_point())
}

/// The camera put `d` out along its orbit as `MoveCamera` places it in the
/// follow orbit, the game's pull (flag 4, or 2 to its closest) ended: in
/// `cur` and in `last` alike, as the frame's last tick left them.
fn place(cam: &mut WalkCamera<Ee>, d: F<Ee>) {
    let p = cam.walk.phys;
    let mut dir = Vect3::<Ee>::default();
    dir.hpr(cam.cur.orbit.x, -cam.cur.orbit.y, F::ZERO);
    let mut at = p.position;
    at.y = at.y + cam.target_height;
    at.add_scaled(&dir, d);
    at.y = at.y + p.height_offset;
    for s in [&mut cam.cur, &mut cam.last] {
        s.dist = d;
        s.camera_pos = at;
        s.flags &= !6;
    }
}

/// Scenes for the camera's and the mouse's tests.
#[cfg(test)]
pub mod testing {
    use vi::coll::{CollSphere, EnvRay};
    use vi::fpu::{Ee, F};
    use vi::scene::{Filter, Scene};
    use vi::vect::Vect3;

    /// A flat floor at y = 0, and a wall all round: a sweep reaching
    /// further than `wall` across from where it starts is stopped there. The
    /// clock is `t` (ms).
    pub struct Room {
        pub t: i64,
        pub wall: f32,
    }

    impl Scene<Ee> for Room {
        fn time(&self) -> i64 {
            self.t
        }
        fn collide_sphere(&mut self, s: &mut CollSphere<Ee>, _: &mut Filter) -> bool {
            let r = &mut s.ray;
            let (a, b) = (r.start.y.0 - s.radius.0, r.end.y.0 - s.radius.0);
            let mut hit: Option<(f32, Vect3<Ee>)> = None;
            if a >= 0.0 && b < 0.0 {
                hit = Some((a / (a - b), Vect3::of(0.0, 1.0, 0.0)));
            }
            let (dx, dz) = (r.end.x.0 - r.start.x.0, r.end.z.0 - r.start.z.0);
            let across = (dx * dx + dz * dz).sqrt();
            if across > self.wall {
                let t = self.wall / across;
                if hit.is_none_or(|h| t < h.0) {
                    hit = Some((t, Vect3::of(-dx / across, 0.0, -dz / across)));
                }
            }
            let Some((t, n)) = hit else { return false };
            r.hit_time = F::new(t);
            r.hit_idx = 0;
            r.hit_normal = n;
            true
        }
        fn collide_env(&mut self, _: &mut EnvRay<Ee>) -> bool {
            false
        }
        fn env_flags(&self, _: &Vect3<Ee>) -> u32 {
            0
        }
    }

    /// Ground and walls as planes, each through `at` with its unit normal
    /// `n` pointing to the open side: a sphere's sweep stops at the first it
    /// comes within its radius of. The clock is `t` (ms).
    pub struct Planes {
        pub t: i64,
        pub planes: Vec<([f32; 3], [f32; 3])>,
    }

    impl Planes {
        /// Ground through `at` rising `slope` (a tangent) along the
        /// horizontal unit `way`, and nothing else.
        pub fn slope(at: [f32; 3], way: [f32; 2], slope: f32) -> Planes {
            let len = (1.0 + slope * slope).sqrt();
            Planes { t: 0, planes: vec![(at, [-slope * way[0] / len, 1.0 / len, -slope * way[1] / len])] }
        }

        /// Flat ground through `at`, and a wall `away` from it along the
        /// horizontal unit `way`, facing back.
        pub fn wall(at: [f32; 3], way: [f32; 2], away: f32) -> Planes {
            let w = [at[0] + way[0] * away, at[1], at[2] + way[1] * away];
            Planes { t: 0, planes: vec![(at, [0.0, 1.0, 0.0]), (w, [-way[0], 0.0, -way[1]])] }
        }
    }

    impl Scene<Ee> for Planes {
        fn time(&self) -> i64 {
            self.t
        }
        fn collide_sphere(&mut self, s: &mut CollSphere<Ee>, _: &mut Filter) -> bool {
            let r = &mut s.ray;
            let rad = s.radius.0;
            let d = |p: &Vect3<Ee>, (at, n): &([f32; 3], [f32; 3])| (p.x.0 - at[0]) * n[0] + (p.y.0 - at[1]) * n[1] + (p.z.0 - at[2]) * n[2] - rad;
            let mut hit: Option<(f32, [f32; 3])> = None;
            for pl in &self.planes {
                let (a, b) = (d(&r.start, pl), d(&r.end, pl));
                if a >= 0.0 && b < 0.0 {
                    let t = a / (a - b);
                    if hit.is_none_or(|h| t < h.0) {
                        hit = Some((t, pl.1));
                    }
                }
            }
            let Some((t, n)) = hit else { return false };
            r.hit_time = F::new(t);
            r.hit_idx = 0;
            r.hit_normal = Vect3::of(n[0], n[1], n[2]);
            true
        }
        fn collide_env(&mut self, _: &mut EnvRay<Ee>) -> bool {
            false
        }
        fn env_flags(&self, _: &Vect3<Ee>) -> u32 {
            0
        }
    }

    /// Real geometry, for what is thin and what is broad: the ground at
    /// y = 0 (two triangles 400 across) and boxes standing on it, each face
    /// two triangles facing out, every sweep against every triangle with the
    /// game's own test (`collide_tri`). The clock is `t` (ms).
    pub struct Boxes {
        pub t: i64,
        pub tris: Vec<[Vect3<Ee>; 3]>,
    }

    impl Boxes {
        pub fn new() -> Boxes {
            let mut b = Boxes { t: 0, tris: Vec::new() };
            let g = 200.0;
            b.quad([-g, 0.0, -g], [g, 0.0, -g], [g, 0.0, g], [-g, 0.0, g], [0.0, 1.0, 0.0]);
            b
        }

        /// A box from `lo` to `hi`, its faces facing out.
        pub fn with(mut self, lo: [f32; 3], hi: [f32; 3]) -> Boxes {
            let ([x0, y0, z0], [x1, y1, z1]) = (lo, hi);
            self.quad([x0, y0, z0], [x0, y1, z0], [x0, y1, z1], [x0, y0, z1], [-1.0, 0.0, 0.0]);
            self.quad([x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1], [1.0, 0.0, 0.0]);
            self.quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1], [0.0, -1.0, 0.0]);
            self.quad([x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1], [0.0, 1.0, 0.0]);
            self.quad([x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], [0.0, 0.0, -1.0]);
            self.quad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [0.0, 0.0, 1.0]);
            self
        }

        /// A square post `side` across and `high` tall, centred at (x, z).
        pub fn post(self, x: f32, z: f32, side: f32, high: f32) -> Boxes {
            let h = side / 2.0;
            self.with([x - h, 0.0, z - h], [x + h, high, z + h])
        }

        /// A quad's two triangles, each turned to face `out`.
        fn quad(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3], out: [f32; 3]) {
            let v = |p: [f32; 3]| Vect3::of(p[0], p[1], p[2]);
            for (p, q, r) in [(a, b, c), (a, c, d)] {
                let (p, mut q, mut r) = (v(p), v(q), v(r));
                let mut pl = vi::geom::Plane::<Ee>::default();
                pl.from_tri(&p, &q, &r);
                if pl.n.x.0 * out[0] + pl.n.y.0 * out[1] + pl.n.z.0 * out[2] < 0.0 {
                    std::mem::swap(&mut q, &mut r);
                }
                self.tris.push([p, q, r]);
            }
        }
    }

    impl Scene<Ee> for Boxes {
        fn time(&self) -> i64 {
            self.t
        }
        fn collide_sphere(&mut self, s: &mut CollSphere<Ee>, _: &mut Filter) -> bool {
            let mut hit = false;
            for [a, b, c] in &self.tris {
                hit |= vi::collide::collide_tri(s, 0, a, b, c);
            }
            hit
        }
        fn collide_env(&mut self, _: &mut EnvRay<Ee>) -> bool {
            false
        }
        fn env_flags(&self, _: &Vect3<Ee>) -> u32 {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{Boxes, Room};
    use super::*;

    /// The follow orbit (style 3) at `dist`, standing on the room's floor,
    /// its distance taken as chosen by a first frame; its zoom-out ceiling
    /// `max` (the Extended page's Camera Distance).
    fn orbit_to(room: &mut Room, dist: f32, max: f32) -> (WalkCamera<Ee>, Reach) {
        let mut cam = WalkCamera::<Ee>::default();
        cam.init(Vect3::of(100.0, 0.0, 100.0), Vect3::of(0.5, 0.0, 0.0), room);
        crate::camstyle::set_style(&mut cam, 3, None, room);
        cam.max_orbit_distance = F::new(max);
        cam.set_orbit_distance(F::new(dist));
        let mut reach = Reach::default();
        room.t += 16;
        reach.process(&mut cam, room, 0.016);
        (cam, reach)
    }

    /// The follow orbit at `dist`, at the client's default ceiling (11).
    fn orbit(room: &mut Room, dist: f32) -> (WalkCamera<Ee>, Reach) {
        orbit_to(room, dist, crate::camstyle::FOLLOW_MAX_DISTANCE)
    }

    /// `n` frames of `ms` each, the distance after each.
    fn frames(cam: &mut WalkCamera<Ee>, reach: Option<&mut Reach>, room: &mut Room, n: usize, ms: i64) -> Vec<f32> {
        let mut reach = reach;
        (0..n)
            .map(|_| {
                room.t += ms;
                match reach.as_deref_mut() {
                    Some(r) => r.process(cam, room, ms as f32 / 1000.0),
                    None => cam.process(room),
                };
                cam.cur.dist.0
            })
            .collect()
    }

    /// Where the camera is held with the room's wall `wall` across: `SHORT`
    /// this side of where the sweep, 15 degrees up (the game's pitch), meets
    /// it.
    fn held(wall: f32) -> f32 {
        wall / GAME_PITCH.cos() - SHORT
    }

    /// A camera's state and eye as bits, to tell two cameras apart exactly.
    fn bits(cam: &WalkCamera<Ee>, eye: &Eye<Ee>) -> Vec<u32> {
        let v = |v: &Vect3<Ee>| [v.x.0.to_bits(), v.y.0.to_bits(), v.z.0.to_bits()];
        let mut b = vec![cam.cur.dist.0.to_bits(), cam.cur.flags, cam.last.dist.0.to_bits(), cam.last.flags];
        for w in [&cam.cur.orbit, &cam.cur.camera_pos, &cam.last.camera_pos, &eye.position, &eye.direction, &cam.walk.phys.position] {
            b.extend(v(w));
        }
        b
    }

    /// A wall 5 out holds the 11 just this side of it, where the game's pull
    /// goes on to its 3.5 (the control: `Process` alone, which then keeps
    /// it), and the choice stays 11; the wall gone, the camera eases back out
    /// to it, from rest (its first step small), never faster than 12 a second
    /// nor ever back, there within a second and a half.
    #[test]
    fn a_wall_holds_the_camera_short_of_it_and_it_eases_back_out_once_gone() {
        for ours in [true, false] {
            let mut room = Room { t: 0, wall: 100.0 };
            let (mut cam, mut reach) = orbit(&mut room, 11.0);
            assert_eq!(reach.chosen(&cam).0, 11.0);
            room.wall = 5.0;
            frames(&mut cam, ours.then_some(&mut reach), &mut room, 60, 16);
            if !ours {
                assert_eq!(cam.cur.dist.0, 3.5, "the game's pull");
                room.wall = 100.0;
                let d = frames(&mut cam, None, &mut room, 125, 16);
                assert_eq!(*d.last().unwrap(), 3.5, "the game keeps it");
                continue;
            }
            assert!((cam.cur.dist.0 - held(5.0)).abs() < 1e-3, "held short of the wall: {}", cam.cur.dist.0);
            assert_eq!(reach.chosen(&cam).0, 11.0, "a pull is no choice");
            room.wall = 100.0;
            let d = frames(&mut cam, Some(&mut reach), &mut room, 125, 16);
            assert!(d[0] - held(5.0) < 0.01, "from rest: {}", d[0]);
            for w in d.windows(2) {
                assert!(w[1] >= w[0] && w[1] - w[0] <= SPEED * 0.016 + THERE, "{w:?}");
            }
            assert_eq!(*d.last().unwrap(), 11.0);
            let at = d.iter().position(|&v| v == 11.0).unwrap();
            assert!((40..95).contains(&at), "there after {} frames", at + 1);
        }
    }

    /// The way back takes the same time at 30 and 165 frames a second.
    #[test]
    fn the_way_back_takes_the_same_time_at_any_rate() {
        let time = |ms: i64| {
            let mut room = Room { t: 0, wall: 100.0 };
            let (mut cam, mut reach) = orbit(&mut room, 11.0);
            room.wall = 5.0;
            frames(&mut cam, Some(&mut reach), &mut room, 120, 8);
            assert!((cam.cur.dist.0 - held(5.0)).abs() < 1e-3, "{}", cam.cur.dist.0);
            room.wall = 100.0;
            let n = (4000 / ms) as usize;
            let d = frames(&mut cam, Some(&mut reach), &mut room, n, ms);
            let at = d.iter().position(|&v| v > 10.9).expect("back out");
            (at + 1) as f32 * ms as f32
        };
        let (a, b) = (time(33), time(6));
        assert!((a - b).abs() < 100.0, "30 Hz {a} ms, 165 Hz {b} ms");
    }

    /// The way out clear for some of the way: the camera goes back out as
    /// far as it is, short of what is still in the way, then the rest once it
    /// is clear. The control: the game alone stays at 3.5 throughout.
    #[test]
    fn it_goes_back_out_as_far_as_the_way_is_clear() {
        for ours in [true, false] {
            let mut room = Room { t: 0, wall: 100.0 };
            let (mut cam, mut reach) = orbit(&mut room, 11.0);
            room.wall = 5.0;
            frames(&mut cam, ours.then_some(&mut reach), &mut room, 60, 16);
            for (wall, want) in [(7.0, held(7.0)), (12.0, 11.0)] {
                room.wall = wall;
                frames(&mut cam, ours.then_some(&mut reach), &mut room, 200, 16);
                let want = if ours { want } else { 3.5 };
                assert!((cam.cur.dist.0 - want).abs() < 1e-3, "ours {ours}, a wall {wall} out: {}", cam.cur.dist.0);
            }
        }
    }

    /// F3 20261025-060553: the camera 30 out (Camera Distance 50) and a wall
    /// 20 out: in at the game's own 23.5 a second at most, never past the
    /// wall, and held there, where the game's goes on to 3.5 (the control);
    /// at 30 and 165 frames a second.
    #[test]
    fn far_out_a_wall_brings_the_camera_in_only_as_far_as_it() {
        for ms in [33i64, 6] {
            for ours in [true, false] {
                let mut room = Room { t: 0, wall: 100.0 };
                let (mut cam, mut reach) = orbit_to(&mut room, 30.0, 50.0);
                assert_eq!((cam.cur.dist.0, reach.chosen(&cam).0), (30.0, 30.0));
                room.wall = 20.0;
                let d = frames(&mut cam, ours.then_some(&mut reach), &mut room, (3000 / ms) as usize, ms);
                if !ours {
                    assert_eq!(*d.last().unwrap(), 3.5, "the game's, {ms} ms");
                    continue;
                }
                let mut was = 30.0f32;
                for &v in &d {
                    assert!(was - v <= 23.5 * ms as f32 / 1000.0 + 0.01, "in faster than the game's: {was} to {v}, {ms} ms");
                    assert!(v >= held(20.0) - 1e-3, "past the wall: {v}, {ms} ms");
                    was = v;
                }
                assert!((d.last().unwrap() - held(20.0)).abs() < 1e-3, "{ms} ms: {}", d.last().unwrap());
                assert_eq!(reach.chosen(&cam).0, 30.0, "a pull is no choice");
            }
        }
    }

    /// A wall nearer than the closest collision distance (3 out: the sweep
    /// meets it 3.1 along the orbit): the pull to 3.5, the hunt for another
    /// angle and the way in to the closest are the game's, bit for bit, every
    /// frame, at 30 and 165 frames a second. The control: a wall 6 out is
    /// not the game's.
    #[test]
    fn nearer_than_the_closest_collision_distance_it_is_the_games() {
        for ms in [33i64, 6] {
            for (wall, same) in [(3.0f32, true), (6.0, false)] {
                let (mut ra, mut rb) = (Room { t: 0, wall: 100.0 }, Room { t: 0, wall: 100.0 });
                let (mut ours, mut reach) = orbit(&mut ra, 11.0);
                let (mut game, _) = orbit(&mut rb, 11.0);
                (ra.wall, rb.wall) = (wall, wall);
                let mut differ = 0;
                for _ in 0..4000 / ms {
                    ra.t += ms;
                    rb.t += ms;
                    let ea = reach.process(&mut ours, &mut ra, ms as f32 / 1000.0);
                    let eb = game.process(&mut rb);
                    differ += (bits(&ours, &ea) != bits(&game, &eb)) as usize;
                }
                assert_eq!(differ == 0, same, "a wall {wall} out, {ms} ms: {differ} frames differ");
                if same {
                    assert_eq!(ours.cur.dist.0, 1.5, "in to its closest, as the game");
                }
            }
        }
    }

    /// With nothing in the way the camera is the game's, bit for bit, every
    /// frame, the body running and the orbit pushed round, at 30 and 165
    /// frames a second. The control: a wall in the way is not.
    #[test]
    fn with_nothing_in_the_way_it_is_the_games() {
        for ms in [33i64, 6] {
            for (wall, same) in [(1.0e6f32, true), (8.0, false)] {
                let (mut ra, mut rb) = (Room { t: 0, wall: 1.0e6 }, Room { t: 0, wall: 1.0e6 });
                let (mut ours, mut reach) = orbit(&mut ra, 11.0);
                let (mut game, _) = orbit(&mut rb, 11.0);
                (ra.wall, rb.wall) = (wall, wall);
                let mut differ = 0;
                for i in 0..4000 / ms {
                    ra.t += ms;
                    rb.t += ms;
                    for c in [&mut ours, &mut game] {
                        c.walk.run(F::ONE, F::ZERO);
                        c.orbit_right(F::new(if i % 50 < 25 { 0.6 } else { -0.4 }));
                    }
                    let ea = reach.process(&mut ours, &mut ra, ms as f32 / 1000.0);
                    let eb = game.process(&mut rb);
                    differ += (bits(&ours, &ea) != bits(&game, &eb)) as usize;
                }
                assert_eq!(differ == 0, same, "a wall {wall} out, {ms} ms: {differ} frames differ");
            }
            // it ran
            let mut r = Room { t: 0, wall: 1.0e6 };
            let (mut c, mut reach) = orbit(&mut r, 11.0);
            let from = c.walk.phys.position;
            for _ in 0..1000 / ms {
                r.t += ms;
                c.walk.run(F::ONE, F::ZERO);
                reach.process(&mut c, &mut r, ms as f32 / 1000.0);
            }
            let mut moved = c.walk.phys.position;
            moved -= from;
            assert!(moved.magnitude().0 > 3.0, "{ms} ms: ran {}", moved.magnitude().0);
        }
    }

    /// The choice follows the stick's zoom (the camera's spring) and a
    /// style's distance (P: 3.38); back from the start menu's framing (mode
    /// 1, the preset put back at the pulled distance) it is what it was.
    #[test]
    fn the_choice_is_the_stick_s_zoom_and_a_style_s_not_a_preset_s() {
        let mut room = Room { t: 0, wall: 100.0 };
        let (mut cam, mut reach) = orbit(&mut room, 11.0);
        // the stick zooms in for a while: the choice goes with it
        for _ in 0..10 {
            cam.orbit_in(F::ONE);
            room.t += 16;
            reach.process(&mut cam, &mut room, 0.016);
        }
        frames(&mut cam, Some(&mut reach), &mut room, 60, 16);
        let zoomed = cam.cur.dist.0;
        assert!(zoomed < 10.0, "{zoomed}");
        assert_eq!(reach.chosen(&cam).0, zoomed);
        // pulled in by a wall, then the start menu's framing and back
        room.wall = 3.0;
        frames(&mut cam, Some(&mut reach), &mut room, 60, 16);
        let pulled = crate::camstyle::save(&cam);
        assert!(pulled.distance.0 < zoomed);
        cam.set_camera_mode(1);
        cam.set_orbit_distance(F::new(2.0));
        frames(&mut cam, Some(&mut reach), &mut room, 10, 16);
        crate::camstyle::apply(&mut cam, &pulled);
        room.wall = 100.0;
        frames(&mut cam, Some(&mut reach), &mut room, 200, 16);
        assert_eq!(cam.cur.dist.0, zoomed, "the choice it had");
        // P out of first person: the style's own distance
        crate::camstyle::set_style(&mut cam, 0, None, &mut room);
        frames(&mut cam, Some(&mut reach), &mut room, 2, 16);
        crate::camstyle::set_style(&mut cam, 3, None, &mut room);
        frames(&mut cam, Some(&mut reach), &mut room, 200, 16);
        assert_eq!((reach.chosen(&cam).0, cam.cur.dist.0), (3.38, 3.38));
    }

    /// The follow orbit at 11 behind a body facing `h0` at `at` on `sc`, its
    /// default pitch `pitch` (the mouse's tilt), then `near` put in the
    /// scene's place (an obstacle moved in once the style has chosen its
    /// angle behind); `ms` frames for 4 seconds, through the reach (ours,
    /// `steered` or not) or the game's `Process` alone (the control). The
    /// most the orbit turned off where it began, the distance at the end,
    /// and the camera.
    fn swing(
        sc: &mut super::testing::Planes,
        near: Option<super::testing::Planes>,
        at: [f32; 3],
        h0: f32,
        pitch: f32,
        ours: bool,
        steered: bool,
        ms: i64,
    ) -> (f32, f32, WalkCamera<Ee>) {
        let mut cam = WalkCamera::<Ee>::default();
        cam.init(Vect3::of(at[0], at[1], at[2]), Vect3::of(h0, 0.0, 0.0), sc);
        crate::camstyle::set_style(&mut cam, 3, None, sc);
        cam.set_orbit_distance(F::new(11.0));
        cam.set_def_orbit_pitch_angle(F::new(pitch));
        if let Some(n) = near {
            sc.planes = n.planes;
        }
        let mut reach = Reach { steered, ..Default::default() };
        let start = cam.cur.orbit.x.0;
        let mut most = 0.0f32;
        for _ in 0..4000 / ms {
            sc.t += ms;
            if ours {
                reach.process(&mut cam, sc, ms as f32 / 1000.0);
            } else {
                cam.process(sc);
            }
            let d = (cam.cur.orbit.x.0 - start).rem_euclid(std::f32::consts::TAU);
            most = most.max(d.min(std::f32::consts::TAU - d));
        }
        (most, cam.cur.dist.0, cam)
    }

    /// The camera right in front of what is in the way, as `come_in` holds
    /// it: the game's own sweep at its pitch clear to it, and meeting
    /// something `SHORT` and a hair farther out.
    fn in_front(sc: &mut super::testing::Planes, cam: &WalkCamera<Ee>) -> bool {
        reach(cam, sc, cam.cur.dist).is_none() && reach(cam, sc, cam.cur.dist + F::new(SHORT + 0.01)).is_some()
    }

    /// Turned by no more than the rounding of the orbit's re-derivation
    /// from where the camera is (`Process`, mode 3), a frame at a time.
    const STILL: f32 = 1e-4;

    /// The body's way back (behind it, where the camera is) at `h0`.
    fn behind(h0: f32) -> [f32; 2] {
        [-h0.sin(), -h0.cos()]
    }

    /// A wall 6 behind a body and the follow orbit 30 out (of 50): held this
    /// side of it, not at 3.5; running away from it, the camera goes back
    /// out after it, never in again and never past it; standing, it settles
    /// with the body, never rocking; walking back toward it, it comes in with it,
    /// never more than a frame's walk past where it is held. The control: the
    /// game's stays at 3.5 however far the body runs. At 30 and 165 frames a
    /// second.
    #[test]
    fn it_follows_a_wall_out_and_back_in_as_the_body_moves() {
        let (at, h0) = ([100.0f32, 0.0, 100.0], 0.5f32);
        let way = behind(h0);
        let wall = [at[0] + way[0] * 6.0, at[2] + way[1] * 6.0];
        // the camera's sphere centre in front of the wall, along the way back
        let front = |p: &Vect3<Ee>| (wall[0] - p.x.0) * way[0] + (wall[1] - p.z.0) * way[1];
        for ms in [33i64, 6] {
            for ours in [true, false] {
                let mut sc = super::testing::Planes::slope(at, way, 0.0);
                let mut cam = WalkCamera::<Ee>::default();
                cam.init(Vect3::of(at[0], at[1], at[2]), Vect3::of(h0, 0.0, 0.0), &mut sc);
                crate::camstyle::set_style(&mut cam, 3, None, &mut sc);
                cam.max_orbit_distance = F::new(50.0);
                cam.set_orbit_distance(F::new(30.0));
                let mut reach = Reach::default();
                sc.t += 16;
                reach.process(&mut cam, &mut sc, 0.016);
                sc.planes = super::testing::Planes::wall(at, way, 6.0).planes;
                // `n` frames pushing the body (`push` forward, - back): the
                // distance and the camera's room in front of the wall after each
                let mut go = |cam: &mut WalkCamera<Ee>, reach: &mut Reach, n: i64, push: f32| -> Vec<(f32, f32)> {
                    (0..n)
                        .map(|_| {
                            sc.t += ms;
                            if push != 0.0 {
                                cam.walk.run(F::new(push), F::ZERO);
                            }
                            if ours {
                                reach.process(cam, &mut sc, ms as f32 / 1000.0);
                            } else {
                                cam.process(&mut sc);
                            }
                            (cam.cur.dist.0, front(&cam.cur.camera_pos))
                        })
                        .collect()
                };
                let settle = go(&mut cam, &mut reach, 2000 / ms, 0.0);
                let first = settle.last().unwrap().0;
                let away = go(&mut cam, &mut reach, 1500 / ms, 1.0);
                if !ours {
                    assert_eq!((first, away.last().unwrap().0), (3.5, 3.5), "the game's, {ms} ms");
                    continue;
                }
                assert!((first - (5.5 / GAME_PITCH.cos() - SHORT)).abs() < 1e-3, "held short of the wall: {first}, {ms} ms");
                for w in away.windows(2) {
                    assert!(w[1].0 >= w[0].0 - 1e-4 && w[1].1 >= 0.5 - 0.02, "running away, {ms} ms: {w:?}");
                }
                let gone = away.last().unwrap().0 - first;
                assert!(gone > 8.0, "back out after the body, {ms} ms: {gone}");
                // standing (the body glides to a stop): never in, and settled
                let stand = go(&mut cam, &mut reach, 2000 / ms, 0.0);
                for w in stand.windows(2) {
                    assert!(w[1].0 >= w[0].0 - 1e-4 && w[1].1 >= 0.5 - 0.02, "standing, {ms} ms: {w:?}");
                }
                let tail = &stand[stand.len() - 10..];
                assert!(tail.windows(2).all(|w| w[1].0 - w[0].0 < 1e-3), "standing it still goes, {ms} ms: {tail:?}");
                let back = go(&mut cam, &mut reach, 1000 / ms, -1.0);
                assert!(back.last().unwrap().0 < tail[0].0 - 2.0, "in with the body, {ms} ms: {:?}", back.last());
                for &(d, room) in &back {
                    assert!(room >= 0.5 - 3.0 * ms as f32 / 1000.0 - 0.02 && d > 3.5, "walking back, {ms} ms: {d}, {room}");
                }
            }
        }
    }

    /// F3 20261024-100428: the camera tilted level with the head (its
    /// default pitch -0.0094, the report's), a 22-degree slope rising behind
    /// a run downhill. The game's camera swings a quarter turn round (the
    /// control: `Process` alone), and the run would turn with it; ours stays
    /// behind and comes in only as far as the slope needs (it went to its
    /// closest, 1.5, until 2026-11-09). Lower still (-0.2), a 10-degree
    /// slope did it.
    #[test]
    fn tilted_under_the_games_pitch_a_slope_brings_the_camera_in_not_round() {
        let (at, h0) = ([100.0, 0.0, 100.0], 0.5);
        for (slope, pitch) in [(22.0f32, -0.0094), (10.0, -0.2)] {
            let mut sc = super::testing::Planes::slope(at, behind(h0), slope.to_radians().tan());
            let (game, _, _) = swing(&mut sc, None, at, h0, pitch, false, false, 16);
            assert!(game > 0.7, "the control: the game's swings round ({game} at {slope} degrees)");
            let mut sc = super::testing::Planes::slope(at, behind(h0), slope.to_radians().tan());
            let (ours, dist, cam) = swing(&mut sc, None, at, h0, pitch, true, false, 16);
            assert!(ours < STILL, "{slope} degrees, pitch {pitch}: turned {ours}");
            assert!(dist > 2.5 && in_front(&mut sc, &cam), "{slope} degrees, pitch {pitch}: in front of the slope, not at the closest: {dist}");
        }
    }

    /// At the game's own pitch a slope behind, however steep, only brings
    /// the camera in (to 3.5; past 26 degrees the alternate pitch takes it
    /// up), so a PS2's never swings on one; a wall close behind swings it a
    /// quarter turn, the game's and ours alike: the guard leaves the game's
    /// pitch to the game.
    #[test]
    fn at_the_games_pitch_the_swing_is_the_games() {
        let (at, h0) = ([100.0, 0.0, 100.0], 0.5);
        for slope in [10.0f32, 22.0, 25.0, 30.0] {
            let mut sc = super::testing::Planes::slope(at, behind(h0), slope.to_radians().tan());
            let (turned, _, _) = swing(&mut sc, None, at, h0, GAME_PITCH, false, false, 16);
            assert!(turned < STILL, "{slope} degrees: {turned}");
        }
        let wall = |away| super::testing::Planes::wall(at, behind(h0), away);
        for ours in [false, true] {
            let (turned, _, _) = swing(&mut wall(100.0), Some(wall(2.5)), at, h0, GAME_PITCH, ours, false, 16);
            assert!((turned - std::f32::consts::FRAC_PI_2).abs() < 1e-4, "ours {ours}: {turned}");
        }
        // the control: the same wall with the camera tilted under it comes
        // in, as far as the wall needs
        let mut sc = wall(100.0);
        let (turned, dist, cam) = swing(&mut sc, Some(wall(2.5)), at, h0, 0.1, true, false, 16);
        assert!(turned < STILL && dist > 1.5 && in_front(&mut sc, &cam), "{turned}, {dist}");
    }

    /// Steered (the camera kept behind the body: the right button), a wall
    /// close behind does not swing the camera round, where the game swings it
    /// a quarter turn (the control: unsteered, as above), and it comes in only
    /// as far as the wall needs: 2.0 out, where it went to its closest (1.5)
    /// until 2026-11-09 (F3, the user, steering round in Freeport's well: "it
    /// hits the poles and zooms all the way in to char"). At 30 and 165
    /// frames a second (at 30 a swing begun in a frame's first tick moves in
    /// its second before it is ended: under a degree).
    #[test]
    fn steered_a_wall_brings_the_camera_in_only_as_far_as_it_not_round() {
        let (at, h0) = ([100.0, 0.0, 100.0], 0.5);
        let wall = |away| super::testing::Planes::wall(at, behind(h0), away);
        for (ms, most) in [(6i64, STILL), (33, 0.0175)] {
            let (game, _, _) = swing(&mut wall(100.0), Some(wall(2.5)), at, h0, GAME_PITCH, true, false, ms);
            assert!((game - std::f32::consts::FRAC_PI_2).abs() < 1e-4, "the control: unsteered it swings round, {ms} ms: {game}");
            let mut sc = wall(100.0);
            let (turned, dist, cam) = swing(&mut sc, Some(wall(2.5)), at, h0, GAME_PITCH, true, true, ms);
            assert!(turned < most, "{ms} ms: turned {turned}");
            // the sweep meets the wall 2.0 across: 2.07 along the 15 degrees
            let want = 2.0 / GAME_PITCH.cos() - SHORT;
            assert!((dist - want).abs() < 1e-3 && in_front(&mut sc, &cam), "{ms} ms: {dist}, not {want}");
        }
    }

    /// At 30 frames a second a frame can hold two of the camera's 32 ms
    /// ticks, and a swing begun in the first moves in the second before the
    /// guard stops it: under a degree; at 165 none.
    #[test]
    fn the_guard_holds_at_30_and_165_frames_a_second() {
        let (at, h0) = ([100.0, 0.0, 100.0], 0.5);
        for (ms, most) in [(6i64, STILL), (33, 0.0175)] {
            let mut sc = super::testing::Planes::slope(at, behind(h0), 22f32.to_radians().tan());
            let (turned, dist, cam) = swing(&mut sc, None, at, h0, -0.2, true, false, ms);
            assert!(turned < most && dist > 1.5 && in_front(&mut sc, &cam), "{ms} ms: {turned}, {dist}");
        }
    }

    /// The follow orbit at `dist` (of 50) behind a body facing heading 0 (the
    /// camera toward -z) at (100, 0, 100), at the game's pitch, its angle
    /// chosen with nothing in the way; then 3 seconds of `ms` frames on `sc`
    /// through the reach, the scene seen through `PastThin` (ours) or as it is
    /// (the control). Each frame's state as `bits`, and the camera.
    fn behind_boxes(sc: &mut Boxes, dist: f32, ours: bool, ms: i64) -> (Vec<Vec<u32>>, WalkCamera<Ee>) {
        let mut cam = WalkCamera::<Ee>::default();
        let mut open = Boxes::new();
        cam.init(Vect3::of(100.0, 0.0, 100.0), Vect3::of(0.0, 0.0, 0.0), &mut open);
        crate::camstyle::set_style(&mut cam, 3, None, &mut open);
        cam.max_orbit_distance = F::new(50.0);
        cam.set_orbit_distance(F::new(dist));
        let mut reach = Reach::default();
        sc.t = open.t;
        let mut out = Vec::new();
        for _ in 0..3000 / ms {
            sc.t += ms;
            let dt = ms as f32 / 1000.0;
            let eye = if ours { reach.process(&mut cam, &mut PastThin { scene: &mut *sc }, dt) } else { reach.process(&mut cam, &mut *sc, dt) };
            out.push(bits(&cam, &eye));
        }
        (out, cam)
    }

    /// A frame's distance out of its `bits`.
    fn dist_of(b: &[u32]) -> f32 {
        f32::from_bits(b[0])
    }

    /// A post 0.28 across (the well's) 3 behind the body, on the camera's
    /// line or 0.4 off it, does not bring a camera 11 out in at all, nor swing
    /// it; the game's comes in to 3.5 and swings round past it (the control).
    /// At 30 and 165 frames a second.
    #[test]
    fn a_post_brings_the_camera_in_not_at_all() {
        for ms in [33i64, 6] {
            for off in [0.0f32, 0.4] {
                let posted = || Boxes::new().post(100.0 + off, 97.0, 0.28, 4.0);
                let (ours, cam) = behind_boxes(&mut posted(), 11.0, true, ms);
                let nearest = ours.iter().map(|b| dist_of(b)).fold(11.0, f32::min);
                assert_eq!(nearest, 11.0, "{ms} ms, {off} off: in to {nearest}");
                assert!(off_behind(cam.cur.orbit.x.0) < 1e-3, "{ms} ms, {off} off: swung to {}", cam.cur.orbit.x.0);
                let (game, _) = behind_boxes(&mut posted(), 11.0, false, ms);
                let (nearest, turned) = moved(&game);
                assert!(nearest <= 3.5 && turned > 0.7, "the control, {ms} ms, {off} off: the game's in to {nearest}, round {turned}");
            }
        }
    }

    /// How far an orbit angle is off straight behind (pi), either way.
    fn off_behind(a: f32) -> f32 {
        let d = (a - std::f32::consts::PI).rem_euclid(std::f32::consts::TAU);
        d.min(std::f32::consts::TAU - d)
    }

    /// The nearest a run's camera came, and the most its orbit turned off
    /// straight behind (radians).
    fn moved(run: &[Vec<u32>]) -> (f32, f32) {
        let nearest = run.iter().map(|b| dist_of(b)).fold(f32::MAX, f32::min);
        let turned = run.iter().map(|b| off_behind(f32::from_bits(b[4]))).fold(0.0, f32::max);
        (nearest, turned)
    }

    /// What is broad is the game's, bit for bit, every frame: a wall 6
    /// across 3 behind, a trunk 1.2 across, and nothing in the way. The
    /// control: a post is not.
    #[test]
    fn walls_and_trunks_are_the_games() {
        let cases: [(&str, Boxes, bool); 4] = [
            ("a wall", Boxes::new().with([97.0, 0.0, 96.85], [103.0, 6.0, 97.15]), true),
            ("a trunk", Boxes::new().post(100.0, 97.0, 1.2, 6.0), true),
            ("nothing", Boxes::new(), true),
            ("a post", Boxes::new().post(100.0, 97.0, 0.28, 4.0), false),
        ];
        for (what, sc, same) in cases {
            for ms in [33i64, 6] {
                let (ours, _) = behind_boxes(&mut Boxes { t: 0, tris: sc.tris.clone() }, 11.0, true, ms);
                let (game, _) = behind_boxes(&mut Boxes { t: 0, tris: sc.tris.clone() }, 11.0, false, ms);
                let differ = ours.iter().zip(&game).filter(|(a, b)| a != b).count();
                assert_eq!(differ == 0, same, "{what}, {ms} ms: {differ} frames differ");
            }
        }
    }

    /// A post 3 behind and a wall 7 behind: the camera goes on past the post
    /// and is held in front of the wall (`SHORT` this side of where the sweep
    /// meets it, 6.35 across), never past it; the game's comes in to 3.5 at
    /// the post (the control).
    #[test]
    fn past_a_post_a_wall_still_stops_the_camera() {
        for ms in [33i64, 6] {
            let walled = || Boxes::new().post(100.0, 97.0, 0.28, 4.0).with([95.0, 0.0, 92.85], [105.0, 8.0, 93.15]);
            let (ours, _) = behind_boxes(&mut walled(), 11.0, true, ms);
            let want = 6.35 / GAME_PITCH.cos() - SHORT;
            let end = dist_of(ours.last().unwrap());
            assert!((end - want).abs() < 1e-3, "{ms} ms: held at {end}, not {want}");
            assert!(ours.iter().all(|b| dist_of(b) >= want - 1e-3), "{ms} ms: past the post, it never came in further");
            let (game, _) = behind_boxes(&mut walled(), 11.0, false, ms);
            assert!(moved(&game).0 <= 3.5, "the control, {ms} ms: the game's came in to {}", moved(&game).0);
        }
    }

    /// A wall 0.3 behind a post (the probes beside the post meet it): once
    /// the camera has had a second to come in from 11 (where both begin,
    /// behind the wall), it never looks through the wall, its line from the
    /// head never crossing the wall's face, at 30 and 165 frames a second;
    /// the control, the same with the game's camera, never does either.
    #[test]
    fn a_wall_just_behind_a_post_is_never_looked_through() {
        for ms in [33i64, 6] {
            for ours in [true, false] {
                let mut sc = Boxes::new().post(100.0, 97.0, 0.28, 4.0).with([97.0, 0.0, 96.25], [103.0, 6.0, 96.55]);
                let (run, _) = behind_boxes(&mut sc, 11.0, ours, ms);
                for (i, b) in run.iter().enumerate().skip(1000 / ms as usize) {
                    let f = |k: usize| f32::from_bits(b[k]);
                    let (head, eye) = ([f(19), f(20) + 1.68, f(21)], [f(7), f(8), f(9)]);
                    // where the head-to-eye line meets the wall's face (z 96.55)
                    if eye[2] < 96.55 {
                        let t = (head[2] - 96.55) / (head[2] - eye[2]);
                        let (x, y) = (head[0] + (eye[0] - head[0]) * t, head[1] + (eye[1] - head[1]) * t);
                        assert!(!((97.0..103.0).contains(&x) && (0.0..6.0).contains(&y)), "ours {ours}, {ms} ms, frame {i}: through the wall at ({x}, {y})");
                    }
                }
            }
        }
    }

    /// The camera turned (as the left button turns it, `SetOrbitAngle` a
    /// frame: a quarter turn a second) from `from` to `to` degrees (0 toward
    /// +z, 90 toward +x) round a body standing at `at` in Freeport's well,
    /// its default pitch `pitch` degrees, `dist` out of `max`; through
    /// `PastThin` (ours) or not (the control). Each frame's orbit (degrees,
    /// as turned), distance, and orbit after the frame (degrees).
    #[allow(clippy::too_many_arguments)]
    fn turn_in_the_well(world: &mut vi::world::World<Ee>, at: [f32; 3], pitch: f32, dist: f32, max: f32, from: f32, to: f32, ours: bool) -> Vec<(f32, f32, f32)> {
        let mut cam = WalkCamera::<Ee>::default();
        cam.init(Vect3::of(at[0], at[1], at[2]), Vect3::of(0.0, 0.0, 0.0), world);
        crate::camstyle::set_style(&mut cam, 3, None, world);
        cam.max_orbit_distance = F::new(max);
        cam.set_orbit_distance(F::new(dist));
        cam.set_def_orbit_pitch_angle(F::new(pitch.to_radians()));
        cam.set_orbit_angle(F::new(from.to_radians()));
        let mut reach = Reach::default();
        let step = std::f32::consts::FRAC_PI_2 * 0.016;
        let mut out = Vec::new();
        let mut a = from.to_radians();
        while a <= to.to_radians() {
            cam.set_orbit_angle(F::new(a));
            world.time += 16;
            if ours {
                reach.process(&mut cam, &mut PastThin { scene: &mut *world }, 0.016);
            } else {
                reach.process(&mut cam, &mut *world, 0.016);
            }
            let after = cam.cur.orbit.x.0.rem_euclid(std::f32::consts::TAU).to_degrees();
            out.push((a.to_degrees(), cam.cur.dist.0, after));
            a += step;
        }
        out
    }

    /// F3 2026-11-09, the user, zoomed out in Freeport's well: "we rotate cam
    /// the posts collision make the cam zoom". Its four posts (0.28 across,
    /// at 38, 142, 218 and 322 degrees round its middle) no longer bring the
    /// camera in at all. From the middle, the camera tilted to 5 degrees
    /// (where the posts are all that is in the way), 30 out of 50 and 11 out
    /// of 11, turned the half turn between the gables: never in, where the
    /// game's came in to the posts (the control). At the game's 15 degrees,
    /// 11 out: never in through the first post's sector, and still in under
    /// the roof's eaves past it (broad: the control that the roof still
    /// stops it). Beside a post at the game's own 3.38: the camera neither
    /// comes in nor swings while the post is behind it, where the game's
    /// swings round (the control).
    #[test]
    fn turned_round_in_freeports_well_the_posts_bring_the_camera_in_not_at_all() {
        let (x, y, z) = (25296.73f32, 54.493f32, 15750.06f32);
        let Some(mut world) = crate::walker::tests::tunaria_round(x, z) else { return };
        let middle = [x, y, z];
        for (dist, max) in [(30.0f32, 50.0f32), (11.0, 11.0)] {
            let ours = turn_in_the_well(&mut world, middle, 5.0, dist, max, 0.0, 180.0, true);
            let nearest = ours.iter().map(|o| o.1).fold(f32::MAX, f32::min);
            assert_eq!(nearest, dist, "tilted 5 degrees, {dist} out: in to {nearest}");
            let game = turn_in_the_well(&mut world, middle, 5.0, dist, max, 0.0, 180.0, false);
            let pulled = game.iter().filter(|o| (25.0..50.0).contains(&o.0)).map(|o| o.1).fold(f32::MAX, f32::min);
            assert!(pulled < dist - 1.0, "the control, {dist} out: the game's came in only to {pulled} at the first post");
        }
        let ours = turn_in_the_well(&mut world, middle, 15.0, 11.0, 11.0, 0.0, 180.0, true);
        let post = ours.iter().filter(|o| o.0 < 43.0).map(|o| o.1).fold(f32::MAX, f32::min);
        assert_eq!(post, 11.0, "at 15 degrees, through the first post's sector");
        let eaves = ours.iter().filter(|o| (70.0..110.0).contains(&o.0)).map(|o| o.1).fold(f32::MAX, f32::min);
        assert!(eaves < 6.0, "the roof's underside still brings it in: {eaves}");
        let game = turn_in_the_well(&mut world, middle, 15.0, 11.0, 11.0, 0.0, 180.0, false);
        let post = game.iter().filter(|o| o.0 < 43.0).map(|o| o.1).fold(f32::MAX, f32::min);
        assert!(post < 10.0, "the control: the game's came in at the first post: {post}");
        // beside the post at 38 degrees, the game's own distance, the post
        // behind the camera from 20 to 65 degrees
        let beside = [x + 1.6, y, z + 2.2];
        let ours = turn_in_the_well(&mut world, beside, 15.0, 3.38, 11.0, 10.0, 65.0, true);
        for &(want, d, after) in &ours {
            assert!(d == 3.38 && (after - want).abs() < 0.05, "beside the post at {want}: {d} out, the orbit at {after}");
        }
        let game = turn_in_the_well(&mut world, beside, 15.0, 3.38, 11.0, 10.0, 65.0, false);
        // (the turn puts the orbit back each frame: a swing begun shows as up
        // to its pi a second for a frame, 2.9 degrees at 60)
        let swung = game.iter().map(|&(want, _, after)| (after - want).abs()).fold(0.0, f32::max);
        assert!(swung > 1.0, "the control: the game's swings round the post: {swung} degrees");
    }
}

