//! The minimap (F3 pass 18, the user: "a minimap next to spellbar top right,
//! but under the name plate, should fit nicely"; the client's own, as the
//! map is). In play it sits under the HUD's quick menu's name plate and left
//! of its column of icons (`FRAME`), out at a wide window's right and up a
//! tall one's top as the quick menus go. It shows the map's own pictures
//! round the player: the overview, and the tiles of the cells it shows, kept
//! on the GPU beside the map's (`Map::take_pictures`).
//!
//! North is up, as the map has it. The compass beside the portrait already
//! turns with the camera, and a map that stays put keeps the waypoint, the
//! camps and the roads where the big map shows them. On it:
//! - the player's dart in the middle;
//! - the group in its compass colours;
//! - the creatures and players in view, in their names' colours;
//! - the target, ringed;
//! - the waypoint, on the rim while it is farther than the minimap shows.
//!
//! A click or a tap on it opens the map (`hud::Pick::Minimap`); the wheel over
//! it zooms it (`Map::mini_wheel`, between `SCALE_LIMITS`). It is shown where
//! the quick menus are (in play, no window up, no player search), only with
//! Extended's Minimap on (`settings::ClientOptions::minimap`, on by default),
//! and never with the map up, under Hide UI or behind the loading screen. Its
//! pass goes between the names and the HUD's quads
//! (`render::Renderer::draw_text_around`), so the HUD's pieces over it (the
//! breath gauge, under water) and every window stay on top.

use super::{Context, Map, NOT_DRAWN, OUTLINE, OUTSIDE, WAYPOINT, view};
use render::map::MapQuad;
use render::ui::{Anchor, Quad, Vertical, frame_from_window, window_from_frame};
use render::wgpu;

/// The minimap's frame in the HUD's 640x448 frame (`Anchor::Right`,
/// `Vertical::Top`, as the quick menus): under the quick menu's name plate
/// (its title bar, x 430..592 and y 25..53, `hudmenu::HudMenu`) and left of
/// its column of icons (from x 589). It is clear of the breath gauge's word
/// and bar under water (x 430..~500 and y 50..80) but for its top left
/// corner, which the gauge, drawn after it, stays over.
pub const FRAME: [i32; 4] = [474, 58, 586, 170];

/// The frame art's border (`vi::ui::frame_sets`' first, a window's), the
/// pictures inside it.
pub const INSET: i32 = 6;

/// World units a frame pixel at first: the pictures' 100 frame pixels across
/// show 600 units, a couple of camps' worth round the player. The wheel's
/// limits (`Map::mini_wheel`): 200 to 2,500 units across.
pub const SCALE: f32 = 6.0;
pub const SCALE_LIMITS: (f32, f32) = (2.0, 25.0);
/// A wheel's notch.
const ZOOM_STEP: f32 = 1.25;

/// The minimap's own state, kept by the map.
#[derive(Clone, Debug)]
pub struct Mini {
    /// world units a frame pixel (`SCALE`, the wheel's)
    pub scale: f32,
    /// drawn this frame (`Map::mini_frame` made its quads)
    pub up: bool,
    /// where its frame is in the window, pixels (`Map::mini_contains`)
    pub frame_px: [f32; 4],
    /// the tiles it shows, by zone: kept on the GPU (`Map::take_pictures`)
    pub wanted: Vec<usize>,
}

impl Default for Mini {
    fn default() -> Self {
        Mini { scale: SCALE, up: false, frame_px: [0.0; 4], wanted: Vec::new() }
    }
}

/// What the minimap is given each frame besides what the map keeps.
#[derive(Clone, Debug, Default)]
pub struct MiniView {
    /// the creatures and players in view: (x, z) and their names' colour
    pub dots: Vec<([f32; 2], [f32; 4])>,
    /// the target's place (x, z), when it is in view
    pub target: Option<[f32; 2]>,
    /// marks of the map's own kinds on it: place (x, z) and colour (the hook
    /// for the map's XP camps and coach stops once the map draws them)
    pub marks: Vec<([f32; 2], [f32; 4])>,
}

/// The pictures' rectangle in the HUD's frame: `FRAME` less the border.
pub fn inner() -> [i32; 4] {
    [FRAME[0] + INSET, FRAME[1] + INSET, FRAME[2] - INSET, FRAME[3] - INSET]
}

