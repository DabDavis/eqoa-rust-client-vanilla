//! The stat trainer (CLIENT `viwndtrainer.cpp`, VIWndGame mode 12, the
//! Character menu's Summary): the seven trainable stats with the points
//! left to spend (left/right a point off/on, held to repeat), "Train"
//! sending them (`TrainStats`, 0x001D with seven zigzag counts), "Details"
//! showing nine derived stats; under the selected stat its base and maximum,
//! and the equipped items that give it.
//!
//! Its modes (`m_mode`): 0 the stats, 1 the Train bar, 2 the Details bar,
//! 3 the details, 4 points sent and waiting for 0x001D.
//!
//! A pointer (`pointer`) moves as the keys move: a stat or a bar clicked is
//! the up and down steps to it (each step finds the stat's items, which only
//! the client's data can), the selected stat's value a point off on its left
//! half and on on its right, the selected bar ✕.

use crate::pointer::{Hits, key};
use crate::startmenu::KeyFlash;
use net::dump::ItemList;
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// `sStats`: (stat, caption, description).
const STATS: [(i32, i32); 7] = [(0, 248), (1, 249), (2, 250), (3, 251), (4, 252), (5, 253), (6, 254)];
/// `sDetails`: (stat, caption).
const DETAILS: [(i32, i32); 9] = [(14, 242), (7, 233), (9, 466), (24, 48), (25, 483), (26, 484), (27, 485), (22, 486), (23, 487)];
/// `sStatWords` (`FindStatWord`): (word, stat).
const STAT_WORDS: [(i32, i32); 19] = [(226, 0), (227, 1), (228, 2), (229, 3), (230, 4), (231, 5), (232, 6), (234, 8), (496, 10), (240, 12), (241, 13), (242, 14), (447, 24), (451, 25), (450, 26), (452, 27), (449, 23), (448, 22), (1203, 28)];
/// `VIClassDef::m_statMask` by class (`IsStatHighlighted`).
const STAT_MASKS: [i32; 15] = [3, 12, 3, 3, 10, 6, 12, 24, 18, 80, 36, 34, 96, 40, 48];
/// `VIClientPart4EC58::GetStat`'s slot -> id map (-1 none).
const MASTERY_IDS: [i32; 29] = [0, 1, 2, 3, 4, 5, 6, -1, -1, -1, -1, -1, -1, -1, 8, 10, 12, 13, 14, -1, -1, -1, 24, 26, 25, 27, 22, 23, -1];

/// `gRaces[r].m_nameStrID` (`GetRace`), the ten playable races.
const RACE_WORDS: [i32; 10] = [183, 185, 186, 187, 188, 189, 191, 192, 193, 190];

/// What the trainer notes where it is drawn, for a pointer: a stat's
/// caption (`STAT` + stat), its value's left and right halves (`LESS`,
/// `MORE` + stat), the bars, a detail's row (`DETAIL` + row).
const STAT: i32 = 0;
const LESS: i32 = 10;
const MORE: i32 = 20;
const TRAIN_BAR: i32 = 30;
const DETAILS_BAR: i32 = 31;
const DETAIL: i32 = 40;
/// A row of bars, 25 apart.
const ROW_H: i32 = 25;

const HI: [f32; 4] = [10.0 / 255.0, 50.0 / 255.0, 70.0 / 255.0, 1.0];
const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// What the trainer reads off the client.
pub struct Data<'a> {
    pub name: &'a str,
    pub class: i32,
    pub race: i32,
    pub level: i32,
    /// `m_player.m_baseStat`, `m_player.m_stat`
    pub base: &'a [i32; 29],
    pub cur: &'a [i32; 29],
    /// `m_player.m_p40` (training points), `m_p44` (the stat cap)
    pub p40: i32,
    pub p44: i32,
    /// `VIClientPart4EC58`'s stats and bonuses
    pub mastery: &'a [i64; 29],
    pub bonuses: &'a [i64; 7],
    pub inv: &'a ItemList,
    /// `gEquipSlotWords` (CLIENT's data): (word, slot)
    pub slot_words: &'a [(i32, i64)],
}

