//! The particles a PS2 frame keeps (`Renderer::set_ps2_particles`): the
//! world's particles of blend mode 3 over a frame against the GS's own steps
//! into the 16-bit frame the game draws in, `C = clamp(C + ((S - C) * As >>
//! 7) + d, 0, 255) & ~7` from the frame as five bits a channel, d each
//! pixel's DIMX entry (a particle's material is `SetDither(0)`'s:
//! 0x2031312020313120, 0 to 3), as `fog16` blends the ground fog. Equal
//! particles are the GS exactly, every pixel; a faint particle's lift never
//! shows where a blend of eight bits a channel lifts it every time, and many
//! faint ones stop short of their colour, which eight bits a channel reaches
//! (the Red Swarming Death's swarm: some fifty of its puffs on a pixel at
//! alphas of a tenth to a quarter); particles of mixed strength come out
//! within a few levels of the GS. The sequential reference
//! (`set_ps2_sequential`, a particle at a time) is the GS exactly, mixed ones
//! too. The controls: the same particles with the switch off, and on a
//! renderer not told it can add the sums up (drawn as they are, eight bits a
//! channel), an additive particle (drawn as it is, the switch on), and the
//! pixels no particle covers, untouched. Needs a graphics adapter (any
//! backend); without one, or one that cannot draw into float textures, it
//! says so and passes.

use glam::{Mat4, Vec3};
use render::billboard::{Batch, Quad};
use render::{Renderer, View};

/// The texel every particle here samples: (200, 100, 50) at alpha 255.
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
        eprintln!("SKIP: the adapter cannot add up the particles' sums (no float colour buffers)");
        return None;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    Some(Gpu { device, queue })
}

const EYE: Vec3 = Vec3::new(25000.0, 60.0, 15000.0);
const AHEAD: Vec3 = Vec3::new(25000.0, 60.0, 15010.0);
const W: usize = 8;

fn texture(id: i32) -> Option<esf::Image> {
    (id == 7).then(|| esf::Image { width: 1, height: 1, rgba: TEXEL.to_vec(), alpha: true })
}

/// Particles of white at `alphas`, in order, of blend mode `blend`, each
/// over the whole view, or (`left`) over its left half (looking down +z,
/// the world's +x is the view's left).
fn particles(alphas: &[f32], blend: usize, left: bool) -> Vec<Batch> {
    let (dx, half) = if left { (20.0, [20.0, 40.0]) } else { (0.0, [40.0, 40.0]) };
    let quads = alphas.iter().map(|&a| Quad { center: [AHEAD.x + dx, AHEAD.y, AHEAD.z], half, frame: 0, color: [1.0, 1.0, 1.0, a] }).collect();
    vec![Batch { texture: 7, blend, ztest: true, zwrite: false, quads }]
}

/// The frame cleared to `clear`, unlit, with `batches` over it: each pixel.
fn frame(g: &Gpu, r: &mut Renderer, clear: Vec3, batches: &[Batch]) -> Vec<[f32; 3]> {
    let v = glam::camera::rh::view::look_at_mat4(EYE, AHEAD, Vec3::Y);
    let p: Mat4 = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.5, 1000.0);
    let view = View { sun: Vec3::ZERO, ambient: Vec3::ZERO, clear, ..View::daylight(p * v) };
    let px = render::capture(&g.device, &g.queue, r, W as u32, W as u32, &view, &[], Some((v, p, batches, &texture)));
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

/// A renderer that adds the sums up, with the particles' switch at `on`.
fn renderer(g: &Gpu, on: bool) -> Renderer {
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    r.set_ps2_sums(true);
    r.set_ps2_particles(on);
    r
}

