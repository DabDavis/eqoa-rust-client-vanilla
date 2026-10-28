//! `VIHSprite`, the skeleton and its pose evaluator, and `VIHSpriteAnim`, a
//! clip (decomp `vihsprite.cpp`, `vihspriteanim.cpp`). A line-by-line port of
//! the evaluator: `Process` → `ProcessPlayList` (with `ProcessPlayTiming`) →
//! `ProcessHierarchy`, the plays and their blends, the bind pose and its
//! inverse, the centre transform, the skin palette (`SetBlendMatrices`).
//!
//! Unlike the walk code, these are NATIVE types: the guest's pools and arrays
//! hold pointers, so bytes cannot cross to the oracle; it is checked instead by
//! building the same skeleton, clips and plays through the guest's API and
//! comparing every node matrix (`tests/oracle_anim.rs`).

use crate::fpu::{F, Model, clamp, f};
use crate::geom::Matrix44;
use crate::quat::Quat;
use crate::vect::Vect3;
use std::collections::HashMap;

/// `VIHSpriteFrame`: a local transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame<M: Model> {
    pub rot: Quat<M>,
    pub scale: F<M>,
    pub trans: Vect3<M>,
}

impl<M: Model> Default for Frame<M> {
    fn default() -> Self {
        Frame { rot: Quat::default(), scale: F::ONE, trans: Vect3::default() }
    }
}

/// `VIHSpriteAnimFormat` and the frames in it.
#[derive(Clone, Debug)]
pub enum Frames<M: Model> {
    Full(Vec<Frame<M>>),
    /// rot ×4, scale, trans ×3, as int16s (1/32767 and 1/512).
    Pack(Vec<[i16; 8]>),
}

/// `VIPlaybackType`.
pub const LOOP: i32 = 0;
pub const ONCE: i32 = 1;
pub const STOPPED: i32 = 2;
pub const STOP_END: i32 = 3;

/// `VIHSpriteAnim`.
#[derive(Clone, Debug)]
pub struct Anim<M: Model> {
    pub num_nodes: i32,
    pub num_frames: i32,
    pub play_rate: F<M>,
    pub play_speed: F<M>,
    pub playback_type: i32,
    /// (ref id, first frame) per node: node j's frames start at j × numFrames.
    pub nodes: Vec<(i32, i32)>,
    pub frames: Frames<M>,
    /// (trigger ref id, frame).
    pub triggers: Vec<(i32, F<M>)>,
}

impl<M: Model> Anim<M> {
    fn frame(&self, i: usize) -> Frame<M> {
        match &self.frames {
            Frames::Full(v) => {
                if v.is_empty() {
                    Frame::default()
                } else {
                    v[i.min(v.len() - 1)]
                }
            }
            Frames::Pack(v) => {
                if v.is_empty() {
                    Frame::default()
                } else {
                    unpack(&v[i.min(v.len() - 1)])
                }
            }
        }
    }
}

/// The packed walker's dequantisation: `(float)s * (1.0f/32767.0f)` and
/// `(float)s * (1.0f/512.0f)`, the constants as the guest folds them
/// (0x38000100, 0x3B000000; go-eqoa-vi's `PackFrame`).
pub fn unpack<M: Model>(p: &[i16; 8]) -> Frame<M> {
    let r = F::<M>::new(f32::from_bits(0x3800_0100));
    let s = F::<M>::new(f32::from_bits(0x3B00_0000));
    let c = |v: i16| F::<M>::new(v as f32);
    Frame {
        rot: Quat { x: c(p[0]) * r, y: c(p[1]) * r, z: c(p[2]) * r, w: c(p[3]) * r },
        scale: c(p[4]) * s,
        trans: Vect3::new(c(p[5]) * s, c(p[6]) * s, c(p[7]) * s),
    }
}

