//! `VIWndEula` (SETUP `viwndeula.cpp`): the licence. The text is the first
//! that reads of EULA_<language>.TXT in EULA.WAD on the card, the same in
//! the disc's `data2\eula.wad`, EULA.TXT on the card, then the disc's
//! `data\EULA.TXT`; this disc has no WAD and a computer no card, so it is
//! always the last: its bytes widened one to a character, tabs made spaces.
//!
//! Sixteen rows show in a 600-wide window (`ComputeTextRows` breaks them
//! where they pass 500 pixels), a scroll bar beside them; up and down
//! scroll, held they repeat (after 10 frames, then every 2). Left and right
//! choose between "I Do Not Accept" (first, where it starts) and "I Accept";
//! declining says so in a box (Notice) and the licence stays; accepting
//! gives the focus back, which `VISetup::Process` takes as done.
//!
//! Two shipped quirks are kept: a row that has no space to break at is cut
//! at the character that did not fit and the next row starts one character
//! back; and every row is drawn one character short (`strcpy16` is handed
//! the row's length in bytes, terminator included), which drops a row's
//! newline, its trailing space or that repeated character, and the text's
//! very last character.

use crate::pointer::{Hits, key, rows_rect};
use crate::textlist::{Message, PROMPT_OK};
use crate::ui::Layout;
use vi::fpu::{Ee, F};
use vi::ui::{UI_COLORS, Vui, Window, wide};

/// A row: where it starts in the text, its length, whether it ended at a
/// newline or a wrap.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Row {
    pub start: usize,
    pub len: usize,
    pub brk: bool,
}

/// The text as `Init` makes it of the file's bytes: each byte a
/// character, a tab a space, cut at a NUL.
pub fn text_of(bytes: &[u8]) -> Vec<u16> {
    bytes.iter().take_while(|&&b| b != 0).map(|&b| if b == 9 { b' ' as u16 } else { b as u16 }).collect()
}

/// `ComputeTextRows`: rows of `text` no wider than `max` pixels (`width`
/// measures), broken at a newline (kept in the row), else after the last
/// space; a carriage return becomes a space.
pub fn compute_rows(text: &mut [u16], max: i32, width: &dyn Fn(&[u16]) -> i32) -> Vec<Row> {
    let mut rows = Vec::new();
    let (mut i, mut space, mut start) = (0usize, 0usize, 0usize);
    while i < text.len() && text[i] != 0 {
        if text[i] == 13 {
            text[i] = b' ' as u16;
        }
        if text[i] == 10 {
            rows.push(Row { start, len: i + 1 - start, brk: true });
            i += 1;
            space = 0;
            start = i;
        } else {
            if text[i] == b' ' as u16 {
                space = i;
            }
            if width(&text[start..i]) > max {
                if space != 0 {
                    rows.push(Row { start, len: space + 1 - start, brk: true });
                    i = space + 1;
                } else {
                    // no space to break at: cut here, and start again one back
                    rows.push(Row { start, len: i - start, brk: true });
                    i -= 1;
                }
                start = i;
                space = 0;
            } else {
                i += 1;
            }
        }
    }
    if i > start {
        rows.push(Row { start, len: i - start, brk: false });
    }
    rows
}

/// A row as `OnDrawSelf` draws it: `strcpy16(line, len * 2, text)`, one
/// character short of its length.
pub fn row_text<'a>(text: &'a [u16], r: &Row) -> &'a [u16] {
    let n = r.len.saturating_sub(1).min(text.len().saturating_sub(r.start));
    &text[r.start..r.start + n]
}

#[derive(Default)]
pub struct Eula {
    window: Window,
    message: Message,
    /// `m_sel`: 0 "I Do Not Accept", 1 "I Accept"
    pub sel: i32,
    dirty: bool,
    pub text: Vec<u16>,
    /// the first row shown
    pub top: i32,
    pub rows: Vec<Row>,
    /// frames to the next held step, and which way (1 down, -1 up)
    repeat: i32,
    repeat_dir: i32,
    /// the decline box is up
    pub asking: bool,
    hits: Hits,
}

