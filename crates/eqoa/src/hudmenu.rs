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
//! Stacked Spell Bars (Extended > UI > HUD, a player's ticket 1913, "move your
//! spellbars vertically, and nest them x2 evenly above the chat box stacked on
//! top of each other. Scale them down about 5% and remove the pop up horizontal
//! box on the current bar that tells you what the spell is. This should be for
//! PC players"): the client's own, on a computer (the keyboard and the mouse;
//! a controller's or a touch screen's play keeps the carousel). The first bar
//! (menu 0, its packed 252-255 too) and the second (menu 1) are drawn as two
//! rows of their slots' frames, the first over the second, just above the
//! chat box's full height, a piece of their own to move (`draw_stacked`,
//! `Piece::SpellBars`; with the chat box's, `Piece::Chat`, until petition 5229),
//! at 95% (`STACKED_SCALE`), with no title bar: no name pops up. A click on a
//! slot uses it, a right click turns its auto (`stacked_at`), and each slot
//! of both can be given a key on the Controls page (`binds::bar_slot`,
//! `slot_entry`). The arrows and E still walk and use the bar up, its
//! selection outlined: stacked, the bars lie across, so left and right walk
//! along the bar up and up and down go to the bar above and below, round the
//! carousel's order (`HudMenus::key`; a player's petition 2457: "since the
//! bars are now horizontal, it would make much more sense that left/right on
//! the dpad would cycle through the same bar's 5 icons and up/down would go
//! to the next"), the d-pad's and the arrows' alike. Played with a controller
//! (`HudMenus::named`: the last input no key, mouse or finger), the name of
//! what the selection holds shows over the bars while the bar is lit, as the
//! carousel's title bar shows it (`draw_name`; the same petition: "can we
//! still see the name of the spell/item the cursor is over when using the
//! dpad?"); on the keyboard and the mouse, none, as ticket 1913 asked.
//!
//! With Ability Bars at 3 or 4 (the server owner, 2026-11-18: "When players
//! turn on 4 they need to be able to stack 4 spell bars, or 2 columns of 2,
//! side by side"), bars 3 and 4 (menus 4 and 5) are stacked too, as Spell Bar
//! Layout has them (`Layout`, `stacked_grid`): Stacked, one column, bar 1 on
//! top and the last bar on the chat box; 2 Columns, bars 1 and 2 at the left
//! and bars 3 and 4 beside them, bar 3 level with bar 1. Their slots take
//! keys as the first two bars' do (`BAR_MENUS`), and the whole block is one
//! piece to move (`Piece::SpellBars`): every cell's frame is drawn, at Spell
//! Bar Opacity 0 unseen, so the piece is the whole block wherever its
//! abilities are.
//!
//! The Utility Bars (the server owner, 2026-11-18: "the player also needs the
//! utility bars in this fashion. They can go in the upper left, side by side
//! vertical, or down below and the player decides. They need to be able to
//! free move these in freemove mode like the others but otherwise work the
//! same way"): while the spell bars are stacked the carousel is not drawn, and
//! with it went the item quick slots (menu 2) and the weapon sets (menu 3).
//! They are drawn as bars of the stacked bars' cells (`draw_utility`), as
//! Utility Bar Layout has them (`UtilityLayout`): Upper Left, a column a bar
//! side by side; Below, a row a bar on the chat box, the spell bars standing
//! on them (`utility_height`). A piece of their own to move
//! (`Piece::UtilityBars`); a click on a cell uses it (`point_utility`), and
//! each slot can be given a key on the Controls page (`ITEMS_BAR`,
//! `SETS_BAR`, `key_menu`), stacked or not.

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
/// Each ability bar's menu, by the bar (from 0): the game's two, then the
/// client's bars 3 and 4.
pub const BAR_MENUS: [usize; MAX_BARS as usize] = [0, 1, 4, 5];

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

/// Fit a shortcut, including its two-pixel shadow, inside a cell. Long
/// names are abbreviated here; the hover label retains the complete name.
fn shortcut_label(r: &dyn Raster, cell: [i32; 4], label: &str) -> Option<([i32; 2], Vec<u16>)> {
    let width = cell[2] - cell[0] - 5;
    if width <= 0 || r.point_size() <= 0 || r.point_size() + 3 > cell[3] - cell[1] { return None; }
    let mut chars: Vec<char> = label.chars().collect();
    let mut wide = crate::ui::wide(label);
    if r.string_width(&wide) > width {
        loop {
            if chars.pop().is_none() { return None; }
            let abbreviated = format!("{}..", chars.iter().collect::<String>());
            wide = crate::ui::wide(&abbreviated);
            if r.string_width(&wide) <= width { break; }
        }
    }
    let w = r.string_width(&wide);
    if w < 0 || w > width { return None; }
    Some(([cell[2] - 3 - w, cell[1] + 1], wide))
}

/// How far the stacked bars stand above the chat box's window top, frame pixels:
/// its panel is drawn 10 inside it, so they stand 2 above the panel.
const STACKED_GAP: i32 = -8;

/// The space between the stacked bars' two columns (Spell Bar Layout's 2
/// Columns), frame pixels before `STACKED_SCALE`.
const COLUMN_GAP: i32 = 6;

/// The keys' bars (`binds::bar_slot`) past the ability bars: the item quick
/// slots' (`items_slot1`..4) and the weapon sets' (`sets_slot1`..5).
pub const ITEMS_BAR: usize = 4;
pub const SETS_BAR: usize = 5;
/// The two PC-only rows for server-confirmed equipped slots 20–39.
pub const PC_ACTION_BAR: usize = 6;

/// The menu a key's bar reads (`binds::bar_slot`): the ability bars'
/// (`BAR_MENUS`), then the items' and the weapon sets'.
pub fn key_menu(bar: usize) -> Option<usize> {
    match bar {
        b if b < BAR_MENUS.len() => Some(BAR_MENUS[b]),
        ITEMS_BAR => Some(ITEMS),
        SETS_BAR => Some(GEAR),
        _ => None,
    }
}

/// Utility Bar Layout (Extended > HUD Layout; the server owner, 2026-11-18:
/// "the player also needs the utility bars in this fashion. They can go in
/// the upper left, side by side vertical, or down below and the player
/// decides"): where the Utility Bars (the item quick slots, and the weapon
/// sets on a server that keeps them) stand while the spell bars are stacked
/// (`HudMenus::draw_utility`), each a piece of its own to move.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UtilityLayout {
    /// a column a bar, side by side, at the screen's upper left
    /// (`UPPER_LEFT_AT`, clear of the character's frame, the effects and
    /// the group's bars)
    #[default]
    UpperLeft,
    /// a row a bar, the items over the sets, standing on the chat box with
    /// the spell bars standing on them
    Below,
}

