//! Fonts (record 0x7000), as `VIESFParse::ParseFontObj` reads them (decomp
//! `src/viesfparse.cpp`): a header (0x7010: DictID, glyph count, height, bit
//! depth, and from version 1 an extra word), the widths (0x7020: one byte a
//! glyph), then the pixels (0x7030): for depths 0 and 1 a palette (count,
//! then RGBA bytes), and per glyph its code (u16), its row size in bytes
//! (u32) and `height` rows. Depth 0 is 4-bit (low nibble first), 1 is 8-bit,
//! 4 is 32-bit RGBA; 2 and 3 (16- and 24-bit) are not used by the game's
//! fonts and are not decoded here.

use crate::{Container, NodeId};

#[derive(Clone, Debug)]
pub struct Glyph {
    pub code: u16,
    pub width: u8,
    /// `width` x the font's height, RGBA8, top row first.
    pub rgba: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Font {
    pub id: i32,
    pub height: u32,
    pub glyphs: Vec<Glyph>,
}

impl Font {
    /// The glyph for a character code.
    pub fn glyph(&self, code: u16) -> Option<&Glyph> {
        self.glyphs.iter().find(|g| g.code == code)
    }

    /// `GetStringWidth`: each glyph's width plus one pixel of spacing.
    pub fn width(&self, s: &str) -> u32 {
        s.chars().filter_map(|c| self.glyph(c as u16)).map(|g| g.width as u32 + 1).sum()
    }
}

fn u32le(d: &[u8], p: usize) -> u32 {
    d.get(p..p + 4).map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
}

/// Reads the font record `id` (type 0x7000).
pub fn read(c: &Container, id: NodeId) -> Option<Font> {
    let hdr = c.child(id, 0x7010)?;
    let h = c.body(hdr);
    let (dict, n, height, depth) = (u32le(h, 0) as i32, u32le(h, 4) as usize, u32le(h, 8) as usize, u32le(h, 12));
    let widths = c.body(c.child(id, 0x7020)?);
    let d = c.body(c.child(id, 0x7030)?);
    let mut p = 0;
    let mut pal = Vec::new();
    if depth < 2 {
        let count = u32le(d, p) as usize;
        p += 4;
        pal = d.get(p..p + count * 4)?.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect::<Vec<_>>();
        p += count * 4;
    }
    let mut glyphs = Vec::with_capacity(n);
    for k in 0..n {
        let code = u16::from_le_bytes(d.get(p..p + 2)?.try_into().ok()?);
        let row = u32le(d, p + 2) as usize;
        p += 6;
        let width = *widths.get(k)? as usize;
        let mut rgba = vec![0u8; width * height * 4];
        for y in 0..height {
            let r = d.get(p..p + row)?;
            p += row;
            for x in 0..width {
                let px = match depth {
                    0 => {
                        let b = *r.get(x / 2).unwrap_or(&0);
                        pal.get(if x % 2 == 0 { b & 0x0f } else { b >> 4 } as usize).copied().unwrap_or([0; 4])
                    }
                    1 => pal.get(*r.get(x).unwrap_or(&0) as usize).copied().unwrap_or([0; 4]),
                    4 => r.get(x * 4..x * 4 + 4).map_or([0; 4], |c| [c[0], c[1], c[2], c[3]]),
                    _ => [0; 4],
                };
                rgba[(y * width + x) * 4..][..4].copy_from_slice(&px);
            }
        }
        glyphs.push(Glyph { code, width: width as u8, rgba });
    }
    Some(Font { id: dict, height: height as u32, glyphs })
}
