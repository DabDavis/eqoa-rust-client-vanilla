//! The options' Brightness (the client's own; the PS2 left it to the TV): a
//! curve over the finished scene's colour levels, given as a table of 256
//! (`set`). With a table the scene is drawn into a texture of the target's
//! size (`scene_target`) and then through the table into the target
//! (`apply`), before the labels, the overlay and the 2D layer, which keep the
//! game's own levels. Without one there is no pass at all, and the frame is
//! drawn exactly as before. The lookup is by the stored byte (curve.wgsl), so
//! the result is the table's on every GPU, WebGL2's included; through the
//! table of no change (`set_through`), the scene's texture is a copy any pass
//! may read, where the target cannot be copied (a page's canvas, a GL
//! window): the ground fog a PS2 frame keeps reads the frame (`fog16`).

/// The table, and the GPU's half once it is first wanted.
pub struct Curve {
    format: wgpu::TextureFormat,
    table: Option<[u8; 256]>,
    /// the table the GPU holds
    uploaded: Option<[u8; 256]>,
    gpu: Option<Gpu>,
    /// `scene_target` gave its texture for this frame: `apply` draws it
    pending: bool,
    /// the scene drawn into the texture without a table, and through no
    /// change into the target (`set_through`)
    through: bool,
}

struct Gpu {
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
    table: wgpu::Texture,
    table_view: wgpu::TextureView,
    /// the scene's texture, its size, its view and the bind group reading it
    scene: Option<((u32, u32), wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
}

impl Curve {
    pub(crate) fn new(format: wgpu::TextureFormat) -> Self {
        Curve { format, table: None, uploaded: None, gpu: None, pending: false, through: false }
    }

    /// The table a level goes through (`None`: none, the picture as drawn).
    pub fn set(&mut self, table: Option<[u8; 256]>) {
        self.table = table;
    }

    pub fn table(&self) -> Option<&[u8; 256]> {
        self.table.as_ref()
    }

    /// For this frame: the scene drawn into the texture even without a table,
    /// and copied into the target unchanged (the table of no change), for a
    /// pass that reads the frame where the target cannot be copied
    /// (`Renderer::fans_read_frame`). Off, a frame without a table has no
    /// pass, as before.
    pub fn set_through(&mut self, on: bool) {
        self.through = on;
    }

    /// The table the frame goes through: the one set, or with `set_through`
    /// the one of no change.
    fn table_now(&self) -> Option<[u8; 256]> {
        self.table.or_else(|| self.through.then(|| std::array::from_fn(|k| k as u8)))
    }

    /// Where this frame's scene is drawn: with a table, the curve's texture
    /// of the target's `size` (made, or made again at a new size); without,
    /// `None`, the target itself.
    pub fn scene_target(&mut self, device: &wgpu::Device, size: (u32, u32)) -> Option<wgpu::TextureView> {
        // none wanted: the texture let go
        if self.table_now().is_none() {
            if let Some(g) = &mut self.gpu {
                g.scene = None;
            }
            return None;
        }
        let format = self.format;
        let g = self.gpu.get_or_insert_with(|| Gpu::new(device, format));
        if g.scene.as_ref().is_none_or(|s| s.0 != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("curve scene"),
                size: wgpu::Extent3d { width: size.0.max(1), height: size.1.max(1), depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                // copied from by the ground fog a PS2 frame keeps (`fog16`)
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&Default::default());
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("curve"),
                layout: &g.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&g.table_view) },
                ],
            });
            g.scene = Some((size, texture, view, bind));
        }
        self.pending = true;
        g.scene.as_ref().map(|s| s.2.clone())
    }

    /// The scene's texture, while there is one (a test writes into it).
    pub fn scene_texture(&self) -> Option<&wgpu::Texture> {
        self.gpu.as_ref()?.scene.as_ref().map(|s| &s.1)
    }

    /// The scene `scene_target` gave this frame, through the table, into
    /// `target`, every pixel of it; nothing if it gave none.
    pub fn apply(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        if !std::mem::take(&mut self.pending) {
            return;
        }
        let (Some(table), Some(g)) = (self.table_now(), self.gpu.as_ref()) else { return };
        let Some((_, _, _, bind)) = &g.scene else { return };
        if self.uploaded != Some(table) {
            queue.write_texture(
                g.table.as_image_copy(),
                &table,
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(1) },
                wgpu::Extent3d { width: 256, height: 1, depth_or_array_layers: 1 },
            );
            self.uploaded = Some(table);
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("curve"),
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
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
    }
}

impl Gpu {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let texture = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("curve"), entries: &[texture(0), texture(1)] });
        let shader = device.create_shader_module(wgpu::include_wgsl!("curve.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("curve"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("curve"),
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
        let table = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("curve table"),
            size: wgpu::Extent3d { width: 256, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let table_view = table.create_view(&Default::default());
        Gpu { layout, pipeline, table, table_view, scene: None }
    }
}
