//! Feet on Slopes (Extended > Graphics > Characters, `feet_on_slopes` in
//! client.opt, ON by default (the owner, 2026-11-26: "make feet on slopes on by
//! default, because it is a bug"), `feet_on_slopes=off` the game's own; the owner's of 2026-11-26, "yes build feet on
//! slopes", after "when trolls and probably others run up hills they
//! somewhat clip knee deep into the hill").
//!
//! The game stands every body on ONE floor point, the ground straight under
//! its origin (`VIWalkControl::CalcFloor`'s 0.001 sphere; `ExtrapolateGround`
//! for the NPCs), and never tips a living biped to the slope
//! (`CalcPitchRollToLandscape` is called for a corpse only), so on a hill the
//! uphill foot goes into the ground by its reach times the slope: 0.3 m at 30
//! degrees for a troll's or a human's stride (`../claude-notes/
//! hill-clip-feet-on-slopes-20261126.md`). With the row on, a biped's legs
//! are fitted to the ground as drawn: the ground looked for under each ankle
//! and toe, the whole body lowered to the downhill foot's ground, and each
//! knee bent (two-bone IK, in the plane the animation bent it in) so its foot
//! comes up onto its own ground, the foot kept as the animation turned it.
//!
//! Drawing only: the node matrices the skin, the attachments and the effects
//! read are changed after `VIHSprite::Process`, nothing else. The walk, the
//! collision, the positions sent and anything the network sees are as they
//! were; off, nothing here runs and every node is the game's, bit for bit.
//! Not in the air, under or at the surface of a liquid, dead, or where the
//! ground under the body is not where it stands (a swimmer, a flier, a body
//! on a deck); and eased in and out over [`EASE_MS`] so nothing pops.
//!
//! Which bodies: [`legs_of`], two mirrored leg chains (hip, knee, ankle, toe)
//! read off the bind pose; anything else is drawn as the game draws it.

use glam::{Quat, Vec3};
use vi::csprite::CSprite;
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;

/// How long the fit takes to come in or go (ms).
pub const EASE_MS: f32 = 150.0;
/// Characters farther than this from the eye are left as the game draws them:
/// their feet are too small to see, and the probes are not spent on them.
pub const NEAR: f32 = 40.0;

/// A body's two legs: per leg (hip, knee, ankle, toe) node indices, the
/// pelvis they hang from, and the hips' bind height (model units).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Legs {
    pub chain: [[usize; 4]; 2],
    pub pelvis: usize,
    pub hip_y: f32,
}

fn bind_at(cs: &CSprite<Ee>, i: usize) -> Vec3 {
    let t = Matrix44::invert_affine(&cs.h.nodes[i].inv_bind_world).m[3];
    Vec3::new(t.x.0, t.y.0, t.z.0)
}

/// The legs of a biped, or None. Read off the bind pose (model space, the
/// floor at y = 0): a leg is a chain hip -> knee -> ankle -> toe ending in a
/// leaf (the toe), the toe on the floor (below 15% of the hip's height), the
/// ankle low, the knee between, thigh and shin each at least a quarter of the
/// hip's height and the chain near straight. A body has exactly two such
/// legs, hanging from one pelvis, mirrored across x (within 15% of the hip's
/// height) and of one length (within 10%), and nothing of it but the two
/// legs reaches the floor (a tail, a third leg). The game's own quadrupeds
/// (skeleton types 2 and 3, which `ExtrapolateGround` tips to a slope) are
/// never fitted.
pub fn legs_of(cs: &CSprite<Ee>) -> Option<Legs> {
    legs_why(cs).ok()
}

