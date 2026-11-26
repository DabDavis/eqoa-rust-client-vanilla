//! The faction tracker (Heirloom petition 3216, Cucumber: "track one chosen
//! faction on screen, outside the faction window"; the owner approved it
//! 2026-11-26): one faction's standing on the HUD, as a number (red below
//! zero, blue above), a bar (the faction window's, from its middle: red to the
//! left, blue to the right, darker toward the ends), or both.
//!
//! The faction is picked in the faction window (a faction's box, ✕ Track: the
//! window's `FactionUp::Track`) or typed (`/track faction <name>`), and kept by
//! its name for the character (`FILE`, as `questtrack`'s), so it is there after
//! a re-login and each character of several follows its own. The game sends standings only while the window is open (0x0AC4,
//! 0x07E2), so go-eqoa-server offers its own (`net::session::
//! OP_FACTION_TRACK_OFFER`): the client names the faction and is sent its
//! standing at once and whenever it changes (a kill's, a quest's, a GM's). The
//! window's pages, when they list it, set it too. Extended > UI > Faction
//! Tracker has its rows (Faction Tracker, off by default; Faction Tracker
//! Shows: Number, Bar or Both). Off, with no faction picked, or on a server
//! that offers none (a PS2's, or world.vanilla_ui), nothing is drawn or asked.
//!
//! On the HUD it sits in the quest tracker's column (`tracker::COLUMN`), above
//! the DPS meter (or the chat box), the quest tracker over it; a piece of its
//! own (`hudlayout::Piece::Faction`) the player can move.

use crate::ui::{Layout, UiRaster};
use render::ui::{Anchor, Vertical};

/// How the standing is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    Number,
    Bar,
    #[default]
    Both,
}

impl Style {
    /// Its name on the Extended page.
    pub fn name(self) -> &'static str {
        match self {
            Style::Number => "Number",
            Style::Bar => "Bar",
            Style::Both => "Both",
        }
    }

    /// In `client.opt`'s line.
    pub fn key(self) -> &'static str {
        match self {
            Style::Number => "number",
            Style::Bar => "bar",
            Style::Both => "both",
        }
    }

    pub fn from_key(s: &str) -> Option<Style> {
        Some(match s {
            "number" => Style::Number,
            "bar" => Style::Bar,
            "both" => Style::Both,
            _ => return None,
        })
    }

    /// The next (`d` > 0) or the one before, round.
    pub fn step(self, d: i32) -> Style {
        const ALL: [Style; 3] = [Style::Number, Style::Bar, Style::Both];
        let i = ALL.iter().position(|&s| s == self).unwrap_or(2) as i32;
        ALL[(i + d.signum()).rem_euclid(3) as usize]
    }

    fn number(self) -> bool {
        self != Style::Bar
    }

    fn bar(self) -> bool {
        self != Style::Number
    }
}

/// The standing last heard for the faction followed: from the server's
/// tracker, or the faction window's page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Heard {
    /// the faction's own name (the server's), or the words asked when unknown
    pub name: String,
    pub standing: i32,
    /// false: the words name no one faction
    pub known: bool,
}

impl Heard {
    /// Whether this is about the faction named `name` (any case).
    pub fn is(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name.trim())
    }
}

/// The file the followed factions are kept in: a line a character
/// (`<server id>:<name>`, a tab, the faction's name), one file for every
/// window, read again before each write (as `questtrack`'s).
pub const FILE: &str = "factiontrack.opt";

/// A name as a line keeps it: no tab or line break in it.
fn clean(name: &str) -> String {
    name.chars().map(|c| if matches!(c, '\t' | '\n' | '\r') { ' ' } else { c }).collect::<String>().trim().to_string()
}

/// The faction the character follows, out of the file's `text` ("" none).
pub fn kept_of(text: &str, server_id: i64, name: &str) -> String {
    let me = format!("{server_id}:{name}");
    text.lines().find_map(|l| l.split_once('\t').filter(|(who, _)| *who == me).map(|(_, f)| f.trim().to_string())).unwrap_or_default()
}

/// The file with this character's line made `faction` (dropped when ""), the
/// others' as they are.
pub fn with_kept(text: &str, server_id: i64, name: &str, faction: &str) -> String {
    let me = format!("{server_id}:{name}");
    let mut out: String = text.lines().filter(|l| l.split('\t').next() != Some(me.as_str()) && !l.trim().is_empty()).map(|l| format!("{l}\n")).collect();
    let f = clean(faction);
    if !f.is_empty() {
        out.push_str(&format!("{me}\t{f}\n"));
    }
    out
}

