//! The chat box: `VIChatText`, the ring of lines it keeps, and
//! `VIWndChat::OnDrawSelf`, the window that slides up from the bottom of
//! the screen to show the lines added in the last 20 seconds (at most
//! five), or the last five lines in a fixed 85-pixel box while its mode is
//! set or it is being scrolled: R2 held (`VIWndGame` mode 33) scrolls the
//! history with up and down, which repeat while held.
//!
//! Selecting lines for the clipboard is the client's own (the user, 2026-11-07:
//! "yes lets add that"), with a mouse: a drag over the lines picks whole rows,
//! on past the box's edges through the kept history, and the Copy key takes them
//! as plain text (`copy_text`), or with none picked the newest line. The game's
//! drawing is untouched: `draw` notes where it put each row and the highlight
//! goes over them (`draw_selection`). A selection is by the lines' own numbers,
//! so it holds while the ring moves on under it, and it keeps the view it was
//! made in until it is cleared: Esc or a click anywhere else.
//!
//! Item links are the client's own too (`itemlink`): a line the server relayed
//! with links keeps where each lies through the wrap (`RowLink`), and the box
//! draws them in the links' purple (`show_links`), a click on one opening it
//! (`Let::Link`); with the history held (mode 33) left walks up through them and
//! right back down (`walk_older`, `walk_newer`), the one lit washed, and the
//! history let go forgets the walk.

use std::cell::RefCell;
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
    /// Its number, counting every line the box has been given (the client's: a
    /// selection's hold on it, where the ring's places get used again).
    seq: u64,
    /// The item links on this row (the client's, `itemlink`).
    pub links: Vec<RowLink>,
}

/// An item link's part of a row: where it lies in the row's text (UTF-16 units),
/// the server's link id, and which link of all the box was given it is (one that
/// wrapped is on two rows, under one number).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RowLink {
    pub start: u16,
    pub len: u16,
    pub id: u32,
    pub inst: u64,
}

impl ChatLine {
    /// Whether the box draws it (`OnDrawSelf`'s test, its freshness aside).
    fn drawn(&self) -> bool {
        !self.text.is_empty() && self.text[0] != 0
    }
}

/// A row of the box as words: the game's `|` codes out (a colour's fade, a
/// button's picture; `Window::draw_text_internal`'s), `||` the bar it draws, a
/// carriage return or tab the space it comes to, any other control left out.
pub fn plain(s: &[u16]) -> String {
    let (bar, mut out, mut i) = (b'|' as u16, Vec::with_capacity(s.len()), 0);
    while i < s.len() && s[i] != 0 {
        if s[i] == bar {
            match s.get(i + 1) {
                None | Some(0) => break,
                Some(&n) if n == bar => out.push(bar),
                Some(_) => {}
            }
            i += 1;
        } else if s[i] == b'\r' as u16 || s[i] == b'\t' as u16 {
            out.push(b' ' as u16);
        } else if s[i] >= 32 && s[i] != 127 {
            out.push(s[i]);
        }
        i += 1;
    }
    String::from_utf16_lossy(&out).trim_end().to_string()
}

/// `VIChatText`.
#[derive(Clone, Debug)]
pub struct ChatText {
    width: i32,
    lines: Vec<ChatLine>,
    head: usize,
    tail: usize,
    /// The number the next line finished takes (`ChatLine::seq`).
    seq: u64,
    /// While a line with links is added (`add_text_linked`): where in the
    /// line each character of the tail's row came from (`usize::MAX` for a
    /// continuation's space), and the links (start, end, id, number) in the
    /// line's units; the number the next link takes.
    src: Vec<usize>,
    spans: Vec<(usize, usize, u32, u64)>,
    link_inst: u64,
}

impl ChatText {
    /// `Init(app, width)`: lines wrap at `width - 30`.
    pub fn new(width: i32) -> Self {
        let mut t = ChatText { width: width - 30, lines: Vec::new(), head: 0, tail: 0, seq: 0, src: Vec::new(), spans: Vec::new(), link_inst: 0 };
        t.reset();
        t
    }

