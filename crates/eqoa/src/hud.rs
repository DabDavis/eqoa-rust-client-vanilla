//! `VIWndGame::OnDrawSelf`: the HUD in play, through `ui::Vui`. The
//! portrait frame, the compass, the health, power and experience gauges,
//! the chat window's backdrop and the target's bar, at the game's own
//! positions in its 640x448 frame.
//!
//! The group's and the pet's bars and the group's patches on the compass are
//! here; the breath gauge, the target's details box and the quick menus are
//! drawn by `play`. So is the client's own row of the pet's effects under its
//! bar (`pet_effects_pieces`), which the game has nowhere. The client's own
//! quest tracker (`tracker`) and DPS meter (`dpsmeter`) share a column under
//! the minimap, the meter on its bottom and the tracker over what it leaves.

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
    /// `HasPet()`: the pet, when the player has one.
    pub pet: Option<PetState>,
    /// `GetP2BE4C()`: the group's size (0 out of one).
    pub group_count: i32,
    /// The members `DrawGroup` draws: not the player, active.
    pub group: Vec<MemberState>,
    /// Every member, for `DrawPatch`.
    pub patches: Vec<PatchMember>,
    /// A raid group's every member (`raidlist`), drawn in place of `group`'s
    /// bars when there are any.
    pub raid: Vec<crate::raidlist::Row>,
    /// The live client's duel (0x1502 sub 8): its centre while the duel is on
    /// and in the player's world, for `DrawPatch`'s orange patch (`duel.rs`).
    pub duel: Option<[f32; 3]>,
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
    /// The DPS meter (`dpsmeter`), while it is up: drawn under the quest
    /// tracker, above the chat box; a pointer on its title shows its other
    /// view or resets it, on its rows pages through them.
    pub meter: Option<crate::dpsmeter::Shown>,
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
    /// The client's own: the raid's marks on its bar (`raidlist::draw_marks`),
    /// none outside a raid or on a server that sends no raid state.
    pub marks: u8,
}

/// What `DrawPet` reads: `GetPetName`, whether the pet is the target, and
/// for a creature pet (`PetIsCreature`) its +0x19 and `Flag4`/`Flag8`; and
/// the client's own, the icons (DictIDs) of the effects on the pet in their
/// order, where the server sends them (`net::session::OP_PET_EFFECTS_DATA`),
/// for the row under the bar: none with Extended's Pet Effects off.
#[derive(Clone, Debug, Default)]
pub struct PetState {
    pub name: String,
    pub is_target: bool,
    pub creature: Option<(i32, bool, bool)>,
    pub effects: Vec<u32>,
}

/// The pet's effects' row (the user, 2026-11-09: "put the buff icons under the
/// pet name plate"; CLIENT shows a pet's effects nowhere, and from petition 1041
/// the server had put them on the player's own effects window, channel 0x43):
/// `VIWndInfluence`'s strip of the player's own (`influence::draw`: the left cap
/// 159, a cell of 157 per effect with its icon 3 in and 6 down, the last cell 3
/// narrower, the right cap 160) at `PET_FX_SCALE` of its size, from the health
/// gauge's left and `PET_FX_AT`'s 48 under the bar's top (the group's bars'
/// stride: clear of the gauge and the status icons), `PET_FX_PER_ROW` to a row,
/// a second row under the first for more, up to `PET_FX_MOST` (the server's most).
const PET_FX_SCALE: f32 = 0.5;
const PET_FX_AT: [i32; 2] = [10, 48];
const PET_FX_PER_ROW: usize = 9;
pub const PET_FX_MOST: usize = 16;
/// A row's height: its icons' (6 down in their cells and 32 high at the
/// scale, so 2 past a cell's 17, as the strip's own hang 4 past its cells),
/// and a pixel between rows.
const PET_FX_ROW_H: i32 = 20;

/// What the row takes under the pet's bar for `n` effects: none for none.
pub fn pet_effects_height(n: usize) -> i32 {
    n.min(PET_FX_MOST).div_ceil(PET_FX_PER_ROW) as i32 * PET_FX_ROW_H
}

