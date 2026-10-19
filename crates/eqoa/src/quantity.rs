//! `VIWndQuantity` (CLIENT `viwndquantity.cpp`): the quantity picker the
//! item windows open. Up to four digits under "Quantity" (word 353), the
//! one under the cursor changed with up and down and moved with left and
//! right (held keys repeating), the total when a unit price is given (word
//! 286), and the legend. Enter hands the value up (293), at most the
//! maximum; Triangle cancels (260).

use crate::startmenu::KeyFlash;
use vi::ui::{Raster, Vui, Window, wide};

/// What the picker tells its parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum QuantityUp {
    /// 293: the value (0 when none) and the id it was opened with
    Value(i32, i32),
    /// 260
    Cancel,
}

#[derive(Default)]
pub struct Quantity {
    pub cursor: i32,
    pub max: i32,
    pub id: i32,
    /// -1: no price row
    pub price: i32,
    pub digits: [i32; 4],
    pub shown: i32,
    window: Window,
    pub keys: KeyFlash,
    pub visible: bool,
}

impl Quantity {
    /// `Open(max, id, price)`, and the focus (295): on the units digit set
    /// to 1, as many digits as the maximum needs.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, max: i32, id: i32, price: i32) {
        self.max = max;
        self.id = id;
        self.price = price;
        self.cursor = 3;
        self.digits = [0, 0, 0, 1];
        self.shown = 1 + (1..4).filter(|&i| max / 10i32.pow(i) > 0).count() as i32;
        self.visible = true;
        self.window.init(vui, r, [320, 224], 300, 6, 1);
        self.window.close(true);
        self.window.open(false);
        self.keys.reset();
    }

    /// `GetValue`.
    pub fn value(&self) -> i32 {
        (0..4).map(|i| self.digits[i] * 10i32.pow(3 - i as u32)).sum()
    }

    /// `SetMax`.
    pub fn set_max(&mut self) {
        let mut v = self.max;
        for i in (0..4).rev() {
            let p = 10i32.pow(i);
            self.digits[3 - i as usize] = v / p;
            v -= self.digits[3 - i as usize] * p;
        }
    }

    /// `OnKeyDown`/`OnKeyUp`: Enter and Triangle answer, the pad keys only
    /// start their repeat (the draw acts on them).
    pub fn key(&mut self, key: i32, down: bool, cues: &mut Vec<i32>) -> Option<QuantityUp> {
        let k = match key {
            17 | 136 => 136,
            18 | 137 => 137,
            19 | 138 => 138,
            20 | 139 => 139,
            _ => 0,
        };
        if !down {
            if k != 0 {
                self.keys.release(k);
            }
            return None;
        }
        match key {
            13 | 134 => Some(QuantityUp::Value(self.value().max(0).min(self.max), self.id)),
            27 | 132 => {
                cues.push(8);
                Some(QuantityUp::Cancel)
            }
            _ => {
                if k != 0 {
                    self.keys.press(k);
                }
                None
            }
        }
    }

    /// `MoveCursor(d)`.
    fn move_cursor(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        self.cursor += d;
        if self.cursor < 4 - self.shown {
            cues.push(9);
            self.cursor = 4 - self.shown;
        } else if self.cursor >= 4 {
            cues.push(9);
            self.cursor = 3;
        } else {
            cues.push(11);
        }
        self.window.reset_tick_pulse(now);
    }

    /// `ChangeDigit(d)`: a digit wraps 0..9; past the maximum the digits
    /// snap to it.
    fn change_digit(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        let c = self.cursor as usize;
        self.digits[c] = (self.digits[c] + d).rem_euclid(10);
        if self.max < self.value() {
            cues.push(9);
            self.set_max();
        } else {
            cues.push(11);
        }
        self.window.reset_tick_pulse(now);
    }

    /// `OnDrawSelf`, then the repeating pad keys.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, cues: &mut Vec<i32>) {
        let rows = if self.price >= 0 { 6 } else { 5 };
        self.window.init(vui, r, [320, 180], 270, rows, 1);
        self.window.draw(vui, r);
        let (white, yellow) = ([1.0, 1.0, 1.0, self.window.open], [1.0, 1.0, 0.0, self.window.open]);
        let w = vui.string_width(r, &wide("W")) - 6;
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        self.window.text_box(vui, r, &wide(&word(353)), 20, 1, 80, 1, white, 3);
        let mut x = 100;
        for i in 4 - self.shown..4 {
            let lit = i == self.cursor;
            self.window.text_box(vui, r, &wide(&self.digits[i as usize].to_string()), x, 1, w, 1, if lit { yellow } else { white }, 0);
            if lit {
                self.window.tick_d(vui, r, x + w / 2, 0, true);
                self.window.tick_u(vui, r, x + w / 2, 2, true);
            }
            x += w;
        }
        let width = self.window.size[0];
        if self.price >= 0 {
            let total = self.price * self.value();
            self.window.text_box(vui, r, &wide(&word(286)), 20, 3, 80, 1, white, 3);
            self.window.text_box(vui, r, &wide(&total.to_string()), 100, 3, width - 100 - 20, 1, white, 1);
        }
        let s = format!("|x{}   |t{}", word(214), word(213));
        self.window.text_box(vui, r, &wide(&s), 20, rows - 1, width - 40, 1, white, 1);
        let now = vui.time;
        if self.keys.pulse(136) {
            self.move_cursor(-1, now, cues);
        }
        if self.keys.pulse(137) {
            self.move_cursor(1, now, cues);
        }
        if self.keys.pulse(138) {
            self.change_digit(1, now, cues);
        }
        if self.keys.pulse(139) {
            self.change_digit(-1, now, cues);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_value_and_max() {
        let mut q = Quantity { max: 250, ..Default::default() };
        q.digits = [0, 3, 1, 0];
        assert_eq!(q.value(), 310);
        q.set_max();
        assert_eq!((q.digits, q.value()), ([0, 2, 5, 0], 250));
        // Enter clamps to the maximum; a cancel says so with sound 8
        q.digits = [0, 9, 9, 9];
        let mut cues = Vec::new();
        assert_eq!(q.key(134, true, &mut cues), Some(QuantityUp::Value(250, 0)));
        assert_eq!(q.key(27, true, &mut cues), Some(QuantityUp::Cancel));
        assert_eq!(cues, vec![8]);
        assert_eq!(q.key(138, true, &mut cues), None, "a pad key only starts its repeat");
    }

    #[test]
    fn a_digit_past_the_maximum_snaps_to_it() {
        let mut q = Quantity { max: 12, cursor: 2, shown: 2, digits: [0, 0, 0, 1], ..Default::default() };
        let mut cues = Vec::new();
        q.change_digit(1, 0, &mut cues);
        assert_eq!((q.value(), cues[0]), (11, 11), "11 is within 12");
        q.change_digit(1, 0, &mut cues);
        assert_eq!((q.value(), cues[1]), (12, 9), "21 is past 12: snapped to it");
        q.move_cursor(-1, 0, &mut cues);
        assert_eq!((q.cursor, *cues.last().unwrap()), (2, 9), "two digits shown: no further left");
    }
}
