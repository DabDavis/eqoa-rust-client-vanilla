//! `VITrailFx` (decomp `vitrailfx.cpp`): the weapon's motion trail. A trail
//! is a ring of segments (two points and their colours, an age) moved
//! between a free list and a used list, the newest at the used list's head;
//! above them a pool of 16 trails moves between three lists, unused,
//! emitting and ending. Rendered each frame as two triangle strips (both
//! windings, so it shows from either side) of at most 24 vertices each, the
//! alpha fading with age.
//!
//! The lists are kept as index sequences in their order (head first).

use crate::fpu::{F, Model};
use crate::particle::Color;
use crate::vect::Vect3;
use std::collections::VecDeque;

/// `VITrailFx::SEGMENT`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Segment<M: Model> {
    pub pos: [Vect3<M>; 2],
    pub color: [Color<M>; 2],
    pub age: F<M>,
}

/// One trail.
#[derive(Clone, Debug, Default)]
pub struct Trail<M: Model> {
    pub segments: Vec<Segment<M>>,
    used: VecDeque<usize>,
    free: VecDeque<usize>,
    last_time: i64,
    life: F<M>,
}

/// A strip as the raster gets it: (colour, vertex) in order.
pub type Strip<M> = Vec<(Color<M>, Vect3<M>)>;

impl<M: Model> Trail<M> {
    /// `Init(now, numSegments, life)`.
    fn init(&mut self, now: i64, n: usize, life: F<M>) {
        self.last_time = now;
        self.life = life;
        self.segments = vec![Segment::default(); n];
        self.used.clear();
        self.free = (0..n).collect();
    }

    /// `ClearSegmentsForward(s)`: meant to free `s` (at `k` in the used
    /// list) and everything older, it frees `s` and at most the one after:
    /// the loop takes the next node from `s` each time, and once `s` is on
    /// the free list its next is that list's tail, which ends the loop. As
    /// shipped.
    fn clear_forward(&mut self, k: usize) {
        let n = (self.used.len() - k).min(2);
        for s in self.used.drain(k..k + n).collect::<Vec<_>>() {
            self.free.push_back(s);
        }
    }

    /// `AddSegment(a, b, ca, cb)`: at the head, the oldest recycled when none
    /// is free.
    pub fn add_segment(&mut self, a: Vect3<M>, b: Vect3<M>, ca: Color<M>, cb: Color<M>) {
        let s = loop {
            if let Some(s) = self.free.pop_front() {
                break s;
            }
            if self.used.is_empty() {
                return;
            }
            self.clear_forward(self.used.len() - 1);
        };
        self.used.push_front(s);
        self.segments[s] = Segment { pos: [a, b], color: [ca, cb], age: F::ZERO };
    }

    /// `Update(now)`: every segment ages; at the first one past its life
    /// whose newer neighbour is past it too (or which is the newest),
    /// `ClearSegmentsForward` from it, and the walk stops. True while any is
    /// left.
    pub fn update(&mut self, now: i64) -> bool {
        let dt = F::<M>::from_int((now - self.last_time) as i32) * F::new(0.001);
        self.last_time = now;
        let mut k = 0;
        while k < self.used.len() {
            let s = self.used[k];
            self.segments[s].age = self.segments[s].age + dt;
            if self.life <= self.segments[s].age {
                if k > 0 {
                    let p = self.used[k - 1];
                    if self.life <= self.segments[p].age {
                        self.clear_forward(k);
                        break;
                    }
                } else {
                    self.clear_forward(k);
                    break;
                }
            }
            k += 1;
        }
        !self.used.is_empty()
    }

    /// `Render`: the alpha set from each segment's age, then the strips, the
    /// second with the two points swapped. Nothing for fewer than two.
    pub fn render(&mut self) -> Vec<Strip<M>> {
        let mut out = Vec::new();
        if self.used.len() < 2 {
            return out;
        }
        for back in [false, true] {
            let mut strip: Strip<M> = Vec::new();
            let mut n = 0;
            let used: Vec<usize> = self.used.iter().copied().collect();
            for (k, &s) in used.iter().enumerate() {
                let seg = &mut self.segments[s];
                let a = F::ONE - seg.age / self.life;
                let a = if a < F::ZERO { F::ZERO } else { a };
                let a = if F::ONE < a { F::ONE } else { a };
                seg.color[0].a = a;
                seg.color[1].a = a;
                let (first, second) = if back { (1, 0) } else { (0, 1) };
                let pair = [(seg.color[first], seg.pos[first]), (seg.color[second], seg.pos[second])];
                strip.extend(pair);
                n += 2;
                if n == 24 {
                    out.push(std::mem::take(&mut strip));
                    n = 0;
                    if k + 1 < used.len() {
                        strip.extend(pair);
                        n = 2;
                    }
                }
            }
            if n != 0 {
                out.push(strip);
            }
        }
        out
    }
}

/// The file-scope pool: 16 trails, unused, emitting and ending.
#[derive(Clone, Debug)]
pub struct Trails<M: Model> {
    pub trails: Vec<Trail<M>>,
    unused: VecDeque<usize>,
    active: VecDeque<usize>,
    ending: VecDeque<usize>,
}

impl<M: Model> Default for Trails<M> {
    /// `VITrailFxInit`.
    fn default() -> Self {
        Trails { trails: vec![Trail::default(); 16], unused: (0..16).collect(), active: VecDeque::new(), ending: VecDeque::new() }
    }
}

impl<M: Model> Trails<M> {
    /// `VITrailFxBegin(now, numSegments, life)`: an unused trail, emitting.
    pub fn begin(&mut self, now: i64, n: usize, life: F<M>) -> Option<usize> {
        let t = self.unused.pop_front()?;
        self.trails[t].init(now, n, life);
        self.active.push_back(t);
        Some(t)
    }

    /// `VITrailFxEnd(t)`: no more segments; it fades out.
    pub fn end(&mut self, t: usize) {
        self.active.retain(|&x| x != t);
        self.unused.retain(|&x| x != t);
        self.ending.retain(|&x| x != t);
        self.ending.push_back(t);
    }

    /// `VITrailFxRender(now, r, c)`: each emitting trail updated and drawn,
    /// each ending one updated and drawn until it is empty, then unused.
    /// Returns (trail, strips) in drawing order.
    pub fn render(&mut self, now: i64) -> Vec<(usize, Vec<Strip<M>>)> {
        let mut out = Vec::new();
        for t in self.active.iter().copied().collect::<Vec<_>>() {
            if self.trails[t].update(now) {
                out.push((t, self.trails[t].render()));
            }
        }
        for t in self.ending.iter().copied().collect::<Vec<_>>() {
            if !self.trails[t].update(now) {
                self.ending.retain(|&x| x != t);
                self.unused.push_back(t);
            } else {
                out.push((t, self.trails[t].render()));
            }
        }
        out
    }
}
