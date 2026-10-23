//! `VIClient::UpdateControls` (CLIENT 0x00BC3AE8): the pad's sticks
//! (`m_p07190`, after `ScaleStick`) turned into the walk control's and the
//! camera's inputs for the camera mode, then `VIWalkCamera::Process`.
//!
//! Per mode: first person (0) and the fixed follow (2) move, turn on the right
//! stick and pitch; the orbit of mode 1 turns on the left stick and orbits on
//! the right; the third-person orbit (3) moves relative to the camera, orbits
//! and zooms on the right stick (under liquid its y swims). The options'
//! Camera Pitch Axis turns the pitch round in modes 0 and 2 and the orbit in
//! 1 and 3. While fighting, or for a second after a swing's effect, the
//! player turns to face the target (mode 3 heads straight for it). Autorun
//! runs forward until the stick is pushed past 0.7; the player's own cast
//! walks; the locks stop movement or the camera; a connection silent for 8 s
//! stops both (`Client::timed_out`, which the client leaves off: see
//! `Play::control_client`).
//!
//! Follow (`m_p00078`, `ToggleFollow`: `/follow` and the quick command, on a
//! targeted group member, `m_p0007C`) turns the player to the member (in
//! mode 3 the camera orbits round to it, the body facing it) and runs at it,
//! walks within 6 and stops within 3, or turns first while it is more than
//! 60 degrees off; under liquid it pitches to the member's height. It ends
//! with the attack, with the stick pushed past 0.7, and when the member is
//! no longer in the group and the scene; it ends autorun, and while it is on
//! the player runs 1.05 times as fast (`UpdateGame`, `Client::run_speed`).
//!
//! The keyboard's own axes are the game's (`Client::keys`, `m_keys[17..=24]`,
//! the cursor block as `VIKeyboard::Read` codes it): the arrows walk and turn
//! (in mode 3 strafe), Home and End orbit mode 3, Page Up and Page Down pitch
//! (zoom in the orbits); left out while a chat line is up. The viewer feeds
//! Home, End, Page Up and Page Down; its arrows are the quick menus' d-pad,
//! because WASD walks there, a pad's left stick (`walker::Controls`, with the
//! client's own right stick on I/J/K/L).
//!
//! Not here: the modal look (mode 38, which holding L1 or R1 begins; the
//! camera effect's blend into a scripted camera is not ported).
//!
//! The client's own, not the game's: `Client::steer`, the mouse steering the
//! body (`mouse.rs`: its right button in the follow orbit, either button in
//! first person and the follow camera, where the mouse turns the body itself).
//! Then mode 3 walks as modes 0 and 2 do (`Move` or `Walk` on the body's own
//! axes: forward, back and a sidestep, the body keeping the heading the mouse
//! gave it, not turning to its motion), and no mode turns to the target. With
//! it off (the pad, the keys, the touch screen) every call is the game's.
//! While it steers, follow paces the player but does not turn it.

// Written as the C writes it: its float literals (pi as 3.1415927), `a = a +
// b`, and the nested tests.
#![allow(clippy::approx_constant, clippy::assign_op_pattern, clippy::collapsible_if, clippy::collapsible_match)]

use vi::fpu::{Ee, F};
use vi::vect::Vect3;

type Fe = F<Ee>;

const PI: f32 = 3.1415927;
const TWO_PI: f32 = 6.2831855;

/// What `UpdateControls` reads of the client besides the pad.
#[derive(Clone, Copy, Debug)]
pub struct Client {
    /// `ModeHasChat`: a chat line is up (modes 2, 28, 29, 30, 33)
    pub has_chat: bool,
    /// the options' Camera Pitch Axis (`m_x07`)
    pub inverted: bool,
    pub auto_attack: bool,
    /// `m_p07568 - m_p071F0 < 1000`: a swing's effect played in the last second
    pub swung: bool,
    /// the target's actor position and the player's, when the target is
    /// another creature with an actor
    pub target: Option<([f32; 3], [f32; 3])>,
    /// `m_p00074`
    pub autorun: bool,
    /// within the player's own cast (`m_p2BB40` .. `m_p2BB48`): `Walk`
    pub casting: bool,
    /// `m_p06DC0`: movement allowed
    pub move_ok: bool,
    /// `m_p06DC4`: the camera allowed
    pub look_ok: bool,
    /// `m_p06DC8` (0x00FC): held in place
    pub rooted: bool,
    /// `IsTimedOut`: nothing has come for 8 s
    pub timed_out: bool,
    /// `m_keys[17..=24]`, the keyboard's keys held, 1 or 0: left, right, up,
    /// down, page up, page down, end, home
    pub keys: [f32; 8],
    /// the client's own (the module's doc): the mouse steers the body
    pub steer: bool,
    /// `m_p00078`: following the group member `m_p0007C`
    pub follow: Follow,
    /// `UpdateGame`'s `m_walkCam.m_runSpeed`, which `Walker::frame_with`
    /// sets before `UpdateControls`: 1.05 times the speed while following
    pub run_speed: f32,
}

