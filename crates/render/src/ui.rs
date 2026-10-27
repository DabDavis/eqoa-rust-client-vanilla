//! The 2D layer `VIRaster` draws between `Begin2D` and `End2D`: textured
//! quads in the PS2's 640x448 pixels (`Blit`, `StretchBlit`, a font's
//! glyphs, the HUD's own fans and strips), each with a blend mode and a
//! colour, drawn in the order they were made, scaled to the target. Blend
//! modes are the material layer's (0 alpha tested at 128, 1 add, 2
//! subtract, 3 alpha, 4 multiply), with the same GS arithmetic as the 3D
//! path: texture times colour, alpha as stored.
//!
//! A font is packed into one texture (`add_font`) and laid out as
//! `VIRaster::DrawString` lays it out: each glyph its width wide and the
//! font's height tall, one pixel apart.
//!
//! The frame keeps its 4:3 shape in a wider target, in the middle, and a
//! quad made under an `Anchor` other than the middle's moves out with the
//! side it belongs to: the HUD reaches the window's edges. A quad with the
//! frame that is the frame's whole width (a window's backdrop, which on a TV
//! runs off both sides) runs the target's whole width; one made under
//! `Anchor::Frame` (the start menu's and the merchant's art) stays the
//! frame's. In a target narrower than 4:3 (two windows side by side, a phone
//! held upright) the frame fills the width, in the middle of the height, and
//! the HUD's pieces keep to the target's top or bottom the same way
//! (`Vertical`).

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use wgpu::util::DeviceExt;

pub const SCREEN_W: f32 = 640.0;
pub const SCREEN_H: f32 = 448.0;

/// A quad's clip that is the whole target (`Ui::push_whole`).
pub const WHOLE: [i32; 4] = [i32::MIN / 4, i32::MIN / 4, i32::MAX / 4, i32::MAX / 4];

/// `VIRaster::Viewport(x1, y1, x2, y2)` as the layer keeps it (`Ui::viewport`):
/// the whole screen is no viewport. A window that saves the viewport and sets
/// it back (`VIUITextScroll` round a long name) leaves the layer as it found
/// it, so a backdrop the frame's width made after it still runs the target's
/// (`widen`); kept as a viewport, every backdrop after the first long name
/// stopped at the frame's sides.
pub fn viewport_of(v: [i32; 4]) -> Option<[i32; 4]> {
    (v != [0, 0, SCREEN_W as i32 - 1, SCREEN_H as i32 - 1]).then_some(v)
}

/// Where the frame sits in a `size` target: its offset and its scale. The
/// frame keeps its 4:3 shape (a PS2 shows it on a 4:3 picture): the full
/// height, in the middle of a wider target (eqoa-port's widescreen does the
/// same with the HUD).
fn frame(size: (u32, u32)) -> (f32, f32, f32, f32) {
    let (tw, th) = (size.0 as f32, size.1 as f32);
    let fw = (th * 4.0 / 3.0).min(tw);
    let fh = if fw < th * 4.0 / 3.0 { fw * 3.0 / 4.0 } else { th };
    ((tw - fw) / 2.0, (th - fh) / 2.0, fw / SCREEN_W, fh / SCREEN_H)
}

/// A point of a `size` target, in normalised device coordinates (y up), as
/// a point of the frame: off its sides in a wider target.
pub fn frame_from_ndc(size: (u32, u32), ndc: [f32; 2]) -> [f32; 2] {
    let (tw, th) = (size.0 as f32, size.1 as f32);
    let (ox, oy, sx, sy) = frame(size);
    [((ndc[0] + 1.0) * 0.5 * tw - ox) / sx, ((1.0 - ndc[1]) * 0.5 * th - oy) / sy]
}

/// A point of a `size` target in its own pixels (y down), as a point of the
/// frame: what is laid out against the window's edges (`Ui::push_whole`).
pub fn frame_from_window(size: (u32, u32), px: [f32; 2]) -> [f32; 2] {
    let (ox, oy, sx, sy) = frame(size);
    [(px[0] - ox) / sx, (px[1] - oy) / sy]
}

/// A point of the frame as a point of a `size` target in its own pixels (y
/// down): `frame_from_window` undone.
pub fn window_from_frame(size: (u32, u32), p: [f32; 2]) -> [f32; 2] {
    let (ox, oy, sx, sy) = frame(size);
    [ox + p[0] * sx, oy + p[1] * sy]
}

/// How far up and left every quad is drawn, in frame pixels. The GS samples
/// a frame pixel at its texel's centre (`VIRaster::Blit` puts the corners
/// half a pixel in), but in a target scaled from 640x448 some pixel centres
/// fall on a texel's edge, and the edge of a quad on one of them samples the
/// texel past it: an atlas's black line round a sprite (the title's sail
/// sailing right showed one at its stern), or the other side of a surface
/// through the repeat. A hair up and left, each pixel takes the texel its
/// frame pixel's centre would; at 640x448 nothing moves. That moves the
/// edge, not the case: at other sizes a pixel's centre falls on the moved
/// edge (1918x1078: frame x 363 less the hair, the start menu's title plate's
/// mirrored right cap), where a mirrored quad takes the uv of its far end,
/// the texel past it. So a quad also keeps to its own texels (`ui.wgsl`,
/// `Vertex::rect`).
pub const SAMPLE_BIAS: f32 = 1.0 / 32.0;

/// How the game's fonts are drawn scaled to the target: every 2D layer's
/// (the HUD, the windows, the pregame) and the labels' and name
/// plates' (`text`). The game's fonts hold whole texels only (ArialUI white
/// or clear, Arial with a grey edge as well), two texels to a stroke, drawn
/// one texel to a pixel on the PS2's 640x448 and softened by the TV.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextFilter {
    /// Crisp: a texel's own colour across it, the next one's mixed in only
    /// round its edge ("sharp bilinear", `ui.wgsl`), so from two pixels a
    /// texel every texel is as wide as every other; the nearest texel alone
    /// makes some a pixel wider than the rest (at 1.5 a two-texel stroke is
    /// two, three or four pixels wide, letter by letter). Nearer one pixel a
    /// texel the mix narrows to none, the nearest texel, as at a whole scale.
    #[default]
    Sharp,
    /// Each texel blended with its neighbours across its whole width, as the
    /// GS magnifies a font's texture (`VIRFont`'s material filters its
    /// magnification): smooth, as text looks on a TV or upscaled.
    Smooth,
}

/// A font's glyphs packed into rows of a `width`-wide RGBA8 atlas, each
/// with a ring of clear texels of its own round all four sides, so a glyph's quad filtered
/// (`TextFilter`) mixes in only clear texels past its edges, never another
/// glyph's: the atlas, its height, and each code's width and texture
/// rectangle (the first glyph of a code kept, as `FindChar` finds it). A
/// clear texel next to ink takes the ink's colour and stays clear: mixed
/// with it, the ink fades out where straight alpha would also darken it
/// toward the clear texel's black. Nearest, a clear texel is clear whatever
/// its colour.
pub(crate) fn pack(font: &esf::font::Font, width: u32) -> (Vec<u8>, u32, HashMap<u16, (u32, [f32; 4])>) {
    let h = font.height;
    let (mut x, mut row) = (1u32, 0u32);
    let mut places = Vec::new();
    for g in &font.glyphs {
        if x + g.width as u32 + 1 > width {
            x = 1;
            row += 1;
        }
        places.push((x, row));
        x += g.width as u32 + 2;
    }
    let ah = (row + 1) * (h + 2);
    let mut atlas = vec![0u8; (width * ah * 4) as usize];
    let mut glyphs = HashMap::new();
    for (g, &(gx, gr)) in font.glyphs.iter().zip(&places) {
        let gy = 1 + gr * (h + 2);
        for y in 0..h {
            for xx in 0..g.width as u32 {
                let s = ((y * g.width as u32 + xx) * 4) as usize;
                let d = (((gy + y) * width + gx + xx) * 4) as usize;
                atlas[d..d + 4].copy_from_slice(&g.rgba[s..s + 4]);
            }
        }
        let uv = [gx as f32 / width as f32, gy as f32 / ah as f32, (gx + g.width as u32) as f32 / width as f32, (gy + h) as f32 / ah as f32];
        glyphs.entry(g.code).or_insert((g.width as u32, uv));
    }
    // each glyph's clear texels, its ring's too, coloured by the ink round
    // them (the mean of their eight neighbours' that are not clear)
    let ink = atlas.clone();
    let at = |x: u32, y: u32| ((y * width + x) * 4) as usize;
    for (g, &(gx, gr)) in font.glyphs.iter().zip(&places) {
        let gy = 1 + gr * (h + 2);
        for y in gy - 1..=gy + h {
            for x in gx - 1..=gx + g.width as u32 {
                if ink[at(x, y) + 3] != 0 {
                    continue;
                }
                let (mut sum, mut n) = ([0u32; 3], 0);
                for (nx, ny) in (y.saturating_sub(1)..=(y + 1).min(ah - 1)).flat_map(|ny| (x.saturating_sub(1)..=(x + 1).min(width - 1)).map(move |nx| (nx, ny))) {
                    let i = at(nx, ny);
                    if ink[i + 3] != 0 {
                        for k in 0..3 {
                            sum[k] += ink[i + k] as u32;
                        }
                        n += 1;
                    }
                }
                if n > 0 {
                    let i = at(x, y);
                    for k in 0..3 {
                        atlas[i + k] = (sum[k] / n) as u8;
                    }
                }
            }
        }
    }
    (atlas, ah, glyphs)
}

