//! The barbarians' tattoo (go-eqoa-server's `entity/tattoos.go`, the server
//! owner's of 2026-11-12: "for barbarians customize character lets add tattoo
//! toggle"): a recolour of mode [`crate::recolor::TATTOO`] on a barbarian's face.
//!
//! The disc paints woad on two of the barbarian man's four faces ($Race,
//! CHARFACE.CSF): face 1 ([`BANDS_FACE`]: a block over the forehead, a band
//! over the eyes, bars down the cheeks) and face 3 ([`CREST_FACE`], and the
//! bodies' own [`CREST_BODY_FACE`], the same art: framed blocks over the brow
//! and a band over the eyes). His other two faces and all four of the woman's
//! have none; her faces lie on the texture as his do (the eyes on the same
//! texels), so a pattern lies on them as it does on his.
//!
//! The man's bare chest wears a tattoo too, a swirl and a knotwork band painted
//! on his chest's second layer ([`CHEST_ART`], the layer an armour's chest
//! replaces; the woman's leather top is the same texture's, her texture
//! coordinates never on the tattoo's): a pattern of none takes it off
//! ([`chest`]), any other keeps it.
//!
//! A pattern is read off its face ([`Woad::of`]): where the woad is (its texels'
//! saturation under 0.3 and lightness 0.15 to 0.62, inside the face, a lone
//! one dropped, and the pattern's own rectangles where the woad is a darker
//! skin rather than a grey: the crest's brow and both eye bands), the skin
//! under it (the woad's texels filled from the skin round them, [`fill`]) and
//! the woad's ratio to that skin, texel by texel. Laying a tattoo on a face
//! ([`lay`]) takes the face's own woad off (its texels become the skin under
//! them; the eyes in a band are left), then multiplies the face's skin texels
//! under the pattern by the pattern's ratios (an eye, a brow, a lip keep their
//! own). A pattern of none only takes the woad off. A level of another size
//! takes each texel's share of the pattern's (a mip level's, an average; a
//! texture pack's larger level, the nearest).

/// The man's face 1, the bands.
pub const BANDS_FACE: u32 = 0xEB79_B952;
/// The man's face 3, the crest.
pub const CREST_FACE: u32 = 0x0E95_25F4;
/// The barbarian man's body's own face: the crest's art.
pub const CREST_BODY_FACE: u32 = 0xEFC0_FF3E;

/// The barbarian man's bare chest's layer, his chest tattoo on it.
pub const CHEST_ART: u32 = 0xEB71_B584;

/// [`CHEST_ART`]'s rectangles (in 128ths: x0, y0, x1, y1, past-last) that
/// hold the man's chest tattoo: the swirl and the knotwork band (his texture
/// coordinates alone reach them).
const CHEST_TATTOO: [[u32; 4]; 2] = [[0, 66, 34, 120], [44, 36, 68, 90]];

/// One level of [`CHEST_ART`] (`rgba`, `w` by `h`) for a tattoo of `pattern`:
/// none takes the chest tattoo off (its texels clear, the skin under them
/// showing), any other keeps it.
pub fn chest(rgba: &mut [u8], w: u32, h: u32, pattern: u8) {
    if pattern != NONE || rgba.len() < (w * h * 4) as usize {
        return;
    }
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x * 128 / w, y * 128 / h);
            if CHEST_TATTOO.iter().any(|r| (r[0]..r[2]).contains(&u) && (r[1]..r[3]).contains(&v)) {
                rgba[((y * w + x) * 4 + 3) as usize] = 0;
            }
        }
    }
}

/// Whether a tattoo is laid on a layer of this texture (made from it at the
/// first): a face (the caller's `$Race`) or the man's bare chest.
pub fn marks(root: u32) -> bool {
    root == CHEST_ART
}

/// The patterns of a tattoo's byte 2.
pub const NONE: u8 = 0;
pub const BANDS: u8 = 1;
pub const CREST: u8 = 2;

/// The pattern a face wears of its own, if any.
pub fn own_pattern(face: u32) -> Option<u8> {
    match face {
        BANDS_FACE => Some(BANDS),
        CREST_FACE | CREST_BODY_FACE => Some(CREST),
        _ => None,
    }
}

