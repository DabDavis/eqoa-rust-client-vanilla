//! The 2D layer `VIRaster` draws between `Begin2D` and `End2D`: textured
//! quads in the PS2's 640x448 pixels (`Blit`, `StretchBlit`, a font's
//! glyphs, the HUD's own fans and strips), each with a blend mode and a
//! colour, drawn in the order they were made, scaled to the target. Blend
//! modes are the material layer's (0 alpha tested at 128, 1 add, 2
//! subtract, 3 alpha, 4 multiply), with the same GS arithmetic as the 3D
//! path: texture times colour, alpha as stored.
//!
//! A font is packed into one texture (`add_font`) and laid out as
//! `VIRaster::DrawString` lays it out: each glyph its width wide and the
//! font's height tall, one pixel apart.

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use wgpu::util::DeviceExt;

pub const SCREEN_W: f32 = 640.0;
pub const SCREEN_H: f32 = 448.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

/// One quad: its corners in screen pixels and texture coordinates, in fan
/// order (the first, second, third and fourth vertex the game sends).
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub tex: usize,
    pub blend: u8,
    pub pos: [[f32; 2]; 4],
    pub uv: [[f32; 2]; 4],
    pub color: [f32; 4],
    /// The viewport it was drawn under (`VIRaster::Viewport`), screen
    /// pixels x0, y0, x1, y1 (exclusive); None for the whole screen.
    pub clip: Option<[i32; 4]>,
}

struct Texture {
    bg: wgpu::BindGroup,
    width: u32,
    height: u32,
}

/// A font packed into one of the layer's textures.
pub struct Font {
    pub tex: usize,
    pub height: u32,
    glyphs: HashMap<u16, (u32, [f32; 4])>,
}

impl Font {
    /// `FindChar`, with `'?'` for a character the font lacks.
    fn find(&self, code: u16) -> Option<&(u32, [f32; 4])> {
        self.glyphs.get(&code).or_else(|| self.glyphs.get(&(b'?' as u16)))
    }

    /// `VIRFont::GetStringWidth(s, 1)`: each glyph's width plus one; -1
    /// when even `'?'` is missing.
    pub fn string_width(&self, s: &[u16]) -> i32 {
        let mut w = 0;
        for &c in s {
            let Some(g) = self.find(c) else { return -1 };
            w += g.0 as i32 + 1;
        }
        w
    }
}

pub struct Ui {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipelines: Vec<wgpu::RenderPipeline>,
    textures: Vec<Texture>,
    pub fonts: Vec<Font>,
    pub quads: Vec<Quad>,
    /// `VIRaster::Viewport(x1, y1, x2, y2)` (inclusive) as quads are made:
    /// they are placed relative to (x1, y1) and cut to the rectangle, as the
    /// GS's offset and scissor do. None is the whole screen.
    pub viewport: Option<[i32; 4]>,
}

