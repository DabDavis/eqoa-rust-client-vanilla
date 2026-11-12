//! The raid list (the user, 2026-11-07: "show names and hp in a way to
//! accomodate"): the client's own, as the game has no raid. A group's roster
//! holds four (`m_group`, whatever the server's group holds), so a raid group's
//! every member comes in a message of the server's own (0x7E1A, `group::raid_page`),
//! to a client that asked (`Session::agree_raid_roster`, the Extended page's Raid
//! List, on by default), and is drawn here in place of the group's bars: a row a
//! member, its name over a thin health bar and a thinner power bar in the HUD's blue
//! (Dwarfrish's petition 1242, the user's word, 2026-11-08: "Add a thin mana bar to the
//! raid window frames that shows the players current mana in blue"; the roster has
//! carried each member's power since it began), in as many columns as it takes, down from
//! where the bars start and above the chat box, the pet's bar under it. Past what
//! four columns hold the wheel steps through them. A click targets a member the
//! client can see (`hud::Pick::Raid`); one out of sight is greyed.
//!
//! On a server that sends the raid's state (go-eqoa-server's 0x7E44, `net::raid`;
//! the GM Jibrish's petition 1448: "A small icon denoting who is raid leader") a
//! row carries its member's marks at the right of its name: a gold crown the
//! leader, a blue cross its helper, a red diamond its main assist (petition 1451).
//!
//! On a server that sends each member's group in the raid (go-eqoa-server's 0x7E1A
//! with a group byte, petition 1687: "sort them by group 1,2,3,4 etc. ... cleanly
//! labeled, not too big and sorted") the rows go under a small "Group N" line each,
//! a group's members in the order they joined, the groups in their numbers, a group
//! never split between columns; past four columns the wheel steps a group at a time.
//! The leader and its helper drag a row onto another group (or onto a member of
//! one, to swap them): `play::raid_drag_*`; Community > Group > Raid > Move to Group
//! does it by the target (a pad, a keyboard, a finger). Every row carries its role
//! at its health bar's left (the same petition): a shield for a Warrior, a Paladin
//! or a Shadow Knight, a staff for a Cleric, a Druid or a Shaman, a sword for the
//! rest. Drawn, not the atlas's: UI.CSF has one shield (texture 120, a 33 by 32
//! crest that is a smudge at 8) and no staff or sword.

use crate::ui::{Layout, UiRaster, Vui, wide};

/// One member's row, as the HUD reads it this frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    pub id: u32,
    pub name: String,
    /// Its health of 255.
    pub hp: u8,
    /// Its power of 255.
    pub power: u8,
    pub is_target: bool,
    /// The client has its entity (`Play::sees_entity`).
    pub seen: bool,
    pub me: bool,
    /// Its marks: [`LEADER`], [`HELPER`], [`ASSIST`] (none on a server that
    /// sends no raid state).
    pub marks: u8,
    /// Its class (the roster's), for its role's icon ([`role`]).
    pub class: u8,
    /// Its group in the raid from 1; 0 on a server that sends none (no sections).
    pub group: u8,
    /// It is being dragged to another group (drawn in gold).
    pub dragged: bool,
}

/// A member's part in a fight, by its class (petition 1687).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Warrior (0), Paladin (2), Shadow Knight (3): a shield
    Tank,
    /// Druid (7), Shaman (8), Cleric (9): a staff
    Healer,
    /// everyone else: a sword
    Damage,
}

/// `class`'s role (the wire's class ids: WAR 0 ... ALC 14).
pub fn role(class: u8) -> Role {
    match class {
        0 | 2 | 3 => Role::Tank,
        7..=9 => Role::Healer,
        _ => Role::Damage,
    }
}

/// A role icon's room at a bar's left: its width and the gap after it.
const ROLE_W: i32 = 8;
const ROLE_GAP: i32 = 2;
const SHIELD: [f32; 4] = [0.55, 0.72, 1.0, 1.0];
const SHIELD_RIM: [f32; 4] = [0.85, 0.92, 1.0, 1.0];
const STAFF: [f32; 4] = [0.45, 0.92, 0.45, 1.0];
const STAFF_SHAFT: [f32; 4] = [0.72, 0.55, 0.32, 1.0];
const SWORD: [f32; 4] = [1.0, 0.42, 0.36, 1.0];
const SWORD_BLADE: [f32; 4] = [0.9, 0.9, 0.95, 1.0];