/// How the 2D layer's other textures (the HUD, the windows, the pregame's
/// pictures: every one but the fonts', `TextFilter`) are drawn scaled to the
/// target. The GS samples them at the nearest texel (the blit material's
/// filters), one texel to a pixel on the PS2's 640x448; at a scale that is
/// not whole the nearest texel makes some texels a pixel wider than the rest
/// (at 1080p, 2.41 pixels a texel: two pixels or three).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HudFilter {
    /// The nearest texel, as the GS takes it: the game's.
    #[default]
    Nearest,
    /// The fonts' Sharp (`TextFilter::Sharp`, `ui.wgsl`'s `sharp`): every
    /// texel as wide as every other, its own colour across it and the next
    /// one's mixed in only round its edge, kept to the quad's own texels as
    /// the nearest texel is (an atlas packs its cells edge to edge), and a
    /// clear texel mixed in by its clearness alone, not its colour (`ui.wgsl`'s
    /// `mix_clear`: the art's clear texels are often black). At a whole
    /// scale, the nearest texel.
    Sharp,
}

impl HudFilter {
    /// The other one (the options' HUD turned over).
    pub fn other(self) -> HudFilter {
        match self {
            HudFilter::Nearest => HudFilter::Sharp,
            HudFilter::Sharp => HudFilter::Nearest,
        }
    }
}

impl TextFilter {
    /// The other one (the options' Text turned over).
    pub fn other(self) -> TextFilter {
        match self {
            TextFilter::Sharp => TextFilter::Smooth,
            TextFilter::Smooth => TextFilter::Sharp,
        }
    }
}

/// A font's quad (an upright rectangle, its uvs running with its corners)
/// drawn one texel wider all round, onto the clear ring round its glyph
/// (`pack`): a mix at the glyph's edge (`TextFilter`) reaches half a pixel or
/// more past it, and drawn only to its edge an edge column lost that light.
/// The quad the game made, which the layer keeps, is its glyph's.
fn ringed(q: &Quad, w: u32, h: u32) -> Quad {
    let mut r = *q;
    let (dx, dy) = (q.pos[1][0] - q.pos[0][0], q.pos[3][1] - q.pos[0][1]);
    let (du, dv) = (q.uv[1][0] - q.uv[0][0], q.uv[3][1] - q.uv[0][1]);
    if dx == 0.0 || dy == 0.0 || du == 0.0 || dv == 0.0 {
        return r;
    }
    // a texel across and down in frame pixels, and which way the uvs run
    let (kx, ky) = ((dx / (du * w as f32)).abs(), (dy / (dv * h as f32)).abs());
    let (su, sv) = ((du / dx).signum(), (dv / dy).signum());
    let (cx, cy) = ((q.pos[0][0] + q.pos[2][0]) / 2.0, (q.pos[0][1] + q.pos[2][1]) / 2.0);
    for i in 0..4 {
        let (ex, ey) = (if q.pos[i][0] > cx { 1.0 } else { -1.0 }, if q.pos[i][1] > cy { 1.0 } else { -1.0 });
        r.pos[i] = [q.pos[i][0] + ex * kx, q.pos[i][1] + ey * ky];
        r.uv[i] = [q.uv[i][0] + ex * su / w as f32, q.uv[i][1] + ey * sv / h as f32];
    }
    r
}

/// The fonts' filter (`TextFilter`): 0 Sharp, 1 Smooth.
static TEXT_FILTER: AtomicU8 = AtomicU8::new(0);

/// Sets how every layer's fonts and the labels are drawn from now on (the
/// options' Text).
pub fn set_text_filter(f: TextFilter) {
    TEXT_FILTER.store(f as u8, Ordering::Relaxed);
}

pub fn text_filter() -> TextFilter {
    match TEXT_FILTER.load(Ordering::Relaxed) {
        1 => TextFilter::Smooth,
        _ => TextFilter::Sharp,
    }
}

/// The other textures' filter (`HudFilter`): 0 Nearest, 1 Sharp.
static HUD_FILTER: AtomicU8 = AtomicU8::new(0);

/// Sets how every layer's textures but the fonts are drawn from now on (the
/// options' HUD).
pub fn set_hud_filter(f: HudFilter) {
    HUD_FILTER.store(f as u8, Ordering::Relaxed);
}

pub fn hud_filter() -> HudFilter {
    match HUD_FILTER.load(Ordering::Relaxed) {
        1 => HudFilter::Sharp,
        _ => HudFilter::Nearest,
    }
}

/// The whole frame pixels a `size` target shows beyond each side of the
/// frame: none when it is 4:3 or narrower.
pub fn side(size: (u32, u32)) -> f32 {
    let (ox, _, sx, _) = frame(size);
    (ox / sx).floor()
}

/// The whole frame pixels a `size` target shows above and below the frame:
/// none when it is 4:3 or wider.
pub fn top(size: (u32, u32)) -> f32 {
    let (_, oy, _, sy) = frame(size);
    (oy / sy).floor()
}

/// Where the HUD's pieces made now go up a target taller than the frame
/// (`Ui::vertical`): the game lays its HUD out for a 4:3 picture, and in a
/// narrower window each piece keeps its distance from the top of the screen
/// or from its bottom, as an `Anchor` keeps it from a side in a wider one.
/// Only what goes to a side (`Left`, `Right`, `Span`) moves: the frame's own
/// (`Middle`, `Frame`, `Wide`: the boxes, the menus, the windows) stays in
/// the middle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Vertical {
    /// With the frame, in the middle of the target's height.
    #[default]
    Middle,
    /// With the target's top.
    Top,
    /// With the target's bottom.
    Bottom,
}

impl Vertical {
    /// Where frame y `y` goes, `top` frame pixels showing above and below
    /// the frame.
    pub fn place(self, y: f32, top: f32) -> f32 {
        match self {
            Vertical::Middle => y,
            Vertical::Top => y - top,
            Vertical::Bottom => y + top,
        }
    }
}

/// Where the quads made now go across a target wider than the frame
/// (`Ui::anchor`). The game lays its HUD out for a 4:3 picture; in a wider
/// one each piece keeps its distance from its own side of the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Anchor {
    /// With the frame, in the middle (the menus and the boxes).
    #[default]
    Middle,
    /// With the target's left side.
    Left,
    /// With the target's right side.
    Right,
    /// Frame x `.0` with the left side and `.1` with the right, stretched
    /// between: a backdrop that joins a left piece to a right one.
    Span(i32, i32),
    /// With the frame, but cut to the target's sides rather than the
    /// frame's: what is laid out at a wider width than the frame's (STATION's
    /// fader, across the whole window).
    Wide,
    /// With the frame and cut to it, as a 4:3 screen shows it: a full-screen
    /// window's art kept 4:3 in a wider target (the start menu, its
    /// inventory, the spell book, the merchant). Never widened, even the
    /// frame's whole width (the start menu's band, the merchant's rails), and
    /// no window that takes the HUD back (`settle`): the compass and the rest
    /// stay at the target's sides, beside the picture (the user, 2026-10-23).
    Frame,
}

impl Anchor {
    /// Where frame x `x` goes, `side` frame pixels showing beyond each side
    /// of the frame.
    pub fn place(self, x: f32, side: f32) -> f32 {
        match self {
            Anchor::Middle | Anchor::Frame | Anchor::Wide => x,
            Anchor::Left => x - side,
            Anchor::Right => x + side,
            Anchor::Span(a, b) => x - side + 2.0 * side * (x - a as f32) / (b - a).max(1) as f32,
        }
    }

    /// Whether quads under it reach the target's left side, or its right.
    fn reaches(self) -> (bool, bool) {
        match self {
            Anchor::Middle | Anchor::Frame => (false, false),
            Anchor::Left => (true, false),
            Anchor::Right => (false, true),
            Anchor::Span(..) | Anchor::Wide => (true, true),
        }
    }

    /// Whether it is a HUD piece's, one that goes to a side (and up or down
    /// a taller target, `Vertical`), rather than the frame's.
    fn hud(self) -> bool {
        matches!(self, Anchor::Left | Anchor::Right | Anchor::Span(..))
    }
}

