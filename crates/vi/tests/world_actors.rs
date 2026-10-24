//! The actors made in play (`CreateActor`, `SetActorTransform`,
//! `SetActorCollideable`, `ReleaseActor`): a creature collides as its
//! CSprite's capsule (`VICSprite::Collide`: 0.5 round, from its feet 2 up),
//! which a walk's sweep stops at and its floor and camera probes pass through,
//! as nothing while its copy is dead or it is not collideable, and never with
//! a ray; and it is in exactly the rooms it reaches as it moves, which the
//! world walk finds as brute force does. On open ground in Freeport, with the
//! disc's signpost (an NPC: CHAR.ESF 0x6A82352C) and human (0x70D898C6).

use vi::coll::{CollRay, CollSphere};
use vi::fpu::{F, Ieee};
use vi::scene::{Filter, FilterKind};
use vi::vect::Vect3;
use vi::world::World;

const SIGNPOST: i32 = 0x6A82352C;
const HUMAN: i32 = 0x70D898C6;
/// Open ground in Freeport (the walker's tests' "open ground").
const AT: (f32, f32) = (25275.115, 15766.627);

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * ((self.next() >> 40) as f32 / (1u64 << 24) as f32)
    }
}

/// TUNARIA's collision world and a library with CHAR.ESF in it; None
/// without the disc.
fn freeport() -> Option<(World<Ieee>, esf::Library)> {
    let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !iso_path.exists() {
        eprintln!("SKIP: no disc");
        return None;
    }
    let mut iso = iso::Iso::open(iso_path).unwrap();
    let mut lib = esf::Library::new();
    let f = lib.add("TUNARIA.ESF", esf::Container::open(iso.read("TUNARIA.ESF").unwrap()).unwrap());
    lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
    let w = World::<Ieee>::load(&lib, f).unwrap();
    Some((w, lib))
}

fn v(x: f32, y: f32, z: f32) -> Vect3<Ieee> {
    Vect3::of(x, y, z)
}

/// A sweep of radius `r` from `a` to `b` under the filter: (hit time, the
/// actor hit), None for no hit.
fn sweep(w: &mut World<Ieee>, a: Vect3<Ieee>, b: Vect3<Ieee>, r: f32, kind: FilterKind, actor: i32) -> Option<(f32, i32)> {
    let mut s = CollSphere::<Ieee>::default();
    s.init(a, b, F::new(r));
    let mut f = Filter::new(kind, actor);
    w.collide_sphere(&mut s, &mut f).then_some((s.ray.hit_time.0, f.hit_actor))
}

/// The ground at (x, z): a floor probe from high above.
fn ground(w: &mut World<Ieee>, x: f32, z: f32) -> f32 {
    let mut s = CollSphere::<Ieee>::default();
    s.init(v(x, 500.0, z), v(x, -200.0, z), F::new(0.001));
    let mut f = Filter::new(FilterKind::WalkFloor, -1);
    assert!(w.collide_sphere(&mut s, &mut f), "ground at ({x}, {z})");
    s.hit_point().y.0
}

/// The rooms an actor is in, as (zone, room).
fn rooms_of(w: &World<Ieee>, a: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (z, zone) in w.zones.iter().enumerate() {
        for (r, room) in zone.rooms.iter().enumerate() {
            let n = room.moving.iter().filter(|&&m| m == a).count();
            assert!(n <= 1, "actor {a} {n} times in room ({z}, {r})");
            if n == 1 {
                out.push((z, r));
            }
        }
    }
    out
}

