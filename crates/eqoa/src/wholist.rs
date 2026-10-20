//! The who list: `VIClientPart38334` (the request 0x0E01 with a serial, the
//! reply 0x0E02, a stale serial dropped), `VIWndWhoList` (ten rows of name,
//! race, class and level in a titled window, "searching..." while the reply
//! is out) and `VIWndOptionList`, the popup a row opens: Tell (the edit box,
//! then `/t name text`), Invite (`InviteToGroup`), Mail (not ported). A
//! pointer (`pointer`) selects a row and opens its popup from the selection;
//! off the popup it closes it.

#![allow(clippy::too_many_arguments)]

use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::startmenu::KeyFlash;
use net::stream::{Reader, Writer};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// `VIRec32_C035B0`: a row.
#[derive(Clone, Debug, Default)]
pub struct WhoRow {
    pub race: u8,
    pub class: u8,
    pub level: u8,
    pub name: String,
    pub x1c: u32,
}

/// `VIClientPart38334`.
#[derive(Default)]
pub struct Who {
    pub list: Vec<WhoRow>,
    pub pending: bool,
    serial: i32,
}

impl Who {
    /// `Open`: `Close`, then the request (0x0E01, 1, the serial).
    pub fn open(&mut self) -> Vec<u8> {
        self.close();
        let mut w = Writer::op(0x0E01);
        w.u8(1).signed(self.serial as i64);
        self.pending = true;
        w.data
    }

    /// `Close`: the list emptied, a new serial for the next request.
    pub fn close(&mut self) {
        self.list.clear();
        self.pending = false;
        self.serial += 1;
    }

    /// `Recv0E02`: a list (1) for the request still out.
    pub fn recv(&mut self, data: &[u8]) {
        let mut r = Reader::new(data);
        if r.u8() != 1 {
            return;
        }
        if r.signed() as i32 != self.serial {
            return;
        }
        let n = r.signed().clamp(0, 4096);
        for _ in 0..n {
            let (race, class, level) = (r.u8(), r.u8(), r.u8());
            let name = String::from_utf8_lossy(&r.string(24)).into_owned();
            let x1c = r.u32();
            if r.error {
                break;
            }
            self.list.push(WhoRow { race, class, level, name, x1c });
        }
        self.pending = false;
    }
}

/// `VIWndOptionList`: choices under a title, the chosen one ticked.
#[derive(Default)]
pub struct OptionList {
    title: Vec<u16>,
    options: Vec<Vec<u16>>,
    center: [i32; 2],
    width: i32,
    rows: i32,
    pub sel: i32,
    pub visible: bool,
    window: Window,
    /// the choices drawn, for a pointer
    hits: Hits,
}

impl OptionList {
    /// A pointer at frame point `p`: `None` off the window; on a choice, it
    /// chosen at once (✕); on the rest of it, nothing.
    pub fn point(&mut self, p: [f32; 2]) -> Option<Option<i32>> {
        match self.hits.at(p)? {
            -1 => Some(None),
            i => {
                self.sel = i;
                Some(Some(key::CROSS))
            }
        }
    }

    /// `Setup(title, options, count, center, width)`.
    pub fn setup(&mut self, vui: &Vui, r: &dyn Raster, title: &[u16], options: Vec<Vec<u16>>, center: [i32; 2], width: i32) {
        self.title = title.to_vec();
        self.center = center;
        let mut w = width.max(300).max(vui.string_width(r, title) + 80);
        for o in &options {
            w = w.max(vui.string_width(r, o) + 80);
        }
        self.width = w;
        self.rows = options.len() as i32 + 3;
        self.options = options;
        self.window.init(vui, r, center, self.width, self.rows, 0);
        self.window.set_title(Some(title));
        self.sel = 0;
    }

    /// `OnMessage` 295: the focus opens it on the first choice.
    pub fn focus(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.close(true);
        self.window.init(vui, r, self.center, self.width, self.rows, 0);
        self.window.open(false);
        let t = self.title.clone();
        self.window.set_title(Some(&t));
        self.sel = 0;
    }

