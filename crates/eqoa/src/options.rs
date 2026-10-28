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
//! The client's own, beside the game's: an Extended row between Sound and
//! Save Changes (the menu a row longer), its words the client's (no
//! WORDS.TXT entry). It opens a menu of its own over the options menu, drawn
//! as that one is: Video, World, UI, Controller, Camera, Characters and
//! Sound (`CATEGORIES`), each of which opens a page laid out and driven as
//! the game's pages are, short enough that none scrolls (`PAGE_MOST`):
//! - Video: Display (natively: Window or Full Screen, the window's own as
//!   F11 and Alt+Enter toggle it, `display`: it changes at once and stays,
//!   kept with the window's place, and Triangle leaves it), Brightness (a
//!   slider, `settings::curve`), Frame Rate (30, 60 or the display's,
//!   `settings::frame_due`), VSync (natively: a frame shown only at the
//!   display's refresh, or at once; `settings::vsync`, the surface's present
//!   mode in `viewer`), Render Scale (x1, the game's picture at the window's
//!   size; x2 or x4: the whole frame drawn that many times larger each way
//!   and averaged down, `render::scale`, at less where the window is too
//!   large for it, which beside Back it then says) and Loading Screens
//!   (Longer: a long trip's loading screen up at least a second, as a PS2's
//!   disc read keeps it; Instant: down as soon as the place is in,
//!   `loading::Screen::hold_far`);
//! - World (the user's, 2026-10-25): Fog (a slider: the game's distance fog
//!   and far plane in the middle, off at the left, thicker to the right,
//!   `fog`), Mipmaps (On, the game's: the textures' mip levels sampled by
//!   depth as the GS samples them, `render::gs_lod_k`; Off: the base level
//!   alone, `render::set_mipmaps`), Flora (On, the game's: the plants it
//!   grows round the camera, `flora`; Off: none), Environment (On, the
//!   game's: the effect volumes, ground fog, dust, leaves, light shafts and
//!   flocks, `fx`; Off: none), Shadows (On, the game's: the characters' blob
//!   shadows, `shadows`; Off: none) and Art (Frontiers' or the 2002 disc's, `era`,
//!   as F9 swaps it: drawn as it moves, the 2002 disc asked for the first
//!   time);
//! - UI: Ability Bars (the game's 2, or 3 or 4: the HUD's quick menus and
//!   the spell book's slots, `hudmenu`), Update Notices (one setting for
//!   every window: off, the game never asks to restart into a newer build,
//!   which starts the next time the game does; `update::asks`, and a page's
//!   Reload notice, `web/main.js`), Text (Sharp or Smooth: the game's font
//!   scaled to the window with even strokes, or filtered as the PS2 filters
//!   it, `render::ui::TextFilter`) and HUD (Nearest, the game's and the
//!   default; or Sharp: the 2D layer's other textures scaled with every texel
//!   as wide as every other, as Text's Sharp draws a font, at a cost,
//!   `render::ui::HudFilter`);
//! - Controller: Controller Focus (which controllers this window reads:
//!   Auto, Window, Pad 1 to 4 or Off, `input::pick`, kept for each window of
//!   the client open, `multibox`; what it reads now is said beside Back) and
//!   Capture Controller (one setting for every window);
//! - Camera: Camera Distance (a slider: the follow orbit's zoom-out ceiling,
//!   8 to 50, `camstyle`), Camera Turn Speed (a slider: how fast the stick
//!   and the keys turn the view, half to three times the game's,
//!   `camstyle::turn_speed`) and Screen Shake;
//! - Characters: Level of Detail (On: the game's levels of an LOD body; Off:
//!   its near level out to its off distance, `lod.rs`), NPC/PC Display (a
//!   slider and its number: the NPCs and players kept in view, the game's 24
//!   to 204 in eights, on a server that offers more) and NPC/PC View
//!   Distance (a slider and its number: how far they are drawn, the game's
//!   100 to 1000 in fifties, `lod::view_dist`, and looked for on a server
//!   that offers it; both told the server by `Play::agree_entity_view`, and
//!   in the world the legend says what this server gives while either is
//!   chosen), Name Distance (a slider and its number: how far from where
//!   the game's camera could be a name plate is drawn, the game's 22, then
//!   30 to 200 in tens, `play::PlateView`) and Overlapping Names (Hide, the
//!   game's: a name whose plate meets a nearer one's is not drawn; Show,
//!   every one, the nearest over the rest: `play::labels`);
//! - Sound: Sound in Background (one setting for every window: off, a window
//!   not in front is not heard, `audio::set_heard`) and natively Output
//!   Device (Default, the system's, followed when it changes, or a device
//!   the system lists, the default while it is not connected: `audio`'s
//!   `retarget`; its page says where the sound plays now).
//! Triangle goes back a step: a page to the categories, the categories to
//! the options menu. They change as they are moved, Triangle on the options
//! menu puts them back, and Save Changes keeps them with the game's
//! (`client.opt`).
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
use crate::settings::{BRIGHTNESS_STEPS, CAMERA_MAX, CAMERA_MIN, CAMERA_STEP, ClientOptions, ENTITY_SLOTS_GAME, ENTITY_SLOTS_MAX, ENTITY_SLOTS_STEP, ENTITY_VIEW_GAME, ENTITY_VIEW_MAX, ENTITY_VIEW_STEP, Focus, FrameRate, NAME_DISTANCE_GAME, NAME_DISTANCE_MAX, TURN_MAX, TURN_MIN, TURN_STEP};
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
    /// Extended's: the category whose page is open (`CATEGORIES`), none while
    /// its menu of categories has the keys; and that menu's row
    cat: Option<usize>,
    cat_sel: i32,
    /// In the world, what this server gives (`Play::agree_entity_view` sets
    /// it), the Characters page's legend's while NPC/PC Display or View
    /// Distance is chosen; none on the title
    pub server_note: Option<String>,
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
    /// Extended's menu of categories, drawn as the options menu is, over it
    window4: Window,
    /// the preview last asked for
    preview: Option<i32>,
    /// what was drawn where, for a pointer: the menu's rows, or Extended's
    /// categories (their index), a page's labels (`LABEL` + row), the left
    /// and right of its values (`LABEL` + `HIT_ROWS` + row, `LABEL` + 2
    /// `HIT_ROWS` + row); and when
    hits: Hits,
    now: i64,
}

