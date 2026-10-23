//! `vi::ui`'s `Raster` on the renderer's 2D layer: UI.CSF's surfaces by
//! DictID, "ArialUI" as a packed font, and each call made into quads as
//! `VIRaster` makes it (a blit's texel rectangle over the surface's size,
//! mirrored by its flags; a string's glyphs one pixel apart; the viewport
//! as the GS's offset and scissor).

use render::ui::{Anchor, Quad, Ui};
use render::wgpu;
use std::collections::HashMap;
pub use vi::ui::{Raster, Vui, icon_id, wide};

/// A raster a window lays itself out on across a window wider than 4:3
/// (the client's own: the game drew for a 4:3 picture): how far the target
/// reaches past the frame's sides, and where the quads made next go
/// (`render::ui::Anchor`). A 4:3 target, or none, reaches 0 past them, and
/// the window is drawn as the game draws it.
pub trait Layout: Raster {
    /// The whole frame pixels the target shows beyond each side of the
    /// frame (0 on a 4:3 target).
    fn side(&self) -> i32 {
        0
    }
    /// Where the quads made from now go.
    fn anchor(&mut self, _a: Anchor) {}
    /// A window's 3D goes here (`pregame::Scene`): the quads made so far
    /// are drawn under it, the rest over it.
    fn mark_split(&mut self) {}
    /// A solid quad at float corners (fan order) in blend `mode`: what a
    /// window draws through a solid-fill material of its own (`VIFader`'s
    /// black, blended, where `FillRectangle`'s material is opaque).
    fn solid(&mut self, _mode: i32, _color: [f32; 4], _corners: [[f32; 2]; 4]) {}
    /// Surface `id` on a quad at float corners (fan order), `uv` over the
    /// whole surface (0..1), in blend `mode`: what a window draws through a
    /// material of its own (the title's bobbing island, `VISplashFx`).
    fn textured(&mut self, _id: u32, _mode: i32, _color: [f32; 4], _corners: [[f32; 2]; 4], _uv: [[f32; 2]; 4]) {}
    /// A solid fill in a material of its own that blends by the colour's
    /// alpha (STATION's `VIGUIObject::FillRect` and `VIFader` make theirs
    /// with `SetLayerBlendMode(0, 3)`), where `fill_fan`'s, the UI's solid
    /// fill, is opaque.
    fn fill_blended(&mut self, color: [f32; 4], corners: [[i32; 2]; 4]) {
        self.fill_fan(color, corners)
    }
    /// Surface `id`'s texels `texels` (fan order, in the surface's own
    /// pixels) on a quad at frame corners `corners` past the frame's sides,
    /// in blend `mode`, cut only to the target: what the client lays beside
    /// a full-screen picture in a wider window (`pregame::wide`).
    fn beyond(&mut self, _id: u32, _mode: i32, _corners: [[f32; 2]; 4], _texels: [[f32; 2]; 4]) {}
}

/// A raster that lays nothing at or below frame line `below` (the client's
/// own): each blit made through it loses the rows of its quad that would
/// fall there, with the texels drawn on them (the texture's last rows, or
/// its first under a vertical mirror), its place and its other rows as made;
/// a fill is cut there too; the rest passes on as it is. A window's frame is
/// drawn through one to end at its bottom rail's bar
/// (`startmenu::RAIL_BAR`).
pub struct Cut<'a> {
    r: &'a mut dyn Layout,
    below: i32,
}

impl<'a> Cut<'a> {
    pub fn new(r: &'a mut dyn Layout, below: i32) -> Self {
        Cut { r, below }
    }

    /// How many of the `h` rows from line `y` fall at or below the cut.
    fn over(&self, y: i32, h: i32) -> i32 {
        (y + h - self.below).clamp(0, h.max(0))
    }

    fn corners(&self, c: [[i32; 2]; 4]) -> [[i32; 2]; 4] {
        c.map(|p| [p[0], p[1].min(self.below)])
    }
}

