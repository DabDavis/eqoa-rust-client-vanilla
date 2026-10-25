//! The chat box: `VIChatText`, the ring of lines it keeps, and
//! `VIWndChat::OnDrawSelf`, the window that slides up from the bottom of
//! the screen to show the lines added in the last 20 seconds (at most
//! five), or the last five lines in a fixed 85-pixel box while its mode is
//! set or it is being scrolled: R2 held (`VIWndGame` mode 33) scrolls the
//! history with up and down, which repeat while held.

use vi::ui::{Raster, Vui, Window};

/// The ring's size. The game's is 32 (`VIChatText::m_lines`, 5,136 bytes of
/// the PS2's memory), holding the last 31 lines; the client's holds the last
/// 1,000 (F3 2026-10-21: lines went off the top of the history too soon),
/// about 180 KB, scrolled as the game scrolls its 31.
const LINES: usize = 1001;

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
    /// `Count`: the lines held (`LINES - 1` once the ring has gone round).
    pub fn count(&self) -> usize {
        if self.tail < self.head { LINES - 1 } else { self.tail - self.head }
    }
    /// `Index`: line `i`'s place counting from the oldest.
    pub fn index(&self, i: usize) -> usize {
        if i < self.head { LINES - self.head + i } else { i - self.head }
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

/// A shout's colour, (63, 255, 63): 0x0A7B's three bytes as Sandstorm sends
/// a shout (`pcaps/ps2_sandstorm.pcap`, `eqoa_live_capture.pcap`) and
/// go-eqoa-server copies it.
pub const SHOUT_COLOR: [u8; 3] = [63, 255, 63];

/// Whether a line the server sent is someone else's shout: in the shout's
/// colour and "<name> shouts: <text>", the name holding no colon (so a say,
/// "<name>: ...", that quotes " shouts: " is not one), and the name not the
/// player's own (`own`: their own shout, as the server echoes it, is kept).
/// The game has no such test: the server stops sending shouts once told
/// (0x0C01 [1, 0]); a server that does not listen still has them dropped
/// while Ignore Shouts is kept (`settings::shouts`).
pub fn is_shout(text: &str, color: [f32; 4], own: &str) -> bool {
    let byte = |v: f32| (v * 255.0).round() as i32;
    if (0..3).any(|i| byte(color[i]) != SHOUT_COLOR[i] as i32) {
        return false;
    }
    let Some((name, _)) = text.split_once(" shouts: ") else { return false };
    !name.is_empty() && !name.contains(':') && !name.eq_ignore_ascii_case(own)
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
    /// `m_scroll`: 1 while scrolling (`SetScroll`)
    scroll: i32,
    /// the first line shown while scrolling, and the line after the last
    top: usize,
    bottom: usize,
    /// the scroll key held (19, 20, 138, 139), and when it repeats
    held: i32,
    repeat: f32,
    /// `PlaySound`s to make (11 a step, 9 at an end)
    pub cues: Vec<i32>,
}

impl WndChat {
    /// Whether frame point `p` is on the box while it shows (before the right
    /// side's move), for a pointer.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        let (tl, br) = (self.window.top_left(), self.window.bottom_right());
        (self.window.is_open() || self.window.is_opening()) && p[0] >= tl[0] as f32 && p[0] < br[0] as f32 && p[1] >= tl[1] as f32 && p[1] < br[1] as f32
    }

    /// `Init`: the window is 462 wide and lines wrap at its width less 45.
    pub fn new() -> Self {
        WndChat { mode: 0, height: 0, target: 0, lines: 0, text: ChatText::new(462 - 45), window: Window::default(), scroll: 0, top: 0, bottom: 0, held: 0, repeat: 0.0, cues: Vec::new() }
    }

    /// `CountLines`: the fresh lines (every line while scrolling; five in a
    /// fixed mode), at most five.
    fn count_lines(&self) -> i32 {
        if self.mode != 0 {
            return 5;
        }
        let mut n = 0;
        let mut i = self.text.first();
        while i != self.text.end() {
            if self.text.line(i).is_some_and(|l| l.kind == 1 || self.scroll == 1) {
                n += 1;
            }
            i = ChatText::next(i);
        }
        n.min(5)
    }

    /// `SetScroll`: 1 opens the view on the last five lines, 0 ends it.
    pub fn set_scroll(&mut self, on: i32) {
        self.scroll = on;
        if on == 1 {
            self.target = 85;
            self.height = 85;
            self.top = self.text.end();
            self.bottom = self.top;
            for _ in 0..5 {
                if self.top == self.text.first() {
                    break;
                }
                self.top = ChatText::prev(self.top);
            }
        } else {
            self.bottom = 0;
        }
    }

    /// `ScrollDown`: a line later, unless at the end.
    fn scroll_down(&mut self) {
        if self.bottom != self.text.end() {
            self.cues.push(11);
            self.bottom = ChatText::next(self.bottom);
            self.top = ChatText::next(self.top);
        } else {
            self.cues.push(9);
        }
    }

    /// `ScrollUp`: a line earlier, unless at the first.
    fn scroll_up(&mut self) {
        if self.top != self.text.first() {
            self.cues.push(11);
            self.top = ChatText::prev(self.top);
            self.bottom = ChatText::prev(self.bottom);
        } else {
            self.cues.push(9);
        }
    }

    /// `OnKeyDown`: up and down scroll, and repeat while held. False for a
    /// key it passes up to the game window (only L3, 146).
    pub fn key_down(&mut self, key: i32) -> bool {
        self.held = 0;
        match key {
            19 | 138 => {
                self.held = key;
                self.repeat = 0.45;
                self.scroll_up();
            }
            20 | 139 => {
                self.held = key;
                self.repeat = 0.45;
                self.scroll_down();
            }
            146 => return false,
            _ => {}
        }
        true
    }

    /// `OnKeyUp`: the held key let go. False for a key it passes up to the
    /// game window (all but the scroll keys).
    pub fn key_up(&mut self, key: i32) -> bool {
        if key == self.held {
            self.held = 0;
        }
        if matches!(key, 19 | 20 | 138 | 139) {
            return true;
        }
        self.held = 0;
        false
    }

    /// `OnDrawSelf`, scrolling aside: the box eases to its height at 200
    /// pixels a second (`dt` seconds since the last frame).
    pub fn draw(&mut self, vui: &Vui, ui: &mut dyn Raster, now: u32, dt: f32) {
        self.text.age(now);
        // a held scroll key repeats every 1/20 s after the first 0.45 s
        if matches!(self.held, 19 | 20 | 138 | 139) {
            self.repeat -= dt;
            if self.repeat <= 0.0 {
                self.repeat = 0.05;
                if matches!(self.held, 138 | 19) {
                    self.scroll_up();
                } else {
                    self.scroll_down();
                }
            }
        }
        self.window.init(vui, ui, [422, 390], 462, 6, 1);
        if self.mode != 0 {
            crate::ui::chat_strip(vui, ui, [200, 331], [442, 118]);
        }
        let it = if self.scroll == 1 { self.bottom } else { self.text.end() };
        self.lines = if self.scroll == 1 { 5 } else { self.count_lines() };
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
                && (self.scroll == 1 || (self.scroll == 0 && l.kind == 1) || self.mode != 0)
            {
                let (text, color) = (l.text.clone(), l.color);
                self.window.text_box(vui, ui, &text, 25, row, self.window.size[0] - 45, 1, color, 0);
                row -= 1;
            }
            i = ChatText::prev(i);
        }
        if self.scroll == 1 {
            let n = self.text.count();
            let t = if n > 5 { self.text.index(self.top) as f32 / (n - 5) as f32 } else { 0.0 };
            let tl = self.window.top_left();
            let at = [tl[0] + 10, tl[1] + 10];
            self.window.draw_vscroll(vui, ui, at, self.window.bottom_right()[1] - tl[1] - 40, t);
        }
        let step = (dt * 200.0) as i32;
        if self.height < self.target {
            self.height = (self.height + step).min(self.target);
        } else if self.target < self.height {
            self.height = (self.height - step).max(self.target);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shout is someone else's "<name> shouts: <text>" in the shout's
    /// colour, as Sandstorm's 0x0A7B carries it (the capture's own line); the
    /// controls: the same words white, a say quoting " shouts: ", the
    /// player's own shout, and another colour are not.
    #[test]
    fn a_shout_is_someone_elses_line_in_the_shouts_colour() {
        let green = [63.0 / 255.0, 1.0, 63.0 / 255.0, 1.0];
        assert!(is_shout("Powerbrick shouts: yrs?", green, "Gnolie"), "the capture's shout");
        assert!(is_shout("Lord Bob shouts: run", green, "Gnolie"), "a name of two words");
        assert!(!is_shout("Powerbrick shouts: yrs?", [1.0; 4], "Gnolie"), "white: not a shout");
        assert!(!is_shout("Bob: Gnolie shouts: too much", green, "Gnolie"), "a say quoting a shout");
        assert!(!is_shout("Gnolie shouts: lfg", green, "Gnolie"), "the player's own");
        assert!(!is_shout("gnolie shouts: lfg", green, "Gnolie"), "the player's own, any case");
        assert!(!is_shout("Powerbrick says: yrs?", green, "Gnolie"), "no shout in it");
        assert!(!is_shout(" shouts: nameless", green, "Gnolie"), "no name");
        assert!(!is_shout("Powerbrick shouts: yrs?", [64.0 / 255.0, 1.0, 63.0 / 255.0, 1.0], "Gnolie"), "a colour a step off");
    }

    /// The history holds the last 1,000 lines, and R2's scroll goes back
    /// through all of them a line a step, sounding 9 at the oldest. The
    /// game's ring of 32 kept 31 (F3 2026-10-21: "at least 1000 lines").
    #[test]
    fn the_history_keeps_a_thousand_lines() {
        let mut c = WndChat::new();
        let width = |s: &[u16]| s.len() as i32 * 8;
        for k in 0..1500 {
            c.text.add_text(&crate::ui::wide(&format!("line {k}")), [1.0; 4], 0, width);
        }
        assert_eq!(c.text.count(), 1000);
        assert_eq!(c.text.line(c.text.first()).unwrap().text, crate::ui::wide("line 500"));
        assert_eq!(c.text.line(ChatText::prev(c.text.end())).unwrap().text, crate::ui::wide("line 1499"));
        // the scroll opens on the last five, and steps back to the first
        c.set_scroll(1);
        let mut steps = 0;
        while c.top != c.text.first() {
            c.scroll_up();
            steps += 1;
            assert!(steps <= 1000, "the scroll never reached the first line");
        }
        assert_eq!(steps, 995);
        assert_eq!(c.text.index(c.top), 0);
        c.cues.clear();
        c.scroll_up();
        assert_eq!(c.cues, vec![9], "the oldest line: the end's sound");
        // and back down to the newest
        let mut down = 0;
        while c.bottom != c.text.end() {
            c.scroll_down();
            down += 1;
        }
        assert_eq!(down, 995);
    }

    /// A window that fixes the chat box (the merchant, the bank, a trade)
    /// shows the world under it: the strip the game lays under the fixed box
    /// (atlas 32 at (200, 331), 442 by 118) is left out with the windows'
    /// backdrop (`ui::chat_strip`; F3 20261023-080729). The control: the
    /// game's own blit, made on the same recorder, is seen there.
    #[test]
    fn a_fixed_chat_box_lays_no_strip() {
        use crate::pointer::testing::{Wide, vui};
        let vui = vui();
        let strip = (render::ui::Anchor::Middle, [200, 331], [442, 118]);
        let mut c = WndChat::new();
        c.mode = 1;
        let mut r = Wide::new(0);
        c.draw(&vui, &mut r, 0, 0.0);
        assert!(!r.2.contains(&strip), "no strip under the fixed box");
        vui.blit_ex_size(&mut r, 32, [200, 331], [442, 118], 0, 1.0, 0);
        assert!(r.2.contains(&strip), "the control: the game's strip is recorded");
    }
}
