//! `VIWndGame::OnDrawSelf`: the HUD in play, through `ui::Vui`. The
//! portrait frame, the compass, the health, power and experience gauges,
//! the chat window's backdrop and the target's bar, at the game's own
//! positions in its 640x448 frame.
//!
//! The group's and the pet's bars and the group's patches on the compass are
//! here; the breath gauge, the target's details box and the quick menus are
//! drawn by `play`.

use crate::ui::{Raster, UiRaster, Vui, icon_id, wide};
use render::ui::{Anchor, Quad, Vertical};
use vi::ui::TextScroll;

/// What the HUD reads off the client this frame.
#[derive(Clone, Debug, Default)]
pub struct HudState {
    /// Whether the player's entity exists (`FindEntity(m_selfId)`).
    pub has_self: bool,
    /// The player's entity's +0x19, its health of 255.
    pub self_hp: i32,
    pub power: i32,
    pub max_power: i32,
    pub level: i32,
    pub exp: i32,
    /// The camera's heading (`m_camera.m_dir.HPR`).
    pub heading: f32,
    /// The chat window is showing (`m_chat->m_mode`).
    pub chat_open: bool,
    /// The target's name when it is a creature (`GetTargetName` answering 0).
    pub target_name: Option<String>,
    /// The target's +0x19.
    pub target_hp: i32,
    /// `m_targetInfoValid` and `m_targetDisposition`.
    pub target_info: Option<u8>,
    /// The player has targeted themselves and has +0x2C (`m_p2C > 0`).
    pub self_targeted_flag: bool,
    /// Target of Target (the Extended page's, the client's own): the name of
    /// whom the target targets (`Play::target_of_target`).
    pub target_of_target: Option<String>,
    /// Mana Bars (the Extended page's, the client's own): the target's power
    /// of 255, when there is one to show (`Play::bar_power`).
    pub target_power: Option<u8>,
    /// The target of target's health of 255 (its +0x19, while it is in
    /// view) and power (Mana Bars), drawn right of its bar.
    pub tot_hp: Option<u8>,
    pub tot_power: Option<u8>,
    /// `HasPet()`: the pet, when the player has one.
    pub pet: Option<PetState>,
    /// `GetP2BE4C()`: the group's size (0 out of one).
    pub group_count: i32,
    /// The members `DrawGroup` draws: not the player, active.
    pub group: Vec<MemberState>,
    /// Every member, for `DrawPatch`.
    pub patches: Vec<PatchMember>,
    /// The map's waypoint in this world (x, z): a dot on the compass's rim
    /// at any distance, the client's own (`map::Map::click`).
    pub waypoint: Option<[f32; 2]>,
    /// The group's waypoint in this world (`map::group_waypoint`), the same
    /// way in its own colour.
    pub group_waypoint: Option<[f32; 2]>,
    /// The minimap is up (`map::mini`, drawn by the map's own layer under
    /// the HUD): its place noted for a pointer, which opens the map there.
    pub minimap: bool,
    /// The quest tracker's quests (`tracker`), none while it is not up: drawn
    /// under the minimap, a pointer on it opens the quest journal.
    pub tracker: Vec<net::session::TrackedQuest>,
    /// The Stats Box's rows (`statbox`) while it is up: drawn under the
    /// effects' icons, a pointer on it opens the Summary.
    pub stats_box: Option<crate::statbox::View>,
    /// `m_selfId` and `m_target`.
    pub self_id: Option<u32>,
    pub target: u32,
    /// The camera's position (`m_camera.m_position`).
    pub camera: [f32; 3],
    /// The scene's clock, in ms.
    pub time_ms: i64,
}

/// What `DrawPatch` reads of a group member: its index in the group (which
/// colours its patch), its id, and where it is: its actor's position while
/// it has one, else its last place from channel 0x40 when that is in the
/// player's world (none otherwise).
#[derive(Clone, Debug, Default)]
pub struct PatchMember {
    pub index: usize,
    pub id: u32,
    pub pos: Option<[f32; 3]>,
}

/// One group member's bar: its name, whether it is the target, and its
/// health when known (`GetGroupMemberHPKnown`) with `GroupMemberFlag4/8`.
#[derive(Clone, Debug, Default)]
pub struct MemberState {
    pub id: u32,
    pub name: String,
    pub is_target: bool,
    pub hp: Option<(u8, bool, bool)>,
    /// Mana Bars: its power of 255 when there is one to show.
    pub power: Option<u8>,
}

/// What `DrawPet` reads: `GetPetName`, whether the pet is the target, and
/// for a creature pet (`PetIsCreature`) its +0x19 and `Flag4`/`Flag8`.
#[derive(Clone, Debug, Default)]
pub struct PetState {
    pub name: String,
    pub is_target: bool,
    pub creature: Option<(i32, bool, bool)>,
}

