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
//! The client's own on the game's Interface page: Profanity Filter under its
//! rows (the GM Jibrish's petition 1490), Off, Medium (only racial slurs) or
//! High (petition 1625), High by default as the game is at every login; it is the server's filter of what the player hears, told to a
//! server that asks for it (go-eqoa-server's 0x7E60,
//! `Session::agree_profanity_filter`), taken by Save Changes and put back by
//! Triangle as an Extended row is.
//!
//! The client's own, beside the game's: an Extended row between Sound and
//! Save Changes (the menu a row longer), its words the client's (no
//! WORDS.TXT entry). It opens a menu of its own over the options menu, drawn
//! as that one is: Video, World, Lighting, UI, Controller, Camera, Characters,
//! Sound, natively Textures, Map, Number Colors and Combat (`CATEGORIES`), each of which opens a page laid out and driven as
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
//!   flocks, `fx`; Off: none), Ground Fog (PS2, the game's look: the effect
//!   volumes' fog blended as the GS blends it into the PS2's 16-bit frame,
//!   `render::fog16`; Exact: at eight bits a channel, a brighter cloud) and
//!   Art (Frontiers' or the 2002 disc's, `era`,
//!   as F9 swaps it: drawn as it moves, the 2002 disc asked for the first
//!   time);
//! - Lighting (2026-11-08): Dynamic Lighting (Off, the PS2's: the strip
//!   programs' light per vertex, the personal light the one point light;
//!   On: the light per fragment, and the fires, placed lights, spells and
//!   glowing weapons lighting what is round them, `lights`, `render::lit`)
//!   and Sun Shadows (Off, the sun through everything as on a PS2; On, with
//!   Dynamic Lighting on: walls, roofs, trees, characters and hills casting
//!   shadows that move with the sun, `render::sunshadow`; on without it,
//!   the page says what they need),
//!   Light Shadows (2026-11-09: Off, the fires, the personal light and the
//!   spells lighting through everything; On, with Dynamic Lighting on: what
//!   stands between a surface and one of those lights casting a shadow
//!   away from it, as Sun Shadows' from the sun, `render::lightshadow`; on
//!   without it, the page says what they need), Shadow Distance (a slider
//!   and its number: how far out Sun Shadows and Light Shadows reach, the
//!   game's 250 to 1000 in 125s: the sun's cascades' far edge, and from the 8
//!   lights shadowed (4 in a page and on Android) to every light Dynamic
//!   Lighting draws, `render::Renderer::set_shadow_distance`),
//!   Ambient Occlusion (Off, the game's picture; On: the opaque scene's
//!   creases, corners and contacts darkened per pixel, `render::ao`) and
//!   Particles (2026-11-09; PS2, the game's look: the particles of blend
//!   mode 3, the Red Swarming Death's swarm, the flies and stench of the
//!   rotting, smoke, blended as the GS blends them into the PS2's 16-bit
//!   frame, `render::Renderer::set_ps2_particles`, as Ground Fog's fog;
//!   Exact: at eight bits a channel, where many faint puffs pile up into a
//!   darker mass; here because World's page is full);
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
//! - HUD Layout (2026-11-11, the server owner's "lets add click to drag HUD
//!   UI elements and also reset button"): Move HUD (Off; On: once the menu is
//!   left, the HUD's pieces outlined and dragged with the mouse or a finger,
//!   a bar at the top with Lock and Reset, `hudlayout`; not kept, as it is a
//!   mode, and Triangle leaves it) and Reset HUD (Reset: every piece back
//!   where the game puts it at once, kept, by ✕, left or right);
//! - Controller: Controller Focus (which controllers this window reads:
//!   Auto, Window, Pad 1 to 4 or Off, `input::pick`, kept for each window of
//!   the client open, `multibox`; what it reads now is said beside Back),
//!   Capture Controller (one setting for every window), and the touch
//!   screen's pad (`touch`; 2026-11-09, Jibrish's petition 1386): Touch
//!   Stick Size and Touch Button Size (sliders and their numbers: the sticks
//!   and the buttons, each 50 to 200% of the pad as it was in fives, drawn
//!   larger than it was only as far as the window has room, `touch::layout`;
//!   where it has none for the size asked the page says the size drawn), Touch
//!   Opacity (10 to 200% of its own in tens) and Touch Edge (how far in from
//!   the window's left and right edges it keeps, 0 to 10% of the window's
//!   shorter side); they took over Touch Pad Size (the whole pad's 50 to 100,
//!   which an older client.opt's line gives both sizes);
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
//!   every one, the nearest over the rest: `play::labels`) and Shadows (On,
//!   the game's: the characters' blob shadows, `shadows`; Off: none; on
//!   World until Ground Fog took its row, 2026-10-30);
//! - Sound: Sound in Background (one setting for every window: off, a window
//!   not in front is not heard, `audio::set_heard`) and natively Output
//!   Device (Default, the system's, followed when it changes, or a device
//!   the system lists, the default while it is not connected: `audio`'s
//!   `retarget`; its page says where the sound plays now);
//! - Textures, natively (2026-11-08): Texture Packs (Off, the game's: every
//!   texture its own; On: one a PCSX2 replacement pack in
//!   `textures/SLUS-20744/replacements` names drawn as the pack has it,
//!   `texpack`; its page says how many the packs hold, and where) and Dump
//!   Textures (Off; On: each texture made written to `textures/SLUS-20744/
//!   dumps` under the names PCSX2 gives it, and how many beside Back), each
//!   making every texture again as it moves; the `textures` folder is
//!   beside the program (2026-11-09; in the settings where the program's
//!   folder cannot be written to, which the page then says, and on
//!   Android), and on a computer Texture Folder (Open) shows it in the
//!   system's file manager, the page saying where it is
//!   (`texpack::open_folder`);
//! - Map (2026-11-09, the user's "also add the map and quest helper we had in
//!   a former update"): Map (On: the client's own map on M and the compass's
//!   click, the minimap, the waypoints on the compass and the group's
//!   waypoint, `map`; Off: none of them, the game's own), Minimap (On: the
//!   map round the player under the quick menus, `map::mini`; N turns it
//!   too), Map Markers (On: the map's XP camps and coaches, `map::markers`;
//!   the map's own X or ○ turns it too), Quest Helper (On: the quests' marks
//!   on the map and the minimap and their tracker, from a server that sends
//!   them, else the client's own guide, `questguide`, `map::quests`; Off:
//!   none, and no server asked for them) and Quest Tracker (On: the HUD's
//!   list of the quests under way, each step counted, under the minimap,
//!   `tracker`). Each defaults to On; the Map and the Quest Helper Off are
//!   the game as it is without them;
//! - Number Colors (2026-11-09, the server owner's "we can add the number
//!   colors to extended menu"): Your Damage, Your Procs, Your Heals, Pet
//!   Damage, Pet Procs and Pet Heals, each Default (the game's own colour) or
//!   one of `numcolor::PALETTE`, its value drawn in it: the floating numbers of
//!   the player's own figures and its pet's in that colour, by what made them,
//!   where the server says (go-eqoa-server's tags, asked for while any row is
//!   not Default: `Session::agree_number_tags`); every row Default by default;
//! - Combat (2026-11-10, the server owner's "lets add dps meters, will work
//!   solo, group, raid"): DPS Meter (Off by default; On: the HUD's damage meter
//!   of the player's group under the quest tracker, where the server keeps one,
//!   `dpsmeter`, the server asked for it: `Session::agree_dps_meter`), DPS Meter
//!   Shows (Fight: the fight under way, or the last; Overall: every fight since
//!   the login; the panel's title and `/dps` turn it too) and Reset DPS Meter
//!   (Reset: the meter cleared and the server's fight under way dropped, by ✕,
//!   left or right, as Texture Folder opens; `dpsmeter::ask_reset`), the page
//!   saying when this server keeps no meter; Auto-Cast (2026-11-11: On, the
//!   quick bars' slots marked auto used by the client itself, `autocast`; Off,
//!   none) and Auto Target (Off by default; On: with no target or a dead one,
//!   the nearest creature fighting the player targeted);
//! - Chat (2026-11-10, the GM Jibrish's petition 1463: "This should be off by
//!   default for ps2 players and a toggle option"): Item Links (On by default:
//!   items linked from the inventory into the chat line, drawn purple in the
//!   chat box and opened in a window, `itemlink`, the server asked for them:
//!   `Session::agree_item_links`; Off: none, the server sends the plain names a
//!   PS2 gets), the page saying when this server has no links.
//! Triangle goes back a step: a page to the categories, the categories to
//! the options menu. They change as they are moved, Triangle on the options
//! menu puts them back, and Save Changes keeps them with the game's
//! (`client.opt`).
//!
//! The client's own Controls page after it (`bindpage`): every key and
//! button of the default layout, moved by pressing the one to put there,
//! kept by Save Changes (`keys.opt`) and put back by Triangle as the rest.
//! Then its Macros page (`macropage`; the user, 2026-11-10: "lets also add
//! it as its own menu entry"): PCSX2's sixteen macros, kept and put back
//! with the keys. The menu's window has a row for each of the three (the
//! game's has seven rows; this one ten), its bottom where a nine-row one's
//! was, so the title's menu under it stays in view; the pages open where
//! the game's open, round the game's middle (`CENTER`).
//!
//! A pointer (`pointer`): a menu row is chosen at once; a page's row is
//! reached with up and down (a page knows which setting its row is only as
//! it draws, so left and right must wait for it), its label then steps its
//! value on, left or right of a value's middle (a slider's knob) is left or
//! right; the box's choices are taken at once.

#![allow(clippy::too_many_arguments)]

