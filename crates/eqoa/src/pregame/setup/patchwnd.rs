//! `VIWndPatcher` (SETUP `viwndpatcher.cpp`): the patch's window over the
//! title, "Update" with its status line ("Status:" and the step's words), its
//! bar, the card notice while it runs, and at Done the choices (View
//! Messages, Play or Restart); the failure box ("Error", Restart); the
//! cancel box (Notice, Yes/No, starting on No); and `VIWndReadMessage`
//! (`viwndreadmessage.cpp`, `Notes` here): the release notes, "Update
//! Messages", seven rows at a time, which open by themselves when the patch
//! changed files (`ShowPatchMessage`).
//!
//! Its keys, as `OnKeyDown`: up 19/138 and down 20/139 move between the
//! choices at Done (sounds 11, 9 at an end), Enter/✕ picks (View Messages
//! 15, Play 11), Escape/△ at Done asks to cancel (15); on the failure box ✕
//! is Restart (9). The client's own: a notice box (OK) when the server's
//! patch port did not answer (`notice`).

use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::textlist::{Message, PROMPT_OK, PROMPT_YESNO};
use vi::ui::{FrameParam, Raster, UI_COLORS, Vui, Window, wide};

/// What the window tells the setup once it gives the focus up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Up {
    /// Done, Play (or the notice closed): on to STATION
    Play,
    /// the failure box's Restart (a PS2 reboots): back to the title
    Restart,
    /// Yes on the cancel box (`m_state` -1): back to the title
    Cancelled,
}

/// The failure box's legend among the window's hits (its choices are 0
/// and 1, `m_sel`'s).
const RESTART: i32 = 2;

/// Which window of its own has the keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Has {
    #[default]
    Own,
    Message,
    Notes,
}

#[derive(Clone, Debug, Default)]
pub struct PatchWnd {
    /// `m_state`: `patcher::state`
    pub state: i32,
    pub progress: f32,
    /// `m_sel`: 0 View Messages, 1 Play/Restart
    pub sel: i32,
    /// `m_confirm`: -1 none, 0 the restart message, 1 the cancel box, 2 the
    /// client's notice
    confirm: i32,
    restart_required: bool,
    show_message: bool,
    restart_shown: bool,
    /// m_window3: the window; m_window2: the failure box
    window3: Window,
    window2: Window,
    message: Message,
    pub notes: Notes,
    has: Has,
    /// `OnDrawSelf`'s statics: the dots' count and its frame tick
    dots: i8,
    tick: i8,
    hits: Hits,
}

/// One WORDS.TXT entry, wide.
fn word(vui: &Vui, id: i32) -> Vec<u16> {
    wide(vui.words.get(id, vui.language))
}

/// `VIUIWindow::DrawProgress` (SUPPORT viuiwindow.cpp): a frame (atlas
/// 101..106) round a dimmed slot (61, mode 3, a tenth of the window's
/// opening), and the done part filled blue.
pub fn draw_progress(w: &Window, vui: &Vui, r: &mut dyn Raster, x: i32, row: i32, width: i32, t: f32) {
    let h = vui.font_height(r);
    let pos = [w.top_left()[0] + x, w.row_center(vui, r, row)[1] - 4];
    let size = [width, h - 8];
    let at = [pos[0] - 4, pos[1] - 3];
    let sz = [width + 8, h - 4];
    let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 0), br: (106, 0), bottom: (102, 0), bl: (105, 0), left: (104, 0) };
    vui.stretch_blit_ex(r, 61, at, sz, 3, w.open * 0.1);
    vui.blit_frame(r, at, sz, &fp, 0, w.open, true);
    let filled = (size[0] as f32 * t) as i32;
    vui.fill_rectangle(r, [pos, [pos[0] + filled, pos[1] + size[1]]], [0.0, 0.0, 0.8, 1.0]);
}

