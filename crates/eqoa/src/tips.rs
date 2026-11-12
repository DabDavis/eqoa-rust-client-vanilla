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
//!
//! Each tip is tagged (the GM Jibrish's petition 1811, 2026-11-12: "For
//! Heirloom: include all tips. Tag some other tips for Live that are NOT about
//! in-game NPCs and ONLY about client features"): `all` (the client's own
//! features and the game's own commands) shows everywhere, `heirloom` (the
//! server's commands, NPCs and features) only on the Heirloom shard, known by
//! the shard last picked (`settings::home_shard`, kept as the server list's
//! pick is made, so in play it is the shard played on). The title screen
//! (`draw_title`, under Play / Network Setup / Options) shows one too, a new
//! one every `ROTATE` ms, by the same shard as last picked (none yet: `all`).

use crate::ui::Layout;
use std::cell::Cell;
use vi::ui::{Vui, wide};

/// The tips, as built in.
const TEXT: &str = include_str!("tips.txt");

/// A tip's tag: shown everywhere, or only on Heirloom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    All,
    Heirloom,
}

/// A line's tag and its text (untagged: Heirloom's).
fn tagged(l: &'static str) -> (Tag, &'static str) {
    match l.split_once(':') {
        Some(("all", t)) => (Tag::All, t.trim()),
        Some(("heirloom", t)) => (Tag::Heirloom, t.trim()),
        _ => (Tag::Heirloom, l),
    }
}

/// Every tip with its tag, in the file's order: its lines but the notes and the blanks.
pub fn every() -> Vec<(Tag, &'static str)> {
    TEXT.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(tagged).collect()
}

/// Every tip's text.
#[cfg(test)]
pub fn all() -> Vec<&'static str> {
    every().into_iter().map(|(_, t)| t).collect()
}

/// A shard's name is Heirloom's ("Heirloom Test Server").
pub fn is_heirloom(shard: Option<&str>) -> bool {
    shard.is_some_and(|s| s.to_ascii_lowercase().contains("heirloom"))
}

/// The tips for `shard` (its name): all of them on Heirloom, else the `all` ones.
pub fn for_shard(shard: Option<&str>) -> Vec<&'static str> {
    let heirloom = is_heirloom(shard);
    every().into_iter().filter(|&(t, _)| heirloom || t == Tag::All).map(|(_, t)| t).collect()
}

/// The tips for the shard last picked.
fn here() -> Vec<&'static str> {
    for_shard(crate::settings::home_shard().as_deref())
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
    /// The title screen's tip, and when it came up (ms on `clock`).
    static TITLE: Cell<Option<(usize, i64)>> = const { Cell::new(None) };
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

/// How long the title screen keeps a tip (ms).
pub const ROTATE: i64 = 8000;

/// The title screen's tip at `now` of `n`: the one up until `ROTATE` has
/// passed since it came up, then another out of `roll`, never the same again;
/// the time it came up with it.
pub fn rotate(shown: Option<(usize, i64)>, now: i64, n: usize, roll: u64) -> (usize, i64) {
    match shown {
        Some((i, at)) if now - at < ROTATE && now >= at && i < n => (i, at),
        // a new one: `pick` with the last one as long gone
        _ => (pick(shown.map(|(i, _)| (i, i64::MIN / 2)), now, n, roll), now),
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
    let tips = here();
    if tips.is_empty() {
        return;
    }
    let now = clock();
    let i = pick(SHOWN.get(), now, tips.len(), seed());
    SHOWN.set(Some((i, now)));
    lines(vui, r, tips[i], 320, y);
}

/// The title screen's tip (petition 1811), centred on `x` from `y` down, in
/// the space under its menu, with Loading Tips on: a new one every `ROTATE`.
pub fn draw_title(vui: &Vui, r: &mut dyn Layout, x: i32, y: i32) {
    if !crate::settings::loading_tips() {
        return;
    }
    let tips = here();
    if tips.is_empty() {
        return;
    }
    let (i, at) = rotate(TITLE.get(), clock(), tips.len(), seed());
    TITLE.set(Some((i, at)));
    lines(vui, r, tips[i], x, y);
}

/// `tip` wrapped to `WIDTH`, each line centred on `x`, from `y` down.
fn lines(vui: &Vui, r: &mut dyn Layout, tip: &str, x: i32, y: i32) {
    let h = vui.font_height(r);
    for (k, line) in wrap(tip, WIDTH, |s| vui.string_width(r, &wide(s))).iter().enumerate() {
        vui.draw_centered_string_ex(r, &wide(line), x, y + k as i32 * h, COLOR, 0, true);
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

    /// Every tip tagged; Heirloom has them all, any other shard (Live, none
    /// picked yet) only the client's own; the control: a server tip is not
    /// Live's.
    #[test]
    fn tips_by_shard() {
        let raw: Vec<&str> = TEXT.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
        assert!(raw.iter().all(|l| l.starts_with("all: ") || l.starts_with("heirloom: ")), "every tip tagged");
        let (h, live, none) = (for_shard(Some("Heirloom Test Server")), for_shard(Some("Live Server")), for_shard(None));
        assert_eq!(h.len(), all().len());
        assert_eq!(live, none);
        assert!(live.len() >= 20 && live.len() < h.len(), "{} of {}", live.len(), h.len());
        assert!(live.iter().any(|t| t.contains("Dynamic Lighting")) && live.iter().any(|t| t.contains("buffs")));
        assert!(!live.iter().any(|t| t.contains("/home") || t.contains("/merc") || t.contains("/stuck")), "a server's tip on Live");
        assert!(h.iter().any(|t| t.contains("/merc")));
        assert!(all().iter().all(|t| t.starts_with("Did you know")), "the tag cut off");
    }

    /// The title screen keeps a tip `ROTATE` ms, then another, never the same.
    #[test]
    fn the_title_rotates_its_tip() {
        let (i, at) = rotate(None, 100, 10, 3);
        assert_eq!((i, at), (3, 100));
        assert_eq!(rotate(Some((i, at)), 100 + ROTATE - 1, 10, 5), (3, 100), "still up");
        for roll in 0..30 {
            let (j, t) = rotate(Some((i, at)), 100 + ROTATE, 10, roll);
            assert!(j != i && j < 10 && t == 100 + ROTATE, "roll {roll}");
        }
        assert_eq!(rotate(Some((0, 0)), ROTATE, 1, 9), (0, ROTATE), "one tip alone");
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
