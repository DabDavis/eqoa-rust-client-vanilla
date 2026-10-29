//! `VIWndCharacterSelect` (CLIENT `viwndcharacterselect.cpp`): eight slots
//! and Delete Character; the highlighted character stands in its race's set
//! of the arena (`ShowCharacter`), the camera turning round it
//! (`ApplyCamera`), with a flourish every five seconds; Enter plays it (with
//! Frontiers, first the Options box: Enter Game World or Customize
//! Character), creates one in an empty slot, or deletes one after the delete
//! slot, asked first (`VIWndMBox`). Exit goes back to the server list.
//! Customise is `cust.rs`, a child of this window as in the game.

use super::cust::Cust;
use super::races::{get_class, get_race};
use super::{CharRecord, Req, UiCx};
use crate::dialog::{BoxAnswer, Key as BoxKey, MBox};
use crate::pointer::{Hits, key};
use crate::pregame::figure::Figure;
use crate::pregame::stage::{Stage, VIEWPORT};
use crate::pregame::{Music, Scene};
use vi::fpu::{Ee, F};
use vi::ui::{FrameParam, UI_COLORS, Window, wide};

/// The view's studded frame (`OnDrawSelf`, at 347): its top and height,
/// which the list's panel and rails at its left span too; the race page
/// (`create/race.rs`) is laid out the same.
pub const PANEL: [i32; 2] = [37, 377];

/// `OnDrawSelf`'s dark plate over the view when no character is highlighted
/// (an empty slot, Delete Character): atlas 91 tiled over the viewport the
/// figure is drawn in (`stage::VIEWPORT`, inclusive: 263 by 349). The game
/// blits it the viewport's width at (320, 47), 38 to the left of the
/// viewport: it covered the list's right end and the view frame's left post
/// and left the view's last 36 columns showing the arena, or with no figure
/// the backdrop (F3 20261023-142644, Bird's; the PS2's own picture, put here
/// where its size says it belongs).
pub const PLATE: ([i32; 2], [i32; 2]) = ([VIEWPORT[0], VIEWPORT[1]], [VIEWPORT[2] - VIEWPORT[0] + 1, VIEWPORT[3] - VIEWPORT[1] + 1]);

/// `GetFrameRate()` (name the decomp's guess): the level cap, 60 with
/// Frontiers' expansion bit, else 50. A character at it stands in the
/// capped hall (`ShowCharacter`).
pub fn level_cap(expansion: u32) -> i32 {
    if expansion & 2 != 0 { 60 } else { 50 }
}

/// `ShowCharacter`'s place: the race's select place, or at the level cap a
/// hall of the human row, the right one for the races and classes of the
/// dark (dark elf, troll, ogre; shadowknight, rogue, necromancer).
pub fn select_place(race: i32, class: i32, level: i32, expansion: u32) -> [f32; 3] {
    if level >= level_cap(expansion) {
        let x = if matches!(race, 2 | 5 | 9) || matches!(class, 3 | 6 | 11) { 13.0 } else { -13.0 };
        [x, 0.0, -26.0]
    } else {
        let rd = get_race(race);
        [rd.select[0], 0.0, rd.select[1]]
    }
}

/// `ApplyCamera` / `UpdateCamera`'s framing: target height, look-at height
/// above it, the default orbit pitch and distance; with no race camera
/// `ApplyCamera`'s own numbers.
pub fn framing(rc: Option<crate::camstyle::RaceCamera>, review: bool) -> [f32; 4] {
    match rc {
        Some(c) if review => c.review,
        Some(c) => c.cust,
        None => [1.1, 1.7, 0.698_131_74, 0.75],
    }
}

/// `VICSprite::SetAttackAction(slot)`'s action for the items' attack
/// types: the two-handed kinds (2, 4, 6) in the first slot decide whatever
/// the slot; None where the game returns -1.
pub fn attack_action(attack: [u8; 3], slot: usize) -> Option<i32> {
    if slot >= 2 {
        return None;
    }
    let first = (slot == 0) as i32;
    match attack[0] {
        2 => Some(26),
        4 => Some(42),
        6 => Some(29),
        _ => match attack[slot] {
            11 => Some(if first == 1 { 23 } else { 22 }),
            1 => Some(if first == 1 { 25 } else { 24 }),
            3 => Some(if first == 1 { 41 } else { 40 }),
            5 => Some(if first == 1 { 28 } else { 27 }),
            7 => Some(31),
            9 => Some(33),
            8 => Some(if first == 1 { 35 } else { 32 }),
            10 => Some(36),
            _ => None,
        },
    }
}