impl Layout for Cut<'_> {
    fn side(&self) -> i32 {
        self.r.side()
    }
    fn anchor(&mut self, a: Anchor) {
        self.r.anchor(a);
    }
    fn mark_split(&mut self) {
        self.r.mark_split();
    }
    fn solid(&mut self, mode: i32, color: [f32; 4], corners: [[f32; 2]; 4]) {
        self.r.solid(mode, color, corners);
    }
    fn textured(&mut self, id: u32, mode: i32, color: [f32; 4], corners: [[f32; 2]; 4], uv: [[f32; 2]; 4]) {
        self.r.textured(id, mode, color, corners, uv);
    }
    fn fill_blended(&mut self, color: [f32; 4], corners: [[i32; 2]; 4]) {
        let c = self.corners(corners);
        self.r.fill_blended(color, c);
    }
    fn beyond(&mut self, id: u32, mode: i32, corners: [[f32; 2]; 4], texels: [[f32; 2]; 4]) {
        self.r.beyond(id, mode, corners, texels);
    }
}

impl Raster for Cut<'_> {
    fn blit(&mut self, id: u32, flags: u32, at: [i32; 2], a: [i32; 2], b: [i32; 2], mode: i32, alpha: f32) {
        let over = self.over(at[1], b[1]);
        if over > 0 && over == b[1] {
            return;
        }
        let a = if flags & 2 != 0 { [a[0], a[1] + over] } else { a };
        self.r.blit(id, flags, at, a, [b[0], b[1] - over], mode, alpha);
    }
    fn stretch_blit(&mut self, id: u32, flags: u32, at: [i32; 2], size: [i32; 2], a: [i32; 2], b: [i32; 2], mode: i32, alpha: f32) {
        let over = self.over(at[1], size[1]);
        if over > 0 && over == size[1] {
            return;
        }
        // the texel rows on the rows cut, in proportion
        let t = if over == 0 { 0 } else { (b[1] as i64 * over as i64 / size[1] as i64) as i32 };
        let a = if flags & 2 != 0 { [a[0], a[1] + t] } else { a };
        self.r.stretch_blit(id, flags, at, [size[0], size[1] - over], a, [b[0], b[1] - t], mode, alpha);
    }
    fn fill_fan(&mut self, color: [f32; 4], corners: [[i32; 2]; 4]) {
        let c = self.corners(corners);
        self.r.fill_fan(color, c);
    }
    fn blit_full(&mut self, id: u32, flags: u32, at: [i32; 2], mode: i32, alpha: f32) {
        self.r.blit_full(id, flags, at, mode, alpha);
    }
    fn draw_string(&mut self, at: [i32; 2], color: [f32; 4], s: &[u16]) {
        self.r.draw_string(at, color, s);
    }
    fn draw_centered_string(&mut self, at: [i32; 2], color: [f32; 4], s: &[u16], mode: u32) {
        self.r.draw_centered_string(at, color, s, mode);
    }
    fn string_width(&self, s: &[u16]) -> i32 {
        self.r.string_width(s)
    }
    fn point_size(&self) -> i32 {
        self.r.point_size()
    }
    fn viewport(&self) -> [i32; 4] {
        self.r.viewport()
    }
    fn set_viewport(&mut self, v: [i32; 4]) {
        self.r.set_viewport(v);
    }
    fn has(&self, id: u32) -> bool {
        self.r.has(id)
    }
}

/// What the layer holds for the UI: surfaces by DictID, and the font.
#[derive(Default)]
pub struct UiAssets {
    surfaces: HashMap<u32, usize>,
    pub font: Option<usize>,
    /// A white texel, for `FillRectangle`'s solid fill.
    pub white: Option<usize>,
    /// The atlas's button glyphs, once SUPPORT's atlas is here: the raster
    /// notes where each is drawn (`pointer::Glyphs`).
    pub glyphs: Option<crate::pointer::GlyphTable>,
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

