//! The faction standings (CLIENT `viclientpart4f078.cpp`, the client part
//! at VIClient+0x4F078, and `viwndfaction.cpp`, VIWndGame mode 41): a page
//! of factions is asked for with 0x0AC4 (byte 102, a sequence number, the
//! page) and answered by 0x07E2 (byte 119, the sequence, the page, the page
//! count, the list); a reply for an older request is dropped. Each standing
//! is a bar from the middle of its cell, red to the left and blue to the
//! right. A pointer (`pointer`) selects a faction and shows the selected
//! one's standing; a click off that box closes it.

use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::startmenu::KeyFlash;
use net::stream::{Reader, Writer};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// `VIClientPart4F078`.
#[derive(Clone, Debug, Default)]
pub struct Factions {
    seq: i32,
    pub ready: bool,
    /// the page shown and the page count (-1 before a reply)
    pub page: i32,
    pub pages: i32,
    /// `VIRec132_BEB670`: (name, standing)
    pub list: Vec<(Vec<u16>, i32)>,
}

impl Factions {
    /// `Reset`: a new sequence number, the list emptied.
    pub fn reset(&mut self) {
        self.pages = -1;
        self.ready = false;
        self.seq += 1;
        self.list.clear();
    }

    /// `Request(x)`: `Reset`, then 0x0AC4 for page `x`.
    pub fn request(&mut self, x: i32) -> Vec<u8> {
        self.reset();
        let mut w = Writer::op(0x0AC4);
        w.u8(102).signed(self.seq as i64).signed(x as i64);
        w.data
    }

    /// `Recv07E2`: byte 119 is the list, kept only for the last request.
    pub fn recv(&mut self, data: &[u8]) {
        let mut r = Reader::new(data);
        if r.u8() != 119 || r.signed() != self.seq as i64 {
            return;
        }
        let page = r.signed() as i32;
        let pages = r.signed() as i32;
        let n = r.signed();
        // VIArray::Init refuses a negative count (an assert in the game)
        let list: Vec<(Vec<u16>, i32)> = (0..n.clamp(0, 4096)).map(|_| (r.wide(64).encode_utf16().collect(), r.signed() as i32)).collect();
        if r.error {
            return;
        }
        (self.page, self.pages, self.list, self.ready) = (page, pages, list, true);
    }
}

/// What the window asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum FactionUp {
    /// `Show` -> `CloseFaction`: reset, back to the start menu
    Close,
    /// a page asked for
    Send(Vec<u8>),
}

/// `VIWndFaction`.
#[derive(Default)]
pub struct FactionWnd {
    sel: i32,
    /// 0 the list, 1 the selected standing in a box
    mode: i32,
    window: Window,
    wait: Window,
    detail: Window,
    flash: KeyFlash,
    /// the factions drawn, by index, and the standing's box (-1); the
    /// draw's time (a selection's tick pulse starts over then)
    hits: Hits,
    drawn: i64,
}

