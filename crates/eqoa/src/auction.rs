//! The auction house: `VIBankControl`'s half of it (CLIENT
//! `VIBankControl.cpp`). Every request is message 0x1205 with a sub-op byte
//! (5 sell, 6 bid, 7 a page of a search, 8 the lots or bids the player has,
//! 9 the bidders of a lot); the replies are 0x1206 with sub-ops 128..137
//! (`Recv1206`). A search page is numbered (`m_page`, raised by every
//! `ClearResults`), and a reply for another page is dropped. The bank's own
//! lots and bids (`m_lots`, `m_bids`) are the MemoryDump's, kept in
//! `dump.bank`, and 0x1206 adds to and removes from them.

#![allow(clippy::too_many_arguments)]

use crate::dialog::{BoxAnswer, Key as BoxKey, MBox};
use crate::editbox::{EditBox, EditUp};
use crate::infotext::Info;
use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::wndlist::WndList;
use net::dump::{AuctionItem, Bank, Bid, ItemList, NetItem};
use net::stream::{Reader, Writer};
use vi::ui::{FrameParam, Raster, Vui, Window, wide};

/// `VIAuctionQuery`: what a search asks for.
#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub kind: u8,
    /// the seller (`ReadString(.., 24)`)
    pub name: String,
    /// the item's name (`ReadString(.., 128)`, wide)
    pub text: String,
    /// the class and race masks, raw words
    pub p9c: u32,
    pub pa0: u32,
    /// the level range, the price range, then two more (wear slot, attack)
    pub pa4: i32,
    pub pa8: i32,
    pub pac: i32,
    pub pb0: i32,
    pub pb4: i32,
    pub pb8: i32,
    pub pbc: bool,
}

impl Default for Query {
    /// The constructor's: everything, levels 1..50, any price.
    fn default() -> Self {
        Query { kind: 0, name: String::new(), text: String::new(), p9c: 0xFFFF_FFFF, pa0: 0xFFFF_FFFF, pa4: 1, pa8: 50, pac: 0, pb0: 0x7FFF_FFFF, pb4: 0, pb8: 0, pbc: true }
    }
}

impl Query {
    /// `operator<<(VIMemStream &, const VIAuctionQuery &)`.
    pub fn write(&self, w: &mut Writer) {
        w.u8(self.kind).string(self.name.as_bytes()).wide(&self.text).u32(self.p9c).u32(self.pa0);
        for v in [self.pa4, self.pa8, self.pac, self.pb0, self.pb4, self.pb8] {
            w.signed(v as i64);
        }
        w.u8(self.pbc as u8);
    }
}

/// `VIDbRemoteCharacterRow`: a bidder (sub-op 135's list).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bidder {
    pub x00: i64,
    pub x04: i64,
    pub x08: i64,
    pub name: String,
    /// the key the list is sorted on, highest first
    pub x24: i64,
    pub x28: i64,
}

/// A message for the chat box (`AddText`), by word, in the auction's cyan.
pub const CYAN: [f32; 4] = [0.19, 0.96, 0.96, 1.0];

/// What a reply asks of the client besides the state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AuctionNote {
    /// `AddText(word)` in cyan
    Word(i32),
    /// `PlaySound(5)`
    Sound,
}

/// `VIBankControl`'s auction state.
#[derive(Clone, Debug, Default)]
pub struct Auction {
    pub query: Query,
    /// `m_page`: the request a results page must answer
    pub page: i32,
    /// the page shown and its three columns (`m_p7880`, `m_p7888`,
    /// `m_p7890`), and the one before it (`m_saved`, `m_p78A0`..)
    pub results: Vec<AuctionItem>,
    pub cols: [Vec<i64>; 3],
    pub saved: Vec<AuctionItem>,
    pub saved_cols: [Vec<i64>; 3],
    /// `m_p7868`: the server's minutes when the page was made; `m_p7870` the
    /// client's time it arrived
    pub minutes: i64,
    pub arrived: i64,
    /// `m_p7860`: nothing has arrived since the request (1)
    pub p7860: i32,
    /// `m_p78BC`, `m_p78C0`, `m_p78B8`: the bidders' lot and list
    pub p78bc: i64,
    pub bidders: Vec<Bidder>,
    pub p78b8: i32,
}

impl Auction {
    pub fn new() -> Self {
        Auction { p7860: 1, p78bc: -1, p78b8: 1, ..Default::default() }
    }

    /// `ClearResults`: the page emptied, and the next request numbered.
    pub fn clear_results(&mut self) {
        self.results.clear();
        self.cols = Default::default();
        self.minutes = -1;
        self.arrived = -1;
        self.p7860 = 1;
        self.page += 1;
    }

    /// `ClearSaved`.
    pub fn clear_saved(&mut self) {
        self.saved.clear();
        self.saved_cols = Default::default();
    }

    /// `SaveResults`.
    fn save_results(&mut self) {
        self.saved = self.results.clone();
        self.saved_cols = self.cols.clone();
    }

    /// The 0x1205 head of a request.
    fn op(sub: u8) -> Writer {
        let mut w = Writer::op(0x1205);
        w.u8(sub);
        w
    }

