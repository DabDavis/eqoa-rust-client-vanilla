//! What the viewer draws: one model, or a zone (terrain plus every placed
//! actor, one upload per distinct sprite).

use esf::{Library, Model, Ref};
use glam::Vec3;
use std::collections::BTreeMap;

pub type Mat = [[f32; 4]; 4];

pub struct Scene {
    /// Each model with the matrices it is drawn at.
    pub parts: Vec<(Model, Vec<Mat>)>,
    /// HSprite placements held out of `parts` to be drawn animated: (sprite
    /// record, actor transform as the game builds it).
    pub hsprites: Vec<(Ref, vi::geom::Matrix44<vi::fpu::Ee>)>,
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
    Scene { bounds: bounds_of(&parts), parts, summary, hsprites: Vec::new() }
}

pub fn zone(lib: &Library, file: usize, zone: esf::NodeId, index: usize) -> Scene {
    zones(lib, file, &[zone], &format!("zone {index}"), false)
}

/// Terrain per zone plus every placed actor of all of them, one upload per
/// distinct sprite. With `animate`, HSprite placements are held out (in
/// `Scene::hsprites`) to be drawn posed.
pub fn zones(lib: &Library, file: usize, list: &[esf::NodeId], name: &str, animate: bool) -> Scene {
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
    let mut hsprites = Vec::new();
    for a in &actors {
        if animate
            && let Some(r) = lib.lookup(a.sprite)
            && lib.file(r.file).node(r.node).ty == esf::types::HSPRITE
        {
            let f = vi::fpu::F::new;
            let m = vi::geom::Matrix44::transform(
                &vi::vect::Vect3::new(f(a.rot[0]), f(a.rot[1]), f(a.rot[2])),
                f(a.scale),
                &vi::vect::Vect3::new(f(a.pos[0]), f(a.pos[1]), f(a.pos[2])),
            );
            hsprites.push((r, m));
            continue;
        }
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
    Scene { bounds: bounds_of(&parts), parts, summary, hsprites }
}

/// Every drawable model in one file, by list position.
pub fn list_models(lib: &Library, file: usize) -> Vec<(i32, Ref)> {
    lib.file(file).models().iter().map(|r| (r.dict_id, Ref { file, node: r.node })).collect()
}
