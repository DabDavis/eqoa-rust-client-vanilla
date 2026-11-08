//! PCSX2's names for the game's textures, so that one texture pack works in
//! PCSX2 and here (the user, 2026-11-08: "Vanilla loads PCSX2 replacement
//! packs as they are (same folder layout and file names, PNG and DDS, HD
//! sizes) and can dump its textures under PCSX2's names, so one pack works in
//! both").
//!
//! PCSX2 names a texture by what its GS texture cache hashes when the game
//! samples it (`pcsx2/GS/Renderers/HW/GSTextureCache.cpp`:
//! `HashCacheKey::Create`, `HashTextureLevel`, `PaletteKeyHash`;
//! `GSTextureReplacements.cpp`: `TextureName` and the file names), so a name
//! is made here from what the game puts in the GS's memory for a surface.
//! The decomp settles that: `VIESFParse::ParseSurfaceObj` copies a surface
//! record's rows and its palette into a `VISurface` as they are (`Convert8`;
//! `Convert4`'s memcpy, so a 4-bit row's even texel is a byte's LOW nibble,
//! as the GS reads PSMT4; `Convert32`), and `VIEESurfCache` uploads every
//! level it keeps (`CalcCacheInfo`: from the base while both sides are at
//! least 16) in the surface's own storage mode (BITBLTBUF's DPSM is TEX0's
//! PSM: PSMT8 for an 8-bit surface, PSMT4 for a 4-bit one), the palette as a
//! 256- or 16-entry PSMCT32 CLUT (CSM1, CSA 0) already in the order CSM1
//! reads it (`VISurface::ReorderPalette`), and samples it with TEX0's TW and
//! TH the log2 of its sides and TEX1's MXL its levels less one, MMIN
//! trilinear (`VIEESurfCache::Set`). Every surface on the Frontiers disc is
//! one of those two (42,150 8-bit with 256 colours, 2 4-bit with 16; all of
//! them a power of two and at least 16 a side), so every name is exact.
//!
//! What PCSX2 hashes (the file:line references are its tree at d4cf5a998,
//! the build the tracer runs):
//! - TEX0Hash: XXH3-64 with seed 0 over each level's bytes in turn
//!   (`GSTextureCache.cpp:8941`): the base alone, or, with PCSX2's
//!   Mipmapping on (its default, `hw_mipmap`) and a draw that reaches past
//!   the base (`GSRendererHW.cpp:2952`, `GSState::IsMipMapDraw`), the level
//!   the draw starts at and every one after it up to MXL (at most 6),
//!   named by that first level's TEX0 (`GSRendererHW.cpp:3009`,
//!   `GSState::GetTex0Layer`). A level at least a block in size (PSMT8 16 x
//!   16, PSMT4 32 x 16) is hashed as the 256-byte blocks of GS memory it
//!   fills, a row of blocks at a time, each byte as the GS swizzles its
//!   texels (`GSTextureCache.cpp:8810`; `columnTable8` and `columnTable4`,
//!   `GSTables.cpp`); a smaller one as its texels expanded to a byte each
//!   (the index), row by row.
//! - CLUTHash: XXH3-64 of the palette as PCSX2 holds it (`GSClut::Read32`'s
//!   32-bit buffer): the 256 or 16 entries in index order, each R, G, B, A
//!   as stored (`GSTextureCache.cpp:8529`).
//! - bits: TEX0's PSM, TW << 6, TH << 10; TEXA's bits only for a 16- or
//!   24-bit texture, which no surface is (`GSTextureReplacements.cpp:44`).
//!
//! A file is `<TEX0Hash>-<CLUTHash>-<bits>.png`, the hashes in lower-case
//! hex without leading zeros, bits as eight digits; a dumped mip level's
//! ends `-mip<n>` (`GSTextureReplacements.cpp:33`, `GetDumpFilename`). A
//! texture PCSX2 sampled through a region clamp has `-r<w>x<h>` before the
//! bits; the game never sets one.
//!
//! Checked against PCSX2's own dumps of the game in the tracer
//! (`tests/pcsx2.rs`), and the hashing against planted errors (below).

use crate::surface::Surface;
use xxhash_rust::xxh3::Xxh3;

/// The Frontiers disc's serial, the folder PCSX2 keeps its textures in
/// (`EmuFolders::Textures/<serial>`, `VMManager::GetDiscSerial`).
pub const SERIAL: &str = "SLUS-20744";