/// `legs_of`, or why not.
fn legs_why(cs: &CSprite<Ee>) -> Result<Legs, &'static str> {
    if cs.skel_type == 2 || cs.skel_type == 3 {
        return Err("a quadruped's skeleton type");
    }
    let n = &cs.h.nodes;
    let parent = |i: usize| -> Option<usize> { usize::try_from(n[i].parent).ok().filter(|&p| p < n.len()) };
    let mut legs: Vec<([usize; 4], usize, [Vec3; 4])> = Vec::new();
    for t in 0..n.len() {
        if n[t].child != -1 {
            continue;
        }
        // the leaf and its ancestors: up to two foot nodes between the toe and
        // the ankle (a ball of the foot)
        let mut anc = vec![t];
        while anc.len() < 8 {
            match parent(*anc.last().unwrap()) {
                Some(p) => anc.push(p),
                None => break,
            }
        }
        let found = (1..=3).filter(|&j| anc.len() > j + 3).find_map(|j| {
            let (a, k, h, p) = (anc[j], anc[j + 1], anc[j + 2], anc[j + 3]);
            let (tp, ap, kp, hp) = (bind_at(cs, t), bind_at(cs, a), bind_at(cs, k), bind_at(cs, h));
            let hy = hp.y;
            if !(hy > 0.05) {
                return None;
            }
            let (thigh, shin) = (hp.distance(kp), kp.distance(ap));
            let ok = tp.y < 0.15 * hy
                && anc[1..j].iter().all(|&f| bind_at(cs, f).y < 0.35 * hy)
                && ap.y < 0.35 * hy
                && kp.y > 0.25 * hy
                && kp.y < 0.8 * hy
                && hp.y > kp.y
                && kp.y > ap.y
                && thigh >= 0.25 * hy
                && shin >= 0.25 * hy
                && thigh + shin >= 0.8 * (hy - ap.y);
            ok.then_some(([h, k, a, t], p, [hp, kp, ap, tp]))
        });
        let Some(leg) = found else { continue };
        // a foot of two leaves (toe and heel) is one leg: its toe the forward one
        match legs.iter_mut().find(|l| l.0[2] == leg.0[2]) {
            Some(l) if leg.2[3].z > l.2[3].z => *l = leg,
            Some(_) => {}
            None => legs.push(leg),
        }
    }
    if legs.len() != 2 {
        return Err(if legs.len() < 2 { "fewer than two legs" } else { "more than two legs" });
    }
    let ((l, lp, lb), (r, rp, rb)) = (legs[0], legs[1]);
    let hy = (lb[0].y + rb[0].y) * 0.5;
    let mirrored = lp == rp
        && lb[0].x * rb[0].x < 0.0
        && (lb[0].x + rb[0].x).abs() < 0.15 * hy
        && (lb[0].y - rb[0].y).abs() < 0.05 * hy
        && (lb[2].x + rb[2].x).abs() < 0.15 * hy;
    let len = |b: &[Vec3; 4]| b[0].distance(b[1]) + b[1].distance(b[2]);
    if !mirrored || (len(&lb) - len(&rb)).abs() > 0.1 * len(&lb) {
        return Err("legs not a mirrored pair");
    }
    // nothing but the legs on the floor
    let floor = lb[3].y.max(rb[3].y) + 0.05 * hy;
    let mut feet = Vec::new();
    subtree(cs, l[2], &mut feet);
    subtree(cs, r[2], &mut feet);
    let in_legs = |i: usize| l.contains(&i) || r.contains(&i) || feet.contains(&i);
    if (0..n.len()).any(|i| !in_legs(i) && bind_at(cs, i).y < floor.max(0.2 * hy)) {
        return Err("something else on the floor");
    }
    Ok(Legs { chain: [l, r], pelvis: lp, hip_y: hy })
}

/// One body's fit, kept from frame to frame: its legs (read once a sprite),
/// how far the fit is in, and the pose it was last applied to.
#[derive(Clone, Debug, Default)]
pub struct Plant {
    key: Option<(usize, usize)>,
    legs: Option<Legs>,
    pub weight: f32,
    last_ms: Option<i64>,
    applied: Option<i64>,
    /// Per foot, the ground last found under it (world point and normal):
    /// the ground under the foot is read off that plane while the foot is
    /// within [`REUSE`] of it, and looked for again past it.
    planes: [Option<(Vec3, Vec3)>; 2],
    /// The last fit's heights, eased out from when it can no longer be found.
    last: [f32; 2],
}

/// How far (world units, across the ground) a foot goes before the ground
/// under it is looked for again: a probe is about 17 us natively, so a
/// standing body probes once and a runner about every third frame.
pub const REUSE: f32 = 0.4;

/// The ground under (x, z), looked for from `top` down to `bottom` (world
/// units): its height and its normal, or None.
pub type Ground<'a> = dyn FnMut(f32, f32, f32, f32) -> Option<(f32, [f32; 3])> + 'a;

/// The height of the plane through `p` with normal `n` at (x, z).
fn on_plane(p: Vec3, n: Vec3, x: f32, z: f32) -> f32 {
    p.y - (n.x * (x - p.x) + n.z * (z - p.z)) / n.y
}

