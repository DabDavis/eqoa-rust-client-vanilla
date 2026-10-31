//! A server's player search, in the window Sandstorm's S2 draws it in
//! (`s2`, `playersearch`). The Group menu's Search opens it over the game:
//! a `VIUIWindow` 608 wide, titled by S2, that while it is open takes every
//! key `VIWndGame` and the HUD's menus would have (S2 puts its handler in
//! both vtables), holds the player and the camera still (`m_p06DC0` and
//! `m_p06DC4` cleared every frame) and hides the HUD's menus.
//!
//! Its left column is the search: a race and a class (any, or one of ten or
//! fifteen), the lowest and highest level, and Search, which sends 0x1502;
//! it opens on the player's own range (`levels`, Bird's report F3
//! 20261023-194630: the stand-in asked for every level) and on the race and
//! class it was left at. On its right, the list the server answers (0x1501
//! empties it, each 0x1500 adds a player), eight rows of name, race, class
//! and level at a time, sorted by a column (L1 and R1 go round them); for
//! the player selected, a Location line (the zone and the world) and a Group
//! line (its size and the player's note). ✕ there is a tell (the chat line
//! begun with a tell to them), □ an invite, △ or Escape closes the window
//! and lets the list go.
//!
//! Nothing of S2 is in this file. Its words and tables (the title, the
//! headings, the legends, the labels, the races' and classes' names and
//! short names, the levels as it writes them, the Location, Group and tell
//! lines' formats, the tick's rows) are read out of the patched image where
//! its code names them (`Art::read`); a build whose code is not this shape
//! has none of them, and the search goes to the game's who window instead.
//!
//! The client's own: the HUD stays drawn under it (S2 skips the HUD's
//! drawing while it is open and covers the screen with the green backdrop;
//! here the world shows round it and the compass stays, as round Start's
//! windows: `ui::window_backdrop`), and a pointer works on it (`point`).

use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::s2::{CLIENT_BASE, Image};
use std::sync::Arc;
use vi::fpu::{Ee, F};
use vi::ui::{Raster, UI_COLORS, Vui, Window};

/// Where S2 hooks what the window is made of, and where in its code each
/// word and table is named (a lui and its addiu or ori, and the register
/// they build), as the live S2 (ss-patch-60, 2026-10-15) has them.
mod at {
    // CLIENT, by offset from its base.
    /// `VIWndGame::OnDrawSelf`+0x64: jal to the window's frame
    pub const ON_DRAW_SELF: u32 = 0x9C23C;
    /// `VIWndGame`'s vtable, its `OnKeyDown` slot: S2's trampoline to the
    /// window's keys
    pub const GAME_ON_KEY_DOWN: u32 = 0x133FC4;
    /// the head of `ProcessMessage`'s opcode switch: jal to the 0x1500 and
    /// 0x1501 handler (`s2::at::CLIENT_OPCODE_SWITCH`)
    pub const OPCODE_SWITCH: u32 = 0x1C4A8;

    // In the frame (the hook's cave).
    /// `SetTitle(a1)`: lui at +0xE8, the addiu in its delay slot
    pub const TITLE: (u32, u32) = (0xE8, 0xF0);
    /// jal the list, then jal the filter
    pub const LIST: u32 = 0x104;
    pub const FILTER: u32 = 0x110;

    // In the list's function.
    /// each column's heading, as it is and as the one sorted by
    pub const HEADS: [[(u32, u32); 2]; 4] = [[(0xE4, 0xE8), (0xF4, 0xF8)], [(0x124, 0x128), (0x134, 0x138)], [(0x164, 0x168), (0x174, 0x178)], [(0x1A4, 0x1A8), (0x1B4, 0x1B8)]];
    /// the two legends' table (t9)
    pub const LEGENDS: (u32, u32) = (0x1F0, 0x1F4);
    /// jal the Location line, then jal the Group line
    pub const LOCATION: u32 = 0x228;
    pub const GROUP: u32 = 0x234;

    // In the filter's function.
    /// the labels: race, class, the two levels, Search (a1)
    pub const LABELS: [(u32, u32); 5] = [(0x20, 0x24), (0xA8, 0xAC), (0x130, 0x134), (0x1A4, 0x1A8), (0x218, 0x21C)];
    /// a race or a class that is any (a1)
    pub const ANY: (u32, u32) = (0x58, 0x5C);
    /// the races' and the classes' names, the numbers (t1)
    pub const RACES: (u32, u32) = (0x74, 0x78);
    pub const CLASSES: (u32, u32) = (0xFC, 0x100);
    pub const NUMBERS: (u32, u32) = (0x170, 0x174);
    /// the tick's row for each of the filter's (t1)
    pub const TICK_ROWS: (u32, u32) = (0x248, 0x24C);
    /// after the count (a1)
    pub const FOUND: (u32, u32) = (0x2C4, 0x2C8);

    // The Location and Group lines' functions: the two formats each (a2).
    pub const LOCATION_FORMATS: [(u32, u32); 2] = [(0x114, 0x118), (0x13C, 0x140)];
    pub const GROUP_FORMATS: [(u32, u32); 2] = [(0x80, 0x84), (0xA8, 0xAC)];
    /// the group's sizes' table (t1)
    pub const SIZES: (u32, u32) = (0x58, 0x5C);

    // The keys' function (past the trampoline's lui): jal the tell; in the
    // tell's, its format (a2).
    pub const TELL: u32 = 0x13C;
    pub const TELL_FORMAT: (u32, u32) = (0x28, 0x2C);

    // The message handler: the short names of the races and classes and
    // the numbers again, which a player found points into (t2).
    pub const RACE_SHORT: (u32, u32) = (0x1DC, 0x1E0);
    pub const CLASS_SHORT: (u32, u32) = (0x1FC, 0x200);
}

/// The registers the pairs build into.
const A1: u32 = 5;
const A2: u32 = 6;
const T1: u32 = 9;
const T2: u32 = 10;
const T9: u32 = 25;

