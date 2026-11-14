//! A texture drawn in another colour: a part of a body a player recoloured, a
//! custom race's option (`net::raceopts`, go-eqoa-server's `Recolour`). The
//! renderer resolves a recoloured texture as any other, by an id that names it
//! ([`recolored`]): [`crate::LibraryTextures`] hands out its source's levels (a
//! texture pack's too) with every texel recoloured, under a key of its own, so a
//! part's texture and the same texture on the rest of the body (or on another
//! creature) are two textures, and nothing changes for a texture no one
//! recoloured.
//!
//! A recolour is a mode in the high byte and three bytes, laid on each texel's
//! hue, saturation and lightness (HSL), its alpha kept: [`DYE`] takes the hue
//! (256ths of a turn) and the saturation (255ths) as given and scales the
//! lightness (128ths: 128 keeps it), as an image editor's colourise, one colour
//! over the part's own shading; [`TONE`] turns the hue and scales the saturation
//! and the lightness (128ths each), the part's own colours shifted. [`SKIN`] (a
//! game race's skin shade, go-eqoa-server's `Shade`) moves a skin texture's
//! own colour to the shade's (hue in 256ths of a turn, saturation and lightness
//! in 255ths, all absolute): [`SkinRef`] is the texture's skin (its texels'
//! mean chroma and median lightness), each texel moved by the difference as far
//! as it is skin-like (its chroma near the skin's: an eye's white, an iris or
//! a strap keeps its own), its lightness by a curve through the skin's that
//! keeps black and white where they are. [`TATTOO`] (a barbarian's tattoo,
//! go-eqoa-server's `Tattoo`) takes a face's own woad off and lays a pattern's
//! (`crate::tattoo`), from the faces it is read off: [`crate::LibraryTextures`]
//! lays it, as it needs the library; here it changes no colour. [`MUL`] (an
//! armour piece's colour, baked) multiplies every texel by a colour, as a
//! layer's colour does; [`TRIM`] (go-eqoa-server's trim dye) draws a plate
//! piece's gold trim ([`trim_weight`]) in a colour's hue at the trim's own
//! brightness ([`trim_glow`]), every other texel as it is, the trim found on
//! the texture it was made from at the first ([`root_of`]: through a [`MUL`]
//! laid before it, [`crate::LibraryTextures`]); [`trimmed`] names the pair. 0,
//! or a mode it does not know, is the texture as it is.

use std::collections::HashMap;
use std::sync::Mutex;

pub use crate::trimart::Trim;

/// One colour over a part's own shading.
pub const DYE: u32 = 1;
/// A part's own colours shifted.
pub const TONE: u32 = 2;
/// A skin texture moved to a shade (go-eqoa-server's `Shade`).
pub const SKIN: u32 = 3;
/// A barbarian's face's woad taken off and a pattern's laid (go-eqoa-server's
/// `Tattoo`: the pattern in byte 2): `crate::tattoo`.
pub const TATTOO: u32 = 4;
/// Every texel times a colour (RGB in the low three bytes; 255 keeps a
/// channel): a layer's colour baked into its texture, so the layer can take
/// white and its trim keep its own colour ([`trimmed`]).
pub const MUL: u32 = 5;
/// A plate piece's gold trim in a colour (RGB in the low three bytes), its
/// other texels as they are ([`trim_weight`], [`trim_glow`]).
pub const TRIM: u32 = 6;
/// Not a colour: a plate texture's trim as a mask ([`trim_mask`]), which the
/// trim's aura (`crate::trimglow`) samples; drawn by no layer.
pub const TRIM_MASK: u32 = 7;

/// Whether `r` changes anything (a known mode).
pub fn changes(r: u32) -> bool {
    matches!(r >> 24, DYE | TONE | SKIN | TATTOO | MUL | TRIM | TRIM_MASK)
}

/// How much of a texel is a plate piece's gold trim, 0 to 1: its hue between
/// 25 and 70 degrees (none below 15 or past 80), its saturation (HSV) past
/// 0.25 (none below 0.12) and its value past 0.18 (none below 0.08). Measured
/// on the disc's plate (sets 4 and 9, `vi::cust::TRIMMED`): the gold and its
/// shading, and on set 4 nothing of the grey steel; the brown leather of
/// sets 2, 5 and 7 would read as gold too, which is why only the textures in
/// that list are ever trimmed.
pub fn trim_weight(c: [u8; 3]) -> f32 {
    let f = c.map(|v| v as f32 / 255.0);
    let (mx, mn) = (f[0].max(f[1]).max(f[2]), f[0].min(f[1]).min(f[2]));
    if mx <= 0.0 || mx - mn <= f32::EPSILON {
        return 0.0;
    }
    let (h, _, _) = hsl(f);
    let deg = h * 360.0;
    let ramp = |x: f32, a: f32, b: f32| ((x - a) / (b - a)).clamp(0.0, 1.0);
    let hue = ramp(deg, 15.0, 25.0).min(1.0 - ramp(deg, 70.0, 80.0));
    hue * ramp((mx - mn) / mx, 0.12, 0.25) * ramp(mx, 0.08, 0.18)
}

