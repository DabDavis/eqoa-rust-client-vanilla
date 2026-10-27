//! The options' Render Scale (`render::scale`): the frame drawn `k` times
//! larger each way and each `k` by `k` block averaged into a pixel, of the
//! stored bytes, rounded half up, on the GPU in hand. Through `capture`, as
//! `shot` draws, a frame at a scale is exactly the frame drawn at the larger
//! size and averaged on the CPU, the world, the name plates and the 2D layer
//! alike; at 1 there is no pass, and the frame is the one drawn. Needs a GPU
//! adapter (any backend); without one it says so and passes.

use render::text::Label;
use render::ui::{Quad, Ui};
use render::{Draw, Renderer, View};

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return None;
    };
    Some(pollster::block_on(adapter.request_device(&Default::default())).unwrap())
}

/// A texture's bytes, row after row.
fn read(device: &wgpu::Device, queue: &wgpu::Queue, t: &wgpu::Texture, (w, h): (u32, u32)) -> Vec<u8> {
    let row = (w * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        t.as_image_copy(),
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |r| r.unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = buf.slice(..).get_mapped_range().unwrap();
    (0..h as usize).flat_map(|y| data[y * row as usize..y * row as usize + w as usize * 4].to_vec()).collect()
}

/// `px` (`w` wide) with each `k` by `k` block averaged into a pixel:
/// `round` true, the sum plus half the count, divided (as the pass); false,
/// truncated (the control); alpha 255.
fn averaged(px: &[u8], (w, h): (u32, u32), k: u32, round: bool) -> Vec<u8> {
    let (ow, oh) = (w / k, h / k);
    let n = k * k;
    let mut out = Vec::with_capacity((ow * oh * 4) as usize);
    for y in 0..oh {
        for x in 0..ow {
            for c in 0..3 {
                let mut sum = 0u32;
                for dy in 0..k {
                    for dx in 0..k {
                        sum += px[(((y * k + dy) * w + x * k + dx) * 4 + c) as usize] as u32;
                    }
                }
                out.push(((sum + if round { n / 2 } else { 0 }) / n) as u8);
            }
            out.push(255);
        }
    }
    out
}

fn target(device: &wgpu::Device, (w, h): (u32, u32)) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render::TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Bytes off a fixed generator: every level, in no order a filter keeps.
fn noise(n: usize, seed: u32) -> Vec<u8> {
    let mut x = seed;
    (0..n)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 24) as u8
        })
        .collect()
}

/// Whatever the frame's texture holds, at 2, 3 and 4, comes out as its
/// blocks' averages, rounded half up, alpha 1; the control: truncated, they
/// do not. At 1 the target is the frame and nothing is averaged.
#[test]
fn every_block_comes_out_as_its_average_rounded_half_up() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let size = (24u32, 16u32);
    let t = target(&device, size);
    let tv = t.create_view(&Default::default());
    let depth = render::depth_view(&device, size.0, size.1);
    let f = r.scale.targets(&device, &tv, &depth, size);
    assert_eq!((f.k, f.size, r.scale.frame_texture().is_none()), (1, size, true), "1, the default: the target itself");
    for k in [2u32, 3, 4] {
        r.scale.set(k);
        let f = r.scale.targets(&device, &tv, &depth, size);
        assert_eq!((f.k, f.size, render::scale::would_draw(k)), (k, (size.0 * k, size.1 * k), k));
        let big = (size.0 * k, size.1 * k);
        let mut px = noise((big.0 * big.1 * 4) as usize, k);
        px.iter_mut().skip(3).step_by(4).for_each(|a| *a = 255);
        queue.write_texture(
            r.scale.frame_texture().unwrap().as_image_copy(),
            &px,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(big.0 * 4), rows_per_image: Some(big.1) },
            wgpu::Extent3d { width: big.0, height: big.1, depth_or_array_layers: 1 },
        );
        let mut enc = device.create_command_encoder(&Default::default());
        r.scale.resolve(&mut enc, &tv);
        queue.submit([enc.finish()]);
        let out = read(&device, &queue, &t, size);
        assert_eq!(out, averaged(&px, big, k, true), "x{k}");
        assert_ne!(out, averaged(&px, big, k, false), "x{k}, the control: truncated");
    }
    r.scale.set(1);
    let f = r.scale.targets(&device, &tv, &depth, size);
    assert_eq!((f.k, r.scale.frame_texture().is_none()), (1, true), "back to 1: the larger textures let go");
}