/// A creature in the way stops a sweep at its capsule grown by the sweep's
/// radius (0.5 + the walk's 0.6), whatever else it is; the controls are the
/// same sweep with no creature, beside it, over it, with the filters that
/// pass characters by, as a ray, dead, not collideable, moved, and released.
#[test]
fn a_creature_stops_a_walk_as_its_capsule() {
    let Some((mut w, lib)) = freeport() else { return };
    let (x, z) = AT;
    let y = ground(&mut w, x, z);
    let body = 1.2; // the walk's sphere: 1.2 over its feet, 0.6 round
    let (from, to) = (v(x - 4.0, y + body, z), v(x + 4.0, y + body, z));
    assert_eq!(sweep(&mut w, from, to, 0.6, FilterKind::Scene, -1), None, "the control: open ground");

    let sprite = w.find_sprite(&lib, lib.lookup(SIGNPOST).unwrap()).unwrap();
    let post = w.create_actor(v(x, y, z), v(0.0, 0.0, 0.0), F::new(1.0), sprite);
    let (t, hit) = sweep(&mut w, from, to, 0.6, FilterKind::Scene, -1).expect("the post stops the sweep");
    assert_eq!(hit, post as i32);
    // 4 from the axis to the grown capsule's 1.1: 2.9 of the 8
    assert!((t - 2.9 / 8.0).abs() < 1e-4, "hit at {t}");

    assert_eq!(sweep(&mut w, from, to, 0.6, FilterKind::Scene, post as i32), None, "a walk passes over its own actor");
    assert_eq!(sweep(&mut w, from, to, 0.6, FilterKind::WalkFloor, -1), None, "the floor probe passes through characters");
    assert_eq!(sweep(&mut w, from, to, 0.6, FilterKind::WalkCamera, -1), None, "the camera passes through characters");
    let mut r = CollRay::<Ieee>::default();
    r.init(from, to);
    assert!(!w.collide_ray(&mut r, &mut Filter::new(FilterKind::Scene, -1)), "a ray never meets a character");

    // the capsule's width and height
    let beside = |w: &mut World<Ieee>, dz: f32, dy: f32| sweep(w, v(x - 4.0, y + dy, z + dz), v(x + 4.0, y + dy, z + dz), 0.6, FilterKind::Scene, -1);
    assert!(beside(&mut w, 1.05, body).is_some());
    assert_eq!(beside(&mut w, 1.15, body), None);
    assert!(beside(&mut w, 0.0, 3.05).is_some(), "the top cap, 2 up, grown to 1.1");
    assert_eq!(beside(&mut w, 0.0, 3.15), None);

    w.set_actor_dead(post, true);
    assert_eq!(sweep(&mut w, from, to, 0.6, FilterKind::Scene, -1), None, "a corpse");
    w.set_actor_dead(post, false);
    assert!(sweep(&mut w, from, to, 0.6, FilterKind::Scene, -1).is_some());
    w.set_actor_collideable(post, false);
    assert_eq!(sweep(&mut w, from, to, 0.6, FilterKind::Scene, -1), None, "not collideable (the ignore list)");
    w.set_actor_collideable(post, true);
    assert!(sweep(&mut w, from, to, 0.6, FilterKind::Scene, -1).is_some());

    // twice the size, twice as round: 1.0 + 0.6
    w.set_actor_transform(post, v(0.0, 0.0, 0.0), F::new(2.0), v(x, y, z));
    assert!(beside(&mut w, 1.55, body).is_some(), "scale 2");
    w.set_actor_transform(post, v(0.0, 0.0, 0.0), F::new(1.0), v(x, y, z));
    assert_eq!(beside(&mut w, 1.55, body), None, "scale 1 again");

    // moved to open ground nearby (its sweep clear before): where it was is
    // open again, where it is it stops the sweep
    let (x2, z2) = [(8.0, 0.0), (-8.0, 0.0), (0.0, -8.0), (0.0, 8.0), (12.0, 12.0), (-12.0, -12.0)]
        .into_iter()
        .map(|(dx, dz)| (x + dx, z + dz))
        .find(|&(x2, z2)| {
            let y2 = ground(&mut w, x2, z2);
            sweep(&mut w, v(x2 - 4.0, y2 + body, z2), v(x2 + 4.0, y2 + body, z2), 0.6, FilterKind::Scene, -1).is_none()
        })
        .expect("open ground near");
    let y2 = ground(&mut w, x2, z2);
    let (from2, to2) = (v(x2 - 4.0, y2 + body, z2), v(x2 + 4.0, y2 + body, z2));
    w.set_actor_transform(post, v(0.0, 0.0, 0.0), F::new(1.0), v(x2, y2, z2));
    assert_eq!(sweep(&mut w, from, to, 0.6, FilterKind::Scene, -1), None, "where it was");
    assert_eq!(sweep(&mut w, from2, to2, 0.6, FilterKind::Scene, -1).map(|h| h.1), Some(post as i32));

    w.release_actor(post);
    assert_eq!(sweep(&mut w, from2, to2, 0.6, FilterKind::Scene, -1), None, "released");
    assert!(rooms_of(&w, post).is_empty());
    let again = w.create_actor(v(x, y, z), v(0.0, 0.0, 0.0), F::new(1.0), sprite);
    assert_eq!(again, post, "its place taken by the next");
    assert_eq!(sweep(&mut w, from, to, 0.6, FilterKind::Scene, -1).map(|h| h.1), Some(again as i32));
}

