//! The scrolling lists (CLIENT `viwndlist.cpp`, `viwndmenu.cpp`,
//! `viwndminimenu.cpp`): `VIWndList`, up to six rows of an icon, a second
//! icon or the arrows, a label and a number, with a scroll bar; `VIWndMenu`,
//! a list over a `VIMenuDef` whose entries send their message or open their
//! sub-menu in a `VIWndMiniMenu` beside the row, and whose "move" mode swaps
//! two entries (277); and `VIWndMiniMenu`, the popup that grows out of its
//! corner.
//!
//! As in `startmenu`, the window tree's messages come back up as values
//! (`ListUp`), and the parent does what its `OnMessage` does. An icon is
//! there when the raster has its surface (`Raster::has`): the client's icon
//! cache (`VIClientPart06D50`) streams them, here ITEMICON.ESF is resident,
//! so an icon is either drawn or missing (the cache's state 2: the empty row
//! with the "?" mark).

#![allow(clippy::too_many_arguments)]

use crate::client_image::Menus;
use crate::startmenu::KeyFlash;
use vi::ui::{FrameParam, Raster, Vui, Window, wide};

/// `VIRec24_C6A8B8`: one row.
#[derive(Clone, Copy, Debug, Default)]
pub struct Row {
    /// 1 icon, 2 label, 4 number, 8 arrows, 0x10 second icon, 0x20 the "X"
    /// mark; 0 an empty row
    pub flags: u32,
    pub icon: i32,
    pub icon2: i32,
    /// `SetItem` stores it; `DrawItem` never reads it
    #[allow(dead_code)]
    pub x0c: i32,
    pub word: i32,
    pub number: i32,
}

/// A message up from a list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ListUp {
    None,
    /// 260
    Back,
    /// 261: an entry's message and id
    Action(u32, i32),
    /// 277: the moved entry's id and the one it was dropped on
    Swap(i32, i32),
}

/// `VIWndList`.
pub struct WndList {
    /// the move mode (`m_x60`): `cur` is carried beside the selection
    pub x60: bool,
    /// the selection shown (`m_x64`)
    pub x64: bool,
    pub sel: i32,
    pub top: i32,
    count: i32,
    pub pos: [i32; 2],
    pub sel_row: i32,
    /// 0 framed, 1 bare (the item menus)
    pub style: i32,
    pub alpha: f32,
    x80: i32,
    x7c: f32,
    /// the marker blinks (`m_x84`, with the focus)
    pub x84: bool,
    pub time: i64,
    pub items: Vec<Row>,
    cur: Row,
    pub flash: KeyFlash,
    window: Window,
}

impl Default for WndList {
    fn default() -> Self {
        WndList { x60: false, x64: true, sel: 0, top: 0, count: 0, pos: [0, 0], sel_row: -1, style: 0, alpha: 1.0, x80: 1, x7c: 1.0, x84: true, time: 0, items: Vec::new(), cur: Row::default(), flash: KeyFlash::default(), window: Window::default() }
    }
}

impl WndList {
    /// `Count`: the rows with something in them.
    pub fn count(&self) -> i32 {
        self.items.iter().filter(|r| r.flags != 0).count() as i32
    }

    /// `SetX64`.
    pub fn set_x64(&mut self, v: bool) {
        self.x64 = v;
        self.flash.reset();
    }

    /// `Select(i)`: the row carried in the move mode.
    pub fn select(&mut self, i: i32) {
        self.sel_row = i;
        self.cur = if i == -1 { Row::default() } else { self.items.get(i as usize).copied().unwrap_or_default() };
    }

    /// `Fit(rows)`: the selection and the top kept inside the rows there
    /// are. (Its 296 to the parent is not used by the item menu.)
    fn fit(&mut self, rows: i32) {
        let count = self.count();
        if count - rows < self.top && rows < count {
            self.top -= 1;
        } else if count == rows {
            self.top = 0;
        }
        self.top = self.top.max(0);
        if self.sel >= count {
            self.sel -= 1;
        }
        self.sel = self.sel.max(0);
        self.count = count;
    }

