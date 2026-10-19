//! `vi::ui`'s `Raster` on the renderer's 2D layer: UI.CSF's surfaces by
//! DictID, "ArialUI" as a packed font, and each call made into quads as
//! `VIRaster` makes it (a blit's texel rectangle over the surface's size,
//! mirrored by its flags; a string's glyphs one pixel apart; the viewport
//! as the GS's offset and scissor).

use render::ui::{Quad, Ui};
use render::wgpu;
use std::collections::HashMap;
pub use vi::ui::{Raster, Vui, icon_id, wide};

/// What the layer holds for the UI: surfaces by DictID, and the font.
#[derive(Default)]
pub struct UiAssets {
    surfaces: HashMap<u32, usize>,
    pub font: Option<usize>,
    /// A white texel, for `FillRectangle`'s solid fill.
    pub white: Option<usize>,
}

impl UiAssets {
    /// Every surface of a UI file into the layer, by DictID.
    pub fn add_surfaces(&mut self, ui: &mut Ui, device: &wgpu::Device, queue: &wgpu::Queue, c: &esf::Container) {
        for s in &c.surfaces {
            if self.surfaces.contains_key(&(s.dict_id as u32)) {
                continue;
            }
            if let Some(img) = s.decode(&c.data) {
                let t = ui.add_texture(device, queue, img.width, img.height, &img.rgba);
                self.surfaces.insert(s.dict_id as u32, t);
            }
        }
    }

    /// The white texel solid fills draw with.
    pub fn add_white(&mut self, ui: &mut Ui, device: &wgpu::Device, queue: &wgpu::Queue) {
        self.white = Some(ui.add_texture(device, queue, 1, 1, &[255; 4]));
    }

    /// The layer texture of a surface DictID (`m_dict->Find`).
    pub fn surface(&self, id: u32) -> Option<usize> {
        self.surfaces.get(&id).copied()
    }

    pub fn raster<'a>(&'a self, ui: &'a mut Ui) -> UiRaster<'a> {
        UiRaster { assets: self, ui }
    }
}

pub struct UiRaster<'a> {
    pub assets: &'a UiAssets,
    pub ui: &'a mut Ui,
}

impl UiRaster<'_> {
    fn text(&mut self, at: [i32; 2], color: [f32; 4], s: &[u16]) {
        if let Some(f) = self.assets.font {
            self.ui.text(f, at[0], at[1], color, s);
        }
    }
}

impl Raster for UiRaster<'_> {
    fn blit(&mut self, id: u32, flags: u32, at: [i32; 2], a: [i32; 2], b: [i32; 2], mode: i32, alpha: f32) {
        let Some(tex) = self.assets.surface(id) else { return };
        let (w, h) = self.ui.texture_size(tex);
        let (iw, ih) = (1.0 / w as f32, 1.0 / h as f32);
        let (x0, y0) = (at[0] as f32, at[1] as f32);
        let (x1, y1) = (x0 + b[0] as f32, y0 + b[1] as f32);
        let (u0, v0) = (a[0] as f32 * iw, a[1] as f32 * ih);
        let (u1, v1) = ((a[0] + b[0]) as f32 * iw, (a[1] + b[1]) as f32 * ih);
        let uv = match flags & 3 {
            0 => [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
            1 => [[u1, v0], [u0, v0], [u0, v1], [u1, v1]],
            2 => [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
            _ => [[u1, v1], [u0, v1], [u0, v0], [u1, v0]],
        };
        self.ui.push(Quad { tex, blend: mode as u8, pos: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], uv, color: [1.0, 1.0, 1.0, alpha], clip: None });
    }

    fn stretch_blit(&mut self, id: u32, flags: u32, at: [i32; 2], size: [i32; 2], a: [i32; 2], b: [i32; 2], mode: i32, alpha: f32) {
        let Some(tex) = self.assets.surface(id) else { return };
        let (w, h) = self.ui.texture_size(tex);
        let (iw, ih) = (1.0 / w as f32, 1.0 / h as f32);
        let (x0, y0) = (at[0] as f32, at[1] as f32);
        let (x1, y1) = (x0 + size[0] as f32, y0 + size[1] as f32);
        let (u0, v0) = (a[0] as f32 * iw, a[1] as f32 * ih);
        let (u1, v1) = ((a[0] + b[0]) as f32 * iw, (a[1] + b[1]) as f32 * ih);
        let uv = match flags & 3 {
            0 => [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
            1 => [[u1, v0], [u0, v0], [u0, v1], [u1, v1]],
            2 => [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
            _ => [[u1, v1], [u0, v1], [u0, v0], [u1, v0]],
        };
        self.ui.push(Quad { tex, blend: mode as u8, pos: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], uv, color: [1.0, 1.0, 1.0, alpha], clip: None });
    }

    fn fill_fan(&mut self, color: [f32; 4], c: [[i32; 2]; 4]) {
        let Some(tex) = self.assets.white else { return };
        let pos = c.map(|p| [p[0] as f32, p[1] as f32]);
        // m_solidFillUIMaterial: InitSolidFill's blend 0 (opaque)
        self.ui.push(Quad { tex, blend: 0, pos, uv: [[0.5, 0.5]; 4], color, clip: None });
    }

    fn blit_full(&mut self, id: u32, flags: u32, at: [i32; 2], mode: i32, alpha: f32) {
        let Some(tex) = self.assets.surface(id) else { return };
        let (w, h) = self.ui.texture_size(tex);
        self.blit(id, flags, at, [0, 0], [w as i32, h as i32], mode, alpha);
    }

    fn draw_string(&mut self, at: [i32; 2], color: [f32; 4], s: &[u16]) {
        self.text(at, color, s);
    }

    fn draw_centered_string(&mut self, at: [i32; 2], color: [f32; 4], s: &[u16], mode: u32) {
        let w = self.string_width(s);
        if w < 0 {
            return;
        }
        let h = self.point_size();
        let at = match mode {
            1 => [at[0], at[1] - (h >> 1)],
            2 => [at[0] - (w >> 1), at[1]],
            0 | 3 => [at[0] - (w >> 1), at[1] - (h >> 1)],
            _ => return,
        };
        self.text(at, color, s);
    }

    fn string_width(&self, s: &[u16]) -> i32 {
        self.assets.font.map_or(0, |f| self.ui.fonts[f].string_width(s))
    }

    fn point_size(&self) -> i32 {
        self.assets.font.map_or(0, |f| self.ui.fonts[f].height as i32)
    }

    fn viewport(&self) -> [i32; 4] {
        self.ui.viewport.unwrap_or([0, 0, 639, 447])
    }

    fn set_viewport(&mut self, v: [i32; 4]) {
        self.ui.viewport = Some(v);
    }
}
