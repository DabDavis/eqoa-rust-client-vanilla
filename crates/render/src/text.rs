//! Text: a font packed into one texture, and labels drawn as screen-space
//! glyph quads the way `VIRaster::DrawCenteredString` lays them out (each
//! glyph its width, one pixel apart, centred on a projected point), in the
//! PS2's 640x448 pixels scaled to the target's height.

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

/// The PS2 frame the game lays text out on.
pub const SCREEN_H: f32 = 448.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
}

/// One string to draw: centred on `at` (clip space after the divide: x, y
/// in -1..1, z the depth), in `color`.
#[derive(Clone, Debug)]
pub struct Label {
    pub at: [f32; 3],
    pub color: [f32; 4],
    pub text: String,
}

struct Glyph {
    width: u32,
    uv: [f32; 4],
}

pub struct Text {
    /// The glyphs' quads under the options' Text (`ui::TextFilter`), Sharp
    /// and Smooth, both through the filtering sampler (`bg`).
    sharp: wgpu::RenderPipeline,
    smooth: wgpu::RenderPipeline,
    bg: wgpu::BindGroup,
    glyphs: HashMap<u16, Glyph>,
    height: u32,
    /// The screen's text (the chat, the target's name), drawn over the
    /// whole frame (`Renderer::draw_text`).
    pub labels: Vec<Label>,
    /// The name plates, drawn in the scene where `VIScene::Render` draws
    /// them (`VINameSprite::RasterDeferred`): after the world, before the
    /// particles, the lightning, the trails and the weather
    /// (`Renderer::draw_plates`).
    pub plates: Vec<Label>,
    /// The plates' size against the game's (1: the PS2's 448 lines to the
    /// target's height, as every label), one for all of them: the zoom's
    /// (the client's `lod::plate_scale`).
    pub plate_scale: f32,
    /// A texel of the atlas in uv, across and down: each glyph's quad is
    /// drawn a texel wider all round, onto its clear ring (`ui::pack`).
    texel: [f32; 2],
}

impl Text {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat, depth: wgpu::TextureFormat, font: &esf::font::Font) -> Self {
        // Pack the glyphs into rows of a 512-wide atlas, each with a clear
        // texel round it (`ui::pack`).
        let w = 512u32;
        let (atlas, ah, packed) = crate::ui::pack(font, w);
        let texel = [1.0 / w as f32, 1.0 / ah as f32];
        let glyphs: HashMap<u16, Glyph> = packed.into_iter().map(|(c, (width, uv))| (c, Glyph { width, uv })).collect();
        let size = wgpu::Extent3d { width: w, height: ah, depth_or_array_layers: 1 };
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("font"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(tex.as_image_copy(), &atlas, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(ah) }, size);
        let view = tex.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("font"),
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
            ],
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("font"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("text.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("text"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let make = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("text"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x4],
                    })],
                },
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: depth,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend: Some(wgpu::BlendState::ALPHA_BLENDING), write_mask: wgpu::ColorWrites::COLOR })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let (sharp, smooth) = (make("fs_sharp"), make("fs_smooth"));
        Text { sharp, smooth, bg, glyphs, height: font.height, labels: Vec::new(), plates: Vec::new(), plate_scale: 1.0, texel }
    }

    /// `GetStringWidth`, in PS2 pixels.
    pub fn width(&self, s: &str) -> u32 {
        s.chars().filter_map(|c| self.glyphs.get(&(c as u16))).map(|g| g.width + 1).sum()
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// `labels`' quads for a target `size` (pixels), at `scale` of the
    /// game's size.
    fn vertices(&self, labels: &[Label], size: (u32, u32), scale: f32) -> Vec<Vertex> {
        let (tw, th) = (size.0 as f32, size.1 as f32);
        let k = th / SCREEN_H * scale;
        let mut out = Vec::new();
        for l in labels {
            // The centre in target pixels, then the string's left edge.
            let cx = (l.at[0] * 0.5 + 0.5) * tw;
            let cy = (-l.at[1] * 0.5 + 0.5) * th;
            let mut x = cx - (self.width(&l.text) / 2) as f32 * k;
            let y = cy - (self.height / 2) as f32 * k;
            for ch in l.text.chars() {
                let Some(g) = self.glyphs.get(&(ch as u16)) else { continue };
                let x1 = x + g.width as f32 * k;
                let y1 = y + self.height as f32 * k;
                let ndc = |px: f32, py: f32| [px / tw * 2.0 - 1.0, 1.0 - py / th * 2.0, l.at[2]];
                let v = |p: [f32; 3], u: f32, w: f32| Vertex { pos: p, uv: [u, w], color: l.color };
                // a texel wider all round, onto the glyph's clear ring: the
                // Sharp and Smooth mixes at its edges reach past them, and
                // drawn only to them an edge column lost that light (`ui`'s
                // `ringed`); the advance stays the game's
                let (tu, tv) = (self.texel[0], self.texel[1]);
                let (a, b, c, d) = (
                    v(ndc(x - k, y - k), g.uv[0] - tu, g.uv[1] - tv),
                    v(ndc(x1 + k, y - k), g.uv[2] + tu, g.uv[1] - tv),
                    v(ndc(x1 + k, y1 + k), g.uv[2] + tu, g.uv[3] + tv),
                    v(ndc(x - k, y1 + k), g.uv[0] - tu, g.uv[3] + tv),
                );
                out.extend([a, b, c, a, c, d]);
                x = x1 + k;
            }
        }
        out
    }

    /// Draws the labels into `pass`.
    pub fn draw<'a>(&'a self, device: &wgpu::Device, pass: &mut wgpu::RenderPass<'a>, size: (u32, u32)) {
        self.draw_list(device, pass, &self.labels, size, 1.0);
    }

    /// Draws the name plates into `pass`, at `plate_scale`.
    pub fn draw_plates<'a>(&'a self, device: &wgpu::Device, pass: &mut wgpu::RenderPass<'a>, size: (u32, u32)) {
        self.draw_list(device, pass, &self.plates, size, self.plate_scale);
    }

    fn draw_list<'a>(&'a self, device: &wgpu::Device, pass: &mut wgpu::RenderPass<'a>, labels: &[Label], size: (u32, u32), scale: f32) {
        let v = self.vertices(labels, size, scale);
        if v.is_empty() {
            return;
        }
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("text"), contents: bytemuck::cast_slice(&v), usage: wgpu::BufferUsages::VERTEX });
        pass.set_pipeline(match crate::ui::text_filter() {
            crate::ui::TextFilter::Sharp => &self.sharp,
            crate::ui::TextFilter::Smooth => &self.smooth,
        });
        pass.set_bind_group(0, &self.bg, &[]);
        pass.set_vertex_buffer(0, buf.slice(..));
        pass.draw(0..v.len() as u32, 0..1);
    }
}
