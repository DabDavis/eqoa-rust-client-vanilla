//! The two dialog windows: `VIWndPrivChat` (an NPC's words, "Done" under
//! them) and `VIWndResponse` (a dialog with answers to pick, More / Back /
//! Close). CLIENT shows three answers a page; vanilla shows `PAGE` (twelve) so
//! a coach's or a merchant-like list is one box (the owner, 2026-11-15: "the
//! coach page to select coaches and the other type pages need to be stacked
//! higher for choices instead of More... to show 3 per page", then "lets
//! increase the dialog box for heroic dungeons, coachmen etc. to stack higher
//! before pagination"): a PS2's server sends six at most, go-eqoa-server up
//! to twelve once told (`net::session::OP_DIALOG_ROOM`). A page whose answers
//! wrap so far that the window would leave the screen shows fewer
//! (`Response::fit`), never under CLIENT's three.

use vi::ui::{Raster, Vui, Window, wide};

/// `VIWndPrivChat`: an NPC's words in a box in the middle of the screen,
/// "|x Close" under them.
#[derive(Default)]
pub struct PrivChat {
    text: Vec<u16>,
    rows: i32,
    window: Window,
    /// The focus arrived and the window is to be sized (`OnMessage` 295).
    focus: bool,
}

impl PrivChat {
    pub fn set_text(&mut self, s: &[u16]) {
        self.text = s.to_vec();
        self.rows = 0;
        self.focus = true;
    }

    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster) {
        if self.focus {
            self.focus = false;
            self.window.init(vui, r, [320, 224], 400, 16, 1);
            self.rows = self.window.count_lines(vui, r, &self.text, 360) + 2;
            self.window.init(vui, r, [320, 224], 400, self.rows, 1);
            if self.window.is_open() {
                self.window.close(true);
            }
            self.window.open(false);
        }
        self.window.draw(vui, r);
        let text = self.text.clone();
        self.window.text_box(vui, r, &text, 20, 0, 360, self.rows - 2, [0.8, 0.8, 0.8, 1.0], 3);
        let s = wide(&format!("|x{}", vui.words.get(280, vui.language)));
        self.window.text(vui, r, &s, 20, self.rows - 1, [1.0; 4], 1);
    }
}


/// `VIDialogBox`: the 0x0034 dialog, its text and options expanded.
#[derive(Clone, Debug, Default)]
pub struct DialogBox {
    pub id: u32,
    pub text: Vec<u16>,
    pub options: Vec<Vec<u16>>,
}

#[derive(Clone, Debug, Default)]
struct Option_ {
    index: i32,
    /// 1 an answer, 2 More, 3 Back, 4 Close.
    kind: i32,
    text: Vec<u16>,
}

/// The answers a page of the response window shows at most (CLIENT's three),
/// as many as the server is told a box may hold.
pub const PAGE: i32 = net::session::DIALOG_ROOM as i32;

/// The fewest answers a page shows when a long page is cut to fit the screen
/// (CLIENT's own page).
const PAGE_LEAST: i32 = 3;

/// The window is centred at y 200 (`draw`) of the frame's 448: it stays on
/// the screen while no taller than twice that.
const TALLEST: i32 = 400;

/// `VIWndResponse`.
#[derive(Default)]
pub struct Response {
    dialog: DialogBox,
    text: Vec<u16>,
    options: Vec<Option_>,
    sel: i32,
    first: i32,
    /// The answers a page shows: `PAGE`, or fewer where those would not fit
    /// on the screen (`fit`); 0 before the first dialog.
    page: i32,
    window: Window,
    focus: bool,
    /// The UI sound the last key asked for (`VIClient::PlaySound`).
    pub cue: Option<i32>,
    /// Where each option was drawn last (its index, frame x0 y0 x1 y1), for
    /// a pointer.
    hits: Vec<(i32, [i32; 4])>,
}

/// What a key did to the response window.
pub enum Answer {
    /// Nothing for the game.
    None,
    /// 0x0035 with the dialog and the choice (255: none); the window closes.
    Send(u32, u8),
}

