//! The HUD's quick menus: `VIWndHUDMenu` (CLIENT `viwndhudmenu.cpp`) and how
//! `VIWndGame` builds and switches them (`UpdateQuickBars`,
//! `BuildAbilityMenu0/1`, `BuildQuickMenu`, the d-pad's left and right).
//!
//! Menu 0 holds the abilities on slots 0-4 and, packed after them, the ones
//! on 252-255; menu 1 (the second bar, when the expansion mask has bit 1)
//! slots 5-9; menu 2 the inventory's four quick slots. One is up at a time,
//! a carousel with the selection at its head; using an entry sends what its
//! message says: 0x8060000 an ability (`Send0032`), 0x2030000 an item.
//!
//! SOE's final live client (`gear`: a server that keeps weapon sets) adds menu
//! 3, the four sets and bare hands (`BuildGearMenu` 0x01359FD0, drawn by its
//! own `DrawGear` 0x0136A888), round the carousel after the items; its item
//! menu shows only potions and gems (`SetQuickMenuItem` 0x01359E60) and its
//! slots' icons are a gem's on 3 too (`sSlotIcons` 0x013FBA30, {5, 5, 3, 3}).
//!
//! The client's own beyond them (the Extended page's Ability Bars, with the
//! second bar there): menus 4 and 5, bars 3 and 4, the abilities on slots
//! 10-14 and 15-19, built and drawn as menu 1 is and reached round the
//! carousel after it, the items and the sets still after them. The game
//! sends a slot as it is (0x0033 puts an ability on one, 0x0032 uses what is
//! on one) and CLIENT shows only 0-9 and 252-255, so what is on 10-19 is the
//! server's to keep: a server that keeps any slot (go-eqoa-server does)
//! needs no change for them.
//!
//! Stacked Spell Bars (Extended > UI > HUD, Jibwiz's ticket 1913, "move your
//! spellbars vertically, and nest them x2 evenly above the chat box stacked on
//! top of each other. Scale them down about 5% and remove the pop up horizontal
//! box on the current bar that tells you what the spell is. This should be for
//! PC players"): the client's own, on a computer (the keyboard and the mouse;
//! a controller's or a touch screen's play keeps the carousel). The first bar
//! (menu 0, its packed 252-255 too) and the second (menu 1) are drawn as two
//! rows of their slots' frames, the first over the second, just above the
//! chat box's full height and moving with it (`draw_stacked`, `Piece::Chat`),
//! at 95% (`STACKED_SCALE`), with no title bar: no name pops up. A click on a
//! slot uses it, a right click turns its auto (`stacked_at`), and each slot
//! of both can be given a key on the Controls page (`binds::bar_slot`,
//! `slot_entry`). The arrows and E still walk and use the bar up, its
//! selection outlined.

use crate::gear::Gear;
use crate::ui::Vui;
use net::dump::{Ability, ItemList};
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

/// The slot icons of the stacked menu (`sSlotIcons`): the disc's, a weapon's
/// on 3, and the live client's, a gem's there too.
pub const SLOT_ICONS: [i32; 4] = [5, 5, 3, 2];
pub const SLOT_ICONS_LIVE: [i32; 4] = [5, 5, 3, 3];

/// The menus: the game's three (bars 1 and 2, the items), the live client's
/// weapon sets, and the client's bars 3 and 4.
pub const MENUS: usize = 6;
/// The items' menu, the one drawn as a stack.
const ITEMS: usize = 2;
/// The weapon sets' menu (`m_hud[3]`).
const GEAR: usize = 3;
/// Round the carousel: the bars in order, then the items, then the sets.
const ORDER: [usize; MENUS] = [0, 1, 4, 5, 2, 3];
/// The most bars the client shows (the Extended page's Ability Bars).
pub const MAX_BARS: i32 = 4;

/// The first slot of the bar menu `k` shows, five from it: menu 1's 5, and
/// the client's bars 3 and 4 (menus 4 and 5) 10 and 15 (menu 0 is built as
/// the game builds it).
fn first_slot(k: usize) -> i64 {
    if k == 1 { 5 } else { (k as i64 - 2) * 5 }
}

/// What the carousel holds besides the first bar and the items: the ability
/// bars (one; the game's two with the expansion mask's bit 1; up to
/// `MAX_BARS` with the Extended page's Ability Bars) and the weapon sets (a
/// server that keeps them, `gear::Gear::on`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bars {
    pub abilities: i32,
    pub gear: bool,
}

impl Default for Bars {
    fn default() -> Self {
        Bars { abilities: 1, gear: false }
    }
}

/// Whether menu `k` is in the carousel: the first bar and the items always,
/// the other bars with as many, the sets with theirs.
fn shown(k: usize, bars: Bars) -> bool {
    match k {
        0 | ITEMS => true,
        1 => bars.abilities >= 2,
        GEAR => bars.gear,
        _ => bars.abilities >= k as i32 - 1,
    }
}

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

/// Stacked Spell Bars' scale (ticket 1913: "scale them down about 5%").
pub const STACKED_SCALE: f32 = 0.95;