/// A creature walking about Freeport is in exactly the rooms a fresh actor
/// where it now is would be in (it leaves those it left), and the walk over
/// the world and zone trees meets it, and everything else, as brute force
/// over every room does. The control: the walk crosses rooms (its rooms
/// change) and the sweeps do meet the creatures.
#[test]
fn creatures_are_in_the_rooms_they_reach_as_they_move() {
    let Some((mut w, lib)) = freeport() else { return };
    let zones: Vec<usize> = (0..w.zones.len())
        .filter(|&z| w.zones[z].pretrans.iter().any(|p| (p.x.0 - AT.0).abs() < 2500.0 && (p.z.0 - AT.1).abs() < 2500.0))
        .collect();
    let human = w.find_sprite(&lib, lib.lookup(HUMAN).unwrap()).unwrap();
    let post = w.find_sprite(&lib, lib.lookup(SIGNPOST).unwrap()).unwrap();
    let mut r = Rng(0xC0FF_EE11);
    // three creatures, one walking 600 m across Freeport, two standing
    let (x0, z0) = AT;
    let y0 = ground(&mut w, x0, z0);
    let mut walker = w.create_actor(v(x0, y0, z0), v(0.0, 0.0, 0.0), F::new(1.0), human);
    let standing = [(x0 + 3.0, z0 + 1.0, post), (x0 - 20.0, z0 + 35.0, human)].map(|(x, z, s)| {
        let y = ground(&mut w, x, z);
        w.create_actor(v(x, y, z), v(r.range(-3.0, 3.0), 0.0, 0.0), F::new(r.range(0.6, 1.6)), s)
    });
    let (mut changes, mut last, mut on_actors, mut swept) = (0, Vec::new(), 0, 0);
    for step in 0..400 {
        let (x, z) = (x0 - 300.0 + 1.5 * step as f32, z0 + 30.0 * (step as f32 * 0.03).sin());
        let y = ground(&mut w, x, z);
        let (hpr, scale) = (v(step as f32 * 0.1, 0.0, 0.0), F::new(1.0));
        // a creature taken out and put back now and then, as entities come
        if step % 97 == 50 {
            w.release_actor(walker);
            walker = w.create_actor(v(x, y, z), hpr, scale, human);
        }
        w.set_actor_transform(walker, hpr, scale, v(x, y, z));
        let now = rooms_of(&w, walker);
        assert!(!now.is_empty(), "step {step}: in no room at ({x}, {y}, {z})");
        let fresh = w.create_actor(v(x, y, z), hpr, scale, human);
        assert_eq!(rooms_of(&w, fresh), now, "step {step}: the rooms of a fresh actor there");
        w.release_actor(fresh);
        if now != last {
            changes += 1;
            last = now;
        }
        // sweeps about it, and about the standing ones, the walk against
        // brute force (every fourth step: brute force is every actor of
        // every zone near)
        if step % 4 != 0 {
            continue;
        }
        for k in 0..4 {
            let (cx, cz) = if k < 2 { (x, z) } else { (x0 + 3.0 - 11.5 * (k - 2) as f32, z0 + 1.0 + 17.0 * (k - 2) as f32) };
            let cy = ground(&mut w, cx, cz);
            let a = v(cx + r.range(-4.0, 4.0), cy + r.range(0.0, 3.0), cz + r.range(-4.0, 4.0));
            let b = v(cx + r.range(-4.0, 4.0), cy + r.range(0.0, 3.0), cz + r.range(-4.0, 4.0));
            let rad = [0.001, 0.5, 0.6][(r.next() % 3) as usize];
            let mut s1 = CollSphere::<Ieee>::default();
            s1.init(a, b, F::new(rad));
            let mut s2 = s1;
            let mut f = Filter::new(FilterKind::Scene, -1);
            let h1 = w.collide_sphere(&mut s1, &mut f);
            let h2 = w.collide_sphere_brute(&mut s2, &zones);
            assert_eq!(h1, h2, "step {step}: walk {h1}, brute force {h2}, {a:?} -> {b:?} r {rad}");
            assert_eq!(s1.ray.hit_time.bits(), s2.ray.hit_time.bits(), "step {step}: hit time");
            if h1 && (f.hit_actor == walker as i32 || standing.contains(&(f.hit_actor as usize))) {
                on_actors += 1;
            }
            swept += 1;
        }
    }
    eprintln!("400 steps: the room set changed {changes} times; {on_actors} of {swept} sweeps stopped at a creature");
    assert!(changes > 5, "the walk crossed rooms: {changes}");
    assert!(on_actors > swept / 10, "the sweeps met the creatures: {on_actors}");
}