/// TEX0 PSM: PSMT8, 8-bit indices.
pub const PSMT8: u32 = 19;
/// TEX0 PSM: PSMT4, 4-bit indices.
pub const PSMT4: u32 = 20;

/// The highest level PCSX2 takes (`GSRendererHW.cpp:2958`: `mxl` at most 6).
const MOST_LEVEL: usize = 6;

/// A name PCSX2 gives a texture: its `TextureName` (`GSTextureReplacements.cpp:44`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Name {
    pub tex0_hash: u64,
    /// 0 for a texture with no palette.
    pub clut_hash: u64,
    /// A region clamp's size (0, 0: none).
    pub region: (u32, u32),
    /// TEX0 PSM | TW << 6 | TH << 10 | TEXA's TA0 << 15, AEM << 23, TA1 << 24.
    pub bits: u32,
}

impl Name {
    pub fn psm(&self) -> u32 {
        self.bits & 63
    }

    /// The width the name stands for: its region's, else 2^TW.
    pub fn width(&self) -> u32 {
        if self.region.0 != 0 { self.region.0 } else { 1 << ((self.bits >> 6) & 15) }
    }

    pub fn height(&self) -> u32 {
        if self.region.1 != 0 { self.region.1 } else { 1 << ((self.bits >> 10) & 15) }
    }

    /// Whether the storage mode takes a palette (PSMT8, PSMT8H, PSMT4,
    /// PSMT4HL, PSMT4HH: `GSLocalMemory::m_psm[].pal`).
    pub fn has_palette(&self) -> bool {
        matches!(self.psm(), 19 | 20 | 27 | 36 | 44)
    }

    /// The file PCSX2 dumps level `mip` of the texture to (0: the texture
    /// itself), as `GetDumpFilename` formats it.
    pub fn file_name(&self, mip: u32) -> String {
        let mut s = format!("{:x}", self.tex0_hash);
        if self.has_palette() {
            s += &format!("-{:x}", self.clut_hash);
        }
        if self.region != (0, 0) {
            s += &format!("-r{}x{}", self.region.0, self.region.1);
        }
        s += &format!("-{:08x}", self.bits);
        if mip > 0 {
            s += &format!("-mip{mip}");
        }
        s + ".png"
    }

