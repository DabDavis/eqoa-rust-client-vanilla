//! The auction search (CLIENT `viwndauctionsearch.cpp`, `VIWndAuctionSearch`,
//! the game window's mode 39), opened from the start menu's bank tables
//! (0xA8010000 search, 0xA8020000 my lots, 0xA8030000 my bids). Its states:
//! 0 the search form (name, class, race, the level and price ranges, wear
//! slot, attack type, a flag, the seller, Search), 1 the result page in a
//! `VIWndMenu` (each lot with Inspect and Bid), 2 a lot, 3 its bidders, 4
//! the bid form (an amount, Bid, Buyout); 5..8 are the class, race, wear
//! slot and attack type pickers.
//!
//! Not ported: the four pickers (their rows keep the "any" the form starts
//! with) and Inspect (`DescribeAuctionItem`'s four describers).
//!
//! A pointer (`pointer`) goes to the child with the keys (the editor, the
//! box, the page's menu and its popup); a click on the form from the page
//! goes right to it, and one on the page's rows from the form left to them,
//! the field or the row under the pointer selected; the form's fields, the
//! bidders and the bid form's rows are selected by a click and used by a
//! second.

#![allow(clippy::too_many_arguments)]

use crate::auction::{Auction, Query, atoi};
use crate::client_image::{Menu, MenuEntry, Menus};
use crate::dialog::{BoxAnswer, Key as BoxKey, MBox};
use crate::editbox::{EditBox, EditUp};
use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::wndlist::{ListUp, WndMenu};
use net::dump::{AuctionItem, Bank, NetItem};
use vi::ui::{Raster, Vui, Window, wide};

/// What the window asks of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum SearchOut {
    /// a 0x1205 request; `Some(state)` is what it sets `m_state` to
    Send(Vec<u8>, i32),
    /// `ClosePanel718`: `CloseAuctionSearch`
    Close,
    /// `AddText(word)` in red
    Red(i32),
}

/// Which child has the keys.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Child {
    #[default]
    Own,
    Menu,
    Edit,
    MBox,
}

/// What the window reads besides its own state.
pub struct View<'a> {
    pub bank: &'a Bank,
    /// `IsIdle`
    pub idle: bool,
    /// `IsSearching` (state 5)
    pub searching: bool,
    /// the player's id (`GetPlayer()->m_id`), for "your bid"
    pub player: i64,
    pub now: i64,
    pub icon: &'a dyn Fn(&NetItem) -> i32,
    pub can_use: &'a dyn Fn(&NetItem) -> bool,
}

pub struct SearchWnd {
    /// 0 search, 1 my lots, 2 my bids
    pub mode: i32,
    pub state: i32,
    field: i32,
    cursor: i32,
    top: i32,
    name: String,
    class: i32,
    race: i32,
    min_level: i32,
    max_level: i32,
    min_price: i32,
    max_price: i32,
    wear: i32,
    attack: i32,
    x4f4: i32,
    seller: String,
    lot: i32,
    bid: i32,
    dots: i32,
    ticks: i32,
    menu: WndMenu,
    menus: Menus,
    edit: EditBox,
    mbox: MBox,
    child: Child,
    window: Window,
    list: Window,
    status: Window,
    /// the search form's fields where they were drawn, by field; the
    /// bidders (by index) or the bid form's rows (by cursor): for a pointer
    form_hits: Hits,
    list_hits: Hits,
}

impl Default for SearchWnd {
    fn default() -> Self {
        let mut menus = Menus::default();
        // m_def: 40 rows, each with its own two-option sub-menu
        let root = menus.push(Menu { x00: -1, x04: -1, entries: vec![MenuEntry::default(); 40] });
        for _ in 0..40 {
            menus.push(Menu { x00: -1, x04: -1, entries: vec![MenuEntry::default(); 2] });
        }
        let mut menu = WndMenu::new([16, 37], 0);
        menu.menu = root;
        SearchWnd {
            mode: 0,
            state: 0,
            field: 0,
            cursor: 0,
            top: 0,
            name: String::new(),
            class: -1,
            race: -1,
            min_level: 1,
            max_level: 50,
            min_price: 0,
            max_price: 0x7FFF_FFFF,
            wear: -2,
            attack: -1,
            x4f4: 1,
            seller: String::new(),
            lot: -1,
            bid: 0,
            dots: 0,
            ticks: 15,
            menu,
            menus,
            edit: EditBox::default(),
            mbox: MBox::default(),
            child: Child::Own,
            window: Window::default(),
            list: Window::default(),
            status: Window::default(),
            form_hits: Hits::default(),
            list_hits: Hits::default(),
        }
    }
}

/// A lot as the item records it carries (for its icon and use).
fn as_net_item(l: &AuctionItem) -> NetItem {
    NetItem { quantity: l.quantity, cur_hp: l.cur_hp, charges: l.charges, equip_slot: -1, quick_slot: -1, inv_index: 0, item: l.item.clone() }
}

