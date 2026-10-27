//! The map: the client's own (the game has none; F3 20261021, jib). M opens
//! it full screen over the game, on the player's area; it is drawn from the
//! game's own terrain off the player's disc, zone by zone while the game
//! plays (`build`: each zone's terrain from straight above, the light
//! pinned, no fog), and kept so it is drawn once (`store`); the names are the
//! game's (`ZoneName`'s grid and `gWorldInfo`'s world names, read out of
//! CLIENT at run time, the words out of WORDS.TXT). A marker shows the player
//! and the way it faces, the group's members their compass colours' dots.
//!
//! It takes the keys while it is up, as a window does: Escape, M or △ close
//! it; the wheel, the right stick, + and - (L1 and R1 a step) zoom out to the
//! whole world and back in; a drag, the left stick, the arrows, WASD and the
//! d-pad pan; ✕, Space or Home goes back to the player and follows it. A
//! finger drags it and two pinch it. There is no pad combination to open it
//! yet (jib's "TBD"): a pad reaches it by M on a keyboard, or by a click or
//! a tap on the HUD's compass, which opens it too (the touch screen's way).
//!
//! A waypoint (F3 20261026-000207, a player's suggestion; the user's choice:
//! personal, any server, nothing sent): a click or a tap on the map puts a
//! magenta diamond there, one a world, and a click on it takes it away; □
//! (Q on a keyboard) puts it at the map's middle once the map is panned off
//! the player (a cross marks the middle for a pad), and takes it away while
//! the map follows the player or when it is at the middle. The HUD's compass
//! shows it as a dot on its rim the way it shows the group (`hud`'s
//! `draw_patch`), at any distance; reached (within `ARRIVE`) it goes. Kept
//! per world in this window's `waypoints.opt` (`settings::waypoints`).
//!
//! The target (F3 20261026-231048, Liav's: "Submitting a waypoint with an
//! NPC targeted uses the NPC's coordinates for the waypoint. No target uses
//! your own coordinates. May cause confusion though?"; the user's choice:
//! both, □'s waypoint and Suggest's spot): while the map follows the player,
//! □ puts the waypoint on the target's spot (whoever is targeted and in view,
//! a merchant, a quest giver, a creature, another player; never the player)
//! and takes it away there, and a white ring marks the target on the map. A
//! target out of view (in no entity slot) leaves □ as it was. So that nothing
//! moves under the player, what they chose on the map comes first: once the
//! map is panned □ goes on its middle as before, the target or not. Suggest
//! takes, in order (`suggest`): a waypoint put since the map was opened; the
//! place the mouse points at once it has moved over the map (the pad's cross
//! once panned); the target, its name already typed into the edit box, so
//! the player sees whose spot it is (`take_suggest_target`); a waypoint from
//! before (Liav's report had one 13,000 units off); the map's middle, the
//! player's own place while it follows.
//!
//! The group's waypoint (`group_waypoint`, F3 20261026-212034, a player's
//! "Long-press square on map to set a group waypoint"; the user's choice:
//! over the group's chat): □ held `HOLD` (Q held), G, a click with Shift held
//! or a finger held still on the map asks Play to tell the group
//! (`take_group_ask`): □ held and G at □'s spot (the middle once panned, else
//! the target, else the player's own place: come to me), a click or a finger
//! at the spot under it (a marker's own spot on one). It is drawn orange under
//! the personal one with its sender's name, on the minimap and the compass
//! too, one at a time; a click on it (or □ where the personal one would go)
//! takes it away, as reaching it does.
//!
//! The markers (`markers`, F3 pass 18, the user's): EQOA-Data's XP camps, a
//! red badge with "XP", and every coach, the horseshoe the game's stable signs
//! carry, each with a card of what it is (a camp's levels, the guide's note,
//! rare creatures and drops; a coach's town, coachman and route) under the
//! mouse, under a finger that tapped it (tapped again, the waypoint goes on
//! it; a click puts it there at once) or under the pad's middle cross. X (○ on
//! a pad) shows or hides them, as Extended's Map Markers does.
//!
//! The community's markers (`community`, the user's: "Players can submit
//! waypoints to be added to the in-game map. Submissions would require
//! official approval, similar to our spellfx"): the list a GM accepted on
//! eqoaclassic, for every player on any server, a pin each with its card, shown
//! and hidden with the markers. L3 (N on a keyboard, the Suggest button)
//! suggests one: this world's waypoint, else the place under the pointer (the
//! pad's cross once the map is panned), else the map's middle; Play asks its
//! name and a note in the game's edit box (`play::mapsuggest`), and the map
//! says what the list answered.
//!
//! Over them the quest marks a server sends, or the client's own guide makes
//! on a server that sends none (`quests`, `questguide`; the user's: "add quest
//! markers for givers to the map too"): a gold "!" at a giver of a quest the
//! player may take, a "?" where a step is to be done, each with a card of its
//! quests that answers the pointer as a marker's does and before it. J (R3 on
//! a pad, the Quests button) shows or hides them, as Extended's Quest Markers
//! does.

pub mod build;
pub mod cells;
pub mod chrome;
pub mod community;
pub mod group_waypoint;
pub mod markers;
mod markers_data;
// the minimap, in play under the quick menus (`mini`)
pub mod mini;
pub mod picture;
pub mod quests;
pub mod store;
// natively (a page has nothing to call it; it compiles there as main.rs's
// commands do)
pub mod tool;
pub mod view;

use crate::pointer::{self, Glyphs, key};
use build::Builder;
use cells::{CELL, OVERVIEW_LEVELS, TILE};
use render::map::{MapLayer, MapQuad};
use render::ui::{Quad, Ui, WHOLE, frame_from_window};
use render::wgpu;
use std::collections::BTreeMap;
use view::View;
use web_time::{Duration, Instant};

/// The world's file in the library, in play (the viewer's file 0).
pub const WORLD_FILE: usize = 0;

/// Build time a frame, while the map is up and behind the game: at most
/// these, and at most this share of the frame's length, so a display faster
/// than 60 a second keeps most of its rate (at 165 the map up builds 2.1 ms
/// a frame, behind the game 0.7).
const OPEN_BUDGET: (Duration, f32) = (Duration::from_millis(6), 0.35);
const PLAY_BUDGET: (Duration, f32) = (Duration::from_millis(2), 0.12);
/// How long after a world is entered the map starts drawing behind the
/// game (the walk's own zones are read first).
const PLAY_DELAY: Duration = Duration::from_secs(5);

/// Tiles held on the GPU at once (512 across with their mips, 1.4 MB each),
/// and the scale (world units a pixel) below which they are wanted: where an
/// overview texel (15.6 units) is more than two pixels across.
const MAX_TILES: usize = 40;
const TILE_SCALE: f32 = 2000.0 / 128.0 / 2.0;

/// A zoom step (L1, R1, + and -, a wheel's notch), and the right stick's
/// and the held keys' zoom and pan a second at full push.
const ZOOM_STEP: f32 = 1.25;
const ZOOM_RATE: f32 = 3.0;
/// pan, in window heights a second
const PAN_RATE: f32 = 0.9;
/// A press moves this far (window pixels) before it is a drag.
const SLOP: f32 = 5.0;

/// Colours: outside the world, a cell not drawn yet, the grid's lines, the
/// marker and its outline.
const OUTSIDE: [f32; 4] = [0.04, 0.05, 0.07, 1.0];
const NOT_DRAWN: [f32; 4] = [0.17, 0.18, 0.21, 1.0];
const GRID: [f32; 4] = [1.0, 1.0, 1.0, 0.12];
const MARKER: [f32; 4] = [1.0, 0.86, 0.2, 1.0];
const OUTLINE: [f32; 4] = [0.0, 0.0, 0.0, 0.85];
/// The waypoint's colour on the map and on the compass: magenta, which no
/// group member's (green, blue, cyan, yellow) nor the player's marker is.
pub const WAYPOINT: [f32; 4] = [1.0, 0.3, 0.85, 1.0];
/// How near the waypoint the player must come (world units, about two
/// seconds' run) for it to be reached and taken away.
pub const ARRIVE: f32 = 15.0;

/// A group member's colour on the map and the minimap, by its index (the
/// compass's: 1 green, 2 blue, 3 cyan, the rest yellow).
pub(crate) fn member_color(index: usize) -> [f32; 4] {
    match index {
        1 => [0.0, 1.0, 0.0, 1.0],
        2 => [0.2, 0.4, 1.0, 1.0],
        3 => [0.0, 1.0, 1.0, 1.0],
        _ => [1.0, 1.0, 0.0, 1.0],
    }
}

/// CLIENT's part: `gWorldInfo`'s grid bounds and world names, by world.
#[derive(Clone, Copy, Debug, Default)]
pub struct WorldNames {
    pub bounds: [Option<[i32; 4]>; 6],
    pub names: [Option<u16>; 6],
}

impl WorldNames {
    pub fn from_image(img: &crate::client_image::ClientImage) -> Self {
        WorldNames {
            bounds: std::array::from_fn(|w| crate::client_image::world_bounds(img, w as i32)),
            names: std::array::from_fn(|w| crate::client_image::world_name_word(img, w as i32)),
        }
    }
}

/// What the map is given each frame.
pub struct Context<'a> {
    pub lib: &'a esf::Library,
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub format: wgpu::TextureFormat,
    pub size: (u32, u32),
    /// the frame's length, seconds
    pub dt: f32,
    /// the player's place and heading
    pub player: Option<([f32; 3], f32)>,
    /// `gWorldInfo`'s index of the world played in
    pub world: i32,
    /// the world's grid's origin (x, z), for the ninth
    pub origin: [f32; 2],
    /// the group's other members: their place and index (the compass's colours)
    pub group: Vec<([f32; 3], usize)>,
    /// in a group (the legend names the group's waypoint then)
    pub in_group: bool,
    /// the target's place (x, z) in the player's world while it is in view,
    /// never the player's own (□ and Suggest take it, the map rings it)
    pub target: Option<[f32; 2]>,
    /// the pad's sticks (rx, ry, lx, ly), the map's while it is up
    pub sticks: [f32; 4],
    /// a keyboard is in use (the legend names its keys, else the pad's)
    pub keyboard: bool,
    pub words: Option<&'a vi::strtab::StringTable>,
    /// the HUD's `VIUI`, once SUPPORT's atlas is here (the pad's glyphs)
    pub vui: Option<&'a vi::ui::Vui>,
    /// the quest marks a server sent (`quests`) while it offers them, or the
    /// client's own guide's on one that offers none (`questguide`), none with
    /// Quest Markers off; `None` with neither (the guide's first seconds in
    /// the world)
    pub quest_marks: Option<&'a [net::session::QuestMark]>,
}

/// A world's map.
struct World {
    /// its file's name, the disc it came off (`era`: Tunaria off the 2002
    /// disc is another map), and its `gWorldInfo` index
    name: String,
    era: esf::Era,
    index: i32,
    builder: Builder,
    /// the overview texture
    overview: Option<usize>,
    /// tiles on the GPU: (texture, the frame last wanted)
    tiles: BTreeMap<usize, (usize, u64)>,
    made: Instant,
    /// the view it was last left at
    view: Option<View>,
}

