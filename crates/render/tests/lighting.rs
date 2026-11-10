//! Dynamic Lighting (`render::lit`): the strip programs' light per fragment,
//! and point lights past the personal one, on a floor seen from above.
//!
//! - A point light's pool lands between the floor's vertices: every corner is
//!   out of its range, so the PS2's way (per vertex) the floor is the ambient
//!   alone (the control), and lit each pixel takes `(1 − d/r) · N·L̂` at its
//!   own place, the personal light and a light of the list alike.
//! - The sun per fragment: two edges' normals tilted apart give 0.6 at each
//!   vertex, which the PS2's way spreads flat; lit, the normal between them is
//!   upright again and the sun lights it whole.
//! - A baked instance takes no fixed light (its colour buffers carry the
//!   scene's), and takes the others.
//! - A group draws lit once its runs are relit, and the PS2's way before.
//! - With Dynamic Lighting on, a view without lights draws byte for byte as a
//!   renderer that never turned it on.
//! Needs a graphics adapter (any backend); without one it says so and passes.

use glam::{Mat4, Vec3};
use render::{Draw, GroupPart, Lights, PointLight, Renderer, View};

const SIZE: u32 = 64;
/// The floor's half-width, and the camera's height over it: a 90° view fills
/// the picture with it.
const HALF: f32 = 20.0;

fn floor(normals: [[f32; 3]; 4], alpha: f32) -> esf::Model {
    let layer = esf::Layer { flags: 0, texture: 0, wrap: 0, blend: 0, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
    let s = HALF;
    // A (-s,-s) B (-s,s) C (s,s) D (s,-s) in (x, z), wound front-up
    esf::Model {
        pos: vec![[-s, 0.0, -s], [-s, 0.0, s], [s, 0.0, s], [s, 0.0, -s]],
        uv: vec![[0.0, 0.0]; 4],
        normal: normals.to_vec(),
        color: vec![[0.0, 0.0, 0.0, alpha]; 4],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2, 0, 2, 3], lod: None }],
        ..Default::default()
    }
}

const UP: [[f32; 3]; 4] = [[0.0, 1.0, 0.0]; 4];

