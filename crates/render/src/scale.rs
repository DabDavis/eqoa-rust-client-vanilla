//! The options' Render Scale (the client's own; the PS2 drew its 640x448): the
//! whole frame, the world and the 2D layer over it, drawn `k` times the
//! target's size each way into a texture of its own with a depth buffer
//! beside it (`targets`), then each `k` by `k` block of it averaged into one
//! pixel of the target (`resolve`). That is supersampling: the edges, the
//! alpha-tested cut-outs and what is thinner than a pixel are smoothed, and
//! nothing else changes. A texture's level goes by depth, not by pixels
//! (`gs_lod_k`), so the levels drawn are the game's; the 2D layer, the labels
//! and the name plates are laid out against the frame, not the pixels
//! (`ui::frame`), so they land where they do at 1; and the options'
//! Brightness still goes over the world alone (`curve`, given the larger
//! size). At 1 there is no pass at all, and the frame is drawn exactly as
//! before. The average is of the stored bytes, rounded half up (scale.wgsl),
//! so it is the same on every GPU, WebGL2's included.

/// No side of the scale's texture is longer than this: wgpu's default limit,
/// which a device made natively here is given (`Limits::default`), kept in a
/// page as well, whose device takes the adapter's larger one. A frame then
/// stays within 8192 by 4608 at 16:9: 37.7 million pixels, 302 MB with its
/// depth buffer.
pub const MAX_SIDE: u32 = 8192;

/// The scale a `size` target is drawn at when `want` is asked: `want`, or
/// the largest below it whose texture keeps within `max` a side (the
/// device's limit and `MAX_SIDE`). Within 8192: x4 to a target 2048 long,
/// x3 to 2730, x2 to 4096.
pub fn fit(want: u32, size: (u32, u32), max: u32) -> u32 {
    let side = size.0.max(size.1).max(1);
    (1..=want.max(1)).rev().find(|&k| side.saturating_mul(k) <= max).unwrap_or(1)
}

/// The last frame's target size and the limit its scale was fitted to, by
/// any renderer (`would_draw`); none before the first.
static LAST_W: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static LAST_H: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static LAST_MAX: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(MAX_SIDE);

/// The scale a frame would be drawn at with `want` asked, at the last
/// frame's size and limit: `want`, or less where the window is too large for
/// it (the options' note, said as the row moves, before the next frame; a
/// report's options line). `want` before any frame is drawn.
pub fn would_draw(want: u32) -> u32 {
    use std::sync::atomic::Ordering::Relaxed;
    fit(want, (LAST_W.load(Relaxed), LAST_H.load(Relaxed)), LAST_MAX.load(Relaxed))
}

/// The scale asked, and the GPU's half once it is first wanted.
pub struct Scale {
    format: wgpu::TextureFormat,
    want: u32,
    gpu: Option<Gpu>,
    /// `targets` gave the scale's texture for this frame: `resolve` averages it
    pending: bool,
    /// the scale the last frame was drawn at (`drawn`)
    drawn: u32,
}

struct Gpu {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    frame: Option<Frame>,
}

/// The scale's texture for a target of `size` at `k`: its view, the depth
/// buffer beside it, and the bind group the average reads it through (with
/// `k` in its uniform).
struct Frame {
    size: (u32, u32),
    k: u32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    depth: wgpu::TextureView,
    bind: wgpu::BindGroup,
}

/// Where a frame is drawn (`Scale::targets`): the colour and depth targets,
/// their size in pixels, and the scale.
pub struct Targets {
    pub color: wgpu::TextureView,
    pub depth: wgpu::TextureView,
    pub size: (u32, u32),
    pub k: u32,
}

impl Scale {
    pub(crate) fn new(format: wgpu::TextureFormat) -> Self {
        Scale { format, want: 1, gpu: None, pending: false, drawn: 1 }
    }

    /// The scale asked: 1 (the frame drawn at the target's size), 2 or 4.
    /// At 1 the larger textures are let go.
    pub fn set(&mut self, k: u32) {
        self.want = k.max(1);
        if self.want == 1
            && let Some(g) = &mut self.gpu
        {
            g.frame = None;
        }
    }

    pub fn want(&self) -> u32 {
        self.want
    }

    /// The scale the last frame was drawn at: the one asked, or less where
    /// the target is too large for it (`fit`).
    pub fn drawn(&self) -> u32 {
        self.drawn
    }

