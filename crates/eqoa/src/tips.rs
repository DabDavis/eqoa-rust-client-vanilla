//! The loading screens' tips (the GM Jibrish's petition 1778, 2026-11-12: "Add
//! text tips to the loading screens. Create one for each command, with a brief
//! one sentence description of the command ... Create one hint that loads on a
//! loading screen that is randomly displayed when they load either into the
//! game, logging out or when zoning / coaching / rezzing etc."). The client's
//! own: the game's loading screen (`loading::draw_2d`) has its bar and gears
//! alone.
//!
//! The tips are `tips.txt`, one sentence a line, built into the client. Each
//! loading screen shows one of them, picked at random as it comes up and kept
//! while it is up (a screen is the draws that follow each other within `GAP`),
//! never the one the last screen showed; it sits under the bar, wrapped to
//! `WIDTH`, in the 4:3 frame's middle (kept there in a wider or a taller
//! window, as the screen's 2D half is). Extended > Other > General's Loading
//! Tips turns them off.

use crate::ui::Layout;
use std::cell::Cell;
use vi::ui::{Vui, wide};

/// The tips, as built in.
const TEXT: &str = include_str!("tips.txt");

/// Every tip, in the file's order: its lines but the notes and the blanks.
pub fn all() -> Vec<&'static str> {
    TEXT.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect()
}

/// Draws further apart than this are two screens (the screen draws every game
/// frame, 33 ms, and every 30 ms while a load holds the frame).
pub const GAP: i64 = 1500;

/// The most a tip's line is across, in frame pixels: clear of the gears in the
/// bottom right corner (`loading::CORNER`, from x 500) on a 4:3 screen.
pub const WIDTH: i32 = 380;

/// The tip's colour: the bar's words' warm white.
const COLOR: [f32; 4] = [1.0, 0.92, 0.7, 1.0];

thread_local! {
    /// The tip shown, and when it was last drawn (ms on `clock`).
    static SHOWN: Cell<Option<(usize, i64)>> = const { Cell::new(None) };
}

/// Milliseconds since this thread first asked (web_time's: a page has no std clock).
fn clock() -> i64 {
    thread_local! {
        static START: web_time::Instant = web_time::Instant::now();
    }
    START.with(|s| s.elapsed().as_millis() as i64)
}

/// A number to pick with: the wall clock's nanoseconds mixed (splitmix64).
fn seed() -> u64 {
    let n = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
    let mut z = n.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The tip for a draw at `now` (ms) of `n` tips: the one shown while the draws
/// follow each other within `GAP`, else a new one out of `roll`, never the
/// last one again when there are two or more.
pub fn pick(shown: Option<(usize, i64)>, now: i64, n: usize, roll: u64) -> usize {
    match shown {
        Some((i, at)) if now - at <= GAP && now >= at && i < n => i,
        _ if n < 2 => 0,
        Some((last, _)) if last < n => {
            let i = (roll % (n as u64 - 1)) as usize;
            if i >= last { i + 1 } else { i }
        }
        _ => (roll % n as u64) as usize,
    }
}

/// `text` in lines of at most `width` as `measure` measures them, broken at
/// spaces (a word wider than the line alone on its own).
pub fn wrap(text: &str, width: i32, measure: impl Fn(&str) -> i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let next = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if !line.is_empty() && measure(&next) > width {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        } else {
            line = next;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// This screen's tip drawn from `y` down, centred in the frame (the 2D half's
/// anchor), with Loading Tips on.
pub fn draw(vui: &Vui, r: &mut dyn Layout, y: i32) {
    if !crate::settings::loading_tips() {
        return;
    }
    let tips = all();
    if tips.is_empty() {
        return;
    }
    let now = clock();
    let i = pick(SHOWN.get(), now, tips.len(), seed());
    SHOWN.set(Some((i, now)));
    let h = vui.font_height(r);
    for (k, line) in wrap(tips[i], WIDTH, |s| vui.string_width(r, &wide(s))).iter().enumerate() {
        vui.draw_centered_string_ex(r, &wide(line), 320, y + k as i32 * h, COLOR, 0, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every tip is one sentence in the petition's form, and none is a GM's.
    #[test]
    fn every_tip_is_a_did_you_know() {
        let t = all();
        assert!(t.len() >= 60, "{} tips", t.len());
        for l in &t {
            assert!(l.starts_with("Did you know"), "{l}");
            assert!(l.ends_with('?'), "{l}");
            assert!(!l.contains('#'), "a GM's command: {l}");
            assert!(l.len() < 160, "too long for three lines: {l}");
        }
        let mut seen = std::collections::HashSet::new();
        assert!(t.iter().all(|l| seen.insert(*l)), "a tip twice");
        for c in ["/help", "/stuck", "/readycheck", "/raidgroup", "/record", "/bug", "/home", "/dps", "/hud", "Extended"] {
            assert!(t.iter().any(|l| l.contains(c)), "no tip for {c}");
        }
    }

    /// A screen keeps its tip while it is drawn; the next one picks another;
    /// the control: one tip alone is always it.
    #[test]
    fn a_screen_keeps_its_tip_and_the_next_has_another() {
        let first = pick(None, 0, 10, 7);
        assert_eq!(first, 7);
        assert_eq!(pick(Some((first, 0)), 33, 10, 3), first, "the same screen");
        assert_eq!(pick(Some((first, 1000)), 1000 + GAP, 10, 3), first);
        for roll in 0..50 {
            let next = pick(Some((first, 0)), GAP + 1, 10, roll);
            assert!(next < 10 && next != first, "roll {roll}: {next}");
        }
        assert_eq!(pick(Some((0, 0)), 10_000, 1, 99), 0);
        assert_eq!(pick(Some((12, 0)), 10, 10, 4), 4, "a tip past the list (it was edited): another");
    }

    /// Wrapped at the width by words; the control: a short line is one.
    #[test]
    fn tips_wrap_by_words() {
        let m = |s: &str| s.len() as i32 * 10;
        assert_eq!(wrap("Did you know you can jump?", 400, m), vec!["Did you know you can jump?"]);
        assert_eq!(wrap("aaa bbb ccc", 70, m), vec!["aaa bbb", "ccc"]);
        assert_eq!(wrap("aaaaaaaaaaaa b", 50, m), vec!["aaaaaaaaaaaa", "b"]);
        for l in all() {
            assert!(wrap(l, WIDTH, |s| s.len() as i32 * 7).len() <= 3, "{l}");
        }
    }
}
