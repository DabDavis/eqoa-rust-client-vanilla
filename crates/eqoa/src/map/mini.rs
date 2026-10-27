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
//! - the waypoint, on the rim while it is farther than the minimap shows, and
//!   the group's (`group_waypoint`, orange) the same way;
//! - the map's XP camps and coaches (`markers`, with Map Markers on), as the
//!   map marks them, smaller: not its coach routes (a line between two towns
//!   thousands of units apart crosses 600 units of picture saying nothing of
//!   where it goes) nor its town names (a name is as wide as the minimap);
//! - marks of other kinds it is given (`Mark`, `MiniView::marks`: the
//!   quests'), a quest's over the rest and on the rim while farther.
//!
//! Every mark is a `Mark` (a kind, a world, a place, its card's lines): the
//! one under the mouse's pointer shows its card left of the minimap.
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
use render::ui::{Anchor, Quad, Ui, Vertical, frame_from_window, window_from_frame};
use render::wgpu;

/// The minimap's frame in the HUD's 640x448 frame (`Anchor::Right`,
/// `Vertical::Top`, as the quick menus): under the quick menu's name plate
/// (its title bar, x 430..592 from y 25, its art 19 high: to 44,
/// `hudmenu::HudMenu`) and left of its column of icons (from x 589). It is
/// clear of the breath gauge's word and bar under water (x 430..~500 and y
/// 50..80) but for its top left corner, which the gauge, drawn after it,
/// stays over.
pub const FRAME: [i32; 4] = [474, 50, 586, 162];

/// The quick menu's name plate in the HUD's frame (`hudmenu::HudMenu`'s
/// title bar, placed as `FRAME` is): a tooltip left of the minimap keeps
/// below it, as the HUD draws over the minimap's pass.
pub const PLATE: [i32; 4] = [430, 25, 592, 44];

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
    /// the mouse's pointer, window pixels, while it is over the window
    /// (`Map::mini_hover`): a mark under it shows its card
    pub cursor: Option<[f32; 2]>,
    /// the map's own markers as marks, a world's (`map_marks`): made once
    /// a world, their cards with them
    pub map_marks: Option<(i32, Vec<Mark>)>,
}

impl Default for Mini {
    fn default() -> Self {
        Mini { scale: SCALE, up: false, frame_px: [0.0; 4], wanted: Vec::new(), cursor: None, map_marks: None }
    }
}

/// N (`binds`' "minimap"): the minimap shown or hidden, kept (Extended's
/// Minimap).
pub fn toggle() {
    let on = !crate::settings::client().minimap;
    if let Err(e) = crate::settings::set_minimap(on) {
        crate::say(&format!("minimap {}, not kept: {e}", if on { "shown" } else { "hidden" }));
    }
    crate::say(&format!("minimap {}", if on { "shown" } else { "hidden" }));
}

/// The map's own marks to draw in world `world` out of the minimap's
/// (`Mini::map_marks`): that world's, none with Map Markers off (`on`, the
/// map's toggle, X there and Extended's World page).
fn own_marks(made: &Option<(i32, Vec<Mark>)>, world: i32, on: bool) -> &[Mark] {
    match made {
        Some((w, marks)) if *w == world && on => marks,
        _ => &[],
    }
}

/// The map's markers of world `world` (`markers::of_world`: its coaches and
/// XP camps) as the minimap's marks, each with its card for a tooltip.
pub fn map_marks(world: i32) -> Vec<Mark> {
    super::markers::of_world(world)
        .map(|m| {
            let kind = match m {
                super::markers::Marker::Camp(_) => MarkKind::Camp,
                super::markers::Marker::Coach(_) => MarkKind::Coach,
            };
            Mark { kind, world: m.world(), at: m.at(), tip: m.card() }
        })
        .collect()
}

/// What a mark on the minimap is, which is how it is drawn: the hook other
/// parts of the client fill (`MiniView::marks`) without touching the drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    /// a popular XP camp: "XP" in gold on red, as the map marks it
    Camp,
    /// a coach: the stable's horseshoe, as the map marks it
    Coach,
    /// a quest to take there: "!" in gold, over the other marks and on the
    /// rim while it is farther than the minimap shows
    QuestOffer,
    /// a quest's step to do there: "?" in gold, as `QuestOffer` is drawn
    QuestStep,
}