    /// `Scroll(d)`: the selection moved, wrapping, kept in the six shown.
    pub fn scroll(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        let count = self.count();
        cues.push(11);
        if self.sel + d >= count && count > 0 {
            self.sel = 0;
            self.top = 0;
        } else if self.sel + d < 0 {
            self.sel = (count - 1).max(0);
            self.top = (count - 6).max(0);
        } else {
            self.sel += d;
            if self.sel == 0 || self.sel == count - 1 {
                self.flash.reset();
            }
        }
        if self.top + 5 < self.sel {
            self.top = self.sel - 5;
        }
        if self.sel < self.top {
            self.top = self.sel;
        }
        self.x80 = 1;
        self.x7c = 1.0;
        self.time = now;
    }

    /// `OnKeyDown`/`OnKeyUp`'s up and down (their repeat runs in the draw).
    pub fn key_up_down(&mut self, key: i32, down: bool) {
        let k = match key {
            19 | 138 => 138,
            20 | 139 => 139,
            _ => return,
        };
        if down { self.flash.press(k) } else { self.flash.release(k) }
    }

    /// `Update(dt)`: the fade in, and the marker's blink.
    fn update(&mut self, dt: f32) {
        self.alpha = (self.alpha + dt * 5.0).min(1.0);
        if self.x84 {
            if self.x80 == -1 {
                self.x7c = (self.x7c - (dt + dt)).max(0.0);
                if self.x7c == 0.0 {
                    self.x80 = 1;
                }
            } else {
                self.x7c = (self.x7c + dt * 3.0).min(1.0);
                if self.x7c == 1.0 {
                    self.x80 = -1;
                }
            }
        } else {
            self.x7c = 1.0;
        }
    }

    /// `DrawItem`: a row's icon, second icon, arrows, "X", label, number,
    /// and the marker when lit.
    fn draw_item(&self, vui: &Vui, r: &mut dyn Raster, item: &Row, pos: [i32; 2], lit: bool, alpha: f32) {
        let mode = if alpha != 1.0 { 3 } else { 0 };
        if item.flags & 1 != 0 {
            draw_icon(vui, r, item.icon, pos, mode, alpha);
        }
        if item.flags & 0x10 != 0 {
            r.blit(item.icon2 as u32, 0, [pos[0] + 46, pos[1] + 13], [0, 0], [22, 22], 3, alpha);
        }
        if item.flags & 8 != 0 {
            self.draw_arrows(vui, r, item.icon2, pos);
        }
        if item.flags & 0x20 != 0 {
            let h = vui.font_height(r);
            vui.draw_string_ex(r, &wide("X"), pos[0] + 3, pos[1] + 44 - h, [0.0, 0.0, 0.0, alpha], false);
        }
        if item.flags & 2 != 0 {
            let mut p = [pos[0] + 57, pos[1] + 22];
            vui.blit(r, 1, pos);
            let white = [1.0, 1.0, 1.0, alpha];
            let word = |id: i32| wide(vui.words.get(id, vui.language));
            match item.word {
                118 | 119 => {
                    p[1] -= 11;
                    vui.draw_centered_string_ex(r, &word(if item.word == 118 { 600 } else { 601 }), p[0], p[1], white, 0, true);
                    p[1] += 19;
                    vui.draw_centered_string_ex(r, &word(301), p[0], p[1], white, 0, true);
                }
                w => vui.draw_centered_string_ex(r, &word(w), p[0], p[1], white, 0, true),
            }
        }
        if item.flags & 4 != 0 {
            let s = wide(&item.number.to_string());
            let w = vui.string_width(r, &s);
            vui.draw_string_ex(r, &s, pos[0] - w + 114, pos[1], [1.0, 1.0, 1.0, alpha], true);
        }
        if lit {
            let t = vui.entry(86);
            let a = self.x7c.min(alpha);
            if self.style == 0 {
                vui.blit_ex(r, t.x00, [pos[0] - 2, pos[1] + 22 - t.uv1[1] / 2], 3, a, 1);
            } else if self.style == 1 {
                vui.blit_ex(r, t.x00, [pos[0] + 118, pos[1] + 22 - t.uv1[1] / 2], 3, a, 0);
            }
        }
    }

    /// `DrawArrows`: the two-part bar (keys 138 / 140 pick the textures).
    fn draw_arrows(&self, vui: &Vui, r: &mut dyn Raster, key: i32, pos: [i32; 2]) {
        let (end, mid) = if key == 140 { (vui.entry(139), vui.entry(140)) } else { (vui.entry(137), vui.entry(138)) };
        let mut p = [pos[0] + 2, pos[1] + 2];
        vui.blit_ex(r, end.x00, p, 3, 0.3, 0);
        p[0] += end.uv1[0];
        vui.stretch_blit_ex(r, mid.x00, p, [110 - end.uv1[0] * 2, end.uv1[1]], 3, 0.3);
        p[0] += 110 - end.uv1[0] * 2;
        vui.blit_ex(r, end.x00, p, 3, 0.3, 1);
    }

