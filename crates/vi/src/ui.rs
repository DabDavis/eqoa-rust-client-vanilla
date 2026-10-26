//! `VIUI`, the device every menu and HUD element is drawn through (SUPPORT,
//! phase 42): the 252-entry atlas (`GetUITexture`), the word and
//! texture-name tables, the "ArialUI" font, and the blitters; and
//! `VIUIWindow` / `VIUITextScroll` as the HUD uses them. Everything is drawn
//! through `Raster`, the `VIRaster` calls the decomp makes (`Blit` by
//! DictID, `DrawString`, `DrawCenteredString`, the viewport), so a renderer
//! turns them into pixels and a test records them for the decomp's own
//! (`tests/oracle_ui.rs`).
//!
//! An atlas entry names a ROW of TEXTURES.TXT; the row's name, as
//! `P:\studio\eqps2\Game_Assets\UI\<name>.tga`, hashes to the DictID of a
//! surface in UI.CSF, and the entry's rectangle is cut out of it. The table
//! itself is SUPPORT's data (only `extern` in the decomp), read out of the
//! player's SUPPORT image (`read_atlas`): their decrypted image, or the
//! disc's own unpacked by crate `dnas` (the client's `modules`).
// The methods keep the decomp's parameter lists.
#![allow(clippy::too_many_arguments)]

use crate::strtab::StringTable;

/// SUPPORT's image is indexed by `VA - 0x003E3D00`; the table is at
/// `0x004EDAF8`, 252 entries of 36 bytes.
const ATLAS_VA: usize = 0x004E_DAF8;
const IMAGE_BASE: usize = 0x003E_3D00;
const ATLAS_LEN: usize = 252;

/// `VIUITexEntry`.
#[derive(Clone, Copy, Debug, Default)]
pub struct UiTexEntry {
    /// A second atlas id (the art a gauge or a filler actually blits).
    pub x00: i32,
    /// The row of TEXTURES.TXT naming the surface.
    pub row: i32,
    pub uv0: [i32; 2],
    pub uv1: [i32; 2],
    pub mode: i32,
    pub alpha: f32,
    pub flags: u32,
}

/// The atlas out of a decrypted SUPPORT image (un-relocated, as
/// `VILauncher::UnpackAndVerify` leaves it). `None` for an image that is not
/// the Frontiers build's: there every entry's first field is its own index,
/// which nothing else at that address is.
pub fn read_atlas(image: &[u8]) -> Option<Vec<UiTexEntry>> {
    let base = ATLAS_VA - IMAGE_BASE;
    let bytes = image.get(base..base + ATLAS_LEN * 36)?;
    let w = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
    let atlas: Vec<UiTexEntry> = (0..ATLAS_LEN)
        .map(|i| {
            let o = i * 36;
            UiTexEntry {
                x00: w(o),
                row: w(o + 4),
                uv0: [w(o + 8), w(o + 12)],
                uv1: [w(o + 16), w(o + 20)],
                mode: w(o + 24),
                alpha: f32::from_bits(w(o + 28) as u32),
                flags: w(o + 32) as u32,
            }
        })
        .collect();
    atlas.iter().enumerate().all(|(i, e)| e.x00 == i as i32).then_some(atlas)
}