/// A trim texel `o` in the hue of `c`, as the eqoa crate's `aura::glow` lays
/// a held item's glow: `c`'s hue and saturation at `o`'s brightness (its
/// largest channel), times how much of a colour `c` is (a grey keeps its
/// lightness of it: white the trim's own brightness, black none).
pub fn trim_glow(o: [u8; 3], c: [u8; 3]) -> [u8; 3] {
    let v = o[0].max(o[1]).max(o[2]) as f32;
    let hi = c[0].max(c[1]).max(c[2]) as f32;
    if v <= 0.0 || hi <= 0.0 {
        return [0; 3];
    }
    let lo = c[0].min(c[1]).min(c[2]) as f32;
    let sat = (hi - lo) / hi;
    let k = sat + (1.0 - sat) * hi / 255.0;
    let s = v * k / hi;
    c.map(|x| (x as f32 * s).round().clamp(0.0, 255.0) as u8)
}

/// How far the trim's halo bleeds off it, as a fraction of a texture's width
/// ([`trim_mask`]'s blur).
pub const HALO: f32 = 1.0 / 20.0;

/// A plate texture's trim as a mask for the trim's aura (`crate::trimglow`):
/// [`trim_mask_of`] by the plate's gold ([`Trim::Gold`]).
pub fn trim_mask(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    trim_mask_of(rgba, w, h, Trim::Gold)
}

/// A texture's trim as a mask for the trim's aura (`crate::trimglow`), a
/// texel each: red the trim's weight (`trim`'s, `crate::trimart`), green its
/// halo (the weight blurred [`HALO`] of the width round, twice, so it fades
/// off the trim's edges onto the rest), blue the texel's brightness (its
/// largest channel, the trim's own shading) and alpha the texel's own (a hole
/// stays a hole). `w` by `h` texels of `rgba`; a size that does not fit is all
/// clear.
pub fn trim_mask_of(rgba: &[u8], w: u32, h: u32, trim: Trim) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return vec![0; rgba.len()];
    }
    let weight = trim_weights(rgba, w, h, trim);
    let r = ((w as f32 * HALO).round() as usize).max(1);
    let mut halo = weight.clone();
    for _ in 0..2 {
        halo = box_blur(&halo, w, h, r);
    }
    let mut out = vec![0u8; rgba.len()];
    for (k, p) in rgba.chunks_exact(4).take(w * h).enumerate() {
        let b = |x: f32| (x * 255.0).round().clamp(0.0, 255.0) as u8;
        // the halo at least the trim's own, and lifted so that its middle
        // reaches the trim's full weight
        let g = (halo[k] * 2.0).min(1.0).max(weight[k]);
        out[4 * k..4 * k + 4].copy_from_slice(&[b(weight[k]), b(g), p[0].max(p[1]).max(p[2]), p[3]]);
    }
    out
}

/// `v` (`w` by `h`) blurred by a box `r` round, across then down, its edges
/// clamped.
fn box_blur(v: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let n = (2 * r + 1) as f32;
    let pass = |src: &[f32], len: usize, lines: usize, at: &dyn Fn(usize, usize) -> usize| {
        let mut out = vec![0.0f32; src.len()];
        for line in 0..lines {
            for i in 0..len {
                let lo = i.saturating_sub(r);
                let hi = (i + r).min(len - 1);
                // the clamped edge counts as many times as the box reaches past it
                let mut sum: f32 = (lo..=hi).map(|j| src[at(line, j)]).sum();
                sum += src[at(line, 0)] * (r.saturating_sub(i)) as f32;
                sum += src[at(line, len - 1)] * ((i + r).saturating_sub(len - 1)) as f32;
                out[at(line, i)] = sum / n;
            }
        }
        out
    };
    let across = pass(v, w, h, &|y, x| y * w + x);
    pass(&across, h, w, &|x, y| y * w + x)
}

/// Each texel's trim weight by `trim` (`w` by `h` texels of `rgba`), at its
/// centre.
fn trim_weights(rgba: &[u8], w: usize, h: usize, trim: Trim) -> Vec<f32> {
    rgba.chunks_exact(4).take(w * h).enumerate().map(|(k, p)| trim.weight([p[0], p[1], p[2]], ((k % w) as f32 + 0.5) / w as f32, ((k / w) as f32 + 0.5) / h as f32)).collect()
}

/// Texel `px` with its trim (read off `root`, the texel of the texture it was
/// made from) in colour `c`: `px` where `root` is no trim, the trim's glow in
/// `c` where it wholly is, between by [`trim_weight`].
fn trim_texel(px: [u8; 3], root: [u8; 3], c: [u8; 3]) -> [u8; 3] {
    trim_texel_by(px, root, c, trim_weight(root))
}

