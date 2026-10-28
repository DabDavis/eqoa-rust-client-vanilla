//! Particles, as the billboard program draws them (`VIRaster::BeginBillboards`
//! and the VU1 program `VIBbRender`, read in `decomp/vu`): a screen-aligned
//! quad per particle at its own view depth, its UVs one of seven
//! sprite-sheet layouts, its colour the particle's, the material's GS state
//! (blend mode, Z test and write) per emitter. Drawn after everything else,
//! as `VIParticleSystem::Render` runs at the end of the scene.

use crate::{DEPTH_FORMAT, Renderer, Textures};
use bytemuck::{Pod, Zeroable};
use glam::Mat4;

/// The 28 corner UVs of the seven layouts (`BeginBillboards`' constant
/// block), four a layout: (left, bottom), (left, top), (right, bottom),
/// (right, top). Layout 0 is the whole sheet, 1-4 its quadrants, 5-6 its
/// halves.
const UVS: [[f32; 2]; 28] = [
    [0.0, 1.0], [0.0, 0.0], [1.0, 1.0], [1.0, 0.0],
    [0.0, 0.5], [0.0, 0.0], [0.5, 0.5], [0.5, 0.0],
    [0.5, 0.5], [0.5, 0.0], [1.0, 0.5], [1.0, 0.0],
    [0.0, 1.0], [0.0, 0.5], [0.5, 1.0], [0.5, 0.5],
    [0.5, 1.0], [0.5, 0.5], [1.0, 1.0], [1.0, 0.5],
    [0.0, 1.0], [0.0, 0.0], [0.5, 1.0], [0.5, 0.0],
    [0.5, 1.0], [0.5, 0.0], [1.0, 1.0], [1.0, 0.0],
];

/// The pipeline a run takes as the GS draws its material: `key` is (blend,
/// Z test, Z write) as the material says, `textured` whether its texture is
/// there. `VIMaterial::ConstructTextPass` gives a textured layer that blends
/// the TEST that fails every pixel's alpha test and keeps its colour only
/// (ATST never, AFAIL RGB only), so it never writes Z whatever its Z-write
/// flag; a solid layer (`ConstructSolidFillPass`) and an opaque one write as
/// the flag says.
pub(crate) fn gs_key(key: (usize, bool, bool), textured: bool) -> (usize, bool, bool) {
    let (blend, ztest, zwrite) = key;
    (blend, ztest, zwrite && (blend == 0 || !textured))
}

/// One particle: its centre (world), half-size, frame (`m_frame << 2`, the
/// first of its four UVs) and colour.
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub center: [f32; 3],
    pub half: [f32; 2],
    pub frame: i32,
    pub color: [f32; 4],
}

/// An emitter's particles and its material.
#[derive(Clone, Debug)]
pub struct Batch {
    /// the texture's DictID (0 none: white)
    pub texture: i32,
    /// 0 opaque (alpha-tested), 1 add, 2 subtract, 3 alpha, 4 multiply
    pub blend: usize,
    pub ztest: bool,
    pub zwrite: bool,
    pub quads: Vec<Quad>,
}

/// Triangle strips in world space (`VIRaster::BeginTriStrip` as
/// `VILightning::Render` and `VITrailFx::Render` use it), one colour or a
/// colour per vertex, drawn through the billboard program with the corners
/// at zero.
#[derive(Clone, Debug)]
pub struct Strips {
    /// the texture's DictID (0 none: white)
    pub texture: i32,
    /// as `Batch::blend`
    pub blend: usize,
    pub ztest: bool,
    pub zwrite: bool,
    pub color: [f32; 4],
    /// each strip's vertices: (u, v), (x, y, z)
    pub strips: Vec<Vec<([f32; 2], [f32; 3])>>,
    /// a colour per vertex, in place of `color` (`VIRaster::Color` before
    /// each `Vertex`); empty for one colour
    pub colors: Vec<Vec<[f32; 4]>>,
}

