//! What the map shows: the world point at the window's middle and how many
//! world units a window pixel covers. North is up and east right, as the
//! game's compass has them: its N points to -z (`hud::draw_compass`, and
//! `Vect3::hpr`'s heading h faces (sin h, cos h) in x and z), and Tunaria's
//! area names put Halas's snow at the small z. So a window pixel right is
//! +x and a pixel down is +z, with no mirroring.

/// The map's view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// the world (x, z) at the window's middle
    pub center: [f32; 2],
    /// world units a window pixel covers
    pub scale: f32,
}

/// The closest the map zooms: a world unit a window pixel (a tile's texel,
/// 3.9 units, four pixels across).
pub const MIN_SCALE: f32 = 1.0;

/// How much of the world the map opens on: this many world units across the
/// window's shorter side (the player's area and a little round it).
pub const OPENING_SPAN: f32 = 2500.0;

impl View {
    /// Where world point `p` (x, z) is in a `size` window.
    pub fn to_window(self, size: (u32, u32), p: [f32; 2]) -> [f32; 2] {
        [(p[0] - self.center[0]) / self.scale + size.0 as f32 / 2.0, (p[1] - self.center[1]) / self.scale + size.1 as f32 / 2.0]
    }

    /// The world point (x, z) at window pixel `s`.
    pub fn to_world(self, size: (u32, u32), s: [f32; 2]) -> [f32; 2] {
        [(s[0] - size.0 as f32 / 2.0) * self.scale + self.center[0], (s[1] - size.1 as f32 / 2.0) * self.scale + self.center[1]]
    }

    /// The world rectangle (x0, z0, x1, z1) the window shows.
    pub fn shown(self, size: (u32, u32)) -> [f32; 4] {
        let a = self.to_world(size, [0.0, 0.0]);
        let b = self.to_world(size, [size.0 as f32, size.1 as f32]);
        [a[0], a[1], b[0], b[1]]
    }

    /// Zoomed by `k` (above 1 out, below 1 in) about window pixel `at`, the
    /// scale kept within `limits` (closest, furthest): the world point under
    /// `at` stays under it.
    pub fn zoom(&mut self, k: f32, at: [f32; 2], size: (u32, u32), limits: (f32, f32)) {
        let before = self.to_world(size, at);
        self.scale = (self.scale * k).clamp(limits.0, limits.1.max(limits.0));
        let after = self.to_world(size, at);
        self.center = [self.center[0] + before[0] - after[0], self.center[1] + before[1] - after[1]];
    }

    /// The map dragged `d` window pixels: what was under the pointer stays
    /// under it.
    pub fn drag(&mut self, d: [f32; 2]) {
        self.center = [self.center[0] - d[0] * self.scale, self.center[1] - d[1] * self.scale];
    }

    /// The middle kept inside `extent` (x0, z0, x1, z1): the map cannot be
    /// lost off the screen.
    pub fn keep_in(&mut self, extent: [f32; 4]) {
        self.center = [self.center[0].clamp(extent[0], extent[2].max(extent[0])), self.center[1].clamp(extent[1], extent[3].max(extent[1]))];
    }
}

/// The scales a `size` window zooms between over a world of `extent`: in to
/// `MIN_SCALE`, out until the whole world fits with a little room.
pub fn limits(extent: [f32; 4], size: (u32, u32)) -> (f32, f32) {
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let fit = ((extent[2] - extent[0]) / w).max((extent[3] - extent[1]) / h) * 1.1;
    (MIN_SCALE, fit.max(opening_scale(size)))
}

/// The scale the map opens at in a `size` window (`OPENING_SPAN`).
pub fn opening_scale(size: (u32, u32)) -> f32 {
    (OPENING_SPAN / size.0.min(size.1).max(1) as f32).max(MIN_SCALE)
}

