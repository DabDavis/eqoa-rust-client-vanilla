//! The blacksmith (CLIENT `viwndblacksmith.cpp`, `VIWndBlacksmith`, the game
//! window's mode 19): the inventory in a list at (16, 37) beside a panel with
//! the highlighted item's description, its repair cost against the player's
//! coin, and up to six stat lines coloured by how the item compares with what
//! is worn. X on an item that can be worn (or used) repairs it (0x0062);
//! anything else gets sound 10. A worn-down item (under a quarter of its hit
//! points) carries the list's "X" mark.
//!
//! The server opens it with 0x0061 (the smith's id, kept for the repairs) and
//! only in play; triangle closes it (0x0063 out), and so does 0x0063 in.

use crate::infotext::{self, Info};
use crate::wndlist::WndList;
use net::dump::NetItem;
use vi::fpu::{Ee, F};
use vi::ui::{FrameParam, Raster, UI_COLORS, Vui, Window, wide};

/// `GetInvItemCondition`: the hit points over their most, 1 with none.
pub fn condition(it: &NetItem) -> F<Ee> {
    if it.item.max_hp == 0 {
        return F::ONE;
    }
    F::<Ee>::from_int(it.cur_hp as i32) / F::from_int(it.item.max_hp as i32)
}

/// `GetInvItemRepairCost`: the value times the condition lost, truncated.
pub fn repair_cost(it: &NetItem) -> i32 {
    (F::<Ee>::from_int(it.item.value as i32) * (F::ONE - condition(it))).0 as i32
}

/// `IsInvItemUsable`: something worn, or with `m_p54` set.
pub fn usable(it: &NetItem) -> bool {
    it.item.wear_slot != -1 || it.item.p54 != 0
}

/// What the blacksmith asks of the game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SmithOut {
    /// 260 to the game window: `Send0063`, then `Msg0063`
    Close,
    /// `Send0062(i)`: the inventory item to repair
    Repair(i32),
}

/// One of the six stat lines and its colour.
#[derive(Clone, Debug, Default)]
struct Line {
    text: String,
    color: [f32; 4],
}

pub struct Blacksmith {
    list: WndList,
    window: Window,
    text: String,
    /// -1: nothing to price
    cost: i32,
    money: i32,
    lines: Vec<Line>,
    pub visible: bool,
}

impl Default for Blacksmith {
    fn default() -> Self {
        // Init: the list at (16, 37), forty rows, the frame style
        Blacksmith { list: WndList::with_rows([16, 37], 0, 40), window: Window::default(), text: String::new(), cost: 0, money: 0, lines: Vec::new(), visible: false }
    }
}

/// What the blacksmith reads off the client.
pub struct View<'a> {
    pub info: &'a Info<'a>,
    /// `m_player.m_p34`
    pub coin: i32,
    pub inv_icon: &'a dyn Fn(&NetItem) -> i32,
}

