//! The buff window in play (`buffwnd`; the GM Jibrish's petition 1761): the
//! set the server sends, drawn in the effects strip's place, its clicks (left
//! selects, right removes: petition 1809), its d-pad (R2 held and L2; the
//! keyboard's C and Z) and its removals sent. A child of
//! `play`, for `Play`'s fields.

use super::Play;
use net::buffs::Line;

impl Play {
    /// The buff window is up: the server sends the set and Buff Window is on.
    pub(crate) fn buff_window_on(&self) -> bool {
        crate::settings::buff_window() && self.session.buffs_on()
    }

    /// The set now, each with its time left.
    fn buff_lines(&self) -> Vec<Line> {
        self.session.buffs_left(self.session.now())
    }

    /// The bar takes the d-pad (R2 held and L2, the keyboard's C and Z), or lets it go when it
    /// has it. False when it is not up or has nothing to walk.
    pub fn toggle_buff_focus(&mut self) -> bool {
        if self.buffs.focused() {
            self.buffs.let_go();
            self.cues.push(crate::sound::Cue::Ui(8));
            return true;
        }
        if !self.buff_window_on() || !crate::influence::shown(self.mode) || crate::hideui::hidden() {
            return false;
        }
        let lines = self.buff_lines();
        let took = self.buffs.take_focus(&lines, crate::settings::pet_effects());
        if took {
            self.cues.push(crate::sound::Cue::Ui(13));
        }
        took
    }

    /// A key while the bar has the d-pad (play, or R2's history held): the
    /// walk, ✕ (Enter) selects, □ (Backspace) a removal asked, △
    /// (Escape) or L2 let go.
    /// True when it was the bar's; its release too.
    pub fn buff_key(&mut self, key: i32, down: bool) -> bool {
        if !self.buffs.focused() {
            return false;
        }
        // the window gone (Buff Window off, another server, a window over play)
        if !self.buff_window_on() || !matches!(self.mode, 0 | 33 | 38) {
            self.buffs.let_go();
            return false;
        }
        let key = match key {
            13 => 134,
            8 | 127 => 133,
            27 => 132,
            17 => 136,
            18 => 137,
            k => k,
        };
        if !down {
            return matches!(key, 132..=134 | 136..=139 | 141);
        }
        let lines = self.buff_lines();
        let mut cues = Vec::new();
        let (took, remove) = self.buffs.key(key, &lines, crate::settings::pet_effects(), &mut cues);
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        if let Some(l) = remove {
            self.remove_buff(&l);
        }
        took
    }

    /// A pointer at frame point `f` on the bar (petition 1809): a left click
    /// (a tap) selects or lets go, a right click (`right`; a long press) takes
    /// it off. False when it is not on a buff (a left click there lets go).
    pub(super) fn buff_point(&mut self, f: [f32; 2], right: bool, finger: bool) -> bool {
        if !self.buff_window_on() || !crate::influence::shown(self.mode) {
            return false;
        }
        let now = self.session.now() as i64;
        let mut cues = Vec::new();
        let Some(remove) = self.buffs.point(f, now, right, finger, &mut cues) else { return false };
        self.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        if let Some(l) = remove {
            self.remove_buff(&l);
        }
        true
    }

    /// A line asked to come off (0x7E59); the server answers in the chat.
    fn remove_buff(&mut self, l: &Line) {
        eprintln!("play: buff {:?} ({}, id {}) asked off", l.name, if l.whose == net::buffs::ON_PET { "pet" } else { "self" }, l.id);
        self.session.send_buff_remove(l);
    }

    /// A finger held still at frame point `f` (`hud_hold`): on a buff, a
    /// right click's removal. False when it is not on one.
    pub(super) fn buff_hold(&mut self, f: [f32; 2]) -> bool {
        self.buff_point(f, true, true)
    }

    /// The bar drawn where the game's strip is (`influence`), into `r` within
    /// the effects' piece's place (`hudlayout`).
    pub(super) fn draw_buffs(&mut self, vui: &vi::ui::Vui, r: &mut crate::ui::UiRaster) {
        let lines = self.buff_lines();
        let now = self.session.now() as i64;
        let off = r.ui.placed([0.0, 0.0]);
        let pieces = self.buffs.pieces(vui, &lines, crate::settings::pet_effects(), off);
        crate::hud::draw_fx_pieces(vui, r, &pieces, 1.0);
        self.buffs.draw_info(vui, r, now);
    }
}
