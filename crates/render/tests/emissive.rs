//! A material's emissive colour lights it as the strip programs light a
//! vertex: `VIRaster::UploadPerMaterial` uploads the ambient plus the
//! material's emissive as the programs' ambient, so the light is
//! min(colour + ambient + emissive + sun + point, 1) and an emissive
//! material stays lit in the dark (the Sword of Virtue's blade at night,
//! emissive (204, 255, 255): F3 pass 26, "it is supposed to be bright
//! sheen"). Needs a GPU adapter (any backend); without one it says so and
//! passes.

use render::{Draw, Renderer, View};

/// The pixel at the view's centre of one grey (128) texel's triangle, opaque
/// or added over it by a second layer, under `ambient`, with the material's
/// `emissive`.
fn pixel(r: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, emissive: [u8; 4], ambient: glam::Vec3, added: bool) -> [u8; 3] {
    let tex = |id: i32| (id == 7).then(|| esf::Image { width: 1, height: 1, rgba: vec![128, 128, 128, 255], alpha: false });
    let layer = |blend: i32, color: [u8; 4]| esf::Layer { flags: 0, texture: 7, wrap: 0, blend, color, uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2], emissive };
    // the Sword of Virtue's: the texel opaque, then itself added in
    // (204, 255, 255) at alpha 153
    let mut layers = vec![layer(0, [255; 4])];
    if added {
        layers.push(layer(1, [204, 255, 255, 153]));
    }
    // one triangle over the whole view, facing it; no vertex colour (an
    // item's mesh has none), no sun
    let model = esf::Model {
        pos: vec![[-3.0, -3.0, 0.5], [3.0, -3.0, 0.5], [0.0, 3.0, 0.5]],
        uv: vec![[0.5, 0.5]; 3],
        normal: vec![[0.0, 0.0, 1.0]; 3],
        color: vec![[0.0, 0.0, 0.0, 1.0]; 3],
        batches: vec![esf::Batch { material: 0, layers, indices: vec![0, 1, 2], lod: None }],
        ..Default::default()
    };
    let m = r.upload(device, queue, &model, &[render::IDENTITY], &tex);
    let view = View { sun: glam::Vec3::ZERO, ambient, clear: glam::Vec3::ZERO, ..View::daylight(glam::Mat4::IDENTITY) };
    let px = render::capture(device, queue, r, 4, 4, &view, &[Draw::Model(&m)], None);
    let at = (2 * 4 + 2) * 4;
    [px[at], px[at + 1], px[at + 2]]
}

#[test]
fn an_emissive_material_is_lit_by_its_emissive_colour() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let night = glam::Vec3::splat(0.2);
    let near = |got: [u8; 3], want: [f32; 3], what: &str| {
        assert!(got.iter().zip(want).all(|(&g, w)| (g as f32 - w).abs() <= 2.0), "{what}: {got:?}, want {want:?}");
    };
    // the control: no emissive, lit by the night's ambient alone
    near(pixel(&mut r, &device, &queue, [0; 4], night, false), [25.6; 3], "no emissive");
    // emissive (204, 255, 255): light min(0.2 + (0.8, 1, 1), 1) = 1
    near(pixel(&mut r, &device, &queue, [204, 255, 255, 255], night, false), [128.0; 3], "emissive");
    // a partial emissive adds to the ambient: 0.2 + 0.3
    near(pixel(&mut r, &device, &queue, [77, 77, 77, 255], night, false), [64.3; 3], "emissive 0.3");
    // the sword's sheen over it: 128 + 128 x (0.8, 1, 1) x 0.6
    near(pixel(&mut r, &device, &queue, [204, 255, 255, 255], night, true), [189.4, 204.8, 204.8], "the sheen, lit");
    // ...where without the emissive the night leaves it dark
    near(pixel(&mut r, &device, &queue, [0; 4], night, true), [37.9, 40.96, 40.96], "the sheen, unlit");
}
