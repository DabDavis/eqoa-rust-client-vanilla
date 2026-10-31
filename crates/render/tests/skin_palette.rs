//! The skin palette holds every node a model on the disc is skinned to.
//! The training dummies' body (CHAR.ESF 0x66262F1C, a tree) has 68 nodes and
//! its right hand is skinned to nodes 64 and 65; with a palette of 64 those
//! vertices were drawn at the model's origin (the shader skipped the node
//! but kept its weight), a thin wedge from the hand down to the feet (F3
//! 20261025-215658). The disc test needs `run/eqoa.iso` (and takes the 2002
//! disc's CHAR.ESF too when `$EQOA_2002_ISO` or the default path has it);
//! the drawing test needs a graphics adapter. Each skips without.

use render::{Draw, MAX_BONES, Renderer, TextureSource, View};

/// The largest node any vertex of a CSprite in `c` is weighted to, and the
/// CSprite (DictID) it is in.
fn largest_bone(c: esf::Container) -> (usize, i32, usize) {
    let mut lib = esf::Library::new();
    let file = lib.add("CHAR.ESF", c);
    let (mut most, mut at, mut sprites) = (0usize, 0i32, 0usize);
    for r in lib.file(file).models().into_iter().filter(|r| r.ty == 0x2700) {
        let Some(m) = lib.build_model(esf::Ref { file, node: r.node }) else { continue };
        sprites += 1;
        for (b, w) in m.bones.iter().zip(&m.weights) {
            for k in 0..4 {
                if w[k] > 0 && b[k] != 255 && b[k] as usize >= most {
                    most = b[k] as usize;
                    at = r.dict_id;
                }
            }
        }
    }
    (most, at, sprites)
}

#[test]
fn every_skeleton_on_the_disc_fits_the_palette() {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
    let Ok(mut iso) = iso::Iso::open(&p) else {
        eprintln!("SKIP: no disc at run/eqoa.iso");
        return;
    };
    let mut discs = vec![("Frontiers", iso.read("CHAR.ESF").unwrap())];
    let y2002 = std::env::var("EQOA_2002_ISO").map(std::path::PathBuf::from).ok().or_else(|| {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../decomp-builds/discs/eqoa-usa");
        std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "iso"))
    });
    if let Some(Ok(mut i)) = y2002.map(|p| iso::Iso::open(&p))
        && let Ok(b) = i.read("CHAR.ESF")
    {
        discs.push(("2002", b));
    }
    for (name, bytes) in discs {
        let (most, at, sprites) = largest_bone(esf::Container::open(bytes).unwrap());
        eprintln!("{name}: {sprites} CSprites, the largest node a vertex is skinned to is {most} (in {at:#010x})");
        assert!(sprites > 300, "{name}: CSprites built");
        // the control: the disc does need more than the old 64
        assert!(most >= 64, "{name}: a body is skinned past node 63");
        assert!(most < MAX_BONES, "{name}: node {most} (in {at:#010x}) is past the palette of {MAX_BONES}");
    }
}

#[test]
fn the_shader_palette_is_max_bones() {
    let s = include_str!("../src/shader.wgsl");
    assert!(s.contains(&format!("m: array<mat4x4<f32>, {MAX_BONES}>,")), "the palette's array is MAX_BONES long");
    assert!(s.contains(&format!("if (b < {MAX_BONES}u && v.weights[k] > 0u) {{")), "the shader takes every node of the palette");
}

/// Texture 7: white.
struct White;

impl TextureSource for White {
    fn image(&self, id: i32) -> Option<esf::Image> {
        (id == 7).then(|| esf::Image { width: 4, height: 4, rgba: [255u8; 4].repeat(16), alpha: false })
    }
}

/// A triangle round the origin, every vertex skinned wholly to node 66,
/// drawn 2 units ahead (a view 90 degrees wide over 16 pixels: 4 pixels a
/// unit) with node 66 moved `dx` to the right: the brightness of the pixel
/// row through it at the origin's column and at `dx`'s.
fn drawn(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer, dx: f32) -> (u8, u8) {
    let layer = esf::Layer { flags: 0, texture: 7, wrap: 0, blend: 0, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.0, uv_rate: [0.0; 2], emissive: [0; 4] };
    let s = 0.6;
    let model = esf::Model {
        pos: vec![[-s, -s, -2.0], [s, -s, -2.0], [0.0, s, -2.0]],
        uv: vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]],
        normal: vec![[0.0, 0.0, 1.0]; 3],
        color: vec![[1.0, 1.0, 1.0, 1.0]; 3],
        bones: vec![[66, 255, 255, 255]; 3],
        weights: vec![[255, 0, 0, 0]; 3],
        skinned: true,
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2], lod: None }],
        ..Default::default()
    };
    let m = r.upload(device, queue, &model, &[render::IDENTITY], &White);
    let mut palette = vec![render::IDENTITY; 68];
    palette[66][3] = [dx, 0.0, 0.0, 1.0];
    r.set_palette(queue, &m, &palette);
    let proj = glam::camera::rh::proj::directx::perspective(90f32.to_radians(), 1.0, 0.5, 100.0);
    let view = View { sun: glam::Vec3::ZERO, ambient: glam::Vec3::ONE, ..View::daylight(proj) };
    let px = render::capture(device, queue, r, 16, 16, &view, &[Draw::Model(&m)], None);
    let at = |x: usize| px[(8 * 16 + x) * 4];
    (at(8), at((8.0 + dx * 4.0) as usize))
}

#[test]
fn a_vertex_on_node_66_is_drawn_where_node_66_is() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    // the control: node 66 where it was, the triangle round the origin
    let (origin, _) = drawn(&device, &queue, &mut r, 0.0);
    assert!(origin > 200, "an unmoved node 66 draws the triangle at the origin ({origin})");
    // node 66 moved 1.5 to the right: the triangle goes with it (a palette
    // too short for node 66 drew it as a point at the origin, nothing)
    let (left, there) = drawn(&device, &queue, &mut r, 1.5);
    assert!(there > 200, "the triangle is drawn where node 66 put it ({there})");
    assert!(left < 120, "and not at the origin ({left})");
}
