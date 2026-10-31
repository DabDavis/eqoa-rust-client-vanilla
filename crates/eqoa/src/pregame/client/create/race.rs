//! `VIWndCharCreateRace` (CLIENT `viwndcharcreaterace.cpp`): the race page.
//! The races the owned expansions allow (`BuildList`, the humans split by
//! city), ten rows at a time with a scroll bar past ten, the sex beside the
//! one picked, the race's description, and its model in its set: the
//! form's look where it has one, else the race's default (`GetRaceInfo`;
//! the eastern humans have their own), the race's music (the humans' by
//! city), the review framing turning round it (`UpdateCamera`).

use super::tables::{self, RaceRow};
use super::{Env, Msg, create_model, frame_camera, legend, play_bgm, word};
use crate::pointer::{Hits, key};
use crate::pregame::Ctx;
use crate::pregame::stage::Stage;
use net::session::NewChar;
use vi::fpu::{Ee, F};
use vi::ui::{FrameParam, Window};

/// What a hit is: a row of the list (its index), the sex's arrows, the legend.
const LEFT: i32 = 1000;
const RIGHT: i32 = 1001;
const TRIANGLE: i32 = 1002;
const CROSS: i32 = 1003;

pub struct RacePage {
    pub sel: usize,
    pub sex: i32,
    glow: f32,
    pulse: i32,
    time: i64,
    window: Window,
    /// 0: the next model keeps the music (the sex changed); 1 plays the race's
    music: i32,
    pub list: Vec<RaceRow>,
    top: usize,
    hits: Hits,
    /// where each row of the list was drawn this frame
    rows: Vec<[i32; 4]>,
}

impl Default for RacePage {
    fn default() -> Self {
        RacePage { sel: 0, sex: 0, glow: 1.0, pulse: 1, time: 0, window: Window::default(), music: 1, list: Vec::new(), top: 0, hits: Hits::default(), rows: Vec::new() }
    }
}

impl RacePage {
    /// `Reset`.
    pub fn reset(&mut self, env: &mut Env) {
        self.glow = 1.0;
        self.pulse = 1;
        self.sel = 0;
        self.sex = 0;
        self.top = 0;
        env.release();
    }

    /// `OnMessage` 295: the list built, the cursor on the form's race and
    /// city (if it has one), the form's sex.
    pub fn activate(&mut self, now: i64, form: &NewChar, expansion: u32) {
        self.pulse = 1;
        self.glow = 1.0;
        self.time = now;
        self.window.open(true);
        self.list = tables::race_list(expansion);
        self.sel = 0;
        self.top = 0;
        if let Some(i) = self.list.iter().position(|r| r.race == form.race && r.city == form.city) {
            self.sel = i;
            let count = self.list.len();
            if count > 10 && i > 9 {
                self.top = (i as i64 - 9).max(0).min(count as i64 - 10) as usize;
            }
        }
        self.sex = if form.race != -1 { form.sex } else { 0 };
    }

    fn row(&self) -> RaceRow {
        self.list.get(self.sel).copied().unwrap_or(tables::RACE_ROWS[0])
    }

    /// `OnKeyDown`: Enter hands up the race, sex and city (262), back is
    /// 260; up and down pick the race, left and right the sex.
    pub fn key(&mut self, key: i32, now: i64, env: &mut Env, sounds: &mut Vec<i32>) -> Option<Msg> {
        match key {
            13 | key::CROSS => {
                sounds.push(13);
                let r = self.row();
                let m = Msg::Race { race: r.race, sex: self.sex, city: r.city };
                env.release();
                self.sel = 0;
                self.top = 0;
                self.sex = 0;
                Some(m)
            }
            27 | key::TRIANGLE => {
                sounds.push(8);
                self.sel = 0;
                self.top = 0;
                self.sex = 0;
                env.release();
                Some(Msg::Back)
            }
            19 | key::UP => {
                self.move_sel(-1, now, env, sounds);
                None
            }
            20 | key::DOWN => {
                self.move_sel(1, now, env, sounds);
                None
            }
            17 | key::LEFT => {
                self.set_sex(-1, now, env, sounds);
                None
            }
            18 | key::RIGHT => {
                self.set_sex(1, now, env, sounds);
                None
            }
            _ => None,
        }
    }

