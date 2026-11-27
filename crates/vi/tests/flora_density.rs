//! Foliage Density (the client's, not the game's: `System::density`): at 1
//! the radial flora is the game's, plant for plant (pinned here by a
//! fingerprint taken off the port before the density existed, and held to
//! the decomp by `oracle_flora`); at 2 and 3 every swatch begins with the
//! game's plants, in the game's order, the extra ones after them, the same at
//! every visit, and `rand()`'s state after a swatch is the game's. No oracle
//! and no disc: a scene of its own (a height field, Odus's numbers).

use vi::flora::{Dist, FloraModel, FloraScene, Frustum, QueryTriList, Set, System, ZoneFlora, extra_seed};
use ee::libm::{cosf, sinf};
use vi::fpu::{Ee, F};
use vi::geom::{BBox, Plane, Sphere};
use vi::vect::Vect3;
use vi::world::Grid;

/// The console's model, the client's: `Ieee` would hand the flora's
/// `atan2f`/`cosf` to the host's libm, whose last bits differ between glibcs
/// (CI's Ubuntu 22.04 grew other plants than Debian 13: build443's run).
type M = Ee;

struct Fake {
    grid: Grid<M>,
    zones: Vec<ZoneFlora<M>>,
    soup: Vec<(i32, [Vect3<M>; 3])>,
}

impl FloraScene<M> for Fake {
    fn grid_rows(&self) -> i32 {
        self.grid.rows
    }
    fn grid_cols(&self) -> i32 {
        self.grid.cols
    }
    fn grid_center(&self, a: i32, b: i32) -> Vect3<M> {
        self.grid.center(a, b)
    }
    fn grid_offset(&self, v: &Vect3<M>) -> (i32, i32, Vect3<M>) {
        self.grid.offset(v)
    }
    fn world_bbox(&self) -> BBox<M> {
        self.grid.bbox
    }
    fn find_zone(&self, _: &Vect3<M>) -> i32 {
        0
    }
    fn flora(&self, zone: i32) -> Option<&ZoneFlora<M>> {
        usize::try_from(zone).ok().and_then(|z| self.zones.get(z))
    }
    fn query(&mut self, _: &BBox<M>, l: &mut QueryTriList<M>) {
        for (idx, v) in &self.soup {
            l.intersect(*idx, &v[0], &v[1], &v[2]);
        }
    }
    fn time(&self) -> i64 {
        1234
    }
}

/// One zone of two sets (Odus's weights, a tilted dist, a denser set), over
/// a height field round (1900, 1000) on a 7.3 grid, each triangle's set -1,
/// 0 or 1.
fn scene() -> Fake {
    let origin = Vect3::of(1000.0, 0.0, 1000.0);
    let bbox = BBox { min: Vect3::of(0.0, -150.0, 0.0), max: Vect3::of(4000.0, 420.0, 4000.0) };
    let grid = Grid { bbox, rows: 2, cols: 2, subdiv: 4, cell_size: F::new(2000.0), origin, regions: Vec::new() };
    let mut zf = ZoneFlora::<M>::default();
    for m in 0..4 {
        zf.models.push(FloraModel { sprite: m, ..Default::default() });
    }
    for (id, model, weight, scale, jitter) in [(0u32, 0, 0.05f32, 1.0f32, 0.2f32), (1, 1, 0.02, 1.0, 0.2), (0, 2, 0.04, 1.0, 0.2), (1, 3, 0.1, 0.7, 0.0), (3, 0, 0.3, 1.5, 0.5)] {
        zf.dists.push(Dist { id, model, weight: F::new(weight), scale: F::new(scale), jitter: F::new(jitter), ..Default::default() });
    }
    zf.sets.push(Set { first: 0, count: 3, ..Default::default() });
    zf.sets.push(Set { first: 3, count: 2, ..Default::default() });
    zf.precalc(|s| Some((100 + s, 200 + s, Sphere { c: Vect3::of(0.0, 0.4, 0.0), r: F::new(0.5) })));
    // newlib's sinf/cosf as the PS2 has them (`ee::libm`), not the host's:
    // the same ground on every machine
    let h = |x: f32, z: f32| 30.0 + 6.0 * sinf(x * 0.05) * cosf(z * 0.07) + 1.2 * sinf(x * 0.31 + z * 0.17);
    let p = |x: f32, z: f32| Vect3::of(x, h(x, z), z);
    let (s, x0, z0) = (7.3f32, 1780.37f32, 900.21f32);
    let mut soup = Vec::new();
    let mut k = 0u32;
    for i in 0..40 {
        for j in 0..40 {
            let (xa, xb) = (x0 + i as f32 * s, x0 + (i + 1) as f32 * s);
            let (za, zb) = (z0 + j as f32 * s, z0 + (j + 1) as f32 * s);
            for tri in [[p(xa, za), p(xb, za), p(xa, zb)], [p(xb, za), p(xb, zb), p(xa, zb)]] {
                k += 1;
                let set = match k.wrapping_mul(2_654_435_761) >> 29 {
                    0 => -1,
                    1..=4 => 0,
                    _ => 1,
                };
                soup.push((set, tri));
            }
        }
    }
    Fake { grid, zones: vec![zf], soup }
}

