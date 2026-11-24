//! The buff window in play (`buffwnd`; a GM's petition 1761): the
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
        let pieces = self.buffs.pieces(vui, &lines, crate::settings::pet_effects(), [0.0; 2]);
        crate::hud::draw_fx_pieces(vui, r, &pieces, 1.0);
        if crate::settings::buff_timers() {
            draw_timers(r, &self.buffs.timers());
        }
        self.buffs.draw_info(vui, r, now);
    }

    /// The mouse moved to frame point `f` (none: it left the window): the
    /// buff under it named (`BuffWnd::hover`, petition 2924), where the bar
    /// is up in play and the HUD shown.
    pub(super) fn buff_hover(&mut self, f: Option<[f32; 2]>) {
        let up = matches!(self.mode, 0 | 33 | 38) && !self.search.open && !crate::hideui::hidden() && self.buff_window_on() && crate::influence::shown(self.mode);
        self.buffs.hover(f.filter(|_| up));
    }
}

/// Buff Timers (`BuffWnd::timers`): each time left at its icon's foot, right
/// aligned, a third of the icon high (8 pixels at least), outlined in black;
/// white, yellow in its last minute. The foot is the cell's frame's inside
/// (an icon hangs 4 of its 32 below the frame, `buffwnd::ROW_H`), so the
/// words stand on the icon, not on the world under the bar.
fn draw_timers(r: &mut crate::ui::UiRaster, timers: &[([f32; 4], String)]) {
    let Some(font) = r.assets.font else { return };
    let fh = r.ui.fonts[font].height.max(1) as f32;
    for (icon, t) in timers {
        let h = (icon[3] * 0.36).max(8.0);
        let scale = h / fh;
        let w = crate::ui::wide(t);
        let tw = r.ui.fonts[font].string_width(&w) as f32 * scale;
        let (x, y) = (icon[0] + icon[2] - tw - 1.0, icon[1] + icon[3] * 26.0 / 32.0 - h);
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            r.ui.text_scaled(font, x + dx, y + dy, scale, [0.0, 0.0, 0.0, 0.9], &w);
        }
        let color = if t.ends_with('s') { [1.0, 0.9, 0.3, 1.0] } else { [1.0; 4] };
        r.ui.text_scaled(font, x, y, scale, color, &w);
    }
}
