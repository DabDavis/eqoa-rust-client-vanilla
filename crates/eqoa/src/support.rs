//! The blacksmith and the customer-service windows in play: `VIClient`'s
//! halves of them (0x0061 / 0x0063 and the repair 0x0062; the CS client on
//! 0x0AA1 / 0x07B0) and `VIWndGame`'s modes: 19 the blacksmith, 31 the CS
//! request form (Support's first entry), 32 the CS inbox (its second, with the
//! reader and the reader's reply form). The windows are `blacksmith` and
//! `cs`; this ties them to the session. A child of `play`, for `Play`'s
//! fields.

use super::Play;
use crate::blacksmith::{Blacksmith, SmithOut};

#[derive(Default)]
pub struct SupportWindows {
    smith: Blacksmith,
    /// `m_p22B78`: the smith's id from 0x0061; -1 after 0x0063 out
    smith_id: u32,
    /// the mode opened, set up with the next HUD frame
    opening: Option<i32>,
    keys: Vec<(i32, bool)>,
}

/// The modes these windows own.
pub fn owns(mode: i32) -> bool {
    matches!(mode, 19 | 31 | 32)
}

impl Play {
    /// A key for the blacksmith or the CS windows, handled with the next HUD
    /// frame.
    pub fn support_key(&mut self, key: i32, down: bool) {
        self.support.keys.push((key, down));
    }

    /// The mode is one of these windows'.
    pub fn support_owns(&self) -> bool {
        owns(self.mode)
    }

    /// The opcodes these windows read; true when handled.
    pub(super) fn support_opcode(&mut self, op: u16, data: &[u8]) -> bool {
        let mut r = net::stream::Reader::new(data);
        match op {
            0x0061 => {
                // Msg0061: only from play (IsLootable); the smith kept, sound
                // 15, mode 19
                let id = r.u32();
                if self.mode == 0 {
                    self.support.smith_id = id;
                    self.cues.push(crate::sound::Cue::Ui(15));
                    self.mode = 19;
                    self.support.opening = Some(19);
                }
            }
            // Msg0063: the window put away, the chat box live again
            0x0063 => self.smith_closed(),
            _ => return false,
        }
        true
    }

    /// `VIWndGame::Msg0063`.
    fn smith_closed(&mut self) {
        self.mode = 0;
        if let Some(c) = self.wnd_chat.as_mut() {
            c.mode = 0;
        }
        self.support.smith.visible = false;
    }

    /// The windows' opening, keys, drawing and what they ask.
    pub(super) fn run_support(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        let now = vui.time;
        if let Some(mode) = self.support.opening.take()
            && mode == 19
        {
            // UpdateMode 19: the chat box fixed, the blacksmith shown and
            // opened, the focus
            if let Some(c) = self.wnd_chat.as_mut() {
                c.mode = 1;
            }
            self.support.smith.open(vui, r);
        }
        let flags = net::session::EXPANSION_FLAGS;
        let attack_words = self.start.as_ref().map(|s| s.attack_words.clone()).unwrap_or_default();
        let pl = &self.dump.player;
        let (class, level) = (pl.class, pl.level);
        let mut cues = Vec::new();
        let mut smith_out = Vec::new();
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
            let view = crate::blacksmith::View { info: &info, coin: self.dump.player.p34 as i32, inv_icon: &icon };
            for (k, down) in std::mem::take(&mut self.support.keys) {
                if self.mode == 19 {
                    self.support.smith.key(&view, k, down, &mut cues, &mut smith_out);
                }
            }
            if self.mode == 19 {
                self.support.smith.draw(vui, r, &view, now, &mut cues);
            }
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for o in smith_out {
            match o {
                SmithOut::Close => {
                    // Send0063, then Msg0063
                    self.support.smith_id = u32::MAX;
                    self.session.send_opcode(0x0063);
                    self.smith_closed();
                }
                SmithOut::Repair(i) => {
                    // Send0062: SendOpIntInt(98, the item's slot id, the smith)
                    if let Some(it) = self.dump.inventory.items.get(i.max(0) as usize) {
                        let mut w = net::stream::Writer::op(0x0062);
                        w.u32(it.inv_index).u32(self.support.smith_id);
                        self.session.send(&w.data);
                    }
                }
            }
        }
    }
}
