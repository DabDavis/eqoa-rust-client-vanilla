//! The options' Brightness (`render::curve`): with a table set, every pixel of
//! the scene comes out as the table's byte for its level, channel by channel,
//! on the GPU in hand; without one there is no pass, and the picture is the
//! one drawn. Needs a GPU adapter (any backend); without one it says so and
//! passes.

use render::{Draw, Renderer, View};

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return None;
    };
    Some(pollster::block_on(adapter.request_device(&Default::default())).unwrap())
}

/// Every level through a table that turns them all round, so no level
/// comes out as itself unless the lookup is wrong.
fn table() -> [u8; 256] {
    std::array::from_fn(|i| 255 - i as u8)
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

/// All 256 levels in each channel, through the table into the target: each
/// the table's byte, alpha kept.
#[test]
fn every_level_comes_out_as_the_tables_byte() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    assert!(r.curve.scene_target(&device, (16, 16)).is_none(), "no table, no pass");
    r.curve.set(Some(table()));
    let scene = r.curve.scene_target(&device, (16, 16)).expect("a table: the scene's texture");
    drop(scene);
    let px: Vec<u8> = (0..256u32).flat_map(|i| [i as u8, (255 - i) as u8, (i * 7 % 256) as u8, 255]).collect();
    queue.write_texture(
        r.curve.scene_texture().unwrap().as_image_copy(),
        &px,
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(16 * 4), rows_per_image: Some(16) },
        wgpu::Extent3d { width: 16, height: 16, depth_or_array_layers: 1 },
    );
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: 16, height: 16, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render::TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut enc = device.create_command_encoder(&Default::default());
    r.curve.apply(&queue, &mut enc, &target.create_view(&Default::default()));
    queue.submit([enc.finish()]);
    let out = read(&device, &queue, &target, (16, 16));
    let t = table();
    let want: Vec<u8> = px.chunks(4).flat_map(|p| [t[p[0] as usize], t[p[1] as usize], t[p[2] as usize], p[3]]).collect();
    assert_eq!(out, want);
}

/// Through `capture`, as `shot` draws: a lit triangle over the clear colour,
/// then the same with the table set, every pixel of it the table's byte of
/// the first picture's; and no table again is the first picture.
#[test]
fn a_shot_through_the_table() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let layer = esf::Layer { flags: 0, texture: 0, wrap: 0, blend: 0, color: [200, 120, 40, 255], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
    let model = esf::Model {
        pos: vec![[-0.5, -0.5, 0.5], [0.5, -0.5, 0.5], [0.0, 0.5, 0.5]],
        uv: vec![[0.5, 0.5]; 3],
        normal: vec![[0.0, 0.0, 1.0]; 3],
        color: vec![[0.7, 0.5, 0.3, 1.0]; 3],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2], lod: None }],
        ..Default::default()
    };
    let m = r.upload(&device, &queue, &model, &[render::IDENTITY], &|_| None);
    let view = View { clear: glam::Vec3::new(0.1, 0.2, 0.3), ..View::daylight(glam::Mat4::IDENTITY) };
    let shot = |r: &mut Renderer| render::capture(&device, &queue, r, 8, 8, &view, &[Draw::Model(&m)], None);
    let plain = shot(&mut r);
    assert!(plain.chunks(4).any(|p| p[..3] != plain[..3]), "the triangle drawn over the clear");
    r.curve.set(Some(table()));
    let curved = shot(&mut r);
    let t = table();
    let want: Vec<u8> = plain.chunks(4).flat_map(|p| [t[p[0] as usize], t[p[1] as usize], t[p[2] as usize], p[3]]).collect();
    assert_eq!(curved, want);
    r.curve.set(None);
    assert_eq!(shot(&mut r), plain, "no table: as drawn");
}