impl Data<'_> {
    /// `IsStatHighlighted(class, stat)`.
    fn highlighted(&self, stat: i32) -> bool {
        (0..7).contains(&stat) && (0..15).contains(&self.class) && (STAT_MASKS[self.class as usize] >> stat) & 1 != 0
    }

    /// `VIClientPart4EC58::GetStat(id)`.
    fn mastery(&self, id: i32) -> i32 {
        mastery_stat(self.mastery, id)
    }

    /// `GetP44Total(i)`: the cap with the mastery bonus for stat `i`.
    fn cap(&self, i: i32) -> i32 {
        stat_cap(self.p44, self.bonuses, i)
    }

    /// `FindStat(i, stat)`: an equipped item's bonus to `stat`.
    fn find_stat(&self, i: usize, stat: i32) -> Option<i64> {
        let it = self.inv.items.get(i)?;
        if it.equip_slot == -1 {
            return None;
        }
        it.item.stats.iter().find(|s| s.id == stat as i64).map(|s| s.value)
    }

    fn count_items(&self, stat: i32) -> i32 {
        (0..self.inv.items.len()).filter(|&i| self.find_stat(i, stat).is_some()).count() as i32
    }

    /// `BaseStat`/`CurStat`: the resists (16-27) are derived, 40 plus the
    /// stat plus a seventh of wisdom, 315 at most.
    pub(crate) fn derived(v: &[i32; 29], stat: i32) -> i32 {
        if !(0..29).contains(&stat) {
            return 0;
        }
        if (16..=27).contains(&stat) { (v[4] / 7 + v[stat as usize] + 40).min(315) } else { v[stat as usize] }
    }
}

/// `VIClientPart4EC58::GetStat(id)`: the masteries' share of stat `id`, out
/// of their 29 slots (`MASTERY_IDS`); 0 for a stat no slot holds.
pub(crate) fn mastery_stat(m: &[i64; 29], id: i32) -> i32 {
    MASTERY_IDS.iter().position(|&s| s == id).map_or(0, |k| m[k] as i32)
}

/// `GetP44Total(i)` over the stat cap `p44` and the masteries' bonuses: the
/// cap of trainable stat `i` (0-6), none for another.
pub(crate) fn stat_cap(p44: i32, bonuses: &[i64; 7], i: i32) -> i32 {
    p44 + if (0..7).contains(&i) { bonuses[i as usize] as i32 } else { 0 }
}

/// What the trainer asks of the game.
#[derive(Clone, Debug, PartialEq)]
pub enum TrainUp {
    /// 260: back to the start menu
    Close,
    /// `TrainStats`: 0x001D with the seven counts
    Train([i32; 7]),
}

/// `VIWndTrainer`.
pub struct Trainer {
    sel: i32,
    points: i32,
    mode: i32,
    add: [i32; 7],
    time: i64,
    fade_dir: i32,
    fade: f32,
    color_dir: i32,
    color: [f32; 4],
    window: Window,
    list: Window,
    last: i32,
    item: i32,
    flash: KeyFlash,
    hits: Hits,
}

impl Default for Trainer {
    fn default() -> Self {
        Trainer { sel: 0, points: 0, mode: 0, add: [0; 7], time: 0, fade_dir: 1, fade: 1.0, color_dir: 1, color: BLACK, window: Window::default(), list: Window::default(), last: 0, item: 0, flash: KeyFlash::default(), hits: Hits::default() }
    }
}

impl Trainer {

