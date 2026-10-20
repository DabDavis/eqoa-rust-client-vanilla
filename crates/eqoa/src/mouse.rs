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
/// A pixel of drag as the stick's push for a frame.
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
        if down {
            if self.press.is_none() {
                self.press = Some(Press { right, at: self.cursor, dragged: false });
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

    /// This frame's drag and wheel on the walker's camera (`look`).
    pub fn look(&mut self, w: &mut Walker, inverted: bool) -> [f32; 2] {
        let (dx, dy) = std::mem::take(&mut self.look);
        let wheel = std::mem::take(&mut self.wheel);
        self.steps = 0.0;
        look(w, dx as f32, dy as f32, wheel, inverted)
    }
}

/// A drag of (dx, dy) window pixels and `wheel` lines on the walker's
/// camera: the orbit and the distance moved at once in the orbiting modes,
/// and the rest as the right stick's push (rx, ry) for `UpdateControls`.
pub fn look(w: &mut Walker, dx: f32, dy: f32, wheel: f32, inverted: bool) -> [f32; 2] {
    let mut stick = [0.0f32; 2];
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
        stick[0] = (dx * STICK_PER_PIXEL).clamp(-1.0, 1.0);
    }
    stick[1] = (dy * STICK_PER_PIXEL).clamp(-1.0, 1.0);
    stick
}

/// `EQOA_MOUSE`, for headless runs: `;`-separated steps at a time in ms,
/// `click <x> <y>@<ms>`, `rclick <x> <y>@<ms>`, `drag <dx> <dy>@<ms>` (from
/// the window's middle) and `wheel <lines>@<ms>`, played through a `Mouse`
/// as the window's events would be.
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

    #[test]
    fn the_segment_distance_clamps_to_its_ends() {
        let (a, b) = (glam::Vec2::new(0.0, 0.0), glam::Vec2::new(0.0, 10.0));
        assert_eq!(to_segment(glam::Vec2::new(3.0, 5.0), a, b), 3.0);
        assert_eq!(to_segment(glam::Vec2::new(0.0, 14.0), a, b), 4.0);
        assert_eq!(to_segment(glam::Vec2::new(4.0, -3.0), a, b), 5.0);
    }
}
