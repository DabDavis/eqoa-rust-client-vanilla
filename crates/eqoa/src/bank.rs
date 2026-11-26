//! The bank window (CLIENT `viwndbank.cpp`, `viwndbankmenu.cpp`,
//! `viwndbankmoney.cpp`, `viwndbankitem.cpp`, `VIWndMoneyMenu.cpp`; the game
//! window's mode 11): `VIWndBank` and its pages. The menu (Money, Items, the
//! auction when the bank offers one, Done); the money page, a three-way menu
//! (deposit, withdraw, back) over `VIWndDeposit` in its bank modes 1 and 2;
//! the item page, the inventory and the bank side by side (40 rows each, or as
//! many as a server that holds more says: `inventory::pack_cap`, `bank_cap`),
//! the selected item's text between them, `VIWndQuantity` for a stack.
//!
//! As in the other windows, the window tree's messages come back up as
//! values: `BankOut` is what `VIBankControl` and `VIWndGame` are asked.
//!
//! A pointer (`pointer`) goes to the page with the keys: the menus' rows at
//! once; on the item page a row of the list in use selected or its
//! selection moved, a row of the other list selected and that list taken
//! up (left or right, as the d-pad does); the pickers and the auction's
//! form as theirs.
//!
//! With Inventory Style at Bags (Heirloom petition 3265: "view bank items
//! the same way as the new inventory", "or filter the bank by equipment,
//! gems and so on") the item page shows the two sides as the item menu's
//! grid does (`bags::draw_grid`: the items' icons, a stack's count, worn
//! items framed, what the class cannot wear washed red), laid out by Bag
//! Sort, two cells across, the item's text between them as the lists have
//! it; □ (or the Filter button) shows All, Equipment, Gems, Crafting or
//! Consumables on both sides (`bags::Filter`). The d-pad moves over a grid
//! (left from the bank's first column and right from the inventory's last
//! go to the other side), ✕ moves the item as the list's ✕ does: the same
//! 0x1254, by the item's place in its list. Vanilla's own; Retail List keeps
//! the game's lists.
//!
//! The bank's pages (Heirloom petition 3297: "a way to change the bank pages
//! while viewing the bank contents"): with go-eqoa-server's account bank
//! (`world.account_bank`, 2000 records) the bank list a client holds is a
//! PAGE of the account's bank, turned by the server's own "/bank next" and
//! "/bank prev" (its accountbank.go: every record of the page taken off with
//! 0x1250, the next page's added with 0x1251, a line in the chat saying
//! which page it is). On the item page L1 and R1 (Page Up and Page Down on
//! the keyboard) send those lines as the chat line would, and at Bags the
//! Page buttons under the bank's grid press them; the bank's side goes back
//! to its top. Nothing new goes to a server: a server without pages answers
//! the line itself.

#![allow(clippy::too_many_arguments)]

use crate::infotext::Info;
use crate::pointer::{Hits, key, rows_rect};
use crate::quantity::{Quantity, QuantityUp};
use crate::trade::{Deposit, DepositUp};
use crate::wndlist::WndList;
use net::dump::{ItemList, NetItem};
use vi::ui::{FrameParam, Raster, Vui, Window, wide};

/// What the bank asks of the client.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BankOut {
    /// `ClosePanel6AC`: `VIWndGame::CloseBank`
    Close,
    /// 288 to the game window: the chat box fixed (1) or live (0)
    ChatMode(i32),
    /// `VIBankControl::Move(fromBank, slot, n)`: 0x1254
    Move(u8, i32, i32),
    /// `VIBankControl::MoveMoney(fromBank, n)`: 0x1255
    MoveMoney(u8, i32),
    /// 310: the auction page
    Auction,
    /// `VIBankControl::Sell(slot, count, price, buyout, duration)`: 0x1205 sub-op 5
    Sell(i32, i32, i32, i32, i32),
    /// `AddText(word 1727)` in red: that cannot be auctioned
    CannotSell,
    /// The bank's page turned (petition 3297): -1 "/bank prev", 1 "/bank
    /// next", the chat line go-eqoa-server's account bank pages by
    Page(i32),
}

/// The chat line that turns the bank's page `d` (-1 back, 1 on).
pub fn page_line(d: i32) -> &'static str {
    if d < 0 { "/bank prev" } else { "/bank next" }
}

/// The page that has the keys.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Page {
    #[default]
    Menu,
    /// the money page: its menu, or the deposit form over it
    MoneyMenu,
    Deposit,
    /// the item page, or its quantity picker
    Items,
    Quantity,
    /// `VIWndAuction` (the auction's own window)
    Auction,
}

/// `VIWndBankMenu`.
#[derive(Default)]
struct BankMenu {
    cursor: i32,
    /// the bank offers the auction (`GetP78CC`)
    auction: bool,
    window: Window,
    /// the rows where they were drawn, by cursor, for a pointer
    hits: Hits,
}

/// `VIWndMoneyMenu`.
#[derive(Default)]
struct MoneyMenu {
    sel: i32,
    window: Window,
    /// the rows where they were drawn, for a pointer
    hits: Hits,
}

/// `VIWndBankItem`.
struct BankItem {
    inv: WndList,
    bank: WndList,
    qty: Quantity,
    /// `m_from`: the list a quantity was asked for (0 inventory, 1 bank)
    from: i32,
    text: String,
    window: Window,
    /// the two lists' frames (0 the inventory, 1 the bank), for a pointer
    hits: Hits,
    /// at Bags (petition 3265): each side's grid moves (0 the inventory, 1
    /// the bank), the side in use, the filter, the items each grid showed
    /// last frame (their places in their lists, in the cells' order), and
    /// where the cells and the Filter button were drawn
    grids: [crate::bags::Bag; 2],
    side: usize,
    filter: crate::bags::Filter,
    shown: [Vec<usize>; 2],
    grid_hits: Hits,
}

impl Default for BankItem {
    fn default() -> Self {
        // Init: the two lists (`SetX84(1)`), 40 rows each
        let mut inv = WndList::with_rows([16, 37], 0, 40);
        let mut bank = WndList::with_rows([475, 37], 0, 40);
        inv.x84 = true;
        bank.x84 = true;
        BankItem {
            inv,
            bank,
            qty: Quantity::default(),
            from: -1,
            text: String::new(),
            window: Window::default(),
            hits: Hits::default(),
            grids: Default::default(),
            side: 0,
            filter: crate::bags::Filter::All,
            shown: Default::default(),
            grid_hits: Hits::default(),
        }
    }
}

/// The item page's grids at Bags (petition 3265): the inventory's at the
/// left and the bank's at the right, where the lists are, two cells across
/// and seven rows shown, clear of the text between them.
pub const GRIDS: [crate::bags::Grid; 2] = [crate::bags::Grid { origin: [24, 60], cols: 2, rows: 7 }, crate::bags::Grid { origin: [460, 60], cols: 2, rows: 7 }];
/// The Filter button under the text (frame points).
pub const FILTER_BUTTON: [i32; 4] = [212, 278, 428, 300];
/// The Page buttons under the bank's grid (petition 3297): back (L1) and on
/// (R1), frame points.
pub const PAGE_BUTTONS: [[i32; 4]; 2] = [[460, 299, 534, 320], [542, 299, 616, 320]];
/// The hits of the Filter button and the two Page buttons.
const FILTER_HIT: i32 = -2;
const PAGE_HITS: [i32; 2] = [-3, -4];
/// A bank cell's hit is this and its number (an inventory cell's its number).
const BANK_CELLS: i32 = 10_000;

