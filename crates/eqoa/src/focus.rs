//! Focus Target (Heirloom petition 3038, Cucumber: "Set a target as focus
//! target. So if you spot something, you can Focus it adding an additional
//! target to use. Useful for healers, tanks and DPS!"; the owner's yes of
//! 2026-11-25): the client's own, a second creature kept beside the target.
//!
//! Set Focus (the Controls page's row, `[` by default, or `/focus`) makes the
//! target the focus; pressed again on the same target, or with no target, it
//! lets the focus go. Target Focus (`]`, or `/focus target`) targets the focus
//! as the target cycle leaves a choice (`Play::target_id`: 0x0010, the game's
//! own message; nothing new goes to a server). While a focus is held a small
//! window shows it (`draw`, `hudlayout::Piece::Focus`, above the target box):
//! its name in its con colour, its health as the state channel's +0x19 gives
//! it (the target box's gauge's, `hud::health_text`), and how far it is.
//!
//! The con colour is the game's (`rings::CON_COLORS`, `TrackTarget`'s), which
//! only a target info (0x0010's answer) tells: the one in force when the focus
//! was set, and each later one while the focus is the target. None known, the
//! name is white.
//!
//! The focus goes when its creature does (`holds`): out of the entities (out
//! of view, despawned, a world changed), dead (+0x19 at 0, as
//! `drop_dead_target_info` reads a death), in another world, or its id taken
//! by another creature (the name kept at the set no longer its). No
//! controller button by default (the pad's every button is the game's); one
//! can be given on the Controls page. Not automation (nothing is pressed for
//! the player), so no server's QoL bit turns it off.

use crate::ui::{Layout, UiRaster, wide};
use render::ui::{Anchor, Vertical};

/// The focus: whom, its name at the set (an id the server gives again to
/// another is not it), and its con (`rings::CON_COLORS`' index) when known.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Focus {
    pub id: u32,
    pub name: String,
    pub con: Option<u8>,
}

/// The target as Set Focus sees it: its id, its name and whether it lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate<'a> {
    pub id: u32,
    pub name: &'a str,
    pub alive: bool,
}

/// What Set Focus did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pressed {
    /// the target is the focus now
    Set(String),
    /// the focus let go (the same target again, or none)
    Cleared,
    /// no target and no focus: nothing
    NoTarget,
    /// a dead target: not taken
    Dead(String),
}

/// Set Focus: `target` the target (a creature), `con` the con of its target
/// info when known. The focus's own target again, or no target, lets the
/// focus go; a living target becomes it; a dead one is refused.
pub fn press(focus: &mut Option<Focus>, target: Option<Candidate>, con: Option<u8>) -> Pressed {
    match target {
        Some(t) if focus.as_ref().is_some_and(|f| f.id == t.id && f.name == t.name) => {
            *focus = None;
            Pressed::Cleared
        }
        Some(t) if !t.alive => Pressed::Dead(t.name.to_string()),
        Some(t) => {
            *focus = Some(Focus { id: t.id, name: t.name.to_string(), con });
            Pressed::Set(t.name.to_string())
        }
        None if focus.take().is_some() => Pressed::Cleared,
        None => Pressed::NoTarget,
    }
}

/// Whether the focus still holds against the entity of its id (`e`: its
/// creature's name, its +0x19, its world) in world `world`.
pub fn holds(focus: &Focus, e: Option<(&str, u8, i64)>, world: i64) -> bool {
    e.is_some_and(|(name, hp, w)| name == focus.name && hp != 0 && w == world)
}

/// Each of the game's con colours (`rings::CON_COLORS`) as a text colour; none
/// known, white.
pub fn con_color(con: Option<u8>) -> [f32; 4] {
    match con.and_then(|c| crate::rings::CON_COLORS.get(c as usize)) {
        Some(&[r, g, b]) => [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0],
        None => WHITE,
    }
}

/// What the window shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub name: String,
    pub con: Option<u8>,
    /// +0x19, of 255
    pub hp: u8,
    /// from the player, in the world's units; none while the player's own
    /// place is not known
    pub distance: Option<f32>,
}

/// How far a focus `d` away reads: whole units.
pub fn distance_text(d: f32) -> String {
    format!("{} away", d.max(0.0).round() as i64)
}

