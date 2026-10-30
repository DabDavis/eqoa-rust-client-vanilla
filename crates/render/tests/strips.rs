//! The frame's triangle strips (`Renderer::strips`: a spell's lightning,
//! `VILightning::Render`, and the weapon trails, `VITrailFxRender`) are drawn
//! through the WORLD's view, after the world, whether or not the sky's
//! billboards (the stars, the weather's lightning patches) were drawn before
//! them through the sky's view, whose translation is zeroed: taken by the
//! sky's draw, as they were, a bolt far from the world's origin went where
//! nothing is, and no spell's lightning or weapon's trail was ever seen in the
//! world (F3 20261026-041245), though the spell preview, which has no sky,
//! drew them. Needs a graphics adapter (any backend); without one it says so
//! and passes.

use glam::{Mat4, Vec3};
use render::billboard::Strips;
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

/// The eye, far from the origin as the world's places are (Freeport's), and
/// what it looks at, 10 ahead.
const EYE: Vec3 = Vec3::new(25000.0, 60.0, 15000.0);
const AHEAD: Vec3 = Vec3::new(25000.0, 60.0, 15010.0);

/// A white strip over the middle of the view, as the lightning's (blend 1,
/// add; Z tested, no Z write; no texture: white).
fn bolt() -> Strips {
    let at = |x: f32, y: f32| ([0.0, 0.0], [AHEAD.x + x, AHEAD.y + y, AHEAD.z]);
    Strips { texture: 0, blend: 1, ztest: true, zwrite: false, color: [1.0, 1.0, 1.0, 1.0], strips: vec![vec![at(-1.0, -1.0), at(-1.0, 1.0), at(1.0, -1.0), at(1.0, 1.0)]], colors: Vec::new() }
}

/// The middle pixel of an 8x8 frame of nothing but the clear, the sky's
/// billboards (none) drawn through the sky's view first when `sky`, then the
/// world's, with `strips` set.
fn middle(g: &Gpu, r: &mut Renderer, strips: Vec<Strips>, sky: bool) -> [u8; 3] {
    let v = glam::camera::rh::view::look_at_mat4(EYE, AHEAD, Vec3::Y);
    let p: Mat4 = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.5, 1000.0);
    let view = View::daylight(p * v);
    let mut rot = v;
    rot.w_axis = glam::Vec4::W;
    let none = |_: i32| -> Option<esf::Image> { None };
    r.strips = strips;
    let px = render::capture_with_sky(&g.device, &g.queue, r, 8, 8, &view, &[], Some((v, p, &[], &none)), sky.then_some((rot, p, &[][..], &none)));
    assert!(r.strips.is_empty(), "the frame's strips are used up");
    let at = (4 * 8 + 4) * 4;
    [px[at], px[at + 1], px[at + 2]]
}

#[test]
fn the_lightning_is_drawn_through_the_worlds_view_under_a_sky() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let clear = middle(&g, &mut r, Vec::new(), true);
    // the bolt adds its white to the clear (its alpha × 64, which the GS's
    // blend reads as one: billboard_alpha.rs)
    let lit = |c: [u8; 3]| (0..3).all(|k| c[k] as i32 >= clear[k] as i32 + 100);
    // the controls: no strips, the clear, with or without the sky
    assert_eq!(middle(&g, &mut r, Vec::new(), false), clear, "no strips, no sky");
    // with no sky (the spell preview's frame) the bolt is drawn
    assert!(lit(middle(&g, &mut r, vec![bolt()], false)), "no sky");
    // and under the sky's billboards, through the world's view, the same
    let got = middle(&g, &mut r, vec![bolt()], true);
    assert!(lit(got), "under the sky: {got:?}, the clear {clear:?}");
}