/// The GPU's part: the pictures' layer and the chrome's (ArialUI, UI.CSF's
/// art for the pad's glyphs), the chrome in the art of the library's view it
/// was made in (`era`, F9).
struct Layers {
    map: MapLayer,
    white: usize,
    ui: Ui,
    assets: crate::ui::UiAssets,
    view: esf::Era,
}

#[derive(Clone, Copy, Debug)]
struct Press {
    at: [f32; 2],
    right: bool,
    dragged: bool,
}

#[derive(Default)]
pub struct Map {
    /// up, full screen, with the keys
    pub open: bool,
    view: Option<View>,
    /// the view goes with the player
    follow: bool,
    /// held: pan up, down, left, right; zoom in, out
    held: [bool; 6],
    press: Option<Press>,
    cursor: [f32; 2],
    /// the window's size last frame
    size: (u32, u32),
    world: Option<World>,
    pub names: WorldNames,
    layers: Option<Layers>,
    /// the frame count, for the tiles' last use
    frame: u64,
    /// where the chrome's buttons and the pad's glyphs were drawn (frame
    /// points), for a pointer
    buttons: Vec<chrome::Button>,
    glyphs: Glyphs,
    /// zoom steps waiting for the window's size (a key before a frame)
    steps: i32,
    /// the last frame's line for the log
    reported: Option<(usize, usize)>,
    /// the waypoints by `gWorldInfo` index, (x, z) (`click`); read out of
    /// the settings at the first use (`read_waypoints`)
    waypoints: BTreeMap<i32, [f32; 2]>,
    waypoints_read: bool,
    /// the quest marks the server sent and the one a finger tapped (`quests`)
    pub quests: quests::QuestMarks,
    /// the marker whose card a finger tapped up (`click`), and whether the
    /// mouse is over the map (its cursor's card); `hover_world`, a world
    /// point taken as the pointer's (`mapshot`'s `EQOA_MAP_HOVER`)
    pinned: Option<markers::Marker>,
    pointer_seen: bool,
    pub hover_world: Option<[f32; 2]>,
    /// the coaches' horseshoe, off the disc (`find_icon`)
    icon: markers::Icon,
    /// the build's time a frame, open or not, in place of the usual
    /// (`EQOA_MAP_BUDGET` in ms, `mapshot`'s measurements)
    pub budget: Option<Duration>,
    /// the minimap's own (`mini`): its zoom, whether it is drawn this
    /// frame, where, and the tiles it keeps on the GPU
    pub mini: mini::Mini,
    /// the community's markers, their asks and a suggestion's (`community`)
    pub community: community::Community,
    /// a keyboard was in use last frame (a pad's cross is not then)
    keyboard: bool,
    /// the personal waypoint was put while the map is up this time (Suggest
    /// takes it first, `suggest`)
    waypoint_fresh: bool,
    /// the target's place and the player's world, as the last frame had them
    /// (`Context::target`, `Context::world`): □'s spot and Suggest's
    target: Option<[f32; 2]>,
    player_world: i32,
    /// Suggest's spot was the target's (Play types its name into the box)
    suggest_target: bool,
    /// □ held: for how long (seconds, by the clock) and whether its long
    /// press has gone (`hold`)
    square_hold: Option<(f32, bool)>,
    /// Shift is held (the viewer's): a click puts the group's waypoint
    pub shift: bool,
    /// the group's waypoint, and the spot the player asked the group to be
    /// told of (`take_group_ask`)
    group_mark: Option<group_waypoint::GroupMark>,
    group_ask: Option<community::Spot>,
    /// the player has been farther than `ARRIVE` from the group's waypoint
    /// since it came: only then is it reached (the sender's own, put where
    /// they stand, would go at once)
    group_armed: bool,
    /// when the player last told the group of one (the session's clock, ms;
    /// `group_wait`)
    group_told: Option<u32>,
}

/// G on a keyboard while the map is up (`keyboard`): the group's waypoint at
/// □'s spot at once. No pad key has it; □ held is the pad's.
pub const KEY_GROUP: i32 = 0x7E31_0001;

impl Map {
    /// Opens the map on the player's area (the view it was last left at, or
    /// the opening one, centred on the player). The mouse points at nothing
    /// until it moves over the map, and no waypoint is this time's yet
    /// (Suggest's spot, `suggest`).
    pub fn show(&mut self) {
        self.open = true;
        self.follow = true;
        self.held = [false; 6];
        self.press = None;
        self.pointer_seen = false;
        self.waypoint_fresh = false;
        self.square_hold = None;
    }

    /// Closes the map; its tiles leave the GPU (they come back from the
    /// cache when it is opened again), the overview stays.
    pub fn close(&mut self) {
        self.open = false;
        self.held = [false; 6];
        self.press = None;
        self.square_hold = None;
        let tiles = self.world.as_mut().map(|w| std::mem::take(&mut w.tiles)).unwrap_or_default();
        if let Some(l) = self.layers.as_mut() {
            for (t, _) in tiles.into_values() {
                l.map.free(t);
            }
        }
    }

    /// A key (the pad's codes, `SetKey`'s: 128 + the field) while the map is
    /// up. Whether the map took it (every key, while it is up).
    pub fn key(&mut self, k: i32, down: bool) -> bool {
        if !self.open {
            return false;
        }
        let hold = |held: &mut [bool; 6], i: usize| held[i] = down;
        match k {
            key::TRIANGLE | key::START if down => self.close(),
            key::CROSS if down => self.follow = true,
            key::SQUARE => self.square_key(down),
            KEY_GROUP if down => self.group_square(),
            key::CIRCLE if down => self.toggle_markers(),
            key::R3 if down => self.toggle_quests(),
            key::SELECT if down => self.toggle_towns(),
            key::L3 if down => self.suggest(),
            key::R1 if down => self.steps -= 1,
            key::L1 if down => self.steps += 1,
            key::UP => hold(&mut self.held, 0),
            key::DOWN => hold(&mut self.held, 1),
            key::LEFT => hold(&mut self.held, 2),
            key::RIGHT => hold(&mut self.held, 3),
            key::R2 => hold(&mut self.held, 4),
            key::L2 => hold(&mut self.held, 5),
            _ => {}
        }
        true
    }

    /// A mouse button on the map: a drag pans it; let go undragged, a click
    /// (a right click is △, as on every window).
    pub fn mouse_button(&mut self, right: bool, down: bool, at: [f32; 2]) -> Option<crate::mouse::Click> {
        self.cursor = at;
        if down {
            if self.press.is_none() {
                self.press = Some(Press { at, right, dragged: false });
            }
            return None;
        }
        let p = self.press.filter(|p| p.right == right)?;
        self.press = None;
        if p.dragged {
            return None;
        }
        let (x, y) = (p.at[0] as f64, p.at[1] as f64);
        Some(if right { crate::mouse::Click::Right(x, y) } else { crate::mouse::Click::Target(x, y) })
    }

    /// The pointer moved: a held button past the slop drags the map.
    pub fn mouse_moved(&mut self, at: [f32; 2]) {
        let d = [at[0] - self.cursor[0], at[1] - self.cursor[1]];
        self.cursor = at;
        self.pointer_seen = true;
        if let Some(p) = self.press.as_mut() {
            if p.dragged {
                self.drag(d);
            } else if ((at[0] - p.at[0]).powi(2) + (at[1] - p.at[1]).powi(2)).sqrt() > SLOP {
                // from where it was pressed: what was under the pointer stays there
                p.dragged = true;
                let from = [at[0] - p.at[0], at[1] - p.at[1]];
                self.drag(from);
            }
        }
    }

    /// The pointer left, or the window lost the focus (□ let go with it: no
    /// release comes for it).
    pub fn let_go(&mut self) {
        self.press = None;
        self.held = [false; 6];
        self.pointer_seen = false;
        self.square_hold = None;
    }

    /// The wheel's `lines` (up positive: in) about window pixel `at` (the
    /// middle when none): the point under the pointer stays under it, so
    /// the map stops following the player unless the pointer is on it.
    pub fn wheel(&mut self, lines: f32, at: Option<[f32; 2]>) {
        if lines == 0.0 {
            return;
        }
        let size = self.size;
        let middle = [size.0 as f32 / 2.0, size.1 as f32 / 2.0];
        let at = at.unwrap_or(middle);
        if (at[0] - middle[0]).abs() > SLOP || (at[1] - middle[1]).abs() > SLOP {
            self.follow = false;
        }
        self.zoom(ZOOM_STEP.powf(-lines), at);
    }

    /// A finger's drag (window pixels) and a pinch (the wheel's lines, apart
    /// positive: `touch.rs`'s, a tenth closer a line, so the map follows the
    /// fingers).
    pub fn touch(&mut self, dx: f32, dy: f32, pinch: f32) {
        if dx != 0.0 || dy != 0.0 || pinch != 0.0 {
            // a finger, no mouse: no cursor's card
            self.pointer_seen = false;
        }
        if dx != 0.0 || dy != 0.0 {
            self.drag([dx, dy]);
        }
        if pinch != 0.0 {
            let size = self.size;
            self.zoom(0.9f32.powf(pinch), [size.0 as f32 / 2.0, size.1 as f32 / 2.0]);
        }
    }

    fn drag(&mut self, d: [f32; 2]) {
        if let Some(v) = self.view.as_mut() {
            v.drag(d);
            self.follow = false;
        }
    }

    fn zoom(&mut self, k: f32, at: [f32; 2]) {
        let size = self.size;
        let Some(extent) = self.world.as_ref().map(|w| w.builder.cells.extent()) else { return };
        if let Some(v) = self.view.as_mut() {
            v.zoom(k, at, size, view::limits(extent, size));
        }
    }

    /// What a pointer at frame point `f` presses on the map's chrome: a
    /// button's key, the pad glyph's (a finger may miss by a little);
    /// nothing on the map itself (a drag pans it, a click is `click`'s).
    pub fn point(&self, f: [f32; 2], finger: bool) -> Vec<i32> {
        let slop = if finger { pointer::FINGER } else { 0.0 };
        let on = |r: [i32; 4], s: f32| f[0] >= r[0] as f32 - s && f[0] < r[2] as f32 + s && f[1] >= r[1] as f32 - s && f[1] < r[3] as f32 + s;
        let button = |s: f32| self.buttons.iter().rev().find(|b| on(b.0, s)).map(|b| b.1);
        match self.glyphs.at(f, slop).or_else(|| button(0.0)).or_else(|| button(slop)) {
            Some(k) => vec![k],
            None => Vec::new(),
        }
    }