/// HUD frame rectangle `r` (x0, y0, x1, y1) placed as the HUD's right side
/// places it (`Anchor::Right`, `Vertical::Top`), in a `size` window's
/// pixels.
pub fn window_rect(size: (u32, u32), r: [i32; 4]) -> [f32; 4] {
    let (side, top) = (render::ui::side(size), render::ui::top(size));
    let a = window_from_frame(size, [Anchor::Right.place(r[0] as f32, side), Vertical::Top.place(r[1] as f32, top)]);
    let b = window_from_frame(size, [Anchor::Right.place(r[2] as f32, side), Vertical::Top.place(r[3] as f32, top)]);
    [a[0], a[1], b[0], b[1]]
}

/// The part of world rectangle `r` (x0, z0, x1, z1) inside `shown`: where it
/// goes in the window (`to_px`) and the share of `r` it is, as a texture's
/// coordinates over `r` (u east, v south, as the map lays its pictures).
fn clip(r: [f32; 4], shown: [f32; 4], to_px: impl Fn([f32; 2]) -> [f32; 2]) -> Option<([f32; 4], [f32; 4])> {
    let (x0, z0, x1, z1) = (r[0].max(shown[0]), r[1].max(shown[1]), r[2].min(shown[2]), r[3].min(shown[3]));
    if x0 >= x1 || z0 >= z1 {
        return None;
    }
    let (a, b) = (to_px([x0, z0]), to_px([x1, z1]));
    let (w, h) = ((r[2] - r[0]).max(1e-6), (r[3] - r[1]).max(1e-6));
    Some(([a[0], a[1], b[0], b[1]], [(x0 - r[0]) / w, (z0 - r[1]) / h, (x1 - r[0]) / w, (z1 - r[1]) / h]))
}

/// Window pixel `q`, outside rectangle `r`, brought in along the line from
/// `mid` to the rectangle's edge: where something farther than the minimap
/// shows is marked on its rim.
fn to_rim(mid: [f32; 2], q: [f32; 2], r: [f32; 4]) -> [f32; 2] {
    let d = [q[0] - mid[0], q[1] - mid[1]];
    let half = [(r[2] - r[0]) / 2.0, (r[3] - r[1]) / 2.0];
    let k = (d[0].abs() / half[0].max(1e-6)).max(d[1].abs() / half[1].max(1e-6));
    if k <= 1.0 {
        return q;
    }
    [mid[0] + d[0] / k, mid[1] + d[1] / k]
}

impl Map {
    /// Whether window pixel `p` is on the minimap, drawn this frame (the
    /// wheel's there: `mini_wheel`).
    pub fn mini_contains(&self, p: [f32; 2]) -> bool {
        let r = self.mini.frame_px;
        self.mini.up && !self.open && p[0] >= r[0] && p[0] < r[2] && p[1] >= r[1] && p[1] < r[3]
    }

    /// The wheel's `lines` over the minimap (up positive: in).
    pub fn mini_wheel(&mut self, lines: f32) {
        self.mini.scale = (self.mini.scale * ZOOM_STEP.powf(-lines)).clamp(SCALE_LIMITS.0, SCALE_LIMITS.1);
    }

    /// Not drawn this frame (Minimap off, a window up, Hide UI, the map up):
    /// nothing composed, its tiles no longer kept for it.
    pub fn mini_off(&mut self) {
        self.mini.up = false;
        self.mini.wanted.clear();
    }

