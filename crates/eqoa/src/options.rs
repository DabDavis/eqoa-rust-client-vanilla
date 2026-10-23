//! The options: `VIGameOptions` (the settings the Options window shows; the
//! login half of the record is not kept here) and `VIWndOptions`, the window
//! the start menu's Options opens: a menu of Game, Interface, Sound and Save
//! Changes, each page a list of rows whose value left and right change.
//! Camera, pitch, volumes, output, battle music and the soft keyboard change
//! as they are moved; Faction Messages, Loot Messages, Auto Attack and
//! Vibration only once saved. Save asks first (`VIWndMessage`), shows
//! "Saving..." for four frames, keeps them (`SaveOptions`: the card's
//! `game.opt`, here `settings::save`) and applies; Triangle on the menu puts
//! every setting back as it was and applies.
//!
//! The client's own, beside the game's: an Extended page between Sound and
//! Save Changes (the menu a row longer), its words the client's (no
//! WORDS.TXT entry): Level of Detail (On: the game's levels of an LOD body;
//! Off: its near level out to its off distance, `lod.rs`), Brightness (a
//! slider, `settings::curve`), Fog (a slider: the game's distance fog and
//! far plane in the middle, off at the left, thicker to the right, `fog`),
//! Frame Rate (30, 60 or the display's,
//! `settings::frame_due`), Camera Distance (a slider: the follow orbit's
//! zoom-out ceiling, 8 to 50, `camstyle`), Camera Turn Speed (a slider: how
//! fast the stick and the keys turn the view, half to three times the
//! game's, `camstyle::turn_speed`), Screen Shake, Controller Focus (which
//! controllers this window reads: Auto, Window, Pad 1 to 4 or Off,
//! `input::pick`, kept for each window of the client open, `multibox`; what
//! it reads now is said beside Back) and Capture Controller, laid out and
//! driven as the game's rows are, eight at a time (the page scrolls a row). They change as they are moved, Triangle on the menu puts them
//! back, and Save Changes keeps them with the game's (`client.opt`).
//!
//! The client's own Controls page after it (`bindpage`): every key and
//! button of the default layout, moved by pressing the one to put there,
//! kept by Save Changes (`keys.opt`) and put back by Triangle as the rest.
//!
//! A pointer (`pointer`): a menu row is chosen at once; a page's row is
//! reached with up and down (a page knows which setting its row is only as
//! it draws, so left and right must wait for it), its label then steps its
//! value on, left or right of a value's middle (a slider's knob) is left or
//! right; the box's choices are taken at once.

#![allow(clippy::too_many_arguments)]

use crate::bindpage::ControlsPage;
use crate::pointer::{Hits, key, rows_rect};
use crate::settings::{BRIGHTNESS_STEPS, CAMERA_MAX, CAMERA_MIN, CAMERA_STEP, ClientOptions, Focus, FrameRate, TURN_MAX, TURN_MIN, TURN_STEP};
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
    /// the choices drawn (for a pointer)
    hits: Hits,
}

impl Message {
    /// A pointer at frame point `p`: a choice taken at once (✕).
    fn point(&mut self, p: [f32; 2]) -> Option<i32> {
        self.sel = self.hits.at(p)?;
        Some(key::CROSS)
    }

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
        self.hits.clear();
        match self.prompt {
            0 => {
                self.window.text_box(vui, r, &lit(w(786), true), 0, row, width, 1, c0, 8);
                self.hits.push(rows_rect(&self.window, vui, r, width / 2 - 80, row, 160, 1), 0);
            }
            p => {
                let (a, b) = if p == 1 { (786, 213) } else { (21, 22) };
                self.window.text_box(vui, r, &lit(w(a), self.sel == 0), col[0] - 100, row, 200, 1, c0, 8);
                self.window.text_box(vui, r, &lit(w(b), self.sel == 1), col[1] - 100, row, 200, 1, c0, 8);
                self.hits.push(rows_rect(&self.window, vui, r, col[0] - 80, row, 160, 1), 0);
                self.hits.push(rows_rect(&self.window, vui, r, col[1] - 80, row, 160, 1), 1);
            }
        }
    }
}

/// `VIWndOptions`.
#[derive(Default)]
pub struct WndOptions {
    /// `m_sel`: the menu's row (3 is the client's Extended, 4 its Controls,
    /// 5 Save Changes)
    sel: i32,
    /// `m_page`: -1 the menu, 0 Game, 1 Interface, 2 Sound, 3 Extended, 4
    /// Controls
    page: i32,
    /// `m_row`: the page's row
    row: i32,
    /// the Extended page's first row shown: it holds nine, eight at a time
    ext_top: i32,
    /// `m_item`: what the page's row changes (set as the page is drawn)
    item: i32,
    /// the values as the window opened, put back by Triangle
    saved: GameOptions,
    /// the client's own as it opened, put back with them
    saved_client: ClientOptions,
    /// the keys and buttons as it opened, put back with them
    saved_binds: crate::binds::Binds,
    /// the Controls page, and the sounds it made drawing (a held key's steps)
    controls: ControlsPage,
    controls_cues: Vec<i32>,
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
    /// what was drawn where, for a pointer: the menu's rows (their index),
    /// a page's labels (10 + row), the left and right of its values (20 +
    /// row, 30 + row); and when
    hits: Hits,
    now: i64,
}

