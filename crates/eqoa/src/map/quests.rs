//! The quest marks on the map (the user: "add quest markers for givers to the
//! map too", "only if eligible for the class/level"): what a server sends for
//! the player (`net::session::OP_QUEST_MARKERS_DATA`, go-eqoa-server's: none
//! from Sandstorm), a "!" at each giver of a quest the player may take and a
//! "?" where a step of its quests is to be done (an NPC to see, or the
//! creatures a kill or loot step hunts), each with a tooltip of its quests:
//! the name and levels, the start of the offer or the step's journal line,
//! and the rewards or what the step needs. A mouse over a mark shows it, a
//! finger's tap on one, the pad's middle cross on one once the map is panned
//! off the player. Extended's Map page turns them off (`quest_markers`).

use net::session::QuestMark;
use vi::ui::{Raster, Vui, wide};

/// A quest to take's mark: gold, as the game's own gold words are.
pub const TAKE: [f32; 4] = [1.0, 0.82, 0.25, 1.0];
/// A step's: pale, a quest under way.
pub const STEP: [f32; 4] = [0.85, 0.9, 1.0, 1.0];
/// A hunt's: the creatures a step kills, warmer.
pub const HUNT: [f32; 4] = [1.0, 0.6, 0.35, 1.0];
/// A mark's badge under its glyph: dark, as the map's outlines are.
pub const BADGE: [f32; 4] = [0.08, 0.07, 0.05, 0.92];
/// The tooltip's lines: the quest's name, its line, its detail.
const NAME: [f32; 4] = [1.0, 0.82, 0.25, 1.0];
const LINE: [f32; 4] = [1.0, 0.97, 0.85, 1.0];
const DETAIL: [f32; 4] = [0.78, 0.78, 0.74, 1.0];

/// The marks a frame draws and where (window pixels), and the one a finger
/// tapped.
#[derive(Default)]
pub struct QuestMarks {
    pub marks: Vec<QuestMark>,
    /// the drawn marks: window point, index in `marks`
    spots: Vec<([f32; 2], usize)>,
    /// the mark a finger last tapped (its tooltip shown until a tap elsewhere)
    pub tapped: Option<usize>,
    /// hidden on the map by ○ (J, its button) for now; Extended's Quest
    /// Markers is the kept off, which asks the server for none
    pub hidden: bool,
}

impl QuestMarks {
    /// The frame's set (the session's, or none with Quest Markers off).
    pub fn set(&mut self, marks: &[QuestMark]) {
        if self.marks != marks {
            self.marks = marks.to_vec();
            self.tapped = None;
            self.spots.clear();
        }
    }

    /// The marks of world `world` (`gWorldInfo`'s index) at their window
    /// points, by `to_window`, noted for the pointer.
    pub fn place(&mut self, world: i32, to_window: impl Fn([f32; 2]) -> [f32; 2]) {
        self.spots = if self.hidden { Vec::new() } else { self.marks.iter().enumerate().filter(|(_, m)| m.world as i32 == world).map(|(i, m)| (to_window([m.x, m.z]), i)).collect() };
    }

    /// ○: shown or hidden, a tapped tooltip let go.
    pub fn toggle(&mut self) {
        self.hidden = !self.hidden;
        self.tapped = None;
        if self.hidden {
            self.spots.clear();
        }
    }

    /// The drawn marks: window point, index in `marks`.
    pub fn spots(&self) -> &[([f32; 2], usize)] {
        &self.spots
    }

    /// The drawn mark nearest window point `s` within `reach` pixels.
    pub fn at(&self, s: [f32; 2], reach: f32) -> Option<usize> {
        self.spots
            .iter()
            .map(|&(p, i)| ((p[0] - s[0]).hypot(p[1] - s[1]), i))
            .filter(|&(d, _)| d <= reach)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, i)| i)
    }

    /// The window point of mark `i`, as drawn.
    pub fn spot(&self, i: usize) -> Option<[f32; 2]> {
        self.spots.iter().find(|&&(_, j)| j == i).map(|&(p, _)| p)
    }
}

/// A mark's glyph and colour.
pub fn look(kind: u8) -> (&'static str, [f32; 4]) {
    match kind {
        1 => ("!", TAKE),
        3 => ("?", HUNT),
        _ => ("?", STEP),
    }
}

/// The tooltip's lines for mark `m`, each its text and colour, wrapped to
/// `width` frame units by `measure`: the NPC's name; for each quest its name
/// and levels, its line and its detail.
pub fn tooltip_lines(m: &QuestMark, width: i32, measure: impl Fn(&str) -> i32) -> Vec<(String, [f32; 4])> {
    let mut out = vec![(m.npc.clone(), LINE)];
    for q in &m.quests {
        let levels = match q.levels {
            (0, 0) => String::new(),
            (a, b) if a == b => format!(" (level {a})"),
            (a, b) => format!(" (levels {a}-{b})"),
        };
        for l in wrap(&format!("{}{levels}", q.name), width, &measure) {
            out.push((l, NAME));
        }
        for (text, color) in [(&q.line, LINE), (&q.detail, DETAIL)] {
            if text.trim().is_empty() {
                continue;
            }
            for l in wrap(text, width, &measure) {
                out.push((l, color));
            }
        }
    }
    out
}

