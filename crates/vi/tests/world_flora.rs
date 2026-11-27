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
    // Foliage Density 3x (the client's): the game's plants first, more on
    // the ground, and its time
    let (mut dense, mut dense_ground, mut dense_ms) = (0usize, 0usize, 0f64);
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
        let t = std::time::Instant::now();
        let mut d3 = System::<Ieee> { density: 3, ..Default::default() };
        d3.update(&mut w, &at);
        dense_ms += t.elapsed().as_secs_f64() * 1000.0;
        for (k, s) in d3.swatches.iter().enumerate() {
            let game = first[k].clone().unwrap_or_default();
            let ours: Vec<_> = s.as_ref().map(|s| s.insts.iter().map(|p| (p.world, p.sphere.r.bits())).collect()).unwrap_or_default();
            assert!(ours.len() >= game.len() && format!("{:?}", &ours[..game.len()]) == format!("{game:?}"), "3x: the game's plants first at {at:?}");
            for p in s.iter().flat_map(|s| &s.insts[game.len()..]) {
                let c = p.world.m[3];
                let mut down = vi::coll::CollRay::<Ieee>::default();
                down.init(Vect3::of(c.x.0, c.y.0 + 0.5, c.z.0), Vect3::of(c.x.0, c.y.0 - 0.5, c.z.0));
                let mut fl = vi::scene::Filter::new(vi::scene::FilterKind::Scene, -1);
                if w.collide_ray(&mut down, &mut fl) && (down.hit_point().y.0 - c.y.0).abs() < 0.05 {
                    dense_ground += 1;
                }
                dense += 1;
            }
        }
    }
    eprintln!("flora 3x: {dense} more plants, {dense_ground} on the ground ({:.1} ms a full 3x3)", dense_ms / places.max(1) as f64);
    assert!(dense >= grown * 2, "3x grew {dense} more over the game's {grown}");
    assert!(dense_ground * 100 >= dense * 95, "3x plants off the ground: {dense_ground} of {dense}");
    let per = t0.elapsed().as_secs_f64() * 1000.0 / (places.max(1) * 2) as f64;
    eprintln!("flora: {grown} plants at {places} places ({per:.1} ms a full 3x3), {on_ground} of {checked} on the ground");
    assert!(grown > 1000, "too few plants grew");
    assert!(on_ground * 100 >= checked * 95, "plants off the ground: {on_ground} of {checked}");
}