impl UtilityLayout {
    /// The Extended row's word.
    pub fn name(self) -> &'static str {
        match self {
            UtilityLayout::UpperLeft => "Upper Left",
            UtilityLayout::Below => "Below",
        }
    }

    /// The other layout (the row turned either way).
    pub fn other(self) -> UtilityLayout {
        match self {
            UtilityLayout::UpperLeft => UtilityLayout::Below,
            UtilityLayout::Below => UtilityLayout::UpperLeft,
        }
    }

    /// As `client.opt` keeps it (`settings`).
    pub fn opt(self) -> &'static str {
        match self {
            UtilityLayout::UpperLeft => "upper_left",
            UtilityLayout::Below => "below",
        }
    }

    /// From `client.opt`'s word.
    pub fn from_opt(s: &str) -> Option<UtilityLayout> {
        match s {
            "upper_left" => Some(UtilityLayout::UpperLeft),
            "below" => Some(UtilityLayout::Below),
            _ => None,
        }
    }
}

/// Upper Left's top left, frame pixels (anchored at the screen's left and
/// top): right of the group's bars (from x 20, about 180 wide) and under the
/// effects' strip and the buff window (from (143, 15)).
pub const UPPER_LEFT_AT: [i32; 2] = [206, 120];

/// Spell Bar Layout (Extended > HUD Layout; the server owner, 2026-11-18):
/// how the stacked bars stand once Ability Bars shows more than two
/// (`stacked_grid`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    /// one column: bar 1 on top, the last bar on the chat box
    #[default]
    Stacked,
    /// columns of two side by side: bars 1 and 2 at the left, bars 3 and 4
    /// at their right, bar 3 level with bar 1
    Columns,
}

impl Layout {
    /// The Extended row's word.
    pub fn name(self) -> &'static str {
        match self {
            Layout::Stacked => "Stacked",
            Layout::Columns => "2 Columns",
        }
    }

    /// The other layout (the row turned either way).
    pub fn other(self) -> Layout {
        match self {
            Layout::Stacked => Layout::Columns,
            Layout::Columns => Layout::Stacked,
        }
    }

    /// As `client.opt` keeps it (`settings`).
    pub fn opt(self) -> &'static str {
        match self {
            Layout::Stacked => "stacked",
            Layout::Columns => "columns",
        }
    }

    /// From `client.opt`'s word.
    pub fn from_opt(s: &str) -> Option<Layout> {
        match s {
            "stacked" => Some(Layout::Stacked),
            "columns" => Some(Layout::Columns),
            _ => None,
        }
    }
}

/// Where each bar shown stands in the stacked block: (its menu, its column,
/// its row in the column from the top), and how many rows a column holds.
/// Stacked: one column of every bar; 2 Columns: columns of two, bar n in
/// column n / 2 (one bar or two stay one column).
fn stacked_grid(bars: Bars, layout: Layout) -> (Vec<(usize, usize, usize)>, usize) {
    let n = bars.abilities.clamp(1, MAX_BARS) as usize;
    let per = match layout {
        Layout::Stacked => n,
        Layout::Columns => n.min(2),
    };
    (BAR_MENUS[..n].iter().enumerate().map(|(b, &menu)| (menu, b / per, b % per)).collect(), per)
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
    /// Server-confirmed equipped abilities for PC rows three and four.
    pub pc_defs: [MenuDef; 2],
    live: bool,
    pub auto: Vec<i64>,
    /// Stacked Spell Bars are drawn (the module's doc): the pointer reads
    /// the stacked rows' cells (`stacked`), not the carousel's.
    pub stacked: bool,
    /// The cells the stacked bars last drew: (menu, entry, rectangle x0, y0,
    /// x1, y1 in the chat box's frame points).
    stacked_hits: Vec<(usize, i32, [i32; 4])>,
    /// The cells the Utility Bars last drew, as `stacked_hits` (in their
    /// own place, `draw_utility`).
    utility_hits: Vec<(usize, i32, [i32; 4])>,
    equipment_hits: Vec<(usize, i32, [i32; 4])>,
    equipment_hover: Option<i32>,
    /// The stacked bars are played with a controller (the last input no key,
    /// mouse or finger: play's): the selection's name is drawn over them
    /// (`draw_name`).
    pub named: bool,
    /// Last mouse position resolved against the spell cells.
    hover: Option<(usize, i32)>,
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
            pc_defs: std::array::from_fn(|_| MenuDef { count: 10, entries: vec![Entry::default(); 10] }),
            live: false,
            auto: Vec::new(),
            stacked: false,
            stacked_hits: Vec::new(),
            utility_hits: Vec::new(),
            equipment_hits: Vec::new(),
            equipment_hover: None,
            named: false,
            hover: None,
        }
    }
}

impl HudMenus {
    /// Reflect only confirmed, currently equipped server slots 20–39.
    pub fn sync_pc_slots(&mut self, abilities: &BTreeMap<i64, Ability>) {
        for def in &mut self.pc_defs { def.entries.fill(Entry::default()); }
        for a in abilities.values().filter(|a| a.valid != 0 && (20..40).contains(&a.slot)) {
            let i = (a.slot - 20) as usize;
            self.pc_defs[i / 10].entries[i % 10] = Entry {
                icon2: a.icon_front as i32,
                x08: if a.lockout > 0 { 61 } else { -1 },
                msg: 0x8060000, id: a.id,
            };
        }
    }
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

    /// Stacked Spell Bars (the module's doc): every bar shown (the second
    /// with the expansion's second bar, the third and fourth with Ability
    /// Bars) as a row of its slots, laid out as `layout` says
    /// (`stacked_grid`), each a slot's frame scaled `STACKED_SCALE` with its
    /// icon, lock badge and auto mark, the lowest rows standing on `chat_top`
    /// (the chat box's top at its full height, frame points, as it is drawn)
    /// from its left at `left`, a second column as far right of the first as
    /// its widest bar and `COLUMN_GAP`; the menu up's selection outlined; no
    /// title bar. The cells are noted for the pointer (`stacked_at`).
    pub fn draw_stacked(&mut self, vui: &Vui, r: &mut dyn Raster, chat_top: i32, left: i32, bars: Bars, layout: Layout) {
        self.draw_stacked_inner(vui, r, chat_top, left, bars, layout, None);
    }

    /// Computer profile: pair the real five-slot menus into ten-wide rows.
    pub fn draw_pc_bars(&mut self, vui: &Vui, r: &mut dyn Raster, bottom: i32, left: i32, bars: Bars, rows: usize) {
        self.draw_stacked_inner(vui, r, bottom, left, bars, Layout::Stacked, Some(rows.clamp(1, 4)));
    }

