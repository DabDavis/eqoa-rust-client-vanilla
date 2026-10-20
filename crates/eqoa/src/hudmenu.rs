//! The HUD's three quick menus: `VIWndHUDMenu` (CLIENT `viwndhudmenu.cpp`)
//! and how `VIWndGame` builds and switches them (`UpdateQuickBars`,
//! `BuildAbilityMenu0/1`, `BuildQuickMenu`, the d-pad's left and right).
//!
//! Menu 0 holds the abilities on slots 0-4 and, packed after them, the ones
//! on 252-255; menu 1 (the second bar, when the expansion mask has bit 1)
//! slots 5-9; menu 2 the inventory's four quick slots. One is up at a time,
//! a carousel with the selection at its head; using an entry sends what its
//! message says: 0x8060000 an ability (`Send0032`), 0x2030000 an item.

use crate::ui::Vui;
use net::dump::{Ability, NetItem};
use std::collections::BTreeMap;
use vi::ui::{Raster, TextScroll};

/// `VIMenuEntry`, the fields the HUD menus use.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub icon2: i32,
    /// 61: the ability is locked out (the badge)
    pub x08: i32,
    pub msg: u32,
    pub id: i64,
}

impl Default for Entry {
    fn default() -> Self {
        Entry { icon2: -1, x08: -1, msg: 0, id: 0 }
    }
}

/// `VIMenuDef`
#[derive(Clone, Debug)]
pub struct MenuDef {
    pub count: i32,
    pub entries: Vec<Entry>,
}

/// The slot icons of the stacked menu (`sSlotIcons`).
const SLOT_ICONS: [i32; 4] = [5, 5, 3, 2];

/// A key for the menus, as `VIWndHUDMenu::OnKeyDown` and `VIWndGame`'s
/// pad keys take them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key {
    /// W / pad up (138): the selection up
    Up,
    /// S / pad down (139)
    Down,
    /// D / pad right (137): `VIWndGame::Interact`'s cycle
    Right,
    /// A / pad left (136): the other way
    Left,
    /// E / X (135): use the selection
    Use,
}

/// `VIWndHUDMenu`
pub struct HudMenu {
    lit: i32,
    alpha: f32,
    pub sel: i32,
    time: i64,
    index: i32,
    pos: [i32; 2],
    text: TextScroll,
}

impl HudMenu {
    fn new(index: i32) -> Self {
        HudMenu { lit: -1, alpha: 0.3, sel: 0, time: 0, index, pos: [430, 25], text: TextScroll::default() }
    }

    fn clamp(&mut self, menu: &MenuDef) {
        if self.sel >= menu.count {
            self.sel = menu.count - 1;
        }
    }

    /// `Move`: the selection round the carousel (sound 11).
    fn step(&mut self, menu: &MenuDef, d: i32, cues: &mut Vec<crate::sound::Cue>) {
        cues.push(crate::sound::Cue::Ui(11));
        let n = self.sel + d;
        self.sel = if n < 0 {
            menu.count - 1
        } else if n >= menu.count {
            0
        } else {
            n
        };
    }