/// `ExpForLevel`.
pub fn exp_for_level(level: i32) -> u32 {
    let l = level as u32;
    if level >= 51 {
        l.wrapping_mul(l).wrapping_mul(l).wrapping_mul((l.wrapping_sub(1)).wrapping_mul(27).wrapping_add(20))
    } else {
        l.wrapping_mul(l).wrapping_mul(l).wrapping_mul((l.wrapping_sub(1)).wrapping_mul(28).wrapping_add(25))
    }
}

pub struct Hud {
    glow0: f32,
    glow0_on: i32,
    glow1: f32,
    glow1_on: i32,
    glow2: f32,
    glow2_on: i32,
    scroll: TextScroll,
    /// `VIGetGameOptions()->m_region`.
    pub region: i8,
    /// `DrawPatch`'s static `start`: the clock when its material was made.
    patch_start: Option<i64>,
    /// Where the player's frame, the group's bars and the pet's were drawn,
    /// as placed (out at a wide window's left, up or down a tall one), for a
    /// pointer: `Pick`.
    hits: crate::pointer::Hits,
    picks: Vec<Pick>,
}

/// What a pointer finds on the HUD's left side (`Hud::pick`): the player's
/// own frame, a group member's bar (its id), the pet's bar. Each is what L1's
/// cycle targets in turn. And the bug report's button, and the Target of
/// Target bar (the game's Assist Target asked).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    Me,
    Member(u32),
    Pet,
    Report,
    TargetsTarget,
    /// the minimap (`map::mini`): the map opened
    Minimap,
    /// the quest tracker (`tracker`): the quest journal opened
    Quests,
    /// the Stats Box (`statbox`): the Character menu's Summary opened
    Stats,
}

/// The Target of Target bar's place (the frame's, out at a wide window's
/// left and down a tall one's bottom with the target's bar) and width: just
/// above the "Target" title (348), in the HUD's own column with its health and
/// power under it as a group member's (46 high, to 346), below a whole group's
/// bars and its pet's (a group of four and a pet end at 283). Right of the
/// title (200, 348) it was under the chat box's left half, which reaches 191
/// and grows up past 340 with five lines (F3 20261026-053926: "concealed
/// behind a full chat log, maybe just above the target?").
const TOT_AT: [i32; 2] = [20, 300];
const TOT_WIDTH: i32 = 174;

/// Where `DrawGroup` starts the group's bars and `DrawTarget` the target's.
const GROUP_AT: [i32; 2] = [20, 93];
const TARGET_AT: [i32; 2] = [20, 348];

/// A gauge in `DrawGauge`'s frame (136): its fill's art (7 health, 9
/// power, the player's own), where, lit or dimmed, and how full.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Gauge {
    bar: i32,
    at: [i32; 2],
    lit: bool,
    v: f32,
}

/// Mana Bars on the group's bars (the client's own, the game drawing no one's
/// power but the player's): each member's power, where there is one, under
/// its health as the player's own frame stacks them (10 below), lit as its
/// health is. A member's bar is 46 high and the next 48 on (`DrawGroup`).
fn member_gauges(s: &HudState) -> Vec<Gauge> {
    s.group
        .iter()
        .enumerate()
        .filter_map(|(k, m)| {
            let at = [GROUP_AT[0] + 10, GROUP_AT[1] + 48 * k as i32 + 35];
            m.power.map(|p| Gauge { bar: 9, at, lit: m.is_target, v: p as f32 / 255.0 })
        })
        .collect()
}

/// Mana Bars on the target's bar, under its health (`DrawTarget`'s, 25
/// below the bar) and above the report's button, and Target of Target's
/// health and power under its bar, as a group member's are (25 and 35
/// below it): each where there is one to show, the target of target's only
/// with its bar.
fn target_gauges(s: &HudState) -> Vec<Gauge> {
    let mut out = Vec::new();
    if s.target_name.is_none() {
        return out;
    }
    if let Some(p) = s.target_power {
        out.push(Gauge { bar: 9, at: [TARGET_AT[0] + 10, TARGET_AT[1] + 60], lit: true, v: p as f32 / 255.0 });
    }
    if s.target_of_target.is_some() {
        let at = [TOT_AT[0] + 10, TOT_AT[1] + 25];
        for (k, (bar, v)) in [(7, s.tot_hp), (9, s.tot_power)].into_iter().enumerate() {
            if let Some(v) = v {
                out.push(Gauge { bar, at: [at[0], at[1] + 10 * k as i32], lit: true, v: v as f32 / 255.0 });
            }
        }
    }
    out
}

impl Hud {
    pub fn new() -> Self {
        Hud {
            glow0: 0.3,
            glow0_on: -1,
            glow1: 0.3,
            glow1_on: -1,
            glow2: 0.3,
            glow2_on: -1,
            scroll: TextScroll::default(),
            region: 0,
            patch_start: None,
            hits: Default::default(),
            picks: Vec::new(),
        }
    }

    /// What is under frame point `p`, where the pieces were placed.
    pub fn pick(&self, p: [f32; 2]) -> Option<Pick> {
        self.hits.at(p).and_then(|i| self.picks.get(i as usize).copied())
    }

