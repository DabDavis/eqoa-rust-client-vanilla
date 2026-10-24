//! A layer pass given another's UV mapping (`Renderer::set_layer`'s `uv`):
//! `VIMaterial::CopyLayer` carries a layer's transform, wrap and scroll with
//! it, so while a change of sky fades the dome's second layer has its base's
//! mapping, not the clouds' it was made with (F3 20261024-065535: the fading
//! sky drawn at the clouds' three times, then jumping to the base's halfway).
//! Needs a GPU adapter (any backend); without one it says so and passes.

use render::{Draw, Renderer, View};

/// The colour drawn by a two-layer model whose second layer (alpha blended,
/// opaque) takes the UV mapping `uv` names: layer 0 maps u once, layer 1
/// three times, over a texture of four texels (red, green, blue, white)
/// sampled at u 0.375, texel 1's centre once and texel 0's (1.125, wrapped)
/// three times.
fn drawn(uv: Option<usize>) -> Option<[u8; 3]> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return None;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let rgba = vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
    let tex = |id: i32| match id {
        7 => Some(esf::Image { width: 4, height: 1, rgba: rgba.clone(), alpha: false }),
        8 => Some(esf::Image { width: 1, height: 1, rgba: vec![0, 0, 0, 255], alpha: false }),
        _ => None,
    };
    let layer = |texture, blend, scale: f32| esf::Layer {
        flags: 1,
        texture,
        wrap: 0,
        blend,
        color: [255; 4],
        uv_transform: [scale, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
        lod_bias: 0.8,
        uv_rate: [0.0; 2],
    };
    // one triangle over the whole view (the projection is the identity),
    // every corner at u 0.375
    let model = esf::Model {
        pos: vec![[-3.0, -3.0, 0.5], [3.0, -3.0, 0.5], [0.0, 3.0, 0.5]],
        uv: vec![[0.375, 0.5]; 3],
        normal: vec![[0.0, 0.0, 1.0]; 3],
        color: vec![[1.0, 1.0, 1.0, 1.0]; 3],
        batches: vec![esf::Batch { material: 0, layers: vec![layer(8, 0, 1.0), layer(7, 3, 3.0)], indices: vec![0, 1, 2], lod: None }],
        ..Default::default()
    };
    let mut m = r.upload(&device, &queue, &model, &[render::IDENTITY], &tex);
    r.set_layer(&device, &queue, &mut m, 0, 1, 7, 3, [1.0; 4], uv, &tex);
    let view = View { sun: glam::Vec3::ZERO, ambient: glam::Vec3::ZERO, ..View::daylight(glam::Mat4::IDENTITY) };
    let px = render::capture(&device, &queue, &mut r, 4, 4, &view, &[Draw::Model(&m)], None);
    let at = (2 * 4 + 2) * 4;
    Some([px[at], px[at + 1], px[at + 2]])
}

#[test]
fn a_layer_drawn_with_the_mapping_it_carries() {
    let Some(base) = drawn(Some(0)) else { return };
    assert_eq!(base, [0, 255, 0], "layer 0's mapping (once): texel 1");
    // the control: the pass's own mapping as made (three times), what the
    // dome drew before a layer carried its mapping
    assert_eq!(drawn(Some(1)).unwrap(), [255, 0, 0], "its own mapping (three times): texel 0");
    // a layer SetNumLayers made: identity, as once
    assert_eq!(drawn(None).unwrap(), [0, 255, 0], "a fresh layer's identity");
}