/// `Ui::push`'s placing: relative to the viewport's corner and cut to it
/// (the GS's offset and scissor), then across the target by `anchor`, the
/// cut too (the whole frame's, without a viewport). A cut at the frame's own
/// side, moved out to the target's, takes the target's edge (past the whole
/// frame pixels of `side`, its fraction of one). With the frame (`Middle`)
/// and no viewport, a quad the frame's whole width runs the target's
/// (`widen`); under `Frame` it stays the frame's.
fn place(q: &mut Quad, viewport: Option<[i32; 4]>, anchor: Anchor, side: f32) {
    if let Some([x1, y1, x2, y2]) = viewport {
        for p in &mut q.pos {
            p[0] += x1 as f32;
            p[1] += y1 as f32;
        }
        q.clip = Some([x1, y1, x2 + 1, y2 + 1]);
    } else {
        q.clip = None;
    }
    if side > 0.0 && !matches!(anchor, Anchor::Middle | Anchor::Frame) {
        for p in &mut q.pos {
            p[0] = anchor.place(p[0], side);
        }
        let [x0, y0, x1, y1] = q.clip.unwrap_or([0, 0, SCREEN_W as i32, SCREEN_H as i32]);
        let x = |v: i32| anchor.place(v as f32, side).round() as i32;
        let (left, right) = anchor.reaches();
        let edge = side as i32 + 1;
        let l = if left && x0 <= 0 { -edge } else { x(x0) };
        let r = if right && x1 >= SCREEN_W as i32 { SCREEN_W as i32 + edge } else { x(x1) };
        q.clip = Some([l, y0, r, y1]);
    } else if side > 0.0 && viewport.is_none() && anchor == Anchor::Middle {
        widen(q, side);
    }
}

/// `Ui::push`'s placing up a target taller than the frame, after `place`: a
/// HUD piece's quad (`Anchor::hud`) moved by `vertical` to the target's top
/// or its bottom, its cut too (the whole frame's, without a viewport); a cut
/// at the frame's own top or bottom, moved out, takes the target's edge (past
/// the whole frame pixels of `top`, its fraction of one). The frame's quads
/// stay where `place` put them, and nothing moves in a target 4:3 or wider.
fn rise(q: &mut Quad, anchor: Anchor, vertical: Vertical, top: f32) {
    if top <= 0.0 || vertical == Vertical::Middle || !anchor.hud() {
        return;
    }
    for p in &mut q.pos {
        p[1] = vertical.place(p[1], top);
    }
    let [x0, y0, x1, y1] = q.clip.unwrap_or([0, 0, SCREEN_W as i32, SCREEN_H as i32]);
    let y = |v: i32| vertical.place(v as f32, top).round() as i32;
    let edge = top as i32 + 1;
    let t = if vertical == Vertical::Top && y0 <= 0 { -edge } else { y(y0) };
    let b = if vertical == Vertical::Bottom && y1 >= SCREEN_H as i32 { SCREEN_H as i32 + edge } else { y(y1) };
    q.clip = Some([x0, t, x1, b]);
}

/// A quad with the frame that is the frame's whole width (a window's
/// backdrop or band: on a TV it runs off both sides) run across the target's
/// whole width, its texture carried on at its own scale (a blit's texels
/// repeat). An axis-aligned quad only.
fn widen(q: &mut Quad, side: f32) {
    let (lo, hi) = q.pos.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[0]), b.max(p[0])));
    let ([a, _], [b, _]) = (q.pos[0], q.pos[1]);
    if lo > 0.0 || hi < SCREEN_W || q.pos[0][1] != q.pos[1][1] || a == b {
        return;
    }
    let k = [(q.uv[1][0] - q.uv[0][0]) / (b - a), (q.uv[1][1] - q.uv[0][1]) / (b - a)];
    let e = side + 1.0;
    for (p, uv) in q.pos.iter_mut().zip(q.uv.iter_mut()) {
        let d = if p[0] == lo { -e } else { e };
        p[0] += d;
        uv[0] += d * k[0];
        uv[1] += d * k[1];
    }
    q.clip = Some(WHOLE);
}

/// The frame rectangle an opaque quad covers (its bounds within its cut):
/// alpha tested, or blended at full alpha.
fn opaque_rect(q: &Quad) -> Option<[f32; 4]> {
    if !(q.blend == 0 || (q.blend == 3 && q.color[3] >= 1.0)) {
        return None;
    }
    let mut r = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in &q.pos {
        r = [r[0].min(p[0]), r[1].min(p[1]), r[2].max(p[0]), r[3].max(p[1])];
    }
    if let Some([x0, y0, x1, y1]) = q.clip {
        r = [r[0].max(x0 as f32), r[1].max(y0 as f32), r[2].min(x1 as f32), r[3].min(y1 as f32)];
    }
    Some(r)
}

/// What `Ui::push` notes of quad `i` for `end`: how far it went to a side,
/// or, placed with the frame (`Middle`), what it covers when it is a
/// full-screen window's backdrop (opaque, the frame's whole width: `widen`'s).
/// A box (the edit box, a dialog, the quick chat, the options) leaves the
/// HUD at the window's edges, where its 4:3 place is not covered (F3
/// 2026-10-21: the say box took the compass and the gauges back under it),
/// and so does a window kept 4:3 (`Frame`).
fn note(i: usize, q: &Quad, anchor: Anchor, side: f32, sided: &mut Vec<(usize, f32)>, opaque: &mut Vec<(usize, [f32; 4])>) {
    match anchor {
        Anchor::Left | Anchor::Right if side > 0.0 => sided.push((i, anchor.place(0.0, side))),
        Anchor::Middle => opaque.extend(opaque_rect(q).filter(|r| r[0] <= 0.0 && r[2] >= SCREEN_W).map(|r| (i, r))),
        _ => {}
    }
}

/// What `Ui::push` notes for `end` of a quad `rise` moved: how far it went up
/// or down, for a piece at a side (`Left`, `Right`), as `note` notes how far
/// one went to its side.
fn rose(anchor: Anchor, vertical: Vertical, top: f32) -> Option<f32> {
    (top > 0.0 && vertical != Vertical::Middle && matches!(anchor, Anchor::Left | Anchor::Right)).then(|| vertical.place(0.0, top))
}

/// `Ui::end`: each quad moved `dx` to a side goes back with the frame when a
/// full-screen window's backdrop made after it (`note`) covers its middle, as
/// that window covers it on a 4:3 screen.
fn settle(quads: &mut [Quad], sided: &[(usize, f32)], opaque: &[(usize, [f32; 4])]) {
    settle_along(quads, sided, opaque, 0);
}

