//! Surfaces (record 0x1000): a header, an optional palette, then mip rows.
//!
//! Palette entries are little-endian 0xAABBGGRR, i.e. bytes R,G,B,A; 4bpp packs
//! the high nibble first. `decode` is the base mip, as the reference parser
//! decodes it; `decode_levels` is every level the game keeps, which the GS
//! samples (`VIEESurfCache::Set`'s TEX1: trilinear, the level by depth; the
//! renderer's `gs_lod_k`). Bytes come out as stored, so they stay comparable
//! with the fixtures; how the GS reads texture alpha is the renderer's
//! business (see `claude-notes/go-client-vs-decomp-audit-20261015.md`,
//! texture alpha).

use crate::i32le;

#[derive(Clone, Debug)]
pub struct Surface {
    pub dict_id: i32,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub mips: i32,
    pub pal_off: usize,
    pub pal_entries: i32,
    pub row_bytes: i32,
    pub pix_off: usize,
    pub bpp: u8,
}

/// A decoded image: top-down RGBA8. `alpha` is true when a pixel actually
/// written has alpha != 255 (an unused translucent palette entry does not count).
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub alpha: bool,
}

impl Surface {
    pub(crate) fn read(d: &[u8], body: usize) -> Surface {
        let r = |at: usize| if at + 4 <= d.len() { i32le(d, at) } else { 0 };
        let (dict_id, width, height, depth, mips) =
            (r(body), r(body + 4), r(body + 8), r(body + 12), r(body + 16));
        let mut p = body + 20;
        let (mut pal_off, mut pal_entries) = (0, 0);
        if depth < 2 {
            pal_entries = r(p);
            p += 4;
            pal_off = p;
            p = p.wrapping_add((pal_entries as isize * 4) as usize);
        }
        let (mut row_bytes, mut pix_off) = (0, 0);
        if mips > 0 {
            row_bytes = r(p);
            pix_off = p + 4;
        }
        // 4bpp when the palette is small AND a row is narrower than the width.
        let bpp = if pal_entries > 0 && pal_entries <= 16 && row_bytes < width { 4 } else { 8 };
        Surface { dict_id, width, height, depth, mips, pal_off, pal_entries, row_bytes, pix_off, bpp }
    }

    /// Decodes the base mip, or `None` for a surface with no palette or no mip
    /// (the formats the reference parser does not decode either).
    pub fn decode(&self, d: &[u8]) -> Option<Image> {
        if self.mips <= 0 || self.pal_entries <= 0 || self.row_bytes <= 0 || self.width <= 0 || self.height <= 0 {
            return None;
        }
        self.decode_level(d, self.pix_off, self.width as usize, self.height as usize, self.row_bytes as usize)
    }

    /// How many levels the game keeps (`VISurface::Init`, which the surface
    /// cache uploads as they are): from the base down while both sides are
    /// at least 16, at most ten and at most the record's own count. A
    /// surface under 16 a side, which none on the disc is, keeps its base.
    pub fn levels(&self) -> usize {
        let (mut w, mut h, mut n) = (self.width, self.height, 0);
        while w >= 16 && h >= 16 && n < 10 && n < self.mips {
            n += 1;
            w = (w >> 1).max(1);
            h = (h >> 1).max(1);
        }
        n.max(1) as usize
    }

    /// Every level the game keeps (`levels`), the base first, each decoded
    /// as the base is, against the one palette: a level is its row length
    /// (a u32) and its rows, the sides halving from the base's
    /// (`ParseSurfaceObj`). A level whose rows the record does not hold
    /// ends the list; none when the base does not decode.
    pub fn decode_levels(&self, d: &[u8]) -> Vec<Image> {
        let Some(base) = self.decode(d) else { return Vec::new() };
        let (mut w, mut h) = (self.width as usize, self.height as usize);
        // the next level's row length, after the base's rows
        let mut at = self.pix_off + h * self.row_bytes as usize;
        let mut out = vec![base];
        for _ in 1..self.levels() {
            w = (w >> 1).max(1);
            h = (h >> 1).max(1);
            let rb = if at + 4 <= d.len() { i32le(d, at) } else { 0 };
            let pix = at + 4;
            if rb <= 0 || pix + h * rb as usize > d.len() {
                break;
            }
            match self.decode_level(d, pix, w, h, rb as usize) {
                Some(level) => out.push(level),
                None => break,
            }
            at = pix + h * rb as usize;
        }
        out
    }

