//! The mouse in play. The PS2 game reads none, so this is the client's own,
//! laid over the game's inputs rather than replacing any. It works as World
//! of Warcraft's does (the user's choice, F3 reports of 2026-10-21):
//!
//! - a held button that drags takes the pointer (`Grab`, `take_pointer`):
//!   the cursor hidden and held (a page's Pointer Lock, asked for in the
//!   drag's move within the press's activation, so a click never hides the
//!   cursor, and in the press itself once the browser has refused that twice
//!   running, `lock_seen`; natively winit's `Locked`), and the look rides the
//!   mouse's own motion (`motion`, winit's `DeviceEvent::MouseMotion`), so no
//!   screen edge stops it. Where the platform has no lock (X11, Windows) the
//!   pointer is `Confined` to the window instead, the cursor's own moves are
//!   the look and it is put back to the window's middle as it nears an edge
//!   (`recentre`): under X11's grab every raw motion comes twice (SDL drops
//!   the second by its X time, which winit does not give). Let go, the
//!   cursor is back where the press was. A window or the start menu
//!   opening, the window losing the focus, Escape (which a browser keeps to
//!   end its lock) and a death let go of it too, clicking nothing.
//! - the left button looks round the orbiting cameras (1 and 3) without
//!   turning the body: across, the orbit angle (`SetOrbitAngle`, the view
//!   turning the way the mouse goes); up and down the tilt (`tilt`: the view
//!   looks up as the mouse goes up, the camera lower, from 45 degrees under
//!   the head, where the ground pulls it in as anything the game's camera
//!   meets does, to 75 overhead; in mode 3 it is the default pitch the
//!   camera's own snaps go back to). Below the game's own 15 degrees
//!   (`camreach::GAME_PITCH`) the ground behind meets the camera on any
//!   slope, and where the game would then swing it round to a clear side
//!   (which turns a run, since the stick and autorun go the way the camera
//!   looks) it comes in to its closest instead, as when steered.
//! - the right button steers (`look_held`, `Mouse::steer`): in the follow
//!   orbit (3) the body and the camera behind it turn together, the tilt as
//!   the left button's, and the walk is on the body's own axes
//!   (`controls::Client::steer`): W and S forward and back, A and D a
//!   sidestep, the body facing the way the camera looks (turned there at
//!   once as the hold begins) and the camera put back behind it every frame
//!   (the game re-derives the orbit from where the camera was, so a sidestep
//!   would drag it round); where the game would swing the camera round to a
//!   clear angle it comes in to its closest instead.
//! - both buttons run forward (the left stick all the way up), steered as
//!   the right button steers; A and D add a sidestep, a diagonal.
//! - in first person and the follow camera (0 and 2) either button turns the
//!   body at once (`set_heading`: full circles) and pitches it (`pitch_body`)
//!   within the walk's range: in first person 89.4 degrees up or down, where
//!   the game's own stops short of straight up and down. At a liquid's
//!   surface the game's pitch is a dive (`PitchDown`), so there the motion
//!   is the right stick's, as before. The Camera Pitch Axis option turns the
//!   pitch round, and in the orbits the orbit too, as it turns the stick's.
//! - the wheel zooms the follow orbit (`zoom`): in from where the camera is
//!   (a wall's pull included), out from the distance chosen, which
//!   `camreach` keeps and eases the camera back out to after something
//!   pulled it in; past the closest it goes into first person (`Zoom::In`,
//!   as P's style 0), and out of first person back to the camera it came
//!   from (`Zoom::Out`, `Play::wheel_style`). The follow camera, which does
//!   not zoom, goes into first person on the wheel's first notch in.
//! - a click (a press let go unmoved, within a few pixels) targets the
//!   creature under it; a double click or a right click also talks to it,
//!   or opens its corpse (`InteractWithTarget`, or `ToggleAutoAttack`'s loot
//!   request).
//!
//! With no button held the keys are what they were (WASD, the arrows, I J K
//! L). A window up takes the clicks first (`Play::point`, `pointer`): a right
//! click is △ there, and the wheel the d-pad's up and down. A finger on the
//! touch screen looks as a drag did before (`look`, `look_cam`).

use crate::walker::Walker;
use vi::camera::WalkCamera;
use vi::fpu::{Ee, F};
use web_time::{Duration, Instant};

/// A press moves this far (window pixels) before it is a drag.
const SLOP: f64 = 5.0;
/// Two clicks this close in time and place are a double click.
const DOUBLE: Duration = Duration::from_millis(400);
/// A page's lock not granted this long after it was asked for: refused.
const REFUSED: Duration = Duration::from_millis(500);
/// A confined cursor's move this near the window's middle, the middle asked
/// for, is the window's own move back there (and whatever came with it).
const WARP_NEAR: f64 = 24.0;
/// The look's turn for a pixel of motion (radians): half a turn over 390.
/// The orbit, the steered body and first person turn alike.
const LOOK_PER_PIXEL: f32 = 0.008;
/// A pixel of drag as the stick's push for a sixtieth of a second (a frame
/// at 60 Hz, where it was set). The push acts for the frame it is given in
/// (`UpdateControls`' thrusts are integrated over the frame), so it is
/// scaled by the frame's length: a drag turns or zooms the same at any rate.
const STICK_PER_PIXEL: f32 = 0.05;
/// The tilt for a pixel of motion (radians), as the orbit's turn.
const TILT_PER_PIXEL: f32 = 0.008;
/// The orbit's pitch the look keeps to: 45 degrees below level (the camera
/// under the head, looking up past it at the sky; the ground pulls it in,
/// as the game's sweep pulls it in from anything it meets: `MoveCamera`'s
/// flag 4, then its closest) to 75 overhead. The game's own pitches lie
/// between: style 3's default 15, style 1's 25, the alternate near a wall
/// 60. Lower, the camera at its closest (1.5 from a head 1.68 up) would
/// meet flat ground with its 0.5 sphere: past 52 degrees.
pub const MIN_TILT: f32 = -std::f32::consts::FRAC_PI_4;
pub const MAX_TILT: f32 = 1.3089969;
/// The walk camera's pi and two pi (`viwalkcamera.cpp`'s literals).
const PI: f32 = std::f32::consts::PI;
const TWO_PI: f32 = std::f32::consts::TAU;

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

/// The body's heading set at once, the client's own (the game turns it
/// only by torque, at most a quarter turn a second in first person): the
/// working and committed states (wrapped as `UpdatePhysics` wraps it), the
/// heading mode 3 tracks, no turn left coasting, and the walk's axes
/// (`UpdateDirections`), which this frame's thrust is along.
pub fn set_heading(cam: &mut WalkCamera<Ee>, h: F<Ee>) {
    let h = h.fmodf(F::new(TWO_PI));
    let w = &mut cam.walk;
    for p in [&mut w.phys, &mut w.committed] {
        p.angles.x = h;
        p.target_heading = h;
        p.angular_vel.x = F::ZERO;
    }
    w.update_directions();
}

