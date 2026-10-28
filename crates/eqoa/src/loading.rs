//! The loading screen: `VIClient::LoadingProgress` (CLIENT 0x00BBF0D8), the
//! progress callback `VILoader` calls while it loads. `EnterWorld` sets it
//! round `VIScene::StreamFillCache` at the arrival of every move (0x00B2's
//! `MoveTo` and `Respawn`, the 0x000D dump; another world's file read first),
//! and `LoadModels` at world entry; the game's frame stops meanwhile, and
//! every 30 ms the callback ticks the clock and the net and draws the screen
//! by itself. Its art is CLIENT's and UI.CSF's (not SETUP's: SETUP shows the
//! launcher's `VIDisplayLoadingScreen` while it boots):
//!
//! - the 2D half (`draw_2d`), the Frontiers layout (`VIGetExpansionFlags() &
//!   2`) or the original game's forty pixels lower: "EVERQUEST ONLINE" and
//!   "ADVENTURES" out of "sheet1" (the HUD's sheet, which the compass is cut
//!   from too), the Frontiers logo (atlas 243), and a bar (33, 34, 33
//!   mirrored) with "Loading, please wait" on it (203). There is no progress
//!   measure: the bar is art, the same from start to end;
//! - then UI.CSF's "Loading_Gears", an HSprite of two wooden gears on two
//!   nodes, turning by the scene's clock, rastered under a model-view that
//!   turns it half round about y 0.7 ahead of the eye, through the camera's
//!   projection (`EnterGameLoad`'s `VICamera::Init`: 55 degrees, 10/7, 0.1 to
//!   900) into `Viewport(500, 350, 642, 450)`, the bottom right corner, under
//!   `EnterWorld`'s `SetDefaultLights(identity)`; the frame cleared black
//!   (`BeginScene`, no scene).
//!
//! Here it is up from a teleport, a world change or world entry until the
//! zones round the arrival are in and drawn (`stream::Streamer::settled`),
//! the walker held at the arrival and put down there once they are, as
//! `StreamFillCache` then `SetLocation` do. A move of 0x00B2's (`Respawn`)
//! has its floor cast again then (`Wait::respawn`): the game casts after
//! `EnterWorld` streamed the zones in, where a far move here casts before
//! they come and finds nothing. Loads here are fast where the
//! PS2's took seconds: natively a frame or two, in a page as long as its
//! reads. In a window wider than 4:3 the 2D half keeps to the middle and the
//! gears to the right side's corner, and in one narrower than 4:3 to the
//! bottom's (the client's own, as the HUD's pieces keep to the edges).

use crate::ui::{Layout, Vui, icon_id};
use render::ui::Anchor;
use vi::fpu::{Ee, f};
use vi::geom::Matrix44;
use vi::hsprite::{AnimScene, HSprite};

/// Past this the screen comes down whatever is still out (a read that never
/// answers): the client's, where the game would wait for good.
pub const CAP_MS: i64 = 60_000;

/// A move at least this far, or into another world, keeps the screen up at
/// least `FAR_HOLD_MS`: a PS2 shows it while `EnterWorld` reads the zones
/// round a far arrival off the disc, seconds of it, where this client has
/// them in about a millisecond and the screen went by unseen, the world
/// jumping from one place to the other (F3 20261025-171303, Return Home:
/// "even a flash will do so its not so jarring"; the user's choice,
/// 2026-10-25), unless the player chose Instant (`Screen::hold_far`). The
/// client's, not the game's; a short hop is as before.
pub const FAR_MOVE: f32 = 200.0;
pub const FAR_HOLD_MS: i64 = 1000;

/// Whether a move from `from` to `to` is a far one (`FAR_MOVE`).
pub fn far(from: [f32; 3], to: [f32; 3]) -> bool {
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() >= FAR_MOVE
}