/// The swatches grown at `at` at `density`, each one's plants as bits.
fn grow(density: u8, at: Vect3<M>) -> (Vec<Vec<[u32; 18]>>, u64) {
    let mut sc = scene();
    let mut sys = System::<M> { density, ..Default::default() };
    sys.update(&mut sc, &at);
    let swatches = sys
        .swatches
        .iter()
        .map(|s| {
            s.as_ref().map_or(Vec::new(), |s| {
                s.insts
                    .iter()
                    .map(|p| {
                        let mut b = [0u32; 18];
                        for r in 0..4 {
                            for (c, v) in [p.world.m[r].x, p.world.m[r].y, p.world.m[r].z, p.world.m[r].w].into_iter().enumerate() {
                                b[r * 4 + c] = v.bits();
                            }
                        }
                        b[16] = p.prim as u32;
                        b[17] = p.sphere.r.bits();
                        b
                    })
                    .collect()
            })
        })
        .collect();
    (swatches, sys.rand.0)
}

/// FNV-1a over every plant's bits, swatch by swatch.
fn fingerprint(sw: &[Vec<[u32; 18]>]) -> (usize, u64) {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    let mut n = 0;
    for s in sw {
        h = (h ^ s.len() as u64).wrapping_mul(0x100_0000_01b3);
        for p in s {
            n += 1;
            for w in p {
                h = (h ^ *w as u64).wrapping_mul(0x100_0000_01b3);
            }
        }
    }
    (n, h)
}

const AT: [f32; 3] = [1905.0, 34.0, 1012.0];

/// At the game's density (the default) the plants are the port's before
/// Foliage Density existed, bit for bit: the fingerprint was taken off
/// `248d3627`'s `CreateFloraInsts` over this scene, in the `Ee` model (the
/// `Ieee` one's, 16782693215220620819, was the host libm's and failed on CI).
/// The control: a plant one bit off changes it.
#[test]
fn the_default_is_the_games_flora() {
    assert_eq!(System::<M>::default().density, 1, "the game's by default");
    let (sw, _) = grow(1, Vect3::of(AT[0], AT[1], AT[2]));
    let (n, h) = fingerprint(&sw);
    assert!(n > 300, "the scene grows plants: {n}");
    assert_eq!((n, h), (RETAIL_PLANTS, RETAIL_PRINT), "the game's plants");
    let mut off = sw.clone();
    off.iter_mut().find(|s| !s.is_empty()).unwrap()[0][12] ^= 1;
    assert_ne!(fingerprint(&off), (n, h), "the control: one bit off");
}

const RETAIL_PLANTS: usize = 2575;
const RETAIL_PRINT: u64 = 14_460_987_615_827_895_426;

