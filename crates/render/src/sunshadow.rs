//! Sun Shadows (the options' Extended > Lighting, which work only with
//! Dynamic Lighting on; off unless chosen): the sun's light stopped by what
//! stands between a surface and the sun. The strip programs let it through
//! everything: their only visibility is the vertex alpha a bake keeps times
//! the actor's static alpha (`lit.wgsl`'s sun term), so on a PS2, and with
//! Dynamic Lighting alone, a wall, a roof or a character casts nothing.
//!
//! A shadow map of [`CASCADES`] cascades (a 2D depth array, [`map_size`] a
//! side), each an orthographic view along the sun's light round the sphere
//! that holds its slice of the view's depth ([`ENDS`]), each drawn every
//! frame the sun is up (`Sun::draw`), from what the frame draws
//! (`suncast.wgsl`): the zones' groups from bundles recorded at their upload
//! in clusters of members near one another (`GroupSun`, each culled against
//! a cascade's box) and the models (characters, what they hold, the boids;
//! not one cleared of casting, `GpuModel::casts`: a Wisp's) by their world
//! box, every opaque layer of a material whose first layer is
//! opaque, both faces, a vertex turned from the sun put back a little
//! ([`BACK_PUSH`]), a solid layer as depth alone and any other alpha tested
//! as it is drawn (leaves cut out); not the radial flora
//! (`Draw::Instances`), the sky, nor any blended layer (water, glass, fog,
//! effects), nor the particles, billboards, fans, plates or the HUD, which
//! `draw` does not draw. Then the world is drawn through a third set of
//! model pipelines, Dynamic Lighting's with the map in the sun term
//! (`sunshadow.wgsl`), and the groups' runs through bundles recorded for
//! them (`Run::sun`).
//!
//! The map follows the eye: every matrix is built in doubles relative to it
//! (Freeport is at x 25,300, where a float steps 2 mm), its centre snapped
//! to its texels, which stay put in the world ([`Grid`]), and each sphere's
//! radius is rounded up, so a camera that moves or turns moves the map by
//! whole texels and nothing shimmers. The sun's direction is taken as it is
//! every frame (the game's day passes in an hour: the sun moves a tenth of a
//! degree a second), and the shadows move with it a little every frame, the
//! texels still under them; taken in eighths of a degree, as at first, they
//! stood still for a second and a quarter and then jumped (a character's at
//! 17:00 by 4 pixels, the texels shuffled under every edge at once). The
//! shadows end at [`RANGE`] before the camera, faded out from [`FADE_FROM`];
//! they fade in as the sun rises clear of the horizon and out as it sets
//! ([`strength`]): by night the game's "sun" is the sun under the ground,
//! its light travelling up (`vi::tod`: (−sin 15h°, cos 15h°, 0), a purple
//! light on what faces down), not a moon, and nothing is shadowed.
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
/// sphere that holds its slice. At the game's 55° and a 16:9 window the radii
/// are about 20, 46, 100 and 265: a texel of 2, 4.5, 10 and 26 cm at 2,048.
/// Where a shadow's edge passes from one cascade to the next it moves by under
/// a pixel (Freeport's street at 960×540: 39 pixels change by more than 16 of
/// 255 as a seam passes, none by more than 48), so the seams are not blended:
/// a second cascade's comparisons over a sixth of each slice cost 0.16 ms at
/// 1080p and 0.39 at 4K on an RX 580.
pub const ENDS: [f32; CASCADES] = [19.0, 43.0, 94.0, 250.0];

/// How far before the camera the shadows reach.
pub const RANGE: f32 = ENDS[CASCADES - 1];

/// The cascades' ends for the options' Shadow Distance at `scale` times the
/// game's ([`ENDS`] at 1 and under, exactly): the far two stretched, the third
/// by the scale's square root and the last by the scale, the near two kept
/// (their texels the same where the eye is nearest).
pub fn ends(scale: f32) -> [f32; CASCADES] {
    if scale <= 1.0 {
        return ENDS;
    }
    [ENDS[0], ENDS[1], ENDS[2] * scale.sqrt(), ENDS[3] * scale]
}

/// Where they start to fade, gone by [`RANGE`].
pub const FADE_FROM: f32 = 212.5;

/// How far toward the sun past its sphere a cascade takes casters: a hill or
/// a tower up to this far upwind of what is drawn still casts onto it.
pub const UPWIND: f64 = 500.0;

/// How high over what it shadows a cluster of casters (not the terrain or a
/// sprite placed all over a zone, which every cascade draws) is taken to
/// reach: a cascade draws such a cluster only as far toward the sun as one
/// that tall could shadow its sphere from (`Cascade::reaches_casters`), at
/// most [`UPWIND`] (the whole box below about 17 degrees). Freeport's walls
/// and towers are a few tens.
pub const CASTER_HEIGHT: f64 = 150.0;

