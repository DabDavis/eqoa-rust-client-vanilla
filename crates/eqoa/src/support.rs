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
    /// F3's opening (`open_bug_report`): the request form set on Bug Report
    bug_preset: bool,
    keys: Vec<(i32, bool)>,
}

impl Default for SupportWindows {
    fn default() -> Self {
        SupportWindows { cs: CsClient::default(), request: Request::new(false), inbox: Inbox::default(), script: (0, false), smith: Blacksmith::default(), smith_id: 0, opening: None, bug_preset: false, keys: Vec::new() }
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

    /// A pointer at frame point `f` on the blacksmith or the CS windows
    /// (`pointer`), to the window the keys go to: the smith's list, the
    /// request form, the inbox (and its reader, its options, the reply).
    /// What is under it selected, and the keys to press.
    pub fn support_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        let s = &mut self.support;
        match self.mode {
            19 => s.smith.point(f),
            31 if s.request.visible => s.request.point(f),
            32 => s.inbox.point(f),
            _ => Vec::new(),
        }
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

    /// F3, on a server that offered bug reports (`Session::bug_reports`):
    /// Support's request form, from the world with no window up, set on the
    /// server's Bug Report (`cs::BUG_REPORT`) with its topics open. False
    /// (nothing done) elsewhere.
    pub fn open_bug_report(&mut self) -> bool {
        if !self.session.bug_reports || self.mode != 0 || self.typing.is_some() {
            return false;
        }
        self.open_support(31);
        self.support.bug_preset = true;
        true
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
                    if std::mem::take(&mut s.bug_preset) {
                        s.request.preset(crate::cs::BUG_REPORT);
                    }
                }
                32 => s.inbox.open(vui, r, &mut s.cs, now),
                _ => {}
            }
        }
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
            // a new request (0x0AA1, sub-op 0): the bug report's note first
            if msg.len() > 2 && msg[..3] == [0xA1, 0x0A, 0] {
                self.send_bug_note();
            }
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

#[cfg(test)]
mod tests {
    use super::super::liveplay::tests::play;

    fn sent(p: &mut super::Play) -> Vec<Vec<u8>> {
        p.session.record.as_mut().map(std::mem::take).unwrap_or_default().into_iter().map(|(_, d)| d).collect()
    }

    /// F3 opens Support's request form only on a server that offered bug
    /// reports, from the world with no window up and not typing; a new
    /// request goes after the client's note, a typed /bug too, nothing else.
    /// The controls: no offer, no form and no note; a window up, nothing.
    #[test]
    fn f3_and_the_note_only_where_offered() {
        let mut p = play();
        assert!(!p.open_bug_report(), "no offer");
        assert_eq!(p.mode, 0);
        p.chat_line("/bug the gate is shut");
        assert!(sent(&mut p).iter().all(|m| m[..2] != [0x0F, 0x7E]), "no offer: no note");

        p.session.bug_reports = true;
        p.mode = 22;
        assert!(!p.open_bug_report(), "a window up");
        p.mode = 0;
        p.typing = Some(String::new());
        assert!(!p.open_bug_report(), "typing");
        p.typing = None;
        assert!(p.open_bug_report());
        assert_eq!((p.mode, p.support.opening, p.support.bug_preset), (31, Some(31), true));

        // a filing (0x0AA1 sub-op 0) goes after the note
        p.support.cs.submit_request(1, 13, &"the guard walks through walls".encode_utf16().collect::<Vec<u16>>());
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        p.run_support(&vui, &mut r);
        let m = sent(&mut p);
        let i = m.iter().position(|d| d[..2] == [0x0F, 0x7E]).expect("the note");
        let j = m.iter().position(|d| d[..3] == [0xA1, 0x0A, 0]).expect("the request");
        assert!(i < j, "the note first");
        let note = String::from_utf8(m[i][10..].to_vec()).unwrap();
        assert!(note.starts_with("build: ") && note.contains("\nplace: world "), "{note}");

        p.chat_line("/bug the gate is shut");
        let m = sent(&mut p);
        assert_eq!(m.len(), 2, "{m:?}");
        assert_eq!((&m[0][..2], &m[1][..2]), (&[0x0F, 0x7E][..], &[0x0B, 0x00][..]), "the note, then the line");
        p.chat_line("hello there");
        assert!(sent(&mut p).iter().all(|d| d[..2] != [0x0F, 0x7E]), "plain chat: no note");
    }
}
