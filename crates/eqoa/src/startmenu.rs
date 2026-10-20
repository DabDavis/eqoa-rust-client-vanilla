//! The start menu (CLIENT `viwndstartmenu.cpp`, `viwndmainmenu.cpp`,
//! `vikeyflash.cpp`): the framed screen `VIWndGame::OpenStartMenu` puts up
//! (mode 13), its `VIWndMainMenu` (seven bars of a `VIMenuDef`, the
//! character's bar, the stats column with the location and the effects, the
//! key hints) and the main menus a sub-menu opens over it.
//!
//! The window tree's messages are kept as the game passes them: a key goes
//! to the menu that has the focus (the deepest open one); 260 (back) and 261
//! (an entry's message) come back up, each parent doing what its
//! `OnMessage` does. The item menu (`VIWndItemMenu`, 0x2000000) is not
//! ported.

#![allow(clippy::too_many_arguments)]

use crate::client_image::Menus;
use crate::ui::Layout;
use render::ui::Anchor;
use vi::ui::{FrameParam, Raster, TextScroll, Vui, Window, wide};

/// `VIKeyFlash`: a held key's auto-repeat. It fires at once, then after 15
/// frames, then 8, 7, ... down to every frame.
#[derive(Clone, Copy, Default)]
pub struct KeyFlash {
    count: usize,
    slots: [(u8, i32, i32); 16],
}

impl KeyFlash {
    pub fn reset(&mut self) {
        self.count = 0;
        for s in &mut self.slots {
            s.0 = 0;
        }
    }

    pub fn press(&mut self, key: u8) {
        let mut k = self.slots.iter().position(|s| s.0 == key || s.0 == 0);
        if k.is_none() && self.count < 16 {
            k = Some(self.count);
            self.count += 1;
        }
        if let Some(k) = k {
            self.slots[k] = (key, 16, 0);
        }
    }

    pub fn release(&mut self, key: u8) {
        if let Some(s) = self.slots.iter_mut().find(|s| s.0 == key) {
            s.0 = 0;
        }
    }

    pub fn pulse(&mut self, key: u8) -> bool {
        let Some(s) = self.slots.iter_mut().find(|s| s.0 == key) else { return false };
        if s.2 == 0 {
            s.1 = if s.1 == 15 { 8 } else { (s.1 - 1).max(1) };
            s.2 = s.1;
            return true;
        }
        s.2 -= 1;
        false
    }
}

/// The game's key codes the menus take.
pub mod keys {
    pub const ESCAPE: i32 = 27;
    pub const UP: i32 = 138;
    pub const DOWN: i32 = 139;
    pub const LEFT: i32 = 136;
    pub const RIGHT: i32 = 137;
    pub const CROSS: i32 = 134;
}

/// A message up the window tree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Up {
    None,
    /// 260: back out
    Back,
    /// 261: an entry's message and id
    Action(u32, i32),
    /// the Start key, handed to the start menu
    Start,
    /// 261 with 0x2000000: `ShowItems`, the item menu over the frame
    Items,
}

/// What the stats column and the character's bar read off the client.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub name: String,
    pub class: i32,
    pub level: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub power: i32,
    pub max_power: i32,
    /// `m_stat[14]`, the armour class
    pub ac: i32,
    /// `m_player.m_p34`, tunar
    pub tunar: i32,
    /// `m_effectName[0 .. m_effectCount]`
    pub effects: Vec<Vec<u16>>,
    /// `VIGetExpansionFlags()`
    pub expansion: u32,
}

/// What `UpdateLocation` needs: the world's grid and the player's place.
#[derive(Clone, Copy, Debug)]
pub struct Location {
    pub world: i32,
    pub pos: [f32; 3],
    pub cols: i32,
    pub rows: i32,
    pub cell: f32,
    pub origin: [f32; 3],
    /// `gWorldInfo`'s bounds, columns then rows
    pub bounds: Option<[i32; 4]>,
}

/// `sClassIcons`: a class's icon in the stats column.
const CLASS_ICONS: [(i32, i32); 15] = [(0, 23), (1, 19), (2, 18), (3, 21), (4, 16), (5, 11), (6, 20), (7, 13), (8, 22), (9, 12), (10, 15), (11, 17), (12, 14), (13, 24), (14, 251)];

