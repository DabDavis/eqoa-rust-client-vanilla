//! The macros running (the user, 2026-11-06: "pcsx2 has a nice macro system
//! for keys/buttons. lets make a similar native solution"; what a macro is,
//! `binds::Macro`; PCSX2's `Pad::SetMacroButtonState` and
//! `UpdateMacroButtons`, `claude-notes/vanilla-macros-20261106.md`). Each
//! frame drawn, the buttons the macros press join the controllers' (after
//! the player's bindings, before `VIPad::Read`), so the game sees them as it
//! sees a pad's: nothing new goes to a server.
//!
//! A macro is on while a trigger (its key, its button) is held, or from one
//! press to the next when it toggles; let go mid-press, its buttons are let
//! go that frame. With a turbo of N its buttons are pressed for N of the
//! game's frames and let go for N, from the frame it came on (PCSX2 counts
//! VSyncs; here the game's 30 a second, `frames.rs`, so a turbo is as fast at
//! 165 Hz as at 30).

use crate::binds::{Binds, MACROS};

/// The macros of one window (a multibox window runs its own).
#[derive(Clone, Debug, Default)]
pub struct Run {
    /// a trigger key held, by macro
    key: [bool; MACROS],
    /// a trigger button held, by macro (last frame's)
    button: [bool; MACROS],
    /// on, by macro
    on: [bool; MACROS],
    /// the game's frame it came on at, by macro
    since: [u64; MACROS],
    /// the game's frames counted
    frame: u64,
}

impl Run {
    /// Macro `m`'s key pressed (`down`) or let go: a press while its button
    /// is held, or a release while it is, changes nothing.
    pub fn key(&mut self, m: usize, down: bool, b: &Binds) {
        if m >= MACROS {
            return;
        }
        let was = self.key[m] || self.button[m];
        self.key[m] = down;
        self.edge(m, was, b);
    }

    /// One frame drawn: `held` the macros whose button is held (a bit each,
    /// `PadFrame::macros`), `game_frame` whether this frame is one of the
    /// game's. The game's buttons the macros press now.
    pub fn frame(&mut self, held: u16, game_frame: bool, b: &Binds) -> u16 {
        if game_frame {
            self.frame += 1;
        }
        let mut out = 0;
        for m in 0..MACROS {
            let was = self.key[m] || self.button[m];
            self.button[m] = held & (1 << m) != 0;
            self.edge(m, was, b);
            let mac = &b.macros[m];
            if !self.on[m] || mac.chord == 0 {
                continue;
            }
            let t = u64::from(mac.turbo);
            if t == 0 || ((self.frame - self.since[m]) / t).is_multiple_of(2) {
                out |= mac.chord;
            }
        }
        out
    }

    /// Every macro off and every trigger let go (the window lost the keys, a
    /// capture began).
    pub fn let_go(&mut self) {
        let frame = self.frame;
        *self = Run { frame, ..Run::default() };
    }

    /// A trigger of macro `m` changed (`was` held before): pressed, it comes
    /// on (or toggles); let go, a held macro goes off.
    fn edge(&mut self, m: usize, was: bool, b: &Binds) {
        let now = self.key[m] || self.button[m];
        if now == was {
            return;
        }
        let toggle = b.macros[m].toggle;
        match (now, toggle) {
            (true, false) => self.start(m),
            (true, true) => {
                if self.on[m] {
                    self.on[m] = false;
                } else {
                    self.start(m);
                }
            }
            (false, false) => self.on[m] = false,
            (false, true) => {}
        }
    }

