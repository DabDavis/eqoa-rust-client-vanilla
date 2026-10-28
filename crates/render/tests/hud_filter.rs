//! The options' HUD (`ui::HudFilter`) on the 2D layer's textures other than
//! the fonts. Nearest, the game's, takes the nearest texel, so at 2.25 pixels
//! a texel some texels are two pixels wide and some three; Sharp, as Text's
//! Sharp draws a font, lights every texel as wide as every other and is the
//! nearest texel at a whole scale; it keeps a quad to its own cell of an
//! atlas, a solid fill to its one texel, and mixes a clear texel in by its
//! clearness alone (the art's clear texels are often black, and mixed in as
//! they are they darkened the edge beside them); and it is no font's (Text
//! draws those). Needs a GPU adapter (any backend); without one it says so
//! and passes.

use render::ui::{self, HudFilter, Quad, TextFilter, Ui};
use render::{Renderer, View};
use std::sync::Mutex;

/// The filters are the process's: one test sets them at a time, and makes
/// its device while it holds them (on GL, five tests' contexts made at once
/// hung the run now and then).
static FILTER: Mutex<()> = Mutex::new(());

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return None;
    };
    Some(pollster::block_on(adapter.request_device(&Default::default())).unwrap())
}

/// 16 by 21, opaque: one-texel columns, white at even x and black at odd.
fn stripes() -> Vec<u8> {
    (0..16 * 21).flat_map(|i| if i % 16 % 2 == 0 { [255; 4] } else { [0, 0, 0, 255] }).collect()
}

/// The quad of texels `u0..u1` by rows 0..21 of a 16-wide texture at frame
/// (`x`, 100), a frame pixel a texel, blended as the HUD's are.
fn quad(tex: usize, x: f32, u0: f32, u1: f32) -> Quad {
    let w = u1 - u0;
    Quad { tex, blend: 3, pos: [[x, 100.0], [x + w, 100.0], [x + w, 121.0], [x, 121.0]], uv: [[u0 / 16.0, 0.0], [u1 / 16.0, 0.0], [u1 / 16.0, 1.0], [u0 / 16.0, 1.0]], color: [1.0; 4], clip: None }
}

/// A `size` target, cleared to `clear`, with `quads` of a 16x21 `rgba` drawn
/// under `filter` (and `font` as the font's 'A' at frame (103, 140)). Its RGBA.
fn drawn(device: &wgpu::Device, queue: &wgpu::Queue, size: (u32, u32), filter: HudFilter, rgba: &[u8], clear: glam::Vec3, quads: impl Fn(usize) -> Vec<Quad>, font: bool) -> Vec<u8> {
    ui::set_hud_filter(filter);
    ui::set_text_filter(TextFilter::Sharp);
    let mut r = Renderer::new(device, queue, render::TARGET_FORMAT);
    let mut u = Ui::new(device, render::TARGET_FORMAT, render::DEPTH_FORMAT);
    let tex = u.add_texture(device, queue, 16, 21, rgba);
    u.begin(size);
    for q in quads(tex) {
        u.push(q);
    }
    if font {
        let f = u.add_font(device, queue, &esf::font::Font { id: 1, height: 21, glyphs: vec![esf::font::Glyph { code: b'A' as u16, width: 16, rgba: rgba.to_vec() }] });
        u.text(f, 103, 140, [1.0; 4], &[b'A' as u16]);
    }
    r.ui = Some(u);
    let view = View { clear, ..View::daylight(glam::Mat4::IDENTITY) };
    let px = render::capture(device, queue, &mut r, size.0, size.1, &view, &[], None);
    ui::set_hud_filter(HudFilter::Nearest);
    px
}

/// The stripes at frame (303, 100) over black.
fn striped(device: &wgpu::Device, queue: &wgpu::Queue, size: (u32, u32), filter: HudFilter) -> Vec<u8> {
    drawn(device, queue, size, filter, &stripes(), glam::Vec3::ZERO, |tex| vec![quad(tex, 303.0, 0.0, 16.0)], false)
}

