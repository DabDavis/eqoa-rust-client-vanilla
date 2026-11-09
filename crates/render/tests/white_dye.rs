//! A slot dyed White (`render::WHITE_DYE`, go-eqoa-server's 0xFFFFFEFF) is
//! drawn bleached: a colour multiplies its texture, so plain white (an undyed
//! slot's tint 0) leaves a brown leather brown, where the White dye takes the
//! brown out and lifts it toward white (shader.wgsl's `tinted`), by the PS2's
//! pipelines and by Dynamic Lighting's alike. Every other colour, one a blue
//! step off it too, multiplies as ever. Needs a GPU adapter (any backend);
//! without one it says so and passes.

use render::{Draw, Lights, Renderer, View};

/// The pixel at the view's centre of one leather-brown (150, 100, 50) texel's
/// triangle in a layer of `color`, under a full ambient (a light of 1).
fn pixel(r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, color: [u8; 4], lit: bool) -> [u8; 3] {
    let tex = |id: i32| (id == 7).then(|| esf::Image { width: 1, height: 1, rgba: vec![150, 100, 50, 255], alpha: false });
    let layer = esf::Layer { flags: 0, texture: 7, wrap: 0, blend: 0, color, uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
    let model = esf::Model {
        pos: vec![[-3.0, -3.0, 0.5], [3.0, -3.0, 0.5], [0.0, 3.0, 0.5]],
        uv: vec![[0.5, 0.5]; 3],
        normal: vec![[0.0, 0.0, 1.0]; 3],
        color: vec![[0.0, 0.0, 0.0, 1.0]; 3],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2], lod: None }],
        ..Default::default()
    };
    let m = r.upload(device, queue, &model, &[render::IDENTITY], &tex);
    let mut view = View { sun: glam::Vec3::ZERO, ambient: glam::Vec3::ONE, clear: glam::Vec3::ZERO, ..View::daylight(glam::Mat4::IDENTITY) };
    if lit {
        view.lights = Some(Lights::default());
    }
    let px = render::capture(device, queue, r, 4, 4, &view, &[Draw::Model(&m)], None);
    let at = (2 * 4 + 2) * 4;
    [px[at], px[at + 1], px[at + 2]]
}

#[test]
fn a_white_dye_bleaches_where_white_leaves_the_texture() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let near = |got: [u8; 3], want: [f32; 3], what: &str| {
        assert!(got.iter().zip(want).all(|(&g, w)| (g as f32 - w).abs() <= 2.0), "{what}: {got:?}, want {want:?}");
    };
    // the brown's luminance: 0.299·150 + 0.587·100 + 0.114·50 = 109.25
    let l = 109.25 / 255.0;
    let bleached = (1.0 - (1.0 - l) * 0.5) * 255.0;
    for lit in [false, true] {
        let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
        if lit {
            r.set_dynamic_lighting(&device, true);
        }
        let how = if lit { "lit" } else { "the PS2's" };
        // the control: white (tint 0, an undyed slot) leaves the brown
        near(pixel(&mut r, &device, &queue, [255; 4], lit), [150.0, 100.0, 50.0], &format!("{how}: white"));
        // the White dye: a near white grey, the brown gone
        near(pixel(&mut r, &device, &queue, render::WHITE_DYE, lit), [bleached; 3], &format!("{how}: the White dye"));
        // another colour, one a blue step off it too, multiplies as ever
        near(pixel(&mut r, &device, &queue, [0xFF, 0xFF, 0xFD, 0xFF], lit), [150.0, 100.0, 49.8], &format!("{how}: fffffd"));
        near(pixel(&mut r, &device, &queue, [0xD1, 0, 0, 0xFF], lit), [123.0, 0.0, 0.0], &format!("{how}: red"));
    }
}
