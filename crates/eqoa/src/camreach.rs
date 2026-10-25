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
//! the camera 11 degrees under the head). Under the game's pitch the swing is
//! not begun: the camera comes in to its closest instead
//! (`mouse::come_in_instead`, as the right button's steer does), and the
//! way back out above brings it out again once the slope is behind. A frame
//! longer than the camera's 32 ms tick can let a swing move before it is
//! stopped: under a degree at 30 frames a second.

use vi::camera::{Eye, WalkCamera};
use vi::coll::CollSphere;
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

#[derive(Clone, Copy, Debug, Default)]
pub struct Reach {
    /// the follow orbit's distance the player chose; none before the orbit
    /// is first seen
    pub chosen: Option<F<Ee>>,
    /// the camera's mode and distance as the last frame left them
    end: Option<(i32, F<Ee>)>,
    /// the way back out's speed now, a second
    speed: f32,
}

impl Reach {
    /// The distance chosen, within the orbit's range (before one is, the
    /// camera's own).
    pub fn chosen(&self, cam: &WalkCamera<Ee>) -> F<Ee> {
        clamp(self.chosen.unwrap_or(cam.cur.dist), cam.min_orbit_distance, cam.max_orbit_distance)
    }

    /// The camera's frame (`Process`), `dt_s` seconds after the last: the
    /// choice kept, the way back out before it, and the game's pull held
    /// short of what is in the way after it.
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
        // tilted under the game's pitch, no swing round (the module's doc)
        if cam.camera_mode == 3 && cam.def_orbit_pitch < F::new(GAME_PITCH) {
            crate::mouse::come_in_instead(cam);
        }
        let pull = (f0 | cam.cur.flags) & 6 != 0;
        // the stick's zoom (the spring) is a choice; a pull (flags 2, 4) is not
        if mode == 3 && cam.camera_mode == 3 && spring && !pull && cam.cur.dist != d0 {
            self.chosen = Some(cam.cur.dist);
        }
        // the game's pull (or one begun and ended inside this frame's ticks:
        // the distance down with no zoom of the stick's) held short of what
        // is in the way, when that is past its closest collision distance
        if mode == 3 && cam.camera_mode == 3 && (pull || (!spring && cam.cur.dist < d0))
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
}

#[cfg(test)]
mod tests {
    use super::testing::Room;
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
    /// angle behind); `ms` frames for 4 seconds, through the reach (ours)
    /// or the game's `Process` alone (the control). The most the orbit
    /// turned off where it began, and the distance at the end.
    fn swing(sc: &mut super::testing::Planes, near: Option<super::testing::Planes>, at: [f32; 3], h0: f32, pitch: f32, ours: bool, ms: i64) -> (f32, f32) {
        let mut cam = WalkCamera::<Ee>::default();
        cam.init(Vect3::of(at[0], at[1], at[2]), Vect3::of(h0, 0.0, 0.0), sc);
        crate::camstyle::set_style(&mut cam, 3, None, sc);
        cam.set_orbit_distance(F::new(11.0));
        cam.set_def_orbit_pitch_angle(F::new(pitch));
        if let Some(n) = near {
            sc.planes = n.planes;
        }
        let mut reach = Reach::default();
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
        (most, cam.cur.dist.0)
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
    /// behind and comes in to its closest. Lower still (-0.2), a 10-degree
    /// slope did it.
    #[test]
    fn tilted_under_the_games_pitch_a_slope_brings_the_camera_in_not_round() {
        let (at, h0) = ([100.0, 0.0, 100.0], 0.5);
        for (slope, pitch) in [(22.0f32, -0.0094), (10.0, -0.2)] {
            let mut sc = super::testing::Planes::slope(at, behind(h0), slope.to_radians().tan());
            let (game, _) = swing(&mut sc, None, at, h0, pitch, false, 16);
            assert!(game > 0.7, "the control: the game's swings round ({game} at {slope} degrees)");
            let mut sc = super::testing::Planes::slope(at, behind(h0), slope.to_radians().tan());
            let (ours, dist) = swing(&mut sc, None, at, h0, pitch, true, 16);
            assert!(ours < STILL && dist == 1.5, "{slope} degrees, pitch {pitch}: {ours}, {dist}");
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
            let (turned, _) = swing(&mut sc, None, at, h0, GAME_PITCH, false, 16);
            assert!(turned < STILL, "{slope} degrees: {turned}");
        }
        let wall = |away| super::testing::Planes::wall(at, behind(h0), away);
        for ours in [false, true] {
            let (turned, _) = swing(&mut wall(100.0), Some(wall(2.5)), at, h0, GAME_PITCH, ours, 16);
            assert!((turned - std::f32::consts::FRAC_PI_2).abs() < 1e-4, "ours {ours}: {turned}");
        }
        // the control: the same wall with the camera tilted under it comes in
        let (turned, dist) = swing(&mut wall(100.0), Some(wall(2.5)), at, h0, 0.1, true, 16);
        assert!(turned < STILL && dist == 1.5, "{turned}, {dist}");
    }

    /// At 30 frames a second a frame can hold two of the camera's 32 ms
    /// ticks, and a swing begun in the first moves in the second before the
    /// guard stops it: under a degree; at 165 none.
    #[test]
    fn the_guard_holds_at_30_and_165_frames_a_second() {
        let (at, h0) = ([100.0, 0.0, 100.0], 0.5);
        for (ms, most) in [(6i64, STILL), (33, 0.0175)] {
            let mut sc = super::testing::Planes::slope(at, behind(h0), 22f32.to_radians().tan());
            let (turned, dist) = swing(&mut sc, None, at, h0, -0.2, true, ms);
            assert!(turned < most && dist == 1.5, "{ms} ms: {turned}, {dist}");
        }
    }
}

