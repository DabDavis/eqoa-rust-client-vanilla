//! A CSprite whose body is a skinned LOD sprite (0x2A40) builds each level's
//! mesh apart (`esf::Batch::lod`), in the order its level table (0x2A70) names
//! them, so one level can be drawn without the other, as `VILODSprite::Raster`
//! draws them. Every CSprite in CHAR.ESF: an LOD body's levels are exactly its
//! level sprites built alone, the near level the denser mesh; any other body
//! has no levels. The control is the same model with its levels merged, as it
//! was built before (both meshes drawn at once).

use esf::types::*;

fn disc() -> Option<iso::Iso> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let p = std::env::var_os("EQOA_ISO").map(std::path::PathBuf::from).unwrap_or_else(|| root.join("run/eqoa.iso"));
    if !p.exists() {
        eprintln!("SKIP: no disc at {}", p.display());
        return None;
    }
    Some(iso::Iso::open(p).unwrap())
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

/// The level table of a CSprite's LOD body: (sprite DictID, distance) a level.
fn levels(c: &esf::Container, cs: usize) -> Option<Vec<(i32, f32)>> {
    let arr = c.child(cs, CSPRITE_ARRAY)?;
    let lod = c.node(arr).children.iter().copied().find(|&k| c.node(k).ty == CSPRITE_VARIANT)?;
    let d = c.body(c.child(lod, 0x2A70)?);
    Some((0..u32_at(d, 0) as usize).map(|i| (u32_at(d, 4 + i * 8) as i32, f32::from_bits(u32_at(d, 8 + i * 8)))).collect())
}

/// Triangles of `m` in level `l`.
fn tris(m: &esf::Model, l: Option<u8>) -> usize {
    m.batches.iter().filter(|b| b.lod == l).map(|b| b.indices.len() / 3).sum()
}

/// What the levels of a built CSprite must be, or why not.
fn check(c: &esf::Container, cs: usize, m: &esf::Model) -> Result<(), String> {
    let Some(lv) = levels(c, cs) else {
        return if m.lod_dists.is_empty() && m.batches.iter().all(|b| b.lod.is_none()) { Ok(()) } else { Err("levels on a plain body".into()) };
    };
    if m.lod_dists != lv.iter().map(|l| l.1).collect::<Vec<_>>() {
        return Err(format!("distances {:?}, the table says {:?}", m.lod_dists, lv));
    }
    if tris(m, None) != 0 {
        return Err(format!("{} triangles in no level", tris(m, None)));
    }
    for (i, &(id, _)) in lv.iter().enumerate() {
        let alone = c.build_model(c.lookup(id).ok_or("a level names no sprite")?).ok_or("an empty level")?;
        if tris(m, Some(i as u8)) != alone.triangles as usize {
            return Err(format!("level {i}: {} triangles, its sprite alone {}", tris(m, Some(i as u8)), alone.triangles));
        }
    }
    if lv.len() == 2 && tris(m, Some(0)) <= tris(m, Some(1)) {
        return Err("the near level is not the denser mesh".into());
    }
    Ok(())
}

#[test]
fn an_lod_body_builds_its_levels_apart() {
    let Some(mut disc) = disc() else { return };
    let c = esf::Container::open(disc.read("CHAR.ESF").unwrap()).unwrap();
    let (mut bodies, mut lods) = (0, 0);
    for cs in (0..c.nodes.len()).filter(|&i| c.node(i).ty == CSPRITE) {
        let Some(m) = c.build_model(cs) else { continue };
        bodies += 1;
        if let Err(e) = check(&c, cs, &m) {
            panic!("CSprite node {cs}: {e}");
        }
        if !m.lod_dists.is_empty() {
            lods += 1;
            // the control: the levels merged, as both were drawn before
            let mut merged = m.clone();
            merged.batches.iter_mut().for_each(|b| b.lod = None);
            assert!(check(&c, cs, &merged).is_err(), "merged levels pass");
        }
    }
    // 568 CSprites, one of them with no geometry
    assert_eq!((bodies, lods), (567, 40), "CSprites built, LOD bodies among them");
    // The barbarian of the report (F3 20261020-202738): 1,682 triangles near,
    // 856 far, to 10 and 100.
    let m = c.build_model(c.lookup(0x61CA3EDFu32 as i32).unwrap()).unwrap();
    assert_eq!((tris(&m, Some(0)), tris(&m, Some(1)), m.lod_dists.clone()), (1682, 856, vec![10.0, 100.0]));
}
