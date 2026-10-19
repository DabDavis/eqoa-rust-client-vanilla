//! The bank in play: `VIBankControl`'s half (the bank's list and tunar
//! kept from the MemoryDump, the requests 0x1254/0x1255 to the banker, the
//! replies 0x1250/0x1251/0x1253, the waiting state) and `VIWndGame`'s
//! (0x124D opens the window from play, mode 11; `CloseBank`). The window is
//! `bank`; this ties it to the session. A child of `play`, for `Play`'s
//! fields.

use super::Play;
use crate::bank::{Bank, BankOut, View};

/// `VIBankControl`'s waiting state and the window.
#[derive(Default)]
pub struct BankPlay {
    /// `m_state`: 0 idle, 1 a money move, 2 an item move, 3..6 the
    /// auction's requests
    pub state: i32,
    wnd: Bank,
    open: bool,
    keys: Vec<(i32, bool)>,
}

impl Play {
    /// The bank window has the keys (mode 11).
    pub fn bank_owns(&self) -> bool {
        self.mode == 11
    }

    /// A key for the bank, handled with the next HUD frame.
    pub fn bank_key(&mut self, key: i32, down: bool) {
        self.bank.keys.push((key, down));
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
            let auction = self.dump.bank.p78cc;
            self.bank.wnd.open(vui, r, auction);
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
            }
        }
    }
}