    /// `what` drawn over frame rectangle `r`, noted where the layer places
    /// it (`Ui::placed`: to a wide window's side, up or down a tall one).
    fn note(&mut self, ui: &UiRaster, r: [i32; 4], what: Pick) {
        let (a, b) = (ui.ui.placed([r[0] as f32, r[1] as f32]), ui.ui.placed([r[2] as f32, r[3] as f32]));
        self.hits.push([a[0], a[1], b[0], b[1]].map(|v| v.round() as i32), self.picks.len() as i32);
        self.picks.push(what);
    }

    /// `UpdateGlow`: each glow brightens while its flag is up and fades to
    /// 0.3.
    fn update_glow(&mut self) {
        for (g, on) in [(&mut self.glow0, self.glow0_on), (&mut self.glow1, self.glow1_on), (&mut self.glow2, self.glow2_on)] {
            *g = if on > 0 { (*g + 0.32).min(1.0) } else { (*g - 0.105).max(0.3) };
        }
    }

    /// `OnDrawSelf`, the drawing half. In a window wider than 4:3 its pieces
    /// keep to the left side (`Anchor`; a menu over them takes them back
    /// under it, `Ui::end`), and in one narrower than 4:3 to its top (the
    /// frame, the compass, the gauges, the group's and the pet's bars) or
    /// its bottom (the target's bar, the report's button: `Vertical`).
    pub fn draw(&mut self, vui: &Vui, ui: &mut UiRaster, s: &HudState) {
        self.update_glow();
        ui.ui.anchor = Anchor::Left;
        ui.ui.vertical = Vertical::Top;
        self.hits.clear();
        self.picks.clear();
        let mut p = [10, 15];
        self.note(ui, [10, 15, 200, 80], Pick::Me);
        vui.blit(ui, 6, p);
        if s.has_self {
            self.draw_compass(ui, s.heading);
            self.draw_patch(ui, s);
        }
        p = [p[0] + 59, p[1] + 21];
        draw_gauge(vui, ui, 7, 136, p, true, s.self_hp as f32 / 255.0);
        p[1] += 10;
        let v = if s.max_power == 0 { 0.0 } else { s.power as f32 / s.max_power as f32 };
        draw_gauge(vui, ui, 9, 136, p, true, v);
        p[1] += 10;
        let need = exp_for_level(s.level + 1);
        let v = s.exp as f32 / need as f32;
        let fifth = (need / 5) as i32;
        // How far through the current fifth of the level.
        let mut part = 0.0f32;
        if s.exp > 0 {
            if s.exp < fifth {
                part = s.exp as f32 / fifth as f32;
            } else {
                let mut i = 2;
                while i < 6 {
                    if s.exp < i * fifth {
                        part = (s.exp - (i - 1) * fifth) as f32 / fifth as f32;
                        break;
                    }
                    i += 1;
                }
            }
        }
        let part = part.clamp(0.0, 1.0);
        draw_gauge(vui, ui, 152, 151, [p[0], p[1] + 10], true, part);
        draw_gauge(vui, ui, 8, 136, p, true, v);
        self.draw_group(vui, ui, GROUP_AT, s);
        for g in member_gauges(s) {
            draw_gauge(vui, ui, g.bar, 136, g.at, g.lit, g.v);
        }
        let y = GROUP_AT[1] + ((s.group_count - 1) * 48).max(0);
        self.draw_pet(vui, ui, [20, y], s);
        ui.ui.vertical = Vertical::Bottom;
        let p = [-3, 331];
        if s.chat_open {
            // the strip's left piece, left out with the strip (`ui::chat_strip`)
            crate::ui::chat_strip(vui, ui, [p[0] + 2, p[1]], [203, 125]);
        }
        self.draw_target(vui, ui, TARGET_AT, s);
        self.draw_target_of_target(vui, ui, TOT_AT, s);
        for g in target_gauges(s) {
            draw_gauge(vui, ui, g.bar, 136, g.at, g.lit, g.v);
        }
        self.draw_report(vui, ui, [20, 421]);
        // the minimap's place, out at a wide window's right and up a tall
        // one's top with the quick menus (`map::mini::FRAME`)
        if s.minimap {
            ui.ui.anchor = Anchor::Right;
            ui.ui.vertical = Vertical::Top;
            self.note(ui, crate::map::mini::FRAME, Pick::Minimap);
        }
        // the quest tracker under it, placed the same way (`tracker`)
        if let Some(r) = crate::tracker::draw(ui, &s.tracker) {
            self.note(ui, r, Pick::Quests);
        }
        // the Stats Box under the effects' icons, out at a wide window's left
        // and up a tall one's top with the HUD's left side (`statbox`)
        if let Some(v) = &s.stats_box {
            crate::statbox::draw(vui, ui, v);
            self.note(ui, crate::statbox::FRAME, Pick::Stats);
        }
    }

