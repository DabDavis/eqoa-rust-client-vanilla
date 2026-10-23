//! The start menu's item menu (CLIENT `viwnditemmenu.cpp`, `VIWndItemMenu`)
//! with the windows it opens (`viwndinspect.cpp`, `viwndquantity.cpp`, a
//! `VIWndMBox`), and the four inventory menus `VIWndGame::UpdateQuickBars`
//! rebuilds every frame (`BuildMenu0310`, `BuildMenu0300`, `BuildInvMenu`,
//! `BuildEquipMenu`): the tabs Usable, Equipment, Charged and Misc (words
//! 437, 438, 1473, 439), each item's sub-menu (use or equip, inspect, quick
//! slot, move, destroy), the description of the selected item beside the
//! list, and the quick-slot mode.
//!
//! What it asks of the client comes back as `ItemOut`, which `play` sends.
//! A pointer (`pointer`) takes the tabs, the rows and their popups, the
//! quick-slot column and the windows it opens.

#![allow(clippy::too_many_arguments)]

use crate::client_image::{Menu, MenuEntry, Menus};
use crate::dialog::{BoxAnswer, Key as BoxKey, MBox};
use crate::infotext::{self, Info};
use crate::inventory;
use crate::pointer::{Hits, key};
use crate::quantity::{Quantity, QuantityUp};
use crate::startmenu::KeyFlash;
use crate::ui::Layout;
use crate::wndlist::{ListUp, WndMenu};
use net::dump::ItemList;
use render::ui::Anchor;
use vi::ui::{Raster, Vui, Window, wide};

/// `sSlotIcons`: the icon on each quick slot.
const SLOT_ICONS: [i32; 4] = [5, 5, 3, 2];

/// What the item menu asks of the client.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ItemOut {
    /// `UseItem(i, b)`: 0x003E (62), use or equip
    UseItem(i32, u8),
    /// `UseItemOn(i, n)`: 0x003C, destroy `n`
    UseItemOn(i32, i32),
    /// `SwapItems(i, j)`: 0x003D (61)
    Swap(i32, i32),
    /// `RequestQuickSlot(i, pos)`: 0x000A
    QuickSlot(i32, i32),
    /// 260 to the start menu: back to its main menu
    Back,
}

/// The four inventory menus, with their sub-menus, as the frame's
/// `UpdateQuickBars` leaves them: (menus, [A usable, B equipment, C
/// charged, D misc]). `counts` are the definitions' slot counts (gItemMenu4,
/// gItemMenu3, gInvMenu, gEquipMenu, out of CLIENT).
pub fn build(inv: &ItemList, counts: [i32; 4], icon: &dyn Fn(usize) -> i32, can_use: &dyn Fn(usize) -> bool) -> (Menus, [usize; 4]) {
    let mut menus = Menus::default();
    let entry = |word: i32, msg: u32, id: i32| MenuEntry { word, msg, id, icon: -1, icon2: -1, number: -1, ..Default::default() };
    let x08 = |k: usize, none_flagged: bool| {
        let it = &inv.items[k];
        if it.item.max_hp > 0 {
            if (it.cur_hp as f32 / it.item.max_hp as f32) < 0.25 { 1 } else { -1 }
        } else if none_flagged {
            1
        } else {
            -1
        }
    };
    let mut out = [0; 4];
    for (tab, count) in counts.iter().enumerate() {
        let keep: fn(&net::dump::NetItem) -> bool = match tab {
            0 => infotext::is_usable,
            1 => infotext::is_wearable,
            2 => infotext::is_charged,
            _ => infotext::is_misc,
        };
        let mut entries = vec![MenuEntry::default(); (*count).max(0) as usize];
        let mut slot = 0;
        for k in 0..inv.items.len() {
            if slot >= entries.len() || !keep(&inv.items[k]) {
                continue;
            }
            let it = &inv.items[k];
            let id = k as i32;
            let mut e = MenuEntry { icon: icon(k), number: it.quantity as i32, x08: -1, word: -1, msg: 0, id, icon2: -1, sub: None };
            let sub = match tab {
                // SetMenu0310Item: use, inspect, (quick slot,) move, destroy;
                // a type-5 item has no quick slot
                0 => {
                    let five = it.item.p4c == 5;
                    let mut v = vec![entry(120, 0x2030000, id), entry(122, 0x2020000, id)];
                    if !five {
                        v.push(entry(433, 0x2070000, id));
                    }
                    v.push(entry(200, 0x2050000, id));
                    v.push(entry(281, 0x2060000, id));
                    v
                }
                // SetMenu0300Item: equip (unequip when worn, badge 138;
                // 140 and "equip" when it cannot be used), inspect, quick
                // slot, move, destroy
                1 => {
                    e.x08 = x08(k, true);
                    let mut first = 121;
                    if it.equip_slot != -1 {
                        e.icon2 = 138;
                        first = 154;
                    }
                    if !can_use(k) {
                        e.icon2 = 140;
                        first = 121;
                    }
                    vec![entry(first, 0x2030000, id), entry(122, 0x2020000, id), entry(433, 0x2070000, id), entry(200, 0x2050000, id), entry(281, 0x2060000, id)]
                }
                // SetInvMenuItem / SetEquipMenuItem: inspect, move, destroy
                _ => {
                    if tab == 2 {
                        e.x08 = x08(k, false);
                    }
                    vec![entry(122, 0x2020000, id), entry(200, 0x2050000, id), entry(281, 0x2060000, id)]
                }
            };
            e.sub = Some(menus.push(Menu { x00: -1, x04: -1, entries: sub }));
            entries[slot] = e;
            slot += 1;
        }
        out[tab] = menus.push(Menu { x00: -1, x04: -1, entries });
    }
    (menus, out)
}

