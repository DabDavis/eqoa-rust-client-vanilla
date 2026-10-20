//! Triangles drawn over the finished scene with the Z test off: what a
//! material whose `SetZTest(0)` the caller made draws after `VIScene::Render`
//! (`VIClient::TrackTarget`'s selection rings). The caller hands world-space
//! triangles, each corner with its UV (the layer's transform applied) and its
//! colour (the layer colour times the strip program's light, 0..1 with 1 the
//! GS's 128), and the layer's texture, blend mode and wrap. They are drawn in
//! the order given, with the 3D path's arithmetic (texture times colour, alpha
//! as stored times the colour's; no fog), after the labels and before the 2D
//! layer (`Renderer::draw_text`), and stay until the next `set_overlay`.

use crate::{DEPTH_FORMAT, Renderer, Textures};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec4};
use wgpu::util::DeviceExt;

/// A triangle's corner: world position, UV, colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Corner {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// One triangle of one layer pass.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tri {
    /// the layer's texture DictID (0 or missing: white)
    pub texture: i32,
    /// 0 alpha tested at 128, 1 add, 2 subtract, 3 alpha, 4 multiply
    pub blend: usize,
    /// the layer's wrap: 0 repeat, 1 and 2 clamp, 3 mirror
    pub wrap: i32,
    pub corners: [Corner; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vert {
    clip: [f32; 4],
    uv: [f32; 2],
    color: [f32; 4],
}

pub(crate) struct Overlay {
    layout: wgpu::BindGroupLayout,
    /// by blend mode
    pipelines: Vec<wgpu::RenderPipeline>,
    /// this frame's vertices, and its runs: blend mode, texture and sampler,
    /// the vertices
    vertices: Option<wgpu::Buffer>,
    runs: Vec<(usize, wgpu::BindGroup, std::ops::Range<u32>)>,
}

impl Overlay {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay"),
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
        let shader = device.create_shader_module(wgpu::include_wgsl!("overlay.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("overlay"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipelines = (0..5)
            .map(|mode| {
                let blend = crate::blend_state(mode);
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("overlay"),
                    layout: Some(&pl),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Vert>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2, 2 => Float32x4],
                        })],
                    },
                    // a camera-facing quad's winding depends on the mirror in
                    // its billboard, so neither face is culled
                    primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                    // Z test off; a blended layer writes no depth (TEST's
                    // AFAIL = RGB_ONLY), and nothing drawn after reads it
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: DEPTH_FORMAT,
                        depth_write_enabled: Some(false),
                        depth_compare: Some(wgpu::CompareFunction::Always),
                        stencil: Default::default(),
                        bias: Default::default(),
                    }),
                    multisample: Default::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some(if mode == 0 { "fs_tested" } else { "fs_main" }),
                        compilation_options: Default::default(),
                        // colour only: the target's alpha stays 1 (render's model pipelines)
                        targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::COLOR })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            })
            .collect();
        Overlay { layout, pipelines, vertices: None, runs: Vec::new() }
    }

    /// This frame's runs into `pass`.
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        let Some(vb) = &self.vertices else { return };
        pass.set_vertex_buffer(0, vb.slice(..));
        for (blend, bg, range) in &self.runs {
            pass.set_pipeline(&self.pipelines[*blend]);
            pass.set_bind_group(0, bg, &[]);
            pass.draw(range.clone(), 0..1);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

/// A corner through `view_proj`, into clip space.
pub fn clip_of(view_proj: Mat4, p: [f32; 3]) -> [f32; 4] {
    (view_proj * Vec4::new(p[0], p[1], p[2], 1.0)).to_array()
}

impl Renderer {
    /// The triangles drawn over this frame's scene (`draw_text` draws them),
    /// through `view_proj`, in the order given; none clears the last frame's.
    pub fn set_overlay(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, view_proj: Mat4, tris: &[Tri], textures: &Textures) {
        if tris.is_empty() {
            if let Some(o) = self.overlay.as_mut() {
                o.vertices = None;
                o.runs.clear();
            }
            return;
        }
        // Runs of one blend mode, texture and wrap, in order.
        let mut runs: Vec<(usize, i32, usize, std::ops::Range<u32>)> = Vec::new();
        let mut verts = Vec::with_capacity(tris.len() * 3);
        for t in tris {
            let blend = t.blend.min(4);
            let si = match t.wrap {
                1 | 2 => 1,
                3 => 2,
                _ => 0,
            };
            let at = verts.len() as u32;
            verts.extend(t.corners.iter().map(|c| Vert { clip: clip_of(view_proj, c.pos), uv: c.uv, color: c.color }));
            match runs.last_mut() {
                Some(r) if r.0 == blend && r.1 == t.texture && r.2 == si => r.3.end = at + 3,
                _ => runs.push((blend, t.texture, si, at..at + 3)),
            }
        }
        let views: Vec<wgpu::TextureView> = runs.iter().map(|r| self.texture(device, queue, r.1, textures).unwrap_or_else(|| self.white.clone())).collect();
        let format = self.format;
        let samplers = &self.samplers;
        let o = self.overlay.get_or_insert_with(|| Overlay::new(device, format));
        o.runs = runs
            .into_iter()
            .zip(&views)
            .map(|((blend, _, si, range), view)| {
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("overlay"),
                    layout: &o.layout,
                    entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&samplers[si]) }],
                });
                (blend, bg, range)
            })
            .collect();
        o.vertices = Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("overlay"), contents: bytemuck::cast_slice(&verts), usage: wgpu::BufferUsages::VERTEX }));
    }
}
