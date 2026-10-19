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
    let set = if set >= 9 { 0 } else { set as usize };
    if set == 0 { None } else { Some(HELM[set]) }
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
#[allow(clippy::needless_range_loop)]
pub fn apply_look<M: Model>(cs: &CSprite<M>, look: &Look, helm_worn: bool, has: impl Fn(u32) -> bool) -> Vec<LayerChange> {
    let mut out = Vec::new();
    let slot = |i: usize| cs.text_slots[i];
    let found = |id: u32| if id != 0 && has(id) { Some(id) } else { None };
    // Which slots the robe took over (`m_slots[i].m_flags & 1`).
    let mut robed = [false; 5];
    // SetRobe
    if look.robe >= 0 {
        let robe = if look.robe >= 4 { 0 } else { look.robe as usize };
        for i in 0..5 {
            let (entry, layer) = slot(i);
            if entry == -1 {
                continue;
            }
            let tex = match i {
                0 => found(ROBE[robe][0]),
                3 => found(ROBE[robe][1]),
                _ => None,
            };
            if let Some(t) = tex {
                robed[i] = true;
                out.push(LayerChange { entry, layer, texture: Some(t), color: Some(TINTS[(look.robe_tint as usize).min(14)]) });
            }
        }
    }
    // SetArmorSlot for chest..gloves (the helm is an attachment).
    for i in 0..5 {
        let (entry, layer) = slot(i);
        if entry == -1 {
            continue;
        }
        let set = if look.armor[i] >= 9 { 0 } else { look.armor[i] as usize };
        let tint = if look.armor_tint[i] >= 15 { 0 } else { look.armor_tint[i] as usize };
        let tex = if set == 0 { None } else { found(ARMOR[set][i]) };
        if (set != 0 && tex.is_none()) || robed[i] {
            continue;
        }
        out.push(LayerChange { entry, layer, texture: tex, color: Some(TINTS[tint]) });
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