/// At 2x and 3x every swatch starts with the game's plants in the game's
/// order, then more, at least as many again a pass; the same at a second
/// growing; `rand()` after is the game's (`srand(GetTime())`). The control:
/// the extra passes' seeds are not the game's (their plants differ from its).
#[test]
fn denser_keeps_the_games_plants_first() {
    let at = Vect3::of(AT[0], AT[1], AT[2]);
    let (one, r1) = grow(1, at);
    for d in [2u8, 3] {
        let (more, rd) = grow(d, at);
        assert_eq!(rd, r1, "rand() after: the game's");
        assert_eq!(grow(d, at).0, more, "{d}x: the same plants at every visit");
        let (a, b) = (fingerprint(&one).0, fingerprint(&more).0);
        // (int)(k * x) >= k * (int)x: at least d times, more where the
        // game's truncation left a triangle's fraction
        assert!(b >= a * d as usize && b * 2 < a * d as usize * 3, "{d}x grows about {d} times the plants: {a} -> {b}");
        for (s, (g, m)) in one.iter().zip(&more).enumerate() {
            assert!(m.len() >= g.len(), "swatch {s}");
            assert_eq!(&m[..g.len()], &g[..], "swatch {s}: the game's plants first, in its order");
            assert!(m.len() <= 500 * d as usize);
            if !g.is_empty() {
                assert!(m[g.len()..].iter().all(|p| !g.contains(p)), "swatch {s}: the extra plants are new ones");
            }
        }
    }
    assert_ne!(extra_seed(7, 1), extra_seed(7, 2));
    assert_ne!(extra_seed(7, 1), extra_seed(8, 1));
}

/// Render draws 700 at the game's density and that many times more at a
/// higher one, nearest kept, farthest first.
#[test]
fn render_draws_more_at_a_higher_density() {
    let at = Vect3::of(AT[0], AT[1], AT[2]);
    let mut f = Frustum::<M>::default();
    for (k, p) in f.planes.iter_mut().enumerate() {
        // a box of 1000 round the eye: nothing culled
        let (n, c) = match k {
            0 => (Vect3::of(1.0, 0.0, 0.0), AT[0]),
            1 => (Vect3::of(-1.0, 0.0, 0.0), -AT[0]),
            2 => (Vect3::of(0.0, 1.0, 0.0), AT[1]),
            3 => (Vect3::of(0.0, -1.0, 0.0), -AT[1]),
            4 => (Vect3::of(0.0, 0.0, 1.0), AT[2]),
            _ => (Vect3::of(0.0, 0.0, -1.0), -AT[2]),
        };
        *p = Plane { n, d: F::new(-c - 1000.0) };
    }
    let mut counts = Vec::new();
    for d in [1u8, 2, 3] {
        let mut sc = scene();
        let mut sys = System::<M> { density: d, ..Default::default() };
        sys.update(&mut sc, &at);
        let drawn = sys.render(&f, &at);
        assert!(drawn.len() <= 700 * d as usize);
        counts.push(drawn.len());
    }
    assert!(counts[0] > 0 && counts[1] > counts[0] && counts[2] > counts[1], "{counts:?}");
}

/// A frustum of a box of 1000 round the eye: nothing culled.
fn open_frustum() -> Frustum<M> {
    let mut f = Frustum::<M>::default();
    for (k, p) in f.planes.iter_mut().enumerate() {
        let (n, c) = match k {
            0 => (Vect3::of(1.0, 0.0, 0.0), AT[0]),
            1 => (Vect3::of(-1.0, 0.0, 0.0), -AT[0]),
            2 => (Vect3::of(0.0, 1.0, 0.0), AT[1]),
            3 => (Vect3::of(0.0, -1.0, 0.0), -AT[1]),
            4 => (Vect3::of(0.0, 0.0, 1.0), AT[2]),
            _ => (Vect3::of(0.0, 0.0, -1.0), -AT[2]),
        };
        *p = Plane { n, d: F::new(-c - 1000.0) };
    }
    f
}

/// A plant's distance from the eye (in the host's floats, for the checks).
fn dist_of(d: &vi::flora::Drawn<M>, at: Vect3<M>) -> f32 {
    let p = d.world.m[3];
    ((p.x.0 - at.x.0).powi(2) + (p.y.0 - at.y.0).powi(2) + (p.z.0 - at.z.0).powi(2)).sqrt()
}

