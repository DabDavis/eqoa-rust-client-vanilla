//! Geometry: PrimBuffers (0x1200 / 0x1210), materials, and sprites flattened
//! into one mesh per model.
//!
//! Vertex layouts, by the buffer's own type:
//!
//! ```text
//! 2  x,y,z,u,v i16 | normal 3×i8 | colour 4×u8              17 B
//! 4  as 2, plus vgroup i16                                  19 B
//! 5  x,y,z,u,v i16 | normal 3×i8 | bone 4×u8 | weight 4×u8   21 B
//! v0 x,y,z f32 | u,v f32 | normal 3×f32 | colour 4×u8        36 B
//! ```
//!
//! Packed positions are scaled by 2^-p1 and UVs by 2^-p2. Ported from the
//! Blender importer's `esf.py`; GroupSprite placement follows
//! `go-eqoa-esf-tools-v2` (`SpritePlacement.Transform`) exactly, because that is
//! what `tests/meshes.rs`'s fixtures were made by.

use crate::types::*;
use crate::{Container, NodeId, f32le, i32le};

/// One face record: a triangle strip with its own material index.
#[derive(Clone, Debug, Default)]
pub struct VertexList {
    pub material: i32,
    pub pos: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub normal: Vec<[f32; 3]>,
    /// RGB in 0..1. A skinned buffer has none (white).
    pub color: Vec<[f32; 3]>,
    /// Skinned only: four bone indices (255 = unused) and weights summing to 127.
    pub bones: Vec<[u8; 4]>,
    pub weights: Vec<[u8; 4]>,
    /// Terrain only (type 4): the pre-translation group of each vertex.
    pub vgroup: Vec<i16>,
}

#[derive(Clone, Debug, Default)]
pub struct PrimBuffer {
    pub kind: i32,
    pub lists: Vec<VertexList>,
}

fn vertex_size(kind: i32) -> Option<usize> {
    match kind {
        2 => Some(17),
        4 => Some(19),
        5 => Some(21),
        _ => None,
    }
}

impl PrimBuffer {
    pub fn read(c: &Container, id: NodeId) -> PrimBuffer {
        let n = c.node(id);
        let d = &c.data[..];
        let mut p = n.body;
        let mut out = PrimBuffer::default();
        let (mut pack1, mut pack2) = (1.0f32, 1.0f32);
        let nfaces;
        if n.version == 0 {
            nfaces = i32le(d, p + 4);
            p += 12; // numMaterials, nfaces, unknown
        } else {
            if n.version > 1 {
                p += 4; // DictID
            }
            out.kind = i32le(d, p);
            nfaces = i32le(d, p + 8);
            let (p1, p2) = (i32le(d, p + 16), i32le(d, p + 20));
            p += 28; // kind, nmats, nfaces, unknown, p1, p2, p3
            pack1 = (2.0f64.powi(-p1)) as f32;
            pack2 = (2.0f64.powi(-p2)) as f32;
            if vertex_size(out.kind).is_none() {
                return out;
            }
        }
        let stride = if n.version == 0 { 36 } else { vertex_size(out.kind).unwrap() };
        if !(0..=100_000).contains(&nfaces) {
            return out;
        }
        for _ in 0..nfaces {
            if p + 8 > d.len() {
                break;
            }
            let (count, material) = (i32le(d, p), i32le(d, p + 4));
            p += 8;
            if !(0..=200_000).contains(&count) || p + count as usize * stride > d.len() {
                break;
            }
            let mut l = VertexList { material, ..Default::default() };
            for v in 0..count as usize {
                let r = &d[p + v * stride..p + (v + 1) * stride];
                if n.version == 0 {
                    let f = |k: usize| f32le(r, k * 4);
                    l.pos.push([f(0), f(1), f(2)]);
                    l.uv.push([f(3), f(4)]);
                    l.normal.push([f(5), f(6), f(7)]);
                    l.color.push([r[32] as f32 / 255.0, r[33] as f32 / 255.0, r[34] as f32 / 255.0]);
                    continue;
                }
                let s = |k: usize| i16::from_le_bytes([r[k * 2], r[k * 2 + 1]]) as f32;
                l.pos.push([s(0) * pack1, s(1) * pack1, s(2) * pack1]);
                l.uv.push([s(3) * pack2, s(4) * pack2]);
                l.normal.push([r[10] as i8 as f32 / 127.0, r[11] as i8 as f32 / 127.0, r[12] as i8 as f32 / 127.0]);
                if out.kind == 5 {
                    l.color.push([1.0; 3]);
                    l.bones.push([r[13], r[14], r[15], r[16]]);
                    l.weights.push([r[17], r[18], r[19], r[20]]);
                } else {
                    l.color.push([r[13] as f32 / 255.0, r[14] as f32 / 255.0, r[15] as f32 / 255.0]);
                    if out.kind == 4 {
                        l.vgroup.push(i16::from_le_bytes([r[17], r[18]]));
                    }
                }
            }
            p += count as usize * stride;
            out.lists.push(l);
        }
        out
    }
}

