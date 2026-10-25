//! What a window does as it comes to the front, said in one line of the log
//! (and so in a problem report, which carries the log's last lines): how
//! long before it came its last frame was drawn (a window whose frames had
//! stopped behind another takes the focus late), how long until its first
//! frame, the longest wait between two frames until its first press, that
//! press (a controller's or a key's) and how long after it came, and the keys
//! already down as it came, which it does not take (winit's synthetic
//! presses, `viewer.rs`: Alt+Tab's Tab). F3 20261025-111324: "First button
//! press after activating a window is often ignored ... when using
//! WinActivate via an AutoHotkey script"; and a second box whose "inputs are
//! ignored for like 1-2 seconds" (2026-10-25).
//!
//! winit sends a focus's synthetic presses just before `Focused(true)` on
//! Windows (both come out of WM_SETFOCUS, the keys first) and just after it
//! on X11, so a key counts as down at the focus within `NEAR` either side.

use std::time::Duration;
use web_time::Instant;

/// How long after coming to the front a window says what it saw, if no
/// press came first.
const QUIET: Duration = Duration::from_secs(5);

/// How far from the focus a synthetic press still counts as the focus's.
const NEAR: Duration = Duration::from_millis(250);

/// The most keys named as down at a focus.
const MOST_KEYS: usize = 6;

/// A window's coming to the front, until it is said.
#[derive(Debug, Default)]
pub struct Arrival {
    /// when it came to the front; none once said
    at: Option<Instant>,
    /// the first frame since, the last one, and the longest wait between two
    first_frame: Option<Duration>,
    last_frame: Option<Instant>,
    longest_gap: Duration,
    /// the keys down as it came (synthetic presses), not taken
    held: Vec<String>,
    /// synthetic presses before any focus (Windows sends them first)
    early: Vec<(Instant, String)>,
    /// the last frame drawn, whether or not it is in front, and how long
    /// before it came to the front that was
    drawn: Option<Instant>,
    before: Option<Duration>,
}

/// A duration as the line says it.
fn ms(d: Duration) -> String {
    format!("{} ms", d.as_millis())
}

impl Arrival {
    /// `Focused(true)`: from now, until said.
    pub fn focused(&mut self, now: Instant) {
        let early = std::mem::take(&mut self.early);
        let drawn = self.drawn;
        *self = Arrival { at: Some(now), drawn, before: drawn.map(|t| now.saturating_duration_since(t)), ..Default::default() };
        for (t, k) in early {
            if now.duration_since(t) <= NEAR {
                self.held_key(t, &k);
            }
        }
    }

    /// A key down as the window came to the front (a synthetic press).
    pub fn held_key(&mut self, now: Instant, name: &str) {
        match self.at {
            Some(at) if now.duration_since(at) <= NEAR => {
                if self.held.len() < MOST_KEYS && !self.held.iter().any(|h| h == name) {
                    self.held.push(name.to_string());
                }
            }
            _ => {
                self.early.retain(|(t, _)| now.duration_since(*t) <= NEAR);
                if self.early.len() < MOST_KEYS {
                    self.early.push((now, name.to_string()));
                }
            }
        }
    }

    /// A frame begun: the line, once nothing was pressed for `QUIET`.
    pub fn frame(&mut self, now: Instant) -> Option<String> {
        self.drawn = Some(now);
        let at = self.at?;
        if self.first_frame.is_none() {
            self.first_frame = Some(now.duration_since(at));
        }
        if let Some(l) = self.last_frame {
            self.longest_gap = self.longest_gap.max(now.duration_since(l));
        }
        self.last_frame = Some(now);
        (now.duration_since(at) >= QUIET).then(|| self.say(now, None))
    }

    /// A press (`what`: "the controller's cross", "the key 1"): the line,
    /// the first time since the window came to the front.
    pub fn press(&mut self, now: Instant, what: &str) -> Option<String> {
        self.at?;
        Some(self.say(now, Some(what)))
    }

    /// `Focused(false)` before a press or `QUIET`: the line.
    pub fn lost(&mut self, now: Instant) -> Option<String> {
        self.at?;
        let line = self.say(now, None);
        Some(format!("{line} (left the front)"))
    }

    fn say(&mut self, now: Instant, press: Option<&str>) -> String {
        let at = self.at.take().unwrap_or(now);
        let mut s = String::from("focus: in front; ");
        if let Some(d) = self.before {
            s += &format!("its last frame {} before, ", ms(d));
        }
        s += &match self.first_frame {
            Some(d) => format!("its first frame {} after", ms(d)),
            None => "no frame drawn".to_string(),
        };
        if self.longest_gap > Duration::ZERO {
            s += &format!(", the longest wait between frames {}", ms(self.longest_gap));
        }
        s += &match press {
            Some(what) => format!("; the first press {} after: {what}", ms(now.duration_since(at))),
            None => format!("; no press in {}", ms(now.duration_since(at))),
        };
        if !self.held.is_empty() {
            s += &format!("; down as it came, not taken: {}", self.held.join(", "));
        }
        s
    }
}