/// Where a pointer lands (`hits`): the two choices, the text's upper and
/// lower halves.
const DECLINE: i32 = 0;
const ACCEPT: i32 = 1;
const TEXT_UP: i32 = 2;
const TEXT_DOWN: i32 = 3;

impl Eula {
    /// `Init`: the text, its rows to be worked out at the first draw.
    pub fn init(&mut self, bytes: &[u8]) {
        self.text = text_of(bytes);
        self.dirty = true;
        self.rows.clear();
    }

    /// `OnMessage` 295: the window (600 wide, 18 rows, at the middle,
    /// titled 1596) opening on the first choice.
    pub fn focus(&mut self, vui: &Vui, r: &dyn Layout) {
        self.window.close(true);
        self.window.init(vui, r, [320, 224], 600, 18, 0);
        self.window.set_title(Some(&wide(vui.words.get(1596, vui.language))));
        self.window.open(false);
        self.asking = false;
        self.sel = 0;
    }

    fn last_top(&self) -> i32 {
        self.rows.len() as i32 - 17
    }

    /// `Down`: a row on, sound 11; at the end, sound 9.
    fn down(&mut self, cues: &mut Vec<i32>) {
        if self.top < self.last_top() {
            self.top += 1;
            cues.push(11);
        } else {
            cues.push(9);
        }
    }

    /// `Up`.
    fn up(&mut self, cues: &mut Vec<i32>) {
        if self.top > 0 {
            self.top -= 1;
            cues.push(11);
        } else {
            cues.push(9);
        }
    }

    /// `OnKeyDown`: true when the licence is accepted (the focus given back).
    pub fn key(&mut self, vui: &Vui, r: &mut dyn Layout, k: i32, now: i64, cues: &mut Vec<i32>) -> bool {
        self.repeat = 0;
        match k {
            17 | key::LEFT => {
                if !self.asking && self.sel == 1 {
                    cues.push(11);
                    self.sel = 0;
                    self.window.reset_tick_pulse(now);
                }
            }
            18 | key::RIGHT => {
                if !self.asking && self.sel == 0 {
                    cues.push(11);
                    self.sel = 1;
                    self.window.reset_tick_pulse(now);
                }
            }
            19 | key::UP => {
                if !self.asking {
                    self.repeat = 10;
                    self.repeat_dir = -1;
                    self.up(cues);
                }
            }
            20 | key::DOWN => {
                if !self.asking {
                    self.repeat = 10;
                    self.repeat_dir = 1;
                    self.down(cues);
                }
            }
            13 | key::CROSS => {
                if !self.asking {
                    if self.sel == 0 {
                        let w = |id: i32| vui.words.get(id, vui.language).to_string();
                        // the expansion's name: Frontiers (flag 2)
                        let game = w(if net::session::EXPANSION_FLAGS & 2 != 0 { 1803 } else { 1802 });
                        let a = w(1594).replacen("%S", &game, 1);
                        let b = w(1595).replacen("%S", &game, 1);
                        let msg = wide(&format!("{a}\n\n{b}"));
                        self.message.setup(vui, r, &wide(&w(785)), &msg, PROMPT_OK, [320, 224], 500);
                        self.asking = true;
                        cues.push(15);
                    } else {
                        cues.push(15);
                        return true;
                    }
                } else {
                    self.asking = false;
                    cues.push(8);
                }
            }
            _ => {}
        }
        false
    }

    /// `OnKeyUp`: a held direction stops.
    pub fn key_up(&mut self) {
        self.repeat = 0;
    }

