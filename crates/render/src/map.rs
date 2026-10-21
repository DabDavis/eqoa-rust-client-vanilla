//! The client's map of the world (`eqoa/src/map`; the game has none): its
//! pictures drawn over the window as quads, each texture with a mip chain
//! made here and sampled trilinearly, so the map reads at any zoom
//! (`MapLayer`); and a tile of it, drawn from straight above by a
//! [`Renderer`] and read back to the CPU for the map's cache (`TileShot`).

use crate::{DEPTH_FORMAT, Draw, Renderer, TARGET_FORMAT, View};
use bytemuck::{Pod, Zeroable};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

/// One quad: its corners in window pixels (y down) and texture coordinates,
/// in fan order, and the colour its texels are multiplied by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapQuad {
    pub tex: usize,
    pub pos: [[f32; 2]; 4],
    pub uv: [[f32; 2]; 4],
    pub color: [f32; 4],
}

impl MapQuad {
    /// An axis-aligned rectangle (x0, y0, x1, y1) of the window showing the
    /// texture's (u0, v0, u1, v1).
    pub fn rect(tex: usize, r: [f32; 4], uv: [f32; 4], color: [f32; 4]) -> Self {
        MapQuad { tex, pos: [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]], uv: [[uv[0], uv[1]], [uv[2], uv[1]], [uv[2], uv[3]], [uv[0], uv[3]]], color }
    }
}

struct Tex {
    tex: wgpu::Texture,
    bg: wgpu::BindGroup,
    size: (u32, u32),
    levels: u32,
}

/// The map's textures and this frame's quads.
pub struct MapLayer {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
    textures: Vec<Option<Tex>>,
    pub quads: Vec<MapQuad>,
}

