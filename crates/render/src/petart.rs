//! Which texels of a pet body's textures are its clothing: what a pet's
//! clothing dye draws in its colour (`recolor::CLOTH`) and its body dye leaves
//! (`recolor::BARE`), go-eqoa-server's Pet Groomers' two areas (the server
//! owner, 2026-11-16: "the pets need their clothes as one dye and the rest
//! another like we two tone equipment").
//!
//! A texture is all clothing ([`Cloth::Whole`]: an animation's plate, a
//! construct's riveted plating, a djinni's robe), part clothing
//! ([`Cloth::Parts`]: an atlas of skin and cloth, the parts `trimart`'s
//! [`TrimPart`]s: a rectangle of the texture, a colour window), or, not here,
//! the body's own (skin, fur, hide, bone, flame). Read off the disc's art by
//! eye, a texture at a time, for the summoned pets' bodies (the eqoa crate's
//! `eqoa dress <body> out.png petareas=1`, `EQOA_TEXDUMP`); the art is the
//! disc's, read at run time: nothing here is game data but these numbers. The
//! armour sets' and robes' own textures are clothing wherever they are worn
//! (the eqoa crate's `petdye`, by `vi::cust::is_garment`), so a charmed
//! creature in a set's armour two-tones too.

use crate::trimart::TrimPart;

/// Which texels of a texture are clothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cloth {
    /// Every texel.
    Whole,
    /// The most of any of the parts' weights (`trimart`'s).
    Parts(&'static [TrimPart]),
}

impl Cloth {
    /// The clothing of a `w` by `h` texture (`rgba`), a weight a texel, 0 or 1.
    pub fn mask(&self, rgba: &[u8], w: usize, h: usize) -> Vec<f32> {
        match self {
            Cloth::Whole => vec![1.0; w * h],
            Cloth::Parts(p) => crate::trimart::Trim::Parts(p).mask(rgba, w, h),
        }
    }
}

const ANY: [f32; 2] = [0.0, 1.0];
const ANY_HUE: [f32; 2] = [0.0, 360.0];

/// The texels inside `rects` (`[u0, v0, u1, v1]`, 0..1 across and down).
const fn rects(rects: &'static [[f32; 4]]) -> TrimPart {
    TrimPart { hue: ANY_HUE, sat: ANY, val: ANY, rects, speck: 0.0, close: false }
}

/// A djinn's pink sash: its pink inside the texture's left column, the skin
/// beside it left out.
const PINK_SASH: TrimPart = TrimPart { hue: [290.0, 365.0], sat: [0.2, 1.0], val: [0.25, 1.0], rects: &[[0.0, 0.0, 0.32, 1.0]], speck: 0.002, close: true };
/// A djinn's teal arm band and the strip beside its face.
const DJINN_BANDS: TrimPart = rects(&[[0.42, 0.33, 0.5, 0.7], [0.86, 0.33, 1.0, 0.7]]);
/// The Ghilan's purple robe and its gold, round the face's frame.
const GHILAN_ROBE: TrimPart = rects(&[[0.0, 0.0, 0.2, 0.27], [0.68, 0.0, 1.0, 0.5], [0.68, 0.73, 1.0, 1.0], [0.0, 0.93, 0.2, 1.0]]);