    /// `OnDrawSelf`: the frame (style 0), up to six rows from the top (the
    /// selection lit, the carried row beside it), empty slots to six, the
    /// scroll bar; then the fade and the repeat.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, now: i64, cues: &mut Vec<i32>) {
        let dt = (now - self.time) as f32 * 0.001;
        self.time = now;
        let count = self.count();
        self.fit(6);
        let at = self.pos;
        let mut pos = at;
        if self.style == 0 {
            let (stud, fill) = (vui.entry(36), vui.entry(82));
            let fp = FrameParam { tl: (36, 0), top: (37, 0), tr: (36, 1), right: (42, 1), br: (153, 1), bottom: (37, 2), bl: (153, 0), left: (42, 0) };
            let studs = FrameParam { tl: (0, 0), top: (38, 0), tr: (0, 0), right: (43, 1), br: (0, 0), bottom: (38, 2), bl: (0, 0), left: (43, 0) };
            vui.blit_fill_studded_frame(r, pos, [stud.uv1[0] * 2 + fill.uv1[0] + 114, stud.uv1[1] * 2 + 264], &fp, &studs, 51, 1, 3, 3, self.alpha, true);
            pos = [pos[0] + stud.uv1[0], pos[1] + stud.uv1[1]];
        }
        let mut i = 0;
        let mut row = self.top;
        while row < count {
            let lit = self.x64 && row == self.sel && !self.x60;
            let item = self.items[row as usize];
            self.draw_item(vui, r, &item, pos, lit, self.alpha);
            if row == self.sel && self.x60 {
                let cur = self.cur;
                self.draw_item(vui, r, &cur, [pos[0] + 160, pos[1]], false, self.alpha.min(0.5));
            }
            i += 1;
            pos[1] += 44;
            if i >= 6 {
                break;
            }
            row = i + self.top;
        }
        while i < 6 {
            vui.blit_ex(r, 1, pos, 3, self.alpha, 0);
            pos[1] += 44;
            i += 1;
        }
        let (h, bar) = if self.style == 0 { (268, [at[0] + 126, at[1] + 9]) } else { (270, [at[0] + 143, at[1] - 2]) };
        self.window.init(vui, r, [at[0] + 57, at[1] + 132], 114, 10, 1);
        if self.alpha >= 1.0 {
            self.window.open(true);
        }
        let t = if count < 7 { 0.0 } else { self.top as f32 / (count - 6) as f32 };
        self.window.draw_vscroll(vui, r, bar, h, t);
        self.update(dt);
        if self.flash.pulse(138) {
            self.scroll(-1, now, cues);
        }
        if self.flash.pulse(139) {
            self.scroll(1, now, cues);
        }
    }
}

/// `VIWndList::DrawIcon`: an inventory icon (the cache's state 0) or, not
/// there, the empty row and the "?" mark (state 2).
pub fn draw_icon(vui: &Vui, r: &mut dyn Raster, icon: i32, pos: [i32; 2], mode: i32, alpha: f32) {
    if r.has(icon as u32) {
        r.blit(icon as u32, 0, pos, [0, 0], [114, 44], mode, alpha);
    } else {
        vui.blit_size(r, 1, pos, [114, 44]);
        vui.blit(r, 10, [pos[0] + 42, pos[1]]);
    }
}

/// `VIWndMiniMenu`: kind 0 (`DrawPlain`) grows its frame out of its
/// corner; kind 1 (`DrawTitled`) has a title over its rows and the Select and
/// Exit legend under them. Its rows are a CLIENT menu's, or a fixed table
/// (word, message, id) a window builds itself (the spell book's).
pub struct MiniMenu {
    pub list: WndList,
    pub menu: usize,
    pub visible: bool,
    size: [i32; 2],
    fixed: Option<Vec<(i32, u32, i32)>>,
    /// `m_kind`
    pub kind: i32,
    /// `m_title` (kind 1)
    pub title: Vec<u16>,
}

impl MiniMenu {
    fn new(menu: usize) -> Self {
        MiniMenu { list: WndList::default(), menu, visible: false, size: [0, 0], fixed: None, kind: 0, title: Vec::new() }
    }

