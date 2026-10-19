//! What the viewer draws: one model, or a zone (terrain plus every placed
//! actor, one upload per distinct sprite).

use esf::{Library, Model, Ref};
use glam::Vec3;
use std::collections::BTreeMap;

pub type Mat = [[f32; 4]; 4];

pub struct Scene {
    /// Each model with the matrices it is drawn at.
    pub parts: Vec<(Model, Vec<Mat>)>,
    pub bounds: (Vec3, Vec3),
    pub summary: String,
}

fn bounds_of(parts: &[(Model, Vec<Mat>)]) -> (Vec3, Vec3) {
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for (m, mats) in parts {
        for mat in mats {
            let mat = glam::Mat4::from_cols_array_2d(mat);
            for p in &m.pos {
                let q = mat.transform_point3(Vec3::from(*p));
                lo = lo.min(q);
                hi = hi.max(q);
            }
        }
    }
    (lo, hi)
}

pub fn model(dict_id: i32, m: Model) -> Scene {
    let summary = format!(
        "0x{:08X}, {} verts, {} tris, {} parts{}",
        dict_id as u32,
        m.pos.len(),
        m.triangles,
        m.parts,
        if m.skinned { ", skinned" } else { "" }
    );
    let parts = vec![(m, vec![render::IDENTITY])];
    Scene { bounds: bounds_of(&parts), parts, summary }
}

pub fn zone(lib: &Library, file: usize, zone: esf::NodeId, index: usize) -> Scene {
    zones(lib, file, &[zone], &format!("zone {index}"))
}

/// The zones around `index` (every zone with a pre-translation island within
/// `radius` of one of its own): what the walk view draws, so the character
/// can leave the zone it started in.
pub fn zones_near(lib: &Library, file: usize, index: usize, radius: f32) -> Scene {
    let c = lib.file(file);
    let all = c.zones();
    let home = c.pre_translations(all[index]);
    let near: Vec<esf::NodeId> = all
        .iter()
        .copied()
        .filter(|&z| {
            c.pre_translations(z)
                .iter()
                .any(|p| home.iter().any(|h| (p[0] - h[0]).abs() < radius && (p[2] - h[2]).abs() < radius))
        })
        .collect();
    zones(lib, file, &near, &format!("zone {index} and {} around it", near.len() - 1))
}

/// Terrain per zone plus every placed actor of all of them, one upload per
/// distinct sprite.
pub fn zones(lib: &Library, file: usize, list: &[esf::NodeId], name: &str) -> Scene {
    let c = lib.file(file);
    let mut parts = Vec::new();
    for &zone in list {
        if let Some(t) = lib.build_terrain(file, zone) {
            parts.push((t, vec![render::IDENTITY]));
        }
    }
    let terrain_parts = parts.len();
    let actors: Vec<esf::zone::Actor> = list.iter().flat_map(|&z| c.actors(z)).collect();
    let mut by_sprite: BTreeMap<i32, Vec<Mat>> = BTreeMap::new();
    for a in &actors {
        by_sprite.entry(a.sprite).or_default().push(a.matrix());
    }
    let mut unresolved = 0;
    // Record type → placements that name a sprite with no geometry (lights,
    // emitters, sounds, effect volumes).
    let mut empty: BTreeMap<u16, usize> = BTreeMap::new();
    for (sprite, mats) in by_sprite {
        match lib.lookup(sprite) {
            None => unresolved += mats.len(),
            Some(r) => match lib.build_model(r) {
                Some(m) => parts.push((m, mats)),
                None => *empty.entry(lib.file(r.file).node(r.node).ty).or_default() += mats.len(),
            },
        }
    }
    let empty: Vec<String> = empty.iter().map(|(t, n)| format!("{n}× {t:04X}")).collect();
    let patches: usize = list.iter().map(|&z| c.terrain_patches(z).len()).sum();
    let summary = format!(
        "{name}: {patches} terrain patches, {} actors ({} sprites drawn, {unresolved} unresolved; no geometry: {})",
        actors.len(),
        parts.len() - terrain_parts,
        if empty.is_empty() { "none".into() } else { empty.join(", ") },
    );
    Scene { bounds: bounds_of(&parts), parts, summary }
}

/// Every drawable model in one file, by list position.
pub fn list_models(lib: &Library, file: usize) -> Vec<(i32, Ref)> {
    lib.file(file).models().iter().map(|r| (r.dict_id, Ref { file, node: r.node })).collect()
}
