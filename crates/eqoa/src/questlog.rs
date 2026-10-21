//! The quest journal (CLIENT `viwndquestlog.cpp`, VIWndGame mode 22) and the
//! quests it lists (`VIClient::m_questTitle`, at most 8: the MemoryDump's,
//! then `Msg007C` adding one and `Msg007D` dropping one). Each quest's title
//! is a text box as many rows as it wraps to, the page scrolled so the
//! selection is whole; X asks (word 574) and a yes sends 0x007D with the
//! quest's index (`Send007D`). A pointer (`pointer`) selects a quest and asks
//! about the selected one; with the box up, its buttons.

use crate::dialog::{BoxAnswer, Key, MBox};
use crate::pointer::{Hits, MOVED, key, rows_rect};
use crate::startmenu::KeyFlash;
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// The quests, as `VIClient` keeps them, with the stamp 0x007C/0x007D carry
/// (`m_p2BB0C`) that `Send007D` echoes.
#[derive(Default)]
pub struct Quests {
    pub titles: Vec<Vec<u16>>,
    pub stamp: u32,
}

impl Quests {
    /// `Msg007C`: a quest, while there are fewer than 8.
    pub fn add(&mut self, title: Vec<u16>) {
        if self.titles.len() < 8 {
            self.titles.push(title);
        }
    }

    /// `Msg007D(i)`: the quest at `i` gone (an index past 7 is ignored; the
    /// count drops even when `i` is past the last, as shipped, though not
    /// below 0 here where the game's would go negative).
    pub fn remove(&mut self, i: i32) {
        if i < 8 {
            if (0..self.titles.len() as i32).contains(&i) {
                self.titles.remove(i as usize);
            } else if !self.titles.is_empty() {
                self.titles.pop();
            }
        }
    }

    /// 0x007C's body: the stamp and a wide title.
    pub fn recv_add(&mut self, data: &[u8]) {
        let mut r = net::stream::Reader::new(data);
        self.stamp = r.u32();
        // ReadString into 128 characters: a longer one reads as empty
        let t = r.wide(128).encode_utf16().collect();
        if !r.error {
            self.add(t);
        }
    }

    /// 0x007D's body: the stamp and a zigzag index.
    pub fn recv_remove(&mut self, data: &[u8]) {
        let mut r = net::stream::Reader::new(data);
        self.stamp = r.u32();
        let i = r.signed() as i32;
        if !r.error {
            self.remove(i);
        }
    }

    /// `Send007D(x)`: the stamp and the index.
    pub fn send_remove(&self, i: i32) -> Vec<u8> {
        let mut w = net::stream::Writer::op(0x007D);
        w.u32(self.stamp).signed(i as i64);
        w.data
    }
}

/// What the journal asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum QuestUp {
    /// `ClosePanel6DC`: back to the start menu
    Close,
    /// the box said yes: 0x007D for the quest
    Drop(i32),
}

/// `VIWndQuestLog`.
pub struct QuestLog {
    sel: i32,
    top: i32,
    active: bool,
    page_end: i32,
    last: i32,
    window: Window,
    keys: KeyFlash,
    mbox: MBox,
    boxed: bool,
    /// each quest drawn, all its rows, by index; and the draw's time (a
    /// selection's tick pulse starts over then, as the keys' does)
    hits: Hits,
    drawn: i64,
}

impl Default for QuestLog {
    fn default() -> Self {
        let mut mbox = MBox::default();
        mbox.pos = [100, 115];
        QuestLog { sel: 0, top: 0, active: true, page_end: 0, last: 0, window: Window::default(), keys: KeyFlash::default(), mbox, boxed: false, hits: Hits::default(), drawn: 0 }
    }
}

impl QuestLog {

