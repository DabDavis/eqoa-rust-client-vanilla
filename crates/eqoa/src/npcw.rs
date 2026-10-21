//! The corpse, merchant and trade windows in play: `VIClient`'s halves of
//! them (the opcodes, the requests) and `VIWndGame`'s (the modes: 1 the
//! corpse, 10 the merchant, 15 trading, 16 asked to trade). The windows are
//! `loot`, `merchant` and `trade`; this is what ties them to the session.
//! A child of `play`, for `Play`'s fields.

use super::Play;
use crate::loot::{Loot, LootOut};
use crate::merchant::{Merchant, MerchantOut, Shop};
use crate::trade::{Trade, TradeMsg, TradeOut, TradeState};
use net::dump::ItemList;

/// The three windows and what they read.
#[derive(Default)]
pub struct NpcWindows {
    /// `HaveCorpse` and `m_corpse`
    pub corpse: Option<ItemList>,
    loot: Loot,
    loot_open: bool,
    /// `m_merchant` and its fields (0x00B7)
    pub shop: Shop,
    merchant: Merchant,
    merchant_open: bool,
    /// `VIClient`'s trade fields
    pub trade_state: TradeState,
    trade: Trade,
    trade_start: bool,
    trade_ask: bool,
    keys: Vec<(i32, bool)>,
}

/// The modes these windows own.
pub fn owns(mode: i32) -> bool {
    matches!(mode, 1 | 10 | 15 | 16)
}

impl Play {
    /// A key for the corpse, merchant or trade window, handled with the
    /// next HUD frame.
    pub fn npcw_key(&mut self, key: i32, down: bool) {
        self.npcw.keys.push((key, down));
    }

