//! `VIWndCharCreateMisc` (CLIENT `viwndcharcreatemisc.cpp`): the appearance
//! page. Four rows on the left (face, hair style, hair length, hair colour),
//! each a picture of what it changes over a slider, left and right stepping
//! it round (0..3, the colour 0..7) on the live model; the hair rows turn
//! the camera a twelfth of a turn. The model stands on its race's spot,
//! turned a little off the camera (+5.934119 rad), framed by the race's
//! appearance camera (the original game's human man has his own), lit by
//! the scene's personal light at the eye (radius 5, (0.75, 0.69, 0.36)).
//! Enter hands up the four (264), back is 260.

use super::race::look_for;
use super::{Env, Msg, create_model, frame_camera, legend, redress, word};
use crate::pointer::{Hits, key};
use crate::pregame::Ctx;
use crate::pregame::stage::Stage;
use net::session::NewChar;
use vi::fpu::{Ee, F};
use vi::ui::{FrameParam, Window};

/// What a hit is: a row (0..3), a row's slider halves (10 + row left, 20 +
/// row right), the legend.
const TRIANGLE: i32 = 100;
const CROSS: i32 = 101;

pub struct MiscPage {
    pub sel: i32,
    pub face: i32,
    pub hair_style: i32,
    pub hair_length: i32,
    pub hair_color: i32,
    pulse: i32,
    time: i64,
    glow: f32,
    window: Window,
    hits: Hits,
    /// the model's look needs building again (a value changed)
    dirty: bool,
    /// `Activate`'s camera, set at the next draw: the race's spot
    pending: Option<[f32; 2]>,
}

impl Default for MiscPage {
    fn default() -> Self {
        // `Init`: 3, 3, 3, 0
        MiscPage { sel: 0, face: 3, hair_style: 3, hair_length: 3, hair_color: 0, pulse: 1, time: 0, glow: 1.0, window: Window::default(), hits: Hits::default(), dirty: false, pending: None }
    }
}

impl MiscPage {
    /// `Reset`.
    pub fn reset(&mut self, env: &mut Env) {
        self.pulse = 1;
        self.glow = 1.0;
        self.sel = 0;
        self.face = 0;
        self.hair_style = 0;
        self.hair_length = 0;
        self.hair_color = 0;
        env.release();
    }

    pub fn look(&self) -> [i32; 4] {
        [self.face, self.hair_style, self.hair_length, self.hair_color]
    }

    /// `Activate` (before the page is made active, the form's 266 and the
    /// review's back): the camera on the race's spot in the scene, its orbit
    /// limits, mode 1.
    pub fn reactivate(&mut self, form: &NewChar, env: &mut Env) {
        let _ = env;
        self.pending = Some(super::tables::race(form.race).select);
    }

    /// `OnMessage` 295: the four values from the form, or where it has none
    /// (-1) the race's defaults (the eastern humans have their own).
    pub fn activate(&mut self, now: i64, form: &NewChar, env: &Env) {
        self.time = now;
        [self.face, self.hair_style, self.hair_length, self.hair_color] = look_for(form.race, form.city, form.sex, form, env);
        self.window.open(true);
    }

    /// `OnKeyDown`.
    pub fn key(&mut self, k: i32, now: i64, env: &mut Env, stage: Option<&mut Stage>, sounds: &mut Vec<i32>) -> Option<Msg> {
        match k {
            13 | key::CROSS => {
                sounds.push(13);
                let m = Msg::Look(self.look());
                env.release();
                self.sel = 0;
                self.face = 3;
                self.hair_style = 3;
                self.hair_length = 3;
                self.hair_color = 0;
                Some(m)
            }
            27 | key::TRIANGLE => {
                sounds.push(8);
                self.sel = 0;
                self.face = 3;
                self.hair_style = 3;
                self.hair_length = 3;
                self.hair_color = 0;
                env.release();
                Some(Msg::Back)
            }
            19 | key::UP => {
                self.move_sel(-1, now, stage, sounds);
                None
            }
            20 | key::DOWN => {
                self.move_sel(1, now, stage, sounds);
                None
            }
            17 | key::LEFT => {
                self.adjust(-1, now, sounds);
                None
            }
            18 | key::RIGHT => {
                self.adjust(1, now, sounds);
                None
            }
            _ => None,
        }
    }

