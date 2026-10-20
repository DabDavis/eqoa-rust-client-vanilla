//! The spell book (CLIENT `viwndspellbook.cpp`, `VIWndSpellBook`): the
//! start menu's "Ability" entry (0x8010000, `VIWndGame` mode 5). A tab per
//! ten levels, six rows of the abilities shown on the tab (by display
//! index), the selected one's stats and description, a popup per ability
//! (Memorize, Arrange, Unmemorize All, Exit) and the hot-bar slots it is
//! put on (five, ten with the expansion's bit 1).
//!
//! What it asks of the server: `Send0033` (an ability's display index onto
//! a slot; the answer is 0x00B4), `Send003A` (Arrange: one display index
//! moved to another's place; 0x00B6), `Send00FE` (Unmemorize All).
//!
//! The popup is `wndlist::MiniMenu` (`VIWndMiniMenu`, drawn plain) over the
//! book's own four rows.
#![allow(clippy::too_many_arguments)]

use crate::client_image::Menus;
use crate::startmenu::KeyFlash;
use crate::wndlist::{ListUp, MiniMenu};
use net::dump::Ability;
use std::collections::BTreeMap;
use vi::ui::{FrameParam, Raster, Vui, Window, wide};

/// `sClassIcons`: a class's crest.
const CLASS_ICONS: [(i32, i32); 15] = [(0, 23), (1, 19), (2, 18), (3, 21), (4, 16), (5, 11), (6, 20), (7, 13), (8, 22), (9, 12), (10, 15), (11, 17), (12, 14), (13, 24), (14, 251)];

/// `sTargetWords` (`FindTargetWord`): a scope's word.
const TARGET_WORDS: [(i32, i64); 5] = [(144, 0), (145, 1), (79, 2), (469, 3), (470, 4)];

/// `gWeaponTypeWords` (`FindWeaponTypeWord`): an equip-mask bit's word.
const WEAPON_WORDS: [(i32, i32); 8] = [(342, 0), (343, 1), (344, 2), (345, 3), (346, 4), (348, 5), (347, 6), (339, 7)];

/// The popup's four entries (`Init`): word and message.
const POPUP: [(i32, u32); 4] = [(1016, 0x0804_0000), (200, 0x0808_0000), (1800, 0x0809_0000), (161, 0x0802_0000)];

/// `VIRec32_C97888`: a row.
#[derive(Clone, Copy, Debug, Default)]
pub struct Row {
    /// the ability (the map's key here, its node in the game)
    pub id: i64,
    pub flags: u32,
    pub back: i64,
    pub front: i64,
    pub valid: bool,
    pub index: i64,
}

/// What the book asks of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// `PlaySound`
    Sound(i32),
    /// `Send0033(index, slot)`
    Send0033(i64, i64),
    /// `Send003A(from, to)`
    Send003A(i64, i64),
    /// `Send00FE`
    Send00FE,
    /// a word to the chat box (`LootOption`)
    Word(i32),
    /// a word to the chat box in red (`AddText`)
    Red(i32),
    /// 260 to `VIWndGame`: back to the start menu
    Close,
}

/// `VIWndSpellBook`.
pub struct SpellBook {
    pub visible: bool,
    /// 0 the list, 1 arranging (one held), 2 choosing a slot
    pub mode: i32,
    pub sel: i32,
    top: i32,
    pub tab: i32,
    tab_count: i32,
    /// `m_x2C`: the row held for Arrange
    held_at: i32,
    /// `m_x34`: the slot chosen in mode 2
    pub slot: i32,
    /// `m_x3C`: the selection drawn
    x3c: bool,
    /// `m_x44`: it has the focus (the pointer blinks)
    x44: bool,
    fade: f32,
    fade_dir: i32,
    time: i64,
    held: Row,
    pub rows: Vec<Row>,
    /// (id, slot, index), by slot
    slots: Vec<(i64, i64, i64)>,
    text: Vec<u16>,
    desc: Vec<u16>,
    window: Window,
    flash: KeyFlash,
    popup: MiniMenu,
}