/// The grids' cells for `shown` items of a list of `items` in a room of
/// `room`: unfiltered the room, as the item menu's; filtered only what is
/// kept, a row at least.
fn grid_cells(g: &crate::bags::Grid, filter: crate::bags::Filter, items: usize, shown: usize, room: usize) -> i32 {
    if filter == crate::bags::Filter::All { g.cells(items, room) } else { g.cells(shown, 0).max(g.cols) }
}

/// `VIWndBank`.
#[derive(Default)]
pub struct Bank {
    pub page: Page,
    menu: BankMenu,
    money: MoneyMenu,
    deposit: Deposit,
    item: BankItem,
    sell: crate::auction::SellWnd,
    pub visible: bool,
}

/// What the bank pages read: the player's coin, the bank's tunar, the two
/// lists, whether a request is waiting.
pub struct View<'a> {
    pub carried: i32,
    pub banked: i32,
    pub inv: &'a ItemList,
    pub bank: &'a ItemList,
    /// `VIBankControl::IsIdle`
    pub idle: bool,
}

/// A row of either list (`OnDrawSelf`): a stack shows its count, a worn
/// item (the inventory) the arrows, one the player cannot use the red ones;
/// `rows` the list's (CLIENT's 40, or what the server holds), the rest empty.
fn fill(list: &mut WndList, l: &ItemList, rows: usize, worn: bool, icon: &dyn Fn(&NetItem) -> i32, can_use: &dyn Fn(&NetItem) -> bool) {
    if list.items.len() != rows {
        list.init(rows);
    }
    let n = l.items.len().min(rows);
    for (i, it) in l.items.iter().take(n).enumerate() {
        let mut flags = if it.quantity >= 2 { 5 } else { 1 };
        let mut color = -1;
        if worn && it.equip_slot != -1 {
            color = 138;
            flags |= 8;
        }
        if it.item.wear_slot != -1 && !can_use(it) {
            color = 140;
            flags |= 8;
        }
        list.set_item(i, flags, icon(it), color, -1, it.quantity as i32);
    }
    for i in n..rows {
        list.set_item(i, 0, -1, -1, -1, 0);
    }
}