/// [`trim_texel`] at a weight of `w`.
fn trim_texel_by(px: [u8; 3], root: [u8; 3], c: [u8; 3], w: f32) -> [u8; 3] {
    if w <= 0.0 {
        return px;
    }
    let g = trim_glow(root, c);
    std::array::from_fn(|k| (px[k] as f32 + (g[k] as f32 - px[k] as f32) * w).round().clamp(0.0, 255.0) as u8)
}

/// Every level of a texture with its trim in `r`'s colour ([`TRIM`]), the
/// trim (`trim`'s texels, `crate::trimart`) read off `root`'s levels (the
/// texture it was made from, each level `width` texels across, the same sizes
/// as `levels`; a level without its root's is left as it is).
pub fn trim_levels(levels: &mut [Vec<u8>], root: &[(Vec<u8>, u32)], r: u32, trim: Trim) {
    let c = rgb_of(r);
    for (l, (rl, rw)) in levels.iter_mut().zip(root) {
        let w = *rw as usize;
        if l.len() != rl.len() || w == 0 || rl.len() % (4 * w) != 0 {
            continue;
        }
        let weight = trim_weights(rl, w, rl.len() / 4 / w, trim);
        for ((px, rp), &k) in l.chunks_exact_mut(4).zip(rl.chunks_exact(4)).zip(&weight) {
            let out = trim_texel_by([px[0], px[1], px[2]], [rp[0], rp[1], rp[2]], c, k);
            px[..3].copy_from_slice(&out);
        }
    }
}

fn rgb_of(r: u32) -> [u8; 3] {
    [(r >> 16) as u8, (r >> 8) as u8, r as u8]
}

/// The id an armour piece's texture `source` with gold trim is drawn by when
/// the piece's colour is `body` and its trim's `trim` (go-eqoa-server's trim
/// dye): its texels times `body` ([`MUL`], none for white) with the trim in
/// `trim` over them ([`TRIM`]). The layer that draws it then takes white (its
/// alpha the piece's): the trim keeps its colour, the rest is as the layer's
/// colour made it. `taken` as [`recolored`]'s.
pub fn trimmed(source: i32, body: [u8; 3], trim: [u8; 3], taken: impl Fn(i32) -> bool) -> i32 {
    let rgb = |c: [u8; 3]| (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32;
    let s = if body == [255; 3] { source } else { recolored(source, MUL << 24 | rgb(body), &taken) };
    recolored(s, TRIM << 24 | rgb(trim), &taken)
}

/// The texture whose texels an id's alpha is: for a [`trimmed`] id (made by
/// [`MUL`] and [`TRIM`] alone, which keep every alpha) the texture it was made
/// from; any other id itself.
pub fn alpha_source(id: i32) -> i32 {
    let mut at = id;
    for _ in 0..16 {
        match source_of(at) {
            Some((source, r)) if matches!(r >> 24, MUL | TRIM) => at = source,
            Some(_) => return id,
            None => return at,
        }
    }
    id
}

/// A texture's skin, as [`SKIN`] moves it: its texels' mean hue (turns, by
/// their saturation), mean saturation, median lightness and median chroma (an
/// RGB's largest less its smallest), each 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkinRef {
    pub hue: f32,
    pub sat: f32,
    pub light: f32,
    pub chroma: f32,
}

impl SkinRef {
    /// The skin of an RGBA image's texels that show (alpha above 0); a plain
    /// mid grey for an image with none.
    pub fn of(rgba: &[u8]) -> SkinRef {
        let (mut v, mut sat, mut n) = ([0.0f32; 2], 0.0f32, 0usize);
        let (mut ls, mut cs) = (Vec::with_capacity(rgba.len() / 4), Vec::with_capacity(rgba.len() / 4));
        for px in rgba.chunks_exact(4).filter(|p| p[3] > 0) {
            let c = [px[0], px[1], px[2]].map(|v| v as f32 / 255.0);
            let (h, s, l) = hsl(c);
            let a = h * std::f32::consts::TAU;
            v[0] += s * a.cos();
            v[1] += s * a.sin();
            sat += s;
            n += 1;
            ls.push(l);
            cs.push(c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]));
        }
        if n == 0 {
            return SkinRef { hue: 0.0, sat: 0.0, light: 0.5, chroma: 0.0 };
        }
        ls.sort_by(f32::total_cmp);
        cs.sort_by(f32::total_cmp);
        SkinRef { hue: (v[1].atan2(v[0]) / std::f32::consts::TAU).rem_euclid(1.0), sat: sat / n as f32, light: ls[ls.len() / 2], chroma: cs[cs.len() / 2] }
    }

    /// One colour's skin: the colour itself (an untextured layer's).
    fn of_colour(c: [f32; 3]) -> SkinRef {
        let (hue, sat, light) = hsl(c);
        SkinRef { hue, sat, light, chroma: c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]) }
    }
}

