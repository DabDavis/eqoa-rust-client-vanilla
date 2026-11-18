//! A pet in a zone's prop (go-eqoa-server's own: a mastery rank's `PetArt`,
//! its `decorart` rows of kind pet, `net::decor::PetArt`; the server owner,
//! 2026-11-17, of Zerkziee's construct: "where dust is is a catapult prop. we
//! have templates for props as mobs now. lets give it to zerkziee. it can ram
//! for attack and we can make the catapult fire").
//!
//! The server sends the pet's channel with the prop as its model (Tunaria's
//! catapult, 0xE64B8D78) only to a client that answered the decorations at
//! layout 3, in a world whose zone holding it the walk reads
//! (`decorlights::art_zones`); a PS2 sees the pet's own body. A prop has no
//! skeleton and no actions, so what CLIENT would play on a character is drawn
//! here from the casts the server sends of it (0x00B1):
//!
//! - **a swing** (a melee round's, its time 3000 ms or more: `VISwing::Delay`)
//!   is a ram: the whole prop lunges toward its target at each of the round's
//!   hits (the server's `swingHits`: Attack4's 0.5, 1, 1.7 and 2.4 s), out fast
//!   and back slower ([`RAM_OUT`], [`RAM_BACK`]), [`RAM_REACH`] of its own
//!   length at the most;
//! - **any other cast at another** (its Tinker Kick, every 12 s) fires the
//!   row's shot (the boulder beside the catapults, 0xBFEE4CCD) from the top of
//!   its arm in an arc onto the target, tumbling, as the prop rocks back
//!   ([`RECOIL`]); the cast's own visual plays on the target as ever.
//!
//! The prop is drawn at the row's scale times the pet's size, its name plate
//! over its top ([`Tops`]).
//!
//! Its moving parts (the owner, 2026-11-18: "the wheels for the catapult
//! should rotate and the arm for the catapult should swing to throw the
//! rocks") are found in its own geometry ([`rig`]) and drawn as a skinned
//! model, a palette matrix a part ([`Rig::palette`]):
//!
//! - **the wheels**: each axle with its two wheels is a piece of its own (the
//!   triangles joined by shared corners), round seen from the side and down
//!   to the ground: it turns about its axle's centre by the distance the pet
//!   rolled along its own forward over the wheels' radius ([`State::roll`]),
//!   backward too, still while it stands;
//! - **the throwing arm**: the beam over the deck in the frame's middle
//!   (its triangles' centres within [`ARM_HALF`] of the middle and over the
//!   deck), pivoting at the crossbar its front end sits in. At a shot it
//!   swings forward fast ([`ARM_UP`]) until its tip is straight over the pivot,
//!   where the boulder leaves from the tip, on a little past it, and winds
//!   back slowly ([`ARM_BACK`]) ([`arm_angle`]).

use std::collections::HashMap;
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::vect::Vect3;

/// How long a ram's lunge takes to reach its farthest (ms).
pub const RAM_OUT: i64 = 140;
/// And to come back from it (ms).
pub const RAM_BACK: i64 = 260;
/// How far a ram lunges, of the prop's drawn length, at the most.
pub const RAM_REACH: f32 = 0.22;
/// How far a shot rocks the prop back (of its length) and for how long (ms).
pub const RECOIL: (f32, i64) = (0.06, 300);
/// The arm's swing: from rest to the release (its tip over the pivot), ms.
pub const ARM_UP: i64 = 220;
/// Past the release, how far (radians) and for how long (ms) it goes on.
pub const ARM_OVER: (f32, i64) = (0.35, 90);
/// How long it rests there, then how long it winds back to rest (ms).
pub const ARM_HOLD: i64 = 160;
pub const ARM_BACK: i64 = 1600;
/// The arm's triangles: their centres within this of the prop's middle
/// across (its own units), and over the frame's lowest by [`ARM_OVER_DECK`].
pub const ARM_HALF: f32 = 0.75;
pub const ARM_OVER_DECK: f32 = 1.05;
/// How near the arm's tip a piece of its own must come to be its bracket,
/// which swings with it; and how far short of the tip (along z from the
/// pivot) the frame's triangles joined to the arm's end begin to be its cup
/// (the broad plate the boulder sits in).
pub const SPOON_REACH: f32 = 0.75;
/// And no longer nor higher than this (the catapult's spoon is 0.44 long and
/// 1.45 high; the deck's rod under the arm's end runs its whole length).
pub const SPOON_SIZE: f32 = 2.0;
/// A move longer than this in one frame (a teleport, a new place) turns no
/// wheel.
pub const ROLL_JUMP: f32 = 6.0;
/// The palette's slot of the frame and of the arm; the wheels follow.
pub const PART_FRAME: u8 = 0;
pub const PART_ARM: u8 = 1;
pub const PART_WHEEL0: u8 = 2;

/// One ram's lunge: when it starts and whom it is at.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Lunge {
    at: i64,
    target: u32,
    /// how far, at its farthest (world units)
    reach: f32,
    /// a shot's recoil: away from the target
    back: bool,
}

/// A boulder in flight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub model: u32,
    pub scale: f32,
    pub from: [f32; 3],
    pub target: u32,
    /// the target's last place seen (its chest)
    pub to: [f32; 3],
    pub start: i64,
    pub flight: i64,
}

