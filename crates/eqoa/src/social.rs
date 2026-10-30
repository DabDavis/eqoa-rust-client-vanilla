//! The buddy and ignore lists. `VIClientPart38300` is the ignore list (in
//! 0x1003 add, 0x1004 erase by id, 0x1005 a counted list, 0x1006 a rename;
//! out 0x1000 add by name, 0x1002 remove by id), kept in arrival order;
//! `VIWndIgnoreList` shows it twelve names a page, with a titled mini menu
//! (`gIgnoreMemberMenu`: Stop Ignore) and a Yes/No box to confirm. The buddy
//! list is VIClient's `m_buddies` map (0x03D5 adds or replaces a record,
//! 0x03D6 erases one), in id order; `VIWndBuddy` shows ten rows of it, with
//! `gBuddyMenu` (Message, Mail, Remove) and the box that also carries an
//! incoming buddy request (`ShowRequest`, 45 s to answer). A pointer
//! (`pointer`) goes to the child with the focus as the keys do: the box's
//! buttons, the menu's rows (off the menu, it closes), the list's rows.
//!
//! Not the game's: the buddy list's order. The game lists the buddies by
//! id; here they are sorted (`Sort`), by name each time the list opens:
//! left and L1 sort by the column before, right and R1 by the one after
//! (round past either end), as Sandstorm's patch sorts its who list, and a
//! click on a column's heading by that column. The heading sorted by is
//! between the game's side arrows and the legend has the d-pad's arms and
//! Sort, as there. A sort leaves the selection at its place in the list
//! (the buddy there now another). The menu and its box act on the buddy
//! shown.
//!
//! Not the game's: the buddy list's rows in a scrolling frame, as the who
//! list's and the guild's roster's are (`ROWS_FRAME`).

#![allow(clippy::too_many_arguments)]

use crate::client_image::Menus;
use crate::dialog::{BoxAnswer, Key, MBox};
use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::startmenu::KeyFlash;
use crate::wndlist::{ListUp, MiniMenu};
use net::stream::{Reader, Writer};
use std::collections::BTreeMap;
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// A game key code as the message box's key.
fn box_key(key: i32) -> Option<Key> {
    match key {
        13 | 134 => Some(Key::Enter),
        27 | 132 => Some(Key::Escape),
        17 | 136 => Some(Key::Left),
        18 | 137 => Some(Key::Right),
        _ => None,
    }
}

/// A pointer on a list with a box and a titled menu over it (the buddy and
/// ignore lists), to the child with the focus: the box's buttons (✕), the
/// menu's rows at once, or off the menu the menu closed (△) and the row
/// under the pointer selected (`select`); else a row selected, or the
/// selection's menu opened (✕). The keys to press.
fn list_point(p: [f32; 2], mbox: Option<&mut MBox>, menu: Option<&mut MiniMenu>, hits: &Hits, sel: i32, select: &mut dyn FnMut(i32)) -> Vec<i32> {
    if let Some(b) = mbox {
        return b.point(p).then_some(key::CROSS).into_iter().collect();
    }
    if let Some(m) = menu {
        if let Some(k) = m.point(p) {
            return k.into_iter().collect();
        }
        let mut keys = Vec::new();
        if let Some(row) = hits.at(p).filter(|&row| row >= 0 && row != sel) {
            select(row);
            keys.push(MOVED);
        }
        keys.push(key::TRIANGLE);
        return keys;
    }
    match hits.at(p) {
        Some(row) if row == sel => vec![key::CROSS],
        Some(row) => {
            select(row);
            vec![MOVED]
        }
        None => Vec::new(),
    }
}

/// A string read off the wire as `ReadString` leaves it: up to its NUL.
fn cstr(b: &[u8]) -> String {
    let n = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..n]).into_owned()
}

/// `VIString::strcasecmp`.
fn same_name(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// `VIRec36_BFBCB8`: a name and the id the server knows it by.
#[derive(Clone, Debug, PartialEq)]
pub struct IgnoreRow {
    pub name: String,
    pub id: u32,
}

/// `VIClientPart38300`: the pool walked in insertion order.
#[derive(Default)]
pub struct Ignore {
    pub list: Vec<IgnoreRow>,
}

impl Ignore {
    /// `Recv(op, s)`: 0x1003..0x1006. False for any other opcode.
    pub fn recv(&mut self, op: u16, data: &[u8]) -> bool {
        let mut r = Reader::new(data);
        let row = |r: &mut Reader| {
            let name = cstr(&r.string(32));
            IgnoreRow { name, id: r.u32() }
        };
        match op {
            0x1003 => {
                let e = row(&mut r);
                self.list.push(e);
            }
            0x1004 => {
                let id = r.u32();
                if let Some(k) = self.list.iter().position(|e| e.id == id) {
                    self.list.remove(k);
                }
            }
            0x1005 => {
                // ReadList: appended, not replaced
                let n = r.signed().clamp(0, 4096);
                for _ in 0..n {
                    let e = row(&mut r);
                    if r.error {
                        break;
                    }
                    self.list.push(e);
                }
            }
            0x1006 => {
                let e = row(&mut r);
                if let Some(x) = self.list.iter_mut().find(|x| x.id == e.id) {
                    x.name = e.name;
                }
            }
            _ => return false,
        }
        true
    }

    /// `Add(name)`: 0x1000 with the name.
    pub fn add(name: &str) -> Vec<u8> {
        let mut w = Writer::op(0x1000);
        w.string(name.as_bytes());
        w.data
    }

    /// `RemoveAt(i)`: 0x1002 with the entry's id, raw.
    pub fn remove_id(id: u32) -> Vec<u8> {
        let mut w = Writer::op(0x1002);
        w.u32(id);
        w.data
    }

    /// `Remove(name)`: the first entry of that name, any case, removed; none
    /// sends nothing.
    pub fn remove(&self, name: &str) -> Option<Vec<u8>> {
        self.list.iter().find(|e| same_name(&e.name, name)).map(|e| Ignore::remove_id(e.id))
    }
}

/// What the ignore list asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum IgnoreUp {
    /// `ClosePanel`: `CloseIgnoreList`, back to the start menu
    Close,
    /// the box's yes to Stop Ignore: `RemoveAt`, the entry's id
    Remove(u32),
}

/// `VIWndIgnoreList`.
pub struct IgnoreList {
    sel: i32,
    top: i32,
    count: i32,
    window: Window,
    pub mbox: MBox,
    pub mbox_visible: bool,
    pub menu: MiniMenu,
    flash: KeyFlash,
    /// the names drawn, by index into the list, and when (for a pointer)
    hits: Hits,
    now: i64,
}

