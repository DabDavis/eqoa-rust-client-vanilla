//! The renderer, on wgpu.
//!
//! Models out of the containers, textured through the dictionary, drawn as the
//! game draws them: the VU1 strip programs' lighting, one PASS PER MATERIAL
//! LAYER with the GS state `VIMaterial::ConstructTextPass` builds for it
//! (alpha test or blend mode, wrap, the UV transform and its scroll), and GPU
//! skinning as `VIRStrip4` does it.
//!
//! Two passes, as the scene draws them: a material whose first layer is
//! opaque draws every layer at once, and one whose first layer blends waits
//! for the second pass (`MaterialHasDepthSort`, `RenderDeferredList`). The
//! second pass goes in model order, where the game's goes rooms, flora, then
//! actors in the order its tree walk met them.

pub mod ao;
pub mod billboard;
pub mod curve;
pub mod fans;
pub mod fog16;
pub mod lightshadow;
pub mod lit;
pub mod map;
pub mod overlay;
pub mod recolor;
pub mod scale;
pub mod shadow;
pub mod standin;
pub mod sunshadow;
pub mod text;
pub mod ui;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

pub use lit::{Lights, MAX_LIGHTS, PointLight};
pub use wgpu;

/// Which winding is front, in esf's strip-to-list order. Settled by looking.
pub const FRONT_FACE: wgpu::FrontFace = wgpu::FrontFace::Ccw;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Colour is NOT sRGB, anywhere: the GS multiplies, adds and fogs the bytes
/// as they are (modulate is `Ct·Cf/128`) and the TV shows them as they are,
/// so the shader does the same arithmetic on the same numbers. An sRGB
/// target would do it in linear light instead, which lightens every lit or
/// fogged surface.
pub const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Palette size: enough for every character skeleton. A human has 40 nodes;
/// the most on either disc (Frontiers' and 2002's CHAR.ESF) is 68, four tree
/// bodies, the training dummies' among them, whose hands are skinned to nodes
/// 64 and 65. A vertex on a node past the palette was drawn at the model's
/// origin (the shader skips that node but keeps its weight), a thin wedge
/// from the hand down to the feet (`tests/skin_palette.rs`).
pub const MAX_BONES: usize = 72;

/// The GS's level of a texel (`VIEESurfCache::Set`'s TEX1: LCM 0, L 0,
/// MMIN 5, MXL the levels kept less one): LOD = log2(1/|Q|) + K, where 1/Q
/// is the vertex's w, its depth before the camera in world units
/// (`VIRaster::Frustum`'s projection puts z in w, and clip-to-screen keeps
/// it), blended between the two levels round it; at 0 or under, the base
/// magnified. K is `(char)(int)(lodBias * -127.0f)` in sixteenths of a
/// level: the game's default bias 0.8 is -101/16, the base out to about 80
/// units, and a bias past 1.008 wraps round to a positive K, as the char
/// does. (The products of the disc's biases are nowhere near a whole number,
/// so the host's multiply truncates as the EE's does.)
pub fn gs_lod_k(lod_bias: f32) -> f32 {
    ((lod_bias * -127.0) as i32 as i8) as f32 / 16.0
}

/// The bias a layer the game makes in code starts with (`VIMaterial`'s
/// layers: `m_lodBias = 0.8f`): the effect volumes', particles', strips'
/// and the selection rings' materials.
pub const DEFAULT_LOD_BIAS: f32 = 0.8;

/// A K that keeps every texel on the base level (the options' Mipmaps off).
pub const BASE_LEVEL_K: f32 = -16.0;