/// One textured layer of a material. The colour is where per-race skin tone
/// lives; the 3×3 UV transform is where a layer's tiling rate lives.
#[derive(Clone, Debug)]
pub struct Layer {
    pub texture: i32,
    pub color: [u8; 4],
    pub uv_transform: [f32; 9],
    pub blend: i32,
}

impl Layer {
    /// u' = u·m0 + v·m3 + m6, v' = u·m1 + v·m4 + m7; a zero diagonal is identity.
    pub fn apply_uv(&self, uv: [f32; 2]) -> [f32; 2] {
        let m = &self.uv_transform;
        let (mut m0, mut m4) = (m[0], m[4]);
        if m0 == 0.0 && m4 == 0.0 {
            m0 = 1.0;
            m4 = 1.0;
        }
        [uv[0] * m0 + uv[1] * m[3] + m[6], uv[0] * m[1] + uv[1] * m4 + m[7]]
    }
}

/// Every layer of one material (0x1100), in record order.
pub fn read_material(d: &[u8], body: usize, version: i16) -> Vec<Layer> {
    let n = i32le(d, body);
    let mut p = body + 4;
    if version > 1 {
        p += 4; // tessellate
    }
    if version > 2 {
        p += 4; // emissive
    }
    if !(0..=64).contains(&n) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for _ in 0..n {
        if p + 68 > d.len() {
            break;
        }
        let texture = i32le(d, p + 4);
        let blend = i32le(d, p + 12);
        let color = [d[p + 16], d[p + 17], d[p + 18], d[p + 19]];
        let mut uv_transform = [0.0; 9];
        for (k, m) in uv_transform.iter_mut().enumerate() {
            *m = f32le(d, p + 20 + k * 4);
        }
        // flags, tex, wrap, blend, colour, uv[9], lod, uRate, vRate
        p += 20 + 36 + 12;
        out.push(Layer { texture, color, uv_transform, blend });
    }
    out
}

/// Triangles that share one material entry.
#[derive(Clone, Debug)]
pub struct Batch {
    /// The material's layers; the first with a texture is the base.
    pub layers: Vec<Layer>,
    pub indices: Vec<u32>,
}

impl Batch {
    pub fn texture(&self) -> i32 {
        self.layers.iter().find(|l| l.texture != 0).map_or(0, |l| l.texture)
    }
}

/// A sprite flattened into one vertex array plus per-material batches.
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub pos: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub normal: Vec<[f32; 3]>,
    pub color: Vec<[f32; 3]>,
    pub bones: Vec<[u8; 4]>,
    pub weights: Vec<[u8; 4]>,
    pub batches: Vec<Batch>,
    pub parts: u32,
    pub triangles: u32,
    pub skinned: bool,
    /// GroupSprite members naming a DictID this container does not hold.
    pub missing: u32,
}

/// A model listed out of a container.
#[derive(Clone, Copy, Debug)]
pub struct ModelRef {
    pub node: NodeId,
    pub ty: u16,
    pub dict_id: i32,
}

/// A GroupSprite member's placement: `SpritePlacement.Transform` in the Go
/// reference — scale if positive, rotate about Y by `rot` (sin/cos rounded to
/// f32 first), translate.
#[derive(Clone, Copy)]
struct Placement {
    pos: [f32; 3],
    scale: f32,
    rot: f32,
}

impl Placement {
    const NONE: Placement = Placement { pos: [0.0; 3], scale: 0.0, rot: 0.0 };

