//! A world built zone by zone (`esf::Container::open_deferred`, zones added
//! through the library and `World::add_zone`, in a scrambled order) against
//! the same zones loaded at once (`World::load` over a library that has them)
//! and against the world file loaded whole. The first two must agree exactly:
//! every room holds the same actors in the same order, and sweeps, rays, the
//! walker's floor and the environment flags answer bit for bit. Against the
//! whole file the same holds, less the actors of zones that are not in (the
//! PS2's streaming leaves them out too): rooms compared over the actors of
//! the zones in, and probes wherever the whole world's hit is not an actor of
//! a zone left out. Controls: a zone left out has no collision; a room put
//! out of order fails the comparison.

use esf::{Container, Library, Window};
use vi::coll::{CollRay, CollSphere};
use vi::fpu::{F, Ieee};
use vi::scene::{Filter, FilterKind};
use vi::vect::Vect3;
use vi::world::World;

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

/// Every room of the zones in `zs` as (sprite id, location bits) per actor,
/// in order, over the actors placed by those zones.
fn rooms(w: &World<Ieee>, zs: &[usize]) -> Vec<Vec<(u32, [u32; 3])>> {
    let mut out = Vec::new();
    for &z in zs {
        for r in &w.zones[z].rooms {
            out.push(
                r.actors
                    .iter()
                    .filter(|&&a| w.actor_zone(a).is_some_and(|az| zs.contains(&az)))
                    .map(|&a| (w.actors[a].sprite_id, [w.actors[a].location.x.bits(), w.actors[a].location.y.bits(), w.actors[a].location.z.bits()]))
                    .collect(),
            );
        }
    }
    out
}

/// The world loaded whole, zone by zone, and the same zones at once; the
/// zones in (those within 1,050 of the Freeport point) and those left out.
fn worlds() -> Option<(World<Ieee>, World<Ieee>, World<Ieee>, Vec<usize>, Vec<usize>)> {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !p.exists() {
        eprintln!("SKIP: no disc");
        return None;
    }
    let disc = iso::Iso::open(&p).unwrap();
    let e = disc.find("TUNARIA.ESF").unwrap();
    let mut src = Window::new(std::fs::File::open(&p).unwrap(), e.lba as u64 * iso::SECTOR, e.size as u64);
    let mut whole = Library::new();
    let f = whole.add("TUNARIA.ESF", Container::open(esf::read_all(&mut src).unwrap()).unwrap());
    let full = World::<Ieee>::load(&whole, f).unwrap();

    let mut lib = Library::new();
    let f = lib.add("TUNARIA.ESF", Container::open_deferred(&mut src).unwrap());
    let at = Vect3::<Ieee>::of(24500.0, 0.0, 14500.0);
    let mut near: Vec<usize> = (0..full.zone_bounds.len())
        .filter(|&z| {
            let mut b = full.zone_bounds[z];
            b.min.y = F::new(-1e9);
            b.max.y = F::new(1e9);
            b.distance_squared(&at).0 < 1050.0 * 1050.0
        })
        .collect();
    assert!(near.len() >= 4, "{} zones near Freeport", near.len());
    // a scrambled order: half before the World is made, the rest after, in reverse
    let mut r = Rng(0xF2EE_9027);
    for i in (1..near.len()).rev() {
        near.swap(i, (r.next() % (i as u64 + 1)) as usize);
    }
    let (first, later) = near.split_at(near.len() / 2);
    let install = |lib: &mut Library, zi: usize, src: &mut Window<std::fs::File>| {
        let (off, len) = lib.zone_range(f, zi).unwrap();
        let mut b = vec![0u8; len];
        esf::ReadAt::read_at(src, off, &mut b).unwrap();
        lib.add_zone(f, zi, Container::parse_records(b)).unwrap();
    };
    for &zi in first {
        install(&mut lib, zi, &mut src);
    }
    let mut part = World::<Ieee>::load(&lib, f).unwrap();
    for &zi in later.iter().rev() {
        install(&mut lib, zi, &mut src);
        assert!(part.add_zone(&lib, f, zi));
        assert!(!part.add_zone(&lib, f, zi), "a zone is added once");
    }
    let left_out: Vec<usize> = (0..full.zones.len()).filter(|z| !part.loaded[*z]).collect();
    let batch = World::<Ieee>::load(&lib, f).unwrap();
    let mut sorted = near.clone();
    sorted.sort();
    Some((full, part, batch, sorted, left_out))
}