    /// The name a replacement's file name gives, as `ParseReplacementName`
    /// reads it (`GSTextureReplacements.cpp:192`, its six `sscanf` formats in
    /// its order, the old ones with a region's whole bits among them); `None`
    /// for one PCSX2 passes over (a dumped mip level's among them: none of
    /// the formats has room for its `-mip<n>`). The extension is not looked
    /// at here, only that a `.` ends the name.
    pub fn parse(file: &str) -> Option<Name> {
        let b = file.as_bytes();
        let done = |n: Name, at: usize| (b.get(at) == Some(&b'.')).then_some(Name { bits: n.bits & !(1 << 14), ..n });
        // TEX0-CLUT-rWxH-bits
        if let Some((t, c, w, h, bits, at)) = (|| {
            let (t, at) = hex(b, 0, 16)?;
            let at = lit(b, at, b"-")?;
            let (c, at) = hex(b, at, 16)?;
            let at = lit(b, at, b"-r")?;
            let (w, at) = dec(b, at)?;
            let at = lit(b, at, b"x")?;
            let (h, at) = dec(b, at)?;
            let at = lit(b, at, b"-")?;
            let (bits, at) = hex(b, at, 8)?;
            Some((t, c, w, h, bits, at))
        })() {
            return done(Name { tex0_hash: t, clut_hash: c, region: (w, h), bits: bits as u32 }, at);
        }
        // TEX0-rWxH-bits
        if let Some((t, w, h, bits, at)) = (|| {
            let (t, at) = hex(b, 0, 16)?;
            let at = lit(b, at, b"-r")?;
            let (w, at) = dec(b, at)?;
            let at = lit(b, at, b"x")?;
            let (h, at) = dec(b, at)?;
            let at = lit(b, at, b"-")?;
            let (bits, at) = hex(b, at, 8)?;
            Some((t, w, h, bits, at))
        })() {
            return done(Name { tex0_hash: t, clut_hash: 0, region: (w, h), bits: bits as u32 }, at);
        }
        // the old formats: TEX0-CLUT-r<region bits>-bits, TEX0-r<region bits>-bits,
        // the region's bits a `SourceRegion`'s (min x, max x, min y, max y,
        // sixteen signed bits each: `GSTextureCache.h:50`), taken for its size
        let old_region = |r: u64| (((r >> 16) as u16 as i16 as i32) - (r as u16 as i16 as i32)) as u32;
        let old_region_h = |r: u64| (((r >> 48) as u16 as i16 as i32) - ((r >> 32) as u16 as i16 as i32)) as u32;
        if let Some((t, c, r, bits, at)) = (|| {
            let (t, at) = hex(b, 0, 16)?;
            let at = lit(b, at, b"-")?;
            let (c, at) = hex(b, at, 16)?;
            let at = lit(b, at, b"-r")?;
            let (r, at) = hex(b, at, 16)?;
            let at = lit(b, at, b"-")?;
            let (bits, at) = hex(b, at, 8)?;
            Some((t, c, r, bits, at))
        })() {
            return done(Name { tex0_hash: t, clut_hash: c, region: (old_region(r), old_region_h(r)), bits: bits as u32 }, at);
        }
        if let Some((t, r, bits, at)) = (|| {
            let (t, at) = hex(b, 0, 16)?;
            let at = lit(b, at, b"-r")?;
            let (r, at) = hex(b, at, 16)?;
            let at = lit(b, at, b"-")?;
            let (bits, at) = hex(b, at, 8)?;
            Some((t, r, bits, at))
        })() {
            return done(Name { tex0_hash: t, clut_hash: 0, region: (old_region(r), old_region_h(r)), bits: bits as u32 }, at);
        }
        // TEX0-CLUT-bits
        if let Some((t, c, bits, at)) = (|| {
            let (t, at) = hex(b, 0, 16)?;
            let at = lit(b, at, b"-")?;
            let (c, at) = hex(b, at, 16)?;
            let at = lit(b, at, b"-")?;
            let (bits, at) = hex(b, at, 8)?;
            Some((t, c, bits, at))
        })() && b.get(at) == Some(&b'.')
        {
            return done(Name { tex0_hash: t, clut_hash: c, region: (0, 0), bits: bits as u32 }, at);
        }
        // TEX0-bits
        let (t, at) = hex(b, 0, 16)?;
        let at = lit(b, at, b"-")?;
        let (bits, at) = hex(b, at, 8)?;
        done(Name { tex0_hash: t, clut_hash: 0, region: (0, 0), bits: bits as u32 }, at)
    }
}

/// `sscanf`'s `%x` (`most` digits at most, as `%08x` caps it; a 64-bit
/// value's sixteen): at least one hex digit from `at`.
fn hex(b: &[u8], at: usize, most: usize) -> Option<(u64, usize)> {
    let mut v: u64 = 0;
    let mut n = 0;
    while n < most {
        let Some(d) = b.get(at + n).and_then(|&c| (c as char).to_digit(16)) else { break };
        v = (v << 4) | d as u64;
        n += 1;
    }
    (n > 0).then_some((v, at + n))
}

/// `sscanf`'s `%u`: at least one decimal digit from `at`.
fn dec(b: &[u8], at: usize) -> Option<(u32, usize)> {
    let mut v: u32 = 0;
    let mut n = 0;
    while let Some(d) = b.get(at + n).and_then(|&c| (c as char).to_digit(10)) {
        v = v.wrapping_mul(10).wrapping_add(d);
        n += 1;
    }
    (n > 0).then_some((v, at + n))
}

fn lit(b: &[u8], at: usize, s: &[u8]) -> Option<usize> {
    b.get(at..at + s.len()).filter(|t| *t == s).map(|_| at + s.len())
}

/// One level of a texture as the GS holds it: `width` x `height` texels in
/// rows, each the index the GS reads for it (a 4-bit one 0 to 15).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GsLevel {
    pub width: u32,
    pub height: u32,
    pub texels: Vec<u8>,
}

/// A surface as the game hands it to the GS (`Surface::gs`): its storage
/// mode, the levels it uploads, and its CLUT as PCSX2 holds it (`GSClut`'s
/// 32-bit buffer: each entry R | G << 8 | B << 16 | A << 24, in index order).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GsTexture {
    pub psm: u32,
    pub levels: Vec<GsLevel>,
    pub clut: Vec<u32>,
}