    fn apply(&self, mut p: [f32; 3]) -> [f32; 3] {
        if self.scale > 0.0 {
            p = p.map(|c| c * self.scale);
        }
        let (sin, cos) = ((self.rot as f64).sin() as f32, (self.rot as f64).cos() as f32);
        let (x, z) = (p[2] * sin + p[0] * cos, p[2] * cos - p[0] * sin);
        [x + self.pos[0], p[1] + self.pos[1], z + self.pos[2]]
    }
}

impl Container {
    /// Material index → layers, for one sprite's palette.
    pub fn sprite_materials(&self, sprite: NodeId) -> Vec<Vec<Layer>> {
        let mut mp = self.child(sprite, MATPAL);
        if mp.is_none() {
            // SubSprite variants name their palette by DictID instead of nesting it.
            let hdr = self.child(sprite, SUBSPRITE_HDR).or_else(|| self.child(sprite, SKINSUBSPRITE2));
            if let Some(h) = hdr.filter(|&h| self.node(h).size >= 8) {
                mp = self.lookup(i32le(&self.data, self.node(h).body + 4)).filter(|&n| self.node(n).ty == MATPAL);
            }
        }
        let Some(arr) = mp.and_then(|mp| self.child(mp, MATERIAL_ARRAY)) else {
            return Vec::new();
        };
        self.node(arr)
            .children
            .iter()
            .filter(|&&m| self.node(m).ty == MATERIAL)
            .map(|&m| read_material(&self.data, self.node(m).body, self.node(m).version))
            .collect()
    }

    /// A LODSprite reference lands on its empty header; step out to the
    /// highest-detail level (the first of 0x2A20's children).
    pub fn resolve_lod(&self, mut id: NodeId) -> NodeId {
        if let (LODSPRITE_HDR, Some(p)) = (self.node(id).ty, self.node(id).parent) {
            id = p;
        }
        if self.node(id).ty == LODSPRITE
            && let Some(&first) = self.child(id, LODSPRITE_ARRAY).and_then(|a| self.node(a).children.first())
        {
            return first;
        }
        id
    }

