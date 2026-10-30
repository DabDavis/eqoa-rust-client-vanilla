//! The auction search (CLIENT `viwndauctionsearch.cpp`, `VIWndAuctionSearch`,
//! the game window's mode 39), opened from the start menu's bank tables
//! (0xA8010000 search, 0xA8020000 my lots, 0xA8030000 my bids). Its states:
//! 0 the search form (name, class, race, the level and price ranges, wear
//! slot, attack type, a flag, the seller, Search), 1 the result page in a
//! `VIWndMenu` (each lot with Inspect and Bid), 2 a lot, 3 its bidders, 4
//! the bid form (an amount, Bid, Buyout); 5..8 are the race, class, wear
//! slot and attack type pickers (Enter on the form's race, class, wear slot
//! and attack type rows), seven rows of a list at a time.
//!
//! The page's Inspect describes the selected lot (`DescribeAuctionItem`'s
//! four describers, `infotext.rs`) in a `VIWndInspect` over the window: △
//! closes it and the page has the keys again, the arrows scroll it.
//!
//! A pointer (`pointer`) goes to the child with the keys (the editor, the
//! box, the page's menu and its popup); a click on the form from the page
//! goes right to it, and one on the page's rows from the form left to them,
//! the field or the row under the pointer selected; the form's fields, the
//! bidders, the bid form's rows and the pickers' rows are selected by a
//! click and used by a second.

#![allow(clippy::too_many_arguments)]

use crate::auction::{Auction, Query, atoi};
use crate::client_image::{Menu, MenuEntry, Menus};
use crate::dialog::{BoxAnswer, Key as BoxKey, MBox};
use crate::editbox::{EditBox, EditUp};
use crate::infotext::Info;
use crate::itemmenu::Inspect;
use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::wndlist::{ListUp, WndMenu};
use net::dump::{AuctionItem, Bank, ItemList, NetItem};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// The pickers' rows (`sRaces`, `sClasses`, `sWears`, `sAttacks`, CLIENT
/// 0x00CE1380 .. 0x00CE148C): the value each row stands for. -1 is "any"
/// (and the wear slot's -1 its "none"), the race's -2 "none", the wear
/// slot's -2 "any".
const RACES: [i32; 12] = [-1, -2, 6, 2, 4, 1, 8, 3, 7, 0, 9, 5];
const CLASSES: [i32; 16] = [-1, 14, 5, 9, 7, 12, 10, 4, 11, 2, 1, 6, 3, 8, 0, 13];
const WEARS: [i32; 27] = [-2, -1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 22, 23, 24, 25, 27];
const ATTACKS: [i32; 11] = [-1, 0, 1, 2, 3, 4, 5, 6, 7, 9, 10];

/// `gRaces[r].m_nameStrID` (`virace.cpp`), the ten races.
const RACE_WORDS: [i32; 10] = [183, 185, 186, 187, 188, 189, 191, 192, 193, 190];

/// `GetClass(c)->m_nameStrID` (`viclass.cpp`: 165..179; out of range is
/// class 0's).
pub(crate) fn class_word(c: i32) -> i32 {
    if (0..15).contains(&c) { 165 + c } else { 165 }
}

/// `GetRace(r)->m_nameStrID` (`virace.cpp`: the ten races, then eleven
/// empty slots whose name is -1; out of range is race 0's).
pub(crate) fn race_word(r: i32) -> i32 {
    match r {
        0..=9 => RACE_WORDS[r as usize],
        10..=20 => -1,
        _ => RACE_WORDS[0],
    }
}

/// A picker (states 5..8): its rows and its list's title. Its list scrolls
/// over the rows past the seven shown (`m_top / (rows - 7)`).
struct Picker {
    rows: Vec<i32>,
    title: i32,
}

/// The picker of state `s`: 5 the races (`DrawRaces`), 6 the classes
/// (`DrawClasses`), 7 the wear slots (`DrawTypes`), 8 the attack types
/// (`DrawSlots`). On a server that sends the live client's feature word
/// without its bit 0x02 (`live`), race 9 and class 14 are left out of their
/// lists, as the live client builds them (0x01314D98).
fn picker(s: i32) -> Option<Picker> {
    let f = crate::live::features();
    match s {
        5 => Some(Picker { rows: RACES.iter().copied().filter(|&r| f.shows_race(r)).collect(), title: 1703 }),
        6 => Some(Picker { rows: CLASSES.iter().copied().filter(|&c| f.shows_class(c)).collect(), title: 1702 }),
        7 => Some(Picker { rows: WEARS.to_vec(), title: 1704 }),
        8 => Some(Picker { rows: ATTACKS.to_vec(), title: 1705 }),
        _ => None,
    }
}

/// What the window asks of the client.
#[derive(Clone, Debug, PartialEq)]
pub enum SearchOut {
    /// a 0x1205 request; `Some(state)` is what it sets `m_state` to
    Send(Vec<u8>, i32),
    /// `ClosePanel718`: `CloseAuctionSearch`
    Close,
    /// `AddText(word)` in red
    Red(i32),
    /// `AddText` in red of a word whose `%d` is the count (the live
    /// client's word 1858)
    RedCount(i32, i64),
}

