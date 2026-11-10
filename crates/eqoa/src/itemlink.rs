//! Item links in chat (the GM Jibrish's petition 1463, 2026-11-10: "shift click
//! an item and if your chat is open it puts a link to that item. If people
//! click it, they can view the items stats, name, icon ... When an item is
//! clicked on it should be succint but show all relevant stats in a window
//! tucked safely at the top middle of the screen. They should be able to open
//! 2 at once, but only 2"): the client's own, as the game has none, on a server
//! that offers them (go-eqoa-server's 0x7E4F-0x7E53, `net::itemlink`) with the
//! Extended row Item Links on (`settings::item_links`, on by default; off, the
//! server is told so and sends the plain names a PS2 is sent).
//!
//! - Making one (`Play::link_item`): the item menu's Link Item (`itemmenu`), or
//!   a Shift+click on an item there, closes the start menu and opens the chat
//!   line with the item's name in it, purple, one unit (`editbox`): a step of
//!   the caret goes over it and Backspace takes it whole. A line holds
//!   [`net::itemlink::LINKS_MOST`]; one cancelled with a link in it is kept
//!   until it is sent or emptied, so a second item can be linked into it.
//! - Sending: the line goes as `net::itemlink::say` in place of its retail
//!   message (`Play::chat_line_linked`), naming each link by its record's list
//!   and key; the server snapshots the item as the player holds it.
//! - Reading: a relayed line's links are purple in the chat box (`chat`); a
//!   click on one opens it, and with the chat box's history held (R2, or C)
//!   left walks up through the links and right back down, ✕ (Enter) opening
//!   the one lit; the history let go, the walk is forgotten.
//! - The windows ([`Links`]): at most [`WINDOWS`], side by side at the top
//!   middle in the band between the gauges and the minimap, a third replacing
//!   the oldest; each its item's icon, name and its lines (`infotext`'s
//!   `link_item`), and its own X; Escape or △ (the game's Exit) close the
//!   newest.

use net::dump::NetItem;
use vi::ui::{FrameParam, Raster, Vui, Window, icon_id, wide};

/// A link's colour, in the chat box and in the chat line being typed.
pub const COLOR: [f32; 4] = [0.74, 0.5, 1.0, 1.0];

/// The wash over the link the history's walk has lit.
pub const LIT: [f32; 4] = [0.74, 0.5, 1.0, 0.4];

/// The most windows open at once.
pub const WINDOWS: usize = 2;

/// A window's width at most, and the band at the top of the 640 by 448 frame
/// the windows keep to: right of the gauges (x 130), left of the minimap (472),
/// under the effects' strip (y 15 to 49); a window wider than 4:3 widens the
/// band by what it shows beyond each side (the gauges and the minimap keep to
/// the sides). Two windows share the band, each as wide as half of it allows.
const W: i32 = 220;
const BAND: [i32; 2] = [134, 470];
const TOP: i32 = 52;
const GAP: i32 = 4;

/// The windows' foot at most: over the chat box's top (y 321 with its five
/// rows), where they cut their text.
const BOTTOM: i32 = 318;

/// The name's rows at most (beside the icon), and the text's.
const NAME_ROWS: i32 = 2;
const ROWS_MOST: i32 = 14;

/// The records the server sent kept, newest last (a link opened again is
/// shown at once).
const KNOWN_MOST: usize = 32;

/// One link's window.
#[derive(Clone, Debug)]
pub struct Wnd {
    pub id: u32,
    /// the name the chat line showed (until the record comes)
    name: String,
    item: Option<NetItem>,
    /// the server holds no such link (expired)
    gone: bool,
    /// where it was drawn last, and its X (frame x0 y0 x1 y1)
    rect: [i32; 4],
    x_hit: [i32; 4],
}

/// The open link windows (oldest first) and the records the server sent.
#[derive(Default)]
pub struct Links {
    pub windows: Vec<Wnd>,
    known: std::collections::VecDeque<(u32, Option<NetItem>)>,
}

impl Links {
    /// Link `id` opened, `name` its name in the line: a window of its own (the
    /// oldest let go for a third), or nothing where one is open already. The id
    /// to ask the server for, when its record is not known yet.
    pub fn open(&mut self, id: u32, name: &str) -> Option<u32> {
        if self.windows.iter().any(|w| w.id == id) {
            return None;
        }
        if self.windows.len() >= WINDOWS {
            self.windows.remove(0);
        }
        let known = self.known.iter().find(|k| k.0 == id).map(|k| k.1.clone());
        let (item, gone) = match &known {
            Some(Some(it)) => (Some(it.clone()), false),
            Some(None) => (None, true),
            None => (None, false),
        };
        self.windows.push(Wnd { id, name: name.to_string(), item, gone, rect: [0; 4], x_hit: [0; 4] });
        known.is_none().then_some(id)
    }

    /// The server's record of link `id` (none: it holds no such link), into its
    /// window and kept.
    pub fn record(&mut self, id: u32, item: Option<NetItem>) {
        for w in self.windows.iter_mut().filter(|w| w.id == id) {
            w.gone = item.is_none();
            w.item = item.clone();
        }
        self.known.retain(|k| k.0 != id);
        self.known.push_back((id, item));
        while self.known.len() > KNOWN_MOST {
            self.known.pop_front();
        }
    }

