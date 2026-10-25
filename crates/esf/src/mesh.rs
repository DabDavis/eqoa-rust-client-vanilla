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
//! A flora prim buffer (0x1230, `ParseFloraPrimBuffer`) has no normal and no
//! colour: x,y,z,u,v as five f32 (packing 7) or five i16 (packing 8).
//!
//! Packed positions are scaled by 2^-p1 and UVs by 2^-p2. Ported from the
//! Blender importer's `esf.py`, except GroupSprite placement, which follows the
//! decomp (see `Placement`).

use crate::types::*;
use crate::{Container, NodeId, f32le, i32le};

/// One face record: a triangle strip with its own material index.
#[derive(Clone, Debug, Default)]
pub struct VertexList {
    pub material: i32,
    pub pos: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub normal: Vec<[f32; 3]>,
    /// RGBA in 0..1. On packed geometry the alpha is a sun-visibility factor
    /// (VU1 `VIRStrip2`/`VIRStrip3`). A skinned buffer has no colour: (0,0,0,1).
    pub color: Vec<[f32; 4]>,
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
        if c.node(id).ty == FLORAPRIMBUF {
            return Self::read_flora(c, id);
        }
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
                    l.color.push([r[32], r[33], r[34], r[35]].map(|c| c as f32 / 255.0));
                    continue;
                }
                let s = |k: usize| i16::from_le_bytes([r[k * 2], r[k * 2 + 1]]) as f32;
                l.pos.push([s(0) * pack1, s(1) * pack1, s(2) * pack1]);
                l.uv.push([s(3) * pack2, s(4) * pack2]);
                l.normal.push([r[10] as i8 as f32 / 127.0, r[11] as i8 as f32 / 127.0, r[12] as i8 as f32 / 127.0]);
                if out.kind == 5 {
                    l.color.push([0.0, 0.0, 0.0, 1.0]);
                    l.bones.push([r[13], r[14], r[15], r[16]]);
                    l.weights.push([r[17], r[18], r[19], r[20]]);
                } else {
                    l.color.push([r[13], r[14], r[15], r[16]].map(|c| c as f32 / 255.0));
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

impl PrimBuffer {
    /// `ParseFloraPrimBuffer`: id, packing, materials, strips, vertices,
    /// position and UV bits; then per strip its count and material and its
    /// vertices. No normal or colour comes off the file (`VIRStrip6` lights a
    /// plant without them): each vertex gets an up normal and white.
    fn read_flora(c: &Container, id: NodeId) -> PrimBuffer {
        let d = &c.data[..];
        let mut p = c.node(id).body;
        let mut out = PrimBuffer::default();
        if p + 28 > d.len() {
            return out;
        }
        out.kind = i32le(d, p + 4);
        let nprims = i32le(d, p + 12);
        let (vbits, uvbits) = (i32le(d, p + 20), i32le(d, p + 24));
        p += 28;
        let stride = match out.kind {
            7 => 20,
            8 => 10,
            _ => return out,
        };
        let (pack1, pack2) = ((2.0f64.powi(-vbits)) as f32, (2.0f64.powi(-uvbits)) as f32);
        if !(0..=100_000).contains(&nprims) {
            return out;
        }
        for _ in 0..nprims {
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
                if out.kind == 7 {
                    let f = |k: usize| f32le(r, k * 4);
                    l.pos.push([f(0), f(1), f(2)]);
                    l.uv.push([f(3), f(4)]);
                } else {
                    let s = |k: usize| i16::from_le_bytes([r[k * 2], r[k * 2 + 1]]) as f32;
                    l.pos.push([s(0) * pack1, s(1) * pack1, s(2) * pack1]);
                    l.uv.push([s(3) * pack2, s(4) * pack2]);
                }
                l.normal.push([0.0, 1.0, 0.0]);
                l.color.push([1.0, 1.0, 1.0, 1.0]);
            }
            p += count as usize * stride;
            out.lists.push(l);
        }
        out
    }
}

/// One layer of a material (`ParseMaterial`). The colour is where per-race
/// skin tone lives; the 3×3 UV transform is where a layer's tiling rate lives.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    /// 1 textured fill, 2 the second UV set.
    pub flags: i32,
    pub texture: i32,
    /// 0 repeat, 1 clamp, 2 region, 3 mirror.
    pub wrap: i32,
    /// `VIAlphaBlendMode`: 0 opaque (alpha test), 1 add, 2 subtract, 3 alpha,
    /// 4 multiply the destination by the source alpha.
    pub blend: i32,
    pub color: [u8; 4],
    pub uv_transform: [f32; 9],
    pub lod_bias: f32,
    /// UV scroll, per second (`VIMaterial::Process`).
    pub uv_rate: [f32; 2],
}

