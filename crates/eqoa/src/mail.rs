//! Mail and the tell forms in play: the gram client (`VIClient+0x2EFD0`,
//! answering 0x0D41) and `VIWndGame`'s modes for its windows: 23 the
//! compose window (Mail > Compose), 24 the inbox (Mail > Inbox, with its
//! reader and the reader's reply), 28 the tell form and 29 the reply form;
//! plus the buddy list's own compose window, which its menu's Mail opens over
//! the list (mode 18). The windows are `gram`'s; this ties them to the
//! session. A child of `play`, for `Play`'s fields.

use super::Play;
use crate::gram::{Compose, ComposeUp, Grams, Inbox, InboxUp, Tell, TellUp};

pub struct MailWindows {
    pub grams: Grams,
    tell: Tell,
    reply: Tell,
    compose: Compose,
    inbox: Inbox,
    /// `VIWndBuddy::m_compose`: a reply-style compose over the buddy list
    buddy: Compose,
    /// the mode opened, set up with the next HUD frame
    opening: Option<i32>,
    /// the buddy compose's To, set up with the next HUD frame
    buddy_open: Option<String>,
    keys: Vec<(i32, bool)>,
    /// `EQOA_MAIL_TEXT`: the playshot's fields, typed one a field as each
    /// editor opens: (field, character)
    script: (usize, usize, bool),
}

impl Default for MailWindows {
    fn default() -> Self {
        MailWindows { grams: Grams::default(), tell: Tell::new(false), reply: Tell::new(true), compose: Compose::new(false), inbox: Inbox::default(), buddy: Compose::new(true), opening: None, buddy_open: None, keys: Vec::new(), script: (0, 0, false) }
    }
}

impl Play {
    /// A key for the mail and tell windows, handled with the next HUD frame.
    pub fn mail_key(&mut self, key: i32, down: bool) {
        self.mail.keys.push((key, down));
    }

