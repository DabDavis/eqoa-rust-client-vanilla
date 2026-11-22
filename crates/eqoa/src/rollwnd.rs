//! The loot rolls' pop-up (a GM's petition 1448: under a raid's need
//! before greed, "for pc a pop up window that does not stop the persons
//! movement. It says "Need, green [greed], pass". People have 120 seconds to
//! choose ... be mindful of controller controls, so if the player holds R2 they
//! can hit right arrow to move to the pop up loot window and select their
//! roll"): the client's own, as the game has no roll. A strip a roll the
//! server offered (go-eqoa-server's 0x7E45, `net::raid::Roll`), at the frame's
//! right between the minimap and the chat box (over the quest tracker's place,
//! clear of the raid list's columns at the left and of the player's effects
//! along the top), the oldest at the top, [`SHOWN`] at once: the item's icon
//! and name, Need, Greed and Pass under them (Need dimmed where the server says
//! the player can neither wear nor use it), and the time left on a bar.
//! More than that open, the strips scroll (jibrish's entry 1757: every roll
//! can be answered, as each one's clock runs from its offer): the d-pad walks
//! past the last strip shown, the wheel (or a finger's drag) over them steps a
//! roll a notch, and "+N above" / "+N more" beside the first and the last
//! strips page by a click, red while a roll they hide has 15 s or less left.
//!
//! It never takes the keys of play. A click on a choice makes it (`point`);
//! with R2 held (the chat box's history, mode 33, where the walk and the camera
//! go on) right gives it the d-pad (`take_focus`): left and right the choice, up
//! and down the roll, ✕ makes it, △ or R2 let go hands the d-pad back. A choice
//! closes its strip at once and goes to the server (0x7E46), which rolls 0-100
//! for a Need or a Greed and says the rolls in raid chat; the server's end of a
//! roll (0 seconds left: decided, or its corpse gone) closes it too.
//! Its clock is the session's (ms), not the game's frames: it counts nothing
//! by frames.

use net::dump::NetItem;
use net::raid::{Choice, Roll};
use vi::ui::{FrameParam, Raster, Vui, icon_id, wide};

/// The most strips drawn at once; the rest are scrolled to (a line beside the
/// first and the last says how many are above and below).
pub const SHOWN: usize = 3;

/// The first strip's top left (the 4:3 frame's, drawn in the middle anchor:
/// under the minimap's 162, right of the raid list's fourth column's 432, the
/// last strip above the chat box's 331), each strip's size and the space
/// between; where the name goes and how wide it may be, the choices' line and
/// middles.
const AT: [i32; 2] = [436, 164];
const W: i32 = 196;
const H: i32 = 52;
const GAP: i32 = 3;
const NAME_X: i32 = 68;
const NAME_W: i32 = 120;
const CHOICE_Y: i32 = 33;
const CHOICE_X: [i32; 3] = [33, 98, 163];

/// The choices, left to right.
const CHOICES: [(&str, Choice); 3] = [("Need", Choice::Need), ("Greed", Choice::Greed), ("Pass", Choice::Pass)];

/// A roll offered and not answered.
#[derive(Clone, Debug)]
pub struct Open {
    pub id: u32,
    /// when it ends, on the session's clock (ms), and how long it was offered
    /// for (the bar's whole)
    pub ends: i64,
    pub length: i64,
    pub may_need: bool,
    pub item: NetItem,
}

impl Open {
    /// Choice `k` (of `CHOICES`) can be made.
    fn may(&self, k: usize) -> bool {
        k != 0 || self.may_need
    }

    /// The seconds left at `now`, rounded up.
    fn seconds(&self, now: i64) -> i64 {
        ((self.ends - now).max(0) + 999) / 1000
    }
}

#[derive(Default)]
pub struct Rolls {
    pub open: Vec<Open>,
    /// the d-pad's (`take_focus`)
    focused: bool,
    /// the roll and the choice lit while focused
    sel: usize,
    choice: usize,
    /// where each choice was drawn last (frame x0 y0 x1 y1), its roll and choice
    hits: Vec<([i32; 4], usize, usize)>,
    /// strips' places left free at the top (the ready check's, `readywnd`):
    /// the rolls start that many lower and that many fewer are shown
    pub skip: usize,
    /// the index in `open` of the top strip shown (`first()` keeps it valid)
    first: usize,
    /// where the counts were drawn last (frame x0 y0 x1 y1), and which way they
    /// page (-1 up, 1 down)
    count_hits: Vec<([i32; 4], i32)>,
    /// the wheel's or a finger's lines not yet a whole notch
    wheel_left: f32,
}

