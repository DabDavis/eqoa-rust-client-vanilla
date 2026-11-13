//! A glowing creature's aura shell: an NPC whose body the server tints with a
//! glow (`eqoa`'s `npctint`, layout 2; Mr. Moogie in Freeport) wears one aura
//! round its whole body, form fitting, as Anarchy Online draws some buffs (the
//! owner, 2026-11-13: "the moogie aura should only be one aura that
//! encompasses his whole body form fitting").
//!
//! It is the body's own skinned mesh drawn a second time ([`Shell`], set on
//! its [`crate::GpuModel`] by `Renderer::set_shell`): each vertex pushed out
//! along its normal by [`inflation`] (a fraction of the body's own size, so a
//! large creature's shell stands as far off it, in proportion, as a small
//! one's), before the skin palette, so the shell bends with every node the
//! body bends with. The normals pushed along are the mesh's own averaged over
//! the vertices that share a place ([`smooth_normals`]), so a seam of the
//! texture's does not open a crack in the shell.
//!
//! `shell.wgsl` draws it additively (the GS's blend 1, `Cs·As + Cd`) in the
//! glow's colour, depth tested against the body and writing no depth, its
//! back faces culled: its alpha is a rim (Fresnel-like: strongest where the
//! surface turns away from the eye, faint where it faces it), times a slow
//! shimmer (value noise in the body's own space, drifting upward with time)
//! and a gentle pulse, faded by the fog as the body is. Nothing opaque.
//!
//! A shell can be in parts ([`Part`]): each covers some of the model's
//! batches (a worn armour piece's: its bracers alone, its breastplate alone)
//! in a colour of its own, or all of them.
//!
//! Cost: one draw call more a part for the model that wears it (its LOD
//! level's batches of the part in one index buffer), a vertex buffer of its
//! normals and a pipeline made at the first shell. A model with no shell (every other
//! creature, every player, everything else) is drawn exactly as before.

use bytemuck::{Pod, Zeroable};

use crate::BufferInit;

/// How far the shell stands off the body, as a fraction of the body's
/// largest extent in its own space ([`inflation`]).
pub const INFLATE: f32 = 0.02;

/// `shell.wgsl`'s `Shell`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ShellUniform {
    /// the glow's colour (0..1) and its strength
    pub color: [f32; 4],
    /// x: the inflation, y: the shimmer's frequency (per unit of the body's
    /// own space), z and w unused
    pub params: [f32; 4],
}

/// What a shell's part covers, and in which colour: `batches` the model's
/// batches it covers (indices into `esf::Model::batches`), `None` all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartOf {
    pub color: [u8; 3],
    pub batches: Option<Vec<usize>>,
}

/// One part of a shell, drawn in one call.
pub(crate) struct Part {
    pub(crate) bg: wgpu::BindGroup,
    _uniform: wgpu::Buffer,
    /// The part's triangles of each LOD level (its batches and those of none)
    /// in one index buffer, and of every level (`None`): `GpuModel::level`
    /// picks.
    pub(crate) runs: Vec<(Option<u8>, wgpu::Buffer, u32)>,
    color: [u8; 3],
}

/// A model's aura shell, drawn after its blended layers.
pub struct Shell {
    pub(crate) parts: Vec<Part>,
    /// Each vertex's smoothed normal ([`smooth_normals`]).
    pub(crate) normals: wgpu::Buffer,
}

impl Shell {
    /// Each part's colour, in order.
    pub fn colors(&self) -> Vec<[u8; 3]> {
        self.parts.iter().map(|p| p.color).collect()
    }
}

impl Part {
    /// The run drawn at `level` (`GpuModel::level`): that level's, else every
    /// level's.
    pub(crate) fn run(&self, level: Option<u8>) -> Option<&(Option<u8>, wgpu::Buffer, u32)> {
        level.and_then(|l| self.runs.iter().find(|r| r.0 == Some(l))).or_else(|| self.runs.iter().find(|r| r.0.is_none()))
    }
}

