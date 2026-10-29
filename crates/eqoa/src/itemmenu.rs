//! The start menu's item menu (CLIENT `viwnditemmenu.cpp`, `VIWndItemMenu`)
//! with the windows it opens (`viwndinspect.cpp`, `viwndquantity.cpp`, a
//! `VIWndMBox`), and the four inventory menus `VIWndGame::UpdateQuickBars`
//! rebuilds every frame (`BuildMenu0310`, `BuildMenu0300`, `BuildInvMenu`,
//! `BuildEquipMenu`): the tabs Usable, Equipment, Charged and Misc (words
//! 437, 438, 1473, 439), each item's sub-menu (use or equip, inspect, quick
//! slot, move, destroy), the description of the selected item beside the
//! list, and the quick-slot mode. With SOE's final live client's layout
//! (`gear`: a server that keeps weapon sets) the quick slots are its own
//! (potions on 0-1, gems on 2-3) and Prepare on a weapon makes a weapon set:
//! its weapon-set mode, the sets' column and their name editor
//! (`VIWndEdit`), read off that build
//! (`../claude-notes/live-toolbars-gear-sets-20261024.md`).
//!
//! What it asks of the client comes back as `ItemOut`, which `play` sends.
//! A pointer (`pointer`) takes the tabs, the rows and their popups, the
//! quick-slot column and the windows it opens.

#![allow(clippy::too_many_arguments)]

use crate::client_image::{Menu, MenuEntry, Menus};
use crate::dialog::{BoxAnswer, Key as BoxKey, MBox};
use crate::editbox::{EditBox, EditUp};
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

/// `sSlotIcons`: the icon on each quick slot; the live client's (0x013FCED0)
/// a gem's on 3 too.
const SLOT_ICONS: [i32; 4] = [5, 5, 3, 2];
const SLOT_ICONS_LIVE: [i32; 4] = [5, 5, 3, 3];

/// What the item menu asks of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemOut {
    /// `UseItem(i, b)`: 0x003E (62), use or equip
    UseItem(i32, u8),
    /// `UseItemOn(i, n)`: 0x003C, destroy `n`
    UseItemOn(i32, i32),
    /// `SwapItems(i, j)`: 0x003D (61)
    Swap(i32, i32),
    /// `RequestQuickSlot(i, pos)`: 0x000A
    QuickSlot(i32, i32),
    /// The live client's `RequestGearSet(set, name)` (0x012C33E8): 0x0021
    GearSet(i32, String),
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
    /// the live client's weapon-set mode, the item menu itself
    Gear,
    /// the live client's set name editor (`m_edit`)
    Edit,
    Inspect,
    Quantity,
    MBox,
}

/// `VIWndItemMenu`.
pub struct ItemMenu {
    /// A usable, B equipment, C charged, D misc (by `m_menu`)
    menus: [WndMenu; 4],
    tab: i32,
    /// the tab shown, -1 in the quick-slot mode, -2 in the weapon-set mode
    /// (the live client's; the disc's none was -2), -3 none
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
    /// the tabs and the quick slots (or the sets), where they were drawn
    tab_hits: Hits,
    slot_hits: Hits,
    /// the live client's weapon sets this frame (`set_gear`)
    gear: crate::gear::View,
    /// the set name editor (`m_edit`, the live `Init`'s `VIWndEdit`)
    edit: EditBox,
}