    /// A pointer at frame point `f` on the corpse, merchant or trade window
    /// (`pointer`), as its keys go: what is under it selected, and the keys
    /// to press.
    pub fn npcw_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        match self.mode {
            1 => self.npcw.loot.point(f),
            10 => self.npcw.merchant.point(f),
            15 | 16 => self.npcw.trade.point(f),
            _ => Vec::new(),
        }
    }

    /// The mode is one of these windows'.
    pub fn npcw_owns(&self) -> bool {
        owns(self.mode)
    }

    /// The trade's chat line is up (it takes the keyboard).
    pub fn npcw_typing(&self) -> bool {
        matches!(self.mode, 15 | 16) && self.npcw.trade.typing()
    }

    fn coin(&self) -> i32 {
        self.dump.player.p34 as i32
    }

    /// `VIWndGame::SetChatMode` (288): the chat box fixed (1) or live (0).
    fn chat_mode(&mut self, m: i32) {
        if let Some(c) = self.wnd_chat.as_mut() {
            c.mode = m;
        }
    }

    /// The opcodes these windows read; true when handled.
    pub(super) fn npcw_opcode(&mut self, op: u16, data: &[u8]) -> bool {
        let mut r = net::stream::Reader::new(data);
        if matches!(op, 0x0016 | 0x0018 | 0x001A | 0x00B7 | 0x00D7 | 0x0056 | 0x0057) {
            eprintln!("play: {op:#06x} ({} bytes) in mode {}", data.len(), self.mode);
        }
        match op {
            0x0018 => {
                // the corpse: the window when in play, else LootNoWindow
                self.npcw.corpse = Some(net::dump::read_item_list(&mut r));
                if self.mode == 0 {
                    self.npcw.loot_open = true;
                    self.mode = 1;
                } else {
                    self.session.send_opcode(0x0016);
                }
            }
            0x0016 => {
                // LootClose, and the corpse forgotten
                if self.mode == 1 {
                    self.mode = 0;
                }
                self.npcw.loot.visible = false;
                self.npcw.corpse = None;
            }
            0x001A => {
                // Msg001A: every corpse item into the inventory, as far as
                // it goes
                if let Some(c) = self.npcw.corpse.as_mut() {
                    let mut i = 0;
                    while i < c.items.len() {
                        let mut item = c.items[i].clone();
                        item.equip_slot = -1;
                        let limit = item.item.max_stack;
                        let n = crate::inventory::add_item(&mut self.dump.inventory, &item, limit, crate::inventory::CAPACITY);
                        crate::inventory::took(c, i, n);
                        if n != item.quantity {
                            i += 1;
                        }
                    }
                }
            }
            0x0052 => self.dump.player.p34 = r.signed(),
            0x00B7 => {
                // MerchantOpen: only from play
                self.npcw.shop = crate::merchant::read_shop(data);
                if self.mode == 0 {
                    self.cues.push(crate::sound::Cue::Ui(15));
                    self.npcw.merchant_open = true;
                    self.mode = 10;
                    self.npcw.shop.pending = false;
                }
            }
            0x00D7 => {
                // Msg00D7: the pending flags cleared
                self.npcw.shop.pending = false;
                self.waiting = false;
            }
            0x0057 => {
                let partner = r.u32();
                let id = r.u32();
                self.trade_offered(partner, id);
            }
            0x0056 => {
                let coin = self.coin();
                if let Some(m) = self.npcw.trade_state.message(data, coin) {
                    self.trade_message(m);
                }
            }
            _ => return false,
        }
        true
    }

    /// 0x003B with the merchant open: sound 3, and the request answered.
    pub(super) fn npcw_item_added(&mut self) {
        if self.npcw.shop.id.is_some() {
            self.cues.push(crate::sound::Cue::Ui(3));
            self.npcw.shop.pending = false;
        }
    }

    /// 0x003C (`Msg003C`): with the merchant open and a sound, sound 3 and
    /// the sale answered; else the destroy.
    pub(super) fn npcw_item_took(&mut self, sound: u8) {
        if self.npcw.shop.id.is_some() && sound != 0 {
            self.cues.push(crate::sound::Cue::Ui(3));
            self.npcw.shop.pending = false;
        } else {
            self.waiting = false;
        }
    }

    /// The MemoryDump's corpse: kept, and `LootNoWindow`.
    pub(super) fn npcw_world_entry(&mut self) {
        self.npcw.corpse = self.dump.corpse.clone();
        if self.npcw.corpse.is_some() {
            self.session.send_opcode(0x0016);
        }
    }

    /// Msg00B2's tail: a trade cancelled, a corpse let go, the merchant
    /// closed.
    pub(super) fn npcw_respawn(&mut self) {
        self.npcw.shop.pending = false;
        if self.npcw.trade_state.with != u32::MAX {
            self.trade_cancel();
        }
        if self.npcw.corpse.is_some() {
            self.session.send_opcode(0x0016);
        }
        if self.npcw.shop.id.is_some() {
            self.merchant_close();
        }
    }

    /// `MerchantClose`.
    fn merchant_close(&mut self) {
        if self.mode == 10 {
            self.mode = 0;
        }
        self.npcw.merchant.visible = false;
        self.npcw.shop.id = None;
    }

    /// `RequestTrade` (0x0B060000): the target must be a player and not the
    /// player; any trade going is cancelled; word 837 in purple, and 0x0057
    /// to the server.
    pub fn request_trade(&mut self) {
        let Some(e) = self.entities.slots.iter().flatten().find(|e| e.id == self.target) else { return };
        let Some(c) = &e.creature else { return };
        if c.p256 != 2 || Some(self.target) == self.self_id32() {
            return;
        }
        let name = c.name.clone();
        if self.npcw.trade_state.with != u32::MAX {
            self.trade_cancel();
        }
        let fmt = self.words.as_ref().map_or_else(|| "%s".to_string(), |w| w.get(837, 0).to_string());
        self.add_chat(fmt.replacen("%s", &name, 1).replacen("%S", &name, 1), [0.5, 0.2, 0.78, 1.0]);
        self.npcw.trade_state.asked = true;
        self.npcw.trade_state.with = self.target;
        self.session.send_op_int(0x0057, self.target);
    }

    /// `TradeName`: the partner's name, or "Unknown".
    fn trade_name(&self) -> String {
        let with = self.npcw.trade_state.with;
        self.entities.slots.iter().flatten().find(|e| e.id == with && with != u32::MAX).and_then(|e| e.creature.as_ref()).map_or_else(|| "Unknown".to_string(), |c| c.name.clone())
    }

    /// `Msg0057`: someone asks to trade. In play, the ask (mode 16); else
    /// word 626 with the name, "busy" (sub 20) to them, and forgotten.
    fn trade_offered(&mut self, partner: u32, id: u32) {
        let st = &mut self.npcw.trade_state;
        st.partner = partner;
        st.with = id;
        if self.mode == 0 {
            st.asked = false;
            self.mode = 16;
            self.npcw.trade_ask = true;
        } else {
            let name = self.entities.slots.iter().flatten().find(|e| e.id == id).and_then(|e| e.creature.as_ref()).map_or_else(|| "Someone".to_string(), |c| c.name.clone());
            let fmt = self.words.as_ref().map_or_else(|| "%s".to_string(), |w| w.get(626, 0).to_string());
            self.add_chat(fmt.replacen("%s", &name, 1).replacen("%S", &name, 1), [1.0; 4]);
            let msg = self.npcw.trade_state.send_sub(20);
            self.session.send(&msg);
            self.npcw.trade_state.reset();
        }
    }

    /// `Trade16`: the window put away (from mode 15 or 16), the chat box
    /// live again; the trade forgotten.
    fn trade_done(&mut self) {
        if self.mode == 15 || self.mode == 16 {
            self.npcw.trade.hide();
            self.chat_mode(0);
            self.mode = 0;
        }
        self.npcw.trade_state.reset();
    }

    /// `TradeCancel`: sub 1 to the partner, then `Trade16`.
    fn trade_cancel(&mut self) {
        let msg = self.npcw.trade_state.send_sub(1);
        self.session.send(&msg);
        self.trade_done();
    }

    /// `TradeSend2`/`6`/`7`: a sub-opcode to the partner, or `Trade16` when
    /// there is no trade.
    fn trade_send(&mut self, sub: u8) {
        if self.npcw.trade_state.with == u32::MAX {
            self.trade_done();
        } else {
            let msg = self.npcw.trade_state.send_sub(sub);
            self.session.send(&msg);
        }
    }

    /// The 0x0056 cases the dispatcher handles itself.
    fn trade_message(&mut self, m: TradeMsg) {
        match m {
            TradeMsg::Accepted(id) => {
                // Trade02: the partner's id, the setup, mode 15 (Start)
                let coin = self.coin();
                let st = &mut self.npcw.trade_state;
                st.partner = id;
                st.setup(&self.dump.inventory, coin);
                self.mode = 15;
                self.chat_mode(1);
                self.npcw.trade_start = true;
            }
            TradeMsg::Cancelled | TradeMsg::Done => self.trade_done(),
            TradeMsg::Busy => {
                // Trade20
                let st = &self.npcw.trade_state;
                if st.asked && st.with != u32::MAX {
                    let name = self.trade_name();
                    let fmt = self.words.as_ref().map_or_else(|| "%s".to_string(), |w| w.get(651, 0).to_string());
                    self.add_chat(fmt.replacen("%s", &name, 1).replacen("%S", &name, 1), [1.0; 4]);
                }
                self.npcw.trade_state.reset();
            }
            TradeMsg::Refused(word) => {
                // Trade17/21/22
                self.warn(word);
                self.npcw.trade.clear_accepted();
            }
            TradeMsg::Unaccepted => self.npcw.trade.clear_accepted(),
            TradeMsg::TheyAccepted => self.npcw.trade.they_accepted = true,
            TradeMsg::Changed => {}
        }
    }

    /// The three windows' opens, keys, drawing and what they ask.
    pub(super) fn run_npcw(&mut self, vui: &vi::ui::Vui, r: &mut dyn crate::ui::Layout) {
        let now = vui.time;
        let flags = net::session::EXPANSION_FLAGS;
        if std::mem::take(&mut self.npcw.loot_open) {
            self.npcw.loot.open(vui, r, now);
        }
        if std::mem::take(&mut self.npcw.merchant_open) {
            // VIWndMercMenu::Refresh: the greeting expanded
            let ctx = vi::ui::ExpandContext { player_name: self.dump.player.name.clone(), ..Default::default() };
            let greeting = vi::ui::expand(&self.npcw.shop.greeting, 256, true, &ctx);
            self.npcw.merchant.open(vui, r, &greeting);
        }
        if std::mem::take(&mut self.npcw.trade_ask) {
            let name = self.trade_name();
            self.npcw.trade.ask(&name, &vui.words, now);
        }
        if std::mem::take(&mut self.npcw.trade_start) {
            self.npcw.trade.start(vui, r, now);
        }
        let attack_words = self.start.as_ref().map(|s| s.attack_words.clone()).unwrap_or_default();
        let pl = &self.dump.player;
        let (class, race, level) = (pl.class, pl.race, pl.level);
        let mut cues = Vec::new();
        let mut loot_out = Vec::new();
        let mut merch_out = Vec::new();
        let mut trade_out = Vec::new();
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
            let icon = |it: &net::dump::NetItem| crate::inventory::item_icon(it, flags) as i32;
            let can_use = |it: &net::dump::NetItem| crate::inventory::can_use(it, class, race, level);
            let w = &mut self.npcw;
            let empty = ItemList::default();
            for (k, down) in std::mem::take(&mut w.keys) {
                match self.mode {
                    1 => loot_out.extend(w.loot.key(k, down, w.corpse.as_ref().unwrap_or(&empty), &mut cues)),
                    10 => {
                        let view = crate::merchant::View { info: &info, shop: &w.shop, coin: self.dump.player.p34 as i32, can_use: &can_use, inv_icon: &icon };
                        w.merchant.key(vui, r, &view, k, down, &mut cues, &mut merch_out);
                    }
                    15 | 16 => w.trade.key(vui, r, &w.trade_state, k, down, &mut cues, &mut trade_out),
                    _ => {}
                }
            }
            match self.mode {
                1 => {
                    let corpse = w.corpse.as_ref().unwrap_or(&empty);
                    let inv = &self.dump.inventory;
                    w.loot.draw(vui, r, &info, corpse, &|k| icon(&inv.items[k]), &|k| icon(&corpse.items[k]), now, &mut cues);
                }
                10 => {
                    let view = crate::merchant::View { info: &info, shop: &w.shop, coin: self.dump.player.p34 as i32, can_use: &can_use, inv_icon: &icon };
                    w.merchant.draw(vui, r, &view, &mut cues, &mut merch_out);
                }
                15 | 16 => w.trade.draw(vui, r, &w.trade_state, &info, &icon, &can_use, &mut cues),
                _ => {}
            }
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for o in loot_out {
            match o {
                LootOut::Close => self.session.send_opcode(0x0016),
                LootOut::One(key, n) => {
                    // SendLoot(25, key, n)
                    let mut w = net::stream::Writer::op(0x0019);
                    w.u32(key as u32).u32(n as u32);
                    self.session.send(&w.data);
                }
                LootOut::All => self.session.send_opcode(0x001A),
            }
        }
        for o in merch_out {
            match o {
                MerchantOut::Close => self.merchant_close(),
                MerchantOut::ChatMode(m) => self.chat_mode(m),
                MerchantOut::Buy(i, n) => {
                    // MerchantRequest: SendOpSigned2Int(74, i, n, id)
                    let mut w = net::stream::Writer::op(0x004A);
                    w.signed(i as i64).signed(n as i64).u32(self.npcw.shop.id.unwrap_or(u32::MAX));
                    self.session.send(&w.data);
                    self.npcw.shop.pending = true;
                }
                MerchantOut::Sell(i, n) => {
                    // SellItem: SendOpIntSignedInt(75, key, n, id), while it holds that many
                    if let Some(it) = self.dump.inventory.items.get(i.max(0) as usize).filter(|it| i >= 0 && it.quantity >= n as i64) {
                        let mut w = net::stream::Writer::op(0x004B);
                        w.u32(it.inv_index).signed(n as i64).u32(self.npcw.shop.id.unwrap_or(u32::MAX));
                        self.session.send(&w.data);
                        self.npcw.shop.pending = true;
                    }
                }
                MerchantOut::Word(id) => self.show_word(id),
            }
        }
        for o in trade_out {
            match o {
                TradeOut::Cancel => self.trade_cancel(),
                TradeOut::Agree => {
                    // 268 from the ask: TradeSetup, TradeSend2, Start, chat 1
                    let coin = self.coin();
                    self.npcw.trade_state.setup(&self.dump.inventory, coin);
                    self.trade_send(2);
                    if self.mode == 16 {
                        self.npcw.trade.start(vui, r, now);
                        self.chat_mode(1);
                    }
                }
                TradeOut::Add(i, n) => match self.npcw.trade_state.send_add(i, n) {
                    Some(m) if self.npcw.trade_state.with != u32::MAX => self.session.send(&m),
                    _ => self.trade_done(),
                },
                TradeOut::Remove(i) => {
                    if self.npcw.trade_state.with == u32::MAX {
                        self.trade_done();
                    } else if let Some(m) = self.npcw.trade_state.send_remove(i) {
                        self.session.send(&m);
                    }
                }
                TradeOut::Money(x) => {
                    if self.npcw.trade_state.with == u32::MAX {
                        self.trade_done();
                    } else {
                        let m = self.npcw.trade_state.send_money(x);
                        self.session.send(&m);
                    }
                }
                TradeOut::Accept => self.trade_send(6),
                TradeOut::Unaccept => self.trade_send(7),
                TradeOut::Chat(text) => {
                    if !text.is_empty() {
                        self.chat_line(&text);
                    }
                }
            }
        }
    }
}
