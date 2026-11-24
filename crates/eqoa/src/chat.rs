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
//!
//! Chat Always Shown (Extended > UI > Chat, a player's ticket 1914: "allows your
//! chat box to stay fully visible without auto collapse") is the client's too
//! (`always`): the box stays open at its five rows as the fixed mode keeps it,
//! the last five lines in it whatever their age, with no backdrop strip (the
//! fixed mode's, a window's). Off, the game's box exactly.
//!
//! Chat Tabs are the client's too (a player's ticket 1951: "optional chat tabs
//! ... Up to 3. Each tab loads a new chat and can be set to filter all of a
//! certain type of chat to that tab. The tab should be nameable as well, up to
//! 8 characters"): each kind of line (`Kind`, told by `kind_of` from its words
//! and its colour, as the server sends them) goes to Main, the box's own, or to
//! one of three tabs (Extended > UI > Chat Tabs, `settings::ClientOptions::
//! chat_tabs`); a tab is there while a kind goes to it, each its own ring of
//! lines (`Tabs`), named by `/tabname` or after its first kind. With a tab the
//! box's open view shows the tabs' names over it, the one shown lit and one
//! with lines unread green; a click shows another (`click_tab`), as `/tab`
//! does. With every kind Main's, as by default, nothing is drawn or kept apart:
//! the game's box.

use std::cell::RefCell;
use std::collections::VecDeque;
use vi::ui::{Raster, Vui, Window};

/// The ring's size. The game's is 32 (`VIChatText::m_lines`, 5,136 bytes of
/// the PS2's memory), holding the last 31 lines; the client's holds the last
/// 1,000 (F3 2026-10-21: lines went off the top of the history too soon),
/// about 180 KB, scrolled as the game scrolls its 31.
const LINES: usize = 1001;

/// The game's short forms use /s for Shout and /sa for Say.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Say, Group, Shout, Guild, Tell, Reply,
}

impl Channel {
    pub fn command(word: &str) -> Option<Self> {
        match word.to_ascii_lowercase().as_str() {
            "/sa" | "/say" => Some(Self::Say),
            "/g" | "/group" => Some(Self::Group),
            "/s" | "/shout" => Some(Self::Shout),
            "/gu" | "/guild" => Some(Self::Guild),
            "/t" | "/tell" => Some(Self::Tell),
            "/r" | "/reply" => Some(Self::Reply),
            _ => None,
        }
    }

    pub fn mode(self) -> Option<u8> {
        match self {
            Self::Say => Some(0), Self::Group => Some(2), Self::Shout => Some(3), Self::Guild => Some(4),
            Self::Tell | Self::Reply => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Say => "Say", Self::Group => "Group", Self::Shout => "Shout",
            Self::Guild => "Guild", Self::Tell => "Tell", Self::Reply => "Reply",
        }
    }
}

/// Retain message boundaries and link offsets so a width change can rewrap
/// existing history, including tabs which are currently hidden.
#[derive(Clone, Debug)]
struct Message {
    text: Vec<u16>,
    color: [f32; 4],
    time: u32,
    links: Vec<net::itemlink::ChatLink>,
    end_seq: u64,
}

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
    messages: VecDeque<Message>,
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
        let mut t = ChatText { width: width - 30, messages: VecDeque::new(), lines: Vec::new(), head: 0, tail: 0, seq: 0, src: Vec::new(), spans: Vec::new(), link_inst: 0 };
        t.reset();
        t
    }

    pub fn reset(&mut self) {
        self.messages.clear();
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
        self.wrap(s, c, now, width, links);
        self.remember(Message { text: s.to_vec(), color: c, time: now, links: links.to_vec(), end_seq: self.seq });
    }

    fn remember(&mut self, message: Message) {
        let first = self.lines[self.head].seq;
        while self.messages.front().is_some_and(|m| m.end_seq <= first) {
            self.messages.pop_front();
        }
        self.messages.push_back(message);
    }

    fn rewrap(&mut self, width: i32, measure: &impl Fn(&[u16]) -> i32) {
        let messages = std::mem::take(&mut self.messages);
        self.reset();
        self.width = width - 30;
        for mut m in messages {
            self.wrap(&m.text, m.color, m.time, measure, &m.links);
            m.end_seq = self.seq;
            self.remember(m);
        }
    }

    fn wrap(&mut self, s: &[u16], c: [f32; 4], now: u32, width: impl Fn(&[u16]) -> i32, links: &[net::itemlink::ChatLink]) {
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

/// A shout's colour, (63, 255, 63): 0x0A7B's three bytes as the reference server sends
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

/// The chat tabs besides Main (ticket 1951).
pub const TABS: usize = 3;
/// A tab's name's most characters.
pub const TAB_NAME_MOST: usize = 8;
/// How many kinds of line there are (`Kind`).
pub const KINDS: usize = 10;

/// What a line in the chat box is, for the tabs (ticket 1951).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// someone's say, an NPC's words ("Name: text")
    Say,
    /// tells and whispers, to the player and from it
    Tells,
    /// the group's chat and the raid's
    Group,
    Guild,
    Shouts,
    /// what hits, misses, resists and slays
    Combat,
    Loot,
    /// /roll's and the loot rolls'
    Rolls,
    Experience,
    /// the rest: the server's notices, the client's own lines
    System,
}

impl Kind {
    pub const ALL: [Kind; KINDS] = [Kind::Say, Kind::Tells, Kind::Group, Kind::Guild, Kind::Shouts, Kind::Combat, Kind::Loot, Kind::Rolls, Kind::Experience, Kind::System];

    /// Its name in client.opt (`chat_tab.<key>`).
    pub fn key(self) -> &'static str {
        match self {
            Kind::Say => "say",
            Kind::Tells => "tells",
            Kind::Group => "group",
            Kind::Guild => "guild",
            Kind::Shouts => "shouts",
            Kind::Combat => "combat",
            Kind::Loot => "loot",
            Kind::Rolls => "rolls",
            Kind::Experience => "experience",
            Kind::System => "system",
        }
    }

    /// Its name as the options' rows and a tab show it.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Say => "Say",
            Kind::Tells => "Tells",
            Kind::Group => "Group",
            Kind::Guild => "Guild",
            Kind::Shouts => "Shouts",
            Kind::Combat => "Combat",
            Kind::Loot => "Loot",
            Kind::Rolls => "Rolls",
            Kind::Experience => "Experience",
            Kind::System => "System",
        }
    }

    pub fn index(self) -> usize {
        Kind::ALL.iter().position(|&k| k == self).unwrap_or(KINDS - 1)
    }
}

/// A tab's name as kept: its first `TAB_NAME_MOST` characters (control
/// characters out, spaces at its ends trimmed), NUL past its end.
pub fn tab_name(s: &str) -> [char; TAB_NAME_MOST] {
    let mut out = ['\0'; TAB_NAME_MOST];
    for (i, c) in s.trim().chars().filter(|c| !c.is_control()).take(TAB_NAME_MOST).enumerate() {
        out[i] = c;
    }
    out
}

/// A kept name as a string.
pub fn tab_name_text(n: &[char; TAB_NAME_MOST]) -> String {
    n.iter().take_while(|&&c| c != '\0').collect::<String>().trim_end().to_string()
}

