//! Trading between two players (CLIENT `viwndtrade.cpp`,
//! `viwndtradelist.cpp`, `viwnddeposit.cpp`, and `VIClient`'s trade half;
//! the game window's modes 15, trading, and 16, being asked).
//!
//! `TradeState` is `VIClient`'s: the partner, the copy of the inventory the
//! player offers from (`m_tradeInv`), both offers of up to four items with
//! their running ids, and both sums of money. The server echoes every
//! change to both sides over 0x0056: the initiator's own changes come back
//! as sub-opcodes 3/4/5 to itself and the other's as 8/9/10 (`TradeA`),
//! and the responder reads the same numbers the other way round
//! (`TradeB`).
//!
//! `Trade` is `VIWndTrade`: the trade copy of the inventory (a list at (6,
//! 37)), the player's offer and the partner's (three-row lists at (155, 37)
//! and (295, 37)), both sums, the text of the item under the cursor, the
//! accept glow, the button legend; with the money entry (`VIWndDeposit`),
//! the quantity picker, the message box and a chat line.
//!
//! A pointer (`pointer`) goes to the child with the keys; on the window
//! itself a click on another list (or the money) moves the focus there as
//! the d-pad does and selects the row under it, and a click on the list
//! with the focus selects a row or uses the selection.

#![allow(clippy::too_many_arguments)]

use crate::dialog::{BoxAnswer, Key as BoxKey, MBox};
use crate::editbox::{EditBox, EditUp};
use crate::infotext::Info;
use crate::pointer::{Hits, MOVED, key};
use crate::quantity::{Quantity, QuantityUp};
use crate::startmenu::KeyFlash;
use crate::wndlist::WndList;
use net::dump::{ItemList, NetItem};
use vi::ui::{Raster, Vui, Window, wide};

/// `VITradeItem`: an offered item and its running id.
#[derive(Clone, Debug, Default)]
pub struct Offer {
    pub item: NetItem,
    pub id: i32,
}

/// `VIClient`'s trade fields.
#[derive(Clone, Debug)]
pub struct TradeState {
    /// `m_tradePartner`: the id every 0x0056 names
    pub partner: u32,
    /// `m_p2BB00`: the partner's entity; `u32::MAX` is no trade
    pub with: u32,
    /// `m_p2BB04`: the player asked (the initiator)
    pub asked: bool,
    /// `m_tradeInv`
    pub inv: ItemList,
    /// `m_tradeB` / `m_tradeBCount`: the player's offer (side 1)
    pub mine: Vec<Offer>,
    /// `m_tradeA` / `m_tradeACount`: the partner's offer (side 0)
    pub theirs: Vec<Offer>,
    /// `m_p2BAEC`: the coin the player still carries
    pub carried: i32,
    /// `m_p2BAF0`, `m_p2BAF4`: the player's money in the trade, the partner's
    pub my_money: i32,
    pub their_money: i32,
    /// `m_p2BAF8`, `m_p2BAFC`: the next running ids, the player's and the
    /// partner's
    next_mine: i32,
    next_theirs: i32,
}

impl Default for TradeState {
    fn default() -> Self {
        TradeState { partner: u32::MAX, with: u32::MAX, asked: false, inv: ItemList::default(), mine: Vec::new(), theirs: Vec::new(), carried: 0, my_money: 0, their_money: 0, next_mine: 0, next_theirs: 0 }
    }
}

/// What a 0x0056 sub-opcode does, once read (`TradeA`/`TradeB` and the
/// dispatcher's own cases).
#[derive(Clone, Debug, PartialEq)]
pub enum TradeMsg {
    /// 2: the partner said yes (`Trade02`)
    Accepted(u32),
    /// 1: cancelled (`Trade01`)
    Cancelled,
    /// 20: the partner is busy (`Trade20`)
    Busy,
    /// 16: the trade is over (`Trade16`)
    Done,
    /// 17, 21, 22: refused, with the word (733, 734, 735)
    Refused(i32),
    /// 7: the accepts cleared (`Trade07`)
    Unaccepted,
    /// 6: the partner accepted (`Trade06`)
    TheyAccepted,
    /// the offers or the money changed
    Changed,
}

impl TradeState {
    /// `TradeSetup`: the inventory copied, the offers emptied, the purse.
    pub fn setup(&mut self, inv: &ItemList, coin: i32) {
        // CopyFrom: the records, and the counter bumped once a record
        let mut copy = inv.clone();
        copy.p04 = copy.p04.wrapping_add(inv.items.len() as u32);
        self.inv = copy;
        self.mine.clear();
        self.theirs.clear();
        self.carried = coin;
        self.my_money = 0;
        self.their_money = 0;
        self.next_mine = 0;
        self.next_theirs = 0;
    }

    /// `TradeReset`.
    pub fn reset(&mut self) {
        self.theirs.clear();
        self.carried = 0;
        self.my_money = 0;
        self.their_money = 0;
        self.next_mine = 0;
        self.next_theirs = 0;
        self.with = u32::MAX;
        self.asked = false;
        self.partner = u32::MAX;
        self.mine.clear();
    }

    /// `TradeAddItem`: onto the partner's entry from the same inventory
    /// slot (the last one found), else a new entry while there are fewer
    /// than four.
    pub fn add_item(&mut self, item: NetItem) {
        match self.theirs.iter().rposition(|o| o.item.inv_index == item.inv_index) {
            Some(at) => self.theirs[at].item.quantity += item.quantity,
            None if self.theirs.len() < 4 => {
                self.theirs.push(Offer { item, id: self.next_theirs });
                self.next_theirs += 1;
            }
            None => {}
        }
    }

    /// `TradeRemoveItem`: the partner's entry with running id `id` out.
    pub fn remove_item(&mut self, id: i32) {
        if let Some(at) = self.theirs.iter().position(|o| o.id == id) {
            self.theirs.remove(at);
        }
    }

    /// `TradeRemoveSlot`: the player's entry with running id `id` back into
    /// the trade copy (stacked as the item allows), and out.
    pub fn remove_slot(&mut self, id: i32) {
        if let Some(at) = self.mine.iter().position(|o| o.id == id) {
            let it = self.mine[at].item.clone();
            crate::infotext::merge(&mut self.inv, &it, it.item.max_stack, 40);
            self.mine.remove(at);
        }
    }

