//! `VIWndCharCreateReview` (CLIENT `viwndcharcreatereview.cpp`): the last
//! page. The name, race, class and the seven stats (60, the bonuses and the
//! points spent) on the left; the model in its look on the right, a
//! flourish every ten seconds (action 15, else standing), framed by the
//! race's review camera turning round it; the starting city over the scene
//! (the humans' by city; a human ranger or druid's is Surefall Glade).
//! Enter creates (267), back is 260.

use super::tables;
use super::{Env, Msg, create_model, frame_camera, legend, word};
use crate::pointer::{Hits, key};
use crate::pregame::Ctx;
use crate::pregame::stage::Stage;
use net::session::NewChar;
use vi::fpu::{Ee, F};
use vi::ui::FrameParam;

const TRIANGLE: i32 = 1;
const CROSS: i32 = 2;

pub struct ReviewPage {
    /// counts down to the model's next flourish (`m_idle`)
    idle: i64,
    /// the clock at the last frame, -1 before the first (`m_time`)
    time: i64,
    hits: Hits,
}

impl Default for ReviewPage {
    fn default() -> Self {
        ReviewPage { idle: -1, time: -1, hits: Hits::default() }
    }
}

/// The starting city's word (`sHomes`; a human ranger or druid starts in
/// Surefall Glade, 602), or None for a race and city not in the table.
pub fn home_word(form: &NewChar) -> Option<i32> {
    let h = tables::HOMES.iter().find(|h| h.0 == form.race && h.1 == form.city)?;
    Some(if h.0 == 0 && (form.class == 1 || form.class == 7) { 602 } else { h.2 })
}

/// The stats the review shows: each the form's points, the race's and the
/// class's bonuses, over 60.
pub fn shown_stats(form: &NewChar) -> [i32; 7] {
    let (race, class) = (tables::race(form.race), tables::class(form.class));
    std::array::from_fn(|i| form.stats[i] + race.bonus[i] + class.bonus[i] + 60)
}

impl ReviewPage {
    /// `Reset`.
    pub fn reset(&mut self, env: &mut Env) {
        self.idle = 0;
        self.time = 0;
        env.release();
    }

    /// `OnKeyDown`: Enter creates (267), back is 260.
    pub fn key(&mut self, k: i32, env: &mut Env, sounds: &mut Vec<i32>) -> Option<Msg> {
        match k {
            13 | key::CROSS => {
                sounds.push(13);
                env.release();
                Some(Msg::Create)
            }
            27 | key::TRIANGLE => {
                sounds.push(8);
                env.release();
                Some(Msg::Back)
            }
            _ => None,
        }
    }