/// `LoadingProgress`'s 2D half: the logo out of "sheet1", the expansion's
/// logo and the bar, the Frontiers layout when `frontiers`
/// (`VIGetExpansionFlags() & 2`, the disc's flags this client sends as its
/// own: `net::session::EXPANSION_FLAGS`), else the original game's.
pub fn draw_2d(vui: Option<&Vui>, r: &mut dyn Layout, frontiers: bool) {
    r.anchor(Anchor::Middle);
    let sheet = icon_id("sheet1");
    // VIUI::Blit(id, at, size, origin): the raster's texels origin..+size
    let dy = if frontiers { 0 } else { 40 };
    r.blit(sheet, 0, [82, 80 + dy], [37, 0], [475, 107], 0, 1.0);
    r.blit(sheet, 0, [177, 197 + dy], [31, 113], [288, 33], 0, 1.0);
    let Some(vui) = vui else { return };
    if frontiers {
        vui.blit_ex(r, 243, [143, 247], 0, 1.0, 0);
    }
    let y = if frontiers { 325 } else { 285 };
    vui.blit_ex(r, 33, [190, y], 0, 1.0, 0);
    vui.blit_horz(r, [206, y], 34, 228, 0, 1.0, 0);
    vui.blit_ex(r, 33, [434, y], 0, 1.0, 1);
    vui.blit_ex(r, 203, [228, y + 7], 0, 1.0, 0);
}

/// The model-view `LoadingProgress` sets for the gears: the identity with x
/// and z negated, 0.7 ahead of the eye.
pub fn model_view() -> Matrix44<Ee> {
    let mut m = Matrix44::<Ee>::identity();
    m.m[0].x = -m.m[0].x;
    m.m[2].z = -m.m[2].z;
    m.m[3].z = f(0.7);
    m
}

/// `Viewport(500, 350, 642, 450)`: inclusive, so 143 by 101 frame pixels
/// about (571.5, 400.5), its half sizes 71.5 and 50.5 (`VIRaster::Viewport`).
const CORNER: [f32; 4] = [571.5, 400.5, 71.5, 50.5];

/// The camera's projection into the corner, as one matrix from the eye's
/// space (x right, y up, z ahead, as `VICamera` builds it) to a `size`
/// target's clip space: `VIRaster::Frustum` for 55 degrees and 10/7 from
/// 0.1 to 900 (depth 0..1 here), then the corner's place in the target, the
/// corner kept at the right side in a wider window (`render::ui::side`) and
/// at the bottom in a taller one (`render::ui::top`).
pub fn corner_view_proj(size: (u32, u32)) -> glam::Mat4 {
    let proj = glam::camera::lh::proj::directx::perspective(55f32.to_radians(), 10.0 / 7.0, 0.1, 900.0);
    let (tw, th) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let (side, top) = (render::ui::side(size), render::ui::top(size));
    let c = render::ui::window_from_frame(size, [CORNER[0] + side, CORNER[1] + top]);
    let o = render::ui::window_from_frame(size, [0.0, 0.0]);
    let e = render::ui::window_from_frame(size, [CORNER[2], CORNER[3]]);
    let (hx, hy) = (e[0] - o[0], e[1] - o[1]);
    // NDC x and y (up) of the corner's middle, and its half sizes in NDC
    let (ax, ay) = (c[0] / tw * 2.0 - 1.0, 1.0 - c[1] / th * 2.0);
    let (bx, by) = (hx / tw * 2.0, hy / th * 2.0);
    let corner = glam::Mat4::from_cols(glam::Vec4::new(bx, 0.0, 0.0, 0.0), glam::Vec4::new(0.0, by, 0.0, 0.0), glam::Vec4::Z, glam::Vec4::new(ax, ay, 0.0, 1.0));
    corner * proj
}

