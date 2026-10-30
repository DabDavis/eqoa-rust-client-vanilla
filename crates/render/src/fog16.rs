//! The ground fog a PS2 frame keeps (the options' Ground Fog, PS2): the
//! effect volumes' fog fans blended as the GS blends them into the game's
//! 16-bit frame, which they are made for. `VIRaster::Init` makes the frame
//! PSMCT16S and sets DTHE in all four draw environments, and a material
//! without the dither flag (an effect volume's, `SetDither(0)`) is drawn
//! under the DIMX matrix 0x2031312020313120, entries 0 to 3: every blend
//! adds the pixel's entry and cuts the low three bits. A fog fan's lift of
//! four levels or less never shows, a darkening of four or less shows
//! twice over, and fans stop a good way short of the fog's colour: on the
//! console the fog is a low, dim band where a blend of eight bits a channel
//! draws a bright cloud (eqoa-port, `VI_GS=cpu`).
//!
//! A blend cannot read the frame each fan finds, so the fans add up what
//! falls on each pixel instead (`fans.wgsl` `fs_sums`, into two float
//! textures, Z tested against the scene as they are drawn: how many, their
//! alphas, alphas squared and alphas times colour; then `fs_spread`, over
//! those, alpha times the colour's distance from their mean squared, into a
//! third), over a copy of the frame before them; then `fog16.wgsl` runs the
//! GS's steps over as many fans, a standard deviation's alpha above the mean
//! and one below, and a standard deviation's colour above the mean and one
//! below, in turn (the count and every sum kept; where the alphas lean so far
//! that the one below would be under 0, as many equal fans as keep the sums
//! of the alphas and of their squares, the two meeting where the mean is the
//! deviation; a fan whose As is 0, which changes nothing, not counted), from
//! the copy as five bits a channel, under the pixel's own matrix entry (so
//! the fog carries the console's 4 by 4 dither, as on a PS2), and changes the
//! frame by what they change it. Equal fans are the GS exactly. The reference is the GS a fan
//! at a time (`Renderer::set_ps2_sequential`: a copy and three passes a fan,
//! slow): over Freeport's sign post's fog at night the sums come within 1 to
//! 2 levels of it (the box the fog fills, averaged), where a blend of eight
//! bits a channel is 6 to 9 too bright. The copy and the passes are made only
//! while a fan of the fog is drawn, and only over the rectangle its fans
//! cover; no larger frame than 3840 by 2160 (`MAX_PIXELS`: the textures are
//! 24 bytes a pixel), above which the fog is drawn as it is.

/// The most pixels a frame can have for its fog to be blended so: the copy
/// and the sums are 24 bytes a pixel (200 MB at 3840 by 2160). A frame
/// larger (a Render Scale above a 4K frame) draws its fog as it is.
pub const MAX_PIXELS: u64 = 3840 * 2160;

/// The sums of alpha times colour (rgb) and of alpha (a).
pub const SUMS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The sum of the alphas squared (r) and the number of fans (g).
pub const SQUARES_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rg16Float;
/// The sum of alpha times the colour's distance from the mean squared (rgb).
pub const COLOURS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// Whether `adapter` can add the sums up: both formats drawn into, blended
/// and read. WebGPU always can; WebGL2 where it has float colour buffers.
pub fn supported(adapter: &wgpu::Adapter) -> bool {
    [SUMS_FORMAT, SQUARES_FORMAT, COLOURS_FORMAT].iter().all(|&f| {
        let ff = adapter.get_texture_format_features(f);
        ff.allowed_usages.contains(wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING) && ff.flags.contains(wgpu::TextureFormatFeatureFlags::BLENDABLE)
    })
}

/// The pixels a frame's fog fans can cover: x, y, width, height.
pub(crate) type Rect = (u32, u32, u32, u32);