/// A slot's cell in the carousel's list, frame pixels (`row_at`'s: 36 across
/// from its frame's left, its rows 31 apart), which the stacked bars scale.
const CELL: [i32; 2] = [36, 31];

/// How far the stacked bars stand above the chat box's window top, frame pixels:
/// its panel is drawn 10 inside it, so they stand 2 above the panel.
const STACKED_GAP: i32 = -8;

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
    /// and out slowly to 0.3, then the list (the bars: menus 0, 1, 4 and 5),
    /// the stack (2, its slot icons the live client's with `live`) or the
    /// sets (3).
    #[allow(clippy::too_many_arguments)]
    fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, menu: &MenuDef, now: i64, name: &dyn Fn(u32, i64) -> Vec<u16>, live: bool, auto: &[i64]) {
        if self.time < now {
            self.lit = -1;
        }
        self.clamp(menu);
        self.alpha = if self.lit > 0 { (self.alpha + 0.32).min(1.0) } else { (self.alpha - 0.105).max(0.3) };
        self.text.init(vui, r, [self.pos[0] + 81, self.pos[1] + 10], 158, 0, 1);
        self.text.window.open(true);
        self.text.on = (self.alpha > 0.35) as i32;
        match self.index as usize {
            ITEMS => self.draw_stack(vui, r, menu, name, live),
            GEAR => self.draw_gear(vui, r, menu, name),
            _ => self.draw_list(vui, r, menu, name, auto),
        }
    }

    /// `DrawList`: the title bar with the selection's name, then the entries
    /// down a frame that grows past five, each its icon, badged while locked;
    /// an ability the player made auto (`autocast`, the client's own) marked
    /// with a gold "A" in its icon's corner.
    fn draw_list(&mut self, vui: &Vui, r: &mut dyn Raster, menu: &MenuDef, name: &dyn Fn(u32, i64) -> Vec<u16>, auto: &[i64]) {
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
            if e.msg == 0x8060000 && e.icon2 != -1 && auto.contains(&e.id) {
                draw_auto_mark(r, icon, a);
            }
            pos[1] += 31;
            n += 1;
        }
    }

    /// `DrawStack`: the title bar, then a column of slot icons in a frame.
    fn draw_stack(&mut self, vui: &Vui, r: &mut dyn Raster, menu: &MenuDef, name: &dyn Fn(u32, i64) -> Vec<u16>, live: bool) {
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
        let icons = if live { SLOT_ICONS_LIVE } else { SLOT_ICONS };
        let mut list = [pos[0] + 17 + 6, pos[1] - 5 + 8];
        let mut n = self.sel;
        for _ in 0..menu.count {
            if n >= menu.count {
                n = 0;
            }
            vui.blit_ex(r, icons[(n.max(0) as usize).min(3)], list, 3, a, 0);
            list[1] += 32;
            n += 1;
        }
    }

    /// The live client's `DrawGear` (0x0136A888): the stack's title bar and
    /// frame, a row taller (five), and each entry its badge, then its icon,
    /// 22 by 22 (`VIUI::Blit(id, at, (22, 22), (0, 0))`) where the stack draws
    /// its slot icons; the title is the selected set's name.
    fn draw_gear(&mut self, vui: &Vui, r: &mut dyn Raster, menu: &MenuDef, name: &dyn Fn(u32, i64) -> Vec<u16>) {
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
        for k in 0..5 {
            let hgt = if k == 4 { mid.uv1[1] - 4 } else { mid.uv1[1] };
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
            let e = menu.entries.get(n.max(0) as usize).copied().unwrap_or_default();
            for id in [e.x08, e.icon2] {
                if id != -1 {
                    r.blit(id as u32, 0, list, [0, 0], [22, 22], 3, a);
                }
            }
            list[1] += 32;
            n += 1;
        }
    }
}

/// The auto mark (`autocast`): a gold "A" on a dark square at the bottom
/// right of a 22-pixel icon at `icon`, at the menu's alpha `a`.
fn draw_auto_mark(r: &mut dyn Raster, icon: [i32; 2], a: f32) {
    let at = [icon[0] + 13, icon[1] + 11];
    r.fill_fan([0.0, 0.0, 0.0, 0.7 * a], [at, [at[0] + 11, at[1]], [at[0] + 11, at[1] + 12], [at[0], at[1] + 12]]);
    r.draw_centered_string([at[0] + 6, at[1] + 6], [1.0, 0.82, 0.25, a], &[b'A' as u16], 0);
}

/// A one-pixel outline of rectangle `c` (x0, y0, x1, y1) in `color`.
fn outline(r: &mut dyn Raster, c: [i32; 4], color: [f32; 4]) {
    let [x0, y0, x1, y1] = c;
    for [a, b] in [[[x0, y0], [x1, y0 + 1]], [[x0, y1 - 1], [x1, y1]], [[x0, y0], [x0 + 1, y1]], [[x1 - 1, y0], [x1, y1]]] {
        r.fill_fan(color, [a, [b[0], a[1]], b, [a[0], b[1]]]);
    }
}