/// What a sprite reads of `VIScene`: the clock, its clips and ref maps.
#[derive(Default)]
pub struct AnimScene<M: Model> {
    pub time: i64,
    pub anims: Vec<Anim<M>>,
    pub ref_maps: Vec<HashMap<i32, i32>>,
}

/// `VIPool<T>`: slots on a free list, iterated in INSERTION order.
#[derive(Clone, Debug)]
pub struct Pool<T> {
    slots: Vec<(Option<T>, i32, i32)>,
    free: Vec<i32>,
    first: i32,
    last: i32,
}

impl<T> Default for Pool<T> {
    fn default() -> Self {
        Pool { slots: Vec::new(), free: Vec::new(), first: -1, last: -1 }
    }
}

impl<T> Pool<T> {
    pub fn add(&mut self, v: T) -> i32 {
        let i = match self.free.pop() {
            Some(i) => i,
            None => {
                self.slots.push((None, -1, -1));
                self.slots.len() as i32 - 1
            }
        };
        self.slots[i as usize] = (Some(v), self.last, -1);
        if self.last != -1 {
            self.slots[self.last as usize].2 = i;
        } else {
            self.first = i;
        }
        self.last = i;
        i
    }
    pub fn erase(&mut self, i: i32) {
        let (_, prev, next) = self.slots[i as usize];
        if next != -1 {
            self.slots[next as usize].1 = prev;
        } else {
            self.last = prev;
        }
        if prev != -1 {
            self.slots[prev as usize].2 = next;
        } else {
            self.first = next;
        }
        self.slots[i as usize].0 = None;
        self.free.push(i);
    }
    pub fn begin(&self) -> i32 {
        self.first
    }
    /// The next in order, or -1.
    pub fn next(&self, i: i32) -> i32 {
        self.slots[i as usize].2
    }
    pub fn get(&self, i: i32) -> &T {
        self.slots[i as usize].0.as_ref().expect("live pool slot")
    }
    pub fn get_mut(&mut self, i: i32) -> &mut T {
        self.slots[i as usize].0.as_mut().expect("live pool slot")
    }
    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.0.is_some()).count()
    }
    pub fn is_empty(&self) -> bool {
        self.first == -1
    }
    pub fn iter(&self) -> impl Iterator<Item = i32> + '_ {
        std::iter::successors(Some(self.first).filter(|&i| i != -1), |&i| Some(self.next(i)).filter(|&n| n != -1))
    }
}

/// `VIHSpriteNode`.
#[derive(Clone, Debug)]
pub struct Node<M: Model> {
    pub parent: i32,
    pub child: i32,
    pub sibling: i32,
    pub lod: i32,
    /// The animated local transform.
    pub frame: Frame<M>,
    /// The animated output (model space).
    pub world: Matrix44<M>,
    pub fix_scale: Vect3<M>,
    pub bind: Frame<M>,
    pub inv_bind_world: Matrix44<M>,
    pub rest: Frame<M>,
}

impl<M: Model> Default for Node<M> {
    fn default() -> Self {
        Node {
            parent: -1,
            child: -1,
            sibling: -1,
            lod: 0,
            frame: Frame::default(),
            world: Matrix44::default(),
            fix_scale: Vect3::default(),
            bind: Frame::default(),
            inv_bind_world: Matrix44::default(),
            rest: Frame::default(),
        }
    }
}

/// `VIHSpritePlay`.
#[derive(Clone, Debug, Default)]
pub struct Play<M: Model> {
    pub anim: usize,
    pub playback_type: i32,
    pub interpolate: bool,
    pub blend_mode: i32,
    pub blend_start: i64,
    pub blend_duration: i64,
    pub weight: F<M>,
    pub rate: F<M>,
    pub speed: F<M>,
    pub frame_a: i32,
    pub frame_b: i32,
    pub time_accum: F<M>,
    pub frame_t: F<M>,
    pub last_time: i64,
    pub frame_rate: F<M>,
    pub frame_duration: F<M>,
    pub num_nodes: i32,
    pub first_node: i32,
}

