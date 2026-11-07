//! The combat record (the user, 2026-11-07: "lets add a combat record for pvp and
//! duels to the menu for players, has lifetime kills, deaths, etc."): the client's own
//! window, as the game has no record, on a server that keeps them (go-eqoa-server's
//! offer, `net::session::OP_COMBAT_RECORD`). Character > Combat Record
//! (`startmenu::StartData::set_record`) asks for the record (`OP_COMBAT_RECORD_DATA`)
//! and opens this window, which says it is waiting until the record comes, as the
//! faction window does for its page. The record is the server's rows, its words and
//! figures as they stand when it sent them (a heading, or a label and its value), drawn
//! as they come; a count while the window is up comes again and is drawn at once. Laid
//! out as the game's windows are (the title bar, a panel, the legend), and left with △
//! (Escape), back to the start menu. A server that never offers it shows no entry.

use crate::pointer::{Hits, key};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// The start menu's message for Character > Combat Record, the client's own (no
/// CLIENT message has the high byte 0x7E): `Play::do_action` takes it first.
pub const MSG_RECORD: u32 = 0x7E20_0001;

/// What Character > Combat Record says.
pub const ENTRY: &str = "Combat Record";

/// A row of the record: a heading (its `label` alone) or a label and its value.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub heading: bool,
    pub label: String,
    pub value: String,
}

/// The record the server sent: the character's name and the rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Record {
    pub name: String,
    pub rows: Vec<Row>,
}

/// 0x7E21's body: a u8 version (1), the name, a u8 count of rows, then each row: a u8
/// style (0 a heading, 1 a label and its value), its label and its value; a string is
/// a u8 byte count and UTF-8. `None` for another version or a record cut short.
pub fn read(data: &[u8]) -> Option<Record> {
    let mut c = Cursor { data, at: 0 };
    if c.u8()? != 1 {
        return None;
    }
    let name = c.string()?;
    let count = c.u8()? as usize;
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let heading = c.u8()? == 0;
        let label = c.string()?;
        let value = c.string()?;
        rows.push(Row { heading, label, value });
    }
    Some(Record { name, rows })
}

/// A reader over 0x7E21's body.
struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn u8(&mut self) -> Option<u8> {
        let b = *self.data.get(self.at)?;
        self.at += 1;
        Some(b)
    }

    fn string(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        let s = String::from_utf8_lossy(self.data.get(self.at..self.at + n)?).into_owned();
        self.at += n;
        Some(s)
    }
}

/// What the window asks of the game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RecordUp {
    /// back to the start menu
    Close,
}

/// The window: its panel, the wait box until the record comes, and where it drew.
#[derive(Default)]
pub struct RecordWnd {
    window: Window,
    wait: Window,
    hits: Hits,
}

/// The panel's middle and width, the label's and the value's columns.
const CENTER: [i32; 2] = [320, 190];
const WIDTH: i32 = 460;
const LABEL_X: i32 = 30;
const VALUE_X: i32 = 190;