    /// `MoveSel`: the hair's length and colour turn the camera a twelfth of
    /// a turn.
    fn move_sel(&mut self, d: i32, now: i64, stage: Option<&mut Stage>, sounds: &mut Vec<i32>) {
        self.glow = 1.0;
        self.pulse = 1;
        self.time = now;
        if self.sel + d < 0 {
            sounds.push(9);
            self.sel = 0;
        } else if self.sel + d >= 4 {
            sounds.push(9);
            self.sel = 3;
        } else {
            sounds.push(11);
            self.sel += d;
        }
        if let Some(st) = stage {
            st.cam.set_orbit_angle(F::<Ee>::new(if self.sel == 2 || self.sel == 3 { 0.523_598_8 } else { 0.0 }));
        }
    }

    /// `Adjust`: the row's value round its range (`AdjustFace` and the
    /// three hair ones), the model dressed again.
    fn adjust(&mut self, d: i32, now: i64, sounds: &mut Vec<i32>) {
        self.pulse = 1;
        self.glow = 1.0;
        self.time = now;
        let (v, n) = match self.sel {
            0 => (&mut self.face, 4),
            1 => (&mut self.hair_style, 4),
            2 => (&mut self.hair_length, 4),
            _ => (&mut self.hair_color, 8),
        };
        let x = *v + d;
        *v = if x < 0 {
            n - 1
        } else if x >= n {
            0
        } else {
            x
        };
        sounds.push(11);
        self.dirty = true;
    }

