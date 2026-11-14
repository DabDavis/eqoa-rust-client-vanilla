//! Which texels of an armour texture are its trim, texture by texture: what a
//! trim dye (go-eqoa-server's chardyes trim slots) draws in its colour
//! (`recolor::TRIM`) and what a trim aura lights (`recolor::TRIM_MASK`).
//!
//! The plate's trim is its gold ([`Trim::Gold`], `recolor::trim_weight`). No
//! other set's trim is gold, and the gold rule on them catches their brown
//! leather and wood, so every other texture has its own rule ([`Trim::Parts`]):
//! texels of a hue, saturation and value window, inside a texture's rectangles
//! where the trim is a band or a piece of the same colours as the rest (a
//! leather strap on leather). Read off the disc's art by eye, a texture at a
//! time (claude-notes/armor-trim-all-sets-20261113.md); the art itself is the
//! disc's, read at run time: nothing here is game data but these numbers.
//!
//! What each set's trim is:
//! - set 1, cloth: the yellow bands and the cream padding (not the legs'
//!   lattice, whose cream reads texel by texel);
//! - set 2, leather: the studded bands (bracer, boots), the knee straps and
//!   their rings (legs), the shoulder bosses (chest), the steel studs and
//!   buckles; the gloves have none;
//! - set 3, chain: the leather straps and cream bars; the bracer and gloves are
//!   mail alone;
//! - set 4, the plain plate, and set 9, the ornate: the gold; their gloves have
//!   none;
//! - set 5, banded: the steel plates and studs on the leather;
//! - set 6: the wooden straps (chest, legs), the top plate (bracer), the knuckle
//!   plate (gloves), the boots' cuff;
//! - set 7, scale: the leather edging and straps;
//! - set 8: the riveted bands; the boots have none;
//! - the robes: robe 0's red bands and gold panels, robe 1's steel-studded
//!   strips (top and hem), robe 2's red bands (its skirt has none), robe 3's studded
//!   straps (its skirt has none);
//! - the helms: each its set's (1 cream, 2 the steel bands, 3 the studded band,
//!   5 the steel plates, 7 the leather, 8 the riveted band); helm 4 (the plate's,
//!   which set 9 wears too) and helm 6 are plain steel, none.

/// A part of a texture's trim: texels whose hue (degrees; `[0, 360]` any
/// colour, a grey too; a window past 360 wraps through red), HSV saturation and
/// value are inside the windows, eased off past each edge (hue [`HUE_EASE`],
/// the others [`EASE`]), and inside one of `rects` (`[u0, v0, u1, v1]`, the
/// texture's 0..1 across and down; none: anywhere).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrimPart {
    pub hue: [f32; 2],
    pub sat: [f32; 2],
    pub val: [f32; 2],
    pub rects: &'static [[f32; 4]],
    /// The clean-up ([`Trim::mask`]): a patch of the part's texels smaller
    /// than this share of the texture's (never fewer than [`SPECK_LEAST`])
    /// is dropped, a hole in it as small is filled; 0 none.
    pub speck: f32,
    /// Close the part's texels first (a 3x3 dilate, then erode): a ragged
    /// edge smoothed and one-texel gaps bridged before the specks go.
    pub close: bool,
}

/// The fewest texels a patch that [`TrimPart::speck`] keeps has, whatever
/// the share says (a stud of 2 by 2 texels on a 64-texel texture).
pub const SPECK_LEAST: usize = 3;

/// How far past a hue window's edge its weight eases to none, in degrees.
pub const HUE_EASE: f32 = 6.0;
/// How far past a saturation or value window's edge its weight eases to none.
pub const EASE: f32 = 0.06;

/// Which texels of a texture are its trim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Trim {
    /// The plate's gold (`recolor::trim_weight`), anywhere.
    Gold,
    /// The most of any of the parts' weights.
    Parts(&'static [TrimPart]),
}

impl Trim {
    /// How much of the texel `c` at `u`, `v` (its centre, 0..1 across and down)
    /// is trim, 0 to 1.
    pub fn weight(&self, c: [u8; 3], u: f32, v: f32) -> f32 {
        match self {
            Trim::Gold => crate::recolor::trim_weight(c),
            Trim::Parts(parts) => parts.iter().map(|p| p.weight(c, u, v)).fold(0.0, f32::max),
        }
    }
}