/// A colour's three bytes.
fn rgb(c: [f32; 4]) -> [u8; 3] {
    [0, 1, 2].map(|i| (c[i].clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// What kind of line `text` in colour `color` is (`Kind`), as the server words
/// and colours them (go-eqoa-server's and the reference server's: a tell in (255, 127,
/// 255), the group's in (.5, .5, 1), the guild's in (.25, 1, .8), a shout
/// "<name> shouts: ", a roll "<name> rolls N (a to b)." in gold or orange; a
/// say "<name>: "), the rest by their words.
pub fn kind_of(text: &str, color: [f32; 4]) -> Kind {
    let c = rgb(color);
    let low = text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| low.contains(w));
    if c == [255, 127, 255] || has(&[" tells you, '", " whispers, '"]) || low.starts_with("you told ") || low.starts_with("you whisper to ") {
        return Kind::Tells;
    }
    if c == [128, 128, 255] || has(&[" tells the group", " tells the raid"]) {
        return Kind::Group;
    }
    if c == [64, 255, 204] || has(&[" tells the guild"]) {
        return Kind::Guild;
    }
    if text.split_once(" shouts: ").is_some_and(|(n, _)| !n.is_empty() && !n.contains(':')) {
        return Kind::Shouts;
    }
    if c == [255, 215, 64] || c == [255, 140, 0] {
        return Kind::Rolls;
    }
    // a say before the words: what someone says ("Bob: incoming, mez
    // resisted", a quick chat's line) is theirs whatever words are in it
    // (petitions 2271 and 5262: says and the quick chat's lines with a
    // combat word in them went to the Combat tab)
    if c == [255, 255, 255]
        && let Some((name, _)) = text.split_once(": ")
        && !name.is_empty()
        && name.len() <= 40
        && name.split(' ').count() <= 4
        && name.chars().all(|ch| ch.is_alphabetic() || matches!(ch, ' ' | '\'' | '-' | '`' | '.'))
    {
        return Kind::Say;
    }
    if has(&[" rolls ", "you roll ", " roll for ", " need roll", " greed roll", " rolled "]) {
        return Kind::Rolls;
    }
    if has(&[" xp", "experience"]) {
        return Kind::Experience;
    }
    if has(&["loot", "you receive ", "you have received ", " receives "]) {
        return Kind::Loot;
    }
    if has(&[" hits you", "you hit ", " hit you", " hits ", "damage", " slain", " slays ", " misses", "you miss", "resist", "has died", " dies.", " critical", "stunned", "interrupted", "out of range", "too far away"]) {
        return Kind::Combat;
    }
    Kind::System
}

/// The tabs: the rings of the tabs not shown (the one shown is `WndChat::text`),
/// which is shown, which have lines unread, where the last frame drew each name.
struct Tabs {
    parked: Vec<Option<ChatText>>,
    active: usize,
    unread: [bool; TABS + 1],
    hits: Vec<([i32; 4], usize)>,
    /// each kind's tab (0 Main) and the tabs' names, as the settings have them
    routes: [u8; KINDS],
    names: [[char; TAB_NAME_MOST]; TABS],
}

impl Default for Tabs {
    fn default() -> Self {
        Tabs { parked: (0..=TABS).map(|_| None).collect(), active: 0, unread: [false; TABS + 1], hits: Vec::new(), routes: [0; KINDS], names: [['\0'; TAB_NAME_MOST]; TABS] }
    }
}

impl Tabs {
    /// Whether tab `t` is there: Main always, another while a kind goes to it.
    fn exists(&self, t: usize) -> bool {
        t == 0 || self.routes.iter().any(|&r| r as usize == t)
    }

    /// The tabs there, in order, Main first.
    fn shown(&self) -> Vec<usize> {
        (0..=TABS).filter(|&t| self.exists(t)).collect()
    }

    /// Tab `t`'s name: Main; its own; else its first kind's.
    fn name(&self, t: usize) -> String {
        if t == 0 {
            return "Main".into();
        }
        let own = tab_name_text(&self.names[t - 1]);
        if !own.is_empty() {
            return own;
        }
        let first = self.routes.iter().position(|&r| r as usize == t).map_or("Tab", |k| Kind::ALL[k].label());
        first.chars().take(TAB_NAME_MOST).collect()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ChatSize {
    width: i32,
    rows: i32,
}

pub enum HistoryKey {
    NextTab,
    PageUp,
    PageDown,
}

/// Fade the complete pane, including opaque solid fills and their labels.
/// Blend-zero fills normally ignore their alpha, so use alpha blending while
/// fading and preserve their originally opaque appearance at the fade start.
pub fn fade_quads(quads: &mut [render::ui::Quad], alpha: f32) {
    if alpha >= 1.0 { return; }
    for q in quads {
        if q.blend == 0 { q.blend = 3; q.color[3] = alpha; }
        else { q.color[3] *= alpha; }
    }
}

impl ChatSize {
    pub const BASE_WIDTH: i32 = 462;
    pub const ORIGINAL: Self = Self { width: Self::BASE_WIDTH, rows: 5 };

    pub fn from_dimensions(opts: &crate::settings::ClientOptions, screen_width: f32, (width, rows): (u8, u8)) -> Self {
        let available = (screen_width * 100.0 / opts.hud_scale as f32).floor() as i32;
        Self { width: (Self::BASE_WIDTH * width as i32 / 100).min(available), rows: rows as i32 }
    }

    fn wrap_width(self) -> i32 {
        self.width - 45
    }

    fn init_window(self, w: &mut Window, vui: &Vui, ui: &dyn Raster, typing: bool, height: i32) {
        let rows = self.rows + i32::from(typing);
        let extra = (rows - 5) * (vui.font_height(ui) - 4);
        // Keep the original right edge and bottom text baseline as the box grows.
        let center = [653 - (self.width + 1) / 2, 475 + extra.div_euclid(2) - height];
        w.init(vui, ui, center, self.width, rows + 1, 1);
    }

    pub fn full_top_left(self, vui: &Vui, ui: &dyn Raster) -> [i32; 2] {
        let mut w = Window::default();
        self.init_window(&mut w, vui, ui, false, 85 + (self.rows - 5) * (vui.font_height(ui) - 4));
        let tl = w.top_left();
        [tl[0] + 25, tl[1]]
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
    /// `m_scroll`: 1 while scrolling (`SetScroll`)
    scroll: i32,
    /// the first line shown while scrolling, and the line after the last
    top: usize,
    bottom: usize,
    /// This pane has the keys while chat is expanded, even when following live chat.
    history_held: bool,
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
    /// Chat Always Shown (the module's doc): open at five rows, every line.
    pub always: bool,
    pub size: ChatSize,
    pub opacity: u8,
    pub typing: bool,
    row_height: i32,
    /// The chat tabs (the module's doc).
    tabs: Tabs,
    /// Detached tabs own their original history and their own interaction state.
    panes: [Option<Box<WndChat>>; TABS],
    pane_title: Option<String>,
    control_hit: Option<[i32; 4]>,
    last_activity: Option<u32>,
    hovered: bool,
    wake_on_message: bool,
    /// PC mode: the tab strip sits against the painted frame. Controller mode
    /// keeps its tabs where they were (`draw_tabs`).
    pub pc_tabs: bool,
}

impl WndChat {
    /// An enabled PC fade replaces the legacy slide-away, even when the
    /// separate Always Shown option is off.
    pub fn hold_open(always: bool, moving: bool, pc: bool, seconds: u8) -> bool {
        always || moving || (pc && seconds > 0)
    }

    /// Whether frame point `p` is on the box while it shows (before the right
    /// side's move), for a pointer.
    pub fn contains(&self, p: [f32; 2]) -> bool {
        let (tl, br) = (self.window.top_left(), self.window.bottom_right());
        (self.window.is_open() || self.window.is_opening()) && p[0] >= tl[0] as f32 && p[0] < br[0] as f32 && p[1] >= tl[1] as f32 && p[1] < br[1] as f32
    }

    /// `Init`: the window is 462 wide and lines wrap at its width less 45.
    pub fn new() -> Self {
        let size = ChatSize::ORIGINAL;
        WndChat { mode: 0, height: 0, target: 0, lines: 0, text: ChatText::new(size.wrap_width()), window: Window::default(), scroll: 0, top: 0, bottom: 0, history_held: false, held: 0, repeat: 0.0, cues: Vec::new(), shown: Vec::new(), sel: None, drag: None, told: None, wheel: 0.0, following: false, link_hits: Vec::new(), show_links: true, walk: None, always: false, size, opacity: 75, typing: false, row_height: 17, tabs: Tabs::default(), panes: std::array::from_fn(|_| None), pane_title: None, control_hit: None, last_activity: None, hovered: false, wake_on_message: false, pc_tabs: false }
    }

    /// A pointer action or active interaction restores this pane without
    /// affecting the other detached panes.
    pub fn activity(&mut self, now: u32) { self.last_activity = Some(now); }

    /// Set once per frame before routing incoming messages, rather than
    /// consulting settings separately for every line in a busy channel.
    pub fn set_wake_on_message(&mut self, on: bool) { self.wake_on_message = on; }

    /// Hover includes the pane's tab and detach/dock controls, even when its
    /// quads are fully transparent; hit testing is never removed by fading.
    pub fn hover(&mut self, p: Option<[f32; 2]>, now: u32) -> bool {
        let hovered = p.is_some_and(|p| self.contains(p) || self.control_at(p)
            || self.tabs.hits.iter().any(|(r, _)| p[0] >= r[0] as f32 && p[0] < r[2] as f32 && p[1] >= r[1] as f32 && p[1] < r[3] as f32));
        if hovered || self.hovered { self.activity(now); }
        self.hovered = hovered;
        hovered
    }

    /// PC inactivity: full opacity for the chosen delay, then a one-second
    /// linear fade. Wrapping arithmetic keeps the session clock's rollover safe.
    pub fn fade_alpha(&mut self, now: u32, seconds: u8, pc: bool, editing: bool) -> f32 {
        if !pc || seconds == 0 || editing || self.hovered || self.typing || self.drag.is_some() || self.history_held || self.held != 0 {
            self.activity(now);
            return 1.0;
        }
        let start = *self.last_activity.get_or_insert(now);
        let elapsed = now.wrapping_sub(start);
        let delay = u32::from(seconds) * 1000;
        1.0 - (elapsed.saturating_sub(delay) as f32 / 1000.0).min(1.0)
    }

    pub fn resize(&mut self, size: ChatSize, measure: impl Fn(&[u16]) -> i32) {
        self.resize_with(size, &measure);
    }

    fn resize_with(&mut self, size: ChatSize, measure: &dyn Fn(&[u16]) -> i32) {
        // A reattached ring may have been wrapped to a different pane width
        // even when this window's own size has not changed.
        let changed = self.size != size || self.text.width != size.wrap_width() - 30;
        for ring in std::iter::once(&mut self.text).chain(self.tabs.parked.iter_mut().flatten()) {
            if ring.width != size.wrap_width() - 30 {
                ring.rewrap(size.wrap_width(), &measure);
            }
        }
        if !changed { return; }
        self.size = size;
        self.sel = None;
        self.drag = None;
        self.told = None;
        self.set_scroll(self.scroll);
    }

    /// The tabs as the settings have them now (each kind's tab, the names):
    /// a tab no kind goes to any more is not drawn, and shown, Main is.
    pub fn set_tabs(&mut self, routes: [u8; KINDS], names: [[char; TAB_NAME_MOST]; TABS]) {
        self.tabs.routes = routes.map(|r| if (r as usize) <= TABS { r } else { 0 });
        self.tabs.names = names;
        for t in 1..=TABS {
            // Keep an orphaned tab's history parked if its last route is removed.
            // Its saved detach preference can take effect again when rerouted.
            if !self.tabs.exists(t) { self.set_detached(t, false); }
            let name = self.tabs.name(t);
            if let Some(pane) = self.panes[t - 1].as_mut() {
                pane.pane_title = Some(name);
            }
        }
        if !self.tabs.exists(self.tabs.active) {
            self.show_tab(0);
        }
    }

    /// A line for the box (`add_text_linked`), into the tab its kind goes to;
    /// one not shown is marked unread.
    pub fn add_line(&mut self, text: &str, c: [f32; 4], now: u32, width: impl Fn(&[u16]) -> i32, links: &[net::itemlink::ChatLink]) {
        let t = self.tabs.routes[kind_of(text, c).index()] as usize;
        let wide: Vec<u16> = text.encode_utf16().collect();
        if t > 0 && let Some(pane) = self.panes[t - 1].as_mut() {
            if self.wake_on_message { pane.activity(now); }
            let at_end = pane.scroll == 1 && pane.bottom == pane.text.end();
            pane.text.add_text_linked(&wide, c, now, width, links);
            if at_end && !pane.selecting() { pane.set_scroll(1); }
        } else if t == self.tabs.active {
            if self.wake_on_message { self.activity(now); }
            self.text.add_text_linked(&wide, c, now, width, links);
        } else {
            if self.wake_on_message { self.activity(now); }
            let ring = self.tabs.parked[t].get_or_insert_with(|| ChatText::new(self.size.wrap_width()));
            ring.add_text_linked(&wide, c, now, width, links);
            self.tabs.unread[t] = true;
        }
    }

    /// The tab shown (0 Main).
    pub fn tab(&self) -> usize {
        self.tabs.active
    }

    /// The tabs there and their names, Main first, and whether each has lines unread.
    pub fn tab_names(&self) -> Vec<(usize, String, bool)> {
        self.tabs.shown().into_iter().map(|t| (t, self.tabs.name(t), self.tabs.unread[t])).collect()
    }

    /// Shows tab `t` (one not there: nothing, false): its lines in the box, the
    /// selection and the walk let go, the history's view on its newest lines.
    pub fn show_tab(&mut self, t: usize) -> bool {
        if t > TABS || !self.tabs.exists(t) || (t > 0 && self.panes[t - 1].is_some()) {
            return false;
        }
        self.tabs.unread[t] = false;
        if t == self.tabs.active {
            return true;
        }
        let into = self.tabs.parked[t].take().unwrap_or_else(|| ChatText::new(self.size.wrap_width()));
        let out = std::mem::replace(&mut self.text, into);
        self.tabs.parked[self.tabs.active] = Some(out);
        self.tabs.active = t;
        self.sel = None;
        self.drag = None;
        self.walk = None;
        self.told = None;
        self.shown.clear();
        self.link_hits.clear();
        if self.scroll == 1 {
            self.set_scroll(1);
        }
        true
    }

    /// Move, never copy, the tab's history between the main window and its pane.
    pub fn set_detached(&mut self, t: usize, detached: bool) -> bool {
        if !(1..=TABS).contains(&t) { return false; }
        if detached {
            if !self.tabs.exists(t) || self.panes[t - 1].is_some() { return false; }
            if self.tabs.active == t { self.show_tab(0); }
            let mut pane = WndChat::new();
            pane.size = self.size;
            pane.text = self.tabs.parked[t].take().unwrap_or_else(|| ChatText::new(self.size.wrap_width()));
            pane.pane_title = Some(self.tabs.name(t));
            pane.always = true;
            self.tabs.unread[t] = false;
            self.panes[t - 1] = Some(Box::new(pane));
        } else {
            let Some(pane) = self.panes[t - 1].take() else { return false };
            self.tabs.parked[t] = Some(pane.text);
            if pane.history_held {
                if !self.show_tab(t) { self.show_tab(0); }
                self.hold_history();
            }
        }
        true
    }

    pub fn pane(&self, t: usize) -> Option<&WndChat> {
        self.panes.get(t.checked_sub(1)?)?.as_deref()
    }

    pub fn pane_mut(&mut self, t: usize) -> Option<&mut WndChat> {
        self.panes.get_mut(t.checked_sub(1)?)?.as_deref_mut()
    }

    fn history_pane(&self) -> Option<usize> {
        (1..=TABS).find(|&t| self.pane(t).is_some_and(|p| p.history_held))
    }

    pub fn history(&self) -> &Self {
        self.history_pane().and_then(|t| self.pane(t)).unwrap_or(self)
    }

    pub fn history_mut(&mut self) -> &mut Self {
        match self.history_pane() {
            Some(t) => self.panes[t - 1].as_deref_mut().unwrap(),
            None => self,
        }
    }

    pub fn begin_history(&mut self) {
        self.end_history();
        self.show_tab(0);
        self.hold_history();
    }

    pub fn end_history(&mut self) {
        let pane = self.history_mut();
        if pane.history_held {
            pane.history_held = false;
            pane.held = 0;
            pane.set_scroll(0);
        }
    }

    fn hold_history(&mut self) {
        self.history_held = true;
        if self.scroll != 1 { self.set_scroll(1); }
    }

    /// Main comes first. Docked tabs share a window and keep their tab order.
    pub fn cycle_history(&mut self, x: impl Fn(usize, [f32; 2]) -> f32) {
        let current = self.history_pane().unwrap_or(self.tabs.active);
        let mut tabs = self.tabs.shown();
        let left = |t| {
            let (piece, pane) = self.pane(t).map_or((0, &*self), |p| (t, p));
            x(piece, pane.window.top_left().map(|v| v as f32))
        };
        tabs[1..].sort_by(|&a, &b| left(a).total_cmp(&left(b)));
        let next = tabs.iter().position(|&t| t == current).and_then(|i| tabs.get(i + 1)).copied().unwrap_or(0);
        if next == current { return; }
        self.end_history();
        if let Some(pane) = self.pane_mut(next) {
            pane.hold_history();
            pane.cues.push(11);
        } else {
            self.show_tab(next);
            self.hold_history();
            self.cues.push(11);
        }
    }

    pub fn control_at(&self, p: [f32; 2]) -> bool {
        self.control_hit.is_some_and(|r| p[0] >= r[0] as f32 && p[0] < r[2] as f32 && p[1] >= r[1] as f32 && p[1] < r[3] as f32)
    }

    /// A click at point `p` (as `contains` takes one): on a tab's name, that
    /// tab shown (sound 11).
    pub fn click_tab(&mut self, p: [f32; 2]) -> bool {
        let (x, y) = (p[0] as i32, p[1] as i32);
        let Some(&(_, t)) = self.tabs.hits.iter().find(|(r, _)| x >= r[0] && x < r[2] && y >= r[1] && y < r[3]) else { return false };
        self.show_tab(t);
        self.cues.push(11);
        true
    }

    /// The tabs' names over the box while it is open (with a tab besides Main):
    /// the one shown light yellow and underlined, one with lines unread green,
    /// the rest grey, each noted for a click (`click_tab`).
    fn draw_tabs(&mut self, vui: &Vui, ui: &mut dyn crate::ui::Layout) {
        self.tabs.hits.clear();
        let shown: Vec<_> = self.tabs.shown().into_iter().filter(|&t| t == 0 || self.panes[t - 1].is_none()).collect();
        if shown.len() < 2 || !(self.fixed() || self.scroll == 1) || !(self.window.is_open() || self.window.is_opening()) {
            return;
        }
        let h = vui.font_height(ui);
        let tl = self.window.top_left();
        // The chat uses frame set 3, painted from logical top-left + 10: the
        // PC strip meets it; Controller keeps its tabs as they were.
        let (mut x, y) = if self.pc_tabs { (tl[0] + 14, tl[1] + 13 - h) } else { (tl[0] + 25, tl[1] - h + 2) };
        let count = shown.len();
        for (i, t) in shown.into_iter().enumerate() {
            let mut name = crate::ui::wide(&self.tabs.name(t));
            let mut w = vui.string_width(ui, &name);
            if self.size.width < ChatSize::BASE_WIDTH {
                let remaining = (count - i) as i32;
                let room = (self.window.bottom_right()[0] - 20 - x - 14 * (remaining - 1)) / remaining;
                if w > room {
                    name = crate::startmenu::fit_label(vui, ui, &name, &[], room);
                    w = vui.string_width(ui, &name);
                }
            }
            let color = if t == self.tabs.active {
                [1.0, 1.0, 0.6, 1.0]
            } else if self.tabs.unread[t] {
                [0.45, 1.0, 0.45, 1.0]
            } else {
                [0.7, 0.7, 0.7, 1.0]
            };
            let tab_rect = [[x - 4, y - 1], [x + w + 4, y - 1], [x + w + 4, y + h - 3], [x - 4, y + h - 3]];
            if self.opacity == 75 {
                ui.fill_fan([0.0, 0.0, 0.0, 0.5], tab_rect);
            } else {
                ui.fill_blended([0.0, 0.0, 0.0, self.opacity as f32 / 100.0], tab_rect);
            }
            vui.draw_string_ex(ui, &name, x, y, color, true);
            if t == self.tabs.active {
                ui.fill_fan(color, [[x, y + h - 5], [x + w, y + h - 5], [x + w, y + h - 4], [x, y + h - 4]]);
            }
            self.tabs.hits.push(([x - 4, y - 1, x + w + 4, y + h - 3], t));
            x += w + 14;
        }
    }

    /// PC pane control, in its own header above the window.
    pub fn draw_pane_control(&mut self, vui: &Vui, ui: &mut dyn crate::ui::Layout, pc: bool) {
        self.control_hit = None;
        if !pc || !(self.window.is_open() || self.window.is_opening()) { return; }
        let label = if let Some(title) = &self.pane_title {
            format!("{title}  [Dock]")
        } else if self.tabs.active > 0 {
            "[Detach tab]".to_string()
        } else { return };
        let text = crate::ui::wide(&label);
        let w = vui.string_width(ui, &text);
        let h = vui.font_height(ui);
        let tl = self.window.top_left();
        // A detached pane has no tab strip; the main window's action sits
        // above its tabs so both remain separately clickable.
        let x = tl[0] + 14;
        let tab_y = tl[1] + 13 - h;
        let y = if self.pane_title.is_some() { tab_y } else { tab_y - h + 2 };
        ui.fill_fan([0.0, 0.0, 0.0, self.opacity as f32 / 100.0], [[x-4,y-1],[x+w+4,y-1],[x+w+4,y+h-3],[x-4,y+h-3]]);
        let color = [1.0, 1.0, 0.6, 1.0];
        vui.draw_string_ex(ui, &text, x, y, color, true);
        if self.pane_title.is_some() && self.history_held {
            ui.fill_fan(color, [[x, y+h-5], [x+w, y+h-5], [x+w, y+h-4], [x, y+h-4]]);
        }
        self.control_hit = Some([x-4,y-1,x+w+4,y+h-3]);
    }

    /// The number of fresh rows visible, or all configured rows while fixed.
    fn count_lines(&self) -> i32 {
        if self.fixed() {
            return self.size.rows;
        }
        let mut n = 0;
        let mut i = self.text.first();
        while i != self.text.end() {
            if self.text.line(i).is_some_and(|l| l.kind == 1 || self.scroll == 1) {
                n += 1;
            }
            i = ChatText::next(i);
        }
        n.min(self.size.rows)
    }

    /// The inline editor occupies the last row inside the frame.
    pub fn input_rect(&self, vui: &Vui, ui: &dyn Raster) -> [i32; 4] {
        let x = self.window.top_left()[0] + 20;
        let y = self.window.row_center(vui, ui, self.size.rows)[1] - self.row_height / 2;
        [x, y, x + self.size.width - 42, y + self.row_height]
    }

    fn full_height(&self) -> i32 {
        85 + (self.size.rows + i32::from(self.typing) - 5) * self.row_height
    }

    /// Whether the box shows every configured row outside the scroll view.
    fn fixed(&self) -> bool {
        self.mode != 0 || self.always || self.typing || (self.history_held && self.scroll == 0)
    }

    /// The controller's scroll view keeps its original indicator. In PC mode
    /// there is nothing to scroll until wrapped history exceeds the rows.
    fn scroll_indicator(&self, pc: bool) -> bool {
        self.scroll == 1 && (!pc || self.text.count() > self.size.rows as usize)
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
            self.target = self.full_height();
            self.height = self.target;
            self.top = self.text.end();
            self.bottom = self.top;
            for _ in 0..self.size.rows {
                if self.top == self.text.first() {
                    break;
                }
                self.top = ChatText::prev(self.top);
            }
        } else {
            self.bottom = 0;
            self.held = 0;
            self.wheel = 0.0;
        }
    }

    /// `ScrollDown`: a line later, unless at the end.
    fn scroll_down(&mut self) {
        if self.scroll == 1 && self.bottom != self.text.end() {
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

    pub fn page_history(&mut self, older: bool) {
        self.held = 0;
        if older && self.scroll != 1 { self.set_scroll(1); }
        let before = self.bottom;
        let cues = self.cues.len();
        for _ in 0..self.size.rows {
            if older { self.scroll_up(); }
            else { self.scroll_down(); }
        }
        self.cues.truncate(cues);
        self.cues.push(if self.bottom == before { 9 } else { 11 });
    }

    /// `OnKeyDown`: up and down scroll, and repeat while held. False for a
    /// key it passes up to the game window (only L3, 146).
    pub fn key_down(&mut self, key: i32) -> bool {
        self.held = 0;
        match key {
            19 | 138 => {
                if self.scroll != 1 { self.set_scroll(1); }
                self.held = key;
                self.repeat = 0.45;
                self.scroll_up();
            }
            20 | 139 => {
                if self.scroll == 1 {
                    self.held = key;
                    self.repeat = 0.45;
                }
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
    pub fn draw(&mut self, vui: &Vui, ui: &mut dyn crate::ui::Layout, now: u32, dt: f32) {
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
        if self.mode != 0 {
            crate::ui::chat_strip(vui, ui, [200, 331], [442, 118]);
        }
        self.follow();
        let it = if self.scroll == 1 { self.bottom } else { self.text.end() };
        self.lines = if self.scroll == 1 { self.size.rows } else { self.count_lines() };
        let h = vui.font_height(ui);
        self.row_height = h - 4;
        if self.fixed() {
            self.target = self.full_height();
            self.height = self.target;
            self.window.open(true);
        } else if self.lines == 0 {
            self.target = -((h - 4) * 2);
            self.window.close(false);
        } else {
            self.target = (h - 4) * self.lines;
            self.window.open(false);
        }
        self.size.init_window(&mut self.window, vui, ui, self.typing, self.height);
        if self.window.is_open() || self.window.is_opening() || self.window.is_closing() {
            self.window.draw_with_panel_opacity(vui, ui, self.opacity as f32 / 100.0);
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
                && (self.scroll == 1 || (self.scroll == 0 && l.kind == 1) || self.fixed())
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
        self.draw_tabs(vui, ui);
        self.drag_on(dt);
        self.tell_copy();
        if self.scroll_indicator(crate::settings::pc_mode()) {
            let n = self.text.count();
            let rows = self.size.rows as usize;
            let t = if n > rows { self.text.index(self.top) as f32 / (n - rows) as f32 } else { 0.0 };
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
        if !held && (self.scroll != 1 || !(self.contains(p) || self.drag.is_some())) {
            return false;
        }
        self.scroll_wheel(lines);
        true
    }

    /// Mouse history does not require holding the controller's scroll mode.
    pub fn pc_wheel(&mut self, p: [f32; 2], lines: f32) -> bool {
        if !self.contains(p) { return false; }
        self.scroll_wheel(lines);
        true
    }

    fn scroll_wheel(&mut self, lines: f32) {
        if !lines.is_finite() { return; }
        self.wheel += lines;
        if self.scroll != 1 {
            if self.text.count() <= self.size.rows as usize {
                self.wheel = 0.0;
                return;
            }
            self.wheel = self.wheel.max(0.0);
            if self.wheel < 1.0 { return; }
            self.set_scroll(1);
        }
        let cues = self.cues.len();
        let steps = (self.wheel.abs() as usize).min(self.text.count());
        for _ in 0..steps {
            if self.wheel > 0.0 {
                self.scroll_up();
            } else {
                self.scroll_down();
            }
        }
        self.wheel = self.wheel.fract();
        self.cues.truncate(cues);
        if lines < 0.0 && self.bottom == self.text.end() && !self.selecting() {
            self.set_scroll(0);
        }
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
        if self.history_held && self.scroll != 1 { self.set_scroll(1); }
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

    /// Ticket 1951: each line's kind as the server words and colours it.
    #[test]
    fn a_lines_kind_is_read_off_its_words_and_colour() {
        let rgb = |r: u8, g: u8, b: u8| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0];
        let white = [1.0; 4];
        for (text, color, want) in [
            ("Ann tells you, 'hi'", rgb(255, 127, 255), Kind::Tells),
            ("You told Ann, 'hi'", rgb(255, 127, 255), Kind::Tells),
            ("Ann whispers, 'psst'", rgb(255, 127, 255), Kind::Tells),
            ("Ann tells the group: pull", rgb(128, 128, 255), Kind::Group),
            ("Ann: the guild's line", rgb(64, 255, 204), Kind::Guild),
            ("Powerbrick shouts: yrs?", rgb(63, 255, 63), Kind::Shouts),
            ("Ann rolls 42 (1 to 100).", rgb(255, 215, 64), Kind::Rolls),
            ("You receive 1200 XP.", white, Kind::Experience),
            ("Your death left an experience debt of 50 (50 owed in all).", white, Kind::Experience),
            ("You have looted a Rusty Sword.", white, Kind::Loot),
            ("Gnoll Guard's Lava Breath hits you for 300 (resisted).", white, Kind::Combat),
            ("You have slain a gnoll.", rgb(255, 140, 40), Kind::Combat),
            ("Ann: hello there", white, Kind::Say),
            ("Warden Holt: Well done.", white, Kind::Say),
            ("The DPS meter is on.", white, Kind::System),
            ("Tell Failed", [1.0, 0.25, 0.25, 1.0], Kind::System),
        ] {
            assert_eq!(kind_of(text, color), want, "{text:?}");
        }
        // the controls: a say quoting a shout is a say; a tell's words in
        // another colour are still a tell
        assert_eq!(kind_of("Bob: Ann shouts: hi", white), Kind::Say);
        assert_eq!(kind_of("Ann tells you, 'hi'", white), Kind::Tells);
        // petitions 2271 and 5262: a say (or a quick chat's line said) with
        // a combat, loot or experience word in it is a say; the controls:
        // the same words in a line of no speaker are what they were
        for say in ["Jibrish: mez resisted, it hits hard", "Jibrish: loot is yours", "Jibrish: 20 xp a kill", "Jibrish: you roll first"] {
            assert_eq!(kind_of(say, white), Kind::Say, "{say:?}");
        }
        assert_eq!(kind_of("Your Mesmerize was resisted.", white), Kind::Combat);
        assert_eq!(kind_of("You have looted a Rusty Sword.", white), Kind::Loot);
        assert_eq!(kind_of("You roll 12 (1 to 100).", white), Kind::Rolls);
    }

    /// Ticket 1951: a kind sent to a tab goes there and not to Main; the tab
    /// is drawn (with Main) only while some kind goes to it, is named by its
    /// first kind until named, marks lines unread until shown, and shown,
    /// holds its lines while Main is shown again. The control: every kind
    /// Main's, every line in the box, no tabs.
    #[test]
    fn a_kind_sent_to_a_tab_goes_there_alone() {
        let w = |s: &[u16]| s.len() as i32 * 8;
        let rows = |t: &ChatText| {
            let (mut out, mut i) = (Vec::new(), t.first());
            while i != t.end() {
                out.push(plain(&t.line(i).unwrap().text));
                i = ChatText::next(i);
            }
            out
        };
        let tell = [1.0, 127.0 / 255.0, 1.0, 1.0];
        let mut c = WndChat::new();
        c.add_line("Ann: hello", [1.0; 4], 0, w, &[]);
        c.add_line("Ann tells you, 'psst'", tell, 0, w, &[]);
        assert_eq!(rows(&c.text), ["Ann: hello", "Ann tells you, 'psst'"], "the control: all Main's");
        assert_eq!(c.tab_names().len(), 1, "no tabs");
        let mut routes = [0u8; KINDS];
        routes[Kind::Tells.index()] = 2;
        routes[Kind::Loot.index()] = 2;
        let mut names = [['\0'; TAB_NAME_MOST]; TABS];
        c.set_tabs(routes, names);
        c.add_line("Ann tells you, 'again'", tell, 0, w, &[]);
        c.add_line("You have looted a Rusty Sword.", [1.0; 4], 0, w, &[]);
        c.add_line("Bob: hi", [1.0; 4], 0, w, &[]);
        assert_eq!(rows(&c.text), ["Ann: hello", "Ann tells you, 'psst'", "Bob: hi"], "Main keeps no tell from now");
        assert_eq!(c.tab_names(), vec![(0, "Main".to_string(), false), (2, "Tells".to_string(), true)]);
        assert!(!c.show_tab(1), "tab 1 has no kind: not there");
        assert!(c.show_tab(2));
        assert_eq!(rows(&c.text), ["Ann tells you, 'again'", "You have looted a Rusty Sword."]);
        assert_eq!(c.tab_names()[1].2, false, "read");
        c.add_line("Cid: yo", [1.0; 4], 0, w, &[]);
        assert!(c.tab_names()[0].2, "Main unread");
        names[1] = tab_name("Whispers and loot");
        c.set_tabs(routes, names);
        assert_eq!(c.tab_names()[1].1, "Whispers", "named, eight characters");
        assert!(c.show_tab(0));
        assert_eq!(rows(&c.text).last().map(String::as_str), Some("Cid: yo"));
        // the tab given up while shown: Main is shown
        assert!(c.show_tab(2));
        c.set_tabs([0; KINDS], names);
        assert_eq!(c.tab(), 0);
        assert_eq!(c.tab_names().len(), 1);
    }

    #[test]
    fn detached_tabs_keep_routing_history_and_independent_scroll() {
        let mut c = WndChat::new();
        let mut routes = [0; KINDS];
        routes[Kind::Tells.index()] = 1;
        routes[Kind::Combat.index()] = 2;
        c.set_tabs(routes, [['\0'; TAB_NAME_MOST]; TABS]);
        assert!(c.show_tab(1));
        c.text.add_text(&crate::ui::wide("old tell"), [1.0; 4], 0, width);
        assert!(c.set_detached(1, true));
        assert!(c.set_detached(2, true));
        assert_eq!(c.tab(), 0);
        for i in 0..10 {
            c.pane_mut(1).unwrap().text.add_text(&crate::ui::wide(&format!("line {i}")), [1.0; 4], 0, width);
        }
        c.pane_mut(1).unwrap().set_scroll(1);
        c.pane_mut(1).unwrap().scroll_up();
        let before = c.pane(1).unwrap().bottom;
        assert_eq!(c.pane(2).unwrap().scroll, 0);
        // A new routed tell leaves the earlier history view in place.
        c.add_line("Someone tells you, 'hello'", [1.0, 0.5, 1.0, 1.0], 0, width, &[]);
        assert_eq!(c.pane(1).unwrap().bottom, before);
        assert_eq!(c.pane(1).unwrap().text.count(), 12);
        assert_eq!(c.pane(2).unwrap().text.count(), 0);
        c.pane_mut(1).unwrap().resize(ChatSize { width: 300, rows: 6 }, width);
        assert_eq!(c.pane(1).unwrap().size.width, 300);
        assert!(c.set_detached(1, false));
        assert!(c.show_tab(1));
        c.resize(ChatSize::ORIGINAL, width);
        assert_eq!(c.text.width, ChatSize::ORIGINAL.wrap_width() - 30, "reattached history fits Main");
        assert!(c.text.count() >= 11, "the original history was moved back");
        assert!(c.pane(2).is_some(), "the other pane stays open");
    }

    #[test]
    fn detached_tab_draws_control_and_link_then_rejoins_after_rerouting() {
        let (v, mut r) = (vui(), Rows);
        let mut c = WndChat::new();
        c.always = true;
        let mut routes = [0; KINDS];
        routes[Kind::Tells.index()] = 1;
        let names = [['\0'; TAB_NAME_MOST]; TABS];
        c.set_tabs(routes, names);
        for i in 0..ChatSize::ORIGINAL.rows {
            c.add_line(&format!("Ann tells you, 'line {i}'"), [1.0; 4], 0, width, &[]);
        }
        let line = "Ann tells you, 'Rusty Sword'";
        let at = line.find("Rusty").unwrap();
        c.add_line(line, [1.0; 4], 0, width, &[link(at, 11, 42)]);
        assert!(c.show_tab(1));
        c.draw(&v, &mut r, 0, 0.1);
        c.draw_pane_control(&v, &mut r, true);
        let control = c.control_hit.unwrap();
        assert!(c.control_at([(control[0] + 1) as f32, (control[1] + 1) as f32]));
        assert!(c.set_detached(1, true));
        let pane = c.pane_mut(1).unwrap();
        pane.draw(&v, &mut r, 0, 0.1);
        pane.draw_pane_control(&v, &mut r, true);
        let dock = pane.control_hit.unwrap();
        assert!(pane.control_at([(dock[0] + 1) as f32, (dock[1] + 1) as f32]));
        let link_point = pane.newest_link_at(0).expect("link drawn in detached pane");
        assert_eq!(pane.link_at(link_point).unwrap().0, 42);
        assert!(pane.pc_wheel(link_point, 1.0));
        assert_eq!(pane.scroll, 1, "the pane takes the wheel without controller scroll mode");
        assert_eq!(pane.text.index(pane.bottom), ChatSize::ORIGINAL.rows as usize);
        assert!(pane.pc_wheel(link_point, -1.0));
        assert_eq!(pane.scroll, 0, "the detached pane goes live at the bottom");
        // Removing the last route parks the same history. Restoring it can
        // detach the same tab again, as the persisted preference requests.
        c.set_tabs([0; KINDS], names);
        assert!(c.pane(1).is_none());
        c.set_tabs(routes, names);
        assert!(c.set_detached(1, true));
        let pane = c.pane_mut(1).unwrap();
        pane.draw(&v, &mut r, 0, 0.1);
        assert_eq!(pane.link_at(pane.newest_link_at(0).unwrap()).unwrap().0, 42);
        assert!(c.set_detached(1, false));
        assert!(c.show_tab(1));
        c.draw(&v, &mut r, 0, 0.1);
        assert_eq!(c.link_at(c.newest_link_at(0).unwrap()).unwrap().0, 42);
    }

    /// A shout is someone else's "<name> shouts: <text>" in the shout's
    /// colour, as the reference server's 0x0A7B carries it (the capture's own line); the
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

    impl crate::ui::Layout for Rows {}
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

    #[test]
    fn resize_rewraps_history_and_links_in_active_and_parked_tabs() {
        let mut c = with_lines(9);
        let mut routes = [0; KINDS];
        routes[Kind::Tells.index()] = 1;
        c.set_tabs(routes, [['\0'; TAB_NAME_MOST]; TABS]);
        let tell = "Ann tells you, 'meet me on the bridge with the Rusty Sword'";
        c.add_line(tell, [1.0; 4], 123, width, &[link(tell.find("Rusty").unwrap(), 11, 27)]);
        c.resize(ChatSize { width: 231, rows: 8 }, width);
        c.set_scroll(1);
        frame(&mut c, 0.1);
        assert_eq!(c.shown.len(), 8);
        assert_eq!(c.text.index(c.top), 1);
        assert!(c.show_tab(1));
        frame(&mut c, 0.1);
        let narrow = c.text.count();
        assert!(narrow > 2);
        assert_eq!(c.link_at(c.newest_link_at(0).unwrap()).unwrap(), (27, "Rusty Sword".into()));
        c.resize(ChatSize::ORIGINAL, width);
        frame(&mut c, 0.1);
        assert!(c.text.count() < narrow);
        assert_eq!(c.text.line(c.text.first()).unwrap().time, 123);
        assert_eq!(c.link_at(c.newest_link_at(0).unwrap()).unwrap(), (27, "Rusty Sword".into()));
        c.set_scroll(0);
        c.draw(&vui(), &mut Rows, 30_000, 0.1);
        assert!(c.shown.is_empty(), "resizing does not make old messages recent");
        c.typing = true;
        c.draw(&vui(), &mut Rows, 30_000, 0.1);
        let input = c.input_rect(&vui(), &Rows);
        assert!(c.shown.iter().all(|r| r.bottom <= input[1]));
        assert!(input[3] <= 448 && input[0] >= 0);
    }

    #[test]
    fn a_narrow_box_keeps_all_tab_buttons_in_reach() {
        let mut c = WndChat::new();
        let mut routes = [0; KINDS];
        routes[..3].copy_from_slice(&[1, 2, 3]);
        c.set_tabs(routes, ["Whispers", "Guildies", "Treasure"].map(tab_name));
        c.resize(ChatSize { width: 231, rows: 3 }, width);
        c.always = true;
        frame(&mut c, 0.1);
        let hits = c.tabs.hits.clone();
        assert_eq!(hits.len(), 4);
        for (rect, tab) in hits {
            assert!(rect[0] >= c.window.top_left()[0] && rect[2] <= c.window.bottom_right()[0]);
            assert!(c.click_tab([(rect[0] + 1) as f32, (rect[1] + 1) as f32]));
            assert_eq!(c.tab(), tab);
        }
    }

    #[test]
    fn tabs_and_pane_header_touch_the_painted_frame() {
        let (v, mut r) = (vui(), Rows);
        for box_width in [231, ChatSize::BASE_WIDTH, 577] {
            let mut c = WndChat::new();
            let mut routes = [0; KINDS];
            routes[Kind::Tells.index()] = 1;
            c.set_tabs(routes, [['\0'; TAB_NAME_MOST]; TABS]);
            c.resize(ChatSize { width: box_width, rows: 5 }, width);
            c.always = true;
            assert!(c.show_tab(1));
            c.draw(&v, &mut r, 0, 0.1);
            let tab = c.tabs.hits[0].0;
            assert_eq!([tab[0], tab[3]], [c.window.top_left()[0] + 21, c.window.top_left()[1] - 1], "Controller tabs stay where they were");
            c.pc_tabs = true;
            c.draw(&v, &mut r, 0, 0.1);
            c.draw_pane_control(&v, &mut r, true);
            let tab = c.tabs.hits[0].0;
            assert_eq!(tab[0], c.window.top_left()[0] + 10);
            assert_eq!(tab[3], c.window.top_left()[1] + 10, "tab meets painted frame");
            assert_eq!(c.control_hit.unwrap()[3], tab[1], "detach control meets tab strip");
            assert!(c.set_detached(1, true));
            let pane = c.pane_mut(1).unwrap();
            pane.draw(&v, &mut r, 0, 0.1);
            pane.draw_pane_control(&v, &mut r, true);
            let header = pane.control_hit.unwrap();
            assert_eq!(header[0], pane.window.top_left()[0] + 10);
            assert_eq!(header[3], pane.window.top_left()[1] + 10, "detached header meets painted frame");
            assert!(pane.control_at([(header[0] + 1) as f32, (header[3] - 1) as f32]));
        }
    }

    #[test]
    fn inactivity_fades_each_pane_and_activity_restores_it() {
        let mut c = WndChat::new();
        let mut routes = [0; KINDS];
        routes[Kind::Tells.index()] = 1;
        routes[Kind::Combat.index()] = 2;
        c.set_tabs(routes, [['\0'; TAB_NAME_MOST]; TABS]);
        assert!(c.set_detached(1, true));
        c.activity(1000);
        c.pane_mut(1).unwrap().activity(1000);
        assert_eq!(c.fade_alpha(6000, 5, true, false), 1.0);
        assert_eq!(c.fade_alpha(6500, 5, true, false), 0.5);
        assert_eq!(c.fade_alpha(7000, 5, true, false), 0.0);
        for _ in 0..20 {
            c.add_line("Ann tells you, 'hello'", [1.0; 4], 7000, width, &[]);
            c.add_line("You hit a target for 10 damage.", [1.0; 4], 7000, width, &[]);
        }
        assert_eq!(c.fade_alpha(7000, 5, true, false), 0.0, "busy parked channel does not wake Main by default");
        assert_eq!(c.pane_mut(1).unwrap().fade_alpha(7000, 5, true, false), 0.0, "busy detached channel stays faded by default");
        c.set_wake_on_message(true);
        c.add_line("Ann tells you, 'hello'", [1.0; 4], 7000, width, &[]);
        assert_eq!(c.fade_alpha(7000, 5, true, false), 0.0, "other pane remains faded");
        assert_eq!(c.pane_mut(1).unwrap().fade_alpha(7000, 5, true, false), 1.0, "routed message restores its pane");
        c.add_line("You hit a target for 10 damage.", [1.0; 4], 7000, width, &[]);
        assert_eq!(c.fade_alpha(7000, 5, true, false), 1.0, "opt-in parked-tab activity restores Main");
        let pane = c.pane_mut(1).unwrap();
        pane.draw(&vui(), &mut Rows, 7000, 0.1);
        pane.draw_pane_control(&vui(), &mut Rows, true);
        let h = pane.control_hit.unwrap();
        assert!(pane.hover(Some([(h[0] + 1) as f32, (h[1] + 1) as f32]), 8000));
        assert_eq!(pane.fade_alpha(20_000, 5, true, false), 1.0, "stationary header hover holds the pane visible");
        pane.hover(None, 20_000);
        assert_eq!(pane.fade_alpha(25_500, 5, true, false), 0.5, "fade restarts after pointer leaves");
        assert_eq!(c.fade_alpha(7000, 5, true, true), 1.0, "HUD edit and composition restore panes");
        assert_eq!(c.fade_alpha(8000, 0, true, false), 1.0, "zero disables fade");
        assert_eq!(c.fade_alpha(9000, 5, false, false), 1.0, "controller behavior stays");
        c.activity(u32::MAX - 499);
        assert_eq!(c.fade_alpha(5500, 5, true, false), 0.0, "clock rollover remains safe");
    }

    #[test]
    fn pc_fade_keeps_the_frame_and_pointer_region_open_without_always_shown() {
        let mut c = with_lines(1);
        c.always = WndChat::hold_open(false, false, true, 30);
        for _ in 0..30 { c.draw(&vui(), &mut Rows, 30_000, 0.1); }
        assert!(c.window.is_open(), "fade delay keeps an otherwise aged pane open");
        let tl = c.window.top_left();
        assert!(c.contains([(tl[0] + 20) as f32, (tl[1] + 20) as f32]), "hover can still restore the faded pane");
        assert!(!WndChat::hold_open(false, false, true, 0), "disabled fade retains legacy slide-away");
        assert!(!WndChat::hold_open(false, false, false, 30), "controller retains legacy slide-away");
    }

    #[test]
    fn fade_covers_opaque_and_blended_pane_quads() {
        let make = |blend, alpha| render::ui::Quad { tex: 0, blend, pos: [[0.0; 2]; 4], uv: [[0.0; 2]; 4], color: [1.0, 1.0, 1.0, alpha], clip: None };
        let mut quads = [make(0, 0.5), make(3, 0.6)];
        fade_quads(&mut quads, 1.0);
        assert_eq!((quads[0].blend, quads[0].color[3]), (0, 0.5), "disabled fade is unchanged");
        fade_quads(&mut quads, 0.5);
        assert_eq!((quads[0].blend, quads[0].color[3]), (3, 0.5), "opaque fills really blend");
        assert!((quads[1].color[3] - 0.3).abs() < 0.0001, "translucent text or panel fades too");
    }

    /// A frame `dt` seconds on.
    fn frame(c: &mut WndChat, dt: f32) {
        c.draw(&vui(), &mut Rows, 0, dt);
    }

    /// Chat Always Shown (ticket 1914): two lines 30 s old (past `Age`'s 20)
    /// are both still drawn, the box open at its five rows; the box with no
    /// lines at all stays open too. The control: the game's box (off) shows
    /// neither line and slides shut.
    #[test]
    fn chat_always_shown_never_collapses() {
        let (v, mut r) = (vui(), Rows);
        let mut run = |always: bool, lines: usize| {
            let mut c = WndChat::new();
            c.always = always;
            for k in 0..lines {
                c.text.add_text(&crate::ui::wide(&format!("line {k}")), [1.0; 4], 0, width);
            }
            for _ in 0..40 {
                c.draw(&v, &mut r, 30_000, 0.1);
            }
            c
        };
        let on = run(true, 2);
        assert_eq!((on.shown.len(), on.lines, on.height), (2, 5, 85), "both old lines in the full box");
        assert!(on.window.is_open());
        let empty = run(true, 0);
        assert!(empty.window.is_open() && empty.height == 85, "open with nothing said yet");
        let off = run(false, 2);
        assert_eq!((off.shown.len(), off.lines), (0, 0), "the control: the game's box let them go");
        assert!(!off.window.is_open() && off.height < 0, "the control: slid shut");
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

    #[test]
    fn chat_history_cycles_main_and_panes_in_screen_order() {
        let mut c = with_lines(20);
        let mut routes = [0; KINDS];
        routes[Kind::Tells.index()] = 1;
        routes[Kind::Loot.index()] = 2;
        routes[Kind::Combat.index()] = 3;
        c.set_tabs(routes, [['\0'; TAB_NAME_MOST]; TABS]);
        assert!(c.set_detached(1, true));
        assert!(c.set_detached(3, true));
        let x = |pane, _| [40.0, 130.0, 0.0, 10.0][pane];
        c.pane_mut(3).unwrap().size.rows = 7;
        for t in [1, 3] {
            let pane = c.pane_mut(t).unwrap();
            for i in 0..20 {
                pane.text.add_text(&crate::ui::wide(&format!("pane {t} line {i}")), [1.0; 4], 0, width);
            }
        }
        assert!(c.show_tab(2));
        c.begin_history();
        assert_eq!(c.tab(), 0, "each hold starts at Main");
        c.key_down(19);
        c.cycle_history(x);
        assert_eq!(c.held, 0, "switching stops the old pane's repeat");
        assert_eq!(c.history_pane(), Some(3), "the leftmost pane is next");
        c.history_mut().page_history(true);
        let pane = c.pane(3).unwrap();
        assert_eq!(pane.text.index(pane.bottom), 13, "page uses that pane's seven lines");
        c.history_mut().key_down(19);
        let pane = c.pane(3).unwrap();
        assert_eq!(pane.text.index(pane.bottom), 12, "up scrolls just one line in the same pane");
        assert_eq!(c.pane(1).unwrap().scroll, 0, "other detached panes are independent");
        c.history_mut().page_history(true);
        c.history_mut().page_history(true);
        let pane = c.pane(3).unwrap();
        assert_eq!(pane.top, pane.text.first(), "page up clamps at the oldest row");
        c.history_mut().page_history(false);
        let pane = c.pane(3).unwrap();
        assert_eq!(pane.text.index(pane.bottom), 14);
        c.history_mut().page_history(false);
        assert_eq!(c.pane(3).unwrap().bottom, c.pane(3).unwrap().text.end(), "page down clamps at the newest row");
        c.cycle_history(x);
        assert_eq!((c.tab(), c.history_pane()), (2, None), "the docked tab is next");
        c.cycle_history(x);
        assert_eq!(c.history_pane(), Some(1), "the rightmost pane is last");
        c.cycle_history(x);
        assert_eq!((c.tab(), c.history_pane()), (0, None), "wraps to Main");
        c.cycle_history(x);
        c.end_history();
        assert_eq!(c.pane(3).unwrap().scroll, 0, "releasing chat leaves the detached pane live");
    }

    #[test]
    fn chat_history_wheel_resumes_live_at_the_bottom() {
        let mut c = with_lines(20);
        let p = on(&c, 19);
        assert!(c.pc_wheel(p, -1.0));
        assert_eq!(c.scroll, 0, "scrolling down at the live edge stays live");
        assert!(c.pc_wheel(p, 0.5));
        assert_eq!(c.scroll, 0, "a partial step leaves the scrollbar hidden");
        assert!(c.pc_wheel(p, 1.5));
        assert_eq!(c.text.index(c.bottom), 18);
        assert!(c.pc_wheel(p, -1.0));
        assert!(c.scroll_indicator(true), "scrollback stays pinned off the bottom");
        assert!(c.pc_wheel(p, -1.0));
        assert!(!c.scroll_indicator(true), "the wheel releases the indicator at the bottom");
        c.text.add_text(&crate::ui::wide("new live line"), [1.0; 4], 0, width);
        frame(&mut c, 0.016);
        assert_eq!(c.shown.first().unwrap().seq, 20, "new chat is visible immediately");

        c.begin_history();
        let outside = [-300.0, -300.0];
        assert!(c.wheel(outside, 3.0, true));
        assert!(c.wheel(outside, -4.0, true));
        assert_eq!(c.scroll, 0);
        assert!(c.fixed(), "reaching the bottom keeps the explicitly held pane expanded");
        assert!(c.key_down(19), "up can scroll again without releasing the expand key");
        assert_eq!(c.text.index(c.bottom), 20);
    }

    #[test]
    fn pc_history_indicator_requires_more_wrapped_lines_than_visible_rows() {
        let mut c = with_lines(5);
        c.set_scroll(1);
        assert!(!c.scroll_indicator(true), "five rows fit the five-row PC pane");
        assert!(c.scroll_indicator(false), "controller scroll view keeps its indicator");
        c.text.add_text(&crate::ui::wide("one more line"), [1.0; 4], 0, width);
        assert!(c.scroll_indicator(true), "extra wrapped history shows the PC indicator");
        c.set_scroll(0);
        assert!(!c.scroll_indicator(true), "the indicator is only in scroll view");
    }

    /// What a row's runs were drawn as: the words and the colour, in order.
    struct Runs(Vec<(String, [f32; 4])>);

    impl crate::ui::Layout for Runs {}
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