impl Response {
    /// `SetDialog`.
    pub fn set_dialog(&mut self, vui: &Vui, d: DialogBox) {
        self.dialog = d;
        self.focus = true;
        self.page = PAGE;
        self.build(vui);
    }

    /// The answers a page shows.
    fn page(&self) -> i32 {
        if self.page > 0 { self.page } else { PAGE }
    }

    /// The rows the window holds for the page of answers from `first` of a
    /// page of `page`: the text, a gap, each answer's lines, More, Back and
    /// Close where they are, and the two of the frame (`draw`'s count).
    fn rows_for(&self, vui: &Vui, r: &mut dyn Raster, first: i32, page: i32) -> i32 {
        let count = self.dialog.options.len() as i32;
        let mut rows = self.window.count_lines(vui, r, &self.text, 460) + 1 + 2;
        for o in &self.dialog.options[first.min(count) as usize..(first + page).min(count) as usize] {
            rows += self.window.count_lines(vui, r, o, 430).max(1);
        }
        rows + (first + page < count) as i32 + (first >= page) as i32 + 1
    }

    /// `PAGE` answers a page, or as many fewer (down to `PAGE_LEAST`) as keep
    /// every page of the dialog on the screen at this font: the window is
    /// `rows * (h - 4) + 36` high (`Window::init`).
    fn fit(&mut self, vui: &Vui, r: &mut dyn Raster) {
        let h = vui.font_height(r) - 4;
        if h <= 0 {
            return;
        }
        let count = self.dialog.options.len() as i32;
        let mut page = PAGE;
        while page > PAGE_LEAST {
            let tallest = (0..count.max(1)).step_by(page as usize).map(|first| self.rows_for(vui, r, first, page)).max().unwrap_or(0);
            if tallest * h + 36 <= TALLEST {
                break;
            }
            page -= 1;
        }
        if page != self.page() {
            self.page = page;
            self.first = 0;
            self.sel = 0;
            self.build(vui);
        }
    }

    /// `Build`: the page of options from `first`, then More / Back / Close
    /// (WORDS 245, 217, 161). (The window is sized when drawn, where the
    /// font is.)
    fn build(&mut self, vui: &Vui) {
        let d = &self.dialog;
        self.text = d.text.clone();
        self.options.clear();
        let count = d.options.len() as i32;
        let mut i = self.first;
        let page = self.page();
        while i < count.min(self.first + page) {
            self.options.push(Option_ { index: i, kind: 1, text: d.options[i as usize].clone() });
            i += 1;
        }
        let word = |n: i32| wide(vui.words.get(n, vui.language));
        if self.first + page < count {
            self.options.push(Option_ { index: i, kind: 2, text: word(245) });
            i += 1;
        }
        if self.first >= page {
            self.options.push(Option_ { index: i, kind: 3, text: word(217) });
            i += 1;
        }
        self.options.push(Option_ { index: i, kind: 4, text: word(161) });
    }

    /// `Move(d)`: up or down among the options shown.
    fn step(&mut self, d: i32, time: i64) {
        let n = self.options.iter().filter(|o| !o.text.is_empty()).count() as i32;
        let at = self.sel - self.first + d;
        if at < 0 {
            self.cue = Some(9);
            self.sel = self.first;
        } else if at >= n {
            self.cue = Some(9);
            self.sel = self.first + n - 1;
        } else {
            self.sel += d;
            self.cue = Some(11);
        }
        self.window.reset_tick_pulse(time);
    }