/// One mip level down: each texel the mean of the 2x2 it covers (an odd
/// side's last texel taken twice), rounded. RGBA8, rows top first.
pub fn half(w: u32, h: u32, rgba: &[u8]) -> (u32, u32, Vec<u8>) {
    let (hw, hh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; (hw * hh * 4) as usize];
    for y in 0..hh {
        for x in 0..hw {
            let (x0, y0) = ((2 * x).min(w - 1), (2 * y).min(h - 1));
            let (x1, y1) = ((2 * x + 1).min(w - 1), (2 * y + 1).min(h - 1));
            for c in 0..4 {
                let at = |xx: u32, yy: u32| rgba[((yy * w + xx) * 4 + c) as usize] as u32;
                let sum = at(x0, y0) + at(x1, y0) + at(x0, y1) + at(x1, y1);
                out[((y * hw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
            }
        }
    }
    (hw, hh, out)
}

/// How many mip levels a `w` x `h` texture has down to one texel.
pub fn full_chain(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

impl MapLayer {
    /// The layer, for a pass into a `format` target with a `DEPTH_FORMAT`
    /// depth attachment (neither tested nor written).
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("map"),
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
        // trilinear, clamped: a tile's edge does not wrap round to its other side
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("map"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("map.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("map"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("map"),
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
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                // colour only: the target's alpha stays the clear's 1
                targets: &[Some(wgpu::ColorTargetState { format, blend: crate::blend_state(3), write_mask: wgpu::ColorWrites::COLOR })],
            }),
            multiview_mask: None,
            cache: None,
        });
        MapLayer { layout, sampler, pipeline, textures: Vec::new(), quads: Vec::new() }
    }

    /// A `w` x `h` RGBA8 texture with `levels` mip levels, all of it
    /// transparent black until written (wgpu clears a texture it makes, so
    /// nothing the size of it passes through memory). Its index.
    pub fn texture(&mut self, device: &wgpu::Device, w: u32, h: u32, levels: u32) -> usize {
        let levels = levels.clamp(1, full_chain(w, h));
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("map"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("map"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        let t = Tex { tex, bg, size: (w, h), levels };
        match self.textures.iter().position(Option::is_none) {
            Some(k) => {
                self.textures[k] = Some(t);
                k
            }
            None => {
                self.textures.push(Some(t));
                self.textures.len() - 1
            }
        }
    }

    /// Writes `rgba` (`w` x `h`) at texel (x, y) of texture `tex`'s first
    /// level and the same region of every level below it, each made from
    /// the one above (`half`). The region must fall on whole texels at every
    /// level: x, y, w and h multiples of 2^(levels - 1).
    pub fn write(&self, queue: &wgpu::Queue, tex: usize, at: [u32; 2], w: u32, h: u32, rgba: &[u8]) {
        let Some(Some(t)) = self.textures.get(tex) else { return };
        let (mut px, mut lw, mut lh) = (rgba.to_vec(), w, h);
        for level in 0..t.levels {
            let d = 1 << level;
            if level > 0 {
                if lw < 2 && lh < 2 {
                    break;
                }
                let (a, b, c) = half(lw, lh, &px);
                (lw, lh, px) = (a, b, c);
            }
            if at[0] / d + lw > (t.size.0 >> level).max(1) || at[1] / d + lh > (t.size.1 >> level).max(1) {
                break;
            }
            write_level(queue, &t.tex, level, [at[0] / d, at[1] / d], lw, lh, &px);
        }
    }

    /// Lets go of texture `tex`; its index is used again.
    pub fn free(&mut self, tex: usize) {
        if let Some(t) = self.textures.get_mut(tex) {
            *t = None;
        }
    }

    /// A texture's size, if it is there.
    pub fn size(&self, tex: usize) -> Option<(u32, u32)> {
        self.textures.get(tex)?.as_ref().map(|t| t.size)
    }

    /// The frame's quads into `pass` (a `size` target, a `DEPTH_FORMAT`
    /// depth attachment), in the order made, each run of one texture one
    /// draw.
    pub fn draw<'a>(&'a self, device: &wgpu::Device, pass: &mut wgpu::RenderPass<'a>, size: (u32, u32)) {
        let quads: Vec<&MapQuad> = self.quads.iter().filter(|q| matches!(self.textures.get(q.tex), Some(Some(_)))).collect();
        if quads.is_empty() {
            return;
        }
        let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
        let mut v = Vec::with_capacity(quads.len() * 6);
        for q in &quads {
            let p = |i: usize| Vertex { pos: [q.pos[i][0] / w * 2.0 - 1.0, 1.0 - q.pos[i][1] / h * 2.0], uv: q.uv[i], color: q.color };
            v.extend([p(0), p(1), p(2), p(0), p(2), p(3)]);
        }
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("map"), contents: bytemuck::cast_slice(&v), usage: wgpu::BufferUsages::VERTEX });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, buf.slice(..));
        let mut i = 0;
        while i < quads.len() {
            let tex = quads[i].tex;
            let mut j = i;
            while j < quads.len() && quads[j].tex == tex {
                j += 1;
            }
            if let Some(Some(t)) = self.textures.get(tex) {
                pass.set_bind_group(0, &t.bg, &[]);
                pass.draw((i * 6) as u32..(j * 6) as u32, 0..1);
            }
            i = j;
        }
    }
}

fn write_level(queue: &wgpu::Queue, tex: &wgpu::Texture, level: u32, at: [u32; 2], w: u32, h: u32, rgba: &[u8]) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: level, origin: wgpu::Origin3d { x: at[0], y: at[1], z: 0 }, aspect: wgpu::TextureAspect::All },
        rgba,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
}

/// A tile drawn offscreen and read back: one at a time (`start`, then
/// `ready` on later frames until the copy has landed, then `take`), its
/// target, depth and readback buffer kept for the next.
pub struct TileShot {
    target: wgpu::Texture,
    depth: wgpu::TextureView,
    buffer: wgpu::Buffer,
    size: (u32, u32),
    row: u32,
    /// 0 idle, 1 mapping, 2 mapped, 3 failed
    state: Arc<AtomicU8>,
}