/// `role`'s icon, 8 by 8 from (`x`, `y`), each quad over its shadow: a shield
/// (its face and point, a light rim down its middle), a staff (a shaft from the
/// bottom left with a green orb at its top), a sword (a pale blade from the
/// bottom left, a red guard across it and a red grip). The quads and colours,
/// for a test.
pub fn draw_role(ui: &mut dyn crate::ui::Layout, x: i32, y: i32, role: Role) -> Vec<([f32; 4], [[f32; 2]; 4])> {
    let (x0, y0) = (x as f32, y as f32);
    let shape: Vec<([f32; 4], [[f32; 2]; 4])> = match role {
        Role::Tank => vec![
            (SHIELD, [[x0, y0], [x0 + 8.0, y0], [x0 + 8.0, y0 + 4.0], [x0, y0 + 4.0]]),
            (SHIELD, [[x0, y0 + 4.0], [x0 + 8.0, y0 + 4.0], [x0 + 4.0, y0 + 8.0], [x0 + 4.0, y0 + 8.0]]),
            (SHIELD_RIM, [[x0 + 3.5, y0 + 1.0], [x0 + 4.5, y0 + 1.0], [x0 + 4.5, y0 + 6.5], [x0 + 3.5, y0 + 6.5]]),
        ],
        Role::Healer => vec![
            (STAFF_SHAFT, [[x0, y0 + 7.0], [x0 + 5.0, y0 + 2.0], [x0 + 6.0, y0 + 3.0], [x0 + 1.0, y0 + 8.0]]),
            (STAFF, [[x0 + 6.0, y0], [x0 + 8.0, y0 + 2.0], [x0 + 6.0, y0 + 4.0], [x0 + 4.0, y0 + 2.0]]),
        ],
        Role::Damage => vec![
            (SWORD_BLADE, [[x0 + 2.0, y0 + 5.0], [x0 + 7.0, y0], [x0 + 8.0, y0 + 1.0], [x0 + 3.0, y0 + 6.0]]),
            (SWORD, [[x0, y0 + 3.0], [x0 + 1.0, y0 + 2.0], [x0 + 6.0, y0 + 7.0], [x0 + 5.0, y0 + 8.0]]),
            (SWORD, [[x0, y0 + 7.0], [x0 + 2.0, y0 + 5.0], [x0 + 3.0, y0 + 6.0], [x0 + 1.0, y0 + 8.0]]),
        ],
    };
    for (_, q) in &shape {
        ui.solid(3, SHADOW, q.map(|p| [p[0] + 1.0, p[1] + 1.0]));
    }
    for (c, q) in &shape {
        ui.solid(3, *c, *q);
    }
    shape
}

/// A row's marks ([`Row::marks`]).
pub const LEADER: u8 = 1;
pub const HELPER: u8 = 2;
pub const ASSIST: u8 = 4;

/// A mark's size and the space after it, and its colours.
const MARK_W: i32 = 9;
const MARK_GAP: i32 = 2;
const CROWN: [f32; 4] = [1.0, 0.8, 0.2, 1.0];
const CROSS: [f32; 4] = [0.45, 0.75, 1.0, 1.0];
const DIAMOND: [f32; 4] = [1.0, 0.33, 0.28, 1.0];

/// The width the marks of `marks` take at a row's right.
pub fn marks_width(marks: u8) -> i32 {
    let n = (marks & (LEADER | HELPER | ASSIST)).count_ones() as i32;
    if n == 0 { 0 } else { n * (MARK_W + MARK_GAP) }
}

