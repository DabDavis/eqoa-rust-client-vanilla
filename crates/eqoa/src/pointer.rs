//! A pointer (the mouse, a finger) on the game's windows. The PS2 game reads
//! neither, so this is the client's own, and a window takes a pointer as the
//! keys it already takes: nothing a pad could not do.
//!
//! - every button glyph on the screen is that button: the legends' "✕
//!   Select  △ Exit", the shoulder buttons' and Start's. The HUD's raster
//!   notes where each was drawn, with the words drawn after it (`Glyphs`),
//!   and a click there presses it. (The side arrow, atlas 128, is not one:
//!   it points out from a slider, in at the trainer's value, between two
//!   stat values and round a sort key, so the windows whose values change
//!   with left and right take a click's side of the value themselves);
//! - a window notes where its rows, tabs and fields were drawn (`Hits`); a
//!   click on one moves the window's selection there, as the d-pad would,
//!   and a click on the selection already there presses ✕. A menu of
//!   actions (the start menu's bars, a popup, a dialog's options) takes a
//!   click at once;
//! - a right click is △, the wheel the d-pad's up and down.

use std::collections::HashMap;
use vi::ui::{Raster, Vui, Window};

/// The pad's keys as `SetKey` takes them: 128 + the `VIConsolePad` field.
pub mod key {
    pub const TRIANGLE: i32 = 132;
    pub const SQUARE: i32 = 133;
    pub const CROSS: i32 = 134;
    pub const CIRCLE: i32 = 135;
    pub const LEFT: i32 = 136;
    pub const RIGHT: i32 = 137;
    pub const UP: i32 = 138;
    pub const DOWN: i32 = 139;
    pub const L1: i32 = 140;
    pub const L2: i32 = 141;
    pub const R1: i32 = 142;
    pub const R2: i32 = 143;
    pub const START: i32 = 144;
    pub const SELECT: i32 = 145;
    pub const L3: i32 = 146;
    pub const R3: i32 = 147;
}

/// Not a key: a window's selection moved under the pointer, which sounds
/// as the d-pad's move does (sound 11).
pub const MOVED: i32 = 0;

/// The keys that step a selection from `from` to `to`: `less` or `more`,
/// once a step (a tab, a page).
pub fn steps(from: i32, to: i32, less: i32, more: i32) -> Vec<i32> {
    vec![if to < from { less } else { more }; (to - from).unsigned_abs() as usize]
}

/// How far (frame pixels) a finger may miss what it meant: a fingertip
/// covers a few of them on a phone, a mouse's tip none.
pub const FINGER: f32 = 6.0;

fn inside(r: [f32; 4], p: [f32; 2], slop: f32) -> bool {
    p[0] >= r[0] - slop && p[0] < r[2] + slop && p[1] >= r[1] - slop && p[1] < r[3] + slop
}

/// Where a window's pickable things were drawn last, as frame rectangles
/// (x0, y0, x1, y1), each with what it is to the window. The last drawn is
/// on top.
#[derive(Clone, Debug, Default)]
pub struct Hits(Vec<([i32; 4], i32)>);

impl Hits {
    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn push(&mut self, r: [i32; 4], what: i32) {
        self.0.push((r, what));
    }

    /// What is under frame point `p`: the last drawn there.
    pub fn at(&self, p: [f32; 2]) -> Option<i32> {
        self.0.iter().rev().find(|(r, _)| inside(r.map(|v| v as f32), p, 0.0)).map(|h| h.1)
    }
}

/// Rows `row` .. `row + rows` of window `w` from `x`, `width` wide, as
/// `TextBox` draws them (each centred on its `RowCenter`, a font height less
/// 4 apart), with room for the tick left of them: a frame rectangle.
pub fn rows_rect(w: &Window, vui: &Vui, r: &dyn Raster, x: i32, row: i32, width: i32, rows: i32) -> [i32; 4] {
    let h = vui.font_height(r) - 4;
    let left = w.top_left()[0] + x;
    let (c0, c1) = (w.row_center(vui, r, row)[1], w.row_center(vui, r, row + rows.max(1) - 1)[1]);
    [left - 20, c0 - h / 2, left + width, c1 + h - h / 2]
}

/// Which pad key a glyph is: some by their flags (a mirrored arrow points
/// the other way).
#[derive(Clone, Copy, Debug, PartialEq)]
enum GlyphKey {
    Key(i32),
    /// the d-pad's down arm (212, 216): flipped, up
    Vertical,
    /// the d-pad's right arm (213, 217): mirrored, left
    Horizontal,
}