/// What the gears are drawn through: `corner_view_proj`, `SetDefaultLights`'
/// lights (ambient 0.4, the sun 0.8 travelling along -0.577 on every axis),
/// no fog, the frame cleared black.
pub fn view(size: (u32, u32)) -> render::View {
    render::View {
        view_proj: corner_view_proj(size),
        eye: glam::Vec3::ZERO,
        sun_dir: glam::Vec3::splat(-0.577),
        sun: glam::Vec3::splat(0.8),
        ambient: glam::Vec3::splat(0.4),
        fog: None,
        point: None,
        clear: glam::Vec3::ZERO,
        sky_view_proj: None,
    }
}

/// UI.CSF's "Loading_Gears" (`m_loadSprite`): its hierarchy, clip and the
/// meshes on its nodes.
pub struct Gears {
    hs: HSprite<Ee>,
    anims: AnimScene<Ee>,
    pub parts: Vec<(i32, esf::Model)>,
    /// the clock at the first pose: the clip turns from there, whatever
    /// the clock counts from (F3 crashes 20261028: posed on the DRDP clock,
    /// wall-clock milliseconds as a u32, the gears' first step from the
    /// load's 0 passed 2^31 at 00:54 UTC and every loading screen stopped the
    /// client)
    start: Option<i64>,
}

impl Gears {
    /// None until UI.CSF is in.
    pub fn load(lib: &esf::Library) -> Option<Self> {
        let r = lib.lookup(vi::hsprite::hash_ref_id("Loading_Gears"))?;
        let mut anims = AnimScene::default();
        let mut cache = vi::character::AnimCache::new();
        let (hs, attach) = vi::character::load_hsprite(lib, r, &mut anims, &mut cache)?;
        let parts: Vec<(i32, esf::Model)> = attach.iter().filter_map(|&(sid, node)| Some((node, lib.build_model(lib.lookup(sid)?)?))).collect();
        (!parts.is_empty()).then_some(Gears { hs, anims, parts, start: None })
    }

    /// `scene.SetTime(now)`, then the sprite's `Raster` under the
    /// model-view: each part's transform at `now` ms (the node's world
    /// matrix times the centre transform), in the eye's space. The scene's
    /// time is `now` since the first pose (`start`), begun again should the
    /// clock go back.
    pub fn pose(&mut self, now: i64) -> Vec<Matrix44<Ee>> {
        let mv = model_view();
        let start = match self.start {
            Some(s) if s <= now => s,
            _ => *self.start.insert(now),
        };
        self.anims.time = now - start;
        let lod = self.hs.calc_lod_level_index(&mv);
        self.hs.process(lod, &self.anims);
        let centre = self.hs.calc_trans_about_center(&mv, &self.anims);
        self.parts.iter().map(|(node, _)| usize::try_from(*node).ok().and_then(|n| self.hs.nodes.get(n)).map_or(centre, |n| Matrix44::mul(&n.world, &centre))).collect()
    }
}

/// What the screen waits for: the arrival, and whether another world's
/// file is still being read (`Play::world_entered` ends that).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wait {
    pub at: [f32; 3],
    pub heading: f32,
    pub entering: bool,
    /// the move is 0x00B2's (`Respawn`): its floor is cast again once the
    /// zones are in (`Play::loading_step`); a dump's (`Msg000D`'s
    /// `SetLocation`) and world entry's stand as given
    pub respawn: bool,
    /// the session's clock when it went up
    pub since: i64,
    /// where the walk is held meanwhile: where it was put (the move's
    /// placing, or a new world's walk's), its place and heading
    pub held: Option<([f32; 3], f32)>,
}

/// The screen across frames: what it waits for, and the gears on the GPU.
#[derive(Default)]
pub struct Screen {
    pub wait: Option<Wait>,
    /// `EQOA_LOADING_HOLD=ms` (in a page `?loadinghold=ms`): up at least
    /// that long (a headless shot of it, where the loads take a frame)
    hold: i64,
    gears: Option<Gears>,
    /// how many files the library held when the gears were last looked for
    looked: usize,
    models: Vec<render::GpuModel>,
    /// Whether the 2D layer's kept quads are the screen's (`Play::hud`).
    pub kept: bool,
}

