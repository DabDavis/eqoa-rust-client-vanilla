//! The ground fog a PS2 frame keeps (`render::fog16`): fans marked `ps2`
//! over a frame against the GS's own steps into a 16-bit frame,
//! `C = clamp(C + ((S - C) * As >> 7) + d, 0, 255) & ~7` from the frame as
//! five bits a channel, d each pixel's DIMX entry (0x2031312020313120: 0 to
//! 3). Equal fans are the GS exactly, every pixel (alphas of 1/2, 1/4 and
//! 3/128 of a texel of 255 keep every sum exact in the float targets); a
//! faint fan's lift never shows where a blend of eight bits a channel lifts
//! it every time, and its darkening shows more; fans of mixed strength come
//! out within a few levels of the GS over the frame. The sequential
//! reference (`set_ps2_sequential`, a fan at a time) is the GS exactly, mixed
//! fans too. The controls: the same fans drawn as they are (a renderer not
//! told it can add the sums up), and the pixels no fan covers, untouched.
//! Needs a graphics adapter (any backend); without one, or one that cannot
//! draw into float textures, it says so and passes.

use glam::{Mat4, Vec3};
use render::fans::{Quad, Run};
use render::{Renderer, View};

/// The texel every fan here samples: (200, 100, 50) at alpha 255.
const TEXEL: [u8; 4] = [200, 100, 50, 255];

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
    if !render::fog16::supported(&adapter) {
        eprintln!("SKIP: the adapter cannot add up the fog's sums (no float colour buffers)");
        return None;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    Some(Gpu { device, queue })
}

/// A quad over clip space from x0 to x1 (the projection is the identity and
/// the eye the origin: depth 0.5, w 1), of a white light at alpha `a`.
fn quad(x0: f32, x1: f32, a: f32) -> Quad {
    Quad { corners: [[x0, -3.0, 0.5], [x1, -3.0, 0.5], [x1, 3.0, 0.5], [x0, 3.0, 0.5]], uvs: [[0.5, 0.5]; 4], color: [1.0, 1.0, 1.0, a], normal: [0.0, 1.0, 0.0] }
}

/// The fans of `alphas` in order over the whole view, or from x0 to x1.
fn fog(alphas: &[f32], (x0, x1): (f32, f32)) -> Vec<Run> {
    vec![Run { texture: 7, blend: 3, zwrite: false, ps2: true, quads: alphas.iter().map(|&a| quad(x0, x1, a)).collect() }]
}

const W: usize = 8;

/// The frame cleared to `clear`, unlit, with `runs` over it: each pixel.
fn frame(g: &Gpu, r: &mut Renderer, clear: Vec3, runs: &[Run]) -> Vec<[f32; 3]> {
    let tex = |id: i32| (id == 7).then(|| esf::Image { width: 1, height: 1, rgba: TEXEL.to_vec(), alpha: true });
    let view = View { sun: Vec3::ZERO, ambient: Vec3::ZERO, clear, ..View::daylight(Mat4::IDENTITY) };
    r.set_fans(&g.device, &g.queue, runs, view.eye, &tex);
    let px = render::capture(&g.device, &g.queue, r, W as u32, W as u32, &view, &[], None);
    px.chunks(4).map(|p| [p[0] as f32, p[1] as f32, p[2] as f32]).collect()
}

/// DIMX 0x2031312020313120's entry at pixel (x, y).
fn dither(x: usize, y: usize) -> i32 {
    ((0x2031_3120_2031_3120u64 >> (4 * (4 * (y & 3) + (x & 3)))) & 7) as i32
}

