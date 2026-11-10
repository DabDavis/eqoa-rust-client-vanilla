//! A tint on the bodies of the NPCs of a name, as this client draws it (the
//! server owner, 2026-11-10: Yhafiran, "the black and green oriental dragon",
//! wears the disc's only green oriental dragon, CHAR.ESF 0xB5C8A651, which the
//! Plane of Disease's twins wear too; no black and green body exists, so
//! "vanilla draws Yhafiran tinted toward black and green"; `net::npctint` has
//! the message, go-eqoa-server's `entity/npctint.go` the table).
//!
//! The server names a colour for an NPC name. A creature whose channel name
//! keys the same (`net::npctint::key`), a player's never, is built with every
//! layer colour of its body times the tint ([`apply`]): the colour the strip
//! programs modulate each texel by (the renderer's layer tint, where a live
//! look's dye goes too), so the whole body is darker or shifted in hue under the
//! game's own light and fog, and what it holds or wears on a node keeps its own
//! colours. A creature with no tint, and every creature on a server that sends
//! none, is drawn exactly as before.
//!
//! One set a thread, as `raceopts`'s looks: a new set replaces the last whole,
//! and a creature made in another tint than its name's now is made again
//! (`play::apply`).

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static TINTS: RefCell<HashMap<String, u32>> = RefCell::default();
}

/// The tints the server sent (`net::session::OP_NPC_TINTS`), by name: the whole
/// set, the last one standing. A tint that changes nothing is left out.
pub fn set(tints: Vec<(String, u32)>) {
    let tints: HashMap<String, u32> = tints.into_iter().map(|(n, c)| (net::npctint::key(&n), c)).filter(|(n, c)| !n.is_empty() && changes(*c)).collect();
    TINTS.with_borrow_mut(|t| *t = tints);
}

/// The tint the NPCs of this name (as their channel carries it) are drawn in,
/// if any.
pub fn of(name: &str) -> Option<u32> {
    TINTS.with_borrow(|t| if t.is_empty() { None } else { t.get(&net::npctint::key(name)).copied() })
}

/// The 255ths a tint 0xRRGGBBAA multiplies a colour's red, green and blue by:
/// its own, as much of each as its alpha says (0xFF the whole, 0 none: 255).
pub fn multipliers(c: u32) -> [u32; 3] {
    let a = c & 0xFF;
    [24, 16, 8].map(|s| 255 - ((255 - ((c >> s) & 0xFF)) * a + 127) / 255)
}

/// Whether a tint changes anything: one of alpha 0, or white, leaves the body
/// as it is.
pub fn changes(c: u32) -> bool {
    multipliers(c) != [255; 3]
}

/// `model` in tint `c`: each layer colour's red, green and blue times the
/// tint's ([`multipliers`]), rounded, its alpha kept (a body is no more see
/// through for a tint). The layer colour reaches the pixel as a multiplier of
/// the texel (`render`'s layer `tint`), so the whole body is multiplied; a
/// batch with no layer at all (drawn white, untextured) is left as it is.
pub fn apply(model: &mut esf::Model, c: u32) {
    let m = multipliers(c);
    if m == [255; 3] {
        return;
    }
    for l in model.batches.iter_mut().flat_map(|b| b.layers.iter_mut()) {
        for (v, m) in l.color.iter_mut().zip(m) {
            *v = ((*v as u32 * m + 127) / 255) as u8;
        }
    }
}