impl MarkKind {
    /// Whether it is a quest's: drawn over the other marks, and kept on the
    /// rim while it lies farther than the minimap shows, pointing the way.
    pub fn quest(self) -> bool {
        matches!(self, MarkKind::QuestOffer | MarkKind::QuestStep)
    }
}

/// A mark on the minimap: its kind, its world (`gWorldInfo`'s index: only
/// the world played in shows its marks), its place (x, z) and its tooltip's
/// lines in their colours, shown while the mouse's pointer is over it (a
/// line wider than the tooltip goes on over the next, `wrap`).
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub kind: MarkKind,
    pub world: i32,
    pub at: [f32; 2],
    pub tip: Vec<(String, [f32; 4])>,
}

impl Mark {
    /// A mark whose tooltip is `text`, its lines apart by '\n': the first
    /// in a title's colour (`TIP_TITLE`), the rest in `TIP_TEXT`.
    pub fn text(kind: MarkKind, world: i32, at: [f32; 2], text: &str) -> Mark {
        let tip = text.lines().enumerate().map(|(i, l)| (l.to_string(), if i == 0 { TIP_TITLE } else { TIP_TEXT })).collect();
        Mark { kind, world, at, tip }
    }
}

/// A tooltip's title and its other lines, as the map's cards have them.
pub const TIP_TITLE: [f32; 4] = [1.0, 0.86, 0.45, 1.0];
pub const TIP_TEXT: [f32; 4] = [0.95, 0.93, 0.86, 1.0];

/// What the minimap is given each frame besides what the map keeps.
#[derive(Clone, Debug, Default)]
pub struct MiniView {
    /// the creatures and players in view: (x, z) and their names' colour
    pub dots: Vec<([f32; 2], [f32; 4])>,
    /// the target's place (x, z), when it is in view
    pub target: Option<[f32; 2]>,
    /// marks of every kind (`Mark`): the XP camps and coach stops, the
    /// quests' (`Play::mini_view` gathers them)
    pub marks: Vec<Mark>,
}

/// A quest's mark's gold.
const QUEST: [f32; 4] = [1.0, 0.82, 0.12, 1.0];
/// A card's backing under the game's panel, as the map's (`markers`).
const CARD_BACK: [f32; 4] = [0.05, 0.06, 0.08, 0.9];
/// A coach's horseshoe from its middle to an edge, a camp's badge's letters'
/// size (of the map font's), and how far from its place a mark is found or
/// still drawn, in the frame's lines.
const HORSESHOE: f32 = 4.5;
const BADGE_TEXT: f32 = 0.6;
const MARK_REACH: f32 = 6.0;

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

/// The corners of rectangle `r` (x0, y0, x1, y1), clockwise from its top
/// left.
fn corners(r: [f32; 4]) -> [[f32; 2]; 4] {
    [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]]
}

/// A square `r` either side of `q`.
fn square(q: [f32; 2], r: f32) -> [[f32; 2]; 4] {
    corners([q[0] - r, q[1] - r, q[0] + r, q[1] + r])
}

/// A diamond `r` either side of `q`: its points up, right, down and left.
fn diamond(q: [f32; 2], r: f32) -> [[f32; 2]; 4] {
    [[q[0], q[1] - r], [q[0] + r, q[1]], [q[0], q[1] + r], [q[0] - r, q[1]]]
}

/// A plain quad at frame points `pos`, cut to `cut`, pushed as it is (the
/// minimap's points are the window's, `frame_from_window`).
fn plain(ui: &mut Ui, white: usize, pos: [[f32; 2]; 4], color: [f32; 4], cut: Option<[i32; 4]>) {
    ui.quads.push(Quad { tex: white, blend: 3, pos, uv: [[0.5, 0.5]; 4], color, clip: cut });
}