/// The marks of `marks` drawn leftwards from `right` (x) at the name's line `y`,
/// each over its shadow: the crown's band and three points, the cross's bars,
/// the diamond. The quads each mark was drawn with, for a test.
pub fn draw_marks(ui: &mut dyn crate::ui::Layout, right: i32, y: i32, marks: u8) -> Vec<([f32; 4], [[f32; 2]; 4])> {
    let mut quads = Vec::new();
    let mut x = right as f32;
    for (bit, colour) in [(ASSIST, DIAMOND), (HELPER, CROSS), (LEADER, CROWN)] {
        if marks & bit == 0 {
            continue;
        }
        x -= (MARK_W + MARK_GAP) as f32;
        let (x0, y0) = (x, y as f32 + 2.0);
        let w = MARK_W as f32;
        let shape: Vec<[[f32; 2]; 4]> = match bit {
            LEADER => {
                let band = y0 + 5.0;
                vec![
                    [[x0, band], [x0 + w, band], [x0 + w, band + 3.0], [x0, band + 3.0]],
                    [[x0, band], [x0 + 3.0, band], [x0, y0], [x0, y0]],
                    [[x0 + 3.0, band], [x0 + 6.0, band], [x0 + 4.5, y0 + 0.5], [x0 + 4.5, y0 + 0.5]],
                    [[x0 + 6.0, band], [x0 + w, band], [x0 + w, y0], [x0 + w, y0]],
                ]
            }
            HELPER => vec![
                [[x0 + 3.5, y0], [x0 + 5.5, y0], [x0 + 5.5, y0 + 8.0], [x0 + 3.5, y0 + 8.0]],
                [[x0 + 0.5, y0 + 3.0], [x0 + 8.5, y0 + 3.0], [x0 + 8.5, y0 + 5.0], [x0 + 0.5, y0 + 5.0]],
            ],
            _ => vec![[[x0 + 4.5, y0], [x0 + w, y0 + 4.0], [x0 + 4.5, y0 + 8.0], [x0, y0 + 4.0]]],
        };
        for q in &shape {
            ui.solid(3, SHADOW, q.map(|p| [p[0] + 1.0, p[1] + 1.0]));
        }
        for q in shape {
            ui.solid(3, colour, q);
            quads.push((colour, q));
        }
    }
    quads
}

/// A row, a column and the line over them, in the HUD's frame.
const ROW_H: i32 = 18;
const COL_W: i32 = 100;
const COL_GAP: i32 = 4;
const HEAD_H: i32 = 13;
/// A name's height (the font is scaled to it), and the bar under it.
const NAME_H: f32 = 12.0;
const BAR_H: i32 = 3;
/// The power bar under it.
const POWER_H: i32 = 2;
/// The most columns: past them the wheel steps through the rows.
const MOST_COLUMNS: usize = 4;
/// The height from where the bars start to above the chat box, and what the pet's
/// bar takes of it (its effects' row more: `hud::pet_effects_height`).
const ROOM_H: i32 = 235;
pub const PET_H: i32 = 48;

const BACK: [f32; 4] = [0.0, 0.0, 0.0, 0.32];
const BAR_BACK: [f32; 4] = [0.0, 0.0, 0.0, 0.7];
const NAME: [f32; 4] = [0.93, 0.93, 0.93, 1.0];
const NAME_FAR: [f32; 4] = [0.6, 0.6, 0.6, 1.0];
const NAME_TARGET: [f32; 4] = [1.0, 0.86, 0.35, 1.0];
const NAME_ME: [f32; 4] = [0.65, 0.82, 1.0, 1.0];
const NAME_DEAD: [f32; 4] = [0.95, 0.4, 0.35, 1.0];
const SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.85];

/// How a list of `n` lays out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    pub columns: usize,
    /// The rows of each column.
    pub rows: usize,
    /// The members shown at once.
    pub shown: usize,
    /// How many are left out: what the wheel steps through.
    pub more: usize,
    pub n: usize,
}

impl Fit {
    /// The fewest columns that hold `n` (four at most), evened out, with `below`
    /// left under it: the pet's bar when the player has one (`PET_H`) and, when
    /// its pet has effects, their row (`hud::pet_effects_height`).
    pub fn under(n: usize, below: i32) -> Fit {
        let room = ROOM_H - HEAD_H - below;
        let most = (room / ROW_H).max(1) as usize;
        let columns = n.div_ceil(most).clamp(1, MOST_COLUMNS);
        let rows = n.div_ceil(columns).min(most);
        let shown = (rows * columns).min(n);
        Fit { columns, rows, shown, more: n - shown, n }
    }

