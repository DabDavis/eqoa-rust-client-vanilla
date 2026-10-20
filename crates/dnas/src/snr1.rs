//! SN Systems' relocatable modules (`SNR1`), the format of `UTIL.REL` and of
//! every module inside a `.AUT`. The header: `+0x04`/`+0x08` the relocation
//! table and its count, `+0x0C`/`+0x10` the symbol table and its count,
//! `+0x18`/`+0x1C` the init and fini functions as image offsets, `+0x20` the
//! image's length. A relocation is `{site, (symbol << 8) | kind, addend}`: the
//! value is `symbol's address + addend`, symbol 0 being the module's own base;
//! kind 2 a whole word, 4 a `jal`/`j` target, 5 a `%hi` (with the carry a
//! sign-extended `%lo` needs), 6 a `%lo`. A symbol is `{name, address, hash}`:
//! address 0 marks an import, bound by name. Checked by relocating `UTIL.REL`
//! at the launcher's base and comparing it with the module resident in a
//! running game: every module-relative site equal.

#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    /// Image offset of an export; 0 for an import.
    pub offset: u32,
}

pub struct Module<'a> {
    pub image: &'a [u8],
    pub symbols: Vec<Symbol>,
    relocs: (usize, usize),
}

fn rd(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|w| u32::from_le_bytes(w.try_into().unwrap()))
}

impl<'a> Module<'a> {
    pub fn parse(image: &'a [u8]) -> Option<Self> {
        if image.get(0..4)? != b"SNR1" {
            return None;
        }
        let (rtab, rcount) = (rd(image, 4)? as usize, rd(image, 8)? as usize);
        let (stab, scount) = (rd(image, 12)? as usize, rd(image, 16)? as usize);
        if rtab.checked_add(rcount.checked_mul(12)?)? > image.len() || stab.checked_add(scount.checked_mul(12)?)? > image.len() {
            return None;
        }
        let mut symbols = Vec::with_capacity(scount);
        for i in 0..scount {
            let at = stab + 12 * i;
            let (name, offset) = (rd(image, at)? as usize, rd(image, at + 4)?);
            let end = name + image.get(name..)?.iter().position(|&c| c == 0)?;
            symbols.push(Symbol { name: String::from_utf8_lossy(&image[name..end]).into_owned(), offset });
        }
        Some(Module { image, symbols, relocs: (rtab, rcount) })
    }

    pub fn len(&self) -> u32 {
        rd(self.image, 0x20).unwrap_or(self.image.len() as u32)
    }

    pub fn init(&self) -> u32 {
        rd(self.image, 0x18).unwrap_or(0)
    }

    /// The image relocated to `base`, imports bound by `bind(index, name)`.
    pub fn relocate(&self, base: u32, mut bind: impl FnMut(usize, &str) -> u32) -> Vec<u8> {
        let mut img = self.image.to_vec();
        let resolved: Vec<u32> = self
            .symbols
            .iter()
            .enumerate()
            .map(|(i, s)| if i == 0 { base } else if s.offset != 0 { base + s.offset } else { bind(i, &s.name) })
            .collect();
        let (tab, count) = self.relocs;
        for i in 0..count {
            let at = tab + 12 * i;
            let (site, info, addend) = (rd(self.image, at).unwrap() as usize, rd(self.image, at + 4).unwrap(), rd(self.image, at + 8).unwrap());
            let Some(w) = rd(&img, site) else { continue };
            let v = resolved.get((info >> 8) as usize).copied().unwrap_or(0).wrapping_add(addend);
            let w = match info & 0xFF {
                2 => v,
                4 => (w & 0xFC00_0000) | ((v >> 2) & 0x03FF_FFFF),
                5 => (w & 0xFFFF_0000) | (v.wrapping_add(0x8000) >> 16),
                6 => (w & 0xFFFF_0000) | (v & 0xFFFF),
                _ => w,
            };
            img[site..site + 4].copy_from_slice(&w.to_le_bytes());
        }
        img
    }
}
