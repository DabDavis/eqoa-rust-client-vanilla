//! The options' Textures, Crisp (`render::set_crisp`): a solid layer's colour
//! from the GS's level "sharp bilinear", each magnified texel flat with a
//! pixel's blend where its edge falls, where Smooth (the game's) mixes it
//! across its whole width; at the level the GS picks by depth; and nothing
//! changed where the filter is what draws it properly: the ground's picture,
//! a blended layer, a scrolling one, the sky, a minified texture and every
//! texture's alpha. Needs a GPU adapter (any backend); without one it says so
//! and passes.

use render::{Draw, Renderer, TextureSource, View};
use std::sync::Mutex;

/// The switch is the process's: one test sets it at a time.
static CRISP: Mutex<()> = Mutex::new(());

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return None;
    };
    Some(pollster::block_on(adapter.request_device(&Default::default())).unwrap())
}

/// Column `c`'s colour at level `l`: every column of a level its own, and
/// every level its own.
fn colour(l: usize, c: u32) -> [u8; 4] {
    let x = c as u8;
    match l {
        0 => [20 + x * 29, 230 - x * 23, (c * 71 % 251) as u8, 255],
        1 => [255 - x * 40, 40 + x * 50, 200, 255],
        _ => [x * 100 + 30, 10, 255 - x * 90, 255],
    }
}

/// Texture 9's alpha by column: clear, then a step and a ramp, then opaque.
const RAMP: [u8; 8] = [0, 0, 0, 100, 200, 255, 255, 255];

/// Texture 7: three levels, 8, 4 and 2 a side, each column its colour
/// (`colour`), every row alike. Texture 8: one level 256 a side, texture 7's
/// eight base columns over and over (minified in the view). Texture 9: texture
/// 7's base with `RAMP`'s alpha (a cut-out).
struct Columns;

impl TextureSource for Columns {
    fn image(&self, id: i32) -> Option<esf::Image> {
        self.levels(id).into_iter().next()
    }
    fn levels(&self, id: i32) -> Vec<esf::Image> {
        let level = |l: usize, side: u32, ramp: bool| {
            let rgba = (0..side * side)
                .flat_map(|i| {
                    let c = i % side % 8;
                    let mut p = colour(l, c);
                    if ramp {
                        p[3] = RAMP[c as usize];
                    }
                    p
                })
                .collect();
            esf::Image { width: side, height: side, rgba, alpha: ramp }
        };
        match id {
            7 => vec![level(0, 8, false), level(1, 4, false), level(2, 2, false)],
            8 => vec![level(0, 256, false)],
            9 => vec![level(0, 8, true)],
            _ => Vec::new(),
        }
    }
}

/// How the square's layer is drawn: its texture, blend mode and scroll, and
/// whether the model is the ground or the sky.
#[derive(Clone, Copy)]
struct Look {
    texture: i32,
    blend: i32,
    scroll: [f32; 2],
    terrain: bool,
    sky: bool,
}

const SOLID: Look = Look { texture: 7, blend: 0, scroll: [0.0; 2], terrain: false, sky: false };

/// Across a 62-pixel view: 15.5 pixels a base texel, no whole scale.
const W: u32 = 62;

const CLEAR: glam::Vec3 = glam::Vec3::new(0.1, 0.2, 0.3);