/// The atlas's button glyphs: the face buttons in their three sets (65-68
/// and 69-72 inline in text, 73-76 large), the shoulder buttons in three
/// (185-188, 191-194, 197-200 and 205), Select and Start (189 and 195
/// inline, 190 and 196, 201 and 202), L3 and R3 (214 and 218, 215 and 219)
/// and the d-pad's arms. The arrows that mark a selection (85-90) and the
/// side arrow (128) are not buttons.
const GLYPHS: &[(i32, GlyphKey)] = {
    use GlyphKey::*;
    use key::*;
    &[
        (65, Key(TRIANGLE)),
        (66, Key(CROSS)),
        (67, Key(SQUARE)),
        (68, Key(CIRCLE)),
        (69, Key(TRIANGLE)),
        (70, Key(CROSS)),
        (71, Key(SQUARE)),
        (72, Key(CIRCLE)),
        (73, Key(TRIANGLE)),
        (74, Key(CROSS)),
        (75, Key(SQUARE)),
        (76, Key(CIRCLE)),
        (185, Key(L1)),
        (186, Key(L2)),
        (187, Key(R1)),
        (188, Key(R2)),
        (189, Key(SELECT)),
        (190, Key(START)),
        (191, Key(L1)),
        (192, Key(L2)),
        (193, Key(R1)),
        (194, Key(R2)),
        (195, Key(SELECT)),
        (196, Key(START)),
        (197, Key(L1)),
        (198, Key(L2)),
        (199, Key(R1)),
        (200, Key(R2)),
        (201, Key(SELECT)),
        (202, Key(START)),
        (205, Key(R2)),
        (212, Vertical),
        (213, Horizontal),
        (214, Key(L3)),
        (215, Key(R3)),
        (216, Vertical),
        (217, Horizontal),
        (218, Key(L3)),
        (219, Key(R3)),
    ]
};

/// The glyphs by what a blit of one hands the raster: its surface, its
/// texel origin and its size.
#[derive(Clone, Debug, Default)]
pub struct GlyphTable(HashMap<(u32, [i32; 2], [i32; 2]), GlyphKey>);

impl GlyphTable {
    pub fn new(vui: &Vui) -> Self {
        let mut t = HashMap::new();
        for &(id, k) in GLYPHS {
            let e = vui.entry(id);
            t.entry((vui.entry_id(&e), e.uv0, e.uv1)).or_insert(k);
        }
        GlyphTable(t)
    }

    /// The key a blit of surface `id`'s texels `a` .. `a + b` with `flags`
    /// draws, when it is a button glyph.
    pub fn key(&self, id: u32, a: [i32; 2], b: [i32; 2], flags: u32) -> Option<i32> {
        Some(match *self.0.get(&(id, a, b))? {
            GlyphKey::Key(k) => k,
            GlyphKey::Vertical => {
                if flags & 2 != 0 {
                    key::UP
                } else {
                    key::DOWN
                }
            }
            GlyphKey::Horizontal => {
                if flags & 1 != 0 {
                    key::LEFT
                } else {
                    key::RIGHT
                }
            }
        })
    }
}

/// A button glyph drawn this frame: where (placed in the frame, with the
/// words drawn after it on its line), and its key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    pub rect: [f32; 4],
    pub key: i32,
}

/// The glyphs a frame drew, as its raster notes them: a glyph, then the
/// label a legend draws right after it ("|x Select").
#[derive(Clone, Debug, Default)]
pub struct Glyphs {
    pub list: Vec<Glyph>,
    /// the last glyph, while the next string may be its label
    label: Option<usize>,
}

impl Glyphs {
    /// A glyph drawn over `rect`.
    pub fn glyph(&mut self, rect: [f32; 4], key: i32) {
        self.list.push(Glyph { rect, key });
        self.label = Some(self.list.len() - 1);
    }

    /// Something else blitted: no label follows.
    pub fn other(&mut self) {
        self.label = None;
    }

    /// A string drawn over `rect`: the last glyph's label when it starts
    /// just after the glyph on its line (a shadow and its text both count).
    pub fn string(&mut self, rect: [f32; 4]) {
        let Some(g) = self.label.and_then(|k| self.list.get_mut(k)) else { return };
        let mid = (rect[1] + rect[3]) / 2.0;
        let r = g.rect;
        if rect[0] >= r[0] && rect[0] <= r[2] + 14.0 && mid >= r[1] - 4.0 && mid <= r[3] + 4.0 {
            g.rect = [r[0], r[1].min(rect[1]), r[2].max(rect[2]), r[3].max(rect[3])];
        } else {
            self.label = None;
        }
    }

    /// The key of the glyph under frame point `p` (the last drawn there),
    /// missed by at most `slop`.
    pub fn at(&self, p: [f32; 2], slop: f32) -> Option<i32> {
        self.list.iter().rev().find(|g| inside(g.rect, p, 0.0)).or_else(|| self.list.iter().rev().find(|g| inside(g.rect, p, slop))).map(|g| g.key)
    }
}