/// The red of the target row through frame y `fy`, from a pixel left of
/// frame x `x0` to one right of `x0 + 16`.
fn row(px: &[u8], size: (u32, u32), x0: f32, fy: f32) -> Vec<u8> {
    let (a, b) = (ui::window_from_frame(size, [x0, fy]), ui::window_from_frame(size, [x0 + 16.0, fy]));
    let y = a[1] as usize;
    (a[0] as usize - 1..=b[0] as usize + 1).map(|x| px[(y * size.0 as usize + x) * 4]).collect()
}

/// Each run of lit pixels' summed light, in whole pixels' worth: one a white
/// column.
fn columns(row: &[u8]) -> Vec<f32> {
    row.split(|&v| v == 0).filter(|r| !r.is_empty()).map(|r| r.iter().map(|&v| v as f32 / 255.0).sum()).collect()
}

/// 1920x1080 is 2.25 pixels a texel across: every white column's light under
/// Sharp is 2.25 pixels' worth, its width, but the first's, whose left edge
/// is the quad's: a pixel there is drawn or not as its centre falls (the
/// rasteriser's, as under Nearest; a font's quad is drawn a texel wider, onto
/// its clear ring, and an atlas's cell has no such ring; the Render Scale
/// smooths it). The control: Nearest lights whole pixels, two or three as the
/// column falls.
#[test]
fn sharp_draws_every_texel_as_wide_as_every_other() {
    let _one = FILTER.lock().unwrap_or_else(|e| e.into_inner());
    let Some((device, queue)) = device() else { return };
    let size = (1920, 1080);
    let sharp = columns(&row(&striped(&device, &queue, size, HudFilter::Sharp), size, 303.0, 110.5));
    assert_eq!(sharp.len(), 8, "eight white columns ({sharp:?})");
    assert!((sharp[0] - sharp[0].round()).abs() < 0.01, "the quad's edge column: whole pixels ({sharp:?})");
    for (k, w) in sharp.iter().enumerate().skip(1) {
        assert!((w - 2.25).abs() < 0.05, "column {k}: {w} pixels' light, want 2.25 ({sharp:?})");
    }
    let nearest = columns(&row(&striped(&device, &queue, size, HudFilter::Nearest), size, 303.0, 110.5));
    assert_eq!(nearest.len(), 8);
    assert!(nearest.iter().all(|w| (w - w.round()).abs() < 0.01) && nearest.iter().any(|w| (w - 2.25).abs() > 0.2), "the control: nearest lights whole pixels ({nearest:?})");
}

/// 1280x960 is two pixels a frame pixel: Sharp is the nearest texel, the
/// whole quad pixel for pixel. The control: at 2.25 it is not.
#[test]
fn sharp_is_the_nearest_texel_at_a_whole_scale() {
    let _one = FILTER.lock().unwrap_or_else(|e| e.into_inner());
    let Some((device, queue)) = device() else { return };
    let size = (1280, 960);
    assert!(striped(&device, &queue, size, HudFilter::Sharp) == striped(&device, &queue, size, HudFilter::Nearest), "every pixel the nearest texel's");
    let size = (1920, 1080);
    assert!(striped(&device, &queue, size, HudFilter::Sharp) != striped(&device, &queue, size, HudFilter::Nearest), "the control");
}

