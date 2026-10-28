//! The blob shadows' pass (`render::shadow`, `VIScene::RenderShadow` under
//! `SetSky`'s material): over an opaque white ground that wrote its depth (a
//! fan), a shadow triangle 0.02 UNDER that ground, ten units below the eye,
//! is drawn over it, darkening it by its alpha (blend 3: the black texel at
//! 0.65 leaves 35% of the white), because `SetZBias(15)` draws it nearer the
//! camera; with no bias it is hidden under the ground (the control for the
//! bias). A shadow 0.05 under is drawn from the eye's own camera (pulled
//! 0.30 at 10) but not when the eye is 8 behind the game's camera (the
//! client's zoom): the game's camera, 2 from the ground, pulls it 0.014. Its triangles face as `RenderShadow` hands them (the query's
//! up-facing winding): the same triangle wound the other way, seen from
//! above, is culled (the control for the face). Nothing set, nothing drawn;
//! set for another eye, not drawn. Needs a graphics adapter (any backend);
//! without one it says so and passes.

use glam::{Mat4, Vec3};
use render::fans::{Quad, Run};
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

/// The eye, far from the origin as Freeport's places are, looking down at
/// the ground 10 below.
const EYE: Vec3 = Vec3::new(25000.0, 60.0, 15000.0);
const GROUND: f32 = 50.0;

/// The ground: a white quad, opaque, writing its depth (corners relative to
/// the eye).
fn ground() -> Vec<Run> {
    let c = |x: f32, z: f32| [x - EYE.x, GROUND - EYE.y, z - EYE.z];
    let q = Quad { corners: [c(24900.0, 14900.0), c(25100.0, 14900.0), c(25100.0, 15100.0), c(24900.0, 15100.0)], uvs: [[0.5, 0.5]; 4], color: [1.0, 1.0, 1.0, 1.0], normal: [0.0, 1.0, 0.0] };
    vec![Run { texture: 0, blend: 0, zwrite: true, quads: vec![q] }]
}

/// A shadow triangle at `y` round the point the middle pixel sees (x 25000,
/// z 15002), a shadow's size, wound as the query keeps one facing up
/// (`VIPlane`'s normal (c - b) x (b - a) up: (xa, za), (xb, za), (xa, zb)),
/// or the other way.
fn shadow(y: f32, up: bool) -> Vec<Vertex> {
    let v = |x: f32, z: f32| Vertex { at: [x - EYE.x, y - EYE.y, z - EYE.z], uv: [0.5, 0.5], alpha: 0.65 };
    let (a, b, c) = (v(24999.0, 15001.0), v(25001.5, 15001.0), v(24999.0, 15003.5));
    if up { vec![a, b, c] } else { vec![a, c, b] }
}

/// The middle pixel of an 8x8 frame: the ground, then `tris` (none: not set)
/// at `bias`, under the view at `eye`.
fn middle(g: &Gpu, r: &mut Renderer, tris: Option<&[Vertex]>, bias: Bias, eye: Vec3) -> [f32; 3] {
    let v = glam::camera::rh::view::look_at_mat4(eye, eye + Vec3::new(0.0, -10.0, 2.0), Vec3::Y);
    let p: Mat4 = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.5, 1000.0);
    let view = View { eye, ..View::daylight(p * v) };
    let tex = |id: i32| (id == 5).then(|| esf::Image { width: 1, height: 1, rgba: vec![0, 0, 0, 255], alpha: false });
    r.set_fans(&g.device, &g.queue, &ground(), eye, &tex);
    if let Some(t) = tris {
        r.set_shadows(&g.device, &g.queue, 5, t, EYE, bias, &tex);
    }
    let px = render::capture_with_sky(&g.device, &g.queue, r, 8, 8, &view, &[], Some((v, p, &[], &tex)), None);
    let at = (4 * 8 + 4) * 4;
    [px[at] as f32, px[at + 1] as f32, px[at + 2] as f32]
}

fn close(got: [f32; 3], want: f32, what: &str) {
    assert!(got.iter().all(|c| (c - want).abs() <= 1.5), "{what}: {got:?}, want {want}");
}

#[test]
fn a_shadow_is_drawn_nearer_by_its_bias_on_its_up_face() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let p: Mat4 = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.5, 1000.0);
    let bias = Bias { clip_z: p.w_axis.z, k: z_bias_reciprocal(15, 0.1, 1000.0), behind: 0.0 };
    let white = 255.0;
    let shaded = white * (1.0 - 0.65);
    close(middle(&g, &mut r, None, bias, EYE), white, "nothing set: the ground");
    // on the ground, lifted as RenderShadow lifts it: drawn
    close(middle(&g, &mut r, Some(&shadow(GROUND + 0.04, true)), bias, EYE), shaded, "lifted 0.04");
    // under the ground: drawn only nearer by the bias
    close(middle(&g, &mut r, Some(&shadow(GROUND - 0.02, true)), bias, EYE), shaded, "0.02 under, biased");
    close(middle(&g, &mut r, Some(&shadow(GROUND - 0.02, true)), Bias::default(), EYE), white, "the control: 0.02 under, no bias: hidden");
    close(middle(&g, &mut r, Some(&shadow(GROUND - 0.05, true)), bias, EYE), shaded, "0.05 under, from the eye's own camera: drawn");
    close(middle(&g, &mut r, Some(&shadow(GROUND - 0.05, true)), Bias { behind: 8.0, ..bias }, EYE), white, "0.05 under, the game's camera 8 nearer: hidden");
    // past what the bias reaches at ten units (about 0.3), hidden
    close(middle(&g, &mut r, Some(&shadow(GROUND - 1.0, true)), bias, EYE), white, "a unit under: hidden");
    // wound the other way, seen from above: culled
    close(middle(&g, &mut r, Some(&shadow(GROUND + 0.04, false)), bias, EYE), white, "the control: the other face");
    // set empty, or for another eye: not drawn
    close(middle(&g, &mut r, Some(&[]), bias, EYE), white, "set empty");
    // a texture no file has (the cache keeps 5 by its id): none
    r.set_shadows(&g.device, &g.queue, 6, &shadow(GROUND + 0.04, true), EYE, bias, &|_: i32| None::<esf::Image>);
    close(middle(&g, &mut r, None, bias, EYE), white, "no texture: none");
    let other = EYE + Vec3::new(0.0, 0.0, 0.5);
    let tex = |id: i32| (id == 5).then(|| esf::Image { width: 1, height: 1, rgba: vec![0, 0, 0, 255], alpha: false });
    r.set_shadows(&g.device, &g.queue, 5, &shadow(GROUND + 0.04, true), EYE, bias, &tex);
    close(middle(&g, &mut r, None, bias, other), white, "set for another eye");
}
