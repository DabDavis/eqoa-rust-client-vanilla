//! The mouse in play. The PS2 game reads none, so this is the client's own,
//! laid over the game's inputs rather than replacing any:
//!
//! - a drag (either button) looks round, as the right stick does: across, in
//!   the orbiting camera modes (1 and 3), the orbit angle itself, a pixel at
//!   a time and the view turning the way the mouse goes (`SetOrbitAngle`);
//!   in first person and the follow camera (0 and 2) the right stick's x,
//!   which turns the body. Up and down is the right stick's y: the orbit
//!   camera zooms on it, first person pitches. The options' Camera Pitch
//!   Axis turns them round as it does the stick.
//! - the wheel zooms the orbiting cameras (`SetOrbitDistance`).
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
use vi::fpu::{Ee, F};
use web_time::{Duration, Instant};

/// A press moves this far (window pixels) before it is a drag.
const SLOP: f64 = 5.0;
/// Two clicks this close in time and place are a double click.
const DOUBLE: Duration = Duration::from_millis(400);
/// The orbit's turn for a pixel of drag (radians): half a turn over 390.
const ORBIT_PER_PIXEL: f32 = 0.008;
/// A pixel of drag as the stick's push for a sixtieth of a second (a frame
/// at 60 Hz, where it was set). The push acts for the frame it is given in
/// (`UpdateControls`' thrusts are integrated over the frame), so it is
/// scaled by the frame's length: a drag turns or zooms the same at any rate.
const STICK_PER_PIXEL: f32 = 0.05;

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

    /// This frame's drag and wheel on the walker's camera (`look`), for a
    /// frame `frame_s` seconds long.
    pub fn look(&mut self, w: &mut Walker, inverted: bool, frame_s: f32) -> [f32; 2] {
        let (dx, dy) = std::mem::take(&mut self.look);
        let wheel = std::mem::take(&mut self.wheel);
        self.steps = 0.0;
        look(w, dx as f32, dy as f32, wheel, inverted, frame_s)
    }
}

/// A drag of (dx, dy) window pixels and `wheel` lines on the walker's
/// camera in a frame `frame_s` seconds long: the orbit and the distance
/// moved at once in the orbiting modes, and the rest as the right stick's
/// push (rx, ry) for `UpdateControls`.
pub fn look(w: &mut Walker, dx: f32, dy: f32, wheel: f32, inverted: bool, frame_s: f32) -> [f32; 2] {
    let mut stick = [0.0f32; 2];
    let per_pixel = stick_per_pixel(frame_s);
    let mode = w.cam.camera_mode;
    if mode == 1 || mode == 3 {
        if dx != 0.0 {
            // the view turns the way the mouse goes: the orbit the other way
            let d = dx * ORBIT_PER_PIXEL * if inverted { 1.0 } else { -1.0 };
            let a = w.cam.cur.orbit.x;
            w.cam.set_orbit_angle(a + F::<Ee>::new(d));
        }
        if wheel != 0.0 {
            let d = w.cam.cur.dist.0 * 0.9f32.powf(wheel);
            w.cam.set_orbit_distance(F::new(d));
        }
    } else {
        stick[0] = (dx * per_pixel).clamp(-1.0, 1.0);
    }
    stick[1] = (dy * per_pixel).clamp(-1.0, 1.0);
    stick
}

/// The stick's push for a pixel of drag in a frame `frame_s` seconds long
/// (`STICK_PER_PIXEL`); a frame under a millisecond counts as one.
fn stick_per_pixel(frame_s: f32) -> f32 {
    STICK_PER_PIXEL * (1.0 / 60.0) / frame_s.max(0.001)
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

    #[test]
    fn a_drag_pushes_the_stick_for_the_same_time_at_any_rate() {
        // 120 pixels a second of drag for a second: the push times the
        // frame's length, summed, is what UpdateControls integrates
        let total = |hz: f32| -> f32 {
            let frame = 1.0 / hz;
            (0..hz as usize).map(|_| (120.0 * frame * stick_per_pixel(frame)).clamp(-1.0, 1.0) * frame).sum()
        };
        let (a, b, c) = (total(30.0), total(60.0), total(165.0));
        assert!((a - b).abs() < 1e-3 && (b - c).abs() < 1e-3, "{a} {b} {c}");
        // at 60 Hz the push is what it was (0.05 a pixel)
        assert!((stick_per_pixel(1.0 / 60.0) - STICK_PER_PIXEL).abs() < 1e-6);
        // control: a push a frame not scaled shrinks with the frame rate
        let plain = |hz: f32| -> f32 { (0..hz as usize).map(|_| (120.0 / hz * STICK_PER_PIXEL).clamp(-1.0, 1.0) / hz).sum() };
        assert!(plain(165.0) < plain(60.0) * 0.5);
    }

    #[test]
    fn the_segment_distance_clamps_to_its_ends() {
        let (a, b) = (glam::Vec2::new(0.0, 0.0), glam::Vec2::new(0.0, 10.0));
        assert_eq!(to_segment(glam::Vec2::new(3.0, 5.0), a, b), 3.0);
        assert_eq!(to_segment(glam::Vec2::new(0.0, 14.0), a, b), 4.0);
        assert_eq!(to_segment(glam::Vec2::new(4.0, -3.0), a, b), 5.0);
    }
}
