//! The who list: `VIClientPart38334` (the request 0x0E01 with a serial, the
//! reply 0x0E02, a stale serial dropped), `VIWndWhoList` (ten rows of name,
//! race, class and level in a titled window, "searching..." while the reply
//! is out) and `VIWndOptionList`, the popup a row opens: Tell (the edit box,
//! then `/t name text`), Invite (`InviteToGroup`), Mail (not ported). A
//! pointer (`pointer`) selects a row and opens its popup from the selection;
//! off the popup it closes it.
//!
//! Not the game's: the rows' order. The game shows the list as the server
//! sent it; here it is sorted (`Sort`, F3 20261023-085524 and -090229, the
//! user's choice), by name from the start, and ○ (C on the keyboard) or a
//! click on a column's heading sorts it by that column instead.
//!
//! A server whose patch sorts the list itself (Sandstorm's S2, `PatchSort`)
//! has it sorted as the patch sorts it: L1 and R1, or left and right, go
//! round the columns, the selection keeping its place in the list (the row
//! there now another player's), the heading sorted by marked before and
//! after, and the legend the patch's (F3 20261023-185531). Its words and
//! tables are read out of the patch where its code names them; with none,
//! or a patch of another shape, the client's own sort above. As on a PS2,
//! the patch sorts a list only when its count is not the one it last sorted,
//! so a list sent again with the same count keeps the server's own order
//! (Sandstorm's: CM count first; `WhoList::patch_get_list`).

#![allow(clippy::too_many_arguments)]

use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::s2::{CLIENT_BASE, Image};
use crate::startmenu::KeyFlash;
use net::stream::{Reader, Writer};
use std::sync::Arc;
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// `VIRec32_C035B0`: a row.
#[derive(Clone, Debug, Default)]
pub struct WhoRow {
    pub race: u8,
    pub class: u8,
    pub level: u8,
    pub name: String,
    pub x1c: u32,
}

/// `VIClientPart38334`.
#[derive(Default)]
pub struct Who {
    pub list: Vec<WhoRow>,
    pub pending: bool,
    serial: i32,
    /// one more each list received: a patch's sort (`PatchSort`) tells a new
    /// list from the one it last saw
    pub received: u64,
}

impl Who {
    /// `Open`: `Close`, then the request (0x0E01, 1, the serial).
    pub fn open(&mut self) -> Vec<u8> {
        self.close();
        let mut w = Writer::op(0x0E01);
        w.u8(1).signed(self.serial as i64);
        self.pending = true;
        w.data
    }

    /// `Close`: the list emptied, a new serial for the next request.
    pub fn close(&mut self) {
        self.list.clear();
        self.pending = false;
        self.serial += 1;
    }

    /// `Recv0E02`: a list (1) for the request still out.
    pub fn recv(&mut self, data: &[u8]) {
        let mut r = Reader::new(data);
        if r.u8() != 1 {
            return;
        }
        if r.signed() as i32 != self.serial {
            return;
        }
        let n = r.signed().clamp(0, 4096);
        for _ in 0..n {
            let (race, class, level) = (r.u8(), r.u8(), r.u8());
            let name = String::from_utf8_lossy(&r.string(24)).into_owned();
            let x1c = r.u32();
            if r.error {
                break;
            }
            self.list.push(WhoRow { race, class, level, name, x1c });
        }
        self.pending = false;
        self.received += 1;
    }
}

/// The column the who list is sorted by (the client's own: the game keeps
/// the server's order). Names, races and classes from A to Z as the window
/// spells them, levels from the highest; a tie by name, then as sent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Name,
    Race,
    Class,
    Level,
}

impl Sort {
    /// The columns from the left, as ○ goes round them.
    const ALL: [Sort; 4] = [Sort::Name, Sort::Race, Sort::Class, Sort::Level];

    /// The column's heading (the window's own words).
    pub fn word(self) -> i32 {
        [509, 510, 511, 503][self as usize]
    }

    /// The next column to the right, round to the first.
    pub fn next(self) -> Sort {
        Sort::ALL[(self as usize + 1) % Sort::ALL.len()]
    }

    /// The next column to the left, round to the last.
    pub fn prev(self) -> Sort {
        Sort::ALL[(self as usize + Sort::ALL.len() - 1) % Sort::ALL.len()]
    }
}

/// Where Sandstorm's S2 hooks the who list, and where in its code each
/// word, table and number it uses is named, as the live S2 (ss-patch-60,
/// 2026-10-15) has them.
mod at {
    // CLIENT, by offset from its base.
    /// `VIClientPart38334::GetList`'s first word: j to the patch's sort
    pub const GET_LIST: u32 = 0x4E7A8;
    /// `VIWndWhoList`'s vtable, its `OnKeyDown` slot (the game's
    /// 0x00CA30B8): the patch's keys
    pub const ON_KEY_DOWN: u32 = 0x13D28C;
    /// `OnDrawSelf`, after the headings: jal to the patch's marks
    pub const MARKS: u32 = 0xEE33C;
    /// `OnDrawSelf`, the legend's `snprintf16`: jal to the patch's format
    pub const LEGEND: u32 = 0xEE53C;

    /// In the patch's `GetList`, jal the sort; in the sort, jal the compare.
    pub const SORT: u32 = 0x64;
    pub const COMPARE: u32 = 0x60;
    /// In the compare: the races' and the classes' order (lui and ori into
    /// t3), and `li t5, n`, how many bytes of a name it compares.
    pub const RACE_RANK: (u32, u32) = (0x3C, 0x40);
    pub const CLASS_RANK: (u32, u32) = (0x78, 0x7C);
    pub const NAME_BYTES: u32 = 0xD0;
    /// In the keys: the four `li t1, key`, two a column back, two on, and
    /// the `li t0` of each step (-1, then 1) their branches go to.
    pub const KEYS: [u32; 4] = [0x04, 0x0C, 0x14, 0x1C];
    pub const STEPS: [u32; 2] = [0x30, 0x3C];
    /// In the marks: the tables' base (t9); the lw of the column's x before
    /// the heading and the addiu that moves it; the mark before (a1); the lw
    /// of the x after it; the mark after.
    pub const BASE: (u32, u32) = (0x3C, 0x40);
    pub const BEFORE: u32 = 0x50;
    pub const BEFORE_BY: u32 = 0x54;
    pub const MARK_BEFORE: u32 = 0x58;
    pub const AFTER: u32 = 0x90;
    pub const MARK_AFTER: u32 = 0x94;
    /// In the legend: its format (a2) and the sort's word (t2).
    pub const FORMAT: (u32, u32) = (0x00, 0x04);
    pub const SORT_WORD: (u32, u32) = (0x08, 0x0C);
}

/// The registers the pairs and the immediates name.
const T0: u32 = 8;
const A1: u32 = 5;
const A2: u32 = 6;
const T1: u32 = 9;
const T2: u32 = 10;
const T3: u32 = 11;
const T9: u32 = 25;

