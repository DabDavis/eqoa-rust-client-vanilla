//! The bank in play: `VIBankControl`'s half (the bank's list and tunar
//! kept from the MemoryDump, the requests 0x1254/0x1255 to the banker, the
//! replies 0x1250/0x1251/0x1253, the waiting state) and `VIWndGame`'s
//! (0x124D opens the window from play, mode 11; `CloseBank`). The window is
//! `bank`; this ties it to the session. A child of `play`, for `Play`'s
//! fields.

use super::Play;
use crate::bank::{Bank, BankOut, View};

/// `VIBankControl`'s waiting state and the window.
pub struct BankPlay {
    /// `m_state`: 0 idle, 1 a money move, 2 an item move, 3..6 the
    /// auction's requests
    pub state: i32,
    /// the auction's half (`VIBankControl`'s search state)
    pub auction: crate::auction::Auction,
    wnd: Bank,
    /// `VIWndAuctionSearch` (mode 39) and the kind it was opened for
    search: crate::auctionwnd::SearchWnd,
    search_open: Option<u32>,
    open: bool,
    keys: Vec<(i32, bool)>,
}

impl Default for BankPlay {
    fn default() -> Self {
        BankPlay { state: 0, auction: crate::auction::Auction::new(), wnd: Bank::default(), search: Default::default(), search_open: None, open: false, keys: Vec::new() }
    }
}

impl Play {
    /// The bank window or the auction search has the keys (modes 11, 39).
    pub fn bank_owns(&self) -> bool {
        self.mode == 11 || self.mode == 39
    }

    /// `VIBankControl::Do` (0xA8010000..0xA8030000 from the start menu):
    /// `VIWndGame::OpenAuctionSearch`, mode 39.
    pub(super) fn open_auction_search(&mut self, kind: u32) {
        self.mode = 39;
        self.bank.search_open = Some(kind);
    }

    /// `CloseAuctionSearch`: the page and the one before cleared, the start
    /// menu again.
    fn close_auction_search(&mut self) {
        self.bank.auction.clear_results();
        self.bank.auction.clear_saved();
        if self.mode == 39 {
            self.mode = 0;
        }
        self.open_start_menu(false);
    }

    /// The auction form's editor has the keyboard.
    pub fn bank_typing(&self) -> bool {
        (self.mode == 11 && self.bank.wnd.typing()) || (self.mode == 39 && self.bank.search.typing())
    }

    /// A key for the bank, handled with the next HUD frame.
    pub fn bank_key(&mut self, key: i32, down: bool) {
        self.bank.keys.push((key, down));
    }