/// The face a pattern is read off.
pub fn pattern_face(pattern: u8) -> Option<u32> {
    match pattern {
        BANDS => Some(BANDS_FACE),
        CREST => Some(CREST_FACE),
        _ => None,
    }
}

/// A tattoo's pattern (byte 2).
pub fn pattern_of(r: u32) -> u8 {
    (r >> 16) as u8
}

/// The pattern's rectangles in 64ths of the face (x0, y0, x1, y1, inclusive)
/// whose every texel is woad but an eye's: the bands' eye band; the crest's
/// eye band and its brow, framed blocks whose woad is a darker skin.
fn solid(pattern: u8) -> &'static [[u32; 4]] {
    match pattern {
        BANDS => &[[13, 20, 32, 27]],
        CREST => &[[15, 20, 50, 28], [13, 0, 50, 17]],
        _ => &[],
    }
}

/// A pattern read off its face, 64 by 64 whatever the face's size.
pub struct Woad {
    /// Woad (the eyes in a band too).
    mask: Vec<bool>,
    /// An eye's texel in a band, left as it is.
    eye: Vec<bool>,
    /// The skin under each texel (0..1 RGB).
    skin: Vec<[f32; 3]>,
    /// Each woad texel's colour against the skin under it, at most 1 (1 off the
    /// woad; an eye's, the woad's mean).
    ratio: Vec<[f32; 3]>,
}

const N: usize = 64;

fn hsl(c: [f32; 3]) -> (f32, f32, f32) {
    crate::recolor::hsl_of(c)
}

/// A colour's hue's distance from the faces' skin's (18 degrees), in turns.
fn off_skin(h: f32) -> f32 {
    let d = (h - 0.05).abs();
    d.min(1.0 - d)
}

/// A texel of a face's skin (its own shading, not an eye, a brow or a lip's
/// darker red).
fn skinlike(c: [f32; 3]) -> bool {
    let (h, s, l) = hsl(c);
    s >= 0.2 && off_skin(h) < 0.1 && l > 0.25 && l < 0.9
}

/// A texel of woad: greyed and darker.
fn woad(c: [f32; 3]) -> bool {
    let (_, s, l) = hsl(c);
    s < 0.3 && l > 0.15 && l < 0.62
}

/// An eye's texel: its white, its pupil, its iris.
fn eyelike(c: [f32; 3]) -> bool {
    let (h, s, l) = hsl(c);
    l > 0.62 || l < 0.12 || (off_skin(h) > 0.15 && s > 0.25)
}

/// Each of `known`'s unknown texels filled from the known round it (an average
/// of its four neighbours, again and again from the known texels' mean).
fn fill(c: &[[f32; 3]], known: &[bool]) -> Vec<[f32; 3]> {
    let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
    for (v, _) in c.iter().zip(known).filter(|(_, k)| **k) {
        for k in 0..3 {
            sum[k] += v[k];
        }
        n += 1.0;
    }
    let mean = sum.map(|s| s / n.max(1.0));
    let mut est: Vec<[f32; 3]> = c.iter().zip(known).map(|(v, &k)| if k { *v } else { mean }).collect();
    for _ in 0..300 {
        let prev = est.clone();
        for i in (0..N * N).filter(|&i| !known[i]) {
            let (x, y) = (i % N, i / N);
            let (mut a, mut m) = ([0.0f32; 3], 0.0f32);
            for (dx, dy) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                if (0..N as i32).contains(&nx) && (0..N as i32).contains(&ny) {
                    let p = prev[ny as usize * N + nx as usize];
                    for k in 0..3 {
                        a[k] += p[k];
                    }
                    m += 1.0;
                }
            }
            est[i] = a.map(|v| v / m);
        }
    }
    est
}