/// Which child has the keys.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Child {
    #[default]
    Own,
    Menu,
    Edit,
    MBox,
    Inspect,
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
    /// `gAttackWords` (CLIENT's data): `FindAttackWord`'s table
    pub attack_words: &'a [(i32, i64)],
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
    /// `m_bidQty` (the live client's +0x5B4): the items of the lot bid for
    bid_qty: i32,
    dots: i32,
    ticks: i32,
    menu: WndMenu,
    menus: Menus,
    edit: EditBox,
    mbox: MBox,
    inspect: Inspect,
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
            // the Auctions window's Max. Level (`li a1, 50`; a server's
            // patch may change it: Sandstorm's S2 opens on 60)
            max_level: crate::s2::get().auction_max_level,
            min_price: 0,
            max_price: 0x7FFF_FFFF,
            wear: -2,
            attack: -1,
            x4f4: 1,
            seller: String::new(),
            lot: -1,
            bid: 0,
            bid_qty: 1,
            dots: 0,
            ticks: 15,
            menu,
            menus,
            edit: EditBox::default(),
            mbox: MBox::default(),
            inspect: Inspect::default(),
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
    /// bidders are selected. The inspect window none (its legend is a
    /// glyph, the wheel scrolls it). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.child {
            Child::Inspect => return Vec::new(),
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
            // the bid form's rows by the cursor, a picker's by its row
            4..=8 => match self.list_hits.at(p) {
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
                    self.bid_qty = 1;
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

    /// `Inspect(lot)`: the lot under the menu's selection (when the bank is
    /// idle and has a page) described in the inspect window, which takes
    /// the keys; the box and the editor put away.
    fn inspect_lot(&mut self, vui: &Vui, r: &mut dyn Raster, auction: &Auction, v: &View, lot: i32) {
        self.lot = lot;
        let Some(l) = self.sel_lot(auction, v) else { return };
        let (inv, base) = (ItemList::default(), [0; 29]);
        let info = Info { words: &vui.words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 0, attack_words: v.attack_words };
        let text = info.auction_item(l);
        self.inspect.set(1, -1, &text);
        self.inspect.visible = true;
        self.inspect.focus(vui, r);
        self.child = Child::Inspect;
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
            // the pickers: class, race, wear slot, attack type
            1 => self.set_state(6, now),
            2 => self.set_state(5, now),
            5 => self.set_state(7, now),
            6 => self.set_state(8, now),
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
            // the live client's quantity first (`live`, 0x01314230)
            if crate::live::features().live() && self.cursor == 0 { (10, 1873) } else { (10, 1698) }
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
            if crate::live::features().live() && self.cursor == 0 {
                self.bid_qty = atoi(&s);
            } else {
                self.bid = atoi(&s);
            }
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
    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, auction: &mut Auction, v: &View, key: i32, down: bool, cues: &mut Vec<i32>, out: &mut Vec<SearchOut>) {
        let now = v.now;
        match self.child {
            // 260 from the inspect window: hidden, the state's keys again
            Child::Inspect => {
                if self.inspect.key(key, down, cues) {
                    self.inspect.visible = false;
                    self.child = Child::Own;
                    let st = self.state;
                    self.set_state(st, now);
                }
                return;
            }
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
                        // the live client's (`live`, 0x01314D00): the count
                        // asked for, and the buy-out the lot's whole stack
                        let live = crate::live::features().live();
                        match p1 {
                            0 if live => out.push(SearchOut::Send(Auction::bid_n(self.lot, self.bid, self.bid_qty), 4)),
                            0 => out.push(SearchOut::Send(Auction::bid(self.lot, self.bid), 4)),
                            1 => {
                                if let Some(l) = self.sel_lot(auction, v) {
                                    let count = if live { l.quantity as i32 } else { 1 };
                                    out.push(SearchOut::Send(Auction::bid_n(self.lot, l.p2f8 as i32, count), 4));
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
                                0xA804_0001 => self.inspect_lot(vui, r, auction, v, id),
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
            4 => {
                // the live client's form (`live`): the quantity first, four
                // rows (0x01310D30)
                let live = crate::live::features().live();
                let last = if live { 3 } else { 2 };
                match key {
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
                        if self.cursor >= last {
                            cues.push(9);
                            self.cursor = last;
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
                    13 | 134 if live => {
                        cues.push(13);
                        self.live_bid_key(vui, auction, v, now, out);
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
                }
            }
            // the pickers: up and down stop at the ends (sound 9), Enter takes
            // the row's value into the form's field, Escape leaves it
            5..=8 => {
                let Some(pk) = picker(self.state) else { return };
                let last = pk.rows.len() as i32 - 1;
                match key {
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
                        if self.cursor >= last {
                            cues.push(9);
                            self.cursor = last;
                        } else {
                            cues.push(11);
                            self.cursor += 1;
                        }
                        self.list.reset_tick_pulse(now);
                    }
                    13 | 134 => {
                        if let Some(&v) = usize::try_from(self.cursor).ok().and_then(|c| pk.rows.get(c)) {
                            match self.state {
                                5 => self.race = v,
                                6 => self.class = v,
                                7 => self.wear = v,
                                _ => self.attack = v,
                            }
                        }
                        cues.push(13);
                        self.set_state(0, now);
                    }
                    27 | 132 => {
                        cues.push(8);
                        self.set_state(0, now);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// The live client's Enter on its bid form (`live`, 0x01310D30 state 4):
    /// the quantity's and the bid's editors; the proxy bid (not on one's own
    /// lot, word 1775; the buy-out price only for the lot's whole stack,
    /// else word 1858 with its count and nothing asked) and the buy-out,
    /// each asked in a box whose quantity line is there with the feature
    /// word's bit 0x08 (the buy-out's box says the quantity on the form, as
    /// the live client's does, though it buys the whole stack).
    fn live_bid_key(&mut self, vui: &Vui, auction: &Auction, v: &View, now: i64, out: &mut Vec<SearchOut>) {
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        match self.cursor {
            0 | 1 => self.edit_field(vui, now),
            c @ (2 | 3) => {
                if Self::has_lot(v.bank, self.lot as i64) {
                    out.push(SearchOut::Red(1775));
                    return;
                }
                let Some(l) = auction.results.get(self.menu.list.sel.max(0) as usize) else { return };
                if c == 2 && self.bid as i64 == l.p2f8 && self.bid_qty as i64 != l.quantity {
                    out.push(SearchOut::RedCount(1858, l.quantity));
                    return;
                }
                let qty = if crate::live::features().has(crate::live::Features::STACKS) { format!("{}: {}\n", w(1853), self.bid_qty) } else { String::new() };
                let (word, price) = if c == 2 { (1728, self.bid as i64) } else { (1697, l.p2f8) };
                let text = format!("{}: {}\n{qty}{}: {} {}\n{}", w(1723), l.item.name, w(word), price, w(479), w(1791));
                self.show_message(&text, c - 2, now);
            }
            _ => {}
        }
    }

    /// The live client's `DrawBid` (`live`, 0x01312D10): nine rows titled
    /// 1695; the quantity (held to the bid's own count, else 1, up to the
    /// lot's), the proxy bid (from the floor, the lot's minimum or the page's
    /// third column whichever is higher, to the buy-out), Submit, OR, and
    /// the buy-out (word 1701 for a lot of one, 1857 each for a stack).
    fn draw_bid_live(&mut self, vui: &Vui, r: &mut dyn Raster, auction: &Auction, v: &View) {
        self.list.init(vui, r, [320, 180], 430, 9, 0);
        let wd = |id: i32| wide(vui.words.get(id, vui.language));
        if self.list.is_open() || self.list.is_opening() || self.list.is_closing() {
            self.list.set_title(Some(&wd(1695)));
            self.list.draw(vui, r);
        }
        let cols = [30, 150];
        let Some(l) = self.sel_lot(auction, v).cloned() else { return };
        let floor_col = auction.cols[2].get(self.menu.list.sel as usize).copied().unwrap_or(0) as i32;
        let least = Auction::find_bid3(v.bank, l.lot_id).map_or(1, |b| b.2 as i32);
        self.bid_qty = self.bid_qty.max(least).min(l.quantity as i32);
        let floor = (l.p2f4 as i32).max(floor_col);
        self.bid = self.bid.max(floor).min(l.p2f8 as i32);
        let lw = self.list.size[0];
        let c = self.cursor;
        Self::draw_row(&self.list, vui, r, 1, Some(&wd(1853)), Some(&wide(&self.bid_qty.to_string())), cols[0], cols[1], c == 0, true, true);
        Self::draw_row(&self.list, vui, r, 2, Some(&wd(1728)), Some(&wide(&self.bid.to_string())), cols[0], cols[1], c == 1, true, true);
        Self::draw_row(&self.list, vui, r, 4, Some(&wd(1699)), None, cols[0], lw - 40, c == 2, true, true);
        Self::draw_row(&self.list, vui, r, 5, Some(&wd(1700)), None, cols[0], cols[1], false, false, true);
        // 1701 as the live client words it ("Buy now", the disc's "Acquire
        // now": `livewords::live`)
        let buy = crate::livewords::live(&vui.words, if l.quantity == 1 { 1701 } else { 1857 }, vui.language).replacen("%d", &l.p2f8.to_string(), 1);
        Self::draw_row(&self.list, vui, r, 6, Some(&wide(&buy)), None, cols[0], lw - 40, c == 3, true, true);
        for (cursor, row) in [(0, 1), (1, 2), (2, 4), (3, 6)] {
            self.list_hits.push(rows_rect(&self.list, vui, r, cols[0], row, lw - 40 - cols[0], 1), cursor);
        }
        let s = wide(&format!("|x{}   |t{}", vui.words.get(212, vui.language), vui.words.get(161, vui.language)));
        self.list.text_box(vui, r, &s, 20, 8, lw - 40, 1, UI_COLORS[0], 0);
    }

    /// The live client's `DrawSummary` (`live`, 0x01313C48): the high bid
    /// (word 1856, or "no bids"), the minimum next bid (the page's third
    /// column) while time is left, the time, and the player's bid as its
    /// count at its price and what is in escrow. Its "(you)" never shows (it
    /// tests the player's id against -1).
    fn draw_summary_live(&self, vui: &Vui, r: &mut dyn Raster, auction: &Auction, v: &View) {
        if v.searching || auction.p7860 != 0 {
            return;
        }
        let sel = self.menu.list.sel;
        let Some(l) = auction.results.get(sel.max(0) as usize).filter(|_| sel >= 0) else { return };
        let i = sel as usize;
        let col = |k: usize| auction.cols[k].get(i).copied().unwrap_or(0);
        let (high, floor) = (col(1), col(2));
        let left = l.p2fc - auction.get_minutes(v.now);
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let mut t = format!("{}\n{}:\n", l.item.name, w(1856));
        if high == -1 {
            t += &format!("{}\n", w(1708));
        } else {
            t += &format!("{high}\n");
        }
        if left > 0 {
            t += &format!("{}:\n{}\n", w(1710), floor);
        }
        t += &Self::format_time(vui, left, true);
        if let Some((price, escrow, count)) = Auction::find_bid3(v.bank, l.lot_id) {
            t += &format!("{}:\n{count}@{price}\n{}:\n{escrow}\n", w(1711), w(1712));
        }
        let ww = self.window.size[0] - 28;
        self.window.text_box(vui, r, &wide(&t), 16, 0, ww, 12, [1.0; 4], 3);
    }

    /// The live client's three-column `DrawRow` (0x01314638): a label, a
    /// framed middle and a right column, ticked when selected.
    fn draw_row3(w: &Window, vui: &Vui, r: &mut dyn Raster, row: i32, cells: [&[u16]; 3], x: [i32; 3], sel: bool) {
        let color = UI_COLORS[sel as usize];
        w.text_box(vui, r, cells[0], x[0], row, x[1] - x[0], 1, color, 0);
        w.text_box(vui, r, cells[1], x[1], row, x[2] - x[1], 1, color, 2);
        w.text_box(vui, r, cells[2], x[2], row, w.size[0] - x[2] - 25, 1, color, 2);
        if sel {
            w.tick_r(vui, r, x[0], row, true);
        }
    }

    /// The live client's `DrawRankings` (`live`, 0x01312930): the bidders
    /// in three columns, name, quantity (with the feature word's bit 0x08;
    /// an empty column without) and bid.
    fn draw_rankings_live(&mut self, vui: &Vui, r: &mut dyn Raster, auction: &Auction, v: &View) {
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
        let x = [40, 210, 300];
        let stacks = crate::live::features().has(crate::live::Features::STACKS);
        let qty_head = if stacks { wd(1853) } else { Vec::new() };
        Self::draw_row3(&self.list, vui, r, 0, [&wd(509), &qty_head, &wd(1697)], x, false);
        let n = auction.bidders.len() as i32;
        let t = if n == 7 { 0.0 } else { self.top as f32 / (n - 7) as f32 };
        let lw = self.list.size[0];
        self.list.draw_frame_vscroll(vui, r, 22, 1, lw - 55, 7, t);
        let mut row = 1;
        let mut i = self.top;
        while i < n && row < 8 {
            let b = &auction.bidders[i as usize];
            let qty = if stacks { wide(&b.x28.to_string()) } else { Vec::new() };
            Self::draw_row3(&self.list, vui, r, row, [&wide(&b.name), &qty, &wide(&b.x24.to_string())], x, i == self.cursor);
            self.list_hits.push(rows_rect(&self.list, vui, r, 22, row, lw - 55, 1), i);
            i += 1;
            row += 1;
        }
        let s = wide(&format!("|t{}", vui.words.get(161, vui.language)));
        self.list.text_box(vui, r, &s, 20, 9, lw - 40, 1, UI_COLORS[0], 0);
    }

    /// `DrawRow`: a label, a (framed) value, in the window's colours
    /// (`VIUICOLOR` 0, black; 1, the selection's dark blue); ticked when
    /// selected.
    fn draw_row(w: &Window, vui: &Vui, r: &mut dyn Raster, row: i32, label: Option<&[u16]>, value: Option<&[u16]>, xa: i32, xb: i32, sel: bool, tick: bool, framed: bool) {
        let color = UI_COLORS[sel as usize];
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
            // the pickers' values: "any" (471), the race's "none" (302), else
            // the name (413 for a wear slot or attack type with none)
            self.class = self.class.clamp(-1, 15);
            let word = if self.class == -1 { 471 } else { class_word(self.class) };
            Self::draw_row(w, vui, r, 2, Some(&wd(288)), Some(&wd(word)), cols[0], cols[1], f == 1, own, true);
            self.race = self.race.clamp(-2, 10);
            let word = match self.race {
                -1 => 471,
                -2 => 302,
                n => race_word(n),
            };
            Self::draw_row(w, vui, r, 3, Some(&wd(289)), Some(&wd(word)), cols[0], cols[1], f == 2, own, true);
            self.min_level = self.min_level.clamp(0, 60);
            Self::draw_row(w, vui, r, 4, Some(&wd(1681)), Some(&wide(&self.min_level.to_string())), cols[0], cols[1], f == 3, own, true);
            self.max_level = self.min_level.max(self.max_level).min(60);
            Self::draw_row(w, vui, r, 5, Some(&wd(1682)), Some(&wide(&self.max_level.to_string())), cols[0], cols[1], f == 4, own, true);
            let word = if self.wear == -2 { 471 } else { crate::infotext::wear_slot_word(self.wear as i64).unwrap_or(413) };
            Self::draw_row(w, vui, r, 6, Some(&wd(1683)), Some(&wd(word)), cols[0], cols[1], f == 5, own, true);
            let word = if self.attack == -1 { 471 } else { crate::infotext::attack_word(v.attack_words, self.attack as i64).unwrap_or(413) };
            Self::draw_row(w, vui, r, 7, Some(&wd(1684)), Some(&wd(word)), cols[0], cols[1], f == 6, own, true);
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
            if crate::live::features().live() {
                self.draw_summary_live(vui, r, auction, v);
            } else {
                self.draw_summary(vui, r, auction, v);
            }
        }
        let ww = self.window.size[0];
        let white = [1.0, 1.0, 1.0, 1.0];
        if self.mode == 0 {
            let s = if self.state == 1 { format!("|s{} |c{}", vui.words.get(1692, vui.language), vui.words.get(216, vui.language)) } else { format!("|-|s{} |c{}", vui.words.get(1692, vui.language), vui.words.get(216, vui.language)) };
            self.window.text_box(vui, r, &wide(&s), 16, 12, ww - 32, 1, white, 1);
        }
        let s = format!("|x{} |t{}", vui.words.get(212, vui.language), vui.words.get(161, vui.language));
        self.window.text_box(vui, r, &wide(&s), 16, 13, ww - 32, 1, white, 1);
        let live = crate::live::features().live();
        match self.state {
            2 => self.draw_lot(vui, r, auction, v),
            3 if live => self.draw_rankings_live(vui, r, auction, v),
            3 => self.draw_rankings(vui, r, auction, v),
            4 if live => self.draw_bid_live(vui, r, auction, v),
            4 => self.draw_bid(vui, r, auction, v),
            5..=8 => self.draw_picker(vui, r, v),
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
            self.status.text_box(vui, r, &wide(&s), 40, 1, sw - 40, 1, UI_COLORS[0], 0);
        } else {
            self.status.close(true);
        }
        match self.child {
            Child::Edit => self.edit.draw(vui, r, now, cues),
            Child::MBox => {
                self.mbox.draw(vui, r, now);
            }
            Child::Inspect => self.inspect.draw(vui, r, cues),
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
        let c = UI_COLORS[0];
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
        self.list.text_box(vui, r, &s, 20, 9, lw - 40, 1, UI_COLORS[0], 0);
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
        self.list.text_box(vui, r, &s, 20, 7, lw - 40, 1, UI_COLORS[0], 0);
    }

    /// `DrawRaces`, `DrawClasses`, `DrawTypes` (the wear slots) and
    /// `DrawSlots` (the attack types), states 5..8: the list's title, seven
    /// of its rows from `top` beside the scroll bar, the tick at the cursor
    /// (the selected row in the selection's colour), and the legend.
    fn draw_picker(&mut self, vui: &Vui, r: &mut dyn Raster, v: &View) {
        let Some(pk) = picker(self.state) else { return };
        self.list.init(vui, r, [320, 180], 290, 10, 0);
        let wd = |id: i32| wide(vui.words.get(id, vui.language));
        if self.list.is_open() || self.list.is_opening() || self.list.is_closing() {
            self.list.set_title(Some(&wd(pk.title)));
            self.list.draw(vui, r);
        }
        if self.cursor - self.top >= 7 {
            self.top = self.cursor - 6;
        } else if self.cursor < self.top {
            self.top = self.cursor;
        }
        let n = pk.rows.len() as i32;
        let lw = self.list.size[0];
        self.list.draw_frame_vscroll(vui, r, 22, 1, lw - 55, 7, self.top as f32 / (n - 7) as f32);
        let mut row = 1;
        let mut i = self.top;
        while i < n && row < 8 {
            let sel = i == self.cursor;
            if sel {
                self.list.tick_r(vui, r, 36, row, true);
            }
            let value = pk.rows[i as usize];
            // the first row of the wear slots and the attack types is "any" by
            // its place, the others' by their value
            let word = match self.state {
                5 => match value {
                    -1 => 471,
                    -2 => 302,
                    n => race_word(n),
                },
                6 => {
                    if value == -1 {
                        471
                    } else {
                        class_word(value)
                    }
                }
                7 if i == 0 => 471,
                7 => crate::infotext::wear_slot_word(value as i64).unwrap_or(413),
                _ if i == 0 => 471,
                _ => crate::infotext::attack_word(v.attack_words, value as i64).unwrap_or(413),
            };
            self.list.text_box(vui, r, &wd(word), 40, row, lw - 75, 1, UI_COLORS[sel as usize], 2);
            self.list_hits.push(rows_rect(&self.list, vui, r, 40, row, lw - 75, 1), i);
            i += 1;
            row += 1;
        }
        let s = wide(&format!("|x{}   |t{}", vui.words.get(212, vui.language), vui.words.get(161, vui.language)));
        self.list.text_box(vui, r, &s, 20, 9, lw - 40, 1, UI_COLORS[0], 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auction::Bidder;
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
            let v = View { bank: &self.bank, idle: true, searching: false, player: 1, now: 0, icon: &icon, can_use: &can_use, attack_words: &ATTACK_WORDS };
            self.w.fill_menu(&self.a, &v);
        }

        fn key(&mut self, k: i32) {
            let (icon, can_use) = (|_: &NetItem| 7, |_: &NetItem| true);
            let v = View { bank: &self.bank, idle: true, searching: false, player: 1, now: 0, icon: &icon, can_use: &can_use, attack_words: &ATTACK_WORDS };
            self.w.key(&self.vui, &mut NullRaster, &mut self.a, &v, k, true, &mut self.cues, &mut self.out);
            self.w.key(&self.vui, &mut NullRaster, &mut self.a, &v, k, false, &mut self.cues, &mut self.out);
        }

        fn draw(&mut self) {
            let (icon, can_use) = (|_: &NetItem| 7, |_: &NetItem| true);
            let v = View { bank: &self.bank, idle: true, searching: false, player: 1, now: 0, icon: &icon, can_use: &can_use, attack_words: &ATTACK_WORDS };
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

    /// The form opens on the Max. Level a server's patch hooks (Sandstorm's
    /// S2: 60), retail's 50 without one (the control).
    #[test]
    fn the_form_opens_on_the_patchs_level() {
        let patched = crate::s2::Rules { auction_max_level: 60, ..crate::s2::Rules::retail() };
        assert_eq!(crate::s2::with_rules(patched, || Rig::new().w.max_level), 60);
        assert_eq!(Rig::new().w.max_level, 50);
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

    /// A stack lot: 20 of them at 100 each, bought out at 300 each.
    fn stack_lot(id: i64) -> AuctionItem {
        AuctionItem { quantity: 20, ..lot(id, 100) }
    }

    /// The live client's bid form (`live`, the feature word with bit 0x08):
    /// the quantity first (its editor titled 1873), held from the player's
    /// own bid's count to the lot's as it is drawn; the bid held from the
    /// floor (the lot's minimum or the page's third column, the higher) to
    /// the buy-out; four rows, the cursor stopping at the buy-out (3). The
    /// buy-out price for less than the whole stack is word 1858 with the
    /// count and nothing asked; a proxy bid's box says the quantity and
    /// sends it; the buy-out sends the whole stack at the buy-out price.
    #[test]
    fn the_live_bid_form_bids_for_a_quantity() {
        crate::live::with_features(Some(0x1B), || {
            let mut g = Rig::new();
            g.vui = Vui::new(numbered_words(), Default::default(), Vec::new());
            g.page(vec![stack_lot(5)]);
            g.a.cols[2] = vec![140];
            g.bank.bids.push(Bid { auction_id: 5, x08: 120, x0c: 3, x10: 360, ..Default::default() });
            g.w.lot = 5;
            g.w.set_state(2, 0);
            g.key(134);
            assert_eq!((g.w.state, g.w.cursor, g.w.bid_qty, g.w.bid), (4, 0, 1, 0));
            g.draw();
            assert_eq!((g.w.bid_qty, g.w.bid), (3, 140), "the count from the player's bid's, the bid from the floor (the page's 140 over the lot's 100)");
            // the quantity's editor: 50 typed, held to the lot's 20
            g.key(134);
            assert!(g.w.typing());
            assert_eq!(String::from_utf16_lossy(&g.w.edit.title), "w1873");
            g.w.edit.set_text(&wide("50"));
            g.w.take_text(0);
            g.draw();
            assert_eq!(g.w.bid_qty, 20);
            // down to the buy-out and one more: 3, sound 9
            for _ in 0..3 {
                g.key(139);
            }
            g.cues.clear();
            g.key(139);
            assert_eq!((g.w.cursor, g.cues.clone()), (3, vec![9]));
            // the proxy bid at the buy-out for 5 of 20: word 1858 with 20, nothing asked
            g.w.cursor = 2;
            g.w.bid = 300;
            g.w.bid_qty = 5;
            g.out.clear();
            g.key(134);
            assert_eq!((g.out.clone(), g.w.child), (vec![SearchOut::RedCount(1858, 20)], Child::Own));
            // a proxy bid of 150 for 5: the box says the quantity, Yes sends it
            g.w.bid = 150;
            g.key(134);
            assert_eq!(g.w.child, Child::MBox);
            assert_eq!(String::from_utf16_lossy(&g.w.mbox.text), "w1723: Sword\nw1853: 5\nw1728: 150 w479\nw1791");
            g.key(136);
            g.out.clear();
            g.key(134);
            assert_eq!(g.out, vec![SearchOut::Send(Auction::bid_n(5, 150, 5), 4)]);
            assert_eq!(g.w.state, 1, "the page again");
            // the buy-out: the whole stack at 300 each
            g.w.set_state(2, 0);
            g.key(134);
            g.w.cursor = 3;
            g.key(134);
            assert!(String::from_utf16_lossy(&g.w.mbox.text).contains("w1697: 300 w479"));
            g.key(136);
            g.out.clear();
            g.key(134);
            assert_eq!(g.out, vec![SearchOut::Send(Auction::bid_n(5, 300, 20), 4)]);
        });
    }

    /// The live bid form words a lot of one's buy-out as the live client
    /// does (row 1701 "Buy now for %d Tunar" where the disc's table says
    /// "Acquire now ...", `livewords::live`); the control, the disc's form
    /// on a server with no word, keeps the disc's.
    #[test]
    fn the_live_buyout_says_buy_now() {
        let words = || {
            let mut lines: Vec<String> = (1..=1814).map(|n| format!("w{n}")).collect();
            lines[1700] = "Acquire now for %d Tunar".into();
            vi::strtab::StringTable::parse(lines.join("\n").as_bytes()).unwrap()
        };
        let shown = |word| {
            crate::live::with_features(word, || {
                let mut g = Rig::new();
                g.vui = Vui::new(words(), Default::default(), Vec::new());
                g.page(vec![lot(5, 100)]);
                g.w.lot = 5;
                g.w.set_state(4, 0);
                drawn(&mut g)
            })
        };
        assert!(shown(Some(0x1B)).contains(&"Buy now for 300 Tunar".to_string()));
        assert!(shown(None).contains(&"Acquire now for 300 Tunar".to_string()));
    }

    /// The live bid form without bit 0x08: the quantity row stays (the live
    /// client draws it always) but the boxes have no quantity line; with no
    /// word (the control, the disc's form) a box sends the disc's count of 1
    /// and the form has three rows.
    #[test]
    fn without_bit_0x08_the_bid_boxes_have_no_quantity() {
        crate::live::with_features(Some(0x10), || {
            let mut g = Rig::new();
            g.vui = Vui::new(numbered_words(), Default::default(), Vec::new());
            g.page(vec![lot(5, 100)]);
            g.w.lot = 5;
            g.w.set_state(4, 0);
            g.draw();
            assert_eq!(g.w.bid, 100);
            assert!(drawn(&mut g).iter().any(|s| s == "w1853"), "the quantity row");
            g.w.cursor = 2;
            g.key(134);
            assert_eq!(String::from_utf16_lossy(&g.w.mbox.text), "w1723: Sword\nw1728: 100 w479\nw1791");
            g.key(136);
            g.out.clear();
            g.key(134);
            assert_eq!(g.out, vec![SearchOut::Send(Auction::bid_n(5, 100, 1), 4)]);
        });
        let mut g = Rig::new();
        g.page(vec![lot(5, 100)]);
        g.w.lot = 5;
        g.w.set_state(4, 0);
        for _ in 0..4 {
            g.key(139);
        }
        assert_eq!(g.w.cursor, 2, "the disc's form: three rows");
    }

    /// The live summary and bidders (`live`): "High bid" or "no bids", the
    /// minimum next bid out of the page's third column, the player's bid as
    /// count@price; the bidders with their quantity column with bit 0x08,
    /// an empty one without. The control: the disc's summary says "Current
    /// Bid" (1707) and the high bid's "(you)" by the leader's column.
    #[test]
    fn the_live_summary_and_bidders() {
        let summary = |word| {
            crate::live::with_features(word, || {
                let mut g = Rig::new();
                g.vui = Vui::new(numbered_words(), Default::default(), Vec::new());
                g.page(vec![stack_lot(5)]);
                g.a.cols = [vec![1], vec![130], vec![131]];
                g.bank.bids.push(Bid { auction_id: 5, x08: 130, x0c: 4, x10: 520, ..Default::default() });
                g.w.set_state(1, 0);
                drawn(&mut g)
            })
        };
        let live = summary(Some(0x1B)).join("|");
        assert!(live.contains("Sword|w1856:|130|w1710:|131|"), "{live}");
        assert!(live.contains("w1711:|4@130|w1712:|520"), "{live}");
        let disc = summary(None).join("|");
        assert!(disc.contains("w1707:|130 w1709|w1710:|131|") && disc.contains("w1711:|130|w1712:|520"), "{disc}");
        let bidders = |word| {
            crate::live::with_features(word, || {
                let mut g = Rig::new();
                g.vui = Vui::new(numbered_words(), Default::default(), Vec::new());
                g.page(vec![stack_lot(5)]);
                g.a.bidders = vec![Bidder { name: "Ann".into(), x24: 150, x28: 7, ..Default::default() }];
                g.w.set_state(2, 0);
                g.w.set_state(3, 0);
                drawn(&mut g).join("|")
            })
        };
        let with = bidders(Some(0x1B));
        assert!(with.contains("w509|w1853|w1697|") && with.contains("Ann|7|150"), "{with}");
        let without = bidders(Some(0x10));
        assert!(without.contains("w509|w1697|") && without.contains("Ann|150") && !without.contains("w1853"), "{without}");
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

    /// A stand-in for `gAttackWords` (CLIENT's data, read off the image in
    /// play): attack type t is word 900 + t.
    const ATTACK_WORDS: [(i32, i64); 11] = [(900, 0), (901, 1), (902, 2), (903, 3), (904, 4), (905, 5), (906, 6), (907, 7), (908, 8), (909, 9), (910, 10)];

    /// Opens the picker of form field `field` and takes row `row` with Enter.
    fn choose(g: &mut Rig, field: i32, row: usize) {
        g.w.field = field;
        g.key(134);
        for _ in 0..row {
            g.key(139);
        }
        g.key(134);
    }

    /// Enter on the form's class, race, wear slot and attack type rows opens
    /// their pickers (states 6, 5, 7, 8) at the first row; the d-pad moves
    /// the row and stops at the ends; Enter takes the row's value into the
    /// field, Escape leaves the field as it was.
    #[test]
    fn each_picker_takes_its_row_into_the_form() {
        for (field, state, rows, row) in [(1, 6, &CLASSES[..], 3), (2, 5, &RACES[..], 4), (5, 7, &WEARS[..], 10), (6, 8, &ATTACKS[..], 2)] {
            let mut g = Rig::new();
            let before = (g.w.class, g.w.race, g.w.wear, g.w.attack);
            g.w.field = field;
            g.cues.clear();
            g.key(134);
            assert_eq!((g.w.state, g.w.cursor, g.w.top, g.cues.clone()), (state, 0, 0, vec![13]), "field {field}");
            // up at the first row: sound 9, still the first
            g.cues.clear();
            g.key(138);
            assert_eq!((g.w.cursor, g.cues.clone()), (0, vec![9]));
            // Escape: the form again, the field untouched (the negative control)
            g.key(139);
            g.cues.clear();
            g.key(132);
            assert_eq!((g.w.state, g.cues.clone()), (0, vec![8]));
            assert_eq!((g.w.class, g.w.race, g.w.wear, g.w.attack), before);
            // down to the last row, and one more: sound 9
            g.key(134);
            for _ in 0..rows.len() - 1 {
                g.key(139);
            }
            g.cues.clear();
            g.key(139);
            assert_eq!((g.w.cursor, g.cues.clone()), (rows.len() as i32 - 1, vec![9]));
            g.key(132);
            // the row taken
            choose(&mut g, field, row);
            assert_eq!(g.w.state, 0);
            let got = match field {
                1 => g.w.class,
                2 => g.w.race,
                5 => g.w.wear,
                _ => g.w.attack,
            };
            assert_eq!(got, rows[row], "field {field}");
        }
    }

    /// On a server that sends the live client's feature word without bit
    /// 0x02 (`live`) the race picker has no Ogre (9) and the class picker no
    /// Alchemist (14), and Enter takes the row's own value; the controls, no
    /// word (the disc's lists, as on Sandstorm) and the retail word 0x1B,
    /// have them. The wear slots and attack types never change.
    #[test]
    fn the_pickers_leave_ogre_and_alchemist_to_the_feature_word() {
        let rows = |word| crate::live::with_features(word, || [5, 6, 7, 8].map(|s| picker(s).unwrap().rows));
        let [races, classes, wears, attacks] = rows(Some(0x10));
        assert_eq!(races, vec![-1, -2, 6, 2, 4, 1, 8, 3, 7, 0, 5]);
        assert_eq!(classes, vec![-1, 5, 9, 7, 12, 10, 4, 11, 2, 1, 6, 3, 8, 0, 13]);
        assert_eq!((wears, attacks), (WEARS.to_vec(), ATTACKS.to_vec()));
        for word in [None, Some(0x1B)] {
            let [races, classes, ..] = rows(word);
            assert_eq!((races, classes), (RACES.to_vec(), CLASSES.to_vec()), "{word:?}");
        }
        // the last race row is the Troll (5) without the Ogre, as Enter takes it
        crate::live::with_features(Some(0x10), || {
            let mut g = Rig::new();
            choose(&mut g, 2, 10);
            assert_eq!(g.w.race, 5);
        });
    }

    /// The search carries what the pickers chose: the class's and race's
    /// bits, the wear slot and attack type with their kind bits, "none" for
    /// a race (no bits) and for a wear slot (-1, searched for).
    #[test]
    fn the_search_carries_the_pickers_choices() {
        let mut g = Rig::new();
        choose(&mut g, 1, 3); // CLASSES[3] = 9, cleric
        choose(&mut g, 2, 4); // RACES[4] = 4, dwarf
        choose(&mut g, 5, 10); // WEARS[10] = 9, waist
        choose(&mut g, 6, 2); // ATTACKS[2] = 1
        g.w.field = 11;
        g.key(134);
        let q = &g.a.query;
        assert_eq!((q.p9c, q.pa0, q.pb4, q.pb8, q.kind), (1 << 9, 1 << 4, 9, 1, 2 | 1));
        let Some(SearchOut::Send(req, 5)) = g.out.last().cloned() else { panic!("{:?}", g.out) };
        let mut w = net::stream::Writer::default();
        q.write(&mut w);
        assert!(req.ends_with(&w.data), "the request ends with the query");
        // the first rows: any class, no race at all, any wear slot (kind 2
        // off), no attack type chosen (kind 1 off)
        let mut g = Rig::new();
        choose(&mut g, 1, 0);
        choose(&mut g, 2, 1);
        choose(&mut g, 5, 0);
        choose(&mut g, 6, 0);
        g.w.field = 11;
        g.key(134);
        let q = &g.a.query;
        assert_eq!((q.p9c, q.pa0, q.pb4, q.pb8, q.kind), (0xFFFF_FFFF, 0, -2, -1, 0));
        // a wear slot of "none" is searched for
        choose(&mut g, 5, 1);
        g.w.field = 11;
        g.key(134);
        assert_eq!((g.a.query.pb4, g.a.query.kind & 2), (-1, 2));
    }

    /// Words as `wN` for WORDS.TXT line N, so a drawing says which it drew.
    fn numbered_words() -> vi::strtab::StringTable {
        let lines: Vec<String> = (1..=2000).map(|n| format!("w{n}")).collect();
        vi::strtab::StringTable::parse(lines.join("\n").as_bytes()).unwrap()
    }

    /// The strings a drawing put down (each shadow too), with their colours.
    #[derive(Default)]
    struct TextRaster(Vec<(String, [f32; 4])>);

    impl Raster for TextRaster {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], c: [f32; 4], s: &[u16]) {
            self.0.push((String::from_utf16_lossy(s), c));
        }
        fn draw_centered_string(&mut self, _: [i32; 2], c: [f32; 4], s: &[u16], _: u32) {
            self.0.push((String::from_utf16_lossy(s), c));
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

    /// What the window drew: each string and its colour, in order.
    fn drawn_colored(g: &mut Rig) -> Vec<(String, [f32; 4])> {
        let (icon, can_use) = (|_: &NetItem| 7, |_: &NetItem| true);
        let v = View { bank: &g.bank, idle: true, searching: false, player: 1, now: 0, icon: &icon, can_use: &can_use, attack_words: &ATTACK_WORDS };
        let mut r = TextRaster::default();
        g.w.draw(&g.vui, &mut r, &g.a, &v, &mut g.cues);
        r.0
    }

    /// What the window drew, as the words in order, shadows dropped.
    fn drawn(g: &mut Rig) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (s, _) in drawn_colored(g) {
            if out.last() != Some(&s) {
                out.push(s);
            }
        }
        out
    }

    /// Inspect on the page: the selected lot described in the inspect window
    /// (sound 13), which takes the keys: the arrows scroll it (sound 11), △
    /// closes it (sound 8) and the page has them again. A click on the
    /// popup's Inspect does the same; while the window is up a click takes
    /// nothing (its legend is a glyph) and the page's rows under it wait.
    #[test]
    fn inspect_describes_the_selected_lot() {
        let mut g = Rig::new();
        g.vui = Vui::new(numbered_words(), Default::default(), Vec::new());
        let mut long = lot(6, 100);
        long.item.name = "Bow".into();
        long.item.desc = (0..200).map(|i| format!("word{i} ")).collect();
        g.page(vec![lot(5, 100), long]);
        g.key(136);
        assert_eq!((g.w.state, g.w.child), (1, Child::Menu));
        g.w.menu.list.sel = 1;
        // Enter: the popup, Inspect its first row; Enter: Inspect
        g.key(134);
        g.cues.clear();
        g.key(134);
        assert_eq!((g.w.child, g.w.lot, g.cues.clone()), (Child::Inspect, 6, vec![13]));
        // the name twice: the summary's and the description's first row
        let names = |d: &[String]| d.iter().filter(|s| *s == "Bow").count();
        let d = drawn(&mut g);
        assert_eq!(names(&d), 2, "the lot's name: {d:?}");
        assert!(d.iter().any(|s| s.starts_with("word0 ")), "its text: {d:?}");
        // the arrows scroll it (the window steps at the end of its draw, so
        // the next one shows it): the name's row goes off the top
        g.cues.clear();
        g.key(139);
        g.draw();
        assert_eq!(g.cues, vec![11]);
        let d = drawn(&mut g);
        assert_eq!(names(&d), 1, "scrolled: {d:?}");
        // a click on the page under it takes nothing, its row stays
        let row = |i: i32| [16.0 + 57.0, 37.0 + 44.0 * i as f32 + 22.0];
        assert!(g.w.point(row(0)).is_empty());
        assert_eq!(g.w.menu.list.sel, 1);
        // triangle: closed, the page has the keys
        g.cues.clear();
        g.key(132);
        assert_eq!((g.w.state, g.w.child, g.cues.clone()), (1, Child::Menu, vec![8]));
        assert!(!drawn(&mut g).iter().any(|s| s.starts_with("word0 ")), "the window is gone");
        // by pointer: the popup's Inspect row clicked (the popup grows in
        // over its first frames, its rows noted once it has: drawn a second on)
        g.w.menu.list.sel = 0;
        g.key(134);
        for now in [0, 1000] {
            let (icon, can_use) = (|_: &NetItem| 7, |_: &NetItem| true);
            let v = View { bank: &g.bank, idle: true, searching: false, player: 1, now, icon: &icon, can_use: &can_use, attack_words: &ATTACK_WORDS };
            g.w.draw(&g.vui, &mut NullRaster, &g.a, &v, &mut g.cues);
        }
        let at = {
            let s = g.w.menu.sub.as_ref().expect("the popup");
            let mut found = None;
            'scan: for y in (0..448).step_by(2) {
                for x in (0..640).step_by(2) {
                    let p = [x as f32, y as f32];
                    if s.list.hits.at(p) == Some(0) {
                        found = Some(p);
                        break 'scan;
                    }
                }
            }
            found.expect("the popup's Inspect row")
        };
        let keys = g.w.point(at);
        assert_eq!(keys, vec![key::CROSS]);
        for k in keys {
            g.key(k);
        }
        assert_eq!((g.w.child, g.w.lot), (Child::Inspect, 5));
        assert_eq!(drawn(&mut g).iter().filter(|s| *s == "Sword").count(), 2, "its summary and its description");
    }

    /// Each picker's list: its title, seven rows' names from the top (the
    /// first wear slot and attack type "any" by their place), scrolled to
    /// keep the cursor shown; the form's rows show the choice.
    #[test]
    fn the_pickers_name_their_rows() {
        let mut g = Rig::new();
        g.vui = Vui::new(numbered_words(), Default::default(), Vec::new());
        let rows_of = |d: &[String], title: &str| -> Vec<String> {
            let at = d.iter().position(|s| s == title).unwrap_or_else(|| panic!("no {title} in {d:?}"));
            d[at + 1..at + 8].to_vec()
        };
        // the classes: any, then alchemist (14) .. as sClasses lists them
        g.w.field = 1;
        g.key(134);
        let d = drawn(&mut g);
        assert_eq!(rows_of(&d, "w1702"), ["w471", "w179", "w170", "w174", "w172", "w177", "w175"]);
        // the last class: the list from the tenth row
        for _ in 0..15 {
            g.key(139);
        }
        let d = drawn(&mut g);
        assert_eq!(g.w.top, 9);
        assert_eq!(rows_of(&d, "w1702"), ["w167", "w166", "w171", "w168", "w173", "w165", "w178"]);
        g.key(132);
        // the races: any, none, then barbarian (6), dark elf (2) ..
        g.w.field = 2;
        g.key(134);
        let d = drawn(&mut g);
        assert_eq!(rows_of(&d, "w1703"), ["w471", "w302", "w191", "w186", "w188", "w185", "w193"]);
        g.key(132);
        // the wear slots: any by place, then -1's word (302, none), head ..
        g.w.field = 5;
        g.key(134);
        let d = drawn(&mut g);
        assert_eq!(rows_of(&d, "w1704"), ["w471", "w302", "w327", "w403", "w329", "w330", "w331"]);
        g.key(132);
        // the attack types: any by place, then gAttackWords' words
        g.w.field = 6;
        g.key(134);
        let d = drawn(&mut g);
        assert_eq!(rows_of(&d, "w1705"), ["w471", "w900", "w901", "w902", "w903", "w904", "w905"]);
        g.key(132);
        // the form shows a choice beside its label (negative control: "any")
        let label_then = |d: &[String], label: &str| d[d.iter().position(|s| s == label).unwrap() + 1].clone();
        let d = drawn(&mut g);
        assert_eq!([label_then(&d, "w288"), label_then(&d, "w289"), label_then(&d, "w1683"), label_then(&d, "w1684")], ["w471", "w471", "w471", "w471"]);
        choose(&mut g, 1, 3);
        choose(&mut g, 2, 1);
        choose(&mut g, 5, 2);
        choose(&mut g, 6, 9);
        let d = drawn(&mut g);
        assert_eq!([label_then(&d, "w288"), label_then(&d, "w289"), label_then(&d, "w1683"), label_then(&d, "w1684")], ["w174", "w302", "w327", "w909"]);
    }

    /// The form's rows, a picker's and the bid form's are the window's
    /// colours (`VIUICOLOR`): black, the selected row the selection's dark
    /// blue, as `DrawRow` and the pickers' `TextBox` draw them.
    #[test]
    fn the_rows_are_the_windows_colours() {
        let mut g = Rig::new();
        g.vui = Vui::new(numbered_words(), Default::default(), Vec::new());
        let colour_of = |d: &[(String, [f32; 4])], s: &str| d.iter().filter(|(t, _)| t == s).map(|(_, c)| *c).collect::<Vec<_>>();
        g.w.field = 3;
        let d = drawn_colored(&mut g);
        // the minimum level's label selected, the keywords' not; each drawn
        // once (no shadow)
        assert_eq!(colour_of(&d, "w1681"), vec![UI_COLORS[1]]);
        assert_eq!(colour_of(&d, "w1680"), vec![UI_COLORS[0]]);
        assert_eq!(colour_of(&d, "w1673"), vec![UI_COLORS[0]], "Search");
        // a picker's rows: the cursor's in the selection's colour
        g.w.field = 1;
        g.key(134);
        g.key(139);
        let d = drawn_colored(&mut g);
        assert_eq!(colour_of(&d, "w179"), vec![UI_COLORS[1]], "alchemist, under the cursor");
        assert_eq!(colour_of(&d, "w170"), vec![UI_COLORS[0]], "bard");
    }

    /// A pointer on a picker: nothing before a drawing noted its rows; then
    /// a click selects a row, a second takes it (Enter), and a scrolled list
    /// answers by the rows it shows.
    #[test]
    fn a_pointer_on_the_pickers() {
        use crate::pointer::testing::{mid, vui};
        let mut g = Rig::new();
        let gv = vui();
        let mut list = Window::default();
        list.init(&gv, &NullRaster, [320, 180], 290, 10, 0);
        list.set_title(Some(&wide("x")));
        let at = |row: i32| mid(rows_rect(&list, &gv, &NullRaster, 40, row, 290 - 75, 1));
        g.w.field = 1;
        g.key(134);
        assert!(g.w.point(at(3)).is_empty(), "no rows noted before a draw");
        g.draw();
        assert_eq!(g.w.point(at(3)), vec![MOVED]);
        assert_eq!(g.w.cursor, 2, "the third row");
        assert!(g.w.point(at(9)).is_empty(), "the legend's row is no row");
        assert_eq!(g.w.point(at(3)), vec![key::CROSS]);
        g.key(key::CROSS);
        assert_eq!((g.w.state, g.w.class), (0, CLASSES[2]));
        // the races scrolled to their end: the first row shown is the sixth
        g.w.field = 2;
        g.key(134);
        for _ in 0..11 {
            g.key(139);
        }
        g.draw();
        assert_eq!(g.w.top, 5);
        assert_eq!(g.w.point(at(1)), vec![MOVED]);
        assert_eq!(g.w.cursor, 5);
        assert_eq!(g.w.point(at(1)), vec![key::CROSS]);
        g.key(key::CROSS);
        assert_eq!(g.w.race, RACES[5]);
    }
}
