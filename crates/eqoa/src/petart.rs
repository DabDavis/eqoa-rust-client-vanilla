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

impl State {
    /// A cast of the prop in `slot` (the pet `art`, at `pos`, `length` long
    /// and `top` high as drawn) at `target` (at `to`, its chest, if seen), of
    /// `time` ms, at `now`: a swing's rams at its round's hits, or a shot.
    #[allow(clippy::too_many_arguments)]
    pub fn cast(&mut self, slot: usize, art: &net::decor::PetArt, pos: [f32; 3], length: f32, top: f32, target: u32, to: Option<[f32; 3]>, time: i32, now: i64) {
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
        lunges.push(Lunge { at: now, target, reach: RECOIL.0 * length, back: true });
        let from = [pos[0], pos[1] + top * 0.95, pos[2]];
        let d = ((to[0] - from[0]).powi(2) + (to[2] - from[2]).powi(2)).sqrt();
        let scale = if art.shot_scale > 0.0 { art.shot_scale } else { 1.0 };
        self.shots.push(Shot { model: art.shot, scale, from, target, to, start: now, flight: flight_ms(d) });
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
    }

    /// The shots in flight now, each its model and transform (a tumble about
    /// its flight), the ended dropped; a shot's target followed where `place`
    /// finds it.
    pub fn flying(&mut self, now: i64, place: impl Fn(u32) -> Option<[f32; 3]>) -> Vec<(u32, Matrix44<Ee>)> {
        self.shots.retain(|s| now - s.start < s.flight);
        let f = F::<Ee>::new;
        let mut out = Vec::new();
        for s in &mut self.shots {
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
            let m = lib.lookup(id as i32).and_then(|r| lib.build_model(r))?;
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
        s.cast(3, &catapult(), [0.0; 3], 5.0, 2.0, 77, Some([10.0, 1.0, 0.0]), 6000, 1000);
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
        s.cast(2, &catapult(), [0.0, 0.0, 0.0], 5.0, 2.0, 9, Some([8.0, 1.0, 6.0]), 0, 500);
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
        s.cast(1, &a, [0.0; 3], 5.0, 2.0, 9, None, 0, 0);
        a.shot = 0;
        s.cast(1, &a, [0.0; 3], 5.0, 2.0, 9, Some([1.0, 0.0, 0.0]), 0, 0);
        assert!(s.shots.is_empty());
    }

    #[test]
    fn bounds_are_the_longer_span_and_the_top() {
        assert_eq!(bounds(&[[-4.0, 0.0, -8.0], [4.0, 6.5, 9.0]]), Some((17.0, 6.5)));
        assert_eq!(bounds(&[]), None);
    }
}
