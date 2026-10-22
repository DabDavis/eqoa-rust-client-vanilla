//! `VIWndCharCreateClass` (CLIENT `viwndcharcreateclass.cpp`): the class
//! page. Page 0 lists every class the owned expansions allow (`BuildList`),
//! the ones the race and city cannot be grey and skipped by the cursor, the
//! highlighted class's base stats (60, the race's bonus and the class's) and
//! its description; page 1 spends 20 points over the seven stats (never
//! below the bonuses), each stat's description below. Enter on page 0 hands
//! up the class (263) and turns to page 1; Enter there hands up the stats
//! less the bonuses (265) when no more than 20 were spent. The "Bonus
//! Points" row and the class's favoured stats pulse toward (10, 50, 70) and
//! back; the cursor's arrows pulse in alpha.

use super::tables;
use super::{Msg, legend, word};
use crate::pointer::{Hits, key};
use crate::pregame::Ctx;
use net::session::NewChar;
use vi::ui::{FrameParam, Window};

/// What a hit is: a class row (its index), a stat row (100 + the row), a
/// stat's value halves, the legend.
const STAT: i32 = 100;
const LESS: i32 = 200;
const MORE: i32 = 201;
const TRIANGLE: i32 = 202;
const CROSS: i32 = 203;

pub struct ClassPage {
    pub sel: i32,
    race: i32,
    city: i32,
    /// the class picked (page 1), -1 none yet
    pub class: i32,
    pub points: i32,
    /// 0 the classes, 1 the stats
    pub page: i32,
    /// STR STA AGI DEX WIS INT CHA over 60: the bonuses and the points spent
    pub stats: [i32; 7],
    /// the arrows' pulse (`m_p64`, `m_p6C`) and the colour's (`m_p68`, `m_color`)
    arrow_dir: i32,
    color_dir: i32,
    arrow: f32,
    color: [f32; 4],
    time: i64,
    window: Window,
    /// the description shown: a word, -1 none (`m_text`)
    text: i32,
    /// the classes on offer: class, description's word
    pub list: Vec<(i32, i32)>,
    hits: Hits,
    /// the class rows' rectangles as drawn, the stat rows', and each stat's
    /// value box
    pub(super) class_rows: Vec<[i32; 4]>,
    pub(super) stat_rows: Vec<[i32; 4]>,
}

impl Default for ClassPage {
    fn default() -> Self {
        ClassPage {
            sel: 0,
            race: 0,
            city: 0,
            class: -1,
            points: 20,
            page: 0,
            stats: [0; 7],
            arrow_dir: 1,
            color_dir: 1,
            arrow: 1.0,
            color: [1.0; 4],
            time: 0,
            window: Window::default(),
            text: -1,
            list: Vec::new(),
            hits: Hits::default(),
            class_rows: Vec::new(),
            stat_rows: Vec::new(),
        }
    }
}

impl ClassPage {
    /// `Reset`.
    pub fn reset(&mut self) {
        self.sel = 0;
        self.race = 0;
        self.city = 0;
        self.class = -1;
        self.points = 20;
        self.page = 0;
        self.arrow_dir = 1;
        self.color_dir = 1;
        self.arrow = 1.0;
        self.color = [1.0; 4];
        self.text = -1;
    }

    /// `OnMessage` 295: the pulses start over, the list built and the
    /// cursor on the form's class (`Refresh`).
    pub fn activate(&mut self, now: i64, form: &NewChar, expansion: u32, _vui: &vi::ui::Vui) {
        self.arrow_dir = 1;
        self.arrow = 1.0;
        self.time = now;
        self.window.open(true);
        self.list = tables::class_list(expansion);
        self.refresh(form, expansion, now);
    }

    /// `SetRace` (the form's 262): page 0 for this race and city.
    pub fn set_race(&mut self, race: i32, city: i32, form: &NewChar, expansion: u32, now: i64) {
        self.page = 0;
        self.race = race;
        self.city = city;
        self.class = -1;
        self.list = tables::class_list(expansion);
        self.refresh(form, expansion, now);
    }

    fn valid(&self, i: i32, expansion: u32) -> bool {
        self.list.get(i as usize).is_some_and(|&(c, _)| tables::valid_combo_expansion(self.race, c, self.city, expansion))
    }