impl Trim {
    /// The trim of a `w` by `h` texture (`rgba`), a weight a texel. The
    /// plate's gold is [`Trim::weight`]'s, eased as before; every other
    /// texture's is crisp, 0 or 1: each part's texels (its weight at least a
    /// half), closed ([`TrimPart::close`]), its specks dropped and its
    /// pinholes filled ([`TrimPart::speck`]), the parts together. So no
    /// stray texel of the right colour outside the trim takes its colour,
    /// and the trim's edge is the art's, not a fade over its neighbours.
    pub fn mask(&self, rgba: &[u8], w: usize, h: usize) -> Vec<f32> {
        let n = w * h;
        if w == 0 || h == 0 || rgba.len() < n * 4 {
            return vec![0.0; n];
        }
        let px = |k: usize| [rgba[4 * k], rgba[4 * k + 1], rgba[4 * k + 2]];
        let at = |k: usize| (((k % w) as f32 + 0.5) / w as f32, ((k / w) as f32 + 0.5) / h as f32);
        match self {
            Trim::Gold => (0..n).map(|k| crate::recolor::trim_weight(px(k))).collect(),
            Trim::Parts(parts) => {
                let mut out = vec![0.0f32; n];
                for p in parts.iter() {
                    let mut on: Vec<bool> = (0..n).map(|k| {
                        let (u, v) = at(k);
                        p.weight(px(k), u, v) >= 0.5
                    }).collect();
                    if p.close {
                        on = erode(&dilate(&on, w, h), w, h);
                    }
                    if p.speck > 0.0 {
                        let least = ((p.speck * n as f32) as usize).max(SPECK_LEAST);
                        drop_small(&mut on, w, h, true, least);
                        drop_small(&mut on, w, h, false, least);
                    }
                    for (o, &b) in out.iter_mut().zip(&on) {
                        if b {
                            *o = 1.0;
                        }
                    }
                }
                out
            }
        }
    }
}

/// A 3x3 dilate of `on` (`w` by `h`), its edges clamped.
fn dilate(on: &[bool], w: usize, h: usize) -> Vec<bool> {
    morph(on, w, h, true)
}

/// A 3x3 erode of `on`, its edges clamped (a texel past the edge is the
/// nearest one's).
fn erode(on: &[bool], w: usize, h: usize) -> Vec<bool> {
    morph(on, w, h, false)
}

fn morph(on: &[bool], w: usize, h: usize, any: bool) -> Vec<bool> {
    (0..w * h)
        .map(|k| {
            let (x, y) = ((k % w) as isize, (k / w) as isize);
            let mut hit = !any;
            for dy in -1..=1isize {
                for dx in -1..=1isize {
                    let nx = (x + dx).clamp(0, w as isize - 1) as usize;
                    let ny = (y + dy).clamp(0, h as isize - 1) as usize;
                    let b = on[ny * w + nx];
                    if any && b {
                        hit = true;
                    }
                    if !any && !b {
                        hit = false;
                    }
                }
            }
            hit
        })
        .collect()
}

/// Every patch (4-connected) of texels equal to `of` in `on` smaller than
/// `least` texels turned to `!of`: specks dropped (`of` true), pinholes
/// filled (`of` false).
fn drop_small(on: &mut [bool], w: usize, h: usize, of: bool, least: usize) {
    let mut seen = vec![false; w * h];
    let mut patch = Vec::new();
    let mut stack = Vec::new();
    for start in 0..w * h {
        if seen[start] || on[start] != of {
            continue;
        }
        patch.clear();
        stack.push(start);
        seen[start] = true;
        while let Some(k) = stack.pop() {
            patch.push(k);
            let (x, y) = (k % w, k / w);
            let mut go = |nk: usize| {
                if !seen[nk] && on[nk] == of {
                    seen[nk] = true;
                    stack.push(nk);
                }
            };
            if x > 0 {
                go(k - 1);
            }
            if x + 1 < w {
                go(k + 1);
            }
            if y > 0 {
                go(k - w);
            }
            if y + 1 < h {
                go(k + w);
            }
        }
        if patch.len() < least {
            for &k in &patch {
                on[k] = !of;
            }
        }
    }
}