impl Default for SpellBook {
    fn default() -> Self {
        SpellBook {
            visible: false,
            mode: 0,
            sel: 0,
            top: 0,
            tab: 0,
            tab_count: 0,
            held_at: -1,
            slot: 0,
            x3c: true,
            x44: false,
            fade: 1.0,
            fade_dir: 1,
            time: 0,
            held: Row::default(),
            rows: Vec::new(),
            slots: Vec::new(),
            text: Vec::new(),
            desc: Vec::new(),
            window: Window::default(),
            flash: KeyFlash::default(),
            popup: MiniMenu::fixed(&POPUP.map(|(w, m)| (w, m, 0))),
        }
    }
}

impl SpellBook {

    /// A pointer at frame point `p` (`pointer`): what is under it selected,
    /// and the keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let _ = p;
        Vec::new()
    }
    /// `Open`, and the focus arriving from another window (295): the tab
    /// count for the expansion, the popup down, the fade, the window.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, expansion: u32, now: i64) {
        self.popup.visible = false;
        self.mode = 0;
        self.sel = 0;
        self.top = 0;
        self.tab = 0;
        self.visible = true;
        self.x3c = true;
        self.tab_count = if expansion & 2 != 0 { 6 } else { 5 };
        self.flash.reset();
        self.focus(vui, r, now);
    }

    /// 295 with the book as the new focus.
    fn focus(&mut self, vui: &Vui, r: &dyn Raster, now: i64) {
        self.fade = 1.0;
        self.x44 = true;
        self.fade_dir = 1;
        self.time = now;
        self.window.init(vui, r, [320, 180], 300, 1, 0);
        self.window.open(true);
    }

    /// The focus back from the popup (`SetActiveWindow(this)`).
    fn refocus(&mut self, now: i64) {
        self.popup.visible = false;
        self.fade = 1.0;
        self.x44 = true;
        self.fade_dir = 1;
        self.time = now;
        self.window.open(true);
    }

    /// `ResetFade`.
    fn reset_fade(&mut self, now: i64) {
        self.fade_dir = 1;
        self.fade = 1.0;
        self.time = now;
    }

    /// `Shown`: on this tab's ten levels, not an item, usable (`m_p14`).
    fn shown(&self, a: &Ability) -> bool {
        let (lo, hi) = (self.tab as i64 * 10 + 1, self.tab as i64 * 10 + 10);
        a.display_index < 252 && a.p14 != 0 && (lo..=hi).contains(&a.level)
    }

    /// `BuildSpells` (by display index, -1 last) and `BuildSlots` (the
    /// memorized ones, by slot).
    pub fn build(&mut self, abilities: &BTreeMap<i64, Ability>) {
        self.rows = abilities
            .values()
            .filter(|a| self.shown(a))
            .map(|a| Row { id: a.id, flags: 17, back: a.icon_back, front: a.icon_front, valid: a.valid != 0, index: a.display_index })
            .collect();
        // CompareSpells
        self.rows.sort_by(|x, y| match (x.index == -1, y.index == -1) {
            (true, _) => std::cmp::Ordering::Greater,
            (_, true) => std::cmp::Ordering::Less,
            _ => x.index.cmp(&y.index),
        });
        self.slots = abilities.values().filter(|a| a.valid != 0 && a.display_index < 252).map(|a| (a.id, a.slot, a.display_index)).collect();
        self.slots.sort_by_key(|s| s.1);
    }

    /// `BuildText`: the selected one's name and stats, and its description.
    fn build_text(&mut self, vui: &Vui, abilities: &BTreeMap<i64, Ability>) {
        self.text.clear();
        self.desc.clear();
        let Some(a) = self.rows.get(self.sel.max(0) as usize).filter(|_| self.sel >= 0).and_then(|r| abilities.get(&r.id)) else { return };
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        let mut s = format!("{}\n", a.name);
        // AddScope
        let scope = TARGET_WORDS.iter().rev().find(|t| t.1 == a.scope).map_or_else(|| a.scope.to_string(), |t| word(t.0));
        s += &format!("{}: {scope} ", word(465));
        s += &format!("{}: {} ", word(463), a.cast_time);
        s += "\n";
        s += &format!("{}: {} ", word(464), a.cooldown);
        s += &format!("{}: {} ", word(466), a.power_cost);
        s += "\n";
        // AddEquipReq
        s += &format!("{}: ", word(467));
        if a.equip_req == 0 {
            s += &format!("{} ", word(302));
        } else if a.equip_req & 0xFF == 255 {
            s += &format!("{} ", word(471));
        } else {
            for i in 0..8 {
                if (a.equip_req >> i) & 1 != 0
                    && let Some(w) = WEAPON_WORDS.iter().rev().find(|w| w.1 == i)
                {
                    s += &format!("{} ", word(w.0));
                }
            }
        }
        s += "\n";
        s += &format!("{}: {} ", word(503), a.level);
        s += &format!("{}: {:.1}", word(457), a.range);
        s += "\n";
        // AddRecords
        if !a.parts.is_empty() {
            s += &format!("{}: ", word(468));
            s += &a.parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
        }
        self.text = wide(&s);
        self.desc = wide(&a.desc);
    }

    /// A key, down or up (the game's codes): the popup's while it is up.
    pub fn key(&mut self, abilities: &BTreeMap<i64, Ability>, expansion: u32, key: i32, down: bool, now: i64) -> Vec<Out> {
        let mut out = Vec::new();
        if self.popup.visible {
            self.popup_key(abilities, key, down, now, &mut out);
            return out;
        }
        if !down {
            match key {
                19 | 138 => self.flash.release(138),
                20 | 139 => self.flash.release(139),
                _ => {}
            }
            return out;
        }
        match key {
            27 | 132 => {
                self.reset_fade(now);
                out.push(Out::Sound(8));
                match self.mode {
                    0 => out.push(Out::Close),
                    2 => self.show_list(now),
                    1 => {
                        self.x3c = true;
                        self.mode = 0;
                    }
                    _ => {}
                }
            }
            17 | 136 if self.mode == 0 => self.tab_by(-1, now, &mut out),
            18 | 137 if self.mode == 0 => self.tab_by(1, now, &mut out),
            19 | 138 => match self.mode {
                0 | 1 => self.flash.press(138),
                2 => self.slot_by(-1, expansion, now, &mut out),
                _ => {}
            },
            20 | 139 => match self.mode {
                0 | 1 => self.flash.press(139),
                2 => self.slot_by(1, expansion, now, &mut out),
                _ => {}
            },
            13 | 134 => match self.mode {
                0 => {
                    out.push(Out::Sound(13));
                    self.pick(now);
                }
                1 => {
                    out.push(Out::Sound(13));
                    self.place(now, &mut out);
                }
                2 => self.assign(now, &mut out),
                _ => {}
            },
            _ => {}
        }
        out
    }

    /// The popup's keys (`VIWndMiniMenu::OnKeyDown`/`OnKeyUp`) and what
    /// they send up (`VIWndSpellBook::OnMessage` 261 and 260).
    fn popup_key(&mut self, abilities: &BTreeMap<i64, Ability>, key: i32, down: bool, now: i64, out: &mut Vec<Out>) {
        let mut cues = Vec::new();
        let up = self.popup.key(&Menus::default(), key, down, &mut cues);
        out.extend(cues.into_iter().map(Out::Sound));
        match up {
            ListUp::Action(msg, _) => {
                let id = self.rows.get(self.sel.max(0) as usize).map_or(-1, |r| r.id);
                self.popup_message(abilities, msg, id, now, out);
            }
            ListUp::Back => self.refocus(now),
            _ => {}
        }
    }

    /// `OnMessage` 261: the popup's entry.
    fn popup_message(&mut self, abilities: &BTreeMap<i64, Ability>, msg: u32, id: i64, now: i64, out: &mut Vec<Out>) {
        let row = (self.sel >= 0).then(|| self.rows.get(self.sel as usize).copied()).flatten();
        match msg {
            0x0808_0000 => {
                // Arrange
                self.hold();
                self.x3c = false;
                self.refocus(now);
                self.mode = 1;
            }
            0x0804_0000 => {
                // Memorize
                self.popup.visible = false;
                if let Some(r) = row {
                    if r.valid {
                        // 272: LootOption(912)
                        out.push(Out::Word(912));
                        self.refocus(now);
                    } else if abilities.get(&id).is_none_or(|a| a.p14 == 0) {
                        out.push(Out::Red(1629));
                        self.refocus(now);
                    } else {
                        self.equip(now);
                    }
                } else {
                    self.refocus(now);
                }
            }
            0x0809_0000 => {
                // Unmemorize All
                if row.is_some() {
                    out.push(Out::Send00FE);
                    out.push(Out::Sound(13));
                }
                self.refocus(now);
            }
            _ => self.refocus(now),
        }
    }

    /// `Pick`: the popup beside the selected row.
    fn pick(&mut self, now: i64) {
        self.reset_fade(now);
        if self.sel >= 0 && (self.sel as usize) < self.rows.len() {
            self.popup.list.pos = [150, (self.sel - self.top) * 44 + 45];
            self.popup.list.sel = 0;
            self.popup.list.top = 0;
            self.popup.visible = true;
            self.x44 = false;
            self.flash.reset();
            self.popup.focus(now);
        }
    }

    /// `Hold`: the row to move.
    fn hold(&mut self) {
        self.held_at = self.sel;
        if let Some(r) = (self.sel >= 0).then(|| self.rows.get(self.sel as usize)).flatten() {
            self.held = *r;
        }
    }

    /// `Place`: the held row onto the selected one's display index.
    fn place(&mut self, now: i64, out: &mut Vec<Out>) {
        let n = self.rows.len() as i32;
        if self.held_at != self.sel && (0..n).contains(&self.held_at) && (0..n).contains(&self.sel) {
            out.push(Out::Send003A(self.rows[self.held_at as usize].index, self.rows[self.sel as usize].index));
        }
        self.mode = 0;
        self.x3c = true;
        self.reset_fade(now);
    }

    /// `Equip`: choose its slot.
    fn equip(&mut self, now: i64) {
        self.reset_fade(now);
        self.x3c = false;
        self.slot = 0;
        self.mode = 2;
        self.refocus(now);
    }

    /// `Assign`: 273 up (`Send0033(index, slot)`).
    fn assign(&mut self, now: i64, out: &mut Vec<Out>) {
        if self.sel >= 0
            && let Some(r) = self.rows.get_mut(self.sel as usize)
        {
            out.push(Out::Send0033(r.index, self.slot as i64));
            r.valid = true;
            self.show_list(now);
        }
    }

    /// `ShowList`.
    fn show_list(&mut self, now: i64) {
        self.reset_fade(now);
        self.x3c = true;
        self.slot = 0;
        self.mode = 0;
    }

    /// `Down`.
    fn down(&mut self, now: i64, out: &mut Vec<Out>) {
        let last = self.rows.len() as i32 - 1;
        if self.sel >= last {
            self.sel = 0;
        } else {
            self.sel += 1;
            if self.sel == last {
                self.flash.reset();
            }
        }
        out.push(Out::Sound(11));
        self.reset_fade(now);
    }

    /// `Up`.
    fn up(&mut self, now: i64, out: &mut Vec<Out>) {
        if self.sel <= 0 {
            self.sel = (self.rows.len() as i32 - 1).max(0);
        } else {
            self.sel -= 1;
            if self.sel == 0 {
                self.flash.reset();
            }
        }
        out.push(Out::Sound(11));
        self.reset_fade(now);
    }

    /// `Slot`: the slot chosen, five or ten.
    fn slot_by(&mut self, d: i32, expansion: u32, now: i64, out: &mut Vec<Out>) {
        let count = if expansion & 2 != 0 { 10 } else { 5 };
        if self.slot + d >= count {
            out.push(Out::Sound(9));
            self.slot = count - 1;
        } else if self.slot + d < 0 {
            out.push(Out::Sound(9));
            self.slot = 0;
        } else {
            out.push(Out::Sound(11));
            self.slot += d;
        }
        self.reset_fade(now);
        self.window.reset_tick_pulse(now);
    }

    /// `Tab`.
    fn tab_by(&mut self, d: i32, now: i64, out: &mut Vec<Out>) {
        self.sel = 0;
        self.top = 0;
        self.tab += d;
        if self.tab < 0 {
            self.tab = self.tab_count - 1;
        } else if self.tab >= self.tab_count {
            self.tab = 0;
        }
        out.push(Out::Sound(11));
        self.reset_fade(now);
    }

    /// `OnDrawSelf`, then the popup over it.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, abilities: &BTreeMap<i64, Ability>, class: i32, expansion: u32, now: i64) -> Vec<Out> {
        let mut out = Vec::new();
        if !self.visible {
            return out;
        }
        self.build(abilities);
        self.build_text(vui, abilities);
        let white = [1.0; 4];
        let black = [0.0, 0.0, 0.0, 1.0];
        let right = [449, 0];
        let dt = (now - self.time) as f32 * 0.001;
        self.time = now;
        vui.blit_rect(r, 32, [-1, -1], [642, 332], 0, 1.0);
        let fp = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
        vui.blit_fill_frame(r, [150, 50], [300, 177], &fp, 35, 0, 1.0, true);
        vui.blit_fill_frame(r, [150, 227], [300, 88], &fp, 35, 0, 1.0, true);
        let fp = FrameParam { tl: (36, 0), top: (37, 0), tr: (36, 1), right: (42, 1), br: (39, 1), bottom: (40, 0), bl: (39, 0), left: (42, 0) };
        let studs = FrameParam { tl: (0, 0), top: (38, 0), tr: (0, 0), right: (43, 1), br: (0, 0), bottom: (41, 0), bl: (0, 0), left: (43, 0) };
        vui.blit_rect(r, 51, [-1, 50], [140, 265], 0, 1.0);
        vui.blit_rect(r, 51, [461, 50], [189, 265], 0, 1.0);
        vui.blit_studded_horz(r, [-1, 39], 37, 38, 140, 1, 0, 1.0, 0);
        vui.blit_studded_horz(r, [-1, 315], 40, 41, 140, 1, 0, 1.0, 0);
        vui.blit_studded_horz(r, [461, 39], 37, 38, 190, 1, 0, 1.0, 0);
        vui.blit_studded_horz(r, [461, 315], 40, 41, 190, 1, 0, 1.0, 0);
        vui.blit_studded_frame(r, [139, 39], [322, 294], &fp, &studs, 2, 3, 0, 1.0, false);
        if self.sel - self.top >= 6 {
            self.top = self.sel - 5;
        } else if self.sel < self.top {
            self.top = self.sel;
        }
        let n = self.rows.len() as i32;
        let t = if n >= 7 { self.top as f32 / (n - 6) as f32 } else { 0.0 };
        self.window.draw_vscroll(vui, r, [137, 47], 270, t);
        if expansion & 2 != 0 {
            self.draw_slots10(vui, r, abilities);
        } else {
            self.draw_slots(vui, r, abilities);
        }
        self.draw_tabs(vui, r);
        self.window.init(vui, r, [300, 135], 280, 9, 0);
        let text = self.text.clone();
        self.window.text_box(vui, r, &text, 0, 0, 280, 9, black, 2);
        self.window.init(vui, r, [300, 270], 280, 4, 0);
        if self.mode == 0 {
            let desc = self.desc.clone();
            self.window.text_box(vui, r, &desc, 0, 0, 280, 4, black, 2);
        } else if self.mode == 2 {
            let s = wide(&format!("|x {}", vui.words.get(268, vui.language)));
            self.window.text_box(vui, r, &s, 0, 0, 280, 4, black, 2);
        }
        let mut p = [4, 50];
        let mut i = self.top;
        let mut k = 0;
        while i < n && k < 6 {
            let lit = self.x3c && i == self.sel && self.mode == 0;
            let row = self.rows[i as usize];
            self.draw_icon(vui, r, &row, p, lit, 1.0);
            if i == self.sel && self.mode == 1 {
                let held = self.held;
                self.draw_icon(vui, r, &held, [p[0] + 150, p[1]], false, 0.5);
            }
            p[1] += 44;
            i += 1;
            k += 1;
        }
        while i < 6 {
            let empty = Row { back: -1, flags: 1, ..Default::default() };
            self.draw_icon(vui, r, &empty, p, false, 1.0);
            p[1] += 44;
            i += 1;
        }
        let mut p = [right[0] + 60, right[1] + 262];
        for &(c, id) in &CLASS_ICONS {
            if c != class {
                continue;
            }
            let tex = vui.entry(id);
            vui.blit(r, 130, p);
            p[0] += 57 - tex.uv1[0] / 2;
            if class == 14 {
                if net::session::EXPANSION_FLAGS & 2 == 0 {
                    break;
                }
                p[1] += 1;
            }
            vui.blit(r, id, p);
        }
        let mut p = [right[0] + 25, right[1] + 260];
        vui.blit(r, 73, p);
        p = [p[0] + 10, p[1] + 34];
        vui.draw_centered_string_ex(r, &wide(vui.words.get(161, vui.language)), p[0], p[1], white, 0, true);
        if self.x44 {
            if self.fade_dir == -1 {
                self.fade = (self.fade - dt * 2.0).max(0.0);
                if self.fade == 0.0 {
                    self.fade_dir = 1;
                }
            } else {
                self.fade = (self.fade + dt * 3.0).min(1.0);
                if self.fade == 1.0 {
                    self.fade_dir = -1;
                }
            }
        } else {
            self.fade = 1.0;
        }
        if self.flash.pulse(138) {
            self.up(now, &mut out);
        }
        if self.flash.pulse(139) {
            self.down(now, &mut out);
        }
        if self.popup.visible {
            let mut cues = Vec::new();
            self.popup.draw(vui, r, &Menus::default(), now, &mut cues);
            out.extend(cues.into_iter().map(Out::Sound));
        }
        out
    }

    /// `DrawIcon`: the plate (with the icon's back), the icon, the pointer.
    fn draw_icon(&self, vui: &Vui, r: &mut dyn Raster, row: &Row, pos: [i32; 2], lit: bool, alpha: f32) {
        let mode = if alpha == 1.0 { 0 } else { 3 };
        if row.flags & 1 != 0 {
            if row.back == -1 {
                vui.blit_ability_icon(r, pos, 1, mode, alpha);
            } else {
                vui.blit_ability_icon(r, pos, 0, mode, alpha);
                vui.blit_id(r, row.back as u32, [pos[0] + 49, pos[1] + 6], mode, alpha, 0);
            }
        }
        if row.flags & 0x10 != 0 {
            // VIUI::Blit(id, at, size, zero)
            r.blit(row.front as u32, 0, [pos[0] + 62, pos[1] + 12], [0, 0], [22, 22], mode, alpha);
        }
        // (flag 0x20, the arrow bar, is never set on a row)
        if lit {
            vui.blit_ex(r, 86, [pos[0] + 111, pos[1] + 14], 3, self.fade, 0);
        }
    }

    /// `DrawTabs`: the bar of level tabs, darker to the right, the names
    /// starting at the tab shown.
    fn draw_tabs(&self, vui: &Vui, r: &mut dyn Raster) {
        let mut p = [0, 5];
        vui.blit(r, 62, p);
        p[0] += 37;
        let mut t = 0.0f32;
        for _ in 0..self.tab_count {
            vui.blit_ex(r, 63, p, 0, 1.0, 0);
            vui.stretch_blit_ex(r, 61, p, [73, 34], 3, t);
            p[0] += 73;
            t += 0.35 / (self.tab_count - 1) as f32;
        }
        p[0] -= 12;
        vui.blit_ex(r, 64, p, 0, 1.0, 0);
        let mut q = [0, 0];
        let mut tab = self.tab;
        for _ in 0..self.tab_count {
            if tab >= self.tab_count {
                break;
            }
            vui.draw_centered_string_ex(r, &wide(vui.words.get(1729 + tab, vui.language)), q[0] + 45, q[1] + 27, [1.0; 4], 1, true);
            tab = (tab + 1) % self.tab_count;
            q[0] += 72;
        }
    }

    /// `DrawSlots`: five bars, the chosen one yellow with the pointer, and
    /// the memorized names on theirs.
    fn draw_slots(&mut self, vui: &Vui, r: &mut dyn Raster, abilities: &BTreeMap<i64, Ability>) {
        let mut p = [464, 60];
        for i in 0..5 {
            let lit = self.mode == 2 && i == self.slot;
            let c = if lit { [1.0, 1.0, 0.0, 1.0] } else { [1.0; 4] };
            if lit {
                vui.blit_bar3(r, &[], c, p, 56, 58, 57, 175, 0, 1.0, 0, true);
                self.window.init(vui, r, [p[0] + 87, p[1] + 13], 175, 1, 0);
                self.window.tick_r(vui, r, 17, 0, true);
            } else {
                vui.blit_bar3(r, &[], c, p, 53, 55, 54, 175, 0, 1.0, 0, true);
            }
            p[1] += 39;
        }
        for &(id, slot, _) in &self.slots.clone() {
            let name = abilities.get(&id).map_or_else(Vec::new, |a| wide(&a.name));
            if slot < 5 {
                let lit = self.mode == 2 && slot == self.slot as i64;
                let c = if lit { [1.0, 1.0, 0.0, 1.0] } else { [1.0; 4] };
                let p = [464, 60 + slot as i32 * 39];
                self.window.init(vui, r, [p[0] + 87, p[1] + 13], 175, 1, 0);
                self.window.text_box(vui, r, &name, 5, 0, 165, 1, c, 11);
                if lit {
                    self.window.tick_r(vui, r, 17, 0, true);
                }
            }
        }
    }

    /// `DrawSlots10`: ten rows in a window, the names on theirs, the
    /// pointer at the chosen one.
    fn draw_slots10(&mut self, vui: &Vui, r: &mut dyn Raster, abilities: &BTreeMap<i64, Ability>) {
        self.window.init(vui, r, [545, 155], 190, 10, 1);
        self.window.open(true);
        let w = self.window.size[0] - 40;
        for i in 0..10 {
            self.window.text_box(vui, r, &[], 20, i, w, 1, [0.0, 0.0, 0.0, 1.0], 4);
        }
        for &(id, slot, _) in &self.slots.clone() {
            let name = abilities.get(&id).map_or_else(Vec::new, |a| wide(&a.name));
            if slot < 10 {
                let lit = self.mode == 2 && slot == self.slot as i64;
                let c = if lit { [1.0, 1.0, 0.0, 1.0] } else { [1.0; 4] };
                self.window.text_box(vui, r, &name, 20, slot as i32, w, 1, c, 11);
            }
        }
        if self.mode == 2 {
            self.window.tick_r(vui, r, 32, self.slot, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ability(id: i64, index: i64, level: i64, valid: bool, slot: i64) -> Ability {
        Ability { id, display_index: index, level, valid: valid as i64, slot, p14: 1, icon_back: -1, ..Default::default() }
    }

    fn book(list: &[Ability]) -> (SpellBook, BTreeMap<i64, Ability>) {
        let map: BTreeMap<i64, Ability> = list.iter().map(|a| (a.id, a.clone())).collect();
        let mut b = SpellBook { visible: true, tab_count: 6, ..Default::default() };
        b.build(&map);
        (b, map)
    }

    /// `Shown` and `CompareSpells`: this tab's levels, items (252 up) and
    /// unusable ones out, by display index with -1 last.
    #[test]
    fn rows_are_the_tabs_abilities_by_display_index() {
        let mut list = vec![ability(1, 5, 3, false, -1), ability(2, -1, 1, false, -1), ability(3, 2, 10, true, 4), ability(4, 1, 11, false, -1), ability(5, 252, 1, false, -1)];
        list.push(Ability { p14: 0, ..ability(6, 0, 1, false, -1) });
        let (mut b, map) = book(&list);
        assert_eq!(b.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![3, 1, 2]);
        b.tab = 1;
        b.build(&map);
        assert_eq!(b.rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![4]);
        // the memorized, by slot, whatever the tab
        assert_eq!(b.slots, vec![(3, 4, 2)]);
    }

    /// Memorize on an unmemorized row opens slot choice; X sends 0x0033
    /// with its display index and the slot; one already memorized says 912.
    #[test]
    fn memorize_sends_0033_with_index_and_slot() {
        let (mut b, map) = book(&[ability(1, 7, 1, false, -1), ability(2, 9, 2, true, 0)]);
        b.key(&map, 0xFFFF_FFFF, 134, true, 0);
        assert!(b.popup.visible);
        b.key(&map, 0xFFFF_FFFF, 134, true, 0);
        assert_eq!(b.mode, 2, "Memorize: Equip");
        for _ in 0..12 {
            b.key(&map, 0xFFFF_FFFF, 139, true, 0);
        }
        assert_eq!(b.slot, 9, "ten slots with the expansion, clamped");
        let out = b.key(&map, 0xFFFF_FFFF, 134, true, 0);
        assert_eq!(out, vec![Out::Send0033(7, 9)]);
        assert_eq!(b.mode, 0);
        // the memorized one
        b.sel = 1;
        b.key(&map, 0, 134, true, 0);
        let out = b.key(&map, 0, 134, true, 0);
        assert_eq!(out, vec![Out::Word(912)]);
        assert_ne!(b.mode, 2);
    }

    /// Arrange: hold one, move, X sends 0x003A (held index, target index);
    /// on itself nothing is sent.
    #[test]
    fn arrange_sends_003a() {
        let (mut b, map) = book(&[ability(1, 3, 1, false, -1), ability(2, 4, 1, false, -1), ability(3, 8, 1, false, -1)]);
        b.key(&map, 0, 134, true, 0);
        b.key(&map, 0, 139, true, 0); // the popup down to Arrange
        // the auto-repeat's first pulse, without drawing
        if b.popup.list.flash.pulse(139) {
            b.popup.list.scroll(1, 0, &mut Vec::new());
        }
        b.key(&map, 0, 134, true, 0);
        assert_eq!(b.mode, 1);
        b.sel = 2;
        let out = b.key(&map, 0, 134, true, 0);
        assert_eq!(out, vec![Out::Sound(13), Out::Send003A(3, 8)]);
        b.mode = 1;
        b.held_at = 2;
        let out = b.key(&map, 0, 134, true, 0);
        assert_eq!(out, vec![Out::Sound(13)], "placed on itself: nothing sent");
    }

    /// Escape: back from slot choice, then out of the book.
    #[test]
    fn escape_backs_out() {
        let (mut b, map) = book(&[ability(1, 3, 1, false, -1)]);
        b.mode = 2;
        assert_eq!(b.key(&map, 0, 27, true, 0), vec![Out::Sound(8)]);
        assert_eq!(b.mode, 0);
        assert_eq!(b.key(&map, 0, 27, true, 0), vec![Out::Sound(8), Out::Close]);
    }
}