/// `VIHashResourceID_Icon`.
pub fn icon_id(name: &str) -> u32 {
    crate::hash::hash_resource_id(&format!("P:\\studio\\eqps2\\Game_Assets\\UI\\{name}.tga"))
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// `VIFont::sm_FontName`'s id ("ArialUI"), the font every UI string is in.
pub fn font_id() -> u32 {
    crate::hash::hash_resource_id("ArialUI")
}

/// The `VIRaster` calls `VIUI` makes. `blit` is by surface DictID (`VIUI`
/// finds it in the dictionary first); a string goes in the font's colour
/// as `VIFont` hands it over; the viewport is inclusive (x1, y1, x2, y2)
/// and moves the origin as well as cutting.
pub trait Raster {
    /// `Blit(i, flags, at, a, b, m, alpha)`: texels `a` .. `a + b`, `b`
    /// big at `at`, mirrored by `flags & 3`.
    fn blit(&mut self, id: u32, flags: u32, at: [i32; 2], a: [i32; 2], b: [i32; 2], mode: i32, alpha: f32);
    /// `StretchBlit(i, flags, at, size, a, b, m, alpha)`: texels `a` ..
    /// `a + b` drawn `size` big at `at`.
    fn stretch_blit(&mut self, id: u32, flags: u32, at: [i32; 2], size: [i32; 2], a: [i32; 2], b: [i32; 2], mode: i32, alpha: f32);
    /// `FillRectangle`'s fan: `SetSolidFillUIMaterial`, the colour, and the
    /// corners `a`, (b.x, a.y), `b`, (a.x, b.y).
    fn fill_fan(&mut self, color: [f32; 4], corners: [[i32; 2]; 4]);
    /// `Blit(i, flags, at, m, alpha)`: the whole surface at its own size.
    fn blit_full(&mut self, id: u32, flags: u32, at: [i32; 2], mode: i32, alpha: f32);
    fn draw_string(&mut self, at: [i32; 2], color: [f32; 4], s: &[u16]);
    /// `DrawCenteredString(font, at, c, s, mode)`: 1 centres vertically, 2
    /// horizontally, 0 and 3 both, anything else draws nothing.
    fn draw_centered_string(&mut self, at: [i32; 2], color: [f32; 4], s: &[u16], mode: u32);
    /// `GetStringWidth`: each glyph's width plus one, `'?'` for a missing
    /// one, -1 when that is missing too.
    fn string_width(&self, s: &[u16]) -> i32;
    /// `GetPointSize`: the font's height.
    fn point_size(&self) -> i32;
    fn viewport(&self) -> [i32; 4];
    fn set_viewport(&mut self, v: [i32; 4]);
    /// Whether surface `id` is there to draw (CLIENT's icon cache asks; a
    /// recorder has everything).
    fn has(&self, _id: u32) -> bool {
        true
    }
}

pub struct Vui {
    pub words: StringTable,
    pub textures: StringTable,
    pub language: i32,
    atlas: Vec<UiTexEntry>,
    /// `VIFont::m_color`, for the plain `DrawString`.
    pub font_color: [f32; 4],
    /// `VITimer::GetTime`, for the pulses.
    pub time: i64,
}

impl Vui {
    pub fn new(words: StringTable, textures: StringTable, atlas: Vec<UiTexEntry>) -> Self {
        Vui { words, textures, language: 0, atlas, font_color: [1.0; 4], time: 0 }
    }

    /// `GetUITexture`: entry 0 out of range.
    pub fn entry(&self, id: i32) -> UiTexEntry {
        let i = if (0..ATLAS_LEN as i32).contains(&id) { id as usize } else { 0 };
        self.atlas.get(i).copied().unwrap_or_default()
    }

    /// The DictID of an entry's surface: its TEXTURES.TXT row's name as an
    /// icon path.
    pub fn entry_id(&self, e: &UiTexEntry) -> u32 {
        icon_id(self.textures.get(e.row, self.language))
    }

    /// `BlitEx(id, at, size, m, alpha, flags)`; a size of -1 takes the
    /// entry's own.
    pub fn blit_ex_size(&self, ui: &mut dyn Raster, id: i32, at: [i32; 2], size: [i32; 2], mode: i32, alpha: f32, flags: u32) {
        let e = self.entry(id);
        let b = [if size[0] == -1 { e.uv1[0] } else { size[0] }, if size[1] == -1 { e.uv1[1] } else { size[1] }];
        ui.blit(self.entry_id(&e), flags, at, e.uv0, b, mode, alpha);
    }

    /// `BlitEx(id, at, m, alpha, flags)`.
    pub fn blit_ex(&self, ui: &mut dyn Raster, id: i32, at: [i32; 2], mode: i32, alpha: f32, flags: u32) {
        self.blit_ex_size(ui, id, at, [-1, -1], mode, alpha, flags);
    }

    /// `Blit(id, at, size)`: the entry's own mode, alpha and flags, `size`
    /// big (-1 for the entry's own).
    pub fn blit_size(&self, ui: &mut dyn Raster, id: i32, at: [i32; 2], size: [i32; 2]) {
        let e = self.entry(id);
        self.blit_ex_size(ui, id, at, size, e.mode, e.alpha, e.flags);
    }

    /// `Blit(id, at)`: the entry's own mode, alpha and flags.
    pub fn blit(&self, ui: &mut dyn Raster, id: i32, at: [i32; 2]) {
        let e = self.entry(id);
        self.blit_ex_size(ui, id, at, [-1, -1], e.mode, e.alpha, e.flags);
    }

    /// `BlitHorz`: a row-1 entry tiled across `w`, else stretched.
    pub fn blit_horz(&self, ui: &mut dyn Raster, at: [i32; 2], id: i32, w: i32, mode: i32, alpha: f32, flags: u32) {
        if w <= 0 {
            return;
        }
        let e = self.entry(id);
        let mut p = at;
        if e.row == 1 {
            while p[0] + e.uv1[0] < at[0] + w {
                self.blit_ex(ui, id, p, mode, alpha, flags);
                p[0] += e.uv1[0];
            }
            self.blit_ex_size(ui, id, p, [at[0] + w - p[0], e.uv1[1]], mode, alpha, flags);
        } else {
            self.blit_ex_size(ui, id, p, [w, e.uv1[1]], mode, alpha, flags);
        }
    }

    /// `BlitVert`: `BlitHorz` down the screen.
    pub fn blit_vert(&self, ui: &mut dyn Raster, at: [i32; 2], id: i32, h: i32, mode: i32, alpha: f32, flags: u32) {
        if h <= 0 {
            return;
        }
        let e = self.entry(id);
        let mut p = at;
        if e.row == 1 {
            while p[1] + e.uv1[1] < at[1] + h {
                self.blit_ex(ui, id, p, mode, alpha, flags);
                p[1] += e.uv1[1];
            }
            self.blit_ex_size(ui, id, p, [e.uv1[0], at[1] + h - p[1]], mode, alpha, flags);
        } else {
            self.blit_ex_size(ui, id, p, [e.uv1[0], h], mode, alpha, flags);
        }
    }

    /// `BlitFrame`: four edges tiled between four corners (the left edge
    /// and the left corners only when `left`).
    pub fn blit_frame(&self, ui: &mut dyn Raster, at: [i32; 2], size: [i32; 2], fp: &FrameParam, mode: i32, alpha: f32, left: bool) {
        let w = size[0];
        if w <= 0 || size[1] <= 0 {
            return;
        }
        let tl = at;
        let br = [size[0] + at[0], size[1] + at[1]];
        let (e0, e1, e2, e3) = (self.entry(fp.tl.0), self.entry(fp.tr.0), self.entry(fp.br.0), self.entry(fp.bl.0));
        self.blit_horz(
            ui,
            [tl[0] + if left { e0.uv1[0] } else { 0 }, tl[1]],
            fp.top.0,
            if left { w - e0.uv1[0] - e1.uv1[0] } else { w - e1.uv1[0] },
            mode,
            alpha,
            fp.top.1,
        );
        self.blit_horz(
            ui,
            [tl[0] + if left { e3.uv1[0] } else { 0 }, br[1] - e3.uv1[1]],
            fp.bottom.0,
            if left { size[0] - e3.uv1[0] - e2.uv1[0] } else { size[0] - e2.uv1[0] },
            mode,
            alpha,
            fp.bottom.1,
        );
        if left {
            self.blit_vert(ui, [tl[0], tl[1] + e0.uv1[1]], fp.left.0, size[1] - e0.uv1[1] - e3.uv1[1], mode, alpha, fp.left.1);
        }
        self.blit_vert(ui, [br[0] - e1.uv1[0], tl[1] + e1.uv1[1]], fp.right.0, size[1] - e1.uv1[1] - e2.uv1[1], mode, alpha, fp.right.1);
        if left {
            self.blit_ex(ui, fp.tl.0, tl, mode, alpha, fp.tl.1);
        }
        self.blit_ex(ui, fp.tr.0, [br[0] - e1.uv1[0], tl[1]], mode, alpha, fp.tr.1);
        if left {
            self.blit_ex(ui, fp.bl.0, [tl[0], br[1] - e3.uv1[1]], mode, alpha, fp.bl.1);
        }
        self.blit_ex(ui, fp.br.0, [br[0] - e2.uv1[0], br[1] - e2.uv1[1]], mode, alpha, fp.br.1);
    }

    /// `FillRectangle(r, c)`: a solid rectangle from `r[0]` to `r[1]`.
    pub fn fill_rectangle(&self, ui: &mut dyn Raster, r: [[i32; 2]; 2], c: [f32; 4]) {
        ui.fill_fan(c, [r[0], [r[1][0], r[0][1]], r[1], [r[0][0], r[1][1]]]);
    }

    /// `FrameFillRectangle(r)`: white, and black two pixels in.
    pub fn frame_fill_rectangle(&self, ui: &mut dyn Raster, r: [[i32; 2]; 2]) {
        self.fill_rectangle(ui, r, [1.0; 4]);
        self.fill_rectangle(ui, [[r[0][0] + 2, r[0][1] + 2], [r[1][0] - 2, r[1][1] - 2]], [0.0, 0.0, 0.0, 1.0]);
    }

    /// `Blit(int id, at, m, alpha, flags)`: a surface by DictID, whole.
    pub fn blit_id(&self, ui: &mut dyn Raster, id: u32, at: [i32; 2], mode: i32, alpha: f32, flags: u32) {
        ui.blit_full(id, flags, at, mode, alpha);
    }

    /// `BlitAbilityIcon(at, idx, m, alpha)`: the ability row's plate, left
    /// cap 77, the middle 80 (idx 0) or 79, the right cap 78.
    pub fn blit_ability_icon(&self, ui: &mut dyn Raster, at: [i32; 2], idx: i32, mode: i32, alpha: f32) {
        let mut p = at;
        let l = self.entry(77);
        self.blit_ex(ui, 77, p, mode, alpha, 0);
        let id = if idx == 0 { 80 } else { 79 };
        let e = self.entry(id);
        p[0] += l.uv1[0];
        self.blit_ex(ui, id, p, mode, alpha, 0);
        p[0] += e.uv1[0];
        self.blit_ex(ui, 78, p, mode, alpha, 0);
    }

    /// `StretchBlitEx(id, at, size, m, alpha)`: the entry's texels drawn
    /// `size` big.
    pub fn stretch_blit_ex(&self, ui: &mut dyn Raster, id: i32, at: [i32; 2], size: [i32; 2], mode: i32, alpha: f32) {
        let e = self.entry(id);
        ui.stretch_blit(self.entry_id(&e), 0, at, size, e.uv0, e.uv1, mode, alpha);
    }

    /// `BlitStuddedFrame`: the frame, then `nx` studs evenly along the top
    /// and bottom and `ny` down the sides (the left only when `left`).
    pub fn blit_studded_frame(&self, ui: &mut dyn Raster, at: [i32; 2], size: [i32; 2], fp: &FrameParam, studs: &FrameParam, nx: i32, ny: i32, mode: i32, alpha: f32, left: bool) {
        self.blit_frame(ui, at, size, fp, mode, alpha, left);
        let mut p = at;
        let e = self.entry(studs.top.0);
        let step = size[0] / (nx + 1) - e.uv1[0] / 2;
        for _ in 0..nx.max(0) {
            p[0] += step;
            self.blit_ex(ui, studs.top.0, p, mode, alpha, studs.top.1);
        }
        p = at;
        let e = self.entry(studs.bottom.0);
        p[1] += size[1] - e.uv1[1];
        let step = size[0] / (nx + 1) - e.uv1[0] / 2;
        for _ in 0..nx.max(0) {
            p[0] += step;
            self.blit_ex(ui, studs.bottom.0, p, mode, alpha, studs.bottom.1);
        }
        if left {
            p = at;
            let e = self.entry(studs.left.0);
            let step = size[1] / (ny + 1) - e.uv1[1] / 2;
            for _ in 0..ny.max(0) {
                p[1] += step;
                self.blit_ex(ui, studs.left.0, p, mode, alpha, studs.left.1);
            }
        }
        p = at;
        let e = self.entry(studs.right.0);
        p[0] += size[0] - e.uv1[0];
        let step = size[1] / (ny + 1) - e.uv1[1] / 2;
        for _ in 0..ny.max(0) {
            p[1] += step;
            self.blit_ex(ui, studs.right.0, p, mode, alpha, studs.right.1);
        }
    }

    /// `BlitFillStuddedFrame`: `fill` inside the corners, then the studded
    /// frame.
    pub fn blit_fill_studded_frame(&self, ui: &mut dyn Raster, at: [i32; 2], size: [i32; 2], fp: &FrameParam, studs: &FrameParam, fill: i32, nx: i32, ny: i32, mode: i32, alpha: f32, left: bool) {
        let (e0, e1) = (self.entry(fp.tl.0), self.entry(fp.br.0));
        let p = [at[0] + e0.uv1[0], at[1] + e0.uv1[1]];
        let sz = [size[0] - e0.uv1[0] - e1.uv1[0], size[1] - e0.uv1[1] - e1.uv1[1]];
        self.blit_rect(ui, fill, p, sz, mode, alpha);
        self.blit_studded_frame(ui, at, size, fp, studs, nx, ny, mode, alpha, left);
    }

    /// `BlitStuddedHorz`: the bar across `w`, then `n` studs along it.
    pub fn blit_studded_horz(&self, ui: &mut dyn Raster, at: [i32; 2], bar: i32, stud: i32, w: i32, n: i32, mode: i32, alpha: f32, flags: u32) {
        if w <= 0 {
            return;
        }
        let mut p = at;
        self.blit_horz(ui, p, bar, w, mode, alpha, flags);
        let e = self.entry(stud);
        let step = w / (n + 1) - e.uv1[0] / 2;
        for _ in 0..n.max(0) {
            p[0] += step;
            self.blit_ex(ui, stud, p, mode, alpha, flags);
        }
    }

    /// `BlitVertBar`: the top cap, the middle down `h` (tiled for a row-1
    /// entry, else stretched), the bottom cap.
    pub fn blit_vert_bar(&self, ui: &mut dyn Raster, at: [i32; 2], top: i32, bot: i32, mid: i32, h: i32, mode: i32, alpha: f32) {
        let (e1, e2, e3) = (self.entry(top), self.entry(bot), self.entry(mid));
        let mut p = at;
        self.blit_ex(ui, top, p, mode, alpha, 0);
        p[1] += e1.uv1[1];
        if e3.row == 1 {
            self.blit_vert(ui, p, mid, h - e1.uv1[1] - e2.uv1[1], mode, alpha, 0);
        } else {
            self.blit_ex_size(ui, e3.x00, p, [e3.uv1[0], h - e1.uv1[1] - e2.uv1[1]], mode, alpha, 0);
        }
        p[1] = at[1] + h - e2.uv1[1];
        self.blit_ex(ui, bot, p, mode, alpha, 0);
    }

    /// `BlitFillFrame`: `fill` tiled inside the corners, then the frame.
    pub fn blit_fill_frame(&self, ui: &mut dyn Raster, at: [i32; 2], size: [i32; 2], fp: &FrameParam, fill: i32, mode: i32, alpha: f32, left: bool) {
        let (e0, e1) = (self.entry(fp.tl.0), self.entry(fp.br.0));
        let p = [at[0] + e0.uv1[0], at[1] + e0.uv1[1]];
        let sz = [size[0] - e0.uv1[0] - e1.uv1[0], size[1] - e0.uv1[1] - e1.uv1[1]];
        self.blit_rect(ui, fill, p, sz, mode, alpha);
        self.blit_frame(ui, at, size, fp, mode, alpha, left);
    }

    /// `BlitRect`: a row-1 entry tiled over the rectangle (the last column
    /// and row cut to fit), else entry `x00` stretched over it.
    pub fn blit_rect(&self, ui: &mut dyn Raster, id: i32, at: [i32; 2], size: [i32; 2], mode: i32, alpha: f32) {
        let e = self.entry(id);
        let mut p = at;
        let mut d = [0, 0];
        if e.row == 1 {
            while p[0] + e.uv1[0] < at[0] + size[0] {
                while p[1] + e.uv1[1] < at[1] + size[1] {
                    self.blit_ex(ui, id, p, mode, alpha, 0);
                    p[1] += e.uv1[1];
                }
                p[0] += e.uv1[0];
                d[1] = at[1] + size[1] - p[1];
                p[1] = at[1];
            }
            d[0] = at[0] + size[0] - p[0];
            while p[1] + e.uv1[1] < at[1] + size[1] {
                self.blit_ex_size(ui, id, p, [d[0], e.uv1[1]], mode, alpha, 0);
                p[1] += e.uv1[1];
            }
            p[0] = at[0];
            while p[0] + e.uv1[0] < at[0] + size[0] {
                self.blit_ex_size(ui, id, p, [e.uv1[0], d[1]], mode, alpha, 0);
                p[0] += e.uv1[0];
            }
            self.blit_ex_size(ui, id, p, d, mode, alpha, 0);
        } else {
            self.blit_ex_size(ui, e.x00, at, size, mode, alpha, 0);
        }
    }

    /// `BlitBar(s, c, at, end, mid, w, m, alpha, just, shadow)`: the end cap,
    /// the middle across, the end cap mirrored, and the text on it.
    pub fn blit_bar(&self, ui: &mut dyn Raster, s: &[u16], c: [f32; 4], at: [i32; 2], end: i32, mid: i32, w: i32, mode: i32, alpha: f32, just: u32, shadow: bool) {
        let (e1, e2) = (self.entry(end), self.entry(mid));
        let mut p = at;
        self.blit_ex(ui, end, p, mode, alpha, 0);
        p[0] += e1.uv1[0];
        if e2.row == 1 {
            self.blit_horz(ui, p, mid, w - e1.uv1[0] * 2, mode, alpha, 0);
        } else {
            self.blit_ex_size(ui, mid, p, [w - e1.uv1[0] * 2, e2.uv1[1]], mode, alpha, 0);
        }
        p[0] = at[0] + w - e1.uv1[0];
        self.blit_ex(ui, end, p, mode, alpha, 1);
        let off = [0, 3];
        let tp = if just == 1 {
            [at[0] + e1.uv1[0] - off[0], at[1] + e2.uv1[1] / 2 - off[1]]
        } else {
            [at[0] + w / 2 - off[0], at[1] + e2.uv1[1] / 2 - off[1]]
        };
        self.draw_centered_string_ex(ui, s, tp[0], tp[1], c, just, shadow);
    }

    /// `BlitBar(s, c, at, left, right, mid, w, m, alpha, just, shadow)`: the
    /// three-piece bar (its own left and right caps), and the text on it.
    pub fn blit_bar3(&self, ui: &mut dyn Raster, s: &[u16], c: [f32; 4], at: [i32; 2], left: i32, right: i32, mid: i32, w: i32, mode: i32, alpha: f32, just: u32, shadow: bool) {
        let (e1, e2, e3) = (self.entry(left), self.entry(right), self.entry(mid));
        let mut p = at;
        self.blit_ex(ui, left, p, mode, alpha, 0);
        p[0] += e1.uv1[0];
        if e3.row == 1 {
            self.blit_horz(ui, p, mid, w - e1.uv1[0] - e2.uv1[0], mode, alpha, 0);
        } else {
            self.blit_ex_size(ui, mid, p, [w - e1.uv1[0] - e2.uv1[0], e3.uv1[1]], mode, alpha, 0);
        }
        p[0] = at[0] + w - e2.uv1[0];
        self.blit_ex(ui, right, p, mode, alpha, 0);
        let off = [0, 3];
        let tp = if just == 1 {
            [at[0] + e1.uv1[0] - off[0], at[1] + e3.uv1[1] / 2 - off[1]]
        } else {
            [at[0] + w / 2 - off[0], at[1] + e3.uv1[1] / 2 - off[1]]
        };
        self.draw_centered_string_ex(ui, s, tp[0], tp[1], c, just, shadow);
    }

    /// `VIFont::GetHeight`.
    pub fn font_height(&self, ui: &dyn Raster) -> i32 {
        ui.point_size()
    }

    /// `VIFont::GetStringWidth`.
    pub fn string_width(&self, ui: &dyn Raster, s: &[u16]) -> i32 {
        ui.string_width(s)
    }

    /// `VIFont::DrawString`, in the font's colour.
    pub fn draw_string(&self, ui: &mut dyn Raster, s: &[u16], x: i32, y: i32) {
        ui.draw_string([x, y], self.font_color, s);
    }

    /// `DrawStringEx`: a black shadow 2 down and right, then the text (the
    /// font's colour is set for each and put back).
    pub fn draw_string_ex(&self, ui: &mut dyn Raster, s: &[u16], x: i32, y: i32, c: [f32; 4], shadow: bool) {
        if shadow {
            ui.draw_string([x + 2, y + 2], [0.0, 0.0, 0.0, c[3]], s);
        }
        ui.draw_string([x, y], c, s);
    }

    /// `DrawStringEx(word, ...)`.
    pub fn draw_word_ex(&self, ui: &mut dyn Raster, word: i32, x: i32, y: i32, c: [f32; 4], shadow: bool) {
        let s = wide(self.words.get(word, self.language));
        self.draw_string_ex(ui, &s, x, y, c, shadow);
    }

    /// `DrawCenteredStringEx`.
    pub fn draw_centered_string_ex(&self, ui: &mut dyn Raster, s: &[u16], x: i32, y: i32, c: [f32; 4], mode: u32, shadow: bool) {
        if shadow {
            ui.draw_centered_string([x + 2, y + 2], [0.0, 0.0, 0.0, c[3]], s, mode);
        }
        ui.draw_centered_string([x, y], c, s, mode);
    }
}

/// What `VIClientText::Expand` reads off the client.
#[derive(Clone, Debug, Default)]
pub struct ExpandContext {
    pub player_name: String,
    pub level: i32,
    /// The class's name (WORDS.TXT 165 + class), empty for none.
    pub class_name: String,
    pub target_name: String,
}

/// `VIClientText::ParseToken`: digits make a number, else one letter (or
/// `& * ( %`) is the token by its code; anything else ends it as 0.
fn parse_token(s: &[u16], mut p: usize) -> (usize, i32) {
    let mut n = 0i32;
    while let Some(&c) = s.get(p) {
        if c == 0 {
            break;
        }
        if (b'0' as u16..=b'9' as u16).contains(&c) {
            n = n.wrapping_mul(10).wrapping_add((c - b'0' as u16) as i32);
            p += 1;
        } else {
            let ch = char::from_u32(c as u32).unwrap_or('\0');
            if ch.is_ascii_alphabetic() || matches!(ch, '&' | '*' | '(' | '%') {
                n = c as i32;
                p += 1;
            }
            break;
        }
    }
    (p, n)
}

/// `VIClientText::Expand(dst, size, src, flag)`: a server string's `%`
/// codes. With `flag`, a letter (or `& * (`, or 7..9) becomes the `|x`
/// inline-icon code and `%p` the player's name; without, `%1`..`%4` are the
/// player's name, level, class and the target's name. `%%` is a `%`,
/// anything else `(?)`. Stops where the next piece no longer fits `size`.
pub fn expand(src: &[u16], size: usize, flag: bool, ctx: &ExpandContext) -> Vec<u16> {
    let mut d: Vec<u16> = Vec::new();
    let mut p = 0;
    while let Some(&c) = src.get(p) {
        if c == 0 {
            break;
        }
        p += 1;
        if c == b'%' as u16 {
            let (np, tok) = parse_token(src, p);
            p = np;
            let piece: String = if flag {
                match tok {
                    116 => "|t".into(),
                    99 => "|c".into(),
                    120 => "|x".into(),
                    115 => "|s".into(),
                    112 => ctx.player_name.clone(),
                    7 => "|7".into(),
                    38 => "|&".into(),
                    8 => "|8".into(),
                    42 => "|*".into(),
                    9 => "|9".into(),
                    40 => "|(".into(),
                    108 => "|v".into(),
                    114 => "|V".into(),
                    117 => "|z".into(),
                    100 => "|Z".into(),
                    97 => "|a".into(),
                    65 => "|A".into(),
                    37 => "%".into(),
                    _ => "(?)".into(),
                }
            } else {
                match tok {
                    1 => ctx.player_name.clone(),
                    2 => ctx.level.to_string(),
                    3 => ctx.class_name.clone(),
                    4 => ctx.target_name.clone(),
                    37 => "%".into(),
                    _ => "(?)".into(),
                }
            };
            let piece = wide(&piece);
            if d.len() + piece.len() < size {
                d.extend(piece);
            } else {
                break;
            }
        } else if d.len() + 1 < size {
            d.push(c);
        } else {
            break;
        }
    }
    d
}

/// `VIUIFrameParam`: (atlas id, flags) for each corner and edge.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameParam {
    pub tl: (i32, u32),
    pub top: (i32, u32),
    pub tr: (i32, u32),
    pub right: (i32, u32),
    pub br: (i32, u32),
    pub bottom: (i32, u32),
    pub bl: (i32, u32),
    pub left: (i32, u32),
}