/// The GS: `alphas` (of a texel of 255, so As = a × 128) blended in order
/// into a 16-bit frame holding `c0` as five bits (the nearest), from the
/// texel's colour, under the matrix entry `d`; the frame changed by what
/// they change it.
fn gs(c0: [f32; 3], alphas: &[f32], d: i32) -> [f32; 3] {
    std::array::from_fn(|k| {
        let s = TEXEL[k] as i32;
        let cq0 = ((c0[k] / 8.0 + 0.5).floor() as i32 * 8).clamp(0, 248);
        let c = alphas.iter().fold(cq0, |c, &a| {
            let a = (a * 128.0) as i32;
            (c + (((s - c) * a) >> 7) + d).clamp(0, 255) & !7
        });
        (c0[k] + (c - cq0) as f32).clamp(0.0, 255.0)
    })
}

/// A blend of eight bits a channel: `alphas` in order over `c0`, each kept
/// to the nearest level, as the frame keeps it.
fn blend(c0: [f32; 3], alphas: &[f32]) -> [f32; 3] {
    std::array::from_fn(|k| alphas.iter().fold(c0[k], |c, &a| (c + (TEXEL[k] as f32 - c) * a).round()))
}

fn close(got: [f32; 3], want: [f32; 3], by: f32, what: &str) {
    assert!((0..3).all(|k| (got[k] - want[k]).abs() <= by), "{what}: {got:?}, want {want:?}");
}

/// Every pixel of `got` against the GS under its own matrix entry.
fn each_pixel(got: &[[f32; 3]], c0: [f32; 3], alphas: &[f32], by: f32, what: &str) {
    for (i, px) in got.iter().enumerate() {
        close(*px, gs(c0, alphas, dither(i % W, i / W)), by, &format!("{what}, pixel {i}"));
    }
}

/// A frame's mean.
fn mean(px: &[[f32; 3]]) -> [f32; 3] {
    std::array::from_fn(|k| px.iter().map(|p| p[k]).sum::<f32>() / px.len() as f32)
}

#[test]
fn ground_fog_is_blended_as_the_gs_blends_it_into_a_16_bit_frame() {
    let Some(g) = gpu() else { return };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    r.set_ps2_sums(true);
    let clear = Vec3::new(0.2, 0.4, 0.6);
    let c0 = [51.0, 102.0, 153.0];
    let whole = (-3.0, 3.0);

    // equal fans: the GS exactly, every pixel, strong, middling and faint
    for (alphas, what) in [(vec![0.5], "one strong fan"), (vec![0.25; 6], "six middling fans"), (vec![3.0 / 128.0; 12], "twelve faint fans")] {
        each_pixel(&frame(&g, &mut r, clear, &fog(&alphas, whole)), c0, &alphas, 1.0, what);
    }

    // twelve faint fans: on a PS2 the red's lift never shows (each is 3
    // levels, and the dither's 3 at most never reaches the next 8), the
    // blue's small darkenings show more; a blend of eight bits a channel
    // lifts the red 37 levels and darkens the blue less
    let faint = vec![3.0 / 128.0; 12];
    let ps2 = frame(&g, &mut r, clear, &fog(&faint, whole));
    assert!(ps2.iter().all(|p| p[0] == c0[0]), "the red's faint lifts never show: {:?}", mean(&ps2));
    assert!(mean(&ps2)[2] < blend(c0, &faint)[2] - 4.0, "the blue's darkenings show more: {:?}, {:?}", mean(&ps2), blend(c0, &faint));

    // fans of mixed strength: within a few levels of the GS over the frame
    let mixed = [0.5, 0.125, 0.25, 0.125, 0.0625, 0.375];
    let want: Vec<[f32; 3]> = (0..W * W).map(|i| gs(c0, &mixed, dither(i % W, i / W))).collect();
    close(mean(&frame(&g, &mut r, clear, &fog(&mixed, whole))), mean(&want), 6.0, "mixed fans");

    // the reference, a fan at a time: the GS exactly, mixed fans too
    let mut seq = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    seq.set_ps2_sums(true);
    seq.set_ps2_sequential(true);
    each_pixel(&frame(&g, &mut seq, clear, &fog(&mixed, whole)), c0, &mixed, 1.0, "the reference, mixed fans");
    each_pixel(&frame(&g, &mut seq, clear, &fog(&faint, whole)), c0, &faint, 1.0, "the reference, faint fans");

    // the control: the pixels no fan covers keep the frame as it was
    let half = frame(&g, &mut r, clear, &fog(&faint, (-3.0, 0.0)));
    for y in 0..W {
        close(half[y * W + W - 1], c0, 0.0, "a pixel the fans do not cover");
        close(half[y * W], ps2[y * W], 0.0, "a pixel they do");
    }

    // the control: a renderer not told it can add the sums up draws the
    // same fans as a blend of eight bits, the red lifted
    let mut plain = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let got = frame(&g, &mut plain, clear, &fog(&faint, whole))[W * W / 2 + W / 2];
    close(got, blend(c0, &faint), 1.5, "drawn as they are");
    assert!(got[0] > c0[0] + 30.0, "the control must differ: {got:?}");
}