impl TrimPart {
    /// The texel's weight by this part, before [`Trim::mask`]'s clean-up.
    pub fn weight(&self, c: [u8; 3], u: f32, v: f32) -> f32 {
        if !self.rects.is_empty() && !self.rects.iter().any(|r| u >= r[0] && u < r[2] && v >= r[1] && v < r[3]) {
            return 0.0;
        }
        let f = c.map(|x| x as f32 / 255.0);
        let (mx, mn) = (f[0].max(f[1]).max(f[2]), f[0].min(f[1]).min(f[2]));
        let d = mx - mn;
        let sat = if mx > 0.0 { d / mx } else { 0.0 };
        let hue = if self.hue == [0.0, 360.0] {
            1.0
        } else if d <= 1e-6 {
            0.0
        } else {
            let h = if mx == f[0] {
                ((f[1] - f[2]) / d).rem_euclid(6.0)
            } else if mx == f[1] {
                (f[2] - f[0]) / d + 2.0
            } else {
                (f[0] - f[1]) / d + 4.0
            } * 60.0;
            let [lo, hi] = self.hue;
            if lo <= hi { window(h, lo, hi, HUE_EASE) } else { window(h, lo, f32::INFINITY, HUE_EASE).max(window(h, f32::NEG_INFINITY, hi, HUE_EASE)) }
        };
        hue * window(sat, self.sat[0], self.sat[1], EASE) * window(mx, self.val[0], self.val[1], EASE)
    }
}

/// 1 inside `lo..=hi`, easing to 0 over `ease` past either edge (an infinite
/// edge none).
fn window(x: f32, lo: f32, hi: f32, ease: f32) -> f32 {
    let ramp = |x: f32, a: f32, b: f32| ((x - a) / (b - a)).clamp(0.0, 1.0);
    let a = if lo.is_finite() { ramp(x, lo - ease, lo) } else { 1.0 };
    let b = if hi.is_finite() { 1.0 - ramp(x, hi, hi + ease) } else { 1.0 };
    a.min(b)
}

const ANY_HUE: [f32; 2] = [0.0, 360.0];
const ANY: [f32; 2] = [0.0, 1.0];

/// Every texel inside the rectangles.
const fn band(rects: &'static [[f32; 4]]) -> TrimPart {
    TrimPart { hue: ANY_HUE, sat: ANY, val: ANY, rects, speck: 0.0, close: false }
}

/// A colour part, closed and its specks (under 0.3% of the texture) dropped.
const fn colour(hue: [f32; 2], sat: [f32; 2], val: [f32; 2]) -> TrimPart {
    TrimPart { hue, sat, val, rects: &[], speck: 0.003, close: true }
}

/// Set 1's yellow bands and cream padding, inside `rects` (none: anywhere),
/// closed and with patches under 0.4% dropped: hue 36-64 takes the padding
/// (38-40) whole, where a window from 41 took it texel by texel (the owner,
/// 2026-11-14: "a bit of color smear not kept in the main trim areas"); the
/// brown leather's highlights that reach it are specks, or outside the
/// rectangles.
const fn cloth(rects: &'static [[f32; 4]]) -> TrimPart {
    TrimPart { hue: [36.0, 64.0], sat: [0.22, 1.0], val: [0.47, 1.0], rects, speck: 0.004, close: true }
}