/// `VIUIWindow::Draw`'s four frame sets (the client's minimap borders
/// itself in the first, `eqoa::map::mini`).
pub fn frame_sets() -> [FrameParam; 4] {
    [
        FrameParam { tl: (36, 0), top: (37, 0), tr: (36, 1), right: (42, 1), br: (36, 1), bottom: (37, 0), bl: (36, 0), left: (42, 0) },
        FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) },
        FrameParam { tl: (0, 0), top: (38, 0), tr: (0, 0), right: (43, 1), br: (0, 0), bottom: (41, 0), bl: (0, 0), left: (43, 0) },
        FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 1), br: (106, 0), bottom: (102, 2), bl: (105, 0), left: (104, 0) },
    ]
}

/// `VIUIWindow::SetUiColors`: the palette a `VIUICOLOR` indexes (0 the
/// text, 1 the selection, 2 dim, 3 light).
pub const UI_COLORS: [[f32; 4]; 4] = [[0.0, 0.0, 0.0, 1.0], [0.039_215_688, 0.196_078_43, 0.274_509_82, 1.0], [0.4, 0.4, 0.4, 1.0], [0.9, 0.9, 0.9, 1.0]];

/// `VIUIWindow`: a framed panel that opens and closes over a few frames.
#[derive(Clone, Copy, Debug, Default)]
pub struct Window {
    pub center: [i32; 2],
    pub size: [i32; 2],
    pub title_h: i32,
    pub rows: i32,
    pub open: f32,
    pub dir: f32,
    pub x178: i32,
    pub pulse_time: i64,
    /// `m_title`, as `SetTitle` keeps it (8-bit, `str16to8`; a character
    /// past 255 is '?' here, where the game maps code page 1252 first)
    pub title: Title,
}

