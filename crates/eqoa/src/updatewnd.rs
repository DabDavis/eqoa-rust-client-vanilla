//! The update's question (`update::Offer`) with what is new in the build it
//! offers: the game's own titled window (`VIUIWindow` in its set 0, as
//! `VIWndMessage` is), the new sections of the update notes in a framed list
//! with its scroll bar (`DrawFrameVScroll`, as `VITextList` draws one), the
//! question under it, and the choice row Restart now / Later, the one chosen
//! between the pulsing arrows (`|q..|w`), starting on Later.
//!
//! Its input: the d-pad's and the cursor keys' up and down (Page Up, Page
//! Down, Home and End too), a stick, the wheel and a drag on the list scroll
//! it; left and right choose, Enter and ✕ take the choice, Escape and △ are
//! Later; a click on a choice takes it at once (`pointer`).

use crate::update::{Section, WhatsNew};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// The window, and the rows in it: a heading, the list, the question, the
/// choices.
const WIDTH: i32 = 540;
const ROWS: i32 = 19;
const LIST_X: i32 = 34;
const LIST_W: i32 = WIDTH - 68;
const LIST_ROW: i32 = 1;
const LIST_ROWS: i32 = 12;
const ASK_ROW: i32 = LIST_ROW + LIST_ROWS + 1;
const CHOICE_ROW: i32 = ROWS - 2;
/// Rows a second a stick held all the way scrolls.
const STICK_RATE: f32 = 10.0;
/// Where the release's notes are read when the game cannot show them.
const RELEASE: &str = "github.com/DabDavis/eqoa-rust-client-vanilla/releases";

/// The window's answer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Answer {
    Restart,
    Later,
}

/// A line of the list, wrapped: its text (the UI's, `ui_text`), whether it
/// is a section's title, and its indent (a bullet's later lines).
#[derive(Clone, Debug)]
struct Line {
    text: Vec<u16>,
    title: bool,
    x: i32,
}

#[derive(Default)]
pub struct NotesWnd {
    window: Window,
    notes: WhatsNew,
    world: bool,
    /// the list's lines as last wrapped (None: to wrap at the next draw,
    /// where the font is)
    lines: Option<Vec<Line>>,
    /// the list's first row shown (between rows while a stick or a drag
    /// moves it)
    top: f32,
    /// 0 Restart now, 1 Later
    sel: i32,
    /// the choices as last drawn (frame x0 y0 x1 y1), Restart now first
    choices: [[i32; 4]; 2],
    /// the list's rectangle as last drawn (frame x0 y0 x1 y1) and its rows'
    /// height
    list: [i32; 4],
    row_h: i32,
    /// a press on the list: the pointer's y and the top then
    drag: Option<(f32, f32)>,
    cues: Vec<i32>,
}

/// The notes as the UI's text: a `|` doubled, the pad's ✕ △ ○ □ its inline
/// glyphs (`|x |t |c |s`).
pub fn ui_text(s: &str) -> Vec<u16> {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '|' => o.push_str("||"),
            '✕' => o.push_str("|x"),
            '△' => o.push_str("|t"),
            '○' => o.push_str("|c"),
            '□' => o.push_str("|s"),
            c => o.push(c),
        }
    }
    wide(&o)
}

