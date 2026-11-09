//! `VIWndCharCreateMisc` (CLIENT `viwndcharcreatemisc.cpp`): the appearance
//! page. Four rows on the left (face, hair style, hair length, hair colour),
//! each a picture of what it changes over a slider, left and right stepping
//! it round (0..3, the colour 0..7) on the live model; the hair rows turn
//! the camera a twelfth of a turn. The model stands on its race's spot,
//! turned a little off the camera (+5.934119 rad), framed by the race's
//! appearance camera (the original game's human man has his own), lit by
//! the scene's personal light at the eye (radius 5, (0.75, 0.69, 0.36)).
//! Enter hands up the four (264), back is 260.
//!
//! A race of the server's own whose options it sent (`raceopts`: a creature's
//! body, on which the four change nothing) shows its rows here instead: each its
//! word and the step's name, over a picture by what it changes (a body or a face
//! the face's, hair the hair style's, a colour the palette), left and right
//! stepping it round on the live model, as the four do. A body row makes the model
//! anew in the body chosen; a row with one step for the sex, or a part the body
//! chosen has not, is not shown (its first step goes). More than four scroll with
//! the cursor. The race's whole body is framed (the review's framing), as a colour
//! is laid on any of it. Enter hands up the race's defaults for the four, and the
//! steps chosen with them (`take_options`).

use super::race::look_for;
use super::{Env, Msg, create_model_dressed, frame_camera, legend, redress_dressed, word};
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

/// A server's race's options on the page: its rows (all of them, each with the
/// step on it) and which are shown, from `top` down.
#[derive(Clone, Debug, Default)]
pub struct Opts {
    pub sex: i32,
    pub rows: Vec<net::raceopts::Row>,
    pub steps: Vec<usize>,
    /// the rows shown, as indices into `rows`
    pub shown: Vec<usize>,
    /// the first row shown on the page (four fit)
    pub top: usize,
}

impl Opts {
    /// The body the steps choose, else the race's own (`fallback`).
    pub fn body(&self, fallback: u32) -> u32 {
        crate::raceopts::body_of_steps(&self.rows, self.sex, &self.steps).unwrap_or(fallback)
    }

    /// The rows shown on body `body`: more than one step for the sex, and a
    /// colour or texture row only with a part of the body.
    fn showing(&self, body: u32) -> Vec<usize> {
        use net::raceopts::Kind;
        (0..self.rows.len()).filter(|&i| self.rows[i].steps(self.sex).len() > 1 && (self.rows[i].kind == Kind::Body || self.rows[i].parts_of(body).next().is_some())).collect()
    }

    /// The steps as a creation sends them: every row's, a row not shown its
    /// first (a body row's single step goes as it is: the sex's own body).
    pub fn chosen(&self, body: u32) -> Vec<(String, u8)> {
        use net::raceopts::Kind;
        let shown = self.showing(body);
        self.rows.iter().enumerate().map(|(i, r)| (r.key.clone(), if r.kind == Kind::Body || shown.contains(&i) { self.steps[i].min(255) as u8 } else { 0 })).collect()
    }
}

/// The rows to show of `rows` for `sex`, the steps out of `form` (a form's
/// options as last handed up, else every row's first).
pub fn opts_for(rows: &[net::raceopts::Row], sex: i32, form: &NewChar, fallback: u32) -> Opts {
    let steps = rows
        .iter()
        .map(|r| form.options.iter().find(|(k, _)| *k == r.key).map_or(0, |(_, s)| *s as usize).min(r.steps(sex).len().saturating_sub(1)))
        .collect();
    let mut o = Opts { sex, rows: rows.to_vec(), steps, shown: Vec::new(), top: 0 };
    o.shown = o.showing(o.body(fallback));
    o
}

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
    /// the model needs making again: a body row chose another body
    rebody: bool,
    /// `Activate`'s camera, set at the next draw: the race's spot
    pending: Option<[f32; 2]>,
    /// a server's race's options on the page instead of the four (`raceopts`)
    pub opts: Option<Opts>,
    /// the steps Enter handed up, for the form (`take_options`)
    handed: Vec<(String, u8)>,
}