/// A window's title: up to 63 characters.
#[derive(Clone, Copy, Debug, Default)]
pub struct Title {
    pub len: u8,
    pub text: [[u16; 16]; 4],
}

impl Title {
    pub fn chars(&self) -> Vec<u16> {
        self.text.iter().flatten().take(self.len as usize).copied().collect()
    }
}

impl Window {
    /// `Init(ui, pos, center, w, rows, x)`.
    pub fn init(&mut self, vui: &Vui, ui: &dyn Raster, center: [i32; 2], w: i32, rows: i32, x: i32) {
        let h = vui.font_height(ui);
        self.center = center;
        self.size = [w, rows * (h - 4) + 36];
        self.rows = rows;
        self.x178 = x;
        self.title_h = 18;
    }

    /// `SetTitle(s)`: a title makes the title bar 24 high instead of 18,
    /// the window growing by twice the change. (`None` is the null pointer,
    /// which the game's would read through.)
    pub fn set_title(&mut self, s: Option<&[u16]>) {
        let h = if s.is_some() { 24 } else { 18 };
        let old = self.title_h;
        self.title_h = h;
        self.size[1] = self.size[1] - old * 2 + h * 2;
        let s = s.unwrap_or(&[]);
        let n = s.iter().position(|&c| c == 0).unwrap_or(s.len()).min(63);
        let mut t = Title { len: n as u8, ..Default::default() };
        for (k, &c) in s[..n].iter().enumerate() {
            t.text[k / 16][k % 16] = if c < 256 { c } else { b'?' as u16 };
        }
        self.title = t;
    }

