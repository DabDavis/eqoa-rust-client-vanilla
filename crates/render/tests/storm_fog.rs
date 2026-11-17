//! A storm's fog (`View::storm_fog`): per fragment, by the distance from the
//! eye, so it is the same whichever way the camera faces.
//!
//! A floor 400 across under a camera 2 over it, the fog a heavy whiteout's
//! (near 1.6, far 20, white) over a black floor: each pixel is the fog's
//! share. The floor's corners are 280 off, two of them behind the camera.
//!
//! - Storm fog: every probe is 1 − clamp((far − d) / (far − near)) for its
//!   point's distance d from the eye, at four yaws alike.
//! - The control, the game's per-vertex fog: the same floor's pixels are
//!   whatever the corners' clamped values spread, and they change as the
//!   camera turns (the owner, 2026-11-16: "when players rotate camera the fog
//!   will not be seen so looks clear").
//!
//! Needs a graphics adapter (any backend); without one it says so and passes.

use glam::{Mat4, Vec3};
use render::{Draw, Renderer, View};

const SIZE: u32 = 64;
const HALF: f32 = 200.0;
const EYE_Y: f32 = 2.0;
const NEAR: f32 = 1.6;
const FAR: f32 = 20.0;
const FOV: f32 = 60.0;
/// How far the camera looks down.
const PITCH: f32 = 0.35;

fn floor() -> esf::Model {
    let layer = esf::Layer { flags: 0, texture: 0, wrap: 0, blend: 0, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
    let s = HALF;
    esf::Model {
        pos: vec![[-s, 0.0, -s], [-s, 0.0, s], [s, 0.0, s], [s, 0.0, -s]],
        uv: vec![[0.0, 0.0]; 4],
        normal: vec![[0.0, 1.0, 0.0]; 4],
        color: vec![[0.0, 0.0, 0.0, 1.0]; 4],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2, 0, 2, 3], lod: None }],
        ..Default::default()
    }
}

fn forward(yaw: f32) -> Vec3 {
    Vec3::new(yaw.sin() * PITCH.cos(), -PITCH.sin(), -yaw.cos() * PITCH.cos())
}

/// The camera at (0, EYE_Y, 0) facing `yaw`, the floor black and the fog
/// white; `storm` the fog's kind.
fn view(yaw: f32, storm: bool) -> (View, Mat4) {
    let proj = glam::camera::rh::proj::directx::perspective(FOV.to_radians(), 1.0, 0.1, 1000.0);
    let eye = Vec3::new(0.0, EYE_Y, 0.0);
    let v = glam::camera::rh::view::look_at_mat4(eye, eye + forward(yaw), Vec3::Y);
    let view = View {
        eye,
        sun: Vec3::ZERO,
        ambient: Vec3::ZERO,
        clear: Vec3::ZERO,
        fog: Some((NEAR, FAR, Vec3::ONE)),
        storm_fog: storm,
        ..View::daylight(proj * v)
    };
    (view, (proj * v).inverse())
}

/// Where pixel (px, py)'s centre meets the floor, from the eye.
fn on_floor(inv: Mat4, px: u32, py: u32) -> Vec3 {
    let nx = (px as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
    let ny = 1.0 - (py as f32 + 0.5) / SIZE as f32 * 2.0;
    let a = inv.project_point3(Vec3::new(nx, ny, 0.0));
    let b = inv.project_point3(Vec3::new(nx, ny, 1.0));
    let d = (b - a).normalize();
    let eye = Vec3::new(0.0, EYE_Y, 0.0);
    eye + d * (-EYE_Y / d.y)
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
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    Some(Gpu { device, queue })
}

fn red(px: &[u8], x: u32, y: u32) -> f32 {
    px[((y * SIZE + x) * 4) as usize] as f32 / 255.0
}

/// Pixels on the floor: the bottom rows, near the eye, across the width.
const PROBES: [(u32, u32); 6] = [(32, 63), (4, 63), (60, 63), (32, 52), (10, 48), (32, 40)];
const YAWS: [f32; 4] = [0.0, 0.8, 2.4, 3.9];

#[test]
fn a_storms_fog_is_the_same_whichever_way_the_camera_faces() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let none = |_: i32| None;
    let m = r.upload(&g.device, &g.queue, &floor(), &[render::IDENTITY], &none);
    let eye = Vec3::new(0.0, EYE_Y, 0.0);
    let mut storm_seen = Vec::new();
    let mut game_seen = Vec::new();
    for yaw in YAWS {
        let (v, inv) = view(yaw, true);
        let px = render::capture(&g.device, &g.queue, &mut r, SIZE, SIZE, &v, &[Draw::Model(&m)], None);
        let mut row = Vec::new();
        for (x, y) in PROBES {
            let d = (on_floor(inv, x, y) - eye).length();
            let want = 1.0 - ((FAR - d) / (FAR - NEAR)).clamp(0.0, 1.0);
            let got = red(&px, x, y);
            assert!((got - want).abs() <= 3.0 / 255.0, "yaw {yaw} ({x}, {y}), {d:.2} off: {got} want {want}");
            row.push(got);
        }
        storm_seen.push(row);
        let (v, _) = view(yaw, false);
        let px = render::capture(&g.device, &g.queue, &mut r, SIZE, SIZE, &v, &[Draw::Model(&m)], None);
        game_seen.push(PROBES.iter().map(|&(x, y)| red(&px, x, y)).collect::<Vec<_>>());
    }
    // the storm's: the same picture at every yaw
    for row in &storm_seen[1..] {
        for (a, b) in row.iter().zip(&storm_seen[0]) {
            assert!((a - b).abs() <= 3.0 / 255.0, "storm fog moved with the yaw: {storm_seen:?}");
        }
    }
    // the control: per vertex, the same floor's fog changes as the camera turns
    let spread = (0..PROBES.len())
        .map(|i| {
            let col: Vec<f32> = game_seen.iter().map(|r| r[i]).collect();
            col.iter().cloned().fold(f32::MIN, f32::max) - col.iter().cloned().fold(f32::MAX, f32::min)
        })
        .fold(0.0, f32::max);
    assert!(spread > 0.1, "the control: the per-vertex fog did not move with the yaw ({game_seen:?})");
}
