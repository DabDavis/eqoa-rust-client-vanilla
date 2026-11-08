//! The options' Ambient Occlusion (`render::ao`) over a floor and a wall
//! placed where Freeport is (x 25,300, z 15,700), seen as the game's camera
//! sees: the floor at the wall's foot darkened, and every pixel of the open
//! floor and the open wall not (the control: a position taken in world
//! coordinates there steps 2 mm, and the floor reads as noise and the open
//! ground as filled; the same scene at the origin gives the same factor), the
//! view depth and the normals the pass reconstructs as they are, nothing
//! brightened, the alpha kept, the fog fading it, a blended model drawn over
//! it as it was (the control: at the darkened foot, a glow darkened with it
//! adds less), with Dynamic Lighting, the Brightness curve and Render Scale
//! x2, and off the frame as it was to the byte; at half the target's size
//! and, past 1,440 lines, a quarter. It runs only in the draw `prepare`
//! readied, and never into part of the target (`draw_viewport`). Needs a GPU
//! adapter (any backend, `WGPU_BACKEND`); without one it says so and passes.

use glam::{DVec4, Mat4, Vec3};
use render::ao::{Mode, Settings};
use render::{Draw, Lights, Renderer, View};

/// A flat quad of colour `grey` on both faces, corners in order round it,
/// drawn in blend mode `blend` (0 opaque, 1 added).
fn quad(corners: [Vec3; 4], normal: Vec3, grey: f32, blend: i32) -> esf::Model {
    let layer = esf::Layer { flags: 0, texture: 0, wrap: 0, blend, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
    esf::Model {
        pos: corners.map(|c| c.to_array()).to_vec(),
        uv: vec![[0.5, 0.5]; 4],
        normal: vec![normal.to_array(); 4],
        color: vec![[grey, grey, grey, 1.0]; 4],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2, 0, 2, 3, 0, 2, 1, 0, 3, 2], lod: None }],
        ..Default::default()
    }
}

/// Where a world point lands on a `w` by `h` target, and its view depth.
fn pixel(vp: Mat4, (w, h): (u32, u32), p: Vec3) -> ((u32, u32), f32) {
    let c = vp.as_dmat4() * DVec4::new(p.x as f64, p.y as f64, p.z as f64, 1.0);
    let (x, y) = (c.x / c.w, c.y / c.w);
    assert!(x.abs() < 1.0 && y.abs() < 1.0, "{p} is off the target");
    ((((x + 1.0) * 0.5 * w as f64) as u32, ((1.0 - y) * 0.5 * h as f64) as u32), c.w as f32)
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
}

fn gpu() -> Option<Gpu> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return None;
    };
    eprintln!("adapter: {:?} ({:?})", adapter.get_info().name, adapter.get_info().backend);
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    Some(Gpu { device, queue })
}

/// The floor (40 by 60) and the wall (20 wide, 12 high) at `(x, y, z)`, the
/// wall's foot along z, and the camera 20 in front of it and 6 above the
/// floor, through the game's projection (walker.rs).
struct Scene {
    origin: Vec3,
    models: Vec<render::GpuModel>,
    vp: Mat4,
    view: View,
    size: (u32, u32),
}