/// A key as the line names it: a named key by its name (Tab, F1, Alt,
/// Enter), any key that types a character only as that. The log goes out
/// with a problem report, and the first key typed into a window just
/// brought to the front can be a password's first letter (Station's login).
pub fn key_label(k: &winit::keyboard::Key) -> String {
    use winit::keyboard::Key;
    match k {
        Key::Named(n) => format!("the key {n:?}"),
        Key::Character(_) => "a character key".to_string(),
        _ => "a key".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    /// The line comes once an activation, at its first press: the first
    /// frame's wait, the longest between frames, the press and when. The
    /// control: a second press says nothing.
    #[test]
    fn a_window_says_its_first_press_once() {
        let t0 = Instant::now();
        let mut a = Arrival::default();
        assert_eq!(a.press(at(t0, 0), "the key F1"), None, "before any focus, nothing");
        a.focused(at(t0, 100));
        assert_eq!(a.frame(at(t0, 130)), None);
        assert_eq!(a.frame(at(t0, 163)), None);
        assert_eq!(a.frame(at(t0, 290)), None);
        let line = a.press(at(t0, 300), "the controller's cross, pressed and let go between two frames");
        assert_eq!(
            line.as_deref(),
            Some("focus: in front; its first frame 30 ms after, the longest wait between frames 127 ms; the first press 200 ms after: the controller's cross, pressed and let go between two frames")
        );
        assert_eq!(a.press(at(t0, 400), "a character key"), None, "the control: once an activation");
        assert_eq!(a.frame(at(t0, 6000)), None, "nor after it");
    }

    /// A window whose frames had stopped behind another says how long
    /// before it came its last one was drawn (its focus taken late, the
    /// presses meanwhile lost to it); one drawing on says its last frame was
    /// a frame ago. The control: a window that drew none before says
    /// nothing of it.
    #[test]
    fn a_window_says_how_long_before_it_came_it_last_drew() {
        let t0 = Instant::now();
        let mut a = Arrival::default();
        assert_eq!(a.frame(at(t0, 0)), None);
        a.focused(at(t0, 1500));
        assert_eq!(a.frame(at(t0, 1510)), None);
        assert_eq!(
            a.press(at(t0, 1600), "the controller's cross").as_deref(),
            Some("focus: in front; its last frame 1500 ms before, its first frame 10 ms after; the first press 100 ms after: the controller's cross")
        );
        assert_eq!(a.frame(at(t0, 1543)), None);
        a.focused(at(t0, 1560));
        assert_eq!(a.lost(at(t0, 1570)).as_deref(), Some("focus: in front; its last frame 17 ms before, no frame drawn; no press in 10 ms (left the front)"));
        let mut b = Arrival::default();
        b.focused(at(t0, 0));
        assert_eq!(b.lost(at(t0, 5)).as_deref(), Some("focus: in front; no frame drawn; no press in 5 ms (left the front)"), "the control");
    }

    /// A named key by its name; a character's key never by its character
    /// (the first key typed in a window can be a password's).
    #[test]
    fn a_key_is_named_but_never_its_character() {
        use winit::keyboard::{Key, NamedKey};
        assert_eq!(key_label(&Key::Named(NamedKey::F1)), "the key F1");
        assert_eq!(key_label(&Key::Named(NamedKey::Tab)), "the key Tab");
        for c in ["1", "p", "Q"] {
            let l = key_label(&Key::Character(c.into()));
            assert_eq!(l, "a character key");
            assert!(!l.contains(c), "the control: {c} not in {l}");
        }
    }

    /// The keys down as it came: on Windows their synthetic presses come
    /// just before the focus, on X11 just after; one long before or after
    /// is not the focus's. With no press the line comes `QUIET` on, or as
    /// the window leaves the front.
    #[test]
    fn keys_down_as_it_came_either_side_of_the_focus() {
        let t0 = Instant::now();
        let mut a = Arrival::default();
        a.held_key(at(t0, 0), "the key Tab");
        a.held_key(at(t0, 990), "the key F1");
        a.focused(at(t0, 1000));
        a.held_key(at(t0, 1010), "a character key");
        a.held_key(at(t0, 1010), "a character key");
        a.held_key(at(t0, 2000), "the key Escape");
        assert_eq!(a.frame(at(t0, 1033)), None);
        assert_eq!(a.frame(at(t0, 5999)), None);
        assert_eq!(
            a.frame(at(t0, 6000)).as_deref(),
            Some("focus: in front; its first frame 33 ms after, the longest wait between frames 4966 ms; no press in 5000 ms; down as it came, not taken: the key F1, a character key")
        );
        let mut b = Arrival::default();
        b.focused(at(t0, 0));
        assert_eq!(b.lost(at(t0, 20)).as_deref(), Some("focus: in front; no frame drawn; no press in 20 ms (left the front)"));
        assert_eq!(b.lost(at(t0, 30)), None, "said once");
    }
}