    fn start(&mut self, m: usize) {
        self.on[m] = true;
        self.since[m] = self.frame;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binds::{BUTTONS, Binds, Macro};
    use crate::input::bit;

    fn macros(m: Macro) -> Binds {
        let mut b = Binds::default();
        b.macros[0] = m;
        b
    }

    /// Held: the chord while the key is held, let go the frame it is let go
    /// (mid-chord); the button triggers it the same, and while either is
    /// held it stays on. The control: a macro with no buttons presses none.
    #[test]
    fn a_held_macro_presses_its_chord_while_held() {
        let b = macros(Macro { key: Some("g".into()), button: Some(14), chord: bit::CROSS | bit::CIRCLE, ..Macro::default() });
        let mut r = Run::default();
        assert_eq!(r.frame(0, true, &b), 0, "nothing held");
        r.key(0, true, &b);
        assert_eq!(r.frame(0, true, &b), bit::CROSS | bit::CIRCLE);
        assert_eq!(r.frame(0, false, &b), bit::CROSS | bit::CIRCLE, "a frame drawn between the game's");
        r.key(0, false, &b);
        assert_eq!(r.frame(0, false, &b), 0, "let go mid-chord: let go that frame");
        // the button, then the key too, then the button let go: on all the while
        assert_eq!(r.frame(1, true, &b), bit::CROSS | bit::CIRCLE);
        r.key(0, true, &b);
        assert_eq!(r.frame(0, true, &b), bit::CROSS | bit::CIRCLE, "the key still holds it");
        r.key(0, false, &b);
        assert_eq!(r.frame(0, true, &b), 0);
        // the control
        let none = macros(Macro { key: Some("g".into()), ..Macro::default() });
        let mut r = Run::default();
        r.key(0, true, &none);
        assert_eq!(r.frame(0, true, &none), 0);
    }

    /// Toggled: on at a press, still on when let go, off at the next press.
    #[test]
    fn a_toggled_macro_runs_from_one_press_to_the_next() {
        let b = macros(Macro { key: Some("g".into()), chord: bit::SQUARE, toggle: true, ..Macro::default() });
        let mut r = Run::default();
        r.key(0, true, &b);
        r.key(0, false, &b);
        for _ in 0..10 {
            assert_eq!(r.frame(0, true, &b), bit::SQUARE);
        }
        r.key(0, true, &b);
        assert_eq!(r.frame(0, true, &b), 0, "the second press");
        r.key(0, false, &b);
        assert_eq!(r.frame(0, true, &b), 0);
    }

    /// The game's frames among the frames drawn at `hz` over one second.
    fn game_frames(hz: f64) -> Vec<bool> {
        let mut f = crate::frames::Frames::default();
        (0..hz as usize).map(|i| f.due(i as f64 * 1000.0 / hz)).collect()
    }

    /// A turbo of 2 presses for two of the game's frames and lets go for two
    /// from the frame it came on, at 30 Hz and at 165 Hz alike: over a second
    /// the game sees the chord on its frames 15 times and as many presses
    /// (eight); the drawn frames between the game's keep the game frame's
    /// state.
    #[test]
    fn a_turbo_steps_on_the_games_frames_at_any_rate() {
        let b = macros(Macro { key: Some("g".into()), chord: bit::CROSS, turbo: 2, ..Macro::default() });
        for hz in [30.0, 165.0] {
            let mut r = Run::default();
            r.key(0, true, &b);
            let (mut on_game, mut presses, mut last) = (0, 0, false);
            for g in game_frames(hz) {
                let on = r.frame(0, g, &b) & bit::CROSS != 0;
                if g {
                    on_game += on as i32;
                }
                presses += (on && !last) as i32;
                last = on;
            }
            assert_eq!(on_game, 15, "{hz} Hz: on for half the game's 30 frames");
            assert_eq!(presses, 8, "{hz} Hz: pressed 8 times in a second");
        }
    }

    /// A macro's buttons reach the game as a pad's: joined to the report
    /// `VIPad::Read` takes, `PadKeys` gives each its key down (as a
    /// controller's own press does: binds' `a_buttons_key_is_the_pads`), and
    /// each its key up when the macro is let go.
    #[test]
    fn a_macros_buttons_reach_the_games_pad() {
        let b = macros(Macro { key: Some("g".into()), chord: bit::CROSS | bit::CIRCLE, ..Macro::default() });
        let mut r = Run::default();
        let mut c = crate::input::Console::new();
        let poll = |c: &mut crate::input::Console, buttons| c.poll(crate::input::Report { buttons, sticks: [0.0; 4] }, true, false, false);
        poll(&mut c, 0);
        r.key(0, true, &b);
        let mut down = poll(&mut c, r.frame(0, true, &b));
        down.sort();
        let mut want = vec![(crate::binds::pad_key(bit::CIRCLE).unwrap(), true), (crate::binds::pad_key(bit::CROSS).unwrap(), true)];
        want.sort();
        assert_eq!(down, want);
        r.key(0, false, &b);
        let up = poll(&mut c, r.frame(0, true, &b));
        assert!(up.len() == 2 && up.iter().all(|&(_, d)| !d), "{up:?}");
    }

    /// Let go of every trigger (the window lost the keys): every macro off.
    #[test]
    fn letting_go_ends_every_macro() {
        let b = macros(Macro { key: Some("g".into()), chord: bit::R1, toggle: true, ..Macro::default() });
        let mut r = Run::default();
        r.key(0, true, &b);
        assert_eq!(r.frame(0, true, &b), bit::R1);
        r.let_go();
        assert_eq!(r.frame(0, true, &b), 0);
        assert!(BUTTONS.len() == 16);
    }
}