/// The window's top left in the HUD's frame (left, `Vertical::Bottom`: above
/// the target box's "Target", `hud::TARGET_AT`'s 348), its width (the target
/// bar's 174), its rows.
pub const AT: [i32; 2] = [20, 298];
pub const WIDTH: i32 = 174;
pub const ROW_H: i32 = 13;
const BAR_H: i32 = 8;
const TEXT_H: f32 = 10.0;
/// Its title.
pub const TITLE: &str = "Focus";

/// The window's rectangle (frame pixels at its own place).
pub fn rect() -> [i32; 4] {
    [AT[0], AT[1], AT[0] + WIDTH, AT[1] + 2 * ROW_H + BAR_H + 7]
}

/// The health bar's rectangle and its fill's right edge for `hp` of 255.
pub fn bar(hp: u8) -> ([f32; 4], f32) {
    let r = rect();
    let (x0, x1) = (r[0] as f32 + 4.0, r[2] as f32 - 40.0);
    let y0 = (r[1] + 2 * ROW_H + 2) as f32;
    ([x0, y0, x1, y0 + BAR_H as f32], x0 + (x1 - x0) * hp as f32 / 255.0)
}

const WHITE: [f32; 4] = [0.95, 0.95, 0.92, 1.0];
const LABEL: [f32; 4] = [1.0, 0.86, 0.35, 1.0];
const BACK: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const BAR_BACK: [f32; 4] = [0.15, 0.15, 0.15, 0.9];
const BAR_FILL: [f32; 4] = [0.78, 0.12, 0.10, 1.0];
const OUTLINE: [f32; 4] = [0.0, 0.0, 0.0, 0.85];

fn corners(r: [f32; 4]) -> [[f32; 2]; 4] {
    [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]]
}

/// Draws the window for `s` in the piece's own place (`hudlayout::begin(
/// Piece::Focus)` round it): "Focus" and the distance, the name in its con
/// colour, the health bar and its percent.
pub fn draw(ui: &mut UiRaster, s: &Shown) {
    let (anchor, vertical) = (ui.ui.anchor, ui.ui.vertical);
    ui.ui.anchor = Anchor::Left;
    ui.ui.vertical = Vertical::Bottom;
    let r = rect().map(|v| v as f32);
    ui.solid(3, BACK, corners(r));
    let (b, fill) = bar(s.hp);
    ui.solid(3, BAR_BACK, corners(b));
    if fill > b[0] {
        ui.solid(3, BAR_FILL, corners([b[0], b[1], fill, b[3]]));
    }
    if let Some(font) = ui.assets.font {
        let scale = TEXT_H / ui.ui.fonts[font].height.max(1) as f32;
        let width = |ui: &UiRaster, s: &[u16]| ui.ui.fonts[font].string_width(s) as f32 * scale;
        let text = |ui: &mut UiRaster, x: f32, y: f32, color: [f32; 4], s: &str| {
            let w = wide(s);
            for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                ui.ui.text_scaled(font, x + dx, y + dy, scale, OUTLINE, &w);
            }
            ui.ui.text_scaled(font, x, y, scale, color, &w);
        };
        let ty = |row: i32| r[1] + 2.0 + (row * ROW_H) as f32;
        text(ui, r[0] + 4.0, ty(0), LABEL, TITLE);
        if let Some(d) = s.distance {
            let t = distance_text(d);
            let w = width(ui, &wide(&t));
            text(ui, r[2] - 4.0 - w, ty(0), WHITE, &t);
        }
        // the name, cut to the window's width
        let mut name: Vec<char> = s.name.chars().collect();
        while !name.is_empty() && width(ui, &wide(&name.iter().collect::<String>())) > (WIDTH - 8) as f32 {
            name.pop();
        }
        text(ui, r[0] + 4.0, ty(1), con_color(s.con), &name.iter().collect::<String>());
        let pct = crate::hud::health_text(s.hp);
        let w = width(ui, &wide(&pct));
        text(ui, r[2] - 4.0 - w, b[1] - 1.0, WHITE, &pct);
    }
    (ui.ui.anchor, ui.ui.vertical) = (anchor, vertical);
}

