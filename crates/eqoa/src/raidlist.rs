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
        let (x, y) = (r[0] + 2, r[1]);
        // the name cut to its column, less its marks
        let marks = marks_width(row.marks);
        let mut name = wide(&row.name);
        while let Some(f) = font
            && !name.is_empty()
            && ui.ui.fonts[f].string_width(&name) as f32 * scale > (COL_W - 4 - marks) as f32
        {
            name.pop();
        }
        text(ui, x, y, name_colour(row), &name);
        if marks > 0 {
            draw_marks(ui, r[2] - 2, y, row.marks);
        }
        let bar = [x, y + ROW_H - BAR_H - POWER_H - 1, r[2] - 2, y + ROW_H - 1];
        ui.solid(3, BAR_BACK, corners(bar));
        // health in the HUD's own gauge (7), and under it power in its blue (9)
        for (v, image, top, h) in [(row.hp, 7, bar[1], BAR_H), (row.power, 9, bar[1] + BAR_H, POWER_H)] {
            let w = ((bar[2] - bar[0]) as f32 * v as f32 / 255.0).round() as i32;
            if w > 0 {
                vui.stretch_blit_ex(ui, image, [bar[0], top], [w, h], 3, 1.0);
            }
        }
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
}
