//! `VIWndBuffPC` (CLIENT `viwndbuffpc.cpp`): the GM's Buff PC picker,
//! `VIWndGame`'s mode 27 (TEST > Avatar > Target Action > Buff PC,
//! `OpenPanel27`). Two digits under word 654: left and right pick one, up
//! and down change it (the tens 0..7, the units 0..9, or 0..5 when the tens
//! is 7); Enter hands the number to `ShowA110` (the 0x0701 sub-op 2 with the
//! target) and closes, Triangle closes. It fades in and the chosen digit's
//! arrows pulse, on the client's timer (`m_p07568`, VITimer's 983.04 ticks
//! a second, read as thousandths). `Init` runs once, with the game window:
//! the digits and the chosen one are kept from one opening to the next.
//!
//! A pointer (`pointer`): on a digit chooses it, above or below one changes
//! it (up, down); the legend's glyphs are their buttons (the raster notes
//! them).

use crate::pointer::{Hits, MOVED, key};
use vi::fpu::{Ee, F, vimax, vimin};
use vi::ui::{FrameParam, Raster, Vui, wide};

/// What the picker tells the game window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BuffUp {
    /// Enter: `ShowA110(Value())`, then 260
    Value(i32),
    /// 260
    Close,
}

pub struct BuffPc {
    /// `m_sel`: the digit, 0 the tens
    pub sel: i32,
    /// `m_p28`: the digits shown, 2
    p28: i32,
    /// `m_pos`: nothing sets it (`SetPos` has no caller), so the frame's
    /// corner stays where the window was allocated: (0, 0)
    pub pos: [i32; 2],
    /// `m_time`: the timer at the last draw or key
    time: i64,
    /// `m_p40`: the window's fade
    p40: F<Ee>,
    /// `m_p44`: the chosen digit's arrows
    p44: F<Ee>,
    /// `m_p48`: their direction
    p48: i32,
    /// `m_digits`: tens, units
    pub digits: [i32; 2],
    pub visible: bool,
    /// each digit drawn (0, 1), the room above it (10 + digit), below it
    /// (20 + digit)
    hits: Hits,
}

impl Default for BuffPc {
    /// The constructor, then `Init` (`VIWndGame`'s).
    fn default() -> Self {
        BuffPc { sel: 0, p28: 2, pos: [0, 0], time: 0, p40: F::ONE, p44: F::ONE, p48: 1, digits: [0, 0], visible: false, hits: Hits::default() }
    }
}

impl BuffPc {
    /// `OnMessage` 295: the focus, as `UpdateMode` makes it the active
    /// window: faded from a quarter, the arrows at full.
    pub fn focus(&mut self, now: i64) {
        self.p40 = F::new(0.25);
        self.p44 = F::ONE;
        self.p48 = 1;
        self.time = now;
        self.visible = true;
    }

    /// `OnKeyDown` (sounds to `cues`).
    pub fn key(&mut self, key: i32, now: i64, cues: &mut Vec<i32>) -> Option<BuffUp> {
        match key {
            27 | 132 => {
                cues.push(8);
                Some(BuffUp::Close)
            }
            13 | 134 => Some(BuffUp::Value(self.value())),
            19 | 138 => {
                self.adjust_digit(1, now, cues);
                None
            }
            20 | 139 => {
                self.adjust_digit(-1, now, cues);
                None
            }
            17 | 136 => {
                self.move_sel(-1, now, cues);
                None
            }
            18 | 137 => {
                self.move_sel(1, now, cues);
                None
            }
            _ => None,
        }
    }

    /// `MoveSel(delta)`: the digit, stopped at either end (sound 9).
    fn move_sel(&mut self, delta: i32, now: i64, cues: &mut Vec<i32>) {
        if self.sel + delta < 2 - self.p28 {
            cues.push(9);
            self.sel = 2 - self.p28;
        } else if self.sel + delta >= 2 {
            cues.push(9);
            self.sel = 1;
        } else {
            self.sel += delta;
            cues.push(11);
        }
        self.p48 = 1;
        self.p44 = F::ONE;
        self.time = now;
    }

