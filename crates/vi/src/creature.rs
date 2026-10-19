//! Moving a creature between its updates (decomp
//! `modules/client/src/viaiextrapolate.cpp`): `ExtrapolateMove` carries the
//! position along the velocity and drops it onto the ground;
//! `ExtrapolateGround` probes 1.5 m above and below for the ground and tips
//! the creature to the slope (pitch for skeleton type 2, pitch and roll for
//! 3), easing a corpse onto it over its first second dead. The probe's
//! filter (`VIAIExtrapolateFilter`) skips the creature's own actor and every
//! character sprite, which is `FilterKind::WalkCamera`'s rule.

use crate::coll::CollSphere;
use crate::fpu::{F, Model, f, vimax, vimin};
use crate::scene::{Filter, FilterKind, Scene};
use crate::vect::Vect3;

/// The tilt `ExtrapolateGround` takes from the hit normal `n` for heading
/// `h`: (pitch, roll).
fn slope<M: Model>(n: &Vect3<M>, h: F<M>) -> (F<M>, F<M>) {
    let mut fwd = Vect3::<M>::default();
    fwd.hpr(h, F::ZERO, F::ZERO);
    let mut r = Vect3::new(n.y * fwd.z - n.z * fwd.y, -n.x * fwd.z + n.z * fwd.x, n.x * fwd.y - n.y * fwd.x);
    r.normalize();
    let fwd = Vect3::new(r.y * n.z - r.z * n.y, -r.x * n.z + r.z * n.x, r.x * n.y - r.y * n.x);
    let pitch = -fwd.y.atan2f((f::<M>(1.0) - fwd.y * fwd.y).sqrt());
    let roll = r.y.atan2f((f::<M>(1.0) - r.y * r.y).sqrt());
    (pitch, roll)
}

/// `ExtrapolateGround`. `skel_type` is the CSprite's (0 when the actor is
/// not one); `dead_ms` is the time since death when its sprite is dead.
pub fn extrapolate_ground<M: Model, S: Scene<M>>(scene: &mut S, actor: i32, skel_type: i32, dead_ms: Option<i64>, pos: &mut Vect3<M>, hpr: &mut Vect3<M>) {
    let mut a = *pos;
    a.y += f(1.5);
    let mut b = *pos;
    b.y -= f(1.5);
    let mut s = CollSphere::<M>::default();
    s.init(a, b, f(0.1));
    let mut filter = Filter::new(FilterKind::WalkCamera, actor);
    if scene.collide_sphere(&mut s, &mut filter) {
        let hit = s.hit_point();
        pos.x = hit.x;
        pos.y = hit.y - f(0.1);
        pos.z = hit.z;
    }
    if filter.hit_actor != -1 {
        hpr.y = F::ZERO;
        hpr.z = F::ZERO;
        return;
    }
    let n = s.ray.hit_normal;
    match skel_type {
        2 => {
            hpr.y = slope(&n, hpr.x).0;
            hpr.z = F::ZERO;
        }
        3 => {
            let (p, r) = slope(&n, hpr.x);
            hpr.y = p;
            hpr.z = r;
        }
        _ => {
            hpr.y = F::ZERO;
            hpr.z = F::ZERO;
        }
    }
    if let Some(ms) = dead_ms {
        let (tp, tr) = slope(&n, hpr.x);
        let k = vimin(f(1.0), vimax(F::ZERO, F::new(ms as i32 as f32) * f(0.001)));
        hpr.y = hpr.y + (tp - hpr.y) * k;
        hpr.z = hpr.z + (tr - hpr.z) * k;
    }
}

/// `ExtrapolateMove`: `dt` seconds along `vel`, then onto the ground.
#[allow(clippy::too_many_arguments)]
pub fn extrapolate_move<M: Model, S: Scene<M>>(
    scene: &mut S,
    actor: i32,
    skel_type: i32,
    dead_ms: Option<i64>,
    vel: &Vect3<M>,
    dt: F<M>,
    pos: &mut Vect3<M>,
    hpr: &mut Vect3<M>,
) {
    pos.x += vel.x * dt;
    pos.y += vel.y * dt;
    pos.z += vel.z * dt;
    extrapolate_ground(scene, actor, skel_type, dead_ms, pos, hpr);
}
