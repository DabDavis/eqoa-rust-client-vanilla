//! A sprite's edge on a scaled target keeps to the sprite's own texels. The
//! title's sail is one 54-texel cell of an atlas whose cells are framed in
//! opaque black (splashAnims2's column 54); sailing right it is drawn
//! mirrored, so its left edge is that cell's u = 54. At 1.5 target pixels a
//! frame pixel (960x720), every other frame x puts that edge on a pixel's
//! centre, and the pixel took the black column: a line at the stern while it
//! moved. The 2D layer samples as the GS does, at the frame pixel's texel
//! (`ui::SAMPLE_BIAS`). Needs a GPU adapter (any backend); without one it
//! says so and passes.

use render::ui::{Quad, Ui};
use render::{Renderer, View};

#[test]
fn a_mirrored_sprite_on_a_pixel_centre_keeps_to_its_cell() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let mut ui = Ui::new(&device, render::TARGET_FORMAT, render::DEPTH_FORMAT);
    // a 128x128 atlas: a white cell in columns 0-53, black from column 54 on
    let mut rgba = vec![0u8; 128 * 128 * 4];
    for y in 0..128 {
        for x in 0..128 {
            let i = (y * 128 + x) * 4;
            let v = if x < 54 { 255 } else { 0 };
            rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    let tex = ui.add_texture(&device, &queue, 128, 128, &rgba);
    let (w, h) = (960u32, 720u32);
    ui.begin((w, h));
    // the cell mirrored (blit flags 1) at frame x 1 (target 1.5, a pixel's
    // centre) and x 101 (151.5); the control: the same quad one texel wider
    // into the black, at x 201
    let quad = |x: f32, u: f32| Quad {
        tex,
        blend: 3,
        pos: [[x, 100.0], [x + 54.0, 100.0], [x + 54.0, 133.0], [x, 133.0]],
        uv: [[u / 128.0, 0.0], [0.0, 0.0], [0.0, 33.0 / 128.0], [u / 128.0, 33.0 / 128.0]],
        color: [1.0; 4],
        clip: None,
    };
    ui.push(quad(1.0, 54.0));
    ui.push(quad(101.0, 54.0));
    ui.push(quad(201.0, 55.0));
    r.ui = Some(ui);
    let view = View { clear: glam::Vec3::new(0.0, 1.0, 0.0), ..View::daylight(glam::Mat4::IDENTITY) };
    let px = render::capture(&device, &queue, &mut r, w, h, &view, &[], None);
    // a row through the cells' middle (frame y 116), the column whose centre
    // is each quad's left edge (frame x 1, 101, 201: target 1.5, 151.5, 301.5)
    let at = |x: usize| {
        let i = (186 * w as usize + x) * 4;
        [px[i], px[i + 1], px[i + 2]]
    };
    assert_eq!(at(1), [255, 255, 255], "the stern at a pixel centre: the cell's white, not the frame's black");
    assert_eq!(at(151), [255, 255, 255], "and at another");
    // the control: a quad that does reach into the black shows it there
    assert_eq!(at(301), [0, 0, 0], "a quad over the black column shows it");
}