    /// Where this frame is drawn for a `size` target: at a scale above 1, the
    /// scale's texture and depth buffer at that many times the size (made,
    /// or made again at a new size or scale), and that size; at 1 `target`
    /// and `depth` themselves.
    pub fn targets(&mut self, device: &wgpu::Device, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32)) -> Targets {
        let max = device.limits().max_texture_dimension_2d.min(MAX_SIDE);
        let k = fit(self.want, size, max);
        self.drawn = k;
        for (at, v) in [(&LAST_W, size.0), (&LAST_H, size.1), (&LAST_MAX, max)] {
            at.store(v, std::sync::atomic::Ordering::Relaxed);
        }
        if k == 1 {
            if let Some(g) = &mut self.gpu {
                g.frame = None;
            }
            self.pending = false;
            return Targets { color: target.clone(), depth: depth.clone(), size, k: 1 };
        }
        let format = self.format;
        let g = self.gpu.get_or_insert_with(|| Gpu::new(device, format));
        if g.frame.as_ref().is_none_or(|f| f.size != size || f.k != k) {
            // the last one let go before the next is made
            g.frame = None;
            g.frame = Some(Frame::new(device, &g.layout, format, size, k));
        }
        self.pending = true;
        let f = g.frame.as_ref().expect("made above");
        Targets { color: f.view.clone(), depth: f.depth.clone(), size: (size.0 * k, size.1 * k), k }
    }

    /// The scale's texture, while there is one (a test writes into it).
    pub fn frame_texture(&self) -> Option<&wgpu::Texture> {
        self.gpu.as_ref()?.frame.as_ref().map(|f| &f.texture)
    }

    /// The frame `targets` gave this frame, each `k` by `k` block of it
    /// averaged into a pixel of `target`, every pixel of it; nothing if it
    /// gave none (a scale of 1).
    pub fn resolve(&mut self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        if !std::mem::take(&mut self.pending) {
            return;
        }
        let Some(g) = self.gpu.as_ref() else { return };
        let Some(f) = &g.frame else { return };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("render scale"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&g.pipeline);
        pass.set_bind_group(0, &f.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl Frame {
    fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, format: wgpu::TextureFormat, size: (u32, u32), k: u32) -> Self {
        let extent = wgpu::Extent3d { width: size.0.max(1) * k, height: size.1.max(1) * k, depth_or_array_layers: 1 };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("render scale"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let depth = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("render scale depth"),
                size: extent,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: crate::DEPTH_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let uniform = wgpu::util::DeviceExt::create_buffer_init(
            device,
            &wgpu::util::BufferInitDescriptor { label: Some("render scale"), contents: bytemuck::cast_slice(&[k, 0, 0, 0]), usage: wgpu::BufferUsages::UNIFORM },
        );
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render scale"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: uniform.as_entire_binding() },
            ],
        });
        Frame { size, k, texture, view, depth, bind }
    }
}

impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("render scale"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                crate::uniform_entry(1, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("scale.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("render scale"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("render scale"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Gpu { layout, pipeline, frame: None }
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_SIDE, fit};

    /// The scale asked where it fits, else the largest below it that does;
    /// the long side decides. The control: the side alone, without the
    /// scale, would let x4 through at 2560.
    #[test]
    fn a_scale_keeps_within_the_limit() {
        assert_eq!(fit(4, (1920, 1080), MAX_SIDE), 4, "7680: within 8192");
        assert_eq!(fit(4, (2048, 1152), MAX_SIDE), 4, "exactly 8192");
        assert_eq!(fit(4, (2049, 1152), MAX_SIDE), 3);
        assert_eq!(fit(4, (2560, 1440), MAX_SIDE), 3, "7680 at x3; x4's 10240 is past it");
        assert_eq!(fit(4, (3840, 2160), MAX_SIDE), 2);
        assert_eq!(fit(2, (3840, 2160), MAX_SIDE), 2);
        assert_eq!(fit(2, (4097, 2160), MAX_SIDE), 1);
        assert_eq!(fit(4, (1080, 2400), MAX_SIDE), 3, "a phone held upright: its height");
        assert_eq!(fit(4, (640, 448), 2048), 3, "a smaller limit");
        assert_eq!(fit(1, (100, 100), MAX_SIDE), 1);
        assert_eq!(fit(0, (100, 100), MAX_SIDE), 1, "none asked is 1");
        assert_eq!(fit(4, (0, 0), MAX_SIDE), 4, "no size yet: as for one pixel");
        assert!(2560 <= MAX_SIDE && fit(4, (2560, 1440), MAX_SIDE) != 4, "the control");
    }
}
