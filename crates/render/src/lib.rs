//! The renderer, on wgpu.
//!
//! Models out of the containers, textured through the dictionary, drawn as the
//! game draws them: the VU1 strip programs' lighting, one PASS PER MATERIAL
//! LAYER with the GS state `VIMaterial::ConstructTextPass` builds for it
//! (alpha test or blend mode, wrap, the UV transform and its scroll), and GPU
//! skinning as `VIRStrip4` does it.
//!
//! One deliberate difference: every opaque pass is drawn before any blended
//! one, where the game draws a strip's layers back to back. A translucent
//! layer then never hides geometry drawn after it.

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

/// Palette size: enough for every character skeleton (40 nodes on a human).
pub const MAX_BONES: usize = 64;

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
    view_proj: [[f32; 4]; 4],
    sun_dir: [f32; 4],
    /// rgb, and w = the time in seconds (for UV scrolling).
    sun: [f32; 4],
    ambient: [f32; 4],
    /// rgb, and w = 1 when fog is on.
    fog_color: [f32; 4],
    /// near, far.
    fog: [f32; 4],
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
}

/// One model on the GPU, drawn once per instance (a model matrix each).
pub struct GpuModel {
    vertices: wgpu::Buffer,
    /// The skin palette and its bind group; for an unskinned model, all
    /// identity and never written.
    palette: wgpu::Buffer,
    palette_bg: wgpu::BindGroup,
    pub skinned: bool,
    instances: wgpu::Buffer,
    instance_count: u32,
    batches: Vec<(wgpu::Buffer, u32, Vec<Pass>)>,
    /// Bounds of the model itself, before any instance transform.
    pub bounds: (Vec3, Vec3),
    /// Part of the sky: drawn first, through `View::sky_view_proj`, with no
    /// depth test or write and its layers in order (`VIAtmosphere::RenderSky`;
    /// `SetResources` turns Z off on the sky's materials).
    pub sky: bool,
    /// Skipped by `draw` when false.
    pub visible: bool,
}

/// A texture source: the decoded image a DictID names, if any.
pub type Textures<'a> = dyn Fn(i32) -> Option<esf::Image> + 'a;

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
    /// DictID -> uploaded texture (`None`: the id resolved to nothing).
    textures: std::collections::HashMap<i32, Option<wgpu::TextureView>>,
    /// DictIDs a batch asked for that no loaded file holds.
    pub missing_textures: std::collections::BTreeSet<i32>,
    start: std::time::Instant,
    format: wgpu::TextureFormat,
    /// Labels (name plates), once a font is set.
    pub text: Option<text::Text>,
    /// The 2D layer (the HUD), drawn over everything.
    pub ui: Option<ui::Ui>,
}