    /// `OnKeyDown`: up/down, Enter (an answer, More, Back, Close), Escape.
    pub fn key(&mut self, vui: &Vui, key: Key, time: i64) -> Answer {
        match key {
            Key::Up => self.step(-1, time),
            Key::Down => self.step(1, time),
            Key::Escape => {
                self.cue = Some(8);
                self.sel = 0;
                self.first = 0;
                return Answer::Send(self.dialog.id, 255);
            }
            Key::Enter => {
                let Some(o) = self.options.iter().find(|o| o.index == self.sel).cloned() else { return Answer::None };
                match o.kind {
                    1 => {
                        self.sel = 0;
                        self.first = 0;
                        return Answer::Send(self.dialog.id, o.index as u8);
                    }
                    4 => {
                        self.sel = 0;
                        self.first = 0;
                        return Answer::Send(self.dialog.id, 255);
                    }
                    2 => {
                        self.first += self.page();
                        self.build(vui);
                        self.step(1, time);
                    }
                    3 => {
                        self.first = (self.first - self.page()).max(0);
                        self.build(vui);
                        self.step(-1, time);
                    }
                    _ => {}
                }
            }
            Key::Left | Key::Right => {}
        }
        Answer::None
    }

    /// A pointer at frame point `p` (the client's own): on an option, that
    /// option selected, for Enter to take. True when it was one.
    pub fn point(&mut self, p: [f32; 2]) -> bool {
        let (x, y) = (p[0] as i32, p[1] as i32);
        let Some(&(index, _)) = self.hits.iter().find(|(_, r)| (r[0]..r[2]).contains(&x) && (r[1]..r[3]).contains(&y)) else { return false };
        self.sel = index;
        true
    }

    /// `OnDrawSelf` (with `Build`'s sizing and the focus's open).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.window.init(vui, r, [320, 224], 500, 32, 1);
        if self.focus {
            self.fit(vui, r);
        }
        let mut rows = self.window.count_lines(vui, r, &self.text, 460) + 1;
        for o in &self.options {
            rows += self.window.count_lines(vui, r, &o.text, 430);
        }
        self.window.init(vui, r, [320, 200], 500, rows + 2, 1);
        if self.focus {
            self.focus = false;
            self.window.close(true);
            self.window.open(false);
        }
        self.window.draw(vui, r);
        let lines = self.window.count_lines(vui, r, &self.text, 460);
        let text = self.text.clone();
        self.window.text_box(vui, r, &text, 20, 0, 460, lines, [0.8, 0.8, 0.8, 1.0], 3);
        let mut row = lines + 1;
        let lh = vui.font_height(r) - 4;
        let x0 = self.window.top_left()[0];
        self.hits.clear();
        for (n, o) in self.options.iter().enumerate() {
            let on = n as i32 + self.first == self.sel;
            let c = if on { [1.0, 1.0, 0.0, 1.0] } else { [1.0; 4] };
            let lines = self.window.count_lines(vui, r, &o.text, 430);
            if o.kind == 1 && quest_tail(&o.text) && crate::settings::head_marks() {
                self.window.text_box(vui, &mut QuestTail(&mut *r), &o.text, 50, row, 430, lines, c, 3);
            } else {
                self.window.text_box(vui, r, &o.text, 50, row, 430, lines, c, 3);
            }
            if on {
                self.window.tick_r(vui, r, 40, row, true);
            }
            let (a, b) = (self.window.row_center(vui, r, row)[1], self.window.row_center(vui, r, row + lines.max(1) - 1)[1]);
            self.hits.push((o.index, [x0 + 30, a - lh / 2, x0 + 490, b + lh - lh / 2]));
            row += lines;
        }
        row += 1;
        let s = wide(&format!("|t{}", vui.words.get(161, vui.language)));
        self.window.text(vui, r, &s, 20, row, [1.0; 4], 1);
    }
}

/// The yellow of a quest's mark: the head marks' (`play::MARK_COLOR`).
const QUEST_MARK: [f32; 3] = [1.0, 0.86, 0.1];

/// An answer that ends in " ?": go-eqoa-server's mark on a coach stop of a
/// quest's step under way (its `questStopTag`, petition 3191), drawn yellow
/// as the quest marks over the heads while Extended's Quest Markers is on;
/// the PS2 shows it white.
fn quest_tail(s: &[u16]) -> bool {
    s.ends_with(&[b' ' as u16, b'?' as u16])
}