impl Scene {
    fn new(g: &Gpu, r: &mut Renderer, origin: Vec3, size: (u32, u32)) -> Scene {
        let Vec3 { x, y, z } = origin;
        let floor = quad(
            [Vec3::new(x - 20.0, y, z - 20.0), Vec3::new(x + 20.0, y, z - 20.0), Vec3::new(x + 20.0, y, z + 40.0), Vec3::new(x - 20.0, y, z + 40.0)],
            Vec3::Y,
            0.5,
            0,
        );
        let wall = quad([Vec3::new(x - 10.0, y, z), Vec3::new(x + 10.0, y, z), Vec3::new(x + 10.0, y + 12.0, z), Vec3::new(x - 10.0, y + 12.0, z)], Vec3::Z, 0.5, 0);
        let none = |_: i32| None;
        let models = [floor, wall].iter().map(|m| r.upload(&g.device, &g.queue, m, &[render::IDENTITY], &none)).collect();
        let eye = Vec3::new(x, y + 6.0, z + 20.0);
        let vp = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), size.0 as f32 / size.1 as f32, 0.3, 6000.0)
            * glam::camera::rh::view::look_at_mat4(eye, Vec3::new(x, y + 2.0, z), Vec3::Y);
        let view = View { eye, sun: Vec3::ZERO, ambient: Vec3::ZERO, ..View::daylight(vp) };
        Scene { origin, models, vp, view, size }
    }

    fn items(&self) -> Vec<Draw<'_>> {
        self.models.iter().map(Draw::Model).collect()
    }

    fn shot(&self, g: &Gpu, r: &mut Renderer, view: &View) -> Vec<u8> {
        render::capture(&g.device, &g.queue, r, self.size.0, self.size.1, view, &self.items(), None)
    }

    /// The pixel a point `d` from the scene's origin lands on.
    fn at(&self, d: Vec3) -> (u32, u32) {
        pixel(self.vp, self.size, self.origin + d).0
    }

    fn rgba(&self, px: &[u8], (x, y): (u32, u32)) -> [u8; 4] {
        let i = ((y * self.size.0 + x) * 4) as usize;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    }

    /// The open floor (5 to 10 in front of the wall, 5 or more from a side)
    /// and the open wall (5 to 9 up), every pixel they cover.
    fn open(&self) -> Vec<(u32, u32)> {
        let mut out = std::collections::BTreeSet::new();
        for k in 0..=40 {
            for side in -12..=12 {
                let across = side as f32 * 0.5;
                for d in [Vec3::new(across, 0.0, 5.0 + k as f32 * 0.125), Vec3::new(across, 5.0 + k as f32 * 0.1, 0.0)] {
                    let c = self.vp.as_dmat4() * (self.origin + d).as_dvec3().extend(1.0);
                    if (c.x / c.w).abs() < 0.98 && (c.y / c.w).abs() < 0.98 {
                        out.insert(self.at(d));
                    }
                }
            }
        }
        out.into_iter().collect()
    }
}

/// Freeport's streets: where the game is drawn far from the origin.
const FREEPORT: Vec3 = Vec3::new(25300.0, 54.0, 15700.0);

#[test]
fn the_foot_of_a_wall_darkened_and_open_ground_not() {
    let Some(g) = gpu() else { return };
    for size in [(160, 120), (120, 1500)] {
        foot_of_a_wall(&g, size);
    }
}