    /// `AdjustDigit(delta)`: the tens wraps 0..7; the units 0..5 while the
    /// tens is 7, else 0..9 (a tens moved to 7 leaves the units as they
    /// are, as shipped).
    fn adjust_digit(&mut self, delta: i32, now: i64, cues: &mut Vec<i32>) {
        let s = self.sel as usize;
        self.digits[s] = self.digits[s].wrapping_add(delta);
        if self.sel == 0 {
            if self.digits[0] >= 8 {
                self.digits[0] = 0;
            }
            if self.digits[s] < 0 {
                self.digits[s] = 7;
            }
        } else if self.digits[0] == 7 {
            if self.digits[1] >= 6 {
                self.digits[1] = 0;
            }
            if self.digits[1] < 0 {
                self.digits[1] = 5;
            }
        } else {
            if self.digits[1] >= 10 {
                self.digits[1] = 0;
            }
            if self.digits[1] < 0 {
                self.digits[1] = 9;
            }
        }
        cues.push(11);
        self.p48 = 1;
        self.p44 = F::ONE;
        self.time = now;
    }

    /// `Value`: tens * 10 + units.
    pub fn value(&self) -> i32 {
        self.digits[0].wrapping_mul(10).wrapping_add(self.digits[1])
    }

    /// A pointer at frame point `p`: the digit under it chosen, and above or
    /// below one changed (up, down). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let Some(h) = self.hits.at(p) else { return Vec::new() };
        let mut keys = Vec::new();
        if h % 10 != self.sel {
            self.sel = h % 10;
            keys.push(MOVED);
        }
        match h / 10 {
            1 => keys.push(key::UP),
            2 => keys.push(key::DOWN),
            _ => {}
        }
        keys
    }

    /// `OnDrawSelf` at the timer `now`: the frame, the title, the two digits
    /// (the chosen one yellow between its arrows), the legend; then the fade
    /// and the arrows' pulse step on.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, now: i64) {
        let pos = self.pos;
        let size = [290, 180];
        let mut org = [pos[0] + 15, pos[1] + 15];
        let dt = F::<Ee>::new((now - self.time) as f32) * F::new(0.001);
        self.time = now;
        let a = self.p40.0;
        let white = [1.0, 1.0, 1.0, a];
        let yellow = [1.0, 1.0, 0.0, a];
        let frame = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 0), br: (106, 0), bottom: (102, 0), bl: (105, 0), left: (104, 0) };
        vui.stretch_blit_ex(r, 61, pos, size, 3, vimin(self.p40, F::new(0.75)).0);
        vui.blit_frame(r, pos, size, &frame, 3, a, true);
        org[1] += 19;
        let w = vui.string_width(r, &wide("W")) - 6;
        let mut pen = org;
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        r.draw_string(pen, white, &word(654));
        pen[0] = pen[0] + 40 + vui.string_width(r, &word(654));
        self.hits.clear();
        let h = vui.font_height(r);
        for i in 2 - self.p28..2 {
            let chosen = i == self.sel;
            r.draw_string(pen, if chosen { yellow } else { white }, &wide(&self.digits[i as usize].to_string()));
            // the digit's cell, the arrows' rooms above and below it
            self.hits.push([pen[0], pen[1] - 14, pen[0] + w, pen[1]], 10 + i);
            self.hits.push([pen[0], pen[1] + h - 4, pen[0] + w, pen[1] + 38], 20 + i);
            self.hits.push([pen[0], pen[1], pen[0] + w, pen[1] + h - 4], i);
            if chosen {
                let at = [pen[0] - 2, pen[1] - 14];
                let arrows = vimin(self.p44, self.p40).0;
                vui.blit_ex(r, 87, at, 3, arrows, 0);
                vui.blit_ex(r, 90, [at[0], at[1] + 38], 3, arrows, 0);
            }
            pen[0] += w;
        }
        vui.blit_ex(r, 70, [pos[0] + 25, pos[1] + 150], 3, a, 0);
        r.draw_string([pos[0] + 50, pos[1] + 146], white, &word(214));
        vui.blit_ex(r, 69, [pos[0] + 125, pos[1] + 150], 3, a, 0);
        r.draw_string([pos[0] + 150, pos[1] + 146], white, &word(217));
        self.p40 = vimin(self.p40 + dt * F::new(5.0), F::ONE);
        if self.p48 == -1 {
            self.p44 = vimax(self.p44 - dt * F::new(2.0), F::ZERO);
            if self.p44 == F::ZERO {
                self.p48 = 1;
            }
        } else {
            self.p44 = vimin(self.p44 + dt * F::new(3.0), F::ONE);
            if self.p44 == F::ONE {
                self.p48 = -1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tens wrap 0..7, the units 0..9 or 0..5 under 70; left and right
    /// stop at the ends with sound 9; Enter gives the value, Triangle closes
    /// with sound 8.
    #[test]
    fn digits_wrap_as_the_game_does() {
        let mut b = BuffPc::default();
        let mut cues = Vec::new();
        b.key(138, 0, &mut cues);
        assert_eq!((b.digits, cues[0]), ([1, 0], 11));
        b.key(139, 0, &mut cues);
        b.key(139, 0, &mut cues);
        assert_eq!(b.digits, [7, 0], "the tens below 0 is 7");
        b.key(137, 0, &mut cues);
        assert_eq!(b.sel, 1);
        b.key(139, 0, &mut cues);
        assert_eq!(b.digits, [7, 5], "under 70 the units wrap to 5");
        b.key(138, 0, &mut cues);
        assert_eq!(b.digits, [7, 0], "and past 5 to 0");
        b.key(136, 0, &mut cues);
        b.key(138, 0, &mut cues);
        b.key(137, 0, &mut cues);
        b.key(139, 0, &mut cues);
        assert_eq!(b.digits, [0, 9], "the tens 0: the units wrap to 9");
        cues.clear();
        b.key(137, 0, &mut cues);
        assert_eq!((b.sel, cues[0]), (1, 9), "no further right");
        b.key(136, 0, &mut cues);
        b.key(136, 0, &mut cues);
        assert_eq!((b.sel, cues[2]), (0, 9), "no further left");
        // a tens moved to 7 leaves the units: 79, as shipped
        b.key(137, 0, &mut cues);
        b.key(136, 0, &mut cues);
        b.key(139, 0, &mut cues);
        assert_eq!((b.digits, b.value()), ([7, 9], 79));
        assert_eq!(b.key(134, 0, &mut cues), Some(BuffUp::Value(79)));
        cues.clear();
        assert_eq!(b.key(132, 0, &mut cues), Some(BuffUp::Close));
        assert_eq!(cues, vec![8]);
        // the control: a key it has no case for does nothing
        assert_eq!(b.key(21, 0, &mut cues), None);
        assert_eq!(b.digits, [7, 9]);
    }

    /// Through `pointer::testing`'s null raster: a digit chosen by the
    /// pointer, above it up and below it down; nothing beside them. Without
    /// a draw there are no hit notes, and the pointer does nothing.
    #[test]
    fn a_pointer_chooses_and_changes_a_digit() {
        use crate::pointer::testing::{NullRaster, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let mut b = BuffPc::default();
        b.focus(0);
        assert!(b.point([100.0, 40.0]).is_empty(), "not drawn yet: no hit notes");
        b.draw(&vui, &mut r, 0);
        // with no words word 654 is "(null)", 6 characters of 8: the digits
        // from 15 + 40 + 48, 34, each "W" less 6 wide
        let (x0, w) = (15.0 + 40.0 + 48.0, 8.0 - 6.0);
        let units = [x0 + w + 0.5, 34.0 + 2.0];
        assert_eq!(b.point(units), vec![MOVED]);
        assert_eq!(b.sel, 1);
        assert_eq!(b.point(units), Vec::<i32>::new(), "already chosen");
        assert_eq!(b.point([units[0], 34.0 - 10.0]), vec![key::UP]);
        assert_eq!(b.point([units[0], 34.0 + 20.0]), vec![key::DOWN]);
        assert_eq!(b.point([x0 + 0.5, 36.0]), vec![MOVED], "the tens");
        assert_eq!(b.sel, 0);
        assert!(b.point([250.0, 100.0]).is_empty(), "the frame beside the digits");
    }

    /// The fade and the pulse on the timer: a quarter at the focus, up 5 a
    /// second to 1; the arrows down 2 a second to 0 and back up 3.
    #[test]
    fn it_fades_in_and_pulses_on_the_timer() {
        use crate::pointer::testing::{NullRaster, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let mut b = BuffPc::default();
        b.focus(1000);
        b.draw(&vui, &mut r, 1100);
        assert_eq!(b.p40, F::new(0.25) + F::new(100.0) * F::new(0.001) * F::new(5.0));
        assert_eq!((b.p44, b.p48), (F::ONE, -1), "the arrows were at 1: they turn");
        b.draw(&vui, &mut r, 1300);
        assert_eq!(b.p40, F::ONE);
        assert_eq!(b.p44, F::ONE - F::new(200.0) * F::new(0.001) * F::new(2.0));
        b.draw(&vui, &mut r, 3000);
        assert_eq!((b.p44, b.p48), (F::ZERO, 1));
        // the control: with no time passed nothing moves
        let before = (b.p40, b.p44);
        b.draw(&vui, &mut r, 3000);
        assert_eq!((b.p40, b.p44), (before.0, F::ZERO + F::ZERO * F::new(3.0)));
    }
}