/// A run of vertices to draw: the pipeline (blend, Z test, Z write), the
/// texture, the first vertex and the end.
type Run = ((usize, bool, bool), i32, u32, u32);

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vert {
    center: [f32; 3],
    corner: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameU {
    view: [[f32; 4]; 4],
    proj: [[f32; 4]; 4],
    flags: [f32; 4],
}

pub(crate) struct Billboards {
    shader: wgpu::ShaderModule,
    frame_layout: wgpu::BindGroupLayout,
    tex_layout: wgpu::BindGroupLayout,
    layout: wgpu::PipelineLayout,
    pipelines: std::collections::HashMap<(usize, bool, bool), wgpu::RenderPipeline>,
    sampler: wgpu::Sampler,
    /// what each of a frame's draws keeps from frame to frame (`Slot`)
    slots: [Option<Kept>; 4],
    /// a texture's bind group by its key (`Textures::key`; none: white),
    /// made once: the renderer's textures are never let go
    groups: std::collections::HashMap<Option<u64>, wgpu::BindGroup>,
}

/// A frame's draws through the billboard program, each with buffers of its
/// own kept between frames (made anew every frame, they were 31 uniform
/// buffers, 62 bind groups and a vertex buffer a frame under the Lavastorm's
/// fireballs, which in a page held it near 50 fps at Klick'Anon, F3
/// 20261027-025514). Each is drawn at most once a submit: its buffers are
/// written by the queue, which writes before the submit's commands run.
#[derive(Clone, Copy)]
enum Slot {
    /// the sky's billboards (stars, the weather's lightning patches)
    Sky = 0,
    /// the world's triangle strips (lightning, weapon trails)
    Strips = 1,
    /// the world's billboards (particles, rain, snow, fireballs)
    World = 2,
    /// the world's billboards drawn after the shadows (the weather's,
    /// `draw_late_billboards`)
    Late = 3,
}

/// A slot's vertex buffer (grown, never shrunk) and its two frame
/// uniforms with their bind groups: an opaque run's (blend 0) and the rest's.
struct Kept {
    verts: wgpu::Buffer,
    capacity: u64,
    frames: [(wgpu::Buffer, wgpu::BindGroup); 2],
}

impl Billboards {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("billboard.wgsl"));
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("billboard frame"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                count: None,
            }],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("billboard texture"),
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
            label: Some("billboards"),
            bind_group_layouts: &[Some(&frame_layout), Some(&tex_layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Billboards { shader, frame_layout, tex_layout, layout, pipelines: Default::default(), sampler, slots: [None, None, None, None], groups: Default::default() }
    }

    fn pipeline(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat, key: (usize, bool, bool)) -> &wgpu::RenderPipeline {
        let (blend, ztest, zwrite) = key;
        self.pipelines.entry(key).or_insert_with(|| {
            let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x2, 3 => Float32x4];
            let state = crate::blend_state(blend);
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("billboard"),
                layout: Some(&self.layout),
                vertex: wgpu::VertexState {
                    module: &self.shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vert>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs })],
                },
                primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(zwrite),
                    depth_compare: Some(if ztest { wgpu::CompareFunction::LessEqual } else { wgpu::CompareFunction::Always }),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &self.shader,
                    entry_point: Some("fs_main"),
                    compilation_options: Default::default(),
                    // colour only: the target's alpha stays 1 (render's model pipelines)
                    targets: &[Some(wgpu::ColorTargetState { format, blend: state, write_mask: wgpu::ColorWrites::COLOR })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })
    }
}