    /// `OnKeyDown`: up/down move the tick, Enter and Triangle close it with
    /// the choice, accepted or not (308).
    pub fn key(&mut self, key: i32, now: i64, cues: &mut Vec<i32>) -> Option<(i32, bool)> {
        match key {
            19 | 138 => {
                if self.sel > 0 {
                    self.sel -= 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            20 | 139 => {
                if self.sel < self.options.len() as i32 - 1 {
                    self.sel += 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                } else {
                    cues.push(9);
                }
            }
            13 | 134 => {
                self.visible = false;
                cues.push(13);
                return Some((self.sel, true));
            }
            27 | 132 => {
                self.visible = false;
                cues.push(8);
                return Some((self.sel, false));
            }
            _ => {}
        }
        None
    }

    /// `OnDrawSelf`.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.window.draw(vui, r);
        self.hits.clear();
        let (tl, br) = (self.window.top_left(), self.window.bottom_right());
        self.hits.push([tl[0], tl[1], br[0], br[1]], -1);
        for (i, o) in self.options.iter().enumerate() {
            self.window.text(vui, r, o, 40, i as i32 + 1, UI_COLORS[(self.sel == i as i32) as usize], 0);
            self.hits.push(rows_rect(&self.window, vui, r, 40, i as i32 + 1, self.width - 80, 1), i as i32);
        }
        self.window.tick_r(vui, r, 36, self.sel + 1, true);
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let s = wide(&format!("|x{}   |t{}", w(212), w(213)));
        self.window.text(vui, r, &s, 40, self.rows - 1, UI_COLORS[0], 0);
    }
}

/// What the who list asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum WhoUp {
    /// `ClosePanel704`: back to the start menu
    Close,
    /// `Refresh`: the request again
    Refresh,
    /// the popup's Tell: the edit box for a line to `name`
    Tell(String),
    /// Invite: `InviteToGroup` with the row as a `VIInviteRec` (its +0x1C
    /// the id)
    Invite(String, u32),
    /// Mail: the gram window (not ported)
    Mail(String),
}

/// `VIWndWhoList`.
#[derive(Default)]
pub struct WhoList {
    sel: i32,
    top: i32,
    window: Window,
    status: Window,
    options: OptionList,
    flash: KeyFlash,
    ticks: i32,
    dots: i32,
    /// the rows drawn, by index into the list, and when (for a pointer)
    hits: Hits,
    now: i64,
}