    /// `OnDrawSelf`: the light fades in fast while a key keeps it lit (20 s)
    /// and out slowly to 0.3, then the list (menus 0 and 1) or the stack.
    fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, menu: &MenuDef, now: i64, name: &dyn Fn(u32, i64) -> Vec<u16>) {
        if self.time < now {
            self.lit = -1;
        }
        self.clamp(menu);
        self.alpha = if self.lit > 0 { (self.alpha + 0.32).min(1.0) } else { (self.alpha - 0.105).max(0.3) };
        self.text.init(vui, r, [self.pos[0] + 81, self.pos[1] + 10], 158, 0, 1);
        self.text.window.open(true);
        self.text.on = (self.alpha > 0.35) as i32;
        match self.index {
            0 | 1 => self.draw_list(vui, r, menu, name),
            _ => self.draw_stack(vui, r, menu, name),
        }
    }

    /// `DrawList`: the title bar with the selection's name, then the entries
    /// down a frame that grows past five, each its icon, badged while locked.
    fn draw_list(&mut self, vui: &Vui, r: &mut dyn Raster, menu: &MenuDef, name: &dyn Fn(u32, i64) -> Vec<u16>) {
        let a = self.alpha;
        let at = self.pos;
        let sel = menu.entries.get(self.sel.max(0) as usize).copied().unwrap_or_default();
        let s = if sel.icon2 != -1 && sel.id >= 0 && sel.msg != 0 { name(sel.msg, sel.id) } else { Vec::new() };
        vui.blit_bar(r, &[], [1.0, 1.0, 1.0, a], at, 27, 28, 162, 3, a, 0, true);
        self.text.text_scroll(vui, r, &s, [1.0, 1.0, 1.0, a], 9);
        let mut pos = [at[0] + 159, at[1] - 12];
        vui.blit_ex(r, 30, pos, 3, a, 0);
        let h = if menu.count > 5 { (menu.count - 5) * 31 } else { 0 };
        pos[1] += 159 + h;
        vui.blit_ex(r, 31, pos, 3, a, 2);
        pos = [pos[0] + 4, pos[1] - 151 - h];
        let mut n = self.sel;
        for _ in 0..menu.count {
            if n >= menu.count {
                n = 0;
            }
            let e = menu.entries.get(n.max(0) as usize).copied().unwrap_or_default();
            let icon = [pos[0] + 7, pos[1] + 2];
            vui.blit_ex(r, 29, pos, 3, a, 0);
            if e.icon2 != -1 {
                // VIUI::Blit(int id, at, a, b): the texels (0, 0) .. (22, 22)
                r.blit(e.icon2 as u32, 0, icon, [0, 0], [22, 22], 3, a);
            }
            if e.x08 == 61 {
                vui.blit_ex_size(r, 61, [icon[0] - 2, icon[1] - 2], [26, 26], 3, a * 0.5, 0);
            }
            pos[1] += 31;
            n += 1;
        }
    }

    /// `DrawStack`: the title bar, then a column of slot icons in a frame.
    fn draw_stack(&mut self, vui: &Vui, r: &mut dyn Raster, menu: &MenuDef, name: &dyn Fn(u32, i64) -> Vec<u16>) {
        let a = self.alpha;
        let at = self.pos;
        let sel = menu.entries.get(self.sel.max(0) as usize).copied().unwrap_or_default();
        let s = if sel.id >= 0 && sel.msg != 0 { name(sel.msg, sel.id) } else { Vec::new() };
        vui.blit_bar(r, &[], [1.0, 1.0, 1.0, a], at, 27, 28, 162, 3, a, 0, true);
        self.text.text_scroll(vui, r, &s, [1.0, 1.0, 1.0, a], 9);
        let pos = [at[0] + 146, at[1]];
        let mut icon = [pos[0] + 17, pos[1] - 5];
        let (top, mid) = (vui.entry(158), vui.entry(121));
        vui.blit_ex(r, top.x00, icon, 3, a, 0);
        icon[1] += top.uv1[1];
        for k in 0..4 {
            let hgt = if k == 3 { mid.uv1[1] - 4 } else { mid.uv1[1] };
            vui.blit_ex_size(r, mid.x00, icon, [34, hgt], 3, a, 0);
            icon[1] += hgt;
        }
        vui.blit_ex_size(r, 158, icon, [top.uv1[0], 3], 3, a, 2);
        let mut list = [pos[0] + 17 + 6, pos[1] - 5 + 8];
        let mut n = self.sel;
        for _ in 0..menu.count {
            if n >= menu.count {
                n = 0;
            }
            vui.blit_ex(r, SLOT_ICONS[(n.max(0) as usize).min(3)], list, 3, a, 0);
            list[1] += 32;
            n += 1;
        }
    }
}