/// What a piece of the row is drawn with: an atlas entry's texels, `size` of
/// them from its corner (the strip's art), or an effect's icon, its surface
/// whole.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FxArt {
    Entry(i32, [i32; 2]),
    Icon(u32),
}

/// One piece of the row, over frame rectangle `r` (x, y, width, height).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxPiece {
    pub art: FxArt,
    pub r: [f32; 4],
}

/// The row's pieces for the pet's bar at `at`, in the order the player's strip
/// draws them (each cell over the last icon's 3 pixels past it, as there).
pub fn pet_effects_pieces(vui: &Vui, at: [i32; 2], icons: &[u32]) -> Vec<FxPiece> {
    let k = PET_FX_SCALE;
    let (cap_l, cell, cap_r) = (vui.entry(159), vui.entry(157), vui.entry(160));
    let piece = |art, x: f32, y: f32, w: i32, h: i32| FxPiece { art, r: [x, y, w as f32 * k, h as f32 * k] };
    let mut out = Vec::new();
    for (row, icons) in icons[..icons.len().min(PET_FX_MOST)].chunks(PET_FX_PER_ROW).enumerate() {
        let y = (at[1] + PET_FX_AT[1] + row as i32 * PET_FX_ROW_H) as f32;
        let mut x = (at[0] + PET_FX_AT[0]) as f32;
        out.push(piece(FxArt::Entry(159, cap_l.uv1), x, y, cap_l.uv1[0], cap_l.uv1[1]));
        x += cap_l.uv1[0] as f32 * k;
        for (i, &icon) in icons.iter().enumerate() {
            let w = if i == icons.len() - 1 { cell.uv1[0] - 3 } else { cell.uv1[0] };
            out.push(piece(FxArt::Entry(157, [w, 34]), x, y, w, 34));
            out.push(piece(FxArt::Icon(icon), x + 3.0 * k, y + 6.0 * k, 32, 32));
            x += w as f32 * k;
        }
        out.push(piece(FxArt::Entry(160, cap_r.uv1), x, y, cap_r.uv1[0], cap_r.uv1[1]));
    }
    out
}

/// The row drawn: each piece's texels over its rectangle, in floats (half the
/// strip's size falls between the frame's pixels), as `Blit` draws them.
fn draw_pet_effects(vui: &Vui, ui: &mut UiRaster, at: [i32; 2], icons: &[u32]) {
    for p in pet_effects_pieces(vui, at, icons) {
        let (id, texels) = match p.art {
            FxArt::Entry(n, size) => {
                let e = vui.entry(n);
                (vui.entry_id(&e), Some([e.uv0[0], e.uv0[1], size[0], size[1]]))
            }
            FxArt::Icon(id) => (id, None),
        };
        let Some(tex) = ui.assets.surface(id) else { continue };
        let (w, h) = ui.ui.texture_size(tex);
        let t = texels.unwrap_or([0, 0, w as i32, h as i32]).map(|v| v as f32);
        let (u0, v0, u1, v1) = (t[0] / w as f32, t[1] / h as f32, (t[0] + t[2]) / w as f32, (t[1] + t[3]) / h as f32);
        let (x0, y0, x1, y1) = (p.r[0], p.r[1], p.r[0] + p.r[2], p.r[1] + p.r[3]);
        ui.ui.push(Quad { tex, blend: 0, pos: [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], uv: [[u0, v0], [u1, v0], [u1, v1], [u0, v1]], color: [1.0; 4], clip: None });
        ui.glyphs.other();
    }
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
    /// The raid list's first row showing, and where it lay at the last frame
    /// (`Hud::raid_wheel`).
    raid_first: usize,
    raid_panel: Option<([f32; 4], usize)>,
    /// Where the DPS meter's panel lay at the last frame, how many items it
    /// showed and how far its list steps (`Hud::meter_wheel`, `meter_page`).
    meter_panel: Option<([f32; 4], usize, usize)>,
}