/// `VIWndMainMenu`.
pub struct MainMenu {
    pub menu: usize,
    sel: i32,
    top: i32,
    x34: bool,
    blink: f32,
    blink_dir: i32,
    x44: bool,
    /// it has the focus (only the focused menu draws)
    x48: bool,
    time: i64,
    sub: Option<Box<MainMenu>>,
    /// a sub-menu (no character's bar)
    sub2: bool,
    visible: bool,
    lines: [[u16; 32]; 2],
    window: Window,
    scroll: TextScroll,
    flash: KeyFlash,
    /// how far its bars were drawn left of the frame's (`Layout::side`): a
    /// pointer meets them there
    side: i32,
}

impl MainMenu {
    pub fn new(menu: usize, sub2: bool) -> Self {
        MainMenu {
            menu,
            sel: 0,
            top: 0,
            x34: true,
            blink: 1.0,
            blink_dir: 1,
            x44: false,
            x48: false,
            time: 0,
            sub: None,
            sub2,
            visible: false,
            lines: [[0; 32]; 2],
            window: Window::default(),
            scroll: TextScroll::default(),
            flash: KeyFlash::default(),
            side: 0,
        }
    }

    /// `OnMessage` 295: the focus arriving (`gain`) or leaving.
    fn focus(&mut self, vui: &Vui, r: &dyn Raster, gain: bool, now: i64) {
        if gain {
            self.x48 = true;
            self.blink = 1.0;
            self.blink_dir = 1;
            self.time = now;
            self.window.init(vui, r, [320, 224], 100, 10, 0);
            self.window.open(true);
        } else {
            self.x48 = false;
        }
        self.flash.reset();
    }

    /// Whether this menu or one opened over it has the focus.
    fn focused(&self) -> bool {
        self.x48 || self.sub.as_ref().is_some_and(|s| s.visible && s.focused())
    }

    /// `Activate`: the focus taken (a reset starts at the top and closes
    /// the sub-menu) or handed down to the open sub-menu; the location
    /// worked out again.
    pub fn activate(&mut self, vui: &Vui, r: &dyn Raster, reset: bool, now: i64, loc: &Location) {
        self.visible = true;
        if reset {
            self.sel = 0;
            self.top = 0;
            if let Some(s) = self.sub.as_mut() {
                s.visible = false;
                s.unfocus();
            }
            self.focus(vui, r, true, now);
            self.x44 = true;
        } else if let Some(s) = self.sub.as_mut() {
            self.x48 = false;
            s.activate(vui, r, false, now, loc);
        } else {
            self.focus(vui, r, true, now);
            self.x44 = true;
        }
        self.update_location(vui, r, loc);
    }

    fn unfocus(&mut self) {
        self.x48 = false;
        self.flash.reset();
        if let Some(s) = self.sub.as_mut() {
            s.unfocus();
        }
    }

    /// `OnKeyDown`/`OnKeyUp`, sent to the focused menu.
    pub fn key(&mut self, vui: &Vui, r: &dyn Raster, menus: &Menus, key: i32, down: bool, now: i64, loc: &Location, cues: &mut Vec<i32>) -> Up {
        if let Some(s) = self.sub.as_mut().filter(|s| s.visible && s.focused()) {
            let up = s.key(vui, r, menus, key, down, now, loc, cues);
            return match up {
                // 260 from the sub-menu: back here
                Up::Back => {
                    let pos = s.scroll.offset;
                    s.visible = false;
                    s.unfocus();
                    s.x44 = false;
                    self.scroll.offset = pos;
                    self.focus(vui, r, true, now);
                    self.x44 = true;
                    Up::None
                }
                // 261: the focus back here, the sub-menu hidden, forwarded
                Up::Action(..) => {
                    s.visible = false;
                    s.unfocus();
                    self.focus(vui, r, true, now);
                    up
                }
                _ => up,
            };
        }
        if !down {
            match key {
                19 | 138 => self.flash.release(138),
                20 | 139 => self.flash.release(139),
                _ => {}
            }
            return Up::None;
        }
        match key {
            27 | 132 => {
                cues.push(8);
                Up::Back
            }
            13 | 134 => {
                cues.push(13);
                self.select(vui, r, menus, now, loc)
            }
            19 | 138 => {
                self.flash.press(138);
                Up::None
            }
            20 | 139 => {
                self.flash.press(139);
                Up::None
            }
            144 => Up::Start,
            _ => Up::None,
        }
    }

