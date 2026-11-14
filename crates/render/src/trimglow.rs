//! The armour trim's aura (the owner, 2026-11-13: "lets also let armor trim
//! glow also. and make dusts armor trim glow a black aura"): the trim of an
//! armour piece (the plate's gold, every other set's own: `vi::cust::TRIMMED`,
//! `crate::trimart`) lit in an aura's colour, with a soft halo bleeding off it
//! onto the rest, or with a dark aura the
//! trim taken toward black under a dark sheen and a smoky shadow off it.
//!
//! It is the piece's batches drawn again over themselves ([`Trim`], set on its
//! [`crate::GpuModel`] by `Renderer::set_trims`), skinned as the body is and
//! pushed out along the smoothed normals ([`crate::shell::smooth_normals`]) a
//! little ([`INFLATE`] of the shell's), so it follows every animation and sits
//! just over the armour. Its fragment samples the trim's mask
//! ([`crate::recolor::trim_mask_of`], the texture the piece's own was made from
//! at the first: the same weight the trim dye's recolour reads, its halo
//! blurred into the mask's green): `trimglow.wgsl` lights the trim's texels,
//! its halo fainter where the surface faces the eye and stronger at its
//! silhouette, with the shell's slow shimmer and pulse.
//!
//! A glowing trim is drawn additively (`Cs·As + Cd`, the shell's blend), a
//! dark one premultiplied over ([`crate::shell::DARK_BLEND`]: the trim and its
//! halo darken what is under them, a faint sheen on top). Depth tested, no
//! depth written, back faces culled.
//!
//! Cost: one draw call a part (a piece's batches of one texture, nearly
//! always one a piece), an index buffer of its triangles a LOD level, the
//! mask texture (cached as any recoloured texture is) and a pipeline pair
//! made at the first trim. A model with no trim is drawn exactly as before.

use bytemuck::{Pod, Zeroable};

use crate::BufferInit;

/// How far the trim's draw stands off the armour, as a share of the shell's
/// ([`crate::shell::inflation`]): just enough to sit over it.
pub const INFLATE: f32 = 0.3;

/// `trimglow.wgsl`'s `Trim`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct TrimUniform {
    /// the aura's colour (0..1) and its strength
    pub color: [f32; 4],
    /// x: the inflation, y: the shimmer's frequency, z: 1 dark, w unused
    pub params: [f32; 4],
}

/// One trim's aura: its colour, dark or not, and the batches it lights, each
/// with the texture id of its trim's mask (`recolor::recolored` of the root
/// texture by `recolor::TRIM_MASK`; resolved by `LibraryTextures`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrimOf {
    pub color: [u8; 3],
    pub dark: bool,
    pub batches: Vec<(usize, i32)>,
}

/// One draw of a trim: a run of batches of one mask.
pub(crate) struct Part {
    pub(crate) bg: wgpu::BindGroup,
    _uniform: wgpu::Buffer,
    pub(crate) runs: Vec<(Option<u8>, wgpu::Buffer, u32)>,
    pub(crate) dark: bool,
}

impl Part {
    /// The run drawn at `level`, as `shell::Part::run`.
    pub(crate) fn run(&self, level: Option<u8>) -> Option<&(Option<u8>, wgpu::Buffer, u32)> {
        level.and_then(|l| self.runs.iter().find(|r| r.0 == Some(l))).or_else(|| self.runs.iter().find(|r| r.0.is_none()))
    }
}

/// A model's trims' auras, drawn after its blended layers.
pub struct Trim {
    pub(crate) parts: Vec<Part>,
    pub(crate) normals: wgpu::Buffer,
}

impl Trim {
    /// How many draws it makes.
    pub fn parts(&self) -> usize {
        self.parts.len()
    }
}

/// `trims`' parts by mask: each trim's batches grouped by their mask's id, in
/// the order first met (a trim of no batch left out).
pub fn by_mask(trims: &[TrimOf]) -> Vec<(usize, i32, Vec<usize>)> {
    let mut out: Vec<(usize, i32, Vec<usize>)> = Vec::new();
    for (k, t) in trims.iter().enumerate() {
        for &(b, mask) in &t.batches {
            match out.iter_mut().find(|(j, m, _)| *j == k && *m == mask) {
                Some(p) => p.2.push(b),
                None => out.push((k, mask, vec![b])),
            }
        }
    }
    out
}

/// The trims' pipelines (a glowing trim's and a dark one's), made at the first.
pub(crate) struct Pipeline {
    pub(crate) glow: wgpu::RenderPipeline,
    pub(crate) dark: wgpu::RenderPipeline,
}

