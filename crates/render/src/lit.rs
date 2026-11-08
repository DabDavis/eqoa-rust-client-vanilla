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
use glam::Vec3;

/// The point lights a frame takes at most, past the personal light: the
/// uniform holds this many, and `lit.wgsl` loops over those in use.
pub const MAX_LIGHTS: usize = 16;

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
}

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
    count: [u32; 4],
    /// each light's (position less the eye, range) and (colour, 1 fixed)
    l: [[[f32; 4]; 2]; MAX_LIGHTS],
}

impl LightsUniform {
    /// `lights` as the shader takes them, relative to `eye` (positions near
    /// Freeport's x 25,300 step 2 mm as floats: the difference is taken here,
    /// where it is exact for a light near the eye, as vs_main takes the
    /// models').
    pub(crate) fn new(lights: &Lights, eye: Vec3) -> Self {
        let mut u = LightsUniform::zeroed();
        u.count[0] = lights.len() as u32;
        for (k, p) in lights.as_slice().iter().enumerate() {
            u.l[k] = [(p.at - eye).extend(p.range.max(0.0)).to_array(), p.color.extend(p.fixed as i32 as f32).to_array()];
        }
        u
    }
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
