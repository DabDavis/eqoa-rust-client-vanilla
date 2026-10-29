//! Two of CLIENT's small UI pieces the Class Mastery windows embed:
//! `VITextList` (`vitextlist.cpp`), a scrolling, selectable list of lines
//! of up to four columns, and `VIWndMessage` (`viwndmessage.cpp`), the
//! titled message box with an Ok, Ok/Cancel or Yes/No row.

use crate::pointer::{Hits, key, rows_rect};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// `VITextLine`: each column's x and text.
#[derive(Clone, Debug, Default)]
struct Line {
    cols: usize,
    x: [i32; 4],
    text: [Option<Vec<u16>>; 4],
}

/// `VITextList`.
#[derive(Clone, Debug, Default)]
pub struct TextList {
    pub width: i32,
    pub rows: i32,
    pub top: i32,
    /// -1 none
    pub sel: i32,
    pub show_sel: bool,
    lines: Vec<Line>,
    /// the lines drawn, by index, for a pointer
    hits: Hits,
}

impl TextList {
    /// `Init(ui, width, rows)`: emptied, the selection shown.
    pub fn init(&mut self, width: i32, rows: i32) {
        self.clear();
        self.width = width;
        self.rows = rows;
        self.show_sel = true;
    }

    /// `Free`/`Clear`: no lines, the top at 0, no selection.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.top = 0;
        self.sel = -1;
    }

    pub fn count(&self) -> i32 {
        self.lines.len() as i32
    }

    /// The line drawn under frame point `p` (`pointer`).
    pub fn line_at(&self, p: [f32; 2]) -> Option<i32> {
        self.hits.at(p)
    }

    /// `AddLine`: one column; the line's index.
    pub fn add_line(&mut self, text: &[u16]) -> i32 {
        let mut l = Line { cols: 1, ..Default::default() };
        l.text[0] = Some(text.to_vec());
        self.lines.push(l);
        self.lines.len() as i32 - 1
    }

    /// `AddText`: a line per '\n', and a break at the last space (or at the
    /// character) where the line would be wider than `width - 24`.
    pub fn add_text(&mut self, vui: &Vui, r: &dyn Raster, text: &[u16]) {
        let text: Vec<u16> = text.iter().copied().take_while(|&c| c != 0).collect();
        let (mut start, mut i, mut space) = (0usize, 0usize, 0usize);
        while i < text.len() {
            if text[i] == b'\n' as u16 {
                i += 1;
                space = 0;
                self.add_line(&text[start..i]);
                start = i;
            } else {
                if text[i] == b' ' as u16 {
                    space = i;
                }
                if vui.string_width(r, &text[start..i]) > self.width - 24 {
                    if space != 0 {
                        self.add_line(&text[start..space + 1]);
                        i = space + 1;
                    } else {
                        self.add_line(&text[start..i]);
                    }
                    start = i;
                    space = 0;
                } else {
                    i += 1;
                }
            }
        }
        if i > start {
            self.add_line(&text[start..i]);
        }
    }

    /// `SetColumns(line, n, x)`.
    pub fn set_columns(&mut self, line: i32, x: &[i32]) {
        if let Some(l) = self.lines.get_mut(line as usize) {
            l.cols = x.len().min(4);
            l.x[..l.cols].copy_from_slice(&x[..l.cols]);
        }
    }

    /// `SetText(line, col, s)`.
    pub fn set_text(&mut self, line: i32, col: usize, s: &[u16]) {
        if let Some(l) = self.lines.get_mut(line as usize) {
            l.text[col] = Some(s.to_vec());
        }
    }

    /// `SetSel(i)`: clamped to the lines (-1 when there are none).
    pub fn set_sel(&mut self, i: i32) {
        self.sel = i.max(0).min(self.count() - 1);
    }

    /// `Scroll(d)`: true when already at that end.
    pub fn scroll(&mut self, d: i32) -> bool {
        let n = self.count();
        if (d < 0 && self.top == 0) || (d > 0 && self.top >= n - self.rows) {
            return true;
        }
        let t = self.top + d;
        self.top = if t < 0 { 0 } else { t.min(n - self.rows) };
        false
    }

    /// `Draw(w, x, y)`: the selection kept in view, the frame (with a
    /// scroll bar when the lines do not fit), the tick and each column.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, w: &Window, x: i32, y: i32) {
        let n = self.count();
        if self.sel != -1 {
            if self.sel < self.top {
                self.top = self.sel;
            }
            if self.sel >= self.top + self.rows {
                self.top = self.sel - self.rows + 1;
            }
        }
        if n > self.rows || self.top > 0 {
            w.draw_frame_vscroll(vui, r, x, y, self.width, self.rows, self.top as f32 / (n - self.rows) as f32);
        } else {
            w.draw_frame(vui, r, x, y, self.width, self.rows, 0);
        }
        if self.show_sel && n == 0 {
            w.tick_r(vui, r, x, y, false);
        }
        self.hits.clear();
        for row in 0..self.rows {
            let k = self.top + row;
            if k >= n {
                break;
            }
            self.hits.push(rows_rect(w, vui, r, x, y + row, self.width, 1), k);
            if self.sel != -1 && k == self.sel && self.show_sel {
                w.tick_r(vui, r, x, y + row, true);
            }
            let l = &self.lines[k as usize];
            for col in 0..l.cols {
                let Some(t) = &l.text[col] else { continue };
                let width = if col + 1 == l.cols { self.width - l.x[col] } else { l.x[col + 1] - l.x[col] };
                let sel = self.sel != -1 && k == self.sel;
                let c = if !self.show_sel { 2 } else { sel as usize };
                w.text_box(vui, r, t, x + l.x[col], y + row, width, 1, UI_COLORS[c], 2);
            }
        }
    }
}