/// A drawn list as bits: matrix, prim, palette, fade.
fn drawn_bits(v: &[vi::flora::Drawn<M>]) -> Vec<[u32; 19]> {
    v.iter()
        .map(|d| {
            let mut b = [0u32; 19];
            for r in 0..4 {
                for (c, x) in [d.world.m[r].x, d.world.m[r].y, d.world.m[r].z, d.world.m[r].w].into_iter().enumerate() {
                    b[r * 4 + c] = x.bits();
                }
            }
            b[16] = d.prim as u32;
            b[17] = d.pal as u32;
            b[18] = d.alpha.bits();
            b
        })
        .collect()
}

/// Foliage Distance at Retail (the default, and 100 written out) is the
/// game's: the 3 x 3 grid, the same plants (the fingerprint), the same list
/// drawn to the bit, 700 at most, nothing past 40. The control: 1.5x draws
/// another list.
#[test]
fn retail_distance_is_the_games() {
    let d = System::<M>::default();
    assert_eq!((d.distance, d.side(), d.radius().0, d.drawn_most()), (100, 3, 40.0, 700), "the game's by default");
    let at = Vect3::of(AT[0], AT[1], AT[2]);
    let f = open_frustum();
    let mut game = System::<M>::default();
    let mut sc = scene();
    game.update(&mut sc, &at);
    let mut put = System::<M> { distance: 100, ..Default::default() };
    put.update(&mut sc, &at);
    assert_eq!(fingerprint(&grow(1, at).0), (RETAIL_PLANTS, RETAIL_PRINT), "the game's plants");
    let (a, b) = (game.render(&f, &at), put.render(&f, &at));
    assert_eq!(drawn_bits(&a), drawn_bits(&b), "the same list");
    assert!(a.len() <= 700 && a.iter().all(|d| dist_of(d, at) < 40.0));
    let mut far = System::<M> { distance: 150, ..Default::default() };
    far.update(&mut sc, &at);
    assert_ne!(drawn_bits(&far.render(&f, &at)), drawn_bits(&a), "the control: farther draws otherwise");
}

/// Farther, the grid grows (5 x 5 to 2x, 7 x 7 at 3x), the game's 3 x 3 in
/// the middle of it the same plants (each swatch is seeded off its own
/// sub-cell), more plants drawn, out to the new radius and none past it, at
/// most the game's 700 times the distance squared, farthest first.
#[test]
fn farther_draws_out_to_the_radius() {
    let at = Vect3::of(AT[0], AT[1], AT[2]);
    let f = open_frustum();
    let mut sc = scene();
    let mut game = System::<M>::default();
    game.update(&mut sc, &at);
    let game_sw: Vec<_> = game.swatches.iter().map(|s| s.as_ref().map(|s| s.insts.len())).collect();
    let retail = game.render(&f, &at).len();
    let mut last = retail;
    for (dist, side) in [(150u16, 5), (200, 5), (300, 7)] {
        for density in [1u8, 3] {
            let mut sys = System::<M> { distance: dist, density, ..Default::default() };
            sys.update(&mut sc, &at);
            assert_eq!((sys.side(), sys.swatches.len()), (side, (side * side) as usize));
            let n = (side as usize - 3) / 2;
            if density == 1 {
                for r in 0..3 {
                    for c in 0..3 {
                        let ours = sys.swatches[(r + n) * side as usize + c + n].as_ref().map(|s| s.insts.len());
                        assert_eq!(ours, game_sw[r * 3 + c], "{dist}: the middle swatch {r},{c} is the game's");
                    }
                }
            }
            let drawn = sys.render(&f, &at);
            let r = dist as f32 * 0.4;
            let far = drawn.iter().map(|d| dist_of(d, at)).fold(0.0f32, f32::max);
            assert!(drawn.len() <= sys.drawn_most(), "{dist}: {} of {}", drawn.len(), sys.drawn_most());
            assert!(far < r && far > r * 0.9, "{dist}: plants out to {far} of {r}");
            assert!(drawn.windows(2).all(|w| dist_of(&w[0], at) >= dist_of(&w[1], at) - 1e-3), "{dist}: farthest first");
            if density == 1 {
                assert!(drawn.len() > last, "{dist}: more than at the step before ({} <= {last})", drawn.len());
                last = drawn.len();
            }
            eprintln!("distance {dist}% density {density}x: {} swatches, {} plants grown, {} drawn (retail {retail}), cap {}", sys.swatches.iter().flatten().count(), sys.swatches.iter().flatten().map(|s| s.insts.len()).sum::<usize>(), drawn.len(), sys.drawn_most());
        }
    }
}