fn read_file() -> String {
    crate::settings::store_read(FILE).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

/// The character's followed faction, as kept.
#[derive(Default, Debug)]
pub struct Followed {
    who: Option<(i64, String)>,
    name: String,
}

impl Followed {
    /// The character played: its faction read when it is another.
    pub fn character(&mut self, server_id: i64, name: &str) {
        if self.who.as_ref().is_some_and(|(i, n)| *i == server_id && n == name) {
            return;
        }
        *self = Followed { who: Some((server_id, name.to_string())), name: kept_of(&read_file(), server_id, name) };
    }

    /// The faction followed, "" none.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Follows `faction` ("" none), kept for the character.
    pub fn set(&mut self, faction: &str) {
        self.name = clean(faction);
        if let Some((id, who)) = &self.who {
            let text = with_kept(&read_file(), *id, who, &self.name);
            if let Err(e) = crate::settings::store_write(FILE, text.as_bytes()) {
                crate::say(&format!("the tracked faction was not kept: {e}"));
            }
        }
    }
}

/// What the HUD draws.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub heard: Heard,
    pub style: Style,
}

/// The panel's width, its right edge the column's; a row's height; the words'
/// height; the bar's; between it and the panel above or below.
pub const WIDTH: i32 = 170;
const ROW_H: i32 = 12;
const TEXT_H: f32 = 10.0;
const BAR_H: i32 = 6;
pub const GAP: i32 = 4;
/// The name, the number below and above zero and at zero, "Unknown faction".
const NAME: [f32; 4] = crate::tracker::NAME;
pub const RED: [f32; 4] = [1.0, 0.32, 0.32, 1.0];
pub const BLUE: [f32; 4] = [0.45, 0.62, 1.0, 1.0];
const ZERO: [f32; 4] = [0.9, 0.9, 0.86, 1.0];
const UNKNOWN: [f32; 4] = [0.6, 0.6, 0.56, 1.0];
const BACK: [f32; 4] = [0.0, 0.0, 0.0, 0.4];
const TRACK: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const MIDDLE: [f32; 4] = [0.85, 0.85, 0.8, 0.9];
const OUTLINE: [f32; 4] = [0.0, 0.0, 0.0, 0.85];
/// The window's scale's ends (`faction::FactionWnd`'s `DrawStanding`).
pub const MOST: i32 = 2000;

/// The number's colour: red below zero, blue above.
pub fn number_color(v: i32) -> [f32; 4] {
    match v {
        ..0 => RED,
        0 => ZERO,
        _ => BLUE,
    }
}

/// The bar's colour, the faction window's own (`DrawStanding`): darker
/// toward either end.
pub fn bar_color(v: i32) -> [f32; 4] {
    match v {
        ..-700 => [0.5, 0.0, 0.0, 1.0],
        -700..-350 => [0.75, 0.0, 0.0, 1.0],
        -350..-150 => [1.0, 0.0, 0.0, 1.0],
        -150..0 => [1.0, 0.25, 0.25, 1.0],
        0..150 => [0.25, 0.25, 1.0, 1.0],
        150..350 => [0.0, 0.0, 1.0, 1.0],
        350..700 => [0.0, 0.0, 0.75, 1.0],
        _ => [0.0, 0.0, 0.5, 1.0],
    }
}

/// The bar's fill in a track from `x0` to `x1`: from the middle, as long as the
/// standing against `MOST` of the half, a pixel at least off zero; None at zero.
pub fn fill(v: i32, x0: f32, x1: f32) -> Option<[f32; 2]> {
    if v == 0 {
        return None;
    }
    let half = (x1 - x0) / 2.0;
    let mid = x0 + half;
    let len = (v.clamp(-MOST, MOST).unsigned_abs() as f32 / MOST as f32 * half).max(1.0);
    Some(if v < 0 { [mid - len, mid] } else { [mid, mid + len] })
}

/// Where the panel is: its rectangle, its words' row and its bar's.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub rect: [i32; 4],
    pub text_y: i32,
    pub bar: Option<[i32; 4]>,
}

/// The panel for `s` with its bottom at `bottom` (the column's, or above the
/// DPS meter), its right the column's: the words' row, and the bar's under it
/// when the style has one (none for a name that is no faction's).
pub fn panel(s: &Shown, bottom: i32) -> Panel {
    let col = crate::tracker::COLUMN;
    let bar = s.style.bar() && s.heard.known;
    let h = ROW_H + if bar { BAR_H + 3 } else { 0 } + 3;
    let (x0, x1) = (col[2] - WIDTH, col[2]);
    let top = bottom - h;
    Panel {
        rect: [x0, top, x1, bottom],
        text_y: top + 2,
        bar: bar.then(|| [x0 + 4, top + 2 + ROW_H, x1 - 4, top + 2 + ROW_H + BAR_H]),
    }
}