/// What a window's pointer tests draw with: a raster that draws nothing, an
/// eight-pixel font sixteen high, and a `Vui` with no atlas.
#[cfg(test)]
pub mod testing {
    use vi::ui::{Raster, Vui};

    #[derive(Default)]
    pub struct NullRaster;

    /// A 4:3 picture: windows lay out as the game does.
    impl crate::ui::Layout for NullRaster {}

    impl Raster for NullRaster {
        fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            16
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 640, 448]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    pub fn vui() -> Vui {
        Vui::new(Default::default(), Default::default(), Vec::new())
    }

    /// A picture `.0` frame pixels wider each side than 4:3 that records
    /// each blit's anchor, place and size (`ui::Layout`).
    pub struct Wide(pub i32, pub render::ui::Anchor, pub Vec<(render::ui::Anchor, [i32; 2], [i32; 2])>);

    impl Wide {
        pub fn new(side: i32) -> Self {
            Wide(side, render::ui::Anchor::Middle, Vec::new())
        }
    }

    impl crate::ui::Layout for Wide {
        fn side(&self) -> i32 {
            self.0
        }
        fn anchor(&mut self, a: render::ui::Anchor) {
            self.1 = a;
        }
    }

    impl Raster for Wide {
        fn blit(&mut self, _: u32, _: u32, at: [i32; 2], _: [i32; 2], b: [i32; 2], _: i32, _: f32) {
            self.2.push((self.1, at, b));
        }
        fn stretch_blit(&mut self, _: u32, _: u32, at: [i32; 2], size: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {
            self.2.push((self.1, at, size));
        }
        fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
        fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
        fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
        fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
        fn string_width(&self, s: &[u16]) -> i32 {
            s.len() as i32 * 8
        }
        fn point_size(&self) -> i32 {
            16
        }
        fn viewport(&self) -> [i32; 4] {
            [0, 0, 639, 447]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    /// The middle of a hit rectangle, as a frame point.
    pub fn mid(r: [i32; 4]) -> [f32; 2] {
        [(r[0] + r[2]) as f32 / 2.0, (r[1] + r[3]) as f32 / 2.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A legend's glyph takes the words after it on its line, shadow and
    /// all; a string elsewhere, or after another blit, does not.
    #[test]
    fn a_glyph_takes_its_label() {
        let mut g = Glyphs::default();
        g.glyph([40.0, 100.0, 55.0, 115.0], key::CROSS);
        g.string([60.0, 102.0, 110.0, 120.0]);
        g.string([58.0, 100.0, 108.0, 118.0]);
        assert_eq!(g.list[0].rect, [40.0, 100.0, 110.0, 120.0]);
        assert_eq!(g.at([100.0, 110.0], 0.0), Some(key::CROSS));
        // the next glyph on the line, and a string far below it
        g.glyph([120.0, 100.0, 135.0, 115.0], key::TRIANGLE);
        g.string([140.0, 300.0, 180.0, 318.0]);
        assert_eq!(g.list[1].rect, [120.0, 100.0, 135.0, 115.0], "not its label");
        g.string([140.0, 102.0, 180.0, 118.0]);
        assert_eq!(g.list[1].rect, [120.0, 100.0, 135.0, 115.0], "the label was given up");
        // a blit between a glyph and a string
        g.glyph([200.0, 100.0, 215.0, 115.0], key::SQUARE);
        g.other();
        g.string([220.0, 102.0, 260.0, 118.0]);
        assert_eq!(g.list[2].rect[2], 215.0);
        assert_eq!(g.at([230.0, 110.0], 0.0), None);
        // a finger's slop, the exact hit first
        assert_eq!(g.at([113.0, 108.0], 0.0), None);
        assert_eq!(g.at([113.0, 108.0], FINGER), Some(key::CROSS), "just past the label's end");
        assert_eq!(g.at([137.0, 108.0], FINGER), Some(key::TRIANGLE));
    }

    #[test]
    fn hits_take_the_last_drawn() {
        let mut h = Hits::default();
        h.push([0, 0, 100, 100], 1);
        h.push([50, 50, 150, 150], 2);
        assert_eq!(h.at([60.0, 60.0]), Some(2));
        assert_eq!(h.at([10.0, 10.0]), Some(1));
        assert_eq!(h.at([140.0, 10.0]), None);
        assert_eq!(h.at([100.0, 100.0]), Some(2), "x1 and y1 are outside the first");
    }

    #[test]
    fn steps_go_the_way_of_the_target() {
        assert_eq!(steps(1, 3, key::LEFT, key::RIGHT), vec![key::RIGHT, key::RIGHT]);
        assert_eq!(steps(3, 2, key::LEFT, key::RIGHT), vec![key::LEFT]);
        assert!(steps(2, 2, key::LEFT, key::RIGHT).is_empty());
    }
}