/// A raster that draws the last "?" of a row of a `quest_tail` answer in
/// `QUEST_MARK` (keeping the row's alpha), the rest as asked: a row that ends
/// " ?" or is that "?" alone, wrapped (a row cut at a space keeps the space,
/// so a "?" ending a row is the mark's). The shadow (black) is left as it is.
struct QuestTail<'a>(&'a mut dyn Raster);

impl QuestTail<'_> {
    /// `s` split before its mark, where a row ends in it.
    fn split(s: &[u16]) -> Option<&[u16]> {
        (s == [b'?' as u16] || quest_tail(s)).then(|| &s[..s.len() - 1])
    }
}

impl Raster for QuestTail<'_> {
    fn blit(&mut self, id: u32, flags: u32, at: [i32; 2], a: [i32; 2], b: [i32; 2], mode: i32, alpha: f32) {
        self.0.blit(id, flags, at, a, b, mode, alpha)
    }
    fn stretch_blit(&mut self, id: u32, flags: u32, at: [i32; 2], size: [i32; 2], a: [i32; 2], b: [i32; 2], mode: i32, alpha: f32) {
        self.0.stretch_blit(id, flags, at, size, a, b, mode, alpha)
    }
    fn fill_fan(&mut self, color: [f32; 4], corners: [[i32; 2]; 4]) {
        self.0.fill_fan(color, corners)
    }
    fn blit_full(&mut self, id: u32, flags: u32, at: [i32; 2], mode: i32, alpha: f32) {
        self.0.blit_full(id, flags, at, mode, alpha)
    }
    fn draw_string(&mut self, at: [i32; 2], color: [f32; 4], s: &[u16]) {
        self.0.draw_string(at, color, s)
    }
    fn draw_centered_string(&mut self, at: [i32; 2], color: [f32; 4], s: &[u16], mode: u32) {
        // mode 1 centres a row vertically only: x is its left, so the mark
        // goes where the rest ends
        match Self::split(s) {
            Some(head) if mode == 1 && color[..3] != [0.0, 0.0, 0.0] => {
                if !head.is_empty() {
                    self.0.draw_centered_string(at, color, head, mode);
                }
                let x = at[0] + self.0.string_width(head).max(0);
                let mark = [QUEST_MARK[0], QUEST_MARK[1], QUEST_MARK[2], color[3]];
                self.0.draw_centered_string([x, at[1]], mark, &s[s.len() - 1..], mode);
            }
            _ => self.0.draw_centered_string(at, color, s, mode),
        }
    }
    fn draw_string_scaled(&mut self, at: [f32; 2], color: [f32; 4], s: &[u16], scale: f32) {
        self.0.draw_string_scaled(at, color, s, scale)
    }
    fn string_width(&self, s: &[u16]) -> i32 {
        self.0.string_width(s)
    }
    fn point_size(&self) -> i32 {
        self.0.point_size()
    }
    fn viewport(&self) -> [i32; 4] {
        self.0.viewport()
    }
    fn set_viewport(&mut self, v: [i32; 4]) {
        self.0.set_viewport(v)
    }
    fn native_list_scrollbars(&self) -> bool {
        self.0.native_list_scrollbars()
    }
    fn note_vscroll(&mut self, bar: vi::ui::VBar) {
        self.0.note_vscroll(bar)
    }
    fn has(&self, id: u32) -> bool {
        self.0.has(id)
    }
}

/// The keys the dialog windows take.
#[derive(Clone, Copy, Debug)]
pub enum Key {
    Up,
    Down,
    Enter,
    Escape,
    Left,
    Right,
}