/// A server's patch's own sort of the who list (Sandstorm's S2: its
/// `GetList` sorts the list in place, a stable insertion sort, whenever a
/// new one is there or a key has moved the column; its `OnKeyDown` moves the
/// column; its `OnDrawSelf` marks the heading and puts the keys in the
/// legend). The words and tables as the patch carries them (read out of the
/// patched image: none of it is in this repository).
#[derive(Clone, Debug, PartialEq)]
pub struct PatchSort {
    /// the order of a race's and of a class's low four bits (the races and
    /// the classes from A to Z as their short words spell them)
    pub race_rank: [u8; 16],
    pub class_rank: [u8; 16],
    /// how many bytes of two names it compares
    pub name_bytes: usize,
    /// the keys that move the column one back (L1 and left) and one on (R1
    /// and right)
    pub back: [i32; 2],
    pub on: [i32; 2],
    /// the marks drawn before and after the heading sorted by, and where
    /// each column's go (the heading's row, 12 wide)
    pub marks: [Vec<u16>; 2],
    pub before: [i32; 4],
    pub after: [i32; 4],
    /// the legend: its format (each `%S` a word: Select, Exit, Update, and
    /// the sort's) and the sort's word
    pub legend: Vec<u8>,
    pub sort_word: Vec<u16>,
}

impl PatchSort {
    /// Read out of the patched image where the hooks' code names it; None
    /// when a hook is absent or its code is not this shape.
    pub fn read(img: &Image) -> Option<PatchSort> {
        let cave = crate::s2::in_cave;
        let call = |a: u32| img.call_target(a).filter(|&t| cave(t));
        let pair = |f: u32, (hi, lo): (u32, u32), r: u32| img.address(f + hi, f + lo, r).filter(|&a| cave(a));
        // an instruction of `op` from `rs` into `rt`: its immediate
        let imm = |a: u32, op: u32, rs: u32, rt: u32| img.word(a).filter(|&w| w >> 26 == op && (w >> 21) & 31 == rs && (w >> 16) & 31 == rt).map(|w| w as u16 as i16 as i32);
        const LW: u32 = 0x23;
        const ADDIU: u32 = 9;
        let get_list = call(CLIENT_BASE + at::GET_LIST)?;
        let compare = call(call(get_list + at::SORT)? + at::COMPARE)?;
        let rank = |p: (u32, u32)| -> Option<[u8; 16]> { img.bytes(pair(compare, p, T3)?, 16)?.try_into().ok() };
        let race_rank = rank(at::RACE_RANK)?;
        let class_rank = rank(at::CLASS_RANK)?;
        // li t5, n
        let name_bytes = imm(compare + at::NAME_BYTES, ADDIU, 0, 13).filter(|&n| n > 0)? as usize;
        let keys = img.word(CLIENT_BASE + at::ON_KEY_DOWN).filter(|&a| cave(a))?;
        let key = |off: u32| imm(keys + off, ADDIU, 0, T1);
        let (back, on) = ([key(at::KEYS[0])?, key(at::KEYS[1])?], [key(at::KEYS[2])?, key(at::KEYS[3])?]);
        if imm(keys + at::STEPS[0], ADDIU, 0, T0) != Some(-1) || imm(keys + at::STEPS[1], ADDIU, 0, T0) != Some(1) {
            return None;
        }
        let marks_at = call(CLIENT_BASE + at::MARKS)?;
        let base = pair(marks_at, at::BASE, T9)?;
        let column = |off: i32, by: i32| -> Option<[i32; 4]> {
            let v: Option<Vec<i32>> = (0..4).map(|i| img.word(base.wrapping_add(off as u32) + 4 * i).map(|w| w as i32 + by)).collect();
            v?.try_into().ok()
        };
        let before = column(imm(marks_at + at::BEFORE, LW, T1, A2)?, imm(marks_at + at::BEFORE_BY, ADDIU, A2, A2)?)?;
        let after = column(imm(marks_at + at::AFTER, LW, T1, A2)?, 0)?;
        let mark = |off: u32| img.wstring(base.wrapping_add(imm(marks_at + off, ADDIU, T9, A1)? as u32));
        let marks = [mark(at::MARK_BEFORE)?, mark(at::MARK_AFTER)?];
        let legend_at = call(CLIENT_BASE + at::LEGEND)?;
        let legend = img.cstring(pair(legend_at, at::FORMAT, A2)?)?.into_bytes();
        let sort_word = img.wstring(pair(legend_at, at::SORT_WORD, T2)?)?;
        Some(PatchSort { race_rank, class_rank, name_bytes, back, on, marks, before, after, legend, sort_word })
    }

    /// The session's patch's, if it sorts the who list.
    pub fn current() -> Option<Arc<PatchSort>> {
        crate::s2::get().image.as_deref().and_then(PatchSort::read).map(Arc::new)
    }

    /// The rows in the order the patch leaves them, by `sort`: the list as
    /// sent, sorted stably by the column (a race or a class by its rank,
    /// levels from the highest) and then the names, byte by byte up to the
    /// first that differs, a NUL or `name_bytes`.
    pub fn order(&self, sort: Sort, list: &[WhoRow]) -> Vec<usize> {
        self.sorted(sort, list, (0..list.len()).collect())
    }

    /// `order` from the order `v` stands in (S2 sorts its list in place, a
    /// stable insertion sort, so a list it sorted before is sorted again from
    /// there).
    pub fn sorted(&self, sort: Sort, list: &[WhoRow], mut v: Vec<usize>) -> Vec<usize> {
        let name = |e: &WhoRow| {
            let b = e.name.as_bytes();
            b[..b.len().min(self.name_bytes)].iter().copied().take_while(|&c| c != 0).collect::<Vec<u8>>()
        };
        v.sort_by(|&a, &b| {
            let (ea, eb) = (&list[a], &list[b]);
            let by = match sort {
                Sort::Name => std::cmp::Ordering::Equal,
                Sort::Race => self.race_rank[(ea.race & 15) as usize].cmp(&self.race_rank[(eb.race & 15) as usize]),
                Sort::Class => self.class_rank[(ea.class & 15) as usize].cmp(&self.class_rank[(eb.class & 15) as usize]),
                Sort::Level => eb.level.cmp(&ea.level),
            };
            by.then_with(|| name(ea).cmp(&name(eb)))
        });
        v
    }
}

/// A row's race as the window shows it: the short word of races 0 to 9,
/// none for 10 to 20, and the first past them (as `OnDrawSelf` reads it).
fn race_word(race: u8) -> i32 {
    const RACE_SHORT: [i32; 10] = [317, 318, 319, 320, 321, 322, 323, 324, 325, 1191];
    if (race as usize) < 21 { RACE_SHORT.get(race as usize).copied().unwrap_or(-1) } else { 317 }
}

/// A row's class as the window shows it (the first past 14).
fn class_word(class: u8) -> i32 {
    [303, 304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 315, 316, 1190][if class < 15 { class as usize } else { 0 }]
}