/// The player's marker: a dart at window pixel `at` pointing the way a
/// heading of `h` faces ((sin h, cos h) in x and z, so on the map too),
/// `r` pixels from its middle to its tip. Its corners in fan order: the tip,
/// a wing, the notch, the other wing.
pub fn dart(at: [f32; 2], h: f32, r: f32) -> [[f32; 2]; 4] {
    let d = [h.sin(), h.cos()];
    let side = [-d[1], d[0]];
    let p = |a: f32, s: f32| [at[0] + d[0] * a * r + side[0] * s * r, at[1] + d[1] * a * r + side[1] * s * r];
    [p(1.0, 0.0), p(-0.7, 0.65), p(-0.35, 0.0), p(-0.7, -0.65)]
}

/// How many window pixels `lines` of the frame's 448 are in a `size`
/// window: the HUD's own scale, so what is drawn by it keeps its size
/// against the HUD whatever the window, and against nothing on the map.
fn lines(size: (u32, u32), lines: f32) -> f32 {
    let (a, b) = (render::ui::window_from_frame(size, [0.0, 0.0]), render::ui::window_from_frame(size, [0.0, lines]));
    b[1] - a[1]
}

/// The player's marker (`dart`), from its middle to its tip, and its
/// outline's width, in window pixels: 7 and 1 of the frame's lines, so the
/// dart is 12 lines long, a little less than the map's names are tall, at
/// every zoom (F3 20261023-050705: 0.022 of the window's height and 6
/// pixels more, a dart 63 pixels long on a 1,417-line window, covered a
/// zone's square once the map was zoomed out to the world).
pub fn marker_radius(size: (u32, u32)) -> (f32, f32) {
    (lines(size, 7.0), lines(size, 1.0))
}

/// A group member's dot, its half-width and its outline's, in window
/// pixels: 2.5 and 0.5 of the frame's lines, a third of the player's dart.
pub fn member_radius(size: (u32, u32)) -> (f32, f32) {
    (lines(size, 2.5), lines(size, 0.5))
}

/// The waypoint's diamond (`Map::click`), from its middle to a corner, and
/// its outline's width, in window pixels: 4.5 and 1 of the frame's lines,
/// between a member's dot and the player's dart.
pub fn waypoint_radius(size: (u32, u32)) -> (f32, f32) {
    (lines(size, 4.5), lines(size, 1.0))
}

/// A quest mark's badge (`quests`), its half side and its outline, in window
/// pixels: a glyph's room, at every zoom.
pub fn quest_radius(size: (u32, u32)) -> (f32, f32) {
    (lines(size, 6.5), lines(size, 1.0))
}

/// A coach's horseshoe (`markers`), from its middle to an edge, and its
/// outline's width, in window pixels: 8 and 1 of the frame's lines, about the
/// player's dart (5 zoomed far out, `compact`, where a world's are many).
pub fn horseshoe_radius(size: (u32, u32), compact: bool) -> (f32, f32) {
    (lines(size, if compact { 5.0 } else { 8.0 }), lines(size, 1.0))
}

/// A community marker's pin (`community`), its head's half width and its
/// outline's, in window pixels: 5.5 and 1 of the frame's lines (3.5 far out,
/// `compact`, where a world's may be many).
pub fn pin_radius(size: (u32, u32), compact: bool) -> (f32, f32) {
    (lines(size, if compact { 3.5 } else { 5.5 }), lines(size, 1.0))
}

/// A coach route's dashes (`markers`), the gaps between them, its line's
/// half width and its shadow's more, in window pixels: 5, 3.5, 0.7 and 0.7 of
/// the frame's lines.
pub fn route_dash(size: (u32, u32)) -> (f32, f32, f32, f32) {
    (lines(size, 5.0), lines(size, 3.5), lines(size, 0.7), lines(size, 0.7))
}

/// How far above (or below) its place a town's name sits (`markers`), in
/// window pixels: clear of a coach's horseshoe there by 2 of the frame's lines.
pub fn name_gap(size: (u32, u32), compact: bool) -> f32 {
    horseshoe_radius(size, compact).0 + lines(size, 2.0)
}

