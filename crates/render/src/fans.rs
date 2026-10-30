//! Immediate-mode fans (`VIRaster::BeginTriFan` .. `EndTriFan`) as the VU1
//! program `VIRFan` draws them: what `VIEffectVolume` puts on the raster (a
//! textured quad a puff of fog, a mote of dust, a leaf or a light shaft).
//! The corners change every frame, so they go up as instances, one quad
//! each, into one buffer that grows as it must: a frame's fans cost one
//! write and a draw call per run of one material. Lit (`fans.wgsl`) as
//! VIRStrip lights a vertex and fogged as the strip programs fog; textured
//! by the material's one layer and blended by its GS state, Z tested and
//! written only as the material says. Drawn after the world and the sky's
//! billboards, before the particles: `VIScene::Render` draws the effect
//! volumes in its deferred actor pass, before name plates and particles.
//! A run marked `ps2` (the ground fog, with the options' Ground Fog at PS2)
//! is blended as the GS blends it into the PS2's 16-bit frame (`fog16`),
//! where the renderer can add its sums up and the target can be copied.

use crate::{DEPTH_FORMAT, Renderer, Textures, View};
use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

/// One fan's front face: its first four vertices (relative to the view's
/// eye) and their UVs, with the raster's colour and normal at the time
/// (`VIRaster::Color`, `Normal`; the normal in the world's axes).
#[derive(Clone, Copy, Debug, Default)]
pub struct Quad {
    pub corners: [[f32; 3]; 4],
    pub uvs: [[f32; 2]; 4],
    pub color: [f32; 4],
    pub normal: [f32; 3],
}