impl Screen {
    /// Up, waiting for the zones round `at` (and, `entering`, for another
    /// world's file first), the move `Respawn`'s when `respawn`. A move
    /// while it is up waits for the new place.
    pub fn begin(&mut self, at: [f32; 3], heading: f32, entering: bool, respawn: bool, now: i64) {
        let up = self.wait.is_some();
        let since = self.wait.map_or(now, |w| w.since);
        self.wait = Some(Wait { at, heading, entering: entering || self.wait.is_some_and(|w| w.entering), respawn, since, held: None });
        #[cfg(target_arch = "wasm32")]
        let hold = crate::web::page_value("loadinghold");
        #[cfg(not(target_arch = "wasm32"))]
        let hold = std::env::var("EQOA_LOADING_HOLD").ok();
        let hold = hold.and_then(|v| v.parse().ok()).unwrap_or(0);
        // a move while it is up keeps the hold the first asked for
        self.hold = if up { self.hold.max(hold) } else { hold };
    }

    pub fn up(&self) -> bool {
        self.wait.is_some()
    }

    /// The screen up at least `ms` from when it went up; a longer hold
    /// already asked for stands.
    pub fn hold_at_least(&mut self, ms: i64) {
        self.hold = self.hold.max(ms);
    }

    /// A far move's hold (`FAR_HOLD_MS`), unless the player chose Instant
    /// (Extended > UI > Loading Screens, `settings::loading_longer`).
    pub fn hold_far(&mut self) {
        if crate::settings::loading_longer() {
            self.hold_at_least(FAR_HOLD_MS);
        }
    }

    /// The world it waits for is entered: the zones are what it waits on now,
    /// and the new world's walk is the one held.
    pub fn entered(&mut self) {
        if let Some(w) = self.wait.as_mut() {
            w.entering = false;
            w.held = None;
        }
    }

    /// Where to hold the walk, given where it stands now: the first place it
    /// stood since the wait began (or the world was entered); none while
    /// another world is read (the old world's walk is let go).
    pub fn hold_at(&mut self, now_at: [f32; 3], now_heading: f32) -> Option<([f32; 3], f32)> {
        let w = self.wait.as_mut().filter(|w| !w.entering)?;
        Some(*w.held.get_or_insert((now_at, now_heading)))
    }

    /// Where it waits for the zones (none while another world is read).
    pub fn waits_at(&self) -> Option<[f32; 3]> {
        self.wait.filter(|w| !w.entering).map(|w| w.at)
    }

    /// Down when the zones round the arrival are in (`settled`) and it has
    /// been up the hold, or past the cap: the wait it ends, for the walker to
    /// be put at.
    pub fn end(&mut self, settled: bool, now: i64) -> Option<Wait> {
        let w = self.wait?;
        let up = now - w.since;
        if (settled && !w.entering && up >= self.hold) || up >= CAP_MS {
            self.wait = None;
            return Some(w);
        }
        None
    }

    /// The art swapped (F9, `era`): the gears made again out of the art now,
    /// as they are next drawn.
    pub fn art_changed(&mut self) {
        self.gears = None;
        self.models.clear();
        self.looked = usize::MAX;
    }

    /// The gears for this frame: uploaded once UI.CSF is in, posed at
    /// `now`. Only while it is up.
    pub fn frame(&mut self, lib: &esf::Library, renderer: &mut render::Renderer, device: &render::wgpu::Device, queue: &render::wgpu::Queue, now: i64) {
        if !self.up() {
            return;
        }
        if self.gears.is_none() && self.looked != lib.files.len() {
            self.looked = lib.files.len();
            self.gears = Gears::load(lib);
            if let Some(g) = &self.gears {
                let textures = render::library_textures(lib);
                self.models = g.parts.iter().map(|(_, m)| renderer.upload(device, queue, m, &[render::IDENTITY], &textures)).collect();
            }
        }
        if let Some(g) = self.gears.as_mut() {
            for (m, gpu) in g.pose(now).iter().zip(&self.models) {
                renderer.set_instances(queue, gpu, &[m.to_cols()]);
            }
        }
    }