/// How near a marker's middle a pointer must be to be on it (`markers::under`),
/// in window pixels: 10 of the frame's lines (an XP badge's half width), and a
/// finger's miss more (`pointer::FINGER` frame pixels) for a tap.
pub fn marker_reach(size: (u32, u32), finger: bool) -> f32 {
    lines(size, 10.0) + if finger { lines(size, crate::pointer::FINGER) } else { 0.0 }
}

/// How near the waypoint a click must be to clear it, in window pixels:
/// twice its diamond, and a finger's miss more (`pointer::FINGER` frame
/// pixels) for a tap.
pub fn waypoint_reach(size: (u32, u32), finger: bool) -> f32 {
    waypoint_radius(size).0 * 2.0 + if finger { lines(size, crate::pointer::FINGER) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    /// North (-z) is up and east (+x) right; the window's middle is the
    /// view's centre; to_world undoes to_window. The control: a map with z
    /// up (north down) puts Halas below Freeport.
    #[test]
    fn north_is_up_and_east_right() {
        let v = View { center: [24000.0, 14000.0], scale: 2.0 };
        let size = (1000, 800);
        assert!(near(v.to_window(size, [24000.0, 14000.0]), [500.0, 400.0]));
        // 200 units north (-z) is 100 pixels up, 200 east (+x) 100 right
        assert!(near(v.to_window(size, [24000.0, 13800.0]), [500.0, 300.0]));
        assert!(near(v.to_window(size, [24200.0, 14000.0]), [600.0, 400.0]));
        for s in [[0.0, 0.0], [123.0, 456.0], [1000.0, 800.0]] {
            assert!(near(v.to_window(size, v.to_world(size, s)), s));
        }
        // Halas (x 12000, z 4000) is above and left of Freeport (24000, 14000)
        let (halas, freeport) = (v.to_window(size, [12000.0, 4000.0]), v.to_window(size, [24000.0, 14000.0]));
        assert!(halas[1] < freeport[1] && halas[0] < freeport[0]);
        let flipped = |p: [f32; 2]| [p[0], size.1 as f32 - p[1]];
        assert!(flipped(halas)[1] > flipped(freeport)[1], "the control: north down");
        assert_eq!(v.shown(size), [23000.0, 13200.0, 25000.0, 14800.0]);
    }

    /// The point under the pointer stays under it as the map zooms; the
    /// scale stops at the limits. The control: zooming about the middle
    /// moves the pointer's point.
    #[test]
    fn a_zoom_keeps_the_point_under_the_pointer() {
        let size = (1280, 720);
        let mut v = View { center: [10000.0, 10000.0], scale: 4.0 };
        let at = [100.0, 650.0];
        let under = v.to_world(size, at);
        v.zoom(0.5, at, size, (1.0, 40.0));
        assert_eq!(v.scale, 2.0);
        assert!(near(v.to_world(size, at), under), "{:?} vs {under:?}", v.to_world(size, at));
        let mut w = View { center: [10000.0, 10000.0], scale: 4.0 };
        w.zoom(0.5, [640.0, 360.0], size, (1.0, 40.0));
        assert!(!near(w.to_world(size, at), under), "the control");
        // the limits
        v.zoom(0.01, at, size, (1.0, 40.0));
        assert_eq!(v.scale, 1.0);
        v.zoom(1000.0, at, size, (1.0, 40.0));
        assert_eq!(v.scale, 40.0);
    }

    /// The whole of Tunaria (26,000 by 34,000) fits at the furthest zoom,
    /// with room; the closest is a world unit a pixel; the opening shows
    /// 2,500 units across the shorter side. A world smaller than the opening
    /// view still zooms out to it.
    #[test]
    fn the_zoom_reaches_the_whole_world_and_back() {
        let tunaria = [2000.0, 0.0, 28000.0, 34000.0];
        let size = (1917, 973);
        let (lo, hi) = limits(tunaria, size);
        assert_eq!(lo, MIN_SCALE);
        let v = View { center: [15000.0, 17000.0], scale: hi };
        let s = v.shown(size);
        assert!(s[0] <= tunaria[0] && s[1] <= tunaria[1] && s[2] >= tunaria[2] && s[3] >= tunaria[3], "{s:?}");
        let tight = View { scale: hi / 1.2, ..v };
        let t = tight.shown(size);
        assert!(t[1] > tunaria[1] || t[3] < tunaria[3], "a fifth closer no longer fits: {t:?}");
        assert!((opening_scale(size) * 973.0 - OPENING_SPAN).abs() < 1e-2);
        let lavastorm = [4000.0, 4000.0, 8000.0, 6000.0];
        assert_eq!(limits(lavastorm, size).1, opening_scale(size));
    }

    /// A drag moves the map with the pointer; the middle stays inside the
    /// world.
    #[test]
    fn a_drag_follows_the_pointer_and_the_world_stays_on_screen() {
        let size = (800, 600);
        let mut v = View { center: [5000.0, 5000.0], scale: 3.0 };
        let grabbed = v.to_world(size, [300.0, 200.0]);
        v.drag([50.0, -20.0]);
        assert!(near(v.to_world(size, [350.0, 180.0]), grabbed));
        v.center = [-9000.0, 99000.0];
        v.keep_in([2000.0, 0.0, 28000.0, 34000.0]);
        assert_eq!(v.center, [2000.0, 34000.0]);
    }

    /// The marker's tip points the way the heading faces: heading pi north
    /// (up), pi/2 east (right), 0 south (down), the compass's convention.
    /// The control: the tip of a dart drawn with x and z swapped.
    #[test]
    fn the_marker_points_the_way_the_player_faces() {
        use std::f32::consts::{FRAC_PI_2, PI};
        let at = [100.0, 100.0];
        assert!(near(dart(at, PI, 10.0)[0], [100.0, 90.0]));
        assert!(near(dart(at, FRAC_PI_2, 10.0)[0], [110.0, 100.0]));
        assert!(near(dart(at, 0.0, 10.0)[0], [100.0, 110.0]));
        assert!(near(dart(at, -FRAC_PI_2, 10.0)[0], [90.0, 100.0]));
        let swapped = |h: f32| [at[0] + h.cos() * 10.0, at[1] + h.sin() * 10.0];
        assert!(!near(swapped(FRAC_PI_2), dart(at, FRAC_PI_2, 10.0)[0]), "the control");
        // the notch is behind the middle, the wings either side of the line
        let d = dart(at, PI, 10.0);
        assert!(d[2][1] > at[1] && d[1][0] != d[3][0]);
    }

    /// The markers are sized by the frame's lines (the HUD's scale): the
    /// same in a window twice as tall in proportion, and nothing to do with
    /// the zoom (they take only the window). F3 20261023-050705's window, 2560
    /// by 1417: a dart 7 lines (22 pixels) to its tip. The control: the old
    /// size, 0.022 of the height and 6 pixels (37 pixels there).
    #[test]
    fn the_markers_keep_the_huds_size() {
        let near1 = |a: f32, b: f32| (a - b).abs() < 1e-3;
        let (r, outline) = marker_radius((2560, 1417));
        assert!(near1(r, 7.0 * 1417.0 / 448.0) && near1(outline, 1417.0 / 448.0), "{r} {outline}");
        let (r1, _) = marker_radius((640, 448));
        assert!(near1(r1, 7.0));
        let (r2, _) = marker_radius((1280, 896));
        assert!(near1(r2, 2.0 * r1), "twice the window, twice the dart");
        // a window wider than 4:3 keeps the frame's full height: the same dart
        assert!(near1(marker_radius((1920, 896)).0, r2));
        let (d, o) = member_radius((640, 448));
        assert!(near1(d, 2.5) && near1(o, 0.5));
        let old = 0.022 * 1417.0 + 6.0;
        assert!(!near1(marker_radius((2560, 1417)).0, old), "the control: the old size");
    }
}