/// Consecutive fans under one material: its texture's DictID (0 none:
/// white), its blend mode (as `billboard::Batch::blend`), whether it
/// writes Z, and whether it is blended as the PS2's 16-bit frame keeps a
/// blend (`fog16`: only a run of mode 3 that does not write Z; the others
/// are drawn as they are).
#[derive(Clone, Debug, Default)]
pub struct Run {
    pub texture: i32,
    pub blend: usize,
    pub zwrite: bool,
    pub ps2: bool,
    pub quads: Vec<Quad>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Inst {
    c: [[f32; 3]; 4],
    uv: [[f32; 2]; 4],
    color: [f32; 4],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameU {
    view_proj: [[f32; 4]; 4],
    sun_dir: [f32; 4],
    sun: [f32; 4],
    ambient: [f32; 4],
    fog_color: [f32; 4],
    fog: [f32; 4],
    point: [f32; 4],
    point_color: [f32; 4],
}

/// The pipeline key: blend mode, Z write.
type Key = (usize, bool);

pub(crate) struct Fans {
    shader: wgpu::ShaderModule,
    frame_layout: wgpu::BindGroupLayout,
    tex_layout: wgpu::BindGroupLayout,
    layout: wgpu::PipelineLayout,
    pipelines: HashMap<Key, wgpu::RenderPipeline>,
    sampler: wgpu::Sampler,
    frame_buf: wgpu::Buffer,
    frame_bg: wgpu::BindGroup,
    /// the instance buffer and how many quads it holds
    instances: Option<(wgpu::Buffer, u64)>,
    /// each texture's bind group, by DictID
    groups: HashMap<i32, wgpu::BindGroup>,
    /// what `draw_fans` draws: each run's key, texture, quads and whether a
    /// PS2 frame keeps it (`fog16`)
    pending: Vec<(Key, i32, std::ops::Range<u32>, bool)>,
    /// the eye their corners are relative to
    eye: glam::Vec3,
    /// the PS2 runs' corners (relative to the eye): the rectangle `fog16`
    /// copies and draws over
    ps2_corners: Vec<[f32; 3]>,
    /// the fans' sums (`fs_sums`, into `fog16`'s two targets), made at the
    /// first PS2 run, and their colours' spread over those sums (`fs_spread`)
    sums: Option<wgpu::RenderPipeline>,
    spread: Option<wgpu::RenderPipeline>,
    fog16: Option<crate::fog16::Fog16>,
}

impl Fans {
    fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("fans.wgsl"));
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fans frame"),
            entries: &[crate::uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fans texture"),
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
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fans"),
            bind_group_layouts: &[Some(&frame_layout), Some(&tex_layout)],
            immediate_size: 0,
        });
        // `VIMaterial::Init`'s layer wraps on both axes (CLAMP_1 0)
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let frame_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fans frame"),
            size: std::mem::size_of::<FrameU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fans frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: frame_buf.as_entire_binding() }],
        });
        Fans {
            shader,
            frame_layout,
            tex_layout,
            layout,
            pipelines: HashMap::new(),
            sampler,
            frame_buf,
            frame_bg,
            instances: None,
            groups: HashMap::new(),
            pending: Vec::new(),
            eye: glam::Vec3::ZERO,
            ps2_corners: Vec::new(),
            sums: None,
            spread: None,
            fog16: None,
        }
    }

    /// An instance's attributes: its four corners, their UVs two to a
    /// slot, the colour and the normal.
    fn vertex_attrs() -> [wgpu::VertexAttribute; 8] {
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32x3, 4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x3]
    }

    /// The sums' pipeline: the fans as they are drawn, Z tested and not
    /// written, each adding its alpha, alpha times colour, alpha squared and
    /// one to `fog16`'s targets.
    fn sums_pipeline(&mut self, device: &wgpu::Device) {
        if self.sums.is_some() {
            return;
        }
        let attrs = Self::vertex_attrs();
        let add = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
        let blend = Some(wgpu::BlendState { color: add, alpha: add });
        self.sums = Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fans sums"),
            layout: Some(&self.layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Inst>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &attrs })],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some("fs_sums"),
                compilation_options: Default::default(),
                targets: &[
                    Some(wgpu::ColorTargetState { format: crate::fog16::SUMS_FORMAT, blend, write_mask: wgpu::ColorWrites::ALL }),
                    Some(wgpu::ColorTargetState { format: crate::fog16::SQUARES_FORMAT, blend, write_mask: wgpu::ColorWrites::ALL }),
                ],
            }),
            multiview_mask: None,
            cache: None,
        }));
    }

    /// The spread's pipeline: the fans again, each adding its alpha times
    /// its colour's distance from the sums' mean squared to `fog16`'s third
    /// target, the sums read through `sums_layout`.
    fn spread_pipeline(&mut self, device: &wgpu::Device, sums_layout: &wgpu::BindGroupLayout) {
        if self.spread.is_some() {
            return;
        }
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fans spread"),
            bind_group_layouts: &[Some(&self.frame_layout), Some(&self.tex_layout), Some(sums_layout)],
            immediate_size: 0,
        });
        let attrs = Self::vertex_attrs();
        let add = wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add };
        self.spread = Some(device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fans spread"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &self.shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Inst>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &attrs })],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.shader,
                entry_point: Some("fs_spread"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: crate::fog16::COLOURS_FORMAT, blend: Some(wgpu::BlendState { color: add, alpha: add }), write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        }));
    }

    fn pipeline(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat, key: Key) {
        let (shader, layout) = (&self.shader, &self.layout);
        self.pipelines.entry(key).or_insert_with(|| {
            let (blend, zwrite) = key;
            let attrs = Self::vertex_attrs();
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("fans"),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Inst>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &attrs })],
                },
                // one face is drawn of each fan (`fans.wgsl`), so nothing to cull
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(zwrite),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    // colour only: the target's alpha stays 1 (the models' pipelines)
                    targets: &[Some(wgpu::ColorTargetState { format, blend: crate::blend_state(blend), write_mask: wgpu::ColorWrites::COLOR })],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
    }
}