/// A roll's seconds left at or under which its bar (and a count that hides
/// it) is red.
const RED_SECONDS: i64 = 15;

impl Rolls {
    /// A roll from the server at `now`: a new one goes last, one of the same id
    /// is replaced, and one of 0 seconds left closes.
    pub fn roll(&mut self, r: Roll, now: i64) {
        let Some(item) = r.item.filter(|_| r.seconds > 0) else {
            self.close(r.id);
            return;
        };
        let length = r.seconds as i64 * 1000;
        let open = Open { id: r.id, ends: now + length, length, may_need: r.may_need, item: *item };
        match self.open.iter_mut().find(|o| o.id == r.id) {
            Some(o) => *o = open,
            None => self.open.push(open),
        }
    }

    /// Roll `id` closed (answered, or the server's end of it).
    pub fn close(&mut self, id: u32) {
        let Some(i) = self.open.iter().position(|o| o.id == id) else { return };
        self.open.remove(i);
        // the lit roll and the window keep to what they showed: one closed
        // above them moves them up with the rest
        if i < self.sel {
            self.sel -= 1;
        }
        if i < self.first {
            self.first -= 1;
        }
        self.sel = self.sel.min(self.open.len().saturating_sub(1));
        self.settle();
        if self.open.is_empty() {
            self.focused = false;
        }
    }

    /// The top strip's index in `open`: kept so the window is full while
    /// enough are open (the last ones closed pull it up).
    fn first(&self) -> usize {
        self.first.min(self.open.len().saturating_sub(self.shown()))
    }

    /// `first` made valid (after a close, a roll, a `skip` changed), and the
    /// lit roll, while focused, kept in the window.
    fn settle(&mut self) {
        self.first = self.first();
        if self.focused && self.is_open() {
            let last = self.first + self.shown() - 1;
            let to = self.sel.clamp(self.first, last);
            if to != self.sel {
                self.sel = to;
                self.choice = self.first_choice(to);
            }
        }
    }

    /// The window moved by `by` rolls (up negative), kept in `open`.
    fn scroll(&mut self, by: i64) {
        let room = self.open.len().saturating_sub(self.shown()) as i64;
        self.first = (self.first() as i64 + by).clamp(0, room) as usize;
        self.settle();
    }

    /// The wheel's `lines` (up positive; a finger's drag down) at frame point
    /// `p`: a roll a whole notch when it is over the strips. False when it is
    /// not (the camera's zoom, then).
    pub fn wheel(&mut self, p: [f32; 2], lines: f32) -> bool {
        if !self.covers(p) {
            self.wheel_left = 0.0;
            return false;
        }
        self.wheel_left += lines;
        let notches = self.wheel_left.trunc();
        self.wheel_left -= notches;
        if notches != 0.0 {
            self.scroll(-(notches as i64));
        }
        true
    }

    /// Those past their time by more than a few seconds go (the server ends
    /// every roll itself; this only keeps a lost end from standing for good).
    pub fn expire(&mut self, now: i64) {
        let gone: Vec<u32> = self.open.iter().filter(|o| now > o.ends + 3000).map(|o| o.id).collect();
        for id in gone {
            self.close(id);
        }
    }

    pub fn is_open(&self) -> bool {
        !self.open.is_empty()
    }

    pub fn focused(&self) -> bool {
        self.focused && self.is_open()
    }

    /// The d-pad to the strips (R2 held, then right): the lit roll the first
    /// shown, its first choice that can be made. False when none is up.
    pub fn take_focus(&mut self) -> bool {
        if !self.is_open() {
            return false;
        }
        self.focused = true;
        let first = self.first();
        self.first = first;
        self.sel = self.sel.clamp(first, first + self.shown() - 1);
        self.choice = self.first_choice(self.sel);
        true
    }

    /// The d-pad handed back (R2 let go, △).
    pub fn let_go(&mut self) {
        self.focused = false;
    }

    /// The item of the lit roll while focused (its description beside the
    /// strips).
    pub fn lit_item(&self) -> Option<&NetItem> {
        self.focused().then(|| self.open.get(self.sel).map(|o| &o.item)).flatten()
    }

    fn shown(&self) -> usize {
        self.open.len().min(SHOWN - self.skip.min(SHOWN - 1))
    }