/// `VIMESSAGEPROMPT`.
pub const PROMPT_OK: i32 = 0;
pub const PROMPT_OKCANCEL: i32 = 1;
pub const PROMPT_YESNO: i32 = 2;

/// `VIWndMessage`: shown while `visible`; Enter or X closes it with the
/// selection (the parent's 307).
#[derive(Clone, Debug, Default)]
pub struct Message {
    pub visible: bool,
    window: Window,
    title: Vec<u16>,
    text: Vec<u16>,
    prompt: i32,
    center: [i32; 2],
    width: i32,
    rows: i32,
    sel: i32,
    /// the choices drawn, for a pointer
    hits: Hits,
}

impl Message {
    /// `Setup(title, text, prompt, center, width)`, then shown: `OnMessage`
    /// 295 opens it on the second choice of two.
    #[allow(clippy::too_many_arguments)]
    pub fn setup(&mut self, vui: &Vui, r: &mut dyn Raster, title: &[u16], text: &[u16], prompt: i32, center: [i32; 2], width: i32) {
        self.title = title.to_vec();
        self.text = text.to_vec();
        self.prompt = prompt;
        self.center = center;
        self.width = width;
        self.window.init(vui, r, center, width, 20, 0);
        self.rows = self.window.count_lines(vui, r, text, width - 80) + 3;
        // 295
        self.window.close(true);
        self.window.init(vui, r, center, width, self.rows, 0);
        self.window.open(false);
        self.window.set_title(Some(&self.title));
        self.sel = if prompt == PROMPT_OK { 0 } else { 1 };
        self.visible = true;
    }

    /// A pointer at frame point `p`: a choice chosen at once (✕).
    pub fn point(&mut self, p: [f32; 2]) -> Option<i32> {
        self.sel = self.hits.at(p)?;
        Some(key::CROSS)
    }

    /// The title and the text it was set up with.
    #[cfg(test)]
    pub fn texts(&self) -> (&[u16], &[u16]) {
        (&self.title, &self.text)
    }

