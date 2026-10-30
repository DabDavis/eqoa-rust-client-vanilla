//! The merchant (CLIENT `viwndmerchant.cpp`, `viwndmercmenu.cpp`;
//! `VIWndMerchant`, the game window's mode 10): first the greeting with Buy,
//! Sell and Leave (`VIWndMercMenu`), then a list at (12, 48) of the
//! merchant's wares or the player's inventory over the panels, the selected
//! item's text, its description, its price and the player's coin, how each
//! of its stats would change the player's, and the Buy or Sell banner. A
//! stack asks how many (`VIWndQuantity`, with the total), and selling
//! something worn, quick-slotted or no-trade asks first (`VIWndMBox`).
//!
//! The merchant is `VIClient`'s `m_merchant` and `m_p1B300`.. off 0x00B7:
//! the merchant's id, the sell and buy rates, the wares, the greeting. A
//! request out (`m_p1B304`) holds Enter until 0x003B or 0x003C (with its
//! sound) or 0x00D7 answers.
//!
//! A pointer (`pointer`) goes to the window with the focus: the menu's
//! Buy, Sell and Leave at once, the list's rows (a second click buys or
//! sells), the picker and the box.
//!
//! In a window wider than 4:3 (the client's own, `ui::Layout`) it stays the
//! 4:3 picture the game draws, as the start menu does: its left panel and its
//! two rails run off a TV's sides, and there posts close them at the
//! picture's edges (`startmenu::close_sides`).

#![allow(clippy::too_many_arguments)]

use crate::dialog::{BoxAnswer, Key as BoxKey, MBox};
use crate::infotext::{self, Info};
use crate::pointer::{Hits, key, rows_rect};
use crate::quantity::{Quantity, QuantityUp};
use crate::ui::Layout;
use crate::wndlist::WndList;
use net::dump::{ItemList, NetItem};
use render::ui::Anchor;
use vi::fpu::{Ee, F};
use vi::ui::{FrameParam, Raster, UI_COLORS, Vui, Window, wide};

/// Where the list's rails run (`Merchant::draw`): the top one's top and the
/// height down to the bottom one's foot, which the posts closing its edges
/// span (`startmenu::close_sides`, the start menu's middle frame's own).
const RAILS: [i32; 2] = [37, 331 - 37];

/// The bottom rail's line (the start menu's, `startmenu::BOTTOM_RAIL`).
const BOTTOM_RAIL: i32 = crate::startmenu::BOTTOM_RAIL;

/// `VIClient`'s merchant: 0x00B7.
#[derive(Clone, Debug, Default)]
pub struct Shop {
    /// `m_p1B300`: the merchant's id; `None` is closed (-1)
    pub id: Option<u32>,
    /// `m_p1B308`, `m_p1B30C`: the sell and buy rates, percent
    pub sell_rate: i32,
    pub buy_rate: i32,
    /// `m_merchant`
    pub wares: ItemList,
    /// `m_merchantName`: the greeting, raw
    pub greeting: Vec<u16>,
    /// `m_p1B304`: a buy or a sale is waiting for the server
    pub pending: bool,
}

/// 0x00B7 as the dispatcher reads it: the id (raw), the two rates
/// (signed), the wares (`VIItemList::Read`), the greeting (`ReadString`,
/// 256 bytes).
pub fn read_shop(data: &[u8]) -> Shop {
    let mut r = net::stream::Reader::new(data);
    let id = r.u32();
    let sell_rate = r.signed() as i32;
    let buy_rate = r.signed() as i32;
    let wares = net::dump::read_item_list(&mut r);
    let greeting: Vec<u16> = r.wide(128).encode_utf16().collect();
    Shop { id: Some(id), sell_rate, buy_rate, wares, greeting, pending: false }
}

/// value × rate / 100, a remainder of 50 or more rounding up.
fn percent(value: i64, rate: i32) -> i32 {
    let v = value as i32 * rate;
    v / 100 + (v % 100 >= 50) as i32
}

/// `GetMerchantPrice`: what one of the merchant's item costs.
pub fn merchant_price(it: &NetItem, buy_rate: i32) -> i32 {
    percent(it.item.value, buy_rate)
}