    /// The minimap's frame, after `frame`: its pictures, the marks on them
    /// and its border composed into the map's layers (the map is not up), the
    /// tiles it shows asked of the cache and kept. Nothing while the world
    /// mapped is not the player's (a teleport's first frames).
    pub fn mini_frame(&mut self, cx: &Context, mv: &MiniView) {
        self.mini.up = false;
        let Some((p, heading)) = cx.player.filter(|_| !self.open) else {
            self.mini.wanted.clear();
            return;
        };
        let frame = self.frame;
        let waypoint = self.waypoints.get(&cx.world).copied();
        let (Some(w), Some(l)) = (self.world.as_mut(), self.layers.as_mut()) else {
            self.mini.wanted.clear();
            return;
        };
        if w.index != cx.world {
            self.mini.wanted.clear();
            return;
        }
        let size = cx.size;
        let outer = window_rect(size, FRAME);
        let inside = window_rect(size, inner());
        let across = (inner()[2] - inner()[0]) as f32;
        let k = (inside[2] - inside[0]) / across;
        if k <= 0.0 {
            return;
        }
        // world units a window pixel; the player at the middle, north up
        let s = self.mini.scale / k;
        let mid = [(inside[0] + inside[2]) / 2.0, (inside[1] + inside[3]) / 2.0];
        let c = [p[0], p[2]];
        let to_px = |q: [f32; 2]| [mid[0] + (q[0] - c[0]) / s, mid[1] + (q[1] - c[1]) / s];
        let shown = [c[0] - (mid[0] - inside[0]) * s, c[1] - (mid[1] - inside[1]) * s, c[0] + (inside[2] - mid[0]) * s, c[1] + (inside[3] - mid[1]) * s];

        // the pictures: outside the world, a cell not drawn yet, the
        // overview, the tiles over it where they are on the GPU
        let full = [0.0, 0.0, 1.0, 1.0];
        let quads = &mut l.map.quads;
        quads.clear();
        quads.push(MapQuad::rect(l.white, inside, full, OUTSIDE));
        // the cells it shows that hold a zone: (zone, its rectangle)
        let cells = &w.builder.cells;
        let extent = cells.extent();
        let showing: Vec<(usize, [f32; 4])> = (0..cells.len())
            .filter_map(|cell| {
                let r = cells.rect(cell);
                let on = r[0] < shown[2] && r[2] > shown[0] && r[1] < shown[3] && r[3] > shown[1];
                Some((cells.zone_at[cell].filter(|_| on)?, r))
            })
            .collect();
        for &(_, rect) in &showing {
            if let Some((r, _)) = clip(rect, shown, to_px) {
                quads.push(MapQuad::rect(l.white, r, full, NOT_DRAWN));
            }
        }
        if let Some((r, uv)) = w.overview.and_then(|_| clip(extent, shown, to_px)) {
            quads.push(MapQuad::rect(w.overview.unwrap_or_default(), r, uv, [1.0; 4]));
        }
        let mut wanted = Vec::new();
        for &(zi, rect) in &showing {
            wanted.push(zi);
            match w.tiles.get_mut(&zi) {
                Some(t) => {
                    t.1 = frame;
                    if let Some((r, uv)) = clip(rect, shown, to_px) {
                        quads.push(MapQuad::rect(t.0, r, uv, [1.0; 4]));
                    }
                }
                None => {
                    w.builder.want_tile(zi);
                }
            }
        }

        // the marks, cut to the pictures, in the layer's frame units
        let ui = &mut l.ui;
        ui.begin(size);
        let Some(white) = l.assets.white else { return };
        let f = |q: [f32; 2]| frame_from_window(size, q);
        let (a, b) = (f([inside[0], inside[1]]), f([inside[2], inside[3]]));
        let cut = Some([a[0].floor() as i32, a[1].floor() as i32, b[0].ceil() as i32, b[1].ceil() as i32]);
        let mut solid = |r: [f32; 4], color: [f32; 4]| {
            let (a, b) = (f([r[0], r[1]]), f([r[2], r[3]]));
            ui.quads.push(Quad { tex: white, blend: 3, pos: [[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]], uv: [[0.5, 0.5]; 4], color, clip: cut });
        };
        let on = |q: [f32; 2], m: f32| q[0] >= inside[0] - m && q[0] <= inside[2] + m && q[1] >= inside[1] - m && q[1] <= inside[3] + m;
        let (dot, dot_o) = dot_radius(size);
        let dot_at = |solid: &mut dyn FnMut([f32; 4], [f32; 4]), q: [f32; 2], r: f32, o: f32, color: [f32; 4]| {
            solid([q[0] - r - o, q[1] - r - o, q[0] + r + o, q[1] + r + o], OUTLINE);
            solid([q[0] - r, q[1] - r, q[0] + r, q[1] + r], color);
        };
        for &(at, color) in mv.marks.iter().chain(&mv.dots) {
            let q = to_px(at);
            if on(q, dot + dot_o) {
                dot_at(&mut solid, q, dot, dot_o, color);
            }
        }
        // the target: a white ring round its dot
        if let Some(t) = mv.target {
            let q = to_px(t);
            let (r, o) = (dot * 2.4, dot_o.max(0.75));
            if on(q, r + o) {
                for (rr, color) in [(r + o, OUTLINE), (r, [1.0; 4])] {
                    let t = o.max(1.0);
                    solid([q[0] - rr, q[1] - rr, q[0] + rr, q[1] - rr + t], color);
                    solid([q[0] - rr, q[1] + rr - t, q[0] + rr, q[1] + rr], color);
                    solid([q[0] - rr, q[1] - rr, q[0] - rr + t, q[1] + rr], color);
                    solid([q[0] + rr - t, q[1] - rr, q[0] + rr, q[1] + rr], color);
                }
            }
        }
        // the group's members, their compass colours, as the map has them
        for (pos, index) in &cx.group {
            let q = to_px([pos[0], pos[2]]);
            let color = super::member_color(*index);
            let (r, o) = view::member_radius(size);
            if on(q, r + o) {
                dot_at(&mut solid, q, r, o, color);
            }
        }
        // the waypoint, on the rim while it is farther than the minimap shows
        if let Some(at) = waypoint {
            let (r, o) = view::waypoint_radius(size);
            let (r, o) = (r * 0.8, o);
            let edge = [inside[0] + r + o, inside[1] + r + o, inside[2] - r - o, inside[3] - r - o];
            let q = to_rim(mid, to_px(at), edge);
            for (rr, color) in [(r + o * 1.5, OUTLINE), (r, WAYPOINT)] {
                let d = [[q[0], q[1] - rr], [q[0] + rr, q[1]], [q[0], q[1] + rr], [q[0] - rr, q[1]]].map(f);
                ui.quads.push(Quad { tex: white, blend: 3, pos: d, uv: [[0.5, 0.5]; 4], color, clip: cut });
            }
        }
        // the player's dart in the middle, a little smaller than the map's
        let (r, o) = view::marker_radius(size);
        let (r, o) = (r * 0.85, o);
        for (rr, color) in [(r + o, OUTLINE), (r, super::MARKER)] {
            let d = view::dart(mid, heading, rr).map(f);
            ui.quads.push(Quad { tex: white, blend: 3, pos: d, uv: [[0.5, 0.5]; 4], color, clip: cut });
        }
        // the border, a window's frame round it, placed as the HUD's right side is
        if let Some(vui) = cx.vui {
            let mut r = l.assets.raster(ui);
            r.ui.anchor = Anchor::Right;
            r.ui.vertical = Vertical::Top;
            vui.blit_frame(&mut r, [FRAME[0], FRAME[1]], [FRAME[2] - FRAME[0], FRAME[3] - FRAME[1]], &vi::ui::frame_sets()[0], 3, 1.0, true);
        }
        self.mini.wanted = wanted;
        self.mini.frame_px = outer;
        self.mini.up = true;
    }

