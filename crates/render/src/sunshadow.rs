//! Sun Shadows (the options' Extended > Lighting, which work only with
//! Dynamic Lighting on; off unless chosen): the sun's light stopped by what
//! stands between a surface and the sun. The strip programs let it through
//! everything: their only visibility is the vertex alpha a bake keeps times
//! the actor's static alpha (`lit.wgsl`'s sun term), so on a PS2, and with
//! Dynamic Lighting alone, a wall, a roof or a character casts nothing.
//!
//! A shadow map of [`CASCADES`] cascades (a 2D depth array, [`map_size`] a
//! side), each an orthographic view along the sun's light round the sphere
//! that holds its slice of the view's depth ([`ENDS`]): drawn each frame the
//! sun is up from what the frame draws (`suncast.wgsl`), the zones' groups
//! from bundles recorded at their upload (`RunSun::casters`, a run wholly out
//! of a cascade's box skipped) and the models (characters, what they hold,
//! the boids) as they are, every opaque layer of a material whose first
//! layer is opaque, alpha tested as the layer is drawn (leaves cut out), the
//! faces turned to the sun alone (as the strip programs cull for a camera);
//! not the radial flora (`Draw::Instances`), the sky, nor any blended layer
//! (water, glass, fog, effects), nor the particles, billboards, fans, plates
//! or the HUD, which `draw` does not draw. Then the world is drawn through a
//! third set of model pipelines, Dynamic Lighting's with the map in the sun
//! term (`sunshadow.wgsl`), and the groups' runs through bundles recorded
//! for them (`RunSun::lit`).
//!
//! The map follows the eye: every matrix is built in doubles relative to it
//! (Freeport is at x 25,300, where a float steps 2 mm), its centre snapped
//! to its texels in the world (the light's axes from the origin), and each
//! sphere's radius is rounded up, so a camera that moves or turns moves the
//! map by whole texels and nothing shimmers. The sun's direction is taken in
//! steps of [`SUN_STEP`] (the game's day passes in an hour: the sun moves a
//! tenth of a degree a second). The shadows end at [`RANGE`] before the
//! camera, faded out from [`FADE_FROM`]; they fade in as the sun rises
//! clear of the horizon and out as it sets ([`strength`]): by night the
//! game's "sun" is the sun under the ground, its light travelling up
//! (`vi::tod`: (−sin 15h°, cos 15h°, 0), a purple light on what faces down),
//! not a moon, and nothing is shadowed.
//!
//! The rule for what the console already darkens: the sun's visibility is
//! the lesser of the strip programs' (colour.a × static.a) and the map's
//! (`sunshadow.wgsl`), never their product, so the map takes light away
//! only where the console's light reaches and a surface its bake darkened
//! is not darkened twice by the same wall; a baked actor whose bake gave it
//! no sun (most of Freeport's walls and props) stays as the console has it.
//!
//! Off (or with Dynamic Lighting off) nothing here is made or run, and every
//! frame is the build's before it, byte for byte: the receivers are a module
//! of their own (lit.wgsl's text with its sun line changed, `receivers_wgsl`),
//! so the lit pipelines Dynamic Lighting draws with alone are untouched.

use crate::{DEPTH_FORMAT, Draw, GpuGroup, GpuModel, Renderer, View, lit};
use bytemuck::{Pod, Zeroable};
use glam::{DMat4, DVec3, DVec4, Mat4, Vec3};

/// The cascades the view's depth is cut into.
pub const CASCADES: usize = 4;

/// Where each cascade ends, in depth before the camera: the practical split
/// (three quarters logarithmic, a quarter even) of 0.3 to [`RANGE`], each a
/// sphere that holds its slice (at the game's 55° and a 16:9 window: radii
/// about 21, 48, 105 and 280, so a texel of 2, 5, 10 and 27 cm at 2,048).
pub const ENDS: [f32; CASCADES] = [19.0, 43.0, 94.0, 250.0];

/// How far before the camera the shadows reach.
pub const RANGE: f32 = ENDS[CASCADES - 1];