/// `tint=RRGGBBAA` (or `tint=0xRRGGBBAA`, or six digits, alpha FF), as `eqoa
/// shot` and `eqoa view` take one: the tint, and `args` without it.
pub fn take_arg(args: &[String]) -> Result<(Option<u32>, Vec<String>), String> {
    let mut tint = None;
    let mut rest = Vec::with_capacity(args.len());
    for a in args {
        match a.strip_prefix("tint=") {
            Some(v) => {
                let h = v.trim().trim_start_matches('#').trim_start_matches("0x").trim_start_matches("0X");
                let c = u32::from_str_radix(h, 16).ok().filter(|_| matches!(h.len(), 6 | 8)).ok_or_else(|| format!("tint={v}: RRGGBB or RRGGBBAA in hex"))?;
                tint = Some(if h.len() == 6 { c << 8 | 0xFF } else { c });
            }
            None => rest.push(a.clone()),
        }
    }
    Ok((tint, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(color: [u8; 4]) -> esf::Layer {
        esf::Layer { flags: 1, texture: 7, wrap: 0, blend: 0, color, uv_transform: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], lod_bias: 0.8, uv_rate: [0.0, 0.0], emissive: [10, 20, 30, 0] }
    }

    fn body() -> esf::Model {
        esf::Model {
            batches: vec![
                esf::Batch { material: 0, layers: vec![layer([255; 4]), layer([128, 64, 200, 77])], indices: vec![0, 1, 2], lod: None },
                esf::Batch { material: 1, layers: vec![], indices: vec![0, 2, 3], lod: Some(1) },
            ],
            ..Default::default()
        }
    }

    /// A tint multiplies every layer colour's red, green and blue, rounded,
    /// and nothing else: its alpha, the emissive, the textures and an empty
    /// batch stay as they were. A white tint, and one of alpha 0, change
    /// nothing at all (the control); half the alpha is half the way.
    #[test]
    fn a_tint_multiplies_the_layer_colours_and_nothing_else() {
        let mut m = body();
        apply(&mut m, 0x558C_50FF);
        let l = &m.batches[0].layers;
        assert_eq!(l[0].color, [0x55, 0x8C, 0x50, 255]);
        assert_eq!(l[1].color, [43, 35, 63, 77], "128x85, 64x140, 200x80 by 255, rounded; alpha kept");
        assert_eq!((l[0].emissive, l[0].texture, l[1].texture), ([10, 20, 30, 0], 7, 7));
        assert!(m.batches[1].layers.is_empty());
        for c in [0xFFFF_FFFF, 0x558C_5000, 0x0000_0000, 0xFFFF_FF80] {
            let mut same = body();
            apply(&mut same, c);
            assert_eq!(same.batches[0].layers, body().batches[0].layers, "{c:08X} changed something");
            assert!(!changes(c));
        }
        assert_eq!(multipliers(0x0000_0080), [127; 3], "half the alpha: half way to black");
        assert_eq!(multipliers(0x558C_50FF), [0x55, 0x8C, 0x50]);
        assert_eq!(multipliers(0x0000_0001), [254; 3], "the least alpha of black: a 255th");
        assert!(changes(0x558C_50FF) && changes(0x0000_0001));
        assert!(!changes(0xFFFF_FE01), "a 255th of a 255th rounds to nothing");
    }

    /// The set is by name as the server keys it, so the channel's name in any
    /// case meets it; a name not in it, and every name once the set is empty,
    /// has none; a tint that changes nothing is none.
    #[test]
    fn the_set_is_by_name() {
        set(vec![("yhafiran".into(), 0x558C_50FF), ("Uzrathixius ".into(), 0xFFFF_FFFF)]);
        assert_eq!(of("Yhafiran"), Some(0x558C_50FF));
        assert_eq!(of("YHAFIRAN "), Some(0x558C_50FF));
        assert_eq!(of("Poxathixius"), None, "the control: an untinted twin");
        assert_eq!(of("Uzrathixius"), None, "a white tint is none");
        set(Vec::new());
        assert_eq!(of("Yhafiran"), None);
    }

    /// The green oriental dragon in Yhafiran's tint, as `eqoa shot ...
    /// tint=` draws him (`viewer::Source::Models`): the library's model with
    /// its layer colours multiplied and nothing else changed, vertices,
    /// triangles, textures and every other layer field as they were; with no
    /// tint, the library's model exactly (the control: what master draws).
    /// Needs `run/eqoa.iso`.
    #[test]
    fn a_tinted_body_differs_from_the_arts_in_its_layer_colours_alone() {
        const DRAGON: i32 = 0xB5C8_A651u32 as i32;
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut iso) = iso::Iso::open(&path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        let f = lib.add("CHAR.ESF", esf::Container::open(iso.read("CHAR.ESF").unwrap()).unwrap());
        let list = crate::scene::list_models(&lib, f);
        let index = list.iter().position(|(id, _)| *id == DRAGON).expect("the dragon");
        let drawn = |tint| crate::viewer::Source::Models { list: list.clone(), index, tint }.scene(&lib).parts.remove(0).0;
        let art = lib.build_model(lib.lookup(DRAGON).unwrap()).unwrap();
        let (plain, tinted) = (drawn(None), drawn(Some(0x2A73_33FF)));
        let geometry = |m: &esf::Model| (m.pos.clone(), m.uv.clone(), m.normal.clone(), m.color.clone(), m.bones.clone(), m.weights.clone());
        let batches = |m: &esf::Model| m.batches.iter().map(|b| (b.material, b.indices.clone(), b.lod)).collect::<Vec<_>>();
        let layers = |m: &esf::Model| m.batches.iter().flat_map(|b| b.layers.clone()).collect::<Vec<_>>();
        assert_eq!((geometry(&plain), batches(&plain), layers(&plain)), (geometry(&art), batches(&art), layers(&art)), "no tint: the art's");
        assert_eq!((geometry(&tinted), batches(&tinted)), (geometry(&art), batches(&art)));
        let (want, got) = (layers(&art), layers(&tinted));
        assert!(!want.is_empty() && want.len() == got.len());
        let m = multipliers(0x2A73_33FF);
        for (a, t) in want.iter().zip(&got) {
            let c = [0, 1, 2].map(|k| ((a.color[k] as u32 * m[k] + 127) / 255) as u8);
            assert_eq!(t.color, [c[0], c[1], c[2], a.color[3]]);
            assert_eq!(esf::Layer { color: a.color, ..t.clone() }, *a, "only the colour changed");
        }
        assert_ne!(want, got, "the tint changed something");
    }

    /// `tint=` comes out of the arguments, in any of its spellings; a bad one
    /// is refused; none leaves the arguments as they were.
    #[test]
    fn the_tint_argument() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(take_arg(&args(&["0xB5C8A651", "tint=558C50FF"])).unwrap(), (Some(0x558C_50FF), args(&["0xB5C8A651"])));
        assert_eq!(take_arg(&args(&["tint=0x558c50", "0.6"])).unwrap(), (Some(0x558C_50FF), args(&["0.6"])));
        assert_eq!(take_arg(&args(&["tint=#558C5080"])).unwrap().0, Some(0x558C_5080));
        assert_eq!(take_arg(&args(&["zone", "3"])).unwrap(), (None, args(&["zone", "3"])));
        assert!(take_arg(&args(&["tint=green"])).is_err());
        assert!(take_arg(&args(&["tint=12345"])).is_err());
    }
}
