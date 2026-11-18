//! A texture pack's textures kept within its budget (`PackLimits`) by what
//! is drawn, not by what came first (the owner's report of 2026-11-18,
//! "some textures are not being loaded from texture packs since the last
//! texture pack update": the budget of 2026-11-17 counted every stand-in
//! ever made and never let one go, so past it every new one was the game's
//! own for the rest of the run, most of all in a busy place with a large
//! pack).
//!
//! Each stand-in the renderer makes is an entry here with its bytes and a
//! stamp: the last frame it was drawn in (`Renderer::draw` stamps the
//! models' and groups' through the `Hold`s they keep, a per-frame user's
//! asking stamps its own). A model or group keeps a `Hold` of each stand-in
//! it was made with, so a stand-in is only let go once nothing kept holds
//! it and it was not drawn in this frame: then the least recently drawn go
//! first, as many as a new one needs room for (`make_room`); dropped, not
//! destroyed, so anything still bound to one keeps it valid, its memory the
//! GPU's again when the last reference goes. Asked for again, it is made
//! again from what the pack has read (off the frame, as before). Where even
//! what is held and drawn fills the budget, the new stand-in is made a level
//! smaller each way, then smaller again (`choose_step`, `step_down`), down
//! to its surface's own size; only past that is it the game's own texture.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// What a model or group made with a stand-in keeps of it: its stamp, set
/// to the frame each time it is drawn (`Renderer::draw`), and kept alive
/// while the model is, which keeps the stand-in from being let go.
pub type Hold = Arc<AtomicU64>;

#[derive(Debug)]
struct Entry {
    bytes: u64,
    stamp: Hold,
    /// Its bytes at the pack's size: more than `bytes` when it was made
    /// smaller (`step_down`) or as the game's own (counted as 0), and made
    /// again at its size once nothing holds it and there is room (`retry`).
    want: u64,
}

impl Entry {
    fn reduced(&self) -> bool {
        self.bytes < self.want
    }
}

/// The stand-ins a renderer has made, their bytes and when each was last
/// drawn.
#[derive(Debug, Default)]
pub struct Budget {
    entries: HashMap<u64, Entry>,
    used: u64,
    /// The frame now: `next_frame` at each draw.
    frame: std::cell::Cell<u64>,
    /// How many stand-ins were let go to make room, this run.
    pub evicted: u64,
}

/// What the Textures page says of a pack's memory (`Renderer::pack_memory`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PackMemory {
    /// The bytes the stand-ins made take now, and the budget.
    pub used: u64,
    pub budget: u64,
    /// How many stand-ins are made now, how many of them smaller than the
    /// pack's or as the game's own (`step_down`), and how many were let go
    /// to make room this run.
    pub textures: usize,
    pub reduced: usize,
    pub evicted: u64,
}

impl PackMemory {
    /// Whether the pack is being held to its budget: a stand-in let go or
    /// made smaller for want of room.
    pub fn held(&self) -> bool {
        self.evicted > 0 || self.reduced > 0
    }
}

impl Budget {
    pub fn frame(&self) -> u64 {
        self.frame.get()
    }

    /// A new frame drawn: what is stamped from now on is this frame's.
    pub fn next_frame(&self) {
        self.frame.set(self.frame.get() + 1);
    }

    pub fn used(&self) -> u64 {
        self.used
    }

    pub fn contains(&self, key: u64) -> bool {
        self.entries.contains_key(&key)
    }

    /// The stand-in under `key` asked for this frame: its stamp set and its
    /// `Hold` handed out, if it is one.
    pub fn touch(&self, key: u64) -> Option<Hold> {
        let e = self.entries.get(&key)?;
        e.stamp.store(self.frame.get(), Ordering::Relaxed);
        Some(e.stamp.clone())
    }

    /// A stand-in made under `key`, of `bytes` (`want` at the pack's own
    /// size): its `Hold`, stamped now.
    pub fn insert(&mut self, key: u64, bytes: u64, want: u64) -> Hold {
        self.remove(key);
        let stamp = Arc::new(AtomicU64::new(self.frame.get()));
        self.used += bytes;
        self.entries.insert(key, Entry { bytes, stamp: stamp.clone(), want: want.max(bytes) });
        stamp
    }

    /// The stand-in under `key` let go (its bytes no longer counted).
    pub fn remove(&mut self, key: u64) -> bool {
        match self.entries.remove(&key) {
            Some(e) => {
                self.used -= e.bytes;
                true
            }
            None => false,
        }
    }