/// The look `ShowCharacter` dresses a record in (`helm`: the sixth slot
/// worn, as select shows it; customise leaves it off).
pub fn look_of(c: &CharRecord, helm: bool) -> vi::cust::Look {
    let mut armor = c.armor;
    if !helm {
        armor[5] = 0;
    }
    vi::cust::Look {
        armor,
        armor_tint: c.tints,
        hair: [c.hair_color as u8, c.hair_length as u8, c.hair_style as u8],
        face: c.face as u8,
        robe: if c.robe == -1 { -1 } else { c.robe as i8 },
        robe_tint: c.robe_tint,
        // the live game's record's colours (`ShowCharacter`'s SetArmorSlot
        // and SetRobe take them), when the server sent its look
        armor_color: c.colors.map(|l| l.armor),
        robe_color: c.colors.map(|l| l.robe),
    }
}

/// The child that has the keys over the list.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Child {
    MBox,
    Cust,
}

#[derive(Default)]
pub struct CharSelect {
    pub sel: i32,
    /// `m_showTime`: when the flourish is due
    show_time: i64,
    deleting: bool,
    busy: bool,
    /// `m_p5EC`: a character being made is in the list
    making: bool,
    choosing: bool,
    choice: i32,
    last_race: i32,
    attack_slot: i32,
    /// the busy and Options box
    window: Window,
    mbox: MBox,
    child: Option<Child>,
    pub cust: Cust,
    /// `m_actor`: the character shown, and its slot
    figure: Option<Figure>,
    /// whether the 3D was drawn this frame
    shown: bool,
    /// "..." after a pending line (`sDots`, `sTicks`)
    dots: i32,
    ticks: i32,
    hits: Hits,
}

/// A slot's place on the screen, for the pointer: the bars are 320 wide
/// and 30 apart from (15, 60).
fn slot_rect(i: i32) -> [i32; 4] {
    [15, 60 + 30 * i - 2, 15 + 320, 60 + 30 * i + 26]
}

const HIT_SELECT: i32 = 100;
const HIT_EXIT: i32 = 101;
const HIT_CHOICE: i32 = 200;

impl CharSelect {
    /// `Connecting` (`ConnectShard` calls it): the state from the top, the
    /// character let go.
    pub fn connecting(&mut self) {
        self.show_time = -1;
        self.attack_slot = -1;
        self.last_race = -1;
        self.deleting = false;
        self.sel = 0;
        self.busy = false;
        self.making = false;
        self.choice = 0;
        self.choosing = false;
        self.figure = None;
    }

    /// `OnMessage` 295, the focus arriving from another window (not the
    /// box, customise or itself): the flourish five seconds on, the
    /// character let go, the busy box made, no choice up; the box and
    /// customise hidden.
    pub fn focus(&mut self, cx: &mut UiCx) {
        self.show_time = cx.now + 5000;
        if self.figure.take().is_some() {
            self.last_race = -1;
        }
        self.window.close(true);
        self.window.init(cx.vui, cx.r, [320, 224], 330, 3, 0);
        self.window.set_title(Some(&wide(cx.vui.words.get(271, cx.vui.language))));
        self.choosing = false;
        self.choice = 0;
        self.child = None;
    }

    /// The art swapped (F9): the character shown, and customise's, made again
    /// in it where they stand.
    pub fn remake(&mut self, lib: &esf::Library, g: &mut crate::play::Gpu, now: i64) {
        if let Some(f) = self.figure.as_mut() {
            f.remake(lib, g, now);
        }
        self.cust.remake(lib, g, now);
    }

    /// The focus going to another window (not the box or customise): the
    /// character let go.
    pub fn lost_focus(&mut self) {
        self.figure = None;
        self.cust.release();
        self.child = None;
    }

    /// `ShowText(s)`: the pending states end, and a text up in the box.
    pub fn show_text(&mut self, s: &[u16], now: i64) {
        self.busy = false;
        self.making = false;
        self.choosing = false;
        if !s.is_empty() {
            self.show_message(s, 0, 0, 0, now);
        }
    }

    /// For the client's tests: the busy box, the Options box, the message
    /// box (whether each is up), and setting the first two.
    #[cfg(test)]
    pub(super) fn pending(&self) -> (bool, bool, bool) {
        (self.busy, self.choosing, self.child == Some(Child::MBox))
    }

    #[cfg(test)]
    pub(super) fn set_pending(&mut self) {
        self.busy = true;
        self.choosing = true;
    }

    /// None of its boxes up (busy, Options, a message, deleting):
    /// `Client::quiet`.
    pub fn quiet(&self) -> bool {
        !self.busy && !self.choosing && !self.deleting && self.child.is_none()
    }

    /// `ShowWord(id)`.
    pub fn show_word(&mut self, vui: &vi::ui::Vui, id: i32) {
        let s = wide(vui.words.get(id, vui.language));
        let now = vui.time;
        self.show_text(&s, now);
    }

    /// `ShowWord(413)`, which is empty: the pending states end.
    pub fn show_text_none(&mut self) {
        self.show_text(&[], 0);
    }

    /// `ShowMessage(s, type, p1, p2)`: the box up with the keys.
    fn show_message(&mut self, s: &[u16], kind: i32, p1: i32, p2: i32, now: i64) {
        self.mbox.prompt(s, kind, p1, p2, now);
        self.child = Some(Child::MBox);
    }

