//! The macros running, as PCSX2 runs its Macro Buttons (the user, 2026-11-06:
//! "pcsx2 has a nice macro system for keys/buttons. lets make a similar
//! native solution"; 2026-11-10: "we are using pcsx2 macro system, but it is
//! not the same yet"): `Pad::SetMacroButtonState`, `ApplyMacroButton` and
//! `UpdateMacroButtons` (`pcsx2/SIO/Pad/Pad.cpp`) over the pad the game reads.
//! What a macro is, `binds::Macro`; the page that sets them, `macropage`;
//! `claude-notes/vanilla-macros-pcsx2-20261110.md`.
//!
//! A macro runs while one of its triggers is held, each of the trigger's
//! inputs further than the macro's Deadzone (PCSX2's `value > deadzone`: a
//! key or a button at any deadzone under 100%, a stick's way or a trigger's
//! pull past it), or with Press To Toggle from one such press to the next.
//! Running, it presses its buttons and sticks' ways at its Pressure, and with
//! a Frequency of N lets them go and presses them again every N fields
//! (PCSX2's VSyncs: N on, N off).
//!
//! The game reads its pad once a frame of its own, and the console's frame is
//! two fields (`frames.rs`): a field at each of the game's frames and one
//! half way between. The macros keep that clock, not the frames drawn: what
//! the triggers did since the game's last frame, and the fields since, are
//! taken in their order at its next (`frame`), and what the macros press
//! changes there, where a PS2's game would read it. So a frequency is as
//! fast at 165 Hz as at 30, and the game sees what it sees on a PS2: 2 a
//! press every other frame (15 a second), 4 seven and a half, 3 ten (on two
//! frames, off one), 60 one a second, and 1 (two turns a frame) held, or
//! nothing, by the half of the frame the trigger came in.
//!
//! The pad the game reads is set as PCSX2's is (`ApplyMacroButton`'s
//! `pad->Set`, as every input's binding sets it): the last to change a
//! button or a stick's way has it, a controller's or a macro's. So a macro
//! letting go lets go of a button the player holds too, until the player
//! presses it again, and a stick the player moves takes back the way a macro
//! pushed. Every trigger's input is the triggers' alone (`binds`: out of the
//! game's fields, rows and stick), so they meet only where the player holds
//! what a macro also presses. The game sees ordinary presses: nothing new
//! goes to a server.

use crate::binds::{Binds, Input, MACROS, Macro, axes, ways};
use crate::frames::FRAME_MS;
use crate::input::{Report, bit};

/// The fields to each of the game's frames: the console's frame is two of
/// NTSC's (`frames.rs`, 59.94 a second; the client's game frame is a
/// thirtieth, so a field is 16.7 ms, 60 a second).
pub const FIELDS_PER_FRAME: u32 = 2;

/// A trigger pulled less than this is at rest to the macros (the host's,
/// as its sticks have one: `input::STICK_DEAD_ZONE`): Microsoft's
/// `XINPUT_GAMEPAD_TRIGGER_THRESHOLD`, 30 of 255. PCSX2 reads a trigger raw,
/// so a pad resting a hair off its end would run a macro of deadzone 0.
pub const TRIGGER_REST: f32 = 30.0 / 255.0;

/// The controllers' inputs the triggers read this frame: their buttons as
/// they hold them (physical, before any binding, but those a capture
/// holds back: `PadFrame::held`), their sticks (rx, ry, lx, ly, +y down,
/// through the host's dead zone) and their triggers' pull (L2, R2, 0..1;
/// none where a controller does not say it, and then the button itself).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Held {
    pub buttons: u16,
    pub sticks: [f32; 4],
    pub triggers: [f32; 2],
}

/// What a macro set going (`ApplyMacroButton`), let go of as it was set
/// though the macro changed since.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Applied {
    buttons: u16,
    ways: u8,
    value: f32,
}

