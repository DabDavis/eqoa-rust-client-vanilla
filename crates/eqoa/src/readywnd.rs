//! The ready check's pop-up (the GM Jibrish's petition 1711: "/readycheck ...
//! pops up a window on their screen with "Are you ready?" and Yes or No. It
//! does not stop them from moving while the pop up is up. It expires after 30
//! seconds"): the client's own, as the game has none. One strip, a check the
//! server asked (go-eqoa-server's 0x7E4B, `net::readycheck::Check`), at the top
//! of the loot rolls' column (`rollwnd`, which then starts a strip lower):
//! "Are you ready?", Yes and No under it, the seconds left and a bar.
//!
//! It never takes the keys of play. A click (or a tap) on Yes or No answers;
//! with R2 held (the chat box's history, mode 33, where the walk and the
//! camera go on) right gives it the d-pad before the rolls (`take_focus`):
//! left and right the choice, ✕ answers, △ or R2 let go hands the d-pad back;
//! the keyboard's C held and its arrows and Enter as those. An answer closes it
//! at once and goes to the server (0x7E56); the server's end of the check (0
//! seconds left) closes it too, and a check past its time goes by itself.
//! Its clock is the session's (ms), not the game's frames.

use net::readycheck::Check;
use vi::ui::{FrameParam, Raster, Vui, icon_id, wide};

/// The strip's top left (the rolls' first place), its size, where its words
/// go and the choices' line and middles.
pub const AT: [i32; 2] = [436, 164];
pub const W: i32 = 196;
pub const H: i32 = 52;
const TEXT_X: i32 = 10;
const CHOICE_Y: i32 = 33;
const CHOICE_X: [i32; 2] = [65, 131];
const CHOICES: [&str; 2] = ["Yes", "No"];

/// A check asked and not answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Open {
    pub id: u32,
    pub asker: String,
    /// when it ends, on the session's clock (ms), and how long it was asked
    /// for (the bar's whole)
    pub ends: i64,
    pub length: i64,
}

impl Open {
    /// The seconds left at `now`, rounded up.
    fn seconds(&self, now: i64) -> i64 {
        ((self.ends - now).max(0) + 999) / 1000
    }
}

#[derive(Default)]
pub struct Ready {
    pub open: Option<Open>,
    /// the d-pad's (`take_focus`)
    focused: bool,
    /// the choice lit while focused: 0 Yes, 1 No
    choice: usize,
    /// where each choice was drawn last (frame x0 y0 x1 y1)
    hits: Vec<([i32; 4], usize)>,
}

impl Ready {
    /// A check from the server at `now`: a new one (or the same id again)
    /// opens, one of 0 seconds left closes the one of its id.
    pub fn check(&mut self, c: Check, now: i64) {
        if c.seconds == 0 {
            if self.open.as_ref().is_some_and(|o| o.id == c.id) {
                self.close();
            }
            return;
        }
        let length = c.seconds as i64 * 1000;
        self.open = Some(Open { id: c.id, asker: c.asker, ends: now + length, length });
        self.choice = 0;
    }

    /// Closed (answered, the server's end, or its time up).
    pub fn close(&mut self) {
        self.open = None;
        self.focused = false;
        self.hits.clear();
    }