    /// The gears' models, while it is up.
    pub fn models(&self) -> impl Iterator<Item = &render::GpuModel> {
        self.models.iter().filter(|_| self.up())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A point `m` puts the eye's space through, in the game's float model.
    fn at(m: &Matrix44<Ee>, p: [f32; 3]) -> [f32; 3] {
        m.mul_point(&vi::vect::Vect3::of(p[0], p[1], p[2])).to_array()
    }

    /// A blit: its surface, place, texels, size and flags.
    type Blit = (u32, [i32; 2], [i32; 2], [i32; 2], u32);

    /// A raster that records each blit and the anchors set.
    #[derive(Default)]
    struct Rec(Vec<Blit>, Vec<Anchor>);
    impl Layout for Rec {
        fn anchor(&mut self, a: Anchor) {
            self.1.push(a);
        }
    }
    impl vi::ui::Raster for Rec {
        fn blit(&mut self, id: u32, flags: u32, at: [i32; 2], a: [i32; 2], b: [i32; 2], _: i32, _: f32) {
            self.0.push((id, at, a, b, flags));
        }
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
            [0, 0, 639, 447]
        }
        fn set_viewport(&mut self, _: [i32; 4]) {}
    }

    /// The 2D half as `LoadingProgress` makes it: the two pieces of the logo
    /// out of "sheet1" (`VIUI::Blit` hands the raster its origin and size the
    /// other way round), then with the atlas the Frontiers logo and the bar:
    /// its caps either side of 228 of middle, the right cap mirrored, the
    /// words on it; all with the frame.
    #[test]
    fn the_screen_is_the_logo_and_the_bar() {
        let sheet = icon_id("sheet1");
        let mut r = Rec::default();
        draw_2d(None, &mut r, true);
        assert_eq!(r.0, vec![(sheet, [82, 80], [37, 0], [475, 107], 0), (sheet, [177, 197], [31, 113], [288, 33], 0)]);
        assert_eq!(r.1, vec![Anchor::Middle]);
        // the original game's is forty pixels lower
        let mut o = Rec::default();
        draw_2d(None, &mut o, false);
        assert_eq!((o.0[0].1, o.0[1].1), ([82, 120], [177, 237]));
        // with an atlas: each entry numbering itself on the sheet, the
        // sizes SUPPORT's
        let mut atlas: Vec<vi::ui::UiTexEntry> = (0..252).map(|i| vi::ui::UiTexEntry { x00: i, row: 1, uv0: [0, 0], uv1: [8, 8], mode: 0, alpha: 1.0, flags: 0 }).collect();
        atlas[33].uv1 = [16, 28];
        atlas[34].row = 19;
        atlas[34].uv1 = [32, 28];
        atlas[203].uv1 = [180, 16];
        atlas[243].row = 35;
        atlas[243].uv1 = [346, 64];
        let vui = Vui::new(Default::default(), Default::default(), atlas);
        let mut a = Rec::default();
        draw_2d(Some(&vui), &mut a, true);
        let places: Vec<([i32; 2], [i32; 2], u32)> = a.0[2..].iter().map(|b| (b.1, b.3, b.4)).collect();
        assert_eq!(places[0], ([143, 247], [346, 64], 0), "the Frontiers logo");
        assert_eq!(places[1], ([190, 325], [16, 28], 0), "the left cap");
        assert_eq!(places.last(), Some(&([228, 332], [180, 16], 0)), "the words");
        assert_eq!(places[places.len() - 2], ([434, 325], [16, 28], 1), "the right cap, mirrored");
        let middle: i32 = places[2..places.len() - 2].iter().map(|p| p.1[0]).sum();
        assert_eq!(middle, 228, "the middle across");
        // control: the original layout has no Frontiers logo, its bar 40 higher
        let mut b = Rec::default();
        draw_2d(Some(&vui), &mut b, false);
        assert_eq!(b.0[2].1, [190, 285]);
    }