/// The window's words and tables, as S2 carries them (read out of the
/// patched image: none of it is in this repository).
#[derive(Clone, Debug, PartialEq)]
pub struct Art {
    /// `SetTitle`'s
    pub title: Vec<u16>,
    /// the columns' headings (name, race, class, level): as they are, and
    /// marked as the one sorted by
    pub heads: [[Vec<u16>; 2]; 4],
    /// the legend under the list: while the filter has the keys, while the
    /// list has them and has a player
    pub legends: [Vec<u16>; 2],
    /// the filter's labels: race, class, the two levels, Search
    pub labels: [Vec<u16>; 5],
    /// a filter's race or class that is any
    pub any: Vec<u16>,
    /// written after the count
    pub found: Vec<u16>,
    /// the filter's names of the races and the classes (16 each: a value's
    /// low four bits index them)
    pub races: Vec<Vec<u16>>,
    pub classes: Vec<Vec<u16>>,
    /// the list's short names of the same
    pub race_short: Vec<Vec<u16>>,
    pub class_short: Vec<Vec<u16>>,
    /// 0 to 255 as the window writes them (the levels, the count)
    pub numbers: Vec<Vec<u16>>,
    /// the tick's row beside each of the filter's
    pub tick_rows: [i32; 5],
    /// the Location line: of a zone and its world, of the world alone (an
    /// 8-bit format, each `%S` a word)
    pub location: [Vec<u8>; 2],
    /// the Group line: of a size and the player's note, of the size alone
    pub group: [Vec<u8>; 2],
    /// a group's size as the Group line writes it (0 none .. 4)
    pub sizes: Vec<Vec<u16>>,
    /// the tell's chat line, of the name
    pub tell: Vec<u8>,
}

/// An 8-bit format with exactly `n` `%S` and no other conversion: the only
/// kind `format` writes.
fn format_of(img: &Image, a: u32, n: usize) -> Option<Vec<u8>> {
    let s = img.cstring(a)?.into_bytes();
    let mut k = 0;
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%' {
            match s.get(i + 1) {
                Some(b'S') => k += 1,
                Some(b'%') => {}
                _ => return None,
            }
            i += 1;
        }
        i += 1;
    }
    (k == n).then_some(s)
}

impl Art {
    /// The window's words and tables out of an S2 applied at delta 0,
    /// following its hooks to the functions that draw and handle the window
    /// and reading each where the code names it; `None` unless every one is
    /// there and of its shape.
    pub fn read(img: &Image) -> Option<Art> {
        let cave = crate::s2::in_cave;
        let call = |a: u32| img.call_target(a).filter(|&t| cave(t));
        let pair = |f: u32, (hi, lo): (u32, u32), r: u32| img.address(f + hi, f + lo, r).filter(|&a| cave(a));
        let text = |f: u32, p: (u32, u32)| img.wstring(pair(f, p, A1)?);
        let table = |f: u32, p: (u32, u32), r: u32, n: u32| -> Option<Vec<Vec<u16>>> {
            let t = pair(f, p, r)?;
            (0..n).map(|i| img.word(t + 4 * i).filter(|&a| cave(a)).and_then(|a| img.wstring(a))).collect()
        };
        let draw = call(CLIENT_BASE + at::ON_DRAW_SELF)?;
        let list = call(draw + at::LIST)?;
        let filter = call(draw + at::FILTER)?;
        let location = call(list + at::LOCATION)?;
        let group = call(list + at::GROUP)?;
        let handler = call(CLIENT_BASE + at::OPCODE_SWITCH)?;
        // the vtable's slot holds S2's trampoline: lui t9, then j the keys
        let trampoline = img.word(CLIENT_BASE + at::GAME_ON_KEY_DOWN).filter(|&a| cave(a))?;
        let keys = call(trampoline + 4)?;
        let tell = call(keys + at::TELL)?;
        let mut heads: [[Vec<u16>; 2]; 4] = Default::default();
        for (k, [plain, sorted]) in at::HEADS.into_iter().enumerate() {
            heads[k] = [text(list, plain)?, text(list, sorted)?];
        }
        let mut labels: [Vec<u16>; 5] = Default::default();
        for (k, p) in at::LABELS.into_iter().enumerate() {
            labels[k] = text(filter, p)?;
        }
        let legends = table(list, at::LEGENDS, T9, 2)?;
        let rows = pair(filter, at::TICK_ROWS, T1)?;
        let tick_rows: Option<Vec<i32>> = (0..5).map(|i| img.word(rows + 4 * i).map(|w| w as i32).filter(|r| (0..14).contains(r))).collect();
        let location_formats = [format_of(img, pair(location, at::LOCATION_FORMATS[0], A2)?, 2)?, format_of(img, pair(location, at::LOCATION_FORMATS[1], A2)?, 1)?];
        let group_formats = [format_of(img, pair(group, at::GROUP_FORMATS[0], A2)?, 2)?, format_of(img, pair(group, at::GROUP_FORMATS[1], A2)?, 1)?];
        Some(Art {
            title: img.wstring(img.address(draw + at::TITLE.0, draw + at::TITLE.1, A1).filter(|&a| cave(a))?)?,
            heads,
            legends: legends.try_into().ok()?,
            labels,
            any: text(filter, at::ANY)?,
            found: text(filter, at::FOUND)?,
            races: table(filter, at::RACES, T1, 16)?,
            classes: table(filter, at::CLASSES, T1, 16)?,
            race_short: table(handler, at::RACE_SHORT, T2, 16)?,
            class_short: table(handler, at::CLASS_SHORT, T2, 16)?,
            numbers: table(filter, at::NUMBERS, T1, 256)?,
            tick_rows: tick_rows?.try_into().ok()?,
            location: location_formats,
            group: group_formats,
            sizes: table(group, at::SIZES, T1, 5)?,
            tell: format_of(img, pair(tell, at::TELL_FORMAT, A2)?, 1)?,
        })
    }

    /// The window's words, when this session's patch has them.
    pub fn current() -> Option<Arc<Art>> {
        crate::s2::get().image.as_deref().and_then(Art::read).map(Arc::new)
    }
}

/// `VIString::snprintf16(buf, bytes, format, ...)` for the `%S` formats
/// `Art` keeps: each `%S` the next word (to its first NUL), `%%` a `%`,
/// at most `bytes / 2 - 1` characters.
pub fn format(f: &[u8], args: &[&[u16]], bytes: usize) -> Vec<u16> {
    let cap = (bytes / 2).saturating_sub(1);
    let mut out = Vec::new();
    let mut args = args.iter();
    let mut i = 0;
    while out.len() < cap && i < f.len() {
        if f[i] == b'%' && i + 1 < f.len() {
            match f[i + 1] {
                b'S' => {
                    let s = args.next().copied().unwrap_or(&[]);
                    for &c in s.iter().take_while(|&&c| c != 0) {
                        if out.len() < cap {
                            out.push(c);
                        }
                    }
                }
                b'%' => out.push(b'%' as u16),
                _ => {}
            }
            i += 2;
        } else {
            out.push(f[i] as u16);
            i += 1;
        }
    }
    out
}

/// The level range a search opens on, of the player's level (S2's
/// FD070, every open): from about three quarters of it (no more than three
/// under it, at least 1) to a third over it (at least three over, at most
/// 60).
pub fn levels(level: i32) -> (i32, i32) {
    let l = level.max(1);
    let min = (((3 * l + 3) as u32) >> 2) as i32;
    let min = min.min(l - 3).max(1);
    let max = ((4 * l) as u32 / 3) as i32;
    let max = max.max(l + 3).min(60);
    (min, max)
}