    /// `OnDrawSelf`, then its children (the box, customise).
    pub fn draw(&mut self, cx: &mut UiCx, chars: &[CharRecord], stage: Option<&mut Stage>, reqs: &mut Vec<Req>) {
        self.ticks -= 1;
        if self.ticks <= 0 {
            self.ticks = 15;
            self.dots += 1;
            if self.dots == 3 {
                self.dots = 0;
            }
        }
        let count = chars.len() as i32;
        let (vui, r) = (cx.vui, &mut *cx.r);
        let white = [1.0, 1.0, 1.0, 1.0];
        let black = [0.0, 0.0, 0.0, 1.0];
        let grey = [0.8, 0.8, 0.8, 1.0];
        let yellow = [1.0, 1.0, 0.0, 1.0];
        let word = |id: i32| wide(vui.words.get(id, vui.language));
        vui.blit_rect(r, 32, [-1, -1], [642, 450], 0, 1.0);
        let title = if self.deleting { 262 } else { 263 };
        let w = vui.string_width(r, &word(title)) + 64;
        vui.blit_bar(r, &word(title), white, [(640 - w) / 2, 8], 33, 34, w, 0, 1.0, 0, true);
        vui.blit_rect(r, 51, [-1, 48], [348, 355], 0, 1.0);
        vui.blit_studded_horz(r, [-1, 37], 37, 38, 348, 3, 0, 1.0, 0);
        vui.blit_studded_horz(r, [-1, 396], 40, 41, 348, 3, 0, 1.0, 0);
        let frame = FrameParam { tl: (36, 0), top: (37, 0), tr: (36, 1), right: (42, 1), br: (39, 1), bottom: (40, 0), bl: (39, 0), left: (42, 0) };
        let studs = FrameParam { tl: (0, 0), top: (38, 0), tr: (0, 0), right: (43, 1), br: (0, 0), bottom: (41, 0), bl: (0, 0), left: (43, 0) };
        vui.blit_studded_frame(r, [347, 37], [284, 377], &frame, &studs, 2, 3, 0, 1.0, true);
        // the client's own, in a window wider than 4:3: the list's panel and
        // its rails run off a TV's left side (from x -1), so there they end
        // cut; a post closes that edge, as the view's frame closes the right
        crate::startmenu::close_edges(vui, r, PANEL, false, [true, false]);
        self.hits.clear();
        let mut pen = [15, 60];
        self.making = false;
        let bar = |sel: bool| if sel { (56, 58, 57) } else { (53, 55, 54) };
        for (slot, c) in chars.iter().enumerate() {
            let (l, m, rr) = bar(slot as i32 == self.sel);
            let colour = if slot as i32 == self.sel { yellow } else { white };
            if c.server_id == -1 {
                self.making = true;
                let mut t = word(878);
                t.extend(std::iter::repeat_n(b'.' as u16, (self.dots + 1) as usize));
                vui.blit_bar3(r, &t, colour, pen, l, m, rr, 320, 0, 1.0, 1, true);
            } else {
                vui.blit_bar3(r, &wide(&c.name), colour, pen, l, m, rr, 320, 0, 1.0, 1, true);
            }
            self.hits.push(slot_rect(slot as i32), slot as i32);
            pen[1] += 30;
        }
        if !self.deleting {
            for slot in count..8 {
                let (l, m, rr) = bar(slot == self.sel);
                vui.blit_bar3(r, &word(219), if slot == self.sel { yellow } else { grey }, pen, l, m, rr, 320, 0, 1.0, 1, true);
                self.hits.push(slot_rect(slot), slot);
                pen[1] += 30;
            }
            let hi = self.sel == 8;
            let (l, m, rr) = bar(hi);
            vui.blit_bar3(r, &word(220), if hi { yellow } else { black }, pen, l, m, rr, 320, 0, 1.0, 1, hi);
            self.hits.push(slot_rect(8), 8);
        }
        let frame = FrameParam { tl: (44, 0), top: (45, 0), tr: (46, 0), right: (52, 0), br: (50, 0), bottom: (49, 0), bl: (48, 0), left: (47, 0) };
        vui.blit_fill_frame(r, [15, 358], [320, 35], &frame, 35, 0, 1.0, true);
        // the legend: ✕ Select, △ Exit, in black
        let tex = vui.entry(65);
        let mut p = [45, 368];
        let mut at = [70, tex.uv1[1] / 2 + 365];
        vui.blit_ex(r, 66, p, 0, 1.0, 0);
        r.draw_centered_string(at, black, &word(212), 1);
        let step = vui.string_width(r, &word(212)) + tex.uv1[0] + 30;
        self.hits.push([p[0], 360, p[0] + step - 10, 392], HIT_SELECT);
        p[0] += step;
        at[0] += step;
        vui.blit_ex(r, 65, p, 0, 1.0, 0);
        r.draw_centered_string(at, black, &word(161), 1);
        self.hits.push([p[0], 360, at[0] + vui.string_width(r, &word(161)) + 4, 392], HIT_EXIT);
        let in_cust = self.child == Some(Child::Cust);
        // `ShowCharacter`, the flourish, `ApplyCamera`, and the scene into its viewport
        let mut stage = stage;
        if self.figure.is_none()
            && self.sel < count
            && !in_cust
            && let (Some(g), Some(lib), Some(st)) = (cx.gpu.as_mut(), cx.lib, stage.as_deref_mut())
        {
            let c = &chars[self.sel as usize];
            self.show_character(c, lib, g, st, cx.expansion, cx.now, &mut cx.music);
        }
        if let Some(f) = self.figure.as_mut() {
            if self.show_time < cx.now {
                match (self.attack_slot >= 0).then(|| attack_action(chars.get(self.sel as usize).map_or([0; 3], |c| c.attack), self.attack_slot as usize)).flatten() {
                    Some(id) => f.set_action(id),
                    None if self.attack_slot >= 0 => {}
                    None => f.set_action(23),
                }
                self.show_time = cx.now + 5000;
            } else {
                f.set_action(0);
            }
        }
        self.shown = false;
        if self.figure.is_some()
            && !in_cust
            && let Some(st) = stage.as_deref_mut()
        {
            let rc = chars.get(self.sel as usize).and_then(|c| cx.race_cams.and_then(|t| crate::camstyle::race_camera(t, c.race, cx.expansion)));
            if self.sel < count {
                let [h, l, p, d] = framing(rc, true);
                let f = F::<Ee>::new;
                st.cam.target_height = f(h);
                st.cam.look_at_height_above = f(l);
                st.cam.set_def_orbit_pitch_angle(f(p));
                st.cam.set_orbit_distance(f(d));
            }
            st.cam.orbit_right(F::new(0.3));
            st.process(cx.now);
            if let (Some(f), Some(g)) = (self.figure.as_mut(), cx.gpu.as_ref()) {
                f.pose(g, cx.now);
            }
            cx.r.mark_split();
            self.shown = true;
        }
        let r = &mut *cx.r;
        if self.sel < count {
            let c = &chars[self.sel as usize];
            if c.level > 0 {
                let mut t = word(503);
                t.extend(wide(&format!(" {}", c.level as u32)));
                vui.draw_centered_string_ex(r, &t, 501, 350, white, 0, true);
                let fh = vui.font_height(r);
                vui.draw_centered_string_ex(r, &word(get_class(c.class).name), 501, fh + 346, white, 0, true);
            }
        } else {
            vui.blit_rect(r, 91, PLATE.0, PLATE.1, 0, 1.0);
            cx.music.push(Music::StopBgm { slot: 0 });
            self.last_race = -1;
        }
        if self.busy {
            self.window.init(vui, r, [320, 224], 330, 3, 0);
            self.window.set_title(Some(&word(271)));
            self.window.open(false);
            self.window.draw(vui, r);
            let mut t = word(879);
            t.extend(std::iter::repeat_n(b'.' as u16, (self.dots + 1) as usize));
            let w = self.window.size[0];
            self.window.text_box(vui, r, &t, 40, 1, w, 1, UI_COLORS[0], 0);
        } else if self.choosing {
            self.window.init(vui, r, [320, 224], 330, 5, 0);
            self.window.set_title(Some(&word(422)));
            self.window.open(false);
            self.window.draw(vui, r);
            let w = self.window.size[0];
            self.window.text_box(vui, r, &word(1778), 40, 1, w - 60, 1, UI_COLORS[(self.choice == 0) as usize], 0);
            if self.choice == 0 {
                self.window.tick_r(vui, r, 40, 1, true);
            }
            self.window.text_box(vui, r, &word(259), 40, 2, w - 60, 1, UI_COLORS[(self.choice == 1) as usize], 0);
            if self.choice == 1 {
                self.window.tick_r(vui, r, 40, 2, true);
            }
            let mut t = wide("|x");
            t.extend(word(212));
            t.extend(wide(" |t"));
            t.extend(word(161));
            self.window.text_box(vui, r, &t, 20, 4, w - 40, 1, UI_COLORS[0], 0);
            for (row, k) in [(1, 0), (2, 1)] {
                self.hits.push(crate::pointer::rows_rect(&self.window, vui, r, 40, row, w - 60, 1), HIT_CHOICE + k);
            }
        } else {
            self.window.close(true);
        }
        // the children: the box, customise
        match self.child {
            Some(Child::MBox) => {
                if let Some(a) = self.mbox.draw(vui, r, cx.now) {
                    self.answer(a, reqs);
                }
            }
            Some(Child::Cust) => {
                if let Some(out) = self.cust.draw(cx, chars, stage) {
                    self.cust_done(out, reqs);
                }
            }
            None => {}
        }
    }