impl RecordWnd {
    /// The window opened: the wait box up until a record comes.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.init(vui, r, CENTER, WIDTH, 3, 0);
        self.window.open(false);
        self.wait.init(vui, r, CENTER, 300, 3, 0);
        self.wait.open(false);
        self.hits.clear();
    }

    /// A key: △ (Escape) closes it, nothing else does anything.
    pub fn key(&mut self, key: i32, down: bool, cues: &mut Vec<i32>) -> Option<RecordUp> {
        if down && matches!(key, 27 | 132) {
            cues.push(8);
            return Some(RecordUp::Close);
        }
        None
    }

    /// A pointer at frame point `p` (`pointer`): a click on the window does nothing,
    /// one off it closes it (△). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.hits.at(p).is_some() { Vec::new() } else { vec![key::TRIANGLE] }
    }

    /// The title bar, then the record (`None`: the wait box), each row a heading in the
    /// selection's colour or a label and its value, and the legend.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, rec: Option<&Record>) {
        let c0 = UI_COLORS[0];
        self.hits.clear();
        crate::ui::window_backdrop(vui, r, 642);
        let title = wide(ENTRY);
        let tw = vui.string_width(r, &title) + 64;
        vui.blit_bar(r, &title, [1.0; 4], [320 - tw / 2, 15], 33, 34, tw, 0, 1.0, 0, true);
        let Some(rec) = rec else {
            self.wait.init(vui, r, CENTER, 300, 3, 0);
            self.wait.open(false);
            self.wait.draw(vui, r);
            self.wait.text_box(vui, r, &wide("Getting your combat record..."), 40, 1, 240, 1, c0, 0);
            let (tl, br) = (self.wait.top_left(), self.wait.bottom_right());
            self.hits.push([tl[0], tl[1], br[0], br[1]], -1);
            return;
        };
        self.wait.close(true);
        let rows = rec.rows.len() as i32;
        self.window.init(vui, r, CENTER, WIDTH, rows + 1, 0);
        self.window.open(false);
        self.window.draw(vui, r);
        let width = self.window.size[0];
        for (i, row) in rec.rows.iter().enumerate() {
            let at = i as i32;
            if row.heading {
                self.window.text_box(vui, r, &wide(&row.label), LABEL_X - 10, at, width - 2 * LABEL_X, 1, UI_COLORS[1], 0);
                continue;
            }
            self.window.text_box(vui, r, &wide(&row.label), LABEL_X, at, VALUE_X - LABEL_X - 10, 1, c0, 2);
            self.window.text_box(vui, r, &wide(&row.value), VALUE_X, at, width - VALUE_X - 25, 1, c0, 0x10 | 2);
        }
        let exit = vui.words.get(161, vui.language).to_string();
        self.window.text_box(vui, r, &wide(&format!("|t{exit}")), LABEL_X - 10, rows, width - 2 * LABEL_X, 1, c0, 0);
        let (tl, br) = (self.window.top_left(), self.window.bottom_right());
        self.hits.push([tl[0], tl[1], br[0], br[1]], -1);
    }

    /// The window's rows (tests).
    #[cfg(test)]
    pub fn rows(&self) -> i32 {
        self.window.rows
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// 0x7E21's body as go-eqoa-server writes it.
    pub(crate) fn body(name: &str, rows: &[(u8, &str, &str)]) -> Vec<u8> {
        let s = |b: &mut Vec<u8>, t: &str| {
            b.push(t.len() as u8);
            b.extend(t.as_bytes());
        };
        let mut b = vec![1];
        s(&mut b, name);
        b.push(rows.len() as u8);
        for (style, label, value) in rows {
            b.push(*style);
            s(&mut b, label);
            s(&mut b, value);
        }
        b
    }

    /// The record read back: its name and rows, headings told from lines; another
    /// version and every record cut short are none.
    #[test]
    fn the_record_is_read_as_the_server_writes_it() {
        let b = body("Dabran", &[(0, "PvP", ""), (1, "Kills", "12"), (1, "Last kill", "Bob, 2 hours ago"), (0, "Duels", ""), (1, "Won", "5")]);
        let rec = read(&b).unwrap();
        assert_eq!(rec.name, "Dabran");
        assert_eq!(rec.rows.len(), 5);
        assert!(rec.rows[0].heading && !rec.rows[1].heading);
        assert_eq!((rec.rows[2].label.as_str(), rec.rows[2].value.as_str()), ("Last kill", "Bob, 2 hours ago"));
        for n in 0..b.len() {
            assert_eq!(read(&b[..n]), None, "cut at {n}");
        }
        let mut other = b.clone();
        other[0] = 2;
        assert_eq!(read(&other), None, "another version");
        assert_eq!(read(&body("Alpha", &[])).map(|r| r.rows.len()), Some(0));
    }

    /// The window: the wait box before the record comes, then a row a row of the
    /// record and the legend's; △ and Escape close it, nothing else; a click on the
    /// window does nothing and one off it closes it, which the window's own note of
    /// where it drew decides (none before it drew: any click closes).
    #[test]
    fn the_window_draws_its_rows_and_closes() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let mut w = RecordWnd::default();
        w.open(&vui, &r);
        assert_eq!(w.point([320.0, 190.0]), vec![key::TRIANGLE], "not drawn yet");
        w.draw(&vui, &mut r, None);
        assert!(w.point([320.0, 190.0]).is_empty(), "on the wait box");
        let rec = read(&body("Dabran", &[(0, "PvP", ""), (1, "Kills", "12"), (0, "Duels", ""), (1, "Won", "5")])).unwrap();
        w.draw(&vui, &mut r, Some(&rec));
        assert_eq!(w.rows(), 5, "four rows and the legend");
        let (tl, br) = (w.window.top_left(), w.window.bottom_right());
        assert!(w.point(mid([tl[0], tl[1], br[0], br[1]])).is_empty(), "on the window");
        assert_eq!(w.point([2.0, 2.0]), vec![key::TRIANGLE], "off it");
        let mut cues = Vec::new();
        for k in [134, 13, 138, 139, 133] {
            assert_eq!(w.key(k, true, &mut cues), None, "key {k}");
        }
        assert_eq!(w.key(132, false, &mut cues), None, "a release");
        assert_eq!(w.key(132, true, &mut cues), Some(RecordUp::Close));
        assert_eq!(w.key(27, true, &mut cues), Some(RecordUp::Close));
        assert_eq!(cues, [8, 8]);
    }
}
