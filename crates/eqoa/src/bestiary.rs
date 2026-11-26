//! The bestiary (Heirloom petition 3192: "some type of bestiary that shows mobs you
//! encounter and the zones they are from"): the client's own window, as the game has
//! none, on a server that keeps one (go-eqoa-server's offer, `net::session::OP_BESTIARY`).
//! Character > Bestiary (`startmenu::StartData::set_bestiary`) asks for its first page
//! (`OP_BESTIARY_DATA`) and opens this window, which says it is waiting until the page
//! comes. A page is the server's rows: a zone's heading, then each creature met there
//! with what is known of it ("12 killed, first Nov 3"). ← and → (the d-pad's, the
//! keyboard's, a click on the page line's ends) turn the pages, each asked for as it
//! is turned. Laid out as the combat record's window (`record`), and left with △
//! (Escape), back to the start menu. A server that never offers it shows no entry.

use crate::pointer::{Hits, key};
use crate::record::Row;
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// The start menu's message for Character > Bestiary, the client's own (no CLIENT
/// message has the high byte 0x7E): `Play::do_action` takes it first.
pub const MSG_BESTIARY: u32 = 0x7EF4_0001;

/// What Character > Bestiary says.
pub const ENTRY: &str = "Bestiary";

/// A page the server sent: which, of how many, the kinds met in all, and its rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub page: u16,
    pub pages: u16,
    pub kinds: u16,
    pub rows: Vec<Row>,
}

/// 0x7EF5's body: a u8 version (1), a u16 page, a u16 count of pages, a u16 of the
/// kinds met, a u8 count of rows, then each row: a u8 style (0 a zone's heading, 1 a
/// kind), its label and its value; a string is a u8 byte count and UTF-8. `None` for
/// another version or a page cut short.
pub fn read(data: &[u8]) -> Option<Page> {
    let mut c = Cursor { data, at: 0 };
    if c.u8()? != 1 {
        return None;
    }
    let (page, pages, kinds) = (c.u16()?, c.u16()?, c.u16()?);
    let count = c.u8()? as usize;
    let mut rows = Vec::with_capacity(count);
    for _ in 0..count {
        let heading = c.u8()? == 0;
        let label = c.string()?;
        let value = c.string()?;
        rows.push(Row { heading, label, value });
    }
    Some(Page { page, pages, kinds, rows })
}