/// The camera HALF over the floor's centre, looking down, +x to the right.
fn view(ambient: f32) -> View {
    let proj = glam::camera::rh::proj::directx::perspective(90f32.to_radians(), 1.0, 0.1, 1000.0);
    let eye = Vec3::new(0.0, HALF, 0.0);
    let v = glam::camera::rh::view::look_at_mat4(eye, Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
    View { eye, sun: Vec3::ZERO, ambient: Vec3::splat(ambient), sun_dir: Vec3::new(0.0, -1.0, 0.0), clear: Vec3::ZERO, ..View::daylight(proj * v) }
}

/// Where pixel (px, py)'s centre falls on the floor.
fn at(px: u32, py: u32) -> Vec3 {
    let nx = (px as f32 + 0.5) / SIZE as f32 * 2.0 - 1.0;
    let ny = 1.0 - (py as f32 + 0.5) / SIZE as f32 * 2.0;
    // screen right +x, screen up -z
    Vec3::new(nx * HALF, 0.0, -ny * HALF)
}

/// The strip programs' point term at `p` on the floor (normal up).
fn point_term(p: Vec3, light: Vec3, range: f32) -> f32 {
    let l = light - p;
    let d = l.length();
    if d >= range { 0.0 } else { (1.0 - d / range) * (l.y / d).max(0.0) }
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

fn shot(g: &Gpu, r: &mut Renderer, view: &View, draws: &[Draw]) -> Vec<u8> {
    render::capture(&g.device, &g.queue, r, SIZE, SIZE, view, draws, None)
}

fn red(px: &[u8], x: u32, y: u32) -> f32 {
    px[((y * SIZE + x) * 4) as usize] as f32 / 255.0
}

fn close(got: f32, want: f32) -> bool {
    (got - want).abs() <= 2.5 / 255.0
}

/// The pixels the test reads: the centre, two off it, and one far out.
const PROBES: [(u32, u32); 4] = [(32, 32), (41, 32), (32, 44), (60, 4)];

#[test]
fn a_light_pool_lands_between_the_vertices() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    r.set_dynamic_lighting(&g.device, true);
    let none = |_: i32| None;
    let m = r.upload(&g.device, &g.queue, &floor(UP, 1.0), &[render::IDENTITY], &none);
    let lamp = Vec3::new(0.0, 2.0, 0.0);
    let range = 10.0;
    let red_light = Vec3::new(1.0, 0.0, 0.0);
    // the PS2's way: the personal light per vertex, every corner out of reach
    let ps2 = View { point: Some((lamp, range, red_light)), ..view(0.1) };
    let px = shot(&g, &mut r, &ps2, &[Draw::Model(&m)]);
    for (x, y) in PROBES {
        assert!(close(red(&px, x, y), 0.1), "the PS2's way at ({x}, {y}): {} (the ambient alone)", red(&px, x, y));
    }
    // lit: the personal light per fragment, then the same light from the list
    let personal = View { lights: Some(Lights::default()), ..ps2 };
    let listed = View { point: None, lights: Some(Lights::new(&[PointLight { at: lamp, range, color: red_light, fixed: true, key: 0 }])), ..ps2 };
    for (what, v) in [("the personal light", &personal), ("a listed light", &listed)] {
        let px = shot(&g, &mut r, v, &[Draw::Model(&m)]);
        for (x, y) in PROBES {
            let want = (0.1 + point_term(at(x, y), lamp, range)).min(1.0);
            assert!(close(red(&px, x, y), want), "{what} at ({x}, {y}): {} want {want}", red(&px, x, y));
        }
        assert!(red(&px, 32, 32) > 0.8, "{what}: the pool under the light");
        let g_ = px[((32 * SIZE + 32) * 4 + 1) as usize];
        assert!(g_ <= 26, "{what}: its colour, red ({g_} green)");
    }
}

/// The screen's tiles (`lit::tile_masks`) cull exactly: a light off to one
/// side, its pool across several tiles, matches the strip programs' term at
/// every pixel of the floor, and a light behind the camera lights none; the
/// control, a light's tiles flipped top to bottom, would leave its pool's
/// pixels unlit (`tile_masks` of a mirrored view names other tiles).
#[test]
fn the_tiles_cull_no_light_a_pixel_takes() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    r.set_dynamic_lighting(&g.device, true);
    let none = |_: i32| None;
    let m = r.upload(&g.device, &g.queue, &floor(UP, 1.0), &[render::IDENTITY], &none);
    let lamp = Vec3::new(-9.0, 1.5, 6.0);
    let behind = Vec3::new(0.0, 25.0, 0.0);
    let v = View { lights: Some(Lights::new(&[PointLight { at: lamp, range: 8.0, color: Vec3::X, fixed: false, key: 0 }, PointLight { at: behind, range: 4.0, color: Vec3::Y, fixed: false, key: 0 }])), ..view(0.1) };
    let px = shot(&g, &mut r, &v, &[Draw::Model(&m)]);
    let (mut lit, mut worst) = (0, 0.0f32);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let want = 0.1 + point_term(at(x, y), lamp, 8.0);
            worst = worst.max((red(&px, x, y) - want).abs());
            lit += (want > 0.11) as u32;
            let green = px[((y * SIZE + x) * 4 + 1) as usize] as f32 / 255.0;
            assert!(close(green, 0.1), "the light behind the camera at ({x}, {y}): {green}");
        }
    }
    assert!(worst <= 2.5 / 255.0, "a pixel off the strip programs' term by {worst}");
    assert!(lit > 100, "the pool covers {lit} pixels");
    // its tiles: on the left of the screen, its middle's among them, and
    // not most of the screen (the box is larger than the sphere)
    let masks = render::lit::tile_masks(v.lights.unwrap().as_slice(), v.view_proj);
    let (tx, ty) = render::lit::TILES;
    let tiles: Vec<(usize, usize)> = (0..tx * ty).filter(|&t| masks[t] & 1 != 0).map(|t| (t % tx, t / tx)).collect();
    assert!(!tiles.is_empty() && tiles.len() < tx * ty / 2, "{} tiles", tiles.len());
    assert!(tiles.iter().all(|&(x, _)| x < tx / 2), "the light's tiles on the left: {tiles:?}");
    let c = v.view_proj.project_point3(lamp);
    let own = ((((c.x * 0.5 + 0.5) * tx as f32) as usize), (((c.y * 0.5 + 0.5) * ty as f32) as usize));
    assert!(tiles.contains(&own), "its middle's tile {own:?}");
    assert!(masks.iter().all(|m| m & 2 == 0), "the light behind the camera, out of the view: no tile");
    // the control: the mask of the view mirrored top to bottom leaves out
    // tiles where the pool is, which the pixels above would have shown
    let flip = Mat4::from_scale(Vec3::new(1.0, -1.0, 1.0)) * v.view_proj;
    let flipped = render::lit::tile_masks(v.lights.unwrap().as_slice(), flip);
    let tile_of = |x: u32, y: u32| {
        let (nx, ny) = ((x as f32 + 0.5) / SIZE as f32, 1.0 - (y as f32 + 0.5) / SIZE as f32);
        ((ny * ty as f32) as usize).min(ty - 1) * tx + ((nx * tx as f32) as usize).min(tx - 1)
    };
    let missed = (0..SIZE * SIZE).filter(|&k| 0.1 + point_term(at(k % SIZE, k / SIZE), lamp, 8.0) > 0.11 && flipped[tile_of(k % SIZE, k / SIZE)] & 1 == 0).count();
    assert!(missed > 50, "the control: a flipped mask misses only {missed} of the pool's pixels");
    assert!((0..SIZE * SIZE).all(|k| 0.1 + point_term(at(k % SIZE, k / SIZE), lamp, 8.0) <= 0.1 || masks[tile_of(k % SIZE, k / SIZE)] & 1 != 0), "a pool pixel outside the light's tiles");
}