/// The words of the row: the name, and the number (or "?" for a name that is
/// no faction's) right-aligned, the number signed ("+120", "-49", "0").
pub fn words(s: &Shown) -> (String, Option<(String, [f32; 4])>) {
    let name = s.heard.name.trim().to_string();
    let n = if !s.heard.known {
        Some(("Unknown faction".to_string(), UNKNOWN))
    } else if s.style.number() {
        let v = s.heard.standing;
        Some((if v > 0 { format!("+{v}") } else { v.to_string() }, number_color(v)))
    } else {
        None
    };
    (name, n)
}

/// Draws the panel `p` for `s` in the HUD's frame (`Anchor::Right`,
/// `Vertical::Top`, as the quest tracker).
pub fn draw(ui: &mut UiRaster, s: &Shown, p: &Panel) {
    ui.ui.anchor = Anchor::Right;
    ui.ui.vertical = Vertical::Top;
    let r = p.rect.map(|v| v as f32);
    ui.solid(3, BACK, corners(r));
    if let Some(b) = p.bar {
        let b = b.map(|v| v as f32);
        ui.solid(3, TRACK, corners(b));
        if let Some([a, z]) = fill(s.heard.standing, b[0], b[2]) {
            ui.solid(3, bar_color(s.heard.standing), corners([a, b[1], z, b[3]]));
        }
        let mid = ((b[0] + b[2]) / 2.0).floor();
        ui.solid(3, MIDDLE, corners([mid, b[1] - 1.0, mid + 1.0, b[3] + 1.0]));
    }
    let Some(font) = ui.assets.font else { return };
    let scale = TEXT_H / ui.ui.fonts[font].height.max(1) as f32;
    let (name, n) = words(s);
    let width = |ui: &UiRaster, t: &str| ui.ui.fonts[font].string_width(&vi::ui::wide(t)) as f32 * scale;
    let (x0, x1, y) = (r[0] + 4.0, r[2] - 4.0, p.text_y as f32);
    let room = x1 - x0 - n.as_ref().map_or(0.0, |(t, _)| width(ui, t) + 6.0);
    let name = cut(&name, room, |t| width(ui, t));
    let put = |ui: &mut UiRaster, x: f32, t: &str, c: [f32; 4]| {
        let w = vi::ui::wide(t);
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
            ui.ui.text_scaled(font, x + dx, y + dy, scale, OUTLINE, &w);
        }
        ui.ui.text_scaled(font, x, y, scale, c, &w);
    };
    put(ui, x0, &name, NAME);
    if let Some((t, c)) = n {
        let x = x1 - width(ui, &t);
        put(ui, x, &t, c);
    }
}

/// `text` no wider than `most` by `width`, cut at a character with "...".
fn cut(text: &str, most: f32, width: impl Fn(&str) -> f32) -> String {
    if width(text) <= most {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let s = chars.iter().collect::<String>().trim_end().to_string() + "...";
        if width(&s) <= most {
            return s;
        }
    }
    String::new()
}

fn corners(r: [f32; 4]) -> [[f32; 2]; 4] {
    [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]]
}

/// `/track`'s words: `faction <name>` follows that faction (turning the
/// tracker on), `faction` or `off` alone stops; `on` shows it again; anything
/// else, how to type it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Faction(String),
    Stop,
    On,
    Help,
}

pub fn command(arg: &str) -> Command {
    let arg = arg.trim();
    let (word, rest) = arg.split_once(' ').unwrap_or((arg, ""));
    match (word.to_ascii_lowercase().as_str(), rest.trim()) {
        ("faction" | "f", "") | ("off" | "none" | "stop", _) => Command::Stop,
        ("faction" | "f", name) => Command::Faction(name.to_string()),
        ("on", _) => Command::On,
        _ => Command::Help,
    }
}

