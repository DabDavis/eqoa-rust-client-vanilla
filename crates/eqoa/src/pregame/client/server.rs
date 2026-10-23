//! `VIWndServerSelect` (CLIENT `viwndserverselect.cpp`): the server list.
//! It waits 50 frames, connects for the list (`ConnectRandomShard`), shows
//! it sorted by name with the home shard picked (game.opt's, else the first
//! valid one, which shows "Recommended"), retrying a failed connection four
//! times; Select keeps the home shard and, three frames on, connects to the
//! shard (`Connect` -> `ConnectShard`); Back leaves for STATION's login.
//!
//! Left out, each where it would be: the memory card's checks before the
//! connection (`McErr`, `VIIsMcSameSession`), the "Saving game options" box
//! the three frames show (the card's) and the card's error box.

use super::{Req, ShardEntry, UiCx};
use crate::pointer::{Hits, key};
use crate::textlist::{Message, PROMPT_OK};
use vi::ui::{UI_COLORS, Window, wide};

/// What a message box up over the list is about (`m_invalid`, `m_empty`,
/// the failures).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Why {
    Invalid,
    Empty,
    Failed,
}

#[derive(Default)]
pub struct ServerSelect {
    window: Window,
    /// `m_status`: the "Connecting" box
    status: Window,
    /// `m_box`: the "Saving game options" box (not drawn: the card's)
    pub message: Message,
    why: Option<Why>,
    /// a message asked for by a key (title and text words, width), set up
    /// at the next draw (it measures)
    pending: Option<(i32, i32, i32)>,
    sel: i32,
    top: i32,
    rows: i32,
    /// `m_order`: the list's indices sorted by name
    order: Vec<usize>,
    /// `m_name`: the selected shard's name (kept through a sort)
    name: Vec<u16>,
    /// `m_home`: the recommended shard (the first valid one when game.opt
    /// names none in the list)
    home: Vec<u16>,
    pub fetching: bool,
    delay: i32,
    retries: i32,
    pub connecting: bool,
    reconnect_failed: bool,
    pub failed: bool,
    requested: bool,
    empty: bool,
    countdown: i32,
    /// "..." after a status: a dot more every 15 frames, three at most
    /// (`sDots`, `sTicks`)
    dots: i32,
    ticks: i32,
    hits: Hits,
}

/// One WORDS.TXT word in the UI's language.
fn word(cx: &UiCx, id: i32) -> Vec<u16> {
    wide(cx.vui.words.get(id, cx.vui.language))
}

/// `strlen16(m_name)` BYTES of the two names compared, as `Sort` compares
/// them (`memcmp` counts bytes, so half the name's characters decide).
fn name_prefix_eq(name: &[u16], other: &[u16]) -> bool {
    let n = name.len();
    let bytes = |s: &[u16]| s.iter().flat_map(|c| c.to_le_bytes()).chain(std::iter::repeat(0)).take(n).collect::<Vec<u8>>();
    bytes(name) == bytes(other)
}

impl ServerSelect {
    /// Not connecting, no message up: `Client::quiet`.
    pub fn quiet(&self) -> bool {
        !self.connecting && self.why.is_none() && self.pending.is_none()
    }

    /// `OnMessage` 295 with the focus arriving: the window and the status
    /// box made again, the list's state from the top.
    pub fn focus(&mut self, cx: &UiCx, shards: &[ShardEntry]) {
        self.window.close(true);
        self.window.init(cx.vui, cx.r, [320, 224], 500, 16, 0);
        self.window.set_title(Some(&word(cx, 940)));
        self.window.open(true);
        self.sel = 0;
        self.name.clear();
        self.home.clear();
        self.top = 0;
        self.rows = 12;
        self.sort(shards);
        self.status.close(true);
        self.status.init(cx.vui, cx.r, [320, 224], 330, 3, 0);
        self.status.set_title(Some(&word(cx, 271)));
        self.status.open(false);
        self.fetching = true;
        self.delay = 50;
        self.retries = 4;
        self.connecting = false;
        self.reconnect_failed = false;
        self.failed = false;
        self.requested = false;
        self.why = None;
        self.empty = false;
        self.countdown = 0;
        self.message.visible = false;
        self.pending = None;
    }

