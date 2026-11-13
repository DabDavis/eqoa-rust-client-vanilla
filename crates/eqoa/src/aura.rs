//! A held item's own colour on its glow: go-eqoa-server sends each hand's
//! item's colour (`itempattern.color`, else its dye) in the live game's look
//! (`vi::cust::Look::item_color`, the channel's bytes 99-110 and the 0x002C
//! record's), which SOE's build read and drew nothing of. The owner,
//! 2026-11-13: "give dust a heirloom sword of twisted virtue (sword of virtue
//! with a evil aura instead of the virtuous aura)": the Sword of Virtue's
//! model in crimson (0xB0102AFF) where the disc's glows white-cyan.
//!
//! What glows on an item is three things, and each takes the colour's hue:
//! its materials' emissive colour (204, 255, 255 on the sword: F3 pass 26),
//! its additive layers (the sword's sheen, (204, 255, 255, 153)) and its
//! particle sprites (the sword's one emitter). Each keeps its own brightness
//! (`glow`), so a dark colour such as crimson still glows as brightly as the
//! cyan did; every other layer is tinted lightly (`TINT`), so a dyed or
//! coloured weapon with no glow shows its colour a little too.
//!
//! White (the server's "plain", 0xFFFFFFFF) and no colour at all (the disc's
//! look, a PS2's server) leave the item exactly as the disc draws it.

/// How far a layer that does not glow goes toward the colour: its colour
/// times `1 - TINT + TINT * colour`.
pub const TINT: f32 = 0.25;

/// The colour a hand's item is drawn in, by its slot (0 main hand, 1 off
/// hand, 2 shield): none with the disc's look or a plain (white) item.
pub fn hand(look: &vi::cust::Look, slot: usize) -> Option<[u8; 3]> {
    let c = look.item_color?.get(slot).copied()?;
    let rgb = [c[0], c[1], c[2]];
    (rgb != [255; 3]).then_some(rgb)
}

/// Each hand's (`hand`), none without a look.
pub fn hands(look: Option<&vi::cust::Look>) -> [Option<[u8; 3]>; 3] {
    std::array::from_fn(|i| look.and_then(|l| hand(l, i)))
}

/// A glow `o` (any scale: 0-255 or 0-1) in the hue of `c`: `c`'s hue and
/// saturation at `o`'s brightness (its largest channel), times how much of
/// a colour `c` is: a full colour keeps all of it (crimson, 0xB0102A, keeps
/// 97%: the sword's (204, 255, 255) emissive goes to (248, 23, 59)), a grey
/// its lightness (black puts the glow out, white keeps it white). Black stays
/// black: there is nothing to recolour.
pub fn glow(o: [f32; 3], c: [u8; 3]) -> [f32; 3] {
    let v = o[0].max(o[1]).max(o[2]);
    if v <= 0.0 {
        return o;
    }
    let hi = c[0].max(c[1]).max(c[2]) as f32;
    if hi <= 0.0 {
        return [0.0; 3];
    }
    let lo = c[0].min(c[1]).min(c[2]) as f32;
    let sat = (hi - lo) / hi;
    let k = sat + (1.0 - sat) * hi / 255.0;
    let s = v * k / hi;
    c.map(|x| x as f32 * s)
}

/// `glow` on a colour of bytes, its alpha kept.
pub fn glow8(o: [u8; 4], c: [u8; 3]) -> [u8; 4] {
    let g = glow([o[0] as f32, o[1] as f32, o[2] as f32], c);
    [byte(g[0]), byte(g[1]), byte(g[2]), o[3]]
}

/// A layer that does not glow, lightly toward `c` (`TINT`), its alpha kept.
pub fn tint8(o: [u8; 4], c: [u8; 3]) -> [u8; 4] {
    let t = |k: usize| byte(o[k] as f32 * (1.0 - TINT + TINT * c[k] as f32 / 255.0));
    [t(0), t(1), t(2), o[3]]
}

fn byte(x: f32) -> u8 {
    x.round().clamp(0.0, 255.0) as u8
}

/// An item's model in colour `c`: every material's emissive and every
/// additive layer's colour in its hue (`glow`), every other layer tinted
/// (`tint8`).
pub fn recolor(m: &mut esf::Model, c: [u8; 3]) {
    for l in m.batches.iter_mut().flat_map(|b| b.layers.iter_mut()) {
        if l.emissive[..3] != [0, 0, 0] {
            l.emissive = glow8(l.emissive, c);
        }
        l.color = if l.blend == 1 { glow8(l.color, c) } else { tint8(l.color, c) };
    }
}