impl Ui {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, depth: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui"),
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
        // Texels to pixels one for one at 640x448, so the nearest texel;
        // a rotated quad (the compass) the GS also samples point.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("ui.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ui"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipelines = (0..5)
            .map(|mode| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("ui"),
                    layout: Some(&pl),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4],
                        })],
                    },
                    primitive: Default::default(),
                    depth_stencil: Some(wgpu::DepthStencilState {
                        format: depth,
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
                        targets: &[Some(wgpu::ColorTargetState { format, blend: crate::blend_state(mode), write_mask: wgpu::ColorWrites::COLOR })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            })
            .collect();
        Ui { layout, sampler, pipelines, textures: Vec::new(), fonts: Vec::new(), quads: Vec::new(), viewport: None }
    }

    /// A texture for the layer: RGBA8, top row first. Its index.
    pub fn add_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) -> usize {
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(tex.as_image_copy(), rgba, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) }, size);
        let view = tex.create_view(&Default::default());
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ui"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        self.textures.push(Texture { bg, width, height });
        self.textures.len() - 1
    }

    /// A texture's size.
    pub fn texture_size(&self, tex: usize) -> (u32, u32) {
        let t = &self.textures[tex];
        (t.width, t.height)
    }

    /// A font, packed into rows of a 2048-wide texture. Its index.
    pub fn add_font(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, font: &esf::font::Font) -> usize {
        let (w, h) = (2048u32, font.height);
        let (mut x, mut row) = (0u32, 0u32);
        let mut places = Vec::new();
        for g in &font.glyphs {
            if x + g.width as u32 > w {
                x = 0;
                row += 1;
            }
            places.push((x, row));
            x += g.width as u32 + 1;
        }
        let ah = (row + 1) * (h + 1);
        let mut atlas = vec![0u8; (w * ah * 4) as usize];
        let mut glyphs = HashMap::new();
        for (g, &(gx, gr)) in font.glyphs.iter().zip(&places) {
            let gy = gr * (h + 1);
            for y in 0..h {
                for xx in 0..g.width as u32 {
                    let s = ((y * g.width as u32 + xx) * 4) as usize;
                    let d = (((gy + y) * w + gx + xx) * 4) as usize;
                    atlas[d..d + 4].copy_from_slice(&g.rgba[s..s + 4]);
                }
            }
            let uv = [gx as f32 / w as f32, gy as f32 / ah as f32, (gx + g.width as u32) as f32 / w as f32, (gy + h) as f32 / ah as f32];
            glyphs.entry(g.code).or_insert((g.width as u32, uv));
        }
        let tex = self.add_texture(device, queue, w, ah, &atlas);
        self.fonts.push(Font { tex, height: h, glyphs });
        self.fonts.len() - 1
    }

    /// `VIRaster::DrawString`: the glyphs from (x, y), each its width, one
    /// pixel apart, alpha-blended in `color` (`'?'` for a character the
    /// font lacks; nothing when it lacks that too).
    pub fn text(&mut self, font: usize, x: i32, y: i32, color: [f32; 4], s: &[u16]) {
        let f = &self.fonts[font];
        let (tex, h) = (f.tex, f.height as f32);
        let mut x0 = x as f32;
        let y0 = y as f32;
        let mut quads = Vec::new();
        for &c in s {
            let Some(&(w, uv)) = f.find(c) else { return };
            let x1 = x0 + w as f32;
            quads.push(Quad {
                tex,
                blend: 3,
                pos: [[x0, y0], [x1, y0], [x1, y0 + h], [x0, y0 + h]],
                uv: [[uv[0], uv[1]], [uv[2], uv[1]], [uv[2], uv[3]], [uv[0], uv[3]]],
                color,
                clip: None,
            });
            x0 = x1 + 1.0;
        }
        for q in quads {
            self.push(q);
        }
    }

    /// A quad, placed in and cut to the current viewport.
    pub fn push(&mut self, mut q: Quad) {
        if let Some([x1, y1, x2, y2]) = self.viewport {
            for p in &mut q.pos {
                p[0] += x1 as f32;
                p[1] += y1 as f32;
            }
            q.clip = Some([x1, y1, x2 + 1, y2 + 1]);
        } else {
            q.clip = None;
        }
        self.quads.push(q);
    }

    /// Draws the frame's quads into `pass` (a `size` target) and keeps them
    /// (the caller clears `quads` when it rebuilds them).
    pub fn draw<'a>(&'a self, device: &wgpu::Device, pass: &mut wgpu::RenderPass<'a>, size: (u32, u32)) {
        if self.quads.is_empty() {
            return;
        }
        // The frame keeps its 4:3 shape (a PS2 shows it on a 4:3 picture):
        // the full height, in the middle of a wider target (eqoa-port's
        // widescreen does the same with the HUD).
        let (tw, th) = (size.0 as f32, size.1 as f32);
        let fw = (th * 4.0 / 3.0).min(tw);
        let fh = if fw < th * 4.0 / 3.0 { fw * 3.0 / 4.0 } else { th };
        let (ox, oy) = ((tw - fw) / 2.0, (th - fh) / 2.0);
        let (sx, sy) = (fw / SCREEN_W, fh / SCREEN_H);
        let mut v = Vec::with_capacity(self.quads.len() * 6);
        for q in &self.quads {
            let p = |i: usize| Vertex {
                pos: [(ox + q.pos[i][0] * sx) / tw * 2.0 - 1.0, 1.0 - (oy + q.pos[i][1] * sy) / th * 2.0],
                uv: q.uv[i],
                color: q.color,
            };
            v.extend([p(0), p(1), p(2), p(0), p(2), p(3)]);
        }
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("ui"), contents: bytemuck::cast_slice(&v), usage: wgpu::BufferUsages::VERTEX });
        pass.set_vertex_buffer(0, buf.slice(..));
        // Runs of one texture, blend mode and viewport, in order.
        let mut i = 0;
        while i < self.quads.len() {
            let (tex, blend, clip) = (self.quads[i].tex, self.quads[i].blend, self.quads[i].clip);
            let mut j = i;
            while j < self.quads.len() && self.quads[j].tex == tex && self.quads[j].blend == blend && self.quads[j].clip == clip {
                j += 1;
            }
            let [x0, y0, x1, y1] = clip.unwrap_or([0, 0, SCREEN_W as i32, SCREEN_H as i32]);
            let px = |v: i32, o: f32, k: f32, max: u32| ((o + v.max(0) as f32 * k) as u32).min(max);
            let (a, b) = (px(x0, ox, sx, size.0), px(y0, oy, sy, size.1));
            let (c, d) = (px(x1, ox, sx, size.0), px(y1, oy, sy, size.1));
            if c <= a || d <= b {
                i = j;
                continue;
            }
            pass.set_scissor_rect(a, b, c - a, d - b);
            pass.set_pipeline(&self.pipelines[blend.min(4) as usize]);
            pass.set_bind_group(0, &self.textures[tex].bg, &[]);
            pass.draw((i * 6) as u32..(j * 6) as u32, 0..1);
            i = j;
        }
        pass.set_scissor_rect(0, 0, size.0, size.1);
    }
}