/// Set 1's yellow bands and cream padding (its brown leather is redder: hue
/// 31-33 against the bands' 45-47 and the padding's 38-40).
const CLOTH: TrimPart = cloth(&[]);
/// Steel studs and buckles on leather: a light grey.
/// Not closed (it would join studs a texel apart); a patch of 3 texels kept.
const STUDS: TrimPart = TrimPart { hue: ANY_HUE, sat: [0.0, 0.15], val: [0.5, 1.0], rects: &[], speck: 0.0002, close: false };
/// Leather and wood on steel or mail: a brown or tan of some colour.
const LEATHER: &[TrimPart] = &[colour([15.0, 60.0], [0.2, 1.0], [0.15, 1.0])];
const LEATHER_CHAIN: &[TrimPart] = &[colour([15.0, 60.0], [0.2, 1.0], [0.2, 1.0])];
const WOOD: &[TrimPart] = &[colour([15.0, 60.0], [0.25, 1.0], [0.15, 1.0])];
/// Set 5's steel plates (a blue grey) and studs (a light grey) on leather.
const BANDED: &[TrimPart] = &[colour([170.0, 250.0], [0.0, 0.7], [0.25, 1.0]), colour(ANY_HUE, [0.0, 0.1], [0.45, 1.0])];
/// Robe 0's and robe 2's red bands, robe 0's gold panels.
const RED: TrimPart = colour([345.0, 15.0], [0.35, 1.0], [0.2, 1.0]);
const ROBE_GOLD: TrimPart = colour([38.0, 65.0], [0.3, 1.0], [0.35, 1.0]);
/// Robe 1's steel strips on its blue.
const ROBE_STEEL: TrimPart = colour(ANY_HUE, [0.0, 0.15], [0.35, 1.0]);