/// A player found: 0x1500's body as S2's handler keeps it (at least three
/// bytes; each later field only when the body reaches it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    /// the player's id, 0 when the body stops before it (19 bytes or more)
    pub id: u32,
    /// the race, the class (each's low four bits name it) and the level
    pub race: u8,
    pub class: u8,
    pub level: u8,
    /// up to 12 characters, to a NUL
    pub name: Vec<u16>,
    /// the group's size, 0xFF absent (23 bytes or more)
    pub group: u8,
    /// 0x01000000 + world << 16 + column << 8 + row, 0 absent (22 bytes or
    /// more)
    pub place: u32,
    /// the player's note: up to 71 bytes, with no NUL check (a NUL ends it
    /// where it is written)
    pub note: Vec<u16>,
}

impl Entry {
    pub fn parse(b: &[u8]) -> Option<Entry> {
        let n = b.len();
        if n < 3 {
            return None;
        }
        let mut e = Entry { race: b[0], class: b[1], level: b[2], group: 0xFF, ..Default::default() };
        if n >= 19 {
            e.id = u32::from_le_bytes(b[15..19].try_into().unwrap());
        }
        if n >= 22 {
            e.place = 0x0100_0000 + ((b[19] as u32) << 16) + ((b[20] as u32) << 8) + b[21] as u32;
        }
        if n >= 23 {
            e.group = b[22];
            e.note = b[23..n.min(23 + 71)].iter().map(|&c| c as u16).collect();
        }
        e.name = b[3..n.min(15)].iter().take_while(|&&c| c != 0).map(|&c| c as u16).collect();
        Some(e)
    }

    /// The name, as the invite's record carries it (each character's low
    /// byte).
    pub fn name_string(&self) -> String {
        self.name.iter().map(|&c| c as u8 as char).collect()
    }
}

/// `EQOA_SEARCH_FOUND=<n>`'s players (a test's, for a server that does not
/// answer 0x1502): n 0x1500 bodies, the races, classes and levels in turn,
/// some grouped, every fourth with a note, the fourth sent without its id,
/// place or group.
pub fn test_players(n: usize) -> Vec<Vec<u8>> {
    const NAMES: [&str; 12] = ["Ashwind", "Brimble", "Corva", "Dunmore", "Elsabeth", "Fenwick", "Grisel", "Hollis", "Ivra", "Jorund", "Kestrel", "Lumara"];
    (0..n)
        .map(|i| {
            let mut b = vec![(i % 10) as u8, (i * 7 % 15) as u8, (10 + i * 5 % 51) as u8];
            let mut name = NAMES[i % NAMES.len()].as_bytes().to_vec();
            name.resize(12, 0);
            b.extend(name);
            if i == 3 {
                return b;
            }
            b.extend((0x1000 + i as u32).to_le_bytes());
            b.extend([0, (3 + i % 9) as u8, (2 + i % 12) as u8]);
            b.push((i % 5) as u8);
            if i % 4 == 1 {
                b.extend(b"LFG, any zone");
            }
            b
        })
        .collect()
}

/// A string as the window keeps it: to its first NUL.
fn cut(s: &[u16]) -> &[u16] {
    &s[..s.iter().position(|&c| c == 0).unwrap_or(s.len())]
}

/// FE3A0: two strings compared a character at a time (64 at most): the
/// first difference, 0 when they end together.
fn compare(a: &[u16], b: &[u16]) -> i32 {
    for i in 0..64 {
        let (x, y) = (a.get(i).copied().unwrap_or(0), b.get(i).copied().unwrap_or(0));
        if x != y {
            return x as i32 - y as i32;
        }
        if x == 0 {
            return 0;
        }
    }
    0
}

/// FE400: the digits a number starts with, as a number.
fn number(s: &[u16]) -> i32 {
    let mut v: i32 = 0;
    for &c in s {
        if !(48..58).contains(&c) {
            break;
        }
        v = v.wrapping_mul(10).wrapping_add(c as i32 - 48);
    }
    v
}

/// What the window asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// FD178: the window closed (the HUD's menu shown again, the player and
    /// the camera let go)
    Close,
    /// the search, 0x1502
    Send(Vec<u8>),
    /// the window closed, then `VIWndGame::OnKeyDown(84)`'s chat line with
    /// this in it
    Tell(Vec<u16>),
    /// `InviteToGroup` by the player's id and name, then the window closed
    Invite(u32, String),
}

/// What the pointer's hits stand for: a player's row by its index, a
/// heading by `HEADING - column`, and a filter's row by its index plus
/// `ROW`, its value's left half plus `LESS` and its right half plus `MORE`.
const HEADING: i32 = -10;
const ROW: i32 = 1000;
const LESS: i32 = 2000;
const MORE: i32 = 3000;

/// The window and its search (S2's state at FCBA0..FCBC0 and FA800..FA80C).
#[derive(Clone, Debug)]
pub struct Search {
    /// FCBA0: up
    pub open: bool,
    /// FCBAC: the keys go to the list, not the filter
    list_keys: bool,
    /// FCBB0: the filter's row, 0 the race .. 4 Search
    cursor: i32,
    /// FCBB4 .. FCBC0: the race and the class (255 any), the two levels
    race: i32,
    class: i32,
    min: i32,
    max: i32,
    /// FA810 and FA800: the players found
    list: Vec<Entry>,
    /// FA804, FA808, FA80C: the selection, the first row shown, the column
    /// sorted by
    sel: i32,
    top: i32,
    sort: i32,
    /// FCE00: the window, set up the first time it is drawn (FCB60)
    window: Window,
    made: bool,
    art: Option<Arc<Art>>,
    /// where a pointer finds what was drawn
    hits: Hits,
}

impl Default for Search {
    fn default() -> Self {
        Search {
            open: false,
            list_keys: false,
            cursor: 0,
            race: 255,
            class: 255,
            min: 1,
            max: 60,
            list: Vec::new(),
            sel: 0,
            top: 0,
            sort: 0,
            window: Window::default(),
            made: false,
            art: None,
            hits: Hits::default(),
        }
    }
}

impl Search {
    /// The Group menu's Search (FD004): up, the filter's race row with the
    /// keys, the first row selected, the levels round the player's own; the
    /// race and the class as they were left.
    pub fn open(&mut self, level: i32, art: Arc<Art>) {
        self.open = true;
        self.list_keys = false;
        self.cursor = 0;
        self.sel = 0;
        self.top = 0;
        (self.min, self.max) = levels(level);
        self.art = Some(art);
    }

    /// FD178's own part: down, the list let go.
    pub fn close(&mut self) {
        self.sel = 0;
        self.top = 0;
        self.open = false;
        self.list = Vec::new();
    }