impl Blacksmith {
    /// Mode 19 (`UpdateMode`): shown and `Open` (the selection shown, the
    /// list back to the top), then the focus (295): the text window placed.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster) {
        self.visible = true;
        self.list.set_x64(true);
        self.list.reset_scroll();
        self.window.init(vui, r, [403, 180], 490, 15, 0);
        self.window.open(true);
    }

    /// `OnKeyDown` / `OnKeyUp`: triangle closes; X and the arrows go to the
    /// list, whose X (256) repairs the selected item when it can be worn.
    pub fn key(&mut self, view: &View, key: i32, down: bool, cues: &mut Vec<i32>, out: &mut Vec<SmithOut>) {
        if !down {
            if matches!(key, 19 | 20 | 138 | 139) {
                self.list.key_up_down(key, false);
            }
            return;
        }
        match key {
            27 | 132 => {
                cues.push(8);
                out.push(SmithOut::Close);
            }
            19 | 20 | 138 | 139 => self.list.key_up_down(key, true),
            13 | 134 => {
                // 256 with the list's selection
                let sel = self.list.sel;
                match view.info.inv.items.get(sel.max(0) as usize).filter(|_| sel >= 0) {
                    Some(it) if usable(it) => out.push(SmithOut::Repair(sel)),
                    _ => cues.push(10),
                }
            }
            _ => {}
        }
    }

    /// `Refresh`: the selected item's description, cost and (for something
    /// worn) stat lines.
    fn refresh(&mut self, view: &View) {
        self.text.clear();
        self.lines.clear();
        let inv = &view.info.inv.items;
        self.money = view.coin;
        if inv.is_empty() {
            self.cost = -1;
            return;
        }
        let item = self.list.sel;
        self.text = view.info.inv_item(item, 0);
        let Some(it) = inv.get(item.max(0) as usize).filter(|_| item >= 0) else {
            self.cost = -1;
            return;
        };
        self.cost = repair_cost(it);
        if it.item.wear_slot != -1 {
            self.show_stats(view, item as usize, it);
        }
    }

    /// `ShowStats`: a line per stat bonus, blue when the item would raise the
    /// stat over what is worn now, red when it would lower it.
    fn show_stats(&mut self, view: &View, item: usize, it: &NetItem) {
        for st in it.item.stats.iter().take(6) {
            let now = view.info.stat_total(st.id);
            let then = view.info.stat_with_item(item, st.id);
            let text = match infotext::stat_word(st.id) {
                Some(w) => format!("{}: {} |}} {}", view.info.words.get(w, 0), st.value, then),
                None => format!("{}: {} ", st.id, st.value),
            };
            let color = if now < then {
                [10.0 / 255.0, 50.0 / 255.0, 70.0 / 255.0, 1.0]
            } else if then < now {
                [95.0 / 255.0, 25.0 / 255.0, 25.0 / 255.0, 1.0]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            };
            self.lines.push(Line { text, color });
        }
    }

    /// `OnDrawSelf` (Refresh first; the banner, the panels, the text; the
    /// rows from the inventory), then the list.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, view: &View, now: i64, cues: &mut Vec<i32>) {
        if !self.visible {
            return;
        }
        self.refresh(view);
        let black = UI_COLORS[0];
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        vui.blit_rect(r, 32, [-1, -1], [642, 332], 0, 1.0);
        let banner = wide(&word(609));
        let w = vui.string_width(r, &banner) + 64;
        vui.blit_bar(r, &banner, [1.0; 4], [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
        let fp = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
        vui.blit_fill_frame(r, [165, 48], [291, 267], &fp, 35, 0, 1.0, true);
        vui.blit_fill_frame(r, [456, 48], [184, 267], &fp, 35, 0, 1.0, true);
        vui.blit_studded_horz(r, [154, 37], 37, 38, 487, 4, 0, 1.0, 0);
        vui.blit_studded_horz(r, [154, 313], 40, 41, 487, 4, 0, 1.0, 0);
        vui.blit_horz(r, [16, 322], 60, 138, 0, 1.0, 0);
        self.window.text_box(vui, r, &wide(&self.text), 20, 0, 270, 15, black, 2);
        if self.cost != -1 {
            self.window.text(vui, r, &wide(&format!("{}:", word(523))), 310, 0, black, 0);
            self.window.text_box(vui, r, &wide(&format!("{} {}", self.cost, word(479))), 310, 1, 160, 1, black, 2);
        }
        let priced = self.cost != -1;
        self.window.text(vui, r, &wide(&format!("{}:", word(506))), 310, if priced { 2 } else { 0 }, black, 0);
        self.window.text_box(vui, r, &wide(&format!("{} {}", self.money, word(479))), 310, if priced { 3 } else { 1 }, 160, 1, black, 2);
        let mut row = if priced { 4 } else { 2 };
        for l in self.lines.iter().take(6) {
            if row >= 13 {
                break;
            }
            if l.text.is_empty() {
                continue;
            }
            let text = wide(&l.text);
            let mut n = self.window.count_lines(vui, r, &text, 160);
            if row == 12 {
                n = n.min(1);
            }
            self.window.text_box(vui, r, &text, 310, row, 160, n, l.color, 2);
            row += n;
        }
        self.window.text(vui, r, &wide(&format!("|x{}", word(523))), 310, 13, black, 0);
        self.window.text(vui, r, &wide(&format!("|t{}", word(161))), 310, 14, black, 0);
        // the rows: a stack's count; something that cannot be worn barred
        // (arrows 140), a worn-down item marked
        let inv = &view.info.inv.items;
        let n = inv.len().min(40);
        for (i, it) in inv.iter().take(n).enumerate() {
            let mut flags = if it.quantity >= 2 { 5 } else { 1 };
            let mut color = -1;
            let mut x = -1;
            if !usable(it) {
                color = 140;
                flags |= 8;
            } else if condition(it) < F::new(0.25) {
                x = 1;
                flags |= 0x20;
            }
            self.list.set_item(i, flags, (view.inv_icon)(it), color, x, it.quantity as i32);
        }
        for i in n..40 {
            self.list.set_item(i, 0, -1, -1, -1, 0);
        }
        self.list.draw(vui, r, now, cues);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(value: i64, hp: i64, max: i64, wear: i64) -> NetItem {
        let mut n = NetItem { quantity: 1, cur_hp: hp, equip_slot: -1, quick_slot: -1, ..Default::default() };
        n.item.value = value;
        n.item.max_hp = max;
        n.item.wear_slot = wear;
        n
    }

    /// The cost is the value times the condition lost, truncated; no hit
    /// points is whole (the control: a whole item costs nothing).
    #[test]
    fn repair_cost_is_the_condition_lost() {
        assert_eq!(repair_cost(&item(100, 50, 50, 1)), 0, "whole");
        assert_eq!(repair_cost(&item(100, 25, 50, 1)), 50);
        assert_eq!(repair_cost(&item(100, 0, 50, 1)), 100, "broken");
        assert_eq!(repair_cost(&item(10, 1, 3, 1)), 6, "10 * 0.6667 truncates to 6");
        assert_eq!(repair_cost(&item(100, 0, 0, 1)), 0, "no hit points: whole");
        assert!(condition(&item(100, 12, 50, 1)) < F::new(0.25));
        assert!(!(condition(&item(100, 13, 50, 1)) < F::new(0.25)));
    }

    /// Only something worn (or flagged usable) can be repaired.
    #[test]
    fn usable_is_worn_or_flagged() {
        assert!(usable(&item(1, 1, 1, 3)));
        let mut it = item(1, 1, 1, -1);
        assert!(!usable(&it));
        it.item.p54 = 1;
        assert!(usable(&it));
    }
}
