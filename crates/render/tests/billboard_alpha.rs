//! A particle's pixel against the GS's arithmetic. The billboard program
//! (decomp/vu: VIBbRender) sends a particle's colour as rgb × 128 and its
//! alpha as a × 64 when the layer blends (a × 128 when it alpha-tests:
//! `BeginBillboards`' alpha scale), MODULATE makes the texel's colour Ct ×
//! that / 128 and As = At × that / 128 (At the texel's 0..255), and ALPHA_1
//! takes As / 128 as its factor (0x80 is one): a blended particle covers
//! with its texel's alpha times its own, as a blended model layer does (the
//! layer emitter's alpha is × 64 too, `ConstructTextPass`), and an opaque one
//! passes the test at As ≥ 128. The 64 keeps a texel of 255 from blending
//! twice over; it is not a half, and drawn as one every particle, every glow
//! an epic weapon's sprite carries, the weather, the stars and the spells'
//! bolts and trails covered half what the game's do. Needs a graphics
//! adapter (any backend); without one it says so and passes.

use glam::{Mat4, Vec3};
use render::billboard::{Batch, Quad, Strips};
use render::{Draw, Renderer, View};

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

/// The texel every draw here samples: (200, 100, 50) at alpha 128 of 255.
const TEXEL: [u8; 4] = [200, 100, 50, 128];

const EYE: Vec3 = Vec3::new(25000.0, 60.0, 15000.0);
const AHEAD: Vec3 = Vec3::new(25000.0, 60.0, 15010.0);

/// The GS's pixel for a texel of `TEXEL` modulated by a vertex colour
/// `cf` (0..255, 0x80 one) and blended by `mode` over `cd`, all in integers
/// as the GS does them: Cv = Ct × Cf / 128 and As = At × Af / 128 (each
/// clamped to 255), then 1 Cd + Cv × As / 128, 3 (Cv − Cd) × As / 128 + Cd.
fn gs(cf: [u32; 4], mode: usize, cd: [f32; 3]) -> [f32; 3] {
    let cv: Vec<f32> = (0..3).map(|k| ((TEXEL[k] as u32 * cf[k]) >> 7).min(255) as f32).collect();
    let a = ((TEXEL[3] as u32 * cf[3]) >> 7).min(255) as f32 / 128.0;
    let px = |k: usize| match mode {
        1 => cd[k] + cv[k] * a,
        3 => (cv[k] - cd[k]) * a + cd[k],
        _ => unreachable!(),
    };
    [px(0).min(255.0), px(1).min(255.0), px(2).min(255.0)]
}

/// The billboard program's vertex colour for a particle's (rgb × 128, a ×
/// the alpha scale), as ftoi0 leaves it.
fn billboard_cf(c: [f32; 4], scale: f32) -> [u32; 4] {
    [(c[0] * 128.0) as u32, (c[1] * 128.0) as u32, (c[2] * 128.0) as u32, (c[3] * scale) as u32]
}

fn texture(id: i32) -> Option<esf::Image> {
    (id == 7).then(|| esf::Image { width: 1, height: 1, rgba: TEXEL.to_vec(), alpha: true })
}

fn view() -> (Mat4, Mat4, View) {
    let v = glam::camera::rh::view::look_at_mat4(EYE, AHEAD, Vec3::Y);
    let p: Mat4 = glam::camera::rh::proj::directx::perspective(1.0, 1.0, 0.5, 1000.0);
    (v, p, View { sun: Vec3::ZERO, ambient: Vec3::ZERO, ..View::daylight(p * v) })
}

/// The middle pixel of a 4x4 frame: the clear, with one particle of colour
/// `c` over the whole view, blended by `mode`.
fn particle(g: &Gpu, c: [f32; 4], mode: usize) -> [f32; 3] {
    let (v, p, view) = view();
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let quad = Quad { center: [AHEAD.x, AHEAD.y, AHEAD.z], half: [40.0, 40.0], frame: 0, color: c };
    let batches = [Batch { texture: 7, blend: mode, ztest: true, zwrite: mode == 0, quads: vec![quad] }];
    let px = render::capture(&g.device, &g.queue, &mut r, 4, 4, &view, &[], Some((v, p, &batches, &texture)));
    let at = (2 * 4 + 2) * 4;
    [px[at] as f32, px[at + 1] as f32, px[at + 2] as f32]
}

/// The same frame with a strip (a spell's bolt, a weapon's trail, which the
/// renderer draws by the billboards' program) of colour `c` over the whole
/// view instead.
fn strip(g: &Gpu, c: [f32; 4], mode: usize) -> [f32; 3] {
    let (v, p, view) = view();
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let at = |x: f32, y: f32| ([0.5, 0.5], [AHEAD.x + x, AHEAD.y + y, AHEAD.z]);
    r.strips = vec![Strips { texture: 7, blend: mode, ztest: true, zwrite: false, color: c, strips: vec![vec![at(-40.0, -40.0), at(-40.0, 40.0), at(40.0, -40.0), at(40.0, 40.0)]], colors: Vec::new() }];
    let px = render::capture(&g.device, &g.queue, &mut r, 4, 4, &view, &[], Some((v, p, &[], &texture)));
    let at = (2 * 4 + 2) * 4;
    [px[at] as f32, px[at + 1] as f32, px[at + 2] as f32]
}