    /// The first strip's top (`skip` places down).
    fn top(&self) -> i32 {
        AT[1] + self.skip.min(SHOWN - 1) as i32 * (H + GAP)
    }

    fn first_choice(&self, roll: usize) -> usize {
        self.open.get(roll).map_or(0, |o| (0..3).find(|&k| o.may(k)).unwrap_or(1))
    }

    /// Roll `k`'s choice `c` made: its strip gone, and what to send.
    fn make(&mut self, k: usize, c: usize) -> Option<(u32, Choice)> {
        let o = self.open.get(k).filter(|o| o.may(c))?;
        let made = (o.id, CHOICES[c].1);
        self.close(made.0);
        if self.focused {
            self.choice = self.first_choice(self.sel);
        }
        Some(made)
    }

    /// A key (the game's codes; the keyboard's arrows and Enter as the pad's)
    /// while focused: what to send when a choice was made. Unfocused, nothing.
    pub fn key(&mut self, key: i32, cues: &mut Vec<i32>) -> Option<(u32, Choice)> {
        if !self.focused() {
            return None;
        }
        let step = |cues: &mut Vec<i32>, moved: bool| cues.push(if moved { 11 } else { 9 });
        match key {
            136 | 17 => {
                let o = &self.open[self.sel];
                let to = (0..self.choice).rev().find(|&k| o.may(k));
                step(cues, to.is_some());
                self.choice = to.unwrap_or(self.choice);
                None
            }
            137 | 18 => {
                let o = &self.open[self.sel];
                let to = (self.choice + 1..3).find(|&k| o.may(k));
                step(cues, to.is_some());
                self.choice = to.unwrap_or(self.choice);
                None
            }
            138 | 19 => {
                step(cues, self.sel > 0);
                if self.sel > 0 {
                    self.sel -= 1;
                    self.choice = self.first_choice(self.sel);
                    // above the first shown: the window follows
                    self.first = self.first().min(self.sel);
                }
                None
            }
            139 | 20 => {
                // past the last shown the walk goes on into those below
                let more = self.sel + 1 < self.open.len();
                step(cues, more);
                if more {
                    self.sel += 1;
                    self.choice = self.first_choice(self.sel);
                    self.first = self.first().max((self.sel + 1).saturating_sub(self.shown()));
                }
                None
            }
            134 | 13 => {
                cues.push(13);
                self.make(self.sel, self.choice)
            }
            132 | 27 => {
                cues.push(8);
                self.let_go();
                None
            }
            _ => None,
        }
    }

    /// A pointer at frame point `p`: a choice that can be made, made (focused
    /// or not); anything else nothing.
    pub fn point(&mut self, p: [f32; 2]) -> Option<(u32, Choice)> {
        let (x, y) = (p[0] as i32, p[1] as i32);
        // a count: a page that way
        if let Some(&(_, dir)) = self.count_hits.iter().find(|(r, _)| (r[0]..r[2]).contains(&x) && (r[1]..r[3]).contains(&y)) {
            self.scroll(dir as i64 * self.shown() as i64);
            return None;
        }
        let &(_, k, c) = self.hits.iter().find(|(r, _, _)| (r[0]..r[2]).contains(&x) && (r[1]..r[3]).contains(&y))?;
        self.make(k, c)
    }

    /// Whether frame point `p` is on a strip (a click there is the strips', not
    /// the world's).
    pub fn covers(&self, p: [f32; 2]) -> bool {
        let n = self.shown() as i32;
        let on_count = self.count_hits.iter().any(|(r, _)| p[0] >= r[0] as f32 && p[0] < r[2] as f32 && p[1] >= r[1] as f32 && p[1] < r[3] as f32);
        on_count || n > 0 && p[0] >= AT[0] as f32 && p[0] < (AT[0] + W) as f32 && p[1] >= self.top() as f32 && p[1] < (self.top() + n * (H + GAP)) as f32
    }