    /// A pointer at frame point `p` (`pointer`), as the keys: a stat, a
    /// detail or a bar not selected is the up and down steps to it (from a
    /// bar, up to the stats first); the selected stat's value a point off
    /// (left, its left half) or on (right); the selected bar ✕. Nothing while
    /// the points are out. The keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let Some(h) = self.hits.at(p) else { return Vec::new() };
        let steps = |from: i32, to: i32| crate::pointer::steps(from, to, key::UP, key::DOWN);
        match (self.mode, h) {
            (4, _) => Vec::new(),
            (3, h) if h >= DETAIL => steps(self.sel, h - DETAIL),
            (0, h) if (LESS..LESS + 7).contains(&h) && h - LESS == self.sel => vec![key::LEFT],
            (0, h) if (MORE..MORE + 7).contains(&h) && h - MORE == self.sel => vec![key::RIGHT],
            (0..=2, h) if h < TRAIN_BAR => {
                let stat = h % 10;
                if self.mode == 0 && stat == self.sel {
                    return Vec::new();
                }
                // up from a bar to the stats, where the selection is as it was
                let mut keys = vec![key::UP; self.mode as usize];
                keys.extend(steps(self.sel, stat));
                keys
            }
            (1, TRAIN_BAR) | (2, DETAILS_BAR) => vec![key::CROSS],
            (0, TRAIN_BAR) => vec![key::DOWN; (7 - self.sel).max(0) as usize],
            (0, DETAILS_BAR) => vec![key::DOWN; (8 - self.sel).max(0) as usize],
            (1, DETAILS_BAR) => vec![key::DOWN],
            (2, TRAIN_BAR) => vec![key::UP],
            _ => Vec::new(),
        }
    }
    /// `Open` (the player's points, mode 0), and `OnMessage` 295's windows.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, d: &Data, now: i64) {
        self.sel = 0;
        self.item = 0;
        self.mode = 0;
        self.points = d.p40;
        self.refresh(d);
        self.list.init(vui, r, [400, 220], 150, 9, 0);
        self.list.open(true);
        self.last = 0;
        self.window.init(vui, r, [320, 162], 640, 17, 0);
        self.window.open(true);
        self.flash.reset();
        self.time = now;
    }

    /// `Refresh` (VIWndGame::Msg001D, the points taken): nothing spent, the
    /// points again; a wait for them ends.
    pub fn refresh(&mut self, d: &Data) {
        self.add = [0; 7];
        self.points = d.p40;
        if self.mode == 4 {
            self.mode = 0;
            self.sel = 0;
            self.item = 0;
        }
    }

    fn reset_fade(&mut self, now: i64) {
        self.fade_dir = 1;
        self.fade = 1.0;
        self.time = now;
    }

    /// `Move(d)`: the selection by `d`, clamped, and the first item giving
    /// the stat.
    fn step(&mut self, d: &Data, k: i32, cues: &mut Vec<i32>) {
        let count = match self.mode {
            0 => 7,
            3 => 9,
            _ => {
                self.sel = 0;
                self.item = 0;
                return;
            }
        };
        if self.sel + k < 0 {
            cues.push(9);
            self.sel = 0;
        } else if self.sel + k >= count {
            cues.push(9);
            self.sel = count - 1;
        } else {
            cues.push(11);
            self.sel += k;
        }
        let stat = self.stat();
        self.item = (0..d.inv.items.len()).find(|&i| d.find_stat(i, stat).is_some()).unwrap_or(0) as i32;
    }

    fn stat(&self) -> i32 {
        if self.mode == 0 { STATS[self.sel.clamp(0, 6) as usize].0 } else { DETAILS[self.sel.clamp(0, 8) as usize].0 }
    }

    /// `Adjust(d)`: a point onto or off the selected stat.
    fn adjust(&mut self, d: &Data, k: i32, cues: &mut Vec<i32>) {
        let stat = STATS[self.sel.clamp(0, 6) as usize].0 as usize;
        if (self.points <= 0 && k > 0) || (self.points == d.p40 && k < 0) {
            cues.push(9);
            return;
        }
        let v = d.base[stat] + self.add[stat];
        if d.cap(-1) < v + k {
            cues.push(9);
            return;
        }
        if k < 0 && d.base[stat] == v {
            cues.push(9);
            return;
        }
        cues.push(11);
        self.points -= k;
        self.add[stat] += k;
    }

    /// `ItemDown`/`ItemUp`: the item list a row down or up.
    fn items(&mut self, d: &Data, down: bool, cues: &mut Vec<i32>) {
        let stat = self.stat();
        let n = d.inv.items.len() as i32;
        let has = |i: i32| d.find_stat(i as usize, stat).is_some();
        if down {
            let last = (self.item..n).rev().find(|&i| has(i));
            if Some(self.last) == last {
                cues.push(9);
                return;
            }
            match (self.item + 1..n).find(|&j| has(j)) {
                Some(j) => {
                    self.item = j;
                    cues.push(11);
                }
                None => cues.push(9),
            }
        } else {
            let first = (0..n).find(|&i| has(i));
            if Some(self.item) == first {
                cues.push(9);
                return;
            }
            match (0..self.item).rev().find(|&j| has(j)) {
                Some(j) => {
                    self.item = j;
                    cues.push(11);
                }
                None => cues.push(9),
            }
        }
    }

    /// `OnKeyDown`/`OnKeyUp`.
    pub fn key(&mut self, d: &Data, key: i32, down: bool, now: i64, cues: &mut Vec<i32>) -> Option<TrainUp> {
        if !down {
            match key {
                17 | 136 if self.mode == 0 => self.flash.release(136),
                18 | 137 if self.mode == 0 => self.flash.release(137),
                _ => {}
            }
            return None;
        }
        self.reset_fade(now);
        match key {
            19 | 138 => match self.mode {
                1 => {
                    cues.push(11);
                    self.mode = 0;
                    self.step(d, 0, cues);
                }
                2 => {
                    cues.push(11);
                    self.mode = 1;
                }
                0 | 3 => self.step(d, -1, cues),
                _ => cues.push(9),
            },
            20 | 139 => match self.mode {
                0 => {
                    if self.sel < 6 {
                        self.step(d, 1, cues);
                    } else {
                        cues.push(11);
                        self.mode = 1;
                    }
                }
                1 => {
                    cues.push(11);
                    self.mode = 2;
                }
                3 => self.step(d, 1, cues),
                _ => cues.push(9),
            },
            17 | 136 => match self.mode {
                0 => self.flash.press(136),
                4 => cues.push(9),
                _ => {}
            },
            18 | 137 => match self.mode {
                0 => self.flash.press(137),
                4 => cues.push(9),
                _ => {}
            },
            13 | 134 => match self.mode {
                1 => {
                    if self.add.iter().sum::<i32>() == 0 {
                        cues.push(10);
                    } else {
                        cues.push(13);
                        self.mode = 4;
                        return Some(TrainUp::Train(self.add));
                    }
                }
                2 => {
                    cues.push(13);
                    self.sel = 0;
                    self.item = 0;
                    self.mode = 3;
                    self.step(d, 0, cues);
                }
                4 => cues.push(9),
                _ => {}
            },
            27 | 132 => {
                cues.push(8);
                if self.mode == 3 {
                    self.mode = 0;
                    self.sel = 0;
                    self.step(d, 0, cues);
                } else {
                    return Some(TrainUp::Close);
                }
            }
            69 | 101 | 142 => match self.mode {
                0 | 3 => self.items(d, false, cues),
                4 => cues.push(9),
                _ => {}
            },
            67 | 99 | 143 => match self.mode {
                0 | 3 => self.items(d, true, cues),
                4 => cues.push(9),
                _ => {}
            },
            _ => {}
        }
        None
    }

    fn bar(vui: &Vui, r: &mut dyn Raster, s: &[u16], c: [f32; 4], at: [i32; 2], w: i32, label: bool) {
        vui.blit_bar3(r, s, c, at, 141, 142, 150, w, 0, 1.0, label as u32, false);
    }

    /// The arrow beside a row (atlas 85, centred on the bar's height).
    fn arrow_y(vui: &Vui, y: i32) -> i32 {
        y + vui.entry(150).uv1[1] / 2 - vui.entry(85).uv1[1] / 2 - 1
    }

    /// `OnDrawSelf`.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, d: &Data, now: i64, cues: &mut Vec<i32>) {
        let dt = (now - self.time) as f32 * 0.001;
        self.time = now;
        self.hits.clear();
        let word = |id: i32| vui.words.get(id, vui.language).to_string();
        crate::ui::window_backdrop(vui, r, 642);
        self.window.init(vui, r, [320, 162], 640, 17, 0);
        self.window.open(true);
        self.window.draw(vui, r);
        let legend = match self.mode {
            0 | 4 => format!("|-|x{}   |+|t{}", word(212), word(161)),
            3 => format!("|-|x{}   |+|t{}", word(212), word(217)),
            _ => format!("|x{}   |t{}", word(212), word(161)),
        };
        self.window.text(vui, r, &wide(&legend), 340, 16, UI_COLORS[0], 0);
        if self.mode != 3 {
            self.draw_stats(vui, r, d);
        } else {
            self.draw_details(vui, r, d);
        }
        let mut bar = [25, 250];
        let mut pos = [0, 0];
        if self.mode != 3 {
            Self::bar(vui, r, &wide(&word(482)), if self.mode == 1 { HI } else { BLACK }, bar, 200, true);
            self.hits.push([bar[0], bar[1], bar[0] + 200, bar[1] + ROW_H], TRAIN_BAR);
        }
        if self.mode == 1 {
            pos = [bar[0], Self::arrow_y(vui, bar[1])];
        }
        if self.mode != 3 {
            bar[1] += 25;
            Self::bar(vui, r, &wide(&word(77)), if self.mode == 2 { HI } else { BLACK }, bar, 200, true);
            self.hits.push([bar[0], bar[1], bar[0] + 200, bar[1] + ROW_H], DETAILS_BAR);
        }
        if self.mode == 2 {
            pos = [bar[0], Self::arrow_y(vui, bar[1])];
        }
        if self.mode == 1 || self.mode == 2 {
            vui.blit_ex(r, vui.entry(85).x00, pos, 3, self.fade, 0);
        }
        // the arrows' pulse, and the class stats' colour
        if self.fade_dir == -1 {
            self.fade = (self.fade - dt * 2.0).max(0.0);
            if self.fade == 0.0 {
                self.fade_dir = 1;
            }
        } else {
            self.fade = (self.fade + dt * 3.0).min(1.0);
            if self.fade == 1.0 {
                self.fade_dir = -1;
            }
        }
        if self.color_dir == -1 {
            for c in &mut self.color[..3] {
                *c = (*c - dt).max(0.0);
            }
            if self.color[..3].iter().all(|&c| c == 0.0) {
                self.color_dir = 1;
            }
        } else {
            for c in &mut self.color[..3] {
                *c = (*c + dt * 2.0).min(0.8);
            }
            if self.color[..3].iter().all(|&c| c == 0.8) {
                self.color_dir = -1;
            }
        }
        if self.mode == 0 {
            if self.flash.pulse(136) {
                self.adjust(d, -1, cues);
            }
            if self.flash.pulse(137) {
                self.adjust(d, 1, cues);
            }
        }
    }

    /// `DrawHeader`: "name: Level n race class".
    fn draw_header(vui: &Vui, r: &mut dyn Raster, d: &Data, at: [i32; 2]) {
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let class = w(if (0..15).contains(&d.class) { 165 + d.class } else { 165 });
        let s = format!("{}: {} {} {} {}", d.name, w(299), d.level, w(RACE_WORDS.get(d.race as usize).copied().unwrap_or(183)), class);
        r.draw_string(at, BLACK, &wide(&s));
    }

    /// `DrawStats`.
    fn draw_stats(&mut self, vui: &Vui, r: &mut dyn Raster, d: &Data) {
        let w = |id: i32| wide(vui.words.get(id, vui.language));
        let p0 = [25, 15];
        let mut p1 = [p0[0], p0[1] + 25];
        let mut p2 = [p1[0] + 300, p1[1]];
        let mut p3 = [p1[0] + 200, p1[1]];
        let mut p4 = [p2[0] + 220, p2[1]];
        Self::draw_header(vui, r, d, p0);
        if self.mode == 4 {
            Self::bar(vui, r, &w(665), BLACK, p1, 200, true);
            Self::bar(vui, r, &[], BLACK, p3, 75, false);
        } else {
            let c = if self.points < 1 { BLACK } else { self.color };
            Self::bar(vui, r, &w(489), c, p1, 200, true);
            Self::bar(vui, r, &wide(&self.points.to_string()), c, p3, 75, false);
        }
        p1[1] += 25;
        p3[1] += 25;
        for (i, &(stat, caption)) in STATS.iter().enumerate() {
            let lit = i as i32 == self.sel && self.mode == 0;
            let c = if lit {
                HI
            } else if d.highlighted(stat) {
                self.color
            } else {
                BLACK
            };
            Self::bar(vui, r, &w(caption), c, p1, 200, true);
            let v = d.cur[stat as usize] + self.add[stat as usize];
            Self::bar(vui, r, &wide(&v.to_string()), c, p3, 75, false);
            let i = i as i32;
            self.hits.push([p1[0], p1[1], p3[0], p1[1] + ROW_H], STAT + i);
            self.hits.push([p3[0], p3[1], p3[0] + 37, p3[1] + ROW_H], LESS + i);
            self.hits.push([p3[0] + 37, p3[1], p3[0] + 75, p3[1] + ROW_H], MORE + i);
            if lit {
                let y = Self::arrow_y(vui, p1[1]);
                vui.blit_ex(r, vui.entry(85).x00, [p1[0], y], 3, self.fade, 0);
                vui.blit_ex(r, 128, [p3[0] + 2, y], 3, self.fade, 1);
                vui.blit_ex(r, 128, [p3[0] + 62, y], 3, self.fade, 0);
            }
            p1[1] += 25;
            p3[1] += 25;
        }
        let (stat, caption) = STATS[self.sel.clamp(0, 6) as usize];
        let su = stat as usize;
        let label = |pre: i32| wide(&format!("{} {}", vui.words.get(pre, vui.language), vui.words.get(caption, vui.language)));
        let base = d.base[su] + self.add[su] + d.mastery(stat);
        Self::bar(vui, r, &label(480), BLACK, p2, 220, true);
        Self::bar(vui, r, &wide(&base.to_string()), BLACK, p4, 55, false);
        p2[1] += 25;
        p4[1] += 25;
        let max = (d.cur[su] + self.add[su]).min(d.cap(stat));
        Self::bar(vui, r, &label(481), BLACK, p2, 220, true);
        Self::bar(vui, r, &wide(&max.to_string()), BLACK, p4, 55, false);
        p2[1] += 35;
        self.draw_items(vui, r, d, p2, stat);
    }

    /// `DrawDetails`.
    fn draw_details(&mut self, vui: &Vui, r: &mut dyn Raster, d: &Data) {
        let w = |id: i32| wide(vui.words.get(id, vui.language));
        let p0 = [25, 15];
        let mut p1 = [p0[0], p0[1] + 25];
        let mut p2 = [p1[0] + 300, p1[1]];
        let mut p3 = [p1[0] + 200, p1[1]];
        let mut p4 = [p2[0] + 200, p2[1]];
        Self::draw_header(vui, r, d, p0);
        for (i, &(stat, caption)) in DETAILS.iter().enumerate() {
            let lit = i as i32 == self.sel && self.mode == 3;
            let c = if lit { HI } else { BLACK };
            Self::bar(vui, r, &w(caption), c, p1, 200, true);
            if lit {
                vui.blit_ex(r, vui.entry(85).x00, [p1[0], Self::arrow_y(vui, p1[1])], 3, self.fade, 0);
            }
            Self::bar(vui, r, &wide(&Data::derived(d.cur, stat).to_string()), c, p3, 75, false);
            self.hits.push([p1[0], p1[1], p3[0] + 75, p1[1] + ROW_H], DETAIL + i as i32);
            p1[1] += 25;
            p3[1] += 25;
        }
        let (stat, caption) = DETAILS[self.sel.clamp(0, 8) as usize];
        let label = |pre: i32| wide(&format!("{} {}", vui.words.get(pre, vui.language), vui.words.get(caption, vui.language)));
        let base = match stat {
            7 => d.base[8] + d.mastery(8),
            9 => d.base[10] + d.mastery(10),
            s => Data::derived(d.base, s) + d.mastery(s),
        };
        Self::bar(vui, r, &label(480), BLACK, p2, 200, true);
        Self::bar(vui, r, &wide(&base.to_string()), BLACK, p4, 75, false);
        p2[1] += 25;
        p4[1] += 25;
        Self::bar(vui, r, &label(481), BLACK, p2, 200, true);
        Self::bar(vui, r, &wide(&Data::derived(d.cur, stat).to_string()), BLACK, p4, 75, false);
        p2[1] += 35;
        let s = match stat {
            7 => 8,
            9 => 10,
            s => s,
        };
        self.draw_items(vui, r, d, p2, s);
    }

    /// `DrawItems`: the equipped items giving `stat`, ten rows from
    /// `m_item`, blue for a bonus and red for a malus.
    fn draw_items(&mut self, vui: &Vui, r: &mut dyn Raster, d: &Data, _at: [i32; 2], stat: i32) {
        let blue = HI;
        let red = [95.0 / 255.0, 25.0 / 255.0, 25.0 / 255.0, 1.0];
        self.list.init(vui, r, [462, 183], 295, 10, 1);
        self.list.open(true);
        let count = match self.mode {
            0 => d.count_items(STATS[self.sel.clamp(0, 6) as usize].0),
            3 => d.count_items(DETAILS[self.sel.clamp(0, 8) as usize].0),
            _ => return,
        };
        let n = (0..(self.item.max(0) as usize).min(d.inv.items.len())).filter(|&i| d.find_stat(i, stat).is_some()).count() as i32;
        let t = if count > 10 { n as f32 / (count - 10) as f32 } else { 0.0 }.clamp(0.0, 1.0);
        let width = self.list.size[0];
        self.list.draw_frame_vscroll(vui, r, 15, 0, width - 40, 10, t);
        let mut lines = 0;
        for i in self.item.max(0) as usize..d.inv.items.len() {
            let slot = d.inv.items[i].equip_slot;
            let Some(&(slot_word, _)) = d.slot_words.iter().rev().find(|&&(_, s)| s == slot) else { continue };
            let Some(v) = d.find_stat(i, stat) else { continue };
            // FindStatWord(s->m_id), and the found stat's id is `stat`
            let (text, c) = match STAT_WORDS.iter().rev().find(|&&(_, s)| s == stat) {
                Some(&(stat_word, _)) => {
                    let sign = if v >= 0 { "+" } else { "" };
                    (format!("{}: {sign}{v} {}", vui.words.get(slot_word, vui.language), vui.words.get(stat_word, vui.language)), if v >= 0 { blue } else { red })
                }
                None => {
                    let sign = if v >= 0 { "+" } else { "" };
                    (format!("{slot}: {sign}{v} {stat}"), BLACK)
                }
            };
            let text = wide(&text);
            let mut rows = self.list.count_lines(vui, r, &text, width - 50);
            if lines < 10 {
                self.last = i as i32;
                if 10 - lines < rows {
                    rows = 10 - lines;
                    self.last = i as i32 - 1;
                }
                self.list.text_box(vui, r, &text, 20, lines, width - 50, rows, c, 2);
            }
            lines += rows;
        }
    }
}

