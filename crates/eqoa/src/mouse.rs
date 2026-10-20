//! The mouse in play. The PS2 game reads none, so this is the client's own,
//! laid over the game's inputs rather than replacing any:
//!
//! - a drag (either button) looks round: across, in the orbiting camera
//!   modes (1 and 3), the orbit angle itself, a pixel at a time and the view
//!   turning the way the mouse goes (`SetOrbitAngle`); in first person and
//!   the follow camera (0 and 2) the right stick's x, which turns the body.
//!   Up and down tilts the orbiting camera (`tilt`: the view looks up as the
//!   mouse goes up, the camera going lower, between level-ish and nearly
//!   overhead; in mode 3 the default pitch its own snaps go back to), where
//!   the right stick's y would zoom it; in first person and the follow
//!   camera it is the stick's y, the pitch. The options' Camera Pitch Axis
//!   turns them round as it does the stick.
//! - the wheel zooms the orbiting cameras (`SetOrbitDistance`); past the
//!   closest it goes into first person (`Zoom::In`, as P's style 0), and out
//!   of first person back to the camera it came from (`Zoom::Out`,
//!   `Play::wheel_style`). The follow camera, which does not zoom, goes into
//!   first person on the wheel's first notch in.
//! - both buttons held run forward (the left stick all the way up), the
//!   way the view faces in the orbiting camera, as many PC games do; a drag
//!   meanwhile steers. Letting go after it is no click.
//! - a click (a press let go within a few pixels) targets the creature under
//!   it; a double click or a right click also talks to it, or opens its
//!   corpse (`InteractWithTarget`, or `ToggleAutoAttack`'s loot request).
//!
//! A window up takes the clicks first (`Play::point`, `pointer`): a right
//! click is △ there, and the wheel the d-pad's up and down.

use crate::walker::Walker;
use vi::camera::WalkCamera;
use vi::fpu::{Ee, F};
use web_time::{Duration, Instant};

/// A press moves this far (window pixels) before it is a drag.
const SLOP: f64 = 5.0;
/// Two clicks this close in time and place are a double click.
const DOUBLE: Duration = Duration::from_millis(400);
/// The orbit's turn for a pixel of drag (radians): half a turn over 390.
const ORBIT_PER_PIXEL: f32 = 0.008;
/// A pixel of drag as the stick's push for a frame.
const STICK_PER_PIXEL: f32 = 0.05;
/// The tilt for a pixel of drag (radians), as the orbit's turn.
const TILT_PER_PIXEL: f32 = 0.008;
/// The orbit's pitch a drag keeps to: 10 degrees below level (the camera
/// under the head, looking up at it) to 75 overhead. The game's own pitches
/// lie between: style 3's default 15, style 1's 25, the alternate near a
/// wall 60.
pub const MIN_TILT: f32 = -0.17453292;
pub const MAX_TILT: f32 = 1.3089969;

/// What the wheel asks of the camera's style, beyond the orbit's zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    None,
    /// in past the closest zoom (or in, in the follow camera): first person
    In,
    /// out, in first person: back to the camera it came from
    Out,
}

/// The wheel's `lines` (in positive) on `cam`: whether it changes the
/// camera's style rather than zooming. An orbit already at its closest goes
/// into first person; first person comes out.
pub fn zoom_step(cam: &WalkCamera<Ee>, lines: f32) -> Zoom {
    match cam.camera_mode {
        1 | 3 if lines > 0.0 && cam.cur.dist <= cam.min_orbit_distance => Zoom::In,
        2 if lines > 0.0 => Zoom::In,
        0 if lines < 0.0 => Zoom::Out,
        _ => Zoom::None,
    }
}