#[test]
fn the_sun_lights_each_fragment() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    r.set_dynamic_lighting(&g.device, true);
    let none = |_: i32| None;
    // the left edge's normals tilted left, the right edge's right: 0.6 up
    let (l, rt) = ([-0.8, 0.6, 0.0], [0.8, 0.6, 0.0]);
    let m = r.upload(&g.device, &g.queue, &floor([l, l, rt, rt], 1.0), &[render::IDENTITY], &none);
    let sun = View { sun: Vec3::ONE, ..view(0.0) };
    let px = shot(&g, &mut r, &sun, &[Draw::Model(&m)]);
    assert!(close(red(&px, 32, 32), 0.6), "the PS2's way: 0.6 at every vertex, so between: {}", red(&px, 32, 32));
    let lit = View { lights: Some(Lights::default()), ..sun };
    let px = shot(&g, &mut r, &lit, &[Draw::Model(&m)]);
    assert!(close(red(&px, 32, 32), 1.0), "lit: the normal upright at the middle: {}", red(&px, 32, 32));
    // the sun's visibility (a baked vertex alpha) still scales it
    let half = r.upload(&g.device, &g.queue, &floor([l, l, rt, rt], 0.5), &[render::IDENTITY], &none);
    let px = shot(&g, &mut r, &lit, &[Draw::Model(&half)]);
    assert!(close(red(&px, 32, 32), 0.5), "at half the sun's visibility: {}", red(&px, 32, 32));
}