impl TileShot {
    /// A `w` x `h` RGBA8 target (the `Renderer` drawing into it must be made
    /// for `TARGET_FORMAT`).
    pub fn new(device: &wgpu::Device, w: u32, h: u32) -> Self {
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("map tile"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let row = (w * 4).div_ceil(256) * 256;
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("map tile readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        TileShot { target, depth: crate::depth_view(device, w, h), buffer, size: (w, h), row, state: Arc::new(AtomicU8::new(0)) }
    }

    /// Whether a shot is under way.
    pub fn busy(&self) -> bool {
        self.state.load(Ordering::Acquire) != 0
    }

    /// Draws `items` through `view` with `r` and starts the copy back.
    pub fn start(&self, device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer, view: &View, items: &[Draw]) {
        let mut enc = device.create_command_encoder(&Default::default());
        let tv = self.target.create_view(&Default::default());
        r.draw(queue, &mut enc, &tv, &self.depth, view, items);
        let (w, h) = self.size;
        enc.copy_texture_to_buffer(
            self.target.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &self.buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(self.row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        queue.submit([enc.finish()]);
        self.state.store(1, Ordering::Release);
        let state = self.state.clone();
        self.buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| state.store(if r.is_ok() { 2 } else { 3 }, Ordering::Release));
    }

    /// Whether the picture has come back (`Some(false)`: the copy failed and
    /// the shot is over), driving the device's callbacks without waiting.
    pub fn ready(&self, device: &wgpu::Device) -> Option<bool> {
        if self.state.load(Ordering::Acquire) == 1 {
            let _ = device.poll(wgpu::PollType::Poll);
        }
        match self.state.load(Ordering::Acquire) {
            2 => Some(true),
            3 => {
                self.state.store(0, Ordering::Release);
                Some(false)
            }
            _ => None,
        }
    }

    /// The picture, RGBA8 rows top first; the shot is over.
    pub fn take(&self) -> Vec<u8> {
        let (w, h) = self.size;
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        if let Ok(data) = self.buffer.slice(..).get_mapped_range() {
            for y in 0..h as usize {
                out.extend_from_slice(&data[y * self.row as usize..y * self.row as usize + w as usize * 4]);
            }
        }
        self.buffer.unmap();
        self.state.store(0, Ordering::Release);
        out
    }

    /// Waits for the picture (for a tool with nothing else to do).
    pub fn wait(&self, device: &wgpu::Device) -> Option<Vec<u8>> {
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        self.ready(device).filter(|&ok| ok).map(|_| self.take())
    }
}

#[cfg(test)]
mod tests {
    use super::{full_chain, half};

    /// Each texel of a level down is the rounded mean of the 2x2 above it; an
    /// odd side's last texel counts twice. The control: the top-left texel
    /// alone.
    #[test]
    fn a_level_down_is_the_mean_of_four() {
        let px: Vec<u8> = [[0, 10, 20, 255], [4, 10, 20, 255], [8, 10, 20, 255], [13, 10, 20, 255]].concat();
        let (w, h, out) = half(2, 2, &px);
        assert_eq!((w, h), (1, 1));
        // (0 + 4 + 8 + 13 + 2) / 4, the 2 rounding
        assert_eq!(&out[..4], &[(4 + 8 + 13 + 2) / 4, 10, 20, 255]);
        assert_ne!(out[0], px[0], "the top-left texel alone");
        // 3 wide: the last column taken twice
        let row = [[0u8, 0, 0, 0], [100, 0, 0, 0], [200, 0, 0, 0]].concat();
        let px = [row.clone(), row].concat();
        let (w, h, out) = half(3, 2, &px);
        assert_eq!((w, h), (1, 1));
        assert_eq!(out[0], 50);
        assert_eq!(full_chain(512, 512), 10);
        assert_eq!(full_chain(1664, 2176), 12);
        assert_eq!(full_chain(1, 1), 1);
    }
}
