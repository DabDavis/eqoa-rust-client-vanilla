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
            ..Default::default()
        });
        Billboards { shader, frame_layout, tex_layout, layout, pipelines: Default::default(), sampler }
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
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: state,
                        write_mask: if state.is_some() { wgpu::ColorWrites::COLOR } else { wgpu::ColorWrites::ALL },
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })
    }
}

impl Renderer {
    /// The particles, over what `draw` drew (`view` and `proj` are the frame's
    /// own, not composed), in the order given.
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
        use wgpu::util::DeviceExt;
        if batches.iter().all(|b| b.quads.is_empty()) {
            return;
        }
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
            let tex = self.texture(device, queue, b.texture, textures).unwrap_or_else(|| self.white.clone());
            draws.push((b, start, verts.len() as u32, tex));
        }
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("billboards"), contents: bytemuck::cast_slice(&verts), usage: wgpu::BufferUsages::VERTEX });
        let format = self.format;
        let bb = self.billboards.get_or_insert_with(|| Billboards::new(device));
        let mut groups = Vec::new();
        for (b, _, _, tex) in &draws {
            let u = FrameU { view: view.to_cols_array_2d(), proj: proj.to_cols_array_2d(), flags: [if b.blend == 0 { 1.0 } else { 0.5 }, (b.blend == 0) as i32 as f32, 0.0, 0.0] };
            let ub = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("billboard frame"), contents: bytemuck::bytes_of(&u), usage: wgpu::BufferUsages::UNIFORM });
            let fbg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &bb.frame_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: ub.as_entire_binding() }] });
            let tbg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &bb.tex_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(tex) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&bb.sampler) }],
            });
            groups.push((fbg, tbg));
        }
        for (b, _, _, _) in &draws {
            bb.pipeline(device, format, (b.blend.min(4), b.ztest, b.zwrite));
        }
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
        pass.set_vertex_buffer(0, vb.slice(..));
        for ((b, start, end, _), (fbg, tbg)) in draws.iter().zip(&groups) {
            pass.set_pipeline(&bb.pipelines[&(b.blend.min(4), b.ztest, b.zwrite)]);
            pass.set_bind_group(0, fbg, &[]);
            pass.set_bind_group(1, tbg, &[]);
            pass.draw(*start..*end, 0..1);
        }
    }
}