    /// One surface of a file into the layer, by its DictID (what the HUD
    /// draws from a file that is not the UI's: FX.CSF's damage font).
    pub fn add_surface(&mut self, ui: &mut Ui, device: &wgpu::Device, queue: &wgpu::Queue, c: &esf::Container, id: u32) {
        if self.surfaces.contains_key(&id) {
            return;
        }
        if let Some(img) = c.surfaces.iter().find(|s| s.dict_id as u32 == id).and_then(|s| s.decode(&c.data)) {
            let t = ui.add_texture(device, queue, img.width, img.height, &img.rgba);
            self.surfaces.insert(id, t);
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
        UiRaster { assets: self, ui, glyphs: Default::default() }
    }
}

pub struct UiRaster<'a> {
    pub assets: &'a UiAssets,
    pub ui: &'a mut Ui,
    /// The button glyphs drawn, where (for a pointer).
    pub glyphs: crate::pointer::Glyphs,
}

impl UiRaster<'_> {
    fn text(&mut self, at: [i32; 2], color: [f32; 4], s: &[u16]) {
        if let Some(f) = self.assets.font {
            self.ui.text(f, at[0], at[1], color, s);
            if color[3] > 0.0 {
                let (w, h) = (self.string_width(s).max(0), self.point_size());
                let (a, b) = (self.ui.placed([at[0] as f32, at[1] as f32]), self.ui.placed([(at[0] + w) as f32, (at[1] + h) as f32]));
                self.glyphs.string([a[0], a[1], b[0], b[1]]);
            }
        }
    }

    /// A blit's glyph noted (a button glyph at its own size, not wholly
    /// faded: a pulsing arrow stays a button at its faintest), or
    /// that something else was blitted.
    fn note(&mut self, id: u32, flags: u32, at: [i32; 2], a: [i32; 2], b: [i32; 2], alpha: f32) {
        match self.assets.glyphs.as_ref().and_then(|t| t.key(id, a, b, flags)).filter(|_| alpha > 0.0) {
            Some(key) => {
                let (p, q) = (self.ui.placed([at[0] as f32, at[1] as f32]), self.ui.placed([(at[0] + b[0]) as f32, (at[1] + b[1]) as f32]));
                self.glyphs.glyph([p[0], p[1], q[0], q[1]], key);
            }
            None => self.glyphs.other(),
        }
    }
}

