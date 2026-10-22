//! `VIWndCharCustFrontiers` (CLIENT `viwndcharcustfrontiers.cpp`): Frontiers'
//! re-styling of a character from select's Options box: face, hair style,
//! hair length and hair colour, each a row (up and down) whose value left and
//! right change, wrapping; the character in its own viewport under a warm
//! personal light, turned a little off the camera; Accept keeps the look
//! (`SetCharLook`, sent with 0x002A when the character is played), Back drops
//! it. Both go back to the Options box.
//!
//! The model is made again when the look changes (the game re-dresses the
//! sprite in place: `SetFace`, `SetHair`), which starts its standing loop
//! over.

use super::select::{framing, look_of};
use super::{CharRecord, UiCx};
use crate::pointer::{Hits, key};
use crate::pregame::Scene;
use crate::pregame::figure::Figure;
use crate::pregame::stage::{Stage, VIEWPORT};
use vi::fpu::{Ee, F};
use vi::ui::{FrameParam, UI_COLORS, Window, wide};

/// The light `OnDrawSelf` sets (`SetPersonalLight(at, 5.0, light)` in mode
/// 1): warm, five units round the camera. Mode 1 ramps the radius up from
/// the scene's last update, which in the character scenes is long past (mode
/// 2 skips the update), so it is whole at once.
pub const LIGHT: [f32; 3] = [0.75, 0.69, 0.36];
pub const LIGHT_RANGE: f32 = 5.0;

/// The look's four values, each round its range.
fn wrap(v: i32, d: i32, top: i32) -> i32 {
    if d < 0 {
        if v != 0 { v - 1 } else { top }
    } else if v == top {
        0
    } else {
        v + 1
    }
}

#[derive(Default)]
pub struct Cust {
    index: usize,
    face: i32,
    style: i32,
    length: i32,
    color: i32,
    sel: i32,
    window: Window,
    figure: Option<Figure>,
    /// the look the figure was made in
    made: Option<[i32; 4]>,
    /// `m_pos`, `m_hpr`: select's spot and angles, kept before `SetChar`
    /// re-aims the camera (the model stands there, turned off them)
    pos: [f32; 3],
    hpr: [f32; 3],
    /// `SetOrbitAngle` from `Up`/`Down`, done at the next draw (the camera
    /// is the stage's)
    turn: Option<f32>,
    /// `SetChar` asked for, done at the next draw (it aims the camera)
    pending: Option<usize>,
    /// the focus arrived (295): the window made at the next draw
    focused: bool,
    shown: bool,
    hits: Hits,
}

const HIT_ACCEPT: i32 = 10;
const HIT_BACK: i32 = 11;

impl Cust {
    /// `SetChar(i, pos, hpr)` and the focus (295): done at the next draw,
    /// where the camera is.
    pub fn set_char(&mut self, i: usize) {
        self.pending = Some(i);
        self.focused = true;
    }

    /// `Release`: the model let go.
    pub fn release(&mut self) {
        self.figure = None;
        self.made = None;
    }

    /// The look as it stands: face, hair style, length, colour.
    pub fn look(&self) -> [i32; 4] {
        [self.face, self.style, self.length, self.color]
    }