    /// A pointer at frame point `p` (`pointer`): with the box up, its
    /// buttons; else a quest selected (`MOVED`), and the selected one asked
    /// about (✕, `Select`). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.boxed {
            return self.mbox.point(p).then_some(key::CROSS).into_iter().collect();
        }
        match self.hits.at(p) {
            Some(i) if i == self.sel => vec![key::CROSS],
            Some(i) => {
                self.sel = i;
                self.window.reset_tick_pulse(self.drawn);
                vec![MOVED]
            }
            None => Vec::new(),
        }
    }
    /// `Open`, with `OnMessage` 295's window: 550 wide, 14 rows, titled
    /// "Quest Journal" (573).
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster) {
        self.sel = 0;
        self.top = 0;
        self.boxed = false;
        self.active = true;
        self.window.init(vui, r, [320, 180], 550, 14, 0);
        self.window.set_title(Some(&wide(vui.words.get(573, vui.language))));
        self.window.open(true);
        self.page_end = 0;
        self.last = 0;
        self.keys.reset();
    }

    /// `ClampSel`: a quest went away; the selection stays on the list.
    fn clamp_sel(&mut self, count: i32) {
        if self.sel >= count {
            self.sel = (self.sel - 1).max(0);
            if self.sel == self.top - 1 {
                self.top = self.sel.max(0);
            }
        }
    }

    fn next(&mut self, count: i32, now: i64, cues: &mut Vec<i32>) {
        self.window.reset_tick_pulse(now);
        if self.sel >= count - 1 {
            self.sel = count - 1;
            cues.push(9);
        } else {
            self.sel += 1;
            cues.push(11);
            if self.sel > self.last {
                self.top += 1;
            }
        }
    }

    fn prev(&mut self, now: i64, cues: &mut Vec<i32>) {
        self.window.reset_tick_pulse(now);
        if self.sel <= 0 {
            self.sel = 0;
            cues.push(9);
        } else {
            self.sel -= 1;
            cues.push(11);
            if self.sel == self.top - 1 {
                self.top = self.sel.max(0);
            }
        }
    }

    /// `OnKeyDown`/`OnKeyUp`, or the box's while it is up (268 drops the
    /// quest it was asked about, 260 just closes it).
    pub fn key(&mut self, vui: &Vui, quests: &Quests, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<QuestUp> {
        if self.boxed {
            if !down {
                return None;
            }
            let k = match key {
                13 | 134 => Key::Enter,
                27 | 132 => Key::Escape,
                136 | 17 => Key::Left,
                137 | 18 => Key::Right,
                _ => return None,
            };
            let answer = self.mbox.key(k, cues)?;
            self.boxed = false;
            self.active = true;
            return match answer {
                BoxAnswer::Yes(p1, _) => Some(QuestUp::Drop(p1)),
                BoxAnswer::No(..) => None,
            };
        }
        if !down {
            match key {
                19 | 138 => self.keys.release(138),
                20 | 139 => self.keys.release(139),
                _ => {}
            }
            return None;
        }
        match key {
            19 | 138 => self.keys.press(138),
            20 | 139 => self.keys.press(139),
            13 | 134 => {
                // Select: ask first
                if !quests.titles.is_empty() {
                    self.mbox.prompt(&wide(vui.words.get(574, vui.language)), 1, self.sel, 0, now);
                    self.boxed = true;
                    self.active = false;
                    self.keys.reset();
                    cues.push(13);
                } else {
                    cues.push(9);
                }
            }
            27 | 132 => {
                cues.push(8);
                return Some(QuestUp::Close);
            }
            _ => {}
        }
        None
    }

    /// `OnDrawSelf`, the box over it.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, quests: &Quests, now: i64, cues: &mut Vec<i32>) {
        let count = quests.titles.len() as i32;
        self.clamp_sel(count);
        self.drawn = now;
        self.hits.clear();
        crate::ui::window_backdrop(vui, r, 641);
        self.window.draw(vui, r);
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        self.window.text(vui, r, &wide(&format!("|x{}   |t{}", w(516), w(161))), 40, 13, UI_COLORS[0], 0);
        let t = if self.top > 0 { (self.top as f32 / (count - self.page_end - 1) as f32).clamp(0.0, 1.0) } else { 0.0 };
        let width = self.window.size[0];
        self.window.draw_frame_vscroll(vui, r, 30, 0, width - 60, 12, t);
        let mut y = 0;
        let mut i = self.top.max(0);
        while i < 8 && i < count {
            let title = &quests.titles[i as usize];
            let mut cut = false;
            let mut rows = self.window.count_lines(vui, r, title, width - 80);
            if y + rows >= 13 {
                cut = true;
                rows = 12 - y;
            }
            let end = y + rows;
            if end < 14 && rows > 0 {
                if i == self.sel {
                    self.window.tick_r(vui, r, 45, y, self.active);
                }
                self.window.text_box(vui, r, title, 45, y, width - 80, rows, UI_COLORS[(i == self.sel) as usize], 0);
                self.hits.push(rows_rect(&self.window, vui, r, 45, y, width - 80, rows), i);
                y = end;
                self.last = i;
                if self.top == 0 {
                    self.page_end = i;
                }
                if i == self.sel && cut {
                    self.top += 1;
                }
            }
            i += 1;
        }
        if self.keys.pulse(138) {
            self.prev(now, cues);
        }
        if self.keys.pulse(139) {
            self.next(count, now, cues);
        }
        if self.boxed {
            self.mbox.draw(vui, r, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    /// `Msg007C`/`Msg007D`: eight at most; a removal past the last still
    /// takes the count down; an index past 7 does nothing.
    #[test]
    fn quests_add_and_remove_as_the_client_does() {
        let mut q = Quests::default();
        for k in 0..10 {
            q.add(w(&format!("q{k}")));
        }
        assert_eq!(q.titles.len(), 8, "the ninth and tenth are dropped");
        q.remove(2);
        assert_eq!(q.titles[2], w("q3"));
        q.remove(9);
        assert_eq!(q.titles.len(), 7, "past 7: ignored");
        q.remove(7);
        assert_eq!(q.titles.len(), 6, "past the last but under 8: the count drops");
        let mut add = 0x1234u32.to_le_bytes().to_vec();
        add.extend(2u32.to_le_bytes());
        add.extend([b'h', 0, b'i', 0]);
        q.recv_add(&add);
        assert_eq!((q.stamp, q.titles.last().unwrap().clone()), (0x1234, w("hi")));
        assert_eq!(q.send_remove(3), vec![0x7D, 0, 0x34, 0x12, 0, 0, 6]);
    }

    /// A click selects a quest (a title wrapped to two rows is two rows
    /// tall), a second asks about it; then the box takes the clicks, the
    /// list under it none; nothing below the last quest.
    #[test]
    fn a_pointer_selects_and_asks() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let mut q = Quests::default();
        q.add(w("short"));
        // eight pixels a character: wider than the 470-pixel box, two rows
        q.add(w(&"long words ".repeat(7)));
        q.add(w("third"));
        let mut l = QuestLog::default();
        l.open(&vui, &r);
        let mut cues = Vec::new();
        l.draw(&vui, &mut r, &q, 0, &mut cues);
        let row = |l: &QuestLog, row: i32| rows_rect(&l.window, &vui, &NullRaster, 45, row, 470, 1);
        assert_eq!(l.point(mid(row(&l, 3))), vec![MOVED], "the third quest, under the two-row title");
        assert_eq!(l.sel, 2);
        assert_eq!(l.point(mid(row(&l, 2))), vec![MOVED], "the long title's second row");
        assert_eq!(l.sel, 1);
        assert_eq!(l.point(mid(row(&l, 1))), vec![key::CROSS], "the selection: asked about");
        assert!(l.point(mid(row(&l, 4))).is_empty(), "below the last quest");
        assert!(l.point([30.0, mid(row(&l, 0))[1]]).is_empty(), "left of the rows");
        // Select opens the box; then a quest row is not the box's
        l.key(&vui, &q, key::CROSS, true, 0, &mut cues);
        assert!(l.boxed);
        l.draw(&vui, &mut r, &q, 0, &mut cues);
        assert!(l.point(mid(row(&l, 0))).is_empty(), "the list under the box");
        assert_eq!(l.sel, 1);
    }
}