impl IgnoreList {
    /// A pointer at frame point `p` (`pointer`): the box's buttons, the
    /// menu's rows (off the menu it closes), or a name selected and the
    /// selection's menu opened (✕). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let (sel, now) = (self.sel, self.now);
        let (sel_out, window) = (&mut self.sel, &mut self.window);
        let mut select = |row: i32| {
            // the list's keys start the tick's pulse over
            *sel_out = row;
            window.reset_tick_pulse(now);
        };
        list_point(p, self.mbox_visible.then_some(&mut self.mbox), self.menu.visible.then_some(&mut self.menu), &self.hits, sel, &mut select)
    }

    /// `Init`: the box (Yes/No, at (80, 115)) and the titled member menu,
    /// hidden.
    pub fn new(menu: usize) -> Self {
        IgnoreList { sel: 0, top: 0, count: 0, window: Window::default(), mbox: MBox::default(), mbox_visible: false, menu: MiniMenu::titled(menu), flash: KeyFlash::default(), hits: Hits::default(), now: 0 }
    }

    /// `OnMessage` 295 with the focus coming to it from outside, then
    /// `Close` (`UpdateMode` 26): the window set up and opening, the
    /// children hidden, the top of the list.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.init(vui, r, [320, 180], 340, 14, 0);
        self.window.open(true);
        self.flash.reset();
        self.mbox_visible = false;
        self.menu.visible = false;
        self.sel = 0;
        self.top = 0;
    }

    /// `m_focus`: the window has the focus while neither child has it.
    fn focused(&self) -> bool {
        !self.mbox_visible && !self.menu.visible
    }

    /// `OnKeyDown`/`OnKeyUp`, the focused child's while one is up.
    pub fn key(&mut self, menus: &Menus, ignore: &Ignore, words: &dyn Fn(i32) -> String, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<IgnoreUp> {
        if self.mbox_visible {
            if !down {
                return None;
            }
            // 268 / 260 from the box
            let answer = self.mbox.key(box_key(key)?, cues)?;
            self.mbox_visible = false;
            if let BoxAnswer::Yes(0x0B05_3004, id) = answer {
                return Some(IgnoreUp::Remove(id as u32));
            }
            return None;
        }
        if self.menu.visible {
            match self.menu.key(menus, key, down, cues) {
                ListUp::Action(msg, _) => {
                    // 261: the member menu's choice
                    self.menu.visible = false;
                    cues.push(13);
                    if msg == 0x0B05_3004
                        && let Some(e) = ignore.list.get(self.sel as usize)
                    {
                        let title = String::from_utf16_lossy(&self.menu.title);
                        let text = words(614).replacen("%S", &title, 1).replacen("%s", &title, 1);
                        self.mbox.prompt(&wide(&text), 1, msg as i32, e.id as i32, now);
                        self.mbox_visible = true;
                    }
                }
                ListUp::Back => self.menu.visible = false,
                _ => {}
            }
            return None;
        }
        self.window.reset_tick_pulse(now);
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
            13 | 134 => {
                if let Some(e) = ignore.list.get(self.sel.max(0) as usize).filter(|_| self.sel < ignore.list.len() as i32) {
                    cues.push(13);
                    // ShowMenu(name)
                    self.menu.list.pos = [170, 50];
                    self.menu.title = wide(&e.name);
                    self.menu.reset_scroll();
                    self.menu.visible = true;
                    self.menu.focus(now);
                } else {
                    cues.push(10);
                }
            }
            27 | 132 => {
                cues.push(8);
                return Some(IgnoreUp::Close);
            }
            _ => {}
        }
        None
    }

    /// `Move(d)`.
    fn step(&mut self, d: i32, count: i32, cues: &mut Vec<i32>) {
        if self.sel + d >= count && count > 0 {
            cues.push(9);
            self.sel = count - 1;
        } else if self.sel + d < 0 || count == 0 {
            cues.push(9);
            self.sel = 0;
        } else {
            self.sel += d;
            cues.push(11);
        }
        if self.top + 11 < self.sel {
            self.top = self.sel - 11;
        }
        if self.sel < self.top {
            self.top = self.sel;
        }
    }

    /// `Clamp`: the page and the selection kept inside a list that shrank.
    fn clamp(&mut self, count: i32) {
        if count - 12 < self.top && count > 12 {
            self.top -= 1;
        } else if count == 12 {
            self.top = 0;
        }
        if self.top < 0 {
            self.top = 0;
        }
        if self.sel >= count {
            self.sel -= 1;
        }
        if self.sel < 0 {
            self.sel = 0;
        }
        self.count = count;
    }

    /// `OnDrawSelf`: the black band, the window titled 612 with its legend,
    /// the framed column with its scroll bar and twelve names (the selected
    /// one ticked); the held arrows repeat. Then the children: the box (whose
    /// timeout is returned, there is none here) and the menu.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, menus: &Menus, ignore: &Ignore, now: i64, cues: &mut Vec<i32>) {
        let count = ignore.list.len() as i32;
        self.clamp(count);
        self.now = now;
        self.hits.clear();
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let black = [0.0, 0.0, 0.0, 1.0];
        crate::ui::window_backdrop(vui, r, 642);
        self.window.set_title(Some(&wide(&w(612))));
        self.window.draw(vui, r);
        self.window.text(vui, r, &wide(&format!("|x{}   |t{}", w(212), w(161))), 20, 13, black, 0);
        let t = if count > 12 { self.top as f32 / (count - 12) as f32 } else { 0.0 };
        let width = self.window.size[0];
        self.window.draw_frame_vscroll(vui, r, 30, 0, width - 70, 12, t);
        let focus = self.focused();
        let mut row = 0;
        let mut n = self.top;
        while n < count {
            if n == self.sel {
                self.window.tick_r(vui, r, 45, row, focus);
            }
            let name = wide(&ignore.list[n as usize].name);
            self.window.text_box(vui, r, &name, 50, row, width - 95, 1, UI_COLORS[(n == self.sel) as usize], 2);
            self.hits.push(rows_rect(&self.window, vui, r, 50, row, width - 95, 1), n);
            row += 1;
            n += 1;
            if row >= 12 {
                break;
            }
        }
        if self.flash.pulse(138) {
            self.step(-1, count, cues);
        }
        if self.flash.pulse(139) {
            self.step(1, count, cues);
        }
        if self.mbox_visible {
            self.mbox.draw(vui, r, now);
        }
        if self.menu.visible {
            self.menu.draw(vui, r, menus, now, cues);
        }
    }
}

/// `VIBuddy`: a `VIPlayerRecord` (id, name, class, race, level), online,
/// and the world's name.
#[derive(Clone, Debug, PartialEq)]
pub struct Buddy {
    pub id: i32,
    pub name: String,
    pub class: i32,
    pub race: i32,
    pub level: i32,
    pub online: bool,
    pub world: String,
}

/// VIClient's buddy map and the request being answered (`m_p2BB10`,
/// `m_buddyRequestName`).
#[derive(Default)]
pub struct Buddies {
    pub map: BTreeMap<i32, Buddy>,
    pub request: u32,
    pub request_name: String,
}

impl Buddies {
    /// The dispatcher's 0x03D5 (a record in, or replacing the one of its id)
    /// and 0x03D6 (erased by id). False for any other opcode.
    pub fn recv(&mut self, op: u16, data: &[u8]) -> bool {
        let mut r = Reader::new(data);
        match op {
            0x03D5 => {
                let id = r.signed() as i32;
                let name = cstr(&r.string(24));
                let (class, race, level) = (r.signed() as i32, r.signed() as i32, r.signed() as i32);
                let online = r.u8() != 0;
                let world = r.wide(64);
                self.map.insert(id, Buddy { id, name, class, race, level, online, world });
            }
            0x03D6 => {
                let id = r.signed() as i32;
                self.map.remove(&id);
            }
            _ => return false,
        }
        true
    }