    /// `ShowCharacter`: the record's model at its place, dressed, its items
    /// in hand; the camera aimed at it; the race's music when the race is
    /// another.
    #[allow(clippy::too_many_arguments)]
    fn show_character(&mut self, c: &CharRecord, lib: &esf::Library, g: &mut crate::play::Gpu, st: &mut Stage, expansion: u32, now: i64, music: &mut Vec<Music>) {
        let at = select_place(c.race, c.class, c.level, expansion);
        let hpr = st.cam.walk.committed.angles.to_array();
        self.figure = Figure::new(lib, g, c.model, look_of(c, true), c.weapons, at, hpr, now);
        self.show_time = now + 5000;
        self.attack_slot = -1;
        if let Some(f) = self.figure.as_mut() {
            for i in 0..2 {
                if c.weapons[i] != 0 && c.attack[i] != 0 {
                    if let Some(id) = attack_action(c.attack, i) {
                        f.set_action(id);
                    }
                    self.attack_slot = i as i32;
                }
            }
        }
        st.aim(at, hpr);
        if c.race != self.last_race {
            music.push(Music::Bgm { slot: 0, path: get_race(c.race).bgm.to_string() });
            self.last_race = c.race;
        }
    }

    /// The box answered: 268 (yes, OK) or 260 (no).
    fn answer(&mut self, a: BoxAnswer, reqs: &mut Vec<Req>) {
        self.child = None;
        match a {
            BoxAnswer::Yes(p1, _) => {
                if self.deleting && p1 != 0 {
                    reqs.push(Req::Delete(self.sel as usize));
                    self.deleting = false;
                    self.figure = None;
                }
            }
            BoxAnswer::No(..) => self.deleting = false,
        }
    }