impl Default for ItemMenu {
    fn default() -> Self {
        let mut mbox = MBox::default();
        mbox.pos = [190, 115];
        ItemMenu {
            menus: std::array::from_fn(|_| WndMenu::new([32, 48], 1)),
            tab: 0,
            menu: -3,
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
            gear: crate::gear::View::default(),
            edit: {
                // the live Init: centred on (320, 310) as it is made
                let mut e = EditBox::default();
                e.set_center([320, 310]);
                e
            },
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
            Focus::Slots | Focus::Gear => match self.slot_hits.at(p) {
                Some(i) if i == self.slot => vec![key::CROSS],
                Some(i) => crate::pointer::steps(self.slot, i, key::UP, key::DOWN),
                None => Vec::new(),
            },
            Focus::Edit => self.edit.point(p).flatten().into_iter().collect(),
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
    /// The live client's weapon sets this frame (`gear::Gear::view`): with
    /// them on, the quick slots and Prepare are the live client's.
    pub fn set_gear(&mut self, v: crate::gear::View) {
        self.gear = v;
    }

    /// The set name editor has the keys: the keyboard types into it.
    pub fn typing(&self) -> bool {
        self.visible && self.focus == Focus::Edit
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
    /// slot's items stay put. The live client's (0x013745E0): a type-7 item
    /// over 2-3, any other over 0-1.
    fn change_slot(&mut self, d: i32, inv: &ItemList, now: i64, cues: &mut Vec<i32>) {
        let p4c = inv.items.get(self.item.max(0) as usize).map_or(0, |it| it.item.p4c);
        if self.slot + d < 0 {
            cues.push(9);
            self.slot = 0;
        } else if self.slot + d >= 4 {
            cues.push(9);
            self.slot = 3;
        } else if self.gear.on {
            if (p4c == 7) == (self.slot + d >= 2) {
                cues.push(11);
                self.slot += d;
            } else {
                cues.push(9);
            }
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

    /// The live client's `ChangeGearSlot` (0x013746F8): the set chosen, 0-3.
    fn change_gear_slot(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        let n = self.slot + d;
        if n < 0 {
            cues.push(9);
            self.slot = 0;
        } else if n >= crate::gear::SETS as i32 {
            cues.push(9);
            self.slot = crate::gear::SETS as i32 - 1;
        } else {
            cues.push(11);
            self.slot = n;
        }
        self.update_gear_name();
        self.fade_in = 1;
        self.alpha = 1.0;
        self.time = now;
    }

    /// The live client's `UpdateGearName` (0x01374810): the set's name.
    fn update_gear_name(&mut self) {
        self.slot_name = self.gear.names.get(self.slot.max(0) as usize).cloned().unwrap_or_default();
    }

    /// `AssignSlot`: tab A's items go to slot 0 (type 2) or 2 (type 7), tab
    /// B's to 3; the quick-slot mode starts. With the live client's sets
    /// (its `AssignSlot`, 0x01373EF0) a weapon of tab B makes a set instead:
    /// worn first when it is not (`UseItem(i, 0)`), and the weapon-set mode
    /// starts on the first empty set, else the first. (The live one updates
    /// the quick slots' name before it sets the slot, and so names the last
    /// slot's item until the cursor moves: here the disc's order.)
    fn assign_slot(&mut self, tab: usize, item: i32, inv: &ItemList, out: &mut Vec<ItemOut>) -> bool {
        self.item = item;
        let it = inv.items.get(item.max(0) as usize).filter(|_| item >= 0);
        let p4c = it.map_or(0, |it| it.item.p4c);
        match tab {
            0 => match p4c {
                2 => self.slot = 0,
                7 => self.slot = 2,
                _ => return false,
            },
            1 if self.gear.on => {
                let Some(it) = it.filter(|it| crate::gear::preparable(it.item.wear_slot)) else { return false };
                if it.equip_slot == -1 {
                    out.push(ItemOut::UseItem(item, 0));
                }
                self.slot = (0..crate::gear::SETS).find(|&k| self.gear.empty[k]).unwrap_or(0) as i32;
                self.menus[tab].list.set_x64(false);
                self.menu = -2;
                self.update_gear_name();
                self.focus = Focus::Gear;
                return true;
            }
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

    /// `Use(item, count)` as SOE's final live client has it (0x013738E0):
    /// every item asks first, in the Yes/No box starting on No: a worn one
    /// with word 916, a quick-slotted one 917, a no-trade one 918 and any
    /// other with 1841, 'Destroy item "<name>"?'. The disc's copied 915
    /// ("Destroy item?") into the box and destroyed an ordinary item at once
    /// (the client asked with 915 since F3 20261023-082704, the user's
    /// choice).
    fn use_item(&mut self, item: i32, count: i32, inv: &ItemList, vui: &Vui, waiting: bool, now: i64) {
        let Some(it) = inv.items.get(item.max(0) as usize).filter(|_| item >= 0 && !waiting) else {
            self.show_menu(false, now);
            return;
        };
        let w = |id: i32| crate::livewords::word(&vui.words, id, vui.language);
        let name = &it.item.name;
        let text = if it.equip_slot != -1 {
            w(916).replacen("%s", name, 1).replacen("%S", name, 1)
        } else if it.quick_slot >= 0 {
            w(917).replacen("%s", name, 1).replacen("%S", name, 1)
        } else if it.item.no_trade > 0 {
            w(918)
        } else {
            w(1841).replacen("%s", name, 1).replacen("%S", name, 1)
        };
        self.mbox.prompt(&wide(&text), 1, item, count, now);
        self.focus = Focus::MBox;
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
                        self.use_item(id, v, inv, vui, waiting, now);
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
            // the live client's OnKeyDown in the weapon-set mode: X opens the
            // name editor on the set's name (SetRows(1), SetMaxLength(16),
            // its title word 160)
            Focus::Gear => {
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
                        let name = wide(&self.gear.names.get(self.slot.max(0) as usize).cloned().unwrap_or_default());
                        self.edit.set_rows(1);
                        self.edit.set_max_length(crate::gear::NAME_MAX);
                        self.edit.set_text(&name);
                        self.edit.title = wide(&crate::gear::word(&vui.words, 160, vui.language));
                        self.edit.focus(now);
                        self.focus = Focus::Edit;
                    }
                    19 | 138 => self.change_gear_slot(-1, now, cues),
                    20 | 139 => self.change_gear_slot(1, now, cues),
                    _ => {}
                }
            }
            // the name editor: done is the live OnMessage 259, RequestGearSet
            // with its text, cancelled 260; either goes back to the menu
            Focus::Edit => {
                if !down {
                    self.edit.key_up(key);
                    return;
                }
                match self.edit.key(vui, &*r, key, now, cues) {
                    EditUp::Done(_) => {
                        out.push(ItemOut::GearSet(self.slot, String::from_utf16_lossy(&self.edit.text)));
                        self.back_to_menu(now);
                    }
                    EditUp::Cancel => self.back_to_menu(now),
                    EditUp::None => {}
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
                        self.use_item(id, 1, inv, vui, waiting, now);
                    }
                }
                0x0207_0000 => {
                    if (tab == 0 || tab == 1) && !self.assign_slot(tab, id, inv, out) {
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
        // ended at the start menu's bottom rail's bar, as its frame is
        let below = crate::startmenu::BOTTOM_RAIL + crate::startmenu::RAIL_BAR;
        crate::startmenu::close_sides(vui, &mut crate::ui::Cut::new(r, below), crate::startmenu::MIDDLE, true);
        r.anchor(Anchor::Middle);
        let e = vui.entry(89);
        lift[1] = pos[1] - e.uv1[1] / 2;
        vui.blit_ex(r, e.x00, lift, 0, 1.0, 0);
        let width = self.window.size[0];
        match self.menu {
            -1 => self.draw_slots(vui, r),
            -2 => self.draw_gear_slots(vui, r),
            _ => {
                let desc = wide(&self.desc);
                self.window.text_box(vui, r, &desc, 20, 0, width - 40, 15, white, 3);
            }
        }
        if self.menu == -1 || self.menu == -2 {
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
        if self.focus == Focus::Edit {
            self.edit.draw(vui, r, now, cues);
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
        let icons = if self.gear.on { SLOT_ICONS_LIVE } else { SLOT_ICONS };
        for (i, &icon) in icons.iter().enumerate() {
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

    /// The live client's `DrawGearSlots` (0x01372FB0): the quick slots'
    /// column a row taller, the cursor pulsing on the set, each of the sets'
    /// menu's five entries (the sets, bare hands) its badge then its icon,
    /// whole (`VIUI::Blit(id, at)`), and the set's name.
    fn draw_gear_slots(&mut self, vui: &Vui, r: &mut dyn Raster) {
        let mut base = [522, 60];
        let mut pos = [base[0] + 6, base[1] + 8];
        let (top, mid) = (vui.entry(158), vui.entry(121));
        vui.blit_ex(r, top.x00, base, 0, 1.0, 0);
        base[1] += top.uv1[1];
        for k in 0..5 {
            let h = if k == 4 { mid.uv1[1] - 4 } else { mid.uv1[1] };
            vui.blit_ex_size(r, mid.x00, base, [34, h], 0, 1.0, 0);
            base[1] += h;
        }
        vui.blit_ex_size(r, 158, base, [top.uv1[0], 3], 0, 1.0, 2);
        self.slot_hits.clear();
        for i in 0..crate::gear::ENTRIES {
            // only a set is chosen; bare hands is shown
            if i < crate::gear::SETS {
                self.slot_hits.push([pos[0] - 6, pos[1] - 4, pos[0] + 28, pos[1] + 28], i as i32);
            }
            if i as i32 == self.slot {
                vui.blit_ex(r, 86, [pos[0] - 25, pos[1] + 2], 3, self.alpha, 1);
            }
            for id in [self.gear.badges[i], self.gear.icons[i]] {
                if id != -1 {
                    vui.blit_id(r, id as u32, pos, 0, 1.0, 0);
                }
            }
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

    /// Destroy asks for every item, in the Yes/No box starting on No, and
    /// only Yes sends 0x003C: a worn item with word 916, a quick-slotted one
    /// with 917 and a NO TRADE one with 918, as the game's `Use` asks
    /// (`VIWndItemMenu::Use`, byte-exact), and any other item with the live
    /// client's 1841 and its name (live 0x01373A24; the table here lacks the
    /// row, so it is `livewords`'). By keys and by the pointer: a click on No
    /// keeps it, on Yes destroys it. The control: the disc's rule, an
    /// ordinary item destroyed with no box, fails it.
    #[test]
    fn destroying_asks_for_every_item() {
        let mut lines = vec![String::new(); 918];
        lines[914] = "Destroy item?".into();
        lines[915] = "Destroy equipped item \"%S\"?".into();
        lines[916] = "Destroy prepared item \"%S\"?".into();
        lines[917] = "Destroy this NO TRADE item?".into();
        let words = vi::strtab::StringTable::parse(lines.join("\n").as_bytes()).unwrap();
        let mut vui = Vui::new(words, Default::default(), Vec::new());
        vui.time = 1_000;
        let named = |name: &str, f: fn(&mut NetItem)| {
            let mut x = it(-1, 2, 0, 1);
            x.item.name = name.into();
            f(&mut x);
            x
        };
        let inv = ItemList {
            p04: 0,
            items: vec![
                named("Rusty Dagger", |_| {}),
                named("Fine Sword", |x| x.equip_slot = 2),
                named("Healing Potion", |x| x.quick_slot = 0),
                named("Quest Token", |x| x.item.no_trade = 1),
            ],
        };
        let words = vui.words.clone();
        let base = [0; 29];
        let info = Info { words: &words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let menu = || {
            let mut m = ItemMenu::default();
            m.set_built(build(&inv, [30, 30, 30, 40], &|_| -1, &|_| true));
            m
        };
        let press = |m: &mut ItemMenu, k: i32, out: &mut Vec<ItemOut>| {
            m.key(&vui, &mut crate::pointer::testing::NullRaster, k, true, &inv, &info, false, &mut Vec::new(), out);
        };
        let cases = [(0, "Destroy item \"Rusty Dagger\"?"), (1, "Destroy equipped item \"Fine Sword\"?"), (2, "Destroy prepared item \"Healing Potion\"?"), (3, "Destroy this NO TRADE item?")];
        for (k, text) in cases {
            let mut m = menu();
            let mut out = Vec::new();
            m.use_item(k, 1, &inv, &vui, false, vui.time);
            assert!(m.focus == Focus::MBox, "item {k}: the box first");
            assert_eq!(String::from_utf16_lossy(&m.mbox.text), text);
            // ✕ on No, where the box starts: nothing destroyed
            press(&mut m, 13, &mut out);
            assert!(out.is_empty() && m.focus != Focus::MBox, "item {k}: No keeps it");
            // again, left to Yes and ✕: destroyed
            m.use_item(k, 1, &inv, &vui, false, vui.time);
            press(&mut m, 17, &mut out);
            press(&mut m, 13, &mut out);
            assert_eq!(out, vec![ItemOut::UseItemOn(k, 1)], "item {k}: Yes destroys it");
        }
        // the pointer, on the ordinary item: the box drawn, its two buttons
        // found where a click takes them (Yes left of No), No then Yes
        let mut m = menu();
        m.open(&vui, &crate::pointer::testing::NullRaster);
        m.use_item(0, 1, &inv, &vui, false, vui.time);
        m.draw(&vui, &mut crate::pointer::testing::NullRaster, &info, &mut Vec::new());
        let mut row: Vec<[f32; 2]> = Vec::new();
        for y in (0..448).step_by(2) {
            for x in (0..640).step_by(2) {
                if !m.point([x as f32, y as f32]).is_empty() {
                    row.push([x as f32, y as f32]);
                }
            }
        }
        assert!(!row.is_empty(), "the box's buttons take a pointer");
        let y = row[0][1];
        let on_row: Vec<f32> = row.iter().filter(|p| p[1] == y).map(|p| p[0]).collect();
        let (yes, no) = ([on_row[0], y], [*on_row.last().unwrap(), y]);
        let mut out = Vec::new();
        for k in m.point(no) {
            press(&mut m, k, &mut out);
        }
        assert!(out.is_empty(), "a click on No keeps it");
        m.use_item(0, 1, &inv, &vui, false, vui.time);
        m.draw(&vui, &mut crate::pointer::testing::NullRaster, &info, &mut Vec::new());
        for k in m.point(yes) {
            press(&mut m, k, &mut out);
        }
        assert_eq!(out, vec![ItemOut::UseItemOn(0, 1)], "a click on Yes destroys it");
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
        // ended at the start menu's bottom rail's bar
        let below = crate::startmenu::BOTTOM_RAIL + crate::startmenu::RAIL_BAR;
        let want = posts::made_below(Anchor::Frame, crate::startmenu::MIDDLE[0], true, below);
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

    /// Words 159 and 160 as the disc has them ("UNUSED"), the rest empty.
    fn gear_vui() -> Vui {
        let mut lines = vec![String::new(); 160];
        lines[158] = "UNUSED".into();
        lines[159] = "UNUSED".into();
        let words = vi::strtab::StringTable::parse(lines.join("\n").as_bytes()).unwrap();
        let mut vui = Vui::new(words, Default::default(), Vec::new());
        vui.time = 1_000;
        vui
    }

    fn gear_view(empty: [bool; 4]) -> crate::gear::View {
        crate::gear::View {
            on: true,
            names: ["Dirk".into(), String::new(), "Pick".into(), String::new(), "bare hands".into()],
            icons: [11, -1, 12, -1, 15],
            badges: [21, -1, -1, -1, -1],
            empty,
        }
    }

    /// SOE's live layout (its `AssignSlot`, `OnKeyDown` and `OnMessage`):
    /// Prepare on a weapon not worn puts it on first (`UseItem(i, 0)`) and
    /// starts the weapon-set mode on the first empty set; up and down pick
    /// 0-3 with sound 9 at the ends; X opens the name editor on the set's
    /// name, titled "NAME CONFIGURATION" where the disc's word 160 is
    /// "UNUSED"; Enter sends the set its name (`RequestGearSet`, 0x0021) and
    /// the menu is back; Escape sends nothing. A weapon already worn is not
    /// put on again, and an item Prepare makes no set of is refused. The
    /// control: without the sets, Prepare on a weapon is the disc's slot 3.
    #[test]
    fn prepare_on_a_weapon_makes_a_weapon_set() {
        let vui = gear_vui();
        let mut worn = it(14, 0, 0, 1);
        worn.equip_slot = 14;
        let inv = ItemList { p04: 0, items: vec![it(12, 0, 0, 1), worn, it(5, 0, 0, 1)] };
        let words = vui.words.clone();
        let base = [0; 29];
        let info = Info { words: &words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let mut r = crate::pointer::testing::NullRaster;
        let menu = |gear: Option<crate::gear::View>| {
            let mut m = ItemMenu::default();
            m.set_built(build(&inv, [30, 30, 30, 40], &|_| -1, &|_| true));
            if let Some(g) = gear {
                m.set_gear(g);
            }
            m
        };
        let (mut cues, mut out) = (Vec::new(), Vec::new());
        // the control: the disc's, slot 3 and the quick-slot mode
        let mut m = menu(None);
        m.action(1, 0x0207_0000, 0, &inv, &info, &vui, &mut r, false, &mut cues, &mut out);
        assert!(out.is_empty() && m.focus == Focus::Slots && (m.menu, m.slot) == (-1, 3), "the disc's Prepare");
        // the live layout
        let mut m = menu(Some(gear_view([false, true, false, true])));
        m.action(1, 0x0207_0000, 0, &inv, &info, &vui, &mut r, false, &mut cues, &mut out);
        assert_eq!(out, vec![ItemOut::UseItem(0, 0)], "the sword not worn: put on first");
        assert!(m.focus == Focus::Gear && (m.menu, m.slot) == (-2, 1), "the first empty set, 1");
        let mut press = |m: &mut ItemMenu, k: i32, cues: &mut Vec<i32>, out: &mut Vec<ItemOut>| {
            m.key(&vui, &mut r, k, true, &inv, &info, false, cues, out);
        };
        cues.clear();
        out.clear();
        press(&mut m, 19, &mut cues, &mut out);
        press(&mut m, 19, &mut cues, &mut out);
        assert_eq!((m.slot, cues.as_slice()), (0, &[11, 9][..]), "up to 0, then its end");
        assert_eq!(m.slot_name, "Dirk", "the set's name shown");
        for _ in 0..4 {
            press(&mut m, 20, &mut cues, &mut out);
        }
        assert_eq!((m.slot, cues.last()), (3, Some(&9)), "down to 3, never bare hands");
        press(&mut m, 19, &mut cues, &mut out);
        assert_eq!(m.slot, 2);
        press(&mut m, 13, &mut cues, &mut out);
        assert!(m.focus == Focus::Edit && m.typing() == m.visible, "the name editor");
        assert_eq!(String::from_utf16_lossy(&m.edit.text), "Pick", "on the set's name");
        assert_eq!(String::from_utf16_lossy(&m.edit.title), "NAME CONFIGURATION");
        // Escape: nothing sent, back to the menu
        press(&mut m, 27, &mut cues, &mut out);
        assert!(out.is_empty() && m.focus == Focus::Menu, "cancelled");
        // again, the name typed over: Enter sends it
        m.action(1, 0x0207_0000, 1, &inv, &info, &vui, &mut crate::pointer::testing::NullRaster, false, &mut cues, &mut out);
        assert!(out.is_empty(), "the worn dirk: nothing put on");
        press(&mut m, 13, &mut cues, &mut out);
        for _ in 0..4 {
            press(&mut m, 8, &mut cues, &mut out);
        }
        for c in "Tank".chars() {
            press(&mut m, c as i32, &mut cues, &mut out);
        }
        press(&mut m, 13, &mut cues, &mut out);
        assert_eq!(out, vec![ItemOut::GearSet(1, "Tank".into())], "set 1, the first empty, named");
        assert!(m.focus == Focus::Menu && m.menu >= 0, "back to the menu");
        // a tunic: no set, refused
        out.clear();
        cues.clear();
        m.action(1, 0x0207_0000, 2, &inv, &info, &vui, &mut crate::pointer::testing::NullRaster, false, &mut cues, &mut out);
        assert!(out.is_empty() && m.focus == Focus::Menu && cues == [10], "refused with sound 10");
    }

    /// The live quick slots (its `ChangeSlot`): a gem moves over 2-3 and
    /// anything else over 0-1. The control: the disc's keeps a gem on 2.
    #[test]
    fn the_live_quick_slots_take_gems_on_2_and_3() {
        let vui = gear_vui();
        let inv = ItemList { p04: 0, items: vec![it(-1, 7, 0, 1), it(-1, 2, 0, 1)] };
        let words = vui.words.clone();
        let base = [0; 29];
        let info = Info { words: &words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let mut r = crate::pointer::testing::NullRaster;
        for live in [false, true] {
            let mut m = ItemMenu::default();
            m.set_built(build(&inv, [30, 30, 30, 40], &|_| -1, &|_| true));
            if live {
                m.set_gear(gear_view([true; 4]));
            }
            let (mut cues, mut out) = (Vec::new(), Vec::new());
            m.action(0, 0x0207_0000, 0, &inv, &info, &vui, &mut r, false, &mut cues, &mut out);
            assert_eq!(m.slot, 2, "a gem starts on 2");
            cues.clear();
            m.key(&vui, &mut r, 20, true, &inv, &info, false, &mut cues, &mut out);
            m.key(&vui, &mut r, 19, true, &inv, &info, false, &mut cues, &mut out);
            m.key(&vui, &mut r, 19, true, &inv, &info, false, &mut cues, &mut out);
            if live {
                assert_eq!((m.slot, cues.as_slice()), (2, &[11, 11, 9][..]), "live: down to 3, back to 2, not below");
            } else {
                assert_eq!((m.slot, cues.as_slice()), (2, &[9, 9, 9][..]), "the disc's: a gem stays on 2");
            }
            m.key(&vui, &mut r, 13, true, &inv, &info, false, &mut cues, &mut out);
            assert_eq!(out, vec![ItemOut::QuickSlot(0, 2)]);
            // a potion: 0-1 in both
            out.clear();
            cues.clear();
            m.action(0, 0x0207_0000, 1, &inv, &info, &vui, &mut r, false, &mut cues, &mut out);
            m.key(&vui, &mut r, 20, true, &inv, &info, false, &mut cues, &mut out);
            m.key(&vui, &mut r, 20, true, &inv, &info, false, &mut cues, &mut out);
            assert_eq!((m.slot, &cues[1..]), (1, &[11, 9][..]), "live {live}: a potion to 1, not 2");
        }
    }

    /// Records where whole surfaces are blitted (the sets' badges and icons).
    #[derive(Default)]
    struct Whole(Vec<(u32, [i32; 2])>);

    impl Layout for Whole {}

    impl Raster for Whole {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, id: u32, _: u32, at: [i32; 2], _: i32, _: f32) {
            self.0.push((id, at));
        }
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

    /// The weapon-set mode's column (`DrawGearSlots`): each of the five
    /// entries' badge, then its icon, whole, at (528, 68) and 32 apart, bare
    /// hands last; a pointer on a set's row steps to it and on the chosen
    /// one presses X, and the bare hands row takes none.
    #[test]
    fn the_sets_column_is_drawn_and_pointed_at() {
        let vui = gear_vui();
        let mut worn = it(14, 0, 0, 1);
        worn.equip_slot = 14;
        let inv = ItemList { p04: 0, items: vec![worn] };
        let words = vui.words.clone();
        let base = [0; 29];
        let info = Info { words: &words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let mut m = ItemMenu::default();
        m.set_built(build(&inv, [30, 30, 30, 40], &|_| -1, &|_| true));
        m.set_gear(gear_view([false; 4]));
        m.open(&vui, &crate::pointer::testing::NullRaster);
        let (mut cues, mut out) = (Vec::new(), Vec::new());
        m.action(1, 0x0207_0000, 0, &inv, &info, &vui, &mut crate::pointer::testing::NullRaster, false, &mut cues, &mut out);
        assert_eq!((m.menu, m.slot), (-2, 0), "every set prepared: the first");
        let mut r = Whole::default();
        m.draw(&vui, &mut r, &info, &mut cues);
        assert_eq!(r.0, vec![(21, [528, 68]), (11, [528, 68]), (12, [528, 132]), (15, [528, 196])], "set 0's badge then icon, set 2's icon, bare hands' icon");
        let row = |k: i32| [540.0, (68 + 32 * k + 10) as f32];
        assert_eq!(m.point(row(2)), vec![key::DOWN, key::DOWN]);
        assert_eq!(m.point(row(0)), vec![key::CROSS]);
        assert!(m.point(row(4)).is_empty(), "bare hands is not a set to name");
    }

}