/// Foliage Distance on the disc (Freeport's ground, the console's model, as
/// the client runs it): at each distance and density, a whole grid grown,
/// a walk east of 400 units (the swatches made as one runs), and `render`
/// with nothing culled (the worst: all round the eye), each timed; plants
/// grown and drawn. The numbers are printed (`--nocapture`); the checks: more
/// drawn farther, never past the cap, Retail's 3 x 3 the game's.
#[test]
fn flora_distance_cost() {
    use vi::flora::Frustum;
    use vi::fpu::Ee;
    use vi::geom::Plane;
    let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !iso_path.exists() {
        eprintln!("SKIP: no disc");
        return;
    }
    let mut iso = iso::Iso::open(iso_path).unwrap();
    let mut lib = esf::Library::new();
    let f = lib.add("TUNARIA.ESF", esf::Container::open(iso.read("TUNARIA.ESF").unwrap()).unwrap());
    let mut w = vi::world::World::<Ee>::load(&lib, f).unwrap();
    // the ground under a point, and a frustum that culls nothing round it
    let ground = |w: &mut vi::world::World<Ee>, x: f32, z: f32| {
        let mut ray = vi::coll::CollRay::<Ee>::default();
        ray.init(Vect3::of(x, 600.0, z), Vect3::of(x, -200.0, z));
        let mut fl = vi::scene::Filter::new(vi::scene::FilterKind::Scene, -1);
        w.collide_ray(&mut ray, &mut fl).then(|| Vect3::<Ee>::of(x, ray.hit_point().y.0 + 1.7, z))
    };
    let open = |at: &Vect3<Ee>| {
        let mut fr = Frustum::<Ee>::default();
        for (k, p) in fr.planes.iter_mut().enumerate() {
            let (n, c) = match k {
                0 => (Vect3::of(1.0, 0.0, 0.0), at.x.0),
                1 => (Vect3::of(-1.0, 0.0, 0.0), -at.x.0),
                2 => (Vect3::of(0.0, 1.0, 0.0), at.y.0),
                3 => (Vect3::of(0.0, -1.0, 0.0), -at.y.0),
                4 => (Vect3::of(0.0, 0.0, 1.0), at.z.0),
                _ => (Vect3::of(0.0, 0.0, -1.0), -at.z.0),
            };
            *p = Plane { n, d: F::new(-c - 1000.0) };
        }
        fr
    };
    let view = |at: &Vect3<Ee>| {
        let (h, v) = ((27.5f32.to_radians().tan() * 16.0 / 9.0).atan(), 27.5f32.to_radians());
        let mut fr = Frustum::<Ee>::default();
        let ns = [(-h.sin(), 0.0, h.cos()), (-h.sin(), 0.0, -h.cos()), (-v.sin(), v.cos(), 0.0), (-v.sin(), -v.cos(), 0.0), (-1.0, 0.0, 0.0), (1.0, 0.0, 0.0)];
        for (k, (x, y, z)) in ns.into_iter().enumerate() {
            let d = -(x * at.x.0 + y * at.y.0 + z * at.z.0) - if k == 5 { 5000.0 } else { 0.0 };
            fr.planes[k] = Plane { n: Vect3::of(x, y, z), d: F::new(d) };
        }
        fr
    };
    // Freeport's grassland east of the city, and a run east across it
    let (x0, z0) = (24200.0f32, 14300.0f32);
    let mut last_drawn = 0usize;
    for dist in [100u16, 150, 200, 300] {
        for density in [1u8, 2, 3] {
            let mut sys = vi::flora::System::<Ee> { distance: dist, density, ..Default::default() };
            let Some(at) = ground(&mut w, x0, z0) else { panic!("no ground at the start") };
            let t = std::time::Instant::now();
            sys.update(&mut w, &at);
            let full = t.elapsed().as_secs_f64() * 1000.0;
            let grown: usize = sys.swatches.iter().flatten().map(|s| s.insts.len()).sum();
            let fr = open(&at);
            let t = std::time::Instant::now();
            let mut drawn = 0;
            for _ in 0..20 {
                drawn = sys.render(&fr, &at).len();
            }
            let render = t.elapsed().as_secs_f64() * 1000.0 / 20.0;
            assert!(drawn <= sys.drawn_most());
            // and a view's: east, 55 degrees up the height at 16:9
            let fr = view(&at);
            let t = std::time::Instant::now();
            let mut seen = Vec::new();
            for _ in 0..20 {
                seen = sys.render(&fr, &at);
            }
            let render_view = t.elapsed().as_secs_f64() * 1000.0 / 20.0;
            let seen_far = seen.iter().map(|d| { let p = d.world.m[3]; ((p.x.0 - at.x.0).powi(2) + (p.z.0 - at.z.0).powi(2)).sqrt() }).fold(0.0f32, f32::max);
            // the run: a step of 2 units at a time (a run's frame is ~0.3)
            let (mut worst, mut total, mut steps) = (0f64, 0f64, 0);
            for k in 1..=200 {
                let Some(p) = ground(&mut w, x0 + k as f32 * 2.0, z0) else { continue };
                let t = std::time::Instant::now();
                sys.update(&mut w, &p);
                let e = t.elapsed().as_secs_f64() * 1000.0;
                worst = worst.max(e);
                total += e;
                steps += 1;
            }
            eprintln!(
                "distance {dist}% density {density}x: {} swatches, {grown} grown, {drawn} drawn all round (cap {}), {} in a view out to {seen_far:.0}; whole grid {full:.2} ms, render {render:.2} ms all round, {render_view:.2} ms a view, run: worst step {worst:.2} ms, {:.3} ms a step over {steps}",
                sys.swatches.iter().flatten().count(),
                sys.drawn_most(),
                seen.len(),
                total / steps.max(1) as f64
            );
            if density == 1 {
                assert!(drawn >= last_drawn, "{dist}: fewer drawn farther");
                last_drawn = drawn;
            }
        }
    }
}