    /// Its rectangle from `at`.
    pub fn bounds(&self, at: [i32; 2]) -> [i32; 4] {
        let w = self.columns as i32 * (COL_W + COL_GAP) - COL_GAP;
        [at[0], at[1], at[0] + w, at[1] + HEAD_H + self.rows as i32 * ROW_H]
    }

    /// The rectangle of the `k`th row showing.
    fn row(&self, at: [i32; 2], k: usize) -> [i32; 4] {
        let (c, r) = ((k / self.rows) as i32, (k % self.rows) as i32);
        let (x, y) = (at[0] + c * (COL_W + COL_GAP), at[1] + HEAD_H + r * ROW_H);
        [x, y, x + COL_W, y + ROW_H]
    }
}

/// A group's line over its rows.
const SECTION_H: i32 = 11;
/// What a group's line is drawn in, and its band.
const SECTION_NAME: [f32; 4] = [0.98, 0.85, 0.5, 1.0];
const SECTION_BAND: [f32; 4] = [0.35, 0.3, 0.15, 0.45];
const SECTION_DROP: [f32; 4] = [0.95, 0.8, 0.3, 0.35];
const SECTION_NAME_H: f32 = 10.0;

/// One group of the raid: its number and its rows (places in the list), in the
/// order they joined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub group: u8,
    pub rows: Vec<usize>,
}

impl Section {
    fn height(&self) -> i32 {
        SECTION_H + self.rows.len().max(1) as i32 * ROW_H
    }
}

/// The groups of `rows` in their numbers, each its rows in the list's order (the
/// order they joined); `extra`, an empty group put in its place (a drag's new
/// group). None when no row has a group (a server that sends none).
pub fn sections(rows: &[Row], extra: Option<u8>) -> Vec<Section> {
    if rows.iter().all(|r| r.group == 0) {
        return Vec::new();
    }
    let mut groups: Vec<u8> = rows.iter().map(|r| r.group).chain(extra).collect();
    groups.sort_unstable();
    groups.dedup();
    groups.into_iter().map(|g| Section { group: g, rows: (0..rows.len()).filter(|&k| rows[k].group == g).collect() }).collect()
}

/// The highest group a member may go to in a raid of `n` whose highest group is
/// `top`: one past what four a group needs, or `top` (go-eqoa-server's
/// `raidSubgroupMost`).
pub fn most_group(n: usize, top: u8) -> u8 {
    (n.div_ceil(4) + 1).max(top as usize).min(255) as u8
}

/// How the groups lay out from group `first`: whole groups down each column, the
/// next column when one does not fit, four columns at most.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupFit {
    /// each column's groups (places in the sections)
    pub columns: Vec<Vec<usize>>,
    /// the tallest column's height under the head line
    pub height: i32,
    /// the first group shown, and how far the wheel goes (0: all are shown)
    pub first: usize,
    pub more: usize,
    pub n: usize,
}

impl GroupFit {
    /// `sections` from `first` (kept within what the wheel reaches), with `below`
    /// left under it as `Fit::under`.
    pub fn under(sections: &[Section], n: usize, below: i32, first: usize) -> GroupFit {
        let room = ROOM_H - HEAD_H - below;
        let pack = |from: usize| -> (Vec<Vec<usize>>, i32, bool) {
            let mut columns: Vec<Vec<usize>> = vec![Vec::new()];
            let (mut h, mut tallest) = (0, 0);
            for (k, s) in sections.iter().enumerate().skip(from) {
                if h + s.height() > room && !columns.last().unwrap().is_empty() {
                    if columns.len() == MOST_COLUMNS {
                        return (columns, tallest, false);
                    }
                    columns.push(Vec::new());
                    h = 0;
                }
                columns.last_mut().unwrap().push(k);
                h += s.height();
                tallest = tallest.max(h);
            }
            (columns, tallest, true)
        };
        let more = (0..sections.len()).find(|&f| pack(f).2).unwrap_or(0);
        let first = first.min(more);
        let (columns, height, _) = pack(first);
        GroupFit { columns, height, first, more, n }
    }