fn get(m: &Matrix44<Ee>, i: usize) -> Vec3 {
    Vec3::new(m.m[i].x.0, m.m[i].y.0, m.m[i].z.0)
}

fn put(m: &mut Matrix44<Ee>, i: usize, v: Vec3) {
    m.m[i].x = F::new(v.x);
    m.m[i].y = F::new(v.y);
    m.m[i].z = F::new(v.z);
}

/// Node `i` and every node under it.
fn subtree(cs: &CSprite<Ee>, i: usize, out: &mut Vec<usize>) {
    out.push(i);
    let mut c = cs.h.nodes[i].child;
    while c >= 0 && (c as usize) < cs.h.nodes.len() {
        subtree(cs, c as usize, out);
        c = cs.h.nodes[c as usize].sibling;
    }
}

/// Every node in `nodes` turned by `q` about `c` (model space).
fn rotate(cs: &mut CSprite<Ee>, nodes: &[usize], q: Quat, c: Vec3) {
    for &i in nodes {
        let w = &mut cs.h.nodes[i].world;
        for r in 0..3 {
            let v = get(w, r);
            put(w, r, q * v);
        }
        let t = get(w, 3);
        put(w, 3, c + q * (t - c));
    }
}

fn at(cs: &CSprite<Ee>, i: usize) -> Vec3 {
    get(&cs.h.nodes[i].world, 3)
}

impl Plant {
    /// Fits the legs of `cs`, posed for this frame, to the ground under its
    /// model matrix `m` (the centre transform times the actor's), unless the
    /// row is off. `can`: the body is on a floor, out of liquid, alive, near.
    /// Once a pose: the skeleton is posed once a time (`Process`), and a body
    /// posed in its turn keeps last frame's nodes, already fitted.
    pub fn plant(&mut self, cs: &mut CSprite<Ee>, m: &Matrix44<Ee>, can: bool, now_ms: i64, ground: &mut Ground) {
        if !crate::settings::feet_on_slopes() {
            self.weight = 0.0;
            self.last_ms = None;
            return;
        }
        self.plant_on(cs, m, can, now_ms, ground);
    }

    /// `plant` with the row taken as on.
    pub fn plant_on(&mut self, cs: &mut CSprite<Ee>, m: &Matrix44<Ee>, can: bool, now_ms: i64, ground: &mut Ground) {
        let key = (cs.h.nodes.as_ptr() as usize, cs.h.nodes.len());
        if self.key != Some(key) {
            self.key = Some(key);
            self.legs = legs_of(cs);
            self.applied = None;
            self.planes = [None; 2];
        }
        let Some(legs) = self.legs else { return };
        if self.applied == Some(cs.h.pose_time) {
            return;
        }
        let dt = self.last_ms.map_or(EASE_MS, |l| (now_ms - l).clamp(0, 1000) as f32);
        self.last_ms = Some(now_ms);
        let fit = can.then(|| fit_of(cs, &legs, m, &mut self.planes, ground)).flatten();
        let step = dt / EASE_MS;
        self.weight = if fit.is_some() { (self.weight + step).min(1.0) } else { (self.weight - step).max(0.0) };
        if self.weight <= 0.0 {
            self.applied = Some(cs.h.pose_time);
            return;
        }
        // going: the last fit's heights eased out
        let dy = fit.unwrap_or(self.last);
        self.last = dy;
        apply(cs, &legs, [dy[0] * self.weight, dy[1] * self.weight]);
        self.applied = Some(cs.h.pose_time);
    }
}