    /// `GetBuddy(n)`: the n-th in the map's order.
    pub fn get(&self, n: i32) -> Option<&Buddy> {
        self.map.values().nth(n.max(0) as usize).filter(|_| n >= 0)
    }

    pub fn count(&self) -> i32 {
        self.map.len() as i32
    }

    /// `AddBuddy(name)`: 870 with the name, unless it is oneself (word 679)
    /// or on the list already (592, with the listed name), said in red.
    pub fn add(&self, me: &str, name: &str) -> Result<Vec<u8>, (i32, String)> {
        if same_name(me, name) {
            return Err((679, String::new()));
        }
        if let Some(b) = self.map.values().find(|b| same_name(&b.name, name)) {
            return Err((592, b.name.clone()));
        }
        let mut w = Writer::op(870);
        w.string(name.as_bytes());
        Ok(w.data)
    }

    /// `RequestBuddy(n)`: 873 with the n-th buddy's id (signed); removes it.
    pub fn remove(&self, n: i32) -> Option<Vec<u8>> {
        let b = self.get(n)?;
        let mut w = Writer::op(873);
        w.signed(b.id as i64);
        Some(w.data)
    }

    /// `Msg076F`'s reads: the request's id and name.
    pub fn recv_request(&mut self, data: &[u8]) {
        let mut r = Reader::new(data);
        self.request = r.u32();
        self.request_name = cstr(&r.string(24));
    }

    /// 872 (0x368) with the request's id and a reason: 1 busy, 2 refused,
    /// 8 a full list; the request is gone.
    pub fn refuse(&mut self, reason: u8) -> Vec<u8> {
        let mut w = Writer::op(0x368);
        w.u32(self.request).u8(reason);
        self.request = u32::MAX;
        w.data
    }

    /// `AnswerBuddyRequest`: 0x367 with the request's id.
    pub fn accept(&self) -> Vec<u8> {
        let mut w = Writer::op(0x367);
        w.u32(self.request);
        w.data
    }
}

/// `GetRace(id)->m_shortStrID` and `GetClass(id)->m_shortStrID`, none out
/// of range.
fn race_short(race: i32) -> Option<i32> {
    [317, 318, 319, 320, 321, 322, 323, 324, 325, 1191].get(usize::try_from(race).ok()?).copied()
}

fn class_short(class: i32) -> Option<i32> {
    [303, 304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 315, 316, 1190].get(usize::try_from(class).ok()?).copied()
}

/// The column the buddy list is sorted by (the client's own: the game
/// lists its buddies by id). Names, races, classes and guilds (the last
/// column, `Buddy::world`) from A to Z as the window spells them, levels
/// from the lowest, the buddies online first; a tie by name, then by id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    #[default]
    Name,
    Online,
    Race,
    Class,
    Level,
    Guild,
}

impl Sort {
    /// The columns from the left.
    const ALL: [Sort; 6] = [Sort::Name, Sort::Online, Sort::Race, Sort::Class, Sort::Level, Sort::Guild];

    /// The next column to the right, round to the first.
    pub fn next(self) -> Sort {
        Sort::ALL[(self as usize + 1) % Sort::ALL.len()]
    }

    /// The column to the left, round to the last.
    pub fn prev(self) -> Sort {
        Sort::ALL[(self as usize + Sort::ALL.len() - 1) % Sort::ALL.len()]
    }
}

/// The columns' headings and where they are drawn (x off the window's left).
const HEADINGS: [(i32, i32); 6] = [(509, 40), (521, 170), (510, 240), (511, 302), (503, 364), (512, 426)];

/// The hit of a column's heading: `HEADING - column`.
const HEADING: i32 = -10;

/// The marks either side of the heading sorted by (the game's side arrow,
/// each pointing at the word, as Sandstorm's who list marks its column), and
/// their gap from it.
const MARKS: [&str; 2] = ["|}", "|{"];
const MARK_GAP: i32 = 3;

/// The buddies in the order shown: their ids, by `sort` (`word` spells a
/// race's or class's word).
pub fn order(sort: Sort, buddies: &Buddies, word: &dyn Fn(i32) -> String) -> Vec<i32> {
    let spelled = |id: Option<i32>| id.map_or_else(String::new, |id| word(id).to_lowercase());
    let mut v: Vec<&Buddy> = buddies.map.values().collect();
    v.sort_by(|a, b| {
        let by = match sort {
            Sort::Name => std::cmp::Ordering::Equal,
            Sort::Online => b.online.cmp(&a.online),
            Sort::Race => spelled(race_short(a.race)).cmp(&spelled(race_short(b.race))),
            Sort::Class => spelled(class_short(a.class)).cmp(&spelled(class_short(b.class))),
            Sort::Level => a.level.cmp(&b.level),
            Sort::Guild => a.world.to_lowercase().cmp(&b.world.to_lowercase()),
        };
        by.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())).then(a.id.cmp(&b.id))
    });
    v.into_iter().map(|b| b.id).collect()
}

/// What the buddy list asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum BuddyUp {
    /// `DeclineBuddyRequest`: `ClosePanel6D0`, back to the start menu
    Close,
    /// 304: the edit box holding "/t name "
    Tell(String),
    /// Mail: the gram window (not ported)
    Mail(String),
    /// the box's yes to Remove: `RequestBuddy(sel)`
    Remove(i32),
    /// the request's box: yes (`AnswerBuddyRequest`), no or timed out
    /// (`RejectBuddyRequest`)
    Accept,
    Reject,
}

/// Not the game's: the buddy list's rows in a scrolling frame, as the who
/// list's and the guild's roster's are (the user, 2026-10-29: "so it matches
/// the visual style of the other two menus"), at the roster's x (20) and
/// width (the window's less 50), round window rows 2 to 11.
const ROWS_FRAME: [i32; 2] = [20, 50];

/// `VIWndBuddy`.
pub struct BuddyList {
    /// `m_p30`: the list is drawn (not while a request is up)
    shown: bool,
    sel: i32,
    top: i32,
    count: i32,
    window: Window,
    pub mbox: MBox,
    pub mbox_visible: bool,
    pub menu: MiniMenu,
    flash: KeyFlash,
    /// the buddies drawn, by index, and the headings (`HEADING - column`),
    /// and when (for a pointer)
    hits: Hits,
    now: i64,
    /// the column the buddies are sorted by (the client's, from the name
    /// each time the list opens)
    pub sort: Sort,
    /// the buddies' ids in the order shown, as last drawn (`order`)
    view: Vec<i32>,
    /// the place in the game's (id) order of the buddy Remove's box asks
    /// about
    removing: i32,
}

