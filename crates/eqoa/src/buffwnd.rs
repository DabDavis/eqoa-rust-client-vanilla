//! The buff window (the GM Jibrish's petition 1761, 2026-11-12: "Give the buff
//! window 2 horizontal bars for buffs on the player, and a third for buffs on
//! their pet. Allow someone to mouse click off their buffs, and add a way for
//! console to navigate to the buff bar and then use arrow keys to click off
//! buffs, or check their name and remaining duration"). The client's own, in
//! the place of the game's effects strip (`influence`, `VIWndInfluence` at
//! (143, 15)) on a server that sends the set (go-eqoa-server's 0x7E57-0x7E59,
//! `net::buffs`) with Extended > UI > HUD's Buff Window on; anywhere else the
//! game's strip, as it was.
//!
//! - The rows: the player's effects in the strip's own art (its caps, a cell of
//!   34 a buff), eight to a row, a second row under the first for nine to
//!   sixteen (the game's strip holds eight); the pet's under them in a row of
//!   their own at half the size (the art of the row under the pet's bar, which
//!   it replaces), with Extended's Pet Effects on.
//! - The mouse and a finger (the GM Jibrish's petition 1809, 2026-11-12: "Make
//!   it right click to remove, and left click to unselect"): a left click (a
//!   tap) on a buff selects it, its name and its time left shown under the rows
//!   for `INFO_MS`; a left click on the selected one, or anywhere off the bar,
//!   lets it go; on another, selects that one. A right click (a long press) on
//!   a buff takes it off at once, selected or not (a buff the server says
//!   cannot come off, a harmful one, is selected and says so).
//! - A controller: hold R2 (the chat box's history, where the walk goes on) and
//!   press L2, and the bar has the d-pad until it is let go: left and right
//!   along a row, up and down between the rows (the highlighted buff framed),
//!   ✕ selects or lets go of the highlighted one (its name and time left
//!   shown), □ takes it off, △ or L2 lets go of the bar. The keyboard: C held
//!   and Z, then the arrows, Enter selects, Backspace takes it off,
//!   Escape lets go. While the bar has them, those keys are its alone; the
//!   sticks walk and turn as ever.
//! - A removal goes to the server (0x7E59), which takes off only what is
//!   beneficial and says so in the chat either way; the set it sends again
//!   takes the buff out of the rows.

use net::buffs::{Line, NO_END, ON_PET, ON_PLAYER};
use vi::ui::{FrameParam, Raster, Vui, Window, icon_id, wide};

/// The strip's place (`VIWndGame::Init`'s for `VIWndInfluence`).
pub const AT: [i32; 2] = [143, 15];
/// Buffs to a row, and the player's rows at most.
pub const PER_ROW: usize = 8;
pub const PLAYER_ROWS: usize = 2;
/// A player's row's stride: a cell's 34 and the 4 its icons hang below it.
pub const ROW_H: i32 = 38;
/// The pet's row's scale, and its height (`hud::PET_FX_ROW_H`'s).
pub const PET_SCALE: f32 = 0.5;
pub const PET_ROW_H: i32 = 20;
/// How long a click's name and time stay up.
pub const INFO_MS: i64 = 6000;

/// One buff's cell: the line it shows (its index in the set drawn), its row
/// (0 and 1 the player's, 2 the pet's), its column and the frame rectangle of
/// its cell (x, y, w, h).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub line: usize,
    pub row: usize,
    pub col: usize,
    pub r: [f32; 4],
}