impl WndOptions {
    /// A pointer at frame point `p` (`pointer`): the box's choices; a menu
    /// row (the options menu's, or Extended's categories) chosen at once
    /// (✕); a page's row reached with up and down, its label then its value
    /// stepped on (right), left or right of a value's middle (a slider's
    /// knob) left or right. Nothing while saving. The keys to press.
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
        let k = h % HIT_ROWS;
        if h < LABEL {
            // a menu: `Prev`/`Next` start the tick's pulse over
            if self.page == 3 {
                self.cat_sel = k;
            } else {
                self.sel = k;
            }
            self.window.reset_tick_pulse(self.now);
            return vec![key::CROSS];
        }
        let mut keys = crate::pointer::steps(self.row, k, key::UP, key::DOWN);
        match h / HIT_ROWS {
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
        self.cat = None;
        self.cat_sel = 0;
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

    /// The row up and down move: the options menu's, Extended's
    /// categories', or a page's.
    fn at(&mut self) -> &mut i32 {
        match (self.page, self.cat) {
            (-1, _) => &mut self.sel,
            (3, None) => &mut self.cat_sel,
            _ => &mut self.row,
        }
    }

    /// `Prev`.
    fn prev(&mut self, now: i64, cues: &mut Vec<i32>) {
        let at = self.at();
        if *at > 0 {
            *at -= 1;
            self.window.reset_tick_pulse(now);
            cues.push(11);
        } else {
            cues.push(9);
        }
    }

    /// `Next`: the last row of each page; Interface gains Vibration; the
    /// menu's Extended and Controls are the client's, and so are Extended's
    /// categories and their pages.
    fn next(&mut self, now: i64, cues: &mut Vec<i32>) {
        let last = match (self.page, self.cat) {
            (-1, _) => 5,
            (3, None) => CATEGORIES.len() as i32 - 1,
            (3, Some(c)) => CATEGORIES[c].items.len() as i32 - 1,
            (page, _) => {
                let mut last = [2, 3, 3];
                if FEATURES & 0x100 != 0 {
                    last[1] = 4;
                }
                last[page as usize]
            }
        };
        let at = self.at();
        if *at < last {
            *at += 1;
            self.window.reset_tick_pulse(now);
            cues.push(11);
        } else {
            cues.push(9);
        }
    }

    /// `Decrement` (`d` -1) and `Increment` (1): the item's value; a volume
    /// stops at its end with sound 9. Items from 12 on are the client's
    /// Extended rows (`EXTENDED_ITEMS`): Level of Detail, Screen Shake (18),
    /// Capture Controller (19) and Sound in Background (21) turn over,
    /// Brightness (13), Fog (20), Camera Distance (15) and Camera Turn Speed
    /// (17) step as a volume does (stopping at their ends with sound 9),
    /// Frame Rate (14) goes round 30, 60 and the display's, Controller Focus
    /// (16) round its seven and Ability Bars (22) round 2, 3 and 4 as Battle
    /// Music goes round its tracks; NPC/PC Display (23) steps by eights from
    /// the game's 24 as a slider does, 200 to 204 its last, and NPC/PC View
    /// Distance (24) by fifties from the game's 100 to 1000; Art (25) and
    /// VSync (26) turn over, VSync natively only (a page's stays on with
    /// sound 9: the browser paces it); Name Distance (27) by tens from the
    /// game's 22 to 200 (`step_name_distance`); Text (30) goes between Sharp and
    /// Smooth; Update Notices (29), Overlapping Names (34, Hide and Show),
    /// Mipmaps (35) and Loading Screens (38, Longer and Instant) turn over;
    /// Output Device (32) goes round the system default and the devices
    /// listed (`step_output`); Display (33) turns the window over (`display`). A
    /// slider (`slider_at`) makes its own
    /// sound, as a volume does; every other row sound 11.
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
            item if extended(item) => {
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
                    21 => c.background_sound = !c.background_sound,
                    // Art (`era`): drawn as it moves, the window's (`viewer`
                    // turns the library's view when the setting does)
                    25 => c.art = crate::era::other(c.art),
                    26 if cfg!(target_arch = "wasm32") => cues.push(9),
                    26 => c.vsync = !c.vsync,
                    // Update Notices (`update::asks`)
                    29 => c.update_notices = !c.update_notices,
                    // Overlapping Names: the plates kept from the next
                    // frame (`play::labels`)
                    34 => c.overlapping_names = !c.overlapping_names,
                    // Text: every font drawn from the next frame (`viewer`)
                    30 => c.text = c.text.other(),
                    // Loading Screens: the next long trip's (`loading`)
                    38 => c.loading_longer = !c.loading_longer,
                    // Render Scale: the frame from the next one (`viewer`,
                    // `render::scale`), round 1, 2 and 4
                    39 => {
                        let all = crate::settings::RENDER_SCALES;
                        let at = all.iter().position(|&k| k == c.render_scale).unwrap_or(0);
                        c.render_scale = all[(at + if d < 0 { all.len() - 1 } else { 1 }) % all.len()];
                    }
                    // HUD: every texture of the 2D layer but the fonts from
                    // the next frame (`viewer`, `render::ui::set_hud_filter`)
                    40 => c.hud = c.hud.other(),
                    // Mipmaps: every texture's levels from the next frame
                    // (`viewer`, `render::set_mipmaps`)
                    35 => c.mipmaps = !c.mipmaps,
                    // Flora: grown or let go from the next frame (`viewer`, `flora`)
                    36 => c.flora = !c.flora,
                    // Environment: the effect volumes from the next frame (`viewer`, `fx`)
                    37 => c.environment = !c.environment,
                    // Shadows: drawn or not from the next frame (`viewer`, `shadows`)
                    46 => c.shadows = !c.shadows,
                    // Output Device: the sound opened there from the next
                    // frame (`audio::retarget`)
                    32 => step_output(&mut c, &crate::audio::outputs().listed.map(|l| l.1).unwrap_or_default(), d),
                    // Display: the window's own, carried out on the next
                    // frame (`display`, `viewer`)
                    33 => crate::display::ask(!crate::display::full()),
                    22 => {
                        let (lo, n) = (crate::settings::ABILITY_BARS_GAME, crate::hudmenu::MAX_BARS as u8 - crate::settings::ABILITY_BARS_GAME + 1);
                        c.ability_bars = lo + (c.ability_bars.saturating_sub(lo).min(n - 1) + if d < 0 { n - 1 } else { 1 }) % n;
                    }
                    13 => slide(&mut c.brightness, 0, BRIGHTNESS_STEPS, 1),
                    23 => {
                        // on to the next of 24, 32 .. 200, 204 (a number
                        // between them, one client.opt held, to the one past it)
                        let (lo, hi, by) = (ENTITY_SLOTS_GAME as i32, ENTITY_SLOTS_MAX as i32, ENTITY_SLOTS_STEP as i32);
                        let v = c.entity_slots as i32;
                        if (d < 0 && v > lo) || (d > 0 && v < hi) {
                            let off = v - lo;
                            let to = if d > 0 { lo + (off / by + 1) * by } else { lo + (off - 1).div_euclid(by) * by };
                            c.entity_slots = to.clamp(lo, hi) as u8;
                            cues.push(11);
                        } else {
                            cues.push(9);
                        }
                    }
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
                    27 => step_name_distance(&mut c, d, cues),
                    24 => {
                        let v = &mut c.entity_view;
                        if (d < 0 && *v > ENTITY_VIEW_GAME) || (d > 0 && *v < ENTITY_VIEW_MAX) {
                            *v = (*v as i32 + d.signum() * ENTITY_VIEW_STEP as i32).clamp(ENTITY_VIEW_GAME as i32, ENTITY_VIEW_MAX as i32) as u16;
                            cues.push(11);
                        } else {
                            cues.push(9);
                        }
                    }
                    14 => {
                        let all = FrameRate::ALL;
                        let at = all.iter().position(|&r| r == crate::settings::frame_rate()).unwrap_or(0);
                        c.frame_rate = Some(all[(at + if d < 0 { all.len() - 1 } else { 1 }) % all.len()]);
                    }
                    _ => {}
                }
                crate::settings::set_client(c);
            }
            _ => {}
        }
        let inert = self.item == 26 && cfg!(target_arch = "wasm32");
        let slides = matches!(self.item, 3 | 4) || slider_at(self.item).is_some();
        if !slides && !inert && ((0..=11).contains(&self.item) || extended(self.item)) {
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
    /// eight rows, an empty title; the client's Controls 600 wide over the
    /// middle of the frame, as many rows as fit), the client's Extended its
    /// menu of categories (a window as the menu's, titled Extended, over it)
    /// and X there a category's page (`page_window`), or on Save Changes
    /// asks (true).
    fn select(&mut self, vui: &Vui, r: &dyn Raster, cues: &mut Vec<i32>) -> bool {
        if self.page == 3 && self.cat.is_none() {
            cues.push(8);
            let c = self.cat_sel.clamp(0, CATEGORIES.len() as i32 - 1) as usize;
            self.cat = Some(c);
            self.row = 0;
            self.page_window(vui, r, CATEGORIES[c].items.len() as i32 + 3);
            return false;
        }
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
            3 => {
                self.page = 3;
                self.cat = None;
                self.cat_sel = 0;
                // a window as the menu's (`open`), as tall as its categories
                // and their legend (`CATEGORY_ROWS`), its bottom where the
                // menu's is: the rows past the menu's nine go up, clear of
                // the title's own menu under it
                self.window4.close(true);
                self.window4.init(vui, r, self.window.center, 300, CATEGORY_ROWS, 0);
                self.window4.open(false);
                self.window4.set_title(Some(&wide(EXTENDED)));
                let bottom = |w: &Window| w.top_left()[1] + w.size[1];
                self.window4.center[1] -= bottom(&self.window4) - bottom(&self.window);
                return false;
            }
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
        self.page_window(vui, r, rows);
        false
    }

    /// A page's window, 440 wide and `rows` high, an empty title bar, over
    /// the menu's middle. One of more rows than the game's pages (eight: a
    /// category of `PAGE_MOST`) goes without their empty title bar (six
    /// pixels each way) and its top where theirs is, so the options window's
    /// title tab above it still shows and its bottom stays clear of the
    /// title's menu under it.
    fn page_window(&mut self, vui: &Vui, r: &dyn Raster, rows: i32) {
        self.window2.close(true);
        self.window2.init(vui, r, self.window.center, 440, rows, 0);
        self.window2.open(false);
        if rows > 8 {
            self.window2.set_title(None);
            let mut eight = Window::default();
            eight.init(vui, r, self.window.center, 440, 8, 0);
            eight.set_title(Some(&[]));
            self.window2.center[1] += eight.top_left()[1] - self.window2.top_left()[1];
        } else {
            self.window2.set_title(Some(&[]));
        }
    }

    /// `Back`: Triangle on a page goes back to the menu (on a category's,
    /// to Extended's categories, and from there to the menu); on the menu,
    /// true to close.
    fn back(&mut self, cues: &mut Vec<i32>) -> bool {
        cues.push(8);
        if self.page == 3 && self.cat.is_some() {
            self.cat = None;
            return false;
        }
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
        // Extended's categories stand in the menu's place (its window, where
        // it is): the menu under them would show its words through theirs
        if self.page == 3 {
            self.item = -1;
        } else {
            self.draw_menu(vui, r);
        }
        match self.page {
            0 => self.draw_game(vui, r, o),
            1 => self.draw_interface(vui, r, o),
            2 => self.draw_sound(vui, r, o),
            3 => {
                self.draw_categories(vui, r);
                if let Some(c) = self.cat {
                    self.draw_category(vui, r, c);
                }
            }
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
    /// the caller. The pages start a row down.
    fn draw_rows(&mut self, vui: &Vui, r: &mut dyn Raster, labels: &[Vec<u16>], values: &[Option<Vec<u16>>]) {
        self.window2.draw(vui, r);
        let width = self.window2.size[0];
        for (k, label) in labels.iter().enumerate() {
            let c = UI_COLORS[(self.row == k as i32) as usize];
            self.window2.text_box(vui, r, label, 40, k as i32 + 1, width - 80, 1, c, 0);
            self.hits.push(rows_rect(&self.window2, vui, r, 40, k as i32 + 1, width - 176 - 48, 1), LABEL + k as i32);
        }
        if (0..labels.len() as i32).contains(&self.row) {
            self.window2.tick_r(vui, r, 36, self.row + 1, true);
        }
        for (k, v) in values.iter().enumerate() {
            let Some(v) = v else { continue };
            let row = k as i32 + 1;
            self.window2.draw_frame(vui, r, width - 176, row, 132, 1, if self.row == k as i32 { 96 } else { 64 });
            self.window2.text_box(vui, r, v, width - 176, row, 132, 1, UI_COLORS[0], 8 | 2);
            // the frame's two halves (its side arrows are glyphs)
            let at = self.window2.top_left()[0] + width - 176;
            self.halves(rows_rect(&self.window2, vui, r, width - 176, row, 132, 1), at - 4, at + 66, at + 136, k as i32);
        }
    }

    /// A value's left and right, `x0..mid` and `mid..x1` over the rows of
    /// `rr`, as row `k`'s (for a pointer).
    fn halves(&mut self, rr: [i32; 4], x0: i32, mid: i32, x1: i32, k: i32) {
        self.hits.push([x0, rr[1], mid, rr[3]], LABEL + HIT_ROWS + k);
        self.hits.push([mid, rr[1], x1, rr[3]], LABEL + 2 * HIT_ROWS + k);
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

    /// Extended's menu: its categories, the selected one lit, and the
    /// legend, laid out as the options menu is (`draw_menu`: a row down, the
    /// legend two under the last) in a window as its (`select`), over it; its
    /// rows answer a pointer only while it has the keys (no category's page
    /// open).
    fn draw_categories(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.window4.draw(vui, r);
        let width = self.window4.size[0];
        for (k, c) in CATEGORIES.iter().enumerate() {
            let on = self.cat_sel == k as i32;
            let s = if on { format!("|q{}|w", c.name) } else { c.name.to_string() };
            self.window4.text_box(vui, r, &wide(&s), 40, k as i32 + 1, width - 80, 1, UI_COLORS[on as usize], 8);
            if self.cat.is_none() {
                self.hits.push(rows_rect(&self.window4, vui, r, 40, k as i32 + 1, width - 80, 1), k as i32);
            }
        }
        let w = |id: i32| vui.words.get(id, vui.language);
        let legend = wide(&format!("|x {}   |t {}", w(212), w(750)));
        self.window4.text(vui, r, &legend, 40, CATEGORY_LEGEND, UI_COLORS[0], 0);
    }

    /// A category's page (`CATEGORIES`), laid out as the game's pages are (a
    /// row down, the legend two under the last): each row's label, its value
    /// in its frame (`value_text`) or its slider (`slider_at`: each side of
    /// the knob for a pointer, and a number before it where it has one);
    /// under them the Back legend and, beside it, the category's note
    /// (`Note`).
    fn draw_category(&mut self, vui: &Vui, r: &mut dyn Raster, c: usize) {
        let cat = &CATEGORIES[c];
        self.item = cat.items.get(self.row as usize).copied().unwrap_or(self.item);
        // Output Device's round: the system's devices listed while it shows
        if cat.items.contains(&32) {
            crate::audio::want_listing();
        }
        let labels: Vec<Vec<u16>> = cat.items.iter().map(|&i| wide(label_of(i))).collect();
        let values: Vec<Option<Vec<u16>>> = cat.items.iter().map(|&i| value_text(vui, i)).collect();
        self.draw_rows(vui, r, &labels, &values);
        let width = self.window2.size[0];
        let knob = vui.entry(166).uv1[0];
        let at = self.window2.top_left()[0] + width - 180;
        // a number ends before its slider's left arrow, two units off it:
        // room after the longest label for NPC/PC Distance's 1000 (F3
        // 20261025-071119: six units off, it ran into the label)
        let end = width - 180 - (vui.entry(128).uv1[0] + 3) - 2;
        for (k, &item) in cat.items.iter().enumerate() {
            let Some((t, number)) = slider_at(item) else { continue };
            let k = k as i32;
            self.window2.draw_hscroll(vui, r, width - 180, k + 1, 140, t, if self.row == k { 32 } else { 0 });
            let mid = at + (t * (140 - knob) as f32) as i32 + knob / 2;
            self.halves(rows_rect(&self.window2, vui, r, width - 180, k + 1, 140, 1), at - 4, mid, at + 144, k);
            if let Some(n) = number {
                self.window2.text_box(vui, r, &wide(&n.to_string()), 40, k + 1, end - 40, 1, UI_COLORS[(self.row == k) as usize], 0x10);
            }
        }
        let legend = cat.items.len() as i32 + 2;
        self.back_legend(vui, r, legend);
        if let Some(note) = self.page_note(c) {
            let after = 40 + self.window2.text_width(vui, r, &wide(&format!("|t {}", vui.words.get(750, vui.language)))) + 16;
            self.window2.text_box(vui, r, &wide(&note), after, legend, width - 40 - after, 1, UI_COLORS[0], 0x10 | 2);
        }
    }

    /// What category `c`'s page says beside its Back legend (`Note`), its
    /// row being the one drawn last (`item`).
    fn page_note(&self, c: usize) -> Option<String> {
        match CATEGORIES[c].note {
            Note::Reads => Some(crate::input::reads()),
            Note::Server => self.server_note.clone().filter(|_| matches!(self.item, 23 | 24)),
            Note::Scale => {
                let asked = crate::settings::client().render_scale as u32;
                scale_note(asked, render::scale::would_draw(asked)).filter(|_| self.item == 39)
            }
            Note::Output if cfg!(target_arch = "wasm32") => None,
            Note::Output => Some(match crate::audio::playing() {
                Some(d) => format!("Plays on {d}"),
                None => "No output now".to_string(),
            }),
            Note::None => None,
        }
    }
}

/// The client's words for its own pages (no WORDS.TXT entry): the menu's
/// rows, Extended's categories and its rows (`EXTENDED_ITEMS`). (WORDS.TXT's
/// "CONTROLS", 1039, is in capitals and used by none of the game's modules.)
const EXTENDED: &str = "Extended";
const CONTROLS: &str = "Controls";

/// Extended's rows, by the item each changes (`change`), and their labels.
/// A row is drawn by its item (`value_text`, or `slider_at` for a slider)
/// and shown on the page of the category that lists it (`CATEGORIES`).
const EXTENDED_ITEMS: [(i32, &str); 28] = [
    (12, "Level of Detail"),
    (13, "Brightness"),
    (14, "Frame Rate"),
    (15, "Camera Distance"),
    (16, "Controller Focus"),
    (17, "Camera Turn Speed"),
    (18, "Screen Shake"),
    (19, "Capture Controller"),
    (20, "Fog"),
    (21, "Sound in Background"),
    (22, "Ability Bars"),
    (23, "NPC/PC Display"),
    (24, "NPC/PC Distance"),
    (25, "Art"),
    (26, "VSync"),
    (27, "Name Distance"),
    (29, "Update Notices"),
    (30, "Text"),
    (32, "Output Device"),
    (33, "Display"),
    (34, "Overlapping Names"),
    (35, "Mipmaps"),
    (36, "Flora"),
    (37, "Environment"),
    (38, "Loading Screens"),
    (39, "Render Scale"),
    (40, "HUD"),
    (46, "Shadows"),
];

/// What a category's page says beside its Back legend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Note {
    None,
    /// what this window reads (`input::reads`)
    Reads,
    /// what this server gives (`WndOptions::server_note`), while NPC/PC
    /// Display or Distance is chosen
    Server,
    /// the scale the frame is drawn at, while Render Scale is chosen and the
    /// window is too large for the one asked (`render::scale::fit`)
    Scale,
    /// where the sound plays now (`audio::playing`), natively
    Output,
}

/// One of Extended's categories: its name, its rows (their items, in the
/// page's order) and its note.
struct Category {
    name: &'static str,
    items: &'static [i32],
    note: Note,
}

/// Extended's categories, in its menu's order (the user's: "Extended, then
/// Video, UI, Controller, etc."), each a page of `PAGE_MOST` rows at most.
const CATEGORIES: [Category; 7] = [
    Category { name: "Video", items: VIDEO, note: Note::Scale },
    Category { name: "World", items: &[20, 35, 36, 37, 46, 25], note: Note::None },
    Category { name: "UI", items: &[22, 29, 30, 40], note: Note::None },
    Category { name: "Controller", items: &[16, 19], note: Note::Reads },
    Category { name: "Camera", items: &[15, 17, 18], note: Note::None },
    Category { name: "Characters", items: &[12, 23, 24, 27, 34], note: Note::Server },
    Category { name: "Sound", items: SOUND, note: Note::Output },
];

/// Video's rows: natively Display first (F3 20261024-201507: full screen was
/// F11's alone); a page's full screen is the browser's. Fog and Mipmaps went
/// to World with Art (the user's, 2026-10-25); Loading Screens came from UI,
/// and Text went back to UI beside HUD when Render Scale came (2026-10-27).
#[cfg(not(target_arch = "wasm32"))]
const VIDEO: &[i32] = &[33, 13, 14, 26, 39, 38];
#[cfg(target_arch = "wasm32")]
const VIDEO: &[i32] = &[13, 14, 26, 39, 38];

/// Sound's rows: natively Output Device after Sound in Background; a page
/// plays where the browser does.
#[cfg(not(target_arch = "wasm32"))]
const SOUND: &[i32] = &[21, 32];
#[cfg(target_arch = "wasm32")]
const SOUND: &[i32] = &[21];

/// Extended's menu (`draw_categories`): the categories from its first row,
/// the legend two under the last, as the options menu's six and its legend
/// on its eighth row of nine (`draw_menu`), the window as tall as they are
/// (F3 20261026-135758: a category drawn on the menu's legend row).
const CATEGORY_LEGEND: i32 = CATEGORIES.len() as i32 + 2;
const CATEGORY_ROWS: i32 = CATEGORY_LEGEND + 1;

/// The most rows a category's page holds: six and the legend are as tall as
/// nine rows, the most the options window has room for (`page_window`), so
/// no page scrolls.
const PAGE_MOST: usize = 6;

/// Whether every category's page holds its rows (`PAGE_MOST`).
const fn fits(cats: &[Category]) -> bool {
    let mut i = 0;
    while i < cats.len() {
        if cats[i].items.len() > PAGE_MOST {
            return false;
        }
        i += 1;
    }
    true
}

const _: () = assert!(fits(&CATEGORIES), "a category's page would scroll: give its rows another category");

/// Whether `item` is one of Extended's rows.
fn extended(item: i32) -> bool {
    EXTENDED_ITEMS.iter().any(|&(i, _)| i == item)
}

/// An Extended row's label.
fn label_of(item: i32) -> &'static str {
    EXTENDED_ITEMS.iter().find(|&&(i, _)| i == item).map_or("", |&(_, l)| l)
}

/// An Extended row's value as its frame shows it; none for a slider
/// (`slider_at`).
fn value_text(vui: &Vui, item: i32) -> Option<Vec<u16>> {
    let c = crate::settings::client();
    let on_off = |on: bool| wide(vui.words.get(if on { 551 } else { 552 }, vui.language));
    Some(match item {
        12 => on_off(c.lod),
        14 => wide(match crate::settings::frame_rate() {
            FrameRate::Thirty => "30",
            FrameRate::Sixty => "60",
            FrameRate::Display => "Display",
        }),
        16 => wide(&match crate::settings::focus() {
            Focus::Off => vui.words.get(552, vui.language).to_string(),
            f => crate::input::focus_name(f),
        }),
        18 => on_off(c.screen_shake),
        19 => on_off(c.capture),
        21 => on_off(c.background_sound),
        22 => wide(&c.ability_bars.to_string()),
        25 => wide(match c.art {
            esf::Era::Frontiers => "Frontiers",
            esf::Era::Y2002 => "2002",
        }),
        26 => on_off(crate::settings::vsync()),
        29 => on_off(c.update_notices),
        34 => wide(if c.overlapping_names { "Show" } else { "Hide" }),
        35 => on_off(c.mipmaps),
        36 => on_off(c.flora),
        37 => on_off(c.environment),
        46 => on_off(c.shadows),
        38 => wide(if c.loading_longer { "Longer" } else { "Instant" }),
        39 => wide(&format!("x{}", c.render_scale)),
        40 => wide(match c.hud {
            render::ui::HudFilter::Nearest => "Nearest",
            render::ui::HudFilter::Sharp => "Sharp",
        }),
        30 => wide(match c.text {
            render::ui::TextFilter::Sharp => "Sharp",
            render::ui::TextFilter::Smooth => "Smooth",
        }),
        32 => wide(c.output.as_ref().map_or(SYSTEM_DEFAULT, |d| d.as_str())),
        33 => wide(if crate::display::full() { "Full Screen" } else { "Window" }),
        _ => return None,
    })
}

/// What Video's page says beside Back while Render Scale is chosen: the
/// scale the frame is drawn at, when that is less than the one asked (the
/// window too large for it, `render::scale::fit`).
fn scale_note(asked: u32, drawn: u32) -> Option<String> {
    (drawn < asked).then(|| format!("Drawn at x{drawn} at this size"))
}

/// Output Device's value for the system's default ("System Default" is too
/// wide for a value's frame; the page's note says the device it is).
const SYSTEM_DEFAULT: &str = "Default";

/// Output Device (item 32) moved a step: round the system's default and the
/// devices the system lists (`names`, the lister's last), the one chosen kept
/// in the round while it is not listed (not connected now), after them.
fn step_output(c: &mut ClientOptions, names: &[String], d: i32) {
    let mut all: Vec<Option<crate::settings::DeviceName>> = vec![None];
    all.extend(names.iter().map(|n| Some(crate::settings::DeviceName::new(n))));
    if let Some(chosen) = c.output.filter(|d| !names.iter().any(|n| d.same(n))) {
        all.push(Some(chosen));
    }
    let at = all.iter().position(|o| *o == c.output).unwrap_or(0);
    c.output = all[(at + if d < 0 { all.len() - 1 } else { 1 }) % all.len()];
}

/// An Extended row's slider, if it is one: where its knob is (0 to 1), and
/// the number shown before it where it has one.
fn slider_at(item: i32) -> Option<(f32, Option<u16>)> {
    let c = crate::settings::client();
    let at = |v: f32, lo: f32, hi: f32| (v - lo) / (hi - lo);
    Some(match item {
        13 => (c.brightness as f32 / BRIGHTNESS_STEPS as f32, None),
        20 => (c.fog as f32 / crate::fog::STEPS as f32, None),
        15 => (at(c.camera_distance as f32, CAMERA_MIN as f32, CAMERA_MAX as f32), None),
        17 => (at(c.camera_turn as f32, TURN_MIN as f32, TURN_MAX as f32), None),
        23 => (at(c.entity_slots as f32, ENTITY_SLOTS_GAME as f32, ENTITY_SLOTS_MAX as f32), Some(c.entity_slots as u16)),
        24 => (at(c.entity_view as f32, ENTITY_VIEW_GAME as f32, ENTITY_VIEW_MAX as f32), Some(c.entity_view)),
        27 => name_distance_slider(&c),
        _ => return None,
    })
}

/// Name Distance (item 27): its slider's knob, the game's 22 at the left
/// and 200 at the right, and the number before it.
fn name_distance_slider(c: &ClientOptions) -> (f32, Option<u16>) {
    ((c.name_distance - NAME_DISTANCE_GAME) as f32 / (NAME_DISTANCE_MAX - NAME_DISTANCE_GAME) as f32, Some(c.name_distance as u16))
}

/// Name Distance (item 27) moved a step: on by tens from the game's 22 to
/// 200 (`settings::step_name_distance`), sound 11, or sound 9 at either end.
fn step_name_distance(c: &mut ClientOptions, d: i32, cues: &mut Vec<i32>) {
    match crate::settings::step_name_distance(c.name_distance, d) {
        Some(v) => {
            c.name_distance = v;
            cues.push(11);
        }
        None => cues.push(9),
    }
}

/// A page's pointer notes (`point`): a menu row below `LABEL`, a row's label
/// at `LABEL` + its row, its value's left half `HIT_ROWS` past that and its
/// right half twice past; a page has fewer rows than `HIT_ROWS` (until
/// 2026-10-23 ten, which the Extended page's eleventh row, Ability Bars,
/// outgrew: its notes read as the first row's).
const HIT_ROWS: i32 = 100;
const LABEL: i32 = HIT_ROWS;

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

    /// Extended by the pad: down three rows on the menu and ✕ opens its
    /// categories (Video lit, left and right changing nothing there); ✕
    /// opens a category's page on its first row, down walks its rows and
    /// stops at its last with sound 9, Triangle goes back to the categories
    /// and from them to the menu. Each row's left and right as it was on the
    /// one page: Level of Detail turns over, Brightness, Fog, Camera Distance
    /// and Camera Turn Speed stop at their ends with sound 9, Frame Rate goes
    /// round from the platform's; Triangle on the menu puts them back; Save
    /// Changes keeps them, and the game's, where nothing was kept before (the
    /// control).
    #[test]
    fn the_extended_pages() {
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
        assert_eq!((w.sel, w.page, w.cat, w.cat_sel, w.item), (3, 3, None, 0, -1), "Extended's categories, Video lit");
        cues.clear();
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(cues.is_empty(), "left and right are nothing on the categories: {cues:?}");
        assert_eq!(settings::client(), settings::ClientOptions::default());
        // Video: Display, Brightness, Frame Rate, VSync, Render Scale, Loading Screens
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.page, w.cat, w.row, w.item), (3, Some(0), 0, 33), "Video, on Display");
        assert_eq!(cues.last(), Some(&8));
        crate::display::seen(false);
        assert_eq!(value_text(&vui, 33), Some(wide("Window")), "the window as it is");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((crate::display::full(), cues.last()), (true, Some(&11)), "full screen, shown at once");
        assert_eq!(value_text(&vui, 33), Some(wide("Full Screen")));
        assert_eq!(crate::display::take(), Some(true), "for the viewer to carry out");
        crate::display::seen(true);
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!((crate::display::full(), crate::display::take()), (false, Some(false)), "and back: a window");
        crate::display::seen(false);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 13, "Brightness");
        for _ in 0..6 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!(settings::client().brightness, 10);
        assert_eq!(&cues[cues.len() - 2..], &[11, 9], "the last step, then its end");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 14, "Frame Rate");
        assert_eq!(settings::frame_rate(), FrameRate::Display, "the platform's");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!(settings::client().frame_rate, Some(FrameRate::Thirty), "round to 30");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!(settings::client().frame_rate, Some(FrameRate::Sixty));
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 26, "VSync");
        assert!(settings::vsync(), "on: a frame only at the display's refresh");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::vsync(), cues.last()), (false, Some(&11)), "off");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::vsync(), "and back: two values");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 39, "Render Scale");
        assert_eq!((settings::client().render_scale, value_text(&vui, 39)), (1, Some(wide("x1"))), "x1, the game's picture as drawn");
        for want in [2, 4, 1] {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
            assert_eq!((settings::client().render_scale, cues.last()), (want, Some(&11)), "round 1, 2 and 4");
        }
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!((settings::client().render_scale, value_text(&vui, 39)), (4, Some(wide("x4"))), "and the other way");
        assert_eq!(w.page_note(0), None, "drawn at the scale asked: nothing said");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!(settings::client().render_scale, 1);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 38, "Loading Screens");
        assert!(settings::loading_longer(), "Longer, the default: a long trip's screen held");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::loading_longer(), cues.last()), (false, Some(&11)), "Instant");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::loading_longer(), "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (5, Some(&9)), "Video's last row (six, a page's most)");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        assert_eq!((w.page, w.cat, w.cat_sel, w.item), (3, None, 0, -1), "back to the categories, Video still lit");
        // World: Fog, Mipmaps, Flora, Environment, Shadows, Art
        press(&mut w, &mut o, key::DOWN, &mut cues);
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(1), 20), "World, Fog");
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
        assert_eq!(w.item, 35, "Mipmaps");
        assert!(settings::client().mipmaps, "on: the game's");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().mipmaps, cues.last()), (false, Some(&11)), "off");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::client().mipmaps, "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 36, "Flora");
        assert!(settings::client().flora, "on: the game's");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().flora, cues.last()), (false, Some(&11)), "off");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::client().flora, "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 37, "Environment");
        assert!(settings::client().environment, "on: the game's");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().environment, settings::client().flora, cues.last()), (false, true, Some(&11)), "off, Flora still on");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::client().environment, "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 46, "Shadows");
        assert!(settings::client().shadows, "on: the game's");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().shadows, settings::client().environment, settings::client().flora, cues.last()), (false, true, true, Some(&11)), "off, the others still on");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::client().shadows, "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 25, "Art, on World since 2026-10-25");
        assert_eq!(settings::art(), esf::Era::Frontiers, "Frontiers', the disc the game runs off");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::art(), cues.last()), (esf::Era::Y2002, Some(&11)), "the 2002 disc's");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!(settings::art(), esf::Era::Frontiers, "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (5, Some(&9)), "World's last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        assert_eq!((w.page, w.cat, w.cat_sel, w.item), (3, None, 1, -1), "back to the categories, World still lit");
        // UI: Ability Bars, Update Notices, Text, HUD
        press(&mut w, &mut o, key::DOWN, &mut cues);
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(2), 22), "UI, Ability Bars");
        assert_eq!(settings::ability_bars(), 2, "the game's two");
        for want in [3, 4, 2] {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
            assert_eq!(settings::ability_bars(), want, "round 2, 3 and 4");
        }
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!(settings::ability_bars(), 4, "and the other way");
        assert_eq!(cues.last(), Some(&11));
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 29, "Update Notices");
        assert!(settings::update_notices(), "on: the game asks to restart into a newer build");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::update_notices(), cues.last()), (false, Some(&11)), "off");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::update_notices(), "and back: two values");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 30, "Text, back on UI since Render Scale took its place on Video");
        assert_eq!(settings::client().text, render::ui::TextFilter::Sharp, "Sharp, the default");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().text, cues.last()), (render::ui::TextFilter::Smooth, Some(&11)), "Smooth");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!(settings::client().text, render::ui::TextFilter::Sharp, "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 40, "HUD");
        assert_eq!((settings::client().hud, value_text(&vui, 40)), (render::ui::HudFilter::Nearest, Some(wide("Nearest"))), "Nearest, the game's and the default");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().hud, value_text(&vui, 40), cues.last()), (render::ui::HudFilter::Sharp, Some(wide("Sharp")), Some(&11)), "Sharp");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!(settings::client().hud, render::ui::HudFilter::Nearest, "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (3, Some(&9)), "UI's last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        // Controller: Controller Focus, Capture Controller
        press(&mut w, &mut o, key::DOWN, &mut cues);
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(3), 16), "Controller, Controller Focus");
        assert_eq!(settings::focus(), Focus::Auto);
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!(settings::focus(), Focus::Off, "round to Off");
        for _ in 0..3 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!(settings::focus(), Focus::Pad(1), "Auto, Window, Pad 1");
        assert_eq!(settings::client().controller[0], Focus::Pad(1), "the first window's: this one, with nowhere to take a place");
        assert_eq!(cues.last(), Some(&11));
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 19, "Capture Controller");
        assert!(!settings::capture(), "off: every window on a pad reads it");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert!(settings::capture());
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        // Camera: Camera Distance, Camera Turn Speed, Screen Shake
        press(&mut w, &mut o, key::DOWN, &mut cues);
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(4), 15), "Camera, Camera Distance");
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
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        // Characters: Level of Detail, NPC/PC Display, NPC/PC Distance
        press(&mut w, &mut o, key::DOWN, &mut cues);
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(5), 12), "Characters, Level of Detail");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert!(!settings::client().lod);
        assert_eq!(cues.last(), Some(&11));
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 23, "NPC/PC Display");
        assert_eq!(settings::entity_slots(), 24, "the game's 24");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!((settings::entity_slots(), cues.last()), (24, Some(&9)), "no fewer than the game's");
        for _ in 0..13 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!(settings::entity_slots(), 128, "24 to 128 in 13 steps of 8");
        for _ in 0..10 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!(settings::entity_slots(), 204, "200, then the most a channel carries");
        assert_eq!(&cues[cues.len() - 2..], &[11, 11]);
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::entity_slots(), cues.last()), (204, Some(&9)), "its end");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!(settings::entity_slots(), 200, "back onto the eights");
        settings::set_client(settings::ClientOptions { entity_slots: 100, ..settings::client() });
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!(settings::entity_slots(), 96, "a number between, as client.opt may hold, to the eight below");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 24, "NPC/PC Distance");
        assert_eq!(settings::entity_view(), 100, "the game's 100");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!((settings::entity_view(), cues.last()), (100, Some(&9)), "no nearer than the game's");
        for _ in 0..10 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!(settings::entity_view(), 600, "100 to 600 in 10 steps of 50");
        for _ in 0..9 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!((settings::entity_view(), cues.last()), (1000, Some(&9)), "1000, then its end");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 27, "Name Distance");
        assert_eq!(settings::client().name_distance, 22, "the game's 22");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!((settings::client().name_distance, cues.last()), (22, Some(&9)), "no nearer than the game's");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().name_distance, cues.last()), (30, Some(&11)), "on to 30");
        for _ in 0..17 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!(settings::client().name_distance, 200, "30 to 200 in 17 steps of 10");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().name_distance, cues.last()), (200, Some(&9)), "its end");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 34, "Overlapping Names");
        assert!(!settings::overlapping_names(), "Hide: the game's");
        assert_eq!(value_text(&vui, 34), Some(wide("Hide")));
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::overlapping_names(), cues.last()), (true, Some(&11)), "Show");
        assert_eq!(value_text(&vui, 34), Some(wide("Show")));
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(!settings::overlapping_names(), "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (4, Some(&9)), "Characters' last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        // Sound: Sound in Background
        press(&mut w, &mut o, key::DOWN, &mut cues);
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(6), 21), "Sound, Sound in Background");
        assert!(settings::background_sound(), "on: a window behind others keeps its sound");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert!(!settings::background_sound());
        assert_eq!(cues.last(), Some(&11));
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 32, "Output Device");
        assert_eq!(value_text(&vui, 32), Some(wide(SYSTEM_DEFAULT)), "the system's default");
        assert!(w.page_note(6).is_some_and(|n| n.starts_with("Plays on ") || n == "No output now"), "where it plays, beside Back");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (1, Some(&9)), "Sound's last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.cat_sel, cues.last()), (6, Some(&9)), "Sound, the last category");
        // Triangle: the menu, then closed with every setting put back
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        assert_eq!((w.page, w.sel), (-1, 3), "the menu, on Extended");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        assert_eq!(settings::client(), settings::ClientOptions::default());
        assert_eq!(testing::kept("client.opt"), None, "nothing kept yet");
        // again, and saved this time, Auto Attack with it
        w.open(&vui, &NullRaster, &o);
        for k in [
            key::DOWN,
            key::CROSS,
            key::RIGHT,
            key::TRIANGLE,
            key::DOWN,
            key::DOWN,
            key::CROSS,
            // Video: Brightness a step down (past Display)
            key::CROSS,
            key::DOWN,
            key::LEFT,
            key::TRIANGLE,
            // World: Fog a step down
            key::DOWN,
            key::CROSS,
            key::LEFT,
            key::TRIANGLE,
            // Camera: Camera Turn Speed a step up
            key::DOWN,
            key::DOWN,
            key::DOWN,
            key::CROSS,
            key::DOWN,
            key::RIGHT,
            key::TRIANGLE,
            key::TRIANGLE,
            // Save Changes, Yes
            key::DOWN,
            key::DOWN,
            key::CROSS,
            key::LEFT,
            key::CROSS,
        ] {
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

    /// Video's note while Render Scale is chosen: the scale the frame is
    /// drawn at only when it is less than the one asked; the control, drawn
    /// as asked (or nothing asked), says nothing.
    #[test]
    fn render_scale_says_when_it_is_drawn_at_less() {
        assert_eq!(scale_note(4, 3), Some("Drawn at x3 at this size".to_string()));
        assert_eq!(scale_note(2, 1), Some("Drawn at x1 at this size".to_string()));
        assert_eq!(scale_note(4, 4), None);
        assert_eq!(scale_note(1, 1), None);
    }

    /// Output Device goes round the system default and the devices the
    /// system lists, either way; a device chosen but not listed now (not
    /// connected) stays in the round while it is the choice, after them, so
    /// a step off it either way lands on a neighbour (the control: a round of
    /// the listed alone would find no place for it and start from the
    /// default).
    #[test]
    fn output_device_goes_round_the_listed_devices() {
        use crate::settings::DeviceName;
        let (sp, tv) = ("Speakers (Realtek(R) Audio)", "LG TV SSCR2 (NVIDIA High Definition Audio)");
        let names = vec![sp.to_string(), tv.to_string()];
        let mut c = ClientOptions::default();
        let name = |c: &ClientOptions| c.output.map(|d| d.as_str().to_string());
        step_output(&mut c, &names, 1);
        assert_eq!(name(&c).as_deref(), Some(sp));
        step_output(&mut c, &names, 1);
        assert_eq!(name(&c).as_deref(), Some(tv));
        step_output(&mut c, &names, 1);
        assert_eq!(c.output, None, "round to the system default");
        step_output(&mut c, &names, -1);
        assert_eq!(name(&c).as_deref(), Some(tv), "and the other way");
        // a device chosen and not connected now: after the listed ones
        let h = "Headphones (2- Arctis 7 Game)";
        c.output = Some(DeviceName::new(h));
        step_output(&mut c, &names, -1);
        assert_eq!(name(&c).as_deref(), Some(tv), "left of it, the last listed");
        c.output = Some(DeviceName::new(h));
        step_output(&mut c, &names, 1);
        assert_eq!(c.output, None, "right of it, round to the start");
        // nothing listed (a page, or the system says nothing): the default
        let mut d = ClientOptions::default();
        step_output(&mut d, &[], 1);
        assert_eq!(d.output, None);
    }

    /// Every category's page opens on its first row, its window its rows and
    /// three high (the game's pages': a row down, the legend two under the
    /// last), and down walks the rows in the table's order; Triangle goes
    /// back a step at a time: the categories with that one still lit, then
    /// the menu on Extended, then closed. (Planted: a Back from a page
    /// straight to the menu, or a page opening on the category before.)
    #[test]
    fn each_category_opens_its_rows_and_back_goes_up_a_step() {
        use crate::pointer::testing::{NullRaster, vui};
        crate::settings::testing::reset(false);
        let vui = vui();
        let mut r = NullRaster;
        let mut o = GameOptions::default();
        let mut w = WndOptions::default();
        let (mut cues, mut ups) = (Vec::new(), Vec::new());
        w.open(&vui, &r, &o);
        w.sel = 3;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        // the menu's window holds every category and, a row clear of the
        // last, the legend (F3 20261026-135758: the eighth on the legend's row)
        assert_eq!(w.window4.rows, CATEGORIES.len() as i32 + 3, "the categories' window");
        assert!(CATEGORY_LEGEND > CATEGORIES.len() as i32 + 1 && CATEGORY_LEGEND < w.window4.rows, "the legend under the last category, in the window");
        let bottom = |w: &Window| w.top_left()[1] + w.size[1];
        assert_eq!(bottom(&w.window4), bottom(&w.window), "its bottom where the menu's is (the title's menu under it in view)");
        assert!(w.window4.top_left()[1] < w.window.top_left()[1] && w.window4.top_left()[1] > 24, "grown upward, its title tab on the picture");
        for (k, c) in CATEGORIES.iter().enumerate() {
            w.draw(&vui, &mut r, &o, &mut ups);
            assert_eq!((w.page, w.cat, w.cat_sel), (3, None, k as i32), "{}: on the categories", c.name);
            w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
            assert_eq!((w.cat, w.row, w.window2.rows), (Some(k), 0, c.items.len() as i32 + 3), "{}: its page", c.name);
            let mut seen = Vec::new();
            for _ in 0..c.items.len() {
                w.draw(&vui, &mut r, &o, &mut ups);
                seen.push(w.item);
                w.key(&vui, &mut r, &mut o, key::DOWN, 0, &mut cues, &mut ups);
            }
            assert_eq!(seen, c.items, "{}: its rows in order", c.name);
            assert_eq!((w.row, cues.last()), (c.items.len() as i32 - 1, Some(&9)), "{}: stopped at its last", c.name);
            w.key(&vui, &mut r, &mut o, key::TRIANGLE, 0, &mut cues, &mut ups);
            assert_eq!((w.page, w.cat, w.cat_sel), (3, None, k as i32), "{}: back to the categories, it still lit", c.name);
            w.key(&vui, &mut r, &mut o, key::DOWN, 0, &mut cues, &mut ups);
        }
        assert_eq!(cues.last(), Some(&9), "no category past Sound");
        assert!(!w.back(&mut cues), "the categories go back to the menu");
        assert_eq!((w.page, w.sel), (-1, 3));
        assert!(w.back(&mut cues), "and the menu closes");
    }

    /// Extended's rows are its categories' between them, every one on one
    /// page and none twice, each with its label, no page more than
    /// `PAGE_MOST`; the controls: a table missing a row, and a page of
    /// seven, fail.
    #[test]
    fn every_row_on_one_page_and_no_page_scrolls() {
        let listed = |cats: &[Category]| {
            let mut v: Vec<i32> = cats.iter().flat_map(|c| c.items.iter().copied()).collect();
            v.sort();
            v
        };
        let all: Vec<i32> = EXTENDED_ITEMS.iter().map(|&(i, _)| i).collect();
        assert_eq!(listed(&CATEGORIES), all);
        assert!(fits(&CATEGORIES));
        assert!(EXTENDED_ITEMS.iter().all(|&(i, l)| !l.is_empty() && label_of(i) == l));
        let short = [Category { name: "Video", items: &[13, 20], note: Note::None }];
        assert_ne!(listed(&short), all, "the control: rows left out");
        let seven = [Category { name: "Video", items: &[13, 20, 14, 26, 25, 12, 15], note: Note::None }];
        assert!(!fits(&seven), "the control: a page that would scroll");
    }

    /// A category's page of `PAGE_MOST` rows, a row more than the game's
    /// eight-row pages, keeps its top where theirs is (the Interface page's:
    /// under the options window's title tab) and, without their empty title
    /// bar, ends less than a row lower; the control: centred as the game's
    /// are, its top was half a row higher, over the tab.
    #[test]
    fn a_full_page_keeps_the_title_tab_in_view() {
        use crate::pointer::testing::{NullRaster, vui};
        crate::settings::testing::reset(false);
        let vui = vui();
        let mut r = NullRaster;
        let mut o = GameOptions::default();
        let mut w = WndOptions::default();
        let (mut cues, mut ups) = (Vec::new(), Vec::new());
        w.open(&vui, &r, &o);
        w.sel = 1;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        let (top8, h8) = (w.window2.top_left()[1], w.window2.size[1]);
        assert_eq!(w.window2.rows, 8, "Interface: eight rows (Vibration)");
        w.page_window(&vui, &r, PAGE_MOST as i32 + 3);
        let (top9, h9) = (w.window2.top_left()[1], w.window2.size[1]);
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

    /// A pointer on Extended: a category's row is chosen at once (✕) and
    /// opens its page; while a page is open the categories behind it answer
    /// nothing. On a page a label is reached by up and down (the selected
    /// one's steps it on), a value's halves are left and right, each
    /// slider's sides of its knob are; nothing where no row was drawn.
    #[test]
    fn a_pointer_on_the_extended_pages() {
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
        let category = |w: &WndOptions, k: i32| mid(rows_rect(&w.window4, &vui, &NullRaster, 40, k + 1, 200, 1));
        let label = |w: &WndOptions, k: i32| mid(rows_rect(&w.window2, &vui, &NullRaster, 40, k + 1, 100, 1));
        let y = |w: &WndOptions, k: i32| w.window2.row_center(&vui, &NullRaster, k + 1)[1] as f32;
        let down = |n: usize, last: i32| std::iter::repeat_n(key::DOWN, n).chain([last]).collect::<Vec<_>>();
        // the categories: Camera chosen at once
        assert_eq!(w.point(category(&w, 4)), vec![key::CROSS]);
        assert_eq!(w.cat_sel, 4, "Camera");
        assert_eq!(w.point(category(&w, 6)), vec![key::CROSS]);
        assert_eq!(w.cat_sel, 6, "Sound");
        assert!(w.point([5.0, 5.0]).is_empty());
        w.point(category(&w, 4));
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.cat, Some(4), "Camera's page");
        for k in 0..CATEGORIES.len() as i32 {
            assert!(w.hits.at(category(&w, k)).is_none_or(|h| h >= LABEL), "category {k} behind the page answers nothing");
        }
        let width = w.window2.size[0];
        let left = w.window2.top_left()[0] as f32;
        let frame = left + (width - 176) as f32;
        let slider = left + (width - 180) as f32;
        assert_eq!(w.point(label(&w, 2)), vec![key::DOWN, key::DOWN], "Screen Shake's label");
        assert_eq!(w.point(label(&w, 0)), vec![key::RIGHT], "Camera Distance's, selected: stepped on");
        // Camera Distance at 11 of 8..50: its knob near the left end
        assert_eq!(w.point([slider + 30.0, y(&w, 0)]), vec![key::RIGHT]);
        assert_eq!(w.point([slider + 2.0, y(&w, 0)]), vec![key::LEFT]);
        // Camera Turn Speed at 100 of 50..300: its knob a fifth of the way
        assert_eq!(w.point([slider + 70.0, y(&w, 1)]), down(1, key::RIGHT));
        assert_eq!(w.point([slider + 2.0, y(&w, 1)]), down(1, key::LEFT));
        assert_eq!(w.point([frame + 110.0, y(&w, 2)]), down(2, key::RIGHT), "Screen Shake's value");
        assert_eq!(w.point([frame + 20.0, y(&w, 2)]), down(2, key::LEFT));
        assert!(w.point([frame + 110.0, y(&w, 3)]).is_empty(), "the row under the last: nothing");
        assert!(w.point([slider + 70.0, y(&w, 4)]).is_empty(), "the legend's row: nothing");
        // Video: Display's value, Brightness at 5 of 10 (its knob mid-bar),
        // Frame Rate's value
        w.key(&vui, &mut r, &mut o, key::TRIANGLE, 0, &mut cues, &mut ups);
        w.cat_sel = 0;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.cat, Some(0), "Video's page");
        assert_eq!(w.point([frame + 20.0, y(&w, 0)]), vec![key::LEFT], "Display's value, left");
        assert_eq!(w.point([slider + 10.0, y(&w, 1)]), down(1, key::LEFT), "Brightness down");
        assert_eq!(w.point([slider + 130.0, y(&w, 1)]), down(1, key::RIGHT), "Brightness up");
        assert_eq!(w.point([frame + 110.0, y(&w, 2)]), down(2, key::RIGHT), "Frame Rate's value");
        assert_eq!(w.point([frame + 110.0, y(&w, 4)]), down(4, key::RIGHT), "Loading Screens' value, under VSync");
        assert_eq!(w.point([frame + 110.0, y(&w, 5)]), down(5, key::RIGHT), "Text's value, under Loading Screens");
        assert!(w.point([frame + 110.0, y(&w, 6)]).is_empty(), "the row under Text: nothing");
        // World: Fog at 5 of 10 (its knob mid-bar), Flora's and Art's values
        w.key(&vui, &mut r, &mut o, key::TRIANGLE, 0, &mut cues, &mut ups);
        w.cat_sel = 1;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.cat, Some(1), "World's page");
        assert_eq!(w.point([slider + 10.0, y(&w, 0)]), vec![key::LEFT], "Fog thinner");
        assert_eq!(w.point([slider + 130.0, y(&w, 0)]), vec![key::RIGHT], "Fog thicker");
        assert_eq!(w.point([frame + 110.0, y(&w, 2)]), down(2, key::RIGHT), "Flora's value");
        assert_eq!(w.point([frame + 20.0, y(&w, 3)]), down(3, key::LEFT), "Environment's value");
        assert_eq!(w.point([frame + 20.0, y(&w, 4)]), down(4, key::LEFT), "Art's value");
        // Characters: NPC/PC Display's knob at its left end (the game's 24),
        // then at 128 near its middle
        w.key(&vui, &mut r, &mut o, key::TRIANGLE, 0, &mut cues, &mut ups);
        w.cat_sel = 5;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.cat, Some(5), "Characters' page");
        assert_eq!(w.point([frame + 110.0, y(&w, 0)]), vec![key::RIGHT], "Level of Detail's value");
        assert_eq!(w.point([slider + 30.0, y(&w, 1)]), down(1, key::RIGHT));
        assert_eq!(w.point([slider - 2.0, y(&w, 1)]), down(1, key::LEFT), "left of the knob at the bar's end");
        crate::settings::set_client(ClientOptions { entity_slots: 128, ..crate::settings::client() });
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.point([slider + 30.0, y(&w, 1)]), down(1, key::LEFT), "left of the knob at 128");
        assert_eq!(w.point([slider + 110.0, y(&w, 1)]), down(1, key::RIGHT), "right of it");
        // NPC/PC Distance: its knob at the left end (the game's 100),
        // then at 1000 at the right end
        assert_eq!(w.point([slider + 30.0, y(&w, 2)]), down(2, key::RIGHT));
        crate::settings::set_client(ClientOptions { entity_view: 1000, ..crate::settings::client() });
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.point([slider + 110.0, y(&w, 2)]), down(2, key::LEFT), "left of the knob at 1000");
        // Name Distance: its knob at the left end (the game's 22), then at
        // 200 at the right end
        assert_eq!(w.point([slider + 30.0, y(&w, 3)]), down(3, key::RIGHT));
        assert_eq!(w.point([slider - 2.0, y(&w, 3)]), down(3, key::LEFT), "left of the knob at the bar's end");
        crate::settings::set_client(ClientOptions { name_distance: 200, ..crate::settings::client() });
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.point([slider + 110.0, y(&w, 3)]), down(3, key::LEFT), "left of the knob at 200");
        // Controller: Controller Focus's halves
        w.key(&vui, &mut r, &mut o, key::TRIANGLE, 0, &mut cues, &mut ups);
        w.cat_sel = 3;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.point([frame + 20.0, y(&w, 0)]), vec![key::LEFT], "Controller Focus's value, left");
        assert_eq!(w.point([frame + 110.0, y(&w, 1)]), down(1, key::RIGHT), "Capture Controller's, right");
        crate::settings::testing::reset(false);
    }

    /// The notes beside a page's Back legend: what this window reads on
    /// Controller's, what this server gives on Characters' while NPC/PC
    /// Display or Distance is chosen (not on Level of Detail), where the
    /// sound plays on Sound's, none on Video's (the control: the note there
    /// before the split).
    #[test]
    fn each_note_on_its_page() {
        use crate::pointer::testing::{NullRaster, vui};
        crate::settings::testing::reset(false);
        let vui = vui();
        let mut r = NullRaster;
        let mut o = GameOptions::default();
        let mut w = WndOptions { server_note: Some("This server: 24 (the game's)".into()), ..Default::default() };
        let (mut cues, mut ups) = (Vec::new(), Vec::new());
        let note = |w: &WndOptions, c: usize| w.page_note(c);
        w.open(&vui, &r, &o);
        w.sel = 3;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        let by_name = |n: &str| CATEGORIES.iter().position(|c| c.name == n).unwrap();
        assert_eq!(CATEGORIES[by_name("Controller")].note, Note::Reads);
        assert_eq!(CATEGORIES[by_name("Characters")].note, Note::Server);
        assert_eq!(CATEGORIES[by_name("Sound")].note, Note::Output);
        assert_eq!(CATEGORIES[by_name("Video")].note, Note::Scale);
        assert!(CATEGORIES.iter().filter(|c| c.note != Note::None).count() == 4, "the four notes only");
        w.cat_sel = by_name("Characters") as i32;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(note(&w, by_name("Characters")), None, "Level of Detail: no note");
        w.key(&vui, &mut r, &mut o, key::DOWN, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(note(&w, by_name("Characters")).as_deref(), Some("This server: 24 (the game's)"), "NPC/PC Display: the server's");
        w.key(&vui, &mut r, &mut o, key::TRIANGLE, 0, &mut cues, &mut ups);
        w.cat_sel = by_name("Controller") as i32;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(note(&w, by_name("Controller")), Some(crate::input::reads()), "Controller: what this window reads");
        assert_eq!(note(&w, by_name("Video")), None, "the control: none on Video's");
        // no output in a test (no device opened): said so
        assert_eq!(note(&w, by_name("Sound")).as_deref(), Some("No output now"), "Sound: where it plays");
    }
}