    /// `Open(now)`.
    pub fn open(&mut self, now: bool) {
        if now {
            self.open = 1.0;
        }
        self.dir = 1.0;
    }

    /// `Close(now)`.
    pub fn close(&mut self, now: bool) {
        if now {
            self.open = 0.0;
        }
        self.dir = -1.0;
    }

    pub fn is_open(&self) -> bool {
        self.open == 1.0
    }

    pub fn is_opening(&self) -> bool {
        self.open < 1.0 && self.dir == 1.0
    }

    pub fn is_closing(&self) -> bool {
        self.open > 0.0 && self.dir == -1.0
    }

    /// `RowCenter(row)`.
    pub fn row_center(&self, vui: &Vui, ui: &dyn Raster, row: i32) -> [i32; 2] {
        let h = vui.font_height(ui);
        let t = (h - 4) * row + 5;
        [self.center[0] - 8, self.center[1] - (self.size[1] >> 1) + self.title_h + t]
    }

    /// `Draw`: steps the open animation (+0.15 opening, -0.2 closing) and
    /// draws the panel in the window's set, faded by `1 - (1-open)^2`, and
    /// the title. True when fully open or fully closed.
    pub fn draw(&mut self, vui: &Vui, ui: &mut dyn Raster) -> bool {
        let fp = frame_sets();
        if self.dir > 0.0 {
            self.open = (self.open + 0.15).min(1.0);
        } else {
            self.open = if self.open - 0.2 < 0.0 { 0.0 } else { self.open - 0.2 };
        }
        let k = 1.0 - self.open;
        let k = 1.0 - k * k;
        let (ctr, size) = (self.center, self.size);
        let tl = [ctr[0] - (size[0] >> 1), ctr[1] - (size[1] >> 1)];
        let ipos = [tl[0] + 10, tl[1] + 10];
        let isize = [size[0] - 20, size[1] - 20];
        match self.x178 {
            0 => {
                vui.blit_fill_frame(ui, ipos, [isize[0] + 1, isize[1]], &fp[1], 35, 3, k, true);
                vui.blit_frame(ui, tl, size, &fp[0], 3, k, true);
            }
            1 => vui.blit_fill_frame(ui, ipos, isize, &fp[3], 61, 3, k * 0.75, true),
            2 => {
                vui.blit_rect(ui, 51, ipos, [isize[0] + 1, isize[1]], 3, k);
                vui.blit_frame(ui, tl, size, &fp[0], 3, k, true);
            }
            _ => {}
        }
        // the title, a captioned bar over the top edge
        if self.title.len > 0 {
            let t = self.title.chars();
            let tw = vui.string_width(ui, &t) + 50;
            vui.blit_bar(ui, &t, [1.0, 1.0, 1.0, k], [ctr[0] - (tw >> 1), ipos[1] - 21], 33, 34, tw, 3, k, 3, true);
        }
        self.open == 1.0 || self.open == 0.0
    }

    /// `Text(s, x, row, c, f)`: one row of `TextBox`, 640 wide. (It works
    /// out a faded colour and the row's position and uses neither: the
    /// caller's colour goes through, as shipped.)
    pub fn text(&self, vui: &Vui, ui: &mut dyn Raster, s: &[u16], x: i32, row: i32, c: [f32; 4], f: u32) {
        self.text_box(vui, ui, s, x, row, 640, 1, c, f);
    }

    /// `DrawColorBar(x, row, w, c)`: a solid bar six pixels short of a row.
    pub fn draw_color_bar(&self, vui: &Vui, ui: &mut dyn Raster, x: i32, row: i32, w: i32, c: [f32; 4]) {
        let h = vui.font_height(ui);
        let pos = [self.top_left()[0] + x, self.row_center(vui, ui, row)[1] - 4];
        vui.fill_rectangle(ui, [pos, [pos[0] + w, pos[1] + h - 6]], c);
    }