/// The same frame with a model's blended layer over the whole view, lit to
/// one (white vertices), the layer colour white at alpha 255.
fn layer(g: &Gpu, mode: usize) -> [f32; 3] {
    let (_, _, view) = view();
    let view = View { view_proj: Mat4::IDENTITY, ..view };
    let mut r = Renderer::new(&g.device, &g.queue, render::TARGET_FORMAT);
    let layer = esf::Layer { flags: 0, texture: 7, wrap: 0, blend: mode as i32, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2] };
    let model = esf::Model {
        pos: vec![[-3.0, -3.0, 0.5], [3.0, -3.0, 0.5], [0.0, 3.0, 0.5]],
        uv: vec![[0.5, 0.5]; 3],
        normal: vec![[0.0, 0.0, 1.0]; 3],
        color: vec![[1.0, 1.0, 1.0, 1.0]; 3],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2], lod: None }],
        ..Default::default()
    };
    let m = r.upload(&g.device, &g.queue, &model, &[render::IDENTITY], &texture);
    let px = render::capture(&g.device, &g.queue, &mut r, 4, 4, &view, &[Draw::Model(&m)], None);
    let at = (2 * 4 + 2) * 4;
    [px[at] as f32, px[at + 1] as f32, px[at + 2] as f32]
}

fn close(got: [f32; 3], want: [f32; 3], what: &str) {
    assert!((0..3).all(|k| (got[k] - want[k]).abs() <= 2.0), "{what}: {got:?}, want {want:?}");
}

/// The rule the billboards were drawn by: the alpha scale's 64 read as a
/// half on top of the texel's alpha. A test that cannot tell it from the
/// GS's proves nothing, so each case checks the old pixel differs.
fn differs(got: [f32; 3], old: [f32; 3], what: &str) {
    assert!((0..3).any(|k| (got[k] - old[k]).abs() > 4.0), "{what}: the control must differ: {got:?} against {old:?}");
}

#[test]
fn a_particle_blends_by_its_texels_alpha_times_its_own() {
    let Some(g) = gpu() else { return };
    let (_, _, view) = view();
    let cd = [view.clear.x * 255.0, view.clear.y * 255.0, view.clear.z * 255.0];
    let clear = particle(&g, [0.0; 4], 1);
    close(clear, cd.map(|c| c.round()), "nothing added");
    let t = |k: usize| TEXEL[k] as f32;
    let ta = TEXEL[3] as f32 / 255.0;

    // added (mode 1): white at the particle's full alpha adds the texel at
    // As / 128 = 128 × 64 / 128 / 128, a half: the texel's own alpha
    let white = [1.0, 1.0, 1.0, 1.0];
    let got = particle(&g, white, 1);
    close(got, gs(billboard_cf(white, 64.0), 1, cd), "added, white");
    differs(got, [0, 1, 2].map(|k| cd[k] + t(k) * ta * 0.5), "added, white");

    // a coloured particle at half its alpha: the colour modulates the texel
    // (× 128), the alpha a quarter
    let tinted = [0.5, 1.0, 0.25, 0.5];
    let got = particle(&g, tinted, 1);
    close(got, gs(billboard_cf(tinted, 64.0), 1, cd), "added, tinted");
    differs(got, [0, 1, 2].map(|k| cd[k] + t(k) * tinted[k] * ta * 0.5 * 0.5), "added, tinted");

    // alpha-blended (mode 3) over the clear
    let got = particle(&g, white, 3);
    close(got, gs(billboard_cf(white, 64.0), 3, cd), "blended");
    differs(got, [0, 1, 2].map(|k| t(k) * ta * 0.5 + cd[k] * (1.0 - ta * 0.5)), "blended");

    // a bolt or a trail (VIRStrip and the layer emitter on the console: the
    // colour × 128 and the alpha × 64 again), the same
    close(strip(&g, white, 1), gs(billboard_cf(white, 64.0), 1, cd), "a strip, added");
    close(strip(&g, tinted, 3), gs(billboard_cf(tinted, 64.0), 3, cd), "a strip, blended");

    // and as much as a model's blended layer of the same texel adds, whose
    // layer colour the emitter sends × 128 and its alpha × 64
    close(layer(&g, 1), particle(&g, white, 1), "a layer and a particle, added");
    close(layer(&g, 3), particle(&g, white, 3), "a layer and a particle, blended");

    // an opaque particle (mode 0) is alpha-tested at As ≥ 128 with its alpha
    // × 128: the texel's 128 at the particle's full alpha passes, drawn as
    // the texel, and at 0.9 (115) does not
    close(particle(&g, white, 0), [t(0), t(1), t(2)], "opaque, passed");
    close(particle(&g, [1.0, 1.0, 1.0, 0.9], 0), clear, "opaque, failed");
}