impl Surface {
    /// What the game puts in the GS's memory for this surface
    /// (`ParseSurfaceObj` into a `VISurface`, `VIEESurfCache` uploading it):
    /// every level it keeps (`levels`) and the palette, or `None` for a
    /// surface that is not 4- or 8-bit, one whose rows or palette the record
    /// does not hold whole, and one whose palette is short of the CLUT's 16
    /// or 256 entries (the rest of a `VISurface`'s CLUT is whatever its
    /// allocation held, which nothing can name).
    pub fn gs(&self, d: &[u8]) -> Option<GsTexture> {
        let (psm, clut) = self.gs_clut(d)?;
        let (mut w, mut h) = (self.width as usize, self.height as usize);
        // a level is its row length (a u32) and its rows; the base's length
        // is the record's (`row_bytes`), just before its rows
        let mut at = self.pix_off.checked_sub(4)?;
        let mut levels = Vec::new();
        for _ in 0..self.levels() {
            let rb = u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?) as usize;
            let rows = d.get(at + 4..at + 4 + h * rb)?;
            let need = if psm == PSMT4 { w.div_ceil(2) } else { w };
            if rb < need {
                return None;
            }
            let mut texels = vec![0u8; w * h];
            for y in 0..h {
                let row = &rows[y * rb..y * rb + rb];
                for x in 0..w {
                    texels[y * w + x] = if psm == PSMT4 { (row[x / 2] >> ((x & 1) * 4)) & 15 } else { row[x] };
                }
            }
            levels.push(GsLevel { width: w as u32, height: h as u32, texels });
            at += 4 + h * rb;
            w = (w >> 1).max(1);
            h = (h >> 1).max(1);
        }
        Some(GsTexture { psm, levels, clut })
    }
}

impl Surface {
    /// The storage mode and the CLUT the game uploads for this surface, or
    /// `None` where `gs` has none.
    fn gs_clut(&self, d: &[u8]) -> Option<(u32, Vec<u32>)> {
        let (psm, colours) = match self.depth {
            0 => (PSMT4, 16usize),
            1 => (PSMT8, 256usize),
            _ => return None,
        };
        if self.pal_entries < colours as i32 || self.mips <= 0 || self.width <= 0 || self.height <= 0 {
            return None;
        }
        let pal = d.get(self.pal_off..self.pal_off + colours * 4)?;
        Some((psm, pal.as_chunks::<4>().0.iter().map(|&c| u32::from_le_bytes(c)).collect()))
    }

    /// The CLUT hash every PCSX2 name of this surface carries
    /// (`GsTexture::clut_hash`), without the levels: what a pack's names
    /// are looked through first, cheaply.
    pub fn gs_clut_hash(&self, d: &[u8]) -> Option<u64> {
        self.gs_clut(d).map(|(_, c)| clut_hash(&c))
    }
}

impl GsTexture {
    /// TEX0's TW or TH for a side: `VIEESurfCache::Log2(side) - 1`, the bit
    /// count less one.
    fn log2(side: u32) -> u32 {
        31 - side.max(1).leading_zeros()
    }

    /// The highest level PCSX2 samples: MXL (the levels less one), at most 6.
    pub fn top_level(&self) -> usize {
        (self.levels.len().max(1) - 1).min(MOST_LEVEL)
    }

    /// PCSX2's name for the levels `from..=to` hashed together, named as
    /// `from`: the base alone is `name(0, 0)`; with its Mipmapping on, a draw
    /// that starts at level `s` is `name(s, top_level())`.
    pub fn name(&self, from: usize, to: usize) -> Name {
        let streams: Vec<Vec<u8>> = self.levels[..=to].iter().map(|l| level_bytes(self.psm, l)).collect();
        self.name_of(&streams, clut_hash(&self.clut), from, to)
    }

    /// `name` over each level's bytes hashed already (`level_bytes`).
    fn name_of(&self, streams: &[Vec<u8>], clut: u64, from: usize, to: usize) -> Name {
        let mut st = Xxh3::new();
        for b in &streams[from..=to] {
            st.update(b);
        }
        let base = &self.levels[0];
        let tw = Self::log2(base.width).saturating_sub(from as u32);
        let th = Self::log2(base.height).saturating_sub(from as u32);
        Name { tex0_hash: st.digest(), clut_hash: clut, region: (0, 0), bits: self.psm | tw << 6 | th << 10 }
    }

