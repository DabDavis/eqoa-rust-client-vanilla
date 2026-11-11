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
//! keeps black and white where they are. 0, or a mode it does not know, is the
//! texture as it is.

use std::collections::HashMap;
use std::sync::Mutex;

/// One colour over a part's own shading.
pub const DYE: u32 = 1;
/// A part's own colours shifted.
pub const TONE: u32 = 2;
/// A skin texture moved to a shade (go-eqoa-server's `Shade`).
pub const SKIN: u32 = 3;

/// Whether `r` changes anything (a known mode).
pub fn changes(r: u32) -> bool {
    matches!(r >> 24, DYE | TONE | SKIN)
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
    if !changes(r) {
        return c;
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
    if !changes(r) {
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
}