    /// `Sell(slot, price, buyout, duration)`: sub-op 5, the banker, the bank
    /// record's key, a count of 1 and the three numbers.
    pub fn sell(target: u32, key: u32, a: i32, b: i32, c: i32) -> Vec<u8> {
        let mut w = Self::op(5);
        w.u32(target).u32(key).signed(1).signed(a as i64).signed(b as i64).signed(c as i64);
        w.data
    }

    /// `Send06(lot, amount)`: a bid (sub-op 6), with a count of 1.
    pub fn bid(a: i32, b: i32) -> Vec<u8> {
        let mut w = Self::op(6);
        w.signed(a as i64).signed(b as i64).signed(1);
        w.data
    }

    /// The search page request (sub-op 7): the page, the lot it starts
    /// from, the direction (1 forward), the query.
    fn page_request(&self, from: i64, forward: bool) -> Vec<u8> {
        let mut w = Self::op(7);
        w.signed(self.page as i64).signed(from).u8(forward as u8);
        self.query.write(&mut w);
        w.data
    }

    /// `Search(q)`: a new search from the first page.
    pub fn search(&mut self, q: Query) -> Vec<u8> {
        self.query = q;
        self.clear_saved();
        self.clear_results();
        self.page_request(-1, true)
    }

    /// `NextPage`: from the last lot shown.
    pub fn next_page(&mut self) -> Vec<u8> {
        let last = self.results.last().map_or(-1, |l| l.lot_id);
        self.save_results();
        self.clear_results();
        self.page_request(last, true)
    }

    /// `PrevPage`: back from the first lot shown.
    pub fn prev_page(&mut self) -> Vec<u8> {
        let first = self.results.first().map_or(-1, |l| l.lot_id);
        self.save_results();
        self.clear_results();
        self.page_request(first, false)
    }

    /// `RequestLots` / `RequestBids`: sub-op 8 with the ids the bank holds.
    pub fn request_ids(&mut self, ids: &[i64]) -> Vec<u8> {
        self.clear_saved();
        self.clear_results();
        let mut w = Self::op(8);
        w.signed(self.page as i64).signed(ids.len() as i64);
        for &id in ids {
            w.signed(id);
        }
        w.data
    }

    /// `Send09(lot)`: the bidders.
    pub fn bidders(lot: i32) -> Vec<u8> {
        let mut w = Self::op(9);
        w.signed(lot as i64);
        w.data
    }

    /// `GetMinutes`: the server's minutes plus those since the page came.
    pub fn get_minutes(&self, now: i64) -> i64 {
        self.minutes + (now - self.arrived) / 60000
    }

    /// `FindBid`: the bank's bid on a lot, its two amounts.
    pub fn find_bid(bank: &Bank, auction: i64) -> Option<(i64, i64)> {
        bank.bids.iter().find(|b| b.auction_id == auction).map(|b| (b.x08, b.x10))
    }

    /// `Recv1206`: the reply's sub-op, over the state and the bank's lots and
    /// bids. `auction_mode` is `IsAuctionMode` (the search window up), `now`
    /// the client's time. Returns the state's new value (0 for every reply
    /// that ends a wait, `None` where the game leaves it) and the notes.
    pub fn recv(&mut self, bank: &mut Bank, data: &[u8], auction_mode: bool, now: i64) -> (Option<i32>, Vec<AuctionNote>) {
        let mut r = Reader::new(data);
        let op = r.u8();
        let mut notes = Vec::new();
        let state = match op {
            129 => self.recv_results(&mut r, auction_mode, now, &mut notes),
            137 => {
                self.recv_update_lot(&mut r);
                None
            }
            128 => {
                self.clear_results();
                Some(0)
            }
            130 => {
                bank.lots.push(net::dump::read_auction_item(&mut r));
                notes.push(AuctionNote::Sound);
                Some(0)
            }
            131 => {
                let id = r.signed();
                if let Some(i) = bank.lots.iter().position(|l| l.lot_id == id) {
                    bank.lots.remove(i);
                }
                notes.push(AuctionNote::Sound);
                None
            }
            132 => {
                bank.bids.push(net::dump::read_bid(&mut r));
                notes.push(AuctionNote::Sound);
                Some(0)
            }
            133 => {
                let id = r.signed();
                if let Some(i) = bank.bids.iter().position(|b| b.auction_id == id) {
                    bank.bids.remove(i);
                }
                notes.push(AuctionNote::Sound);
                None
            }
            134 => {
                let b: Bid = net::dump::read_bid(&mut r);
                if let Some(x) = bank.bids.iter_mut().find(|x| x.auction_id == b.auction_id) {
                    *x = b;
                }
                notes.push(AuctionNote::Sound);
                None
            }
            135 => {
                self.p78bc = r.signed();
                let n = r.signed().max(0) as usize;
                self.bidders = (0..n)
                    .map_while(|_| {
                        let b = Bidder { x00: r.signed(), x04: r.signed(), x08: r.signed(), name: String::from_utf8_lossy(&r.string(24)).into_owned(), x24: r.signed(), x28: r.signed() };
                        (!r.error).then_some(b)
                    })
                    .collect();
                // qsort by +36, highest first (not stable)
                self.bidders.sort_by_key(|b| std::cmp::Reverse(b.x24));
                self.p7860 = 0;
                self.p78b8 = 0;
                Some(0)
            }
            136 => {
                self.p78bc = r.signed();
                self.p7860 = 0;
                self.p78b8 = 1;
                Some(0)
            }
            _ => {
                eprintln!("auction: unknown message {op}");
                None
            }
        };
        (state, notes)
    }

