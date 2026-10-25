//! A radial flora plant as `VIRStrip6` draws it (`Renderer::upload_flora`):
//! no normal and no vertex colour, so its light is the ambient plus the sun
//! times 0.15 whichever way the sun points, plus the point light's falloff
//! times 0.15 at the distance in the plant's own space (the world distance
//! over its scale); its fade (an instance's static-light alpha) is the vertex
//! alpha the texel's is multiplied by, so a blended plant lets the backdrop
//! through. `Draw::Instances` draws the instances it names and no other.
//! The control: the same quad uploaded as any other model is lit by the sun's
//! direction, and a world-distance falloff would be another colour.
//! Needs a graphics adapter (any backend); without one it says so and passes.

use render::{Draw, Renderer, View};

/// A quad facing the camera at depth 1, filling the view, under one layer:
/// untextured white, blended (3) or opaque (0).
fn quad(blend: i32) -> esf::Model {
    let layer = esf::Layer { flags: 0, texture: 0, wrap: 0, blend, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2] };
    let s = 1.5;
    esf::Model {
        pos: vec![[-s, -s, 0.0], [s, -s, 0.0], [s, s, 0.0], [-s, -s, 0.0], [s, s, 0.0], [-s, s, 0.0]],
        uv: vec![[0.0, 0.0]; 6],
        normal: vec![[0.0, 1.0, 0.0]; 6],
        color: vec![[1.0, 1.0, 1.0, 1.0]; 6],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2, 3, 4, 5], lod: None }],
        ..Default::default()
    }
}

/// Instance `k` of `m` placed at depth 1 at scale `scale`, the rest far
/// behind the camera; the centre pixel of the capture.
fn centre(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer, m: &render::GpuModel, draw: Draw, view: &View) -> [f32; 3] {
    let _ = m;
    let px = render::capture(device, queue, r, 4, 4, view, &[draw], None);
    let i = (2 * 4 + 2) * 4;
    [px[i] as f32 / 255.0, px[i + 1] as f32 / 255.0, px[i + 2] as f32 / 255.0]
}

fn place(scale: f32, z: f32) -> [[f32; 4]; 4] {
    glam::Mat4::from_scale_rotation_translation(glam::Vec3::splat(scale), glam::Quat::IDENTITY, glam::Vec3::new(0.0, 0.0, z)).to_cols_array_2d()
}

#[test]
fn a_plant_is_lit_and_faded_as_strip6() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let none = |_: i32| None;
    let proj = glam::camera::rh::proj::directx::perspective(90f32.to_radians(), 1.0, 0.1, 1000.0);
    let base = View { sun: glam::Vec3::new(1.0, 0.5, 0.25), ambient: glam::Vec3::new(0.1, 0.2, 0.3), sun_dir: glam::Vec3::new(0.0, 0.0, 1.0), clear: glam::Vec3::ZERO, ..View::daylight(proj) };
    let near = |a: [f32; 3], b: [f32; 3], tol: f32| a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol);
    let lit = [0.1 + 0.15, 0.2 + 0.075, 0.3 + 0.0375];

    let opaque = r.upload_flora(&device, &queue, &quad(0), 4, &none);
    // instance 0 in view, 1 behind the camera
    r.set_instances(&queue, &opaque, &[place(1.0, -1.0), place(1.0, 50.0)]);
    r.set_static_lights(&queue, &opaque, &[[0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0]]);
    let got = centre(&device, &queue, &mut r, &opaque, Draw::Instances(&opaque, 0..1), &base);
    assert!(near(got, lit, 2.5 / 255.0), "ambient + sun x 0.15: {got:?}, want {lit:?}");
    // the sun the other way: the same (no normal)
    let other = View { sun_dir: glam::Vec3::new(0.0, 0.0, -1.0), ..base };
    let got = centre(&device, &queue, &mut r, &opaque, Draw::Instances(&opaque, 0..1), &other);
    assert!(near(got, lit, 2.5 / 255.0), "the sun's direction counted: {got:?}");
    // only the instances named: instance 1 alone draws nothing
    let got = centre(&device, &queue, &mut r, &opaque, Draw::Instances(&opaque, 1..2), &base);
    assert!(near(got, [0.0; 3], 1.5 / 255.0), "an instance not named was drawn: {got:?}");

    // the point light 10 behind the eye on the axis, range 20, white; the
    // plant at scale 2: its own distance is 10 / 2 (the corners a little more)
    r.set_instances(&queue, &opaque, &[place(2.0, -1.0)]);
    let lamp = View { point: Some((glam::Vec3::new(0.0, 0.0, 9.0), 20.0, glam::Vec3::ONE)), ..base };
    let got = centre(&device, &queue, &mut r, &opaque, Draw::Instances(&opaque, 0..1), &lamp);
    let own = 0.15 * (1.0 - 5.0 / 20.0);
    let world = 0.15 * (1.0 - 10.0 / 20.0);
    let want = lit.map(|c| c + own);
    assert!(near(got, want, 3.0 / 255.0), "the point light's falloff in the plant's space: {got:?}, want {want:?}");
    assert!(!near(got, lit.map(|c| c + world), 3.0 / 255.0), "the control: a world-distance falloff is another colour");

    // fade: a blended plant over black at fade 0.5 is half its light
    let blended = r.upload_flora(&device, &queue, &quad(3), 2, &none);
    r.set_instances(&queue, &blended, &[place(1.0, -1.0)]);
    r.set_static_lights(&queue, &blended, &[[0.0, 0.0, 0.0, 0.5]]);
    let got = centre(&device, &queue, &mut r, &blended, Draw::Instances(&blended, 0..1), &base);
    assert!(near(got, lit.map(|c| c * 0.5), 2.5 / 255.0), "faded to 0.5: {got:?}");

    // the control: the same quad as any model is lit by the sun's direction
    let plain = r.upload(&device, &queue, &quad(0), &[place(1.0, -1.0)], &none);
    let away = View { sun_dir: glam::Vec3::new(0.0, 1.0, 0.0), ..base };
    let got = centre(&device, &queue, &mut r, &plain, Draw::Model(&plain), &away);
    assert!(!near(got, lit, 2.5 / 255.0), "the control: an ordinary model lit as a plant: {got:?}");
}