impl WndOptions {
    /// A pointer at frame point `p` (`pointer`): the box's choices; a menu
    /// row chosen at once (✕); a page's row reached with up and down, its
    /// label then its value stepped on (right), left or right of a value's
    /// middle (a slider's knob) left or right. Nothing while saving. The
    /// keys to press.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.message.visible {
            return self.message.point(p).into_iter().collect();
        }
        if self.save_count != 0 {
            return Vec::new();
        }
        if self.page == 4 {
            return self.controls.point(p);
        }
        let Some(h) = self.hits.at(p) else { return Vec::new() };
        let k = h % 10;
        if h < 10 {
            // the menu: `Prev`/`Next` start the tick's pulse over
            self.sel = k;
            self.window.reset_tick_pulse(self.now);
            return vec![key::CROSS];
        }
        let mut keys = crate::pointer::steps(self.row, k, key::UP, key::DOWN);
        match h / 10 {
            1 if keys.is_empty() => keys.push(key::RIGHT),
            2 => keys.push(key::LEFT),
            3 => keys.push(key::RIGHT),
            _ => {}
        }
        keys
    }
    /// `OnMessage` 295 with the focus coming from outside: the window (300
    /// wide, seven rows and one each for Extended and Controls, titled 788)
    /// opening, the values kept, the menu.
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, o: &GameOptions) {
        self.window.close(true);
        self.window.init(vui, r, [320, 246], 300, 9, 0);
        self.window.open(false);
        self.window.set_title(Some(&wide(vui.words.get(788, vui.language))));
        self.message.visible = false;
        self.saved = *o;
        self.saved_client = crate::settings::client();
        self.saved_binds = crate::settings::binds();
        self.edit = [(o.no_faction == 0) as u8, (o.no_loot == 0) as u8, o.auto_attack, o.vibration];
        self.sel = 0;
        self.page = -1;
        self.item = -1;
        self.save_count = 0;
        self.preview = None;
    }

    /// The Controls page straight from play (F1, the HUD's "F1 Controls"):
    /// the window opened as from the start menu, on its Controls row and
    /// page; Triangle goes back to the menu as from there.
    pub fn open_controls(&mut self, vui: &Vui, r: &dyn Raster, o: &GameOptions) {
        self.open(vui, r, o);
        self.sel = 4;
        self.select(vui, r, &mut Vec::new());
    }

    /// A key let go: the Controls page's held up or down ends.
    pub fn release(&mut self, key: i32) {
        if self.page == 4 {
            self.controls.key(key, false, self.now, &mut Vec::new());
        }
    }

    /// What the Controls page waits for, if anything.
    pub fn capturing(&self) -> Option<crate::bindpage::Column> {
        if self.page == 4 && !self.message.visible { self.controls.capture } else { None }
    }

    /// A key pressed while the Controls page waits (`binds::name_of`).
    pub fn capture_key(&mut self, vui: &Vui, name: &str, cues: &mut Vec<i32>) {
        if self.capturing().is_some() {
            self.controls.capture_key(vui, name, cues);
        }
    }

    /// A controller's button pressed while the Controls page waits (its bit
    /// index, before any remap).
    pub fn capture_button(&mut self, vui: &Vui, phys: u8, cues: &mut Vec<i32>) {
        if self.capturing().is_some() {
            self.controls.capture_button(vui, phys, cues);
        }
    }

    /// The sounds the Controls page made as it drew (a held key's steps).
    pub fn take_cues(&mut self) -> Vec<i32> {
        std::mem::take(&mut self.controls_cues)
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
        if self.page == 4 {
            if self.controls.key(key, true, now, cues) {
                self.back(cues);
            }
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
                    crate::settings::set_client(self.saved_client);
                    crate::settings::set_binds(self.saved_binds.clone());
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

    /// `Next`: the last row of each page; Interface gains Vibration; the
    /// menu's Extended and Controls are the client's.
    fn next(&mut self, now: i64, cues: &mut Vec<i32>) {
        let last = if self.page == -1 {
            5
        } else {
            let mut last = [2, 3, 3, EXTENDED_ROWS.len() as i32 - 1];
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
    /// stops at its end with sound 9. Items 12 to 20 are the client's
    /// Extended rows: Level of Detail, Screen Shake (18) and Capture
    /// Controller (19) turn over, Brightness, Fog (20), Camera Distance and
    /// Camera Turn Speed (17) step as a volume does (stopping at their ends
    /// with sound 9), Frame Rate goes round 30, 60 and the display's and
    /// Controller Focus round its seven as Battle Music goes round its tracks.
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
            16 => {
                let all = Focus::ALL;
                let at = all.iter().position(|&f| f == crate::settings::focus()).unwrap_or(0);
                crate::settings::set_focus(all[(at + if d < 0 { all.len() - 1 } else { 1 }) % all.len()]);
            }
            12..=15 | 17..=20 => {
                let mut c = crate::settings::client();
                // a slider's step, or sound 9 at its end
                let mut slide = |v: &mut u8, lo: u8, hi: u8, by: u8| {
                    if (d < 0 && *v > lo) || (d > 0 && *v < hi) {
                        *v = (*v as i32 + d.signum() * by as i32).clamp(lo as i32, hi as i32) as u8;
                        cues.push(11);
                    } else {
                        cues.push(9);
                    }
                };
                match self.item {
                    12 => c.lod = !c.lod,
                    18 => c.screen_shake = !c.screen_shake,
                    19 => c.capture = !c.capture,
                    13 => slide(&mut c.brightness, 0, BRIGHTNESS_STEPS, 1),
                    20 => slide(&mut c.fog, 0, crate::fog::STEPS, 1),
                    15 => slide(&mut c.camera_distance, CAMERA_MIN, CAMERA_MAX, CAMERA_STEP),
                    17 => {
                        let t = &mut c.camera_turn;
                        if (d < 0 && *t > TURN_MIN) || (d > 0 && *t < TURN_MAX) {
                            *t = (*t as i32 + d.signum() * TURN_STEP as i32).clamp(TURN_MIN as i32, TURN_MAX as i32) as u16;
                            cues.push(11);
                        } else {
                            cues.push(9);
                        }
                    }
                    // 14, Frame Rate
                    _ => {
                        let all = FrameRate::ALL;
                        let at = all.iter().position(|&r| r == crate::settings::frame_rate()).unwrap_or(0);
                        c.frame_rate = Some(all[(at + if d < 0 { all.len() - 1 } else { 1 }) % all.len()]);
                    }
                }
                crate::settings::set_client(c);
            }
            _ => {}
        }
        if !matches!(self.item, 3 | 4 | 13 | 15 | 17 | 20) && (0..=20).contains(&self.item) {
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
    /// eight rows, an empty title; the client's Extended nine, eight of its
    /// rows and the legend, its Controls 600 wide over the middle of the
    /// frame, as many rows as fit), or on Save Changes asks (true).
    fn select(&mut self, vui: &Vui, r: &dyn Raster, cues: &mut Vec<i32>) -> bool {
        if self.page != -1 {
            cues.push(9);
            return false;
        }
        cues.push(8);
        self.row = 0;
        self.ext_top = 0;
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
            3 => 9,
            4 => {
                self.page = 4;
                self.controls.reset();
                self.window2.close(true);
                self.window2.init(vui, r, [320, 224], 600, ControlsPage::window_rows(vui.font_height(r)), 0);
                self.window2.open(false);
                self.window2.set_title(Some(&wide(CONTROLS)));
                return false;
            }
            _ => return true,
        };
        self.page = self.sel;
        self.window2.close(true);
        self.window2.init(vui, r, self.window.center, 440, rows, 0);
        self.window2.open(false);
        if rows > 8 {
            // the client's Extended page holds a row more than the game's
            // pages (eight): it goes without their empty title bar (six
            // pixels each way) and its top where theirs is, so the options
            // window's title tab above it still shows and its bottom stays
            // clear of the title's menu under it
            self.window2.set_title(None);
            let mut eight = Window::default();
            eight.init(vui, r, self.window.center, 440, 8, 0);
            eight.set_title(Some(&[]));
            self.window2.center[1] += eight.top_left()[1] - self.window2.top_left()[1];
        } else {
            self.window2.set_title(Some(&[]));
        }
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
    /// menu, the page, then "Saving..." for four frames before the save
    /// (`SaveOptions`), which keeps them and applies.
    pub fn draw(&mut self, vui: &Vui, r: &mut dyn Raster, o: &GameOptions, ups: &mut Vec<OptionsUp>) {
        self.now = vui.time;
        self.hits.clear();
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
            3 => self.draw_extended(vui, r),
            4 => self.controls.draw(vui, r, &mut self.window2, &mut self.controls_cues),
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
                // SaveOptions: VIGameOptions::Save (a failure is said and
                // play goes on), then it closes and applies
                crate::settings::save(o);
                ups.push(OptionsUp::Apply);
            }
        }
        if self.message.visible {
            self.message.draw(vui, r, true);
        }
    }

    /// `DrawMenu`: Game, Interface, Sound, the client's Extended and
    /// Controls, Save Changes, the selected one lit, and the legend.
    fn draw_menu(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.item = -1;
        self.window.draw(vui, r);
        let w = |id: i32| match id {
            -1 => EXTENDED.to_string(),
            -2 => CONTROLS.to_string(),
            id => vui.words.get(id, vui.language).to_string(),
        };
        let width = self.window.size[0];
        for (k, id) in [773, 778, 800, -1, -2, 482].into_iter().enumerate() {
            let on = self.sel == k as i32;
            let s = if on { format!("|q{}|w", w(id)) } else { w(id) };
            self.window.text_box(vui, r, &wide(&s), 40, k as i32 + 1, width - 80, 1, UI_COLORS[on as usize], 8);
            if self.page == -1 {
                self.hits.push(rows_rect(&self.window, vui, r, 40, k as i32 + 1, width - 80, 1), k as i32);
            }
        }
        let legend = wide(&format!("|x {}   |t {}", w(212), w(753)));
        self.window.text(vui, r, &legend, 40, 8, UI_COLORS[0], 0);
    }

    /// A page's rows: its labels, the tick, and each value in its frame
    /// (the arrows on the selected one); a value `None` is a slider drawn by
    /// the caller. The game's pages start a row down (`first` 1).
    fn draw_rows(&mut self, vui: &Vui, r: &mut dyn Raster, labels: &[Vec<u16>], values: &[Option<Vec<u16>>]) {
        self.draw_rows_from(vui, r, labels, values, 1, 0);
    }

    /// `draw_rows` with the first label at row `first`, the page's rows from
    /// `skip` on (a page that scrolls: the labels and values given are the
    /// ones shown, the hits and the tick the page's own rows).
    fn draw_rows_from(&mut self, vui: &Vui, r: &mut dyn Raster, labels: &[Vec<u16>], values: &[Option<Vec<u16>>], first: i32, skip: i32) {
        self.window2.draw(vui, r);
        let width = self.window2.size[0];
        for (k, label) in labels.iter().enumerate() {
            let c = UI_COLORS[(self.row == k as i32 + skip) as usize];
            self.window2.text_box(vui, r, label, 40, k as i32 + first, width - 80, 1, c, 0);
            self.hits.push(rows_rect(&self.window2, vui, r, 40, k as i32 + first, width - 176 - 48, 1), 10 + k as i32 + skip);
        }
        if (skip..skip + labels.len() as i32).contains(&self.row) {
            self.window2.tick_r(vui, r, 36, self.row - skip + first, true);
        }
        for (k, v) in values.iter().enumerate() {
            let Some(v) = v else { continue };
            let row = k as i32 + first;
            self.window2.draw_frame(vui, r, width - 176, row, 132, 1, if self.row == k as i32 + skip { 96 } else { 64 });
            self.window2.text_box(vui, r, v, width - 176, row, 132, 1, UI_COLORS[0], 8);
            // the frame's two halves (its side arrows are glyphs)
            let at = self.window2.top_left()[0] + width - 176;
            self.halves(rows_rect(&self.window2, vui, r, width - 176, row, 132, 1), at - 4, at + 66, at + 136, k as i32 + skip);
        }
    }

    /// A value's left and right, `x0..mid` and `mid..x1` over the rows of
    /// `rr`, as row `k`'s (for a pointer).
    fn halves(&mut self, rr: [i32; 4], x0: i32, mid: i32, x1: i32, k: i32) {
        self.hits.push([x0, rr[1], mid, rr[3]], 20 + k);
        self.hits.push([mid, rr[1], x1, rr[3]], 30 + k);
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
        self.draw_rows(vui, r, &words(vui, &[738, 739, 759]), &[on_off(self.edit[0]), on_off(self.edit[1]), on_off((o.no_damage == 0) as u8)]);
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
        self.draw_rows(vui, r, &words(vui, &labels), &values);
        self.back_legend(vui, r, if FEATURES & 0x200 != 0 { 7 } else { 6 });
    }

    /// `DrawSoundPage`: Battle Music, Fx Volume, Music Volume, Sound Output.
    fn draw_sound(&mut self, vui: &Vui, r: &mut dyn Raster, o: &GameOptions) {
        self.item = [2, 3, 4, 5].get(self.row as usize).copied().unwrap_or(self.item);
        let word = |id: i32| Some(wide(vui.words.get(id, vui.language)));
        // sTracks: None, Song 1..8, Random
        let tracks = [517, 795, 796, 797, 798, 799, 1576, 1577, 1578, 1630];
        let track = word(tracks.get(o.battle as usize).copied().unwrap_or(517));
        self.draw_rows(vui, r, &words(vui, &[751, 772, 782, 801]), &[track, None, None, word(if o.stereo != 0 { 804 } else { 781 })]);
        let width = self.window2.size[0];
        self.window2.draw_hscroll(vui, r, width - 180, 2, 140, o.fx_volume.0, if self.row == 1 { 32 } else { 0 });
        self.window2.draw_hscroll(vui, r, width - 180, 3, 140, o.music_volume.0, if self.row == 2 { 32 } else { 0 });
        // each slider's left and right of its knob
        let knob = vui.entry(166).uv1[0];
        let at = self.window2.top_left()[0] + width - 180;
        for (k, v) in [(1, o.fx_volume.0), (2, o.music_volume.0)] {
            let mid = at + (v * (140 - knob) as f32) as i32 + knob / 2;
            self.halves(rows_rect(&self.window2, vui, r, width - 180, k + 1, 140, 1), at - 4, mid, at + 144, k);
        }
        self.back_legend(vui, r, 6);
    }

    /// The client's Extended page, laid out as the Interface page is: Level
    /// of Detail (the game's On and Off), Brightness and Fog (sliders as the
    /// volumes are, and each side of a knob for a pointer), Frame Rate,
    /// Camera Distance and Camera Turn Speed (sliders), Screen Shake,
    /// Controller Focus (Off the game's word) and Capture Controller; under
    /// them, beside the Back legend, what this window reads (`input::reads`).
    /// Its rows and the legend are more than the game's pages hold (seven and
    /// the legend under an empty row): they start at the top row, with the
    /// legend's row shared, so the page is no taller than nine rows
    /// (`select`) and clears the options window's title tab and the title's
    /// menu under it; eight rows show at a time, the selected one among them,
    /// with a scroll bar down the right as the Controls page has.
    fn draw_extended(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.item = [12, 13, 20, 14, 15, 17, 18, 16, 19].get(self.row as usize).copied().unwrap_or(self.item);
        let n = EXTENDED_ROWS.len() as i32;
        if self.row < self.ext_top {
            self.ext_top = self.row;
        } else if self.row >= self.ext_top + EXTENDED_SHOWN {
            self.ext_top = self.row - EXTENDED_SHOWN + 1;
        }
        self.ext_top = self.ext_top.clamp(0, (n - EXTENDED_SHOWN).max(0));
        let top = self.ext_top;
        let c = crate::settings::client();
        let on_off = |on: bool| wide(vui.words.get(if on { 551 } else { 552 }, vui.language));
        let lod = on_off(c.lod);
        let rate = match crate::settings::frame_rate() {
            FrameRate::Thirty => "30",
            FrameRate::Sixty => "60",
            FrameRate::Display => "Display",
        };
        let focus = match crate::settings::focus() {
            Focus::Off => vui.words.get(552, vui.language).to_string(),
            f => crate::input::focus_name(f),
        };
        let values = [Some(lod), None, None, Some(wide(rate)), None, None, Some(on_off(c.screen_shake)), Some(wide(&focus)), Some(on_off(c.capture))];
        let shown = top as usize..(top + EXTENDED_SHOWN).min(n) as usize;
        let labels: Vec<Vec<u16>> = EXTENDED_ROWS[shown.clone()].iter().map(|l| wide(l)).collect();
        self.draw_rows_from(vui, r, &labels, &values[shown], 0, top);
        let width = self.window2.size[0];
        let knob = vui.entry(166).uv1[0];
        let at = self.window2.top_left()[0] + width - 180;
        let fog = c.fog as f32 / crate::fog::STEPS as f32;
        let camera = (c.camera_distance - CAMERA_MIN) as f32 / (CAMERA_MAX - CAMERA_MIN) as f32;
        let turn = (c.camera_turn - TURN_MIN) as f32 / (TURN_MAX - TURN_MIN) as f32;
        for (k, t) in [(1, c.brightness as f32 / BRIGHTNESS_STEPS as f32), (2, fog), (4, camera), (5, turn)] {
            if !(top..top + EXTENDED_SHOWN).contains(&k) {
                continue;
            }
            self.window2.draw_hscroll(vui, r, width - 180, k - top, 140, t, if self.row == k { 32 } else { 0 });
            let mid = at + (t * (140 - knob) as f32) as i32 + knob / 2;
            self.halves(rows_rect(&self.window2, vui, r, width - 180, k - top, 140, 1), at - 4, mid, at + 144, k);
        }
        // the scroll bar, as the Controls page's
        if n > EXTENDED_SHOWN {
            let h = vui.font_height(r) - 4;
            let y = self.window2.row_center(vui, r, 0)[1] - 4;
            let t = top as f32 / (n - EXTENDED_SHOWN) as f32;
            self.window2.draw_vscroll(vui, r, [self.window2.top_left()[0] + width - 24, y - 3], EXTENDED_SHOWN * h + 4, t);
        }
        // the legend, and what this window reads at the row's right
        let legend = EXTENDED_SHOWN;
        self.back_legend(vui, r, legend);
        let after = 40 + self.window2.text_width(vui, r, &wide(&format!("|t {}", vui.words.get(750, vui.language)))) + 16;
        self.window2.text_box(vui, r, &wide(&crate::input::reads()), after, legend, width - 40 - after, 1, UI_COLORS[0], 0x10 | 2);
    }
}

/// The client's words for its own pages (no WORDS.TXT entry): the menu's
/// rows and the Extended page's nine. (WORDS.TXT's "CONTROLS", 1039, is in
/// capitals and used by none of the game's modules.)
const EXTENDED: &str = "Extended";
const CONTROLS: &str = "Controls";
const EXTENDED_ROWS: [&str; 9] = ["Level of Detail", "Brightness", "Fog", "Frame Rate", "Camera Distance", "Camera Turn Speed", "Screen Shake", "Controller Focus", "Capture Controller"];
/// The Extended page's rows shown at a time: as many as nine rows' height
/// holds with the legend's.
const EXTENDED_SHOWN: i32 = 8;

/// WORDS.TXT's entries `ids`, wide.
fn words(vui: &Vui, ids: &[i32]) -> Vec<Vec<u16>> {
    ids.iter().map(|&id| wide(vui.words.get(id, vui.language))).collect()
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

    /// A pointer on the options: a menu row chosen at once; a page's row
    /// reached by up and down, then its label steps it on; left or right of
    /// a value's middle, or of a slider's knob, is left or right; the save
    /// box's choices taken at once; nothing while saving.
    #[test]
    fn a_pointer_on_the_options() {
        use crate::pointer::key;
        use crate::pointer::testing::{NullRaster, mid, vui};
        let vui = vui();
        let mut r = NullRaster;
        let mut o = GameOptions::default();
        let mut w = WndOptions::default();
        w.open(&vui, &r, &o);
        let (mut cues, mut ups) = (Vec::new(), Vec::new());
        w.draw(&vui, &mut r, &o, &mut ups);
        let menu = |w: &WndOptions, k: i32| mid(rows_rect(&w.window, &vui, &NullRaster, 40, k + 1, 200, 1));
        assert_eq!(w.point(menu(&w, 2)), vec![key::CROSS]);
        assert_eq!(w.sel, 2, "Sound");
        assert!(w.point([5.0, 5.0]).is_empty());
        // ✕ opens the Sound page: rows Battle Music, two sliders, Output
        w.key(&vui, &mut r, &mut o, 134, 0, &mut cues, &mut ups);
        assert_eq!(w.page, 2);
        w.draw(&vui, &mut r, &o, &mut ups);
        let width = w.window2.size[0];
        let left = w.window2.top_left()[0];
        let label = |w: &WndOptions, k: i32| mid(rows_rect(&w.window2, &vui, &NullRaster, 40, k + 1, 100, 1));
        let y = |w: &WndOptions, k: i32| w.window2.row_center(&vui, &NullRaster, k + 1)[1] as f32;
        assert_eq!(w.point(label(&w, 3)), vec![key::DOWN, key::DOWN, key::DOWN], "up and down to it");
        assert_eq!(w.row, 0, "the keys move it, not the pointer");
        // the value frame of row 0 (Battle Music): its halves
        let frame = (left + width - 176) as f32;
        assert_eq!(w.point([frame + 20.0, y(&w, 0)]), vec![key::LEFT]);
        assert_eq!(w.point([frame + 110.0, y(&w, 0)]), vec![key::RIGHT]);
        assert_eq!(w.point(label(&w, 0)), vec![key::RIGHT], "the selected row's label steps it on");
        // a slider: left and right of its knob (Music Volume, row 2)
        let slider = (left + width - 180) as f32;
        assert_eq!(w.point([slider + 20.0, y(&w, 2)]), vec![key::DOWN, key::DOWN, key::LEFT], "volume 1: the knob at the right end");
        o.music_volume = F::ZERO;
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.point([slider + 20.0, y(&w, 2)]), vec![key::DOWN, key::DOWN, key::RIGHT], "volume 0: the knob at the left end");
        // back to the menu, Save Changes (past the client's Extended and
        // Controls): the box's choices
        w.key(&vui, &mut r, &mut o, 132, 0, &mut cues, &mut ups);
        w.sel = 5;
        w.key(&vui, &mut r, &mut o, 134, 0, &mut cues, &mut ups);
        assert!(w.message.visible);
        w.draw(&vui, &mut r, &o, &mut ups);
        let mw = w.message.window.size[0];
        let yes = mid(rows_rect(&w.message.window, &vui, &NullRaster, mw / 3 - 50, w.message.rows - 1, 100, 1));
        assert_eq!(w.message.sel, 1, "a two-choice box starts on the second");
        assert_eq!(w.point(yes), vec![key::CROSS]);
        assert_eq!(w.message.sel, 0, "Yes");
        assert!(w.point(menu(&w, 1)).is_empty(), "the menu behind the box");
        // saving: nothing
        w.message.visible = false;
        w.save_count = 4;
        assert!(w.point(menu(&w, 1)).is_empty());
    }

    /// The client's Extended page, by the pad: down three rows on the menu
    /// and ✕; each row's left and right (Level of Detail turns over,
    /// Brightness, Fog, Camera Distance and Camera Turn Speed stop at their
    /// ends with sound 9, Frame Rate goes round from the platform's); the
    /// page scrolls a row to show the last; Triangle on the menu puts them
    /// back; Save Changes keeps them, and the game's, where nothing was kept
    /// before (the control).
    #[test]
    fn the_extended_page() {
        use crate::pointer::testing::{NullRaster, vui};
        use crate::settings::{self, testing};
        testing::reset(false);
        let vui = vui();
        let mut r = NullRaster;
        let mut o = GameOptions::default();
        let mut w = WndOptions::default();
        let (mut cues, mut ups) = (Vec::new(), Vec::new());
        let mut press = |w: &mut WndOptions, o: &mut GameOptions, k: i32, cues: &mut Vec<i32>| {
            w.key(&vui, &mut r, o, k, 0, cues, &mut ups);
            w.draw(&vui, &mut NullRaster, o, &mut ups);
        };
        w.open(&vui, &NullRaster, &o);
        w.draw(&vui, &mut NullRaster, &o, &mut Vec::new());
        for k in [key::DOWN, key::DOWN, key::DOWN, key::CROSS] {
            press(&mut w, &mut o, k, &mut cues);
        }
        assert_eq!((w.sel, w.page, w.item), (3, 3, 12), "Extended, Level of Detail");
        cues.clear();
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert!(!settings::client().lod);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        for _ in 0..6 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!(settings::client().brightness, 10);
        assert_eq!(&cues[cues.len() - 2..], &[11, 9], "the last step, then its end");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 20, "Fog");
        assert_eq!(settings::fog(), crate::fog::GAME, "the game's");
        for _ in 0..6 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!(settings::fog(), crate::fog::STEPS);
        assert_eq!(&cues[cues.len() - 2..], &[11, 9], "the last step, then its end");
        for _ in 0..11 {
            press(&mut w, &mut o, key::LEFT, &mut cues);
        }
        assert_eq!(settings::fog(), 0, "off");
        assert_eq!(cues.last(), Some(&9));
        for _ in 0..5 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!(settings::fog(), crate::fog::GAME);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(settings::frame_rate(), FrameRate::Display, "the platform's");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!(settings::client().frame_rate, Some(FrameRate::Thirty), "round to 30");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!(settings::client().frame_rate, Some(FrameRate::Sixty));
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 15, "Camera Distance");
        for _ in 0..15 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!(settings::client().camera_distance, 50, "11 to 50 in 13 steps of 3");
        assert_eq!(&cues[cues.len() - 3..], &[11, 9, 9]);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 17, "Camera Turn Speed");
        assert_eq!(settings::camera_turn(), 100, "the game's");
        for _ in 0..9 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!(settings::camera_turn(), 300, "100 to 300 in 8 steps of 25");
        assert_eq!(&cues[cues.len() - 2..], &[11, 9], "the last step, then its end");
        for _ in 0..11 {
            press(&mut w, &mut o, key::LEFT, &mut cues);
        }
        assert_eq!(settings::camera_turn(), 50, "down to half");
        assert_eq!(cues.last(), Some(&9));
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 18, "Screen Shake");
        assert!(settings::screen_shake(), "the game's shake");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert!(!settings::screen_shake());
        assert_eq!(cues.last(), Some(&11));
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::screen_shake(), "turned over again");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 16, "Controller Focus");
        assert_eq!(settings::focus(), Focus::Auto);
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!(settings::focus(), Focus::Off, "round to Off");
        for _ in 0..3 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!(settings::focus(), Focus::Pad(1), "Auto, Window, Pad 1");
        assert_eq!(settings::client().controller[0], Focus::Pad(1), "the first window's: this one, with nowhere to take a place");
        assert_eq!(cues.last(), Some(&11));
        assert_eq!(w.ext_top, 0, "the first eight rows");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 19, "Capture Controller");
        assert_eq!(w.ext_top, 1, "scrolled a row to show it");
        assert!(!settings::capture(), "off: every window on a pad reads it");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert!(settings::capture());
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (8, Some(&9)), "the last row");
        // Triangle: the menu, then closed with every setting put back
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        assert_eq!(w.page, -1);
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        assert_eq!(settings::client(), settings::ClientOptions::default());
        assert_eq!(testing::kept("client.opt"), None, "nothing kept yet");
        // again, and saved this time, Auto Attack with it
        w.open(&vui, &NullRaster, &o);
        for k in [key::DOWN, key::CROSS, key::RIGHT, key::TRIANGLE, key::DOWN, key::DOWN, key::CROSS, key::DOWN, key::LEFT, key::DOWN, key::LEFT, key::DOWN, key::DOWN, key::DOWN, key::RIGHT, key::TRIANGLE, key::DOWN, key::DOWN, key::CROSS, key::LEFT, key::CROSS] {
            press(&mut w, &mut o, k, &mut cues);
        }
        assert_eq!(w.save_count, 3, "Saving... (one frame drawn)");
        for _ in 0..3 {
            w.draw(&vui, &mut NullRaster, &o, &mut ups);
        }
        let kept = String::from_utf8(testing::kept("client.opt").unwrap()).unwrap();
        assert_eq!(kept, "lod=on\nbrightness=4\ncamera_distance=11\ncamera_turn=125\nfog=4\n");
        assert_eq!(GameOptions::from_card(&testing::kept("game.opt").unwrap()).unwrap().auto_attack, 1);
        testing::reset(false);
    }

    /// The Extended page, a row more than the game's eight-row pages, keeps
    /// its top where theirs is (the Interface page's: under the options
    /// window's title tab) and, without their empty title bar, ends less than
    /// a row lower; the control: centred as the game's are, its top was half
    /// a row higher, over the tab.
    #[test]
    fn the_extended_page_keeps_the_title_tab_in_view() {
        use crate::pointer::testing::{NullRaster, vui};
        crate::settings::testing::reset(false);
        let vui = vui();
        let mut r = NullRaster;
        let mut o = GameOptions::default();
        let mut w = WndOptions::default();
        let (mut cues, mut ups) = (Vec::new(), Vec::new());
        let page = |w: &mut WndOptions, sel: i32, r: &mut NullRaster, o: &mut GameOptions, cues: &mut Vec<i32>, ups: &mut Vec<OptionsUp>| {
            w.open(&vui, r, o);
            w.sel = sel;
            w.key(&vui, r, o, key::CROSS, 0, cues, ups);
            (w.window2.top_left()[1], w.window2.size[1])
        };
        let (top8, h8) = page(&mut w, 1, &mut r, &mut o, &mut cues, &mut ups);
        assert_eq!(w.window2.rows, 8, "Interface: eight rows (Vibration)");
        let (top9, h9) = page(&mut w, 3, &mut r, &mut o, &mut cues, &mut ups);
        assert_eq!((w.window2.rows, w.window2.title_h), (9, 18), "a row more, no title bar");
        assert_eq!(top9, top8, "the top where an eight-row page's is");
        let d = vui.font_height(&r) - 4;
        assert!((0..d).contains(&(h9 - h8)), "less than a row lower at the bottom: {} of {d}", h9 - h8);
        // the control: nine rows laid out as the game's pages are, centred
        let mut game = Window::default();
        game.init(&vui, &r, w.window.center, 440, 9, 0);
        game.set_title(Some(&[]));
        assert!(game.top_left()[1] < top8, "centred, it reaches over the tab: {} vs {top8}", game.top_left()[1]);
    }

    /// The client's Controls page: the menu's fifth row opens it; a key
    /// moved there applies at once; Triangle on the menu puts it back (the
    /// control: before it, moved), Save Changes keeps it in keys.opt; F1's
    /// way opens straight on the page, and its Triangle goes to the menu.
    #[test]
    fn the_controls_page() {
        use crate::bindpage::Column;
        use crate::pointer::testing::{NullRaster, vui};
        use crate::settings::{self, testing};
        testing::reset(false);
        let vui = vui();
        let mut o = GameOptions::default();
        let mut w = WndOptions::default();
        let (mut cues, mut ups) = (Vec::new(), Vec::new());
        let mut press = |w: &mut WndOptions, o: &mut GameOptions, k: i32| {
            w.key(&vui, &mut NullRaster, o, k, 0, &mut cues, &mut ups);
            w.release(k);
            w.draw(&vui, &mut NullRaster, o, &mut ups);
        };
        w.open(&vui, &NullRaster, &o);
        for k in [key::DOWN, key::DOWN, key::DOWN, key::DOWN, key::CROSS] {
            press(&mut w, &mut o, k);
        }
        assert_eq!((w.sel, w.page), (4, 4), "Controls");
        assert_eq!(w.capturing(), None);
        press(&mut w, &mut o, key::CROSS);
        assert_eq!(w.capturing(), Some(Column::Keyboard), "Walk forward's key awaited");
        w.capture_key(&vui, "up", &mut Vec::new());
        assert_eq!(settings::binds().alias("up"), crate::binds::Alias::To("w"), "at once");
        // Triangle: the menu, then closed and put back
        press(&mut w, &mut o, key::TRIANGLE);
        assert_eq!(w.page, -1);
        assert!(!settings::binds().is_default(), "the control: still moved on the menu");
        press(&mut w, &mut o, key::TRIANGLE);
        assert!(settings::binds().is_default(), "put back");
        assert_eq!(testing::kept("keys.opt"), None);
        // again, kept with Save Changes
        w.open_controls(&vui, &NullRaster, &o);
        assert_eq!((w.sel, w.page), (4, 4), "F1's way: on the page");
        press(&mut w, &mut o, key::CROSS);
        w.capture_key(&vui, "up", &mut Vec::new());
        for k in [key::TRIANGLE, key::DOWN, key::CROSS, key::LEFT, key::CROSS] {
            press(&mut w, &mut o, k);
        }
        for _ in 0..3 {
            w.draw(&vui, &mut NullRaster, &o, &mut ups);
        }
        assert_eq!(String::from_utf8(testing::kept("keys.opt").unwrap()).unwrap(), "key.walk_forward=up\nkey.menu_up=w\n");
        settings::set_binds(Default::default());
        testing::reset(false);
    }

    /// A pointer on the Extended page: a label is reached by up and down,
    /// a value's halves are left and right, each slider's sides of its knob
    /// are; nothing where no row was drawn. Scrolled a row (the last row
    /// selected), the rows shown answer as the page's own rows: the first,
    /// gone from view, answers nothing, and the last is where the eighth was.
    #[test]
    fn a_pointer_on_the_extended_page() {
        use crate::pointer::testing::{NullRaster, mid, vui};
        crate::settings::testing::reset(false);
        let vui = vui();
        let mut r = NullRaster;
        let mut o = GameOptions::default();
        let mut w = WndOptions::default();
        let (mut cues, mut ups) = (Vec::new(), Vec::new());
        w.open(&vui, &r, &o);
        w.sel = 3;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.page, 3);
        let width = w.window2.size[0];
        let left = w.window2.top_left()[0] as f32;
        // its rows from the top one (eight shown and the legend:
        // `draw_extended`)
        let y = |w: &WndOptions, k: i32| w.window2.row_center(&vui, &NullRaster, k)[1] as f32;
        let label = |w: &WndOptions, k: i32| mid(rows_rect(&w.window2, &vui, &NullRaster, 40, k, 100, 1));
        let down = |n: usize, last: i32| std::iter::repeat_n(key::DOWN, n).chain([last]).collect::<Vec<_>>();
        assert_eq!(w.point(label(&w, 2)), vec![key::DOWN, key::DOWN], "Fog's label");
        assert_eq!(w.point(label(&w, 3)), vec![key::DOWN, key::DOWN, key::DOWN], "Frame Rate's label");
        assert_eq!(w.point(label(&w, 0)), vec![key::RIGHT], "Level of Detail's, selected: stepped on");
        let frame = left + (width - 176) as f32;
        assert_eq!(w.point([frame + 20.0, y(&w, 0)]), vec![key::LEFT]);
        assert_eq!(w.point([frame + 110.0, y(&w, 3)]), down(3, key::RIGHT), "Frame Rate's value");
        let slider = left + (width - 180) as f32;
        // Brightness and Fog at 5 of 10: their knobs mid-bar
        assert_eq!(w.point([slider + 10.0, y(&w, 1)]), vec![key::DOWN, key::LEFT]);
        assert_eq!(w.point([slider + 130.0, y(&w, 1)]), vec![key::DOWN, key::RIGHT]);
        assert_eq!(w.point([slider + 10.0, y(&w, 2)]), down(2, key::LEFT), "Fog thinner");
        assert_eq!(w.point([slider + 130.0, y(&w, 2)]), down(2, key::RIGHT), "Fog thicker");
        // Camera Distance at 11 of 8..50: its knob near the left end
        assert_eq!(w.point([slider + 30.0, y(&w, 4)]), down(4, key::RIGHT));
        assert_eq!(w.point([slider + 2.0, y(&w, 4)]), down(4, key::LEFT));
        // Camera Turn Speed at 100 of 50..300: its knob a fifth of the way
        assert_eq!(w.point([slider + 70.0, y(&w, 5)]), down(5, key::RIGHT));
        assert_eq!(w.point([slider + 2.0, y(&w, 5)]), down(5, key::LEFT));
        assert_eq!(w.point([frame + 110.0, y(&w, 6)]), down(6, key::RIGHT), "Screen Shake's value");
        assert_eq!(w.point([frame + 20.0, y(&w, 7)]), down(7, key::LEFT), "Controller Focus's value");
        assert!(w.point([frame + 110.0, y(&w, 8)]).is_empty(), "the legend's row: Capture Controller not shown yet");
        assert!(w.point([slider + 70.0, y(&w, 7) + 60.0]).is_empty(), "below the rows");
        // down to the last row: the page scrolls a row
        for _ in 0..8 {
            w.key(&vui, &mut r, &mut o, key::DOWN, 0, &mut cues, &mut ups);
        }
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!((w.row, w.ext_top, w.item), (8, 1, 19), "Capture Controller, a row scrolled");
        assert_eq!(w.point([frame + 110.0, y(&w, 7)]), vec![key::RIGHT], "Capture Controller's value, where the eighth row was");
        assert_eq!(w.point(label(&w, 0)), std::iter::repeat_n(key::UP, 7).collect::<Vec<_>>(), "the top row shown is Brightness");
        assert_eq!(w.point([slider + 130.0, y(&w, 1)]), std::iter::repeat_n(key::UP, 6).chain([key::RIGHT]).collect::<Vec<_>>(), "Fog's slider a row up");
    }

}