/// What to draw this frame. The lights are the VU1 strip programs' uniforms:
/// the direction the sun's light TRAVELS, its colour, and ambient.
pub struct View {
    pub view_proj: Mat4,
    pub sun_dir: Vec3,
    pub sun: Vec3,
    pub ambient: Vec3,
    /// `VIRaster::SetFog`: linear in view depth from `near` (none) to `far`
    /// (all fog colour), as the strip programs' per-vertex fog value and the
    /// GS's FOGCOL mix.
    pub fog: Option<(f32, f32, Vec3)>,
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
            sun_dir: Vec3::new(0.35, -0.8, 0.45),
            sun: Vec3::splat(0.6),
            ambient: Vec3::splat(0.45),
            fog: None,
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
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend,
                            write_mask: if blend.is_some() { wgpu::ColorWrites::COLOR } else { wgpu::ColorWrites::ALL },
                        })],
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
        let sampler = |mode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u: mode,
                address_mode_v: mode,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            })
        };
        let samplers = [
            sampler(wgpu::AddressMode::Repeat),
            sampler(wgpu::AddressMode::ClampToEdge),
            sampler(wgpu::AddressMode::MirrorRepeat),
        ];
        let white = upload_rgba(device, queue, 1, 1, &[255; 4]);
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
            start: std::time::Instant::now(),
            format,
            text: None,
            ui: None,
        }
    }

    fn texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, id: i32, src: &Textures) -> Option<wgpu::TextureView> {
        if id == 0 {
            return None;
        }
        let missing = &mut self.missing_textures;
        self.textures
            .entry(id)
            .or_insert_with(|| {
                let img = src(id);
                if img.is_none() {
                    missing.insert(id);
                }
                img.map(|i| upload_rgba(device, queue, i.width, i.height, &i.rgba))
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
        let verts: Vec<Vertex> = (0..m.pos.len())
            .map(|i| Vertex {
                pos: m.pos[i],
                uv: m.uv[i],
                normal: m.normal[i],
                color: m.color[i],
                bones: m.bones.get(i).copied().unwrap_or([255; 4]),
                weights: m.weights.get(i).copied().unwrap_or([0; 4]),
            })
            .collect();
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
            // A material with no layer at all draws once, untextured.
            let untextured = [esf::Layer {
                flags: 1,
                texture: 0,
                wrap: 0,
                blend: 0,
                color: [255; 4],
                uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                lod_bias: 0.8,
                uv_rate: [0.0, 0.0],
            }];
            let layers: &[esf::Layer] = if b.layers.is_empty() { &untextured } else { &b.layers };
            let mut passes = Vec::new();
            for l in layers {
                let view = self.texture(device, queue, l.texture, textures);
                let blend = (l.blend.clamp(0, 4)) as usize;
                let k = l.uv_coeffs();
                // The layer colour reaches the GS as c/255 × 128 and modulates
                // by /128: a tint of c/255.
                let tint = l.color.map(|c| c as f32 / 255.0);
                let base = LayerUniform {
                    tint,
                    flags: [view.is_some() as i32 as f32, (blend != 0) as i32 as f32, 0.0, 0.0],
                    uv0: [k[0], k[1], k[2], k[3]],
                    uv1: [k[4], k[5], l.uv_rate[0], l.uv_rate[1]],
                };
                let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("layer"),
                    contents: bytemuck::bytes_of(&base),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                });
                let sampler = match l.wrap {
                    1 | 2 => &self.samplers[1],
                    3 => &self.samplers[2],
                    _ => &self.samplers[0],
                };
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
                passes.push(Pass { blend, bg, uniform, base });
            }
            let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: bytemuck::cast_slice(&b.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            batches.push((indices, b.indices.len() as u32, passes));
        }
        let lo = m.pos.iter().fold(Vec3::splat(f32::MAX), |a, p| a.min(Vec3::from(*p)));
        let hi = m.pos.iter().fold(Vec3::splat(f32::MIN), |a, p| a.max(Vec3::from(*p)));
        let instance_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("instances"),
            contents: bytemuck::cast_slice(instances),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        GpuModel {
            vertices,
            palette,
            palette_bg,
            skinned: m.skinned,
            instances: instance_buf,
            instance_count: instances.len() as u32,
            batches,
            bounds: (lo, hi),
            sky: false,
            visible: true,
        }
    }

    /// Sets every layer's colour to `rgb` (0..1), keeping each layer's own
    /// alpha (`VIAtmosphere::SetSkyColor`).
    pub fn set_layer_rgb(&self, queue: &wgpu::Queue, m: &GpuModel, rgb: Vec3) {
        for (_, _, passes) in &m.batches {
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

    /// The labels, over what `draw` drew (depth-tested against it, not
    /// writing it). `size` is the target's, in pixels.
    pub fn draw_text(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32)) {
        let labels = self.text.as_ref().filter(|t| !t.labels.is_empty());
        let quads = self.ui.as_ref().filter(|u| !u.quads.is_empty());
        if labels.is_none() && quads.is_none() {
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
        models: &[&GpuModel],
    ) {
        let uniform = FrameUniform {
            view_proj: view.view_proj.to_cols_array_2d(),
            sun_dir: view.sun_dir.normalize().extend(0.0).to_array(),
            sun: view.sun.extend(self.start.elapsed().as_secs_f32()).to_array(),
            ambient: view.ambient.extend(0.0).to_array(),
            fog_color: match view.fog {
                Some((_, _, c)) => c.extend(1.0).to_array(),
                None => [0.0; 4],
            },
            fog: match view.fog {
                Some((n, f, _)) => [n, f.max(n + 1e-3), 0.0, 0.0],
                None => [0.0, 1.0, 0.0, 0.0],
            },
        };
        queue.write_buffer(&self.frame_buf, 0, bytemuck::bytes_of(&uniform));
        if let Some(vp) = view.sky_view_proj {
            let sky = FrameUniform { view_proj: vp.to_cols_array_2d(), ..uniform };
            queue.write_buffer(&self.sky_frame_buf, 0, bytemuck::bytes_of(&sky));
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("models"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: view.clear.x as f64,
                        g: view.clear.y as f64,
                        b: view.clear.z as f64,
                        a: 1.0,
                    }),
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
        // The sky: its own view, no depth, every layer in order.
        if view.sky_view_proj.is_some() {
            pass.set_bind_group(0, &self.sky_frame_bg, &[]);
            for m in models.iter().filter(|m| m.sky && m.visible && m.instance_count > 0) {
                pass.set_vertex_buffer(0, m.vertices.slice(..));
                pass.set_vertex_buffer(1, m.instances.slice(..));
                pass.set_bind_group(2, &m.palette_bg, &[]);
                for (indices, count, passes) in &m.batches {
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                    for p in passes {
                        pass.set_pipeline(&self.sky_pipelines[p.blend]);
                        pass.set_bind_group(1, &p.bg, &[]);
                        pass.draw_indexed(0..*count, 0, 0..m.instance_count);
                    }
                }
            }
        }
        pass.set_bind_group(0, &self.frame_bg, &[]);
        // Opaque passes first, then the blended ones (see the module docs).
        for blended in [false, true] {
            let mut current = usize::MAX;
            for m in models {
                if m.instance_count == 0 || m.sky || !m.visible {
                    continue;
                }
                pass.set_vertex_buffer(0, m.vertices.slice(..));
                pass.set_vertex_buffer(1, m.instances.slice(..));
                pass.set_bind_group(2, &m.palette_bg, &[]);
                for (indices, count, passes) in &m.batches {
                    pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                    for p in passes.iter().filter(|p| (p.blend != 0) == blended) {
                        if p.blend != current {
                            pass.set_pipeline(&self.pipelines[p.blend]);
                            current = p.blend;
                        }
                        pass.set_bind_group(1, &p.bg, &[]);
                        pass.draw_indexed(0..*count, 0, 0..m.instance_count);
                    }
                }
            }
        }
    }
}

fn uniform_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
        count: None,
    }
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

/// Renders one frame offscreen and returns it as RGBA8 rows (for `--shot`).
pub fn capture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &Renderer,
    w: u32,
    h: u32,
    view: &View,
    models: &[&GpuModel],
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
    renderer.draw(queue, &mut enc, &tv, &depth, view, models);
    renderer.draw_text(device, &mut enc, &tv, &depth, (w, h));
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

/// Textures out of a library: the surface a DictID names, wherever it was
/// registered, decoded.
pub fn library_textures(lib: &esf::Library) -> impl Fn(i32) -> Option<esf::Image> + '_ {
    move |id| lib.surface(id).and_then(|(c, s)| s.decode(&c.data))
}