/// `VIHSpritePlayNode`: (state, node, first frame, frozen frame).
#[derive(Clone, Debug)]
pub struct PlayNode<M: Model> {
    pub a: i32,
    pub node: i32,
    pub first_frame: i32,
    pub frame: Frame<M>,
}

/// `VIHSpriteTrigger`: (ref id, tripped, when).
#[derive(Clone, Copy, Debug, Default)]
pub struct Trigger {
    pub id: i32,
    pub tripped: i32,
    pub time: i64,
}

#[derive(Clone, Debug)]
pub struct HSprite<M: Model> {
    pub nodes: Vec<Node<M>>,
    pub blend: Vec<Matrix44<M>>,
    pub triggers: Vec<Trigger>,
    /// (sprite, node; -1 for a skin).
    pub attach: Pool<(i32, i32)>,
    pub plays: Pool<Play<M>>,
    pub play_nodes: Pool<PlayNode<M>>,
    pub lod_dist: Vec<F<M>>,
    pub play_time: i64,
    pub center_time: i64,
    pub pose_time: i64,
    pub default_scale: F<M>,
    pub center: [Vect3<M>; 2],
    pub transform: [Frame<M>; 2],
    pub blend_ms: i32,
    pub blend_start: i64,
    pub center_matrix: Matrix44<M>,
    pub ref_map: usize,
}

impl<M: Model> HSprite<M> {
    /// The constructor's and `Clear`'s state.
    pub fn new(ref_map: usize) -> Self {
        HSprite {
            nodes: Vec::new(),
            blend: Vec::new(),
            triggers: Vec::new(),
            attach: Pool::default(),
            plays: Pool::default(),
            play_nodes: Pool::default(),
            lod_dist: Vec::new(),
            play_time: -1,
            center_time: -1,
            pose_time: -1,
            default_scale: F::ONE,
            center: [Vect3::default(); 2],
            transform: [Frame::default(); 2],
            blend_ms: 0,
            blend_start: 0,
            center_matrix: Matrix44::default(),
            ref_map,
        }
    }

    // ----------------------------------------------------------- building

    pub fn add_root(&mut self) -> i32 {
        if self.nodes.is_empty() {
            self.nodes.push(Node::default());
            self.blend.push(Matrix44::default());
            return 0;
        }
        -1
    }

    pub fn add(&mut self, parent: i32) -> i32 {
        if self.nodes.is_empty() {
            return -1;
        }
        let i = self.nodes.len() as i32;
        self.nodes.push(Node { parent, ..Node::default() });
        self.blend.push(Matrix44::default());
        let p = parent as usize;
        if self.nodes[p].child == -1 {
            self.nodes[p].child = i;
        } else {
            let mut j = self.nodes[p].child;
            while self.nodes[j as usize].sibling != -1 {
                j = self.nodes[j as usize].sibling;
            }
            self.nodes[j as usize].sibling = i;
        }
        i
    }

    /// `SetNodeBindPose`: the bind pose is MODEL space; its inverse is kept,
    /// and the local rest frame is derived through the parent's inverse.
    pub fn set_node_bind_pose(&mut self, i: usize, q: Quat<M>, s: F<M>, t: Vect3<M>) {
        self.nodes[i].bind = Frame { rot: q, scale: s, trans: t };
        let m = Matrix44::transform_quat(&q, s, &t);
        self.nodes[i].inv_bind_world = Matrix44::invert(&m).unwrap_or(Matrix44::identity());
        let parent = self.nodes[i].parent;
        if parent != -1 {
            let r = Matrix44::mul(&m, &self.nodes[parent as usize].inv_bind_world);
            let (rq, rs, rt) = r.convert_to();
            self.nodes[i].rest = Frame { rot: rq, scale: rs, trans: rt };
        } else {
            self.nodes[i].rest = Frame { rot: q, scale: s, trans: t };
        }
    }