/// The rows in the order shown: indices into `list`, by `sort` (`word`
/// spells a race's or class's word).
pub fn order(sort: Sort, list: &[WhoRow], word: &dyn Fn(i32) -> String) -> Vec<usize> {
    let name = |i: usize| list[i].name.to_lowercase();
    let spelled = |id: i32| word(id).to_lowercase();
    let mut v: Vec<usize> = (0..list.len()).collect();
    v.sort_by(|&a, &b| {
        let (ea, eb) = (&list[a], &list[b]);
        let by = match sort {
            Sort::Name => std::cmp::Ordering::Equal,
            Sort::Race => spelled(race_word(ea.race)).cmp(&spelled(race_word(eb.race))),
            Sort::Class => spelled(class_word(ea.class)).cmp(&spelled(class_word(eb.class))),
            Sort::Level => eb.level.cmp(&ea.level),
        };
        by.then_with(|| name(a).cmp(&name(b))).then(a.cmp(&b))
    });
    v
}

/// The hit of a column's heading: `HEADING - column`.
const HEADING: i32 = -10;

/// The client's word for its sort in the legends (no WORDS.TXT entry).
pub const SORT: &str = "Sort";

/// `VIWndOptionList`: choices under a title, the chosen one ticked.
#[derive(Default)]
pub struct OptionList {
    title: Vec<u16>,
    options: Vec<Vec<u16>>,
    center: [i32; 2],
    width: i32,
    rows: i32,
    pub sel: i32,
    pub visible: bool,
    window: Window,
    /// the choices drawn, for a pointer
    hits: Hits,
}

impl OptionList {
    /// A pointer at frame point `p`: `None` off the window; on a choice, it
    /// chosen at once (✕); on the rest of it, nothing.
    pub fn point(&mut self, p: [f32; 2]) -> Option<Option<i32>> {
        match self.hits.at(p)? {
            -1 => Some(None),
            i => {
                self.sel = i;
                Some(Some(key::CROSS))
            }
        }
    }

    /// `Setup(title, options, count, center, width)`.
    pub fn setup(&mut self, vui: &Vui, r: &dyn Raster, title: &[u16], options: Vec<Vec<u16>>, center: [i32; 2], width: i32) {
        self.title = title.to_vec();
        self.center = center;
        let mut w = width.max(300).max(vui.string_width(r, title) + 80);
        for o in &options {
            w = w.max(vui.string_width(r, o) + 80);
        }
        self.width = w;
        self.rows = options.len() as i32 + 3;
        self.options = options;
        self.window.init(vui, r, center, self.width, self.rows, 0);
        self.window.set_title(Some(title));
        self.sel = 0;
    }

    /// `OnMessage` 295: the focus opens it on the first choice.
    pub fn focus(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.close(true);
        self.window.init(vui, r, self.center, self.width, self.rows, 0);
        self.window.open(false);
        let t = self.title.clone();
        self.window.set_title(Some(&t));
        self.sel = 0;
    }

    /// `OnKeyDown`: up/down move the tick, Enter and Triangle close it with
    /// the choice, accepted or not (308).
    pub fn key(&mut self, key: i32, now: i64, cues: &mut Vec<i32>) -> Option<(i32, bool)> {
        match key {
            19 | 138 => {
                if self.sel > 0 {
                    self.sel -= 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            20 | 139 => {
                if self.sel < self.options.len() as i32 - 1 {
                    self.sel += 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            13 | 134 => {
                self.visible = false;
                cues.push(13);
                return Some((self.sel, true));
            }
            27 | 132 => {
                self.visible = false;
                cues.push(8);
                return Some((self.sel, false));
            }
            _ => {}
        }
        None
    }

    /// `OnDrawSelf`.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.window.draw(vui, r);
        self.hits.clear();
        let (tl, br) = (self.window.top_left(), self.window.bottom_right());
        self.hits.push([tl[0], tl[1], br[0], br[1]], -1);
        for (i, o) in self.options.iter().enumerate() {
            self.window.text(vui, r, o, 40, i as i32 + 1, UI_COLORS[(self.sel == i as i32) as usize], 0);
            self.hits.push(rows_rect(&self.window, vui, r, 40, i as i32 + 1, self.width - 80, 1), i as i32);
        }
        self.window.tick_r(vui, r, 36, self.sel + 1, true);
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let s = wide(&format!("|x{}   |t{}", w(212), w(213)));
        self.window.text(vui, r, &s, 40, self.rows - 1, UI_COLORS[0], 0);
    }
}

/// What the who list asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum WhoUp {
    /// `ClosePanel704`: back to the start menu
    Close,
    /// `Refresh`: the request again
    Refresh,
    /// the popup's Tell: the edit box for a line to `name`
    Tell(String),
    /// Invite: `InviteToGroup` with the row as a `VIInviteRec` (its +0x1C
    /// the id)
    Invite(String, u32),
    /// Mail: the gram window (not ported)
    Mail(String),
}

/// `VIWndWhoList`.
#[derive(Default)]
pub struct WhoList {
    sel: i32,
    top: i32,
    window: Window,
    status: Window,
    options: OptionList,
    flash: KeyFlash,
    ticks: i32,
    dots: i32,
    /// the rows drawn, by their place on screen, and the headings
    /// (`HEADING - column`), and when (for a pointer)
    hits: Hits,
    now: i64,
    /// the column the rows are sorted by (the client's, kept while it runs)
    pub sort: Sort,
    /// the rows' order as last drawn (`order`)
    view: Vec<usize>,
    /// the row (an index into the list) to keep selected once a pointer's
    /// sort is drawn
    keep: Option<usize>,
    /// the server's patch's own sort, when it has one (`PatchSort`)
    patch: Option<Arc<PatchSort>>,
    /// The patch's sort as its `GetList` keeps it (S2, FC090): the list in
    /// the order the sort left it (`patch_order`, for the list `patch_gen`),
    /// and the count it last sorted (FC008; None when FC004 is 0, so the
    /// next list is sorted). Kept for the session, as S2's are.
    patch_order: Vec<usize>,
    patch_gen: u64,
    patch_count: Option<usize>,
}

impl WhoList {
    /// The rows in the order shown: the patch's sort's (as `patch_get_list`
    /// last left them), or the client's.
    fn order(&self, vui: &Vui, list: &[WhoRow]) -> Vec<usize> {
        match &self.patch {
            Some(_) if self.patch_order.len() == list.len() => self.patch_order.clone(),
            Some(p) => p.order(self.sort, list),
            None => order(self.sort, list, &|id: i32| vui.words.get(id, vui.language).to_string()),
        }
    }

    /// S2's hook on `VIClientPart38334::GetList` (FC090), run wherever the
    /// game asks for the list (its window's frame and keys): a list of two or
    /// more rows is sorted in place by the column (FC150) unless its data and
    /// count are those it last sorted (FC004, FC008); it remembers them. A
    /// list the server sends again lands in the same buffer the last one
    /// freed (`Close`'s `VIVector::Clear`, then the same growth), so a list
    /// of the same count stays in the server's own order (Sandstorm's: CM
    /// count first) until the count changes or a key moves the column (its
    /// `OnKeyDown` zeroes FC004). The first list is sorted.
    fn patch_get_list(&mut self, who: &Who) {
        let Some(p) = self.patch.clone() else { return };
        if who.received != self.patch_gen || self.patch_order.len() != who.list.len() {
            self.patch_gen = who.received;
            self.patch_order = (0..who.list.len()).collect();
        }
        let n = who.list.len();
        if n < 2 || self.patch_count == Some(n) {
            return;
        }
        self.patch_order = p.sorted(self.sort, &who.list, std::mem::take(&mut self.patch_order));
        self.patch_count = Some(n);
    }

    /// The column sorted by, set: with the patch's sort, as its `OnKeyDown`
    /// does it (FC004 zeroed: the list sorted again at the next look).
    fn set_sort(&mut self, sort: Sort) {
        self.sort = sort;
        self.patch_count = None;
    }

    /// A pointer at frame point `p` (`pointer`): with the popup up, its
    /// choice taken, or off it the popup closed (△) and the row under the
    /// pointer selected; else a row selected, or the selection's popup
    /// opened (✕). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.options.visible {
            if let Some(k) = self.options.point(p) {
                return k.into_iter().collect();
            }
            let mut keys = Vec::new();
            if let Some(row) = self.hits.at(p).filter(|&row| row >= 0 && row != self.sel) {
                self.select(row);
                keys.push(MOVED);
            }
            keys.push(key::TRIANGLE);
            return keys;
        }
        match self.hits.at(p) {
            // a heading: the rows by its column (the client's), the
            // selected one kept once drawn (with the patch's sort, the
            // selection's place, as its keys keep it); the column already
            // sorted by is nothing
            Some(h) if h <= HEADING => {
                let sort = Sort::ALL[(HEADING - h) as usize];
                if sort == self.sort {
                    return Vec::new();
                }
                if self.patch.is_none() {
                    self.keep = self.view.get(self.sel.max(0) as usize).copied();
                }
                self.set_sort(sort);
                vec![MOVED]
            }
            Some(row) if row == self.sel => vec![key::CROSS],
            Some(row) => {
                self.select(row);
                vec![MOVED]
            }
            None => Vec::new(),
        }
    }