    /// Every name PCSX2 can give this texture, with the level each stands
    /// for (its picture is that level's), the one a pack is most likely to
    /// hold for it first: the whole chain from the base (Mipmapping on, a
    /// draw past the base: most of the world), the base alone (Mipmapping
    /// off, a texture with no levels, or a draw that stays on the base),
    /// then the chain from each level after the base (a draw that starts
    /// farther down).
    pub fn names(&self) -> Vec<(Name, usize)> {
        let top = self.top_level();
        let streams: Vec<Vec<u8>> = self.levels[..=top].iter().map(|l| level_bytes(self.psm, l)).collect();
        let clut = clut_hash(&self.clut);
        let mut out = vec![(self.name_of(&streams, clut, 0, top), 0)];
        if top > 0 {
            out.push((self.name_of(&streams, clut, 0, 0), 0));
            for s in 1..=top {
                out.push((self.name_of(&streams, clut, s, top), s));
            }
        }
        out
    }

    /// XXH3-64 of the CLUT as PCSX2 hashes it (`PaletteKeyHash`): every
    /// name of the texture carries it.
    pub fn clut_hash(&self) -> u64 {
        clut_hash(&self.clut)
    }

    /// A level as PCSX2 dumps it: RGBA8 rows, each texel its CLUT entry's
    /// bytes as stored (`psm.rtx` through the CLUT, `DumpTexture`).
    pub fn rgba(&self, level: usize) -> Vec<u8> {
        let l = &self.levels[level];
        let mut out = Vec::with_capacity(l.texels.len() * 4);
        for &t in &l.texels {
            out.extend_from_slice(&self.clut.get(t as usize).copied().unwrap_or(0).to_le_bytes());
        }
        out
    }
}

/// `PaletteKeyHash`: XXH3-64 over the CLUT's entries.
fn clut_hash(clut: &[u32]) -> u64 {
    let bytes: Vec<u8> = clut.iter().flat_map(|c| c.to_le_bytes()).collect();
    xxhash_rust::xxh3::xxh3_64(&bytes)
}

/// What `HashTextureLevel` hashes of a PSMT8 or PSMT4 level (`fmsk` all
/// ones, no region): a level of at least a block (16 x 16, 32 x 16) as the
/// GS memory's blocks over it, a row of blocks at a time (`bnMulti`, x then
/// y), 256 bytes each; a smaller one as `rtxP` expands it, a byte a texel,
/// `tw` bytes a row.
fn level_bytes(psm: u32, l: &GsLevel) -> Vec<u8> {
    let (bw, bh) = if psm == PSMT4 { (32, 16) } else { (16, 16) };
    let tw = 1usize << GsTexture::log2(l.width);
    let th = 1usize << GsTexture::log2(l.height);
    let w = l.width as usize;
    if tw < bw || th < bh {
        let mut out = Vec::with_capacity(tw * th);
        for y in 0..th {
            out.extend_from_slice(&l.texels[y * w..y * w + tw]);
        }
        return out;
    }
    let mut out = vec![0u8; (tw / bw) * (th / bh) * 256];
    let mut at = 0;
    for by in 0..th / bh {
        for bx in 0..tw / bw {
            let block = &mut out[at..at + 256];
            for y in 0..bh {
                let row = &l.texels[(by * bh + y) * w + bx * bw..][..bw];
                for (x, &t) in row.iter().enumerate() {
                    if psm == PSMT4 {
                        let n = COLUMN4[y][x] as usize;
                        block[n >> 1] |= t << ((n & 1) * 4);
                    } else {
                        block[COLUMN8[y][x] as usize] = t;
                    }
                }
            }
            at += 256;
        }
    }
    out
}

