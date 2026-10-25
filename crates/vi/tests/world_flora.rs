//! The radial flora on the disc, TUNARIA round zone 84 (Freeport's). The
//! query walk (`World::query_intersection`: world tree, zone trees, rooms)
//! against brute force over every room of the zones involved: the walk only
//! culls, so it must keep the same triangles. Then the flora grown over the
//! world (`vi::flora::System` with the world as its scene): plants come up,
//! each one inside its swatch's sub-cell and on ground the walker stands on,
//! and a Reset grows the same plants again (each swatch is seeded off its
//! sub-cell). The control: brute force without the zones' pre-translations
//! keeps other triangles.

use vi::flora::{FloraScene, QueryTriList, System};
use vi::fpu::{F, Ieee};
use vi::geom::BBox;
use vi::vect::Vect3;

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

/// A list's triangles as sortable keys: index, then the nine coordinates' bits.
fn keys(l: &QueryTriList<Ieee>) -> Vec<[u32; 10]> {
    let mut v: Vec<[u32; 10]> = l
        .tris
        .iter()
        .map(|t| {
            let b = |p: Vect3<Ieee>| [p.x.bits(), p.y.bits(), p.z.bits()];
            let (a, c, d) = (b(t.v0), b(t.v1), b(t.v2));
            [t.actor as u32, a[0], a[1], a[2], c[0], c[1], c[2], d[0], d[1], d[2]]
        })
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn flora_on_tunaria() {
    let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !iso_path.exists() {
        eprintln!("SKIP: no disc");
        return;
    }
    let mut iso = iso::Iso::open(iso_path).unwrap();
    let mut lib = esf::Library::new();
    let f = lib.add("TUNARIA.ESF", esf::Container::open(iso.read("TUNARIA.ESF").unwrap()).unwrap());
    let mut w = vi::world::World::<Ieee>::load(&lib, f).unwrap();
    let zones: Vec<usize> = (0..w.zones.len())
        .filter(|&z| w.zones[z].pretrans.iter().any(|p| (p.x.0 - 24500.0).abs() < 2500.0 && (p.z.0 - 14500.0).abs() < 2500.0))
        .collect();
    let with_flora = zones.iter().filter(|&&z| !w.zones[z].flora.sets.is_empty()).count();
    assert!(with_flora > 0, "no zone round Freeport has flora");
    let (ylo, yhi) = { let b = w.world_bbox(); (b.min.y.0, b.max.y.0) };
    let mut r = Rng(0x00F1_0A7A);
    let (mut boxes, mut kept, mut control_differs) = (0, 0, 0);
    for i in 0..400 {
        let (x, z) = (r.range(23600.0, 25400.0), r.range(13600.0, 15400.0));
        // a swatch's box (40 by 40, the world's height), or a smaller one
        let (hx, hz, y0, y1) = if i % 2 == 0 { (20.0, 20.0, ylo, yhi) } else { (r.range(0.5, 15.0), r.range(0.5, 15.0), r.range(-50.0, 50.0), r.range(50.0, 200.0)) };
        let bb = BBox { min: Vect3::of(x - hx, y0, z - hz), max: Vect3::of(x + hx, y1, z + hz) };
        let up = Vect3::of(0.0, 1.0, 0.0);
        let mut walk = QueryTriList::new(100_000, &bb, up, F::new(30.0));
        w.query_intersection(&bb, &mut walk);
        let mut brute = QueryTriList::new(100_000, &bb, up, F::new(30.0));
        w.query_brute(&bb, &mut brute, &zones);
        assert_eq!(keys(&walk), keys(&brute), "box {i} {bb:?}: the walk and brute force keep different triangles");
        // the control: brute force with the pre-translations zeroed
        let saved: Vec<_> = zones.iter().map(|&z| std::mem::take(&mut w.zones[z].pretrans)).collect();
        w.collide.set_pretranslations(&[Vect3::default(); 32]);
        let mut bare = QueryTriList::new(100_000, &bb, up, F::new(30.0));
        w.query_brute(&bb, &mut bare, &zones);
        for (k, &z) in zones.iter().enumerate() {
            w.zones[z].pretrans = saved[k].clone();
        }
        control_differs += (keys(&bare) != keys(&walk)) as usize;
        boxes += 1;
        kept += walk.tris.len();
    }
    eprintln!("{boxes} boxes over {} zones ({with_flora} with flora): walk == brute force, {kept} triangles; the control differed in {control_differs}", zones.len());
    assert!(kept > boxes * 10, "too few triangles to mean anything");
    assert!(control_differs > boxes / 2, "the control rarely differed");

    // the flora at points round Freeport
    let mut sys = System::<Ieee>::default();
    let (mut grown, mut places, mut on_ground, mut checked) = (0usize, 0, 0, 0);
    let t0 = std::time::Instant::now();
    for _ in 0..12 {
        let (x, z) = (r.range(23600.0, 25400.0), r.range(13600.0, 15400.0));
        let mut ray = vi::coll::CollRay::<Ieee>::default();
        ray.init(Vect3::of(x, 600.0, z), Vect3::of(x, -200.0, z));
        let mut fl = vi::scene::Filter::new(vi::scene::FilterKind::Scene, -1);
        if !w.collide_ray(&mut ray, &mut fl) {
            continue;
        }
        let at = Vect3::of(x, ray.hit_point().y.0 + 1.7, z);
        sys.reset();
        sys.update(&mut w, &at);
        let first: Vec<_> = sys.swatches.iter().map(|s| s.as_ref().map(|s| s.insts.iter().map(|p| (p.world, p.sphere.r.bits())).collect::<Vec<_>>())).collect();
        for s in sys.swatches.iter().flatten() {
            for p in &s.insts {
                let c = p.world.m[3];
                let e = 1e-3;
                assert!(c.x.0 >= s.bbox.min.x.0 - e && c.x.0 <= s.bbox.max.x.0 + e && c.z.0 >= s.bbox.min.z.0 - e && c.z.0 <= s.bbox.max.z.0 + e, "a plant outside its swatch: {c:?} in {:?}", s.bbox);
                let mut down = vi::coll::CollRay::<Ieee>::default();
                down.init(Vect3::of(c.x.0, c.y.0 + 0.5, c.z.0), Vect3::of(c.x.0, c.y.0 - 0.5, c.z.0));
                let mut fl = vi::scene::Filter::new(vi::scene::FilterKind::Scene, -1);
                if w.collide_ray(&mut down, &mut fl) && (down.hit_point().y.0 - c.y.0).abs() < 0.05 {
                    on_ground += 1;
                }
                checked += 1;
                grown += 1;
            }
        }
        places += 1;
        // a Reset grows the same plants
        sys.reset();
        sys.update(&mut w, &at);
        let again: Vec<_> = sys.swatches.iter().map(|s| s.as_ref().map(|s| s.insts.iter().map(|p| (p.world, p.sphere.r.bits())).collect::<Vec<_>>())).collect();
        assert!(format!("{first:?}") == format!("{again:?}"), "a Reset grew other plants at {at:?}");
    }
    let per = t0.elapsed().as_secs_f64() * 1000.0 / (places.max(1) * 2) as f64;
    eprintln!("flora: {grown} plants at {places} places ({per:.1} ms a full 3x3), {on_ground} of {checked} on the ground");
    assert!(grown > 1000, "too few plants grew");
    assert!(on_ground * 100 >= checked * 95, "plants off the ground: {on_ground} of {checked}");
}