    /// `OnDrawSelf`: the title bar and the two panels over the dark
    /// background, the rows and their sliders, then the model in its own
    /// viewport. None: still up.
    pub fn draw(&mut self, cx: &mut UiCx, chars: &[CharRecord], stage: Option<&mut Stage>) -> Option<Option<(usize, [i32; 4])>> {
        let mut stage = stage;
        if let Some(i) = self.pending.take()
            && let Some(c) = chars.get(i)
            && let Some(st) = stage.as_deref_mut()
        {
            // `SetChar`: the record's look, the camera at the select scene's
            // place for it with the scenes' limits, orbiting
            self.index = i;
            (self.face, self.style, self.length, self.color) = (c.face, c.hair_style, c.hair_length, c.hair_color);
            self.pos = st.cam.walk.committed.position.to_array();
            self.hpr = st.cam.walk.committed.angles.to_array();
            st.aim(self.pos, [0.0; 3]);
            let f = F::<Ee>::new;
            st.cam.min_orbit_distance = f(0.5);
            st.cam.max_orbit_distance = f(25.0);
            st.cam.min_collision_dist = f(3.5);
            st.cam.set_camera_mode(1);
        }
        let (vui, r) = (cx.vui, &mut *cx.r);
        if self.focused {
            // 295: the window, open at once, on the first row
            self.focused = false;
            self.window.init(vui, r, [224, 320], 500, 1, 1);
            self.window.open(true);
            self.sel = 0;
            self.release();
        }
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        let white = [1.0, 1.0, 1.0, 1.0];
        vui.blit_rect(r, 32, [-1, -1], [642, 450], 0, 1.0);
        let w = vui.string_width(r, &word(259)) + 64;
        vui.blit_bar(r, &word(259), white, [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
        self.hits.clear();
        self.draw_rows(cx);
        self.draw_values(cx);
        // the model (`CreateModel`), standing (`SetAction(0)`), the camera
        // (`UpdateCamera`), the scene into the viewport
        self.shown = false;
        let c = chars.get(self.index)?;
        if self.made != Some(self.look()) {
            self.figure = None;
        }
        if self.figure.is_none()
            && let (Some(g), Some(lib), Some(_)) = (cx.gpu.as_mut(), cx.lib, stage.as_deref_mut())
        {
            let mut rec = c.clone();
            (rec.face, rec.hair_style, rec.hair_length, rec.hair_color) = (self.face, self.style, self.length, self.color);
            // turned a little off the camera: the heading plus 5.934119
            let hpr = [(F::<Ee>::new(self.hpr[0]) + F::new(5.934_119)).0, self.hpr[1], self.hpr[2]];
            self.figure = Figure::new(lib, g, rec.model, look_of(&rec, false), rec.weapons, self.pos, hpr, cx.now);
            self.made = Some(self.look());
        }
        let fig = self.figure.as_mut()?;
        fig.set_action(0);
        if let Some(st) = stage {
            let rc = cx.race_cams.and_then(|t| crate::camstyle::race_camera(t, c.race, cx.expansion));
            if rc.is_some() {
                let [h, l, p, d] = framing(rc, false);
                let f = F::<Ee>::new;
                st.cam.target_height = f(h);
                st.cam.look_at_height_above = f(l);
                st.cam.set_def_orbit_pitch_angle(f(p));
                st.cam.set_orbit_distance(f(d));
            }
            if let Some(a) = self.turn.take() {
                st.cam.set_orbit_angle(F::new(a));
            }
            st.process(cx.now);
            if let Some(g) = cx.gpu.as_ref() {
                fig.pose(g, cx.now);
            }
            cx.r.mark_split();
            self.shown = true;
        }
        None
    }

    /// `DrawRows`: the gem corners of the model's box and its foot, the
    /// rows' panel (a filled frame and two studded frames), the legend bar.
    fn draw_rows(&mut self, cx: &mut UiCx) {
        let (vui, r) = (cx.vui, &mut *cx.r);
        let (c2, c3) = (vui.entry(36), vui.entry(39));
        let size = [284, 377];
        let tl = [1, 37];
        let at = [tl[0] + c2.uv1[0], tl[1] + c2.uv1[1]];
        let gem = vui.entry(59);
        let (x, mut y) = (285, 37);
        vui.blit_ex(r, 59, [x, y], 0, 1.0, 0);
        vui.blit_ex(r, 59, [x + gem.uv1[0], y], 0, 1.0, 1);
        vui.blit_ex(r, 59, [x + gem.uv1[0], y + gem.uv1[1]], 0, 1.0, 3);
        vui.blit_ex(r, 59, [x, y + gem.uv1[1]], 0, 1.0, 2);
        y = 405;
        vui.blit_horz(r, [x, y], 60, 62, 0, 1.0, 0);
        let fp = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
        let inner = [size[0] - c2.uv1[0] - c3.uv1[0], size[1] - c2.uv1[1] - c3.uv1[1]];
        vui.blit_fill_frame(r, at, inner, &fp, 35, 0, 1.0, true);
        let fp = FrameParam { tl: (36, 0), top: (37, 0), tr: (36, 1), right: (42, 1), br: (39, 1), bottom: (40, 0), bl: (39, 0), left: (42, 0) };
        let studs = FrameParam { tl: (0, 0), top: (38, 0), tr: (0, 0), right: (43, 1), br: (0, 0), bottom: (41, 0), bl: (0, 0), left: (43, 0) };
        vui.blit_studded_frame(r, tl, size, &fp, &studs, 2, 3, 0, 1.0, true);
        vui.blit_studded_frame(r, [tl[0] + 346, tl[1]], size, &fp, &studs, 2, 3, 0, 1.0, true);
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        let at = [50, 408];
        let w = vui.string_width(r, &word(217)) + 110 + vui.string_width(r, &word(218));
        vui.blit_bar(r, &[], [0.0, 0.0, 0.0, 1.0], at, 25, 26, w, 0, 1.0, 0, true);
        self.window.init(vui, r, [at[0] + (w >> 1), at[1] + 12], w, 1, 0);
        self.window.open(true);
        let mut t = wide("|x");
        t.extend(word(1779));
        t.extend(wide("   |t"));
        t.extend(word(217));
        let ww = self.window.size[0];
        self.window.text_box(vui, r, &t, 20, 0, ww - 40, 1, UI_COLORS[0], 0);
        let row = crate::pointer::rows_rect(&self.window, vui, r, 20, 0, ww - 40, 1);
        let x0 = self.window.top_left()[0] + 20;
        let w1 = self.window.text_width(vui, r, &[wide("|x"), word(1779)].concat());
        self.hits.push([x0, row[1], x0 + w1, row[3]], HIT_ACCEPT);
        self.hits.push([x0 + w1 + 8, row[1], row[2], row[3]], HIT_BACK);
    }

    /// `DrawValues`: the four rows (a bracket pair, the row's icon, its
    /// pulse when selected, its word), the selection's arrows, the four
    /// sliders.
    fn draw_values(&mut self, cx: &mut UiCx) {
        let (vui, r) = (cx.vui, &mut *cx.r);
        let black = [0.0, 0.0, 0.0, 1.0];
        let pulse = self.window.tick_pulse(vui.time);
        let mut base = [27, 92];
        // the icon and its offset, the glow and its offset, the word and its offset
        type Row = (i32, [i32; 2], i32, [i32; 2], i32, [i32; 2]);
        let rows: [Row; 4] = [
            (131, [95, -40], 206, [9, 15], 604, [25, 60]),
            (132, [95, -40], 207, [23, 25], 605, [25, 60]),
            (133, [95, -40], 208, [1, 2], 606, [25, 60]),
            (134, [95, -35], 209, [5, 6], 607, [25, 56]),
        ];
        for (k, &(icon, off, glow, goff, word, woff)) in rows.iter().enumerate() {
            let mut pos = base;
            vui.blit_ex(r, 135, pos, 0, 1.0, 0);
            pos[0] += 120;
            vui.blit_ex(r, 135, pos, 0, 1.0, 1);
            let mut pos = [base[0] + off[0], base[1] + off[1]];
            vui.blit_ex(r, icon, pos, 0, 1.0, 0);
            if self.sel == k as i32 {
                vui.blit_ex(r, glow, [pos[0] + goff[0], pos[1] + goff[1]], 3, pulse, 0);
            }
            self.hits.push([base[0], base[1] - 45, base[0] + 160, base[1] + 30], k as i32);
            pos = [pos[0] + woff[0], pos[1] + woff[1]];
            r.draw_centered_string(pos, black, &wide(vui.words.get(word, vui.language)), 0);
            base[1] += 84;
        }
        let mut pos = [73, self.sel * 83 + 75];
        vui.blit_ex(r, 85, pos, 0, 1.0, 0);
        pos[0] += 125;
        vui.blit_ex(r, 85, pos, 0, 1.0, 1);
        self.window.init(vui, r, [142, 240], 280, 16, 1);
        let w = self.window.size[0];
        for (row, v, top) in [(1, self.face, 3.0), (6, self.style, 3.0), (11, self.length, 3.0), (16, self.color, 7.0)] {
            self.window.draw_hscroll(vui, r, 30, row, w - 60, v as f32 / top, 0);
        }
    }

    /// `OnKeyDown`: back (260, nothing kept), Accept (the look kept), the
    /// rows and their values. Some when it closes.
    pub fn key(&mut self, k: i32, now: i64, cues: &mut Vec<i32>) -> Option<Option<(usize, [i32; 4])>> {
        match k {
            27 | 132 => {
                cues.push(8);
                return Some(None);
            }
            13 | 134 => {
                cues.push(13);
                return Some(Some((self.index, self.look())));
            }
            17 | 136 => self.change(-1, now, cues),
            18 | 137 => self.change(1, now, cues),
            19 | 138 => self.row(-1, now, cues),
            20 | 139 => self.row(1, now, cues),
            _ => {}
        }
        None
    }

    /// `Up` / `Down`: the hair rows turn the camera a twelfth of a turn
    /// round the model (`SetOrbitAngle`, at the ends too: done at the next
    /// draw, where the camera is).
    fn row(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        let s = self.sel + d;
        if !(0..=3).contains(&s) {
            self.sel = s.clamp(0, 3);
            cues.push(9);
        } else {
            self.sel = s;
            cues.push(11);
        }
        self.window.reset_tick_pulse(now);
        self.turn = Some(self.orbit_angle());
    }

    /// `Prev` / `Next`: the row's value, wrapping (0..3, the colour 0..7).
    fn change(&mut self, d: i32, now: i64, cues: &mut Vec<i32>) {
        cues.push(11);
        match self.sel {
            0 => self.face = wrap(self.face, d, 3),
            1 => self.style = wrap(self.style, d, 3),
            2 => self.length = wrap(self.length, d, 3),
            3 => self.color = wrap(self.color, d, 7),
            _ => {}
        }
        self.window.reset_tick_pulse(now);
    }

    /// The camera's orbit angle the selected row asks for (`Up`/`Down`'s
    /// `SetOrbitAngle`): the hair rows a twelfth of a turn.
    fn orbit_angle(&self) -> f32 {
        // the game's 0.5235988f, which is pi/6's float
        if matches!(self.sel, 2 | 3) { std::f32::consts::FRAC_PI_6 } else { 0.0 }
    }

    /// A pointer: a row selects it (the selected one's left or right half
    /// changes its value), the legend's Accept and Back.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.hits.at(p) {
            Some(HIT_ACCEPT) => vec![key::CROSS],
            Some(HIT_BACK) => vec![key::TRIANGLE],
            Some(row) if row == self.sel => vec![if p[0] < 107.0 { key::LEFT } else { key::RIGHT }],
            Some(row) => crate::pointer::steps(self.sel, row, key::UP, key::DOWN),
            None => Vec::new(),
        }
    }

