//! The flora sprites off the disc (0x2F00, `ParseFloraSpriteObj`): each one
//! builds a model out of its 0x1230 prim buffer (`ParseFloraPrimBuffer`'s
//! packing, scaled by its position and UV bits) under its own palette, every
//! strip's vertex count read, and every vertex inside the box its header
//! carries, as the game's bounding sphere for it assumes. The control: the
//! positions read with one bit of scale too few fall outside nearly every box.

#[test]
fn every_flora_sprite_builds_inside_its_box() {
    let Some(path) = std::env::var_os("EQOA_ISO")
        .map(std::path::PathBuf::from)
        .or_else(|| Some(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso")))
        .filter(|p| p.exists())
    else {
        eprintln!("SKIP: no disc (set EQOA_ISO)");
        return;
    };
    let mut iso = iso::Iso::open(path).unwrap();
    let c = esf::Container::open(iso.read("ODUS.ESF").unwrap()).unwrap();
    let (mut sprites, mut verts, mut outside_doubled) = (0, 0, 0);
    for (i, n) in c.nodes.iter().enumerate() {
        if n.ty != esf::types::FLORASPRITE {
            continue;
        }
        let h = c.child(i, 0x2F01).expect("a header");
        let d = c.body(h);
        let f = |k: usize| f32::from_le_bytes(d[4 + k * 4..8 + k * 4].try_into().unwrap());
        let (lo, hi) = ([f(0), f(1), f(2)], [f(3), f(4), f(5)]);
        let pb = c.child(i, esf::types::FLORAPRIMBUF).expect("a flora prim buffer");
        let want = i32::from_le_bytes(c.body(pb)[16..20].try_into().unwrap()) as usize;
        let m = c.build_model(i).expect("a model");
        assert_eq!(m.pos.len(), want, "sprite {i}: vertices");
        assert!(m.batches.iter().any(|b| b.layers.iter().any(|l| l.texture != 0)), "sprite {i}: no textured layer");
        let e = 1e-3;
        let inside = |p: &[f32; 3]| (0..3).all(|k| p[k] >= lo[k] - e && p[k] <= hi[k] + e);
        for p in &m.pos {
            assert!(inside(p), "sprite {i}: vertex {p:?} outside {lo:?}..{hi:?}");
        }
        outside_doubled += m.pos.iter().any(|p| !inside(&[p[0] * 2.0, p[1] * 2.0, p[2] * 2.0])) as usize;
        sprites += 1;
        verts += m.pos.len();
    }
    eprintln!("{sprites} flora sprites, {verts} vertices, all inside their boxes; doubled, {outside_doubled} leave theirs");
    assert!(sprites > 100);
    assert!(outside_doubled * 10 >= sprites * 9, "the control: doubled positions stay inside");
}