    /// `TradeAddSlot`: `n` of the trade copy's item with key `id` onto the
    /// player's offer (the entry holding that slot, or a new one while
    /// there are fewer than four), and taken from the copy.
    pub fn add_slot(&mut self, id: u32, n: i32) {
        let Some(slot) = crate::inventory::find(&self.inv, id) else { return };
        let it = self.inv.items[slot].clone();
        if it.quantity < n as i64 {
            return;
        }
        match self.mine.iter().position(|o| o.item.inv_index == it.inv_index) {
            Some(at) => self.mine[at].item.quantity += n as i64,
            None => {
                if self.mine.len() == 4 {
                    return;
                }
                let mut item = it;
                item.quantity = n as i64;
                self.mine.push(Offer { item, id: self.next_mine });
                self.next_mine += 1;
            }
        }
        crate::inventory::took(&mut self.inv, slot, n as i64);
    }

    /// `TradeSetMoney`: the player's money in the trade, and what is left.
    pub fn set_money(&mut self, money: i32, coin: i32) {
        self.my_money = money;
        self.carried = coin - money;
    }

    /// `TradeA` (the initiator) / `TradeB` (the responder): sub-opcodes 3-5
    /// and 8-10.
    pub fn recv(&mut self, sub: u8, r: &mut net::stream::Reader, coin: i32) -> bool {
        // TradeB reads the same handlers numbered the other way round
        let sub = match (self.asked, sub) {
            (true, s) => s,
            (false, s @ 3..=5) => s + 5,
            (false, s @ 8..=10) => s - 5,
            (false, s) => s,
        };
        match sub {
            3 => {
                let a = r.u32();
                let b = r.signed() as i32;
                self.add_slot(a, b);
            }
            4 => {
                let a = r.u32() as i32;
                self.remove_slot(a);
            }
            5 => {
                let c = r.signed() as i32;
                self.set_money(c, coin);
            }
            8 => self.add_item(net::dump::read_net_item(r)),
            9 => {
                let a = r.u32() as i32;
                self.remove_item(a);
            }
            10 => self.their_money = r.signed() as i32,
            _ => return false,
        }
        true
    }

    /// The 0x0056 dispatcher: the sub-opcode and the id, dropped unless the
    /// id is the partner's (but for 1, 2 and 20).
    pub fn message(&mut self, data: &[u8], coin: i32) -> Option<TradeMsg> {
        let mut r = net::stream::Reader::new(data);
        let sub = r.u8();
        let id = r.u32();
        if id != self.partner && sub != 2 && sub != 20 && sub != 1 {
            return None;
        }
        Some(match sub {
            2 => TradeMsg::Accepted(id),
            1 => TradeMsg::Cancelled,
            20 => TradeMsg::Busy,
            16 => TradeMsg::Done,
            17 => TradeMsg::Refused(733),
            21 => TradeMsg::Refused(734),
            22 => TradeMsg::Refused(735),
            7 => TradeMsg::Unaccepted,
            6 => TradeMsg::TheyAccepted,
            _ => {
                if !self.recv(sub, &mut r, coin) {
                    return None;
                }
                TradeMsg::Changed
            }
        })
    }

    /// `SendOpByteInt(86, sub, partner)`.
    pub fn send_sub(&self, sub: u8) -> Vec<u8> {
        let mut w = net::stream::Writer::op(0x0056);
        w.u8(sub).u32(self.partner);
        w.data
    }

    /// `TradeSend3(i, n)`: `n` of trade-copy item `i` offered, by its key.
    pub fn send_add(&self, i: usize, n: i32) -> Option<Vec<u8>> {
        let it = self.inv.items.get(i)?;
        let mut w = net::stream::Writer::op(0x0056);
        w.u8(3).u32(self.partner).u32(it.inv_index).signed(n as i64);
        Some(w.data)
    }

    /// `TradeSend4(i)`: the player's offered entry `i` taken back, by its
    /// running id.
    pub fn send_remove(&self, i: usize) -> Option<Vec<u8>> {
        let o = self.mine.get(i)?;
        let mut w = net::stream::Writer::op(0x0056);
        w.u8(4).u32(self.partner).u32(o.id as u32);
        Some(w.data)
    }

    /// `TradeSend5(x)`: the money offered.
    pub fn send_money(&self, x: i32) -> Vec<u8> {
        let mut w = net::stream::Writer::op(0x0056);
        w.u8(5).u32(self.partner).signed(x as i64);
        w.data
    }
}

/// What the trade window asks of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum TradeOut {
    /// `TradeCancel`: 0x0056 sub 1, then `Trade16`
    Cancel,
    /// 268 with -1 from the ask: `TradeSetup`, `TradeSend2`, `Start`, the
    /// chat box's mode 1
    Agree,
    /// `TradeSend3(i, n)`
    Add(usize, i32),
    /// `TradeSend4(i)`
    Remove(usize),
    /// `TradeSend5(x)`
    Money(i32),
    /// `TradeSend6`: accept
    Accept,
    /// `TradeSend7`: take the accept back
    Unaccept,
    /// `ChatLine`: the chat line's text
    Chat(String),
}

/// `VIWndDeposit`: a money amount, nine digits, the one under the cursor
/// changed with up and down (repeating) and moved with left and right.
/// Mode 3 is the trade's (word 473, the coin carried); 1 and 2 are the
/// bank's; 4 is a duel's wager (the live client's, `duelwnd`: word 1867,
/// the coin carried, taken as mode 3 takes it).
#[derive(Default)]
pub struct Deposit {
    pub cursor: i32,
    pub carried: i32,
    pub banked: i32,
    pub mode: i32,
    pub width: i32,
    pub digits: [i32; 9],
    window: Window,
    flash: KeyFlash,
    pub visible: bool,
    /// each digit drawn (its index), the room above it (10 + index) and
    /// below it (20 + index), for a pointer
    hits: Hits,
}