    /// Its rectangle from `at`.
    pub fn bounds(&self, at: [i32; 2]) -> [i32; 4] {
        let w = self.columns.len().max(1) as i32 * (COL_W + COL_GAP) - COL_GAP;
        [at[0], at[1], at[0] + w, at[1] + HEAD_H + self.height]
    }

    /// The groups shown.
    fn shown(&self) -> usize {
        self.columns.iter().map(Vec::len).sum()
    }
}

/// What a pointer finds on the grouped list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// a member's row (its id)
    Row(u32),
    /// a group's line or the room under its rows (its number)
    Group(u8),
}

/// The grouped list from `at` in the HUD's frame: the head line, then each group
/// shown its line ("Group 2") over its rows; `drop`, the group a drag is over,
/// lit. Each group's rectangle, then each row's, for a pointer (a row is the
/// last drawn over its group: `Hits::at`).
pub fn draw_groups(vui: &Vui, ui: &mut UiRaster, at: [i32; 2], fit: &GroupFit, sections: &[Section], rows: &[Row], drop: Option<u8>) -> Vec<([i32; 4], Hit)> {
    let mut hits = Vec::new();
    ui.solid(3, BACK, corners(fit.bounds(at)));
    let font = ui.assets.font;
    let text = |ui: &mut UiRaster, x: i32, y: i32, h: f32, c: [f32; 4], s: &[u16]| {
        if let Some(f) = font {
            let scale = h / ui.ui.fonts[f].height.max(1) as f32;
            ui.ui.text_scaled(f, x as f32 + 1.0, y as f32 + 1.0, scale, SHADOW, s);
            ui.ui.text_scaled(f, x as f32, y as f32, scale, c, s);
        }
    };
    let head = if fit.more == 0 {
        format!("Raid ({})", fit.n)
    } else {
        let shown = fit.shown();
        let (a, b) = (sections[fit.first].group, sections[fit.first + shown - 1].group);
        format!("Raid ({}) Groups {a}-{b}", fit.n)
    };
    text(ui, at[0] + 2, at[1] + 1, NAME_H, NAME, &wide(&head));
    let mut row_hits = Vec::new();
    for (c, column) in fit.columns.iter().enumerate() {
        let x = at[0] + c as i32 * (COL_W + COL_GAP);
        let mut y = at[1] + HEAD_H;
        for &k in column {
            let sec = &sections[k];
            let whole = [x, y, x + COL_W, y + sec.height()];
            let band = if drop == Some(sec.group) { SECTION_DROP } else { SECTION_BAND };
            ui.solid(3, band, corners([x, y + 1, x + COL_W, y + SECTION_H - 1]));
            if drop == Some(sec.group) {
                ui.solid(3, SECTION_DROP, corners(whole));
            }
            text(ui, x + 3, y, SECTION_NAME_H, SECTION_NAME, &wide(&format!("Group {}", sec.group)));
            hits.push((whole, Hit::Group(sec.group)));
            y += SECTION_H;
            for &i in &sec.rows {
                let r = [x, y, x + COL_W, y + ROW_H];
                draw_row(vui, ui, r, &rows[i]);
                row_hits.push((r, Hit::Row(rows[i].id)));
                y += ROW_H;
            }
            if sec.rows.is_empty() {
                y += ROW_H;
            }
        }
    }
    hits.extend(row_hits);
    hits
}