    /// `Refresh`: the cursor onto the form's class, or the first one allowed.
    fn refresh(&mut self, form: &NewChar, expansion: u32, now: i64) {
        if self.race < 0 || self.city < 0 {
            return;
        }
        self.sel = self.list.iter().position(|&(c, _)| c == form.class).unwrap_or(0) as i32;
        if !self.valid(self.sel, expansion) {
            self.move_sel(1, false, expansion, now, &mut Vec::new());
        }
        self.update_text();
    }

    /// `Pulse`.
    fn pulse(&mut self, now: i64) {
        self.arrow_dir = 1;
        self.arrow = 1.0;
        self.time = now;
    }

    /// `UpdateText`: the class's description and its base stats (page 0),
    /// or the stat's description (page 1).
    fn update_text(&mut self) {
        self.text = -1;
        if self.page == 0 {
            if let Some(&(c, w)) = self.list.get(self.sel as usize) {
                self.text = w;
                self.set_base_stats(c);
            }
        } else if self.page == 1 {
            self.text = tables::STAT_ROWS[self.sel.clamp(0, 6) as usize].2;
        }
    }

    /// `SetBaseStats`: the race's bonus and the class's.
    fn set_base_stats(&mut self, c: i32) {
        let (race, class) = (tables::race(self.race), tables::class(c));
        for &(s, _, _) in &tables::STAT_ROWS {
            self.stats[s] = race.bonus[s] + class.bonus[s];
        }
    }

    /// `Move(delta, sound)`: page 0 skips the classes the race and city
    /// cannot be.
    fn move_sel(&mut self, delta: i32, sound: bool, expansion: u32, now: i64, sounds: &mut Vec<i32>) {
        self.pulse(now);
        let mut delta = delta;
        let count = self.list.len() as i32;
        let bound = if self.page == 0 { count } else { 7 };
        if self.sel + delta < 0 {
            if sound {
                sounds.push(9);
            }
            self.sel = 0;
        } else if self.sel + delta >= bound {
            if sound {
                sounds.push(9);
            }
            self.sel = bound - 1;
        } else {
            if sound {
                sounds.push(11);
            }
            self.sel += delta;
        }
        if self.page == 0 {
            // at most the list's length of steps: a race no class allows
            // would loop for good on the console
            for _ in 0..count * 2 {
                if self.valid(self.sel, expansion) {
                    break;
                }
                if self.sel == 0 {
                    delta = 1;
                } else if self.sel == count - 1 {
                    delta = -1;
                }
                self.sel += delta;
            }
        }
        self.update_text();
    }

    /// `AdjustStat`: the stat row by a point, never below the bonuses, no
    /// more than the 20 points.
    fn adjust_stat(&mut self, delta: i32, now: i64, sounds: &mut Vec<i32>) {
        self.pulse(now);
        if self.points <= 0 && delta > 0 {
            sounds.push(9);
            return;
        }
        if self.points - delta > 20 {
            sounds.push(9);
            return;
        }
        let s = tables::STAT_ROWS[self.sel.clamp(0, 6) as usize].0;
        let base = tables::race(self.race).bonus[s] + tables::class(self.class).bonus[s];
        if delta < 0 && self.stats[s] == base {
            return;
        }
        if self.stats[s] + delta < base {
            return;
        }
        sounds.push(11);
        self.points -= delta;
        self.stats[s] += delta;
    }