/// Where they start to fade, gone by [`RANGE`].
pub const FADE_FROM: f32 = 212.5;

/// How far toward the sun past its sphere a cascade takes casters: a hill or
/// a tower up to this far upwind of what is drawn still casts onto it.
pub const UPWIND: f64 = 500.0;

/// The step the sun's direction is taken in (radians: an eighth of a
/// degree, the sun's travel in about 1.25 s of the game's day), so a
/// shadow's edge stays put between steps instead of crawling every frame.
pub const SUN_STEP: f64 = 0.125 * std::f64::consts::PI / 180.0;

/// A side of each cascade's map: 2,048 natively, 1,024 in a page and on
/// Android (a quarter of the memory and fill), never past the device's
/// largest texture.
pub fn map_size(device: &wgpu::Device) -> u32 {
    let want = if cfg!(any(target_arch = "wasm32", target_os = "android")) { 1024 } else { 2048 };
    want.min(device.limits().max_texture_dimension_2d)
}

/// How strong the shadows are for a sun whose light travels along `dir` (a
/// unit): none while it is under the horizon (its light travelling up, the
/// game's night), in from half a degree above it to full at six.
pub fn strength(dir: Vec3) -> f32 {
    let (lo, hi) = (0.5f32.to_radians().sin(), 6f32.to_radians().sin());
    let e = -dir.y;
    let t = ((e - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The sun's direction as the map takes it: on the game's path (no z, its
/// angle `a` with the light (−sin a, cos a, 0)) the angle in steps of
/// [`SUN_STEP`]; any other direction as it is.
pub fn stepped(dir: Vec3) -> DVec3 {
    let d = dir.as_dvec3().normalize_or(DVec3::NEG_Y);
    if d.z.abs() > 1e-6 {
        return d;
    }
    let a = (-d.x).atan2(d.y);
    let a = (a / SUN_STEP).round() * SUN_STEP;
    DVec3::new(-a.sin(), a.cos(), 0.0)
}

/// The light's axes for a light travelling along `l` (a unit): across, up
/// and along it. On the game's path, up is the world's z, which the path
/// never leaves, so one axis of the map's texels stays put all day.
pub fn axes(l: DVec3) -> [DVec3; 3] {
    let up = if l.z.abs() < 0.9 { DVec3::Z } else { DVec3::X };
    let across = l.cross(up).normalize();
    [across, across.cross(l), l]
}

/// One cascade as a frame draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cascade {
    /// A position less the eye to the cascade's clip space (x, y the map's
    /// −1..1, z 0 at the end toward the sun to 1).
    pub m: Mat4,
    /// The world size of one of its texels.
    pub texel: f32,
    /// The depth its clip z spans, in world units.
    pub depth: f32,
    /// Its box in the world: the light's axes, and along each the box's
    /// least and most (for what it draws).
    pub axes: [DVec3; 3],
    pub lo: [f64; 3],
    pub hi: [f64; 3],
}

impl Cascade {
    /// Whether any of the world box `(lo, hi)` is inside the cascade's box.
    pub fn reaches(&self, (lo, hi): (Vec3, Vec3)) -> bool {
        let (lo, hi) = (lo.as_dvec3(), hi.as_dvec3());
        let (c, e) = ((lo + hi) * 0.5, (hi - lo) * 0.5);
        (0..3).all(|k| {
            let a = self.axes[k];
            let mid = c.dot(a);
            let r = e.x * a.x.abs() + e.y * a.y.abs() + e.z * a.z.abs();
            mid + r >= self.lo[k] && mid - r <= self.hi[k]
        })
    }
}

/// A frame's cascades for the view `view_proj` (the world's, as `View`
/// has it) whose camera is at `eye`, under a sun whose light travels along
/// `sun_dir`, `size` texels a side. Every matrix is built in doubles from the
/// view relative to the eye (`view_proj`'s own translation is the eye's to
/// a millimetre or so, not to nothing: the camera's apex is found, not
/// assumed), each sphere's centre snapped to the cascade's texels along the
/// light's axes from the world's origin, its radius rounded up to a
/// sixteenth: the same world point falls on the same texel's centre as the
/// camera moves.
pub fn fit(view_proj: Mat4, eye: Vec3, sun_dir: Vec3, size: u32) -> [Cascade; CASCADES] {
    let eye = eye.as_dvec3();
    let rel = view_proj.as_dmat4() * DMat4::from_translation(eye);
    let inv = rel.inverse();
    let dehom = |v: DVec4| v.truncate() / v.w;
    // the apex: the point whose clip x, y and w are 0
    let h = inv * DVec4::new(0.0, 0.0, 1.0, 0.0);
    let apex = if h.w.abs() > 1e-12 { h.truncate() / h.w } else { DVec3::ZERO };
    let fwd = (dehom(inv * DVec4::new(0.0, 0.0, 0.0, 1.0)) - apex).normalize_or(DVec3::NEG_Z);
    // the frustum's widest reach across, a unit of depth before the camera
    let mut k2: f64 = 0.0;
    for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let q = dehom(inv * DVec4::new(x, y, 0.0, 1.0)) - apex;
        let along = q.dot(fwd);
        if along > 1e-9 {
            k2 = k2.max((q / along - fwd).length_squared());
        }
    }
    let ax = axes(stepped(sun_dir));
    let s = size.max(4) as f64;
    std::array::from_fn(|c| {
        let (n, f) = (if c == 0 { 0.0 } else { ENDS[c - 1] as f64 }, ENDS[c] as f64);
        // the sphere round the slice's eight corners, its centre on the axis
        let zc = ((n + f) * (1.0 + k2) * 0.5).min(f);
        let r = ((f - zc).powi(2) + k2 * f * f).max((zc - n).powi(2) + k2 * n * n).sqrt();
        let r = (r * 16.0).ceil() / 16.0;
        // a texel's margin round it, so the snapped box still holds it
        let t = 2.0 * r / (s - 2.0);
        let half = r + t;
        let centre = eye + apex + fwd * zc;
        let snap = |v: f64| (v / t).round() * t;
        let (x0, y0, zm) = (snap(centre.dot(ax[0])), snap(centre.dot(ax[1])), centre.dot(ax[2]));
        let (z0, z1) = (zm - r - UPWIND, zm + r);
        let depth = z1 - z0;
        // rows over a position less the eye: (p + eye)·axis less the box's
        let row = |a: DVec3, off: f64, scale: f64| DVec4::new(a.x / scale, a.y / scale, a.z / scale, (eye.dot(a) - off) / scale);
        let m = DMat4::from_cols(row(ax[0], x0, half), row(ax[1], y0, half), row(ax[2], z0, depth), DVec4::W).transpose();
        Cascade {
            m: m.as_mat4(),
            texel: t as f32,
            depth: depth as f32,
            axes: ax,
            lo: [x0 - half, y0 - half, z0],
            hi: [x0 + half, y0 + half, z1],
        }
    })
}

/// `sunshadow.wgsl`'s `SunShadow`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ShadowUniform {
    m: [[[f32; 4]; 4]; CASCADES],
    ends: [f32; 4],
    texel: [f32; 4],
    inv_depth: [f32; 4],
    /// strength, the fade's start and end, 1 / the map's size
    params: [f32; 4],
}

impl ShadowUniform {
    fn new(cascades: &[Cascade; CASCADES], strength: f32, size: u32) -> Self {
        ShadowUniform {
            m: std::array::from_fn(|c| cascades[c].m.to_cols_array_2d()),
            ends: ENDS,
            texel: std::array::from_fn(|c| cascades[c].texel),
            inv_depth: std::array::from_fn(|c| 1.0 / cascades[c].depth.max(1e-3)),
            params: [strength, FADE_FROM, RANGE, 1.0 / size as f32],
        }
    }
}

/// The sun's line of lit.wgsl, and a radial flora plant's return, as the
/// receivers change them (`receivers_wgsl`).
const SUN_LINE: &str = "    light += frame.sun.rgb * max(-dot(n, frame.sun_dir.xyz), 0.0) * i.base.w;\n";
const SUN_LINE_SHADOWED: &str = "    light += frame.sun.rgb * max(-dot(n, frame.sun_dir.xyz), 0.0) * min(i.base.w, sun_shadow(i, n));\n";
const PLANT_RETURN: &str = "        return min(light, vec3<f32>(1.0));\n";
const PLANT_RETURN_SHADOWED: &str = "        return min(light - frame.sun.rgb * (0.15 * (1.0 - sun_shadow(i, vec3<f32>(0.0, 1.0, 0.0)))), vec3<f32>(1.0));\n";

/// The receivers' module: shader.wgsl, lit.wgsl with its sun term taking the
/// map's visibility (the lesser of the two) and a plant's sun the map's,
/// then `sunshadow.wgsl`. `None` if lit.wgsl's lines are not there to
/// change (tests::the_receivers_take_the_sun_line holds them to it).
pub(crate) fn receivers_wgsl() -> Option<String> {
    let lit = include_str!("lit.wgsl");
    if lit.matches(SUN_LINE).count() != 1 || lit.matches(PLANT_RETURN).count() != 1 {
        return None;
    }
    let lit = lit.replace(SUN_LINE, SUN_LINE_SHADOWED).replace(PLANT_RETURN, PLANT_RETURN_SHADOWED);
    Some([include_str!("shader.wgsl"), &lit, include_str!("sunshadow.wgsl")].join("\n"))
}

/// The models' vertex buffers as `model_pipeline` lays them out: the
/// vertex, its instance's matrix, its instance's static light.
const VERTEX: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32x3, 3 => Float32x4, 8 => Uint8x4, 9 => Uint8x4];
const INSTANCE: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4];
const STATIC: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![10 => Float32x4];