    pub fn reset(&mut self) {
        self.lines = (0..LINES).map(|_| ChatLine { text: Vec::new(), color: [1.0; 4], kind: 0, time: 0, seq: 0, links: Vec::new() }).collect();
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
    /// space. `width` measures a run of text in the font. (The game's lines
    /// all come through `add_text_linked`, links or none.)
    #[cfg(test)]
    pub fn add_text(&mut self, s: &[u16], c: [f32; 4], now: u32, width: impl Fn(&[u16]) -> i32) {
        self.add_text_linked(s, c, now, width, &[]);
    }

    /// `add_text` for a line with item links (the client's, `itemlink`): each
    /// link's place in `s` (UTF-16 units) and the server's id. The rows are
    /// `AddText`'s, character for character; each row keeps the part of each
    /// link it holds (`ChatLine::links`).
    pub fn add_text_linked(&mut self, s: &[u16], c: [f32; 4], now: u32, width: impl Fn(&[u16]) -> i32, links: &[net::itemlink::ChatLink]) {
        self.src.clear();
        self.spans = links
            .iter()
            .map(|l| {
                self.link_inst += 1;
                (l.start as usize, l.start as usize + l.len as usize, l.id, self.link_inst)
            })
            .collect();
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
                    self.src = vec![usize::MAX];
                }
                if at(s, pos) != 0 {
                    let t = &mut self.lines[self.tail].text;
                    t.truncate(n);
                    t.push(at(s, pos));
                    self.src.truncate(n);
                    self.src.push(pos);
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
                self.src.truncate(space);
                pos -= n - space - 1;
            }
            self.add_line(c, now);
            space = 0;
            n = 1;
            self.lines[self.tail].text = vec![b' ' as u16];
            self.src = vec![usize::MAX];
            w = 0;
        }
        self.add_line(c, now);
        self.spans.clear();
    }

    /// `AddLine`: the line at the tail is finished; the ring moves on.
    fn add_line(&mut self, c: [f32; 4], now: u32) {
        let links = self.row_links();
        let l = &mut self.lines[self.tail];
        l.color = c;
        l.kind = 1;
        l.time = now;
        l.seq = self.seq;
        l.links = links;
        self.seq += 1;
        if self.head == 0 && self.tail < LINES - 1 {
            self.tail += 1;
        } else {
            self.head = Self::next(self.head);
            self.tail = Self::next(self.tail);
        }
        // The new tail line starts empty (it is overwritten as it fills).
        self.lines[self.tail].text.clear();
        self.lines[self.tail].links.clear();
        self.src.clear();
    }

    /// The links' parts on the tail's row as it is finished: for each link, the
    /// row's characters that came from it (the row's characters come from the
    /// line in order, so they are one run).
    fn row_links(&self) -> Vec<RowLink> {
        let mut out = Vec::new();
        for &(a, b, id, inst) in &self.spans {
            let mut run = self.src.iter().enumerate().filter(|&(_, &p)| p != usize::MAX && (a..b).contains(&p)).map(|(i, _)| i);
            if let Some(first) = run.next() {
                let last = run.last().unwrap_or(first);
                out.push(RowLink { start: first as u16, len: (last + 1 - first) as u16, id, inst });
            }
        }
        out
    }

    /// The lines numbered `from..=to` still held, a row a line, oldest first.
    fn rows(&self, from: u64, to: u64) -> String {
        let (mut out, mut i) = (Vec::new(), self.first());
        while i != self.end() {
            let l = &self.lines[i];
            if (from..=to).contains(&l.seq) && l.drawn() {
                out.push(plain(&l.text));
            }
            i = Self::next(i);
        }
        out.join("\n")
    }

    /// The newest line as the sender wrote it: all its rows, where it wrapped (a
    /// continuation row starts with a space and was added with the row before).
    fn newest(&self) -> Option<String> {
        let (stop, mut i, mut from) = (Self::prev(self.first()), Self::prev(self.end()), None);
        while i != stop && self.count() > 0 {
            let l = &self.lines[i];
            if l.drawn() {
                if from.is_some_and(|(_, time)| time != l.time) {
                    break;
                }
                from = Some((l.seq, l.time));
                if l.text[0] != b' ' as u16 {
                    break;
                }
            }
            i = Self::prev(i);
        }
        let (seq, _) = from?;
        Some(self.rows(seq, u64::MAX)).filter(|s| !s.trim().is_empty())
    }
}

/// What the Copy key takes when no edit box is up, as the box drawn last left it
/// (the page's keys come in from outside the game, where only this is in reach).
#[derive(Default)]
struct ForCopy {
    /// when a box was last drawn (`clipboard::screen_time`)
    drawn: Option<f64>,
    text: Option<String>,
    /// taken since the frame last looked: its sound
    copied: bool,
}

thread_local! {
    static FOR_COPY: RefCell<ForCopy> = RefCell::default();
}

/// A chat box not drawn for this long is gone (`editbox::FIELD_GONE`'s reason).
const COPY_GONE: f64 = 0.5;

/// What the Copy key takes with no edit box up: the lines selected, or the newest
/// line; none with no chat box on the screen.
pub fn copy_text() -> Option<String> {
    let now = crate::clipboard::screen_time();
    FOR_COPY.with_borrow(|c| if c.drawn.is_some_and(|d| now - d < COPY_GONE) { c.text.clone() } else { None })
}

/// `copy_text`'s text has gone to the clipboard: the step's sound, which is all
/// that says so.
pub fn copied() {
    FOR_COPY.with_borrow_mut(|c| c.copied = true);
}

/// A row as the last frame drew it: its line, and its top and bottom in the frame
/// (before the right side's move, as `contains` takes a point).
#[derive(Clone, Copy, Debug)]
struct Shown {
    seq: u64,
    top: i32,
    bottom: i32,
}

/// The mouse's press on a line, until it lets go.
#[derive(Clone, Copy, Debug)]
struct Drag {
    /// the line pressed on
    from: u64,
    /// where, in the window's pixels
    press: (f64, f64),
    /// a selection was up as it came down: a click only clears that
    had: bool,
    /// gone past the slop: a drag, the selection under way
    moved: bool,
    /// the pointer now, as `contains` takes a point
    at: [f32; 2],
    /// the lines owed to the scroll past the box's edge
    owed: f32,
    /// where it came down, as `contains` takes a point (a link there opens)
    p0: [f32; 2],
}

/// An item link as the last frame drew it (`itemlink`): its place (as
/// `contains` takes a point, x0 y0 x1 y1), the server's id and which link it is.
#[derive(Clone, Debug)]
struct LinkHit {
    rect: [i32; 4],
    id: u32,
    inst: u64,
}

/// The button let go (`WndChat::release`).
#[derive(Clone, Debug, PartialEq)]
pub enum Let {
    /// no press of the box's: the button is the game's
    NotHeld,
    /// a drag, or the click that cleared a selection
    Done,
    /// a click on a line with no selection up, as it is on the box: at this place
    Click(f64, f64),
    /// a click on an item link (`itemlink`): its id and its name as drawn
    Link(u32, String),
}