    /// `Select`: an entry's message goes up (any sub-menu dropped); a
    /// sub-menu opens over this one.
    fn select(&mut self, vui: &Vui, r: &dyn Raster, menus: &Menus, now: i64, loc: &Location) -> Up {
        let Some(e) = menus.list[self.menu].entries.get(self.sel as usize).copied() else { return Up::None };
        if e.msg != 0 {
            self.sub = None;
            return Up::Action(e.msg, e.id);
        }
        if let Some(m) = e.sub {
            let sub = self.sub.get_or_insert_with(|| Box::new(MainMenu::new(m, true)));
            self.x44 = false;
            sub.scroll.offset = self.scroll.offset;
            sub.x44 = true;
            sub.menu = m;
            self.x48 = false;
            self.flash.reset();
            sub.activate(vui, r, true, now, loc);
        }
        Up::None
    }

    /// A pointer at frame point `p` (the client's own; the game takes the
    /// pad and the keyboard): on a bar of the focused menu (this one or one
    /// opened over it, the only one drawn), that entry selected. True when
    /// it was one.
    pub fn point(&mut self, p: [f32; 2], menus: &Menus) -> bool {
        if let Some(s) = self.sub.as_mut().filter(|s| s.visible && s.focused()) {
            return s.point(p, menus);
        }
        let count = menus.list[self.menu].entries.len() as i32;
        // the left panel's, wherever it went (`Layout`)
        let (x, y) = (p[0] as i32 + self.side, p[1] as i32);
        // the bars: from (18, 56), 27 apart, seven shown
        let i = (y - 56).div_euclid(27);
        if !(18..18 + 150).contains(&x) || !(0..7).contains(&i) || (y - 56).rem_euclid(27) >= 26 || self.top + i >= count {
            return false;
        }
        self.sel = self.top + i;
        self.blink_dir = 1;
        self.blink = 1.0;
        true
    }

    /// `Scroll`: the selection moved by `d`, wrapping, seven bars shown.
    fn scroll_by(&mut self, d: i32, count: i32, now: i64, cues: &mut Vec<i32>) {
        let i = self.sel + d;
        cues.push(11);
        if i >= count {
            self.sel = 0;
            self.top = 0;
        } else if i < 0 {
            self.sel = (count - 1).max(0);
            self.top = (count - 7).max(0);
        } else if self.top + 6 < i {
            self.sel += d;
            self.top = self.sel - 6;
        } else if i < self.top {
            self.top = self.sel + d;
            self.sel += d;
        } else {
            self.sel += d;
        }
        if (self.sel == 0 && d < 0) || (self.sel == count - 1 && d > 0) {
            self.flash.reset();
        }
        self.blink_dir = 1;
        self.blink = 1.0;
        self.time = now;
    }

