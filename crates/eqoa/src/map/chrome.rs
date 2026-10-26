//! What the map draws over its picture, in the game's look: the area names
//! (`DrawCenteredStringEx`'s shadowed words), the title (the world and the
//! player's area, as the start menu's location line has it), the buttons, the
//! legend and the build's progress, each on `VIWndGoto`'s panel (atlas 61
//! stretched at 0.75, the game's own map-like window, its GM teleport). Laid
//! out against the window's edges in the frame's units; a pointer finds the
//! buttons where they were drawn (`draw`'s answer) and the legend's pad
//! glyphs where the raster noted them.

use crate::pointer::key;
use vi::ui::{Raster, Vui, wide};

/// The buttons, right to left from the top right corner: their words and
/// the pad's keys they press.
pub const BUTTONS: [(&str, i32); 4] = [("Close", key::TRIANGLE), ("+", key::R1), ("-", key::L1), ("Center", key::CROSS)];

/// The legend for the pad: the inline glyphs (△ 65, ✕ 66, □ 67, L1 185,
/// R1 187) and what each does.
const PAD_LEGEND: [(i32, &str); 7] = [(65, "Close"), (66, "Center"), (67, "Waypoint"), (185, ""), (187, "Zoom"), (-1, "Sticks, drag, pinch: pan and zoom"), (-1, "Tap: waypoint")];
const KEY_LEGEND: [&str; 5] = ["M Close", "Space Center", "+ - or wheel Zoom", "Arrows, WASD or drag Pan", "Click or Q Waypoint"];

pub const NAME: [f32; 4] = [1.0, 0.97, 0.85, 1.0];
const PANEL: i32 = 61;

/// What the chrome shows.
pub struct Chrome {
    /// the window's corners (top left, bottom right) as frame points
    pub corners: [[f32; 2]; 2],
    pub title: String,
    /// the cells' names and their middles, frame points
    pub names: Vec<([f32; 2], String)>,
    /// a keyboard is in use: its keys in the legend, else the pad's glyphs
    pub keyboard: bool,
    /// zones on the map, of how many, while it is being drawn
    pub progress: Option<(usize, usize)>,
}

/// A panel the size of `w` x `h` at `at` (VIWndGoto's).
fn panel(vui: &Vui, r: &mut dyn Raster, at: [i32; 2], w: i32, h: i32) {
    vui.stretch_blit_ex(r, PANEL, at, [w, h], 3, 0.75);
}

/// A button drawn: its frame rectangle (x0, y0, x1, y1) and its key.
pub type Button = ([i32; 4], i32);

/// The cells' names (under the marker, which goes over them).
pub fn names(r: &mut dyn Raster, vui: &Vui, c: &Chrome) {
    for (at, name) in &c.names {
        vui.draw_centered_string_ex(r, &wide(name), at[0] as i32, at[1] as i32, NAME, 0, true);
    }
}