    /// A click or a tap at window pixel `s` on the map itself (not on its
    /// chrome: `point` found nothing there): on the waypoint (within
    /// `view::waypoint_reach`, a finger's miss more) it takes it away, else
    /// on the group's it takes that away, else it puts this world's waypoint
    /// there, on the world's ground. With Shift held it asks the group to be
    /// told of the spot under it instead (`spot_under`). Whether it did any.
    pub fn click(&mut self, s: [f32; 2], finger: bool) -> bool {
        let (true, Some(v), Some(w)) = (self.open, self.view, self.world.as_ref()) else { return false };
        let (world, extent, size) = (w.index, w.builder.cells.extent(), self.size);
        if self.shift && !finger {
            let Some(at) = self.spot_under(s, finger) else { return false };
            self.ask_group(world, at);
            return true;
        }
        self.read_waypoints();
        if let Some(at) = self.waypoints.get(&world) {
            let p = v.to_window(size, *at);
            if (p[0] - s[0]).hypot(p[1] - s[1]) <= view::waypoint_reach(size, finger) {
                self.set_waypoint(world, None);
                return true;
            }
        }
        if let Some(g) = self.group_mark.as_ref().filter(|g| g.world == world) {
            let p = v.to_window(size, g.at);
            if (p[0] - s[0]).hypot(p[1] - s[1]) <= view::waypoint_reach(size, finger) {
                self.clear_group_waypoint("taken away");
                return true;
            }
        }
        // a quest mark (`quests`), over the markers, as a marker is: a tap
        // shows its card, a click (or a second tap) puts the waypoint on it
        if let Some(i) = self.quests.under(world, v, size, s, view::marker_reach(size, finger)) {
            self.pinned = None;
            if finger && self.quests.pinned != Some(i) {
                self.quests.pinned = Some(i);
                return true;
            }
            self.quests.pinned = None;
            let at = [self.quests.marks[i].x, self.quests.marks[i].z];
            self.set_waypoint(world, Some(at));
            return true;
        }
        self.quests.pinned = None;
        // a community marker (`community`), over the camps and coaches, as a
        // marker is
        if let Some(i) = crate::settings::map_markers().then(|| self.community.under(world, v, size, s, view::marker_reach(size, finger), self.pin_size(v))).flatten() {
            self.pinned = None;
            if finger && self.community.pinned != Some(i) {
                self.community.pinned = Some(i);
                return true;
            }
            self.community.pinned = None;
            let at = self.community.marks[i].at;
            self.set_waypoint(world, Some(at));
            return true;
        }
        self.community.pinned = None;
        // a marker: a tap shows its card, a click (or a second tap) puts the
        // waypoint on it
        if let Some(m) = crate::settings::map_markers().then(|| markers::under(world, v, size, s, finger)).flatten() {
            if finger && self.pinned != Some(m) {
                self.pinned = Some(m);
                return true;
            }
            self.pinned = None;
            self.set_waypoint(world, Some(m.at()));
            return true;
        }
        self.pinned = None;
        let at = v.to_world(size, s);
        if at[0] < extent[0] || at[0] > extent[2] || at[1] < extent[1] || at[1] > extent[3] {
            return false;
        }
        self.set_waypoint(world, Some(at));
        true
    }

    /// The spot a click or a tap at window pixel `s` names, as `click` puts
    /// the waypoint: a quest mark's, a community marker's or a camp's or
    /// coach's under it (those two while the markers show), else the
    /// ground's within the world's edge; none past it.
    fn spot_under(&self, s: [f32; 2], finger: bool) -> Option<[f32; 2]> {
        let (Some(v), Some(w)) = (self.view, self.world.as_ref()) else { return None };
        let (world, extent, size) = (w.index, w.builder.cells.extent(), self.size);
        let reach = view::marker_reach(size, finger);
        if let Some(i) = self.quests.under(world, v, size, s, reach) {
            return Some([self.quests.marks[i].x, self.quests.marks[i].z]);
        }
        if crate::settings::map_markers() {
            if let Some(i) = self.community.under(world, v, size, s, reach, self.pin_size(v)) {
                return Some(self.community.marks[i].at);
            }
            if let Some(m) = markers::under(world, v, size, s, finger) {
                return Some(m.at());
            }
        }
        let at = v.to_world(size, s);
        (at[0] >= extent[0] && at[0] <= extent[2] && at[1] >= extent[1] && at[1] <= extent[3]).then_some(at)
    }

    /// A finger held still on the map at window pixel `s` (`touch`'s long
    /// press, not on the chrome): the group asked to be told of the spot
    /// under it, as a click with Shift held asks. Whether it asked.
    pub fn long_press(&mut self, s: [f32; 2]) -> bool {
        let (true, Some(w)) = (self.open, self.world.as_ref()) else { return false };
        let world = w.index;
        let f = frame_from_window(self.size, s);
        if !self.point(f, true).is_empty() {
            return false;
        }
        let Some(at) = self.spot_under(s, true) else { return false };
        self.ask_group(world, at);
        true
    }

    /// The group to be told of a spot (Play's, `take_group_ask`).
    fn ask_group(&mut self, world: i32, at: [f32; 2]) {
        self.group_ask = Some(community::Spot { world, at });
    }

    /// The spot the player asked the group to be told of, once: Play says it
    /// in the group's chat (`play::groupmark`).
    pub fn take_group_ask(&mut self) -> Option<community::Spot> {
        self.group_ask.take()
    }

    /// A line under the map's title for a few seconds (the community list's
    /// answers' place).
    pub fn notice(&mut self, line: &str) {
        self.community.notice(line);
    }

    /// How long (ms) before the player may tell the group of another
    /// waypoint, `group_waypoint::EVERY_MS` after the last (`now` the
    /// session's clock); none when they may.
    pub fn group_wait(&self, now: u32) -> Option<u32> {
        let gone = now.wrapping_sub(self.group_told?);
        (gone < group_waypoint::EVERY_MS).then(|| group_waypoint::EVERY_MS - gone)
    }

    /// The player told the group of a waypoint at `now` (`group_wait`).
    pub fn group_told(&mut self, now: u32) {
        self.group_told = Some(now);
    }

    /// The group's waypoint now (`group_waypoint`): another, or the first.
    pub fn set_group_waypoint(&mut self, mark: group_waypoint::GroupMark) {
        crate::say(&format!("map: the group's waypoint at ({:.0}, {:.0}) in world {}, from {}", mark.at[0], mark.at[1], mark.world, mark.from));
        self.group_mark = Some(mark);
        self.group_armed = false;
    }

    /// The group's waypoint gone (`why`, for the log).
    pub fn clear_group_waypoint(&mut self, why: &str) {
        if let Some(g) = self.group_mark.take() {
            crate::say(&format!("map: the group's waypoint from {} {why}", g.from));
        }
    }

    /// The group's waypoint and who put it.
    pub fn group_mark(&self) -> Option<&group_waypoint::GroupMark> {
        self.group_mark.as_ref()
    }

    /// The group's waypoint in world `world` (x, z): the compass's dot.
    pub fn group_waypoint(&self, world: i32) -> Option<[f32; 2]> {
        self.group_mark.as_ref().filter(|g| g.world == world).map(|g| g.at)
    }

    /// The group's waypoint kept while the player is in a group and its
    /// sender is a member of it (`is_member`, by name): gone when either is
    /// no longer so.
    pub fn keep_group_waypoint(&mut self, in_group: bool, is_member: impl Fn(&str) -> bool) {
        if self.group_mark.as_ref().is_some_and(|g| !in_group || !is_member(&g.from)) {
            self.clear_group_waypoint(if in_group { "went: its sender left the group" } else { "went: out of the group" });
        }
    }

    /// World `world`'s name as the map's title has it: `gWorldInfo`'s word,
    /// else, for the world mapped (one past CLIENT's six), its file's.
    pub fn world_title(&self, world: i32, words: Option<&vi::strtab::StringTable>) -> Option<String> {
        let word = self.names.names.get(usize::try_from(world).ok()?).copied().flatten();
        if let Some(n) = word.and_then(|n| words.map(|t| t.get(n as i32, 0).to_string())).filter(|s| s != "(null)" && !s.is_empty()) {
            return Some(n);
        }
        let w = self.world.as_ref().filter(|w| w.index == world)?;
        Some(title_case(w.name.split('.').next().unwrap_or(""))).filter(|s| !s.is_empty())
    }

    /// The world a name names (`world_title`'s, in any case): one of
    /// CLIENT's six, else the world mapped.
    pub fn world_named(&self, name: &str, words: Option<&vi::strtab::StringTable>) -> Option<i32> {
        let mut worlds: Vec<i32> = (0..6).collect();
        if let Some(w) = self.world.as_ref().filter(|w| !(0..6).contains(&w.index)) {
            worlds.push(w.index);
        }
        worlds.into_iter().find(|&i| self.world_title(i, words).is_some_and(|t| t.eq_ignore_ascii_case(name)))
    }

    /// The area a spot is in: its cell's name, as the map's names and title
    /// have it (`cells::name`), when there is one.
    pub fn area_of(&self, spot: community::Spot, words: Option<&vi::strtab::StringTable>) -> Option<String> {
        let bounds = self.names.bounds.get(usize::try_from(spot.world).ok()?).copied().flatten();
        let n = cells::name(spot.world, bounds, (spot.at[0] / CELL) as i32, (spot.at[1] / CELL) as i32)?;
        words.map(|t| t.get(n as i32, 0).to_string()).filter(|s| s != "(null)" && !s.is_empty())
    }

    /// What the group is told of `spot` (`group_waypoint::line`): none for a
    /// world the map cannot name.
    pub fn group_line(&self, spot: community::Spot, words: Option<&vi::strtab::StringTable>) -> Option<String> {
        let world = self.world_title(spot.world, words)?;
        Some(group_waypoint::line(&world, spot.at, self.area_of(spot, words).as_deref()))
    }

    /// ○ (X on a keyboard): the markers shown or hidden, kept (Extended's
    /// Map Markers).
    fn toggle_markers(&mut self) {
        let on = !crate::settings::map_markers();
        if let Err(e) = crate::settings::set_map_markers(on) {
            crate::say(&format!("map: markers {}, not kept: {e}", if on { "shown" } else { "hidden" }));
        }
        self.pinned = None;
        crate::say(&format!("map: markers {}", if on { "shown" } else { "hidden" }));
    }

    /// R3 (J on a keyboard, the Quests button): the quest marks shown or
    /// hidden, kept (Extended's Quest Markers); hidden, the server is told to
    /// send none (`Play`'s `agree_quest_marks`).
    fn toggle_quests(&mut self) {
        let on = !crate::settings::quest_markers();
        if let Err(e) = crate::settings::set_quest_markers(on) {
            crate::say(&format!("map: quest marks {}, not kept: {e}", if on { "shown" } else { "hidden" }));
        }
        if !on {
            self.quests.set(&[]);
        }
        self.quests.pinned = None;
        crate::say(&format!("map: quest marks {}", if on { "shown" } else { "hidden" }));
    }

    /// The towns' names shown or hidden (T, Select), and kept.
    fn toggle_towns(&mut self) {
        let on = !crate::settings::map_towns();
        if let Err(e) = crate::settings::set_map_towns(on) {
            crate::say(&format!("map: town names {}, not kept: {e}", if on { "shown" } else { "hidden" }));
        }
        crate::say(&format!("map: town names {}", if on { "shown" } else { "hidden" }));
    }