/// The body's pitch moved by `d` at once (down positive), within the walk's
/// range for the camera (`SetPitchRange`: first person's stops at
/// `PITCH_LIMIT`, 89.4 degrees, never straight up or down), the client's
/// own as `set_heading` is.
pub fn pitch_body(cam: &mut WalkCamera<Ee>, d: F<Ee>) {
    let w = &mut cam.walk;
    let p = vi::fpu::clamp(w.phys.angles.y + d, w.min_pitch, w.max_pitch);
    for s in [&mut w.phys, &mut w.committed] {
        s.angles.y = p;
        s.angular_vel.y = F::ZERO;
    }
    w.update_directions();
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

/// A gesture: from a button's press until no button is held.
#[derive(Clone, Copy, Debug)]
struct Press {
    right: bool,
    at: (f64, f64),
    dragged: bool,
    /// the mouse's own motion since the press: past the slop it drags too
    /// (a pointer held from the press keeps the cursor still)
    travel: (f64, f64),
}

/// The pointer as the look holds it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Grab {
    /// the window's: the cursor shown, its moves a drag's look
    #[default]
    Free,
    /// asked of the window (a page's lock comes a moment later, if it
    /// comes): the cursor hidden, the mouse's own motion the look
    Asked,
    /// held: the cursor hidden and kept where it is
    Held,
    /// kept in the window, hidden (natively, where there is no lock): the
    /// cursor's moves are the look, the cursor put back to the middle
    Confined,
    /// the window would not: the drag goes on with the cursor, as before
    Refused,
}

#[derive(Debug, Default)]
pub struct Mouse {
    pub cursor: (f64, f64),
    press: Option<Press>,
    /// the left and the right button held
    held: [bool; 2],
    last_click: Option<((f64, f64), Instant)>,
    /// the look since the last frame (pixels of motion)
    look: (f64, f64),
    /// the wheel since the last frame, in lines (up positive), and in a
    /// window's steps (a notch one)
    wheel: f32,
    steps: f32,
    /// the camera the wheel went into first person from (`Zoom`)
    pub back: Option<crate::camstyle::ZoomBack>,
    grab: Grab,
    /// where the cursor goes back to when the grab ends: the press's place
    home: (f64, f64),
    /// the platform gives the mouse's own motion (some has come): it is
    /// the look, the cursor's moves only find the drag (a page hands both
    /// for one move, the canvas's first); until it does, the cursor's moves
    /// are the look
    raw: bool,
    /// a page's lock: when this grab asked for it, whether it was found
    /// refused, how many drags running were refused, and whether it is
    /// now asked for in the press (`lock_seen`)
    asked_at: Option<Instant>,
    counted: bool,
    refused: u8,
    at_press: bool,
    /// confined: the middle the cursor was put back to, its move not yet
    /// seen, and the moves seen since (`recentre`)
    warp: Option<(f64, f64)>,
    warp_age: u8,
    /// the last frame steered the follow orbit (a new hold turns the body
    /// to the view first)
    steered: bool,
}

impl Mouse {
    /// The cursor moved: a held button past the slop drags. Where the
    /// platform gives the mouse's own motion that is the look, not the
    /// cursor's moves (held, the cursor stays put or keeps in the window).
    pub fn moved(&mut self, x: f64, y: f64) {
        let mut from = self.cursor;
        if let Some(w) = self.warp {
            self.warp_age = self.warp_age.saturating_add(1);
            if (x - w.0).abs() <= WARP_NEAR && (y - w.1).abs() <= WARP_NEAR {
                // the window's move back to the middle: only what came with
                // it (a move the platform folded into it) is looked with
                from = w;
                self.warp = None;
            } else if self.warp_age > 8 {
                self.warp = None;
            }
        }
        let (dx, dy) = (x - from.0, y - from.1);
        self.cursor = (x, y);
        let looks = !self.raw || self.grab == Grab::Confined;
        if let Some(p) = self.press.as_mut() {
            if !p.dragged && ((x - p.at.0).powi(2) + (y - p.at.1).powi(2)).sqrt() > SLOP {
                p.dragged = true;
            }
            if p.dragged && looks {
                self.look.0 += dx;
                self.look.1 += dy;
            }
        }
    }

    /// Confined: the cursor gone a quarter of the window (`size`) from its
    /// middle, the middle it is to be put back to (the window's
    /// `set_cursor_position`), or else a confined cursor would stop at the
    /// edge; the move back is no look (`moved`).
    pub fn recentre(&mut self, size: (u32, u32)) -> Option<(f64, f64)> {
        let (w, h) = (size.0 as f64, size.1 as f64);
        let c = (w / 2.0, h / 2.0);
        let far = (self.cursor.0 - c.0).abs() > w / 4.0 || (self.cursor.1 - c.1).abs() > h / 4.0;
        if self.grab != Grab::Confined || self.warp.is_some() || !far {
            return None;
        }
        self.warp = Some(c);
        self.warp_age = 0;
        Some(c)
    }

    /// The mouse's own motion (winit's `DeviceEvent::MouseMotion`: a page's
    /// `movementX`, natively the device's counts): a drag's look, whatever
    /// the screen's edges once the pointer is held; past the slop from the
    /// press it drags as the cursor's moves do (a pointer taken at the press
    /// keeps the cursor still).
    pub fn motion(&mut self, dx: f64, dy: f64) {
        self.raw = true;
        if self.grab == Grab::Confined {
            return;
        }
        let Some(p) = self.press.as_mut() else { return };
        p.travel.0 += dx;
        p.travel.1 += dy;
        if !p.dragged && (p.travel.0.powi(2) + p.travel.1.powi(2)).sqrt() > SLOP {
            p.dragged = true;
        }
        if p.dragged {
            self.look.0 += dx;
            self.look.1 += dy;
        }
    }

    /// Whether a button is held and has moved past the slop.
    #[cfg(test)]
    pub fn dragging(&self) -> bool {
        self.press.is_some_and(|p| p.dragged)
    }