    pub fn is_open(&self) -> bool {
        !self.windows.is_empty()
    }

    /// Escape or △: the newest closed; false with none open.
    pub fn close_newest(&mut self) -> bool {
        self.windows.pop().is_some()
    }

    /// Every window closed (the Extended row turned off).
    pub fn close_all(&mut self) {
        self.windows.clear();
    }

    /// Whether frame point `p` is on a window (a click there is theirs).
    pub fn covers(&self, p: [f32; 2]) -> bool {
        self.windows.iter().any(|w| inside(w.rect, p))
    }

    /// A click at frame point `p`: on a window's X it closes (true); anywhere
    /// else on a window nothing.
    pub fn point(&mut self, p: [f32; 2]) -> bool {
        match self.windows.iter().position(|w| inside(w.x_hit, p)) {
            Some(k) => {
                self.windows.remove(k);
                true
            }
            None => false,
        }
    }

    /// The windows' width and where each goes when `n` are open, the band
    /// widened by `side` (the frame pixels shown beyond each side): one in its
    /// middle, two side by side.
    fn places(n: usize, side: i32) -> (i32, Vec<[i32; 2]>) {
        let band = [BAND[0] - side.max(0), BAND[1] + side.max(0)];
        let n = n as i32;
        let w = if n <= 1 { W } else { W.min((band[1] - band[0] - GAP * (n - 1)) / n) };
        let total = n * w + GAP * (n - 1).max(0);
        let x0 = (band[0] + band[1]) / 2 - total / 2;
        (w, (0..n).map(|k| [x0 + k * (w + GAP), TOP]).collect())
    }

    /// The windows, each where `places` puts it (`side` as there): `describe` is
    /// an item's lines, told what fits on one of its rows (`fits`), `icon` its
    /// surface (`inventory::item_icon`). Each one's place and its X are noted
    /// for a pointer.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, side: i32, describe: &dyn Fn(&NetItem, &dyn Fn(&str) -> bool) -> String, icon: &dyn Fn(&NetItem) -> i32) {
        let (ww, at_of) = Self::places(self.windows.len(), side);
        let (white, yellow) = ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 0.0, 1.0]);
        let step = vui.font_height(r) - 4;
        let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) };
        // a row's room (`TextBox` wraps a row reaching its width less 8)
        let room = ww - 20 - 8;
        for (w, at) in self.windows.iter_mut().zip(at_of) {
            let text = match (&w.item, w.gone) {
                (Some(it), _) => {
                    let rr: &dyn Raster = r;
                    describe(it, &|row: &str| Window::default().text_width(vui, rr, &wide(row)) < room)
                }
                (None, true) => "This link has expired.".to_string(),
                (None, false) => "...".to_string(),
            };
            let text = wide(&text);
            // the head: the item's icon at half size and the X at the right,
            // the name beside the icon where it fits on one row, else under
            // them across the window (two rows at most, then cut)
            let icon_id_of = w.item.as_ref().map(icon).filter(|&id| r.has(id as u32));
            let shown = w.item.as_ref().map_or(w.name.as_str(), |it| it.item.name.as_str());
            let name = wide(shown);
            let beside = icon_id_of.is_none() || vui.string_width(r, &name) < ww - 64 - 22 - 8;
            let (name_x, name_w, name_top) = match (icon_id_of.is_some(), beside) {
                (true, true) => (64, ww - 64 - 22, 6),
                (true, false) => (10, ww - 20, 30),
                (false, _) => (10, ww - 32, 6),
            };
            let name_rows = Window::default().count_lines(vui, r, &name, name_w).clamp(1, NAME_ROWS);
            let head = (name_top + name_rows * step + 4).max(32);
            let room_rows = ((BOTTOM - TOP - head - 8) / step).clamp(1, ROWS_MOST);
            let rows = Window::default().count_lines(vui, r, &text, ww - 20).clamp(1, room_rows);
            let h = head + rows * step + 8;
            w.rect = [at[0], at[1], at[0] + ww, at[1] + h];
            r.blit(icon_id("black"), 0, at, [0, 0], [ww, h], 3, 0.8);
            vui.blit_frame(r, at, [ww, h], &fp, 3, 1.0, true);
            if let Some(id) = icon_id_of {
                r.stretch_blit(id as u32, 0, [at[0] + 6, at[1] + 6], [52, 20], [0, 0], [114, 44], 3, 1.0);
            }
            rows_at(vui, r, at, ww, at[1] + name_top + step / 2, name_rows).text_box(vui, r, &name, name_x, 0, name_w, name_rows, yellow, 3);
            let xx = at[0] + ww - 16;
            vui.draw_string_ex(r, &wide("x"), xx, at[1] + 5, white, true);
            w.x_hit = [xx - 6, at[1], at[0] + ww, at[1] + 24];
            // the lines under the head
            rows_at(vui, r, at, ww, at[1] + head + step / 2, rows).text_box(vui, r, &text, 10, 0, ww - 20, rows, white, 3);
        }
    }
}

