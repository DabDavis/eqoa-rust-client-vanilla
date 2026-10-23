//! CLIENT's data, read out of its image (`modules`: the player's decrypted
//! one, or the disc's own unpacked by crate `dnas`): the menu definitions
//! the decomp leaves `extern` (eqoa-port's gendata reads the same roots,
//! `data/client.txt`) and `gWorldInfo`. Addresses are the decomp's (CLIENT
//! at 0x00BB4980); a pointer word is resolved through the image's SNR1
//! relocation table (`R_WORD`, no import), as gendata does. Nothing read
//! here is committed.

use std::collections::HashMap;

/// Where the decomp's CLIENT addresses put the image.
pub const BASE: u32 = 0x00BB4980;
const R_WORD: u32 = 2;

pub struct ClientImage {
    data: Vec<u8>,
    /// pointer site -> target, both image offsets
    words: HashMap<u32, u32>,
    /// CLIENT's memory as a server's patch leaves it (`s2`): what the patch
    /// wrote is read first, its arena too (the Group menu's Search, Register
    /// and Unregister are a table of its)
    patch: Option<std::sync::Arc<crate::s2::Image>>,
}

impl ClientImage {
    /// An un-relocated SNR1 image, as `VILauncher::UnpackAndVerify` leaves it.
    pub fn from_bytes(data: Vec<u8>) -> Option<Self> {
        if data.get(0..4)? != b"SNR1" {
            return None;
        }
        let rd = |o: usize| data.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
        let (table, count) = (rd(4)? as usize, rd(8)? as usize);
        let mut words = HashMap::new();
        for i in 0..count {
            let o = table + 12 * i;
            let (site, info, target) = (rd(o)?, rd(o + 4)?, rd(o + 8)?);
            if info & 0xFF == R_WORD && info >> 8 == 0 {
                words.insert(site, target);
            }
        }
        Some(ClientImage { data, words, patch: None })
    }

    /// The image as a server's patch leaves CLIENT's memory (`s2::Rules`'s
    /// `image`; `None` is the disc's).
    pub fn patched(mut self, patch: Option<std::sync::Arc<crate::s2::Image>>) -> Self {
        self.patch = patch;
        self
    }

    /// Whether this is the Frontiers CLIENT these addresses are read from
    /// (another build keeps its data elsewhere): `gStartItems` begins with
    /// the "TEST" entry (word 143) and counts its entries sanely.
    pub fn is_frontiers(&self) -> bool {
        self.i32_at(0x00CCFFF0 + 12) == 143 && (2..=64).contains(&self.i32_at(0x00CD0130))
    }

    pub fn i32_at(&self, va: u32) -> i32 {
        if let Some(w) = self.patch.as_ref().and_then(|p| p.word(va)) {
            return w as i32;
        }
        let o = va.wrapping_sub(BASE) as usize;
        self.data.get(o..o + 4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()))
    }

    /// The pointer stored at `va`, as an address (`None` for a null or a
    /// word nothing relocates).
    pub fn ptr_at(&self, va: u32) -> Option<u32> {
        // a pointer the patch wrote is an address as the console has it
        // (the arena's at the address it was linked at)
        if let Some(w) = self.patch.as_ref().and_then(|p| p.word(va)) {
            return (w != 0).then_some(w);
        }
        self.words.get(&va.wrapping_sub(BASE)).map(|t| t + BASE)
    }
}

/// `VIMenuEntry`, its sub-menu as an index into `Menus`.
#[derive(Clone, Copy, Debug)]
pub struct MenuEntry {
    pub icon: i32,
    pub icon2: i32,
    pub x08: i32,
    pub word: i32,
    pub number: i32,
    pub sub: Option<usize>,
    pub msg: u32,
    pub id: i32,
}

impl Default for MenuEntry {
    /// An emptied slot, as the Build* functions clear them.
    fn default() -> Self {
        MenuEntry { icon: -1, icon2: -1, x08: -1, word: -1, number: -1, sub: None, msg: 0, id: -1 }
    }
}

/// `VIMenuDef`.
#[allow(dead_code)]
#[derive(Clone, Debug, Default)]
pub struct Menu {
    pub x00: i32,
    pub x04: i32,
    pub entries: Vec<MenuEntry>,
}

/// Every menu reached from the roots, by index, each loaded once.
#[derive(Default)]
pub struct Menus {
    pub list: Vec<Menu>,
    by_va: HashMap<u32, usize>,
}