/// A `W` by `W` capture of a square at depth `d` filling a 90-degree view
/// twice over (its uvs 0.25 to 0.75 across it), lit white, its layer's bias
/// 0 (the GS's level log2(d)), crisp or not.
fn shot(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer, look: Look, d: f32, crisp: bool) -> Vec<u8> {
    let layer = esf::Layer { flags: 0, texture: look.texture, wrap: 0, blend: look.blend, color: [255; 4], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.0, uv_rate: look.scroll };
    let s = d * 2.0;
    let model = esf::Model {
        pos: vec![[-s, -s, -d], [s, -s, -d], [s, s, -d], [-s, -s, -d], [s, s, -d], [-s, s, -d]],
        uv: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        normal: vec![[0.0, 0.0, 1.0]; 6],
        color: vec![[1.0, 1.0, 1.0, 1.0]; 6],
        batches: vec![esf::Batch { material: 0, layers: vec![layer], indices: vec![0, 1, 2, 3, 4, 5], lod: None }],
        terrain: look.terrain,
        ..Default::default()
    };
    let mut m = r.upload(device, queue, &model, &[render::IDENTITY], &Columns);
    m.sky = look.sky;
    let proj = glam::camera::rh::proj::directx::perspective(90f32.to_radians(), 1.0, 0.5, 100_000.0);
    let mut view = View { clear: CLEAR, sun: glam::Vec3::ZERO, ambient: glam::Vec3::ONE, ..View::daylight(proj) };
    if look.sky {
        view.sky_view_proj = Some(proj);
    }
    render::set_crisp(crisp);
    let px = render::capture(device, queue, r, W, W, &view, &[Draw::Model(&m)], None);
    render::set_crisp(false);
    px
}

/// The texel column of level `l` a pixel's centre in column `x` falls in,
/// and how far that centre is from the column's nearer edge, in pixels.
fn column(x: u32, l: u32) -> (u32, f32) {
    let cols = (8 >> l) as f32;
    let t = (0.25 + (x as f32 + 0.5) / W as f32 * 0.5) * cols;
    let f = t.fract();
    (t.floor() as u32, f.min(1.0 - f) * W as f32 / (0.5 * cols))
}

/// Pixel `x` of the middle row.
fn rgb(px: &[u8], x: u32) -> [u8; 3] {
    let i = ((W / 2 * W + x) * 4) as usize;
    [px[i], px[i + 1], px[i + 2]]
}

fn setup() -> Option<(wgpu::Device, wgpu::Queue, Renderer)> {
    let (device, queue) = gpu()?;
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    r.scroll_time = Some(0.0);
    Some((device, queue, r))
}

/// Every pixel whose centre is more than half a pixel from its texel's edge
/// is that texel's colour exactly, and of the rest only the pixel each edge
/// falls in mixes two (one at a pixel's centre, at x 15.5 and 46.5; none at
/// a pixel's corner, x 31); the control: Smooth, the same view, mixes nearly
/// every pixel.
#[test]
fn crisp_draws_a_magnified_texel_flat_with_a_pixels_blend_at_its_edge() {
    let _one = CRISP.lock().unwrap_or_else(|e| e.into_inner());
    let Some((device, queue, mut r)) = setup() else { return };
    let crisp = shot(&device, &queue, &mut r, SOLID, 1.0, true);
    let smooth = shot(&device, &queue, &mut r, SOLID, 1.0, false);
    for x in 0..W {
        let (c, edge) = column(x, 0);
        if edge > 0.501 {
            assert_eq!(rgb(&crisp, x), colour(0, c)[..3], "pixel {x}, {edge:.2} from its texel's edge: the texel's own");
        }
    }
    let mixed: Vec<u32> = (0..W).filter(|&x| rgb(&crisp, x) != colour(0, column(x, 0).0)[..3]).collect();
    assert_eq!(mixed, [15, 46], "the pixels an edge falls in the middle of, and no other");
    let exact = (0..W).filter(|&x| rgb(&smooth, x) == colour(0, column(x, 0).0)[..3]).count();
    assert!(exact < 8, "the control: Smooth mixes texels across their width ({exact} of {W} exact)");
}