impl Default for Client {
    fn default() -> Self {
        Client { has_chat: false, inverted: false, auto_attack: false, swung: false, target: None, autorun: false, casting: false, move_ok: true, look_ok: true, rooted: false, timed_out: false, keys: [0.0; 8], steer: false, follow: Follow::Off, run_speed: vi::walk::RUN_SPEED }
    }
}

/// Following (`m_p00078`), and the member followed (`m_p0007C`) as
/// `UpdateControls` finds it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Follow {
    #[default]
    Off,
    /// The member is not in the group, or not in the scene with an actor,
    /// or the player has none: the follow ends.
    Lost,
    /// The member's actor position and the player's.
    At([f32; 3], [f32; 3]),
}

/// What `UpdateControls` changed of the client, for it to carry out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Changed {
    /// `ToggleP74`: autorun off, by the stick or by a follow
    pub autorun: bool,
    /// `m_p00078 = 0`: the follow ended
    pub follow: bool,
}

/// What `UpdateControls` calls on `m_walkCam` (a `VIWalkCamera`, which is a
/// `VIWalkControl`), and what it reads of it.
pub trait Walk {
    fn camera_mode(&self) -> i32;
    /// `m_committed.m_angles.x`
    fn heading(&self) -> Fe;
    /// `m_committed.m_angles.y`
    fn pitch(&self) -> Fe;
    /// `m_last.m_orbit.x`: the orbit's angle, the camera's way round
    fn orbit_angle(&self) -> Fe;
    /// `m_camera.m_dir`
    fn camera_dir(&self) -> Vect3<Ee>;
    fn under_liquid(&self) -> bool;
    fn at_liquid_surface(&self) -> bool;
    /// `Move`
    fn move_stick(&mut self, x: Fe, y: Fe);
    fn walk(&mut self, x: Fe, y: Fe);
    fn turn_left(&mut self, t: Fe);
    fn pitch_down(&mut self, t: Fe);
    fn swim_up(&mut self, t: Fe);
    fn view_relative_move(&mut self, r: &Vect3<Ee>, x: Fe, y: Fe);
    fn view_relative_walk(&mut self, r: &Vect3<Ee>, x: Fe, y: Fe);
    fn set_target_heading(&mut self, h: Fe);
    fn orbit_in(&mut self, t: Fe);
    fn orbit_right(&mut self, t: Fe);
}