    /// `OnDrawSelf` (only with the focus), then the sub-menu over it.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout, menus: &Menus, now: i64, st: &Stats, cues: &mut Vec<i32>) {
        if self.visible && self.x48 {
            self.draw_self(vui, r, menus, now, st, cues);
        }
        if let Some(s) = self.sub.as_mut().filter(|s| s.visible) {
            s.draw(vui, r, menus, now, st, cues);
        }
    }

    /// In a window wider than 4:3 the bars, their scroll bar and the key
    /// hints go with the left panel to the left side, the stats with the
    /// right panel to the right; the title stays in the middle.
    fn draw_self(&mut self, vui: &Vui, r: &mut dyn Layout, menus: &Menus, now: i64, st: &Stats, cues: &mut Vec<i32>) {
        let dt = (now - self.time) as f32 * 0.001;
        self.time = now;
        self.side = r.side();
        r.anchor(Anchor::Left);
        let menu = &menus.list[self.menu];
        let count = menu.entries.len() as i32;
        let lit = [10.0 / 255.0, 50.0 / 255.0, 70.0 / 255.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        let white = [1.0; 4];
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        let mut pos = [18, 56];
        let mut i = 0;
        let mut row = self.top;
        while row < count {
            let sel = self.x34 && row == self.sel;
            vui.blit_bar3(r, &word(menu.entries[row as usize].word), if sel { lit } else { black }, pos, 141, 142, 150, 140, 0, 1.0, 0, false);
            if sel {
                let (a, b) = (vui.entry(85), vui.entry(150));
                let p = [pos[0], pos[1] + b.uv1[1] / 2 - a.uv1[1] / 2 - 3];
                vui.blit_ex(r, a.x00, p, 3, self.blink, 0);
            }
            i += 1;
            pos[1] += 27;
            if i >= count || i >= 7 {
                break;
            }
            row = i + self.top;
        }
        let t = if count < 8 { 0.0 } else { self.top as f32 / (count - 7) as f32 };
        self.window.draw_vscroll(vui, r, [175, 46], 270, t);
        r.anchor(Anchor::Middle);
        if menu.x04 > 0 {
            if self.x44 {
                let w = vui.string_width(r, &word(menu.x04)) + 64;
                vui.blit_bar(r, &word(menu.x04), white, [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
            }
        } else if !self.sub2 {
            let class = vui.words.get(165 + st.class.clamp(0, 14), vui.language).to_string();
            let s = wide(&format!("{}: {} {} {}", st.name, vui.words.get(503, vui.language), st.level, class));
            let w = vui.string_width(r, &s) + 64;
            vui.blit_bar(r, &s, white, [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
        }
        r.anchor(Anchor::Right);
        self.draw_stats(vui, r, st);
        r.anchor(Anchor::Left);
        let mut at = [20, 260];
        vui.blit_ex(r, 66, at, 0, 1.0, 0);
        draw_plain(vui, r, &word(212), [at[0] + 22, at[1] - 5], black);
        at[1] += 19;
        vui.blit_ex(r, 65, at, 0, 1.0, 0);
        draw_plain(vui, r, &word(161), [at[0] + 22, at[1] - 5], black);
        r.anchor(Anchor::Middle);
        if self.blink_dir == -1 {
            self.blink = (self.blink - (dt + dt)).max(0.0);
            if self.blink == 0.0 {
                self.blink_dir = 1;
            }
        } else {
            self.blink = (self.blink + dt * 3.0).min(1.0);
            if self.blink == 1.0 {
                self.blink_dir = -1;
            }
        }
        let now_ms = now;
        if self.flash.pulse(138) {
            self.scroll_by(-1, count, now_ms, cues);
        }
        if self.flash.pulse(139) {
            self.scroll_by(1, count, now_ms, cues);
        }
    }

    /// `DrawStats`: the class's icon, hit points, power, AC, the location,
    /// tunar and the effects scrolling.
    fn draw_stats(&mut self, vui: &Vui, r: &mut dyn Raster, st: &Stats) {
        let white = [1.0; 4];
        let yellow = [1.0, 1.0, 0.0, 1.0];
        let h = vui.font_height(r) - 4;
        let mut at = [545, 48];
        let mut icon = [485, 48];
        for &(c, id) in &CLASS_ICONS {
            if c != st.class {
                continue;
            }
            let t = vui.entry(id);
            vui.blit(r, 130, icon);
            icon[0] += 57 - t.uv1[0] / 2;
            if st.class == 14 {
                if st.expansion & 2 == 0 {
                    break;
                }
                icon[1] += 1;
            }
            vui.blit(r, id, icon);
        }
        at[1] += 48;
        let mut line = |r: &mut dyn Raster, s: &[u16], c: [f32; 4]| {
            vui.draw_centered_string_ex(r, s, at[0], at[1], c, 0, true);
            at[1] += h;
        };
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        line(r, &word(919), yellow);
        line(r, &wide(&format!("{}/{}", st.hp, st.max_hp)), white);
        line(r, &word(466), yellow);
        line(r, &wide(&format!("{}/{}", st.power, st.max_power)), white);
        line(r, &word(242), yellow);
        line(r, &wide(&st.ac.to_string()), white);
        line(r, &word(18), yellow);
        for l in self.lines {
            if l[0] != 0 {
                let n = l.iter().position(|&c| c == 0).unwrap_or(32);
                line(r, &l[..n], white);
            }
        }
        line(r, &word(479), yellow);
        line(r, &wide(&st.tunar.to_string()), white);
        line(r, &word(15), yellow);
        // strcat16 into a 256-character buffer: 255 at most
        let mut effects: Vec<u16> = Vec::new();
        let n = st.effects.len();
        for (i, name) in st.effects.iter().enumerate() {
            let sep = if n >= 2 && i < n - 1 { wide("    ") } else { Vec::new() };
            for &c in name.iter().chain(&sep) {
                if effects.len() + 1 < 256 {
                    effects.push(c);
                }
            }
        }
        self.scroll.init(vui, r, [at[0], at[1] + 2], 160, 0, 0);
        self.scroll.window.open(true);
        if effects.is_empty() {
            self.scroll.reset();
        }
        self.scroll.text_scroll(vui, r, &effects, white, 9);
    }

    /// `UpdateLocation`: the grid cell's zone name and which ninth of it.
    fn update_location(&mut self, vui: &Vui, r: &dyn Raster, loc: &Location) {
        self.lines = [[0; 32]; 2];
        if loc.cols == 1 && loc.rows == 1 {
            for (k, c) in "***".encode_utf16().enumerate() {
                self.lines[0][k] = c;
            }
            return;
        }
        // CalcWorldGridOffset
        let col = (loc.pos[0] / loc.cell) as i32;
        let row = (loc.pos[2] / loc.cell) as i32;
        let off = [loc.pos[0] - col as f32 * loc.cell - loc.origin[0], loc.pos[2] - row as f32 * loc.cell - loc.origin[2]];
        let zone = zone_name(loc, row - 1, col - 1);
        let name = vui.words.get(zone as i32, vui.language).to_string();
        const DIRS: [[i32; 3]; 3] = [[1426, 1422, 1427], [1425, 1430, 1424], [1428, 1423, 1429]];
        let third = |v: f32| if v < -333.34 { 0 } else if v <= 333.34 { 1 } else { 2 };
        let dir = vui.words.get(DIRS[third(off[1])][third(off[0])], vui.language).to_string();
        let text = if dir.is_empty() { name } else { format!("{name} ({dir})") };
        self.wrap(vui, r, 150, 32, 2, &wide(&text));
    }

    /// `Wrap`: `text` into `rows` lines of at most `chars - 1`, broken at a
    /// space or after a hyphen once a line reaches `width` pixels.
    fn wrap(&mut self, vui: &Vui, r: &dyn Raster, width: i32, chars: usize, rows: usize, text: &[u16]) {
        let mut flat = [0u16; 64];
        let at = |i: usize| text.get(i).copied().unwrap_or(0);
        let (mut src, mut line, mut len, mut brk, mut hyph, mut w) = (0usize, 0usize, 0usize, 0usize, false, 0);
        let mut start = 0usize;
        if at(0) == 0 {
            return;
        }
        while at(src) != 0 {
            let mut out = line * 32;
            if width > 0 && chars > 1 {
                let mut s = src;
                while len < chars - 1 && line < rows {
                    if at(s) == 0 {
                        self.store(&flat);
                        return;
                    }
                    if at(s) == b' ' as u16 {
                        brk = len;
                        hyph = false;
                    } else if at(s) == b'-' as u16 {
                        hyph = at(s + 1) != 0;
                        brk = len + hyph as usize;
                    }
                    if at(s) == b'\n' as u16 {
                        out += 32;
                        start += 32;
                        line += 1;
                        s += 1;
                        src += 1;
                        len = 0;
                        w = 0;
                        brk = 0;
                        hyph = false;
                    }
                    if at(s) != 0 && out + len + 1 < flat.len() {
                        flat[out + len] = at(s);
                        len += 1;
                        flat[out + len] = 0;
                        s += 1;
                        src += 1;
                        let n = flat[out..].iter().position(|&c| c == 0).unwrap_or(0);
                        w = vui.string_width(r, &flat[out..out + n]);
                    }
                    if w >= width {
                        break;
                    }
                }
            }
            if at(src) == 0 {
                break;
            }
            if brk > 0 {
                if start + brk < flat.len() {
                    flat[start + brk] = 0;
                }
                src = if hyph { src - (len - brk) } else { src + 1 - (len - brk) };
            }
            line += 1;
            if line >= rows {
                break;
            }
            start += 32;
            len = 0;
            w = 0;
            brk = 0;
            hyph = false;
        }
        self.store(&flat);
    }

    fn store(&mut self, flat: &[u16; 64]) {
        self.lines[0].copy_from_slice(&flat[..32]);
        self.lines[1].copy_from_slice(&flat[32..]);
    }
}

/// `VIFont::DrawString` in the font's colour (here passed).
fn draw_plain(vui: &Vui, r: &mut dyn Raster, s: &[u16], at: [i32; 2], c: [f32; 4]) {
    let _ = vui;
    r.draw_string(at, c, s);
}

/// `ZoneName(world, row, col)`: 641 out of the world's bounds.
fn zone_name(loc: &Location, row: i32, col: i32) -> u16 {
    let Some(b) = loc.bounds else { return 641 };
    if col < b[0] || b[1] < col || row < b[2] || b[3] < row {
        return 641;
    }
    crate::zones::zone(loc.world, row, col).unwrap_or(641)
}

/// `VIWndStartMenu`: the frame, and the main menu in it.
pub struct StartMenu {
    pub main: MainMenu,
    pub visible: bool,
}

impl StartMenu {
    pub fn new(menu: usize) -> Self {
        StartMenu { main: MainMenu::new(menu, false), visible: false }
    }

    /// A pointer on the focused menu's bars (`MainMenu::point`).
    pub fn point(&mut self, p: [f32; 2], menus: &Menus) -> bool {
        self.visible && self.main.point(p, menus)
    }

    /// `ShowMain(a)`.
    pub fn show_main(&mut self, vui: &Vui, r: &dyn Raster, reset: bool, now: i64, loc: &Location) {
        self.main.activate(vui, r, reset, now, loc);
    }

    /// A key, to the focused menu; what reaches `VIWndGame` comes back:
    /// `Back` closes the start menu, `Action` is an entry's message.
    pub fn key(&mut self, vui: &Vui, r: &dyn Raster, menus: &Menus, key: i32, down: bool, now: i64, loc: &Location, cues: &mut Vec<i32>) -> Up {
        match self.main.key(vui, r, menus, key, down, now, loc, cues) {
            // VIWndStartMenu::OnKeyDown 144
            Up::Start => {
                cues.push(8);
                Up::Back
            }
            // 261: the item menu (ShowItems: the main menu hidden), or
            // anything else, which closes
            Up::Action(0x0200_0000, _) => {
                self.main.visible = false;
                Up::Items
            }
            Up::Action(m, id) => {
                self.main.visible = false;
                Up::Action(m, id)
            }
            up => up,
        }
    }

    /// `OnDrawSelf`: the black band, the left and right panels and the
    /// studded frame round the middle; then the main menu.
    ///
    /// In a window wider than 4:3 (the client's own, `Layout`) the panels go
    /// to its sides, each with its rails, the band runs its whole width and
    /// the middle frame, where the character stands in the world, is as wide
    /// as the panels leave, studded at about the game's spacing. A 4:3
    /// window draws it as the game does.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout, menus: &Menus, now: i64, st: &Stats, cues: &mut Vec<i32>) {
        if !self.visible {
            return;
        }
        let s = r.side();
        r.anchor(Anchor::Wide);
        vui.blit_rect(r, 32, [-1 - s, -1], [642 + 2 * s, 48], 0, 1.0);
        r.anchor(Anchor::Left);
        let fp = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
        vui.blit_fill_frame(r, [-1, 48], [176, 265], &fp, 35, 0, 1.0, true);
        r.anchor(Anchor::Right);
        vui.blit_rect(r, 51, [451, 48], [199, 265], 0, 1.0);
        let fp = FrameParam { tl: (36, 0), top: (37, 0), tr: (36, 1), right: (42, 1), br: (39, 1), bottom: (40, 0), bl: (39, 0), left: (42, 0) };
        let studs = FrameParam { tl: (0, 0), top: (38, 0), tr: (0, 0), right: (43, 1), br: (0, 0), bottom: (41, 0), bl: (0, 0), left: (43, 0) };
        r.anchor(Anchor::Left);
        vui.blit_studded_horz(r, [-1, 37], 37, 38, 177, 1, 0, 1.0, 0);
        vui.blit_studded_horz(r, [-1, 313], 40, 41, 177, 1, 0, 1.0, 0);
        r.anchor(Anchor::Right);
        vui.blit_studded_horz(r, [451, 37], 37, 38, 200, 1, 0, 1.0, 0);
        vui.blit_studded_horz(r, [451, 313], 40, 41, 200, 1, 0, 1.0, 0);
        r.anchor(Anchor::Wide);
        let w = 275 + 2 * s;
        vui.blit_studded_frame(r, [176 - s, 37], [w, 294], &fp, &studs, studs_across(w), 3, 0, 1.0, false);
        r.anchor(Anchor::Middle);
        self.main.draw(vui, r, menus, now, st, cues);
    }
}

/// The studs along the middle frame's top and bottom: the game's two on its
/// 275, and on a wider middle (`StartMenu::draw`) as many as keep about its
/// spacing.
fn studs_across(w: i32) -> i32 {
    if w <= 275 { 2 } else { (w * 3 + 137) / 275 - 1 }
}

/// The start menu with what it reads out of CLIENT: every menu reached
/// from the four start tables (`gStartItems`, its expansion and bank
/// variants), and `gWorldInfo`'s grid bounds.
pub struct StartData {
    pub menus: Menus,
    /// gStartItems, gStartItemsX, gStartItemsBank, gStartItemsBankX: each
    /// without its first entry ("TEST"), as `OpenStartMenu` leaves it while
    /// `m_p382E8` is not enabled
    tables: [usize; 4],
    pub bounds: [Option<[i32; 4]>; 6],
    /// the item menus' slot counts: gItemMenu4, gItemMenu3, gInvMenu,
    /// gEquipMenu (the item menu's tabs A, B, C, D)
    pub item_counts: [i32; 4],
    /// `gAttackWords` (0x00CDF6A0): (word, attack type), 11 of them
    pub attack_words: Vec<(i32, i64)>,
    /// `gEquipSlotWords` (0x00CDF6F8): (word, equip slot), 23 of them
    pub slot_words: Vec<(i32, i64)>,
    /// `gBuddyMenu` (0x00CD03B0) and `gIgnoreMemberMenu` (0x00CD0340): the
    /// buddy and ignore lists' member menus
    pub buddy_menu: usize,
    pub ignore_menu: usize,
    pub menu: StartMenu,
}

impl StartData {
    pub fn from_image(img: &crate::client_image::ClientImage) -> Self {
        let mut menus = Menus::default();
        // (entries, count) of gStartItems, X, Bank, BankX
        let roots = [(0x00CCFFF0, 0x00CD0130), (0x00CCFD40, 0x00CCFE80), (0x00CD0138, 0x00CD0298), (0x00CCFE88, 0x00CCFFE8)];
        let tables = roots.map(|(e, c)| menus.table(img, e + 32, img.i32_at(c) - 1));
        let bounds = std::array::from_fn(|w| crate::client_image::world_bounds(img, w as i32));
        let menu = StartMenu::new(tables[0]);
        // gEquipMenu is set up by vimenudefs.cpp's static initialiser (its
        // icon is a hashed name), so the image holds no count: 40 there
        let mut item_counts = [0x00CD0310u32, 0x00CD0300, 0x00CD02E0, 0x00CD02F0].map(|va| img.i32_at(va + 12).clamp(0, 256));
        item_counts[3] = 40;
        let attack_words = (0..11u32).map(|k| (img.i32_at(0x00CDF6A0 + 8 * k), img.i32_at(0x00CDF6A4 + 8 * k) as i64)).collect();
        let slot_words = (0..23u32).map(|k| (img.i32_at(0x00CDF6F8 + 8 * k), img.i32_at(0x00CDF6FC + 8 * k) as i64)).collect();
        let buddy_menu = menus.def(img, 0x00CD03B0);
        let ignore_menu = menus.def(img, 0x00CD0340);
        StartData { menus, tables, bounds, item_counts, attack_words, slot_words, buddy_menu, ignore_menu, menu }
    }

    /// `OpenStartMenu`'s table: the bank's when a banker is in range, the
    /// expansion's when the mask has bit 1.
    pub fn pick(&mut self, bank: bool, expansion: u32) {
        let k = bank as usize * 2 + (expansion & 2 != 0) as usize;
        self.menu.main.menu = self.tables[k];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_image::Menu;

    /// `VIKeyFlash::Pulse`: at once, then after 15 frames, then 8, 7, ...
    /// down to every frame; nothing once released.
    #[test]
    fn key_flash_repeats_as_the_game_does() {
        let mut f = KeyFlash::default();
        f.press(138);
        let mut when = Vec::new();
        for frame in 0..60 {
            if f.pulse(138) {
                when.push(frame);
            }
        }
        // 0, +16 (wait 15), +9 (8), +8 (7), +7, +6, +5, +4, +3, +2, +2...
        assert_eq!(&when[..6], &[0, 16, 25, 33, 40, 46]);
        assert!(!f.pulse(139), "another key never fires");
        f.release(138);
        assert!((0..30).all(|_| !f.pulse(138)), "a released key stops");
    }

    /// A raster `.0` frame pixels wider than the frame each side, noting
    /// each blit (where, how big) with the anchor it was made under.
    struct Wide(i32, Anchor, Vec<(Anchor, [i32; 2], [i32; 2])>);

    impl Raster for Wide {
        fn blit(&mut self, _: u32, _: u32, at: [i32; 2], _: [i32; 2], b: [i32; 2], _: i32, _: f32) {
            self.2.push((self.1, at, b));
        }
        fn stretch_blit(&mut self, _: u32, _: u32, at: [i32; 2], size: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {
            self.2.push((self.1, at, size));
        }
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            16
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 639, 447]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    impl Layout for Wide {
        fn side(&self) -> i32 {
            self.0
        }
        fn anchor(&mut self, a: Anchor) {
            self.1 = a;
        }
    }

    /// The start menu in a window `side` wider each side: open, drawn once.
    fn drawn(side: i32) -> (StartMenu, Menus, Wide) {
        let vui = crate::pointer::testing::vui();
        let mut menus = Menus::default();
        let m = menus.push(Menu { x00: -1, x04: -1, entries: (0..7).map(|i| crate::client_image::MenuEntry { word: 100 + i, ..Default::default() }).collect() });
        let loc = Location { world: 0, pos: [0.0; 3], cols: 1, rows: 1, cell: 1.0, origin: [0.0; 3], bounds: None };
        let mut sm = StartMenu::new(m);
        sm.visible = true;
        let mut r = Wide(side, Anchor::Middle, Vec::new());
        sm.show_main(&vui, &r, true, 0, &loc);
        sm.draw(&vui, &mut r, &menus, 16, &Stats::default(), &mut Vec::new());
        (sm, menus, r)
    }

    /// In a window wider than 4:3 the panels go to its sides with their
    /// rails and bars, the band runs its whole width and the middle frame is
    /// as wide as they leave; on a 4:3 one every piece is where the game
    /// puts it (the control), and the anchors move nothing there.
    #[test]
    fn the_start_menu_goes_out_to_a_wide_windows_sides() {
        use Anchor::{Left, Right, Wide as W};
        let (_, _, tv) = drawn(0);
        let (_, _, wide) = drawn(106);
        assert!(tv.2.contains(&(W, [-1, -1], [642, 48])), "the game's band");
        assert!(wide.2.contains(&(W, [-107, -1], [854, 48])), "the band the window's whole width");
        // the middle frame from the left panel's edge to the right one's
        assert!(tv.2.iter().any(|b| b.1 == [176, 37]));
        assert!(wide.2.iter().any(|b| b.0 == W && b.1 == [70, 37]), "its corner at the left panel's edge, moved out");
        assert!(!wide.2.iter().any(|b| b.1 == [176, 37]));
        // the panels at their own places, each with its side
        for at in [[451, 48], [451, 37], [451, 313]] {
            assert!(wide.2.iter().any(|b| b.0 == Right && b.1 == at), "right panel {at:?}");
        }
        for at in [[-1, 37], [-1, 313], [18, 56], [18, 56 + 27 * 6], [20, 260]] {
            assert!(wide.2.iter().any(|b| b.0 == Left && b.1 == at), "left panel {at:?}");
        }
        // the same pieces either way but the band and the middle frame
        let plain = |r: &Wide| r.2.iter().filter(|b| b.0 != W).map(|b| (b.1, b.2)).collect::<Vec<_>>();
        assert_eq!(plain(&tv), plain(&wide));
    }

    /// A pointer meets the bars where they were drawn: at the left side of a
    /// wide window (the control: the same point on a 4:3 picture is off
    /// them, and the bar's own place there is on one).
    #[test]
    fn a_pointer_follows_the_bars_to_the_side() {
        let (mut sm, menus, _) = drawn(106);
        let p = [18.0 + 40.0 - 106.0, 56.0 + 27.0 + 10.0];
        assert!(sm.point(p, &menus));
        assert_eq!(sm.main.sel, 1);
        let (mut tv, menus, _) = drawn(0);
        assert!(!tv.point(p, &menus));
        assert!(tv.point([18.0 + 40.0, 56.0 + 27.0 + 10.0], &menus));
    }

    /// The start tables out of the player's CLIENT image (SKIPs without
    /// it): each loses its first entry ("TEST", word 143), and the
    /// sub-menus are reached through the relocation table.
    #[test]
    fn the_start_tables_read_out_of_client() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/client.bin");
        let Some(img) = std::fs::read(&path).ok().and_then(crate::client_image::ClientImage::from_bytes) else {
            eprintln!("SKIP: no run/client.bin");
            return;
        };
        assert!(img.is_frontiers());
        let mut d = StartData::from_image(&img);
        for (bank, x) in [(false, 0), (false, 2), (true, 0), (true, 2)] {
            d.pick(bank, x);
            let m = &d.menus.list[d.menu.main.menu];
            assert!(m.entries.iter().all(|e| e.word != 143), "TEST dropped");
            assert!(m.entries.iter().any(|e| e.sub.is_some()), "sub-menus reached");
            assert_eq!(m.entries.last().unwrap().word, 423, "Quit last");
        }
        assert_eq!(d.bounds[0].map(|b| b[1] - b[0] >= 0), Some(true));
    }
}
