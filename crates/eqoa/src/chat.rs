//! The chat box: `VIChatText`, the 32-line ring it keeps, and
//! `VIWndChat::OnDrawSelf`, the window that slides up from the bottom of
//! the screen to show the lines added in the last 20 seconds (at most
//! five), or the last five lines in a fixed 85-pixel box while its mode is
//! set or it is being scrolled.

use vi::ui::{Raster, Vui, Window};

const LINES: usize = 32;

#[derive(Clone, Debug, Default)]
pub struct ChatLine {
    pub text: Vec<u16>,
    pub color: [f32; 4],
    /// 1 while it is fresh (`Age` clears it after 20 s).
    pub kind: i32,
    pub time: u32,
}

/// `VIChatText`.
#[derive(Clone, Debug)]
pub struct ChatText {
    width: i32,
    lines: Vec<ChatLine>,
    head: usize,
    tail: usize,
}

impl ChatText {
    /// `Init(app, width)`: lines wrap at `width - 30`.
    pub fn new(width: i32) -> Self {
        let mut t = ChatText { width: width - 30, lines: Vec::new(), head: 0, tail: 0 };
        t.reset();
        t
    }

    pub fn reset(&mut self) {
        self.lines = (0..LINES).map(|_| ChatLine { text: Vec::new(), color: [1.0; 4], kind: 0, time: 0 }).collect();
        self.head = 0;
        self.tail = 0;
    }

    /// `Age`: lines older than 20 s are no longer shown.
    pub fn age(&mut self, now: u32) {
        for l in &mut self.lines {
            if now.wrapping_sub(l.time) as i32 > 20000 {
                l.kind = 0;
            }
        }
    }

    pub fn first(&self) -> usize {
        self.head
    }
    pub fn end(&self) -> usize {
        self.tail
    }
    pub fn next(i: usize) -> usize {
        if i == LINES - 1 { 0 } else { i + 1 }
    }
    pub fn prev(i: usize) -> usize {
        if i == 0 { LINES - 1 } else { i - 1 }
    }
    pub fn line(&self, i: usize) -> Option<&ChatLine> {
        self.lines.get(i)
    }

    /// `AddText`: wraps at the width, at most 63 characters a line, at the
    /// last space when there is one; a continuation line starts with a
    /// space. `width` measures a run of text in the font.
    pub fn add_text(&mut self, s: &[u16], c: [f32; 4], now: u32, width: impl Fn(&[u16]) -> i32) {
        let at = |s: &[u16], i: usize| s.get(i).copied().unwrap_or(0);
        let (mut w, mut space, mut pos, mut n) = (0, 0usize, 0usize, 0usize);
        while at(s, pos) != 0 {
            while w < self.width && n < 63 && self.tail < LINES && at(s, pos) != 0 {
                if at(s, pos) == b' ' as u16 {
                    space = n;
                }
                if at(s, pos) == b'\n' as u16 && at(s, pos + 1) != 0 {
                    pos += 1;
                    self.add_line(c, now);
                    space = 0;
                    w = 0;
                    n = 1;
                    self.lines[self.tail].text = vec![b' ' as u16];
                }
                if at(s, pos) != 0 {
                    let t = &mut self.lines[self.tail].text;
                    t.truncate(n);
                    t.push(at(s, pos));
                    n += 1;
                    pos += 1;
                    w = width(&self.lines[self.tail].text);
                }
            }
            if at(s, pos) == 0 {
                break;
            }
            if self.lines[self.tail].text.get(n - 1) != Some(&(b' ' as u16)) && space > 0 {
                self.lines[self.tail].text.truncate(space);
                pos -= n - space - 1;
            }
            self.add_line(c, now);
            space = 0;
            n = 1;
            self.lines[self.tail].text = vec![b' ' as u16];
            w = 0;
        }
        self.add_line(c, now);
    }

    /// `AddLine`: the line at the tail is finished; the ring moves on.
    fn add_line(&mut self, c: [f32; 4], now: u32) {
        let l = &mut self.lines[self.tail];
        l.color = c;
        l.kind = 1;
        l.time = now;
        if self.head == 0 && self.tail < LINES - 1 {
            self.tail += 1;
        } else {
            self.head = Self::next(self.head);
            self.tail = Self::next(self.tail);
        }
        // The new tail line starts empty (it is overwritten as it fills).
        self.lines[self.tail].text.clear();
    }
}

/// `VIWndChat`.
pub struct WndChat {
    /// Nonzero: the fixed box, every line.
    pub mode: i32,
    height: i32,
    target: i32,
    lines: i32,
    pub text: ChatText,
    window: Window,
}

impl WndChat {
    /// `Init`: the window is 462 wide and lines wrap at its width less 45.
    pub fn new() -> Self {
        WndChat { mode: 0, height: 0, target: 0, lines: 0, text: ChatText::new(462 - 45), window: Window::default() }
    }

    /// `CountLines`: the fresh lines (or five in a fixed mode), at most five.
    fn count_lines(&self) -> i32 {
        if self.mode != 0 {
            return 5;
        }
        let mut n = 0;
        let mut i = self.text.first();
        while i != self.text.end() {
            if self.text.line(i).is_some_and(|l| l.kind == 1) {
                n += 1;
            }
            i = ChatText::next(i);
        }
        n.min(5)
    }

    /// `OnDrawSelf`, scrolling aside: the box eases to its height at 200
    /// pixels a second (`dt` seconds since the last frame).
    pub fn draw(&mut self, vui: &Vui, ui: &mut dyn Raster, now: u32, dt: f32) {
        self.text.age(now);
        self.window.init(vui, ui, [422, 390], 462, 6, 1);
        if self.mode != 0 {
            vui.blit_ex_size(ui, 32, [200, 331], [442, 118], 0, 1.0, 0);
        }
        let it = self.text.end();
        self.lines = self.count_lines();
        let h = vui.font_height(ui);
        if self.mode != 0 {
            self.target = 85;
            self.height = 85;
            self.window.open(true);
        } else if self.lines == 0 {
            self.target = -((h - 4) * 2);
            self.window.close(false);
        } else {
            self.target = (h - 4) * self.lines;
            self.window.open(false);
        }
        self.window.center = [422, 475 - self.height];
        if self.window.is_open() || self.window.is_opening() || self.window.is_closing() {
            self.window.draw(vui, ui);
        }
        let mut row = (self.lines - 1).max(0);
        let mut i = ChatText::prev(it);
        let stop = ChatText::prev(self.text.first());
        while i != stop && row >= 0 {
            if let Some(l) = self.text.line(i)
                && !l.text.is_empty()
                && l.text[0] != 0
                && (l.kind == 1 || self.mode != 0)
            {
                let (text, color) = (l.text.clone(), l.color);
                self.window.text_box(vui, ui, &text, 25, row, self.window.size[0] - 45, 1, color, 0);
                row -= 1;
            }
            i = ChatText::prev(i);
        }
        let step = (dt * 200.0) as i32;
        if self.height < self.target {
            self.height = (self.height + step).min(self.target);
        } else if self.target < self.height {
            self.height = (self.height - step).max(self.target);
        }
    }
}