impl WhoList {
    /// A pointer at frame point `p` (`pointer`): with the popup up, its
    /// choice taken, or off it the popup closed (△) and the row under the
    /// pointer selected; else a row selected, or the selection's popup
    /// opened (✕). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.options.visible {
            if let Some(k) = self.options.point(p) {
                return k.into_iter().collect();
            }
            let mut keys = Vec::new();
            if let Some(row) = self.hits.at(p).filter(|&row| row != self.sel) {
                self.select(row);
                keys.push(MOVED);
            }
            keys.push(key::TRIANGLE);
            return keys;
        }
        match self.hits.at(p) {
            Some(row) if row == self.sel => vec![key::CROSS],
            Some(row) => {
                self.select(row);
                vec![MOVED]
            }
            None => Vec::new(),
        }
    }

    /// The selection on `row` as the d-pad's move leaves it: the tick's
    /// pulse started over.
    fn select(&mut self, row: i32) {
        self.sel = row;
        self.window.reset_tick_pulse(self.now);
    }
    /// `OnMessage` 295 from outside: the window and the status box set up
    /// and titled (730, 271), the popup's three choices (425, 942, 515)
    /// under 422.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster) {
        let w = |id: i32| wide(vui.words.get(id, vui.language));
        self.window.init(vui, r, [320, 180], 600, 13, 0);
        self.window.set_title(Some(&w(730)));
        self.window.open(true);
        self.status.close(true);
        self.status.init(vui, r, [320, 180], 330, 4, 0);
        self.status.set_title(Some(&w(271)));
        self.options.setup(vui, r, &w(422), vec![w(425), w(942), w(515)], [320, 180], 256);
        self.options.visible = false;
        self.sel = 0;
        self.top = 0;
        self.flash.reset();
    }

    /// `OnKeyDown`/`OnKeyUp` (the popup's while it is up).
    pub fn key(&mut self, vui: &Vui, r: &dyn Raster, who: &Who, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<WhoUp> {
        if self.options.visible {
            if !down {
                return None;
            }
            let (sel, ok) = self.options.key(key, now, cues)?;
            let e = who.list.get(self.sel.max(0) as usize)?;
            let name = e.name.clone();
            let up = match sel {
                0 => WhoUp::Tell(name),
                1 => WhoUp::Invite(name, e.x1c),
                _ => WhoUp::Mail(name),
            };
            return ok.then_some(up);
        }
        if !down {
            match key {
                19 | 138 => self.flash.release(138),
                20 | 139 => self.flash.release(139),
                _ => {}
            }
            return None;
        }
        match key {
            19 | 138 => self.flash.press(138),
            20 | 139 => self.flash.press(139),
            27 | 132 => {
                cues.push(8);
                return Some(WhoUp::Close);
            }
            13 | 134 => {
                // Select
                if (self.sel as usize) < who.list.len() {
                    cues.push(11);
                    self.options.visible = true;
                    self.options.focus(vui, r);
                }
            }
            81 | 113 | 133 => {
                self.sel = 0;
                self.top = 0;
                self.flash.reset();
                return Some(WhoUp::Refresh);
            }
            _ => {}
        }
        None
    }

    /// `OnDrawSelf`: the black band, the window with the header row and ten
    /// rows (the selected one ticked), the legend, "searching..." while the
    /// reply is out; then the popup.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, who: &Who, now: i64, focused: bool, cues: &mut Vec<i32>) {
        let w = |id: i32| wide(vui.words.get(id, vui.language));
        let n = who.list.len() as i32;
        self.now = now;
        self.hits.clear();
        vui.blit_rect(r, 32, [-1, -1], [642, 332], 0, 1.0);
        self.window.draw(vui, r);
        if self.sel > n - 1 {
            self.sel = n - 1;
        }
        self.sel = self.sel.max(0);
        if self.sel - self.top > 9 {
            self.top = self.sel - 9;
        } else if self.sel < self.top {
            self.top = self.sel;
        }
        let t = if n == 10 { 0.0 } else { self.top as f32 / (n - 10) as f32 };
        let width = self.window.size[0];
        self.window.draw_frame_vscroll(vui, r, 30, 1, width - 65, 10, t);
        let x = [50, 300, 400, 500];
        let c0 = UI_COLORS[0];
        self.window.text_box(vui, r, &w(509), x[0], 0, x[1] - x[0], 1, c0, 0);
        self.window.text_box(vui, r, &w(510), x[1], 0, x[2] - x[1], 1, c0, 0);
        self.window.text_box(vui, r, &w(511), x[2], 0, x[3] - x[2], 1, c0, 0);
        self.window.text_box(vui, r, &w(503), x[3], 0, width - x[3] - 40, 1, c0, 0);
        let mut row = 1;
        let mut i = self.top;
        while i < n && row < 11 {
            let e = &who.list[i as usize];
            let c = UI_COLORS[(i == self.sel) as usize];
            const RACE_SHORT: [i32; 10] = [317, 318, 319, 320, 321, 322, 323, 324, 325, 1191];
            let race = if (e.race as usize) < 21 { RACE_SHORT.get(e.race as usize).copied().unwrap_or(-1) } else { 317 };
            let class = [303, 304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 315, 316, 1190][if e.class < 15 { e.class as usize } else { 0 }];
            self.window.text_box(vui, r, &wide(&e.name), x[0], row, x[1] - x[0], 1, c, 2);
            self.window.text_box(vui, r, &w(race), x[1], row, x[2] - x[1], 1, c, 2);
            self.window.text_box(vui, r, &w(class), x[2], row, x[3] - x[2], 1, c, 2);
            self.window.text_box(vui, r, &wide(&e.level.to_string()), x[3], row, width - x[3] - 40, 1, c, 2);
            self.hits.push(rows_rect(&self.window, vui, r, x[0], row, width - x[0] - 40, 1), i);
            if i == self.sel {
                self.window.tick_r(vui, r, 46, row, focused && !self.options.visible);
            }
            i += 1;
            row += 1;
        }
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        let legend = wide(&format!("|x{}   |t{}          |s{}", word(212), word(161), word(811)));
        self.window.text_box(vui, r, &legend, 30, 12, width - 60, 1, c0, 0);
        self.ticks -= 1;
        if self.ticks <= 0 {
            self.ticks = 15;
            self.dots += 1;
            if self.dots == 3 {
                self.dots = 0;
            }
        }
        if who.pending {
            self.status.open(false);
            self.status.draw(vui, r);
            let s = wide(&format!("{}{}", word(943), ".".repeat(self.dots as usize + 1)));
            let sw = self.status.size[0];
            self.status.text_box(vui, r, &s, 40, 1, sw - 40, 1, c0, 0);
            let back = wide(&format!("|t{}", word(161)));
            self.status.text_box(vui, r, &back, 40, 3, sw - 40, 1, c0, 0);
        }
        if self.options.visible {
            self.options.draw(vui, r);
        }
        if self.flash.pulse(138) {
            if self.sel > 0 {
                self.sel -= 1;
                self.window.reset_tick_pulse(now);
                cues.push(11);
            } else {
                cues.push(9);
            }
        }
        if self.flash.pulse(139) {
            if self.sel < n - 1 {
                self.sel += 1;
                self.window.reset_tick_pulse(now);
                cues.push(11);
            } else {
                cues.push(9);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(serial: i64, names: &[&str]) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(1).signed(serial).signed(names.len() as i64);
        for (k, n) in names.iter().enumerate() {
            w.u8(0).u8(7).u8(k as u8 + 1).string(n.as_bytes()).u32(100 + k as u32);
        }
        w.data
    }

    /// `Recv01`: the reply to the request out fills the list; one to an
    /// older request (its serial) is dropped; a reply of another kind too.
    #[test]
    fn a_stale_reply_is_dropped() {
        let mut who = Who::default();
        let req = who.open();
        assert_eq!(&req[..3], &[0x01, 0x0E, 1]);
        assert!(who.pending);
        who.recv(&reply(0, &["Old"]));
        assert!(who.list.is_empty() && who.pending, "serial 0 is the previous request's");
        let mut other = reply(1, &["Nobody"]);
        other[0] = 2;
        who.recv(&other);
        assert!(who.list.is_empty());
        who.recv(&reply(1, &["Rsgroup", "TestBear"]));
        assert!(!who.pending);
        assert_eq!(who.list.iter().map(|e| (e.name.as_str(), e.level, e.x1c)).collect::<Vec<_>>(), vec![("Rsgroup", 1, 100), ("TestBear", 2, 101)]);
    }

    /// A pointer on the who list: a row selected, then its popup opened
    /// (✕); on the popup a choice taken at once; off it the popup closed
    /// (△) and the row under the pointer selected. The header, the rows
    /// past the list and what is off the window are nothing.
    #[test]
    fn a_pointer_selects_a_row_and_opens_its_popup() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        use crate::pointer::{MOVED, key};
        let vui = vui();
        let mut r = NullRaster;
        let mut who = Who::default();
        for k in 0..3 {
            who.list.push(WhoRow { name: format!("P{k}"), ..Default::default() });
        }
        let mut l = WhoList::default();
        l.open(&vui, &r);
        let mut cues = Vec::new();
        l.draw(&vui, &mut r, &who, 0, true, &mut cues);
        // the list's rows are the window's rows 1.., from x 50
        let row = |l: &WhoList, i: i32| rows_rect(&l.window, &vui, &NullRaster, 50, i + 1, 400, 1);
        assert_eq!(l.point(mid(row(&l, 1))), vec![MOVED]);
        assert_eq!(l.sel, 1);
        assert_eq!(l.point(mid(row(&l, 1))), vec![key::CROSS], "the selection: its popup");
        assert!(l.point(mid(row(&l, -1))).is_empty(), "the header");
        assert!(l.point(mid(row(&l, 3))).is_empty(), "past the list");
        assert!(l.point([5.0, 5.0]).is_empty(), "off the window");
        assert_eq!(l.sel, 1);
        // ✕ opens the popup; drawn, its choices are there
        l.key(&vui, &r, &who, 134, true, 0, &mut cues);
        assert!(l.options.visible);
        l.draw(&vui, &mut r, &who, 16, true, &mut cues);
        let choice = mid(rows_rect(&l.options.window, &vui, &NullRaster, 40, 2, 100, 1));
        assert_eq!(l.point(choice), vec![key::CROSS]);
        assert_eq!(l.options.sel, 1, "Invite");
        // the popup's own frame: nothing
        let tl = l.options.window.top_left();
        assert!(l.point([tl[0] as f32 + 4.0, tl[1] as f32 + 4.0]).is_empty());
        // off the popup on row 0, at its left end: closed, row 0 selected
        let r0 = row(&l, 0);
        assert_eq!(l.point([r0[0] as f32 + 10.0, mid(r0)[1]]), vec![MOVED, key::TRIANGLE]);
        assert_eq!(l.sel, 0);
        assert_eq!(l.point([5.0, 5.0]), vec![key::TRIANGLE], "off everything: closed only");
    }

}
