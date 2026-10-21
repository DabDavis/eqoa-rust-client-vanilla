//! `VIWndGame::OnDrawSelf`: the HUD in play, through `ui::Vui`. The
//! portrait frame, the compass, the health, power and experience gauges,
//! the chat window's backdrop and the target's bar, at the game's own
//! positions in its 640x448 frame.
//!
//! The group's and the pet's bars and the group's patches on the compass are
//! here; the breath gauge, the target's details box and the quick menus are
//! drawn by `play`.

use crate::ui::{Raster, UiRaster, Vui, icon_id, wide};
use render::ui::{Anchor, Quad};
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
    /// before the left side moves them (for a pointer: `Pick`).
    hits: crate::pointer::Hits,
    picks: Vec<Pick>,
}

/// What a pointer finds on the HUD's left side (`Hud::pick`): the player's
/// own frame, a group member's bar (its id), the pet's bar. Each is what L1's
/// cycle targets in turn. In a page, also the bug report's button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    Me,
    Member(u32),
    Pet,
    #[cfg(target_arch = "wasm32")]
    Report,
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

    /// What is under frame point `p` (before the left side's move).
    pub fn pick(&self, p: [f32; 2]) -> Option<Pick> {
        self.hits.at(p).and_then(|i| self.picks.get(i as usize).copied())
    }

    fn note(&mut self, r: [i32; 4], what: Pick) {
        self.hits.push(r, self.picks.len() as i32);
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
    /// under it, `Ui::end`).
    pub fn draw(&mut self, vui: &Vui, ui: &mut UiRaster, s: &HudState) {
        self.update_glow();
        ui.ui.anchor = Anchor::Left;
        self.hits.clear();
        self.picks.clear();
        let mut p = [10, 15];
        self.note([10, 15, 200, 80], Pick::Me);
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
        self.draw_group(vui, ui, [20, 93], s);
        let y = 93 + ((s.group_count - 1) * 48).max(0);
        self.draw_pet(vui, ui, [20, y], s);
        let p = [-3, 331];
        if s.chat_open {
            // the strip's left piece, stretched to the chat box's piece,
            // which keeps to the right
            ui.ui.anchor = Anchor::Span(p[0] + 2, p[0] + 2 + 203);
            vui.blit_rect(ui, 32, [p[0] + 2, p[1]], [203, 125], 0, 1.0);
            ui.ui.anchor = Anchor::Left;
        }
        self.draw_target(vui, ui, [20, 348], s);
        #[cfg(target_arch = "wasm32")]
        self.draw_report(vui, ui, [20, 421]);
    }

    /// The page's bug report (F3, web/report.js) as a button under the
    /// target's bar: the bar's art, solid as its words are (a tester asked
    /// that it not fade), with its key and what it does on it.
    #[cfg(target_arch = "wasm32")]
    fn draw_report(&mut self, vui: &Vui, ui: &mut UiRaster, at: [i32; 2]) {
        let label = wide("F3  Report a bug");
        let w = vui.string_width(ui, &label) + 28;
        self.note([at[0], at[1], at[0] + w, at[1] + vui.entry(26).uv1[1]], Pick::Report);
        vui.blit_bar(ui, &label, [1.0, 1.0, 1.0, 1.0], at, 25, 26, w, 3, 1.0, 0, true);
    }

    /// `DrawGroup`: a bar for each other active member, 48 apart: its name,
    /// its health when known and two status icons; the target's glows.
    fn draw_group(&mut self, vui: &Vui, ui: &mut UiRaster, at: [i32; 2], s: &HudState) {
        let mut p = at;
        let mut glowing = false;
        for m in &s.group {
            let start = p;
            self.note([p[0], p[1], p[0] + 174, p[1] + 46], Pick::Member(m.id));
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
        self.note([p[0], p[1], p[0] + 174, p[1] + 46], Pick::Pet);
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
    /// first call with the art.
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
    }
}

/// Where `DrawPatch` puts a member `d` from the camera, whose heading is
/// `yaw`: 27 pixels out from (39, 55) at its bearing (ahead is up); none
/// past 2000. (The game's two wraps, the first testing the heading as read,
/// move the angle by whole turns.)
fn patch_point(yaw: f32, d: [f32; 3]) -> Option<[f32; 2]> {
    use std::f32::consts::PI;
    let (center, radius) = ([39.0f32, 55.0], 27.0f32);
    if (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() > 2000.0 {
        return None;
    }
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
    Some([center[0] - x.sin() * radius, center[1] - x.cos() * radius])
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
    use super::patch_point;

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
}