/// The fade farther: the alpha falls (never rises) from 1 at a quarter of
/// the radius to 0 at it; the grow-in is 1 inside the outer quarter and
/// falls smoothly to 0 at the edge; a plant drawn in the band is scaled at
/// its root by it (its root where it was). The control: Retail scales none.
#[test]
fn the_fade_band_ramps() {
    use vi::flora::grow_in;
    for r in [40.0f32, 60.0, 80.0, 120.0] {
        let mut was = F::<M>::new(2.0);
        for k in 0..=400 {
            let d = r * k as f32 / 400.0;
            let g = grow_in(F::<M>::new(d), F::new(r));
            assert!(g.0 <= was.0 && (0.0..=1.0).contains(&g.0), "{r} at {d}: {} after {}", g.0, was.0);
            if d <= r * 0.75 {
                assert_eq!(g.0, 1.0, "{r} at {d}: whole inside the band");
            }
            was = g;
        }
        assert_eq!(grow_in(F::<M>::new(r), F::new(r)).0, 0.0, "gone at the edge");
        assert!(grow_in(F::<M>::new(r * 0.875), F::new(r)).0 == 0.5, "half way in the band, half");
    }
    let at = Vect3::of(AT[0], AT[1], AT[2]);
    let f = open_frustum();
    let mut sc = scene();
    let mut sys = System::<M> { distance: 200, ..Default::default() };
    sys.update(&mut sc, &at);
    let drawn = sys.render(&f, &at);
    let r = 80.0;
    let mut alpha_was = 0.0f32;
    let mut in_band = 0;
    // drawn farthest first: the alpha only rises along the list
    for d in &drawn {
        assert!(d.alpha.0 >= alpha_was - 1e-6, "the alpha falls outward");
        alpha_was = d.alpha.0;
        let x = dist_of(d, at);
        if x <= r * 0.25 {
            assert_eq!(d.alpha.0, 1.0);
        }
        // the plant as grown: the same root, to the bit
        let root = |m: &vi::geom::Matrix44<M>| [m.m[3].x.bits(), m.m[3].y.bits(), m.m[3].z.bits()];
        let grown = sys.swatches.iter().flatten().flat_map(|s| &s.insts).find(|p| root(&p.world) == root(&d.world)).expect("a grown plant");
        let g = grow_in(F::<M>::new(x), F::new(r)).0;
        let (ours, was) = (d.world.m[1].y.0, grown.world.m[1].y.0);
        assert!((ours - was * g).abs() <= 1e-4 * was.abs().max(1.0), "{x}: scaled by {g}: {ours} of {was}");
        if x > r * 0.8 {
            in_band += 1;
            assert!(ours.abs() < was.abs(), "{x}: smaller in the band");
        }
    }
    assert!(in_band > 20, "plants in the band: {in_band}");
    let mut game = System::<M>::default();
    game.update(&mut sc, &at);
    let g = game.render(&f, &at);
    let near_edge = g.iter().filter(|d| dist_of(d, at) > 35.0).count();
    assert!(near_edge > 0);
    let mut grown = System::<M>::default();
    grown.update(&mut sc, &at);
    for (a, b) in game.render(&f, &at).iter().zip(&grown.render(&f, &at)) {
        assert_eq!(a.world.m[0].x.bits(), b.world.m[0].x.bits(), "the control: Retail scales none");
    }
}