fn foot_of_a_wall(g: &Gpu, size: (u32, u32)) {
    let (w, h) = size;
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let s = Scene::new(g, &mut r, FREEPORT, size);
    let view = View { ..s.view };
    let plain = s.shot(g, &mut r, &view);
    let foot = s.at(Vec3::new(0.0, 0.0, 0.4));
    let w_foot = pixel(s.vp, size, FREEPORT + Vec3::new(0.0, 0.0, 0.4)).1;
    let (open, high) = (s.at(Vec3::new(0.0, 0.0, 6.0)), s.at(Vec3::new(0.0, 9.0, 0.0)));
    assert_eq!(s.rgba(&plain, foot)[..3], [128; 3], "grey where it was drawn");

    // the factor alone: dark at the foot, white in the open, every pixel of
    // the open floor and the open wall (no stripes, no noise)
    r.ao.set(Settings::with(Mode::Factor));
    let factor = s.shot(g, &mut r, &view);
    let (f_foot, f_open, f_high) = (s.rgba(&factor, foot)[0], s.rgba(&factor, open)[0], s.rgba(&factor, high)[0]);
    eprintln!("{w}x{h} factor: foot {f_foot}, open floor {f_open}, open wall {f_high}");
    assert!(f_foot < 200, "the floor at the wall's foot: {f_foot}");
    let open_all = s.open();
    assert!(open_all.len() > 20, "{} open pixels", open_all.len());
    let least = open_all.iter().map(|&p| (s.rgba(&factor, p)[0], p)).min().unwrap();
    assert!(least.0 >= 250, "the open floor and wall at {:?}: {}", least.1, least.0);

    // the view depth and the normals the pass works from
    r.ao.set(Settings::with(Mode::Depth));
    let depth = s.rgba(&s.shot(g, &mut r, &view), foot)[0] as f32;
    assert!((depth - w_foot / 100.0 * 255.0).abs() <= 2.0, "view depth {w_foot}: {depth}");
    r.ao.set(Settings::with(Mode::Normals));
    let normals = s.shot(g, &mut r, &view);
    let near = |a: [u8; 4], b: [u8; 3]| a.iter().zip(b).all(|(&a, b)| a.abs_diff(b) <= 3);
    assert!(near(s.rgba(&normals, open), [128, 255, 128]), "the floor faces up: {:?}", s.rgba(&normals, open));
    assert!(near(s.rgba(&normals, high), [128, 128, 255]), "the wall faces the camera: {:?}", s.rgba(&normals, high));

    // into the scene: darker at the foot, no pixel brighter, alpha kept
    r.ao.set(Settings::with(Mode::On));
    let on = s.shot(g, &mut r, &view);
    for (a, b) in plain.chunks(4).zip(on.chunks(4)) {
        assert!(b[..3].iter().zip(&a[..3]).all(|(b, a)| b <= a), "a pixel brightened: {a:?} to {b:?}");
        assert_eq!(b[3], a[3], "alpha");
    }
    assert!(s.rgba(&on, foot)[0] < 110, "the foot darkened: {:?}", s.rgba(&on, foot));
    assert!(s.rgba(&on, open)[0] >= 126, "the open floor: {:?}", s.rgba(&on, open));

    // a blended model (a glow, added) drawn after it, as it was: what it adds
    // is the same with it on as off, at the foot too (where a glow darkened
    // with the scene, the control, would add a quarter less)
    let none = |_: i32| None;
    let c = FREEPORT + Vec3::new(0.0, 0.0, 1.0);
    let glow = quad([c + Vec3::new(-3.0, -1.0, 0.0), c + Vec3::new(3.0, -1.0, 0.0), c + Vec3::new(3.0, 3.0, 0.0), c + Vec3::new(-3.0, 3.0, 0.0)], Vec3::Z, 0.2, 1);
    let glow = r.upload(&g.device, &g.queue, &glow, &[render::IDENTITY], &none);
    let mut with_glow = s.items();
    with_glow.push(Draw::Model(&glow));
    let glow_shot = |r: &mut Renderer| render::capture(&g.device, &g.queue, r, w, h, &view, &with_glow, None);
    let on_glow = glow_shot(&mut r);
    r.ao.set(Settings::OFF);
    let off_glow = glow_shot(&mut r);
    let added = |a: &[u8], b: &[u8], p| s.rgba(a, p)[0] as i32 - s.rgba(b, p)[0] as i32;
    let (off_added, on_added) = (added(&off_glow, &plain, foot), added(&on_glow, &on, foot));
    eprintln!("{w}x{h} the glow adds {off_added} off, {on_added} on, at the foot");
    assert!(off_added >= 40, "the glow over the foot: {off_added}");
    assert!((on_added - off_added).abs() <= 1, "the glow drawn after the occlusion: {on_added} on, {off_added} off");
    assert!(s.rgba(&on_glow, foot)[0] < s.rgba(&off_glow, foot)[0], "the floor under the glow still darkened");

    // with Dynamic Lighting (the lit pipelines): the foot darkened all the same
    r.set_dynamic_lighting(&g.device, true);
    let lit_view = View { lights: Some(Lights::default()), ..view };
    let lit_off = s.shot(g, &mut r, &lit_view);
    r.ao.set(Settings::with(Mode::On));
    let lit_on = s.shot(g, &mut r, &lit_view);
    assert!(s.rgba(&lit_on, foot)[0] + 10 < s.rgba(&lit_off, foot)[0], "lit: the foot {:?} from {:?}", s.rgba(&lit_on, foot), s.rgba(&lit_off, foot));
    assert!(s.rgba(&lit_on, open)[0] + 2 >= s.rgba(&lit_off, open)[0], "lit: the open floor");
    r.set_dynamic_lighting(&g.device, false);

    // through the Brightness curve's texture (a table that lifts every level)
    r.curve.set(Some(std::array::from_fn(|k| (k as u32 * 3 / 2).min(255) as u8)));
    let curved_on = s.shot(g, &mut r, &view);
    r.ao.set(Settings::OFF);
    let curved_off = s.shot(g, &mut r, &view);
    r.curve.set(None);
    assert!(s.rgba(&curved_on, foot)[0] + 10 < s.rgba(&curved_off, foot)[0], "through the curve: the foot");
    assert_eq!(s.rgba(&curved_on, open)[0].abs_diff(s.rgba(&curved_off, open)[0]), 0, "through the curve: the open floor");

    // Render Scale x2: the frame twice the size each way and its depth the
    // scale's, the small size the window's; the factor as at x1, near enough
    r.scale.set(2);
    r.ao.set(Settings::with(Mode::Factor));
    let factor2 = s.shot(g, &mut r, &view);
    let f2_foot = s.rgba(&factor2, foot)[0];
    eprintln!("{w}x{h} at x2: foot {f2_foot} (x1 {f_foot})");
    assert!(f2_foot < 200 && f2_foot.abs_diff(f_foot) <= 30, "x2: the foot {f2_foot}, x1 {f_foot}");
    let least2 = open_all.iter().map(|&p| (s.rgba(&factor2, p)[0], p)).min().unwrap();
    assert!(least2.0 >= 248, "x2: the open floor and wall at {:?}: {}", least2.1, least2.0);
    r.ao.set(Settings::OFF);
    let plain2 = s.shot(g, &mut r, &view);
    r.ao.set(Settings::with(Mode::On));
    let on2 = s.shot(g, &mut r, &view);
    assert!(s.rgba(&on2, foot)[0] + 10 < s.rgba(&plain2, foot)[0], "x2: the foot darkened");
    r.scale.set(1);

    // all fog: nothing left to darken, the frame as it was
    let fogged = View { fog: Some((0.5, 1.0, Vec3::new(0.2, 0.3, 0.4))), ..view };
    r.ao.set(Settings::OFF);
    let fog_plain = s.shot(g, &mut r, &fogged);
    r.ao.set(Settings::with(Mode::On));
    assert_eq!(s.shot(g, &mut r, &fogged), fog_plain, "the fog fades it");

    // a view with no inverse: nothing done
    let flat = View { view_proj: Mat4::ZERO, ..view };
    let flat_on = s.shot(g, &mut r, &flat);
    r.ao.set(Settings::OFF);
    assert_eq!(flat_on, s.shot(g, &mut r, &flat), "a camera with no inverse");

    // off: the frame drawn before any of it, to the byte, and a renderer
    // that was never told of it the same (and on again at a new size, the
    // passes' textures made again)
    assert_eq!(s.shot(g, &mut r, &view), plain, "off");
    let mut fresh = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let fresh_scene = Scene::new(g, &mut fresh, FREEPORT, size);
    assert_eq!(fresh_scene.shot(g, &mut fresh, &view), plain, "never on");
    r.ao.set(Settings::with(Mode::On));
    let wider = render::capture(&g.device, &g.queue, &mut r, w + 40, h, &view, &s.items(), None);
    assert_eq!(wider.len(), ((w + 40) * h * 4) as usize);
}

