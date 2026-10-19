//! The outdoors probe on TUNARIA's own data (no oracle reaches a whole
//! scene pick; the strip walk and the leaves are held in oracle_pick.rs):
//! from high above, the first surface under each point of a grid round
//! Freeport, and where another surface lies below it (a roof over a floor),
//! the probe from just under the roof. Baked alpha is sun visibility, so the
//! floors under roofs must come out darker than open ground. ("A surface with
//! another within 60 below" also takes in foliage, stairs and overhangs, so
//! the roofed set is mixed; tests/lighting_fit.rs holds the colour buffers'
//! assignment.)

use vi::fpu::Ieee;
use vi::pick::PickSegment;
use vi::vect::Vect3;

#[test]
fn roofed_ground_is_darker_than_open_ground() {
    let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !iso_path.exists() {
        eprintln!("SKIP: no disc");
        return;
    }
    let mut iso = iso::Iso::open(iso_path).unwrap();
    let mut lib = esf::Library::new();
    let f = lib.add("TUNARIA.ESF", esf::Container::open(iso.read("TUNARIA.ESF").unwrap()).unwrap());
    let mut w = vi::world::World::<Ieee>::load(&lib, f).unwrap();
    let pick = |w: &mut vi::world::World<Ieee>, x: f32, y0: f32, y1: f32, z: f32| {
        let mut s = PickSegment::<Ieee>::new(Vect3::of(x, y0, z), Vect3::of(x, y1, z));
        w.pick_outdoors(&mut s).then(|| (s.hit_point().y.0, s.hit_color.a.0))
    };
    let (mut open, mut roofed) = (Vec::new(), Vec::new());
    for i in 0..120 {
        for j in 0..120 {
            let (x, z) = (24000.0 + i as f32 * 10.0, 14000.0 + j as f32 * 10.0);
            let Some((top, a)) = pick(&mut w, x, 3000.0, -3000.0, z) else { continue };
            match pick(&mut w, x, top - 0.5, top - 60.0, z) {
                Some((_, b)) => roofed.push(b),
                None => open.push(a),
            }
        }
    }
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
    eprintln!("open ground: {} points, mean alpha {:.3}; under a roof: {} points, mean alpha {:.3}", open.len(), mean(&open), roofed.len(), mean(&roofed));
    assert!(open.len() > 1000 && roofed.len() > 20, "{} {}", open.len(), roofed.len());
    assert!(mean(&roofed) < mean(&open) - 0.1);
}