    /// `OnDrawSelf`.
    pub fn draw(&mut self, cx: &mut Ctx, env: &mut Env, stage: Option<&mut Stage>, form: &NewChar) {
        let vui = cx.vui;
        let now = cx.now;
        let white = [1.0, 1.0, 1.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        self.hits.clear();
        {
            let r = &mut *cx.r;
            let (c2, c3) = (vui.entry(36).uv1, vui.entry(39).uv1);
            let size = [284, 377];
            let tl = [1, 37];
            let at = [c2[0] + 1, c2[1] + 37];
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
            let title = word(vui, 261);
            let w = r.string_width(&title) + 64;
            vui.blit_bar(r, &title, white, [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
            let [tri, cross] = legend(vui, r, 217, 218);
            self.hits.push(tri, TRIANGLE);
            self.hits.push(cross, CROSS);
            // the name, race, class and stats, in the font's black
            let mut tp = [60, 96];
            r.draw_string(tp, black, &vi::ui::wide(&form.name));
            tp[1] += 19;
            r.draw_string(tp, black, &word(vui, tables::race(form.race).name));
            tp[1] += 19;
            r.draw_string(tp, black, &word(vui, tables::class(form.class).name));
            tp[1] += 38;
            for (i, v) in shown_stats(form).iter().enumerate() {
                r.draw_string(tp, black, &word(vui, 248 + i as i32));
                r.draw_string([tp[0] + 140, tp[1]], black, &vi::ui::wide(&v.to_string()));
                tp[1] += 19;
            }
        }
        // the model, its flourish, the camera, the scene
        if let Some(stage) = stage {
            if env.figure.is_none() {
                let c = &stage.cam.walk.committed;
                let (at, hpr) = (c.position.to_array(), c.angles.to_array());
                let look = [form.face, form.hair_style, form.hair_length, form.hair_color];
                env.figure = create_model(cx, tables::race(form.race).model_id(form.sex), look, at, hpr);
            }
            if self.time == -1 {
                self.time = now;
            }
            self.idle -= now - self.time;
            if let Some(f) = env.figure.as_mut() {
                f.anims.time = now;
                if self.idle <= 0 || self.idle > 10000 {
                    f.set_action(15);
                    self.idle = 10000;
                } else {
                    f.set_action(0);
                }
            }
            let framing = env.race_camera(form.race).map_or(super::DEFAULT_FRAMING, |c| c.review);
            frame_camera(stage, framing);
            stage.cam.orbit_right(F::<Ee>::new(0.3));
            stage.process(now);
            if let (Some(f), Some(g)) = (env.figure.as_mut(), cx.gpu.as_ref()) {
                f.pose(g, now);
                cx.r.mark_split();
                env.show3d = true;
            }
        }
        self.time = now;
        // the starting city, over the scene
        let r = &mut *cx.r;
        let mut label = word(vui, 406);
        label.extend(vi::ui::wide(": "));
        vui.draw_centered_string_ex(r, &label, 490, 350, white, 0, true);
        if let Some(w) = home_word(form) {
            let y = r.point_size() + 345;
            vui.draw_centered_string_ex(r, &word(vui, w), 490, y, white, 0, true);
        }
    }

    /// A pointer: the legend's glyphs.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.hits.at(p) {
            Some(TRIANGLE) => vec![key::TRIANGLE],
            Some(CROSS) => vec![key::CROSS],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_review_shows_the_stats_and_the_home() {
        let form = NewChar { race: 6, class: 8, sex: 0, city: 0, stats: [5, 0, 0, 0, 10, 5, 0], ..super::super::blank_form() };
        // barbarian 15 10 0 5 0 5 0, shaman -10 10 0 -5 15 -10 0
        assert_eq!(shown_stats(&form), [70, 80, 60, 60, 85, 60, 60]);
        assert_eq!(home_word(&form), Some(364), "Halas");
        let ranger = NewChar { race: 0, city: 2, class: 1, ..super::super::blank_form() };
        assert_eq!(home_word(&ranger), Some(602), "a human ranger: Surefall Glade");
        let warrior = NewChar { class: 0, ..ranger.clone() };
        assert_eq!(home_word(&warrior), Some(357), "the control: Qeynos");
        let east = NewChar { city: 1, ..warrior };
        assert_eq!(home_word(&east), Some(358), "Freeport");
        assert_eq!(home_word(&NewChar { race: 0, city: 0, ..super::super::blank_form() }), None, "a human of no city");
    }

    #[test]
    fn enter_creates_and_back_goes_back() {
        let mut p = ReviewPage::default();
        let mut env = Env::default();
        let mut s = Vec::new();
        assert_eq!(p.key(key::CROSS, &mut env, &mut s), Some(Msg::Create));
        assert_eq!(p.key(27, &mut env, &mut s), Some(Msg::Back));
        assert_eq!(s, vec![13, 8]);
        assert_eq!(p.key(key::UP, &mut env, &mut s), None, "the control: nothing else");
    }

    #[test]
    fn a_pointer_presses_the_legend() {
        let mut p = ReviewPage::default();
        let v = super::super::tests_vui();
        let mut r = crate::pointer::testing::NullRaster;
        let res = crate::pregame::res::Res::new();
        let mut shared = crate::pregame::Shared::default();
        let (mut cues, mut music) = (Vec::new(), Vec::new());
        let mut cx = Ctx { now: 0, game_frame: true, vui: &v, r: &mut r, res: &res, shared: &mut shared, cues: &mut cues, music: &mut music, gpu: None };
        let mut env = Env::default();
        let form = NewChar { race: 1, class: 13, ..super::super::blank_form() };
        p.draw(&mut cx, &mut env, None, &form);
        // the legend's two glyphs, on the bar at (50, 408)
        let found: Vec<i32> = (40..400).step_by(2).flat_map(|x| p.point([x as f32, 420.0])).collect();
        assert!(found.contains(&key::TRIANGLE) && found.contains(&key::CROSS));
        p.hits.clear();
        assert!((40..400).step_by(2).all(|x| p.point([x as f32, 420.0]).is_empty()), "the control");
    }
}
