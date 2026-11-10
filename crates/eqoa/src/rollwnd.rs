//! The loot rolls' pop-up (the GM Jibrish's petition 1448: under a raid's need
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

/// The most strips drawn at once; the rest wait their turn (a line beside the
/// last says how many).
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
}

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
        self.open.retain(|o| o.id != id);
        self.sel = self.sel.min(self.open.len().saturating_sub(1));
        if self.open.is_empty() {
            self.focused = false;
        }
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
        self.sel = self.sel.min(self.shown() - 1);
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
        self.open.len().min(SHOWN)
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
                }
                None
            }
            139 | 20 => {
                let more = self.sel + 1 < self.shown();
                step(cues, more);
                if more {
                    self.sel += 1;
                    self.choice = self.first_choice(self.sel);
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
        let &(_, k, c) = self.hits.iter().find(|(r, _, _)| (r[0]..r[2]).contains(&x) && (r[1]..r[3]).contains(&y))?;
        self.make(k, c)
    }

    /// Whether frame point `p` is on a strip (a click there is the strips', not
    /// the world's).
    pub fn covers(&self, p: [f32; 2]) -> bool {
        let n = self.shown() as i32;
        n > 0 && p[0] >= AT[0] as f32 && p[0] < (AT[0] + W) as f32 && p[1] >= AT[1] as f32 && p[1] < (AT[1] + n * (H + GAP)) as f32
    }

    /// The strips at `now` (the session's clock), each choice's place noted.
    /// `icon` is an item's surface (`inventory::item_icon`).
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, now: i64, icon: &dyn Fn(&NetItem) -> i32) {
        self.hits.clear();
        let (white, yellow, dim) = ([1.0, 1.0, 1.0, 1.0], [1.0, 1.0, 0.0, 1.0], [0.55, 0.55, 0.55, 0.7]);
        let fh = vui.font_height(r);
        let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) };
        for (k, o) in self.open.iter().take(SHOWN).enumerate() {
            let lit_roll = self.focused && k == self.sel;
            let at = [AT[0], AT[1] + k as i32 * (H + GAP)];
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
                let c = if o.seconds(now) <= 15 { [0.95, 0.25, 0.2, 1.0] } else { [1.0, 0.55, 0.0, 1.0] };
                r.fill_fan(c, [[x0, y0], [x0 + w, y0], [x0 + w, y0 + 2], [x0, y0 + 2]]);
            }
        }
        // more waiting: their count left of the last strip (under it is the chat box)
        if self.open.len() > SHOWN {
            let s = wide(&format!("+{} more", self.open.len() - SHOWN));
            let y = AT[1] + (SHOWN as i32 - 1) * (H + GAP) + H / 2 - fh / 2;
            let w = vui.string_width(r, &s);
            vui.draw_string_ex(r, &s, AT[0] - 6 - w, y, white, true);
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

    /// Past `SHOWN` the rest wait: the d-pad steps only over those drawn.
    #[test]
    fn only_the_first_rolls_are_shown_and_lit() {
        let mut r = Rolls::default();
        let mut cues = Vec::new();
        for id in 1..=6 {
            r.roll(roll(id, 120, true), 0);
        }
        r.take_focus();
        for _ in 0..10 {
            r.key(139, &mut cues);
        }
        assert_eq!(r.sel, SHOWN - 1);
        assert_eq!(r.key(134, &mut cues), Some((SHOWN as u32, Choice::Need)));
        assert_eq!(r.open.len(), 5);
    }
}