    /// Customise closed (260): the Options box again; Accept kept the look.
    fn cust_done(&mut self, look: Option<(usize, [i32; 4])>, reqs: &mut Vec<Req>) {
        if let Some((i, l)) = look {
            reqs.push(Req::Look(i, l));
        }
        self.choice = 0;
        self.choosing = true;
        self.child = None;
        self.cust.release();
    }

    /// `OnKeyDown`: to the box or customise while one has the keys.
    #[allow(clippy::too_many_arguments)]
    pub fn key(&mut self, k: i32, down: bool, now: i64, chars: &[CharRecord], expansion: u32, cues: &mut Vec<i32>, reqs: &mut Vec<Req>) {
        if !down {
            return;
        }
        match self.child {
            Some(Child::MBox) => {
                let bk = match k {
                    13 | 134 => Some(BoxKey::Enter),
                    27 | 132 => Some(BoxKey::Escape),
                    17 | 136 => Some(BoxKey::Left),
                    18 | 137 => Some(BoxKey::Right),
                    _ => None,
                };
                if let Some(bk) = bk
                    && let Some(a) = self.mbox.key(bk, cues)
                {
                    self.answer(a, reqs);
                }
                return;
            }
            Some(Child::Cust) => {
                if let Some(out) = self.cust.key(k, now, cues) {
                    self.cust_done(out, reqs);
                }
                return;
            }
            None => {}
        }
        match k {
            13 | 134 => self.option(false, now, chars, expansion, cues, reqs),
            27 | 132 => {
                if self.busy || self.making {
                    cues.push(10);
                } else {
                    cues.push(8);
                    if self.deleting {
                        self.deleting = false;
                    } else if self.choosing {
                        self.choosing = false;
                    } else {
                        self.sel = 0;
                        reqs.push(Req::Msg0777);
                    }
                }
            }
            19 | 138 => {
                if self.choosing {
                    self.window.reset_tick_pulse(now);
                    if self.choice > 0 {
                        self.choice -= 1;
                        cues.push(11);
                    } else {
                        self.choice = 0;
                        cues.push(9);
                    }
                } else {
                    self.move_sel(-1, chars.len() as i32, cues);
                }
            }
            20 | 139 => {
                if self.choosing {
                    self.window.reset_tick_pulse(now);
                    if self.choice > 0 {
                        self.choice = 1;
                        cues.push(9);
                    } else {
                        self.choice += 1;
                        cues.push(11);
                    }
                } else {
                    self.move_sel(1, chars.len() as i32, cues);
                }
            }
            _ => {}
        }
    }

    /// `MoveSel(d)`: the delete slot (8) only with no delete pending.
    fn move_sel(&mut self, d: i32, count: i32, cues: &mut Vec<i32>) {
        if self.busy || self.making {
            cues.push(10);
            return;
        }
        self.sel += d;
        if self.sel < 0 {
            cues.push(9);
            self.sel = 0;
        } else if self.deleting && self.sel >= count {
            cues.push(9);
            self.sel = count - 1;
        } else if !self.deleting && self.sel >= 8 {
            cues.push(if self.sel > 8 { 9 } else { 11 });
            self.sel = 8;
        } else {
            cues.push(11);
            self.figure = None;
        }
    }