impl Pipeline {
    /// Over the frame's layout, a layer's (`material`: its texture, sampler
    /// and uniform, which the trim's mask and `TrimUniform` take) and the
    /// skin palette's.
    pub(crate) fn new(device: &wgpu::Device, frame: &wgpu::BindGroupLayout, material: &wgpu::BindGroupLayout, palette: &wgpu::BindGroupLayout, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::include_wgsl!("trimglow.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("trim glow"),
            bind_group_layouts: &[Some(frame), Some(material), Some(palette)],
            immediate_size: 0,
        });
        // the model's own vertices (position, uv, bones, weights of `Vertex`),
        // its instances, and the smoothed normals
        let vertex = [
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 12, shader_location: 1 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint8x4, offset: 48, shader_location: 8 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint8x4, offset: 52, shader_location: 9 },
        ];
        let make = |blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("trim glow"),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_trim"),
                    compilation_options: Default::default(),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<crate::Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &vertex }),
                        Some(wgpu::VertexBufferLayout {
                            array_stride: 64,
                            step_mode: wgpu::VertexStepMode::Instance,
                            attributes: &wgpu::vertex_attr_array![4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4],
                        }),
                        Some(wgpu::VertexBufferLayout { array_stride: 12, step_mode: wgpu::VertexStepMode::Vertex, attributes: &wgpu::vertex_attr_array![2 => Float32x3] }),
                    ],
                },
                primitive: wgpu::PrimitiveState { front_face: crate::FRONT_FACE, cull_mode: Some(wgpu::Face::Back), ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: crate::DEPTH_FORMAT,
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::LessEqual),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_trim"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::COLOR })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        Pipeline { glow: make(crate::blend_state(1)), dark: make(Some(crate::shell::DARK_BLEND)) }
    }

    /// The trims of model `m` (as uploaded, its bounds `bounds`): a part a
    /// trim and mask (`by_mask`) whose mask `mask` resolves (its view) and
    /// whose batches have triangles; none when no part is left. The bind
    /// groups take `material`'s layout with `sampler`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn trim(
        &self,
        device: &wgpu::Device,
        material: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        m: &esf::Model,
        bounds: (glam::Vec3, glam::Vec3),
        trims: &[TrimOf],
        mut mask: impl FnMut(i32) -> Option<wgpu::TextureView>,
    ) -> Option<Trim> {
        let inflate = crate::shell::inflation(bounds) * INFLATE;
        let size = (bounds.1 - bounds.0).max(glam::Vec3::ZERO).max_element().max(1e-3);
        let mut parts = Vec::new();
        for (k, id, batches) in by_mask(trims) {
            let Some(view) = mask(id) else { continue };
            let runs: Vec<_> = crate::shell::runs_of(m, Some(&batches))
                .into_iter()
                .map(|(l, idx)| {
                    let b = device.buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("trim indices"), contents: bytemuck::cast_slice(&idx), usage: wgpu::BufferUsages::INDEX });
                    (l, b, idx.len() as u32)
                })
                .collect();
            if runs.is_empty() {
                continue;
            }
            let t = &trims[k];
            let c = t.color;
            let u = TrimUniform { color: [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0], params: [inflate, 6.0 / size, t.dark as u8 as f32, 0.0] };
            let uniform = device.buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("trim glow"), contents: bytemuck::bytes_of(&u), usage: wgpu::BufferUsages::UNIFORM });
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("trim glow"),
                layout: material,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                    wgpu::BindGroupEntry { binding: 2, resource: uniform.as_entire_binding() },
                ],
            });
            parts.push(Part { bg, _uniform: uniform, runs, dark: t.dark });
        }
        if parts.is_empty() {
            return None;
        }
        let normals = crate::shell::smooth_normals(&m.pos, &m.normal);
        let normals = device.buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("trim normals"), contents: bytemuck::cast_slice(&normals), usage: wgpu::BufferUsages::VERTEX });
        Some(Trim { parts, normals })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trim's batches go in a part a mask, in the order met; two trims of
    /// one mask are two parts (each its own colour); a trim of nothing none.
    #[test]
    fn a_part_a_trim_and_mask() {
        let t = |c: u8, b: Vec<(usize, i32)>| TrimOf { color: [c; 3], dark: false, batches: b };
        let got = by_mask(&[t(1, vec![(0, 7), (2, 9), (3, 7)]), t(2, vec![]), t(3, vec![(4, 7)])]);
        assert_eq!(got, vec![(0, 7, vec![0, 3]), (0, 9, vec![2]), (2, 7, vec![4])]);
        assert!(by_mask(&[]).is_empty());
    }

    /// `trimglow.wgsl` validates and comes out as GLSL ES 3.00 for a page's
    /// WebGL2, both entry points (naga, as wgpu's GL backend writes it).
    #[test]
    fn the_shader_compiles_for_webgl2() {
        use wgpu::naga::{self, back::glsl};
        let src = include_str!("trimglow.wgsl");
        let module = naga::front::wgsl::parse_str(src).unwrap_or_else(|e| panic!("{}", e.emit_to_string(src)));
        let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap();
        let options = glsl::Options {
            version: glsl::Version::Embedded { version: 300, is_webgl: true },
            writer_flags: glsl::WriterFlags::ADJUST_COORDINATE_SPACE | glsl::WriterFlags::FORCE_POINT_SIZE,
            binding_map: Default::default(),
            zero_initialize_workgroup_memory: true,
        };
        let mut entries = Vec::new();
        for ep in &module.entry_points {
            let pipeline = glsl::PipelineOptions { shader_stage: ep.stage, entry_point: ep.name.clone(), multiview: None };
            let mut text = String::new();
            glsl::Writer::new(&mut text, &module, &info, &options, &pipeline, naga::proc::BoundsCheckPolicies::default()).and_then(|mut w| w.write().map(|_| ())).unwrap_or_else(|e| panic!("{}: {e:?}", ep.name));
            assert!(text.contains("#version 300 es"));
            entries.push(ep.name.clone());
        }
        assert_eq!(entries, ["vs_trim", "fs_trim"]);
    }
}