impl Default for MiscPage {
    fn default() -> Self {
        // `Init`: 3, 3, 3, 0
        MiscPage { sel: 0, face: 3, hair_style: 3, hair_length: 3, hair_color: 0, pulse: 1, time: 0, glow: 1.0, window: Window::default(), hits: Hits::default(), dirty: false, rebody: false, pending: None, opts: None, handed: Vec::new() }
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
        self.opts = None;
        self.handed.clear();
        env.release();
    }

    /// The steps the last Enter handed up (a server's race's options; none for
    /// any other race), taken.
    pub fn take_options(&mut self) -> Vec<(String, u8)> {
        std::mem::take(&mut self.handed)
    }

    /// How many rows the page has: a server's race's shown, else the four.
    fn rows(&self) -> i32 {
        self.opts.as_ref().map_or(4, |o| o.shown.len() as i32)
    }

    pub fn look(&self) -> [i32; 4] {
        [self.face, self.hair_style, self.hair_length, self.hair_color]
    }

    /// `Activate` (before the page is made active, the form's 266 and the
    /// review's back): the camera on the race's spot in the scene, its orbit
    /// limits, mode 1.
    pub fn reactivate(&mut self, form: &NewChar, env: &mut Env) {
        let _ = env;
        self.pending = Some(super::tables::get_race(super::tables::like(form.race)).select);
    }

    /// `OnMessage` 295: the four values from the form, or where it has none
    /// (-1) the race's defaults (the eastern humans have their own).
    pub fn activate(&mut self, now: i64, form: &NewChar, env: &Env) {
        self.time = now;
        [self.face, self.hair_style, self.hair_length, self.hair_color] = look_for(form.race, form.city, form.sex, form, env);
        // a server's race with options: its rows (the race's defaults for the four)
        self.opts = crate::raceopts::options().map(|o| o.rows(form.race).to_vec()).filter(|r| !r.is_empty()).map(|rows| opts_for(&rows, form.sex, form, super::tables::model_of(form.race, form.sex)));
        if self.opts.is_some() {
            let blank = NewChar { face: -1, hair_style: -1, hair_length: -1, hair_color: -1, ..form.clone() };
            [self.face, self.hair_style, self.hair_length, self.hair_color] = look_for(form.race, form.city, form.sex, &blank, env);
        }
        self.sel = self.sel.clamp(0, (self.rows() - 1).max(0));
        self.window.open(true);
    }

