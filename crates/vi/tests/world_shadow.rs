//! The shadows' query on the disc, TUNARIA round zone 84 (Freeport's): the
//! walk that takes the actors (`World::query_intersection_actors`, as
//! `RenderShadow`'s default `VIActorFilter` queries: world tree, zone trees,
//! rooms, each room's actors under their stamps) against brute force over
//! every room and every actor of the zones involved: the walk only culls, so
//! it must keep the same triangles. Boxes of a shadow's size at the ground a
//! ray finds (on the terrain or on a prop), and larger ones. The control: the
//! flora's walk, which takes no actor, keeps fewer triangles in many of the
//! same boxes (the props are what the actors add).

use vi::flora::QueryTriList;
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
fn shadow_query_on_tunaria() {
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
    // the placed props of those zones (a resolved sprite, not a proxy)
    let mut props: Vec<usize> = zones.iter().flat_map(|&z| w.zones[z].rooms.iter().flat_map(|r| r.actors.iter().copied())).filter(|&a| w.actors[a].sprite.is_some() && w.actors[a].flags & 4 == 0).collect();
    props.sort_unstable();
    props.dedup();
    assert!(props.len() > 100, "{} props round Freeport", props.len());
    let mut r = Rng(0x05AD_0B10);
    let (mut boxes, mut kept, mut props_add, mut on_props) = (0, 0, 0, 0);
    let up = Vect3::of(0.0, 1.0, 0.0);
    for i in 0..600 {
        let (x, z) = (r.range(23600.0, 25400.0), r.range(13600.0, 15400.0));
        let bb = if i % 4 == 2 {
            // a shadow's on a prop: round its bound's centre, about its top half
            let b = w.actors[props[(r.next() % props.len() as u64) as usize]].bound;
            let (cx, cz) = (b.c.x.0 + r.range(-0.5, 0.5) * b.r.0, b.c.z.0 + r.range(-0.5, 0.5) * b.r.0);
            let y = b.c.y.0 + r.range(-0.2, 0.9) * b.r.0;
            let (hx, hz) = (r.range(0.3, 1.5), r.range(0.3, 1.5));
            BBox { min: Vect3::of(cx - hx, y - 1.0, cz - hz), max: Vect3::of(cx + hx, y + 0.5, cz + hz) }
        } else if i % 4 != 3 {
            // a shadow's: the ground under (x, z), 1 below to 0.5 above
            let mut ray = vi::coll::CollRay::<Ieee>::default();
            ray.init(Vect3::of(x, 600.0, z), Vect3::of(x, -200.0, z));
            let mut fl = vi::scene::Filter::new(vi::scene::FilterKind::Scene, -1);
            if !w.collide_ray(&mut ray, &mut fl) {
                continue;
            }
            on_props += (fl.hit_actor != -1) as usize;
            let y = ray.hit_point().y.0;
            let (hx, hz) = (r.range(0.3, 1.5), r.range(0.3, 1.5));
            BBox { min: Vect3::of(x - hx, y - 1.0, z - hz), max: Vect3::of(x + hx, y + 0.5, z + hz) }
        } else {
            let (hx, hz) = (r.range(2.0, 15.0), r.range(2.0, 15.0));
            BBox { min: Vect3::of(x - hx, r.range(-50.0, 50.0), z - hz), max: Vect3::of(x + hx, r.range(50.0, 200.0), z + hz) }
        };
        let mut walk = QueryTriList::new(100_000, &bb, up, F::new(80.0));
        w.query_intersection_actors(&bb, &mut walk);
        let mut brute = QueryTriList::new(100_000, &bb, up, F::new(80.0));
        w.query_brute_actors(&bb, &mut brute, &zones);
        assert_eq!(keys(&walk), keys(&brute), "box {i} {bb:?}: the walk and brute force keep different triangles");
        // the control: the rooms alone
        let mut rooms = QueryTriList::new(100_000, &bb, up, F::new(80.0));
        w.query_intersection(&bb, &mut rooms);
        assert!(rooms.tris.len() <= walk.tris.len(), "box {i}: the rooms alone kept more");
        props_add += (rooms.tris.len() < walk.tris.len()) as usize;
        boxes += 1;
        kept += walk.tris.len();
    }
    eprintln!("{boxes} boxes over {} zones and {} props ({on_props} rays found a prop): walk == brute force, {kept} triangles; the props added to {props_add}", zones.len(), props.len());
    assert!(kept > boxes * 3, "too few triangles to mean anything");
    assert!(props_add > boxes / 10, "the props rarely added anything");
}