    /// The model-view is half a turn about y, 0.7 ahead: a point on the
    /// sprite's +z comes toward the eye, its +x goes left, and its
    /// determinant is +1 (the winding is kept).
    #[test]
    fn the_gears_turn_half_round_ahead() {
        let m = model_view();
        assert_eq!(at(&m, [0.0, 0.0, 0.0]), [0.0, 0.0, 0.7]);
        let p = at(&m, [0.5, 0.25, 0.125]);
        assert_eq!(p, [-0.5, 0.25, 0.575]);
        let r = &m.m;
        let det = r[0].x.0 * (r[1].y.0 * r[2].z.0 - r[1].z.0 * r[2].y.0) - r[0].y.0 * (r[1].x.0 * r[2].z.0 - r[1].z.0 * r[2].x.0) + r[0].z.0 * (r[1].x.0 * r[2].y.0 - r[1].y.0 * r[2].x.0);
        assert_eq!(det, 1.0);
    }

    /// The camera's projection lands the eye's axis in the corner's middle,
    /// (571.5, 400.5) of the frame: on a 4:3 target where the frame is, on a
    /// wider one moved with the right side; the view's corner at the
    /// viewport's (`Viewport` is inclusive: 143 by 101); depth 0 to 1 from
    /// 0.1 to 900.
    #[test]
    fn the_corner_is_the_viewport_at_the_right() {
        let frame_at = |size: (u32, u32), p: glam::Vec3| {
            let c = corner_view_proj(size) * p.extend(1.0);
            let (x, y, z) = (c.x / c.w, c.y / c.w, c.z / c.w);
            let px = [(x + 1.0) * 0.5 * size.0 as f32, (1.0 - y) * 0.5 * size.1 as f32];
            (render::ui::frame_from_window(size, px), z)
        };
        let close = |a: [f32; 2], b: [f32; 2]| (a[0] - b[0]).abs() < 0.01 && (a[1] - b[1]).abs() < 0.01;
        let t = 27.5f32.to_radians().tan();
        for size in [(640, 480), (1280, 960)] {
            let (p, z) = frame_at(size, glam::Vec3::new(0.0, 0.0, 0.7));
            assert!(close(p, [571.5, 400.5]), "{size:?}: {p:?}");
            assert!((0.0..1.0).contains(&z));
            // up and right at the projection's edge: the viewport's top right
            let (q, _) = frame_at(size, glam::Vec3::new(t * 10.0 / 7.0, t, 1.0));
            assert!(close(q, [643.0, 350.0]), "{size:?}: {q:?}");
        }
        // 16:9: 106 whole frame pixels each side, the corner with the right one
        let (p, _) = frame_at((1920, 1080), glam::Vec3::new(0.0, 0.0, 0.7));
        assert!(close(p, [571.5 + 106.0, 400.5]), "{p:?}");
        // control: without the side it would sit in the frame's corner
        assert!(!close(p, [571.5, 400.5]));
        // two windows side by side (798x828): 85 whole frame pixels above
        // and below, the corner with the bottom; without, the frame's
        assert_eq!(render::ui::top((798, 828)), 85.0);
        let (p, _) = frame_at((798, 828), glam::Vec3::new(0.0, 0.0, 0.7));
        assert!(close(p, [571.5, 400.5 + 85.0]), "{p:?}");
        assert!(!close(p, [571.5, 400.5]));
        let (_, near) = frame_at((640, 480), glam::Vec3::new(0.0, 0.0, 0.1));
        let (_, far) = frame_at((640, 480), glam::Vec3::new(0.0, 0.0, 900.0));
        assert!(near.abs() < 1e-6 && (far - 1.0).abs() < 1e-6, "{near} {far}");
    }

