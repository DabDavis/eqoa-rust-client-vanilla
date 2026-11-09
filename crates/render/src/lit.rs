//! Dynamic Lighting (the options' Extended > Lighting: Off, the PS2's, or
//! On): the world drawn with the strip programs' light evaluated per
//! fragment instead of per vertex, the sun and the personal light as the
//! console has them (the same colours, ranges and falloff, `lit.wgsl`), and
//! up to [`MAX_LIGHTS`] point lights more, which `VIRaster::MaxPointLights`
//! (1, the personal light's) leaves the console without: what the scene's
//! fires and its placed lights cast, and spells and glowing weapons
//! (`eqoa`'s `lights`). A fixed light (a fire's) is not added on a baked
//! instance ([`crate::GpuModel::baked`], [`crate::GroupPart::baked`]): its
//! colour buffers were baked with the scene's fixed lights, and carry them.
//!
//! Off, nothing here is made or run: the PS2's pipelines (shader.wgsl) and
//! the groups' bundles recorded with them draw every frame, as before.
//! On, a frame whose view carries lights (`View::lights`) draws its models
//! through [`Lit`]'s pipelines and its groups' runs through their lit
//! bundles (`Renderer::relight`); any other view (the loading screen's
//! gears, a window's model) is drawn the PS2's way still.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};

/// The point lights a frame takes at most, past the personal light: the
/// uniform holds this many, and `lit.wgsl` loops over those in use.
pub const MAX_LIGHTS: usize = 16;

/// The screen's tiles, across and down: each holds a mask of the lights
/// whose sphere's box, projected, meets it (`tile_masks`), and a fragment
/// evaluates only its tile's. Exact: a point within a light's range is
/// inside its sphere, whose projection is inside the box's.
pub const TILES: (usize, usize) = (24, 16);

/// A point light as the strip programs light with it (`VIRasterPointLight`:
/// a position, a range and a colour), in the world.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointLight {
    pub at: Vec3,
    pub range: f32,
    /// rgb, the light at its centre (0..1)
    pub color: Vec3,
    /// A light fixed in the scene (a fire's, a placed light's), which a
    /// baked instance's colour buffers already hold.
    pub fixed: bool,
    /// Which light it is from frame to frame (`eqoa`'s `lights`: a fixed
    /// light's place in its list, a light of the moment's own), for what
    /// follows a light over frames (Light Shadows' maps, `lightshadow`); 0
    /// for one with none, [`PERSONAL_KEY`] the personal light's.
    pub key: u64,
}

/// The personal light's key ([`PointLight::key`]): the view's `point`, put
/// first in the frame's list ([`listed`]).
pub const PERSONAL_KEY: u64 = u64::MAX;

/// A frame's point lights, at most [`MAX_LIGHTS`] (the rest dropped), in a
/// form a view can copy.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lights {
    count: usize,
    list: [PointLight; MAX_LIGHTS],
}

impl Lights {
    pub fn new(lights: &[PointLight]) -> Self {
        let mut l = Lights::default();
        for p in lights.iter().take(MAX_LIGHTS) {
            l.list[l.count] = *p;
            l.count += 1;
        }
        l
    }