fn corners(r: [i32; 4]) -> [[f32; 2]; 4] {
    let (x0, y0, x1, y1) = (r[0] as f32, r[1] as f32, r[2] as f32, r[3] as f32);
    [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

/// What a row's name is drawn in.
fn name_colour(r: &Row) -> [f32; 4] {
    if r.is_target {
        NAME_TARGET
    } else if r.hp == 0 {
        NAME_DEAD
    } else if r.me {
        NAME_ME
    } else if r.seen {
        NAME
    } else {
        NAME_FAR
    }
}

/// The line over the rows: how many, and which while some are left out.
fn head(fit: &Fit, first: usize) -> String {
    if fit.more == 0 {
        format!("Raid ({})", fit.n)
    } else {
        format!("Raid ({}) {}-{}", fit.n, first + 1, first + fit.shown)
    }
}

/// One member's row in rectangle `r`: its name (cut to the column, less its
/// marks) and marks over its role's icon and its health and power bars.
fn draw_row(vui: &Vui, ui: &mut UiRaster, r: [i32; 4], row: &Row) {
    let font = ui.assets.font;
    let scale = font.map_or(1.0, |f| NAME_H / ui.ui.fonts[f].height.max(1) as f32);
    let (x, y) = (r[0] + 2, r[1]);
    let marks = marks_width(row.marks);
    let mut name = wide(&row.name);
    while let Some(f) = font
        && !name.is_empty()
        && ui.ui.fonts[f].string_width(&name) as f32 * scale > (COL_W - 4 - marks) as f32
    {
        name.pop();
    }
    if let Some(f) = font {
        let c = if row.dragged { NAME_TARGET } else { name_colour(row) };
        ui.ui.text_scaled(f, x as f32 + 1.0, y as f32 + 1.0, scale, SHADOW, &name);
        ui.ui.text_scaled(f, x as f32, y as f32, scale, c, &name);
    }
    if marks > 0 {
        draw_marks(ui, r[2] - 2, y, row.marks);
    }
    // the role's icon at the bars' left, the bars after it
    let bar_top = y + ROW_H - BAR_H - POWER_H - 1;
    draw_role(ui, x, bar_top - 3, role(row.class));
    let bar = [x + ROLE_W + ROLE_GAP, bar_top, r[2] - 2, y + ROW_H - 1];
    ui.solid(3, BAR_BACK, corners(bar));
    // health in the HUD's own gauge (7), and under it power in its blue (9)
    for (v, image, top, h) in [(row.hp, 7, bar[1], BAR_H), (row.power, 9, bar[1] + BAR_H, POWER_H)] {
        let w = ((bar[2] - bar[0]) as f32 * v as f32 / 255.0).round() as i32;
        if w > 0 {
            vui.stretch_blit_ex(ui, image, [bar[0], top], [w, h], 3, 1.0);
        }
    }
}

/// The list from `at` in the HUD's frame, from row `first`; each row's rectangle and
/// member, for a pointer.
pub fn draw(vui: &Vui, ui: &mut UiRaster, at: [i32; 2], fit: &Fit, rows: &[Row], first: usize) -> Vec<([i32; 4], u32)> {
    let mut hits = Vec::with_capacity(fit.shown);
    ui.solid(3, BACK, corners(fit.bounds(at)));
    let font = ui.assets.font;
    let scale = font.map_or(1.0, |f| NAME_H / ui.ui.fonts[f].height.max(1) as f32);
    let text = |ui: &mut UiRaster, x: i32, y: i32, c: [f32; 4], s: &[u16]| {
        if let Some(f) = font {
            ui.ui.text_scaled(f, x as f32 + 1.0, y as f32 + 1.0, scale, SHADOW, s);
            ui.ui.text_scaled(f, x as f32, y as f32, scale, c, s);
        }
    };
    text(ui, at[0] + 2, at[1] + 1, NAME, &wide(&head(fit, first)));
    for (k, row) in rows.iter().skip(first).take(fit.shown).enumerate() {
        let r = fit.row(at, k);
        draw_row(vui, ui, r, row);
        hits.push((r, row.id));
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The frame's left side the HUD leaves it: from the group's bars' place down to
    /// above the chat box, and short of the frame's far side.
    const AT: [i32; 2] = [20, 93];

    /// The marks at a row's right, leftwards from its edge: none without a raid's
    /// state (the control), the crown's four quads in gold, and all three in a row
    /// each in its colour, never past the edge; their room is what a name loses.
    #[test]
    fn the_marks_are_drawn_from_the_right() {
        use crate::pointer::testing::NullRaster;
        assert!(draw_marks(&mut NullRaster, 100, 50, 0).is_empty());
        assert_eq!(marks_width(0), 0);
        let crown = draw_marks(&mut NullRaster, 100, 50, LEADER);
        assert_eq!(crown.len(), 4);
        assert!(crown.iter().all(|(c, _)| *c == CROWN));
        let all = draw_marks(&mut NullRaster, 100, 50, LEADER | HELPER | ASSIST);
        assert_eq!(all.iter().map(|(c, _)| *c).collect::<Vec<_>>(), [vec![DIAMOND], vec![CROSS; 2], vec![CROWN; 4]].concat());
        let xs = all.iter().flat_map(|(_, q)| q.iter().map(|p| p[0]));
        let (lo, hi) = xs.fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(x), b.max(x)));
        assert!(hi <= 100.0 && lo >= 100.0 - marks_width(LEADER | HELPER | ASSIST) as f32, "{lo}..{hi}");
        assert_eq!(marks_width(LEADER | ASSIST), 2 * (MARK_W + MARK_GAP));
    }

    #[test]
    fn it_fits_any_number_above_the_chat_box() {
        // none under it, the pet's bar, the bar and its effects' rows
        let fx = crate::hud::pet_effects_height(crate::hud::PET_FX_MOST);
        assert!(fx > crate::hud::pet_effects_height(1), "two rows for the most");
        for below in [0, PET_H, PET_H + crate::hud::pet_effects_height(1), PET_H + fx] {
            for n in 1..=300 {
                let f = Fit::under(n, below);
                assert_eq!(f.shown + f.more, n, "{n}");
                let b = f.bounds(AT);
                let bottom = AT[1] + ROOM_H - below;
                assert!(b[3] <= bottom, "{n} ({below} under it): down to {} past {bottom}", b[3]);
                assert!(b[2] < 640, "{n}: out to {}", b[2]);
                // no column is empty, and every row showing lies in the list
                assert!(f.columns <= MOST_COLUMNS && (f.columns - 1) * f.rows < f.shown, "{n}: {f:?}");
                for k in 0..f.shown {
                    let r = f.row(AT, k);
                    assert!(r[0] >= b[0] && r[1] >= b[1] && r[2] <= b[2] && r[3] <= b[3], "{n} row {k}: {r:?} of {b:?}");
                }
            }
        }
    }

    #[test]
    fn one_column_for_a_small_raid_more_as_it_grows() {
        let f = |n| Fit::under(n, 0);
        assert_eq!((f(5).columns, f(5).rows, f(5).more), (1, 5, 0));
        assert_eq!((f(6).columns, f(6).rows), (1, 6));
        // a row 18 high (the name, health and power): 12 to a column
        assert_eq!((f(12).columns, f(12).rows), (1, 12));
        assert_eq!((f(13).columns, f(13).rows), (2, 7), "evened out");
        assert_eq!((f(14).columns, f(14).rows), (2, 7));
        assert_eq!((f(20).columns, f(20).rows, f(20).shown), (2, 10, 20));
        assert_eq!((f(48).columns, f(48).rows, f(48).more), (4, 12, 0));
        assert_eq!((f(49).columns, f(49).rows, f(49).shown, f(49).more), (4, 12, 48, 1), "past that the wheel");
        // the pet's bar takes rows
        assert_eq!((Fit::under(12, PET_H).columns, Fit::under(12, PET_H).rows), (2, 6));
    }

    #[test]
    fn a_name_says_who_it_is() {
        let r = Row { seen: true, hp: 200, ..Default::default() };
        assert_eq!(name_colour(&r), NAME);
        assert_eq!(name_colour(&Row { seen: false, ..r.clone() }), NAME_FAR);
        assert_eq!(name_colour(&Row { me: true, ..r.clone() }), NAME_ME);
        assert_eq!(name_colour(&Row { hp: 0, ..r.clone() }), NAME_DEAD);
        assert_eq!(name_colour(&Row { is_target: true, hp: 0, ..r.clone() }), NAME_TARGET);
    }

    #[test]
    fn the_head_says_how_many_and_which() {
        assert_eq!(head(&Fit::under(6, 0), 0), "Raid (6)");
        assert_eq!(head(&Fit::under(60, 0), 3), "Raid (60) 4-51");
    }

    /// Petition 1687's roles: Warrior, Paladin, Shadow Knight a shield; Druid,
    /// Shaman, Cleric a staff; the rest a sword, each in its own colours and
    /// within its 8 by 8 (and the shadow's pixel).
    #[test]
    fn each_class_has_its_role_icon() {
        use crate::pointer::testing::NullRaster;
        let tanks: Vec<u8> = (0..15).filter(|&c| role(c) == Role::Tank).collect();
        let healers: Vec<u8> = (0..15).filter(|&c| role(c) == Role::Healer).collect();
        assert_eq!(tanks, [0, 2, 3]);
        assert_eq!(healers, [7, 8, 9]);
        assert_eq!(role(1), Role::Damage, "ranger");
        assert_eq!(role(14), Role::Damage, "alchemist");
        for (r, colour) in [(Role::Tank, SHIELD), (Role::Healer, STAFF), (Role::Damage, SWORD)] {
            let quads = draw_role(&mut NullRaster, 10, 20, r);
            assert!(quads.iter().any(|(c, _)| *c == colour), "{r:?}");
            for (_, q) in &quads {
                assert!(q.iter().all(|p| (10.0..=18.0).contains(&p[0]) && (20.0..=28.0).contains(&p[1])), "{r:?}: {q:?}");
            }
        }
    }

    fn grouped(groups: &[u8]) -> Vec<Row> {
        groups.iter().enumerate().map(|(i, &g)| Row { id: i as u32 + 1, name: format!("m{i}"), group: g, ..Default::default() }).collect()
    }

    /// The groups in their numbers, each its members in the list's order; an
    /// extra group (a drag's) in its place, empty; none at all from a server
    /// that sends no groups (the control: the old list).
    #[test]
    fn the_rows_go_under_their_groups() {
        let rows = grouped(&[1, 1, 2, 3, 1, 2, 3]);
        let s = sections(&rows, None);
        assert_eq!(s, [Section { group: 1, rows: vec![0, 1, 4] }, Section { group: 2, rows: vec![2, 5] }, Section { group: 3, rows: vec![3, 6] }]);
        let s = sections(&rows, Some(4));
        assert_eq!(s.last(), Some(&Section { group: 4, rows: vec![] }));
        assert!(sections(&grouped(&[0, 0, 0]), None).is_empty(), "no groups sent");
        assert_eq!(most_group(10, 3), 4);
        assert_eq!(most_group(8, 2), 3);
        assert_eq!(most_group(5, 6), 6, "a group in use past it");
    }

    /// Any raid in groups of four fits above the chat box, whole groups in a
    /// column, four columns at most, inside the frame; past that the wheel
    /// steps a group, and the last group is always reachable.
    #[test]
    fn groups_fit_above_the_chat_box() {
        let fx = crate::hud::pet_effects_height(crate::hud::PET_FX_MOST);
        for below in [0, PET_H, PET_H + fx] {
            for n in 1..=200usize {
                let rows = grouped(&(0..n).map(|i| (i / 4 + 1) as u8).collect::<Vec<_>>());
                let secs = sections(&rows, Some((n.div_ceil(4) + 1) as u8));
                for first in [0, 1, 1000] {
                    let f = GroupFit::under(&secs, n, below, first);
                    let b = f.bounds(AT);
                    assert!(b[3] <= AT[1] + ROOM_H - below, "{n} ({below} under): {b:?}");
                    assert!(b[2] < 640 && f.columns.len() <= MOST_COLUMNS, "{n}: {f:?}");
                    assert!(f.columns.iter().all(|c| !c.is_empty()), "{n}: an empty column");
                    let shown: Vec<usize> = f.columns.concat();
                    assert_eq!(shown, (f.first..f.first + shown.len()).collect::<Vec<_>>(), "{n}: in order");
                    if first == 1000 {
                        assert_eq!(*shown.last().unwrap(), secs.len() - 1, "{n}: the wheel reaches the last group");
                    }
                    if f.more == 0 {
                        assert_eq!(shown.len(), secs.len(), "{n}: all shown");
                    }
                }
            }
        }
        // a raid of 8 (two groups and a new one's room): one column; of 24, two
        let rows = grouped(&[1, 1, 1, 1, 2, 2, 2, 2]);
        assert_eq!(GroupFit::under(&sections(&rows, None), 8, 0, 0).columns, [vec![0, 1]]);
        let rows = grouped(&(0..24).map(|i| (i / 4 + 1) as u8).collect::<Vec<_>>());
        let f = GroupFit::under(&sections(&rows, None), 24, 0, 0);
        assert_eq!((f.columns.len(), f.more), (3, 0));
    }
}
