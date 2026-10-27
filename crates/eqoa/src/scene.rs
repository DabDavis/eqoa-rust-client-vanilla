//! What the viewer draws: one model, or a zone (terrain plus every placed
//! actor, one upload per distinct sprite).

use esf::{Library, Model, Ref};
use glam::Vec3;
use std::collections::BTreeMap;

pub type Mat = [[f32; 4]; 4];

/// An actor's colour buffers, each its colours.
type ColorBuffers = Vec<Vec<[u8; 4]>>;

pub struct Scene {
    /// Each model with the matrices it is drawn at.
    pub parts: Vec<(Model, Vec<Mat>)>,
    /// Each part's instances' static lights (`VIActor::m_p70`: the zone
    /// actor record's colour / 255; (0, 0, 0, 1) for terrain and models).
    pub statics: Vec<Vec<[f32; 4]>>,
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
    Scene { bounds: bounds_of(&parts), parts, statics: vec![vec![NO_STATIC]], summary, hsprites: Vec::new() }
}

/// A placement's transform as the game builds it (`VIScene::UpdateActor`:
/// `Transform(hpr, scale, location)`, pitch and roll included; about 9% of
/// Tunaria's placements have them).
fn placement(a: &esf::zone::Actor) -> Mat {
    let f = vi::fpu::F::<vi::fpu::Ee>::new;
    let v = |x: [f32; 3]| vi::vect::Vect3::new(f(x[0]), f(x[1]), f(x[2]));
    vi::geom::Matrix44::transform(&v(a.rot), f(a.scale), &v(a.pos)).to_cols()
}

/// No static light: nothing added, the sun term whole.
pub const NO_STATIC: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

pub fn zone(lib: &Library, file: usize, zone: esf::NodeId, index: usize) -> Scene {
    zones(lib, file, &[zone], &format!("zone {index}"), false)
}

/// Terrain per zone plus every placed actor of all of them, one upload per
/// distinct sprite. With `animate`, HSprite placements are held out (in
/// `Scene::hsprites`) to be drawn posed.
pub fn zones(lib: &Library, file: usize, list: &[esf::NodeId], name: &str, animate: bool) -> Scene {
    build(lib, file, list, name, animate, Of::All)
}

/// The actors of one room of `zone` and no terrain (what `VIZone::RenderRoom`
/// draws of a room), one upload per distinct sprite among them.
pub fn room(lib: &Library, file: usize, zone: esf::NodeId, room: i32) -> Scene {
    build(lib, file, &[zone], &format!("room {room}"), false, Of::Room(room))
}

/// The terrain patches of `zone` whose DictID `keep` takes, and no actors: a
/// room's own sprite, which is one of them (`VIZone::RenderRoom`).
pub fn patches(lib: &Library, file: usize, zone: esf::NodeId, keep: &dyn Fn(i32) -> bool) -> Scene {
    build(lib, file, &[zone], "terrain", false, Of::Patches(keep))
}

/// What a scene is made of.
#[derive(Clone, Copy)]
enum Of<'a> {
    All,
    Room(i32),
    Patches(&'a dyn Fn(i32) -> bool),
}

fn build(lib: &Library, file: usize, list: &[esf::NodeId], name: &str, animate: bool, of: Of) -> Scene {
    let c = lib.file(file);
    let mut parts = Vec::new();
    for &zone in list {
        let t = match of {
            Of::All => lib.build_terrain(file, zone),
            Of::Patches(keep) => lib.build_terrain_where(file, zone, keep),
            Of::Room(_) => None,
        };
        if let Some(t) = t {
            parts.push((t, vec![render::IDENTITY]));
        }
    }
    let terrain_parts = parts.len();
    let mut statics: Vec<Vec<[f32; 4]>> = vec![vec![NO_STATIC]; terrain_parts];
    // the actors with each zone's baked lighting
    let actors: Vec<(esf::zone::Actor, Option<ColorBuffers>)> = list
        .iter()
        .flat_map(|&z| {
            let mut lit = c.static_lightings(z);
            c.actors(z).into_iter().map(move |a| {
                let l = lit.remove(&(a.room, a.slot));
                (a, l)
            })
        })
        .filter(|(a, _)| match of {
            Of::All => true,
            Of::Room(r) => a.room == r,
            Of::Patches(_) => false,
        })
        .collect();
    let mut by_sprite: BTreeMap<i32, Vec<(Mat, [f32; 4])>> = BTreeMap::new();
    let mut built: BTreeMap<i32, Option<Model>> = BTreeMap::new();
    let (mut lit_drawn, mut lit_misfit) = (0, 0);
    let mut hsprites = Vec::new();
    for (a, light) in &actors {
        let st = a.color.map(|c| c as f32 / 255.0);
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
        // A lit actor draws its own copy, its colour buffers in place of the
        // vertex colours (one per simple sprite, GetNextColorBuffer's order,
        // which is the flattened model's), when they fit it. An LOD sprite
        // draws its level 0 here, which takes the first buffer (Raster skips
        // one per simple level it passes over).
        if let Some(bufs) = light {
            let lod = lib.lookup(a.sprite).is_some_and(|r| lib.file(r.file).node(r.node).ty == 0x2A00);
            let cols: Vec<[u8; 4]> = if lod { bufs.first().cloned().unwrap_or_default() } else { bufs.concat() };
            let m = built.entry(a.sprite).or_insert_with(|| lib.lookup(a.sprite).and_then(|r| lib.build_model(r)));
            if let Some(m) = m.as_ref() {
                if m.pos.len() == cols.len() {
                    let mut m = m.clone();
                    m.color = cols.iter().map(|c| c.map(|b| b as f32 / 255.0)).collect();
                    parts.push((m, vec![placement(a)]));
                    statics.push(vec![st]);
                    lit_drawn += 1;
                    continue;
                }
                lit_misfit += 1;
            }
        }
        by_sprite.entry(a.sprite).or_default().push((placement(a), st));
    }
    let mut unresolved = 0;
    // Record type → placements that name a sprite with no geometry (lights,
    // emitters, sounds, effect volumes).
    let mut empty: BTreeMap<u16, usize> = BTreeMap::new();
    for (sprite, inst) in by_sprite {
        match lib.lookup(sprite) {
            None => unresolved += inst.len(),
            Some(r) => match built.remove(&sprite).unwrap_or_else(|| lib.build_model(r)) {
                Some(m) => {
                    parts.push((m, inst.iter().map(|i| i.0).collect()));
                    statics.push(inst.iter().map(|i| i.1).collect());
                }
                None => *empty.entry(lib.file(r.file).node(r.node).ty).or_default() += inst.len(),
            },
        }
    }
    let empty: Vec<String> = empty.iter().map(|(t, n)| format!("{n}× {t:04X}")).collect();
    let patches: usize = list.iter().map(|&z| c.terrain_patches(z).len()).sum();
    let summary = format!(
        "{name}: {patches} terrain patches, {} actors ({} parts drawn, {lit_drawn} with baked lighting, {lit_misfit} whose lighting does not fit; {unresolved} unresolved; no geometry: {})",
        actors.len(),
        parts.len() - terrain_parts,
        if empty.is_empty() { "none".into() } else { empty.join(", ") },
    );
    Scene { bounds: bounds_of(&parts), parts, statics, summary, hsprites }
}

/// Every drawable model in one file, by list position.
pub fn list_models(lib: &Library, file: usize) -> Vec<(i32, Ref)> {
    lib.file(file).models().iter().map(|r| (r.dict_id, Ref { file, node: r.node })).collect()
}