    /// `OnDrawSelf`, one frame of SETUP's.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout, now: i64, cues: &mut Vec<i32>) {
        if self.asking {
            self.window.reset_tick_pulse(now);
        }
        if self.dirty {
            let max = self.window.size[0] - 100;
            let width = |s: &[u16]| vui.string_width(r, s);
            self.rows = compute_rows(&mut self.text, max, &width);
            self.dirty = false;
            self.top = 0;
        }
        if self.repeat != 0 {
            self.repeat -= 1;
            if self.repeat == 0 {
                self.repeat = 2;
                if self.repeat_dir > 0 {
                    self.down(cues);
                } else {
                    self.up(cues);
                }
            }
        }
        vui.blit_rect(r, 32, [-1, -1], [641, 449], 3, 1.0);
        self.window.draw(vui, r);
        let width = self.window.size[0];
        let t = F::<Ee>::from_int(self.top) / F::<Ee>::from_int(self.last_top());
        self.window.draw_frame_vscroll(vui, r, 40, 0, width - 80, 16, t.0);
        for i in 0..16 {
            let Some(row) = self.rows.get((i + self.top) as usize) else { break };
            let line = row_text(&self.text, row).to_vec();
            self.window.text(vui, r, &line, 40, i, UI_COLORS[0], 0);
        }
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let lit = |id: i32, on: bool| wide(&if on { format!("|q{}|w", w(id)) } else { w(id) });
        self.window.text(vui, r, &lit(1592, self.sel == 0), if self.sel == 0 { 34 } else { 50 }, 17, UI_COLORS[(self.sel == 0) as usize], 0);
        self.window.text(vui, r, &lit(1593, self.sel == 1), if self.sel == 1 { 234 } else { 250 }, 17, UI_COLORS[(self.sel == 1) as usize], 0);
        // a pointer's places: the choices, and the text's halves
        self.hits.clear();
        self.hits.push(rows_rect(&self.window, vui, r, 34, 17, 190, 1), DECLINE);
        self.hits.push(rows_rect(&self.window, vui, r, 234, 17, 190, 1), ACCEPT);
        self.hits.push(rows_rect(&self.window, vui, r, 40, 0, width - 80, 8), TEXT_UP);
        self.hits.push(rows_rect(&self.window, vui, r, 40, 8, width - 80, 8), TEXT_DOWN);
        if self.asking {
            self.message.draw(vui, r);
        }
    }

    /// A pointer at frame point `p`: a choice taken (moved to and pressed),
    /// the text's upper or lower half scrolled; the box's Ok while it is up.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.asking {
            return self.message.point(p).into_iter().collect();
        }
        match self.hits.at(p) {
            Some(DECLINE) if self.sel != 0 => vec![key::LEFT, key::CROSS],
            Some(ACCEPT) if self.sel != 1 => vec![key::RIGHT, key::CROSS],
            Some(DECLINE | ACCEPT) => vec![key::CROSS],
            Some(TEXT_UP) => vec![key::UP],
            Some(TEXT_DOWN) => vec![key::DOWN],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, mid, vui};

    /// 8 pixels a character, as the null raster measures.
    fn eight(s: &[u16]) -> i32 {
        s.len() as i32 * 8
    }

    /// The rows break at a newline (kept in the row), after the last space
    /// once a row passes the width, and with no space one character back;
    /// a row draws one character short.
    #[test]
    fn rows_break_as_compute_text_rows_breaks_them() {
        // 40 pixels: five characters fit, the sixth passes
        let mut t = wide("ab cdefg\nhijklmnop");
        let rows = compute_rows(&mut t, 40, &eight);
        let shown: Vec<String> = rows.iter().map(|r| String::from_utf16_lossy(row_text(&t, r))).collect();
        // "ab cdefg": at 'f' (index 5) "ab cde" (6 wide, 48 > 40) breaks after the space
        assert_eq!(rows[0], Row { start: 0, len: 3, brk: true });
        assert_eq!(shown[0], "ab", "the trailing space is the character not drawn");
        // "cdefg\n": fits (5 * 8 = 40, not past it), ends at the newline
        assert_eq!(shown[1], "cdefg");
        assert_eq!(rows[1].len, 6, "the newline is in the row");
        // "hijklmnop": no space; cut at 'n' (6 past 40), the next starts at 'm'
        assert_eq!(rows[2], Row { start: 9, len: 6, brk: true });
        assert_eq!(shown[2], "hijkl");
        assert_eq!(rows[3].start, 14, "one character back: 'm' again");
        assert_eq!(shown[3], "mno", "the text's last character is not drawn");
        assert!(!rows[3].brk);
        // the control: a carriage return is a space, not a break
        let mut c = wide("a\rb");
        assert_eq!(compute_rows(&mut c, 400, &eight).len(), 1);
        assert_eq!(c[1], b' ' as u16);
    }

    #[test]
    fn the_bytes_are_widened_and_tabs_made_spaces() {
        assert_eq!(text_of(b"a\tb\xe9\0zz"), vec![97, 32, 98, 0xE9]);
    }

    fn eula(lines: usize) -> (Eula, Vui, NullRaster) {
        let (v, mut r) = (vui(), NullRaster);
        let mut e = Eula::default();
        let text: String = (0..lines).map(|k| format!("line {k}\n")).collect();
        e.init(text.as_bytes());
        e.focus(&v, &r);
        e.draw(&v, &mut r, 0, &mut Vec::new());
        (e, v, r)
    }

    /// Down a row a press, sound 11, and 9 at the end; held, the next step
    /// after 10 frames, then every 2; a key let go stops it.
    #[test]
    fn scrolling_steps_and_repeats_as_held() {
        let (mut e, v, mut r) = eula(30);
        assert_eq!(e.rows.len(), 30);
        let mut cues = Vec::new();
        e.key(&v, &mut r, key::DOWN, 0, &mut cues);
        assert_eq!((e.top, cues.clone()), (1, vec![11]));
        for _ in 0..9 {
            e.draw(&v, &mut r, 0, &mut cues);
        }
        assert_eq!(e.top, 1, "nothing for nine frames");
        e.draw(&v, &mut r, 0, &mut cues);
        assert_eq!(e.top, 2, "the tenth repeats");
        e.draw(&v, &mut r, 0, &mut cues);
        e.draw(&v, &mut r, 0, &mut cues);
        assert_eq!(e.top, 3, "then every second frame");
        e.key_up();
        for _ in 0..10 {
            e.draw(&v, &mut r, 0, &mut cues);
        }
        assert_eq!(e.top, 3, "let go: no more");
        // 30 rows, 17 shown with the last: the end is 13
        e.top = 13;
        cues.clear();
        e.key(&v, &mut r, 20, 0, &mut cues);
        assert_eq!((e.top, cues), (13, vec![9]), "at the end: sound 9");
        // the control: up at the top is 9, not a move
        let mut c = Vec::new();
        e.top = 0;
        e.key(&v, &mut r, 19, 0, &mut c);
        assert_eq!((e.top, c), (0, vec![9]));
    }

    /// Declining asks and keeps the licence; Enter again closes the box;
    /// accepting is done.
    #[test]
    fn declining_asks_and_only_accepting_goes_on() {
        let (mut e, v, mut r) = eula(30);
        let mut cues = Vec::new();
        assert_eq!(e.sel, 0, "it starts on I Do Not Accept");
        assert!(!e.key(&v, &mut r, 13, 0, &mut cues));
        assert!(e.asking);
        e.key(&v, &mut r, 18, 0, &mut cues);
        assert_eq!(e.sel, 0, "the choice does not move under the box");
        assert!(!e.key(&v, &mut r, key::CROSS, 0, &mut cues));
        assert!(!e.asking, "Enter closes the box");
        e.key(&v, &mut r, key::RIGHT, 0, &mut cues);
        assert_eq!(e.sel, 1);
        assert!(e.key(&v, &mut r, 13, 0, &mut cues), "I Accept: done");
        assert_eq!(cues, vec![15, 8, 11, 15]);
    }

    /// A pointer: the choices and the text's halves.
    #[test]
    fn a_pointer_chooses_and_scrolls() {
        let (mut e, v, mut r) = eula(30);
        e.draw(&v, &mut r, 0, &mut Vec::new());
        let accept = rows_rect(&e.window, &v, &r, 234, 17, 190, 1);
        assert_eq!(e.point(mid(accept)), vec![key::RIGHT, key::CROSS]);
        let decline = rows_rect(&e.window, &v, &r, 34, 17, 190, 1);
        assert_eq!(e.point(mid(decline)), vec![key::CROSS], "already chosen: pressed");
        let low = rows_rect(&e.window, &v, &r, 40, 12, 100, 1);
        assert_eq!(e.point(mid(low)), vec![key::DOWN]);
        // the control: without the hit notes, nothing
        e.hits.clear();
        assert!(e.point(mid(accept)).is_empty());
    }
}