    /// A titled popup (kind 1) over a CLIENT menu (the buddy and ignore
    /// lists' member menus).
    pub fn titled(menu: usize) -> Self {
        MiniMenu { kind: 1, ..MiniMenu::new(menu) }
    }

    /// `ResetScroll`: the selection and the top back to the first row.
    pub fn reset_scroll(&mut self) {
        self.list.sel = 0;
        self.list.top = 0;
    }

    /// A popup over a fixed table of (word, message, id).
    pub fn fixed(rows: &[(i32, u32, i32)]) -> Self {
        MiniMenu { fixed: Some(rows.to_vec()), ..MiniMenu::new(0) }
    }

    /// The rows: (word, message, id).
    fn rows(&self, menus: &Menus) -> Vec<(i32, u32, i32)> {
        match &self.fixed {
            Some(r) => r.clone(),
            None => menus.list[self.menu].entries.iter().map(|e| (e.word, e.msg, e.id)).collect(),
        }
    }

    /// `OnMessage` 295 from its parent: the frame starts over.
    pub fn focus(&mut self, now: i64) {
        self.size = [0, 0];
        self.list.alpha = 0.3;
        self.list.time = now;
        self.list.flash.reset();
    }

    /// `OnKeyDown`/`OnKeyUp`.
    pub fn key(&mut self, menus: &Menus, key: i32, down: bool, cues: &mut Vec<i32>) -> ListUp {
        if !down {
            self.list.key_up_down(key, false);
            return ListUp::None;
        }
        match key {
            13 | 134 => match self.rows(menus).get(self.list.sel as usize) {
                Some(&(_, msg, id)) if msg != 0 => ListUp::Action(msg, id),
                _ => ListUp::None,
            },
            27 | 132 => {
                cues.push(8);
                ListUp::Back
            }
            _ => {
                self.list.key_up_down(key, true);
                ListUp::None
            }
        }
    }

