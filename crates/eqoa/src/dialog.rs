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
}