/// A lit triangle, a name plate on it, a label and a 2D quad half over it,
/// through `capture` at 640x480 (a font's texel about a pixel, as on the
/// PS2): at 2 and 4 each is the frame captured at 1280x960 and 2560x1920 and
/// averaged on the CPU, byte for byte; at 1 again the frame as drawn. The control: the frame at 1 is not the one at 2 (its
/// edges are smoothed).
#[test]
fn a_shot_at_a_scale_is_the_larger_shot_averaged() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let layer = esf::Layer { flags: 0, texture: 0, wrap: 0, blend: 0, color: [200, 120, 40, 255], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2] };
    let model = esf::Model {
        pos: vec![[-0.7, -0.6, 0.5], [0.55, -0.45, 0.5], [0.1, 0.65, 0.5]],
        uv: vec![[0.5, 0.5]; 3],
        normal: vec![[0.0, 0.0, 1.0]; 3],
        color: vec![[0.7, 0.5, 0.3, 1.0]; 3],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2], lod: None }],
        ..Default::default()
    };
    let m = r.upload(&device, &queue, &model, &[render::IDENTITY], &|_| None);
    // a font of one glyph, 'A', 7 by 9, its texels clear and white by turns
    let glyph: Vec<u8> = (0..7 * 9).flat_map(|i| if (i % 7 + i / 7) % 3 == 0 { [0, 0, 0, 0] } else { [255; 4] }).collect();
    r.set_font(&device, &queue, &esf::font::Font { id: 1, height: 9, glyphs: vec![esf::font::Glyph { code: b'A' as u16, width: 7, rgba: glyph }] });
    let t = r.text.as_mut().unwrap();
    t.plates = vec![Label { at: [-0.1, 0.05, 0.5], color: [0.2, 0.9, 0.4, 1.0], text: "AAA".into() }];
    t.labels = vec![Label { at: [-0.45, -0.6, 0.0], color: [1.0, 1.0, 0.6, 1.0], text: "AA".into() }];
    // a 2D quad over the triangle's right edge, its texture's texels of four
    // colours by turns, at a frame scale that is not whole
    let mut ui = Ui::new(&device, render::TARGET_FORMAT, render::DEPTH_FORMAT);
    let rgba: Vec<u8> = (0..16 * 16).flat_map(|i| [[255, 255, 255, 255], [0, 0, 0, 255], [255, 0, 0, 255], [0, 0, 255, 128]][(i % 16 / 2 + i / 32) % 4]).collect();
    let tex = ui.add_texture(&device, &queue, 16, 16, &rgba);
    ui.begin((640, 480));
    ui.push(Quad { tex, blend: 3, pos: [[331.3, 141.7], [517.9, 141.7], [517.9, 300.2], [331.3, 300.2]], uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], color: [1.0; 4], clip: None });
    r.ui = Some(ui);
    let view = View { clear: glam::Vec3::new(0.1, 0.2, 0.3), ..View::daylight(glam::Mat4::IDENTITY) };
    let shot = |r: &mut Renderer, k: u32, w: u32, h: u32| {
        r.scale.set(k);
        render::capture(&device, &queue, r, w, h, &view, &[Draw::Model(&m)], None)
    };
    let plain = shot(&mut r, 1, 640, 480);
    assert!(plain.chunks(4).any(|p| p[..3] != plain[..3]), "the triangle and the quad drawn over the clear");
    let shows = |px: &[u8], c: [u8; 3]| px.chunks(4).any(|p| (0..3).all(|i| (p[i] as i32 - c[i] as i32).abs() <= 2));
    assert!(shows(&plain, [51, 230, 102]) && shows(&plain, [255, 255, 153]), "the plate and the label drawn");
    for k in [2u32, 4] {
        let big = shot(&mut r, 1, 640 * k, 480 * k);
        let scaled = shot(&mut r, k, 640, 480);
        assert!(scaled == averaged(&big, (640 * k, 480 * k), k, true), "x{k}: the {}x{} frame averaged", 640 * k, 480 * k);
        assert!(scaled != plain, "x{k}, the control: the edges smoothed");
    }
    assert!(shot(&mut r, 1, 640, 480) == plain, "back at 1: as drawn");
}