    /// `OnDrawSelf`: the kind's drawing, then the held arrows' repeat.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, menus: &Menus, now: i64, cues: &mut Vec<i32>) {
        if self.kind == 1 {
            self.draw_titled(vui, r, menus, now);
        } else {
            self.draw_plain(vui, r, menus, now);
        }
        if self.list.flash.pulse(138) {
            self.list.scroll(-1, now, cues);
        }
        if self.list.flash.pulse(139) {
            self.list.scroll(1, now, cues);
        }
    }

    /// `DrawTitled`: the width from the legend, the title and the labels;
    /// the frame at once (no growing), the title, the rows two lines under
    /// it, and the Select (70) and Exit (69) legend.
    fn draw_titled(&mut self, vui: &Vui, r: &mut dyn Raster, menus: &Menus, now: i64) {
        let rows = self.rows(menus);
        self.list.items = rows.iter().map(|&(word, _, _)| Row { flags: u32::MAX, icon: -1, icon2: -1, x0c: -1, word, number: -1 }).collect();
        let h = 19;
        let dt = (now - self.list.time) as f32 * 0.001;
        self.list.time = now;
        let a = self.list.alpha;
        let white = [1.0, 1.0, 1.0, a];
        let yellow = [1.0, 1.0, 0.0, a];
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        let button = vui.entry(70);
        let mut size = [vui.string_width(r, &word(161)) + vui.string_width(r, &word(212)) + 105 + button.uv1[0], 0];
        size[0] = size[0].max(vui.string_width(r, &self.title) + 75);
        for &(w, _, _) in &rows {
            size[0] = size[0].max(vui.string_width(r, &word(w)) + 75);
        }
        size[1] = (rows.len() as i32 + 5) * h;
        self.size = size;
        let (top, corner) = (vui.entry(101), vui.entry(106));
        let pos = self.list.pos;
        let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 0), br: (106, 0), bottom: (102, 0), bl: (105, 0), left: (104, 0) };
        let stretch = [(self.size[0] - corner.uv1[0]).max(0), self.size[1] - top.uv1[1] - corner.uv1[1] + 1];
        vui.stretch_blit_ex(r, 61, [pos[0], pos[1] + top.uv1[1] - 1], stretch, 3, a.min(0.75));
        vui.blit_frame(r, pos, self.size, &fp, 3, a, true);
        let mut p = [pos[0] + 20, pos[1] + 10];
        vui.draw_string_ex(r, &self.title, p[0], p[1], white, true);
        p = [p[0] + h * 2, p[1] + h * 2];
        for (i, &(w, _, _)) in rows.iter().enumerate() {
            // DrawItem
            let lit = self.list.x64 && i as i32 == self.list.sel;
            if lit {
                let t = vui.entry(89);
                vui.blit_ex(r, 89, [p[0] - 25, p[1] - t.uv1[1] / 2], 3, a.min(self.list.x7c), 0);
            }
            vui.draw_centered_string_ex(r, &word(w), p[0], p[1], if lit { yellow } else { white }, 1, true);
            p[1] += h;
        }
        let mut bp = [pos[0] + 20, pos[1] + self.size[1] - 32];
        let mut tp = [bp[0] + 22, bp[1] + button.uv1[1] / 2 - 3];
        vui.blit_ex(r, 70, bp, 3, a, 0);
        vui.draw_centered_string_ex(r, &word(212), tp[0], tp[1], white, 1, true);
        let step = vui.string_width(r, &word(212)) + 30 + button.uv1[0];
        bp[0] += step;
        tp[0] += step;
        vui.blit_ex(r, 69, bp, 3, a, 0);
        vui.draw_centered_string_ex(r, &word(161), tp[0], tp[1], white, 1, true);
        self.list.update(dt);
    }

    /// `DrawPlain`: the rows' widest label sets the size, the frame grows to
    /// it at three sizes a second, the rows drawn once it is there.
    fn draw_plain(&mut self, vui: &Vui, r: &mut dyn Raster, menus: &Menus, now: i64) {
        self.list.items = self.rows(menus).iter().map(|&(word, _, _)| Row { flags: u32::MAX, icon: -1, icon2: -1, x0c: -1, word, number: -1 }).collect();
        let dt = (now - self.list.time) as f32 * 0.001;
        self.list.time = now;
        let mut size = [0, 0];
        for it in &self.list.items {
            size[0] = size[0].max(vui.string_width(r, &wide(vui.words.get(it.word as u16 as i32, vui.language))));
        }
        size[0] += 75;
        size[1] = self.list.items.len() as i32 * 19 + 25;
        self.list.alpha = 1.0;
        let grow = dt * 3.0;
        self.size[0] = (self.size[0] + (grow * size[0] as f32) as i32).min(size[0]);
        self.size[1] = (self.size[1] + (grow * size[1] as f32) as i32).min(size[1]);
        let (top, corner) = (vui.entry(101), vui.entry(106));
        let mut pos = self.list.pos;
        if pos[1] + size[1] > 320 {
            pos[1] = 320 - size[1];
        }
        let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 0), br: (106, 0), bottom: (102, 0), bl: (105, 0), left: (104, 0) };
        let stretch = [(self.size[0] - corner.uv1[0]).max(0), self.size[1] - top.uv1[1] - corner.uv1[1] + 1];
        vui.stretch_blit_ex(r, 61, [pos[0], pos[1] + top.uv1[1] - 1], stretch, 3, 0.75);
        vui.blit_frame(r, pos, self.size, &fp, 0, 1.0, false);
        pos = [pos[0] + 35, pos[1] + 20];
        if self.size == size {
            for (i, it) in self.list.items.clone().iter().enumerate() {
                let lit = self.list.x64 && i as i32 == self.list.sel;
                let (yellow, white) = ([1.0, 1.0, 0.0, self.list.alpha], [1.0, 1.0, 1.0, self.list.alpha]);
                if lit {
                    let t = vui.entry(89);
                    vui.blit_ex(r, 89, [pos[0] - 25, pos[1] - t.uv1[1] / 2], 3, self.list.alpha.min(self.list.x7c), 0);
                }
                vui.draw_centered_string_ex(r, &wide(vui.words.get(it.word, vui.language)), pos[0], pos[1], if lit { yellow } else { white }, 1, true);
                pos[1] += 19;
            }
        }
        self.list.update(dt);
    }
}

/// `VIWndMenu`: a list over a menu definition, with its popup.
pub struct WndMenu {
    pub list: WndList,
    pub menu: usize,
    pub visible: bool,
    pub sub: Option<MiniMenu>,
}

impl WndMenu {
    pub fn new(pos: [i32; 2], style: i32) -> Self {
        let mut list = WndList { pos, style, ..Default::default() };
        list.items.clear();
        WndMenu { list, menu: 0, visible: false, sub: None }
    }

