//! The combat record in play (`record.rs`): Character > Combat Record in the start
//! menu while the server offers it (`Session::combat_record`), its ask (0x7E21) and
//! its window (mode [`MODE`], the client's own), with the record as the server last
//! sent it. A child of `play`, for `Play`'s fields.

use super::Play;
use crate::record::{MSG_RECORD, Record, RecordUp, RecordWnd};

/// `VIWndGame`'s mode for the record's window: none of the game's (0-44).
pub const MODE: i32 = 60;

/// The record, its window and the start menu's entry.
#[derive(Default)]
pub struct CombatRecord {
    /// what the server last sent (0x7E21), kept for the window
    pub data: Option<Record>,
    wnd: RecordWnd,
    /// the window to open with the next HUD frame, which has the raster
    open: bool,
    keys: Vec<(i32, bool)>,
    /// the start menu's entry as last set (`record_menu`)
    menu: Option<bool>,
}

impl Play {
    /// The mode is the record window's.
    pub fn record_owns(&self) -> bool {
        self.mode == MODE
    }

    /// A key for the record's window, handled with the next HUD frame.
    pub fn record_key(&mut self, key: i32, down: bool) {
        self.record.keys.push((key, down));
    }

    /// A pointer at frame point `f` on the record's window: the keys to press.
    pub fn record_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        self.record.wnd.point(f)
    }

    /// The record the server sent last, if any (tests, a playshot's look).
    pub fn combat_record(&self) -> Option<&Record> {
        self.record.data.as_ref()
    }

    /// `DoAction`'s part: Character > Combat Record asks the server for the record
    /// and opens the window, which waits for it. True when the message was its.
    pub(super) fn record_action(&mut self, id: u32) -> bool {
        if id != MSG_RECORD {
            return false;
        }
        if self.session.ask_combat_record() {
            eprintln!("play: the combat record asked for");
            self.record.open = true;
            self.record.keys.clear();
            self.mode = MODE;
        }
        true
    }

    /// 0x7E21: the record, kept (drawn at once while the window is up).
    pub(super) fn record_data(&mut self, data: &[u8]) {
        match crate::record::read(data) {
            Some(r) => {
                eprintln!("play: the combat record of {} ({} rows)", r.name, r.rows.len());
                self.record.data = Some(r);
            }
            None => eprintln!("play: a combat record of {} bytes not read", data.len()),
        }
    }

    /// The start menu's Character > Combat Record, there while the server offers the
    /// record (`Session::combat_record`).
    pub(super) fn record_menu(&mut self) {
        let on = self.session.combat_record;
        if let Some(s) = self.start.as_mut()
            && self.record.menu != Some(on)
        {
            s.set_record(on);
            self.record.menu = Some(on);
        }
    }

    /// The menus came again (a CLIENT image read): the entry is put in again.
    pub(super) fn record_menu_again(&mut self) {
        self.record.menu = None;
        self.record_menu();
    }

    /// The window's open, keys and drawing, and its close: back to the start menu,
    /// as the game's character windows go.
    pub(super) fn run_record(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        if std::mem::take(&mut self.record.open) {
            self.record.wnd.open(vui, r);
        }
        let mut ups = Vec::new();
        let mut cues = Vec::new();
        for (k, down) in std::mem::take(&mut self.record.keys) {
            if self.mode != MODE {
                break;
            }
            ups.extend(self.record.wnd.key(k, down, &mut cues));
        }
        if self.mode == MODE {
            self.record.wnd.draw(vui, r, self.record.data.as_ref());
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for up in ups {
            match up {
                RecordUp::Close => {
                    self.mode = 0;
                    self.open_start_menu(false);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::session::{Player, Session};

    /// Play on a session nothing answers, recording what it sends.
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
        Play::new(s, Player::default(), Box::default())
    }

    /// The record's message is the record's alone: from a server that offered it the
    /// entry asks (0x7E21) and opens the window, waiting; the record then comes and is
    /// kept, and △ goes back to the start menu. The controls: on a server that never
    /// offered it the entry does nothing, and any other message is not the record's.
    #[test]
    fn the_entry_asks_and_the_window_shows_the_record() {
        let mut p = play();
        assert!(p.record_action(MSG_RECORD), "the record's message");
        assert_eq!(p.mode, 0, "no offer: nothing opened");
        assert_eq!(p.session.record.as_deref(), Some(&[][..]), "no offer: nothing asked");
        assert!(!p.record_action(0xAA00_0001), "Factions' is not the record's");
        p.session.combat_record = true;
        assert!(p.record_action(MSG_RECORD));
        assert_eq!(p.mode, MODE);
        assert_eq!(p.session.record.as_deref().map(|r| r.iter().map(|(_, d)| d.clone()).collect::<Vec<_>>()), Some(vec![vec![0x21, 0x7E, 1, 0, 0, 0]]), "the ask");
        assert!(p.record_owns() && p.combat_record().is_none());
        let rec = crate::record::tests::body("Dabran", &[(0, "Duels", ""), (1, "Won", "5")]);
        p.record_data(&rec);
        assert_eq!(p.combat_record().map(|r| (r.name.as_str(), r.rows.len())), Some(("Dabran", 2)));
        p.record_data(&[2, 0]);
        assert!(p.combat_record().is_some(), "a record not read leaves the last");
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        p.record_key(132, true);
        p.run_record(&vui, &mut r);
        assert_eq!(p.mode, 0, "△: back");
    }
}