/// The layer's repeat under Sharp: a quad tiling a texture past its edges,
/// below 0 as past 1 (u -1 to 1.5 and v -10/21 to 1, each from a texel's
/// edge as an atlas's cell starts, a texel a frame pixel). Where a frame
/// pixel is whole pixels across (1280x960, two; the frame is drawn 4:3, so
/// 2.14 down) Sharp is the nearest texel, which Nearest's sampler wraps
/// itself, so a texture that changes column by column is drawn pixel for
/// pixel as Nearest draws it; where it is whole pixels down (1280x896, two),
/// one that changes row by row (ui.wgsl's `wrap`; GL's integer `%`, which it
/// had, is undefined below 0). The control: left of u 0, and above v 0, the
/// quad shows each column and each row, where a wrap that clamped would show
/// the first all along.
#[test]
fn sharp_wraps_as_the_layers_repeat_does() {
    let _one = FILTER.lock().unwrap_or_else(|e| e.into_inner());
    let Some((device, queue)) = device() else { return };
    let tiled = |tex: usize| vec![Quad { tex, blend: 3, pos: [[303.0, 100.0], [343.0, 100.0], [343.0, 131.0], [303.0, 131.0]], uv: [[-1.0, -10.0 / 21.0], [1.5, -10.0 / 21.0], [1.5, 1.0], [-1.0, 1.0]], color: [1.0; 4], clip: None }];
    for (size, across) in [((1280, 960), true), ((1280, 896), false)] {
        // red by column, or by row, the other way alike
        let rgba: Vec<u8> = (0..16 * 21).map(|i| if across { i % 16 } else { i / 16 }).flat_map(|k| [(k * 12) as u8, 255 - (k * 12) as u8, 128, 255]).collect();
        let sharp = drawn(&device, &queue, size, HudFilter::Sharp, &rgba, glam::Vec3::ZERO, tiled, false);
        let nearest = drawn(&device, &queue, size, HudFilter::Nearest, &rgba, glam::Vec3::ZERO, tiled, false);
        assert!(sharp == nearest, "{size:?}: the nearest texel, wrapped as the sampler wraps it");
        // the 16 columns left of u 0, or the 10 whole rows above v 0
        let at: Vec<[f32; 2]> = if across { (0..16).map(|c| [303.5 + c as f32, 115.5]).collect() } else { (0..10).map(|r| [320.5, 100.5 + r as f32]).collect() };
        let reds: std::collections::BTreeSet<u8> = at
            .iter()
            .map(|&f| {
                let p = ui::window_from_frame(size, f);
                sharp[(p[1] as usize * size.0 as usize + p[0] as usize) * 4]
            })
            .collect();
        assert_eq!(reds.len(), at.len(), "{size:?}: the control, every one of them ({reds:?})");
    }
}

/// A cell of an atlas with a black column on each side of it, its quad (16
/// texels over 15.5 frame pixels at 1918x1078, its edges anywhere on a
/// pixel) mirrored as a sprite sailing right is: under Sharp no pixel of it
/// takes the black past its cell, and a solid fill (one uv, as `SolidFill`
/// draws) is its texel whole. The control: the quad one texel wider takes
/// the black.
#[test]
fn sharp_keeps_a_quad_to_its_cell_and_a_fill_to_its_texel() {
    let _one = FILTER.lock().unwrap_or_else(|e| e.into_inner());
    let Some((device, queue)) = device() else { return };
    // white in columns 1..15, black at 0 and 15
    let rgba: Vec<u8> = (0..16 * 21).flat_map(|i| if (1..15).contains(&(i % 16)) { [255; 4] } else { [0, 0, 0, 255] }).collect();
    let size = (1918, 1078);
    let mirrored = |tex: usize, x: f32, u0: f32, u1: f32| {
        let mut q = quad(tex, x, u0, u1);
        q.pos[1][0] = x + 15.5;
        q.pos[2][0] = x + 15.5;
        q.uv = [q.uv[1], q.uv[0], q.uv[3], q.uv[2]];
        q
    };
    let fill = |tex: usize| Quad { tex, blend: 3, pos: [[500.0, 100.0], [530.3, 100.0], [530.3, 121.0], [500.0, 121.0]], uv: [[3.0 / 16.0, 0.0]; 4], color: [1.0; 4], clip: None };
    let green = glam::Vec3::new(0.0, 1.0, 0.0);
    let px = drawn(&device, &queue, size, HudFilter::Sharp, &rgba, green, |tex| vec![mirrored(tex, 300.2, 1.0, 15.0), mirrored(tex, 400.7, 0.0, 15.0), fill(tex)], false);
    let at = |fx: f32, fy: f32| {
        let p = ui::window_from_frame(size, [fx, fy]);
        let i = (p[1] as usize * size.0 as usize + p[0] as usize) * 4;
        [px[i], px[i + 1], px[i + 2]]
    };
    let (a, b) = (ui::window_from_frame(size, [300.2, 110.5]), ui::window_from_frame(size, [315.7, 110.5]));
    for x in a[0].ceil() as usize..b[0].floor() as usize {
        let i = (a[1] as usize * size.0 as usize + x) * 4;
        assert_eq!(px[i..i + 3], [255, 255, 255], "column {x}: the cell's white, never the black past it");
    }
    assert_eq!(at(515.0, 110.5), [255, 255, 255], "the fill: its texel, whole");
    let (a, b) = (ui::window_from_frame(size, [400.7, 110.5]), ui::window_from_frame(size, [416.2, 110.5]));
    let dark = (a[0].ceil() as usize..b[0].floor() as usize).any(|x| px[(a[1] as usize * size.0 as usize + x) * 4] < 128);
    assert!(dark, "the control: a quad over the black column shows it");
}