impl PatchWnd {
    /// `Init`, then `OnMessage` 295 from outside its children: the window
    /// (430 wide, 8 rows, about (320, 246), "Update"), the failure box
    /// ("Error"), the notes cleared, Done's choice on View Messages.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster) {
        *self = PatchWnd { tick: 15, ..Default::default() };
        self.notes = Notes::default();
        self.window3.close(true);
        self.window3.init(vui, r, [320, 246], 430, 4, 0);
        self.window3.open(false);
        self.window3.set_title(Some(&word(vui, 811)));
        self.window2.init(vui, r, [320, 246], 480, 6, 0);
        self.window2.open(false);
        self.window2.set_title(Some(&word(vui, 767)));
        self.sel = 0;
        self.confirm = -1;
        self.progress = 0.0;
        self.state = crate::patcher::state::CANCELLED;
    }

    /// `AppendMessage`: the release notes.
    pub fn append_message(&mut self, s: &[u16]) {
        self.notes.text.extend_from_slice(s);
        self.notes.dirty = true;
    }

    /// `ShowPatchMessage`: the notes open by themselves (the patch changed
    /// files), Done's choice on Play behind them.
    pub fn show_patch_message(&mut self, vui: &Vui, r: &dyn Raster, cues: &mut Vec<i32>) {
        self.sel = 1;
        self.notes.focus(vui, r);
        self.has = Has::Notes;
        cues.push(15);
    }

    /// The client's notice: the server's patch port did not answer, so its
    /// folder is played as it stands. OK goes on.
    pub fn notice(&mut self, vui: &Vui, r: &mut dyn Raster, text: &str) {
        self.confirm = 2;
        self.message.setup(vui, r, &word(vui, 785), &wide(text), PROMPT_OK, self.window3.center, 460);
        self.has = Has::Message;
    }

    /// A key for the window or the child with the keys.
    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, k: i32, now: i64, cues: &mut Vec<i32>) -> Option<Up> {
        match self.has {
            Has::Notes => {
                if self.notes.key(k, cues) {
                    // the focus back (`OnMessage` 295 from the notes)
                    self.has = Has::Own;
                    if self.restart_required && !self.restart_shown {
                        self.show_message = true;
                        self.restart_shown = true;
                    }
                }
                None
            }
            Has::Message => {
                let sel = self.message.key(k, now, cues)?;
                // 307: the box closed
                self.has = Has::Own;
                match self.confirm {
                    1 if sel == 0 => {
                        self.state = crate::patcher::state::CANCELLED;
                        Some(Up::Cancelled)
                    }
                    2 => Some(Up::Play),
                    _ => None,
                }
            }
            Has::Own => self.own_key(vui, r, k, now, cues),
        }
    }

    fn own_key(&mut self, vui: &Vui, r: &mut dyn Raster, k: i32, now: i64, cues: &mut Vec<i32>) -> Option<Up> {
        use crate::patcher::state::*;
        let notes = !self.notes.text.is_empty();
        match k {
            19 | key::UP if self.state == DONE && notes => {
                if self.sel > 0 {
                    self.sel -= 1;
                    self.window3.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            20 | key::DOWN if self.state == DONE && notes => {
                if self.sel < 1 {
                    self.sel += 1;
                    self.window3.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            13 | key::CROSS => {
                if self.state == DONE && !self.show_message {
                    if self.sel == 0 {
                        self.notes.focus(vui, r);
                        self.has = Has::Notes;
                        cues.push(15);
                    } else if self.restart_required {
                        return Some(Up::Restart);
                    } else {
                        cues.push(11);
                        return Some(Up::Play);
                    }
                } else if self.state == FAILED || self.state == AUTH_FAILED {
                    self.restart_required = true;
                    cues.push(9);
                    return Some(Up::Restart);
                }
            }
            27 | key::TRIANGLE if self.state == DONE && !self.show_message && !self.restart_required => {
                self.confirm = 1;
                cues.push(15);
                self.message.setup(vui, r, &word(vui, 785), &word(vui, 810), PROMPT_YESNO, self.window3.center, 400);
                self.has = Has::Message;
            }
            _ => {}
        }
        None
    }

    /// A pointer at frame point `p`: the keys for the child with them, a
    /// Done choice (selected by a click, picked by a second), or the failure
    /// box's Restart (its legend's ✕, pressed at once).
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.has {
            Has::Message => self.message.point(p).into_iter().collect(),
            Has::Notes => self.notes.point(p),
            Has::Own => {
                let Some(k) = self.hits.at(p) else { return Vec::new() };
                if k == RESTART {
                    return vec![key::CROSS];
                }
                if k == self.sel {
                    vec![key::CROSS]
                } else {
                    self.sel = k;
                    vec![MOVED]
                }
            }
        }
    }

    /// `OnDrawSelf` (its children after it).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, cues: &mut Vec<i32>) {
        use crate::patcher::state::*;
        self.window3.init(vui, r, [320, 246], 430, 8, 0);
        let status = match self.state {
            CANCELLED => return,
            CONNECTING => Some(757),
            CHECKING => Some(754),
            UPDATING => Some(812),
            DONE => {
                self.window3.init(vui, r, [320, 246], 430, 9, 0);
                self.window3.open(true);
                Some(764)
            }
            READING => Some(792),
            WRITING => Some(814),
            FAILED | AUTH_FAILED => Some(768),
            _ => None,
        };
        self.window3.draw(vui, r);
        let w3 = self.window3.size[0];
        self.hits.clear();
        if self.state == DONE {
            let color = |on: bool| UI_COLORS[on as usize];
            if !self.notes.text.is_empty() {
                self.window3.text(vui, r, &word(vui, 813), 40, 5, color(self.sel == 0), 0);
                self.hits.push(rows_rect(&self.window3, vui, r, 40, 5, w3 - 80, 1), 0);
            } else {
                let mut s = wide("|-");
                s.extend(word(vui, 813));
                self.window3.text(vui, r, &s, 40, 5, UI_COLORS[0], 0);
                self.sel = 1;
            }
            self.window3.text(vui, r, &word(vui, if self.restart_required { 793 } else { 790 }), 40, 6, color(self.sel == 1), 0);
            self.hits.push(rows_rect(&self.window3, vui, r, 40, 6, w3 - 80, 1), 1);
            let mut s = wide("|x ");
            s.extend(word(vui, 212));
            s.extend(wide(if self.restart_required { "   |-|t " } else { "   |t " }));
            s.extend(word(vui, 750));
            self.window3.text(vui, r, &s, 40, 8, UI_COLORS[0], 0);
            self.window3.tick_r(vui, r, 36, self.sel + 5, true);
        } else {
            self.window3.text_box(vui, r, &word(vui, 763), 40, 5, w3 - 80, 2, UI_COLORS[0], 8);
        }
        // the "..." after a busy status: a dot more every 15 frames, up to
        // three (formatted over below, as shipped: the status shows none)
        self.tick -= 1;
        if self.tick <= 0 {
            self.tick = 15;
            self.dots += 1;
            if self.dots == 3 {
                self.dots = 0;
            }
        }
        let mut line = word(vui, 803);
        if let Some(id) = status {
            line.push(b' ' as u16);
            line.extend(word(vui, id));
        }
        self.window3.text_box(vui, r, &line, 40, 1, w3 - 80, 2, UI_COLORS[0], 0);
        draw_progress(&self.window3, vui, r, 40, 3, w3 - 80, self.progress);
        if self.state == FAILED || self.state == AUTH_FAILED {
            self.window2.draw(vui, r);
            let w2 = self.window2.size[0];
            self.window2.text_box(vui, r, &word(vui, if self.state == FAILED { 769 } else { 787 }), 40, 1, w2 - 80, 3, UI_COLORS[0], 8);
            let mut s = wide("|x ");
            s.extend(word(vui, 793));
            self.window2.text(vui, r, &s, 40, 5, UI_COLORS[0], 0);
            self.hits.push(rows_rect(&self.window2, vui, r, 40, 5, w2 - 80, 1), RESTART);
        }
        if self.show_message {
            self.show_message = false;
            self.confirm = 0;
            cues.push(15);
            let game = word(vui, if net::session::EXPANSION_FLAGS & 2 != 0 { 1803 } else { 1802 });
            let text = vui.words.get(770, vui.language).replace("%S", &String::from_utf16_lossy(&game));
            self.message.setup(vui, r, &word(vui, 785), &wide(&text), PROMPT_OK, self.window3.center, 500);
            self.has = Has::Message;
        }
        match self.has {
            Has::Notes => self.notes.draw(vui, r),
            Has::Message if self.message.visible => self.message.draw(vui, r),
            _ => {}
        }
    }
}

/// `VIWndReadMessage`: a page of text seven rows at a time.
#[derive(Clone, Debug, Default)]
pub struct Notes {
    window: Window,
    pub text: Vec<u16>,
    rows: i32,
    pub top: i32,
    dirty: bool,
    /// the legend's row as last drawn (frame x0 y0 x1 y1): its △ Back
    back: [i32; 4],
}

impl Notes {
    /// `OnMessage` 295: "Update Messages", 440 wide, 10 rows, opening.
    pub fn focus(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.close(true);
        self.window.init(vui, r, [320, 246], 440, 10, 0);
        self.window.set_title(Some(&word(vui, 789)));
        self.window.open(false);
        self.dirty = true;
    }

    /// `OnKeyDown`: up and down scroll; Escape or △ goes back (true).
    pub fn key(&mut self, k: i32, cues: &mut Vec<i32>) -> bool {
        match k {
            19 | key::UP => {
                if self.top > 0 {
                    self.top -= 1;
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            20 | key::DOWN => {
                if self.top < self.rows - 7 {
                    self.top += 1;
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            27 | key::TRIANGLE => {
                cues.push(8);
                return true;
            }
            _ => {}
        }
        false
    }

    /// The pointer: the legend's △ Back goes back (the screens before the
    /// world note no glyphs of the raster's); a click in the text's upper or
    /// lower half scrolls.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let b = self.back;
        if p[0] >= b[0] as f32 && p[0] < b[2] as f32 && p[1] >= b[1] as f32 && p[1] < b[3] as f32 {
            return vec![key::TRIANGLE];
        }
        let c = self.window.center;
        let half = self.window.size[1] / 2;
        if (p[1] as i32) < c[1] - half || (p[1] as i32) > c[1] + half {
            return Vec::new();
        }
        vec![if (p[1] as i32) < c[1] { key::UP } else { key::DOWN }]
    }

    /// `OnDrawSelf`.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.window.draw(vui, r);
        let w = self.window.size[0];
        if self.dirty {
            self.rows = self.window.count_lines(vui, r, &self.text, 360);
            self.top = 0;
            self.dirty = false;
        }
        let t = if self.rows < 8 { -1.0 } else { self.top as f32 / (self.rows - 7) as f32 };
        self.window.draw_frame_vscroll(vui, r, 40, 1, w - 80, 7, t);
        self.window.text_box(vui, r, &self.text, 40, 1, w - 80, 7, UI_COLORS[0], (self.top as u32) << 24);
        let mut s = wide("|t ");
        s.extend(word(vui, 750));
        self.window.text(vui, r, &s, 40, 9, UI_COLORS[0], 0);
        self.back = rows_rect(&self.window, vui, r, 40, 9, w - 80, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patcher::state;
    use crate::pointer::testing::{NullRaster, vui};

    fn wnd() -> (PatchWnd, Vui) {
        let v = vui();
        let mut w = PatchWnd::default();
        w.open(&v, &NullRaster);
        (w, v)
    }

    /// Done with notes: down to Play, ✕ goes on; the control: up from the
    /// top is refused (sound 9), and before Done ✕ does nothing.
    #[test]
    fn done_then_play() {
        let (mut w, v) = wnd();
        let mut cues = Vec::new();
        w.state = state::CHECKING;
        assert_eq!(w.key(&v, &mut NullRaster, key::CROSS, 0, &mut cues), None, "not before Done");
        w.state = state::DONE;
        w.append_message(&wide("New this week."));
        w.draw(&v, &mut NullRaster, &mut cues);
        assert_eq!(w.sel, 0, "View Messages first");
        w.key(&v, &mut NullRaster, key::UP, 0, &mut cues);
        assert_eq!(cues.last(), Some(&9));
        w.key(&v, &mut NullRaster, key::DOWN, 0, &mut cues);
        assert_eq!(w.sel, 1);
        assert_eq!(w.key(&v, &mut NullRaster, key::CROSS, 0, &mut cues), Some(Up::Play));
    }

    /// View Messages opens the notes and △ comes back; with no notes the
    /// choice is Play (the row dimmed).
    #[test]
    fn the_notes_open_and_close() {
        let (mut w, v) = wnd();
        let mut cues = Vec::new();
        w.state = state::DONE;
        w.append_message(&wide("one\ntwo"));
        assert_eq!(w.key(&v, &mut NullRaster, key::CROSS, 0, &mut cues), None);
        assert_eq!(w.has, Has::Notes);
        assert_eq!(cues.last(), Some(&15));
        w.key(&v, &mut NullRaster, key::TRIANGLE, 0, &mut cues);
        assert_eq!(w.has, Has::Own);
        let (mut w, v) = wnd();
        w.state = state::DONE;
        w.draw(&v, &mut NullRaster, &mut cues);
        assert_eq!(w.sel, 1, "no notes: Play");
        assert_eq!(w.key(&v, &mut NullRaster, key::CROSS, 0, &mut cues), Some(Up::Play));
    }

    /// △ at Done asks first, on No; Yes cancels (back to the title); No
    /// stays (the control).
    #[test]
    fn cancel_asks_first() {
        let (mut w, v) = wnd();
        let mut cues = Vec::new();
        w.state = state::DONE;
        w.key(&v, &mut NullRaster, key::TRIANGLE, 0, &mut cues);
        assert_eq!(w.has, Has::Message);
        assert_eq!(w.key(&v, &mut NullRaster, key::CROSS, 0, &mut cues), None, "on No: stays");
        assert_eq!(w.has, Has::Own);
        w.key(&v, &mut NullRaster, key::TRIANGLE, 0, &mut cues);
        w.key(&v, &mut NullRaster, key::LEFT, 0, &mut cues);
        assert_eq!(w.key(&v, &mut NullRaster, key::CROSS, 0, &mut cues), Some(Up::Cancelled));
        assert_eq!(w.state, state::CANCELLED);
    }

    /// The failure box's ✕ is Restart; the notice's OK goes on.
    #[test]
    fn failure_restarts_and_the_notice_goes_on() {
        let (mut w, v) = wnd();
        let mut cues = Vec::new();
        w.state = state::FAILED;
        assert_eq!(w.key(&v, &mut NullRaster, key::CROSS, 0, &mut cues), Some(Up::Restart));
        let (mut w, v) = wnd();
        w.notice(&v, &mut NullRaster, "The update server did not answer.");
        assert_eq!(w.key(&v, &mut NullRaster, key::CROSS, 0, &mut cues), Some(Up::Play));
    }

    /// A pointer on Done's choices: a click selects, a second picks; the
    /// control: nothing noted, nothing hit.
    #[test]
    fn a_pointer_picks_a_choice() {
        let (mut w, v) = wnd();
        let mut cues = Vec::new();
        w.state = state::DONE;
        w.append_message(&wide("notes"));
        assert!(w.point([320.0, 246.0]).is_empty(), "nothing drawn yet: nothing hit");
        w.draw(&v, &mut NullRaster, &mut cues);
        let play = crate::pointer::testing::mid(rows_rect(&w.window3, &v, &NullRaster, 40, 6, w.window3.size[0] - 80, 1));
        assert_eq!(w.point(play), vec![MOVED]);
        assert_eq!(w.sel, 1);
        assert_eq!(w.point(play), vec![key::CROSS]);
    }

    /// A finger or a mouse alone can leave the notes and restart after a
    /// failure: the notes' legend is △ Back, the failure box's is ✕ Restart
    /// (pressed at once). The controls: the notes' text below the middle
    /// still scrolls down, and the failure box's row is nothing before a
    /// failure.
    #[test]
    fn a_pointer_leaves_the_notes_and_restarts() {
        let (mut w, v) = wnd();
        let mut cues = Vec::new();
        w.state = state::DONE;
        w.append_message(&wide("one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\nnine\nten"));
        assert_eq!(w.key(&v, &mut NullRaster, key::CROSS, 0, &mut cues), None, "View Messages");
        assert_eq!(w.has, Has::Notes);
        w.draw(&v, &mut NullRaster, &mut cues);
        let back = crate::pointer::testing::mid(rows_rect(&w.notes.window, &v, &NullRaster, 40, 9, w.notes.window.size[0] - 80, 1));
        let text = crate::pointer::testing::mid(rows_rect(&w.notes.window, &v, &NullRaster, 40, 6, w.notes.window.size[0] - 80, 1));
        assert_eq!(w.point(text), vec![key::DOWN], "the control: the text's lower half scrolls");
        assert_eq!(w.point(back), vec![key::TRIANGLE], "the legend's △ Back");
        w.key(&v, &mut NullRaster, key::TRIANGLE, 0, &mut cues);
        assert_eq!(w.has, Has::Own, "back on the window's own choices");
        // the failure box
        let (mut w, v) = wnd();
        w.state = state::CHECKING;
        w.draw(&v, &mut NullRaster, &mut cues);
        let restart = |w: &PatchWnd| crate::pointer::testing::mid(rows_rect(&w.window2, &v, &NullRaster, 40, 5, w.window2.size[0] - 80, 1));
        let at = restart(&w);
        assert!(w.point(at).is_empty(), "the control: no failure, no Restart");
        w.state = state::FAILED;
        w.draw(&v, &mut NullRaster, &mut cues);
        let at = restart(&w);
        assert_eq!(w.point(at), vec![key::CROSS], "✕ Restart at once");
        assert_eq!(w.key(&v, &mut NullRaster, key::CROSS, 0, &mut cues), Some(Up::Restart));
    }
}
