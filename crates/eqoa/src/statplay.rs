//! The Stat Window in play (`statwnd`; petition 2925): F4 and `/stats` turn
//! it, a click on its X closes it. A child of `play`, for `Play`'s fields.

use super::Play;

impl Play {
    /// F4 (`binds`' "stat_window"): the Stat Window shown or hidden, kept.
    pub fn toggle_stat_window(&mut self) {
        self.set_stat_window(!crate::settings::stat_window());
    }

    /// The Stat Window shown (`on`) or hidden, kept, said in the chat; on a
    /// server that turns it off (`qol::STAT_CAPS`), that said instead.
    fn set_stat_window(&mut self, on: bool) {
        use crate::settings::qol;
        if qol::off(qol::STAT_CAPS) {
            self.add_chat("The Stat Window is off on this server.".into(), [1.0; 4]);
            return;
        }
        if let Err(e) = crate::settings::set_stat_window(on) {
            crate::say(&format!("stat window {}, not kept: {e}", if on { "shown" } else { "hidden" }));
        }
        self.cues.push(crate::sound::Cue::Ui(if on { 11 } else { 8 }));
    }

    /// `/stats` (`statwnd::command`).
    pub(super) fn stats_command(&mut self, arg: &str) {
        use crate::statwnd::Command;
        match crate::statwnd::command(arg) {
            Command::Toggle => self.toggle_stat_window(),
            Command::On(on) => self.set_stat_window(on),
            Command::Help => self.add_chat(crate::statwnd::HELP.into(), [1.0; 4]),
        }
    }

    /// A pointer at frame point `f` in the window's own place (`hudlayout`'s
    /// `local_point`): on its X, the window closed; anywhere on it, nothing
    /// else's. False off it, or with it not drawn.
    pub(super) fn stat_point(&mut self, f: [f32; 2]) -> bool {
        let Some(p) = self.stat_panel else { return false };
        let inside = |r: [i32; 4]| f[0] >= r[0] as f32 && f[0] < r[2] as f32 && f[1] >= r[1] as f32 && f[1] < r[3] as f32;
        if inside(p.close) {
            self.set_stat_window(false);
            self.stat_panel = None;
            return true;
        }
        inside(p.rect)
    }
}

#[cfg(test)]
mod tests {
    /// F4 and `/stats` turn the window and keep it; its X closes it, a click
    /// on it is its own and off it is not; on a server that turns the stat
    /// caps off, it stays off and says so. The controls: before a draw, no
    /// click is its; `/stats what` says how to type it.
    #[test]
    fn the_key_the_command_and_the_x() {
        use crate::settings::{self, qol};
        settings::testing::reset(false);
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        let mut s = net::session::Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap();
        s.self_id = Some(0x100);
        let mut p = super::Play::new(s, net::session::Player::default(), Box::default());
        let said = |p: &super::Play| p.chat.back().map(|(t, _)| t.clone()).unwrap_or_default();
        assert!(!settings::stat_window(), "off by default");
        p.toggle_stat_window();
        assert!(settings::stat_window(), "F4 shows it");
        p.chat_line("/stats");
        assert!(!settings::stat_window(), "/stats hides it");
        p.chat_line("/stats on");
        assert!(settings::stat_window());
        assert!(!p.stat_point([10.0, 160.0]), "not drawn yet: nothing found");
        let panel = crate::statwnd::panel([3, 7, 10]);
        p.stat_panel = Some(panel);
        assert!(p.stat_point([(panel.rect[0] + 20) as f32, (panel.rect[1] + 30) as f32]), "on it: its own");
        assert!(settings::stat_window(), "and still up");
        assert!(!p.stat_point([(panel.rect[2] + 5) as f32, (panel.rect[1] + 30) as f32]), "off it: the world's");
        assert!(p.stat_point([(panel.close[0] + 2) as f32, (panel.close[1] + 2) as f32]), "the X");
        assert!(!settings::stat_window(), "closed");
        qol::set_off(qol::STAT_CAPS);
        p.toggle_stat_window();
        assert!(!settings::stat_window(), "off on this server");
        assert_eq!(said(&p), "The Stat Window is off on this server.");
        qol::set_off(0);
        assert!(!settings::stat_window(), "and the player's own choice kept (hidden)");
        p.chat_line("/stats what");
        assert_eq!(said(&p), crate::statwnd::HELP);
        settings::testing::reset(false);
    }
}