/// A reader over 0x7EF5's body.
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

    fn u16(&mut self) -> Option<u16> {
        let b = self.data.get(self.at..self.at + 2)?;
        self.at += 2;
        Some(u16::from_le_bytes([b[0], b[1]]))
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
pub enum BestiaryUp {
    /// back to the start menu
    Close,
    /// this page asked for
    Page(u16),
}

/// What a click on the page line's ends turns: back, on.
const HIT_BACK: i32 = 1;
const HIT_ON: i32 = 2;

/// The window: its panel, the wait box until a page comes, and where it drew.
#[derive(Default)]
pub struct BestiaryWnd {
    window: Window,
    wait: Window,
    hits: Hits,
}

/// The panel's middle and width, the kind's and the value's columns.
const CENTER: [i32; 2] = [320, 200];
const WIDTH: i32 = 500;
const LABEL_X: i32 = 30;
const VALUE_X: i32 = 250;

impl BestiaryWnd {
    /// The window opened: the wait box up until a page comes.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.init(vui, r, CENTER, WIDTH, 3, 0);
        self.window.open(false);
        self.wait.init(vui, r, CENTER, 300, 3, 0);
        self.wait.open(false);
        self.hits.clear();
    }

    /// A key on `page` (`None`: none come yet): △ (Escape) closes it, ← and → turn
    /// the page while there is one that way, nothing else does anything.
    pub fn key(&mut self, key: i32, down: bool, page: Option<&Page>, cues: &mut Vec<i32>) -> Option<BestiaryUp> {
        if !down {
            return None;
        }
        match key {
            27 | key::TRIANGLE => {
                cues.push(8);
                Some(BestiaryUp::Close)
            }
            17 | key::LEFT => {
                let p = page?;
                (p.page > 0).then(|| {
                    cues.push(11);
                    BestiaryUp::Page(p.page - 1)
                })
            }
            18 | key::RIGHT => {
                let p = page?;
                (p.page + 1 < p.pages).then(|| {
                    cues.push(11);
                    BestiaryUp::Page(p.page + 1)
                })
            }
            _ => None,
        }
    }

    /// A pointer at frame point `p` (`pointer`): a click on the page line's left or
    /// right end turns the page that way, elsewhere on the window does nothing, and
    /// one off it closes it (△). The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.hits.at(p) {
            Some(HIT_BACK) => vec![key::LEFT],
            Some(HIT_ON) => vec![key::RIGHT],
            Some(_) => Vec::new(),
            None => vec![key::TRIANGLE],
        }
    }

    /// The title bar, then the page (`None`: the wait box), each row a zone's heading
    /// in the selection's colour or a kind and what is known of it, the page line and
    /// the legend.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, page: Option<&Page>) {
        let c0 = UI_COLORS[0];
        self.hits.clear();
        crate::ui::window_backdrop(vui, r, 642);
        let title = wide(ENTRY);
        let tw = vui.string_width(r, &title) + 64;
        vui.blit_bar(r, &title, [1.0; 4], [320 - tw / 2, 15], 33, 34, tw, 0, 1.0, 0, true);
        let Some(page) = page else {
            self.wait.init(vui, r, CENTER, 300, 3, 0);
            self.wait.open(false);
            self.wait.draw(vui, r);
            self.wait.text_box(vui, r, &wide("Opening your bestiary..."), 40, 1, 240, 1, c0, 0);
            let (tl, br) = (self.wait.top_left(), self.wait.bottom_right());
            self.hits.push([tl[0], tl[1], br[0], br[1]], -1);
            return;
        };
        self.wait.close(true);
        let body = page.rows.len().max(1) as i32;
        self.window.init(vui, r, CENTER, WIDTH, body + 2, 0);
        self.window.open(false);
        self.window.draw(vui, r);
        let width = self.window.size[0];
        if page.rows.is_empty() {
            self.window.text_box(vui, r, &wide("You have met no creatures yet."), LABEL_X - 10, 0, width - 2 * LABEL_X, 1, c0, 0);
        }
        for (i, row) in page.rows.iter().enumerate() {
            let at = i as i32;
            if row.heading {
                self.window.text_box(vui, r, &wide(&row.label), LABEL_X - 10, at, width - 2 * LABEL_X, 1, UI_COLORS[1], 0);
                continue;
            }
            self.window.text_box(vui, r, &wide(&row.label), LABEL_X, at, VALUE_X - LABEL_X - 10, 1, c0, 2);
            self.window.text_box(vui, r, &wide(&row.value), VALUE_X, at, width - VALUE_X - 25, 1, c0, 0x10 | 2);
        }
        let (tl, br) = (self.window.top_left(), self.window.bottom_right());
        self.hits.push([tl[0], tl[1], br[0], br[1]], -1);
        let pages = page.pages.max(1);
        let back = if page.page > 0 { "<  " } else { "   " };
        let on = if page.page + 1 < pages { "  >" } else { "   " };
        let line = format!("{back}Page {} of {pages}, {} kinds met{on}", page.page + 1, page.kinds);
        self.window.text_box(vui, r, &wide(&line), LABEL_X - 10, body, width - 2 * LABEL_X, 1, c0, 0);
        let row = crate::pointer::rows_rect(&self.window, vui, r, LABEL_X - 10, body, width - 2 * LABEL_X, 1);
        let third = (row[2] - row[0]) / 3;
        self.hits.push([row[0], row[1], row[0] + third, row[3]], HIT_BACK);
        self.hits.push([row[2] - third, row[1], row[2], row[3]], HIT_ON);
        let exit = vui.words.get(161, vui.language).to_string();
        self.window.text_box(vui, r, &wide(&format!("|t{exit}")), LABEL_X - 10, body + 1, width - 2 * LABEL_X, 1, c0, 0);
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

    /// 0x7EF5's body as go-eqoa-server writes it.
    pub(crate) fn body(page: u16, pages: u16, kinds: u16, rows: &[(u8, &str, &str)]) -> Vec<u8> {
        let s = |b: &mut Vec<u8>, t: &str| {
            b.push(t.len() as u8);
            b.extend(t.as_bytes());
        };
        let mut b = vec![1];
        for v in [page, pages, kinds] {
            b.extend(v.to_le_bytes());
        }
        b.push(rows.len() as u8);
        for (style, label, value) in rows {
            b.push(*style);
            s(&mut b, label);
            s(&mut b, value);
        }
        b
    }

    /// A page read back: which, of how many, the kinds, and its rows, headings told
    /// from kinds; another version and every page cut short are none.
    #[test]
    fn a_page_is_read_as_the_server_writes_it() {
        let b = body(1, 3, 40, &[(0, "Blackburrow", ""), (1, "gnoll pup", "12 killed, first Nov 3"), (1, "a bat", "met Nov 4")]);
        let p = read(&b).unwrap();
        assert_eq!((p.page, p.pages, p.kinds, p.rows.len()), (1, 3, 40, 3));
        assert!(p.rows[0].heading && !p.rows[1].heading);
        assert_eq!((p.rows[1].label.as_str(), p.rows[1].value.as_str()), ("gnoll pup", "12 killed, first Nov 3"));
        for n in 0..b.len() {
            assert_eq!(read(&b[..n]), None, "cut at {n}");
        }
        let mut other = b.clone();
        other[0] = 2;
        assert_eq!(read(&other), None, "another version");
        assert_eq!(read(&body(0, 1, 0, &[])).map(|p| p.rows.len()), Some(0));
    }

    /// The window: the wait box before a page comes, then its rows, the page line and
    /// the legend; ← and → turn the page while there is one that way (the keyboard's
    /// arrows and the d-pad's), never past either end nor before a page came; △ and
    /// Escape close it; a click on the page line's ends turns it, elsewhere on the
    /// window does nothing and off it closes it.
    #[test]
    fn the_window_turns_its_pages_and_closes() {
        use crate::pointer::testing::{NullRaster, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let mut w = BestiaryWnd::default();
        w.open(&vui, &r);
        let mut cues = Vec::new();
        assert_eq!(w.key(key::RIGHT, true, None, &mut cues), None, "no page yet");
        w.draw(&vui, &mut r, None);
        assert!(w.point([320.0, 200.0]).is_empty(), "on the wait box");
        let first = read(&body(0, 2, 3, &[(0, "Freeport", ""), (1, "rat", "met Nov 3"), (1, "bat", "2 killed, first Nov 3")])).unwrap();
        w.draw(&vui, &mut r, Some(&first));
        assert_eq!(w.rows(), 5, "three rows, the page line and the legend");
        assert_eq!(w.key(key::LEFT, true, Some(&first), &mut cues), None, "no page before the first");
        assert_eq!(w.key(key::RIGHT, true, Some(&first), &mut cues), Some(BestiaryUp::Page(1)));
        assert_eq!(w.key(18, true, Some(&first), &mut cues), Some(BestiaryUp::Page(1)), "the keyboard's →");
        assert_eq!(w.key(key::RIGHT, false, Some(&first), &mut cues), None, "a release");
        let last = read(&body(1, 2, 3, &[(0, "Qeynos", ""), (1, "wolf", "met Nov 5")])).unwrap();
        assert_eq!(w.key(key::RIGHT, true, Some(&last), &mut cues), None, "no page past the last");
        assert_eq!(w.key(17, true, Some(&last), &mut cues), Some(BestiaryUp::Page(0)), "the keyboard's ←");
        w.draw(&vui, &mut r, Some(&first));
        let (tl, br) = (w.window.top_left(), w.window.bottom_right());
        let line = crate::pointer::rows_rect(&w.window, &vui, &r, LABEL_X - 10, 3, w.window.size[0] - 2 * LABEL_X, 1);
        let y = (line[1] + line[3]) as f32 / 2.0;
        assert_eq!(w.point([line[0] as f32 + 2.0, y]), vec![key::LEFT], "the page line's left end");
        assert_eq!(w.point([line[2] as f32 - 2.0, y]), vec![key::RIGHT], "its right end");
        assert!(w.point([(tl[0] + br[0]) as f32 / 2.0, (tl[1] + 4) as f32]).is_empty(), "on the window");
        assert_eq!(w.point([2.0, 2.0]), vec![key::TRIANGLE], "off it");
        assert_eq!(w.key(132, true, Some(&first), &mut cues), Some(BestiaryUp::Close));
        assert_eq!(w.key(27, true, None, &mut cues), Some(BestiaryUp::Close));
        assert_eq!(cues, [11, 11, 11, 8, 8]);
        let empty = read(&body(0, 1, 0, &[])).unwrap();
        w.draw(&vui, &mut r, Some(&empty));
        assert_eq!(w.rows(), 3, "its line, the page line and the legend");
    }
}