    /// `DrawVScroll(at, h, t, flags)`: the bar down `h` and the thumb at
    /// `t` (0 top, 1 bottom; none outside that).
    pub fn draw_vscroll(&self, vui: &Vui, ui: &mut dyn Raster, at: [i32; 2], h: i32, t: f32) {
        let e = vui.entry(84);
        vui.blit_vert_bar(ui, at, 81, 83, 82, h, 3, self.open);
        if (0.0..=1.0).contains(&t) {
            let p = [at[0] - 1, at[1] + (t * (h - e.uv1[1]) as f32) as i32];
            vui.blit_ex(ui, e.x00, p, 3, self.open, 0);
        }
    }

    /// `CountLines(s, w)`: the rows `s` wraps to at `w` (`TextBox`
    /// measuring, up to 256).
    pub fn count_lines(&self, vui: &Vui, ui: &mut dyn Raster, s: &[u16], w: i32) -> i32 {
        self.text_box(vui, ui, s, 0, 0, w, 256, [0.0, 0.0, 0.0, 1.0], 0xFFFF_FFFF)
    }

    /// `AppendEllipsis`: cut `s` until it and "..." fit `w`, then add it.
    fn append_ellipsis(&self, vui: &Vui, ui: &dyn Raster, s: &mut Vec<u16>, w: i32) {
        let ew = vui.string_width(ui, &wide("..."));
        if ew < w {
            while self.text_width(vui, ui, s) + ew >= w && !s.is_empty() {
                s.pop();
            }
            s.extend(wide("..."));
        }
    }

    /// `TextBox(s, x, row, w, rows, c, f)`: `s` wrapped to `w` pixels (at
    /// the last space, else the last character that fit) over at most
    /// `rows` rows from `row`; `f` places each row (8 centred, 0x10 right),
    /// 2 ends a cut-off last row in "...", 0x80 shows asterisks, and the
    /// top byte skips that many rows. 4 draws the frame round the rows
    /// first (`DrawFrame`, which reads 0x20 and 0x40 of the same flags),
    /// text or none: the spell book's ten slot plates are empty boxes.
    /// The rows drawn.
    pub fn text_box(&self, vui: &Vui, ui: &mut dyn Raster, s: &[u16], x: i32, row: i32, w: i32, rows: i32, c: [f32; 4], f: u32) -> i32 {
        let all = f == 0xFFFF_FFFF;
        if !all && f & 4 != 0 {
            self.draw_frame(vui, ui, x, row, w, rows, f);
        }
        let pos = [self.top_left()[0] + x, self.row_center(vui, ui, row)[1] - 4];
        let size_x = w;
        let copy = |from: usize, n: usize| -> Vec<u16> {
            if !all && f & 0x80 != 0 { vec![b'*' as u16; n.min(512)] } else { s[from..from + n.min(511)].to_vec() }
        };
        let skip = if all { 0 } else { (f >> 24) as i32 };
        let mut drawn = 0;
        let mut count = 0;
        let draw = |me: &Self, ui: &mut dyn Raster, buf: &[u16], drawn: &mut i32, count: i32| {
            if count >= skip {
                if !all {
                    let y = me.row_center(vui, ui, row + *drawn)[1];
                    let bx = if f & 8 != 0 {
                        pos[0] + (size_x >> 1) - (me.text_width(vui, ui, buf) >> 1)
                    } else if f & 0x10 != 0 {
                        pos[0] + size_x - me.text_width(vui, ui, buf)
                    } else {
                        pos[0]
                    };
                    me.draw_text_internal(vui, ui, buf, bx, y, c, f);
                }
                *drawn += 1;
            }
        };
        let (mut line, mut lastw, mut i, mut space, mut fit) = (0usize, 0, 0usize, 0usize, 0usize);
        while i < s.len() && s[i] != 0 {
            if s[i] == b'\n' as u16 {
                let mut buf = copy(line, i - line);
                if !all && f & 2 != 0 && drawn + 1 == rows && s.get(i + 1).is_some_and(|&c| c != 0) {
                    self.append_ellipsis(vui, ui, &mut buf, size_x);
                }
                draw(self, ui, &buf, &mut drawn, count);
                count += 1;
                i += 1;
                line = i;
                space = 0;
                fit = 0;
                if drawn >= rows {
                    break;
                }
            } else {
                let buf = copy(line, i - line);
                let pw = lastw;
                lastw = self.text_width(vui, ui, &buf);
                if lastw >= size_x - 8 {
                    let mut buf;
                    if space != 0 {
                        buf = copy(line, space + 1 - line);
                        i = space + 1;
                        line = space + 1;
                    } else {
                        buf = copy(line, fit - line);
                        line = fit;
                        i = fit;
                    }
                    if !all && f & 2 != 0 && drawn + 1 == rows && s.get(line).is_some_and(|&c| c != 0) {
                        self.append_ellipsis(vui, ui, &mut buf, size_x);
                    }
                    draw(self, ui, &buf, &mut drawn, count);
                    count += 1;
                    if drawn >= rows {
                        break;
                    }
                    space = 0;
                    fit = 0;
                } else {
                    if pw != lastw {
                        fit = i;
                    }
                    if s[i] == b' ' as u16 {
                        space = i;
                    }
                    i += 1;
                }
            }
        }
        let m = i - line;
        if m != 0 {
            let buf = copy(line, m);
            draw(self, ui, &buf, &mut drawn, count);
        }
        drawn
    }

    pub fn top_left(&self) -> [i32; 2] {
        [self.center[0] - (self.size[0] >> 1), self.center[1] - (self.size[1] >> 1)]
    }

    pub fn bottom_right(&self) -> [i32; 2] {
        [self.center[0] + (self.size[0] >> 1), self.center[1] + (self.size[1] >> 1)]
    }

    /// `TickPulse`: a 0..1 throb, scaled by how far the window is open.
    pub fn tick_pulse(&self, time: i64) -> f32 {
        let d = 1.0 - self.open;
        let d = 1.0 - d * d;
        let mut t = (time - self.pulse_time) as f32 * 0.01; // 0.00999999978f, 0.01's float
        if t > 6.283_185_5 {
            t -= 6.283_185_5;
        }
        d * ((t + 1.570_796_4).sin() * 0.5 + 0.5)
    }

    /// `TickR(x, row, pulse)`: the pointing arrow (atlas 89) left of a row,
    /// pulsing or steady.
    pub fn tick_r(&self, vui: &Vui, ui: &mut dyn Raster, x: i32, row: i32, pulse: bool) {
        let at = self.row_center(vui, ui, row);
        let at = [self.center[0] - (self.size[0] >> 1) + x - 16, at[1] - 5];
        vui.blit_ex(ui, 89, at, 3, if pulse { self.tick_pulse(vui.time) } else { 1.0 }, 0);
    }

    /// `TickU(x, row, pulse)`: the up tick (atlas 90) over a column.
    pub fn tick_u(&self, vui: &Vui, ui: &mut dyn Raster, x: i32, row: i32, pulse: bool) {
        let at = self.row_center(vui, ui, row);
        let at = [self.center[0] - (self.size[0] >> 1) + x - 8, at[1] - 5];
        vui.blit_ex(ui, 90, at, 3, if pulse { self.tick_pulse(vui.time) } else { 1.0 }, 0);
    }

    /// `TickD(x, row, pulse)`: the down tick (atlas 87).
    pub fn tick_d(&self, vui: &Vui, ui: &mut dyn Raster, x: i32, row: i32, pulse: bool) {
        let at = self.row_center(vui, ui, row);
        let at = [self.center[0] - (self.size[0] >> 1) + x - 8, at[1] - 5];
        vui.blit_ex(ui, 87, at, 3, if pulse { self.tick_pulse(vui.time) } else { 1.0 }, 0);
    }