impl Layer {
    /// u' = u·m0 + v·m3 + m6, v' = u·m1 + v·m4 + m7; a zero diagonal is identity.
    /// The transform as the renderer applies it per layer: (m0, m1, m3, m4,
    /// m6, m7), a zero diagonal reading as identity (as the reference
    /// parsers do).
    pub fn uv_coeffs(&self) -> [f32; 6] {
        let m = &self.uv_transform;
        let (mut m0, mut m4) = (m[0], m[4]);
        if m0 == 0.0 && m4 == 0.0 {
            m0 = 1.0;
            m4 = 1.0;
        }
        [m0, m[1], m[3], m4, m[6], m[7]]
    }

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
        let flags = i32le(d, p);
        let texture = i32le(d, p + 4);
        let wrap = i32le(d, p + 8);
        let blend = i32le(d, p + 12);
        let color = [d[p + 16], d[p + 17], d[p + 18], d[p + 19]];
        let mut uv_transform = [0.0; 9];
        for (k, m) in uv_transform.iter_mut().enumerate() {
            *m = f32le(d, p + 20 + k * 4);
        }
        // Version 0 reads the LOD bias and throws it away for 0.8.
        let lod_bias = if version == 0 { 0.8 } else { f32le(d, p + 56) };
        let uv_rate = [f32le(d, p + 60), f32le(d, p + 64)];
        // flags, tex, wrap, blend, colour, uv[9], lod, uRate, vRate
        p += 20 + 36 + 12;
        out.push(Layer { flags, texture, wrap, blend, color, uv_transform, lod_bias, uv_rate });
    }
    out
}

/// Triangles that share one material entry.
#[derive(Clone, Debug)]
pub struct Batch {
    /// The material's index in its sprite's palette (-1 for none): what a
    /// CSprite's texture slots name.
    pub material: i32,
    /// The material's layers; the first with a texture is the base.
    pub layers: Vec<Layer>,
    pub indices: Vec<u32>,
    /// The level of a skinned LOD body the triangles belong to (`VILODSprite`
    /// draws one level at a time, by distance: [`Model::lod_dists`]), or `None`
    /// for triangles drawn at every distance.
    pub lod: Option<u8>,
}

impl Batch {
    pub fn texture(&self) -> i32 {
        self.layers.iter().find(|l| l.texture != 0).map_or(0, |l| l.texture)
    }
}

/// A sprite flattened into one vertex array plus per-material batches. UVs are
/// the RAW vertex UVs: each layer's transform is applied when it is drawn.
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub pos: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub normal: Vec<[f32; 3]>,
    pub color: Vec<[f32; 4]>,
    pub bones: Vec<[u8; 4]>,
    pub weights: Vec<[u8; 4]>,
    pub batches: Vec<Batch>,
    pub parts: u32,
    pub triangles: u32,
    pub skinned: bool,
    /// GroupSprite members naming a DictID this container does not hold.
    pub missing: u32,
    /// A CSprite whose body is a skinned LOD sprite (0x2A40): each level's
    /// distance (`m_levels[i].m_dist`), in level order. Its batches carry their
    /// level ([`Batch::lod`]), and one level is drawn at a time.
    pub lod_dists: Vec<f32>,
}

/// A model listed out of a container.
#[derive(Clone, Copy, Debug)]
pub struct ModelRef {
    pub node: NodeId,
    pub ty: u16,
    pub dict_id: i32,
}

/// A GroupSprite member's placement, as `VIGroupSprite` applies it:
/// `VIMatrix44::Transform(m_loc, m_scale, m_hpr)`, whose arguments are
/// (ANGLES, scale, TRANSLATION). So the record is id, angles, scale,
/// translation -- every member in ITEM.ESF and TUNARIA.ESF has an all-zero
/// first triple -- and the scale is applied as it is, never skipped.
/// `go-eqoa-esf-tools-v2` reads it as translation, scale, yaw, which puts every
/// member at its group's origin (`claude-notes/groupsprite-member-order-20261019.md`).
#[derive(Clone, Copy)]
struct Placement {
    angles: [f32; 3],
    scale: f32,
    translation: [f32; 3],
}

impl Placement {

    /// `p * Transform(angles, scale, t)`, row vectors, in host floats.
    fn apply(&self, p: [f32; 3]) -> [f32; 3] {
        let (si, ci) = self.angles[0].sin_cos();
        let (sj, cj) = self.angles[1].sin_cos();
        let (sh, ch) = self.angles[2].sin_cos();
        let s = self.scale;
        let r = [
            [(ch * ci + sh * sj * si) * s, (sh * cj) * s, (-ch * si + sh * sj * ci) * s],
            [(-sh * ci + ch * sj * si) * s, (ch * cj) * s, (sh * si + ch * sj * ci) * s],
            [(cj * si) * s, (-sj) * s, (cj * ci) * s],
        ];
        let t = self.translation;
        [
            p[0] * r[0][0] + p[1] * r[1][0] + p[2] * r[2][0] + t[0],
            p[0] * r[0][1] + p[1] * r[1][1] + p[2] * r[2][1] + t[1],
            p[0] * r[0][2] + p[1] * r[1][2] + p[2] * r[2][2] + t[2],
        ]
    }
}