/// Whether the textures' mip levels are sampled (Options > Extended > Video
/// > Mipmaps; on, the game's).
static MIPMAPS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn set_mipmaps(on: bool) {
    MIPMAPS.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn mipmaps() -> bool {
    MIPMAPS.load(std::sync::atomic::Ordering::Relaxed)
}

/// K for a pass the game makes in code (`DEFAULT_LOD_BIAS`), or the base
/// level's with the mip levels off.
pub fn effects_lod_k() -> f32 {
    if mipmaps() { gs_lod_k(DEFAULT_LOD_BIAS) } else { BASE_LEVEL_K }
}

/// What stands in for the game's own textures (the options' Texture Packs:
/// `eqoa`'s `texpack`), asked about every surface a library's texture is
/// made from (`LibraryTextures`) while one is set (`set_pack`). With none
/// set, every texture is the surface's own, as ever.
pub trait Pack: Send + Sync {
    /// The levels to draw in place of the surface `s` (its record's bytes
    /// `c.data`, off the disc of `era`), the base first; `None` for its own.
    fn levels(&self, c: &esf::Container, s: &esf::Surface, era: esf::Era) -> Option<Vec<esf::Image>>;
    /// Whether `levels` has a stand-in for the surface `key` names (a
    /// library's `texture_key`), so that its texture is cached apart from
    /// the surface's own.
    fn replaces(&self, key: u64, c: &esf::Container, s: &esf::Surface, era: esf::Era) -> bool;
    /// What tells this pack's stand-ins apart from an earlier pack's: a
    /// replaced texture's key carries it (`LibraryTextures::key`).
    fn generation(&self) -> u64;
    /// A stand-in still being read for the surface (on the pack's own
    /// thread): its size and level count, `(w, h, n)`, as `levels` will
    /// give them, its read asked for under the texture key `key`
    /// (`take_ready` hands its levels back under it); `None` when the pack
    /// has none for the surface, or has its levels already (`levels`).
    fn request(&self, _key: u64, _c: &esf::Container, _s: &esf::Surface, _era: esf::Era) -> Option<(u32, u32, u32)> {
        None
    }
    /// The stand-ins read since the last call, by the key each was asked
    /// for under (`request`).
    fn take_ready(&self) -> standin::Ready {
        Vec::new()
    }
}

static PACK: std::sync::RwLock<Option<std::sync::Arc<dyn Pack>>> = std::sync::RwLock::new(None);

/// Sets what stands in for the game's textures, or with `None` nothing.
/// What is drawn changes as each texture is next made: the textures made
/// already stay until their models are made again (`eqoa`'s
/// `Play::art_changed` makes them again).
pub fn set_pack(p: Option<std::sync::Arc<dyn Pack>>) {
    *PACK.write().unwrap_or_else(|e| e.into_inner()) = p;
}

/// The pack set, if any.
pub fn pack() -> Option<std::sync::Arc<dyn Pack>> {
    PACK.read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// How many levels a texture's base is past the size its surface gives
/// (`TextureSource::native_size`): log2 of the larger ratio of their sides,
/// 0 for the surface's own size. A texture twice its surface's size each way
/// is sampled a level farther on at every depth (`fs_main`'s level plus it),
/// so a pack's larger texture keeps the game's texel density at a distance
/// and adds detail only near, where the game magnifies its base.
pub fn level_shift(base: (u32, u32), native: Option<(u32, u32)>) -> f32 {
    match native {
        Some((w, h)) if w > 0 && h > 0 && base.0 > 0 && base.1 > 0 => {
            let r = (base.0 as f32 / w as f32).max(base.1 as f32 / h as f32);
            if r == 1.0 { 0.0 } else { r.log2() }
        }
        _ => 0.0,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    uv: [f32; 2],
    normal: [f32; 3],
    color: [f32; 4],
    /// Skinning: four palette indices (255 unused) and their weights.
    bones: [u8; 4],
    weights: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniform {
    /// `View::view_proj` with the camera moved to the origin: the shader
    /// projects positions relative to `eye`.
    view_proj: [[f32; 4]; 4],
    /// The camera, and w = 1 when the textures' mip levels are sampled
    /// (`mipmaps`).
    eye: [f32; 4],
    sun_dir: [f32; 4],
    /// rgb, and w = the time in seconds (for UV scrolling).
    sun: [f32; 4],
    ambient: [f32; 4],
    /// rgb, and w = 1 when fog is on.
    fog_color: [f32; 4],
    /// near, far.
    fog: [f32; 4],
    /// The point light: position and range (0: none), and its colour.
    point: [f32; 4],
    point_color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LayerUniform {
    tint: [f32; 4],
    /// x: 1 textured, y: 1 blended (no alpha test).
    flags: [f32; 4],
    /// (m0, m1, m3, m4) and (m6, m7, rate u, rate v): u' = u·m0 + v·m3 + m6 +
    /// scroll, v' = u·m1 + v·m4 + m7 + scroll.
    uv0: [f32; 4],
    uv1: [f32; 4],
    /// The material's emissive colour, which the light adds to the ambient
    /// (`VIRaster::UploadPerMaterial`).
    emissive: [f32; 4],
}

/// One layer's pass over a batch.
struct Pass {
    blend: usize,
    bg: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    base: LayerUniform,
    /// the DictID bound, and the sampler (0 repeat, 1 clamp, 2 mirror)
    texture: i32,
    sampler: usize,
    /// The UV mapping the pass was made with (`uv0`, `uv1` and the sampler),
    /// which another pass of the batch can be given (`set_layer`).
    made: UvMapping,
    /// The texture's level shift (`level_shift`), in `base`'s K.
    shift: f32,
    /// The least alpha of its texture's texels, every level (255 for none):
    /// Sun Shadows draw a layer whose alpha test can let nothing through as
    /// depth alone (`Pass::solid`).
    alpha_min: u8,
}

impl Pass {
    /// Whether no texel of the layer fails the alpha test (`solid`).
    fn solid(&self) -> bool {
        solid(self.alpha_min, self.base.tint[3])
    }
}

/// Whether a layer whose texture's least alpha is `alpha_min` and whose
/// colour's alpha is `tint_a` lets every texel through fs_main's alpha test
/// (texel × tint ≥ 0.5 at every level and every filtered place between):
/// Sun Shadows draw it as depth alone, with no fragment to run.
pub(crate) fn solid(alpha_min: u8, tint_a: f32) -> bool {
    alpha_min as f32 / 255.0 * tint_a >= 0.5
}

/// A layer's UV mapping: the transform and scroll (`LayerUniform::uv0`,
/// `uv1`) and its wrap's sampler.
type UvMapping = ([f32; 4], [f32; 4], usize);

/// A layer `VIMaterial::SetNumLayers` made: an identity transform, no scroll,
/// wrapping.
const FRESH_UV: UvMapping = ([1.0, 0.0, 0.0, 1.0], [0.0; 4], 0);

/// One model on the GPU, drawn once per instance (a model matrix each).
pub struct GpuModel {
    vertices: wgpu::Buffer,
    /// The skin palette and its bind group; for an unskinned model, all
    /// identity and never written.
    palette: wgpu::Buffer,
    palette_bg: wgpu::BindGroup,
    pub skinned: bool,
    instances: wgpu::Buffer,
    /// Each instance's static light, rgb added and alpha scaling the sun
    /// term; (0, 0, 0, 1), none, unless set.
    statics: wgpu::Buffer,
    instance_count: u32,
    /// Each batch's indices and their count, its layer passes and its LOD
    /// level (`esf::Batch::lod`).
    batches: Vec<(wgpu::Buffer, u32, Vec<Pass>, Option<u8>)>,
    /// Bounds of the model itself, before any instance transform.
    pub bounds: (Vec3, Vec3),
    /// Part of the sky: drawn first, through `View::sky_view_proj`, with no
    /// depth test or write and its layers in order (`VIAtmosphere::RenderSky`;
    /// `SetResources` turns Z off on the sky's materials).
    pub sky: bool,
    /// Skipped by `draw` when false.
    pub visible: bool,
    /// How many of each batch's layer passes are drawn (`VIMaterial`'s
    /// layer count, which the sky's fade changes).
    pub passes_used: usize,
    /// The LOD level drawn of a skinned LOD body (`VILODSprite::Raster` draws
    /// one): batches of any other level are skipped, and with `None` every
    /// level is. Level 0, the near mesh, unless set.
    pub level: Option<u8>,
    /// The levels' distances (`esf::Model::lod_dists`), for whoever sets
    /// `level`; empty for a model with no levels.
    pub lod_dists: Vec<f32>,
    /// Its instances' vertex colours are a zone's baked lighting (colour
    /// buffers): Dynamic Lighting adds no fixed light on it (`lit`).
    pub baked: bool,
    /// Casts into Sun Shadows' and Light Shadows' maps while it is drawn
    /// (`sunshadow`, `lightshadow`); true unless cleared: `eqoa` clears it on
    /// a Wisp's body and everything it wears and holds (`wisp`), whose shadow
    /// would give away the human under its armour.
    pub casts: bool,
    /// The world box its instances cover as last set (`set_instances`), for
    /// Sun Shadows' casters (`sunshadow::world_box`).
    world: std::cell::Cell<Option<(Vec3, Vec3)>>,
    /// The same without a skinned model's room to move: the box its own
    /// bounds make at its instances, which Light Shadows ask whether it holds
    /// a light (`lightshadow`).
    core: std::cell::Cell<Option<(Vec3, Vec3)>>,
}

impl GpuModel {
    /// How many batches (each with its layer passes) the model has.
    pub fn batch_count(&self) -> usize {
        self.batches.len()
    }
}

/// Models uploaded together (`Renderer::upload_group`), a streamed zone's:
/// one vertex, index, instance and static-light buffer for all of them, the
/// materials shared with every other group's that are the same, and each
/// pass's commands recorded once into render bundles, validated then and
/// replayed every frame. What changes afterwards is buffer contents (the
/// props' instances, `set_group_instances`), which a bundle reads as they
/// are at the draw.
///
/// It is what wgpu's per-frame cost is counted in: a model of its own costs
/// a vertex, instance, static-light and palette buffer, an index buffer a
/// batch and a uniform buffer and bind group a layer, each tracked again at
/// every submit (Tunaria round Freeport: 1,134 models, ~14,000 resources).
///
/// The members are recorded in runs of `RUN` (in their order, so the order
/// they are drawn in is the order given) and a run wholly outside the view is
/// skipped (`Run::bounds`).
pub struct GpuGroup {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    instances: wgpu::Buffer,
    statics: wgpu::Buffer,
    members: Vec<Member>,
    runs: Vec<Run>,
    /// Its casters for Sun Shadows (`sunshadow`), recorded while they are on.
    sun: Option<sunshadow::GroupSun>,
    /// Which group it is, and what its members can cover in the world: Light
    /// Shadows' maps keep what the groups within a light's reach cast while
    /// they are the same ones (`lightshadow`).
    id: u64,
    bounds: Option<(Vec3, Vec3)>,
}

/// The next group's id (`GpuGroup::id`).
static GROUP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Members a run of a group.
const RUN: usize = 32;

/// Consecutive members of a group, recorded and drawn as one.
struct Run {
    /// What they can cover in the world: the static ones' transformed bounds,
    /// and round each moving one's instances a sphere of its size and
    /// `MOVING_SLACK` more.
    bounds: Option<(Vec3, Vec3)>,
    /// the first pass's commands and the deferred pass's, if it draws any
    bundles: [Option<wgpu::RenderBundle>; 2],
    /// models with anything in either pass, and each pass's draw calls
    drawn: (u32, [u32; 2]),
    /// The same through Dynamic Lighting's pipelines (`lit`), recorded while
    /// it is on (`Renderer::relight`); none until then.
    lit: Option<[Option<wgpu::RenderBundle>; 2]>,
    /// The same through Sun Shadows' pipelines (`sunshadow`), recorded while
    /// they are on; none until then.
    sun: Option<[Option<wgpu::RenderBundle>; 2]>,
    /// The same through Light Shadows' (`lightshadow`), alone and with the
    /// sun's map (while Sun Shadows are on too), recorded while they are on.
    points: Option<[Option<wgpu::RenderBundle>; 2]>,
    both: Option<[Option<wgpu::RenderBundle>; 2]>,
}

/// One model of a group: its first instance and their count, and each
/// batch's index range in the group's buffer with its layer passes.
struct Member {
    instance0: u32,
    instances: u32,
    batches: Vec<(std::ops::Range<u32>, Vec<Material>)>,
    /// `GroupPart::baked`
    baked: bool,
    /// What it can cover in the world (`part_bounds`), for Sun Shadows'
    /// casters, which draw the members near one another together.
    bounds: Option<(Vec3, Vec3)>,
    /// Its instances move after the upload (`GroupPart::moves`), and the box
    /// they cover as last set (`set_group_instances`); its model's own box;
    /// and, for one that does not move, each instance's box: Light Shadows'
    /// casters, culled instance by instance (`lightshadow`).
    moves: bool,
    posed: std::cell::Cell<Option<(Vec3, Vec3)>>,
    own: Option<(Vec3, Vec3)>,
    boxes: Vec<(Vec3, Vec3)>,
}

/// A layer pass whose bind group (texture, sampler, uniform) every layer
/// with the same three shares.
#[derive(Clone)]
struct Material {
    blend: usize,
    bg: wgpu::BindGroup,
    /// No texel fails its alpha test (`solid`).
    solid: bool,
}

/// A model as `upload_group` takes it: the mesh, a matrix per instance
/// (column-major) and their static lights (fewer: (0, 0, 0, 1) for the rest).
pub struct GroupPart<'a> {
    pub model: &'a esf::Model,
    pub instances: &'a [[[f32; 4]; 4]],
    pub statics: &'a [[f32; 4]],
    /// Its instances move after the upload (`set_group_instances`: a zone's
    /// animated props), so its bounds are taken loosely.
    pub moves: bool,
    /// Its vertex colours are baked lighting (`GpuModel::baked`).
    pub baked: bool,
}

/// How far past its own size a moving part's bounds reach from where its
/// instances were at the upload (a prop's parts swing about their nodes).
const MOVING_SLACK: f32 = 50.0;

/// A model's own box, its vertices' least and most.
fn model_box(m: &esf::Model) -> Option<(Vec3, Vec3)> {
    let (first, rest) = m.pos.split_first()?;
    Some(rest.iter().fold((Vec3::from(*first), Vec3::from(*first)), |(lo, hi), q| (lo.min(Vec3::from(*q)), hi.max(Vec3::from(*q)))))
}

/// The world box the box `b` makes at each of `instances` (column-major).
fn instance_boxes(b: Option<(Vec3, Vec3)>, instances: &[[[f32; 4]; 4]]) -> Vec<(Vec3, Vec3)> {
    let Some(b) = b else { return Vec::new() };
    instances.iter().map(|m| sunshadow::world_box(b, false, std::slice::from_ref(m)).unwrap_or(b)).collect()
}

/// The world-space box a part's instances cover (`GpuGroup::bounds`).
fn part_bounds(p: &GroupPart) -> Option<(Vec3, Vec3)> {
    let (first, rest) = p.model.pos.split_first()?;
    let (lo, hi) = rest.iter().fold((Vec3::from(*first), Vec3::from(*first)), |(lo, hi), q| (lo.min(Vec3::from(*q)), hi.max(Vec3::from(*q))));
    let mut out: Option<(Vec3, Vec3)> = None;
    for m in p.instances {
        let m = Mat4::from_cols_array_2d(m);
        let (a, b) = if p.moves {
            let scale = m.x_axis.truncate().length().max(m.y_axis.truncate().length()).max(m.z_axis.truncate().length());
            let r = scale * lo.abs().max(hi.abs()).length() + MOVING_SLACK;
            let c = m.w_axis.truncate();
            (c - Vec3::splat(r), c + Vec3::splat(r))
        } else {
            (0..8).map(|k| m.transform_point3(Vec3::new([lo.x, hi.x][k & 1], [lo.y, hi.y][k >> 1 & 1], [lo.z, hi.z][k >> 2]))).fold(
                (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                |(a, b), w| (a.min(w), b.max(w)),
            )
        };
        out = Some(out.map_or((a, b), |(lo, hi)| (lo.min(a), hi.max(b))));
    }
    out
}

/// The planes a box must be inside (left, right, bottom, top, far) for any of
/// it to be drawn through `view_proj`, as (normal, distance) with the inside
/// positive; the near plane is left out (it would save little).
fn frustum(view_proj: Mat4) -> [glam::Vec4; 5] {
    let (r0, r1, r2, r3) = (view_proj.row(0), view_proj.row(1), view_proj.row(2), view_proj.row(3));
    [r3 + r0, r3 - r0, r3 + r1, r3 - r1, r3 - r2]
}

/// Whether the box `(lo, hi)` may show through the planes: outside one of
/// them wholly, it cannot.
fn in_view(planes: &[glam::Vec4; 5], (lo, hi): (Vec3, Vec3)) -> bool {
    planes.iter().all(|p| {
        let far = Vec3::new(if p.x >= 0.0 { hi.x } else { lo.x }, if p.y >= 0.0 { hi.y } else { lo.y }, if p.z >= 0.0 { hi.z } else { lo.z });
        p.truncate().dot(far) + p.w >= 0.0
    })
}

/// What the world is drawn through (`Renderer::pipeline_of`): the PS2's
/// pipelines, Dynamic Lighting's (`lit`), Sun Shadows' (`sunshadow`), or
/// Light Shadows' (`lightshadow`; with the sun's map too when `true`).
#[derive(Clone, Copy)]
enum Shade<'a> {
    Ps2,
    Lit(&'a lit::Lit),
    Sun(&'a sunshadow::Sun),
    Points(&'a lightshadow::Receivers, bool),
}

impl<'a> Shade<'a> {
    /// The group 3 its pipelines take (the lights, and the maps), if any.
    fn group3(self) -> Option<&'a wgpu::BindGroup> {
        match self {
            Shade::Ps2 => None,
            Shade::Lit(l) => Some(&l.bg),
            Shade::Sun(s) => Some(&s.bg),
            Shade::Points(p, _) => Some(&p.bg),
        }
    }
}

/// What `draw` draws, in order.
pub enum Draw<'a> {
    Model(&'a GpuModel),
    Group(&'a GpuGroup),
    /// Some of a model's instances, in their order (a run of the radial
    /// flora's plants, drawn farthest first: `Renderer::upload_flora`).
    Instances(&'a GpuModel, std::ops::Range<u32>),
}

/// A texture source: the decoded image a DictID names, if any, and what the
/// renderer's cache keys it by.
pub trait TextureSource {
    fn image(&self, id: i32) -> Option<esf::Image>;
    /// The image's mip levels, the base first (`esf::Surface::decode_levels`:
    /// those the game keeps, which the GS samples); the base alone for a
    /// source that has no others.
    fn levels(&self, id: i32) -> Vec<esf::Image> {
        self.image(id).into_iter().collect()
    }
    /// What tells this source's image for `id` apart from another image a
    /// source names by the same id: equal keys are one texture. A plain
    /// function's is the id.
    fn key(&self, id: i32) -> u64 {
        id as u32 as u64
    }
    /// The size the game's own texture for `id` has (its surface's), which
    /// `levels` may hand out larger (a pack's: `level_shift`); `None` when
    /// the levels are always the source's own.
    fn native_size(&self, _id: i32) -> Option<(u32, u32)> {
        None
    }
    /// A pack's stand-in for `id` still being read (`Pack::request`, asked
    /// under the texture key `key`): its size and level count, which its
    /// texture is made at at once, the surface's own levels drawn into it
    /// (`standin::placeholder`) until it is read (`Renderer::take_stand_ins`).
    /// `None`: `levels` are the texture's now.
    fn pending(&self, _id: i32, _key: u64) -> Option<(u32, u32, u32)> {
        None
    }
}

impl<F: Fn(i32) -> Option<esf::Image>> TextureSource for F {
    fn image(&self, id: i32) -> Option<esf::Image> {
        self(id)
    }
}

/// What every upload takes its textures from.
pub type Textures<'a> = dyn TextureSource + 'a;

pub struct Renderer {
    /// Indexed by blend mode: 0 opaque, 1 add, 2 subtract, 3 alpha, 4 multiply.
    pipelines: Vec<wgpu::RenderPipeline>,
    /// The same with Z test and write off, for the sky.
    sky_pipelines: Vec<wgpu::RenderPipeline>,
    frame_buf: wgpu::Buffer,
    frame_bg: wgpu::BindGroup,
    sky_frame_buf: wgpu::Buffer,
    sky_frame_bg: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    palette_layout: wgpu::BindGroupLayout,
    /// The frame uniform's layout, which Dynamic Lighting's pipelines share.
    frame_layout: wgpu::BindGroupLayout,
    /// Dynamic Lighting's pipelines and lights (`lit`), made when it is first
    /// turned on, and whether it is on.
    lit: Option<lit::Lit>,
    lit_on: bool,
    /// Sun Shadows (`sunshadow`): the map and its pipelines, made when they
    /// are first on with Dynamic Lighting, and whether they are on.
    sun: Option<sunshadow::Sun>,
    sun_on: bool,
    /// Light Shadows (`lightshadow`): the maps and their pipelines, made when
    /// they are first on with Dynamic Lighting, and whether they are on.
    points: Option<lightshadow::Points>,
    points_on: bool,
    /// The options' Shadow Distance, times the game's (1 to 4,
    /// `set_shadow_distance`): the sun's cascades' far edge, and how many
    /// lights cast.
    shadow_scale: f32,
    /// repeat, clamp, mirror.
    samplers: [wgpu::Sampler; 3],
    white: wgpu::TextureView,
    /// A source's key for a DictID (`TextureSource::key`) -> uploaded texture
    /// and its level shift (`level_shift`; `None`: the id resolved to nothing).
    textures: std::collections::HashMap<u64, Option<(wgpu::TextureView, f32)>>,
    /// Each uploaded texture's least texel alpha over its levels, by the same
    /// key (`Pass::alpha_min`).
    texture_alpha: std::collections::HashMap<u64, u8>,
    /// The textures of stand-ins being read, by key, with the size and level
    /// count each was made at (`standin`), and the stand-ins read and not
    /// yet written into them, in order (`take_stand_ins`).
    stand_ins: std::collections::HashMap<u64, (wgpu::Texture, (u32, u32, u32))>,
    stand_ins_ready: std::collections::VecDeque<(u64, std::sync::Arc<Vec<esf::Image>>)>,
    /// What draws a placeholder's levels, made with the first.
    blit: Option<standin::Blit>,
    /// DictIDs a batch asked for that no loaded file holds.
    pub missing_textures: std::collections::BTreeSet<i32>,
    start: web_time::Instant,
    /// The time the layers' UVs scroll by, in seconds: the wall clock since
    /// the renderer was made, unless set (`shot` sets its script's time, so a
    /// shot is the same picture every run).
    pub scroll_time: Option<f32>,
    format: wgpu::TextureFormat,
    /// Labels (name plates), once a font is set.
    pub text: Option<text::Text>,
    /// The 2D layer (the HUD), drawn over everything.
    pub ui: Option<ui::Ui>,
    /// The particles' pipelines, made at the first particle.
    billboards: Option<billboard::Billboards>,
    /// The immediate-mode fans' (`set_fans`), made at the first fan.
    fans: Option<fans::Fans>,
    /// Whether a fan run marked `ps2` is drawn as the PS2's 16-bit frame
    /// keeps it (`fog16`): only where the adapter can add up its sums
    /// (`set_ps2_sums`, `fog16::supported`); off until told.
    ps2_sums: bool,
    /// The PS2 runs drawn a fan at a time, the GS's arithmetic exactly
    /// (`set_ps2_sequential`): the reference the sums are measured against.
    ps2_sequential: bool,
    /// Whether the world's particles of blend mode 3 are blended as the
    /// PS2's 16-bit frame keeps them (`set_ps2_particles`, `billboard`), as
    /// the fans marked `ps2` are; off until told.
    ps2_particles: bool,
    /// The copy and the sums the PS2's blends are run over (`fog16`), the
    /// fans' and the particles' in turn: one set, made at the first.
    fog16: Option<fog16::Fog16>,
    /// The characters' blob shadows' (`set_shadows`), made at the first.
    shadows: Option<shadow::Shadows>,
    /// What is drawn over the scene with Z off (`set_overlay`), made at the
    /// first.
    overlay: Option<overlay::Overlay>,
    /// Strips drawn after the particles and the shadows, in the order given:
    /// the spell effects' lightning (`VISpellEffectSystem::Process`), then the
    /// weapon trails (`VITrailFxRender`), as `VIScene::Render` draws them after
    /// `VIParticleSystem::Render` and `RenderShadows` and before the weather
    /// (`draw_billboards_and_shadows`, `draw_queued_strips`).
    pub strips: Vec<billboard::Strips>,
    /// How many of the batches `draw_billboards_and_shadows` is handed are
    /// the particle system's: the shadows and the strips go after them, the
    /// rest (the weather's) after those. Set for each frame.
    pub particles_end: usize,
    /// The last `draw`'s models and draw calls (`EQOA_PERF`).
    pub drawn: std::cell::Cell<(u32, u32)>,
    /// Groups' materials by texture (its key), sampler and uniform bytes.
    materials: std::collections::HashMap<(u64, usize, [u32; 20]), wgpu::BindGroup>,
    /// The palette every group's models draw with: theirs is never set.
    identity_palette: wgpu::BindGroup,
    /// The options' Render Scale (`scale`): 1 until set.
    pub scale: scale::Scale,
    /// The options' Brightness over the scene (`curve`): none until set.
    pub curve: curve::Curve,
    /// The options' Ambient Occlusion (`ao`): off until set.
    pub ao: ao::Ao,
}

/// What to draw this frame. The lights are the VU1 strip programs' uniforms:
/// the direction the sun's light TRAVELS, its colour, and ambient.
pub struct View {
    pub view_proj: Mat4,
    /// Where the camera is, in the world: what the models are drawn relative
    /// to (`draw`). The origin is right for a view near it.
    pub eye: Vec3,
    pub sun_dir: Vec3,
    pub sun: Vec3,
    pub ambient: Vec3,
    /// `VIRaster::SetFog`: linear in view depth from `near` (none) to `far`
    /// (all fog colour), as the strip programs' per-vertex fog value and the
    /// GS's FOGCOL mix.
    pub fog: Option<(f32, f32, Vec3)>,
    /// The point light (`VIRaster::SetPointLight` 0, the personal light):
    /// position, range, colour.
    pub point: Option<(Vec3, f32, Vec3)>,
    /// What the frame clears to: the sky colour (under the dome, if drawn).
    pub clear: Vec3,
    /// The view with its translation zeroed, for `GpuModel::sky` models.
    pub sky_view_proj: Option<Mat4>,
    /// Dynamic Lighting's point lights (`lit`): with them (and it on), the
    /// models drawn lit per fragment; `None`, the PS2's way.
    pub lights: Option<Lights>,
}

impl View {
    /// Fixed daylight, no fog: for looking at models on their own.
    pub fn daylight(view_proj: Mat4) -> Self {
        View {
            view_proj,
            eye: Vec3::ZERO,
            sun_dir: Vec3::new(0.35, -0.8, 0.45),
            sun: Vec3::splat(0.6),
            ambient: Vec3::splat(0.45),
            fog: None,
            point: None,
            clear: Vec3::new(0.18, 0.2, 0.24),
            sky_view_proj: None,
            lights: None,
        }
    }
}

/// The GS ALPHA register of each blend mode, as a wgpu blend:
/// 1 `Cs·As + Cd`, 2 `Cd − Cs·As`, 3 `(Cs − Cd)·As + Cd`, 4 `Cd·As`.
pub(crate) fn blend_state(mode: usize) -> Option<wgpu::BlendState> {
    use wgpu::{BlendComponent as C, BlendFactor as Fa, BlendOperation as Op};
    let color = match mode {
        1 => C { src_factor: Fa::SrcAlpha, dst_factor: Fa::One, operation: Op::Add },
        2 => C { src_factor: Fa::SrcAlpha, dst_factor: Fa::One, operation: Op::ReverseSubtract },
        3 => C { src_factor: Fa::SrcAlpha, dst_factor: Fa::OneMinusSrcAlpha, operation: Op::Add },
        4 => C { src_factor: Fa::Zero, dst_factor: Fa::SrcAlpha, operation: Op::Add },
        _ => return None,
    };
    Some(wgpu::BlendState { color, alpha: C { src_factor: Fa::Zero, dst_factor: Fa::One, operation: Op::Add } })
}

/// A model layer's pipeline for blend `mode` (`blend_state`; the sky's with
/// no depth test or write), through `module`'s entry points `entries`
/// (vertex, fragment): the PS2's (shader.wgsl's `vs_main`, `fs_main`) and
/// Dynamic Lighting's (`lit`) are made alike, over the same vertices.
pub(crate) fn model_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    entries: (&str, &str),
    mode: usize,
    sky: bool,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let blend = blend_state(mode);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("layer"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(entries.0),
            compilation_options: Default::default(),
            buffers: &[
                Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3, 3 => Float32x4, 8 => Uint8x4, 9 => Uint8x4],
                }),
                Some(wgpu::VertexBufferLayout {
                    array_stride: 64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4],
                }),
                // the instance's static light (VIRaster::SetStaticLighting)
                Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![10 => Float32x4],
                }),
            ],
        },
        // Strips are lists with the winding made consistent; the
        // strip programs cull back faces (ADC), so this does too.
        primitive: wgpu::PrimitiveState {
            front_face: FRONT_FACE,
            cull_mode: Some(wgpu::Face::Back),
            ..Default::default()
        },
        // Opaque: depth tested and written. Blended: TEST's
        // AFAIL = RGB_ONLY with a NEVER alpha test, i.e. colour
        // only, no depth write.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(blend.is_none() && !sky),
            depth_compare: Some(if sky { wgpu::CompareFunction::Always } else { wgpu::CompareFunction::LessEqual }),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(entries.1),
            compilation_options: Default::default(),
            // Colour only, opaque or not: nothing reads the
            // target's alpha (no GS ALPHA setting the game uses
            // takes the destination's), and it must stay the
            // clear's 1. A page's WebGL2 canvas is composited
            // with it (wgpu makes the context with alpha), and
            // a picture of it came out with the colour
            // of every alpha-tested edge wrapped round: a cyan
            // fringe where red exceeded alpha.
            targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::COLOR })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("layer"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                uniform_entry(2, wgpu::ShaderStages::VERTEX_FRAGMENT),
            ],
        });
        let palette_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("palette"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model"),
            bind_group_layouts: &[Some(&frame_layout), Some(&material_layout), Some(&palette_layout)],
            immediate_size: 0,
        });
        let make = |mode: usize, sky: bool| model_pipeline(device, &layout, &shader, ("vs_main", "fs_main"), mode, sky, format);
        let pipelines = (0..5).map(|m| make(m, false)).collect();
        let sky_pipelines = (0..5).map(|m| make(m, true)).collect();
        let frame_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: frame_buf.as_entire_binding() }],
        });
        let sky_frame_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky frame"),
            size: std::mem::size_of::<FrameUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sky_frame_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: sky_frame_buf.as_entire_binding() }],
        });
        // TEX1's filters (`VIEESurfCache::Set`): MMAG linear, MMIN 5
        // (LINEAR_MIPMAP_LINEAR), the level the shader's (`gs_lod_k`)
        let sampler = |mode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: mode,
                address_mode_v: mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            })
        };
        let samplers = [
            sampler(wgpu::AddressMode::Repeat),
            sampler(wgpu::AddressMode::ClampToEdge),
            sampler(wgpu::AddressMode::MirrorRepeat),
        ];
        let white = upload_rgba(device, queue, 1, 1, &[255; 4]);
        let identity = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("identity palette"),
            contents: bytemuck::cast_slice(&[IDENTITY; MAX_BONES]),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let identity_palette = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("identity palette"),
            layout: &palette_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: identity.as_entire_binding() }],
        });
        Renderer {
            pipelines,
            sky_pipelines,
            frame_buf,
            frame_bg,
            sky_frame_buf,
            sky_frame_bg,
            material_layout,
            palette_layout,
            frame_layout,
            lit: None,
            lit_on: false,
            sun: None,
            sun_on: false,
            points: None,
            points_on: false,
            shadow_scale: 1.0,
            samplers,
            white,
            textures: Default::default(),
            texture_alpha: Default::default(),
            stand_ins: Default::default(),
            stand_ins_ready: Default::default(),
            blit: None,
            missing_textures: Default::default(),
            start: web_time::Instant::now(),
            scroll_time: None,
            format,
            text: None,
            ui: None,
            billboards: None,
            fans: None,
            ps2_sums: false,
            ps2_sequential: false,
            ps2_particles: false,
            fog16: None,
            shadows: None,
            overlay: None,
            strips: Vec::new(),
            particles_end: 0,
            drawn: Default::default(),
            materials: Default::default(),
            identity_palette,
            scale: scale::Scale::new(format),
            curve: curve::Curve::new(format),
            ao: ao::Ao::new(format),
        }
    }

    fn texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, id: i32, src: &Textures) -> Option<wgpu::TextureView> {
        self.texture_shifted(device, queue, id, src).map(|(v, _)| v)
    }

    /// `texture`, with how many levels its base is past its surface's
    /// (`level_shift`: 0 but for a pack's larger texture).
    fn texture_shifted(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, id: i32, src: &Textures) -> Option<(wgpu::TextureView, f32)> {
        if id == 0 {
            return None;
        }
        let key = src.key(id);
        if !self.textures.contains_key(&key) && self.blit.is_none() && pack().is_some() {
            self.blit = Some(standin::Blit::new(device));
        }
        let missing = &mut self.missing_textures;
        let alpha = &mut self.texture_alpha;
        let (stand_ins, blit) = (&mut self.stand_ins, &self.blit);
        self.textures
            .entry(key)
            .or_insert_with(|| {
                // a pack's stand-in still being read: its texture made at its
                // size now, the surface's own drawn into it, written over
                // once read (`standin`); asked first, so that a stand-in
                // read meanwhile is still written in
                let pending = src.pending(id, key);
                let levels = src.levels(id);
                if levels.is_empty() {
                    missing.insert(id);
                    return None;
                }
                if let (Some(size), Some(blit)) = (pending, blit) {
                    let (tex, view) = standin::placeholder(device, queue, blit, &levels, size);
                    // its texels unknown until read: never drawn as one no
                    // texel of which fails its alpha test (Sun Shadows)
                    alpha.insert(key, 0);
                    stand_ins.insert(key, (tex, size));
                    return Some((view, level_shift((size.0, size.1), src.native_size(id))));
                }
                alpha.insert(key, levels.iter().flat_map(|l| l.rgba.chunks_exact(4).map(|p| p[3])).min().unwrap_or(255));
                let shift = level_shift((levels[0].width, levels[0].height), src.native_size(id));
                Some((upload_levels(device, queue, &levels), shift))
            })
            .clone()
    }

    /// The stand-ins a pack has read since (`Pack::take_ready`) written over
    /// their textures' placeholders (`standin`), as many as `standin::BUDGET`
    /// bytes allow (one at least), the rest left for the next frame: called
    /// once a frame, before it is drawn. How many were written.
    pub fn take_stand_ins(&mut self, queue: &wgpu::Queue) -> usize {
        if let Some(p) = pack() {
            self.stand_ins_ready.extend(p.take_ready());
        }
        let sizes: Vec<usize> = self.stand_ins_ready.iter().map(|(_, l)| standin::bytes(l)).collect();
        let n = standin::due(&sizes, standin::BUDGET);
        for (key, levels) in self.stand_ins_ready.drain(..n) {
            if let Some((tex, size)) = self.stand_ins.remove(&key)
                && standin::fits(&levels, size)
            {
                standin::write(queue, &tex, &levels);
            }
        }
        n
    }

    /// Stand-ins read and not yet written (`take_stand_ins`).
    pub fn stand_ins_waiting(&self) -> usize {
        self.stand_ins_ready.len()
    }

    /// The least texel alpha of the texture `id` names in `src` (255 for none
    /// or one not uploaded).
    fn alpha_min(&self, id: i32, src: &Textures) -> u8 {
        if id == 0 { 255 } else { self.texture_alpha.get(&src.key(id)).copied().unwrap_or(255) }
    }

    /// Uploads a model, drawn once per matrix in `instances` (column-major),
    /// with its textures out of `textures` (cached by DictID).
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        m: &esf::Model,
        instances: &[[[f32; 4]; 4]],
        textures: &Textures,
    ) -> GpuModel {
        self.upload_with(device, queue, m, instances, textures, false)
    }

    /// Uploads a radial flora plant (a flora sprite's model) with room for
    /// `capacity` instances, lit as `VIRStrip6` lights it: no normal and no
    /// vertex colour, the ambient plus the sun and the point light's falloff
    /// each times 0.15, the falloff's distance in the plant's own space; the
    /// vertex alpha is the plant's fade, an instance's static-light alpha
    /// (`set_static_lights`). Draw its instances with `Draw::Instances`.
    pub fn upload_flora(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, m: &esf::Model, capacity: usize, textures: &Textures) -> GpuModel {
        self.upload_with(device, queue, m, &vec![IDENTITY; capacity.max(1)], textures, true)
    }

    fn upload_with(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        m: &esf::Model,
        instances: &[[[f32; 4]; 4]],
        textures: &Textures,
        flora: bool,
    ) -> GpuModel {
        let verts = vertices_of(m);
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let identity = [IDENTITY; MAX_BONES];
        let palette = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("palette"),
            contents: bytemuck::cast_slice(&identity),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let palette_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("palette"),
            layout: &self.palette_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: palette.as_entire_binding() }],
        });
        let mut batches = Vec::new();
        for b in &m.batches {
            if b.indices.is_empty() {
                continue;
            }
            let mut passes = Vec::new();
            for l in layers_of(b) {
                let (view, shift) = self.texture_shifted(device, queue, l.texture, textures).unzip();
                let (blend, mut base, si) = layer_state(l, view.is_some());
                base.flags[2] += shift.unwrap_or(0.0);
                base.flags[3] = flora as i32 as f32;
                let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("layer"),
                    contents: bytemuck::bytes_of(&base),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                let sampler = &self.samplers[si];
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("layer"),
                    layout: &self.material_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view.as_ref().unwrap_or(&self.white)),
                        },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                        wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
                    ],
                });
                let alpha_min = if view.is_some() { self.alpha_min(l.texture, textures) } else { 255 };
                passes.push(Pass { blend, bg, uniform, base, texture: l.texture, sampler: si, made: (base.uv0, base.uv1, si), shift: shift.unwrap_or(0.0), alpha_min });
            }
            let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: bytemuck::cast_slice(&b.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            batches.push((indices, b.indices.len() as u32, passes, b.lod));
        }
        let lo = m.pos.iter().fold(Vec3::splat(f32::MAX), |a, p| a.min(Vec3::from(*p)));
        let hi = m.pos.iter().fold(Vec3::splat(f32::MIN), |a, p| a.max(Vec3::from(*p)));
        let instance_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("instances"),
            contents: bytemuck::cast_slice(instances),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let statics = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("static lights"),
            contents: bytemuck::cast_slice(&vec![[0.0f32, 0.0, 0.0, 1.0]; instances.len().max(1)]),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        GpuModel {
            vertices,
            palette,
            palette_bg,
            skinned: m.skinned,
            instances: instance_buf,
            statics,
            instance_count: instances.len() as u32,
            batches,
            bounds: (lo, hi),
            sky: false,
            visible: true,
            passes_used: usize::MAX,
            level: Some(0),
            lod_dists: m.lod_dists.clone(),
            baked: false,
            casts: true,
            world: std::cell::Cell::new(sunshadow::world_box((lo, hi), m.skinned, instances)),
            core: std::cell::Cell::new(sunshadow::world_box((lo, hi), false, instances)),
        }
    }

    /// Lets go of every texture uploaded so far (the models made keep
    /// theirs while they live): for a renderer that draws one thing after
    /// another and keeps nothing from one to the next (the map's tiles).
    pub fn forget_textures(&mut self) {
        self.textures.clear();
        self.missing_textures.clear();
        self.materials.clear();
    }

    /// Re-points one layer pass of one batch, as `VIAtmosphere` changes the
    /// sky's materials: its texture (rebound when it changes), blend mode and
    /// colour (0..1, alpha included).
    #[allow(clippy::too_many_arguments)]
    /// A layer pass as a material has it this frame: its texture, blend mode
    /// and colour (`rgba`), and its UV mapping, the one pass `uv` of the
    /// batch was made with (`VIMaterial::CopyLayer` carries a layer's
    /// transform, wrap and scroll with it), or with `None` a fresh layer's
    /// (`SetNumLayers`: identity, no scroll). The scroll's phase is the
    /// mapping's own, rate × time, as a copied offset keeps it.
    #[allow(clippy::too_many_arguments)]
    pub fn set_layer(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, m: &mut GpuModel, batch: usize, pass: usize, texture: i32, blend: usize, rgba: [f32; 4], uv: Option<usize>, textures: &Textures) {
        let Some(ps) = m.batches.get(batch).map(|b| &b.2) else { return };
        let Some(p) = ps.get(pass) else { return };
        let (uv0, uv1, si) = uv.and_then(|u| ps.get(u)).map_or(FRESH_UV, |src| src.made);
        let rebind = p.texture != texture || p.sampler != si;
        let (view, shift) = if rebind { self.texture_shifted(device, queue, texture, textures).unzip() } else { (None, None) };
        let alpha_min = if view.is_some() { self.alpha_min(texture, textures) } else { 255 };
        let Some(p) = m.batches.get_mut(batch).and_then(|b| b.2.get_mut(pass)) else { return };
        if rebind {
            let shift = shift.unwrap_or(0.0);
            p.base.flags[2] += shift - p.shift;
            p.shift = shift;
            p.bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("layer"),
                layout: &self.material_layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view.as_ref().unwrap_or(&self.white)) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.samplers[si]) },
                    wgpu::BindGroupEntry { binding: 2, resource: p.uniform.as_entire_binding() },
                ],
            });
            p.texture = texture;
            p.sampler = si;
            p.base.flags[0] = view.is_some() as i32 as f32;
            p.alpha_min = alpha_min;
        }
        p.blend = blend.min(4);
        p.base.flags[1] = (p.blend != 0) as i32 as f32;
        p.base.tint = rgba;
        (p.base.uv0, p.base.uv1) = (uv0, uv1);
        queue.write_buffer(&p.uniform, 0, bytemuck::bytes_of(&p.base));
    }

    /// Sets every layer's colour to `rgb` (0..1), keeping each layer's own
    /// alpha (`VIAtmosphere::SetSkyColor`).
    pub fn set_layer_rgb(&self, queue: &wgpu::Queue, m: &GpuModel, rgb: Vec3) {
        for (_, _, passes, _) in &m.batches {
            for p in passes {
                let mut u = p.base;
                u.tint = [rgb.x, rgb.y, rgb.z, p.base.tint[3]];
                queue.write_buffer(&p.uniform, 0, bytemuck::bytes_of(&u));
            }
        }
    }

    /// The font labels are drawn in.
    pub fn set_font(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, font: &esf::font::Font) {
        self.text = Some(text::Text::new(device, queue, self.format, DEPTH_FORMAT, font));
    }

    /// The name plates (`text.plates`), into the scene at their anchors'
    /// depths, depth-tested against what `draw` drew and not writing it, as
    /// the font's material draws them (`VIRFont`: Z test on, alpha blended,
    /// so its TEST keeps colour only). `VIScene::Render` draws them after
    /// the world and before the particles, the shadows, the spell lightning,
    /// the trails and the weather, which blend over them: call it after
    /// `draw_fans` and before `draw_billboards`.
    pub fn draw_plates(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32)) {
        let Some(t) = self.text.as_ref().filter(|t| !t.plates.is_empty()) else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("plates"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        t.draw_plates(device, &mut pass, size);
    }

    /// The labels, over what `draw` drew (depth-tested against it, not
    /// writing it), then the overlay (`set_overlay`), then the 2D layer.
    /// `size` is the target's, in pixels.
    pub fn draw_text(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32)) {
        self.text_pass(device, encoder, target, depth, size, true, true);
    }

    /// `draw_text` in two passes with `between` drawn between them: the
    /// labels and the overlay, then `between`'s (a piece of the HUD another
    /// layer draws, the minimap: over the names, under the HUD and its
    /// windows), then the 2D layer's quads.
    pub fn draw_text_around(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32), between: impl FnOnce(&mut wgpu::CommandEncoder)) {
        self.text_pass(device, encoder, target, depth, size, true, false);
        between(encoder);
        self.text_pass(device, encoder, target, depth, size, false, true);
    }

    /// The text pass: the labels and the overlay (`over`), the 2D layer's
    /// quads (`hud`).
    #[allow(clippy::too_many_arguments)]
    fn text_pass(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32), over: bool, hud: bool) {
        let labels = self.text.as_ref().filter(|t| over && !t.labels.is_empty());
        let overlay = self.overlay.as_ref().filter(|o| over && !o.is_empty());
        let quads = self.ui.as_ref().filter(|u| hud && !u.quads.is_empty());
        if labels.is_none() && overlay.is_none() && quads.is_none() {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("text"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some(t) = labels {
            t.draw(device, &mut pass, size);
        }
        if let Some(o) = overlay {
            o.draw(&mut pass);
        }
        if let Some(u) = quads {
            u.draw(device, &mut pass, size);
        }
    }

    /// The skin palette for a skinned model: one matrix per skeleton node
    /// (`VIHSprite::SetBlendMatrices`), as column-major arrays.
    pub fn set_palette(&self, queue: &wgpu::Queue, m: &GpuModel, palette: &[[[f32; 4]; 4]]) {
        let n = palette.len().min(MAX_BONES);
        queue.write_buffer(&m.palette, 0, bytemuck::cast_slice(&palette[..n]));
    }

    /// Sets a model's instances' static lights (as many as it has).
    pub fn set_static_lights(&self, queue: &wgpu::Queue, m: &GpuModel, lights: &[[f32; 4]]) {
        let n = lights.len().min(m.instance_count as usize);
        queue.write_buffer(&m.statics, 0, bytemuck::cast_slice(&lights[..n]));
    }

    /// Moves a model's instances (as many as it was uploaded with).
    pub fn set_instances(&self, queue: &wgpu::Queue, m: &GpuModel, instances: &[[[f32; 4]; 4]]) {
        let n = instances.len().min(m.instance_count as usize);
        queue.write_buffer(&m.instances, 0, bytemuck::cast_slice(&instances[..n]));
        m.world.set(sunshadow::world_box(m.bounds, m.skinned, &instances[..n]));
        m.core.set(sunshadow::world_box(m.bounds, false, &instances[..n]));
    }

    pub fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        view: &View,
        items: &[Draw],
    ) {
        self.draw_in(queue, encoder, target, depth, view, items, None);
    }

    /// `draw` into the rectangle `rect` of the target (x, y, width, height in
    /// its pixels), as `VIRaster::Viewport` puts a window's 3D into part of
    /// the screen: what the target holds is kept (no clear), depth is cleared,
    /// the view's projection fills the rectangle and nothing is drawn outside it.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_viewport(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, view: &View, items: &[Draw], rect: [u32; 4]) {
        self.draw_in(queue, encoder, target, depth, view, items, Some(rect));
    }

    /// The 2D layer's quads in `range` alone, over what the target holds
    /// (a window's quads under its 3D, then over it: `ui::Ui::split`).
    pub fn draw_ui(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32), range: std::ops::Range<usize>) {
        let Some(u) = self.ui.as_ref().filter(|u| !u.quads.is_empty()) else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui part"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        u.draw_range(device, &mut pass, size, range);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_in(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, view: &View, items: &[Draw], rect: Option<[u32; 4]>) {
        // The view-projection relative to the eye, in doubles: `view_proj`'s
        // translation is the projection of the eye's world position, which
        // the eye's own translation here cancels (shader.wgsl, vs_main).
        let rel = view.view_proj.as_dmat4() * glam::DMat4::from_translation(view.eye.as_dvec3());
        let uniform = FrameUniform {
            view_proj: rel.as_mat4().to_cols_array_2d(),
            eye: view.eye.extend(mipmaps() as i32 as f32).to_array(),
            sun_dir: view.sun_dir.normalize().extend(0.0).to_array(),
            sun: view.sun.extend(self.scroll_time.unwrap_or_else(|| self.start.elapsed().as_secs_f32())).to_array(),
            ambient: view.ambient.extend(0.0).to_array(),
            fog_color: match view.fog {
                Some((_, _, c)) => c.extend(1.0).to_array(),
                None => [0.0; 4],
            },
            fog: match view.fog {
                Some((n, f, _)) => [n, f.max(n + 1e-3), 0.0, 0.0],
                None => [0.0, 1.0, 0.0, 0.0],
            },
            point: match view.point {
                Some((p, r, _)) => p.extend(r).to_array(),
                None => [0.0; 4],
            },
            point_color: match view.point {
                Some((_, _, c)) => c.extend(1.0).to_array(),
                None => [0.0; 4],
            },
        };
        queue.write_buffer(&self.frame_buf, 0, bytemuck::bytes_of(&uniform));
        if let Some(vp) = view.sky_view_proj {
            // the sky's view has no translation: its eye is the origin
            let sky = FrameUniform { view_proj: vp.to_cols_array_2d(), eye: [0.0, 0.0, 0.0, uniform.eye[3]], ..uniform };
            queue.write_buffer(&self.sky_frame_buf, 0, bytemuck::bytes_of(&sky));
        }
        // Sun Shadows: the frame's casters into their map before the world,
        // and the world drawn through their pipelines while the sun casts
        // (`sunshadow`)
        let sun = match (view.lights.as_ref(), self.sun_drawn()) {
            (Some(_), Some((_, s))) if s.draw(self, queue, encoder, view, items) => Some(s),
            _ => None,
        };
        // Light Shadows: the frame's lights' maps, and the world drawn
        // through their receivers (with the sun's map too while Sun Shadows
        // are on, whose sun term is the strip programs' alone in a frame the
        // sun casts nothing: by night)
        let points = match (view.lights.as_ref(), self.points_drawn()) {
            (Some(_), Some(p)) => {
                p.draw(self, queue, encoder, view, items, self.scroll_time.unwrap_or_else(|| self.start.elapsed().as_secs_f32()));
                if sun.is_none()
                    && let Some((_, s)) = self.sun_drawn()
                {
                    s.unshadowed(queue);
                }
                Some(p)
            }
            _ => None,
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("models"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: match rect {
                        Some(_) => wgpu::LoadOp::Load,
                        None => wgpu::LoadOp::Clear(wgpu::Color { r: view.clear.x as f64, g: view.clear.y as f64, b: view.clear.z as f64, a: 1.0 }),
                    },
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if let Some([x, y, w, h]) = rect {
            pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
            pass.set_scissor_rect(x, y, w, h);
        }
        // The sky: its own view, no depth, every layer in order.
        if view.sky_view_proj.is_some() {
            pass.set_bind_group(0, &self.sky_frame_bg, &[]);
            let sky = items.iter().filter_map(|i| match i {
                Draw::Model(m) => Some(*m),
                Draw::Group(_) | Draw::Instances(..) => None,
            });
            for m in sky.filter(|m| m.sky && m.visible && m.instance_count > 0) {
                pass.set_vertex_buffer(0, m.vertices.slice(..));
                pass.set_vertex_buffer(1, m.instances.slice(..));
                pass.set_vertex_buffer(2, m.statics.slice(..));
                pass.set_bind_group(2, &m.palette_bg, &[]);
                for (indices, count, passes, _) in &m.batches {
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                    for p in passes.iter().take(m.passes_used) {
                        pass.set_pipeline(&self.sky_pipelines[p.blend]);
                        pass.set_bind_group(1, &p.bg, &[]);
                        pass.draw_indexed(0..*count, 0, 0..m.instance_count);
                    }
                }
            }
        }
        // Dynamic Lighting: this view's lights, its models through the lit
        // pipelines and its groups' runs through their lit bundles (`lit`)
        let lit = match (view.lights.as_ref(), self.lit.as_ref()) {
            (Some(l), Some(lit)) if self.lit_on => {
                queue.write_buffer(&lit.buf, 0, bytemuck::bytes_of(&lit::LightsUniform::new(view.point, l, view.eye, view.view_proj)));
                Some(lit)
            }
            _ => None,
        };
        let shade = match (lit, sun, points) {
            (None, _, _) => Shade::Ps2,
            (Some(_), _, Some(p)) => match (self.sun_drawn(), p.with_sun.as_ref()) {
                (Some(_), Some(w)) => Shade::Points(w, true),
                _ => Shade::Points(&p.receivers, false),
            },
            (Some(_), Some(s), None) => Shade::Sun(s),
            (Some(l), None, None) => Shade::Lit(l),
        };
        let planes = frustum(view.view_proj);
        let shown = |r: &&Run| r.bounds.is_some_and(|b| in_view(&planes, b));
        let (mut drawn_models, mut draws) = (0u32, 0u32);
        for item in items {
            match item {
                Draw::Model(m) => drawn_models += Self::draws_anything(m) as u32,
                Draw::Instances(m, r) => drawn_models += (Self::draws_anything(m) && !r.is_empty()) as u32,
                Draw::Group(g) => drawn_models += g.runs.iter().filter(shown).map(|r| r.drawn.0).sum::<u32>(),
            }
        }
        // Two passes, as VIScene::Render's: a material whose layer 0 is
        // opaque (MaterialHasDepthSort) draws all its layers at once in the
        // first, one whose layer 0 blends is deferred whole to the second
        // (DrawPrimBuffer's flags; RenderDeferredList). The Ambient Occlusion
        // goes between them, over what the first wrote, in passes of its own
        // (`ao`: the frame `prepare` readied it for, never part of one).
        let ao = rect.is_none() && self.ao.take(depth);
        for deferred in [false, true] {
            if deferred && ao {
                drop(pass);
                self.ao.run(queue, encoder, target, rel, view.fog);
                pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("models, blended"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: depth,
                        depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
            }
            // the pass's state, unset again by each bundle it executes
            let mut bound = false;
            let mut current = usize::MAX;
            for item in items {
                match item {
                    Draw::Model(m) => {
                        if !Self::draws_anything(m) {
                            continue;
                        }
                        if !bound {
                            pass.set_bind_group(0, &self.frame_bg, &[]);
                            if let Some(bg) = shade.group3() {
                                pass.set_bind_group(3, bg, &[]);
                            }
                            bound = true;
                            current = usize::MAX;
                        }
                        draws += self.encode_model(&mut pass, m, deferred, &mut current, 0..m.instance_count, shade);
                    }
                    Draw::Instances(m, r) => {
                        if !Self::draws_anything(m) || r.is_empty() {
                            continue;
                        }
                        if !bound {
                            pass.set_bind_group(0, &self.frame_bg, &[]);
                            if let Some(bg) = shade.group3() {
                                pass.set_bind_group(3, bg, &[]);
                            }
                            bound = true;
                            current = usize::MAX;
                        }
                        draws += self.encode_model(&mut pass, m, deferred, &mut current, r.start..r.end.min(m.instance_count), shade);
                    }
                    Draw::Group(g) => {
                        for r in g.runs.iter().filter(shown) {
                            let earlier = match (lit, sun, &r.sun, &r.lit) {
                                (Some(_), Some(_), Some(b), _) => b,
                                (Some(_), _, _, Some(b)) => b,
                                _ => &r.bundles,
                            };
                            // Light Shadows' where they are recorded (`relight`)
                            let bundles = match shade {
                                Shade::Points(_, true) => r.both.as_ref().unwrap_or(earlier),
                                Shade::Points(_, false) => r.points.as_ref().unwrap_or(earlier),
                                _ => earlier,
                            };
                            if let Some(b) = &bundles[deferred as usize] {
                                pass.execute_bundles([b]);
                                bound = false;
                            }
                            draws += r.drawn.1[deferred as usize];
                        }
                    }
                }
            }
        }
        self.drawn.set((drawn_models, draws));
    }

    /// Whether `draw` draws `m` at all (the sky draws apart).
    fn draws_anything(m: &GpuModel) -> bool {
        m.instance_count > 0 && !m.sky && m.visible
    }

    /// One model's commands for one of the two passes, into a pass or a
    /// bundle; `current` is the pipeline set (`pipeline_of`'s key), through
    /// `shade`'s pipelines. Returns the draw calls.
    fn encode_model<'a, E: wgpu::util::RenderEncoder<'a>>(&'a self, e: &mut E, m: &'a GpuModel, deferred: bool, current: &mut usize, range: std::ops::Range<u32>, shade: Shade<'a>) -> u32 {
        let in_pass = |passes: &[Pass], lod: Option<u8>| passes.first().is_some_and(|p| (p.blend != 0) == deferred) && level_drawn(lod, m.level);
        if !m.batches.iter().any(|(_, _, passes, lod)| in_pass(passes, *lod)) {
            return 0;
        }
        e.set_vertex_buffer(0, Some(m.vertices.slice(..)));
        e.set_vertex_buffer(1, Some(m.instances.slice(..)));
        e.set_vertex_buffer(2, Some(m.statics.slice(..)));
        e.set_bind_group(2, Some(&m.palette_bg), &[]);
        let mut draws = 0;
        for (indices, count, passes, lod) in &m.batches {
            if !in_pass(passes, *lod) {
                continue;
            }
            e.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            for p in passes.iter().take(m.passes_used) {
                let (key, pipeline) = self.pipeline_of(p.blend, m.baked, shade);
                if key != *current {
                    e.set_pipeline(pipeline);
                    *current = key;
                }
                e.set_bind_group(1, Some(&p.bg), &[]);
                e.draw_indexed(0..*count, 0, range.clone());
                draws += 1;
            }
        }
        draws
    }

    /// The shared bind group of a layer (cached by texture, sampler and
    /// uniform), and its blend mode.
    fn material(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, l: &esf::Layer, textures: &Textures) -> Material {
        let (view, shift) = self.texture_shifted(device, queue, l.texture, textures).unzip();
        let (blend, mut base, si) = layer_state(l, view.is_some());
        base.flags[2] += shift.unwrap_or(0.0);
        let solid = solid(if view.is_some() { self.alpha_min(l.texture, textures) } else { 255 }, base.tint[3]);
        let key = (textures.key(l.texture), si, bytemuck::cast(base));
        if let Some(bg) = self.materials.get(&key) {
            return Material { blend, bg: bg.clone(), solid };
        }
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("material"),
            contents: bytemuck::bytes_of(&base),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("material"),
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view.as_ref().unwrap_or(&self.white)) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.samplers[si]) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
            ],
        });
        self.materials.insert(key, bg.clone());
        Material { blend, bg, solid }
    }

    /// Uploads `parts` as one [`GpuGroup`], drawn as `draw` would draw them
    /// one by one as models, in this order.
    pub fn upload_group(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, parts: &[GroupPart], textures: &Textures) -> GpuGroup {
        // The buffers are sized first and each part written as it is built: a
        // whole zone's vertices at once (and the copies made of them on the
        // way to the GPU) would stay in a page's wasm heap, which never shrinks.
        let (mut nverts, mut nindices, mut ninstances) = (0, 0, 0);
        for p in parts {
            nverts += p.model.pos.len();
            nindices += p.model.batches.iter().filter(|b| group_drawn(b)).map(|b| b.indices.len()).sum::<usize>();
            ninstances += p.instances.len();
        }
        // a buffer of no bytes cannot be made; an empty group draws nothing
        let buffer = |label: &str, bytes: usize, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes.max(16) as u64,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let vertices = buffer("group vertices", nverts * size_of::<Vertex>(), wgpu::BufferUsages::VERTEX);
        let indices = buffer("group indices", nindices * 4, wgpu::BufferUsages::INDEX);
        let instances = buffer("group instances", ninstances * 64, wgpu::BufferUsages::VERTEX);
        let statics = buffer("group static lights", ninstances * 16, wgpu::BufferUsages::VERTEX);
        let mut members = Vec::new();
        let (mut base, mut first_index, mut instance0) = (0u32, 0u32, 0u32);
        for p in parts {
            let verts = vertices_of(p.model);
            let mut part_indices: Vec<u32> = Vec::new();
            let mut batches = Vec::new();
            for b in p.model.batches.iter().filter(|b| !b.indices.is_empty() && group_drawn(b)) {
                let first = first_index + part_indices.len() as u32;
                // rebased here: WebGL2 has no base vertex
                part_indices.extend(b.indices.iter().map(|&i| i + base));
                let passes = layers_of(b).iter().map(|l| self.material(device, queue, l, textures)).collect();
                batches.push((first..first_index + part_indices.len() as u32, passes));
            }
            let lights: Vec<[f32; 4]> = (0..p.instances.len()).map(|k| p.statics.get(k).copied().unwrap_or([0.0, 0.0, 0.0, 1.0])).collect();
            let write = |to: &wgpu::Buffer, at: u32, stride: usize, bytes: &[u8]| {
                if !bytes.is_empty() {
                    queue.write_buffer(to, at as u64 * stride as u64, bytes);
                }
            };
            write(&vertices, base, size_of::<Vertex>(), bytemuck::cast_slice(&verts));
            write(&indices, first_index, 4, bytemuck::cast_slice(&part_indices));
            write(&instances, instance0, 64, bytemuck::cast_slice(p.instances));
            write(&statics, instance0, 16, bytemuck::cast_slice(&lights));
            let own = model_box(p.model);
            members.push(Member {
                instance0,
                instances: p.instances.len() as u32,
                batches,
                baked: p.baked,
                bounds: part_bounds(p),
                moves: p.moves,
                posed: std::cell::Cell::new(if p.moves { own.and_then(|b| sunshadow::world_box(b, false, p.instances)) } else { None }),
                own,
                boxes: if p.moves { Vec::new() } else { instance_boxes(own, p.instances) },
            });
            base += verts.len() as u32;
            first_index += part_indices.len() as u32;
            instance0 += p.instances.len() as u32;
        }
        let bounds = members.iter().filter_map(|m| m.bounds).reduce(|(a, b), (c, d)| (a.min(c), b.max(d)));
        let id = GROUP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut g = GpuGroup { vertices, indices, instances, statics, members, runs: Vec::new(), sun: None, id, bounds };
        for at in (0..parts.len()).step_by(RUN) {
            let span = at..(at + RUN).min(parts.len());
            let bounds = parts[span.clone()].iter().filter_map(part_bounds).reduce(|(a, b), (c, d)| (a.min(c), b.max(d)));
            let drawn = g.members[span.clone()].iter().filter(|m| m.instances > 0 && !m.batches.is_empty()).count() as u32;
            let (bundles, n) = self.record(device, &g, span.clone(), Shade::Ps2);
            // and through Dynamic Lighting's pipelines while it is on
            let lit = self.lit.as_ref().filter(|_| self.lit_on).map(|l| self.record(device, &g, span.clone(), Shade::Lit(l)).0);
            // and Sun Shadows' while they are on
            let sun = self.sun_drawn().map(|(_, s)| self.record(device, &g, span.clone(), Shade::Sun(s)).0);
            // and Light Shadows' while they are on, with the sun's map while
            // Sun Shadows are too
            let [points, both] = self.points_shades().map(|sh| sh.map(|sh| self.record(device, &g, span.clone(), sh).0));
            g.runs.push(Run { bounds, bundles, drawn: (drawn, n), lit, sun, points, both });
        }
        // and its casters
        g.sun = self.sun_drawn().map(|(_, s)| s.casters(self, device, &g));
        g
    }

    /// The bundles of a group's members `span` for the two passes (none for
    /// a pass they draw nothing in), and each pass's draw calls, through
    /// `shade`'s pipelines.
    fn record(&self, device: &wgpu::Device, g: &GpuGroup, span: std::ops::Range<usize>, shade: Shade) -> ([Option<wgpu::RenderBundle>; 2], [u32; 2]) {
        let (mut bundles, mut drawn) = ([None, None], [0, 0]);
        for deferred in [false, true] {
            let mut e = device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                label: Some("group"),
                color_formats: &[Some(self.format)],
                depth_stencil: Some(wgpu::RenderBundleDepthStencil { format: DEPTH_FORMAT, depth_read_only: false, stencil_read_only: true }),
                sample_count: 1,
                multiview: None,
            });
            let n = self.encode_run(&mut e, g, &g.members[span.clone()], deferred, shade);
            if n > 0 {
                bundles[deferred as usize] = Some(e.finish(&wgpu::RenderBundleDescriptor { label: Some("group") }));
            }
            drawn[deferred as usize] = n;
        }
        (bundles, drawn)
    }

    /// Turns Dynamic Lighting on or off (`lit`): on, its pipelines made the
    /// first time, a view's lights drawn, and a group uploaded from now on
    /// recorded for it as well; the groups uploaded before need `relight`.
    pub fn set_dynamic_lighting(&mut self, device: &wgpu::Device, on: bool) {
        if on && self.lit.is_none() {
            self.lit = Some(lit::Lit::new(device, [&self.frame_layout, &self.material_layout, &self.palette_layout], self.format));
        }
        self.lit_on = on;
        self.make_sun(device);
        self.make_points(device);
    }

    /// Turns Sun Shadows on or off (`sunshadow`): drawn only while Dynamic
    /// Lighting is on too, their map and pipelines made the first time both
    /// are, and a group uploaded from then on recorded for them; the groups
    /// uploaded before need `relight`.
    pub fn set_sun_shadows(&mut self, device: &wgpu::Device, on: bool) {
        self.sun_on = on;
        self.make_sun(device);
        self.make_points(device);
    }

    /// Turns Light Shadows on or off (`lightshadow`): drawn only while
    /// Dynamic Lighting is on too, their maps and pipelines made the first
    /// time both are, and a group uploaded from then on recorded for them;
    /// the groups uploaded before need `relight`.
    pub fn set_light_shadows(&mut self, device: &wgpu::Device, on: bool) {
        self.points_on = on;
        self.make_points(device);
    }

    /// The options' Shadow Distance, `scale` times the game's (1 to 4, as
    /// built at 1): Sun Shadows' far edge (`sunshadow::ends`) and how many
    /// lights Light Shadows shadow (`lightshadow::slots_at`), from the next
    /// frame.
    pub fn set_shadow_distance(&mut self, scale: f32) {
        self.shadow_scale = scale.clamp(1.0, 4.0);
    }

    /// Whether Light Shadows are on (`set_light_shadows`; drawn only with
    /// Dynamic Lighting on).
    pub fn light_shadows(&self) -> bool {
        self.points_on
    }

    /// What the last frame drew into Light Shadows' maps; `None` before they
    /// were first on.
    pub fn light_shadow_drawn(&self) -> Option<lightshadow::Drawn> {
        self.points.as_ref().map(|p| p.drawn.get())
    }

    /// The maps and their pipelines, the first time Light Shadows and
    /// Dynamic Lighting are both on; and their receivers with the sun's map
    /// once Sun Shadows' are made.
    fn make_points(&mut self, device: &wgpu::Device) {
        let layouts = [&self.frame_layout, &self.material_layout, &self.palette_layout];
        if self.points_on
            && self.points.is_none()
            && let Some(l) = self.lit.as_ref().filter(|_| self.lit_on)
        {
            self.points = lightshadow::Points::new(device, layouts, l, self.sun.as_ref(), self.format);
        }
        if let (Some(p), Some(l), Some(s)) = (self.points.as_mut(), self.lit.as_ref(), self.sun.as_ref()) {
            p.attach_sun(device, layouts, l, s, self.format);
        }
    }

    /// Light Shadows' while they and Dynamic Lighting are on.
    fn points_drawn(&self) -> Option<&lightshadow::Points> {
        self.lit.as_ref().filter(|_| self.lit_on)?;
        self.points.as_ref().filter(|_| self.points_on)
    }

    /// The shades a group records for Light Shadows while they are on: alone
    /// while Sun Shadows are off, with the sun's map while they are on.
    fn points_shades(&self) -> [Option<Shade<'_>>; 2] {
        let Some(p) = self.points_drawn() else { return [None, None] };
        match (self.sun_drawn(), p.with_sun.as_ref()) {
            (Some(_), Some(w)) => [None, Some(Shade::Points(w, true))],
            _ => [Some(Shade::Points(&p.receivers, false)), None],
        }
    }

    /// Whether Sun Shadows are on (`set_sun_shadows`; drawn only with
    /// Dynamic Lighting on).
    pub fn sun_shadows(&self) -> bool {
        self.sun_on
    }

    /// What the last frame drew into Sun Shadows' cascades, all four: the
    /// casters' bundles executed and the draw calls; `None` before they were
    /// first on.
    pub fn sun_shadow_drawn(&self) -> Option<(u32, u32)> {
        self.sun.as_ref().map(|s| s.drawn.get())
    }

    /// The sun's map and pipelines, the first time Sun Shadows and Dynamic
    /// Lighting are both on.
    fn make_sun(&mut self, device: &wgpu::Device) {
        if self.sun_on
            && self.sun.is_none()
            && let Some(l) = self.lit.as_ref().filter(|_| self.lit_on)
        {
            self.sun = sunshadow::Sun::new(device, [&self.frame_layout, &self.material_layout, &self.palette_layout], l, self.format);
        }
    }

    /// Dynamic Lighting's and Sun Shadows' while both are on.
    fn sun_drawn(&self) -> Option<(&lit::Lit, &sunshadow::Sun)> {
        Some((self.lit.as_ref().filter(|_| self.lit_on)?, self.sun.as_ref().filter(|_| self.sun_on)?))
    }

    /// Whether Dynamic Lighting is on (`set_dynamic_lighting`).
    pub fn dynamic_lighting(&self) -> bool {
        self.lit_on
    }

    /// Records a group's lit bundles where it has none, while Dynamic
    /// Lighting is on (a group uploaded while it was off draws the PS2's way
    /// until then).
    pub fn relight(&self, device: &wgpu::Device, g: &mut GpuGroup) {
        let Some(l) = self.lit.as_ref().filter(|_| self.lit_on) else { return };
        let sun = self.sun.as_ref().filter(|_| self.sun_on);
        let [points, both] = self.points_shades();
        for k in 0..g.runs.len() {
            let span = k * RUN..((k + 1) * RUN).min(g.members.len());
            if g.runs[k].lit.is_none() {
                let (bundles, _) = self.record(device, g, span.clone(), Shade::Lit(l));
                g.runs[k].lit = Some(bundles);
            }
            // and Sun Shadows' while they are on
            if let Some(s) = sun.filter(|_| g.runs[k].sun.is_none()) {
                g.runs[k].sun = Some(self.record(device, g, span.clone(), Shade::Sun(s)).0);
            }
            // and Light Shadows' while they are on
            if let Some(sh) = points.filter(|_| g.runs[k].points.is_none()) {
                g.runs[k].points = Some(self.record(device, g, span.clone(), sh).0);
            }
            if let Some(sh) = both.filter(|_| g.runs[k].both.is_none()) {
                g.runs[k].both = Some(self.record(device, g, span, sh).0);
            }
        }
        if let Some(s) = sun.filter(|_| g.sun.is_none()) {
            g.sun = Some(s.casters(self, device, g));
        }
    }

    /// The pipeline a layer pass of blend `blend` is drawn through, and its
    /// key for a run of passes (a pipeline set once while the key stays):
    /// the PS2's, or Dynamic Lighting's, a baked instance's own, Sun Shadows'
    /// or Light Shadows' (`shade`).
    fn pipeline_of<'a>(&'a self, blend: usize, baked: bool, shade: Shade<'a>) -> (usize, &'a wgpu::RenderPipeline) {
        match shade {
            Shade::Ps2 => (blend, &self.pipelines[blend]),
            Shade::Sun(s) => (32 + blend + 8 * baked as usize, &s.pipelines[baked as usize][blend]),
            Shade::Lit(l) => (8 + blend + 8 * baked as usize, &l.pipelines[baked as usize][blend]),
            Shade::Points(p, sun) => (64 + 32 * sun as usize + blend + 8 * baked as usize, &p.pipelines[baked as usize][blend]),
        }
    }

    /// The commands of some of a group's members for one of the two passes,
    /// as `encode_model` issues a model's, through `shade`'s pipelines.
    /// Returns the draw calls.
    fn encode_run<'a>(&'a self, e: &mut wgpu::RenderBundleEncoder<'a>, g: &'a GpuGroup, members: &'a [Member], deferred: bool, shade: Shade<'a>) -> u32 {
        let in_pass = |passes: &[Material]| passes.first().is_some_and(|p| (p.blend != 0) == deferred);
        e.set_bind_group(0, &self.frame_bg, &[]);
        e.set_bind_group(2, &self.identity_palette, &[]);
        if let Some(bg) = shade.group3() {
            e.set_bind_group(3, bg, &[]);
        }
        e.set_vertex_buffer(0, g.vertices.slice(..));
        e.set_index_buffer(g.indices.slice(..), wgpu::IndexFormat::Uint32);
        let mut current = usize::MAX;
        let mut draws = 0;
        for m in members.iter().filter(|m| m.instances > 0) {
            if !m.batches.iter().any(|(_, passes)| in_pass(passes)) {
                continue;
            }
            let (i0, n) = (m.instance0 as u64, m.instances as u64);
            e.set_vertex_buffer(1, g.instances.slice(i0 * 64..(i0 + n) * 64));
            e.set_vertex_buffer(2, g.statics.slice(i0 * 16..(i0 + n) * 16));
            for (range, passes) in &m.batches {
                if !in_pass(passes) {
                    continue;
                }
                for p in passes {
                    let (key, pipeline) = self.pipeline_of(p.blend, m.baked, shade);
                    if key != current {
                        e.set_pipeline(pipeline);
                        current = key;
                    }
                    e.set_bind_group(1, &p.bg, &[]);
                    e.draw_indexed(range.clone(), 0, 0..m.instances);
                    draws += 1;
                }
            }
        }
        draws
    }

    /// Moves the instances of a group's `member`-th model (as many as it was
    /// uploaded with).
    pub fn set_group_instances(&self, queue: &wgpu::Queue, g: &GpuGroup, member: usize, instances: &[[[f32; 4]; 4]]) {
        let Some(m) = g.members.get(member) else { return };
        let n = instances.len().min(m.instances as usize);
        queue.write_buffer(&g.instances, m.instance0 as u64 * 64, bytemuck::cast_slice(&instances[..n]));
        // where it is now, for Light Shadows' casters (an animated prop)
        if m.moves {
            m.posed.set(m.own.and_then(|b| sunshadow::world_box(b, false, &instances[..n])));
        }
    }
}

