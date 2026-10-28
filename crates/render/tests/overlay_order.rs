//! `VIScene::Render`'s order over the world: the particle system
//! (`VIParticleSystem::Render`), the shadows (`RenderShadows`), the spell
//! effects' lightning (`VISpellEffectSystem::Process`), the weapon trails
//! (`VITrailFxRender`), then the weather (`RenderWeather`). Over one pixel an
//! opaque particle (red), an opaque strip (green) and an opaque weather
//! billboard (blue): the strip is drawn over the particle and under the
//! weather, and over a ground's shadow, not darkened by it. The controls: the
//! particle alone, and the weather's batch counted among the particles'
//! (`particles_end` past it), which puts it under the strip. The strips were
//! drawn before the particles until 2026-10-28. Needs a graphics adapter (any
//! backend); without one it says so and passes.

use glam::{Mat4, Vec3};
use render::billboard::{Batch, Quad, Strips};
use render::fans::{Quad as FanQuad, Run};
use render::shadow::{Bias, Vertex, z_bias_reciprocal};
use render::{Renderer, View};

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

/// The eye, far from the origin as Freeport's places are, looking down at the
/// ground 10 below; what the middle pixel sees there.
const EYE: Vec3 = Vec3::new(25000.0, 60.0, 15000.0);
const GROUND: f32 = 50.0;
const SEEN: Vec3 = Vec3::new(25000.0, 50.0, 15002.0);

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

/// An opaque billboard in `color` a little above the ground, over the middle
/// pixel (no texture: white).
fn batch(color: [f32; 4]) -> Batch {
    Batch { texture: 0, blend: 0, ztest: true, zwrite: false, quads: vec![Quad { center: [SEEN.x, SEEN.y + 1.0, SEEN.z], half: [1.0, 1.0], frame: 0, color }] }
}

/// An opaque green strip across the middle pixel, as high as the billboards.
fn strip() -> Strips {
    let at = |x: f32, z: f32| ([0.0, 0.0], [SEEN.x + x, SEEN.y + 1.0, SEEN.z + z]);
    Strips { texture: 0, blend: 0, ztest: true, zwrite: false, color: GREEN, strips: vec![vec![at(-1.0, -1.0), at(-1.0, 1.0), at(1.0, -1.0), at(1.0, 1.0)]], colors: Vec::new() }
}

/// The ground: a white quad, opaque, writing its depth (corners relative to
/// the eye).
fn ground() -> Vec<Run> {
    let c = |x: f32, z: f32| [x - EYE.x, GROUND - EYE.y, z - EYE.z];
    let q = FanQuad { corners: [c(24900.0, 14900.0), c(25100.0, 14900.0), c(25100.0, 15100.0), c(24900.0, 15100.0)], uvs: [[0.5, 0.5]; 4], color: [1.0, 1.0, 1.0, 1.0], normal: [0.0, 1.0, 0.0] };
    vec![Run { texture: 0, blend: 0, zwrite: true, quads: vec![q] }]
}

/// A shadow triangle on the ground round the middle pixel, wound as the
/// query keeps one facing up.
fn shadow() -> Vec<Vertex> {
    let v = |x: f32, z: f32| Vertex { at: [x - EYE.x, GROUND + 0.04 - EYE.y, z - EYE.z], uv: [0.5, 0.5], alpha: 0.65 };
    vec![v(24999.0, 15001.0), v(25001.5, 15001.0), v(24999.0, 15003.5)]
}

/// The middle pixel of an 8x8 frame over the ground: `batches` (the first
/// `particles` of them the particle system's, the rest the weather's), the
/// shadow when `shadowed`, and `strips`.
fn middle(g: &Gpu, r: &mut Renderer, batches: &[Batch], particles: usize, shadowed: bool, strips: Vec<Strips>) -> [u8; 3] {
    let v = glam::camera::rh::view::look_at_mat4(EYE, EYE + Vec3::new(0.0, -10.0, 2.0), Vec3::Y);
    let p: Mat4 = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.5, 1000.0);
    let view = View { eye: EYE, ..View::daylight(p * v) };
    let tex = |id: i32| (id == 5).then(|| esf::Image { width: 1, height: 1, rgba: vec![0, 0, 0, 255], alpha: false });
    r.set_fans(&g.device, &g.queue, &ground(), EYE, &tex);
    let bias = Bias { clip_z: p.w_axis.z, k: z_bias_reciprocal(15, 0.5, 1000.0), behind: 0.0 };
    let tris = if shadowed { shadow() } else { Vec::new() };
    r.set_shadows(&g.device, &g.queue, 5, &tris, EYE, bias, &tex);
    r.particles_end = particles;
    r.strips = strips;
    let px = render::capture_with_sky(&g.device, &g.queue, r, 8, 8, &view, &[], Some((v, p, batches, &tex)), None);
    assert!(r.strips.is_empty(), "the frame's strips are used up");
    let at = (4 * 8 + 4) * 4;
    [px[at], px[at + 1], px[at + 2]]
}

fn near(c: [u8; 3], want: [f32; 4]) -> bool {
    (0..3).all(|k| (c[k] as f32 - want[k] * 255.0).abs() < 4.0)
}

#[test]
fn the_strips_go_after_the_particles_and_the_shadows_and_before_the_weather() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    // the controls: the particle alone is red; the ground alone white, under
    // the shadow darker
    let got = middle(&g, &mut r, &[batch(RED)], 1, false, Vec::new());
    assert!(near(got, RED), "the particle alone: {got:?}");
    let white = middle(&g, &mut r, &[], 0, false, Vec::new());
    let shaded = middle(&g, &mut r, &[], 0, true, Vec::new());
    assert!(shaded[0] < white[0] - 100, "the shadow darkens the ground: {shaded:?} against {white:?}");
    // the strip over the particle
    let got = middle(&g, &mut r, &[batch(RED)], 1, false, vec![strip()]);
    assert!(near(got, GREEN), "the strip over the particle: {got:?}");
    // the weather over the strip
    let got = middle(&g, &mut r, &[batch(RED), batch(BLUE)], 1, false, vec![strip()]);
    assert!(near(got, BLUE), "the weather over the strip: {got:?}");
    // control: the same batch among the particles' goes under the strip
    let got = middle(&g, &mut r, &[batch(RED), batch(BLUE)], 2, false, vec![strip()]);
    assert!(near(got, GREEN), "a particle under the strip: {got:?}");
    // the strip over the shadow, not darkened by it
    let got = middle(&g, &mut r, &[], 0, true, vec![strip()]);
    assert!(near(got, GREEN), "the strip over the shadow: {got:?}");
}