    /// The marker whose card shows: the one a finger tapped, else the one
    /// under the mouse (not while it drags the map), else, for a pad, the
    /// one under the middle cross once the map is panned; none while the
    /// markers are hidden.
    fn card_marker(&self, keyboard: bool) -> Option<markers::Marker> {
        let (true, Some(v), Some(w)) = (crate::settings::map_markers(), self.view, self.world.as_ref()) else { return None };
        let size = self.size;
        if let Some(m) = self.pinned.filter(|m| m.world() == w.index) {
            return Some(m);
        }
        if let Some(p) = self.hover_world {
            return markers::under(w.index, v, size, v.to_window(size, p), false);
        }
        if self.pointer_seen
            && !self.press.is_some_and(|p| p.dragged)
            && let Some(m) = markers::under(w.index, v, size, self.cursor, false)
        {
            return Some(m);
        }
        if !self.follow && !keyboard {
            return markers::under(w.index, v, size, [size.0 as f32 / 2.0, size.1 as f32 / 2.0], false);
        }
        None
    }

    /// The world's place (x, z) the chrome names under its title: under the
    /// mouse while it is over the map, else, for a pad, under the middle
    /// cross once the map is panned (as `card_marker` picks a card's).
    fn pointer_world(&self, keyboard: bool) -> Option<[f32; 2]> {
        let v = self.view?;
        let size = self.size;
        if let Some(p) = self.hover_world {
            return Some(p);
        }
        if self.pointer_seen {
            return Some(v.to_world(size, self.cursor));
        }
        (!self.follow && !keyboard).then(|| v.to_world(size, [size.0 as f32 / 2.0, size.1 as f32 / 2.0]))
    }

    /// The quest mark whose card shows (`quests`), as `card_marker` picks a
    /// marker's: the one a finger tapped, else the one under the mouse (not
    /// while it drags the map), else, for a pad, the one under the middle
    /// cross once the map is panned.
    fn card_quest(&self, keyboard: bool) -> Option<usize> {
        let (Some(v), Some(w)) = (self.view, self.world.as_ref()) else { return None };
        let (size, world) = (self.size, w.index);
        let reach = view::marker_reach(size, false);
        if let Some(i) = self.quests.pinned.filter(|&i| self.quests.marks.get(i).is_some_and(|m| m.world as i32 == world)) {
            return Some(i);
        }
        if let Some(p) = self.hover_world {
            return self.quests.under(world, v, size, v.to_window(size, p), reach);
        }
        if self.pointer_seen
            && !self.press.is_some_and(|p| p.dragged)
            && let Some(i) = self.quests.under(world, v, size, self.cursor, reach)
        {
            return Some(i);
        }
        if !self.follow && !keyboard {
            return self.quests.under(world, v, size, [size.0 as f32 / 2.0, size.1 as f32 / 2.0], reach);
        }
        None
    }

    /// The community marker whose card shows (`community`), as `card_quest`
    /// picks a quest mark's; none while the markers are hidden.
    fn card_community(&self, keyboard: bool) -> Option<usize> {
        let (true, Some(v), Some(w)) = (crate::settings::map_markers(), self.view, self.world.as_ref()) else { return None };
        let (size, world) = (self.size, w.index);
        let (reach, r) = (view::marker_reach(size, false), self.pin_size(v));
        if let Some(i) = self.community.pinned.filter(|&i| self.community.marks.get(i).is_some_and(|m| m.world == world)) {
            return Some(i);
        }
        if let Some(p) = self.hover_world {
            return self.community.under(world, v, size, community::head(v.to_window(size, p), r), reach, r);
        }
        if self.pointer_seen
            && !self.press.is_some_and(|p| p.dragged)
            && let Some(i) = self.community.under(world, v, size, self.cursor, reach, r)
        {
            return Some(i);
        }
        if !self.follow && !keyboard {
            let m = [size.0 as f32 / 2.0, size.1 as f32 / 2.0];
            return self.community.under(world, v, size, community::head(m, r), reach, r);
        }
        None
    }

    /// A pin's size in this view (`view::pin_radius`): smaller far out.
    fn pin_size(&self, v: View) -> f32 {
        view::pin_radius(self.size, v.scale > markers::COMPACT_FROM).0
    }

    /// The card to show, a quest mark's or a marker's: the quest mark's
    /// (`card_quest`) first, the marks being over the markers, but a marker
    /// a finger tapped keeps its card.
    fn cards(&self, keyboard: bool) -> (Option<usize>, Option<markers::Marker>) {
        let marker = self.card_marker(keyboard);
        let quest = self.card_quest(keyboard).filter(|&i| self.quests.pinned == Some(i) || marker.is_none() || marker != self.pinned);
        if quest.is_some() { (quest, None) } else { (None, marker) }
    }

    /// The coaches' horseshoe: the map's cache first (`ICON_FILE` under
    /// "icons", kept from an earlier run), else off the disc: a file of the
    /// library holding it (Freeport's zones round the player), a zone the
    /// builder reads, or, Tunaria's map drawn and still none, the sign's zone
    /// read once for it (`Builder::peek`); then onto the layer. Off the 2002
    /// disc's Tunaria none is read; the drawn horseshoe stands in meanwhile.
    fn find_icon(&mut self, cx: &Context) {
        let icon = &mut self.icon;
        if icon.image.is_none() && icon.store.is_none() && !icon.answered {
            icon.store = store::open("icons");
            match icon.store.as_mut() {
                Some(s) => s.get(markers::ICON_FILE),
                None => icon.answered = true,
            }
        }
        if let Some(s) = icon.store.as_mut() {
            let mut got = Vec::new();
            while let Some(g) = s.poll() {
                got.push(g);
            }
            for g in got {
                if let store::Got::File(name, bytes) = g
                    && name == markers::ICON_FILE
                {
                    icon.cached(bytes);
                }
            }
        }
        if icon.image.is_none()
            && icon.answered
            && let Some((c, s)) = cx.lib.surface(markers::ICON)
            && let Some(im) = s.decode(&c.data)
        {
            icon.take(&im, "off the disc, out of a zone in play");
        }
        if let Some(w) = self.world.as_mut() {
            let tunaria = w.name.eq_ignore_ascii_case("TUNARIA.ESF") && w.era == esf::Era::Frontiers;
            w.builder.want_icon(icon.image.is_none() && tunaria);
            if let Some(im) = w.builder.icon.take() {
                icon.take(&im, "off the disc, out of a zone the map read");
            }
            if icon.image.is_none() && icon.answered && tunaria && !icon.peeked && w.builder.done() {
                icon.peeked = true;
                if let Some(zi) = w.builder.cells.at(markers::ICON_AT[0], markers::ICON_AT[1]).and_then(|c| w.builder.cells.zone_at[c]) {
                    w.builder.peek(zi);
                }
            }
        }
        if let (Some((iw, ih, px)), None, Some(l)) = (icon.image.as_ref(), icon.tex, self.layers.as_mut()) {
            icon.tex = Some(l.ui.add_texture(cx.device, cx.queue, *iw, *ih, px));
        }
    }

    /// L3 (N on a keyboard, the Suggest button): a spot of this world asked
    /// to be suggested to the community's list (`community`), the one the
    /// player chose last: this world's waypoint put since the map was
    /// opened; else the place under the pointer (not a pointer on the
    /// chrome: the button's own) or under a pad's cross once the map is
    /// panned (`pointer_world`); else, while the map follows the player, the
    /// target's spot (`target_spot`); else a waypoint from before; else the
    /// map's middle. Play takes it (`take_suggest`) and asks its name, the
    /// target's already typed when the spot is the target's
    /// (`take_suggest_target`).
    fn suggest(&mut self) {
        let (Some(v), Some(w)) = (self.view, self.world.as_ref()) else { return };
        let world = w.index;
        self.read_waypoints();
        let waypoint = self.waypoints.get(&world).copied();
        let pointer = if self.on_chrome() { None } else { self.pointer_world(self.keyboard) };
        let target = self.target_spot().filter(|_| self.follow);
        let at = waypoint.filter(|_| self.waypoint_fresh).or(pointer).or(target).or(waypoint).unwrap_or(v.center);
        self.suggest_target = target.is_some_and(|t| (t[0] - at[0]).hypot(t[1] - at[1]) < 0.5);
        self.community.ask(community::Spot { world, at });
    }

    /// Whether the spot the map last asked to suggest is the target's
    /// (Play types the target's name into the box), once.
    pub fn take_suggest_target(&mut self) -> bool {
        std::mem::take(&mut self.suggest_target)
    }

    /// Whether the mouse is over the chrome's buttons (`point`'s).
    fn on_chrome(&self) -> bool {
        if !self.pointer_seen {
            return false;
        }
        let f = frame_from_window(self.size, self.cursor);
        self.buttons.iter().any(|b| f[0] >= b.0[0] as f32 && f[0] < b.0[2] as f32 && f[1] >= b.0[1] as f32 && f[1] < b.0[3] as f32)
    }

    /// The spot the map asked to suggest, once (`suggest`): Play's to ask
    /// its name (`play::mapsuggest`).
    pub fn take_suggest(&mut self) -> Option<community::Spot> {
        self.community.take_spot()
    }

    /// Whether the map follows the player.
    pub fn following(&self) -> bool {
        self.follow
    }

    /// The map up again where it was (after the edit box a suggestion took):
    /// its view kept, following the player or not as it was.
    pub fn resume(&mut self, follow: bool) {
        self.show();
        self.follow = follow;
    }

    /// The map panned off the player (`false`) or following it, for a test
    /// beside it (`play::mapsuggest`).
    #[cfg(test)]
    pub fn follow_for_test(&mut self, on: bool) {
        self.follow = on;
    }

    /// □ pressed or let go (Q on a keyboard, held as it is: `keyboard`): let
    /// go before `group_waypoint::HOLD` it is □'s waypoint (`square`); held
    /// that long, the group's (`hold`, by the clock), and its release
    /// nothing. A repeat while it is down is nothing.
    fn square_key(&mut self, down: bool) {
        match (down, self.square_hold) {
            (true, None) => self.square_hold = Some((0.0, false)),
            (true, Some(_)) => {}
            (false, Some((_, false))) => {
                self.square_hold = None;
                self.square();
            }
            (false, _) => self.square_hold = None,
        }
    }

    /// □ held on by `dt` seconds of the clock (every frame drawn while the
    /// map is up): at `group_waypoint::HOLD` the group's waypoint, once.
    fn hold(&mut self, dt: f32) {
        let Some((held, false)) = self.square_hold else { return };
        let held = held + dt.max(0.0);
        let long = held >= group_waypoint::HOLD;
        self.square_hold = Some((held, long));
        if long {
            self.group_square();
        }
    }