use crate::bindpage::ControlsPage;
use crate::macropage::MacrosPage;
use crate::pointer::{Hits, key, rows_rect};
use crate::settings::{BRIGHTNESS_STEPS, CAMERA_MAX, CAMERA_MIN, CAMERA_STEP, ClientOptions, ENTITY_SLOTS_GAME, ENTITY_SLOTS_MAX, ENTITY_SLOTS_STEP, ENTITY_VIEW_GAME, ENTITY_VIEW_MAX, ENTITY_VIEW_STEP, Focus, FrameRate, NAME_DISTANCE_GAME, NAME_DISTANCE_MAX, SHADOW_DISTANCE_GAME, SHADOW_DISTANCE_MAX, SHADOW_DISTANCE_STEP, TOUCH_EDGE_MAX, TOUCH_EDGE_STEP, TOUCH_OPACITY_MAX, TOUCH_OPACITY_MIN, TOUCH_OPACITY_STEP, TOUCH_SIZE_MAX, TOUCH_SIZE_MIN, TOUCH_SIZE_STEP, TURN_MAX, TURN_MIN, TURN_STEP};
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
            // on from the start (the user, 2026-11-07: "make auto attack on by
            // default"): off, the attack button must be held, and a new player's
            // tap swings once and stops, which reads as a laggy server. CLIENT's
            // SetDefaults leaves it 0; a saved game.opt keeps its own
            auto_attack: 1,
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
    /// 5 its Macros, 6 Save Changes)
    sel: i32,
    /// `m_page`: -1 the menu, 0 Game, 1 Interface, 2 Sound, 3 Extended, 4
    /// Controls, 5 Macros
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
    /// In the world on a server that keeps no damage meter, that it keeps
    /// none (`Play::agree_dps_meter` sets it), the Combat page's legend's
    pub meter_note: Option<String>,
    /// In the world on a server that offers no item links, that it has none
    /// (`Play::agree_item_links` sets it), the Chat page's legend's
    pub links_note: Option<String>,
    /// `m_item`: what the page's row changes (set as the page is drawn)
    item: i32,
    /// the values as the window opened, put back by Triangle
    saved: GameOptions,
    /// the client's own as it opened, put back with them
    saved_client: ClientOptions,
    /// The Save Changes box carries the account's notice (`settings::ACCOUNT_NOTICE`).
    notice: bool,
    /// the keys and buttons as it opened, put back with them
    saved_binds: crate::binds::Binds,
    /// the Controls page and the Macros page, and the sounds they made
    /// drawing (a held key's steps)
    controls: ControlsPage,
    macros: MacrosPage,
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
        if self.page == 5 {
            return self.macros.point(p);
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
    /// wide, seven rows and one each for Extended, Controls and Macros,
    /// titled 788) opening, the values kept, the menu. Its bottom is where
    /// the nine-row window's was before Macros came (the rows past it grow
    /// upward, clear of the title's menu under it).
    pub fn open(&mut self, vui: &Vui, r: &dyn Raster, o: &GameOptions) {
        self.window.close(true);
        self.window.init(vui, r, CENTER, 300, MENU_ROWS, 0);
        let mut nine = Window::default();
        nine.init(vui, r, CENTER, 300, 9, 0);
        self.window.center[1] -= self.window.bottom_right()[1] - nine.bottom_right()[1];
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

    /// A key let go: the Controls or Macros page's held direction ends.
    pub fn release(&mut self, key: i32) {
        match self.page {
            4 => {
                self.controls.key(key, false, self.now, &mut Vec::new());
            }
            5 => {
                self.macros.key(key, false, self.now, &mut Vec::new());
            }
            _ => {}
        }
    }

    /// What the Controls or Macros page waits for, if anything: a key, a
    /// button, or (Macros') a trigger.
    pub fn capturing(&self) -> Option<crate::bindpage::Column> {
        match self.page {
            _ if self.message.visible => None,
            4 => self.controls.capture,
            5 => self.macros.capturing(),
            _ => None,
        }
    }

    /// A key pressed while a page waits (`binds::name_of`).
    pub fn capture_key(&mut self, vui: &Vui, name: &str, cues: &mut Vec<i32>) {
        match self.capturing().map(|_| self.page) {
            Some(4) => self.controls.capture_key(vui, name, cues),
            Some(5) => self.macros.capture_key(vui, name, cues),
            _ => {}
        }
    }

    /// A key let go while the Macros page waits for a trigger: one of the
    /// trigger's, and it is kept.
    pub fn capture_key_up(&mut self, vui: &Vui, name: &str, cues: &mut Vec<i32>) {
        if self.capturing().is_some() && self.page == 5 {
            self.macros.capture_key_up(vui, name, cues);
        }
    }

    /// A controller's button pressed while a page waits (its bit index,
    /// before any remap).
    pub fn capture_button(&mut self, vui: &Vui, phys: u8, cues: &mut Vec<i32>) {
        match self.capturing().map(|_| self.page) {
            Some(4) => self.controls.capture_button(vui, phys, cues),
            Some(5) => self.macros.capture_button(phys, cues),
            _ => {}
        }
    }

    /// A controller's button let go while the Macros page waits.
    pub fn capture_button_up(&mut self, vui: &Vui, phys: u8, cues: &mut Vec<i32>) {
        if self.capturing().is_some() && self.page == 5 {
            self.macros.capture_button_up(vui, phys, cues);
        }
    }

    /// The controllers' sticks while the Macros page waits for a trigger.
    pub fn capture_axes(&mut self, vui: &Vui, sticks: [f32; 4], cues: &mut Vec<i32>) {
        if self.capturing().is_some() && self.page == 5 {
            self.macros.capture_axes(vui, sticks, cues);
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
                if std::mem::take(&mut self.notice) {
                    crate::settings::account_notice_seen();
                }
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
        if self.page == 5 {
            if self.macros.key(key, true, now, cues) {
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
                    // the account's options (petition 1621): said in the box until a Yes
                    let mut text = w(1634);
                    self.notice = crate::settings::account_notice_due();
                    if self.notice {
                        text.extend(wide(&format!(" {}", crate::settings::ACCOUNT_NOTICE)));
                    }
                    self.message.setup(vui, r, w(1633), text, 2, CENTER, 300);
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
            (-1, _) => MENU.len() as i32 - 1,
            (3, None) => CATEGORIES.len() as i32 - 1,
            (3, Some(c)) => CATEGORIES[c].items.len() as i32 - 1,
            (page, _) => {
                let mut last = [2, 3, 3];
                if FEATURES & 0x100 != 0 {
                    last[1] = 4;
                }
                // the client's Profanity Filter under the game's rows
                last[1] += 1;
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
    /// game's 22 to 200 (`step_name_distance`); Touch Stick Size (59) and
    /// Touch Button Size (60) by fives from 50 to 200, Touch Opacity (61) by
    /// tens from 10 to 200 and Touch Edge (62) by ones from 0 to 10, each as
    /// a slider; Text (30) goes between Sharp and
    /// Smooth; Update Notices (29), Overlapping Names (34, Hide and Show),
    /// Mipmaps (35), Loading Screens (38, Longer and Instant), Texture Packs
    /// (50), Dump Textures (51), the Map (41), Minimap (42), Map Markers (43),
    /// the Quest Helper (44) and Quest Tracker (45) turn over; Texture Folder (55) shows the
    /// folder (`texpack::open_folder`: sound 8, or 9 with none to show);
    /// Output Device (32) goes round the system default and the devices
    /// listed (`step_output`); Display (33) turns the window over (`display`);
    /// the Number Colors rows (64 to 69) go round Default and
    /// `numcolor::PALETTE`. A slider (`slider_at`) makes its own
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
            // Profanity Filter (Interface, the client's row): the server told
            // from the next frame (`Session::agree_profanity_filter`), kept by
            // Save Changes as an Extended row is
            PROFANITY => {
                let mut c = crate::settings::client();
                c.profanity_filter = c.profanity_filter.step(d);
                crate::settings::set_client(c);
            }
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
                    // Raid List: the server told from the next frame
                    // (`Session::agree_raid_roster`)
                    49 => c.raid_list = !c.raid_list,
                    // Pet Effects: the server told from the next frame
                    // (`Session::agree_pet_effects`), the row under the pet's
                    // bar drawn or not (`hud`)
                    58 => c.pet_effects = !c.pet_effects,
                    // the Map: the map, the minimap and the waypoints from
                    // the next frame (`Play::map_frame`; off, an open map
                    // closes)
                    41 => c.map = !c.map,
                    // Minimap: drawn or not from the next frame (`map::mini`)
                    42 => c.minimap = !c.minimap,
                    // Map Markers: the map's camps and coaches from its next
                    // frame (`map::markers`)
                    43 => c.map_markers = !c.map_markers,
                    // the Quest Helper and its Quest Tracker: the server told
                    // from the next frame (`Play::agree_quests`), the marks
                    // and the HUD's list drawn or not
                    44 => c.quest_helper = !c.quest_helper,
                    45 => c.quest_tracker = !c.quest_tracker,
                    // Texture Packs and Dump Textures: set from the next
                    // frame, every texture made again (`texpack::sync`,
                    // `viewer`'s `retexture`)
                    50 => c.texture_packs = !c.texture_packs,
                    51 => c.dump_textures = !c.dump_textures,
                    // Texture Folder: shown either way, as ✕ shows it
                    // (`select`)
                    55 => cues.push(if crate::texpack::open_folder() { 8 } else { 9 }),
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
                    // Ground Fog: the fog's fans from the next frame (`fx`,
                    // `render::fog16`)
                    47 => c.ps2_fog = !c.ps2_fog,
                    // Sun Shadows: cast or not from the next frame, with
                    // Dynamic Lighting on (`viewer`, `render::sunshadow`)
                    53 => c.sun_shadows = !c.sun_shadows,
                    // Light Shadows: cast or not from the next frame, with
                    // Dynamic Lighting on (`viewer`, `render::lightshadow`)
                    56 => c.light_shadows = !c.light_shadows,
                    // Shadow Distance: a step of 125 from the game's 250 to
                    // 1000, the shadows' reach from the next frame (`viewer`)
                    57 => {
                        let v = c.shadow_distance as i32 + d.signum() * SHADOW_DISTANCE_STEP as i32;
                        if (SHADOW_DISTANCE_GAME as i32..=SHADOW_DISTANCE_MAX as i32).contains(&v) {
                            c.shadow_distance = v as u16;
                            cues.push(11);
                        } else {
                            cues.push(9);
                        }
                    }
                    // Ambient Occlusion: its passes over the world from the
                    // next frame (`viewer`, `render::ao`)
                    54 => c.ambient_occlusion = !c.ambient_occlusion,
                    // Particles: their blend from the next frame (`viewer`,
                    // `render::Renderer::set_ps2_particles`)
                    63 => c.ps2_particles = !c.ps2_particles,
                    // Number Colors: round Default and the palette, the next
                    // number drawn in it and the server told from the next
                    // frame whether to tag them (`Session::agree_number_tags`)
                    64..=69 | 80 | 81 => {
                        if let Some(row) = crate::numcolor::row_of_item(self.item) {
                            c.number_colors[row] = crate::numcolor::step(c.number_colors[row], d);
                        }
                    }
                    // DPS Meter: the server told from the next frame
                    // (`Session::agree_dps_meter`), the panel drawn or not
                    70 => c.dps_meter = !c.dps_meter,
                    // DPS Meter Shows: the panel's view from the next frame
                    71 => c.dps_view = c.dps_view.other(),
                    // Item Links: the server told from the next frame
                    // (`Session::agree_item_links`), the chat box's links drawn
                    // or plain, the windows closed when off (`itemlink`)
                    73 => c.item_links = !c.item_links,
                    // Auto-Cast and Auto Target (`autocast`): from the next
                    // game frame
                    78 => c.auto_cast = !c.auto_cast,
                    79 => c.auto_target = !c.auto_target,
                    // Move HUD: the HUD's pieces dragged once the menu is left
                    // (`hudlayout`); not an option kept, so Triangle leaves it
                    76 => crate::hudlayout::set_unlocked(!crate::hudlayout::unlocked()),
                    // Reset HUD: every piece back at once, kept, either way
                    // as ✕ does it (`select`)
                    77 => {
                        crate::hudlayout::reset();
                        cues.push(8);
                    }
                    // Reset DPS Meter: done either way, as ✕ does it (`select`)
                    72 => {
                        crate::dpsmeter::ask_reset();
                        cues.push(8);
                    }
                    // Dynamic Lighting: the world lit per fragment or the
                    // PS2's way from the next frame (`viewer`, `lights`)
                    52 => c.dynamic_lighting = !c.dynamic_lighting,
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
                    // the touch rows: the pad drawn and touched so from the
                    // next frame (`viewer`, `touch::layout`, `TouchPad::draw`)
                    59 => slide(&mut c.touch_stick, TOUCH_SIZE_MIN, TOUCH_SIZE_MAX, TOUCH_SIZE_STEP),
                    60 => slide(&mut c.touch_buttons, TOUCH_SIZE_MIN, TOUCH_SIZE_MAX, TOUCH_SIZE_STEP),
                    61 => slide(&mut c.touch_opacity, TOUCH_OPACITY_MIN, TOUCH_OPACITY_MAX, TOUCH_OPACITY_STEP),
                    62 => slide(&mut c.touch_edge, 0, TOUCH_EDGE_MAX, TOUCH_EDGE_STEP),
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
        // (Texture Folder and Reset DPS Meter make their own sound)
        let inert = (self.item == 26 && cfg!(target_arch = "wasm32")) || self.item == 55 || self.item == 72 || self.item == 77;
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
    /// eight rows, an empty title; the client's Controls and Macros 600 wide
    /// over the middle of the frame, as many rows as fit), the client's
    /// Extended its menu of categories (a window as the menu's, titled
    /// Extended, over it) and X there a category's page (`page_window`), or
    /// on Save Changes asks (true).
    fn select(&mut self, vui: &Vui, r: &dyn Raster, cues: &mut Vec<i32>) -> bool {
        if self.page == 3 && self.cat.is_none() {
            cues.push(8);
            let c = self.cat_sel.clamp(0, CATEGORIES.len() as i32 - 1) as usize;
            self.cat = Some(c);
            self.row = 0;
            self.page_window(vui, r, CATEGORIES[c].items.len() as i32 + 3);
            return false;
        }
        // ✕ on Texture Folder shows it, as left and right do (`change`); on
        // any other row of a page it does nothing, as on the game's
        if self.page == 3 && self.cat.is_some() && self.item == 55 {
            cues.push(if crate::texpack::open_folder() { 8 } else { 9 });
            return false;
        }
        // and on Reset DPS Meter it resets, as left and right do
        if self.page == 3 && self.cat.is_some() && self.item == 72 {
            crate::dpsmeter::ask_reset();
            cues.push(8);
            return false;
        }
        // Reset HUD the same; Move HUD turns, as left and right turn it
        if self.page == 3 && self.cat.is_some() && self.item == 77 {
            crate::hudlayout::reset();
            cues.push(8);
            return false;
        }
        if self.page == 3 && self.cat.is_some() && self.item == 76 {
            crate::hudlayout::set_unlocked(!crate::hudlayout::unlocked());
            cues.push(11);
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
            // with the client's Profanity Filter a row more
            1 => {
                if FEATURES & 0x200 != 0 {
                    9
                } else {
                    8
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
                self.window4.init(vui, r, CENTER, 300, CATEGORY_ROWS, 0);
                self.window4.open(false);
                self.window4.set_title(Some(&wide(EXTENDED)));
                let bottom = |w: &Window| w.top_left()[1] + w.size[1];
                self.window4.center[1] -= bottom(&self.window4) - bottom(&self.window);
                return false;
            }
            4 | 5 => {
                self.page = self.sel;
                if self.sel == 4 {
                    self.controls.reset();
                } else {
                    self.macros.reset();
                }
                self.window2.close(true);
                self.window2.init(vui, r, [320, 224], 600, ControlsPage::window_rows(vui.font_height(r)), 0);
                self.window2.open(false);
                self.window2.set_title(Some(&wide(if self.sel == 4 { CONTROLS } else { MACROS_PAGE })));
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
        self.window2.init(vui, r, CENTER, 440, rows, 0);
        self.window2.open(false);
        if rows > 8 {
            self.window2.set_title(None);
            let mut eight = Window::default();
            eight.init(vui, r, CENTER, 440, 8, 0);
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
            5 => self.macros.draw(vui, r, &mut self.window2, &mut self.controls_cues),
            _ => {}
        }
        let w = |id: i32| wide(vui.words.get(id, vui.language));
        if self.save_count != 0 {
            self.save_count -= 1;
            self.window3.init(vui, r, CENTER, 400, 3, 0);
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

    /// `DrawMenu`: Game, Interface, Sound, the client's Extended, Controls
    /// and Macros, Save Changes, the selected one lit, and the legend two
    /// rows under the last.
    fn draw_menu(&mut self, vui: &Vui, r: &mut dyn Raster) {
        self.item = -1;
        self.window.draw(vui, r);
        let w = |id: i32| match id {
            -1 => EXTENDED.to_string(),
            -2 => CONTROLS.to_string(),
            -3 => MACROS_PAGE.to_string(),
            id => vui.words.get(id, vui.language).to_string(),
        };
        let width = self.window.size[0];
        for (k, id) in MENU.into_iter().enumerate() {
            let on = self.sel == k as i32;
            let s = if on { format!("|q{}|w", w(id)) } else { w(id) };
            self.window.text_box(vui, r, &wide(&s), 40, k as i32 + 1, width - 80, 1, UI_COLORS[on as usize], 8);
            if self.page == -1 {
                self.hits.push(rows_rect(&self.window, vui, r, 40, k as i32 + 1, width - 80, 1), k as i32);
            }
        }
        let legend = wide(&format!("|x {}   |t {}", w(212), w(753)));
        self.window.text(vui, r, &legend, 40, MENU_ROWS - 1, UI_COLORS[0], 0);
    }

    /// A page's rows: its labels, the tick, and each value in its frame
    /// (the arrows on the selected one); a value `None` is a slider drawn by
    /// the caller. The pages start a row down.
    fn draw_rows(&mut self, vui: &Vui, r: &mut dyn Raster, labels: &[Vec<u16>], values: &[Option<Vec<u16>>]) {
        self.draw_rows_painted(vui, r, labels, values, &[]);
    }

    /// `draw_rows` with a value drawn in its own colour where `paints` gives
    /// one (a Number Colors row's, `numcolor::swatch`), shadowed so a light
    /// one reads on the window; every other as the game's pages draw them.
    fn draw_rows_painted(&mut self, vui: &Vui, r: &mut dyn Raster, labels: &[Vec<u16>], values: &[Option<Vec<u16>>], paints: &[Option<[f32; 4]>]) {
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
            let (c, shadow) = paints.get(k).copied().flatten().map_or((UI_COLORS[0], 0), |c| (c, 1));
            self.window2.text_box(vui, r, v, width - 176, row, 132, 1, c, 8 | 2 | shadow);
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
    /// Axis, Soft Keyboard and (feature 0x200) Vibration; then the client's
    /// Profanity Filter (the GM Jibrish's petition 1490), its words the
    /// client's.
    fn draw_interface(&mut self, vui: &Vui, r: &mut dyn Raster, o: &GameOptions) {
        let mut items = vec![10, 0, 1, 6];
        let word = |id: i32| Some(wide(vui.words.get(id, vui.language)));
        let on_off = |v: u8| word(if v != 0 { 551 } else { 552 });
        let mut labels = vec![1792, 747, 752, 794];
        let mut values = vec![on_off(self.edit[2]), word(if o.follow != 0 { 68 } else { 805 }), word(if o.inverted != 0 { 779 } else { 784 }), word(if o.compact != 0 { 755 } else { 802 })];
        if FEATURES & 0x200 != 0 {
            items.push(11);
            labels.push(1604);
            values.push(on_off(self.edit[3]));
        }
        items.push(PROFANITY);
        values.push(Some(wide(crate::settings::profanity_filter().label())));
        self.item = items.get(self.row as usize).copied().unwrap_or(self.item);
        let mut labels = words(vui, &labels);
        labels.push(wide(PROFANITY_LABEL));
        let n = labels.len() as i32;
        self.draw_rows(vui, r, &labels, &values);
        self.back_legend(vui, r, n + 2);
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
        let paints: Vec<Option<[f32; 4]>> = cat.items.iter().map(|&i| value_paint(i)).collect();
        self.draw_rows_painted(vui, r, &labels, &values, &paints);
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
            Note::Reads => Some(touch_note(self.item, &crate::settings::client().touch_opts(), crate::touch::drawn_here).unwrap_or_else(crate::input::reads)),
            Note::Server => self.server_note.clone().filter(|_| matches!(self.item, 23 | 24)),
            Note::Scale => {
                let asked = crate::settings::client().render_scale as u32;
                scale_note(asked, render::scale::would_draw(asked)).filter(|_| self.item == 39)
            }
            Note::Output | Note::Textures if cfg!(target_arch = "wasm32") => None,
            Note::Textures => crate::texpack::note(self.item),
            Note::Output => Some(match crate::audio::playing() {
                Some(d) => format!("Plays on {d}"),
                None => "No output now".to_string(),
            }),
            Note::Lighting => match self.item {
                53 => sun_note(&crate::settings::client()),
                56 => light_note(&crate::settings::client()),
                _ => None,
            },
            Note::Meter => self.meter_note.clone().filter(|_| matches!(self.item, 70..=72)),
            Note::Links => self.links_note.clone().filter(|_| self.item == 73),
            Note::Hud => (self.item == 76 && crate::hudlayout::unlocked()).then(|| "Leave the menu and drag the pieces".to_string()),
            Note::None => None,
        }
    }
}

/// What Controller's page says beside Back while Touch Stick Size, Touch
/// Button Size or Touch Edge is chosen (`drawn` answers the sizes the pad is
/// drawn at in its window as `o` asks, `touch::drawn_here`): where the window
/// has no room for a size asked, the size it is drawn at (the larger comes
/// down first: `touch::layout`), named when only one is; none otherwise
/// (the page says what this window reads).
fn touch_note(item: i32, o: &crate::touch::Opts, drawn: impl Fn(&crate::touch::Opts) -> Option<[f32; 2]>) -> Option<String> {
    if !matches!(item, 59 | 60 | 62) {
        return None;
    }
    let [s, b] = drawn(o)?;
    // in whole percent, never above what is drawn
    let pct = |v: f32| (v * 100.0 + 1e-3).floor() as u32;
    match (s < o.stick, b < o.buttons) {
        (true, true) => Some(format!("Drawn at {}% here", pct(s.max(b)))),
        (true, false) => Some(format!("Sticks drawn at {}% here", pct(s))),
        (false, true) => Some(format!("Buttons drawn at {}% here", pct(b))),
        (false, false) => None,
    }
}

/// What Lighting's page says beside Back while Sun Shadows are chosen: that
/// they need Dynamic Lighting, when they are on and it is off (they are
/// drawn only with it).
fn sun_note(c: &ClientOptions) -> Option<String> {
    (c.sun_shadows && !c.dynamic_lighting).then(|| "Sun Shadows need Dynamic Lighting".to_string())
}

/// And while Light Shadows are chosen: the same of them.
fn light_note(c: &ClientOptions) -> Option<String> {
    (c.light_shadows && !c.dynamic_lighting).then(|| "Light Shadows need Dynamic Lighting".to_string())
}

/// The client's words for its own pages (no WORDS.TXT entry): the menu's
/// rows, Extended's categories and its rows (`EXTENDED_ITEMS`). (WORDS.TXT's
/// "CONTROLS", 1039, is in capitals and used by none of the game's modules.)
const EXTENDED: &str = "Extended";
const CONTROLS: &str = "Controls";
const MACROS_PAGE: &str = "Macros";

/// The options menu's rows, by their words: Game, Interface, Sound (773,
/// 778, 800), the client's Extended, Controls and Macros, Save Changes
/// (482).
const MENU: [i32; 7] = [773, 778, 800, -1, -2, -3, 482];

/// The menu's window: a row down, its rows, a row clear, the legend.
const MENU_ROWS: i32 = MENU.len() as i32 + 3;

/// The game's middle for the options' windows (`VIWndOptions`' 320, 246):
/// the pages, the box and "Saving..." open round it.
const CENTER: [i32; 2] = [320, 246];

/// Extended's rows, by the item each changes (`change`), and their labels.
/// A row is drawn by its item (`value_text`, or `slider_at` for a slider)
/// and shown on the page of the category that lists it (`CATEGORIES`).
const EXTENDED_ITEMS: [(i32, &str); 65] = [
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
    (41, "Map"),
    (42, "Minimap"),
    (43, "Map Markers"),
    (44, "Quest Helper"),
    (45, "Quest Tracker"),
    (46, "Shadows"),
    (47, "Ground Fog"),
    (49, "Raid List"),
    (50, "Texture Packs"),
    (51, "Dump Textures"),
    (52, "Dynamic Lighting"),
    (53, "Sun Shadows"),
    (54, "Ambient Occlusion"),
    (55, "Texture Folder"),
    (56, "Light Shadows"),
    (57, "Shadow Distance"),
    (58, "Pet Effects"),
    (59, "Touch Stick Size"),
    (60, "Touch Button Size"),
    (61, "Touch Opacity"),
    (62, "Touch Edge"),
    (63, "Particles"),
    (64, crate::numcolor::LABELS[0]),
    (65, crate::numcolor::LABELS[1]),
    (66, crate::numcolor::LABELS[2]),
    (67, crate::numcolor::LABELS[3]),
    (68, crate::numcolor::LABELS[4]),
    (69, crate::numcolor::LABELS[5]),
    (70, "DPS Meter"),
    (71, "DPS Meter Shows"),
    (72, "Reset DPS Meter"),
    (73, "Item Links"),
    (76, "Move HUD"),
    (77, "Reset HUD"),
    (78, "Auto-Cast"),
    (79, "Auto Target"),
    (80, crate::numcolor::LABELS[6]),
    (81, crate::numcolor::LABELS[7]),
];

/// What a category's page says beside its Back legend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Note {
    None,
    /// what this window reads (`input::reads`); while a touch size or
    /// Touch Edge is chosen and the window has no room for a size asked, the
    /// size drawn (`touch_note`)
    Reads,
    /// what this server gives (`WndOptions::server_note`), while NPC/PC
    /// Display or Distance is chosen
    Server,
    /// the scale the frame is drawn at, while Render Scale is chosen and the
    /// window is too large for the one asked (`render::scale::fit`)
    Scale,
    /// where the sound plays now (`audio::playing`), natively
    Output,
    /// how many replacements the packs hold and where, how many textures
    /// have been dumped while Dump Textures is chosen, or where the folder
    /// is while Texture Folder is (`texpack::note`), natively
    Textures,
    /// that Sun Shadows or Light Shadows need Dynamic Lighting, while they
    /// are chosen and on and it is off (`sun_note`, `light_note`)
    Lighting,
    /// that this server keeps no meter, while a DPS Meter row is chosen
    /// (`WndOptions::meter_note`, which play sets)
    Meter,
    /// that this server has no item links, while Item Links is chosen
    /// (`WndOptions::links_note`, which play sets)
    Links,
    /// how the HUD's pieces move while Move HUD is on and chosen (`hudlayout`)
    Hud,
}

/// One of Extended's categories: its name, its rows (their items, in the
/// page's order) and its note.
#[derive(Clone, Copy)]
struct Category {
    name: &'static str,
    items: &'static [i32],
    note: Note,
}

/// Extended's categories, in its menu's order (the user's: "Extended, then
/// Video, UI, Controller, etc."), each a page of `PAGE_MOST` rows at most;
/// Lighting after World (2026-11-08: Dynamic Lighting, the world's light,
/// and Ambient Occlusion);
/// natively Textures (2026-11-08: Texture Packs and Dump Textures, a
/// folder's files a page cannot reach; 2026-11-09: Texture Folder); Map last
/// (2026-11-09: the map and the quest helper, the client's own, back from
/// before 2026-10-27); Number Colors after it (2026-11-09: the floating
/// numbers' colours by what made them, `numcolor`); Merc Colors after it
/// (2026-11-11: the merc's numbers' rows, Number Colors' page full); Combat last (2026-11-10:
/// the DPS meter, `dpsmeter`); HUD Layout last (2026-11-11: Move HUD and
/// Reset HUD, `hudlayout`; UI's page was full).
const ALL_CATEGORIES: [Category; 15] = [
    Category { name: "Video", items: VIDEO, note: Note::Scale },
    Category { name: "World", items: &[20, 35, 36, 37, 47, 25], note: Note::None },
    Category { name: "Lighting", items: &[52, 53, 56, 57, 54, 63], note: Note::Lighting },
    Category { name: "UI", items: &[22, 29, 30, 40, 49, 58], note: Note::None },
    Category { name: "Controller", items: &[16, 19, 59, 60, 61, 62], note: Note::Reads },
    Category { name: "Camera", items: &[15, 17, 18], note: Note::None },
    Category { name: "Characters", items: &[12, 23, 24, 27, 34, 46], note: Note::Server },
    Category { name: "Sound", items: SOUND, note: Note::Output },
    Category { name: "Textures", items: TEXTURES, note: Note::Textures },
    Category { name: "Map", items: &[41, 42, 43, 44, 45], note: Note::None },
    Category { name: "Number Colors", items: &crate::numcolor::NUMBER_ITEMS, note: Note::None },
    Category { name: "Merc Colors", items: &crate::numcolor::MERC_ITEMS, note: Note::None },
    Category { name: "Combat", items: &[70, 71, 72, 78, 79], note: Note::Meter },
    Category { name: "Chat", items: &[73], note: Note::Links },
    Category { name: "HUD Layout", items: &[76, 77], note: Note::Hud },
];
#[cfg(not(target_arch = "wasm32"))]
const CATEGORIES: &[Category] = &ALL_CATEGORIES;
/// A page's: all but Textures.
#[cfg(target_arch = "wasm32")]
const CATEGORIES: &[Category] = &{
    let a = ALL_CATEGORIES;
    [a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[9], a[10], a[11], a[12], a[13], a[14]]
};

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

/// Textures' rows (natively; a page has none): on a computer Texture Folder
/// after Texture Packs and Dump Textures, showing the folder beside the
/// program (`texpack::open_folder`); an Android app's is in its own
/// storage, which no file manager shows.
#[cfg(not(target_os = "android"))]
const TEXTURES: &[i32] = &[50, 51, 55];
#[cfg(target_os = "android")]
const TEXTURES: &[i32] = &[50, 51];

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

const _: () = assert!(fits(CATEGORIES), "a category's page would scroll: give its rows another category");

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
        41 => on_off(c.map),
        42 => on_off(c.minimap),
        43 => on_off(c.map_markers),
        44 => on_off(c.quest_helper),
        45 => on_off(c.quest_tracker),
        34 => wide(if c.overlapping_names { "Show" } else { "Hide" }),
        49 => on_off(c.raid_list),
        58 => on_off(c.pet_effects),
        50 => on_off(c.texture_packs),
        51 => on_off(c.dump_textures),
        55 => wide("Open"),
        35 => on_off(c.mipmaps),
        36 => on_off(c.flora),
        37 => on_off(c.environment),
        46 => on_off(c.shadows),
        47 => wide(if c.ps2_fog { "PS2" } else { "Exact" }),
        53 => on_off(c.sun_shadows),
        56 => on_off(c.light_shadows),
        54 => on_off(c.ambient_occlusion),
        63 => wide(if c.ps2_particles { "PS2" } else { "Exact" }),
        64..=69 | 80 | 81 => wide(crate::numcolor::name(c.number_colors[crate::numcolor::row_of_item(item)?])),
        70 => on_off(c.dps_meter),
        71 => wide(c.dps_view.name()),
        72 => wide("Reset"),
        76 => on_off(crate::hudlayout::unlocked()),
        77 => wide("Reset"),
        73 => on_off(c.item_links),
        78 => on_off(c.auto_cast),
        79 => on_off(c.auto_target),
        52 => on_off(c.dynamic_lighting),
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

/// The colour an Extended row's value is drawn in, none for the windows'
/// own: a Number Colors row's colour (Default none).
fn value_paint(item: i32) -> Option<[f32; 4]> {
    let row = crate::numcolor::row_of_item(item)?;
    crate::numcolor::swatch(crate::settings::client().number_colors[row])
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
        59 => (at(c.touch_stick as f32, TOUCH_SIZE_MIN as f32, TOUCH_SIZE_MAX as f32), Some(c.touch_stick as u16)),
        60 => (at(c.touch_buttons as f32, TOUCH_SIZE_MIN as f32, TOUCH_SIZE_MAX as f32), Some(c.touch_buttons as u16)),
        61 => (at(c.touch_opacity as f32, TOUCH_OPACITY_MIN as f32, TOUCH_OPACITY_MAX as f32), Some(c.touch_opacity as u16)),
        62 => (c.touch_edge as f32 / TOUCH_EDGE_MAX as f32, Some(c.touch_edge as u16)),
        57 => (at(c.shadow_distance as f32, SHADOW_DISTANCE_GAME as f32, SHADOW_DISTANCE_MAX as f32), Some(c.shadow_distance)),
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

/// The Interface page's client row, Profanity Filter (`draw_interface`), its
/// item past every Extended one's.
const PROFANITY: i32 = 74;
const PROFANITY_LABEL: &str = "Profanity Filter";
const LABEL: i32 = HIT_ROWS;

/// WORDS.TXT's entries `ids`, wide.
fn words(vui: &Vui, ids: &[i32]) -> Vec<Vec<u16>> {
    ids.iter().map(|&id| wide(vui.words.get(id, vui.language))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The four held settings change only when saved; the live ones at
    /// Auto Attack is on from the start (the user's choice); everything else
    /// is CLIENT's SetDefaults.
    #[test]
    fn auto_attack_on_by_default() {
        let o = GameOptions::default();
        assert_eq!(o.auto_attack, 1);
        assert_eq!((o.stereo, o.follow, o.vibration, o.no_loot), (1, 0, 1, 0));
    }

    /// The Interface page's last row is the client's Profanity Filter (petition
    /// 1490): down to it past the game's five, right steps it High -> Off ->
    /// Medium in the client's settings (what Play tells the server; petition
    /// 1625's tiers), left back, Triangle on the menu puts it back to High. The
    /// control: the row above it (Vibration) leaves it alone.
    #[test]
    fn the_interface_page_turns_the_profanity_filter() {
        use crate::pointer::key;
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
        assert_eq!(w.page, 1);
        for _ in 0..4 {
            w.key(&vui, &mut r, &mut o, key::DOWN, 0, &mut cues, &mut ups);
        }
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.item, 11, "Vibration");
        w.key(&vui, &mut r, &mut o, key::RIGHT, 0, &mut cues, &mut ups);
        use crate::settings::Profanity;
        assert_eq!(crate::settings::profanity_filter(), Profanity::High, "the control: Vibration is not it");
        w.key(&vui, &mut r, &mut o, key::DOWN, 0, &mut cues, &mut ups);
        w.key(&vui, &mut r, &mut o, key::DOWN, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!((w.row, w.item), (5, PROFANITY), "the last row, and no row past it");
        w.key(&vui, &mut r, &mut o, key::RIGHT, 0, &mut cues, &mut ups);
        assert_eq!(crate::settings::profanity_filter(), Profanity::Off, "turned off");
        w.key(&vui, &mut r, &mut o, key::RIGHT, 0, &mut cues, &mut ups);
        assert_eq!(crate::settings::profanity_filter(), Profanity::Medium, "Medium");
        w.key(&vui, &mut r, &mut o, key::LEFT, 0, &mut cues, &mut ups);
        w.key(&vui, &mut r, &mut o, key::LEFT, 0, &mut cues, &mut ups);
        assert_eq!(crate::settings::profanity_filter(), Profanity::High, "left back round to High");
        w.key(&vui, &mut r, &mut o, key::RIGHT, 0, &mut cues, &mut ups);
        w.key(&vui, &mut r, &mut o, key::RIGHT, 0, &mut cues, &mut ups);
        // Triangle to the menu, Triangle there: every setting as it was
        w.key(&vui, &mut r, &mut o, key::TRIANGLE, 0, &mut cues, &mut ups);
        w.key(&vui, &mut r, &mut o, key::TRIANGLE, 0, &mut cues, &mut ups);
        assert_eq!(crate::settings::profanity_filter(), Profanity::High, "put back to High");
        crate::settings::testing::reset(false);
    }

    /// once; Triangle on the menu puts the live ones back.
    #[test]
    fn held_settings_wait_for_save() {
        let mut w = WndOptions::default();
        let mut o = GameOptions { auto_attack: 0, ..GameOptions::default() };
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
        // back to the menu, Save Changes (past the client's Extended,
        // Controls and Macros): the box's choices
        w.key(&vui, &mut r, &mut o, 132, 0, &mut cues, &mut ups);
        w.sel = 6;
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
        let mut o = GameOptions { auto_attack: 0, ..GameOptions::default() };
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
        // World: Fog, Mipmaps, Flora, Environment, Ground Fog, Art
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
        assert_eq!(w.item, 47, "Ground Fog");
        assert!(settings::client().ps2_fog, "PS2: the game's look");
        assert_eq!(value_text(&vui, 47), Some(wide("PS2")));
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().ps2_fog, settings::client().environment, cues.last()), (false, true, Some(&11)), "Exact, Environment still on");
        assert_eq!(value_text(&vui, 47), Some(wide("Exact")));
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::client().ps2_fog, "and back: two values");
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
        // Lighting: Dynamic Lighting, Off (the PS2's), then On
        press(&mut w, &mut o, key::DOWN, &mut cues);
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(2), 52), "Lighting, Dynamic Lighting");
        assert_eq!(value_text(&vui, 52), Some(wide(vui.words.get(552, vui.language))), "Off, the PS2's light");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().dynamic_lighting, cues.last()), (true, Some(&11)), "On");
        assert_eq!(value_text(&vui, 52), Some(wide(vui.words.get(551, vui.language))));
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(!settings::client().dynamic_lighting, "and back: two values");
        // Sun Shadows: Off, then On, which without Dynamic Lighting the page
        // says they need
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 53, "Sun Shadows");
        assert_eq!(value_text(&vui, 53), Some(wide(vui.words.get(552, vui.language))), "Off");
        assert_eq!(w.page_note(2), None, "off: no note");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().sun_shadows, cues.last()), (true, Some(&11)), "On");
        assert_eq!(value_text(&vui, 53), Some(wide(vui.words.get(551, vui.language))));
        assert_eq!(w.page_note(2).as_deref(), Some("Sun Shadows need Dynamic Lighting"), "on without Dynamic Lighting");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(!settings::client().sun_shadows, "and back: two values");
        // Light Shadows: the same, with their own note
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 56, "Light Shadows");
        assert_eq!(value_text(&vui, 56), Some(wide(vui.words.get(552, vui.language))), "Off");
        assert_eq!(w.page_note(2), None, "off: no note");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().light_shadows, settings::client().sun_shadows, cues.last()), (true, false, Some(&11)), "On, Sun Shadows still off");
        assert_eq!(value_text(&vui, 56), Some(wide(vui.words.get(551, vui.language))));
        assert_eq!(w.page_note(2).as_deref(), Some("Light Shadows need Dynamic Lighting"), "on without Dynamic Lighting");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(!settings::client().light_shadows, "and back: two values");
        // Shadow Distance: a slider and its number, the game's 250 at the
        // left, a step of 125 a press, 1000 at the right
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 57, "Shadow Distance");
        assert_eq!(slider_at(57), Some((0.0, Some(250))), "the game's 250");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!((settings::client().shadow_distance, cues.last()), (250, Some(&9)), "nothing under the game's");
        for want in [375u16, 500, 625, 750, 875, 1000] {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
            assert_eq!((settings::client().shadow_distance, cues.last()), (want, Some(&11)));
        }
        assert_eq!(slider_at(57), Some((1.0, Some(1000))));
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().shadow_distance, cues.last()), (1000, Some(&9)), "nothing past 1000");
        for _ in 0..6 {
            press(&mut w, &mut o, key::LEFT, &mut cues);
        }
        assert_eq!(settings::client().shadow_distance, 250, "and back");
        // Ambient Occlusion, Off (the game's picture), then On
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 54, "Ambient Occlusion");
        assert_eq!(value_text(&vui, 54), Some(wide(vui.words.get(552, vui.language))), "Off, the game's picture");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().ambient_occlusion, settings::client().dynamic_lighting, cues.last()), (true, false, Some(&11)), "On, Dynamic Lighting still off");
        assert_eq!(value_text(&vui, 54), Some(wide(vui.words.get(551, vui.language))));
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(!settings::client().ambient_occlusion, "and back: two values");
        // Particles, PS2 (the game's look), then Exact
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 63, "Particles");
        assert!(settings::client().ps2_particles, "PS2: the game's look");
        assert_eq!(value_text(&vui, 63), Some(wide("PS2")));
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().ps2_particles, settings::client().ps2_fog, cues.last()), (false, true, Some(&11)), "Exact, Ground Fog still PS2");
        assert_eq!(value_text(&vui, 63), Some(wide("Exact")));
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::client().ps2_particles, "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (5, Some(&9)), "Lighting's last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        assert_eq!((w.page, w.cat, w.cat_sel, w.item), (3, None, 2, -1), "back to the categories, Lighting still lit");
        // UI: Ability Bars, Update Notices, Text, HUD, Raid List, Pet Effects
        press(&mut w, &mut o, key::DOWN, &mut cues);
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(3), 22), "UI, Ability Bars");
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
        assert_eq!(w.item, 49, "Raid List");
        assert!(settings::raid_list(), "on: a raid group's every name and health");
        assert_eq!(value_text(&vui, 49), Some(wide(vui.words.get(551, vui.language))));
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::raid_list(), cues.last()), (false, Some(&11)), "off: the game's bars");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::raid_list(), "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 58, "Pet Effects");
        assert!(settings::pet_effects(), "on: the pet's effects under its bar");
        assert_eq!(value_text(&vui, 58), Some(wide(vui.words.get(551, vui.language))));
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::pet_effects(), cues.last()), (false, Some(&11)), "off: the game's bar alone");
        assert_eq!(value_text(&vui, 58), Some(wide(vui.words.get(552, vui.language))));
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::pet_effects(), "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (5, Some(&9)), "UI's last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        // Controller: Controller Focus, Capture Controller, Touch Pad Size
        press(&mut w, &mut o, key::DOWN, &mut cues);
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(4), 16), "Controller, Controller Focus");
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
        // the touch rows (they took over Touch Pad Size): each from the pad
        // as it was, to both ends, sound 9 past each
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 59, "Touch Stick Size");
        assert_eq!((settings::client().touch_stick, slider_at(59)), (100, Some((50.0 / 150.0, Some(100)))), "the pad's as it was, a third along");
        for _ in 0..21 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!((settings::client().touch_stick, slider_at(59)), (200, Some((1.0, Some(200)))), "100 to 200 in 20 steps of 5");
        assert_eq!(&cues[cues.len() - 2..], &[11, 9], "the last step, then its end");
        for _ in 0..31 {
            press(&mut w, &mut o, key::LEFT, &mut cues);
        }
        assert_eq!((settings::client().touch_stick, slider_at(59), cues.last()), (50, Some((0.0, Some(50))), Some(&9)), "down to 50 in 30");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!(settings::client().touch_stick, 55);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 60, "Touch Button Size");
        assert_eq!((settings::client().touch_buttons, slider_at(60)), (100, Some((50.0 / 150.0, Some(100)))));
        for _ in 0..11 {
            press(&mut w, &mut o, key::LEFT, &mut cues);
        }
        assert_eq!((settings::client().touch_buttons, cues.last()), (50, Some(&9)), "100 to 50 in 10 steps of 5");
        for _ in 0..31 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!((settings::client().touch_buttons, slider_at(60), cues.last()), (200, Some((1.0, Some(200))), Some(&9)));
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 61, "Touch Opacity");
        assert_eq!((settings::client().touch_opacity, slider_at(61)), (100, Some((90.0 / 190.0, Some(100)))), "its own alphas");
        for _ in 0..10 {
            press(&mut w, &mut o, key::LEFT, &mut cues);
        }
        assert_eq!((settings::client().touch_opacity, slider_at(61), cues.last()), (10, Some((0.0, Some(10))), Some(&9)), "100 to 10 in 9 steps of 10");
        for _ in 0..20 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!((settings::client().touch_opacity, cues.last()), (200, Some(&9)), "to 200 in 19");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 62, "Touch Edge");
        assert_eq!((settings::client().touch_edge, slider_at(62)), (0, Some((0.0, Some(0)))), "none");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!((settings::client().touch_edge, cues.last()), (0, Some(&9)), "no less");
        for _ in 0..11 {
            press(&mut w, &mut o, key::RIGHT, &mut cues);
        }
        assert_eq!((settings::client().touch_edge, slider_at(62), cues.last()), (10, Some((1.0, Some(10))), Some(&9)), "0 to 10 in ones");
        assert_eq!(settings::touch_options(), crate::touch::Opts { stick: 0.55, buttons: 2.0, opacity: 2.0, edge: 0.1 }, "the pad drawn so");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (5, Some(&9)), "Controller's last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        // Camera: Camera Distance, Camera Turn Speed, Screen Shake
        press(&mut w, &mut o, key::DOWN, &mut cues);
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(5), 15), "Camera, Camera Distance");
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
        assert_eq!((w.cat, w.item), (Some(6), 12), "Characters, Level of Detail");
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
        assert_eq!(w.item, 46, "Shadows, on Characters since 2026-10-30");
        assert!(settings::client().shadows, "on: the game's");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::client().shadows, settings::client().ps2_fog, cues.last()), (false, true, Some(&11)), "off, Ground Fog still PS2");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::client().shadows, "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (5, Some(&9)), "Characters' last row (six, a page's most)");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        // Sound: Sound in Background
        press(&mut w, &mut o, key::DOWN, &mut cues);
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(7), 21), "Sound, Sound in Background");
        assert!(settings::background_sound(), "on: a window behind others keeps its sound");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert!(!settings::background_sound());
        assert_eq!(cues.last(), Some(&11));
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 32, "Output Device");
        assert_eq!(value_text(&vui, 32), Some(wide(SYSTEM_DEFAULT)), "the system's default");
        assert!(w.page_note(7).is_some_and(|n| n.starts_with("Plays on ") || n == "No output now"), "where it plays, beside Back");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (1, Some(&9)), "Sound's last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        // Textures (natively): Texture Packs and Dump Textures, each Off
        // then On; a test keeps no folder, so the note says there is none
        assert_eq!((w.cat_sel, cues.last()), (8, Some(&11)), "Textures, after Sound");
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(8), 50), "Textures, Texture Packs");
        assert_eq!(value_text(&vui, 50), Some(wide(vui.words.get(552, vui.language))), "Off, the game's own textures");
        assert_eq!(w.page_note(8), None, "off: no note");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert!(settings::client().texture_packs);
        assert_eq!((value_text(&vui, 50), cues.last()), (Some(wide(vui.words.get(551, vui.language))), Some(&11)));
        assert_eq!(w.page_note(8).as_deref(), Some("No folder here"));
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 51, "Dump Textures");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::client().dump_textures, "either way turns it over");
        assert_eq!(w.page_note(8).as_deref(), Some("0 dumped"), "how many dumped, beside Back");
        // Texture Folder: Open; with no folder here (a test's), nothing to
        // show, sound 9, by either way or ✕
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.item, cues.last()), (55, Some(&11)), "Texture Folder");
        assert_eq!(value_text(&vui, 55), Some(wide("Open")));
        assert_eq!(w.page_note(8).as_deref(), Some("No folder here"), "where it is, beside Back");
        for k in [key::RIGHT, key::LEFT, key::CROSS] {
            press(&mut w, &mut o, k, &mut cues);
            assert_eq!(cues.last(), Some(&9), "none to show ({k})");
        }
        assert!(crate::texpack::testing::shown().is_empty());
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (2, Some(&9)), "Textures' last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        // Map: the Map, Minimap, Map Markers, the Quest Helper and Quest
        // Tracker, each On then Off, one moving no other
        assert_eq!((w.cat_sel, cues.last()), (9, Some(&11)), "Map, after Textures");
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(9), 41), "Map, the Map");
        for (item, on) in [(41, settings::map as fn() -> bool), (42, settings::minimap), (43, settings::map_markers), (44, settings::quest_helper), (45, settings::quest_tracker)] {
            assert_eq!(w.item, item, "{}", label_of(item));
            assert!(on(), "{}: on by default", label_of(item));
            assert_eq!(value_text(&vui, item), Some(wide(vui.words.get(551, vui.language))), "{}: On, the game's word", label_of(item));
            press(&mut w, &mut o, key::RIGHT, &mut cues);
            assert_eq!((on(), cues.last()), (false, Some(&11)), "{}: off", label_of(item));
            assert_eq!(value_text(&vui, item), Some(wide(vui.words.get(552, vui.language))));
            press(&mut w, &mut o, key::LEFT, &mut cues);
            assert!(on(), "{}: and back, two values", label_of(item));
            press(&mut w, &mut o, key::DOWN, &mut cues);
        }
        assert_eq!((w.row, cues.last()), (4, Some(&9)), "Map's last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        // Number Colors: six rows, each Default (the game's colour, drawn as
        // any value) then round the palette either way, each drawn in its
        // colour, one moving no other
        assert_eq!((w.cat_sel, cues.last()), (10, Some(&11)), "Number Colors, after Map");
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(10), 64), "Number Colors, Your Damage");
        let last = crate::numcolor::PALETTE.len() as u8;
        for (row, item) in crate::numcolor::NUMBER_ITEMS.into_iter().enumerate() {
            assert_eq!((w.item, label_of(item)), (item, crate::numcolor::LABELS[row]));
            assert_eq!(settings::number_colors(), crate::numcolor::DEFAULTS, "{}: every row as it came", label_of(item));
            assert_eq!((value_text(&vui, item), value_paint(item)), (Some(wide("Default")), None), "{}: Default, drawn as any value", label_of(item));
            press(&mut w, &mut o, key::RIGHT, &mut cues);
            assert_eq!((settings::number_colors()[row], cues.last()), (1, Some(&11)), "{}: on to the first colour", label_of(item));
            assert_eq!((value_text(&vui, item), value_paint(item)), (Some(wide("White")), Some([1.0; 4])), "{}: White, in white", label_of(item));
            assert_eq!(settings::number_colors().iter().zip(crate::numcolor::DEFAULTS).filter(|(c, d)| *c != d).count(), 1, "{}: no other row moved", label_of(item));
            press(&mut w, &mut o, key::LEFT, &mut cues);
            press(&mut w, &mut o, key::LEFT, &mut cues);
            assert_eq!(settings::number_colors()[row], last, "{}: left of Default, the last", label_of(item));
            assert_eq!(value_text(&vui, item), Some(wide("Gray")));
            press(&mut w, &mut o, key::RIGHT, &mut cues);
            assert_eq!(settings::number_colors()[row], 0, "{}: and round to Default", label_of(item));
            press(&mut w, &mut o, key::DOWN, &mut cues);
        }
        assert_eq!((w.row, cues.last()), (5, Some(&9)), "Number Colors' last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        // Merc Colors: Merc Damage Light Blue and Merc Heals Purple as they
        // come (the owner, petition 1613: distinct from the player's own
        // without a pick), each drawn in its colour, then round the palette,
        // Default the game's
        assert_eq!((w.cat_sel, cues.last()), (11, Some(&11)), "Merc Colors, after Number Colors");
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(11), 80), "Merc Colors, Merc Damage");
        for (k, (item, start, next)) in [(80, "Light Blue", "Cyan"), (81, "Purple", "Blue")].into_iter().enumerate() {
            let row = 6 + k;
            assert_eq!((w.item, label_of(item)), (item, crate::numcolor::LABELS[row]));
            assert_eq!(settings::number_colors(), crate::numcolor::DEFAULTS, "{}: as it came", label_of(item));
            assert_eq!(value_text(&vui, item), Some(wide(start)), "{}: {start} by default", label_of(item));
            assert!(value_paint(item).is_some(), "{}: drawn in its colour", label_of(item));
            press(&mut w, &mut o, key::RIGHT, &mut cues);
            assert_eq!((value_text(&vui, item), cues.last()), (Some(wide(next)), Some(&11)), "{}: on to the next", label_of(item));
            for _ in 0..crate::numcolor::PALETTE.len() - crate::numcolor::DEFAULTS[row] as usize {
                press(&mut w, &mut o, key::RIGHT, &mut cues);
            }
            assert_eq!((settings::number_colors()[row], value_paint(item)), (0, None), "{}: round to Default, the game's", label_of(item));
            assert_eq!(value_text(&vui, item), Some(wide("Default")));
            for _ in 0..crate::numcolor::DEFAULTS[row] {
                press(&mut w, &mut o, key::RIGHT, &mut cues);
            }
            assert_eq!(settings::number_colors()[row], crate::numcolor::DEFAULTS[row], "{}: and back", label_of(item));
            press(&mut w, &mut o, key::DOWN, &mut cues);
        }
        assert_eq!((w.row, cues.last()), (1, Some(&9)), "Merc Colors' last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        // Combat: the DPS Meter Off then On, what it shows Fight then Overall,
        // and Reset DPS Meter asking the meter's reset either way, as ✕ does
        assert_eq!((w.cat_sel, cues.last()), (12, Some(&11)), "Combat, after Merc Colors");
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(12), 70), "Combat, the DPS Meter");
        assert!(!settings::dps_meter(), "off by default");
        assert_eq!(value_text(&vui, 70), Some(wide(vui.words.get(552, vui.language))), "Off, the game's word");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::dps_meter(), cues.last()), (true, Some(&11)), "on");
        assert_eq!(value_text(&vui, 70), Some(wide(vui.words.get(551, vui.language))));
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(!settings::dps_meter(), "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 71, "DPS Meter Shows");
        assert_eq!((settings::dps_view(), value_text(&vui, 71)), (crate::dpsmeter::View::Fight, Some(wide("Fight"))), "the fight by default");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::dps_view(), value_text(&vui, 71), cues.last()), (crate::dpsmeter::View::Overall, Some(wide("Overall")), Some(&11)), "Overall");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert_eq!(settings::dps_view(), crate::dpsmeter::View::Fight, "and back: two values");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.item, value_text(&vui, 72)), (72, Some(wide("Reset"))), "Reset DPS Meter");
        assert!(!crate::dpsmeter::take_reset(), "none asked yet");
        for k in [key::RIGHT, key::LEFT, key::CROSS] {
            press(&mut w, &mut o, k, &mut cues);
            assert_eq!((crate::dpsmeter::take_reset(), cues.last()), (true, Some(&8)), "a reset asked ({k}), its sound");
        }
        assert_eq!(w.page_note(12), None, "no note off a server");
        w.meter_note = Some("This server keeps no meter".into());
        assert_eq!(w.page_note(12).as_deref(), Some("This server keeps no meter"), "a server that keeps none said");
        // Auto-Cast On by default (no slot is auto until marked), Auto Target
        // Off by default (`autocast`), each two values
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 78, "Auto-Cast");
        assert!(settings::auto_cast(), "on by default");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::auto_cast(), cues.last()), (false, Some(&11)), "off");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::auto_cast(), "and back");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!(w.item, 79, "Auto Target");
        assert!(!settings::auto_target(), "off by default");
        assert_eq!(value_text(&vui, 79), Some(wide(vui.words.get(552, vui.language))));
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::auto_target(), cues.last()), (true, Some(&11)), "on");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(!settings::auto_target(), "and back");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (4, Some(&9)), "Combat's last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        // Chat: Item Links On by default, then Off and back (the petition 1463's
        // toggle), the page saying where the server has none
        assert_eq!((w.cat_sel, cues.last()), (13, Some(&11)), "Chat, after Combat");
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(13), 73), "Chat, Item Links");
        assert!(settings::item_links(), "on by default");
        assert_eq!(value_text(&vui, 73), Some(wide(vui.words.get(551, vui.language))), "On, the game's word");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((settings::item_links(), cues.last()), (false, Some(&11)), "off");
        press(&mut w, &mut o, key::LEFT, &mut cues);
        assert!(settings::item_links(), "and back: two values");
        assert_eq!(w.page_note(13), None, "no note off a server");
        w.links_note = Some("This server has no item links".into());
        assert_eq!(w.page_note(13).as_deref(), Some("This server has no item links"));
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (0, Some(&9)), "Chat's one row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        // HUD Layout (`hudlayout`): Move HUD Off, then On and back by left,
        // right and ✕, its note while on; Reset HUD putting every piece back
        // by any of them, kept, with its own sound
        assert_eq!((w.cat_sel, cues.last()), (14, Some(&11)), "HUD Layout, after Chat");
        crate::hudlayout::testing_reset();
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert_eq!((w.cat, w.item), (Some(14), 76), "HUD Layout, Move HUD");
        assert!(!crate::hudlayout::unlocked(), "off by default");
        assert_eq!(value_text(&vui, 76), Some(wide(vui.words.get(552, vui.language))));
        assert_eq!(w.page_note(14), None, "no note while off");
        press(&mut w, &mut o, key::RIGHT, &mut cues);
        assert_eq!((crate::hudlayout::unlocked(), cues.last()), (true, Some(&11)), "on");
        assert_eq!(w.page_note(14).as_deref(), Some("Leave the menu and drag the pieces"));
        press(&mut w, &mut o, key::CROSS, &mut cues);
        assert!(!crate::hudlayout::unlocked(), "✕ turns it too");
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.item, value_text(&vui, 77)), (77, Some(wide("Reset"))), "Reset HUD");
        for k in [key::RIGHT, key::LEFT, key::CROSS] {
            testing::put("hud.opt", b"chat l 1 t 1\n");
            crate::hudlayout::testing_reset();
            assert!(crate::hudlayout::layout().get(crate::hudlayout::Piece::Chat).is_some(), "the control: moved");
            press(&mut w, &mut o, k, &mut cues);
            assert_eq!(crate::hudlayout::layout(), Default::default(), "every piece back ({k})");
            assert_eq!((testing::kept("hud.opt"), cues.last()), (Some(Vec::new()), Some(&8)), "kept so, its sound ({k})");
        }
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.row, cues.last()), (1, Some(&9)), "HUD Layout's last row");
        press(&mut w, &mut o, key::TRIANGLE, &mut cues);
        press(&mut w, &mut o, key::DOWN, &mut cues);
        assert_eq!((w.cat_sel, cues.last()), (14, Some(&9)), "HUD Layout, the last category");
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
            // Camera: Camera Turn Speed a step up (past Lighting)
            key::DOWN,
            key::DOWN,
            key::DOWN,
            key::DOWN,
            key::CROSS,
            key::DOWN,
            key::RIGHT,
            key::TRIANGLE,
            key::TRIANGLE,
            // Save Changes (past Controls and Macros), Yes
            key::DOWN,
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
        assert_eq!(cues.last(), Some(&9), "no category past the last");
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
        // (an Android app has no Texture Folder: `TEXTURES`)
        let all: Vec<i32> = EXTENDED_ITEMS.iter().map(|&(i, _)| i).filter(|&i| !(cfg!(target_os = "android") && i == 55)).collect();
        assert_eq!(listed(CATEGORIES), all);
        assert!(fits(CATEGORIES));
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
        assert_eq!(w.window2.rows, 9, "Interface: nine rows (Vibration, the client's Profanity Filter)");
        w.page_window(&vui, &r, 8);
        let (top8, h8) = (w.window2.top_left()[1], w.window2.size[1]);
        w.page_window(&vui, &r, PAGE_MOST as i32 + 3);
        let (top9, h9) = (w.window2.top_left()[1], w.window2.size[1]);
        assert_eq!((w.window2.rows, w.window2.title_h), (9, 18), "a row more, no title bar");
        assert_eq!(top9, top8, "the top where an eight-row page's is");
        let d = vui.font_height(&r) - 4;
        assert!((0..d).contains(&(h9 - h8)), "less than a row lower at the bottom: {} of {d}", h9 - h8);
        // the control: nine rows laid out as the game's pages are, centred
        let mut game = Window::default();
        game.init(&vui, &r, CENTER, 440, 9, 0);
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
        // down past Macros to Save Changes, Yes
        for k in [key::TRIANGLE, key::DOWN, key::DOWN, key::CROSS, key::LEFT, key::CROSS] {
            press(&mut w, &mut o, k);
        }
        for _ in 0..3 {
            w.draw(&vui, &mut NullRaster, &o, &mut ups);
        }
        assert_eq!(String::from_utf8(testing::kept("keys.opt").unwrap()).unwrap(), "key.walk_forward=up\nkey.menu_up=w\n");
        settings::set_binds(Default::default());
        testing::reset(false);
    }

    /// The client's Macros page, the menu's sixth row (the user,
    /// 2026-11-10: "lets also add it as its own menu entry"): ✕ opens it on
    /// Macro 1, ✕ goes into the macro, ✕ on its trigger waits for one (the
    /// window says so: `capturing`), Y held and let go is kept at once, the
    /// grid's ✕ ticks the cursor's button; Triangle backs out a step at a
    /// time and on the menu puts them back; Save Changes keeps them in
    /// keys.opt beside the keys. The menu's ten rows end where its nine did
    /// (the title's menu under it); the controls: the Controls page, a row
    /// up, is still the fifth row, and Save Changes the last.
    #[test]
    fn the_macros_page() {
        use crate::bindpage::Column;
        use crate::binds::Input;
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
        let mut nine = Window::default();
        nine.init(&vui, &NullRaster, CENTER, 300, 9, 0);
        nine.set_title(Some(&[1]));
        assert_eq!((w.window.rows, w.window.bottom_right()[1]), (10, nine.bottom_right()[1]), "a row more, grown upward");
        for _ in 0..5 {
            press(&mut w, &mut o, key::DOWN);
        }
        assert_eq!(w.sel, 5);
        press(&mut w, &mut o, key::CROSS);
        assert_eq!((w.page, w.window2.title.chars()), (5, wide("Macros")), "Macros");
        press(&mut w, &mut o, key::CROSS);
        assert!(w.macros.editing, "into Macro 1");
        press(&mut w, &mut o, key::CROSS);
        assert_eq!(w.capturing(), Some(Column::Trigger), "its trigger awaited");
        w.capture_key(&vui, "y", &mut Vec::new());
        w.capture_key_up(&vui, "y", &mut Vec::new());
        assert_eq!((w.capturing(), settings::binds().macros[0].triggers.clone()), (None, vec![vec![Input::Key("y".into())]]), "kept as it was let go");
        // down to the grid (Add, Clear, the four settings), ✕ ticks ↑
        for _ in 0..7 {
            press(&mut w, &mut o, key::DOWN);
        }
        press(&mut w, &mut o, key::CROSS);
        assert_eq!(settings::binds().macros[0].buttons, crate::input::bit::UP);
        // Triangle: the list, the menu, then closed and put back
        press(&mut w, &mut o, key::TRIANGLE);
        assert!(!w.macros.editing && w.page == 5);
        press(&mut w, &mut o, key::TRIANGLE);
        assert_eq!((w.page, w.sel), (-1, 5));
        press(&mut w, &mut o, key::TRIANGLE);
        assert!(settings::binds().is_default(), "put back");
        // again, kept with Save Changes
        w.open(&vui, &NullRaster, &o);
        w.sel = 5;
        for k in [key::CROSS, key::CROSS, key::CROSS] {
            press(&mut w, &mut o, k);
        }
        w.capture_button(&vui, crate::binds::bit_index(crate::input::bit::L3).unwrap(), &mut Vec::new());
        w.capture_button_up(&vui, crate::binds::bit_index(crate::input::bit::L3).unwrap(), &mut Vec::new());
        for k in [key::TRIANGLE, key::TRIANGLE, key::DOWN, key::CROSS, key::LEFT, key::CROSS] {
            press(&mut w, &mut o, k);
        }
        for _ in 0..3 {
            w.draw(&vui, &mut NullRaster, &o, &mut ups);
        }
        assert_eq!(String::from_utf8(testing::kept("keys.opt").unwrap()).unwrap(), "macro.1.trigger=pad:l3\n");
        // the controls: Controls is the fifth row, Save Changes the seventh
        assert_eq!(MENU[4], -2);
        assert_eq!(MENU.last(), Some(&482));
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
        assert_eq!(w.point(category(&w, 5)), vec![key::CROSS]);
        assert_eq!(w.cat_sel, 5, "Camera");
        assert_eq!(w.point(category(&w, 7)), vec![key::CROSS]);
        assert_eq!(w.cat_sel, 7, "Sound");
        assert!(w.point([5.0, 5.0]).is_empty());
        w.point(category(&w, 5));
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.cat, Some(5), "Camera's page");
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
        assert_eq!(w.point([frame + 20.0, y(&w, 4)]), down(4, key::LEFT), "Ground Fog's value");
        // Characters: NPC/PC Display's knob at its left end (the game's 24),
        // then at 128 near its middle
        w.key(&vui, &mut r, &mut o, key::TRIANGLE, 0, &mut cues, &mut ups);
        w.cat_sel = 6;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        assert_eq!(w.cat, Some(6), "Characters' page");
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
        w.cat_sel = 4;
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
        assert_eq!(CATEGORIES[by_name("Lighting")].note, Note::Lighting);
        assert_eq!(CATEGORIES[by_name("Combat")].note, Note::Meter);
        assert_eq!(CATEGORIES[by_name("Chat")].note, Note::Links);
        assert_eq!(CATEGORIES[by_name("HUD Layout")].note, Note::Hud);
        assert!(CATEGORIES.iter().filter(|c| c.note != Note::None).count() == if cfg!(target_arch = "wasm32") { 8 } else { 9 }, "the notes only");
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Textures: nothing to say while both are off (a test keeps no
            // folder, so with Texture Packs on there is none to read)
            assert_eq!(CATEGORIES[by_name("Textures")].note, Note::Textures);
            assert_eq!(CATEGORIES[by_name("Textures")].items, &[50, 51, 55]);
            assert_eq!(note(&w, by_name("Textures")), None, "both off: no note");
            crate::settings::set_client(crate::settings::ClientOptions { texture_packs: true, ..crate::settings::client() });
            assert_eq!(note(&w, by_name("Textures")).as_deref(), Some("No folder here"), "on, with no folder to read");
            crate::settings::set_client(crate::settings::ClientOptions { texture_packs: false, ..crate::settings::client() });
        }
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

    /// Texture Folder (55) on a computer: its value Open; right, left and ✕
    /// each show the Frontiers disc's folder in the `textures` beside the
    /// program (sound 8), made with its replacements, the page saying
    /// where, and no setting changes; the control: with no folder (a run
    /// that keeps no settings) sound 9 and nothing shown.
    #[cfg(not(target_os = "android"))]
    #[test]
    fn the_texture_folder_row_shows_the_folder() {
        use crate::pointer::testing::{NullRaster, vui};
        use crate::texpack::{Beside, Place, testing};
        crate::settings::testing::reset(false);
        let vui = vui();
        let mut r = NullRaster;
        let mut o = GameOptions::default();
        let mut w = WndOptions::default();
        let (mut cues, mut ups) = (Vec::new(), Vec::new());
        let d = std::env::temp_dir().join(format!("eqoa-options-texture-folder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        testing::set_place(Some(Place { dir: d.join("textures"), also: None, beside: Beside::Program("eqoa.exe".into()), moved: None, why: None }));
        let textures = CATEGORIES.iter().position(|c| c.name == "Textures").unwrap();
        w.open(&vui, &r, &o);
        w.sel = 3;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        w.cat_sel = textures as i32;
        w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
        w.draw(&vui, &mut r, &o, &mut ups);
        for _ in 0..2 {
            w.key(&vui, &mut r, &mut o, key::DOWN, 0, &mut cues, &mut ups);
            w.draw(&vui, &mut r, &o, &mut ups);
        }
        assert_eq!((w.cat, w.item), (Some(textures), 55), "Texture Folder, the third row");
        assert_eq!(w.page_note(textures).as_deref(), Some("Beside eqoa.exe"));
        for k in [key::RIGHT, key::LEFT, key::CROSS] {
            cues.clear();
            w.key(&vui, &mut r, &mut o, k, 0, &mut cues, &mut ups);
            w.draw(&vui, &mut r, &o, &mut ups);
            assert_eq!(cues, vec![8], "shown ({k})");
        }
        let folder = d.join("textures").join(esf::pcsx2::SERIAL);
        assert_eq!(testing::shown(), vec![folder.clone(); 3]);
        assert!(folder.join("replacements").is_dir(), "made as PCSX2 makes it");
        assert_eq!(crate::settings::client(), ClientOptions::default(), "no setting changed");
        testing::set_place(None);
        cues.clear();
        w.key(&vui, &mut r, &mut o, key::RIGHT, 0, &mut cues, &mut ups);
        assert_eq!((cues, testing::shown()), (vec![9], vec![]), "the control: no folder");
        crate::settings::testing::reset(false);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Sun Shadows' note: only while they are on and Dynamic Lighting is
    /// off (they are drawn only with it); none with both on, none off.
    #[test]
    fn sun_shadows_say_what_they_need() {
        let with = |sun_shadows: bool, dynamic_lighting: bool| sun_note(&ClientOptions { sun_shadows, dynamic_lighting, ..Default::default() });
        assert_eq!(with(true, false).as_deref(), Some("Sun Shadows need Dynamic Lighting"));
        assert_eq!(with(true, true), None, "both on: drawn");
        assert_eq!(with(false, false), None);
        assert_eq!(with(false, true), None);
    }

    /// Light Shadows' note: the same, of them alone (Sun Shadows' says
    /// nothing of them, nor theirs of Sun Shadows).
    #[test]
    fn light_shadows_say_what_they_need() {
        let with = |light_shadows: bool, dynamic_lighting: bool| light_note(&ClientOptions { light_shadows, dynamic_lighting, ..Default::default() });
        assert_eq!(with(true, false).as_deref(), Some("Light Shadows need Dynamic Lighting"));
        assert_eq!(with(true, true), None, "both on: drawn");
        assert_eq!(with(false, false), None);
        assert_eq!(with(false, true), None);
        assert_eq!(sun_note(&ClientOptions { light_shadows: true, ..Default::default() }), None, "Sun Shadows' note not theirs");
        assert_eq!(light_note(&ClientOptions { sun_shadows: true, ..Default::default() }), None, "nor theirs Sun Shadows'");
    }

    /// Controller's page names the size the touch pad is drawn at while a
    /// touch size or Touch Edge is chosen and the window has no room for the
    /// size asked, naming which when only one is; otherwise, and on any other
    /// row, none (the page says what this window reads). With the real
    /// layout: a phone held upright has no room for buttons at 200%, one held
    /// wide has for sticks at 200%.
    #[test]
    fn controllers_page_says_the_touch_size_drawn_where_less_than_asked() {
        let o = crate::touch::Opts { stick: 2.0, buttons: 1.5, ..Default::default() };
        let note = |item: i32, d: Option<[f32; 2]>| touch_note(item, &o, |_| d);
        assert_eq!(note(59, Some([1.6834, 1.5])).as_deref(), Some("Sticks drawn at 168% here"));
        assert_eq!(note(60, Some([2.0, 1.25])).as_deref(), Some("Buttons drawn at 125% here"));
        assert_eq!(note(62, Some([1.2, 1.2])).as_deref(), Some("Drawn at 120% here"));
        assert_eq!(note(62, Some([1.5, 1.5])).as_deref(), Some("Sticks drawn at 150% here"), "the sticks down to the buttons' size");
        assert_eq!(note(59, Some([2.0, 1.5])), None, "as asked");
        assert_eq!(note(61, Some([1.0, 1.0])), None, "not on Touch Opacity");
        assert_eq!(note(16, Some([1.0, 1.0])), None);
        assert_eq!(note(59, None), None, "no window drawn yet");
        let real = |item: i32, o: crate::touch::Opts, size: (u32, u32)| touch_note(item, &o, |o| Some(crate::touch::layout(size, o).drawn));
        let upright = real(60, crate::touch::Opts { buttons: 2.0, ..Default::default() }, (1080, 2400));
        assert!(upright.as_deref().is_some_and(|n| n.starts_with("Buttons drawn at 1") && n.ends_with("% here")), "{upright:?}");
        assert_eq!(real(59, crate::touch::Opts { stick: 2.0, ..Default::default() }, (2400, 1080)), None);
    }

    /// Number Colors' page draws a chosen row's value in its colour over a
    /// black shadow (it reads on the window, a light one too), and a Default
    /// row's as every value is drawn, in the window's black with no shadow
    /// (the control: every row Default draws no colour at all).
    #[test]
    fn number_colors_values_are_drawn_in_their_colour() {
        use crate::pointer::testing::{NullRaster, vui};
        #[derive(Default)]
        struct Texts(Vec<([i32; 2], [f32; 4], String)>);
        impl Raster for Texts {
            fn blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
            fn stretch_blit(&mut self, _: u32, _: u32, _: [i32; 2], _: [i32; 2], _: [i32; 2], _: [i32; 2], _: i32, _: f32) {}
            fn fill_fan(&mut self, _: [f32; 4], _: [[i32; 2]; 4]) {}
            fn blit_full(&mut self, _: u32, _: u32, _: [i32; 2], _: i32, _: f32) {}
            fn draw_string(&mut self, _: [i32; 2], _: [f32; 4], _: &[u16]) {}
            fn draw_centered_string(&mut self, at: [i32; 2], c: [f32; 4], s: &[u16], _: u32) {
                self.0.push((at, c, String::from_utf16_lossy(s)));
            }
            fn string_width(&self, s: &[u16]) -> i32 {
                s.len() as i32 * 8
            }
            fn point_size(&self) -> i32 {
                16
            }
            fn viewport(&self) -> [i32; 4] {
                [0, 0, 640, 448]
            }
            fn set_viewport(&mut self, _: [i32; 4]) {}
        }
        impl crate::ui::Layout for Texts {}
        let page = |choices: [u8; crate::numcolor::ROWS]| -> Vec<([i32; 2], [f32; 4], String)> {
            crate::settings::testing::reset(false);
            let mut c = crate::settings::client();
            c.number_colors = choices;
            crate::settings::set_client(c);
            let vui = vui();
            let mut r = NullRaster;
            let mut o = GameOptions::default();
            let mut w = WndOptions::default();
            let (mut cues, mut ups) = (Vec::new(), Vec::new());
            w.open(&vui, &r, &o);
            w.sel = 3;
            w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
            w.cat_sel = CATEGORIES.iter().position(|c| c.name == "Number Colors").unwrap() as i32;
            w.key(&vui, &mut r, &mut o, key::CROSS, 0, &mut cues, &mut ups);
            let mut t = Texts::default();
            w.draw(&vui, &mut t, &o, &mut ups);
            t.0
        };
        let red = [1.0, 48.0 / 255.0, 48.0 / 255.0, 1.0];
        let drawn = page([0, crate::numcolor::from_key("red").unwrap(), 0, 0, 0, 0, 0, 0]);
        let of = |s: &str| drawn.iter().filter(|d| d.2 == s).cloned().collect::<Vec<_>>();
        let reds = of("Red");
        assert_eq!(reds.len(), 2, "its shadow and itself: {drawn:?}");
        assert_eq!((reds[0].1, reds[1].1), ([0.0, 0.0, 0.0, 1.0], red), "a black shadow under it, then it in red");
        assert_eq!(reds[0].0, [reds[1].0[0] + 2, reds[1].0[1] + 2], "the shadow 2 down and right");
        let defaults = of("Default");
        assert_eq!(defaults.len(), 5, "five rows Default, no shadows: {defaults:?}");
        assert!(defaults.iter().all(|d| d.1 == UI_COLORS[0]), "Default in the window's black");
        let plain = page([0; crate::numcolor::ROWS]);
        assert!(plain.iter().filter(|d| d.2 == "Default").all(|d| d.1 == UI_COLORS[0]) && plain.iter().filter(|d| d.2 == "Default").count() == 6);
        for p in &crate::numcolor::LABELS[..6] {
            assert!(plain.iter().any(|d| d.2 == *p), "{p}'s label on the page");
        }
        crate::settings::testing::reset(false);
    }

    /// Every Extended row is one item, and each category's rows are rows:
    /// two branches that took the same number for a row (Lighting's) fail
    /// here, where a second match arm would only warn.
    #[test]
    fn every_extended_row_has_its_own_item() {
        let mut items: Vec<i32> = EXTENDED_ITEMS.iter().map(|r| r.0).collect();
        items.sort_unstable();
        let n = items.len();
        items.dedup();
        assert_eq!(items.len(), n, "an item named twice in EXTENDED_ITEMS");
        for c in ALL_CATEGORIES.iter() {
            for i in c.items {
                assert!(EXTENDED_ITEMS.iter().any(|r| r.0 == *i), "{}'s item {i} has no row", c.name);
            }
        }
        let listed: Vec<i32> = ALL_CATEGORIES.iter().flat_map(|c| c.items.iter().copied()).collect();
        let mut once = listed.clone();
        once.sort_unstable();
        once.dedup();
        assert_eq!(once.len(), listed.len(), "an item on two pages");
    }
}