fn vertex_buffers() -> [Option<wgpu::VertexBufferLayout<'static>>; 3] {
    let layout = |stride: usize, step_mode, attributes| Some(wgpu::VertexBufferLayout { array_stride: stride as u64, step_mode, attributes });
    [
        layout(size_of::<crate::Vertex>(), wgpu::VertexStepMode::Vertex, &VERTEX[..]),
        layout(64, wgpu::VertexStepMode::Instance, &INSTANCE[..]),
        layout(16, wgpu::VertexStepMode::Instance, &STATIC[..]),
    ]
}

/// The casters' module: shader.wgsl's structs and bindings, then
/// `suncast.wgsl`.
const CAST_WGSL: &str = concat!(include_str!("shader.wgsl"), "\n", include_str!("suncast.wgsl"));

/// A group's run for Sun Shadows: its two passes through the receivers'
/// pipelines, and its opaque layers into each cascade.
pub(crate) struct RunSun {
    pub(crate) lit: [Option<wgpu::RenderBundle>; 2],
    casters: Vec<Option<wgpu::RenderBundle>>,
}

/// The map, its pipelines and uniforms, made the first time Sun Shadows are
/// drawn with Dynamic Lighting on (`Renderer::set_sun_shadows`) and kept:
/// the bundles a group records name them.
pub(crate) struct Sun {
    /// The receivers by blend mode, as `lit::Lit`'s: `[0]` for an instance
    /// lit by every light, `[1]` for a baked one.
    pub(crate) pipelines: [Vec<wgpu::RenderPipeline>; 2],
    /// Group 3 of the receivers: the lights (Dynamic Lighting's buffer), the
    /// map, its comparison sampler and the uniform.
    pub(crate) bg: wgpu::BindGroup,
    buf: wgpu::Buffer,
    caster: wgpu::RenderPipeline,
    /// Group 3 of the casters, one a cascade (its matrix).
    cascades: Vec<wgpu::BindGroup>,
    cascade_buf: wgpu::Buffer,
    stride: u64,
    /// Each cascade's layer, as the casters' depth attachment.
    layers: Vec<wgpu::TextureView>,
    size: u32,
    /// The last frame's draws into the map, for `EQOA_PERF`: runs and draw
    /// calls.
    pub(crate) drawn: std::cell::Cell<(u32, u32)>,
}

