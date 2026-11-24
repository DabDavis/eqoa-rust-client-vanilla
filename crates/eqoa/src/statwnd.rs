//! The Stat Window (petition 2925: "a stat box ... health, power, resists,
//! AC. Would it be possible to also include OFF MODS, DEF MODS ... but also
//! HoT and PoT"): the client's own, a HUD panel of what the stat channel
//! (0x42, `net::dump::read_stats`: `m_stat` and `m_baseStat`, 29 each) already
//! tells the client, kept up to date as it changes: health and power (now and
//! most), HoT and PoT (stats 12 and 13, CLIENT's words 240 and 241: what the
//! six-second tick gives back), AC (14), the seven stats with what items and
//! spells add over the base, and the six resists as Character > Summary's
//! Details shows them (`trainer::Data::derived`: CLIENT's `CurStat`).
//!
//! OFF and DEF MOD are not in the channel (go-eqoa-server's are fitted
//! placeholders no roll reads, `#stats`), so they are not shown: a server
//! would have to send them.
//!
//! Opened and closed by its key (F4, the Controls page's "Stat Window"),
//! `/stats`, or Extended > UI > HUD Extras' Stat Window; its title's X closes
//! it. A piece of the HUD to move (`hudlayout::Piece::Stats`), at the left by
//! default. A server that turns Character > Summary's stat caps off (the game's
//! own UI, `settings::qol::STAT_CAPS`) turns it off with them.

use crate::ui::{Layout, UiRaster, wide};
use render::ui::{Anchor, Vertical};

/// The panel's top left in the HUD's frame (left, `Vertical::Middle`: under
/// the group's bars at 4:3), its width, a row's height and the title's.
pub const AT: [i32; 2] = [6, 150];
pub const WIDTH: i32 = 196;
pub const ROW_H: i32 = 12;
pub const HEAD_H: i32 = 14;
/// A section's gap under its rows.
const GAP: i32 = 4;
const TEXT_H: f32 = 10.0;
/// Its title.
pub const TITLE: &str = "Stats";

/// What the panel shows, out of the stat channel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Facts {
    /// health and power: (now, most)
    pub hp: [i32; 2],
    pub power: [i32; 2],
    pub hot: i32,
    pub pot: i32,
    pub ac: i32,
    /// STR STA AGI DEX WIS INT CHA: (now, base)
    pub stats: [[i32; 2]; 7],
    /// fire, ice (cold), lightning, arcane, poison, disease, as CLIENT
    /// derives them (`RESISTS`' order)
    pub resists: [i32; 6],
}

/// The seven stats' words (Str .. Cha).
pub const STAT_WORDS: [i32; 7] = [226, 227, 228, 229, 230, 231, 232];
/// The resists: (stat, word), Summary's Details' (`trainer`'s `sDetails`)
/// order and words: Fire, Ice, Lightning, Arcane, Poison, Disease.
pub const RESISTS: [(i32, i32); 6] = [(24, 48), (25, 483), (26, 484), (27, 485), (22, 486), (23, 487)];
/// HP, Power, HoT, PoT and AC's words.
pub const HP_WORD: i32 = 233;
pub const POWER_WORD: i32 = 466;
pub const HOT_WORD: i32 = 240;
pub const POT_WORD: i32 = 241;
pub const AC_WORD: i32 = 242;

/// The facts out of the stat channel's two arrays (`cur`: `m_stat`, `base`:
/// `m_baseStat`).
pub fn facts(cur: &[i32; 29], base: &[i32; 29]) -> Facts {
    Facts {
        hp: [cur[7], cur[8]],
        power: [cur[9], cur[10]],
        hot: cur[12],
        pot: cur[13],
        ac: cur[14],
        stats: std::array::from_fn(|i| [cur[i], base[i]]),
        resists: RESISTS.map(|(s, _)| crate::trainer::Data::derived(cur, s)),
    }
}

/// One entry: its label, its value and (dim, after it) what is added over
/// the base, at a column (0 left, 1 right) of a row.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub label: String,
    pub value: String,
    pub extra: String,
    pub col: usize,
    pub row: usize,
}

/// What is added over the base: "+25", "-5", none for nothing.
pub fn bonus(now: i32, base: i32) -> String {
    match now - base {
        0 => String::new(),
        d => format!("{d:+}"),
    }
}

/// The entries for `f`, the game's words by `word`: health and power, then
/// HoT, PoT and AC; then the seven stats, two to a row; then the resists, two
/// to a row. With the rows each section takes (its last row's index + 1).
pub fn entries(f: &Facts, word: &dyn Fn(i32) -> String) -> (Vec<Entry>, [usize; 3]) {
    let mut out = Vec::new();
    let e = |label: String, value: String, extra: String, i: usize, first: usize| Entry { label, value, extra, col: i % 2, row: first + i / 2 };
    let vitals = [
        (word(HP_WORD), format!("{}/{}", f.hp[0], f.hp[1])),
        (word(POWER_WORD), format!("{}/{}", f.power[0], f.power[1])),
        (word(HOT_WORD), f.hot.to_string()),
        (word(POT_WORD), f.pot.to_string()),
        (word(AC_WORD), f.ac.to_string()),
    ];
    for (i, (l, v)) in vitals.into_iter().enumerate() {
        out.push(e(l, v, String::new(), i, 0));
    }
    let s1 = 3;
    for (i, &w) in STAT_WORDS.iter().enumerate() {
        let [now, base] = f.stats[i];
        out.push(e(word(w), now.to_string(), bonus(now, base), i, s1));
    }
    let s2 = s1 + 4;
    for (i, &(_, w)) in RESISTS.iter().enumerate() {
        out.push(e(word(w), f.resists[i].to_string(), String::new(), i, s2));
    }
    (out, [s1, s2, s2 + 3])
}