/// How far out the shell is pushed, in the body's own space: [`INFLATE`] of
/// the largest side of its bounds.
pub fn inflation(bounds: (glam::Vec3, glam::Vec3)) -> f32 {
    let d = (bounds.1 - bounds.0).max(glam::Vec3::ZERO);
    d.max_element() * INFLATE
}

/// Each vertex's normal averaged (unit length) with every other vertex's at
/// the same place (to a thousandth), so the shell pushed along them stays
/// closed across the mesh's seams; a vertex whose sum is nothing keeps its
/// own.
pub fn smooth_normals(pos: &[[f32; 3]], normal: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let key = |p: &[f32; 3]| p.map(|v| (v * 1000.0).round() as i64);
    let mut sums: std::collections::HashMap<[i64; 3], glam::Vec3> = std::collections::HashMap::new();
    for (p, n) in pos.iter().zip(normal) {
        let n = glam::Vec3::from(*n);
        if n.is_finite() {
            *sums.entry(key(p)).or_default() += n.normalize_or_zero();
        }
    }
    pos.iter()
        .zip(normal)
        .map(|(p, n)| {
            let s = sums.get(&key(p)).copied().unwrap_or_default();
            let own = glam::Vec3::from(*n).normalize_or_zero();
            if s.length_squared() > 1e-8 { s.normalize().to_array() } else { own.to_array() }
        })
        .collect()
}

/// The index runs of `m`'s batches `only` (`None`: all; a part's, [`PartOf`]):
/// every level's, then each level's.
pub fn runs_of(m: &esf::Model, only: Option<&[usize]>) -> Vec<(Option<u8>, Vec<u32>)> {
    let picked: Vec<&esf::Batch> = m.batches.iter().enumerate().filter(|(i, _)| only.is_none_or(|o| o.contains(i))).map(|(_, b)| b).collect();
    let mut levels: Vec<u8> = picked.iter().filter_map(|b| b.lod).collect();
    levels.sort_unstable();
    levels.dedup();
    let of = |pick: &dyn Fn(Option<u8>) -> bool| picked.iter().filter(|b| pick(b.lod)).flat_map(|b| b.indices.iter().copied()).collect::<Vec<u32>>();
    let mut out = vec![(None, of(&|_| true))];
    for l in levels {
        out.push((Some(l), of(&|lod| lod.is_none_or(|x| x == l))));
    }
    out.retain(|r| !r.1.is_empty());
    out
}

/// The shell's pipeline and its uniform's layout, made at the first shell.
pub(crate) struct Pipeline {
    pub(crate) pipeline: wgpu::RenderPipeline,
    pub(crate) layout: wgpu::BindGroupLayout,
}

