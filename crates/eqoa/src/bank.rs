//! The bank window (CLIENT `viwndbank.cpp`, `viwndbankmenu.cpp`,
//! `viwndbankmoney.cpp`, `viwndbankitem.cpp`, `VIWndMoneyMenu.cpp`; the game
//! window's mode 11): `VIWndBank` and its pages. The menu (Money, Items, the
//! auction when the bank offers one, Done); the money page, a three-way menu
//! (deposit, withdraw, back) over `VIWndDeposit` in its bank modes 1 and 2;
//! the item page, the inventory and the bank side by side (40 rows each), the
//! selected item's text between them, `VIWndQuantity` for a stack.
//!
//! As in the other windows, the window tree's messages come back up as
//! values: `BankOut` is what `VIBankControl` and `VIWndGame` are asked.
//!
//! A pointer (`pointer`) goes to the page with the keys: the menus' rows at
//! once; on the item page a row of the list in use selected or its
//! selection moved, a row of the other list selected and that list taken
//! up (left or right, as the d-pad does); the pickers and the auction's
//! form as theirs.

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
}

impl Default for BankItem {
    fn default() -> Self {
        // Init: the two lists (`SetX84(1)`), 40 rows each
        let mut inv = WndList::with_rows([16, 37], 0, 40);
        let mut bank = WndList::with_rows([475, 37], 0, 40);
        inv.x84 = true;
        bank.x84 = true;
        BankItem { inv, bank, qty: Quantity::default(), from: -1, text: String::new(), window: Window::default(), hits: Hits::default() }
    }
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
/// item (the inventory) the arrows, one the player cannot use the red ones.
fn fill(list: &mut WndList, l: &ItemList, worn: bool, icon: &dyn Fn(&NetItem) -> i32, can_use: &dyn Fn(&NetItem) -> bool) {
    let n = l.items.len().min(40);
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
    for i in n..40 {
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
        it.window.init(vui, r, [323, 188], 290, 15, 1);
        it.window.open(true);
        self.page = Page::Items;
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

    /// `VIWndBankItem::OnKeyDown` / `OnKeyUp`, and the lists' 256.
    fn items_key(&mut self, vui: &Vui, r: &dyn Raster, v: &View, key: i32, down: bool, cues: &mut Vec<i32>, out: &mut Vec<BankOut>) {
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
        let fp = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
        vui.blit_fill_frame(r, [165, 48], [310, 267], &fp, 35, 0, 1.0, true);
        vui.blit_studded_horz(r, [165, 37], 37, 38, 310, 2, 0, 1.0, 0);
        vui.blit_studded_horz(r, [165, 313], 40, 41, 310, 2, 0, 1.0, 0);
        vui.blit_horz(r, [16, 322], 60, 608, 0, 1.0, 0);
        self.update_text(info, v);
        let it = &mut self.item;
        let text = wide(&it.text);
        it.window.text_box(vui, r, &text, 0, 0, 280, 14, [0.0, 0.0, 0.0, 1.0], 2);
        fill(&mut it.inv, v.inv, true, icon, can_use);
        fill(&mut it.bank, v.bank, false, icon, can_use);
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
        fill(&mut l, &inv, true, &|it| it.inv_index as i32, &|_| false);
        let rows: Vec<_> = l.items[..3].iter().map(|r| (r.flags, r.icon, r.icon2, r.number)).collect();
        assert_eq!(rows, vec![(9, 100, 138, 1), (13, 101, 140, 5), (0, -1, -1, 0)]);
        // the bank side never shows the worn arrows
        let mut l2 = WndList::with_rows([0, 0], 0, 40);
        fill(&mut l2, &inv, false, &|it| it.inv_index as i32, &|_| true);
        assert_eq!((l2.items[0].flags, l2.items[0].icon2), (1, -1));
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
    }
}