    /// □ (Q on a keyboard), the spot chosen on the map first: once the map
    /// is panned off the player, the waypoint at its middle (taken away when
    /// it is there, or the group's there taken away); while the map follows
    /// the player, on the target's spot while one is in view (taken away when
    /// it is there already), else taken away (the middle is the player,
    /// where a waypoint is already reached), the group's when there is no
    /// waypoint of the player's own.
    fn square(&mut self) {
        let (Some(v), Some(w)) = (self.view, self.world.as_ref()) else { return };
        let (world, size) = (w.index, self.size);
        self.read_waypoints();
        let reach = view::waypoint_reach(size, false);
        let near = |a: [f32; 2], b: [f32; 2]| {
            let (p, q) = (v.to_window(size, a), v.to_window(size, b));
            (p[0] - q[0]).hypot(p[1] - q[1]) <= reach
        };
        let mine = self.waypoints.get(&world).copied();
        let group = self.group_waypoint(world);
        if !self.follow {
            if mine.is_some_and(|m| near(m, v.center)) {
                self.set_waypoint(world, None);
            } else if group.is_some_and(|g| near(g, v.center)) {
                self.clear_group_waypoint("taken away");
            } else {
                self.set_waypoint(world, Some(v.center));
            }
        } else if let Some(t) = self.target_spot() {
            let at = if mine.is_some_and(|m| near(m, t)) { None } else { Some(t) };
            self.set_waypoint(world, at);
        } else if mine.is_some() {
            self.set_waypoint(world, None);
        } else if group.is_some() {
            self.clear_group_waypoint("taken away");
        }
    }

    /// □ held (`hold`) or G: the group asked to be told of □'s spot (Play's,
    /// `take_group_ask`): the map's middle once it is panned, else the
    /// target's spot, else the player's own place (the middle while the map
    /// follows the player: come to me).
    fn group_square(&mut self) {
        let (Some(v), Some(w)) = (self.view, self.world.as_ref()) else { return };
        let world = w.index;
        let at = if self.follow { self.target_spot().unwrap_or(v.center) } else { v.center };
        self.ask_group(world, at);
    }

    /// The target's place (x, z) while it is in view in the player's world
    /// and that is the world mapped (the last frame's, `Context::target`).
    fn target_spot(&self) -> Option<[f32; 2]> {
        let w = self.world.as_ref()?;
        self.target.filter(|_| w.index == self.player_world)
    }

    /// The target's place as the map has it, for a test beside it
    /// (`play::groupmark`).
    #[cfg(test)]
    pub fn target_for_test(&mut self, at: Option<[f32; 2]>) {
        self.target = at;
    }

    /// □ held on by `dt` seconds (`hold`), for a test beside it.
    #[cfg(test)]
    pub fn hold_for_test(&mut self, dt: f32) {
        self.hold(dt);
    }

    /// The waypoint of world `world` (`gWorldInfo`'s index), (x, z), when
    /// one is set: the compass's dot.
    pub fn waypoint(&self, world: i32) -> Option<[f32; 2]> {
        self.waypoints.get(&world).copied()
    }

    /// The waypoints out of the settings, the first time they are wanted.
    fn read_waypoints(&mut self) {
        if !self.waypoints_read {
            self.waypoints_read = true;
            self.waypoints = crate::settings::waypoints().into_iter().collect();
        }
    }

    /// World `world`'s waypoint put at `at` (none: taken away), and kept;
    /// put, it is this time's (`suggest`).
    fn set_waypoint(&mut self, world: i32, at: Option<[f32; 2]>) {
        self.read_waypoints();
        self.waypoint_fresh = at.is_some();
        match at {
            Some(p) => {
                self.waypoints.insert(world, p);
                crate::say(&format!("map: waypoint at ({:.0}, {:.0}) in world {world}", p[0], p[1]));
            }
            None => {
                if self.waypoints.remove(&world).is_none() {
                    return;
                }
                crate::say(&format!("map: waypoint in world {world} taken away"));
            }
        }
        let list: Vec<(i32, [f32; 2])> = self.waypoints.iter().map(|(&w, &p)| (w, p)).collect();
        crate::settings::save_waypoints(&list);
    }

    /// The waypoint of the world the player is in taken away once the player
    /// comes within `ARRIVE` of it; the group's too, once the player has been
    /// farther than that from it since it came (`group_armed`).
    fn arrive(&mut self, player: Option<([f32; 3], f32)>, world: i32) {
        let Some((p, _)) = player else { return };
        let near = |at: [f32; 2]| (p[0] - at[0]).hypot(p[2] - at[1]) <= ARRIVE;
        if let Some(at) = self.group_waypoint(world) {
            if !near(at) {
                self.group_armed = true;
            } else if self.group_armed {
                self.clear_group_waypoint("reached");
            }
        }
        let Some(at) = self.waypoints.get(&world).copied() else { return };
        if near(at) {
            crate::say("map: waypoint reached");
            self.set_waypoint(world, None);
        }
    }

    /// CLIENT's part, once its image is here.
    pub fn client_data(&mut self, img: &crate::client_image::ClientImage) {
        self.names = WorldNames::from_image(img);
    }

    /// One frame: the world's map made or carried on (a few milliseconds of
    /// its build, behind the game too), and while it is up the view moved by
    /// what is held and its quads made.
    pub fn frame(&mut self, cx: &Context) {
        self.frame += 1;
        self.size = cx.size;
        self.keyboard = cx.keyboard;
        self.target = cx.target;
        self.player_world = cx.world;
        if self.open {
            self.hold(cx.dt);
        }
        self.quests.set(cx.quest_marks.unwrap_or_default());
        self.community.frame(self.open);
        self.read_waypoints();
        self.arrive(cx.player, cx.world);
        let name = cx.lib.files.get(WORLD_FILE).map(|f| f.0.clone()).unwrap_or_default();
        let era = cx.lib.era_of(WORLD_FILE);
        let r = renew(self.layers.as_ref().map(|l| l.view), cx.lib.view(), self.world.as_ref().map(|w| (w.name.as_str(), w.era)), &name, era);
        let looking = self.view;
        if r.layers {
            // their textures go with them, the world's pictures among them
            self.layers = None;
            self.world = None;
            self.icon.tex = None;
        }
        self.layers(cx);
        if r.world {
            self.new_world(cx, name);
            if r.keep_view && let Some(w) = self.world.as_mut() {
                self.view = looking;
                w.view = looking;
            }
        }
        let Some(w) = self.world.as_mut() else { return };
        // the build: harder while the map is up, behind the game a while after the world was entered
        let focus = match (self.open, self.view, cx.player) {
            (true, Some(v), _) => v.center,
            (_, _, Some((p, _))) => [p[0], p[2]],
            _ => w.builder.cells.extent()[..2].try_into().unwrap(),
        };
        let budget = match self.budget {
            Some(b) => Some(b),
            None if self.open => Some(budget(OPEN_BUDGET, cx.dt)),
            None => (w.made.elapsed() >= PLAY_DELAY).then(|| budget(PLAY_BUDGET, cx.dt)),
        };
        if let Some(b) = budget {
            w.builder.step(cx.lib, &build::Gpu { device: cx.device, queue: cx.queue }, focus, b);
        }
        self.find_icon(cx);
        let Some(w) = self.world.as_mut() else { return };
        let progress = w.builder.progress();
        if w.builder.done() && self.reported != Some(progress) {
            crate::say(&format!("{} in {:.1} s ({:.2} s of it working, the longest frame's {:.1} ms)", w.builder.describe(), w.builder.stats.took.unwrap_or_default().as_secs_f64(), w.builder.stats.busy.as_secs_f64(), w.builder.stats.worst.as_secs_f64() * 1e3));
            self.reported = Some(progress);
        }
        // what the build made, onto the GPU (the minimap's tiles too)
        if let Some(l) = self.layers.as_mut() {
            Self::take_pictures(w, l, cx, self.open, self.view, self.frame, &self.mini.wanted);
        }
        if !self.open {
            return;
        }
        self.steer(cx.size, cx.dt, cx.sticks, cx.player.map(|p| [p.0[0], p.0[2]]));
        self.want_tiles(cx.size);
        self.compose(cx);
    }

    /// The layers, made with the first frame.
    fn layers(&mut self, cx: &Context) {
        if self.layers.is_some() {
            return;
        }
        let mut map = MapLayer::new(cx.device, cx.format);
        let white = map.texture(cx.device, 1, 1, 1);
        map.write(cx.queue, white, [0, 0], 1, 1, &[255; 4]);
        let mut ui = Ui::new(cx.device, cx.format, render::DEPTH_FORMAT);
        let mut assets = crate::ui::UiAssets::default();
        assets.add_white(&mut ui, cx.device, cx.queue);
        // the files as the library's view has them (`esf::Library::in_view`:
        // the 2002 disc's UI.CSF ahead of Frontiers', whose surfaces it lacks
        // still go up after it, and the atlas's sheet Frontiers' in both,
        // `UiAssets::add_surfaces_of`)
        let files = cx.lib.in_view();
        for &i in &files {
            if cx.lib.files[i].0.eq_ignore_ascii_case("UI.CSF") {
                assets.add_surfaces_of(&mut ui, cx.device, cx.queue, cx.lib, i);
            }
        }
        let id = vi::ui::font_id() as i32;
        let font = files.iter().map(|&i| &cx.lib.files[i].1).find_map(|c| (0..c.nodes.len()).filter(|&i| c.nodes[i].ty == 0x7000).find_map(|i| esf::font::read(c, i).filter(|f| f.id == id)));
        if let Some(f) = font {
            assets.font = Some(ui.add_font(cx.device, cx.queue, &f));
        }
        self.layers = Some(Layers { map, white, ui, assets, view: cx.lib.view() });
    }

    /// The world played in changed (or the first): its map begun, its
    /// pictures from the cache or made.
    fn new_world(&mut self, cx: &Context, name: String) {
        if let (Some(old), Some(l)) = (self.world.take(), self.layers.as_mut()) {
            if let Some(t) = old.overview {
                l.map.free(t);
            }
            for (t, _) in old.tiles.values() {
                l.map.free(*t);
            }
        }
        self.view = None;
        self.reported = None;
        let Some(c) = cx.lib.files.get(WORLD_FILE).map(|f| &f.1) else { return };
        let Some(base) = c.roots.first().and_then(|&r| c.child(r, 0x8200)) else { return };
        let bounds: Vec<[f32; 4]> = vi::world::World::<vi::fpu::Ee>::proxies(c, base).iter().map(|b| [b.min.x.0, b.min.z.0, b.max.x.0, b.max.z.0]).collect();
        if bounds.is_empty() {
            return;
        }
        let ranges: Vec<(u64, usize)> = (0..cx.lib.zone_count(WORLD_FILE)).map(|z| cx.lib.zone_range(WORLD_FILE, z).unwrap_or((0, 0))).collect();
        let key = cells::cache_key(&name, &c.data, &ranges);
        let store = store::open(&key);
        let source = source(cx.lib);
        let builder = Builder::new(WORLD_FILE, &bounds, store, source);
        let overview = self.layers.as_mut().map(|l| {
            let (w, h) = builder.cells.overview_size();
            l.map.texture(cx.device, w.max(1), h.max(1), OVERVIEW_LEVELS)
        });
        crate::say(&format!("map: {name}, {} zones in {} x {} cells, kept as {key}", bounds.len(), builder.cells.cols, builder.cells.rows));
        // its `gWorldInfo` index by its file (the player's world number
        // changes with a teleport before the library has the new file)
        let index = crate::play::WORLD_FILES.iter().position(|f| f.eq_ignore_ascii_case(&name)).map_or(cx.world, |i| i as i32);
        self.world = Some(World { name, era: cx.lib.era_of(WORLD_FILE), index, builder, overview, tiles: BTreeMap::new(), made: Instant::now(), view: None });
    }