    /// A button pressed or let go; a click when the gesture ends (no button
    /// held) undragged. The other button pressed meanwhile makes it no click
    /// (both: a run), and letting go of one of two goes on with the other.
    pub fn button(&mut self, right: bool, down: bool, now: Instant) -> Option<Click> {
        self.held[right as usize] = down;
        if down {
            match self.press.as_mut() {
                None => self.press = Some(Press { right, at: self.cursor, dragged: false, travel: (0.0, 0.0) }),
                Some(p) => p.dragged = true,
            }
            return None;
        }
        if self.held[0] || self.held[1] {
            return None;
        }
        let p = self.press.take().filter(|p| p.right == right)?;
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

    /// What is held, for a report: the buttons, a press that drags, the
    /// pointer's grab.
    pub fn describe(&self) -> String {
        let held: Vec<&str> = [(self.held[0], "left"), (self.held[1], "right")].iter().filter(|h| h.0).map(|h| h.1).collect();
        let grab = match self.grab {
            Grab::Free => String::new(),
            g => format!(", pointer {g:?}"),
        };
        match self.press {
            Some(p) => format!("held [{}], {}{grab}", held.join(", "), if p.dragged { "dragging" } else { "pressed" }),
            None if held.is_empty() => format!("none held{grab}"),
            None => format!("held [{}]{grab}", held.join(", ")),
        }
    }

    /// Both buttons held: the client's run forward.
    pub fn running(&self) -> bool {
        self.held[0] && self.held[1]
    }

    /// Whether the mouse steers the body in camera `mode` this frame
    /// (`controls::Client::steer`): the right button in the follow orbit,
    /// either in first person and the follow camera, which it turns itself.
    pub fn steer(&self, mode: i32) -> bool {
        match mode {
            3 => self.held[1],
            0 | 2 => self.held[0] || self.held[1],
            _ => false,
        }
    }

    /// The window lost the pointer or the focus, or the look was taken
    /// (`grab_change`): what was held is let go (its release may never
    /// come), clicking nothing.
    pub fn release_all(&mut self) {
        self.held = [false; 2];
        self.press = None;
    }

    /// The window's answer (`take_pointer`), or a page's lock seen granted.
    pub fn set_grab(&mut self, g: Grab) {
        self.grab = g;
    }

    /// Where the cursor goes back to when the grab ends.
    pub fn home(&self) -> (f64, f64) {
        self.home
    }

    /// What the window is to do with the pointer, `free` meaning the look
    /// may go on (in play, no window up, alive): `Some(true)` take it (a
    /// drag began; in a page whose browser refuses a lock asked for in a
    /// move, a press), `Some(false)` give it back (the drag ended; or a
    /// window, the start menu or a death took the look, which ends that
    /// gesture clicking nothing).
    pub fn grab_change(&mut self, free: bool) -> Option<bool> {
        if !free && self.grab != Grab::Free {
            self.release_all();
        }
        let want = free && self.press.is_some_and(|p| p.dragged || self.at_press);
        match (want, self.grab) {
            (true, Grab::Free) => {
                self.home = self.press.map_or(self.cursor, |p| p.at);
                Some(true)
            }
            (false, Grab::Asked | Grab::Held | Grab::Confined) => {
                self.grab = Grab::Free;
                self.asked_at = None;
                self.counted = false;
                self.warp = None;
                Some(false)
            }
            (false, Grab::Refused) => {
                self.grab = Grab::Free;
                None
            }
            _ => None,
        }
    }

    /// A page's lock as the browser has it now (once a frame, at `now`):
    /// an asked grab granted is held; held and gone (Escape, or the
    /// browser's own reasons), the look ends, clicking nothing. Asked and
    /// not granted within `REFUSED` the drag goes on with the cursor; two
    /// drags running refused (a browser that grants a lock only in the
    /// press's own event, which the drag's move is not), and the lock is
    /// asked for in the press from then on (`grab_change`). True when the
    /// page holds a lock nothing wants (granted after its drag had ended),
    /// which is to be let go.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn lock_seen(&mut self, locked: bool, now: Instant) -> bool {
        match self.grab {
            Grab::Asked if locked => {
                self.grab = Grab::Held;
                self.refused = 0;
            }
            Grab::Asked => {
                let t = *self.asked_at.get_or_insert(now);
                if !self.counted && now.duration_since(t) > REFUSED {
                    self.counted = true;
                    self.refused = self.refused.saturating_add(1);
                    self.at_press |= self.refused >= 2;
                }
            }
            Grab::Held if !locked => self.release_all(),
            Grab::Free | Grab::Refused => return locked,
            _ => {}
        }
        false
    }

