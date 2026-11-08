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
//! and the lightness (128ths each), the part's own colours shifted. 0, or a mode
//! it does not know, is the texture as it is.

use std::collections::HashMap;
use std::sync::Mutex;

/// One colour over a part's own shading.
pub const DYE: u32 = 1;
/// A part's own colours shifted.
pub const TONE: u32 = 2;

/// Whether `r` changes anything (a known mode).
pub fn changes(r: u32) -> bool {
    matches!(r >> 24, DYE | TONE)
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
    let (h, s, l) = hsl(c.map(|v| v as f32 / 255.0));
    let (h, s, l) = if r >> 24 == DYE { (a / 256.0, b / 255.0, l * d / 128.0) } else { (h + a / 256.0, s * b / 128.0, l * d / 128.0) };
    rgb(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0)).map(|v| (v * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// Every texel of an RGBA image recoloured by `r`, its alpha kept.
pub fn recolor_rgba(rgba: &mut [u8], r: u32) {
    if !changes(r) {
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