#[test]
fn a_baked_instance_takes_no_fixed_light() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    r.set_dynamic_lighting(&g.device, true);
    let none = |_: i32| None;
    let mut m = r.upload(&g.device, &g.queue, &floor(UP, 1.0), &[render::IDENTITY], &none);
    m.baked = true;
    let lamp = Vec3::new(0.0, 2.0, 0.0);
    let light = |fixed| View { lights: Some(Lights::new(&[PointLight { at: lamp, range: 10.0, color: Vec3::X, fixed, key: 0 }])), ..view(0.1) };
    let px = shot(&g, &mut r, &light(true), &[Draw::Model(&m)]);
    assert!(close(red(&px, 32, 32), 0.1), "a fixed light on a baked instance: {}", red(&px, 32, 32));
    let pool = 0.1 + point_term(at(32, 32), lamp, 10.0);
    let px = shot(&g, &mut r, &light(false), &[Draw::Model(&m)]);
    assert!(close(red(&px, 32, 32), pool), "a light of the moment (a spell's) on it: {} want {pool}", red(&px, 32, 32));
    // the personal light lights it, as the console's one point light lights
    // baked geometry
    let personal = View { point: Some((lamp, 10.0, Vec3::X)), lights: Some(Lights::default()), ..view(0.1) };
    let px = shot(&g, &mut r, &personal, &[Draw::Model(&m)]);
    assert!(close(red(&px, 32, 32), pool), "the personal light on a baked instance: {}", red(&px, 32, 32));
    m.baked = false;
    let px = shot(&g, &mut r, &light(true), &[Draw::Model(&m)]);
    assert!(close(red(&px, 32, 32), pool), "the control: unbaked, the fixed light: {}", red(&px, 32, 32));
}

#[test]
fn a_group_draws_lit_once_relit() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let none = |_: i32| None;
    let model = floor(UP, 1.0);
    let part = |baked| GroupPart { model: &model, instances: &[render::IDENTITY], statics: &[], moves: false, baked };
    // uploaded while Dynamic Lighting is off: the PS2's bundles alone
    let mut plain = r.upload_group(&g.device, &g.queue, &[part(false)], &none);
    r.set_dynamic_lighting(&g.device, true);
    let lit = View { lights: Some(Lights::new(&[PointLight { at: Vec3::new(0.0, 2.0, 0.0), range: 10.0, color: Vec3::X, fixed: true, key: 0 }])), ..view(0.1) };
    let px = shot(&g, &mut r, &lit, &[Draw::Group(&plain)]);
    assert!(close(red(&px, 32, 32), 0.1), "before relight: the PS2's way: {}", red(&px, 32, 32));
    r.relight(&g.device, &mut plain);
    let pool = 0.1 + point_term(at(32, 32), Vec3::new(0.0, 2.0, 0.0), 10.0);
    let px = shot(&g, &mut r, &lit, &[Draw::Group(&plain)]);
    assert!(close(red(&px, 32, 32), pool), "relit: {} want {pool}", red(&px, 32, 32));
    // uploaded while on: lit at once; a baked part not by the fixed light
    let baked = r.upload_group(&g.device, &g.queue, &[part(true)], &none);
    let px = shot(&g, &mut r, &lit, &[Draw::Group(&baked)]);
    assert!(close(red(&px, 32, 32), 0.1), "a baked part: {}", red(&px, 32, 32));
    // and with no lights in the view, the PS2's bundles still
    let ps2 = View { lights: None, ..lit };
    let px = shot(&g, &mut r, &ps2, &[Draw::Group(&plain)]);
    assert!(close(red(&px, 32, 32), 0.1), "a view without lights: {}", red(&px, 32, 32));
}

#[test]
fn a_view_without_lights_draws_as_before() {
    let Some(g) = gpu() else { return };
    let none = |_: i32| None;
    let (l, rt) = ([-0.8, 0.6, 0.0], [0.8, 0.6, 0.0]);
    let model = floor([l, l, rt, rt], 0.75);
    let v = View { sun: Vec3::new(0.9, 0.7, 0.5), point: Some((Vec3::new(3.0, 2.0, -4.0), 30.0, Vec3::new(0.2, 0.5, 0.9))), ..view(0.15) };
    let draw = |r: &mut Renderer| {
        let m = r.upload(&g.device, &g.queue, &model, &[render::IDENTITY], &none);
        let grp = r.upload_group(&g.device, &g.queue, &[GroupPart { model: &model, instances: &[Mat4::from_translation(Vec3::new(0.0, 0.5, 0.0)).to_cols_array_2d()], statics: &[], moves: false, baked: false }], &none);
        shot(&g, r, &v, &[Draw::Model(&m), Draw::Group(&grp)])
    };
    let mut never = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let before = draw(&mut never);
    let mut on = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    on.set_dynamic_lighting(&g.device, true);
    let after = draw(&mut on);
    assert!(before == after, "Dynamic Lighting on changed a view without lights");
    // the control: the same view with lights is another picture
    let lit = View { lights: Some(Lights::default()), ..v };
    let m = on.upload(&g.device, &g.queue, &model, &[render::IDENTITY], &none);
    let px = shot(&g, &mut on, &lit, &[Draw::Model(&m)]);
    assert!(px != before, "the control: lit is the same picture");
}

