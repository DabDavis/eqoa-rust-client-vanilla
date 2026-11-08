//! Ambient Occlusion (the options' Extended > Lighting; the client's own, the
//! PS2 has none): the creases, corners and contacts of the opaque scene
//! darkened, as the light that fills a scene reaches less of them.
//! Screen-space, from the depth the opaque half of `Renderer::draw` wrote,
//! and run between that half and the blended one, so water, glows, the
//! sky's billboards, the effect volumes' fans, the particles and the blob
//! shadows draw over it as they were, and the name plates, the labels and
//! the 2D layer after them. The game's light is per vertex and Tunaria's
//! terrain triangles are 15 units across (Freeport's median edge), so a
//! per-vertex term could not darken the ground at a wall's foot; this is per
//! pixel.
//!
//! Each pixel's position relative to the eye comes out of the frame's own
//! eye-relative view-projection, inverted in doubles (a world position at
//! Freeport steps 2 mm, and its neighbours' differences would be noise), its
//! normal out of its neighbours'. Twelve samples on a spiral round it reach
//! `Settings::radius` in the world; each counts by how far it rises above
//! the pixel's plane and fades to nothing at the radius, so a wall a step
//! away darkens the floor at its foot and one across the room does not. A
//! sample's depth comes out of a pyramid of the depth, coarser the farther
//! the sample, as Scalable Ambient Obscurance has it: the reads were most of
//! the cost. That is done at a small size (`shift`: half the window's, a
//! quarter past 1,440 lines, and as much again smaller for the options'
//! Render Scale, so the frame drawn larger costs it no more), blurred across
//! and down without crossing a depth edge, and multiplied into the frame at
//! its own size from the four nearest small values, faded with distance and
//! with the fog (ao.wgsl).
//!
//! Off (the default) there is no pass at all: `draw` records the commands it
//! did before, and the frame is the same to the byte. On, it runs in the one
//! `draw` that follows `prepare` (the world's frame: not the loading
//! screen's gears, the pregame's scenes or a window's model, nor a draw into
//! part of the target, `draw_viewport`).

use bytemuck::{Pod, Zeroable};
use glam::{DMat4, Vec3};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum Mode {
    /// none: no pass
    #[default]
    Off = 0,
    /// the opaque scene darkened
    On = 1,
    /// the factor alone, white where nothing is near (for tuning, and tests)
    Factor = 2,
    /// each pixel's view depth, white at 100
    Depth = 3,
    /// each pixel's normal as the pass takes it
    Normals = 4,
}