/// The same floor and wall at the origin and at Freeport give the same
/// factor (within the blur's rounding): the positions are the eye's, never
/// the world's. The control: the origin's at half the radius, which the
/// comparison tells apart.
#[test]
fn the_same_at_freeport_as_at_the_origin() {
    let Some(g) = gpu() else { return };
    let size = (160, 120);
    let factor = |origin: Vec3, settings: Settings| {
        let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
        let s = Scene::new(&g, &mut r, origin, size);
        r.ao.set(settings);
        s.shot(&g, &mut r, &s.view)
    };
    let worst = |a: &[u8], b: &[u8]| a.chunks(4).zip(b.chunks(4)).map(|(a, b)| a[0].abs_diff(b[0])).max().unwrap();
    let freeport = factor(FREEPORT, Settings::with(Mode::Factor));
    let origin = factor(Vec3::ZERO, Settings::with(Mode::Factor));
    let darkened = freeport.chunks(4).filter(|p| p[0] < 200).count();
    let (same, control) = (worst(&origin, &freeport), worst(&factor(Vec3::ZERO, Settings { radius: Settings::RADIUS / 2.0, ..Settings::with(Mode::Factor) }), &freeport));
    eprintln!("the origin's factor against Freeport's: {same} at most ({control} at half the radius); {darkened} pixels under 200");
    assert!(darkened > 10, "a crease to compare");
    assert!(same <= 6, "the factor differs by {same}");
    assert!(control > 20, "the control: half the radius differs by {control}");
}

