//! The world walk (world BSP -> zone BSP -> rooms -> sprites and actors)
//! against brute force over every room and actor of the zones involved: the
//! walk only CULLS, so for any sweep both must find the same first hit. A room
//! or actor the walk fails to reach, or a cull that is too tight, shows up as
//! a different hit time. Run on TUNARIA's zone 84 and its neighbours.

use vi::coll::CollSphere;
use vi::fpu::{F, Ieee};
use vi::scene::{Filter, FilterKind};
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

#[test]
fn bsp_walk_equals_brute_force() {
    let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !iso_path.exists() {
        eprintln!("SKIP: no disc");
        return;
    }
    let mut iso = iso::Iso::open(iso_path).unwrap();
    let mut lib = esf::Library::new();
    let f = lib.add("TUNARIA.ESF", esf::Container::open(iso.read("TUNARIA.ESF").unwrap()).unwrap());
    let mut w = vi::world::World::<Ieee>::load(&lib, f).unwrap();
    // Zone 84 and every zone whose leaves the sweeps can reach: those with a
    // pre-translation island within 2.5 km of the sample square.
    let zones: Vec<usize> = (0..w.zones.len())
        .filter(|&z| w.zones[z].pretrans.iter().any(|p| (p.x.0 - 24500.0).abs() < 2500.0 && (p.z.0 - 14500.0).abs() < 2500.0))
        .collect();
    let mut r = Rng(0x0057_AB1E);
    let (mut hits, mut n) = (0, 0);
    for i in 0..3000 {
        let (x, z) = (r.range(23600.0, 25400.0), r.range(13600.0, 15400.0));
        let ground = {
            let mut s = CollSphere::<Ieee>::default();
            s.init(Vect3::of(x, 500.0, z), Vect3::of(x, -200.0, z), F::new(0.001));
            let mut f = Filter::new(FilterKind::Scene, -1);
            if !w.collide_sphere(&mut s, &mut f) {
                continue;
            }
            s.hit_point().y.0
        };
        // Sweeps at body height: walking, falling, and the camera's.
        let a = Vect3::of(x, ground + r.range(0.0, 4.0), z);
        let b = Vect3::of(x + r.range(-6.0, 6.0), ground + r.range(-3.0, 3.0), z + r.range(-6.0, 6.0));
        let rad = [0.001, 0.5, 0.6][(r.next() % 3) as usize];
        let mut s1 = CollSphere::<Ieee>::default();
        s1.init(a, b, F::new(rad));
        let mut s2 = s1;
        let mut f = Filter::new(FilterKind::Scene, -1);
        let h1 = w.collide_sphere(&mut s1, &mut f);
        let h2 = w.collide_sphere_brute(&mut s2, &zones);
        assert_eq!(h1, h2, "case {i}: walk hit {h1}, brute force {h2}, sweep {a:?} -> {b:?} r {rad}");
        assert_eq!(s1.ray.hit_time.bits(), s2.ray.hit_time.bits(), "case {i}: hit time {:?} vs {:?}", s1.ray.hit_time, s2.ray.hit_time);
        n += 1;
        hits += h1 as usize;
    }
    eprintln!("{n} sweeps over {} zones: walk == brute force, {hits} hits", zones.len());
    assert!(hits > n / 10);
}