/// Where the GS keeps PSMT8 texel (x, y) of a block: the byte
/// (`columnTable8`, `pcsx2/GS/GSTables.cpp`; the GS manual's PSMT8 block).
pub const COLUMN8: [[u8; 16]; 16] = [
    [0, 4, 16, 20, 32, 36, 48, 52, 2, 6, 18, 22, 34, 38, 50, 54],
    [8, 12, 24, 28, 40, 44, 56, 60, 10, 14, 26, 30, 42, 46, 58, 62],
    [33, 37, 49, 53, 1, 5, 17, 21, 35, 39, 51, 55, 3, 7, 19, 23],
    [41, 45, 57, 61, 9, 13, 25, 29, 43, 47, 59, 63, 11, 15, 27, 31],
    [96, 100, 112, 116, 64, 68, 80, 84, 98, 102, 114, 118, 66, 70, 82, 86],
    [104, 108, 120, 124, 72, 76, 88, 92, 106, 110, 122, 126, 74, 78, 90, 94],
    [65, 69, 81, 85, 97, 101, 113, 117, 67, 71, 83, 87, 99, 103, 115, 119],
    [73, 77, 89, 93, 105, 109, 121, 125, 75, 79, 91, 95, 107, 111, 123, 127],
    [128, 132, 144, 148, 160, 164, 176, 180, 130, 134, 146, 150, 162, 166, 178, 182],
    [136, 140, 152, 156, 168, 172, 184, 188, 138, 142, 154, 158, 170, 174, 186, 190],
    [161, 165, 177, 181, 129, 133, 145, 149, 163, 167, 179, 183, 131, 135, 147, 151],
    [169, 173, 185, 189, 137, 141, 153, 157, 171, 175, 187, 191, 139, 143, 155, 159],
    [224, 228, 240, 244, 192, 196, 208, 212, 226, 230, 242, 246, 194, 198, 210, 214],
    [232, 236, 248, 252, 200, 204, 216, 220, 234, 238, 250, 254, 202, 206, 218, 222],
    [193, 197, 209, 213, 225, 229, 241, 245, 195, 199, 211, 215, 227, 231, 243, 247],
    [201, 205, 217, 221, 233, 237, 249, 253, 203, 207, 219, 223, 235, 239, 251, 255],
];

/// Where the GS keeps PSMT4 texel (x, y) of a block: the nibble, the even
/// one a byte's low four bits (`columnTable4`, `pcsx2/GS/GSTables.cpp`).
pub const COLUMN4: [[u16; 32]; 16] = [
    [0, 8, 32, 40, 64, 72, 96, 104, 2, 10, 34, 42, 66, 74, 98, 106, 4, 12, 36, 44, 68, 76, 100, 108, 6, 14, 38, 46, 70, 78, 102, 110],
    [16, 24, 48, 56, 80, 88, 112, 120, 18, 26, 50, 58, 82, 90, 114, 122, 20, 28, 52, 60, 84, 92, 116, 124, 22, 30, 54, 62, 86, 94, 118, 126],
    [65, 73, 97, 105, 1, 9, 33, 41, 67, 75, 99, 107, 3, 11, 35, 43, 69, 77, 101, 109, 5, 13, 37, 45, 71, 79, 103, 111, 7, 15, 39, 47],
    [81, 89, 113, 121, 17, 25, 49, 57, 83, 91, 115, 123, 19, 27, 51, 59, 85, 93, 117, 125, 21, 29, 53, 61, 87, 95, 119, 127, 23, 31, 55, 63],
    [192, 200, 224, 232, 128, 136, 160, 168, 194, 202, 226, 234, 130, 138, 162, 170, 196, 204, 228, 236, 132, 140, 164, 172, 198, 206, 230, 238, 134, 142, 166, 174],
    [208, 216, 240, 248, 144, 152, 176, 184, 210, 218, 242, 250, 146, 154, 178, 186, 212, 220, 244, 252, 148, 156, 180, 188, 214, 222, 246, 254, 150, 158, 182, 190],
    [129, 137, 161, 169, 193, 201, 225, 233, 131, 139, 163, 171, 195, 203, 227, 235, 133, 141, 165, 173, 197, 205, 229, 237, 135, 143, 167, 175, 199, 207, 231, 239],
    [145, 153, 177, 185, 209, 217, 241, 249, 147, 155, 179, 187, 211, 219, 243, 251, 149, 157, 181, 189, 213, 221, 245, 253, 151, 159, 183, 191, 215, 223, 247, 255],
    [256, 264, 288, 296, 320, 328, 352, 360, 258, 266, 290, 298, 322, 330, 354, 362, 260, 268, 292, 300, 324, 332, 356, 364, 262, 270, 294, 302, 326, 334, 358, 366],
    [272, 280, 304, 312, 336, 344, 368, 376, 274, 282, 306, 314, 338, 346, 370, 378, 276, 284, 308, 316, 340, 348, 372, 380, 278, 286, 310, 318, 342, 350, 374, 382],
    [321, 329, 353, 361, 257, 265, 289, 297, 323, 331, 355, 363, 259, 267, 291, 299, 325, 333, 357, 365, 261, 269, 293, 301, 327, 335, 359, 367, 263, 271, 295, 303],
    [337, 345, 369, 377, 273, 281, 305, 313, 339, 347, 371, 379, 275, 283, 307, 315, 341, 349, 373, 381, 277, 285, 309, 317, 343, 351, 375, 383, 279, 287, 311, 319],
    [448, 456, 480, 488, 384, 392, 416, 424, 450, 458, 482, 490, 386, 394, 418, 426, 452, 460, 484, 492, 388, 396, 420, 428, 454, 462, 486, 494, 390, 398, 422, 430],
    [464, 472, 496, 504, 400, 408, 432, 440, 466, 474, 498, 506, 402, 410, 434, 442, 468, 476, 500, 508, 404, 412, 436, 444, 470, 478, 502, 510, 406, 414, 438, 446],
    [385, 393, 417, 425, 449, 457, 481, 489, 387, 395, 419, 427, 451, 459, 483, 491, 389, 397, 421, 429, 453, 461, 485, 493, 391, 399, 423, 431, 455, 463, 487, 495],
    [401, 409, 433, 441, 465, 473, 497, 505, 403, 411, 435, 443, 467, 475, 499, 507, 405, 413, 437, 445, 469, 477, 501, 509, 407, 415, 439, 447, 471, 479, 503, 511],
];