/// `settle` along `axis` (0 across, 1 up and down: what `rise` moved).
fn settle_along(quads: &mut [Quad], moved: &[(usize, f32)], opaque: &[(usize, [f32; 4])], axis: usize) {
    for &(i, d) in moved {
        let q = &mut quads[i];
        let mut c = [q.pos.iter().map(|p| p[0]).sum::<f32>() / 4.0, q.pos.iter().map(|p| p[1]).sum::<f32>() / 4.0];
        c[axis] -= d;
        if opaque.iter().any(|&(j, r)| j > i && c[0] >= r[0] && c[0] < r[2] && c[1] >= r[1] && c[1] < r[3]) {
            for p in &mut q.pos {
                p[axis] -= d;
            }
            // back with the frame, cut to it
            if let Some(c) = &mut q.clip {
                let (lo, hi, max) = if axis == 0 { (0, 2, SCREEN_W) } else { (1, 3, SCREEN_H) };
                c[lo] = (c[lo] - d as i32).max(0);
                c[hi] = (c[hi] - d as i32).min(max as i32);
            }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    /// The quad's texture rectangle (its corners' least and greatest uv):
    /// what the fragment keeps to (`ui.wgsl`).
    rect: [f32; 4],
}

/// One quad: its corners in screen pixels and texture coordinates, in fan
/// order (the first, second, third and fourth vertex the game sends).
#[derive(Clone, Copy, Debug)]
pub struct Quad {
    pub tex: usize,
    pub blend: u8,
    pub pos: [[f32; 2]; 4],
    pub uv: [[f32; 2]; 4],
    pub color: [f32; 4],
    /// The viewport it was drawn under (`VIRaster::Viewport`), screen
    /// pixels x0, y0, x1, y1 (exclusive); None for the whole screen.
    pub clip: Option<[i32; 4]>,
}

struct Texture {
    bg: wgpu::BindGroup,
    /// The same texture through the filtering sampler, which a font's quads
    /// take under either `TextFilter` and the others' under `HudFilter::Sharp`.
    filtered: wgpu::BindGroup,
    /// A font's (`add_font`).
    font: bool,
    width: u32,
    height: u32,
}

/// A font packed into one of the layer's textures.
pub struct Font {
    pub tex: usize,
    pub height: u32,
    glyphs: HashMap<u16, (u32, [f32; 4])>,
}

impl Font {
    /// `FindChar`, with `'?'` for a character the font lacks.
    fn find(&self, code: u16) -> Option<&(u32, [f32; 4])> {
        self.glyphs.get(&code).or_else(|| self.glyphs.get(&(b'?' as u16)))
    }

    /// `VIRFont::GetStringWidth(s, 1)`: each glyph's width plus one; -1
    /// when even `'?'` is missing.
    pub fn string_width(&self, s: &[u16]) -> i32 {
        let mut w = 0;
        for &c in s {
            let Some(g) = self.find(c) else { return -1 };
            w += g.0 as i32 + 1;
        }
        w
    }
}

pub struct Ui {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// `Texture::filtered`'s: linear.
    filtering: wgpu::Sampler,
    pipelines: Vec<wgpu::RenderPipeline>,
    /// A font's quads by blend mode, under `TextFilter::Sharp` and `Smooth`.
    sharp: Vec<wgpu::RenderPipeline>,
    smooth: Vec<wgpu::RenderPipeline>,
    /// The other textures' quads by blend mode under `HudFilter::Sharp`.
    art_sharp: Vec<wgpu::RenderPipeline>,
    textures: Vec<Texture>,
    pub fonts: Vec<Font>,
    pub quads: Vec<Quad>,
    /// `VIRaster::Viewport(x1, y1, x2, y2)` (inclusive) as quads are made:
    /// they are placed relative to (x1, y1) and cut to the rectangle, as the
    /// GS's offset and scissor do. None is the whole screen.
    pub viewport: Option<[i32; 4]>,
    /// Where the quads made now go across a wider target.
    pub anchor: Anchor,
    /// Where the HUD's pieces made now go up a taller target.
    pub vertical: Vertical,
    /// The target's frame pixels beyond each side of the frame, and above
    /// and below it (`begin`).
    side: f32,
    top: f32,
    /// The quads moved to a side, and how far, the quads moved up or down,
    /// and how far; the opaque quads placed with the frame, and what they
    /// cover (`end`).
    sided: Vec<(usize, f32)>,
    risen: Vec<(usize, f32)>,
    opaque: Vec<(usize, [f32; 4])>,
    /// The quads `keep` kept, which `again` makes the frame's.
    kept: Vec<Quad>,
    /// Where a window's 3D goes among the quads (character select's and
    /// creation's scene, drawn into its viewport): the quads before it are
    /// under it, the rest over it. None: no 3D.
    pub split: Option<usize>,
    /// This layer's own number (`serial`).
    serial: u64,
}

/// The layers made so far (`Ui::serial`).
static LAYERS: AtomicU64 = AtomicU64::new(0);

impl Ui {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, depth: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ui"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        // Texels to pixels one for one at 640x448, so the nearest texel;
        // a rotated quad (the compass) the GS also samples point. Repeating,
        // as the blit material's layer does (`InitSolidFill`'s flags 1, so
        // CLAMP_1 is 0): a blit bigger than its surface tiles it, which is
        // how the menus' panels and frame edges are drawn (every UI surface
        // is a power of two, as the GS's wrap needs).
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("ui.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("ui"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        // `Texture::filtered`'s: a font's quads mix texels (`TextFilter`), and
        // the others' under the HUD's Sharp (`HudFilter`).
        let filtering = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // A pipeline for each blend mode, mode 0 alpha tested (`tested`), the
        // others through `main`.
        let set = |label: &str, tested: &str, main: &str| -> Vec<wgpu::RenderPipeline> {
            (0..5)
                .map(|mode| {
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some(label),
                        layout: Some(&pl),
                        vertex: wgpu::VertexState {
                            module: &shader,
                            entry_point: Some("vs_main"),
                            compilation_options: Default::default(),
                            buffers: &[Some(wgpu::VertexBufferLayout {
                                array_stride: std::mem::size_of::<Vertex>() as u64,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4],
                            })],
                        },
                        primitive: Default::default(),
                        depth_stencil: Some(wgpu::DepthStencilState {
                            format: depth,
                            depth_write_enabled: Some(false),
                            depth_compare: Some(wgpu::CompareFunction::Always),
                            stencil: Default::default(),
                            bias: Default::default(),
                        }),
                        multisample: Default::default(),
                        fragment: Some(wgpu::FragmentState {
                            module: &shader,
                            entry_point: Some(if mode == 0 { tested } else { main }),
                            compilation_options: Default::default(),
                            targets: &[Some(wgpu::ColorTargetState { format, blend: crate::blend_state(mode), write_mask: wgpu::ColorWrites::COLOR })],
                        }),
                        multiview_mask: None,
                        cache: None,
                    })
                })
                .collect()
        };
        let pipelines = set("ui", "fs_tested", "fs_main");
        let sharp = set("ui sharp text", "fs_sharp_tested", "fs_sharp");
        let smooth = set("ui smooth text", "fs_smooth_tested", "fs_smooth");
        let art_sharp = set("ui sharp", "fs_art_sharp_tested", "fs_art_sharp");
        Ui { layout, sampler, filtering, pipelines, sharp, smooth, art_sharp, textures: Vec::new(), fonts: Vec::new(), quads: Vec::new(), viewport: None, anchor: Anchor::Middle, vertical: Vertical::Middle, side: 0.0, top: 0.0, sided: Vec::new(), risen: Vec::new(), opaque: Vec::new(), kept: Vec::new(), split: None, serial: LAYERS.fetch_add(1, Ordering::Relaxed) }
    }

    /// A new frame's quads, for a `size` target: none yet, and those made
    /// next go with the frame (`anchor`, `vertical`) under the whole screen
    /// (`viewport`).
    pub fn begin(&mut self, size: (u32, u32)) {
        self.quads.clear();
        self.sided.clear();
        self.risen.clear();
        self.opaque.clear();
        self.side = side(size);
        self.top = top(size);
        self.anchor = Anchor::Middle;
        self.vertical = Vertical::Middle;
        self.viewport = None;
        self.split = None;
    }

    /// The 3D goes here: the quads made so far are drawn under it.
    pub fn mark_split(&mut self) {
        self.split = Some(self.quads.len());
    }

    /// The whole frame pixels the target shows beyond each side of the
    /// frame this frame (`side`): none on a 4:3 target.
    pub fn side(&self) -> f32 {
        self.side
    }

    /// The frame's quads are made: a piece moved to a side (or up or down a
    /// taller target) that a full-screen window made after it covers on a
    /// 4:3 screen goes back under that window, so a menu hides what it hides
    /// there (and the piece is not cut by its edge); what no such window
    /// covers, a box's 4:3 place included, stays at the side.
    pub fn end(&mut self) {
        settle(&mut self.quads, &self.sided, &self.opaque);
        settle_along(&mut self.quads, &self.risen, &self.opaque, 1);
        self.sided.clear();
        self.risen.clear();
        self.opaque.clear();
    }

    /// The quads made so far kept for the frames that follow (`again`): the
    /// game makes its 2D layer once a frame of its own, 30 a second, and a
    /// faster display draws the same quads in between.
    pub fn keep(&mut self) {
        self.kept.clone_from(&self.quads);
    }

    /// The kept quads made this frame's, in place of making them again.
    pub fn again(&mut self) {
        self.quads.clone_from(&self.kept);
    }

    /// A texture for the layer: RGBA8, top row first. Its index.
    pub fn add_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) -> usize {
        self.add_texture_as(device, queue, width, height, rgba, false)
    }

    /// `add_texture`, a font's if `font`.
    fn add_texture_as(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8], font: bool) -> usize {
        let size = wgpu::Extent3d { width, height, depth_or_array_layers: 1 };
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: crate::TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(tex.as_image_copy(), rgba, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) }, size);
        let view = tex.create_view(&Default::default());
        let bind = |sampler: &wgpu::Sampler| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("ui"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
                ],
            })
        };
        let (bg, filtered) = (bind(&self.sampler), bind(&self.filtering));
        self.textures.push(Texture { bg, filtered, font, width, height });
        self.textures.len() - 1
    }

    /// This layer's own number among every layer made. A texture index is
    /// good only on the layer that made it, and a layer goes with its session
    /// (the next one's HUD makes another), so what keeps an index from frame
    /// to frame (the touch pad's discs) keeps the number with it.
    pub fn serial(&self) -> u64 {
        self.serial
    }

    /// How many textures the layer holds.
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// A texture's size.
    pub fn texture_size(&self, tex: usize) -> (u32, u32) {
        let t = &self.textures[tex];
        (t.width, t.height)
    }

    /// A font, packed into rows of a 2048-wide texture (`pack`). Its index.
    pub fn add_font(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, font: &esf::font::Font) -> usize {
        let (atlas, ah, glyphs) = pack(font, 2048);
        let tex = self.add_texture_as(device, queue, 2048, ah, &atlas, true);
        self.fonts.push(Font { tex, height: font.height, glyphs });
        self.fonts.len() - 1
    }

    /// `VIRaster::DrawString`: the glyphs from (x, y), each its width, one
    /// pixel apart, alpha-blended in `color` (`'?'` for a character the
    /// font lacks; nothing when it lacks that too).
    pub fn text(&mut self, font: usize, x: i32, y: i32, color: [f32; 4], s: &[u16]) {
        let f = &self.fonts[font];
        let (tex, h) = (f.tex, f.height as f32);
        let mut x0 = x as f32;
        let y0 = y as f32;
        let mut quads = Vec::new();
        for &c in s {
            let Some(&(w, uv)) = f.find(c) else { return };
            let x1 = x0 + w as f32;
            quads.push(Quad {
                tex,
                blend: 3,
                pos: [[x0, y0], [x1, y0], [x1, y0 + h], [x0, y0 + h]],
                uv: [[uv[0], uv[1]], [uv[2], uv[1]], [uv[2], uv[3]], [uv[0], uv[3]]],
                color,
                clip: None,
            });
            x0 = x1 + 1.0;
        }
        for q in quads {
            self.push(q);
        }
    }

    /// A quad cut only to the target, not to the frame or a viewport: for
    /// what floats over the world, which reaches past the frame's sides in a
    /// wider window. Its place is its own (no `anchor`).
    pub fn push_whole(&mut self, mut q: Quad) {
        q.clip = Some(WHOLE);
        self.quads.push(q);
    }

    /// Where frame point `p` made now lands, as a frame point: `push`'s
    /// placing of a quad's corner (the viewport's corner, then `anchor`,
    /// then `vertical`). For what is picked on the screen later.
    pub fn placed(&self, p: [f32; 2]) -> [f32; 2] {
        let (mut x, mut y) = (p[0], p[1]);
        if let Some([x1, y1, _, _]) = self.viewport {
            x += x1 as f32;
            y += y1 as f32;
        }
        if self.side > 0.0 && self.anchor != Anchor::Middle {
            x = self.anchor.place(x, self.side);
        }
        if self.top > 0.0 && self.anchor.hud() {
            y = self.vertical.place(y, self.top);
        }
        [x, y]
    }

    /// A quad, placed in and cut to the current viewport, then moved across
    /// a wider target by `anchor`, or up a taller one by `vertical`.
    pub fn push(&mut self, mut q: Quad) {
        place(&mut q, self.viewport, self.anchor, self.side);
        rise(&mut q, self.anchor, self.vertical, self.top);
        note(self.quads.len(), &q, self.anchor, self.side, &mut self.sided, &mut self.opaque);
        if let Some(dy) = rose(self.anchor, self.vertical, self.top) {
            self.risen.push((self.quads.len(), dy));
        }
        self.quads.push(q);
    }

    /// Draws the frame's quads into `pass` (a `size` target) and keeps them
    /// (the caller clears `quads` when it rebuilds them).
    pub fn draw<'a>(&'a self, device: &wgpu::Device, pass: &mut wgpu::RenderPass<'a>, size: (u32, u32)) {
        self.draw_range(device, pass, size, 0..self.quads.len());
    }

    /// `draw`, for the quads in `range` only (around a window's 3D, `split`).
    pub fn draw_range<'a>(&'a self, device: &wgpu::Device, pass: &mut wgpu::RenderPass<'a>, size: (u32, u32), range: std::ops::Range<usize>) {
        let range = range.start.min(self.quads.len())..range.end.min(self.quads.len());
        if range.is_empty() {
            return;
        }
        let quads = &self.quads[range];
        let (tw, th) = (size.0 as f32, size.1 as f32);
        let (ox, oy, sx, sy) = frame(size);
        let hud = hud_filter();
        let mut v = Vec::with_capacity(quads.len() * 6);
        for q in quads {
            let rect = q.uv.iter().fold([f32::MAX, f32::MAX, f32::MIN, f32::MIN], |r, t| [r[0].min(t[0]), r[1].min(t[1]), r[2].max(t[0]), r[3].max(t[1])]);
            // a font's quad as its filter draws it (`ringed`), with no bias:
            // that picks a nearest texel, and a font's mixes are its filter's;
            // under the HUD's Sharp the others' with none either, a whole
            // scale's pixel centres then on their texels' (the nearest texel)
            let t = &self.textures[q.tex];
            let (q, bias) = match (t.font, hud) {
                (true, _) => (ringed(q, t.width, t.height), 0.0),
                (false, HudFilter::Sharp) => (*q, 0.0),
                (false, HudFilter::Nearest) => (*q, SAMPLE_BIAS),
            };
            let p = |i: usize| Vertex {
                pos: [(ox + (q.pos[i][0] - bias) * sx) / tw * 2.0 - 1.0, 1.0 - (oy + (q.pos[i][1] - bias) * sy) / th * 2.0],
                uv: q.uv[i],
                color: q.color,
                rect,
            };
            v.extend([p(0), p(1), p(2), p(0), p(2), p(3)]);
        }
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("ui"), contents: bytemuck::cast_slice(&v), usage: wgpu::BufferUsages::VERTEX });
        pass.set_vertex_buffer(0, buf.slice(..));
        // Runs of one texture, blend mode and viewport, in order.
        let mut i = 0;
        while i < quads.len() {
            let (tex, blend, clip) = (quads[i].tex, quads[i].blend, quads[i].clip);
            let mut j = i;
            while j < quads.len() && quads[j].tex == tex && quads[j].blend == blend && quads[j].clip == clip {
                j += 1;
            }
            let [x0, y0, x1, y1] = clip.unwrap_or([0, 0, SCREEN_W as i32, SCREEN_H as i32]);
            let px = |v: i32, o: f32, k: f32, max: u32| (o + v as f32 * k).clamp(0.0, max as f32) as u32;
            let (a, b) = (px(x0, ox, sx, size.0), px(y0, oy, sy, size.1));
            let (c, d) = (px(x1, ox, sx, size.0), px(y1, oy, sy, size.1));
            if c <= a || d <= b {
                i = j;
                continue;
            }
            pass.set_scissor_rect(a, b, c - a, d - b);
            // a font's quads through its filter, the rest through the HUD's
            let t = &self.textures[tex];
            let (pipelines, bg) = match (t.font, text_filter(), hud) {
                (true, TextFilter::Sharp, _) => (&self.sharp, &t.filtered),
                (true, TextFilter::Smooth, _) => (&self.smooth, &t.filtered),
                (false, _, HudFilter::Sharp) => (&self.art_sharp, &t.filtered),
                (false, _, HudFilter::Nearest) => (&self.pipelines, &t.bg),
            };
            pass.set_pipeline(&pipelines[blend.min(4) as usize]);
            pass.set_bind_group(0, bg, &[]);
            pass.draw((i * 6) as u32..(j * 6) as u32, 0..1);
            i = j;
        }
        pass.set_scissor_rect(0, 0, size.0, size.1);
    }
}