/// Every armour, robe and helm texture with trim, by DictID, and its trim.
pub const TRIMS: &[(u32, Trim)] = &[
    // set 1, cloth: chest, bracer, gloves, legs, boots
    (0xBF4B_7E0A, Trim::Parts(&[CLOTH])),
    (0x6244_4D1A, Trim::Parts(&[cloth(&[[0.0, 0.0, 1.0, 0.35]])])),
    (0x84D2_F275, Trim::Parts(&[cloth(&[[0.25, 0.0, 0.75, 0.3]])])),
    (0x2CB5_3288, Trim::Parts(&[cloth(&[[0.0, 0.1, 0.5, 0.75]]), TrimPart { speck: 0.001, ..cloth(&[[0.0, 0.9, 1.0, 1.0]]) }])),
    (0x7AD7_D1B1, Trim::Parts(&[cloth(&[[0.0, 0.0, 1.0, 0.3], [0.55, 0.5, 1.0, 0.85]])])),
    // set 2, leather (the gloves have none)
    (0x72D0_73EB, Trim::Parts(&[STUDS, band(&[[0.25, 0.34, 0.44, 0.62], [0.56, 0.0, 0.75, 0.28]])])),
    (0xB99B_D955, Trim::Parts(&[band(&[[0.0, 0.0, 1.0, 0.34]])])),
    (0xA73D_0D53, Trim::Parts(&[STUDS, band(&[[0.0, 0.5, 1.0, 0.6], [0.27, 0.48, 0.45, 0.65], [0.54, 0.48, 0.73, 0.65]])])),
    (0xF55F_AC7C, Trim::Parts(&[band(&[[0.0, 0.0, 1.0, 0.17]])])),
    // set 3, chain (the bracer and gloves are mail alone)
    (0x5810_46F4, Trim::Parts(LEATHER_CHAIN)),
    (0x06CA_3DD6, Trim::Parts(LEATHER_CHAIN)),
    (0x54EC_DCFF, Trim::Parts(LEATHER_CHAIN)),
    // set 4, the plain plate (the gloves have none)
    (0x40B0_DDA7, Trim::Gold),
    (0xEB41_6A29, Trim::Gold),
    (0xF506_17E7, Trim::Gold),
    (0x4328_B710, Trim::Gold),
    // set 5, banded
    (0x39BC_F85F, Trim::Parts(BANDED)),
    (0xE60E_EF91, Trim::Parts(BANDED)),
    (0xFF44_6CCA, Trim::Parts(BANDED)),
    (0x75F2_A4CF, Trim::Parts(BANDED)),
    (0xC415_43F8, Trim::Parts(BANDED)),
    // set 6
    (0xF94B_D113, Trim::Parts(WOOD)),
    (0xA0A2_488D, Trim::Parts(&[band(&[[0.0, 0.0, 1.0, 0.45]])])),
    (0xBED3_457E, Trim::Parts(&[band(&[[0.25, 0.03, 0.78, 0.38]])])),
    (0x6F97_E30B, Trim::Parts(WOOD)),
    (0xBDBA_8234, Trim::Parts(&[band(&[[0.0, 0.0, 1.0, 0.1]])])),
    // set 7, scale
    (0x2603_BB5F, Trim::Parts(LEATHER)),
    (0x238E_0091, Trim::Parts(LEATHER)),
    (0xEB8B_2FCA, Trim::Parts(LEATHER)),
    (0x8D3F_65CF, Trim::Parts(LEATHER)),
    (0xDB62_04F8, Trim::Parts(LEATHER)),
    // set 8 (the boots have none)
    (0x3FDB_0DDB, Trim::Parts(&[band(&[[0.0, 0.0, 0.31, 0.07], [0.5, 0.0, 0.5625, 0.31], [0.5, 0.94, 0.5625, 1.0], [0.0, 0.94, 0.31, 1.0], [0.69, 0.45, 1.0, 0.5], [0.69, 0.75, 1.0, 0.81]])])),
    (0x8134_3DA5, Trim::Parts(&[band(&[[0.0, 0.0, 1.0, 0.14], [0.0, 0.83, 1.0, 1.0]])])),
    (0x0562_8246, Trim::Parts(&[band(&[[0.38, 0.08, 0.69, 0.28]])])),
    (0x0E6C_08A3, Trim::Parts(&[band(&[[0.0, 0.0, 1.0, 0.07]])])),
    // set 9, the ornate plate (`vi::cust::ORNATE`; the gloves have none)
    (0xC214_1C0A, Trim::Gold),
    (0x03C1_BE9E, Trim::Gold),
    (0x432C_4890, Trim::Gold),
    (0xB179_23D2, Trim::Gold),
    // the robes, top and skirt (robe 2's and robe 3's skirts have none)
    (0x455F_0D60, Trim::Parts(&[RED, ROBE_GOLD])),
    (0x7DDA_79FA, Trim::Parts(&[RED, ROBE_GOLD])),
    (0xA89B_E30B, Trim::Parts(&[ROBE_STEEL])),
    (0x0D44_77B3, Trim::Parts(&[TrimPart { rects: &[[0.0, 0.75, 1.0, 1.0]], ..ROBE_STEEL }])),
    (0x5D2F_97E1, Trim::Parts(&[RED])),
    (0x2720_732E, Trim::Parts(&[band(&[[0.29, 0.0, 0.53, 0.33]])])),
    // the helms' textures (helms 4 and 6 have none)
    (0x2B59_1A0C, Trim::Parts(&[band(&[[0.0, 0.13, 1.0, 0.3]]), cloth(&[[0.0, 0.55, 0.75, 0.95]])])),
    (0x52B1_66B3, Trim::Parts(&[TrimPart { val: [0.55, 1.0], ..STUDS }])),
    (0xDF3A_8004, Trim::Parts(&[band(&[[0.0, 0.36, 1.0, 0.5]])])),
    (
        0xF942_4951,
        Trim::Parts(&[TrimPart { rects: &[[0.0, 0.28, 0.82, 0.95]], ..BANDED[0] }, TrimPart { rects: &[[0.0, 0.28, 0.82, 0.95]], ..BANDED[1] }]),
    ),
    (0xAD2D_F46F, Trim::Parts(WOOD)),
    (0x6BA7_2C60, Trim::Parts(&[band(&[[0.0, 0.27, 1.0, 0.42]])])),
];

