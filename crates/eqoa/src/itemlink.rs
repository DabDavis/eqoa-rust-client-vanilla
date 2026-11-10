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
//!   `link_item`), and its own X; Escape or ○ close the newest.

use net::dump::NetItem;
use vi::ui::{FrameParam, Raster, Vui, Window, icon_id, wide};

/// A link's colour, in the chat box and in the chat line being typed.
pub const COLOR: [f32; 4] = [0.74, 0.5, 1.0, 1.0];

/// The wash over the link the history's walk has lit.
pub const LIT: [f32; 4] = [0.74, 0.5, 1.0, 0.4];

/// The most windows open at once.
pub const WINDOWS: usize = 2;

/// A window's width, and the band at the top of the 640 by 448 frame the
/// windows keep to: right of the gauges (x 130), left of the minimap (472),
/// under the effects' strip (y 15 to 49).
const W: i32 = 166;
const BAND: [i32; 2] = [134, 470];
const TOP: i32 = 52;
const GAP: i32 = 4;

/// The head (the icon and the name) and the text's rows at most.
const HEAD: i32 = 30;
const ROWS_MOST: i32 = 12;

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

    /// Escape or ○: the newest closed; false with none open.
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

    /// Where window `k` of `n` goes: one in the band's middle, two side by side.
    fn at(k: usize, n: usize) -> [i32; 2] {
        if n <= 1 {
            return [(BAND[0] + BAND[1]) / 2 - W / 2, TOP];
        }
        [BAND[0] + k as i32 * (W + GAP), TOP]
    }

    /// The windows, each where `at` puts it: `describe` is an item's lines,
    /// `icon` its surface (`inventory::item_icon`). Each one's place and its X
    /// are noted for a pointer.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, describe: &dyn Fn(&NetItem) -> String, icon: &dyn Fn(&NetItem) -> i32) {
        let n = self.windows.len();
        let (white, yellow) = ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 0.0, 1.0]);
        let fh = vui.font_height(r);
        let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) };
        for k in 0..n {
            let at = Self::at(k, n);
            let w = &mut self.windows[k];
            let text = match (&w.item, w.gone) {
                (Some(it), _) => describe(it),
                (None, true) => "This link has expired.".to_string(),
                (None, false) => "...".to_string(),
            };
            let text = wide(&text);
            let mut tw = Window::default();
            let rows = tw.count_lines(vui, r, &text, W - 20).clamp(1, ROWS_MOST);
            let h = HEAD + rows * (fh - 4) + 10;
            w.rect = [at[0], at[1], at[0] + W, at[1] + h];
            r.blit(icon_id("black"), 0, at, [0, 0], [W, h], 3, 0.8);
            vui.blit_frame(r, at, [W, h], &fp, 3, 1.0, true);
            // the item's icon at half size, its name cut to what is left of the
            // head beside the X
            let mut name_x = at[0] + 8;
            if let Some(it) = &w.item {
                let id = icon(it);
                if r.has(id as u32) {
                    r.stretch_blit(id as u32, 0, [at[0] + 6, at[1] + 5], [52, 20], [0, 0], [114, 44], 3, 1.0);
                    name_x = at[0] + 62;
                }
            }
            let shown = w.item.as_ref().map_or(w.name.as_str(), |it| it.item.name.as_str());
            let mut name = wide(shown);
            let room = at[0] + W - 22 - name_x;
            while !name.is_empty() && vui.string_width(r, &name) > room {
                name.pop();
            }
            vui.draw_string_ex(r, &name, name_x, at[1] + 7, yellow, true);
            let x = wide("x");
            let xx = at[0] + W - 16;
            vui.draw_string_ex(r, &x, xx, at[1] + 5, white, true);
            w.x_hit = [xx - 6, at[1], at[0] + W, at[1] + 24];
            // the lines: a window of the text's rows, its first row under the head
            let top = at[1] + HEAD - 15;
            let size = rows * (fh - 4) + 36;
            tw.init(vui, r, [at[0] + W / 2, top + size / 2], W, rows, 1);
            tw.text_box(vui, r, &text, 10, 0, W - 20, rows, white, 3);
        }
    }
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
        l.draw(&v, &mut r, &|it| it.item.name.clone(), &|_| -1);
        let (a, b) = (l.windows[0].rect, l.windows[1].rect);
        assert!(a[0] >= BAND[0] && b[2] <= BAND[1] && a[2] < b[0], "side by side in the band: {a:?} {b:?}");
        assert_eq!((a[1], b[1]), (TOP, TOP));
        let mid = |r: [i32; 4]| [(r[0] + r[2]) as f32 / 2.0, (r[1] + r[3]) as f32 / 2.0];
        assert!(l.covers(mid(a)) && !l.covers([600.0, 400.0]));
        assert!(!l.point(mid(a)), "the window, not its X");
        let x = l.windows[0].x_hit;
        assert!(l.point([(x[0] + x[2]) as f32 / 2.0, (x[1] + x[3]) as f32 / 2.0]));
        assert_eq!(l.windows.iter().map(|w| w.id).collect::<Vec<_>>(), [8], "its own window closed");
        l.draw(&v, &mut r, &|it| it.item.name.clone(), &|_| -1);
        let one = l.windows[0].rect;
        assert_eq!((one[0] + one[2]) / 2, (BAND[0] + BAND[1]) / 2, "one alone in the band's middle");
    }
}