    /// `MoveSel`: ten rows show at a time.
    fn move_sel(&mut self, d: i32, now: i64, env: &mut Env, sounds: &mut Vec<i32>) {
        self.glow = 1.0;
        self.pulse = 1;
        self.time = now;
        let count = self.list.len() as i32;
        let s = self.sel as i32;
        if s + d < 0 {
            sounds.push(9);
            self.sel = 0;
        } else if s + d >= count {
            sounds.push(9);
            self.sel = (count - 1).max(0) as usize;
        } else {
            sounds.push(11);
            self.sel = (s + d) as usize;
            env.release();
        }
        if self.top + 9 < self.sel {
            self.top = self.sel - 9;
        }
        if self.sel < self.top {
            self.top = self.sel;
        }
    }

    /// `SetSex`.
    fn set_sex(&mut self, d: i32, now: i64, env: &mut Env, sounds: &mut Vec<i32>) {
        self.pulse = 1;
        self.glow = 1.0;
        self.time = now;
        if self.sex + d < 0 {
            sounds.push(9);
            self.sex = 0;
        } else if self.sex + d >= 2 {
            sounds.push(9);
            self.sex = 1;
        } else {
            sounds.push(11);
            self.sex += d;
            if env.figure.is_some() {
                self.music = 0;
                env.release();
            }
        }
    }

    /// The look `CreateModel` dresses the model in: the form's where it has
    /// one (not -1), else the eastern humans' own or the race's default.
    pub fn look(&self, form: &NewChar, env: &Env) -> [i32; 4] {
        let r = self.row();
        look_for(r.race, r.city, self.sex, form, env)
    }