/// `text` in lines no wider than `width` by `measure`, broken at spaces (a
/// word too long for a line has one of its own).
pub fn wrap(text: &str, width: i32, measure: impl Fn(&str) -> i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let tried = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if !line.is_empty() && measure(&tried) > width {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = tried;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The marks' glyphs (their badges are the map's quads), at frame points.
pub fn draw_glyphs(r: &mut dyn Raster, vui: &Vui, marks: &QuestMarks, f: impl Fn([f32; 2]) -> [f32; 2]) {
    for &(p, i) in &marks.spots {
        let (glyph, color) = look(marks.marks[i].kind);
        let at = f(p);
        vui.draw_centered_string_ex(r, &wide(glyph), at[0] as i32, at[1] as i32, color, 0, true);
    }
}

/// Mark `i`'s tooltip beside frame point `at`, inside `corners` (the window's
/// frame corners), on the map's panel (`panel`).
pub fn draw_tooltip(r: &mut dyn Raster, vui: &Vui, m: &QuestMark, at: [f32; 2], corners: [[f32; 2]; 2], panel: impl Fn(&mut dyn Raster, [i32; 2], i32, i32)) {
    let width = 260;
    let lines = tooltip_lines(m, width, |s| vui.string_width(&*r, &wide(s)));
    let line = r.point_size() + 2;
    let w = lines.iter().map(|(s, _)| vui.string_width(&*r, &wide(s))).max().unwrap_or(0) + 20;
    let h = line * lines.len() as i32 + 14;
    let (tl, br) = (corners[0], corners[1]);
    let mut x = at[0] as i32 + 18;
    if x + w > br[0] as i32 - 4 {
        x = at[0] as i32 - 18 - w;
    }
    let x = x.clamp(tl[0] as i32 + 4, (br[0] as i32 - 4 - w).max(tl[0] as i32 + 4));
    let y = (at[1] as i32 - h / 2).clamp(tl[1] as i32 + 4, (br[1] as i32 - 4 - h).max(tl[1] as i32 + 4));
    panel(r, [x, y], w, h);
    for (k, (s, color)) in lines.iter().enumerate() {
        vui.draw_string_ex(r, &wide(s), x + 10, y + 7 + k as i32 * line, *color, true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::session::MarkQuest;

    fn mark(kind: u8, world: u8, x: f32, z: f32) -> QuestMark {
        QuestMark {
            kind,
            world,
            x,
            z,
            npc: "Malsis".into(),
            quests: vec![MarkQuest { name: "The pincers".into(), levels: (1, 10), line: "You there, will you prove your worth?".into(), detail: "430 XP, Smoldering Aura".into() }],
        }
    }

    /// The marks of the world shown are noted where drawn and found by a
    /// pointer within reach; the controls: another world's mark is not
    /// placed, and a point past reach finds nothing.
    #[test]
    fn a_mark_is_found_where_it_was_drawn() {
        let mut q = QuestMarks::default();
        q.set(&[mark(1, 0, 100.0, 200.0), mark(2, 3, 100.0, 200.0)]);
        q.place(0, |p| [p[0] * 2.0, p[1] * 2.0]);
        assert_eq!(q.spot(0), Some([200.0, 400.0]));
        assert_eq!(q.spot(1), None, "another world's mark");
        assert_eq!(q.at([205.0, 398.0], 10.0), Some(0));
        assert_eq!(q.at([230.0, 400.0], 10.0), None, "past reach");
        q.tapped = Some(0);
        q.set(&[mark(1, 0, 100.0, 200.0), mark(2, 3, 100.0, 200.0)]);
        assert_eq!(q.tapped, Some(0), "the same set keeps the tap");
        q.set(&[mark(1, 0, 101.0, 200.0)]);
        assert_eq!(q.tapped, None, "a new set lets it go");
        // hidden: none placed and none found, then shown again
        q.tapped = Some(0);
        q.toggle();
        q.place(0, |p| p);
        assert!(q.spots().is_empty() && q.at([101.0, 200.0], 10.0).is_none() && q.tapped.is_none());
        q.set(&[mark(1, 0, 102.0, 200.0)]);
        assert!(q.hidden, "a new set stays hidden");
        q.toggle();
        q.place(0, |p| p);
        assert_eq!(q.at([102.0, 200.0], 1.0), Some(0));
    }

    /// A giver's "!" is gold, a step's "?" pale, a hunt's warmer; the
    /// tooltip names the NPC, then the quest with its levels in gold, its
    /// line and its detail, wrapped to the width (each character 6 wide
    /// here); the control: an empty detail adds no line.
    #[test]
    fn the_tooltip_is_the_marks_quests() {
        assert_eq!(look(1), ("!", TAKE));
        assert_eq!(look(2), ("?", STEP));
        assert_eq!(look(3), ("?", HUNT));
        let m = mark(1, 0, 0.0, 0.0);
        let lines = tooltip_lines(&m, 120, |s| s.len() as i32 * 6);
        let text: Vec<&str> = lines.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(text, ["Malsis", "The pincers (levels", "1-10)", "You there, will you", "prove your worth?", "430 XP, Smoldering", "Aura"]);
        assert_eq!(lines[1].1, NAME);
        assert_eq!(lines[3].1, LINE);
        assert_eq!(lines[5].1, DETAIL);
        let mut bare = m.clone();
        bare.quests[0].detail.clear();
        assert_eq!(tooltip_lines(&bare, 120, |s| s.len() as i32 * 6).len(), 5, "no detail, no line");
    }
}