#[cfg(test)]
mod tests {
    use super::{Anchor, Quad, Vertical, note, pack, place, rise, rose, settle, settle_along, side, top, viewport_of};

    /// A glyph `w` wide and 3 tall: every texel ink of colour `c` but its
    /// middle row's first, clear.
    fn glyph(code: u16, w: u8, c: [u8; 3]) -> esf::font::Glyph {
        let mut rgba = Vec::new();
        for y in 0..3 {
            for x in 0..w {
                rgba.extend(if y == 1 && x == 0 { [0, 0, 0, 0] } else { [c[0], c[1], c[2], 255] });
            }
        }
        esf::font::Glyph { code, width: w, rgba }
    }

    /// Whether every texel of the ring round `r` (x0, y0, x1, y1 exclusive)
    /// in a `width`-wide atlas is clear.
    fn ring_clear(atlas: &[u8], width: u32, r: [u32; 4]) -> bool {
        let a = |x: u32, y: u32| atlas[((y * width + x) * 4 + 3) as usize];
        (r[0] - 1..=r[2]).all(|x| a(x, r[1] - 1) == 0 && a(x, r[3]) == 0) && (r[1] - 1..=r[3]).all(|y| a(r[0] - 1, y) == 0 && a(r[2], y) == 0)
    }