    /// The build's overview cells into the overview; its tiles onto the GPU
    /// when the view wants them, or the minimap (`mini`: its zones), the rest
    /// in the cache for later.
    #[allow(clippy::too_many_arguments)]
    fn take_pictures(w: &mut World, l: &mut Layers, cx: &Context, open: bool, v: Option<View>, frame: u64, mini: &[usize]) {
        for (zi, px) in w.builder.cells_out.drain(..) {
            if let (Some(t), Some(cell)) = (w.overview, w.builder.cells.cell_of.get(zi).copied().flatten()) {
                l.map.write(cx.queue, t, w.builder.cells.overview_at(cell), cells::OVERVIEW, cells::OVERVIEW, &px);
            }
        }
        let mut wanted = match (open, v) {
            (true, Some(v)) => wanted_tiles(&w.builder.cells, v, cx.size),
            _ => Vec::new(),
        };
        wanted.extend_from_slice(mini);
        for (zi, px) in std::mem::take(&mut w.builder.tiles_out) {
            if !wanted.contains(&zi) || w.tiles.contains_key(&zi) {
                continue;
            }
            let t = l.map.texture(cx.device, TILE, TILE, render::map::full_chain(TILE, TILE));
            l.map.write(cx.queue, t, [0, 0], TILE, TILE, &px);
            w.tiles.insert(zi, (t, frame));
        }
        // the least lately wanted let go past the limit
        while w.tiles.len() > MAX_TILES {
            let Some((&zi, &(t, _))) = w.tiles.iter().min_by_key(|(_, (_, used))| *used) else { break };
            l.map.free(t);
            w.tiles.remove(&zi);
        }
    }

    /// The view in a `size` window for a frame `dt` seconds long: made at
    /// the first frame up (where it was last left, else the opening one),
    /// following the `player` (x, z) until moved, moved by the held keys and
    /// the sticks (rx, ry, lx, ly) by the time, zoomed by the steps.
    fn steer(&mut self, size: (u32, u32), dt: f32, sticks: [f32; 4], player: Option<[f32; 2]>) {
        let Some(w) = self.world.as_mut() else { return };
        let extent = w.builder.cells.extent();
        let limits = view::limits(extent, size);
        let v = self.view.get_or_insert_with(|| w.view.unwrap_or(View { center: [(extent[0] + extent[2]) / 2.0, (extent[1] + extent[3]) / 2.0], scale: view::opening_scale(size) }));
        if self.follow
            && let Some(p) = player
        {
            v.center = p;
        }
        let dt = dt.clamp(0.0, 0.1);
        // the keys held and the left stick pan; the right stick's y and the
        // triggers zoom (up: in)
        let axis = |neg: bool, pos: bool| pos as i32 as f32 - neg as i32 as f32;
        let mut pan = [axis(self.held[2], self.held[3]), axis(self.held[0], self.held[1])];
        for (p, s) in pan.iter_mut().zip([sticks[2], sticks[3]]) {
            if s * s > *p * *p {
                *p = s;
            }
        }
        if pan != [0.0, 0.0] {
            let px = PAN_RATE * size.1 as f32 * dt;
            v.drag([-pan[0] * px, -pan[1] * px]);
            self.follow = false;
        }
        let z = (sticks[1] + axis(self.held[4], self.held[5])).clamp(-1.0, 1.0);
        let middle = [size.0 as f32 / 2.0, size.1 as f32 / 2.0];
        if z != 0.0 {
            v.zoom(ZOOM_RATE.powf(z * dt), middle, size, limits);
        }
        if self.steps != 0 {
            v.zoom(ZOOM_STEP.powi(self.steps), middle, size, limits);
            self.steps = 0;
        }
        v.scale = v.scale.clamp(limits.0, limits.1);
        v.keep_in(extent);
        w.view = Some(*v);
    }

    /// The tiles the view wants asked for: of the cache, else nothing (the
    /// overview stands in until a tile is drawn).
    fn want_tiles(&mut self, size: (u32, u32)) {
        let (Some(w), Some(v)) = (self.world.as_mut(), self.view) else { return };
        for zi in wanted_tiles(&w.builder.cells, v, size) {
            match w.tiles.get_mut(&zi) {
                Some(t) => t.1 = self.frame,
                None => {
                    w.builder.want_tile(zi);
                }
            }
        }
    }