/// The rams and shots of every pet in a prop, by its entity slot.
#[derive(Default)]
pub struct State {
    lunges: HashMap<usize, Vec<Lunge>>,
    /// each prop's last throw (when its arm began to swing)
    throws: HashMap<usize, i64>,
    /// each prop's place last seen and the distance it has rolled
    rolls: HashMap<usize, ([f32; 3], f32)>,
    /// each prop's arm's tip at its release, in the world, as last drawn
    tips: HashMap<usize, [f32; 3]>,
    pub shots: Vec<Shot>,
    /// this frame's shots, each its model and transform (`flying`), drawn
    /// with the projectiles
    pub drawn: Vec<(u32, Matrix44<Ee>)>,
}

/// The times of a round's hits from its swing's delay (ms), as the server's
/// `swingHits` by its `VISwing::Delay`: 6000 is Attack4's four, 4000 and 5000
/// the two of Attack2/3, anything else one.
pub fn round_hits(delay: i32) -> &'static [i64] {
    match delay {
        6000.. => &[500, 1000, 1700, 2400],
        4000..=5999 => &[500, 1000],
        _ => &[500],
    }
}

/// Whether a cast of `time` ms is a melee round's swing (a ram) rather than a
/// cast (a shot): `VISwing::Delay` is 3000 ms at the least.
pub fn is_swing(time: i32) -> bool {
    time >= 3000
}

/// A curve of a lunge `t` ms in: 0 to 1 over [`RAM_OUT`] (eased out), back to
/// 0 over [`RAM_BACK`] (eased in and out), then 0.
pub fn lunge_curve(t: i64) -> f32 {
    if t < 0 {
        0.0
    } else if t < RAM_OUT {
        let u = t as f32 / RAM_OUT as f32;
        1.0 - (1.0 - u) * (1.0 - u)
    } else if t < RAM_OUT + RAM_BACK {
        let u = (t - RAM_OUT) as f32 / RAM_BACK as f32;
        0.5 + 0.5 * (u * std::f32::consts::PI).cos()
    } else {
        0.0
    }
}

/// How long a shot flies `d` units (ms): a quick lob, longer the farther.
pub fn flight_ms(d: f32) -> i64 {
    (420.0 + 22.0 * d).clamp(420.0, 1100.0) as i64
}

/// Where a shot is `u` (0 to 1) along its flight from `from` to `to`: a
/// parabola over the straight line, its top 1.5 plus a fifth of the distance
/// over the higher end's.
pub fn arc(from: [f32; 3], to: [f32; 3], u: f32) -> [f32; 3] {
    let d = ((to[0] - from[0]).powi(2) + (to[2] - from[2]).powi(2)).sqrt();
    let h = 1.5 + 0.2 * d;
    let lift = 4.0 * h * u * (1.0 - u);
    [from[0] + (to[0] - from[0]) * u, from[1] + (to[1] - from[1]) * u + lift, from[2] + (to[2] - from[2]) * u]
}

/// The arm's turn `t` ms into a throw whose release (the tip straight over
/// the pivot) is `release` radians from rest: speeding up to it over
/// [`ARM_UP`], on past it by [`ARM_OVER`] slowing, held [`ARM_HOLD`], then
/// eased back to rest over [`ARM_BACK`]; 0 before and after.
pub fn arm_angle(t: i64, release: f32) -> f32 {
    let over = release + ARM_OVER.0 * release.signum();
    if t < 0 {
        0.0
    } else if t < ARM_UP {
        let u = t as f32 / ARM_UP as f32;
        release * u * u
    } else if t < ARM_UP + ARM_OVER.1 {
        let u = (t - ARM_UP) as f32 / ARM_OVER.1 as f32;
        release + (over - release) * (1.0 - (1.0 - u) * (1.0 - u))
    } else if t < ARM_UP + ARM_OVER.1 + ARM_HOLD {
        over
    } else if t < ARM_UP + ARM_OVER.1 + ARM_HOLD + ARM_BACK {
        let u = (t - ARM_UP - ARM_OVER.1 - ARM_HOLD) as f32 / ARM_BACK as f32;
        over * (1.0 - u * u * (3.0 - 2.0 * u))
    } else {
        0.0
    }
}

/// The wheels' turn (radians) after rolling `dist` on wheels of `radius`:
/// forward turns the top of a wheel forward (+ about its axle, the prop's x).
pub fn wheel_angle(dist: f32, radius: f32) -> f32 {
    if radius > 1e-3 { dist / radius } else { 0.0 }
}

/// A wheel's axle: its centre (its own units) and its wheels' radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wheel {
    pub centre: [f32; 3],
    pub radius: f32,
}

/// The arm: where it pivots, its tip at rest, and the turn (radians about
/// the prop's x) that sets the tip straight over the pivot: the release.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Arm {
    pub pivot: [f32; 3],
    pub tip: [f32; 3],
    pub release: f32,
}

/// A prop's moving parts ([`rig`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Rig {
    pub wheels: Vec<Wheel>,
    pub arm: Option<Arm>,
    /// the triangles of each part (frame, arm, the wheels), for the record
    pub tris: Vec<usize>,
}

/// A turn of `a` about the prop's x through `at`, as a palette matrix.
fn turn_x(at: [f32; 3], a: f32) -> [[f32; 4]; 4] {
    let p = glam::Vec3::from(at);
    (glam::Mat4::from_translation(p) * glam::Mat4::from_rotation_x(a) * glam::Mat4::from_translation(-p)).to_cols_array_2d()
}

/// `p` turned `a` about the prop's x through `at`.
pub fn turned(p: [f32; 3], at: [f32; 3], a: f32) -> [f32; 3] {
    let (y, z) = (p[1] - at[1], p[2] - at[2]);
    let (s, c) = a.sin_cos();
    [p[0], at[1] + y * c - z * s, at[2] + y * s + z * c]
}

