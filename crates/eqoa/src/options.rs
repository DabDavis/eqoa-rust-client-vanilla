//! The options: `VIGameOptions` (the settings the Options window shows; the
//! login half of the record is not kept here) and `VIWndOptions`, the window
//! the start menu's Options opens: a menu of Game, Interface, Sound and Save
//! Changes, each page a list of rows whose value left and right change.
//! Camera, pitch, volumes, output, battle music and the soft keyboard change
//! as they are moved; Faction Messages, Loot Messages, Auto Attack and
//! Vibration only once saved. Save asks first (`VIWndMessage`), shows
//! "Saving..." for four frames and applies; Triangle on the menu puts every
//! setting back as it was and applies. The memory card is not written.

#![allow(clippy::too_many_arguments)]

use vi::fpu::{Ee, F};
use vi::ui::{Raster, UI_COLORS, Vui, Window, wide};

/// `VIGetFeatureFlags()`: 0x300 in every live game (0x100 the eight songs
/// and Random, 0x200 Vibration).
const FEATURES: u32 = 0x300;

/// `VIGameOptions`' settings, by the Options window's labels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameOptions {
    /// `m_language` (+0x04): the Battle Music track: 0 none, 1..8, 9 Random
    pub battle: u8,
    /// `m_x05`: Sound Output, 1 Stereo
    pub stereo: u8,
    /// `m_x06`: 3rd Person Camera, 0 Tether, 1 Follow
    pub follow: u8,
    /// `m_x07`: Camera Pitch Axis, 1 Inverted
    pub inverted: u8,
    /// `m_musicVolume` (+0x08): shown as Fx Volume, sound class 0
    pub fx_volume: F<Ee>,
    /// `m_effectVolume` (+0x0C): shown as Music Volume, sound class 1
    pub music_volume: F<Ee>,
    /// `m_x91`: Soft Keyboard, 1 Compact
    pub compact: u8,
    /// `m_x92`: no damage numbers while set
    pub no_damage: u8,
    /// `m_x93` / `m_x94`: faction / loot messages off while set
    pub no_faction: u8,
    pub no_loot: u8,
    /// `m_x95`: Auto Attack (a toggle, not held)
    pub auto_attack: u8,
    /// `m_x97`: Vibration
    pub vibration: u8,
}

impl Default for GameOptions {
    /// `SetDefaults`.
    fn default() -> Self {
        GameOptions {
            battle: if FEATURES & 0x100 != 0 { 9 } else { 0 },
            stereo: 1,
            follow: 0,
            inverted: 0,
            fx_volume: F::ONE,
            music_volume: F::ONE,
            compact: 0,
            no_damage: 0,
            no_faction: 0,
            no_loot: 0,
            auto_attack: 0,
            vibration: 1,
        }
    }
}

/// What the window asks of the game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OptionsUp {
    /// `SetSoundClassVolume(class, v)`
    Volume(i32, F<Ee>),
    /// `SetOutputMode(stereo)`
    Output(bool),
    /// the Sound page's preview of the Battle Music choice (`m_music`):
    /// a track (0 silence), until `PreviewEnd`
    Preview(i32),
    PreviewEnd,
    /// `SetVisible(false)` and `ApplyOptions`
    Apply,
}

/// `VIWndMessage`: a titled box of text and its choices, Yes/No here.
#[derive(Default)]
struct Message {
    title: Vec<u16>,
    text: Vec<u16>,
    prompt: i32,
    center: [i32; 2],
    width: i32,
    rows: i32,
    sel: i32,
    visible: bool,
    window: Window,
}

impl Message {
    /// `Setup(title, text, prompt, center, width)`.
    fn setup(&mut self, vui: &Vui, r: &mut dyn Raster, title: Vec<u16>, text: Vec<u16>, prompt: i32, center: [i32; 2], width: i32) {
        self.title = title;
        self.text = text;
        self.prompt = prompt;
        self.center = center;
        self.width = width;
        self.window.init(vui, r, center, width, 20, 0);
        self.rows = self.window.count_lines(vui, r, &self.text, width - 80) + 3;
        self.sel = 0;
    }