    /// This frame's quads: the overview and the tiles; the grid, the
    /// group and the marker; the chrome (`chrome::draw`).
    fn compose(&mut self, cx: &Context) {
        let (quest, card) = self.cards(cx.keyboard);
        // a community marker's card over a marker's, but a marker a finger
        // tapped keeps its own
        let shared = if quest.is_none() { self.card_community(cx.keyboard).filter(|_| card.is_none() || card != self.pinned) } else { None };
        let pointer = self.pointer_world(cx.keyboard).map(|p| format!("x {:.0}, z {:.0}", p[0], p[1]));
        let icon = self.icon.tex;
        let target = self.target_spot();
        let (Some(w), Some(v), Some(l)) = (self.world.as_ref(), self.view, self.layers.as_mut()) else { return };
        let group_mark = self.group_mark.as_ref().filter(|g| g.world == w.index);
        let size = cx.size;
        let cells = &w.builder.cells;
        let extent = cells.extent();
        let rect_of = |r: [f32; 4]| {
            let (a, b) = (v.to_window(size, [r[0], r[1]]), v.to_window(size, [r[2], r[3]]));
            [a[0], a[1], b[0], b[1]]
        };
        let full = [0.0, 0.0, 1.0, 1.0];
        l.map.quads.clear();
        l.map.quads.push(MapQuad::rect(l.white, [0.0, 0.0, size.0 as f32, size.1 as f32], full, OUTSIDE));
        let world_rect = rect_of(extent);
        // a cell with a zone not drawn yet; one with none stays outside
        for cell in (0..cells.len()).filter(|&c| cells.zone_at[c].is_some()) {
            l.map.quads.push(MapQuad::rect(l.white, rect_of(cells.rect(cell)), full, NOT_DRAWN));
        }
        if let Some(t) = w.overview {
            l.map.quads.push(MapQuad::rect(t, world_rect, full, [1.0; 4]));
        }
        for (&zi, &(t, _)) in &w.tiles {
            if let Some(cell) = cells.cell_of[zi] {
                l.map.quads.push(MapQuad::rect(t, rect_of(cells.rect(cell)), full, [1.0; 4]));
            }
        }
        // over it, against the window, in the frame's units
        let ui = &mut l.ui;
        ui.begin(size);
        let f = |p: [f32; 2]| frame_from_window(size, p);
        let Some(white) = l.assets.white else { return };
        let solid = |ui: &mut Ui, r: [f32; 4], color: [f32; 4]| {
            let (a, b) = (f([r[0], r[1]]), f([r[2], r[3]]));
            ui.push(Quad { tex: white, blend: 3, pos: [[a[0], a[1]], [b[0], a[1]], [b[0], b[1]], [a[0], b[1]]], uv: [[0.5, 0.5]; 4], color, clip: None });
        };
        // the grid, where a cell is big enough to see
        if CELL / v.scale >= 48.0 {
            for c in 0..=cells.cols {
                let x = v.to_window(size, [extent[0] + c as f32 * CELL, 0.0])[0];
                solid(ui, [x - 0.5, world_rect[1], x + 0.5, world_rect[3]], GRID);
            }
            for r in 0..=cells.rows {
                let y = v.to_window(size, [0.0, extent[1] + r as f32 * CELL])[1];
                solid(ui, [world_rect[0], y - 0.5, world_rect[2], y + 0.5], GRID);
            }
        }
        // what the chrome shows: the names, the title, the build's progress
        let words = cx.words;
        let word = |id: i32| words.map(|t| t.get(id, 0).to_string()).filter(|s| s != "(null)" && !s.is_empty());
        let bounds = self.names.bounds.get(w.index.max(0) as usize).copied().flatten();
        let mut names = Vec::new();
        let k = (f([1.0, 0.0])[0] - f([0.0, 0.0])[0]).max(1e-6);
        let font = l.assets.font.map(|fo| (ui.fonts[fo].height as f32, fo));
        for cell in 0..cells.len() {
            let (c, r) = cells.col_row(cell);
            let rect = rect_of(cells.rect(cell));
            if rect[2] < 0.0 || rect[0] > size.0 as f32 || rect[3] < 0.0 || rect[1] > size.1 as f32 {
                continue;
            }
            let Some(n) = cells::name(w.index, bounds, c, r).and_then(|n| word(n as i32)) else { continue };
            let tw = font.map_or(0, |(_, fo)| ui.fonts[fo].string_width(&vi::ui::wide(&n))) as f32 / k;
            let mid = [(rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0];
            let on = (0.0..size.0 as f32).contains(&mid[0]) && (0.0..size.1 as f32).contains(&mid[1]);
            if on && rect[2] - rect[0] >= tw + 16.0 && rect[3] - rect[1] >= font.map_or(16.0, |h| h.0) / k * 2.0 {
                names.push((f(mid), n));
            }
        }
        // the waypoint's distance under its diamond, while the player is in its world
        let waypoint = self.waypoints.get(&w.index).copied();
        if let (Some(at), Some((p, _)), true) = (waypoint, cx.player, cx.world == w.index) {
            let s = v.to_window(size, at);
            let r = view::waypoint_radius(size).0;
            names.push((f([s[0], s[1] + r * 2.8]), format!("{:.0} away", (p[0] - at[0]).hypot(p[2] - at[1]))));
        }
        // the group's, with whose it is (`group_waypoint`)
        if let (Some(g), Some((p, _)), true) = (group_mark, cx.player, cx.world == w.index) {
            let s = v.to_window(size, g.at);
            let r = view::waypoint_radius(size).0;
            let from = g.from.trim_end_matches(['\0', ' ']);
            names.push((f([s[0], s[1] + r * 2.8]), format!("{from}: {:.0} away", (p[0] - g.at[0]).hypot(p[2] - g.at[1]))));
        }
        let world_name = self.names.names.get(w.index.max(0) as usize).copied().flatten().and_then(|n| word(n as i32)).unwrap_or_else(|| title_case(w.name.split('.').next().unwrap_or("")));
        let area = cx.player.and_then(|(p, _)| {
            let (c, r) = ((p[0] / CELL) as i32, (p[2] / CELL) as i32);
            let n = word(cells::name(w.index, bounds, c, r)? as i32)?;
            Some(match word(cells::ninth(p[0], p[2], cx.origin)) {
                Some(d) => format!("{n} ({d})"),
                None => n,
            })
        });
        let (built, of) = w.builder.progress();
        let c = chrome::Chrome {
            corners: [f([0.0, 0.0]), f([size.0 as f32, size.1 as f32])],
            title: match area {
                Some(a) => format!("{world_name}: {a}"),
                None => world_name,
            },
            names,
            keyboard: cx.keyboard,
            progress: (built < of).then_some((built, of)),
            quests: cx.quest_marks.map(|_| crate::settings::quest_markers()),
            pointer,
            notice: self.community.notice_now().map(str::to_string),
            group: cx.in_group,
        };
        let fallback;
        let vui = match cx.vui {
            Some(v) => v,
            None => {
                fallback = vi::ui::Vui::new(words.cloned().unwrap_or_default(), Default::default(), Vec::new());
                &fallback
            }
        };
        // where the chrome will go, for what keeps clear of it
        let footprint = chrome::footprint(&l.assets.raster(ui), vui, &c);
        // the names, then the group and the player over them, then the rest
        chrome::names(&mut l.assets.raster(ui), vui, &c);
        // the markers (`markers`): the coach routes, the coaches' horseshoes,
        // then the camps' badges over them, under the group and the player
        if crate::settings::map_markers() {
            let (lo, hi) = ([-40.0, -40.0], [size.0 as f32 + 40.0, size.1 as f32 + 40.0]);
            let (on, off, half, shade) = view::route_dash(size);
            let mut pieces = Vec::new();
            for r in markers::routes_of(w.index) {
                let lit = r.of(card);
                let (a, b) = (v.to_window(size, r.a), v.to_window(size, r.b));
                for (p, q) in markers::dashes(a, b, [lo[0], lo[1], hi[0], hi[1]], on, if lit { 0.0 } else { off }) {
                    pieces.push((f(p), f(q), lit));
                }
            }
            markers::draw_routes(ui, white, &pieces, half * k, shade * k);
            let shown: Vec<(markers::Marker, [f32; 2])> = markers::of_world(w.index)
                .map(|m| (m, v.to_window(size, m.at())))
                .filter(|(_, p)| p[0] >= lo[0] && p[1] >= lo[1] && p[0] <= hi[0] && p[1] <= hi[1])
                .map(|(m, p)| (m, f(p)))
                .collect();
            let (r, o) = view::horseshoe_radius(size, v.scale > markers::COMPACT_FROM);
            for &(m, p) in &shown {
                if matches!(m, markers::Marker::Coach(_)) {
                    markers::draw_horseshoe(ui, white, icon, p, r * k, o * k);
                }
            }
            let camps: Vec<(markers::Marker, [f32; 2])> = shown.iter().copied().filter(|(m, _)| matches!(m, markers::Marker::Camp(_))).collect();
            markers::draw_badges(ui, &l.assets, vui, white, font, &camps, v.scale);
            // the community's pins (`community`) over the camps
            let pins: Vec<(community::Kind, [f32; 2])> = self
                .community
                .of_world(w.index)
                .map(|(_, m)| (m.kind, v.to_window(size, m.at)))
                .filter(|(_, p)| p[0] >= lo[0] && p[1] >= lo[1] && p[0] <= hi[0] && p[1] <= hi[1])
                .map(|(k, p)| (k, f(p)))
                .collect();
            let (r, o) = view::pin_radius(size, v.scale > markers::COMPACT_FROM);
            community::draw_pins(ui, white, &pins, r * k, o * k);
        }
        // the towns' names over them, the biggest first, clear of the
        // player's arrow, the chrome and the map's own names (a town the map
        // names itself is left to it)
        let mut town_rects: Vec<[f32; 4]> = Vec::new();
        if let (true, Some((h, fo))) = (crate::settings::map_towns(), font) {
            let t = markers::name_scale(v.scale);
            let pad = [3.0 * t, 1.0 * t];
            let mut obstacles: Vec<[f32; 4]> = c
                .names
                .iter()
                .map(|(p, n)| {
                    let w = ui.fonts[fo].string_width(&vi::ui::wide(n)).max(0) as f32;
                    [p[0] - w / 2.0, p[1] - h / 2.0, p[0] + w / 2.0, p[1] + h / 2.0]
                })
                .collect();
            obstacles.extend_from_slice(&footprint);
            if let Some((p, _)) = cx.player {
                let at = f(v.to_window(size, [p[0], p[2]]));
                let (r, o) = view::marker_radius(size);
                let e = (r + o) * k * 1.25;
                obstacles.push([at[0] - e, at[1] - e, at[0] + e, at[1] + e]);
            }
            let cands: Vec<(usize, [f32; 2], [f32; 2])> = markers::towns_of(w.index)
                .filter(|(_, town)| !c.names.iter().any(|(_, n)| markers::same_name(n, town.name)))
                .map(|(i, town)| {
                    let width = ui.fonts[fo].string_width(&vi::ui::wide(town.name)).max(0) as f32 * t;
                    (i, f(v.to_window(size, town.at)), [width + pad[0] * 2.0, h * t + pad[1] * 2.0])
                })
                .collect();
            let gap = view::name_gap(size, v.scale > markers::COMPACT_FROM) * k;
            let placed = markers::place_names(&cands, gap, &obstacles, [c.corners[0][0], c.corners[0][1], c.corners[1][0], c.corners[1][1]]);
            markers::draw_towns(ui, white, fo, &placed, t, pad);
            town_rects.extend(placed.iter().map(|(_, r)| *r));
        }
        // the community markers' names in close (`community`), as the towns'
        // are, clear of them too
        if let (true, true, Some((h, fo))) = (crate::settings::map_markers(), v.scale < markers::NAMES_FROM, font) {
            let t = markers::name_scale(v.scale);
            let pad = [3.0 * t, 1.0 * t];
            let mut obstacles: Vec<[f32; 4]> = c
                .names
                .iter()
                .map(|(p, n)| {
                    let w = ui.fonts[fo].string_width(&vi::ui::wide(n)).max(0) as f32;
                    [p[0] - w / 2.0, p[1] - h / 2.0, p[0] + w / 2.0, p[1] + h / 2.0]
                })
                .collect();
            obstacles.extend_from_slice(&footprint);
            obstacles.extend_from_slice(&town_rects);
            if let Some((p, _)) = cx.player {
                let at = f(v.to_window(size, [p[0], p[2]]));
                let (r, o) = view::marker_radius(size);
                let e = (r + o) * k * 1.25;
                obstacles.push([at[0] - e, at[1] - e, at[0] + e, at[1] + e]);
            }
            let (r, _) = view::pin_radius(size, false);
            let mut names = Vec::new();
            let cands: Vec<(usize, [f32; 2], [f32; 2])> = self
                .community
                .of_world(w.index)
                .map(|(_, m)| {
                    names.push(m.name.clone());
                    let width = ui.fonts[fo].string_width(&vi::ui::wide(&m.name)).max(0) as f32 * t;
                    (names.len() - 1, f(community::head(v.to_window(size, m.at), r)), [width + pad[0] * 2.0, h * t + pad[1] * 2.0])
                })
                .collect();
            let placed = markers::place_names(&cands, r * k * 1.6, &obstacles, [c.corners[0][0], c.corners[0][1], c.corners[1][0], c.corners[1][1]]);
            community::draw_names(ui, white, fo, &placed, &names, t, pad);
        }
        // the quest marks (`quests`) over the markers and the towns' names,
        // under the group and the player
        let (lo, hi) = ([-40.0, -40.0], [size.0 as f32 + 40.0, size.1 as f32 + 40.0]);
        let marks: Vec<(u8, [f32; 2])> = self
            .quests
            .of_world(w.index)
            .map(|(_, m)| (m.kind, v.to_window(size, [m.x, m.z])))
            .filter(|(_, p)| p[0] >= lo[0] && p[1] >= lo[1] && p[0] <= hi[0] && p[1] <= hi[1])
            .map(|(kind, p)| (kind, f(p)))
            .collect();
        let (r, o) = view::quest_radius(size);
        quests::draw_marks(ui, &l.assets, vui, white, &marks, r * k, o * k);
        // the group's members, their compass colours (1 green, 2 blue, 3 cyan, the rest yellow)
        for (pos, index) in &cx.group {
            let p = v.to_window(size, [pos[0], pos[2]]);
            let color = match index {
                1 => [0.0, 1.0, 0.0, 1.0],
                2 => [0.2, 0.4, 1.0, 1.0],
                3 => [0.0, 1.0, 1.0, 1.0],
                _ => [1.0, 1.0, 0.0, 1.0],
            };
            let (r, o) = view::member_radius(size);
            solid(&mut *ui, [p[0] - r - o, p[1] - r - o, p[0] + r + o, p[1] + r + o], OUTLINE);
            solid(&mut *ui, [p[0] - r, p[1] - r, p[0] + r, p[1] + r], color);
        }
        // the target (`Context::target`, □'s and Suggest's spot while the map
        // follows the player): a white ring round it, as the minimap rings it
        if let Some(t) = target {
            let p = v.to_window(size, t);
            let (r, o) = view::member_radius(size);
            let (rr, th) = (r * 2.4, o.max(1.0));
            for (grow, color) in [(o, OUTLINE), (0.0, [1.0; 4])] {
                let (a, t) = (rr + grow, th + grow * 2.0);
                for e in [[p[0] - a, p[1] - a, p[0] + a, p[1] - a + t], [p[0] - a, p[1] + a - t, p[0] + a, p[1] + a], [p[0] - a, p[1] - a, p[0] - a + t, p[1] + a], [p[0] + a - t, p[1] - a, p[0] + a, p[1] + a]] {
                    solid(&mut *ui, e, color);
                }
            }
        }
        // the waypoints, diamonds (`click`), under the player's marker: the
        // group's (`group_waypoint`) under the player's own
        for (at, color) in [(group_mark.map(|g| g.at), group_waypoint::COLOR), (waypoint, WAYPOINT)] {
            let Some(at) = at else { continue };
            let p = v.to_window(size, at);
            let (r, o) = view::waypoint_radius(size);
            for (rr, color) in [(r + o * 1.5, OUTLINE), (r, color)] {
                let d = [[p[0], p[1] - rr], [p[0] + rr, p[1]], [p[0], p[1] + rr], [p[0] - rr, p[1]]].map(f);
                ui.push(Quad { tex: white, blend: 3, pos: d, uv: [[0.5, 0.5]; 4], color, clip: None });
            }
        }
        // the middle, where □ puts the waypoint, once the map is panned off
        // the player (for a pad: a mouse or a finger points at the place)
        if !self.follow && !cx.keyboard {
            let m = [size.0 as f32 / 2.0, size.1 as f32 / 2.0];
            let (r, o) = view::waypoint_radius(size);
            let (arm, half) = (r * 1.4, o * 0.75);
            for (grow, color) in [(o, OUTLINE), (0.0, chrome::NAME)] {
                solid(&mut *ui, [m[0] - arm - grow, m[1] - half - grow, m[0] + arm + grow, m[1] + half + grow], color);
                solid(&mut *ui, [m[0] - half - grow, m[1] - arm - grow, m[0] + half + grow, m[1] + arm + grow], color);
            }
        }
        // the player: the HUD's size at every zoom (`view::marker_radius`)
        if let Some((p, h)) = cx.player {
            let at = v.to_window(size, [p[0], p[2]]);
            let (r, o) = view::marker_radius(size);
            for (rr, color) in [(r + o, OUTLINE), (r, MARKER)] {
                let d = view::dart(at, h, rr).map(f);
                ui.push(Quad { tex: white, blend: 3, pos: d, uv: [[0.5, 0.5]; 4], color, clip: None });
            }
        }
        // the card of the quest mark or the marker under the pointer, over the
        // rest but the chrome (between the chrome's title and its legend)
        let band = chrome::clear_band(&footprint, c.corners);
        let win = [[c.corners[0][0], band[0]], [c.corners[1][0], band[1]]];
        if let Some(m) = quest.and_then(|i| self.quests.marks.get(i)) {
            markers::draw_lines_card(ui, &l.assets, vui, white, quests::card(m), f(v.to_window(size, [m.x, m.z])), win);
        } else if let Some(m) = shared.and_then(|i| self.community.marks.get(i)) {
            let r = view::pin_radius(size, v.scale > markers::COMPACT_FROM).0;
            markers::draw_lines_card(ui, &l.assets, vui, white, m.card(), f(community::head(v.to_window(size, m.at), r)), win);
        } else if let Some(m) = card {
            markers::draw_card(ui, &l.assets, vui, white, m, f(v.to_window(size, m.at())), win);
        }
        let mut r = l.assets.raster(ui);
        self.buttons = chrome::draw(&mut r, vui, &c);
        self.glyphs = r.glyphs;
        // laid out against the window: cut to it, not to the frame
        for q in &mut ui.quads {
            q.clip = Some(WHOLE);
        }
    }

    /// The map over the frame, while it is up: its own pass into `target`
    /// (a `size` window, `depth` its depth buffer).
    pub fn draw(&self, device: &wgpu::Device, enc: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, depth: &wgpu::TextureView, size: (u32, u32)) {
        let (true, Some(l)) = (self.open, self.layers.as_ref()) else { return };
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("map"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }), stencil_ops: None }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        l.map.draw(device, &mut pass, size);
        l.ui.draw(device, &mut pass, size);
    }

    /// The map's state (a shot's last lines, a report).
    pub fn describe(&self) -> String {
        let mut waypoints: String = self.waypoints.iter().map(|(w, p)| format!(", a waypoint in world {w} at ({:.0}, {:.0})", p[0], p[1])).collect();
        if let Some(g) = &self.group_mark {
            waypoints.push_str(&format!(", the group's waypoint from {} in world {} at ({:.0}, {:.0})", g.from, g.world, g.at[0], g.at[1]));
        }
        if !crate::settings::map_markers() {
            waypoints.push_str(", markers hidden");
        }
        if !crate::settings::map_towns() {
            waypoints.push_str(", town names hidden");
        }
        if !crate::settings::quest_markers() {
            waypoints.push_str(", quest marks hidden");
        } else if !self.quests.marks.is_empty() {
            waypoints.push_str(&format!(", {} quest marks", self.quests.marks.len()));
        }
        waypoints.push_str(&self.community.describe());
        match &self.world {
            Some(w) => format!("{}{}{waypoints}", w.builder.describe(), if self.open { ", up" } else { "" }),
            None => format!("map: no world{waypoints}"),
        }
    }
}

