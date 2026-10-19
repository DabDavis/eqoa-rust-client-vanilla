//! Surfaces (record 0x1000): a header, an optional palette, then mip rows.
//!
//! Palette entries are little-endian 0xAABBGGRR, i.e. bytes R,G,B,A; 4bpp packs
//! the high nibble first. This decodes the base mip only, as the reference
//! parser does. Bytes come out as stored, so they stay comparable with the
//! fixtures; how the GS reads texture alpha is the renderer's business (see
//! `claude-notes/go-client-vs-decomp-audit-20261015.md`, texture alpha).

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
        let entries = self.pal_entries as usize;
        if self.pal_off + entries * 4 > d.len() {
            return None;
        }
        let pal = &d[self.pal_off..self.pal_off + entries * 4];
        let (w, h, rb) = (self.width as usize, self.height as usize, self.row_bytes as usize);
        let mut rgba = vec![0u8; w * h * 4];
        let mut alpha = false;
        // Only whole rows inside the buffer are read, as the reference does.
        let rows = h.min(d.len().saturating_sub(self.pix_off) / rb);
        for y in 0..rows {
            let row = &d[self.pix_off + y * rb..self.pix_off + (y + 1) * rb];
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