    /// `Sort`: the list by name, the selection kept on the shard it was on
    /// (`m_name`), else one up.
    pub fn sort(&mut self, shards: &[ShardEntry]) {
        if shards.is_empty() {
            return;
        }
        self.order = (0..shards.len()).collect();
        self.order.sort_by(|&a, &b| shards[a].name.cmp(&shards[b].name));
        let old = self.sel;
        self.sel = -1;
        if self.name.is_empty() {
            self.sel = 0;
        } else if let Some(i) = self.order.iter().position(|&k| name_prefix_eq(&self.name, &shards[k].name)) {
            self.sel = i as i32;
        }
        if self.sel == -1 {
            self.sel = (old - 1).max(0);
        }
        self.sel = self.sel.min(shards.len() as i32 - 1).max(0);
        self.name = shards[self.order[self.sel as usize]].name.clone();
    }

    /// The shard picked, by its place in the list as given.
    pub fn picked(&self) -> Option<usize> {
        self.order.get(self.sel.max(0) as usize).copied()
    }

    /// `UpdateList`: the wait, the connection for the list, and what came:
    /// -1 failed, 1 still fetching, 0 done.
    fn update_list(&mut self, cx: &mut UiCx, shards: &[ShardEntry], reqs: &mut Vec<Req>) -> i32 {
        if self.fetching && !self.failed {
            if !self.requested {
                if self.delay > 0 {
                    self.delay -= 1;
                } else {
                    self.requested = true;
                    reqs.push(Req::ConnectForList);
                }
            } else {
                match cx.state {
                    21 => {
                        self.fetching = false;
                        self.sel = -1;
                        if shards.is_empty() {
                            self.empty = true;
                            self.show_message(cx, Why::Empty, 1605, word(cx, 1606), 400);
                        } else {
                            if let Some(i) = shards.iter().position(|s| s.name == cx.home_shard) {
                                self.sel = i as i32;
                                self.name = shards[i].name.clone();
                            }
                            if self.sel == -1 {
                                match shards.iter().position(|s| s.recommended && s.valid) {
                                    Some(i) => {
                                        self.sel = i as i32;
                                        self.home = shards[i].name.clone();
                                    }
                                    None => self.sel = 0,
                                }
                                self.name = shards[self.sel as usize].name.clone();
                            }
                            self.sort(shards);
                            self.requested = false;
                        }
                    }
                    20 => {}
                    s => {
                        if matches!(s, 3 | 4 | 5 | 12 | 16) {
                            self.retries = 0;
                        }
                        if self.retries > 0 {
                            self.retries -= 1;
                            self.delay = 50;
                            self.requested = false;
                        } else {
                            self.failed = true;
                            self.show_failure(cx);
                        }
                    }
                }
            }
        }
        if self.failed {
            return -1;
        }
        self.fetching as i32
    }

    /// `UpdateConnect`: the connection to the shard picked: -1 failed, 1
    /// still connecting, 0 not connecting.
    fn update_connect(&mut self, cx: &mut UiCx) -> i32 {
        if self.connecting && !self.reconnect_failed && !matches!(cx.state, 20 | 21) {
            self.reconnect_failed = true;
            self.show_failure(cx);
        }
        if self.reconnect_failed {
            return -1;
        }
        self.connecting as i32
    }

    /// The box `ConnectStatusString` fills, sized to it.
    fn show_failure(&mut self, cx: &mut UiCx) {
        let text = cx.status.clone();
        let w = self.window.text_width(cx.vui, cx.r, &text);
        self.show_message(cx, Why::Failed, 272, text, if w > 440 { 540 } else { w + 100 });
    }