    /// The wait: up at a move, held while another world is read, down once
    /// the zones round the arrival are in, or at the cap whatever is out; a
    /// second move while it is up keeps its start and waits for the new place.
    #[test]
    fn the_screen_waits_for_the_arrival() {
        let mut s = Screen::default();
        assert!(!s.up() && s.end(true, 0).is_none());
        s.begin([1.0, 2.0, 3.0], 0.5, false, true, 1000);
        assert_eq!(s.waits_at(), Some([1.0, 2.0, 3.0]));
        assert_eq!(s.end(false, 1500), None, "the zones not in");
        assert_eq!(s.end(true, 1600).map(|w| (w.at, w.heading, w.respawn)), Some(([1.0, 2.0, 3.0], 0.5, true)), "a teleport's: its floor cast again");
        assert!(!s.up());
        // a world change: nothing ends it until the world is entered
        s.begin([5.0, 0.0, 5.0], 0.0, true, true, 2000);
        assert_eq!(s.waits_at(), None);
        assert_eq!(s.end(true, 2100), None);
        s.begin([6.0, 0.0, 6.0], 1.0, false, false, 2200);
        assert_eq!(s.wait.map(|w| (w.since, w.entering)), Some((2000, true)), "a second move keeps the first's start and its world change");
        s.entered();
        assert_eq!(s.waits_at(), Some([6.0, 0.0, 6.0]));
        assert_eq!(s.end(true, 2300).map(|w| w.respawn), Some(false), "the second move's own: a dump's, as given");
        // the cap
        s.begin([0.0; 3], 0.0, true, false, 10_000);
        assert_eq!(s.end(false, 10_000 + CAP_MS - 1), None);
        assert!(s.end(false, 10_000 + CAP_MS).is_some());
    }

    /// A far move's hold (F3 20261025-171303): the screen stays up
    /// `FAR_HOLD_MS` though the zones are in at once, a move while it is up
    /// keeps that hold, and 200 is far where 199.9 is not. The control: with
    /// no hold the screen comes down the moment the zones are in.
    #[test]
    fn a_far_move_holds_the_screen_a_second() {
        let mut s = Screen::default();
        s.begin([0.0; 3], 0.0, false, true, 5000);
        s.hold_at_least(FAR_HOLD_MS);
        assert_eq!(s.end(true, 5000 + FAR_HOLD_MS - 1), None, "the zones are in, the screen held");
        s.begin([10.0, 0.0, 0.0], 0.0, false, true, 5500);
        assert_eq!(s.end(true, 5000 + FAR_HOLD_MS - 1), None, "a short move after it keeps the hold");
        assert!(s.end(true, 5000 + FAR_HOLD_MS).is_some());
        // the control: no hold asked for
        let mut c = Screen::default();
        c.begin([0.0; 3], 0.0, false, true, 5000);
        assert!(c.end(true, 5000).is_some(), "down as the zones are in");
        assert!(far([0.0; 3], [120.0, 0.0, 160.0]), "200 away");
        assert!(!far([0.0; 3], [119.9, 0.0, 160.0]), "under 200");
        assert!(far([0.0, 0.0, 0.0], [0.0, 200.0, 0.0]), "straight up counts");
    }