/// At the level the GS's rule picks: level 1 at depth 2 (bias 0: log2(d)),
/// the last level, 2, at depth 4 and past it; each flat in its own texels.
/// The control: the base's columns are not level 1's.
#[test]
fn crisp_takes_the_level_the_gs_picks() {
    let _one = CRISP.lock().unwrap_or_else(|e| e.into_inner());
    let Some((device, queue, mut r)) = setup() else { return };
    for (d, l) in [(2.0, 1u32), (4.0, 2), (16.0, 2)] {
        let px = shot(&device, &queue, &mut r, SOLID, d, true);
        for x in 0..W {
            let (c, edge) = column(x, l);
            if edge > 0.501 {
                assert_eq!(rgb(&px, x), colour(l as usize, c)[..3], "depth {d}, level {l}, pixel {x}");
            }
        }
    }
    assert_ne!(colour(0, column(20, 0).0), colour(1, column(20, 1).0), "the control: the levels told apart");
}

/// What the filter draws properly keeps it whatever the switch: the ground's
/// picture, a blended layer, a scrolling one, the sky, and a minified texture
/// (a texel under a pixel: the filter's, to a level of rounding). The
/// control: a solid layer's magnified texels change.
#[test]
fn what_needs_the_filter_keeps_it() {
    let _one = CRISP.lock().unwrap_or_else(|e| e.into_inner());
    let Some((device, queue, mut r)) = setup() else { return };
    let cases = [
        (Look { terrain: true, ..SOLID }, "the ground's picture"),
        (Look { blend: 3, ..SOLID }, "a blended layer"),
        (Look { scroll: [0.1, 0.0], ..SOLID }, "a scrolling layer"),
        (Look { sky: true, ..SOLID }, "the sky"),
    ];
    for (look, what) in cases {
        let (on, off) = (shot(&device, &queue, &mut r, look, 1.0, true), shot(&device, &queue, &mut r, look, 1.0, false));
        assert!(off.chunks(4).any(|p| p[..3] != off[..3]), "{what}: drawn");
        assert!(on == off, "{what}: as the filter draws it");
    }
    let minified = Look { texture: 8, ..SOLID };
    let (on, off) = (shot(&device, &queue, &mut r, minified, 1.0, true), shot(&device, &queue, &mut r, minified, 1.0, false));
    assert!(on.iter().zip(&off).all(|(a, b)| a.abs_diff(*b) <= 1), "128 texels over 62 pixels: the filter's");
    assert!(shot(&device, &queue, &mut r, SOLID, 1.0, true) != shot(&device, &queue, &mut r, SOLID, 1.0, false), "the control: a solid layer");
}

/// A cut-out's edge is where the filtered alpha crosses the test's half, as
/// Smooth draws it, between the ramp's columns 3 and 4 (alpha 100 and 200):
/// 0.28 of a texel before their edge, the same pixels kept either way. The
/// control: with the alpha crisp too, the edge would be the texels' own,
/// over three pixels on.
#[test]
fn a_cut_outs_edge_is_the_filters() {
    let _one = CRISP.lock().unwrap_or_else(|e| e.into_inner());
    let Some((device, queue, mut r)) = setup() else { return };
    let cut = Look { texture: 9, ..SOLID };
    let (on, off) = (shot(&device, &queue, &mut r, cut, 1.0, true), shot(&device, &queue, &mut r, cut, 1.0, false));
    let clear = render::capture(&device, &queue, &mut r, W, W, &View { clear: CLEAR, ..View::daylight(glam::Mat4::IDENTITY) }, &[], None);
    let kept = |px: &[u8]| (0..W).map(|x| rgb(px, x) != rgb(&clear, x)).collect::<Vec<bool>>();
    assert_eq!(kept(&on), kept(&off), "the same pixels cut out");
    let first = kept(&on).iter().position(|&k| k).expect("some kept");
    assert!(first > 0, "some cut");
    // the filter's edge: alpha 128 at t = 3.5 + 28/100 (x 27.1); the
    // texels' own at t = 4 (x 30.5)
    let x_at = |t: f32| (t / 8.0 - 0.25) / 0.5 * W as f32 - 0.5;
    assert!((first as f32 - x_at(3.78)).abs() <= 1.0, "the filter's edge: pixel {first}, want about {:.1}", x_at(3.78));
    assert!(x_at(4.0) - first as f32 >= 2.5, "the control: the texels' own edge is past it");
}