    /// Whether nothing kept holds the stand-in under `key` and it was not
    /// drawn this frame: it may be let go.
    fn free(&self, e: &Entry) -> bool {
        Arc::strong_count(&e.stamp) == 1 && e.stamp.load(Ordering::Relaxed) < self.frame.get()
    }

    /// Whether the stand-in under `key` was made smaller (or as the game's
    /// own), nothing holds it now, and it would fit at its size in `budget`
    /// were the others nothing holds let go: worth making again.
    pub fn retry(&self, key: u64, budget: u64) -> bool {
        let Some(e) = self.entries.get(&key).filter(|e| e.reduced() && self.free(e)) else { return false };
        let others: u64 = self.entries.iter().filter(|(k, o)| **k != key && self.free(o)).map(|(_, o)| o.bytes).sum();
        self.used - e.bytes + e.want <= budget + others
    }

    /// The stand-in to let go first: of those nothing holds and not drawn
    /// this frame, the least recently drawn (the smaller key of two drawn
    /// last together, so the order is the same every run).
    pub fn least_recent(&self) -> Option<u64> {
        self.entries.iter().filter(|(_, e)| self.free(e) && e.bytes > 0).min_by_key(|(k, e)| (e.stamp.load(Ordering::Relaxed), **k)).map(|(k, _)| *k)
    }

    /// Lets go of the least recently drawn stand-ins (`least_recent`) until
    /// `need` more bytes fit in `budget` or none is left to let go: the keys
    /// let go, for the renderer to drop their textures.
    pub fn make_room(&mut self, need: u64, budget: u64) -> Vec<u64> {
        let mut gone = Vec::new();
        while self.used + need > budget {
            let Some(k) = self.least_recent() else { break };
            self.remove(k);
            self.evicted += 1;
            gone.push(k);
        }
        gone
    }

    /// How many levels smaller each way a stand-in is made to fit in
    /// `budget` (`bytes(k)` its bytes made `k` smaller), no more than
    /// `most` (its surface's own size): the least that fits, `None` for
    /// none (the game's own then).
    pub fn choose_step(&self, budget: u64, most: u32, bytes: impl Fn(u32) -> u64) -> Option<u32> {
        (0..=most).find(|&k| self.used + bytes(k) <= budget)
    }

    /// The stand-ins of another pack than generation `generation` (a
    /// stand-in's key carries its pack's from bit 40: `LibraryTextures::key`)
    /// that nothing holds and were not drawn this frame: a pack made again
    /// (`eqoa`'s `texpack`) leaves its old ones to be let go at once, not
    /// only as room is wanted.
    pub fn stale(&self, generation: u64) -> Vec<u64> {
        self.entries.iter().filter(|(k, e)| (*k >> 40) & 0xFF_FFFF != generation && self.free(e)).map(|(k, _)| *k).collect()
    }

    /// The memory as the Textures page says it.
    pub fn memory(&self, budget: u64) -> PackMemory {
        PackMemory {
            used: self.used,
            budget,
            textures: self.entries.len(),
            reduced: self.entries.values().filter(|e| e.reduced()).count(),
            evicted: self.evicted,
        }
    }

    /// Everything let go (`Renderer::forget_textures`).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.used = 0;
    }
}

/// A texture's size and level count `(w, h, n)` made `k` levels smaller
/// each way (one level at least).
pub fn stepped((w, h, n): (u32, u32, u32), k: u32) -> (u32, u32, u32) {
    ((w >> k).max(1), (h >> k).max(1), n.saturating_sub(k).max(1))
}

/// A texture's bytes at 4 a texel over its levels.
pub fn bytes((w, h, n): (u32, u32, u32)) -> u64 {
    (0..n).map(|k| u64::from((w >> k).max(1)) * u64::from((h >> k).max(1)) * 4).sum()
}

/// `levels` (each half the one before, the base first) made `k` levels
/// smaller each way, as `stepped` says: the first `k` dropped while one is
/// left, the last halved for the rest.
pub fn step_down(mut levels: Vec<esf::Image>, k: u32) -> Vec<esf::Image> {
    if levels.is_empty() || k == 0 {
        return levels;
    }
    let drop = (k as usize).min(levels.len() - 1);
    levels.drain(..drop);
    for _ in drop..k as usize {
        levels = vec![halve(&levels[0])];
    }
    levels
}