    /// The walk is held where it first stood once the wait began (where the
    /// move put it), not where it drifts to; not at all while another world
    /// is read; and a new world's walk is held where it starts.
    #[test]
    fn the_walk_is_held_where_the_move_put_it() {
        let mut s = Screen::default();
        assert_eq!(s.hold_at([1.0; 3], 0.0), None, "nothing up");
        s.begin([5.0, 54.0, 5.0], 1.0, false, true, 0);
        assert_eq!(s.hold_at([5.0, 54.0, 5.0], 1.0), Some(([5.0, 54.0, 5.0], 1.0)));
        assert_eq!(s.hold_at([5.0, 40.0, 7.0], 0.5), Some(([5.0, 54.0, 5.0], 1.0)), "drifted: back");
        // another move: its own place
        s.begin([9.0, 54.0, 9.0], 2.0, false, true, 10);
        assert_eq!(s.hold_at([9.0, 60.0, 9.0], 2.0), Some(([9.0, 60.0, 9.0], 2.0)));
        // a world change: the old world's walk is not held; the new one is
        s.begin([1.0, 2.0, 3.0], 0.0, true, true, 20);
        assert_eq!(s.hold_at([9.0, 60.0, 9.0], 2.0), None);
        s.entered();
        assert_eq!(s.hold_at([1.0, 9.0, 3.0], 0.0), Some(([1.0, 9.0, 3.0], 0.0)));
        // control: a hold that followed the walk would not take it back
        assert_ne!(s.hold_at([1.0, 2.0, 3.0], 0.0), Some(([1.0, 2.0, 3.0], 0.0)));
    }

    /// UI.CSF's "Loading_Gears" (SKIPs without the disc): two gear meshes on
    /// nodes 1 and 2, lit (black vertex colours), turning by the clock, and
    /// every vertex inside the camera's view, so inside the corner, at any
    /// time.
    #[test]
    fn the_gears_off_the_disc() {
        let iso_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut disc) = iso::Iso::open(&iso_path) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let mut lib = esf::Library::new();
        lib.add("UI.CSF", esf::Container::open(disc.read("DATA2/UI.CSF").unwrap()).unwrap());
        let mut g = Gears::load(&lib).expect("Loading_Gears in UI.CSF");
        assert_eq!(g.parts.iter().map(|p| p.0).collect::<Vec<_>>(), vec![1, 2]);
        assert!(g.parts.iter().all(|(_, m)| m.color.iter().all(|c| c[..3] == [0.0; 3])), "lit, not coloured");
        let vp = glam::camera::lh::proj::directx::perspective(55f32.to_radians(), 10.0 / 7.0, 0.1, 900.0);
        let mut firsts = Vec::new();
        for ms in (0..4000i64).step_by(250) {
            let parts = g.pose(ms);
            assert_eq!(parts.len(), 2);
            for (m, (_, model)) in parts.iter().zip(&g.parts) {
                for p in &model.pos {
                    let e = at(m, *p);
                    let c = vp * glam::Vec4::new(e[0], e[1], e[2], 1.0);
                    assert!(c.w > 0.0 && (c.x / c.w).abs() < 1.0 && (c.y / c.w).abs() < 1.0, "{ms} ms: {e:?} off the corner");
                }
            }
            firsts.push(at(&parts[0], g.parts[0].1.pos[0]));
        }
        // control: they turn, so a still pose would not pass for the clip
        assert!(firsts.iter().any(|p| (p[0] - firsts[0][0]).abs() > 0.01), "{firsts:?}");
        // F3 crashes 20261028: the DRDP clock (wall-clock milliseconds as a
        // u32) past 2^31 at the first pose stopped the client; the gears
        // turn the same from any clock, and through one that goes back
        for base in [0i64, 2_147_717_568, (1 << 32) - 1000] {
            let mut g = Gears::load(&lib).unwrap();
            let turned: Vec<[f32; 3]> = (0..4000i64).step_by(250).map(|ms| at(&g.pose(base + ms)[0], g.parts[0].1.pos[0])).collect();
            assert_eq!(turned, firsts, "from {base}");
        }
        let mut g = Gears::load(&lib).unwrap();
        g.pose(2_147_717_568);
        assert_eq!(g.pose(500).len(), 2, "a clock gone back");
    }
}
