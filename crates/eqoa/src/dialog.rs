//! The two dialog windows: `VIWndPrivChat` (an NPC's words, "Done" under
//! them) and `VIWndResponse` (a dialog with answers to pick, three a page,
//! More / Back / Close).

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

/// `VIWndResponse`.
#[derive(Default)]
pub struct Response {
    dialog: DialogBox,
    text: Vec<u16>,
    options: Vec<Option_>,
    sel: i32,
    first: i32,
    window: Window,
    focus: bool,
    /// The UI sound the last key asked for (`VIClient::PlaySound`).
    pub cue: Option<i32>,
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
        self.build(vui);
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
        while self.options.len() < 6 && i < count.min(self.first + 3) {
            self.options.push(Option_ { index: i, kind: 1, text: d.options[i as usize].clone() });
            i += 1;
        }
        let word = |n: i32| wide(vui.words.get(n, vui.language));
        if self.first + 3 < count {
            self.options.push(Option_ { index: i, kind: 2, text: word(245) });
            i += 1;
        }
        if self.first >= 3 {
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
                        self.first += 3;
                        self.build(vui);
                        self.step(1, time);
                    }
                    3 => {
                        self.first -= 3;
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

    /// `OnDrawSelf` (with `Build`'s sizing and the focus's open).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.window.init(vui, r, [320, 224], 500, 32, 1);
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
        for (n, o) in self.options.iter().enumerate() {
            let on = n as i32 + self.first == self.sel;
            let c = if on { [1.0, 1.0, 0.0, 1.0] } else { [1.0; 4] };
            let lines = self.window.count_lines(vui, r, &o.text, 430);
            self.window.text_box(vui, r, &o.text, 50, row, 430, lines, c, 3);
            if on {
                self.window.tick_r(vui, r, 40, row, true);
            }
            row += lines;
        }
        row += 1;
        let s = wide(&format!("|t{}", vui.words.get(161, vui.language)));
        self.window.text(vui, r, &s, 20, row, [1.0; 4], 1);
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
}

/// What a box answered: `OnMessage` 268 (yes / OK) or 260 (no, cancel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BoxAnswer {
    Yes(i32, i32),
    No(i32, i32),
}

impl Default for MBox {
    fn default() -> Self {
        MBox { kind: 0, text: Vec::new(), sel: 0, p1: 0, p2: 0, alpha: 0.0, time: 0, pos: [80, 115], timeout: -1, window: Window::default() }
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
        for (k, &(id, x)) in buttons.iter().enumerate() {
            vui.draw_centered_string_ex(r, &word(id), x, y, if k as i32 == sel { yellow } else { white }, 3, true);
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