#[cfg(test)]
mod tests {
    use super::*;

    fn level(w: u32, h: u32, f: impl Fn(u32, u32) -> u8) -> GsLevel {
        GsLevel { width: w, height: h, texels: (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| f(x, y)).collect() }
    }

    /// A PSMT8 block is the GS's swizzle of its 16 x 16 texels
    /// (`columnTable8`): texel (x, y) at byte COLUMN8[y][x], every byte one
    /// texel's; the blocks a row at a time. The control: the texels in plain
    /// rows (the order PCSX2 does not hash a whole block in) hash otherwise.
    #[test]
    fn a_psmt8_block_is_the_gs_swizzle() {
        let l = level(32, 16, |x, y| (x + 16 * y) as u8 ^ (x / 16 * 0x80) as u8);
        let b = level_bytes(PSMT8, &l);
        assert_eq!(b.len(), 512, "two blocks");
        for y in 0..16 {
            for x in 0..32 {
                let t = l.texels[(y * 32 + x) as usize];
                assert_eq!(b[(x as usize / 16) * 256 + COLUMN8[y as usize][x as usize % 16] as usize], t, "texel ({x}, {y})");
            }
        }
        assert_eq!((b[0], b[4], b[8], b[1]), (0, 1, 16, 36), "(0,0), (1,0), (0,1), then the first column's (4,2)");
        let plain = GsTexture { psm: PSMT8, levels: vec![l.clone()], clut: vec![0; 256] };
        let rows: Vec<u8> = l.texels.clone();
        assert_ne!(xxhash_rust::xxh3::xxh3_64(&rows), plain.name(0, 0).tex0_hash, "the control: plain rows");
        assert_eq!(xxhash_rust::xxh3::xxh3_64(&b), plain.name(0, 0).tex0_hash);
    }

    /// A PSMT4 block packs two texels a byte at the GS's nibbles
    /// (`columnTable4`), the even nibble the low four bits; a level narrower
    /// than a block (32) is its texels a byte each, row by row (`rtxP`). The
    /// control: the nibbles the other way round.
    #[test]
    fn a_psmt4_block_and_a_narrow_level() {
        let l = level(32, 16, |x, y| ((x * 7 + y * 3) & 15) as u8);
        let b = level_bytes(PSMT4, &l);
        assert_eq!(b.len(), 256);
        for (y, row) in COLUMN4.iter().enumerate() {
            for (x, &n) in row.iter().enumerate() {
                let n = n as usize;
                assert_eq!((b[n >> 1] >> ((n & 1) * 4)) & 15, l.texels[y * 32 + x], "texel ({x}, {y})");
            }
        }
        let swapped: Vec<u8> = b.iter().map(|v| v.rotate_left(4)).collect();
        assert_ne!(swapped, b, "the control: the nibbles swapped are other bytes");
        let narrow = level(16, 16, |x, y| ((x + y) & 15) as u8);
        assert_eq!(level_bytes(PSMT4, &narrow), narrow.texels, "16 wide: a byte a texel, in rows");
        let small = level(16, 8, |x, y| (x ^ y) as u8);
        assert_eq!(level_bytes(PSMT8, &small), small.texels, "under 16 high: in rows");
    }