impl Bank {
    /// `VIWndGame::OpenBank`'s `ShowMenu(1)`: the menu page, its cursor at
    /// the top, the focus (295: the window at (320, 224) closed and opening,
    /// the auction row when the bank has one).
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, auction: bool) {
        self.visible = true;
        self.show_menu(vui, r, auction);
    }

    /// The menu has the auction's row (`m_auction`).
    #[cfg(test)]
    pub fn offers_auction(&self) -> bool {
        self.menu.auction
    }

    /// `ShowMenu`.
    fn show_menu(&mut self, vui: &Vui, r: &dyn Raster, auction: bool) {
        self.page = Page::Menu;
        self.item.qty.visible = false;
        self.deposit.visible = false;
        // ResetCursor, then SetActiveWindow's 295
        self.menu.cursor = 0;
        self.menu.window.init(vui, r, [320, 224], 250, 8, 1);
        self.menu.window.close(true);
        self.menu.window.open(false);
        self.menu.auction = auction;
    }

    /// `VIWndBankMoney::Reset`: the deposit form put away, the money menu
    /// again with the focus.
    fn money_reset(&mut self, vui: &Vui, r: &dyn Raster) {
        self.deposit.cursor = 8;
        self.deposit.digits = [0; 9];
        self.deposit.visible = false;
        self.deposit.carried = 0;
        self.deposit.banked = 0;
        self.deposit.setup(0);
        self.money.sel = 0;
        self.money.window.init(vui, r, [320, 224], 250, 7, 1);
        self.money.window.close(true);
        self.money.window.open(false);
        self.page = Page::MoneyMenu;
    }

    /// `VIWndBankItem::Show`: the inventory active, both from the top, the
    /// quantity put away, the focus (295: the text window opened).
    fn items_show(&mut self, vui: &Vui, r: &dyn Raster) {
        let it = &mut self.item;
        it.inv.set_x64(true);
        it.inv.reset_scroll();
        it.bank.set_x64(false);
        it.bank.reset_scroll();
        it.qty.visible = false;
        it.grids = Default::default();
        it.side = 0;
        it.window.init(vui, r, [323, 188], 290, 15, 1);
        it.window.open(true);
        self.page = Page::Items;
    }

    /// The grids' orders now (petition 3265): each list by Bag Sort, then
    /// what the filter keeps.
    fn grid_orders(&self, v: &View) -> [Vec<usize>; 2] {
        let (sort, f) = (crate::settings::bag_sort(), self.item.filter);
        [crate::bags::order_filtered(v.inv, sort, f), crate::bags::order_filtered(v.bank, sort, f)]
    }

    /// The grids' cells now, by their orders.
    fn grid_counts(&self, v: &View, orders: &[Vec<usize>; 2]) -> [i32; 2] {
        let f = self.item.filter;
        [
            grid_cells(&GRIDS[0], f, v.inv.items.len(), orders[0].len(), crate::inventory::pack_cap()),
            grid_cells(&GRIDS[1], f, v.bank.items.len(), orders[1].len(), crate::inventory::bank_cap()),
        ]
    }

    /// A key to the item page at Bags: the d-pad over the grid in use (left
    /// from the bank's first column, right from the inventory's last, to the
    /// other side), □ the next filter (both grids from their tops), ✕ the
    /// cell's item moved as the list's ✕ moves it (`items_key`), △ back,
    /// L1 and R1 the bank's page (`turn_page`).
    fn grid_key(&mut self, vui: &Vui, r: &dyn Raster, v: &View, key: i32, down: bool, cues: &mut Vec<i32>, out: &mut Vec<BankOut>) {
        if !down {
            return;
        }
        let orders = self.grid_orders(v);
        let cells = self.grid_counts(v, &orders);
        let it = &mut self.item;
        let s = it.side;
        let g = &GRIDS[s];
        let col = it.grids[s].sel % g.cols;
        match key {
            27 | 132 => {
                cues.push(8);
                self.back(vui, r, out);
            }
            133 => {
                cues.push(11);
                it.filter = it.filter.step(1);
                it.grids = Default::default();
            }
            140 | 142 => self.turn_page(v, if key == 140 { -1 } else { 1 }, cues, out),
            17 | 136 if s == 1 && col == 0 => {
                cues.push(11);
                it.side = 0;
            }
            18 | 137 if s == 0 && (col == g.cols - 1 || it.grids[0].sel + 1 >= cells[0]) => {
                cues.push(11);
                it.side = 1;
            }
            17 | 136 => cues.push(it.grids[s].step_in(g, -1, 0, cells[s])),
            18 | 137 => cues.push(it.grids[s].step_in(g, 1, 0, cells[s])),
            19 | 138 => cues.push(it.grids[s].step_in(g, 0, -1, cells[s])),
            20 | 139 => cues.push(it.grids[s].step_in(g, 0, 1, cells[s])),
            13 | 134 => {
                let Some(&k) = orders[s].get(it.grids[s].sel.max(0) as usize) else {
                    cues.push(10);
                    return;
                };
                if !v.idle {
                    cues.push(9);
                    return;
                }
                cues.push(13);
                it.from = -1;
                let list = if s == 1 { v.bank } else { v.inv };
                let item = &list.items[k];
                if item.item.max_stack != 0 && item.quantity >= 2 {
                    it.from = s as i32;
                    it.qty.open(vui, r, item.quantity as i32, k as i32, -1);
                    self.page = Page::Quantity;
                } else {
                    out.push(BankOut::Move(s as u8, k as i32, 1));
                }
            }
            _ => {}
        }
    }

    /// L1 or R1 on the item page (petition 3297): the bank's page turned
    /// (`d` -1 back, 1 on), the bank's side from its top again; refused
    /// (sound 9) while a request waits, as a move is.
    fn turn_page(&mut self, v: &View, d: i32, cues: &mut Vec<i32>, out: &mut Vec<BankOut>) {
        if !v.idle {
            cues.push(9);
            return;
        }
        cues.push(11);
        let it = &mut self.item;
        it.grids[1] = Default::default();
        it.bank.reset_scroll();
        out.push(BankOut::Page(d));
    }

    /// 260 from a page to `VIWndBank`: the chat box live, the menu again.
    fn back(&mut self, vui: &Vui, r: &dyn Raster, out: &mut Vec<BankOut>) {
        out.push(BankOut::ChatMode(0));
        let auction = self.menu.auction;
        self.show_menu(vui, r, auction);
    }

    /// `VIWndBankMenu::MoveCursor`.
    fn menu_move(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        let last = if self.menu.auction { 3 } else { 2 };
        self.menu.cursor += d;
        if self.menu.cursor < 0 {
            cues.push(9);
            self.menu.cursor = 0;
        } else if self.menu.cursor > last {
            cues.push(9);
            self.menu.cursor = last;
        } else {
            cues.push(11);
        }
        self.menu.window.reset_tick_pulse(now);
    }

    /// `VIWndMoneyMenu::Move`.
    fn money_move(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        if self.money.sel + d < 0 {
            cues.push(9);
            self.money.sel = 0;
        } else if self.money.sel + d >= 3 {
            cues.push(9);
            self.money.sel = 2;
        } else {
            cues.push(11);
            self.money.sel += d;
        }
        self.money.window.reset_tick_pulse(now);
    }

    /// The deposit form for `mode` (291 deposit, 292 withdraw).
    fn deposit_open(&mut self, vui: &Vui, r: &dyn Raster, v: &View, mode: i32) {
        self.money.sel = 0;
        let d = &mut self.deposit;
        d.carried = v.carried;
        d.banked = v.banked;
        d.digits = [0; 9];
        d.cursor = 8;
        d.setup(mode);
        d.focus(vui, r);
        self.page = Page::Deposit;
    }

    /// The keys, to the page that has them (`OnKeyDown` / `OnKeyUp`).
    pub fn key(&mut self, vui: &Vui, r: &dyn Raster, v: &View, key: i32, down: bool, now: i64, cues: &mut Vec<i32>, out: &mut Vec<BankOut>) {
        match self.page {
            Page::Menu => {
                if !down {
                    return;
                }
                match key {
                    27 | 132 => {
                        cues.push(8);
                        out.push(BankOut::Close);
                    }
                    13 | 134 => {
                        cues.push(13);
                        match self.menu.cursor {
                            // 290: the money page
                            0 => self.money_reset(vui, r),
                            // 289: the item page
                            1 => {
                                out.push(BankOut::ChatMode(1));
                                self.items_show(vui, r);
                            }
                            2 if self.menu.auction => {
                                // 310: the auction page, the focus
                                out.push(BankOut::ChatMode(1));
                                self.page = Page::Auction;
                                self.sell.focus(vui, r);
                                out.push(BankOut::Auction);
                            }
                            _ => out.push(BankOut::Close),
                        }
                    }
                    19 | 138 => self.menu_move(-1, now, cues),
                    20 | 139 => self.menu_move(1, now, cues),
                    _ => {}
                }
            }
            Page::MoneyMenu => {
                if !down {
                    return;
                }
                match key {
                    // 260 to the money page from its menu: back to the bank
                    27 | 132 => {
                        cues.push(8);
                        self.back(vui, r, out);
                    }
                    13 | 134 => {
                        cues.push(13);
                        match self.money.sel {
                            0 => self.deposit_open(vui, r, v, 1),
                            1 => self.deposit_open(vui, r, v, 2),
                            _ => self.back(vui, r, out),
                        }
                    }
                    19 | 138 => self.money_move(-1, now, cues),
                    20 | 139 => self.money_move(1, now, cues),
                    _ => {}
                }
            }
            Page::Deposit => match self.deposit.key(key, down, now, cues) {
                // 260 from the deposit form: the money menu again
                Some(DepositUp::Cancel) => self.money_reset(vui, r),
                Some(DepositUp::Bank(dir, n)) => {
                    if v.idle {
                        out.push(BankOut::MoveMoney(dir as u8, n));
                        self.money_reset(vui, r);
                    } else {
                        cues.push(9);
                    }
                }
                _ => {}
            },
            Page::Items => self.items_key(vui, r, v, key, down, cues, out),
            Page::Quantity => match self.item.qty.key(key, down, cues) {
                // 293: the move, the picker away
                Some(QuantityUp::Value(n, slot)) => {
                    if self.item.from != -1 && n > 0 {
                        out.push(BankOut::Move(self.item.from as u8, slot, n));
                    }
                    self.item.qty.visible = false;
                    self.page = Page::Items;
                }
                Some(QuantityUp::Cancel) => {
                    self.item.qty.visible = false;
                    self.page = Page::Items;
                }
                None => {}
            },
            Page::Auction => {
                use crate::auction::SellOut;
                let mut so = Vec::new();
                self.sell.key(vui, r, v.bank, key, down, now, cues, &mut so);
                for o in so {
                    match o {
                        SellOut::Back => self.back(vui, r, out),
                        SellOut::Sell(slot, n, a, b, c) => out.push(BankOut::Sell(slot, n, a, b, c)),
                        SellOut::CannotSell => out.push(BankOut::CannotSell),
                    }
                }
            }
        }
    }

    /// The item a row of the item page is under frame point `p` (a
    /// Shift+click's, `itemlink`): its list (0 the inventory, 1 the bank) and
    /// its index there (a row is its list's item of that place); none off the
    /// item page.
    pub fn item_at(&self, p: [f32; 2]) -> Option<(u8, i32)> {
        if !self.visible || self.page != Page::Items {
            return None;
        }
        if crate::settings::inventory_bags() {
            let c = self.item.grid_hits.at(p).filter(|&c| c >= 0)?;
            let side = (c >= BANK_CELLS) as usize;
            let k = *self.item.shown[side].get((c % BANK_CELLS) as usize)?;
            return Some((side as u8, k as i32));
        }
        let side = self.item.hits.at(p)?;
        let list = if side == 0 { &self.item.inv } else { &self.item.bank };
        let row = list.row_at(p)?;
        list.items.get(row as usize).filter(|r| r.flags != 0).map(|_| (side.min(1) as u8, row))
    }

    /// A pointer at frame point `p`, to the page with the keys: a row of
    /// the bank's menu or the money menu chosen at once; on the item page a
    /// row of the active list selected, or its selection moved (Enter), and
    /// on the other list its row selected and the list taken up (left or
    /// right); the deposit form, the quantity picker and the auction's own.
    /// The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.page {
            Page::Menu => match self.menu.hits.at(p) {
                Some(k) => {
                    self.menu.cursor = k;
                    vec![key::CROSS]
                }
                None => Vec::new(),
            },
            Page::MoneyMenu => match self.money.hits.at(p) {
                Some(k) => {
                    self.money.sel = k;
                    vec![key::CROSS]
                }
                None => Vec::new(),
            },
            Page::Deposit => self.deposit.point(p),
            Page::Items if crate::settings::inventory_bags() => {
                let it = &mut self.item;
                match it.grid_hits.at(p) {
                    Some(FILTER_HIT) => vec![key::SQUARE],
                    Some(c) if c == PAGE_HITS[0] => vec![key::L1],
                    Some(c) if c == PAGE_HITS[1] => vec![key::R1],
                    Some(c) if c >= 0 => {
                        let (side, cell) = ((c >= BANK_CELLS) as usize, c % BANK_CELLS);
                        if side == it.side && cell == it.grids[side].sel {
                            return vec![key::CROSS];
                        }
                        it.side = side;
                        it.grids[side].sel = cell;
                        vec![crate::pointer::MOVED]
                    }
                    _ => Vec::new(),
                }
            }
            Page::Items => {
                let it = &mut self.item;
                let Some(side) = it.hits.at(p) else { return Vec::new() };
                let (list, other) = if side == 0 { (&mut it.inv, key::LEFT) } else { (&mut it.bank, key::RIGHT) };
                let row = list.row_at(p);
                if list.x64 {
                    return row.map(|r| list.pick(r)).into_iter().collect();
                }
                if let Some(r) = row {
                    list.pick(r);
                }
                vec![other]
            }
            Page::Quantity => self.item.qty.point(p),
            Page::Auction => self.sell.point(p),
        }
    }

    /// The auction form's editor is up (it takes the keyboard).
    pub fn typing(&self) -> bool {
        self.page == Page::Auction && self.sell.typing()
    }

    /// `VIWndBankItem::OnKeyDown` / `OnKeyUp`, and the lists' 256; L1 and
    /// R1 the bank's page (petition 3297, `turn_page`).
    fn items_key(&mut self, vui: &Vui, r: &dyn Raster, v: &View, key: i32, down: bool, cues: &mut Vec<i32>, out: &mut Vec<BankOut>) {
        if crate::settings::inventory_bags() {
            self.grid_key(vui, r, v, key, down, cues, out);
            return;
        }
        let it = &mut self.item;
        if !down {
            if it.inv.x64 {
                it.inv.key_up_down(key, false);
            } else if it.bank.x64 {
                it.bank.key_up_down(key, false);
            }
            return;
        }
        match key {
            27 | 132 => {
                cues.push(8);
                self.back(vui, r, out);
            }
            140 | 142 => self.turn_page(v, if key == 140 { -1 } else { 1 }, cues, out),
            17 | 136 => {
                cues.push(if it.bank.x64 { 11 } else { 9 });
                it.inv.set_x64(true);
                it.bank.set_x64(false);
            }
            18 | 137 => {
                cues.push(if it.inv.x64 { 11 } else { 9 });
                it.inv.set_x64(false);
                it.bank.set_x64(true);
            }
            19 | 20 | 138 | 139 => {
                if it.inv.x64 {
                    it.inv.key_up_down(key, true);
                } else if it.bank.x64 {
                    it.bank.key_up_down(key, true);
                }
            }
            13 | 134 => {
                // the active list's 256, with its selection
                let from_bank = if it.inv.x64 {
                    false
                } else if it.bank.x64 {
                    true
                } else {
                    return;
                };
                let p1 = if from_bank { it.bank.sel } else { it.inv.sel };
                if !v.idle {
                    cues.push(9);
                    return;
                }
                cues.push(13);
                it.from = -1;
                let list = if from_bank { v.bank } else { v.inv };
                if p1 < 0 || p1 as usize >= list.items.len() {
                    return;
                }
                let item = &list.items[p1 as usize];
                if item.item.max_stack != 0 && item.quantity >= 2 {
                    it.from = from_bank as i32;
                    it.qty.open(vui, r, item.quantity as i32, p1, -1);
                    self.page = Page::Quantity;
                } else {
                    out.push(BankOut::Move(from_bank as u8, p1, 1));
                }
            }
            _ => {}
        }
    }

    /// `VIWndBankItem::UpdateText`: the active list's item (the inventory's
    /// `DescribeInvItemQuick`, the bank's `DescribeBankItem`).
    fn update_text(&mut self, info: &Info, v: &View) {
        let it = &mut self.item;
        it.text.clear();
        if it.inv.x64 && v.inv.items.is_empty() {
            return;
        }
        if it.bank.x64 && v.bank.items.is_empty() {
            return;
        }
        if it.inv.x64 {
            it.text = info.inv_item(it.inv.sel, 0);
        } else if it.bank.x64 {
            let i = it.bank.sel;
            if i >= 0 && (i as usize) < v.bank.items.len() {
                it.text = info.bank_item(&v.bank.items[i as usize]);
            }
        }
    }

    /// The pages drawn (the window's children after it), and the repeating
    /// keys.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, info: &Info, v: &View, icon: &dyn Fn(&NetItem) -> i32, can_use: &dyn Fn(&NetItem) -> bool, now: i64, cues: &mut Vec<i32>) {
        match self.page {
            Page::Menu => self.draw_menu(vui, r),
            Page::MoneyMenu => self.draw_money(vui, r),
            Page::Deposit => self.deposit.draw(vui, r, cues),
            Page::Items | Page::Quantity => {
                self.draw_items(vui, r, info, v, icon, can_use, now, cues);
                if self.page == Page::Quantity {
                    self.item.qty.draw(vui, r, cues);
                }
            }
            Page::Auction => self.sell.draw(vui, r, info, v.bank, icon, now, cues),
        }
    }

    /// `VIWndBankMenu::OnDrawSelf`.
    fn draw_menu(&mut self, vui: &Vui, r: &mut dyn Raster) {
        let m = &mut self.menu;
        let rows = if m.auction { 8 } else { 7 };
        m.window.init(vui, r, [320, 180], 250, rows, 1);
        m.window.draw(vui, r);
        let (white, yellow) = ([1.0, 1.0, 1.0, m.window.open], [1.0, 1.0, 0.0, m.window.open]);
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        let w = m.window.size[0];
        let lit = |k: i32| if m.cursor == k { yellow } else { white };
        m.window.text_box(vui, r, &word(300), 20, 0, w - 40, 1, white, 0);
        m.window.text_box(vui, r, &word(349), 40, 2, w - 60, 1, lit(0), 0);
        m.window.text_box(vui, r, &word(350), 40, 3, w - 60, 1, lit(1), 0);
        if !m.auction {
            m.window.text_box(vui, r, &word(161), 40, 4, w - 60, 1, lit(2), 0);
        } else {
            m.window.text_box(vui, r, &word(1617), 40, 4, w - 60, 1, lit(2), 0);
            m.window.text_box(vui, r, &word(161), 40, 5, w - 60, 1, lit(3), 0);
        }
        m.hits.clear();
        for k in 0..if m.auction { 4 } else { 3 } {
            m.hits.push(rows_rect(&m.window, vui, r, 40, k + 2, w - 60, 1), k);
        }
        m.window.tick_r(vui, r, 31, m.cursor + 2, true);
        let s = wide(&format!("|x{}   |t{}", vui.words.get(212, vui.language), vui.words.get(161, vui.language)));
        m.window.text_box(vui, r, &s, 20, rows - 1, w - 40, 1, white, 0);
    }

    /// `VIWndMoneyMenu::OnDrawSelf`.
    fn draw_money(&mut self, vui: &Vui, r: &mut dyn Raster) {
        let m = &mut self.money;
        m.window.init(vui, r, [320, 180], 250, 7, 1);
        m.window.draw(vui, r);
        let (white, yellow) = ([1.0, 1.0, 1.0, m.window.open], [1.0, 1.0, 0.0, m.window.open]);
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        let w = m.window.size[0];
        let lit = |k: i32| if m.sel == k { yellow } else { white };
        m.window.text_box(vui, r, &word(349), 20, 0, w - 40, 1, white, 0);
        m.window.text_box(vui, r, &word(351), 40, 2, w - 60, 1, lit(0), 0);
        m.window.text_box(vui, r, &word(352), 40, 3, w - 60, 1, lit(1), 0);
        m.window.text_box(vui, r, &word(217), 40, 4, w - 60, 1, lit(2), 0);
        m.hits.clear();
        for k in 0..3 {
            m.hits.push(rows_rect(&m.window, vui, r, 40, k + 2, w - 60, 1), k);
        }
        m.window.tick_r(vui, r, 31, m.sel + 2, true);
        let s = wide(&format!("|x{}   |t{}", vui.words.get(212, vui.language), vui.words.get(217, vui.language)));
        m.window.text_box(vui, r, &s, 20, 6, w - 40, 1, white, 0);
    }

    /// `VIWndBankItem::OnDrawSelf`: the backdrop, the bar with "Bank", the
    /// framed panel between the lists, the text; then both lists' rows set
    /// and the lists drawn.
    fn draw_items(&mut self, vui: &Vui, r: &mut dyn Raster, info: &Info, v: &View, icon: &dyn Fn(&NetItem) -> i32, can_use: &dyn Fn(&NetItem) -> bool, now: i64, cues: &mut Vec<i32>) {
        let white = [1.0, 1.0, 1.0, 1.0];
        crate::ui::window_backdrop(vui, r, 642);
        let title = wide(vui.words.get(300, vui.language));
        let w = vui.string_width(r, &title) + 64;
        vui.blit_bar(r, &title, white, [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
        if crate::settings::inventory_bags() {
            self.draw_grids(vui, r, info, v, icon, can_use, now);
            return;
        }
        let fp = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
        vui.blit_fill_frame(r, [165, 48], [310, 267], &fp, 35, 0, 1.0, true);
        vui.blit_studded_horz(r, [165, 37], 37, 38, 310, 2, 0, 1.0, 0);
        vui.blit_studded_horz(r, [165, 313], 40, 41, 310, 2, 0, 1.0, 0);
        vui.blit_horz(r, [16, 322], 60, 608, 0, 1.0, 0);
        self.update_text(info, v);
        let it = &mut self.item;
        let text = wide(&it.text);
        it.window.text_box(vui, r, &text, 0, 0, 280, 14, [0.0, 0.0, 0.0, 1.0], 2);
        fill(&mut it.inv, v.inv, crate::inventory::pack_cap(), true, icon, can_use);
        fill(&mut it.bank, v.bank, crate::inventory::bank_cap(), false, icon, can_use);
        // the lists' frames, as `VIWndList::OnDrawSelf` sizes them
        let (stud, fill_e) = (vui.entry(36), vui.entry(82));
        let size = [stud.uv1[0] * 2 + fill_e.uv1[0] + 114, stud.uv1[1] * 2 + 264];
        it.hits.clear();
        for (l, k) in [(&it.inv, 0), (&it.bank, 1)] {
            it.hits.push([l.pos[0], l.pos[1], l.pos[0] + size[0], l.pos[1] + size[1]], k);
        }
        it.inv.draw(vui, r, now, cues);
        it.bank.draw(vui, r, now, cues);
    }

    /// The item page at Bags (petition 3265): each side's header (its name,
    /// the room taken, the filter's count when filtered), its grid
    /// (`bags::draw_grid`, the selection only on the side in use), the
    /// framed panel between them with the selected item's text, and the
    /// Filter button under it; the Page buttons under the bank's grid.
    fn draw_grids(&mut self, vui: &Vui, r: &mut dyn Raster, info: &Info, v: &View, icon: &dyn Fn(&NetItem) -> i32, can_use: &dyn Fn(&NetItem) -> bool, now: i64) {
        let orders = self.grid_orders(v);
        let cells = self.grid_counts(v, &orders);
        let it = &mut self.item;
        for (s, g) in GRIDS.iter().enumerate() {
            it.grids[s].show_in(g, cells[s]);
        }
        it.shown = orders;
        // the selected item's text (`UpdateText`'s, the side in use)
        it.text.clear();
        if let Some(&k) = it.shown[it.side].get(it.grids[it.side].sel.max(0) as usize) {
            it.text = if it.side == 0 { info.inv_item(k as i32, 0) } else { v.bank.items.get(k).map_or_else(String::new, |x| info.bank_item(x)) };
        }
        let fp = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
        vui.blit_fill_frame(r, [196, 48], [248, 222], &fp, 35, 0, 1.0, true);
        vui.blit_studded_horz(r, [196, 37], 37, 38, 248, 2, 0, 1.0, 0);
        vui.blit_studded_horz(r, [196, 268], 40, 41, 248, 2, 0, 1.0, 0);
        vui.blit_horz(r, [16, 322], 60, 608, 0, 1.0, 0);
        it.window.init(vui, r, [320, 160], 228, 12, 1);
        let text = wide(&it.text);
        it.window.text_box(vui, r, &text, 0, 0, 218, 11, [0.0, 0.0, 0.0, 1.0], 2);
        let (gold, dim, white) = ([0.95, 0.82, 0.45, 1.0], [0.7, 0.62, 0.42, 1.0], [1.0; 4]);
        let pulse = 0.55 + 0.45 * ((now % 1000) as f32 / 1000.0 * std::f32::consts::TAU).cos();
        it.grid_hits.clear();
        let filtered = it.filter != crate::bags::Filter::All;
        let sides = [(v.inv, "Inventory", crate::inventory::pack_cap(), true), (v.bank, "Bank", crate::inventory::bank_cap(), false)];
        for (s, (list, name, room, worn)) in sides.into_iter().enumerate() {
            let g = &GRIDS[s];
            let head = if filtered { format!("{name}  {} of {}", it.shown[s].len(), list.items.len()) } else { format!("{name}  {}/{room}", list.items.len()) };
            let head = wide(&head);
            let x = if s == 0 { g.origin[0] } else { g.rect()[2] - vui.string_width(r, &head) };
            vui.draw_string_ex(r, &head, x, 34, if s == it.side { gold } else { dim }, true);
            let shown: Vec<Option<crate::bags::Cell>> = (0..cells[s])
                .map(|c| {
                    let k = *it.shown[s].get(c as usize)?;
                    let x = list.items.get(k)?;
                    Some(crate::bags::Cell { icon: icon(x), quantity: x.quantity, worn: worn && x.equip_slot != -1, unusable: x.item.wear_slot != -1 && !can_use(x), carried: false })
                })
                .collect();
            crate::bags::draw_grid(vui, r, g, &it.grids[s], &shown, pulse, s == it.side, if s == 1 { BANK_CELLS } else { 0 }, &mut it.grid_hits);
        }
        // the Filter button, □ before its words on the Controller interface
        let b = FILTER_BUTTON;
        r.fill_fan([0.12, 0.1, 0.06, 0.9], [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]]);
        crate::bags::outline(r, [0.62, 0.52, 0.28, 1.0], b, 1);
        let label = wide(&format!("Filter: {}", it.filter.name()));
        let w = vui.string_width(r, &label);
        let glyph = if crate::settings::pc_mode() { 0 } else { vui.entry(71).uv1[0] + 4 };
        let x = b[0] + (b[2] - b[0] - w - glyph) / 2;
        if glyph > 0 {
            vui.blit_ex(r, 71, [x, b[1] + (b[3] - b[1] - vui.entry(71).uv1[1]) / 2], 3, 1.0, 0);
        }
        vui.draw_string_ex(r, &label, x + glyph, b[1] + 3, white, true);
        it.grid_hits.push(b, FILTER_HIT);
        // the Page buttons (petition 3297): L1's glyph before the first's
        // word and R1's after the second's on the Controller interface,
        // arrows on the PC's
        let pc = crate::settings::pc_mode();
        for (k, b) in PAGE_BUTTONS.into_iter().enumerate() {
            r.fill_fan([0.12, 0.1, 0.06, 0.9], [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]]);
            crate::bags::outline(r, [0.62, 0.52, 0.28, 1.0], b, 1);
            let label = wide(match (pc, k) {
                (true, 0) => "< Page",
                (true, _) => "Page >",
                _ => "Page",
            });
            let w = vui.string_width(r, &label);
            let entry = if k == 0 { 191 } else { 193 };
            let glyph = if pc { 0 } else { vui.entry(entry).uv1[0] + 4 };
            let x = b[0] + (b[2] - b[0] - w - glyph) / 2;
            if glyph > 0 {
                let gx = if k == 0 { x } else { x + w + 4 };
                vui.blit_ex(r, entry, [gx, b[1] + (b[3] - b[1] - vui.entry(entry).uv1[1]) / 2], 3, 1.0, 0);
            }
            vui.draw_string_ex(r, &label, if k == 0 { x + glyph } else { x }, b[1] + 2, white, true);
            it.grid_hits.push(b, PAGE_HITS[k]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::dump::{Item, NetItem};

    fn list(q: &[i64]) -> ItemList {
        let items = q.iter().enumerate().map(|(k, &n)| NetItem { quantity: n, inv_index: 100 + k as u32, equip_slot: -1, quick_slot: -1, item: Item { max_stack: if n >= 2 { 20 } else { 0 }, wear_slot: -1, ..Default::default() }, ..Default::default() }).collect();
        ItemList { p04: 0, items }
    }

    /// A raster that draws nothing: the windows' focus measures with it.
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

    fn vui() -> Vui {
        Vui::new(Default::default(), Default::default(), Vec::new())
    }

    /// The pages' keys without a raster: drive the state as the windows
    /// would, the draws not needed.
    struct Rig {
        b: Bank,
        cues: Vec<i32>,
        out: Vec<BankOut>,
    }

    #[test]
    fn rows_of_both_lists() {
        let mut l = WndList::with_rows([0, 0], 0, 40);
        let mut inv = list(&[1, 5]);
        inv.items[0].equip_slot = 3;
        inv.items[1].item.wear_slot = 2;
        fill(&mut l, &inv, 40, true, &|it| it.inv_index as i32, &|_| false);
        let rows: Vec<_> = l.items[..3].iter().map(|r| (r.flags, r.icon, r.icon2, r.number)).collect();
        assert_eq!(rows, vec![(9, 100, 138, 1), (13, 101, 140, 5), (0, -1, -1, 0)]);
        // the bank side never shows the worn arrows
        let mut l2 = WndList::with_rows([0, 0], 0, 40);
        fill(&mut l2, &inv, 40, false, &|it| it.inv_index as i32, &|_| true);
        assert_eq!((l2.items[0].flags, l2.items[0].icon2), (1, -1));
    }

    /// A server that holds 100 (`inventory::pack_cap`): both lists show all
    /// 100, and scroll down to the last; the control: CLIENT's 40 shows the
    /// first 40 and stops there.
    #[test]
    fn a_list_of_a_hundred_shows_every_row() {
        let inv = list(&[1; 100]);
        let mut l = WndList::with_rows([0, 0], 0, 40);
        fill(&mut l, &inv, 100, false, &|it| it.inv_index as i32, &|_| true);
        assert_eq!((l.items.len(), l.count(), l.items[99].icon), (100, 100, 199));
        let mut cues = Vec::new();
        l.scroll(-1, 0, &mut cues);
        assert_eq!((l.sel, l.top), (99, 94), "Up from the first row: the last, the last six shown");
        // the control: CLIENT's 40, the last row the 40th
        let mut l = WndList::with_rows([0, 0], 0, 40);
        fill(&mut l, &inv, 40, false, &|it| it.inv_index as i32, &|_| true);
        assert_eq!((l.items.len(), l.count()), (40, 40), "the control: CLIENT's 40");
        l.scroll(-1, 0, &mut cues);
        assert_eq!((l.sel, l.top), (39, 34));
    }

    #[test]
    fn enter_moves_one_or_asks_how_many() {
        let mut g = Rig { b: Bank::default(), cues: Vec::new(), out: Vec::new() };
        g.b.page = Page::Items;
        let (inv, bank) = (list(&[1, 7]), list(&[3]));
        let v = View { carried: 0, banked: 0, inv: &inv, bank: &bank, idle: true };
        // a single item from the inventory (row 0): Move(0, 0, 1)
        let r = NullRaster;
        let vui = vui();
        g.b.item.inv.sel = 0;
        g.b.items_key(&vui, &r, &v, 134, true, &mut g.cues, &mut g.out);
        assert_eq!(g.out, vec![BankOut::Move(0, 0, 1)]);
        // a stack: the picker, then 293 moves that many from the same list
        g.out.clear();
        g.b.item.inv.sel = 1;
        g.b.items_key(&vui, &r, &v, 134, true, &mut g.cues, &mut g.out);
        assert_eq!((g.b.page, g.b.item.from, g.b.item.qty.max), (Page::Quantity, 0, 7));
        g.b.key(&vui, &r, &v, 134, true, 0, &mut g.cues, &mut g.out);
        assert_eq!(g.out, vec![BankOut::Move(0, 1, 1)], "the picker opens at 1");
        assert_eq!(g.b.page, Page::Items);
        // right: the bank list, its stack
        g.out.clear();
        g.b.items_key(&vui, &r, &v, 137, true, &mut g.cues, &mut g.out);
        g.b.item.bank.sel = 0;
        g.b.items_key(&vui, &r, &v, 134, true, &mut g.cues, &mut g.out);
        assert_eq!((g.b.page, g.b.item.from), (Page::Quantity, 1));
        // a request waiting: refused with sound 9
        let busy = View { idle: false, ..v };
        g.b.page = Page::Items;
        g.cues.clear();
        g.b.items_key(&vui, &r, &busy, 134, true, &mut g.cues, &mut g.out);
        assert_eq!(g.cues, vec![9]);
        // negative control: a row past the list asks nothing
        g.cues.clear();
        g.out.clear();
        g.b.item.bank.sel = 5;
        g.b.items_key(&vui, &r, &v, 134, true, &mut g.cues, &mut g.out);
        assert!(g.out.is_empty() && g.b.page == Page::Items);
    }

    #[test]
    fn the_menu_and_the_money_page() {
        let mut g = Rig { b: Bank::default(), cues: Vec::new(), out: Vec::new() };
        let r = NullRaster;
        let vui = vui();
        let (inv, bank) = (list(&[]), list(&[]));
        let v = View { carried: 50, banked: 900, inv: &inv, bank: &bank, idle: true };
        g.b.open(&vui, &r, false);
        // the cursor stops at Done (2) without the auction
        for _ in 0..4 {
            g.b.key(&vui, &r, &v, 139, true, 0, &mut g.cues, &mut g.out);
        }
        assert_eq!(g.b.menu.cursor, 2);
        assert_eq!(g.cues, vec![11, 11, 9, 9]);
        g.b.key(&vui, &r, &v, 134, true, 0, &mut g.cues, &mut g.out);
        assert_eq!(g.out, vec![BankOut::Close], "Done closes");
        // with the auction there is a fourth row, and row 2 is the auction
        g.out.clear();
        g.b.open(&vui, &r, true);
        g.b.menu.cursor = 2;
        g.b.key(&vui, &r, &v, 134, true, 0, &mut g.cues, &mut g.out);
        assert_eq!(g.out, vec![BankOut::ChatMode(1), BankOut::Auction]);
        // Money > Withdraw: the form in mode 2 bounded by the bank
        g.out.clear();
        g.b.open(&vui, &r, false);
        g.b.key(&vui, &r, &v, 134, true, 0, &mut g.cues, &mut g.out);
        assert_eq!(g.b.page, Page::MoneyMenu);
        g.b.key(&vui, &r, &v, 139, true, 0, &mut g.cues, &mut g.out);
        g.b.key(&vui, &r, &v, 134, true, 0, &mut g.cues, &mut g.out);
        assert_eq!((g.b.page, g.b.deposit.mode, g.b.deposit.width), (Page::Deposit, 2, 3));
        g.b.deposit.digits[8] = 7;
        g.b.key(&vui, &r, &v, 134, true, 0, &mut g.cues, &mut g.out);
        assert_eq!(g.out, vec![BankOut::MoveMoney(1, 7)]);
        assert_eq!(g.b.page, Page::MoneyMenu, "the money menu again after the request");
        // busy: sound 9 and the form stays
        g.out.clear();
        g.b.key(&vui, &r, &v, 134, true, 0, &mut g.cues, &mut g.out);
        g.b.deposit.digits[8] = 7;
        let busy = View { idle: false, ..v };
        g.cues.clear();
        g.b.key(&vui, &r, &busy, 134, true, 0, &mut g.cues, &mut g.out);
        assert_eq!((g.cues.clone(), g.b.page), (vec![9], Page::Deposit));
        assert!(g.out.is_empty());
    }

    /// The menus take a click at once; on the item page a click on the list
    /// in use selects a row and a second moves it, one on the other list
    /// selects its row and asks for that list (right, as the d-pad does);
    /// the panel between the lists takes nothing.
    #[test]
    fn a_pointer_on_the_pages() {
        use crate::pointer::testing::mid;
        use crate::pointer::{MOVED, key, rows_rect};
        let (vui, mut r) = (vui(), NullRaster);
        let (inv, bank) = (list(&[1, 1, 3]), list(&[2, 1]));
        let base = [0; 29];
        let info = Info { words: &vui.words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let v = View { carried: 50, banked: 900, inv: &inv, bank: &bank, idle: true };
        let (icon, can_use) = (|_: &NetItem| 1, |_: &NetItem| true);
        let (mut cues, mut out) = (Vec::new(), Vec::new());
        let mut b = Bank::default();
        b.open(&vui, &r, true);
        b.draw(&vui, &mut r, &info, &v, &icon, &can_use, 0, &mut cues);
        let w = b.menu.window.size[0];
        let done = mid(rows_rect(&b.menu.window, &vui, &r, 40, 5, w - 60, 1));
        assert_eq!(b.point(done), vec![key::CROSS]);
        assert_eq!(b.menu.cursor, 3, "Done, with the auction's row above it");
        let items = mid(rows_rect(&b.menu.window, &vui, &r, 40, 3, w - 60, 1));
        assert_eq!(b.point(items), vec![key::CROSS]);
        b.key(&vui, &r, &v, 134, true, 0, &mut cues, &mut out);
        assert_eq!(b.page, Page::Items);
        b.draw(&vui, &mut r, &info, &v, &icon, &can_use, 0, &mut cues);
        // the lists at (16, 37) and (475, 37): rows 44 high (no atlas here)
        let row = |x: f32, i: i32| [x + 57.0, 37.0 + 44.0 * i as f32 + 22.0];
        assert!(b.point(items).is_empty(), "the menu is put away");
        assert_eq!(b.point(row(16.0, 2)), vec![MOVED]);
        assert_eq!(b.item.inv.sel, 2);
        assert_eq!(b.point(row(16.0, 2)), vec![key::CROSS], "moved to the bank (Enter)");
        assert_eq!(b.point(row(475.0, 1)), vec![key::RIGHT], "the bank's row, and the bank list asked for");
        assert_eq!(b.item.bank.sel, 1);
        b.key(&vui, &r, &v, key::RIGHT, true, 0, &mut cues, &mut out);
        assert!(b.item.bank.x64 && !b.item.inv.x64);
        assert_eq!(b.point(row(475.0, 1)), vec![key::CROSS]);
        assert!(b.point(row(475.0, 2)).is_empty(), "two items, two rows");
        assert!(b.point([320.0, 150.0]).is_empty(), "the text between the lists");
        assert_eq!(b.point(row(16.0, 0)), vec![key::LEFT]);
        assert_eq!(b.item.inv.sel, 0);
        // a Shift+click's item (`itemlink`): each list's row, its list and place;
        // an empty row and the text between the lists none
        assert_eq!(b.item_at(row(16.0, 2)), Some((0, 2)));
        assert_eq!(b.item_at(row(475.0, 1)), Some((1, 1)));
        assert_eq!(b.item_at(row(475.0, 2)), None, "two items, two rows");
        assert_eq!(b.item_at([320.0, 150.0]), None);
        b.page = Page::Menu;
        assert_eq!(b.item_at(row(16.0, 2)), None, "the control: off the item page");
    }

    /// Inventory Style at Bags (petition 3265): the item page's two grids.
    /// The inventory's in use first; right from its last column the bank's;
    /// ✕ moves the cell's item by its place in its list (a stack asks how
    /// many); □ filters both sides (Equipment: the helm alone on the bank's)
    /// and the cells then hold only what is kept; a click on the other side
    /// takes it up, a second click moves; a Shift+click's item is the
    /// cell's. The control: at Retail List the lists as ever.
    #[test]
    fn bags_show_the_bank_as_grids_with_a_filter() {
        use crate::pointer::key;
        use crate::pointer::testing::mid;
        crate::settings::testing::reset(false);
        let mut c = crate::settings::client();
        c.inventory_bags = true;
        crate::settings::set_client(c);
        let (vui, mut r) = (vui(), NullRaster);
        let inv = list(&[1, 4]);
        let mut bank = list(&[1, 1, 3]);
        bank.items[1].item.wear_slot = 1;
        bank.items[1].item.name = "Iron Helm".into();
        let base = [0; 29];
        let info = Info { words: &vui.words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let v = View { carried: 0, banked: 0, inv: &inv, bank: &bank, idle: true };
        let (icon, can_use) = (|it: &NetItem| it.inv_index as i32, |_: &NetItem| true);
        let (mut cues, mut out) = (Vec::new(), Vec::new());
        let mut b = Bank::default();
        b.open(&vui, &r, false);
        b.key(&vui, &r, &v, key::DOWN, true, 0, &mut cues, &mut out);
        b.key(&vui, &r, &v, key::CROSS, true, 0, &mut cues, &mut out);
        assert_eq!(b.page, Page::Items);
        b.draw(&vui, &mut r, &info, &v, &icon, &can_use, 0, &mut cues);
        assert_eq!(b.item.shown, [vec![0, 1], vec![0, 1, 2]]);
        // right: the second column, then the bank's side
        b.key(&vui, &r, &v, key::RIGHT, true, 0, &mut cues, &mut out);
        assert_eq!((b.item.side, b.item.grids[0].sel), (0, 1));
        b.key(&vui, &r, &v, key::RIGHT, true, 0, &mut cues, &mut out);
        assert_eq!(b.item.side, 1, "the bank's");
        b.key(&vui, &r, &v, key::RIGHT, true, 0, &mut cues, &mut out);
        b.key(&vui, &r, &v, key::CROSS, true, 0, &mut cues, &mut out);
        assert_eq!(out.last(), Some(&BankOut::Move(1, 1, 1)), "the helm, the bank's second item");
        // □: Equipment; the bank's grid shows the helm alone
        b.key(&vui, &r, &v, key::SQUARE, true, 0, &mut cues, &mut out);
        assert_eq!(b.item.filter, crate::bags::Filter::Equipment);
        b.draw(&vui, &mut r, &info, &v, &icon, &can_use, 0, &mut cues);
        assert_eq!(b.item.shown, [vec![], vec![1]]);
        b.key(&vui, &r, &v, key::CROSS, true, 0, &mut cues, &mut out);
        assert_eq!(out.last(), Some(&BankOut::Move(1, 1, 1)), "the bank's first cell is the helm now");
        // the pointer: the filter button, the inventory's cells
        assert_eq!(b.point(mid(FILTER_BUTTON)), vec![key::SQUARE]);
        b.key(&vui, &r, &v, key::SQUARE, true, 0, &mut cues, &mut out);
        b.key(&vui, &r, &v, key::SQUARE, true, 0, &mut cues, &mut out);
        b.key(&vui, &r, &v, key::SQUARE, true, 0, &mut cues, &mut out);
        b.key(&vui, &r, &v, key::SQUARE, true, 0, &mut cues, &mut out);
        assert_eq!(b.item.filter, crate::bags::Filter::All, "round");
        b.draw(&vui, &mut r, &info, &v, &icon, &can_use, 0, &mut cues);
        let cell1 = mid(GRIDS[0].cell_rect(1, 0).unwrap());
        assert_eq!(b.point(cell1), vec![crate::pointer::MOVED]);
        assert_eq!((b.item.side, b.item.grids[0].sel), (0, 1), "the inventory's side taken up");
        assert_eq!(b.item_at(cell1), Some((0, 1)));
        out.clear();
        assert_eq!(b.point(cell1), vec![key::CROSS]);
        b.key(&vui, &r, &v, key::CROSS, true, 0, &mut cues, &mut out);
        assert_eq!((b.page, b.item.from, b.item.qty.max), (Page::Quantity, 0, 4), "a stack: how many");
        assert!(out.is_empty());
        // the control: Retail List
        b.page = Page::Items;
        crate::settings::testing::reset(false);
        b.draw(&vui, &mut r, &info, &v, &icon, &can_use, 0, &mut cues);
        assert_eq!(b.point([475.0 + 57.0, 37.0 + 22.0]), vec![key::RIGHT], "the bank's list row, as ever");
        assert_eq!(b.item_at([16.0 + 57.0, 37.0 + 22.0]), Some((0, 0)), "the inventory's first row");
    }
    /// The bank's pages (petition 3297): on the item page R1 and L1 turn
    /// the page ("/bank next", "/bank prev"), the bank's grid from its top
    /// again; at Bags the Page buttons under the bank's grid press them; at
    /// Retail List the shoulder buttons do the same, the bank list from its
    /// top. The controls: a request waiting refuses the turn (sound 9,
    /// nothing asked); the bank's menu takes no L1 or R1; the Filter button
    /// is still □.
    #[test]
    fn the_shoulder_buttons_turn_the_banks_page() {
        use crate::pointer::key;
        use crate::pointer::testing::mid;
        crate::settings::testing::reset(false);
        let mut c = crate::settings::client();
        c.inventory_bags = true;
        crate::settings::set_client(c);
        let (vui, mut r) = (vui(), NullRaster);
        let (inv, bank) = (list(&[1, 4]), list(&[1; 30]));
        let base = [0; 29];
        let info = Info { words: &vui.words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let v = View { carried: 0, banked: 0, inv: &inv, bank: &bank, idle: true };
        let (icon, can_use) = (|it: &NetItem| it.inv_index as i32, |_: &NetItem| true);
        let (mut cues, mut out) = (Vec::new(), Vec::new());
        let mut b = Bank::default();
        b.open(&vui, &r, false);
        // the control: the menu takes no shoulder button
        b.key(&vui, &r, &v, key::R1, true, 0, &mut cues, &mut out);
        assert!(out.is_empty() && cues.is_empty(), "the menu: nothing");
        b.key(&vui, &r, &v, key::DOWN, true, 0, &mut cues, &mut out);
        b.key(&vui, &r, &v, key::CROSS, true, 0, &mut cues, &mut out);
        assert_eq!(b.page, Page::Items);
        out.clear();
        b.draw(&vui, &mut r, &info, &v, &icon, &can_use, 0, &mut cues);
        // down the bank's grid, then R1: the next page, the grid at its top
        b.item.side = 1;
        for _ in 0..9 {
            b.key(&vui, &r, &v, key::DOWN, true, 0, &mut cues, &mut out);
        }
        assert!(b.item.grids[1].top > 0, "scrolled");
        cues.clear();
        b.key(&vui, &r, &v, key::R1, true, 0, &mut cues, &mut out);
        assert_eq!((out.clone(), cues.clone()), (vec![BankOut::Page(1)], vec![11]));
        assert_eq!((b.item.grids[1].sel, b.item.grids[1].top, b.item.side), (0, 0, 1), "from the top, the bank's side kept");
        b.key(&vui, &r, &v, key::L1, true, 0, &mut cues, &mut out);
        assert_eq!(out.last(), Some(&BankOut::Page(-1)));
        assert_eq!((page_line(-1), page_line(1)), ("/bank prev", "/bank next"));
        // a key let go asks nothing
        out.clear();
        b.key(&vui, &r, &v, key::R1, false, 0, &mut cues, &mut out);
        assert!(out.is_empty());
        // the control: a request waiting
        let busy = View { idle: false, ..v };
        cues.clear();
        b.key(&vui, &r, &busy, key::R1, true, 0, &mut cues, &mut out);
        assert_eq!((out.is_empty(), cues.clone()), (true, vec![9]), "refused while a request waits");
        // the pointer: the Page buttons, the Filter button as before
        b.draw(&vui, &mut r, &info, &v, &icon, &can_use, 0, &mut cues);
        assert_eq!(b.point(mid(PAGE_BUTTONS[0])), vec![key::L1]);
        assert_eq!(b.point(mid(PAGE_BUTTONS[1])), vec![key::R1]);
        assert_eq!(b.point(mid(FILTER_BUTTON)), vec![key::SQUARE], "the control: the Filter's");
        assert_eq!(b.item_at(mid(PAGE_BUTTONS[1])), None, "a button is no item");
        // under the bank's grid, clear of its cells and the Filter button
        let g = GRIDS[1].rect();
        for p in PAGE_BUTTONS {
            assert!(p[1] > g[3] && p[0] >= g[0] && p[2] <= g[2] && p[3] < 322, "{p:?} under {g:?}");
        }
        // Retail List: the shoulder buttons the same, the bank list from its top
        crate::settings::testing::reset(false);
        out.clear();
        b.key(&vui, &r, &v, key::RIGHT, true, 0, &mut cues, &mut out);
        b.item.bank.sel = 20;
        b.item.bank.top = 15;
        b.key(&vui, &r, &v, key::R1, true, 0, &mut cues, &mut out);
        assert_eq!(out, vec![BankOut::Page(1)]);
        assert_eq!((b.item.bank.sel, b.item.bank.top), (0, 0));
        b.draw(&vui, &mut r, &info, &v, &icon, &can_use, 0, &mut cues);
        assert!(b.point(mid(PAGE_BUTTONS[1])).iter().all(|&k| k != key::R1), "the control: no Page button on the game's lists");
    }
}
