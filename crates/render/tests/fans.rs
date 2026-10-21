//! The immediate-mode fans (`render::fans`, `VIRFan`'s maths): one quad over
//! the whole view, its pixel against the GS's arithmetic. A blended fan is
//! drawn ONCE (its front face; the effect volumes' fans are both faces of a
//! quad and VIRFan culls one), modulated by its texel, lit as VIRStrip
//! lights a vertex (colour + ambient + sun, clamped), fogged by depth, and
//! blended by its mode; nothing set, nothing drawn, and fans are drawn only
//! under the view they were made for. Needs a graphics adapter (any
//! backend); without one it says so and passes.

use glam::{Mat4, Vec3};
use render::fans::{Quad, Run};
use render::{Renderer, View};

/// A quad over the whole view: the projection is the identity and the eye
/// the origin, so the corners are clip space (depth 0.5, w 1).
fn quad(color: [f32; 4], normal: [f32; 3]) -> Quad {
    Quad {
        corners: [[-3.0, -3.0, 0.5], [3.0, -3.0, 0.5], [3.0, 3.0, 0.5], [-3.0, 3.0, 0.5]],
        uvs: [[0.5, 0.5]; 4],
        color,
        normal,
    }
}

/// The texel every fan here samples: (200, 100, 50) at alpha 128 of 255.
const TEXEL: [u8; 4] = [200, 100, 50, 128];

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

/// The centre pixel of a 4x4 frame cleared to the view's colour with `runs`
/// drawn over it (none set when `runs` is None).
fn pixel(g: &Gpu, r: &mut Renderer, view: &View, runs: Option<&[Run]>) -> [f32; 3] {
    let tex = |id: i32| (id == 7).then(|| esf::Image { width: 1, height: 1, rgba: TEXEL.to_vec(), alpha: true });
    if let Some(runs) = runs {
        r.set_fans(&g.device, &g.queue, runs, view.eye, &tex);
    }
    let px = render::capture(&g.device, &g.queue, r, 4, 4, view, &[], None);
    let at = (2 * 4 + 2) * 4;
    [px[at] as f32, px[at + 1] as f32, px[at + 2] as f32]
}

fn close(got: [f32; 3], want: [f32; 3], what: &str) {
    assert!((0..3).all(|k| (got[k] - want[k]).abs() <= 1.5), "{what}: {got:?}, want {want:?}");
}

#[test]
fn a_fan_is_lit_modulated_fogged_and_blended_once() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let clear = Vec3::new(0.2, 0.4, 0.6);
    let dark = View { sun: Vec3::ZERO, ambient: Vec3::ZERO, clear, ..View::daylight(Mat4::IDENTITY) };
    let t = Vec3::new(TEXEL[0] as f32, TEXEL[1] as f32, TEXEL[2] as f32) / 255.0;
    let ta = TEXEL[3] as f32 / 255.0;
    let c = clear * 255.0;
    let v3 = |v: Vec3| [v.x, v.y, v.z];

    // nothing set: the clear
    close(pixel(&g, &mut r, &dark, None), v3(c), "no fans");
    close(pixel(&g, &mut r, &dark, Some(&[])), v3(c), "none set");

    // a white fan at alpha 0.5, blended (mode 3) once: texel x light, alpha
    // the texel's times the fan's
    let white = |blend: usize, a: f32| vec![Run { texture: 7, blend, zwrite: false, quads: vec![quad([1.0, 1.0, 1.0, a], [0.0, 1.0, 0.0])] }];
    let a = ta * 0.5;
    let once = t * 255.0 * a + c * (1.0 - a);
    let got = pixel(&g, &mut r, &dark, Some(&white(3, 0.5)));
    close(got, v3(once), "mode 3");
    // control: both faces drawn would blend twice
    let twice = t * 255.0 * a + once * (1.0 - a);
    assert!((0..3).any(|k| (got[k] - v3(twice)[k]).abs() > 2.0), "the control must differ: {got:?} {twice:?}");

    // kept for their view: drawn again under it, not under another eye's
    close(pixel(&g, &mut r, &dark, None), v3(once), "kept");
    // (an eye so near that the quad would still cover the view if drawn)
    let elsewhere = View { eye: Vec3::new(0.0, 0.0, 1e-3), ..dark };
    close(pixel(&g, &mut r, &elsewhere, None), v3(c), "another view's");

    // mode 1 adds
    close(pixel(&g, &mut r, &dark, Some(&white(1, 0.5))), v3((c + t * 255.0 * a).min(Vec3::splat(255.0))), "mode 1");

    // lit: a black fan facing up under ambient and a sun shining down
    let lit = View { sun: Vec3::splat(0.25), ambient: Vec3::new(0.25, 0.5, 0.75), sun_dir: Vec3::new(0.0, -1.0, 0.0), ..dark };
    let black = vec![Run { texture: 7, blend: 3, zwrite: false, quads: vec![quad([0.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0])] }];
    let light = (Vec3::new(0.25, 0.5, 0.75) + Vec3::splat(0.25)).min(Vec3::ONE);
    close(pixel(&g, &mut r, &lit, Some(&black)), v3(t * light * 255.0 * ta + c * (1.0 - ta)), "lit");
    // facing down, the sun adds nothing
    let down = vec![Run { texture: 7, blend: 3, zwrite: false, quads: vec![quad([0.0, 0.0, 0.0, 1.0], [0.0, -1.0, 0.0])] }];
    close(pixel(&g, &mut r, &lit, Some(&down)), v3(t * Vec3::new(0.25, 0.5, 0.75) * 255.0 * ta + c * (1.0 - ta)), "facing away");

    // fogged before it blends: depth 1 halfway from near 0 to far 2
    let fog_c = Vec3::new(1.0, 0.0, 1.0);
    let foggy = View { fog: Some((0.0, 2.0, fog_c)), ..dark };
    let src = (t * 0.5 + fog_c * 0.5) * 255.0;
    close(pixel(&g, &mut r, &foggy, Some(&white(3, 1.0))), v3(src * ta + c * (1.0 - ta)), "fogged");
}