/// `VIWndInspect`: the scrolling description, Triangle to close, X to go a
/// level deeper (the stats) where there is one.
#[derive(Default)]
pub struct Inspect {
    pub depth: i32,
    p28: i32,
    text: Vec<u16>,
    lines: i32,
    rows: i32,
    top: i32,
    shown: i32,
    window: Window,
    pub flash: KeyFlash,
    pub visible: bool,
}

impl Inspect {
    /// `Init(depth, p)` and `SetText`.
    pub fn set(&mut self, depth: i32, p: i32, text: &str) {
        self.depth = depth;
        self.p28 = p;
        self.text = wide(text);
        self.lines = 0;
    }

    /// `OnMessage` 295: the window sized to the text.
    pub fn focus(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.window.init(vui, r, [320, 170], 410, 14, 1);
        let text = self.text.clone();
        self.lines = self.window.count_lines(vui, r, &text, 410);
        self.rows = (self.lines + 2).min(17);
        self.top = 0;
        self.shown = self.rows - 2;
        let w = if self.shown < self.lines { 470 } else { 450 };
        self.window.init(vui, r, [320, 170], w, self.rows, 1);
        if self.window.is_open() {
            self.window.close(true);
        }
        self.window.open(false);
        self.flash.reset();
    }

    /// The keys of a window with no stats behind it (depth 1): Triangle
    /// closes it (true, sound 8), the arrows scroll while held.
    pub fn key(&mut self, key: i32, down: bool, cues: &mut Vec<i32>) -> bool {
        if !down {
            match key {
                19 | 138 => self.flash.release(138),
                20 | 139 => self.flash.release(139),
                _ => {}
            }
            return false;
        }
        match key {
            27 | 132 => {
                cues.push(8);
                return true;
            }
            19 | 138 => self.flash.press(138),
            20 | 139 => self.flash.press(139),
            _ => {}
        }
        false
    }

    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, cues: &mut Vec<i32>) {
        let white = [1.0; 4];
        self.window.draw(vui, r);
        let text = self.text.clone();
        if self.shown < self.lines {
            self.window.draw_frame_vscroll(vui, r, 20, 0, 420, self.shown, self.top as f32 / (self.lines - self.shown) as f32);
            self.window.text_box(vui, r, &text, 20, 0, 410, self.shown, white, ((self.top as u32) << 24) | 1);
        } else {
            self.window.text_box(vui, r, &text, 20, 0, 410, self.shown, white, 1);
        }
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let s = if self.depth > 1 { format!("|x{}   |t{}", w(77), w(161)) } else { format!("|t{}", w(161)) };
        self.window.text(vui, r, &wide(&s), 20, self.rows - 1, white, 1);
        if self.flash.pulse(139) && self.lines > self.shown {
            if self.top < self.lines - self.shown {
                cues.push(11);
                self.top += 1;
            } else {
                cues.push(9);
            }
        }
        if self.flash.pulse(138) && self.shown < self.lines {
            if self.top <= 0 {
                cues.push(9);
                self.top = 0;
            } else {
                cues.push(11);
                self.top -= 1;
            }
        }
    }
}