    /// The strips at `now` (the session's clock), each choice's place noted.
    /// `icon` is an item's surface (`inventory::item_icon`).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, now: i64, icon: &dyn Fn(&NetItem) -> i32) {
        self.hits.clear();
        self.count_hits.clear();
        self.settle();
        let (white, yellow, dim) = ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 0.0, 1.0], [0.55, 0.55, 0.55, 0.7]);
        let fh = vui.font_height(r);
        let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) };
        let (shown, top, first) = (self.shown(), self.top(), self.first());
        for (k, o) in self.open.iter().enumerate().skip(first).take(shown) {
            let lit_roll = self.focused && k == self.sel;
            let at = [AT[0], top + (k - first) as i32 * (H + GAP)];
            r.blit(icon_id("black"), 0, at, [0, 0], [W, H], 3, if lit_roll { 0.85 } else { 0.7 });
            vui.blit_frame(r, at, [W, H], &fp, 3, 1.0, true);
            // the item: its icon at half size, its name cut to its room
            let id = icon(&o.item);
            if r.has(id as u32) {
                r.stretch_blit(id as u32, 0, [at[0] + 6, at[1] + 5], [57, 22], [0, 0], [114, 44], 3, 1.0);
            }
            let mut name = wide(&o.item.item.name);
            while !name.is_empty() && vui.string_width(r, &name) > NAME_W {
                name.pop();
            }
            vui.draw_string_ex(r, &name, at[0] + NAME_X, at[1] + 7, if lit_roll { yellow } else { white }, true);
            // the choices
            let y = at[1] + CHOICE_Y;
            for (c, (label, _)) in CHOICES.iter().enumerate() {
                let x = at[0] + CHOICE_X[c];
                let s = wide(label);
                let lit = lit_roll && c == self.choice;
                let color = if !o.may(c) { dim } else if lit { yellow } else { white };
                vui.draw_centered_string_ex(r, &s, x, y, color, 3, true);
                let half = vui.string_width(r, &s) / 2;
                if lit {
                    vui.blit_ex(r, 89, [x - half - 20, y - 5], 3, 1.0, 0);
                }
                if o.may(c) {
                    self.hits.push(([x - half - 8, y - fh / 2 - 3, x + half + 8, y + fh / 2 + 3], k, c));
                }
            }
            // the time left: a bar along the strip's foot, red in its last 15 s
            let left = (o.ends - now).clamp(0, o.length);
            let full = W - 16;
            let w = if o.length > 0 { (full as i64 * left / o.length) as i32 } else { 0 };
            let (x0, y0) = (at[0] + 8, at[1] + H - 6);
            r.fill_fan([0.15, 0.15, 0.15, 1.0], [[x0, y0], [x0 + full, y0], [x0 + full, y0 + 2], [x0, y0 + 2]]);
            if w > 0 {
                let c = if o.seconds(now) <= RED_SECONDS { [0.95, 0.25, 0.2, 1.0] } else { [1.0, 0.55, 0.0, 1.0] };
                r.fill_fan(c, [[x0, y0], [x0 + w, y0], [x0 + w, y0 + 2], [x0, y0 + 2]]);
            }
        }
        // those scrolled past: their counts left of the first strip (above)
        // and of the last (below; under it is the chat box), each a click to
        // page that way, red while one it hides is about to lapse
        let red = [0.95, 0.25, 0.2, 1.0];
        let below = first + shown;
        for (dir, hidden, slot, word) in [(-1, 0..first, 0, "above"), (1, below.min(self.open.len())..self.open.len(), shown.max(1) - 1, "more")] {
            if hidden.is_empty() {
                continue;
            }
            let lapsing = self.open[hidden.clone()].iter().any(|o| o.seconds(now) <= RED_SECONDS);
            let s = wide(&format!("+{} {word}", hidden.len()));
            let y = top + slot as i32 * (H + GAP) + H / 2 - fh / 2;
            let w = vui.string_width(r, &s);
            let x = AT[0] - 6 - w;
            vui.draw_string_ex(r, &s, x, y, if lapsing { red } else { white }, true);
            self.count_hits.push(([x - 4, y - 4, AT[0], y + fh + 4], dir));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, vui};

    fn item(name: &str) -> Box<NetItem> {
        let mut it = NetItem::default();
        it.item.name = name.to_string();
        Box::new(it)
    }

    fn roll(id: u32, seconds: u16, may_need: bool) -> Roll {
        Roll { id, seconds, may_need, item: Some(item(&format!("item {id}"))) }
    }

    #[test]
    fn rolls_open_replace_and_close() {
        let mut r = Rolls::default();
        r.roll(roll(3, 120, true), 1000);
        r.roll(roll(4, 120, false), 2000);
        assert_eq!(r.open.iter().map(|o| o.id).collect::<Vec<_>>(), [3, 4]);
        assert_eq!(r.open[1].ends, 122_000);
        r.roll(roll(3, 60, true), 5000);
        assert_eq!((r.open.len(), r.open[0].ends), (2, 65_000), "the same id replaced in place");
        r.roll(Roll { id: 3, ..Default::default() }, 6000);
        assert_eq!(r.open.iter().map(|o| o.id).collect::<Vec<_>>(), [4], "0 seconds: closed");
        // a roll of 0 seconds that was never open is no strip (the control)
        r.roll(Roll { id: 9, seconds: 0, item: Some(item("x")), ..Default::default() }, 6000);
        assert_eq!(r.open.len(), 1);
        r.expire(122_000 + 3000);
        assert_eq!(r.open.len(), 1, "its time and a little more");
        r.expire(122_000 + 3001);
        assert!(!r.is_open(), "then gone");
    }

    #[test]
    fn the_dpad_only_while_focused_and_need_skipped_when_dimmed() {
        let mut r = Rolls::default();
        let mut cues = Vec::new();
        r.roll(roll(1, 120, false), 0);
        r.roll(roll(2, 120, true), 0);
        // unfocused: no key is the strips' (the control)
        assert_eq!(r.key(134, &mut cues), None);
        assert_eq!(r.open.len(), 2);
        assert!(r.take_focus());
        assert_eq!(r.choice, 1, "Need dimmed: Greed lit first");
        r.key(136, &mut cues);
        assert_eq!((r.choice, cues.last()), (1, Some(&9)), "no step onto a dimmed Need");
        r.key(137, &mut cues);
        assert_eq!(r.choice, 2);
        r.key(139, &mut cues);
        assert_eq!((r.sel, r.choice), (1, 0), "the next roll, its Need");
        assert_eq!(r.key(134, &mut cues), Some((2, Choice::Need)));
        assert_eq!(r.open.iter().map(|o| o.id).collect::<Vec<_>>(), [1], "closed at once");
        assert!(r.focused(), "still focused while one is up");
        assert_eq!(r.key(134, &mut cues), Some((1, Choice::Greed)));
        assert!(!r.focused() && !r.is_open(), "the last one made: the d-pad back");
        // △ lets go and makes nothing
        r.roll(roll(5, 120, true), 0);
        r.take_focus();
        assert_eq!(r.key(132, &mut cues), None);
        assert!(!r.focused() && r.is_open());
        assert!(!Rolls::default().take_focus(), "none up: nothing to focus");
    }

    /// A click on a choice makes it where it was drawn; a dimmed Need is no
    /// choice, and nothing is found before a frame drew the strips (the
    /// control: no hit notes).
    #[test]
    fn a_pointer_makes_a_choice_where_it_was_drawn() {
        let (vui, mut nr) = (vui(), NullRaster);
        let mut r = Rolls::default();
        r.roll(roll(1, 120, false), 0);
        r.roll(roll(2, 120, true), 0);
        let greed_of = |k: i32| [(AT[0] + CHOICE_X[1]) as f32, (AT[1] + k * (H + GAP) + CHOICE_Y) as f32];
        let need_of = |k: i32| [(AT[0] + CHOICE_X[0]) as f32, (AT[1] + k * (H + GAP) + CHOICE_Y) as f32];
        assert_eq!(r.point(greed_of(0)), None, "not drawn yet");
        r.draw(&vui, &mut nr, 1000, &|_| 0);
        assert!(r.covers(greed_of(1)) && !r.covers([10.0, 10.0]));
        assert_eq!(r.point(need_of(0)), None, "a dimmed Need");
        assert_eq!(r.point(need_of(1)), Some((2, Choice::Need)));
        r.draw(&vui, &mut nr, 1000, &|_| 0);
        assert_eq!(r.point(greed_of(0)), Some((1, Choice::Greed)));
        assert!(!r.is_open());
    }

    /// A ready check's strip up (`skip` 1): the rolls a strip lower, one
    /// fewer shown; at 0 where they were (the control).
    #[test]
    fn the_rolls_make_room_for_the_ready_check() {
        let (vui, mut nr) = (vui(), NullRaster);
        let mut r = Rolls::default();
        for id in 1..=4 {
            r.roll(roll(id, 120, true), 0);
        }
        let greed = |slot: i32| [(AT[0] + CHOICE_X[1]) as f32, (AT[1] + slot * (H + GAP) + CHOICE_Y) as f32];
        r.skip = 1;
        r.draw(&vui, &mut nr, 1000, &|_| 0);
        assert!(!r.covers(greed(0)), "the ready check's place is not the rolls'");
        assert!(r.covers(greed(2)) && r.shown() == SHOWN - 1);
        assert_eq!(r.point(greed(0)), None);
        assert_eq!(r.point(greed(1)), Some((1, Choice::Greed)), "the first roll a strip lower");
        r.skip = 0;
        r.draw(&vui, &mut nr, 1000, &|_| 0);
        assert_eq!(r.point(greed(0)), Some((2, Choice::Greed)));
    }

    /// A raster that keeps each string drawn with its colour.
    #[derive(Default)]
    struct Colours(Vec<(String, [f32; 4])>);

    impl crate::ui::Layout for Colours {}

    impl Raster for Colours {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], c: [f32; 4], s: &[u16]) {
            self.0.push((String::from_utf16_lossy(s), c));
        }
        fn draw_centered_string(&mut self, _: [i32; 2], c: [f32; 4], s: &[u16], _: u32) {
            self.0.push((String::from_utf16_lossy(s), c));
        }
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            16
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 640, 448]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    impl Colours {
        /// The colour a string starting `with` was drawn in, if it was (its
        /// last drawing: the shadow is drawn first, in black).
        fn of(&self, with: &str) -> Option<[f32; 4]> {
            self.0.iter().rev().find(|(s, _)| s.starts_with(with)).map(|(_, c)| *c)
        }
        fn has(&self, s: &str) -> bool {
            self.0.iter().any(|(t, _)| t == s)
        }
    }

    fn ids(r: &Rolls) -> Vec<u32> {
        r.open.iter().map(|o| o.id).collect()
    }

    /// Past `SHOWN` the d-pad walks on into those below and the window
    /// follows; up walks back to the first. The cue is 9 only at either end.
    /// (Before entry 1757 the walk stopped at the third strip.)
    #[test]
    fn every_roll_is_reached_by_the_dpad() {
        let mut r = Rolls::default();
        let mut cues = Vec::new();
        for id in 1..=6 {
            r.roll(roll(id, 120, true), 0);
        }
        r.take_focus();
        for _ in 0..5 {
            r.key(139, &mut cues);
        }
        assert_eq!((r.sel, r.first()), (5, 3), "roll 6 lit, the window at 4..6");
        assert!(cues.iter().all(|&c| c == 11), "every step moved: {cues:?}");
        r.key(139, &mut cues);
        assert_eq!(cues.last(), Some(&9), "the real end");
        assert_eq!(r.key(134, &mut cues), Some((6, Choice::Need)));
        assert_eq!((r.open.len(), r.first(), r.sel), (5, 2, 4), "the window stays full after the close");
        cues.clear();
        for _ in 0..4 {
            r.key(138, &mut cues);
        }
        assert_eq!((r.sel, r.first()), (0, 0), "back up to roll 1");
        assert!(cues.iter().all(|&c| c == 11));
        r.key(138, &mut cues);
        assert_eq!(cues.last(), Some(&9));
    }

    /// The counts: "+N above" left of the first strip and "+N more" left of
    /// the last, a click on one a page that way and never a choice.
    #[test]
    fn a_click_on_a_count_pages() {
        let vui = vui();
        let mut r = Rolls::default();
        for id in 1..=7 {
            r.roll(roll(id, 120, true), 0);
        }
        let mut c = Colours::default();
        r.draw(&vui, &mut c, 1000, &|_| 0);
        assert!(c.has("+4 more") && !c.0.iter().any(|(s, _)| s.ends_with("above")));
        let (more, _) = r.count_hits[0];
        let at = [(more[0] + 2) as f32, (more[1] + 6) as f32];
        assert!(r.covers(at), "the count is the strips', not the world's");
        assert_eq!(r.point(at), None, "a count makes no choice");
        assert_eq!((r.first(), r.open.len()), (3, 7), "a page down");
        let mut c = Colours::default();
        r.draw(&vui, &mut c, 1000, &|_| 0);
        assert!(c.has("+3 above") && c.has("+1 more"), "{:?}", c.0);
        // "+N above" is the first count drawn now
        let (above, dir) = r.count_hits[0];
        assert_eq!(dir, -1);
        assert_eq!(r.point([(above[0] + 2) as f32, (above[1] + 6) as f32]), None);
        assert_eq!(r.first(), 0, "a page up");
        // the control: a click left of the strips, off the counts, is not theirs
        r.draw(&vui, &mut NullRaster, 1000, &|_| 0);
        assert!(!r.covers([(AT[0] - 150) as f32, (AT[1] + 2) as f32]));
    }

    /// A count is red while a roll it hides has 15 s or less left; white
    /// while every one it hides has more (the control).
    #[test]
    fn a_count_turns_red_for_a_lapsing_roll() {
        let vui = vui();
        let red = [0.95, 0.25, 0.2, 1.0];
        let mut r = Rolls::default();
        for id in 1..=3 {
            r.roll(roll(id, 120, true), 0);
        }
        r.roll(roll(4, 30, true), 0); // hidden below, ends at 30 s
        r.roll(roll(5, 120, true), 0);
        let mut c = Colours::default();
        r.draw(&vui, &mut c, 14_000, &|_| 0);
        assert_ne!(c.of("+2 more"), Some(red), "16 s left: white");
        let mut c = Colours::default();
        r.draw(&vui, &mut c, 16_000, &|_| 0);
        assert_eq!(c.of("+2 more"), Some(red), "14 s left: red");
        // scrolled past it, the count above it turns red instead
        r.scroll(2);
        let mut c = Colours::default();
        r.draw(&vui, &mut c, 16_000, &|_| 0);
        assert_ne!(c.of("+2 above"), Some(red), "rolls 1 and 2 have time");
        assert!(c.of("+").is_some());
        r.scroll(-2);
        r.roll(roll(1, 10, true), 16_000);
        r.scroll(1);
        let mut c = Colours::default();
        r.draw(&vui, &mut c, 16_000, &|_| 0);
        assert_eq!(c.of("+1 above"), Some(red), "roll 1 at 10 s, above");
    }

    /// A ready check up (`skip` 1): two shown of five, and the d-pad still
    /// reaches roll 5.
    #[test]
    fn under_a_ready_check_every_roll_is_reached() {
        let mut r = Rolls::default();
        let mut cues = Vec::new();
        for id in 1..=5 {
            r.roll(roll(id, 120, true), 0);
        }
        r.skip = 1;
        r.take_focus();
        for _ in 0..4 {
            r.key(139, &mut cues);
        }
        assert_eq!((r.shown(), r.sel, r.first()), (2, 4, 3));
        assert_eq!(r.key(134, &mut cues), Some((5, Choice::Need)));
    }

    /// The last rolls closed while scrolled to the bottom pull the window
    /// up: no empty strip is drawn. The control: closed above, the window
    /// keeps the same rolls.
    #[test]
    fn closing_the_last_rolls_pulls_the_window_up() {
        let vui = vui();
        let mut r = Rolls::default();
        for id in 1..=6 {
            r.roll(roll(id, 120, true), 0);
        }
        r.scroll(10);
        assert_eq!(r.first(), 3);
        r.close(6);
        r.close(5);
        assert_eq!(r.first(), 1);
        let mut c = Colours::default();
        r.draw(&vui, &mut c, 0, &|_| 0);
        assert!(c.has("item 2") && c.has("item 4") && !c.has("item 1") && c.has("+1 above"), "{:?}", c.0);
        r.close(1);
        assert_eq!((r.first(), ids(&r)), (0, vec![2, 3, 4]), "closed above: the same three shown");
    }

    /// The wheel over the strips steps a roll a notch (up positive) and is
    /// taken; off them it is not (the camera's).
    #[test]
    fn the_wheel_over_the_strips_scrolls() {
        let vui = vui();
        let mut r = Rolls::default();
        for id in 1..=5 {
            r.roll(roll(id, 120, true), 0);
        }
        r.draw(&vui, &mut NullRaster, 0, &|_| 0);
        let on = [(AT[0] + 10) as f32, (AT[1] + 10) as f32];
        assert!(r.wheel(on, -1.0));
        assert_eq!(r.first(), 1);
        assert!(r.wheel(on, -0.5) && r.first() == 1, "half a notch waits");
        assert!(r.wheel(on, -0.5) && r.first() == 2);
        assert!(r.wheel(on, -3.0) && r.first() == 2, "kept at the bottom");
        assert!(r.wheel(on, 5.0) && r.first() == 0);
        assert!(!r.wheel([10.0, 10.0], -1.0), "off the strips: not theirs");
        assert_eq!(r.first(), 0);
    }
}