impl Renderer {
    /// The particles, over what `draw` drew (`view` and `proj` are the frame's
    /// own, not composed), in the order given; `strips` first.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_billboards(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        view: Mat4,
        proj: Mat4,
        batches: &[Batch],
        textures: &Textures,
    ) {
        self.billboards(device, queue, encoder, target, depth, view, proj, batches, textures, Slot::World);
    }

    /// The world's billboards after the shadows and the strips
    /// (`VIScene::Render`: the weather after `RenderShadows`, the spell
    /// effects and the trails), as `draw_billboards` draws them but through a
    /// buffer of their own.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_late_billboards(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        view: Mat4,
        proj: Mat4,
        batches: &[Batch],
        textures: &Textures,
    ) {
        self.billboards(device, queue, encoder, target, depth, view, proj, batches, textures, Slot::Late);
    }

    /// Billboards that belong to the sky (`VIAtmosphere::RenderStars`: the
    /// stars and the lightning patches, Z off, drawn before the world): at
    /// the far plane, so drawn after the world they still land only where
    /// nothing else did. `view` is the sky's, its translation zeroed.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_sky_billboards(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        view: Mat4,
        proj: Mat4,
        batches: &[Batch],
        textures: &Textures,
    ) {
        self.billboards(device, queue, encoder, target, depth, view, proj, batches, textures, Slot::Sky);
    }

    /// The billboards; `far` puts them at the far plane with Z off. The
    /// strips are not drawn here: the sky's view has no translation, so taken
    /// there they went where nothing is (F3 20261026-041245: no spell's bolt
    /// was ever seen in the world), and the world's are drawn after the
    /// particles and the shadows (`draw_queued_strips`).
    #[allow(clippy::too_many_arguments)]
    fn billboards(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        view: Mat4,
        proj: Mat4,
        batches: &[Batch],
        textures: &Textures,
        slot: Slot,
    ) {
        let far = matches!(slot, Slot::Sky);
        let mut verts: Vec<Vert> = Vec::new();
        let mut draws = Vec::new();
        for b in batches.iter().filter(|b| !b.quads.is_empty()) {
            let start = verts.len() as u32;
            for q in &b.quads {
                let k = (q.frame.clamp(0, 24) as usize) & !3;
                let [w, h] = q.half;
                // (left, bottom), (left, top), (right, bottom), (right, top)
                let corners = [[-w, -h], [-w, h], [w, -h], [w, h]];
                let v = |i: usize| Vert { center: q.center, corner: corners[i], uv: UVS[k + i], color: q.color };
                verts.extend([v(0), v(1), v(2), v(2), v(1), v(3)]);
            }
            let key = if far { (b.blend.min(4), true, false) } else { (b.blend.min(4), b.ztest, b.zwrite) };
            draws.push((key, b.texture, start, verts.len() as u32));
        }
        self.draw_verts(device, queue, encoder, target, depth, view, proj, &verts, &draws, textures, slot);
    }

    /// The strips queued for this frame (`strips`: the spell lightning, then
    /// the weapon trails), drawn and let go.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_queued_strips(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        view: Mat4,
        proj: Mat4,
        textures: &Textures,
    ) {
        let strips = std::mem::take(&mut self.strips);
        if !strips.is_empty() {
            self.draw_strips(device, queue, encoder, target, depth, view, proj, &strips, textures);
        }
    }

    /// Triangle strips (lightning), over what `draw` drew, in the order given.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_strips(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        view: Mat4,
        proj: Mat4,
        strips: &[Strips],
        textures: &Textures,
    ) {
        let mut verts: Vec<Vert> = Vec::new();
        let mut draws = Vec::new();
        for s in strips {
            let start = verts.len() as u32;
            for (k, strip) in s.strips.iter().enumerate() {
                let v = |i: usize| Vert { center: strip[i].1, corner: [0.0, 0.0], uv: strip[i].0, color: s.colors.get(k).and_then(|c| c.get(i)).copied().unwrap_or(s.color) };
                for i in 2..strip.len() {
                    verts.extend([v(i - 2), v(i - 1), v(i)]);
                }
            }
            if verts.len() as u32 > start {
                draws.push(((s.blend.min(4), s.ztest, s.zwrite), s.texture, start, verts.len() as u32));
            }
        }
        self.draw_verts(device, queue, encoder, target, depth, view, proj, &verts, &draws, textures, Slot::Strips);
    }

    /// The billboard program over `verts`, a run of them per (pipeline key,
    /// texture), through `slot`'s kept buffers; the sky's at the far plane.
    #[allow(clippy::too_many_arguments)]
    fn draw_verts(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        view: Mat4,
        proj: Mat4,
        verts: &[Vert],
        runs: &[Run],
        textures: &Textures,
        slot: Slot,
    ) {
        if runs.is_empty() || verts.is_empty() {
            return;
        }
        let far = matches!(slot, Slot::Sky);
        let mut draws = Vec::new();
        for &(key, texture, start, end) in runs {
            let tex = self.texture(device, queue, texture, textures);
            let key = gs_key(key, tex.is_some());
            draws.push((key, start, end, tex.is_some().then(|| textures.key(texture)), tex));
        }
        let format = self.format;
        let white = self.white.clone();
        let bb = self.billboards.get_or_insert_with(|| Billboards::new(device));
        let bytes: &[u8] = bytemuck::cast_slice(verts);
        let size = bytes.len() as u64;
        let kept = &mut bb.slots[slot as usize];
        if kept.as_ref().is_none_or(|k| k.capacity < size) {
            let capacity = size.next_power_of_two().max(64 << 10);
            let verts = device.create_buffer(&wgpu::BufferDescriptor { label: Some("billboards"), size: capacity, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            let frames = match kept.take() {
                Some(k) => k.frames,
                None => [0, 1].map(|_| {
                    let ub = device.create_buffer(&wgpu::BufferDescriptor { label: Some("billboard frame"), size: std::mem::size_of::<FrameU>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bb.frame_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: ub.as_entire_binding() }] });
                    (ub, bg)
                }),
            };
            *kept = Some(Kept { verts, capacity, frames });
        }
        let Some(kept) = bb.slots[slot as usize].as_ref() else { return };
        queue.write_buffer(&kept.verts, 0, bytes);
        // an opaque run alpha-tests at 128 (the TEST of an unblended layer)
        for (i, opaque) in [true, false].into_iter().enumerate() {
            if draws.iter().any(|d| (d.0.0 == 0) == opaque) {
                let u = FrameU { view: view.to_cols_array_2d(), proj: proj.to_cols_array_2d(), flags: [if opaque { 1.0 } else { 0.5 }, opaque as i32 as f32, far as i32 as f32, crate::effects_lod_k()] };
                queue.write_buffer(&kept.frames[i].0, 0, bytemuck::bytes_of(&u));
            }
        }
        for (_, _, _, id, tex) in &draws {
            if !bb.groups.contains_key(id) {
                let tbg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &bb.tex_layout,
                    entries: &[
                        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(tex.as_ref().unwrap_or(&white)) },
                        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&bb.sampler) },
                    ],
                });
                bb.groups.insert(*id, tbg);
            }
        }
        for (key, _, _, _, _) in &draws {
            bb.pipeline(device, format, *key);
        }
        let kept = bb.slots[slot as usize].as_ref().expect("made above");
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("billboards"),
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
        pass.set_vertex_buffer(0, kept.verts.slice(..size));
        for (key, start, end, id, _) in &draws {
            pass.set_pipeline(&bb.pipelines[key]);
            pass.set_bind_group(0, &kept.frames[(key.0 != 0) as usize].1, &[]);
            pass.set_bind_group(1, &bb.groups[id], &[]);
            pass.draw(*start..*end, 0..1);
        }
    }
}
