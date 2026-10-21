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

/// The legend for the pad: the inline glyphs (△ 65, ✕ 66, L1 185, R1 187)
/// and what each does.
const PAD_LEGEND: [(i32, &str); 5] = [(65, "Close"), (66, "Center"), (185, ""), (187, "Zoom"), (-1, "Sticks: pan and zoom")];
const KEY_LEGEND: &str = "M Close   Space Center   + - or wheel Zoom   Arrows, WASD or drag Pan";

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

/// Draws the chrome; where its buttons went, with their keys.
pub fn draw(r: &mut dyn Raster, vui: &Vui, c: &Chrome) -> Vec<Button> {
    let mut hits = Vec::new();
    let [tl, br] = c.corners.map(|p| [p[0].round() as i32, p[1].round() as i32]);
    let (m, line) = (8, r.point_size() + 6);
    for (at, name) in &c.names {
        vui.draw_centered_string_ex(r, &wide(name), at[0] as i32, at[1] as i32, NAME, 0, true);
    }
    // the title, top left
    let s = wide(&c.title);
    let tw = r.string_width(&s).max(0);
    panel(vui, r, [tl[0] + m, tl[1] + m], tw + 12, line);
    vui.draw_string_ex(r, &s, tl[0] + m + 6, tl[1] + m + 3, NAME, true);
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
    // the legend, bottom left
    let y = br[1] - m - line;
    if c.keyboard {
        let s = wide(KEY_LEGEND);
        let tw = r.string_width(&s).max(0);
        panel(vui, r, [tl[0] + m, y], tw + 12, line);
        vui.draw_string_ex(r, &s, tl[0] + m + 6, y + 3, NAME, true);
    } else {
        let mut x = tl[0] + m + 6;
        let w: i32 = PAD_LEGEND.iter().map(|(g, s)| if *g >= 0 { vui.entry(*g).uv1[0] } else { 0 } + r.string_width(&wide(s)).max(0) + 14).sum();
        panel(vui, r, [tl[0] + m, y], w, line);
        for (glyph, label) in PAD_LEGEND {
            if glyph >= 0 {
                let e = vui.entry(glyph);
                vui.blit(r, glyph, [x, y + (line - e.uv1[1]) / 2]);
                x += e.uv1[0] + 2;
            }
            let s = wide(label);
            r.draw_string([x, y + 3], NAME, &s);
            x += r.string_width(&s).max(0) + 12;
        }
    }
    // the build, bottom right
    if let Some((done, of)) = c.progress {
        let s = wide(&format!("Drawing the map: {done} of {of} areas"));
        let tw = r.string_width(&s).max(0);
        panel(vui, r, [br[0] - m - tw - 12, y], tw + 12, line);
        vui.draw_string_ex(r, &s, br[0] - m - tw - 6, y + 3, NAME, true);
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, mid, vui};

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