/// Whether a batch of LOD level `lod` (`esf::Batch::lod`) is drawn while its
/// model's level is `level` (`GpuModel::level`): a batch in no level always,
/// one in a level only while that level is the model's.
fn level_drawn(lod: Option<u8>, level: Option<u8>) -> bool {
    lod.is_none() || lod == level
}

/// Whether a group draws a batch: a group's members are never moved closer or
/// further, so of a skinned LOD body only the near level, 0.
fn group_drawn(b: &esf::Batch) -> bool {
    b.lod.is_none_or(|l| l == 0)
}

/// The vertices of a model as the shader takes them.
fn vertices_of(m: &esf::Model) -> Vec<Vertex> {
    (0..m.pos.len())
        .map(|i| Vertex {
            pos: m.pos[i],
            uv: m.uv[i],
            normal: m.normal[i],
            color: m.color[i],
            bones: m.bones.get(i).copied().unwrap_or([255; 4]),
            weights: m.weights.get(i).copied().unwrap_or([0; 4]),
        })
        .collect()
}

/// A material with no layer at all draws once, untextured.
const UNTEXTURED: [esf::Layer; 1] = [esf::Layer {
    flags: 1,
    texture: 0,
    wrap: 0,
    blend: 0,
    color: [255; 4],
    uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    lod_bias: 0.8,
    uv_rate: [0.0, 0.0],
    emissive: [0; 4],
}];