/// A frame's build time, `b`'s most or its share of a frame `dt` seconds
/// long, whichever is less (a frame counted no shorter than 1/240 s).
fn budget(b: (Duration, f32), dt: f32) -> Duration {
    b.0.min(Duration::from_secs_f32(dt.max(1.0 / 240.0) * b.1))
}

/// The tiles worth having for view `v` in a `size` window: the cells it
/// shows, nearest its middle first, while an overview texel would be more
/// than two pixels across.
fn wanted_tiles(cells: &cells::Cells, v: View, size: (u32, u32)) -> Vec<usize> {
    if v.scale > TILE_SCALE {
        return Vec::new();
    }
    let s = v.shown(size);
    let mut out: Vec<(f32, usize)> = (0..cells.len())
        .filter_map(|cell| {
            let zi = cells.zone_at[cell]?;
            let r = cells.rect(cell);
            if r[2] < s[0] || r[0] > s[2] || r[3] < s[1] || r[1] > s[3] {
                return None;
            }
            let (dx, dz) = ((r[0] + r[2]) / 2.0 - v.center[0], (r[1] + r[3]) / 2.0 - v.center[1]);
            Some((dx * dx + dz * dz, zi))
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.into_iter().take(MAX_TILES).map(|(_, zi)| zi).collect()
}

/// How a keyboard key works the map while it is up, by its name (the
/// viewer's: the cursor block, Home, Page Up and Down by name, a character
/// as itself in lower case, and "escape", "backspace", "enter", "space"):
/// the pad's key it is, and how: `Press::Once` on a press, `Press::Step` on
/// each press and repeat, `Press::Held` while it is down.
pub fn keyboard(name: &str) -> Option<(i32, Hold)> {
    Some(match name {
        // X is the map's ○: the markers shown or hidden; T its Select: the
        // towns' names
        "x" => (key::CIRCLE, Hold::Once),
        "t" => (key::SELECT, Hold::Once),
        "escape" | "backspace" | "m" => (key::TRIANGLE, Hold::Once),
        "space" | "enter" | "home" => (key::CROSS, Hold::Once),
        // Q is the keyboard's □ (`binds`' "talk" row): the waypoint, held
        // the group's (its release counts, `Map::square_key`)
        "q" => (key::SQUARE, Hold::Held),
        // G: the group's waypoint at once (`binds`' "map_group_waypoint" row)
        "g" => (KEY_GROUP, Hold::Once),
        // J (the journal's letter) is R3: the quest marks shown or hidden
        "j" => (key::R3, Hold::Once),
        // N is L3: a community marker suggested (`community`)
        "n" => (key::L3, Hold::Once),
        "+" | "=" | "pageup" => (key::R1, Hold::Step),
        "-" | "_" | "pagedown" => (key::L1, Hold::Step),
        "up" | "w" => (key::UP, Hold::Held),
        "down" | "s" => (key::DOWN, Hold::Held),
        "left" | "a" => (key::LEFT, Hold::Held),
        "right" | "d" => (key::RIGHT, Hold::Held),
        _ => return None,
    })
}

/// How a keyboard key acts (`keyboard`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hold {
    Once,
    Step,
    Held,
}

impl Map {
    /// A keyboard key by name (`keyboard`), pressed or let go, `repeat` when
    /// the system repeats it: the map's key, as it acts.
    pub fn keyboard_key(&mut self, name: &str, down: bool, repeat: bool) {
        match keyboard(name) {
            Some((k, Hold::Once)) if down && !repeat => {
                self.key(k, true);
            }
            Some((k, Hold::Step)) if down => {
                self.key(k, true);
            }
            Some((k, Hold::Held)) if !repeat => {
                self.key(k, down);
            }
            _ => {}
        }
    }
}

/// Whether frame point `f` (the HUD's own, before its left side moves out
/// in a wide window or its top up a tall one) is on the compass's rose (`DrawCompass`: 44 pixels
/// across about (39, 54)), which a click or a tap opens the map from.
pub fn on_compass(f: [f32; 2]) -> bool {
    (f[0] - 39.0).powi(2) + (f[1] - 54.0).powi(2) <= 22.0 * 22.0
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c.flat_map(char::to_lowercase)).collect()).unwrap_or_default()
}

/// Where the map reads zones it draws: natively the disc on a thread of its
/// own, in a page the page's reads (`stream::source_for`).
/// What a frame makes again of the map (`Map::frame`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Renew {
    /// the layers: the art swapped (F9, `era`) since they were made, so the
    /// chrome (UI.CSF's art, ArialUI) is made in the library's view now; the
    /// world's pictures were on them and go with them
    layers: bool,
    /// the world's map begun: a world first, another file, the same file off
    /// the other disc (Tunaria off the 2002 disc is another map, its own
    /// cache), or its pictures gone with the layers
    world: bool,
    /// where the map was looking kept: the same world's file (the same ground)
    keep_view: bool,
}

/// `Renew` for a frame: the layers' view (`None`: not made yet), the
/// library's view, the world mapped (its file and disc), and the world in
/// the library now.
fn renew(layers: Option<esf::Era>, view: esf::Era, mapped: Option<(&str, esf::Era)>, name: &str, era: esf::Era) -> Renew {
    let swapped = layers.is_some_and(|l| l != view);
    let world = swapped || mapped.is_none_or(|(n, e)| n != name || e != era);
    let keep_view = world && mapped.is_some_and(|(n, _)| n == name);
    Renew { layers: swapped, world, keep_view }
}

fn source(lib: &esf::Library) -> Option<Box<dyn crate::stream::ZoneSource>> {
    if !lib.is_deferred(WORLD_FILE) {
        return None;
    }
    #[cfg(target_arch = "wasm32")]
    {
        crate::stream::source_for(lib, WORLD_FILE)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // off the disc the world came off (`era`: Tunaria off the 2002 disc)
        let name = lib.files[WORLD_FILE].0.clone();
        let image = crate::era::image_of(lib.era_of(WORLD_FILE))?;
        let z = crate::stream::FileZones::open(std::path::Path::new(&image), &name)?;
        Some(Box::new(build::ThreadZones::new(Box::new(z))))
    }
}

#[cfg(test)]
mod tests;

/// A map without a GPU for the tests here and beside it (`play`'s pointer).
#[cfg(test)]
pub mod testing {
    use super::*;

    /// Tunaria's layout: a zone a 2,000 square, x 2,000..28,000, z 0..34,000.
    pub fn tunaria() -> Vec<[f32; 4]> {
        (0..17).flat_map(|r| (1..14).map(move |c| [c as f32 * CELL, r as f32 * CELL, (c + 1) as f32 * CELL, (r + 1) as f32 * CELL])).collect()
    }

    pub const FREEPORT: [f32; 2] = [25300.0, 15200.0];

    /// A map up over Tunaria (`gWorldInfo` 0) in a `size` window, its view
    /// made on the player at Freeport.
    pub fn up(size: (u32, u32)) -> Map {
        let mut m = Map {
            world: Some(World { name: "TUNARIA.ESF".into(), era: esf::Era::Frontiers, index: 0, builder: Builder::new(WORLD_FILE, &tunaria(), None, None), overview: None, tiles: BTreeMap::new(), made: Instant::now(), view: None }),
            size,
            ..Default::default()
        };
        m.show();
        m.steer(size, 0.0, [0.0; 4], Some(FREEPORT));
        m
    }
}