/// The macros of one window (a multibox window runs its own).
#[derive(Clone, Debug, Default)]
pub struct Run {
    /// the triggers' keys held, by name
    keys: Vec<String>,
    /// the controllers' inputs as last read
    held_in: Held,
    /// by macro: a trigger wholly held (as last judged); running
    /// (`trigger_state`); pressing now (`toggle_state`); the fields to its
    /// next turn (`toggle_counter`); what it set going
    held: [bool; MACROS],
    on: [bool; MACROS],
    out: [bool; MACROS],
    counter: [u16; MACROS],
    applied: [Option<Applied>; MACROS],
    /// the triggers' edges since the game's last frame, in order: (when,
    /// macro, held)
    edges: Vec<(f64, usize, bool)>,
    /// when the game's last frame was (ms)
    last_frame: Option<f64>,
    /// the macros' turns at this frame, in order: (macro, pressing)
    queue: Vec<(usize, bool)>,
    /// the pad as the game is to read it (PCSX2's `PadDualshock2` state):
    /// its buttons (fields) and its sticks' eight ways
    pad_buttons: u16,
    pad_ways: [f32; 8],
    /// the controllers', the touch screen's and a script's pad as last seen
    /// (each change of it sets the pad, as its binding sets PCSX2's)
    src_buttons: u16,
    src_ways: [f32; 8],
    /// the macros as they were set last frame: a change lets them all go
    config: [Macro; MACROS],
    /// a macro was set up since the window opened (`connects`)
    connects: bool,
}

impl Run {
    /// A trigger's key pressed (`down`) or let go at `now` (ms, the frames'
    /// clock), by name (`binds::Sent::Macro`; its repeats are not keys): the
    /// macros it is in judged then, as PCSX2 judges at each event; what they
    /// do comes at the game's next frame.
    pub fn key(&mut self, name: &str, down: bool, now: f64, b: &Binds) {
        self.sync(b);
        let at = self.keys.iter().position(|k| k == name);
        match (down, at) {
            (true, None) => self.keys.push(name.to_string()),
            (false, Some(i)) => {
                self.keys.swap_remove(i);
            }
            _ => return,
        }
        self.judge(now, b);
    }

    /// One frame drawn at `now` (ms): `held` the controllers' inputs for the
    /// triggers, `src` the pad from every source as the game would read it
    /// without the macros, `game_frame` whether this frame is one of the
    /// game's (its edges and fields taken), `capturing` whether a page waits
    /// for an input (no macro runs). The pad the game reads.
    pub fn frame(&mut self, now: f64, game_frame: bool, capturing: bool, held: &Held, src: &Report, b: &Binds) -> Report {
        self.sync(b);
        // each source's change sets the pad (`SetControllerState`)
        let changed = src.buttons ^ self.src_buttons;
        self.pad_buttons = (self.pad_buttons & !changed) | (src.buttons & changed);
        self.src_buttons = src.buttons;
        let w = ways(src.sticks);
        for (i, &v) in w.iter().enumerate() {
            if v != self.src_ways[i] {
                self.pad_ways[i] = v;
            }
        }
        self.src_ways = w;
        self.held_in = *held;
        if capturing {
            self.let_go();
        } else {
            self.judge(now, b);
            if game_frame {
                self.game_frame(now, b);
            }
        }
        for (m, on) in std::mem::take(&mut self.queue) {
            self.apply(m, on, &b.macros[m]);
        }
        if b.macros.iter().any(Macro::set_up) {
            self.connects = true;
        }
        Report { buttons: self.pad_buttons, sticks: axes(self.pad_ways), triggers: src.triggers }
    }

    /// Whether a macro has been set up since the window opened: from then on
    /// the pad reads as connected (as the touch screen's, once used), so a
    /// macro run from the keyboard presses the game's pad with no controller
    /// there, from its first press (libpad finds a pad a read after it is
    /// there), and the game never sees its pad come and go.
    pub fn connects(&self) -> bool {
        self.connects
    }

    /// Every macro off and every trigger let go (the window lost the keys, a
    /// page waits for an input): what each pressed let go of at once.
    pub fn let_go(&mut self) {
        self.queue.clear();
        self.edges.clear();
        for m in 0..MACROS {
            if let Some(a) = self.applied[m].take() {
                self.release(a);
            }
            (self.held[m], self.on[m], self.out[m], self.counter[m]) = (false, false, false, 0);
        }
        self.keys.clear();
    }

    /// The macros changed (the Macros page, a window reading another's
    /// keys.opt): every one let go first, as PCSX2 reloads them.
    fn sync(&mut self, b: &Binds) {
        if self.config != b.macros {
            self.let_go();
            self.config = b.macros.clone();
        }
    }