/// What `/track` with no meant word says.
pub const HELP: &str = "/track faction <name>: show a faction's standing on screen; /track off: stop; or pick one in the faction window (a faction's box, Track)";

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(name: &str, standing: i32, known: bool, style: Style) -> Shown {
        Shown { heard: Heard { name: name.into(), standing, known }, style }
    }

    /// The number red below zero, blue above, plain at zero, signed; Bar shows
    /// no number; a name that is no faction's says so, with no bar.
    #[test]
    fn the_number_is_red_or_blue() {
        assert_eq!(words(&shown("Freeport Militia", -49, true, Style::Both)), ("Freeport Militia".into(), Some(("-49".into(), RED))));
        assert_eq!(words(&shown("Freeport Militia", 120, true, Style::Number)), ("Freeport Militia".into(), Some(("+120".into(), BLUE))));
        assert_eq!(words(&shown("Freeport Militia", 0, true, Style::Both)).1, Some(("0".into(), ZERO)));
        assert_eq!(words(&shown("Freeport Militia", -49, true, Style::Bar)).1, None, "Bar: no number");
        assert_eq!(words(&shown("zzz", 0, false, Style::Bar)).1, Some(("Unknown faction".into(), UNKNOWN)));
        assert!(panel(&shown("zzz", 0, false, Style::Both), 300).bar.is_none(), "no bar for no faction");
    }

    /// The bar from the middle: left for below zero, right for above, as long
    /// as the standing against 2000 of the half, held there; a pixel at least;
    /// none at zero. Its colours the window's.
    #[test]
    fn the_bar_grows_from_the_middle() {
        assert_eq!(fill(-1000, 0.0, 100.0), Some([25.0, 50.0]));
        assert_eq!(fill(2000, 0.0, 100.0), Some([50.0, 100.0]));
        assert_eq!(fill(9999, 0.0, 100.0), Some([50.0, 100.0]), "held to the end");
        assert_eq!(fill(1, 0.0, 100.0), Some([50.0, 51.0]), "a pixel at least");
        assert_eq!(fill(0, 0.0, 100.0), None);
        assert_eq!(bar_color(-800), [0.5, 0.0, 0.0, 1.0]);
        assert_eq!(bar_color(100), [0.25, 0.25, 1.0, 1.0]);
    }

    /// The panel sits on `bottom` at the column's right; Number is one row,
    /// Bar and Both a row and the bar.
    #[test]
    fn the_panel_sits_on_its_bottom() {
        let n = panel(&shown("A", 5, true, Style::Number), 300);
        let b = panel(&shown("A", 5, true, Style::Both), 300);
        assert_eq!((n.rect[3], n.rect[2], n.rect[2] - n.rect[0]), (300, crate::tracker::COLUMN[2], WIDTH));
        assert!(n.bar.is_none() && b.bar.is_some());
        assert!(b.rect[1] < n.rect[1], "the bar's row taller");
        let bar = b.bar.unwrap();
        assert!(bar[1] > b.text_y && bar[3] < b.rect[3], "the bar under the words, in the panel");
    }

    /// A character's faction written and read back by name, another's left as
    /// it was; "" drops the line. The control: another character reads none.
    #[test]
    fn the_followed_faction_round_trips() {
        let other = "7:Bo\tCitizens of Qeynos\n";
        let text = with_kept(other, 12, "Cucumber", "The Freeport Militia");
        assert_eq!(text, "7:Bo\tCitizens of Qeynos\n12:Cucumber\tThe Freeport Militia\n");
        assert_eq!(kept_of(&text, 12, "Cucumber"), "The Freeport Militia");
        assert_eq!(kept_of(&text, 7, "Bo"), "Citizens of Qeynos");
        assert_eq!(kept_of(&text, 13, "Cucumber"), "", "the control");
        let text = with_kept(&text, 12, "Cucumber", "Citizens of Freeport");
        assert_eq!(kept_of(&text, 12, "Cucumber"), "Citizens of Freeport", "replaced, not added");
        assert_eq!(text.lines().count(), 2);
        assert_eq!(with_kept(&text, 12, "Cucumber", ""), other, "none: the line dropped");
    }

    /// The styles round; their keys read back.
    #[test]
    fn styles_step_round() {
        assert_eq!(Style::default(), Style::Both);
        assert_eq!([Style::Number.step(1), Style::Both.step(1), Style::Number.step(-1)], [Style::Bar, Style::Number, Style::Both]);
        for s in [Style::Number, Style::Bar, Style::Both] {
            assert_eq!(Style::from_key(s.key()), Some(s));
        }
        assert_eq!(Style::from_key("x"), None);
    }

    /// `/track faction <name>` keeps the name's own words (case too); `off`,
    /// or `faction` alone, stops.
    #[test]
    fn track_commands() {
        assert_eq!(command("faction Citizens of Freeport"), Command::Faction("Citizens of Freeport".into()));
        assert_eq!(command("FACTION  militia "), Command::Faction("militia".into()));
        assert_eq!(command("faction"), Command::Stop);
        assert_eq!(command("off"), Command::Stop);
        assert_eq!(command("on"), Command::On);
        assert_eq!(command(""), Command::Help);
        assert_eq!(command("quest x"), Command::Help);
        assert!(Heard { name: "Citizens of Freeport".into(), standing: 0, known: true }.is(" citizens of freeport"));
    }
}