    /// `Option(on)`: Enter on a slot.
    fn option(&mut self, on: bool, now: i64, chars: &[CharRecord], expansion: u32, cues: &mut Vec<i32>, reqs: &mut Vec<Req>) {
        let count = chars.len() as i32;
        if self.busy || self.making {
            cues.push(10);
            return;
        }
        cues.push(13);
        if self.deleting {
            let Some(c) = chars.get(self.sel as usize).filter(|c| c.server_id != -1) else { return };
            let text = self.delete_question(c);
            self.show_message(&text, 1, 1, 0, now);
        } else if self.choosing {
            if self.choice == 0 {
                self.window.close(true);
                self.choosing = false;
                self.busy = true;
                reqs.push(Req::Select(self.sel as usize));
            } else if self.choice == 1 {
                self.child = Some(Child::Cust);
                self.cust.set_char(self.sel as usize);
                self.figure = None;
            }
        } else if self.sel == 8 {
            if count > 0 {
                self.deleting = true;
                self.sel = 0;
                self.figure = None;
            }
        } else if self.sel >= count && self.sel < 8 {
            reqs.push(Req::Select(self.sel as usize));
            self.figure = None;
        } else if chars.get(self.sel as usize).is_some_and(|c| c.server_id != -1) {
            if !on && expansion & 2 != 0 {
                self.choice = 0;
                self.choosing = true;
            } else {
                self.busy = true;
                reqs.push(Req::Select(self.sel as usize));
            }
        }
    }

    /// Word 880 with the character's name (`"%s"`, `GetSel()`).
    fn delete_question(&self, c: &CharRecord) -> Vec<u16> {
        let q = DELETE_QUESTION.with(|q| q.borrow().clone());
        let s = q.replacen("%s", &c.name, 1);
        wide(&s)
    }

    /// A pointer at frame point `p`: a slot moves the selection there (the
    /// selected one presses ✕); the Options box's rows; the legend's glyphs;
    /// the box's buttons; customise's rows.
    pub fn point(&mut self, p: [f32; 2], count: usize) -> Vec<i32> {
        match self.child {
            Some(Child::MBox) => return if self.mbox.point(p) { vec![key::CROSS] } else { Vec::new() },
            Some(Child::Cust) => return self.cust.point(p),
            None => {}
        }
        let Some(h) = self.hits.at(p) else { return Vec::new() };
        match h {
            HIT_SELECT => vec![key::CROSS],
            HIT_EXIT => vec![key::TRIANGLE],
            _ if h >= HIT_CHOICE => {
                let k = h - HIT_CHOICE;
                if k == self.choice { vec![key::CROSS] } else { crate::pointer::steps(self.choice, k, key::UP, key::DOWN) }
            }
            _ if self.choosing || self.busy => Vec::new(),
            _ if h == self.sel => vec![key::CROSS],
            _ => {
                let _ = count;
                crate::pointer::steps(self.sel, h, key::UP, key::DOWN)
            }
        }
    }

    /// The 3D this frame: the arena and the character (or customise's).
    pub fn scene<'a>(&'a self, st: &'a Stage) -> Option<Scene<'a>> {
        if self.child == Some(Child::Cust) {
            return self.cust.scene(st);
        }
        let f = self.figure.as_ref().filter(|_| self.shown)?;
        Some(Scene { view: st.view(), viewport: VIEWPORT, models: st.models().chain(f.models()).collect() })
    }
}