fn layers_of(b: &esf::Batch) -> &[esf::Layer] {
    if b.layers.is_empty() { &UNTEXTURED } else { &b.layers }
}

/// A layer's blend mode, uniform and sampler (0 repeat, 1 clamp, 2 mirror),
/// `textured` if its texture resolved.
fn layer_state(l: &esf::Layer, textured: bool) -> (usize, LayerUniform, usize) {
    let blend = (l.blend.clamp(0, 4)) as usize;
    let k = l.uv_coeffs();
    // The layer colour reaches the GS as c/255 × 128 and modulates by /128:
    // a tint of c/255.
    let tint = l.color.map(|c| c as f32 / 255.0);
    let base = LayerUniform {
        tint,
        flags: [textured as i32 as f32, (blend != 0) as i32 as f32, gs_lod_k(l.lod_bias), 0.0],
        uv0: [k[0], k[1], k[2], k[3]],
        uv1: [k[4], k[5], l.uv_rate[0], l.uv_rate[1]],
        // as UploadPerMaterial converts it: c/255
        emissive: l.emissive.map(|c| c as f32 / 255.0),
    };
    let si = match l.wrap {
        1 | 2 => 1,
        3 => 2,
        _ => 0,
    };
    (blend, base, si)
}

pub(crate) fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