impl FactionWnd {
    /// A pointer at frame point `p` (`pointer`): with the standing's box up,
    /// a click off it closes it (△); else a faction selected (`MOVED`), and
    /// the selected one's standing shown (✕). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let hit = self.hits.at(p);
        if self.mode == 1 {
            return if hit == Some(-1) { Vec::new() } else { vec![key::TRIANGLE] };
        }
        match hit {
            Some(i) if i == self.sel => vec![key::CROSS],
            Some(i) if i >= 0 => {
                self.sel = i;
                self.window.reset_tick_pulse(self.drawn);
                vec![MOVED]
            }
            _ => Vec::new(),
        }
    }

    /// `OnMessage` 295: the windows, the selection at the top.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.init(vui, r, [320, 180], 600, 15, 0);
        self.window.open(true);
        self.wait.init(vui, r, [320, 180], 300, 3, 0);
        self.wait.close(true);
        self.detail.init(vui, r, [320, 180], 300, 3, 0);
        self.detail.close(true);
        self.sel = 0;
        self.mode = 0;
        self.flash.reset();
    }

    /// `OnKeyDown`/`OnKeyUp`.
    pub fn key(&mut self, part: &mut Factions, key: i32, down: bool, cues: &mut Vec<i32>) -> Option<FactionUp> {
        if !down {
            match key {
                19 | 138 => self.flash.release(138),
                20 | 139 => self.flash.release(139),
                _ => {}
            }
            return None;
        }
        match key {
            27 | 132 => {
                cues.push(8);
                if self.mode == 0 {
                    return Some(FactionUp::Close);
                }
                self.mode = 0;
            }
            // PrevPage, NextPage
            81 | 113 | 133 | 69 | 101 | 135 if self.mode == 0 => {
                let next = matches!(key, 69 | 101 | 135);
                let end = if next { part.pages - 1 } else { 0 };
                if !part.ready || part.page == end {
                    cues.push(9);
                } else {
                    self.sel = 0;
                    cues.push(13);
                    return Some(FactionUp::Send(part.request(part.page + if next { 1 } else { -1 })));
                }
            }
            19 | 138 if self.mode == 0 => self.flash.press(138),
            20 | 139 if self.mode == 0 => self.flash.press(139),
            // Select: the standing in a box
            13 | 134 if self.mode == 0 => {
                if !part.ready || self.sel >= part.list.len() as i32 {
                    cues.push(9);
                } else {
                    cues.push(13);
                    self.mode = 1;
                }
            }
            _ => {}
        }
        None
    }

    /// `DrawStanding(value, x, row, w)`: clamped to +-2000, a bar from the
    /// cell's middle, darker toward the extremes.
    fn draw_standing(&self, vui: &Vui, r: &mut dyn Raster, value: i32, x: i32, row: i32, w: i32) {
        if value == 0 {
            return;
        }
        let value = value.clamp(-2000, 2000);
        let half = w >> 1;
        let len = if value < 0 { value as f32 / -2000.0 * half as f32 } else { value as f32 / 2000.0 * half as f32 } as i32;
        let len = if len == 0 { 1 } else { len };
        let at = if value < 0 { x + half - len } else { x + half };
        let c = match value {
            ..-700 => [0.5, 0.0, 0.0, 1.0],
            -700..-350 => [0.75, 0.0, 0.0, 1.0],
            -350..-150 => [1.0, 0.0, 0.0, 1.0],
            -150..0 => [1.0, 0.25, 0.25, 1.0],
            0..150 => [0.25, 0.25, 1.0, 1.0],
            150..350 => [0.0, 0.0, 1.0, 1.0],
            350..700 => [0.0, 0.0, 0.75, 1.0],
            _ => [0.0, 0.0, 0.5, 1.0],
        };
        self.window.draw_frame(vui, r, x, row, w, 1, 0);
        self.window.draw_color_bar(vui, r, at, row, len, c);
    }

    /// `OnDrawSelf`.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, part: &Factions, now: i64, cues: &mut Vec<i32>) {
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        let c0 = UI_COLORS[0];
        self.drawn = now;
        self.hits.clear();
        vui.blit_rect(r, 32, [-1, -1], [642, 332], 0, 1.0);
        let title = wide("Factions");
        let tw = vui.string_width(r, &title) + 64;
        vui.blit_bar(r, &title, [1.0; 4], [320 - tw / 2, 15], 33, 34, tw, 0, 1.0, 0, true);
        self.window.init(vui, r, [320, 180], 600, 14, 0);
        self.window.open(true);
        self.window.draw(vui, r);
        let width = self.window.size[0];
        let (x, y) = (40, 330);
        self.window.text_box(vui, r, &wide(&word(509)), x, 1, y - x, 1, c0, 0);
        self.window.text_box(vui, r, &wide(&word(1769)), y, 1, width - y - 25, 1, c0, 8);
        if part.ready {
            for (i, (name, value)) in part.list.iter().enumerate().take(10) {
                let row = 2 + i as i32;
                let sel = i as i32 == self.sel;
                self.window.text_box(vui, r, name, x, row, y - x, 1, UI_COLORS[sel as usize], 2);
                if sel {
                    self.window.tick_r(vui, r, x, row, self.mode == 0);
                }
                self.draw_standing(vui, r, *value, y, row, width - y - 25);
                self.hits.push(rows_rect(&self.window, vui, r, x, row, width - x - 25, 1), i as i32);
            }
            if self.mode == 1 {
                if let Some((name, value)) = part.list.get(self.sel as usize) {
                    let mut s = name.clone();
                    s.extend(wide(&format!(": {value}")));
                    let lines = self.detail.count_lines(vui, r, &s, 310);
                    self.detail.init(vui, r, [320, 180], 350, lines + 2, 1);
                    self.detail.open(false);
                    self.detail.draw(vui, r);
                    let (tl, br) = (self.detail.top_left(), self.detail.bottom_right());
                    self.hits.push([tl[0], tl[1], br[0], br[1]], -1);
                    let c = [1.0, 1.0, 1.0, self.detail.open];
                    self.detail.text_box(vui, r, &s, 20, 0, 310, lines, c, 1);
                    self.detail.text_box(vui, r, &wide(&format!("|t{}", word(161))), 20, lines + 1, 350, 1, c, 1);
                }
            } else {
                self.detail.close(true);
            }
        }
        // the legend: the page keys greyed at either end
        let (s, cc, xx, t) = (word(1692), word(216), word(212), word(161));
        let legend = if part.pages < 2 {
            format!("|-|s{s}  |c{cc}  |+|x{xx}  |t{t}")
        } else if part.pages > 0 && part.page == part.pages - 1 {
            format!("|s{s}  |-|c{cc}  |+|x{xx}  |t{t}")
        } else if part.pages > 0 && part.page == 0 {
            format!("|-|s{s}  |+|c{cc}  |x{xx}  |t{t}")
        } else {
            format!("|s{s}  |c{cc}  |x{xx}  |t{t}")
        };
        let y = 390;
        self.window.text_box(vui, r, &wide(&legend), x - 10, 13, y - x, 1, c0, 0);
        if part.ready {
            let page = if part.pages > 0 { part.page + 1 } else { 0 };
            // word 1773, "Page %d of %d"
            let fmt = word(1773);
            let text = fmt.replacen("%d", &page.to_string(), 1).replacen("%d", &part.pages.to_string(), 1);
            self.window.text_box(vui, r, &wide(&text), y, 13, width - y - 25, 1, c0, 16);
            self.wait.close(true);
        } else {
            self.wait.init(vui, r, [320, 180], 300, 3, 0);
            self.wait.open(false);
            self.wait.set_title(Some(&wide(&word(271))));
            self.wait.draw(vui, r);
            self.wait.text_box(vui, r, &wide(&format!("{}...", word(1771))), 40, 1, width - 60, 1, c0, 0);
        }
        if self.flash.pulse(138) {
            // Up
            if !part.ready || self.sel == 0 {
                cues.push(9);
            } else {
                self.sel -= 1;
                cues.push(11);
            }
            self.window.reset_tick_pulse(now);
        }
        if self.flash.pulse(139) {
            // Down
            if !part.ready || self.sel >= part.list.len() as i32 - 1 {
                cues.push(9);
            } else {
                self.sel += 1;
                cues.push(11);
            }
            self.window.reset_tick_pulse(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reply carries the request's sequence number; an older one (a page
    /// asked for before) is dropped, and the request's bytes are zigzags.
    #[test]
    fn only_the_last_request_is_answered() {
        let mut f = Factions::default();
        f.reset();
        let first = f.request(0);
        assert_eq!(first, vec![0xC4, 0x0A, 102, 4, 0], "seq 2 after OpenFaction's Reset");
        let reply = |seq: i64| {
            let mut w = Writer::default();
            w.u8(119).signed(seq).signed(1).signed(34).signed(1).wide("Freeport Militia").signed(-400);
            w.data
        };
        f.request(1);
        f.recv(&reply(2));
        assert!(!f.ready, "an older request's reply");
        f.recv(&reply(3));
        assert!(f.ready);
        assert_eq!((f.page, f.pages, f.list[0].1), (1, 34, -400));
        assert_eq!(String::from_utf16_lossy(&f.list[0].0), "Freeport Militia");
    }

    /// A click selects a faction, a second shows its standing in the box;
    /// a click on the box does nothing, off it closes it (△); no rows
    /// before the page has come.
    #[test]
    fn a_pointer_selects_and_shows_the_standing() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let mut f = Factions::default();
        let mut w = FactionWnd::default();
        w.open(&vui, &r);
        let mut cues = Vec::new();
        w.draw(&vui, &mut r, &f, 0, &mut cues);
        let row = |w: &FactionWnd, row: i32| rows_rect(&w.window, &vui, &NullRaster, 40, row, 535, 1);
        assert!(w.point(mid(row(&w, 3))).is_empty(), "no page yet: no rows");
        f.page = 0;
        f.pages = 1;
        f.list = (0..4).map(|k| (wide(&format!("Faction {k}")), k * 100)).collect();
        f.ready = true;
        w.draw(&vui, &mut r, &f, 0, &mut cues);
        // the list's rows start at row 2
        assert_eq!(w.point(mid(row(&w, 4))), vec![MOVED]);
        assert_eq!(w.sel, 2);
        assert!(w.point(mid(row(&w, 7))).is_empty(), "past the four factions");
        assert_eq!(w.point(mid(row(&w, 4))), vec![key::CROSS]);
        w.key(&mut f, key::CROSS, true, &mut cues);
        assert_eq!(w.mode, 1);
        w.draw(&vui, &mut r, &f, 0, &mut cues);
        let (tl, br) = (w.detail.top_left(), w.detail.bottom_right());
        assert!(w.point(mid([tl[0], tl[1], br[0], br[1]])).is_empty(), "on the box");
        assert_eq!(w.point([20.0, 20.0]), vec![key::TRIANGLE], "off the box: closed");
        assert_eq!(w.sel, 2, "no other faction selected under the box");
    }
}
