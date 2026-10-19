//! The renderer, on wgpu.
//!
//! For now: models out of a container, textured from the same container, drawn
//! with a stand-in shader. The game's own vertex maths (the VU1 strip programs,
//! already redone as shaders in eqoa-port's `VI_HLE` path) replaces the shader
//! when the walk/camera slice needs it to match.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use wgpu::util::DeviceExt;

pub use wgpu;

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    uv: [f32; 2],
    normal: [f32; 3],
    color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniform {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MaterialUniform {
    tint: [f32; 4],
    flags: [f32; 4],
}

/// One model on the GPU.
pub struct GpuModel {
    vertices: wgpu::Buffer,
    batches: Vec<(wgpu::Buffer, u32, wgpu::BindGroup)>,
    pub bounds: (Vec3, Vec3),
}

pub struct Renderer {
    pipeline: wgpu::RenderPipeline,
    frame_buf: wgpu::Buffer,
    frame_bg: wgpu::BindGroup,
    material_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    white: wgpu::TextureView,
}

/// What to draw this frame.
pub struct View {
    pub view_proj: Mat4,
    pub light_dir: Vec3,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame"),
            entries: &[uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("material"),
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
                uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("model"),
            bind_group_layouts: &[Some(&frame_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("model"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3, 3 => Float32x3],
                })],
            },
            // Strips are flattened to lists; the game draws both faces of most
            // geometry (the GS has no culling), so no culling here either.
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let white = upload_rgba(device, queue, 1, 1, &[255; 4]);
        Renderer { pipeline, frame_buf, frame_bg, material_layout, sampler, white }
    }

    /// Uploads a model and the textures it names out of `c`.
    pub fn upload(&self, device: &wgpu::Device, queue: &wgpu::Queue, c: &esf::Container, m: &esf::Model) -> GpuModel {
        let verts: Vec<Vertex> = (0..m.pos.len())
            .map(|i| Vertex { pos: m.pos[i], uv: m.uv[i], normal: m.normal[i], color: m.color[i] })
            .collect();
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut batches = Vec::new();
        for b in &m.batches {
            if b.indices.is_empty() {
                continue;
            }
            let image = c.surface(b.texture()).and_then(|s| s.decode(&c.data));
            let view = image.as_ref().map(|img| upload_rgba(device, queue, img.width, img.height, &img.rgba));
            let tint = b
                .layers
                .iter()
                .find(|l| l.texture != 0)
                .map_or([1.0; 4], |l| l.color.map(|c| c as f32 / 128.0));
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("material"),
                contents: bytemuck::bytes_of(&MaterialUniform {
                    tint,
                    flags: [if view.is_some() { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
                }),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("material"),
                layout: &self.material_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(view.as_ref().unwrap_or(&self.white)),
                    },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
                ],
            });
            let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("indices"),
                contents: bytemuck::cast_slice(&b.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            batches.push((indices, b.indices.len() as u32, bg));
        }
        let lo = m.pos.iter().fold(Vec3::splat(f32::MAX), |a, p| a.min(Vec3::from(*p)));
        let hi = m.pos.iter().fold(Vec3::splat(f32::MIN), |a, p| a.max(Vec3::from(*p)));
        GpuModel { vertices, batches, bounds: (lo, hi) }
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
            light_dir: view.light_dir.normalize().extend(0.0).to_array(),
        };
        queue.write_buffer(&self.frame_buf, 0, bytemuck::bytes_of(&uniform));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("models"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.18, g: 0.2, b: 0.24, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.frame_bg, &[]);
        for m in models {
            pass.set_vertex_buffer(0, m.vertices.slice(..));
            for (indices, count, bg) in &m.batches {
                pass.set_bind_group(1, bg, &[]);
                pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..*count, 0, 0..1);
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
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
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
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
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
    renderer.draw(queue, &mut enc, &target.create_view(&Default::default()), &depth, view, models);
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
