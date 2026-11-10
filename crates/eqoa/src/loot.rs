//! The corpse window (CLIENT `viwndloot.cpp`, `VIWndLoot`, the game
//! window's mode 1): the inventory (40 rows, or the pack's on a server that
//! holds more, at (5, 10)) and the corpse (42
//! rows at (144, 10): "LEAVE ALL" (word 119) or "Empty" (641) on top, the
//! items, "TAKE ALL" (118) after them) as two lists rebuilt every frame, and
//! the selected item's text in a window at (460, 152). Enter on the corpse
//! list: the top row closes (`LootNoWindow`), an item takes it all
//! (`LootOne`), the row after the items takes everything (`RequestLootAll`).
//!
//! The corpse itself is `VIClient::m_corpse`, read off 0x0018 (and the
//! MemoryDump); 0x0016 closes the window and forgets it. The server answers
//! a `LootOne` with 0x0019 (that item moved into the inventory, `Msg0019`)
//! and a `RequestLootAll` with 0x001A (every item, `Msg001A`), and a corpse
//! it will not open with 0x0085 (why, in the chat box): `npcw.rs`.
//!
//! A pointer (`pointer`) takes the corpse list's rows, as its keys do; the
//! inventory list beside it takes no keys, so no pointer either.
//!
//! The client's own: on a server that makes this player a raid's master looter
//! of the corpse (go-eqoa-server's 0x7E47), the choice of an item opens the
//! master looter's menu in place of taking it (`masterloot`: Take, Inspect,
//! Assign, Abandon); on any other corpse the window is the game's.

#![allow(clippy::too_many_arguments)]

use crate::infotext::Info;
use crate::wndlist::WndList;
use net::dump::ItemList;
use vi::ui::{Raster, Vui, Window, wide};

/// What the window asks of the client.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LootOut {
    /// `LootNoWindow`: 0x0016, no payload
    Close,
    /// `LootOne(i, n)`: 0x0019 (25), the corpse item's key and the count
    One(i32, i32),
    /// `RequestLootAll`: 0x001A (26)
    All,
    /// the master looter's Assign (`masterloot`): the item of that key to the
    /// player of that id (0x7E48 op 3)
    Assign { key: u32, id: u32 },
    /// the master looter's Abandon (0x7E48 op 4)
    Abandon,
}

pub struct Loot {
    inv: WndList,
    loot: WndList,
    desc: String,
    window: Window,
    pub visible: bool,
    /// the master looter's menu and its windows, while the server says this
    /// player is the corpse's master looter (`masterloot`)
    pub ml: crate::masterloot::MasterLooter,
}

impl Default for Loot {
    fn default() -> Self {
        // Init: the two lists and their rows
        let inv = WndList::with_rows([5, 10], 0, 40);
        let loot = WndList::with_rows([144, 10], 0, 42);
        Loot { inv, loot, desc: String::new(), window: Window::default(), visible: false, ml: Default::default() }
    }
}

/// The inventory list's rows (`OnDrawSelf`): a stack shows its count, a worn
/// item the arrows; as many rows as the pack holds (CLIENT's 40, or a larger
/// server's: `inventory::pack_cap`).
fn fill_inv(list: &mut WndList, inv: &ItemList, icon: &dyn Fn(usize) -> i32) {
    let rows = crate::inventory::pack_cap();
    if list.items.len() != rows {
        list.init(rows);
    }
    let n = inv.items.len().min(rows);
    for (i, it) in inv.items.iter().take(n).enumerate() {
        let mut flags = if it.quantity >= 2 { 5 } else { 1 };
        let mut arrows = -1;
        if it.equip_slot != -1 {
            arrows = 138;
            flags |= 8;
        }
        list.set_item(i, flags, icon(i), arrows, -1, it.quantity as i32);
    }
    for i in n..rows {
        list.set_item(i, 0, -1, -1, -1, 0);
    }
}

/// The corpse list's rows: "LEAVE ALL" (119) or "Empty" (641) on top, the
/// items, "TAKE ALL" (118) after them when there are any.
fn fill_loot(list: &mut WndList, corpse: &ItemList, icon: &dyn Fn(usize) -> i32) {
    let n = corpse.items.len().min(40);
    list.set_item(0, 2, -1, -1, if n > 0 { 119 } else { 641 }, 0);
    for (i, it) in corpse.items.iter().take(n).enumerate() {
        let flags = if it.quantity >= 2 { 5 } else { 1 };
        list.set_item(i + 1, flags, icon(i), -1, -1, it.quantity as i32);
    }
    if n > 0 {
        list.set_item(n + 1, 2, -1, -1, 118, 0);
    } else {
        list.set_item(n + 1, 0, -1, -1, -1, 0);
    }
    for i in n + 2..42 {
        list.set_item(i, 0, -1, -1, -1, 0);
    }
}

