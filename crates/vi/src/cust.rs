//! Character customisation (`VICSpriteCust`, decomp `src/vicspritecust.cpp`,
//! and `VICSprite::SetRobe`/`SetArmorSlot`/`SetHair`/`SetFace`,
//! `vicsprite.cpp`): a look becomes texture and colour changes on the layers
//! the sprite's text slots name. `SetResources`' tables are DictIDs here (the
//! hashes it looks up); a renderer resolves them as it resolves any texture.
//!
//! Not here: the helm (an attached sprite, `SetHelm`), which also hides hair
//! slots while worn.

use crate::csprite::CSprite;
use crate::fpu::Model;

/// `Armor[set][slot]`: chest, boots, bracer, legs, gloves textures per set.
const ARMOR: [[u32; 5]; 9] = {
    let mut a = [[0u32; 5]; 9];
    a[1] = [0xBF4B7E0A, 0x62444D1A, 0x84D2F275, 0x2CB53288, 0x7AD7D1B1];
    a[2] = [0x72D073EB, 0xB99BD955, 0x3857E856, 0xA73D0D53, 0xF55FAC7C];
    a[3] = [0x581046F4, 0xC0EC54C8, 0x1D97BB5F, 0x06CA3DD6, 0x54ECDCFF];
    a[4] = [0x40B0DDA7, 0xEB416A29, 0x06385212, 0xF50617E7, 0x4328B710];
    a[5] = [0x39BCF85F, 0xE60EEF91, 0xFF446CCA, 0x75F2A4CF, 0xC41543F8];
    a[6] = [0xF94BD113, 0xA0A2488D, 0xBED3457E, 0x6F97E30B, 0xBDBA8234];
    a[7] = [0x2603BB5F, 0x238E0091, 0xEB8B2FCA, 0x8D3F65CF, 0xDB6204F8];
    a[8] = [0x3FDB0DDB, 0x81343DA5, 0x05628246, 0x0E6C08A3, 0x5C8EA7CC];
    a
};
const HAIR: [u32; 8] = [0x98F86489, 0x221E7298, 0x6DCEF2D0, 0xFEBCE4D5, 0xD499494B, 0x8931CEE6, 0x9F87C7AF, 0xDC79B7C3];
const HAIR_NULL: u32 = 0x431B69AA;
const ROBE: [[u32; 2]; 4] = [[0x455F0D60, 0x7DDA79FA], [0xA89BE30B, 0x0D4477B3], [0x5D2F97E1, 0x455D0BA5], [0x2720732E, 0xB64B5794]];
/// `Race[race][face][sex]`: the face textures.
const RACE: [[[u32; 2]; 4]; 11] = [
    [[0x4A7AB44E, 0x765CFD7D], [0x5C086A9F, 0x87EAB3CE], [0x6D9620F0, 0x99786A1F], [0x7F23D741, 0xAB062070]],
    [[0x74732F1C, 0xF83D38BB], [0x8600E56D, 0x09CAEF0C], [0x978E9BBE, 0x1B58A55D], [0xA91C520F, 0x2CE65BAE]],
    [[0x42FB8FD2, 0xEC584121], [0x54894623, 0xFDE5F772], [0x6616FC74, 0x0F73ADC3], [0x77A4B2C5, 0x21016414]],
    [[0x99B0BB43, 0x63155B1A], [0xAB3E7194, 0x74A3116B], [0xBCCC27E5, 0x8630C7BC], [0xCE59DE36, 0x97BE7E0D]],
    [[0x6F7E7049, 0xBDB14A50], [0x810C269A, 0xCF3F00A1], [0x9299DCEB, 0xE0CCB6F2], [0xA427933C, 0xF25A6D43]],
    [[0x12A88832, 0x7E6E1C81], [0x24363E83, 0x8FFBD2D2], [0x35C3F4D4, 0xA1898923], [0x4751AB25, 0xB3173F74]],
    [[0xD9EC0301, 0x28F19AC8], [0xEB79B952, 0x3A7F5119], [0xFD076FA3, 0x4C0D076A], [0x0E9525F4, 0x5D9ABDBB]],
    [[0x6D7EAC68, 0xBB6F8467], [0x7F0C62B9, 0xCCFD3AB8], [0x909A190A, 0xDE8AF109], [0xA227CF5B, 0xF018A75A]],
    [[0xD4259B41, 0x07D0B508], [0xE5B35192, 0x195E6B59], [0xF74107E3, 0x2AEC21AA], [0x08CEBE34, 0x3C79D7FB]],
    [[0; 2]; 4],
    [[0xFD04B372, 0xDAFD61C1], [0x0E9269C3, 0xEC8B1812], [0x20202014, 0xFE18CE63], [0x31ADD665, 0x0FA684B4]],
];
/// `Helm[set]`: the helm sprites.
const HELM: [u32; 9] = [0, 0x0C6F2FDA, 0x330D43A7, 0x079EAC92, 0x03B8E453, 0x5A51ACC1, 0x7184EC3D, 0x91E411BB, 0x1E25A25F];

