//! A texture pack's stand-in that is still being read (`eqoa`'s `texpack`
//! reads and decodes it on a thread of its own): its texture is made at once
//! at the stand-in's size and level count (`Pack::request` knows them from
//! the file's header), and the surface's own levels are drawn into each of
//! its levels by the GPU (`placeholder`), so the game's own texture shows
//! until the stand-in is read. Then its levels are written over it at a
//! frame's start (`Renderer::take_stand_ins`, `BUDGET` bytes a frame), into
//! the same texture: every bind group and render bundle made with it stays
//! as it is, so nothing is made again and no frame waits for a file.

use std::sync::Arc;

/// The bytes of stand-ins written into their textures a frame, one stand-in
/// at least however large (a few milliseconds of copying at most: a
/// 1024 x 1024 stand-in with its levels is 5.6 MB).
pub const BUDGET: usize = 8 << 20;

/// What draws a placeholder's levels: one triangle over a level, the
/// surface's own levels read bilinear.
pub struct Blit {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
}

impl Blit {
    pub fn new(device: &wgpu::Device) -> Blit {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("stand-in"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
                crate::uniform_entry(2, wgpu::ShaderStages::FRAGMENT),
            ],
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("standin.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("stand-in"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("stand-in"),
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[] },
            primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: crate::TEXTURE_FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        // wrapping, as the surface is drawn: a tiling texture's edges meet
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("stand-in"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        Blit { layout, pipeline, sampler }
    }
}

/// The level of a surface's own (`own`, the base's size) a placeholder's
/// level of `to` is drawn from: the one nearest its size, the base for any
/// level larger than it (magnified).
pub fn source_level(own: (u32, u32), to: (u32, u32)) -> f32 {
    let r = (own.0 as f32 / to.0.max(1) as f32).max(own.1 as f32 / to.1.max(1) as f32);
    if r <= 1.0 { 0.0 } else { r.log2().round() }
}

/// A level's size in a texture of base `w` x `h`.
pub fn level_size(w: u32, h: u32, k: u32) -> (u32, u32) {
    ((w >> k).max(1), (h >> k).max(1))
}

/// The texture of a stand-in being read, `w` x `h` with `n` levels, each
/// level the surface's own levels (`own`) drawn into it (`source_level`):
/// the texture, and its view to bind.
pub fn placeholder(device: &wgpu::Device, queue: &wgpu::Queue, blit: &Blit, own: &[esf::Image], (w, h, n): (u32, u32, u32)) -> (wgpu::Texture, wgpu::TextureView) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("stand-in"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: n.max(1),
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: crate::TEXTURE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = tex.create_view(&Default::default());
    let Some(base) = own.first() else { return (tex, view) };
    let src = crate::upload_levels(device, queue, own);
    let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("stand-in") });
    for k in 0..n.max(1) {
        let level = [source_level((base.width, base.height), level_size(w, h, k)).min((own.len() - 1) as f32), 0.0, 0.0, 0.0];
        let uniform = wgpu::util::DeviceExt::create_buffer_init(device, &wgpu::util::BufferInitDescriptor { label: Some("stand-in"), contents: bytemuck::cast_slice(&level), usage: wgpu::BufferUsages::UNIFORM });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stand-in"),
            layout: &blit.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&src) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&blit.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
            ],
        });
        let target = tex.create_view(&wgpu::TextureViewDescriptor { base_mip_level: k, mip_level_count: Some(1), ..Default::default() });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("stand-in"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&blit.pipeline);
        pass.set_bind_group(0, &bg, &[]);
        pass.draw(0..3, 0..1);
    }
    queue.submit([enc.finish()]);
    (tex, view)
}

/// Whether `levels` are the ones a placeholder `w` x `h` with `n` levels
/// was made for (a stand-in read otherwise than its header said is left
/// unwritten: the game's own goes on showing).
pub fn fits(levels: &[esf::Image], (w, h, n): (u32, u32, u32)) -> bool {
    levels.len() == n as usize && levels.iter().enumerate().all(|(k, l)| (l.width, l.height) == level_size(w, h, k as u32) && l.rgba.len() == (l.width * l.height * 4) as usize)
}

/// A stand-in's levels written over its placeholder.
pub fn write(queue: &wgpu::Queue, tex: &wgpu::Texture, levels: &[esf::Image]) {
    for (k, l) in levels.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: k as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            &l.rgba,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(l.width * 4), rows_per_image: Some(l.height) },
            wgpu::Extent3d { width: l.width, height: l.height, depth_or_array_layers: 1 },
        );
    }
}

/// How many of the stand-ins waiting (their sizes in bytes, in order) are
/// written this frame: one at least, then each while the frame's bytes stay
/// within `budget`.
pub fn due(sizes: &[usize], budget: usize) -> usize {
    let mut spent = 0;
    sizes.iter().take_while(|&&b| {
        let go = spent == 0 || spent + b <= budget;
        spent += b;
        go
    }).count()
}

/// A stand-in's bytes, every level.
pub fn bytes(levels: &[esf::Image]) -> usize {
    levels.iter().map(|l| l.rgba.len()).sum()
}

/// The stand-ins read, waiting to be written (`Renderer::take_stand_ins`),
/// by the texture key each was asked for under.
pub type Ready = Vec<(u64, Arc<Vec<esf::Image>>)>;

#[cfg(test)]
mod tests {
    use super::*;

    /// A placeholder's level is drawn from the surface's level nearest its
    /// size, the base for a larger one; the control: a level as large as
    /// the base is the base's.
    #[test]
    fn each_level_from_the_nearest_of_the_surfaces() {
        assert_eq!(source_level((256, 256), (1024, 1024)), 0.0, "magnified: the base");
        assert_eq!(source_level((256, 256), (256, 256)), 0.0, "the control: its size");
        assert_eq!(source_level((256, 256), (128, 128)), 1.0);
        assert_eq!(source_level((256, 128), (64, 64)), 2.0, "the larger ratio");
        assert_eq!(source_level((256, 256), (96, 96)), 1.0, "the nearest, rounded");
        assert_eq!(level_size(1024, 512, 10), (1, 1));
    }

    /// One stand-in a frame at least, then as many as the budget holds; the
    /// control: a budget of nothing still writes one.
    #[test]
    fn stand_ins_written_within_a_frames_budget() {
        let mb = 1 << 20;
        assert_eq!(due(&[3 * mb, 3 * mb, 3 * mb], 8 * mb), 2);
        assert_eq!(due(&[20 * mb, mb], 8 * mb), 1, "a large one alone");
        assert_eq!(due(&[mb, mb], 0), 1, "the control: one at least");
        assert_eq!(due(&[], 8 * mb), 0);
    }

    /// Levels fit the placeholder they were read for, not another's.
    #[test]
    fn levels_fit_their_placeholder() {
        let img = |w: u32, h: u32| esf::Image { width: w, height: h, rgba: vec![0; (w * h * 4) as usize], alpha: false };
        let l = vec![img(64, 32), img(32, 16), img(16, 8)];
        assert!(fits(&l, (64, 32, 3)));
        assert!(!fits(&l, (64, 32, 4)), "another count");
        assert!(!fits(&l, (32, 32, 3)), "another size");
        assert_eq!(bytes(&l), (64 * 32 + 32 * 16 + 16 * 8) * 4);
    }
}
