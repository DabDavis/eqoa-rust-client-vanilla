//! The characters' blob shadows (`VIScene::RenderShadows`, `vi::shadow`):
//! each a run of triangles in the world under `VIScene::SetSky`'s shadow
//! material, drawn as the VU1 program `VIRFan` draws a fan of three
//! (`shadow.wgsl`). The material: shadow.tga's one layer, modulated, blend
//! mode 3 (`(Cs - Cd) As + Cd`), LOD bias 1.0, Z tested and not written, no
//! dither, wrap mode 3 (`ConstructTextPass` makes it CLAMP on both axes);
//! fogged as the raster's fog says (the fan's PRIM has FGE). `SetZBias(15)`
//! moves its fragments nearer the camera by a constant in the GS's screen Z,
//! a shift of 1/w (`Bias`): from the game's camera, which the client's zoom
//! past the game's ceiling only moves back (`lod::eye`, `Bias::behind`), so
//! the shadow is pulled as far as the console pulled it at its farthest. A
//! fan's winding parity is fixed (`VIRFan`),
//! so its triangle (0, 1, 2) is judged as a strip's first: esf draws that one
//! as (0, 2, 1) for `FRONT_FACE`, so here the other winding is the front.
//! Drawn after the particle system's billboards and before the weather's, as
//! `VIScene::Render` draws them after `VIParticleSystem::Render`.

use crate::billboard::Batch;
use crate::{DEPTH_FORMAT, FRONT_FACE, Renderer, Textures, View};
use bytemuck::{Pod, Zeroable};
use glam::Mat4;

/// `SetLayerTextureLODBias(0, 1.0f)`.
pub const LOD_BIAS: f32 = 1.0;

/// One vertex as `RenderShadow` hands it to the raster: where (relative to
/// the eye the pass is set for), its UV and the alpha of its colour (white).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct Vertex {
    pub at: [f32; 3],
    pub uv: [f32; 2],
    pub alpha: f32,
}

/// How `SetZBias` draws a shadow nearer the camera (`shadow.wgsl`). The GS
/// adds a constant to its screen Z after the divide by w, which is a shift
/// `k` of 1/w for the game's camera (`z_bias_reciprocal`); `behind` is how far
/// this view's eye is behind the game's camera along the view (`lod::eye`: 0
/// within the game's ceiling), the pull then the one the game's camera, that
/// much nearer, makes at its own depth; `clip_z` is the projection's
/// `w_axis.z` (glam's [0, 1] perspective: depth = -z_axis.z + w_axis.z / w),
/// which turns a change of 1/w into clip z. All zero: no bias.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bias {
    pub clip_z: f32,
    pub k: f32,
    pub behind: f32,
}

impl Bias {
    /// The depth a fragment `w` deep is drawn at (`shadow.wgsl`'s): `w` less
    /// the pull the game's camera makes at its depth, `w - behind`. With
    /// `behind` 0 that is 1/w larger by `k`, the GS's constant offset.
    pub fn drawn_at(&self, w: f32) -> f32 {
        if w <= 1e-4 {
            return w;
        }
        let wg = (w - self.behind).max(0.0);
        (w - self.k * wg * wg / (1.0 + self.k * wg)).max(1e-4)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameU {
    view_proj: [[f32; 4]; 4],
    fog_color: [f32; 4],
    fog: [f32; 4],
    bias: [f32; 4],
}

pub(crate) struct Shadows {
    pipeline: wgpu::RenderPipeline,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    frame_buf: wgpu::Buffer,
    frame_bg: wgpu::BindGroup,
    /// the vertex buffer and how many vertices it holds
    verts: Option<(wgpu::Buffer, u64)>,
    /// the texture's bind group, by its key
    group: Option<(u64, wgpu::BindGroup)>,
    /// what `draw_shadows` draws: this many vertices, made for this eye with
    /// this Z bias
    count: u32,
    eye: glam::Vec3,
    bias: Bias,
}

impl Shadows {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shadow.wgsl"));
        let frame_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadows frame"),
            entries: &[crate::uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadows texture"),
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
            label: Some("shadows"),
            bind_group_layouts: &[Some(&frame_layout), Some(&tex_layout)],
            immediate_size: 0,
        });
        // wrap mode 3: CLAMP_1 5, both axes clamped
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let frame_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shadows frame"),
            size: std::mem::size_of::<FrameU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let frame_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadows frame"),
            layout: &frame_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: frame_buf.as_entire_binding() }],
        });
        let front_face = match FRONT_FACE {
            wgpu::FrontFace::Ccw => wgpu::FrontFace::Cw,
            wgpu::FrontFace::Cw => wgpu::FrontFace::Ccw,
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadows"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32],
                })],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, front_face, cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                // colour only: the target's alpha stays 1 (the models' pipelines)
                targets: &[Some(wgpu::ColorTargetState { format, blend: crate::blend_state(3), write_mask: wgpu::ColorWrites::COLOR })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Shadows { pipeline, tex_layout, sampler, frame_buf, frame_bg, verts: None, group: None, count: 0, eye: glam::Vec3::ZERO, bias: Bias::default() }
    }
}