/// `VIWndMBox`: the message box (`VIWndGame::Prompt`), at (80, 115): the
/// text, shrunk to its fewest lines, over black in frame style 1, and its
/// buttons: type 0 OK (word 214), 1 Yes/No (21/22, starting on No), 2 word
/// 213. It fades in over a fifth of a second.
pub struct MBox {
    pub kind: i32,
    pub text: Vec<u16>,
    sel: i32,
    pub p1: i32,
    pub p2: i32,
    alpha: f32,
    time: i64,
    /// `m_pos` (the game window's at (80, 115), the item menu's at (190, 115))
    pub pos: [i32; 2],
    /// `m_timeout`: the time the box cancels itself at, or -1
    pub timeout: i64,
    window: Window,
    /// Where each button was drawn last (frame x0 y0 x1 y1), for a pointer.
    hits: Vec<[i32; 4]>,
}

/// What a box answered: `OnMessage` 268 (yes / OK) or 260 (no, cancel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoxAnswer {
    Yes(i32, i32),
    No(i32, i32),
}

impl Default for MBox {
    fn default() -> Self {
        MBox { kind: 0, text: Vec::new(), sel: 0, p1: 0, p2: 0, alpha: 0.0, time: 0, pos: [80, 115], timeout: -1, window: Window::default(), hits: Vec::new() }
    }
}

impl MBox {
    /// `Prompt(text, type, p1, p2)`: `Init(type, 1)`, the text and the two
    /// parameters; gaining the focus starts the fade (295).
    pub fn prompt(&mut self, text: &[u16], kind: i32, p1: i32, p2: i32, now: i64) {
        self.kind = kind;
        self.timeout = -1;
        self.reset_sel();
        self.text = text.to_vec();
        self.p1 = p1;
        self.p2 = p2;
        self.alpha = 0.0;
        self.time = now;
    }

    /// `ResetSel`: a Yes/No box starts on No.
    fn reset_sel(&mut self) {
        self.sel = if self.kind == 1 { 1 } else { 0 };
    }

    /// `OnKeyDown`: Enter / X accepts, Escape / triangle cancels, left and
    /// right move between the buttons. Sounds go to `cues`.
    pub fn key(&mut self, key: Key, cues: &mut Vec<i32>) -> Option<BoxAnswer> {
        match key {
            Key::Enter => {
                cues.push(13);
                let a = match (self.kind, self.sel) {
                    (0, _) | (1, 0) => BoxAnswer::Yes(self.p1, self.p2),
                    _ => BoxAnswer::No(self.p1, self.p2),
                };
                if self.kind != 2 {
                    self.reset_sel();
                }
                Some(a)
            }
            Key::Escape => {
                cues.push(8);
                self.reset_sel();
                Some(BoxAnswer::No(self.p1, self.p2))
            }
            Key::Left | Key::Right => {
                let dir = if matches!(key, Key::Left) { -1 } else { 1 };
                if self.kind == 1 {
                    self.sel += dir;
                    cues.push(if (0..=1).contains(&self.sel) { 11 } else { 9 });
                    self.sel = self.sel.clamp(0, 1);
                } else {
                    cues.push(9);
                    self.sel = 0;
                }
                None
            }
            _ => None,
        }
    }

    /// A pointer at frame point `p` (the client's own): on a button, that
    /// button selected, for Enter to take. True when it was one.
    pub fn point(&mut self, p: [f32; 2]) -> bool {
        let (x, y) = (p[0] as i32, p[1] as i32);
        let Some(k) = self.hits.iter().position(|r| (r[0]..r[2]).contains(&x) && (r[1]..r[3]).contains(&y)) else { return false };
        if self.kind == 1 {
            self.sel = k as i32;
        }
        true
    }