    fn draw_stacked_inner(&mut self, vui: &Vui, r: &mut dyn Raster, chat_top: i32, left: i32, bars: Bars, layout: Layout, pc_rows: Option<usize>) {
        let pc = pc_rows.is_some();
        self.stacked_hits.clear();
        let frame = vui.entry(29);
        let s = |v: i32| (v as f32 * STACKED_SCALE).round() as i32;
        let (cw, ch) = (s(CELL[0]), s(CELL[1]));
        let (grid, per) = if let Some(rows) = pc_rows {
            let n = (rows * 2).min(BAR_MENUS.len());
            let mut grid: Vec<_> = BAR_MENUS[..n].iter().enumerate().map(|(i, &m)| (m, i % 2, i / 2)).collect();
            for row in 2..rows { grid.push((MENUS + row - 2, 0, row)); }
            (grid, rows)
        } else { stacked_grid(bars, layout) };
        // each column from the one before it: as wide as its widest bar (the
        // first bar's packed 252-255 make it up to nine), then the gap
        let width = |menu: usize| (self.defs[menu].count.max(0) as f32 * CELL[0] as f32 * STACKED_SCALE).round() as i32;
        let columns = grid.iter().map(|g| g.1).max().unwrap_or(0) + 1;
        let column_x: Vec<i32> = (0..columns)
            .scan(left, |x, c| {
                let at = *x;
                *x += if pc { cw * 5 } else { grid.iter().filter(|g| g.1 == c).map(|g| width(g.0)).max().unwrap_or(0) + s(COLUMN_GAP) };
                Some(at)
            })
            .collect();
        let up = self.up();
        // Spell Bar Opacity (ticket 1953): the cells' frames, not the icons;
        // drawn at 0 too, unseen, so the piece moved is the whole block
        let opacity = frame_alpha(crate::settings::client().spell_bar_opacity);
        for &(menu, column, row) in &grid {
            let y = chat_top - STACKED_GAP - ch * if pc { (row + 1) as i32 } else { (per - row) as i32 };
            let def = if menu < MENUS { &self.defs[menu] } else { &self.pc_defs[menu - MENUS] };
            // In PC mode the selected row count controls slots beyond 9;
            // the legacy five-slot Ability Bars setting is independent. A
            // non-expansion server still supports only the first five.
            let unavailable = pc && bars.abilities < 2 && menu != 0;
            for n in 0..def.count.max(0) {
                // Packed special slots 252–255 remain reachable in their own
                // short row above the normal ten-wide block.
                let (x, y) = if pc && menu == 0 && n >= 5 {
                    (left + (n - 5) * cw, chat_top - STACKED_GAP - ch * (per as i32 + 1))
                } else {
                    (column_x[column] + if pc { n * cw } else { (n as f32 * CELL[0] as f32 * STACKED_SCALE).round() as i32 }, y)
                };
                let at = [x, y];
                r.stretch_blit(vui.entry_id(&frame), 0, at, [cw, ch], frame.uv0, frame.uv1, 3, if unavailable { opacity * 0.35 } else { opacity });
                let e = if unavailable { Entry::default() } else { def.entries.get(n as usize).copied().unwrap_or_default() };
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
                if menu < MENUS && menu == up && n == self.menus[menu].sel {
                    outline(r, [icon[0] - 2, icon[1] - 2, icon[0] + s(22) + 1, icon[1] + s(22) + 1], [1.0, 0.85, 0.3, 1.0]);
                }
                if pc && self.hover == Some((menu, n)) {
                    outline(r, [x, y, x + cw - 1, y + ch - 1], [1.0, 1.0, 1.0, 1.0]);
                }
                if pc && (n < 5 || menu >= MENUS) {
                    let row = if menu >= MENUS { Some(format!("pcbar{}_slot{}", menu - MENUS + 3, n + 1)) }
                        else { BAR_MENUS.iter().position(|&m| m == menu).map(|bar| format!("bar{}_slot{}", bar + 1, n + 1)) };
                    if let Some(row) = row {
                        let label = crate::binds::row_named(&row).and_then(|i| crate::settings::with_binds(|b| b.profile_key(i, true).filter(|k| !k.is_empty()).map(crate::binds::display)));
                        if let Some((at, label)) = label.and_then(|label| shortcut_label(r, [x, y, x + cw, y + ch], &label)) {
                            vui.draw_string_ex(r, &label, at[0], at[1], [1.0, 1.0, 1.0, 1.0], true);
                        }
                    }
                }
                self.stacked_hits.push((menu, n, [x, y, x + cw, y + ch]));
            }
        }
    }

    /// The name of what the selection of the menu up holds (`name`: an
    /// ability's, an item's, a set's), over the stacked bars (`utility`: the
    /// Utility Bars) where the menu up is one of theirs, as they were last
    /// drawn: centred over the selected cell, a line above the block's top,
    /// with a shadow; only while the bars are played with a controller
    /// (`named`) and the menu is lit by a key (20 s, as the carousel's title
    /// bar shows it bright). Draw it in the bars' own place (their piece),
    /// not counted in it (`hudlayout::seal`), and kept within `screen` (its
    /// left and right in the piece's frame pixels, `hudlayout::screen_x`):
    /// against the screen's edge the name moves, the bars do not (petition
    /// 2547).
    pub fn draw_name(&self, vui: &Vui, r: &mut dyn Raster, now: i64, utility: bool, screen: Option<[i32; 2]>, name: &dyn Fn(u32, i64) -> Vec<u16>) {
        let up = self.up();
        let m = &self.menus[up];
        if !self.named || m.lit <= 0 || m.time < now {
            return;
        }
        let hits = if utility { &self.utility_hits } else { &self.stacked_hits };
        let Some(&(_, _, c)) = hits.iter().find(|&&(menu, n, _)| menu == up && n == m.sel) else { return };
        let e = self.defs[up].entries.get(m.sel.max(0) as usize).copied().unwrap_or_default();
        if e.msg == 0 || e.id < 0 {
            return;
        }
        let s = name(e.msg, e.id);
        if s.is_empty() {
            return;
        }
        let top = hits.iter().map(|h| h.2[1]).min().unwrap_or(c[1]);
        let left = hits.iter().map(|h| h.2[0]).min().unwrap_or(c[0]);
        let half = r.string_width(&s) / 2;
        let mut x = ((c[0] + c[2]) / 2).max(left + half);
        if let Some([lo, hi]) = screen {
            x = x.min(hi - half).max(lo + half);
        }
        let y = top - r.point_size() / 2 - 4;
        vui.draw_centered_string_ex(r, &s, x, y, [1.0, 1.0, 1.0, 1.0], 0, true);
    }

    /// The stacked bars' cell at frame point `p` (in the chat box's place, as
    /// `draw_stacked` drew it): its menu and entry.
    pub fn stacked_at(&self, p: [f32; 2]) -> Option<(usize, i32)> {
        let (x, y) = (p[0] as i32, p[1] as i32);
        self.stacked_hits.iter().find(|(_, _, c)| (c[0]..c[2]).contains(&x) && (c[1]..c[3]).contains(&y)).map(|&(m, n, _)| (m, n))
    }

    pub fn pc_entry(&self, menu: usize, n: i32) -> Option<Entry> {
        let def = if menu < MENUS { self.defs.get(menu)? } else { self.pc_defs.get(menu - MENUS)? };
        def.entries.get(usize::try_from(n).ok()?).copied()
    }

    pub fn hover_stacked(&mut self, p: Option<[f32; 2]>) {
        self.hover = p.and_then(|p| self.stacked_at(p));
    }