    /// The selection on `row` as the d-pad's move leaves it: the tick's
    /// pulse started over.
    fn select(&mut self, row: i32) {
        self.sel = row;
        self.window.reset_tick_pulse(self.now);
    }
    /// `OnMessage` 295 from outside: the window and the status box set up
    /// and titled (730, 271), the popup's three choices (425, 942, 515)
    /// under 422.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster) {
        let w = |id: i32| wide(vui.words.get(id, vui.language));
        self.window.init(vui, r, [320, 180], 600, 13, 0);
        self.window.set_title(Some(&w(730)));
        self.window.open(true);
        self.status.close(true);
        self.status.init(vui, r, [320, 180], 330, 4, 0);
        self.status.set_title(Some(&w(271)));
        self.options.setup(vui, r, &w(422), vec![w(425), w(942), w(515)], [320, 180], 256);
        self.options.visible = false;
        self.sel = 0;
        self.top = 0;
        self.flash.reset();
        self.patch = PatchSort::current();
    }

    /// `OnKeyDown`/`OnKeyUp` (the popup's while it is up).
    pub fn key(&mut self, vui: &Vui, r: &dyn Raster, who: &Who, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<WhoUp> {
        self.patch_get_list(who);
        if self.options.visible {
            if !down {
                return None;
            }
            let (sel, ok) = self.options.key(key, now, cues)?;
            let view = self.order(vui, &who.list);
            let e = who.list.get(*view.get(self.sel.max(0) as usize)?)?;
            let name = e.name.clone();
            let up = match sel {
                0 => WhoUp::Tell(name),
                1 => WhoUp::Invite(name, e.x1c),
                _ => WhoUp::Mail(name),
            };
            return ok.then_some(up);
        }
        if !down {
            match key {
                19 | 138 => self.flash.release(138),
                20 | 139 => self.flash.release(139),
                _ => {}
            }
            return None;
        }
        match key {
            19 | 138 => self.flash.press(138),
            20 | 139 => self.flash.press(139),
            27 | 132 => {
                cues.push(8);
                return Some(WhoUp::Close);
            }
            13 | 134 => {
                // Select
                if (self.sel as usize) < who.list.len() {
                    cues.push(11);
                    self.options.visible = true;
                    self.options.focus(vui, r);
                }
            }
            81 | 113 | 133 => {
                self.sel = 0;
                self.top = 0;
                self.flash.reset();
                return Some(WhoUp::Refresh);
            }
            // the patch's sort (S2's `OnKeyDown`, before the game's, which
            // takes none of these): a column back or on, the selection left
            // where it is in the list
            k if self.patch.as_ref().is_some_and(|p| p.back.contains(&k)) => self.set_sort(self.sort.prev()),
            k if self.patch.as_ref().is_some_and(|p| p.on.contains(&k)) => self.set_sort(self.sort.next()),
            // ○ (the client's): the next column's order, the selected row
            // kept selected (with the patch's sort, its place, as the
            // patch's keys keep it)
            135 => {
                let keep = self.order(vui, &who.list).get(self.sel.max(0) as usize).copied();
                self.set_sort(self.sort.next());
                self.patch_get_list(who);
                if self.patch.is_none()
                    && let Some(at) = keep.and_then(|k| self.order(vui, &who.list).iter().position(|&i| i == k))
                {
                    self.sel = at as i32;
                }
                self.window.reset_tick_pulse(now);
                cues.push(11);
            }
            _ => {}
        }
        None
    }

    /// `OnDrawSelf`: the black band, the window with the header row and ten
    /// rows (the selected one ticked), the legend, "searching..." while the
    /// reply is out; then the popup.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, who: &Who, now: i64, focused: bool, cues: &mut Vec<i32>) {
        let w = |id: i32| wide(vui.words.get(id, vui.language));
        let n = who.list.len() as i32;
        self.now = now;
        self.hits.clear();
        self.patch_get_list(who);
        self.view = self.order(vui, &who.list);
        if let Some(at) = self.keep.take().and_then(|k| self.view.iter().position(|&i| i == k)) {
            self.sel = at as i32;
        }
        crate::ui::window_backdrop(vui, r, 642);
        self.window.draw(vui, r);
        if self.sel > n - 1 {
            self.sel = n - 1;
        }
        self.sel = self.sel.max(0);
        if self.sel - self.top > 9 {
            self.top = self.sel - 9;
        } else if self.sel < self.top {
            self.top = self.sel;
        }
        let t = if n == 10 { 0.0 } else { self.top as f32 / (n - 10) as f32 };
        let width = self.window.size[0];
        self.window.draw_frame_vscroll(vui, r, 30, 1, width - 65, 10, t);
        let x = [50, 300, 400, 500];
        let c0 = UI_COLORS[0];
        // the headings, the one sorted by in the selection's colour (the
        // client's) or, with the patch's sort, between its marks (S2's
        // `OnDrawSelf`, after the headings), each a pointer's sort
        let ends = [x[1], x[2], x[3], width - 40];
        for (k, sort) in Sort::ALL.into_iter().enumerate() {
            let c = UI_COLORS[(sort == self.sort && self.patch.is_none()) as usize];
            self.window.text_box(vui, r, &w(sort.word()), x[k], 0, ends[k] - x[k], 1, c, 0);
            self.hits.push(rows_rect(&self.window, vui, r, x[k], 0, ends[k] - x[k], 1), HEADING - k as i32);
        }
        if let Some(p) = &self.patch {
            let k = self.sort as usize;
            self.window.text_box(vui, r, &p.marks[0], p.before[k], 0, 12, 1, c0, 0);
            self.window.text_box(vui, r, &p.marks[1], p.after[k], 0, 12, 1, c0, 0);
        }
        let mut row = 1;
        let mut i = self.top;
        while i < n && row < 11 {
            let e = &who.list[self.view[i as usize]];
            let c = UI_COLORS[(i == self.sel) as usize];
            let (race, class) = (race_word(e.race), class_word(e.class));
            self.window.text_box(vui, r, &wide(&e.name), x[0], row, x[1] - x[0], 1, c, 2);
            self.window.text_box(vui, r, &w(race), x[1], row, x[2] - x[1], 1, c, 2);
            self.window.text_box(vui, r, &w(class), x[2], row, x[3] - x[2], 1, c, 2);
            self.window.text_box(vui, r, &wide(&e.level.to_string()), x[3], row, width - x[3] - 40, 1, c, 2);
            self.hits.push(rows_rect(&self.window, vui, r, x[0], row, width - x[0] - 40, 1), i);
            if i == self.sel {
                self.window.tick_r(vui, r, 46, row, focused && !self.options.visible);
            }
            i += 1;
            row += 1;
        }
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        // the game's legend, and the client's ○ Sort with its column; the
        // patch's, with its keys and its word, when it sorts
        let legend = match &self.patch {
            Some(p) => crate::searchwnd::format(&p.legend, &[&w(212), &w(161), &w(811), &p.sort_word], 256),
            None => wide(&format!("|x{}   |t{}   |s{}   |c{}: {}", word(212), word(161), word(811), SORT, word(self.sort.word()))),
        };
        self.window.text_box(vui, r, &legend, 30, 12, width - 60, 1, c0, 0);
        self.ticks -= 1;
        if self.ticks <= 0 {
            self.ticks = 15;
            self.dots += 1;
            if self.dots == 3 {
                self.dots = 0;
            }
        }
        if who.pending {
            self.status.open(false);
            self.status.draw(vui, r);
            let s = wide(&format!("{}{}", word(943), ".".repeat(self.dots as usize + 1)));
            let sw = self.status.size[0];
            self.status.text_box(vui, r, &s, 40, 1, sw - 40, 1, c0, 0);
            let back = wide(&format!("|t{}", word(161)));
            self.status.text_box(vui, r, &back, 40, 3, sw - 40, 1, c0, 0);
        }
        if self.options.visible {
            self.options.draw(vui, r);
        }
        if self.flash.pulse(138) {
            if self.sel > 0 {
                self.sel -= 1;
                self.window.reset_tick_pulse(now);
                cues.push(11);
            } else {
                cues.push(9);
            }
        }
        if self.flash.pulse(139) {
            if self.sel < n - 1 {
                self.sel += 1;
                self.window.reset_tick_pulse(now);
                cues.push(11);
            } else {
                cues.push(9);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(serial: i64, names: &[&str]) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(1).signed(serial).signed(names.len() as i64);
        for (k, n) in names.iter().enumerate() {
            w.u8(0).u8(7).u8(k as u8 + 1).string(n.as_bytes()).u32(100 + k as u32);
        }
        w.data
    }

    /// `Recv01`: the reply to the request out fills the list; one to an
    /// older request (its serial) is dropped; a reply of another kind too.
    #[test]
    fn a_stale_reply_is_dropped() {
        let mut who = Who::default();
        let req = who.open();
        assert_eq!(&req[..3], &[0x01, 0x0E, 1]);
        assert!(who.pending);
        who.recv(&reply(0, &["Old"]));
        assert!(who.list.is_empty() && who.pending, "serial 0 is the previous request's");
        let mut other = reply(1, &["Nobody"]);
        other[0] = 2;
        who.recv(&other);
        assert!(who.list.is_empty());
        who.recv(&reply(1, &["Rsgroup", "TestBear"]));
        assert!(!who.pending);
        assert_eq!(who.list.iter().map(|e| (e.name.as_str(), e.level, e.x1c)).collect::<Vec<_>>(), vec![("Rsgroup", 1, 100), ("TestBear", 2, 101)]);
    }

    /// A pointer on the who list: a row selected, then its popup opened
    /// (✕); on the popup a choice taken at once; off it the popup closed
    /// (△) and the row under the pointer selected. The header, the rows
    /// past the list and what is off the window are nothing.
    #[test]
    fn a_pointer_selects_a_row_and_opens_its_popup() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        use crate::pointer::{MOVED, key};
        let vui = vui();
        let mut r = NullRaster;
        let mut who = Who::default();
        for k in 0..3 {
            who.list.push(WhoRow { name: format!("P{k}"), ..Default::default() });
        }
        let mut l = WhoList::default();
        l.open(&vui, &r);
        let mut cues = Vec::new();
        l.draw(&vui, &mut r, &who, 0, true, &mut cues);
        // the list's rows are the window's rows 1.., from x 50
        let row = |l: &WhoList, i: i32| rows_rect(&l.window, &vui, &NullRaster, 50, i + 1, 400, 1);
        assert_eq!(l.point(mid(row(&l, 1))), vec![MOVED]);
        assert_eq!(l.sel, 1);
        assert_eq!(l.point(mid(row(&l, 1))), vec![key::CROSS], "the selection: its popup");
        assert!(l.point(mid(row(&l, -1))).is_empty(), "the header");
        assert!(l.point(mid(row(&l, 3))).is_empty(), "past the list");
        assert!(l.point([5.0, 5.0]).is_empty(), "off the window");
        assert_eq!(l.sel, 1);
        // ✕ opens the popup; drawn, its choices are there
        l.key(&vui, &r, &who, 134, true, 0, &mut cues);
        assert!(l.options.visible);
        l.draw(&vui, &mut r, &who, 16, true, &mut cues);
        let choice = mid(rows_rect(&l.options.window, &vui, &NullRaster, 40, 2, 100, 1));
        assert_eq!(l.point(choice), vec![key::CROSS]);
        assert_eq!(l.options.sel, 1, "Invite");
        // the popup's own frame: nothing
        let tl = l.options.window.top_left();
        assert!(l.point([tl[0] as f32 + 4.0, tl[1] as f32 + 4.0]).is_empty());
        // off the popup on row 0, at its left end: closed, row 0 selected
        let r0 = row(&l, 0);
        assert_eq!(l.point([r0[0] as f32 + 10.0, mid(r0)[1]]), vec![MOVED, key::TRIANGLE]);
        assert_eq!(l.sel, 0);
        assert_eq!(l.point([5.0, 5.0]), vec![key::TRIANGLE], "off everything: closed only");
    }

    /// Races and classes as a window would spell them (the test's own).
    fn spelled(id: i32) -> String {
        let classes = ["Warrior", "Ranger", "Paladin", "Shadowknight", "Monk", "Bard", "Rogue", "Druid", "Shaman", "Cleric", "Magician", "Necromancer", "Enchanter", "Wizard"];
        let races = ["Human", "Elf", "Dark Elf", "Gnome", "Dwarf", "Troll", "Barbarian", "Halfling", "Erudite"];
        match id {
            303..=316 => classes[(id - 303) as usize].into(),
            1190 => "Alchemist".into(),
            317..=325 => races[(id - 317) as usize].into(),
            1191 => "Ogre".into(),
            _ => String::new(),
        }
    }

    fn row(name: &str, race: u8, class: u8, level: u8) -> WhoRow {
        WhoRow { name: name.into(), race, class, level, x1c: 0 }
    }

    /// The client's sort (F3 20261023-085524, -090229): by name from A to
    /// Z, case aside, where the game keeps the server's order (the
    /// control); by race and class as the window spells them (an Ogre after
    /// a Human, an Alchemist, class 14, first), by level from the highest;
    /// a tie by name.
    #[test]
    fn the_rows_sort_by_a_column() {
        let list = vec![row("Zed", 9, 5, 30), row("amy", 0, 13, 50), row("Bob", 0, 14, 30), row("cal", 2, 5, 10)];
        let names = |sort| order(sort, &list, &spelled).into_iter().map(|i| list[i].name.as_str()).collect::<Vec<_>>();
        assert_eq!(Sort::default(), Sort::Name);
        assert_eq!(names(Sort::Name), ["amy", "Bob", "cal", "Zed"]);
        assert_ne!(list.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), names(Sort::Name), "the game's order is the server's");
        // Dark Elf, Human (amy, Bob), Ogre
        assert_eq!(names(Sort::Race), ["cal", "amy", "Bob", "Zed"]);
        // Alchemist, Bard (cal, Zed), Wizard
        assert_eq!(names(Sort::Class), ["Bob", "cal", "Zed", "amy"]);
        // 50, 30 (Bob, Zed), 10
        assert_eq!(names(Sort::Level), ["amy", "Bob", "Zed", "cal"]);
        assert_eq!([Sort::Name, Sort::Race, Sort::Class, Sort::Level].map(Sort::next), [Sort::Race, Sort::Class, Sort::Level, Sort::Name]);
    }

    /// ○ goes round the columns with the selected row kept selected, and
    /// the popup's Tell and Invite go to the row shown, not to the one the
    /// server sent at that place.
    #[test]
    fn circle_sorts_and_the_popup_acts_on_the_row_shown() {
        use crate::pointer::testing::{NullRaster, vui};
        let vui = vui();
        let mut r = NullRaster;
        let who = Who { list: vec![WhoRow { x1c: 7, ..row("Zed", 0, 0, 30) }, WhoRow { x1c: 8, ..row("amy", 0, 0, 50) }, WhoRow { x1c: 9, ..row("Bob", 0, 0, 10) }], ..Default::default() };
        let mut l = WhoList::default();
        l.open(&vui, &r);
        let mut cues = Vec::new();
        l.draw(&vui, &mut r, &who, 0, true, &mut cues);
        let shown = |l: &WhoList| l.view.iter().map(|&i| who.list[i].name.clone()).collect::<Vec<_>>();
        assert_eq!(shown(&l), ["amy", "Bob", "Zed"]);
        // Zed, shown third, selected
        l.sel = 2;
        // ○ three times: race and class (all alike here), then level
        for _ in 0..3 {
            l.key(&vui, &r, &who, 135, true, 0, &mut cues);
        }
        assert_eq!(l.sort, Sort::Level);
        l.draw(&vui, &mut r, &who, 16, true, &mut cues);
        assert_eq!(shown(&l), ["amy", "Zed", "Bob"]);
        assert_eq!(l.sel, 1, "Zed still selected");
        // the popup: Tell and Invite name Zed (the server sent amy second)
        l.key(&vui, &r, &who, 134, true, 0, &mut cues);
        assert_eq!(l.key(&vui, &r, &who, 134, true, 0, &mut cues), Some(WhoUp::Tell("Zed".into())));
        l.key(&vui, &r, &who, 134, true, 0, &mut cues);
        l.key(&vui, &r, &who, 139, true, 0, &mut cues);
        assert_eq!(l.key(&vui, &r, &who, 134, true, 0, &mut cues), Some(WhoUp::Invite("Zed".into(), 7)));
        // ○ once more: back to names
        l.key(&vui, &r, &who, 135, true, 0, &mut cues);
        assert_eq!(l.sort, Sort::Name);
    }

    /// A pointer on a heading sorts by its column, the selected row kept
    /// once drawn; the column already sorted by is nothing; with the popup
    /// up a heading only closes it.
    #[test]
    fn a_heading_sorts_by_its_column() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        use crate::pointer::{MOVED, key};
        let vui = vui();
        let mut r = NullRaster;
        let who = Who { list: vec![row("Zed", 0, 0, 30), row("amy", 0, 0, 50), row("Bob", 0, 0, 10)], ..Default::default() };
        let mut l = WhoList::default();
        l.open(&vui, &r);
        let mut cues = Vec::new();
        l.draw(&vui, &mut r, &who, 0, true, &mut cues);
        l.sel = 2;
        // the Level heading: x 500 to the window's last 40
        let level = mid(rows_rect(&l.window, &vui, &NullRaster, 500, 0, 60, 1));
        let name = mid(rows_rect(&l.window, &vui, &NullRaster, 50, 0, 200, 1));
        assert!(l.point(name).is_empty(), "sorted by name already");
        assert_eq!(l.point(level), vec![MOVED]);
        assert_eq!(l.sort, Sort::Level);
        l.draw(&vui, &mut r, &who, 16, true, &mut cues);
        assert_eq!(l.view.iter().map(|&i| who.list[i].name.as_str()).collect::<Vec<_>>(), ["amy", "Zed", "Bob"]);
        assert_eq!(l.sel, 1, "Zed kept");
        assert!(l.point(level).is_empty(), "the same column: nothing");
        l.key(&vui, &r, &who, 134, true, 0, &mut cues);
        l.draw(&vui, &mut r, &who, 32, true, &mut cues);
        assert_eq!(l.point(name), vec![key::TRIANGLE], "the popup closed, the sort kept");
        assert_eq!(l.sort, Sort::Level);
    }

    /// What the patch's sort draws, by place: a recording raster.
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

    /// The short words of the races and the classes, as the window shows
    /// them (the test's own spelling of the game's words).
    const RACE_SHORT: [&str; 10] = ["HUM", "ELF", "DELF", "GNO", "DWF", "TRL", "BAR", "HLF", "ERU", "OGR"];
    const CLASS_SHORT: [&str; 15] = ["WAR", "RAN", "PAL", "SK", "MNK", "BRD", "RGE", "DRD", "SHA", "CL", "MAG", "NEC", "ENC", "WIZ", "ALC"];

    /// A patch's sort stood in for a test: ranks from A to Z by the short
    /// words, its keys, marks, places and legend the test's own.
    fn patch_sort() -> PatchSort {
        let rank = |words: &[&str]| {
            let mut order: Vec<usize> = (0..words.len()).collect();
            order.sort_by_key(|&i| words[i]);
            let mut r = [words.len() as u8; 16];
            for (k, &i) in order.iter().enumerate() {
                r[i] = k as u8;
            }
            r
        };
        PatchSort {
            race_rank: rank(&RACE_SHORT),
            class_rank: rank(&CLASS_SHORT),
            name_bytes: 29,
            back: [140, 136],
            on: [142, 137],
            marks: [wide("<"), wide(">")],
            before: [35, 285, 385, 485],
            after: [102, 352, 464, 564],
            legend: b"|x%S |t%S |s%S |v|V%S".to_vec(),
            sort_word: wide("Order"),
        }
    }

    /// Sandstorm's S2 sorts the who list itself: what its hooks use, read out
    /// of the live S2 where its code names it. Its keys are L1 and left one
    /// column back, R1 and right one on; its ranks put the races and the
    /// classes in the order their short words spell; its marks are the ones
    /// round the search window's sorted heading (read there too), placed
    /// just before each heading and past its start; its legend takes four
    /// words. The controls: an image without the patch has no sort, and one
    /// whose race table is built into another register is refused.
    /// S2's `GetList` sorts a list only when its count is not the one it
    /// last sorted (FC004/FC008: the buffer and the count; a list the server
    /// sends again lands in the buffer the last one freed). So the first list
    /// is sorted by the column, a second of the same count stays in the
    /// server's own order (Sandstorm's: CM count first, F3 2026-10-29) until a
    /// key moves the column, and one of another count is sorted. The control:
    /// the patch's sort run on every list, as before, puts the second in
    /// name order.
    #[test]
    fn a_list_sent_again_with_the_same_count_keeps_the_servers_order() {
        let p = Arc::new(patch_sort());
        let mut wl = WhoList { patch: Some(p.clone()), ..Default::default() };
        let mut who = Who::default();
        let rows = |names: &[&str]| names.iter().map(|n| row(n, 0, 0, 60)).collect::<Vec<_>>();
        let shown = |wl: &WhoList, who: &Who| wl.patch_order.iter().map(|&i| who.list[i].name.clone()).collect::<Vec<_>>();
        who.list = rows(&["Zed", "amy", "Bob"]);
        who.received = 1;
        wl.patch_get_list(&who);
        assert_eq!(shown(&wl, &who), ["Bob", "Zed", "amy"], "the first list, sorted by name (bytes: capitals first)");
        wl.patch_get_list(&who);
        assert_eq!(shown(&wl, &who), ["Bob", "Zed", "amy"], "the same list, looked at again");
        who.list = rows(&["Endo", "Mud", "Abe"]);
        who.received = 2;
        wl.patch_get_list(&who);
        assert_eq!(shown(&wl, &who), ["Endo", "Mud", "Abe"], "sent again with the same count: the server's order");
        assert_ne!(p.order(Sort::Name, &who.list), vec![0, 1, 2], "the control: sorted every time, the server's order is lost");
        wl.set_sort(Sort::Level);
        wl.set_sort(Sort::Name);
        wl.patch_get_list(&who);
        assert_eq!(shown(&wl, &who), ["Abe", "Endo", "Mud"], "a key moved the column: sorted again");
        who.list = rows(&["Endo", "Mud", "Abe", "Cal"]);
        who.received = 3;
        wl.patch_get_list(&who);
        assert_eq!(shown(&wl, &who), ["Abe", "Cal", "Endo", "Mud"], "another count: sorted");
    }

    #[test]
    fn the_patchs_sort_is_read_out_of_the_live_s2() {
        let Some(patch) = crate::searchwnd::testing::live() else { return };
        let img = patch.apply(0);
        let p = PatchSort::read(&img).expect("the live S2 sorts the who list");
        assert_eq!((p.back, p.on), ([140, 136], [142, 137]));
        let by_rank = |n: usize, rank: &[u8; 16]| {
            let mut v: Vec<usize> = (0..n).collect();
            v.sort_by_key(|&i| rank[i]);
            v
        };
        let by_word = |words: &[&str]| {
            let mut v: Vec<usize> = (0..words.len()).collect();
            v.sort_by_key(|&i| words[i]);
            v
        };
        assert_eq!(by_rank(10, &p.race_rank), by_word(&RACE_SHORT));
        assert_eq!(by_rank(15, &p.class_rank), by_word(&CLASS_SHORT));
        assert!(p.name_bytes >= 12, "at least a name's twelve");
        let x = [50, 300, 400, 500];
        for k in 0..4 {
            assert!(p.before[k] + 12 <= x[k] && p.before[k] > x[k] - 30, "the mark before heading {k} at {}", p.before[k]);
            assert!(p.after[k] > x[k], "the mark after heading {k} at {}", p.after[k]);
        }
        let art = crate::searchwnd::Art::read(&img).expect("the search window");
        let marked = [p.marks[0].clone(), art.heads[0][0].clone(), p.marks[1].clone()].concat();
        assert_eq!(art.heads[0][1], marked, "the search window's marks");
        assert_eq!(p.legend.windows(2).filter(|w| w == b"%S").count(), 4);
        assert!(!p.sort_word.is_empty());
        assert!(PatchSort::read(&Image::default()).is_none(), "the control: no patch, no sort");
        let compare = img.call_target(img.call_target(img.call_target(CLIENT_BASE + at::GET_LIST).unwrap() + at::SORT).unwrap() + at::COMPARE).unwrap();
        let mut moved = patch.clone();
        let mut found = false;
        for rec in &mut moved.records {
            let o = (compare + at::RACE_RANK.0).wrapping_sub(rec.addr & 0x7FFF_FFFF) as usize;
            if rec.addr & 0x8000_0000 != 0 && o + 4 <= rec.data.len() {
                // lui t3 made lui t4
                rec.data[o + 2] = (rec.data[o + 2] & !31) | 12;
                found = true;
            }
        }
        assert!(found);
        assert!(PatchSort::read(&moved.apply(0)).is_none(), "the control: a table built into another register");
    }

    /// The patch's order: the list as sent, stably by the column (a race or
    /// a class by its rank, levels from the highest), then the names byte by
    /// byte, capitals first, where the client's own goes by them case aside
    /// (the control).
    #[test]
    fn the_patchs_order() {
        let p = patch_sort();
        let list = vec![row("Zed", 9, 5, 30), row("amy", 0, 13, 50), row("Bob", 0, 14, 30), row("cal", 2, 5, 10)];
        let names = |v: Vec<usize>| v.into_iter().map(|i| list[i].name.as_str()).collect::<Vec<_>>();
        assert_eq!(names(p.order(Sort::Name, &list)), ["Bob", "Zed", "amy", "cal"]);
        assert_eq!(names(order(Sort::Name, &list, &spelled)), ["amy", "Bob", "cal", "Zed"], "the control: the client's");
        // DELF (cal), HUM (Bob, amy), OGR (Zed)
        assert_eq!(names(p.order(Sort::Race, &list)), ["cal", "Bob", "amy", "Zed"]);
        // ALC (Bob), BRD (Zed, cal), WIZ (amy)
        assert_eq!(names(p.order(Sort::Class, &list)), ["Bob", "Zed", "cal", "amy"]);
        assert_eq!(names(p.order(Sort::Level, &list)), ["amy", "Bob", "Zed", "cal"]);
        assert_eq!([Sort::Name, Sort::Race, Sort::Class, Sort::Level].map(Sort::prev), [Sort::Level, Sort::Name, Sort::Race, Sort::Class]);
    }

    /// With the patch's sort (F3 20261023-185531: "anchor to whatever
    /// position in the list you are currently on"), R1 and right go a column
    /// on and L1 and left one back, the selection staying where it is in the
    /// list (the player there now another); ○ and a heading too. The
    /// control: the client's own sort keeps the player selected instead.
    #[test]
    fn the_patchs_sort_keeps_the_selections_place() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        let vui = vui();
        let mut r = NullRaster;
        let who = Who { list: vec![row("Zed", 0, 0, 30), row("Amy", 0, 0, 50), row("Bob", 0, 0, 10)], ..Default::default() };
        let mut cues = Vec::new();
        let shown = |l: &WhoList| l.view.iter().map(|&i| who.list[i].name.clone()).collect::<Vec<_>>();
        let selected = |l: &WhoList| who.list[l.view[l.sel as usize]].name.clone();
        let mut l = WhoList::default();
        l.open(&vui, &r);
        l.patch = Some(Arc::new(patch_sort()));
        l.draw(&vui, &mut r, &who, 0, true, &mut cues);
        assert_eq!(shown(&l), ["Amy", "Bob", "Zed"]);
        l.sel = 2;
        // R1: race (all alike), then right: class, then R1: level
        for k in [142, 137, 142] {
            l.key(&vui, &r, &who, k, true, 0, &mut cues);
        }
        assert_eq!(l.sort, Sort::Level);
        l.draw(&vui, &mut r, &who, 16, true, &mut cues);
        assert_eq!(shown(&l), ["Amy", "Zed", "Bob"]);
        assert_eq!((l.sel, selected(&l).as_str()), (2, "Bob"), "the place kept, the player there another");
        // L1 and left: back to class, then race
        l.key(&vui, &r, &who, 140, true, 0, &mut cues);
        l.key(&vui, &r, &who, 136, true, 0, &mut cues);
        assert_eq!((l.sort, l.sel), (Sort::Race, 2));
        // ○ and a heading keep the place too
        l.key(&vui, &r, &who, 135, true, 0, &mut cues);
        l.draw(&vui, &mut r, &who, 32, true, &mut cues);
        assert_eq!((l.sort, l.sel), (Sort::Class, 2));
        let level = mid(rows_rect(&l.window, &vui, &NullRaster, 500, 0, 60, 1));
        assert_eq!(l.point(level), vec![MOVED]);
        l.draw(&vui, &mut r, &who, 48, true, &mut cues);
        assert_eq!((l.sort, l.sel, selected(&l).as_str()), (Sort::Level, 2, "Bob"));
        // the control: without the patch ○ keeps the player
        let mut c = WhoList::default();
        c.open(&vui, &r);
        c.draw(&vui, &mut r, &who, 0, true, &mut cues);
        c.sel = 2;
        for _ in 0..3 {
            c.key(&vui, &r, &who, 135, true, 0, &mut cues);
        }
        c.draw(&vui, &mut r, &who, 16, true, &mut cues);
        assert_eq!((c.sel, selected(&c).as_str()), (1, "Zed"));
        // and the patch's keys are nothing to it
        c.key(&vui, &r, &who, 142, true, 0, &mut cues);
        assert_eq!(c.sort, Sort::Level);
    }

    /// With the patch's sort the heading sorted by is drawn between the
    /// patch's marks at its places, in the headings' own colour, and the
    /// legend is the patch's (its keys and its word); the control: the
    /// client's sort draws no marks and its own legend.
    #[test]
    fn the_patchs_marks_and_legend_are_drawn() {
        use crate::pointer::testing::vui;
        let vui = vui();
        let who = Who { list: vec![row("Zed", 0, 0, 30), row("Amy", 0, 0, 50)], ..Default::default() };
        let mut cues = Vec::new();
        let mut l = WhoList::default();
        l.open(&vui, &Texts::default());
        l.patch = Some(Arc::new(patch_sort()));
        l.sort = Sort::Class;
        let mut t = Texts::default();
        l.draw(&vui, &mut t, &who, 0, true, &mut cues);
        let at = |t: &Texts, s: &str| t.0.iter().find(|(_, x)| x == s).map(|(a, _)| *a);
        // the first row's (Amy) a row under the headings
        let heading_y = at(&t, "Amy").map(|a| a[1] - 16);
        let (before, after) = (at(&t, "<").expect("the mark before"), at(&t, ">").expect("the mark after"));
        let x0 = l.window.top_left()[0];
        assert_eq!((before[0] - x0, after[0] - x0), (385, 464), "the class's places");
        assert_eq!((Some(before[1]), Some(after[1])), (heading_y, heading_y), "on the headings' row");
        // the legend's words between its glyphs: the patch's last
        assert!(t.0.iter().any(|(_, s)| s == "Order") && !t.0.iter().any(|(_, s)| s.contains(SORT)), "the patch's legend: {:?}", t.0);
        let mut c = WhoList::default();
        c.open(&vui, &Texts::default());
        c.sort = Sort::Class;
        let mut t = Texts::default();
        c.draw(&vui, &mut t, &who, 0, true, &mut cues);
        assert!(at(&t, "<").is_none() && at(&t, ">").is_none(), "the control: no marks");
        assert!(t.0.iter().any(|(_, s)| s.contains(SORT)) && !t.0.iter().any(|(_, s)| s == "Order"), "the client's legend");
    }
}