/// The cells for `lines` (the player's first, then the pet's), `cell_w` the
/// strip's cell's width and `cap_w` its left cap's: each row starts after a
/// cap, its cells side by side, the last 3 narrower as the strip's last is.
/// The pet's row only when `pets` (Pet Effects on).
pub fn layout(lines: &[Line], cell_w: i32, cap_w: i32, pets: bool) -> Vec<Cell> {
    let mine: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].whose == ON_PLAYER).take(PER_ROW * PLAYER_ROWS).collect();
    let theirs: Vec<usize> = if pets { (0..lines.len()).filter(|&i| lines[i].whose == ON_PET).take(PER_ROW).collect() } else { Vec::new() };
    let mut out = Vec::new();
    let row_of = |idx: &[usize], row: usize, y: f32, k: f32, out: &mut Vec<Cell>| {
        let mut x = AT[0] as f32 + cap_w as f32 * k;
        for (col, &line) in idx.iter().enumerate() {
            let w = if col == idx.len() - 1 { cell_w - 3 } else { cell_w };
            out.push(Cell { line, row, col, r: [x, y, w as f32 * k, 34.0 * k] });
            x += w as f32 * k;
        }
    };
    let rows = mine.len().div_ceil(PER_ROW);
    for (row, chunk) in mine.chunks(PER_ROW).enumerate() {
        row_of(chunk, row, (AT[1] + row as i32 * ROW_H) as f32, 1.0, &mut out);
    }
    if !theirs.is_empty() {
        row_of(&theirs, PLAYER_ROWS, (AT[1] + rows as i32 * ROW_H) as f32, PET_SCALE, &mut out);
    }
    out
}

/// The window's foot in the frame: under its last row.
pub fn bottom(cells: &[Cell]) -> i32 {
    cells.iter().map(|c| if c.row == PLAYER_ROWS { c.r[1] as i32 + PET_ROW_H } else { c.r[1] as i32 + ROW_H }).max().unwrap_or(AT[1])
}

/// A time left as it is shown: "1h 05m", "12:34", "45s"; none for no end.
pub fn time_text(ms: u32) -> String {
    if ms == NO_END {
        return String::new();
    }
    let s = ms.div_ceil(1000);
    match s {
        3600.. => format!("{}h {:02}m", s / 3600, s % 3600 / 60),
        60.. => format!("{}:{:02}", s / 60, s % 60),
        _ => format!("{s}s"),
    }
}

/// What makes a line the same line across sets: whose, its id and icon.
fn key_of(l: &Line) -> (u8, u32, u32, String) {
    (l.whose, l.id, l.icon, l.name.clone())
}

pub struct BuffWnd {
    /// the bar has the d-pad (R2 + L2; C + Z)
    focused: bool,
    /// the lit buff, and until when its name shows (a click's; the session's ms)
    lit: Option<(u8, u32, u32, String)>,
    lit_until: i64,
    /// focused: the highlighted buff selected (✕), its name and time shown
    chosen: bool,
    /// the last pointer was a finger (the hint says long-press, not right-click)
    finger: bool,
    /// the lines and cells drawn last, and the frame's offset from the frame to
    /// where they were placed (`Ui::placed`), for a pointer
    lines: Vec<Line>,
    cells: Vec<Cell>,
    off: [f32; 2],
    /// the strip's cell's and left cap's widths, as last drawn (the d-pad's
    /// nearest across before the first draw: SUPPORT's 32 and 5)
    cw: i32,
    capw: i32,
}

impl Default for BuffWnd {
    fn default() -> Self {
        BuffWnd { focused: false, lit: None, lit_until: 0, chosen: false, finger: false, lines: Vec::new(), cells: Vec::new(), off: [0.0; 2], cw: 32, capw: 5 }
    }
}

impl BuffWnd {
    pub fn focused(&self) -> bool {
        self.focused
    }

    /// The lit line in `lines`, by its index.
    fn lit_index(&self, lines: &[Line]) -> Option<usize> {
        let k = self.lit.as_ref()?;
        lines.iter().position(|l| &key_of(l) == k)
    }

    /// The set as it is now: a lit buff gone (it ran out, or came off) lets
    /// go of the light, and the bar of the d-pad when it is empty; focused, the
    /// light moves to the first buff.
    pub fn sync(&mut self, lines: &[Line], pets: bool) {
        let cells = layout(lines, self.cw.max(1), self.capw.max(1), pets);
        let shown = |i: usize| cells.iter().any(|c| c.line == i);
        if self.lit_index(lines).is_none_or(|i| !shown(i)) {
            self.lit = None;
            if self.focused {
                match cells.first() {
                    Some(c) => self.lit = Some(key_of(&lines[c.line])),
                    None => self.focused = false,
                }
            }
        }
    }