    /// Show the hovered ability's actual name, using the same lookup as the
    /// controller's selected-slot label.
    pub fn draw_hover_name(&self, vui: &Vui, r: &mut dyn Raster, screen: Option<[i32; 2]>, name: &dyn Fn(u32, i64) -> Vec<u16>) {
        let Some((menu, n)) = self.hover else { return };
        let Some(&(_, _, c)) = self.stacked_hits.iter().find(|&&(m, i, _)| m == menu && i == n) else { return };
        let def = if menu < MENUS { &self.defs[menu] } else { &self.pc_defs[menu - MENUS] };
        let Some(e) = def.entries.get(n as usize) else { return };
        let mut text = if e.msg == 0 || e.id < 0 { Vec::new() } else { name(e.msg, e.id) };
        let row = if menu >= MENUS { Some(format!("pcbar{}_slot{}", menu - MENUS + 3, n + 1)) }
            else { BAR_MENUS.iter().position(|&m| m == menu).filter(|_| n < 5).map(|bar| format!("bar{}_slot{}", bar + 1, n + 1)) };
        if let Some(row) = row {
            let key = crate::binds::row_named(&row).and_then(|i| crate::settings::with_binds(|b| b.profile_key(i, true).filter(|k| !k.is_empty()).map(crate::binds::display)));
            if let Some(key) = key {
                text = if text.is_empty() { crate::ui::wide(&key) } else { crate::ui::wide(&format!("{} ({key})", String::from_utf16_lossy(&text))) };
            }
        }
        if menu >= MENUS && text.is_empty() { text = crate::ui::wide("Empty spell slot"); }
        if text.is_empty() { return; }
        let half = r.string_width(&text) / 2;
        let mut x = (c[0] + c[2]) / 2;
        if let Some([lo, hi]) = screen { x = x.min(hi - half).max(lo + half); }
        let y = c[1] - r.point_size() / 2 - 4;
        vui.draw_centered_string_ex(r, &text, x, y, [1.0, 1.0, 1.0, 1.0], 0, true);
    }

    /// The Utility Bars' cells (the module's doc, `UtilityLayout`): the item
    /// quick slots' bar, and the weapon sets' with them (`bars.gear`), each
    /// cell a stacked bar's size. Upper Left: a column a bar from `at` (their
    /// top left), side by side `COLUMN_GAP` apart; Below: a row a bar, the
    /// items over the sets, the lower standing on `at` (their bottom left, the
    /// chat box's top as the stacked bars stand on it). (menu, entry,
    /// rectangle), in frame points.
    fn utility_cells(&self, bars: Bars, layout: UtilityLayout, at: [i32; 2]) -> Vec<(usize, i32, [i32; 4])> {
        let s = |v: i32| (v as f32 * STACKED_SCALE).round() as i32;
        let (cw, ch) = (s(CELL[0]), s(CELL[1]));
        let menus: Vec<usize> = if bars.gear { vec![ITEMS, GEAR] } else { vec![ITEMS] };
        let mut cells = Vec::new();
        for (k, &menu) in menus.iter().enumerate() {
            for n in 0..self.defs[menu].count.max(0) {
                let (x, y) = match layout {
                    UtilityLayout::UpperLeft => (at[0] + k as i32 * (cw + s(COLUMN_GAP)), at[1] + n * ch),
                    UtilityLayout::Below => (at[0] + (n as f32 * CELL[0] as f32 * STACKED_SCALE).round() as i32, at[1] - STACKED_GAP - ch * (menus.len() - k) as i32),
                };
                cells.push((menu, n, [x, y, x + cw, y + ch]));
            }
        }
        cells
    }

    /// How far the Utility Bars below lift the stacked bars standing on them
    /// (Below's rows and a gap; nothing for Upper Left), frame pixels.
    pub fn utility_height(bars: Bars, layout: UtilityLayout) -> i32 {
        let s = |v: i32| (v as f32 * STACKED_SCALE).round() as i32;
        match layout {
            UtilityLayout::UpperLeft => 0,
            UtilityLayout::Below => s(CELL[1]) * (1 + bars.gear as i32) + s(COLUMN_GAP),
        }
    }

    /// The Utility Bars (`utility_cells`): each cell a slot's frame at Spell
    /// Bar Opacity, an item slot's icon the game's for that slot (the
    /// carousel's stack draws the same, `SLOT_ICONS`; an item's own picture
    /// is the inventory's banner, not a square), whole while it holds an item
    /// and faint while empty; a set's badge and icon as the live client's
    /// `DrawGear` has them; the menu up's selection outlined; no name. The
    /// cells are noted for the pointer (`utility_at`).
    pub fn draw_utility(&mut self, vui: &Vui, r: &mut dyn Raster, bars: Bars, layout: UtilityLayout, at: [i32; 2]) {
        self.draw_utility_subset(vui, r, bars, layout, at, false);
    }

    pub fn draw_equipment(&mut self, vui: &Vui, r: &mut dyn Raster, bars: Bars, at: [i32; 2]) {
        self.draw_utility_subset(vui, r, bars, UtilityLayout::Below, at, true);
    }

    fn draw_utility_subset(&mut self, vui: &Vui, r: &mut dyn Raster, bars: Bars, layout: UtilityLayout, at: [i32; 2], equipment: bool) {
        let cells: Vec<_> = self.utility_cells(bars, layout, at).into_iter().filter(|c| !equipment || c.0 == GEAR).collect();
        let frame = vui.entry(29);
        let s = |v: i32| (v as f32 * STACKED_SCALE).round() as i32;
        let opacity = frame_alpha(crate::settings::client().spell_bar_opacity);
        let icons = if self.live { SLOT_ICONS_LIVE } else { SLOT_ICONS };
        let up = self.up();
        for &(menu, n, c) in &cells {
            r.stretch_blit(vui.entry_id(&frame), 0, [c[0], c[1]], [c[2] - c[0], c[3] - c[1]], frame.uv0, frame.uv1, 3, opacity);
            let e = self.defs[menu].entries.get(n as usize).copied().unwrap_or_default();
            let icon = [c[0] + s(7), c[1] + s(2)];
            if menu == ITEMS {
                let id = icons[(n.max(0) as usize).min(3)];
                let size = vui.entry(id).uv1;
                let size = [s(size[0]), s(size[1])];
                let centre = [c[0] + (c[2] - c[0] - size[0]) / 2, c[1] + (c[3] - c[1] - size[1]) / 2];
                vui.blit_ex_size(r, id, centre, size, 3, if e.msg != 0 { 1.0 } else { 0.35 }, 0);
            } else {
                for id in [e.x08, e.icon2] {
                    if id != -1 {
                        r.stretch_blit(id as u32, 0, icon, [s(22), s(22)], [0, 0], [22, 22], 3, 1.0);
                    }
                }
            }
            if menu == up && n == self.menus[menu].sel {
                outline(r, [icon[0] - 2, icon[1] - 2, icon[0] + s(22) + 1, icon[1] + s(22) + 1], [1.0, 0.85, 0.3, 1.0]);
            }
        }
        if equipment { self.equipment_hits = cells; } else { self.utility_hits = cells; }
    }

    pub fn point_equipment(&mut self, p: [f32; 2]) -> Option<Key> {
        let (x, y) = (p[0] as i32, p[1] as i32);
        let &(menu, n, _) = self.equipment_hits.iter().find(|(_, _, c)| (c[0]..c[2]).contains(&x) && (c[1]..c[3]).contains(&y))?;
        self.sel = menu as i32;
        self.menus[menu].sel = n;
        Some(Key::Use)
    }