/// `text` broken into lines no wider than `width` (at a space, else at a
/// character), measured as the window measures (`TextWidth`, the inline
/// glyphs counted).
fn wrap(w: &Window, vui: &Vui, r: &dyn Raster, text: &[u16], width: i32) -> Vec<Vec<u16>> {
    let mut out = Vec::new();
    let mut cur: Vec<u16> = Vec::new();
    for word in text.split(|&c| c == b' ' as u16).filter(|w| !w.is_empty()) {
        let mut next = cur.clone();
        if !next.is_empty() {
            next.push(b' ' as u16);
        }
        next.extend_from_slice(word);
        if cur.is_empty() || w.text_width(vui, r, &next) <= width {
            cur = next;
        } else {
            out.push(std::mem::take(&mut cur));
            cur = word.to_vec();
        }
        // a word wider than the line on its own: cut after the last '/'
        // that fits (an address), else at the character
        while w.text_width(vui, r, &cur) > width && cur.len() > 1 {
            let mut k = cur.len() - 1;
            while k > 1 && w.text_width(vui, r, &cur[..k]) > width {
                k -= 1;
            }
            if let Some(j) = cur[..k].iter().rposition(|&c| c == b'/' as u16).filter(|&j| j > 0) {
                k = j + 1;
            }
            out.push(cur[..k].to_vec());
            cur = cur[k..].to_vec();
        }
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

impl NotesWnd {
    /// Put up for `notes`, in the world or not: the list at its top, the
    /// choice on Later, the window opening (`OnMessage` 295).
    pub fn open(&mut self, notes: WhatsNew, world: bool) {
        self.notes = notes;
        self.world = world;
        self.lines = None;
        self.top = 0.0;
        self.sel = 1;
        self.drag = None;
        self.window.close(true);
        self.window.open(false);
    }

    /// The choices as last drawn, Restart now first (tests).
    #[cfg(test)]
    pub fn choices(&self) -> [[i32; 4]; 2] {
        self.choices
    }

    /// The list as last drawn (tests).
    #[cfg(test)]
    pub fn list_rect(&self) -> [i32; 4] {
        self.list
    }

    /// The sounds asked for since the last call.
    pub fn take_cues(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.cues)
    }

    /// The list's lines for the notes: each section's title, its bullets
    /// with their later lines under the text, a gap between sections; or
    /// the line saying where the notes are.
    fn layout(&self, vui: &Vui, r: &dyn Raster) -> Vec<Line> {
        let w = &self.window;
        let width = LIST_W - 24;
        let mut lines = Vec::new();
        let para = |lines: &mut Vec<Line>, s: &str| {
            for t in wrap(w, vui, r, &ui_text(s), width) {
                lines.push(Line { text: t, title: false, x: 0 });
            }
        };
        match &self.notes {
            WhatsNew::Sections(sections) => {
                let dash = wide("- ");
                let indent = w.text_width(vui, r, &dash);
                for (k, Section { title, bullets }) in sections.iter().enumerate() {
                    if k > 0 {
                        lines.push(Line { text: Vec::new(), title: false, x: 0 });
                    }
                    for t in wrap(w, vui, r, &ui_text(title), width) {
                        lines.push(Line { text: t, title: true, x: 0 });
                    }
                    for b in bullets {
                        for (n, t) in wrap(w, vui, r, &ui_text(b), width - indent).into_iter().enumerate() {
                            if n == 0 {
                                let mut first = dash.clone();
                                first.extend(t);
                                lines.push(Line { text: first, title: false, x: 0 });
                            } else {
                                lines.push(Line { text: t, title: false, x: indent });
                            }
                        }
                    }
                }
            }
            WhatsNew::Nothing => para(&mut lines, &format!("Its notes are not written yet. The release page lists what changed: {RELEASE}")),
            WhatsNew::Unread => para(&mut lines, &format!("What is new in it could not be read here. The release page lists it: {RELEASE}")),
        }
        lines
    }

    fn count(&self) -> usize {
        self.lines.as_ref().map_or(0, Vec::len)
    }

    /// The furthest the list's top goes: its last row at the bottom.
    fn max_top(&self) -> f32 {
        (self.count() as f32 - LIST_ROWS as f32).max(0.0)
    }

    /// The list moved `d` rows (kept inside it); true when it moved.
    fn scroll(&mut self, d: f32) -> bool {
        let t = (self.top + d).clamp(0.0, self.max_top());
        let moved = t != self.top;
        self.top = t;
        moved
    }

    /// A key, the game's code: what it answered, if it did.
    pub fn key(&mut self, key: i32, now: i64) -> Option<Answer> {
        let page = LIST_ROWS as f32 - 1.0;
        let step = |me: &mut Self, d: f32| {
            let moved = me.scroll(d);
            me.cues.push(if moved { 11 } else { 9 });
        };
        match key {
            19 | 138 => step(self, -1.0),
            20 | 139 => step(self, 1.0),
            21 => step(self, -page),
            22 => step(self, page),
            24 => step(self, -self.max_top()),
            23 => step(self, self.max_top()),
            17 | 136 => {
                self.cues.push(if self.sel > 0 { 11 } else { 9 });
                self.sel = 0;
                self.window.reset_tick_pulse(now);
            }
            18 | 137 => {
                self.cues.push(if self.sel < 1 { 11 } else { 9 });
                self.sel = 1;
                self.window.reset_tick_pulse(now);
            }
            13 | 134 => {
                let restart = self.sel == 0;
                self.cues.push(if restart { 13 } else { 8 });
                return Some(if restart { Answer::Restart } else { Answer::Later });
            }
            27 | 132 => {
                self.cues.push(8);
                return Some(Answer::Later);
            }
            _ => {}
        }
        None
    }

    /// A stick's up and down (-1..1, down positive), held `dt` seconds.
    pub fn stick(&mut self, y: f32, dt: f32) {
        self.scroll(y * STICK_RATE * dt);
    }

    /// The wheel: `lines` up (positive) or down.
    pub fn wheel(&mut self, lines: f32) {
        self.scroll(-lines);
    }

    /// A press at frame point `p`: on a choice, that choice taken (Some of
    /// its answer); on the list, a drag begun (Some(None)); elsewhere not
    /// the window's (None).
    pub fn press(&mut self, p: [f32; 2]) -> Option<Option<Answer>> {
        let inside = |r: [i32; 4]| p[0] >= r[0] as f32 && p[0] < r[2] as f32 && p[1] >= r[1] as f32 && p[1] < r[3] as f32;
        if let Some(k) = self.choices.iter().position(|&r| inside(r)) {
            let k = k as i32;
            self.sel = k;
            let restart = k == 0;
            self.cues.push(if restart { 13 } else { 8 });
            return Some(Some(if restart { Answer::Restart } else { Answer::Later }));
        }
        if inside(self.list) {
            self.drag = Some((p[1], self.top));
            return Some(None);
        }
        None
    }

    /// The pointer at `p` while pressed: the list dragged with it. True
    /// while a drag is on.
    pub fn drag_to(&mut self, p: [f32; 2]) -> bool {
        let Some((y0, t0)) = self.drag else { return false };
        let h = self.row_h.max(1) as f32;
        self.top = (t0 + (y0 - p[1]) / h).clamp(0.0, self.max_top());
        true
    }

    /// The press let go: true when it was the list's drag.
    pub fn release(&mut self) -> bool {
        self.drag.take().is_some()
    }

    /// What frame point `p` is on, as last drawn: the list, elsewhere on
    /// the window (the choices among it), or nothing of it.
    pub fn part(&self, p: [f32; 2]) -> Option<crate::update::OfferPart> {
        let inside = |r: [i32; 4]| p[0] >= r[0] as f32 && p[0] < r[2] as f32 && p[1] >= r[1] as f32 && p[1] < r[3] as f32;
        let (c, s) = (self.window.center, self.window.size);
        if inside(self.list) {
            Some(crate::update::OfferPart::List)
        } else if inside([c[0] - s[0] / 2, c[1] - s[1] / 2, c[0] + s[0] / 2, c[1] + s[1] / 2]) {
            Some(crate::update::OfferPart::Window)
        } else {
            None
        }
    }

    /// `OnDrawSelf`: the window, the heading, the list and its scroll bar,
    /// the question and the choices; `left_ms` until it is left standing,
    /// the last ten seconds counted down by the choices.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, left_ms: i64) {
        self.window.init(vui, r, [320, 224], WIDTH, ROWS, 0);
        self.window.set_title(Some(&wide("A new version is ready")));
        if self.lines.is_none() {
            self.lines = Some(self.layout(vui, r));
            self.top = self.top.clamp(0.0, self.max_top());
        }
        self.window.draw(vui, r);
        let black = UI_COLORS[0];
        let blue = UI_COLORS[1];
        let w = self.window.size[0];
        self.window.text_box(vui, r, &wide("What's new in it:"), LIST_X - 4, 0, LIST_W, 1, black, 0);
        // the list, framed, its bar's thumb where the top is
        let n = self.count();
        let t = if self.max_top() > 0.0 { self.top / self.max_top() } else { -1.0 };
        self.window.draw_frame_vscroll(vui, r, LIST_X, LIST_ROW, LIST_W, LIST_ROWS, t);
        let first = self.top.floor() as usize;
        if let Some(lines) = &self.lines {
            for (row, line) in lines.iter().skip(first).take(LIST_ROWS as usize).enumerate() {
                let c = if line.title { blue } else { black };
                self.window.text_box(vui, r, &line.text, LIST_X + 4 + line.x, LIST_ROW + row as i32, LIST_W - 12 - line.x, 1, c, 0);
            }
        }
        let h = vui.font_height(r) - 4;
        self.row_h = h;
        let tl = self.window.top_left();
        let ly = self.window.row_center(vui, r, LIST_ROW)[1] - 4;
        self.list = [tl[0] + LIST_X - 4, ly - 3, tl[0] + LIST_X + LIST_W + 4, ly + LIST_ROWS * h + 1];
        let _ = n;
        // the question, and the choices centred on the window's thirds
        let ask = if self.world { "Restart now to play it? You leave the world and sign in again." } else { "Restart now to play it?" };
        self.window.text_box(vui, r, &wide(ask), 20, ASK_ROW, w - 40, 2, black, 8);
        let col = [w / 3, w / 3 * 2];
        let choice = |on: bool, s: &str| wide(&if on { format!("|q{s}|w") } else { s.to_string() });
        self.window.text_box(vui, r, &choice(self.sel == 0, "Restart now"), col[0] - 100, CHOICE_ROW, 200, 1, black, 8);
        self.window.text_box(vui, r, &choice(self.sel == 1, "Later"), col[1] - 100, CHOICE_ROW, 200, 1, black, 8);
        // each choice its half of the row, split between their centres
        let split = tl[0] + (col[0] + col[1]) / 2;
        let cy = self.window.row_center(vui, r, CHOICE_ROW)[1];
        self.choices = [[tl[0] + 20, cy - h, split, cy + h], [split, cy - h, tl[0] + w - 20, cy + h]];
        if (0..10_000).contains(&left_ms) {
            self.window.text_box(vui, r, &wide(&(left_ms / 1000 + 1).to_string()), 24, CHOICE_ROW, 40, 1, black, 0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};

    fn notes(n: usize, bullets: usize) -> WhatsNew {
        WhatsNew::Sections((0..n).map(|k| Section { title: format!("2026-10-2{k} · Section {k}"), bullets: (0..bullets).map(|b| format!("Bullet {b} of section {k}, long enough to wrap onto a second line of the list, and a third maybe.")).collect() }).collect())
    }

    fn opened(n: WhatsNew) -> NotesWnd {
        let (vui, mut r) = (vui(), NullRaster);
        let mut w = NotesWnd::default();
        w.open(n, true);
        w.draw(&vui, &mut r, 60_000);
        w
    }

    /// The pad's ✕ △ ○ □ are the UI's glyphs, a `|` is doubled, the rest as
    /// it is.
    #[test]
    fn the_notes_glyphs_are_the_uis() {
        assert_eq!(ui_text("pick ✕, △ back | ○ □"), wide("pick |x, |t back || |c |s"));
        assert_eq!(ui_text("plain"), wide("plain"), "the control: nothing to change");
    }

    /// A long text breaks at spaces within the width; a word wider than the
    /// line is cut; an empty text is one empty line.
    #[test]
    fn the_list_wraps_at_spaces() {
        let (vui, r) = (vui(), NullRaster);
        let w = Window::default();
        // the null raster: 8 pixels a character
        let lines = wrap(&w, &vui, &r, &wide("aaaa bbbb cccc dddd"), 80);
        assert_eq!(lines, vec![wide("aaaa bbbb"), wide("cccc dddd")]);
        assert_eq!(wrap(&w, &vui, &r, &wide("aaaa bbbb"), 200), vec![wide("aaaa bbbb")], "the control: it fits");
        assert_eq!(wrap(&w, &vui, &r, &wide("abcdefghijkl"), 40), vec![wide("abcde"), wide("fghij"), wide("kl")]);
        assert_eq!(wrap(&w, &vui, &r, &wide("ab/cdefg/hijk"), 72), vec![wide("ab/cdefg/"), wide("hijk")], "an address breaks after the last '/' that fits");
        assert_eq!(wrap(&w, &vui, &r, &wide("ab/cdefg/hijk"), 64), vec![wide("ab/"), wide("cdefg/"), wide("hijk")]);
        assert_eq!(wrap(&w, &vui, &r, &[], 40), vec![Vec::<u16>::new()]);
    }

    /// Each section its title (blue), its bullets with a dash and their
    /// later lines under the text, a gap between sections; when the notes
    /// are missing, one line saying where they are.
    #[test]
    fn the_list_is_the_new_sections() {
        let w = opened(notes(2, 2));
        let lines = w.lines.as_ref().unwrap();
        assert!(lines[0].title && String::from_utf16_lossy(&lines[0].text).contains("Section 0"));
        assert!(String::from_utf16_lossy(&lines[1].text).starts_with("- Bullet 0"));
        assert!(lines[2].x > 0 && !lines[2].title, "a bullet's later line under its text");
        let gap = lines.iter().position(|l| l.text.is_empty()).unwrap();
        assert!(lines[gap + 1].title && String::from_utf16_lossy(&lines[gap + 1].text).contains("Section 1"));
        let unread = opened(WhatsNew::Unread);
        let text: String = unread.lines.as_ref().unwrap().iter().map(|l| String::from_utf16_lossy(&l.text) + " ").collect();
        assert!(text.contains("could not be read") && text.contains("eqoa-rust-client-vanilla"));
        assert!(unread.lines.as_ref().unwrap().iter().all(|l| !l.title), "no section titles");
        let nothing = opened(WhatsNew::Nothing);
        assert!(nothing.lines.as_ref().unwrap().iter().any(|l| String::from_utf16_lossy(&l.text).contains("not written yet")));
    }

    /// Up and down, a page, Home and End move the list inside its rows,
    /// a sound for each step and the end's when it cannot go on; the
    /// stick and the wheel move it too; a short list does not move.
    #[test]
    fn the_list_scrolls_inside_its_rows() {
        let mut w = opened(notes(3, 4));
        let max = w.max_top();
        assert!(max > LIST_ROWS as f32, "{max}: more than a page to scroll");
        assert_eq!(w.key(138, 0), None);
        assert_eq!((w.top, w.take_cues()), (0.0, vec![9]), "at the top: the end's sound");
        w.key(139, 0);
        w.key(20, 0);
        assert_eq!((w.top, w.take_cues()), (2.0, vec![11, 11]));
        w.key(22, 0);
        assert_eq!(w.top, 2.0 + LIST_ROWS as f32 - 1.0, "a page: the rows less one");
        w.key(23, 0);
        assert_eq!(w.top, max, "End");
        w.key(20, 0);
        assert_eq!((w.top, w.take_cues().last().copied()), (max, Some(9)), "the bottom holds");
        w.key(24, 0);
        assert_eq!(w.top, 0.0, "Home");
        w.stick(1.0, 0.5);
        assert_eq!(w.top, STICK_RATE * 0.5);
        w.wheel(2.0);
        assert_eq!(w.top, STICK_RATE * 0.5 - 2.0, "the wheel up");
        w.wheel(-1000.0);
        assert_eq!(w.top, max, "kept inside");
        let mut short = opened(notes(1, 1));
        short.key(139, 0);
        assert_eq!((short.top, short.take_cues()), (0.0, vec![9]), "the control: a short list stays");
    }

    /// The choice starts on Later: Enter (or ✕) alone is Later, as Escape
    /// and △ are; left then Enter is Restart now.
    #[test]
    fn it_starts_on_later() {
        let mut w = opened(notes(1, 1));
        assert_eq!(w.key(13, 0), Some(Answer::Later));
        assert_eq!(w.key(134, 0), Some(Answer::Later));
        assert_eq!(w.key(27, 0), Some(Answer::Later));
        assert_eq!(w.key(132, 0), Some(Answer::Later));
        assert_eq!(w.key(136, 0), None);
        assert_eq!(w.key(134, 0), Some(Answer::Restart));
        assert_eq!(w.key(18, 0), None);
        assert_eq!(w.key(13, 0), Some(Answer::Later), "right goes back to Later");
        assert_eq!(w.key(119, 0), None, "the control: a key not its own does nothing");
    }

    /// A click on a choice takes it at once; a press on the list starts a
    /// drag that moves the list with the pointer; anywhere else is not the
    /// window's (`pointer`).
    #[test]
    fn a_pointer_on_its_choices_and_its_list() {
        let mut w = opened(notes(3, 4));
        let [yes, later] = w.choices;
        let mid = |b: [i32; 4]| [(b[0] + b[2]) as f32 / 2.0, (b[1] + b[3]) as f32 / 2.0];
        assert_eq!(w.press([2.0, 2.0]), None, "the control: off the window");
        assert_eq!(w.press(mid(later)), Some(Some(Answer::Later)));
        assert_eq!(w.press(mid(yes)), Some(Some(Answer::Restart)));
        // a drag on the list: up by three rows' height scrolls three rows down
        let at = mid(w.list);
        assert_eq!(w.press(at), Some(None));
        assert!(w.drag_to([at[0], at[1] - 3.0 * w.row_h as f32]));
        assert_eq!(w.top, 3.0);
        assert!(w.release());
        assert!(!w.drag_to([at[0], at[1] - 90.0]), "let go: no more dragging");
        assert_eq!(w.top, 3.0);
        assert!(!w.release());
    }

    /// What a finger is put down on (`part`): the list, which it drags; the
    /// window's choices and the rest of it, where it taps; off the window,
    /// the game's (the touch screen's pad keeps it). The control: a corner
    /// of the frame, which the window does not reach, is nothing of it.
    #[test]
    fn a_finger_finds_the_list_the_window_or_nothing() {
        use crate::update::OfferPart;
        let w = opened(notes(3, 4));
        let mid = |b: [i32; 4]| [(b[0] + b[2]) as f32 / 2.0, (b[1] + b[3]) as f32 / 2.0];
        assert_eq!(w.part(mid(w.list)), Some(OfferPart::List));
        assert_eq!(w.part(mid(w.choices[0])), Some(OfferPart::Window), "Restart now");
        assert_eq!(w.part(mid(w.choices[1])), Some(OfferPart::Window), "Later");
        // the title bar, above the list: still the window
        let c = w.window.center;
        assert_eq!(w.part([c[0] as f32, (c[1] - w.window.size[1] / 2 + 4) as f32]), Some(OfferPart::Window));
        assert_eq!(w.part([2.0, 2.0]), None, "the control: the frame's corner");
        assert_eq!(w.part([(c[0] + w.window.size[0] / 2 + 3) as f32, c[1] as f32]), None, "just right of the window");
    }
}