/// What a pointer finds on the HUD's left side (`Hud::pick`): the player's
/// own frame, a group member's bar (its id), the pet's bar. Each is what L1's
/// cycle targets in turn. A raid list's row (its id) is targeted only when the
/// client can see that member.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    Me,
    Member(u32),
    Pet,
    Raid(u32),
    /// the minimap (`map::mini`): the map opened
    Minimap,
    /// the quest tracker (`tracker`): the quest journal opened
    Quests,
    /// the DPS meter (`dpsmeter`): its title's view or Reset, or its rows
    Meter(crate::dpsmeter::Hit),
}

/// Where `DrawGroup` starts the group's bars and `DrawTarget` the target's.
const GROUP_AT: [i32; 2] = [20, 93];
const TARGET_AT: [i32; 2] = [20, 348];

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
            raid_first: 0,
            raid_panel: None,
            meter_panel: None,
        }
    }

    /// The wheel's `lines` (up positive) over the DPS meter at frame point `p`,
    /// its list from `first`: where it goes, an item a notch, while it holds
    /// more than it shows. None when it is not there.
    pub fn meter_wheel(&self, p: [f32; 2], lines: f32, first: usize) -> Option<usize> {
        let (r, _, room) = self.meter_panel?;
        if p[0] < r[0] || p[0] > r[2] || p[1] < r[1] || p[1] > r[3] || room == 0 {
            return None;
        }
        Some((first as i64 - lines.round() as i64).clamp(0, room as i64) as usize)
    }

    /// Where a click on the DPS meter's rows takes its list from `first`: a page
    /// on, back to the top past the last (`dpsmeter::page`).
    pub fn meter_page(&self, first: usize) -> usize {
        self.meter_panel.map_or(0, |(_, shown, room)| crate::dpsmeter::page(first.min(room), shown, room))
    }

    /// The wheel's `lines` (up positive) over the raid list, at frame point `p`:
    /// a row a notch when it holds more than it shows. False when it is not there.
    pub fn raid_wheel(&mut self, p: [f32; 2], lines: f32) -> bool {
        let Some((r, room)) = self.raid_panel else { return false };
        if p[0] < r[0] || p[0] > r[2] || p[1] < r[1] || p[1] > r[3] || room == 0 {
            return false;
        }
        let notch = lines.round() as i64;
        self.raid_first = (self.raid_first as i64 - notch).clamp(0, room as i64) as usize;
        true
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
    /// its bottom (the target's bar: `Vertical`).
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
        let y = if s.raid.is_empty() {
            self.raid_panel = None;
            self.raid_first = 0;
            self.draw_group(vui, ui, GROUP_AT, s);
            GROUP_AT[1] + ((s.group_count - 1) * 48).max(0)
        } else {
            self.glow1_on = -1;
            // the pet's bar under it, and its effects' row
            let below = s.pet.as_ref().map_or(0, |p| crate::raidlist::PET_H + pet_effects_height(p.effects.len()));
            let fit = crate::raidlist::Fit::under(s.raid.len(), below);
            self.raid_first = self.raid_first.min(fit.more);
            for (r, id) in crate::raidlist::draw(vui, ui, GROUP_AT, &fit, &s.raid, self.raid_first) {
                self.note(ui, r, Pick::Raid(id));
            }
            let b = fit.bounds(GROUP_AT);
            let (a, c) = (ui.ui.placed([b[0] as f32, b[1] as f32]), ui.ui.placed([b[2] as f32, b[3] as f32]));
            self.raid_panel = Some(([a[0], a[1], c[0], c[1]], fit.more));
            b[3] + 2
        };
        self.draw_pet(vui, ui, [20, y], s);
        ui.ui.vertical = Vertical::Bottom;
        let p = [-3, 331];
        if s.chat_open {
            // the strip's left piece, left out with the strip (`ui::chat_strip`)
            crate::ui::chat_strip(vui, ui, [p[0] + 2, p[1]], [203, 125]);
        }
        self.draw_target(vui, ui, TARGET_AT, s);
        // the minimap's place, out at a wide window's right and up a tall
        // one's top with the quick menus (`map::mini::FRAME`)
        if s.minimap {
            ui.ui.anchor = Anchor::Right;
            ui.ui.vertical = Vertical::Top;
            self.note(ui, crate::map::mini::FRAME, Pick::Minimap);
        }
        // the DPS meter (`dpsmeter`) on the column's bottom, above the chat
        // box, at most half of it under the quest tracker's quests; the
        // tracker over what is left, both placed the same way
        let col = crate::tracker::COLUMN;
        let meter = s.meter.as_ref().map(|m| {
            let room = if s.tracker.is_empty() { col[3] - col[1] } else { (col[3] - col[1]) / 2 };
            (m, crate::dpsmeter::panel(m, room))
        });
        let bottom = meter.as_ref().map_or(col[3], |(_, p)| p.rect[1] - crate::dpsmeter::GAP);
        if let Some(r) = crate::tracker::draw(ui, &s.tracker, bottom) {
            self.note(ui, r, Pick::Quests);
        }
        self.meter_panel = None;
        if let Some((m, p)) = meter {
            crate::dpsmeter::draw(ui, m, &p);
            self.note(ui, p.view, Pick::Meter(crate::dpsmeter::Hit::View));
            self.note(ui, p.reset, Pick::Meter(crate::dpsmeter::Hit::Reset));
            self.note(ui, p.rows, Pick::Meter(crate::dpsmeter::Hit::Rows));
            let (a, b) = (ui.ui.placed([p.rect[0] as f32, p.rect[1] as f32]), ui.ui.placed([p.rect[2] as f32, p.rect[3] as f32]));
            self.meter_panel = Some(([a[0], a[1], b[0], b[1]], p.shown.len(), p.room));
        }
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
            if m.marks != 0 {
                crate::raidlist::draw_marks(ui, p[0] + 168, p[1] + 5, m.marks);
            }
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
    /// and for a creature its health and two status icons; and the client's
    /// own row of its effects under them (`pet_effects_pieces`), a pointer on
    /// which finds the pet as on its bar.
    fn draw_pet(&mut self, vui: &Vui, ui: &mut UiRaster, at: [i32; 2], s: &HudState) {
        let Some(pet) = &s.pet else {
            self.glow2_on = -1;
            self.glow2 = 0.3;
            return;
        };
        let mut p = at;
        let fx = &pet.effects[..pet.effects.len().min(PET_FX_MOST)];
        let bottom = if fx.is_empty() { 46 } else { PET_FX_AT[1] + pet_effects_height(fx.len()) };
        self.note(ui, [p[0], p[1], p[0] + 174, p[1] + bottom], Pick::Pet);
        self.glow2_on = if pet.is_target { 1 } else { -1 };
        let g = self.glow2;
        vui.blit_bar(ui, &[], [0.0; 4], p, 25, 26, 174, 3, g, 0, true);
        let mut w = vi::ui::Window::default();
        w.init(vui, ui, [p[0] + 87, p[1] + 12], 160, 1, 0);
        w.open(true);
        w.text_box(vui, ui, &wide(&pet.name), 0, 0, 160, 1, [0.0, 0.0, 0.0, g], 10);
        draw_pet_effects(vui, ui, p, fx);
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
    /// first call with the art.
    fn draw_patch(&mut self, ui: &mut UiRaster, s: &HudState) {
        let Some(tex) = ui.assets.surface(icon_id("Patch_Art")) else { return };
        let start = *self.patch_start.get_or_insert(s.time_ms);
        let phase = (s.time_ms - start) % 1500;
        let (uv, size, tex_size) = ([3.0f32, 3.0], [10, 10], 16.0f32);
        let mut patch = |p: [f32; 2], color: [f32; 4]| {
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
        };
        for m in &s.patches {
            if Some(m.id) == s.self_id || (phase > 1350 && s.target == m.id) {
                continue;
            }
            let Some(at) = m.pos else { continue };
            let d = [at[0] - s.camera[0], at[1] - s.camera[1], at[2] - s.camera[2]];
            let Some(p) = patch_point(s.heading, d) else { continue };
            let color = match m.index {
                1 => [0.0, 1.0, 0.0, 1.0],
                2 => [0.0, 0.0, 1.0, 1.0],
                3 => [0.0, 1.0, 1.0, 1.0],
                _ => [1.0, 1.0, 0.0, 1.0],
            };
            patch(p, color);
        }
        // the map's waypoints (the client's own, `map::Map::click`), at any
        // distance: the group's in its orange, the player's own in magenta
        // over it (`waypoint_point`)
        for (w, color) in [(s.group_waypoint, crate::map::group_waypoint::COLOR), (s.waypoint, crate::map::WAYPOINT)] {
            if let Some(w) = w {
                patch(waypoint_point(s.heading, s.camera, w), color);
            }
        }
        // the live client's duel, last (eqoa-live-1500's DrawPatch, 0x0135E124):
        // the duel's centre in orange, at any distance
        if let Some(c) = s.duel {
            let d = [c[0] - s.camera[0], c[1] - s.camera[1], c[2] - s.camera[2]];
            patch(bearing_point(s.heading, d), crate::duel::ORANGE);
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
    Some(bearing_point(yaw, d))
}

/// Where the map's waypoint `w` (x, z) goes on the compass seen from the
/// camera at `camera` turned to `yaw`: `DrawPatch`'s rim at its bearing,
/// however far it is (the client's own: a member past 2000 has no patch).
fn waypoint_point(yaw: f32, camera: [f32; 3], w: [f32; 2]) -> [f32; 2] {
    bearing_point(yaw, [w[0] - camera[0], 0.0, w[1] - camera[2]])
}

/// `patch_point` at any distance (the live client's duel patch measures
/// none, nor does the map's waypoint).
fn bearing_point(yaw: f32, d: [f32; 3]) -> [f32; 2] {
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
    use super::{FxArt, FxPiece, Hud, HudState, MemberState, PetState, Pick, bearing_point, patch_point, pet_effects_height, pet_effects_pieces, waypoint_point};

    /// A Vui whose atlas has the player's effects strip's art at the disc's
    /// sizes (SUPPORT's: the cell 157 32x32, the caps 159 5x34 and 160 3x34).
    fn strip_vui() -> crate::ui::Vui {
        let mut atlas = vec![vi::ui::UiTexEntry::default(); 161];
        atlas[157].uv1 = [32, 32];
        atlas[159].uv1 = [5, 34];
        atlas[160].uv1 = [3, 34];
        crate::ui::Vui::new(Default::default(), Default::default(), atlas)
    }

    /// The pet's effects' row: the player's own strip at half its size (the
    /// caps, a cell per effect, the last 3 narrower, each icon 3 in and 6 down
    /// in it, all halved), from the gauge's left 48 under the bar's top, under
    /// the gauge (to 35) and the status icons (to 48) and within the bar's 174;
    /// nine to a row, a second row 20 under the first, sixteen at most. The
    /// control: no effects, no pieces and no height.
    #[test]
    fn the_pets_effects_sit_in_a_row_under_its_bar() {
        let vui = strip_vui();
        let at = [20, 141];
        assert!(pet_effects_pieces(&vui, at, &[]).is_empty(), "the control: none");
        assert_eq!(pet_effects_height(0), 0);
        let got = pet_effects_pieces(&vui, at, &[7, 8, 9]);
        let p = |art, r| FxPiece { art, r };
        let want = vec![
            p(FxArt::Entry(159, [5, 34]), [30.0, 189.0, 2.5, 17.0]),
            p(FxArt::Entry(157, [32, 34]), [32.5, 189.0, 16.0, 17.0]),
            p(FxArt::Icon(7), [34.0, 192.0, 16.0, 16.0]),
            p(FxArt::Entry(157, [32, 34]), [48.5, 189.0, 16.0, 17.0]),
            p(FxArt::Icon(8), [50.0, 192.0, 16.0, 16.0]),
            p(FxArt::Entry(157, [29, 34]), [64.5, 189.0, 14.5, 17.0]),
            p(FxArt::Icon(9), [66.0, 192.0, 16.0, 16.0]),
            p(FxArt::Entry(160, [3, 34]), [79.0, 189.0, 1.5, 17.0]),
        ];
        assert_eq!(got, want);
        assert_eq!(pet_effects_height(3), 20);
        let icons = |v: &[FxPiece]| v.iter().filter(|q| matches!(q.art, FxArt::Icon(_))).count();
        for n in [1, 9, 10, 16, 20] {
            let all: Vec<u32> = (1..=n as u32).collect();
            let v = pet_effects_pieces(&vui, at, &all);
            assert_eq!(icons(&v), n.min(16), "{n}: sixteen at most");
            let rows = if n > 9 { 2 } else { 1 };
            assert_eq!(pet_effects_height(n), rows * 20, "{n}");
            for q in &v {
                assert!(q.r[1] >= (at[1] + 48) as f32, "{n}: {q:?} over the gauge and the status icons");
                assert!(q.r[1] + q.r[3] <= (at[1] + 48 + rows * 20) as f32, "{n}: {q:?} past its rows");
                assert!(q.r[0] >= (at[0] + 10) as f32 && q.r[0] + q.r[2] <= (at[0] + 174) as f32, "{n}: {q:?} out of the bar's width");
            }
        }
        let ten = pet_effects_pieces(&vui, at, &(1..=10).collect::<Vec<u32>>());
        let tenth = ten.iter().find(|q| q.art == FxArt::Icon(10)).unwrap();
        assert_eq!(tenth.r, [34.0, 212.0, 16.0, 16.0], "the tenth starts the second row, as the first did");
        // the first row's icons clear of the second's art
        let first_bottom = ten.iter().filter(|q| q.r[1] < 200.0).map(|q| q.r[1] + q.r[3]).fold(0.0, f32::max);
        let second_top = ten.iter().filter(|q| q.r[1] >= 200.0).map(|q| q.r[1]).fold(f32::MAX, f32::min);
        assert!(first_bottom < second_top, "rows overlap: {first_bottom} over {second_top}");
    }

    /// A pointer on the pet's effects' row finds the pet, as one on its bar
    /// does; the control: with no effects the same place finds nothing. Needs a
    /// graphics adapter; without one it says so and passes.
    #[test]
    fn a_pointer_finds_the_pet_on_its_effects_row() {
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let Some(mut ui) = crate::ui::tests::layer() else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let vui = strip_vui();
        let assets = crate::ui::UiAssets::default();
        // no group: the pet's bar where the group's start (20, 93), its row 48 under it
        let row = [60.0, 93.0 + 48.0 + 8.0];
        for (effects, want) in [(vec![1, 2, 3], Some(Pick::Pet)), (Vec::new(), None)] {
            ui.begin((640, 480));
            let mut hud = Hud::new();
            let s = HudState { pet: Some(PetState { name: "Swarming Death".into(), effects: effects.clone(), ..Default::default() }), ..Default::default() };
            hud.draw(&vui, &mut assets.raster(&mut ui), &s);
            assert_eq!(hud.pick(row), want, "{} effects", effects.len());
            assert_eq!(hud.pick([60.0, 100.0]), Some(Pick::Pet), "the bar itself");
        }
    }

    fn near(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    /// The wheel steps the raid list while it holds more than it shows, only over
    /// the list; with nothing left out it takes no notch.
    #[test]
    fn the_wheel_steps_the_raid_list() {
        let mut h = Hud::new();
        assert!(!h.raid_wheel([30.0, 100.0], 1.0), "no list");
        h.raid_panel = Some(([20.0, 93.0, 300.0, 300.0], 3));
        assert!(!h.raid_wheel([400.0, 100.0], -1.0), "off it");
        assert_eq!(h.raid_first, 0);
        assert!(h.raid_wheel([30.0, 100.0], -2.0));
        assert_eq!(h.raid_first, 2);
        assert!(h.raid_wheel([30.0, 100.0], -5.0));
        assert_eq!(h.raid_first, 3, "no farther than the last");
        assert!(h.raid_wheel([30.0, 100.0], 9.0));
        assert_eq!(h.raid_first, 0, "back to the first");
        h.raid_panel = Some(([20.0, 93.0, 300.0, 300.0], 0));
        assert!(!h.raid_wheel([30.0, 100.0], -1.0), "all showing: the wheel is the camera's");
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
        // the live client's duel patch measures no distance: past the edge
        // it is where a member would be within it
        assert!(near(bearing_point(0.0, [100.0, 0.0, 0.0]), [12.0, 55.0]));
        assert!(near(bearing_point(0.0, [4000.0, 0.0, 0.0]), [12.0, 55.0]));
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

    /// A raid list's rows are where a pointer finds them, past a wide window's left
    /// side too, and take the group's bars' place: no member's bar there. The
    /// wheel knows it only while it is drawn, and off the list the control.
    /// Needs a graphics adapter; without one it says so and passes.
    #[test]
    fn a_pointer_finds_the_raid_lists_rows() {
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let Some(mut ui) = crate::ui::tests::layer() else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let vui = crate::ui::Vui::new(Default::default(), Default::default(), vec![vi::ui::UiTexEntry::default(); 27]);
        let assets = crate::ui::UiAssets::default();
        let row = |id: u32| crate::raidlist::Row { id, name: format!("m{id}"), hp: 255, seen: true, ..Default::default() };
        // the third row's middle, then the second column's first row's, of twenty
        let fit = crate::raidlist::Fit::under(20, 0);
        assert_eq!((fit.columns, fit.rows), (2, 10));
        let (third, next) = ([40.0, 93.0 + 13.0 + 2.0 * 16.0 + 7.0], [20.0 + 104.0 + 20.0, 93.0 + 13.0 + 7.0]);
        for (size, side) in [((640, 480), 0.0), ((1280, 720), 106.0)] {
            ui.begin(size);
            let mut hud = Hud::new();
            let s = HudState { raid: (1..=20).map(row).collect(), group_count: 4, group: vec![MemberState { id: 99, name: "bar".into(), ..Default::default() }], ..Default::default() };
            hud.draw(&vui, &mut assets.raster(&mut ui), &s);
            assert_eq!(hud.pick([third[0] - side, third[1]]), Some(Pick::Raid(3)), "{size:?}");
            assert_eq!(hud.pick([next[0] - side, next[1]]), Some(Pick::Raid(11)), "{size:?}");
            assert_eq!(hud.picks.iter().filter(|p| matches!(p, Pick::Member(_))).count(), 0, "{size:?}: no bars");
            assert!(hud.raid_panel.is_some());
            // the raid over: the bars again, the panel gone
            ui.begin(size);
            let s = HudState { group_count: 2, group: vec![MemberState { id: 99, name: "bar".into(), ..Default::default() }], ..Default::default() };
            hud.draw(&vui, &mut assets.raster(&mut ui), &s);
            assert_eq!(hud.pick([third[0] - side, 100.0]), Some(Pick::Member(99)), "{size:?}: the control");
            assert!(hud.raid_panel.is_none() && !hud.raid_wheel([third[0] - side, third[1]], -1.0));
        }
    }

    /// The DPS meter's panel sits on the quest tracker's column's bottom, out at
    /// a wide window's right (106 frame pixels) and up a tall one's top (85):
    /// a pointer finds its title's view, its Reset and its rows there; the
    /// wheel steps a list longer than it shows, and a click a page on. The
    /// controls: no meter, nothing there; every line showing, the wheel is the
    /// camera's. Needs a graphics adapter; without one it says so and passes.
    #[test]
    fn a_pointer_finds_the_dps_meter() {
        use crate::dpsmeter::Hit;
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let Some(mut ui) = crate::ui::tests::layer() else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let vui = crate::ui::Vui::new(Default::default(), Default::default(), vec![vi::ui::UiTexEntry::default(); 27]);
        let assets = crate::ui::UiAssets::default();
        let fight = |n: u32| {
            let mut m = crate::dpsmeter::Meter::default();
            let lines = (0..n).map(|i| net::session::DpsLine { own: i == 0, class: 1, damage: 1000 - i, name: format!("N{i}"), ..Default::default() }).collect();
            m.take(net::session::DpsMeter { fight: 1, over: false, length_ms: 5000, total: 3000, count: n as u16, place: 1, lines });
            m.shown(crate::dpsmeter::View::Fight, "")
        };
        // three lines: the panel 14 + 3 x 13 + 2 high on the column's bottom (336)
        let (view, reset, row) = ([400.0, 288.0], [570.0, 288.0], [450.0, 310.0]);
        for (size, side, up) in [((640, 480), 0.0, 0.0), ((1280, 720), 106.0, 0.0), ((798, 828), 0.0, 85.0)] {
            let at = |p: [f32; 2]| [p[0] + side, p[1] - up];
            ui.begin(size);
            let mut hud = Hud::new();
            hud.draw(&vui, &mut assets.raster(&mut ui), &HudState { meter: Some(fight(3)), ..Default::default() });
            assert_eq!(hud.pick(at(view)), Some(Pick::Meter(Hit::View)), "{size:?}");
            assert_eq!(hud.pick(at(reset)), Some(Pick::Meter(Hit::Reset)), "{size:?}");
            assert_eq!(hud.pick(at(row)), Some(Pick::Meter(Hit::Rows)), "{size:?}");
            assert_eq!(hud.meter_wheel(at(row), -1.0, 0), None, "{size:?}: every line showing, the wheel is the camera's");
            if side + up > 0.0 {
                assert_eq!(hud.pick(view), None, "{size:?}: not at the frame's own place");
            }
            // twenty lines: ten show, the wheel and a click step them
            ui.begin(size);
            hud.draw(&vui, &mut assets.raster(&mut ui), &HudState { meter: Some(fight(20)), ..Default::default() });
            let r = at([450.0, 300.0]);
            assert_eq!(hud.pick(r), Some(Pick::Meter(Hit::Rows)), "{size:?}");
            assert_eq!(hud.meter_wheel(r, -3.0, 0), Some(3), "{size:?}");
            assert_eq!(hud.meter_wheel(r, 9.0, 3), Some(0), "{size:?}: no farther than the top");
            assert_eq!(hud.meter_wheel(r, -99.0, 0), Some(10), "{size:?}: nor the last");
            assert_eq!(hud.meter_wheel(at([100.0, 300.0]), -1.0, 0), None, "{size:?}: off it");
            assert_eq!((hud.meter_page(0), hud.meter_page(9), hud.meter_page(10)), (9, 10, 0), "{size:?}: a page on, back to the top");
            // the control: no meter
            ui.begin(size);
            hud.draw(&vui, &mut assets.raster(&mut ui), &HudState::default());
            assert_eq!(hud.pick(at(row)), None, "{size:?}: no meter");
            assert_eq!(hud.meter_wheel(at(row), -1.0, 0), None);
        }
    }

    /// A pointer finds the HUD's pieces where they were drawn: in a 4:3
    /// window at the game's places; in a 16:9 one out at the left side (106
    /// frame pixels); in one narrower than 4:3 (two side by side, 798x828:
    /// 85 frame pixels above and below the frame) the player's frame up with
    /// the top, where the game's place no longer holds it (the control).
    /// Needs a graphics adapter; without one it says so and passes.
    #[test]
    fn a_pointer_finds_the_pieces_where_a_tall_window_put_them() {
        let _gpu = crate::GPU_TEST.lock().unwrap_or_else(|e| e.into_inner());
        let Some(mut ui) = crate::ui::tests::layer() else {
            eprintln!("SKIP: no graphics adapter");
            return;
        };
        let vui = crate::ui::Vui::new(Default::default(), Default::default(), vec![vi::ui::UiTexEntry::default(); 27]);
        let assets = crate::ui::UiAssets::default();
        let me = [100.0, 40.0];
        for (size, side, up) in [((640, 480), 0.0, 0.0), ((1280, 720), 106.0, 0.0), ((798, 828), 0.0, 85.0)] {
            ui.begin(size);
            let mut hud = Hud::new();
            hud.draw(&vui, &mut assets.raster(&mut ui), &HudState::default());
            assert_eq!(hud.pick([me[0] - side, me[1] - up]), Some(Pick::Me), "{size:?}");
            if side + up > 0.0 {
                assert_eq!(hud.pick(me), None, "{size:?}: the game's place");
            }
        }
    }
}