/// `GetHelm`: the helm sprite for an armour set (`SetArmorSlot` slot 5 hands
/// it to `SetHelm`, which attaches a copy at node 8 coloured with the tint).
pub fn helm(set: u8) -> Option<u32> {
    helm_with(set, &Tables::retail())
}

/// `GetHelm` by a patch's tables: `SetArmorSlot`'s clamp, then the patch's
/// remap (Sandstorm's set 9 wears set 4's helm), then `$Helm`, whose slots
/// past 8 retail never fills.
pub fn helm_with(set: u8, t: &Tables) -> Option<u32> {
    let set = if set >= t.armor_limit { 0 } else { set };
    let set = t.helm_remap.iter().find(|(from, _)| *from == set).map_or(set, |&(_, to)| to) as usize;
    if set == 0 { None } else { HELM.get(set).copied() }
}

/// `Tints`, RGBA.
pub const TINTS: [[u8; 4]; 15] = [
    [255, 255, 255, 255],
    [90, 90, 90, 255],
    [0, 180, 255, 255],
    [150, 100, 50, 255],
    [0, 0, 140, 255],
    [0, 200, 50, 255],
    [250, 175, 50, 255],
    [80, 100, 0, 255],
    [240, 60, 60, 255],
    [165, 245, 255, 255],
    [160, 170, 180, 255],
    [220, 190, 150, 255],
    [0, 200, 180, 255],
    [150, 0, 60, 255],
    [240, 240, 0, 255],
];

/// The customisation tables a look is dressed by: the retail disc's, or as a
/// server's patch leaves them (`GetTintColor`'s table and `SetArmorSlot`'s
/// clamps, `GetHelm`, `GetRobeTexture`; the eqoa crate's `s2`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tables {
    /// `GetTintColor`'s table
    pub tints: Vec<[u8; 4]>,
    /// `SetArmorSlot` keeps a tint below this, else 0 (retail 15)
    pub tint_limit: usize,
    /// `SetArmorSlot`: a set at or past it is 0 (retail 9)
    pub armor_limit: u8,
    /// `GetHelm`: a set that wears another set's helm
    pub helm_remap: Vec<(u8, u8)>,
    /// `GetRobeTexture`: robes 0 .. limit-1; one at or past it is robe 0
    /// (retail 4)
    pub robe_limit: u8,
    /// a robe past retail's four: its (chest, legs) DictIDs, robe 0's where
    /// one does not resolve
    pub robe_art: Vec<(u8, [u32; 2])>,
    /// a set past `$Armor`'s nine: its five slots' DictIDs (Sandstorm's
    /// set 9, `GetArmorSetTexture`'s hook)
    pub armor_extra: Vec<(u8, [u32; 5])>,
}

impl Tables {
    /// The retail disc's.
    pub fn retail() -> Tables {
        Tables { tints: TINTS.to_vec(), tint_limit: 15, armor_limit: 9, helm_remap: Vec::new(), robe_limit: 4, robe_art: Vec::new(), armor_extra: Vec::new() }
    }

    /// A tint's colour as `SetArmorSlot` takes it: past the limit, tint 0.
    pub fn tint(&self, i: u8) -> [u8; 4] {
        let i = if (i as usize) < self.tint_limit { i as usize } else { 0 };
        self.tints.get(i).or(self.tints.first()).copied().unwrap_or(TINTS[0])
    }

    /// A tint as `SetRobe` and `SetHelm` read it (`GetTintColor` with no
    /// clamp of its own): 14 at most on retail's table, as the client has
    /// always taken it; a patch's table to its end.
    pub fn tint_at_most(&self, i: u8) -> [u8; 4] {
        if self.tint_limit > 15 { self.tint(i) } else { self.tints.get((i as usize).min(14)).copied().unwrap_or(TINTS[0]) }
    }
}