    pub fn attach_skin(&mut self, sprite: i32) -> i32 {
        self.attach.add((sprite, -1))
    }

    pub fn find_trigger(&self, id: i32) -> Option<usize> {
        self.triggers.iter().position(|t| t.id == id)
    }

    pub fn reset_trigger(&mut self, i: usize) {
        self.triggers[i].tripped = 0;
    }

    // -------------------------------------------------------------- plays

    pub fn add_play(&mut self, anim: usize, playback_type: i32, speed: F<M>, sc: &AnimScene<M>) -> i32 {
        let a = &sc.anims[anim];
        let i = self.plays.add(Play {
            anim,
            playback_type,
            interpolate: true,
            blend_mode: 0,
            blend_start: sc.time,
            blend_duration: 0,
            weight: F::ZERO,
            rate: a.play_rate,
            speed,
            frame_a: 0,
            frame_b: 0,
            time_accum: F::ZERO,
            frame_t: F::ZERO,
            last_time: sc.time,
            frame_rate: F::ZERO,
            frame_duration: F::ZERO,
            num_nodes: 0,
            first_node: -1,
        });
        let map = &sc.ref_maps[self.ref_map];
        for &(ref_id, first_frame) in &a.nodes {
            let Some(&node) = map.get(&ref_id) else { continue };
            if node == -1 {
                continue;
            }
            let n = self.play_nodes.add(PlayNode { a: 0, node, first_frame, frame: Frame::default() });
            let p = self.plays.get_mut(i);
            if p.num_nodes == 0 {
                p.first_node = n;
            }
            p.num_nodes += 1;
        }
        self.precalc_playback(i);
        i
    }

    pub fn add_play_default(&mut self, anim: usize, sc: &AnimScene<M>) -> i32 {
        let (t, s) = (sc.anims[anim].playback_type, sc.anims[anim].play_speed);
        self.add_play(anim, t, s, sc)
    }

    pub fn add_interpolated_play(&mut self, anim: usize, t: i32, speed: F<M>, ms: i64, sc: &AnimScene<M>) -> i32 {
        let i = self.add_play(anim, t, speed, sc);
        let p = self.plays.get_mut(i);
        p.blend_mode = 1;
        p.blend_duration = ms;
        i
    }

    pub fn add_interpolated_play_with_sync(&mut self, anim: usize, sync: i32, ms: i64, sc: &AnimScene<M>) -> i32 {
        let (t, s) = (self.plays.get(sync).playback_type, self.plays.get(sync).speed);
        let i = self.add_play(anim, t, s, sc);
        let p = self.plays.get_mut(i);
        p.blend_mode = 1;
        p.blend_duration = ms;
        self.sync_timing(i, sync, sc);
        i
    }

    pub fn sync_timing(&mut self, a: i32, b: i32, sc: &AnimScene<M>) -> i32 {
        let pb = self.plays.get(b).clone();
        if sc.anims[self.plays.get(a).anim].num_frames != sc.anims[pb.anim].num_frames {
            return -1;
        }
        let pa = self.plays.get_mut(a);
        pa.frame_a = pb.frame_a;
        pa.frame_b = pb.frame_b;
        pa.time_accum = pb.time_accum;
        pa.frame_t = pb.frame_t;
        pa.last_time = pb.last_time;
        pa.frame_rate = pb.frame_rate;
        0
    }

    fn precalc_playback(&mut self, i: i32) {
        let p = self.plays.get_mut(i);
        let s = p.speed;
        if s != F::ZERO {
            if F::ZERO < s {
                p.frame_rate = p.rate * s / f(1000.0);
            } else {
                p.frame_rate = p.rate * -s / f(1000.0);
            }
            p.frame_duration = F::ONE / p.frame_rate;
        } else {
            p.frame_rate = F::ZERO;
            p.frame_duration = F::ZERO;
        }
    }