    pub fn as_slice(&self) -> &[PointLight] {
        &self.list[..self.count]
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

/// `lit.wgsl`'s `Lights`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct LightsUniform {
    /// x: lights in use, y and z: the tiles across and down, w: the mask of
    /// the fixed ones (a baked instance's fragments take the others alone)
    count: [u32; 4],
    /// each light's (position less the eye, range) and (colour, the range's
    /// inverse, negative for a fixed light)
    l: [[[f32; 4]; 2]; MAX_LIGHTS],
    /// each tile's mask of lights, four tiles a row, from the bottom left
    tiles: [[u32; 4]; TILES.0 * TILES.1 / 4],
}

/// The frame's lights as `lit.wgsl` takes them, in its order: the personal
/// light `point` (the view's: position, range, colour; none at range 0),
/// then `lights`, at most [`MAX_LIGHTS`] in all. The personal light is one
/// of the list, not fixed (the console's one point light lights baked
/// geometry too).
pub fn listed(point: Option<(Vec3, f32, Vec3)>, lights: &Lights) -> Lights {
    let personal = point.filter(|p| p.1 > 0.0).map(|(at, range, color)| PointLight { at, range, color, fixed: false, key: PERSONAL_KEY });
    let all: Vec<PointLight> = personal.into_iter().chain(lights.as_slice().iter().copied()).take(MAX_LIGHTS).collect();
    Lights::new(&all)
}

impl LightsUniform {
    /// The personal light `point` (the view's: position, range, colour),
    /// then `lights`, as the shader takes them, at most `MAX_LIGHTS` in all:
    /// relative to `eye` (positions near Freeport's x 25,300 step 2 mm as
    /// floats: the difference is taken here, where it is exact for a light
    /// near the eye, as vs_main takes the models'), and the tiles of the
    /// screen `view_proj` draws to that each can reach. The personal light is
    /// one of the list, not fixed (the console's one point light lights baked
    /// geometry too), so it is culled by the tiles and evaluated as cheaply.
    pub(crate) fn new(point: Option<(Vec3, f32, Vec3)>, lights: &Lights, eye: Vec3, view_proj: Mat4) -> Self {
        let lights = listed(point, lights);
        let mut u = LightsUniform::zeroed();
        let fixed = lights.as_slice().iter().enumerate().filter(|(_, p)| p.fixed).fold(0u32, |m, (k, _)| m | 1 << k);
        u.count = [lights.len() as u32, TILES.0 as u32, TILES.1 as u32, fixed];
        for (k, p) in lights.as_slice().iter().enumerate() {
            let r = p.range.max(1e-3);
            u.l[k] = [(p.at - eye).extend(p.range.max(0.0)).to_array(), p.color.extend(if p.fixed { -1.0 / r } else { 1.0 / r }).to_array()];
        }
        for (t, m) in tile_masks(lights.as_slice(), view_proj).iter().enumerate() {
            u.tiles[t / 4][t % 4] = *m;
        }
        u
    }
}

/// Each tile's mask of the lights that can reach it: a light's tiles are
/// those its sphere's bounding box covers projected through `view_proj`
/// (every one when the box reaches behind the camera, none when it is all
/// behind it), from the bottom left
/// in rows of `TILES.0`, as `lit.wgsl` finds a fragment's from its NDC.
pub fn tile_masks(lights: &[PointLight], view_proj: Mat4) -> Vec<u32> {
    let (tx, ty) = TILES;
    let mut masks = vec![0u32; tx * ty];
    for (k, p) in lights.iter().enumerate().take(MAX_LIGHTS) {
        let r = p.range.max(0.0);
        let (mut lo, mut hi) = (glam::Vec2::splat(f32::MAX), glam::Vec2::splat(f32::MIN));
        let mut behind = 0;
        for c in 0..8 {
            let corner = p.at + Vec3::new([-r, r][c & 1], [-r, r][c >> 1 & 1], [-r, r][c >> 2]);
            let clip = view_proj * corner.extend(1.0);
            if clip.w <= 1e-3 {
                behind += 1;
                continue;
            }
            let ndc = clip.truncate().truncate() / clip.w;
            lo = lo.min(ndc);
            hi = hi.max(ndc);
        }
        // wholly behind the camera: nothing; partly: every tile
        if behind == 8 {
            continue;
        }
        let whole = behind > 0;
        if !whole {
            (lo, hi) = (lo.max(glam::Vec2::splat(-1.0)), hi.min(glam::Vec2::splat(1.0)));
            if lo.x > hi.x || lo.y > hi.y {
                continue;
            }
        } else {
            (lo, hi) = (glam::Vec2::splat(-1.0), glam::Vec2::splat(1.0));
        }
        let tile = |v: f32, n: usize| (((v * 0.5 + 0.5) * n as f32) as isize).clamp(0, n as isize - 1) as usize;
        for y in tile(lo.y, ty)..=tile(hi.y, ty) {
            for x in tile(lo.x, tx)..=tile(hi.x, tx) {
                masks[y * tx + x] |= 1 << k;
            }
        }
    }
    masks
}

/// The lit pipelines and the lights' uniform, made the first time Dynamic
/// Lighting is turned on (`Renderer::set_dynamic_lighting`) and kept: the
/// lit bundles a group records name them.
pub(crate) struct Lit {
    /// By blend mode, as `Renderer::pipelines`: `[0]` for an instance lit by
    /// every light, `[1]` for a baked one (`fs_lit_baked`).
    pub(crate) pipelines: [Vec<wgpu::RenderPipeline>; 2],
    pub(crate) buf: wgpu::Buffer,
    pub(crate) bg: wgpu::BindGroup,
}

/// shader.wgsl's structs and bindings, then the lit entry points over them.
const LIT_WGSL: &str = concat!(include_str!("shader.wgsl"), "\n", include_str!("lit.wgsl"));

impl Lit {
    pub(crate) fn new(device: &wgpu::Device, layouts: [&wgpu::BindGroupLayout; 3], format: wgpu::TextureFormat) -> Lit {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("lit"), source: wgpu::ShaderSource::Wgsl(LIT_WGSL.into()) });
        let lights_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lights"),
            entries: &[crate::uniform_entry(0, wgpu::ShaderStages::FRAGMENT)],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lit model"),
            bind_group_layouts: &[Some(layouts[0]), Some(layouts[1]), Some(layouts[2]), Some(&lights_layout)],
            immediate_size: 0,
        });
        let make = |fs: &str| (0..5).map(|m| crate::model_pipeline(device, &layout, &module, ("vs_lit", fs), m, false, format)).collect();
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lights"),
            size: std::mem::size_of::<LightsUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lights"),
            layout: &lights_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
        });
        Lit { pipelines: [make("fs_lit"), make("fs_lit_baked")], buf, bg }
    }
}