impl SearchWnd {
    /// `VIWndGame::OpenAuctionSearch(kind)`: mode 39, the focus from outside
    /// (295: everything reset), `SetMode`, then the bank's matching list
    /// asked for and `Open` (my lots, my bids), or the search's page cleared.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, kind: u32, auction: &mut Auction, bank: &Bank, now: i64) -> Option<Vec<u8>> {
        self.focus(vui, r);
        self.mode = match kind {
            0xA802_0000 => 1,
            0xA803_0000 => 2,
            _ => 0,
        };
        match kind {
            0xA802_0000 | 0xA803_0000 => {
                let ids: Vec<i64> = if kind == 0xA802_0000 { bank.lots.iter().map(|l| l.lot_id).collect() } else { bank.bids.iter().map(|b| b.auction_id).collect() };
                let req = auction.request_ids(&ids);
                // Open: the menu from the top, state 1
                self.menu.list.reset_scroll();
                self.set_state(1, now);
                Some(req)
            }
            _ => {
                auction.clear_results();
                auction.clear_saved();
                None
            }
        }
    }

    /// The focus from outside (295).
    fn focus(&mut self, vui: &Vui, r: &dyn Raster) {
        *self = SearchWnd { menus: std::mem::take(&mut self.menus), ..SearchWnd::default() };
        self.window.init(vui, r, [300, 180], 290, 14, 0);
        self.window.open(true);
        self.list.init(vui, r, [300, 180], 290, 10, 0);
        self.list.close(true);
        self.status.init(vui, r, [300, 180], 300, 3, 0);
        self.status.close(true);
        self.menu.visible = false;
        self.menu.list.set_x64(false);
    }

    /// The editor is up (it takes the keyboard).
    pub fn typing(&self) -> bool {
        self.child == Child::Edit
    }

    /// A pointer at frame point `p`, to the child with the keys: the editor,
    /// the box, the page's menu (its popup first). From the page a click on
    /// the search form goes right to it with the field under it selected;
    /// from the form one on the page's rows goes left to them with the row
    /// selected (the keys say whether there is a page). The form's fields,
    /// the bid form's rows: a click selects, a second uses (Enter); the
    /// bidders are selected. The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.child {
            Child::Edit => return self.edit.point(p).flatten().into_iter().collect(),
            Child::MBox => return self.mbox.point(p).then_some(key::CROSS).into_iter().collect(),
            Child::Menu => {
                let popup = self.menu.sub.as_ref().is_some_and(|s| s.visible);
                if !popup
                    && self.mode == 0
                    && let Some(f) = self.form_hits.at(p)
                {
                    self.field = f;
                    return vec![key::RIGHT];
                }
                return self.menu.point(p);
            }
            Child::Own => {}
        }
        match self.state {
            0 => {
                if let Some(f) = self.form_hits.at(p) {
                    if f == self.field {
                        return vec![key::CROSS];
                    }
                    self.field = f;
                    return vec![MOVED];
                }
                match self.menu.list.row_at(p) {
                    Some(row) => {
                        self.menu.list.pick(row);
                        vec![key::LEFT]
                    }
                    None => Vec::new(),
                }
            }
            3 => match self.list_hits.at(p) {
                Some(i) if i != self.cursor => {
                    self.cursor = i;
                    vec![MOVED]
                }
                _ => Vec::new(),
            },
            4 => match self.list_hits.at(p) {
                Some(c) if c == self.cursor => vec![key::CROSS],
                Some(c) => {
                    self.cursor = c;
                    vec![MOVED]
                }
                None => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    /// `SetState`.
    fn set_state(&mut self, state: i32, now: i64) {
        let old = self.state;
        self.state = state;
        let reopen = |l: &mut Window| {
            l.close(true);
            l.open(false);
        };
        let to_menu = |m: &mut WndMenu, c: &mut Child| {
            m.list.set_x64(true);
            m.focus(true, now);
            *c = Child::Menu;
        };
        match old {
            0 => {
                if state == 1 {
                    to_menu(&mut self.menu, &mut self.child);
                } else if (5..=8).contains(&state) {
                    self.cursor = 0;
                    self.top = 0;
                    reopen(&mut self.list);
                } else if state == 0 {
                    self.child = Child::Own;
                }
            }
            1 => match state {
                0 => {
                    self.menu.list.set_x64(false);
                    self.child = Child::Own;
                }
                2 => {
                    self.menu.list.set_x64(false);
                    self.cursor = 0;
                    self.top = 0;
                    reopen(&mut self.list);
                    self.child = Child::Own;
                }
                1 => to_menu(&mut self.menu, &mut self.child),
                _ => {}
            },
            2 => match state {
                1 => {
                    self.list.close(true);
                    to_menu(&mut self.menu, &mut self.child);
                }
                3 => {
                    self.cursor = 0;
                    self.top = 0;
                    reopen(&mut self.list);
                }
                4 => {
                    self.bid = 0;
                    self.cursor = 0;
                    self.top = 0;
                    reopen(&mut self.list);
                }
                _ => {}
            },
            3 => {
                if state == 2 {
                    self.cursor = 0;
                    self.top = 0;
                    reopen(&mut self.list);
                }
            }
            4 => match state {
                2 => {
                    self.cursor = 0;
                    self.top = 0;
                    reopen(&mut self.list);
                }
                4 => self.child = Child::Own,
                1 => {
                    self.list.close(true);
                    to_menu(&mut self.menu, &mut self.child);
                }
                _ => {}
            },
            _ => self.list.close(true),
        }
    }

    /// The lot shown in the menu's selection, when a page is there.
    fn sel_lot<'a>(&self, auction: &'a Auction, v: &View) -> Option<&'a AuctionItem> {
        if !v.idle || auction.p7860 != 0 {
            return None;
        }
        auction.results.get(self.menu.list.sel.max(0) as usize).filter(|_| self.menu.list.sel >= 0)
    }

    /// `FillMenu` / `FillRow`: the 40 rows emptied, then one per lot of the
    /// page (its icon, count, red arrows when unusable, Inspect and Bid).
    fn fill_menu(&mut self, auction: &Auction, v: &View) {
        let root = self.menu.menu;
        for row in 0..40 {
            self.menus.list[root].entries[row] = MenuEntry { icon: -1, icon2: -1, x08: -1, word: -1, number: -1, sub: None, msg: 0, id: -1 };
        }
        if v.searching || auction.p7860 != 0 {
            return;
        }
        for (row, lot) in auction.results.iter().take(40).enumerate() {
            let it = as_net_item(lot);
            let sub = root + 1 + row;
            self.menus.list[root].entries[row] = MenuEntry { icon: (v.icon)(&it), icon2: if (v.can_use)(&it) { -1 } else { 140 }, x08: -1, word: -1, number: lot.quantity as i32, sub: Some(sub), msg: 0, id: lot.lot_id as i32 };
            for (k, (word, msg)) in [(1676, 0xA804_0001u32), (1677, 0xA804_0000)].into_iter().enumerate() {
                self.menus.list[sub].entries[k] = MenuEntry { icon: -1, icon2: -1, x08: -1, word, number: -1, sub: None, msg, id: lot.lot_id as i32 };
            }
        }
    }

    /// `Select`: Enter on a form row.
    fn select(&mut self, vui: &Vui, auction: &mut Auction, now: i64, cues: &mut Vec<i32>, out: &mut Vec<SearchOut>) {
        match self.field {
            // the pickers (states 6, 5, 7, 8) are not ported
            1 | 2 | 5 | 6 => eprintln!("auction: the search form's picker {} is not ported", self.field),
            0 | 3 | 4 | 7 | 8 | 10 => self.edit_field(vui, now),
            9 => self.x4f4 = 1 - self.x4f4,
            11 => {
                let mut q = Query { kind: 0, ..Default::default() };
                if self.max_price < 999_999_999 {
                    q.kind = 16;
                }
                if self.wear >= -1 {
                    q.kind |= 2;
                }
                if self.attack >= 0 {
                    q.kind |= 1;
                }
                if !self.name.is_empty() {
                    q.kind |= 12;
                }
                q.text = self.name.clone();
                q.name = self.seller.clone();
                q.pa0 = match self.race {
                    -2 => 0,
                    -1 => 0xFFFF_FFFF,
                    r => 1u32 << r,
                };
                q.p9c = if self.class != -1 { 1u32 << self.class } else { 0xFFFF_FFFF };
                q.pbc = self.x4f4 != 0;
                q.pa4 = self.min_level;
                q.pa8 = self.max_level;
                q.pac = self.min_price;
                q.pb0 = self.max_price;
                q.pb4 = self.wear;
                q.pb8 = self.attack;
                out.push(SearchOut::Send(auction.search(q), 5));
                self.menu.list.reset_scroll();
            }
            _ => {}
        }
        cues.push(13);
    }

    /// `EditField`: the editor over a form row, or the bid.
    fn edit_field(&mut self, vui: &Vui, now: i64) {
        let w = |id: i32| wide(vui.words.get(id, vui.language));
        let e = &mut self.edit;
        e.set_rows(1);
        e.set_multi_line(false);
        e.set_center([320, 180]);
        e.set_max_length(128);
        let (n, title) = if self.state == 0 {
            match self.field {
                0 => (64, 1680),
                3 => (5, 1713),
                4 => (5, 1714),
                7 => (10, 1715),
                8 => (10, 1716),
                10 => (24, 1717),
                _ => return,
            }
        } else if self.state == 4 {
            (10, 1698)
        } else {
            return;
        };
        e.set_max_length(n);
        e.title = w(title);
        e.clear();
        e.focus(now);
        self.child = Child::Edit;
    }

    /// `TakeText`.
    fn take_text(&mut self, now: i64) {
        let s = String::from_utf16_lossy(&self.edit.text);
        if self.state == 0 {
            match self.field {
                0 => self.name = s,
                3 => self.min_level = atoi(&s),
                4 => self.max_level = atoi(&s),
                7 => self.min_price = atoi(&s),
                8 => self.max_price = atoi(&s),
                10 => self.seller = s.chars().take(23).collect(),
                _ => return,
            }
        } else if self.state == 4 {
            self.bid = atoi(&s);
        }
        self.child = Child::Own;
        let st = self.state;
        self.set_state(st, now);
    }

    /// `ShowMessage(text, p1, p2)`: the Yes/No box.
    fn show_message(&mut self, text: &str, p1: i32, now: i64) {
        self.mbox.prompt(&wide(text), 1, p1, 0, now);
        self.mbox.pos = [175, 115];
        self.child = Child::MBox;
    }

    /// `HasLot`: one of the player's own.
    fn has_lot(bank: &Bank, id: i64) -> bool {
        bank.lots.iter().any(|l| l.lot_id == id)
    }

    /// The keys: the children's, or `OnKeyDown` for the state.
    pub fn key(&mut self, vui: &Vui, r: &dyn Raster, auction: &mut Auction, v: &View, key: i32, down: bool, cues: &mut Vec<i32>, out: &mut Vec<SearchOut>) {
        let now = v.now;
        match self.child {
            Child::Edit => {
                if !down {
                    self.edit.key_up(key);
                    return;
                }
                match self.edit.key(vui, r, key, now, cues) {
                    EditUp::Done(_) => self.take_text(now),
                    EditUp::Cancel => {
                        self.child = Child::Own;
                        let st = self.state;
                        self.set_state(st, now);
                    }
                    EditUp::None => {}
                }
                return;
            }
            Child::MBox => {
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
                    // 268: Confirm (0 bid, 1 buyout), then the page
                    Some(BoxAnswer::Yes(p1, _)) => {
                        match p1 {
                            0 => out.push(SearchOut::Send(Auction::bid(self.lot, self.bid), 4)),
                            1 => {
                                if let Some(l) = self.sel_lot(auction, v) {
                                    out.push(SearchOut::Send(Auction::bid(self.lot, l.p2f8 as i32), 4));
                                }
                            }
                            _ => {}
                        }
                        self.child = Child::Own;
                        self.set_state(1, now);
                    }
                    Some(BoxAnswer::No(..)) => {
                        self.child = Child::Own;
                        let st = self.state;
                        self.set_state(st, now);
                    }
                    None => {}
                }
                return;
            }
            Child::Menu => {
                let popup = self.menu.sub.as_ref().is_some_and(|s| s.visible);
                if popup || matches!(key, 13 | 134 | 27 | 132 | 19 | 20 | 138 | 139) {
                    match self.menu.key(&self.menus, key, down, now, cues) {
                        ListUp::Back => out.push(SearchOut::Close),
                        ListUp::Action(msg, id) => {
                            // 261: Bid or Inspect, sound 13
                            cues.push(13);
                            match msg {
                                0xA804_0000 => {
                                    self.lot = id;
                                    self.set_state(2, now);
                                }
                                0xA804_0001 => eprintln!("auction: Inspect (DescribeAuctionItem) is not ported"),
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                    return;
                }
                // anything else goes up to the window (VIWnd::OnKeyDown)
            }
            Child::Own => {}
        }
        if !down {
            return;
        }
        match self.state {
            0 => match key {
                17 | 136 => {
                    if self.menus.list[self.menu.menu].entries.iter().any(|e| e.id >= 0) {
                        cues.push(11);
                        self.set_state(1, now);
                    } else {
                        cues.push(9);
                    }
                }
                19 | 138 => {
                    self.field = if self.field <= 0 { 11 } else { self.field - 1 };
                    cues.push(11);
                    self.window.reset_tick_pulse(now);
                }
                20 | 139 => {
                    self.field = if self.field >= 11 { 0 } else { self.field + 1 };
                    cues.push(11);
                    self.window.reset_tick_pulse(now);
                }
                13 | 134 => self.select(vui, auction, now, cues, out),
                27 | 132 => {
                    cues.push(8);
                    out.push(SearchOut::Close);
                }
                _ => {}
            },
            1 => match key {
                18 | 137 => {
                    if self.mode == 0 {
                        cues.push(11);
                        self.set_state(0, now);
                    } else {
                        cues.push(9);
                    }
                    self.window.reset_tick_pulse(now);
                }
                81 | 113 | 133 if self.mode == 0 => {
                    cues.push(13);
                    out.push(SearchOut::Send(auction.prev_page(), 5));
                    self.menu.list.reset_scroll();
                }
                69 | 101 | 135 if self.mode == 0 => {
                    cues.push(13);
                    out.push(SearchOut::Send(auction.next_page(), 5));
                    self.menu.list.reset_scroll();
                }
                27 | 132 if self.mode == 1 || self.mode == 2 => {
                    cues.push(8);
                    out.push(SearchOut::Close);
                }
                _ => {}
            },
            2 => match key {
                27 | 132 => {
                    cues.push(8);
                    self.set_state(1, now);
                }
                81 | 113 | 133 => {
                    cues.push(13);
                    out.push(SearchOut::Send(Auction::bidders(self.lot), 6));
                    self.set_state(3, now);
                }
                13 | 134 => {
                    let Some(l) = auction.results.get(self.menu.list.sel.max(0) as usize) else { return };
                    if l.p2fc - auction.get_minutes(now) < 0 {
                        out.push(SearchOut::Red(1776));
                        cues.push(9);
                    } else if Self::has_lot(v.bank, l.lot_id) {
                        cues.push(9);
                    } else {
                        cues.push(13);
                        self.set_state(4, now);
                    }
                }
                _ => {}
            },
            3 => match key {
                19 | 138 => {
                    if self.cursor <= 0 {
                        self.cursor = 0;
                        cues.push(9);
                    } else {
                        self.cursor -= 1;
                        cues.push(11);
                    }
                    self.list.reset_tick_pulse(now);
                }
                20 | 139 => {
                    if v.idle && auction.p7860 == 0 {
                        let n = auction.bidders.len() as i32;
                        if n == 0 {
                            self.cursor = 0;
                            cues.push(9);
                        } else if self.cursor >= n - 1 {
                            self.cursor = n - 1;
                            cues.push(9);
                        } else {
                            self.cursor += 1;
                            cues.push(11);
                        }
                        self.list.reset_tick_pulse(now);
                    }
                }
                27 | 132 => {
                    cues.push(8);
                    self.set_state(2, now);
                }
                _ => {}
            },
            4 => match key {
                19 | 138 => {
                    if self.cursor <= 0 {
                        cues.push(9);
                        self.cursor = 0;
                    } else {
                        cues.push(11);
                        self.cursor -= 1;
                    }
                    self.list.reset_tick_pulse(now);
                }
                20 | 139 => {
                    if self.cursor >= 2 {
                        cues.push(9);
                        self.cursor = 2;
                    } else {
                        cues.push(11);
                        self.cursor += 1;
                    }
                    self.list.reset_tick_pulse(now);
                }
                27 | 132 => {
                    cues.push(8);
                    self.set_state(2, now);
                }
                13 | 134 => {
                    cues.push(13);
                    let w = |id: i32| vui.words.get(id, vui.language).to_string();
                    match self.cursor {
                        0 => self.edit_field(vui, now),
                        1 | 2 => {
                            if Self::has_lot(v.bank, self.lot as i64) {
                                out.push(SearchOut::Red(1775));
                            } else if let Some(l) = auction.results.get(self.menu.list.sel.max(0) as usize) {
                                let text = if self.cursor == 1 {
                                    format!("{}: {}\n{}: {} {}\n{}", w(1723), l.item.name, w(1728), self.bid, w(479), w(1791))
                                } else {
                                    format!("{}: {}\n{}: {} {}\n{}", w(1723), l.item.name, w(1697), l.p2f8, w(479), w(1791))
                                };
                                self.show_message(&text, self.cursor - 1, now);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }

    /// `DrawRow`: a label, a (framed) value; ticked when selected.
    fn draw_row(w: &Window, vui: &Vui, r: &mut dyn Raster, row: i32, label: Option<&[u16]>, value: Option<&[u16]>, xa: i32, xb: i32, sel: bool, tick: bool, framed: bool) {
        let color = if sel { [1.0, 1.0, 0.0, w.open] } else { [1.0, 1.0, 1.0, w.open] };
        if let Some(a) = label {
            w.text_box(vui, r, a, xa, row, xb - xa, 1, color, 0);
        }
        if let Some(b) = value {
            if framed {
                w.draw_frame(vui, r, xb, row, w.size[0] - xb - 25, 1, 64);
            }
            w.text_box(vui, r, b, xb, row, w.size[0] - xb - 25, 1, color, 2);
        }
        if sel && tick {
            w.tick_r(vui, r, xa, row, true);
        }
    }

    /// `FormatTime`: the time left ("Ended" when not).
    fn format_time(vui: &Vui, minutes: i64, lines: bool) -> String {
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let (h, m) = (minutes / 60, minutes % 60);
        if minutes <= 0 {
            if h != 0 {
                if lines { format!("{}:\n{}h {}m\n", w(1718), -h, -m) } else { format!("{}: {}h {}m", w(1718), -h, -m) }
            } else if lines {
                format!("{}:\n{}m\n", w(1718), -m)
            } else {
                format!("{}: {}m", w(1718), -m)
            }
        } else if h != 0 {
            if lines { format!("{}:\n{}h {}m\n", w(1719), h, m) } else { format!("{}: {}h {}m", w(1719), h, m) }
        } else if lines {
            format!("{}:\n{} {}m\n", w(1719), w(1720), m)
        } else {
            format!("{}: {} {}m", w(1719), w(1720), m)
        }
    }

    /// `OnDrawSelf`: the backdrop and title, the form (search mode), the
    /// menu, the summary panel, the legends, the state's window, and the
    /// "searching" box.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, auction: &Auction, v: &View, cues: &mut Vec<i32>) {
        let now = v.now;
        self.fill_menu(auction, v);
        let wd = |id: i32| wide(vui.words.get(id, vui.language));
        crate::ui::window_backdrop(vui, r, 642);
        let title = wd(match self.mode {
            1 => 1674,
            2 => 1675,
            _ => 1678,
        });
        let tw = vui.string_width(r, &title) + 64;
        vui.blit_bar(r, &title, [1.0; 4], [320 - tw / 2, 10], 33, 34, tw, 0, 1.0, 0, true);
        self.window.init(vui, r, [300, 180], 290, 14, 0);
        self.window.set_title(Some(&[]));
        self.window.draw(vui, r);
        let cols = [30, 140];
        self.form_hits.clear();
        self.list_hits.clear();
        if self.mode == 0 {
            let w = &self.window;
            let f = self.field;
            let own = self.state == 0;
            let ww = w.size[0];
            Self::draw_row(w, vui, r, 0, Some(&wd(1679)), None, cols[0], ww - 20, false, false, true);
            Self::draw_row(w, vui, r, 1, Some(&wd(1680)), Some(&wide(&self.name)), cols[0], cols[1], f == 0, own, true);
            // class, race, wear and attack: the pickers are not ported, so these
            // are the form's "any"
            self.class = self.class.clamp(-1, 15);
            Self::draw_row(w, vui, r, 2, Some(&wd(288)), Some(&wd(471)), cols[0], cols[1], f == 1, own, true);
            self.race = self.race.clamp(-2, 10);
            Self::draw_row(w, vui, r, 3, Some(&wd(289)), Some(&wd(if self.race == -2 { 302 } else { 471 })), cols[0], cols[1], f == 2, own, true);
            self.min_level = self.min_level.clamp(0, 60);
            Self::draw_row(w, vui, r, 4, Some(&wd(1681)), Some(&wide(&self.min_level.to_string())), cols[0], cols[1], f == 3, own, true);
            self.max_level = self.min_level.max(self.max_level).min(60);
            Self::draw_row(w, vui, r, 5, Some(&wd(1682)), Some(&wide(&self.max_level.to_string())), cols[0], cols[1], f == 4, own, true);
            Self::draw_row(w, vui, r, 6, Some(&wd(1683)), Some(&wd(471)), cols[0], cols[1], f == 5, own, true);
            Self::draw_row(w, vui, r, 7, Some(&wd(1684)), Some(&wd(471)), cols[0], cols[1], f == 6, own, true);
            self.min_price = self.min_price.clamp(0, 999_999_999);
            Self::draw_row(w, vui, r, 8, Some(&wd(1685)), Some(&wide(&self.min_price.to_string())), cols[0], cols[1], f == 7, own, true);
            self.max_price = self.min_price.max(self.max_price).min(999_999_999);
            Self::draw_row(w, vui, r, 9, Some(&wd(1686)), Some(&wide(&self.max_price.to_string())), cols[0], cols[1], f == 8, own, true);
            Self::draw_row(w, vui, r, 10, Some(&wd(803)), Some(&wd(if self.x4f4 != 0 { 1688 } else { 1689 })), cols[0], cols[1], f == 9, own, true);
            Self::draw_row(w, vui, r, 11, Some(&wd(1690)), Some(&wide(&self.seller)), cols[0], cols[1], f == 10, own, true);
            Self::draw_row(w, vui, r, 12, Some(&wd(509)), None, cols[0], cols[1], f == 10, false, true);
            Self::draw_row(w, vui, r, 13, Some(&wd(1673)), None, cols[0], cols[1], f == 11, own, true);
            // the fields' rows: field k on row k + 1, the seller's two, Search on 13
            for (field, row, rows) in (0..10).map(|k| (k, k + 1, 1)).chain([(10, 11, 2), (11, 13, 1)]) {
                self.form_hits.push(rows_rect(w, vui, r, cols[0], row, ww - 25 - cols[0], rows), field);
            }
        }
        self.menu.visible = true;
        self.menu.draw(vui, r, &self.menus, now, cues);
        self.window.init(vui, r, [536, 180], 200, 14, 2);
        self.window.set_title(Some(&[]));
        self.window.draw(vui, r);
        vui.blit_horz(r, [16, 322], 60, 619, 0, 1.0, 0);
        if (1..=4).contains(&self.state) {
            self.draw_summary(vui, r, auction, v);
        }
        let ww = self.window.size[0];
        let white = [1.0, 1.0, 1.0, 1.0];
        if self.mode == 0 {
            let s = if self.state == 1 { format!("|s{} |c{}", vui.words.get(1692, vui.language), vui.words.get(216, vui.language)) } else { format!("|-|s{} |c{}", vui.words.get(1692, vui.language), vui.words.get(216, vui.language)) };
            self.window.text_box(vui, r, &wide(&s), 16, 12, ww - 32, 1, white, 1);
        }
        let s = format!("|x{} |t{}", vui.words.get(212, vui.language), vui.words.get(161, vui.language));
        self.window.text_box(vui, r, &wide(&s), 16, 13, ww - 32, 1, white, 1);
        match self.state {
            2 => self.draw_lot(vui, r, auction, v),
            3 => self.draw_rankings(vui, r, auction, v),
            4 => self.draw_bid(vui, r, auction, v),
            _ => {}
        }
        if v.searching {
            self.status.init(vui, r, [320, 180], 300, 3, 0);
            self.status.set_title(Some(&wd(271)));
            self.status.open(false);
            self.status.draw(vui, r);
            self.ticks -= 1;
            if self.ticks <= 0 {
                self.ticks = 15;
                self.dots += 1;
                if self.dots == 3 {
                    self.dots = 0;
                }
            }
            let mut s = vui.words.get(1693, vui.language).to_string();
            for _ in 0..self.dots + 1 {
                s.push('.');
            }
            let sw = self.status.size[0];
            self.status.text_box(vui, r, &wide(&s), 40, 1, sw - 40, 1, [1.0, 1.0, 1.0, self.status.open], 0);
        } else {
            self.status.close(true);
        }
        match self.child {
            Child::Edit => self.edit.draw(vui, r, now, cues),
            Child::MBox => {
                self.mbox.draw(vui, r, now);
            }
            _ => {}
        }
    }

    /// `DrawSummary`: the selected lot on the right panel.
    fn draw_summary(&self, vui: &Vui, r: &mut dyn Raster, auction: &Auction, v: &View) {
        if v.searching || auction.p7860 != 0 {
            return;
        }
        let sel = self.menu.list.sel;
        let Some(l) = auction.results.get(sel.max(0) as usize).filter(|_| sel >= 0) else { return };
        let i = sel as usize;
        let col = |k: usize| auction.cols[k].get(i).copied().unwrap_or(0);
        let (owner, bid, bids) = (col(0), col(1), col(2));
        let left = l.p2fc - auction.get_minutes(v.now);
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let mut t = format!("{}\n", l.item.name);
        t += &format!("{}:\n", w(if left <= 0 { 1706 } else { 1707 }));
        if bid == -1 {
            t += &format!("{}\n", w(1708));
        } else if owner == v.player {
            t += &format!("{} {}\n", bid, w(1709));
        } else {
            t += &format!("{bid}\n");
        }
        if left > 0 {
            t += &format!("{}:\n{}\n", w(1710), bids);
        }
        t += &Self::format_time(vui, left, true);
        if let Some((a, b)) = Auction::find_bid(v.bank, l.lot_id) {
            t += &format!("{}:\n{}\n{}:\n{}\n", w(1711), a, w(1712), b);
        }
        let ww = self.window.size[0] - 28;
        self.window.text_box(vui, r, &wide(&t), 16, 0, ww, 12, [1.0; 4], 3);
    }

    /// `DrawLot` (state 2).
    fn draw_lot(&mut self, vui: &Vui, r: &mut dyn Raster, auction: &Auction, v: &View) {
        let sel = self.menu.list.sel;
        let Some(l) = self.sel_lot(auction, v).cloned() else { return };
        let _ = sel;
        let left = l.p2fc - auction.get_minutes(v.now);
        self.list.init(vui, r, [320, 180], 375, 10, 0);
        let wd = |id: i32| wide(vui.words.get(id, vui.language));
        if self.list.is_open() || self.list.is_opening() || self.list.is_closing() {
            self.list.set_title(Some(&wd(1677)));
            self.list.draw(vui, r);
        }
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let mut t = format!("{}\n", l.item.name);
        t += &format!("{}: {}\n", w(1777), l.seller);
        t += &format!("{}: {}/{}\n", w(233), l.cur_hp, l.item.max_hp);
        t += &format!("{}: {}\n", w(1685), l.p2f4);
        t += &format!("{}: {}\n", w(1686), l.p2f8);
        t += &Self::format_time(vui, left, false);
        let lw = self.list.size[0] - 40;
        let c = [1.0, 1.0, 1.0, self.list.open];
        self.list.text_box(vui, r, &wide(&t), 20, 0, lw, 8, c, 2);
        let legend = if left < 0 && self.mode != 1 {
            format!("|s{}  |-|x{}  |+|t{}", w(1694), w(1695), w(161))
        } else if self.mode == 1 || Self::has_lot(v.bank, l.lot_id) {
            format!("|s{}  |t{}", w(1694), w(161))
        } else {
            format!("|s{}  |x{}  |t{}", w(1694), w(1695), w(161))
        };
        self.list.text_box(vui, r, &wide(&legend), 20, 9, lw, 1, c, 0);
    }

    /// `DrawRankings` (state 3): the bidders, seven at a time.
    fn draw_rankings(&mut self, vui: &Vui, r: &mut dyn Raster, auction: &Auction, v: &View) {
        self.list.init(vui, r, [320, 180], 450, 10, 0);
        let wd = |id: i32| wide(vui.words.get(id, vui.language));
        if self.list.is_open() || self.list.is_opening() || self.list.is_closing() {
            self.list.set_title(Some(&wd(1696)));
            self.list.draw(vui, r);
        }
        if !v.idle || auction.p7860 != 0 {
            return;
        }
        if self.cursor - self.top >= 7 {
            self.top = self.cursor - 6;
        } else if self.cursor < self.top {
            self.top = self.cursor;
        }
        let cols = [40, 270];
        Self::draw_row(&self.list, vui, r, 0, Some(&wd(509)), Some(&wd(1697)), cols[0], cols[1], false, false, false);
        let n = auction.bidders.len() as i32;
        let t = if n == 7 { 0.0 } else { self.top as f32 / (n - 7) as f32 };
        let lw = self.list.size[0];
        self.list.draw_frame_vscroll(vui, r, 22, 1, lw - 55, 7, t);
        let mut row = 1;
        let mut i = self.top;
        while i < n && row < 8 {
            let b = &auction.bidders[i as usize];
            let sel = i == self.cursor;
            Self::draw_row(&self.list, vui, r, row, Some(&wide(&b.name)), Some(&wide(&b.x24.to_string())), cols[0], cols[1], sel, sel, false);
            self.list_hits.push(rows_rect(&self.list, vui, r, 22, row, lw - 55, 1), i);
            i += 1;
            row += 1;
        }
        let s = wide(&format!("|t{}", vui.words.get(161, vui.language)));
        self.list.text_box(vui, r, &s, 20, 9, lw - 40, 1, [1.0, 1.0, 1.0, self.list.open], 0);
    }

    /// `DrawBid` (state 4): the amount clamped to above the high bid and at
    /// most the buyout, Bid, the buyout line.
    fn draw_bid(&mut self, vui: &Vui, r: &mut dyn Raster, auction: &Auction, v: &View) {
        self.list.init(vui, r, [320, 180], 430, 8, 0);
        let wd = |id: i32| wide(vui.words.get(id, vui.language));
        if self.list.is_open() || self.list.is_opening() || self.list.is_closing() {
            self.list.set_title(Some(&wd(1698)));
            self.list.draw(vui, r);
        }
        let cols = [30, 150];
        let Some(l) = self.sel_lot(auction, v).cloned() else { return };
        let high = auction.cols[1].get(self.menu.list.sel as usize).copied().unwrap_or(0) as i32;
        let min = (l.p2f4 as i32).max(high.wrapping_add(1));
        self.bid = self.bid.max(min).min(l.p2f8 as i32);
        let lw = self.list.size[0];
        let c = self.cursor;
        Self::draw_row(&self.list, vui, r, 1, Some(&wd(1728)), Some(&wide(&self.bid.to_string())), cols[0], cols[1], c == 0, true, true);
        Self::draw_row(&self.list, vui, r, 3, Some(&wd(1699)), None, cols[0], lw - 40, c == 1, true, true);
        Self::draw_row(&self.list, vui, r, 4, Some(&wd(1700)), None, cols[0], cols[1], false, false, true);
        let buy = wide(&vui.words.get(1701, vui.language).replacen("%d", &l.p2f8.to_string(), 1));
        Self::draw_row(&self.list, vui, r, 5, Some(&buy), None, cols[0], lw - 40, c == 2, true, true);
        for (cursor, row) in [(0, 1), (1, 3), (2, 5)] {
            self.list_hits.push(rows_rect(&self.list, vui, r, cols[0], row, lw - 40 - cols[0], 1), cursor);
        }
        let s = wide(&format!("|x{}   |t{}", vui.words.get(212, vui.language), vui.words.get(161, vui.language)));
        self.list.text_box(vui, r, &s, 20, 7, lw - 40, 1, [1.0, 1.0, 1.0, self.list.open], 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::dump::{Bid, Item};

    #[derive(Default)]
    struct NullRaster;

    impl Raster for NullRaster {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
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

    fn lot(id: i64, ends: i64) -> AuctionItem {
        AuctionItem { lot_id: id, quantity: 1, p2f4: 100, p2f8: 300, p2fc: ends, seller: "S".into(), item: Item { name: "Sword".into(), wear_slot: -1, ..Default::default() }, ..Default::default() }
    }

    struct Rig {
        w: SearchWnd,
        a: Auction,
        bank: Bank,
        vui: Vui,
        cues: Vec<i32>,
        out: Vec<SearchOut>,
    }

    impl Rig {
        fn new() -> Self {
            let vui = Vui::new(Default::default(), Default::default(), Vec::new());
            let mut w = SearchWnd::default();
            let mut a = Auction::new();
            let bank = Bank::default();
            w.open(&vui, &NullRaster, 0xA801_0000, &mut a, &bank, 0);
            Rig { w, a, bank, vui, cues: Vec::new(), out: Vec::new() }
        }

        /// A page as RecvResults leaves it, the menu filled.
        fn page(&mut self, lots: Vec<AuctionItem>) {
            let n = lots.len();
            self.a.results = lots;
            self.a.cols = [vec![0; n], vec![-1; n], vec![0; n]];
            self.a.minutes = 0;
            self.a.arrived = 0;
            self.a.p7860 = 0;
            let (icon, can_use) = (|_: &NetItem| 7, |_: &NetItem| true);
            let v = View { bank: &self.bank, idle: true, searching: false, player: 1, now: 0, icon: &icon, can_use: &can_use };
            self.w.fill_menu(&self.a, &v);
        }

        fn key(&mut self, k: i32) {
            let (icon, can_use) = (|_: &NetItem| 7, |_: &NetItem| true);
            let v = View { bank: &self.bank, idle: true, searching: false, player: 1, now: 0, icon: &icon, can_use: &can_use };
            self.w.key(&self.vui, &NullRaster, &mut self.a, &v, k, true, &mut self.cues, &mut self.out);
            self.w.key(&self.vui, &NullRaster, &mut self.a, &v, k, false, &mut self.cues, &mut self.out);
        }

        fn draw(&mut self) {
            let (icon, can_use) = (|_: &NetItem| 7, |_: &NetItem| true);
            let v = View { bank: &self.bank, idle: true, searching: false, player: 1, now: 0, icon: &icon, can_use: &can_use };
            self.w.draw(&self.vui, &mut NullRaster, &self.a, &v, &mut self.cues);
        }
    }

    /// The form's fields take a click and a second edits; from the form a
    /// click on the page's rows goes left to them with the row selected, and
    /// from the page one on the form goes right back with the field selected;
    /// the menu then takes its own; the bid form's rows are the cursor's.
    #[test]
    fn a_pointer_across_the_search() {
        use crate::pointer::testing::{mid, vui};
        let mut g = Rig::new();
        // the geometry measured with a Vui of its own (the rig's is borrowed
        // by its draws); the font is the raster's either way
        let gv = vui();
        g.page(vec![lot(5, 100), lot(6, 100), lot(7, 100)]);
        g.draw();
        // the form's window as the draw sets it up (it is the summary's after)
        let mut form = Window::default();
        form.init(&gv, &NullRaster, [300, 180], 290, 14, 0);
        form.set_title(Some(&[]));
        let field = |f: i32, rows: i32| mid(rows_rect(&form, &gv, &NullRaster, 30, f + 1, 290 - 25 - 30, rows));
        // the page's rows at (16, 37), 44 high (no atlas here)
        let row = |i: i32| [16.0 + 57.0, 37.0 + 44.0 * i as f32 + 22.0];
        assert_eq!(g.w.point(field(3, 1)), vec![MOVED]);
        assert_eq!(g.w.field, 3, "the minimum level");
        assert_eq!(g.w.point(field(3, 1)), vec![key::CROSS]);
        assert_eq!(g.w.point(field(10, 2)), vec![MOVED], "the seller's two rows");
        assert_eq!(g.w.field, 10);
        assert_eq!(g.w.point(row(1)), vec![key::LEFT], "onto the page");
        assert_eq!(g.w.menu.list.sel, 1);
        assert!(g.w.point(row(3)).is_empty(), "three lots");
        g.key(key::LEFT);
        assert_eq!((g.w.state, g.w.child), (1, Child::Menu));
        g.draw();
        // the page has the keys: its rows are the menu's
        assert_eq!(g.w.point(row(2)), vec![MOVED]);
        assert_eq!(g.w.point(row(2)), vec![key::CROSS]);
        assert_eq!(g.w.point(field(5, 1)), vec![key::RIGHT], "back to the form");
        assert_eq!(g.w.field, 5);
        g.key(key::RIGHT);
        assert_eq!((g.w.state, g.w.child), (0, Child::Own));
        // the bid form (a lot of the page): its three rows by the cursor
        g.w.menu.list.sel = 0;
        g.w.lot = 5;
        g.w.set_state(1, 0);
        g.w.set_state(2, 0);
        g.w.set_state(4, 0);
        g.draw();
        let mut bid = Window::default();
        bid.init(&gv, &NullRaster, [320, 180], 430, 8, 0);
        bid.set_title(Some(&wide("x")));
        let at = |row: i32| mid(rows_rect(&bid, &gv, &NullRaster, 30, row, 430 - 40 - 30, 1));
        assert_eq!(g.w.point(at(5)), vec![MOVED]);
        assert_eq!(g.w.cursor, 2, "the buyout");
        assert_eq!(g.w.point(at(5)), vec![key::CROSS]);
        assert!(g.w.point(at(4)).is_empty(), "the line between is no row");
        assert_eq!(g.w.cursor, 2);
    }

    #[test]
    fn the_form_searches_with_its_query() {
        let mut g = Rig::new();
        // up from the first row wraps to Search (11)
        g.key(138);
        assert_eq!(g.w.field, 11);
        g.key(134);
        let Some(SearchOut::Send(req, 5)) = g.out.first().cloned() else { panic!("{:?}", g.out) };
        assert_eq!(&req[..3], &[0x05, 0x12, 7]);
        // the default form: no price bound (16 not set: 0x7FFFFFFF is past
        // 999999999 only after the draw clamps it), nothing else
        let q = &g.a.query;
        assert_eq!((q.kind, q.p9c, q.pa0, q.pa4, q.pa8, q.pbc), (0, 0xFFFF_FFFF, 0xFFFF_FFFF, 1, 50, true));
        // negative control: with the max price clamped by a draw, kind 16
        g.w.max_price = 999_999;
        g.out.clear();
        g.key(134);
        assert_eq!(g.a.query.kind, 16);
        // left with no page: sound 9, the form stays
        g.cues.clear();
        g.key(136);
        assert_eq!((g.w.state, g.cues.clone()), (0, vec![9]));
    }

    #[test]
    fn a_lot_and_its_bid() {
        let mut g = Rig::new();
        g.page(vec![lot(5, 100), lot(6, -1)]);
        g.key(136);
        assert_eq!(g.w.state, 1, "left onto the page");
        // Enter: the popup (Inspect, Bid); down, Enter: Bid -> the lot
        g.key(134);
        // (the popup's down moves it in the draw; here the selection is set)
        g.w.menu.sub.as_mut().expect("the popup").list.sel = 1;
        g.key(134);
        assert_eq!((g.w.state, g.w.lot), (2, 5));
        // Enter on a live lot that is not ours: the bid form
        g.key(134);
        assert_eq!(g.w.state, 4);
        // down to Bid, Enter: the box; left to Yes, Enter: Send06(5, bid)
        g.w.bid = 150;
        g.key(139);
        g.key(134);
        assert_eq!(g.w.child, Child::MBox);
        g.key(136);
        g.out.clear();
        g.key(134);
        assert_eq!(g.out, vec![SearchOut::Send(Auction::bid(5, 150), 4)]);
        assert_eq!(g.w.state, 1, "the page again");
        // an expired lot: word 1776 in red, sound 9
        g.w.menu.list.sel = 1;
        g.w.lot = 6;
        g.w.set_state(2, 0);
        g.out.clear();
        g.cues.clear();
        g.key(134);
        assert_eq!((g.out.clone(), g.cues.clone(), g.w.state), (vec![SearchOut::Red(1776)], vec![9], 2));
        // one of our own: sound 9 and no bid form (negative control on HasLot)
        g.bank.lots.push(lot(5, 100));
        g.w.menu.list.sel = 0;
        g.cues.clear();
        g.key(134);
        assert_eq!((g.cues.clone(), g.w.state), (vec![9], 2));
        // the bid row on our own lot: word 1775
        g.w.set_state(4, 0);
        g.w.lot = 5;
        g.w.cursor = 1;
        g.out.clear();
        g.key(134);
        assert_eq!(g.out, vec![SearchOut::Red(1775)]);
        let _ = Bid::default();
    }

    #[test]
    fn my_lots_asks_for_the_banks_ids() {
        let vui = Vui::new(Default::default(), Default::default(), Vec::new());
        let mut w = SearchWnd::default();
        let mut a = Auction::new();
        let mut bank = Bank::default();
        bank.lots = vec![lot(11, 9), lot(12, 9)];
        let req = w.open(&vui, &NullRaster, 0xA802_0000, &mut a, &bank, 0).expect("a request");
        assert_eq!(req, vec![0x05, 0x12, 8, 2, 4, 22, 24]);
        assert_eq!((w.mode, w.state), (1, 1));
        // the search kind asks nothing
        let mut w2 = SearchWnd::default();
        assert!(w2.open(&vui, &NullRaster, 0xA801_0000, &mut a, &bank, 0).is_none());
        assert_eq!((w2.mode, w2.state), (0, 0));
    }
}