/// `SetZBias(z)`'s offset as a shift of 1/w: the raster adds z x 166 to the
/// GS's screen Z after the divide, whose range [16, 2^23] spans the game
/// camera's [near, far] (`VIRaster::Frustum`'s z, `CalcClipToScreen`), so a
/// fragment w deep is drawn as if 1/w were larger by this.
pub fn z_bias_reciprocal(z: i32, near: f32, far: f32) -> f32 {
    let half = (8_388_608.0f64 - 16.0) * 0.5;
    let bias = if z > 0 { z as f64 * 166.0 } else { 0.0 };
    let (n, f) = (near as f64, far as f64);
    (bias * (f - n) / (half * 2.0 * f * n)) as f32
}

impl Renderer {
    /// The shadows `draw_shadows` draws from now on: `verts` three a
    /// triangle, relative to `eye`, textured by `texture` (a DictID; none in
    /// `textures`: nothing is drawn), nearer the camera by `bias`, after the
    /// particle system's billboard batches (`particles_end`,
    /// `draw_billboards_and_shadows`). Empty: nothing at all.
    #[allow(clippy::too_many_arguments)]
    pub fn set_shadows(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, texture: i32, verts: &[Vertex], eye: glam::Vec3, bias: Bias, textures: &Textures) {
        if verts.is_empty() {
            if let Some(s) = self.shadows.as_mut() {
                s.count = 0;
            }
            return;
        }
        let Some(view) = self.texture(device, queue, texture, textures) else {
            if let Some(s) = self.shadows.as_mut() {
                s.count = 0;
            }
            return;
        };
        let key = textures.key(texture);
        let format = self.format;
        let s = self.shadows.get_or_insert_with(|| Shadows::new(device, format));
        if s.group.as_ref().is_none_or(|(k, _)| *k != key) {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("shadows texture"),
                layout: &s.tex_layout,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&s.sampler) }],
            });
            s.group = Some((key, bg));
        }
        let n = verts.len() as u64;
        if s.verts.as_ref().is_none_or(|(_, cap)| *cap < n) {
            let cap = n.next_power_of_two().max(768);
            let buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shadows"),
                size: cap * std::mem::size_of::<Vertex>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            s.verts = Some((buf, cap));
        }
        if let Some((buf, _)) = &s.verts {
            queue.write_buffer(buf, 0, bytemuck::cast_slice(verts));
        }
        s.count = verts.len() as u32;
        s.eye = eye;
        s.bias = bias;
    }

    /// `VIScene::Render`'s tail: the particle system's batches (the first
    /// `particles_end`), the shadows `set_shadows` set for `view`
    /// (`RenderShadows`), the strips (`draw_queued_strips`: the spell
    /// effects' lightning, then `VITrailFxRender`'s trails), then the rest of
    /// the batches, the weather's (`draw_late_billboards`).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_billboards_and_shadows(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        depth: &wgpu::TextureView,
        v: Mat4,
        p: Mat4,
        batches: &[Batch],
        textures: &Textures,
        view: &View,
    ) {
        let k = self.particles_end.min(batches.len());
        self.draw_billboards(device, queue, encoder, target, depth, v, p, &batches[..k], textures);
        self.draw_shadows(queue, encoder, target, depth, view);
        self.draw_queued_strips(device, queue, encoder, target, depth, v, p, textures);
        self.draw_late_billboards(device, queue, encoder, target, depth, v, p, &batches[k..], textures);
    }

    /// Whether `draw_shadows` has anything to draw for `view`.
    pub fn shadows_pending(&self, view: &View) -> bool {
        self.shadows.as_ref().is_some_and(|s| s.count > 0 && s.eye == view.eye)
    }

    /// Draws the shadows `set_shadows` set over what is in `target` and
    /// `depth`, under `view`'s fog, if `view` is the one they were made for.
    pub fn draw_shadows(&mut self, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, view: &View) {
        if !self.shadows_pending(view) {
            return;
        }
        let Some(s) = self.shadows.as_ref() else { return };
        let (Some((buf, _)), Some((_, group))) = (&s.verts, &s.group) else { return };
        let rel = (view.view_proj.as_dmat4() * glam::DMat4::from_translation(view.eye.as_dvec3())).as_mat4();
        let k = if crate::mipmaps() { crate::gs_lod_k(LOD_BIAS) } else { crate::BASE_LEVEL_K };
        let u = FrameU {
            view_proj: rel.to_cols_array_2d(),
            fog_color: view.fog.map_or([0.0; 4], |(_, _, c)| c.extend(1.0).to_array()),
            fog: {
                let [n, fa] = view.fog.map_or([0.0, 1.0], |(n, fa, _)| [n, fa.max(n + 1e-3)]);
                [n, fa, k, 0.0]
            },
            bias: [s.bias.clip_z, s.bias.k, s.bias.behind, 0.0],
        };
        queue.write_buffer(&s.frame_buf, 0, bytemuck::bytes_of(&u));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shadows"),
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
        pass.set_pipeline(&s.pipeline);
        pass.set_bind_group(0, &s.frame_bg, &[]);
        pass.set_bind_group(1, group, &[]);
        pass.set_vertex_buffer(0, buf.slice(..s.count as u64 * std::mem::size_of::<Vertex>() as u64));
        pass.draw(0..s.count, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Z bias as the GS has it: 15 x 166 = 2490 of the screen Z's
    /// [16, 2^23] over the camera's [0.1, 900] is 2.968e-3 of 1/w, which
    /// draws a fragment 5 deep 0.073 nearer, 20 deep 1.12, 50 deep 6.46. With
    /// the eye behind the game's camera (the client's zoom), the pull is the
    /// one the game's camera makes at its own depth: 0.19 at the game's 8,
    /// whatever the zoom. The controls: no bias, no pull; the zoom's eye
    /// taken as the game's (behind 0), 6.46 at 50.
    #[test]
    fn the_z_bias_is_a_shift_of_the_reciprocal_depth_from_the_games_camera() {
        let k = z_bias_reciprocal(15, 0.1, 900.0);
        assert!((k - 2.968e-3).abs() < 2e-6, "{k}");
        let game = Bias { clip_z: -0.3, k, behind: 0.0 };
        for (w, nearer) in [(5.0f32, 0.0731f32), (20.0, 1.1207), (50.0, 6.4611)] {
            assert!((w - game.drawn_at(w) - nearer).abs() < 1e-3, "{w}: {}", w - game.drawn_at(w));
            // the same as 1/w larger by k
            assert!((1.0 / game.drawn_at(w) - (1.0 / w + k)).abs() < 1e-6);
        }
        assert_eq!(z_bias_reciprocal(0, 0.1, 900.0), 0.0);
        assert_eq!(z_bias_reciprocal(-3, 0.1, 900.0), 0.0);
        assert_eq!(Bias::default().drawn_at(12.0), 12.0, "the control: no bias");
        // zoomed out to 50 from the game's 8: the game's pull at 8
        let zoomed = Bias { behind: 42.0, ..game };
        let at8 = 8.0 - game.drawn_at(8.0);
        assert!((at8 - 0.1855).abs() < 1e-3, "{at8}");
        assert!((50.0 - zoomed.drawn_at(50.0) - at8).abs() < 1e-4, "{}", 50.0 - zoomed.drawn_at(50.0));
        assert!(50.0 - game.drawn_at(50.0) > 6.0, "the control: from the zoom's eye, 6.46");
        // nearer the eye than the game's camera: no pull; behind the eye: none
        assert_eq!(zoomed.drawn_at(30.0), 30.0);
        assert_eq!(game.drawn_at(-1.0), -1.0);
    }
}