    /// Target of Target (the Extended page's, the client's own): a second
    /// bar above the target's, its art, glowing with it, the name of whom
    /// the target targets in it as the group's bars hold their names; none
    /// without a target or a name. A pointer on it asks the game's Assist
    /// Target (`Pick::TargetsTarget`).
    fn draw_target_of_target(&mut self, vui: &Vui, ui: &mut UiRaster, at: [i32; 2], s: &HudState) {
        let Some(name) = s.target_of_target.as_deref().filter(|_| s.target_name.is_some()) else { return };
        self.note(ui, [at[0], at[1], at[0] + TOT_WIDTH, at[1] + vui.entry(26).uv1[1]], Pick::TargetsTarget);
        vui.blit_bar(ui, &[], [0.0; 4], at, 25, 26, TOT_WIDTH, 3, self.glow0, 0, true);
        let mut w = vi::ui::Window::default();
        w.init(vui, ui, [at[0] + TOT_WIDTH / 2, at[1] + 12], TOT_WIDTH - 14, 1, 0);
        w.open(true);
        w.text_box(vui, ui, &wide(name), 0, 0, TOT_WIDTH - 14, 1, [0.0, 0.0, 0.0, self.glow0], 10);
    }

    /// The bug report (F3, `report`) as a button under the target's bar:
    /// the bar's art, solid as its words are (a tester asked that it not
    /// fade), with its key and what it does on it.
    fn draw_report(&mut self, vui: &Vui, ui: &mut UiRaster, at: [i32; 2]) {
        let label = wide("F3  Report a bug");
        let w = vui.string_width(ui, &label) + 28;
        self.note(ui, [at[0], at[1], at[0] + w, at[1] + vui.entry(26).uv1[1]], Pick::Report);
        vui.blit_bar(ui, &label, [1.0, 1.0, 1.0, 1.0], at, 25, 26, w, 3, 1.0, 0, true);
    }

    /// `DrawGroup`: a bar for each other active member, 48 apart: its name,
    /// its health when known and two status icons; the target's glows.
    fn draw_group(&mut self, vui: &Vui, ui: &mut UiRaster, at: [i32; 2], s: &HudState) {
        let mut p = at;
        let mut glowing = false;
        for m in &s.group {
            let start = p;
            self.note(ui, [p[0], p[1], p[0] + 174, p[1] + 46], Pick::Member(m.id));
            let glow = if m.is_target { self.glow1 } else { 0.3 };
            glowing |= m.is_target;
            vui.blit_bar(ui, &[], [0.0; 4], p, 25, 26, 174, 3, glow, 0, true);
            let mut w = vi::ui::Window::default();
            w.init(vui, ui, [p[0] + 87, p[1] + 12], 160, 1, 0);
            w.open(true);
            w.text_box(vui, ui, &wide(&m.name), 0, 0, 160, 1, [0.0, 0.0, 0.0, glow], 10);
            if let Some((hp, f4, f8)) = m.hp {
                p = [p[0] + 10, p[1] + 25];
                draw_gauge(vui, ui, 7, 136, p, m.is_target, hp as f32 / 255.0);
                p = [p[0] + 65, p[1] + 2];
                if f4 {
                    vui.blit_ex(ui, 118, [p[0], p[1] - 2], 3, glow, 0);
                    p[0] += 25;
                }
                if f8 {
                    vui.blit_ex(ui, 119, p, 3, glow, 0);
                }
            }
            p = [start[0], start[1] + 48];
        }
        self.glow1_on = if glowing { 1 } else { -1 };
    }

    /// `DrawPet`: the bar, glowing while the pet is the target, its name,
    /// and for a creature its health and two status icons.
    fn draw_pet(&mut self, vui: &Vui, ui: &mut UiRaster, at: [i32; 2], s: &HudState) {
        let Some(pet) = &s.pet else {
            self.glow2_on = -1;
            self.glow2 = 0.3;
            return;
        };
        let mut p = at;
        self.note(ui, [p[0], p[1], p[0] + 174, p[1] + 46], Pick::Pet);
        self.glow2_on = if pet.is_target { 1 } else { -1 };
        let g = self.glow2;
        vui.blit_bar(ui, &[], [0.0; 4], p, 25, 26, 174, 3, g, 0, true);
        let mut w = vi::ui::Window::default();
        w.init(vui, ui, [p[0] + 87, p[1] + 12], 160, 1, 0);
        w.open(true);
        w.text_box(vui, ui, &wide(&pet.name), 0, 0, 160, 1, [0.0, 0.0, 0.0, g], 10);
        let Some((hp, f4, f8)) = pet.creature else { return };
        p = [p[0] + 10, p[1] + 25];
        draw_gauge(vui, ui, 7, 136, p, pet.is_target, hp as f32 / 255.0);
        p = [p[0] + 65, p[1] + 2];
        if f4 {
            vui.blit_ex(ui, 118, [p[0], p[1] - 2], 3, g, 0);
            p[0] += 25;
        }
        if f8 {
            vui.blit_ex(ui, 119, p, 3, g, 0);
        }
    }

