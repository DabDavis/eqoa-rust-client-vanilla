//! The scroll bar's blue knob, grabbed by a pointer (a mouse's left button, a
//! finger): press on the knob, move with it held, and the list or the history
//! goes with the pointer; let go ends it.
//!
//! One helper for every box that draws the knob. `Window::draw_vscroll` (and
//! its `draw_frame_vscroll`) tell the raster where each bar went
//! (`Raster::note_vscroll`); the drawing is the game's, untouched. What each
//! box does with a grab differs:
//!
//! - the chat box knows its own lines, and puts its view at the knob's
//!   place at once (`chat.rs`, `WndChat::bar_press`), and a press on the
//!   track above or below the knob pages the history as its keys do;
//! - any other window takes the pad's keys, so a grab here follows the
//!   pointer with Up and Down presses (`Follow`) until the knob it drew is
//!   under the pointer: a step a frame, however many rows the list has, and
//!   no window needs to know.
//!
//! The PS2 draws no such thing: the game's own scroll bars are only drawn.

use vi::ui::VBar;

/// How far (frame pixels) a press may miss the bar and still take it.
pub const SLOP: f32 = 3.0;

/// Where on a bar a press is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zone {
    Knob,
    /// the track above the knob
    Above,
    /// the track below the knob
    Below,
}

/// The knob's top edge: `t` of the way down the track less the knob's height.
pub fn knob_top(bar: &VBar) -> i32 {
    bar.at[1] + (bar.t * (bar.h - bar.knob[1]) as f32) as i32
}

/// What frame point `p` is on the bar, if anything: a bar with no knob
/// (`t` outside 0..1, nothing to scroll) is nothing.
pub fn zone(bar: &VBar, p: [f32; 2], slop: f32) -> Option<Zone> {
    if !(0.0..=1.0).contains(&bar.t) || !bar.t.is_finite() {
        return None;
    }
    // the knob is drawn one left of the bar and may be the wider
    let (x0, x1) = ((bar.at[0] - 1) as f32, (bar.at[0] - 1 + bar.w.max(bar.knob[0]).max(1)) as f32);
    let (y0, y1) = (bar.at[1] as f32, (bar.at[1] + bar.h) as f32);
    if p[0] < x0 - slop || p[0] >= x1 + slop || p[1] < y0 || p[1] >= y1 {
        return None;
    }
    let top = knob_top(bar) as f32;
    Some(if p[1] < top {
        Zone::Above
    } else if p[1] >= top + bar.knob[1] as f32 {
        Zone::Below
    } else {
        Zone::Knob
    })
}

/// The topmost bar (the last drawn) under `p`, with what part.
pub fn hit(bars: &[VBar], p: [f32; 2], slop: f32) -> Option<(VBar, Zone)> {
    bars.iter().rev().find_map(|b| zone(b, p, slop).map(|z| (*b, z)))
}

/// Where the knob would be (0 to 1) with the pointer at `y`, holding it
/// `grip` pixels below its top.
pub fn t_at(bar: &VBar, grip: f32, y: f32) -> f32 {
    let travel = (bar.h - bar.knob[1]).max(1) as f32;
    ((y - grip - bar.at[1] as f32) / travel).clamp(0.0, 1.0)
}

/// How far down the knob the pointer at `y` holds it.
pub fn grip_at(bar: &VBar, y: f32) -> f32 {
    y - knob_top(bar) as f32
}

/// What a grab on a window's bar asks of the window this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    /// nothing now
    Wait,
    /// a press of this pad key (`pointer::key::UP`, `DOWN`)
    Key(i32),
    /// the bar is gone (the window closed, or drew another): the grab ends
    Gone,
}

/// A window's bar held by the pointer: the knob is brought to where the
/// pointer holds it by the d-pad's steps, one a frame. The list is not
/// known, so the step is learned: how far the knob went for the last key.
#[derive(Clone, Copy, Debug)]
pub struct Follow {
    bar: VBar,
    grip: f32,
    target: f32,
    /// how far `t` went for the last key (0 until one is seen)
    step: f32,
    /// the knob's place when the last key went, and which way
    sent: Option<(f32, i32)>,
    /// the direction the list refused (the knob did not move)
    blocked: Option<i32>,
    settle: u8,
    missing: u8,
}