/// Where the panel is: its rectangle, its title's and its X's (frame pixels,
/// the piece's own place).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Panel {
    pub rect: [i32; 4],
    pub close: [i32; 4],
}

/// The panel for `rows` (`entries`' sections) at `AT`.
pub fn panel(sections: [usize; 3]) -> Panel {
    let h = HEAD_H + sections[2] as i32 * ROW_H + 2 * GAP + 3;
    let rect = [AT[0], AT[1], AT[0] + WIDTH, AT[1] + h];
    Panel { rect, close: [rect[2] - HEAD_H, rect[1], rect[2], rect[1] + HEAD_H] }
}

/// A row's top in `p` (its section's gaps above it).
pub fn row_top(p: &Panel, sections: [usize; 3], row: usize) -> i32 {
    let gaps = sections[..2].iter().filter(|&&s| row >= s).count() as i32;
    p.rect[1] + HEAD_H + 2 + row as i32 * ROW_H + gaps * GAP
}

const WHITE: [f32; 4] = [0.95, 0.95, 0.92, 1.0];
const LABEL: [f32; 4] = [1.0, 0.86, 0.35, 1.0];
const DIM: [f32; 4] = [0.55, 0.85, 0.55, 1.0];
const BACK: [f32; 4] = [0.0, 0.0, 0.0, 0.55];
const HEAD_BACK: [f32; 4] = [0.0, 0.0, 0.0, 0.35];
const RULE: [f32; 4] = [1.0, 0.86, 0.35, 0.35];
const OUTLINE: [f32; 4] = [0.0, 0.0, 0.0, 0.85];

fn corners(r: [f32; 4]) -> [[f32; 2]; 4] {
    [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]]
}

/// Draws the panel for `f` with the game's words by `word`, in the piece's
/// own place (`hudlayout::begin(Piece::Stats)` round it): its title `title`
/// and X, then each entry, its label gold, its value white, what is added over
/// the base green after it. Returns the panel (for the pointer).
pub fn draw(ui: &mut UiRaster, f: &Facts, title: &str, word: &dyn Fn(i32) -> String) -> Panel {
    let (anchor, vertical) = (ui.ui.anchor, ui.ui.vertical);
    ui.ui.anchor = Anchor::Left;
    ui.ui.vertical = Vertical::Middle;
    let (es, sections) = entries(f, word);
    let p = panel(sections);
    let r = p.rect.map(|v| v as f32);
    ui.solid(3, BACK, corners(r));
    ui.solid(3, HEAD_BACK, corners([r[0], r[1], r[2], r[1] + HEAD_H as f32]));
    for &s in &sections[..2] {
        let y = row_top(&p, sections, s) as f32 - GAP as f32 / 2.0 - 1.0;
        ui.solid(3, RULE, corners([r[0] + 4.0, y, r[2] - 4.0, y + 1.0]));
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
        let ty = |top: i32| top as f32 + ((ROW_H as f32 - TEXT_H) / 2.0).floor();
        text(ui, r[0] + 4.0, ty(p.rect[1]) + 1.0, WHITE, title);
        let xw = width(ui, &wide("X"));
        text(ui, (p.close[0] + p.close[2]) as f32 / 2.0 - xw / 2.0, ty(p.rect[1]) + 1.0, WHITE, "X");
        let half = WIDTH as f32 / 2.0;
        for e in &es {
            let x0 = r[0] + 4.0 + e.col as f32 * half;
            let y = ty(row_top(&p, sections, e.row));
            text(ui, x0, y, LABEL, &e.label);
            // the value at the column's right, what is added after it
            let right = x0 + half - 8.0;
            let ew = if e.extra.is_empty() { 0.0 } else { width(ui, &wide(&e.extra)) + 3.0 };
            let vw = width(ui, &wide(&e.value));
            text(ui, right - ew - vw, y, WHITE, &e.value);
            if !e.extra.is_empty() {
                text(ui, right - ew + 3.0, y, DIM, &e.extra);
            }
        }
    }
    (ui.ui.anchor, ui.ui.vertical) = (anchor, vertical);
    p
}

/// `/stats`' words: no word, the window turned; `on` or `off`, that; anything
/// else, how to type it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Toggle,
    On(bool),
    Help,
}

