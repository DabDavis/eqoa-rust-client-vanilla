//! The bestiary in play (`bestiary.rs`): Character > Bestiary in the start menu while
//! the server offers it (`Session::bestiary`), its asks (0x7EF5) and its window (mode
//! [`MODE`], the client's own), with the page the server last sent. A child of `play`,
//! for `Play`'s fields.

use super::Play;
use crate::bestiary::{BestiaryUp, BestiaryWnd, MSG_BESTIARY, Page};

/// `VIWndGame`'s mode for the bestiary's window: none of the game's (0-44), nor the
/// client's other windows' (60-63).
pub const MODE: i32 = 66;

/// The page shown, its window and the start menu's entry.
#[derive(Default)]
pub struct Bestiary {
    /// what the server last sent (0x7EF5), kept for the window
    pub data: Option<Page>,
    wnd: BestiaryWnd,
    /// the window to open with the next HUD frame, which has the raster
    open: bool,
    keys: Vec<(i32, bool)>,
    /// the start menu's entry as last set (`bestiary_menu`)
    menu: Option<bool>,
}

impl Play {
    /// The mode is the bestiary's window.
    pub fn bestiary_owns(&self) -> bool {
        self.mode == MODE
    }

    /// A key for the bestiary's window, handled with the next HUD frame.
    pub fn bestiary_key(&mut self, key: i32, down: bool) {
        self.bestiary.keys.push((key, down));
    }

    /// A pointer at frame point `f` on the bestiary's window: the keys to press.
    pub fn bestiary_point(&mut self, f: [f32; 2]) -> Vec<i32> {
        self.bestiary.wnd.point(f)
    }

    /// The page the server sent last, if any (tests, a playshot's look).
    pub fn bestiary_page(&self) -> Option<&Page> {
        self.bestiary.data.as_ref()
    }

    /// `DoAction`'s part: Character > Bestiary asks the server for its first page and
    /// opens the window, which waits for it. True when the message was its.
    pub(super) fn bestiary_action(&mut self, id: u32) -> bool {
        if id != MSG_BESTIARY {
            return false;
        }
        if self.session.ask_bestiary(0) {
            eprintln!("play: the bestiary asked for");
            self.bestiary.data = None;
            self.bestiary.open = true;
            self.bestiary.keys.clear();
            self.mode = MODE;
        }
        true
    }

    /// 0x7EF5: a page, kept (drawn at once while the window is up).
    pub(super) fn bestiary_data(&mut self, data: &[u8]) {
        match crate::bestiary::read(data) {
            Some(p) => {
                eprintln!("play: bestiary page {} of {} ({} rows)", p.page + 1, p.pages, p.rows.len());
                self.bestiary.data = Some(p);
            }
            None => eprintln!("play: a bestiary page of {} bytes not read", data.len()),
        }
    }

    /// The start menu's Character > Bestiary, there while the server offers it
    /// (`Session::bestiary`).
    pub(super) fn bestiary_menu(&mut self) {
        let on = self.session.bestiary;
        if let Some(s) = self.start.as_mut()
            && self.bestiary.menu != Some(on)
        {
            s.set_bestiary(on);
            self.bestiary.menu = Some(on);
        }
    }

    /// The menus came again (a CLIENT image read): the entry is put in again.
    pub(super) fn bestiary_menu_again(&mut self) {
        self.bestiary.menu = None;
        self.bestiary_menu();
    }

    /// The window's open, keys and drawing, and its close: back to the start menu, as
    /// the game's character windows go. A page turned is asked for; the one shown stays
    /// until it comes.
    pub(super) fn run_bestiary(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster) {
        if std::mem::take(&mut self.bestiary.open) {
            self.bestiary.wnd.open(vui, r);
        }
        let mut ups = Vec::new();
        let mut cues = Vec::new();
        for (k, down) in std::mem::take(&mut self.bestiary.keys) {
            if self.mode != MODE {
                break;
            }
            ups.extend(self.bestiary.wnd.key(k, down, self.bestiary.data.as_ref(), &mut cues));
        }
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        for up in ups {
            match up {
                BestiaryUp::Close => {
                    self.mode = 0;
                    self.open_start_menu(false);
                }
                BestiaryUp::Page(n) => {
                    self.session.ask_bestiary(n);
                }
            }
        }
        if self.mode == MODE {
            self.bestiary.wnd.draw(vui, r, self.bestiary.data.as_ref());
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

    fn sent(p: &mut Play) -> Vec<Vec<u8>> {
        std::mem::take(p.session.record.as_mut().unwrap()).into_iter().map(|(_, d)| d).collect()
    }

    /// The bestiary's message is the bestiary's alone: from a server that offered it
    /// the entry asks for the first page (0x7EF5, page 0) and opens the window,
    /// waiting; the page then comes and is kept; → asks for the next and ← the one
    /// before (the page shown stays until it comes), △ goes back to the start menu.
    /// The controls: on a server that never offered it the entry does nothing, any
    /// other message is not the bestiary's, and a page not read leaves the last.
    #[test]
    fn the_entry_asks_and_the_window_turns_pages() {
        let mut p = play();
        assert!(p.bestiary_action(MSG_BESTIARY), "the bestiary's message");
        assert_eq!(p.mode, 0, "no offer: nothing opened");
        assert!(sent(&mut p).is_empty(), "no offer: nothing asked");
        assert!(!p.bestiary_action(crate::record::MSG_RECORD), "the record's is not the bestiary's");
        p.session.bestiary = true;
        assert!(p.bestiary_action(MSG_BESTIARY));
        assert_eq!(p.mode, MODE);
        assert_eq!(sent(&mut p), [vec![0xF5, 0x7E, 0, 0]], "the first page asked");
        assert!(p.bestiary_owns() && p.bestiary_page().is_none());
        p.bestiary_data(&crate::bestiary::tests::body(0, 2, 3, &[(0, "Freeport", ""), (1, "rat", "met Nov 3")]));
        assert_eq!(p.bestiary_page().map(|g| (g.page, g.pages, g.rows.len())), Some((0, 2, 2)));
        p.bestiary_data(&[2, 0]);
        assert!(p.bestiary_page().is_some(), "a page not read leaves the last");
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        p.run_bestiary(&vui, &mut r);
        p.bestiary_key(crate::pointer::key::RIGHT, true);
        p.run_bestiary(&vui, &mut r);
        assert_eq!(sent(&mut p), [vec![0xF5, 0x7E, 1, 0]], "→: the next page");
        assert_eq!(p.bestiary_page().map(|g| g.page), Some(0), "shown until it comes");
        p.bestiary_data(&crate::bestiary::tests::body(1, 2, 3, &[(0, "Qeynos", ""), (1, "wolf", "met Nov 5")]));
        p.bestiary_key(crate::pointer::key::LEFT, true);
        p.run_bestiary(&vui, &mut r);
        assert_eq!(sent(&mut p), [vec![0xF5, 0x7E, 0, 0]], "←: the page before");
        p.bestiary_key(132, true);
        p.run_bestiary(&vui, &mut r);
        assert_eq!(p.mode, 0, "△: back");
    }
}