/// An item's particles' billboards in colour `c` (`glow`, alpha kept).
pub fn recolor_quads(b: &mut render::billboard::Batch, c: [u8; 3]) {
    for q in &mut b.quads {
        let g = glow([q.color[0], q.color[1], q.color[2]], c);
        q.color = [g[0], g[1], g[2], q.color[3]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CRIMSON: [u8; 3] = [0xB0, 0x10, 0x2A];

    /// The look's colours by slot: white and the disc's look are none.
    #[test]
    fn a_hands_colour_off_the_look() {
        let mut l = vi::cust::Look::default();
        assert_eq!(hands(Some(&l)), [None; 3], "the disc's look: none");
        assert_eq!(hands(None), [None; 3]);
        l.item_color = Some([[0xB0, 0x10, 0x2A, 0xFF], [0xFF; 4], [0, 0, 0xFF, 0xFF]]);
        assert_eq!(hands(Some(&l)), [Some(CRIMSON), None, Some([0, 0, 0xFF])], "white is plain");
    }

    /// The sword's glow goes crimson at the cyan's brightness; a grey keeps
    /// its lightness of it; black stays black.
    #[test]
    fn the_glow_takes_the_hue_and_keeps_the_brightness() {
        assert_eq!(glow8([204, 255, 255, 255], CRIMSON), [248, 23, 59, 255]);
        assert_eq!(glow8([204, 255, 255, 153], CRIMSON), [248, 23, 59, 153], "the sheen layer's alpha kept");
        assert_eq!(glow8([204, 255, 255, 255], [255, 255, 255]), [255, 255, 255, 255]);
        assert_eq!(glow8([204, 255, 255, 255], [0x80, 0x80, 0x80]), [128, 128, 128, 255]);
        assert_eq!(glow8([204, 255, 255, 255], [0, 0, 0]), [0, 0, 0, 255], "black puts it out");
        assert_eq!(glow8([0, 0, 0, 255], CRIMSON), [0, 0, 0, 255], "no glow: none made");
        let f = glow([0.8, 1.0, 1.0], CRIMSON);
        assert!((f[0] - 248.0 / 255.0).abs() < 0.005 && f[1] < 0.1, "{f:?}: any scale");
        assert_eq!(tint8([128, 128, 128, 255], CRIMSON), [118, 98, 101, 255]);
    }

    /// `recolor` on a model like the sword's: emissive and sheen glow,
    /// the base tinted; the control, a white look, leaves it as it was.
    #[test]
    fn a_model_recoloured() {
        let layer = |blend: i32, color: [u8; 4], emissive: [u8; 4]| esf::Layer { flags: 1, texture: 7, wrap: 0, blend, color, uv_transform: [0.0; 9], lod_bias: 0.8, uv_rate: [0.0; 2], emissive };
        let e = [204, 255, 255, 255];
        let mut m = esf::Model::default();
        m.batches.push(esf::Batch { material: 0, layers: vec![layer(0, [128; 4], e), layer(1, [204, 255, 255, 153], e)], indices: vec![], lod: None });
        let before = m.batches.clone();
        recolor(&mut m, CRIMSON);
        let l = &m.batches[0].layers;
        assert_eq!((l[0].emissive, l[1].emissive), ([248, 23, 59, 255], [248, 23, 59, 255]));
        assert_eq!((l[0].color, l[1].color), ([118, 98, 101, 128], [248, 23, 59, 153]));
        assert_ne!(m.batches[0].layers, before[0].layers);
    }

    /// The Sword of Virtue off the disc in the Heirloom Sword of Twisted
    /// Virtue's crimson: all four materials' emissive and the blade's sheen
    /// red at the cyan's brightness, alphas and textures kept. Needs
    /// `run/eqoa.iso`; without it, SKIPs.
    #[test]
    fn the_sword_of_twisted_virtue() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&p) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let c = esf::Container::open(disc.read("ITEM.ESF").unwrap()).unwrap();
        let r = c.models().into_iter().find(|r| r.dict_id as u32 == 0x88EC_092D).expect("the Sword of Virtue");
        let virtue = c.build_model(r.node).expect("its model");
        let mut twisted = virtue.clone();
        recolor(&mut twisted, CRIMSON);
        for (a, b) in virtue.batches.iter().zip(&twisted.batches) {
            for (la, lb) in a.layers.iter().zip(&b.layers) {
                assert_eq!((la.emissive, lb.emissive), ([204, 255, 255, 255], [248, 23, 59, 255]));
                assert_eq!((lb.texture, lb.blend, lb.color[3]), (la.texture, la.blend, la.color[3]));
            }
        }
        assert_eq!(twisted.batches[3].layers[1].color, [248, 23, 59, 153], "the sheen");
    }
}