/// `cargo test --release -p render --test fog16 -- --ignored --nocapture`:
/// what the ground fog a PS2 frame keeps costs the GPU, a 1920 by 1080
/// frame of 60 fog fans over a band a third of its height (Freeport's
/// sign post at night draws 40 to 70): the frame with them blended as they
/// are and as a PS2 frame keeps them, less the frame without fans, the best
/// of five runs of 100 frames each waited for.
#[test]
#[ignore]
fn ground_fog_cost() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else { return };
    if !render::fog16::supported(&adapter) {
        return;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    eprintln!("{:?}", adapter.get_info().name);
    let (w, h) = (1920, 1080);
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: render::TARGET_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let tv = target.create_view(&Default::default());
    let depth = render::depth_view(&device, w, h);
    let tex = |id: i32| (id == 7).then(|| esf::Image { width: 1, height: 1, rgba: TEXEL.to_vec(), alpha: true });
    let view = View { sun: Vec3::ZERO, ambient: Vec3::ZERO, clear: Vec3::new(0.2, 0.4, 0.6), ..View::daylight(Mat4::IDENTITY) };
    // 60 puffs over the band from y -0.6 to 0.1, overlapping
    let quads: Vec<Quad> = (0..60)
        .map(|k| {
            let x = -1.0 + (k % 15) as f32 * 0.14;
            let y = -0.6 + (k / 15) as f32 * 0.12;
            Quad { corners: [[x, y, 0.5], [x + 0.35, y, 0.5], [x + 0.35, y + 0.35, 0.5], [x, y + 0.35, 0.5]], uvs: [[0.5, 0.5]; 4], color: [1.0, 1.0, 1.0, 0.1], normal: [0.0, 1.0, 0.0] }
        })
        .collect();
    let time = |ps2: Option<bool>| -> f64 {
        let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
        r.set_ps2_sums(true);
        let runs = ps2.map(|p| vec![Run { texture: 7, blend: 3, zwrite: false, ps2: p, quads: quads.clone() }]).unwrap_or_default();
        r.set_fans(&device, &queue, &runs, view.eye, &tex);
        let mut best = f64::MAX;
        for _ in 0..5 {
            let start = std::time::Instant::now();
            for _ in 0..100 {
                let mut enc = device.create_command_encoder(&Default::default());
                r.draw(&queue, &mut enc, &tv, &depth, &view, &[]);
                r.draw_fans(&device, &queue, &mut enc, &tv, &depth, &view);
                queue.submit([enc.finish()]);
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            }
            best = best.min(start.elapsed().as_secs_f64() * 1000.0 / 100.0);
        }
        best
    };
    let (none, exact, ps2) = (time(None), time(Some(false)), time(Some(true)));
    eprintln!("a frame: no fans {none:.3} ms; the fog as it is +{:.3} ms; as a PS2 frame keeps it +{:.3} ms", exact - none, ps2 - none);
}
