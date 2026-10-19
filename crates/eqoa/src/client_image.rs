//! CLIENT's data, read out of the player's decrypted CLIENT image
//! (`run/client.bin`, untracked, as `run/support.bin` is SUPPORT's): the menu
//! definitions the decomp leaves `extern` (eqoa-port's gendata reads the same
//! roots, `data/client.txt`) and `gWorldInfo`. Addresses are the decomp's
//! (CLIENT at 0x00BB4980); a pointer word is resolved through the image's
//! SNR1 relocation table (`R_WORD`, no import), as gendata does. Nothing read
//! here is committed.

use std::collections::HashMap;

/// Where the decomp's CLIENT addresses put the image.
pub const BASE: u32 = 0x00BB4980;
const R_WORD: u32 = 2;

pub struct ClientImage {
    data: Vec<u8>,
    /// pointer site -> target, both image offsets
    words: HashMap<u32, u32>,
}

impl ClientImage {
    pub fn open(path: &std::path::Path) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
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
        Some(ClientImage { data, words })
    }

    pub fn i32_at(&self, va: u32) -> i32 {
        let o = va.wrapping_sub(BASE) as usize;
        self.data.get(o..o + 4).map_or(0, |b| i32::from_le_bytes(b.try_into().unwrap()))
    }

    /// The pointer stored at `va`, as an address (`None` for a null or a
    /// word nothing relocates).
    pub fn ptr_at(&self, va: u32) -> Option<u32> {
        self.words.get(&va.wrapping_sub(BASE)).map(|t| t + BASE)
    }
}

/// `VIMenuEntry`, its sub-menu as an index into `Menus`. (The icons and
/// number are the item menus', not ported yet.)
#[allow(dead_code)]
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
