//! The options' Text (`ui::TextFilter`) on a font scaled to the target, the
//! 2D layer's: a glyph of one-texel columns, white and clear by turns, beside
//! the same image as a surface, which the layer samples nearest whatever the
//! filter (the control). Sharp is the nearest texel at a whole scale, near it
//! at a little over one pixel a texel (where nearest is near even), and from
//! two pixels a texel draws every texel as wide as every other: a column's
//! light, summed over the pixels it falls on, is its width in pixels, where
//! the nearest texel alone makes some columns a pixel wider than the rest.
//! Smooth mixes texels across their whole width. Needs a GPU adapter (any
//! backend); without one it says so and passes.

use render::ui::{self, Quad, TextFilter, Ui};
use render::{Renderer, View};
use std::sync::Mutex;

/// The filter is the process's: one test sets it at a time.
static FILTER: Mutex<()> = Mutex::new(());

fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
        eprintln!("SKIP: no graphics adapter");
        return None;
    };
    Some(pollster::block_on(adapter.request_device(&Default::default())).unwrap())
}

/// 16 by 21: one-texel columns, white at even x and clear at odd, over rows
/// 2 to 18, the rest clear.
fn stripes() -> Vec<u8> {
    let mut rgba = vec![0u8; 16 * 21 * 4];
    for y in 2..19 {
        for x in (0..16).step_by(2) {
            rgba[(y * 16 + x) * 4..][..4].copy_from_slice(&[255; 4]);
        }
    }
    rgba
}

/// A `size` target drawn black under `filter`: the stripes as the font's 'A'
/// at frame (103, 100), and as a surface at frame (303, 100). Its RGBA. (At
/// 1920x1080, x 103 puts the glyph's edge at three quarters of a pixel, so a
/// mix there falls partly past it.)
fn drawn(device: &wgpu::Device, queue: &wgpu::Queue, size: (u32, u32), filter: TextFilter) -> Vec<u8> {
    ui::set_text_filter(filter);
    let mut r = Renderer::new(device, queue, render::TARGET_FORMAT);
    let mut u = Ui::new(device, render::TARGET_FORMAT, render::DEPTH_FORMAT);
    let font = esf::font::Font { id: 1, height: 21, glyphs: vec![esf::font::Glyph { code: b'A' as u16, width: 16, rgba: stripes() }] };
    let f = u.add_font(device, queue, &font);
    let tex = u.add_texture(device, queue, 16, 21, &stripes());
    u.begin(size);
    u.text(f, 103, 100, [1.0; 4], &[b'A' as u16]);
    u.push(Quad { tex, blend: 3, pos: [[303.0, 100.0], [319.0, 100.0], [319.0, 121.0], [303.0, 121.0]], uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], color: [1.0; 4], clip: None });
    r.ui = Some(u);
    let view = View { clear: glam::Vec3::ZERO, ..View::daylight(glam::Mat4::IDENTITY) };
    render::capture(device, queue, &mut r, size.0, size.1, &view, &[], None)
}

/// The red of the target row through frame y 110 (the columns' middle),
/// from a pixel left of frame x `x0` to one right of `x0 + 16`.
fn row(px: &[u8], size: (u32, u32), x0: f32) -> Vec<u8> {
    let (a, b) = (ui::window_from_frame(size, [x0, 110.5]), ui::window_from_frame(size, [x0 + 16.0, 110.5]));
    let y = a[1] as usize;
    (a[0] as usize - 1..=b[0] as usize + 1).map(|x| px[(y * size.0 as usize + x) * 4]).collect()
}

/// Each run of lit pixels' summed light, in whole pixels' worth: one a white
/// column.
fn columns(row: &[u8]) -> Vec<f32> {
    row.split(|&v| v == 0).filter(|r| !r.is_empty()).map(|r| r.iter().map(|&v| v as f32 / 255.0).sum()).collect()
}

/// The pixels neither black nor white: a mix of two texels.
fn mixed(row: &[u8]) -> usize {
    row.iter().filter(|&&v| v != 0 && v != 255).count()
}