/// How far a caster's vertex whose normal is turned from the sun is put back
/// along the light (`suncast.wgsl`): a ceiling or a cave's roof, seen from
/// above, still casts, and the underside of a thin thing does not shadow its
/// own top. Over Freeport such a face lies over a face turned to the sun in 6
/// to 8% of the map's texels, 5 cm over it at the median and under 0.5 in
/// 99.8 to 100% (a dump of the map's two halves, front faces and back faces
/// apart); over seven towns at noon a face turned from the sun lies more
/// than 0.5 over every face turned to it below (a ceiling nothing covers) in
/// up to 0.14% of a cascade's texels (Qeynos; 0 to 0.006% in the others),
/// which culling those faces would have lost.
pub const BACK_PUSH: f64 = 0.5;

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
    /// Along the light, the nearest a cluster of casters no taller than
    /// [`CASTER_HEIGHT`] over its sphere can be and shadow it.
    pub casters_from: f64,
}

impl Cascade {
    /// Whether any of the world box `(lo, hi)` is inside the cascade's box.
    pub fn reaches(&self, b: (Vec3, Vec3)) -> bool {
        self.inside(b, self.lo[2])
    }

    /// Whether a cluster of casters in the world box `b` (no taller than
    /// [`CASTER_HEIGHT`] over what it shadows) can shadow the cascade's
    /// sphere: its box short of the end toward the sun.
    pub fn reaches_casters(&self, b: (Vec3, Vec3)) -> bool {
        self.inside(b, self.casters_from)
    }

    fn inside(&self, (lo, hi): (Vec3, Vec3), from: f64) -> bool {
        let (lo, hi) = (lo.as_dvec3(), hi.as_dvec3());
        let (c, e) = ((lo + hi) * 0.5, (hi - lo) * 0.5);
        (0..3).all(|k| {
            let a = self.axes[k];
            let mid = c.dot(a);
            let r = e.x * a.x.abs() + e.y * a.y.abs() + e.z * a.z.abs();
            mid + r >= if k == 2 { from } else { self.lo[k] } && mid - r <= self.hi[k]
        })
    }
}

/// Where the cascades' texels lie in the world as the sun moves (`fit`): the
/// light's across and up axes the last frame took, and what each cascade
/// takes off a point's place along them. Turned with the sun, the axes move
/// a point's place along them by its depth along the light from the world's
/// origin times the turn: at Freeport, 25,000 out, a frame of the game's day
/// (60 a second) swept cascade 1's texels through the town 11 to 15 at a
/// time, every edge drawn on texels shuffled anew each frame. So each
/// cascade takes back what the turn moves the middle of its slice by (in the
/// first, about where a third-person camera's character stands), and a
/// point moves within its texels by its depth from there times the turn
/// alone (what its sphere holds, under a twentieth of a texel a frame), and
/// not at all while the sun stands still: the camera moves the map by whole
/// texels as before.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Grid {
    axes: Option<[DVec3; 2]>,
    off: [[f64; 2]; CASCADES],
}

/// A frame's cascades for the view `view_proj` (the world's, as `View`
/// has it) whose camera is at `eye`, under a sun whose light travels along
/// `sun_dir`, `size` texels a side, on the texels `grid` has kept (and moves
/// on with the sun). Every matrix is built in doubles from the view relative
/// to the eye (`view_proj`'s own translation is the eye's to a millimetre
/// or so, not to nothing: the camera's apex is found, not assumed), each
/// sphere's centre snapped to the cascade's texels, its radius rounded up to
/// a sixteenth: the same world point falls on the same texel's centre as the
/// camera moves.
pub fn fit(view_proj: Mat4, eye: Vec3, sun_dir: Vec3, size: u32, grid: &mut Grid) -> [Cascade; CASCADES] {
    fit_to(view_proj, eye, sun_dir, size, grid, ENDS)
}