    /// One level: `h` rows of `rb` bytes from `pix`, `w` texels each.
    fn decode_level(&self, d: &[u8], pix: usize, w: usize, h: usize, rb: usize) -> Option<Image> {
        let entries = self.pal_entries as usize;
        if self.pal_off + entries * 4 > d.len() {
            return None;
        }
        let pal = &d[self.pal_off..self.pal_off + entries * 4];
        let mut rgba = vec![0u8; w * h * 4];
        let mut alpha = false;
        // Only whole rows inside the buffer are read, as the reference does.
        let rows = h.min(d.len().saturating_sub(pix) / rb);
        for y in 0..rows {
            let row = &d[pix + y * rb..pix + (y + 1) * rb];
            let lim = if self.bpp == 4 { w.min(w.div_ceil(2).min(rb) * 2) } else { w.min(rb) };
            for x in 0..lim {
                let idx = if self.bpp == 4 {
                    let b = row[x / 2];
                    if x % 2 == 0 { b >> 4 } else { b & 0x0F }
                } else {
                    row[x]
                } as usize;
                if idx >= entries {
                    continue;
                }
                let px = &pal[idx * 4..idx * 4 + 4];
                alpha |= px[3] != 255;
                rgba[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(px);
            }
        }
        Some(Image { width: w as u32, height: h as u32, rgba, alpha })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A surface record's body: 32 x 32 at 8 bits, three mips in the file
    /// (32, 16, 8), each level its row length and rows; the palette's entry i
    /// is (i, 255 - i, i / 2, 255). Level 0's texels are (x + y) & 255,
    /// level 1's all 200, level 2's all 7.
    fn record(cut: usize) -> Vec<u8> {
        let mut d = Vec::new();
        for v in [0x1234, 32, 32, 1, 3] {
            d.extend_from_slice(&(v as i32).to_le_bytes());
        }
        d.extend_from_slice(&256i32.to_le_bytes());
        for i in 0..256u32 {
            d.extend_from_slice(&[i as u8, 255 - i as u8, (i / 2) as u8, 255]);
        }
        for (side, fill) in [(32usize, None), (16, Some(200u8)), (8, Some(7))] {
            d.extend_from_slice(&(side as i32).to_le_bytes());
            for y in 0..side {
                for x in 0..side {
                    d.push(fill.unwrap_or(((x + y) & 255) as u8));
                }
            }
        }
        d.truncate(d.len() - cut);
        d
    }

    /// The levels the game keeps, down to 16 a side (`VISurface::Init`):
    /// the base as `decode` gives it, then level 1 of its own rows. The
    /// controls: the file's 8 x 8 third level is not kept, and a record cut
    /// short in level 1's rows keeps the base alone.
    #[test]
    fn every_level_the_game_keeps_is_decoded() {
        let d = record(0);
        let s = Surface::read(&d, 0);
        assert_eq!((s.mips, s.levels()), (3, 2), "the 8 x 8 level is under 16");
        let levels = s.decode_levels(&d);
        assert_eq!(levels.len(), 2);
        let base = s.decode(&d).unwrap();
        assert_eq!((levels[0].width, levels[0].height, &levels[0].rgba), (32, 32, &base.rgba), "the base as decode gives it");
        assert_eq!((levels[1].width, levels[1].height), (16, 16));
        assert!(levels[1].rgba.chunks(4).all(|p| p == [200, 55, 100, 255]), "level 1 from its own rows");
        // level 2 (8 x 8, all 7) is in the file and not kept; cut 9 bytes
        // off the end and level 1 is still whole
        assert_eq!(s.decode_levels(&record(9)).len(), 2);
        // cut into level 1's rows: the base alone
        let short = record(4 + 64 + 8);
        assert_eq!(s.decode_levels(&short).len(), 1, "a level the record does not hold ends the list");
    }
}