/// The three menus and which is up (`m_hudSel`).
pub struct HudMenus {
    pub sel: i32,
    pub menus: [HudMenu; 3],
    pub defs: [MenuDef; 3],
}

impl Default for HudMenus {
    fn default() -> Self {
        HudMenus {
            sel: 0,
            menus: [HudMenu::new(0), HudMenu::new(1), HudMenu::new(2)],
            defs: [
                MenuDef { count: 5, entries: vec![Entry::default(); 9] },
                MenuDef { count: 5, entries: vec![Entry::default(); 5] },
                MenuDef { count: 4, entries: vec![Entry::default(); 4] },
            ],
        }
    }
}

impl HudMenus {
    /// `UpdateQuickBars`' three HUD menus, from the ability list (in its
    /// map's order: by id) and the inventory.
    pub fn build(&mut self, abilities: &BTreeMap<i64, Ability>, inventory: &[NetItem]) {
        // BuildAbilityMenu0
        let m0 = &mut self.defs[0];
        for e in &mut m0.entries {
            *e = Entry::default();
        }
        let (mut n1, mut n2) = (0, 0);
        let item = |a: &Ability| Entry { icon2: a.icon_front as i32, x08: if a.lockout > 0 { 61 } else { -1 }, msg: 0x8060000, id: a.id };
        for a in abilities.values() {
            if a.valid == 0 {
                continue;
            }
            let mut slot = a.slot;
            if slot >= 9 {
                if !(252..256).contains(&slot) {
                    continue;
                }
                slot = n2 + 5;
                n2 += 1;
            } else {
                if slot >= 5 {
                    continue;
                }
                n1 += 1;
            }
            if (0..9).contains(&slot) {
                m0.entries[slot as usize] = item(a);
            }
        }
        for e in &mut m0.entries {
            if e.icon2 == -1 {
                *e = Entry::default();
            }
        }
        let n1 = n1.clamp(0, 5);
        m0.count = if n2 > 0 { (n2 + 5).min(9) } else { n1.max(5) } as i32;
        // BuildAbilityMenu1
        let m1 = &mut self.defs[1];
        for e in &mut m1.entries {
            *e = Entry::default();
        }
        let mut n = 0;
        for a in abilities.values() {
            if a.valid != 0 && (5..10).contains(&a.slot) {
                n += 1;
                m1.entries[(a.slot - 5) as usize] = item(a);
            }
        }
        m1.count = n.clamp(0, 5).max(5);
        // BuildQuickMenu
        let m2 = &mut self.defs[2];
        for e in &mut m2.entries {
            *e = Entry::default();
        }
        for (k, it) in inventory.iter().enumerate() {
            let q = it.quick_slot;
            if (0..4).contains(&q) {
                // (the count, `m_number`, is not drawn by the stack)
                m2.entries[q as usize] = Entry { icon2: -1, x08: -1, msg: 0x2030000, id: k as i64 };
            }
        }
    }