    /// `OnMessage` 295: the focus opens it; a two-choice box starts on the
    /// second choice.
    fn focus(&mut self, vui: &Vui, r: &dyn Raster) {
        self.window.close(true);
        self.window.init(vui, r, self.center, self.width, self.rows, 0);
        self.window.open(false);
        let t = self.title.clone();
        self.window.set_title(Some(&t));
        self.sel = if self.prompt == 0 { 0 } else { 1 };
    }

    /// `OnKeyDown`: left and right move, Enter or X closes it with the
    /// choice (307).
    fn key(&mut self, key: i32, now: i64, cues: &mut Vec<i32>) -> Option<i32> {
        match key {
            17 | 136 => {
                if self.sel > 0 {
                    self.sel -= 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                }
            }
            18 | 137 => {
                if self.sel < (self.prompt != 0) as i32 {
                    self.sel += 1;
                    self.window.reset_tick_pulse(now);
                    cues.push(11);
                }
            }
            13 | 134 => {
                self.visible = false;
                cues.push(if self.prompt == 0 || self.sel == 0 { 13 } else { 8 });
                return Some(self.sel);
            }
            _ => {}
        }
        None
    }

    /// `OnDrawSelf`.
    fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, focused: bool) {
        if !focused {
            self.window.reset_tick_pulse(vui.time);
        }
        let col = [self.window.size[0] / 3, self.window.size[0] / 3 * 2];
        self.window.draw(vui, r);
        let c0 = UI_COLORS[0];
        let width = self.window.size[0];
        let text = self.text.clone();
        self.window.text_box(vui, r, &text, 40, 1, width - 80, self.rows - 2, c0, 8);
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let lit = |s: String, on: bool| wide(&if on { format!("|q{s}|w") } else { s });
        let row = self.rows - 1;
        match self.prompt {
            0 => {
                self.window.text_box(vui, r, &lit(w(786), true), 0, row, width, 1, c0, 8);
            }
            p => {
                let (a, b) = if p == 1 { (786, 213) } else { (21, 22) };
                self.window.text_box(vui, r, &lit(w(a), self.sel == 0), col[0] - 100, row, 200, 1, c0, 8);
                self.window.text_box(vui, r, &lit(w(b), self.sel == 1), col[1] - 100, row, 200, 1, c0, 8);
            }
        }
    }
}

/// `VIWndOptions`.
#[derive(Default)]
pub struct WndOptions {
    /// `m_sel`: the menu's row
    sel: i32,
    /// `m_page`: -1 the menu, 0 Game, 1 Interface, 2 Sound
    page: i32,
    /// `m_row`: the page's row
    row: i32,
    /// `m_item`: what the page's row changes (set as the page is drawn)
    item: i32,
    /// the values as the window opened, put back by Triangle
    saved: GameOptions,
    /// `m_edit93` .. `m_edit97`: the four settings only Save commits, held
    /// the way the window holds them (faction and loot ON while set)
    edit: [u8; 4],
    save_count: i32,
    message: Message,
    window: Window,
    window2: Window,
    window3: Window,
    /// the preview last asked for
    preview: Option<i32>,
}

impl WndOptions {