    /// Flattens a sprite into one mesh. GroupSprite members are resolved through
    /// the dictionary and placed. `None` when it carries no geometry.
    pub fn build_model(&self, id: NodeId) -> Option<Model> {
        let id = self.resolve_lod(id);
        let mut m = Model::default();
        // (material layers) → batch index, keyed on texture and tint so per-race
        // skin tones under one texture stay apart.
        let mut keys: Vec<(i32, [u8; 4])> = Vec::new();
        let mut add = |m: &mut Model, sprite: NodeId, place: Placement| {
            let Some(pb) = self.child(sprite, PRIMBUF).or_else(|| self.child(sprite, SKINPRIMBUF)) else {
                return;
            };
            m.parts += 1;
            let geom = PrimBuffer::read(self, pb);
            let mats = self.sprite_materials(sprite);
            for l in geom.lists.iter().filter(|l| !l.pos.is_empty()) {
                let layers = usize::try_from(l.material).ok().and_then(|i| mats.get(i)).cloned().unwrap_or_default();
                let base_layer = layers.iter().find(|x| x.texture != 0).cloned();
                let base = m.pos.len() as u32;
                m.pos.extend(l.pos.iter().map(|&p| place.apply(p)));
                m.uv.extend(l.uv.iter().map(|&uv| base_layer.as_ref().map_or(uv, |b| b.apply_uv(uv))));
                m.normal.extend_from_slice(&l.normal);
                m.color.extend_from_slice(&l.color);
                if l.bones.is_empty() {
                    m.bones.extend(std::iter::repeat_n([255u8; 4], l.pos.len()));
                    m.weights.extend(std::iter::repeat_n([0u8; 4], l.pos.len()));
                } else {
                    m.skinned = true;
                    m.bones.extend_from_slice(&l.bones);
                    m.weights.extend_from_slice(&l.weights);
                }
                let key = base_layer.as_ref().map_or((0, [255; 4]), |b| (b.texture, b.color));
                let bi = keys.iter().position(|k| *k == key).unwrap_or_else(|| {
                    keys.push(key);
                    m.batches.push(Batch { layers: layers.clone(), indices: Vec::new() });
                    keys.len() - 1
                });
                let idx = &mut m.batches[bi].indices;
                // Strip: triangle k is (k, k+2, k+1) when k is even, (k, k+1, k+2) when odd.
                for k in 0..l.pos.len().saturating_sub(2) as u32 {
                    let (a, b, c) = if k % 2 == 0 { (k, k + 2, k + 1) } else { (k, k + 1, k + 2) };
                    idx.extend([base + a, base + b, base + c]);
                    m.triangles += 1;
                }
            }
        };
        let ty = self.node(id).ty;
        match ty {
            GROUPSPRITE => {
                if let Some(mem) = self.child(id, GROUPSPRITE_MEMBERS) {
                    let d = &self.data;
                    let mut p = self.node(mem).body;
                    let n = i32le(d, p).clamp(0, 4096);
                    p += 4;
                    for _ in 0..n {
                        if p + 32 > d.len() {
                            break;
                        }
                        let place = Placement {
                            pos: [f32le(d, p + 4), f32le(d, p + 8), f32le(d, p + 12)],
                            scale: f32le(d, p + 16),
                            rot: f32le(d, p + 20),
                        };
                        match self.lookup(i32le(d, p)) {
                            Some(s) => add(&mut m, s, place),
                            None => m.missing += 1,
                        }
                        p += 32;
                    }
                }
                add(&mut m, id, Placement::NONE);
            }
            CSPRITE => {
                let arr = self.child(id, CSPRITE_ARRAY).map(|a| self.node(a).children.clone()).unwrap_or_default();
                for c in arr {
                    if self.node(c).ty == CSPRITE_VARIANT {
                        for &s in self.child(c, CSPRITE_VARIANT_MESHES).map_or(&[][..], |v| &self.node(v).children) {
                            add(&mut m, s, Placement::NONE);
                        }
                    } else {
                        add(&mut m, c, Placement::NONE);
                    }
                }
            }
            HSPRITE => {
                let arr = self.child(id, HSPRITE_ARRAY).map(|a| self.node(a).children.clone()).unwrap_or_default();
                for c in arr {
                    add(&mut m, c, Placement::NONE);
                }
            }
            _ => add(&mut m, id, Placement::NONE),
        }
        if m.pos.is_empty() {
            return None;
        }
        // Equipment stores vertex colour as all zero (its lighting is baked
        // elsewhere); drawn as albedo that is pure black, so use flat grey.
        if m.color.iter().all(|c| *c == [0.0; 3]) {
            m.color.iter_mut().for_each(|c| *c = [0.62; 3]);
        }
        Some(m)
    }

    /// Every sprite that can be drawn as an object: the resource directory's
    /// sprites, or failing that, every sprite in the file (not descending into
    /// a sprite — its parts are not models of their own).
    pub fn models(&self) -> Vec<ModelRef> {
        let res_dir = self.find_deep(&self.roots, RESDIR2);
        let scope = res_dir.map_or(self.roots.clone(), |r| self.node(r).children.clone());
        let out = self.collect_sprites(&scope, false);
        if !out.is_empty() {
            return out;
        }
        self.collect_sprites(&self.roots, true)
    }

    fn find_deep(&self, from: &[NodeId], ty: u16) -> Option<NodeId> {
        let mut stack = from.to_vec();
        while let Some(n) = stack.pop() {
            if self.node(n).ty == ty {
                return Some(n);
            }
            stack.extend_from_slice(&self.node(n).children);
        }
        None
    }

    fn collect_sprites(&self, nodes: &[NodeId], deep: bool) -> Vec<ModelRef> {
        const SPRITES: [u16; 5] = [SIMPLESPRITE, GROUPSPRITE, CSPRITE, LODSPRITE, HSPRITE];
        const HEADERS: [u16; 5] = [SIMPLESPRITE_HDR, GROUPSPRITE_HDR, CSPRITE_HDR, LODSPRITE_HDR, HSPRITE_HDR];
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = nodes.iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            let ty = self.node(n).ty;
            if SPRITES.contains(&ty) {
                let dict_id = HEADERS
                    .iter()
                    .find_map(|&h| self.child(n, h))
                    .map_or(0, |h| i32le(&self.data, self.node(h).body));
                out.push(ModelRef { node: n, ty, dict_id });
            } else if deep {
                stack.extend(self.node(n).children.iter().rev());
            }
        }
        out
    }
}