impl Follow {
    /// A press at `p` on the knob of one of `bars`: the grab.
    pub fn press(bars: &[VBar], p: [f32; 2], slop: f32) -> Option<Follow> {
        let (bar, zone) = hit(bars, p, slop)?;
        (zone == Zone::Knob).then(|| Follow { bar, grip: grip_at(&bar, p[1]), target: bar.t, step: 0.0, sent: None, blocked: None, settle: 0, missing: 0 })
    }

    /// The pointer moved to `p`.
    pub fn moved(&mut self, p: [f32; 2]) {
        self.target = t_at(&self.bar, self.grip, p[1]);
    }

    /// A frame on, the bars it drew: the key to press, if any. The knob
    /// drawn where the grab began is the one followed.
    pub fn tick(&mut self, bars: &[VBar]) -> Step {
        let Some(now) = bars.iter().find(|b| b.at == self.bar.at && b.h == self.bar.h) else {
            self.missing += 1;
            return if self.missing >= 3 { Step::Gone } else { Step::Wait };
        };
        self.missing = 0;
        if !(0.0..=1.0).contains(&now.t) {
            return Step::Wait;
        }
        if let Some((was, dir)) = self.sent.take() {
            let moved = (now.t - was).abs();
            if moved > 0.0 {
                self.step = moved;
                self.blocked = None;
            } else {
                self.blocked = Some(dir);
            }
        }
        if self.settle > 0 {
            self.settle -= 1;
            return Step::Wait;
        }
        let err = self.target - now.t;
        if err.abs() <= self.step * 0.5 || err == 0.0 {
            return Step::Wait;
        }
        let key = if err > 0.0 { crate::pointer::key::DOWN } else { crate::pointer::key::UP };
        if self.blocked == Some(key) {
            return Step::Wait;
        }
        self.sent = Some((now.t, key));
        // the knob shows a key's effect a frame on
        self.settle = 1;
        Step::Key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::key;

    fn bar(t: f32) -> VBar {
        VBar { at: [100, 20], h: 200, w: 5, knob: [7, 20], t }
    }

    #[test]
    fn zones_are_knob_above_below_and_the_slop_is_a_few_pixels() {
        let b = bar(0.5);
        // travel 180: the knob's top at 20 + 90
        assert_eq!(knob_top(&b), 110);
        assert_eq!(zone(&b, [102.0, 115.0], 0.0), Some(Zone::Knob));
        assert_eq!(zone(&b, [102.0, 60.0], 0.0), Some(Zone::Above));
        assert_eq!(zone(&b, [102.0, 200.0], 0.0), Some(Zone::Below));
        // the control: off the bar's side, and past its ends, nothing
        assert_eq!(zone(&b, [90.0, 115.0], 0.0), None);
        assert_eq!(zone(&b, [102.0, 10.0], 0.0), None);
        assert_eq!(zone(&b, [102.0, 221.0], 0.0), None);
        // a finger's miss of a couple of pixels still takes it
        assert_eq!(zone(&b, [96.0, 115.0], SLOP), Some(Zone::Knob));
        assert_eq!(zone(&b, [96.0, 115.0], 0.0), None);
        // no knob (nothing to scroll): not a bar to grab
        assert_eq!(zone(&bar(-1.0), [102.0, 115.0], SLOP), None);
        assert_eq!(zone(&bar(f32::NAN), [102.0, 115.0], SLOP), None);
    }

    #[test]
    fn the_knob_follows_the_pointer_by_the_grip() {
        let b = bar(0.5);
        let grip = grip_at(&b, 115.0);
        assert_eq!(grip, 5.0);
        // not moved: the knob stays
        assert!((t_at(&b, grip, 115.0) - 0.5).abs() < 0.01);
        // the pointer at the top and past it: 0; past the bottom: 1
        assert_eq!(t_at(&b, grip, 0.0), 0.0);
        assert_eq!(t_at(&b, grip, 1000.0), 1.0);
        // a quarter of the travel down
        assert!((t_at(&b, grip, 115.0 + 45.0) - 0.75).abs() < 0.01);
    }

    /// A list of `n` rows, `shown` visible, selection moved by the d-pad: the
    /// knob's place is what a window draws.
    struct List {
        n: i32,
        at: i32,
    }

    impl List {
        fn t(&self) -> f32 {
            self.at as f32 / (self.n - 1) as f32
        }
        fn key(&mut self, k: i32) {
            self.at = (self.at + if k == key::DOWN { 1 } else { -1 }).clamp(0, self.n - 1);
        }
    }

    /// Runs a grab for `frames` frames over `list`, the pointer dragged to `to`.
    fn drag(list: &mut List, to_y: f32, frames: usize) -> usize {
        let mut f = Follow::press(&[bar(list.t())], [102.0, knob_top(&bar(list.t())) as f32 + 5.0], SLOP).expect("on the knob");
        f.moved([102.0, to_y]);
        let mut keys = 0;
        for _ in 0..frames {
            match f.tick(&[bar(list.t())]) {
                Step::Key(k) => {
                    list.key(k);
                    keys += 1;
                }
                Step::Wait => {}
                Step::Gone => panic!("the bar was there"),
            }
        }
        keys
    }

    #[test]
    fn a_grab_brings_a_window_list_to_the_pointer_and_stops_there() {
        // 40 rows, selection at the top; pulled to the middle of the track
        let mut l = List { n: 40, at: 0 };
        // the knob's grip is 5 down it; the middle of the travel is y = 20 + 90 + 5
        let keys = drag(&mut l, 115.0, 200);
        assert!((l.t() - 0.5).abs() < 0.03, "stopped at the middle: {}", l.t());
        assert!(keys >= 18 && keys <= 24, "about 20 rows' keys, not a runaway: {keys}");
        // and back up past the top
        let keys_before = keys;
        let mut f = Follow::press(&[bar(l.t())], [102.0, knob_top(&bar(l.t())) as f32 + 5.0], SLOP).unwrap();
        f.moved([102.0, -50.0]);
        for _ in 0..200 {
            if let Step::Key(k) = f.tick(&[bar(l.t())]) {
                assert_eq!(k, key::UP);
                l.key(k);
            }
        }
        assert_eq!(l.at, 0, "pulled to the top");
        assert!(keys_before > 0);
    }

    #[test]
    fn a_short_list_does_not_hunt_between_two_rows() {
        // 4 rows: a step is a third of the track; the pointer rests between
        let mut l = List { n: 4, at: 0 };
        let mut f = Follow::press(&[bar(l.t())], [102.0, 25.0], SLOP).unwrap();
        f.moved([102.0, 20.0 + 5.0 + 90.0]);
        let (mut early, mut late) = (0, 0);
        for i in 0..100 {
            if let Step::Key(k) = f.tick(&[bar(l.t())]) {
                l.key(k);
                if i < 50 { early += 1 } else { late += 1 }
            }
        }
        assert!((1..=3).contains(&early), "it moved: {early}");
        assert_eq!(late, 0, "then rested, not a back and forth");
    }

    #[test]
    fn a_list_at_its_end_is_not_pushed_forever() {
        let mut l = List { n: 10, at: 9 };
        // the list refuses (a window whose rows do not move the knob)
        let mut f = Follow::press(&[bar(1.0)], [102.0, knob_top(&bar(1.0)) as f32 + 5.0], SLOP).unwrap();
        f.moved([102.0, 100.0]);
        let mut keys = 0;
        for _ in 0..50 {
            if let Step::Key(_) = f.tick(&[bar(1.0)]) {
                keys += 1;
            }
        }
        assert!(keys <= 1, "one try, then it waits: {keys}");
        let _ = &mut l;
    }

    #[test]
    fn a_grab_ends_when_its_bar_is_gone() {
        let mut f = Follow::press(&[bar(0.5)], [102.0, 115.0], SLOP).unwrap();
        assert_eq!(f.tick(&[]), Step::Wait);
        assert_eq!(f.tick(&[]), Step::Wait);
        assert_eq!(f.tick(&[]), Step::Gone);
        // the control: another bar elsewhere is not it
        let other = VBar { at: [300, 20], ..bar(0.5) };
        let mut f = Follow::press(&[bar(0.5)], [102.0, 115.0], SLOP).unwrap();
        for _ in 0..3 {
            let _ = f.tick(&[other]);
        }
        assert_eq!(f.tick(&[other]), Step::Gone);
    }

    #[test]
    fn a_press_off_the_knob_grabs_nothing() {
        assert!(Follow::press(&[bar(0.5)], [102.0, 60.0], SLOP).is_none(), "the track above");
        assert!(Follow::press(&[bar(0.5)], [400.0, 115.0], SLOP).is_none(), "off the bar");
    }
}