/// A radial flora plant lit (`VIRStrip6`): no normal, so a listed light is
/// its falloff times 0.15 at the distance in the plant's own space, as
/// vs_main lights it with the personal light; the control, the world
/// distance's falloff, is another colour.
#[test]
fn a_plant_takes_its_lights_as_strip6() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    r.set_dynamic_lighting(&g.device, true);
    let none = |_: i32| None;
    let layer = esf::Layer { flags: 0, texture: 0, wrap: 0, blend: 0, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
    let s = 1.5;
    let quad = esf::Model {
        pos: vec![[-s, -s, 0.0], [s, -s, 0.0], [s, s, 0.0], [-s, -s, 0.0], [s, s, 0.0], [-s, s, 0.0]],
        uv: vec![[0.0, 0.0]; 6],
        normal: vec![[0.0, 1.0, 0.0]; 6],
        color: vec![[1.0, 1.0, 1.0, 1.0]; 6],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2, 3, 4, 5], lod: None }],
        ..Default::default()
    };
    let plant = r.upload_flora(&g.device, &g.queue, &quad, 2, &none);
    r.set_instances(&g.queue, &plant, &[Mat4::from_scale_rotation_translation(Vec3::splat(2.0), glam::Quat::IDENTITY, Vec3::new(0.0, 0.0, -1.0)).to_cols_array_2d()]);
    r.set_static_lights(&g.queue, &plant, &[[0.0, 0.0, 0.0, 1.0]]);
    let proj = glam::camera::rh::proj::directx::perspective(90f32.to_radians(), 1.0, 0.1, 1000.0);
    let lamp = PointLight { at: Vec3::new(0.0, 0.0, 9.0), range: 20.0, color: Vec3::ONE, fixed: true, key: 0 };
    let v = View { sun: Vec3::new(1.0, 0.5, 0.25), ambient: Vec3::new(0.1, 0.2, 0.3), sun_dir: Vec3::new(0.0, 0.0, 1.0), clear: Vec3::ZERO, lights: Some(Lights::new(&[lamp])), ..View::daylight(proj) };
    let px = render::capture(&g.device, &g.queue, &mut r, 4, 4, &v, &[Draw::Instances(&plant, 0..1)], None);
    let got = [0, 1, 2].map(|c| px[(2 * 4 + 2) * 4 + c] as f32 / 255.0);
    let base = [0.1 + 0.15, 0.2 + 0.075, 0.3 + 0.0375];
    let own = 0.15 * (1.0 - 5.0 / 20.0);
    let world = 0.15 * (1.0 - 10.0 / 20.0);
    let near = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 3.0 / 255.0);
    assert!(near(got, base.map(|c| c + own)), "the listed light's falloff in the plant's space: {got:?}");
    assert!(!near(got, base.map(|c| c + world)), "the control: the world distance's");
}

#[test]
fn a_frame_takes_sixteen_lights() {
    let many: Vec<PointLight> = (0..20).map(|k| PointLight { at: Vec3::splat(k as f32), range: 1.0, color: Vec3::ONE, fixed: false, key: 0 }).collect();
    let l = Lights::new(&many);
    assert_eq!(l.len(), render::MAX_LIGHTS);
    assert_eq!(l.as_slice(), &many[..16], "the first sixteen, in order");
}