/// Resolves a DictID to the record it names and the container holding it.
pub type Resolve<'a> = dyn Fn(i32) -> Option<(&'a Container, NodeId)> + 'a;

/// Material index → layers, for one sprite's palette.
fn sprite_materials<'a>(c: &'a Container, sprite: NodeId, resolve: &Resolve<'a>) -> Vec<Vec<Layer>> {
    let mut mp = c.child(sprite, MATPAL).map(|n| (c, n));
    if mp.is_none() {
        // SubSprite variants name their palette by DictID instead of nesting it.
        let hdr = c.child(sprite, SUBSPRITE_HDR).or_else(|| c.child(sprite, SKINSUBSPRITE2));
        if let Some(h) = hdr.filter(|&h| c.node(h).size >= 8) {
            mp = resolve(i32le(&c.data, c.node(h).body + 4)).filter(|(pc, n)| pc.node(*n).ty == MATPAL);
        }
    }
    let Some((pc, arr)) = mp.and_then(|(pc, mp)| pc.child(mp, MATERIAL_ARRAY).map(|a| (pc, a))) else {
        return Vec::new();
    };
    pc.node(arr)
        .children
        .iter()
        .filter(|&&m| pc.node(m).ty == MATERIAL)
        .map(|&m| read_material(&pc.data, pc.node(m).body, pc.node(m).version))
        .collect()
}