/// Draws the chrome over everything; where its buttons went, with their
/// keys. In a window too narrow for a row, the title goes under the buttons
/// and the legend onto as many lines as it needs, from the foot up.
pub fn draw(r: &mut dyn Raster, vui: &Vui, c: &Chrome) -> Vec<Button> {
    let mut hits = Vec::new();
    let [tl, br] = c.corners.map(|p| [p[0].round() as i32, p[1].round() as i32]);
    let (m, line) = (8, r.point_size() + 6);
    let avail = br[0] - tl[0] - 2 * m;
    // the buttons, top right
    let mut x = br[0] - m;
    for (label, k) in BUTTONS {
        let s = wide(label);
        let bw = (r.string_width(&s).max(0) + 16).max(line + 4);
        let at = [x - bw, tl[1] + m];
        panel(vui, r, at, bw, line);
        vui.draw_centered_string_ex(r, &s, at[0] + bw / 2, at[1] + line / 2, NAME, 0, true);
        hits.push(([at[0], at[1], at[0] + bw, at[1] + line], k));
        x = at[0] - 6;
    }
    // the title, top left (under the buttons when they would meet), and the
    // build's progress under it
    let s = wide(&c.title);
    let tw = r.string_width(&s).max(0);
    let mut y = tl[1] + m;
    if tl[0] + m + tw + 12 > x {
        y += line + 4;
    }
    panel(vui, r, [tl[0] + m, y], tw + 12, line);
    vui.draw_string_ex(r, &s, tl[0] + m + 6, y + 3, NAME, true);
    if let Some((done, of)) = c.progress {
        let s = wide(&format!("Drawing the map: {done} of {of} areas"));
        let tw = r.string_width(&s).max(0);
        let y = y + line + 4;
        panel(vui, r, [tl[0] + m, y], tw + 12, line);
        vui.draw_string_ex(r, &s, tl[0] + m + 6, y + 3, NAME, true);
    }
    // the legend: its pieces (a glyph or none, and words) in lines that fit
    let pieces: Vec<(i32, &str)> = if c.keyboard { KEY_LEGEND.iter().map(|s| (-1, *s)).collect() } else { PAD_LEGEND.to_vec() };
    let width = |r: &dyn Raster, (g, s): (i32, &str)| if g >= 0 { vui.entry(g).uv1[0] + 2 } else { 0 } + r.string_width(&wide(s)).max(0) + 12;
    let mut lines: Vec<Vec<(i32, &str)>> = vec![Vec::new()];
    let mut used = 0;
    for p in pieces {
        let w = width(r, p);
        if used + w > avail && !lines.last().unwrap().is_empty() {
            lines.push(Vec::new());
            used = 0;
        }
        used += w;
        lines.last_mut().unwrap().push(p);
    }
    for (k, pieces) in lines.iter().enumerate() {
        let y = br[1] - m - line - (lines.len() - 1 - k) as i32 * (line + 4);
        let w: i32 = pieces.iter().map(|p| width(r, *p)).sum();
        panel(vui, r, [tl[0] + m, y], w + 6, line);
        let mut x = tl[0] + m + 6;
        for &(glyph, label) in pieces {
            if glyph >= 0 {
                let e = vui.entry(glyph);
                vui.blit(r, glyph, [x, y + (line - e.uv1[1]) / 2]);
                x += e.uv1[0] + 2;
            }
            let s = wide(label);
            vui.draw_string_ex(r, &s, x, y + 3, NAME, true);
            x += r.string_width(&s).max(0) + 12;
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, mid, vui};

    /// In a phone's tall window (frame units 640 wide) the legend breaks into
    /// lines that each fit, the foot's last, and the buttons stay inside the
    /// window; in a wide one it is one line. The control: the wide window's
    /// one line is wider than the narrow window.
    #[test]
    fn a_narrow_window_breaks_the_legend_into_lines() {
        #[derive(Default)]
        struct Panels(Vec<[i32; 4]>);
        impl crate::ui::Layout for Panels {}
        impl Raster for Panels {
            fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
            fn stretch_blit(&mut self, _: u32, _: u32, at: [i32; 2], size: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {
                self.0.push([at[0], at[1], at[0] + size[0], at[1] + size[1]]);
            }
            fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
            fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
            fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
            fn draw_centered_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16], _: u32) {}
            // ArialUI's letters run about 12 frame pixels with their gap
            fn string_width(&self, s: &[u16]) -> i32 {
                s.len() as i32 * 12
            }
            fn point_size(&self) -> i32 {
                16
            }
            fn viewport(&self) -> [i32; 4] {
                [0, 0, 640, 448]
            }
            fn set_viewport(&mut self, _: [i32; 4]) {}
        }
        // a stand-in atlas: every entry 61 (the panel) of surface 1, so panels draw
        let atlas: Vec<vi::ui::UiTexEntry> = (0..252).map(|i| vi::ui::UiTexEntry { x00: i, uv0: [0, 0], uv1: [16, 16], ..Default::default() }).collect();
        let v = vi::ui::Vui::new(Default::default(), Default::default(), atlas);
        let tall = Chrome { corners: [[0.0, -470.0], [640.0, 918.0]], ..chrome(false) };
        let mut r = Panels::default();
        let hits = draw(&mut r, &v, &tall);
        let foot: Vec<&[i32; 4]> = r.0.iter().filter(|p| p[1] > 600).collect();
        assert!(foot.len() >= 2, "{} lines: {foot:?}", foot.len());
        assert!(foot.iter().all(|p| p[2] <= 640 - 8 + 6), "each line inside the window: {foot:?}");
        assert!(hits.iter().all(|h| h.0[0] >= 0 && h.0[2] <= 640), "{hits:?}");
        let mut w = Panels::default();
        draw(&mut w, &v, &Chrome { corners: [[-300.0, 0.0], [940.0, 448.0]], ..chrome(false) });
        let one: Vec<&[i32; 4]> = w.0.iter().filter(|p| p[1] > 380).collect();
        assert_eq!(one.len(), 1, "a wide window's one line");
        assert!(one[0][2] - one[0][0] > 640 - 16, "the control: wider than the narrow window");
    }

    fn chrome(keyboard: bool) -> Chrome {
        Chrome { corners: [[-106.0, 0.0], [746.0, 448.0]], title: "Tunaria: Freeport (North)".into(), names: Vec::new(), keyboard, progress: Some((3, 175)) }
    }

    /// Each button is where it was drawn, against the window's top right
    /// corner (here a 16:9 window's, past the frame's side), and a pointer
    /// there presses its key; a pointer elsewhere presses none. The control:
    /// the map with no buttons noted finds nothing at the Close button.
    #[test]
    fn a_pointer_presses_the_button_under_it() {
        let hits = draw(&mut NullRaster, &vui(), &chrome(true));
        let mut map = super::super::Map::default();
        let close = hits.iter().find(|h| h.1 == key::TRIANGLE).unwrap().0;
        assert!(close[2] <= 746 - 8 && close[2] > 640, "against the window's right side, past the frame's: {close:?}");
        assert!(map.point(mid(close), false).is_empty(), "the control: no buttons noted");
        map.buttons = hits.clone();
        assert_eq!(map.point(mid(close), false), vec![key::TRIANGLE]);
        for &(r, k) in &hits {
            assert_eq!(map.point(mid(r), false), vec![k]);
        }
        assert_eq!(hits.len(), BUTTONS.len());
        assert!(map.point([320.0, 224.0], false).is_empty(), "the map itself");
        // a finger's slop reaches a button just missed
        assert_eq!(map.point([close[2] as f32 + 3.0, mid(close)[1]], true), vec![key::TRIANGLE]);
        assert!(map.point([close[2] as f32 + 3.0, mid(close)[1]], false).is_empty());
    }
}