    /// Each macro's triggers judged at `now`: one wholly held, or none,
    /// since the last judging is an edge, taken at the game's next frame.
    fn judge(&mut self, now: f64, b: &Binds) {
        for m in 0..MACROS {
            let mac = &b.macros[m];
            let dz = f32::from(mac.deadzone) / 100.0;
            let held = mac.triggers.iter().any(|c| !c.is_empty() && c.iter().all(|i| self.value(i) > dz));
            if held != self.held[m] {
                self.held[m] = held;
                self.edges.push((now, m, held));
            }
        }
    }

    /// One of the game's frames at `now`: the edges since its last frame
    /// and the two fields since (the one half way, `FRAME_MS` / 2 after the
    /// last frame, and its own) taken in their order.
    fn game_frame(&mut self, now: f64, b: &Binds) {
        let mut mid = self.last_frame.map(|t| t + FRAME_MS / 2.0);
        for (t, m, held) in std::mem::take(&mut self.edges) {
            if mid.is_some_and(|mid| t >= mid) {
                self.fields(b);
                mid = None;
            }
            self.edge(m, held, &b.macros[m]);
        }
        if mid.is_some() {
            self.fields(b);
        }
        self.fields(b);
        self.last_frame = Some(now);
    }

    /// A field: each macro's frequency counted.
    fn fields(&mut self, b: &Binds) {
        for m in 0..MACROS {
            self.field(m, &b.macros[m]);
        }
    }

    /// How far an input of a trigger is pressed, 0..1: a key or a button
    /// all or nothing, L2 and R2 by their pull where the controller says it,
    /// a stick's way by its push.
    fn value(&self, i: &Input) -> f32 {
        match i {
            Input::Key(k) => f32::from(u8::from(self.keys.iter().any(|h| h == k))),
            Input::Button(p) => {
                let pressed = f32::from(u8::from(self.held_in.buttons & (1 << (p & 15)) != 0));
                let pull = match 1u16 << (p & 15) {
                    bit::L2 => self.held_in.triggers[0],
                    bit::R2 => self.held_in.triggers[1],
                    _ => 0.0,
                };
                if pull >= TRIGGER_REST { pull.min(1.0) } else { pressed }
            }
            Input::Stick(w) => ways(self.held_in.sticks)[(w & 7) as usize],
        }
    }

    /// `SetMacroButtonState`: a trigger came to be held (`now`) or let go.
    /// Held, it starts the macro, or with Press To Toggle turns it; let go,
    /// it stops one that is held. Starting or stopping, the frequency's
    /// count starts again; the macro presses or lets go there.
    fn edge(&mut self, m: usize, now: bool, mac: &Macro) {
        // a macro that presses nothing takes no notice of its trigger
        if !mac.presses() {
            return;
        }
        let state = if mac.toggle {
            if now { !self.on[m] } else { self.on[m] }
        } else {
            now
        };
        if state == self.on[m] {
            return;
        }
        self.counter[m] = mac.frequency;
        self.on[m] = state;
        if self.out[m] != state {
            self.out[m] = state;
            self.queue.push((m, state));
        }
    }

    /// `UpdateMacroButtons`, at one field: a running macro with a frequency
    /// counts it down, and at nothing turns what it presses and counts again.
    fn field(&mut self, m: usize, mac: &Macro) {
        if !self.on[m] || mac.frequency == 0 {
            return;
        }
        self.counter[m] = self.counter[m].saturating_sub(1);
        if self.counter[m] > 0 {
            return;
        }
        self.counter[m] = mac.frequency;
        self.out[m] = !self.out[m];
        self.queue.push((m, self.out[m]));
    }

    /// `ApplyMacroButton`: each of the macro's buttons and ways set, to its
    /// pressure (a button pressed) or to nothing.
    fn apply(&mut self, m: usize, on: bool, mac: &Macro) {
        if on {
            let a = Applied { buttons: mac.buttons, ways: mac.ways, value: f32::from(mac.pressure) / 100.0 };
            self.pad_buttons |= a.buttons;
            for (w, v) in self.pad_ways.iter_mut().enumerate() {
                if a.ways & (1 << w) != 0 {
                    *v = a.value;
                }
            }
            self.applied[m] = Some(a);
        } else if let Some(a) = self.applied[m].take() {
            self.release(a);
        }
    }