    /// A zigzag-counted list of zigzag words (`VIArray<int>`).
    fn read_ints(r: &mut Reader) -> Vec<i64> {
        let n = r.signed().max(0) as usize;
        (0..n).map_while(|_| (!r.error).then(|| r.signed())).collect()
    }

    /// `RecvResults` (129).
    fn recv_results(&mut self, r: &mut Reader, auction_mode: bool, now: i64, notes: &mut Vec<AuctionNote>) -> Option<i32> {
        let page = r.signed();
        if page != self.page as i64 {
            return Some(0);
        }
        self.minutes = r.signed();
        let n = r.signed().max(0) as usize;
        self.results = (0..n).map_while(|_| (!r.error).then(|| net::dump::read_auction_item(r))).collect();
        for c in &mut self.cols {
            *c = Self::read_ints(r);
        }
        if self.results.is_empty() {
            if self.saved.is_empty() {
                notes.push(AuctionNote::Word(1787));
            } else {
                // back on the page before, and word 1798
                self.results = self.saved.clone();
                self.cols = self.saved_cols.clone();
                notes.push(AuctionNote::Word(1798));
            }
            self.clear_saved();
        } else {
            // the same lots as the page before: word 1799
            let changed = self.results.iter().zip(&self.saved).any(|(a, b)| a.lot_id != b.lot_id) || self.results.len() != self.saved.len();
            if !changed {
                notes.push(AuctionNote::Word(1799));
            }
            self.clear_saved();
        }
        self.arrived = now;
        self.p7860 = 0;
        if !auction_mode {
            self.clear_saved();
            self.clear_results();
        }
        Some(0)
    }

    /// `RecvUpdateLot` (137): one lot, the minutes and its three columns,
    /// wherever either page shows it.
    fn recv_update_lot(&mut self, r: &mut Reader) {
        let lot = net::dump::read_auction_item(r);
        self.minutes = r.signed();
        let (a, b, c) = (r.signed(), r.signed(), r.signed());
        for (items, cols) in [(&mut self.saved, &mut self.saved_cols), (&mut self.results, &mut self.cols)] {
            if let Some(i) = items.iter().position(|l| l.lot_id == lot.lot_id) {
                items[i] = lot.clone();
                for (k, v) in [a, b, c].into_iter().enumerate() {
                    if let Some(x) = cols[k].get_mut(i) {
                        *x = v;
                    }
                }
            }
        }
    }
}

/// `VIWndAuction::CanSell`: tradeable, not lore, rentable, and its stack
/// test as the server's patch leaves it (`s2::StackTest`): retail's, not a
/// stack; Sandstorm's S2 lets a stack go too (its record 3 rewrites the
/// test, so on its PS2s crafting materials and gems sell).
pub fn can_sell(it: &NetItem) -> bool {
    can_sell_by(it, crate::s2::get().sell_stack)
}

/// `can_sell` under a given stack test.
pub fn can_sell_by(it: &NetItem, stack: crate::s2::StackTest) -> bool {
    it.item.no_trade == 0 && it.item.lore == 0 && it.item.rent != 0 && stack.passes(it.item.max_stack)
}

/// `atoi`: blanks, a sign, the digits.
pub fn atoi(s: &str) -> i32 {
    let t = s.trim_start();
    let (neg, t) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let v = t.bytes().take_while(u8::is_ascii_digit).fold(0i32, |v, d| v.wrapping_mul(10).wrapping_add((d - b'0') as i32));
    if neg { v.wrapping_neg() } else { v }
}

/// What the sell form asks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SellOut {
    /// 260 to the bank: back to its menu
    Back,
    /// `Sell(slot, price, buyout, duration)` confirmed
    Sell(i32, i32, i32, i32),
    /// `AddText(word 1727)` in red: the item cannot be sold
    CannotSell,
}

/// Which child of the form has the keys.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum SellChild {
    #[default]
    Form,
    Edit,
    MBox,
}

/// `VIWndAuction`: the bank's items (80 rows); Enter on one that can be sold
/// opens the form (the item, minimum bid, buyout, duration, Ok and the fee),
/// whose numbers are typed into the editor and whose Ok is confirmed in a
/// Yes/No box before the sale is sent. A pointer (`pointer`) takes the
/// list's rows, then the form's (a click on another row selects it, on the
/// selected one edits it or asks), then the editor's and the box's.
pub struct SellWnd {
    bank: WndList,
    edit: EditBox,
    mbox: MBox,
    child: SellChild,
    /// `m_selling`: the form is up
    pub selling: bool,
    /// `m_field`: 0 the item .. 4 Ok
    pub field: i32,
    pub price: i32,
    pub buyout: i32,
    /// minutes
    pub duration: i32,
    window: Window,
    sell: Window,
    /// the form's rows where they were drawn, by field, for a pointer
    hits: Hits,
}