/// `TrainStats(points)`: 0x001D with the seven counts (none sent when they
/// add to nothing or past the points).
pub fn train_stats(points: &[i32; 7], p40: i32) -> Option<Vec<u8>> {
    let total: i32 = points.iter().sum();
    if total == 0 || p40 < total {
        return None;
    }
    let mut w = net::stream::Writer::op(0x001D);
    for &p in points {
        w.signed(p as i64);
    }
    Some(w.data)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Adjust`: a point off only what was added, onto a stat only while
    /// there are points and under the cap; `TrainStats` refuses nothing and
    /// more than the points.
    #[test]
    fn points_go_on_and_off_as_the_trainer_allows() {
        let base = [10; 29];
        let inv = ItemList::default();
        let d = Data { name: "X", class: 0, race: 0, level: 1, base: &base, cur: &base, p40: 2, p44: 11, mastery: &[0; 29], bonuses: &[0; 7], inv: &inv, slot_words: &[] };
        let mut t = Trainer::default();
        t.points = 2;
        let mut cues = Vec::new();
        t.adjust(&d, -1, &mut cues);
        assert_eq!((t.points, t.add[0], cues.pop()), (2, 0, Some(9)), "nothing to take off");
        t.adjust(&d, 1, &mut cues);
        assert_eq!((t.points, t.add[0], cues.pop()), (1, 1, Some(11)));
        t.adjust(&d, 1, &mut cues);
        assert_eq!((t.points, t.add[0], cues.pop()), (1, 1, Some(9)), "the cap (m_p44 11)");
        t.sel = 1;
        t.adjust(&d, 1, &mut cues);
        t.adjust(&d, 1, &mut cues);
        assert_eq!((t.points, t.add[1], cues.pop()), (0, 1, Some(9)), "out of points");
        assert_eq!(train_stats(&[0; 7], 2), None);
        assert_eq!(train_stats(&[1, 2, 0, 0, 0, 0, 0], 2), None, "3 points asked, 2 there");
        assert_eq!(train_stats(&[1, 1, 0, 0, 0, 0, 0], 2), Some(vec![0x1D, 0, 2, 2, 0, 0, 0, 0, 0]));
        assert!(d.highlighted(0) && d.highlighted(1) && !d.highlighted(2), "WAR's mask 3");
        assert_eq!(Data::derived(&base, 20), (10 / 7 + 10 + 40).min(315));
    }

    /// A pointer as the keys: another stat is the steps to it, the selected
    /// stat's value left and right, the bars the steps down past the last
    /// stat and ✕ on the selected one; from a bar up to the stats first; the
    /// details' rows; nothing while the points are out. The steps land where
    /// they were meant to.
    #[test]
    fn a_pointer_steps_as_the_keys() {
        use crate::pointer::testing::{NullRaster, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let base = [10; 29];
        let inv = ItemList::default();
        let d = Data { name: "X", class: 0, race: 0, level: 1, base: &base, cur: &base, p40: 2, p44: 11, mastery: &[0; 29], bonuses: &[0; 7], inv: &inv, slot_words: &[] };
        let mut t = Trainer::default();
        t.open(&vui, &r, &d, 0);
        let mut cues = Vec::new();
        let mut press = |t: &mut Trainer, keys: Vec<i32>| {
            for k in keys {
                t.key(&d, k, true, 0, &mut cues);
                t.key(&d, k, false, 0, &mut cues);
            }
        };
        t.draw(&vui, &mut r, &d, 0, &mut Vec::new());
        // the stats' rows from y 65, 25 apart
        let stat = |i: i32, x: f32| [x, (65 + 25 * i + 12) as f32];
        assert_eq!(t.point(stat(3, 100.0)), vec![key::DOWN; 3]);
        assert_eq!(t.point(stat(0, 240.0)), vec![key::LEFT], "the selected stat's value, left half");
        assert_eq!(t.point(stat(0, 290.0)), vec![key::RIGHT], "right half");
        assert!(t.point(stat(0, 100.0)).is_empty(), "its caption");
        assert_eq!(t.point(stat(3, 240.0)), vec![key::DOWN; 3], "another stat's value is that stat");
        assert!(t.point([100.0, 20.0]).is_empty(), "the header");
        // Train past the seven stats, Details one more
        let (train, details) = ([100.0, 262.0], [100.0, 287.0]);
        assert_eq!(t.point(details), vec![key::DOWN; 8]);
        let keys = t.point(train);
        assert_eq!(keys, vec![key::DOWN; 7]);
        press(&mut t, keys);
        assert_eq!((t.mode, t.sel), (1, 6), "the steps landed on Train");
        t.draw(&vui, &mut r, &d, 0, &mut Vec::new());
        assert_eq!(t.point(train), vec![key::CROSS]);
        assert_eq!(t.point(details), vec![key::DOWN]);
        let keys = t.point(stat(4, 100.0));
        assert_eq!(keys, vec![key::UP, key::UP, key::UP]);
        press(&mut t, keys);
        assert_eq!((t.mode, t.sel), (0, 4), "up to the stats, then to the fifth");
        // Details, then its rows
        let keys = t.point(details);
        press(&mut t, keys);
        assert_eq!(t.mode, 2);
        t.draw(&vui, &mut r, &d, 0, &mut Vec::new());
        assert_eq!(t.point(train), vec![key::UP], "Train from Details");
        t.key(&d, key::CROSS, true, 0, &mut cues);
        assert_eq!(t.mode, 3);
        t.draw(&vui, &mut r, &d, 0, &mut Vec::new());
        assert_eq!(t.point([100.0, (40 + 25 * 5 + 12) as f32]), vec![key::DOWN; 5]);
        assert!(t.point(details).is_empty(), "no bars on the details");
        assert!(t.point([100.0, (40 + 12) as f32]).is_empty(), "the selected detail");
        // waiting for the points: nothing
        t.mode = 4;
        t.draw(&vui, &mut r, &d, 0, &mut Vec::new());
        assert!(t.point(stat(3, 100.0)).is_empty());
        assert!(t.point(train).is_empty());
    }
}