impl Pipeline {
    pub(crate) fn new(device: &wgpu::Device, frame: &wgpu::BindGroupLayout, palette: &wgpu::BindGroupLayout, format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::include_wgsl!("shell.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shell"),
            entries: &[crate::uniform_entry(0, wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shell"),
            bind_group_layouts: &[Some(frame), Some(&layout), Some(palette)],
            immediate_size: 0,
        });
        // the model's own vertices (position, bones, weights of `Vertex`),
        // its instances, and the smoothed normals
        let vertex = [
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x3, offset: 0, shader_location: 0 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint8x4, offset: 48, shader_location: 8 },
            wgpu::VertexAttribute { format: wgpu::VertexFormat::Uint8x4, offset: 52, shader_location: 9 },
        ];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shell"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_shell"),
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
                entry_point: Some("fs_shell"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format, blend: crate::blend_state(1), write_mask: wgpu::ColorWrites::COLOR })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Pipeline { pipeline, layout }
    }

    /// A shell for model `m` (as uploaded, its bounds `bounds`) in `parts`
    /// (a part covering nothing left out); none when no part covers anything.
    /// The inflation and the shimmer are the whole model's, so a piece's part
    /// stands as far off as the body's would.
    pub(crate) fn shell(&self, device: &wgpu::Device, m: &esf::Model, bounds: (glam::Vec3, glam::Vec3), parts: &[PartOf]) -> Option<Shell> {
        let inflate = inflation(bounds);
        let size = (bounds.1 - bounds.0).max(glam::Vec3::ZERO).max_element().max(1e-3);
        let parts: Vec<Part> = parts
            .iter()
            .filter_map(|p| {
                let runs: Vec<_> = runs_of(m, p.batches.as_deref())
                    .into_iter()
                    .map(|(l, idx)| {
                        let b = device.buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("shell indices"), contents: bytemuck::cast_slice(&idx), usage: wgpu::BufferUsages::INDEX });
                        (l, b, idx.len() as u32)
                    })
                    .collect();
                if runs.is_empty() {
                    return None;
                }
                let c = p.color;
                let u = ShellUniform { color: [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0], params: [inflate, 6.0 / size, 0.0, 0.0] };
                let uniform = device.buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("shell"), contents: bytemuck::bytes_of(&u), usage: wgpu::BufferUsages::UNIFORM });
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("shell"),
                    layout: &self.layout,
                    entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
                });
                Some(Part { bg, _uniform: uniform, runs, color: c })
            })
            .collect();
        if parts.is_empty() {
            return None;
        }
        let normals = smooth_normals(&m.pos, &m.normal);
        let normals = device.buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("shell normals"), contents: bytemuck::cast_slice(&normals), usage: wgpu::BufferUsages::VERTEX });
        Some(Shell { parts, normals })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A seam's two vertices at one place take one normal between theirs; a
    /// vertex alone keeps its own.
    #[test]
    fn normals_are_shared_across_a_seam() {
        let pos = [[0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let n = smooth_normals(&pos, &[[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 2.0]]);
        let h = std::f32::consts::FRAC_1_SQRT_2;
        assert_eq!(n[0], n[1]);
        assert!((n[0][0] - h).abs() < 1e-6 && (n[0][1] - h).abs() < 1e-6 && n[0][2] == 0.0);
        assert_eq!(n[2], [0.0, 0.0, 1.0]);
    }

    /// `shell.wgsl` validates and comes out as GLSL ES 3.00 for a page's
    /// WebGL2, both entry points (naga, as wgpu's GL backend writes it).
    #[test]
    fn the_shader_compiles_for_webgl2() {
        use wgpu::naga::{self, back::glsl};
        let src = include_str!("shell.wgsl");
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
        assert_eq!(entries, ["vs_shell", "fs_shell"]);
    }

    /// The shell stands off a body in proportion to its size.
    #[test]
    fn inflation_is_the_bodys_size() {
        let b = (glam::Vec3::new(-1.0, 0.0, -0.5), glam::Vec3::new(1.0, 1.5, 0.5));
        assert!((inflation(b) - 2.0 * INFLATE).abs() < 1e-6);
        let big = (b.0 * 2.0, b.1 * 2.0);
        assert!((inflation(big) - 2.0 * inflation(b)).abs() < 1e-6);
    }

    /// A body with no LOD levels has one run of every triangle; a LOD body a
    /// run per level, each with its own and the shared batches'.
    #[test]
    fn a_run_a_level() {
        let batch = |lod: Option<u8>, i: u32| esf::Batch { material: 0, layers: Vec::new(), indices: vec![i, i, i], lod };
        let flat = esf::Model { batches: vec![batch(None, 0), batch(None, 1)], ..Default::default() };
        let r = runs_of(&flat, None);
        assert_eq!(r, vec![(None, vec![0, 0, 0, 1, 1, 1])]);
        assert_eq!(runs_of(&flat, Some(&[1])), vec![(None, vec![1, 1, 1])], "a part: its batches alone");
        assert!(runs_of(&flat, Some(&[])).is_empty(), "a part of nothing");
        let lod = esf::Model { batches: vec![batch(Some(0), 0), batch(Some(1), 1), batch(None, 2)], ..Default::default() };
        let r = runs_of(&lod, None);
        assert_eq!(r[1], (Some(0), vec![0, 0, 0, 2, 2, 2]));
        assert_eq!(r[2], (Some(1), vec![1, 1, 1, 2, 2, 2]));
        assert_eq!(r[0].1.len(), 9);
    }
}