    /// What a macro set going let go of.
    fn release(&mut self, a: Applied) {
        self.pad_buttons &= !a.buttons;
        for (w, v) in self.pad_ways.iter_mut().enumerate() {
            if a.ways & (1 << w) != 0 {
                *v = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binds::{BUTTONS, Binds, Input, Macro, bit_index};
    use crate::input::bit;

    fn macros(m: Macro) -> Binds {
        let mut b = Binds::default();
        b.macros[0] = m;
        b
    }

    fn key(k: &str) -> Input {
        Input::Key(k.to_string())
    }

    fn pad(b: u16) -> Input {
        Input::Button(bit_index(b).unwrap())
    }

    /// A window's macros on a clock: `step` draws a frame, `game` one of
    /// the game's (a thirtieth of a second on), `key` a key between.
    struct Clock {
        r: Run,
        t: f64,
    }

    impl Clock {
        fn new() -> Self {
            Clock { r: Run::default(), t: 0.0 }
        }

        /// A key at `dt` ms on.
        fn key(&mut self, k: &str, down: bool, dt: f64, b: &Binds) {
            self.t += dt;
            self.r.key(k, down, self.t, b);
        }

        /// The game's next frame, the controllers holding `buttons` and the
        /// sources' pad `src`: the pad's buttons.
        fn game(&mut self, buttons: u16, src: u16, b: &Binds) -> u16 {
            self.t = (self.t / FRAME_MS).floor() * FRAME_MS + FRAME_MS;
            self.r.frame(self.t, true, false, &Held { buttons, ..Held::default() }, &Report { buttons: src, ..Report::default() }, b).buttons
        }

        /// A frame drawn `dt` ms on that is not one of the game's.
        fn drawn(&mut self, dt: f64, buttons: u16, b: &Binds) -> u16 {
            self.t += dt;
            self.r.frame(self.t, false, false, &Held { buttons, ..Held::default() }, &Report::default(), b).buttons
        }
    }

    /// Held: the buttons from the game's frame after the key, through the
    /// frames drawn between, let go at its frame after the key is let go; a
    /// controller's button triggers it the same (a second trigger), and
    /// while either is held it stays on. The control: a macro with no
    /// buttons presses none.
    #[test]
    fn a_held_macro_presses_its_buttons_while_held() {
        let b = macros(Macro { triggers: vec![vec![key("y")], vec![pad(bit::L3)]], buttons: bit::CROSS | bit::CIRCLE, ..Macro::default() });
        let mut c = Clock::new();
        assert_eq!(c.game(0, 0, &b), 0, "nothing held");
        c.key("y", true, 5.0, &b);
        assert_eq!(c.drawn(1.0, 0, &b), 0, "not before the game's frame");
        assert_eq!(c.game(0, 0, &b), bit::CROSS | bit::CIRCLE);
        assert_eq!(c.drawn(6.0, 0, &b), bit::CROSS | bit::CIRCLE, "a frame drawn between the game's");
        c.key("y", false, 1.0, &b);
        assert_eq!(c.drawn(1.0, 0, &b), bit::CROSS | bit::CIRCLE, "let go: still pressed till the game's frame");
        assert_eq!(c.game(0, 0, &b), 0);
        // the button, then the key too, then the button let go: on all the while
        assert_eq!(c.game(bit::L3, 0, &b), bit::CROSS | bit::CIRCLE);
        c.key("y", true, 3.0, &b);
        assert_eq!(c.game(0, 0, &b), bit::CROSS | bit::CIRCLE, "the key still holds it");
        c.key("y", false, 3.0, &b);
        assert_eq!(c.game(0, 0, &b), 0);
        // the control
        let none = macros(Macro { triggers: vec![vec![key("y")]], ..Macro::default() });
        let mut c = Clock::new();
        c.key("y", true, 1.0, &none);
        assert_eq!(c.game(0, 0, &none), 0);
    }

    /// A chord (PCSX2's `&`): every input held, in any order, runs it; any
    /// one let go stops it; Press To Toggle turns it each time the chord is
    /// wholly held. Two triggers are either one (PCSX2's list of bindings).
    /// The controls: a chord's parts one at a time do nothing, and the other
    /// trigger's alone runs it.
    #[test]
    fn a_chord_runs_only_wholly_held_and_either_trigger_runs_it() {
        let b = macros(Macro { triggers: vec![vec![pad(bit::L1), key("y")], vec![pad(bit::R3)]], buttons: bit::SQUARE, ..Macro::default() });
        let mut c = Clock::new();
        assert_eq!(c.game(bit::L1, 0, &b), 0, "L1 alone");
        c.key("y", true, 2.0, &b);
        assert_eq!(c.game(bit::L1, 0, &b), bit::SQUARE, "L1 and Y");
        assert_eq!(c.game(0, 0, &b), 0, "L1 let go");
        assert_eq!(c.game(bit::L1, 0, &b), bit::SQUARE, "pressed again, Y still held");
        c.key("y", false, 2.0, &b);
        assert_eq!(c.game(bit::L1, 0, &b), 0, "Y let go");
        assert_eq!(c.game(0, 0, &b), 0);
        c.key("y", true, 2.0, &b);
        c.key("y", false, 2.0, &b);
        assert_eq!(c.game(0, 0, &b), 0, "Y alone, pressed and let go");
        assert_eq!(c.game(bit::R3, 0, &b), bit::SQUARE, "the other trigger");
        assert_eq!(c.game(bit::R3 | bit::L1, 0, &b), bit::SQUARE);
        assert_eq!(c.game(0, 0, &b), 0);
        // toggled: on as the chord is complete, still on as it breaks, off
        // at the next time it is complete
        let mut t = b.clone();
        t.macros[0].toggle = true;
        let mut c = Clock::new();
        assert_eq!(c.game(bit::L1, 0, &t), 0);
        c.key("y", true, 2.0, &t);
        assert_eq!(c.game(bit::L1, 0, &t), bit::SQUARE);
        c.key("y", false, 2.0, &t);
        for _ in 0..5 {
            assert_eq!(c.game(0, 0, &t), bit::SQUARE, "on, let go");
        }
        c.key("y", true, 2.0, &t);
        assert_eq!(c.game(0, 0, &t), bit::SQUARE, "Y alone does not turn it");
        assert_eq!(c.game(bit::L1, 0, &t), 0, "the chord again: off");
        assert_eq!(c.game(0, 0, &t), 0);
    }

    /// An axis in a trigger counts past the macro's Deadzone (PCSX2's
    /// `value > deadzone`): a stick's way pushed further than it, a
    /// trigger's pull further than it (where the controller says its pull;
    /// else its button), a key or a button at any deadzone but 100%. The
    /// controls: under the deadzone, and the stick's other way, run nothing.
    #[test]
    fn an_axis_counts_past_the_deadzone() {
        let mut b = macros(Macro { triggers: vec![vec![Input::Stick(4)]], buttons: bit::R1, deadzone: 40, ..Macro::default() });
        let mut r = Run::default();
        let mut t = 0.0;
        let mut frame = |r: &mut Run, held: Held, b: &Binds| {
            t += FRAME_MS;
            r.frame(t, true, false, &held, &Report::default(), b).buttons
        };
        let stick = |ry: f32| Held { sticks: [0.0, ry, 0.0, 0.0], ..Held::default() };
        assert_eq!(frame(&mut r, stick(-0.39), &b), 0, "R up under 40%");
        assert_eq!(frame(&mut r, stick(-0.41), &b), bit::R1, "past it");
        assert_eq!(frame(&mut r, stick(0.9), &b), 0, "R down: the other way");
        // a trigger's pull: L2 past 40% where the controller says it pulls
        b.macros[0].triggers = vec![vec![pad(bit::L2)]];
        let pull = |buttons: u16, p: f32| Held { buttons, triggers: [p, 0.0], ..Held::default() };
        assert_eq!(frame(&mut r, pull(bit::L2, 0.35), &b), 0, "pulled 35%, its button down (past a third)");
        assert_eq!(frame(&mut r, pull(bit::L2, 0.45), &b), bit::R1);
        assert_eq!(frame(&mut r, pull(0, 0.0), &b), 0);
        assert_eq!(frame(&mut r, pull(bit::L2, 0.0), &b), bit::R1, "a controller that says no pull: its button");
        // at deadzone 0 a trigger counts once pulled past its rest, a
        // twelfth of the way (the control: just over it)
        b.macros[0].deadzone = 0;
        assert_eq!(frame(&mut r, pull(0, 0.0), &b), 0);
        assert_eq!(frame(&mut r, pull(0, 0.1), &b), 0, "resting a hair off its end");
        assert_eq!(frame(&mut r, pull(0, 0.13), &b), bit::R1, "the control: past its rest");
        // a key at any deadzone under 100, never at 100
        for (dz, runs) in [(0, true), (99, true), (100, false)] {
            let k = macros(Macro { triggers: vec![vec![key("y")]], buttons: bit::R1, deadzone: dz, ..Macro::default() });
            let mut c = Clock::new();
            c.key("y", true, 1.0, &k);
            assert_eq!(c.game(0, 0, &k) != 0, runs, "deadzone {dz}");
        }
    }

    /// At a display of `hz`, the key pressed `phase` ms into a frame of the
    /// game's, `s` seconds on: the presses the game sees (the buttons as
    /// they are at each of its frames) and how long each lasts on the frames
    /// drawn, in ms.
    fn turbo(frequency: u16, hz: f64, phase: f64, s: f64) -> (usize, Vec<f64>) {
        let b = macros(Macro { triggers: vec![vec![key("y")]], buttons: bit::CROSS, frequency, ..Macro::default() });
        let mut r = Run::default();
        let mut f = crate::frames::Frames::default();
        let (mut presses, mut last, mut lengths, mut since, mut drawn) = (0, false, Vec::new(), 0.0, false);
        let (mut keyed, mut first) = (false, None);
        for i in 0..(hz * (s + 1.0)) as usize {
            let t = i as f64 * 1000.0 / hz;
            let g = f.due(t);
            if g && first.is_none() {
                first = Some(t);
            }
            // the key, `phase` into the game's first frame
            if !keyed && first.is_some_and(|f0| t >= f0 + phase) {
                r.key("y", true, first.unwrap() + phase, &b);
                keyed = true;
            }
            let on = r.frame(t, g, false, &Held::default(), &Report::default(), &b).buttons & bit::CROSS != 0;
            if !keyed || t > first.unwrap() + phase + s * 1000.0 {
                continue;
            }
            if g {
                presses += (on && !last) as usize;
                last = on;
            }
            if on && !drawn {
                since = t;
            } else if !on && drawn {
                lengths.push(t - since);
            }
            drawn = on;
        }
        (presses, lengths)
    }

    /// Frequency counts fields (PCSX2's VSyncs), two to each of the game's
    /// frames, and the game sees the macros at its frames: at 30, 60 and 165
    /// Hz alike the presses it sees are the same (2: on one frame, off one,
    /// 15 a second; 4: 7.5; 3: on two, off one, 10; 60: one a second), each
    /// press N fields of 16.7 ms on the frames drawn; 1, two turns a frame,
    /// is held when the trigger came in the first half of a frame and never
    /// pressed in the second, as on a PS2. The control: the fields counted at
    /// every frame drawn, 4 at 165 Hz pressed some forty times in a second.
    #[test]
    fn a_frequency_counts_fields_at_any_rate() {
        for hz in [30.0, 60.0, 165.0] {
            let tol = 1000.0 / hz + 5.0;
            for (f, want) in [(2, 15), (4, 8), (3, 10), (60, 1)] {
                let (presses, lengths) = turbo(f, hz, 5.0, 1.0);
                assert!(presses.abs_diff(want) <= 1, "{hz} Hz, frequency {f}: {presses} presses");
                if f % 2 == 0 && f < 60 {
                    for l in &lengths[1..] {
                        assert!((l - f as f64 * 1000.0 / 60.0).abs() <= tol, "{hz} Hz, frequency {f}: {lengths:?}");
                    }
                }
            }
            assert_eq!(turbo(1, hz, 5.0, 1.0), (1, vec![]), "{hz} Hz: 1 from the first half, held");
            assert_eq!(turbo(1, hz, 25.0, 1.0).0, 0, "{hz} Hz: 1 from the second half, never");
        }
        // the control: a field at every frame drawn
        let b = macros(Macro { triggers: vec![vec![key("y")]], buttons: bit::CROSS, frequency: 4, ..Macro::default() });
        let mut r = Run::default();
        r.key("y", true, 0.0, &b);
        let mut presses = 0;
        let mut last = false;
        for i in 0..165 {
            let on = r.frame(i as f64, true, false, &Held::default(), &Report::default(), &b).buttons & bit::CROSS != 0;
            presses += (on && !last) as usize;
            last = on;
        }
        assert!(presses > 15, "{presses}");
    }

    /// A stick's way bound is pushed as far as the macro's Pressure, and
    /// through `input::stick_byte` and `ScaleStick` the game reads the walk
    /// PCSX2 gives it: under 24% at rest, from 24% a walk, from 60% a run
    /// (`RUN_THRESHOLD` 0.7 on the stick's magnitude); a button pressed
    /// whatever its pressure. The control: at 100% the stick is all the
    /// way up.
    #[test]
    fn a_pressure_is_how_far_a_stick_is_pushed() {
        let read = |pressure: u8| -> (f32, u16) {
            let b = macros(Macro { triggers: vec![vec![key("y")]], buttons: bit::CROSS, ways: 1 << 0, pressure, ..Macro::default() });
            let mut c = Clock::new();
            c.key("y", true, 1.0, &b);
            c.game(0, 0, &b);
            let out = c.r.frame(c.t, false, false, &Held::default(), &Report::default(), &b);
            let ly = vi::pad::scale_stick::<vi::fpu::Ee>(crate::input::stick_byte(out.sticks[3]));
            (ly.0, out.buttons)
        };
        assert_eq!(crate::input::stick_byte(-0.5), 43, "PCSX2's byte: 127 - (0.5 x 1.33 x 255 = 169) / 2");
        for (p, want) in [(1, "rest"), (23, "rest"), (24, "walk"), (59, "walk"), (60, "run"), (100, "run")] {
            let (ly, buttons) = read(p);
            let got = if ly == 0.0 {
                "rest"
            } else if -ly <= 0.7 {
                "walk"
            } else {
                "run"
            };
            assert_eq!((got, buttons), (want, bit::CROSS), "pressure {p}%: {ly}");
        }
        assert_eq!(read(100).0, -1.0, "the control: all the way");
    }

    /// The pad is set as PCSX2 sets it, the last change of a button having
    /// it: a macro letting go lets go of a button the player holds, which
    /// is the player's again only once pressed again; the player letting go
    /// of a button a held macro presses lets go of it (until the macro
    /// turns); a stick the player moves takes back the way a macro pushed.
    /// The controls: a button only the player holds, and one only a macro
    /// presses, are each as they are, and so is a stick.
    #[test]
    fn the_pad_is_set_by_the_last_change() {
        let b = macros(Macro { triggers: vec![vec![key("y")]], buttons: bit::CROSS, ways: 1 << 1, ..Macro::default() });
        let mut c = Clock::new();
        assert_eq!(c.game(0, bit::CROSS | bit::R1, &b), bit::CROSS | bit::R1, "the player's");
        c.key("y", true, 2.0, &b);
        assert_eq!(c.game(0, bit::CROSS | bit::R1, &b), bit::CROSS | bit::R1);
        c.key("y", false, 2.0, &b);
        assert_eq!(c.game(0, bit::CROSS | bit::R1, &b), bit::R1, "the macro let ✕ go, the player's hold too");
        assert_eq!(c.game(0, bit::R1, &b), bit::R1);
        assert_eq!(c.game(0, bit::CROSS | bit::R1, &b), bit::CROSS | bit::R1, "pressed again: the player's");
        // the player lets go of what a running macro presses
        c.key("y", true, 2.0, &b);
        assert_eq!(c.game(0, bit::CROSS, &b), bit::CROSS);
        assert_eq!(c.game(0, 0, &b), 0, "the player's release has it");
        // a stick: the macro pushes L right; the player's stick moved takes it back
        c.key("y", false, 2.0, &b);
        c.game(0, 0, &b);
        c.key("y", true, 2.0, &b);
        let stick = |c: &mut Clock, lx: f32| {
            c.t += FRAME_MS;
            c.r.frame(c.t, true, false, &Held::default(), &Report { sticks: [0.0, 0.0, lx, 0.0], ..Report::default() }, &b).sticks[2]
        };
        assert_eq!(stick(&mut c, 0.0), 1.0, "the macro's push");
        assert_eq!(stick(&mut c, 0.4), 0.4, "the player's");
        assert_eq!(stick(&mut c, 0.0), 0.0, "and back at rest");
        // the controls
        let none = Binds::default();
        let mut d = Clock::new();
        for s in [bit::R2, 0, bit::R2 | bit::L1] {
            assert_eq!(d.game(0, s, &none), s);
        }
        let sticks = [0.3, -0.7, -1.0, 0.25];
        assert_eq!(d.r.frame(d.t + 1.0, false, false, &Held::default(), &Report { sticks, ..Report::default() }, &none).sticks, sticks);
    }

    /// A macro's buttons reach the game as a pad's: through `Console::poll`,
    /// `PadKeys` gives each its key down (as a controller's own press does:
    /// binds' `a_buttons_key_is_the_pads`), and each its key up when the
    /// macro is let go; set up, the macros say the pad is there, which an
    /// unplugged pad (read as nothing at all) shows they must.
    #[test]
    fn a_macros_buttons_reach_the_games_pad() {
        let b = macros(Macro { triggers: vec![vec![key("y")]], buttons: bit::CROSS | bit::CIRCLE, ..Macro::default() });
        let mut c = Clock::new();
        let mut console = crate::input::Console::new();
        let poll = |console: &mut crate::input::Console, buttons| console.poll(Report { buttons, ..Report::default() }, true, false, false);
        poll(&mut console, 0);
        c.key("y", true, 1.0, &b);
        let mut down = poll(&mut console, c.game(0, 0, &b));
        down.sort();
        let mut want = vec![(crate::binds::pad_key(bit::CIRCLE).unwrap(), true), (crate::binds::pad_key(bit::CROSS).unwrap(), true)];
        want.sort();
        assert_eq!(down, want);
        c.key("y", false, 1.0, &b);
        let up = poll(&mut console, c.game(0, 0, &b));
        assert!(up.len() == 2 && up.iter().all(|&(_, d)| !d), "{up:?}");
        assert!(c.r.connects(), "set up: the pad reads as there");
        assert!(!Run::default().connects(), "the control");
        // why it must: a pad not there (no controller, no macro set up, as
        // the viewer's `connected` was) reads as nothing, so a macro run
        // from the keyboard pressed nothing of the game's
        let mut unplugged = crate::input::Console::new();
        for _ in 0..3 {
            assert!(unplugged.poll(Report { buttons: bit::CROSS, ..Report::default() }, false, false, false).is_empty());
        }
    }

    /// Let go of every trigger (the window lost the keys), or a page
    /// waiting for an input, or the macros changed: every macro off, its
    /// buttons let go at once. The control: a toggled macro stays on frame
    /// after frame.
    #[test]
    fn letting_go_ends_every_macro() {
        let b = macros(Macro { triggers: vec![vec![key("y")]], buttons: bit::R1, toggle: true, ..Macro::default() });
        let mut c = Clock::new();
        c.key("y", true, 1.0, &b);
        c.key("y", false, 1.0, &b);
        for _ in 0..3 {
            assert_eq!(c.game(0, 0, &b), bit::R1, "the control: on");
        }
        c.r.let_go();
        assert_eq!(c.drawn(1.0, 0, &b), 0, "at once");
        c.key("y", true, 1.0, &b);
        assert_eq!(c.game(0, 0, &b), bit::R1);
        assert_eq!(c.r.frame(c.t + 1.0, false, true, &Held::default(), &Report::default(), &b).buttons, 0, "a capture");
        c.key("y", false, 1.0, &b);
        c.key("y", true, 1.0, &b);
        assert_eq!(c.game(0, 0, &b), bit::R1);
        let mut changed = b.clone();
        changed.macros[0].buttons = bit::L1;
        assert_eq!(c.drawn(1.0, 0, &changed), 0, "the macro changed: let go, R1 with it");
        assert!(BUTTONS.len() == 16);
    }
}