    /// 0x1501: the list emptied (shown or not).
    pub fn clear(&mut self) {
        self.list.clear();
        self.sel = 0;
        self.top = 0;
    }

    /// 0x1500: a player found, added (shown or not).
    pub fn found(&mut self, body: &[u8]) {
        if let Some(e) = Entry::parse(body) {
            self.list.push(e);
        }
    }

    /// The search as it stands: the race, the class and the two levels.
    pub fn filter(&self) -> [i32; 4] {
        [self.race, self.class, self.min, self.max]
    }

    /// FA9E0: the player selected, when the selection is in the list.
    fn selected(&self) -> Option<&Entry> {
        usize::try_from(self.sel).ok().and_then(|i| self.list.get(i))
    }

    /// FD4B8: a key while the window is up.
    pub fn key(&mut self, k: i32) -> Option<Out> {
        match k {
            27 | 132 => return Some(Out::Close),
            140 | 142 => {
                self.sort = (self.sort + if k == 140 { -1 } else { 1 }) & 3;
                self.sel = 0;
                self.top = 0;
            }
            _ if !self.list_keys => return self.filter_key(k),
            134 | 13 => return self.tell(),
            133 => return self.invite(),
            19 | 138 => {
                if self.sel == 0 {
                    self.to_filter();
                } else {
                    self.sel -= 1;
                }
            }
            20 | 139 => {
                if self.sel + 1 != self.list.len() as i32 {
                    self.sel += 1;
                }
            }
            136 => self.to_filter(),
            _ => {}
        }
        None
    }

    /// Back to the filter, on Search.
    fn to_filter(&mut self) {
        self.cursor = 4;
        self.list_keys = false;
    }

    /// FFA80: a key to the filter.
    fn filter_key(&mut self, k: i32) -> Option<Out> {
        match k {
            19 | 138 => {
                if self.cursor != 0 {
                    self.cursor -= 1;
                }
            }
            20 | 139 => {
                if self.cursor != 4 {
                    self.cursor += 1;
                } else {
                    self.to_list();
                }
            }
            136 => self.change(false),
            137 if self.cursor == 4 => self.to_list(),
            137 => self.change(true),
            13 | 134 if self.cursor == 4 => {
                let [race, class, min, max] = self.filter();
                return Some(Out::Send(crate::playersearch::search(race as u8, class as u8, min as u8, max as u8)));
            }
            _ => {}
        }
        None
    }

    /// Past Search into the list, when it has a player: the selection the
    /// first row shown.
    fn to_list(&mut self) {
        if !self.list.is_empty() {
            self.list_keys = true;
            self.sel = self.top;
        }
    }

    /// Left or right on a filter's row: the race or the class round any
    /// and its ten or fifteen, the lowest level up to the highest and down
    /// to 1, the highest up to 60 and down to the lowest.
    fn change(&mut self, up: bool) {
        let round = |v: i32, last: i32| match (up, v) {
            (true, 255) => 0,
            (true, v) if v == last => 255,
            (true, v) => v + 1,
            (false, 255) => last,
            (false, 0) => 255,
            (false, v) => v - 1,
        };
        match self.cursor {
            0 => self.race = round(self.race, 9),
            1 => self.class = round(self.class, 14),
            2 => self.min = if up { (self.min + 1).min(self.max) } else { (self.min - 1).max(1) },
            3 => self.max = if up { (self.max + 1).min(60) } else { (self.max - 1).max(self.min) },
            _ => {}
        }
    }

    /// FAA40: the tell to the player selected.
    fn tell(&self) -> Option<Out> {
        let (art, e) = (self.art.as_ref()?, self.selected()?);
        Some(Out::Tell(format(&art.tell, &[&e.name], 64)))
    }

    /// FAB10: the invite, to a player found with an id.
    fn invite(&self) -> Option<Out> {
        let e = self.selected().filter(|e| e.id != 0)?;
        Some(Out::Invite(e.id, e.name_string()))
    }

    /// FE100: the list in order of the column sorted by, a row moving up
    /// past every one that sorts after it (every frame, before it is drawn).
    fn sort_list(&mut self, art: &Art) {
        let cmp = |a: &Entry, b: &Entry| -> i32 {
            let by = match self.sort {
                1 => compare(&art.race_short[(a.race & 15) as usize], &art.race_short[(b.race & 15) as usize]),
                2 => compare(&art.class_short[(a.class & 15) as usize], &art.class_short[(b.class & 15) as usize]),
                3 => number(&art.numbers[a.level as usize]).wrapping_sub(number(&art.numbers[b.level as usize])),
                _ => 0,
            };
            if by != 0 { by } else { compare(&a.name, &b.name) }
        };
        for i in 1..self.list.len() {
            let mut j = i;
            while j != 0 && cmp(&self.list[j - 1], &self.list[j]) > 0 {
                self.list.swap(j - 1, j);
                j -= 1;
            }
        }
    }