/// Every pet texture with clothing in it, by DictID.
pub const CLOTHES: &[(u32, Cloth)] = &[
    // Jarek's Creation (0x15764DA8), the constructs (0x9CF75E81) and the
    // gnome's invention (0xFBABEB00): the riveted plating, the sleeves, the
    // gauntlets and the dark plates (their faces, hair and the spider are
    // their bodies')
    (0xFF88_DE6B, Cloth::Whole),
    (0x8A4C_D4D5, Cloth::Whole),
    (0xC510_52D6, Cloth::Whole),
    (0xEEA9_F0D3, Cloth::Whole),
    (0x3CCC_8FFC, Cloth::Whole),
    // the animations (0xDC3B344D-0xDC3B3450): their plate, set 4's and their
    // own; all of an animation is plate, so it is dyed whole (`petdye`)
    (0x0638_5212, Cloth::Whole),
    (0x1A84_5C4B, Cloth::Whole),
    (0x35D5_513E, Cloth::Whole),
    (0x68A6_FB74, Cloth::Whole),
    (0x704D_DCD3, Cloth::Whole),
    (0xBD30_8F78, Cloth::Whole),
    (0xDBE8_25CD, Cloth::Whole),
    (0xEE48_011B, Cloth::Whole),
    (0x40B0_DDA7, Cloth::Whole),
    (0x4328_B710, Cloth::Whole),
    (0xEB41_6A29, Cloth::Whole),
    (0xF506_17E7, Cloth::Whole),
    // the djinni vizier's robe (0xDBDA1FE9)
    (0x1EF3_D84F, Cloth::Whole),
    (0x30DF_D665, Cloth::Parts(&[rects(&[[0.0, 0.0, 0.5, 1.0]])])),
    (0xE11D_5821, Cloth::Parts(&[PINK_SASH, DJINN_BANDS])),
    // the djinn (0x2F4EA104): its curtain skirt and chain, its sash and bands
    (0xDFA4_099B, Cloth::Parts(&[rects(&[[0.0, 0.0, 0.5, 0.75], [0.5, 0.7, 1.0, 0.78]])])),
    (0x4F83_8CC3, Cloth::Parts(&[PINK_SASH, DJINN_BANDS])),
    // the efreeti (0xEBBC3236, 0xAA489CBB): the dark cloth, the belt and its
    // buckle, the boots
    (0x1297_157B, Cloth::Parts(&[rects(&[[0.0, 0.0, 0.5, 0.2], [0.0, 0.75, 1.0, 1.0]])])),
    (0x61E2_A063, Cloth::Parts(&[rects(&[[0.42, 0.35, 0.5, 0.65], [0.65, 0.35, 1.0, 0.65]])])),
    (0x0808_CC05, Cloth::Parts(&[rects(&[[0.0, 0.0, 0.5, 0.3], [0.0, 0.75, 1.0, 1.0]])])),
    (0xFB13_0901, Cloth::Parts(&[rects(&[[0.4, 0.3, 0.5, 0.7], [0.68, 0.3, 1.0, 0.7]])])),
    (0xD8EC_DDFD, Cloth::Parts(&[rects(&[[0.0, 0.45, 1.0, 1.0]])])),
    // the Ghilan (0x64B20292) and the Ghilan Noble (0x90DCF1D7): the curtain
    // trousers (the Noble's hands share their texture), the purple robe and its
    // gold, the rope and scarf, the sandals
    (0x4F99_0CCF, Cloth::Whole),
    (0x99E4_305F, Cloth::Parts(&[GHILAN_ROBE])),
    (0xEAE5_8B35, Cloth::Parts(&[GHILAN_ROBE])),
    (0x9DBB_ABF8, Cloth::Parts(&[rects(&[[0.0, 0.45, 1.0, 1.0]])])),
    (0xE6B0_82C1, Cloth::Parts(&[rects(&[[0.27, 0.0, 0.5, 0.18], [0.5, 0.0, 1.0, 1.0], [0.0, 0.82, 0.5, 1.0]])])),
    // the Badi (0x1EB3AAEC): the curtain and the turban
    (0x74B3_E243, Cloth::Parts(&[rects(&[[0.5, 0.0, 1.0, 0.15], [0.5, 0.85, 1.0, 1.0]])])),
    (0x969F_6ABB, Cloth::Parts(&[rects(&[[0.68, 0.28, 1.0, 0.47], [0.68, 0.75, 1.0, 0.88]])])),
    // the zombies (0x1BBB898C): their brown rags on the grey flesh
    (0xE113_0965, Cloth::Parts(&[TrimPart { hue: [10.0, 45.0], sat: [0.3, 1.0], val: [0.1, 0.65], rects: &[], speck: 0.002, close: true }])),
];

/// A texture's clothing; none for a texture of none (the body's own).
pub fn cloth_of(tex: u32) -> Option<Cloth> {
    CLOTHES.iter().find(|(id, _)| *id == tex).map(|&(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// No texture twice; a part's rectangles inside the texture.
    #[test]
    fn the_table() {
        for (i, (a, c)) in CLOTHES.iter().enumerate() {
            assert!(CLOTHES[i + 1..].iter().all(|(b, _)| b != a), "{a:08X} twice");
            if let Cloth::Parts(p) = c {
                for r in p.iter().flat_map(|p| p.rects.iter()) {
                    assert!(r.iter().all(|v| (0.0..=1.0).contains(v)) && r[0] < r[2] && r[1] < r[3], "{a:08X}: {r:?}");
                }
            }
        }
    }

    /// A whole texture is all clothing; a part's rectangle is its texels
    /// alone (the control: those outside are the body's).
    #[test]
    fn a_mask_is_its_parts() {
        let rgba = vec![200u8; 4 * 4 * 4];
        assert_eq!(Cloth::Whole.mask(&rgba, 4, 4), vec![1.0; 16]);
        const CORNER: &[TrimPart] = &[rects(&[[0.0, 0.0, 0.5, 0.5]])];
        let m = Cloth::Parts(CORNER).mask(&rgba, 4, 4);
        assert_eq!((m[0], m[5], m[2], m[15]), (1.0, 1.0, 0.0, 0.0));
        assert_eq!(cloth_of(0xFF88_DE6B), Some(Cloth::Whole));
        assert_eq!(cloth_of(0xAC3E_6DF0), None, "the spider: the body's");
    }
}
