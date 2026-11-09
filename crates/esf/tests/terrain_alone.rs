//! A zone's terrain built from the zone's record alone
//! (`Container::build_terrain`, the client's map) is the terrain the library
//! builds with the world loaded (`Library::build_terrain`): every palette and
//! texture a zone's terrain names is in the zone's own record (all six
//! worlds), and built a few patches at a time it is the same triangles. Each
//! check has a control that must fail.

use esf::{Container, Library, ReadAt, Window};
use std::fs::File;

fn disc() -> Option<(iso::Iso, std::path::PathBuf)> {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    if !p.exists() {
        eprintln!("SKIP: no disc at {}", p.display());
        return None;
    }
    Some((iso::Iso::open(&p).unwrap(), p))
}

/// Positions, texture ids and triangles of a model, for comparing.
fn shape(m: &esf::Model) -> (Vec<[u32; 3]>, Vec<i32>, usize) {
    let pos = m.pos.iter().map(|p| p.map(f32::to_bits)).collect();
    let tex = m.batches.iter().flat_map(|b| b.layers.iter().map(|l| l.texture)).collect();
    (pos, tex, m.batches.iter().map(|b| b.indices.len()).sum())
}

#[test]
fn a_zones_terrain_builds_from_its_record_alone() {
    let Some((disc, path)) = disc() else { return };
    for name in ["TUNARIA.ESF", "RATHE.ESF", "ODUS.ESF", "LAVASTM.ESF", "PLANESKY.ESF", "SECRETS.ESF"] {
        let e = disc.find(name).unwrap().clone();
        let mut src = Window::new(File::open(&path).unwrap(), e.lba as u64 * iso::SECTOR, e.size as u64);
        let mut lib = Library::new();
        let w = lib.add(name, Container::open_deferred(&mut src).unwrap());
        let n = lib.zone_count(w);
        // a few zones of each world: the first, the last and some between
        let picks: Vec<usize> = (0..n).filter(|z| z % 29 == 0 || *z == n - 1).collect();
        for zi in picks {
            let (off, len) = lib.zone_range(w, zi).unwrap();
            let mut bytes = vec![0u8; len];
            src.read_at(off, &mut bytes).unwrap();
            let alone = Container::parse_records(bytes.clone());
            let z = alone.roots[0];
            let (patches, pre) = (alone.terrain_patches(z), alone.pre_translations(z));
            let mine = alone.build_terrain(&patches, &pre).unwrap();
            lib.add_zone(w, zi, Container::parse_records(bytes));
            let (zf, zn) = lib.zone(w, zi).unwrap();
            let theirs = lib.build_terrain(zf, zn).unwrap();
            assert_eq!(shape(&mine), shape(&theirs), "{name} zone {zi}");
            // every texture it names, in the record itself
            for b in &mine.batches {
                for l in b.layers.iter().filter(|l| l.texture != 0) {
                    assert!(alone.surface(l.texture).is_some(), "{name} zone {zi}: texture {:#x} not in the record", l.texture);
                }
            }
            // a few patches at a time: the same vertices, piece after piece
            let pieces: Vec<esf::Model> = patches.chunks(8).filter_map(|c| alone.build_terrain(c, &pre)).collect();
            let joined: Vec<[u32; 3]> = pieces.iter().flat_map(|m| m.pos.iter().map(|p| p.map(f32::to_bits))).collect();
            assert_eq!(joined, shape(&mine).0, "{name} zone {zi}: by pieces");
            let tris: usize = pieces.iter().map(|m| m.triangles as usize).sum();
            assert_eq!(tris, mine.triangles as usize);
            // the controls: no pre-translations moves the land; a record with
            // no dictionary loses its textures
            let moved = alone.build_terrain(&patches, &[]).unwrap();
            if pre.iter().any(|t| *t != [0.0; 3]) {
                assert_ne!(shape(&moved).0, shape(&mine).0, "{name} zone {zi}: the control, no pre-translations");
            }
            let mut bare = Container::parse_records(lib.file(zf).data.clone());
            bare.dict.clear();
            let untextured = bare.build_terrain(&bare.terrain_patches(bare.roots[0]), &pre).unwrap();
            assert_ne!(shape(&untextured).1, shape(&mine).1, "{name} zone {zi}: the control, no dictionary");
        }
    }
}