/// The passes run only in the draw `prepare` readied, and never into part of
/// the target: a `draw` with no `prepare` before it, and a `draw_viewport`,
/// draw the frame as they would with it off.
#[test]
fn only_the_draw_after_prepare() {
    let Some(g) = gpu() else { return };
    let size = (160u32, 120u32);
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let s = Scene::new(&g, &mut r, FREEPORT, size);
    let foot = s.at(Vec3::new(0.0, 0.0, 0.4));
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum How {
        Prepared,
        Unprepared,
        Viewport,
    }
    let draw = |r: &mut Renderer, how: How| {
        let target = g.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: render::TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color = target.create_view(&Default::default());
        let depth = render::depth_view(&g.device, size.0, size.1);
        let f = render::scale::Targets { color: color.clone(), depth: depth.clone(), size, k: 1 };
        if how != How::Unprepared {
            r.ao.prepare(&g.device, &f);
        }
        let mut enc = g.device.create_command_encoder(&Default::default());
        if how == How::Viewport {
            // the viewport keeps what the target holds: cleared to the view's
            // colour first, as `draw` clears it
            let c = s.view.clear;
            drop(enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &color,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: c.x as f64, g: c.y as f64, b: c.z as f64, a: 1.0 }), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            }));
            r.draw_viewport(&g.queue, &mut enc, &color, &depth, &s.view, &s.items(), [0, 0, size.0, size.1]);
        } else {
            r.draw(&g.queue, &mut enc, &color, &depth, &s.view, &s.items());
        }
        let row = (size.0 * 4).div_ceil(256) * 256;
        let readback = g.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (row * size.1) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        enc.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(size.1) } },
            wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        );
        g.queue.submit([enc.finish()]);
        readback.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("map"));
        g.device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
        let data = readback.slice(..).get_mapped_range().expect("mapped");
        (0..size.1 as usize).flat_map(|y| data[y * row as usize..y * row as usize + size.0 as usize * 4].to_vec()).collect::<Vec<u8>>()
    };
    let plain = draw(&mut r, How::Unprepared);
    r.ao.set(Settings::with(Mode::On));
    let on = draw(&mut r, How::Prepared);
    assert!(s.rgba(&on, foot)[0] + 10 < s.rgba(&plain, foot)[0], "prepared: the foot darkened");
    assert_eq!(draw(&mut r, How::Unprepared), plain, "no prepare: none");
    assert_eq!(draw(&mut r, How::Viewport), plain, "into part of the target: none");
    assert_ne!(draw(&mut r, How::Prepared), plain, "and prepared again, on");
}