/// `GetInvItemSellPrice`: what one of an inventory item fetches, less its
/// wear (`GetInvItemWear`: half the condition lost; a broken item half).
pub fn sell_price(it: &NetItem, sell_rate: i32) -> i32 {
    let c = if it.item.max_hp == 0 { F::<Ee>::ONE } else { F::<Ee>::from_int(it.cur_hp as i32) / F::from_int(it.item.max_hp as i32) };
    let wear = if c == F::ZERO {
        F::new(0.5)
    } else if c == F::ONE {
        F::ZERO
    } else {
        (F::ONE - c) * F::new(0.5)
    };
    let r = percent(it.item.value, sell_rate);
    r - (F::<Ee>::from_int(r) * wear).0 as i32
}

/// What the merchant asks of the client.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MerchantOut {
    /// `MerchantClose`
    Close,
    /// `MerchantRequest(i, n)`: 0x004A (74), the ware's index and the count
    Buy(i32, i32),
    /// `SellItem(i, n)`: 0x004B (75), the inventory item and the count
    Sell(i32, i32),
    /// 288 to the game window: the chat box's mode
    ChatMode(i32),
    /// 823: the item would not fit to be compared
    Word(i32),
}

/// `VIWndMercMenu`: the greeting and Buy (63), Sell (62), Leave (161).
#[derive(Default)]
struct MercMenu {
    sel: i32,
    text: Vec<u16>,
    rows: i32,
    window: Window,
    visible: bool,
    /// Buy, Sell and Leave where they were drawn, for a pointer
    hits: Hits,
}

impl MercMenu {
    /// `Move(d)`.
    fn move_sel(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        if self.sel + d < 0 {
            cues.push(9);
            self.sel = 0;
        } else if self.sel + d >= 3 {
            cues.push(9);
            self.sel = 2;
        } else {
            cues.push(11);
            self.sel += d;
        }
        self.window.reset_tick_pulse(now);
    }

    /// 295: the window sized to the greeting.
    fn focus(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.window.init(vui, r, [320, 180], 390, 10, 1);
        let w = self.window.size[0];
        self.rows = self.window.count_lines(vui, r, &self.text, w - 40);
        self.window.init(vui, r, [320, 180], w, self.rows + 6, 1);
        self.window.close(true);
        self.window.open(false);
    }

    fn draw(&mut self, vui: &Vui, r: &mut dyn Raster) {
        let (white, yellow) = ([1.0, 1.0, 1.0, self.window.open], [1.0, 1.0, 0.0, self.window.open]);
        self.window.draw(vui, r);
        let w = self.window.size[0];
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        self.window.text_box(vui, r, &self.text, 20, 0, w - 40, self.rows, white, 0);
        self.hits.clear();
        for (k, id) in [63, 62, 161].into_iter().enumerate() {
            let k = k as i32;
            self.window.text_box(vui, r, &word(id), 40, self.rows + 1 + k, w - 60, 1, if self.sel == k { yellow } else { white }, 0);
            self.hits.push(rows_rect(&self.window, vui, r, 40, self.rows + 1 + k, w - 60, 1), k);
        }
        let s = wide(&format!("|t{}", vui.words.get(161, vui.language)));
        self.window.text_box(vui, r, &s, 20, self.rows + 5, w - 40, 1, white, 0);
        self.window.tick_r(vui, r, 35, self.rows + 1 + self.sel, true);
    }
}

/// Which window has the keys.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Focus {
    Menu,
    List,
    Quantity,
    MBox,
}

/// One of the six stat lines and its colour.
#[derive(Clone, Debug)]
struct Line {
    text: String,
    color: [f32; 4],
}

pub struct Merchant {
    menu: MercMenu,
    list: WndList,
    quantity: Quantity,
    mbox: MBox,
    /// 0 the menu, 1 selling, 2 buying
    pub mode: i32,
    focus: Focus,
    window: Window,
    stats: String,
    desc: String,
    price: i32,
    coin: i32,
    lines: Vec<Line>,
    pub visible: bool,
}