thread_local! {
    /// Word 880 as read at the box's last setup (a test sets its own).
    pub static DELETE_QUESTION: std::cell::RefCell<String> = std::cell::RefCell::new("Permanently delete character \"%s\"?".to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(name: &str, id: i64) -> CharRecord {
        CharRecord { name: name.into(), server_id: id, level: 1, ..Default::default() }
    }

    /// `ShowCharacter`'s place: the race's, or the capped hall for the
    /// dark races and classes at the cap (60 with the expansion, else 50).
    #[test]
    fn the_place_is_the_races_or_the_capped_halls() {
        assert_eq!(select_place(0, 0, 1, 3), [0.0, 0.0, -26.0]);
        assert_eq!(select_place(2, 11, 1, 3), [-13.0, 0.0, 0.0]);
        assert_eq!(select_place(4, 9, 1, 3), [-13.0, 0.0, -13.0]);
        assert_eq!(select_place(2, 0, 60, 3), [13.0, 0.0, -26.0], "a dark race at the cap");
        assert_eq!(select_place(0, 6, 60, 3), [13.0, 0.0, -26.0], "a rogue at the cap");
        assert_eq!(select_place(0, 0, 60, 3), [-13.0, 0.0, -26.0], "the light hall");
        assert_eq!(select_place(0, 0, 55, 3), [0.0, 0.0, -26.0], "55 is under the expansion's cap");
        assert_eq!(select_place(0, 0, 55, 1), [-13.0, 0.0, -26.0], "and over the original's 50");
        // with no race camera, ApplyCamera's own numbers
        assert_eq!(framing(None, true), [1.1, 1.7, 0.698_131_74, 0.75]);
    }

    /// `SetAttackAction`: a two-handed kind in the first slot decides; a
    /// one-handed first slot 23, second 22; nothing for an empty type.
    #[test]
    fn the_attack_actions() {
        assert_eq!(attack_action([2, 11, 0], 1), Some(26));
        assert_eq!(attack_action([11, 0, 0], 0), Some(23));
        assert_eq!(attack_action([0, 11, 0], 1), Some(22));
        assert_eq!(attack_action([5, 5, 0], 1), Some(27));
        assert_eq!(attack_action([0, 0, 0], 0), None);
        assert_eq!(attack_action([11, 11, 0], 2), None, "only the two hands");
    }

    /// The slots by key: down to the delete slot (a buzz past it), up to
    /// the top (a buzz past it); Enter on the delete slot starts deleting
    /// at the top; Enter there asks; yes sends the delete; Exit goes back
    /// to the list.
    #[test]
    fn the_keys_move_delete_and_exit() {
        let chars = vec![rec("A", 1), rec("B", 2)];
        let mut s = CharSelect::default();
        let (mut cues, mut reqs) = (Vec::new(), Vec::new());
        for _ in 0..9 {
            s.key(139, true, 0, &chars, 3, &mut cues, &mut reqs);
        }
        assert_eq!(s.sel, 8);
        assert_eq!(&cues[6..], &[11, 11, 9], "into the delete slot, then a buzz");
        s.key(134, true, 0, &chars, 3, &mut cues, &mut reqs);
        assert!(s.deleting && s.sel == 0);
        s.key(139, true, 0, &chars, 3, &mut cues, &mut reqs);
        s.key(139, true, 0, &chars, 3, &mut cues, &mut reqs);
        assert_eq!(s.sel, 1, "deleting: no further than the characters");
        s.key(134, true, 0, &chars, 3, &mut cues, &mut reqs);
        assert_eq!(s.child, Some(Child::MBox));
        // the box starts on No: Enter there cancels
        s.key(134, true, 0, &chars, 3, &mut cues, &mut reqs);
        assert!(reqs.is_empty() && !s.deleting && s.child.is_none());
        // again: the delete slot, down to B, asked, Left to Yes, Enter: B deleted
        s.sel = 8;
        s.key(134, true, 0, &chars, 3, &mut cues, &mut reqs);
        s.key(139, true, 0, &chars, 3, &mut cues, &mut reqs);
        s.key(134, true, 0, &chars, 3, &mut cues, &mut reqs);
        s.key(136, true, 0, &chars, 3, &mut cues, &mut reqs);
        s.key(134, true, 0, &chars, 3, &mut cues, &mut reqs);
        assert_eq!(reqs, vec![Req::Delete(1)]);
        assert!(!s.deleting && s.child.is_none());
        reqs.clear();
        s.key(132, true, 0, &chars, 3, &mut cues, &mut reqs);
        assert_eq!(reqs, vec![Req::Msg0777], "Exit: back to the list");
    }

    /// Enter on a character: with Frontiers' bit the Options box first
    /// (Enter Game World plays it, busy), without it played at once; on an
    /// empty slot, creation; nothing while busy (a buzz).
    #[test]
    fn enter_plays_after_the_options_box() {
        let chars = vec![rec("A", 1)];
        let (mut cues, mut reqs) = (Vec::new(), Vec::new());
        let mut s = CharSelect::default();
        s.key(134, true, 0, &chars, 3, &mut cues, &mut reqs);
        assert!(s.choosing && reqs.is_empty());
        s.key(134, true, 0, &chars, 3, &mut cues, &mut reqs);
        assert_eq!(reqs, vec![Req::Select(0)]);
        assert!(s.busy);
        cues.clear();
        s.key(134, true, 0, &chars, 3, &mut cues, &mut reqs);
        assert_eq!(cues, vec![10], "busy: a buzz");
        let mut t = CharSelect::default();
        reqs.clear();
        t.key(134, true, 0, &chars, 1, &mut cues, &mut reqs);
        assert_eq!(reqs, vec![Req::Select(0)], "the control: no expansion bit, no box");
        let mut u = CharSelect::default();
        reqs.clear();
        u.sel = 3;
        u.key(134, true, 0, &chars, 3, &mut cues, &mut reqs);
        assert_eq!(reqs, vec![Req::Select(3)], "an empty slot: creation");
        // Customize from the box: customise up with the keys
        let mut v = CharSelect::default();
        reqs.clear();
        v.key(134, true, 0, &chars, 3, &mut cues, &mut reqs);
        v.key(139, true, 0, &chars, 3, &mut cues, &mut reqs);
        v.key(134, true, 0, &chars, 3, &mut cues, &mut reqs);
        assert_eq!(v.child, Some(Child::Cust));
    }

    /// A pointer on the slots: a slot moves the selection by its keys, the
    /// selected one presses ✕, the legend's glyphs their buttons; the
    /// control: no notes, nothing.
    #[test]
    fn a_pointer_picks_a_slot() {
        let chars = vec![rec("A", 1), rec("B", 2)];
        let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
        let mut s = CharSelect::default();
        let mut cx = UiCx::bare(&vui, &mut r);
        let mut reqs = Vec::new();
        s.draw(&mut cx, &chars, None, &mut reqs);
        let mid = |i: i32| {
            let r = slot_rect(i);
            [(r[0] + r[2]) as f32 / 2.0, (r[1] + r[3]) as f32 / 2.0]
        };
        assert_eq!(s.point(mid(3), 2), vec![key::DOWN, key::DOWN, key::DOWN]);
        assert_eq!(s.point(mid(0), 2), vec![key::CROSS]);
        assert_eq!(s.point([60.0, 375.0], 2), vec![key::CROSS], "the ✕ glyph");
        s.hits.clear();
        assert!(s.point(mid(3), 2).is_empty(), "the control");
    }

    /// The dots after a pending line: one more every 15 of the game's
    /// frames, so the same at any display rate (the windows draw on the
    /// game's frames: `Ctx::game_frame`).
    #[test]
    fn the_dots_step_on_the_games_frames() {
        for hz in [30.0, 165.0] {
            let mut f = crate::frames::Frames::default();
            let mut s = CharSelect::default();
            let (vui, mut r) = (crate::pointer::testing::vui(), crate::pointer::testing::NullRaster);
            let mut seen = Vec::new();
            for k in 0..(hz as usize * 2) {
                if f.due(k as f64 * 1000.0 / hz) {
                    let mut cx = UiCx::bare(&vui, &mut r);
                    s.draw(&mut cx, &[], None, &mut Vec::new());
                    seen.push(s.dots);
                }
            }
            // two seconds: 60 of the game's frames, the dots changing every 15
            assert!((59..=61).contains(&seen.len()), "{hz} Hz: {}", seen.len());
            let changes = seen.windows(2).filter(|w| w[0] != w[1]).count();
            assert_eq!(changes, 3, "{hz} Hz");
        }
    }

    /// The delete row's plate (atlas 91) covers the view's viewport, where
    /// the figure is drawn, and nothing else: not the list's right end nor
    /// the view frame's left post (the game's (320, 47) did), and to the
    /// viewport's right edge (the game's left the last 36 columns open). Its
    /// tiles are what the delete row draws beyond a highlighted character's
    /// row, in a 4:3 window and a wide one alike (the plate is in the frame's
    /// coordinates, as the viewport is).
    #[test]
    fn the_delete_rows_plate_covers_the_view() {
        let chars = vec![rec("A", 1)];
        let vui = crate::startmenu::posts::vui();
        for side in [0, 106] {
            let drawn = |sel: i32| {
                let mut r = crate::pointer::testing::Wide::new(side);
                let mut cx = UiCx::bare(&vui, &mut r);
                let mut cs = CharSelect { sel, ..Default::default() };
                cs.draw(&mut cx, &chars, None, &mut Vec::new());
                r.2
            };
            let (character, delete) = (drawn(0), drawn(8));
            let plate: Vec<_> = delete.iter().filter(|b| !character.contains(b)).copied().collect();
            assert!(!plate.is_empty(), "the delete row draws its plate");
            let lo = plate.iter().fold([i32::MAX; 2], |a, b| [a[0].min(b.1[0]), a[1].min(b.1[1])]);
            let hi = plate.iter().fold([i32::MIN; 2], |a, b| [a[0].max(b.1[0] + b.2[0]), a[1].max(b.1[1] + b.2[1])]);
            let [x0, y0, x1, y1] = VIEWPORT;
            assert_eq!((lo, hi), ([x0, y0], [x1 + 1, y1 + 1]), "side {side}: the plate is the viewport");
        }
    }

    /// In a window wider than 4:3 (F3 20261023-033942) the list's open left
    /// edge is closed by a post as tall as the view's frame, studded as that
    /// frame's sides are (`close_edges`); the right edge, the view's own
    /// frame, gets none. A 4:3 window draws no post (the control), and what
    /// else either draws is the same.
    #[test]
    fn a_wide_window_closes_the_lists_left_edge() {
        let chars = vec![rec("A", 1)];
        let vui = crate::startmenu::posts::vui();
        let drawn = |side: i32| {
            let mut r = crate::pointer::testing::Wide::new(side);
            let mut cx = UiCx::bare(&vui, &mut r);
            CharSelect::default().draw(&mut cx, &chars, None, &mut Vec::new());
            r.2
        };
        let (tv, wide) = (drawn(0), drawn(106));
        let added: Vec<_> = wide.iter().filter(|b| !tv.contains(b)).copied().collect();
        assert_eq!(added, crate::startmenu::posts::made_at(render::ui::Anchor::Middle, PANEL, false, [true, false]));
        let rest: Vec<_> = wide.iter().filter(|b| !added.contains(b)).copied().collect();
        assert_eq!(rest, tv);
    }
}