/// Its cost on the GPU (`-- --ignored --nocapture`): `draw` over a street of
/// boxes that fills the frame to 100, every pixel in the passes' reach,
/// timed by the GPU's own timestamps with it off and on, at 1920x1080,
/// 2560x1440 and 3840x2160, and a 1920x1080 window at Render Scale x2 and x4.
#[test]
#[ignore]
fn ambient_occlusion_cost() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else { return };
    let want = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
    if !adapter.features().contains(want) {
        eprintln!("SKIP: no timestamps inside encoders on {:?}", adapter.get_info().name);
        return;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_features: want, ..Default::default() })).unwrap();
    eprintln!("{:?} ({:?})", adapter.get_info().name, adapter.get_info().backend);
    let period = queue.get_timestamp_period() as f64;
    let none = |_: i32| None;
    // a floor and a street of boxes either side of the camera's way
    let mut boxes = vec![quad([Vec3::new(-60.0, 0.0, -200.0), Vec3::new(60.0, 0.0, -200.0), Vec3::new(60.0, 0.0, 20.0), Vec3::new(-60.0, 0.0, 20.0)], Vec3::Y, 0.5, 0)];
    for row in 0..30 {
        for side in [-1.0f32, 1.0] {
            let (x, z) = (side * (3.0 + (row % 3) as f32), -(row as f32) * 5.0);
            for (a, b, n) in [(Vec3::new(x - 1.0, 0.0, z + 1.0), Vec3::new(x + 1.0, 0.0, z + 1.0), Vec3::Z), (Vec3::new(x + 1.0, 0.0, z + 1.0), Vec3::new(x + 1.0, 0.0, z - 1.0), Vec3::X), (Vec3::new(x - 1.0, 0.0, z - 1.0), Vec3::new(x - 1.0, 0.0, z + 1.0), -Vec3::X)] {
                boxes.push(quad([a, b, b + Vec3::Y * 4.0, a + Vec3::Y * 4.0], n, 0.5, 0));
            }
        }
    }
    let eye = Vec3::new(0.0, 2.0, 10.0);
    for ((w, h), k) in [((1920, 1080), 1), ((2560, 1440), 1), ((3840, 2160), 1), ((1920, 1080), 2), ((1920, 1080), 4)] {
        let vp = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), w as f32 / h as f32, 0.3, 6000.0) * glam::camera::rh::view::look_at_mat4(eye, Vec3::new(0.0, 1.0, -40.0), Vec3::Y);
        let view = View { eye, ..View::daylight(vp) };
        let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
        let models: Vec<render::GpuModel> = boxes.iter().map(|m| r.upload(&device, &queue, m, &[render::IDENTITY], &none)).collect();
        let items: Vec<Draw> = models.iter().map(Draw::Model).collect();
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: render::TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let tv = target.create_view(&Default::default());
        let depth = render::depth_view(&device, w, h);
        r.scale.set(k);
        let frames = 60u32;
        let queries = device.create_query_set(&wgpu::QuerySetDescriptor { label: None, ty: wgpu::QueryType::Timestamp, count: frames * 2 });
        let resolve = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: frames as u64 * 16, usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let read = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: frames as u64 * 16, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let time = |r: &mut Renderer, mode: Mode| -> f64 {
            r.ao.set(Settings::with(mode));
            let mut samples = Vec::new();
            for round in 0..3 {
                let mut enc = device.create_command_encoder(&Default::default());
                for f in 0..frames {
                    let t = r.scale.targets(&device, &tv, &depth, (w, h));
                    r.ao.prepare(&device, &t);
                    enc.write_timestamp(&queries, f * 2);
                    r.draw(&queue, &mut enc, &t.color, &t.depth, &view, &items);
                    enc.write_timestamp(&queries, f * 2 + 1);
                    r.scale.resolve(&mut enc, &tv);
                }
                enc.resolve_query_set(&queries, 0..frames * 2, &resolve, 0);
                enc.copy_buffer_to_buffer(&resolve, 0, &read, 0, frames as u64 * 16);
                queue.submit([enc.finish()]);
                read.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("map"));
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                let ticks: Vec<u64> = bytemuck::cast_slice(&read.slice(..).get_mapped_range().expect("mapped")).to_vec();
                read.unmap();
                if round > 0 {
                    samples.extend(ticks.chunks(2).map(|t| (t[1] - t[0]) as f64 * period / 1e6));
                }
            }
            samples.sort_by(f64::total_cmp);
            samples[samples.len() / 2]
        };
        let (off, on) = (time(&mut r, Mode::Off), time(&mut r, Mode::On));
        eprintln!("{w}x{h} x{k} (frame {}x{}): draw {off:.3} ms off, {on:.3} ms on: +{:.3} ms", w * k, h * k, on - off);
    }
}