impl Default for Merchant {
    fn default() -> Self {
        let list = WndList::with_rows([12, 48], 1, 40);
        let mut mbox = MBox::default();
        mbox.pos = [180, 115];
        mbox.p1 = -1;
        mbox.p2 = -1;
        Merchant {
            menu: MercMenu::default(),
            list,
            quantity: Quantity::default(),
            mbox,
            mode: 0,
            focus: Focus::Menu,
            window: Window::default(),
            stats: String::new(),
            desc: String::new(),
            price: -1,
            coin: 0,
            lines: Vec::new(),
            visible: false,
        }
    }
}

/// What the merchant reads off the client.
pub struct View<'a> {
    pub info: &'a Info<'a>,
    pub shop: &'a Shop,
    /// `m_player.m_p34`
    pub coin: i32,
    /// `CanUseListItem`, by the item
    pub can_use: &'a dyn Fn(&NetItem) -> bool,
    pub inv_icon: &'a dyn Fn(&NetItem) -> i32,
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

impl Merchant {
    /// `VIWndMerchant::Open` (on `MerchantOpen`, and Escape from a list):
    /// back to the buy/sell menu, the greeting expanded.
    pub fn open(&mut self, vui: &Vui, r: &mut dyn Raster, greeting: &[u16]) {
        self.mode = 0;
        self.list.set_x64(true);
        self.list.reset_scroll();
        self.quantity.visible = false;
        self.mbox.p1 = -1;
        self.menu.text = greeting.to_vec();
        self.menu.sel = 0;
        self.menu.visible = true;
        self.menu.focus(vui, r);
        self.focus = Focus::Menu;
        self.visible = true;
    }

    /// `ShowList`, and the focus (295): the text window placed.
    fn show_list(&mut self, vui: &Vui, r: &dyn Raster, now: i64) {
        self.list.set_x64(true);
        self.list.reset_scroll();
        self.list.open(now);
        self.quantity.visible = false;
        self.menu.sel = 0;
        self.menu.visible = false;
        self.focus = Focus::List;
        self.list.x84 = true;
        self.window.init(vui, r, [403, 180], 490, 15, 1);
        self.window.open(true);
    }