    /// `OnKeyDown`.
    pub fn key(&mut self, k: i32, now: i64, env: &mut Env, stage: Option<&mut Stage>, sounds: &mut Vec<i32>) -> Option<Msg> {
        match k {
            13 | key::CROSS => {
                sounds.push(13);
                let m = Msg::Look(self.look());
                self.handed = self.opts.take().map(|o| o.chosen(o.body(0))).unwrap_or_default();
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
                self.opts = None;
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
        let n = self.rows();
        if self.sel + d < 0 {
            sounds.push(9);
            self.sel = 0;
        } else if self.sel + d >= n {
            sounds.push(9);
            self.sel = (n - 1).max(0);
        } else {
            sounds.push(11);
            self.sel += d;
        }
        if let Some(o) = self.opts.as_mut() {
            // the cursor's row in view, four at a time
            let sel = self.sel.max(0) as usize;
            o.top = o.top.min(sel).max(sel.saturating_sub(3));
            return;
        }
        if let Some(st) = stage {
            // the game's 0.5235988f, which is pi/6's float
            st.cam.set_orbit_angle(F::<Ee>::new(if self.sel == 2 || self.sel == 3 { std::f32::consts::FRAC_PI_6 } else { 0.0 }));
        }
    }

    /// `Adjust`: the row's value round its range (`AdjustFace` and the
    /// three hair ones), the model dressed again.
    fn adjust(&mut self, d: i32, now: i64, sounds: &mut Vec<i32>) {
        self.pulse = 1;
        self.glow = 1.0;
        self.time = now;
        if let Some(o) = self.opts.as_mut() {
            let Some(&ri) = o.shown.get(self.sel.max(0) as usize) else { return };
            let n = o.rows[ri].steps(o.sex).len() as i32;
            let x = o.steps[ri] as i32 + d;
            o.steps[ri] = if x < 0 { n - 1 } else if x >= n { 0 } else { x } as usize;
            if o.rows[ri].kind == net::raceopts::Kind::Body {
                // another body: the rows its parts show, and the model made anew
                let body = o.body(0);
                o.shown = o.showing(body);
                self.sel = self.sel.min((o.shown.len() as i32 - 1).max(0));
                o.top = o.top.min(self.sel as usize);
                self.rebody = true;
            }
            sounds.push(11);
            self.dirty = true;
            return;
        }
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
            // the four rows: two plates, the picture (lit when picked), the
            // word; a server's race's rows instead, four from `top` (its word
            // and the step's name, the picture by what it changes)
            const RETAIL: [(i32, i32, [i32; 2], [i32; 2], i32); 4] = [(131, 206, [95, -40], [9, 15], 60), (132, 207, [95, -40], [23, 25], 60), (133, 208, [95, -40], [1, 2], 60), (134, 209, [95, -35], [5, 6], 56)];
            // (row index, art, the word, the slider's place)
            let shown: Vec<(i32, (i32, i32, [i32; 2], [i32; 2], i32), Vec<u16>, f32)> = match &self.opts {
                None => [(self.face, 3.0), (self.hair_style, 3.0), (self.hair_length, 3.0), (self.hair_color, 7.0)]
                    .iter()
                    .enumerate()
                    .map(|(i, &(v, n))| (i as i32, RETAIL[i], word(vui, 604 + i as i32), v as f32 / n))
                    .collect(),
                Some(o) => o
                    .shown
                    .iter()
                    .enumerate()
                    .skip(o.top)
                    .take(4)
                    .map(|(i, &ri)| {
                        let row = &o.rows[ri];
                        let steps = row.steps(o.sex);
                        let k = o.steps[ri].min(steps.len().saturating_sub(1));
                        let art = match row.kind {
                            net::raceopts::Kind::Body => RETAIL[0],
                            net::raceopts::Kind::Texture if row.key == "face" => RETAIL[0],
                            net::raceopts::Kind::Texture => RETAIL[1],
                            net::raceopts::Kind::Colour => RETAIL[3],
                        };
                        let name = steps.get(k).map_or("", |s| s.name.as_str());
                        (i as i32, art, vi::ui::wide(&format!("{}: {}", row.label, name)), k as f32 / (steps.len().max(2) - 1) as f32)
                    })
                    .collect(),
            };
            let top = self.opts.as_ref().map_or(0, |o| o.top as i32);
            let mut tl = [27, 92];
            for (i, (pic, lit, off, lit_off, wy), label, _) in &shown {
                vui.blit_ex(r, 135, tl, 0, 1.0, 0);
                vui.blit_ex(r, 135, [tl[0] + 120, tl[1]], 0, 1.0, 1);
                let at = [tl[0] + off[0], tl[1] + off[1]];
                vui.blit_ex(r, *pic, at, 0, 1.0, 0);
                if self.sel == *i {
                    vui.blit_ex(r, *lit, [at[0] + lit_off[0], at[1] + lit_off[1]], 3, self.glow, 0);
                }
                let at = [at[0] + 25, at[1] + wy];
                r.draw_centered_string(at, black, label, 0);
                tl[1] += 84;
            }
            if !shown.is_empty() {
                let at = [73, (self.sel - top) * 83 + 75];
                vui.blit_ex(r, 85, at, 0, 1.0, 0);
                vui.blit_ex(r, 85, [at[0] + 125, at[1]], 0, 1.0, 1);
            }
            self.window.init(vui, r, [142, 240], 280, 16, 1);
            let ww = self.window.size[0];
            for (row, (i, _, _, v)) in shown.iter().enumerate() {
                let line = [1, 6, 11, 16][row];
                self.window.draw_hscroll(vui, r, 30, line, ww - 60, *v, 0);
                // the row: its picture and its slider; the slider's halves
                let left = self.window.top_left()[0] + 30;
                let y = self.window.row_center(vui, r, line)[1] - 4;
                let mid = left + (ww - 60) / 2;
                self.hits.push([left - 10, y - 4, mid, y + 20], 10 + *i);
                self.hits.push([mid, y - 4, left + ww - 60 + 10, y + 20], 20 + *i);
                self.hits.push([27, 52 + 84 * row as i32, 300, y - 4], *i);
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
        let own = super::tables::model_of(form.race, form.sex);
        let model = self.opts.as_ref().map_or(own, |o| o.body(own));
        let race_look = self.opts.as_ref().map(|o| crate::raceopts::look_of_steps(&o.rows, o.sex, &o.steps, model));
        if std::mem::take(&mut self.rebody) {
            env.release();
        }
        if env.figure.is_none() {
            let c = &stage.cam.walk.committed;
            let (at, hpr) = (c.position.to_array(), c.angles.to_array());
            // turned a little off the camera
            let hpr = [(F::<Ee>::new(hpr[0]) + F::new(5.934_119)).0, hpr[1], hpr[2]];
            env.figure = create_model_dressed(cx, model, self.look(), race_look.as_ref(), at, hpr);
            self.dirty = false;
        } else if std::mem::take(&mut self.dirty)
            && let Some(f) = env.figure.as_mut()
        {
            redress_dressed(cx, f, self.look(), race_look.as_ref());
        }
        if let Some(f) = env.figure.as_mut() {
            f.anims.time = now;
            f.set_action(0);
        }
        // UpdateCamera: the race's appearance framing (the original game's
        // human man his own), no turning
        let stage_race = super::tables::like(form.race);
        let framing = match env.race_camera(stage_race) {
            // a server's race's options: its whole body, the review's framing
            Some(c) if self.opts.is_some() => c.review,
            Some(_) if stage_race == 0 && form.sex == 0 && env.expansion & 2 == 0 => [0.9, 1.48, 0.567_232, 1.25],
            Some(c) => c.cust,
            None => super::DEFAULT_FRAMING,
        };
        let framing = super::body_framing(&cx.res.lib, framing, model, super::tables::model_of(stage_race, form.sex));
        // the options' whole body: a little farther, for a cap, wings or horns
        let framing = if self.opts.is_some() { [framing[0], framing[1], framing[2], framing[3] * 1.15] } else { framing };
        frame_camera(stage, framing);
        stage.process(now);
        // the arena's particles: its braziers' flames
        stage.particles_frame(&cx.res.lib, now);
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
    use crate::raceopts::tests::fungus_options;
    use net::raceopts::Row;

    /// A server's race with options: its rows instead of the four (the row of
    /// one step not shown), each stepping round its own steps; the body row
    /// chooses another body, whose parts decide the rows shown (the red one has
    /// no bag), the cursor kept on a row; four rows in view, scrolling with the
    /// cursor; Enter hands up every row's step, a row not shown its first, and
    /// the race's defaults for the four. The control: a race with no options (a
    /// human) has the four, and hands up no options.
    #[test]
    fn a_servers_race_has_its_own_rows() {
        crate::raceopts::set_options(Some(&fungus_options()));
        let (mut env, mut s) = (Env::default(), Vec::new());
        let form = NewChar { race: 28, sex: 0, ..super::super::blank_form() };
        let mut p = MiscPage::default();
        p.activate(0, &form, &env);
        let o = p.opts.clone().expect("the fungusman's rows");
        assert_eq!(o.shown, vec![0, 1, 2, 3], "every row but the one of one step");
        assert_eq!(p.rows(), 4);
        // the cap: round its three
        p.key(key::DOWN, 0, &mut env, None, &mut s);
        for (k, want) in [(key::RIGHT, 1), (key::RIGHT, 2), (key::RIGHT, 0), (key::LEFT, 2)] {
            p.key(k, 0, &mut env, None, &mut s);
            assert_eq!(p.opts.as_ref().unwrap().steps[1], want);
        }
        // the bag, then the red body: no bag on it
        p.key(key::DOWN, 0, &mut env, None, &mut s);
        p.key(key::RIGHT, 0, &mut env, None, &mut s);
        assert_eq!(p.opts.as_ref().unwrap().steps[2], 1);
        for _ in 0..2 {
            p.key(key::UP, 0, &mut env, None, &mut s);
        }
        p.key(key::RIGHT, 0, &mut env, None, &mut s);
        let o = p.opts.clone().unwrap();
        assert_eq!((o.body(0), o.shown.clone()), (0x5874_ACFC, vec![0, 1, 3]), "the red body: no bag row");
        assert!(p.rebody, "the model made again in the body chosen");
        // Enter: every row, the hidden bag at its first, the four the defaults
        let m = p.key(key::CROSS, 0, &mut env, None, &mut s);
        assert!(matches!(m, Some(Msg::Look(_))));
        let opts: Vec<(String, u8)> = p.take_options();
        assert_eq!(opts, vec![("body".into(), 1), ("cap".into(), 2), ("bag".into(), 0), ("skin".into(), 0), ("one".into(), 0)]);
        assert!(p.take_options().is_empty(), "taken");
        // the steps come back from the form
        let back = NewChar { options: opts.clone(), ..form.clone() };
        p.activate(0, &back, &env);
        assert_eq!(p.opts.as_ref().unwrap().steps, vec![1, 2, 0, 0, 0]);

        // the control: a human, the four
        let mut human = MiscPage::default();
        human.activate(0, &NewChar { race: 0, city: 1, sex: 0, ..super::super::blank_form() }, &env);
        assert!(human.opts.is_none());
        assert_eq!(human.rows(), 4);
        human.key(key::CROSS, 0, &mut env, None, &mut s);
        assert!(human.take_options().is_empty());
        crate::raceopts::set_options(None);
        let mut gone = MiscPage::default();
        gone.activate(0, &form, &env);
        assert!(gone.opts.is_none(), "no options from the server: the four");
    }

    /// More rows than four scroll with the cursor, the four in view about it.
    #[test]
    fn more_than_four_rows_scroll() {
        let mut o = fungus_options();
        let extra = o.races[0].1[1].clone();
        for k in ["w", "x"] {
            o.races[0].1.push(Row { key: k.into(), ..extra.clone() });
        }
        crate::raceopts::set_options(Some(&o));
        let (mut env, mut s) = (Env::default(), Vec::new());
        let mut p = MiscPage::default();
        p.activate(0, &NewChar { race: 28, sex: 0, ..super::super::blank_form() }, &env);
        assert_eq!(p.rows(), 6);
        for _ in 0..5 {
            p.key(key::DOWN, 0, &mut env, None, &mut s);
        }
        assert_eq!((p.sel, p.opts.as_ref().unwrap().top), (5, 2), "the last row: the four above it");
        p.key(key::DOWN, 0, &mut env, None, &mut s);
        assert_eq!(s.last(), Some(&9), "the end");
        for _ in 0..5 {
            p.key(key::UP, 0, &mut env, None, &mut s);
        }
        assert_eq!((p.sel, p.opts.as_ref().unwrap().top), (0, 0));
        crate::raceopts::set_options(None);
    }

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