impl BuddyList {
    /// A pointer at frame point `p` (`pointer`): the box's buttons (the
    /// request's too), the menu's rows (off the menu it closes), or a buddy
    /// selected and the selection's menu opened (✕); a heading sorts by its
    /// column (the client's). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        // a heading: the buddies by its column, the selection left at its
        // place; the column already sorted by is nothing
        if !self.mbox_visible
            && !self.menu.visible
            && let Some(h) = self.hits.at(p).filter(|&h| h <= HEADING)
        {
            let sort = Sort::ALL[(HEADING - h) as usize];
            if sort == self.sort {
                return Vec::new();
            }
            self.sort = sort;
            return vec![MOVED];
        }
        let (sel, now) = (self.sel, self.now);
        let (sel_out, window) = (&mut self.sel, &mut self.window);
        let mut select = |row: i32| {
            // `Move` starts the tick's pulse over
            *sel_out = row;
            window.reset_tick_pulse(now);
        };
        list_point(p, self.mbox_visible.then_some(&mut self.mbox), self.menu.visible.then_some(&mut self.menu), &self.hits, sel, &mut select)
    }

    pub fn new(menu: usize) -> Self {
        BuddyList { shown: false, sel: 0, top: 0, count: 0, window: Window::default(), mbox: MBox::default(), mbox_visible: false, menu: MiniMenu::titled(menu), flash: KeyFlash::default(), hits: Hits::default(), now: 0, sort: Sort::default(), view: Vec::new(), removing: 0 }
    }

    /// The buddy shown at `n` (the game's `GetBuddy(n)`, in the client's
    /// order as last drawn).
    fn buddy<'a>(&self, buddies: &'a Buddies, n: i32) -> Option<&'a Buddy> {
        buddies.map.get(self.view.get(usize::try_from(n).ok()?)?)
    }

    /// `OnMessage` 295 then `Close` (`UpdateMode` 18): the window, titled
    /// 513, opening; shown, the children hidden, the top of the list; the
    /// buddies sorted by name again (the client's).
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.init(vui, r, [320, 180], 600, 14, 0);
        self.window.set_title(Some(&wide(vui.words.get(513, vui.language))));
        self.window.open(true);
        self.flash.reset();
        self.shown = true;
        self.mbox_visible = false;
        self.menu.visible = false;
        self.sel = 0;
        self.top = 0;
        self.sort = Sort::default();
    }

    /// `ShowRequest` (`UpdateMode` 17): the list hidden, the box with words
    /// 519, the name and 520, 45 seconds to answer.
    pub fn show_request(&mut self, words: &dyn Fn(i32) -> String, name: &str, now: i64) {
        let text = format!("{}\n{}\n\n{}", words(519), name, words(520));
        self.shown = false;
        self.menu.visible = false;
        self.mbox.prompt(&wide(&text), 1, 0, 0, now);
        self.mbox.timeout = now + 45000;
        self.mbox_visible = true;
    }

    /// `HideChildren`.
    pub fn hide(&mut self) {
        self.mbox_visible = false;
        self.menu.visible = false;
        self.shown = false;
    }

    /// 268 / 260 from the box: p1 0 the request, 1 the removal.
    fn answered(&mut self, a: BoxAnswer) -> Option<BuddyUp> {
        self.mbox_visible = false;
        match a {
            BoxAnswer::Yes(0, _) => Some(BuddyUp::Accept),
            BoxAnswer::Yes(1, _) => Some(BuddyUp::Remove(self.removing)),
            BoxAnswer::No(0, _) => Some(BuddyUp::Reject),
            _ => None,
        }
    }

    /// `OnKeyDown`/`OnKeyUp`, the focused child's while one is up.
    pub fn key(&mut self, menus: &Menus, buddies: &Buddies, words: &dyn Fn(i32) -> String, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<BuddyUp> {
        if self.mbox_visible {
            if !down {
                return None;
            }
            let a = self.mbox.key(box_key(key)?, cues)?;
            return self.answered(a);
        }
        if self.menu.visible {
            match self.menu.key(menus, key, down, cues) {
                ListUp::Action(msg, _) => {
                    // 261: the mini menu's choice
                    self.menu.visible = false;
                    cues.push(13);
                    let shown = self.buddy(buddies, self.sel);
                    let name = shown.map(|b| b.name.clone()).unwrap_or_default();
                    match msg {
                        0x0B05_0001 => return Some(BuddyUp::Tell(name)),
                        0x0B05_0002 => return Some(BuddyUp::Mail(name)),
                        0x0B05_0003 => {
                            // RequestBuddy(n) takes its place in the game's order
                            self.removing = shown.and_then(|b| buddies.map.keys().position(|&id| id == b.id)).map_or(-1, |n| n as i32);
                            self.mbox.prompt(&wide(&words(518)), 1, 1, 0, now);
                            self.mbox_visible = true;
                        }
                        _ => {}
                    }
                }
                ListUp::Back => self.menu.visible = false,
                _ => {}
            }
            return None;
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
            27 | 132 => {
                cues.push(8);
                return Some(BuddyUp::Close);
            }
            13 | 134 => {
                if let Some(b) = self.buddy(buddies, self.sel) {
                    cues.push(13);
                    // ShowMenu(name)
                    self.menu.list.pos = [170, 50];
                    self.menu.title = wide(&b.name);
                    self.menu.reset_scroll();
                    self.menu.visible = true;
                    self.menu.focus(now);
                } else {
                    cues.push(10);
                }
            }
            19 | 138 => self.flash.press(138),
            20 | 139 => self.flash.press(139),
            // the client's sort: left and L1 the column before, right and R1
            // the one after, the selection left at its place in the list
            136 | 140 => {
                self.sort = self.sort.prev();
                self.window.reset_tick_pulse(now);
                cues.push(11);
            }
            137 | 142 => {
                self.sort = self.sort.next();
                self.window.reset_tick_pulse(now);
                cues.push(11);
            }
            _ => {}
        }
        None
    }

    /// `Move(d)`.
    fn step(&mut self, d: i32, n: i32, now: i64, cues: &mut Vec<i32>) {
        if self.sel + d >= n && n > 0 {
            cues.push(9);
            self.sel = n - 1;
        } else if self.sel + d < 0 || n == 0 {
            cues.push(9);
            self.sel = 0;
        } else {
            self.sel += d;
            cues.push(11);
        }
        if self.top + 9 < self.sel {
            self.top = self.sel - 9;
        }
        if self.sel < self.top {
            self.top = self.sel;
        }
        self.window.reset_tick_pulse(now);
    }

    /// `Clamp`: one step back up when the list shrank under the page or
    /// the selection.
    fn clamp(&mut self, n: i32) {
        if (n - 10 < self.top && n >= 11) || self.sel >= n {
            self.top = (self.top - 1).max(0);
            self.sel = (self.sel - 1).max(0);
        }
        self.count = n;
    }

    /// `OnDrawSelf` while shown: the black band, the window, the Select and
    /// Exit legend (and the client's left/right Sort), the column headers
    /// (the one sorted by between its marks) and ten rows of name, online, race,
    /// class, level and world, the selected one blue with the pulsing arrow
    /// (in the client's scrolling frame, `ROWS_FRAME`); the held arrows
    /// repeat. Then the box (its timeout answered here) and the menu.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, menus: &Menus, buddies: &Buddies, now: i64, cues: &mut Vec<i32>) -> Option<BuddyUp> {
        self.now = now;
        self.hits.clear();
        if self.shown {
            self.draw_list(vui, r, buddies, now, cues);
        }
        let mut up = None;
        if self.mbox_visible
            && let Some(a) = self.mbox.draw(vui, r, now)
        {
            up = self.answered(a);
        }
        if self.menu.visible {
            self.menu.draw(vui, r, menus, now, cues);
        }
        up
    }

    fn draw_list(&mut self, vui: &Vui, r: &mut dyn Raster, buddies: &Buddies, now: i64, cues: &mut Vec<i32>) {
        let n = buddies.count();
        self.clamp(n);
        self.view = order(self.sort, buddies, &|id| vui.words.get(id, vui.language).to_string());
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        let black = [0.0, 0.0, 0.0, 1.0];
        let blue = [10.0 / 255.0, 50.0 / 255.0, 70.0 / 255.0, 1.0];
        crate::ui::window_backdrop(vui, r, 641);
        self.window.draw(vui, r);
        let left = self.window.top_left()[0];
        let mut p = self.window.row_center(vui, r, 13);
        p[0] = left + 40;
        p[1] -= 2;
        vui.blit(r, 66, [p[0], p[1] - 6]);
        r.draw_string([p[0] + 20, p[1] - 10], black, &word(212));
        p[0] += 100;
        vui.blit(r, 65, [p[0], p[1] - 6]);
        r.draw_string([p[0] + 20, p[1] - 10], black, &word(161));
        // the client's left and right Sort (the d-pad's arms inline, as the
        // who list's legend has them on Sandstorm), centred on the words'
        let half = r.point_size() >> 1;
        let sort = wide(&format!("|v|V{}", crate::wholist::SORT));
        self.window.draw_text_internal(vui, r, &sort, p[0] + 100, p[1] - 10 + half, black, 0);
        let y0 = self.window.row_center(vui, r, 0)[1] - 4;
        let width = self.window.size[0];
        // the headings, each a pointer's sort, out to the next; the one
        // sorted by between its marks (the client's)
        for (k, (id, x)) in HEADINGS.into_iter().enumerate() {
            r.draw_string([left + x, y0], black, &word(id));
            let end = HEADINGS.get(k + 1).map_or(width - 40, |h| h.1);
            self.hits.push(rows_rect(&self.window, vui, r, x, 0, end - x, 1), HEADING - k as i32);
        }
        let (id, x) = HEADINGS[self.sort as usize];
        let (before, after) = (wide(MARKS[0]), wide(MARKS[1]));
        let at = left + x;
        let before_x = at - MARK_GAP - self.window.text_width(vui, r, &before);
        self.window.draw_text_internal(vui, r, &before, before_x, y0 + half, black, 0);
        let after_x = at + r.string_width(&word(id)) + MARK_GAP;
        self.window.draw_text_internal(vui, r, &after, after_x, y0 + half, black, 0);
        // the client's: the ten rows in the scrolling frame the who list and
        // the guild's roster have (`DrawFrameVScroll`, the guild's place and
        // width), the thumb at the page shown
        let t = if n > 10 { self.top as f32 / (n - 10) as f32 } else { 0.0 };
        self.window.draw_frame_vscroll(vui, r, ROWS_FRAME[0], 2, width - ROWS_FRAME[1], 10, t);
        for i in 0..10 {
            let k = i + self.top;
            let y = self.window.row_center(vui, r, i + 2)[1] - 10;
            let c = if k == self.sel {
                vui.blit_ex(r, 89, [left + 40 - 16, y + 4], 3, self.window.tick_pulse(vui.time), 0);
                blue
            } else {
                black
            };
            let Some(b) = self.buddy(buddies, k) else { break };
            self.hits.push(rows_rect(&self.window, vui, r, 40, i + 2, width - 80, 1), k);
            r.draw_string([left + 40, y], c, &wide(&b.name));
            r.draw_string([left + 170, y], c, &word(if b.online { 21 } else { 22 }));
            let Some(race) = race_short(b.race) else { continue };
            r.draw_string([left + 240, y], c, &word(race));
            let Some(class) = class_short(b.class) else { continue };
            r.draw_string([left + 302, y], c, &word(class));
            r.draw_string([left + 364, y], c, &wide(&(b.level as u32).to_string()));
            // the world: a text box placed off the window's corner twice over,
            // as shipped
            self.window.text_box(vui, r, &wide(&b.world), left + 406, i + 2, width - 446, 1, c, 2);
        }
        if self.flash.pulse(138) {
            self.step(-1, n, now, cues);
        }
        if self.flash.pulse(139) {
            self.step(1, n, now, cues);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(name: &str, id: u32) -> Vec<u8> {
        let mut w = Writer::default();
        w.string(name.as_bytes()).u32(id);
        w.data
    }

    /// 0x1003 appends, 0x1005 appends a counted list, 0x1006 renames by id,
    /// 0x1004 erases by id; an id that is not there changes nothing; any
    /// other opcode is not the list's.
    #[test]
    fn the_ignore_list_follows_its_opcodes() {
        let mut ig = Ignore::default();
        assert!(ig.recv(0x1003, &row("Alpha", 7)));
        let mut list = Writer::default();
        list.signed(2);
        list.bytes(&row("Bravo", 9)).bytes(&row("Charlie", 3));
        ig.recv(0x1005, &list.data);
        assert_eq!(ig.list.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Alpha", "Bravo", "Charlie"], "arrival order, not id order");
        ig.recv(0x1006, &row("Bob", 9));
        ig.recv(0x1004, &3u32.to_le_bytes());
        ig.recv(0x1004, &42u32.to_le_bytes());
        assert_eq!(ig.list, vec![IgnoreRow { name: "Alpha".into(), id: 7 }, IgnoreRow { name: "Bob".into(), id: 9 }]);
        assert!(!ig.recv(0x1001, &[]));
        // Remove is any case and by id, raw
        assert_eq!(ig.remove("bob"), Some(vec![0x02, 0x10, 9, 0, 0, 0]));
        assert_eq!(ig.remove("Zed"), None);
        assert_eq!(Ignore::add("Zed"), vec![0x00, 0x10, 3, 0, 0, 0, b'Z', b'e', b'd']);
    }

    fn buddy(id: i64, name: &str, online: bool, world: &str) -> Vec<u8> {
        let mut w = Writer::default();
        w.signed(id).string(name.as_bytes()).signed(6).signed(0).signed(12).u8(online as u8).wide(world);
        w.data
    }

    /// 0x03D5 keeps the map in id order and replaces a record of the same
    /// id; 0x03D6 erases one; RequestBuddy sends the id zigzagged; AddBuddy
    /// refuses oneself and a listed name in any case.
    #[test]
    fn the_buddy_map_is_in_id_order() {
        let mut b = Buddies::default();
        b.recv(0x03D5, &buddy(40, "Zed", false, ""));
        b.recv(0x03D5, &buddy(5, "Amy", true, "Tunaria"));
        b.recv(0x03D5, &buddy(40, "Zed", true, "Odus"));
        assert_eq!(b.count(), 2);
        assert_eq!(b.get(0).unwrap().name, "Amy");
        let z = b.get(1).unwrap();
        assert!(z.online && z.world == "Odus" && z.level == 12 && z.class == 6, "{z:?}");
        assert_eq!(b.get(2), None);
        assert_eq!(b.get(-1), None);
        assert_eq!(b.remove(1), Some(vec![0x69, 0x03, 80]));
        assert_eq!(b.add("Me", "me"), Err((679, String::new())));
        assert_eq!(b.add("Me", "AMY"), Err((592, "Amy".into())));
        assert!(b.add("Me", "Bob").is_ok());
        b.recv(0x03D6, &[80]);
        assert_eq!(b.count(), 1);
        assert!(!b.recv(0x03D7, &[]));
        // a request answered, then refused: the id goes with each, then is gone
        let mut req = 0x1234u32.to_le_bytes().to_vec();
        req.extend([4, 0, 0, 0]);
        req.extend(b"Kim\0");
        b.recv_request(&req);
        assert_eq!((b.request, b.request_name.as_str()), (0x1234, "Kim"));
        assert_eq!(b.accept(), vec![0x67, 0x03, 0x34, 0x12, 0, 0]);
        assert_eq!(b.refuse(2), vec![0x68, 0x03, 0x34, 0x12, 0, 0, 2]);
        assert_eq!(b.request, u32::MAX);
    }

    /// The message box's timeout cancels it once passed, and only once.
    #[test]
    fn a_request_times_out() {
        let mut m = MBox::default();
        m.prompt(&[], 1, 0, 0, 0);
        m.timeout = 45000;
        assert!(m.timeout != -1);
        let mut l = BuddyList::new(0);
        l.show_request(&|_| String::new(), "Kim", 0);
        assert_eq!(l.mbox.timeout, 45000);
        assert!(l.mbox_visible && !l.shown);
        // the answer a timeout gives is the box's No
        assert_eq!(l.answered(BoxAnswer::No(0, 0)), Some(BuddyUp::Reject));
        assert_eq!(l.answered(BoxAnswer::No(1, 0)), None, "a removal box's no only closes it");
        assert_eq!(l.answered(BoxAnswer::Yes(1, 0)), Some(BuddyUp::Remove(0)));
    }

    /// The first point of the frame (on a 4-pixel grid) a pointer finds a
    /// box's button at, by asking `point`.
    fn find_button(point: &mut dyn FnMut([f32; 2]) -> Vec<i32>) -> Option<[f32; 2]> {
        (0..112).flat_map(|y| (0..160).map(move |x| [x as f32 * 4.0, y as f32 * 4.0])).find(|&p| point(p) == vec![crate::pointer::key::CROSS])
    }

    fn one_menu() -> (Menus, usize) {
        use crate::client_image::{Menu, MenuEntry};
        let mut menus = Menus::default();
        let m = menus.push(Menu { x00: -1, x04: -1, entries: (0..3).map(|k| MenuEntry { word: 10 + k, msg: 0x0B05_0001 + k as u32, id: k, ..Default::default() }).collect() });
        (menus, m)
    }

    /// A pointer on the buddy list: a row selected, then its menu opened
    /// (✕); the menu's rows used at once; off the menu it closes (△) with
    /// the row under the pointer selected; under a request only the box's
    /// buttons are there.
    #[test]
    fn a_pointer_on_the_buddy_list() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        let vui = vui();
        let mut r = NullRaster;
        let (menus, m) = one_menu();
        let mut b = Buddies::default();
        for k in 0..3 {
            b.recv(0x03D5, &buddy(k + 1, &format!("B{k}"), true, "W"));
        }
        let mut l = BuddyList::new(m);
        l.open(&vui, &r);
        let mut cues = Vec::new();
        l.draw(&vui, &mut r, &menus, &b, 0, &mut cues);
        // ten rows from the window's row 2
        let row = |l: &BuddyList, i: i32| rows_rect(&l.window, &vui, &NullRaster, 40, i + 2, 400, 1);
        assert_eq!(l.point(mid(row(&l, 2))), vec![MOVED]);
        assert_eq!(l.sel, 2);
        assert_eq!(l.point(mid(row(&l, 2))), vec![key::CROSS]);
        assert!(l.point(mid(row(&l, 3))).is_empty(), "past the list");
        // the headings' row: Name's (sorted by already) is nothing
        assert!(l.point(mid(rows_rect(&l.window, &vui, &NullRaster, 40, 0, 130, 1))).is_empty(), "the Name heading");
        // ✕: the member menu; drawn, its rows are used at once
        l.key(&menus, &b, &|_| String::new(), 134, true, 0, &mut cues);
        assert!(l.menu.visible);
        l.draw(&vui, &mut r, &menus, &b, 16, &mut cues);
        // DrawTitled: the rows 19 apart from 48 under the menu's corner
        let at = l.menu.list.pos;
        assert_eq!(l.point([(at[0] + 60) as f32, (at[1] + 48 + 19) as f32]), vec![key::CROSS]);
        assert_eq!(l.menu.list.sel, 1);
        assert!(l.point([(at[0] + 60) as f32, (at[1] + 4) as f32]).is_empty(), "the menu's frame");
        // off the menu, on row 0 left of it: closed, row 0 selected
        let r0 = row(&l, 0);
        assert!(r0[0] + 10 < at[0]);
        assert_eq!(l.point([r0[0] as f32 + 10.0, mid(r0)[1]]), vec![MOVED, key::TRIANGLE]);
        assert_eq!(l.sel, 0);
        // a request: the list is not drawn, the box takes its buttons
        l.menu.visible = false;
        l.show_request(&|_| String::new(), "Kim", 0);
        l.draw(&vui, &mut r, &menus, &b, 32, &mut cues);
        assert!(l.point(mid(row(&l, 1))).is_empty(), "no list under a request");
        assert!(l.point([630.0, 440.0]).is_empty(), "off the box: nothing");
        assert!(find_button(&mut |p| l.point(p)).is_some(), "the box's buttons");
        // negative control: no box, no list: nothing anywhere
        l.hide();
        l.draw(&vui, &mut r, &menus, &b, 48, &mut cues);
        assert!(find_button(&mut |p| l.point(p)).is_none());
    }

    /// The ignore list: twelve names a page; a name selected, then its menu
    /// (✕); the menu and the box as the buddy list's.
    #[test]
    fn a_pointer_on_the_ignore_list() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        let vui = vui();
        let mut r = NullRaster;
        let (menus, m) = one_menu();
        let mut ig = Ignore::default();
        for k in 0..14 {
            ig.list.push(IgnoreRow { name: format!("N{k}"), id: k });
        }
        let mut l = IgnoreList::new(m);
        l.open(&vui, &r);
        let mut cues = Vec::new();
        l.draw(&vui, &mut r, &menus, &ig, 0, &mut cues);
        let row = |l: &IgnoreList, i: i32| rows_rect(&l.window, &vui, &NullRaster, 50, i, 200, 1);
        assert_eq!(l.point(mid(row(&l, 11))), vec![MOVED]);
        assert_eq!(l.sel, 11);
        assert_eq!(l.point(mid(row(&l, 11))), vec![key::CROSS]);
        assert!(l.point(mid(row(&l, 12))).is_empty(), "the thirteenth name is on the next page");
        // scrolled: the rows are the names from the top
        l.top = 2;
        l.draw(&vui, &mut r, &menus, &ig, 16, &mut cues);
        assert_eq!(l.point(mid(row(&l, 0))), vec![MOVED]);
        assert_eq!(l.sel, 2);
        // the menu: its rows at once; off it (right of it, on a row) closed
        l.key(&menus, &ig, &|_| String::new(), 134, true, 0, &mut cues);
        assert!(l.menu.visible);
        l.draw(&vui, &mut r, &menus, &ig, 32, &mut cues);
        let at = l.menu.list.pos;
        assert_eq!(l.point([(at[0] + 60) as f32, (at[1] + 48) as f32]), vec![key::CROSS]);
        assert_eq!(l.menu.list.sel, 0);
        let r1 = row(&l, 1);
        assert_eq!(l.point([r1[2] as f32 - 4.0, mid(r1)[1]]), vec![MOVED, key::TRIANGLE]);
        assert_eq!(l.sel, 3);
        // the box: its buttons only
        l.menu.visible = false;
        l.mbox.prompt(&wide("Sure?"), 1, 0x0B05_3004, 5, 0);
        l.mbox_visible = true;
        l.draw(&vui, &mut r, &menus, &ig, 48, &mut cues);
        assert!(l.point([630.0, 440.0]).is_empty());
        assert!(find_button(&mut |p| l.point(p)).is_some());
    }

    /// The buddies for the sort's tests: in id order Zed, amy, Bob, cal.
    fn four() -> Buddies {
        let mut b = Buddies::default();
        for (id, name, race, class, level, online, world) in [(1, "Zed", 1, 5, 30, false, "Tunaria"), (2, "amy", 0, 13, 50, true, "Odus"), (3, "Bob", 2, 14, 30, true, "Tunaria"), (4, "cal", 2, 5, 10, false, "odus")] {
            b.map.insert(id, Buddy { id, name: name.into(), class, race, level, online, world: world.into() });
        }
        b
    }

    /// The test's own spelling of the game's words.
    fn spelled(id: i32) -> String {
        match id {
            308 => "Bard".into(),
            316 => "Wizard".into(),
            317 => "Human".into(),
            319 => "Dark Elf".into(),
            318 => "Elf".into(),
            1190 => "Alchemist".into(),
            _ => String::new(),
        }
    }

    /// The client's sort: names, races, classes and guilds from A to Z,
    /// case aside (races and classes as the window spells them), levels from
    /// the lowest, the buddies online first; a tie by name. The control:
    /// the game's own order is by id.
    #[test]
    fn the_buddy_list_sorts_by_a_column() {
        let b = four();
        let names = |sort| order(sort, &b, &spelled).into_iter().map(|id| b.map[&id].name.as_str()).collect::<Vec<_>>();
        assert_eq!(Sort::default(), Sort::Name);
        assert_eq!(names(Sort::Name), ["amy", "Bob", "cal", "Zed"]);
        assert_ne!(b.map.values().map(|b| b.name.as_str()).collect::<Vec<_>>(), names(Sort::Name), "the game's order is by id");
        assert_eq!(names(Sort::Online), ["amy", "Bob", "cal", "Zed"]);
        // Dark Elf (Bob, cal), Elf, Human
        assert_eq!(names(Sort::Race), ["Bob", "cal", "Zed", "amy"]);
        // Alchemist, Bard (cal, Zed), Wizard
        assert_eq!(names(Sort::Class), ["Bob", "cal", "Zed", "amy"]);
        // 10, 30 (Bob, Zed), 50
        assert_eq!(names(Sort::Level), ["cal", "Bob", "Zed", "amy"]);
        // Odus (amy, cal), Tunaria (Bob, Zed)
        assert_eq!(names(Sort::Guild), ["amy", "cal", "Bob", "Zed"]);
        assert_eq!(Sort::ALL.map(Sort::next), [Sort::Online, Sort::Race, Sort::Class, Sort::Level, Sort::Guild, Sort::Name]);
        assert_eq!(Sort::ALL.map(Sort::prev), [Sort::Guild, Sort::Name, Sort::Online, Sort::Race, Sort::Class, Sort::Level]);
    }

    /// Left and L1 sort by the column before, right and R1 by the one after,
    /// round past either end, the selection left at its place in the list
    /// (the buddy there now another); a heading sorts by its column, the
    /// same; the menu and Remove's box act on the buddy shown (Remove with
    /// its place in the game's order); the list opens by name again. The
    /// control: ○ does nothing to it.
    #[test]
    fn the_buddy_lists_keys_sort_it() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        let vui = vui();
        let mut r = NullRaster;
        let (menus, m) = one_menu();
        let b = four();
        let words = |_: i32| String::new();
        let mut cues = Vec::new();
        let mut l = BuddyList::new(m);
        l.open(&vui, &r);
        let shown = |l: &BuddyList| l.view.iter().map(|id| b.map[id].name.clone()).collect::<Vec<_>>();
        l.draw(&vui, &mut r, &menus, &b, 0, &mut cues);
        assert_eq!(shown(&l), ["amy", "Bob", "cal", "Zed"]);
        l.sel = 1;
        // left from Name: round to Guild
        l.key(&menus, &b, &words, 136, true, 0, &mut cues);
        l.draw(&vui, &mut r, &menus, &b, 16, &mut cues);
        assert_eq!((l.sort, shown(&l)), (Sort::Guild, vec!["amy".to_string(), "cal".into(), "Bob".into(), "Zed".into()]));
        assert_eq!((l.sel, l.buddy(&b, l.sel).unwrap().name.as_str()), (1, "cal"), "the place kept, the buddy there another");
        // L1: Level
        l.key(&menus, &b, &words, 140, true, 0, &mut cues);
        l.draw(&vui, &mut r, &menus, &b, 32, &mut cues);
        assert_eq!((l.sort, shown(&l)), (Sort::Level, vec!["cal".to_string(), "Bob".into(), "Zed".into(), "amy".into()]));
        // right, R1: Guild, then round to Name
        l.key(&menus, &b, &words, 137, true, 0, &mut cues);
        assert_eq!(l.sort, Sort::Guild);
        l.key(&menus, &b, &words, 142, true, 0, &mut cues);
        assert_eq!(l.sort, Sort::Name);
        // the control: ○ is nothing to it
        l.key(&menus, &b, &words, 135, true, 0, &mut cues);
        assert_eq!(l.sort, Sort::Name);
        // the Level heading: x 364 to Guild's 426
        let level = mid(rows_rect(&l.window, &vui, &NullRaster, 364, 0, 62, 1));
        l.sel = 2;
        assert_eq!(l.point(level), vec![MOVED]);
        l.draw(&vui, &mut r, &menus, &b, 48, &mut cues);
        assert_eq!((l.sort, l.sel), (Sort::Level, 2));
        assert!(l.point(level).is_empty(), "the column already sorted by: nothing");
        // Zed shown third (id 1, first in the game's order): ✕ then Tell
        assert_eq!(l.buddy(&b, 2).unwrap().name, "Zed");
        l.key(&menus, &b, &words, 134, true, 0, &mut cues);
        assert_eq!(l.key(&menus, &b, &words, 134, true, 0, &mut cues), Some(BuddyUp::Tell("Zed".into())));
        // ✕ then Remove (the menu's third row), and the box's yes
        l.key(&menus, &b, &words, 134, true, 0, &mut cues);
        l.draw(&vui, &mut r, &menus, &b, 64, &mut cues);
        let at = l.menu.list.pos;
        assert_eq!(l.point([(at[0] + 60) as f32, (at[1] + 48 + 2 * 19) as f32]), vec![key::CROSS]);
        l.key(&menus, &b, &words, 134, true, 0, &mut cues);
        assert!(l.mbox_visible, "Remove asks");
        assert_eq!(l.answered(BoxAnswer::Yes(1, 0)), Some(BuddyUp::Remove(0)), "Zed's place in the game's order");
        // opened again: by name
        l.open(&vui, &r);
        l.draw(&vui, &mut r, &menus, &b, 80, &mut cues);
        assert_eq!((l.sort, shown(&l)), (Sort::Name, vec!["amy".to_string(), "Bob".into(), "cal".into(), "Zed".into()]));
    }

    /// The text a raster is handed, with its colour, and where it blits.
    #[derive(Default)]
    struct Texts(Vec<([i32; 2], [f32; 4], String)>, Vec<[i32; 2]>);
    impl crate::ui::Layout for Texts {}
    impl Raster for Texts {
        fn blit(&mut self, _: u32, _: u32, at: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {
            self.1.push(at);
        }
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, at: [i32; 2], c: [f32; 4], s: &[u16]) {
            self.0.push((at, c, String::from_utf16_lossy(s)));
        }
        fn draw_centered_string(&mut self, at: [i32; 2], c: [f32; 4], s: &[u16], _: u32) {
            self.0.push((at, c, String::from_utf16_lossy(s)));
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

    /// What a raster is handed to stretch, and where it blits.
    #[derive(Default)]
    struct Blits {
        stretched: Vec<([i32; 2], [i32; 2])>,
        at: Vec<[i32; 2]>,
    }
    impl crate::ui::Layout for Blits {}
    impl Raster for Blits {
        fn blit(&mut self, _: u32, _: u32, at: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {
            self.at.push(at);
        }
        fn stretch_blit(&mut self, _: u32, _: u32, at: [i32; 2], size: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {
            self.stretched.push((at, size));
        }
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
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

    /// The legend's sort is the d-pad's left and right arms (drawn as
    /// icons, never as their codes) and Sort; the heading sorted by is
    /// between the game's side arrows, a gap from each end of its word, on
    /// the headings' row. The control: another column, the marks go with it.
    #[test]
    fn the_buddy_lists_sort_is_drawn() {
        use crate::pointer::testing::vui;
        let vui = vui();
        let (menus, m) = one_menu();
        let b = four();
        let mut cues = Vec::new();
        let mut l = BuddyList::new(m);
        l.open(&vui, &Texts::default());
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        let mut t = Texts::default();
        l.draw(&vui, &mut t, &menus, &b, 0, &mut cues);
        let left = l.window.top_left()[0];
        let sort = t.0.iter().find(|(_, _, s)| s == crate::wholist::SORT).unwrap_or_else(|| panic!("{:?}", t.0));
        // after the arms, Select's and Exit's 100 apart from x 40
        let arms = l.window.text_width(&vui, &t, &wide("|v|V"));
        assert!(arms > 0);
        assert_eq!(sort.0[0], left + 240 + arms, "Sort past the left and right arms");
        assert!(!t.0.iter().any(|(_, _, s)| s.contains('|')), "no glyph code drawn as text: {:?}", t.0);
        // the marks: blits a gap before and after the heading's word
        let marks = |l: &BuddyList, t: &Texts, k: usize| {
            let (id, x) = HEADINGS[k];
            let at = left + x;
            let before = at - MARK_GAP - l.window.text_width(&vui, t, &wide(MARKS[0]));
            let after = at + t.string_width(&wide(&word(id))) + MARK_GAP;
            let y = t.0.iter().find(|(p, _, _)| p[0] == at).map(|(p, _, _)| p[1]).expect("the heading");
            let near = |x: i32| t.1.iter().any(|p| p[0] == x && (p[1] - y).abs() <= 12);
            (near(before), near(after))
        };
        assert_eq!(marks(&l, &t, 0), (true, true), "Name between its marks: {:?}", t.1);
        assert_eq!(marks(&l, &t, 4), (false, false));
        l.sort = Sort::Level;
        let mut t = Texts::default();
        l.draw(&vui, &mut t, &menus, &b, 16, &mut cues);
        assert_eq!(marks(&l, &t, 4), (true, true), "Level's now");
        assert_eq!(marks(&l, &t, 0), (false, false), "and not Name's");
    }

    /// The buddy list's ten rows are in the scrolling frame the guild's
    /// roster has (its x and width), from the first row's top to the last's
    /// bottom; its thumb moves with the page shown. The control: the frame
    /// is where `DrawFrameVScroll` puts the roster's, and nothing else is
    /// stretched over the rows.
    #[test]
    fn the_buddy_lists_rows_are_in_a_scrolling_frame() {
        use crate::pointer::testing::vui;
        let vui = vui();
        let (menus, m) = one_menu();
        let mut b = Buddies::default();
        for k in 0..15 {
            b.recv(0x03D5, &buddy(k + 1, &format!("B{k:02}"), true, "W"));
        }
        let mut l = BuddyList::new(m);
        l.open(&vui, &Blits::default());
        let mut cues = Vec::new();
        let mut first = Blits::default();
        l.draw(&vui, &mut first, &menus, &b, 0, &mut cues);
        let h = vui.font_height(&first) - 4;
        let left = l.window.top_left()[0];
        let at = [left + ROWS_FRAME[0] - 4, l.window.row_center(&vui, &first, 2)[1] - 4 - 3];
        let size = [l.window.size[0] - ROWS_FRAME[1] + 8, 10 * h + 4];
        assert!(first.stretched.contains(&(at, size)), "the frame round the rows: {at:?} {size:?} in {:?}", first.stretched);
        assert!(at[1] < l.window.row_center(&vui, &first, 2)[1] && at[1] + size[1] > l.window.row_center(&vui, &first, 11)[1], "rows 2 to 11 inside");
        // the last page: the thumb lower down its bar
        l.sel = 14;
        l.top = 5;
        let mut last = Blits::default();
        l.draw(&vui, &mut last, &menus, &b, 16, &mut cues);
        let bar_x = at[0] + size[0] - 4;
        let thumb = |r: &Blits| r.at.iter().filter(|p| p[0] == bar_x - 1).map(|p| p[1]).max();
        assert!(thumb(&last) > thumb(&first), "the thumb down the bar: {:?} {:?}", thumb(&first), thumb(&last));
    }
}