    /// `DrawTarget`: "Target", the bar with the name scrolling in it, the
    /// health gauge, the disposition icon.
    fn draw_target(&mut self, vui: &Vui, ui: &mut UiRaster, at: [i32; 2], s: &HudState) {
        let mut p = at;
        let has = s.target_name.is_some();
        self.glow0_on = if has { 1 } else { -1 };
        let name = wide(s.target_name.as_deref().unwrap_or(""));
        vui.draw_word_ex(ui, 408, p[0], p[1], [1.0, 1.0, 1.0, self.glow0], true);
        p[1] += 25;
        vui.blit_bar(ui, &[], [0.0; 4], p, 25, 26, 174, 3, self.glow0, 0, true);
        self.scroll.init(vui, ui, [p[0] + 87, p[1] + 12], 160, 0, 1);
        self.scroll.window.open(true);
        if name.is_empty() {
            self.scroll.reset();
        }
        self.scroll.text_scroll(vui, ui, &name, [0.0, 0.0, 0.0, self.glow0], 8);
        p = [p[0] + 10, p[1] + 25];
        draw_gauge(vui, ui, 7, 136, p, has, s.target_hp as f32 / 255.0);
        p = [p[0] + 65, p[1] + 2];
        if let (Some(d), true) = (s.target_info, has) {
            let icon = match d {
                0 | 1 => 115,
                4..=6 => 117,
                _ => 116,
            };
            vui.blit_ex(ui, icon, p, 3, self.glow0, 0);
            p[0] += 25;
        }
        if s.self_targeted_flag {
            vui.blit_ex(ui, 211, p, 3, self.glow0, 0);
        }
    }

    /// `DrawCompass`: a 44-pixel quad of "sheet1" turned by the heading
    /// about (39, 54); the rose's window in the sheet is the region's.
    fn draw_compass(&self, ui: &mut UiRaster, yaw: f32) {
        let Some(tex) = ui.assets.surface(icon_id("sheet1")) else { return };
        let a = yaw + std::f32::consts::PI;
        let (sn, cs) = (a.sin(), a.cos());
        let at = |x: f32, y: f32| [x * cs - y * sn + 39.0, x * sn + y * cs + 54.0];
        let out = [at(-22.0, 22.0), at(-22.0, -22.0), at(22.0, 22.0), at(22.0, -22.0)];
        let (u0, v0, u1, v1) = match self.region {
            2 => (0.775_390_6, 0.861_328_1, 0.859_375, 0.945_312_5),
            3 => (0.863_281_25, 0.861_328_1, 0.947_265_6, 0.945_312_5),
            _ => (0.046_875, 0.474_609_4, 0.130_859_4, 0.558_593_75),
        };
        // The strip's four vertices (0 1 2 3) as a fan: 0 1 3 2.
        ui.ui.push(Quad {
            tex,
            blend: 0,
            pos: [out[0], out[1], out[3], out[2]],
            uv: [[u0, v1], [u0, v0], [u1, v0], [u1, v1]],
            color: [1.0; 4],
            clip: None,
        });
    }
}