    /// `Reset`: shown, at the top, the selection lit, the popup closed.
    pub fn reset(&mut self) {
        self.visible = true;
        self.list.sel = 0;
        self.list.top = 0;
        self.list.set_x64(true);
        if let Some(s) = self.sub.as_mut() {
            s.visible = false;
        }
    }

    /// `GetId(i)`: entry `i`'s id, -1 past the end.
    pub fn id(&self, menus: &Menus, i: i32) -> i32 {
        menus.list.get(self.menu).and_then(|m| m.entries.get(i as usize)).map_or(-1, |e| e.id)
    }

    /// `OnMessage` 295: the focus arriving (`gain`) or leaving.
    pub fn focus(&mut self, gain: bool, now: i64) {
        if gain {
            self.list.x84 = true;
            self.list.alpha = 1.0;
            self.list.x80 = 1;
            self.list.x7c = 1.0;
            self.list.time = now;
        } else {
            self.list.x84 = false;
        }
        self.list.flash.reset();
    }

    /// `OnKeyDown`/`OnKeyUp`, to the popup when it is up.
    pub fn key(&mut self, menus: &Menus, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> ListUp {
        if let Some(s) = self.sub.as_mut().filter(|s| s.visible) {
            return match s.key(menus, key, down, cues) {
                // 260 / 261 from the popup: the focus back here, the popup
                // hidden; 261 goes on up
                ListUp::Back => {
                    s.visible = false;
                    self.focus(true, now);
                    ListUp::None
                }
                ListUp::Action(m, id) => {
                    s.visible = false;
                    self.focus(true, now);
                    ListUp::Action(m, id)
                }
                up => up,
            };
        }
        if !down {
            self.list.key_up_down(key, false);
            return ListUp::None;
        }
        let entries = &menus.list[self.menu].entries;
        match key {
            13 | 134 => {
                cues.push(13);
                let Some(e) = entries.get(self.list.sel as usize).copied() else { return ListUp::None };
                let mut up = ListUp::None;
                if e.msg != 0 {
                    up = ListUp::Action(e.msg, e.id);
                }
                if self.list.x60 {
                    let from = entries.get(self.list.sel_row.max(0) as usize).map_or(-1, |x| x.id);
                    return ListUp::Swap(from, e.id);
                }
                if let Some(m) = e.sub {
                    let step = (self.list.sel - self.list.top) * 44;
                    let pos = if self.list.style == 1 { [self.list.pos[0] + 155, self.list.pos[1] + step] } else { [self.list.pos[0] + 139, self.list.pos[1] + step + 10] };
                    let s = self.sub.get_or_insert_with(|| MiniMenu::new(m));
                    s.list.pos = pos;
                    s.menu = m;
                    s.list.sel = 0;
                    s.list.top = 0;
                    s.visible = true;
                    s.focus(now);
                    self.focus(false, now);
                }
                up
            }
            27 | 132 => {
                cues.push(8);
                if self.list.x60 {
                    self.list.x60 = false;
                    self.list.select(-1);
                    ListUp::None
                } else {
                    ListUp::Back
                }
            }
            _ => {
                self.list.key_up_down(key, true);
                ListUp::None
            }
        }
    }

    /// `OnDrawSelf`: the rows rebuilt from the menu, the list, then the
    /// popup over it.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, menus: &Menus, now: i64, cues: &mut Vec<i32>) {
        if !self.visible {
            return;
        }
        let menu = &menus.list[self.menu];
        let (mut icon, mut icon2, mut x0c) = (-1, -1, -1);
        self.list.items = menu
            .entries
            .iter()
            .map(|e| {
                let mut flags = 0;
                if e.word != -1 {
                    flags = 2;
                    icon = -1;
                }
                if e.icon != -1 {
                    icon = e.icon;
                    flags |= 1;
                }
                if e.icon2 != -1 {
                    icon2 = e.icon2;
                    flags |= if e.msg == 0x0805_0000 { 0x10 } else { 8 };
                }
                if e.number >= 2 {
                    icon = e.icon;
                    flags |= 4;
                }
                if e.x08 != -1 {
                    x0c = e.x08;
                    flags |= 0x20;
                }
                Row { flags, icon, icon2, x0c, word: e.word, number: e.number }
            })
            .collect();
        self.list.draw(vui, r, now, cues);
        if let Some(s) = self.sub.as_mut().filter(|s| s.visible) {
            s.draw(vui, r, menus, now, cues);
        }
    }
}