    /// Each glyph is its own texels, at its rectangle, inside a clear ring
    /// (rows filled to the atlas's last column included), the clear texels
    /// next to its ink of the ink's colour; a code's first glyph is kept.
    /// The control: ink planted in a ring is seen.
    #[test]
    fn a_packed_glyph_keeps_a_clear_ring_of_its_inks_colour() {
        let red = [200, 10, 10];
        // the first two fill a 16-wide row, ring and all (1 + 6 + 2 + 6 + 1)
        let font = esf::font::Font { id: 1, height: 3, glyphs: vec![glyph(65, 6, red), glyph(66, 6, [10, 200, 10]), glyph(67, 1, [10, 10, 200]), glyph(68, 5, red), glyph(65, 2, [0, 0, 0])] };
        let (atlas, ah, glyphs) = pack(&font, 16);
        assert_eq!(ah, 2 * 5, "two rows of 3, each glyph's ring above and below");
        assert_eq!(glyphs[&65].0, 6, "a code's first glyph");
        for (k, g) in font.glyphs.iter().take(4).enumerate() {
            let uv = glyphs[&g.code].1;
            let r = [(uv[0] * 16.0).round() as u32, (uv[1] * ah as f32).round() as u32, (uv[2] * 16.0).round() as u32, (uv[3] * ah as f32).round() as u32];
            assert_eq!((r[2] - r[0], r[3] - r[1]), (g.width as u32, 3), "glyph {k}");
            for y in 0..3 {
                for x in 0..g.width as u32 {
                    let (d, s) = ((((r[1] + y) * 16 + r[0] + x) * 4) as usize, ((y * g.width as u32 + x) * 4) as usize);
                    let ink = g.rgba.chunks(4).find(|t| t[3] != 0).unwrap();
                    let want = if g.rgba[s + 3] == 0 { [ink[0], ink[1], ink[2], 0] } else { [g.rgba[s], g.rgba[s + 1], g.rgba[s + 2], 255] };
                    assert_eq!(atlas[d..d + 4], want, "glyph {k} texel {x},{y}: its own, a clear one of its ink's colour");
                }
            }
            assert!(ring_clear(&atlas, 16, r), "glyph {k}'s ring");
            let left = (((r[1] + 1) * 16 + r[0] - 1) * 4) as usize;
            let ink = g.rgba.chunks(4).find(|t| t[3] != 0).unwrap();
            assert_eq!(atlas[left..left + 3], ink[..3], "glyph {k}: the ring by its ink takes its colour");
        }
        let uv = glyphs[&66].1;
        assert_eq!((uv[2] * 16.0).round() as u32, 15, "the second glyph's ring is the row's last column");
        assert_eq!((glyphs[&67].1[1] * ah as f32).round() as u32, 6, "the third glyph on the next row");
        // the control
        let r = [1, 1, 7, 4];
        let mut planted = atlas.clone();
        planted[((4 * 16 + 3) * 4 + 3) as usize] = 255;
        assert!(ring_clear(&atlas, 16, r) && !ring_clear(&planted, 16, r), "ink in the ring is seen");
    }

    fn quad(x0: f32, x1: f32) -> Quad {
        Quad { tex: 0, blend: 0, pos: [[x0, 10.0], [x1, 10.0], [x1, 20.0], [x0, 20.0]], uv: [[0.0; 2]; 4], color: [1.0; 4], clip: None }
    }

    #[test]
    fn a_target_shows_whole_frame_pixels_beyond_the_frame_only_when_wider() {
        assert_eq!(side((640, 480)), 0.0);
        assert_eq!(side((1280, 960)), 0.0);
        assert_eq!(side((540, 960)), 0.0);
        // 16:9: the frame is 720 of 960 at 9/8 a pixel, 120 / 1.125 beyond each side
        assert_eq!(side((960, 540)), 106.0);
        assert_eq!(side((1920, 1080)), 106.0);
        assert_eq!(side((2560, 1080)), 248.0);
    }

    #[test]
    fn a_four_by_three_target_places_every_anchor_as_the_frame() {
        for a in [Anchor::Middle, Anchor::Left, Anchor::Right, Anchor::Span(-1, 202), Anchor::Wide, Anchor::Frame] {
            let (mut q, mut v) = (quad(5.0, 25.0), quad(5.0, 25.0));
            place(&mut q, None, a, 0.0);
            assert_eq!((q.pos, q.clip), (quad(5.0, 25.0).pos, None), "{a:?}");
            place(&mut v, Some([100, 200, 299, 259]), a, 0.0);
            assert_eq!(v.pos[0], [105.0, 210.0], "{a:?}");
            assert_eq!(v.clip, Some([100, 200, 300, 260]), "{a:?}");
        }
    }

    #[test]
    fn an_anchored_quad_keeps_its_distance_from_its_side() {
        let s = 106.0;
        let mut l = quad(10.0, 69.0);
        place(&mut l, None, Anchor::Left, s);
        assert_eq!([l.pos[0][0], l.pos[1][0]], [10.0 - s, 69.0 - s]);
        // cut to the frame moved with it: the target's left side, the
        // fraction of a pixel past the whole ones too
        assert_eq!(l.clip, Some([-107, 0, 534, 448]));
        let mut r = quad(430.0, 592.0);
        place(&mut r, Some([400, 20, 599, 59]), Anchor::Right, s);
        assert_eq!([r.pos[0][0], r.pos[1][0]], [830.0 + s, 992.0 + s]);
        assert_eq!(r.clip, Some([506, 20, 706, 60]));
        // the middle stays the frame's
        let mut m = quad(300.0, 340.0);
        place(&mut m, None, Anchor::Middle, s);
        assert_eq!((m.pos[0][0], m.clip), (300.0, None));
        // wide: placed as the frame, cut to the target's sides
        let mut w = quad(176.0 - s, 451.0 + s);
        place(&mut w, None, Anchor::Wide, s);
        assert_eq!([w.pos[0][0], w.pos[1][0]], [176.0 - s, 451.0 + s]);
        assert_eq!(w.clip, Some([-107, 0, 747, 448]));
    }

    /// A backdrop the frame's whole width runs the target's, its texture at
    /// its own scale (one texel a pixel, mirrored or not); a quad short of
    /// either side, under a viewport or on a 4:3 target stays as made.
    #[test]
    fn a_quad_the_frames_whole_width_runs_the_targets() {
        let s = 106.0;
        let mut b = quad(-1.0, 641.0);
        b.uv = [[0.0, 0.0], [642.0 / 64.0, 0.0], [642.0 / 64.0, 1.0], [0.0, 1.0]];
        place(&mut b, None, Anchor::Middle, s);
        assert_eq!([b.pos[0][0], b.pos[1][0], b.pos[2][0], b.pos[3][0]], [-108.0, 748.0, 748.0, -108.0]);
        let texel = 1.0 / 64.0;
        assert_eq!(b.uv[0], [-107.0 * texel, 0.0]);
        assert_eq!(b.uv[1][0], (642.0 + 107.0) * texel);
        assert_eq!(b.uv[3], [-107.0 * texel, 1.0]);
        assert!(b.clip.is_some_and(|c| c[0] < -1000 && c[2] > 1000), "cut to the target, {:?}", b.clip);
        // mirrored (u runs back)
        let mut m = quad(0.0, 640.0);
        m.uv = [[10.0, 0.0], [0.0, 0.0], [0.0, 1.0], [10.0, 1.0]];
        place(&mut m, None, Anchor::Middle, s);
        assert_eq!((m.uv[0][0], m.uv[1][0]), (10.0 + 107.0 / 64.0, -107.0 / 64.0));
        // the controls
        for (x0, x1, vp, side) in [(-1.0, 639.0, None, s), (1.0, 641.0, None, s), (-1.0, 641.0, Some([0, 0, 639, 447]), s), (-1.0, 641.0, None, 0.0)] {
            let mut q = quad(x0, x1);
            place(&mut q, vp, Anchor::Middle, side);
            assert_eq!([q.pos[0][0], q.pos[1][0]], [x0, x1], "{x0}..{x1} {vp:?} {side}");
        }
    }

    /// Under `Frame` a quad stays the frame's, cut to it: the frame's whole
    /// width too (the start menu's band), where the same quad with the frame
    /// (`Middle`) runs the target's (the control); a post at its edge stays
    /// at the edge.
    #[test]
    fn a_quad_kept_four_by_three_stays_the_frames() {
        let s = 106.0;
        let mut band = quad(-1.0, 641.0);
        place(&mut band, None, Anchor::Frame, s);
        assert_eq!(([band.pos[0][0], band.pos[1][0]], band.clip), ([-1.0, 641.0], None));
        let mut wide = quad(-1.0, 641.0);
        place(&mut wide, None, Anchor::Middle, s);
        assert_eq!([wide.pos[0][0], wide.pos[1][0]], [-108.0, 748.0]);
        let mut post = quad(629.0, 640.0);
        place(&mut post, None, Anchor::Frame, s);
        assert_eq!(([post.pos[0][0], post.pos[1][0]], post.clip), ([629.0, 640.0], None));
    }