impl Default for SellWnd {
    fn default() -> Self {
        let mut bank = WndList::with_rows([16, 37], 0, 80);
        bank.set_x64(true);
        SellWnd { bank, edit: EditBox::default(), mbox: MBox::default(), child: SellChild::Form, selling: false, field: 0, price: 0, buyout: 0, duration: 0, window: Window::default(), sell: Window::default(), hits: Hits::default() }
    }
}

thread_local! {
    /// `InMinutes`: the GM channel's flag (`VIClientPart382E8::IsEnabled() >
    /// 0`), kept here by `adminplay` each HUD frame; a player's never has
    /// it: durations are in hours.
    static IN_MINUTES: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `InMinutes`.
fn in_minutes() -> bool {
    IN_MINUTES.with(|c| c.get())
}

/// The GM channel's flag, for `InMinutes`.
pub fn set_in_minutes(on: bool) {
    IN_MINUTES.with(|c| c.set(on));
}

impl SellWnd {
    /// The focus from outside (295): the windows reset, the form down.
    pub fn focus(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.init(vui, r, [320, 180], 330, 14, 0);
        self.window.open(true);
        self.sell.init(vui, r, [320, 180], 600, 9, 0);
        self.sell.close(true);
        self.selling = false;
        self.field = 0;
        self.child = SellChild::Form;
    }

    /// The editor is up (it takes the keyboard's characters).
    pub fn typing(&self) -> bool {
        self.child == SellChild::Edit
    }

    /// A pointer at frame point `p`, to the child with the keys (the
    /// editor, the box); else, the form down, a row of the bank's list
    /// selected or its selection taken (Enter: the form); the form up, one
    /// of its rows selected (not the item's, which the keys skip) or the
    /// selected one edited or confirmed (Enter). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.child {
            SellChild::Edit => return self.edit.point(p).flatten().into_iter().collect(),
            SellChild::MBox => return self.mbox.point(p).then_some(key::CROSS).into_iter().collect(),
            SellChild::Form => {}
        }
        if !self.selling {
            return self.bank.row_at(p).map(|row| self.bank.pick(row)).into_iter().collect();
        }
        match self.hits.at(p) {
            Some(f) if f == self.field => vec![key::CROSS],
            Some(f) if f >= 1 => {
                self.field = f;
                vec![MOVED]
            }
            _ => Vec::new(),
        }
    }

    /// `Select`: the form for the selected bank item, if it can be sold.
    fn select(&mut self, bank: &ItemList, cues: &mut Vec<i32>, out: &mut Vec<SellOut>) {
        let Some(it) = bank.items.get(self.bank.sel.max(0) as usize) else { return };
        if can_sell(it) {
            cues.push(13);
            self.price = it.item.value as i32;
            self.buyout = self.price.wrapping_mul(2);
            self.duration = if in_minutes() { 2 } else { 720 };
            self.field = 1;
            self.selling = true;
            self.sell.close(true);
            self.sell.open(false);
        } else {
            out.push(SellOut::CannotSell);
            cues.push(9);
        }
    }

    /// `Accept`: Ok asks to confirm, another row is edited.
    fn accept(&mut self, vui: &Vui, bank: &ItemList, now: i64, cues: &mut Vec<i32>) {
        let mut sound = 13;
        if self.field == 4 {
            match bank.items.get(self.bank.sel.max(0) as usize).filter(|it| can_sell(it)) {
                Some(it) => {
                    let w = |id: i32| vui.words.get(id, vui.language).to_string();
                    let text = format!("{}: {}\n{}: {} {}\n{}: {} {}\n{}", w(1723), it.item.name, w(1789), self.price, w(479), w(1726), if in_minutes() { self.duration } else { self.duration / 60 }, w(if in_minutes() { 1724 } else { 1725 }), w(1790));
                    self.mbox.prompt(&wide(&text), 1, 0, 0, now);
                    self.mbox.pos = [175, 115];
                    self.child = SellChild::MBox;
                }
                None => sound = 9,
            }
        } else {
            self.edit_field(vui, now);
        }
        cues.push(sound);
    }

    /// `EditField`: the editor over the minimum bid, buyout or duration.
    fn edit_field(&mut self, vui: &Vui, now: i64) {
        let e = &mut self.edit;
        e.set_rows(1);
        e.set_multi_line(false);
        e.set_center([320, 180]);
        e.set_max_length(128);
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        match self.field {
            1 => {
                e.set_max_length(10);
                e.title = wide(&w(1715));
            }
            2 => {
                e.set_max_length(10);
                e.title = wide(&w(1716));
            }
            3 => {
                e.set_max_length(5);
                e.title = wide(&format!("{} {}", w(1726), w(if in_minutes() { 1687 } else { 1691 })));
            }
            _ => return,
        }
        e.clear();
        e.focus(now);
        self.child = SellChild::Edit;
    }

    /// `TakeText`: the editor's number into its row.
    fn take_text(&mut self) {
        let v = atoi(&String::from_utf16_lossy(&self.edit.text));
        match self.field {
            1 => self.price = v,
            2 => self.buyout = v,
            3 => self.duration = if in_minutes() { v } else { v.wrapping_mul(60) },
            _ => {}
        }
        self.child = SellChild::Form;
    }

    /// `OnKeyDown` / `OnKeyUp`, and the children's.
    pub fn key(&mut self, vui: &Vui, r: &dyn Raster, bank: &ItemList, key: i32, down: bool, now: i64, cues: &mut Vec<i32>, out: &mut Vec<SellOut>) {
        match self.child {
            SellChild::Edit => {
                if !down {
                    self.edit.key_up(key);
                    return;
                }
                match self.edit.key(vui, r, key, now, cues) {
                    EditUp::Done(_) => self.take_text(),
                    EditUp::Cancel => self.child = SellChild::Form,
                    EditUp::None => {}
                }
                return;
            }
            SellChild::MBox => {
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
                    // 268: Confirm
                    Some(BoxAnswer::Yes(..)) => {
                        let slot = self.bank.sel;
                        if bank.items.get(slot.max(0) as usize).is_some_and(can_sell) {
                            out.push(SellOut::Sell(slot, self.price, self.buyout, self.duration));
                            self.selling = false;
                        }
                        self.child = SellChild::Form;
                    }
                    Some(BoxAnswer::No(..)) => self.child = SellChild::Form,
                    None => {}
                }
                return;
            }
            SellChild::Form => {}
        }
        if !down {
            if !self.selling {
                self.bank.key_up_down(key, false);
            }
            return;
        }
        if !self.selling {
            match key {
                13 | 134 => self.select(bank, cues, out),
                19 | 138 | 20 | 139 => self.bank.key_up_down(key, true),
                27 | 132 => out.push(SellOut::Back),
                _ => {}
            }
        } else {
            match key {
                13 | 134 => self.accept(vui, bank, now, cues),
                19 | 138 => {
                    // Prev
                    if self.field < 2 {
                        self.field = 1;
                        cues.push(9);
                    } else {
                        self.field -= 1;
                        cues.push(11);
                    }
                    self.sell.reset_tick_pulse(now);
                }
                20 | 139 => {
                    // Next
                    if self.field >= 4 {
                        self.field = 4;
                        cues.push(9);
                    } else {
                        self.field += 1;
                        cues.push(11);
                    }
                    self.sell.reset_tick_pulse(now);
                }
                27 | 132 => {
                    cues.push(8);
                    self.selling = false;
                }
                _ => {}
            }
        }
    }

    /// `FillLists`' bank half: a stack shows its count, what cannot be sold
    /// the red arrows.
    fn fill(&mut self, bank: &ItemList, icon: &dyn Fn(&NetItem) -> i32) {
        for i in 0..80 {
            self.bank.set_item(i, 0, -1, -1, -1, 0);
        }
        let stack = crate::s2::get().sell_stack;
        for (i, it) in bank.items.iter().take(80).enumerate() {
            let mut flags = if it.quantity >= 2 { 5 } else { 1 };
            let mut color = -1;
            if !can_sell_by(it, stack) {
                color = 140;
                flags |= 8;
            }
            self.bank.set_item(i, flags, icon(it), color, -1, it.quantity as i32);
        }
    }

    /// `DrawRow`: a label, a framed value, a unit; ticked when selected.
    fn draw_row(w: &Window, vui: &Vui, r: &mut dyn Raster, row: i32, a: Option<&[u16]>, xa: i32, b: Option<&[u16]>, xb: i32, c: Option<&[u16]>, xc: i32, sel: bool, tick: bool) {
        let color = if sel { [1.0, 1.0, 0.0, w.open] } else { [1.0, 1.0, 1.0, w.open] };
        if let Some(a) = a {
            w.text_box(vui, r, a, xa, row, xb - xa, 1, color, 0);
        }
        if let Some(b) = b {
            let width = if c.is_some() { xc - xb - 12 } else { w.size[0] - xb - 30 };
            w.draw_frame(vui, r, xb, row, width, 1, 64);
            w.text_box(vui, r, b, xb, row, width, 1, color, 2);
        }
        if let Some(c) = c {
            w.text_box(vui, r, c, xc, row, w.size[0] - xb - 30, 1, color, 0);
        }
        if sel && tick {
            w.tick_r(vui, r, xa, row, true);
        }
    }

    /// `OnDrawSelf`: the backdrop, the bar (word 1721), the frame, the
    /// selected item's text and the bank's list; the form over them.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, info: &Info, bank: &ItemList, icon: &dyn Fn(&NetItem) -> i32, now: i64, cues: &mut Vec<i32>) {
        self.fill(bank, icon);
        self.hits.clear();
        let white = [1.0, 1.0, 1.0, 1.0];
        crate::ui::window_backdrop(vui, r, 642);
        let title = wide(vui.words.get(1721, vui.language));
        let w = vui.string_width(r, &title) + 64;
        vui.blit_bar(r, &title, white, [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
        let fp = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
        vui.blit_fill_frame(r, [165, 48], [310, 267], &fp, 35, 0, 1.0, true);
        vui.blit_studded_horz(r, [165, 37], 37, 38, 310, 2, 0, 1.0, 0);
        vui.blit_studded_horz(r, [165, 313], 40, 41, 310, 2, 0, 1.0, 0);
        vui.blit_horz(r, [16, 322], 60, 608, 0, 1.0, 0);
        let text = bank.items.get(self.bank.sel.max(0) as usize).map_or_else(String::new, |it| info.bank_item(it));
        self.window.init(vui, r, [390, 180], 470, 14, 0);
        self.window.set_title(Some(&[]));
        self.window.draw(vui, r);
        let ww = self.window.size[0];
        self.window.text_box(vui, r, &wide(&text), 20, 0, ww - 40, 14, [1.0, 1.0, 1.0, self.window.open], 2);
        self.bank.x84 = !self.selling;
        self.bank.draw(vui, r, now, cues);
        if self.selling {
            self.draw_sell(vui, r, bank);
        }
        match self.child {
            SellChild::Edit => self.edit.draw(vui, r, now, cues),
            SellChild::MBox => {
                self.mbox.draw(vui, r, now);
            }
            SellChild::Form => {}
        }
    }

    /// `DrawSell`: the numbers clamped as they are drawn (the price at least
    /// the item's value, the buyout at least the price, both under a
    /// billion; 12 hours to 3 days).
    fn draw_sell(&mut self, vui: &Vui, r: &mut dyn Raster, bank: &ItemList) {
        let Some(it) = bank.items.get(self.bank.sel.max(0) as usize) else { return };
        self.sell.init(vui, r, [320, 180], 475, 9, 0);
        self.sell.set_title(Some(&wide(vui.words.get(1722, vui.language))));
        self.sell.draw(vui, r);
        let cols = [40, 180, 240];
        self.price = self.price.max(it.item.value as i32).min(999_999_999);
        self.buyout = self.buyout.max(self.price).min(999_999_999);
        self.duration = if in_minutes() { self.duration.clamp(2, 4320) } else { self.duration.clamp(720, 4320) };
        let w = |id: i32| wide(vui.words.get(id, vui.language));
        let s = &self.sell;
        let f = self.field;
        Self::draw_row(s, vui, r, 1, Some(&w(1723)), cols[0], Some(&wide(&it.item.name)), cols[1], None, cols[2], f == 0, true);
        Self::draw_row(s, vui, r, 2, Some(&w(1685)), cols[0], Some(&wide(&self.price.to_string())), cols[1], None, cols[2], f == 1, true);
        Self::draw_row(s, vui, r, 3, Some(&w(1686)), cols[0], Some(&wide(&self.buyout.to_string())), cols[1], None, cols[2], f == 2, true);
        let unit = w(if in_minutes() { 1724 } else { 1725 });
        let d = if in_minutes() { self.duration } else { self.duration / 60 };
        Self::draw_row(s, vui, r, 4, Some(&w(1726)), cols[0], Some(&wide(&d.to_string())), cols[1], Some(&unit), cols[2], f == 3, true);
        let ok = wide(&format!("{}*", vui.words.get(218, vui.language)));
        Self::draw_row(s, vui, r, 5, Some(&ok), cols[0], None, cols[1], None, cols[2], f == 4, true);
        // the rows 1..5 are the fields 0..4
        for field in 0..5 {
            self.hits.push(rows_rect(s, vui, r, cols[0], field + 1, s.size[0] - 30 - cols[0], 1), field);
        }
        let fee = (self.price / 100).max(1);
        let line = wide(&format!("*{}: {} {}", vui.words.get(1788, vui.language), fee, vui.words.get(479, vui.language)));
        let width = s.size[0] - 30 - cols[0];
        Self::draw_row(s, vui, r, 7, Some(&line), cols[0], None, width, None, cols[2], false, false);
        let legend = wide(&format!("|x{}   |t{}", vui.words.get(212, vui.language), vui.words.get(161, vui.language)));
        s.text_box(vui, r, &legend, 20, 8, s.size[0] - 40, 1, [1.0, 1.0, 1.0, s.open], 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::dump::{Item, read_auction_item};

    /// A lot on the wire, as the server writes one (`operator<<`).
    fn lot_bytes(w: &mut Writer, id: i64) {
        for v in [id, 1, 0, 0, 0, 0, 0] {
            w.signed(v);
        }
        w.string(b"Seller");
        w.bytes(&item_bytes());
    }

    /// A zeroed item record, as long as `read_item` reads from zeros.
    fn item_bytes() -> Vec<u8> {
        let zeros = vec![0u8; 4096];
        let mut r = Reader::new(&zeros);
        let _: Item = net::dump::read_item(&mut r);
        vec![0u8; r.pos]
    }

    fn page(page: i64, ids: &[i64]) -> Vec<u8> {
        let mut w = Writer::op(0);
        w.data.clear();
        w.u8(129).signed(page).signed(30).signed(ids.len() as i64);
        for &id in ids {
            lot_bytes(&mut w, id);
        }
        for _ in 0..3 {
            w.signed(ids.len() as i64);
            for _ in ids {
                w.signed(7);
            }
        }
        w.data
    }

    #[test]
    fn a_lot_reads_back() {
        let mut w = Writer::op(0);
        w.data.clear();
        lot_bytes(&mut w, 42);
        let mut r = Reader::new(&w.data);
        let l = read_auction_item(&mut r);
        assert_eq!((l.lot_id, l.seller.as_str()), (42, "Seller"));
        assert!(!r.error && r.pos == w.data.len());
    }

    #[test]
    fn pages_and_their_numbers() {
        let mut a = Auction::new();
        let mut bank = Bank::default();
        let req = a.search(Query::default());
        assert_eq!(&req[..3], &[0x05, 0x12, 7]);
        assert_eq!(a.page, 1);
        // a reply for another page is dropped
        let (st, notes) = a.recv(&mut bank, &page(0, &[5]), true, 1000);
        assert_eq!((st, notes.len(), a.results.len()), (Some(0), 0, 0));
        let (_, notes) = a.recv(&mut bank, &page(1, &[5, 6]), true, 1000);
        assert!(notes.is_empty());
        assert_eq!(a.results.iter().map(|l| l.lot_id).collect::<Vec<_>>(), vec![5, 6]);
        assert_eq!((a.cols[2].clone(), a.minutes, a.p7860), (vec![7, 7], 30, 0));
        assert_eq!(a.get_minutes(1000 + 120_000), 32);
        // next page: the same lots again is word 1799
        a.next_page();
        let (_, notes) = a.recv(&mut bank, &page(2, &[5, 6]), true, 2000);
        assert_eq!(notes, vec![AuctionNote::Word(1799)]);
        // an empty page after one: the page before kept, word 1798
        a.next_page();
        let (_, notes) = a.recv(&mut bank, &page(3, &[]), true, 3000);
        assert_eq!(notes, vec![AuctionNote::Word(1798)]);
        assert_eq!(a.results.len(), 2);
        // not in the search window: the page dropped at once
        a.next_page();
        a.recv(&mut bank, &page(4, &[9]), false, 0);
        assert!(a.results.is_empty() && a.page == 5);
        // negative control: nothing at all is word 1787
        a.search(Query::default());
        let p = a.page as i64;
        let (_, notes) = a.recv(&mut bank, &page(p, &[]), true, 0);
        assert_eq!(notes, vec![AuctionNote::Word(1787)]);
    }

    #[test]
    fn lots_and_bids_of_the_bank() {
        let mut a = Auction::new();
        let mut bank = Bank::default();
        let mut w = Writer::op(0);
        w.data.clear();
        w.u8(130);
        lot_bytes(&mut w, 77);
        let (st, notes) = a.recv(&mut bank, &w.data, true, 0);
        assert_eq!((st, notes), (Some(0), vec![AuctionNote::Sound]));
        assert_eq!(bank.lots.len(), 1);
        let mut w = Writer::op(0);
        w.data.clear();
        w.u8(132);
        for v in [1, 77, 500, 0, 900] {
            w.signed(v);
        }
        w.u8(0);
        a.recv(&mut bank, &w.data, true, 0);
        assert_eq!(Auction::find_bid(&bank, 77), Some((500, 900)));
        assert_eq!(Auction::find_bid(&bank, 78), None);
        // removing them leaves the wait alone (None)
        let mut w = Writer::op(0);
        w.data.clear();
        w.u8(131).signed(77);
        assert_eq!(a.recv(&mut bank, &w.data, true, 0).0, None);
        assert!(bank.lots.is_empty());
        // the bidders, highest first
        let mut w = Writer::op(0);
        w.data.clear();
        w.u8(135).signed(77).signed(2);
        for (name, k) in [("Low", 10), ("High", 90)] {
            w.signed(0).signed(0).signed(0).string(name.as_bytes()).signed(k).signed(0);
        }
        a.recv(&mut bank, &w.data, true, 0);
        assert_eq!(a.bidders.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), vec!["High", "Low"]);
        assert_eq!((a.p78bc, a.p78b8), (77, 0));
    }

    #[test]
    fn requests_on_the_wire() {
        let s = Auction::sell(0x10, 0x20, 100, 200, 720);
        assert_eq!(s, vec![0x05, 0x12, 5, 0x10, 0, 0, 0, 0x20, 0, 0, 0, 2, 200, 1, 144, 3, 160, 11]);
        assert_eq!(Auction::bid(77, 5), vec![0x05, 0x12, 6, 154, 1, 10, 2]);
        let mut a = Auction::new();
        let r = a.request_ids(&[3, 4]);
        assert_eq!(r, vec![0x05, 0x12, 8, 2, 4, 6, 8]);
        let mut q = Writer::op(0);
        q.data.clear();
        Query::default().write(&mut q);
        assert_eq!(q.data.len(), 1 + 4 + 4 + 4 + 4 + 1 + 1 + 1 + 5 + 1 + 1 + 1);
    }

    /// A stack (a gem, a crafting material: max stack 20) is refused under
    /// retail's CanSell and sold under Sandstorm's S2, which rewrites the
    /// test; the form opens for it, or says 1727 and plays sound 9. The
    /// other tests stand under both: lore, no trade and no rent stay
    /// refused.
    #[test]
    fn a_stack_sells_where_the_patch_lets_it() {
        use crate::pointer::testing::{NullRaster, vui};
        use crate::s2::{Rules, StackTest, with_rules};
        let (vui, r) = (vui(), NullRaster);
        let mut gem = NetItem { quantity: 5, equip_slot: -1, quick_slot: -1, ..Default::default() };
        (gem.item.rent, gem.item.value, gem.item.max_stack) = (1, 300, 20);
        let mut sword = gem.clone();
        (sword.quantity, sword.item.max_stack) = (1, 1);
        let mut bank = ItemList::default();
        bank.items.extend([gem.clone(), sword.clone()]);
        let sandstorm = || Rules { sell_stack: StackTest::Plus(1), ..Rules::retail() };
        // the control: retail's rules refuse the gem
        assert!(!can_sell(&gem) && can_sell(&sword));
        assert!(with_rules(sandstorm(), || can_sell(&gem) && can_sell(&sword)));
        for (name, edit) in [("lore", 0), ("no trade", 1), ("no rent", 2)] {
            let mut it = gem.clone();
            match edit {
                0 => it.item.lore = 1,
                1 => it.item.no_trade = 1,
                _ => it.item.rent = 0,
            }
            assert!(!with_rules(sandstorm(), || can_sell(&it)), "{name}");
        }
        let select = |rules: Rules| {
            with_rules(rules, || {
                let (mut cues, mut out) = (Vec::new(), Vec::new());
                let mut s = SellWnd::default();
                s.focus(&vui, &r);
                s.key(&vui, &r, &bank, 134, true, 0, &mut cues, &mut out);
                (s.selling, s.price, cues, out)
            })
        };
        assert_eq!(select(Rules::retail()), (false, 0, vec![9], vec![SellOut::CannotSell]));
        assert_eq!(select(sandstorm()), (true, 300, vec![13], vec![]));
    }

    /// `InMinutes` (the GM channel's flag, `adminplay`): the form starts at
    /// 2 minutes and keeps them, held to 2..4320; the control, a player's:
    /// 720 minutes shown as 12 hours, typed hours made minutes.
    #[test]
    fn the_gm_sells_in_minutes() {
        use crate::pointer::testing::{NullRaster, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let mut bank = ItemList::default();
        let mut it = NetItem { quantity: 1, equip_slot: -1, quick_slot: -1, ..Default::default() };
        it.item.rent = 1;
        it.item.value = 50;
        bank.items.push(it);
        let mut form = |gm: bool| {
            set_in_minutes(gm);
            let (mut cues, mut out) = (Vec::new(), Vec::new());
            let mut s = SellWnd::default();
            s.focus(&vui, &r);
            s.key(&vui, &r, &bank, 134, true, 0, &mut cues, &mut out);
            let start = s.duration;
            s.field = 3;
            s.edit.set_text(&wide("1"));
            s.take_text();
            let typed = s.duration;
            s.duration = 1;
            s.draw_sell(&vui, &mut r, &bank);
            set_in_minutes(false);
            (start, typed, s.duration)
        };
        assert_eq!(form(true), (2, 1, 2), "minutes: 2 to start, 1 typed is 1, held at 2");
        assert_eq!(form(false), (720, 60, 720), "hours: 12 to start, 1 typed is 60, held at 720");
    }

    /// The bank's rows take a click, a second asks for the form; with the
    /// form up its rows take them (not the item's, which the keys skip, nor
    /// the list behind it), a second click editing the row.
    #[test]
    fn a_pointer_on_the_sell_form() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let mut bank = ItemList::default();
        for v in [10, 20] {
            let mut it = NetItem { quantity: 1, equip_slot: -1, quick_slot: -1, ..Default::default() };
            it.item.rent = 1;
            it.item.value = v;
            bank.items.push(it);
        }
        let base = [0; 29];
        let info = Info { words: &vui.words, inv: &bank, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let (mut cues, mut out) = (Vec::new(), Vec::new());
        let mut s = SellWnd::default();
        s.focus(&vui, &r);
        s.draw(&vui, &mut r, &info, &bank, &|_| 1, 0, &mut cues);
        // the bank's list at (16, 37), rows 44 high (no atlas here)
        let row = |i: i32| [16.0 + 57.0, 37.0 + 44.0 * i as f32 + 22.0];
        assert_eq!(s.point(row(1)), vec![MOVED]);
        assert_eq!(s.point(row(1)), vec![key::CROSS]);
        assert!(s.point(row(2)).is_empty(), "two items");
        s.key(&vui, &r, &bank, 134, true, 0, &mut cues, &mut out);
        assert!(s.selling && s.field == 1);
        s.draw(&vui, &mut r, &info, &bank, &|_| 1, 0, &mut cues);
        let field = |s: &SellWnd, f: i32| mid(rows_rect(&s.sell, &vui, &r, 40, f + 1, s.sell.size[0] - 70, 1));
        let p = field(&s, 3);
        assert_eq!(s.point(p), vec![MOVED]);
        assert_eq!(s.field, 3, "the duration");
        assert_eq!(s.point(p), vec![key::CROSS]);
        let p = field(&s, 0);
        assert!(s.point(p).is_empty(), "the item's row");
        assert!(s.point(row(0)).is_empty(), "the list behind the form");
        assert_eq!(s.field, 3);
        // Enter on the duration: the editor, which then takes the clicks
        s.key(&vui, &r, &bank, 134, true, 0, &mut cues, &mut out);
        assert!(s.typing());
        let p = field(&s, 4);
        let want: Vec<i32> = s.edit.point(p).flatten().into_iter().collect();
        assert_eq!(s.point(p), want, "the editor's answer");
        assert_eq!(s.field, 3, "Ok is behind the editor");
    }
}