/// The highlight, over the rows: a wash the words read through.
const SELECTED: [f32; 4] = [0.30, 0.55, 1.0, 0.40];

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
    /// The rows the last frame drew, the bottom one first.
    shown: Vec<Shown>,
    /// The lines selected: the one it began on and the one it has come to.
    sel: Option<(u64, u64)>,
    drag: Option<Drag>,
    /// What `FOR_COPY` was last told of (the lines' count, the selection).
    told: Option<(u64, Option<(u64, u64)>)>,
    /// The wheel's part of a line not yet stepped.
    wheel: f32,
    /// The history's view is on the newest lines, nothing having taken it back
    /// (`follow`).
    following: bool,
    /// The item links the last frame drew (`itemlink`), whether they are drawn as
    /// links (the Extended row Item Links: off, the line's plain words), and the
    /// one the history's walk has lit (its number).
    link_hits: Vec<LinkHit>,
    pub show_links: bool,
    walk: Option<u64>,
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
        WndChat { mode: 0, height: 0, target: 0, lines: 0, text: ChatText::new(462 - 45), window: Window::default(), scroll: 0, top: 0, bottom: 0, held: 0, repeat: 0.0, cues: Vec::new(), shown: Vec::new(), sel: None, drag: None, told: None, wheel: 0.0, following: false, link_hits: Vec::new(), show_links: true, walk: None }
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

    /// `SetScroll`: 1 opens the view on the last five lines, 0 ends it. (A
    /// selection, the client's, keeps the view it was made in until it is cleared.)
    pub fn set_scroll(&mut self, on: i32) {
        // the walk through the links is the hold's: opened or let go, none lit
        self.walk = None;
        if self.sel.is_some() && self.scroll == 1 {
            return;
        }
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
            self.following = self.bottom == self.text.end();
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
            self.following = false;
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
        self.follow();
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
        self.shown.clear();
        self.link_hits.clear();
        let mut row = (self.lines - 1).max(0);
        let mut i = ChatText::prev(it);
        let stop = ChatText::prev(self.text.first());
        while i != stop && row >= 0 {
            if let Some(l) = self.text.line(i)
                && !l.text.is_empty()
                && l.text[0] != 0
                && (self.scroll == 1 || (self.scroll == 0 && l.kind == 1) || self.mode != 0)
            {
                let (text, color, seq) = (l.text.clone(), l.color, l.seq);
                let links = if self.show_links { l.links.clone() } else { Vec::new() };
                let (mid, step) = (self.window.row_center(vui, ui, row)[1], h - 4);
                if links.is_empty() {
                    self.window.text_box(vui, ui, &text, 25, row, self.window.size[0] - 45, 1, color, 0);
                } else {
                    self.draw_linked_row(vui, ui, &text, &links, row, color, [mid - step / 2, mid - step / 2 + step]);
                }
                self.shown.push(Shown { seq, top: mid - step / 2, bottom: mid - step / 2 + step });
                row -= 1;
            }
            i = ChatText::prev(i);
        }
        self.drag_on(dt);
        self.tell_copy();
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

    /// A selection made on the newest lines goes on with them once the drag is
    /// done, so a player who copied and went back to playing misses nothing said;
    /// one scrolled back stays where it is.
    fn follow(&mut self) {
        if self.sel.is_some() && self.drag.is_none() && self.scroll == 1 && self.following {
            let cues = self.cues.len();
            while self.bottom != self.text.end() {
                self.scroll_down();
            }
            self.cues.truncate(cues);
        }
    }

    /// The row drawn at point `p` of the box (as `contains` takes one).
    fn row_at(&self, p: [f32; 2]) -> Option<u64> {
        if !self.contains(p) {
            return None;
        }
        self.shown.iter().find(|r| p[1] >= r.top as f32 && p[1] < r.bottom as f32).map(|r| r.seq)
    }

    /// The left button down at point `p` (window pixels `at`): the box's when it
    /// is on a line.
    pub fn press(&mut self, p: [f32; 2], at: (f64, f64)) -> bool {
        let Some(from) = self.row_at(p) else { return false };
        self.drag = Some(Drag { from, press: at, had: self.sel.is_some(), moved: false, at: p, owed: 0.0, p0: p });
        true
    }

    /// A press of the box's is down.
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// The pointer moved: past the slop a press selects, its line first, and the
    /// box holds still on the lines there are from then on (a fresh line's fade
    /// would take the row away from under the pointer).
    pub fn moved(&mut self, p: [f32; 2], at: (f64, f64)) {
        let Some(d) = self.drag.as_mut() else { return };
        d.at = p;
        if !d.moved && ((at.0 - d.press.0).powi(2) + (at.1 - d.press.1).powi(2)).sqrt() > crate::mouse::SLOP {
            d.moved = true;
            let from = d.from;
            let scrolled = self.scroll == 1;
            self.sel = Some((from, from));
            if !scrolled {
                self.set_scroll(1);
            }
            self.following = self.bottom == self.text.end();
        }
    }

    /// Each frame, a drag: the selection comes to the row under the pointer; past
    /// the rows it steps on through the history, the faster the farther out
    /// (silently, where a key's steps tick).
    fn drag_on(&mut self, dt: f32) {
        let Some(mut d) = self.drag.filter(|d| d.moved) else { return };
        let (Some(&bottom), Some(&top)) = (self.shown.first(), self.shown.last()) else { return };
        let y = d.at[1];
        let (above, below) = (top.top as f32 - y, y - bottom.bottom as f32);
        let head = if above > 0.0 || below > 0.0 {
            d.owed += (8.0 + above.max(below)).min(200.0) * dt;
            let cues = self.cues.len();
            while d.owed >= 1.0 {
                d.owed -= 1.0;
                if above > 0.0 {
                    self.scroll_up();
                } else {
                    self.scroll_down();
                }
            }
            self.cues.truncate(cues);
            if above > 0.0 { top.seq } else { bottom.seq }
        } else {
            d.owed = 0.0;
            self.shown.iter().find(|r| y >= r.top as f32 && y < r.bottom as f32).map_or(d.from, |r| r.seq)
        };
        self.drag = Some(d);
        if let Some(s) = self.sel.as_mut() {
            s.1 = head;
        }
    }

    /// The left button let go.
    pub fn release(&mut self, scrolling: bool) -> Let {
        let Some(d) = self.drag.take() else { return Let::NotHeld };
        if d.moved {
            return Let::Done;
        }
        if d.had {
            self.clear_selection(scrolling);
            return Let::Done;
        }
        if let Some((id, name)) = self.link_at(d.p0) {
            return Let::Link(id, name);
        }
        Let::Click(d.press.0, d.press.1)
    }

    /// The press dropped where its release may never come (the pointer gone from
    /// the window, the keys gone to another): what it selected stays.
    pub fn let_go(&mut self) {
        self.drag = None;
    }

    /// Whether a selection is up or being made.
    pub fn selecting(&self) -> bool {
        self.sel.is_some() || self.drag.is_some()
    }

    /// The selection gone, and the view it held with it (`scrolling`: R2's holds
    /// on).
    pub fn clear_selection(&mut self, scrolling: bool) {
        self.drag = None;
        if self.sel.take().is_some() && !scrolling {
            self.set_scroll(0);
        }
    }

    /// The wheel's `lines` (up positive) with the history in view and the pointer
    /// on the box, or a drag on, or the history held (`held`: R2 or C, mode 33,
    /// wherever the pointer is: the petition 1463's "mouse scrolling up and down
    /// on the chat log when holding the chat expand button"): a line a notch
    /// through the history. False when it is not the box's.
    pub fn wheel(&mut self, p: [f32; 2], lines: f32, held: bool) -> bool {
        if self.scroll != 1 || !(held || self.contains(p) || self.drag.is_some()) {
            return false;
        }
        self.wheel += lines;
        let cues = self.cues.len();
        while self.wheel.abs() >= 1.0 {
            if self.wheel > 0.0 {
                self.scroll_up();
            } else {
                self.scroll_down();
            }
            self.wheel -= self.wheel.signum();
        }
        self.cues.truncate(cues);
        true
    }

    /// What the Copy key would take: the lines selected, in order, or the newest.
    fn copy_now(&self) -> Option<String> {
        match self.sel {
            Some((a, b)) => Some(self.text.rows(a.min(b), a.max(b))).filter(|s| !s.is_empty()),
            None => self.text.newest(),
        }
    }

    /// `FOR_COPY` kept up, the text made again only when it has changed; and its
    /// sound, when it has been taken.
    fn tell_copy(&mut self) {
        let now = (self.text.seq, self.sel);
        let text = (self.told != Some(now)).then(|| self.copy_now());
        self.told = Some(now);
        let copied = FOR_COPY.with_borrow_mut(|c| {
            c.drawn = Some(crate::clipboard::screen_time());
            if let Some(text) = text {
                c.text = text;
            }
            std::mem::take(&mut c.copied)
        });
        if copied {
            self.cues.push(11);
        }
    }

    /// The rows selected as the last frame drew them, top and bottom.
    fn selected(&self) -> Vec<(i32, i32)> {
        let Some((a, b)) = self.sel else { return Vec::new() };
        self.shown.iter().filter(|r| (a.min(b)..=a.max(b)).contains(&r.seq)).map(|r| (r.top, r.bottom)).collect()
    }

    /// The highlight, over the game's own drawing (the client's), where the rows
    /// were laid: the text's column across.
    pub fn draw_selection(&self, ui: &mut dyn crate::ui::Layout) {
        let x0 = self.window.top_left()[0] + 22;
        let x1 = self.window.top_left()[0] + self.window.size[0] - 20;
        for (top, bottom) in self.selected() {
            ui.fill_blended(SELECTED, [[x0, top], [x1, top], [x1, bottom], [x0, bottom]]);
        }
        // and the link the history's walk has lit (`itemlink`)
        for h in self.link_hits.iter().filter(|h| Some(h.inst) == self.walk) {
            let [a, b, c, d] = h.rect;
            ui.fill_blended(crate::itemlink::LIT, [[a - 2, b], [c + 2, b], [c + 2, d], [a - 2, d]]);
        }
    }

    /// A row with item links (`itemlink`): its runs drawn one after another as
    /// `DrawTextInternal` draws its runs, each link's in the links' purple, and
    /// each link's place noted (`link_hits`). `y` is the row's top and bottom.
    fn draw_linked_row(&mut self, vui: &Vui, ui: &mut dyn Raster, text: &[u16], links: &[RowLink], row: i32, color: [f32; 4], y: [i32; 2]) {
        let x0 = self.window.top_left()[0];
        let (mut x, mut at) = (25, 0usize);
        for l in links {
            let a = (l.start as usize).min(text.len());
            let b = (a + l.len as usize).min(text.len());
            if a < at || a >= b {
                continue;
            }
            if a > at {
                let run = &text[at..a];
                self.window.text(vui, ui, run, x, row, color, 0);
                x += self.window.text_width(vui, ui, run);
            }
            let run = &text[a..b];
            self.window.text(vui, ui, run, x, row, crate::itemlink::COLOR, 0);
            let w = self.window.text_width(vui, ui, run);
            self.link_hits.push(LinkHit { rect: [x0 + x, y[0], x0 + x + w, y[1]], id: l.id, inst: l.inst });
            x += w;
            at = b;
        }
        if at < text.len() {
            self.window.text(vui, ui, &text[at..], x, row, color, 0);
        }
    }

    /// The middle of the `k`th newest item link the last frame drew (0 the
    /// newest; as `contains` takes a point): what a headless run's `linkclick`
    /// clicks.
    pub fn newest_link_at(&self, k: usize) -> Option<[f32; 2]> {
        let mut insts: Vec<u64> = self.link_hits.iter().map(|h| h.inst).collect();
        insts.sort_unstable_by(|a, b| b.cmp(a));
        insts.dedup();
        let inst = *insts.get(k)?;
        let h = self.link_hits.iter().find(|h| h.inst == inst)?;
        Some([(h.rect[0] + h.rect[2]) as f32 / 2.0, (h.rect[1] + h.rect[3]) as f32 / 2.0])
    }

    /// The item link drawn at point `p` (as `contains` takes one): its id and its
    /// name, its parts on other rows joined.
    pub fn link_at(&self, p: [f32; 2]) -> Option<(u32, String)> {
        let h = self.link_hits.iter().find(|h| p[0] >= h.rect[0] as f32 && p[0] < h.rect[2] as f32 && p[1] >= h.rect[1] as f32 && p[1] < h.rect[3] as f32)?;
        Some((h.id, self.link_name(h.inst)))
    }

    /// Link `inst`'s name: its parts on every row that holds it, in order (a
    /// continuation row's own space left out).
    fn link_name(&self, inst: u64) -> String {
        let mut out = Vec::new();
        let mut i = self.text.first();
        while i != self.text.end() {
            if let Some(l) = self.text.line(i) {
                for k in l.links.iter().filter(|k| k.inst == inst) {
                    let a = (k.start as usize).min(l.text.len());
                    let b = (a + k.len as usize).min(l.text.len());
                    out.extend_from_slice(&l.text[a..b]);
                }
            }
            i = ChatText::next(i);
        }
        plain(&out)
    }

    /// The links in the history, oldest first: each one's number, id, and the
    /// line it starts on.
    fn walk_list(&self) -> Vec<(u64, u32, u64)> {
        let mut out: Vec<(u64, u32, u64)> = Vec::new();
        let mut i = self.text.first();
        while i != self.text.end() {
            if let Some(l) = self.text.line(i).filter(|l| l.drawn()) {
                for k in &l.links {
                    if !out.iter().any(|o| o.0 == k.inst) {
                        out.push((k.inst, k.id, l.seq));
                    }
                }
            }
            i = ChatText::next(i);
        }
        out
    }

    /// With the history held (mode 33) and links shown: left, the next link up
    /// the history lit (from none, the newest), the view moved to hold it. False
    /// (sound 9) at the oldest, or with none.
    pub fn walk_older(&mut self) -> bool {
        let list = self.walk_list();
        if self.scroll != 1 || !self.show_links || list.is_empty() {
            self.cues.push(9);
            return false;
        }
        let to = match list.iter().position(|o| Some(o.0) == self.walk) {
            None => list.len() - 1,
            Some(0) => {
                self.cues.push(9);
                return false;
            }
            Some(k) => k - 1,
        };
        self.walk_to(list[to].0, list[to].2);
        true
    }

    /// Right: the next link down lit; past the newest none, and the view back on
    /// the newest lines. False with none lit.
    pub fn walk_newer(&mut self) -> bool {
        let list = self.walk_list();
        let Some(k) = list.iter().position(|o| Some(o.0) == self.walk) else {
            self.walk = None;
            return false;
        };
        if k + 1 < list.len() {
            self.walk_to(list[k + 1].0, list[k + 1].2);
        } else {
            self.walk = None;
            let cues = self.cues.len();
            while self.bottom != self.text.end() {
                self.scroll_down();
            }
            self.cues.truncate(cues);
            self.cues.push(11);
        }
        true
    }

    /// Link `inst` lit, the view moved to hold line `seq`, sound 11.
    fn walk_to(&mut self, inst: u64, seq: u64) {
        self.walk = Some(inst);
        let cues = self.cues.len();
        while self.top != self.text.first() && self.text.line(self.top).is_some_and(|l| l.seq > seq) {
            self.scroll_up();
        }
        while self.bottom != self.text.end() && self.text.line(ChatText::prev(self.bottom)).is_some_and(|l| l.seq < seq) {
            self.scroll_down();
        }
        self.cues.truncate(cues);
        self.cues.push(11);
    }

    /// A link is lit by the history's walk.
    pub fn walking(&self) -> bool {
        self.walk.is_some()
    }

    /// The link the walk has lit: its id and its name.
    pub fn walked(&self) -> Option<(u32, String)> {
        let inst = self.walk?;
        let id = self.walk_list().into_iter().find(|o| o.0 == inst)?.1;
        Some((id, self.link_name(inst)))
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

    /// What lands on the clipboard is the words: the game's codes out.
    #[test]
    fn a_row_as_words() {
        let s = |t: &str| plain(&crate::ui::wide(t));
        assert_eq!(s("Bob says, 'hail'"), "Bob says, 'hail'");
        assert_eq!(s("|-faded|+ back |x button a||b"), "faded back  button a|b");
        assert_eq!(s("ends on a bar|"), "ends on a bar");
        assert_eq!(s("tab\there\rand\u{1}there   "), "tab here andthere");
        assert_eq!(s(" a continuation keeps its place"), " a continuation keeps its place");
        assert_eq!(s("\u{e9} \u{4e2d}"), "\u{e9} \u{4e2d}", "words of other tongues are not codes");
    }

    use crate::pointer::testing::vui;

    /// The game's font's height (21: five rows are the 85 the scroll view snaps
    /// to), no picture.
    struct Rows;

    impl Raster for Rows {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            21
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 640, 448]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    fn width(s: &[u16]) -> i32 {
        s.len() as i32 * 8
    }

    /// A box holding lines `line 0`.., as a frame leaves it, ready for a pointer.
    fn with_lines(n: usize) -> WndChat {
        let mut c = WndChat::new();
        for k in 0..n {
            c.text.add_text(&crate::ui::wide(&format!("line {k}")), [1.0; 4], 0, width);
        }
        // the window opened and eased to its height
        let (v, mut r) = (vui(), Rows);
        for _ in 0..30 {
            c.draw(&v, &mut r, 0, 0.1);
        }
        c
    }

    /// A frame `dt` seconds on.
    fn frame(c: &mut WndChat, dt: f32) {
        c.draw(&vui(), &mut Rows, 0, dt);
    }

    /// A press on line `seq`, past the slop and a frame on: the view the drag goes
    /// on in.
    fn begin(c: &mut WndChat, seq: u64) {
        assert!(c.press(on(c, seq), (0.0, 0.0)));
        c.moved(on(c, seq), (0.0, 40.0));
        frame(c, 0.016);
        assert_eq!(c.sel, Some((seq, seq)));
    }

    /// The drag to line `seq`, and a frame.
    fn to(c: &mut WndChat, seq: u64) {
        c.moved(on(c, seq), (0.0, 40.0));
        frame(c, 0.016);
    }

    fn mid(r: &Shown) -> [f32; 2] {
        [300.0, (r.top + r.bottom) as f32 / 2.0]
    }

    /// The point on the row of line `seq`, as the last frame drew it.
    fn on(c: &WndChat, seq: u64) -> [f32; 2] {
        mid(c.shown.iter().find(|r| r.seq == seq).expect("drawn"))
    }

    /// A drag from a line to another picks them whole and in order, whichever way
    /// it went, and only the rows between; the words are as shown, one a row.
    #[test]
    fn a_drag_picks_whole_lines() {
        let mut c = with_lines(8);
        assert_eq!(c.shown.len(), 5, "the box's five");
        assert!(c.press(on(&c, 6), (0.0, 0.0)));
        assert!(c.sel.is_none(), "a press alone selects nothing");
        c.release(false);
        begin(&mut c, 6);
        to(&mut c, 4);
        assert_eq!(c.sel, Some((6, 4)));
        assert_eq!(c.release(false), Let::Done);
        assert_eq!(c.copy_now().as_deref(), Some("line 4\nline 5\nline 6"), "oldest first");
        assert_eq!(c.selected().len(), 3, "the rows lit");
        // the other way
        let mut c = with_lines(8);
        begin(&mut c, 4);
        to(&mut c, 5);
        c.release(false);
        assert_eq!(c.copy_now().as_deref(), Some("line 4\nline 5"));
    }

    /// A click is not a drag: on a line it is what it was (the caller's), and the
    /// controls: within the slop, and off the rows, where the box takes no press.
    #[test]
    fn a_click_stays_a_click() {
        let mut c = with_lines(8);
        assert!(c.press(on(&c, 7), (100.0, 100.0)));
        c.moved(on(&c, 7), (103.0, 103.0));
        assert_eq!(c.release(false), Let::Click(100.0, 100.0));
        assert!(c.sel.is_none());
        assert!(!c.press([300.0, -50.0], (0.0, 0.0)), "off the box");
        assert!(!c.press([-300.0, on(&c, 7)[1]], (0.0, 0.0)), "beside the rows");
        assert_eq!(c.release(false), Let::NotHeld, "the button is the game's");
        // a box with nothing in it has no rows to press
        let c2 = with_lines(0);
        assert!(c2.shown.is_empty());
    }

    /// Past the top of the rows the drag goes on through the kept history, to the
    /// oldest line, and copies all of it in order; the step makes no sound. Then
    /// back past the bottom, to the newest.
    #[test]
    fn a_drag_reaches_the_history() {
        let mut c = with_lines(1000);
        let bottom = *c.shown.first().unwrap();
        c.press(mid(&bottom), (0.0, 0.0));
        c.moved([300.0, bottom.top as f32 - 500.0], (0.0, 500.0));
        c.cues.clear();
        let mut frames = 0;
        while c.top != c.text.first() {
            frame(&mut c, 0.05);
            frames += 1;
            assert!(frames < 200, "the drag never reached the first line");
        }
        frame(&mut c, 0.05);
        assert!(c.cues.is_empty(), "no ticks: {:?}", c.cues);
        assert_eq!(c.sel, Some((999, 0)));
        c.release(false);
        let all = c.copy_now().unwrap();
        let lines: Vec<&str> = all.lines().collect();
        assert_eq!((lines.len(), lines[0], lines[999]), (1000, "line 0", "line 999"));
        // it holds where it was with the box on the first five
        assert!(c.shown.iter().any(|r| r.seq == 0));
    }

    /// New lines go on under a selection. Left on the newest lines the box goes on
    /// with them, the selection's rows going up it and off, its text as it was; even
    /// where the ring's places have been used again; gone off the end, there is
    /// nothing to copy. Scrolled back, the view stays; and while a drag is on.
    #[test]
    fn a_selection_holds_while_lines_come() {
        let more = |c: &mut WndChat, n: usize| {
            for k in 0..n {
                c.text.add_text(&crate::ui::wide(&format!("more {k}")), [1.0; 4], 0, width);
            }
            frame(c, 0.016);
        };
        let mut c = with_lines(8);
        begin(&mut c, 7);
        to(&mut c, 6);
        // still dragging: the rows stay under the pointer
        more(&mut c, 1);
        assert_eq!(c.shown.first().unwrap().seq, 7, "the view holds through a drag");
        c.release(false);
        frame(&mut c, 0.016);
        assert_eq!(c.shown.first().unwrap().seq, 8, "the drag done, it catches up");
        assert_eq!(c.selected().len(), 2, "lit");
        more(&mut c, 1);
        assert_eq!(c.shown.first().unwrap().seq, 9, "goes on with them");
        assert_eq!(c.selected().len(), 2, "its rows gone up the box");
        more(&mut c, 9);
        assert_eq!(c.shown.first().unwrap().seq, 18);
        assert!(c.selected().is_empty(), "and off it");
        assert_eq!(c.copy_now().as_deref(), Some("line 6\nline 7"), "its text as it was");
        more(&mut c, 1100);
        assert_eq!(c.copy_now(), None, "gone with the ring");
        // scrolled back, the view stays where it is
        let mut c = with_lines(20);
        c.set_scroll(1);
        frame(&mut c, 0.016);
        assert!(c.wheel(on(&c, 19), 3.0, false));
        frame(&mut c, 0.016);
        begin(&mut c, 14);
        c.release(false);
        more(&mut c, 5);
        assert_eq!(c.shown.first().unwrap().seq, 16, "the view held");
        assert_eq!(c.selected().len(), 1, "lit");
        // and back at the newest by a key, it goes on with them again
        for _ in 0..8 {
            c.key_down(20);
            c.key_up(20);
        }
        frame(&mut c, 0.016);
        assert_eq!(c.shown.first().unwrap().seq, 24);
        more(&mut c, 1);
        assert_eq!(c.shown.first().unwrap().seq, 25);
    }

    /// Esc (the caller's) or a click elsewhere clears it, and the box goes back to
    /// the fresh lines; R2 still held keeps its view. A click on the box with one
    /// up only clears it.
    #[test]
    fn a_selection_clears() {
        let mut c = with_lines(8);
        begin(&mut c, 7);
        to(&mut c, 6);
        c.release(false);
        assert_eq!(c.scroll, 1, "the view held for it");
        c.set_scroll(0);
        assert_eq!(c.scroll, 1, "R2 let go does not take the view from under it");
        assert!(c.press(on(&c, 7), (5.0, 5.0)));
        assert_eq!(c.release(false), Let::Done, "the click only clears it");
        assert!(!c.selecting());
        assert_eq!(c.scroll, 0);
        // R2 held
        let mut c = with_lines(8);
        c.set_scroll(1);
        frame(&mut c, 0.016);
        begin(&mut c, 7);
        to(&mut c, 6);
        c.release(true);
        c.clear_selection(true);
        assert_eq!(c.scroll, 1, "R2's view");
    }

    /// With nothing selected the Copy key takes the newest line, all its rows
    /// where it wrapped, and no line before it; nothing with no lines at all, or
    /// with no box drawn lately.
    #[test]
    fn the_copy_key_with_nothing_selected() {
        let mut c = with_lines(0);
        assert_eq!(c.copy_now(), None);
        assert_eq!(copy_text(), None);
        c.text.add_text(&crate::ui::wide("the line before"), [1.0; 4], 5, width);
        let long = "Bob says, ".to_string() + &"words and more ".repeat(10);
        c.text.add_text(&crate::ui::wide(&long), [1.0; 4], 6, width);
        assert!(c.text.count() > 2, "it wrapped");
        let got = c.copy_now().unwrap();
        assert!(got.starts_with("Bob says, "));
        assert!(!got.contains("before"));
        assert_eq!(got.replace("\n ", " ").replace("  ", " "), long.trim_end());
        // told once drawn
        frame(&mut c, 0.016);
        assert_eq!(copy_text().as_deref(), Some(got.as_str()));
        c.text.add_text(&crate::ui::wide("newest"), [1.0; 4], 7, width);
        frame(&mut c, 0.016);
        assert_eq!(copy_text().as_deref(), Some("newest"));
        // its sound once taken, and then none
        c.cues.clear();
        copied();
        frame(&mut c, 0.016);
        assert_eq!(c.cues, vec![11]);
        // the box gone: nothing to take
        FOR_COPY.with_borrow_mut(|f| f.drawn = Some(crate::clipboard::screen_time() - COPY_GONE * 2.0));
        assert_eq!(copy_text(), None);
    }

    /// The wheel steps the history under a pointer on the box while it is in view;
    /// otherwise it is the game's (the camera's zoom).
    #[test]
    fn the_wheel_steps_the_history_in_view() {
        let mut c = with_lines(20);
        assert!(!c.wheel(on(&c, 19), 1.0, false), "no history in view: the game's");
        c.set_scroll(1);
        frame(&mut c, 0.016);
        let p = on(&c, 19);
        assert!(!c.wheel([-300.0, p[1]], 1.0, false), "off the box: the game's");
        assert!(c.wheel(p, 2.5, false));
        frame(&mut c, 0.016);
        assert_eq!(c.shown.first().unwrap().seq, 17, "two lines back, the half kept");
        assert!(c.wheel(p, -0.5, false));
        frame(&mut c, 0.016);
        assert_eq!(c.shown.first().unwrap().seq, 17);
        assert!(c.wheel(p, -1.0, false));
        frame(&mut c, 0.016);
        assert_eq!(c.shown.first().unwrap().seq, 18);
    }

    /// What a row's runs were drawn as: the words and the colour, in order.
    struct Runs(Vec<(String, [f32; 4])>);

    impl Raster for Runs {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], c: [f32; 4], s: &[u16]) {
            self.0.push((String::from_utf16_lossy(s), c));
        }
        fn draw_centered_string(&mut self, _: [i32; 2], c: [f32; 4], s: &[u16], _: u32) {
            self.0.push((String::from_utf16_lossy(s), c));
        }
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            21
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 640, 448]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    fn link(start: usize, len: usize, id: u32) -> net::itemlink::ChatLink {
        net::itemlink::ChatLink { start: start as u16, len: len as u16, id }
    }

    /// A line's links keep their place through `AddText`'s wrap: each row holds
    /// its part of a link that wrapped, under one number, and the rows' words are
    /// the plain line's character for character. The control: the same line
    /// added plain has the same rows and no links.
    #[test]
    fn links_keep_their_place_through_the_wrap() {
        let line = "Bob tells the group: see Heartsblood Seralite Scale Leggings and the Rusty Sword now";
        let at = |w: &str| line.find(w).unwrap();
        let links = [link(at("Heartsblood"), "Heartsblood Seralite Scale Leggings".len(), 7), link(at("Rusty"), "Rusty Sword".len(), 9)];
        let (mut a, mut b) = (ChatText::new(462 - 45), ChatText::new(462 - 45));
        a.add_text_linked(&crate::ui::wide(line), [1.0; 4], 0, width, &links);
        b.add_text(&crate::ui::wide(line), [1.0; 4], 0, width);
        assert!(a.count() >= 2, "it wrapped");
        let rows = |t: &ChatText| (0..t.count()).map(|k| t.lines[k].text.clone()).collect::<Vec<_>>();
        assert_eq!(rows(&a), rows(&b), "the rows are the plain line's");
        assert!((0..b.count()).all(|k| b.lines[k].links.is_empty()), "the control: no links");
        let mut parts: Vec<(u32, u64, String)> = Vec::new();
        for k in 0..a.count() {
            let l = &a.lines[k];
            for r in &l.links {
                parts.push((r.id, r.inst, String::from_utf16_lossy(&l.text[r.start as usize..(r.start + r.len) as usize])));
            }
        }
        let leggings: Vec<&(u32, u64, String)> = parts.iter().filter(|p| p.0 == 7).collect();
        assert!(leggings.len() == 2 && leggings[0].1 == leggings[1].1, "one link on two rows: {parts:?}");
        assert_eq!(leggings.iter().map(|p| p.2.trim()).collect::<Vec<_>>().join(" "), "Heartsblood Seralite Scale Leggings");
        assert!(parts.iter().any(|p| p.0 == 9 && p.2 == "Rusty Sword"), "{parts:?}");
    }

    /// A box with a line holding a link, drawn: the link's run in the links'
    /// purple between the line's own; a click on it is the link, beside it the
    /// line. The control: with Item Links off the row is one run, and a click on
    /// the same place is the line's.
    #[test]
    fn a_link_is_drawn_purple_and_a_click_opens_it() {
        let mut c = WndChat::new();
        let line = "Ann: see Rusty Sword ok";
        c.text.add_text_linked(&crate::ui::wide(line), [1.0; 4], 0, width, &[link(9, 11, 42)]);
        let (v, mut r) = (vui(), Runs(Vec::new()));
        for _ in 0..30 {
            r.0.clear();
            c.draw(&v, &mut r, 0, 0.1);
        }
        assert_eq!(r.0, vec![("Ann: see ".to_string(), [1.0; 4]), ("Rusty Sword".to_string(), crate::itemlink::COLOR), (" ok".to_string(), [1.0; 4])]);
        let h = c.link_hits[0].clone();
        let on = [(h.rect[0] + h.rect[2]) as f32 / 2.0, (h.rect[1] + h.rect[3]) as f32 / 2.0];
        assert!(c.press(on, (1.0, 1.0)));
        assert_eq!(c.release(false), Let::Link(42, "Rusty Sword".into()));
        let beside = [h.rect[0] as f32 - 20.0, on[1]];
        assert!(c.press(beside, (2.0, 2.0)));
        assert_eq!(c.release(false), Let::Click(2.0, 2.0), "the line beside it");
        c.show_links = false;
        r.0.clear();
        c.draw(&v, &mut r, 0, 0.1);
        assert_eq!(r.0, vec![(line.to_string(), [1.0; 4])], "off: the plain line");
        assert!(c.press(on, (3.0, 3.0)));
        assert_eq!(c.release(false), Let::Click(3.0, 3.0), "off: the line's click");
    }

    /// With the history held, left walks up through the links (the newest first,
    /// the view following one scrolled out of it) and right back down; past the
    /// newest none is lit and the view is on the newest lines again. Let go and
    /// opened again, none is lit: the walk is not kept. The controls: with no
    /// history held, or links off, nothing is lit.
    #[test]
    fn the_walk_goes_up_and_down_and_is_forgotten() {
        let mut c = WndChat::new();
        c.text.add_text_linked(&crate::ui::wide("Ann: Sword"), [1.0; 4], 0, width, &[link(5, 5, 1)]);
        for k in 0..8 {
            c.text.add_text(&crate::ui::wide(&format!("line {k}")), [1.0; 4], 0, width);
        }
        c.text.add_text_linked(&crate::ui::wide("Bo: Shield and Helm"), [1.0; 4], 0, width, &[link(4, 6, 2), link(15, 4, 3)]);
        let frame = |c: &mut WndChat| c.draw(&vui(), &mut Rows, 0, 0.016);
        frame(&mut c);
        assert!(!c.walk_older(), "no history held");
        c.set_scroll(1);
        frame(&mut c);
        c.show_links = false;
        assert!(!c.walk_older(), "links off");
        c.show_links = true;
        assert!(c.walk_older());
        assert_eq!(c.walked(), Some((3, "Helm".into())), "the newest first");
        assert!(c.walk_older());
        assert_eq!(c.walked(), Some((2, "Shield".into())));
        assert!(c.walk_older());
        assert_eq!(c.walked(), Some((1, "Sword".into())), "up the history");
        frame(&mut c);
        assert!(c.shown.iter().any(|r| r.seq == 0), "the view moved to hold it");
        assert!(c.link_hits.iter().any(|h| h.id == 1), "drawn where the view now is");
        c.cues.clear();
        assert!(!c.walk_older(), "the oldest");
        assert_eq!(c.cues, vec![9]);
        assert!(c.walk_newer() && c.walk_newer());
        assert_eq!(c.walked().map(|w| w.0), Some(3));
        assert!(c.walk_newer());
        assert!(!c.walking(), "past the newest: none");
        assert_eq!(c.bottom, c.text.end(), "the view on the newest lines");
        assert!(c.walk_older());
        c.set_scroll(0);
        assert!(!c.walking(), "let go: forgotten");
        c.set_scroll(1);
        assert!(!c.walking() && c.walked().is_none(), "opened again: none lit, the newest lines");
        assert_eq!(c.bottom, c.text.end());
    }

    /// The wheel steps the history wherever the pointer is while it is held
    /// (R2 or C); the control: not held, off the box it is the game's.
    #[test]
    fn the_wheel_steps_the_history_anywhere_while_held() {
        let mut c = with_lines(20);
        c.set_scroll(1);
        frame(&mut c, 0.016);
        let off = [-300.0, -300.0];
        assert!(!c.wheel(off, 1.0, false), "not held: the game's");
        assert!(c.wheel(off, 2.0, true), "held: anywhere");
        frame(&mut c, 0.016);
        assert_eq!(c.shown.first().unwrap().seq, 17, "two lines back");
    }
}