/// 1280x960 is two pixels a frame pixel across (a 4:3 target's frame is its
/// height's 4:3): Sharp's row is the surface's, the nearest texel's, pixel
/// for pixel. The control: Smooth's is not.
#[test]
fn sharp_is_the_nearest_texel_at_a_whole_scale() {
    let Some((device, queue)) = device() else { return };
    let _one = FILTER.lock().unwrap_or_else(|e| e.into_inner());
    let size = (1280, 960);
    let sharp = drawn(&device, &queue, size, TextFilter::Sharp);
    let (font, surface) = (row(&sharp, size, 103.0), row(&sharp, size, 303.0));
    assert_eq!(mixed(&surface), 0, "the surface is the nearest texel");
    assert_eq!(font, surface, "Sharp at two pixels a texel: the nearest texel");
    let smooth = drawn(&device, &queue, size, TextFilter::Smooth);
    assert_ne!(row(&smooth, size, 103.0), surface, "the control: Smooth mixes them");
    ui::set_text_filter(TextFilter::Sharp);
}

/// 958x501 (two windows side by side) is 1.044 pixels a texel across: Sharp
/// mixes next to no pixels, as nearest mixes none. The control: Smooth mixes
/// most of them.
#[test]
fn sharp_stays_crisp_near_one_pixel_a_texel() {
    let Some((device, queue)) = device() else { return };
    let _one = FILTER.lock().unwrap_or_else(|e| e.into_inner());
    let size = (958, 501);
    let sharp = row(&drawn(&device, &queue, size, TextFilter::Sharp), size, 103.0);
    assert!(mixed(&sharp) <= 2, "Sharp: {} of {} pixels mixed ({sharp:?})", mixed(&sharp), sharp.len());
    let smooth = row(&drawn(&device, &queue, size, TextFilter::Smooth), size, 103.0);
    assert!(mixed(&smooth) * 2 > smooth.len(), "the control: Smooth mixes most ({smooth:?})");
    ui::set_text_filter(TextFilter::Sharp);
}

/// 1920x1080 is 2.25 pixels a texel across: every white column's light under
/// Sharp is 2.25 pixels' worth, its width, the glyph's edge column's too;
/// the control, the nearest texel's surface, lights whole pixels (two or
/// three, as the column falls). The font's clear
/// texels carry its ink's colour (`ui::pack`), so a mix fades the white out
/// rather than darkening it as well (straight alpha would light a column
/// less than its width).
#[test]
fn sharp_draws_every_texel_as_wide_from_two_pixels_a_texel() {
    let Some((device, queue)) = device() else { return };
    let _one = FILTER.lock().unwrap_or_else(|e| e.into_inner());
    let size = (1920, 1080);
    let px = drawn(&device, &queue, size, TextFilter::Sharp);
    let (font, surface) = (columns(&row(&px, size, 103.0)), columns(&row(&px, size, 303.0)));
    assert_eq!(font.len(), 8, "eight white columns ({font:?})");
    for (k, w) in font.iter().enumerate() {
        assert!((w - 2.25).abs() < 0.05, "column {k}: {w} pixels' light, want 2.25 ({font:?})");
    }
    assert_eq!(surface.len(), 8);
    assert!(surface.iter().all(|w| (w - w.round()).abs() < 0.01) && surface.iter().any(|w| (w - 2.25).abs() > 0.2), "the control: nearest lights whole pixels ({surface:?})");
    ui::set_text_filter(TextFilter::Sharp);
}

/// Smooth, at 2.25 pixels a texel: most pixels a mix, no column a run of
/// white; and the surface, nearest, exactly as under Sharp (a font's filter
/// is no other quad's).
#[test]
fn smooth_mixes_texels_and_only_a_fonts() {
    let Some((device, queue)) = device() else { return };
    let _one = FILTER.lock().unwrap_or_else(|e| e.into_inner());
    let size = (1920, 1080);
    let smooth = drawn(&device, &queue, size, TextFilter::Smooth);
    let sharp = drawn(&device, &queue, size, TextFilter::Sharp);
    let font = row(&smooth, size, 103.0);
    assert!(mixed(&font) * 2 > font.len(), "Smooth mixes most pixels ({font:?})");
    assert_eq!(row(&smooth, size, 303.0), row(&sharp, size, 303.0), "the surface is drawn alike");
    let (a, b) = (ui::window_from_frame(size, [303.0, 100.0]), ui::window_from_frame(size, [319.0, 121.0]));
    for y in a[1] as usize..b[1] as usize {
        let at = |px: &[u8]| px[(y * size.0 as usize + a[0] as usize) * 4..(y * size.0 as usize + b[0] as usize) * 4].to_vec();
        assert_eq!(at(&smooth), at(&sharp), "surface row {y}");
    }
    assert_ne!(row(&smooth, size, 103.0), row(&sharp, size, 103.0), "the control: the font's are not");
    ui::set_text_filter(TextFilter::Sharp);
}