    pub fn hover_equipment(&mut self, p: Option<[f32; 2]>) {
        self.equipment_hover = p.and_then(|p| {
            let (x, y) = (p[0] as i32, p[1] as i32);
            self.equipment_hits.iter().find(|(_, _, c)| (c[0]..c[2]).contains(&x) && (c[1]..c[3]).contains(&y)).map(|(_, n, _)| *n)
        });
    }

    pub fn hide_equipment(&mut self) {
        self.equipment_hits.clear();
        self.equipment_hover = None;
    }

    /// Names the independent equipment cell under the pointer or pad focus.
    pub fn draw_equipment_name(&self, vui: &Vui, r: &mut dyn Raster, now: i64, screen: Option<[i32; 2]>, name: &dyn Fn(u32, i64) -> Vec<u16>) {
        let n = self.equipment_hover.or_else(|| {
            let m = &self.menus[GEAR];
            (self.named && self.up() == GEAR && m.lit > 0 && m.time >= now).then_some(m.sel)
        });
        let Some(n) = n else { return };
        let Some(&(_, _, c)) = self.equipment_hits.iter().find(|(_, i, _)| *i == n) else { return };
        let Some(e) = self.defs[GEAR].entries.get(n as usize) else { return };
        let text = name(e.msg, e.id);
        if text.is_empty() { return; }
        let half = r.string_width(&text) / 2;
        let mut x = (c[0] + c[2]) / 2;
        if let Some([lo, hi]) = screen { x = x.min(hi - half).max(lo + half); }
        let y = c[1] - r.point_size() / 2 - 4;
        vui.draw_centered_string_ex(r, &text, x, y, [1.0; 4], 0, true);
    }

    /// The Utility Bars' cell at frame point `p` (in their own place, as
    /// `draw_utility` drew them): its menu and entry.
    pub fn utility_at(&self, p: [f32; 2]) -> Option<(usize, i32)> {
        let (x, y) = (p[0] as i32, p[1] as i32);
        self.utility_hits.iter().find(|(_, _, c)| (c[0]..c[2]).contains(&x) && (c[1]..c[3]).contains(&y)).map(|&(m, n, _)| (m, n))
    }

    /// A pointer on the Utility Bars at `p` (their place): a cell makes its
    /// bar the one up and its entry the selection, and is used, as a stacked
    /// bar's cell is (`point`).
    pub fn point_utility(&mut self, p: [f32; 2]) -> Option<Key> {
        if !self.stacked {
            return None;
        }
        let (menu, n) = self.utility_at(p)?;
        self.sel = menu as i32;
        self.menus[menu].sel = n;
        Some(Key::Use)
    }

    /// The Utility Bars are not drawn (the stacked bars are not): their
    /// cells forgotten, so no pointer finds them.
    pub fn hide_utility(&mut self) {
        self.utility_hits.clear();
        self.equipment_hits.clear();
        self.equipment_hover = None;
    }