impl Menus {
    /// A `VIMenuDef` at `va`, its entries and their sub-menus.
    pub fn def(&mut self, img: &ClientImage, va: u32) -> usize {
        if let Some(&k) = self.by_va.get(&va) {
            return k;
        }
        let k = self.list.len();
        self.by_va.insert(va, k);
        self.list.push(Menu { x00: img.i32_at(va), x04: img.i32_at(va + 4), entries: Vec::new() });
        let count = img.i32_at(va + 12).clamp(0, 256);
        let entries = img.ptr_at(va + 8).map(|e| self.entries(img, e, count)).unwrap_or_default();
        self.list[k].entries = entries;
        k
    }

    /// A menu built at run time (the item menus `UpdateQuickBars` fills).
    pub fn push(&mut self, m: Menu) -> usize {
        self.list.push(m);
        self.list.len() - 1
    }

    /// A menu made of `count` entries at `va` (the start menu's tables,
    /// whose definition is set up at run time).
    pub fn table(&mut self, img: &ClientImage, va: u32, count: i32) -> usize {
        let entries = self.entries(img, va, count.clamp(0, 256));
        self.list.push(Menu { x00: -1, x04: -1, entries });
        self.list.len() - 1
    }

    fn entries(&mut self, img: &ClientImage, va: u32, count: i32) -> Vec<MenuEntry> {
        (0..count as u32)
            .map(|i| {
                let e = va + 32 * i;
                let w = |k: u32| img.i32_at(e + 4 * k);
                let sub = img.ptr_at(e + 20).map(|s| self.def(img, s));
                MenuEntry { icon: w(0), icon2: w(1), x08: w(2), word: w(3), number: w(4), sub, msg: w(6) as u32, id: w(7) }
            })
            .collect()
    }
}

/// `gWorldInfo[i]`'s grid bounds (`m_p0C` .. `m_p18`: columns, then rows).
pub fn world_bounds(img: &ClientImage, world: i32) -> Option<[i32; 4]> {
    if !(0..6).contains(&world) {
        return None;
    }
    let va = 0x00CF3738 + 0x20 * world as u32;
    Some([img.i32_at(va + 0x0C), img.i32_at(va + 0x10), img.i32_at(va + 0x14), img.i32_at(va + 0x18)])
}

/// `gWorldInfo[i].m_nameWord` (+0x08, a halfword): the world's name, a
/// WORDS.TXT word (`VIWndGoto` draws it).
pub fn world_name_word(img: &ClientImage, world: i32) -> Option<u16> {
    if !(0..6).contains(&world) {
        return None;
    }
    let w = (img.i32_at(0x00CF3738 + 0x20 * world as u32 + 8) & 0xFFFF) as u16;
    (w != 0).then_some(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sandstorm's S2 gives the Group menu (`VIMenuDef` 0x00CCED20) three
    /// more entries out of its arena: Search, Register and Unregister, whose
    /// messages its own code takes (0xFF000001-3); the disc's image (the
    /// control) has the eight retail ones. Skipped without CLIENT's image
    /// (`run/client.bin`) or the S2 (`s2`'s tests).
    #[test]
    fn a_patch_gives_the_group_menu_its_entries() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let Ok(img) = std::fs::read(root.join("run/client.bin")) else {
            eprintln!("SKIP: no run/client.bin");
            return;
        };
        let s2 = std::env::var_os("EQOA_S2_BUILDS").map(std::path::PathBuf::from).unwrap_or_else(|| root.join("..")).join("extracted-assets/sandstorm-patch-20261015/S2");
        let Ok(s2) = std::fs::read(&s2) else {
            eprintln!("SKIP: no S2 at {}", s2.display());
            return;
        };
        let rules = crate::s2::Rules::from_patch(&crate::s2::Patch::parse(&s2).unwrap());
        let group = |c: &ClientImage| {
            let mut m = Menus::default();
            let k = m.def(c, 0x00CC_ED20);
            m.list[k].entries.iter().map(|e| (e.word, e.msg)).collect::<Vec<_>>()
        };
        let disc = ClientImage::from_bytes(img.clone()).unwrap();
        let retail = group(&disc);
        assert_eq!(retail.len(), 8);
        assert_eq!(retail[0], (86, 0x0701_0000), "Invite");
        let patched = ClientImage::from_bytes(img).unwrap().patched(rules.image.clone());
        let s = group(&patched);
        assert_eq!(&s[..8], &retail[..]);
        assert_eq!(&s[8..], &[(1673, 0xFF00_0001), (1815, 0xFF00_0002), (1816, 0xFF00_0003)]);
        // and the start menu reaches them (Community's Group)
        let start = crate::startmenu::StartData::from_image(&patched);
        assert!(start.menus.list.iter().any(|m| m.entries.iter().any(|e| e.msg == 0xFF00_0001)), "the start menu reaches the Group menu");
        // the rest of the image is the disc's
        assert_eq!(patched.i32_at(0x00CC_FFF0 + 12), disc.i32_at(0x00CC_FFF0 + 12));
    }
}