impl Rig {
    /// The palette: the frame still, the arm turned `arm`, every wheel turned
    /// by `rolled` over its radius.
    pub fn palette(&self, rolled: f32, arm: f32) -> Vec<[[f32; 4]; 4]> {
        let mut out = vec![turn_x([0.0; 3], 0.0), self.arm.map_or(turn_x([0.0; 3], 0.0), |a| turn_x(a.pivot, arm))];
        out.extend(self.wheels.iter().map(|w| turn_x(w.centre, wheel_angle(rolled, w.radius))));
        out
    }

    /// The arm's tip, its own units, at its release.
    pub fn release_tip(&self) -> Option<[f32; 3]> {
        self.arm.map(|a| turned(a.tip, a.pivot, a.release))
    }
}

/// Splits a prop's model into its moving parts, from its geometry alone, and
/// makes it a skinned model of them (each vertex wholly one part's: the
/// palette's [`PART_FRAME`], [`PART_ARM`], [`PART_WHEEL0`] on), a corner
/// shared by two parts copied; none where it has no wheel and no arm.
///
/// Its pieces are the triangles joined by shared corners (by place). A wheel
/// piece reaches down to the prop's lowest (within a tenth of its height),
/// is about as long as it is high (round from the side, within a third) and
/// wider across (x) than long (an axle with a wheel at each end): it turns
/// about its box's centre, its radius half its height. The frame is the
/// largest of the rest; its arm the triangles whose centres are within
/// [`ARM_HALF`] of the middle across and [`ARM_OVER_DECK`] over the frame's
/// lowest. Its pivot is the centre of the crossbar (another piece wider
/// across than [`ARM_HALF`] twice, nearly level) nearest (within 2) one of the arm's two
/// ends along z, that end then; its tip the arm's corner farthest from it.
/// The arm also carries its cup (the frame's triangles joined to its end and
/// wholly past its tip less [`SPOON_REACH`]) and any compact piece by its tip
/// (an iron bracket); the boulder leaves from the cup's middle.
pub fn rig(m: &mut esf::Model) -> Option<Rig> {
    // every triangle: (batch, first index's place in it)
    let tris: Vec<(usize, usize)> = m.batches.iter().enumerate().flat_map(|(b, bt)| (0..bt.indices.len() / 3).map(move |t| (b, t * 3))).collect();
    if tris.is_empty() || m.skinned {
        return None;
    }
    let corner = |&(b, i): &(usize, usize), k: usize| m.batches[b].indices[i + k] as usize;
    // pieces: union-find by a corner's place
    let mut parent: Vec<usize> = (0..tris.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let mut at: HashMap<[u32; 3], usize> = HashMap::new();
    for (t, tri) in tris.iter().enumerate() {
        for k in 0..3 {
            let p = m.pos[corner(tri, k)];
            let key = [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
            let first = *at.entry(key).or_insert(t);
            let (a, b) = (find(&mut parent, first), find(&mut parent, t));
            parent[a] = b;
        }
    }
    let mut pieces: HashMap<usize, Vec<usize>> = HashMap::new();
    for t in 0..tris.len() {
        let r = find(&mut parent, t);
        pieces.entry(r).or_default().push(t);
    }
    let box_of = |ts: &[usize]| {
        let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for &t in ts {
            for k in 0..3 {
                let p = m.pos[corner(&tris[t], k)];
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
        }
        (lo, hi)
    };
    let (lo, hi) = box_of(&(0..tris.len()).collect::<Vec<_>>());
    let height = hi[1] - lo[1];
    let mid = (lo[0] + hi[0]) * 0.5;
    let mut pieces: Vec<Vec<usize>> = pieces.into_values().collect();
    pieces.sort_by_key(|p| (std::cmp::Reverse(p.len()), p[0]));
    let mut part = vec![PART_FRAME; tris.len()];
    let mut wheels = Vec::new();
    let mut rest = Vec::new();
    for p in &pieces {
        let (l, h) = box_of(p);
        let (across, high, along) = (h[0] - l[0], h[1] - l[1], h[2] - l[2]);
        let round = high > 0.0 && (along / high - 1.0).abs() < 0.34;
        if l[1] < lo[1] + 0.1 * height && round && across > along && wheels.len() + PART_WHEEL0 as usize <= 64 {
            for &t in p {
                part[t] = PART_WHEEL0 + wheels.len() as u8;
            }
            wheels.push(Wheel { centre: [(l[0] + h[0]) * 0.5, (l[1] + h[1]) * 0.5, (l[2] + h[2]) * 0.5], radius: high * 0.5 });
        } else {
            rest.push(p);
        }
    }
    // the arm, in the frame: the largest piece left
    let mut arm = None;
    if let Some(frame) = rest.first() {
        let (fl, _) = box_of(frame);
        let centre = |t: usize| {
            let c: Vec<[f32; 3]> = (0..3).map(|k| m.pos[corner(&tris[t], k)]).collect();
            [(c[0][0] + c[1][0] + c[2][0]) / 3.0, (c[0][1] + c[1][1] + c[2][1]) / 3.0, (c[0][2] + c[1][2] + c[2][2]) / 3.0]
        };
        let beam: Vec<usize> = frame.iter().copied().filter(|&t| {
            let c = centre(t);
            (c[0] - mid).abs() < ARM_HALF && c[1] > fl[1] + ARM_OVER_DECK
        }).collect();
        if beam.len() >= 4 {
            let (bl, bh) = box_of(&beam);
            // its two ends along z, each at the height of its corners there
            let end = |z: f32| {
                let ys: Vec<f32> = beam.iter().flat_map(|&t| (0..3).map(move |k| (t, k))).map(|(t, k)| m.pos[corner(&tris[t], k)]).filter(|p| (p[2] - z).abs() < 0.6).map(|p| p[1]).collect();
                [mid, ys.iter().sum::<f32>() / ys.len().max(1) as f32, z]
            };
            let ends = [end(bl[2]), end(bh[2])];
            // the crossbar nearest an end
            let mut best: Option<(f32, [f32; 3])> = None;
            for p in rest.iter().skip(1) {
                let (l, h) = box_of(p);
                if h[0] - l[0] < 2.0 * ARM_HALF || h[1] - l[1] > 0.6 * (h[0] - l[0]) {
                    continue;
                }
                let c = [(l[0] + h[0]) * 0.5, (l[1] + h[1]) * 0.5, (l[2] + h[2]) * 0.5];
                for e in &ends {
                    let d = ((c[1] - e[1]).powi(2) + (c[2] - e[2]).powi(2)).sqrt();
                    if d < 2.0 && best.is_none_or(|(b, _)| d < b) {
                        best = Some((d, c));
                    }
                }
            }
            let pivot = best.map_or(ends[1], |(_, c)| [mid, c[1], c[2]]);
            let mut tip = pivot;
            let mut far = 0.0;
            for &t in &beam {
                for k in 0..3 {
                    let p = m.pos[corner(&tris[t], k)];
                    let d = (p[1] - pivot[1]).powi(2) + (p[2] - pivot[2]).powi(2);
                    if d > far {
                        (far, tip) = (d, p);
                    }
                }
            }
            if far > 0.25 {
                for &t in &beam {
                    part[t] = PART_ARM;
                }
                // the cup: the broad wooden plate the arm's end carries (the
                // owner's spoon, 2026-11-18), the frame's own triangles (it
                // shares the beam's end corners but none of the deck's, which
                // it rests just over), grown from the beam through shared
                // corners over the frame's triangles lying wholly at the tip's
                // end: every corner past the tip less SPOON_REACH, along z
                // from the pivot
                let dir = (tip[2] - pivot[2]).signum();
                let past = (tip[2] - pivot[2]).abs() - SPOON_REACH;
                let key = |p: [f32; 3]| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
                let mut by_corner: HashMap<[u32; 3], Vec<usize>> = HashMap::new();
                for &t in frame.iter() {
                    for k in 0..3 {
                        by_corner.entry(key(m.pos[corner(&tris[t], k)])).or_default().push(t);
                    }
                }
                let at_end = |t: usize| (0..3).all(|k| (m.pos[corner(&tris[t], k)][2] - pivot[2]) * dir >= past);
                let mut cup: Vec<usize> = Vec::new();
                let mut grow: Vec<usize> = beam.clone();
                while let Some(t) = grow.pop() {
                    for k in 0..3 {
                        for &n in by_corner.get(&key(m.pos[corner(&tris[t], k)])).map_or(&[][..], |v| &v[..]) {
                            if part[n] != PART_ARM && at_end(n) {
                                part[n] = PART_ARM;
                                cup.push(n);
                                grow.push(n);
                            }
                        }
                    }
                }
                // the spoon: a piece of its own at the arm's end (the owner,
                // 2026-11-18: "the catapult end part that looks like a spoon
                // is not firing with the arm"), any piece but the frame and
                // the wheels whose box comes within SPOON_REACH of the tip
                // (in y and z), compact (no longer nor higher than
                // SPOON_SIZE: not the deck's long rod under it) and no wider
                // across than the arm's beam twice; it goes with the arm, and
                // the boulder leaves from its middle
                let mut spoon: Option<([f32; 3], [f32; 3])> = None;
                for p in rest.iter().skip(1) {
                    let (l, h) = box_of(p);
                    let gap = |v: f32, lo: f32, hi: f32| if v < lo { lo - v } else if v > hi { v - hi } else { 0.0 };
                    let d = (gap(tip[1], l[1], h[1]).powi(2) + gap(tip[2], l[2], h[2]).powi(2)).sqrt();
                    if d <= SPOON_REACH && h[0] - l[0] <= 4.0 * ARM_HALF && h[1] - l[1] <= SPOON_SIZE && h[2] - l[2] <= SPOON_SIZE {
                        for &t in p.iter() {
                            part[t] = PART_ARM;
                        }
                        spoon = Some(match spoon {
                            Some((a, b)) => ([a[0].min(l[0]), a[1].min(l[1]), a[2].min(l[2])], [b[0].max(h[0]), b[1].max(h[1]), b[2].max(h[2])]),
                            None => (l, h),
                        });
                    }
                }
                // the boulder leaves from the cup's middle, else the bracket's
                if cup.len() >= 4 {
                    let (l, h) = box_of(&cup);
                    tip = [(l[0] + h[0]) * 0.5, (l[1] + h[1]) * 0.5, (l[2] + h[2]) * 0.5];
                } else if let Some((l, h)) = spoon {
                    tip = [(l[0] + h[0]) * 0.5, (l[1] + h[1]) * 0.5, (l[2] + h[2]) * 0.5];
                }
                let release = (-(tip[2] - pivot[2])).atan2(tip[1] - pivot[1]);
                arm = Some(Arm { pivot, tip, release });
            }
        }
    }
    if wheels.is_empty() && arm.is_none() {
        return None;
    }
    // each vertex one part's: a corner shared by two copied
    let n = m.pos.len();
    let mut owner: Vec<Option<u8>> = vec![None; n];
    let mut copies: HashMap<(usize, u8), u32> = HashMap::new();
    let mut bones = vec![[255u8, 0, 0, 0]; n];
    let mut weights = vec![[0u8; 4]; n];
    for (t, &(b, i)) in tris.iter().enumerate() {
        let want = part[t];
        for k in 0..3 {
            let v = m.batches[b].indices[i + k] as usize;
            match owner[v] {
                None => {
                    owner[v] = Some(want);
                    bones[v] = [want, 0, 0, 0];
                    weights[v] = [255, 0, 0, 0];
                }
                Some(o) if o == want => {}
                Some(_) => {
                    let c = *copies.entry((v, want)).or_insert_with(|| {
                        let c = m.pos.len() as u32;
                        m.pos.push(m.pos[v]);
                        if let Some(&x) = m.uv.get(v) {
                            m.uv.push(x);
                        }
                        if let Some(&x) = m.normal.get(v) {
                            m.normal.push(x);
                        }
                        if let Some(&x) = m.color.get(v) {
                            m.color.push(x);
                        }
                        bones.push([want, 0, 0, 0]);
                        weights.push([255, 0, 0, 0]);
                        c
                    });
                    m.batches[b].indices[i + k] = c;
                }
            }
        }
    }
    m.bones = bones;
    m.weights = weights;
    m.skinned = true;
    let mut counts = vec![0usize; PART_WHEEL0 as usize + wheels.len()];
    for &p in &part {
        counts[p as usize] += 1;
    }
    Some(Rig { wheels, arm, tris: counts })
}

/// The catapult's two dye areas as go-eqoa-server's `petdyeareas` row has
/// them (the owner, 2026-11-18: "the catapult pet needs two tone color
/// regions for pet groomer"): area 1 its wood (the frame's planks 0x10A8243D,
/// material 0; the green-painted boards 0xC16FFA97, 2; the stone wheels
/// 0xD0793459, 3), area 2 its iron (0x6140DAAF, material 1: the arm, the
/// fittings, the spikes). Vanilla dyes it as any pet's two areas by mask
/// (`petdye::apply`), the server sending the dyes for the art it draws.
pub const CATAPULT_IRON: u64 = 1 << 1;

/// `m` dyed in two areas (0xRRGGBBAA each, 0 none), area 2 the materials of
/// `mask`, as `petdye::apply` dyes a pet: the viewer's picture of a dye.
pub fn dye(m: &mut esf::Model, area: [u32; 2], mask: u64) {
    crate::petdye::apply(m, net::petdye::PetDye { look: 0, area, mask, rule: false });
}

/// `m` as its rig draws it with `rolled` and `arm` ([`Rig::palette`]),
/// baked into its corners and unskinned: the viewer's picture of a pose
/// (`eqoa shot <world> zonemodel <zone> <id> out.png` with
/// `EQOA_PROP_POSE=<arm radians>,<rolled>`).
pub fn posed(m: &mut esf::Model, rolled: f32, arm: f32) -> Option<Rig> {
    let r = rig(m)?;
    let pal: Vec<glam::Mat4> = r.palette(rolled, arm).iter().map(glam::Mat4::from_cols_array_2d).collect();
    for (i, p) in m.pos.iter_mut().enumerate() {
        if let Some(t) = m.bones.get(i).and_then(|b| pal.get(b[0] as usize)) {
            *p = t.transform_point3(glam::Vec3::from(*p)).to_array();
        }
    }
    for (i, n) in m.normal.iter_mut().enumerate() {
        if let Some(t) = m.bones.get(i).and_then(|b| pal.get(b[0] as usize)) {
            *n = t.transform_vector3(glam::Vec3::from(*n)).to_array();
        }
    }
    m.bones.clear();
    m.weights.clear();
    m.skinned = false;
    Some(r)
}

impl State {
    /// A cast of the prop in `slot` (the pet `art`, at `pos`, `length` long
    /// and `top` high as drawn) at `target` (at `to`, its chest, if seen), of
    /// `time` ms, at `now`: a swing's rams at its round's hits, or a shot:
    /// its arm swung, the boulder leaving `release` (ms on and the arm's tip
    /// then, in the world) or, a prop with no arm, at once from its top.
    #[allow(clippy::too_many_arguments)]
    pub fn cast(&mut self, slot: usize, art: &net::decor::PetArt, pos: [f32; 3], length: f32, top: f32, target: u32, to: Option<[f32; 3]>, time: i32, now: i64, release: Option<(i64, [f32; 3])>) {
        let Some(to) = to else { return };
        let lunges = self.lunges.entry(slot).or_default();
        lunges.retain(|l| now - l.at < RAM_OUT + RAM_BACK);
        if is_swing(time) {
            for &h in round_hits(time) {
                lunges.push(Lunge { at: now + h - RAM_OUT, target, reach: RAM_REACH * length, back: false });
            }
            return;
        }
        if art.shot == 0 {
            return;
        }
        let (delay, from) = release.unwrap_or((0, [pos[0], pos[1] + top * 0.95, pos[2]]));
        if release.is_some() {
            self.throws.insert(slot, now);
        }
        lunges.push(Lunge { at: now + delay, target, reach: RECOIL.0 * length, back: true });
        let d = ((to[0] - from[0]).powi(2) + (to[2] - from[2]).powi(2)).sqrt();
        let scale = if art.shot_scale > 0.0 { art.shot_scale } else { 1.0 };
        self.shots.push(Shot { model: art.shot, scale, from, target, to, start: now + delay, flight: flight_ms(d) });
    }

    /// How far the prop in `slot` at `pos` is moved now by its rams and a
    /// shot's recoil (world units, x and z), toward (or away from) each one's
    /// target where `place` finds it.
    pub fn offset(&mut self, slot: usize, pos: [f32; 3], now: i64, place: impl Fn(u32) -> Option<[f32; 3]>) -> [f32; 3] {
        let Some(lunges) = self.lunges.get_mut(&slot) else { return [0.0; 3] };
        lunges.retain(|l| now - l.at < RAM_OUT + RAM_BACK);
        let mut off = [0.0f32; 3];
        for l in lunges.iter() {
            let k = lunge_curve(now - l.at) * l.reach;
            let Some(t) = place(l.target) else { continue };
            let (dx, dz) = (t[0] - pos[0], t[2] - pos[2]);
            let n = (dx * dx + dz * dz).sqrt();
            if n < 1e-3 || k == 0.0 {
                continue;
            }
            let s = if l.back { -k } else { k } / n;
            off[0] += dx * s;
            off[2] += dz * s;
        }
        if lunges.is_empty() {
            self.lunges.remove(&slot);
        }
        off
    }

    /// Forgets the prop in `slot` (its entity gone or another).
    pub fn forget(&mut self, slot: usize) {
        self.lunges.remove(&slot);
        self.throws.remove(&slot);
        self.rolls.remove(&slot);
        self.tips.remove(&slot);
    }

    /// Keeps where the prop in `slot` lets its boulder go (none: no arm).
    pub fn set_tip(&mut self, slot: usize, tip: Option<[f32; 3]>) {
        match tip {
            Some(t) => self.tips.insert(slot, t),
            None => self.tips.remove(&slot),
        };
    }

    /// Where the prop in `slot` lets its boulder go, as last drawn.
    pub fn tip(&self, slot: usize) -> Option<[f32; 3]> {
        self.tips.get(&slot).copied()
    }

    /// The distance the prop in `slot` has rolled, now at `pos` with its own
    /// forward `forward` (the actor's +z, in the world): what it moved since
    /// the last frame along its forward added (backward taken off), a jump
    /// past [`ROLL_JUMP`] not counted.
    pub fn roll(&mut self, slot: usize, pos: [f32; 3], forward: [f32; 3]) -> f32 {
        let (last, dist) = self.rolls.entry(slot).or_insert((pos, 0.0));
        let d = [pos[0] - last[0], pos[1] - last[1], pos[2] - last[2]];
        let n = (forward[0] * forward[0] + forward[1] * forward[1] + forward[2] * forward[2]).sqrt();
        if n > 1e-6 && (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) < ROLL_JUMP * ROLL_JUMP {
            *dist += (d[0] * forward[0] + d[1] * forward[1] + d[2] * forward[2]) / n;
        }
        *last = pos;
        *dist
    }

    /// The arm's turn now of the prop in `slot` whose release is at `release`
    /// radians ([`arm_angle`]); 0 at rest.
    pub fn arm(&self, slot: usize, now: i64, release: f32) -> f32 {
        self.throws.get(&slot).map_or(0.0, |&at| arm_angle(now - at, release))
    }

    /// The shots in flight now, each its model and transform (a tumble about
    /// its flight), the ended dropped; a shot's target followed where `place`
    /// finds it.
    pub fn flying(&mut self, now: i64, place: impl Fn(u32) -> Option<[f32; 3]>) -> Vec<(u32, Matrix44<Ee>)> {
        self.shots.retain(|s| now - s.start < s.flight);
        let f = F::<Ee>::new;
        let mut out = Vec::new();
        for s in &mut self.shots {
            // still in the arm's swing
            if now < s.start {
                continue;
            }
            if let Some(t) = place(s.target) {
                s.to = t;
            }
            let u = ((now - s.start) as f32 / s.flight as f32).clamp(0.0, 1.0);
            let p = arc(s.from, s.to, u);
            let spin = u * 7.0;
            out.push((s.model, Matrix44::transform(&Vect3::new(f(spin * 0.7), f(spin), F::ZERO), f(s.scale), &Vect3::new(f(p[0]), f(p[1]), f(p[2])))));
        }
        out
    }
}

/// Each prop's drawn bounds by DictID, read once off its model: its length
/// (the longer of across and along) and its top.
#[derive(Default)]
pub struct Tops(HashMap<u32, Option<(f32, f32)>>);

impl Tops {
    /// The prop `id`'s (length, top) at scale 1, if the library has it.
    pub fn of(&mut self, lib: &esf::Library, id: u32) -> Option<(f32, f32)> {
        *self.0.entry(id).or_insert_with(|| {
            let m = lib.lookup(id as i32).and_then(|r| lib.build_model(r)).or_else(|| crate::decorlights::pet_asset_model(id))?;
            bounds(&m.pos)
        })
    }
}

/// The (length, top) of points: the longer of the x and z spans, the
/// highest y.
pub fn bounds(pos: &[[f32; 3]]) -> Option<(f32, f32)> {
    if pos.is_empty() {
        return None;
    }
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in pos {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    Some(((hi[0] - lo[0]).max(hi[2] - lo[2]), hi[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catapult() -> net::decor::PetArt {
        net::decor::PetArt { model: 0xE64B_8D78, kind: net::decor::PET_ART, shot: 0xBFEE_4CCD, shot_scale: 0.3, scale: 0.35 }
    }

    #[test]
    fn a_round_of_four_is_four_rams_toward_the_target() {
        let mut s = State::default();
        let target = |_| Some([10.0, 0.0, 0.0]);
        s.cast(3, &catapult(), [0.0; 3], 5.0, 2.0, 77, Some([10.0, 1.0, 0.0]), 6000, 1000, None);
        assert!(s.shots.is_empty(), "a swing fires nothing");
        // before the first hit's lunge, at rest
        assert_eq!(s.offset(3, [0.0; 3], 1000, target), [0.0; 3]);
        // at each hit, at its farthest, toward the target along x
        for h in round_hits(6000) {
            let o = s.offset(3, [0.0; 3], 1000 + h, target);
            assert!((o[0] - RAM_REACH * 5.0).abs() < 1e-4 && o[2] == 0.0, "at {h}: {o:?}");
        }
        // and back at rest after the last
        assert_eq!(s.offset(3, [0.0; 3], 1000 + 2400 + RAM_BACK + 1, target), [0.0; 3]);
        assert_eq!(round_hits(3000), &[500]);
    }

    #[test]
    fn a_cast_fires_the_shot_in_an_arc_onto_the_target() {
        let mut s = State::default();
        s.cast(2, &catapult(), [0.0, 0.0, 0.0], 5.0, 2.0, 9, Some([8.0, 1.0, 6.0]), 0, 500, None);
        assert_eq!(s.shots.len(), 1);
        let sh = s.shots[0];
        assert_eq!((sh.model, sh.scale, sh.from, sh.flight), (0xBFEE_4CCD, 0.3, [0.0, 1.9, 0.0], flight_ms(10.0)));
        // the prop rocks back, away from the target
        let o = s.offset(2, [0.0; 3], 500 + RAM_OUT, |_| Some([8.0, 1.0, 6.0]));
        assert!(o[0] < 0.0 && o[2] < 0.0, "{o:?}");
        // in flight: over the line between, highest at the middle
        let mid = arc(sh.from, sh.to, 0.5);
        assert!(mid[1] > 1.9 && (mid[0] - 4.0).abs() < 1e-4);
        assert_eq!(arc(sh.from, sh.to, 1.0), [8.0, 1.0, 6.0]);
        assert_eq!(s.flying(600, |_| None).len(), 1);
        // landed: gone
        assert!(s.flying(500 + sh.flight, |_| None).is_empty());
        assert!(s.shots.is_empty());
    }

    #[test]
    fn no_shot_without_one_or_a_target() {
        let mut s = State::default();
        let mut a = catapult();
        s.cast(1, &a, [0.0; 3], 5.0, 2.0, 9, None, 0, 0, None);
        a.shot = 0;
        s.cast(1, &a, [0.0; 3], 5.0, 2.0, 9, Some([1.0, 0.0, 0.0]), 0, 0, None);
        assert!(s.shots.is_empty());
    }

    #[test]
    fn the_wheels_turn_with_the_distance_rolled_both_ways() {
        let r = 1.7148438;
        assert!((wheel_angle(std::f32::consts::TAU * r, r) - std::f32::consts::TAU).abs() < 1e-5, "a turn a circumference");
        let mut s = State::default();
        let fwd = [0.0, 0.0, 2.0];
        assert_eq!(s.roll(4, [0.0; 3], fwd), 0.0, "standing: still");
        assert!((s.roll(4, [0.0, 0.0, 1.5], fwd) - 1.5).abs() < 1e-6, "forward along its own +z");
        assert!((s.roll(4, [1.0, 0.0, 1.5], fwd) - 1.5).abs() < 1e-6, "sideways: no turn");
        assert!((s.roll(4, [1.0, 0.0, 0.5], fwd) - 0.5).abs() < 1e-6, "backward: turned back");
        assert_eq!(s.roll(4, [1.0, 0.0, 0.5], fwd), s.roll(4, [1.0, 0.0, 0.5], fwd), "standing again: still");
        assert!((s.roll(4, [1.0, 0.0, 40.0], fwd) - 0.5).abs() < 1e-6, "a jump turns nothing");
        s.forget(4);
        assert_eq!(s.roll(4, [9.0; 3], fwd), 0.0, "forgotten: from naught");
    }

    #[test]
    fn the_arm_swings_fast_to_its_release_and_winds_back_slowly() {
        let rel = 1.53;
        assert_eq!(arm_angle(-1, rel), 0.0);
        assert_eq!(arm_angle(0, rel), 0.0);
        assert!((arm_angle(ARM_UP, rel) - rel).abs() < 1e-6, "the release at ARM_UP");
        assert!(arm_angle(ARM_UP / 2, rel) < rel * 0.5, "speeding up");
        let over = rel + ARM_OVER.0;
        assert!((arm_angle(ARM_UP + ARM_OVER.1 + 10, rel) - over).abs() < 1e-6, "held past it");
        let back = ARM_UP + ARM_OVER.1 + ARM_HOLD;
        assert!(arm_angle(back + ARM_BACK / 2, rel) < over && arm_angle(back + ARM_BACK / 2, rel) > 0.0, "winding back");
        assert_eq!(arm_angle(back + ARM_BACK, rel), 0.0, "at rest");
        assert!(ARM_BACK > 5 * ARM_UP, "back slowly, up fast");
        // a pivot on the other side swings the other way
        assert!((arm_angle(ARM_UP, -rel) + rel).abs() < 1e-6);
        // the tip at the release is straight over the pivot
        let (pivot, tip): ([f32; 3], [f32; 3]) = ([0.0, 2.24, 5.55], [0.0, 2.7, -5.54]);
        let release = (-(tip[2] - pivot[2])).atan2(tip[1] - pivot[1]);
        let t = turned(tip, pivot, release);
        assert!((t[2] - pivot[2]).abs() < 1e-4 && t[1] > pivot[1] + 11.0, "{t:?}");
    }

    #[test]
    fn a_shot_with_an_arm_leaves_from_the_tip_at_the_release() {
        let mut s = State::default();
        let tip = [0.0, 6.0, 1.0];
        s.cast(2, &catapult(), [0.0; 3], 5.0, 2.0, 9, Some([8.0, 1.0, 6.0]), 0, 500, Some((ARM_UP, tip)));
        assert_eq!((s.shots[0].from, s.shots[0].start), (tip, 500 + ARM_UP));
        assert!(s.flying(500 + ARM_UP - 1, |_| None).is_empty(), "still in the arm");
        assert_eq!(s.flying(500 + ARM_UP + 1, |_| None).len(), 1);
        assert!((s.arm(2, 500 + ARM_UP, 1.5) - 1.5).abs() < 1e-6, "the arm at its release then");
    }

    #[test]
    fn bounds_are_the_longer_span_and_the_top() {
        assert_eq!(bounds(&[[-4.0, 0.0, -8.0], [4.0, 6.5, 9.0]]), Some((17.0, 6.5)));
        assert_eq!(bounds(&[]), None);
    }
}

/// The catapult itself off the disc (`EQOA_ISO`; skipped without it):
/// Tunaria's zone 108's 0xE64B8D78 split into its frame, arm and two axles.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod disc_tests {
    use super::*;

    /// Tunaria zone `zone`'s sprite `id`, built, if the disc is there.
    pub(crate) fn zone_model(zone: usize, id: u32) -> Option<esf::Model> {
        // `EQOA_ISO`, else the checkout's `run/eqoa.iso`
        let iso = std::env::var("EQOA_ISO").ok().or_else(|| {
            let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
            p.exists().then(|| p.to_string_lossy().into_owned())
        })?;
        let disc = iso::Iso::open(std::path::Path::new(&iso)).ok()?;
        let e = disc.find("TUNARIA.ESF")?.clone();
        let mut w = esf::Window::new(iso::open_file(std::path::Path::new(&iso)).ok()?, e.lba as u64 * iso::SECTOR, e.size as u64);
        let def = esf::Container::open_deferred(&mut w).ok()?;
        let (off, len) = def.node(*def.zones().get(zone)?).deferred?;
        let mut b = vec![0u8; len];
        esf::ReadAt::read_at(&mut w, off, &mut b).ok()?;
        let c = esf::Container::parse_records(b);
        let n = *c.dict.get(&(id as i32))?;
        c.build_model(n)
    }

    #[test]
    fn the_catapult_splits_into_frame_arm_and_two_axles() {
        let Some(mut m) = zone_model(108, 0xE64B_8D78) else {
            eprintln!("no EQOA_ISO: skipped");
            return;
        };
        for b in &m.batches {
            eprintln!("batch material {} texture {:#010X} tris {}", b.material, b.texture() as u32, b.indices.len() / 3);
        }
        let r = rig(&mut m).expect("rigged");
        eprintln!("{r:?}");
        assert_eq!(r.wheels.len(), 2, "two axles");
        let a = r.arm.expect("an arm");
        // the parts' triangles: the frame, the arm (its beam's 48, the cup's
        // 34 and the iron bracket's 24, the owner, 2026-11-18: "the catapult
        // end part that looks like a spoon is not firing with the arm"), each
        // axle's 160
        assert_eq!(r.tris, vec![137, 106, 160, 160], "frame, arm with its cup and bracket, two axles");
        // the iron bracket (a piece of its own at the arm's end, behind the
        // rear axle) is the arm's: every corner there over the deck turns
        // with it, and no corner of the frame's far from the arm does
        let at = |v: usize| m.pos[v];
        let arm_corner = |v: usize| m.bones[v][0] == PART_ARM;
        let spoon: Vec<usize> = (0..m.pos.len()).filter(|&v| at(v)[2] < -4.85 && at(v)[2] > -5.35 && at(v)[1] > 1.85 && at(v)[0].abs() < 0.9 && !arm_corner(v)).collect();
        assert!(spoon.iter().all(|&v| at(v)[1] < 2.7), "a corner of the spoon's left behind: {:?}", spoon.iter().map(|&v| at(v)).collect::<Vec<_>>());
        // nothing wide (an upright, a wheel) on the arm but the cup, 1.64
        // across, past the deck's end at its top
        assert!((0..m.pos.len()).filter(|&v| arm_corner(v)).all(|v| at(v)[0].abs() < 1.0 || (at(v)[0].abs() < 1.7 && at(v)[2] < -5.7 && at(v)[1] > 2.05 && at(v)[1] < 2.6)), "nothing wide on the arm but its cup");
        assert!((0..m.pos.len()).filter(|&v| m.bones[v][0] >= PART_WHEEL0).all(|v| at(v)[1] < 3.1), "the wheels' alone on the axles");
        // the cup (the broad wooden plate past the deck's end, over its top
        // at 2.03) is wholly the arm's, and the deck under it wholly the
        // frame's
        let cup: Vec<usize> = (0..m.pos.len()).filter(|&v| at(v)[2] < -5.7 && at(v)[1] > 2.05 && at(v)[1] < 2.6).collect();
        assert!(cup.len() >= 12 && cup.iter().all(|&v| arm_corner(v)), "a corner of the cup's left behind: {:?}", cup.iter().filter(|&&v| !arm_corner(v)).map(|&v| at(v)).collect::<Vec<_>>());
        assert!((0..m.pos.len()).filter(|&v| at(v)[1] < 2.04 && at(v)[0].abs() > 1.1).all(|v| !arm_corner(v)), "the deck stays");
        // the boulder leaves from the cup's middle
        assert!(a.tip[2] < -6.7 && a.tip[2] > -7.2 && a.tip[1] < 2.7, "{:?}", a.tip);
        // the boulder leaves from above the pivot, over the frame's top
        let tip = r.release_tip().unwrap();
        assert!((tip[2] - a.pivot[2]).abs() < 1e-3 && tip[1] > a.pivot[1] + 5.0, "{tip:?} over {:?}", a.pivot);
        assert!(m.skinned && m.bones.len() == m.pos.len());
    }
}