/// A texture of `levels` (the base first, each half the one before), as
/// the surface cache puts a surface's levels in the GS's memory.
pub(crate) fn upload_levels(device: &wgpu::Device, queue: &wgpu::Queue, levels: &[esf::Image]) -> wgpu::TextureView {
    let base = &levels[0];
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("surface"),
        size: wgpu::Extent3d { width: base.width, height: base.height, depth_or_array_layers: 1 },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (k, l) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: k as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &l.rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(l.width * 4), rows_per_image: Some(l.height) },
            wgpu::Extent3d { width: l.width, height: l.height, depth_or_array_layers: 1 },
        );
    }
    tex.create_view(&Default::default())
}

fn upload_rgba(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32, rgba: &[u8]) -> wgpu::TextureView {
    let size = wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("surface"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        tex.as_image_copy(),
        rgba,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
        size,
    );
    tex.create_view(&Default::default())
}

/// A depth buffer the size of the target.
pub fn depth_view(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            // read by the Ambient Occlusion's passes (`ao`)
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

impl Renderer {
    /// Before `curve.scene_target`, for a frame drawn into `color`: where the
    /// fans set read the frame they are drawn over (`fans_read_frame`, the
    /// ground fog a PS2 frame keeps) and `color` cannot be copied (a page's
    /// canvas, a GL window), the scene goes through the curve's texture,
    /// which can (`curve::Curve::set_through`); otherwise as before.
    pub fn read_frame_through(&mut self, color: &wgpu::TextureView) {
        let through = self.fans_read_frame() && !color.texture().usage().contains(wgpu::TextureUsages::COPY_SRC);
        self.curve.set_through(through);
    }
}

/// Renders one frame offscreen and returns it as RGBA8 rows (for `--shot`),
/// with the particles' billboards (view, projection, batches) if any.
#[allow(clippy::too_many_arguments)]
pub fn capture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    w: u32,
    h: u32,
    view: &View,
    items: &[Draw],
    particles: Option<(Mat4, Mat4, &[billboard::Batch], &Textures)>,
) -> Vec<u8> {
    capture_with_sky(device, queue, renderer, w, h, view, items, particles, None)
}