    /// A pointer at frame point `p`, to the window with the focus: the
    /// menu's Buy, Sell or Leave chosen at once; a row of the list selected,
    /// or the selection bought or sold (Enter); the picker's and the box's.
    /// The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.focus {
            Focus::Menu => match self.menu.hits.at(p) {
                Some(k) => {
                    self.menu.sel = k;
                    vec![key::CROSS]
                }
                None => Vec::new(),
            },
            Focus::List => self.list.row_at(p).map(|row| self.list.pick(row)).into_iter().collect(),
            Focus::Quantity => self.quantity.point(p),
            Focus::MBox => self.mbox.point(p).then_some(key::CROSS).into_iter().collect(),
        }
    }

    /// The keys of whichever window has them.
    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, view: &View, key: i32, down: bool, cues: &mut Vec<i32>, out: &mut Vec<MerchantOut>) {
        let now = vui.time;
        match self.focus {
            Focus::Menu => {
                if !down {
                    return;
                }
                match key {
                    27 | 132 => {
                        cues.push(8);
                        out.push(MerchantOut::Close);
                    }
                    13 | 134 => {
                        cues.push(13);
                        match self.menu.sel {
                            // 281 buy, 280 sell
                            0 | 1 => {
                                out.push(MerchantOut::ChatMode(1));
                                self.mode = if self.menu.sel == 0 { 2 } else { 1 };
                                self.show_list(vui, r, now);
                            }
                            _ => out.push(MerchantOut::Close),
                        }
                    }
                    19 | 138 => self.menu.move_sel(-1, now, cues),
                    20 | 139 => self.menu.move_sel(1, now, cues),
                    _ => {}
                }
            }
            Focus::List => {
                if !down {
                    if matches!(key, 19 | 20 | 138 | 139) {
                        self.list.key_up_down(key, false);
                    }
                    return;
                }
                match key {
                    27 | 132 => {
                        cues.push(8);
                        out.push(MerchantOut::ChatMode(0));
                        let greeting = self.menu.text.clone();
                        self.open(vui, r, &greeting);
                    }
                    19 | 20 | 138 | 139 => self.list.key_up_down(key, true),
                    13 | 134 => {
                        if view.shop.pending {
                            cues.push(9);
                        } else {
                            self.pick(vui, r, view, self.list.sel, cues, out);
                        }
                    }
                    _ => {}
                }
            }
            Focus::Quantity => match self.quantity.key(key, down, cues) {
                Some(QuantityUp::Value(n, id)) => {
                    // 293
                    self.quantity.visible = false;
                    self.focus = Focus::List;
                    if n <= 0 {
                        cues.push(13);
                    } else if self.mode == 2 {
                        out.push(MerchantOut::Buy(id, n));
                    } else if self.mode == 1 {
                        self.sell(view, id, n, now, out);
                    }
                }
                Some(QuantityUp::Cancel) => {
                    self.quantity.visible = false;
                    self.focus = Focus::List;
                }
                None => {}
            },
            Focus::MBox => {
                if !down {
                    return;
                }
                let Some(k) = box_key(key) else { return };
                match self.mbox.key(k, cues) {
                    Some(BoxAnswer::Yes(i, n)) => {
                        // 268: sold after all
                        out.push(MerchantOut::Sell(i, n));
                        self.focus = Focus::List;
                    }
                    Some(BoxAnswer::No(..)) => self.focus = Focus::List,
                    None => {}
                }
            }
        }
    }

    /// 256: a row picked. A ware that stacks, or a stack of the player's,
    /// asks how many (with the unit price); else one is bought or sold.
    fn pick(&mut self, vui: &Vui, r: &dyn Raster, view: &View, p1: i32, cues: &mut Vec<i32>, out: &mut Vec<MerchantOut>) {
        let now = vui.time;
        let ware = view.shop.wares.items.get(p1.max(0) as usize).filter(|_| p1 >= 0);
        let mine = view.info.inv.items.get(p1.max(0) as usize).filter(|_| p1 >= 0);
        if self.mode == 2
            && let Some(w) = ware.filter(|w| w.item.max_stack != 0)
        {
            cues.push(13);
            self.quantity.open(vui, r, w.item.max_stack as i32, p1, merchant_price(w, view.shop.buy_rate));
            self.focus = Focus::Quantity;
        } else if self.mode == 1
            && let Some(m) = mine.filter(|m| m.quantity >= 2)
        {
            cues.push(13);
            self.quantity.open(vui, r, m.quantity as i32, p1, sell_price(m, view.shop.sell_rate));
            self.focus = Focus::Quantity;
        } else if self.mode == 2 && ware.is_some() {
            out.push(MerchantOut::Buy(p1, 1));
        } else if self.mode == 1 && mine.is_some() {
            self.sell(view, p1, 1, now, out);
        }
    }

    /// `Sell(i, n)` as SOE's final live client has it (0x013858A0):
    /// something worn (930), quick-slotted (931) or no-trade (932) asks
    /// first, as on the disc, and so does anything but a misc item (loot:
    /// `IsMisc`), with 1842, 'Sell item "<name>"?'; a misc item sells at
    /// once. The box starts on No.
    fn sell(&mut self, view: &View, i: i32, n: i32, now: i64, out: &mut Vec<MerchantOut>) {
        let Some(it) = view.info.inv.items.get(i as usize) else { return };
        let w = |id: i32| crate::livewords::word(view.info.words, id, 0);
        let warn = if it.equip_slot != -1 {
            Some(w(930).replacen("%S", &it.item.name, 1).replacen("%s", &it.item.name, 1))
        } else if it.quick_slot >= 0 {
            Some(w(931).replacen("%S", &it.item.name, 1).replacen("%s", &it.item.name, 1))
        } else if it.item.no_trade > 0 {
            Some(w(932))
        } else if !crate::infotext::is_misc(it) {
            Some(w(1842).replacen("%S", &it.item.name, 1).replacen("%s", &it.item.name, 1))
        } else {
            None
        };
        match warn {
            Some(s) => {
                self.mbox.prompt(&wide(&s), 1, i, n, now);
                self.focus = Focus::MBox;
            }
            None => out.push(MerchantOut::Sell(i, n)),
        }
    }

    /// `UpdateInfo`: the selected row's text, description and price; for
    /// something wearable, each stat as it would change the player's.
    fn update_info(&mut self, view: &View, out: &mut Vec<MerchantOut>) {
        self.stats.clear();
        self.desc.clear();
        self.lines.clear();
        let inv = &view.info.inv.items;
        let wares = &view.shop.wares.items;
        if (self.mode == 1 && inv.is_empty()) || (self.mode == 2 && wares.is_empty()) || self.mode == 0 {
            self.price = -1;
            self.coin = view.coin;
            return;
        }
        let sel = self.list.sel;
        let it = if self.mode == 1 { inv.get(sel as usize) } else { wares.get(sel as usize) };
        let Some(it) = it else { return };
        self.stats = if self.mode == 1 { view.info.inv_item(sel, 0) } else { view.info.merchant_item(it) };
        self.desc = it.item.desc.clone();
        self.price = if self.mode == 1 { sell_price(it, view.shop.sell_rate) } else { merchant_price(it, view.shop.buy_rate) };
        self.coin = view.coin;
        if it.item.wear_slot == -1 {
            return;
        }
        for st in it.item.stats.iter().take(6) {
            let now = view.info.stat_total(st.id);
            let then = if self.mode == 2 {
                let (v, fit) = view.info.stat_with_merchant_item(it, st.id);
                if !fit {
                    out.push(MerchantOut::Word(823));
                }
                v
            } else {
                view.info.stat_with_item(sel as usize, st.id)
            };
            let text = match infotext::stat_word(st.id) {
                Some(w) => format!("{}: {} |}} {}", view.info.words.get(w, 0), st.value, then),
                None => format!("{}: {} ", st.id, st.value),
            };
            let color = if now < then {
                [0.039_215_688, 0.196_078_43, 0.274_509_82, 1.0]
            } else if then < now {
                [0.372_549_03, 0.098_039_22, 0.098_039_22, 1.0]
            } else {
                [0.0, 0.0, 0.0, 1.0]
            };
            self.lines.push(Line { text, color });
        }
    }

    /// `DrawInfo`: the item text and description, the price and the coin,
    /// the stat lines, the banner (63 buying, 62 selling).
    fn draw_info(&mut self, vui: &Vui, r: &mut dyn Raster) {
        let black = UI_COLORS[0];
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        self.window.text_box(vui, r, &wide(&self.stats), 20, 0, 270, 11, black, 2);
        self.window.text_box(vui, r, &wide(&self.desc), 20, 12, 445, 3, black, 2);
        if self.price != -1 {
            self.window.text(vui, r, &wide(&format!("{}:", word(505))), 310, 0, black, 0);
            self.window.text_box(vui, r, &wide(&format!("{} {}", self.price, word(479))), 310, 1, 160, 1, black, 2);
        }
        self.window.text(vui, r, &wide(&format!("{}:", word(506))), 310, if self.price != -1 { 2 } else { 0 }, black, 0);
        self.window.text_box(vui, r, &wide(&format!("{} {}", self.coin, word(479))), 310, if self.price == -1 { 1 } else { 3 }, 160, 1, black, 2);
        let mut row = if self.price == -1 { 2 } else { 4 };
        for l in self.lines.iter().take(6) {
            if row >= 11 {
                break;
            }
            if !l.text.is_empty() {
                let text = wide(&l.text);
                let mut n = self.window.count_lines(vui, r, &text, 160);
                if row == 10 {
                    n = n.min(1);
                }
                self.window.text_box(vui, r, &text, 310, row, 160, n, l.color, 2);
                row += n;
            }
        }
        let banner = match self.mode {
            2 => 63,
            1 => 62,
            _ => return,
        };
        let s = wide(&word(banner));
        let w = vui.string_width(r, &s) + 64;
        vui.blit_bar(r, &s, [1.0; 4], [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
    }

    /// `FillInvList` / `FillMerchantList`.
    fn fill(&mut self, view: &View) {
        let n;
        if self.mode == 1 {
            let inv = &view.info.inv.items;
            n = inv.len().min(40);
            for (i, it) in inv.iter().take(n).enumerate() {
                let mut flags = if it.quantity >= 2 { 5 } else { 1 };
                let mut arrow = -1;
                if it.equip_slot != -1 {
                    arrow = 138;
                    flags |= 8;
                }
                if it.item.wear_slot != -1 && !(view.can_use)(it) {
                    arrow = 140;
                    flags |= 8;
                }
                self.list.set_item(i, flags, (view.inv_icon)(it), arrow, -1, it.quantity as i32);
            }
        } else {
            let wares = &view.shop.wares.items;
            n = wares.len().min(40);
            for (i, it) in wares.iter().take(n).enumerate() {
                let mut flags = 1;
                let mut arrow = -1;
                if it.item.wear_slot != -1 && !(view.can_use)(it) {
                    arrow = 140;
                    flags |= 8;
                }
                self.list.set_item(i, flags, (view.inv_icon)(it), arrow, -1, 1);
            }
        }
        for i in n..40 {
            self.list.set_item(i, 0, -1, -1, -1, 0);
        }
    }

    /// `OnDrawSelf` (the panels behind the list and the text, the rows),
    /// then the children: the menu, the list, the picker, the box.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Layout, view: &View, cues: &mut Vec<i32>, out: &mut Vec<MerchantOut>) {
        let now = vui.time;
        if view.shop.id.is_none() {
            return;
        }
        self.update_info(view, out);
        if self.mode != 0 {
            crate::ui::window_backdrop(vui, r, 642);
            let fp = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
            r.anchor(Anchor::Frame);
            {
                // ended at the bottom rail's bar, as the start menu is
                let r = &mut crate::ui::Cut::new(r, BOTTOM_RAIL + crate::startmenu::RAIL_BAR);
                vui.blit_rect(r, 51, [-1, 48], [156, 265], 0, 1.0);
                vui.blit_fill_frame(r, [456, 48], [184, 202], &fp, 35, 0, 1.0, true);
                vui.blit_fill_frame(r, [167, 48], [289, 202], &fp, 35, 0, 1.0, true);
                vui.blit_fill_frame(r, [167, 250], [473, 65], &fp, 35, 0, 1.0, true);
                vui.blit_studded_horz(r, [-1, 37], 37, 38, 642, 5, 0, 1.0, 0);
                vui.blit_studded_horz(r, [-1, BOTTOM_RAIL], 40, 41, 642, 5, 0, 1.0, 0);
                crate::startmenu::close_sides(vui, r, RAILS, false);
            }
            r.anchor(Anchor::Middle);
            self.fill(view);
            self.draw_info(vui, r);
        }
        if self.menu.visible && self.mode == 0 {
            self.menu.draw(vui, r);
        }
        if self.mode != 0 {
            self.list.draw(vui, r, now, cues);
        }
        if self.quantity.visible {
            self.quantity.draw(vui, r, cues);
        }
        if self.focus == Focus::MBox {
            self.mbox.draw(vui, r, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(value: i64, hp: i64, max: i64) -> NetItem {
        let mut n = NetItem { quantity: 1, cur_hp: hp, equip_slot: -1, quick_slot: -1, ..Default::default() };
        n.item.value = value;
        n.item.max_hp = max;
        n
    }

    #[test]
    fn prices_round_half_up_and_wear_takes_its_share() {
        assert_eq!(merchant_price(&item(15, 0, 0), 110), 17, "16.5 rounds up");
        assert_eq!(merchant_price(&item(13, 0, 0), 110), 14, "14.3 rounds down");
        assert_eq!(sell_price(&item(100, 0, 0), 100), 100, "no hit points: no wear");
        assert_eq!(sell_price(&item(100, 50, 50), 100), 100, "whole");
        assert_eq!(sell_price(&item(100, 25, 50), 100), 75, "half worn: a quarter off");
        assert_eq!(sell_price(&item(100, 0, 50), 100), 50, "broken: half off");
        assert_eq!(sell_price(&item(10, 1, 3), 100), 7, "10 - (int)(10 * 0.3333) = 7");
    }

    #[test]
    fn the_shop_message() {
        let mut w = net::stream::Writer::default();
        w.u32(0x1234).signed(90).signed(110);
        w.signed(0).u32(0); // an empty list
        w.u32(5);
        for c in "Hello".encode_utf16() {
            w.bytes(&c.to_le_bytes());
        }
        let s = read_shop(&w.data);
        assert_eq!((s.id, s.sell_rate, s.buy_rate, s.wares.items.len()), (Some(0x1234), 90, 110, 0));
        assert_eq!(String::from_utf16_lossy(&s.greeting), "Hello");
    }

    /// In a window wider than 4:3 the merchant is the 4:3 picture the game
    /// draws, as the start menu is: every piece at the game's place, nothing
    /// at a side, the rails across the picture only (`Frame`: the frame's
    /// whole width, run across the window by nothing), and its open edges
    /// closed, a post at each from the top rail down to the bottom one. A 4:3
    /// window draws no post (the control); what else either draws is the same.
    #[test]
    fn the_merchant_stays_four_by_three_closed_at_its_edges() {
        use crate::pointer::testing::Wide;
        use crate::startmenu::posts;
        use render::ui::Anchor::{Frame, Middle};
        let vui = posts::vui();
        let mut inv = ItemList::default();
        inv.items.push(item(10, 0, 0));
        let shop = Shop { id: Some(7), sell_rate: 100, buy_rate: 100, wares: inv.clone(), greeting: wide("Hello"), pending: false };
        let base = [0; 29];
        let info = Info { words: &vui.words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let (can_use, icon) = (|_: &NetItem| true, |_: &NetItem| 1);
        let view = View { info: &info, shop: &shop, coin: 0, can_use: &can_use, inv_icon: &icon };
        let drawn = |side: i32| {
            let (mut cues, mut out) = (Vec::new(), Vec::new());
            let mut r = Wide::new(side);
            let mut m = Merchant::default();
            m.open(&vui, &mut r, &wide("Hello"));
            m.key(&vui, &mut r, &view, 134, true, &mut cues, &mut out);
            r.2.clear();
            m.draw(&vui, &mut r, &view, &mut cues, &mut out);
            r
        };
        let (tv, wide) = (drawn(0), drawn(106));
        assert!(wide.2.iter().all(|b| b.0 == Frame || b.0 == Middle), "nothing at a side");
        assert!(wide.2.contains(&(Frame, [-1, 48], [156, 265])), "the game's left panel");
        assert!(wide.2.contains(&(Frame, [-1, 37], [642, 0])), "the top rail, the picture's width");
        assert!(wide.2.contains(&(Frame, [-1, 313], [642, 0])), "the bottom rail");
        let added: Vec<_> = wide.2.iter().filter(|b| !tv.2.contains(b)).copied().collect();
        assert_eq!(added, posts::made_below(Frame, RAILS[0], false, BOTTOM_RAIL + crate::startmenu::RAIL_BAR));
        let rest: Vec<_> = wide.2.iter().filter(|b| !added.contains(b)).copied().collect();
        assert_eq!(rest, tv.2);
        assert!(rest.iter().any(|b| b.0 == Middle), "the list and its text are drawn with the frame");
    }

    /// Selling asks as SOE's final live client does (`Sell`, 0x013858A0):
    /// anything but a misc item with 1842 and its name (the table here
    /// lacks the row, so it is `livewords`'), a worn one with the disc's
    /// 930, each in the Yes/No box starting on No: No keeps it, Yes sells
    /// it. The control: a misc item (loot) sells at once, as on the disc.
    #[test]
    fn selling_asks_for_all_but_loot() {
        use crate::pointer::testing::NullRaster;
        let mut lines = vec![String::new(); 932];
        lines[929] = "Sell equipped item \"%S\"?".into();
        let words = vi::strtab::StringTable::parse(lines.join("\n").as_bytes()).unwrap();
        let vui = Vui::new(words, Default::default(), Vec::new());
        let mut r = NullRaster;
        let named = |name: &str, f: fn(&mut NetItem)| {
            let mut it = item(10, 0, 0);
            it.item.name = name.into();
            it.item.wear_slot = -1;
            f(&mut it);
            it
        };
        let inv = ItemList {
            p04: 0,
            items: vec![
                named("Bone Chips", |_| {}),
                named("Rusty Dagger", |x| x.item.wear_slot = 12),
                named("Healing Potion", |x| x.item.p4c = 2),
                named("Fine Sword", |x| x.equip_slot = 2),
            ],
        };
        assert!(crate::infotext::is_misc(&inv.items[0]) && !crate::infotext::is_misc(&inv.items[1]) && !crate::infotext::is_misc(&inv.items[2]));
        let shop = Shop { id: Some(7), sell_rate: 100, buy_rate: 100, wares: ItemList::default(), greeting: wide("Hello"), pending: false };
        let base = [0; 29];
        let info = Info { words: &vui.words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let (can_use, icon) = (|_: &NetItem| true, |_: &NetItem| 1);
        let view = View { info: &info, shop: &shop, coin: 0, can_use: &can_use, inv_icon: &icon };
        let mut cues = Vec::new();
        let mut m = Merchant::default();
        m.open(&vui, &mut r, &wide("Hello"));
        for (i, text) in [(1, "Sell item \"Rusty Dagger\"?"), (2, "Sell item \"Healing Potion\"?"), (3, "Sell equipped item \"Fine Sword\"?")] {
            let mut out = Vec::new();
            m.sell(&view, i, 1, 0, &mut out);
            assert!(out.is_empty() && m.focus == Focus::MBox, "item {i}: the box first");
            assert_eq!(String::from_utf16_lossy(&m.mbox.text), text);
            m.key(&vui, &mut r, &view, 134, true, &mut cues, &mut out);
            assert!(out.is_empty() && m.focus == Focus::List, "item {i}: No keeps it");
            m.sell(&view, i, 1, 0, &mut out);
            for k in [136, 134] {
                m.key(&vui, &mut r, &view, k, true, &mut cues, &mut out);
            }
            assert_eq!(out, [MerchantOut::Sell(i, 1)], "item {i}: Yes sells it");
        }
        let mut out = Vec::new();
        m.sell(&view, 0, 1, 0, &mut out);
        assert_eq!(out, [MerchantOut::Sell(0, 1)], "loot: at once");
        assert_eq!(m.focus, Focus::List);
    }

    /// The menu's Sell is chosen at once by a click; then the list takes
    /// clicks (a row selected, the selection sold), and while the picker is
    /// up the list behind it takes none.
    #[test]
    fn a_pointer_through_the_merchant() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        use crate::pointer::{MOVED, rows_rect};
        let (vui, mut r) = (vui(), NullRaster);
        let mut inv = ItemList::default();
        for q in [1, 1, 5] {
            let mut it = item(10, 0, 0);
            it.quantity = q;
            inv.items.push(it);
        }
        let shop = Shop { id: Some(7), sell_rate: 100, buy_rate: 100, wares: inv.clone(), greeting: wide("Hello"), pending: false };
        let base = [0; 29];
        let info = Info { words: &vui.words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        let (can_use, icon) = (|_: &NetItem| true, |_: &NetItem| 1);
        let view = View { info: &info, shop: &shop, coin: 0, can_use: &can_use, inv_icon: &icon };
        let (mut cues, mut out) = (Vec::new(), Vec::new());
        let mut m = Merchant::default();
        m.open(&vui, &mut r, &wide("Hello"));
        m.draw(&vui, &mut r, &view, &mut cues, &mut out);
        let w = m.menu.window.size[0];
        let sell = mid(rows_rect(&m.menu.window, &vui, &r, 40, m.menu.rows + 2, w - 60, 1));
        assert_eq!(m.point(sell), vec![key::CROSS]);
        assert_eq!(m.menu.sel, 1, "Sell");
        assert!(m.point([20.0, 20.0]).is_empty(), "off the menu");
        m.key(&vui, &mut r, &view, 134, true, &mut cues, &mut out);
        assert_eq!((m.mode, m.focus), (1, Focus::List));
        m.draw(&vui, &mut r, &view, &mut cues, &mut out);
        // the list at (12, 48), bare: rows 44 high
        let row = |i: i32| [12.0 + 57.0, 48.0 + 44.0 * i as f32 + 22.0];
        assert!(m.point(sell).is_empty(), "the menu is gone");
        assert_eq!(m.point(row(2)), vec![MOVED]);
        assert_eq!(m.list.sel, 2);
        assert_eq!(m.point(row(2)), vec![key::CROSS]);
        assert!(m.point(row(3)).is_empty(), "three items, three rows");
        // Enter on the stack: the picker, which has the focus; the list's
        // rows behind it take no pointer
        m.key(&vui, &mut r, &view, 134, true, &mut cues, &mut out);
        assert_eq!(m.focus, Focus::Quantity);
        m.draw(&vui, &mut r, &view, &mut cues, &mut out);
        assert!(m.point(row(0)).is_empty());
        assert_eq!(m.list.sel, 2);
    }
}
