//! The blacksmith and the customer-service windows in play: `VIClient`'s
//! halves of them (0x0061 / 0x0063 and the repair 0x0062; the CS client on
//! 0x0AA1 / 0x07B0) and `VIWndGame`'s modes: 19 the blacksmith, 31 the CS
//! request form (Support's first entry), 32 the CS inbox (its second, with the
//! reader and the reader's reply form). The windows are `blacksmith` and
//! `cs`; this ties them to the session. A child of `play`, for `Play`'s
//! fields.

use super::Play;
use crate::blacksmith::{Blacksmith, SmithOut};
use crate::cs::{CsClient, Inbox, InboxUp, Request, RequestUp};

pub struct SupportWindows {
    /// `VIClient::m_p33950`
    pub cs: CsClient,
    request: Request,
    inbox: Inbox,
    /// `EQOA_CS_TEXT`: the playshot's text, typed into the next editor that
    /// opens: (character, typing seen)
    script: (usize, bool),
    smith: Blacksmith,
    /// `m_p22B78`: the smith's id from 0x0061; -1 after 0x0063 out
    smith_id: u32,
    /// the mode opened, set up with the next HUD frame
    opening: Option<i32>,
    keys: Vec<(i32, bool)>,
}

impl Default for SupportWindows {
    fn default() -> Self {
        SupportWindows { cs: CsClient::default(), request: Request::new(false), inbox: Inbox::default(), script: (0, false), smith: Blacksmith::default(), smith_id: 0, opening: None, keys: Vec::new() }
    }
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

    /// An editor in the CS windows is up (it takes the keyboard).
    pub fn support_typing(&self) -> bool {
        match self.mode {
            31 => self.support.request.typing(),
            32 => self.support.inbox.typing(),
            _ => false,
        }
    }

    /// Support's entries: `OpenCSRequest` (31) and `OpenCSInbox` (32), the
    /// window with the next HUD frame.
    pub(super) fn open_support(&mut self, mode: i32) {
        self.mode = mode;
        self.support.opening = Some(mode);
        self.support.keys.clear();
        self.support.script = (0, false);
    }

    /// The pump's 0xFF: a new connection (`VICSClient::OnConnect`).
    pub(super) fn support_connect(&mut self) {
        self.support.cs.on_connect();
    }

    /// `EQOA_CS_TEXT=...`: typed into the next editor that opens, a
    /// character a frame, then done (Tab, 143).
    pub fn support_script_step(&mut self) {
        let Ok(text) = std::env::var("EQOA_CS_TEXT") else { return };
        let (c, was) = self.support.script;
        if !self.support_typing() {
            if was {
                self.support.script = (usize::MAX, false);
            }
            return;
        }
        if c == usize::MAX {
            return;
        }
        match text.chars().nth(c) {
            Some(ch) => self.support_key(ch as i32, true),
            None => self.support_key(143, true),
        }
        self.support.script = (c + 1, true);
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
            // VICSClient::Recv07B0
            0x07B0 => self.support.cs.recv(data),
            _ => return false,
        }
        true
    }

    /// `CloseCSRequest` / `CloseCSInbox`: hidden, back to the start menu.
    fn close_cs(&mut self) {
        self.support.request.visible = false;
        self.mode = 0;
        if let Some(c) = self.wnd_chat.as_mut() {
            c.mode = 0;
        }
        self.open_start_menu(false);
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
        if let Some(mode) = self.support.opening.take() {
            // UpdateMode 19, 31, 32: the chat box fixed, the window shown
            // (Open / Refresh), the focus (295)
            if let Some(c) = self.wnd_chat.as_mut() {
                c.mode = 1;
            }
            let s = &mut self.support;
            match mode {
                19 => s.smith.open(vui, r),
                31 => {
                    s.request.refresh(&mut s.cs);
                    s.request.visible = true;
                    s.request.focus(vui, r);
                }
                32 => s.inbox.open(vui, r, &mut s.cs, now),
                _ => {}
            }
        }
        let pad_only = self.options.compact != 0;
        self.support.request.edit.pad_only = pad_only;
        self.support.inbox.read.request.edit.pad_only = pad_only;
        let mut req_ups = Vec::new();
        let mut inbox_ups = Vec::new();
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
            let s = &mut self.support;
            for (k, down) in std::mem::take(&mut s.keys) {
                match self.mode {
                    19 => s.smith.key(&view, k, down, &mut cues, &mut smith_out),
                    31 => req_ups.extend(s.request.key(vui, r, &mut s.cs, k, down, now, &mut cues)),
                    32 => inbox_ups.extend(s.inbox.key(vui, r, &mut s.cs, k, down, now, &mut cues)),
                    _ => {}
                }
            }
            match self.mode {
                19 => s.smith.draw(vui, r, &view, now, &mut cues),
                31 if s.request.visible => req_ups.extend(s.request.draw(vui, r, &mut s.cs, now, &mut cues)),
                32 => inbox_ups.extend(s.inbox.draw(vui, r, &mut s.cs, now, &mut cues)),
                _ => {}
            }
        }
        for msg in std::mem::take(&mut self.support.cs.out) {
            self.session.send(&msg);
        }
        for up in req_ups {
            match up {
                RequestUp::Close => self.close_cs(),
                RequestUp::Sent => {
                    self.show_word(864);
                    self.close_cs();
                }
                RequestUp::Full => {
                    self.warn(874);
                    self.close_cs();
                }
            }
        }
        for up in inbox_ups {
            match up {
                InboxUp::Close => self.close_cs(),
                InboxUp::Show(id) => self.show_word(id),
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