/// `capture`, with the sky's billboards (the stars, the lightning patches)
/// drawn behind the world through the sky's view, and the fans `set_fans`
/// set after them.
#[allow(clippy::too_many_arguments)]
pub fn capture_with_sky(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    w: u32,
    h: u32,
    view: &View,
    items: &[Draw],
    particles: Option<(Mat4, Mat4, &[billboard::Batch], &Textures)>,
    sky: Option<(Mat4, Mat4, &[billboard::Batch], &Textures)>,
) -> Vec<u8> {
    capture_around(device, queue, renderer, w, h, view, items, particles, sky, |_, _, _, _| {})
}

/// `capture_with_sky` with `between` drawn between the labels and the 2D
/// layer, as `Renderer::draw_text_around` draws it (the minimap): given the
/// encoder, the target and its depth, and the target's size (the Render
/// Scale's, `w` x `h` times it).
#[allow(clippy::too_many_arguments)]
pub fn capture_around(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    w: u32,
    h: u32,
    view: &View,
    items: &[Draw],
    particles: Option<(Mat4, Mat4, &[billboard::Batch], &Textures)>,
    sky: Option<(Mat4, Mat4, &[billboard::Batch], &Textures)>,
    between: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView, &wgpu::TextureView, (u32, u32)),
) -> Vec<u8> {
    let format = TARGET_FORMAT;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("capture"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let row = (w * 4).div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (row * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let depth = depth_view(device, w, h);
    let mut enc = device.create_command_encoder(&Default::default());
    let tv = target.create_view(&Default::default());
    // the frame at the Render Scale's size (the target's own at 1), the scene
    // into the Brightness curve's texture when one is set
    let f = renderer.scale.targets(device, &tv, &depth, (w, h));
    renderer.read_frame_through(&f.color);
    let scene = renderer.curve.scene_target(device, f.size);
    let st = scene.as_ref().unwrap_or(&f.color);
    // the Ambient Occlusion's passes over its opaque half, while set (`ao`)
    renderer.ao.prepare(device, &f);
    renderer.draw(queue, &mut enc, st, &f.depth, view, items);
    if let Some((v, p, batches, textures)) = sky {
        renderer.draw_sky_billboards(device, queue, &mut enc, st, &f.depth, v, p, batches, textures);
    }
    renderer.draw_fans(device, queue, &mut enc, st, &f.depth, view);
    renderer.draw_plates(device, &mut enc, st, &f.depth, f.size);
    if let Some((v, p, batches, textures)) = particles {
        renderer.draw_billboards_and_shadows(device, queue, &mut enc, st, &f.depth, v, p, batches, textures, view);
    }
    renderer.curve.apply(queue, &mut enc, &f.color);
    renderer.draw_text_around(device, &mut enc, &f.color, &f.depth, f.size, |enc| between(enc, &f.color, &f.depth, f.size));
    renderer.scale.resolve(&mut enc, &tv);
    enc.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    readback.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
    device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
    let data = readback.slice(..).get_mapped_range().expect("mapped readback");
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        out.extend_from_slice(&data[y * row as usize..y * row as usize + w as usize * 4]);
    }
    out
}

/// One instance, untransformed.
pub const IDENTITY: [[f32; 4]; 4] = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];