/// `fit` with the cascades ending at `ends` (`ends`: the options' Shadow
/// Distance).
pub fn fit_to(view_proj: Mat4, eye: Vec3, sun_dir: Vec3, size: u32, grid: &mut Grid, ends: [f32; CASCADES]) -> [Cascade; CASCADES] {
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
    let ax = axes(sun_dir.as_dvec3().normalize_or(DVec3::NEG_Y));
    let turned = grid.axes.replace([ax[0], ax[1]]).map(|p| [ax[0] - p[0], ax[1] - p[1]]);
    let s = size.max(4) as f64;
    std::array::from_fn(|c| {
        let (n, f) = (if c == 0 { 0.0 } else { ends[c - 1] as f64 }, ends[c] as f64);
        // the sphere round the slice's eight corners, its centre on the axis
        let zc = ((n + f) * (1.0 + k2) * 0.5).min(f);
        let r = ((f - zc).powi(2) + k2 * f * f).max((zc - n).powi(2) + k2 * n * n).sqrt();
        let r = (r * 16.0).ceil() / 16.0;
        // a texel's margin round it, so the snapped box still holds it
        let t = 2.0 * r / (s - 2.0);
        let half = r + t;
        let centre = eye + apex + fwd * zc;
        // what the sun's turn since the last frame moved the slice's middle by
        let off = &mut grid.off[c];
        if let Some(d) = turned {
            let pivot = eye + apex + fwd * ((n + f) * 0.5);
            off[0] += pivot.dot(d[0]);
            off[1] += pivot.dot(d[1]);
        }
        let snap = |v: f64| (v / t).round() * t;
        let (x0, y0, zm) = (snap(centre.dot(ax[0]) - off[0]), snap(centre.dot(ax[1]) - off[1]), centre.dot(ax[2]));
        let (z0, z1) = (zm - r - UPWIND, zm + r);
        // a caster CASTER_HEIGHT over a point of the sphere is that over the
        // sine of the sun's height nearer the sun
        let up = -ax[2].y;
        let reach = if up > 1e-6 { (CASTER_HEIGHT / up).min(UPWIND) } else { UPWIND };
        let depth = z1 - z0;
        // rows over a position less the eye: (p + eye)·axis less the box's
        let row = |a: DVec3, at: f64, scale: f64| DVec4::new(a.x / scale, a.y / scale, a.z / scale, (eye.dot(a) - at) / scale);
        let (x0, y0) = (x0 + off[0], y0 + off[1]);
        let m = DMat4::from_cols(row(ax[0], x0, half), row(ax[1], y0, half), row(ax[2], z0, depth), DVec4::W).transpose();
        Cascade {
            m: m.as_mat4(),
            texel: t as f32,
            depth: depth as f32,
            axes: ax,
            lo: [x0 - half, y0 - half, z0],
            hi: [x0 + half, y0 + half, z1],
            casters_from: zm - r - reach,
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
    fn new(cascades: &[Cascade; CASCADES], strength: f32, size: u32, ends: [f32; CASCADES]) -> Self {
        // the shadows fade out over the last 15% of their reach (`FADE_FROM`)
        let (fade, range) = if ends == ENDS { (FADE_FROM, RANGE) } else { (ends[CASCADES - 1] * (FADE_FROM / RANGE), ends[CASCADES - 1]) };
        ShadowUniform {
            m: std::array::from_fn(|c| cascades[c].m.to_cols_array_2d()),
            ends,
            texel: std::array::from_fn(|c| cascades[c].texel),
            inv_depth: std::array::from_fn(|c| 1.0 / cascades[c].depth.max(1e-3)),
            params: [strength, fade, range, 1.0 / size as f32],
        }
    }
}

/// `suncast.wgsl`'s `Cascade`: the matrix, and how far a face turned from the
/// sun is put back in its clip z.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CascadeUniform {
    m: [[f32; 4]; 4],
    push: [f32; 4],
}

/// The sun's line of lit.wgsl, and a radial flora plant's return, as the
/// receivers change them (`receivers_wgsl`).
const SUN_LINE: &str = "    light += frame.sun.rgb * max(-dot(n, frame.sun_dir.xyz), 0.0) * i.base.w;\n";
const SUN_LINE_SHADOWED: &str = "    light += frame.sun.rgb * max(-dot(n, frame.sun_dir.xyz), 0.0) * min(i.base.w, sun_shadow(i, n));\n";
const PLANT_RETURN: &str = "        return min(light, vec3<f32>(1.0));\n";
const PLANT_RETURN_SHADOWED: &str = "        return min(light - frame.sun.rgb * (0.15 * (1.0 - sun_shadow(i, vec3<f32>(0.0, 1.0, 0.0)))), vec3<f32>(1.0));\n";

/// The receivers' module: shader.wgsl, lit.wgsl with its sun term taking the
/// map's visibility (the lesser of the two) and a plant's sun the map's
/// (`shadowed_lit`), then `sunshadow.wgsl`. `None` if lit.wgsl's lines are
/// not there to change (tests::the_receivers_take_the_sun_line holds them
/// to it).
pub(crate) fn receivers_wgsl() -> Option<String> {
    let lit = shadowed_lit(include_str!("lit.wgsl"))?;
    Some([include_str!("shader.wgsl"), &lit, include_str!("sunshadow.wgsl")].join("\n"))
}

/// lit.wgsl's text `lit` with its sun line and a plant's return taking the
/// map's visibility, as the receivers have them (Light Shadows' with the
/// sun's too, `lightshadow::receivers_wgsl`, change this further).
pub(crate) fn shadowed_lit(lit: &str) -> Option<String> {
    if lit.matches(SUN_LINE).count() != 1 || lit.matches(PLANT_RETURN).count() != 1 {
        return None;
    }
    Some(lit.replace(SUN_LINE, SUN_LINE_SHADOWED).replace(PLANT_RETURN, PLANT_RETURN_SHADOWED))
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

/// A group's casters for Sun Shadows, its members in clusters of those near
/// one another, each cluster's box and its opaque layers into each cascade.
/// The group's runs keep its drawing order (`upload_group`), which scatters a
/// run's members over the zone (its sprites' placements go together): by
/// run, Freeport's cascades drew three times the world's draws; by cluster,
/// a cascade draws what its box reaches.
pub(crate) struct GroupSun {
    clusters: Vec<Cluster>,
}

struct Cluster {
    bounds: (Vec3, Vec3),
    bundles: Vec<Option<wgpu::RenderBundle>>,
    /// draw calls a cascade's bundle holds
    draws: u32,
    /// Its members are the wide ones (the terrain, a sprite placed all over
    /// the zone): drawn wherever the cascade's whole box reaches them.
    wide: bool,
}

/// A cluster's cell, across the ground.
const CELL: f32 = 48.0;
/// A member wider than this (a sprite placed all over the zone, the
/// terrain) goes with the other wide ones, in their order.
const WIDE: f32 = 96.0;
/// Members a cluster holds at most.
const CLUSTER_MOST: usize = 48;

/// The world box `instances` (column-major) put a model's `bounds` in: each
/// instance's eight corners, and a skinned model's box widened by its own
/// size and a little more, where its palette may move its vertices (a
/// character's limbs as it moves).
pub(crate) fn world_box((lo, hi): (Vec3, Vec3), skinned: bool, instances: &[[[f32; 4]; 4]]) -> Option<(Vec3, Vec3)> {
    if lo.x > hi.x {
        return None;
    }
    let pad = if skinned { (hi - lo).max_element() + 2.0 } else { 0.0 };
    instances.iter().fold(None, |acc, m| {
        let m = Mat4::from_cols_array_2d(m);
        let (a, b) = (0..8).map(|k| m.transform_point3(Vec3::new([lo.x, hi.x][k & 1], [lo.y, hi.y][k >> 1 & 1], [lo.z, hi.z][k >> 2]))).fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), w| (a.min(w), b.max(w)));
        let (a, b) = (a - Vec3::splat(pad), b + Vec3::splat(pad));
        Some(acc.map_or((a, b), |(p, q): (Vec3, Vec3)| (p.min(a), q.max(b))))
    })
}

/// The map, its pipelines and uniforms, made the first time Sun Shadows are
/// drawn with Dynamic Lighting on (`Renderer::set_sun_shadows`) and kept:
/// the bundles a group records name them.
pub(crate) struct Sun {
    /// The receivers by blend mode, as `lit::Lit`'s: `[0]` for an instance
    /// lit by every light, `[1]` for a baked one.
    pub(crate) pipelines: [Vec<wgpu::RenderPipeline>; 2],
    /// Group 3 of the receivers: the lights (Dynamic Lighting's buffer), the
    /// map, its comparison sampler and the uniform; and those three, for
    /// Light Shadows' receivers with the sun's map (`lightshadow`).
    pub(crate) bg: wgpu::BindGroup,
    pub(crate) buf: wgpu::Buffer,
    pub(crate) whole: wgpu::TextureView,
    pub(crate) cmp: wgpu::Sampler,
    /// The casters: alpha tested, and a solid layer's, depth alone.
    caster: wgpu::RenderPipeline,
    solid: wgpu::RenderPipeline,
    /// Group 3 of the casters, one a cascade (its matrix).
    cascades: Vec<wgpu::BindGroup>,
    cascade_buf: wgpu::Buffer,
    stride: u64,
    /// Each cascade's layer, as the casters' depth attachment.
    layers: Vec<wgpu::TextureView>,
    size: u32,
    /// The last frame's draws into the map: the clusters' bundles executed
    /// and the draw calls in all.
    pub(crate) drawn: std::cell::Cell<(u32, u32)>,
    /// Where the cascades' texels lie as the sun moves (`fit`).
    grid: std::cell::Cell<Grid>,
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
        let stride = (size_of::<CascadeUniform>() as u64).next_multiple_of(device.limits().min_uniform_buffer_offset_alignment as u64);
        let cascade_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sun shadow cascades"),
            size: stride * CASCADES as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let cascade_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sun shadow cascade"),
            entries: &[crate::uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let cascades = (0..CASCADES as u64)
            .map(|c| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("sun shadow cascade"),
                    layout: &cascade_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &cascade_buf, offset: c * stride, size: wgpu::BufferSize::new(size_of::<CascadeUniform>() as u64) }),
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
        let cast_pipeline = |vs: &str, fs: Option<&str>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("sun shadow casters"),
                layout: Some(&cast_layout),
                vertex: wgpu::VertexState { module: &cast_module, entry_point: Some(vs), compilation_options: Default::default(), buffers: &vertex_buffers() },
                // both faces, a vertex turned from the sun put back (`BACK_PUSH`):
                // culled, a cave's roof or a ceiling seen from above casts
                // nothing; drawn as they are, the undersides of thin things (an
                // awning's patches, a roof's planks a little over its top)
                // shadowed their own tops
                primitive: wgpu::PrimitiveState { front_face: crate::FRONT_FACE, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: fs.map(|fs| wgpu::FragmentState { module: &cast_module, entry_point: Some(fs), compilation_options: Default::default(), targets: &[] }),
                multiview_mask: None,
                cache: None,
            })
        };
        let (caster, solid) = (cast_pipeline("vs_cast", Some("fs_cast")), cast_pipeline("vs_cast_solid", None));
        Some(Sun { pipelines, bg, buf, whole, cmp, caster, solid, cascades, cascade_buf, stride, layers, size, drawn: Default::default(), grid: Default::default() })
    }

    /// A group's casters, in clusters: its members that draw an opaque
    /// layer, by the cell their box's middle is in (the wide ones apart, in
    /// order), at most `CLUSTER_MOST` a cluster, each recorded into every
    /// cascade.
    pub(crate) fn casters(&self, r: &Renderer, device: &wgpu::Device, g: &GpuGroup) -> GroupSun {
        let opaque = |passes: &[crate::Material]| passes.first().is_some_and(|p| p.blend == 0);
        let mut keyed: Vec<(i64, usize)> = Vec::new();
        for (k, m) in g.members.iter().enumerate() {
            if m.instances == 0 || !m.batches.iter().any(|(_, p)| opaque(p)) {
                continue;
            }
            let Some((lo, hi)) = m.bounds else { continue };
            let key = if (hi - lo).max_element() > WIDE {
                i64::MAX
            } else {
                let c = (lo + hi) * 0.5;
                ((c.x / CELL).floor() as i64) << 32 | ((c.z / CELL).floor() as i64 & 0xFFFF_FFFF)
            };
            keyed.push((key, k));
        }
        keyed.sort_unstable();
        let mut clusters = Vec::new();
        for same in keyed.chunk_by(|a, b| a.0 == b.0) {
            for chunk in same.chunks(CLUSTER_MOST) {
                let members: Vec<usize> = chunk.iter().map(|&(_, k)| k).collect();
                let Some(bounds) = members.iter().filter_map(|&k| g.members[k].bounds).reduce(|(a, b), (c, d)| (a.min(c), b.max(d))) else { continue };
                let mut draws = 0;
                let bundles = (0..CASCADES)
                    .map(|c| {
                        let mut e = device.create_render_bundle_encoder(&wgpu::RenderBundleEncoderDescriptor {
                            label: Some("sun shadow casters"),
                            color_formats: &[],
                            depth_stencil: Some(wgpu::RenderBundleDepthStencil { format: DEPTH_FORMAT, depth_read_only: false, stencil_read_only: true }),
                            sample_count: 1,
                            multiview: None,
                        });
                        e.set_bind_group(0, &r.frame_bg, &[]);
                        e.set_bind_group(2, &r.identity_palette, &[]);
                        e.set_bind_group(3, &self.cascades[c], &[]);
                        e.set_vertex_buffer(0, g.vertices.slice(..));
                        e.set_index_buffer(g.indices.slice(..), wgpu::IndexFormat::Uint32);
                        draws = 0;
                        // the solid layers first, depth alone (any layer's
                        // group bound once: the vertex alone reads none),
                        // then the alpha tested ones, each with its texture
                        for solid in [true, false] {
                            let mut first = true;
                            for &k in &members {
                                let m = &g.members[k];
                                let mut layers = m.batches.iter().filter(|(_, p)| opaque(p)).flat_map(|(range, p)| p.iter().filter(|p| p.blend == 0 && p.solid == solid).map(move |p| (range, p))).peekable();
                                if layers.peek().is_none() {
                                    continue;
                                }
                                if first {
                                    e.set_pipeline(if solid { &self.solid } else { &self.caster });
                                    first = false;
                                }
                                let (i0, n) = (m.instance0 as u64, m.instances as u64);
                                e.set_vertex_buffer(1, g.instances.slice(i0 * 64..(i0 + n) * 64));
                                e.set_vertex_buffer(2, g.statics.slice(i0 * 16..(i0 + n) * 16));
                                for (range, p) in layers {
                                    if !solid || draws == 0 {
                                        e.set_bind_group(1, &p.bg, &[]);
                                    }
                                    e.draw_indexed(range.clone(), 0, 0..m.instances);
                                    draws += 1;
                                }
                            }
                        }
                        Some(e.finish(&wgpu::RenderBundleDescriptor { label: Some("sun shadow casters") }))
                    })
                    .collect();
                clusters.push(Cluster { bounds, bundles, draws, wide: same[0].0 == i64::MAX });
            }
        }
        GroupSun { clusters }
    }

    /// Draws the frame's casters into the map for `view`, and sets the
    /// receivers' uniform: whether the sun is up enough to cast (`strength`),
    /// its light not black. `false`: no shadow this frame (the receivers
    /// need not be used). Every cascade is drawn every frame: the far two
    /// drawn in turn, as at first (each every other frame, kept a frame
    /// between), left a moving thing's shadow where it had been at every
    /// other frame, and its sunlit side in it (a box running 0.15 a frame 74
    /// off: its shadow's centroid back 4.4 pixels and on 6.4 by turns, of
    /// 512); every cascade every frame costs a third of a millisecond more on
    /// an RX 580.
    pub(crate) fn draw(&self, r: &Renderer, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &View, items: &[Draw]) -> bool {
        let dir = view.sun_dir.normalize_or(Vec3::NEG_Y);
        let k = strength(dir);
        if k <= 0.0 || view.sun.max_element() < 0.5 / 255.0 {
            self.drawn.set((0, 0));
            return false;
        }
        let mut grid = self.grid.get();
        let ends = ends(r.shadow_scale);
        let cascades = fit_to(view.view_proj, view.eye, dir, self.size, &mut grid, ends);
        self.grid.set(grid);
        queue.write_buffer(&self.buf, 0, bytemuck::bytes_of(&ShadowUniform::new(&cascades, k, self.size, ends)));
        for (c, cascade) in cascades.iter().enumerate() {
            let push = [(BACK_PUSH / cascade.depth as f64) as f32, 0.0, 0.0, 0.0];
            let u = CascadeUniform { m: cascade.m.to_cols_array_2d(), push };
            queue.write_buffer(&self.cascade_buf, c as u64 * self.stride, bytemuck::bytes_of(&u));
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
            let mut current: Option<bool> = None;
            for item in items {
                match item {
                    Draw::Group(g) => {
                        let reached = g.sun.iter().flat_map(|s| &s.clusters).filter(|k| if k.wide { cascade.reaches(k.bounds) } else { cascade.reaches_casters(k.bounds) });
                        let bundles: Vec<&wgpu::RenderBundle> = reached.filter_map(|k| {
                            draws += k.draws;
                            k.bundles[c].as_ref()
                        }).collect();
                        if !bundles.is_empty() {
                            runs += bundles.len() as u32;
                            pass.execute_bundles(bundles);
                            bound = false;
                            current = None;
                        }
                    }
                    Draw::Model(m) => {
                        // a model cleared of casting (`GpuModel::casts`: a
                        // Wisp's) is drawn into no cascade
                        if !m.casts || m.world.get().is_some_and(|b| !cascade.reaches(b)) {
                            continue;
                        }
                        if !bound {
                            pass.set_bind_group(0, &r.frame_bg, &[]);
                            pass.set_bind_group(3, &self.cascades[c], &[]);
                            bound = true;
                            current = None;
                        }
                        draws += self.encode_model(&mut pass, m, &mut current);
                    }
                    // the radial flora casts nothing
                    Draw::Instances(..) => {}
                }
            }
        }
        self.drawn.set((runs, draws));
        true
    }

    /// The receivers' uniform with no shadow in it, for a frame the sun
    /// casts none in (`draw` false: by night) that Light Shadows' receivers
    /// with the sun's map still draw through (`lightshadow`): their sun term
    /// then the strip programs' alone.
    pub(crate) fn unshadowed(&self, queue: &wgpu::Queue) {
        let mut u = ShadowUniform::zeroed();
        u.params = [0.0, FADE_FROM, RANGE, 1.0 / self.size as f32];
        queue.write_buffer(&self.buf, 0, bytemuck::bytes_of(&u));
    }

    /// One model's opaque layers into a cascade (groups 0 and 3 set; the
    /// pipeline set, solid or alpha tested, `current`): the batches of its
    /// level drawn whose first layer is opaque, as `encode_model` draws them
    /// in the first pass. Returns the draw calls.
    fn encode_model<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, m: &'a GpuModel, current: &mut Option<bool>) -> u32 {
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
                let solid = p.solid();
                if *current != Some(solid) {
                    pass.set_pipeline(if solid { &self.solid } else { &self.caster });
                    *current = Some(solid);
                }
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
    /// Freeport's x, on texels the sun has turned a while. The control: the
    /// range's far corner is outside the first cascade's box.
    #[test]
    fn each_cascade_holds_its_slice() {
        let noon = Vec3::new(-0.3, -0.95, 0.0).normalize();
        for eye in [Vec3::new(0.0, 5.0, 0.0), Vec3::new(25_300.0, 60.0, 15_700.0)] {
            let look = Vec3::new(1.0, -0.3, 0.4);
            let vp = view(eye, eye + look);
            // on texels the sun has turned (and the camera moved) a while
            let mut grid = Grid::default();
            for k in 0..30 {
                let (a, e) = ((200.0 + k as f32).to_radians(), eye + Vec3::new(3.0, 0.0, -2.0) * k as f32);
                fit(view(e, e + look), e, Vec3::new(-a.sin(), a.cos(), 0.0), 2048, &mut grid);
            }
            let cs = fit(vp, eye, noon, 2048, &mut grid);
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
        let (mut drifted, mut grid) = (0, Grid::default());
        for step in 0..100 {
            let eye = Vec3::new(25_300.0 + step as f32 * 0.0137, 60.0, 15_700.0 + step as f32 * 0.0071);
            let vp = view(eye, eye + Vec3::new(1.0, -0.4, 0.3));
            let c = fit(vp, eye, noon, 2048, &mut grid)[1];
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

    /// A cluster of casters far toward the sun is drawn only as far as one
    /// [`CASTER_HEIGHT`] tall could shadow the cascade from: at 45 degrees a
    /// box 400 up the light from the sphere is out, and 100 up it is in;
    /// the whole box (the terrain's test) takes both. Under a low sun the
    /// two are the same.
    #[test]
    fn a_far_cluster_is_drawn_where_it_can_shadow() {
        let eye = Vec3::new(25_300.0, 60.0, 15_700.0);
        let vp = view(eye, eye + Vec3::new(1.0, -0.3, 0.4));
        let sun = Vec3::new(1.0, -1.0, 0.0).normalize();
        let c = fit(vp, eye, sun, 2048, &mut Grid::default())[0];
        // the sphere's middle, then boxes up the light from it
        let mid = (c.axes[0] * ((c.lo[0] + c.hi[0]) * 0.5) + c.axes[1] * ((c.lo[1] + c.hi[1]) * 0.5) + c.axes[2] * (c.hi[2] - 20.0)).as_vec3();
        let at = |s: f32| {
            let p = mid - sun * s;
            (p - Vec3::ONE, p + Vec3::ONE)
        };
        assert!(c.reaches_casters(at(100.0)) && c.reaches(at(100.0)), "100 up the light");
        assert!(!c.reaches_casters(at(400.0)), "400 up the light, past a 150-tall caster's reach");
        assert!(c.reaches(at(400.0)), "the whole box takes it");
        let low = Vec3::new(1.0, -0.15, 0.0).normalize();
        let c = fit(vp, eye, low, 2048, &mut Grid::default())[0];
        assert_eq!(c.casters_from, c.lo[2], "under a low sun, the whole box");
    }

    /// A layer is solid when no texel of it can fail the alpha test: its
    /// least alpha times its colour's at 0.5 or more (128 of 255 with a full
    /// colour, 255 at a half), not one under.
    #[test]
    fn a_layer_is_solid_when_nothing_fails_its_test() {
        assert!(crate::solid(128, 1.0) && crate::solid(255, 1.0) && crate::solid(255, 0.5));
        assert!(!crate::solid(127, 1.0) && !crate::solid(0, 1.0) && !crate::solid(255, 0.49));
    }

    /// The options' Shadow Distance: at the game's (1) the cascades end
    /// exactly where they did, and raised to 4 the last reaches 1,000, a point
    /// 900 down the view inside it, which the game's last does not hold; the
    /// near two the same.
    #[test]
    fn the_shadow_distance_stretches_the_far_cascades() {
        assert_eq!(ends(1.0), ENDS, "the game's, exactly");
        assert_eq!(ends(0.5), ENDS);
        let far = ends(4.0);
        assert_eq!((far[0], far[1], far[2], far[3]), (ENDS[0], ENDS[1], ENDS[2] * 2.0, 1000.0));
        let eye = Vec3::new(25_300.0, 60.0, 15_700.0);
        let look = Vec3::new(1.0, -0.05, 0.3);
        let vp = view(eye, eye + look);
        let sun = Vec3::new(-0.4, -0.9, 0.1).normalize();
        let p = eye.as_dvec3() + look.as_dvec3().normalize() * 900.0;
        let inside = |c: &Cascade| {
            let q = c.m.as_dmat4() * (p - eye.as_dvec3()).extend(1.0);
            q.x.abs() <= 1.0 && q.y.abs() <= 1.0 && (0.0..=1.0).contains(&q.z)
        };
        let raised = fit_to(vp, eye, sun, 2048, &mut Grid::default(), far);
        let game = fit(vp, eye, sun, 2048, &mut Grid::default());
        assert!(inside(&raised[3]) && !inside(&game[3]), "900 out: in the raised last cascade, not the game's");
        assert_eq!((raised[0], raised[1]), (game[0], game[1]), "the near two the same");
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

    /// The map follows the sun as it is, its texels still in the world: as
    /// the sun turns a hundredth of a degree at a time (six frames of the
    /// game's day) over Freeport, the light through a point 10 over the
    /// ground meets the map a little farther from the ground under it at
    /// every turn, by the same each time (the shadow moving with the sun,
    /// smoothly), while the ground in the middle of cascade 1's slice stays
    /// where it was within its texel and the ground 10 off it moves on by a
    /// few hundredths of a texel a turn: with the camera standing, and
    /// walking. The controls: on fresh texels each time (the light's axes
    /// turned from the world's origin) the ground lands at a new place in
    /// its texel at almost every turn, the texels sweeping through it; and
    /// the sun taken in eighths of a degree (the build before) holds the
    /// shadow still over most turns, then moves it at once. The light's up
    /// axis is the world's z all along.
    #[test]
    fn the_map_follows_the_sun_smoothly() {
        let base = Vec3::new(25_300.0, 60.0, 15_700.0);
        let look = Vec3::new(1.0, -0.3, 0.4);
        let at = |deg: f64| {
            let a = deg.to_radians();
            Vec3::new(-(a.sin() as f32), a.cos() as f32, 0.0)
        };
        // the middle of cascade 1's slice on the view's axis, the ground 10
        // off it, and a point 10 over that
        let mid = base.as_dvec3() + look.as_dvec3().normalize() * ((ENDS[0] + ENDS[1]) as f64 * 0.5);
        let ground = mid + DVec3::new(10.0, 0.0, -4.0);
        let top = ground + DVec3::Y * 10.0;
        // a point's place across cascade 1, in texels
        let place = |c: &Cascade, eye: Vec3, p: DVec3| ((c.m.as_dmat4() * (p - eye.as_dvec3()).extend(1.0)).x * 0.5 + 0.5) * 2048.0;
        // how far a place moved within its texel (either way)
        let within = |a: f64, b: f64| {
            let d = (a - b).rem_euclid(1.0);
            d.min(1.0 - d)
        };
        for walk in [0.0f32, 0.0137] {
            let mut grid = Grid::default();
            let mut last: Option<[f64; 4]> = None;
            let (mut moves, mut swept, mut worst) = (Vec::new(), 0, [0.0f64; 2]);
            for k in 0..=40 {
                let deg = 225.0 + 0.01 * k as f64;
                let eye = base + Vec3::new(walk, 0.0, walk * 0.5) * k as f32;
                let vp = view(eye, eye + look);
                let c = fit(vp, eye, at(deg), 2048, &mut grid)[1];
                assert!((c.axes[1] - DVec3::Z).length() < 1e-9, "the map's up the world's z");
                let fresh = fit(vp, eye, at(deg), 2048, &mut Grid::default())[1];
                let now = [place(&c, eye, mid), place(&c, eye, ground), place(&c, eye, top) - place(&c, eye, ground), place(&fresh, eye, ground)];
                if let Some(was) = last {
                    worst = [worst[0].max(within(now[0], was[0])), worst[1].max(within(now[1], was[1]))];
                    moves.push(now[2] - was[2]);
                    if within(now[3], was[3]) > 0.05 {
                        swept += 1;
                    }
                }
                last = Some(now);
            }
            let mean = moves.iter().sum::<f64>() / moves.len() as f64;
            assert!(worst[0] < if walk == 0.0 { 1e-3 } else { 5e-3 } && worst[1] < 0.05, "walking {walk}: the ground moved within its texel by up to {worst:?} a turn");
            assert!(mean.abs() > 0.01 && moves.iter().all(|m| (m - mean).abs() < 0.1 * mean.abs()), "walking {walk}: the shadow's moves {moves:?}");
            assert!(swept >= 35, "the control, walking {walk}: on fresh texels the ground moved within its texel at {swept} of 40 turns");
        }
        // the control: the sun taken in eighths of a degree, as before
        let eye = base;
        let vp = view(eye, eye + look);
        let stepped = |k: usize| {
            let deg = ((225.0 + 0.01 * k as f64) / 0.125).round() * 0.125;
            let c = fit(vp, eye, at(deg), 2048, &mut Grid::default())[1];
            place(&c, eye, top) - place(&c, eye, ground)
        };
        let shadow: Vec<f64> = (0..=40).map(stepped).collect();
        let still = shadow.windows(2).filter(|w| w[0] == w[1]).count();
        let jump = shadow.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f64::max);
        assert!(still >= 35 && jump > 5.0 * (shadow[40] - shadow[0]).abs() / 40.0, "stepped: {still} of 40 turns still, a jump of {jump}");
    }
}