    fn show_message(&mut self, cx: &mut UiCx, why: Why, title: i32, text: Vec<u16>, width: i32) {
        self.why = Some(why);
        let title = word(cx, title);
        self.message.setup(cx.vui, cx.r, &title, &text, PROMPT_OK, [320, 224], width);
    }

    /// `OnDrawSelf`, its message box after it.
    pub fn draw(&mut self, cx: &mut UiCx, shards: &[ShardEntry], reqs: &mut Vec<Req>) {
        if let Some((title, text, width)) = self.pending.take() {
            let text = word(cx, text);
            self.show_message(cx, Why::Invalid, title, text, width);
        }
        if !self.failed && self.update_list(cx, shards, reqs) != 1 && self.update_connect(cx) != 1 && cx.state != 21 {
            self.failed = true;
            self.show_failure(cx);
        }
        // the recommended shard gone invalid: the first valid one is
        if !shards.is_empty() && !self.home.is_empty() {
            for s in shards {
                if s.name == self.home
                    && !s.valid
                    && let Some(f) = shards.iter().find(|s| s.recommended && s.valid)
                {
                    self.home = f.name.clone();
                    break;
                }
            }
        }
        self.sort(shards);
        let (vui, r) = (cx.vui, &mut *cx.r);
        if self.connecting {
            self.window.reset_tick_pulse(vui.time);
        }
        vui.blit_rect(r, 32, [-1, -1], [641, 449], 3, 1.0);
        self.window.draw(vui, r);
        let count = shards.len() as i32;
        if self.sel - self.top >= self.rows {
            self.top = self.sel - (self.rows - 1);
        } else if self.sel < self.top {
            self.top = self.sel;
        }
        let mut t = if count == self.rows { 0.0 } else { self.top as f32 / (count - self.rows) as f32 };
        if self.empty {
            t = 0.0;
        }
        let width = self.window.size[0];
        self.window.draw_frame_vscroll(vui, r, 40, 2, width - 80, self.rows, t);
        let cols = [58, 58 + (460 - 58) * 65 / 100, 460];
        let words = |id: i32| wide(vui.words.get(id, vui.language));
        self.window.text_box(vui, r, &words(509), cols[0] - 12, 1, cols[1] - cols[0], 1, UI_COLORS[0], 0);
        self.window.text_box(vui, r, &words(803), cols[1], 1, cols[2] - cols[1], 1, UI_COLORS[0], 0);
        self.hits.clear();
        if count > 0 {
            for i in self.top..(self.top + self.rows).min(count) {
                let s = &shards[self.order[i as usize]];
                let row = i - self.top + 2;
                let c = UI_COLORS[(self.sel == i) as usize];
                self.window.text_box(vui, r, &s.name, cols[0], row, cols[1] - cols[0] - 16, 1, c, 2);
                let state = if !s.valid {
                    words(1605)
                } else if !self.home.is_empty() && self.home == s.name {
                    words(1607)
                } else {
                    words(1608)
                };
                self.window.text_box(vui, r, &state, cols[1], row, cols[2] - cols[1], 1, c, 0);
                if self.sel == i {
                    self.window.tick_r(vui, r, 56, row, true);
                }
                self.hits.push(crate::pointer::rows_rect(&self.window, vui, r, cols[0], row, cols[2] - cols[0], 1), i);
            }
        }
        // the legend, dimmed while connecting; each glyph its button
        let (sel, back) = (words(212), words(217));
        let mut legend = wide(if self.connecting { "|-|x" } else { "|x" });
        legend.extend(&sel);
        legend.extend(wide("   |t"));
        legend.extend(&back);
        self.window.text(vui, r, &legend, 40, 15, UI_COLORS[0], 0);
        if !self.connecting {
            let row = crate::pointer::rows_rect(&self.window, vui, r, 40, 15, 0, 1);
            let x0 = self.window.top_left()[0] + 40;
            let w1 = self.window.text_width(vui, r, &[wide("|x"), sel.clone()].concat());
            let w2 = self.window.text_width(vui, r, &[wide("   |t"), back].concat());
            self.hits.push([x0, row[1], x0 + w1, row[3]], HIT_SELECT);
            self.hits.push([x0 + w1 + 8, row[1], x0 + w1 + w2, row[3]], HIT_BACK);
        }
        self.ticks -= 1;
        if self.ticks <= 0 {
            self.ticks = 15;
            self.dots += 1;
            if self.dots == 3 {
                self.dots = 0;
            }
        }
        let dots = |id: i32| {
            let mut b = words(id);
            b.extend(std::iter::repeat_n(b'.' as u16, (self.dots + 1) as usize));
            b
        };
        if self.fetching {
            self.status.draw(vui, r);
            let w = self.status.size[0];
            self.status.text_box(vui, r, &dots(938), 40, 1, w, 1, UI_COLORS[0], 0);
        } else if self.connecting {
            self.status.draw(vui, r);
            let w = self.status.size[0];
            self.status.text_box(vui, r, &dots(939), 40, 1, w, 1, UI_COLORS[0], 0);
        }
        // `m_countdown`: three frames (the card's save box, not drawn), then `Connect`
        if self.countdown > 0 {
            self.countdown -= 1;
            if self.countdown == 0 {
                self.connect(shards, reqs);
            }
        }
        if self.message.visible {
            self.message.draw(vui, r);
        }
    }