    /// The entry a key's bar's slot shows (`binds::bar_slot`, `key_menu`: the
    /// ability bars, bar 0 menu 0, bar 1 menu 1, bars 2 and 3 menus 4 and 5;
    /// then the item quick slots and the weapon sets), when it holds something
    /// to use.
    pub fn slot_entry(&self, bar: usize, slot: usize) -> Option<(u32, i64)> {
        let menu = key_menu(bar)?;
        let e = self.defs[menu].entries.get(slot).copied()?;
        // an item's entry has no icon of its own (the stack draws the slot's)
        (e.msg != 0 && e.id >= 0 && (e.icon2 != -1 || menu == ITEMS)).then_some((e.msg, e.id))
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
            if menu >= MENUS { return None; }
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
        let def = if menu < MENUS { &self.defs[menu] } else { &self.pc_defs[menu - MENUS] };
        let e = def.entries.get(n as usize).copied()?;
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
    /// only light the menu up. Stacked (`stacked`, the module's doc) the bars
    /// lie across: left and right walk the bar up as up and down walk the
    /// carousel's, and up and down go round the menus (up the one before, the
    /// bar above), or only light it, as left and right do the carousel's.
    pub fn key(&mut self, key: Key, now: i64, bars: Bars, switch: bool, cues: &mut Vec<crate::sound::Cue>) -> Option<(u32, i64)> {
        let key = if self.stacked { across(key) } else { key };
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

/// A key on the stacked bars, which lie across (`HudMenus::key`; petition
/// 2457), as the carousel's column takes it: left and right walk the bar
/// (the column's up and down), up and down go round the bars (its left and
/// right).
fn across(key: Key) -> Key {
    match key {
        Key::Left => Key::Up,
        Key::Right => Key::Down,
        Key::Up => Key::Left,
        Key::Down => Key::Right,
        Key::Use => Key::Use,
    }
}

/// Spell Bar Opacity's percent (ticket 1953) as the stacked bars' frames'
/// alpha: 100 the frames as before, 0 none drawn.
pub fn frame_alpha(percent: u8) -> f32 {
    percent.min(100) as f32 / 100.0
}

#[cfg(test)]
mod tests {
    #[test]
    fn pc_shortcut_text_and_shadow_stay_inside_the_cell() {
        use vi::ui::Raster;
        let r = crate::pointer::testing::NullRaster;
        for cell in [[150, 390, 184, 419], [456, 419, 490, 448]] {
            for key in ["1", "0", "shift+f12", "ctrl+shift+9"] {
                let (at, text) = super::shortcut_label(&r, cell, key).expect("a compact label fits");
                assert!(at[0] >= cell[0] && at[1] >= cell[1]);
                assert!(at[0] + r.string_width(&text) + 2 <= cell[2]);
                assert!(at[1] + r.point_size() + 2 <= cell[3]);
                if key.len() > 3 { assert!(String::from_utf16_lossy(&text).ends_with("..")); }
            }
        }
        assert!(super::shortcut_label(&r, [0, 0, 10, 10], "1").is_none(), "do not spill out when even the font cannot fit");
    }
    #[test]
    fn pc_pairs_real_menus_and_keeps_packed_slots_accessible() {
        let vui = crate::pointer::testing::vui();
        let mut r = crate::pointer::testing::NullRaster;
        let mut h = super::HudMenus::default();
        h.defs[0].count = 7;
        h.stacked = true;
        h.draw_pc_bars(&vui, &mut r, 440, 150, super::Bars { abilities: 4, gear: false }, 2);
        assert_eq!(h.stacked_hits.len(), 22);
        let cell = |m, n| h.stacked_hits.iter().find(|&&(menu, slot, _)| menu == m && slot == n).unwrap().2;
        let (a, b, c, d, packed) = (cell(0, 0), cell(1, 0), cell(4, 0), cell(5, 0), cell(0, 5));
        assert_eq!(a[1], b[1]);
        assert_eq!(c[1], d[1]);
        assert_eq!(b[0] - a[0], 5 * (a[2] - a[0]));
        assert!(packed[1] < c[1] && c[1] < a[1]);
        assert_eq!(h.stacked_at([(b[0] + 1) as f32, (b[1] + 1) as f32]), Some((1, 0)));
        h.defs[0].count = 5;
        h.draw_pc_bars(&vui, &mut r, 440, 150, super::Bars { abilities: 1, gear: false }, 4);
        assert_eq!(h.stacked_hits.len(), 40, "even a five-gem server has four aligned ten-cell rows");
        let primary_four = h.stacked_hits.iter().find(|c| c.0 == 0 && c.1 == 0).unwrap().2;
        h.draw_pc_bars(&vui, &mut r, 440, 150, super::Bars { abilities: 1, gear: false }, 1);
        let primary_one = h.stacked_hits.iter().find(|c| c.0 == 0 && c.1 == 0).unwrap().2;
        assert_eq!(primary_one, primary_four, "primary gem stays in place as rows are added");
        h.draw_pc_bars(&vui, &mut r, 440, 150, super::Bars { abilities: 1, gear: false }, 4);
        for menu in [0, 1, 4, 5, super::MENUS, super::MENUS + 1] {
            let expected = if menu >= super::MENUS { 10 } else { 5 };
            assert_eq!(h.stacked_hits.iter().filter(|c| c.0 == menu).count(), expected);
        }
    }

    #[test]
    fn pc_slots_show_only_confirmed_equipped_spells() {
        let mut h = super::HudMenus::default();
        let mut abilities = std::collections::BTreeMap::new();
        for slot in [19, 20, 29, 30, 39, 40] {
            abilities.insert(slot, net::dump::Ability { id: slot, slot, valid: 1, icon_front: 5, ..Default::default() });
        }
        abilities.get_mut(&29).unwrap().lockout = 1;
        abilities.get_mut(&30).unwrap().valid = 0;
        h.sync_pc_slots(&abilities);
        assert_eq!(h.pc_defs[0].entries[0].id, 20);
        assert_eq!(h.pc_defs[0].entries[9].id, 29);
        assert_eq!(h.pc_defs[0].entries[9].x08, 61, "the server lockout badge is retained");
        assert_eq!(h.pc_defs[1].entries[0].msg, 0, "unconfirmed slot is empty");
        assert_eq!(h.pc_defs[1].entries[9].id, 39);
        abilities.get_mut(&20).unwrap().valid = 0;
        abilities.get_mut(&30).unwrap().valid = 1;
        h.sync_pc_slots(&abilities);
        assert_eq!(h.pc_defs[0].entries[0].msg, 0, "unmemorization clears the old cell");
        assert_eq!(h.pc_defs[1].entries[0].id, 30, "the acknowledgement fills the new cell");
    }

    #[test]
    fn equipment_hover_and_pad_focus_name_the_separate_set() {
        let vui = crate::pointer::testing::vui();
        let mut r = crate::pointer::testing::Said::default();
        let mut h = super::HudMenus::default();
        h.stacked = true;
        h.defs[super::GEAR].entries[1] = super::Entry { msg: crate::gear::MSG, id: 1, icon2: 42, x08: -1 };
        h.draw_equipment(&vui, &mut r, super::Bars { abilities: 4, gear: true }, [469, 306]);
        let cell = h.equipment_hits.iter().find(|(_, n, _)| *n == 1).unwrap().2;
        let point = [(cell[0] + 1) as f32, (cell[1] + 1) as f32];
        let name = |msg, id| if msg == crate::gear::MSG && id == 1 { crate::ui::wide("Shield Set") } else { Vec::new() };
        h.hover_equipment(Some(point));
        h.draw_equipment_name(&vui, &mut r, 10, None, &name);
        assert!(r.0.iter().any(|s| s == "Shield Set"));
        h.hover_equipment(None);
        h.named = true;
        h.sel = super::GEAR as i32;
        h.menus[super::GEAR].sel = 1;
        h.menus[super::GEAR].lit = 1;
        h.menus[super::GEAR].time = 100;
        r.0.clear();
        h.draw_equipment_name(&vui, &mut r, 10, None, &name);
        assert!(r.0.iter().any(|s| s == "Shield Set"), "pad focus names the same actual set");
        assert_eq!(h.point_equipment(point), Some(super::Key::Use));
    }
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
        h.draw_stacked(&vui, &mut r, 320, 216, DISC, Layout::Stacked);
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
        h.draw_stacked(&vui, &mut r, 320, 216, Bars { abilities: 1, gear: false }, Layout::Stacked);
        assert_eq!(h.stacked_hits.len(), 5, "one bar, one row");
        h.draw_stacked(&vui, &mut r, 320, 216, DISC, Layout::Columns);
        assert_eq!(h.stacked_hits, hits, "two bars: 2 Columns is one column of two, as Stacked");
    }

    /// Petition 2457: on the stacked bars, which lie across, left and right
    /// walk the bar up (round its five) and up and down go to the bar above
    /// and below, without the switch only lighting it; and played with a
    /// controller (`named`) the selection's name is drawn over the bars, at
    /// the selected cell, while the bar is lit, none past its 20 s or with a
    /// key or the mouse last. The controls: the carousel (not stacked) walks
    /// up and down and switches with left and right, as the game's.
    #[test]
    fn stacked_bars_walk_across_and_name_the_selection_for_a_controller() {
        struct Rec(Vec<([i32; 2], Vec<u16>)>);
        impl Raster for Rec {
            fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
            fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
            fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
            fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
            fn draw_string(&mut self, at: [i32; 2], _: [f32; 4], s: &[u16]) {
                self.0.push((at, s.to_vec()));
            }
            fn draw_centered_string(&mut self, at: [i32; 2], _: [f32; 4], s: &[u16], _: u32) {
                self.0.push((at, s.to_vec()));
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
        let walk = |stacked: bool, keys: &[Key], switch: bool| {
            let mut h = HudMenus { stacked, ..Default::default() };
            for &k in keys {
                h.key(k, 0, DISC, switch, &mut Vec::new());
            }
            (h.sel, h.menus[0].sel, h.menus[1].sel)
        };
        // stacked: along the bar, then down to bar 2 (its own selection kept)
        assert_eq!(walk(true, &[Key::Right, Key::Right], true), (0, 2, 0), "right walks the bar");
        assert_eq!(walk(true, &[Key::Left], true), (0, 4, 0), "left from the first goes round to the fifth");
        assert_eq!(walk(true, &[Key::Down], true), (1, 0, 0), "down: the bar below");
        assert_eq!(walk(true, &[Key::Down, Key::Up], true), (0, 0, 0), "up: the bar above");
        assert_eq!(walk(true, &[Key::Down, Key::Right], true), (1, 0, 1), "along bar 2");
        assert_eq!(walk(true, &[Key::Down, Key::Up], false), (0, 0, 0), "without the switch up and down only light");
        assert_eq!(walk(true, &[Key::Right], false), (0, 1, 0), "and left and right still walk");
        // the controls: the carousel, the game's keys
        assert_eq!(walk(false, &[Key::Down, Key::Down], true), (0, 2, 0), "the carousel: down walks");
        assert_eq!(walk(false, &[Key::Right], true), (1, 0, 0), "the carousel: right switches");
        // the name, over the bars, at the selected cell
        let vui = crate::pointer::testing::vui();
        let ab = |id: i64| Entry { icon2: 5, x08: -1, msg: 0x8060000, id };
        let named_in = |pad: bool, at: i64, screen: Option<[i32; 2]>| {
            let mut h = HudMenus { stacked: true, named: pad, ..Default::default() };
            h.defs[0] = MenuDef { count: 5, entries: vec![ab(11), ab(12), ab(13), Entry::default(), Entry::default()] };
            h.defs[1] = MenuDef { count: 5, entries: vec![ab(21), Entry::default(), Entry::default(), Entry::default(), Entry::default()] };
            h.key(Key::Right, 100, DISC, true, &mut Vec::new());
            let mut r = Rec(Vec::new());
            h.draw_stacked(&vui, &mut r, 320, 216, DISC, Layout::Stacked);
            assert!(r.0.is_empty(), "the bars draw no name themselves");
            let name = |msg: u32, id: i64| crate::ui::wide(&format!("{msg:x} {id}"));
            h.draw_name(&vui, &mut r, at, false, screen, &name);
            let cell = h.stacked_hits.iter().find(|c| c.0 == 0 && c.1 == 1).unwrap().2;
            let top = h.stacked_hits.iter().map(|c| c.2[1]).min().unwrap();
            (r.0, cell, top)
        };
        let named = |pad: bool, at: i64| named_in(pad, at, None);
        let (drawn, cell, top) = named(true, 5000);
        assert_eq!(drawn.len(), 2, "the name and its shadow: {drawn:?}");
        let (at, s) = &drawn[1];
        assert_eq!(String::from_utf16_lossy(s), "8060000 12", "the second slot's ability");
        assert!(at[1] < top && at[0] >= cell[0] && at[0] <= cell[2], "over the bars at the cell: {at:?} {cell:?} top {top}");
        assert!(named(true, 100 + 20001).0.is_empty(), "past the bar's 20 s lit: none");
        assert!(named(false, 5000).0.is_empty(), "a key or the mouse last: none (ticket 1913)");
        // petition 2547: the bars against the screen's right edge, the name
        // (the piece does not count it, so the bars stay) kept on the screen
        let half = "8060000 12".len() as i32 * 8 / 2;
        let mid = (cell[0] + cell[2]) / 2;
        let edge = mid + 10;
        let (drawn, _, _) = named_in(true, 5000, Some([0, edge]));
        assert_eq!(drawn[1].0[0], edge - half, "pulled in to end at the screen's edge: {drawn:?}");
        let (drawn, _, _) = named_in(true, 5000, Some([0, 4000]));
        assert_eq!(drawn[1].0[0], mid, "the control: room on the screen, over the cell");
    }

    /// The server owner's "stack 4 spell bars, or 2 columns of 2, side by
    /// side" (2026-11-18): with four bars, Stacked is four rows of five, bar
    /// 1 on top and bar 4 on the chat box, all from its left; 2 Columns is
    /// bars 1 and 2 at the left and bars 3 and 4 to their right past the gap,
    /// bar 3 level with bar 1, the lower rows on the chat box. A click on a
    /// third or fourth bar's cell makes that bar the one up and uses it; their
    /// slots' keys find their entries (`slot_entry`); three bars in columns
    /// leave bar 3 beside bar 1. Every cell's frame is drawn, at Spell Bar
    /// Opacity 0 too (unseen), so the moved piece is the whole block.
    #[test]
    fn four_bars_stack_or_stand_in_two_columns() {
        struct Rec(Vec<([i32; 2], [i32; 2], f32)>);
        impl Raster for Rec {
            fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
            fn stretch_blit(&mut self, _: u32, _: u32, at: [i32; 2], size: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, alpha: f32) {
                self.0.push((at, size, alpha));
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
                [0, 0, 640, 448]
            }
            fn set_viewport(&mut self, _: [i32; 4]) {}
        }
        crate::settings::testing::reset(false);
        let vui = crate::pointer::testing::vui();
        let mut h = HudMenus::default();
        let ab = |id: i64| Entry { icon2: 5, x08: -1, msg: 0x8060000, id };
        let bar = |first: i64| MenuDef { count: 5, entries: (0..5).map(|n| if n == 2 { Entry::default() } else { ab(first + n) }).collect() };
        for (k, &menu) in BAR_MENUS.iter().enumerate() {
            h.defs[menu] = bar(10 * (k as i64 + 1));
        }
        h.stacked = true;
        let four = Bars { abilities: 4, gear: false };
        let s = |v: i32| (v as f32 * STACKED_SCALE).round() as i32;
        let (cw, ch) = (s(CELL[0]), s(CELL[1]));
        let bottom = 320 - STACKED_GAP;

        let mut r = Rec(Vec::new());
        h.draw_stacked(&vui, &mut r, 320, 216, four, Layout::Stacked);
        assert_eq!(h.stacked_hits.len(), 20, "four bars of five");
        let hits = h.stacked_hits.clone();
        let cell = |hits: &[(usize, i32, [i32; 4])], m: usize, n: i32| hits.iter().find(|c| c.0 == m && c.1 == n).unwrap().2;
        for (k, &menu) in BAR_MENUS.iter().enumerate() {
            let c = cell(&hits, menu, 0);
            assert_eq!([c[0], c[1]], [216, bottom - ch * (4 - k as i32)], "bar {} in its row from the chat box's left", k + 1);
        }
        assert_eq!(cell(&hits, 5, 4)[3], bottom, "bar 4 on the chat box");
        assert_eq!(r.0.iter().filter(|&&(_, size, _)| size == [cw, ch]).count(), 20, "every cell's frame");

        let mut r = Rec(Vec::new());
        h.draw_stacked(&vui, &mut r, 320, 216, four, Layout::Columns);
        assert_eq!(h.stacked_hits.len(), 20);
        let hits = h.stacked_hits.clone();
        let right = 216 + (5.0 * CELL[0] as f32 * STACKED_SCALE).round() as i32 + s(COLUMN_GAP);
        let at = |m: usize| {
            let c = cell(&hits, m, 0);
            [c[0], c[1]]
        };
        assert_eq!((at(0), at(1)), ([216, bottom - 2 * ch], [216, bottom - ch]), "bars 1 and 2 at the left");
        assert_eq!((at(4), at(5)), ([right, bottom - 2 * ch], [right, bottom - ch]), "bars 3 and 4 beside them");
        assert!(cell(&hits, 0, 4)[2] < right, "the columns apart");
        let mid = |c: [i32; 4]| [(c[0] + c[2]) as f32 / 2.0, (c[1] + c[3]) as f32 / 2.0];
        assert_eq!(h.point(mid(cell(&hits, 5, 3))), Some(Key::Use));
        assert_eq!((h.sel, h.menus[5].sel), (5, 3), "bar 4 up, its fourth selected");
        assert_eq!(h.key(Key::Use, 0, four, false, &mut Vec::new()), Some((0x8060000, 43)));
        assert_eq!(h.ability_at(mid(cell(&hits, 4, 1))), Some(31));
        assert_eq!((h.slot_entry(2, 0), h.slot_entry(3, 4), h.slot_entry(3, 2), h.slot_entry(6, 0)), (Some((0x8060000, 30)), Some((0x8060000, 44)), None, None));

        // three bars in columns: bar 3 beside bar 1, nothing beside bar 2
        h.draw_stacked(&vui, &mut r, 320, 216, Bars { abilities: 3, gear: false }, Layout::Columns);
        assert_eq!(h.stacked_hits.len(), 15);
        let c = cell(&h.stacked_hits, 4, 0);
        assert_eq!([c[0], c[1]], [right, bottom - 2 * ch]);
        assert!(!h.stacked_hits.iter().any(|c| c.0 == 5), "no fourth bar");

        // Spell Bar Opacity 0: the frames drawn unseen, the block whole
        crate::settings::set_client(crate::settings::ClientOptions { spell_bar_opacity: 0, ..crate::settings::client() });
        let mut r = Rec(Vec::new());
        h.draw_stacked(&vui, &mut r, 320, 216, four, Layout::Columns);
        let frames: Vec<f32> = r.0.iter().filter(|&&(_, size, _)| size == [cw, ch]).map(|f| f.2).collect();
        assert!(frames.len() == 20 && frames.iter().all(|&a| a == 0.0), "{frames:?}");
        crate::settings::testing::reset(false);
    }

    /// The Utility Bars (the server owner's "upper left, side by side
    /// vertical, or down below", 2026-11-18): with a server's weapon sets,
    /// Upper Left is the items' column of four and the sets' of five beside
    /// it from `UPPER_LEFT_AT`; Below is the items' row over the sets', the
    /// sets' standing on the chat box, and lifts the spell bars by both rows
    /// and the gap. A slot holding an item is drawn whole, an empty one faint.
    /// A click on a cell makes its bar the one up and uses its entry (an
    /// item, a set); the keys' bars find their entries. The controls: with
    /// the bars not stacked a click finds nothing; without the sets, the
    /// items alone, and Below lifts the spell bars one row.
    #[test]
    fn the_utility_bars_stand_upper_left_or_below() {
        struct Rec(Vec<f32>);
        impl Raster for Rec {
            fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, alpha: f32) {
                self.0.push(alpha);
            }
            fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, alpha: f32) {
                self.0.push(alpha);
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
                [0, 0, 640, 448]
            }
            fn set_viewport(&mut self, _: [i32; 4]) {}
        }
        crate::settings::testing::reset(false);
        let vui = crate::pointer::testing::vui();
        let mut h = HudMenus::default();
        let item = |k: i64| Entry { icon2: -1, x08: -1, msg: 0x2030000, id: k };
        h.defs[ITEMS] = MenuDef { count: 4, entries: vec![item(3), Entry::default(), item(5), Entry::default()] };
        let set = |k: i64| Entry { icon2: 7, x08: -1, msg: crate::gear::MSG, id: k };
        h.defs[GEAR] = MenuDef { count: 5, entries: vec![set(0), Entry::default(), Entry::default(), Entry::default(), set(BARE_HANDS)] };
        let s = |v: i32| (v as f32 * STACKED_SCALE).round() as i32;
        let (cw, ch) = (s(CELL[0]), s(CELL[1]));
        let cell = |h: &HudMenus, m: usize, n: i32| h.utility_hits.iter().find(|c| c.0 == m && c.1 == n).unwrap().2;
        let mid = |c: [i32; 4]| [(c[0] + c[2]) as f32 / 2.0, (c[1] + c[3]) as f32 / 2.0];

        let mut r = Rec(Vec::new());
        h.draw_utility(&vui, &mut r, LIVE, UtilityLayout::UpperLeft, UPPER_LEFT_AT);
        assert_eq!(h.utility_hits.len(), 9, "four items and five sets");
        let [x, y] = UPPER_LEFT_AT;
        assert_eq!((cell(&h, ITEMS, 0), cell(&h, ITEMS, 3)[1]), ([x, y, x + cw, y + ch], y + 3 * ch), "the items' column");
        assert_eq!(cell(&h, GEAR, 4)[..2], [x + cw + s(COLUMN_GAP), y + 4 * ch], "the sets' column beside it");
        assert_eq!(r.0.iter().filter(|&&a| a == 0.35).count(), 2, "two empty item slots faint: {:?}", r.0);
        assert_eq!(HudMenus::utility_height(LIVE, UtilityLayout::UpperLeft), 0, "the spell bars where they were");

        assert_eq!(h.point_utility(mid(cell(&h, ITEMS, 2))), None, "the control: not stacked");
        h.stacked = true;
        assert_eq!(h.point_utility(mid(cell(&h, ITEMS, 2))), Some(Key::Use));
        assert_eq!((h.sel, h.menus[ITEMS].sel), (ITEMS as i32, 2));
        assert_eq!(h.key(Key::Use, 0, LIVE, false, &mut Vec::new()), Some((0x2030000, 5)), "the third slot's item");
        assert_eq!(h.point_utility(mid(cell(&h, GEAR, 4))), Some(Key::Use));
        assert_eq!(h.key(Key::Use, 0, LIVE, false, &mut Vec::new()), Some((MSG, BARE_HANDS)), "bare hands");
        assert_eq!(h.point_utility([0.0, 0.0]), None);
        assert_eq!(
            (h.slot_entry(ITEMS_BAR, 0), h.slot_entry(ITEMS_BAR, 1), h.slot_entry(SETS_BAR, 4), h.slot_entry(SETS_BAR, 2)),
            (Some((0x2030000, 3)), None, Some((MSG, BARE_HANDS)), None)
        );

        let bottom = 320 - STACKED_GAP;
        h.draw_utility(&vui, &mut r, LIVE, UtilityLayout::Below, [216, 320]);
        assert_eq!(h.utility_hits.len(), 9);
        assert_eq!(cell(&h, ITEMS, 0)[..2], [216, bottom - 2 * ch], "the items' row over");
        assert_eq!(cell(&h, GEAR, 0)[..2], [216, bottom - ch], "the sets' row on the chat box");
        assert_eq!(cell(&h, GEAR, 4)[0], 216 + (4.0 * CELL[0] as f32 * STACKED_SCALE).round() as i32);
        assert_eq!(HudMenus::utility_height(LIVE, UtilityLayout::Below), 2 * ch + s(COLUMN_GAP), "the spell bars over both rows");

        h.draw_utility(&vui, &mut r, DISC, UtilityLayout::Below, [216, 320]);
        assert_eq!(h.utility_hits.len(), 4, "no sets: the items alone");
        assert_eq!(cell(&h, ITEMS, 0)[..2], [216, bottom - ch]);
        assert_eq!(HudMenus::utility_height(DISC, UtilityLayout::Below), ch + s(COLUMN_GAP));
        h.hide_utility();
        assert_eq!(h.point_utility(mid([216, bottom - ch, 216 + cw, bottom])), None, "hidden: nothing found");
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