/// The orbit's pitch moved by `d` (radians, up positive: the camera
/// higher), kept within `MIN_TILT`..`MAX_TILT`. Mode 3 snaps its pitch back
/// to the default whenever the orbit is clear at it, and to the alternate
/// pitch near a wall (`move_camera`'s flags 8), so there the DEFAULT is what
/// moves (`SetDefOrbitPitchAngle`), and the camera with it when it rests
/// there; at the alternate pitch or on its way somewhere it is left to its
/// snaps, which go to the new default once the orbit is clear. Mode 1 has no
/// snaps: its pitch moves. The game has no tilt; this is the client's mouse.
pub fn tilt(cam: &mut WalkCamera<Ee>, d: F<Ee>) {
    let (lo, hi) = (F::new(MIN_TILT), F::new(MAX_TILT));
    match cam.camera_mode {
        3 => {
            let resting = cam.cur.flags & 8 == 0 && cam.cur.orbit.y == cam.def_orbit_pitch;
            let p = vi::fpu::clamp(cam.def_orbit_pitch + d, lo, hi);
            if resting {
                cam.set_def_orbit_pitch_angle(p);
            } else {
                cam.def_orbit_pitch = p;
            }
        }
        1 => {
            let p = vi::fpu::clamp(cam.cur.orbit.y + d, lo, hi);
            cam.set_orbit_pitch_angle(p);
        }
        _ => {}
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Click {
    /// a click or a tap: target what is under (x, y)
    Target(f64, f64),
    /// a double click or tap: target it and talk to it (or open its corpse)
    Interact(f64, f64),
    /// a right click: in play as `Interact`, on a window △
    Right(f64, f64),
}

impl Click {
    /// Where it was, window pixels.
    pub fn at(self) -> (f64, f64) {
        let (Click::Target(x, y) | Click::Interact(x, y) | Click::Right(x, y)) = self;
        (x, y)
    }
}

#[derive(Clone, Copy, Debug)]
struct Press {
    right: bool,
    at: (f64, f64),
    dragged: bool,
}

#[derive(Debug, Default)]
pub struct Mouse {
    pub cursor: (f64, f64),
    press: Option<Press>,
    /// the left and the right button held
    held: [bool; 2],
    last_click: Option<((f64, f64), Instant)>,
    /// the drag since the last frame, window pixels
    look: (f64, f64),
    /// the wheel since the last frame, in lines (up positive), and in a
    /// window's steps (a notch one)
    wheel: f32,
    steps: f32,
    /// the camera the wheel went into first person from (`Zoom`)
    pub back: Option<crate::camstyle::ZoomBack>,
}

impl Mouse {
    /// The cursor moved: a held button past the slop drags.
    pub fn moved(&mut self, x: f64, y: f64) {
        let (dx, dy) = (x - self.cursor.0, y - self.cursor.1);
        self.cursor = (x, y);
        if let Some(p) = self.press.as_mut() {
            if !p.dragged && ((x - p.at.0).powi(2) + (y - p.at.1).powi(2)).sqrt() > SLOP {
                p.dragged = true;
            }
            if p.dragged {
                self.look.0 += dx;
                self.look.1 += dy;
            }
        }
    }

    /// Whether a button is held and has moved past the slop.
    #[cfg(test)]
    pub fn dragging(&self) -> bool {
        self.press.is_some_and(|p| p.dragged)
    }

    /// A button pressed or let go; a click when it is let go undragged.
    pub fn button(&mut self, right: bool, down: bool, now: Instant) -> Option<Click> {
        self.held[right as usize] = down;
        if down {
            match self.press.as_mut() {
                None => self.press = Some(Press { right, at: self.cursor, dragged: false }),
                // the other one too: a run (`running`), no click after it
                Some(p) => p.dragged = true,
            }
            return None;
        }
        let p = self.press.filter(|p| p.right == right)?;
        self.press = None;
        if p.dragged {
            return None;
        }
        let (x, y) = p.at;
        if p.right {
            return Some(Click::Right(x, y));
        }
        let double = self.last_click.is_some_and(|(at, t)| now.duration_since(t) < DOUBLE && ((x - at.0).powi(2) + (y - at.1).powi(2)).sqrt() <= SLOP);
        if double {
            self.last_click = None;
            return Some(Click::Interact(x, y));
        }
        self.last_click = Some(((x, y), now));
        Some(Click::Target(x, y))
    }

    /// What is held, for a report: the buttons, and a press that drags.
    #[cfg(target_arch = "wasm32")]
    pub fn describe(&self) -> String {
        let held: Vec<&str> = [(self.held[0], "left"), (self.held[1], "right")].iter().filter(|h| h.0).map(|h| h.1).collect();
        match self.press {
            Some(p) => format!("held [{}], {}", held.join(", "), if p.dragged { "dragging" } else { "pressed" }),
            None if held.is_empty() => "none held".to_string(),
            None => format!("held [{}]", held.join(", ")),
        }
    }

    /// Both buttons held: the client's run forward.
    pub fn running(&self) -> bool {
        self.held[0] && self.held[1]
    }

    /// The window lost the pointer or the focus: what was held is let go
    /// (its release may never come), clicking nothing.
    pub fn release_all(&mut self) {
        self.held = [false; 2];
        self.press = None;
    }

    /// The wheel turned: `lines` for the camera's zoom, `steps` for a window
    /// up (the d-pad's up or down a step; a notch is one).
    pub fn wheel(&mut self, lines: f32, steps: f32) {
        self.wheel += lines;
        self.steps += steps;
    }

    /// The wheel's steps for a window since the last frame, taken.
    pub fn take_wheel(&mut self) -> f32 {
        std::mem::take(&mut self.steps)
    }

    /// This frame's drag and wheel let go of (a window up, the camera held).
    pub fn discard(&mut self) {
        self.look = (0.0, 0.0);
        self.wheel = 0.0;
        self.steps = 0.0;
    }

    /// This frame's drag and wheel on the walker's camera (`look`), and
    /// what the wheel asks of its style (`zoom_step`, which the caller
    /// carries out: `Play::wheel_style`); a wheel that does is no zoom.
    pub fn look(&mut self, w: &mut Walker, inverted: bool) -> ([f32; 2], Zoom) {
        let (dx, dy) = std::mem::take(&mut self.look);
        let wheel = std::mem::take(&mut self.wheel);
        self.steps = 0.0;
        let zoom = zoom_step(&w.cam, wheel);
        let wheel = if zoom == Zoom::None { wheel } else { 0.0 };
        (look(w, dx as f32, dy as f32, wheel, inverted), zoom)
    }
}

/// A drag of (dx, dy) window pixels and `wheel` lines on the walker's
/// camera (`look_cam`).
pub fn look(w: &mut Walker, dx: f32, dy: f32, wheel: f32, inverted: bool) -> [f32; 2] {
    look_cam(&mut w.cam, dx, dy, wheel, inverted)
}

/// A drag of (dx, dy) window pixels and `wheel` lines on `cam`: in the
/// orbiting modes the orbit, its tilt and its distance moved at once;
/// otherwise the right stick's push (rx, ry) for `UpdateControls`.
pub fn look_cam(cam: &mut WalkCamera<Ee>, dx: f32, dy: f32, wheel: f32, inverted: bool) -> [f32; 2] {
    let mut stick = [0.0f32; 2];
    let mode = cam.camera_mode;
    if mode == 1 || mode == 3 {
        if dx != 0.0 {
            // the view turns the way the mouse goes: the orbit the other way
            let d = dx * ORBIT_PER_PIXEL * if inverted { 1.0 } else { -1.0 };
            let a = cam.cur.orbit.x;
            cam.set_orbit_angle(a + F::<Ee>::new(d));
        }
        if dy != 0.0 {
            // the view looks the way the mouse goes: up, the camera lower
            tilt(cam, F::new(dy * TILT_PER_PIXEL * if inverted { -1.0 } else { 1.0 }));
        }
        if wheel != 0.0 {
            let d = cam.cur.dist.0 * 0.9f32.powf(wheel);
            cam.set_orbit_distance(F::new(d));
        }
    } else {
        stick[0] = (dx * STICK_PER_PIXEL).clamp(-1.0, 1.0);
        stick[1] = (dy * STICK_PER_PIXEL).clamp(-1.0, 1.0);
    }
    stick
}

/// `EQOA_MOUSE`, for headless runs: `;`-separated steps at a time in ms,
/// `click <x> <y>@<ms>`, `rclick <x> <y>@<ms>`, `drag <dx> <dy>@<ms>` (from
/// the window's middle), `wheel <lines>@<ms>` and `press l|r@<ms>` /
/// `release l|r@<ms>`, played through a `Mouse` as the window's events
/// would be.
pub fn script() -> Vec<(i64, String)> {
    let Ok(s) = std::env::var("EQOA_MOUSE") else { return Vec::new() };
    s.split(';')
        .filter_map(|p| {
            let (what, ms) = p.trim().rsplit_once('@')?;
            Some((ms.trim().parse::<i64>().ok()? / 16 * 16, what.trim().to_string()))
        })
        .collect()
}

impl Mouse {
    /// One script step on a `size` window: the click it makes, if any.
    pub fn play(&mut self, step: &str, size: (u32, u32), now: Instant) -> Option<Click> {
        let w: Vec<&str> = step.split_whitespace().collect();
        let num = |i: usize| w.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
        let mid = (size.0 as f64 / 2.0, size.1 as f64 / 2.0);
        match w.first().copied() {
            Some("click") | Some("rclick") => {
                let right = w[0] == "rclick";
                self.moved(num(1), num(2));
                self.button(right, true, now);
                self.button(right, false, now)
            }
            Some("drag") => {
                self.moved(mid.0, mid.1);
                self.button(false, true, now);
                self.moved(mid.0 + num(1), mid.1 + num(2));
                self.button(false, false, now)
            }
            Some("wheel") => {
                self.wheel(num(1) as f32, num(1) as f32);
                None
            }
            // `press l|r` and `release l|r`: a button held across steps
            Some(s @ ("press" | "release")) => self.button(w.get(1) == Some(&"r"), s == "press", now),
            _ => None,
        }
    }
}

/// The distance from `p` to the segment `a`..`b`.
pub fn to_segment(p: glam::Vec2, a: glam::Vec2, b: glam::Vec2) -> f32 {
    let ab = b - a;
    let t = if ab.length_squared() > 0.0 { ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
    (a + ab * t - p).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_still_press_clicks_a_moved_one_drags() {
        let t0 = Instant::now();
        let mut m = Mouse::default();
        m.moved(100.0, 100.0);
        assert_eq!(m.button(false, true, t0), None);
        m.moved(103.0, 101.0);
        assert!(!m.dragging(), "within the slop");
        assert_eq!(m.button(false, false, t0), Some(Click::Target(100.0, 100.0)));
        // the second click soon after, near: a double click
        m.button(false, true, t0 + Duration::from_millis(150));
        assert_eq!(m.button(false, false, t0 + Duration::from_millis(200)), Some(Click::Interact(103.0, 101.0)));
        // a drag is no click, and it is looked with
        m.button(false, true, t0 + Duration::from_millis(2000));
        m.moved(140.0, 90.0);
        assert!(m.dragging());
        assert_eq!(m.button(false, false, t0 + Duration::from_millis(2100)), None);
        assert_eq!(m.look, (37.0, -11.0), "the move that passes the slop counts whole");
        // a right click, at once
        m.button(true, true, t0 + Duration::from_millis(3000));
        assert_eq!(m.button(true, false, t0 + Duration::from_millis(3050)), Some(Click::Right(140.0, 90.0)));
        // a slow second click is a click again
        m.button(false, true, t0 + Duration::from_millis(4000));
        m.button(false, false, t0 + Duration::from_millis(4010));
        m.button(false, true, t0 + Duration::from_millis(4600));
        assert_eq!(m.button(false, false, t0 + Duration::from_millis(4610)), Some(Click::Target(140.0, 90.0)));
    }

    /// Both buttons run while both are held, either order, and letting go
    /// clicks nothing; one button alone neither runs nor stops clicking (the
    /// control), and a lost focus lets go of a run.
    #[test]
    fn both_buttons_run_and_click_nothing() {
        let t0 = Instant::now();
        let mut m = Mouse::default();
        m.moved(100.0, 100.0);
        m.button(false, true, t0);
        assert!(!m.running(), "one button");
        m.button(true, true, t0);
        assert!(m.running());
        m.moved(130.0, 100.0);
        assert_eq!(m.look, (30.0, 0.0), "a drag while running steers");
        assert_eq!(m.button(false, false, t0), None, "no click");
        assert!(!m.running());
        assert_eq!(m.button(true, false, t0), None, "no right click");
        m.button(true, true, t0 + Duration::from_millis(1000));
        m.button(false, true, t0 + Duration::from_millis(1000));
        assert!(m.running(), "right first");
        m.release_all();
        assert!(!m.running(), "a lost focus lets go");
        assert_eq!(m.button(true, false, t0 + Duration::from_millis(1100)), None);
        // the control: one button is a click as before
        m.button(false, true, t0 + Duration::from_millis(3000));
        assert_eq!(m.button(false, false, t0 + Duration::from_millis(3010)), Some(Click::Target(130.0, 100.0)));
    }

    /// Open ground: nothing to hit, no liquid.
    struct Open;

    impl vi::scene::Scene<Ee> for Open {
        fn time(&self) -> i64 {
            0
        }
        fn collide_sphere(&mut self, _: &mut vi::coll::CollSphere<Ee>, _: &mut vi::scene::Filter) -> bool {
            false
        }
        fn collide_env(&mut self, _: &mut vi::coll::EnvRay<Ee>) -> bool {
            false
        }
        fn env_flags(&self, _: &vi::vect::Vect3<Ee>) -> u32 {
            0
        }
    }

    /// The play camera, as the first world entry leaves it: style 3, the
    /// follow orbit.
    fn follow() -> WalkCamera<Ee> {
        let mut cam = WalkCamera::<Ee>::default();
        cam.init(vi::vect::Vect3::of(100.0, 0.0, 100.0), vi::vect::Vect3::of(0.5, 0.0, 0.0), &mut Open);
        crate::camstyle::set_style(&mut cam, 3, None, &mut Open);
        cam
    }

    /// A drag up and down tilts the follow orbit (the default pitch, and the
    /// camera resting there with it), within its range, the option turning
    /// it round; it no longer zooms (no stick y). In first person the drag
    /// is still the stick's y, the pitch (the control).
    #[test]
    fn a_drag_up_and_down_tilts_the_orbit() {
        let mut cam = follow();
        let (def, dist) = (cam.def_orbit_pitch.0, cam.cur.dist.0);
        assert_eq!(def, 0.261_799_4, "style 3's default pitch");
        // up 50 pixels: the view looks up, the camera 0.4 lower
        let stick = look_cam(&mut cam, 0.0, -50.0, 0.0, false);
        assert_eq!(stick, [0.0, 0.0], "no zoom on the stick");
        assert_eq!(cam.def_orbit_pitch, F::new(def) + F::new(-0.4));
        assert_eq!(cam.cur.orbit.y, cam.def_orbit_pitch, "resting there, the camera with it");
        assert_eq!(cam.last.orbit.y, cam.def_orbit_pitch);
        assert_eq!(cam.cur.dist.0, dist, "the distance as it was");
        // past the bottom of the range, then the option turning it round
        look_cam(&mut cam, 0.0, -500.0, 0.0, false);
        assert_eq!(cam.def_orbit_pitch.0, MIN_TILT);
        look_cam(&mut cam, 0.0, -500.0, 0.0, true);
        assert_eq!(cam.def_orbit_pitch.0, MAX_TILT, "inverted: up raises it, to the top");
        // on its way to the alternate pitch (a wall), only the default moves
        cam.cur.flags |= 8;
        cam.cur.orbit.y = F::new(1.0);
        look_cam(&mut cam, 0.0, 100.0, 0.0, false);
        assert_eq!(cam.def_orbit_pitch, vi::fpu::clamp(F::new(MAX_TILT) + F::new(0.8), F::new(MIN_TILT), F::new(MAX_TILT)));
        assert_eq!(cam.cur.orbit.y.0, 1.0, "left to its snap");
        // first person: the stick's y
        crate::camstyle::set_style(&mut cam, 0, None, &mut Open);
        let pitch = cam.def_orbit_pitch;
        let stick = look_cam(&mut cam, 0.0, -10.0, 0.0, false);
        assert_eq!(stick, [0.0, -0.5]);
        assert_eq!(cam.def_orbit_pitch, pitch);
    }

    /// The wheel zooms the orbit until its closest; one more notch in is
    /// first person, and out of first person it is the orbit again at the
    /// distance and tilt it left, the follow camera's ceiling the client's
    /// 11.
    #[test]
    fn the_wheel_goes_into_first_person_and_back() {
        let mut cam = follow();
        assert_eq!(cam.max_orbit_distance.0, crate::camstyle::FOLLOW_MAX_DISTANCE);
        // out all the way: 11, not the game's 8
        look_cam(&mut cam, 0.0, 0.0, -40.0, false);
        assert_eq!(cam.cur.dist.0, 11.0);
        assert_eq!(zoom_step(&cam, 1.0), Zoom::None, "far out: a notch in zooms");
        // in all the way: 1.5 is the closest; there a notch in is first person
        look_cam(&mut cam, 0.0, 0.0, 40.0, false);
        assert_eq!(cam.cur.dist.0, 1.5);
        assert_eq!(zoom_step(&cam, -1.0), Zoom::None, "out zooms");
        assert_eq!(zoom_step(&cam, 1.0), Zoom::In);
        look_cam(&mut cam, 0.0, -25.0, 0.0, false);
        let tilt = cam.def_orbit_pitch;
        let back = crate::camstyle::zoom_in(&mut cam, None, &mut Open);
        assert_eq!(cam.camera_mode, 0, "first person");
        assert_eq!(zoom_step(&cam, 1.0), Zoom::None, "in, in first person: nothing");
        assert_eq!(zoom_step(&cam, -1.0), Zoom::Out);
        crate::camstyle::zoom_out(&mut cam, &back, None, &mut Open);
        assert_eq!((cam.camera_mode, cam.cur.dist.0), (3, 1.5), "the orbit at its closest again");
        assert_eq!(cam.def_orbit_pitch, tilt, "the tilt kept");
        assert_eq!(cam.cur.orbit.y, tilt);
        // the follow camera does not zoom: in is first person at once
        crate::camstyle::set_style(&mut cam, 2, None, &mut Open);
        assert_eq!(zoom_step(&cam, 0.5), Zoom::In);
        assert_eq!(zoom_step(&cam, -0.5), Zoom::None);
    }

    /// A wall `.1` behind the head at time `.0`: any sweep reaching further
    /// is stopped.
    struct Wall(i64, f32);

    impl vi::scene::Scene<Ee> for Wall {
        fn time(&self) -> i64 {
            self.0
        }
        fn collide_sphere(&mut self, s: &mut vi::coll::CollSphere<Ee>, _: &mut vi::scene::Filter) -> bool {
            let (a, b) = (s.ray.start, s.ray.end);
            let (dx, dz) = (b.x.0 - a.x.0, b.z.0 - a.z.0);
            dx * dx + dz * dz > self.1 * self.1
        }
        fn collide_env(&mut self, _: &mut vi::coll::EnvRay<Ee>) -> bool {
            false
        }
        fn env_flags(&self, _: &vi::vect::Vect3<Ee>) -> u32 {
            0
        }
    }

    /// The client's longer reach does not go through walls: zoomed out to
    /// 11, the orbit blocked past 5 comes in to the game's
    /// `min_collision_dist` (3.5) as it did from 8; in the open it stays
    /// out (the control).
    #[test]
    fn walls_still_stop_the_longer_orbit() {
        for (reach, want) in [(5.0, 3.5), (100.0, 11.0)] {
            let mut cam = follow();
            look_cam(&mut cam, 0.0, 0.0, -40.0, false);
            assert_eq!(cam.cur.dist.0, 11.0);
            for t in 1..=200 {
                cam.process(&mut Wall(t * 16, reach));
            }
            assert_eq!(cam.cur.dist.0, want, "a wall {reach} behind");
        }
    }

    #[test]
    fn the_segment_distance_clamps_to_its_ends() {
        let (a, b) = (glam::Vec2::new(0.0, 0.0), glam::Vec2::new(0.0, 10.0));
        assert_eq!(to_segment(glam::Vec2::new(3.0, 5.0), a, b), 3.0);
        assert_eq!(to_segment(glam::Vec2::new(0.0, 14.0), a, b), 4.0);
        assert_eq!(to_segment(glam::Vec2::new(4.0, -3.0), a, b), 5.0);
    }
}