/// Red ink beside clear black texels over white, at 2.25 pixels a texel:
/// under Sharp every pixel is the red over the white by the mix's alpha, its
/// red full (the clear texels' black never in it), and the edges are mixes.
/// The control: Nearest has no mixes at all.
#[test]
fn sharp_mixes_a_clear_texel_in_by_its_clearness_alone() {
    let _one = FILTER.lock().unwrap_or_else(|e| e.into_inner());
    let Some((device, queue)) = device() else { return };
    let rgba: Vec<u8> = (0..16 * 21).flat_map(|i| if i % 16 % 4 < 2 { [255, 0, 0, 255] } else { [0, 0, 0, 0] }).collect();
    let size = (1920, 1080);
    let white = glam::Vec3::ONE;
    let px = drawn(&device, &queue, size, HudFilter::Sharp, &rgba, white, |tex| vec![quad(tex, 303.0, 0.0, 16.0)], false);
    let (a, b) = (ui::window_from_frame(size, [303.0, 110.5]), ui::window_from_frame(size, [319.0, 110.5]));
    let y = a[1] as usize;
    let row: Vec<[u8; 3]> = (a[0] as usize..b[0] as usize).map(|x| {
        let i = (y * size.0 as usize + x) * 4;
        [px[i], px[i + 1], px[i + 2]]
    }).collect();
    assert!(row.iter().all(|p| p[0] >= 254 && p[1] == p[2]), "red over white, never darker: {row:?}");
    assert!(row.iter().any(|p| p[1] > 8 && p[1] < 247), "edges mixed: {row:?}");
    let nearest = drawn(&device, &queue, size, HudFilter::Nearest, &rgba, white, |tex| vec![quad(tex, 303.0, 0.0, 16.0)], false);
    assert!((a[0] as usize..b[0] as usize).all(|x| matches!(nearest[(y * size.0 as usize + x) * 4 + 1], 0 | 255)), "the control: nearest mixes none");
}

/// A font is Text's whatever the HUD's filter: the stripes as 'A' drawn alike
/// under both, where the surface beside it is not.
#[test]
fn a_font_is_texts_whatever_the_huds() {
    let _one = FILTER.lock().unwrap_or_else(|e| e.into_inner());
    let Some((device, queue)) = device() else { return };
    let size = (1920, 1080);
    let draw = |f| drawn(&device, &queue, size, f, &stripes(), glam::Vec3::ZERO, |tex| vec![quad(tex, 303.0, 0.0, 16.0)], true);
    let (sharp, nearest) = (draw(HudFilter::Sharp), draw(HudFilter::Nearest));
    assert_eq!(row(&sharp, size, 103.0, 150.5), row(&nearest, size, 103.0, 150.5), "the font's row");
    assert_ne!(row(&sharp, size, 303.0, 110.5), row(&nearest, size, 303.0, 110.5), "the control: the surface's");
}