impl Renderer {
    /// The fans `draw_fans` draws from now on, in this order, their corners
    /// relative to `eye` (none: it draws nothing at all). Their textures are
    /// uploaded here.
    pub fn set_fans(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, runs: &[Run], eye: glam::Vec3, textures: &Textures) {
        let n: usize = runs.iter().map(|r| r.quads.len()).sum();
        if n == 0 {
            if let Some(f) = self.fans.as_mut() {
                f.pending.clear();
                f.ps2_corners.clear();
            }
            return;
        }
        let mut inst = Vec::with_capacity(n);
        let mut pending = Vec::new();
        let mut ps2_corners = Vec::new();
        for r in runs.iter().filter(|r| !r.quads.is_empty()) {
            let start = inst.len() as u32;
            inst.extend(r.quads.iter().map(|q| Inst { c: q.corners, uv: q.uvs, color: q.color, normal: q.normal }));
            // a PS2 frame's blend is ALPHA_1's (Cs - Cd) As + Cd, Z not written
            let ps2 = r.ps2 && r.blend == 3 && !r.zwrite && self.ps2_sums;
            if ps2 {
                ps2_corners.extend(r.quads.iter().flat_map(|q| q.corners));
            }
            pending.push(((r.blend.min(4), r.zwrite), r.texture, start..inst.len() as u32, ps2));
        }
        let views: Vec<(i32, Option<wgpu::TextureView>)> = pending.iter().map(|p| (p.1, self.texture(device, queue, p.1, textures))).collect();
        let format = self.format;
        let white = self.white.clone();
        let f = self.fans.get_or_insert_with(|| Fans::new(device));
        for (id, view) in views {
            if !f.groups.contains_key(&id) {
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("fans texture"),
                    layout: &f.tex_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view.as_ref().unwrap_or(&white)) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&f.sampler) },
                    ],
                });
                f.groups.insert(id, bg);
            }
        }
        for p in &pending {
            f.pipeline(device, format, p.0);
            if p.3 {
                f.sums_pipeline(device);
            }
        }
        if f.instances.as_ref().is_none_or(|(_, cap)| *cap < n as u64) {
            let cap = (n as u64).next_power_of_two().max(256);
            let buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("fans"),
                size: cap * std::mem::size_of::<Inst>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            f.instances = Some((buf, cap));
        }
        if let Some((buf, _)) = &f.instances {
            queue.write_buffer(buf, 0, bytemuck::cast_slice(&inst));
        }
        f.pending = pending;
        f.ps2_corners = ps2_corners;
        f.eye = eye;
    }

    /// Whether the fans `set_fans` set read the frame they are drawn over (a
    /// PS2 run, `fog16`): its target must be one that can be copied, or the
    /// runs are drawn as they are (`curve::Curve::set_through` makes the
    /// scene's own texture one).
    pub fn fans_read_frame(&self) -> bool {
        self.fans.as_ref().is_some_and(|f| f.pending.iter().any(|p| p.3))
    }

    /// Whether this renderer adds a PS2 run's sums up (`fog16::supported`,
    /// told by whoever has the adapter; off until told).
    pub fn set_ps2_sums(&mut self, on: bool) {
        self.ps2_sums = on;
    }

    /// The PS2 runs as the GS draws them exactly, for measuring the sums
    /// against: each fan alone over the frame the last left, under each
    /// pixel's own matrix entry (a copy and three passes a fan: slow).
    pub fn set_ps2_sequential(&mut self, on: bool) {
        self.ps2_sequential = on;
    }

    /// Draws the fans `set_fans` set, over what is already in `target` and
    /// `depth`, under `view`'s lights and fog, if `view` is the one they were
    /// made for (its eye theirs): a fan faces the camera it was made for, so
    /// another view of the same frame does not draw them.
    pub fn draw_fans(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, view: &View) {
        let (format, sequential) = (self.format, self.ps2_sequential);
        let Some(f) = self.fans.as_mut().filter(|f| !f.pending.is_empty() && f.eye == view.eye) else { return };
        if f.instances.is_none() {
            return;
        }
        let rel = (view.view_proj.as_dmat4() * glam::DMat4::from_translation(view.eye.as_dvec3())).as_mat4();
        let u = FrameU {
            view_proj: rel.to_cols_array_2d(),
            sun_dir: view.sun_dir.normalize().extend(0.0).to_array(),
            sun: view.sun.extend(0.0).to_array(),
            ambient: view.ambient.extend(0.0).to_array(),
            fog_color: view.fog.map_or([0.0; 4], |(_, _, c)| c.extend(1.0).to_array()),
            // z: the GS's K for a fan's material (made in code: the default
            // bias), or the base level's with the mip levels off
            fog: {
                let [n, fa] = view.fog.map_or([0.0, 1.0], |(n, fa, _)| [n, fa.max(n + 1e-3)]);
                [n, fa, crate::effects_lod_k(), 0.0]
            },
            point: view.point.map_or([0.0; 4], |(p, r, _)| (p - view.eye).extend(r).to_array()),
            point_color: view.point.map_or([0.0; 4], |(_, _, c)| c.extend(1.0).to_array()),
        };
        queue.write_buffer(&f.frame_buf, 0, bytemuck::bytes_of(&u));
        // The PS2 runs (`fog16`) where the target can be copied: the runs
        // before the first drawn as they are, then the PS2 runs' sums and
        // the GS's steps over them, then the other runs after it. Elsewhere
        // every run is drawn as it is.
        let t = target.texture();
        let copyable = t.usage().contains(wgpu::TextureUsages::COPY_SRC) && t.format() == format && u64::from(t.width()) * u64::from(t.height()) <= crate::fog16::MAX_PIXELS;
        let first = if copyable { f.pending.iter().position(|p| p.3) } else { None };
        let size = (t.width(), t.height());
        let rect = first.and_then(|_| crate::fog16::rect(&f.ps2_corners, rel, size));
        if rect.is_some() && f.spread.is_none() {
            let layout = f.fog16.get_or_insert_with(|| crate::fog16::Fog16::new(device, format)).sums_layout().clone();
            f.spread_pipeline(device, &layout);
        }
        let (before, after) = f.pending.split_at(first.unwrap_or(f.pending.len()));
        let ps2 = first.is_some();
        Fans::draw_runs(f, encoder, target, depth, before.iter());
        if let Some(rect) = rect {
            let runs: Vec<_> = after.iter().filter(|p| p.3).cloned().collect();
            let (sums, spread, groups, frame_bg) = (f.sums.as_ref().expect("made with the run"), f.spread.as_ref().expect("made above"), &f.groups, &f.frame_bg);
            let Some((fans_buf, _)) = &f.instances else { return };
            let fog = f.fog16.as_mut().expect("made above");
            // the fans twice: their sums, then their colours' spread over them
            let draw = |pass: &mut wgpu::RenderPass<'_>, second: bool, texture: i32, range: std::ops::Range<u32>| {
                pass.set_pipeline(if second { spread } else { sums });
                pass.set_bind_group(0, frame_bg, &[]);
                pass.set_vertex_buffer(0, fans_buf.slice(..));
                pass.set_bind_group(1, &groups[&texture], &[]);
                pass.draw(0..6, range);
            };
            if sequential {
                // each fan alone, over the frame as the last left it
                let mut j = 0;
                for (_, texture, range, _) in &runs {
                    for i in range.clone() {
                        if let Some(r) = crate::fog16::rect(&f.ps2_corners[4 * j..4 * j + 4], rel, size) {
                            fog.draw(device, encoder, target, depth, r, true, |pass, second| draw(pass, second, *texture, i..i + 1));
                        }
                        j += 1;
                    }
                }
            } else {
                fog.draw(device, encoder, target, depth, rect, false, |pass, second| {
                    for (_, texture, range, _) in &runs {
                        draw(pass, second, *texture, range.clone());
                    }
                });
            }
        }
        Fans::draw_runs(f, encoder, target, depth, after.iter().filter(|p| !(ps2 && p.3)));
    }
}

impl Fans {
    /// `runs` over `target`, in one pass, each by its own pipeline.
    fn draw_runs<'a>(f: &Fans, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, runs: impl Iterator<Item = &'a (Key, i32, std::ops::Range<u32>, bool)>) {
        let mut runs = runs.peekable();
        if runs.peek().is_none() {
            return;
        }
        let Some((buf, _)) = &f.instances else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fans"),
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
        pass.set_bind_group(0, &f.frame_bg, &[]);
        pass.set_vertex_buffer(0, buf.slice(..));
        let mut current = None;
        for (key, texture, range, _) in runs {
            if current != Some(*key) {
                pass.set_pipeline(&f.pipelines[key]);
                current = Some(*key);
            }
            pass.set_bind_group(1, &f.groups[texture], &[]);
            pass.draw(0..6, range.clone());
        }
    }
}