    /// The menu up, drawn.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, now: i64, name: &dyn Fn(u32, i64) -> Vec<u16>) {
        let k = self.sel.clamp(0, 2) as usize;
        let def = self.defs[k].clone();
        self.menus[k].draw(vui, r, &def, now, name);
    }

    /// A pointer at frame point `p`, in the menu's own place (before a wide
    /// window moves it out to the right; the client's own, as the game takes
    /// the pad and the keyboard): on an entry of the menu up, that entry
    /// selected and the key that uses it; on the title bar, the next menu
    /// (as the d-pad's right).
    pub fn point(&mut self, p: [f32; 2]) -> Option<Key> {
        let k = self.sel.clamp(0, 2) as usize;
        let count = self.defs[k].count;
        let m = &mut self.menus[k];
        let at = m.pos;
        let (x, y) = (p[0] as i32, p[1] as i32);
        if (at[0]..at[0] + 162).contains(&x) && (at[1]..at[1] + 28).contains(&y) {
            return Some(Key::Right);
        }
        // the carousel from the selection down: a list's rows 31 apart from
        // (+163, -4), the stack's slots 32 apart from (+169, +3)
        let (x0, y0, w, step) = if k < 2 { (at[0] + 163, at[1] - 4, 36, 31) } else { (at[0] + 169, at[1] + 3, 28, 32) };
        let row = (y - y0).div_euclid(step);
        if count <= 0 || !(x0..x0 + w).contains(&x) || !(0..count).contains(&row) {
            return None;
        }
        m.sel = (m.sel + row).rem_euclid(count);
        Some(Key::Use)
    }

    /// A key in play: the menu up takes up, down and use (lit for 20 s);
    /// left and right switch menus, menu 1 only with the expansion's second
    /// bar (`second`). Returns what was used: (message, id).
    pub fn key(&mut self, key: Key, now: i64, second: bool, cues: &mut Vec<crate::sound::Cue>) -> Option<(u32, i64)> {
        let k = self.sel.clamp(0, 2) as usize;
        let def = self.defs[k].clone();
        let m = &mut self.menus[k];
        let mut used = None;
        match key {
            Key::Use => {
                let e = def.entries.get(m.sel.max(0) as usize).copied().unwrap_or_default();
                if e.msg != 0 && e.id >= 0 {
                    used = Some((e.msg, e.id));
                }
            }
            Key::Up => m.step(&def, -1, cues),
            Key::Down => m.step(&def, 1, cues),
            Key::Right | Key::Left => {}
        }
        m.lit = 1;
        m.time = now + 20000;
        match key {
            // VIWndGame::Interact
            Key::Right => {
                self.sel = match self.sel {
                    0 => {
                        if second {
                            1
                        } else {
                            2
                        }
                    }
                    1 => 2,
                    _ => 0,
                };
                self.relight(now);
            }
            // VIWndGame::Attack
            Key::Left => {
                self.sel = match self.sel {
                    0 => 2,
                    1 => 0,
                    _ => second as i32,
                };
                self.relight(now);
            }
            _ => {}
        }
        used
    }

    /// The focus passing from one HUD menu to another relights it
    /// (`VIWndHUDMenu::OnMessage` 295).
    fn relight(&mut self, now: i64) {
        let m = &mut self.menus[self.sel.clamp(0, 2) as usize];
        m.lit = 1;
        m.alpha = 0.3;
        m.time = now + 20000;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pointer on the quick menu up: the title bar is the next menu, a
    /// row of the carousel (counted from the selection) is selected and
    /// used, anything else is nothing.
    #[test]
    fn a_pointer_selects_and_uses_the_row_under_it() {
        let mut h = HudMenus::default();
        h.defs[0] = MenuDef { count: 4, entries: vec![Entry::default(); 4] };
        h.menus[0].sel = 3;
        let at = h.menus[0].pos;
        assert_eq!(h.point([(at[0] + 50) as f32, (at[1] + 10) as f32]), Some(Key::Right));
        // the third row down from the selection (3): entry (3 + 2) % 4
        assert_eq!(h.point([(at[0] + 180) as f32, (at[1] - 4 + 2 * 31 + 15) as f32]), Some(Key::Use));
        assert_eq!(h.menus[0].sel, 1);
        assert_eq!(h.point([(at[0] + 180) as f32, (at[1] - 4 + 4 * 31 + 15) as f32]), None, "past the last row");
        assert_eq!(h.point([(at[0] + 120) as f32, (at[1] + 60) as f32]), None, "beside the rows");
        // an empty menu takes nothing but its title
        h.defs[0].count = 0;
        assert_eq!(h.point([(at[0] + 180) as f32, (at[1] + 11) as f32]), None);
    }
}