    /// `OnKeyDown`: Some(selection) when it closes.
    pub fn key(&mut self, key: i32, now: i64, cues: &mut Vec<i32>) -> Option<i32> {
        match key {
            17 | 136 => {
                if self.sel > 0 {
                    self.sel -= 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                }
            }
            18 | 137 => {
                if self.sel < (self.prompt != 0) as i32 {
                    self.sel += 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                }
            }
            13 | 134 => {
                self.visible = false;
                cues.push(if self.prompt == PROMPT_OK || self.sel == 0 { 13 } else { 8 });
                return Some(self.sel);
            }
            _ => {}
        }
        None
    }

    /// `OnDrawSelf`: the text, and the choices centred on the window's
    /// thirds, the chosen one wrapped in |q..|w.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster) {
        let w = self.window.size[0];
        let col = [w / 3, w / 3 * 2];
        self.window.draw(vui, r);
        let c = UI_COLORS[0];
        self.window.text_box(vui, r, &self.text, 40, 1, w - 80, self.rows - 2, c, 8);
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        let choice = |on: bool, id: i32| wide(&if on { format!("|q{}|w", word(id)) } else { word(id) });
        self.hits.clear();
        match self.prompt {
            PROMPT_OK => {
                self.window.text_box(vui, r, &choice(true, 786), 0, self.rows - 1, w, 1, c, 8);
                self.hits.push(rows_rect(&self.window, vui, r, w / 2 - 80, self.rows - 1, 160, 1), 0);
            }
            p => {
                let (a, b) = if p == PROMPT_OKCANCEL { (786, 213) } else { (21, 22) };
                self.window.text_box(vui, r, &choice(self.sel == 0, a), col[0] - 100, self.rows - 1, 200, 1, c, 8);
                self.window.text_box(vui, r, &choice(self.sel == 1, b), col[1] - 100, self.rows - 1, 200, 1, c, 8);
                // each its half of the row, split between their centres
                let left = self.window.top_left()[0];
                let split = left + (col[0] + col[1]) / 2;
                for (k, (x0, x1)) in [(left, split), (split, left + w)].into_iter().enumerate() {
                    let mut rr = rows_rect(&self.window, vui, r, 0, self.rows - 1, 0, 1);
                    (rr[0], rr[2]) = (x0, x1);
                    self.hits.push(rr, k as i32);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Yes/No message's choices each take their half of the row, split
    /// between their centres (in a narrow one they used to overlap, and the
    /// second took the first's right side).
    #[test]
    fn a_message_choice_takes_its_half() {
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        let mut m = Message::default();
        m.setup(&vui, &mut r, &wide("T"), &wide("Sure?"), PROMPT_YESNO, [320, 200], 300);
        m.draw(&vui, &mut r);
        let left = m.window.top_left()[0] as f32;
        let y = m.window.row_center(&vui, &r, m.rows - 1)[1] as f32;
        assert_eq!(m.point([left + 140.0, y]), Some(key::CROSS));
        assert_eq!(m.sel, 0, "left of the split (150)");
        assert_eq!(m.point([left + 160.0, y]), Some(key::CROSS));
        assert_eq!(m.sel, 1);
        assert_eq!(m.point([left + 20.0, y]), Some(key::CROSS));
        assert_eq!(m.sel, 0, "the first's half reaches the window's edge");
        assert_eq!(m.point([left - 10.0, y]), None, "off the window");
        assert_eq!(m.point([left + 140.0, y - 60.0]), None, "above the choices");
    }

    /// `SetSel` clamps to the lines; `Scroll` reports the end it is at and
    /// moves only inside `0..=count-rows`.
    #[test]
    fn selection_and_scroll_clamp() {
        let mut l = TextList::default();
        l.init(100, 3);
        l.set_sel(0);
        assert_eq!(l.sel, -1, "no lines: none");
        for k in 0..5 {
            l.add_line(&wide(&k.to_string()));
        }
        l.set_sel(9);
        assert_eq!(l.sel, 4);
        assert!(l.scroll(-1), "already at the top");
        assert!(!l.scroll(1));
        assert!(!l.scroll(5));
        assert_eq!(l.top, 2, "count - rows at most");
        assert!(l.scroll(1), "already at the bottom");
    }
}