    /// The d-pad to the bar (R2 + L2; C + Z): the first buff lit. False with none.
    pub fn take_focus(&mut self, lines: &[Line], pets: bool) -> bool {
        let cells = layout(lines, self.cw.max(1), self.capw.max(1), pets);
        let Some(first) = cells.first() else { return false };
        self.focused = true;
        self.chosen = false;
        if self.lit_index(lines).is_none_or(|i| !cells.iter().any(|c| c.line == i)) {
            self.lit = Some(key_of(&lines[first.line]));
        }
        true
    }

    /// The d-pad handed back (△, L2, Escape), the light with it.
    pub fn let_go(&mut self) {
        self.focused = false;
        self.chosen = false;
        self.lit = None;
    }

    /// A key (the game's codes; the keyboard's arrows come as the pad's)
    /// while focused: whether it was the bar's, and the line to take off when
    /// □ asked it of one that may come off (✕ selects or lets go of the
    /// highlighted one). Unfocused, no key is the bar's.
    pub fn key(&mut self, key: i32, lines: &[Line], pets: bool, cues: &mut Vec<i32>) -> (bool, Option<Line>) {
        if !self.focused {
            return (false, None);
        }
        let cells = layout(lines, self.cw.max(1), self.capw.max(1), pets);
        let at = self.lit_index(lines).and_then(|i| cells.iter().position(|c| c.line == i));
        let Some(at) = at else {
            self.let_go();
            return (matches!(key, 132..=139 | 141), None);
        };
        let here = cells[at];
        let moved = match key {
            136 => at.checked_sub(1).filter(|&j| cells[j].row == here.row),
            137 => Some(at + 1).filter(|&j| j < cells.len() && cells[j].row == here.row),
            138 | 139 => {
                // the row above or below that has buffs, the cell nearest across
                let rows: Vec<usize> = {
                    let mut v: Vec<usize> = cells.iter().map(|c| c.row).collect();
                    v.dedup();
                    v
                };
                let k = rows.iter().position(|&r| r == here.row).unwrap_or(0);
                let to = if key == 138 { k.checked_sub(1) } else { Some(k + 1).filter(|&k| k < rows.len()) };
                to.and_then(|k| {
                    let mid = here.r[0] + here.r[2] / 2.0;
                    (0..cells.len()).filter(|&j| cells[j].row == rows[k]).min_by(|&a, &b| {
                        let d = |j: usize| (cells[j].r[0] + cells[j].r[2] / 2.0 - mid).abs();
                        d(a).total_cmp(&d(b))
                    })
                })
            }
            134 => {
                self.chosen = !self.chosen;
                cues.push(if self.chosen { 11 } else { 8 });
                return (true, None);
            }
            133 => {
                let l = &lines[here.line];
                if l.removable {
                    cues.push(13);
                    return (true, Some(l.clone()));
                }
                // one that may not come off: selected, so it says so
                self.chosen = true;
                cues.push(9);
                return (true, None);
            }
            132 | 141 => {
                cues.push(8);
                self.let_go();
                return (true, None);
            }
            _ => return (false, None),
        };
        match moved {
            Some(j) => {
                cues.push(11);
                self.lit = Some(key_of(&lines[cells[j].line]));
            }
            None => cues.push(9),
        }
        (true, None)
    }

    /// A pointer at frame point `p` (as placed on the screen); `right` a
    /// right click or a long press, `finger` a touch screen's. Off the bar,
    /// None, and a left click lets go of a selected buff (the click is still
    /// the world's). On a buff, Some: a left click selects it, or lets it go
    /// when it was the one selected; a right click takes it off at once (the
    /// line to send) when it may come off, else selects it to say so.
    pub fn point(&mut self, p: [f32; 2], now: i64, right: bool, finger: bool, cues: &mut Vec<i32>) -> Option<Option<Line>> {
        let at = self.cells.iter().find(|c| {
            let (x, y) = (p[0] - self.off[0], p[1] - self.off[1]);
            x >= c.r[0] && x < c.r[0] + c.r[2] && y >= c.r[1] && y < c.r[1] + c.r[3] + if c.row == PLAYER_ROWS { 2.0 } else { 4.0 }
        });
        let Some(l) = at.and_then(|c| self.lines.get(c.line)).cloned() else {
            if !right && !self.focused {
                self.lit = None;
                self.lit_until = 0;
            }
            return None;
        };
        self.finger = finger;
        let selected = self.lit.as_ref() == Some(&key_of(&l)) && (if self.focused { self.chosen } else { now < self.lit_until });
        if right {
            if l.removable {
                cues.push(13);
                if self.lit.as_ref() == Some(&key_of(&l)) && !self.focused {
                    self.lit = None;
                    self.lit_until = 0;
                }
                return Some(Some(l));
            }
            cues.push(9);
        } else if selected {
            cues.push(8);
            if self.focused {
                self.chosen = false;
            } else {
                self.lit = None;
                self.lit_until = 0;
            }
            return Some(None);
        } else {
            cues.push(11);
        }
        self.lit = Some(key_of(&l));
        self.lit_until = now + INFO_MS;
        self.chosen = true;
        Some(None)
    }