#[test]
fn zone_by_zone_equals_whole() {
    let Some((mut full, mut part, mut batch, near, left_out)) = worlds() else { return };
    eprintln!("zones in: {near:?}; {} actors against {} whole", part.actors.len(), full.actors.len());
    assert_eq!(rooms(&part, &near), rooms(&batch, &near), "zone by zone against at once: room actor lists");
    assert_eq!(rooms(&full, &near), rooms(&part, &near), "against the whole file: room actor lists");
    // Control: one room's list put out of order no longer compares equal.
    {
        let z = near.iter().copied().find(|&z| part.zones[z].rooms.iter().any(|r| r.actors.len() > 1)).unwrap();
        let ri = part.zones[z].rooms.iter().position(|r| r.actors.len() > 1).unwrap();
        part.zones[z].rooms[ri].actors.reverse();
        assert_ne!(rooms(&full, &near), rooms(&part, &near), "a reversed room must show");
        part.zones[z].rooms[ri].actors.reverse();
    }
    // Sweeps, rays, floors and flags round the point.
    let mut r = Rng(0x0057_AB1E);
    let (mut n, mut hits, mut foreign) = (0, 0, 0);
    // the whole world's hit is an actor of a zone that is not in
    let outside = |w: &World<Ieee>, f: &Filter| f.hit_actor >= 0 && w.actor_zone(f.hit_actor as usize).is_none_or(|z| !near.contains(&z));
    for i in 0..3000 {
        let (x, z) = (r.range(23900.0, 25100.0), r.range(13900.0, 15100.0));
        let probe = |w: &mut World<Ieee>| {
            let mut s = CollSphere::<Ieee>::default();
            s.init(Vect3::of(x, 2000.0, z), Vect3::of(x, -2000.0, z), F::new(0.001));
            let mut f = Filter::new(FilterKind::WalkFloor, -1);
            let hit = w.collide_sphere(&mut s, &mut f).then(|| s.hit_point().y.bits());
            (hit, f)
        };
        let ((g1, f1), (g2, _), (g3, _)) = (probe(&mut full), probe(&mut part), probe(&mut batch));
        assert_eq!(g2, g3, "case {i}: floor at ({x}, {z}), zone by zone against at once");
        if outside(&full, &f1) {
            foreign += 1;
            continue;
        }
        assert_eq!(g1, g2, "case {i}: floor at ({x}, {z})");
        let Some(g) = g1 else { continue };
        let ground = f32::from_bits(g);
        let at = Vect3::<Ieee>::of(x, ground + 1.0, z);
        assert_eq!(full.env_flags(&at), part.env_flags(&at), "case {i}: env flags");
        let a = Vect3::of(x, ground + r.range(0.0, 4.0), z);
        let b = Vect3::of(x + r.range(-6.0, 6.0), ground + r.range(-3.0, 3.0), z + r.range(-6.0, 6.0));
        let rad = [0.001, 0.5, 0.6][(r.next() % 3) as usize];
        let mut s1 = CollSphere::<Ieee>::default();
        s1.init(a, b, F::new(rad));
        let (mut s2, mut s3) = (s1, s1);
        let (mut f1, mut f2, mut f3) = (Filter::new(FilterKind::Scene, -1), Filter::new(FilterKind::Scene, -1), Filter::new(FilterKind::Scene, -1));
        let (h1, h2, h3) = (full.collide_sphere(&mut s1, &mut f1), part.collide_sphere(&mut s2, &mut f2), batch.collide_sphere(&mut s3, &mut f3));
        // the same actor (by what it is and where), or the same room sprite
        let who = |w: &World<Ieee>, f: &Filter| (f.hit_actor >= 0).then(|| (w.actors[f.hit_actor as usize].sprite_id, w.actors[f.hit_actor as usize].location.x.bits()));
        assert_eq!((h2, s2.ray.hit_time.bits(), who(&part, &f2)), (h3, s3.ray.hit_time.bits(), who(&batch, &f3)), "case {i}: sweep, zone by zone against at once");
        if outside(&full, &f1) {
            foreign += 1;
            continue;
        }
        assert_eq!(h1, h2, "case {i}: sweep hit");
        assert_eq!(s1.ray.hit_time.bits(), s2.ray.hit_time.bits(), "case {i}: sweep hit time");
        assert_eq!(s1.ray.hit_normal.x.bits(), s2.ray.hit_normal.x.bits(), "case {i}: sweep normal");
        assert_eq!(who(&full, &f1), who(&part, &f2), "case {i}: the actor hit");
        let mut r1 = CollRay::<Ieee>::default();
        r1.init(a, b);
        let mut r2 = r1;
        let (mut f1, mut f2) = (Filter::new(FilterKind::Scene, -1), Filter::new(FilterKind::Scene, -1));
        let (k1, k2) = (full.collide_ray(&mut r1, &mut f1), part.collide_ray(&mut r2, &mut f2));
        if !outside(&full, &f1) {
            assert_eq!(k1, k2, "case {i}: ray hit");
            assert_eq!(r1.hit_time.bits(), r2.hit_time.bits(), "case {i}: ray hit time");
        }
        hits += h1 as usize;
        n += 1;
    }
    eprintln!("{n} probes with ground, {hits} sweep hits: zone by zone == at once == whole ({foreign} hit an actor of a zone not in, skipped)");
    assert!(n > 2000 && hits > n / 10);
    assert!(foreign < n / 20, "{foreign} probes reached zones not in");

    // Control: a zone left out has no collision where the whole world has.
    let z = *left_out.iter().find(|&&z| full.zones[z].rooms.iter().any(|r| r.sprite.is_some())).unwrap();
    let b = full.zone_bounds[z];
    let (x, zz) = ((b.min.x.0 + b.max.x.0) * 0.5, (b.min.z.0 + b.max.z.0) * 0.5);
    let floor = |w: &mut World<Ieee>| {
        let mut s = CollSphere::<Ieee>::default();
        s.init(Vect3::of(x, 5000.0, zz), Vect3::of(x, -5000.0, zz), F::new(0.001));
        w.collide_sphere(&mut s, &mut Filter::new(FilterKind::WalkFloor, -1))
    };
    assert!(floor(&mut full), "zone {z} has ground whole");
    assert!(!floor(&mut part), "zone {z} left out has none");
}