    /// A pointer at frame point `p` (`pointer`): what is under it selected,
    /// and the keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let _ = p;
        Vec::new()
    }
    /// `OnMessage` 295 with the focus coming from outside: the window (300
    /// wide, seven rows, titled 788) opening, the values kept, the menu.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, o: &GameOptions) {
        self.window.close(true);
        self.window.init(vui, r, [320, 246], 300, 7, 0);
        self.window.open(false);
        self.window.set_title(Some(&wide(vui.words.get(788, vui.language))));
        self.message.visible = false;
        self.saved = *o;
        self.edit = [(o.no_faction == 0) as u8, (o.no_loot == 0) as u8, o.auto_attack, o.vibration];
        self.sel = 0;
        self.page = -1;
        self.item = -1;
        self.save_count = 0;
        self.preview = None;
    }

    /// `OnKeyDown`; the message box's while it is up.
    pub fn key(&mut self, vui: &Vui, r: &mut dyn Raster, o: &mut GameOptions, key: i32, now: i64, cues: &mut Vec<i32>, ups: &mut Vec<OptionsUp>) {
        if self.message.visible {
            // 307: Yes commits the four held settings and saves
            if self.message.key(key, now, cues) == Some(0) {
                o.no_faction = (self.edit[0] == 0) as u8;
                o.no_loot = (self.edit[1] == 0) as u8;
                o.auto_attack = self.edit[2];
                o.vibration = self.edit[3];
                self.save_count = 4;
            }
            return;
        }
        if self.save_count != 0 {
            return;
        }
        match key {
            19 | 138 => self.prev(now, cues),
            20 | 139 => self.next(now, cues),
            17 | 136 => self.change(o, -1, cues, ups),
            18 | 137 => self.change(o, 1, cues, ups),
            13 | 134 => {
                if self.select(vui, r, cues) {
                    let w = |id: i32| wide(vui.words.get(id, vui.language));
                    self.message.setup(vui, r, w(1633), w(1634), 2, self.window.center, 300);
                    self.message.visible = true;
                    self.message.focus(vui, r);
                }
            }
            27 | 132
                if self.back(cues) => {
                    let s = self.saved;
                    o.follow = s.follow;
                    o.inverted = s.inverted;
                    o.battle = s.battle;
                    o.stereo = s.stereo;
                    o.fx_volume = s.fx_volume;
                    o.music_volume = s.music_volume;
                    o.compact = s.compact;
                    o.no_damage = s.no_damage;
                    ups.push(OptionsUp::Volume(0, o.fx_volume));
                    ups.push(OptionsUp::Volume(1, o.music_volume));
                    ups.push(OptionsUp::Apply);
                }
            _ => {}
        }
    }

    /// `Prev`.
    fn prev(&mut self, now: i64, cues: &mut Vec<i32>) {
        let at = if self.page == -1 { &mut self.sel } else { &mut self.row };
        if *at > 0 {
            *at -= 1;
            self.window.reset_tick_pulse(now);
            cues.push(11);
        } else {
            cues.push(9);
        }
    }

    /// `Next`: the last row of each page; Interface gains Vibration.
    fn next(&mut self, now: i64, cues: &mut Vec<i32>) {
        let last = if self.page == -1 {
            3
        } else {
            let mut last = [2, 3, 3];
            if FEATURES & 0x100 != 0 {
                last[1] = 4;
            }
            last[self.page as usize]
        };
        let at = if self.page == -1 { &mut self.sel } else { &mut self.row };
        if *at < last {
            *at += 1;
            self.window.reset_tick_pulse(now);
            cues.push(11);
        } else {
            cues.push(9);
        }
    }

    /// `Decrement` (`d` -1) and `Increment` (1): the item's value; a volume
    /// stops at its end with sound 9.
    fn change(&mut self, o: &mut GameOptions, d: i32, cues: &mut Vec<i32>, ups: &mut Vec<OptionsUp>) {
        let tracks = if FEATURES & 0x100 != 0 { 10 } else { 3 };
        let step = F::<Ee>::new(0.1);
        let volume = |v: &mut F<Ee>, cues: &mut Vec<i32>| {
            if d < 0 && F::ZERO < *v {
                let n = *v - step;
                *v = if n < F::ZERO { F::ZERO } else { n };
                cues.push(11);
            } else if d > 0 && *v < F::ONE {
                let n = *v + step;
                *v = if F::ONE < n { F::ONE } else { n };
                cues.push(11);
            } else {
                cues.push(9);
            }
        };
        match self.item {
            0 => o.follow = (o.follow != 1) as u8,
            1 => o.inverted = (o.inverted == 0) as u8,
            2 => o.battle = ((o.battle as i32 + if d < 0 { tracks - 1 } else { 1 }) % tracks) as u8,
            3 => {
                volume(&mut o.fx_volume, cues);
                ups.push(OptionsUp::Volume(0, o.fx_volume));
            }
            4 => {
                volume(&mut o.music_volume, cues);
                ups.push(OptionsUp::Volume(1, o.music_volume));
            }
            5 => {
                o.stereo ^= 1;
                ups.push(OptionsUp::Output(o.stereo != 0));
            }
            6 => o.compact ^= 1,
            7 => o.no_damage = (o.no_damage == 0) as u8,
            8..=11 => self.edit[self.item as usize - 8] ^= 1,
            _ => {}
        }
        if !(3..=4).contains(&self.item) && (0..=11).contains(&self.item) {
            cues.push(11);
        }
        if d < 0 {
            if o.fx_volume < F::new(0.05) {
                o.fx_volume = F::ZERO;
            }
            if o.music_volume < F::new(0.05) {
                o.music_volume = F::ZERO;
            }
        } else {
            if F::new(0.95) < o.fx_volume {
                o.fx_volume = F::ONE;
            }
            if F::new(0.95) < o.music_volume {
                o.music_volume = F::ONE;
            }
        }
    }

    /// `Select`: X on the menu opens a page (its window 440 wide, six to
    /// eight rows, an empty title), or on Save Changes asks (true).
    fn select(&mut self, vui: &Vui, r: &dyn Raster, cues: &mut Vec<i32>) -> bool {
        if self.page != -1 {
            cues.push(9);
            return false;
        }
        cues.push(8);
        self.row = 0;
        let rows = match self.sel {
            0 => 6,
            1 => {
                if FEATURES & 0x200 != 0 {
                    8
                } else {
                    7
                }
            }
            2 => 7,
            _ => return true,
        };
        self.page = self.sel;
        self.window2.close(true);
        self.window2.init(vui, r, self.window.center, 440, rows, 0);
        self.window2.open(false);
        self.window2.set_title(Some(&[]));
        false
    }

    /// `Back`: Triangle on a page goes back to the menu; on the menu, true
    /// to close.
    fn back(&mut self, cues: &mut Vec<i32>) -> bool {
        cues.push(8);
        if self.page >= 0 {
            self.page = -1;
            return false;
        }
        true
    }

    /// `OnDrawSelf`: the Sound page previews the Battle Music choice, the
    /// menu, the page, then "Saving..." for four frames before the save.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, o: &GameOptions, ups: &mut Vec<OptionsUp>) {
        let focused = !self.message.visible;
        if !focused {
            self.window.reset_tick_pulse(vui.time);
        }
        // m_music: on the Sound page Play(track), Random as Stop (silence
        // here too); off it StopNow, which hands the slot back
        let want = (self.page == 2).then_some(if o.battle < 9 { o.battle as i32 } else { 0 });
        if want != self.preview {
            ups.push(match want {
                Some(t) => OptionsUp::Preview(t),
                None => OptionsUp::PreviewEnd,
            });
            self.preview = want;
        }
        self.draw_menu(vui, r);
        match self.page {
            0 => self.draw_game(vui, r, o),
            1 => self.draw_interface(vui, r, o),
            2 => self.draw_sound(vui, r, o),
            _ => {}
        }
        let w = |id: i32| wide(vui.words.get(id, vui.language));
        if self.save_count != 0 {
            self.save_count -= 1;
            self.window3.init(vui, r, self.window.center, 400, 3, 0);
            self.window3.open(true);
            self.window3.draw(vui, r);
            let width = self.window3.size[0];
            self.window3.text_box(vui, r, &w(1615), 40, 0, width - 80, 3, UI_COLORS[0], 8);
            if self.save_count == 0 {
                // SaveOptions: the card is not written; it closes and applies
                ups.push(OptionsUp::Apply);
            }
        }
        if self.message.visible {
            self.message.draw(vui, r, true);
        }
    }

    /// `DrawMenu`: Game, Interface, Sound, Save Changes, the selected one
    /// lit, and the legend.
    fn draw_menu(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.item = -1;
        self.window.draw(vui, r);
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let width = self.window.size[0];
        for (k, id) in [773, 778, 800, 482].into_iter().enumerate() {
            let on = self.sel == k as i32;
            let s = if on { format!("|q{}|w", w(id)) } else { w(id) };
            self.window.text_box(vui, r, &wide(&s), 40, k as i32 + 1, width - 80, 1, UI_COLORS[on as usize], 8);
        }
        let legend = wide(&format!("|x {}   |t {}", w(212), w(753)));
        self.window.text(vui, r, &legend, 40, 6, UI_COLORS[0], 0);
    }

    /// A page's rows: its labels, the tick, and each value in its frame
    /// (the arrows on the selected one); a value `None` is a slider drawn by
    /// the caller.
    fn draw_rows(&mut self, vui: &Vui, r: &mut dyn Raster, labels: &[i32], values: &[Option<Vec<u16>>]) {
        self.window2.draw(vui, r);
        let width = self.window2.size[0];
        for (k, &id) in labels.iter().enumerate() {
            let c = UI_COLORS[(self.row == k as i32) as usize];
            self.window2.text_box(vui, r, &wide(vui.words.get(id, vui.language)), 40, k as i32 + 1, width - 80, 1, c, 0);
        }
        self.window2.tick_r(vui, r, 36, self.row + 1, true);
        for (k, v) in values.iter().enumerate() {
            let Some(v) = v else { continue };
            let row = k as i32 + 1;
            self.window2.draw_frame(vui, r, width - 176, row, 132, 1, if self.row == k as i32 { 96 } else { 64 });
            self.window2.text_box(vui, r, v, width - 176, row, 132, 1, UI_COLORS[0], 8);
        }
    }

    /// The pages' legend: Triangle, Back.
    fn back_legend(&mut self, vui: &Vui, r: &mut dyn Raster, row: i32) {
        let s = wide(&format!("|t {}", vui.words.get(750, vui.language)));
        self.window2.text(vui, r, &s, 40, row, UI_COLORS[0], 0);
    }

    /// `DrawGamePage`: Faction Messages, Loot Messages, Damage Numbers.
    fn draw_game(&mut self, vui: &Vui, r: &mut dyn Raster, o: &GameOptions) {
        self.item = [8, 9, 7].get(self.row as usize).copied().unwrap_or(self.item);
        let on_off = |v: u8| Some(wide(vui.words.get(if v != 0 { 551 } else { 552 }, vui.language)));
        self.draw_rows(vui, r, &[738, 739, 759], &[on_off(self.edit[0]), on_off(self.edit[1]), on_off((o.no_damage == 0) as u8)]);
        self.back_legend(vui, r, 5);
    }

    /// `DrawInterfacePage`: Auto Attack, 3rd Person Camera, Camera Pitch
    /// Axis, Soft Keyboard and (feature 0x200) Vibration.
    fn draw_interface(&mut self, vui: &Vui, r: &mut dyn Raster, o: &GameOptions) {
        self.item = [10, 0, 1, 6, 11].get(self.row as usize).copied().unwrap_or(self.item);
        let word = |id: i32| Some(wide(vui.words.get(id, vui.language)));
        let on_off = |v: u8| word(if v != 0 { 551 } else { 552 });
        let mut labels = vec![1792, 747, 752, 794];
        let mut values = vec![on_off(self.edit[2]), word(if o.follow != 0 { 68 } else { 805 }), word(if o.inverted != 0 { 779 } else { 784 }), word(if o.compact != 0 { 755 } else { 802 })];
        if FEATURES & 0x200 != 0 {
            labels.push(1604);
            values.push(on_off(self.edit[3]));
        }
        self.draw_rows(vui, r, &labels, &values);
        self.back_legend(vui, r, if FEATURES & 0x200 != 0 { 7 } else { 6 });
    }

    /// `DrawSoundPage`: Battle Music, Fx Volume, Music Volume, Sound Output.
    fn draw_sound(&mut self, vui: &Vui, r: &mut dyn Raster, o: &GameOptions) {
        self.item = [2, 3, 4, 5].get(self.row as usize).copied().unwrap_or(self.item);
        let word = |id: i32| Some(wide(vui.words.get(id, vui.language)));
        // sTracks: None, Song 1..8, Random
        let tracks = [517, 795, 796, 797, 798, 799, 1576, 1577, 1578, 1630];
        let track = word(tracks.get(o.battle as usize).copied().unwrap_or(517));
        self.draw_rows(vui, r, &[751, 772, 782, 801], &[track, None, None, word(if o.stereo != 0 { 804 } else { 781 })]);
        let width = self.window2.size[0];
        self.window2.draw_hscroll(vui, r, width - 180, 2, 140, o.fx_volume.0, if self.row == 1 { 32 } else { 0 });
        self.window2.draw_hscroll(vui, r, width - 180, 3, 140, o.music_volume.0, if self.row == 2 { 32 } else { 0 });
        self.back_legend(vui, r, 6);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four held settings change only when saved; the live ones at
    /// once; Triangle on the menu puts the live ones back.
    #[test]
    fn held_settings_wait_for_save() {
        let mut w = WndOptions::default();
        let mut o = GameOptions::default();
        w.saved = o;
        w.edit = [1, 1, 0, 1];
        let (mut cues, mut ups) = (Vec::new(), Vec::new());
        w.item = 10; // Auto Attack
        w.change(&mut o, 1, &mut cues, &mut ups);
        assert_eq!((w.edit[2], o.auto_attack), (1, 0), "held until Save");
        w.item = 0; // 3rd Person Camera: live
        w.change(&mut o, 1, &mut cues, &mut ups);
        assert_eq!(o.follow, 1);
        w.item = 2; // Battle Music wraps both ways over ten
        w.change(&mut o, 1, &mut cues, &mut ups);
        assert_eq!(o.battle, 0);
        w.change(&mut o, -1, &mut cues, &mut ups);
        assert_eq!(o.battle, 9);
        assert_eq!(cues, vec![11, 11, 11, 11]);
        // Yes on the save box commits the held four
        w.message.visible = true;
        w.message.prompt = 2;
        w.message.sel = 0;
        // (no Vui needed for the message's keys)
        let sel = w.message.key(134, 0, &mut cues);
        assert_eq!(sel, Some(0));
        // as WndOptions::key does on Yes
        o.auto_attack = w.edit[2];
        assert_eq!(o.auto_attack, 1);
        // Triangle on the menu: back() closes
        w.page = 1;
        assert!(!w.back(&mut cues), "a page goes back to the menu first");
        assert!(w.back(&mut cues));
    }

    /// A volume stops at its ends with sound 9 and snaps within 0.05 of
    /// them; ten steps down from 1 reach exactly 0.
    #[test]
    fn volumes_stop_at_their_ends() {
        let mut w = WndOptions { item: 3, ..Default::default() };
        let mut o = GameOptions::default();
        let (mut cues, mut ups) = (Vec::new(), Vec::new());
        w.change(&mut o, 1, &mut cues, &mut ups);
        assert_eq!(cues, vec![9], "already at 1");
        for _ in 0..10 {
            w.change(&mut o, -1, &mut cues, &mut ups);
        }
        assert_eq!(o.fx_volume, F::ZERO);
        assert_eq!(cues.len(), 11);
        assert!(cues[1..].iter().all(|&c| c == 11));
        assert_eq!(ups.last(), Some(&OptionsUp::Volume(0, F::ZERO)));
        w.change(&mut o, -1, &mut cues, &mut ups);
        assert_eq!(cues.last(), Some(&9));
        assert_ne!(o.music_volume, F::ZERO, "the other volume is untouched");
    }
}
