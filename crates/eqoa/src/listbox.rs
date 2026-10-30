//! `VIUIListBox`, a SUPPORT class only the live client has (live SUPPORT
//! 0x00478580..0x00478E78 in the Corsten state, named in its export table): a
//! framed list of rows of up to four columns over a `VIUIWindow`, scrolled to
//! keep its selection in view. The live group search lists its players in
//! one (`groupsearch`); its Class Mastery windows were rebuilt on it too.

use vi::ui::{Raster, UI_COLORS, Vui, Window};

/// `LINEDATA` (36 bytes): the columns' x (from the box's) and texts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub cols: Vec<(i32, Vec<u16>)>,
}

#[derive(Clone, Debug, Default)]
pub struct ListBox {
    /// `+4` the width, `+8` the rows shown
    pub width: i32,
    pub rows: i32,
    /// `+12` the first row shown, `+16` the selection (-1 none)
    pub top: i32,
    pub sel: i32,
    /// `+20`: the box has the keys (its selection drawn and ticked)
    pub active: bool,
    /// `+24`: the tick pulses
    pub pulse: bool,
    /// `+28`
    pub lines: Vec<Line>,
}

impl ListBox {
    /// `Init(ui, width, rows)`: emptied (`Clear`), active, pulsing.
    pub fn init(&mut self, width: i32, rows: i32) {
        self.clear();
        (self.width, self.rows, self.active, self.pulse) = (width, rows, true, true);
    }

    /// `Clear` / `ResetContent`: no rows, the top 0, no selection.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.top = 0;
        self.sel = -1;
    }

    /// `AddString`: a row of one column at the box's left; its index.
    pub fn add_string(&mut self, s: &[u16]) -> usize {
        self.lines.push(Line { cols: vec![(0, s.to_vec())] });
        self.lines.len() - 1
    }

    /// `SetColumns(line, n, xs)`: the row's first `n` columns at `xs` (a
    /// column past its texts has none).
    pub fn set_columns(&mut self, line: usize, xs: &[i32]) {
        if let Some(l) = self.lines.get_mut(line) {
            let mut cols: Vec<(i32, Vec<u16>)> = xs.iter().map(|&x| (x, Vec::new())).collect();
            for (k, (_, s)) in std::mem::take(&mut l.cols).into_iter().enumerate() {
                if let Some(c) = cols.get_mut(k) {
                    c.1 = s;
                }
            }
            l.cols = cols;
        }
    }

    /// `SetColumnString(line, col, s)`.
    pub fn set_column_string(&mut self, line: usize, col: usize, s: &[u16]) {
        if let Some(c) = self.lines.get_mut(line).and_then(|l| l.cols.get_mut(col)) {
            c.1 = s.to_vec();
        }
    }

    pub fn count(&self) -> i32 {
        self.lines.len() as i32
    }

    /// `SetCurSel(n)`: held to the rows (-1 with none).
    pub fn set_cur_sel(&mut self, n: i32) {
        self.sel = n.max(0);
        if self.sel > self.count() - 1 {
            self.sel = self.count() - 1;
        }
    }

    /// `Draw(window, x, row)`: the selection brought into view, the frame
    /// (with a scroll bar while the rows overflow or are scrolled), a steady
    /// tick on the first row when active and empty, then each row shown, its
    /// columns' texts cut to their widths (the last to the box's edge) in the
    /// text's colour, the selection's when it is the selected row of an
    /// active box, dim when the box is not active.
    pub fn draw(&mut self, w: &Window, vui: &Vui, r: &mut dyn Raster, x: i32, row: i32) {
        if self.sel != -1 {
            if self.sel < self.top {
                self.top = self.sel;
            }
            if self.sel >= self.top + self.rows {
                self.top = self.sel - self.rows + 1;
            }
        }
        let n = self.count();
        if n > self.rows || self.top > 0 {
            w.draw_frame_vscroll(vui, r, x, row, self.width, self.rows, self.top as f32 / (n - self.rows) as f32);
        } else {
            w.draw_frame(vui, r, x, row, self.width, self.rows, 0);
        }
        if self.active && n == 0 {
            w.tick_r(vui, r, x, row, false);
        }
        let mut i = 0;
        while i < self.rows && self.top + i < n {
            let k = self.top + i;
            let selected = self.sel != -1 && k == self.sel;
            if selected && self.active {
                w.tick_r(vui, r, x, row + i, self.pulse);
            }
            let line = &self.lines[k as usize];
            for (c, (cx, s)) in line.cols.iter().enumerate() {
                let end = line.cols.get(c + 1).map_or(self.width, |next| next.0);
                let color = if self.active { UI_COLORS[selected as usize] } else { UI_COLORS[2] };
                w.text_box(vui, r, s, x + cx, row + i, end - cx, 1, color, 2);
            }
            i += 1;
        }
    }

    /// The rows shown: (row index, its window row from `row`).
    pub fn shown(&self, row: i32) -> impl Iterator<Item = (i32, i32)> + '_ {
        (0..self.rows).take_while(move |&i| self.top + i < self.count()).map(move |i| (self.top + i, row + i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SetCurSel` holds the selection to the rows, -1 when there are none;
    /// `SetColumns` keeps the first column's text, and the draw brings the
    /// selection into view (the control: a selection already shown leaves
    /// the top alone).
    #[test]
    fn the_selection_is_held_and_kept_in_view() {
        let mut b = ListBox::default();
        b.init(395, 10);
        b.set_cur_sel(0);
        assert_eq!(b.sel, -1, "no rows: none");
        for k in 0..14 {
            let i = b.add_string(&vi::ui::wide(&format!("p{k}")));
            b.set_columns(i, &[0, 195, 265, 335]);
            b.set_column_string(i, 3, &vi::ui::wide("60"));
        }
        assert_eq!(b.lines[3].cols[0].1, vi::ui::wide("p3"));
        assert_eq!((b.lines[3].cols[3].0, &b.lines[3].cols[3].1[..]), (335, &vi::ui::wide("60")[..]));
        b.set_cur_sel(99);
        assert_eq!(b.sel, 13);
        b.set_cur_sel(-5);
        assert_eq!(b.sel, 0);
        let vui = crate::pointer::testing::vui();
        let mut r = crate::pointer::testing::NullRaster;
        let w = Window::default();
        b.sel = 12;
        b.draw(&w, &vui, &mut r, 170, 1);
        assert_eq!(b.top, 3, "row 12 the last of ten");
        b.sel = 5;
        b.draw(&w, &vui, &mut r, 170, 1);
        assert_eq!(b.top, 3, "row 5 already shown");
        b.sel = 1;
        b.draw(&w, &vui, &mut r, 170, 1);
        assert_eq!(b.top, 1);
        assert_eq!(b.shown(1).next(), Some((1, 1)));
        assert_eq!(b.shown(1).count(), 10);
    }
}