/// What the money entry tells its parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DepositUp {
    /// 260
    Cancel,
    /// 294: the amount (mode 3)
    Value(i32),
    /// the bank's: `MoveMoney(dir, v)` (modes 1, 2)
    Bank(i32, i32),
}

impl Deposit {
    /// `GetValue`.
    pub fn value(&self) -> i32 {
        (0..9).map(|i| self.digits[i] * 10i32.pow(8 - i as u32)).sum()
    }

    /// A pointer at frame point `p`: the cursor to the digit under it, and
    /// above or below it the digit changed (up, down). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let Some(h) = self.hits.at(p) else { return Vec::new() };
        let mut keys = Vec::new();
        if h % 10 != self.cursor {
            self.cursor = h % 10;
            keys.push(MOVED);
        }
        match h / 10 {
            1 => keys.push(key::UP),
            2 => keys.push(key::DOWN),
            _ => {}
        }
        keys
    }

    fn limit(&self) -> i32 {
        if self.mode == 2 { self.banked } else { self.carried }
    }

    /// `SetMax`.
    fn set_max(&mut self) {
        let mut v = self.limit();
        self.digits = [0; 9];
        for i in (0..9).rev() {
            let p = 10i32.pow(i);
            self.digits[8 - i as usize] = v / p;
            v -= self.digits[8 - i as usize] * p;
        }
    }

    /// `Setup(mode)`: as many digits as the limit needs.
    pub fn setup(&mut self, mode: i32) {
        self.mode = mode;
        let v = self.limit();
        self.width = 1 + (1..9).filter(|&i| v / 10i32.pow(i) > 0).count() as i32;
    }

    /// The trade's `Select` on the money: carried and banked set, the
    /// digits cleared, the cursor on the last, mode 3, and the focus (295).
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, carried: i32, banked: i32) {
        self.open_mode(vui, r, carried, banked, 3);
    }

    /// `open` in `mode` (the duel's wager's is 4, `Setup(4)`).
    pub fn open_mode(&mut self, vui: &Vui, r: &dyn Raster, carried: i32, banked: i32, mode: i32) {
        self.carried = carried;
        self.banked = banked;
        self.digits = [0; 9];
        self.cursor = 8;
        self.setup(mode);
        self.visible = true;
        self.window.init(vui, r, [320, 224], 300, 7, 1);
        self.window.close(true);
        self.window.open(false);
        self.flash.reset();
    }

    /// The focus (295): the window at (320, 224) closed and opening, the
    /// repeat reset; shown.
    pub fn focus(&mut self, vui: &Vui, r: &dyn Raster) {
        self.visible = true;
        self.window.init(vui, r, [320, 224], 300, 7, 1);
        self.window.close(true);
        self.window.open(false);
        self.flash.reset();
    }

    /// `MoveCursor(d)`.
    fn move_cursor(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        let c = self.cursor + d;
        if c < 9 - self.width {
            cues.push(9);
            self.cursor = 9 - self.width;
        } else if c >= 9 {
            cues.push(9);
            self.cursor = 8;
        } else {
            self.cursor = c;
            cues.push(11);
        }
        self.window.reset_tick_pulse(now);
    }

    /// `ChangeDigit(d)`: wrapping; past the limit, snapped to it.
    fn change_digit(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        let c = self.cursor as usize;
        self.digits[c] = (self.digits[c] + d).rem_euclid(10);
        if self.limit() < self.value() {
            cues.push(9);
            self.set_max();
        } else {
            cues.push(11);
        }
        self.window.reset_tick_pulse(now);
    }

    /// `OnKeyDown` / `OnKeyUp`.
    pub fn key(&mut self, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<DepositUp> {
        if !down {
            match key {
                19 | 138 => self.flash.release(138),
                20 | 139 => self.flash.release(139),
                _ => {}
            }
            return None;
        }
        match key {
            27 | 132 => {
                cues.push(8);
                Some(DepositUp::Cancel)
            }
            13 | 134 => {
                let v = self.value();
                if v == 0 {
                    cues.push(3);
                    Some(DepositUp::Cancel)
                } else if self.mode == 1 && v <= self.carried {
                    Some(DepositUp::Bank(0, v))
                } else if self.mode == 2 && v <= self.banked {
                    Some(DepositUp::Bank(1, v))
                } else if (self.mode == 3 || self.mode == 4) && v <= self.carried {
                    cues.push(13);
                    Some(DepositUp::Value(v))
                } else {
                    None
                }
            }
            19 | 138 => {
                self.flash.press(138);
                None
            }
            20 | 139 => {
                self.flash.press(139);
                None
            }
            17 | 136 => {
                self.move_cursor(-1, now, cues);
                None
            }
            18 | 137 => {
                self.move_cursor(1, now, cues);
                None
            }
            _ => None,
        }
    }

    /// `OnDrawSelf`, then the repeating keys.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, cues: &mut Vec<i32>) {
        self.window.init(vui, r, [320, 180], 320, 7, 1);
        self.window.draw(vui, r);
        let (white, yellow) = ([1.0, 1.0, 1.0, self.window.open], [1.0, 1.0, 0.0, self.window.open]);
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        let title = match self.mode {
            1 => 351,
            2 => 352,
            3 => 473,
            4 => 1867,
            _ => -1,
        };
        if title >= 0 {
            let s = wide(&crate::duel::word(&vui.words, title, vui.language));
            let w = vui.string_width(r, &s) + 20;
            self.window.text_box(vui, r, &s, 20, 1, w, 1, white, 0);
        }
        let half = self.window.size[0] / 2 - 10;
        let mut x = half;
        let w = vui.string_width(r, &wide("W")) - 6;
        self.hits.clear();
        for i in 9 - self.width..9 {
            let lit = i == self.cursor;
            for (row, what) in [(1, 0), (0, 10), (2, 20)] {
                let rr = crate::pointer::rows_rect(&self.window, vui, r, x, row, w, 1);
                self.hits.push([rr[0] + 20, rr[1], rr[2], rr[3]], what + i);
            }
            self.window.text_box(vui, r, &wide(&self.digits[i as usize].to_string()), x, 1, w, 1, if lit { yellow } else { white }, 0);
            if lit {
                self.window.tick_d(vui, r, x + w / 2, 0, true);
                self.window.tick_u(vui, r, x + w / 2, 2, true);
            }
            x += w;
        }
        let mut buf = Vec::new();
        if self.mode == 1 || self.mode == 3 || self.mode == 4 {
            let s = word(479);
            let sw = vui.string_width(r, &s) + 20;
            self.window.text_box(vui, r, &s, 20, 4, sw, 1, white, 0);
            buf = wide(&self.carried.to_string());
        } else if self.mode == 2 {
            let s = word(300);
            let sw = vui.string_width(r, &s) + 20;
            self.window.text_box(vui, r, &s, 20, 4, sw, 1, white, 0);
            buf = wide(&self.banked.to_string());
        }
        let bw = vui.string_width(r, &buf);
        self.window.text_box(vui, r, &buf, half, 4, bw, 1, white, 0);
        let s = wide(&format!("|x{}   |t{}", vui.words.get(214, vui.language), vui.words.get(217, vui.language)));
        let width = self.window.size[0];
        self.window.text_box(vui, r, &s, 20, 6, width - 40, 1, white, 0);
        let now = vui.time;
        if self.flash.pulse(138) {
            self.change_digit(1, now, cues);
        }
        if self.flash.pulse(139) {
            self.change_digit(-1, now, cues);
        }
    }
}