/// `/focus`' words: none, Set Focus's press; `target`, Target Focus; `clear`
/// or `off`, the focus let go; anything else, how to type it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Press,
    Target,
    Clear,
    Help,
}

pub fn command(arg: &str) -> Command {
    match arg.trim().to_ascii_lowercase().as_str() {
        "" | "set" => Command::Press,
        "target" | "t" => Command::Target,
        "clear" | "off" | "none" => Command::Clear,
        _ => Command::Help,
    }
}

/// What `/focus` with no meant word says.
pub const HELP: &str = "/focus: your target as your focus (again, or with no target, lets it go); /focus target targets it; /focus clear lets it go";

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: u32, name: &str, alive: bool) -> Candidate<'_> {
        Candidate { id, name, alive }
    }

    /// Set Focus takes the target, its con with it; the same target again
    /// lets it go; another target takes its place; no target lets it go, and
    /// with none held says there is none. The control: a dead target is
    /// refused and the focus held is kept.
    #[test]
    fn set_focus_takes_toggles_and_clears() {
        let mut f = None;
        assert_eq!(press(&mut f, None, None), Pressed::NoTarget);
        assert_eq!(press(&mut f, Some(cand(7, "a gnoll", true)), Some(1)), Pressed::Set("a gnoll".into()));
        assert_eq!(f, Some(Focus { id: 7, name: "a gnoll".into(), con: Some(1) }));
        assert_eq!(press(&mut f, Some(cand(9, "a bat", true)), None), Pressed::Set("a bat".into()));
        assert_eq!(f.as_ref().map(|f| f.id), Some(9), "another target takes its place");
        assert_eq!(press(&mut f, Some(cand(4, "a corpse", false)), None), Pressed::Dead("a corpse".into()));
        assert_eq!(f.as_ref().map(|f| f.id), Some(9), "the control: a dead one is not taken");
        assert_eq!(press(&mut f, Some(cand(9, "a bat", true)), None), Pressed::Cleared, "the focus's own target again");
        assert_eq!(f, None);
        press(&mut f, Some(cand(9, "a bat", true)), None);
        assert_eq!(press(&mut f, None, None), Pressed::Cleared, "no target lets it go");
        assert_eq!(f, None);
        // an id given again to another creature is not the focus's
        press(&mut f, Some(cand(9, "a bat", true)), None);
        assert_eq!(press(&mut f, Some(cand(9, "a rat", true)), None), Pressed::Set("a rat".into()));
    }

    /// The focus holds while its creature is in, by its name, alive and in
    /// this world; each of those gone lets it go.
    #[test]
    fn the_focus_goes_with_its_creature() {
        let f = Focus { id: 7, name: "a gnoll".into(), con: None };
        assert!(holds(&f, Some(("a gnoll", 200, 0)), 0), "the control: there, alive");
        assert!(!holds(&f, None, 0), "out of view or despawned");
        assert!(!holds(&f, Some(("a gnoll", 0, 0)), 0), "dead");
        assert!(!holds(&f, Some(("a gnoll", 200, 1)), 0), "in another world");
        assert!(!holds(&f, Some(("a bat", 200, 0)), 0), "its id another's");
    }

    #[test]
    fn the_con_colour_is_the_games() {
        assert_eq!(con_color(Some(0)), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(con_color(Some(5)), [0.0, 128.0 / 255.0, 0.0, 1.0]);
        assert_eq!(con_color(None), WHITE);
        assert_eq!(con_color(Some(40)), WHITE, "out of the table");
    }

    #[test]
    fn the_bar_fills_by_health() {
        let (b, full) = bar(255);
        assert_eq!(full, b[2]);
        assert_eq!(bar(0).1, b[0]);
        let half = bar(128).1;
        assert!(half > b[0] && half < b[2]);
        assert_eq!(distance_text(12.6), "13 away");
    }

    #[test]
    fn the_commands_words() {
        assert_eq!(command(""), Command::Press);
        assert_eq!(command(" Target "), Command::Target);
        assert_eq!(command("clear"), Command::Clear);
        assert_eq!(command("off"), Command::Clear);
        assert_eq!(command("what"), Command::Help);
    }
}