    /// A window kept 4:3 (`Frame`) leaves the HUD at the target's sides,
    /// the compass included, even under art the frame's whole height and
    /// width (the user, 2026-10-23: "dont make compass hide"); the same
    /// backdrop made with the frame (`Middle`, a full-screen window's) takes
    /// the compass back under it (the control).
    #[test]
    fn a_window_kept_four_by_three_leaves_the_hud_at_the_sides() {
        let s = 106.0;
        let compass = (10.0, 69.0, 15.0, 84.0, 0, 1.0, Anchor::Left);
        let gauges = (69.0, 205.0, 36.0, 46.0, 0, 1.0, Anchor::Left);
        let band = (-1.0, 641.0, -1.0, 47.0, 0, 1.0, Anchor::Frame);
        let panel = (-1.0, 175.0, 48.0, 313.0, 0, 1.0, Anchor::Frame);
        let q = layer(s, &[compass, gauges, band, panel]);
        assert_eq!((q[0].pos[0][0], q[1].pos[0][0]), (10.0 - s, 69.0 - s), "the compass and the gauges at the side");
        assert_eq!([q[2].pos[0][0], q[2].pos[1][0]], [-1.0, 641.0], "the band the frame's");
        let whole = (-1.0, 641.0, -1.0, 331.0, 0, 1.0, Anchor::Frame);
        assert_eq!(layer(s, &[compass, whole])[0].pos[0][0], 10.0 - s);
        let backdrop = (-1.0, 641.0, -1.0, 331.0, 0, 1.0, Anchor::Middle);
        assert_eq!(layer(s, &[compass, backdrop])[0].pos[0][0], 10.0);
    }

    /// The whole screen set as the viewport (what `VIUITextScroll` leaves:
    /// it saves the viewport, cuts to its window and sets it back) is no
    /// viewport, so a backdrop the frame's width made after it still runs the
    /// target's; a smaller viewport stays one.
    #[test]
    fn the_whole_screen_set_back_is_no_viewport() {
        assert_eq!(viewport_of([0, 0, 639, 447]), None);
        assert_eq!(viewport_of([10, 20, 300, 400]), Some([10, 20, 300, 400]));
        assert_eq!(viewport_of([0, 0, 640, 448]), Some([0, 0, 640, 448]));
        let s = 106.0;
        let mut b = quad(-1.0, 641.0);
        place(&mut b, viewport_of([0, 0, 639, 447]), Anchor::Middle, s);
        assert_eq!([b.pos[0][0], b.pos[1][0]], [-108.0, 748.0]);
        // control: kept as a viewport (before 2026-10-21) the backdrop
        // stayed the frame's, the world showing at its sides
        let mut k = quad(-1.0, 641.0);
        place(&mut k, Some([0, 0, 639, 447]), Anchor::Middle, s);
        assert_eq!([k.pos[0][0], k.pos[1][0]], [-1.0, 641.0]);
    }

    /// On a 4:3 target neither change moves a pixel: a viewport set back to
    /// the whole screen cuts to the frame as no viewport does, and nothing is
    /// moved to a side for a window to take back.
    #[test]
    fn a_four_by_three_target_is_the_games_whatever_the_viewport() {
        for vp in [None, Some([0, 0, 639, 447]), viewport_of([0, 0, 639, 447])] {
            let mut q = quad(-1.0, 641.0);
            place(&mut q, vp, Anchor::Middle, 0.0);
            assert_eq!([q.pos[0][0], q.pos[1][0]], [-1.0, 641.0]);
            assert_eq!(q.clip.unwrap_or([0, 0, 640, 448]), [0, 0, 640, 448], "{vp:?}");
        }
        let compass = (10.0, 69.0, 15.0, 84.0, 0, 1.0, Anchor::Left);
        let edit = (17.0, 468.0, 28.0, 140.0, 0, 1.0, Anchor::Middle);
        let backdrop = (-1.0, 641.0, -1.0, 331.0, 0, 1.0, Anchor::Middle);
        assert_eq!(layer(0.0, &[compass, edit])[0].pos[0], [10.0, 15.0]);
        assert_eq!(layer(0.0, &[compass, backdrop])[0].pos[0], [10.0, 15.0]);
    }

    #[test]
    fn a_span_is_one_stretch_so_tiles_stay_edge_to_edge_and_reach_the_right_piece() {
        // The chat backdrop: the HUD's piece at -1..202 (tiled as BlitRect
        // tiles 64-wide art), the chat's at 200..642 with the right side.
        let s = 213.0;
        let a = Anchor::Span(-1, 202);
        let edges = [-1.0, 63.0, 127.0, 191.0, 202.0];
        let mut tiles: Vec<Quad> = edges.windows(2).map(|e| quad(e[0], e[1])).collect();
        for t in &mut tiles {
            place(t, None, a, s);
        }
        for w in tiles.windows(2) {
            assert_eq!(w[0].pos[1][0], w[1].pos[0][0]);
        }
        assert_eq!(tiles[0].pos[0][0], -1.0 - s);
        let mut chat = quad(200.0, 642.0);
        place(&mut chat, None, Anchor::Right, s);
        let end = tiles.last().unwrap().pos[1][0];
        assert!(end >= chat.pos[0][0] && end - chat.pos[0][0] == 2.0, "{end} vs {}", chat.pos[0][0]);
        // each tile stretched on its own would leave the strip in pieces
        let mut alone = quad(63.0, 127.0);
        place(&mut alone, None, Anchor::Span(63, 127), s);
        assert_ne!(alone.pos[0][0], tiles[1].pos[0][0]);
    }

