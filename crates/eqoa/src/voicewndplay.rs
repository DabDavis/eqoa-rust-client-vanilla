//! The voice window in play (`voicewnd`): opened from play (the Controls
//! page's Voice Window key, `/voice` alone, the HUD's mic badge), its keys
//! and pointer while play has them (a box the server opens over it, a dialog
//! or the edit box, takes them first), its text fields through the edit box
//! (mode 2: the password's face for a password), and what it asks carried
//! out through the voice window's interface (`voiceplay`: `voice_view`,
//! `voice_set_mode`, `voice_refresh_lobby`, `voice_join`, `voice_leave`,
//! `voice_set_volume`, `voice_set_muted`). A child of `play`, for `Play`'s
//! fields.

use super::Play;
use crate::voice::VoiceView;
use crate::voicewnd::{Ask, Field};

impl Play {
    /// Voice as the window shows it: its test view's under
    /// `EQOA_VOICE_WINDOW_TEST`, else voice's.
    pub fn voice_window_view(&self) -> VoiceView {
        self.voice_wnd.test.clone().unwrap_or_else(|| self.voice_view())
    }

    /// The window put up over play (play's own modes only, and the chat
    /// line's edit box, which `/voice` closes: the start menu, the game's
    /// windows and a server's player search are left alone), the lobby
    /// asked for.
    pub fn open_voice_window(&mut self) {
        if !matches!(self.mode, 0 | 2 | 38) || self.search.open || self.voice_wnd.open {
            return;
        }
        if self.voice_wnd.test.is_none() && std::env::var_os("EQOA_VOICE_WINDOW_TEST").is_some() {
            self.voice_wnd.test = Some(crate::voicewnd::test_view());
        }
        crate::hideui::show();
        let asks = self.voice_wnd.open();
        self.cues.push(crate::sound::Cue::Ui(13));
        self.voice_asks(asks);
    }

    /// The window opened, or closed (the Controls page's key again).
    pub fn toggle_voice_window(&mut self) {
        if self.voice_wnd.open {
            self.voice_wnd.close();
            self.cues.push(crate::sound::Cue::Ui(8));
        } else {
            self.open_voice_window();
        }
    }

    /// Whether the window has the keys: up, and play's (no box, dialog or
    /// edit box over it).
    pub fn voice_window_owns(&self) -> bool {
        self.voice_wnd.open && (self.mode == 0 || self.mode == 38)
    }

    /// A key for the window (the game's code), at once.
    pub fn voice_window_key(&mut self, key: i32, down: bool) {
        if !down || !self.voice_window_owns() {
            return;
        }
        let view = self.voice_window_view();
        let asks = self.voice_wnd.key(key, &view, self.session.now() as i64);
        self.voice_asks(asks);
    }

    /// What the window asked, done: voice's calls (a password scrubbed from
    /// reports first), the edit box for a field, the window closed.
    fn voice_asks(&mut self, asks: Vec<Ask>) {
        for a in asks {
            if let Some(t) = self.voice_wnd.test.as_mut() {
                crate::voicewnd::apply_test(t, &a);
            }
            let test = self.voice_wnd.test.is_some();
            match a {
                Ask::Edit(f) => self.voice_edit(f),
                Ask::Close => self.voice_wnd.close(),
                _ if test => {}
                Ask::Mode(m) => self.voice_set_mode(m),
                Ask::Refresh => self.voice_refresh_lobby(),
                Ask::Join { name, password, unlisted } => {
                    if !password.is_empty() {
                        crate::report::secret("", &password);
                    }
                    self.voice_join(&name, &password, unlisted);
                }
                Ask::Leave => self.voice_leave(),
                Ask::Volume(peer, v) => self.voice_set_volume(peer, v),
                Ask::Muted(peer, m) => self.voice_set_muted(peer, m),
            }
        }
        let cues = self.voice_wnd.take_cues();
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
    }

    /// The edit box up for one of the window's fields: titled with it, the
    /// room's name typed so far in it, a password's typed as stars.
    fn voice_edit(&mut self, f: Field) {
        self.open_edit(f.purpose(), f.max_len(), -1, false);
        self.edit.title = crate::ui::wide(&f.title());
        self.edit.password = f.masked();
        if f == Field::Name {
            let name = self.voice_wnd.form_name();
            self.edit.set_text(&crate::ui::wide(&name));
        }
    }