pub fn command(arg: &str) -> Command {
    match arg.trim().to_ascii_lowercase().as_str() {
        "" => Command::Toggle,
        "on" | "show" => Command::On(true),
        "off" | "hide" => Command::On(false),
        _ => Command::Help,
    }
}

/// What `/stats` with no meant word says.
pub const HELP: &str = "/stats: the Stat Window shown or hidden; /stats on, /stats off";

#[cfg(test)]
mod tests {
    use super::*;

    fn arrays() -> ([i32; 29], [i32; 29]) {
        let mut cur = [0; 29];
        let mut base = [0; 29];
        for i in 0..7 {
            cur[i] = 100 + i as i32;
            base[i] = 90 + i as i32;
        }
        cur[2] = base[2];
        cur[6] = base[6] - 5;
        (cur[7], cur[8], cur[9], cur[10]) = (750, 1000, 460, 700);
        (cur[12], cur[13], cur[14]) = (20, 14, 529);
        for (k, s) in (22..28).enumerate() {
            cur[s] = 10 * k as i32;
        }
        (cur, base)
    }

    /// The facts are the channel's: health, power, HoT (12), PoT (13), AC
    /// (14), the stats with their bases, the resists as CLIENT derives them
    /// (40 + the resist + a seventh of WIS, 315 at most) in Summary's order.
    #[test]
    fn the_facts_are_the_stat_channels() {
        let (cur, base) = arrays();
        let f = facts(&cur, &base);
        assert_eq!((f.hp, f.power), ([750, 1000], [460, 700]));
        assert_eq!((f.hot, f.pot, f.ac), (20, 14, 529));
        assert_eq!(f.stats[0], [100, 90]);
        let wis7 = cur[4] / 7;
        // 24 (fire) holds 20, 25 (ice) 30 ... 22 (poison) 0, 23 (disease) 10
        assert_eq!(f.resists, [20, 30, 40, 50, 0, 10].map(|v| v + 40 + wis7));
        let mut high = cur;
        high[24] = 400;
        assert_eq!(facts(&high, &base).resists[0], 315, "CLIENT's cap");
    }

    /// The entries: five vitals in three rows, the seven stats in four (each
    /// with what is added over its base, none when nothing is, a minus when
    /// less), the six resists in three; the game's words for their labels.
    #[test]
    fn the_entries_read_as_the_window_shows_them() {
        let (cur, base) = arrays();
        let word = |w: i32| format!("w{w}");
        let (es, sections) = entries(&facts(&cur, &base), &word);
        assert_eq!(sections, [3, 7, 10]);
        assert_eq!(es.len(), 5 + 7 + 6);
        let at = |label: &str| es.iter().find(|e| e.label == label).unwrap().clone();
        assert_eq!(at("w233"), Entry { label: "w233".into(), value: "750/1000".into(), extra: String::new(), col: 0, row: 0 });
        assert_eq!((at("w466").value, at("w466").col, at("w466").row), ("460/700".into(), 1, 0), "power beside health");
        assert_eq!((at("w240").value, at("w241").value, at("w242").value), ("20".into(), "14".into(), "529".into()));
        assert_eq!((at("w242").col, at("w242").row), (0, 2), "AC on the third row");
        assert_eq!((at("w226").value, at("w226").extra, at("w226").row), ("100".into(), "+10".into(), 3), "STR, its items' and spells' share");
        assert_eq!(at("w228").extra, "", "nothing added: nothing said");
        assert_eq!(at("w232").extra, "-5", "less than the base");
        assert_eq!((at("w232").col, at("w232").row), (0, 6));
        assert_eq!((at("w48").row, at("w487").col, at("w487").row), (7, 1, 9), "fire first, disease last");
    }

    /// The panel: its rows under the title, a gap after each section, the X at
    /// the title's right; the rows inside it.
    #[test]
    fn the_panel_holds_its_rows() {
        let sections = [3, 7, 10];
        let p = panel(sections);
        assert_eq!(p.rect, [AT[0], AT[1], AT[0] + WIDTH, AT[1] + HEAD_H + 10 * ROW_H + 2 * GAP + 3]);
        assert_eq!(p.close, [AT[0] + WIDTH - HEAD_H, AT[1], AT[0] + WIDTH, AT[1] + HEAD_H]);
        assert_eq!(row_top(&p, sections, 0), AT[1] + HEAD_H + 2);
        assert_eq!(row_top(&p, sections, 3) - row_top(&p, sections, 2), ROW_H + GAP, "a gap after the vitals");
        assert_eq!(row_top(&p, sections, 6) - row_top(&p, sections, 5), ROW_H, "none inside a section");
        assert!(row_top(&p, sections, 9) + ROW_H <= p.rect[3], "the last row inside");
    }

    #[test]
    fn bonuses_and_the_command() {
        assert_eq!((bonus(120, 95), bonus(90, 95), bonus(5, 5)), ("+25".into(), "-5".into(), String::new()));
        assert_eq!(command(""), Command::Toggle);
        assert_eq!(command(" ON "), Command::On(true));
        assert_eq!(command("hide"), Command::On(false));
        assert_eq!(command("what"), Command::Help);
    }
}