/// A square dot at frame point `p`, `r` frame pixels from its middle to an
/// edge, outlined `o` more.
fn square_dot(ui: &mut Ui, white: usize, cut: Option<[i32; 4]>, p: [f32; 2], r: f32, o: f32, color: [f32; 4]) {
    plain(ui, white, square(p, r + o), OUTLINE, cut);
    plain(ui, white, square(p, r), color, cut);
}

/// A camp's badge at frame point `p`: "XP" in gold on red, outlined, as the
/// map's (`markers::draw_badges`), its letters `BADGE_TEXT` of the font's.
fn badge(ui: &mut Ui, white: usize, font: Option<usize>, p: [f32; 2]) {
    let label = vi::ui::wide("XP");
    let (h, tw) = font.map_or((14.0, 20.0), |fo| (ui.fonts[fo].height as f32, ui.fonts[fo].string_width(&label) as f32));
    let (h, tw) = (h * BADGE_TEXT, tw * BADGE_TEXT);
    let (hw, hh) = (tw / 2.0 + 2.0, h / 2.0);
    plain(ui, white, corners([p[0] - hw - 1.0, p[1] - hh - 1.0, p[0] + hw + 1.0, p[1] + hh + 1.0]), OUTLINE, None);
    plain(ui, white, corners([p[0] - hw, p[1] - hh, p[0] + hw, p[1] + hh]), super::markers::BADGE, None);
    if let Some(fo) = font {
        ui.text_scaled(fo, p[0] - tw / 2.0, p[1] - h / 2.0, BADGE_TEXT, super::markers::BADGE_TEXT, &label);
    }
}

/// A tooltip's most lines and its widest, frame pixels (a line wider goes
/// on over the next).
const TIP_LINES: usize = 14;
const TIP_WIDTH: i32 = 260;

/// `line` cut at its spaces into lines no wider than `most` (by `width`): a
/// word wider than that alone on its line.
fn wrap(line: &str, most: i32, width: impl Fn(&[u16]) -> i32) -> Vec<Vec<u16>> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in line.split_whitespace() {
        let next = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if !cur.is_empty() && width(&vi::ui::wide(&next)) > most {
            lines.push(vi::ui::wide(&cur));
            cur = word.to_string();
        } else {
            cur = next;
        }
    }
    if !cur.is_empty() {
        lines.push(vi::ui::wide(&cur));
    }
    lines
}

/// Where `marks` go on a minimap whose pictures fill window rectangle
/// `inside` (`to_px` places a world point, `mid` its middle): only world
/// `world`'s; a camp or a coach stop where it lies, while it is on the
/// pictures (`r` its half-width); a quest's where it lies or on the rim of
/// `edge` while farther, pointing the way, and after the others, so drawn
/// over them.
fn place_marks<'a>(marks: &[&'a Mark], world: i32, to_px: &dyn Fn([f32; 2]) -> [f32; 2], inside: [f32; 4], r: f32, mid: [f32; 2], edge: [f32; 4]) -> Vec<([f32; 2], &'a Mark)> {
    let here = || marks.iter().copied().filter(move |m| m.world == world);
    let mut placed: Vec<([f32; 2], &Mark)> = here()
        .filter(|m| !m.kind.quest())
        .map(|m| (to_px(m.at), m))
        .filter(|(q, _)| q[0] >= inside[0] - r && q[0] <= inside[2] + r && q[1] >= inside[1] - r && q[1] <= inside[3] + r)
        .collect();
    placed.extend(here().filter(|m| m.kind.quest()).map(|m| (to_rim(mid, to_px(m.at), edge), m)));
    placed
}