/// A look, as a creature update carries it (`ApplyLook`'s arguments).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Look {
    /// Armour set and tint per slot: chest, boots, bracer, legs, gloves, helm.
    pub armor: [u8; 6],
    pub armor_tint: [u8; 6],
    /// Colour, length, style.
    pub hair: [u8; 3],
    pub face: u8,
    /// Negative: no robe.
    pub robe: i8,
    pub robe_tint: u8,
    /// SOE's live game's colours (its channel and 0x002C record carry one per
    /// armour slot and the robe where the disc's carry a tint): each the
    /// slot's layer colour, as the live `SetArmorSlot` / `SetRobe` /
    /// `SetHelm` set it with no ArmorGroup, in place of the tint's. None: the
    /// tints'.
    pub armor_color: Option<[[u8; 4]; 6]>,
    pub robe_color: Option<[u8; 4]>,
}

impl Look {
    /// The helm's colour (`SetHelm` colours every layer of its copy): the
    /// live game's, else its tint's as `SetHelm` reads it.
    pub fn helm_color(&self, t: &Tables) -> [u8; 4] {
        self.armor_color.map_or_else(|| t.tint_at_most(self.armor_tint[5]), |a| a[5])
    }
}

/// One change to one layer of one palette entry. `texture` None keeps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerChange {
    pub entry: i32,
    pub layer: i32,
    pub texture: Option<u32>,
    pub color: Option<[u8; 4]>,
}

/// `ApplyLook` on a CSprite: `SetRobe`, `SetArmorSlot` for the five worn
/// slots, `SetHair`, `SetFace`, in that order; later changes to a layer win.
/// `has` tells whether a texture DictID resolves (`FindTyped`).
pub fn apply_look<M: Model>(cs: &CSprite<M>, look: &Look, helm_worn: bool, has: impl Fn(u32) -> bool) -> Vec<LayerChange> {
    apply_look_with(cs, look, helm_worn, has, &Tables::retail())
}

