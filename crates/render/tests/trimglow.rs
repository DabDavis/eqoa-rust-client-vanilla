//! The armour trim's aura (`render::trimglow`) and a dark aura's shell
//! (`render::shell`, `PartOf::dark`), drawn on a sphere whose texture has a
//! gold band round its equator (the trim) on grey steel. A trim aura lights
//! the band and nothing far from it, in one draw call more; a dark trim
//! darkens the band; taking it away leaves the picture bit for bit as before.
//! A dark shell darkens about the silhouette against a bright background and
//! adds no light there. Needs a GPU adapter (any backend); without one it says
//! so and passes.

use render::{Draw, Renderer, TextureSource, View};

fn camera(z: f32, fovy: f32, aspect: f32) -> glam::Mat4 {
    let (near, far) = (0.1f32, 100.0f32);
    let f = 1.0 / (fovy / 2.0).tan();
    let proj = glam::Mat4::from_cols_array_2d(&[[f / aspect, 0.0, 0.0, 0.0], [0.0, f, 0.0, 0.0], [0.0, 0.0, far / (near - far), -1.0], [0.0, 0.0, near * far / (near - far), 0.0]]);
    proj * glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, -z))
}

/// A unit sphere at the origin, textured with `texture` (u round it, v from
/// its top to its bottom).
fn sphere(texture: i32) -> esf::Model {
    let (rings, segs) = (24u32, 32u32);
    let mut m = esf::Model::default();
    for i in 0..=rings {
        let th = std::f32::consts::PI * i as f32 / rings as f32;
        for j in 0..=segs {
            let ph = 2.0 * std::f32::consts::PI * j as f32 / segs as f32;
            let n = [th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()];
            m.pos.push(n);
            m.normal.push(n);
            m.uv.push([j as f32 / segs as f32, i as f32 / rings as f32]);
            m.color.push([0.0, 0.0, 0.0, 1.0]);
        }
    }
    let mut idx = Vec::new();
    for i in 0..rings {
        for j in 0..segs {
            let a = i * (segs + 1) + j;
            let b = a + segs + 1;
            idx.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
        }
    }
    let layer = esf::Layer { flags: 0, texture, wrap: 0, blend: 0, color: [128, 128, 128, 255], uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0; 2], emissive: [0; 4] };
    m.batches.push(esf::Batch { material: 0, layers: vec![layer], indices: idx, lod: None });
    m
}

const PLATE: i32 = 0x5151;

/// The plate's texture: grey steel with a gold band across v 0.44..0.56.
fn plate() -> esf::Image {
    let (w, h) = (64u32, 64u32);
    let mut rgba = Vec::new();
    for y in 0..h {
        for _ in 0..w {
            let gold = (28..36).contains(&y);
            rgba.extend_from_slice(&if gold { [210, 160, 50, 255] } else { [120, 124, 130, 255] });
        }
    }
    esf::Image { width: w, height: h, rgba, alpha: false }
}

/// The plate and its trim's mask, as `LibraryTextures` resolves them.
struct Plate(i32);

impl TextureSource for Plate {
    fn image(&self, id: i32) -> Option<esf::Image> {
        let p = plate();
        if id == PLATE {
            return Some(p);
        }
        (id == self.0).then(|| esf::Image { rgba: render::recolor::trim_mask(&p.rgba, p.width, p.height), ..p })
    }
}

#[test]
fn a_trim_lights_its_band_and_a_dark_aura_darkens() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return;
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut r = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    r.scroll_time = Some(1.0);
    let mask = render::recolor::recolored(PLATE, render::recolor::TRIM_MASK << 24, |_| false);
    let tex = Plate(mask);
    let body = sphere(PLATE);
    let mut m = r.upload(&device, &queue, &body, &[render::IDENTITY], &tex);
    let n = 64u32;
    let vp = camera(4.0, 0.9, 1.0);
    let view = View { eye: glam::Vec3::new(0.0, 0.0, 4.0), clear: glam::Vec3::splat(0.75), ..View::daylight(vp) };
    let shot = |r: &mut Renderer, m: &render::GpuModel| (render::capture(&device, &queue, r, n, n, &view, &[Draw::Model(m)], None), r.drawn.get().1);
    let (before, draws) = shot(&mut r, &m);
    let at = |x: u32, y: u32| ((y * n + x) * 4) as usize;
    let lum = |px: &[u8], x: u32, y: u32| {
        let i = at(x, y);
        px[i] as i32 + px[i + 1] as i32 + px[i + 2] as i32
    };
    let band = |px: &[u8]| (24..40).map(|x| lum(px, x, n / 2)).sum::<i32>();
    let pole = |px: &[u8]| (28..36).map(|x| lum(px, x, n / 2 - 15)).sum::<i32>();

    // a glowing gold trim
    let trims = [render::trimglow::TrimOf { color: [255, 190, 60], dark: false, batches: vec![(0, mask)] }];
    r.set_trims(&device, &queue, &mut m, &body, &trims, &tex);
    assert_eq!(m.trim.as_ref().map(|t| t.parts()), Some(1));
    let (lit, draws_lit) = shot(&mut r, &m);
    assert_eq!(draws_lit, draws + 1, "one draw call more");
    eprintln!("band {} -> {}, pole {} -> {}", band(&before), band(&lit), pole(&before), pole(&lit));
    assert!(band(&lit) > band(&before) + 16 * 60, "the band lit");
    assert_eq!(pole(&lit), pole(&before), "far from the trim: nothing");
    for y in 0..n {
        for x in 0..n {
            let i = at(x, y);
            assert!(lit[i] >= before[i] && lit[i + 1] >= before[i + 1], "a glow only adds, at {x},{y}");
        }
    }

    // the same in black: the band darkened
    let dark = [render::trimglow::TrimOf { color: [0, 0, 0], dark: true, batches: vec![(0, mask)] }];
    r.set_trims(&device, &queue, &mut m, &body, &dark, &tex);
    let (shadow, _) = shot(&mut r, &m);
    eprintln!("dark band {} -> {}", band(&before), band(&shadow));
    assert!(band(&shadow) < band(&before) * 6 / 10, "the band darkened");
    assert_eq!(pole(&shadow), pole(&before), "far from the trim: nothing");

    // taken away: bit for bit as before; a trim of no batch is none
    r.set_trims(&device, &queue, &mut m, &body, &[], &tex);
    assert!(m.trim.is_none());
    let (after, draws_after) = shot(&mut r, &m);
    assert_eq!(draws_after, draws);
    assert!(after == before, "no trim: bit for bit as before");
    r.set_trims(&device, &queue, &mut m, &body, &[render::trimglow::TrimOf { color: [255; 3], dark: false, batches: vec![(0, 0x7777)] }], &tex);
    assert!(m.trim.is_none(), "a mask that resolves to nothing: none");

    // a dark shell against the bright background: about the silhouette
    // darker, nowhere much lighter
    r.set_shells(&device, &mut m, &body, &[render::shell::PartOf { color: [0, 0, 0], batches: None, dark: true }]);
    let (smoke, draws_smoke) = shot(&mut r, &m);
    assert_eq!(draws_smoke, draws + 1);
    let mut darker = 0;
    for y in 0..n {
        for x in 0..n {
            let (b, s) = (lum(&before, x, y), lum(&smoke, x, y));
            assert!(s <= b + 24, "a shadow adds little light: {b} -> {s} at {x},{y}");
            darker += (s + 30 < b) as u32;
        }
    }
    eprintln!("{darker} pixels darker");
    assert!(darker > 100, "{darker} pixels darker");
}