    /// The rows' art for `lines` (the set now, each with its time left), each
    /// cell noted for a pointer at `off` from the frame (`Ui::placed` of the
    /// frame's corner). `pets`: Pet Effects on. Drawn by `hud::draw_fx_pieces`,
    /// then `draw_info` over them.
    pub fn pieces(&mut self, vui: &Vui, lines: &[Line], pets: bool, off: [f32; 2]) -> Vec<crate::hud::FxPiece> {
        let (cap_l, cell, cap_r) = (vui.entry(159), vui.entry(157), vui.entry(160));
        self.sync(lines, pets);
        (self.cw, self.capw) = (cell.uv1[0], cap_l.uv1[0]);
        let cells = layout(lines, cell.uv1[0], cap_l.uv1[0], pets);
        self.lines = lines.to_vec();
        self.off = off;
        use crate::hud::{FxArt, FxPiece};
        let mut pieces = Vec::new();
        let mut rows: Vec<usize> = cells.iter().map(|c| c.row).collect();
        rows.dedup();
        for row in rows {
            let rc: Vec<&Cell> = cells.iter().filter(|c| c.row == row).collect();
            let k = if row == PLAYER_ROWS { PET_SCALE } else { 1.0 };
            let (x0, y) = (rc[0].r[0] - cap_l.uv1[0] as f32 * k, rc[0].r[1]);
            pieces.push(FxPiece { art: FxArt::Entry(159, cap_l.uv1), r: [x0, y, cap_l.uv1[0] as f32 * k, cap_l.uv1[1] as f32 * k] });
            for c in &rc {
                pieces.push(FxPiece { art: FxArt::Entry(157, [(c.r[2] / k).round() as i32, 34]), r: c.r });
                pieces.push(FxPiece { art: FxArt::Icon(lines[c.line].icon), r: [c.r[0] + 3.0 * k, c.r[1] + 6.0 * k, 32.0 * k, 32.0 * k] });
            }
            let last = rc[rc.len() - 1];
            pieces.push(FxPiece { art: FxArt::Entry(160, cap_r.uv1), r: [last.r[0] + last.r[2], y, cap_r.uv1[0] as f32 * k, cap_r.uv1[1] as f32 * k] });
        }
        self.cells = cells;
        pieces
    }