/// The menus and which is up (`m_hudSel`), and whether the items' stack
/// is drawn with the live client's slot icons (the layout `build` saw);
/// the abilities the player made auto (`autocast`), marked as drawn.
pub struct HudMenus {
    pub sel: i32,
    pub menus: [HudMenu; MENUS],
    pub defs: [MenuDef; MENUS],
    live: bool,
    pub auto: Vec<i64>,
    /// Stacked Spell Bars are drawn (the module's doc): the pointer reads
    /// the stacked rows' cells (`stacked`), not the carousel's.
    pub stacked: bool,
    /// The cells the stacked bars last drew: (menu, entry, rectangle x0, y0,
    /// x1, y1 in the chat box's frame points).
    stacked_hits: Vec<(usize, i32, [i32; 4])>,
}

impl Default for HudMenus {
    fn default() -> Self {
        HudMenus {
            sel: 0,
            menus: std::array::from_fn(|k| HudMenu::new(k as i32)),
            defs: std::array::from_fn(|k| match k {
                0 => MenuDef { count: 5, entries: vec![Entry::default(); 9] },
                ITEMS => MenuDef { count: 4, entries: vec![Entry::default(); 4] },
                _ => MenuDef { count: 5, entries: vec![Entry::default(); 5] },
            }),
            live: false,
            auto: Vec::new(),
            stacked: false,
            stacked_hits: Vec::new(),
        }
    }
}

impl HudMenus {
    /// `UpdateQuickBars`' HUD menus, and the client's bars as menu 1 is
    /// built, from the ability list (in its map's order: by id), the
    /// inventory and the weapon sets; a menu no longer in the carousel (the
    /// Extended page's Ability Bars turned down, a server without sets) is
    /// not left up.
    pub fn build(&mut self, abilities: &BTreeMap<i64, Ability>, inventory: &ItemList, gear: &Gear, bars: Bars) {
        self.live = bars.gear;
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
        // BuildAbilityMenu1, and bars 3 and 4 as it
        for k in [1, 4, 5] {
            let first = first_slot(k);
            let m = &mut self.defs[k];
            for e in &mut m.entries {
                *e = Entry::default();
            }
            let mut n = 0;
            for a in abilities.values() {
                if a.valid != 0 && (first..first + 5).contains(&a.slot) {
                    n += 1;
                    m.entries[(a.slot - first) as usize] = item(a);
                }
            }
            m.count = n.clamp(0, 5).max(5);
        }
        // BuildQuickMenu; the live client's SetQuickMenuItem takes only a
        // potion or a gem (type 2 or 7)
        let m2 = &mut self.defs[2];
        for e in &mut m2.entries {
            *e = Entry::default();
        }
        for (k, it) in inventory.items.iter().enumerate() {
            let q = it.quick_slot;
            if (0..4).contains(&q) && (!bars.gear || matches!(it.item.p4c, 2 | 7)) {
                // (the count, `m_number`, is not drawn by the stack)
                m2.entries[q as usize] = Entry { icon2: -1, x08: -1, msg: 0x2030000, id: k as i64 };
            }
        }
        // the live client's BuildGearMenu (0x01359FD0) and SetGearMenuItem
        // (0x0135A0C8): a set not empty, then bare hands, each its icon and
        // badge, using it UseGearSet's message
        let m3 = &mut self.defs[GEAR];
        for e in &mut m3.entries {
            *e = Entry::default();
        }
        for k in 0..crate::gear::ENTRIES as i64 {
            if k == crate::gear::BARE_HANDS || !gear.empty(k) {
                m3.entries[k as usize] = Entry { icon2: gear.icon(k, inventory), x08: gear.badge(k, inventory), msg: crate::gear::MSG, id: k };
            }
        }
        if !shown(self.up(), bars) {
            self.sel = 0;
        }
    }

    /// The menu up.
    fn up(&self) -> usize {
        self.sel.clamp(0, MENUS as i32 - 1) as usize
    }