    /// A pointer at frame point `f` on the bank (its pages and the auction's
    /// sell form) or the auction search (`pointer`), as their keys go: what
    /// is under it selected, and the keys to press.
    pub fn bank_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        match self.mode {
            11 => self.bank.wnd.point(f),
            39 => self.bank.search.point(f),
            _ => Vec::new(),
        }
    }

    /// The bank's opcodes; true when handled. 0x00D7 (`Msg00D7`) clears the
    /// waiting state and is left for the merchant's half too.
    pub(super) fn bank_opcode(&mut self, op: u16, data: &[u8]) -> bool {
        let mut r = net::stream::Reader::new(data);
        match op {
            0x124D => {
                // only from play (IsLootable): OpenBank, VIBankControl::Close
                if self.mode == 0 {
                    self.cues.push(crate::sound::Cue::Ui(15));
                    self.bank.open = true;
                    self.mode = 11;
                    self.bank.state = 0;
                }
            }
            0x1253 => {
                // the bank's tunar
                let money = r.signed();
                self.cues.push(crate::sound::Cue::Ui(3));
                self.dump.player.p38 = money;
                self.bank.state = 0;
            }
            0x1251 => {
                // RecvAdded: AddItem(item, its maximum stack, 40)
                let item = net::dump::read_net_item(&mut r);
                let limit = item.item.max_stack;
                crate::inventory::add_item(&mut self.dump.bank.items, &item, limit, 40);
                self.cues.push(crate::sound::Cue::Ui(5));
                self.bank.state = 0;
            }
            0x1250 => {
                // RecvTook: n of the slot
                let slot = r.u8() as usize;
                let n = r.signed();
                if slot < self.dump.bank.items.items.len() {
                    crate::inventory::took(&mut self.dump.bank.items, slot, n);
                }
                self.cues.push(crate::sound::Cue::Ui(5));
                self.bank.state = 0;
            }
            0x1206 => {
                // Recv1206: the auction's replies (IsAuctionMode: the search, mode 39)
                let now = self.session.now() as i64;
                let (state, notes) = self.bank.auction.recv(&mut self.dump.bank, data, self.mode == 39, now);
                if let Some(st) = state {
                    self.bank.state = st;
                }
                for n in notes {
                    match n {
                        crate::auction::AuctionNote::Sound => self.cues.push(crate::sound::Cue::Ui(5)),
                        crate::auction::AuctionNote::Word(id) => {
                            let text = self.words.as_ref().map_or_else(String::new, |w| w.get(id, 0).to_string());
                            self.add_chat(text, crate::auction::CYAN);
                        }
                    }
                }
            }
            0x00D7 => {
                self.bank.state = 0;
                return false;
            }
            _ => return false,
        }
        true
    }

    /// `VIBankControl::Close` from a respawn (0x00B2) or a new dump.
    pub(super) fn bank_reset(&mut self) {
        self.bank.state = 0;
    }

    /// `ClosePanel6AC`: `CloseBank`, mode 0.
    fn bank_close(&mut self) {
        if self.mode == 11 {
            self.mode = 0;
        }
        self.bank.wnd.visible = false;
    }

    /// The window's opens, keys, drawing, and what it asks.
    pub(super) fn run_bank(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        if std::mem::take(&mut self.bank.open) {
            // `m_auction`: the dump's flag, or the live word's bit 0x01 (`live`)
            let auction = crate::live::features().auction(self.dump.bank.p78cc);
            self.bank.wnd.open(vui, r, auction);
        }
        if let Some(kind) = self.bank.search_open.take() {
            let now = vui.time;
            if let Some(req) = self.bank.search.open(vui, r, kind, &mut self.bank.auction, &self.dump.bank, now) {
                self.session.send(&req);
                self.bank.state = 5;
            }
        }
        if self.mode == 39 {
            self.run_auction_search(vui, r);
            return;
        }
        if self.mode != 11 {
            self.bank.keys.clear();
            return;
        }
        let now = vui.time;
        let flags = net::session::EXPANSION_FLAGS;
        let attack_words = self.start.as_ref().map(|s| s.attack_words.clone()).unwrap_or_default();
        let pl = &self.dump.player;
        let (class, race, level) = (pl.class, pl.race, pl.level);
        let mut cues = Vec::new();
        let mut out = Vec::new();
        {
            let info = crate::infotext::Info {
                words: &vui.words,
                inv: &self.dump.inventory,
                hp: [self.stats.stat[7], self.stats.stat[8], self.stats.stat[9], self.stats.stat[10]],
                base: &self.stats.base,
                class,
                level,
                attack_words: &attack_words,
            };
            let view = View { carried: pl.p34 as i32, banked: pl.p38 as i32, inv: &self.dump.inventory, bank: &self.dump.bank.items, idle: self.bank.state == 0 };
            let icon = |it: &net::dump::NetItem| crate::inventory::item_icon(it, flags) as i32;
            let can_use = |it: &net::dump::NetItem| crate::inventory::can_use(it, class, race, level);
            let b = &mut self.bank;
            for (k, down) in std::mem::take(&mut b.keys) {
                // the requests a key makes count at once (IsIdle)
                let v = View { idle: b.state == 0 && !out.iter().any(|o| matches!(o, BankOut::Move(..) | BankOut::MoveMoney(..))), ..view };
                b.wnd.key(vui, r, &v, k, down, now, &mut cues, &mut out);
            }
            b.wnd.draw(vui, r, &info, &view, &icon, &can_use, now, &mut cues);
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for o in out {
            match o {
                BankOut::Close => self.bank_close(),
                BankOut::ChatMode(m) => {
                    if let Some(c) = self.wnd_chat.as_mut() {
                        c.mode = m;
                    }
                }
                BankOut::Move(from_bank, slot, n) => {
                    // VIBankControl::Move: the record's key in the list it leaves
                    let list = if from_bank != 0 { &self.dump.bank.items } else { &self.dump.inventory };
                    if let Some(it) = list.items.get(slot.max(0) as usize).filter(|_| slot >= 0) {
                        let mut w = net::stream::Writer::op(0x1254);
                        w.u32(self.target).u8(from_bank).u32(it.inv_index).signed(n as i64);
                        self.session.send(&w.data);
                        self.bank.state = 2;
                    }
                }
                BankOut::MoveMoney(dir, n) => {
                    let mut w = net::stream::Writer::op(0x1255);
                    w.u32(self.target).u8(dir).signed(n as i64);
                    self.session.send(&w.data);
                    self.bank.state = 1;
                }
                BankOut::Auction => {}
                BankOut::Sell(slot, n, a, b, c) => {
                    // VIBankControl::Sell: the bank record's key, the count
                    // (the live client's quantity), state 3
                    if let Some(it) = self.dump.bank.items.items.get(slot.max(0) as usize).filter(|_| slot >= 0) {
                        let msg = crate::auction::Auction::sell(self.target, it.inv_index, n, a, b, c);
                        self.session.send(&msg);
                        self.bank.state = 3;
                    }
                }
                BankOut::CannotSell => {
                    let text = self.words.as_ref().map_or_else(String::new, |w| w.get(1727, 0).to_string());
                    self.add_chat(text, [1.0, 0.25, 0.25, 1.0]);
                }
            }
        }
    }

    /// The auction search's keys, drawing and requests.
    fn run_auction_search(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        let flags = net::session::EXPANSION_FLAGS;
        let pl = &self.dump.player;
        let (class, race, level, id) = (pl.class, pl.race, pl.level, pl.id);
        let icon = |it: &net::dump::NetItem| crate::inventory::item_icon(it, flags) as i32;
        let can_use = |it: &net::dump::NetItem| crate::inventory::can_use(it, class, race, level);
        let attack_words = self.start.as_ref().map(|s| s.attack_words.clone()).unwrap_or_default();
        let mut cues = Vec::new();
        let mut out = Vec::new();
        let b = &mut self.bank;
        let now = self.session.now() as i64;
        for (k, down) in std::mem::take(&mut b.keys) {
            let v = crate::auctionwnd::View { bank: &self.dump.bank, idle: b.state == 0, searching: b.state == 5, player: id, now, icon: &icon, can_use: &can_use, attack_words: &attack_words };
            b.search.key(vui, r, &mut b.auction, &v, k, down, &mut cues, &mut out);
            for o in &out {
                if let crate::auctionwnd::SearchOut::Send(_, st) = o {
                    b.state = *st;
                }
            }
        }
        let v = crate::auctionwnd::View { bank: &self.dump.bank, idle: b.state == 0, searching: b.state == 5, player: id, now, icon: &icon, can_use: &can_use, attack_words: &attack_words };
        b.search.draw(vui, r, &b.auction, &v, &mut cues);
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for o in out {
            match o {
                crate::auctionwnd::SearchOut::Send(msg, _) => self.session.send(&msg),
                crate::auctionwnd::SearchOut::Close => self.close_auction_search(),
                crate::auctionwnd::SearchOut::Red(id) => {
                    let text = self.words.as_ref().map_or_else(String::new, |w| w.get(id, 0).to_string());
                    self.add_chat(text, [1.0, 0.25, 0.25, 1.0]);
                }
                crate::auctionwnd::SearchOut::RedCount(id, n) => {
                    let text = self.words.as_ref().map_or_else(String::new, |w| w.get(id, 0).replacen("%d", &n.to_string(), 1));
                    self.add_chat(text, [1.0, 0.25, 0.25, 1.0]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::pointer::testing::{NullRaster, vui};

    /// The bank menu's Auction row: the MemoryDump's flag as the disc has
    /// it, or, on a server that sends the live client's feature word
    /// (`live`), its bit 0x01 whatever the dump says (the live dump has no
    /// flag).
    #[test]
    fn the_auction_row_by_the_dump_or_the_feature_word() {
        let (vui, mut r) = (vui(), NullRaster);
        let mut row = |word: Option<u32>, flag: bool| {
            crate::live::with_features(word, || {
                let mut p = crate::play::liveplay::tests::play();
                crate::live::set_features(crate::live::Features(word));
                p.dump.bank.p78cc = flag;
                p.bank.open = true;
                p.run_bank(&vui, &mut r);
                p.bank.wnd.offers_auction()
            })
        };
        assert!(row(None, true) && !row(None, false), "the disc: the dump's flag");
        assert!(row(Some(0x1B), false), "bit 0x01");
        assert!(!row(Some(0x10), true), "the control: a word without 0x01");
    }
}