    /// The edit box went down (Done with its text, or cancelled): true when
    /// it was for the window, which takes the text (its copy in the box
    /// cleared, the box's password face put back).
    pub(super) fn voice_edit_done(&mut self, purpose: u32, done: bool) -> bool {
        if !self.voice_wnd.edits(purpose) {
            return false;
        }
        let text = String::from_utf16_lossy(&std::mem::take(&mut self.edit.text));
        self.edit.password = false;
        self.edit.title.clear();
        self.mode = 0;
        let asks = self.voice_wnd.edited(done.then_some(text));
        self.voice_asks(asks);
        true
    }

    /// The window drawn (the HUD's frame, in the middle), while it is up:
    /// the player and the camera held still, as a server's player search
    /// holds them.
    pub(super) fn draw_voice_window(&mut self, vui: &vi::ui::Vui, r: &mut crate::ui::UiRaster) {
        if !self.voice_wnd.open {
            return;
        }
        if self.voice_window_owns() {
            self.locks = (false, false);
        }
        let view = self.voice_window_view();
        self.voice_wnd.draw(vui, r, &view);
    }

    /// The HUD's mic badge this frame: voice on, where the HUD's right side
    /// is up.
    pub(super) fn voice_badge(&self) -> Option<crate::voicewnd::Badge> {
        if !self.right_hud_up() {
            return None;
        }
        crate::voicewnd::badge(&self.voice_window_view())
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;

    /// Play on a session nothing answers (as `commands`' tests make it), its
    /// voice window on a test view.
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
        let mut p = Play::new(s, Player::default(), Box::default());
        p.voice_wnd.test = Some(crate::voicewnd::test_view());
        p
    }

    /// `/voice` alone opens the window; `/voice rooms` is the command's
    /// (the control); the window has the keys only while play would, a box
    /// over it taking them.
    #[test]
    fn voice_alone_opens_the_window() {
        let mut p = play();
        p.chat_line("/voice rooms");
        assert!(!p.voice_wnd.open, "the control: a command is the command's");
        p.chat_line("/voice");
        assert!(p.voice_wnd.open && p.voice_window_owns());
        p.mode = 14;
        assert!(p.voice_wnd.open && !p.voice_window_owns(), "a box over it has the keys");
        p.mode = 0;
        p.voice_window_key(crate::pointer::key::TRIANGLE, true);
        assert!(!p.voice_wnd.open, "△ closes it");
        p.mode = 13;
        p.open_voice_window();
        assert!(!p.voice_wnd.open, "not over the start menu");
    }

    /// A field typed through the edit box: its title, the name's text in
    /// it, a password's stars; Done fills the form and clears the box's
    /// copy (the password face put back for the next line); a cancel keeps
    /// what was there. The control: the chat line's edit box is no field's.
    #[test]
    fn the_fields_go_through_the_edit_box() {
        use crate::pointer::key;
        let mut p = play();
        p.open_voice_window();
        p.voice_wnd.select_field_for_test(crate::voicewnd::Field::Name);
        p.voice_window_key(key::CROSS, true);
        assert_eq!(p.mode, 2, "the edit box up");
        assert_eq!(String::from_utf16_lossy(&p.edit.title), "Voice: the room's name");
        assert!(!p.edit.password);
        p.edit.set_text(&crate::ui::wide("Raid Night"));
        let purpose = p.edit.purpose;
        p.edit_done(purpose);
        assert_eq!((p.mode, p.voice_wnd.form_name().as_str(), p.edit.text.len()), (0, "Raid Night", 0));
        p.voice_wnd.select_field_for_test(crate::voicewnd::Field::Password);
        p.voice_window_key(key::CROSS, true);
        assert!(p.edit.password, "a password's stars");
        p.edit.set_text(&crate::ui::wide("hunter22"));
        let purpose = p.edit.purpose;
        p.edit_done(purpose);
        assert!(!p.edit.password && p.edit.text.is_empty(), "the box's copy cleared, its face put back");
        assert!(p.voice_wnd.has_password_for_test());
        // a cancel
        p.voice_wnd.select_field_for_test(crate::voicewnd::Field::Name);
        p.voice_window_key(key::CROSS, true);
        p.edit.set_text(&crate::ui::wide("Other"));
        assert!(p.voice_edit_done(p.edit.purpose, false));
        assert_eq!(p.voice_wnd.form_name(), "Raid Night", "cancelled: kept");
        // the control: the chat line's
        p.chat_key("", true);
        assert!(!p.voice_edit_done(p.edit.purpose, true) && p.mode == 2);
    }
}