#[test]
fn particles_are_blended_as_the_gs_blends_them_into_a_16_bit_frame() {
    let Some(g) = gpu() else { return };
    let mut r = renderer(&g, true);
    let clear = Vec3::new(0.2, 0.4, 0.6);
    let c0 = [51.0, 102.0, 153.0];

    // equal particles: the GS exactly, every pixel, strong, middling and faint
    for (alphas, what) in [(vec![0.5], "one strong particle"), (vec![0.25; 6], "six middling particles"), (vec![3.0 / 128.0; 12], "twelve faint particles")] {
        each_pixel(&frame(&g, &mut r, clear, &particles(&alphas, 3, false)), c0, &alphas, 1.0, what);
    }

    // twelve faint particles: on a PS2 the red's lift never shows (each is 3
    // levels, and the dither's 3 at most never reaches the next 8), the
    // blue's small darkenings show more; a blend of eight bits a channel
    // lifts the red 37 levels and darkens the blue less
    let faint = vec![3.0 / 128.0; 12];
    let ps2 = frame(&g, &mut r, clear, &particles(&faint, 3, false));
    assert!(ps2.iter().all(|p| p[0] == c0[0]), "the red's faint lifts never show: {:?}", mean(&ps2));
    assert!(mean(&ps2)[2] < blend(c0, &faint)[2] - 4.0, "the blue's darkenings show more: {:?}, {:?}", mean(&ps2), blend(c0, &faint));

    // fifty puffs of a tenth (the swarm's, its alpha 0.1 to 0.25 of its
    // texel): eight bits a channel reach the puffs' colour, the GS stops
    // short of it, where each step no longer reaches the next five bits
    let swarm = vec![13.0 / 128.0; 50];
    let ps2 = frame(&g, &mut r, clear, &particles(&swarm, 3, false));
    each_pixel(&ps2, c0, &swarm, 1.0, "fifty puffs of a tenth");
    let eight = blend(c0, &swarm);
    assert!((eight[1] - TEXEL[1] as f32).abs() <= 2.0, "eight bits reach the colour: {eight:?}");
    assert!(mean(&ps2)[1] > eight[1] + 4.0 || mean(&ps2)[2] > eight[2] + 4.0, "the GS stops short of it: {:?}, {eight:?}", mean(&ps2));

    // particles of mixed strength: within a few levels of the GS over the frame
    let mixed = [0.5, 0.125, 0.25, 0.125, 0.0625, 0.375];
    let want: Vec<[f32; 3]> = (0..W * W).map(|i| gs(c0, &mixed, dither(i % W, i / W))).collect();
    close(mean(&frame(&g, &mut r, clear, &particles(&mixed, 3, false))), mean(&want), 6.0, "mixed particles");

    // the reference, a particle at a time: the GS exactly, mixed ones too
    let mut seq = renderer(&g, true);
    seq.set_ps2_sequential(true);
    each_pixel(&frame(&g, &mut seq, clear, &particles(&mixed, 3, false)), c0, &mixed, 1.0, "the reference, mixed particles");
    each_pixel(&frame(&g, &mut seq, clear, &particles(&faint, 3, false)), c0, &faint, 1.0, "the reference, faint particles");

    // the controls: the switch off, and a renderer that cannot add the sums
    // up, draw them as they are, eight bits a channel
    let mut off = renderer(&g, false);
    close(mean(&frame(&g, &mut off, clear, &particles(&faint, 3, false))), blend(c0, &faint), 1.0, "the switch off: as they are");
    let mut nosums = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    nosums.set_ps2_particles(true);
    close(mean(&frame(&g, &mut nosums, clear, &particles(&faint, 3, false))), blend(c0, &faint), 1.0, "no sums: as they are");
    // an additive particle is drawn as it is, the switch on: Cd + Cv × As
    let add = frame(&g, &mut r, clear, &particles(&[0.25], 1, false));
    close(mean(&add), std::array::from_fn(|k| (c0[k] + TEXEL[k] as f32 * 0.25).min(255.0)), 1.0, "an additive particle, as it is");
    // the pixels no particle covers: untouched; those under them within a
    // few levels of the GS, as over the whole frame
    let half = frame(&g, &mut r, clear, &particles(&mixed, 3, true));
    let (mut under, mut want) = (Vec::new(), Vec::new());
    for (i, px) in half.iter().enumerate() {
        if i % W >= W / 2 + 1 {
            close(*px, c0, 0.0, &format!("pixel {i}, no particle"));
        } else if i % W < W / 2 - 1 {
            under.push(*px);
            want.push(gs(c0, &mixed, dither(i % W, i / W)));
        }
    }
    close(mean(&under), mean(&want), 6.0, "half the frame, under them");
}

/// `cargo test --release -p render --test particles16 -- --ignored --nocapture`:
/// what the PS2's blend costs a 1920x1080 frame on this adapter, a swarm's
/// 126 puffs over the middle fifth of the frame against them drawn as they
/// are and against none (best of five runs of 100 frames, each submitted and
/// waited for: the GPU's time, with the copy and the passes).
#[test]
#[ignore]
fn particles_cost() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else { return };
    if !render::fog16::supported(&adapter) {
        return;
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    eprintln!("{:?}", adapter.get_info().name);
    let g = Gpu { device, queue };
    let (w, h) = (1920, 1080);
    let v = glam::camera::rh::view::look_at_mat4(EYE, AHEAD, Vec3::Y);
    let p: Mat4 = glam::camera::rh::proj::directx::perspective(0.8, w as f32 / h as f32, 0.5, 1000.0);
    let view = View { sun: Vec3::ZERO, ambient: Vec3::ZERO, clear: Vec3::new(0.2, 0.4, 0.6), ..View::daylight(p * v) };
    // 126 puffs of a tenth to a quarter, half sizes 0.4 to 0.75, in a cloud
    // a metre and a half across, ten ahead
    let quads: Vec<Quad> = (0..126)
        .map(|k| {
            let f = k as f32;
            let c = [AHEAD.x + (f * 0.37).sin() * 0.8, AHEAD.y + (f * 0.53).cos() * 0.6, AHEAD.z + (f * 0.29).sin() * 0.8];
            Quad { center: c, half: [0.4 + (k % 8) as f32 * 0.05; 2], frame: 0, color: [1.0, 0.1, 0.1, 0.25 - (k % 6) as f32 * 0.025] }
        })
        .collect();
    let batches = [Batch { texture: 7, blend: 3, ztest: true, zwrite: false, quads }];
    let target = g.device.create_texture(&wgpu::TextureDescriptor {
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
    let depth = render::depth_view(&g.device, w, h);
    let textures = |id: i32| texture(id);
    let time = |batches: &[Batch], ps2: bool| -> f64 {
        let mut r = renderer(&g, ps2);
        let mut best = f64::MAX;
        for _ in 0..5 {
            let start = std::time::Instant::now();
            for _ in 0..100 {
                let mut enc = g.device.create_command_encoder(&Default::default());
                r.draw(&g.queue, &mut enc, &tv, &depth, &view, &[]);
                r.draw_billboards(&g.device, &g.queue, &mut enc, &tv, &depth, v, p, batches, &textures);
                g.queue.submit([enc.finish()]);
                g.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
            }
            best = best.min(start.elapsed().as_secs_f64() * 1000.0 / 100.0);
        }
        best
    };
    let (none, exact, ps2) = (time(&[], false), time(&batches, false), time(&batches, true));
    eprintln!("a frame: no particles {none:.3} ms; the swarm as it is +{:.3} ms; as a PS2 frame keeps it +{:.3} ms", exact - none, ps2 - none);
}