    pub fn erase_play(&mut self, i: i32) {
        let (mut n, mut j) = (self.plays.get(i).num_nodes, self.plays.get(i).first_node);
        while n > 0 {
            let k = self.play_nodes.next(j);
            self.play_nodes.erase(j);
            n -= 1;
            j = k;
        }
        self.plays.erase(i);
    }

    /// `SetPlayFrame(i, frame)`: the play put at `frame` (its fraction as
    /// time into the frame), held at the last frame for a play-once, wrapped
    /// otherwise.
    pub fn set_play_frame(&mut self, i: i32, frame: F<M>, sc: &AnimScene<M>) {
        let nf = sc.anims[self.plays.get(i).anim].num_frames;
        if nf <= 0 {
            return;
        }
        let p = self.plays.get_mut(i);
        let fi = frame.0 as i32;
        p.frame_a = fi;
        p.time_accum = (frame - F::from_int(fi)) * p.frame_duration;
        if p.playback_type == ONCE {
            if p.frame_a < 0 {
                p.frame_a = 0;
            }
            if nf - 1 < p.frame_a {
                p.frame_a = nf - 1;
            }
            p.frame_b = p.frame_a + 1;
            if nf - 1 < p.frame_b {
                p.frame_b = p.frame_a;
                p.time_accum = F::ZERO;
            }
        } else {
            p.frame_a = ((fi % nf) + nf) % nf;
            p.frame_b = p.frame_a + 1;
            if nf - 1 < p.frame_b {
                p.frame_b = 0;
            }
        }
        p.frame_t = p.time_accum * p.frame_rate;
        p.last_time = sc.time;
    }

    pub fn stop_playback(&mut self, i: i32, sc: &AnimScene<M>) {
        self.process_play_timing(i, sc);
        let p = self.plays.get_mut(i);
        p.playback_type = STOP_END;
        p.blend_mode = 0;
    }

    pub fn freeze_playback(&mut self, i: i32, sc: &AnimScene<M>) {
        let (mut n, mut j) = (self.plays.get(i).num_nodes, self.plays.get(i).first_node);
        while n > 0 {
            self.play_nodes.get_mut(j).a = 1;
            n -= 1;
            j = self.play_nodes.next(j);
        }
        self.process_play_list(-1, sc);
        let p = self.plays.get_mut(i);
        p.playback_type = STOP_END;
        p.blend_mode = 0;
    }

    pub fn is_blend_complete(&self, i: i32, sc: &AnimScene<M>) -> bool {
        let p = self.plays.get(i);
        p.blend_duration < sc.time - p.blend_start
    }

    /// `IsPlaybackStopped(i)`. For -1 (a play `SetDeath` just erased,
    /// which `SetAnimation` still asks about) the game reads the slot before
    /// the pool's first; here it answers stopped.
    pub fn is_playback_stopped(&self, i: i32) -> bool {
        if i < 0 {
            return true;
        }
        self.plays.get(i).playback_type == STOP_END
    }

    // ---------------------------------------------------------- evaluator

    /// `Process(lod, scene)`: the pose for the scene's time, once per time.
    pub fn process(&mut self, lod: i32, sc: &AnimScene<M>) {
        if sc.time != self.pose_time {
            if !self.plays.is_empty() {
                self.process_play_list(lod, sc);
                self.process_hierarchy();
            } else {
                self.set_bind_pose();
            }
        }
        self.pose_time = sc.time;
    }

    pub fn set_bind_pose(&mut self) {
        for n in &mut self.nodes {
            n.world = Matrix44::transform_quat(&n.bind.rot, n.bind.scale, &n.bind.trans);
        }
    }

    pub fn process_hierarchy(&mut self) {
        if !self.nodes.is_empty() {
            self.process_hierarchy_from(0, &Matrix44::identity());
        }
    }