impl Woad {
    /// Pattern `pattern` read off its face's RGBA (`w` by `h`, sampled at 64 by
    /// 64). None for a pattern of none.
    pub fn of(pattern: u8, rgba: &[u8], w: u32, h: u32) -> Option<Woad> {
        if pattern_face(pattern).is_none() || w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
            return None;
        }
        let c: Vec<[f32; 3]> = (0..N * N)
            .map(|i| {
                let (x, y) = ((i % N) as u32 * w / N as u32, (i / N) as u32 * h / N as u32);
                let o = ((y * w + x) * 4) as usize;
                [rgba[o], rgba[o + 1], rgba[o + 2]].map(|v| v as f32 / 255.0)
            })
            .collect();
        // the woad's texels inside the face (the ears and past them left), a
        // lone one dropped, then the woad touching them taken back in
        let inside = |i: usize| (13..=51).contains(&(i % N)) && i / N < 48;
        let raw: Vec<bool> = (0..N * N).map(|i| inside(i) && woad(c[i])).collect();
        let near = |m: &[bool], i: usize, d: &[(i32, i32)]| {
            d.iter()
                .filter(|&&(dx, dy)| {
                    let (x, y) = ((i % N) as i32 + dx, (i / N) as i32 + dy);
                    (0..N as i32).contains(&x) && (0..N as i32).contains(&y) && m[y as usize * N + x as usize]
                })
                .count()
        };
        const FOUR: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        const EIGHT: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
        let kept: Vec<bool> = (0..N * N).map(|i| raw[i] && near(&raw, i, &FOUR) >= 2).collect();
        let mut mask: Vec<bool> = (0..N * N).map(|i| kept[i] || (raw[i] && near(&kept, i, &EIGHT) > 0)).collect();
        for r in solid(pattern) {
            for y in r[1]..=r[3] {
                for x in r[0]..=r[2] {
                    mask[y as usize * N + x as usize] = true;
                }
            }
        }
        let eye: Vec<bool> = (0..N * N).map(|i| mask[i] && (18..=30).contains(&(i / N)) && eyelike(c[i])).collect();
        let known: Vec<bool> = (0..N * N).map(|i| !mask[i] && skinlike(c[i])).collect();
        let skin = fill(&c, &known);
        let mut ratio: Vec<[f32; 3]> = (0..N * N).map(|i| if mask[i] && !eye[i] { std::array::from_fn(|k| (c[i][k] / skin[i][k].max(1.0 / 255.0)).min(1.0)) } else { [1.0; 3] }).collect();
        let woad: Vec<usize> = (0..N * N).filter(|&i| mask[i] && !eye[i]).collect();
        let mean: [f32; 3] = std::array::from_fn(|k| woad.iter().map(|&i| ratio[i][k]).sum::<f32>() / woad.len().max(1) as f32);
        for i in (0..N * N).filter(|&i| eye[i]) {
            ratio[i] = mean;
        }
        Some(Woad { mask, eye, skin, ratio })
    }

    /// The pattern's woad texels (an eye's in a band too).
    pub fn covers(&self, x: usize, y: usize) -> bool {
        self.mask[y * N + x]
    }

    /// The pattern's mean ratio, over its woad's texels.
    pub fn mean_ratio(&self) -> [f32; 3] {
        let woad: Vec<usize> = (0..N * N).filter(|&i| self.mask[i] && !self.eye[i]).collect();
        std::array::from_fn(|k| woad.iter().map(|&i| self.ratio[i][k]).sum::<f32>() / woad.len().max(1) as f32)
    }
}

/// The 64ths a level's texel `x` of `w` covers: its footprint's first and
/// past-last (one at least).
fn span(x: u32, w: u32) -> (usize, usize) {
    let a = (x as usize * N) / w as usize;
    let b = (((x + 1) as usize * N).div_ceil(w as usize)).max(a + 1).min(N);
    (a.min(N - 1), b)
}