/// Each foot's ground above (+) or below (-) the floor the body stands on,
/// in model units, or None where the body is not standing on the ground
/// found (tipped, swimming, flying, a probe that finds nothing). One probe
/// a foot, under its ankle, its plane carried to the toe and to the origin,
/// and kept while the foot stays within [`REUSE`] of it.
fn fit_of(cs: &CSprite<Ee>, legs: &Legs, m: &Matrix44<Ee>, planes: &mut [Option<(Vec3, Vec3)>; 2], ground: &mut Ground) -> Option<[f32; 2]> {
    let up = get(m, 1);
    let s = up.length();
    if !(s > 1e-4) || up.x.abs() > 0.02 * s || up.z.abs() > 0.02 * s {
        return None;
    }
    let to_world = |p: Vec3| -> Vec3 {
        let r = m.mul_point(&vi::vect::Vect3::of(p.x, p.y, p.z));
        Vec3::new(r.x.0, r.y.0, r.z.0)
    };
    let o = get(m, 3);
    let reach = legs.hip_y * s;
    let (top, bottom) = (o.y + reach + 0.5, o.y - reach - 1.0);
    let mut dy = [0.0f32; 2];
    for ((leg, d), plane) in legs.chain.iter().zip(dy.iter_mut()).zip(planes.iter_mut()) {
        let (ankle, toe) = (to_world(at(cs, leg[2])), to_world(at(cs, leg[3])));
        let kept = plane.filter(|(p, _)| {
            let (dx, dz) = (p.x - ankle.x, p.z - ankle.z);
            dx * dx + dz * dz < REUSE * REUSE && p.y < top && p.y > bottom
        });
        let (p, n) = match kept {
            Some(k) => k,
            None => {
                let (y, n) = ground(ankle.x, ankle.z, top, bottom)?;
                let n = Vec3::from(n);
                // ground steeper than a wall's limit is no floor to stand on
                if !(n.y > 0.3) {
                    *plane = None;
                    return None;
                }
                *plane = Some((Vec3::new(ankle.x, y, ankle.z), n));
                (Vec3::new(ankle.x, y, ankle.z), n)
            }
        };
        // the ground under the body is where it stands
        if (on_plane(p, n, o.x, o.z) - o.y).abs() > 0.15 * reach + 0.05 {
            return None;
        }
        let g = on_plane(p, n, ankle.x, ankle.z).max(on_plane(p, n, toe.x, toe.z));
        *d = ((g - o.y) / s).clamp(-0.5 * legs.hip_y, 0.5 * legs.hip_y);
    }
    Some(dy)
}

/// The fit: the whole body down by the lower foot's drop, then each leg's
/// knee bent until its ankle is up by the rest.
fn apply(cs: &mut CSprite<Ee>, legs: &Legs, dy: [f32; 2]) {
    let drop = dy[0].min(dy[1]).min(0.0).max(-0.4 * legs.hip_y);
    if drop != 0.0 {
        for n in &mut cs.h.nodes {
            n.world.m[3].y = F::new(n.world.m[3].y.0 + drop);
        }
    }
    let mut buf = Vec::new();
    for (leg, d) in legs.chain.iter().zip(dy) {
        let lift = d - drop;
        if lift > 1e-4 {
            lift_leg(cs, leg, lift, &mut buf);
        }
    }
}

/// Two-bone IK: the ankle of `leg` moved up by `lift` with the hip held, the
/// knee in the plane the pose bent it in, the foot (the ankle and what hangs
/// from it) carried as posed.
fn lift_leg(cs: &mut CSprite<Ee>, leg: &[usize; 4], lift: f32, buf: &mut Vec<usize>) {
    let [h, k, a, _] = *leg;
    let (hp, kp, ap) = (at(cs, h), at(cs, k), at(cs, a));
    let (l1, l2) = (hp.distance(kp), kp.distance(ap));
    if l1 < 1e-4 || l2 < 1e-4 {
        return;
    }
    let target = ap + Vec3::Y * lift;
    let to = target - hp;
    let d = to.length().clamp((l1 - l2).abs() + 1e-3, l1 + l2 - 1e-4);
    let dir = to.normalize_or(Vec3::NEG_Y);
    let target = hp + dir * d;
    // the bend's side: the knee off the hip-ankle line, as posed
    let mut pole = (kp - hp) - dir * (kp - hp).dot(dir);
    if pole.length_squared() < 1e-8 {
        pole = Vec3::Z - dir * Vec3::Z.dot(dir);
    }
    let pole = pole.normalize_or(Vec3::Z);
    let along = (l1 * l1 - l2 * l2 + d * d) / (2.0 * d);
    let knee = hp + dir * along + pole * (l1 * l1 - along * along).max(0.0).sqrt();
    // the foot as posed, carried
    buf.clear();
    subtree(cs, a, buf);
    let foot: Vec<(usize, Matrix44<Ee>)> = buf.iter().map(|&i| (i, cs.h.nodes[i].world)).collect();
    // the thigh to the new knee
    let q1 = Quat::from_rotation_arc((kp - hp).normalize(), (knee - hp).normalize());
    buf.clear();
    subtree(cs, h, buf);
    rotate(cs, buf, q1, hp);
    // the shin to the target
    let a1 = at(cs, a);
    let q2 = Quat::from_rotation_arc((a1 - knee).normalize_or(Vec3::NEG_Y), (target - knee).normalize_or(Vec3::NEG_Y));
    buf.clear();
    subtree(cs, k, buf);
    rotate(cs, buf, q2, knee);
    let moved = target - ap;
    for (i, mut w) in foot {
        let t = get(&w, 3) + moved;
        put(&mut w, 3, t);
        cs.h.nodes[i].world = w;
    }
}

