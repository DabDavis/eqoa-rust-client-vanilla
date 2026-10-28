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

pub mod billboard;
pub mod curve;
pub mod fans;
pub mod overlay;
pub mod scale;
pub mod shadow;
pub mod text;
pub mod ui;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

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
}

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
}

/// One model of a group: its first instance and their count, and each
/// batch's index range in the group's buffer with its layer passes.
struct Member {
    instance0: u32,
    instances: u32,
    batches: Vec<(std::ops::Range<u32>, Vec<Material>)>,
}

/// A layer pass whose bind group (texture, sampler, uniform) every layer
/// with the same three shares.
#[derive(Clone)]
struct Material {
    blend: usize,
    bg: wgpu::BindGroup,
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
}

/// How far past its own size a moving part's bounds reach from where its
/// instances were at the upload (a prop's parts swing about their nodes).
const MOVING_SLACK: f32 = 50.0;

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
    /// repeat, clamp, mirror.
    samplers: [wgpu::Sampler; 3],
    white: wgpu::TextureView,
    /// A source's key for a DictID (`TextureSource::key`) -> uploaded texture
    /// (`None`: the id resolved to nothing).
    textures: std::collections::HashMap<u64, Option<wgpu::TextureView>>,
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
    materials: std::collections::HashMap<(u64, usize, [u32; 16]), wgpu::BindGroup>,
    /// The palette every group's models draw with: theirs is never set.
    identity_palette: wgpu::BindGroup,
    /// The options' Render Scale (`scale`): 1 until set.
    pub scale: scale::Scale,
    /// The options' Brightness over the scene (`curve`): none until set.
    pub curve: curve::Curve,
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
        let make = |mode: usize, sky: bool| {
                let blend = blend_state(mode);
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("layer"),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
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
                        module: &shader,
                        entry_point: Some("fs_main"),
                        compilation_options: Default::default(),
                        // Colour only, opaque or not: nothing reads the
                        // target's alpha (no GS ALPHA setting the game uses
                        // takes the destination's), and it must stay the
                        // clear's 1. A page's WebGL2 canvas is composited
                        // with it (wgpu makes the context with alpha), and
                        // its picture (a report's) came out with the colour
                        // of every alpha-tested edge wrapped round: a cyan
                        // fringe where red exceeded alpha.
                        targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::COLOR })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
        };
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
            samplers,
            white,
            textures: Default::default(),
            missing_textures: Default::default(),
            start: web_time::Instant::now(),
            scroll_time: None,
            format,
            text: None,
            ui: None,
            billboards: None,
            fans: None,
            shadows: None,
            overlay: None,
            strips: Vec::new(),
            particles_end: 0,
            drawn: Default::default(),
            materials: Default::default(),
            identity_palette,
            scale: scale::Scale::new(format),
            curve: curve::Curve::new(format),
        }
    }

    fn texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, id: i32, src: &Textures) -> Option<wgpu::TextureView> {
        if id == 0 {
            return None;
        }
        let missing = &mut self.missing_textures;
        self.textures
            .entry(src.key(id))
            .or_insert_with(|| {
                let levels = src.levels(id);
                if levels.is_empty() {
                    missing.insert(id);
                    return None;
                }
                Some(upload_levels(device, queue, &levels))
            })
            .clone()
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
                let view = self.texture(device, queue, l.texture, textures);
                let (blend, mut base, si) = layer_state(l, view.is_some());
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
                passes.push(Pass { blend, bg, uniform, base, texture: l.texture, sampler: si, made: (base.uv0, base.uv1, si) });
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
        }
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
        let view = if rebind { self.texture(device, queue, texture, textures) } else { None };
        let Some(p) = m.batches.get_mut(batch).and_then(|b| b.2.get_mut(pass)) else { return };
        if rebind {
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
        let labels = self.text.as_ref().filter(|t| !t.labels.is_empty());
        let overlay = self.overlay.as_ref().filter(|o| !o.is_empty());
        let quads = self.ui.as_ref().filter(|u| !u.quads.is_empty());
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
        let rel = (view.view_proj.as_dmat4() * glam::DMat4::from_translation(view.eye.as_dvec3())).as_mat4();
        let uniform = FrameUniform {
            view_proj: rel.to_cols_array_2d(),
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
        // (DrawPrimBuffer's flags; RenderDeferredList).
        for deferred in [false, true] {
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
                            bound = true;
                            current = usize::MAX;
                        }
                        draws += self.encode_model(&mut pass, m, deferred, &mut current, 0..m.instance_count);
                    }
                    Draw::Instances(m, r) => {
                        if !Self::draws_anything(m) || r.is_empty() {
                            continue;
                        }
                        if !bound {
                            pass.set_bind_group(0, &self.frame_bg, &[]);
                            bound = true;
                            current = usize::MAX;
                        }
                        draws += self.encode_model(&mut pass, m, deferred, &mut current, r.start..r.end.min(m.instance_count));
                    }
                    Draw::Group(g) => {
                        for r in g.runs.iter().filter(shown) {
                            if let Some(b) = &r.bundles[deferred as usize] {
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
    /// bundle; `current` is the blend mode whose pipeline is set. Returns the
    /// draw calls.
    fn encode_model<'a, E: wgpu::util::RenderEncoder<'a>>(&'a self, e: &mut E, m: &'a GpuModel, deferred: bool, current: &mut usize, range: std::ops::Range<u32>) -> u32 {
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
                if p.blend != *current {
                    e.set_pipeline(&self.pipelines[p.blend]);
                    *current = p.blend;
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
        let view = self.texture(device, queue, l.texture, textures);
        let (blend, base, si) = layer_state(l, view.is_some());
        let key = (textures.key(l.texture), si, bytemuck::cast(base));
        if let Some(bg) = self.materials.get(&key) {
            return Material { blend, bg: bg.clone() };
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
        Material { blend, bg }
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
            members.push(Member { instance0, instances: p.instances.len() as u32, batches });
            base += verts.len() as u32;
            first_index += part_indices.len() as u32;
            instance0 += p.instances.len() as u32;
        }
        let mut g = GpuGroup { vertices, indices, instances, statics, members, runs: Vec::new() };
        for at in (0..parts.len()).step_by(RUN) {
            let span = at..(at + RUN).min(parts.len());
            let bounds = parts[span.clone()].iter().filter_map(part_bounds).reduce(|(a, b), (c, d)| (a.min(c), b.max(d)));
            let drawn = g.members[span.clone()].iter().filter(|m| m.instances > 0 && !m.batches.is_empty()).count() as u32;
            let mut run = Run { bounds, bundles: [None, None], drawn: (drawn, [0, 0]) };
            for deferred in [false, true] {
                let mut e = device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                    label: Some("group"),
                    color_formats: &[Some(self.format)],
                    depth_stencil: Some(wgpu::RenderBundleDepthStencil { format: DEPTH_FORMAT, depth_read_only: false, stencil_read_only: true }),
                    sample_count: 1,
                    multiview: None,
                });
                let n = self.encode_run(&mut e, &g, &g.members[span.clone()], deferred);
                if n > 0 {
                    run.bundles[deferred as usize] = Some(e.finish(&wgpu::RenderBundleDescriptor { label: Some("group") }));
                }
                run.drawn.1[deferred as usize] = n;
            }
            g.runs.push(run);
        }
        g
    }

    /// The commands of some of a group's members for one of the two passes,
    /// as `encode_model` issues a model's. Returns the draw calls.
    fn encode_run<'a>(&'a self, e: &mut wgpu::RenderBundleEncoder<'a>, g: &'a GpuGroup, members: &'a [Member], deferred: bool) -> u32 {
        let in_pass = |passes: &[Material]| passes.first().is_some_and(|p| (p.blend != 0) == deferred);
        e.set_bind_group(0, &self.frame_bg, &[]);
        e.set_bind_group(2, &self.identity_palette, &[]);
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
                    if p.blend != current {
                        e.set_pipeline(&self.pipelines[p.blend]);
                        current = p.blend;
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
    };
    let si = match l.wrap {
        1 | 2 => 1,
        3 => 2,
        _ => 0,
    };
    (blend, base, si)
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
}

/// A texture of `levels` (the base first, each half the one before), as
/// the surface cache puts a surface's levels in the GS's memory.
fn upload_levels(device: &wgpu::Device, queue: &wgpu::Queue, levels: &[esf::Image]) -> wgpu::TextureView {
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
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
    let scene = renderer.curve.scene_target(device, f.size);
    let st = scene.as_ref().unwrap_or(&f.color);
    renderer.draw(queue, &mut enc, st, &f.depth, view, items);
    if let Some((v, p, batches, textures)) = sky {
        renderer.draw_sky_billboards(device, queue, &mut enc, st, &f.depth, v, p, batches, textures);
    }
    renderer.draw_fans(queue, &mut enc, st, &f.depth, view);
    renderer.draw_plates(device, &mut enc, st, &f.depth, f.size);
    if let Some((v, p, batches, textures)) = particles {
        renderer.draw_billboards_and_shadows(device, queue, &mut enc, st, &f.depth, v, p, batches, textures, view);
    }
    renderer.curve.apply(queue, &mut enc, &f.color);
    renderer.draw_text(device, &mut enc, &f.color, &f.depth, f.size);
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

impl TextureSource for LibraryTextures<'_> {
    fn image(&self, id: i32) -> Option<esf::Image> {
        self.0.surface(id).and_then(|(c, s)| s.decode(&c.data))
    }
    fn levels(&self, id: i32) -> Vec<esf::Image> {
        self.0.surface(id).map(|(c, s)| s.decode_levels(&c.data)).unwrap_or_default()
    }
    fn key(&self, id: i32) -> u64 {
        self.0.texture_key(id)
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