    /// A pointer at frame point `p`: the keys to press. A filter's row or a
    /// player's row is selected by a click (the keys go there), and used by
    /// a second: Search searches, a player's row is its tell (✕); a value's
    /// left or right half steps it down or up at once, its row selected with
    /// it (as the options' sliders); a heading sorts by its column. The
    /// legend's buttons are the game's glyphs (`Play::point`).
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let Some(h) = self.hits.at(p) else { return Vec::new() };
        if h <= HEADING {
            let col = HEADING - h;
            if col == self.sort {
                return Vec::new();
            }
            self.sort = col;
            self.sel = 0;
            self.top = 0;
            return vec![MOVED];
        }
        if h >= ROW {
            let (part, row) = (h / 1000, h % 1000);
            let was = !self.list_keys && self.cursor == row;
            self.list_keys = false;
            self.cursor = row;
            let used = match part {
                2 if row < 4 => Some(key::LEFT),
                3 if row < 4 => Some(key::RIGHT),
                _ if was && row == 4 => Some(key::CROSS),
                _ => None,
            };
            return match (was, used) {
                (_, Some(k)) if was || part != 1 => vec![k],
                _ if was => Vec::new(),
                _ => vec![MOVED],
            };
        }
        if self.list_keys && h == self.sel {
            return vec![key::CROSS];
        }
        self.list_keys = true;
        self.sel = h;
        vec![MOVED]
    }

    /// FCA00's window, once the HUD is drawn under it: S2's backdrop (left
    /// out), the window set up the first time, its frame and title, the
    /// list (FA400) and the filter (FF780).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster) {
        let Some(art) = self.art.clone() else { return };
        // BlitRect(32, (-1, -1), (641, 449), 3, 1): the client leaves it out
        crate::ui::window_backdrop(vui, r, 641);
        if !self.made {
            self.made = true;
            self.window = Window::default();
            self.window.init(vui, r, [320, 212], 608, 14, 0);
            self.window.open(true);
            self.window.set_title(Some(&art.title));
        }
        self.window.draw(vui, r);
        self.hits.clear();
        self.draw_list(vui, r, &art);
        self.draw_filter(vui, r, &art);
    }

    /// FA400: the list's frame and scroll bar, the headings, the legend,
    /// the selected player's Location and Group lines, and up to eight rows
    /// from the first shown (which keeps the selection in sight).
    fn draw_list(&mut self, vui: &Vui, r: &mut dyn Raster, art: &Art) {
        let n = self.list.len() as i32;
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + 8 {
            self.top = self.sel - 7;
        }
        self.sort_list(art);
        let w = &self.window;
        let t = if n - 8 > 0 { (F::<Ee>::from_int(self.top) / F::from_int(n - 8)).0 } else { 0.0 };
        w.draw_frame_vscroll(vui, r, 164, 1, w.size[0] - 192, 8, t);
        // the headings, the column sorted by marked (its mark 12 to the left)
        const COLS: [(i32, i32); 4] = [(184, 110), (298, 84), (386, 100), (490, 92)];
        for (k, &(x, width)) in COLS.iter().enumerate() {
            let sorted = self.sort == k as i32;
            w.text_box(vui, r, &art.heads[k][sorted as usize], if sorted { x - 12 } else { x }, 0, width, 1, UI_COLORS[0], 0);
            // its column only (no room for a tick: the one to its left ends there)
            let [x0, y0, x1, y1] = rows_rect(w, vui, r, x, 0, width, 1);
            self.hits.push([x0 + 20, y0, x1, y1], HEADING - k as i32);
        }
        let legend = &art.legends[(self.list_keys && n > 0) as usize];
        w.text_box(vui, r, legend, 30, 13, w.size[0] - 60, 1, UI_COLORS[0], 0);
        self.draw_location(vui, r, art);
        self.draw_group(vui, r, art);
        let mut rows = (n - self.top).min(8);
        let (mut i, mut row) = (self.top, 1);
        while rows > 0 {
            let e = &self.list[i as usize];
            let c = UI_COLORS[(i == self.sel) as usize];
            w.text_box(vui, r, &e.name, 184, row, 110, 1, c, 2);
            w.text_box(vui, r, &art.race_short[(e.race & 15) as usize], 298, row, 84, 1, c, 2);
            w.text_box(vui, r, &art.class_short[(e.class & 15) as usize], 386, row, 100, 1, c, 2);
            w.text_box(vui, r, &art.numbers[e.level as usize], 490, row, 92, 1, c, 2);
            if self.list_keys && i == self.sel {
                w.tick_r(vui, r, 180, row, true);
            }
            self.hits.push(rows_rect(w, vui, r, 184, row, 398, 1), i);
            i += 1;
            row += 1;
            rows -= 1;
            if i == n {
                break;
            }
        }
    }

    /// FBB80: with the list's keys, the selected player's place, when the
    /// server sent it: the zone and its world, or the world alone where
    /// `ZoneName` has no zone (641, or 0), by the world table the patch
    /// gives CLIENT.
    fn draw_location(&self, vui: &Vui, r: &mut dyn Raster, art: &Art) {
        if !self.list_keys {
            return;
        }
        let Some(e) = self.selected().filter(|e| e.place != 0) else { return };
        let world = (e.place >> 16) & 0xFF;
        if world >= 8 {
            return;
        }
        let rules = crate::s2::get();
        let Some(info) = rules.worlds.as_ref().and_then(|t| t.get(world as usize)).filter(|w| w.name_id != 0) else { return };
        let (row, col) = ((e.place & 0xFF) as i32, ((e.place >> 8) & 0xFF) as i32);
        let b = info.bounds;
        let zone = if col < b[0] || b[1] < col || row < b[2] || b[3] < row { 641 } else { crate::zones::zone(world as i32, row, col).unwrap_or(641) };
        let word = |id: i32| vi::ui::wide(vui.words.get(id, vui.language));
        let world_name = word(info.name_id);
        let s = if zone != 641 && zone != 0 { format(&art.location[0], &[&word(zone as i32), &world_name], 224) } else { format(&art.location[1], &[&world_name], 224) };
        self.window.text_box(vui, r, &s, 184, 10, 396, 1, UI_COLORS[0], 0);
    }

    /// FBEA0: with the list's keys, the selected player's group: its size
    /// (none, 1 .. 4) and their note when they wrote one; nothing for a size
    /// past 4 (0xFF, a player sent without it, included).
    fn draw_group(&self, vui: &Vui, r: &mut dyn Raster, art: &Art) {
        if !self.list_keys {
            return;
        }
        let Some(e) = self.selected().filter(|e| e.group < 5) else { return };
        let size = &art.sizes[e.group as usize];
        let note = cut(&e.note);
        let s = if !note.is_empty() { format(&art.group[0], &[size, note], 224) } else { format(&art.group[1], &[size], 224) };
        self.window.text_box(vui, r, &s, 184, 11, 396, 1, UI_COLORS[0], 0);
    }

    /// FF780: the filter's column (each row's label, its value framed under
    /// or beside it, Search, the tick on the row with the keys) and the
    /// count found.
    fn draw_filter(&mut self, vui: &Vui, r: &mut dyn Raster, art: &Art) {
        let w = &self.window;
        let c0 = UI_COLORS[0];
        let name = |v: i32, names: &[Vec<u16>]| if v == 255 { art.any.clone() } else { names[(v & 15) as usize].clone() };
        w.text(vui, r, &art.labels[0], 40, 1, c0, 0);
        w.draw_frame(vui, r, 40, 2, 104, 1, 0x40);
        w.text_box(vui, r, &name(self.race, &art.races), 46, 2, 94, 1, c0, 0);
        w.text(vui, r, &art.labels[1], 40, 3, c0, 0);
        w.draw_frame(vui, r, 40, 4, 104, 1, 0x40);
        w.text_box(vui, r, &name(self.class, &art.classes), 46, 4, 94, 1, c0, 0);
        w.text(vui, r, &art.labels[2], 40, 5, c0, 0);
        w.draw_frame(vui, r, 108, 5, 40, 1, 0x40);
        w.text_box(vui, r, &art.numbers[self.min.clamp(0, 255) as usize], 112, 5, 34, 1, c0, 0);
        w.text(vui, r, &art.labels[3], 40, 6, c0, 0);
        w.draw_frame(vui, r, 108, 6, 40, 1, 0x40);
        w.text_box(vui, r, &art.numbers[self.max.clamp(0, 255) as usize], 112, 6, 34, 1, c0, 0);
        w.text(vui, r, &art.labels[4], 40, 7, c0, 0);
        if !self.list_keys {
            w.tick_r(vui, r, 34, art.tick_rows[self.cursor.clamp(0, 4) as usize], true);
        }
        let n = self.list.len().min(255);
        w.text_box(vui, r, &art.numbers[n], 406, 13, 40, 1, c0, 0);
        w.text(vui, r, &art.found, 424, 13, c0, 0);
        // the rows, for a pointer: each label with its value, and the
        // values' halves (the race's and class's under their labels, the
        // levels' beside them)
        let half = |x: i32, row: i32, width: i32| {
            let a = rows_rect(w, vui, r, x, row, width / 2, 1);
            let b = rows_rect(w, vui, r, x + width / 2, row, width - width / 2, 1);
            ([a[0] + 20, a[1], a[2], a[3]], [b[0] + 20, b[1], b[2], b[3]])
        };
        for (k, (label, value)) in [(1, Some((40, 2, 104))), (3, Some((40, 4, 104))), (5, Some((108, 5, 40))), (6, Some((108, 6, 40))), (7, None)].into_iter().enumerate() {
            let k = k as i32;
            self.hits.push(rows_rect(w, vui, r, 40, label, 104, 1), ROW + k);
            if let Some((x, row, width)) = value {
                let (less, more) = half(x - 4, row, width + 8);
                self.hits.push(less, LESS + k);
                self.hits.push(more, MORE + k);
            }
        }
    }
}