    /// `OnDrawSelf` (no item icon). `Update` runs in it: a timeout that has
    /// passed cancels the box (260), which is returned.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, now: i64) -> Option<BoxAnswer> {
        let yellow = [1.0, 1.0, 0.0, self.alpha];
        let white = [1.0, 1.0, 1.0, self.alpha];
        let pos = self.pos;
        let text = self.text.clone();
        let mut width = 375;
        let lines = self.window.count_lines(vui, r, &text, width);
        let mut step;
        loop {
            step = width / 6;
            width -= step;
            let n = self.window.count_lines(vui, r, &text, width);
            if !(width > 100 && lines == n) {
                break;
            }
        }
        width += step;
        let n = self.window.count_lines(vui, r, &text, width);
        let h = vui.font_height(r) - 4;
        let size = [width + 32, (n + 2) * h + 32];
        self.window.init(vui, r, [pos[0] + size[0] / 2, size[1] / 2 + pos[1] - 16], width, n, 1);
        self.window.open(true);
        // Update: the fade, and the timeout
        self.alpha = (self.alpha + (now - self.time) as f32 * 0.001 * 5.0).min(1.0);
        self.time = now;
        let mut answer = None;
        if self.timeout != -1 && self.timeout < now {
            answer = Some(BoxAnswer::No(self.p1, self.p2));
            self.timeout = -1;
        }
        r.blit(vi::ui::icon_id("black"), 0, pos, [0, 0], size, 3, self.alpha * 0.75);
        let fp = vi::ui::FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) };
        vui.blit_frame(r, pos, size, &fp, 3, self.alpha, true);
        self.window.text_box(vui, r, &text, 0, 0, width, n, white, 9);
        let mut y = pos[1] + h * (n + 1) + 22;
        let col = [pos[0] + (size[0] >> 2), pos[0] + (size[0] >> 1), pos[0] + (size[0] >> 2) * 3];
        // the last ten seconds counted down at the left
        if self.timeout != -1 && self.timeout - now < 11000 {
            r.draw_string([pos[0] + 16, y], white, &wide(&((self.timeout - now) / 1000).to_string()));
        }
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        // the buttons, the selected one yellow between two arrows
        let buttons: &[(i32, i32)] = match self.kind {
            1 => &[(21, col[0]), (22, col[2])],
            0 => &[(214, col[1])],
            2 => &[(213, col[1])],
            _ => &[],
        };
        let sel = if self.kind == 1 { self.sel } else { 0 };
        let fh = vui.font_height(r);
        self.hits.clear();
        for (k, &(id, x)) in buttons.iter().enumerate() {
            vui.draw_centered_string_ex(r, &word(id), x, y, if k as i32 == sel { yellow } else { white }, 3, true);
            let w = vui.string_width(r, &word(id)) >> 1;
            self.hits.push([x - w - 24, y - fh, x + w + 24, y + fh]);
        }
        if let Some(&(id, x)) = buttons.get(sel as usize) {
            let w = vui.string_width(r, &word(id)) >> 1;
            y -= 5;
            vui.blit_ex(r, 89, [x - 20 - w, y], 3, self.alpha, 0);
            vui.blit_ex(r, 88, [x + w + 8, y], 3, self.alpha, 0);
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A raster with the game's font's measures: ARIALUI.CSF off the disc at
    /// `$EQOA_ISO` (or `run/eqoa.iso`) when there is one, else a stand-in of its
    /// height (21) and a typical width (7 a character with the gap).
    struct Font(Option<esf::font::Font>);
    impl Raster for Font {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
        fn string_width(&self, s: &[u16]) -> i32 {
            match &self.0 {
                Some(f) => s.iter().map(|&c| f.glyph(c).or_else(|| f.glyph(b'?' as u16)).map_or(0, |g| g.width as i32 + 1)).sum(),
                None => s.len() as i32 * 7,
            }
        }
        fn point_size(&self) -> i32 {
            self.0.as_ref().map_or(21, |f| f.height as i32)
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 639, 447]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    fn font() -> Font {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = std::env::var_os("EQOA_ISO").map(std::path::PathBuf::from).unwrap_or_else(|| root.join("run/eqoa.iso"));
        let f = iso::Iso::open(&path).ok().and_then(|mut d| d.read("ARIALUI.CSF").ok()).and_then(|b| esf::Container::open(b).ok()).and_then(|c| {
            (0..c.nodes.len()).filter(|&i| c.nodes[i].ty == 0x7000).find_map(|i| esf::font::read(&c, i).filter(|f| f.id == vi::ui::font_id() as i32))
        });
        if f.is_none() {
            eprintln!("no disc at {}: the stand-in font (21 high)", path.display());
        }
        Font(f)
    }

    fn dialog(text: &str, n: usize, answer: impl Fn(usize) -> String) -> DialogBox {
        DialogBox { id: 7, text: wide(text), options: (0..n).map(|i| wide(&answer(i))).collect() }
    }

    /// A coachman's greeting of two or three lines (go-eqoa-server's waysender
    /// question is the longest, 127 characters) over twelve stops.
    const GREETING: &str = "Greetings, traveler! Where would you like to go? Each of these stops is a short ride from here, and the fare is paid.";

    /// The window as drawn: its top and bottom on the 448-high frame.
    fn span(w: &Response) -> (i32, i32) {
        (w.window.center[1] - w.window.size[1] / 2, w.window.center[1] + w.window.size[1] / 2)
    }

    /// Twelve answers of a stop's length are one page under a greeting of
    /// two or three lines, the window on the screen at the game's font, with
    /// Close under them and no More; each answer's pointer hit is its own row,
    /// top to bottom, and picks it.
    #[test]
    fn twelve_answers_fit_one_page_on_the_screen() {
        let vui = Vui::new(Default::default(), Default::default(), Vec::new());
        let mut r = font();
        let mut w = Response::default();
        w.set_dialog(&vui, dialog(GREETING, 12, |i| format!("Stop {i}: the Plains of Karana (120 tunar)")));
        w.draw(&vui, &mut r);
        let lines = w.window.count_lines(&vui, &mut r, &wide(GREETING), 460);
        assert!((2..=3).contains(&lines), "the greeting is {lines} lines");
        assert_eq!(w.page(), 12);
        assert_eq!(w.options.len(), 13, "twelve and Close");
        assert!(w.options[..12].iter().all(|o| o.kind == 1) && w.options[12].kind == 4);
        let (top, bottom) = span(&w);
        eprintln!("twelve answers, a greeting of {lines} lines, font {} high: the window {top}..{bottom} of 448", r.point_size());
        assert!(top >= 0 && bottom <= 448, "the window {top}..{bottom}");
        assert_eq!(w.hits.len(), 13);
        for k in 1..w.hits.len() {
            assert!(w.hits[k].1[1] >= w.hits[k - 1].1[3] - 1, "hit {k} overlaps the one above: {:?}", w.hits);
        }
        assert!(w.hits.iter().all(|(_, h)| h[1] >= top && h[3] <= bottom));
        for (i, &(index, h)) in w.hits.clone().iter().enumerate() {
            assert_eq!(index, i as i32);
            assert!(w.point([((h[0] + h[2]) / 2) as f32, ((h[1] + h[3]) / 2) as f32]));
            assert_eq!(w.sel, i as i32);
        }
        // the eleventh, picked by the keys: ten downs from the first
        w.sel = 0;
        for _ in 0..10 {
            w.key(&vui, Key::Down, 0);
        }
        assert!(matches!(w.key(&vui, Key::Enter, 0), Answer::Send(7, 10)));
    }

    /// More than twelve (a server that sends them) pages by twelve, More and
    /// Back where they lead, each answer's index its own; answers so long
    /// that twelve would leave the screen are paged fewer, never under three.
    #[test]
    fn long_lists_page_and_tall_ones_shrink() {
        let vui = Vui::new(Default::default(), Default::default(), Vec::new());
        let mut r = font();
        let mut w = Response::default();
        w.set_dialog(&vui, dialog("Where to?", 20, |i| format!("Stop {i}")));
        w.draw(&vui, &mut r);
        assert_eq!(w.page(), 12);
        assert_eq!(w.options.iter().map(|o| o.kind).collect::<Vec<_>>(), [vec![1; 12], vec![2, 4]].concat());
        w.sel = 12; // More
        w.key(&vui, Key::Enter, 0);
        w.draw(&vui, &mut r);
        assert_eq!(w.options.iter().map(|o| (o.index, o.kind)).collect::<Vec<_>>()[..9], (12..20).map(|i| (i, 1)).chain([(20, 3)]).collect::<Vec<_>>()[..]);
        let (top, bottom) = span(&w);
        assert!(top >= 0 && bottom <= 448);
        w.sel = 13; // the second page's second answer
        assert!(matches!(w.key(&vui, Key::Enter, 0), Answer::Send(7, 13)));

        // twelve answers of the most a 0x0034 carries (63 characters), each
        // two lines or more: fewer a page, every page on the screen
        let mut w = Response::default();
        w.set_dialog(&vui, dialog(GREETING, 12, |i| format!("{i} {}", "Wide wares and wondrous wearables for the weary walker".repeat(2))[..63].to_string()));
        w.draw(&vui, &mut r);
        assert!((PAGE_LEAST..12).contains(&w.page()), "page {}", w.page());
        let (top, bottom) = span(&w);
        eprintln!("long answers: {} a page, the window {top}..{bottom}", w.page());
        assert!(top >= 0 && bottom <= 448, "the window {top}..{bottom}");
        assert_eq!(w.options.iter().filter(|o| o.kind == 2).count(), 1, "More");
    }

    /// A raster that keeps each string drawn, its place and colour, over
    /// the game's font's measures.
    struct Rec(Font, Vec<([i32; 2], [f32; 4], String)>);
    impl Raster for Rec {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, at: [i32; 2], c: [f32; 4], s: &[u16]) {
            self.1.push((at, c, String::from_utf16_lossy(s)));
        }
        fn draw_centered_string(&mut self, at: [i32; 2], c: [f32; 4], s: &[u16], _: u32) {
            self.1.push((at, c, String::from_utf16_lossy(s)));
        }
        fn string_width(&self, s: &[u16]) -> i32 {
            self.0.string_width(s)
        }
        fn point_size(&self) -> i32 {
            self.0.point_size()
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 639, 447]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    /// Heirloom petition 3191: a coach stop of a quest's step ends " ?"
    /// (go-eqoa-server's questStopTag, after the coachman's period); its "?"
    /// is drawn yellow, the quest marks' yellow, right after the rest of its
    /// row, and its shadow stays black. The control: a stop with no tag, and
    /// one whose sentence ends in a question mark of its own ("...Fayspires?"),
    /// have no yellow.
    #[test]
    fn a_quest_stops_trailing_mark_is_yellow() {
        let vui = Vui::new(Default::default(), Default::default(), Vec::new());
        let mut r = Rec(font(), Vec::new());
        let mut w = Response::default();
        let stops = ["Get me a horse to Aviak Village. ?", "Get me a horse to West Plateau.", "How about one to Fayspires?"];
        w.set_dialog(&vui, dialog("Welcome my friend, do you need to take a horse somewhere?", 3, |i| stops[i].to_string()));
        w.draw(&vui, &mut r);
        r.1.clear();
        w.sel = 1; // the first unselected, drawn white
        w.draw(&vui, &mut r);
        let yellow: Vec<_> = r.1.iter().filter(|(_, c, _)| c[..3] == QUEST_MARK).collect();
        assert_eq!(yellow.len(), 1, "one yellow mark: {:?}", r.1);
        assert_eq!(yellow[0].2, "?");
        let head = r.1.iter().find(|(_, c, s)| c[..3] == [1.0, 1.0, 1.0] && s == "Get me a horse to Aviak Village. ").expect("the rest of the row, white");
        assert_eq!(yellow[0].0, [head.0[0] + r.string_width(&wide(&head.2)), head.0[1]], "the mark right after the row");
        assert_eq!(yellow[0].1[3], head.1[3], "the row's alpha");
        assert!(r.1.iter().any(|(_, c, s)| c[..3] == [0.0, 0.0, 0.0] && s == "Get me a horse to Aviak Village. ?"), "the shadow whole and black");
        assert!(r.1.iter().any(|(_, c, s)| c[..3] == [1.0, 1.0, 1.0] && s == "How about one to Fayspires?"), "its own question mark white");
    }
}
