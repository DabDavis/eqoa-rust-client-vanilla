//! A sprite's edge on a scaled target keeps to the sprite's own texels. The
//! title's sail is one 54-texel cell of an atlas whose cells are framed in
//! opaque black (splashAnims2's column 54); sailing right it is drawn
//! mirrored, so its left edge is that cell's u = 54. At 1.5 target pixels a
//! frame pixel (960x720), every other frame x puts that edge on a pixel's
//! centre, and the pixel took the black column: a line at the stern while it
//! moved. The 2D layer samples as the GS does, at the frame pixel's texel
//! (`ui::SAMPLE_BIAS`). That bias moved the edge, and at other sizes a
//! pixel's centre falls on the moved one (F3 20261024-045053, 1918x1078: a
//! dark line in the start menu's title plate, where its mirrored right cap
//! begins), so a quad also keeps to its own texels (`ui.wgsl`'s `own`).
//! Needs a GPU adapter (any backend); without one it says so and passes.

use render::ui::{Quad, Ui, SAMPLE_BIAS};
use render::{Renderer, View};

/// A device, or `None` (said) without an adapter.
fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return None;
    };
    Some(pollster::block_on(adapter.request_device(&Default::default())).unwrap())
}

/// A 128x128 atlas: a white cell in columns `x0..x1`, black elsewhere.
fn atlas(x0: usize, x1: usize) -> Vec<u8> {
    let mut rgba = vec![0u8; 128 * 128 * 4];
    for y in 0..128 {
        for x in 0..128 {
            let i = (y * 128 + x) * 4;
            let v = if (x0..x1).contains(&x) { 255 } else { 0 };
            rgba[i..i + 4].copy_from_slice(&[v, v, v, 255]);
        }
    }
    rgba
}

/// Texels `u0..u1` (rows 0..33) at frame x `x`, `u1 - u0` wide, mirrored
/// (blit flags 1: the far uv at the left corners).
fn mirrored(tex: usize, x: f32, u0: f32, u1: f32) -> Quad {
    let w = u1 - u0;
    Quad {
        tex,
        blend: 3,
        pos: [[x, 100.0], [x + w, 100.0], [x + w, 133.0], [x, 133.0]],
        uv: [[u1 / 128.0, 0.0], [u0 / 128.0, 0.0], [u0 / 128.0, 33.0 / 128.0], [u1 / 128.0, 33.0 / 128.0]],
        color: [1.0; 4],
        clip: None,
    }
}

/// `w`x`h` drawn with `quads` over green: its RGBA rows.
fn drawn(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32, rgba: &[u8], quads: impl Fn(usize) -> Vec<Quad>) -> Vec<u8> {
    let mut r = Renderer::new(device, queue, render::TARGET_FORMAT);
    let mut ui = Ui::new(device, render::TARGET_FORMAT, render::DEPTH_FORMAT);
    let tex = ui.add_texture(device, queue, 128, 128, rgba);
    ui.begin((w, h));
    for q in quads(tex) {
        ui.push(q);
    }
    r.ui = Some(ui);
    let view = View { clear: glam::Vec3::new(0.0, 1.0, 0.0), ..View::daylight(glam::Mat4::IDENTITY) };
    render::capture(device, queue, &mut r, w, h, &view, &[], None)
}

/// Pixel (`x`, `y`)'s RGB in a `w` wide capture.
fn rgb(px: &[u8], w: usize, x: usize, y: usize) -> [u8; 3] {
    let i = (y * w + x) * 4;
    [px[i], px[i + 1], px[i + 2]]
}

/// A mirrored quad whose left edge, moved by the bias, is exactly a pixel's
/// centre: 960x720 is 1.5 target pixels a frame pixel with no side margin,
/// so frame x 67 + the bias puts the edge at target 100.5 (and 167 + it at
/// 250.5). A pixel centre on a left edge is the quad's (the rasterisers'
/// top-left rule), at the quad's far uv: without the quad's own texels it
/// takes the texel past its cell (the control's, black). The control: the
/// same quad one texel wider into the black shows the black there.
#[test]
fn a_mirrored_sprite_on_its_moved_edge_keeps_to_its_cell() {
    let Some((device, queue)) = device() else { return };
    let px = drawn(&device, &queue, 960, 720, &atlas(0, 54), |tex| vec![mirrored(tex, 67.0 + SAMPLE_BIAS, 0.0, 54.0), mirrored(tex, 167.0 + SAMPLE_BIAS, 0.0, 55.0)]);
    let at = |x, y| rgb(&px, 960, x, y);
    assert_eq!(at(100, 186), [255, 255, 255], "a pixel centre on the moved edge: the cell's white, not the black past it");
    assert_eq!(at(101, 186), [255, 255, 255]);
    assert_eq!(at(99, 186), [0, 255, 0], "left of the quad, the clear");
    assert_eq!(at(250, 186), [0, 0, 0], "the control: a quad over the black column shows it");
}

/// The report's own: the start menu's "Group" title plate at 1918x1078
/// (`StartMenu`'s `blit_bar`, 118 wide at frame x 261: atlas 33's 16x28 cap
/// at texels 52..68, the middle, the cap mirrored at x 363). The cap's left
/// edge lands within a 256th of target pixel 1055's centre, which a GPU
/// snapping to eight sub-pixel bits (RADV, lavapipe) puts on it: column 68
/// of the sheet, black, was a line down the plate.
#[test]
fn the_title_plates_mirrored_cap_keeps_to_its_cell_at_1918x1078() {
    let Some((device, queue)) = device() else { return };
    let px = drawn(&device, &queue, 1918, 1078, &atlas(52, 68), |tex| {
        let cap = mirrored(tex, 363.0, 52.0, 68.0);
        // the plate's middle, left of the cap: the same white (texels 52..68
        // stretched over 86 frame pixels)
        let mut mid = mirrored(tex, 277.0, 52.0, 68.0);
        for (p, x) in mid.pos.iter_mut().zip([277.0, 363.0, 363.0, 277.0]) {
            p[0] = x;
        }
        mid.uv = [[52.0 / 128.0, 0.0], [68.0 / 128.0, 0.0], [68.0 / 128.0, 33.0 / 128.0], [52.0 / 128.0, 33.0 / 128.0]];
        vec![mid, cap]
    });
    // frame y 116 is target row 279
    for x in 1050..1062 {
        assert_eq!(rgb(&px, 1918, x, 279), [255, 255, 255], "column {x}: the plate, no line where the cap begins");
    }
}

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