#[cfg(test)]
pub mod testing {
    /// The live S2 (Sandstorm's, not in the repository: `$EQOA_S2_BUILDS`,
    /// else beside it), parsed; a test without it skips.
    pub fn live() -> Option<crate::s2::Patch> {
        let root = std::env::var_os("EQOA_S2_BUILDS").map(std::path::PathBuf::from).unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.."));
        let path = root.join("extracted-assets/sandstorm-patch-20261015/S2");
        match std::fs::read(&path) {
            Ok(d) => Some(crate::s2::Patch::parse(&d).expect("the stub takes it")),
            Err(_) => {
                eprintln!("SKIP: no live S2 at {}", path.display());
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::live;
    use super::*;
    use crate::pointer::testing::{NullRaster, mid, vui};

    /// Art stood in for a test: every table full, its words the test's own.
    fn art() -> Art {
        let w = vi::ui::wide;
        let list = |n: usize, f: &dyn Fn(usize) -> String| (0..n).map(|i| w(&f(i))).collect::<Vec<_>>();
        Art {
            title: w("T"),
            heads: [[w("Nm"), w("|}Nm|{")], [w("Rc"), w("|}Rc|{")], [w("Cl"), w("|}Cl|{")], [w("Lv"), w("|}Lv|{")]],
            legends: [w("a"), w("b")],
            labels: [w("r"), w("c"), w("lo"), w("hi"), w("go")],
            any: w("*"),
            found: w(" f"),
            races: list(16, &|i| format!("race{i}")),
            classes: list(16, &|i| format!("class{i}")),
            race_short: list(16, &|i| ["H", "E", "D", "G", "W", "T", "B", "F", "U", "O"].get(i).map_or("?", |s| s).to_string()),
            class_short: list(16, &|i| format!("{}", (b'A' + i as u8) as char)),
            numbers: list(256, &|i| i.to_string()),
            tick_rows: [2, 4, 5, 6, 7],
            location: [b"at %S, %S".to_vec(), b"at %S".to_vec()],
            group: [b"g %S - %S".to_vec(), b"g %S".to_vec()],
            sizes: list(5, &|i| if i == 0 { "no".into() } else { i.to_string() }),
            tell: b"tell %S!".to_vec(),
        }
    }

    fn body(race: u8, class: u8, level: u8, name: &str, id: u32) -> Vec<u8> {
        let mut b = vec![race, class, level];
        let mut n = name.as_bytes().to_vec();
        n.resize(12, 0);
        b.extend(n);
        b.extend(id.to_le_bytes());
        b.extend([0, 7, 9, 2]);
        b
    }

    /// S2's own level range (FD070) for the levels round the edges; the
    /// control: the stand-in's 1 to 60 is none of them past level 45.
    #[test]
    fn the_search_opens_on_the_players_own_levels() {
        assert_eq!(levels(1), (1, 4));
        assert_eq!(levels(0), (1, 4), "a level under 1 is 1's");
        assert_eq!(levels(3), (1, 6));
        assert_eq!(levels(10), (7, 13));
        assert_eq!(levels(20), (15, 26));
        assert_eq!(levels(49), (37, 60));
        assert_eq!(levels(50), (38, 60));
        assert_eq!(levels(60), (45, 60), "Bird's level");
        for l in 1..=60 {
            let (lo, hi) = levels(l);
            assert!(1 <= lo && lo <= l && l <= hi && hi <= 60, "{l}: {lo}..{hi}");
        }
        assert_ne!(levels(60), (1, 60), "the control: the stand-in's range");
        // the search sent is the filter's, the levels the player's own
        let mut s = Search::default();
        s.open(60, Arc::new(art()));
        assert_eq!(s.filter(), [255, 255, 45, 60]);
        for _ in 0..4 {
            assert_eq!(s.key(139), None);
        }
        assert_eq!(s.key(134), Some(Out::Send(vec![0x02, 0x15, 0xFF, 0xFF, 45, 60])));
    }

    /// Every word and table, read out of the live S2 where its code names
    /// them, is what that code draws (as the disassembly reads it); the
    /// controls: an image without the patch has none, and one whose title
    /// pair is moved off its register is refused.
    #[test]
    fn the_windows_words_are_read_out_of_the_patch() {
        let Some(patch) = live() else { return };
        let img = patch.apply(0);
        let a = Art::read(&img).expect("the live S2's window");
        let s = |v: &[u16]| String::from_utf16_lossy(v);
        // the window's own (its title only checked for a shape here)
        assert!(!a.title.is_empty() && a.title.iter().all(|&c| (32..127).contains(&c)));
        assert_eq!(a.heads.iter().map(|h| s(&h[0])).collect::<Vec<_>>().len(), 4);
        for h in &a.heads {
            assert_eq!(s(&h[1]), format!("|}}{}|{{", s(&h[0])), "a sorted heading is the heading between the marks");
        }
        assert_eq!(a.tick_rows, [2, 4, 5, 6, 7]);
        assert_eq!(a.numbers.len(), 256);
        assert!(a.numbers.iter().enumerate().all(|(i, n)| s(n) == i.to_string()));
        assert_eq!((a.races.len(), a.classes.len(), a.race_short.len(), a.class_short.len(), a.sizes.len()), (16, 16, 16, 16, 5));
        assert_eq!(a.location[0].iter().filter(|&&c| c == b'%').count(), 2);
        assert_eq!(a.tell.iter().filter(|&&c| c == b'%').count(), 1);
        // the race and class names line up with the game's own order: ten
        // races and fifteen classes, the rest one mark
        assert!(a.races[10..].iter().all(|r| *r == a.races[10]) && a.races[9] != a.races[10]);
        assert!(a.classes[15] != a.classes[14]);
        assert!(Art::read(&Image::default()).is_none(), "the control: no patch, no window");
        let mut moved = patch.clone();
        let draw = img.call_target(CLIENT_BASE + at::ON_DRAW_SELF).unwrap();
        // the title's addiu into a2 instead of a1: not the code read here
        for rec in &mut moved.records {
            let base = rec.addr & 0x7FFF_FFFF;
            let o = (draw + at::TITLE.1).wrapping_sub(base) as usize;
            if rec.addr & 0x8000_0000 != 0 && o + 4 <= rec.data.len() {
                let w = u32::from_le_bytes(rec.data[o..o + 4].try_into().unwrap());
                rec.data[o..o + 4].copy_from_slice(&((w & !(31 << 16)) | (6 << 16)).to_le_bytes());
            }
        }
        assert!(Art::read(&moved.apply(0)).is_none(), "the control: a shape it does not know");
    }

    /// 0x1500 kept as S2 keeps it (the fields each by the body's length),
    /// and the list sorted by a column every frame, a row moving up past
    /// each that sorts after it: names, the short races and classes as the
    /// window spells them, levels as numbers (10 after 9), a tie by name.
    /// The control: the order the server sent is not the names' order.
    #[test]
    fn the_list_sorts_by_its_column() {
        let full = body(0x13, 0x0C, 49, "Dolph", 0x15C0C);
        let e = Entry::parse(&full).unwrap();
        assert_eq!((e.race, e.class, e.level, e.id, e.place, e.group), (0x13, 0x0C, 49, 0x15C0C, 0x0100_0709, 2));
        assert_eq!(String::from_utf16_lossy(&e.name), "Dolph");
        let short = Entry::parse(&full[..10]).unwrap();
        assert_eq!((short.id, short.place, short.group, short.note.len()), (0, 0, 0xFF, 0));
        assert_eq!(Entry::parse(&[1, 2]), None);
        let mut noted = full.clone();
        noted.extend(b"lfg\0x");
        assert_eq!(cut(&Entry::parse(&noted).unwrap().note), vi::ui::wide("lfg").as_slice(), "a NUL ends the note where it is drawn");
        let a = art();
        let mut s = Search::default();
        s.open(20, Arc::new(a.clone()));
        for b in [body(9, 3, 9, "zed", 1), body(0, 14, 10, "amy", 2), body(0, 1, 9, "Bob", 3)] {
            s.found(&b);
        }
        let names = |s: &Search| s.list.iter().map(|e| String::from_utf16_lossy(&e.name)).collect::<Vec<_>>();
        assert_eq!(names(&s), ["zed", "amy", "Bob"], "the control: as sent");
        s.sort_list(&a);
        assert_eq!(names(&s), ["Bob", "amy", "zed"], "by name, as the characters compare (capitals first)");
        s.sort = 1;
        s.sort_list(&a);
        assert_eq!(names(&s), ["Bob", "amy", "zed"], "by race: H, H (a tie by name), O");
        s.sort = 2;
        s.sort_list(&a);
        assert_eq!(names(&s), ["Bob", "zed", "amy"], "by class: B, D, O");
        s.sort = 3;
        s.sort_list(&a);
        assert_eq!(names(&s), ["Bob", "zed", "amy"], "by level: 9, 9, 10");
        // R1 and L1 go round the columns, the selection back at the top
        s.sel = 2;
        assert_eq!(s.key(142), None);
        assert_eq!((s.sort, s.sel, s.top), (0, 0, 0));
        assert_eq!(s.key(140), None);
        assert_eq!(s.sort, 3);
        // 0x1501 empties it
        s.clear();
        assert!(s.list.is_empty());
    }

    /// The filter's keys as FFA80 has them: up and down along its rows,
    /// left and right through a race and a class round "any", the levels
    /// held to each other and to 1 and 60; past Search into the list only
    /// when it has a player. The list's: ✕ the tell, □ the invite (none for
    /// a player sent without an id), up past the first row and left back
    /// to Search, △ closes. The control: with the list empty, down from
    /// Search stays on the filter.
    #[test]
    fn the_keys_go_as_the_patch_takes_them() {
        let mut s = Search::default();
        s.open(1, Arc::new(art()));
        assert_eq!(s.filter(), [255, 255, 1, 4]);
        s.key(137);
        assert_eq!(s.race, 0);
        s.key(136);
        s.key(136);
        assert_eq!(s.race, 9, "left from any is the last race");
        s.key(137);
        assert_eq!(s.race, 255, "right past the last is any");
        s.key(139);
        s.key(136);
        assert_eq!(s.class, 14);
        s.key(139);
        s.key(137);
        s.key(137);
        s.key(137);
        s.key(137);
        assert_eq!(s.min, 4, "the lowest up to the highest");
        s.key(139);
        s.key(136);
        assert_eq!(s.max, 4, "the highest down to the lowest");
        for _ in 0..70 {
            s.key(137);
        }
        assert_eq!(s.max, 60);
        s.key(138);
        for _ in 0..10 {
            s.key(136);
        }
        assert_eq!(s.min, 1);
        s.key(139);
        s.key(139);
        assert_eq!(s.cursor, 4);
        s.key(139);
        assert!(!s.list_keys, "the control: no player, no list");
        s.found(&body(0, 0, 5, "Ann", 0));
        s.found(&body(1, 2, 7, "Cy", 77));
        s.key(139);
        assert!(s.list_keys && s.sel == 0);
        assert_eq!(s.key(133), None, "no id, no invite");
        assert_eq!(s.key(134), Some(Out::Tell(vi::ui::wide("tell Ann!"))));
        s.key(139);
        assert_eq!(s.key(133), Some(Out::Invite(77, "Cy".into())));
        s.key(139);
        assert_eq!(s.sel, 1, "down stops at the last");
        s.key(138);
        s.key(138);
        assert!(!s.list_keys && s.cursor == 4, "up past the first row: Search");
        s.key(137);
        assert!(s.list_keys);
        s.key(136);
        assert!(!s.list_keys && s.cursor == 4, "left: Search");
        assert_eq!(s.key(132), Some(Out::Close));
        s.close();
        assert!(!s.open && s.list.is_empty());
        // opened again: the race and class as left, the levels the player's
        s.race = 3;
        s.open(60, Arc::new(art()));
        assert_eq!(s.filter(), [3, 14, 45, 60]);
    }

    /// The Location and Group lines' words (S2's formats, `%S` a word, the
    /// buffer's length a cap).
    #[test]
    fn the_lines_are_formatted_as_snprintf16_writes_them() {
        let w = vi::ui::wide;
        assert_eq!(format(b"at %S, %S", &[&w("Freeport"), &w("Tunaria")], 224), w("at Freeport, Tunaria"));
        assert_eq!(format(b"100%% %S", &[&w("x\0y")], 224), w("100% x"), "a word to its NUL");
        assert_eq!(format(b"to %S!", &[&w("Abcdefghijkl")], 16), w("to Abcd"), "7 characters in 16 bytes");
    }

    /// A pointer on the window: a filter's row selected by a click, a
    /// value's halves step it once selected, Search searches; a player's
    /// row selected, then its tell; a heading sorts by its column. Off the
    /// window, nothing.
    #[test]
    fn a_pointer_selects_and_uses_what_it_is_on() {
        let vui = vui();
        let mut r = NullRaster;
        let mut s = Search::default();
        s.open(20, Arc::new(art()));
        s.found(&body(0, 0, 21, "Ann", 5));
        s.found(&body(0, 0, 22, "Bea", 6));
        s.draw(&vui, &mut r);
        let w = s.window;
        let filter = |row| mid(rows_rect(&w, &vui, &NullRaster, 40, row, 104, 1));
        // the class's label (row 3): selected
        assert_eq!(s.point(filter(3)), vec![MOVED]);
        assert_eq!(s.cursor, 1);
        assert!(s.point(filter(3)).is_empty(), "selected already: its label is nothing more");
        // the highest level's right half (row 6 at x 108): selected, then stepped
        let more = mid(rows_rect(&w, &vui, &NullRaster, 108 - 4 + 24, 6, 24, 1));
        assert_eq!(s.point(more), vec![key::RIGHT]);
        assert_eq!(s.cursor, 3);
        assert_eq!(s.point(more), vec![key::RIGHT]);
        // Search: selected, then the search
        assert_eq!(s.point(filter(7)), vec![MOVED]);
        assert_eq!(s.point(filter(7)), vec![key::CROSS]);
        // a player's row: selected, then its tell
        let player = |row| mid(rows_rect(&w, &vui, &NullRaster, 184, row, 398, 1));
        assert_eq!(s.point(player(2)), vec![MOVED]);
        assert!(s.list_keys && s.sel == 1);
        assert_eq!(s.point(player(2)), vec![key::CROSS]);
        // the level heading: sorted by it
        let head = mid(rows_rect(&w, &vui, &NullRaster, 490, 0, 92, 1));
        assert_eq!(s.point(head), vec![MOVED]);
        assert_eq!((s.sort, s.sel), (3, 0));
        assert!(s.point(head).is_empty(), "sorted by it already");
        // just left of the race heading (the name column's end): the name's
        let name_end = [(16 + 298 - 6) as f32, head[1]];
        assert_eq!(s.point(name_end), vec![MOVED]);
        assert_eq!(s.sort, 0);
        assert!(s.point([2.0, 2.0]).is_empty(), "off the window");
        // the control: before it is drawn there is nothing to hit
        let mut fresh = Search::default();
        fresh.open(20, Arc::new(art()));
        assert!(fresh.point(filter(3)).is_empty());
    }

    /// What the window draws for the list and the filter, by its strings'
    /// places: the headings at their columns (the sorted one 12 to the
    /// left), the rows from row 1, the count and the words after it on row
    /// 13; the Location and Group lines only with the list's keys. The
    /// control: with the filter's keys there is no Location line.
    #[test]
    fn it_draws_where_the_patch_draws() {
        #[derive(Default)]
        struct Texts(Vec<([i32; 2], String)>);
        impl crate::ui::Layout for Texts {}
        impl Raster for Texts {
            fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
            fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
            fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
            fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
            fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
            fn draw_centered_string(&mut self, at: [i32; 2], _: [f32; 4], s: &[u16], _: u32) {
                self.0.push((at, String::from_utf16_lossy(s)));
            }
            fn string_width(&self, s: &[u16]) -> i32 {
                s.len() as i32 * 8
            }
            fn point_size(&self) -> i32 {
                20
            }
            fn viewport(&self) -> [i32; 4] {
                [0, 0, 640, 448]
            }
            fn set_viewport(&mut self, _: [i32; 4]) {}
        }
        let vui = vui();
        let mut s = Search::default();
        let mut rules = crate::s2::Rules::retail();
        rules.worlds = Some(vec![crate::s2::World { name: "tunaria".into(), esf: "data\\tunaria.esf".into(), name_id: 0x413, bounds: [0, 12, 0, 15] }]);
        crate::s2::with_rules(rules, || {
            s.open(20, Arc::new(art()));
            s.found(&body(0, 2, 21, "Ann", 5));
            let mut r = Texts::default();
            s.draw(&vui, &mut r);
            let at = |r: &Texts, t: &str| r.0.iter().find(|(_, s)| s == t).map(|(a, _)| *a);
            // the window: centre (320, 212), 608 wide, its rows 16 apart
            // under a 24-high title, so x 16 + its x, row k at 105 + 16k
            // sorted by name: its heading marked, the mark 12 wide before
            // it, so the word stays where it stands unmarked
            assert_eq!(at(&r, "Nm"), Some([16 + 184, 105]));
            assert_eq!(at(&r, "Rc"), Some([16 + 298, 105]));
            assert_eq!(at(&r, "Lv"), Some([16 + 490, 105]));
            assert_eq!(at(&r, "Ann"), Some([16 + 184, 121]));
            assert_eq!(at(&r, "C"), Some([16 + 386, 121]), "the class's short name");
            assert_eq!(at(&r, "21"), Some([16 + 490, 121]));
            assert_eq!(at(&r, "1"), Some([16 + 406, 105 + 16 * 13]), "the count");
            assert_eq!(at(&r, " f"), Some([16 + 424, 105 + 16 * 13]));
            assert_eq!(at(&r, "15"), Some([16 + 112, 105 + 16 * 5]), "the lowest level");
            assert_eq!(at(&r, "*"), Some([16 + 46, 105 + 16 * 2]), "any race");
            assert!(r.0.iter().all(|(_, t)| !t.starts_with("at ")), "the control: the filter's keys, no Location");
            s.key(139);
            s.key(139);
            s.key(139);
            s.key(139);
            s.key(139);
            assert!(s.list_keys);
            let mut r = Texts::default();
            s.draw(&vui, &mut r);
            // grid column 7, row 9 of world 0: a zone word, its name the
            // words' (none here, so empty) before the world's
            assert!(r.0.iter().any(|(a, t)| t.starts_with("at ") && *a == [16 + 184, 105 + 16 * 10]), "{:?}", r.0);
            assert!(r.0.iter().any(|(a, t)| t == "g 2" && *a == [16 + 184, 105 + 16 * 11]), "{:?}", r.0);
        });
    }
}