/// A texture's trim; none for a texture with none.
pub fn trim_of(tex: u32) -> Option<Trim> {
    TRIMS.iter().find(|(id, _)| *id == tex).map(|&(_, t)| t)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No texture twice; the plate's eight are the gold, and nothing else is.
    #[test]
    fn the_table() {
        for (i, (a, _)) in TRIMS.iter().enumerate() {
            assert!(TRIMS[i + 1..].iter().all(|(b, _)| b != a), "{a:08X} twice");
        }
        let gold: Vec<u32> = TRIMS.iter().filter(|(_, t)| *t == Trim::Gold).map(|&(id, _)| id).collect();
        assert_eq!(gold, [0x40B0_DDA7, 0xEB41_6A29, 0xF506_17E7, 0x4328_B710, 0xC214_1C0A, 0x03C1_BE9E, 0x432C_4890, 0xB179_23D2]);
    }

    /// `Trim::mask`: a colour part's texels crisp (1 or 0, its eased edge
    /// gone), a lone speck of its colour dropped and a pinhole in a patch
    /// filled; a part with no clean-up keeps a lone texel; the gold as
    /// `weight` says, eased.
    #[test]
    fn the_mask_is_cleaned() {
        let (w, h) = (16usize, 16usize);
        let brown = [110u8, 70, 35, 255];
        let yellow = [200u8, 170, 90, 255];
        let mut px = vec![brown; w * h];
        for y in 2..8 {
            for x in 2..12 {
                px[y * w + x] = yellow;
            }
        }
        px[4 * w + 6] = brown; // a pinhole
        px[12 * w + 12] = yellow; // a speck
        px[13 * w + 3] = [200, 160, 90, 255]; // an eased texel, alone
        let rgba: Vec<u8> = px.concat();
        static PART: TrimPart = TrimPart { hue: [41.0, 64.0], sat: [0.2, 1.0], val: [0.5, 1.0], rects: &[], speck: 0.02, close: false };
        let m = Trim::Parts(std::slice::from_ref(&PART)).mask(&rgba, w, h);
        assert!(m.iter().all(|&v| v == 0.0 || v == 1.0), "crisp");
        assert_eq!((m[3 * w + 3], m[4 * w + 6], m[12 * w + 12], m[13 * w + 3], m[0]), (1.0, 1.0, 0.0, 0.0, 0.0));
        static RAW: TrimPart = TrimPart { hue: [41.0, 64.0], sat: [0.2, 1.0], val: [0.5, 1.0], rects: &[], speck: 0.0, close: false };
        let raw = Trim::Parts(std::slice::from_ref(&RAW)).mask(&rgba, w, h);
        assert_eq!((raw[12 * w + 12], raw[4 * w + 6]), (1.0, 0.0), "no clean-up: the speck and the hole as they are");
        let gold = Trim::Gold.mask(&rgba, w, h);
        assert_eq!(gold[13 * w + 3], crate::recolor::trim_weight([200, 160, 90]));
    }

    /// A part's windows: inside wholly, past the ease none, between eased;
    /// a hue window through red wraps; a grey has no hue (none, unless the
    /// window is any colour); a rectangle keeps its outside out.
    #[test]
    fn a_parts_weight() {
        let p = TrimPart { hue: [41.0, 64.0], sat: [0.2, 1.0], val: [0.5, 1.0], rects: &[], speck: 0.0, close: false };
        assert_eq!(p.weight([200, 170, 90], 0.5, 0.5), 1.0, "a yellow band");
        assert_eq!(p.weight([110, 70, 35], 0.5, 0.5), 0.0, "brown leather");
        assert_eq!(p.weight([128, 128, 128], 0.5, 0.5), 0.0, "a grey");
        let w = p.weight([200, 160, 90], 0.5, 0.5);
        assert!(w > 0.0 && w < 1.0, "a hue at the edge: {w}");
        assert_eq!(RED.weight([200, 40, 40], 0.5, 0.5), 1.0);
        assert_eq!(RED.weight([200, 40, 70], 0.5, 0.5), 1.0, "past 345");
        assert_eq!(RED.weight([40, 200, 40], 0.5, 0.5), 0.0);
        assert_eq!(STUDS.weight([200, 200, 205], 0.5, 0.5), 1.0, "any hue takes a grey");
        let b = band(&[[0.0, 0.0, 1.0, 0.25]]);
        assert_eq!((b.weight([10, 20, 30], 0.5, 0.1), b.weight([10, 20, 30], 0.5, 0.3)), (1.0, 0.0));
        assert_eq!(Trim::Gold.weight([200, 160, 40], 0.0, 0.0), 1.0);
    }
}