/// Which child has the keys.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Child {
    Trade,
    MBox,
    Deposit,
    Quantity,
    Edit,
}

pub struct Trade {
    /// -1 asking, 0 the inventory, 1 the player's offer, 2 the money, 3 the
    /// partner's offer
    pub focus: i32,
    fade_dir: i32,
    fade: f32,
    list: WndList,
    mine: WndList,
    theirs: WndList,
    mbox: MBox,
    deposit: Deposit,
    quantity: Quantity,
    edit: EditBox,
    child: Child,
    pub i_accepted: bool,
    pub they_accepted: bool,
    time: i64,
    window: Window,
    offer: Window,
    pub visible: bool,
    /// the three lists' frames and the money where they were drawn, by the
    /// focus each is (0 the inventory, 1 the offer, 2 the money, 3 the
    /// partner's offer), for a pointer
    hits: Hits,
}

impl Default for Trade {
    fn default() -> Self {
        let list = WndList::with_rows([6, 37], 0, 40);
        let mut mine = WndList::with_rows([155, 37], 0, 4);
        mine.x64 = false;
        let mut theirs = WndList::with_rows([295, 37], 0, 4);
        theirs.x64 = false;
        Trade {
            focus: -1,
            fade_dir: 1,
            fade: 1.0,
            list,
            mine,
            theirs,
            mbox: MBox::default(),
            deposit: Deposit::default(),
            quantity: Quantity::default(),
            edit: EditBox::default(),
            child: Child::Trade,
            i_accepted: false,
            they_accepted: false,
            time: 0,
            window: Window::default(),
            offer: Window::default(),
            visible: false,
            hits: Hits::default(),
        }
    }
}

fn box_key(key: i32) -> Option<BoxKey> {
    match key {
        13 | 134 => Some(BoxKey::Enter),
        27 | 132 => Some(BoxKey::Escape),
        17 | 136 => Some(BoxKey::Left),
        18 | 137 => Some(BoxKey::Right),
        _ => None,
    }
}

/// A list's rows off a set of items (`RefreshLists`): a stack shows its
/// count, a worn item the arrows (138), one the player cannot use the red
/// ones (140).
fn fill(list: &mut WndList, items: &[&NetItem], cap: usize, icon: &dyn Fn(&NetItem) -> i32, can_use: &dyn Fn(&NetItem) -> bool) {
    let n = items.len().min(cap);
    for (i, it) in items.iter().take(n).enumerate() {
        let mut flags = if it.quantity >= 2 { 5 } else { 1 };
        let mut mark = -1;
        if it.equip_slot != -1 {
            mark = 138;
            flags |= 8;
        }
        if it.item.wear_slot != -1 && !can_use(it) {
            mark = 140;
            flags |= 8;
        }
        list.set_item(i, flags, icon(it), mark, -1, it.quantity as i32);
    }
    for i in n..cap {
        list.set_item(i, 0, -1, -1, -1, 0);
    }
}

impl Trade {
    /// `Ask`: the partner's name and word 472 as a Yes/No box at (80, 115)
    /// (parameter -1); nothing else shown.
    pub fn ask(&mut self, name: &str, words: &vi::strtab::StringTable, now: i64) {
        self.quantity.visible = false;
        self.deposit.visible = false;
        self.mbox.pos = [80, 115];
        let text = format!("{name} {}", words.get(472, 0));
        self.mbox.prompt(&wide(&text), 1, -1, 0, now);
        self.child = Child::MBox;
        self.focus = -1;
        self.visible = true;
    }

    /// `Start`: the inventory list lit, both offers from the top, nothing
    /// else up, the accepts cleared; and the focus (295).
    pub fn start(&mut self, vui: &Vui, r: &dyn Raster, now: i64) {
        self.list.set_x64(true);
        self.list.reset_scroll();
        self.mine.set_x64(false);
        self.mine.reset_scroll();
        self.theirs.set_x64(false);
        self.theirs.reset_scroll();
        self.deposit.visible = false;
        self.deposit.carried = 0;
        self.quantity.visible = false;
        self.child = Child::Trade;
        self.focus = 0;
        self.i_accepted = false;
        self.they_accepted = false;
        self.window.init(vui, r, [395, 183], 480, 15, 0);
        self.window.open(true);
        self.time = now;
        self.fade_dir = 1;
        self.fade = 1.0;
        self.visible = true;
    }

    /// The chat line has the keys.
    pub fn typing(&self) -> bool {
        self.child == Child::Edit
    }

    /// `Hide`.
    pub fn hide(&mut self) {
        self.deposit.visible = false;
        self.child = Child::Trade;
        self.focus = 0;
        self.visible = false;
    }

    /// `ClearAccepted`.
    pub fn clear_accepted(&mut self) {
        self.i_accepted = false;
        self.they_accepted = false;
    }