impl Hud {
    /// `DrawPatch`: each other member within 2000 of the camera as a 10x10
    /// patch of "Patch_Art" (ADDART.CSF, 16x16, texels 3 to 13) 27 pixels
    /// out from (39, 55) at its bearing from the camera's heading, in its
    /// index's colour (1 green, 2 blue, 3 cyan, the rest yellow); the
    /// target's is off for the last 150 ms of every 1.5 s, counted from the
    /// first call with the art. The map's waypoint goes on the rim the same
    /// way, magenta, at any distance (`waypoint_point`), and the group's
    /// waypoint in its orange under it.
    fn draw_patch(&mut self, ui: &mut UiRaster, s: &HudState) {
        let Some(tex) = ui.assets.surface(icon_id("Patch_Art")) else { return };
        let start = *self.patch_start.get_or_insert(s.time_ms);
        let phase = (s.time_ms - start) % 1500;
        let (uv, size, tex_size) = ([3.0f32, 3.0], [10, 10], 16.0f32);
        for m in &s.patches {
            if Some(m.id) == s.self_id || (phase > 1350 && s.target == m.id) {
                continue;
            }
            let Some(at) = m.pos else { continue };
            let d = [at[0] - s.camera[0], at[1] - s.camera[1], at[2] - s.camera[2]];
            let Some(p) = patch_point(s.heading, d) else { continue };
            let (hx, hy) = ((size[0] >> 1) as f32, (size[1] >> 1) as f32);
            let color = match m.index {
                1 => [0.0, 1.0, 0.0, 1.0],
                2 => [0.0, 0.0, 1.0, 1.0],
                3 => [0.0, 1.0, 1.0, 1.0],
                _ => [1.0, 1.0, 0.0, 1.0],
            };
            let (u0, v0) = (uv[0] / tex_size, uv[1] / tex_size);
            let (u1, v1) = ((uv[0] + size[0] as f32) / tex_size, (uv[1] + size[1] as f32) / tex_size);
            ui.ui.push(Quad {
                tex,
                blend: 0,
                pos: [[p[0] - hx, p[1] - hy], [p[0] + hx, p[1] - hy], [p[0] + hx, p[1] + hy], [p[0] - hx, p[1] + hy]],
                uv: [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
                color,
                clip: None,
            });
        }
        for (w, color) in [(s.group_waypoint, crate::map::group_waypoint::COLOR), (s.waypoint, crate::map::WAYPOINT)] {
            let Some(p) = w.map(|w| waypoint_point(s.heading, s.camera, w)) else { continue };
            let (hx, hy) = ((size[0] >> 1) as f32, (size[1] >> 1) as f32);
            let (u0, v0) = (uv[0] / tex_size, uv[1] / tex_size);
            let (u1, v1) = ((uv[0] + size[0] as f32) / tex_size, (uv[1] + size[1] as f32) / tex_size);
            ui.ui.push(Quad {
                tex,
                blend: 0,
                pos: [[p[0] - hx, p[1] - hy], [p[0] + hx, p[1] - hy], [p[0] + hx, p[1] + hy], [p[0] - hx, p[1] + hy]],
                uv: [[u0, v0], [u1, v0], [u1, v1], [u0, v1]],
                color,
                clip: None,
            });
        }
    }
}

/// Where `DrawPatch` puts a member `d` from the camera, whose heading is
/// `yaw`: 27 pixels out from (39, 55) at its bearing (ahead is up); none
/// past 2000. (The game's two wraps, the first testing the heading as read,
/// move the angle by whole turns.)
fn patch_point(yaw: f32, d: [f32; 3]) -> Option<[f32; 2]> {
    if (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() > 2000.0 {
        return None;
    }
    Some(rim_point(yaw, d))
}

/// Where the map's waypoint `w` (x, z) goes on the compass seen from the
/// camera at `camera` turned to `yaw`: `DrawPatch`'s rim at its bearing,
/// however far it is (the client's own: a member past 2000 has no patch).
fn waypoint_point(yaw: f32, camera: [f32; 3], w: [f32; 2]) -> [f32; 2] {
    rim_point(yaw, [w[0] - camera[0], 0.0, w[1] - camera[2]])
}

/// `DrawPatch`'s rim: 27 pixels out from (39, 55) at the bearing of `d`
/// from a camera turned to `yaw`.
fn rim_point(yaw: f32, d: [f32; 3]) -> [f32; 2] {
    use std::f32::consts::PI;
    let (center, radius) = ([39.0f32, 55.0], 27.0f32);
    // HPR with y zeroed
    let h = d[0].atan2(d[2]);
    let mut x = h;
    if h - yaw > PI {
        x -= 2.0 * PI;
    }
    if x - yaw < -PI {
        x += 2.0 * PI;
    }
    x -= yaw;
    [center[0] - x.sin() * radius, center[1] - x.cos() * radius]
}

/// `DrawGauge`: the frame (`frame`'s art), then `bar`'s art cut to `v` of
/// its width, inset 3,1 for the experience bar (152) and 2,2 otherwise;
/// dimmed to 0.3 unless lit.
pub fn draw_gauge(vui: &Vui, ui: &mut dyn Raster, bar: i32, frame: i32, at: [i32; 2], lit: bool, v: f32) {
    let fill = vui.entry(bar);
    let bx = vui.entry(frame);
    let alpha = if lit { 1.0 } else { 0.3 };
    vui.blit_ex(ui, bx.x00, at, 3, alpha, 0);
    let inset = if bar == 152 { [3, 1] } else { [2, 2] };
    let at = [at[0] + inset[0], at[1] + inset[1]];
    let size = [(fill.uv1[0] as f32 * v) as i32, -1];
    vui.blit_ex_size(ui, fill.x00, at, size, 3, alpha, 0);
}

#[cfg(test)]
mod tests {
    use super::{Gauge, Hud, HudState, MemberState, Pick, member_gauges, patch_point, target_gauges, waypoint_point};

    fn near(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn a_patch_sits_at_its_bearing_from_the_camera() {
        // facing +z: ahead is the top of the rose, +x (to the left, the
        // world is right-handed) its left, behind its foot
        assert!(near(patch_point(0.0, [0.0, 5.0, 100.0]).unwrap(), [39.0, 28.0]));
        assert!(near(patch_point(0.0, [100.0, 0.0, 0.0]).unwrap(), [12.0, 55.0]));
        assert!(near(patch_point(0.0, [0.0, 0.0, -100.0]).unwrap(), [39.0, 82.0]));
        // turned to face +x, a member there is ahead
        assert!(near(patch_point(std::f32::consts::FRAC_PI_2, [100.0, 0.0, 0.0]).unwrap(), [39.0, 28.0]));
        // across the seam, a member 0.083 rad off the heading either way
        let off = 2.0 * std::f32::consts::PI - 6.2;
        let a = patch_point(3.1, [(-3.1f32).sin(), 0.0, (-3.1f32).cos()]).unwrap();
        let b = patch_point(-3.1, [(3.1f32).sin(), 0.0, (3.1f32).cos()]).unwrap();
        assert!(near(a, [39.0 - off.sin() * 27.0, 55.0 - off.cos() * 27.0]), "{a:?}");
        assert!(near(b, [39.0 + off.sin() * 27.0, 55.0 - off.cos() * 27.0]), "{b:?}");
        // 2000 is the edge, measured before y is dropped
        assert!(patch_point(0.0, [0.0, 0.0, 2000.0]).is_some());
        assert!(patch_point(0.0, [0.0, 100.0, 1998.0]).is_none());
    }

    /// The map's waypoint sits on the rim at its bearing from the camera,
    /// as a member's patch does, but at any distance and whatever the
    /// camera's height: one 5,000 ahead is at the top, one to the east (+x)
    /// at the left facing +z and at the top facing +x. The control: a
    /// member as far has no patch at all.
    #[test]
    fn the_waypoint_sits_on_the_rim_at_any_distance() {
        let cam = [25000.0, 60.0, 15000.0];
        assert!(near(waypoint_point(0.0, cam, [25000.0, 20000.0]), [39.0, 28.0]));
        assert!(near(waypoint_point(0.0, cam, [25100.0, 15000.0]), [12.0, 55.0]));
        assert!(near(waypoint_point(std::f32::consts::FRAC_PI_2, cam, [25100.0, 15000.0]), [39.0, 28.0]));
        assert!(near(waypoint_point(0.0, cam, [25000.0, 15000.0 - 9000.0]), [39.0, 82.0]));
        assert!(patch_point(0.0, [0.0, 0.0, 5000.0]).is_none(), "the control: a member that far has no patch");
    }

    /// Mana Bars: a member's power under its health (10 below, as the
    /// player's own gauges stack), lit as its health is, the bars 48 apart;
    /// the target's under its health, above the report's button (421); the
    /// target of target's health and power under its bar, above the target's
    /// title (348); none where there is no value (the controls), and none for
    /// the target of target without its bar or for anything without a
    /// target.
    #[test]
    fn mana_bars_sit_under_the_health_they_go_with() {
        let m = |id, power, is_target| MemberState { id, name: "x".into(), is_target, hp: Some((200, false, false)), power };
        let s = HudState { group: vec![m(1, Some(255), false), m(2, None, false), m(3, Some(0), true)], ..Default::default() };
        let health = [30, 93 + 25];
        assert_eq!(
            member_gauges(&s),
            [Gauge { bar: 9, at: [health[0], health[1] + 10], lit: false, v: 1.0 }, Gauge { bar: 9, at: [30, 93 + 96 + 35], lit: true, v: 0.0 }],
            "the second member has none; the third is the target's, lit"
        );
        assert!(35 + 10 <= 46, "inside a member's bar");

        let t = HudState { target_name: Some("Casil".into()), target_power: Some(51), ..Default::default() };
        assert_eq!(target_gauges(&t), [Gauge { bar: 9, at: [30, 398 + 10], lit: true, v: 0.2 }]);
        assert!(408 + 10 <= 421, "above the report's button");
        assert_eq!(target_gauges(&HudState { target_power: None, ..t.clone() }), [], "no power sent");
        assert_eq!(target_gauges(&HudState { target_name: None, ..t.clone() }), [], "no target");

        let tot = HudState { target_of_target: Some("Luck".into()), tot_hp: Some(255), tot_power: Some(0), ..t.clone() };
        assert_eq!(
            target_gauges(&tot)[1..],
            [Gauge { bar: 7, at: [30, 325], lit: true, v: 1.0 }, Gauge { bar: 9, at: [30, 335], lit: true, v: 0.0 }]
        );
        assert!(335 + 10 <= 348, "above the target's title");
        assert_eq!(target_gauges(&HudState { tot_hp: None, ..tot.clone() })[1..], [Gauge { bar: 9, at: [30, 335], lit: true, v: 0.0 }], "power alone keeps its row");
        assert_eq!(target_gauges(&HudState { target_of_target: None, ..tot.clone() }).len(), 1, "no bar, no gauges beside it");
        assert_eq!(target_gauges(&HudState { target_name: None, ..tot }), [], "no target");
    }

    /// A pointer finds the HUD's pieces where they were drawn: in a 4:3
    /// window at the game's places; in a 16:9 one out at the left side (106
    /// frame pixels); in one narrower than 4:3 (two side by side, 798x828:
    /// 85 frame pixels above and below the frame) the player's frame up with
    /// the top and the report's button down with the bottom, where the
    /// game's places now hold neither (the control). Needs a graphics
    /// adapter; without one it says so and passes.
    #[test]
    fn a_pointer_finds_the_pieces_where_a_tall_window_put_them() {
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let Some(mut ui) = crate::ui::tests::layer() else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        // the report's bar art 28 high (the atlas's entry 26)
        let mut atlas = vec![vi::ui::UiTexEntry::default(); 27];
        atlas[26].uv1 = [16, 28];
        let vui = crate::ui::Vui::new(Default::default(), Default::default(), atlas);
        let assets = crate::ui::UiAssets::default();
        let (me, report) = ([100.0, 40.0], [30.0, 430.0]);
        for (size, side, up) in [((640, 480), 0.0, 0.0), ((1280, 720), 106.0, 0.0), ((798, 828), 0.0, 85.0)] {
            ui.begin(size);
            let mut hud = Hud::new();
            hud.draw(&vui, &mut assets.raster(&mut ui), &HudState::default());
            assert_eq!(hud.pick([me[0] - side, me[1] - up]), Some(Pick::Me), "{size:?}");
            assert_eq!(hud.pick([report[0] - side, report[1] + up]), Some(Pick::Report), "{size:?}");
            if side + up > 0.0 {
                assert_eq!((hud.pick(me), hud.pick(report)), (None, None), "{size:?}: the game's places");
            }
        }
    }

    /// The minimap's place (`map::mini::FRAME`): a pointer finds it in a
    /// 4:3 window where it is, in a 16:9 one out at the right with the quick
    /// menus (106 frame pixels), in a tall one (798x828) up at the top (85);
    /// the control: with the minimap off nothing is there. Needs a graphics
    /// adapter; without one it says so and passes.
    #[test]
    fn a_pointer_finds_the_minimap_where_the_quick_menus_go() {
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let Some(mut ui) = crate::ui::tests::layer() else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let mut atlas = vec![vi::ui::UiTexEntry::default(); 27];
        atlas[26].uv1 = [16, 28];
        let vui = crate::ui::Vui::new(Default::default(), Default::default(), atlas);
        let assets = crate::ui::UiAssets::default();
        let f = crate::map::mini::FRAME;
        let mid = [(f[0] + f[2]) as f32 / 2.0, (f[1] + f[3]) as f32 / 2.0];
        for (size, side, up) in [((640, 480), 0.0, 0.0), ((1280, 720), 106.0, 0.0), ((798, 828), 0.0, 85.0)] {
            for (on, want) in [(true, Some(Pick::Minimap)), (false, None)] {
                ui.begin(size);
                let mut hud = Hud::new();
                hud.draw(&vui, &mut assets.raster(&mut ui), &HudState { minimap: on, ..Default::default() });
                assert_eq!(hud.pick([mid[0] + side, mid[1] - up]), want, "{size:?}, minimap {on}");
                if side + up > 0.0 {
                    assert_eq!(hud.pick(mid), None, "{size:?}: not at the frame's own place");
                }
            }
        }
    }

    /// Target of Target: its bar just above the "Target" title, where a
    /// pointer finds it (in a 16:9 window out at the left side with the
    /// target's bar), in the HUD's column left of the chat box (which reaches
    /// x 191 and y 340 holding five lines: F3 20261026-053926) and below a
    /// whole group's bars and its pet's (283); none without a name to show or
    /// without a target (the controls). Needs a graphics adapter; without one
    /// it says so and passes.
    #[test]
    fn the_target_of_target_bar_sits_above_the_targets() {
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let Some(mut ui) = crate::ui::tests::layer() else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let mut atlas = vec![vi::ui::UiTexEntry::default(); 27];
        atlas[26].uv1 = [16, 28];
        let vui = crate::ui::Vui::new(Default::default(), Default::default(), atlas);
        let assets = crate::ui::UiAssets::default();
        let at = [super::TOT_AT[0] as f32 + 87.0, super::TOT_AT[1] as f32 + 14.0];
        assert!(super::TOT_AT[1] + 46 <= 348, "above the target's title");
        assert!(super::TOT_AT[1] >= 93 + 3 * 48 + 46, "below a group of four's bars and the pet's");
        assert!(super::TOT_AT[0] + super::TOT_WIDTH <= 194 && super::TOT_AT[0] == super::TARGET_AT[0], "the target's column, left of the chat box");
        let shown = HudState { target_name: Some("Casil".into()), target_of_target: Some("Luck".into()), ..Default::default() };
        for (size, side) in [((640, 480), 0.0), ((1280, 720), 106.0)] {
            for (s, want) in [
                (shown.clone(), Some(Pick::TargetsTarget)),
                (HudState { target_of_target: None, ..shown.clone() }, None),
                (HudState { target_name: None, ..shown.clone() }, None),
            ] {
                ui.begin(size);
                let mut hud = Hud::new();
                hud.draw(&vui, &mut assets.raster(&mut ui), &s);
                assert_eq!(hud.pick([at[0] - side, at[1]]), want, "{size:?} {:?}", s.target_of_target);
            }
        }
    }
}
