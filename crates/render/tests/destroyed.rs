//! GPU memory let go when it is done with, not left to a garbage collector
//! (petition 2097, the web page dropping players: wgpu's WebGPU backend
//! destroys nothing on a drop, and the map's background drawing alone made
//! tens of thousands of buffers a minute, every one waiting for the browser
//! to collect it). A model destroyed after the frame that drew it is
//! submitted draws that frame as ever and frees its buffers at once; the
//! 2D layer's per-frame vertices are destroyed `RETIRE_AFTER` draws on,
//! frame after frame, with no error. The controls: a destroyed model drawn
//! again is refused (so `destroy` really destroys), a retired buffer past
//! the bound is refused and one inside it is not. Needs a GPU adapter (any backend); without one it says
//! so and passes.

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

fn model() -> esf::Model {
    let layer = esf::Layer { flags: 0, texture: 7, wrap: 0, blend: 0, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
    esf::Model {
        pos: vec![[-3.0, -3.0, 0.5], [3.0, -3.0, 0.5], [0.0, 3.0, 0.5]],
        uv: vec![[0.5, 0.5]; 3],
        normal: vec![[0.0, 0.0, 1.0]; 3],
        color: vec![[0.0, 0.0, 0.0, 1.0]; 3],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2], lod: None }],
        ..Default::default()
    }
}

/// What `f` raised as a validation error, if anything.
fn refused(device: &wgpu::Device, f: impl FnOnce()) -> Option<String> {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    f();
    pollster::block_on(scope.pop()).map(|e| e.to_string())
}

fn buffers(device: &wgpu::Device) -> isize {
    device.get_internal_counters().hal.buffers.read()
}

#[test]
fn a_model_destroyed_after_its_frame_draws_it_and_frees_its_buffers() {
    let Some((device, queue)) = gpu() else { return };
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    let tex = |id: i32| (id == 7).then(|| esf::Image { width: 1, height: 1, rgba: vec![200, 100, 50, 255], alpha: false });
    let view = View { sun: glam::Vec3::ZERO, ambient: glam::Vec3::ONE, clear: glam::Vec3::ZERO, ..View::daylight(glam::Mat4::IDENTITY) };
    let centre = |px: &[u8]| px[(2 * 4 + 2) * 4..(2 * 4 + 2) * 4 + 3].to_vec();

    let kept = r.upload(&device, &queue, &model(), &[render::IDENTITY], &tex);
    let want = centre(&render::capture(&device, &queue, &mut r, 4, 4, &view, &[Draw::Model(&kept)], None));
    assert_ne!(want, vec![0, 0, 0], "the control: the triangle is drawn");

    let before = buffers(&device);
    let m = r.upload(&device, &queue, &model(), &[render::IDENTITY], &tex);
    let made = buffers(&device) - before;
    assert!(made >= 5, "a model's buffers: vertices, palette, instances, statics, indices, a layer's uniform ({made})");
    let mut got = Vec::new();
    let err = refused(&device, || {
        got = centre(&render::capture(&device, &queue, &mut r, 4, 4, &view, &[Draw::Model(&m)], None));
        m.destroy();
        r.forget_textures();
    });
    assert_eq!(err, None, "destroyed after its frame was submitted: no error");
    assert_eq!(got, want, "the frame drawn before the destroy is the model's");
    let _ = device.poll(wgpu::PollType::wait_indefinitely());
    assert!(buffers(&device) <= before, "its buffers gone at once ({} left of {made})", buffers(&device) - before);

    // the control: the destroyed model drawn again is refused
    let err = refused(&device, || {
        let _ = render::capture(&device, &queue, &mut r, 4, 4, &view, &[Draw::Model(&m)], None);
    });
    assert!(err.is_some(), "a destroyed model drawn again is a validation error");
}

#[test]
fn the_2d_layers_vertices_are_destroyed_a_few_frames_on() {
    let Some((device, queue)) = gpu() else { return };
    let size = (64u32, 64u32);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render::TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth = render::depth_view(&device, size.0, size.1);
    let tv = target.create_view(&Default::default());
    let mut ui = Ui::new(&device, render::TARGET_FORMAT, render::DEPTH_FORMAT);
    let tex = ui.add_texture(&device, &queue, 1, 1, &[255, 255, 255, 255]);
    ui.quads.push(Quad { tex, blend: 0, pos: [[10.0, 10.0], [50.0, 10.0], [50.0, 50.0], [10.0, 50.0]], uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], color: [1.0; 4], clip: None });
    let frame = |ui: &Ui| {
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &tv, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // two draws a frame, as the viewer's around a window's 3D
            ui.draw(&device, &mut pass, size);
            ui.draw(&device, &mut pass, size);
        }
        queue.submit([enc.finish()]);
    };
    let err = refused(&device, || {
        for _ in 0..200 {
            frame(&ui);
        }
    });
    assert_eq!(err, None, "no retired buffer was destroyed while a frame still drew it");
}

/// A buffer `Retire` keeps lives `RETIRE_AFTER` draws, then is destroyed
/// (natively a dropped buffer is freed too, so it is seen directly: written
/// to, a destroyed one is refused).
#[test]
fn retired_buffers_are_destroyed_after_retire_after_draws() {
    let Some((device, queue)) = gpu() else { return };
    let retire = render::Retire::default();
    let made: Vec<wgpu::Buffer> = (0..render::RETIRE_AFTER + 8)
        .map(|_| device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 16, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }))
        .collect();
    for b in &made {
        retire.keep(b.clone());
    }
    assert_eq!(retire.len(), render::RETIRE_AFTER);
    let write = |b: &wgpu::Buffer| {
        refused(&device, || {
            queue.write_buffer(b, 0, &[0; 16]);
            queue.submit([]);
        })
    };
    assert!(write(&made[0]).is_some(), "the oldest, past RETIRE_AFTER, is destroyed");
    assert!(write(&made[7]).is_some(), "the eighth too");
    assert_eq!(write(&made[8]), None, "the control: one still kept is not");
    assert_eq!(write(made.last().unwrap()), None, "nor the newest");
}

#[test]
fn a_mapping_too_short_or_detached_is_not_written() {
    assert!(render::mapping_holds(16, 16));
    assert!(!render::mapping_holds(0, 16), "a detached mapping has no bytes");
    assert!(!render::mapping_holds(12, 16));
}