    /// The menu up, drawn.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, now: i64, name: &dyn Fn(u32, i64) -> Vec<u16>) {
        let k = self.up();
        let def = self.defs[k].clone();
        let live = self.live;
        self.menus[k].draw(vui, r, &def, now, name, live, &self.auto);
    }

    /// Stacked Spell Bars (the module's doc): the first bar's slots over the
    /// second's (the second with the expansion's second bar), each a slot's
    /// frame scaled `STACKED_SCALE` with its icon, lock badge and auto mark,
    /// the rows standing on `chat_top` (the chat box's top at its full
    /// height, frame points, as it is drawn) from its left at `left`; the
    /// menu up's selection outlined; no title bar. The cells are noted for
    /// the pointer (`stacked_at`).
    pub fn draw_stacked(&mut self, vui: &Vui, r: &mut dyn Raster, chat_top: i32, left: i32, bars: Bars) {
        self.stacked_hits.clear();
        let frame = vui.entry(29);
        let s = |v: i32| (v as f32 * STACKED_SCALE).round() as i32;
        let (cw, ch) = (s(CELL[0]), s(CELL[1]));
        let rows: Vec<usize> = if bars.abilities >= 2 { vec![0, 1] } else { vec![0] };
        let up = self.up();
        for (k, &menu) in rows.iter().enumerate() {
            let y = chat_top - STACKED_GAP - ch * (rows.len() - k) as i32;
            let def = &self.defs[menu];
            for n in 0..def.count.max(0) {
                let x = left + (n as f32 * CELL[0] as f32 * STACKED_SCALE).round() as i32;
                let at = [x, y];
                r.stretch_blit(vui.entry_id(&frame), 0, at, [cw, ch], frame.uv0, frame.uv1, 3, 1.0);
                let e = def.entries.get(n as usize).copied().unwrap_or_default();
                // the icon where the list puts it in its frame, scaled
                let icon = [x + s(7), y + s(2)];
                if e.icon2 != -1 {
                    r.stretch_blit(e.icon2 as u32, 0, icon, [s(22), s(22)], [0, 0], [22, 22], 3, 1.0);
                }
                if e.x08 == 61 {
                    let badge = vui.entry(61);
                    r.stretch_blit(vui.entry_id(&badge), 0, [icon[0] - s(2), icon[1] - s(2)], [s(26), s(26)], badge.uv0, [26, 26], 3, 0.5);
                }
                if e.msg == 0x8060000 && e.icon2 != -1 && self.auto.contains(&e.id) {
                    draw_auto_mark(r, icon, 1.0);
                }
                if menu == up && n == self.menus[menu].sel {
                    outline(r, [icon[0] - 2, icon[1] - 2, icon[0] + s(22) + 1, icon[1] + s(22) + 1], [1.0, 0.85, 0.3, 1.0]);
                }
                self.stacked_hits.push((menu, n, [x, y, x + cw, y + ch]));
            }
        }
    }

    /// The stacked bars' cell at frame point `p` (in the chat box's place, as
    /// `draw_stacked` drew it): its menu and entry.
    pub fn stacked_at(&self, p: [f32; 2]) -> Option<(usize, i32)> {
        let (x, y) = (p[0] as i32, p[1] as i32);
        self.stacked_hits.iter().find(|(_, _, c)| (c[0]..c[2]).contains(&x) && (c[1]..c[3]).contains(&y)).map(|&(m, n, _)| (m, n))
    }

    /// The entry an ability bar's slot shows (`binds::bar_slot`: bar 0 is
    /// menu 0, bar 1 menu 1), when it holds something to use.
    pub fn slot_entry(&self, bar: usize, slot: usize) -> Option<(u32, i64)> {
        let menu = [0, 1].get(bar).copied()?;
        let e = self.defs[menu].entries.get(slot).copied()?;
        (e.msg != 0 && e.id >= 0 && e.icon2 != -1).then_some((e.msg, e.id))
    }

    /// A pointer at frame point `p`, in the menu's own place (before a wide
    /// window moves it out to the right, or a tall one up to its top; the
    /// client's own, as the game takes the pad and the keyboard): on an
    /// entry of the menu up, that entry selected and the key that uses it;
    /// on the title bar, the next menu (as the d-pad's right). With the bars
    /// stacked (`stacked`), `p` is in the chat box's place: a slot's cell
    /// makes its bar the one up and its entry the selection, and is used.
    pub fn point(&mut self, p: [f32; 2]) -> Option<Key> {
        if self.stacked {
            let (menu, n) = self.stacked_at(p)?;
            self.sel = menu as i32;
            self.menus[menu].sel = n;
            return Some(Key::Use);
        }
        let k = self.up();
        let at = self.menus[k].pos;
        let (x, y) = (p[0] as i32, p[1] as i32);
        if (at[0]..at[0] + 162).contains(&x) && (at[1]..at[1] + 28).contains(&y) {
            return Some(Key::Right);
        }
        let n = self.row_at(p)?;
        self.menus[k].sel = n;
        Some(Key::Use)
    }

    /// The entry of the menu up under frame point `p` (as `point` reads it),
    /// its index in the menu; the selection left as it is.
    fn row_at(&self, p: [f32; 2]) -> Option<i32> {
        let k = self.up();
        let count = self.defs[k].count;
        let m = &self.menus[k];
        let at = m.pos;
        let (x, y) = (p[0] as i32, p[1] as i32);
        // the carousel from the selection down: a list's (a bar's) rows 31
        // apart from (+163, -4), the stack's slots and the sets 32 apart from
        // (+169, +3)
        let (x0, y0, w, step) = if matches!(k, ITEMS | GEAR) { (at[0] + 169, at[1] + 3, 28, 32) } else { (at[0] + 163, at[1] - 4, 36, 31) };
        let row = (y - y0).div_euclid(step);
        if count <= 0 || !(x0..x0 + w).contains(&x) || !(0..count).contains(&row) {
            return None;
        }
        Some((m.sel + row).rem_euclid(count))
    }

    /// The ability under frame point `p` on a bar (`point`'s place), its id:
    /// what a right click or a long press makes auto (`autocast`).
    pub fn ability_at(&self, p: [f32; 2]) -> Option<i64> {
        let (menu, n) = if self.stacked { self.stacked_at(p)? } else { (self.up(), self.row_at(p)?) };
        let e = self.defs[menu].entries.get(n as usize).copied()?;
        (e.msg == 0x8060000 && e.icon2 != -1).then_some(e.id)
    }

    /// The ability selected on the bar up, its id (○ or E held on it: `autocast`).
    pub fn selected_ability(&self) -> Option<i64> {
        let k = self.up();
        let e = self.defs[k].entries.get(self.menus[k].sel.max(0) as usize).copied()?;
        (e.msg == 0x8060000 && e.icon2 != -1).then_some(e.id)
    }

    /// A key in play: the menu up takes up, down and use (lit for 20 s);
    /// left and right switch menus round the carousel (menu 1 only with the
    /// second bar, menus 4 and 5 with bars 3 and 4, menu 3 only with the
    /// sets). Returns what was used:
    /// (message, id). `switch`: left and right go round the menus
    /// (`VIWndGame::Interact` and `Attack`, play's alone); without it they
    /// only light the menu up.
    pub fn key(&mut self, key: Key, now: i64, bars: Bars, switch: bool, cues: &mut Vec<crate::sound::Cue>) -> Option<(u32, i64)> {
        let k = self.up();
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
            _ if !switch => {}
            // VIWndGame::Interact: the next menu round
            Key::Right => {
                self.sel = self.round(1, bars) as i32;
                self.relight(now);
            }
            // VIWndGame::Attack: the one before
            Key::Left => {
                self.sel = self.round(-1, bars) as i32;
                self.relight(now);
            }
            _ => {}
        }
        used
    }

    /// The menu `d` (1, -1) round the carousel from the one up, past those
    /// not shown: the disc's 0, 1, 2 with the second bar, 0 and 2 without
    /// (`Interact`'s and `Attack`'s switches), the live client's sets after
    /// the items (its `Interact` 0x0135E680: 0, 1 or 2, 2, 3, 0; its `Attack`
    /// the other way round), and the client's bars 3 and 4 between bar 2 and
    /// the items.
    fn round(&self, d: i32, bars: Bars) -> usize {
        let n = ORDER.len() as i32;
        let at = ORDER.iter().position(|&k| k == self.up()).unwrap_or(0) as i32;
        (1..n).map(|i| ORDER[(at + d * i).rem_euclid(n) as usize]).find(|&k| shown(k, bars)).unwrap_or(0)
    }

    /// The focus passing from one HUD menu to another relights it
    /// (`VIWndHUDMenu::OnMessage` 295).
    fn relight(&mut self, now: i64) {
        let m = &mut self.menus[self.up()];
        m.lit = 1;
        m.alpha = 0.3;
        m.time = now + 20000;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gear::{BARE_HANDS, MSG};
    use net::dump::{Item, NetItem};

    const DISC: Bars = Bars { abilities: 2, gear: false };
    const LIVE: Bars = Bars { abilities: 2, gear: true };

    /// The auto mark (`autocast`): an "A" drawn on an auto ability's row
    /// alone, none without the mark; a right click's row read without
    /// moving the selection.
    #[test]
    fn auto_abilities_are_marked_and_found_under_the_pointer() {
        struct Texts(Vec<Vec<u16>>);
        impl Raster for Texts {
            fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
            fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
            fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
            fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
            fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], s: &[u16]) {
                self.0.push(s.to_vec());
            }
            fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], s: &[u16], _: u32) {
                self.0.push(s.to_vec());
            }
            fn string_width(&self, s: &[u16]) -> i32 {
                s.len() as i32 * 8
            }
            fn point_size(&self) -> i32 {
                16
            }
            fn viewport(&self) -> [i32; 4] {
                [0, 0, 640, 448]
            }
            fn set_viewport(&mut self, _: [i32; 4]) {}
        }
        let vui = crate::pointer::testing::vui();
        let mut h = HudMenus::default();
        let ab = |id: i64| Entry { icon2: 5, x08: -1, msg: 0x8060000, id };
        h.defs[0] = MenuDef { count: 5, entries: vec![ab(11), ab(12), ab(13), Entry::default(), Entry::default()] };
        let name = |_: u32, _: i64| Vec::new();
        let marks = |h: &mut HudMenus| {
            let mut r = Texts(Vec::new());
            h.draw(&vui, &mut r, 0, &name);
            r.0.iter().filter(|t| t.as_slice() == [b'A' as u16]).count()
        };
        assert_eq!(marks(&mut h), 0, "control: none auto, no mark");
        h.auto = vec![12, 99];
        assert_eq!(marks(&mut h), 1, "the one auto on the bar marked");
        let at = h.menus[0].pos;
        h.menus[0].sel = 1;
        let row = |n: i32| [(at[0] + 170) as f32, (at[1] - 4 + 31 * n + 10) as f32];
        assert_eq!(h.ability_at(row(0)), Some(12), "the selection's row");
        assert_eq!(h.ability_at(row(1)), Some(13));
        assert_eq!(h.ability_at(row(2)), None, "an empty slot");
        assert_eq!(h.ability_at([(at[0] + 50) as f32, (at[1] + 10) as f32]), None, "the title bar");
        assert_eq!(h.menus[0].sel, 1, "the selection not moved");
        assert_eq!(h.selected_ability(), Some(12));
    }

    /// Stacked Spell Bars (ticket 1913): two rows of five cells, the first bar
    /// over the second, standing on the chat box's top, each 95% of a slot's
    /// frame, and no name drawn (the carousel's title, the control, draws the
    /// selection's); a click on a cell makes its bar the one up and uses its
    /// entry; a right click finds its ability; a bar's slot key finds its
    /// entry; with one bar, one row.
    #[test]
    fn the_stacked_bars_are_two_rows_with_no_name() {
        struct Rec(Vec<Vec<u16>>, Vec<([i32; 2], [i32; 2])>);
        impl Raster for Rec {
            fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
            fn stretch_blit(&mut self, _: u32, _: u32, at: [i32; 2], size: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {
                self.1.push((at, size));
            }
            fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
            fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
            fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], s: &[u16]) {
                self.0.push(s.to_vec());
            }
            fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], s: &[u16], _: u32) {
                self.0.push(s.to_vec());
            }
            fn string_width(&self, s: &[u16]) -> i32 {
                s.len() as i32 * 8
            }
            fn point_size(&self) -> i32 {
                16
            }
            fn viewport(&self) -> [i32; 4] {
                [0, 0, 640, 448]
            }
            fn set_viewport(&mut self, _: [i32; 4]) {}
        }
        let vui = crate::pointer::testing::vui();
        let mut h = HudMenus::default();
        let ab = |id: i64| Entry { icon2: 5, x08: -1, msg: 0x8060000, id };
        h.defs[0] = MenuDef { count: 5, entries: vec![ab(11), ab(12), ab(13), Entry::default(), Entry::default()] };
        h.defs[1] = MenuDef { count: 5, entries: vec![Entry::default(), ab(22), Entry::default(), Entry::default(), ab(25)] };
        let name = |_: u32, _: i64| crate::ui::wide("Fire Bolt");
        let mut r = Rec(Vec::new(), Vec::new());
        h.draw(&vui, &mut r, 0, &name);
        assert!(!r.0.is_empty(), "the control: the carousel's title draws the name");
        let mut r = Rec(Vec::new(), Vec::new());
        h.stacked = true;
        h.draw_stacked(&vui, &mut r, 320, 216, DISC);
        assert!(r.0.is_empty(), "no name popped up: {:?}", r.0);
        assert_eq!(h.stacked_hits.len(), 10);
        let hits = h.stacked_hits.clone();
        let cell = |m: usize, n: i32| hits.iter().find(|c| c.0 == m && c.1 == n).unwrap().2;
        let (a, b) = (cell(0, 0), cell(1, 0));
        let s = |v: i32| (v as f32 * STACKED_SCALE).round() as i32;
        assert_eq!([a[2] - a[0], a[3] - a[1]], [s(CELL[0]), s(CELL[1])], "95% of a slot's cell");
        assert!(a[3] <= b[1] && b[3] == 320 - STACKED_GAP, "the first bar over the second, on the chat box's panel: {a:?} {b:?}");
        assert_eq!((a[0], b[0]), (216, 216), "evenly, from the chat box's left");
        assert_eq!(cell(0, 4)[1], a[1], "a row");
        // icons drawn 21 for 22
        assert!(r.1.iter().any(|&(_, size)| size == [21, 21]));
        let mid = |c: [i32; 4]| [(c[0] + c[2]) as f32 / 2.0, (c[1] + c[3]) as f32 / 2.0];
        assert_eq!(h.point(mid(cell(1, 4))), Some(Key::Use));
        assert_eq!((h.sel, h.menus[1].sel), (1, 4), "bar 2 up, its fifth selected");
        assert_eq!(h.ability_at(mid(cell(0, 2))), Some(13));
        assert_eq!(h.ability_at(mid(cell(1, 0))), None, "an empty slot");
        assert_eq!(h.point([0.0, 0.0]), None);
        assert_eq!((h.slot_entry(0, 1), h.slot_entry(1, 4), h.slot_entry(1, 0), h.slot_entry(2, 0)), (Some((0x8060000, 12)), Some((0x8060000, 25)), None, None));
        h.draw_stacked(&vui, &mut r, 320, 216, Bars { abilities: 1, gear: false });
        assert_eq!(h.stacked_hits.len(), 5, "one bar, one row");
    }

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
        // the sets' rows are the stack's: 32 apart from (+169, +3)
        h.sel = 3;
        h.menus[3].sel = 0;
        assert_eq!(h.point([(at[0] + 180) as f32, (at[1] + 3 + 4 * 32 + 10) as f32]), Some(Key::Use));
        assert_eq!(h.menus[3].sel, 4, "the fifth row: bare hands");
        // the client's bar 3 (menu 4) has a list's rows, past the items' and
        // the sets' place in the array: its first row starts above the
        // stack's first slot
        h.sel = 4;
        assert_eq!(h.point([(at[0] + 180) as f32, (at[1] + 1) as f32]), Some(Key::Use), "bar 3's first row");
        h.sel = 3;
        assert_eq!(h.point([(at[0] + 180) as f32, (at[1] + 1) as f32]), None, "the sets' first slot starts lower (control)");
    }

    /// The carousel (`VIWndGame::Interact` right, `Attack` left): without
    /// the sets the disc's switches as CLIENT has them (right: 0 to 1, or 2
    /// without the second bar, 1 to 2, 2 to 0; left: 0 to 2, 1 to 0, 2 to
    /// 1, or 0 without), with them the live client's (right 0, 1 or 2, 2, 3,
    /// 0; left 0, 3, 2, 1 or 0), and with three or four bars the client's
    /// between bar 2 and the items, each only with as many bars.
    #[test]
    fn the_carousel_is_the_discs_the_live_clients_and_goes_through_more_bars() {
        let go = |sel: i32, key: Key, bars: Bars| {
            let mut h = HudMenus { sel, ..Default::default() };
            h.key(key, 0, bars, true, &mut Vec::new());
            h.sel
        };
        for second in [false, true] {
            let bars = Bars { abilities: 1 + second as i32, gear: false };
            assert_eq!(go(0, Key::Right, bars), if second { 1 } else { 2 });
            assert_eq!(go(1, Key::Right, bars), 2);
            assert_eq!(go(2, Key::Right, bars), 0);
            assert_eq!(go(0, Key::Left, bars), 2);
            assert_eq!(go(1, Key::Left, bars), 0);
            assert_eq!(go(2, Key::Left, bars), second as i32);
            let bars = Bars { abilities: 1 + second as i32, gear: true };
            assert_eq!(go(0, Key::Right, bars), if second { 1 } else { 2 }, "live Interact: 0");
            assert_eq!(go(1, Key::Right, bars), 2, "live Interact: 1");
            assert_eq!(go(2, Key::Right, bars), 3, "live Interact: 2, on to the sets");
            assert_eq!(go(3, Key::Right, bars), 0, "live Interact: 3");
            assert_eq!(go(0, Key::Left, bars), 3, "live Attack: 0, back to the sets");
            assert_eq!(go(3, Key::Left, bars), 2, "live Attack: 3");
            assert_eq!(go(2, Key::Left, bars), second as i32, "live Attack: 2");
            assert_eq!(go(1, Key::Left, bars), 0, "live Attack: 1");
        }
        let walk = |bars: Bars, key: Key| {
            let mut h = HudMenus::default();
            (0..6)
                .map(|_| {
                    h.key(key, 0, bars, true, &mut Vec::new());
                    h.sel
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(walk(DISC, Key::Right), [1, 2, 0, 1, 2, 0], "the disc's two bars");
        assert_eq!(walk(LIVE, Key::Right), [1, 2, 3, 0, 1, 2]);
        assert_eq!(walk(LIVE, Key::Left), [3, 2, 1, 0, 3, 2]);
        let bars = |abilities, gear| Bars { abilities, gear };
        assert_eq!(walk(bars(3, false), Key::Right), [1, 4, 2, 0, 1, 4], "bar 3 after bar 2");
        assert_eq!(walk(bars(4, false), Key::Right), [1, 4, 5, 2, 0, 1], "bars 3 and 4, then the items");
        assert_eq!(walk(bars(4, false), Key::Left), [2, 5, 4, 1, 0, 2]);
        assert_eq!(walk(bars(4, true), Key::Right), [1, 4, 5, 2, 3, 0], "the sets after the items still");
        assert_eq!(walk(bars(4, true), Key::Left), [3, 2, 5, 4, 1, 0]);
        assert_eq!(walk(bars(3, true), Key::Right), [1, 4, 2, 3, 0, 1]);
    }

    /// The manual camera (mode 38) gives the menu the keys as play does, but
    /// `VIWndGame::Interact` and `Attack` are play's: left and right light
    /// the menu up and switch nothing. The control: with the switch, they go
    /// round.
    #[test]
    fn left_and_right_only_light_the_menu_without_the_switch() {
        for key in [Key::Left, Key::Right] {
            let mut h = HudMenus { sel: 1, ..Default::default() };
            h.menus[1].lit = 0;
            h.key(key, 5, DISC, false, &mut Vec::new());
            assert_eq!((h.sel, h.menus[1].lit, h.menus[1].time), (1, 1, 20005), "{key:?}: lit, not switched");
            let mut h = HudMenus { sel: 1, ..Default::default() };
            h.key(key, 5, DISC, true, &mut Vec::new());
            assert_ne!(h.sel, 1, "{key:?}: the control");
        }
    }

    fn net(id: i64, wear: i64, p4c: i64, quick: i64) -> NetItem {
        NetItem { equip_slot: -1, quick_slot: quick, item: Item { id, wear_slot: wear, p4c, attack_type: 1, ..Default::default() }, ..Default::default() }
    }

    /// The bars' menus from the abilities' slots: bar 1 its 0-4 and the
    /// 252-255 ones after them, bar 2 its 5-9, and the client's bars 3 and 4
    /// their 10-14 and 15-19, each entry at its slot's place, using sending
    /// the ability; the game's menus take nothing from 10-19, as CLIENT's do
    /// not (the control). A bar up when Ability Bars is turned down goes
    /// back to the first; the sets' menu keeps its own place beside them.
    #[test]
    fn each_bar_holds_its_own_five_slots() {
        let list: BTreeMap<i64, Ability> = [(1, 0), (2, 4), (3, 7), (4, 10), (5, 14), (6, 16), (7, 252), (8, 19)]
            .into_iter()
            .map(|(id, slot)| (id, Ability { id, slot, valid: 1, icon_front: 100 + id, ..Default::default() }))
            .collect();
        let none = ItemList::default();
        let four = Bars { abilities: 4, gear: true };
        let three = Bars { abilities: 3, gear: true };
        let mut h = HudMenus::default();
        h.build(&list, &none, &Gear::default(), four);
        let ids = |h: &HudMenus, k: usize| h.defs[k].entries.iter().map(|e| if e.icon2 == -1 { 0 } else { e.id }).collect::<Vec<_>>();
        assert_eq!(ids(&h, 0), [1, 0, 0, 0, 2, 7, 0, 0, 0], "slots 0-4, then 252-255");
        assert_eq!(ids(&h, 1), [0, 0, 3, 0, 0]);
        assert_eq!(ids(&h, 4), [4, 0, 0, 0, 5], "bar 3: slots 10-14");
        assert_eq!(ids(&h, 5), [0, 6, 0, 0, 8], "bar 4: slots 15-19");
        assert_eq!(h.defs[4].count, 5);
        assert_eq!((h.defs[3].entries[4].msg, h.defs[3].entries[4].id), (MSG, BARE_HANDS), "the sets' menu where it was");
        h.sel = 1;
        h.menus[1].sel = 2;
        assert_eq!(h.key(Key::Use, 0, four, true, &mut Vec::new()), Some((0x8060000, 3)), "slot 7's ability used");
        h.sel = 5;
        h.menus[5].sel = 4;
        assert_eq!(h.key(Key::Use, 0, four, true, &mut Vec::new()), Some((0x8060000, 8)), "slot 19's ability used");
        h.build(&list, &none, &Gear::default(), three);
        assert_eq!(h.sel, 0, "bar 4 gone: the first bar up");
        h.sel = 4;
        h.build(&list, &none, &Gear::default(), three);
        assert_eq!(h.sel, 4, "bar 3 stays with three");
        h.sel = 1;
        h.build(&list, &none, &Gear::default(), Bars::default());
        assert_eq!(h.sel, 0, "bar 2 gone: the first bar up");
    }

    /// The items' stack: every item on a quick slot with the disc's layout,
    /// only potions and gems with the live client's (its SetQuickMenuItem);
    /// its slot icons a weapon's on 3 or a gem's.
    #[test]
    fn the_live_items_menu_shows_potions_and_gems() {
        let inv = ItemList { items: vec![net(1, -1, 2, 0), net(2, 14, 0, 3), net(3, -1, 7, 2)], ..Default::default() };
        let mut h = HudMenus::default();
        let used = |h: &HudMenus| h.defs[2].entries.iter().map(|e| e.msg != 0).collect::<Vec<_>>();
        h.build(&BTreeMap::new(), &inv, &Gear::default(), DISC);
        assert_eq!(used(&h), [true, false, true, true], "the disc's: the weapon on 3 too");
        assert!(!h.live);
        h.build(&BTreeMap::new(), &inv, &Gear::default(), LIVE);
        assert_eq!(used(&h), [true, false, true, false], "the live client's: not the weapon");
        assert!(h.live);
        assert_eq!((SLOT_ICONS[3], SLOT_ICONS_LIVE[3]), (2, 3));
    }

    /// The sets' menu (`BuildGearMenu`): a set that is not empty at its
    /// place with its icon and badge, bare hands last always, each used as
    /// UseGearSet's message with its number; an empty set's entry is empty.
    #[test]
    fn the_sets_menu_holds_the_sets_and_bare_hands() {
        let inv = ItemList { items: vec![net(10, 12, 0, -1), net(11, 13, 0, -1)], ..Default::default() };
        let mut g = Gear::default();
        g.set(1, "Tank".into(), 10, 11);
        let mut h = HudMenus::default();
        h.build(&BTreeMap::new(), &inv, &g, LIVE);
        let m = &h.defs[3];
        assert_eq!(m.count, 5);
        assert_eq!(m.entries[0].msg, 0, "set 0 is empty");
        assert_eq!((m.entries[1].msg, m.entries[1].id), (MSG, 1));
        assert_eq!(m.entries[1].icon2, vi::ui::icon_id("sword") as i32);
        assert_eq!(m.entries[1].x08, vi::ui::icon_id("defence_active") as i32, "a shield in the off hand");
        assert_eq!((m.entries[4].msg, m.entries[4].id, m.entries[4].icon2), (MSG, BARE_HANDS, vi::ui::icon_id("Fist") as i32));
        h.sel = 3;
        h.menus[3].sel = 1;
        assert_eq!(h.key(Key::Use, 0, LIVE, true, &mut Vec::new()), Some((MSG, 1)), "set 1 taken up");
        h.build(&BTreeMap::new(), &inv, &g, DISC);
        assert_eq!(h.sel, 0, "no sets on this server: the first bar up");
    }
}