    /// Over the rows (`pieces`): the lit buff framed, and under the rows its
    /// name, its time left at `now` and what may be done with it.
    pub fn draw_info(&mut self, vui: &Vui, r: &mut dyn Raster, now: i64) {
        let (lines, cells) = (&self.lines, &self.cells);
        // the lit buff: a frame round its icon, and its name and time under the rows
        let shown = self.focused || now < self.lit_until;
        let Some(i) = self.lit_index(lines).filter(|_| shown) else { return };
        // focused with none selected: the highlight framed and the keys only
        let info = !self.focused || self.chosen;
        let Some(c) = cells.iter().find(|c| c.line == i).copied() else { return };
        let k = if c.row == PLAYER_ROWS { PET_SCALE } else { 1.0 };
        let (x0, y0) = ((c.r[0] + 2.0 * k) as i32, (c.r[1] + 5.0 * k) as i32);
        let (x1, y1) = (x0 + (34.0 * k) as i32, y0 + (34.0 * k) as i32);
        let gold = [1.0, 0.85, 0.2, 1.0];
        for (a, b) in [([x0, y0], [x1, y0 + 2]), ([x0, y1 - 2], [x1, y1]), ([x0, y0], [x0 + 2, y1]), ([x1 - 2, y0], [x1, y1])] {
            r.fill_fan(gold, [[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]]);
        }
        let l = &lines[i];
        let fh = vui.font_height(r);
        let name = wide(&if l.whose == ON_PET { format!("{} (pet)", l.name) } else { l.name.clone() });
        let left = wide(&time_text(l.left_ms));
        let hint = wide(hint(self.focused, info, l.removable, self.finger));
        let mut win = Window::default();
        let w = if info { vui.string_width(r, &name) + vui.string_width(r, &left) + 36 } else { 0 }.max(win.text_width(vui, r, &hint) + 20).max(180);
        let h = if info { fh * 2 + 12 } else { fh + 12 };
        let at = [AT[0], bottom(cells) + 2];
        r.blit(icon_id("black"), 0, at, [0, 0], [w, h], 3, 0.8);
        let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) };
        vui.blit_frame(r, at, [w, h], &fp, 3, 1.0, true);
        let mut y = at[1] + 5;
        if info {
            vui.draw_string_ex(r, &name, at[0] + 10, y, [1.0, 1.0, 0.0, 1.0], true);
            let lw = vui.string_width(r, &left);
            vui.draw_string_ex(r, &left, at[0] + w - 10 - lw, y, [1.0; 4], true);
            y += fh;
        }
        win.init(vui, r, [at[0], at[1]], w, 1, 0);
        win.open(true);
        win.draw_text_internal(vui, r, &hint, at[0] + 10, y + fh / 2, [0.85, 0.85, 0.85, 1.0], 1);
    }
}