/// `c` moved from the skin `from` to shade `r` ([`SKIN`]), as far as it is
/// skin. Its colour (hue and saturation) moves wholly for a hue within 25
/// degrees of the skin's, not at all past 40 (a troll's brown marks keep
/// theirs); its lightness wholly within 40, not at all past 90 (the marks
/// lighten and darken with the skin, an iris does not). A texel of almost no
/// colour (an eye's white, a brow) moves by how much it has, but on a skin of
/// almost none itself (a dark elf's) wholly. The hue becomes the shade's plus
/// a quarter of its own turn from the skin's, the saturation the shade's plus a
/// third of its own difference from the skin's, the lightness follows a curve
/// through (the skin's, the shade's) that keeps black and white where they
/// are.
fn skin(c: [f32; 3], r: u32, from: &SkinRef) -> [f32; 3] {
    let (th, ts, tl) = (((r >> 16) & 0xFF) as f32 / 256.0, ((r >> 8) & 0xFF) as f32 / 255.0, (r & 0xFF) as f32 / 255.0);
    let (h, s, l) = hsl(c);
    let chroma = c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]);
    let turn = |a: f32| (a + 0.5).rem_euclid(1.0) - 0.5;
    let d = turn(h - from.hue).abs() * 360.0;
    let within = |near: f32, far: f32| (1.0 - (d - near) / (far - near)).clamp(0.0, 1.0);
    let grey = if from.chroma < 0.08 && chroma < 0.06 {
        Some(1.0)
    } else if chroma < 0.06 {
        Some(chroma / 0.06)
    } else {
        None
    };
    let (wc, wl) = match grey {
        Some(1.0) => (1.0, 1.0),
        Some(g) => (g * within(25.0, 40.0), g * within(40.0, 90.0)),
        None => (within(25.0, 40.0), within(40.0, 90.0)),
    };
    if wl <= 0.0 {
        return c;
    }
    // the shade's hue, a quarter of the texel's own turn from the skin's kept
    // (a troll's yellow-brown creases, a turn from its green, would have gone
    // red at the whole of it), the shade's saturation a third of its own
    // difference
    let nh = h + wc * turn(th + turn(h - from.hue) * 0.25 - h);
    let ns = s + wc * (ts + (s - from.sat) / 3.0 - s);
    let m = from.light.clamp(0.02, 0.98);
    let curve = if l <= m { l * tl / m } else { tl + (l - m) * (1.0 - tl) / (1.0 - m) };
    let nl = l + wl * (curve - l);
    rgb(nh.rem_euclid(1.0), ns.clamp(0.0, 1.0), nl.clamp(0.0, 1.0))
}

/// An RGB colour's hue (turns, 0..1), saturation and lightness (HSL), for
/// `crate::tattoo`.
pub(crate) fn hsl_of(c: [f32; 3]) -> (f32, f32, f32) {
    hsl(c)
}

/// An RGB colour's hue (turns, 0..1), saturation and lightness (HSL).
fn hsl(c: [f32; 3]) -> (f32, f32, f32) {
    let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    let l = (mx + mn) * 0.5;
    if mx - mn <= f32::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = mx - mn;
    let s = if l > 0.5 { d / (2.0 - mx - mn) } else { d / (mx + mn) };
    let h = if mx == c[0] {
        (c[1] - c[2]) / d + if c[1] < c[2] { 6.0 } else { 0.0 }
    } else if mx == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    (h / 6.0, s, l)
}

/// HSL back to RGB.
fn rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    if s <= 0.0 {
        return [l; 3];
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let ch = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [ch(h + 1.0 / 3.0), ch(h), ch(h - 1.0 / 3.0)]
}

/// One colour recoloured by `r` (its RGB; alpha is never touched).
pub fn recolor_rgb(c: [u8; 3], r: u32) -> [u8; 3] {
    if !changes(r) || r >> 24 == TATTOO {
        return c;
    }
    match r >> 24 {
        MUL => {
            let m = rgb_of(r);
            return std::array::from_fn(|k| ((c[k] as u32 * m[k] as u32 + 127) / 255) as u8);
        }
        // the texel its own root
        TRIM => return trim_texel(c, c, rgb_of(r)),
        _ => {}
    }
    let (a, b, d) = (((r >> 16) & 0xFF) as f32, ((r >> 8) & 0xFF) as f32, (r & 0xFF) as f32);
    if r >> 24 == SKIN {
        let f = c.map(|v| v as f32 / 255.0);
        return skin(f, r, &SkinRef::of_colour(f)).map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8);
    }
    let (h, s, l) = hsl(c.map(|v| v as f32 / 255.0));
    let (h, s, l) = if r >> 24 == DYE { (a / 256.0, b / 255.0, l * d / 128.0) } else { (h + a / 256.0, s * b / 128.0, l * d / 128.0) };
    rgb(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0)).map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// Every texel of an RGBA image recoloured by `r`, its alpha kept (a [`SKIN`]