    fn process_hierarchy_from(&mut self, i: usize, m: &Matrix44<M>) {
        let n = &self.nodes[i];
        let mut v = n.fix_scale;
        v *= n.frame.scale;
        let t = Matrix44::transform_quat3(&n.frame.rot, &v, &n.frame.trans);
        let world = Matrix44::mul(&t, m);
        self.nodes[i].world = world;
        let mut c = self.nodes[i].child;
        while c != -1 {
            self.process_hierarchy_from(c as usize, &world);
            c = self.nodes[c as usize].sibling;
        }
    }

    /// `ProcessPlayTiming`: advance one play's frames to the scene's time, trip
    /// triggers it passed, and ramp its blend weight.
    pub fn process_play_timing(&mut self, i: i32, sc: &AnimScene<M>) {
        let p0 = self.plays.get(i);
        let a = &sc.anims[p0.anim];
        let nf = a.num_frames;
        let mut tripped: Vec<(usize, i64)> = Vec::new();
        let p = self.plays.get_mut(i);
        if p.playback_type != STOPPED && p.playback_type != STOP_END {
            let since = sc.time - p.last_time;
            let mut dt = F::new(since as i32 as f32);
            let from = if F::ZERO < p.speed {
                F::new(p.frame_a as f32) + p.frame_t
            } else {
                F::new(p.frame_a as f32) - p.frame_t
            };
            if p.frame_rate != F::ZERO {
                let acc = p.time_accum + dt;
                let n = (acc * p.frame_rate).0 as i32;
                p.time_accum = acc - F::new(n as f32) * p.frame_duration;
                if F::ZERO < p.speed {
                    p.frame_a += n;
                    if p.playback_type == ONCE {
                        if p.frame_a < 0 {
                            p.frame_a = 0;
                        }
                        if nf - 1 < p.frame_a {
                            p.frame_a = nf - 1;
                        }
                        p.frame_b = p.frame_a + 1;
                        if nf - 1 < p.frame_b {
                            p.frame_b = p.frame_a;
                            p.time_accum = F::ZERO;
                            p.playback_type = STOP_END;
                        }
                    } else if nf > 0 {
                        p.frame_a = ((p.frame_a % nf) + nf) % nf;
                        p.frame_b = p.frame_a + 1;
                        if nf - 1 < p.frame_b {
                            p.frame_b = 0;
                        }
                    }
                } else {
                    p.frame_a -= n;
                    if p.playback_type == ONCE {
                        if p.frame_a < 0 {
                            p.frame_a = 0;
                        }
                        if nf - 1 < p.frame_a {
                            p.frame_a = nf - 1;
                        }
                        p.frame_b = p.frame_a - 1;
                        if p.frame_b < 0 {
                            p.frame_b = p.frame_a;
                            p.time_accum = F::ZERO;
                            p.playback_type = STOP_END;
                        }
                    } else if nf > 0 {
                        p.frame_a = ((p.frame_a % nf) + nf) % nf;
                        p.frame_b = p.frame_a - 1;
                        if p.frame_b < 0 {
                            p.frame_b = nf - 1;
                        }
                    }
                }
                p.frame_t = p.time_accum * p.frame_rate;
                if !a.triggers.is_empty() {
                    let fnf = F::<M>::new(nf as f32);
                    let to = if F::ZERO < p.speed {
                        let to = F::new(p.frame_a as f32) + p.frame_t;
                        if fnf < to { to.fmodf(fnf) + fnf } else { to }
                    } else {
                        let to = F::new(p.frame_a as f32) - p.frame_t;
                        if to < -fnf { to.fmodf(fnf) - fnf } else { to }
                    };
                    for &(id, frame) in &a.triggers {
                        let Some(idx) = self.triggers.iter().position(|t| t.id == id) else { continue };
                        let mut tf = frame;
                        let hit = |tf: F<M>| {
                            if F::ZERO < p.speed { from < tf && tf <= to } else { tf < from && to <= tf }
                        };
                        let when = |tf: F<M>| {
                            let d = if F::ZERO < p.speed { to - tf } else { tf - to };
                            sc.time - (d * p.frame_duration).0 as i64
                        };
                        if hit(tf) {
                            tripped.push((idx, when(tf)));
                        } else {
                            tf = if F::ZERO < p.speed { tf + fnf } else { tf - fnf };
                            if hit(tf) {
                                tripped.push((idx, when(tf)));
                            }
                        }
                    }
                }
            }
            match p.blend_mode {
                0 => p.weight = F::ZERO,
                1 => {
                    let since = sc.time - p.blend_start;
                    dt = F::new(since as i32 as f32);
                    if p.blend_duration < since {
                        p.blend_mode = 0;
                    } else {
                        p.weight = dt / F::new(p.blend_duration as i32 as f32);
                    }
                }
                _ => {}
            }
        }
        p.last_time = sc.time;
        for (idx, when) in tripped {
            self.triggers[idx].time = when;
            self.triggers[idx].tripped = 1;
        }
    }