/// Which of the item menu's windows has the focus.
#[derive(Clone, Copy, PartialEq)]
enum Focus {
    /// the tab's menu
    Menu,
    /// the quick-slot mode, the item menu itself
    Slots,
    Inspect,
    Quantity,
    MBox,
}

/// `VIWndItemMenu`.
pub struct ItemMenu {
    /// A usable, B equipment, C charged, D misc (by `m_menu`)
    menus: [WndMenu; 4],
    tab: i32,
    /// the tab shown, -1 in the quick-slot mode, -2 none
    menu: i32,
    item: i32,
    slot: i32,
    alpha: f32,
    fade_in: i32,
    time: i64,
    desc: String,
    slot_name: String,
    window: Window,
    inspect: Inspect,
    quantity: Quantity,
    mbox: MBox,
    focus: Focus,
    pub visible: bool,
    /// the four inventory menus as this frame built them (`set_built`)
    built: (Menus, [usize; 4]),
    /// the tabs and the quick slots, where they were drawn
    tab_hits: Hits,
    slot_hits: Hits,
}

impl Default for ItemMenu {
    fn default() -> Self {
        let mut mbox = MBox::default();
        mbox.pos = [190, 115];
        ItemMenu {
            menus: std::array::from_fn(|_| WndMenu::new([32, 48], 1)),
            tab: 0,
            menu: -2,
            item: -1,
            slot: 0,
            alpha: 1.0,
            fade_in: 1,
            time: 0,
            desc: String::new(),
            slot_name: String::new(),
            window: Window::default(),
            inspect: Inspect::default(),
            quantity: Quantity::default(),
            mbox,
            focus: Focus::Menu,
            visible: false,
            built: (Menus::default(), [0; 4]),
            tab_hits: Hits::default(),
            slot_hits: Hits::default(),
        }
    }
}

impl ItemMenu {