/// shade from the image's own skin).
pub fn recolor_rgba(rgba: &mut [u8], r: u32) {
    if !changes(r) || r >> 24 == TATTOO {
        return;
    }
    if r >> 24 == SKIN {
        let from = SkinRef::of(rgba);
        recolor_skin(rgba, r, &from);
        return;
    }
    // a texture has few distinct colours: each worked out once
    let mut seen: HashMap<[u8; 3], [u8; 3]> = HashMap::new();
    for px in rgba.chunks_exact_mut(4) {
        let c = [px[0], px[1], px[2]];
        let out = *seen.entry(c).or_insert_with(|| recolor_rgb(c, r));
        px[..3].copy_from_slice(&out);
    }
}

/// Every level of a texture recoloured by `r`, the base first: a [`SKIN`]
/// shade moves every level from the base's skin, so a far level is the near
/// one's colour.
pub fn recolor_levels(levels: &mut [Vec<u8>], r: u32) {
    if r >> 24 == SKIN {
        let Some(base) = levels.first() else { return };
        let from = SkinRef::of(base);
        for l in levels {
            recolor_skin(l, r, &from);
        }
        return;
    }
    for l in levels {
        recolor_rgba(l, r);
    }
}

/// Every texel moved from skin `from` to shade `r`, its alpha kept.
fn recolor_skin(rgba: &mut [u8], r: u32, from: &SkinRef) {
    let mut seen: HashMap<[u8; 3], [u8; 3]> = HashMap::new();
    for px in rgba.chunks_exact_mut(4) {
        let c = [px[0], px[1], px[2]];
        let out = *seen.entry(c).or_insert_with(|| skin(c.map(|v| v as f32 / 255.0), r, from).map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8));
        px[..3].copy_from_slice(&out);
    }
}

/// The recoloured textures named so far: id to (source, recolour), and back.
#[derive(Default)]
struct Names {
    of: HashMap<i32, (i32, u32)>,
    by: HashMap<(i32, u32), i32>,
}

static NAMES: Mutex<Option<Names>> = Mutex::new(None);

/// The id that names texture `source` recoloured by `r`, for a layer to draw:
/// the same id for the same pair every time; one no texture of `taken` has (a
/// library's lookup: a DictID is a hash, so a name is drawn again past one that
/// is taken). `source` itself when `r` changes nothing.
pub fn recolored(source: i32, r: u32, taken: impl Fn(i32) -> bool) -> i32 {
    if !changes(r) || source == 0 {
        return source;
    }
    let mut g = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    let names = g.get_or_insert_with(Names::default);
    if let Some(&id) = names.by.get(&(source, r)) {
        return id;
    }
    // a hash of the pair, never 0, never a name another pair has, never a
    // texture's own
    let mut id = (source as u32 ^ r.rotate_left(13)).wrapping_mul(0x9E37_79B1) as i32 | 1;
    while names.of.contains_key(&id) || taken(id) {
        id = (id as u32).wrapping_mul(0x85EB_CA6B).wrapping_add(0xC2B2_AE35) as i32 | 1;
    }
    names.of.insert(id, (source, r));
    names.by.insert((source, r), id);
    id
}

/// What a recoloured texture's id names: its source and the recolour; none for
/// any id [`recolored`] did not hand out.
pub fn source_of(id: i32) -> Option<(i32, u32)> {
    let g = NAMES.lock().unwrap_or_else(|e| e.into_inner());
    g.as_ref().and_then(|n| n.of.get(&id).copied())
}

/// The texture a recoloured texture's id is made from at the first: its
/// source's source, and so on, to a texture's own id (`id` itself for one).
pub fn root_of(id: i32) -> i32 {
    let mut id = id;
    for _ in 0..16 {
        match source_of(id) {
            Some((source, _)) => id = source,
            None => break,
        }
    }
    id
}