    /// Quads made as `Ui::push` makes them (each `(x0, x1, y0, y1, blend,
    /// alpha, anchor)`), then `Ui::end`'s settling; their left edges.
    fn layer(side: f32, made: &[(f32, f32, f32, f32, u8, f32, Anchor)]) -> Vec<Quad> {
        let (mut quads, mut sided, mut opaque) = (Vec::new(), Vec::new(), Vec::new());
        for &(x0, x1, y0, y1, blend, alpha, a) in made {
            let mut q = Quad { tex: 0, blend, pos: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], uv: [[0.0; 2]; 4], color: [1.0, 1.0, 1.0, alpha], clip: None };
            place(&mut q, None, a, side);
            note(quads.len(), &q, a, side, &mut sided, &mut opaque);
            quads.push(q);
        }
        settle(&mut quads, &sided, &opaque);
        quads
    }

    #[test]
    fn a_menu_takes_back_under_it_the_pieces_it_covers_on_a_four_by_three_screen() {
        let s = 106.0;
        // the compass, then a full-screen window's backdrop (642x332 at -1,-1)
        let compass = (10.0, 69.0, 15.0, 84.0, 0, 1.0, Anchor::Left);
        let backdrop = (-1.0, 641.0, -1.0, 331.0, 0, 1.0, Anchor::Middle);
        let q = layer(s, &[compass, backdrop]);
        assert_eq!(q[0].pos[0], [10.0, 15.0]);
        assert_eq!(q[0].clip, Some([0, 0, 640, 448]));
        // the target's bar below it stays at the side, and so does the chat box at the right
        let target = (20.0, 194.0, 373.0, 399.0, 3, 1.0, Anchor::Left);
        let chat = (191.0, 653.0, 380.0, 449.0, 3, 1.0, Anchor::Right);
        let q = layer(s, &[target, chat, backdrop]);
        assert_eq!((q[0].pos[0][0], q[1].pos[0][0]), (20.0 - s, 191.0 + s));
        // nothing moves on a 4:3 screen
        let q = layer(0.0, &[compass, backdrop]);
        assert_eq!((q[0].pos[0][0], q[0].clip), (10.0, None));
    }

    /// A box over a piece's 4:3 place leaves it at its side: only a
    /// full-screen window, whose backdrop is the frame's whole width, takes
    /// it back. F3 2026-10-21: the say box (`VIWndEdit` at the top) took the
    /// compass and the gauges back under itself, off the window's edge.
    #[test]
    fn a_box_leaves_the_hud_at_the_sides() {
        let s = 213.0;
        let compass = (10.0, 69.0, 15.0, 84.0, 0, 1.0, Anchor::Left);
        let gauges = (72.0, 130.0, 30.0, 70.0, 0, 1.0, Anchor::Left);
        // the say box's frame, opaque, over both on a 4:3 screen
        let edit = (17.0, 468.0, 28.0, 140.0, 0, 1.0, Anchor::Middle);
        let q = layer(s, &[compass, gauges, edit]);
        assert_eq!((q[0].pos[0][0], q[1].pos[0][0]), (10.0 - s, 72.0 - s));
        // a full-screen window's backdrop still takes them back (its own
        // runs the target's width over the sides)
        let backdrop = (-1.0, 641.0, -1.0, 331.0, 0, 1.0, Anchor::Middle);
        let q = layer(s, &[compass, gauges, edit, backdrop]);
        assert_eq!((q[0].pos[0][0], q[1].pos[0][0]), (10.0, 72.0));
        // one under the whole screen's viewport (cut to the frame, not
        // widened) is still a full-screen window's
        let mut quads = vec![Quad { tex: 0, blend: 0, pos: [[10.0, 15.0], [69.0, 15.0], [69.0, 84.0], [10.0, 84.0]], uv: [[0.0; 2]; 4], color: [1.0; 4], clip: None }];
        let mut b = Quad { tex: 0, blend: 0, pos: [[-1.0, -1.0], [641.0, -1.0], [641.0, 331.0], [-1.0, 331.0]], uv: [[0.0; 2]; 4], color: [1.0; 4], clip: None };
        let (mut sided, mut opaque) = (Vec::new(), Vec::new());
        place(&mut quads[0], None, Anchor::Left, s);
        note(0, &quads[0], Anchor::Left, s, &mut sided, &mut opaque);
        place(&mut b, Some([0, 0, 639, 447]), Anchor::Middle, s);
        note(1, &b, Anchor::Middle, s, &mut sided, &mut opaque);
        quads.push(b);
        settle(&mut quads, &sided, &opaque);
        assert_eq!(quads[0].pos[0][0], 10.0);
    }

    #[test]
    fn only_an_opaque_window_made_after_a_piece_covers_it() {
        let s = 106.0;
        let compass = (10.0, 69.0, 15.0, 84.0, 0, 1.0, Anchor::Left);
        // a box that leaves the corner alone (the options)
        let boxed = layer(s, &[compass, (120.0, 520.0, 100.0, 330.0, 0, 1.0, Anchor::Middle)]);
        assert_eq!(boxed[0].pos[0][0], 10.0 - s);
        // a backdrop drawn before the piece is under it, not over it
        let before = layer(s, &[(-1.0, 641.0, -1.0, 331.0, 0, 1.0, Anchor::Middle), compass]);
        assert_eq!(before[1].pos[0][0], 10.0 - s);
        // a translucent box does not hide it; one blended at full alpha does
        let dim = layer(s, &[compass, (-1.0, 641.0, -1.0, 331.0, 3, 0.75, Anchor::Middle)]);
        assert_eq!(dim[0].pos[0][0], 10.0 - s);
        let full = layer(s, &[compass, (-1.0, 641.0, -1.0, 331.0, 3, 1.0, Anchor::Middle)]);
        assert_eq!(full[0].pos[0][0], 10.0);
        // a piece of another side is no window
        let sided = layer(s, &[compass, (-1.0, 641.0, -1.0, 331.0, 0, 1.0, Anchor::Right)]);
        assert_eq!(sided[0].pos[0][0], 10.0 - s);
    }

    #[test]
    fn a_target_shows_whole_frame_pixels_above_and_below_only_when_narrower() {
        assert_eq!(top((640, 480)), 0.0);
        assert_eq!(top((1920, 1080)), 0.0);
        assert_eq!(top((640, 448)), 0.0);
        // two windows side by side: the frame 798 by 598.5 of 828, 114.75
        // above it at 1.336 a frame pixel
        assert_eq!(top((798, 828)), 85.0);
        // a phone held upright (9:16)
        assert_eq!(top((540, 960)), 306.0);
        // and the sides then show nothing beyond the frame
        assert_eq!((side((798, 828)), side((540, 960))), (0.0, 0.0));
    }

    /// A HUD piece's quad (made under `Left` or `Right`) in a taller target
    /// keeps its distance from the top or the bottom, its cut moved with it
    /// to the target's edge (past the whole frame pixels, the fraction of
    /// one too); `Middle` leaves it with the frame.
    #[test]
    fn a_hud_piece_keeps_its_distance_from_the_top_or_the_bottom() {
        let t = 85.0;
        let mut compass = quad(10.0, 69.0);
        place(&mut compass, None, Anchor::Left, 0.0);
        rise(&mut compass, Anchor::Left, Vertical::Top, t);
        assert_eq!([compass.pos[0], compass.pos[2]], [[10.0, 10.0 - t], [69.0, 20.0 - t]]);
        assert_eq!(compass.clip, Some([0, -86, 640, 363]));
        let mut target = quad(20.0, 194.0);
        place(&mut target, Some([0, 300, 300, 420]), Anchor::Left, 0.0);
        rise(&mut target, Anchor::Left, Vertical::Bottom, t);
        assert_eq!(target.pos[0], [20.0, 310.0 + t]);
        assert_eq!(target.clip, Some([0, 385, 301, 506]));
        let mut chat = quad(191.0, 653.0);
        place(&mut chat, None, Anchor::Right, 0.0);
        rise(&mut chat, Anchor::Right, Vertical::Bottom, t);
        assert_eq!((chat.pos[0], chat.clip), ([191.0, 10.0 + t], Some([0, 85, 640, 448 + 86])));
        let mut middle = quad(300.0, 340.0);
        place(&mut middle, None, Anchor::Left, 0.0);
        rise(&mut middle, Anchor::Left, Vertical::Middle, t);
        assert_eq!((middle.pos[0], middle.clip), ([300.0, 10.0], None));
        assert_eq!(rose(Anchor::Left, Vertical::Top, t), Some(-t));
        assert_eq!(rose(Anchor::Right, Vertical::Bottom, t), Some(t));
        assert_eq!(rose(Anchor::Left, Vertical::Middle, t), None);
    }

    /// The frame's own quads never rise (the boxes, the menus, a window kept
    /// 4:3, a band across the whole target), whatever `vertical` is left at;
    /// and a target 4:3 or wider moves nothing up or down (the control:
    /// the same piece rises in a taller one).
    #[test]
    fn only_a_hud_piece_rises_and_only_in_a_taller_target() {
        for a in [Anchor::Middle, Anchor::Frame, Anchor::Wide] {
            let mut q = quad(5.0, 25.0);
            rise(&mut q, a, Vertical::Top, 85.0);
            assert_eq!((q.pos, q.clip), (quad(5.0, 25.0).pos, None), "{a:?}");
            assert_eq!(rose(a, Vertical::Bottom, 85.0), None, "{a:?}");
        }
        for (a, s) in [(Anchor::Left, 0.0), (Anchor::Right, 0.0), (Anchor::Left, 106.0), (Anchor::Right, 106.0)] {
            let (mut q, mut want) = (quad(5.0, 25.0), quad(5.0, 25.0));
            place(&mut q, None, a, s);
            place(&mut want, None, a, s);
            rise(&mut q, a, Vertical::Top, 0.0);
            assert_eq!((q.pos, q.clip), (want.pos, want.clip), "{a:?} {s}");
            assert_eq!(rose(a, Vertical::Bottom, 0.0), None);
        }
        let mut q = quad(5.0, 25.0);
        rise(&mut q, Anchor::Left, Vertical::Top, 85.0);
        assert_ne!(q.pos, quad(5.0, 25.0).pos);
    }

    /// Quads made under `Left` (and `vertical`) in a taller target, then a
    /// full-screen window's backdrop with the frame: `Ui::end`'s settling up
    /// and down, their top edges.
    fn risen(t: f32, made: &[(f32, f32, f32, f32, u8, Anchor, Vertical)]) -> Vec<Quad> {
        let (mut quads, mut moved, mut sided, mut opaque) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for &(x0, x1, y0, y1, blend, a, v) in made {
            let mut q = Quad { tex: 0, blend, pos: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], uv: [[0.0; 2]; 4], color: [1.0; 4], clip: None };
            place(&mut q, None, a, 0.0);
            rise(&mut q, a, v, t);
            note(quads.len(), &q, a, 0.0, &mut sided, &mut opaque);
            moved.extend(rose(a, v, t).map(|d| (quads.len(), d)));
            quads.push(q);
        }
        assert!(sided.is_empty(), "nothing to a side in a taller target");
        settle_along(&mut quads, &moved, &opaque, 1);
        quads
    }

    /// As across a wider target: a full-screen window's backdrop made after a
    /// risen piece takes it back under it where it covers its 4:3 place, cut
    /// to the frame again; a box leaves it up, and so does a backdrop made
    /// before it, or one kept 4:3 (`Frame`).
    #[test]
    fn a_menu_takes_back_what_it_covers_up_and_down_too() {
        let t = 85.0;
        let compass = (10.0, 69.0, 15.0, 84.0, 0, Anchor::Left, Vertical::Top);
        let report = (20.0, 160.0, 421.0, 441.0, 0, Anchor::Left, Vertical::Bottom);
        let backdrop = (-1.0, 641.0, -1.0, 449.0, 0, Anchor::Middle, Vertical::Middle);
        let q = risen(t, &[compass, report, backdrop]);
        assert_eq!((q[0].pos[0][1], q[1].pos[0][1]), (15.0, 421.0));
        assert_eq!((q[0].clip, q[1].clip), (Some([0, 0, 640, 448]), Some([0, 0, 640, 448])), "cut to the frame again");
        let edit = (17.0, 468.0, 28.0, 140.0, 0, Anchor::Middle, Vertical::Middle);
        let q = risen(t, &[compass, report, edit]);
        assert_eq!((q[0].pos[0][1], q[1].pos[0][1]), (15.0 - t, 421.0 + t));
        let q = risen(t, &[backdrop, compass]);
        assert_eq!(q[1].pos[0][1], 15.0 - t);
        let kept = (-1.0, 641.0, -1.0, 449.0, 0, Anchor::Frame, Vertical::Middle);
        assert_eq!(risen(t, &[compass, kept])[0].pos[0][1], 15.0 - t);
    }
}