impl Layout for UiRaster<'_> {
    fn side(&self) -> i32 {
        self.ui.side() as i32
    }
    fn anchor(&mut self, a: Anchor) {
        self.ui.anchor = a;
    }
    fn mark_split(&mut self) {
        self.ui.mark_split();
    }
    fn solid(&mut self, mode: i32, color: [f32; 4], corners: [[f32; 2]; 4]) {
        let Some(tex) = self.assets.white else { return };
        self.ui.push(Quad { tex, blend: mode as u8, pos: corners, uv: [[0.5, 0.5]; 4], color, clip: None });
        self.glyphs.other();
    }
    fn textured(&mut self, id: u32, mode: i32, color: [f32; 4], corners: [[f32; 2]; 4], uv: [[f32; 2]; 4]) {
        let Some(tex) = self.assets.surface(id) else { return };
        self.ui.push(Quad { tex, blend: mode as u8, pos: corners, uv, color, clip: None });
        self.glyphs.other();
    }
    fn fill_blended(&mut self, color: [f32; 4], c: [[i32; 2]; 4]) {
        let Some(tex) = self.assets.white else { return };
        let pos = c.map(|p| [p[0] as f32, p[1] as f32]);
        self.ui.push(Quad { tex, blend: 3, pos, uv: [[0.5, 0.5]; 4], color, clip: None });
        self.glyphs.other();
    }
    fn beyond(&mut self, id: u32, mode: i32, corners: [[f32; 2]; 4], texels: [[f32; 2]; 4]) {
        let Some(tex) = self.assets.surface(id) else { return };
        let (w, h) = self.ui.texture_size(tex);
        let uv = texels.map(|t| [t[0] / w as f32, t[1] / h as f32]);
        self.ui.push_whole(Quad { tex, blend: mode as u8, pos: corners, uv, color: [1.0; 4], clip: None });
        self.glyphs.other();
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
        self.note(id, flags, at, a, b, alpha);
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
        self.glyphs.other();
    }

    fn fill_fan(&mut self, color: [f32; 4], c: [[i32; 2]; 4]) {
        let Some(tex) = self.assets.white else { return };
        let pos = c.map(|p| [p[0] as f32, p[1] as f32]);
        // m_solidFillUIMaterial: InitSolidFill's blend 0 (opaque)
        self.ui.push(Quad { tex, blend: 0, pos, uv: [[0.5, 0.5]; 4], color, clip: None });
        self.glyphs.other();
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
        // the whole screen is none: what `viewport` answers for none, set
        // back, leaves a frame-wide backdrop free to run the target's width
        self.ui.viewport = render::ui::viewport_of(v);
    }

    fn has(&self, id: u32) -> bool {
        self.assets.surface(id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layer on the machine's GPU (any backend), as a window lays itself out
    /// on it; None without an adapter.
    fn layer() -> Option<Ui> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        let (device, _queue) = pollster::block_on(adapter.request_device(&Default::default())).ok()?;
        Some(Ui::new(&device, render::TARGET_FORMAT, render::DEPTH_FORMAT))
    }

    /// A name scrolled (`VIUITextScroll`: the viewport saved, cut to its
    /// window, set back) leaves the layer with no viewport, and a frame starts
    /// with none: a full-screen window's backdrop made after it runs a wide
    /// window's whole width. F3 2026-10-21 (Class Mastery, Factions, the
    /// tradeskills): the scroll left the whole screen set as a viewport, kept
    /// from frame to frame, and every backdrop stopped at the 4:3 frame's
    /// sides. Needs a graphics adapter; without one it says so and passes.
    #[test]
    fn a_scrolled_name_leaves_no_viewport_behind() {
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let Some(mut ui) = layer() else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let assets = UiAssets::default();
        ui.begin((1920, 1080));
        {
            let mut r = assets.raster(&mut ui);
            let saved = r.viewport();
            assert_eq!(saved, [0, 0, 639, 447], "none answers the whole screen");
            r.set_viewport([430, 25, 600, 50]);
            assert_eq!(r.viewport(), [430, 25, 600, 50]);
            r.set_viewport(saved);
        }
        assert_eq!(ui.viewport, None, "set back, the whole screen is none");
        // a viewport left over from the frame before is gone at the next
        ui.viewport = Some([430, 25, 600, 50]);
        ui.begin((1920, 1080));
        assert_eq!(ui.viewport, None);
    }

    /// A call a raster was asked: (call, flags, at, size, texels from,
    /// texels big), a fill's corners as (`fill`, 0, top-left, bottom-right,
    /// ..).
    type Call = (&'static str, u32, [i32; 2], [i32; 2], [i32; 2], [i32; 2]);

    /// What a raster was asked, and the anchor it was left at.
    #[derive(Default)]
    struct Calls(Vec<Call>, Anchor);

    impl Layout for Calls {
        fn side(&self) -> i32 {
            106
        }
        fn anchor(&mut self, a: Anchor) {
            self.1 = a;
        }
    }

    impl Raster for Calls {
        fn blit(&mut self, _: u32, flags: u32, at: [i32; 2], a: [i32; 2], b: [i32; 2], _: i32, _: f32) {
            self.0.push(("blit", flags, at, b, a, b));
        }
        fn stretch_blit(&mut self, _: u32, flags: u32, at: [i32; 2], size: [i32; 2], a: [i32; 2], b: [i32; 2], _: i32, _: f32) {
            self.0.push(("stretch", flags, at, size, a, b));
        }
        fn fill_fan(&mut self, _: [f32; 4], c: [[i32; 2]; 4]) {
            self.0.push(("fill", 0, c[0], c[2], [0, 0], [0, 0]));
        }
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
        fn string_width(&self, _: &[u16]) -> i32 {
            0
        }
        fn point_size(&self) -> i32 {
            16
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 639, 447]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    /// The same calls on `r`: the bottom rail's bar on line 313 (atlas 40's
    /// 18 rows from texel row 11), its corner mirrored both ways, a panel
    /// above the cut, a piece wholly under it, a stretched one across it,
    /// and a fill across it.
    fn frame(r: &mut dyn Layout) {
        r.anchor(Anchor::Frame);
        r.blit(1, 0, [-1, 313], [0, 11], [177, 18], 0, 1.0);
        r.blit(2, 3, [440, 313], [107, 293], [11, 18], 0, 1.0);
        r.blit(3, 0, [-1, 48], [0, 0], [176, 265], 0, 1.0);
        r.blit(4, 0, [16, 330], [0, 0], [64, 8], 0, 1.0);
        r.stretch_blit(5, 0, [0, 319], [10, 12], [0, 0], [10, 24], 0, 1.0);
        r.fill_fan([1.0; 4], [[0, 300], [10, 300], [10, 340], [0, 340]]);
    }

    /// A raster cut at a line loses, of each blit across it, the rows under
    /// it with their texels (a texture's last rows, a vertically mirrored
    /// one's first), keeps its place and the rest, drops a blit wholly under
    /// it, cuts a stretched blit's texels in proportion and a fill at the
    /// line, and passes the anchor on. The control: the same calls on the
    /// raster itself reach line 331.
    #[test]
    fn a_cut_raster_draws_nothing_below_its_line() {
        let mut cut = Calls::default();
        frame(&mut Cut::new(&mut cut, 325));
        assert_eq!(cut.1, Anchor::Frame, "the anchor passed on");
        assert_eq!(
            cut.0,
            vec![
                ("blit", 0, [-1, 313], [177, 12], [0, 11], [177, 12]),
                ("blit", 3, [440, 313], [11, 12], [107, 299], [11, 12]),
                ("blit", 0, [-1, 48], [176, 265], [0, 0], [176, 265]),
                ("stretch", 0, [0, 319], [10, 6], [0, 0], [10, 12]),
                ("fill", 0, [0, 300], [10, 325], [0, 0], [0, 0]),
            ]
        );
        assert!(cut.0.iter().filter(|c| c.0 != "fill").all(|c| c.2[1] + c.3[1] <= 325), "nothing at or below line 325");
        let mut whole = Calls::default();
        frame(&mut whole);
        assert!(whole.0.iter().any(|c| c.2[1] + c.3[1] == 331), "the control: uncut, the rail reaches 331");
        assert_eq!(whole.0.len(), 6, "the control: the piece under the line drawn");
    }
}

/// The backdrop a full-screen window puts under itself: `BlitRect` of atlas
/// entry 32 over the frame's top 332 lines, `width` wide (642; 641 in Class
/// Mastery, and blended (mode 3, at full alpha) in the buddy list and the
/// mail's compose and inbox), which on a TV hides the world behind the
/// window. The client leaves it out, so the world shows round the window
/// (the user's choice, F3 20261021-043455, once the wide fix had run it to
/// the window's sides; the blended three until 2026-10-23, their backdrop
/// across a wide window and the compass under it); the start menu keeps its
/// own band, and the strip under a fixed chat box goes too (`chat_strip`).
pub fn window_backdrop(_vui: &vi::ui::Vui, _r: &mut dyn vi::ui::Raster, _width: i32) {}

/// The strip under the chat box while a window fixes it (`VIWndChat::
/// OnDrawSelf`'s `BlitEx` of atlas entry 32 at (200, 331), 442 by 118, with
/// `m_mode` set) and `VIWndGame`'s left piece of it under the target (at
/// (-1, 331), 203 by 125): the lower part of the backdrop that the merchant,
/// the bank, the auction and a trade cover the screen with on a TV. The
/// client leaves it out with that backdrop (`window_backdrop`), so the world
/// shows under the chat box and the target there too (the user, F3
/// 20261023-080729: "seeing green background bottom part of merchant
/// screen"); the chat box keeps its own box.
pub fn chat_strip(_vui: &vi::ui::Vui, _r: &mut dyn vi::ui::Raster, _at: [i32; 2], _size: [i32; 2]) {}