/// A level half the size of `l` each way (one texel at least): each texel
/// the mean of the texels it covers, rounded.
pub fn halve(l: &esf::Image) -> esf::Image {
    let (w, h) = ((l.width / 2).max(1), (l.height / 2).max(1));
    let (sx, sy) = (if l.width > 1 { 2 } else { 1 }, if l.height > 1 { 2 } else { 1 });
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            for c in 0..4 {
                let mut sum = 0u32;
                for dy in 0..sy {
                    for dx in 0..sx {
                        sum += u32::from(l.rgba[(((y * sy + dy) * l.width + x * sx + dx) * 4 + c) as usize]);
                    }
                }
                let n = sx * sy;
                rgba.push(((sum + n / 2) / n) as u8);
            }
        }
    }
    let alpha = rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 255);
    esf::Image { width: w, height: h, rgba, alpha }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MB: u64 = 1 << 20;

    /// The least recently drawn stand-in goes first, then the next; one a
    /// model holds, or drawn this frame, never; the bytes counted as each
    /// goes.
    #[test]
    fn the_least_recently_drawn_go_first() {
        let mut b = Budget::default();
        let mut holds = Vec::new();
        for k in 1..=4u64 {
            holds.push(b.insert(k, 10 * MB, 0));
            b.next_frame();
        }
        // drawn since: 1 at frame 5, 2 left at 1, 3 at 2, 4 at 3
        b.next_frame();
        b.touch(1);
        b.next_frame();
        drop(holds);
        assert_eq!(b.least_recent(), Some(2));
        assert_eq!(b.make_room(10 * MB, 40 * MB), vec![2], "room for one: the least recently drawn");
        assert_eq!(b.used(), 30 * MB);
        assert_eq!(b.make_room(30 * MB, 40 * MB), vec![3, 4], "then the next, in order");
        assert_eq!(b.make_room(40 * MB, 40 * MB), vec![1], "the most recent last");
        assert_eq!((b.used(), b.evicted), (0, 4));
    }

    /// A stand-in a model holds stays however long ago it was drawn, and
    /// one drawn this frame stays though nothing holds it (the frame's
    /// commands may use it yet); let go, a model's goes like any other.
    #[test]
    fn held_or_drawn_now_is_never_let_go() {
        let mut b = Budget::default();
        let held = b.insert(1, 10 * MB, 0);
        drop(b.insert(2, 10 * MB, 0));
        b.next_frame();
        b.touch(2);
        assert_eq!(b.make_room(20 * MB, 20 * MB), Vec::<u64>::new(), "1 held, 2 drawn this frame");
        b.next_frame();
        assert_eq!(b.make_room(20 * MB, 20 * MB), vec![2]);
        assert_eq!(b.make_room(20 * MB, 20 * MB), Vec::<u64>::new(), "1 still held");
        drop(held);
        assert_eq!(b.make_room(20 * MB, 20 * MB), vec![1]);
    }

    /// A model drawing a stand-in keeps it to the front: the stamp it holds
    /// is the stand-in's own.
    #[test]
    fn a_draw_through_a_hold_stamps_the_stand_in() {
        let mut b = Budget::default();
        let h1 = b.insert(1, MB, 0);
        let h2 = b.insert(2, MB, 0);
        for _ in 0..3 {
            b.next_frame();
        }
        h1.store(b.frame(), Ordering::Relaxed);
        b.next_frame();
        drop((h1, h2));
        assert_eq!(b.make_room(MB, 2 * MB), vec![2], "2 not drawn since it was made");
    }

    /// Nothing refused while there is anything to let go: a thousand
    /// stand-ins of 5.3 MB (a 1024 pack's, levels and all) through a
    /// budget of 256 MB, each made at its size once the older ones go.
    #[test]
    fn none_made_smaller_while_any_can_go() {
        let mut b = Budget::default();
        let size = (1024, 1024, 11);
        let budget = 256 * MB;
        for key in 0..1000u64 {
            b.make_room(bytes(size), budget);
            let k = b.choose_step(budget, 4, |k| bytes(stepped(size, k)));
            assert_eq!(k, Some(0), "stand-in {key} at its size");
            drop(b.insert(key, bytes(size), 0));
            b.next_frame();
        }
        assert!(b.used() <= budget);
        assert_eq!(b.memory(budget).textures, (budget / bytes(size)) as usize);
        assert_eq!(b.evicted, 1000 - (budget / bytes(size)));
    }

    /// Where what is held fills the budget, a new stand-in is made a level
    /// smaller each way until it fits, a quarter of the bytes a step, never
    /// below its surface's size (`most`); past that, `None`: the game's own.
    #[test]
    fn held_full_steps_down_then_the_games_own() {
        let mut b = Budget::default();
        let size = (1024, 1024, 11);
        let _held = b.insert(1, 250 * MB, 0);
        let budget = 256 * MB;
        assert!(b.make_room(bytes(size), budget).is_empty());
        // 6 MB left: 5.3 MB at its size fits; 4.0 at 1 left does not
        assert_eq!(b.choose_step(budget, 4, |k| bytes(stepped(size, k))), Some(0));
        let _also = b.insert(2, 5 * MB, 0);
        // 1 MB left: 1024 (5.3) no, 512 (1.3) no, 256 (0.33) yes
        assert_eq!(b.choose_step(budget, 4, |k| bytes(stepped(size, k))), Some(2));
        assert_eq!(b.choose_step(budget, 1, |k| bytes(stepped(size, k))), None, "not below its surface's size");
        assert_eq!((bytes(size), bytes(stepped(size, 1)), bytes(stepped(size, 2))), (4 * 1_398_101, 4 * 349_525, 4 * 87_381), "a quarter a step");
    }

    /// `step_down`'s levels are what `stepped` says, a level's texels the
    /// means of the four they cover; past the levels it has, the last
    /// halved.
    #[test]
    fn step_down_halves_to_stepped() {
        let level = |w: u32, h: u32, v: u8| esf::Image { width: w, height: h, rgba: vec![v; (w * h * 4) as usize], alpha: false };
        let chain: Vec<esf::Image> = (0..3).map(|k| level(64 >> k, 32 >> k, k as u8)).collect();
        for k in 0..6 {
            let got = step_down(chain.clone(), k);
            let (w, h, n) = stepped((64, 32, 3), k);
            assert!(crate::standin::fits(&got, (w, h, n)), "k {k}: {}x{}x{}", got[0].width, got[0].height, got.len());
        }
        assert_eq!(step_down(chain.clone(), 1)[0].rgba[0], 1, "level 1 kept as it is");
        let img = esf::Image { width: 2, height: 2, rgba: vec![0, 0, 0, 255, 10, 10, 10, 255, 20, 20, 20, 255, 31, 31, 31, 255], alpha: false };
        assert_eq!(halve(&img).rgba, vec![15, 15, 15, 255], "(0 + 10 + 20 + 31) / 4, rounded");
    }

    /// A pack made again: the old pack's stand-ins nothing holds are stale,
    /// a held one and the new pack's are not (the control).
    #[test]
    fn an_old_packs_stand_ins_are_stale() {
        let mut b = Budget::default();
        let old = |k: u64| k | 1 << 40;
        let new = |k: u64| k | 2 << 40;
        drop(b.insert(old(1), MB, 0));
        let held = b.insert(old(2), MB, 0);
        drop(b.insert(new(3), MB, 0));
        b.next_frame();
        assert_eq!(b.stale(2), vec![old(1)]);
        drop(held);
        let mut s = b.stale(2);
        s.sort();
        assert_eq!(s, vec![old(1), old(2)]);
    }

    /// A stand-in made smaller is made again once nothing holds it; one at
    /// its size is not.
    #[test]
    fn a_reduced_stand_in_is_tried_again_once_let_go() {
        let mut b = Budget::default();
        // 1 made at 1 MB of its 4, 2 at its size
        let small = b.insert(1, MB, 4 * MB);
        let full = b.insert(2, MB, 0);
        b.next_frame();
        assert!(!b.retry(1, 8 * MB), "held");
        drop((small, full));
        assert!(b.retry(1, 8 * MB) && !b.retry(2, 8 * MB));
        assert!(b.retry(1, 4 * MB), "room once 2 is let go");
        assert!(!b.retry(1, 3 * MB), "no room for its 4 MB however much goes: not made again every frame");
        b.touch(2);
        assert!(!b.retry(1, 4 * MB), "2 drawn this frame: no room");
        let m = b.memory(4 * MB);
        assert_eq!((m.used, m.textures, m.reduced, m.held()), (2 * MB, 2, 1, true));
    }
}