/// A recoloured texture's key in the renderer's cache: its source's key and the
/// recolour, apart from every texture's own (bit 63).
pub fn key_of(source_key: u64, r: u32) -> u64 {
    let mut h = source_key ^ (r as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    h | 1 << 63
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The trim's mask: a gold stripe down a steel texture reads as trim in
    /// red, its halo in green spreading off it onto the steel and fading, the
    /// gold's brightness in blue, every alpha kept; steel far off is none.
    #[test]
    fn a_trim_mask_and_its_halo() {
        let (w, h) = (40u32, 4u32);
        let mut rgba = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                let px = if (18..22).contains(&x) { [200, 150, 40, 255] } else { [120, 124, 130, 128] };
                rgba.extend_from_slice(&px);
            }
        }
        let m = trim_mask(&rgba, w, h);
        let at = |x: usize| &m[4 * (w as usize + x)..4 * (w as usize + x) + 4];
        assert_eq!((at(20)[0], at(20)[1], at(20)[2], at(20)[3]), (255, 255, 200, 255), "the trim");
        assert_eq!((at(0)[0], at(0)[1], at(0)[3]), (0, 0, 128), "steel far off: none, its alpha kept");
        assert_eq!(at(16)[0], 0, "steel beside the trim is no trim");
        assert!(at(16)[1] > at(14)[1] && at(14)[1] > 0, "the halo fades off the trim: {} {}", at(16)[1], at(14)[1]);
        assert_eq!(trim_mask(&rgba[..8], w, h), vec![0; 8], "a size that does not fit");
        assert!(changes(TRIM_MASK << 24));
    }

    /// 0 and an unknown mode leave every colour as it is; a tone of 128s and
    /// no turn keeps a colour within a rounding; a dye's grey stays grey with
    /// no saturation; a dye takes its hue whatever the colour was (a blue cap
    /// red), its shading kept: a darker texel stays darker.
    #[test]
    fn recolours() {
        for c in [[0, 0, 0], [255, 255, 255], [70, 110, 190], [12, 200, 7]] {
            assert_eq!(recolor_rgb(c, 0), c);
            assert_eq!(recolor_rgb(c, 0x0980_8080), c, "an unknown mode");
            let t = recolor_rgb(c, TONE << 24 | 0x00_8080);
            assert!(t.iter().zip(c).all(|(a, b)| (*a as i32 - b as i32).abs() <= 1), "{c:?} {t:?}");
        }
        assert_eq!(recolor_rgb([128, 128, 128], DYE << 24 | 0x40_0080), [128, 128, 128], "no saturation: grey");
        let red = DYE << 24 | 0x00_C080; // hue 0, saturation 0.75, lightness kept
        let (light, dark) = (recolor_rgb([70, 110, 190], red), recolor_rgb([35, 55, 95], red));
        assert!(light[0] > light[1] && light[0] > light[2], "red: {light:?}");
        assert!(dark[0] < light[0], "a darker texel stays darker: {dark:?} {light:?}");
        let turned = recolor_rgb([200, 40, 40], TONE << 24 | 0x55_8080); // a third of a turn
        assert!(turned[1] > turned[0] && turned[1] > turned[2], "red turned green: {turned:?}");
        let mut px = vec![70, 110, 190, 77, 70, 110, 190, 0];
        recolor_rgba(&mut px, red);
        assert_eq!((px[3], px[7]), (77, 0), "alpha kept");
        assert_eq!(&px[..3], &px[4..7]);
    }

    /// A skin shade moves a texture's skin to the shade's colour whatever the
    /// skin was (a human's light, an erudite's dark: the same colour comes
    /// out), its shading kept (a darker texel stays darker); black and white
    /// stay near where they were, an iris of another hue keeps it (only its
    /// lightness follows), alpha is kept; every level moves from the base's
    /// skin. The controls: a first step (0) and a mode it does not know leave
    /// the texels.
    #[test]
    fn skin_shades() {
        let dark = SKIN << 24 | 19 << 16 | 107 << 8 | 59; // go-eqoa-server's Shade(27, 42, 23)
        let skin_of = |c: [u8; 3]| -> Vec<u8> {
            let mut v = Vec::new();
            for k in 0..64u8 {
                let d = k as i32 - 32;
                v.extend([(c[0] as i32 + d / 4).clamp(0, 255) as u8, (c[1] as i32 + d / 4).clamp(0, 255) as u8, (c[2] as i32 + d / 4).clamp(0, 255) as u8, 255]);
            }
            v
        };
        let mean = |v: &[u8]| -> [f32; 3] {
            let n = (v.len() / 4) as f32;
            let mut m = [0.0f32; 3];
            for p in v.chunks_exact(4) {
                for k in 0..3 {
                    m[k] += p[k] as f32 / n;
                }
            }
            m
        };
        let mut human = skin_of([214, 156, 135]);
        let mut erudite = skin_of([91, 51, 41]);
        recolor_rgba(&mut human, dark);
        recolor_rgba(&mut erudite, dark);
        let (h, e) = (mean(&human), mean(&erudite));
        assert!(h.iter().zip(e).all(|(a, b)| (a - b).abs() < 6.0), "both to the shade: {h:?} {e:?}");
        let want = rgb(19.0 / 256.0, 107.0 / 255.0, 59.0 / 255.0).map(|v| v * 255.0);
        assert!(h.iter().zip(want).all(|(a, b)| (a - b).abs() < 8.0), "the shade's colour: {h:?} {want:?}");
        assert!(human[0] < human[4 * 63], "a darker texel stays darker");

        let from = SkinRef::of(&skin_of([214, 156, 135]));
        let to = |c: [u8; 3]| skin(c.map(|v| v as f32 / 255.0), dark, &from).map(|v| (v * 255.0).round() as u8);
        assert!(to([250, 250, 248])[0] > 200, "an eye's white stays light: {:?}", to([250, 250, 248]));
        assert!(to([8, 8, 8])[0] < 20, "black stays black: {:?}", to([8, 8, 8]));
        let iris = to([60, 110, 200]);
        assert!(iris[2] > iris[0], "a blue iris stays blue: {iris:?}");

        let mut px = vec![214, 156, 135, 77, 214, 156, 135, 0];
        recolor_rgba(&mut px, dark);
        assert_eq!((px[3], px[7]), (77, 0), "alpha kept");
        let base = skin_of([214, 156, 135]);
        let mut levels = vec![base.clone(), base[..32].to_vec()];
        recolor_levels(&mut levels, dark);
        let mut alone = base[..32].to_vec();
        recolor_skin(&mut alone, dark, &SkinRef::of(&base));
        assert_eq!(levels[1], alone, "a far level from the base's skin");

        for r in [0, 0x0900_0000 | 0x13_6B3B] {
            let mut v = skin_of([214, 156, 135]);
            recolor_rgba(&mut v, r);
            assert_eq!(v, skin_of([214, 156, 135]), "the control {r:#x}");
        }
        assert!(changes(dark) && !changes(0x0900_0000));
    }

    /// A pair names one id every time, never 0 and never one taken; another
    /// pair another; a recolour that changes nothing is the source itself.
    #[test]
    fn names() {
        let a = recolored(0x1234, DYE << 24 | 1, |_| false);
        assert_eq!(recolored(0x1234, DYE << 24 | 1, |_| false), a);
        assert_eq!(source_of(a), Some((0x1234, DYE << 24 | 1)));
        let b = recolored(0x1234, DYE << 24 | 2, |_| false);
        assert_ne!(a, b);
        assert_eq!(recolored(0x1234, 0, |_| false), 0x1234);
        let first = (0x5555u32 ^ (DYE << 24 | 3).rotate_left(13)).wrapping_mul(0x9E37_79B1) as i32 | 1;
        let c = recolored(0x5555, DYE << 24 | 3, |id| id == first);
        assert_ne!(c, first, "a texture's own id is passed over");
        assert_eq!(source_of(0x7777), None);
        assert_ne!(key_of(5, 1), key_of(5, 2));
        assert_ne!(key_of(5, 1) & (1 << 63), 0);
    }

    /// The trim's weight: gold (the plate's, lit and shaded) wholly, the grey
    /// steel, set 9's blue steel, black and white not at all; a colour past
    /// the hue window (a red, a green) not at all.
    #[test]
    fn the_trim_is_the_gold() {
        for c in [[200, 160, 40], [120, 96, 30], [235, 205, 110]] {
            assert_eq!(trim_weight(c), 1.0, "gold {c:?}");
        }
        for c in [[128, 128, 130], [100, 140, 150], [0, 0, 0], [255, 255, 255], [200, 30, 30], [40, 180, 60], [20, 16, 6]] {
            assert_eq!(trim_weight(c), 0.0, "{c:?}");
        }
        let edge = trim_weight([150, 140, 120]);
        assert!(edge > 0.0 && edge < 1.0, "a greyish gold: {edge}");
    }

    /// The trim's colour is `aura::glow`'s: the colour's hue at the trim's
    /// brightness (the sword's numbers), a grey its lightness of it; MUL
    /// multiplies as a layer's colour does (255 keeps).
    #[test]
    fn the_trims_glow_and_the_multiply() {
        assert_eq!(trim_glow([204, 255, 255], [0xB0, 0x10, 0x2A]), [248, 23, 59]);
        assert_eq!(trim_glow([200, 160, 40], [255, 255, 255]), [200, 200, 200]);
        assert_eq!(trim_glow([200, 160, 40], [0, 0, 0]), [0, 0, 0]);
        assert_eq!(recolor_rgb([200, 100, 50], MUL << 24 | 0xFF_FFFF), [200, 100, 50]);
        assert_eq!(recolor_rgb([200, 100, 50], MUL << 24 | 0x80_8080), [100, 50, 25]);
        assert_eq!(recolor_rgb([128, 128, 130], TRIM << 24 | 0xD1_0000), [128, 128, 130], "steel kept");
        assert_eq!(recolor_rgb([200, 160, 40], TRIM << 24 | 0xD1_0000), [200, 0, 0], "gold red");
    }

    /// `trim_levels`: the trim (read off the root) in the colour, every other
    /// texel of the level exactly as it was (the body's colour baked in by a
    /// MUL before it stays), alpha kept; a level whose root is another size
    /// is left.
    #[test]
    fn trim_levels_touch_the_trim_alone() {
        let root = vec![200, 160, 40, 255, 128, 128, 130, 77, 100, 140, 150, 0];
        let mut body = root.clone();
        recolor_rgba(&mut body, MUL << 24 | 0x40_8040);
        let mut l = vec![body.clone(), vec![1, 2, 3, 4]];
        trim_levels(&mut l, &[(root.clone(), 3), (vec![], 1)], TRIM << 24 | 0x20_50E0, Trim::Gold);
        assert_eq!(&l[0][..4], &[trim_glow([200, 160, 40], [0x20, 0x50, 0xE0])[0], trim_glow([200, 160, 40], [0x20, 0x50, 0xE0])[1], trim_glow([200, 160, 40], [0x20, 0x50, 0xE0])[2], 255]);
        assert_eq!(&l[0][4..], &body[4..], "the steel as the MUL left it, alpha too");
        assert_eq!(l[1], vec![1, 2, 3, 4]);
    }

    /// `trimmed`: one id a (texture, colour, trim), the trim over a MUL of the
    /// colour (none for white); its alpha source and root the texture.
    #[test]
    fn trimmed_names() {
        let tex = 0x40B0_DDA7u32 as i32;
        let a = trimmed(tex, [0x80, 0x10, 0x10], [0xD1, 0, 0], |_| false);
        assert_eq!(trimmed(tex, [0x80, 0x10, 0x10], [0xD1, 0, 0], |_| false), a);
        let (mul, r) = source_of(a).unwrap();
        assert_eq!(r, TRIM << 24 | 0xD1_0000);
        assert_eq!(source_of(mul), Some((tex, MUL << 24 | 0x80_1010)));
        assert_eq!((root_of(a), alpha_source(a)), (tex, tex));
        let w = trimmed(tex, [255; 3], [0xD1, 0, 0], |_| false);
        assert_eq!(source_of(w), Some((tex, TRIM << 24 | 0xD1_0000)), "white: the trim alone");
        assert_ne!(a, w);
        let dyed = recolored(tex, DYE << 24 | 1, |_| false);
        assert_eq!(alpha_source(dyed), dyed, "another recolour is its own");
        assert_eq!(alpha_source(tex), tex);
    }

    /// The disc's plate (`vi::cust::TRIMMED`, sets 4 and 9; set 9's out of
    /// CHAR.ESF by file offset): how much of each texture is trim (printed;
    /// set 9's chest, bracer, legs and boots a third and more, set 4's a
    /// fifth and less, of the texels that show), and the gloves (no trim on either set) nearly none.
    /// Needs `run/eqoa.iso`; without it, SKIPs.
    #[test]
    fn the_plates_trim_on_the_disc() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("CHARCUST.CSF", esf::Container::open(disc.read("CHARCUST.CSF").unwrap()).unwrap());
        let file = lib.add("CHAR.ESF", esf::Container::open(disc.read("CHAR.ESF").unwrap()).unwrap());
        let ornate = [0xC214_1C0Au32, 0x03C1_BE9E, 0x85EA_2A1D, 0x432C_4890, 0xB179_23D2];
        for (id, off) in ornate.iter().zip([0x073BECusize, 0x072257, 0x0C7D31, 0x06C8BE, 0x06AF29]) {
            let node = lib.file(file).nodes.iter().position(|n| n.ty == esf::types::SURFACE && n.body == off + 12).unwrap();
            lib.alias(*id as i32, esf::Ref { file, node });
        }
        let share = |id: u32| {
            let (c, s) = lib.surface(id as i32).unwrap();
            let im = s.decode(&c.data).unwrap();
            let px: Vec<[u8; 3]> = im.rgba.chunks_exact(4).filter(|p| p[3] > 0 || !im.alpha).map(|p| [p[0], p[1], p[2]]).collect();
            px.iter().map(|&c| trim_weight(c)).sum::<f32>() / px.len() as f32
        };
        let set4 = [0x40B0_DDA7u32, 0xEB41_6A29, 0x0638_5212, 0xF506_17E7, 0x4328_B710];
        for (name, set) in [("set 4", set4), ("set 9", ornate)] {
            let v: Vec<f32> = set.iter().map(|&id| share(id)).collect();
            eprintln!("{name}: chest {:.1}% bracer {:.1}% gloves {:.1}% legs {:.1}% boots {:.1}%", v[0] * 100.0, v[1] * 100.0, v[2] * 100.0, v[3] * 100.0, v[4] * 100.0);
            assert!(v[2] < 0.01, "{name}'s gloves have no trim: {}", v[2]);
        }
        for id in [ornate[0], ornate[1], ornate[3], ornate[4]] {
            assert!(share(id) > 0.30, "{id:08X}: {}", share(id));
        }
        for id in [set4[0], set4[1], set4[3], set4[4]] {
            let s = share(id);
            assert!(s > 0.02 && s < 0.25, "{id:08X}: {s}");
        }
    }

    /// A tattoo is a mode that changes something, but no colour here (it is
    /// laid by `LibraryTextures`, off the faces); its id's root is the face it
    /// was made from at the first, through a shade laid over it.
    #[test]
    fn a_tattoo_names_a_face() {
        let t = TATTOO << 24 | 2 << 16;
        assert!(changes(t));
        assert_eq!(recolor_rgb([214, 156, 135], t), [214, 156, 135]);
        let mut px = vec![214, 156, 135, 255];
        recolor_rgba(&mut px, t);
        assert_eq!(px, [214, 156, 135, 255]);
        let face = 0x0E95_25F4u32 as i32;
        let tattooed = recolored(face, t, |_| false);
        let shaded = recolored(tattooed, SKIN << 24 | 0x13_6B3B, |_| false);
        assert_eq!((root_of(shaded), root_of(tattooed), root_of(face)), (face, face, face));
    }
}