/// Textures out of a library: the surface a DictID names in the library's
/// view, wherever it was registered, decoded; keyed by the id and the disc
/// its record came off (`esf::Library::texture_key`), so a 2002 texture and
/// its Frontiers namesake are two textures, and an unchanged one is one.
pub struct LibraryTextures<'a>(pub &'a esf::Library);

impl LibraryTextures<'_> {
    /// The surface a DictID names in the library's view, its container, and
    /// the disc its record came off.
    fn surface(&self, id: i32) -> Option<(&esf::Container, esf::Surface, esf::Era)> {
        let r = self.0.lookup(id)?;
        let (c, s) = self.0.surface(id)?;
        Some((c, s, self.0.era_of(r.file)))
    }
}

/// With a pack set (`set_pack`), a surface it has a stand-in for is drawn
/// as the pack's levels, keyed apart (its generation in bits 40 and up);
/// every other texture, and every texture with none set, is the surface's
/// own under its own key.
///
/// A recoloured texture's id ([`recolor::recolored`]: a body's part a player
/// recoloured) is its source's levels, recoloured, under a key of its own; a
/// texture's own id resolves as it always did.
impl TextureSource for LibraryTextures<'_> {
    fn image(&self, id: i32) -> Option<esf::Image> {
        self.levels(id).into_iter().next()
    }
    fn levels(&self, id: i32) -> Vec<esf::Image> {
        let Some((c, s, era)) = self.surface(id) else {
            return match recolor::source_of(id) {
                Some((source, r)) => {
                    let mut levels = self.levels(source);
                    for l in &mut levels {
                        recolor::recolor_rgba(&mut l.rgba, r);
                    }
                    levels
                }
                None => Vec::new(),
            };
        };
        if let Some(l) = pack().and_then(|p| p.levels(c, &s, era)).filter(|l| !l.is_empty()) {
            return l;
        }
        s.decode_levels(&c.data)
    }
    fn key(&self, id: i32) -> u64 {
        if self.surface(id).is_none()
            && let Some((source, r)) = recolor::source_of(id)
        {
            return recolor::key_of(self.key(source), r);
        }
        let key = self.0.texture_key(id);
        let Some(p) = pack() else { return key };
        match self.surface(id) {
            Some((c, s, era)) if p.replaces(key, c, &s, era) => key | p.generation() << 40,
            _ => key,
        }
    }
    fn native_size(&self, id: i32) -> Option<(u32, u32)> {
        if let Some((_, s)) = self.0.surface(id) {
            return Some((s.width.max(0) as u32, s.height.max(0) as u32));
        }
        recolor::source_of(id).and_then(|(source, _)| self.native_size(source))
    }
    fn pending(&self, id: i32, key: u64) -> Option<(u32, u32, u32)> {
        let (c, s, era) = self.surface(id)?;
        pack()?.request(key, c, &s, era)
    }
}

pub fn library_textures(lib: &esf::Library) -> LibraryTextures<'_> {
    LibraryTextures(lib)
}

#[cfg(test)]
mod tests {
    use super::level_drawn;

    /// One level of an LOD body at a time, none past the last; a batch in no
    /// level whatever the model's level. The control: drawing every batch
    /// (the old rule) draws the far level with the near one.
    #[test]
    fn one_level_at_a_time() {
        for level in [None, Some(0), Some(1)] {
            assert!(level_drawn(None, level));
        }
        assert!(level_drawn(Some(0), Some(0)) && !level_drawn(Some(1), Some(0)));
        assert!(level_drawn(Some(1), Some(1)) && !level_drawn(Some(0), Some(1)));
        assert!(!level_drawn(Some(0), None) && !level_drawn(Some(1), None));
        let every = |_: Option<u8>, _: Option<u8>| true;
        assert_ne!(every(Some(1), Some(0)), level_drawn(Some(1), Some(0)));
    }
}