    /// Past its time by a little: gone (the server ends every check itself;
    /// this only keeps a lost end from standing for good).
    pub fn expire(&mut self, now: i64) {
        if self.open.as_ref().is_some_and(|o| now > o.ends + 1000) {
            self.close();
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    pub fn focused(&self) -> bool {
        self.focused && self.is_open()
    }

    /// The d-pad to the strip (R2 held, then right), Yes lit. False when none
    /// is up.
    pub fn take_focus(&mut self) -> bool {
        if !self.is_open() {
            return false;
        }
        self.focused = true;
        self.choice = 0;
        true
    }

    /// The d-pad handed back (R2 let go, △).
    pub fn let_go(&mut self) {
        self.focused = false;
    }

    /// Choice `c` made: the strip gone, and what to send (the check's id,
    /// ready).
    fn make(&mut self, c: usize) -> Option<(u32, bool)> {
        let id = self.open.as_ref()?.id;
        self.close();
        Some((id, c == 0))
    }

    /// A key (the game's codes; the keyboard's arrows and Enter as the pad's)
    /// while focused: what to send when an answer was made. Unfocused, nothing.
    pub fn key(&mut self, key: i32, cues: &mut Vec<i32>) -> Option<(u32, bool)> {
        if !self.focused() {
            return None;
        }
        match key {
            136 | 17 => {
                cues.push(if self.choice > 0 { 11 } else { 9 });
                self.choice = 0;
                None
            }
            137 | 18 => {
                cues.push(if self.choice < 1 { 11 } else { 9 });
                self.choice = 1;
                None
            }
            134 | 13 => {
                cues.push(13);
                self.make(self.choice)
            }
            132 | 27 => {
                cues.push(8);
                self.let_go();
                None
            }
            _ => None,
        }
    }

    /// A pointer at frame point `p`: Yes or No, made (focused or not);
    /// anything else nothing.
    pub fn point(&mut self, p: [f32; 2]) -> Option<(u32, bool)> {
        let (x, y) = (p[0] as i32, p[1] as i32);
        let &(_, c) = self.hits.iter().find(|(r, _)| (r[0]..r[2]).contains(&x) && (r[1]..r[3]).contains(&y))?;
        self.make(c)
    }

    /// Whether frame point `p` is on the strip (a click there is the strip's,
    /// not the world's).
    pub fn covers(&self, p: [f32; 2]) -> bool {
        self.is_open() && p[0] >= AT[0] as f32 && p[0] < (AT[0] + W) as f32 && p[1] >= AT[1] as f32 && p[1] < (AT[1] + H) as f32
    }

    /// The strip at `now` (the session's clock), each choice's place noted.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, now: i64) {
        self.hits.clear();
        let Some(o) = self.open.as_ref() else { return };
        let (white, yellow) = ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 0.0, 1.0]);
        let fh = vui.font_height(r);
        let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) };
        let at = AT;
        r.blit(icon_id("black"), 0, at, [0, 0], [W, H], 3, if self.focused { 0.85 } else { 0.7 });
        vui.blit_frame(r, at, [W, H], &fp, 3, 1.0, true);
        // "Are you ready?" (the petition's words; the asker's name has no room
        // beside them at the game's font: the asker is the one told), and the
        // seconds left at the right
        let line = wide("Are you ready?");
        vui.draw_string_ex(r, &line, at[0] + TEXT_X, at[1] + 7, if self.focused { yellow } else { white }, true);
        let secs = wide(&format!("{}", o.seconds(now)));
        let sw = vui.string_width(r, &secs);
        vui.draw_string_ex(r, &secs, at[0] + W - 10 - sw, at[1] + 7, white, true);
        // the choices
        let y = at[1] + CHOICE_Y;
        for (c, label) in CHOICES.iter().enumerate() {
            let x = at[0] + CHOICE_X[c];
            let s = wide(label);
            let lit = self.focused && c == self.choice;
            vui.draw_centered_string_ex(r, &s, x, y, if lit { yellow } else { white }, 3, true);
            let half = vui.string_width(r, &s) / 2;
            if lit {
                vui.blit_ex(r, 89, [x - half - 20, y - 5], 3, 1.0, 0);
            }
            self.hits.push(([x - half - 14, y - fh / 2 - 4, x + half + 14, y + fh / 2 + 4], c));
        }
        // the time left: a bar along the strip's foot, red in its last 10 s
        let left = (o.ends - now).clamp(0, o.length);
        let full = W - 16;
        let w = if o.length > 0 { (full as i64 * left / o.length) as i32 } else { 0 };
        let (x0, y0) = (at[0] + 8, at[1] + H - 6);
        r.fill_fan([0.15, 0.15, 0.15, 1.0], [[x0, y0], [x0 + full, y0], [x0 + full, y0 + 2], [x0, y0 + 2]]);
        if w > 0 {
            let c = if o.seconds(now) <= 10 { [0.95, 0.25, 0.2, 1.0] } else { [0.3, 0.85, 0.3, 1.0] };
            r.fill_fan(c, [[x0, y0], [x0 + w, y0], [x0 + w, y0 + 2], [x0, y0 + 2]]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};

    fn check(id: u32, seconds: u8) -> Check {
        Check { id, seconds, asker: "Ann".into() }
    }

    #[test]
    fn a_check_opens_closes_and_expires() {
        let mut r = Ready::default();
        r.check(check(3, 30), 1000);
        assert_eq!(r.open.as_ref().map(|o| (o.id, o.ends)), Some((3, 31_000)));
        r.check(check(4, 0), 2000);
        assert!(r.is_open(), "another check's end: this one stays (the control)");
        r.check(check(3, 0), 2000);
        assert!(!r.is_open(), "its own end closes it");
        r.check(check(5, 30), 0);
        r.expire(31_000);
        assert!(r.is_open(), "its time and a little more");
        r.expire(31_001);
        assert!(!r.is_open(), "then gone");
    }

    #[test]
    fn the_dpad_only_while_focused() {
        let mut r = Ready::default();
        let mut cues = Vec::new();
        assert!(!r.take_focus(), "none up: nothing to focus");
        r.check(check(7, 30), 0);
        assert_eq!(r.key(134, &mut cues), None, "unfocused: no key is the strip's (the control)");
        assert!(r.is_open());
        assert!(r.take_focus());
        r.key(137, &mut cues);
        assert_eq!(r.key(134, &mut cues), Some((7, false)), "right, ✕: No");
        assert!(!r.is_open() && !r.focused());
        r.check(check(8, 30), 0);
        r.take_focus();
        r.key(137, &mut cues);
        r.key(136, &mut cues);
        assert_eq!(r.key(13, &mut cues), Some((8, true)), "back left, Enter: Yes");
        r.check(check(9, 30), 0);
        r.take_focus();
        assert_eq!(r.key(132, &mut cues), None);
        assert!(!r.focused() && r.is_open(), "△ lets go, answers nothing");
    }

    /// A click on Yes or No answers where it was drawn; nothing is found
    /// before a frame drew it (the control: no hit notes).
    #[test]
    fn a_pointer_answers_where_it_was_drawn() {
        let (vui, mut nr) = (vui(), NullRaster);
        let mut r = Ready::default();
        r.check(check(2, 30), 0);
        let no = [(AT[0] + CHOICE_X[1]) as f32, (AT[1] + CHOICE_Y) as f32];
        let yes = [(AT[0] + CHOICE_X[0]) as f32, (AT[1] + CHOICE_Y) as f32];
        assert_eq!(r.point(no), None, "not drawn yet");
        r.draw(&vui, &mut nr, 1000);
        assert!(r.covers(no) && !r.covers([10.0, 10.0]));
        assert_eq!(r.point([(AT[0] + 2) as f32, (AT[1] + 2) as f32]), None, "the strip, no choice");
        assert_eq!(r.point(no), Some((2, false)));
        assert!(!r.is_open());
        r.check(check(3, 30), 0);
        r.draw(&vui, &mut nr, 1000);
        assert_eq!(r.point(yes), Some((3, true)));
    }
}