/// `ApplyLook` by a patch's tables (`Tables`): retail's give `apply_look`.
#[allow(clippy::needless_range_loop)]
pub fn apply_look_with<M: Model>(cs: &CSprite<M>, look: &Look, helm_worn: bool, has: impl Fn(u32) -> bool, t: &Tables) -> Vec<LayerChange> {
    let mut out = Vec::new();
    let slot = |i: usize| cs.text_slots[i];
    let found = |id: u32| if id != 0 && has(id) { Some(id) } else { None };
    // Which slots the robe took over (`m_slots[i].m_flags & 1`).
    let mut robed = [false; 5];
    // SetRobe
    if look.robe >= 0 {
        let robe = if look.robe as u8 >= t.robe_limit { 0 } else { look.robe as usize };
        // a patch robe: its art, robe 0's where a half does not resolve
        let art = |half: usize| match robe {
            0..=3 => found(ROBE[robe][half]),
            _ => t.robe_art.iter().find(|(k, _)| *k as usize == robe).and_then(|(_, a)| found(a[half])).or_else(|| found(ROBE[0][half])),
        };
        for i in 0..5 {
            let (entry, layer) = slot(i);
            if entry == -1 {
                continue;
            }
            let tex = match i {
                0 => art(0),
                3 => art(1),
                _ => None,
            };
            if let Some(tex) = tex {
                robed[i] = true;
                let color = look.robe_color.unwrap_or_else(|| t.tint_at_most(look.robe_tint));
                out.push(LayerChange { entry, layer, texture: Some(tex), color: Some(color) });
            }
        }
    }
    // SetArmorSlot for chest..gloves (the helm is an attachment).
    for i in 0..5 {
        let (entry, layer) = slot(i);
        if entry == -1 {
            continue;
        }
        let set = if look.armor[i] >= t.armor_limit { 0 } else { look.armor[i] as usize };
        let extra = || t.armor_extra.iter().find(|(k, _)| *k as usize == set).map(|(_, a)| a[i]);
        let tex = if set == 0 { None } else { ARMOR.get(set).map(|a| a[i]).or_else(extra).and_then(found) };
        if (set != 0 && tex.is_none()) || robed[i] {
            continue;
        }
        let color = look.armor_color.map_or_else(|| t.tint(look.armor_tint[i]), |a| a[i]);
        out.push(LayerChange { entry, layer, texture: tex, color: Some(color) });
    }
    // SetHair: every hair slot must exist.
    if (11..16).all(|i| slot(i).0 != -1) {
        let (hair, a, b) = (look.hair[0] as i32, look.hair[1] as i32, look.hair[2] as i32);
        let null = found(HAIR_NULL);
        let tex = found(HAIR[hair.clamp(0, 7) as usize]).or(null);
        if let Some(tex) = tex {
            let pick = |show: bool| if show { Some(tex) } else { null };
            let mut set = |i: usize, t: Option<u32>| {
                let (entry, layer) = slot(i);
                if entry != -1 && t.is_some() {
                    out.push(LayerChange { entry, layer, texture: t, color: None });
                }
            };
            for i in 8..11 {
                set(i, pick(((i - 8) as i32) < b));
            }
            for i in 16..19 {
                set(i, pick(((i - 16) as i32) < b && !helm_worn));
            }
            for i in 11..14 {
                set(i, pick(((i - 11) as i32) < a && !helm_worn));
            }
            set(14, pick(a >= 2 && b >= 2 && !helm_worn));
            set(15, Some(tex));
        }
    }
    // SetFace
    let (entry, layer) = slot(7);
    if entry != -1 && (0..11).contains(&cs.race) && look.face < 4 {
        let id = RACE[cs.race as usize][look.face as usize][(cs.sex != 0) as usize];
        if let Some(t) = found(id) {
            out.push(LayerChange { entry, layer, texture: Some(t), color: None });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpu::{Ee, F};
    use crate::hsprite::HSprite;

    /// A sprite whose five worn slots are palette entries 10 to 14, layer 0.
    fn sprite() -> CSprite<Ee> {
        let mut cs = CSprite::new(HSprite::new(0), 1, 0, 0, F::new(1.0), true);
        for i in 0..5 {
            cs.text_slots[i] = (10 + i as i32, 0);
        }
        cs
    }

    /// Sandstorm's tables as its S2 leaves them: 30 tints (15 clear), sets
    /// below 10, set 9 wearing set 4's helm, robes 4 and 5 out of its art.
    fn sandstorm() -> Tables {
        let mut tints = TINTS.to_vec();
        tints.extend((15..30u8).map(|i| [i, i, i, if i == 15 { 0 } else { 255 }]));
        Tables { tints, tint_limit: 30, armor_limit: 10, helm_remap: vec![(9, 4)], robe_limit: 6, robe_art: vec![(4, [0xDEAD_BEEF, 0xCAFE_BABE]), (5, [0xDEAD_BEFE, 0xCAFE_BAEB])], armor_extra: Vec::new() }
    }

    fn change(out: &[LayerChange], entry: i32) -> Option<LayerChange> {
        out.iter().rev().find(|c| c.entry == entry).copied()
    }

    /// A patch's robes, tints, sets and helm, each against retail's reading
    /// of the same look (the control: retail's tables give the old answers).
    #[test]
    fn a_patch_dresses_by_its_own_tables() {
        let cs = sprite();
        let all = |_: u32| true;
        let (retail, ss) = (Tables::retail(), sandstorm());
        // robe 4 in tint 20: retail draws robe 0 in tint 14, the patch its own robe and tint
        let look = Look { robe: 4, robe_tint: 20, ..Default::default() };
        let r = apply_look_with(&cs, &look, false, all, &retail);
        assert_eq!(change(&r, 10).unwrap().texture, Some(ROBE[0][0]));
        assert_eq!(change(&r, 10).unwrap().color, Some(TINTS[14]));
        let s = apply_look_with(&cs, &look, false, all, &ss);
        assert_eq!(change(&s, 10).unwrap().texture, Some(0xDEAD_BEEF));
        assert_eq!(change(&s, 13).unwrap().texture, Some(0xCAFE_BABE));
        assert_eq!(change(&s, 10).unwrap().color, Some(ss.tints[20]));
        // its art not there: robe 0's, as GetRobeTexture's cave falls back
        let s = apply_look_with(&cs, &look, false, |id| id != 0xDEAD_BEEF, &ss);
        assert_eq!(change(&s, 10).unwrap().texture, Some(ROBE[0][0]));
        assert_eq!(change(&s, 13).unwrap().texture, Some(0xCAFE_BABE));
        // a robe past the patch's six is robe 0
        let s = apply_look_with(&cs, &Look { robe: 6, ..look }, false, all, &ss);
        assert_eq!(change(&s, 10).unwrap().texture, Some(ROBE[0][0]));
        // armour tint 20: retail's clamp makes it 0, the patch keeps it
        let look = Look { armor: [1; 6], armor_tint: [20; 6], ..Default::default() };
        assert_eq!(change(&apply_look_with(&cs, &look, false, all, &retail), 11).unwrap().color, Some(TINTS[0]));
        assert_eq!(change(&apply_look_with(&cs, &look, false, all, &ss), 11).unwrap().color, Some(ss.tints[20]));
        // set 9: retail's clamp makes it none (the tint on the bare layer);
        // the patch keeps it, and with no art of its own here leaves the slot
        let look = Look { armor: [9; 6], ..Default::default() };
        assert_eq!(change(&apply_look_with(&cs, &look, false, all, &retail), 11), Some(LayerChange { entry: 11, layer: 0, texture: None, color: Some(TINTS[0]) }));
        assert_eq!(change(&apply_look_with(&cs, &look, false, all, &ss), 11), None);
        // set 9 with the patch's art: its slots its own
        let art = Tables { armor_extra: vec![(9, [0xC214_1C0A, 0x03C1_BE9E, 0x85EA_2A1D, 0x432C_4890, 0xB179_23D2])], ..sandstorm() };
        assert_eq!(change(&apply_look_with(&cs, &look, false, all, &art), 11).unwrap().texture, Some(0x03C1_BE9E));
        // the helm: retail has none for set 9, the patch set 4's
        assert_eq!(helm_with(9, &retail), None);
        assert_eq!(helm_with(9, &ss), Some(HELM[4]));
        assert_eq!(helm(4), helm_with(4, &ss));
        // retail's tables are `apply_look`'s
        let look = Look { armor: [3, 2, 1, 0, 5, 1], armor_tint: [1, 2, 3, 15, 16, 4], robe: -1, ..Default::default() };
        assert_eq!(apply_look(&cs, &look, false, all), apply_look_with(&cs, &look, false, all, &retail));
        assert_eq!(retail.tint_at_most(20), TINTS[14]);
        assert_eq!(ss.tint_at_most(20), ss.tints[20]);
    }

    /// The live game's colours are the layers' colours, in place of the
    /// tints' (a robe's too, the helm's by `helm_color`), whatever a patch's
    /// tables say; textures are the same. A colour equal to a tint's makes
    /// the very changes that tint makes: an undyed slot the server sends in
    /// its tint's colour is drawn as before. The control: with no colours,
    /// the tints'.
    #[test]
    fn live_colours_are_the_layers() {
        let cs = sprite();
        let all = |_: u32| true;
        let (retail, ss) = (Tables::retail(), sandstorm());
        let tinted = Look { armor: [3, 2, 1, 2, 5, 1], armor_tint: [1, 2, 3, 0, 8, 4], robe: 1, robe_tint: 6, ..Default::default() };
        let mut live = Look { armor_tint: [0; 6], robe_tint: 0, ..tinted };
        live.armor_color = Some([[0xA5, 5, 1, 255], [1, 2, 3, 255], [4, 5, 6, 255], [7, 8, 9, 255], [10, 11, 12, 255], [13, 14, 15, 255]]);
        live.robe_color = Some([0x15, 0, 0, 255]);
        for t in [&retail, &ss] {
            let a = apply_look_with(&cs, &tinted, false, all, t);
            let b = apply_look_with(&cs, &live, false, all, t);
            assert_eq!(a.len(), b.len());
            for (x, y) in a.iter().zip(&b) {
                assert_eq!((x.entry, x.layer, x.texture), (y.entry, y.layer, y.texture), "textures as the tints'");
            }
            assert_eq!(change(&b, 10).unwrap().color, Some([0x15, 0, 0, 255]), "the robe's slot");
            assert_eq!(change(&b, 11).unwrap().color, Some([1, 2, 3, 255]), "slot 1's");
            assert_eq!(change(&b, 14).unwrap().color, Some([10, 11, 12, 255]), "slot 4's");
            assert_eq!(live.helm_color(t), [13, 14, 15, 255]);
        }
        assert_eq!(change(&apply_look(&cs, &tinted, false, all), 11).unwrap().color, Some(TINTS[2]), "the control: the tint's");
        assert_eq!(tinted.helm_color(&retail), TINTS[4]);
        // the tints' own colours sent as colours: the same changes
        let as_colors = Look {
            armor_tint: [0; 6],
            robe_tint: 0,
            armor_color: Some(tinted.armor_tint.map(|i| retail.tint(i))),
            robe_color: Some(retail.tint_at_most(tinted.robe_tint)),
            ..tinted
        };
        assert_eq!(apply_look(&cs, &as_colors, false, all), apply_look(&cs, &tinted, false, all));
        assert_eq!(as_colors.helm_color(&retail), tinted.helm_color(&retail));
    }
}