    /// The 3D: the arena and the model under the warm light.
    pub fn scene<'a>(&'a self, st: &'a Stage) -> Option<Scene<'a>> {
        let f = self.figure.as_ref().filter(|_| self.shown)?;
        let mut view = st.view();
        let at = glam::Vec3::from(st.eye.position.to_array());
        view.point = Some((at, LIGHT_RANGE, glam::Vec3::from(LIGHT)));
        Some(Scene { view, viewport: VIEWPORT, models: st.models().chain(f.models()).collect() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The values wrap round their ranges both ways; the rows stop at their
    /// ends with a buzz; Accept hands the look, Back nothing.
    #[test]
    fn the_rows_and_values() {
        let mut c = Cust { face: 0, style: 3, length: 1, color: 7, ..Default::default() };
        let mut cues = Vec::new();
        c.key(136, 0, &mut cues);
        assert_eq!(c.face, 3, "face 0 back wraps to 3");
        c.key(139, 0, &mut cues);
        c.key(137, 0, &mut cues);
        assert_eq!(c.style, 0, "style 3 on wraps to 0");
        c.key(139, 0, &mut cues);
        c.key(139, 0, &mut cues);
        c.key(137, 0, &mut cues);
        assert_eq!(c.color, 0, "colour 7 on wraps to 0");
        cues.clear();
        c.key(139, 0, &mut cues);
        assert_eq!((c.sel, cues.as_slice()), (3, &[9][..]), "a buzz past the last row");
        assert_eq!(c.turn, Some(std::f32::consts::FRAC_PI_6), "the hair rows turn the camera, at the end too");
        assert_eq!(std::f32::consts::FRAC_PI_6.to_bits(), 0x3F06_0A92, "the game's 0.5235988f");
        c.key(138, 0, &mut cues);
        c.key(138, 0, &mut cues);
        assert_eq!((c.sel, c.turn), (1, Some(0.0)), "the face and style rows face it");
        c.key(139, 0, &mut cues);
        c.key(139, 0, &mut cues);
        assert_eq!(c.key(134, 0, &mut cues), Some(Some((0, [3, 0, 1, 0]))));
        assert_eq!(c.key(132, 0, &mut cues), Some(None));
        // the control: up and down do not close it
        assert_eq!(c.key(138, 0, &mut cues), None);
        assert_eq!((wrap(2, 1, 3), wrap(2, -1, 3)), (3, 1));
    }

    /// A pointer: a row selects it, the selected row's halves change its
    /// value; the control: with nothing drawn, nothing.
    #[test]
    fn a_pointer_on_the_rows() {
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        let mut c = Cust::default();
        let mut cx = UiCx::bare(&vui, &mut r);
        c.draw_rows(&mut cx);
        c.draw_values(&mut cx);
        assert_eq!(c.point([60.0, 92.0 + 84.0 * 2.0]), vec![key::DOWN, key::DOWN]);
        assert_eq!(c.point([40.0, 92.0]), vec![key::LEFT]);
        assert_eq!(c.point([150.0, 92.0]), vec![key::RIGHT]);
        c.hits.clear();
        assert!(c.point([40.0, 92.0]).is_empty());
    }
}