    /// Escape while the look holds the pointer: let go of it (as a browser
    /// ends its lock on Escape), the key taken. False otherwise.
    pub fn escape(&mut self) -> bool {
        let held = matches!(self.grab, Grab::Asked | Grab::Held | Grab::Confined);
        if held {
            self.release_all();
        }
        held
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

    /// This frame's drag, the wheel let go of (the manual camera's look,
    /// `aim`).
    pub fn take_look(&mut self) -> (f64, f64) {
        let look = std::mem::take(&mut self.look);
        self.discard();
        look
    }

    /// This frame's drag and wheel let go of (a window up, the camera held).
    pub fn discard(&mut self) {
        self.look = (0.0, 0.0);
        self.wheel = 0.0;
        self.steps = 0.0;
        self.steered = false;
    }

    /// This frame's look and wheel on the walker (`look_held`, `zoom`), for a
    /// frame `frame_s` seconds long: the right stick's push it leaves to
    /// `UpdateControls` (first person's pitch at a liquid's surface), and
    /// what the wheel asks of the camera's style (`zoom_step`, which the
    /// caller carries out: `Play::wheel_style`); a wheel that does is no
    /// zoom.
    pub fn look(&mut self, w: &mut Walker, inverted: bool, frame_s: f32) -> ([f32; 2], Zoom) {
        let (dx, dy) = std::mem::take(&mut self.look);
        let wheel = std::mem::take(&mut self.wheel);
        self.steps = 0.0;
        let zoom = zoom_step(&w.cam, wheel);
        let stick = look_held(&mut w.cam, self.held, dx as f32, dy as f32, inverted, frame_s, self.steered);
        self.steered = self.held[1] && w.cam.camera_mode == 3;
        if zoom == Zoom::None && wheel != 0.0 {
            self::zoom(&mut w.cam, &mut w.reach, wheel);
        }
        (stick, zoom)
    }
}

/// An angle snap the game began (`MoveCamera`'s `FindOrbitAngle` found a
/// clear angle: flag 1) ends where it is, and the camera comes in to its
/// closest instead, as when `FindOrbitAngle` finds none (flag 2): the
/// client's own, where swinging round would turn the player with it (the
/// right button's steer, and a tilt under the game's pitch:
/// `camreach::Reach::process`).
pub fn come_in_instead(cam: &mut WalkCamera<Ee>) {
    if cam.cur.flags & 1 != 0 {
        cam.cur.snap_angle = cam.cur.orbit.x;
        cam.cur.flags |= 2;
        cam.last.snap_angle = cam.cur.snap_angle;
        cam.last.flags = cam.cur.flags;
    }
}

/// The mouse's look this frame, (dx, dy) pixels of its motion with `held`
/// (left, right) down, on `cam` (the module's doc): the follow orbit
/// steered by the right button (`steering`: the last frame steered too, so
/// the body need not be turned to the view first), the orbits looked round
/// by the left, first person and the follow camera turned and pitched by
/// either. The right stick's push it leaves to `UpdateControls`.
pub fn look_held(cam: &mut WalkCamera<Ee>, held: [bool; 2], dx: f32, dy: f32, inverted: bool, frame_s: f32, steering: bool) -> [f32; 2] {
    let mut stick = [0.0f32; 2];
    let along = |d: f32| F::<Ee>::new(d * LOOK_PER_PIXEL * if inverted { 1.0 } else { -1.0 });
    let up = |d: f32| F::<Ee>::new(d * TILT_PER_PIXEL * if inverted { -1.0 } else { 1.0 });
    match cam.camera_mode {
        3 if held[1] => {
            // the body turned to the view as the hold begins, then by the
            // mouse; the camera put back behind it
            let from = if steering { cam.walk.phys.angles.x } else { cam.cur.orbit.x + F::new(PI) };
            set_heading(cam, from + along(dx));
            let behind = cam.walk.phys.angles.x + F::new(PI);
            cam.set_orbit_angle(behind);
            // an angle snap would swing the camera off the body
            come_in_instead(cam);
            if dy != 0.0 {
                tilt(cam, up(dy));
            }
        }
        1 | 3 => {
            if dx != 0.0 {
                // the view turns the way the mouse goes: the orbit the other way
                let a = cam.cur.orbit.x;
                cam.set_orbit_angle(a + along(dx));
            }
            if dy != 0.0 {
                // the view looks the way the mouse goes: up, the camera lower
                tilt(cam, up(dy));
            }
        }
        0 | 2 if held[0] || held[1] => {
            if dx != 0.0 {
                let h = cam.walk.phys.angles.x;
                set_heading(cam, h + F::new(-dx * LOOK_PER_PIXEL));
            }
            if dy != 0.0 {
                if cam.walk.is_at_liquid_surface() {
                    // a dive: the stick's (`UpdateControls` turns it round)
                    stick[1] = (dy * stick_per_pixel(frame_s)).clamp(-1.0, 1.0);
                } else {
                    pitch_body(cam, F::new(dy * LOOK_PER_PIXEL * if inverted { -1.0 } else { 1.0 }));
                }
            }
        }
        _ => {}
    }
    stick
}

/// The manual camera's look by the mouse's or a finger's drag of (dx, dy)
/// pixels, the client's (as first person turns and pitches the body by
/// them, an angle a pixel): its heading and pitch, kept as `UpdateControls`
/// keeps the right stick's (the heading in [0, 2pi), the pitch short of
/// straight up or down, turned round under the Camera Pitch Axis option),
/// once its blend is all the way in. The body follows the look as the
/// stick's (`UpdateControls`).
pub fn aim(cam: &mut WalkCamera<Ee>, dx: f32, dy: f32, inverted: bool) {
    let e = &mut cam.effect;
    if e.manual_terp != F::ONE || (dx == 0.0 && dy == 0.0) {
        return;
    }
    let mut look = e.manual_hpr;
    look.x = (look.x + F::new(-dx * LOOK_PER_PIXEL)).fmodf(F::new(TWO_PI));
    if look.x < F::ZERO {
        look.x += F::new(TWO_PI);
    }
    let up = F::new(dy * LOOK_PER_PIXEL * if inverted { -1.0 } else { 1.0 });
    look.y = vi::fpu::clamp(look.y + up, F::new(-1.5607964), F::new(1.5607964));
    e.manual_hpr = look;
}

/// The wheel's `lines` notches (in positive) on the orbit's distance: in
/// the follow orbit in from where the camera is (a wall's pull included)
/// and out from the distance chosen (`camreach::Reach`), the choice kept and
/// the camera there at once, unless that is out past a pull, which the
/// camera eases back from when there is room (`Reach`); the other orbit at
/// once.
pub fn zoom(cam: &mut WalkCamera<Ee>, reach: &mut crate::camreach::Reach, lines: f32) {
    let k = 0.9f32.powf(lines);
    match cam.camera_mode {
        3 => {
            let chosen = reach.chosen(cam);
            let from = if lines > 0.0 { vi::fpu::vimin(chosen, cam.cur.dist) } else { chosen };
            let to = vi::fpu::clamp(F::new(from.0 * k), cam.min_orbit_distance, cam.max_orbit_distance);
            reach.chosen = Some(to);
            if to < cam.cur.dist || chosen <= cam.cur.dist {
                cam.set_orbit_distance(to);
            }
        }
        1 => {
            let d = cam.cur.dist.0 * k;
            cam.set_orbit_distance(F::new(d));
        }
        _ => {}
    }
}

/// A finger's drag of (dx, dy) window pixels and `wheel` lines (its pinch)
/// on the walker's camera in a frame `frame_s` seconds long (`look_cam`):
/// the touch screen's look, as the mouse's drag was before it took the
/// pointer.
pub fn look(w: &mut Walker, dx: f32, dy: f32, wheel: f32, inverted: bool, frame_s: f32) -> [f32; 2] {
    look_cam(&mut w.cam, dx, dy, wheel, inverted, frame_s)
}

/// A drag of (dx, dy) window pixels and `wheel` lines on `cam` in a frame
/// `frame_s` seconds long: in the orbiting modes the orbit, its tilt and its
/// distance moved at once; otherwise the right stick's push (rx, ry) for
/// `UpdateControls`.
pub fn look_cam(cam: &mut WalkCamera<Ee>, dx: f32, dy: f32, wheel: f32, inverted: bool, frame_s: f32) -> [f32; 2] {
    let mut stick = [0.0f32; 2];
    let per_pixel = stick_per_pixel(frame_s);
    let mode = cam.camera_mode;
    if mode == 1 || mode == 3 {
        if dx != 0.0 {
            // the view turns the way the mouse goes: the orbit the other way
            let d = dx * LOOK_PER_PIXEL * if inverted { 1.0 } else { -1.0 };
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
        stick[0] = (dx * per_pixel).clamp(-1.0, 1.0);
        stick[1] = (dy * per_pixel).clamp(-1.0, 1.0);
    }
    stick
}

/// The stick's push for a pixel of drag in a frame `frame_s` seconds long
/// (`STICK_PER_PIXEL`); a frame under a millisecond counts as one.
fn stick_per_pixel(frame_s: f32) -> f32 {
    STICK_PER_PIXEL * (1.0 / 60.0) / frame_s.max(0.001)
}

/// Takes the pointer for the look, the cursor hidden: held (a page's
/// Pointer Lock, `Asked`, granted a moment later; natively winit's
/// `Locked`), else kept in the window (`Confined`: X11, Windows). None
/// when the window will do neither.
pub fn take_pointer(window: &winit::window::Window) -> Option<Grab> {
    use winit::window::CursorGrabMode;
    let grab = if window.set_cursor_grab(CursorGrabMode::Locked).is_ok() {
        if cfg!(target_arch = "wasm32") { Grab::Asked } else { Grab::Held }
    } else if window.set_cursor_grab(CursorGrabMode::Confined).is_ok() {
        Grab::Confined
    } else {
        return None;
    };
    window.set_cursor_visible(false);
    Some(grab)
}

/// Puts the cursor at `at` (window pixels), a confined one's way back to
/// the middle (`Mouse::recentre`).
pub fn put_pointer(window: &winit::window::Window, at: (f64, f64)) {
    let _ = window.set_cursor_position(winit::dpi::PhysicalPosition::new(at.0, at.1));
}

/// Gives the pointer back, shown again at `home` (natively put there; a
/// page's browser puts it back where its lock began itself).
pub fn give_pointer(window: &winit::window::Window, home: (f64, f64)) {
    let _ = window.set_cursor_grab(winit::window::CursorGrabMode::None);
    window.set_cursor_visible(true);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = window.set_cursor_position(winit::dpi::PhysicalPosition::new(home.0, home.1));
    #[cfg(target_arch = "wasm32")]
    let _ = home;
}

/// `EQOA_MOUSE`, for headless runs: `;`-separated steps at a time in ms,
/// `click <x> <y>@<ms>`, `rclick <x> <y>@<ms>`, `drag <dx> <dy>@<ms>` (from
/// the window's middle), `wheel <lines>@<ms>`, `press l|r@<ms>` /
/// `release l|r@<ms>` and `move <dx> <dy>@<ms>` (the mouse moved: the
/// cursor, or once a drag holds the pointer its own motion), played
/// through a `Mouse` as the window's events would be.
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
            // the mouse moved, as a page hands it: the cursor (a held
            // pointer stays put), then its own motion
            Some("move") => {
                if !matches!(self.grab, Grab::Asked | Grab::Held) {
                    self.moved(self.cursor.0 + num(1), self.cursor.1 + num(2));
                }
                self.motion(num(1), num(2));
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

    /// One of two buttons let go: the gesture goes on with the other (its
    /// look, no click when it ends); the control, one button let go alone,
    /// ends it.
    #[test]
    fn letting_go_of_one_of_two_buttons_goes_on_with_the_other() {
        let t0 = Instant::now();
        let mut m = Mouse::default();
        m.moved(100.0, 100.0);
        m.button(false, true, t0);
        m.button(true, true, t0);
        assert_eq!(m.button(false, false, t0), None);
        m.moved(120.0, 100.0);
        assert_eq!(m.look, (20.0, 0.0), "the right button still looks");
        assert!(m.steer(3) && !m.running());
        assert_eq!(m.button(true, false, t0), None, "no click after a run");
        m.moved(140.0, 100.0);
        assert_eq!(m.look, (20.0, 0.0), "the gesture is over");
        assert!(!m.steer(3));
    }

    /// A press that drags takes the pointer (the window asked at the drag,
    /// not at the press, so a click never hides the cursor), and the mouse's
    /// own motion is the look, so no edge stops it once it is held: a page
    /// hands each move twice (the canvas's cursor, then the window's own
    /// motion) and it is looked with once, a held cursor stays put and a
    /// confined one's moves do not count. Let go, no click, the pointer back
    /// where the press was. The control: a still press takes nothing and
    /// clicks.
    #[test]
    fn a_drag_takes_the_pointer_and_rides_the_mouse_s_own_motion() {
        let t0 = Instant::now();
        let page = |m: &mut Mouse, x: f64, y: f64| {
            let d = (x - m.cursor.0, y - m.cursor.1);
            m.moved(x, y);
            m.motion(d.0, d.1);
        };
        let mut m = Mouse::default();
        page(&mut m, 100.0, 100.0);
        m.button(true, true, t0);
        assert_eq!(m.grab_change(true), None, "a press takes nothing");
        page(&mut m, 103.0, 100.0);
        assert_eq!(m.grab_change(true), None, "within the slop");
        assert_eq!(m.look, (0.0, 0.0));
        page(&mut m, 110.0, 100.0);
        assert_eq!(m.look, (7.0, 0.0), "the move past the slop, whole and once");
        assert_eq!(m.grab_change(true), Some(true));
        assert_eq!(m.home(), (100.0, 100.0), "where the press was");
        m.set_grab(Grab::Held);
        assert_eq!(m.grab_change(true), None, "taken once");
        for _ in 0..100 {
            m.moved(110.0, 100.0);
            m.motion(40.0, -2.0);
        }
        m.moved(700.0, 100.0);
        assert_eq!(m.look, (4007.0, -200.0), "4,000 pixels of motion, the cursor's not counted");
        assert_eq!(m.button(true, false, t0), None, "a look is no click");
        assert_eq!(m.grab_change(true), Some(false), "given back");
        assert_eq!(m.grab, Grab::Free);
        m.motion(50.0, 0.0);
        assert_eq!(m.look, (4007.0, -200.0), "let go, its motion is nothing");
        // the control
        page(&mut m, 200.0, 200.0);
        m.button(true, true, t0);
        assert_eq!(m.grab_change(true), None);
        assert_eq!(m.button(true, false, t0), Some(Click::Right(200.0, 200.0)));
        assert_eq!(m.grab_change(true), None);
    }

    /// A platform with no motion of the mouse's own (a grab held, none
    /// coming): the cursor's moves stay the look; a refused grab leaves the
    /// drag as it was, asked for once.
    #[test]
    fn without_its_own_motion_the_cursor_still_looks() {
        let t0 = Instant::now();
        let mut m = Mouse::default();
        m.moved(100.0, 100.0);
        m.button(false, true, t0);
        m.moved(110.0, 100.0);
        assert_eq!(m.grab_change(true), Some(true));
        m.set_grab(Grab::Held);
        m.moved(150.0, 120.0);
        assert_eq!(m.look, (50.0, 20.0));
        m.button(false, false, t0);
        assert_eq!(m.grab_change(true), Some(false));
        m.button(false, true, t0);
        m.moved(200.0, 120.0);
        assert_eq!(m.grab_change(true), Some(true));
        m.set_grab(Grab::Refused);
        assert_eq!(m.grab_change(true), None, "not asked again");
        m.moved(230.0, 120.0);
        assert_eq!(m.look, (130.0, 20.0));
        m.button(false, false, t0);
        assert_eq!(m.grab_change(true), None, "nothing to give back");
        assert_eq!(m.grab, Grab::Free);
    }

    /// The look taken away (a window, the start menu, a death: `free`
    /// false), Escape, and a page's lock lost all end the gesture and give
    /// the pointer back, its release clicking nothing; a lock the page holds
    /// after its drag is let go. The controls: Escape with nothing held is
    /// the game's (false), and a window up with no grab leaves its clicks.
    #[test]
    fn a_window_escape_or_a_lost_lock_lets_go_of_the_look() {
        let t0 = Instant::now();
        let grabbed = || {
            let mut m = Mouse::default();
            m.moved(100.0, 100.0);
            m.button(true, true, t0);
            m.moved(120.0, 100.0);
            assert_eq!(m.grab_change(true), Some(true));
            m.set_grab(Grab::Held);
            m
        };
        let mut m = grabbed();
        assert_eq!(m.grab_change(false), Some(false), "a window up");
        assert!(!m.steer(3) && m.button(true, false, t0).is_none());
        let mut m = grabbed();
        assert!(m.escape());
        assert_eq!(m.grab_change(true), Some(false));
        assert!(m.button(true, false, t0).is_none());
        assert!(!m.escape(), "Escape with nothing held is the game's");
        let mut m = grabbed();
        m.set_grab(Grab::Asked);
        assert!(!m.lock_seen(false, t0), "asked, not yet granted: waits");
        assert!(!m.lock_seen(true, t0));
        assert_eq!(m.grab, Grab::Held, "granted");
        assert!(!m.lock_seen(false, t0), "lost (Escape in the browser)");
        assert_eq!(m.grab_change(true), Some(false));
        assert!(m.button(true, false, t0).is_none());
        assert!(m.lock_seen(true, t0), "a lock granted after its drag: let go");
        assert!(!m.lock_seen(false, t0));
        // the control: with a window up and nothing grabbed a click is a click
        let mut m = Mouse::default();
        m.moved(50.0, 50.0);
        m.button(false, true, t0);
        assert_eq!(m.grab_change(false), None);
        assert_eq!(m.button(false, false, t0), Some(Click::Target(50.0, 50.0)));
    }

    /// Confined (natively, no lock): the cursor's moves are the look and
    /// the mouse's own motion is not (X11 hands each one twice under the
    /// grab); a quarter of the window from the middle the cursor is to be
    /// put back there, once until that move is seen, which is no look, but
    /// what a platform folds into it (Windows' moves come as one) is. Let go
    /// the warp is forgotten. The control: held (a lock), the cursor is not
    /// put back and its own motion is the look.
    #[test]
    fn a_confined_cursor_looks_and_is_put_back_in_the_middle() {
        let t0 = Instant::now();
        let size = (800, 600);
        let mut m = Mouse::default();
        m.moved(400.0, 300.0);
        m.motion(0.0, 0.0);
        m.button(true, true, t0);
        m.moved(420.0, 300.0);
        assert_eq!(m.grab_change(true), Some(true));
        m.set_grab(Grab::Confined);
        m.discard();
        m.motion(200.0, 0.0);
        m.motion(200.0, 0.0);
        m.moved(620.0, 300.0);
        assert_eq!(m.look, (200.0, 0.0), "the cursor's 200, not its own motion twice");
        assert_eq!(m.recentre(size), Some((400.0, 300.0)));
        assert_eq!(m.recentre(size), None, "once until the move back is seen");
        m.moved(650.0, 300.0);
        assert_eq!(m.look, (230.0, 0.0), "a move before the way back");
        m.moved(400.0, 300.0);
        assert_eq!(m.look, (230.0, 0.0), "the way back is no look");
        m.moved(500.0, 310.0);
        assert_eq!(m.look, (330.0, 10.0), "from the middle on");
        assert_eq!(m.recentre(size), None, "near enough the middle");
        m.moved(700.0, 310.0);
        assert_eq!(m.recentre(size), Some((400.0, 300.0)));
        m.moved(410.0, 304.0);
        assert_eq!(m.look, (540.0, 14.0), "a move folded into the way back counts");
        m.button(true, false, t0);
        assert_eq!(m.grab_change(true), Some(false));
        assert_eq!(m.warp, None);
        // the control: held, its own motion is the look, nothing put back
        m.button(true, true, t0);
        m.moved(430.0, 304.0);
        assert_eq!(m.grab_change(true), Some(true));
        m.set_grab(Grab::Held);
        m.discard();
        m.motion(300.0, 0.0);
        m.moved(730.0, 304.0);
        assert_eq!((m.look, m.recentre(size)), ((300.0, 0.0), None));
    }

    /// A page whose browser refuses the lock asked for in a drag's move
    /// (none granted within half a second, two drags running) asks for it
    /// in the press from then on, inside the press's own event; a click
    /// still clicks, its pointer given back. A lock granted between two
    /// refusals starts the count again, and one refusal alone changes
    /// nothing (the controls).
    #[test]
    fn a_page_refused_its_lock_in_the_move_asks_in_the_press() {
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        // a drag at `at`: asked, then `granted` or not within 600 ms, let go
        let drag = |m: &mut Mouse, at: u64, granted: bool| {
            m.moved(100.0, 100.0);
            m.button(true, true, ms(at));
            m.moved(120.0, 100.0);
            assert_eq!(m.grab_change(true), Some(true));
            m.set_grab(Grab::Asked);
            for f in 0..40 {
                m.lock_seen(granted && f > 2, ms(at + f * 16));
            }
            assert_eq!(m.grab, if granted { Grab::Held } else { Grab::Asked });
            m.button(true, false, ms(at + 700));
            assert_eq!(m.grab_change(true), Some(false));
        };
        let press_asks = |m: &mut Mouse| {
            m.moved(100.0, 100.0);
            m.button(false, true, t0);
            let asked = m.grab_change(true) == Some(true);
            m.release_all();
            m.grab_change(true);
            asked
        };
        let mut m = Mouse::default();
        drag(&mut m, 0, false);
        assert!(!press_asks(&mut m), "one refusal: still asked in the move");
        drag(&mut m, 1000, true);
        drag(&mut m, 2000, false);
        assert!(!press_asks(&mut m), "a lock granted in between: counted again");
        drag(&mut m, 3000, false);
        assert!(press_asks(&mut m), "two refusals running: asked in the press");
        // held from the press the cursor keeps still: its own motion finds
        // the drag, the move past the slop counted whole
        m.discard();
        m.moved(150.0, 150.0);
        m.button(true, true, ms(4000));
        assert_eq!(m.grab_change(true), Some(true));
        m.set_grab(Grab::Held);
        m.moved(150.0, 150.0);
        m.motion(3.0, 0.0);
        assert!(!m.dragging(), "within the slop");
        m.moved(150.0, 150.0);
        m.motion(10.0, 0.0);
        assert!(m.dragging());
        assert_eq!(m.look, (10.0, 0.0));
        m.button(true, false, ms(4500));
        assert_eq!(m.grab_change(true), Some(false));
        // a click still clicks, the pointer taken at the press and given back
        m.button(false, true, ms(5000));
        assert_eq!(m.grab_change(true), Some(true));
        m.set_grab(Grab::Asked);
        m.motion(2.0, 1.0);
        assert_eq!(m.button(false, false, ms(5050)), Some(Click::Target(150.0, 150.0)));
        assert_eq!(m.grab_change(true), Some(false));
        // and with a window up nothing is taken
        m.button(false, true, ms(6000));
        assert_eq!(m.grab_change(false), None);
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
        let stick = look_cam(&mut cam, 0.0, -50.0, 0.0, false, 1.0 / 60.0);
        assert_eq!(stick, [0.0, 0.0], "no zoom on the stick");
        assert_eq!(cam.def_orbit_pitch, F::new(def) + F::new(-0.4));
        assert_eq!(cam.cur.orbit.y, cam.def_orbit_pitch, "resting there, the camera with it");
        assert_eq!(cam.last.orbit.y, cam.def_orbit_pitch);
        assert_eq!(cam.cur.dist.0, dist, "the distance as it was");
        // past the bottom of the range, then the option turning it round
        look_cam(&mut cam, 0.0, -500.0, 0.0, false, 1.0 / 60.0);
        assert_eq!(cam.def_orbit_pitch.0, MIN_TILT);
        look_cam(&mut cam, 0.0, -500.0, 0.0, true, 1.0 / 60.0);
        assert_eq!(cam.def_orbit_pitch.0, MAX_TILT, "inverted: up raises it, to the top");
        // on its way to the alternate pitch (a wall), only the default moves
        cam.cur.flags |= 8;
        cam.cur.orbit.y = F::new(1.0);
        look_cam(&mut cam, 0.0, 100.0, 0.0, false, 1.0 / 60.0);
        assert_eq!(cam.def_orbit_pitch, vi::fpu::clamp(F::new(MAX_TILT) + F::new(0.8), F::new(MIN_TILT), F::new(MAX_TILT)));
        assert_eq!(cam.cur.orbit.y.0, 1.0, "left to its snap");
        // first person: the stick's y
        crate::camstyle::set_style(&mut cam, 0, None, &mut Open);
        let pitch = cam.def_orbit_pitch;
        let stick = look_cam(&mut cam, 0.0, -10.0, 0.0, false, 1.0 / 60.0);
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
        look_cam(&mut cam, 0.0, 0.0, -40.0, false, 1.0 / 60.0);
        assert_eq!(cam.cur.dist.0, 11.0);
        assert_eq!(zoom_step(&cam, 1.0), Zoom::None, "far out: a notch in zooms");
        // in all the way: 1.5 is the closest; there a notch in is first person
        look_cam(&mut cam, 0.0, 0.0, 40.0, false, 1.0 / 60.0);
        assert_eq!(cam.cur.dist.0, 1.5);
        assert_eq!(zoom_step(&cam, -1.0), Zoom::None, "out zooms");
        assert_eq!(zoom_step(&cam, 1.0), Zoom::In);
        look_cam(&mut cam, 0.0, -25.0, 0.0, false, 1.0 / 60.0);
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
            look_cam(&mut cam, 0.0, 0.0, -40.0, false, 1.0 / 60.0);
            assert_eq!(cam.cur.dist.0, 11.0);
            for t in 1..=200 {
                cam.process(&mut Wall(t * 16, reach));
            }
            assert_eq!(cam.cur.dist.0, want, "a wall {reach} behind");
        }
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

    /// An angle, wrapped into -pi..pi.
    fn wrapped(a: f32) -> f32 {
        let t = std::f32::consts::TAU;
        (a % t + t + std::f32::consts::PI) % t - std::f32::consts::PI
    }

    /// The right button in the follow orbit: the body turned to the view as
    /// the hold begins (the camera where it was), then the body and the
    /// camera behind it turned together by the mouse, the tilt as the
    /// left's. The control: the left button swings the orbit alone.
    #[test]
    fn the_right_button_turns_the_body_and_the_camera_behind_it() {
        let mut cam = follow();
        let h0 = cam.walk.phys.angles.x;
        look_held(&mut cam, [true, false], -100.0, 0.0, false, 1.0 / 60.0, false);
        assert_eq!(cam.walk.phys.angles.x, h0, "the left button leaves the body");
        let view = cam.cur.orbit.x + F::new(PI);
        assert!(wrapped((view - h0).0).abs() > 0.7, "the camera swung off its back");
        let orbit = cam.cur.orbit.x;
        look_held(&mut cam, [false, true], 0.0, 0.0, false, 1.0 / 60.0, false);
        let h = cam.walk.phys.angles.x;
        assert_eq!(h, view.fmodf(F::new(TWO_PI)), "the body to the view");
        assert_eq!((cam.walk.committed.angles.x, cam.walk.phys.target_heading), (h, h));
        assert!(wrapped((cam.cur.orbit.x - orbit).0).abs() < 1e-5, "the camera where it was");
        // 50 pixels right: both turned 0.4, the camera behind
        look_held(&mut cam, [false, true], 50.0, 0.0, false, 1.0 / 60.0, true);
        let h1 = (h + F::new(-(50.0 * LOOK_PER_PIXEL))).fmodf(F::new(TWO_PI));
        assert_eq!(cam.walk.phys.angles.x, h1);
        assert_eq!(cam.cur.orbit.x, (h1 + F::new(PI)).fmodf(F::new(TWO_PI)));
        // up: the tilt, as the left button's (the camera lower)
        let pitch = cam.def_orbit_pitch;
        look_held(&mut cam, [false, true], 0.0, -10.0, false, 1.0 / 60.0, true);
        assert_eq!(cam.def_orbit_pitch, pitch + F::new(-10.0 * TILT_PER_PIXEL));
        assert_eq!(cam.walk.phys.angles.x, h1, "no turn");
    }

    /// Steered, a snap of the orbit to a clear angle (the game's, near a
    /// wall) would swing the camera off the body's back: it ends where it
    /// is and the camera comes in to its closest. The control: unsteered
    /// the game's snap goes on.
    #[test]
    fn steered_the_camera_comes_in_where_the_game_would_swing_it_round() {
        for steered in [true, false] {
            let mut cam = follow();
            cam.cur.flags |= 1;
            cam.cur.snap_angle = F::new(2.0);
            look_held(&mut cam, [!steered, steered], 0.0, 0.0, false, 1.0 / 60.0, true);
            if steered {
                assert_eq!(cam.cur.snap_angle, cam.cur.orbit.x);
                assert_eq!(cam.cur.flags & 3, 3, "the snap ends at once, the camera comes in");
            } else {
                assert_eq!((cam.cur.snap_angle.0, cam.cur.flags & 3), (2.0, 1));
            }
        }
    }

    /// First person: either button turns the body at once, a full turn and
    /// more in one move (the stick's turn stops at a quarter turn a second),
    /// and pitches it, stopping at the walk's 89.4 degrees up and down; the
    /// option turns the pitch round, not the turn; at a liquid's surface
    /// the pitch is the stick's (a dive). The control: the touch screen's
    /// look there pushes the stick, all the way at most.
    #[test]
    fn first_person_turns_full_circles_and_stops_short_of_straight_up_and_down() {
        let mut cam = follow();
        crate::camstyle::set_style(&mut cam, 0, None, &mut Open);
        let h0 = cam.walk.phys.angles.x;
        let stick = look_held(&mut cam, [false, true], 1000.0, 0.0, false, 1.0 / 60.0, false);
        assert_eq!(stick, [0.0, 0.0]);
        let h1 = (h0 + F::new(-1000.0 * LOOK_PER_PIXEL)).fmodf(F::new(TWO_PI));
        assert_eq!((cam.walk.phys.angles.x, cam.walk.committed.angles.x), (h1, h1), "8 radians");
        look_held(&mut cam, [true, false], 0.0, 1000.0, false, 1.0 / 60.0, false);
        assert_eq!(cam.walk.phys.angles.y.0, 1.5607964, "straight down, all but");
        look_held(&mut cam, [true, false], 0.0, -1000.0, false, 1.0 / 60.0, false);
        assert_eq!(cam.walk.phys.angles.y.0, -1.5607964, "straight up, all but");
        look_held(&mut cam, [true, false], 0.0, 300.0, false, 1.0 / 60.0, false);
        let p = cam.walk.phys.angles.y;
        look_held(&mut cam, [true, false], 10.0, 10.0, true, 1.0 / 60.0, false);
        assert_eq!(cam.walk.phys.angles.y, p + F::new(-10.0 * LOOK_PER_PIXEL), "inverted: down looks up");
        assert_eq!(cam.walk.phys.angles.x, (h1 + F::new(-10.0 * LOOK_PER_PIXEL)).fmodf(F::new(TWO_PI)), "the turn as it was");
        cam.walk.phys.flags |= vi::walk::flags::AT_WATER_SURFACE;
        let p = cam.walk.phys.angles.y;
        assert_eq!(look_held(&mut cam, [true, false], 0.0, 10.0, false, 1.0 / 60.0, false), [0.0, 0.5]);
        assert_eq!(cam.walk.phys.angles.y, p, "the dive is the stick's");
        // the control: the touch screen's look turns nothing itself
        let h = cam.walk.phys.angles.x;
        assert_eq!(look_cam(&mut cam, 1000.0, 0.0, 0.0, false, 1.0 / 60.0), [1.0, 0.0]);
        assert_eq!(cam.walk.phys.angles.x, h);
    }

    /// The wheel in the follow orbit: out from the distance chosen, the
    /// camera left where a wall pulled it (it eases back out when there is
    /// room, `camreach`); in from where the camera is, at once; with no pull
    /// both at once, as before.
    #[test]
    fn the_wheel_zooms_out_from_the_choice_and_in_from_the_camera() {
        let mut cam = follow();
        let mut reach = crate::camreach::Reach::default();
        cam.set_orbit_distance(F::new(3.5));
        reach.chosen = Some(F::new(8.0));
        zoom(&mut cam, &mut reach, -1.0);
        assert_eq!((reach.chosen(&cam).0, cam.cur.dist.0), (8.0 / 0.9, 3.5), "out: the choice");
        zoom(&mut cam, &mut reach, 1.0);
        assert_eq!((reach.chosen(&cam).0, cam.cur.dist.0), (3.5 * 0.9, 3.5 * 0.9), "in: from the camera");
        zoom(&mut cam, &mut reach, -1.0);
        assert_eq!((reach.chosen(&cam).0, cam.cur.dist.0), (3.5 * 0.9 / 0.9, 3.5 * 0.9 / 0.9), "no pull: at once");
        zoom(&mut cam, &mut reach, -40.0);
        assert_eq!((reach.chosen(&cam).0, cam.cur.dist.0), (11.0, 11.0), "the follow orbit's ceiling");
    }

    /// The walk camera on a room, driven as `Walker` drives it.
    struct Rig {
        cam: WalkCamera<Ee>,
        eye: vi::camera::Eye<Ee>,
        room: crate::camreach::testing::Room,
    }

    impl crate::controls::Walk for Rig {
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
        fn camera_dir(&self) -> vi::vect::Vect3<Ee> {
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
        fn view_relative_move(&mut self, r: &vi::vect::Vect3<Ee>, x: F<Ee>, y: F<Ee>) {
            self.cam.walk.view_relative_move(r, x, y);
        }
        fn view_relative_walk(&mut self, r: &vi::vect::Vect3<Ee>, x: F<Ee>, y: F<Ee>) {
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
        fn manual_hpr(&self) -> vi::vect::Vect3<Ee> {
            self.cam.effect.manual_hpr
        }
        fn set_manual_hpr(&mut self, v: vi::vect::Vect3<Ee>) {
            self.cam.effect.manual_hpr = v;
        }
    }

    impl Rig {
        /// The follow orbit on the room's floor, facing 0.5, settled.
        fn new() -> Self {
            let mut room = crate::camreach::testing::Room { t: 0, wall: f32::INFINITY };
            let mut cam = WalkCamera::<Ee>::default();
            cam.init(vi::vect::Vect3::of(100.0, 0.0, 100.0), vi::vect::Vect3::of(0.5, 0.0, 0.0), &mut room);
            crate::camstyle::set_style(&mut cam, 3, None, &mut room);
            let eye = cam.process(&mut room);
            let mut r = Rig { cam, eye, room };
            for _ in 0..30 {
                r.frame([0.0; 4], false, None);
            }
            r
        }

        /// A 16 ms frame: the mouse's look (`held`, with the frames before
        /// it steered), `UpdateControls` on `sticks`, `Process`.
        fn frame(&mut self, sticks: [f32; 4], steer: bool, held: Option<([bool; 2], bool)>) {
            if let Some((held, steering)) = held {
                look_held(&mut self.cam, held, 0.0, 0.0, false, 0.016, steering);
            }
            let c = crate::controls::Client { steer, ..Default::default() };
            crate::controls::update_controls(self, sticks, &c);
            self.room.t += 16;
            self.eye = self.cam.process(&mut self.room);
        }
    }

    /// The right button and A: a sidestep along a line (the strafe's
    /// animation), the body facing as it was, the camera behind it; both
    /// buttons and A run the diagonal, forward and left at once. The
    /// controls: unsteered the game turns the body to its way (and back
    /// runs it round), and steered with the camera not put back behind it,
    /// the camera is dragged round.
    #[test]
    fn steered_a_sidesteps_in_a_line_and_both_buttons_run_the_diagonal() {
        for (both, steer, behind) in [(false, true, true), (true, true, true), (false, false, false), (false, true, false)] {
            let mut r = Rig::new();
            let (h0, p0) = (r.cam.walk.phys.angles.x.0, r.cam.walk.phys.position);
            let sticks = [0.0, 0.0, -1.0, if both { -1.0 } else { 0.0 }];
            for f in 0..90 {
                r.frame(sticks, steer, behind.then_some(([both, true], f > 0)));
            }
            let (h, p) = (r.cam.walk.phys.angles.x.0, r.cam.walk.phys.position);
            let (dx, dz) = (p.x.0 - p0.x.0, p.z.0 - p0.z.0);
            let ahead = dx * h0.sin() + dz * h0.cos();
            let left = dx * h0.cos() - dz * h0.sin();
            let off = wrapped(r.cam.cur.orbit.x.0 - h - std::f32::consts::PI).abs();
            let case = format!("both {both} steer {steer} behind {behind}: heading {h0} -> {h}, ahead {ahead}, left {left}, camera off its back {off}");
            if !steer {
                assert!(wrapped(h - h0).abs() > 0.5, "the game turns the body: {case}");
                continue;
            }
            assert!(wrapped(h - h0).abs() < 1e-4, "{case}");
            if both {
                assert!(left > 5.0 && (ahead - left).abs() < 0.05 * left, "the diagonal: {case}");
                assert_eq!(r.cam.walk.phys.loco_state, 3, "running");
            } else {
                assert!(left > 3.0 && ahead.abs() < 0.05 * left, "a line: {case}");
                assert!(matches!(r.cam.walk.phys.loco_state, 10 | 11), "the strafe: {case}");
            }
            if behind {
                assert!(off < 0.05, "{case}");
            } else {
                assert!(off > 0.3, "dragged round: {case}");
            }
        }
    }
}