    /// `ProcessPlayList`: every play, in pool order, writes the frames of the
    /// nodes it drives: overwritten (blend mode 0) or slerped toward by its
    /// weight (blend mode 1). Nodes past the LOD take their rest frame.
    pub fn process_play_list(&mut self, lod: i32, sc: &AnimScene<M>) {
        if sc.time == self.play_time {
            return;
        }
        self.play_time = sc.time;
        let mut p = self.plays.begin();
        while p != -1 {
            self.process_play_timing(p, sc);
            let play = self.plays.get(p).clone();
            let an = &sc.anims[play.anim];
            let (weight, fa, fb, t) = (play.weight, play.frame_a, play.frame_b, play.frame_t);
            let mut count = play.num_nodes;
            let mut pn = play.first_node;
            while count > 0 {
                let e = self.play_nodes.get(pn).clone();
                let ni = e.node as usize;
                if lod < self.nodes[ni].lod {
                    let (a, b);
                    if e.a != 2 {
                        let total_frames = match &an.frames {
                            Frames::Full(v) => v.len(),
                            Frames::Pack(v) => v.len(),
                        };
                        let nf = an.num_frames.max(1);
                        let fa_offset = ((fa % nf) + nf) % nf;
                        let fb_offset = ((fb % nf) + nf) % nf;
                        let idx_a = ((e.first_frame + fa_offset).max(0) as usize).min(total_frames.saturating_sub(1));
                        let idx_b = ((e.first_frame + fb_offset).max(0) as usize).min(total_frames.saturating_sub(1));
                        a = an.frame(idx_a);
                        b = an.frame(idx_b);
                        if e.a == 1 {
                            let pe = self.play_nodes.get_mut(pn);
                            pe.a = 2;
                            pe.frame = a;
                        }
                    } else {
                        a = e.frame;
                        b = e.frame;
                    }
                    let mut c = if !play.interpolate {
                        a
                    } else {
                        Frame {
                            rot: Quat::slerp(&a.rot, &b.rot, t),
                            scale: a.scale + (b.scale - a.scale) * t,
                            trans: Vect3::new(
                                (b.trans.x - a.trans.x) * t + a.trans.x,
                                (b.trans.y - a.trans.y) * t + a.trans.y,
                                (b.trans.z - a.trans.z) * t + a.trans.z,
                            ),
                        }
                    };
                    match play.blend_mode {
                        0 => self.nodes[ni].frame = c,
                        1 => {
                            let nf = self.nodes[ni].frame;
                            c.rot = Quat::slerp(&nf.rot, &c.rot, weight);
                            let ns = nf.scale;
                            c.trans.x -= nf.trans.x;
                            c.scale = ns + (c.scale - ns) * weight;
                            c.trans.y -= nf.trans.y;
                            c.trans.z -= nf.trans.z;
                            c.trans *= weight;
                            c.trans += nf.trans;
                            self.nodes[ni].frame = c;
                        }
                        _ => {}
                    }
                } else {
                    self.nodes[ni].frame = self.nodes[ni].rest;
                }
                pn = self.play_nodes.next(pn);
                count -= 1;
            }
            p = self.plays.next(p);
        }
    }