    /// The minimap over the frame, when it was composed this frame: its own
    /// pass into `target` (a `size` window, `depth` its depth buffer),
    /// between the names and the HUD (`render::Renderer::draw_text_around`).
    pub fn draw_mini(&self, device: &wgpu::Device, enc: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32)) {
        let (true, false, Some(l)) = (self.mini.up, self.open, self.layers.as_ref()) else { return };
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("minimap"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }), stencil_ops: None }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        l.map.draw(device, &mut pass, size);
        l.ui.draw(device, &mut pass, size);
    }
}

/// A creature's or a player's dot on the minimap, its half-width and its
/// outline's, in window pixels: 1.6 and 0.5 of the frame's lines, smaller
/// than a group member's (`view::member_radius`, 2.5).
pub fn dot_radius(size: (u32, u32)) -> (f32, f32) {
    let line = window_from_frame(size, [0.0, 1.0])[1] - window_from_frame(size, [0.0, 0.0])[1];
    (1.6 * line, 0.5 * line)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frame sits under the quick menu's name plate (to y 53) and left of
    /// its column of icons (from x 589), in the HUD's right half, and clear
    /// of the compass and the portrait at the left; the pictures inside it.
    /// The control: a frame reaching the column.
    #[test]
    fn the_minimap_fits_under_the_name_plate_beside_the_icons() {
        let plate = [430, 25, 592, 53];
        let column_x = 589;
        assert!(FRAME[1] > plate[3], "below the name plate");
        assert!(FRAME[2] < column_x, "left of the column of icons");
        assert!(FRAME[0] >= plate[0] && FRAME[2] <= plate[2], "under the plate, not past its ends");
        assert!(FRAME[0] > 320, "in the HUD's right half, nowhere near the compass (x 17..61)");
        let i = inner();
        assert!(i[0] > FRAME[0] && i[2] < FRAME[2] && i[1] > FRAME[1] && i[3] < FRAME[3]);
        assert_eq!((i[2] - i[0], i[3] - i[1]), (100, 100), "the pictures square");
        let reaching = [FRAME[0], FRAME[1], column_x + 2, FRAME[3]];
        assert!(reaching[2] >= column_x, "the control: a frame on the column");
    }

    /// The frame is placed as the HUD's right side places the quick menus: in
    /// a 4:3 window where the frame is, in a wide one out at the window's
    /// right by the same amount the menus go, and up a tall one's top. The
    /// control: placed with the frame (no anchor), a wide window's minimap
    /// would stay over the picture's middle.
    #[test]
    fn it_goes_out_with_the_huds_right_side() {
        // 640x480 is 4:3: the frame's place, scaled to the window's height
        let four = window_rect((640, 480), FRAME);
        let k = 480.0 / 448.0;
        assert!((four[0] - 474.0).abs() < 1e-3 && (four[2] - 586.0).abs() < 1e-3, "{four:?}");
        assert!((four[1] - 58.0 * k).abs() < 1e-3 && (four[3] - 170.0 * k).abs() < 1e-3, "{four:?}");
        // 16:9: 1280x720 shows a frame 4/3 of 720 = 960 wide, 160 pixels a side (106 frame pixels)
        let wide = window_rect((1280, 720), FRAME);
        let side = render::ui::side((1280, 720));
        let unplaced = window_from_frame((1280, 720), [FRAME[0] as f32, FRAME[1] as f32]);
        assert!(side > 100.0 && wide[0] > unplaced[0] + 100.0, "out at the right: {wide:?} {unplaced:?}");
        assert!(wide[2] <= 1280.0, "inside the window");
        // tall: 798x828 shows 85 frame pixels above the frame: up with the top
        let tall = window_rect((798, 828), FRAME);
        let unplaced = window_from_frame((798, 828), [FRAME[0] as f32, FRAME[1] as f32]);
        assert!(tall[1] < unplaced[1] - 50.0 && tall[1] >= 0.0, "up with the top: {tall:?} {unplaced:?}");
    }

    /// What lies past the pictures' edge is marked on the rim, along its
    /// way from the middle; what is inside stays put.
    #[test]
    fn a_far_waypoint_goes_on_the_rim() {
        let r = [0.0, 0.0, 100.0, 100.0];
        let mid = [50.0, 50.0];
        assert_eq!(to_rim(mid, [60.0, 40.0], r), [60.0, 40.0]);
        assert_eq!(to_rim(mid, [250.0, 50.0], r), [100.0, 50.0]);
        let q = to_rim(mid, [50.0 + 300.0, 50.0 - 150.0], r);
        assert!((q[0] - 100.0).abs() < 1e-3 && (q[1] - 25.0).abs() < 1e-3, "{q:?}");
    }

    /// A world rectangle cut to what the minimap shows: its window place and
    /// its share as a texture's coordinates; nothing when it is all outside.
    #[test]
    fn a_picture_is_cut_to_what_is_shown() {
        let shown = [1000.0, 1000.0, 1500.0, 1500.0];
        let to_px = |q: [f32; 2]| [q[0] - 1000.0, q[1] - 1000.0];
        let (r, uv) = clip([0.0, 0.0, 2000.0, 2000.0], shown, to_px).unwrap();
        assert_eq!(r, [0.0, 0.0, 500.0, 500.0]);
        assert_eq!(uv, [0.5, 0.5, 0.75, 0.75]);
        assert!(clip([2000.0, 0.0, 4000.0, 2000.0], shown, to_px).is_none(), "the control: a cell beside it");
    }
}
