//! `QueryEmittersAtLocation` for TUNARIA's music regions (streamed-audio
//! boxes, sprite type 12) against brute force: at points in and around every
//! box, the room walk must find exactly the boxes a scan of EVERY actor in the
//! file finds (the walk only culls), with the same keys, in the same order.

use vi::emit::camera_view;
use vi::fpu::{F, Ieee};
use vi::geom::Matrix44;
use vi::vect::Vect3;
use vi::world::SpriteKind;

#[test]
fn music_regions_equal_brute_force() {
    let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !iso_path.exists() {
        eprintln!("SKIP: no disc");
        return;
    }
    let mut iso = iso::Iso::open(iso_path).unwrap();
    let mut lib = esf::Library::new();
    let f = lib.add("TUNARIA.ESF", esf::Container::open(iso.read("TUNARIA.ESF").unwrap()).unwrap());
    let mut w = vi::world::World::<Ieee>::load(&lib, f).unwrap();
    let boxes: Vec<usize> = (0..w.actors.len()).filter(|&a| w.actors[a].sprite.is_some_and(|s| matches!(w.sprites[s].kind, SpriteKind::Stream { .. }))).collect();
    assert!(boxes.len() > 100, "{} music regions placed", boxes.len());
    let (mut found, mut points) = (0, 0);
    for (i, &a) in boxes.iter().enumerate() {
        let s = w.actors[a].sprite.unwrap();
        let b = w.sprites[s].bbox;
        let c = Vect3::<Ieee>::new((b.min.x + b.max.x) * F::new(0.5), (b.min.y + b.max.y) * F::new(0.5), (b.min.z + b.max.z) * F::new(0.5));
        for (k, off) in [(0.0, 0.0), (0.45, 0.2), (-0.49, -0.3), (0.6, 0.0)].iter().enumerate() {
            let mut p = c;
            p.x = p.x + (b.max.x - b.min.x) * F::new(off.0);
            p.z = p.z + (b.max.z - b.min.z) * F::new(off.1);
            let at = w.actors[a].object_to_world.mul_point(&p);
            let dir = Vect3::of([1.0, 0.0, -0.7, 0.3][(i + k) % 4], -0.2, [0.0, 1.0, 0.7, -0.9][(i + k) % 4]);
            let (view, _) = camera_view(&at, &dir);
            let Some(got) = w.query_emitters_at_location(&at, &view, 12, 32) else { continue };
            // brute force: every placed box whose actor's sphere holds the
            // point and whose box holds it in its own space
            let mut want: Vec<(F<Ieee>, usize)> = Vec::new();
            for &o in &boxes {
                let act = &w.actors[o];
                let d = Vect3::<Ieee>::new(at.x - act.bound.c.x, at.y - act.bound.c.y, at.z - act.bound.c.z);
                if !(d.x * d.x + d.y * d.y + d.z * d.z <= act.bound.r * act.bound.r) || act.flags & 36 != 0 {
                    continue;
                }
                let sp = &w.sprites[act.sprite.unwrap()];
                let m = Matrix44::mul(&act.object_to_world, &view);
                let inv = Matrix44::invert_affine(&m);
                let q = Vect3::<Ieee>::new(inv.m[3].x, inv.m[3].y, inv.m[3].z);
                let bb = sp.bbox;
                if !(q.x >= bb.min.x && q.y >= bb.min.y && q.z >= bb.min.z && q.x <= bb.max.x && q.y <= bb.max.y && q.z <= bb.max.z) {
                    continue;
                }
                let SpriteKind::Stream { priority, .. } = sp.kind else { unreachable!() };
                let half = F::new(0.5);
                let r = Vect3::<Ieee>::new(q.x - (bb.min.x + bb.max.x) * half, q.y - (bb.min.y + bb.max.y) * half, q.z - (bb.min.z + bb.max.z) * half);
                want.push((F::new((priority + 1) as f32) - r.magnitude() * F::new(0.0001), o));
            }
            let mut got_set: Vec<(u32, usize)> = got.iter().map(|e| (e.key.bits(), e.actor)).collect();
            let mut want_set: Vec<(u32, usize)> = want.iter().map(|(k, o)| (k.bits(), *o)).collect();
            got_set.sort();
            want_set.sort();
            assert_eq!(got_set, want_set, "box {i} point {k} at {at:?}");
            for pair in got.windows(2) {
                assert!(pair[0].key <= pair[1].key, "ascending keys");
            }
            points += 1;
            found += got.len();
        }
    }
    eprintln!("{} boxes, {points} points: the walk equals brute force, {found} regions heard", boxes.len());
    assert!(found > points / 2);
}