    /// A pointer at frame point `p`, to the child with the keys (the box,
    /// the money entry, the picker, the chat line). On the window: on the
    /// list (or the money) with the focus, a row selected or the selection
    /// used (Enter: offered, taken back, the money entered); on another, the
    /// focus moved there as the d-pad moves it and the row under the
    /// pointer selected. The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.child {
            Child::MBox => return self.mbox.point(p).then_some(key::CROSS).into_iter().collect(),
            Child::Deposit => return self.deposit.point(p),
            Child::Quantity => return self.quantity.point(p),
            Child::Edit => return self.edit.point(p).flatten().into_iter().collect(),
            Child::Trade => {}
        }
        let Some(target) = self.hits.at(p) else { return Vec::new() };
        let row = match target {
            0 => self.list.row_at(p),
            1 => self.mine.row_at(p),
            3 => self.theirs.row_at(p),
            _ => None,
        };
        if target == self.focus {
            return match (target, row) {
                (2, _) => vec![key::CROSS],
                (0, Some(r)) => vec![self.list.pick(r)],
                (1, Some(r)) => vec![self.mine.pick(r)],
                (3, Some(r)) => vec![self.theirs.pick(r)],
                _ => Vec::new(),
            };
        }
        self.focus_to(target);
        if let Some(r) = row {
            let (l, rows) = match target {
                0 => (&mut self.list, 6),
                1 => (&mut self.mine, 3),
                _ => (&mut self.theirs, 3),
            };
            l.pick(r);
            if l.top + rows - 1 < r {
                l.top = r - rows + 1;
            }
            l.top = l.top.min(r);
        }
        vec![MOVED]
    }

    /// The focus moved to `target` (0 the inventory, 1 the offer, 2 the
    /// money, 3 the partner's offer) as the d-pad moves it: `Left`, `Right`,
    /// `Up` from the money, and `Down`'s step off the bottom of the offer
    /// onto the money (the rows above skipped).
    fn focus_to(&mut self, target: i32) {
        let mut cues = Vec::new();
        for _ in 0..4 {
            match (self.focus, target) {
                (f, t) if f == t => break,
                (0, _) | (1 | 2, 3) => self.right(&mut cues),
                (2, 1) => self.up(138, &mut cues),
                (1, 2) => {
                    self.focus = 2;
                    self.mine.set_x64(false);
                }
                (1..=3, _) => self.left(&mut cues),
                _ => break,
            }
        }
    }

    /// The keys of whichever child has them.
    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, st: &TradeState, key: i32, down: bool, cues: &mut Vec<i32>, out: &mut Vec<TradeOut>) {
        let now = vui.time;
        match self.child {
            Child::MBox => {
                if !down {
                    return;
                }
                let Some(k) = box_key(key) else { return };
                match self.mbox.key(k, cues) {
                    // 268
                    Some(BoxAnswer::Yes(p1, _)) => {
                        self.child = Child::Trade;
                        if p1 == -1 {
                            out.push(TradeOut::Agree);
                        } else {
                            out.push(TradeOut::Cancel);
                        }
                    }
                    // 260
                    Some(BoxAnswer::No(p1, _)) => {
                        self.child = Child::Trade;
                        if p1 == -1 {
                            out.push(TradeOut::Cancel);
                        }
                    }
                    None => {}
                }
            }
            Child::Deposit => match self.deposit.key(key, down, now, cues) {
                Some(DepositUp::Value(v)) => {
                    // 294
                    out.push(TradeOut::Money(v));
                    self.deposit.visible = false;
                    self.deposit.carried = 0;
                    self.child = Child::Trade;
                }
                Some(DepositUp::Cancel) => {
                    self.deposit.visible = false;
                    self.deposit.carried = 0;
                    self.child = Child::Trade;
                }
                _ => {}
            },
            Child::Quantity => match self.quantity.key(key, down, cues) {
                Some(QuantityUp::Value(n, id)) => {
                    // 293
                    if self.focus == 0 && n > 0 {
                        out.push(TradeOut::Add(id as usize, n));
                    }
                    self.quantity.visible = false;
                    self.child = Child::Trade;
                }
                Some(QuantityUp::Cancel) => {
                    self.quantity.visible = false;
                    self.child = Child::Trade;
                }
                None => {}
            },
            Child::Edit => {
                if !down {
                    self.edit.key_up(key);
                    return;
                }
                match self.edit.key(vui, r, key, now, cues) {
                    EditUp::Done(_) => {
                        // 259: SendChat
                        out.push(TradeOut::Chat(String::from_utf16_lossy(&self.edit.text)));
                        self.child = Child::Trade;
                    }
                    EditUp::Cancel => self.child = Child::Trade,
                    EditUp::None => {}
                }
            }
            Child::Trade => {
                if !down {
                    // OnKeyUp: up and down to the list with the focus
                    if matches!(key, 19 | 138 | 20 | 139) {
                        match self.focus {
                            0 => self.list.key_up_down(key, false),
                            1 => self.mine.key_up_down(key, false),
                            3 => self.theirs.key_up_down(key, false),
                            _ => {}
                        }
                    }
                    return;
                }
                match key {
                    19 | 138 => self.up(key, cues),
                    20 | 139 => self.down(key, st, cues),
                    17 | 136 => self.left(cues),
                    18 | 137 => self.right(cues),
                    13 | 134 => self.select(vui, r, st, cues, out),
                    27 | 132 => self.cancel(vui, cues, out),
                    81 | 113 | 133 => {
                        // Accept
                        if self.focus >= 0 {
                            cues.push(13);
                            out.push(TradeOut::Accept);
                            self.i_accepted = true;
                        }
                    }
                    32 | 147 => {
                        // Chat
                        self.edit.clear();
                        self.edit.set_max_length(128);
                        self.edit.purpose = 0;
                        self.edit.focus(now);
                        self.child = Child::Edit;
                    }
                    _ => {}
                }
                self.window.reset_tick_pulse(now);
            }
        }
    }

    /// `Up`.
    fn up(&mut self, key: i32, cues: &mut Vec<i32>) {
        match self.focus {
            0 => self.list.key_up_down(key, true),
            1 => self.mine.key_up_down(key, true),
            2 => {
                self.focus = 1;
                self.mine.set_x64(true);
                cues.push(11);
            }
            3 => self.theirs.key_up_down(key, true),
            _ => {}
        }
    }

    /// `Down`: off the bottom of the player's offer onto the money.
    fn down(&mut self, key: i32, st: &TradeState, cues: &mut Vec<i32>) {
        match self.focus {
            0 => self.list.key_up_down(key, true),
            1 => {
                if self.mine.sel >= st.mine.len() as i32 - 1 || self.mine.count() <= 0 {
                    self.focus = 2;
                    self.mine.set_x64(false);
                    cues.push(11);
                } else {
                    self.mine.key_up_down(key, true);
                }
            }
            2 => cues.push(9),
            3 => self.theirs.key_up_down(key, true),
            _ => {}
        }
    }

    /// `Left`.
    fn left(&mut self, cues: &mut Vec<i32>) {
        match self.focus {
            1 | 2 => {
                self.focus = 0;
                self.list.set_x64(true);
                self.mine.set_x64(false);
                cues.push(11);
            }
            3 => {
                cues.push(11);
                self.focus = 1;
                self.theirs.set_x64(false);
                self.mine.set_x64(true);
            }
            _ => {}
        }
    }

    /// `Right`.
    fn right(&mut self, cues: &mut Vec<i32>) {
        match self.focus {
            0 => {
                self.focus = 1;
                self.list.set_x64(false);
                self.mine.reset_scroll();
                self.mine.set_x64(true);
                cues.push(11);
            }
            1 | 2 => {
                self.focus = 3;
                self.mine.set_x64(false);
                self.theirs.set_x64(true);
                self.theirs.reset_scroll();
                cues.push(11);
            }
            _ => {}
        }
    }

    /// `Select`: an inventory item offered (a stack asks how many), an
    /// offered one taken back, the money entered.
    fn select(&mut self, vui: &Vui, r: &dyn Raster, st: &TradeState, cues: &mut Vec<i32>, out: &mut Vec<TradeOut>) {
        match self.focus {
            0 => {
                let i = self.list.sel;
                if let Some(it) = st.inv.items.get(i.max(0) as usize).filter(|_| i >= 0) {
                    if it.item.max_stack != 0 && it.quantity >= 2 {
                        self.quantity.open(vui, r, it.quantity as i32, i, -1);
                        self.child = Child::Quantity;
                    } else {
                        out.push(TradeOut::Add(i as usize, 1));
                    }
                }
                cues.push(13);
            }
            1 => {
                out.push(TradeOut::Remove(self.mine.sel.max(0) as usize));
                cues.push(13);
            }
            2 => {
                self.deposit.open(vui, r, st.carried, st.my_money);
                cues.push(13);
                self.child = Child::Deposit;
            }
            _ => {}
        }
    }

    /// `Cancel`: while asking, the trade cancelled; else an accept taken
    /// back, or the "cancel the trade?" box (941) at (180, 115).
    fn cancel(&mut self, vui: &Vui, cues: &mut Vec<i32>, out: &mut Vec<TradeOut>) {
        if self.focus == -1 {
            cues.push(8);
            out.push(TradeOut::Cancel);
        } else if self.i_accepted {
            self.i_accepted = false;
            out.push(TradeOut::Unaccept);
        } else {
            cues.push(15);
            self.mbox.pos = [180, 115];
            self.mbox.prompt(&wide(vui.words.get(941, vui.language)), 1, self.focus, 0, vui.time);
            self.child = Child::MBox;
        }
    }

    /// `OnDrawSelf`: the back, the two panels, the banner (473), both sums,
    /// the text of the item under the cursor, the legend, the three lists
    /// (an accepted offer glowing green); then the children up.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, st: &TradeState, info: &Info, icon: &dyn Fn(&NetItem) -> i32, can_use: &dyn Fn(&NetItem) -> bool, cues: &mut Vec<i32>) {
        let now = vui.time;
        self.hits.clear();
        if self.focus != -1 {
            // RefreshLists
            let inv: Vec<&NetItem> = st.inv.items.iter().collect();
            fill(&mut self.list, &inv, 40, icon, can_use);
            let mine: Vec<&NetItem> = st.mine.iter().map(|o| &o.item).collect();
            fill(&mut self.mine, &mine, 4, icon, can_use);
            let theirs: Vec<&NetItem> = st.theirs.iter().map(|o| &o.item).collect();
            fill(&mut self.theirs, &theirs, 4, icon, can_use);
            let white = [1.0; 4];
            crate::ui::window_backdrop(vui, r, 642);
            self.window.init(vui, r, [395, 183], 480, 15, 0);
            self.window.open(true);
            let word = |id: i32| wide(vui.words.get(id, vui.language));
            let banner = word(473);
            let w = vui.string_width(r, &banner) + 64;
            vui.blit_bar(r, &banner, white, [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
            self.offer.init(vui, r, [554, 162], 240, 14, 1);
            self.offer.open(true);
            self.offer.draw(vui, r);
            self.offer.init(vui, r, [300, 255], 310, 3, 1);
            self.offer.open(true);
            self.offer.draw(vui, r);
            self.window.text_box(vui, r, &word(479), 30, 10, 105, 1, white, 1);
            self.window.text_box(vui, r, &wide(&st.my_money.to_string()), 30, 11, 105, 1, white, 5);
            self.hits.push(crate::pointer::rows_rect(&self.window, vui, r, 30, 10, 105, 2), 2);
            if self.focus == 2 {
                self.window.tick_r(vui, r, 20, 11, self.child == Child::Trade);
            }
            self.window.text_box(vui, r, &word(479), 170, 10, 105, 1, white, 1);
            self.window.text_box(vui, r, &wide(&st.their_money.to_string()), 170, 11, 105, 1, white, 5);
            let pick = |items: &[&NetItem], sel: i32| items.get(sel.max(0) as usize).filter(|_| sel >= 0).map(|it| info.trade_item(it));
            let desc = match self.focus {
                0 => pick(&inv, self.list.sel),
                1 | 2 => pick(&mine, self.mine.sel),
                3 => pick(&theirs, self.theirs.sel),
                _ => None,
            }
            .unwrap_or_default();
            self.window.text_box(vui, r, &wide(&desc), 295, -1, 170, 14, white, 3);
            let legend = wide(&format!("|x{}  |s{}  |t{}  |(Chat", vui.words.get(212, vui.language), vui.words.get(478, vui.language), vui.words.get(213, vui.language)));
            let w2 = vui.string_width(r, &legend);
            vui.blit_bar(r, &[], white, [165, 290], 25, 26, w2 + 64, 0, 1.0, 0, true);
            self.window.text_box(vui, r, &legend, 25, 14, w2 + 34, 1, vi::ui::UI_COLORS[0], 0);
            let dt = (now - self.time) as f32 * 0.001;
            self.time = now;
            let glow = |on: bool, fade: f32| if on { [0.5, 1.0, 0.5, fade] } else { [1.0; 4] };
            let focused = self.child == Child::Trade;
            self.list.x84 = focused;
            self.mine.x84 = focused;
            self.theirs.x84 = focused;
            // the lists' frames, as `VIWndList::OnDrawSelf` and
            // `VIWndTradeList::OnDrawSelf` size them
            let (stud, fill) = (vui.entry(36), vui.entry(82));
            let w = stud.uv1[0] * 2 + fill.uv1[0] + 114;
            for (l, h, k) in [(&self.list, 264, 0), (&self.mine, 3 * 44 + 30, 1), (&self.theirs, 3 * 44 + 30, 3)] {
                self.hits.push([l.pos[0], l.pos[1], l.pos[0] + w, l.pos[1] + stud.uv1[1] * 2 + h], k);
            }
            self.list.draw(vui, r, now, cues);
            let (mc, tc) = (glow(self.i_accepted, self.fade), glow(self.they_accepted, self.fade));
            self.mine.draw_trade(vui, r, 3, 474, mc, now, cues);
            self.theirs.draw_trade(vui, r, 3, 475, tc, now, cues);
            if self.fade_dir == -1 {
                self.fade = (self.fade - dt * 2.0).max(0.0);
                if self.fade <= 0.0 {
                    self.fade_dir = 1;
                }
            } else {
                self.fade = (self.fade + dt * 3.0).min(1.0);
                if self.fade >= 1.0 {
                    self.fade_dir = -1;
                }
            }
        }
        match self.child {
            Child::MBox => {
                self.mbox.draw(vui, r, now);
            }
            Child::Deposit => self.deposit.draw(vui, r, cues),
            Child::Quantity => self.quantity.draw(vui, r, cues),
            Child::Edit => self.edit.draw(vui, r, now, cues),
            Child::Trade => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn it(id: i64, key: u32, qty: i64, max_stack: i64) -> NetItem {
        let mut n = NetItem { quantity: qty, inv_index: key, equip_slot: -1, quick_slot: -1, ..Default::default() };
        n.item.id = id;
        n.item.max_stack = max_stack;
        n
    }

    fn state(asked: bool) -> TradeState {
        let inv = ItemList { p04: 7, items: vec![it(100, 3, 5, 20), it(200, 4, 1, 1)] };
        let mut s = TradeState { partner: 0x55, asked, ..Default::default() };
        s.setup(&inv, 1000);
        s
    }

    #[test]
    fn the_players_offer_comes_out_of_the_copy_and_goes_back() {
        let mut s = state(true);
        s.add_slot(3, 2);
        assert_eq!((s.mine.len(), s.mine[0].item.quantity, s.mine[0].id, s.inv.items[0].quantity), (1, 2, 0, 3));
        s.add_slot(3, 2);
        assert_eq!((s.mine.len(), s.mine[0].item.quantity, s.inv.items[0].quantity), (1, 4, 1), "onto the same entry");
        s.add_slot(3, 2);
        assert_eq!(s.mine[0].item.quantity, 4, "more than is left: nothing");
        s.add_slot(4, 1);
        assert_eq!((s.mine.len(), s.mine[1].id, s.inv.items.len()), (2, 1, 1), "the last one of a slot takes the slot");
        s.remove_slot(0);
        assert_eq!((s.mine.len(), s.inv.items[0].quantity), (1, 5), "back onto the stack");
        s.remove_slot(9);
        assert_eq!(s.mine.len(), 1, "an unknown id: nothing");
    }

    #[test]
    fn the_partners_offer_stacks_by_slot_and_stops_at_four() {
        let mut s = state(true);
        for k in 0..5 {
            s.add_item(it(1, 10 + k, 1, 1));
        }
        assert_eq!(s.theirs.len(), 4);
        s.add_item(it(1, 11, 3, 20));
        assert_eq!(s.theirs[1].item.quantity, 4);
        s.remove_item(1);
        assert_eq!(s.theirs.iter().map(|o| o.id).collect::<Vec<_>>(), vec![0, 2, 3]);
    }

    #[test]
    fn sub_opcodes_read_the_other_way_round_for_the_responder() {
        // sub 3 to the initiator is its own item by key; to the responder a
        // whole record of the partner's
        let mut w = net::stream::Writer::default();
        w.u8(3).u32(0x55).u32(3).signed(2);
        let mut a = state(true);
        assert_eq!(a.message(&w.data, 1000), Some(TradeMsg::Changed));
        assert_eq!((a.mine.len(), a.theirs.len()), (1, 0));
        let mut b = state(false);
        let mut w8 = net::stream::Writer::default();
        w8.u8(8).u32(0x55).u32(3).signed(2);
        assert_eq!(b.message(&w8.data, 1000), Some(TradeMsg::Changed));
        assert_eq!((b.mine.len(), b.theirs.len()), (1, 0), "8 is the responder's own");
        // money: 5 is the initiator's own, 10 the other's
        let mut m = net::stream::Writer::default();
        m.u8(5).u32(0x55).signed(300);
        a.message(&m.data, 1000);
        assert_eq!((a.my_money, a.carried, a.their_money), (300, 700, 0));
        b.message(&m.data, 1000);
        assert_eq!((b.my_money, b.their_money), (0, 300));
        // another partner's id is dropped, but for 1, 2 and 20
        let mut x = net::stream::Writer::default();
        x.u8(6).u32(0x99);
        assert_eq!(a.message(&x.data, 1000), None);
        let mut y = net::stream::Writer::default();
        y.u8(2).u32(0x99);
        assert_eq!(a.message(&y.data, 1000), Some(TradeMsg::Accepted(0x99)));
    }

    #[test]
    fn the_messages_sent() {
        let s = state(true);
        assert_eq!(s.send_sub(6), vec![0x56, 0, 6, 0x55, 0, 0, 0]);
        assert_eq!(s.send_add(0, 2).unwrap(), vec![0x56, 0, 3, 0x55, 0, 0, 0, 3, 0, 0, 0, 4]);
        assert_eq!(s.send_add(5, 1), None);
        assert_eq!(s.send_money(-1), vec![0x56, 0, 5, 0x55, 0, 0, 0, 1]);
    }

    #[test]
    fn the_money_digits() {
        let mut d = Deposit { carried: 1234, ..Default::default() };
        d.setup(3);
        assert_eq!(d.width, 4);
        d.digits[5] = 9;
        assert!(d.value() > d.carried);
        d.set_max();
        assert_eq!(d.value(), 1234);
        let mut cues = Vec::new();
        assert_eq!(d.key(134, true, 0, &mut cues), Some(DepositUp::Value(1234)));
        d.digits = [0; 9];
        assert_eq!(d.key(134, true, 0, &mut cues), Some(DepositUp::Cancel), "nothing: cancelled, sound 3");
        assert_eq!(cues, vec![13, 3]);
    }

    /// The duel's wager (`duelwnd`) takes the deposit in mode 4, the live
    /// client's: the purse its limit (as modes 1 and 3), OK as mode 3's (sound
    /// 13, the value), past the purse nothing. The control: mode 2 (the
    /// bank's withdrawal) is measured by the banked sum.
    #[test]
    fn the_wager_takes_the_deposit_in_mode_four() {
        let mut d = Deposit { carried: 300, banked: 5, ..Default::default() };
        d.setup(4);
        assert_eq!(d.width, 3, "three digits for 300");
        d.digits[6] = 2;
        let mut cues = Vec::new();
        assert_eq!(d.key(134, true, 0, &mut cues), Some(DepositUp::Value(200)));
        d.digits[6] = 4;
        assert_eq!(d.key(134, true, 0, &mut cues), None, "more than the purse");
        assert_eq!(cues, vec![13]);
        let mut b = Deposit { carried: 300, banked: 5, ..Default::default() };
        b.setup(2);
        assert_eq!(b.width, 1, "the control: the bank's limit");
    }

    /// On the list with the focus a click selects a row and a second uses
    /// it; on another list or the money the focus goes there as the d-pad
    /// takes it (the lists lit as `Left`/`Right` light them) with the row
    /// under the pointer selected; with the money entry up only it takes
    /// clicks.
    #[test]
    fn a_pointer_across_the_lists() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        use crate::pointer::rows_rect;
        let (vui, mut r) = (vui(), NullRaster);
        // the inventory left with two rows, the offer with two
        let mut st = state(true);
        st.inv.items.push(it(300, 5, 1, 1));
        st.inv.items.push(it(400, 6, 1, 1));
        st.add_slot(4, 1);
        st.add_slot(6, 1);
        assert_eq!((st.inv.items.len(), st.mine.len()), (2, 2));
        let base = [0; 29];
        let info = Info { words: &vui.words, inv: &st.inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let (icon, can_use) = (|_: &NetItem| 1, |_: &NetItem| true);
        let mut cues = Vec::new();
        let mut t = Trade::default();
        t.start(&vui, &r, 0);
        t.draw(&vui, &mut r, &st, &info, &icon, &can_use, &mut cues);
        // no atlas here: the lists' rows start at their corners, 44 high;
        // a trade list's rows under its 30-pixel title
        let inv_row = |i: i32| [6.0 + 57.0, 37.0 + 44.0 * i as f32 + 22.0];
        let mine_row = |i: i32| [155.0 + 57.0, 37.0 + 30.0 + 44.0 * i as f32 + 22.0];
        let theirs_frame = [295.0 + 57.0, 100.0];
        let money = mid(rows_rect(&t.window, &vui, &r, 30, 10, 105, 2));
        assert_eq!(t.point(inv_row(1)), vec![MOVED]);
        assert_eq!(t.list.sel, 1);
        assert_eq!(t.point(inv_row(1)), vec![key::CROSS], "offered (Enter)");
        assert!(t.point(inv_row(2)).is_empty(), "two rows");
        // the offer's second row: Right, then the row
        assert_eq!(t.point(mine_row(1)), vec![MOVED]);
        assert_eq!((t.focus, t.mine.sel, t.list.x64, t.mine.x64), (1, 1, false, true));
        assert_eq!(t.point(mine_row(1)), vec![key::CROSS], "taken back (Enter)");
        // the money: Down's step off the offer; Enter on it opens the entry
        assert_eq!(t.point(money), vec![MOVED]);
        assert_eq!((t.focus, t.mine.x64), (2, false));
        assert_eq!(t.point(money), vec![key::CROSS]);
        // the partner's empty offer: its frame takes the focus
        assert_eq!(t.point(theirs_frame), vec![MOVED]);
        assert_eq!((t.focus, t.theirs.x64), (3, true));
        // back across to the inventory: Left twice, its first row
        assert_eq!(t.point(inv_row(0)), vec![MOVED]);
        assert_eq!((t.focus, t.list.sel, t.list.x64, t.mine.x64, t.theirs.x64), (0, 0, true, false, false));
        assert!(t.point([600.0, 20.0]).is_empty(), "off the window");
        // the money entry up: the lists behind it take no clicks
        t.focus_to(2);
        t.key(&vui, &mut r, &st, 134, true, &mut cues, &mut Vec::new());
        assert_eq!(t.child, Child::Deposit);
        t.draw(&vui, &mut r, &st, &info, &icon, &can_use, &mut cues);
        assert!(t.point(inv_row(0)).is_empty());
        assert_eq!(t.focus, 2);
    }
}