/// One level of a face (`rgba`, `w` by `h`) tattooed: the woad of `own` (the
/// face's own pattern) taken off, to the skin under it where the level is the
/// face's own art (`plain`: its own texels' skin), else divided out by its
/// ratios (a face moved on from the disc's); then `put`'s ratios laid on its
/// skin's texels. Alpha kept.
pub fn lay(rgba: &mut [u8], w: u32, h: u32, own: Option<&Woad>, plain: bool, put: Option<&Woad>) {
    if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
        return;
    }
    for y in 0..h {
        let (y0, y1) = span(y, h);
        for x in 0..w {
            let (x0, x1) = span(x, w);
            let o = ((y * w + x) * 4) as usize;
            let mut c = [rgba[o], rgba[o + 1], rgba[o + 2]].map(|v| v as f32 / 255.0);
            let n = ((y1 - y0) * (x1 - x0)) as f32;
            if let Some(own) = own {
                let (mut k, mut skin, mut ratio) = (0.0f32, [0.0f32; 3], [0.0f32; 3]);
                for py in y0..y1 {
                    for px in x0..x1 {
                        let i = py * N + px;
                        if own.mask[i] && !own.eye[i] {
                            k += 1.0;
                            for j in 0..3 {
                                skin[j] += own.skin[i][j];
                                ratio[j] += own.ratio[i][j];
                            }
                        }
                    }
                }
                if k > 0.0 {
                    let f = k / n;
                    for j in 0..3 {
                        let under = if plain { skin[j] / k } else { (c[j] / (ratio[j] / k).max(0.05)).min(1.0) };
                        c[j] = c[j] * (1.0 - f) + under * f;
                    }
                }
            }
            if let Some(put) = put
                && skinlike(c)
            {
                let mut m = [0.0f32; 3];
                for py in y0..y1 {
                    for px in x0..x1 {
                        for j in 0..3 {
                            m[j] += put.ratio[py * N + px][j];
                        }
                    }
                }
                for j in 0..3 {
                    c[j] *= m[j] / n;
                }
            }
            for j in 0..3 {
                rgba[o + j] = (c[j] * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A face of 64 by 64: skin (a little lighter down the face) with the
    /// crest's brow and eye band in woad (the skin times 0.6, greyed), an eye's
    /// white and pupil in the band, and a grey block at (54..58, 40..44) that is
    /// no pattern's (past the face's woad, by the ear).
    fn face() -> Vec<u8> {
        let mut px = Vec::with_capacity(64 * 64 * 4);
        for y in 0..64u32 {
            for x in 0..64u32 {
                let skin = [200.0 + y as f32 * 0.5, 140.0 + y as f32 * 0.3, 115.0];
                let band = (15..=50).contains(&x) && (20..=28).contains(&y) || (13..=50).contains(&x) && y <= 17;
                let c = if (24..=26).contains(&x) && y == 23 {
                    [235.0, 235.0, 230.0] // an eye's white
                } else if x == 25 && y == 24 {
                    [10.0, 10.0, 10.0] // its pupil
                } else if band {
                    [120.0, 108.0, 104.0]
                } else if (54..58).contains(&x) && (40..44).contains(&y) {
                    [110.0, 108.0, 106.0]
                } else {
                    skin
                };
                px.extend([c[0] as u8, c[1] as u8, c[2] as u8, 255]);
            }
        }
        px
    }

    fn at(px: &[u8], w: u32, x: u32, y: u32) -> [u8; 3] {
        let o = ((y * w + x) * 4) as usize;
        [px[o], px[o + 1], px[o + 2]]
    }

    /// The crest read off a face: its rectangles and its woad are the
    /// pattern, an eye in the band is not laid; the skin round them is not.
    /// Taken off its own face, the woad becomes the skin under it (near the
    /// skin round it), the eye's white and pupil kept, every texel off the
    /// pattern byte for byte. Laid on a plain face, the woad is darker skin
    /// there, the eye's white of that face kept, the rest byte for byte. A
    /// pattern of none reads nothing.
    #[test]
    fn a_pattern_off_and_on() {
        let f = face();
        assert!(Woad::of(NONE, &f, 64, 64).is_none());
        let crest = Woad::of(CREST, &f, 64, 64).unwrap();
        assert!(crest.covers(30, 5) && crest.covers(40, 24) && !crest.covers(5, 5) && !crest.covers(30, 35));
        assert!(!crest.covers(55, 41), "a mark of no pattern's");
        let m = crest.mean_ratio();
        assert!(m.iter().all(|&r| r > 0.4 && r < 0.95), "{m:?}");

        let mut off = f.clone();
        lay(&mut off, 64, 64, Some(&crest), true, None);
        let skin_near = at(&f, 64, 30, 19);
        let under = at(&off, 64, 30, 5);
        assert!(under.iter().zip(skin_near).all(|(&a, b)| (a as i32 - b as i32).abs() < 12), "the woad off: {under:?} against {skin_near:?}");
        assert_eq!(at(&off, 64, 25, 23), at(&f, 64, 25, 23), "the eye's white kept");
        assert_eq!(at(&off, 64, 25, 24), at(&f, 64, 25, 24), "its pupil kept");
        for (x, y) in [(5, 5), (30, 35), (55, 41), (60, 60)] {
            assert_eq!(at(&off, 64, x, y), at(&f, 64, x, y), "off the pattern ({x}, {y})");
        }

        let mut plain: Vec<u8> = off.clone();
        let mut on = plain.clone();
        lay(&mut on, 64, 64, None, true, Some(&crest));
        let (a, b) = (at(&plain, 64, 30, 5), at(&on, 64, 30, 5));
        assert!(b.iter().zip(a).all(|(&b, a)| b < a), "the woad on: {b:?} under {a:?}");
        assert_eq!(at(&on, 64, 25, 23), at(&plain, 64, 25, 23), "the face's eye kept");
        assert_eq!(at(&on, 64, 5, 5), at(&plain, 64, 5, 5));
        // nothing to take off and nothing to lay: the face byte for byte
        lay(&mut plain, 64, 64, None, true, None);
        assert_eq!(plain, off);
    }

    /// A mip level (16 by 16) takes each texel's share of the pattern: a texel
    /// wholly in the brow darker, one wholly off it byte for byte; a level
    /// larger than the pattern (128) the nearest texel's.
    #[test]
    fn a_level_of_another_size() {
        let f = face();
        let crest = Woad::of(CREST, &f, 64, 64).unwrap();
        let mut small = vec![0u8; 16 * 16 * 4];
        for px in small.chunks_exact_mut(4) {
            px.copy_from_slice(&[210, 150, 120, 255]);
        }
        let was = small.clone();
        lay(&mut small, 16, 16, None, true, Some(&crest));
        assert!(at(&small, 16, 8, 1)[0] < 200, "in the brow");
        assert_eq!(at(&small, 16, 1, 14), at(&was, 16, 1, 14), "off it");
        let mut big = vec![0u8; 128 * 128 * 4];
        for px in big.chunks_exact_mut(4) {
            px.copy_from_slice(&[210, 150, 120, 255]);
        }
        lay(&mut big, 128, 128, None, true, Some(&crest));
        assert!(at(&big, 128, 60, 10)[0] < 200 && at(&big, 128, 10, 120)[0] == 210);
    }

    /// The chest: none clears the swirl's and the band's texels, a pattern
    /// keeps them; the rest of the texture byte for byte either way.
    #[test]
    fn the_chest() {
        let px = vec![200u8; 128 * 128 * 4];
        let mut off = px.clone();
        chest(&mut off, 128, 128, NONE);
        let a = |p: &[u8], x: usize, y: usize| p[(y * 128 + x) * 4 + 3];
        assert_eq!((a(&off, 10, 90), a(&off, 50, 60), a(&off, 100, 20), a(&off, 10, 10)), (0, 0, 200, 200));
        let mut on = px.clone();
        chest(&mut on, 128, 128, CREST);
        assert_eq!(on, px);
        assert!(marks(CHEST_ART) && !marks(BANDS_FACE));
    }

    /// The faces' own patterns and the patterns' faces.
    #[test]
    fn faces_and_patterns() {
        assert_eq!((own_pattern(BANDS_FACE), own_pattern(CREST_FACE), own_pattern(CREST_BODY_FACE), own_pattern(0xD9EC_0301)), (Some(BANDS), Some(CREST), Some(CREST), None));
        assert_eq!((pattern_face(BANDS), pattern_face(CREST), pattern_face(NONE)), (Some(BANDS_FACE), Some(CREST_FACE), None));
        assert_eq!(pattern_of(0x0402_0000), CREST);
    }
}