    /// `OnDrawSelf`: the frame, the list, the description, the legend; then
    /// the model (made if there is none), standing, the camera framing it,
    /// and the scene into the viewport.
    pub fn draw(&mut self, cx: &mut Ctx, env: &mut Env, stage: Option<&mut Stage>, form: &NewChar, sounds: &mut Vec<i32>) {
        let _ = sounds;
        let vui = cx.vui;
        let now = cx.now;
        let dt = (now - self.time) as f32 * 0.001;
        self.time = now;
        let white = [1.0, 1.0, 1.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        let yellow = [1.0, 1.0, 0.0, 1.0];
        self.hits.clear();
        self.rows.clear();
        {
            let r = &mut *cx.r;
            vui.blit_rect(r, 32, [-1, -1], [642, 450], 0, 1.0);
            let title = word(vui, 257);
            let w = r.string_width(&title) + 64;
            vui.blit_bar(r, &title, white, [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
            vui.blit_rect(r, 51, [-1, 48], [348, 355], 0, 1.0);
            vui.blit_studded_horz(r, [-1, 37], 37, 38, 348, 3, 0, 1.0, 0);
            vui.blit_studded_horz(r, [-1, 396], 40, 41, 348, 3, 0, 1.0, 0);
            let fp = FrameParam { tl: (36, 0), top: (37, 0), tr: (36, 1), right: (42, 1), br: (39, 1), bottom: (40, 0), bl: (39, 0), left: (42, 0) };
            let studs = FrameParam { tl: (0, 0), top: (38, 0), tr: (0, 0), right: (43, 1), br: (0, 0), bottom: (41, 0), bl: (0, 0), left: (43, 0) };
            vui.blit_studded_frame(r, [347, 37], [284, 377], &fp, &studs, 2, 3, 0, 1.0, true);
            // the client's own, in a window wider than 4:3: the list's left
            // edge closed, as character select's (`select::PANEL`)
            crate::startmenu::close_edges(vui, r, crate::pregame::client::select::PANEL, false, [true, false]);
            let arrow = vui.entry(54);
            let step = arrow.uv1[1] - 3;
            let mut pos = [15, 48];
            let count = self.list.len();
            let bw = if count < 11 {
                210
            } else {
                let t = self.top as f32 / (count - 10) as f32;
                self.window.init(vui, r, [0, 0], 345, 5, 1);
                self.window.draw_vscroll(vui, r, [322, pos[1]], step * 10, t);
                190
            };
            for i in 0..count.min(10) {
                let row = i + self.top;
                let Some(&rr) = self.list.get(row) else { break };
                let (left, mid, right, c) = if row == self.sel { (56, 57, 58, yellow) } else { (53, 54, 55, white) };
                let name = if rr.race == 0 {
                    let mut n = word(vui, tables::get_race(0).name);
                    n.push(b'-' as u16);
                    n.extend(word(vui, rr.city_word));
                    n
                } else {
                    word(vui, tables::get_race(rr.race).name)
                };
                vui.blit_bar3(r, &name, c, pos, left, right, mid, bw, 0, 1.0, 1, true);
                let at = [pos[0] + bw, pos[1]];
                if row == self.sel {
                    let (male, female) = match self.sex {
                        0 => (128, 127),
                        1 => (126, 128),
                        _ => (128, 128),
                    };
                    vui.blit_bar3(r, &word(vui, tables::sex_word(self.sex)), c, at, left, right, mid, 117, 0, 1.0, 0, true);
                    vui.blit_ex(r, male, [at[0] + 1, at[1] + 5], 3, if male == 128 { 1.0 } else { self.glow }, (male == 128) as u32);
                    vui.blit_ex(r, female, [at[0] + 104, at[1] + 5], 3, if female == 128 { 1.0 } else { self.glow }, 0);
                    // the sex's two halves: left and right
                    self.hits.push([at[0], at[1], at[0] + 58, at[1] + step], LEFT);
                    self.hits.push([at[0] + 58, at[1], at[0] + 117, at[1] + step], RIGHT);
                } else {
                    vui.blit_bar3(r, &[], c, at, left, right, mid, 117, 0, 1.0, 0, true);
                }
                let rect = [pos[0], pos[1], pos[0] + bw + if row == self.sel { 0 } else { 117 }, pos[1] + step];
                self.hits.push(rect, row as i32);
                self.rows.push(rect);
                pos[1] += step;
            }
            let fp = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
            pos[1] += 5;
            vui.blit_fill_frame(r, pos, [325, 100], &fp, 35, 0, 1.0, true);
            let pos = [pos[0] + 163, pos[1] + 50];
            self.window.init(vui, r, pos, 345, 5, 1);
            let desc = word(vui, self.row().desc_word);
            let ww = self.window.size[0];
            self.window.text_box(vui, r, &desc, 20, 0, ww - 40, 5, black, 2);
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
        let [tri, cross] = legend(vui, cx.r, 217, 212);
        self.hits.push(tri, TRIANGLE);
        self.hits.push(cross, CROSS);
        // the model, the camera, the scene
        let Some(stage) = stage else { return };
        if env.figure.is_none() {
            let r = self.row();
            let race = tables::get_race(r.race);
            let at = [race.select[0], 0.0, race.select[1]];
            let hpr = stage.cam.walk.committed.angles.to_array();
            let look = self.look(form, env);
            env.figure = create_model(cx, race.model_id(self.sex), look, at, hpr);
            if env.figure.is_some() {
                stage.aim(at, hpr);
                if self.music > 0 {
                    let bgm = match r.city {
                        2 => "bgm\\qeynos.bgm",
                        1 => "bgm\\freeport.bgm",
                        _ => race.bgm,
                    };
                    play_bgm(cx, bgm);
                } else {
                    self.music = 1;
                }
            }
        }
        // SetAction(0) on the scene's clock this frame
        if let Some(f) = env.figure.as_mut() {
            f.anims.time = now;
            f.set_action(0);
        }
        // UpdateCamera: the race's review framing, turning
        let framing = env.race_camera(self.row().race).map_or(super::DEFAULT_FRAMING, |c| c.review);
        frame_camera(stage, framing);
        stage.cam.orbit_right(F::<Ee>::new(0.3));
        stage.process(now);
        // the arena's particles: its braziers' flames
        stage.particles_frame(&cx.res.lib, now);
        // SetPersonalLightMode(2): none
        if let (Some(f), Some(g)) = (env.figure.as_mut(), cx.gpu.as_ref()) {
            f.pose(g, now);
            cx.r.mark_split();
            env.show3d = true;
        }
    }

    /// A pointer at frame point `p`: a row moves the cursor there (a press on
    /// the row already picked picks it), the sex's halves are left and
    /// right, the legend's glyphs their buttons.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.hits.at(p) {
            Some(LEFT) => vec![key::LEFT],
            Some(RIGHT) => vec![key::RIGHT],
            Some(TRIANGLE) => vec![key::TRIANGLE],
            Some(CROSS) => vec![key::CROSS],
            Some(row) if row as usize == self.sel => vec![key::CROSS],
            Some(row) => crate::pointer::steps(self.sel as i32, row, key::UP, key::DOWN),
            None => Vec::new(),
        }
    }
}

/// `CreateModel`'s look (the race and appearance pages' alike): the form's
/// values where set, else the eastern humans' own (Frontiers' or the
/// original's) or the race's default (`GetRaceInfo`), by sex. Face, hair
/// style, hair length, hair colour.
pub fn look_for(race: i32, city: i32, sex: i32, form: &NewChar, env: &Env) -> [i32; 4] {
    let def = if city == 1 {
        if env.expansion & 2 != 0 { if sex == 0 { [2, 2, 2, 1] } else { [0, 0, 3, 1] } } else { [2, 0, 2, 2] }
    } else {
        let info = env.race_info(race);
        if sex == 0 { info.male } else { info.female }
    };
    let pick = |v: i32, d: i32| if v != -1 { v } else { d };
    [pick(form.face, def[0]), pick(form.hair_style, def[1]), pick(form.hair_length, def[2]), pick(form.hair_color, def[3])]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, mid};

    fn vui() -> vi::ui::Vui {
        super::super::tests_vui()
    }

    fn page() -> RacePage {
        let mut p = RacePage::default();
        p.activate(0, &super::super::blank_form(), 3);
        p
    }

    #[test]
    fn the_list_moves_and_stops_at_its_ends_with_the_games_sounds() {
        let mut p = page();
        let mut env = Env::default();
        let mut s = Vec::new();
        assert_eq!(p.list.len(), 11);
        p.key(key::UP, 0, &mut env, &mut s);
        assert_eq!((p.sel, s.as_slice()), (0, &[9][..]), "above the top: refused");
        s.clear();
        for _ in 0..12 {
            p.key(key::DOWN, 0, &mut env, &mut s);
        }
        assert_eq!(p.sel, 10);
        assert_eq!(p.top, 1, "ten rows at a time");
        assert_eq!(s.iter().filter(|&&v| v == 9).count(), 2, "two past the end");
        // the sex: 0..1
        s.clear();
        p.key(key::RIGHT, 0, &mut env, &mut s);
        p.key(key::RIGHT, 0, &mut env, &mut s);
        assert_eq!((p.sex, s.as_slice()), (1, &[11, 9][..]));
    }

    #[test]
    fn enter_hands_up_the_race_sex_and_city_and_back_is_260() {
        let mut p = page();
        let mut env = Env::default();
        let mut s = Vec::new();
        // to the eastern humans (row 7), female
        for _ in 0..7 {
            p.key(key::DOWN, 0, &mut env, &mut s);
        }
        p.key(key::RIGHT, 0, &mut env, &mut s);
        assert_eq!(p.key(key::CROSS, 0, &mut env, &mut s), Some(Msg::Race { race: 0, sex: 1, city: 1 }));
        assert_eq!((p.sel, p.sex), (0, 0), "the cursor goes back to the top");
        assert_eq!(p.key(key::TRIANGLE, 0, &mut env, &mut s), Some(Msg::Back));
        assert_eq!(s.last(), Some(&8));
    }

    #[test]
    fn coming_back_finds_the_forms_race() {
        let mut p = RacePage::default();
        let form = NewChar { race: 5, city: 0, sex: 1, ..super::super::blank_form() };
        p.activate(0, &form, 3);
        assert_eq!((p.list[p.sel].race, p.sex, p.top), (5, 1, 1), "the troll, the last row, scrolled into view");
        // the control: a blank form starts at the top
        p.activate(0, &super::super::blank_form(), 3);
        assert_eq!((p.sel, p.sex, p.top), (0, 0, 0));
    }

    #[test]
    fn the_look_is_the_forms_else_the_defaults() {
        let env = Env { expansion: 3, ..Default::default() };
        let blank = super::super::blank_form();
        assert_eq!(look_for(0, 1, 0, &blank, &env), [2, 2, 2, 1], "an eastern man of Frontiers");
        assert_eq!(look_for(0, 1, 1, &blank, &env), [0, 0, 3, 1]);
        let old = Env { expansion: 1, ..Default::default() };
        assert_eq!(look_for(0, 1, 0, &blank, &old), [2, 0, 2, 2], "the original's");
        let form = NewChar { face: 3, hair_color: 7, ..blank };
        assert_eq!(look_for(0, 1, 0, &form, &env), [3, 2, 2, 7], "the form's own where set");
        // a race's default with no CLIENT image here: zeros, the control
        assert_eq!(look_for(6, 0, 0, &super::super::blank_form(), &env), [0, 0, 0, 0]);
    }

    /// A pointer on a row moves the cursor there; on the picked row it is
    /// Enter; the sex's halves and the legend; nothing off them.
    #[test]
    fn a_pointer_picks_rows_the_sex_and_the_legend() {
        let mut p = page();
        let mut env = Env::default();
        let mut r = NullRaster;
        let v = vui();
        let mut music = Vec::new();
        let mut cues = Vec::new();
        let res = crate::pregame::res::Res::new();
        let mut shared = crate::pregame::Shared::default();
        let mut cx = Ctx { now: 0, game_frame: true, vui: &v, r: &mut r, res: &res, shared: &mut shared, cues: &mut cues, music: &mut music, gpu: None };
        let form = super::super::blank_form();
        let mut s = Vec::new();
        p.draw(&mut cx, &mut env, None, &form, &mut s);
        let rows = p.rows.clone();
        assert_eq!(rows.len(), 10, "ten rows drawn");
        assert_eq!(p.point(mid(rows[3])), vec![key::DOWN; 3]);
        assert_eq!(p.point(mid(rows[0])), vec![key::CROSS], "the picked row: Enter");
        assert!(p.point([630.0, 440.0]).is_empty(), "off everything");
        // the control: without the hit notes nothing answers
        p.hits.clear();
        assert!(p.point(mid(rows[3])).is_empty());
    }

    /// In a window wider than 4:3 the list's open left edge is closed as
    /// character select's is (the same layout, `select::PANEL`); a 4:3
    /// window draws no post (the control), and the rest is the same.
    #[test]
    fn a_wide_window_closes_the_lists_left_edge() {
        let v = crate::startmenu::posts::vui();
        let res = crate::pregame::res::Res::new();
        let form = super::super::blank_form();
        let drawn = |side: i32| {
            let mut r = crate::pointer::testing::Wide::new(side);
            let (mut shared, mut cues, mut music) = (crate::pregame::Shared::default(), Vec::new(), Vec::new());
            let mut cx = Ctx { now: 0, game_frame: true, vui: &v, r: &mut r, res: &res, shared: &mut shared, cues: &mut cues, music: &mut music, gpu: None };
            page().draw(&mut cx, &mut Env::default(), None, &form, &mut Vec::new());
            r.2
        };
        let (tv, wide) = (drawn(0), drawn(106));
        let added: Vec<_> = wide.iter().filter(|b| !tv.contains(b)).copied().collect();
        let panel = crate::pregame::client::select::PANEL;
        assert_eq!(added, crate::startmenu::posts::made_at(render::ui::Anchor::Middle, panel, false, [true, false]));
        let rest: Vec<_> = wide.iter().filter(|b| !added.contains(b)).copied().collect();
        assert_eq!(rest, tv);
    }
}