/// The mark whose tooltip shows with the mouse's pointer at window pixel
/// `c`: the topmost (the last placed) within `reach` of it.
fn mark_under<'a>(placed: &[([f32; 2], &'a Mark)], c: [f32; 2], reach: f32) -> Option<([f32; 2], &'a Mark)> {
    placed.iter().rev().find(|(q, _)| (q[0] - c[0]).hypot(q[1] - c[1]) <= reach).copied()
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

    /// The mouse's pointer moved to window pixel `at` (none: it left the
    /// window): a mark under it on the minimap says its tooltip.
    pub fn mini_hover(&mut self, at: Option<[f32; 2]>) {
        self.mini.cursor = at;
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
        if self.mini.map_marks.as_ref().is_none_or(|(world, _)| *world != cx.world) {
            self.mini.map_marks = Some((cx.world, map_marks(cx.world)));
        }
        let Some((p, heading)) = cx.player.filter(|_| !self.open) else {
            self.mini.wanted.clear();
            return;
        };
        let frame = self.frame;
        let waypoint = self.waypoints.get(&cx.world).copied();
        let group_waypoint = self.group_waypoint(cx.world);
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
        let px_line = window_from_frame(size, [0.0, 1.0])[1] - window_from_frame(size, [0.0, 0.0])[1];
        let (dot, dot_o) = dot_radius(size);
        let font = l.assets.font;
        // where the marks go: the map's own camps and coaches (Map Markers),
        // then the rest it is given; the quests' on the rim while farther, a
        // glyph's half height in from it
        let half = font.map_or(8.0, |fo| ui.fonts[fo].height as f32 / 2.0) * px_line;
        let edge = [inside[0] + half * 0.6, inside[1] + half, inside[2] - half * 0.6, inside[3] - half];
        let own = own_marks(&self.mini.map_marks, cx.world, crate::settings::map_markers());
        let all: Vec<&Mark> = own.iter().chain(&mv.marks).collect();
        let placed = place_marks(&all, cx.world, &to_px, inside, MARK_REACH * px_line, mid, edge);
        let on = |q: [f32; 2], m: f32| q[0] >= inside[0] - m && q[0] <= inside[2] + m && q[1] >= inside[1] - m && q[1] <= inside[3] + m;
        // the coaches (the stable's horseshoe, the map's: off the disc), then
        // the camps' XP badges over them, as the map draws them, smaller;
        // the creatures and players over those (a camp's own among them)
        let from = ui.quads.len();
        for &(q, m) in &placed {
            if m.kind == MarkKind::Coach {
                super::markers::draw_horseshoe(ui, white, self.icon.tex, f(q), HORSESHOE, 1.0);
            }
        }
        for &(q, m) in &placed {
            if m.kind == MarkKind::Camp {
                badge(ui, white, font, f(q));
            }
        }
        for q in &mut ui.quads[from..] {
            q.clip = cut;
        }
        for &(at, color) in &mv.dots {
            let q = to_px(at);
            if on(q, dot + dot_o) {
                square_dot(ui, white, cut, f(q), dot / px_line, dot_o / px_line, color);
            }
        }
        // the target: a white ring round its dot
        if let Some(t) = mv.target {
            let q = to_px(t);
            let (r, o) = (dot * 2.4, dot_o.max(0.75));
            if on(q, r + o) {
                for (rr, color) in [(r + o, OUTLINE), (r, [1.0; 4])] {
                    let t = o.max(1.0);
                    for e in [[q[0] - rr, q[1] - rr, q[0] + rr, q[1] - rr + t], [q[0] - rr, q[1] + rr - t, q[0] + rr, q[1] + rr], [q[0] - rr, q[1] - rr, q[0] - rr + t, q[1] + rr], [q[0] + rr - t, q[1] - rr, q[0] + rr, q[1] + rr]] {
                        plain(ui, white, corners(e).map(f), color, cut);
                    }
                }
            }
        }
        // the group's members, their compass colours, as the map has them
        for (pos, index) in &cx.group {
            let q = to_px([pos[0], pos[2]]);
            let (r, o) = view::member_radius(size);
            if on(q, r + o) {
                square_dot(ui, white, cut, f(q), r / px_line, o / px_line, super::member_color(*index));
            }
        }
        // the waypoints, on the rim while farther than the minimap shows: the
        // group's (`group_waypoint`) under the player's own
        for (at, color) in [(group_waypoint, super::group_waypoint::COLOR), (waypoint, WAYPOINT)] {
            let Some(at) = at else { continue };
            let (r, o) = view::waypoint_radius(size);
            let (r, o) = (r * 0.8, o);
            let edge = [inside[0] + r + o, inside[1] + r + o, inside[2] - r - o, inside[3] - r - o];
            let q = to_rim(mid, to_px(at), edge);
            for (rr, color) in [(r + o * 1.5, OUTLINE), (r, color)] {
                plain(ui, white, diamond(q, rr).map(f), color, cut);
            }
        }
        let mut r = l.assets.raster(ui);
        if let Some(vui) = cx.vui {
            // the border, a window's frame round it, placed as the HUD's right side is
            r.ui.anchor = Anchor::Right;
            r.ui.vertical = Vertical::Top;
            vui.blit_frame(&mut r, [FRAME[0], FRAME[1]], [FRAME[2] - FRAME[0], FRAME[3] - FRAME[1]], &vi::ui::frame_sets()[0], 3, 1.0, true);
            // north, up: the compass turns with the camera, the minimap does not
            let i = inner();
            let h = vui.font_height(&r);
            vui.draw_centered_string_ex(&mut r, &vi::ui::wide("N"), (i[0] + i[2]) / 2, i[1] + h / 2, [1.0, 1.0, 1.0, 0.85], 3, true);
            // the quests' "!" and "?" over the other marks, where they were
            // placed in the window (cut to the pictures as the marks are)
            r.ui.anchor = Anchor::Middle;
            r.ui.vertical = Vertical::Middle;
            let from = r.ui.quads.len();
            for &(q, m) in placed.iter().filter(|(_, m)| m.kind.quest()) {
                let s = if m.kind == MarkKind::QuestOffer { "!" } else { "?" };
                let p = f(q);
                vui.draw_centered_string_ex(&mut r, &vi::ui::wide(s), p[0].round() as i32, p[1].round() as i32, QUEST, 3, true);
            }
            for q in &mut r.ui.quads[from..] {
                q.clip = cut;
            }
        }
        // the player's dart in the middle, over every mark, a little smaller
        // than the map's
        let (dr, o) = view::marker_radius(size);
        for (rr, color) in [(dr * 0.85 + o, OUTLINE), (dr * 0.85, super::MARKER)] {
            plain(r.ui, white, view::dart(mid, heading, rr).map(f), color, cut);
        }
        // the card of the mark under the mouse's pointer, left of the frame,
        // drawn as the map's cards are
        let reach = (MARK_REACH * px_line).max(half) + px_line;
        if let (Some(vui), Some((q, m))) = (cx.vui, self.mini.cursor.and_then(|c| mark_under(&placed, c, reach))) {
            r.ui.anchor = Anchor::Middle;
            r.ui.vertical = Vertical::Middle;
            let lines: Vec<(Vec<u16>, [f32; 4])> = m
                .tip
                .iter()
                .flat_map(|(l, c)| wrap(l, TIP_WIDTH, |s| vui.string_width(&r, s)).into_iter().map(move |l| (l, *c)))
                .take(TIP_LINES)
                .collect();
            if !lines.is_empty() {
                // the map's card, closer set: it is over the game
                let line = vui.font_height(&r) - 1;
                let (pad, wide) = (6.0, lines.iter().map(|(l, _)| vui.string_width(&r, l)).max().unwrap_or(0) as f32);
                let (w, hgt) = (wide + pad * 2.0, lines.len() as f32 * line as f32 + pad * 2.0);
                // left of the frame, level with the mark, below the name
                // plate (the HUD's pass, after this one, would cover it), in
                // the window
                let (corner, at) = (f([0.0, 0.0]), f([outer[0], q[1]]));
                let (plate, bottom) = (f([0.0, window_rect(size, PLATE)[3]])[1] + 1.0, f([0.0, size.1 as f32])[1]);
                let x = (at[0] - 6.0 - w).max(corner[0] + 1.0);
                let y = (at[1] - hgt / 2.0).min(bottom - hgt - 1.0).max(plate);
                let from = r.ui.quads.len();
                plain(r.ui, white, corners([x - 1.0, y - 1.0, x + w + 1.0, y + hgt + 1.0]), OUTLINE, None);
                plain(r.ui, white, corners([x, y, x + w, y + hgt]), CARD_BACK, None);
                vui.stretch_blit_ex(&mut r, 61, [x.round() as i32, y.round() as i32], [w.round() as i32, hgt.round() as i32], 3, 0.75);
                for (k, (l, c)) in lines.iter().enumerate() {
                    vui.draw_string_ex(&mut r, l, (x + pad).round() as i32, (y + pad).round() as i32 + k as i32 * line, *c, true);
                }
                for q in &mut r.ui.quads[from..] {
                    q.clip = Some(render::ui::WHOLE);
                }
            }
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
/// outline's, in window pixels: 1.2 and 0.4 of the frame's lines, half a
/// group member's (`view::member_radius`, 2.5).
pub fn dot_radius(size: (u32, u32)) -> (f32, f32) {
    let line = window_from_frame(size, [0.0, 1.0])[1] - window_from_frame(size, [0.0, 0.0])[1];
    (1.2 * line, 0.4 * line)
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
        // the quick menu's title bar: at (430, 25), 162 across, its art 19 high
        let plate = PLATE;
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
        assert!((four[0] - FRAME[0] as f32).abs() < 1e-3 && (four[2] - FRAME[2] as f32).abs() < 1e-3, "{four:?}");
        assert!((four[1] - FRAME[1] as f32 * k).abs() < 1e-3 && (four[3] - FRAME[3] as f32 * k).abs() < 1e-3, "{four:?}");
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

    /// The marks go where they lie, the world's own only: a camp past the
    /// pictures is not drawn, a quest's is kept on the rim along its way, and
    /// the quests' come after the rest (drawn over them). The controls:
    /// another world's mark, and a quest's on the pictures, which stays where
    /// it lies.
    #[test]
    fn the_quests_marks_point_the_way_from_the_rim() {
        let mark = |kind, world, at: [f32; 2]| Mark { kind, world, at, tip: Vec::new() };
        let marks = [
            mark(MarkKind::QuestOffer, 0, [1000.0, 0.0]),
            mark(MarkKind::Camp, 0, [10.0, 20.0]),
            mark(MarkKind::Camp, 0, [1000.0, 0.0]),
            mark(MarkKind::Coach, 1, [0.0, 0.0]),
            mark(MarkKind::QuestStep, 0, [-20.0, -30.0]),
        ];
        // 100 pixels round (0, 0), a world unit a pixel
        let (inside, mid) = ([0.0, 0.0, 100.0, 100.0], [50.0, 50.0]);
        let to_px = |q: [f32; 2]| [mid[0] + q[0], mid[1] + q[1]];
        let marks: Vec<&Mark> = marks.iter().collect();
        let placed = place_marks(&marks, 0, &to_px, inside, 3.0, mid, [5.0, 8.0, 95.0, 92.0]);
        let kinds: Vec<MarkKind> = placed.iter().map(|(_, m)| m.kind).collect();
        assert_eq!(kinds, [MarkKind::Camp, MarkKind::QuestOffer, MarkKind::QuestStep]);
        let near = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3;
        assert!(near(placed[0].0, [60.0, 70.0]), "the camp where it lies: {:?}", placed[0].0);
        assert!(near(placed[1].0, [95.0, 50.0]), "the far quest on the east rim: {:?}", placed[1].0);
        assert!(near(placed[2].0, [30.0, 20.0]), "the near one where it lies: {:?}", placed[2].0);
        assert!(place_marks(&marks, 2, &to_px, inside, 3.0, mid, [5.0, 8.0, 95.0, 92.0]).is_empty(), "no marks of world 2");
    }

    /// The map's camps and coaches come in as marks of their kinds, a
    /// world's only, each with its card (the title first); none with Map
    /// Markers off, nor for another world than the one they were made for.
    #[test]
    fn the_maps_camps_and_coaches_are_its_marks() {
        let marks = map_marks(0);
        let camps = marks.iter().filter(|m| m.kind == MarkKind::Camp).count();
        let coaches = marks.iter().filter(|m| m.kind == MarkKind::Coach).count();
        assert_eq!(camps, super::super::markers::of_world(0).filter(|m| matches!(m, super::super::markers::Marker::Camp(_))).count());
        assert!(camps > 0 && coaches > 0, "Tunaria has both: {camps} camps, {coaches} coaches");
        assert!(marks.iter().all(|m| m.world == 0 && !m.tip.is_empty() && m.tip[0].1 == super::super::markers::TITLE));
        let freeport = marks.iter().find(|m| m.kind == MarkKind::Coach && m.tip.iter().any(|(l, _)| l.contains("Ronks"))).expect("Coachman Ronks's coach");
        assert!((freeport.at[0] - 25274.0).abs() < 100.0 && (freeport.at[1] - 15719.0).abs() < 100.0, "{:?}", freeport.at);
        let made = Some((0, marks));
        assert!(!own_marks(&made, 0, true).is_empty());
        assert!(own_marks(&made, 0, false).is_empty(), "Map Markers off");
        assert!(own_marks(&made, 2, true).is_empty(), "made for another world");
        assert!(own_marks(&None, 0, true).is_empty());
    }

    /// The tooltip is the topmost mark's under the pointer: a quest's over a
    /// camp's at the same place, the camp's alone. The control: the pointer
    /// away from both.
    #[test]
    fn the_tooltip_is_the_topmost_marks_under_the_pointer() {
        let camp = Mark::text(MarkKind::Camp, 0, [0.0; 2], "Orc camp\nLevels 10-14");
        let quest = Mark::text(MarkKind::QuestOffer, 0, [0.0; 2], "A quest");
        assert_eq!(camp.tip, [("Orc camp".to_string(), TIP_TITLE), ("Levels 10-14".to_string(), TIP_TEXT)], "the title's colour, then the text's");
        let placed = [([50.0, 50.0], &camp), ([52.0, 50.0], &quest)];
        assert_eq!(mark_under(&placed, [51.0, 51.0], 4.0).map(|(_, m)| m.kind), Some(MarkKind::QuestOffer));
        assert_eq!(mark_under(&placed[..1], [51.0, 51.0], 4.0).map(|(_, m)| m.kind), Some(MarkKind::Camp));
        assert_eq!(mark_under(&placed, [60.0, 50.0], 4.0), None, "the control: away from both");
    }

    /// A tooltip's line wider than it goes on over the next, cut at its
    /// spaces; a word wider than it alone. The control: a line that fits
    /// stays one.
    #[test]
    fn a_long_tooltip_line_wraps_at_its_spaces() {
        let width = |s: &[u16]| s.len() as i32 * 8;
        let text = |ls: Vec<Vec<u16>>| ls.iter().map(|l| String::from_utf16_lossy(l)).collect::<Vec<_>>();
        assert_eq!(text(wrap("Two camps of orcs by the river", 80, width)), ["Two camps", "of orcs by", "the river"]);
        assert_eq!(text(wrap("Kerran-Village-Elders here", 80, width)), ["Kerran-Village-Elders", "here"]);
        assert_eq!(text(wrap("Fits well", 80, width)), ["Fits well"], "the control");
    }

    /// The wheel over it zooms it between its limits (up: in); it is found
    /// only where it was drawn, while it is up and the map is not. The
    /// control: not drawn this frame, nothing is there.
    #[test]
    fn the_wheel_zooms_it_between_its_limits() {
        let mut m = Map::default();
        assert_eq!(m.mini.scale, SCALE);
        m.mini_wheel(1.0);
        assert!(m.mini.scale < SCALE, "up: in");
        for _ in 0..40 {
            m.mini_wheel(1.0);
        }
        assert_eq!(m.mini.scale, SCALE_LIMITS.0);
        for _ in 0..40 {
            m.mini_wheel(-1.0);
        }
        assert_eq!(m.mini.scale, SCALE_LIMITS.1);
        m.mini.frame_px = [10.0, 10.0, 20.0, 20.0];
        assert!(!m.mini_contains([15.0, 15.0]), "the control: not drawn this frame");
        m.mini.up = true;
        assert!(m.mini_contains([15.0, 15.0]) && !m.mini_contains([25.0, 15.0]));
        m.open = true;
        assert!(!m.mini_contains([15.0, 15.0]), "the map up covers it");
        m.open = false;
        m.mini_off();
        assert!(!m.mini_contains([15.0, 15.0]) && m.mini.wanted.is_empty(), "off: gone, its tiles no longer kept");
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