    /// `OnKeyDown`.
    pub fn key(&mut self, key: i32, now: i64, form: &NewChar, expansion: u32, sounds: &mut Vec<i32>, _vui: &vi::ui::Vui) -> Option<Msg> {
        let _ = form;
        match key {
            13 | key::CROSS => {
                sounds.push(13);
                if self.page == 0 {
                    let c = self.list.get(self.sel as usize).map_or(0, |l| l.0);
                    self.class = c;
                    self.pulse(now);
                    self.sel = 0;
                    self.points = 20;
                    self.page = 1;
                    self.update_text();
                    return Some(Msg::Class(c));
                }
                let (race, class) = (tables::race(self.race), tables::class(self.class));
                let mut c = [0; 7];
                let mut total = 0;
                for &(s, _, _) in &tables::STAT_ROWS {
                    c[s] = self.stats[s] - race.bonus[s] - class.bonus[s];
                    total += c[s];
                }
                if total <= 20 {
                    self.sel = 0;
                    self.page = 0;
                    self.points = 20;
                    self.move_sel(-1, false, expansion, now, sounds);
                    self.update_text();
                    return Some(Msg::Stats(c));
                }
                None
            }
            27 | key::TRIANGLE => {
                sounds.push(8);
                if self.page == 0 {
                    self.sel = 0;
                    self.points = 20;
                    self.update_text();
                    self.stats = [0; 7];
                    return Some(Msg::Back);
                }
                self.page = 0;
                self.pulse(now);
                self.sel = 0;
                self.points = 20;
                self.update_text();
                if let Some(i) = self.list.iter().position(|&(c, _)| c == self.class) {
                    self.sel = i as i32;
                    self.update_text();
                }
                None
            }
            19 | key::UP => {
                self.move_sel(-1, true, expansion, now, sounds);
                None
            }
            20 | key::DOWN => {
                self.move_sel(1, true, expansion, now, sounds);
                None
            }
            17 | key::LEFT => {
                if self.page == 1 {
                    self.adjust_stat(-1, now, sounds);
                }
                None
            }
            18 | key::RIGHT => {
                if self.page == 1 {
                    self.adjust_stat(1, now, sounds);
                }
                None
            }
            _ => None,
        }
    }

