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
//! Here the distance the player chose is kept (`Reach::chosen`): set by
//! whatever sets the distance between two frames (the wheel, a style, a
//! touch screen's pinch) and by the stick's zoom (the spring moving while no
//! pull is under way); back from the start menu's framing
//! (mode 1, `ApplyCameraPreset`) the choice it had stands. Once the game's own
//! sweep reaches the chosen distance clear, with the camera at rest (no snap,
//! no pull, no zoom under way), it eases back out there (`ease`): its speed
//! grows from nothing, 30 a second, to at most 12 a second, and slows over
//! the last 2: 3.5 to 11 in about 1.3 seconds. Pulled in again on its way,
//! the pull is the game's.

use vi::camera::{Eye, WalkCamera};
use vi::coll::CollSphere;
use vi::fpu::{Ee, F, clamp};
use vi::scene::{Filter, FilterKind, Scene};
use vi::vect::Vect3;

/// The way back out: its speed grows `ACCEL` a second from rest, to at most
/// `SPEED`, and is at most `RATE` times the distance left (m/s); within
/// `THERE` of the distance chosen it is there.
const ACCEL: f32 = 30.0;
const SPEED: f32 = 12.0;
const RATE: f32 = 6.0;
const THERE: f32 = 0.05;

/// The distance spring moving faster than this (a second): a zoom under way.
const MOVING: f32 = 0.001;

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
    /// choice kept, and the way back out before it.
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
        let eye = cam.process(scene);
        // the stick's zoom (the spring) is a choice; a pull (flags 2, 4) is not
        if mode == 3 && cam.camera_mode == 3 && spring && (f0 | cam.cur.flags) & 6 == 0 && cam.cur.dist != d0 {
            self.chosen = Some(cam.cur.dist);
        }
        self.end = Some((cam.camera_mode, cam.cur.dist));
        eye
    }

    /// A step back out toward the distance chosen, when the camera is at
    /// rest and the game's sweep reaches that distance clear.
    fn ease<S: Scene<Ee>>(&mut self, cam: &mut WalkCamera<Ee>, scene: &mut S, dt_s: f32) {
        let want = self.chosen(cam);
        let resting = cam.cur.flags & (1 | 2 | 4 | 8) == 0 && !zooming(cam);
        if !resting || want <= cam.cur.dist || !clear(cam, scene, want) {
            self.speed = 0.0;
            return;
        }
        let left = (want - cam.cur.dist).0;
        self.speed = (self.speed + ACCEL * dt_s).min(SPEED).min(left * RATE);
        let step = self.speed * dt_s;
        let d = if step >= left - THERE { want } else { cam.cur.dist + F::new(step) };
        cam.cur.dist = d;
        cam.last.dist = d;
    }
}

/// The distance spring moving or pushed: the stick's zoom under way.
fn zooming(cam: &WalkCamera<Ee>) -> bool {
    cam.cur.dist_force != F::ZERO || F::new(MOVING) < cam.cur.dist_vel.abs()
}

/// Whether the game's own sweep (`MoveCamera`'s first: from the head out
/// along the orbit at the default pitch) reaches `dist` clear.
pub fn clear<S: Scene<Ee>>(cam: &WalkCamera<Ee>, scene: &mut S, dist: F<Ee>) -> bool {
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
    !scene.collide_sphere(&mut s, &mut f)
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
}

#[cfg(test)]
mod tests {
    use super::testing::Room;
    use super::*;

    /// The follow orbit (style 3) at `dist`, standing on the room's floor,
    /// its distance taken as chosen by a first frame.
    fn orbit(room: &mut Room, dist: f32) -> (WalkCamera<Ee>, Reach) {
        let mut cam = WalkCamera::<Ee>::default();
        cam.init(Vect3::of(100.0, 0.0, 100.0), Vect3::of(0.5, 0.0, 0.0), room);
        crate::camstyle::set_style(&mut cam, 3, None, room);
        cam.set_orbit_distance(F::new(dist));
        let mut reach = Reach::default();
        room.t += 16;
        reach.process(&mut cam, room, 0.016);
        (cam, reach)
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

    /// A wall 5 out pulls the 11 in to the game's 3.5 and the choice stays
    /// 11; the wall gone, the camera eases back out to it, from rest (its
    /// first step small), never faster than 12 a second nor ever back, there
    /// within two seconds. The control: the game alone keeps 3.5.
    #[test]
    fn a_pulled_in_camera_eases_back_out_once_there_is_room() {
        for reached in [true, false] {
            let mut room = Room { t: 0, wall: 100.0 };
            let (mut cam, mut reach) = orbit(&mut room, 11.0);
            assert_eq!(reach.chosen(&cam).0, 11.0);
            room.wall = 5.0;
            frames(&mut cam, Some(&mut reach), &mut room, 60, 16);
            assert_eq!(cam.cur.dist.0, 3.5, "the game's pull");
            assert_eq!(reach.chosen(&cam).0, 11.0, "a pull is no choice");
            room.wall = 100.0;
            let d = frames(&mut cam, reached.then_some(&mut reach), &mut room, 125, 16);
            if !reached {
                assert_eq!(*d.last().unwrap(), 3.5, "the game keeps it");
                continue;
            }
            assert!(d[0] - 3.5 < 0.01, "from rest: {}", d[0]);
            for w in d.windows(2) {
                assert!(w[1] >= w[0] && w[1] - w[0] <= SPEED * 0.016 + THERE, "{w:?}");
            }
            assert_eq!(*d.last().unwrap(), 11.0);
            let at = d.iter().position(|&v| v == 11.0).unwrap();
            assert!((60..125).contains(&at), "there after {} frames", at + 1);
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
            assert_eq!(cam.cur.dist.0, 3.5);
            room.wall = 100.0;
            let n = (4000 / ms) as usize;
            let d = frames(&mut cam, Some(&mut reach), &mut room, n, ms);
            let at = d.iter().position(|&v| v > 10.9).expect("back out");
            (at + 1) as f32 * ms as f32
        };
        let (a, b) = (time(33), time(6));
        assert!((a - b).abs() < 100.0, "30 Hz {a} ms, 165 Hz {b} ms");
    }

    /// Room for some of the way but not for the distance chosen: the camera
    /// stays where the game left it (no creeping up to the wall, which a body
    /// backing toward it would pull in again and again); the control, room
    /// for all of it, goes.
    #[test]
    fn it_goes_back_only_where_the_distance_chosen_is_clear() {
        for (wall, want) in [(7.0, 3.5), (12.0, 11.0)] {
            let mut room = Room { t: 0, wall: 100.0 };
            let (mut cam, mut reach) = orbit(&mut room, 11.0);
            room.wall = 5.0;
            frames(&mut cam, Some(&mut reach), &mut room, 60, 16);
            room.wall = wall;
            frames(&mut cam, Some(&mut reach), &mut room, 200, 16);
            assert_eq!(cam.cur.dist.0, want, "a wall {wall} out");
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
}
