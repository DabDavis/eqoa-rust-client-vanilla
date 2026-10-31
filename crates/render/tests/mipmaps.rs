//! A texel's mip level as the GS picks it (`render::gs_lod_k`, TEX1 of
//! `VIEESurfCache::Set`): log2(w) + K, w the depth before the camera, K the
//! layer's bias as sixteenths of a level, blended between levels. A texture
//! whose three levels are red, green and blue, drawn square on at depths
//! where the rule picks each: the game's 0.8 keeps the base to about 80
//! units, level 1 at 2^7.3125 (159), level 2 at 2^8.3125 and past. The
//! controls: with the options' Mipmaps off every depth is the base; a bias
//! of 0 takes level 1 at 2 units; and a bias of 1.5 wraps round to a K of
//! +4.125, as the GS's char does, so even near it is the last level.
//! Needs a graphics adapter (any backend); without one it says so and passes.

use render::{Draw, Renderer, TextureSource, View};

/// Texture 7: three levels, 64 red, 32 green, 16 blue.
struct Levels;

impl TextureSource for Levels {
    fn image(&self, id: i32) -> Option<esf::Image> {
        self.levels(id).into_iter().next()
    }
    fn levels(&self, id: i32) -> Vec<esf::Image> {
        if id != 7 {
            return Vec::new();
        }
        [(64u32, [255u8, 0, 0, 255]), (32, [0, 255, 0, 255]), (16, [0, 0, 255, 255])]
            .iter()
            .map(|&(side, c)| esf::Image { width: side, height: side, rgba: c.repeat((side * side) as usize), alpha: false })
            .collect()
    }
}

/// The centre pixel of a square on at depth `d` (a view 90 degrees wide,
/// the square filling it), its layer's bias `bias`.
fn at(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer, d: f32, bias: f32) -> [u8; 3] {
    let layer = esf::Layer { flags: 0, texture: 7, wrap: 0, blend: 0, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: bias, uv_rate: [0.0; 2], emissive: [0; 4] };
    let s = d * 2.0;
    let model = esf::Model {
        pos: vec![[-s, -s, -d], [s, -s, -d], [s, s, -d], [-s, -s, -d], [s, s, -d], [-s, s, -d]],
        uv: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        normal: vec![[0.0, 0.0, 1.0]; 6],
        color: vec![[1.0, 1.0, 1.0, 1.0]; 6],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2, 3, 4, 5], lod: None }],
        ..Default::default()
    };
    let m = r.upload(device, queue, &model, &[render::IDENTITY], &Levels);
    let proj = glam::camera::rh::proj::directx::perspective(90f32.to_radians(), 1.0, 0.5, 100_000.0);
    let view = View { sun: glam::Vec3::ZERO, ambient: glam::Vec3::ONE, ..View::daylight(proj) };
    let px = render::capture(device, queue, r, 4, 4, &view, &[Draw::Model(&m)], None);
    let i = (2 * 4 + 2) * 4;
    [px[i], px[i + 1], px[i + 2]]
}

#[test]
fn a_texel_comes_from_the_level_the_gs_picks_by_depth() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    assert_eq!((render::gs_lod_k(0.8), render::gs_lod_k(1.0), render::gs_lod_k(0.0), render::gs_lod_k(1.5)), (-6.3125, -7.9375, 0.0, 4.125));
    let near = |c: [u8; 3], want: [u8; 3]| c.iter().zip(want).all(|(a, b)| (*a as i32 - b as i32).abs() <= 3);
    let one = 2f32.powf(1.0 - render::gs_lod_k(0.8));
    let cases = [
        (40.0, 0.8, [255, 0, 0], "the base within 80 units"),
        (one, 0.8, [0, 255, 0], "level 1 at 2^7.3125"),
        (one * 2.0, 0.8, [0, 0, 255], "level 2 at 2^8.3125"),
        (one * 16.0, 0.8, [0, 0, 255], "the last level past it"),
        (one / 2f32.sqrt(), 0.8, [128, 128, 0], "half way between the base and level 1"),
        (2.0, 0.0, [0, 255, 0], "a bias of 0: level 1 at 2 units"),
        (40.0, 1.5, [0, 0, 255], "a bias of 1.5 wraps to K +4.125: the last level"),
    ];
    for (d, bias, want, what) in cases {
        let got = at(&device, &queue, &mut r, d, bias);
        assert!(near(got, want), "{what} (depth {d}, bias {bias}): {got:?}, want {want:?}");
    }
    // the control: Mipmaps off, the base at every depth
    render::set_mipmaps(false);
    for (d, bias, _, what) in cases {
        let got = at(&device, &queue, &mut r, d, bias);
        render::set_mipmaps(true);
        assert!(near(got, [255, 0, 0]), "Mipmaps off, {what} (depth {d}): {got:?}, want the base");
        render::set_mipmaps(false);
    }
    render::set_mipmaps(true);
}