    /// A 4-bit surface record as the game copies it: a row's even texel is a
    /// byte's low nibble. The control: the high nibble first (what the
    /// reference parser decodes) is other texels.
    #[test]
    fn a_4bit_record_reads_low_nibble_first() {
        let mut d = Vec::new();
        for v in [0x55i32, 32, 16, 0, 1] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&16i32.to_le_bytes());
        for i in 0..16u8 {
            d.extend_from_slice(&[i * 16, 255 - i * 16, i, 255]);
        }
        d.extend_from_slice(&16i32.to_le_bytes());
        for y in 0..16u8 {
            for i in 0..16u8 {
                d.push(((2 * i + y) & 15) | (((2 * i + 1 + y) & 15) << 4));
            }
        }
        let s = Surface::read(&d, 0);
        let t = s.gs(&d).expect("a 4-bit surface");
        assert_eq!((t.psm, t.levels.len(), t.clut.len()), (PSMT4, 1, 16));
        let l = &t.levels[0];
        for y in 0..16usize {
            for x in 0..32usize {
                let byte = d[d.len() - 256 + y * 16 + x / 2];
                assert_eq!(l.texels[y * 32 + x], if x % 2 == 0 { byte & 15 } else { byte >> 4 }, "({x}, {y})");
            }
        }
        assert_eq!((l.texels[0], l.texels[1]), (0, 1));
        assert_ne!(l.texels[0], d[d.len() - 256] >> 4, "the control: the high nibble is texel 1, not texel 0");
        assert_eq!(&t.rgba(0)[..8], &[0, 255, 0, 255, 16, 239, 1, 255], "texels 0 and 1 of row 0: colours 0 and 1");
        // a palette short of 16 names nothing
        let mut short = d.clone();
        short[20..24].copy_from_slice(&15i32.to_le_bytes());
        assert!(Surface::read(&short, 0).gs(&short).is_none());
    }

    /// The names of a three-level texture, the order a pack is looked
    /// through: the chain from the base (named as the base), the base alone,
    /// then the chains from levels 1 and 2 (named as those levels: TW and TH
    /// one and two less). Each chain is its levels' bytes hashed in turn.
    #[test]
    fn a_textures_names_in_order() {
        let t = GsTexture {
            psm: PSMT8,
            levels: vec![level(64, 32, |x, y| (x * y) as u8), level(32, 16, |x, y| (x + y) as u8), level(16, 8, |x, y| (x ^ y) as u8)],
            clut: (0..256).collect(),
        };
        let names = t.names();
        assert_eq!(names.iter().map(|n| n.1).collect::<Vec<_>>(), vec![0, 0, 1, 2]);
        let bits = |tw: u32, th: u32| PSMT8 | tw << 6 | th << 10;
        assert_eq!(names.iter().map(|n| n.0.bits).collect::<Vec<_>>(), vec![bits(6, 5), bits(6, 5), bits(5, 4), bits(4, 3)]);
        let stream = |k: usize| level_bytes(PSMT8, &t.levels[k]);
        let cat = |ks: &[usize]| ks.iter().flat_map(|&k| stream(k)).collect::<Vec<u8>>();
        assert_eq!(names[0].0.tex0_hash, xxhash_rust::xxh3::xxh3_64(&cat(&[0, 1, 2])));
        assert_eq!(names[1].0.tex0_hash, xxhash_rust::xxh3::xxh3_64(&cat(&[0])));
        assert_eq!(names[2].0.tex0_hash, xxhash_rust::xxh3::xxh3_64(&cat(&[1, 2])));
        assert_eq!(names[3].0.tex0_hash, xxhash_rust::xxh3::xxh3_64(&cat(&[2])));
        let clut: Vec<u8> = (0..256u32).flat_map(|c| c.to_le_bytes()).collect();
        assert!(names.iter().all(|n| n.0.clut_hash == xxhash_rust::xxh3::xxh3_64(&clut)));
        assert_eq!(t.name(0, 2), names[0].0);
    }
}