    /// `Connect`: the home shard kept (game.opt), then `ConnectShard`.
    fn connect(&mut self, shards: &[ShardEntry], reqs: &mut Vec<Req>) {
        let Some(s) = self.picked().and_then(|i| shards.get(i)) else { return };
        self.connecting = true;
        reqs.push(Req::ConnectShard(s.endpoint));
    }

    /// `OnKeyDown` (and the message box's, while it is up).
    pub fn key(&mut self, k: i32, down: bool, now: i64, shards: &[ShardEntry], cues: &mut Vec<i32>, reqs: &mut Vec<Req>) {
        if !down {
            return;
        }
        if self.message.visible {
            if self.message.key(k, now, cues).is_some() {
                self.closed(reqs);
            }
            return;
        }
        if self.fetching || self.countdown > 0 {
            return;
        }
        match k {
            19 | 138 if !self.connecting => self.step(-1, now, shards, cues),
            20 | 139 if !self.connecting => self.step(1, now, shards, cues),
            27 | 132 if !self.connecting => {
                cues.push(8);
                reqs.push(Req::Disconnect { reason: 0, quiet: false, code: -1 });
                reqs.push(Req::Done(3));
            }
            13 | 134 if !self.connecting => {
                cues.push(15);
                match self.picked().and_then(|i| shards.get(i)) {
                    Some(s) if s.valid => {
                        reqs.push(Req::Home(s.name.clone()));
                        self.countdown = 3;
                    }
                    Some(_) => {
                        // `m_invalid`: "Unavailable", the server's not there
                        self.pending = Some((1605, 666, 440));
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    /// `Next` / `Prev`.
    fn step(&mut self, d: i32, now: i64, shards: &[ShardEntry], cues: &mut Vec<i32>) {
        let count = shards.len() as i32;
        let moved = if d > 0 { self.sel < count - 1 } else { self.sel > 0 };
        if moved {
            self.sel += d;
            self.window.reset_tick_pulse(now);
            cues.push(11);
        } else {
            cues.push(9);
        }
        if count > 0 {
            self.name = shards[self.order[self.sel as usize]].name.clone();
            self.sort(shards);
        }
    }

    /// `OnMessage` 307: the message box closed.
    fn closed(&mut self, reqs: &mut Vec<Req>) {
        match self.why.take() {
            Some(Why::Invalid) => {}
            Some(Why::Empty) => {
                reqs.push(Req::Disconnect { reason: 0, quiet: false, code: -1 });
                reqs.push(Req::Done(3));
            }
            _ => reqs.push(Req::DoneByKind),
        }
    }

    /// A pointer at frame point `p`: a row selects its shard (a click on the
    /// selected one presses ✕); the legend's glyphs are their buttons; the
    /// message box's choice.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.message.visible {
            return self.message.point(p).into_iter().collect();
        }
        if self.fetching || self.countdown > 0 {
            return Vec::new();
        }
        match self.hits.at(p) {
            Some(HIT_SELECT) => vec![key::CROSS],
            Some(HIT_BACK) => vec![key::TRIANGLE],
            Some(i) if i == self.sel => vec![key::CROSS],
            Some(i) if !self.connecting => crate::pointer::steps(self.sel, i, key::UP, key::DOWN),
            _ => Vec::new(),
        }
    }

}

const HIT_SELECT: i32 = -1;
const HIT_BACK: i32 = -2;

#[cfg(test)]
mod tests {
    use super::*;

    fn shard(name: &str, recommended: bool, valid: bool) -> ShardEntry {
        ShardEntry { name: wide(name), recommended, valid, endpoint: 7 }
    }

    /// `Sort` keeps the selection on its shard's name, comparing only the
    /// first half of its characters (`memcmp` counts bytes): a shard whose
    /// name shares that half is found first.
    #[test]
    fn the_sort_keeps_the_name_by_its_first_half() {
        let shards = vec![shard("Zeta", true, true), shard("Alpha", true, true), shard("Marr's Fist", true, true)];
        let mut s = ServerSelect { name: wide("Marr's Fist"), ..Default::default() };
        s.sort(&shards);
        assert_eq!(s.order, vec![1, 2, 0], "by name");
        assert_eq!(s.sel, 1);
        // a name that matches only past its first half: half its bytes decide
        let shards2 = vec![shard("Abcdef", true, true), shard("Abcxyz", true, true)];
        let mut t = ServerSelect { name: wide("Abcxyz"), ..Default::default() };
        t.sort(&shards2);
        assert_eq!(t.sel, 0, "\"Abc\" is half of six characters' bytes: the first match wins");
        assert!(name_prefix_eq(&wide("Abcxyz"), &wide("Abcdef")) && !name_prefix_eq(&wide("Abdxyz"), &wide("Abcdef")), "the control");
    }

    /// The home shard: game.opt's when the list has it (then nothing is
    /// "Recommended"), else the first valid recommended one.
    #[test]
    fn the_list_picks_the_home_shard() {
        let shards = vec![shard("Bristle", false, true), shard("Castle", true, true), shard("Abyss", true, false)];
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        let mut reqs = Vec::new();
        for home in ["", "Bristle"] {
            let mut s = ServerSelect::default();
            let mut cx = UiCx::bare(&vui, &mut r);
            cx.home_shard = wide(home);
            s.focus(&cx, &shards);
            // 50 frames, then the connection is asked for once
            for _ in 0..51 {
                s.update_list(&mut cx, &shards, &mut reqs);
            }
            assert!(matches!(reqs.as_slice(), [Req::ConnectForList]), "{reqs:?}");
            reqs.clear();
            cx.state = 21;
            assert_eq!(s.update_list(&mut cx, &shards, &mut reqs), 0);
            let picked = s.picked().map(|i| String::from_utf16_lossy(&shards[i].name));
            if home.is_empty() {
                assert_eq!(picked.as_deref(), Some("Castle"), "the first valid recommended");
                assert_eq!(s.home, wide("Castle"));
            } else {
                assert_eq!(picked.as_deref(), Some("Bristle"), "game.opt's");
                assert!(s.home.is_empty(), "nothing recommended then");
            }
        }
    }

    /// A failed connection for the list is tried again after 50 frames, four
    /// times, then the failure box; a refusal (3) at once.
    #[test]
    fn the_list_retries_four_times() {
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        for (state, connects) in [(10, 5), (3, 1)] {
            let mut s = ServerSelect::default();
            let mut cx = UiCx::bare(&vui, &mut r);
            cx.status = wide("gone");
            s.focus(&cx, &[]);
            let mut asked = 0;
            for _ in 0..1000 {
                let mut reqs = Vec::new();
                cx.state = if s.requested { state } else { 20 };
                let v = s.update_list(&mut cx, &[], &mut reqs);
                asked += reqs.iter().filter(|q| matches!(q, Req::ConnectForList)).count();
                if v == -1 {
                    break;
                }
            }
            assert_eq!(asked, connects, "state {state}");
            assert!(s.failed && s.message.visible);
        }
    }

    /// The keys: up and down move (a buzz at the ends), ✕ keeps the home
    /// shard and connects three frames on, △ disconnects back to STATION;
    /// nothing while the list is fetched.
    #[test]
    fn the_keys_select_and_back() {
        let shards = vec![shard("A", true, true), shard("B", true, true)];
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        let mut s = ServerSelect::default();
        let mut cx = UiCx::bare(&vui, &mut r);
        cx.state = 21;
        s.focus(&cx, &shards);
        let (mut cues, mut reqs) = (Vec::new(), Vec::new());
        s.key(134, true, 0, &shards, &mut cues, &mut reqs);
        assert!(reqs.is_empty(), "nothing while fetching");
        s.fetching = false;
        s.sort(&shards);
        s.key(138, true, 0, &shards, &mut cues, &mut reqs);
        s.key(139, true, 0, &shards, &mut cues, &mut reqs);
        s.key(139, true, 0, &shards, &mut cues, &mut reqs);
        assert_eq!(cues, vec![9, 11, 9]);
        s.key(134, true, 0, &shards, &mut cues, &mut reqs);
        assert!(matches!(&reqs[..], [Req::Home(n)] if *n == wide("B")));
        reqs.clear();
        for _ in 0..3 {
            s.draw(&mut cx, &shards, &mut reqs);
        }
        assert!(matches!(&reqs[..], [Req::ConnectShard(7)]), "{reqs:?}");
        reqs.clear();
        s.connecting = false;
        s.key(132, true, 0, &shards, &mut cues, &mut reqs);
        assert!(matches!(&reqs[..], [Req::Disconnect { reason: 0, .. }, Req::Done(3)]));
    }

    /// A pointer: a row selects its shard, the selected one presses ✕; the
    /// control: no hit notes, no keys.
    #[test]
    fn a_pointer_picks_a_shard() {
        let shards = vec![shard("A", true, true), shard("B", true, true), shard("C", true, true)];
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        let mut s = ServerSelect::default();
        let mut cx = UiCx::bare(&vui, &mut r);
        cx.state = 21;
        s.focus(&cx, &shards);
        s.fetching = false;
        s.sort(&shards);
        let mut reqs = Vec::new();
        s.draw(&mut cx, &shards, &mut reqs);
        // the rows as the draw lays them: rows 2.. from 58
        let at = |s: &ServerSelect, i: i32| {
            let r = crate::pointer::rows_rect(&s.window, &vui, &crate::pointer::testing::NullRaster, 58, i + 2, 402, 1);
            [(r[0] + r[2]) as f32 / 2.0, (r[1] + r[3]) as f32 / 2.0]
        };
        assert_eq!(s.point(at(&s, 2)), vec![key::DOWN, key::DOWN]);
        assert_eq!(s.point(at(&s, 0)), vec![key::CROSS]);
        let legend = crate::pointer::rows_rect(&s.window, &vui, &crate::pointer::testing::NullRaster, 40, 15, 0, 1);
        let x0 = (s.window.top_left()[0] + 40) as f32;
        assert_eq!(s.point([x0 + 4.0, (legend[1] + legend[3]) as f32 / 2.0]), vec![key::CROSS], "the legend's Select");
        s.hits.clear();
        assert!(s.point([100.0, 100.0]).is_empty(), "the control: no notes, nothing");
    }
}