/// The line under a selected buff's name: what may be done with it, for the
/// d-pad (`focused`; `info` one selected) or a pointer (`finger` a touch's).
pub fn hint(focused: bool, info: bool, removable: bool, finger: bool) -> &'static str {
    match (focused, info, removable, finger) {
        (true, false, _, _) => "|x Select   |s Remove   |t Done",
        (true, true, true, _) => "|s Remove   |x Deselect   |t Done",
        (true, true, false, _) => "Cannot be removed   |x Deselect   |t Done",
        (false, _, true, false) => "Right-click to remove",
        (false, _, true, true) => "Long-press to remove",
        (false, _, false, _) => "Cannot be removed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(whose: u8, id: u32, removable: bool) -> Line {
        Line { whose, removable, id, icon: 1000 + id, left_ms: 60_000, name: format!("Buff {id}") }
    }

    fn set(mine: u32, pets: u32) -> Vec<Line> {
        (1..=mine).map(|i| line(ON_PLAYER, i, i % 5 != 0)).chain((1..=pets).map(|i| line(ON_PET, 100 + i, true))).collect()
    }

    /// Eight to a row, a second row for nine to sixteen, the pet's under them at
    /// half the size; the pet's row only with Pet Effects on (the control).
    #[test]
    fn the_rows_hold_two_of_the_players_and_one_of_the_pets() {
        let l = set(11, 3);
        let c = layout(&l, 37, 13, true);
        assert_eq!(c.len(), 14);
        assert_eq!((c[0].row, c[0].r), (0, [156.0, 15.0, 37.0, 34.0]));
        assert_eq!((c[7].row, c[7].r[2]), (0, 34.0), "the row's last is 3 narrower");
        assert_eq!((c[8].row, c[8].col, c[8].r[1]), (1, 0, 53.0));
        assert_eq!((c[11].row, c[11].r), (2, [149.5, 91.0, 18.5, 17.0]), "the pet's at half, under both");
        assert_eq!(bottom(&c), 111);
        let one = layout(&set(3, 2), 37, 13, true);
        assert_eq!(one[3].r[1], 53.0, "one row of the player's: the pet's right under it");
        assert_eq!(layout(&l, 37, 13, false).len(), 11, "Pet Effects off: no pet row");
        assert_eq!(layout(&set(20, 0), 37, 13, true).len(), 16, "two rows at most");
        assert!(layout(&[], 37, 13, true).is_empty());
    }

    #[test]
    fn times_read_as_a_clock() {
        assert_eq!(time_text(45_000), "45s");
        assert_eq!(time_text(44_001), "45s");
        assert_eq!(time_text(754_000), "12:34");
        assert_eq!(time_text(3_900_000), "1h 05m");
        assert_eq!(time_text(NO_END), "");
    }

    /// The d-pad, focused: right along the row, down to the next row's nearest,
    /// down again to the pet's, ✕ selects and lets go, □ takes off one that may
    /// come off and not one that may not (that one selected instead), △ lets
    /// go. The control: unfocused, no key is the bar's.
    #[test]
    fn the_dpad_walks_the_buffs_and_takes_one_off() {
        let l = set(10, 2);
        let mut b = BuffWnd::default();
        let mut cues = Vec::new();
        assert_eq!(b.key(137, &l, true, &mut cues), (false, None), "unfocused: not the bar's");
        assert_eq!(b.key(133, &l, true, &mut cues), (false, None), "unfocused: □ not the bar's");
        assert!(b.take_focus(&l, true));
        let lit = |b: &BuffWnd| b.lit.as_ref().map(|k| k.1);
        assert_eq!(lit(&b), Some(1));
        assert!(!b.chosen, "highlighted, not selected");
        assert_eq!(b.key(134, &l, true, &mut cues), (true, None), "✕ selects");
        assert!(b.chosen);
        assert_eq!(b.key(134, &l, true, &mut cues), (true, None), "✕ again lets go");
        assert!(!b.chosen && b.focused());
        b.key(137, &l, true, &mut cues);
        b.key(137, &l, true, &mut cues);
        assert_eq!(lit(&b), Some(3));
        b.key(139, &l, true, &mut cues);
        assert_eq!(lit(&b), Some(10), "down: the second row's nearest (it has two)");
        b.key(139, &l, true, &mut cues);
        assert_eq!(lit(&b), Some(102), "down: the pet's row's nearest");
        assert_eq!(b.key(134, &l, true, &mut cues), (true, None), "✕ never removes");
        assert_eq!(b.key(133, &l, true, &mut cues), (true, Some(l[11].clone())), "□: the pet's buff off");
        b.key(138, &l, true, &mut cues);
        b.key(138, &l, true, &mut cues);
        b.key(136, &l, true, &mut cues);
        assert_eq!(b.key(136, &l, true, &mut cues).0, true, "left at the row's start: still the bar's");
        b.key(137, &l, true, &mut cues);
        b.key(137, &l, true, &mut cues);
        b.key(137, &l, true, &mut cues);
        b.key(137, &l, true, &mut cues);
        assert_eq!(lit(&b), Some(5));
        b.key(134, &l, true, &mut cues);
        assert_eq!(b.key(133, &l, true, &mut cues), (true, None), "□ on one that may not come off: nothing");
        assert!(b.chosen, "selected, to say so");
        assert_eq!(b.key(132, &l, true, &mut cues), (true, None));
        assert!(!b.focused() && !b.chosen);
        assert!(!b.take_focus(&[], true), "nothing to walk");
    }

    #[test]
    fn the_hint_names_the_keys() {
        assert_eq!(hint(true, false, true, false), "|x Select   |s Remove   |t Done");
        assert_eq!(hint(true, true, true, false), "|s Remove   |x Deselect   |t Done");
        assert_eq!(hint(true, true, false, true), "Cannot be removed   |x Deselect   |t Done");
        assert_eq!(hint(false, true, true, false), "Right-click to remove");
        assert_eq!(hint(false, true, true, true), "Long-press to remove");
        assert_eq!(hint(false, true, false, false), "Cannot be removed");
    }

    /// The set changing under it: the lit one gone, the light to the first
    /// while focused, and the d-pad let go when none is left.
    #[test]
    fn a_lit_buff_gone_moves_the_light() {
        let mut l = set(3, 0);
        let mut b = BuffWnd::default();
        b.take_focus(&l, true);
        let mut cues = Vec::new();
        b.key(137, &l, true, &mut cues);
        l.remove(1);
        b.sync(&l, true);
        assert_eq!(b.lit.as_ref().map(|k| k.1), Some(1));
        b.sync(&[], true);
        assert!(!b.focused() && b.lit.is_none());
    }

    /// Through the drawing: `pieces` notes each cell where it went (here 10
    /// right of the frame, as a moved piece is), a left click there selects
    /// it and a right click takes it off; the name's box draws over a null
    /// raster, focused or not. The control: before `pieces`, a click there
    /// finds nothing.
    #[test]
    fn a_click_finds_a_buff_where_it_was_drawn() {
        use crate::pointer::testing::NullRaster;
        let mut atlas = vec![vi::ui::UiTexEntry::default(); 161];
        atlas[157].uv1 = [32, 32];
        atlas[159].uv1 = [5, 34];
        atlas[160].uv1 = [3, 34];
        let vui = Vui::new(Default::default(), Default::default(), atlas);
        let l = set(4, 1);
        let mut b = BuffWnd::default();
        let mut cues = Vec::new();
        let at = [10.0 + 148.0 + 32.0 + 16.0, 30.0];
        assert_eq!(b.point(at, 0, false, false, &mut cues), None, "nothing noted yet");
        let p = b.pieces(&vui, &l, true, [10.0, 0.0]);
        assert_eq!(p.len(), 2 + 4 * 2 + 2 + 2, "two rows: caps, a cell and an icon a buff");
        assert_eq!(b.point(at, 0, false, false, &mut cues), Some(None));
        b.draw_info(&vui, &mut NullRaster, 10);
        assert_eq!(b.point(at, 20, true, false, &mut cues), Some(Some(l[1].clone())));
        b.take_focus(&l, true);
        b.draw_info(&vui, &mut NullRaster, 30);
        b.key(134, &l, true, &mut cues);
        b.draw_info(&vui, &mut NullRaster, 40);
    }

    /// A pointer (petition 1809): a left click selects a buff, a left click on
    /// it again lets it go, on another selects that, off the bar lets go too;
    /// a right click takes one off at once, selected or not, and one that may
    /// not come off is only selected. The controls: two left clicks remove
    /// nothing, a right click off the bar is not the bar's, nothing before a
    /// frame drew it.
    #[test]
    fn left_selects_right_removes() {
        let l = set(9, 0);
        let mut b = BuffWnd::default();
        let mut cues = Vec::new();
        assert_eq!(b.point([160.0, 20.0], 0, false, false, &mut cues), None, "nothing drawn yet");
        b.lines = l.clone();
        b.cells = layout(&l, 37, 13, true);
        b.off = [10.0, 0.0];
        let lit = |b: &BuffWnd, now: i64| b.lit.as_ref().filter(|_| now < b.lit_until).map(|k| k.1);
        assert_eq!(b.point([170.0, 20.0], 0, false, false, &mut cues), Some(None), "left: selected");
        assert_eq!(lit(&b, 50), Some(1));
        assert_eq!(b.point([170.0, 20.0], 100, false, false, &mut cues), Some(None), "left again: not removed");
        assert_eq!(lit(&b, 150), None, "left again: let go");
        b.point([170.0, 20.0], 200, false, false, &mut cues);
        assert_eq!(b.point([210.0, 20.0], 300, false, false, &mut cues), Some(None), "another: selected");
        assert_eq!(lit(&b, 350), Some(2));
        assert_eq!(b.point([600.0, 20.0], 400, false, false, &mut cues), None, "off the bar: the world's");
        assert_eq!(lit(&b, 450), None, "and let go");
        assert_eq!(b.point([210.0, 20.0], 500, false, false, &mut cues), Some(None), "selected again");
        assert_eq!(b.point([210.0, 20.0], 500 + INFO_MS, false, false, &mut cues), Some(None), "its name gone: selected again, not off");
        assert_eq!(lit(&b, 600 + INFO_MS), Some(2));
        assert_eq!(b.point([600.0, 20.0], 0, true, false, &mut cues), None, "a right click off the bar");
        assert_eq!(b.point([180.0, 20.0], 0, true, false, &mut cues), Some(Some(l[0].clone())), "right: off, none selected first");
        assert_eq!(b.point([210.0, 20.0], 700 + INFO_MS, true, true, &mut cues), Some(Some(l[1].clone())), "a long press on the selected one: off");
        // one that may not come off (the fifth): right: selected, refused
        let x = 10.0 + b.cells[4].r[0] + 5.0;
        assert_eq!(b.point([x, 20.0], 0, true, false, &mut cues), Some(None));
        assert_eq!(lit(&b, 10), Some(5), "selected, to say so");
    }
}
