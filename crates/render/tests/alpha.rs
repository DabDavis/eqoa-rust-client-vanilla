//! The target's alpha stays 1 wherever a model is drawn: an alpha-tested layer
//! whose texel passes the test with alpha below 1 writes its colour and leaves
//! the alpha alone. A page's WebGL2 canvas is composited, and a report's
//! picture of it taken, with that alpha (F3 20261020-202738: the colour of
//! every alpha-tested edge wrapped round into a cyan fringe in the picture).
//! Needs a GPU adapter (any backend); without one it says so and passes.

use render::{Draw, Renderer, View};

#[test]
fn an_alpha_tested_texel_leaves_the_target_opaque() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    // a red texel at alpha 200 of 255: past the test (128), not opaque
    let tex = |id: i32| (id == 7).then(|| esf::Image { width: 1, height: 1, rgba: vec![255, 0, 0, 200], alpha: true });
    let layer = esf::Layer { flags: 0, texture: 7, wrap: 0, blend: 0, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2] };
    // one triangle over the whole view (the projection is the identity),
    // facing it, lit white by its colour
    let model = esf::Model {
        pos: vec![[-3.0, -3.0, 0.5], [3.0, -3.0, 0.5], [0.0, 3.0, 0.5]],
        uv: vec![[0.5, 0.5]; 3],
        normal: vec![[0.0, 0.0, 1.0]; 3],
        color: vec![[1.0, 1.0, 1.0, 1.0]; 3],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2], lod: None }],
        ..Default::default()
    };
    let m = r.upload(&device, &queue, &model, &[render::IDENTITY], &tex);
    let view = View { sun: glam::Vec3::ZERO, ambient: glam::Vec3::ZERO, ..View::daylight(glam::Mat4::IDENTITY) };
    let px = render::capture(&device, &queue, &mut r, 4, 4, &view, &[Draw::Model(&m)], None);
    let at = (2 * 4 + 2) * 4;
    // drawn (not the clear's colour), in the texel's colour...
    assert_eq!(&px[at..at + 3], &[255, 0, 0], "the texel's colour");
    // ...with the target's alpha left at the clear's 1, not the texel's 200
    assert_eq!(px[at + 3], 255, "the target's alpha");
}