impl Mode {
    /// The names `EQOA_AO` and a page's `?ao=` take.
    pub fn named(s: &str) -> Option<Mode> {
        Some(match s {
            "0" | "off" => Mode::Off,
            "1" | "on" => Mode::On,
            "factor" | "only" => Mode::Factor,
            "depth" => Mode::Depth,
            "normals" => Mode::Normals,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub mode: Mode,
    /// How far an occluder counts, in the world's units.
    pub radius: f32,
    /// How dark a crease goes: the factor is 1 less this times the share of
    /// the space round the pixel that is filled.
    pub intensity: f32,
}

impl Settings {
    /// Chosen by eye in Freeport, Kithicor, Neriak and Qeynos at noon and at
    /// night: the foot of a wall, a barrel's and a tree's darkened, open
    /// ground and open walls not.
    pub const RADIUS: f32 = 3.0;
    pub const INTENSITY: f32 = 1.5;
    pub const OFF: Settings = Settings { mode: Mode::Off, radius: Self::RADIUS, intensity: Self::INTENSITY };

    /// `mode` at the default radius and intensity.
    pub fn with(mode: Mode) -> Self {
        Settings { mode, ..Self::OFF }
    }

    /// Radius 0.1 to 20, intensity 0 to 4; anything else (a NaN) the default.
    fn bounded(self) -> Self {
        let bound = |v: f32, lo: f32, hi: f32, default: f32| if v.is_finite() { v.clamp(lo, hi) } else { default };
        Settings { mode: self.mode, radius: bound(self.radius, 0.1, 20.0, Self::RADIUS), intensity: bound(self.intensity, 0.0, 4.0, Self::INTENSITY) }
    }
}

/// A sample counts once it rises this far (a sine) above the pixel's plane:
/// a flat surface's own depth steps never darken it.
const BIAS: f32 = 0.1;

/// The textures the passes draw into: the factor and the view depth in two
/// bytes, and the pyramid's depths as their floats' bits, which every GPU
/// (WebGL2's too) can draw into.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The depth pyramid's levels, the small size to an eighth of it.
const LEVELS: usize = 4;

/// The small size is the frame's shifted right by this: for a window
/// `height` lines tall, a half, or past 1,440 lines a quarter (the factor is
/// soft; its cost then stays near a 1080-line window's however many pixels
/// the display has); and for a frame drawn `scale` times the window's size
/// (the options' Render Scale) that many times smaller again, the scale's
/// whole powers of two (x3 as x2), so the small size stays the window's.
fn shift(height: u32, scale: u32) -> u32 {
    let base = if height > 1440 { 2 } else { 1 };
    base + scale.max(1).ilog2()
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    inverse: [[f32; 4]; 4],
    size: [f32; 4],
    params: [f32; 4],
    fog: [f32; 4],
    mode: [u32; 4],
    levels: [[i32; 4]; LEVELS],
}

/// The settings, and the GPU's half once they are first on.
pub struct Ao {
    format: wgpu::TextureFormat,
    settings: Settings,
    gpu: Option<Gpu>,
    /// `prepare` readied the passes for the next `draw` (`take`)
    armed: std::cell::Cell<bool>,
}

struct Gpu {
    layout: wgpu::BindGroupLayout,
    pyramid: wgpu::RenderPipeline,
    occlude: wgpu::RenderPipeline,
    blur_h: wgpu::RenderPipeline,
    blur_v: wgpu::RenderPipeline,
    /// into the target, multiplied; and replacing it, for the diagnostic views
    multiply: wgpu::RenderPipeline,
    replace: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    sampler: wgpu::Sampler,
    targets: Option<Targets>,
}

/// What the passes read and draw into for one depth texture at one size.
struct Targets {
    size: (u32, u32),
    small: (u32, u32),
    shift: u32,
    /// the depth they read (a view compares by the texture it names)
    depth: wgpu::TextureView,
    /// the pyramid's levels side by side (`levels`)
    pyramid: wgpu::TextureView,
    levels: [[i32; 4]; LEVELS],
    a: wgpu::TextureView,
    b: wgpu::TextureView,
    /// the depth with `a`, and with `b`, each with the pyramid: a pass reads
    /// the one it is not drawing; and the depth alone, for the pyramid's
    reads_a: wgpu::BindGroup,
    reads_b: wgpu::BindGroup,
    reads_depth: wgpu::BindGroup,
}

impl Ao {
    pub(crate) fn new(format: wgpu::TextureFormat) -> Self {
        Ao { format, settings: Settings::OFF, gpu: None, armed: Default::default() }
    }

    /// The settings from now on; Off lets the passes' textures go.
    pub fn set(&mut self, settings: Settings) {
        self.settings = settings.bounded();
        if self.settings.mode == Mode::Off {
            self.armed.set(false);
            if let Some(g) = &mut self.gpu {
                g.targets = None;
            }
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings
    }

    /// Readies the passes for the next `draw`, into the frame `f`
    /// (`scale::Scale::targets`: its depth, its size and the Render Scale it
    /// is drawn at), the textures made the first time and again for another
    /// depth texture or size. Nothing while off; a `draw` with none before
    /// it runs none.
    pub fn prepare(&mut self, device: &wgpu::Device, f: &crate::scale::Targets) {
        self.armed.set(false);
        if self.settings.mode == Mode::Off {
            return;
        }
        let size = (f.size.0.max(1), f.size.1.max(1));
        let shift = shift(size.1 / f.k.max(1), f.k);
        let format = self.format;
        let g = self.gpu.get_or_insert_with(|| Gpu::new(device, format));
        if !g.targets.as_ref().is_some_and(|t| t.size == size && t.shift == shift && t.depth == f.depth) {
            // the last ones let go before the next are made
            g.targets = None;
            g.targets = Some(Targets::new(device, g, &f.depth, size, shift));
        }
        self.armed.set(true);
    }

    /// Whether this `draw` runs the passes over `depth`: on, `prepare` called
    /// for it since the last `draw`, and readied for this depth texture.
    /// Takes the readiness: the next `draw` needs a `prepare` of its own.
    pub(crate) fn take(&self, depth: &wgpu::TextureView) -> bool {
        self.armed.replace(false) && self.settings.mode != Mode::Off && self.gpu.as_ref().and_then(|g| g.targets.as_ref()).is_some_and(|t| t.depth == *depth)
    }

    /// The passes, after the opaque half of the frame: `rel` is the frame's
    /// view-projection relative to the eye (`draw`'s, in doubles), `fog` its
    /// fog.
    pub(crate) fn run(&self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, rel: DMat4, fog: Option<(f32, f32, Vec3)>) {
        let Some(g) = &self.gpu else { return };
        let Some(t) = &g.targets else { return };
        // the full inverse: `view_proj` is made in floats, so the eye's own
        // translation leaves a millimetre in its w row, and a view is never
        // the pure perspective a shortcut would take it for
        let inverse = rel.inverse();
        if !inverse.is_finite() {
            return;
        }
        let s = self.settings;
        // pixels a unit of the world covers at view depth 1: the projection's
        // y scale (its second row's length, the rest being a rotation)
        let per_unit = rel.row(1).truncate().length() * t.size.1 as f64 * 0.5;
        let u = Uniforms {
            inverse: inverse.as_mat4().to_cols_array_2d(),
            size: [t.size.0 as f32, t.size.1 as f32, t.small.0 as f32, t.small.1 as f32],
            params: [s.radius, s.intensity, BIAS, per_unit as f32],
            fog: match fog {
                Some((near, far, _)) => [near, far.max(near + 1e-3), 1.0, 0.0],
                None => [0.0, 1.0, 0.0, 0.0],
            },
            mode: [s.mode as u32, t.shift, 0, 0],
            levels: t.levels,
        };
        queue.write_buffer(&g.uniforms, 0, bytemuck::bytes_of(&u));
        let pass = |encoder: &mut wgpu::CommandEncoder, into: &wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>, pipeline: &wgpu::RenderPipeline, reads: &wgpu::BindGroup| {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ambient occlusion"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: into,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, reads, &[]);
            pass.draw(0..3, 0..1);
        };
        let clear = wgpu::LoadOp::Clear(wgpu::Color::WHITE);
        pass(encoder, &t.pyramid, clear, &g.pyramid, &t.reads_depth);
        pass(encoder, &t.a, clear, &g.occlude, &t.reads_b);
        pass(encoder, &t.b, clear, &g.blur_h, &t.reads_a);
        pass(encoder, &t.a, clear, &g.blur_v, &t.reads_b);
        let apply = if s.mode == Mode::On { &g.multiply } else { &g.replace };
        pass(encoder, target, wgpu::LoadOp::Load, apply, &t.reads_a);
    }
}

/// The depth pyramid's levels side by side in one texture: level 0 (the
/// small size, `small`) at the left, the others down its right, each a half
/// of the one before, rounded up; each where it starts and its size, and the
/// texture's size.
fn levels(small: (u32, u32)) -> ([[i32; 4]; LEVELS], (u32, u32)) {
    let mut out = [[0, 0, small.0 as i32, small.1 as i32]; LEVELS];
    let (mut down, mut right) = (0, 0);
    for (l, level) in out.iter_mut().enumerate().skip(1) {
        let (w, h) = (small.0.div_ceil(1 << l), small.1.div_ceil(1 << l));
        *level = [small.0 as i32, down as i32, w as i32, h as i32];
        down += h;
        right = right.max(w);
    }
    (out, (small.0 + right, small.1.max(down)))
}

impl Targets {
    fn new(device: &wgpu::Device, g: &Gpu, depth: &wgpu::TextureView, size: (u32, u32), shift: u32) -> Self {
        let small = (size.0.div_ceil(1 << shift), size.1.div_ceil(1 << shift));
        let (levels, atlas) = levels(small);
        let texture = |label, (width, height): (u32, u32)| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let (a, b, pyramid) = (texture("ambient occlusion a", small), texture("ambient occlusion b", small), texture("ambient occlusion depths", atlas));
        let reads = |small_view: &wgpu::TextureView, pyramid: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ambient occlusion"),
                layout: &g.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(depth) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(small_view) },
                    wgpu::BindGroupEntry { binding: 2, resource: g.uniforms.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&g.sampler) },
                    wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(pyramid) },
                ],
            })
        };
        // the pyramid's pass draws into it, so its group names `a` there
        let (reads_a, reads_b, reads_depth) = (reads(&a, &pyramid), reads(&b, &pyramid), reads(&a, &a));
        Targets { size, small, shift, depth: depth.clone(), pyramid, levels, a, b, reads_a, reads_b, reads_depth }
    }
}

impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            // the depth too: a depth texture binds as an unfilterable float,
            // read through the nearest sampler (ao.wgsl: wgpu's GL backend
            // leaves a depth texture's own filters linear, which GLES 3.0
            // counts as incomplete for a texelFetch)
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ambient occlusion"),
            entries: &[
                texture(0),
                texture(1),
                crate::uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
                wgpu::BindGroupLayoutEntry { binding: 3, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering), count: None },
                texture(4),
            ],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("ao.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ambient occlusion"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = |entry: &str, format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>, write_mask: wgpu::ColorWrites| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ambient occlusion"),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[] },
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        // the target's colour times the factor; its alpha, which must stay
        // the clear's 1 (a page's canvas is composited with it), not written
        let multiply = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::Src, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
        };
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("ambient occlusion"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ambient occlusion depth"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let all = wgpu::ColorWrites::ALL;
        Gpu {
            pyramid: pipeline("fs_z", FORMAT, None, all),
            occlude: pipeline("fs_ao", FORMAT, None, all),
            blur_h: pipeline("fs_blur_h", FORMAT, None, all),
            blur_v: pipeline("fs_blur_v", FORMAT, None, all),
            multiply: pipeline("fs_apply", format, Some(multiply), wgpu::ColorWrites::COLOR),
            replace: pipeline("fs_apply", format, None, wgpu::ColorWrites::COLOR),
            layout,
            uniforms,
            sampler,
            targets: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_by_name_and_bounds() {
        assert_eq!(Mode::named("1"), Some(Mode::On));
        assert_eq!(Mode::named("off"), Some(Mode::Off));
        assert_eq!(Mode::named("only"), Some(Mode::Factor));
        assert_eq!(Mode::named("normals"), Some(Mode::Normals));
        assert_eq!(Mode::named("bright"), None);
        let s = Settings { mode: Mode::On, radius: f32::NAN, intensity: 9.0 }.bounded();
        assert_eq!((s.radius, s.intensity), (Settings::RADIUS, 4.0));
        assert_eq!(Settings { radius: -1.0, ..Settings::OFF }.bounded().radius, 0.1);
        // the uniform block as ao.wgsl declares it: a matrix, four vectors
        // and the four levels
        assert_eq!(size_of::<Uniforms>(), 64 + 4 * 16 + 4 * 16);
    }

    /// The pyramid's levels side by side, none over another, each holding
    /// every texel of the level before halved (the last row and column too).
    #[test]
    fn the_levels_side_by_side() {
        for small in [(960, 540), (1, 1), (961, 541), (80, 3)] {
            let (l, size) = levels(small);
            assert_eq!(l[0], [0, 0, small.0 as i32, small.1 as i32]);
            for k in 1..LEVELS {
                let [x, y, w, h] = l[k];
                assert!(x as u32 + w as u32 <= size.0 && y as u32 + h as u32 <= size.1, "{small:?} level {k} inside");
                assert_eq!(((small.0 as i32 - 1) >> k, (small.1 as i32 - 1) >> k), (w - 1, h - 1), "{small:?} level {k}'s last texel");
                assert!(x >= l[0][2] && (k == 1 || y >= l[k - 1][1] + l[k - 1][3]), "{small:?} level {k} apart");
            }
        }
    }

    /// The small size: a half of a window up to 1,440 lines, a quarter past
    /// it, and the Render Scale's frame as much smaller again, so a window
    /// drawn at x2 or x4 has the small size it has at x1 (x3 a half again
    /// larger). The control: the frame's own height alone (the scale left
    /// out) gives an x4 frame of a 1080-line window four times the pixels.
    #[test]
    fn the_small_size_is_the_windows() {
        assert_eq!((shift(1080, 1), shift(1440, 1), shift(2160, 1)), (1, 1, 2));
        assert_eq!((shift(1080, 2), shift(1080, 3), shift(1080, 4)), (2, 2, 3));
        assert_eq!((shift(2160, 2), shift(0, 0)), (3, 1));
        let small = |frame: u32, k: u32| frame >> shift(frame / k, k);
        assert_eq!([small(1080, 1), small(2160, 2), small(4320, 4)], [540; 3]);
        assert_eq!(small(2160, 1), 540, "a 2160-line window's quarter");
        assert_ne!(4320 >> shift(4320, 1), 540, "the control: the frame's height alone");
    }
}