    /// `OnDrawSelf`: the two framed panels, the title, the legend, the class
    /// list, the points and the seven stat bars, the description; then the
    /// pulses step.
    pub fn draw(&mut self, cx: &mut Ctx, form: &NewChar, expansion: u32) {
        let _ = form;
        let vui = cx.vui;
        let now = cx.now;
        let dt = (now - self.time) as f32 * 0.001;
        self.time = now;
        let white = [1.0, 1.0, 1.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        let yellow = [1.0, 1.0, 0.0, 1.0];
        let grey = [0.35, 0.35, 0.35, 1.0];
        let blue = [10.0 / 255.0, 50.0 / 255.0, 70.0 / 255.0, 1.0f32];
        self.hits.clear();
        self.class_rows.clear();
        self.stat_rows.clear();
        let r = &mut *cx.r;
        vui.blit_rect(r, 32, [-1, -1], [642, 450], 0, 1.0);
        let (c2, c3) = (vui.entry(36).uv1, vui.entry(39).uv1);
        let size = [284, 377];
        let tl = [1, 37];
        let at = [tl[0] + c2[0], tl[1] + c2[1]];
        let fp = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
        let inner = [size[0] - c2[0] - c3[0], size[1] - c2[1] - c3[1]];
        vui.blit_fill_frame(r, at, inner, &fp, 35, 0, 1.0, true);
        let frame = FrameParam { tl: (36, 0), top: (37, 0), tr: (36, 1), right: (42, 1), br: (39, 1), bottom: (40, 0), bl: (39, 0), left: (42, 0) };
        let studs = FrameParam { tl: (0, 0), top: (38, 0), tr: (0, 0), right: (43, 1), br: (0, 0), bottom: (41, 0), bl: (0, 0), left: (43, 0) };
        vui.blit_studded_frame(r, tl, size, &frame, &studs, 2, 3, 0, 1.0, true);
        let at = [tl[0] + size[0], tl[1] + c2[1]];
        let inner = [340, 348];
        vui.blit_rect(r, 51, at, inner, 0, 1.0);
        let at = [tl[0] + size[0], tl[1]];
        let inner = [inner[0] + c2[0], inner[1] + c2[1] + c3[1]];
        vui.blit_studded_frame(r, at, inner, &frame, &studs, 3, 2, 0, 1.0, false);
        let at = [at[0] + 10, at[1] + 225];
        vui.blit_fill_frame(r, at, [320, 135], &fp, 35, 0, 1.0, true);
        let title = word(vui, if self.page != 0 { 603 } else { 258 });
        let w = r.string_width(&title) + 64;
        vui.blit_bar(r, &title, white, [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
        let [tri, cross] = legend(vui, r, 217, if self.page == 0 { 212 } else { 216 });
        self.hits.push(tri, TRIANGLE);
        self.hits.push(cross, CROSS);
        // the classes
        let mut at = [60, 87];
        let fh = r.point_size();
        for (i, &(c, _)) in self.list.iter().enumerate() {
            let mut colour = black;
            if (i as i32 == self.sel && self.page == 0) || (c == self.class && self.page == 1) {
                let arrow = vui.entry(85);
                vui.blit_ex(r, arrow.x00, [at[0] - 32, at[1] - arrow.uv1[1] / 2], 3, if self.page != 0 { 1.0 } else { self.arrow }, 0);
                colour = yellow;
            }
            let valid = tables::valid_combo_expansion(self.race, c, self.city, expansion);
            if !valid {
                colour = grey;
            }
            r.draw_centered_string(at, colour, &word(vui, tables::class(c).name), 1);
            let rect = [at[0] - 32, at[1] - fh / 2, at[0] + 200, at[1] + 19 - fh / 2];
            self.class_rows.push(rect);
            if self.page == 0 && valid {
                self.hits.push(rect, i as i32);
            }
            at[1] += 19;
        }
        // the points and the stats
        let bar_h = vui.entry(54).uv1[1] - 3;
        let mut at = [295, 65];
        let mut num = [530, 65];
        vui.blit_bar3(r, &word(vui, 255), self.color, at, 53, 55, 54, 235, 0, 1.0, 1, false);
        vui.blit_bar3(r, &vi::ui::wide(&self.points.to_string()), self.color, num, 53, 55, 54, 85, 0, 1.0, 0, false);
        at[1] += bar_h;
        num[1] += bar_h;
        let arrows = vui.entry(126);
        for (k, &(s, name, _)) in tables::STAT_ROWS.iter().enumerate() {
            let mut mark = [0, 0];
            let (l, m, rr, c, shadow) = if k as i32 == self.sel && self.page == 1 {
                mark = num;
                (56, 58, 57, yellow, true)
            } else {
                let cls = if self.page == 0 { self.list.get(self.sel as usize).map_or(0, |l| l.0) } else { self.class };
                if tables::stat_highlighted(cls, s as i32) { (53, 55, 54, self.color, false) } else { (53, 55, 54, white, true) }
            };
            vui.blit_bar3(r, &word(vui, name), c, at, l, m, rr, 235, 0, 1.0, 1, shadow);
            vui.blit_bar3(r, &vi::ui::wide(&(self.stats[s] + 60).to_string()), c, num, l, m, rr, 85, 0, 1.0, 0, shadow);
            if mark[0] > 0 && mark[1] > 0 {
                mark[0] += 1;
                mark[1] += arrows.uv1[1] / 2 - 3;
                vui.blit_ex(r, arrows.x00, mark, 3, self.arrow, 0);
                mark[0] = mark[0] - 4 + (85 - arrows.uv1[0]);
                vui.blit_ex(r, arrows.x00, mark, 3, self.arrow, 1);
            }
            let row = [at[0], at[1], num[0] + 85, at[1] + bar_h];
            self.stat_rows.push(row);
            if self.page == 1 {
                // the row, and over it the picked one's value halves
                self.hits.push(row, STAT + k as i32);
                if k as i32 == self.sel {
                    self.hits.push([num[0], num[1], num[0] + 42, num[1] + bar_h], LESS);
                    self.hits.push([num[0] + 42, num[1], num[0] + 85, num[1] + bar_h], MORE);
                }
            }
            at[1] += bar_h;
            num[1] += bar_h;
        }
        self.window.init(vui, r, [455, 328], 340, 7, 1);
        let text = if self.text >= 0 { word(vui, self.text) } else { Vec::new() };
        let ww = self.window.size[0];
        self.window.text_box(vui, r, &text, 20, 0, ww - 40, 7, black, 2);
        // the arrows: down at twice the time, up at three times
        if self.arrow_dir == -1 {
            self.arrow = (self.arrow - dt * 2.0).max(0.0);
            if self.arrow == 0.0 {
                self.arrow_dir = 1;
            }
        } else {
            self.arrow = (self.arrow + dt * 3.0).min(1.0);
            if self.arrow == 1.0 {
                self.arrow_dir = -1;
            }
        }
        // the colour: down to the blue at the time, up to white at twice it
        if self.color_dir == -1 {
            for i in 0..3 {
                self.color[i] = (self.color[i] - dt).max(blue[i]);
            }
            if (0..3).all(|i| self.color[i] == blue[i]) {
                self.color_dir = 1;
            }
        } else {
            for i in 0..3 {
                self.color[i] = (self.color[i] + dt * 2.0).min(1.0);
            }
            if (0..3).all(|i| self.color[i] == 1.0) {
                self.color_dir = -1;
            }
        }
    }

    /// A pointer at frame point `p`: a class the race may be moves the
    /// cursor there (the picked one picks it), a stat row the cursor there,
    /// the picked stat's value its left or right half, the legend's glyphs.
    pub fn point(&mut self, p: [f32; 2], form: &NewChar, expansion: u32) -> Vec<i32> {
        let _ = form;
        match self.hits.at(p) {
            Some(TRIANGLE) => vec![key::TRIANGLE],
            Some(CROSS) => vec![key::CROSS],
            Some(LESS) => vec![key::LEFT],
            Some(MORE) => vec![key::RIGHT],
            Some(s) if (STAT..STAT + 7).contains(&s) => {
                let s = s - STAT;
                if s == self.sel { Vec::new() } else { crate::pointer::steps(self.sel, s, key::UP, key::DOWN) }
            }
            Some(row) if row == self.sel => vec![key::CROSS],
            Some(row) => self.keys_to(row, expansion),
            None => Vec::new(),
        }
    }

    /// The d-pad's presses that bring page 0's cursor to `row` (the cursor
    /// skips the classes the race cannot be), tried on a copy.
    fn keys_to(&self, row: i32, expansion: u32) -> Vec<i32> {
        let k = if row < self.sel { key::UP } else { key::DOWN };
        let mut sim = ClassPage { list: self.list.clone(), sel: self.sel, race: self.race, city: self.city, page: self.page, ..Default::default() };
        let mut keys = Vec::new();
        for _ in 0..self.list.len() {
            if sim.sel == row {
                return keys;
            }
            sim.move_sel(if k == key::UP { -1 } else { 1 }, false, expansion, 0, &mut Vec::new());
            keys.push(k);
        }
        if sim.sel == row { keys } else { Vec::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::testing::{NullRaster, mid};

    fn vui() -> vi::ui::Vui {
        super::super::tests_vui()
    }

    fn barbarian() -> ClassPage {
        let mut p = ClassPage::default();
        let form = super::super::blank_form();
        p.set_race(6, 0, &form, 3, 0);
        p
    }

    #[test]
    fn the_cursor_skips_what_the_race_cannot_be() {
        let p = barbarian();
        // alphabetical, the barbarian's first allowed: rogue (row 10)
        assert_eq!(p.list.len(), 15);
        assert_eq!(p.list[p.sel as usize].0, 6, "the rogue");
        let mut p = p;
        let mut s = Vec::new();
        p.key(key::DOWN, 0, &super::super::blank_form(), 3, &mut s, &vui());
        assert_eq!(p.list[p.sel as usize].0, 8, "the shaman, the shadowknight skipped");
        p.key(key::DOWN, 0, &super::super::blank_form(), 3, &mut s, &vui());
        assert_eq!(p.list[p.sel as usize].0, 0, "the warrior");
        p.key(key::DOWN, 0, &super::super::blank_form(), 3, &mut s, &vui());
        assert_eq!(p.list[p.sel as usize].0, 0, "the wizard is past: the cursor comes back");
        // the base stats follow the cursor: a barbarian warrior
        assert_eq!(p.stats, [25, 25, 0, 5, -5, -5, -10]);
    }

    #[test]
    fn twenty_points_never_below_the_bonuses() {
        let mut p = barbarian();
        let form = super::super::blank_form();
        let mut s = Vec::new();
        let v = vui();
        assert_eq!(p.key(key::CROSS, 0, &form, 3, &mut s, &v), Some(Msg::Class(6)));
        assert_eq!((p.page, p.points), (1, 20));
        // with every point unspent, a point back is refused aloud (the
        // points' check comes first)
        s.clear();
        p.key(key::LEFT, 0, &form, 3, &mut s, &v);
        assert_eq!((p.points, s.as_slice()), (20, &[9][..]));
        // one into strength, then stamina at its bonus: refused, silently
        p.key(key::RIGHT, 0, &form, 3, &mut s, &v);
        p.key(key::DOWN, 0, &form, 3, &mut s, &v);
        s.clear();
        p.key(key::LEFT, 0, &form, 3, &mut s, &v);
        assert_eq!((p.points, s.len()), (19, 0));
        // the rest into strength, the twenty-first refused
        p.key(key::UP, 0, &form, 3, &mut s, &v);
        for _ in 0..20 {
            p.key(key::RIGHT, 0, &form, 3, &mut s, &v);
        }
        assert_eq!(p.points, 0);
        assert_eq!(s.last(), Some(&9));
        assert_eq!(p.stats[0], 15 + 20);
        // Enter: the points over the bonuses go up
        let m = p.key(key::CROSS, 0, &form, 3, &mut s, &v);
        assert_eq!(m, Some(Msg::Stats([20, 0, 0, 0, 0, 0, 0])));
        assert_eq!(p.page, 0);
    }

    #[test]
    fn back_from_the_stats_finds_the_class() {
        let mut p = barbarian();
        let form = super::super::blank_form();
        let v = vui();
        let mut s = Vec::new();
        p.key(key::DOWN, 0, &form, 3, &mut s, &v);
        p.key(key::CROSS, 0, &form, 3, &mut s, &v);
        assert_eq!(p.class, 8);
        assert_eq!(p.key(key::TRIANGLE, 0, &form, 3, &mut s, &v), None);
        assert_eq!((p.page, p.list[p.sel as usize].0), (0, 8));
        // and back once more is the page's 260
        assert_eq!(p.key(key::TRIANGLE, 0, &form, 3, &mut s, &v), Some(Msg::Back));
    }

    #[test]
    fn a_human_city_picks_its_classes() {
        let mut p = ClassPage::default();
        p.set_race(0, 2, &super::super::blank_form(), 3, 0);
        // the western humans' first allowed in the list: alchemist (Frontiers)
        assert_eq!(p.list[p.sel as usize].0, 14);
        let mut q = ClassPage::default();
        q.set_race(0, 2, &super::super::blank_form(), 1, 0);
        assert_eq!(q.list.len(), 14, "no alchemist without Frontiers");
        assert_eq!(q.list[q.sel as usize].0, 5, "the bard first, the control");
    }

    /// A pointer: a class row walks the cursor there, a grey one does
    /// nothing; on page 1 the picked stat's value halves are left and right.
    #[test]
    fn a_pointer_picks_classes_and_steps_stats() {
        let mut p = barbarian();
        let v = vui();
        let mut r = NullRaster;
        let res = crate::pregame::res::Res::new();
        let mut shared = crate::pregame::Shared::default();
        let (mut cues, mut music) = (Vec::new(), Vec::new());
        let mut cx = Ctx { now: 0, game_frame: true, vui: &v, r: &mut r, res: &res, shared: &mut shared, cues: &mut cues, music: &mut music, gpu: None };
        let form = super::super::blank_form();
        p.draw(&mut cx, &form, 3);
        let warrior = p.list.iter().position(|l| l.0 == 0).unwrap();
        let wizard = p.list.iter().position(|l| l.0 == 13).unwrap();
        assert_eq!(p.point(mid(p.class_rows[warrior]), &form, 3), vec![key::DOWN, key::DOWN]);
        assert!(p.point(mid(p.class_rows[wizard]), &form, 3).is_empty(), "a grey class");
        assert_eq!(p.point(mid(p.class_rows[p.sel as usize]), &form, 3), vec![key::CROSS]);
        // page 1
        let mut s = Vec::new();
        p.key(key::CROSS, 0, &form, 3, &mut s, &v);
        let mut cx = Ctx { now: 0, game_frame: true, vui: &v, r: &mut r, res: &res, shared: &mut shared, cues: &mut cues, music: &mut music, gpu: None };
        p.draw(&mut cx, &form, 3);
        let row = p.stat_rows[0];
        let value = [530, row[1], 530 + 85, row[3]];
        assert_eq!(p.point([(value[0] + 10) as f32, mid(value)[1]], &form, 3), vec![key::LEFT]);
        assert_eq!(p.point([(value[2] - 10) as f32, mid(value)[1]], &form, 3), vec![key::RIGHT]);
        assert_eq!(p.point(mid(p.stat_rows[3]), &form, 3), vec![key::DOWN; 3]);
        // the control: no hit notes, no keys
        p.hits.clear();
        assert!(p.point(mid(p.stat_rows[3]), &form, 3).is_empty());
    }
}
