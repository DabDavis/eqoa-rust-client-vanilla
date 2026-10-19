//! The auction house: `VIBankControl`'s half of it (CLIENT
//! `VIBankControl.cpp`). Every request is message 0x1205 with a sub-op byte
//! (5 sell, 6 bid, 7 a page of a search, 8 the lots or bids the player has,
//! 9 the bidders of a lot); the replies are 0x1206 with sub-ops 128..137
//! (`Recv1206`). A search page is numbered (`m_page`, raised by every
//! `ClearResults`), and a reply for another page is dropped. The bank's own
//! lots and bids (`m_lots`, `m_bids`) are the MemoryDump's, kept in
//! `dump.bank`, and 0x1206 adds to and removes from them.

use net::dump::{AuctionItem, Bank, Bid};
use net::stream::{Reader, Writer};

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
                self.bidders.sort_by(|a, b| b.x24.cmp(&a.x24));
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
}