    /// `DrawFrame(x, row, w, rows, flags)`: a frame round `rows` rows from
    /// `x`, over a faint black (0.1); 0x40 drops the bottom margin, 0x20
    /// adds the two pulsing side arrows (atlas 128).
    pub fn draw_frame(&self, vui: &Vui, ui: &mut dyn Raster, x: i32, row: i32, w: i32, rows: i32, flags: u32) {
        let h = vui.font_height(ui) - 4;
        let pos = [self.top_left()[0] + x, self.row_center(vui, ui, row)[1] - 4];
        let size = [w, rows * h];
        let at = [pos[0] - 4, pos[1] - 3];
        let sz = [size[0] + 8, if flags & 0x40 != 0 { size[1] } else { size[1] + 4 }];
        let fp = FrameParam { tl: (101, 0), top: (102, 0), tr: (103, 0), right: (104, 0), br: (106, 0), bottom: (102, 0), bl: (105, 0), left: (104, 0) };
        vui.stretch_blit_ex(ui, 61, at, sz, 3, self.open * 0.1);
        vui.blit_frame(ui, at, sz, &fp, 0, self.open, true);
        let e = vui.entry(128);
        if flags & 0x20 != 0 {
            let aw = e.uv1[0] + 3;
            let y = at[1] + (sz[1] >> 1) - (e.uv1[1] >> 1);
            let a = self.tick_pulse(vui.time);
            vui.blit_ex(ui, 128, [at[0] - aw, y], 3, a, 0);
            vui.blit_ex(ui, 128, [at[0] + sz[0] + 3, y], 3, a, 1);
        }
    }

    /// `DrawHScroll(x, row, w, t, flags)`: a slider bar `w` wide with its
    /// knob at `t`; 0x20 adds the two pulsing side arrows (atlas 128).
    pub fn draw_hscroll(&self, vui: &Vui, ui: &mut dyn Raster, x: i32, row: i32, w: i32, t: f32, flags: u32) {
        let mut pos = [self.top_left()[0] + x, self.row_center(vui, ui, row)[1] - 4];
        let size = [w, 16];
        vui.blit_bar(ui, &[], [0.0; 4], pos, 165, 164, size[0], 3, self.open, 0, true);
        let e = vui.entry(166);
        if flags & 0x20 != 0 {
            let a = vui.entry(128);
            let aw = a.uv1[0] + 3;
            let p = self.tick_pulse(vui.time);
            vui.blit_ex(ui, 128, [pos[0] - aw, pos[1] - 3], 3, p, 0);
            vui.blit_ex(ui, 128, [pos[0] + size[0] + 3, pos[1] - 3], 3, p, 1);
        }
        pos[0] += (t * (size[0] - e.uv1[0]) as f32) as i32;
        vui.blit_ex(ui, e.x00, pos, 3, self.open, 0);
    }

    /// `DrawFrameVScroll(x, row, w, rows, t)`: the frame with a scroll bar
    /// down its right edge, the thumb at `t` (none outside 0..1).
    pub fn draw_frame_vscroll(&self, vui: &Vui, ui: &mut dyn Raster, x: i32, row: i32, w: i32, rows: i32, t: f32) {
        let h = vui.font_height(ui) - 4;
        let pos = [self.top_left()[0] + x, self.row_center(vui, ui, row)[1] - 4];
        let size = [w, rows * h];
        let at = [pos[0] - 4, pos[1] - 3];
        let sz = [size[0] + 8, size[1] + 4];
        self.draw_frame(vui, ui, x, row, w, rows, 0);
        if (0.0..=1.0).contains(&t) {
            self.draw_vscroll(vui, ui, [at[0] + sz[0] - 4, at[1]], sz[1], t);
        }
    }

    /// `ResetTickPulse`: the pulse starts over now.
    pub fn reset_tick_pulse(&mut self, time: i64) {
        self.pulse_time = time;
    }

    /// `TextWidth`: the font's width, plus each inline icon's.
    pub fn text_width(&self, vui: &Vui, ui: &dyn Raster, s: &[u16]) -> i32 {
        if s.len() >= 1024 {
            return 0;
        }
        let mut buf = Vec::new();
        let mut extra = 0;
        let mut i = 0;
        while i < s.len() {
            if s[i] == b'|' as u16 {
                let Some(&n) = s.get(i + 1) else { break };
                if let Some(c) = char::from_u32(n as u32) {
                    match c {
                        'c' | 's' | 't' | 'x' => extra += 18,
                        '<' | '>' | 'e' | 'q' | 'r' | 'w' => extra += 16,
                        'i' | 'o' | 'u' | 'y' | '{' | '}' => extra += 12,
                        '|' => buf.push(b'|' as u16),
                        'A' | 'a' => extra += 27,
                        '&' | '(' | '*' | '7' | '8' | '9' => extra += 32,
                        'V' | 'Z' | 'v' | 'z' => extra += 17,
                        _ => {}
                    }
                }
                i += 1;
            } else {
                buf.push(s[i]);
            }
            i += 1;
        }
        vui.string_width(ui, &buf) + extra
    }