    /// A pointer at frame point `f` on the mail and tell windows
    /// (`pointer`), to the window the keys go to: the compose window, the
    /// inbox (and its reader, its options, the reply), the tell and reply
    /// forms, the buddy list's compose. What is under it selected, and the
    /// keys to press.
    pub fn mail_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        let m = &mut self.mail;
        match self.mode {
            23 if m.compose.visible => m.compose.point(f),
            24 => m.inbox.point(f),
            28 => m.tell.point(f),
            29 => m.reply.point(f),
            18 if m.buddy.visible => m.buddy.point(f),
            _ => Vec::new(),
        }
    }

    /// The mode is one of these windows', or the buddy list's compose is up.
    pub fn mail_owns(&self) -> bool {
        matches!(self.mode, 23 | 24 | 28 | 29) || (self.mode == 18 && (self.mail.buddy.visible || self.mail.buddy_open.is_some()))
    }

    /// An editor in them is up (it takes the keyboard).
    pub fn mail_typing(&self) -> bool {
        let m = &self.mail;
        match self.mode {
            23 => m.compose.typing(),
            24 => m.inbox.typing(),
            28 => m.tell.typing(),
            29 => m.reply.typing(),
            18 => m.buddy.visible && m.buddy.typing(),
            _ => false,
        }
    }

    /// `OpenComposeGram` (23), `OpenGramInbox` (24), `OpenTell` (28),
    /// `OpenReply` (29): the mode; the window with the next HUD frame.
    pub(super) fn open_mail(&mut self, mode: i32) {
        self.mode = mode;
        self.mail.opening = Some(mode);
        self.mail.keys.clear();
        self.mail.script = (0, 0, false);
    }

    /// The buddy list's Mail: its compose, addressed to the buddy.
    pub(super) fn buddy_mail(&mut self, name: String) {
        self.mail.buddy_open = Some(name);
        self.mail.keys.clear();
    }

    /// `Recv0D41`.
    pub(super) fn mail_recv(&mut self, data: &[u8]) {
        self.mail.grams.recv(data);
    }

    /// go-eqoa-server's offer of the live client's mail forms
    /// (`gram::OP_LIVE_MAIL`, a u32): taken and answered 1, the forms the
    /// live ones from then on; 0 back to the disc's. One cut short is not
    /// taken.
    pub(super) fn mail_offer(&mut self, data: &[u8]) {
        let mut r = net::stream::Reader::new(data);
        let v = r.u32();
        if r.error {
            return;
        }
        self.mail.grams.live = v != 0;
        if v != 0 {
            self.session.send_op_int(crate::gram::OP_LIVE_MAIL, 1);
        }
        eprintln!("mail: the live client's forms {} (0x7E0A {v})", if v != 0 { "on" } else { "off" });
    }

    /// The pump's 0xFF: a new connection (`OnConnect`).
    pub(super) fn mail_connect(&mut self) {
        self.mail.grams.on_connect();
    }

    /// `GetTargetPlayerName`: the target's name when it is a player.
    fn target_player_name(&self) -> String {
        self.entities.slots.iter().flatten().find(|e| e.id == self.target).and_then(|e| e.creature.as_ref()).filter(|c| c.p256 == 2).map(|c| c.name.clone()).unwrap_or_default()
    }

    /// `EQOA_MAIL_TEXT=a|b|c`: each field typed into the next editor that
    /// opens, a character a frame, then done (143).
    pub fn mail_script_step(&mut self) {
        let Ok(spec) = std::env::var("EQOA_MAIL_TEXT") else { return };
        let fields: Vec<&str> = spec.split('|').collect();
        let typing = self.mail_typing();
        let (f, c, was) = self.mail.script;
        if !typing {
            if was {
                self.mail.script = (f + 1, 0, false);
            }
            return;
        }
        let Some(text) = fields.get(f) else { return };
        match text.chars().nth(c) {
            Some(ch) => self.mail_key(ch as i32, true),
            None if c == text.chars().count() => self.mail_key(143, true),
            None => {}
        }
        self.mail.script = (f, c + 1, true);
    }

    /// The windows' opening, keys, drawing and what they ask.
    pub(super) fn run_mail(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        let now = vui.time;
        if let Some(mode) = self.mail.opening.take() {
            match mode {
                23 => {
                    // UpdateMode: Refresh, then the focus
                    self.mail.compose.refresh();
                    self.mail.compose.visible = true;
                    self.mail.compose.focus(vui, r);
                }
                24 => {
                    // Reply All's own name (`GetPlayer()+4`)
                    self.mail.inbox.read.me = self.dump.player.name.as_bytes().to_vec();
                    let m = &mut self.mail;
                    m.inbox.open(vui, r, &mut m.grams);
                }
                28 => {
                    let name = crate::ui::wide(&self.target_player_name());
                    self.mail.tell.open(vui, r, &name);
                }
                29 => {
                    let name = crate::ui::wide(&self.reply_name);
                    self.mail.reply.open(vui, r, &name);
                }
                _ => {}
            }
        }
        if let Some(name) = self.mail.buddy_open.take() {
            // VIWndBuddy 261, Mail: shown, the focus (295 empties it), Refresh,
            // then To
            let b = &mut self.mail.buddy;
            b.visible = true;
            b.focus(vui, r);
            b.refresh();
            b.set_to(&crate::ui::wide(&name));
        }
        let mut cues = Vec::new();
        let mut tell_ups = Vec::new();
        let mut compose_ups = Vec::new();
        let mut buddy_ups = Vec::new();
        let mut inbox_ups = Vec::new();
        for (k, down) in std::mem::take(&mut self.mail.keys) {
            let m = &mut self.mail;
            match self.mode {
                23 => compose_ups.extend(m.compose.key(vui, r, &mut m.grams, k, down, now, &mut cues)),
                24 => inbox_ups.extend(m.inbox.key(vui, r, &mut m.grams, k, down, now, &mut cues)),
                28 => tell_ups.extend(m.tell.key(vui, r, k, down, now, &mut cues)),
                29 => tell_ups.extend(m.reply.key(vui, r, k, down, now, &mut cues)),
                18 if m.buddy.visible => buddy_ups.extend(m.buddy.key(vui, r, &mut m.grams, k, down, now, &mut cues)),
                _ => {}
            }
        }
        {
            let m = &mut self.mail;
            match self.mode {
                23 if m.compose.visible => compose_ups.extend(m.compose.draw(vui, r, &m.grams, now, &mut cues)),
                24 => m.inbox.draw(vui, r, &m.grams, true, now, &mut cues),
                28 => m.tell.draw(vui, r, now, &mut cues),
                29 => m.reply.draw(vui, r, now, &mut cues),
                18 if m.buddy.visible => buddy_ups.extend(m.buddy.draw(vui, r, &m.grams, now, &mut cues)),
                _ => {}
            }
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for up in tell_ups {
            match up {
                // CloseTell / CloseReply: back to play
                TellUp::Close => self.mode = 0,
                TellUp::Send(line) => {
                    self.chat_line(&line);
                    self.mode = 0;
                }
            }
        }
        for up in compose_ups {
            match up {
                // ClosePanel6E0: CloseComposeGram, back to the start menu
                ComposeUp::Close | ComposeUp::Sent => {
                    self.mail.compose.visible = false;
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                ComposeUp::Warn(id) => self.warn(id),
            }
        }
        for up in buddy_ups {
            match up {
                // VIWndBuddy 260 / 309: the compose put away, the list again
                ComposeUp::Close | ComposeUp::Sent => self.mail.buddy.visible = false,
                ComposeUp::Warn(id) => self.warn(id),
            }
        }
        for up in inbox_ups {
            match up {
                // ClosePanel6E4: CloseGramInbox, back to the start menu
                InboxUp::Close => {
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                InboxUp::Warn(id) => self.warn(id),
            }
        }
        for msg in std::mem::take(&mut self.mail.grams.out) {
            self.session.send(&msg);
        }
        for id in std::mem::take(&mut self.mail.grams.shown) {
            self.show_word(id);
        }
        // what the compose windows said (the live answers' lines)
        let m = &mut self.mail;
        let said: Vec<(String, [f32; 4])> = [&mut m.compose, &mut m.buddy, &mut m.inbox.read.compose].into_iter().flat_map(|c| std::mem::take(&mut c.said)).collect();
        for (line, color) in said {
            self.add_chat(line, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::play::Play;
    use net::session::{Player, Session};

    fn play() -> Play {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let mut s = Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        s.record = Some(Vec::new());
        Play::new(s, Player::default(), Default::default())
    }

    /// What the session sent since the last look.
    fn sent(p: &mut Play) -> Vec<Vec<u8>> {
        p.session.record.replace(Vec::new()).unwrap_or_default().into_iter().map(|(_, m)| m).collect()
    }

    /// go-eqoa-server's offer of the live mail forms (0x7E0A): one cut
    /// short is not taken; 1 is taken and answered 1, and the gram client
    /// speaks the live forms; 0 goes back to the disc's, unanswered. The
    /// control: before any offer the forms are the disc's.
    #[test]
    fn the_live_mail_offer() {
        let mut p = play();
        p.mail.grams.request_message(5);
        assert_eq!(p.mail.grams.out.pop().unwrap()[3..6], [0x06, 0x41, 0x0D], "no offer: the disc's envelope");
        p.mail_offer(&[1, 0, 0]);
        assert!(!p.mail.grams.live && sent(&mut p).is_empty(), "cut short");
        p.mail_offer(&[1, 0, 0, 0]);
        assert!(p.mail.grams.live);
        assert_eq!(sent(&mut p), [vec![0x0A, 0x7E, 1, 0, 0, 0]]);
        p.mail.grams.request_message(5);
        assert_eq!(p.mail.grams.out.pop().unwrap(), [0x01, 0x0D, 3, 10], "the live open");
        p.mail_offer(&[0, 0, 0, 0]);
        assert!(!p.mail.grams.live && sent(&mut p).is_empty(), "0: the disc's, unanswered");
    }
}