impl Sun {
    pub(crate) fn new(device: &wgpu::Device, layouts: [&wgpu::BindGroupLayout; 3], lit: &lit::Lit, format: wgpu::TextureFormat) -> Option<Sun> {
        let size = map_size(device);
        let map = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sun shadows"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: CASCADES as u32 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let layers = (0..CASCADES as u32)
            .map(|c| {
                map.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("sun shadow cascade"),
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    base_array_layer: c,
                    array_layer_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let whole = map.create_view(&wgpu::TextureViewDescriptor { label: Some("sun shadows"), dimension: Some(wgpu::TextureViewDimension::D2Array), ..Default::default() });
        // compared, bilinear (four texels a tap), clamped
        let cmp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("sun shadows"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sun shadows"),
            size: size_of::<ShadowUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let fs = wgpu::ShaderStages::FRAGMENT;
        let recv_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lights and sun shadows"),
            entries: &[
                crate::uniform_entry(0, fs),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: fs,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2Array, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 2, visibility: fs, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None },
                crate::uniform_entry(3, fs),
            ],
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lights and sun shadows"),
            layout: &recv_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: lit.buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&whole) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&cmp) },
                wgpu::BindGroupEntry { binding: 3, resource: buf.as_entire_binding() },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("sun shadows"), source: wgpu::ShaderSource::Wgsl(receivers_wgsl()?.into()) });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sun shadowed model"),
            bind_group_layouts: &[Some(layouts[0]), Some(layouts[1]), Some(layouts[2]), Some(&recv_layout)],
            immediate_size: 0,
        });
        let make = |fs: &str| (0..5).map(|m| crate::model_pipeline(device, &layout, &module, ("vs_lit", fs), m, false, format)).collect();
        let pipelines = [make("fs_lit"), make("fs_lit_baked")];
        // the casters: a cascade's matrix each, at the device's uniform alignment
        let stride = (size_of::<[[f32; 4]; 4]>() as u64).next_multiple_of(device.limits().min_uniform_buffer_offset_alignment as u64);
        let cascade_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sun shadow cascades"),
            size: stride * CASCADES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cascade_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sun shadow cascade"),
            entries: &[crate::uniform_entry(0, wgpu::ShaderStages::VERTEX)],
        });
        let cascades = (0..CASCADES as u64)
            .map(|c| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sun shadow cascade"),
                    layout: &cascade_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &cascade_buf, offset: c * stride, size: wgpu::BufferSize::new(64) }),
                    }],
                })
            })
            .collect();
        let cast_module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("sun shadow casters"), source: wgpu::ShaderSource::Wgsl(CAST_WGSL.into()) });
        let cast_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sun shadow casters"),
            bind_group_layouts: &[Some(layouts[0]), Some(layouts[1]), Some(layouts[2]), Some(&cascade_layout)],
            immediate_size: 0,
        });
        let caster = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sun shadow casters"),
            layout: Some(&cast_layout),
            vertex: wgpu::VertexState {
                module: &cast_module,
                entry_point: Some("vs_cast"),
                compilation_options: Default::default(),
                buffers: &vertex_buffers(),
            },
            // what the sun sees casts: back faces culled, as the strip
            // programs cull them for the camera. Drawn with both faces, the
            // undersides of thin things (an awning's patches, a roof's
            // ceiling planks, a little above its top) shadowed their tops.
            primitive: wgpu::PrimitiveState { front_face: crate::FRONT_FACE, cull_mode: Some(wgpu::Face::Back), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState { module: &cast_module, entry_point: Some("fs_cast"), compilation_options: Default::default(), targets: &[] }),
            multiview_mask: None,
            cache: None,
        });
        Some(Sun { pipelines, bg, buf, caster, cascades, cascade_buf, stride, layers, size, drawn: Default::default() })
    }

    /// The commands of a group's members `span` drawn into each cascade.
    pub(crate) fn record(&self, r: &Renderer, device: &wgpu::Device, g: &GpuGroup, span: std::ops::Range<usize>, lit: [Option<wgpu::RenderBundle>; 2]) -> RunSun {
        let casters = (0..CASCADES)
            .map(|c| {
                let mut e = device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                    label: Some("sun shadow casters"),
                    color_formats: &[],
                    depth_stencil: Some(wgpu::RenderBundleDepthStencil { format: DEPTH_FORMAT, depth_read_only: false, stencil_read_only: true }),
                    sample_count: 1,
                    multiview: None,
                });
                e.set_pipeline(&self.caster);
                e.set_bind_group(0, &r.frame_bg, &[]);
                e.set_bind_group(2, &r.identity_palette, &[]);
                e.set_bind_group(3, &self.cascades[c], &[]);
                e.set_vertex_buffer(0, g.vertices.slice(..));
                e.set_index_buffer(g.indices.slice(..), wgpu::IndexFormat::Uint32);
                let mut draws = 0;
                for m in g.members[span.clone()].iter().filter(|m| m.instances > 0) {
                    let opaque = |passes: &[crate::Material]| passes.first().is_some_and(|p| p.blend == 0);
                    if !m.batches.iter().any(|(_, passes)| opaque(passes)) {
                        continue;
                    }
                    let (i0, n) = (m.instance0 as u64, m.instances as u64);
                    e.set_vertex_buffer(1, g.instances.slice(i0 * 64..(i0 + n) * 64));
                    e.set_vertex_buffer(2, g.statics.slice(i0 * 16..(i0 + n) * 16));
                    for (range, passes) in m.batches.iter().filter(|(_, p)| opaque(p)) {
                        for p in passes.iter().filter(|p| p.blend == 0) {
                            e.set_bind_group(1, &p.bg, &[]);
                            e.draw_indexed(range.clone(), 0, 0..m.instances);
                            draws += 1;
                        }
                    }
                }
                (draws > 0).then(|| e.finish(&wgpu::RenderBundleDescriptor { label: Some("sun shadow casters") }))
            })
            .collect();
        RunSun { lit, casters }
    }

    /// Draws the frame's casters into the map for `view`, and sets the
    /// receivers' uniform: whether the sun is up enough to cast (`strength`),
    /// its light not black. `false`: no shadow this frame (the receivers
    /// need not be used).
    pub(crate) fn draw(&self, r: &Renderer, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &View, items: &[Draw]) -> bool {
        let dir = view.sun_dir.normalize_or(Vec3::NEG_Y);
        let k = strength(dir);
        if k <= 0.0 || view.sun.max_element() < 0.5 / 255.0 {
            self.drawn.set((0, 0));
            return false;
        }
        let cascades = fit(view.view_proj, view.eye, dir, self.size);
        queue.write_buffer(&self.buf, 0, bytemuck::bytes_of(&ShadowUniform::new(&cascades, k, self.size)));
        for (c, cascade) in cascades.iter().enumerate() {
            queue.write_buffer(&self.cascade_buf, c as u64 * self.stride, bytemuck::bytes_of(&cascade.m.to_cols_array_2d()));
        }
        let (mut runs, mut draws) = (0u32, 0u32);
        for (c, cascade) in cascades.iter().enumerate() {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("sun shadow cascade"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.layers[c],
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut bound = false;
            for item in items {
                match item {
                    Draw::Group(g) => {
                        for run in g.runs.iter().filter(|run| run.bounds.is_some_and(|b| cascade.reaches(b))) {
                            if let Some(b) = run.sun.as_ref().and_then(|s| s.casters[c].as_ref()) {
                                pass.execute_bundles([b]);
                                bound = false;
                                runs += 1;
                            }
                        }
                    }
                    Draw::Model(m) => {
                        if !bound {
                            pass.set_pipeline(&self.caster);
                            pass.set_bind_group(0, &r.frame_bg, &[]);
                            pass.set_bind_group(3, &self.cascades[c], &[]);
                            bound = true;
                        }
                        draws += Self::encode_model(&mut pass, m);
                    }
                    // the radial flora casts nothing
                    Draw::Instances(..) => {}
                }
            }
        }
        self.drawn.set((runs, draws));
        true
    }

    /// One model's opaque layers into a cascade (the pipeline and groups 0
    /// and 3 set): the batches of its level drawn whose first layer is
    /// opaque, as `encode_model` draws them in the first pass. Returns the
    /// draw calls.
    fn encode_model<'a>(pass: &mut wgpu::RenderPass<'a>, m: &'a GpuModel) -> u32 {
        if !Renderer::draws_anything(m) {
            return 0;
        }
        let cast = |passes: &[crate::Pass], lod: Option<u8>| passes.first().is_some_and(|p| p.blend == 0) && crate::level_drawn(lod, m.level);
        if !m.batches.iter().any(|(_, _, passes, lod)| cast(passes, *lod)) {
            return 0;
        }
        pass.set_vertex_buffer(0, m.vertices.slice(..));
        pass.set_vertex_buffer(1, m.instances.slice(..));
        pass.set_vertex_buffer(2, m.statics.slice(..));
        pass.set_bind_group(2, &m.palette_bg, &[]);
        let mut draws = 0;
        for (indices, count, passes, lod) in &m.batches {
            if !cast(passes, *lod) {
                continue;
            }
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            for p in passes.iter().take(m.passes_used).filter(|p| p.blend == 0) {
                pass.set_bind_group(1, &p.bg, &[]);
                pass.draw_indexed(0..*count, 0, 0..m.instance_count);
                draws += 1;
            }
        }
        draws
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// lit.wgsl's sun line and a plant's return are there to change, once
    /// each: a change to either (another branch's) fails here, not as
    /// shadows that never come.
    #[test]
    fn the_receivers_take_the_sun_line() {
        let s = receivers_wgsl().expect("lit.wgsl's sun line and plant return");
        assert!(s.contains("min(i.base.w, sun_shadow(i, n))") && s.contains(PLANT_RETURN_SHADOWED));
        assert!(!s.contains(SUN_LINE), "the unshadowed line gone");
        assert!(s.contains("fn fs_lit_baked") && s.contains("fn sun_shadow") && CAST_WGSL.contains("fn fs_cast"));
    }

    fn view(eye: Vec3, at: Vec3) -> Mat4 {
        let proj = glam::camera::rh::proj::directx::perspective(55f32.to_radians(), 1.9, 0.3, 800.0);
        proj * glam::camera::rh::view::look_at_mat4(eye, at, Vec3::Y)
    }

    /// The point `d` before the camera on the ray through NDC (x, y).
    fn corner(vp: Mat4, x: f64, y: f64, d: f64) -> DVec3 {
        let inv = vp.as_dmat4().inverse();
        let at = |z: f64| {
            let p = inv * DVec4::new(x, y, z, 1.0);
            p.truncate() / p.w
        };
        let (near, far) = (at(0.0), at(1.0));
        // depth before the camera is clip w, linear along the ray
        let w = |p: DVec3| (vp.as_dmat4() * p.extend(1.0)).w;
        near + (far - near) * ((d - w(near)) / (w(far) - w(near)))
    }

    /// Each cascade's box holds its slice of the view: every corner of the
    /// slice falls inside its map, seen near the origin and far out at
    /// Freeport's x; the control, the last slice's far corner, is outside
    /// the first cascade's box.
    #[test]
    fn each_cascade_holds_its_slice() {
        let noon = Vec3::new(-0.3, -0.95, 0.0).normalize();
        for eye in [Vec3::new(0.0, 5.0, 0.0), Vec3::new(25_300.0, 60.0, 15_700.0)] {
            let vp = view(eye, eye + Vec3::new(1.0, -0.3, 0.4));
            let cs = fit(vp, eye, noon, 2048);
            let inside = |c: usize, p: DVec3| {
                let q = cs[c].m.as_dmat4() * (p - eye.as_dvec3()).extend(1.0);
                q.x.abs() <= 1.0 && q.y.abs() <= 1.0 && (0.0..=1.0).contains(&q.z)
            };
            for c in 0..CASCADES {
                let n = if c == 0 { 0.3 } else { ENDS[c - 1] as f64 };
                for d in [n, ENDS[c] as f64] {
                    for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                        assert!(inside(c, corner(vp, x, y, d)), "cascade {c} at {eye}: corner ({x}, {y}) at depth {d}");
                    }
                }
                assert!(cs[c].texel > 0.0 && cs[c].texel < [0.03, 0.06, 0.12, 0.3][c], "cascade {c}'s texel {}", cs[c].texel);
            }
            assert!(!inside(0, corner(vp, 1.0, 1.0, RANGE as f64)), "the control: the range's far corner is outside cascade 0");
        }
    }

    /// The map moves with the eye by whole texels: a world point falls at
    /// the same place within its texel (to a thousandth) through a hundred
    /// small steps of the camera across Freeport; the control, a grid
    /// centred on the camera itself, puts it elsewhere in its texel at each.
    #[test]
    fn the_map_moves_by_whole_texels() {
        let noon = Vec3::new(-0.4, -0.9, 0.0).normalize();
        let point = DVec3::new(25_310.0, 55.0, 15_720.0);
        let (mut seen, mut unsnapped): (Option<(f64, f64)>, Option<f64>) = (None, None);
        let mut drifted = 0;
        for step in 0..100 {
            let eye = Vec3::new(25_300.0 + step as f32 * 0.0137, 60.0, 15_700.0 + step as f32 * 0.0071);
            let vp = view(eye, eye + Vec3::new(1.0, -0.4, 0.3));
            let c = fit(vp, eye, noon, 2048)[1];
            let q = c.m.as_dmat4() * (point - eye.as_dvec3()).extend(1.0);
            // the point's place within its texel
            let within = |v: f64| ((v * 0.5 + 0.5) * 2048.0).rem_euclid(1.0);
            let at = (within(q.x), within(q.y));
            if let Some(s) = seen {
                let d = |a: f64, b: f64| (a - b).abs().min(1.0 - (a - b).abs());
                assert!(d(s.0, at.0) < 1e-3 && d(s.1, at.1) < 1e-3, "step {step}: in its texel at {at:?}, before {s:?}");
            }
            seen = Some(at);
            let u = ((point - eye.as_dvec3()).dot(c.axes[0]) / c.texel as f64).rem_euclid(1.0);
            if unsnapped.is_some_and(|v| (v - u).abs() > 1e-3) {
                drifted += 1;
            }
            unsnapped = Some(u);
        }
        assert!(drifted > 90, "the control: on a grid centred on the camera the point moves within its texel ({drifted} of 99 steps)");
    }

    /// The sun's strength: none under the horizon (the game's night, its
    /// light travelling up), none at it, full from six degrees up; the
    /// game's hours 6 and 18 none, 7 to 17 full.
    #[test]
    fn shadows_only_while_the_sun_is_up() {
        let at = |h: f32| {
            let a = (h * 15.0).to_radians();
            strength(Vec3::new(-a.sin(), a.cos(), 0.0))
        };
        for h in [0.0, 3.0, 5.0, 6.0, 18.0, 21.0] {
            assert_eq!(at(h), 0.0, "hour {h}");
        }
        for h in [7.0, 9.0, 12.0, 15.0, 17.0] {
            assert_eq!(at(h), 1.0, "hour {h}");
        }
        assert!(at(6.2) > 0.0 && at(6.2) < 1.0, "rising: {}", at(6.2));
    }

    /// The sun's direction in steps: two directions within a step's half
    /// apart give one, the next step's another; a direction off the game's
    /// path is kept.
    #[test]
    fn the_sun_moves_in_steps() {
        let dir = |a: f64| Vec3::new(-(a.sin() as f32), a.cos() as f32, 0.0);
        let a = 2.0;
        assert_eq!(stepped(dir(a)), stepped(dir(a + SUN_STEP * 0.3)));
        assert_ne!(stepped(dir(a)), stepped(dir(a + SUN_STEP)));
        let off = Vec3::new(0.3, -0.8, 0.5).normalize();
        assert!((stepped(off) - off.as_dvec3()).length() < 1e-6);
        // the light's up axis is the world's z on the game's path
        assert!((axes(stepped(dir(a)))[1] - DVec3::Z).length() < 1e-9);
    }
}
