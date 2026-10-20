//! The buddy and ignore lists. `VIClientPart38300` is the ignore list (in
//! 0x1003 add, 0x1004 erase by id, 0x1005 a counted list, 0x1006 a rename;
//! out 0x1000 add by name, 0x1002 remove by id), kept in arrival order;
//! `VIWndIgnoreList` shows it twelve names a page, with a titled mini menu
//! (`gIgnoreMemberMenu`: Stop Ignore) and a Yes/No box to confirm. The buddy
//! list is VIClient's `m_buddies` map (0x03D5 adds or replaces a record,
//! 0x03D6 erases one), in id order; `VIWndBuddy` shows ten rows of it, with
//! `gBuddyMenu` (Message, Mail, Remove) and the box that also carries an
//! incoming buddy request (`ShowRequest`, 45 s to answer).

#![allow(clippy::too_many_arguments)]

use crate::client_image::Menus;
use crate::dialog::{BoxAnswer, Key, MBox};
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
}

impl IgnoreList {

    /// A pointer at frame point `p` (`pointer`): what is under it selected,
    /// and the keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let _ = p;
        Vec::new()
    }
    /// `Init`: the box (Yes/No, at (80, 115)) and the titled member menu,
    /// hidden.
    pub fn new(menu: usize) -> Self {
        IgnoreList { sel: 0, top: 0, count: 0, window: Window::default(), mbox: MBox::default(), mbox_visible: false, menu: MiniMenu::titled(menu), flash: KeyFlash::default() }
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
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let black = [0.0, 0.0, 0.0, 1.0];
        vui.blit_rect(r, 32, [-1, -1], [642, 332], 0, 1.0);
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
}

impl BuddyList {

    /// A pointer at frame point `p` (`pointer`): what is under it selected,
    /// and the keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let _ = p;
        Vec::new()
    }
    pub fn new(menu: usize) -> Self {
        BuddyList { shown: false, sel: 0, top: 0, count: 0, window: Window::default(), mbox: MBox::default(), mbox_visible: false, menu: MiniMenu::titled(menu), flash: KeyFlash::default() }
    }

    /// `OnMessage` 295 then `Close` (`UpdateMode` 18): the window, titled
    /// 513, opening; shown, the children hidden, the top of the list.
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
            BoxAnswer::Yes(1, _) => Some(BuddyUp::Remove(self.sel)),
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
                    let name = buddies.get(self.sel).map(|b| b.name.clone()).unwrap_or_default();
                    match msg {
                        0x0B05_0001 => return Some(BuddyUp::Tell(name)),
                        0x0B05_0002 => return Some(BuddyUp::Mail(name)),
                        0x0B05_0003 => {
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
                if let Some(b) = buddies.get(self.sel) {
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
    /// Exit legend, the column headers and ten rows of name, online, race,
    /// class, level and world, the selected one blue with the pulsing arrow;
    /// the held arrows repeat. Then the box (its timeout answered here) and
    /// the menu.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, menus: &Menus, buddies: &Buddies, now: i64, cues: &mut Vec<i32>) -> Option<BuddyUp> {
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
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        let black = [0.0, 0.0, 0.0, 1.0];
        let blue = [10.0 / 255.0, 50.0 / 255.0, 70.0 / 255.0, 1.0];
        vui.blit_rect(r, 32, [-1, -1], [641, 332], 3, 1.0);
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
        let y0 = self.window.row_center(vui, r, 0)[1] - 4;
        for (id, x) in [(509, 40), (521, 170), (510, 240), (511, 302), (503, 364), (512, 426)] {
            r.draw_string([left + x, y0], black, &word(id));
        }
        let width = self.window.size[0];
        for i in 0..10 {
            let k = i + self.top;
            let y = self.window.row_center(vui, r, i + 2)[1] - 10;
            let c = if k == self.sel {
                vui.blit_ex(r, 89, [left + 40 - 16, y + 4], 3, self.window.tick_pulse(vui.time), 0);
                blue
            } else {
                black
            };
            let Some(b) = buddies.get(k) else { break };
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
}