/// The ground for `plant` in a world: a ray (CalcFloor's 0.001 sphere)
/// that passes through every character, as the walk's floor query does.
pub fn world_ground(w: &mut vi::world::World<Ee>) -> impl FnMut(f32, f32, f32, f32) -> Option<(f32, [f32; 3])> + '_ {
    use vi::coll::CollSphere;
    use vi::scene::{Filter, FilterKind};
    move |x, z, top, bottom| {
        let mut s = CollSphere::<Ee>::default();
        s.init(vi::vect::Vect3::of(x, top, z), vi::vect::Vect3::of(x, bottom, z), F::new(0.001));
        let mut f = Filter::new(FilterKind::WalkFloor, -1);
        w.collide_sphere(&mut s, &mut f).then(|| {
            let n = s.ray.hit_normal;
            (s.hit_point().y.0, [n.x.0, n.y.0, n.z.0])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vi::character::{AnimCache, load_csprite};
    use vi::csprite::Rand;
    use vi::hsprite::AnimScene;
    use vi::vect::Vect3;

    fn char_esf() -> Option<esf::Library> {
        let iso_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&iso_path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        };
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", esf::Container::open(disc.read("CHAR.ESF").unwrap()).unwrap());
        Some(lib)
    }

    /// A plane rising `tan(a)` per unit along +z through the origin.
    fn slope(a: f32) -> impl FnMut(f32, f32, f32, f32) -> Option<(f32, [f32; 3])> {
        let k = a.to_radians().tan();
        let n = Vec3::new(0.0, 1.0, -k).normalize();
        move |_x, z, _t, _b| Some((z * k, n.to_array()))
    }

    fn world_of(m: &Matrix44<Ee>, p: Vec3) -> Vec3 {
        let r = m.mul_point(&Vect3::of(p.x, p.y, p.z));
        Vec3::new(r.x.0, r.y.0, r.z.0)
    }

    /// Every node's matrix, bit for bit.
    fn bits(cs: &CSprite<Ee>) -> Vec<u32> {
        cs.h.nodes.iter().flat_map(|n| n.world.to_cols().into_iter().flatten().map(f32::to_bits).collect::<Vec<_>>()).collect()
    }

    const TROLL: u32 = 0x4C6F_A532;
    const HUMAN: u32 = 0x70D8_98C6;

    /// A troll running up a 30 degree hill (heading so it faces up it), over
    /// two seconds of its run: with the fit, no ankle or toe is ever under
    /// the ground and the planted foot is on it (within 3 cm); the control,
    /// the same frames unfitted, sinks one by more than 20 cm. Off, every
    /// node is bit for bit the game's.
    #[test]
    fn a_troll_running_up_a_hill_keeps_its_feet_on_it() {
        let Some(lib) = char_esf() else { return };
        for id in [TROLL, HUMAN] {
            for loco in [1, 3, 0] {
                let r = lib.lookup(id as i32).unwrap();
                let mut anims = AnimScene::<Ee>::default();
                let mut rand = Rand(1);
                let mut cs = load_csprite(&lib, r, &mut anims, &mut AnimCache::new(), &mut rand).unwrap();
                let legs = legs_of(&cs).expect("a biped's legs");
                let mut rand2 = Rand(1);
                let mut control = load_csprite(&lib, r, &mut anims, &mut AnimCache::new(), &mut rand2).unwrap();
                cs.set_locomotion(loco, &anims, &mut rand);
                control.set_locomotion(loco, &anims, &mut rand2);
                let actor = Matrix44::<Ee>::identity();
                let mut plant = Plant::default();
                let (mut worst_fit, mut worst_game, mut planted_err) = (f32::MIN, f32::MIN, 0.0f32);
                for t in (0..2500i64).step_by(33) {
                    anims.time = t;
                    cs.h.process(0, &anims);
                    control.h.process(0, &anims);
                    let m = cs.h.calc_trans_about_center(&actor, &anims);
                    let mc = control.h.calc_trans_about_center(&actor, &anims);
                    plant.plant_on(&mut cs, &m, true, t, &mut slope(30.0));
                    // a second call on the same pose changes nothing
                    let once = bits(&cs);
                    plant.plant_on(&mut cs, &m, true, t, &mut slope(30.0));
                    assert!(bits(&cs) == once, "applied twice at {t}");
                    if t < 300 {
                        continue; // eased in
                    }
                    let k = 30f32.to_radians().tan();
                    for leg in &legs.chain {
                        // how far the ankle and the toe are under their ground, beside
                        // how high the game's pose holds each over its flat floor
                        let mut over = f32::MAX;
                        for &i in &leg[2..4] {
                            let base = control.h.nodes[i].world.m[3].y.0;
                            let p = world_of(&m, at(&cs, i));
                            let pc = world_of(&mc, at(&control, i));
                            let under = p.z * k + base * m.m[1].y.0 - p.y;
                            let under_c = pc.z * k + base * mc.m[1].y.0 - pc.y;
                            worst_fit = worst_fit.max(under);
                            worst_game = worst_game.max(under_c);
                            over = over.min(-under);
                        }
                        // and the nearer of them not floating over it
                        planted_err = planted_err.max(over);
                    }
                }
                eprintln!("{id:08X} loco {loco}: worst under ground, fitted {worst_fit:.3}, game {worst_game:.3}, nearer of ankle/toe over its ground at most {planted_err:.3}");
                assert!(worst_fit < 0.03, "{id:08X} loco {loco}: a foot {worst_fit} under the hill");
                assert!(planted_err < 0.15, "{id:08X} loco {loco}: a foot {planted_err} over the hill");
                if loco != 0 {
                    assert!(worst_game > 0.2, "{id:08X} loco {loco}: the control sinks a foot ({worst_game})");
                }
            }
        }
    }

    /// Off (the row's own path): nothing is touched, bit for bit.
    #[test]
    fn off_leaves_every_node_as_posed() {
        let Some(lib) = char_esf() else { return };
        let r = lib.lookup(TROLL as i32).unwrap();
        let mut anims = AnimScene::<Ee>::default();
        let mut rand = Rand(1);
        let mut cs = load_csprite(&lib, r, &mut anims, &mut AnimCache::new(), &mut rand).unwrap();
        cs.set_locomotion(3, &anims, &mut rand);
        let mut plant = Plant::default();
        let actor = Matrix44::<Ee>::identity();
        crate::settings::testing::reset(false);
        assert!(crate::settings::feet_on_slopes(), "on by default");
        crate::settings::set_client(crate::settings::ClientOptions { feet_on_slopes: false, ..crate::settings::client() });
        for t in (0..1000i64).step_by(33) {
            anims.time = t;
            cs.h.process(0, &anims);
            let before = bits(&cs);
            let m = cs.h.calc_trans_about_center(&actor, &anims);
            plant.plant(&mut cs, &m, true, t, &mut slope(30.0));
            assert!(bits(&cs) == before, "touched at {t}");
        }
        // the control for the path off: the same row on fits the same frames
        crate::settings::set_client(crate::settings::ClientOptions { feet_on_slopes: true, ..crate::settings::client() });
        let mut moved = false;
        for t in (1000..1500i64).step_by(33) {
            anims.time = t;
            cs.h.process(0, &anims);
            let before = bits(&cs);
            let m = cs.h.calc_trans_about_center(&actor, &anims);
            plant.plant(&mut cs, &m, true, t, &mut slope(30.0));
            moved |= bits(&cs) != before;
        }
        assert!(moved, "on, the slope moves the legs");
        crate::settings::testing::reset(false);
        // and on the flat with the row on, the fit is the identity too
        let mut plant = Plant::default();
        for t in (1000..2000i64).step_by(33) {
            anims.time = t;
            cs.h.process(0, &anims);
            let before = bits(&cs);
            let m = cs.h.calc_trans_about_center(&actor, &anims);
            plant.plant_on(&mut cs, &m, true, t, &mut |_, _, _, _| Some((0.0, [0.0, 1.0, 0.0])));
            assert!(bits(&cs) == before, "the flat moved a node at {t}");
        }
    }

    /// In the air, swimming or dead (`can` false), or over nothing found:
    /// eased out, then untouched.
    #[test]
    fn not_fitted_where_it_cannot_stand() {
        let Some(lib) = char_esf() else { return };
        let r = lib.lookup(TROLL as i32).unwrap();
        let mut anims = AnimScene::<Ee>::default();
        let mut rand = Rand(1);
        let mut cs = load_csprite(&lib, r, &mut anims, &mut AnimCache::new(), &mut rand).unwrap();
        cs.set_locomotion(1, &anims, &mut rand);
        let actor = Matrix44::<Ee>::identity();
        for case in 0..2 {
            let mut plant = Plant::default();
            for t in (0..400i64).step_by(33) {
                anims.time = t;
                cs.h.process(0, &anims);
                let before = bits(&cs);
                let m = cs.h.calc_trans_about_center(&actor, &anims);
                match case {
                    0 => plant.plant_on(&mut cs, &m, false, t, &mut slope(30.0)),
                    // the ground under it 2 m below: swimming, flying, a deck
                    _ => plant.plant_on(&mut cs, &m, true, t, &mut |_, _, _, _| Some((-2.0, [0.0, 1.0, 0.0]))),
                }
                assert!(bits(&cs) == before, "case {case} at {t}");
            }
        }
        // fitted, then leaving the ground: eased out over EASE_MS, not dropped
        let mut plant = Plant::default();
        let mut moved = Vec::new();
        for t in (400..1400i64).step_by(33) {
            anims.time = t;
            cs.h.process(0, &anims);
            let before = bits(&cs);
            let hip = at(&cs, 0).y;
            let m = cs.h.calc_trans_about_center(&actor, &anims);
            plant.plant_on(&mut cs, &m, t < 900, t, &mut slope(30.0));
            moved.push((t, bits(&cs) != before, (at(&cs, 0).y - hip).abs()));
        }
        let out: Vec<_> = moved.iter().filter(|m| m.0 >= 900).collect();
        assert!(out[0].1 && out[1].1, "eased, not dropped: {out:?}");
        assert!(out[1].2 < out[0].2 || out[0].2 == 0.0, "going: {out:?}");
        assert!(out.iter().filter(|m| m.0 >= 900 + EASE_MS as i64 + 33).all(|m| !m.1), "gone: {out:?}");
    }

    /// Which of CHAR.ESF's bodies are fitted (`--ignored --nocapture`).
    #[test]
    #[ignore]
    fn survey_legs() {
        let Some(lib) = char_esf() else { return };
        let c = lib.file(0);
        let (mut yes, mut no) = (0, 0);
        for i in 0..c.nodes.len() {
            if c.nodes[i].ty != 0x2700 {
                continue;
            }
            let mut anims = AnimScene::<Ee>::default();
            let Some(cs) = load_csprite(&lib, esf::Ref { file: 0, node: i }, &mut anims, &mut AnimCache::new(), &mut Rand(1)) else { continue };
            let why = legs_why(&cs);
            let legs = why.ok();
            let top = (0..cs.h.nodes.len()).map(|j| bind_at(&cs, j).y).fold(0.0f32, f32::max);
                        let hdr = c.child(i, 0x2710).map_or(0, |h| { let d = c.body(h); u32::from_le_bytes([d[0], d[1], d[2], d[3]]) });
            println!("{} {hdr:08X} node {i} skel {} nodes {} top {top:.2}: {}", if legs.is_some() { "YES" } else { "no " }, cs.skel_type, cs.h.nodes.len(), legs.map_or(why.err().unwrap_or("").to_string(), |l| format!("{:?} hip {:.2}", l.chain, l.hip_y)));
            if legs.is_some() {
                yes += 1;
            } else {
                no += 1;
            }
        }
        println!("{yes} fitted, {no} not");
    }

    /// A standing body looks for the ground under its feet once; a walking
    /// one far less than twice a frame (`REUSE`).
    #[test]
    fn the_ground_found_is_kept_while_a_foot_stays() {
        let Some(lib) = char_esf() else { return };
        for (loco, most) in [(0, 2), (1, 40)] {
            let r = lib.lookup(TROLL as i32).unwrap();
            let mut anims = AnimScene::<Ee>::default();
            let mut rand = Rand(1);
            let mut cs = load_csprite(&lib, r, &mut anims, &mut AnimCache::new(), &mut rand).unwrap();
            cs.set_locomotion(loco, &anims, &mut rand);
            let actor = Matrix44::<Ee>::identity();
            let mut plant = Plant::default();
            let mut probes = 0;
            let mut inner = slope(30.0);
            let mut counted = |x: f32, z: f32, t: f32, b: f32| {
                probes += 1;
                inner(x, z, t, b)
            };
            for f in 0..60i64 {
                anims.time = f * 33;
                cs.h.process(0, &anims);
                let m = cs.h.calc_trans_about_center(&actor, &anims);
                plant.plant_on(&mut cs, &m, true, f * 33, &mut counted);
            }
            assert!(probes <= most, "loco {loco}: {probes} probes in 60 frames");
            assert!(probes >= 2, "loco {loco}: looked for the ground");
        }
    }

    /// The fit's whole cost in Tunaria (probes in the real collision world):
    /// a troll running up the hill at 24746, 15042, 60 frames a second
    /// (`--ignored --nocapture`, `--release`).
    #[test]
    #[ignore]
    fn cost_in_tunaria() {
        let iso_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&iso_path) else { return };
        let mut lib = esf::Library::new();
        let f = lib.add("TUNARIA.ESF", esf::Container::open(disc.read("TUNARIA.ESF").unwrap()).unwrap());
        lib.add("CHAR.ESF", esf::Container::open(disc.read("CHAR.ESF").unwrap()).unwrap());
        let mut w = vi::world::World::<Ee>::load(&lib, f).unwrap();
        let r = lib.lookup(TROLL as i32).unwrap();
        let mut anims = AnimScene::<Ee>::default();
        let mut rand = Rand(1);
        let mut cs = load_csprite(&lib, r, &mut anims, &mut AnimCache::new(), &mut rand).unwrap();
        cs.set_locomotion(3, &anims, &mut rand);
        let heading = 0.889f32;
        let (mut x, mut z) = (24746.0f32, 15042.0f32);
        let mut plant = Plant::default();
        let (mut fit_t, mut pose_t, mut probes) = (std::time::Duration::ZERO, std::time::Duration::ZERO, 0);
        let frames = 600;
        for i in 0..frames {
            let t = i * 1000 / 60;
            x += heading.sin() * 8.83 / 60.0;
            z += heading.cos() * 8.83 / 60.0;
            let y = crate::walker::ground(&mut w, x, z).unwrap();
            let actor = Matrix44::transform(&Vect3::of(heading, 0.0, 0.0), F::new(1.44), &Vect3::of(x, y, z));
            anims.time = t;
            let s0 = std::time::Instant::now();
            cs.h.process(0, &anims);
            let m = cs.h.calc_trans_about_center(&actor, &anims);
            let s1 = std::time::Instant::now();
            let mut g = world_ground(&mut w);
            let mut counted = |a: f32, b: f32, c: f32, d: f32| {
                probes += 1;
                g(a, b, c, d)
            };
            plant.plant_on(&mut cs, &m, true, t, &mut counted);
            fit_t += s1.elapsed();
            pose_t += s1 - s0;
        }
        println!("Tunaria, a troll running uphill: the fit {:?} a frame ({probes} probes in {frames} frames, weight {}), the pose itself {:?}", fit_t / frames as u32, plant.weight, pose_t / frames as u32);
    }

    /// The fit's cost: a posed troll fitted on a slope, a frame.
    #[test]
    #[ignore]
    fn cost() {
        let Some(lib) = char_esf() else { return };
        let r = lib.lookup(TROLL as i32).unwrap();
        let mut anims = AnimScene::<Ee>::default();
        let mut rand = Rand(1);
        let mut cs = load_csprite(&lib, r, &mut anims, &mut AnimCache::new(), &mut rand).unwrap();
        cs.set_locomotion(3, &anims, &mut rand);
        let actor = Matrix44::<Ee>::identity();
        let mut plant = Plant::default();
        let n = 3000;
        let start = std::time::Instant::now();
        for t in 0..n {
            anims.time = t * 33;
            cs.h.process(0, &anims);
            let m = cs.h.calc_trans_about_center(&actor, &anims);
            plant.plant_on(&mut cs, &m, true, t * 33, &mut slope(30.0));
        }
        let with = start.elapsed();
        let start = std::time::Instant::now();
        for t in 0..n {
            anims.time = t * 33 + 1;
            cs.h.process(0, &anims);
            let _ = cs.h.calc_trans_about_center(&actor, &anims);
        }
        let without = start.elapsed();
        println!("pose+fit {:?}/frame, pose alone {:?}/frame (probes not counted)", with / n as u32, without / n as u32);
    }
}