/// `UpdateControls` up to `Process`, with the sticks `keys` (rx, ry, lx,
/// ly). What it changed of the client (autorun, the follow) is the client's
/// to carry out.
pub fn update_controls(w: &mut impl Walk, keys: [f32; 4], c: &Client) -> Changed {
    let f = Fe::new;
    let k = keys.map(f);
    // `m_keys[17..=24]`: the keyboard's cursor block
    let kb = |i: usize| f(c.keys[i - 17]);
    let mut orbit = -k[0];
    let mut turn = Fe::ZERO;
    let mut strafe = k[2];
    let mut pitch;
    let mut fwd;
    if c.has_chat {
        pitch = k[1];
        fwd = -k[3];
    } else {
        pitch = k[1] + kb(22) - kb(21);
        fwd = (kb(19) - kb(20)) - k[3];
    }
    let mode = w.camera_mode();
    if c.inverted && (mode == 2 || mode == 0) {
        pitch = -pitch;
    }
    match mode {
        0 | 2 => turn = if c.has_chat { -k[0] } else { -k[0] + kb(17) - kb(18) },
        1 => turn = if c.has_chat { -k[2] } else { -k[2] + kb(17) - kb(18) },
        3 => {
            if !c.has_chat {
                orbit = orbit + (kb(24) - kb(23));
                strafe = strafe + (kb(18) - kb(17));
            }
        }
        _ => {}
    }
    let mut face = None;
    // (`!c.steer`: the client's own, the mouse's heading wins)
    if !c.steer && ((orbit == Fe::ZERO && c.swung) || c.auto_attack) {
        if let Some((t, s)) = c.target {
            let mut d = Vect3::<Ee>::of(t[0], t[1], t[2]);
            d -= Vect3::of(s[0], s[1], s[2]);
            d.y = Fe::ZERO;
            let (hx, _, _) = d.get_hpr();
            face = Some(hx);
            let mut h = hx - w.heading();
            if f(PI) < h {
                h = h - f(TWO_PI);
            } else if h < f(-PI) {
                h = h + f(TWO_PI);
            }
            if f(0.17453292) < h {
                turn = Fe::ONE;
            } else if h < f(-0.17453292) {
                turn = f(-1.0);
            }
        }
    }
    // Follow: ended by the attack and by the stick, else steered to the
    // member while it is found (`Follow::At`).
    let mut following = c.follow != Follow::Off && !c.auto_attack;
    let (mut dpitch, mut dh, mut height, mut dist) = (Fe::ZERO, Fe::ZERO, Fe::ZERO, Fe::ZERO);
    if following && c.move_ok && f(0.7) < fwd.abs() {
        following = false;
    }
    if following {
        following = false;
        if let Follow::At(o, s) = c.follow {
            following = true;
            let mut d = Vect3::<Ee>::of(o[0], o[1], o[2]);
            d -= Vect3::of(s[0], s[1], s[2]);
            height = d.y;
            d.y = dh;
            let (hx, _, _) = d.get_hpr();
            dist = d.magnitude();
            dh = if mode == 3 { hx - (w.orbit_angle() + f(PI)).fmodf(f(TWO_PI)) } else { hx - w.heading() };
            if f(PI) < dh {
                dh = dh - f(TWO_PI);
            } else if dh < f(-PI) {
                dh = dh + f(TWO_PI);
            }
            let mut d = Vect3::<Ee>::of(o[0], o[1], o[2]);
            d -= Vect3::of(s[0], s[1], s[2]);
            let (_, p, _) = d.get_hpr();
            dpitch = p - w.pitch();
            // (`!c.steer`, as above: the mouse's heading wins)
            if !c.steer {
                face = Some(hx);
                if mode == 3 {
                    if f(3.0) < dist {
                        if f(0.34906584) < dh {
                            orbit = f(-1.0);
                        } else if dh < f(-0.34906584) {
                            orbit = Fe::ONE;
                        } else if f(0.08726646) < dh {
                            orbit = f(-0.125);
                        } else if dh < f(-0.08726646) {
                            orbit = f(0.125);
                        }
                        if c.inverted {
                            orbit = -orbit;
                        }
                    }
                } else if f(0.17453292) < dh {
                    turn = Fe::ONE;
                } else if dh < f(-0.17453292) {
                    turn = f(-1.0);
                } else if f(0.08726646) < dh {
                    turn = f(0.25);
                } else if dh < f(-0.08726646) {
                    turn = f(-0.25);
                }
            }
        }
    }
    let mut walking = c.casting;
    let mut autorun = c.autorun;
    let mut toggled = false;
    if autorun && c.move_ok && f(0.7) < fwd.abs() {
        autorun = false;
        toggled = true;
    }
    if following && autorun {
        autorun = false;
        toggled = true;
    }
    if following {
        if w.under_liquid() || w.at_liquid_surface() {
            if mode != 3 {
                if f(0.17453292) < dpitch {
                    pitch = Fe::ONE;
                } else if dpitch < f(-0.17453292) {
                    pitch = f(-1.0);
                } else if f(0.08726646) < dpitch {
                    pitch = f(0.25);
                } else if dpitch < f(-0.08726646) {
                    pitch = f(-0.25);
                }
                // (the decomp's `1 < fabsf(height)` stops here, a store every
                // path below overwrites)
            } else if Fe::ONE < height {
                pitch = f(-1.0);
            } else if height < f(-1.0) {
                pitch = Fe::ONE;
            }
        }
        if f(1.0471976) < dh || dh < f(-1.0471976) {
            fwd = Fe::ZERO;
        } else if f(6.0) < dist {
            fwd = Fe::ONE;
        } else if dist < f(3.0) {
            fwd = Fe::ZERO;
        } else {
            fwd = Fe::ONE;
            walking = true;
        }
    } else if autorun {
        fwd = Fe::ONE;
    }
    if !c.move_ok || c.rooted {
        strafe = Fe::ZERO;
        fwd = strafe;
    }
    if !c.look_ok {
        orbit = Fe::ZERO;
        pitch = orbit;
        turn = orbit;
    }
    if c.timed_out {
        strafe = Fe::ZERO;
        fwd = strafe;
        orbit = strafe;
        pitch = orbit;
        turn = orbit;
    }
    match mode {
        0 | 2 => {
            if !walking {
                w.move_stick(fwd, strafe);
            } else {
                w.walk(fwd, strafe);
            }
            w.turn_left(turn);
            w.pitch_down(pitch);
        }
        1 => {
            if !walking {
                w.move_stick(fwd, strafe);
            } else {
                w.walk(fwd, strafe);
            }
            w.turn_left(turn);
            w.orbit_in(-pitch);
            w.orbit_right(if c.inverted { orbit } else { -orbit });
        }
        3 => {
            if c.steer {
                // the client's own: the body's own axes, as modes 0 and 2
                if !walking {
                    w.move_stick(fwd, strafe);
                } else {
                    w.walk(fwd, strafe);
                }
            } else {
                let dir = w.camera_dir();
                if !walking {
                    w.view_relative_move(&dir, fwd, strafe);
                } else {
                    w.view_relative_walk(&dir, fwd, strafe);
                }
                if let Some(h) = face {
                    w.set_target_heading(h);
                }
            }
            if !w.under_liquid() && !w.at_liquid_surface() {
                w.orbit_in(-pitch);
            } else {
                w.swim_up(-pitch);
            }
            w.orbit_right(if !c.inverted { -orbit } else { orbit });
        }
        _ => {}
    }
    Changed { autorun: toggled, follow: c.follow != Follow::Off && !following }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The calls `UpdateControls` makes, as text.
    struct Log {
        mode: i32,
        heading: f32,
        /// the orbit's angle: the camera's way round is it plus pi
        orbit: f32,
        liquid: bool,
        calls: Vec<String>,
    }

    impl Log {
        fn new(mode: i32) -> Self {
            Log { mode, heading: 0.0, orbit: PI, liquid: false, calls: Vec::new() }
        }
    }

    impl Walk for Log {
        fn camera_mode(&self) -> i32 {
            self.mode
        }
        fn heading(&self) -> Fe {
            Fe::new(self.heading)
        }
        fn pitch(&self) -> Fe {
            Fe::ZERO
        }
        fn orbit_angle(&self) -> Fe {
            Fe::new(self.orbit)
        }
        fn camera_dir(&self) -> Vect3<Ee> {
            Vect3::of(0.0, 0.0, 1.0)
        }
        fn under_liquid(&self) -> bool {
            self.liquid
        }
        fn at_liquid_surface(&self) -> bool {
            false
        }
        fn move_stick(&mut self, x: Fe, y: Fe) {
            self.calls.push(format!("move {} {}", x.0, y.0));
        }
        fn walk(&mut self, x: Fe, y: Fe) {
            self.calls.push(format!("walk {} {}", x.0, y.0));
        }
        fn turn_left(&mut self, t: Fe) {
            self.calls.push(format!("turn {}", t.0));
        }
        fn pitch_down(&mut self, t: Fe) {
            self.calls.push(format!("pitch {}", t.0));
        }
        fn swim_up(&mut self, t: Fe) {
            self.calls.push(format!("swim {}", t.0));
        }
        fn view_relative_move(&mut self, _: &Vect3<Ee>, x: Fe, y: Fe) {
            self.calls.push(format!("vmove {} {}", x.0, y.0));
        }
        fn view_relative_walk(&mut self, _: &Vect3<Ee>, x: Fe, y: Fe) {
            self.calls.push(format!("vwalk {} {}", x.0, y.0));
        }
        fn set_target_heading(&mut self, h: Fe) {
            self.calls.push(format!("heading {:.3}", h.0));
        }
        fn orbit_in(&mut self, t: Fe) {
            self.calls.push(format!("in {}", t.0));
        }
        fn orbit_right(&mut self, t: Fe) {
            self.calls.push(format!("right {}", t.0));
        }
    }

    fn run(mode: i32, keys: [f32; 4], c: Client) -> (Vec<String>, Changed) {
        let mut w = Log::new(mode);
        let t = update_controls(&mut w, keys, &c);
        (w.calls, t)
    }

    // rx, ry, lx, ly as the sticks read them; +y down
    const STICKS: [f32; 4] = [0.5, 0.25, -0.75, -1.0];

    #[test]
    fn each_mode_takes_the_sticks_as_the_game_does() {
        let c = Client::default();
        // first person and the fixed follow: move, turn on rx, pitch on ry
        for m in [0, 2] {
            assert_eq!(run(m, STICKS, c).0, ["move 1 -0.75", "turn -0.5", "pitch 0.25"], "mode {m}");
        }
        // mode 1: the left stick's x strafes AND turns; the right orbits
        assert_eq!(run(1, STICKS, c).0, ["move 1 -0.75", "turn 0.75", "in -0.25", "right 0.5"]);
        // mode 3: view relative, zoom on ry, orbit on rx
        assert_eq!(run(3, STICKS, c).0, ["vmove 1 -0.75", "in -0.25", "right 0.5"]);
        // under liquid ry swims instead
        let mut w = Log::new(3);
        w.liquid = true;
        update_controls(&mut w, STICKS, &c);
        assert_eq!(w.calls, ["vmove 1 -0.75", "swim -0.25", "right 0.5"]);
        // the player's own cast walks
        let cast = Client { casting: true, ..c };
        assert_eq!(run(0, STICKS, cast).0[0], "walk 1 -0.75");
        assert_eq!(run(3, STICKS, cast).0[0], "vwalk 1 -0.75");
    }

    #[test]
    fn the_camera_option_turns_the_pitch_round_in_0_and_2_and_the_orbit_in_1_and_3() {
        let c = Client { inverted: true, ..Client::default() };
        assert_eq!(run(0, STICKS, c).0, ["move 1 -0.75", "turn -0.5", "pitch -0.25"]);
        assert_eq!(run(2, STICKS, c).0, ["move 1 -0.75", "turn -0.5", "pitch -0.25"]);
        assert_eq!(run(1, STICKS, c).0, ["move 1 -0.75", "turn 0.75", "in -0.25", "right -0.5"]);
        assert_eq!(run(3, STICKS, c).0, ["vmove 1 -0.75", "in -0.25", "right -0.5"]);
    }

    #[test]
    fn the_locks_and_a_silent_line_stop_what_they_stop() {
        let c = Client::default();
        assert_eq!(run(0, STICKS, Client { move_ok: false, ..c }).0, ["move 0 0", "turn -0.5", "pitch 0.25"]);
        assert_eq!(run(0, STICKS, Client { rooted: true, ..c }).0, ["move 0 0", "turn -0.5", "pitch 0.25"]);
        assert_eq!(run(3, STICKS, Client { look_ok: false, ..c }).0, ["vmove 1 -0.75", "in -0", "right -0"]);
        assert_eq!(run(1, STICKS, Client { timed_out: true, ..c }).0, ["move 0 0", "turn 0", "in -0", "right -0"]);
    }

    #[test]
    fn autorun_runs_forward_until_the_stick_passes_0_7() {
        let c = Client { autorun: true, ..Client::default() };
        // (-0 + 0 is +0: the keyboard terms the C adds make the idle turn +0)
        assert_eq!(run(0, [0.0; 4], c), (vec!["move 1 0".into(), "turn 0".into(), "pitch 0".into()], Changed::default()));
        // pulled back past 0.7: cancelled, and this frame the stick's own
        let (calls, t) = run(0, [0.0, 0.0, 0.0, 0.8], c);
        assert_eq!(t, Changed { autorun: true, follow: false });
        assert_eq!(calls[0], "move -0.8 0");
        // 0.7 itself does not
        assert!(!run(0, [0.0, 0.0, 0.0, 0.7], c).1.autorun);
        // nor anything while movement is locked
        assert!(!run(0, [0.0, 0.0, 0.0, 1.0], Client { move_ok: false, ..c }).1.autorun);
    }

    // The player at (10, 0, 10); a member 10 along +x, heading pi/2.
    const ME: [f32; 3] = [10.0, 0.0, 10.0];
    fn member(dx: f32, dy: f32) -> Client {
        Client { follow: Follow::At([10.0 + dx, dy, 10.0], ME), ..Client::default() }
    }

    /// Following in first person and the fixed follow camera: a turn to the
    /// member, full past 10 degrees, a quarter past 5; a run to it past 6, a
    /// walk from 3 to 6, a stop within 3, and none while it is more than 60
    /// degrees off (the control: not following, the same frame is idle).
    #[test]
    fn follow_turns_to_the_member_and_runs_walks_or_stops_by_its_distance() {
        assert_eq!(run(0, [0.0; 4], Client::default()).0, ["move 0 0", "turn 0", "pitch 0"]);
        // facing 0, the member a quarter turn round: turn, no step
        assert_eq!(run(0, [0.0; 4], member(10.0, 0.0)), (vec!["move 0 0".into(), "turn 1".into(), "pitch 0".into()], Changed::default()));
        let facing = |heading: f32, c: Client| {
            let mut w = Log::new(2);
            w.heading = heading;
            update_controls(&mut w, [0.0; 4], &c);
            w.calls
        };
        assert_eq!(facing(1.5, member(10.0, 0.0)), ["move 1 0", "turn 0", "pitch 0"], "within 5 degrees: a run, no turn");
        assert_eq!(facing(1.45, member(10.0, 0.0))[1], "turn 0.25");
        assert_eq!(facing(1.7, member(10.0, 0.0))[1], "turn -0.25");
        assert_eq!(facing(1.0, member(10.0, 0.0)), ["move 1 0", "turn 1", "pitch 0"], "33 degrees off: on, turning");
        assert_eq!(facing(0.5, member(10.0, 0.0))[0], "move 0 0", "61 degrees off: it turns first");
        assert_eq!(facing(1.5708, member(5.0, 0.0))[0], "walk 1 0");
        assert_eq!(facing(1.5708, member(6.0, 0.0))[0], "walk 1 0", "6 itself walks");
        assert_eq!(facing(1.5708, member(2.0, 0.0))[0], "move 0 0");
        // the same distances without the follow: no step
        assert_eq!(facing(1.5708, Client::default())[0], "move 0 0");
    }

    /// In the follow orbit the camera swings round to the member (a full
    /// orbit past 20 degrees, an eighth past 5, the option leaving it as it
    /// is) and the body heads for it; within 3 the camera is left alone.
    #[test]
    fn follow_in_the_follow_orbit_swings_the_camera_round_to_the_member() {
        let orbit = |view: f32, c: Client| {
            let mut w = Log::new(3);
            w.orbit = view - PI;
            update_controls(&mut w, [0.0; 4], &c);
            w.calls
        };
        // the camera looking along pi/2, at the member
        assert_eq!(orbit(1.5707964, member(10.0, 0.0)), ["vmove 1 0", "heading 1.571", "in -0", "right -0"]);
        assert_eq!(orbit(1.0707964, member(10.0, 0.0))[3], "right 1");
        assert_eq!(orbit(2.0707964, member(10.0, 0.0))[3], "right -1");
        assert_eq!(orbit(1.3707964, member(10.0, 0.0))[3], "right 0.125");
        assert_eq!(orbit(1.0707964, Client { inverted: true, ..member(10.0, 0.0) })[3], "right 1");
        assert_eq!(orbit(1.0707964, member(2.5, 0.0)), ["vmove 0 0", "heading 1.571", "in -0", "right -0"]);
        // the control: not following, nothing turns
        assert_eq!(orbit(1.0707964, Client::default()), ["vmove 0 0", "in -0", "right -0"]);
    }

    /// The attack ends a follow, and the stick pushed past 0.7 (not with
    /// movement locked); a member gone ends it; it ends autorun.
    #[test]
    fn what_ends_a_follow_and_what_a_follow_ends() {
        let ended = Changed { autorun: false, follow: true };
        let (calls, t) = run(0, [0.0; 4], Client { auto_attack: true, ..member(10.0, 0.0) });
        assert_eq!((calls[0].as_str(), t), ("move 0 0", ended));
        assert_eq!(run(0, [0.0, 0.0, 0.0, -0.8], member(10.0, 0.0)), (vec!["move 0.8 0".into(), "turn 0".into(), "pitch 0".into()], ended));
        assert_eq!(run(0, [0.0, 0.0, 0.0, -0.7], member(10.0, 0.0)).1, Changed::default());
        assert_eq!(run(0, [0.0, 0.0, 0.0, -1.0], Client { move_ok: false, ..member(10.0, 0.0) }).1, Changed::default());
        assert_eq!(run(0, [0.0; 4], Client { follow: Follow::Lost, ..Client::default() }).1, ended);
        assert_eq!(run(0, [0.0; 4], Client::default()).1, Changed::default());
        // autorun off, the follow on: the member a quarter turn round stops it
        assert_eq!(run(0, [0.0; 4], Client { autorun: true, ..member(10.0, 0.0) }), (vec!["move 0 0".into(), "turn 1".into(), "pitch 0".into()], Changed { autorun: true, follow: false }));
    }

    /// Under liquid a follow pitches to the member: by the angle up to it in
    /// first person, by its height in the follow orbit; out of it, not.
    #[test]
    fn follow_under_liquid_pitches_to_the_member() {
        let swim = |mode: i32, liquid: bool| {
            let mut w = Log::new(mode);
            w.heading = 1.5708;
            w.orbit = 1.5707964 - PI;
            w.liquid = liquid;
            update_controls(&mut w, [0.0; 4], &member(10.0, 5.0));
            w.calls
        };
        assert_eq!(swim(0, true), ["move 1 0", "turn 0", "pitch -1"]);
        assert_eq!(swim(0, false), ["move 1 0", "turn 0", "pitch 0"]);
        assert_eq!(swim(3, true), ["vmove 1 0", "heading 1.571", "swim 1", "right -0"]);
        assert_eq!(swim(3, false), ["vmove 1 0", "heading 1.571", "in -0", "right -0"]);
    }

    /// The mouse steering: the follow sets the pace on the body's own axes,
    /// and turns nothing.
    #[test]
    fn a_steered_follow_paces_and_does_not_turn() {
        let steer = Client { steer: true, ..member(10.0, 0.0) };
        let mut w = Log::new(3);
        w.heading = 1.5708;
        w.orbit = 1.0707964 - PI;
        update_controls(&mut w, [0.0; 4], &steer);
        assert_eq!(w.calls, ["move 1 0", "in -0", "right -0"]);
        let mut w = Log::new(0);
        update_controls(&mut w, [0.0; 4], &steer);
        assert_eq!(w.calls, ["move 0 0", "turn 0", "pitch 0"], "a quarter turn off: stopped, unturned");
    }

    #[test]
    fn a_fight_or_a_fresh_swing_turns_the_player_to_the_target() {
        let me = [10.0, 0.0, 10.0];
        // straight along +x: heading pi/2 (HPR's heading is atan2(x, z))
        let t = [20.0, 3.0, 10.0];
        let fight = Client { auto_attack: true, target: Some((t, me)), ..Client::default() };
        assert_eq!(run(0, [0.0; 4], fight).0, ["move 0 0", "turn 1", "pitch 0"]);
        assert_eq!(run(3, [0.0; 4], fight).0, ["vmove 0 0", "heading 1.571", "in -0", "right -0"]);
        // facing it already (within 10 degrees): no turn
        let mut w = Log::new(0);
        w.heading = 1.6;
        update_controls(&mut w, [0.0; 4], &fight);
        assert_eq!(w.calls[1], "turn 0");
        // the difference wraps round: from 3 to a target at -3 the short way
        // is +0.28 (unwrapped it would be -6, a turn the other way)
        let mut w = Log::new(0);
        w.heading = 3.0;
        let at = [10.0 + (-3.0f32).sin(), 0.0, 10.0 + (-3.0f32).cos()];
        let back = Client { target: Some((at, [10.0, 0.0, 10.0])), ..fight };
        update_controls(&mut w, [0.0; 4], &back);
        assert_eq!(w.calls[1], "turn 1");
        // a swing a moment ago does it too, but not while the stick orbits
        let swung = Client { auto_attack: false, swung: true, ..fight };
        assert_eq!(run(0, [0.0; 4], swung).0[1], "turn 1");
        assert_eq!(run(0, [0.5, 0.0, 0.0, 0.0], swung).0[1], "turn -0.5");
        // and nothing without a fight or a swing
        assert_eq!(run(0, [0.0; 4], Client { auto_attack: false, ..fight }).0[1], "turn 0");
    }

    /// The keyboard's cursor block (`m_keys[17..=24]`) as the game reads it:
    /// up walks forward, left turns left in first person and the fixed
    /// follow and strafes left in the follow orbit, Home and End orbit it,
    /// Page Down and Page Up pitch (zoom it); with a chat line up they do
    /// nothing (the control: the same keys, no calls moved).
    #[test]
    fn the_keyboard_axes_walk_and_turn_as_the_game_reads_them() {
        let key = |held: &[usize]| {
            let mut k = [0.0; 8];
            for &i in held {
                k[i - 17] = 1.0;
            }
            Client { keys: k, ..Client::default() }
        };
        // up and left (19, 17): forward, turning left; page down (22): pitch down
        assert_eq!(run(0, [0.0; 4], key(&[19, 17, 22])).0, ["move 1 0", "turn 1", "pitch 1"]);
        assert_eq!(run(2, [0.0; 4], key(&[20, 18])).0, ["move -1 0", "turn -1", "pitch 0"]);
        // mode 3: left strafes left, home orbits one way and end the
        // other, page up zooms out
        assert_eq!(run(3, [0.0; 4], key(&[17, 24, 21])).0, ["vmove 0 -1", "in 1", "right -1"]);
        assert_eq!(run(3, [0.0; 4], key(&[18, 23])).0, ["vmove 0 1", "in -0", "right 1"]);
        // the sticks and the keys add
        assert_eq!(run(0, [0.0, 0.0, 0.0, -0.5], key(&[19])).0[0], "move 1.5 0");
        let chat = Client { has_chat: true, ..key(&[19, 17, 22, 24]) };
        assert_eq!(run(0, [0.0; 4], chat).0, ["move -0 0", "turn -0", "pitch 0"]);
        assert_eq!(run(3, [0.0; 4], chat).0, ["vmove -0 0", "in -0", "right 0"]);
    }

    /// The client's own steer (the mouse, `Client::steer`): mode 3 walks on
    /// the body's own axes (modes 0 and 2's calls) and no mode turns to the
    /// target; the stick's zoom and orbit stay, the cast walks. The controls:
    /// without it the same sticks are the game's view-relative move and turn.
    #[test]
    fn the_mouse_steer_walks_mode_3_on_the_body_and_turns_to_no_target() {
        let steer = Client { steer: true, ..Client::default() };
        assert_eq!(run(3, STICKS, steer).0, ["move 1 -0.75", "in -0.25", "right 0.5"]);
        assert_eq!(run(3, STICKS, Client::default()).0[0], "vmove 1 -0.75");
        assert_eq!(run(3, STICKS, Client { casting: true, ..steer }).0[0], "walk 1 -0.75");
        // a fight: the game turns to the target; steered, the mouse's heading stays
        let me = [10.0, 0.0, 10.0];
        let fight = Client { auto_attack: true, target: Some(([20.0, 3.0, 10.0], me)), ..Client::default() };
        assert_eq!(run(3, [0.0; 4], fight).0, ["vmove 0 0", "heading 1.571", "in -0", "right -0"]);
        assert_eq!(run(3, [0.0; 4], Client { steer: true, ..fight }).0, ["move 0 0", "in -0", "right -0"]);
        assert_eq!(run(0, [0.0; 4], fight).0[1], "turn 1");
        assert_eq!(run(0, [0.0; 4], Client { steer: true, ..fight }).0[1], "turn 0");
        // both buttons (the stick's y all the way up) with A: forward and left
        assert_eq!(run(3, [0.0, 0.0, -1.0, -1.0], steer).0[0], "move 1 -1");
    }

    #[test]
    fn the_chat_line_up_leaves_the_sticks_as_they_are() {
        // the PS2's keyboard axes are left out then; the sticks are the same
        let c = Client { has_chat: true, ..Client::default() };
        assert_eq!(run(0, STICKS, c).0, ["move 1 -0.75", "turn -0.5", "pitch 0.25"]);
        assert_eq!(run(3, STICKS, c).0, ["vmove 1 -0.75", "in -0.25", "right 0.5"]);
    }
}