    /// A pointer at frame point `p` (`pointer`), to the window with the
    /// focus: a tab is the left or right steps to it (the popup closed
    /// first); a row, its popup's rows, the quick slots (up and down to it,
    /// `ChangeSlot`'s rules), the quantity picker and the box take it as
    /// theirs. The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.focus {
            // the description scrolls on the wheel; its legend is glyphs
            Focus::Inspect => Vec::new(),
            Focus::Quantity => self.quantity.point(p),
            Focus::MBox => self.mbox.point(p).then_some(key::CROSS).into_iter().collect(),
            Focus::Slots => match self.slot_hits.at(p) {
                Some(i) if i == self.slot => vec![key::CROSS],
                Some(i) => crate::pointer::steps(self.slot, i, key::UP, key::DOWN),
                None => Vec::new(),
            },
            Focus::Menu if self.menu >= 0 => {
                let m = &mut self.menus[self.menu as usize];
                match self.tab_hits.at(p) {
                    Some(t) => {
                        let mut keys = Vec::new();
                        if m.sub.as_ref().is_some_and(|s| s.visible) {
                            keys.push(key::TRIANGLE);
                        }
                        keys.extend(crate::pointer::steps(self.menu, t, key::LEFT, key::RIGHT));
                        keys
                    }
                    None => m.point(p),
                }
            }
            Focus::Menu => Vec::new(),
        }
    }
    /// The frame's four menus (`SetMenuA`..`D` point the tabs at them).
    pub fn set_built(&mut self, built: (Menus, [usize; 4])) {
        for (m, &k) in self.menus.iter_mut().zip(&built.1) {
            m.menu = k;
        }
        self.built = built;
    }

    /// `Open`: tab 0 at its top, the description window. On the clock the
    /// menu draws with (`vui.time`): the game has one (`m_p07568`), and the
    /// tab's list fades by the time since its focus came, so a focus stamped
    /// on another clock left its rows and empty slots at a negative alpha
    /// until a tab change stamped it again (F3 20261020-204418).
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster) {
        let now = vui.time;
        self.quantity.visible = false;
        self.inspect.visible = false;
        for m in &mut self.menus {
            m.visible = false;
        }
        self.visible = true;
        self.tab = 0;
        self.desc.clear();
        self.menu = 0;
        self.menus[0].reset();
        self.menus[0].focus(true, now);
        self.focus = Focus::Menu;
        self.window.init(vui, r, [535, 180], 200, 15, 1);
        self.window.open(true);
        self.time = now;
    }

    /// `ShowMenu(reset)`: the tab's menu shown and given the focus.
    fn show_menu(&mut self, reset: bool, now: i64) {
        for m in &mut self.menus {
            m.visible = false;
        }
        if let Some(m) = self.menus.get_mut(self.menu.max(0) as usize).filter(|_| self.menu >= 0) {
            if reset {
                m.reset();
            }
            m.visible = true;
            m.list.set_x64(true);
            m.focus(true, now);
        }
        self.focus = Focus::Menu;
    }

    /// `ChangeTab`.
    fn change_tab(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        if self.menu + d < 0 {
            cues.push(9);
            self.menu = 0;
            self.tab = 0;
        } else if self.menu + d >= 4 {
            cues.push(9);
            self.menu = 3;
            self.tab = 3;
        } else {
            cues.push(11);
            self.menu += d;
            self.tab += d;
        }
        self.show_menu(true, now);
        self.fade_in = 1;
        self.alpha = 1.0;
        self.time = now;
    }

    /// `ChangeSlot`: slots 0-1 for a type-2 item; type 7 and the third
    /// slot's items stay put.
    fn change_slot(&mut self, d: i32, inv: &ItemList, now: i64, cues: &mut Vec<i32>) {
        let p4c = inv.items.get(self.item.max(0) as usize).map_or(0, |it| it.item.p4c);
        if self.slot + d < 0 {
            cues.push(9);
            self.slot = 0;
        } else if self.slot + d >= 4 {
            cues.push(9);
            self.slot = 3;
        } else if self.slot == 3 || p4c == 7 {
            cues.push(9);
        } else if self.slot + d < 2 {
            cues.push(11);
            self.slot += d;
        } else {
            cues.push(9);
        }
        self.update_slot_name(inv);
        self.fade_in = 1;
        self.alpha = 1.0;
        self.time = now;
    }

    /// `UpdateSlotName`: what is on the slot now.
    fn update_slot_name(&mut self, inv: &ItemList) {
        self.slot_name = inventory::find_by_quick_slot(inv, self.slot as i64).map_or_else(String::new, |i| inv.items[i].item.name.clone());
    }

    /// `AssignSlot`: tab A's items go to slot 0 (type 2) or 2 (type 7), tab
    /// B's to 3; the quick-slot mode starts.
    fn assign_slot(&mut self, tab: usize, item: i32, inv: &ItemList) -> bool {
        self.item = item;
        let p4c = inv.items.get(item.max(0) as usize).map_or(0, |it| it.item.p4c);
        match tab {
            0 => match p4c {
                2 => self.slot = 0,
                7 => self.slot = 2,
                _ => return false,
            },
            1 => self.slot = 3,
            _ => return false,
        }
        self.menus[tab].list.set_x64(false);
        self.menu = -1;
        self.update_slot_name(inv);
        self.focus = Focus::Slots;
        true
    }

    /// `BackToMenu`.
    fn back_to_menu(&mut self, now: i64) {
        self.menu = if self.menus[3].visible {
            3
        } else if self.menus[0].visible {
            0
        } else {
            1
        };
        self.show_menu(false, now);
    }

    /// `Use(item, count)`: a worn (916), quick-slotted (917) or no-trade
    /// (918) item asks first.
    fn use_item(&mut self, item: i32, count: i32, inv: &ItemList, vui: &Vui, waiting: bool, now: i64, out: &mut Vec<ItemOut>) {
        let Some(it) = inv.items.get(item.max(0) as usize).filter(|_| item >= 0 && !waiting) else {
            self.show_menu(false, now);
            return;
        };
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let name = &it.item.name;
        let text = if it.equip_slot != -1 {
            Some(w(916).replacen("%s", name, 1).replacen("%S", name, 1))
        } else if it.quick_slot >= 0 {
            Some(w(917).replacen("%s", name, 1).replacen("%S", name, 1))
        } else if it.item.no_trade > 0 {
            Some(w(918))
        } else {
            None
        };
        match text {
            Some(t) => {
                self.mbox.prompt(&wide(&t), 1, item, count, now);
                self.focus = Focus::MBox;
            }
            None => {
                out.push(ItemOut::UseItemOn(item, count));
                self.show_menu(false, now);
            }
        }
    }

    /// `Inspect(item)`: the tab's long description in the inspect window.
    fn inspect(&mut self, item: i32, info: &Info, vui: &Vui, r: &mut dyn Raster) {
        let (depth, p, text) = match self.menu {
            0 => (1, -1, info.inv_usable(item)),
            1 => (2, item, info.inv_equipment(item)),
            3 => (1, -1, info.inv_misc(item)),
            _ => (1, -1, info.inv_charged(item)),
        };
        self.inspect.set(depth, p, &text);
        for m in &mut self.menus {
            m.visible = false;
        }
        self.inspect.visible = true;
        self.inspect.focus(vui, r);
        self.focus = Focus::Inspect;
    }

    /// A key (the game's codes), down or up, to the window with the focus;
    /// what the client must do comes back in `out`.
    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, key: i32, down: bool, inv: &ItemList, info: &Info, waiting: bool, cues: &mut Vec<i32>, out: &mut Vec<ItemOut>) {
        let now = vui.time;
        match self.focus {
            Focus::Inspect => {
                if !down {
                    match key {
                        19 | 138 => self.inspect.flash.release(138),
                        20 | 139 => self.inspect.flash.release(139),
                        _ => {}
                    }
                    return;
                }
                match key {
                    27 | 132 => {
                        cues.push(8);
                        self.inspect.visible = false;
                        self.show_menu(false, now);
                    }
                    13 | 134 if self.inspect.depth > 1 => {
                        // 297: InspectBack, the stats
                        self.inspect.depth -= 1;
                        cues.push(13);
                        let text = info.inv_stats(self.inspect.p28);
                        self.inspect.text = wide(&text);
                        self.inspect.focus(vui, r);
                    }
                    19 | 138 => self.inspect.flash.press(138),
                    20 | 139 => self.inspect.flash.press(139),
                    _ => {}
                }
            }
            Focus::Quantity => match self.quantity.key(key, down, cues) {
                // 293: the value, at most the maximum
                Some(QuantityUp::Value(v, id)) => {
                    self.quantity.visible = false;
                    if v > 0 {
                        self.use_item(id, v, inv, vui, waiting, now, out);
                    } else {
                        self.show_menu(false, now);
                    }
                }
                Some(QuantityUp::Cancel) => {
                    self.quantity.visible = false;
                    self.show_menu(false, now);
                }
                None => {}
            },
            Focus::MBox => {
                if !down {
                    return;
                }
                let k = match key {
                    13 | 134 => BoxKey::Enter,
                    27 | 132 => BoxKey::Escape,
                    17 | 136 => BoxKey::Left,
                    18 | 137 => BoxKey::Right,
                    _ => return,
                };
                match self.mbox.key(k, cues) {
                    // 268: UseItemOn; 260: back to the menu
                    Some(BoxAnswer::Yes(i, n)) => {
                        out.push(ItemOut::UseItemOn(i, n));
                        self.show_menu(false, now);
                    }
                    Some(BoxAnswer::No(..)) => self.show_menu(false, now),
                    None => {}
                }
            }
            Focus::Slots => {
                if !down {
                    return;
                }
                match key {
                    27 | 132 => {
                        cues.push(8);
                        self.back_to_menu(now);
                    }
                    13 | 134 => {
                        cues.push(13);
                        out.push(ItemOut::QuickSlot(self.item, self.slot));
                        self.back_to_menu(now);
                    }
                    19 | 138 => self.change_slot(-1, inv, now, cues),
                    20 | 139 => self.change_slot(1, inv, now, cues),
                    _ => {}
                }
            }
            Focus::Menu => {
                let Some(tab) = (self.menu >= 0).then_some(self.menu as usize) else { return };
                // left and right fall through the menu to here (OnKeyDown)
                if down && matches!(key, 17 | 136 | 18 | 137) && !self.menus[tab].sub.as_ref().is_some_and(|s| s.visible) {
                    self.change_tab(if matches!(key, 17 | 136) { -1 } else { 1 }, now, cues);
                    return;
                }
                let up = {
                    let (menus, _) = &self.built;
                    self.menus[tab].key(menus, key, down, now, cues)
                };
                match up {
                    ListUp::Back => out.push(ItemOut::Back),
                    ListUp::Swap(a, b) => {
                        if a != b {
                            out.push(ItemOut::Swap(a, b));
                        }
                        self.menus[tab].list.x60 = false;
                        self.menus[tab].list.select(-1);
                    }
                    ListUp::Action(msg, id) => self.action(tab, msg, id, inv, info, vui, r, waiting, cues, out),
                    ListUp::None => {}
                }
            }
        }
    }

    /// `OnMessage` 261 from a tab's menu: the entry's message, with sound 13
    /// (10 while a request is waiting or for one refused).
    fn action(&mut self, tab: usize, msg: u32, id: i32, inv: &ItemList, info: &Info, vui: &Vui, r: &mut dyn Raster, waiting: bool, cues: &mut Vec<i32>, out: &mut Vec<ItemOut>) {
        let now = vui.time;
        let valid = id >= 0 && (id as usize) < inv.items.len() && !waiting;
        let mut sound = 13;
        if waiting {
            sound = 10;
        } else {
            match msg {
                0x0203_0000 => {
                    if valid {
                        out.push(ItemOut::UseItem(id, 0));
                    } else {
                        sound = 10;
                    }
                }
                0x0202_0000 => {
                    if valid {
                        self.inspect(id, info, vui, r);
                    } else {
                        sound = 10;
                    }
                }
                0x0206_0000 => {
                    if !valid {
                        sound = 10;
                    } else if inv.items[id as usize].quantity > 1 {
                        self.quantity.open(vui, r, inv.items[id as usize].quantity as i32, id, -1);
                        self.focus = Focus::Quantity;
                    } else {
                        self.use_item(id, 1, inv, vui, waiting, now, out);
                    }
                }
                0x0207_0000 => {
                    if (tab == 0 || tab == 1) && !self.assign_slot(tab, id, inv) {
                        sound = 10;
                    }
                }
                0x0205_0000 => {
                    let l = &mut self.menus[tab].list;
                    if l.count() >= 2 {
                        let sel = l.sel;
                        l.select(sel);
                        l.x60 = true;
                    }
                }
                _ => {}
            }
        }
        cues.push(sound);
    }

    /// `OnDrawSelf` (with `UpdateDesc`), then the open windows over it. In a
    /// window wider than 4:3 it stays in the start menu's 4:3 picture, whose
    /// left post its own left panel covers: the post goes back over it
    /// (`startmenu::close_sides`).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout, info: &Info, cues: &mut Vec<i32>) {
        if !self.visible {
            return;
        }
        let now = vui.time;
        let dt = (now - self.time) as f32 * 0.001;
        self.time = now;
        // UpdateDesc: the selected entry's short description
        let menus = &self.built.0;
        self.desc = match self.menu {
            0..=3 => {
                let m = &self.menus[self.menu as usize];
                let item = m.id(menus, m.list.sel);
                match self.menu {
                    3 => info.inv_food(item),
                    0 => info.inv_consumable(item),
                    1 => info.inv_equipment_short(item),
                    _ => info.inv_charged_short(item),
                }
            }
            _ => String::new(),
        };
        let (white, black) = ([1.0; 4], [0.0, 0.0, 0.0, 1.0]);
        let mut pos = [110, 20];
        let mut lift = pos;
        vui.blit_bar(r, &[], black, [70, 8], 33, 34, 500, 0, 1.0, 0, true);
        self.tab_hits.clear();
        for (k, (word, step)) in [(437, 100), (438, 140), (1473, 140), (439, 0)].iter().enumerate() {
            let label = wide(vui.words.get(*word, vui.language));
            self.tab_hits.push([pos[0] - 25, 8, pos[0] + vui.string_width(r, &label) + 10, 34], k as i32);
            vui.draw_centered_string_ex(r, &label, pos[0], pos[1], white, 1, true);
            if self.tab == k as i32 {
                lift[0] = pos[0] - 25;
            }
            pos[0] += step;
        }
        vui.blit_rect(r, 51, [-1, 48], [175, 265], 0, 1.0);
        r.anchor(Anchor::Frame);
        crate::startmenu::close_sides(vui, r, crate::startmenu::MIDDLE, true);
        r.anchor(Anchor::Middle);
        let e = vui.entry(89);
        lift[1] = pos[1] - e.uv1[1] / 2;
        vui.blit_ex(r, e.x00, lift, 0, 1.0, 0);
        let width = self.window.size[0];
        if self.menu != -1 {
            let desc = wide(&self.desc);
            self.window.text_box(vui, r, &desc, 20, 0, width - 40, 15, white, 3);
        } else {
            self.draw_slots(vui, r);
        }
        if self.menu == -1 {
            if self.fade_in == -1 {
                self.alpha = (self.alpha - dt * 2.0).max(0.0);
                if self.alpha == 0.0 {
                    self.fade_in = 1;
                }
            } else {
                self.alpha = (self.alpha + dt * 3.0).min(1.0);
                if self.alpha == 1.0 {
                    self.fade_in = -1;
                }
            }
        } else {
            self.alpha = 1.0;
        }
        for m in &mut self.menus {
            m.draw(vui, r, &self.built.0, now, cues);
        }
        if self.inspect.visible {
            self.inspect.draw(vui, r, cues);
        }
        if self.quantity.visible {
            self.quantity.draw(vui, r, cues);
        }
        if self.focus == Focus::MBox {
            self.mbox.draw(vui, r, now);
        }
    }

    /// `DrawSlots`: the slot column, the cursor pulsing on the slot, the
    /// slotted item's name.
    fn draw_slots(&mut self, vui: &Vui, r: &mut dyn Raster) {
        let mut base = [522, 60];
        let mut pos = [base[0] + 6, base[1] + 8];
        let (top, mid) = (vui.entry(158), vui.entry(121));
        vui.blit_ex(r, top.x00, base, 0, 1.0, 0);
        base[1] += top.uv1[1];
        for k in 0..4 {
            let h = if k == 3 { mid.uv1[1] - 4 } else { mid.uv1[1] };
            vui.blit_ex_size(r, mid.x00, base, [34, h], 0, 1.0, 0);
            base[1] += h;
        }
        vui.blit_ex_size(r, 158, base, [top.uv1[0], 3], 0, 1.0, 2);
        self.slot_hits.clear();
        for (i, &icon) in SLOT_ICONS.iter().enumerate() {
            self.slot_hits.push([pos[0] - 6, pos[1] - 4, pos[0] + 28, pos[1] + 28], i as i32);
            if i as i32 == self.slot {
                vui.blit_ex(r, 86, [pos[0] - 25, pos[1] + 2], 3, self.alpha, 1);
            }
            vui.blit(r, icon, pos);
            pos[1] += 32;
        }
        let width = self.window.size[0];
        let name = wide(&self.slot_name);
        self.window.text_box(vui, r, &name, 20, 11, width - 40, 2, [1.0; 4], 11);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::dump::NetItem;

    fn it(wear: i64, p4c: i64, p54: i64, qty: i64) -> NetItem {
        let mut n = NetItem { quantity: qty, equip_slot: -1, quick_slot: -1, ..Default::default() };
        n.item.wear_slot = wear;
        n.item.p4c = p4c;
        n.item.p54 = p54;
        n
    }

    /// The blits at the item list's six row places (32, 48 + 44 k): the
    /// empty slots of the grid, with their alpha.
    #[derive(Default)]
    struct Slots(Vec<f32>);

    impl Layout for Slots {}

    impl Raster for Slots {
        fn blit(&mut self, _: u32, _: u32, at: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, alpha: f32) {
            if at[0] == 32 && (48..48 + 6 * 44).contains(&at[1]) && (at[1] - 48) % 44 == 0 {
                self.0.push(alpha);
            }
        }
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
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

    /// The inventory opened shows its grid from the first frame on: the
    /// tab's list is focused on the clock it is drawn with (`vui.time`, the
    /// HUD's), whatever clock the session keeps, so its six empty slots are
    /// drawn whole frame after frame. With the focus stamped on another
    /// clock (the DRDP clock, milliseconds from the wall clock at connect)
    /// the list's fade ran from a huge negative time and the slots were
    /// drawn at an alpha far below zero until a tab change (F3
    /// 20261020-204418).
    #[test]
    fn the_grid_is_drawn_from_the_first_frame() {
        let inv = ItemList { p04: 0, items: Vec::new() };
        let mut vui = crate::pointer::testing::vui();
        let words = vui.words.clone();
        let base = [0; 29];
        let info = Info { words: &words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let mut m = ItemMenu::default();
        m.set_built(build(&inv, [30, 30, 30, 40], &|_| -1, &|_| true));
        vui.time = 1_000;
        m.open(&vui, &Slots::default());
        assert_eq!(m.menus[0].list.time, vui.time, "focused on the HUD's clock");
        for frame in 1..=3 {
            vui.time = 1_000 + 33 * frame;
            let mut r = Slots::default();
            let mut cues = Vec::new();
            m.draw(&vui, &mut r, &info, &mut cues);
            assert_eq!(r.0.len(), 6, "six empty slots");
            assert!(r.0.iter().all(|&a| a == 1.0), "frame {frame}: the slots drawn whole, {:?}", r.0);
        }
    }

    /// The inventory stays in the start menu's 4:3 picture in a wide window:
    /// a pointer meets its rows and tabs at their places there, as on a 4:3
    /// window (where a wide one had the rows, out at its left side until
    /// 2026-10-23, is off them: the control). Its own left panel, laid over
    /// the start menu's, has the posts drawn again right over it, on a wide
    /// window only.
    #[test]
    fn the_inventory_stays_in_the_picture() {
        use crate::pointer::testing::Wide;
        use crate::startmenu::posts;
        let inv = ItemList { p04: 0, items: vec![it(-1, 2, 0, 1), it(-1, 2, 0, 1)] };
        let words = crate::pointer::testing::vui().words.clone();
        let base = [0; 29];
        let info = Info { words: &words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let menu = |side: i32| {
            let mut vui = posts::vui();
            vui.time = 1_000;
            let mut m = ItemMenu::default();
            m.set_built(build(&inv, [30, 30, 30, 40], &|k| 100 + k as i32, &|_| true));
            m.open(&vui, &Wide::new(side));
            vui.time = 1_033;
            let mut r = Wide::new(side);
            m.draw(&vui, &mut r, &info, &mut Vec::new());
            (m, r)
        };
        let row1 = [32.0 + 50.0, 48.0 + 44.0 + 20.0];
        for side in [0, 106] {
            let (mut m, _) = menu(side);
            assert!(m.point([row1[0] - 106.0, row1[1]]).is_empty(), "side {side}: off the list");
            assert_eq!(m.point(row1), vec![crate::pointer::MOVED], "side {side}: row 1");
            // the tabs (the third, two steps right)
            assert_eq!(m.point([110.0 + 240.0, 20.0]), vec![key::RIGHT, key::RIGHT]);
        }
        let ((_, tv), (_, wide)) = (menu(0), menu(106));
        let want = posts::made(Anchor::Frame, crate::startmenu::MIDDLE[0], true);
        let panel = wide.2.iter().position(|b| b.1 == [-1, 48]).expect("the left panel");
        assert_eq!(wide.2[panel + 1..panel + 1 + want.len()], want[..], "the posts over the panel");
        let added: Vec<_> = wide.2.iter().filter(|b| !tv.2.contains(b)).copied().collect();
        assert_eq!(added, want, "nothing else; no post on a 4:3 window");
        assert!(wide.2.iter().all(|b| b.0 == Anchor::Middle || b.0 == Anchor::Frame), "nothing at a side");
    }

    #[test]
    fn the_four_menus() {
        let mut worn = it(1, 0, 0, 1);
        worn.equip_slot = 0;
        let inv = ItemList { p04: 0, items: vec![it(-1, 2, 0, 3), worn, it(-1, 5, 0, 1), it(-1, 0, 4, 2), it(-1, 0, 0, 1), it(12, 0, 0, 1)] };
        let (menus, t) = build(&inv, [30, 30, 30, 40], &|k| 100 + k as i32, &|k| k != 5);
        let ids = |m: usize| menus.list[m].entries.iter().filter(|e| e.id >= 0).map(|e| e.id).collect::<Vec<_>>();
        assert_eq!(ids(t[0]), vec![0, 2]);
        assert_eq!(ids(t[1]), vec![1, 5]);
        assert_eq!(ids(t[2]), vec![3]);
        assert_eq!(ids(t[3]), vec![4]);
        assert_eq!(menus.list[t[3]].entries.len(), 40, "the definition's slots, the rest empty");
        let sub = |m: usize, k: usize| menus.list[menus.list[m].entries[k].sub.unwrap()].entries.iter().map(|e| e.word).collect::<Vec<_>>();
        assert_eq!(sub(t[0], 0), vec![120, 122, 433, 200, 281]);
        assert_eq!(sub(t[0], 1), vec![120, 122, 200, 281], "type 5: no quick slot");
        assert_eq!(sub(t[1], 0), vec![154, 122, 433, 200, 281], "worn: unequip");
        assert_eq!(menus.list[t[1]].entries[0].icon2, 138);
        assert_eq!((menus.list[t[1]].entries[1].icon2, sub(t[1], 1)[0]), (140, 121), "unusable");
        assert_eq!(menus.list[t[1]].entries[0].x08, 1, "no hit points: flagged");
        assert_eq!(menus.list[t[0]].entries[0].icon, 100);
        assert_eq!(menus.list[t[0]].entries[0].number, 3);
    }
}
