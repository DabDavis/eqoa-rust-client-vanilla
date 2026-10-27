//! The billboard program's draws keep their buffers from frame to frame (a
//! vertex buffer per draw of a frame, grown as needed, and its frame
//! uniforms), where they were made anew every frame: 31 uniform buffers, 62
//! bind groups and a vertex buffer a frame under the Lavastorm's fireballs,
//! which held a page near 50 fps at Klick'Anon (F3 20261027-025514). Kept,
//! a frame must still draw exactly what it is given: nothing of the frame
//! before, the same pixels as a renderer drawing it first, and the sky's
//! billboards and the world's, drawn in one submit, each from their own.
//! Needs a graphics adapter (any backend); without one it says so and passes.

use glam::{Mat4, Vec3};
use render::billboard::{Batch, Quad};
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

const EYE: Vec3 = Vec3::new(25000.0, 60.0, 15000.0);
const AHEAD: Vec3 = Vec3::new(25000.0, 60.0, 15010.0);
const SIZE: u32 = 32;

/// A particle `dx` across from the middle of the view, 10 ahead.
fn quad(dx: f32, half: f32, color: [f32; 4]) -> Quad {
    Quad { center: [AHEAD.x + dx, AHEAD.y, AHEAD.z], half: [half, half], frame: 0, color }
}

/// An emitter's particles as the world draws them: added (blend 1), Z
/// tested, untextured (white).
fn batch(quads: Vec<Quad>) -> Batch {
    Batch { texture: 0, blend: 1, ztest: true, zwrite: false, quads }
}

/// A frame of `world` (and of `sky`, through the sky's view: its quads'
/// centres are directions from the eye).
fn frame(g: &Gpu, r: &mut Renderer, world: &[Batch], sky: &[Batch]) -> Vec<u8> {
    let v = glam::camera::rh::view::look_at_mat4(EYE, AHEAD, Vec3::Y);
    let p: Mat4 = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.5, 1000.0);
    let view = View::daylight(p * v);
    let mut rot = v;
    rot.w_axis = glam::Vec4::W;
    let none = |_: i32| -> Option<esf::Image> { None };
    render::capture_with_sky(&g.device, &g.queue, r, SIZE, SIZE, &view, &[], Some((v, p, world, &none)), (!sky.is_empty()).then_some((rot, p, sky, &none)))
}

/// The pixels where `a` and `b` differ.
fn differ(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4).zip(b.chunks(4)).filter(|(x, y)| x[..3] != y[..3]).count()
}

#[test]
fn a_frame_draws_what_it_is_given_and_nothing_of_the_last() {
    let Some(g) = gpu() else { return };
    let red = [1.0, 0.0, 0.0, 1.0];
    let green = [0.0, 1.0, 0.0, 1.0];
    let left = vec![batch(vec![quad(-3.0, 1.5, red)])];
    let right = vec![batch(vec![quad(3.0, 1.0, green)]), batch(vec![quad(2.0, 0.5, red)])];
    // more quads than the kept buffer's first 64 KB, so it grows
    let many = vec![batch((0..400).map(|i| quad(-4.0 + i as f32 * 0.02, 0.3, green)).collect())];
    let fresh = |w: &[Batch]| frame(&g, &mut Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT), w, &[]);
    let (clear, a, b, c) = (fresh(&[]), fresh(&left), fresh(&right), fresh(&many));
    // the controls: each frame draws something, and something else
    assert!(differ(&clear, &a) > 0 && differ(&clear, &b) > 0 && differ(&a, &b) > 0 && differ(&b, &c) > 0);
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    for (name, world, want) in [("left", &left, &a), ("right", &right, &b), ("left again", &left, &a), ("many", &many, &c), ("right after many", &right, &b), ("none", &Vec::new(), &clear)] {
        let got = frame(&g, &mut r, world, &[]);
        assert_eq!(differ(&got, want), 0, "{name}: as a renderer drawing it first");
    }
}

#[test]
fn the_skys_billboards_and_the_worlds_keep_their_own_in_one_submit() {
    let Some(g) = gpu() else { return };
    let blue = [0.0, 0.0, 1.0, 1.0];
    let red = [1.0, 0.0, 0.0, 1.0];
    // the sky's: a direction ahead and off to one side; the world's: the other side
    let sky = vec![Batch { texture: 0, blend: 1, ztest: false, zwrite: false, quads: vec![Quad { center: [-3.0, 0.0, 10.0], half: [1.5, 1.5], frame: 0, color: blue }] }];
    let world = vec![batch(vec![quad(3.0, 1.5, red)]), batch(vec![quad(3.5, 1.0, red)])];
    let fresh = || Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let only_sky = frame(&g, &mut fresh(), &[], &sky);
    let only_world = frame(&g, &mut fresh(), &world, &[]);
    let clear = frame(&g, &mut fresh(), &[], &[]);
    let mut r = fresh();
    for round in 0..3 {
        let both = frame(&g, &mut r, &world, &sky);
        // every pixel is the sky's where only the sky drew, the world's where only the world did
        let mut checked = (0, 0);
        for i in (0..both.len()).step_by(4) {
            let (s, w, c) = (&only_sky[i..i + 3], &only_world[i..i + 3], &clear[i..i + 3]);
            if s != c && w == c {
                assert_eq!(&both[i..i + 3], s, "round {round}: a sky pixel");
                checked.0 += 1;
            } else if w != c && s == c {
                assert_eq!(&both[i..i + 3], w, "round {round}: a world pixel");
                checked.1 += 1;
            }
        }
        assert!(checked.0 > 0 && checked.1 > 0, "the controls: both drew somewhere of their own ({checked:?})");
    }
}