impl Loot {
    /// `Open`: the inventory unlit, the corpse lit, both from the top; the
    /// text window reopened; the corpse list takes the keys.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, now: i64) {
        self.inv.set_x64(false);
        self.inv.open(now);
        self.inv.reset_scroll();
        self.loot.set_x64(true);
        self.loot.open(now);
        self.loot.reset_scroll();
        self.desc.clear();
        self.window.init(vui, r, [460, 152], 340, 10, 1);
        if self.window.is_open() {
            self.window.close(true);
        }
        self.window.open(false);
        self.visible = true;
    }

    /// A pointer at frame point `p`: a row of the corpse list selected, the
    /// selection taken (Enter: take it, all, or close). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.ml.busy() {
            return self.ml.point(p);
        }
        self.loot.row_at(p).map(|row| self.loot.pick(row)).into_iter().collect()
    }

    /// The corpse list's keys (`VIWndList::OnKeyDown`, Enter as 256 to
    /// `VIWndLoot::OnMessage`) and the window's own (Escape); the master
    /// looter's windows' while one is up (`describe`: Inspect's text of an item,
    /// `now`: the session's clock).
    pub fn key(&mut self, key: i32, down: bool, corpse: &ItemList, describe: &dyn Fn(&net::dump::NetItem) -> String, now: i64, cues: &mut Vec<i32>) -> Option<LootOut> {
        if self.ml.busy() {
            return self.ml.key(key, down, corpse, describe, now, cues).map(|o| match o {
                crate::masterloot::MlOut::Take(k, n) => LootOut::One(k, n),
                crate::masterloot::MlOut::Assign { key, id } => LootOut::Assign { key, id },
                crate::masterloot::MlOut::Abandon => LootOut::Abandon,
            });
        }
        if !down {
            self.loot.key_up_down(key, false);
            return None;
        }
        match key {
            27 | 132 => {
                cues.push(8);
                Some(LootOut::Close)
            }
            13 | 134 => {
                cues.push(13);
                let p1 = self.loot.sel;
                let n = corpse.items.len() as i32;
                if p1 == 0 {
                    Some(LootOut::Close)
                } else if p1 < n + 1 && self.ml.on() {
                    // the master looter's: its menu for the item
                    self.ml.open((p1 - 1) as usize);
                    None
                } else if p1 < n + 1 {
                    let it = &corpse.items[(p1 - 1) as usize];
                    Some(LootOut::One(it.inv_index as i32, it.quantity as i32))
                } else if p1 == n + 1 {
                    Some(LootOut::All)
                } else {
                    None
                }
            }
            _ => {
                self.loot.key_up_down(key, true);
                None
            }
        }
    }

    /// `OnDrawSelf` with `UpdateDesc`, then the two lists (the window's
    /// children draw after it).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, info: &Info, corpse: &ItemList, inv_icon: &dyn Fn(usize) -> i32, loot_icon: &dyn Fn(usize) -> i32, now: i64, cues: &mut Vec<i32>) {
        fill_inv(&mut self.inv, info.inv, inv_icon);
        fill_loot(&mut self.loot, corpse, loot_icon);
        // UpdateDesc
        self.desc.clear();
        let i = self.loot.sel - 1;
        if !corpse.items.is_empty() && i >= 0 && (i as usize) < corpse.items.len() {
            self.desc = info.loot_item(&corpse.items[i as usize]);
        }
        if self.desc.is_empty() {
            self.window.close(true);
        } else {
            let text = wide(&self.desc);
            let rows = self.window.count_lines(vui, r, &text, self.window.size[0] - 40);
            self.window.init(vui, r, [460, 152], 340, rows, 1);
            self.window.open(false);
            self.window.draw(vui, r);
            let width = self.window.size[0];
            self.window.text_box(vui, r, &text, 20, 0, width - 40, rows, [1.0, 1.0, 1.0, self.window.open], 3);
        }
        self.inv.draw(vui, r, now, cues);
        self.loot.draw(vui, r, now, cues);
        self.ml.draw(vui, r, corpse, now, cues);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::dump::NetItem;

    fn corpse(n: usize) -> ItemList {
        let items = (0..n).map(|k| NetItem { quantity: 1 + k as i64, inv_index: 10 + k as u32, equip_slot: -1, quick_slot: -1, ..Default::default() }).collect();
        ItemList { p04: 0, items }
    }

    #[test]
    fn the_corpse_rows() {
        let mut l = WndList::default();
        l.init(42);
        fill_loot(&mut l, &corpse(2), &|k| 100 + k as i32);
        let rows: Vec<_> = l.items[..5].iter().map(|r| (r.flags, r.word, r.number)).collect();
        assert_eq!(rows, vec![(2, 119, 0), (1, -1, 1), (5, -1, 2), (2, 118, 0), (0, -1, 0)]);
        assert_eq!(l.items[1].icon, 100);
        fill_loot(&mut l, &corpse(0), &|_| 0);
        assert_eq!((l.items[0].word, l.items[1].flags), (641, 0), "empty: one row, no take-all");
    }

    #[test]
    fn enter_on_each_row() {
        let mut w = Loot::default();
        let c = corpse(2);
        let mut cues = Vec::new();
        let mut at = |sel: i32| {
            w.loot.sel = sel;
            w.key(134, true, &c, &|_| String::new(), 0, &mut cues)
        };
        assert_eq!(at(0), Some(LootOut::Close));
        assert_eq!(at(1), Some(LootOut::One(10, 1)), "the key, not the row");
        assert_eq!(at(2), Some(LootOut::One(11, 2)));
        assert_eq!(at(3), Some(LootOut::All));
        assert_eq!(at(4), None);
        assert_eq!(w.key(27, true, &c, &|_| String::new(), 0, &mut cues), Some(LootOut::Close));
        assert_eq!(cues, vec![13, 13, 13, 13, 13, 8]);
    }

    /// The master looter's corpse (0x7E47): ✕ on an item opens its menu in
    /// place of taking it, and the menu's Take is the game's take of it, its
    /// Abandon asked first; the server's word that it is over puts the game's
    /// window back (the control: every other corpse takes at once).
    #[test]
    fn a_master_looters_item_opens_its_menu() {
        let mut w = Loot::default();
        let c = corpse(2);
        let mut cues = Vec::new();
        let none = |_: &net::dump::NetItem| String::new();
        w.loot.sel = 2;
        assert_eq!(w.key(134, true, &c, &none, 0, &mut cues), Some(LootOut::One(11, 2)), "the control");
        w.ml.set(Some(net::raid::MasterLoot { seconds: 300, members: vec![(5, "Bo".into())] }));
        assert_eq!(w.key(134, true, &c, &none, 0, &mut cues), None);
        assert!(w.ml.busy(), "the menu, the item kept");
        assert_eq!(w.key(134, true, &c, &none, 0, &mut cues), Some(LootOut::One(11, 2)), "Take: the game's");
        let outs: Vec<LootOut> = [134, 139, 139, 139, 134, 136, 134].iter().filter_map(|&k| w.key(k, true, &c, &none, 0, &mut cues)).collect();
        assert_eq!(outs, [LootOut::Abandon], "the menu again, Abandon, the box's Yes");
        assert!(!w.ml.busy());
        w.ml.set(None);
        assert_eq!(w.key(134, true, &c, &none, 0, &mut cues), Some(LootOut::One(11, 2)), "over: the game's window again");
    }

    /// A click selects a row of the corpse list where it was drawn, a second
    /// takes it (Enter); the inventory list beside it takes no keys and no
    /// pointer, and a row past the corpse's is not there.
    #[test]
    fn a_pointer_on_the_corpse() {
        use crate::pointer::testing::{NullRaster, vui};
        use crate::pointer::{MOVED, key};
        let (vui, mut r) = (vui(), NullRaster);
        let (inv, c) = (corpse(3), corpse(2));
        let base = [0; 29];
        let info = Info { words: &vui.words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let mut w = Loot::default();
        w.open(&vui, &r, 0);
        let mut cues = Vec::new();
        w.draw(&vui, &mut r, &info, &c, &|_| 1, &|_| 1, 0, &mut cues);
        // the corpse list at (144, 10), the inventory's at (5, 10): rows 44
        // high (no atlas here, so no studs before them)
        let row = |x: f32, i: i32| [x + 57.0, 10.0 + 44.0 * i as f32 + 22.0];
        assert_eq!(w.point(row(144.0, 2)), vec![MOVED]);
        assert_eq!(w.loot.sel, 2);
        assert_eq!(w.point(row(144.0, 2)), vec![key::CROSS], "the selection: Enter");
        assert_eq!(w.key(134, true, &c, &|_| String::new(), 0, &mut cues), Some(LootOut::One(11, 2)), "the row the pointer chose");
        assert!(w.point(row(144.0, 4)).is_empty(), "past \"done\": no row");
        assert!(w.point(row(5.0, 0)).is_empty(), "the inventory list takes no pointer");
        assert_eq!(w.loot.sel, 2);
    }
}