/// Appends one sprite's PrimBuffer to `m`, placed, its batches in LOD level
/// `lod` (`None`: drawn at every distance).
#[allow(clippy::too_many_arguments)]
fn add_sprite<'a>(
    m: &mut Model,
    keys: &mut Vec<(i32, Vec<Layer>, Option<u8>)>,
    c: &'a Container,
    sprite: NodeId,
    place: impl Fn([f32; 3]) -> [f32; 3],
    pre: &[[f32; 3]],
    resolve: &Resolve<'a>,
    lod: Option<u8>,
) {
    let Some(pb) = c.child(sprite, PRIMBUF).or_else(|| c.child(sprite, SKINPRIMBUF)).or_else(|| c.child(sprite, FLORAPRIMBUF)) else {
        return;
    };
    m.parts += 1;
    let geom = PrimBuffer::read(c, pb);
    let mats = sprite_materials(c, sprite, resolve);
    for l in geom.lists.iter().filter(|l| !l.pos.is_empty()) {
        let layers = usize::try_from(l.material).ok().and_then(|i| mats.get(i)).cloned().unwrap_or_default();
        let base = m.pos.len() as u32;
        // Terrain: each vertex is offset by its pre-translation group. pbtype 4
        // carries the group; pbtype 2 takes entry 0 (the zero value indexes the
        // table just the same in the Go reference).
        m.pos.extend(l.pos.iter().enumerate().map(|(i, &p)| {
            let p = place(p);
            let g = l.vgroup.get(i).copied().unwrap_or(0);
            match usize::try_from(g).ok().and_then(|g| pre.get(g)) {
                Some(t) => [p[0] + t[0], p[1] + t[1], p[2] + t[2]],
                None => p,
            }
        }));
        m.uv.extend_from_slice(&l.uv);
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
        // One batch per distinct material (every layer) and LOD level, so
        // per-race skin tones under one texture stay apart, layers are kept
        // whole and one level can be drawn without the other.
        let material = if (l.material as usize) < mats.len() { l.material } else { -1 };
        let bi = keys.iter().position(|k| k.0 == material && k.1 == layers && k.2 == lod).unwrap_or_else(|| {
            keys.push((material, layers.clone(), lod));
            m.batches.push(Batch { material, layers: layers.clone(), indices: Vec::new(), lod });
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
}

/// Flattens a sprite into one mesh. GroupSprite members are resolved through
/// `resolve` and placed. `None` when it carries no geometry.
pub fn build<'a>(c: &'a Container, id: NodeId, resolve: &Resolve<'a>) -> Option<Model> {
    let id = c.resolve_lod(id);
    let mut m = Model::default();
    let mut keys = Vec::new();
    let none = |p| p;
    match c.node(id).ty {
        GROUPSPRITE => {
            if let Some(mem) = c.child(id, GROUPSPRITE_MEMBERS) {
                let d = &c.data;
                let mut p = c.node(mem).body;
                let n = i32le(d, p).clamp(0, 4096);
                p += 4;
                for _ in 0..n {
                    if p + 32 > d.len() {
                        break;
                    }
                    let place = Placement {
                        angles: [f32le(d, p + 4), f32le(d, p + 8), f32le(d, p + 12)],
                        scale: f32le(d, p + 16),
                        translation: [f32le(d, p + 20), f32le(d, p + 24), f32le(d, p + 28)],
                    };
                    match resolve(i32le(d, p)) {
                        Some((mc, s)) => add_sprite(&mut m, &mut keys, mc, mc.resolve_lod(s), |q| place.apply(q), &[], resolve, None),
                        None => m.missing += 1,
                    }
                    p += 32;
                }
            }
            add_sprite(&mut m, &mut keys, c, id, none, &[], resolve, None);
        }
        // The body: the sprite array's one member (every CSprite on the disc
        // has one, the one its skin list's slot 0 names, `m_body[0]`). A
        // skinned LOD sprite (0x2A40, `ParseSkinLODSpriteObj`) has two meshes,
        // a near one and a far one, and `VILODSprite::Raster` draws ONE of
        // them; both at once put the far mesh's flatter triangles through the
        // near one's skin (the darker wedges on a barbarian's arm and back).
        CSPRITE => {
            for &k in c.child(id, CSPRITE_ARRAY).map_or(&[][..], |a| &c.node(a).children) {
                if c.node(k).ty == CSPRITE_VARIANT {
                    add_lod_levels(&mut m, &mut keys, c, k, resolve);
                } else {
                    add_sprite(&mut m, &mut keys, c, k, none, &[], resolve, None);
                }
            }
        }
        HSPRITE => {
            for &k in c.child(id, HSPRITE_ARRAY).map_or(&[][..], |a| &c.node(a).children) {
                add_sprite(&mut m, &mut keys, c, k, none, &[], resolve, None);
            }
        }
        _ => add_sprite(&mut m, &mut keys, c, id, none, &[], resolve, None),
    }
    finish(m)
}

/// Builds terrain patches (SubSprites) into one model, each vertex offset by
/// its pre-translation group.
pub fn build_terrain<'a>(c: &'a Container, patches: &[NodeId], pre: &[[f32; 3]], resolve: &Resolve<'a>) -> Option<Model> {
    let mut m = Model::default();
    let mut keys = Vec::new();
    for &ss in patches {
        add_sprite(&mut m, &mut keys, c, ss, |p| p, pre, resolve, None);
    }
    finish(m)
}

/// A skinned LOD sprite's levels (`ParseSkinLODSpriteLevels`, 0x2A70: a count,
/// then each level's sprite DictID and distance), each level's mesh added with
/// its level. The ids name the sprites of its 0x2A60 array, found through the
/// dictionary as the parser finds them (`m_dict->Find`).
fn add_lod_levels<'a>(m: &mut Model, keys: &mut Vec<(i32, Vec<Layer>, Option<u8>)>, c: &'a Container, lod: NodeId, resolve: &Resolve<'a>) {
    let Some(t) = c.child(lod, 0x2A70) else { return };
    let d = c.body(t);
    let n = if d.len() >= 4 { i32le(d, 0) } else { 0 };
    for i in 0..n.clamp(0, 8) as usize {
        let at = 4 + i * 8;
        if at + 8 > d.len() {
            break;
        }
        m.lod_dists.push(f32le(d, at + 4));
        if let Some((sc, s)) = resolve(i32le(d, at)) {
            add_sprite(m, keys, sc, s, |p| p, &[], resolve, Some(i as u8));
        }
    }
}

fn finish(m: Model) -> Option<Model> {
    // Vertex colour is a LIGHT term, added to ambient and sun before the
    // texture multiplies (VU1 `VIRStrip`), so an all-zero colour is not black.
    (!m.pos.is_empty()).then_some(m)
}

impl Container {
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

    /// [`build`] with DictIDs resolved inside this container only.
    pub fn build_model(&self, id: NodeId) -> Option<Model> {
        build(self, id, &|d| self.lookup(d).map(|n| (self, n)))
    }

    /// Some of a zone's terrain patches ([`Container::terrain_patches`]) built
    /// into one model, offset by the zone's pre-translations, with DictIDs
    /// resolved inside this container only: a zone's own record holds every
    /// palette and texture its terrain names (all six worlds), so it builds
    /// alone, a few patches at a time (the client's map).
    pub fn build_terrain(&self, patches: &[NodeId], pre: &[[f32; 3]]) -> Option<Model> {
        build_terrain(self, patches, pre, &|d| self.lookup(d).map(|n| (self, n)))
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