    /// `SetBlendMatrices`: the skin palette, `invBindWorld × world` per node,
    /// when a skin is attached.
    pub fn set_blend_matrices(&mut self) -> bool {
        if !self.attach.iter().any(|i| self.attach.get(i).1 == -1) {
            return false;
        }
        for (b, n) in self.blend.iter_mut().zip(&self.nodes) {
            *b = Matrix44::mul(&n.inv_bind_world, &n.world);
        }
        true
    }

    pub fn calc_lod_level_index(&self, m: &Matrix44<M>) -> i32 {
        let d = Vect3::new(m.m[3].x, m.m[3].y, m.m[3].z);
        let dist = d.x * d.x + d.y * d.y + d.z * d.z;
        let mut lvl = -1;
        for &ld in &self.lod_dist {
            if dist < ld * ld {
                break;
            }
            lvl += 1;
        }
        lvl
    }

    // ---------------------------------------------------- centre transform

    pub fn calc_trans_about_center(&mut self, m: &Matrix44<M>, sc: &AnimScene<M>) -> Matrix44<M> {
        if sc.time != self.center_time {
            self.update_center_transform(sc);
        }
        Matrix44::mul(&self.center_matrix, m)
    }

    fn update_center_transform(&mut self, sc: &AnimScene<M>) {
        let (center, fr) = self.interp_trans_about_center(sc);
        self.center_matrix = Matrix44::scale(self.default_scale);
        let a = Matrix44::translate(&Vect3::new(-center.x, -center.y, -center.z));
        let b = Matrix44::mul(&self.center_matrix, &a);
        self.center_matrix = Matrix44::transform_quat(&fr.rot, fr.scale, &fr.trans);
        let a = Matrix44::mul(&b, &self.center_matrix);
        let b = Matrix44::translate(&center);
        self.center_matrix = Matrix44::mul(&a, &b);
        self.center_time = sc.time;
    }

    fn interp_trans_about_center(&mut self, sc: &AnimScene<M>) -> (Vect3<M>, Frame<M>) {
        if self.blend_ms == 0 || sc.time - self.blend_start > self.blend_ms as i64 {
            self.blend_ms = 0;
            return (self.center[1], self.transform[1]);
        }
        let t = F::<M>::new((sc.time - self.blend_start) as i32 as f32) / F::new(self.blend_ms as f32);
        let t = clamp(t, F::ZERO, F::ONE);
        let mut center = self.center[1];
        center -= self.center[0];
        center *= t;
        center += self.center[0];
        let (t0, t1) = (self.transform[0], self.transform[1]);
        let mut trans = t1.trans;
        trans -= t0.trans;
        trans *= t;
        trans += t0.trans;
        let fr = Frame { rot: Quat::slerp(&t0.rot, &t1.rot, t), scale: t0.scale + (t1.scale - t0.scale) * t, trans };
        (center, fr)
    }

    pub fn blend_to_center_transform(&mut self, center: Vect3<M>, hpr: &Vect3<M>, s: F<M>, t: Vect3<M>, ms: i32, sc: &AnimScene<M>) {
        if self.blend_ms == 0 || sc.time - self.blend_start > self.blend_ms as i64 {
            self.center[0] = self.center[1];
            self.transform[0] = self.transform[1];
            self.center[1] = center;
            self.transform[1] = Frame { rot: Quat::from_hpr(hpr), scale: s, trans: t };
            self.blend_ms = ms;
            self.blend_start = sc.time;
        }
    }
}

/// `VIHashRefID`: h = h*131 + c.
pub fn hash_ref_id(s: &str) -> i32 {
    crate::hash::hash_resource_id(s) as i32
}