    /// `OnDrawSelf`: the frames, the title and legend, the four rows with
    /// their pictures, cursor and sliders; then the model (made if there is
    /// none), standing, the camera, the personal light, the scene.
    pub fn draw(&mut self, cx: &mut Ctx, env: &mut Env, stage: Option<&mut Stage>, form: &NewChar) {
        let vui = cx.vui;
        let now = cx.now;
        let dt = (now - self.time) as f32 * 0.001;
        self.time = now;
        let white = [1.0, 1.0, 1.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        self.hits.clear();
        {
            let r = &mut *cx.r;
            let (c2, c3) = (vui.entry(36).uv1, vui.entry(39).uv1);
            let size = [284, 377];
            let tl = [1, 37];
            let at = [tl[0] + c2[0], tl[1] + c2[1]];
            vui.blit_rect(r, 32, [-1, -1], [642, 450], 0, 1.0);
            let gem = vui.entry(59).uv1;
            let (x, y) = (285, 37);
            vui.blit_ex(r, 59, [x, y], 0, 1.0, 0);
            vui.blit_ex(r, 59, [x + gem[0], y], 0, 1.0, 1);
            vui.blit_ex(r, 59, [x + gem[0], y + gem[1]], 0, 1.0, 3);
            vui.blit_ex(r, 59, [x, y + gem[1]], 0, 1.0, 2);
            vui.blit_horz(r, [x, 405], 60, 62, 0, 1.0, 0);
            let fp = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
            let inner = [size[0] - c2[0] - c3[0], size[1] - c2[1] - c3[1]];
            vui.blit_fill_frame(r, at, inner, &fp, 35, 0, 1.0, true);
            let frame = FrameParam { tl: (36, 0), top: (37, 0), tr: (36, 1), right: (42, 1), br: (39, 1), bottom: (40, 0), bl: (39, 0), left: (42, 0) };
            let studs = FrameParam { tl: (0, 0), top: (38, 0), tr: (0, 0), right: (43, 1), br: (0, 0), bottom: (41, 0), bl: (0, 0), left: (43, 0) };
            vui.blit_studded_frame(r, tl, size, &frame, &studs, 2, 3, 0, 1.0, true);
            vui.blit_studded_frame(r, [tl[0] + 346, tl[1]], size, &frame, &studs, 2, 3, 0, 1.0, true);
            let title = word(vui, 259);
            let w = r.string_width(&title) + 64;
            vui.blit_bar(r, &title, white, [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
            let [tri, cross] = legend(vui, r, 217, 216);
            self.hits.push(tri, TRIANGLE);
            self.hits.push(cross, CROSS);
            // the four rows: two plates, the picture (lit when picked), the word
            let rows: [(i32, i32, [i32; 2], [i32; 2], i32, i32); 4] =
                [(131, 206, [95, -40], [9, 15], 604, 60), (132, 207, [95, -40], [23, 25], 605, 60), (133, 208, [95, -40], [1, 2], 606, 60), (134, 209, [95, -35], [5, 6], 607, 56)];
            let mut tl = [27, 92];
            for (i, &(pic, lit, off, lit_off, wd, wy)) in rows.iter().enumerate() {
                vui.blit_ex(r, 135, tl, 0, 1.0, 0);
                vui.blit_ex(r, 135, [tl[0] + 120, tl[1]], 0, 1.0, 1);
                let at = [tl[0] + off[0], tl[1] + off[1]];
                vui.blit_ex(r, pic, at, 0, 1.0, 0);
                if self.sel == i as i32 {
                    vui.blit_ex(r, lit, [at[0] + lit_off[0], at[1] + lit_off[1]], 3, self.glow, 0);
                }
                let at = [at[0] + 25, at[1] + wy];
                r.draw_centered_string(at, black, &word(vui, wd), 0);
                tl[1] += 84;
            }
            let at = [73, self.sel * 83 + 75];
            vui.blit_ex(r, 85, at, 0, 1.0, 0);
            vui.blit_ex(r, 85, [at[0] + 125, at[1]], 0, 1.0, 1);
            self.window.init(vui, r, [142, 240], 280, 16, 1);
            let ww = self.window.size[0];
            for (row, (v, n)) in [(self.face, 3.0), (self.hair_style, 3.0), (self.hair_length, 3.0), (self.hair_color, 7.0)].iter().enumerate() {
                let line = [1, 6, 11, 16][row];
                self.window.draw_hscroll(vui, r, 30, line, ww - 60, *v as f32 / n, 0);
                // the row: its picture and its slider; the slider's halves
                let left = self.window.top_left()[0] + 30;
                let y = self.window.row_center(vui, r, line)[1] - 4;
                let mid = left + (ww - 60) / 2;
                self.hits.push([left - 10, y - 4, mid, y + 20], 10 + row as i32);
                self.hits.push([mid, y - 4, left + ww - 60 + 10, y + 20], 20 + row as i32);
                self.hits.push([27, 52 + 84 * row as i32, 300, y - 4], row as i32);
            }
        }
        // the glow: down at twice the time, up at three times
        if self.pulse == -1 {
            self.glow = (self.glow - (dt + dt)).max(0.0);
            if self.glow == 0.0 {
                self.pulse = 1;
            }
        } else {
            self.glow = (self.glow + dt * 3.0).min(1.0);
            if self.glow == 1.0 {
                self.pulse = -1;
            }
        }
        let Some(stage) = stage else { return };
        if let Some(at) = self.pending.take() {
            // Activate: Init(pos, 0), SetSceneCameraLimits(0.5, 25, 3.5), mode 1
            stage.aim([at[0], 0.0, at[1]], [0.0; 3]);
            stage.cam.min_orbit_distance = F::new(0.5);
            stage.cam.max_orbit_distance = F::new(25.0);
            stage.cam.min_collision_dist = F::new(3.5);
        }
        let race = super::tables::race(form.race);
        if env.figure.is_none() {
            let c = &stage.cam.walk.committed;
            let (at, hpr) = (c.position.to_array(), c.angles.to_array());
            // turned a little off the camera
            let hpr = [(F::<Ee>::new(hpr[0]) + F::new(5.934_119)).0, hpr[1], hpr[2]];
            env.figure = create_model(cx, race.model_id(form.sex), self.look(), at, hpr);
            self.dirty = false;
        } else if std::mem::take(&mut self.dirty)
            && let Some(f) = env.figure.as_mut()
        {
            redress(cx, f, self.look());
        }
        if let Some(f) = env.figure.as_mut() {
            f.anims.time = now;
            f.set_action(0);
        }
        // UpdateCamera: the race's appearance framing (the original game's
        // human man his own), no turning
        let framing = match env.race_camera(form.race) {
            Some(_) if form.race == 0 && form.sex == 0 && env.expansion & 2 == 0 => [0.9, 1.48, 0.567_232, 1.25],
            Some(c) => c.cust,
            None => super::DEFAULT_FRAMING,
        };
        frame_camera(stage, framing);
        stage.process(now);
        // the personal light at the eye: mode 1, its radius ramping to 5
        if env.figure.is_some() {
            let step = if now > env.light_time { (now - env.light_time) as f32 * 0.002 } else { 0.0 };
            env.light = (env.light + step).clamp(0.0, 5.0);
            env.light_time = now;
            let eye = glam::Vec3::from(stage.cam.last.camera_pos.to_array());
            env.point = (env.light > 0.0).then_some((eye, env.light, glam::Vec3::new(0.75, 0.69, 0.36)));
        }
        if let (Some(f), Some(g)) = (env.figure.as_mut(), cx.gpu.as_ref()) {
            f.pose(g, now);
            cx.r.mark_split();
            env.show3d = true;
        }
    }

    /// A pointer: a row's picture picks the row, its slider's halves step
    /// it (picking it first), the legend's glyphs.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.hits.at(p) {
            Some(TRIANGLE) => vec![key::TRIANGLE],
            Some(CROSS) => vec![key::CROSS],
            Some(h) if h >= 10 => {
                let row = h % 10;
                let mut keys = crate::pointer::steps(self.sel, row, key::UP, key::DOWN);
                keys.push(if h >= 20 { key::RIGHT } else { key::LEFT });
                keys
            }
            Some(row) => crate::pointer::steps(self.sel, row, key::UP, key::DOWN),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_values_go_round_their_ranges() {
        let mut p = MiscPage::default();
        let mut env = Env::default();
        let mut s = Vec::new();
        p.face = 0;
        p.key(key::LEFT, 0, &mut env, None, &mut s);
        assert_eq!(p.face, 3, "face 0 back to 3");
        p.key(key::RIGHT, 0, &mut env, None, &mut s);
        assert_eq!(p.face, 0);
        for _ in 0..3 {
            p.key(key::DOWN, 0, &mut env, None, &mut s);
        }
        p.hair_color = 7;
        p.key(key::RIGHT, 0, &mut env, None, &mut s);
        assert_eq!(p.hair_color, 0, "colour 7 on to 0");
        p.key(key::LEFT, 0, &mut env, None, &mut s);
        assert_eq!(p.hair_color, 7);
        // the rows stop at their ends
        s.clear();
        p.key(key::DOWN, 0, &mut env, None, &mut s);
        assert_eq!((p.sel, s.as_slice()), (3, &[9][..]));
        assert!(p.dirty, "the model is dressed again");
    }

    #[test]
    fn enter_hands_up_the_four_and_resets() {
        let mut p = MiscPage { face: 1, hair_style: 2, hair_length: 0, hair_color: 6, ..Default::default() };
        let mut env = Env::default();
        let mut s = Vec::new();
        assert_eq!(p.key(key::CROSS, 0, &mut env, None, &mut s), Some(Msg::Look([1, 2, 0, 6])));
        assert_eq!(p.look(), [3, 3, 3, 0], "Init's values again");
        assert_eq!(p.key(key::TRIANGLE, 0, &mut env, None, &mut s), Some(Msg::Back));
    }

    #[test]
    fn the_form_seeds_the_values_else_the_defaults() {
        let mut p = MiscPage::default();
        let env = Env { expansion: 3, ..Default::default() };
        let form = NewChar { race: 0, city: 1, sex: 1, hair_color: 4, ..super::super::blank_form() };
        p.activate(0, &form, &env);
        assert_eq!(p.look(), [0, 0, 3, 4], "the eastern woman's defaults, the form's colour");
    }

    /// A pointer: a slider's right half picks its row and steps it right.
    #[test]
    fn a_pointer_steps_the_sliders() {
        let mut p = MiscPage::default();
        let v = super::super::tests_vui();
        let mut r = crate::pointer::testing::NullRaster;
        let res = crate::pregame::res::Res::new();
        let mut shared = crate::pregame::Shared::default();
        let (mut cues, mut music) = (Vec::new(), Vec::new());
        let mut cx = Ctx { now: 0, game_frame: true, vui: &v, r: &mut r, res: &res, shared: &mut shared, cues: &mut cues, music: &mut music, gpu: None };
        let mut env = Env::default();
        p.draw(&mut cx, &mut env, None, &super::super::blank_form());
        // the colour's slider (row 3), its right end
        let w = p.window;
        let line_y = w.row_center(&v, &r, 16)[1] - 4 + 6;
        let right = w.top_left()[0] + 30 + (w.size[0] - 60) - 4;
        assert_eq!(p.point([right as f32, line_y as f32]), vec![key::DOWN, key::DOWN, key::DOWN, key::RIGHT]);
        // the control
        p.hits.clear();
        assert!(p.point([right as f32, line_y as f32]).is_empty());
    }
}