    /// `DrawTextInternal`: runs of text, centred vertically on `y`, with
    /// the inline icons (`|t`, `|s`, ...) blitted between them; `|-` dims
    /// what follows to 0.3, `|+` restores it.
    pub fn draw_text_internal(&self, vui: &Vui, ui: &mut dyn Raster, s: &[u16], mut x: i32, y: i32, c: [f32; 4], f: u32) {
        let pulse = self.tick_pulse(vui.time);
        let mut alpha = 1.0f32;
        let mut buf: Vec<u16> = Vec::new();
        let shadow = f & 1 != 0;
        let mut i = 0;
        while i < s.len() {
            if s[i] == b'|' as u16 {
                let Some(&n) = s.get(i + 1) else { break };
                if !buf.is_empty() {
                    vui.draw_centered_string_ex(ui, &buf, x, y, [c[0], c[1], c[2], c[3] * alpha], 1, shadow);
                    x += vui.string_width(ui, &buf);
                }
                buf.clear();
                let open = self.open * alpha;
                // (the set's two ids, their y offsets, the advance, alpha, flags)
                let icon = |a: i32, b: i32, dy: (i32, i32), adv: i32, al: f32, fl: u32| (a, b, dy, adv, al, fl);
                let pick = match char::from_u32(n as u32).unwrap_or('\0') {
                    't' => Some(icon(65, 69, (5, 5), 18, open, 0)),
                    's' => Some(icon(67, 71, (5, 5), 18, open, 0)),
                    'c' => Some(icon(68, 72, (5, 5), 18, open, 0)),
                    'x' => Some(icon(66, 70, (5, 5), 18, open, 0)),
                    '>' => Some(icon(89, 89, (6, 6), 16, open, 0)),
                    'q' => Some(icon(89, 89, (6, 6), 16, pulse * alpha, 0)),
                    'e' => Some(icon(89, 89, (6, 6), 16, (1.0 - pulse) * alpha, 0)),
                    '<' => Some(icon(88, 88, (6, 6), 16, open, 0)),
                    'w' => Some(icon(88, 88, (6, 6), 16, pulse * alpha, 0)),
                    'r' => Some(icon(88, 88, (6, 6), 16, (1.0 - pulse) * alpha, 0)),
                    '}' => Some(icon(128, 128, (6, 6), 12, open, 1)),
                    'y' => Some(icon(128, 128, (6, 6), 12, pulse * alpha, 1)),
                    'i' => Some(icon(128, 128, (6, 6), 12, (1.0 - pulse) * alpha, 1)),
                    '{' => Some(icon(128, 128, (6, 6), 12, open, 0)),
                    'u' => Some(icon(128, 128, (6, 6), 12, pulse * alpha, 0)),
                    'o' => Some(icon(128, 128, (6, 6), 12, (1.0 - pulse) * alpha, 0)),
                    'a' => Some(icon(190, 196, (4, 4), 27, open, 0)),
                    'A' => Some(icon(189, 195, (4, 4), 27, open, 0)),
                    '7' => Some(icon(185, 191, (6, 7), 32, open, 0)),
                    '&' => Some(icon(186, 192, (6, 7), 32, open, 0)),
                    '8' => Some(icon(187, 193, (6, 7), 32, open, 0)),
                    '*' => Some(icon(188, 194, (6, 7), 32, open, 0)),
                    '9' => Some(icon(214, 218, (6, 7), 32, open, 0)),
                    '(' => Some(icon(215, 219, (6, 7), 32, open, 0)),
                    'z' => Some(icon(212, 216, (5, 5), 17, open, 0)),
                    'Z' => Some(icon(212, 216, (5, 5), 17, open, 2)),
                    'v' => Some(icon(213, 217, (5, 5), 17, open, 0)),
                    'V' => Some(icon(213, 217, (5, 5), 17, open, 1)),
                    '-' => {
                        alpha = 0.3;
                        None
                    }
                    '+' => {
                        alpha = c[3];
                        None
                    }
                    '|' => {
                        buf.push(b'|' as u16);
                        None
                    }
                    _ => None,
                };
                if let Some((a, b, (dy0, dy1), adv, al, fl)) = pick {
                    // The pulsing arrows and the item marks draw whatever
                    // the window's set; the rest only for sets 0, 1, 2.
                    let same = a == b;
                    if same || self.x178 == 0 {
                        vui.blit_ex(ui, a, [x, y - dy0], 3, al, fl);
                    } else if self.x178 == 1 || self.x178 == 2 {
                        vui.blit_ex(ui, b, [x, y - dy1], 3, al, fl);
                    }
                    x += adv;
                }
                i += 1;
            } else {
                buf.push(if s[i] == b'\r' as u16 { b' ' as u16 } else { s[i] });
            }
            i += 1;
        }
        if !buf.is_empty() {
            vui.draw_centered_string_ex(ui, &buf, x, y, [c[0], c[1], c[2], c[3] * alpha], 1, shadow);
        }
    }
}

/// `VIUITextScroll`: one row of text that, wider than its window, scrolls
/// (style 0 right to left and round, style 1 to and fro with a 30-frame
/// pause at each end), cut to the window.
#[derive(Clone, Copy, Debug)]
pub struct TextScroll {
    pub window: Window,
    pub style: i32,
    pub offset: i32,
    pub dir: i32,
    pub pause: i32,
    pub width: i32,
    pub on: i32,
}

impl Default for TextScroll {
    fn default() -> Self {
        TextScroll { window: Window::default(), style: 1, offset: 0, dir: 1, pause: 30, width: 0, on: 1 }
    }
}

impl TextScroll {
    /// `Init(ui, at, w, x, style)`.
    pub fn init(&mut self, vui: &Vui, ui: &dyn Raster, at: [i32; 2], w: i32, x: i32, style: i32) {
        self.style = style;
        self.width = w;
        self.window.init(vui, ui, at, w, 1, x);
    }

    /// `Reset`.
    pub fn reset(&mut self) {
        self.dir = 1;
        self.pause = 30;
        self.offset = 0;
    }

    /// `TextScroll(s, c, f)`: one frame of it.
    pub fn text_scroll(&mut self, vui: &Vui, ui: &mut dyn Raster, s: &[u16], c: [f32; 4], f: u32) {
        let y = self.window.center[1] - 2;
        let tw = self.window.text_width(vui, ui, s);
        let saved = ui.viewport();
        if self.width < tw {
            match self.style {
                0 => {
                    if self.on != 0 || self.offset != self.width {
                        self.offset -= 2;
                    }
                    if self.offset + tw < 0 || self.width < self.offset {
                        self.offset = self.width;
                    }
                }
                1 => {
                    if self.width >= self.offset + tw {
                        self.pause -= 1;
                        if self.pause > 0 {
                            self.dir = 0;
                        } else {
                            self.dir = 1;
                            self.pause = 30;
                        }
                    } else if self.offset >= 0 {
                        self.pause -= 1;
                        if self.on == 0 {
                            self.dir = 0;
                            self.pause = 30;
                        } else if self.pause <= 0 {
                            self.dir = -1;
                            self.pause = 30;
                        } else {
                            self.dir = 0;
                        }
                    }
                    if self.dir > 0 {
                        self.offset += 2;
                    } else if self.dir < 0 {
                        self.offset -= 2;
                    }
                    self.offset = (self.width - tw).max(self.offset).min(0);
                }
                _ => {}
            }
            let (tl, br) = (self.window.top_left(), self.window.bottom_right());
            ui.set_viewport([tl[0], tl[1], br[0], br[1]]);
            let mut x = 0;
            for piece in s.chunks(64) {
                self.window.draw_text_internal(vui, ui, piece, self.offset + x, 23, c, f);
                x += self.window.text_width(vui, ui, piece);
            }
            ui.set_viewport(saved);
            return;
        }
        self.reset();
        let tl = self.window.top_left();
        let x = if f & 8 != 0 {
            self.window.center[0] - (tw >> 1)
        } else if f & 0x10 != 0 {
            tl[0] + self.width - tw
        } else {
            tl[0]
        };
        self.window.draw_text_internal(vui, ui, s, x, y, c, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_codes() {
        let ctx = ExpandContext { player_name: "Dab".into(), level: 7, class_name: "Cleric".into(), target_name: "Vaal".into() };
        let e = |s: &str, f| String::from_utf16_lossy(&expand(&wide(s), 218, f, &ctx));
        assert_eq!(e("Hail, %p! Press %x to go. 100%%", true), "Hail, Dab! Press |x to go. 100%");
        assert_eq!(e("%7 %& %8 %* %9 %( %l%r%u%d%a%A", true), "|7 |& |8 |* |9 |( |v|V|z|Z|a|A");
        // A letter (or % & * () after the digits replaces the number.
        assert_eq!(e("%7%&", true), "%&");
        assert_eq!(e("%12x", true), "|x");
        assert_eq!(e("%1 the level %2 %3 sees %4.", false), "Dab the level 7 Cleric sees Vaal.");
        // Unknown and empty tokens, and a number token under the flag.
        assert_eq!(e("%q %! %5", true), "(?) (?)! (?)");
        assert_eq!(e("%z", false), "(?)");
        // A piece that does not fit stops the copy (size counts the NUL).
        assert_eq!(String::from_utf16_lossy(&expand(&wide("abc%pdef"), 6, true, &ctx)), "abc");
        assert_eq!(String::from_utf16_lossy(&expand(&wide("abcdefgh"), 5, true, &ctx)), "abcd");
    }
}