/// A window `w` wide from `at`'s x whose row 0 is centred on frame line `y`,
/// `rows` rows: what `Window::text_box` lays text out in (its rows are
/// `font_height - 4` apart, from its top past an 18-pixel title and 5 more).
fn rows_at(vui: &Vui, r: &dyn Raster, at: [i32; 2], w: i32, y: i32, rows: i32) -> Window {
    let mut win = Window::default();
    win.init(vui, r, [0, 0], w, rows, 1);
    let top = y - 23;
    win.center = [at[0] + w / 2, top + win.size[1] / 2];
    win
}

fn inside(r: [i32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] as f32 && p[0] < r[2] as f32 && p[1] >= r[1] as f32 && p[1] < r[3] as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};

    fn item(name: &str) -> NetItem {
        let mut it = NetItem::default();
        it.item.name = name.to_string();
        it
    }

    /// Two at most: a third opens in place of the oldest; one open already is
    /// not opened twice; each asks the server once, and a record known is shown
    /// at once. The control: a fresh link asks.
    #[test]
    fn two_windows_at_most() {
        let mut l = Links::default();
        assert_eq!(l.open(1, "Sword"), Some(1), "unknown: asked for");
        assert_eq!(l.open(2, "Shield"), Some(2));
        assert_eq!(l.open(1, "Sword"), None, "open already");
        assert_eq!(l.windows.len(), 2);
        assert_eq!(l.open(3, "Helm"), Some(3));
        assert_eq!(l.windows.iter().map(|w| w.id).collect::<Vec<_>>(), [2, 3], "the oldest went for the third");
        l.record(1, Some(item("Sword of Faith")));
        assert_eq!(l.open(1, "Sword"), None, "known: not asked again");
        assert_eq!(l.windows.iter().map(|w| w.id).collect::<Vec<_>>(), [3, 1]);
        assert_eq!(l.windows[1].item.as_ref().map(|i| i.item.name.as_str()), Some("Sword of Faith"));
        l.record(3, None);
        assert!(l.windows[0].gone, "the server holds no such link");
        assert!(l.close_newest() && l.close_newest() && !l.close_newest());
    }

    /// Drawn side by side in the top band; a click on a window's X closes that
    /// one, elsewhere on it nothing, and off them it is not theirs. The control:
    /// before a frame drew them nothing is there.
    #[test]
    fn each_window_closes_on_its_own_x() {
        let (v, mut r) = (vui(), NullRaster);
        let mut l = Links::default();
        l.open(7, "Sword");
        l.open(8, "Shield");
        assert!(!l.covers([200.0, 60.0]), "not drawn yet");
        l.draw(&v, &mut r, 0, &|it, _| it.item.name.clone(), &|_| -1);
        let (a, b) = (l.windows[0].rect, l.windows[1].rect);
        assert!(a[0] >= BAND[0] && b[2] <= BAND[1] && a[2] < b[0], "side by side in the band: {a:?} {b:?}");
        assert_eq!((a[1], b[1]), (TOP, TOP));
        let mid = |r: [i32; 4]| [(r[0] + r[2]) as f32 / 2.0, (r[1] + r[3]) as f32 / 2.0];
        assert!(l.covers(mid(a)) && !l.covers([600.0, 400.0]));
        assert!(!l.point(mid(a)), "the window, not its X");
        let x = l.windows[0].x_hit;
        assert!(l.point([(x[0] + x[2]) as f32 / 2.0, (x[1] + x[3]) as f32 / 2.0]));
        assert_eq!(l.windows.iter().map(|w| w.id).collect::<Vec<_>>(), [8], "its own window closed");
        l.draw(&v, &mut r, 0, &|it, _| it.item.name.clone(), &|_| -1);
        let one = l.windows[0].rect;
        assert_eq!((one[0] + one[2]) / 2, (BAND[0] + BAND[1]) / 2, "one alone in the band's middle");
        assert_eq!(one[2] - one[0], W, "and its whole width");
        // a window wider than 4:3: the band widened, two at their whole width
        l.open(9, "Helm");
        l.draw(&v, &mut r, 60, &|it, _| it.item.name.clone(), &|_| -1);
        let (a, b) = (l.windows[0].rect, l.windows[1].rect);
        assert!(a[2] - a[0] == W && b[2] - b[0] == W && a[0] >= BAND[0] - 60 && b[2] <= BAND[1] + 60, "{a:?} {b:?}");
        // a long text is cut over the chat box (the control: a short one is
        // its own height)
        let long = "a line\n".repeat(40);
        l.record(8, Some(item("Shield")));
        l.record(9, Some(item("Helm")));
        l.draw(&v, &mut r, 0, &|_, _| long.clone(), &|_| -1);
        let tall = l.windows[0].rect;
        l.draw(&v, &mut r, 0, &|it, _| it.item.name.clone(), &|_| -1);
        assert!(tall[3] <= BOTTOM && tall[3] > l.windows[0].rect[3], "{tall:?}");
    }
}