/// The rectangle `corners` (relative to the eye) cover on a `size` frame
/// through `rel` (the view-projection with the eye at the origin), a pixel
/// over on each side; the whole frame when one is behind the eye, none
/// when they are all off it.
pub(crate) fn rect(corners: &[[f32; 3]], rel: glam::Mat4, size: (u32, u32)) -> Option<Rect> {
    if corners.is_empty() {
        return None;
    }
    let (w, h) = (size.0 as f32, size.1 as f32);
    let (mut lo, mut hi) = (glam::Vec2::splat(f32::MAX), glam::Vec2::splat(f32::MIN));
    for c in corners {
        let p = rel * glam::Vec3::from(*c).extend(1.0);
        if p.w <= 1e-3 {
            return Some((0, 0, size.0, size.1));
        }
        let ndc = glam::Vec2::new(p.x, p.y) / p.w;
        lo = lo.min(ndc);
        hi = hi.max(ndc);
    }
    // NDC to pixels, y down
    let x0 = ((lo.x * 0.5 + 0.5) * w - 1.0).floor().clamp(0.0, w);
    let x1 = ((hi.x * 0.5 + 0.5) * w + 1.0).ceil().clamp(0.0, w);
    let y0 = ((0.5 - hi.y * 0.5) * h - 1.0).floor().clamp(0.0, h);
    let y1 = ((0.5 - lo.y * 0.5) * h + 1.0).ceil().clamp(0.0, h);
    (x1 > x0 && y1 > y0).then(|| (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}

/// The resolve's pipeline, and the textures of the last frame size.
pub(crate) struct Fog16 {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    /// one fan's step under the pixel's own matrix entry (`fs_one`)
    one: wgpu::RenderPipeline,
    /// the sums as the spread's pass reads them (`fans.wgsl` `fs_spread`)
    sums_layout: wgpu::BindGroupLayout,
    frame: Option<Frame>,
}

struct Frame {
    size: (u32, u32),
    copy: wgpu::Texture,
    sums: wgpu::TextureView,
    squares: wgpu::TextureView,
    colours: wgpu::TextureView,
    bind: wgpu::BindGroup,
    sums_bind: wgpu::BindGroup,
}

impl Fog16 {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("fog16"), entries: &[texture(0), texture(1), texture(2), texture(3)] });
        let shader = device.create_shader_module(wgpu::include_wgsl!("fog16.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("fog16"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let resolve = |entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("fog16"),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[] },
                primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    // colour only: the target's alpha stays 1, as the fans' own
                    targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::COLOR })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let (pipeline, one) = (resolve("fs_main"), resolve("fs_one"));
        let sums_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("fog16 sums"), entries: &[texture(0)] });
        Fog16 { layout, pipeline, one, sums_layout, frame: None }
    }

    pub(crate) fn sums_layout(&self) -> &wgpu::BindGroupLayout {
        &self.sums_layout
    }

    /// The copy and the sums for a `size` frame of `format`, made again at
    /// a new size.
    fn frame(&mut self, device: &wgpu::Device, size: (u32, u32), format: wgpu::TextureFormat) {
        if self.frame.as_ref().is_none_or(|f| f.size != size || f.copy.format() != format) {
            self.frame = None;
            let extent = wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 };
            let make = |label, format, usage| {
                device.create_texture(&wgpu::TextureDescriptor { label: Some(label), size: extent, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] })
            };
            let copy = make("fog16 copy", format, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
            let sums = make("fog16 sums", SUMS_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING).create_view(&Default::default());
            let squares = make("fog16 squares", SQUARES_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING).create_view(&Default::default());
            let colours = make("fog16 colours", COLOURS_FORMAT, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING).create_view(&Default::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fog16"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&copy.create_view(&Default::default())) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&sums) },
                    wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&squares) },
                    wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&colours) },
                ],
            });
            let sums_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("fog16 sums"),
                layout: &self.sums_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&sums) }],
            });
            self.frame = Some(Frame { size, copy, sums, squares, colours, bind, sums_bind });
        }
    }

    /// The fog over `target`: the frame within `rect` copied, the sums
    /// cleared and added up by `fans` (the fans drawn, given the pass and
    /// whether it is the second: the sums first, then their colours' spread
    /// over them, both over `depth`), then the GS's steps over them within
    /// `rect`; with `one`, of a single fan, its step under each pixel's own
    /// matrix entry (the sequential reference).
    pub(crate) fn draw(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        rect: Rect,
        one: bool,
        fans: impl Fn(&mut wgpu::RenderPass<'_>, bool),
    ) {
        let t = target.texture();
        let size = (t.width(), t.height());
        self.frame(device, size, t.format());
        let f = self.frame.as_ref().expect("made above");
        let origin = wgpu::Origin3d { x: rect.0, y: rect.1, z: 0 };
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo { texture: t, mip_level: 0, origin, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyTextureInfo { texture: &f.copy, mip_level: 0, origin, aspect: wgpu::TextureAspect::All },
            wgpu::Extent3d { width: rect.2, height: rect.3, depth_or_array_layers: 1 },
        );
        {
            let clear = wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fog16 sums"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment { view: &f.sums, depth_slice: None, resolve_target: None, ops: clear }),
                    Some(wgpu::RenderPassColorAttachment { view: &f.squares, depth_slice: None, resolve_target: None, ops: clear }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_scissor_rect(rect.0, rect.1, rect.2, rect.3);
            fans(&mut pass, false);
        }
        {
            let clear = wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("fog16 spread"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &f.colours, depth_slice: None, resolve_target: None, ops: clear })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_scissor_rect(rect.0, rect.1, rect.2, rect.3);
            pass.set_bind_group(2, &f.sums_bind, &[]);
            fans(&mut pass, true);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fog16"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_scissor_rect(rect.0, rect.1, rect.2, rect.3);
        pass.set_pipeline(if one { &self.one } else { &self.pipeline });
        pass.set_bind_group(0, &f.bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::rect;
    use glam::Mat4;

    /// A quad's rectangle on the frame, a pixel over each side; the whole
    /// frame when a corner is behind the eye; none off the frame.
    #[test]
    fn the_rectangle_a_fan_covers() {
        let id = Mat4::IDENTITY;
        let quad = |x0: f32, y0: f32, x1: f32, y1: f32| [[x0, y0, 0.5], [x1, y0, 0.5], [x1, y1, 0.5], [x0, y1, 0.5]];
        assert_eq!(rect(&quad(-0.5, -0.5, 0.5, 0.5), id, (100, 100)), Some((24, 24, 52, 52)));
        assert_eq!(rect(&quad(-3.0, -3.0, 3.0, 3.0), id, (100, 80)), Some((0, 0, 100, 80)), "clamped to the frame");
        assert_eq!(rect(&quad(2.0, 2.0, 3.0, 3.0), id, (100, 100)), None, "off the frame");
        let behind = Mat4::from_cols_array(&[1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0]);
        let mut q = quad(-0.1, -0.1, 0.1, 0.1);
        q[0][2] = -1.0;
        assert_eq!(rect(&q, behind, (64, 48)), Some((0, 0, 64, 48)), "a corner behind the eye: all of it");
        assert_eq!(rect(&[], id, (64, 48)), None);
    }
}
