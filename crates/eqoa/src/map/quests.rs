//! The quest marks on the map (the user: "add quest markers for givers to the
//! map too", "only if eligible for the class/level", "on the quest icon on the
//! map have hover tooltip with quest info"): what a server sends for the
//! player (`net::session::OP_QUEST_MARKERS_DATA`, go-eqoa-server's), or on a
//! server that sends none (Sandstorm) the client's own guide's (`questguide`,
//! each card's first line "<NPC> (guide)"), a gold "!" at each giver of a
//! quest the player may take and a
//! "?" where a step of its quests is to be done (pale at an NPC to see, warmer
//! where a kill or loot step's creatures are), over the map's other markers
//! (`markers`), each with a card of its quests drawn as theirs are: the name
//! and levels, the start of the offer or the step's journal line, and the
//! rewards or what the step needs. As a marker's, the card shows under the
//! mouse, for a finger that tapped the mark (tapped again, the waypoint goes
//! on it; a click puts it there at once) and under the pad's middle cross once
//! the map is panned. Quest Markers (Extended's Quests page, and the map's own
//! J or R3) hides them, and then the server is asked for none.

use super::view::View;
use net::session::QuestMark;
use render::ui::{Quad, Ui};

/// A quest to take's mark: gold, as the game's own gold words are.
pub const TAKE: [f32; 4] = [1.0, 0.82, 0.25, 1.0];
/// A step's at an NPC: pale, a quest under way.
pub const STEP: [f32; 4] = [0.85, 0.9, 1.0, 1.0];
/// A hunt's: the creatures a step kills or loots, warmer.
pub const HUNT: [f32; 4] = [1.0, 0.6, 0.35, 1.0];
/// A badge's inside under its glyph, and its outline.
const BADGE: [f32; 4] = [0.08, 0.07, 0.05, 0.92];
const OUTLINE: [f32; 4] = [0.0, 0.0, 0.0, 0.85];
/// The card's lines: the NPC, a quest's name, its line, its detail.
const NPC: [f32; 4] = [0.95, 0.93, 0.86, 1.0];
const NAME: [f32; 4] = [1.0, 0.82, 0.25, 1.0];
const LINE: [f32; 4] = [1.0, 0.97, 0.85, 1.0];
const DETAIL: [f32; 4] = [0.72, 0.9, 1.0, 1.0];

/// The marks the server sent last, and the one a finger tapped.
#[derive(Default)]
pub struct QuestMarks {
    pub marks: Vec<QuestMark>,
    /// the mark whose card a finger tapped up (`Map::click`)
    pub pinned: Option<usize>,
}

impl QuestMarks {
    /// The frame's set (the session's, none with Quest Markers off); a new
    /// one lets the tapped card go.
    pub fn set(&mut self, marks: &[QuestMark]) {
        if self.marks != marks {
            self.marks = marks.to_vec();
            self.pinned = None;
        }
    }

    /// The marks of world `world` (`gWorldInfo`'s index), with their indices.
    pub fn of_world(&self, world: i32) -> impl Iterator<Item = (usize, &QuestMark)> {
        self.marks.iter().enumerate().filter(move |(_, m)| m.world as i32 == world)
    }

    /// The mark under window pixel `s` of view `v` in a `size` window, of
    /// world `world`: the nearest within `reach` window pixels of its middle.
    pub fn under(&self, world: i32, v: View, size: (u32, u32), s: [f32; 2], reach: f32) -> Option<usize> {
        self.of_world(world)
            .map(|(i, m)| {
                let p = v.to_window(size, [m.x, m.z]);
                (i, (p[0] - s[0]).hypot(p[1] - s[1]))
            })
            .filter(|&(_, d)| d <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
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

/// Mark `m`'s card: its lines and their colours (wrapped when drawn,
/// `markers::draw_lines_card`), the NPC first, then each quest's name and
/// levels, its line and its detail.
pub fn card(m: &QuestMark) -> Vec<(String, [f32; 4])> {
    let mut out = vec![(m.npc.clone(), NPC)];
    for q in &m.quests {
        let levels = match q.levels {
            (0, 0) => String::new(),
            (a, b) if a == b => format!(" (level {a})"),
            (a, b) => format!(" (levels {a}-{b})"),
        };
        out.push((format!("{}{levels}", q.name), NAME));
        for (text, color) in [(&q.line, LINE), (&q.detail, DETAIL)] {
            if !text.trim().is_empty() {
                out.push((text.clone(), color));
            }
        }
    }
    out
}

/// The marks as the minimap's (`mini::Mark`): a quest to take a gold "!"
/// (`QuestOffer`), a step to do a "?" (`QuestStep`), each with its card for
/// a tooltip.
pub fn mini_marks(marks: &[QuestMark]) -> Vec<super::mini::Mark> {
    use super::mini::{Mark, MarkKind};
    marks.iter().map(|m| Mark { kind: if m.kind == 1 { MarkKind::QuestOffer } else { MarkKind::QuestStep }, world: m.world as i32, at: [m.x, m.z], tip: card(m) }).collect()
}

fn rect(ui: &mut Ui, white: usize, r: [f32; 4], color: [f32; 4]) {
    ui.push(Quad { tex: white, blend: 3, pos: [[r[0], r[1]], [r[2], r[1]], [r[2], r[3]], [r[0], r[3]]], uv: [[0.5, 0.5]; 4], color, clip: None });
}

/// The marks at their frame points (their kinds), `r` frame pixels from a
/// badge's middle to its edge and `o` its ring: dark inside a ring of the
/// mark's colour, its glyph over it in the map's words.
pub fn draw_marks(ui: &mut Ui, assets: &crate::ui::UiAssets, vui: &vi::ui::Vui, white: usize, shown: &[(u8, [f32; 2])], r: f32, o: f32) {
    for &(kind, p) in shown {
        for (grow, color) in [(o * 2.0, OUTLINE), (o, look(kind).1), (0.0, BADGE)] {
            rect(ui, white, [p[0] - r - grow, p[1] - r - grow, p[0] + r + grow, p[1] + r + grow], color);
        }
    }
    let mut raster = assets.raster(ui);
    for &(kind, p) in shown {
        let (glyph, color) = look(kind);
        vui.draw_centered_string_ex(&mut raster, &vi::ui::wide(glyph), p[0].round() as i32, p[1].round() as i32, color, 0, true);
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

    /// The mark under a pointer is the nearest of the world shown within
    /// reach; the controls: another world's mark at the same place is never
    /// under it, and past reach none is. A new set lets a tapped card go, the
    /// same set keeps it.
    #[test]
    fn the_mark_under_a_pointer() {
        let v = View { center: [100.0, 200.0], scale: 1.0 };
        let size = (1280, 720);
        let set = [mark(2, 3, 100.0, 200.0), mark(1, 0, 100.0, 200.0), mark(1, 0, 140.0, 200.0)];
        let mut q = QuestMarks::default();
        q.set(&set);
        let at = v.to_window(size, [100.0, 200.0]);
        assert_eq!(q.under(0, v, size, [at[0] + 3.0, at[1]], 10.0), Some(1), "world 0's, the nearer");
        assert_eq!(q.under(3, v, size, at, 10.0), Some(0), "world 3's at the same place");
        assert_eq!(q.under(0, v, size, [at[0] + 25.0, at[1]], 10.0), None, "past reach");
        q.pinned = Some(1);
        q.set(&set);
        assert_eq!(q.pinned, Some(1), "the same set keeps the tap");
        q.set(&[mark(1, 0, 101.0, 200.0)]);
        assert_eq!(q.pinned, None, "a new set lets it go");
    }

    /// On the minimap a giver's mark is a quest to take, a step's and a
    /// hunt's a step to do, each at its place in its world with its card;
    /// the control: no marks, none there.
    #[test]
    fn the_minimap_has_the_marks() {
        use super::super::mini::MarkKind;
        let got = mini_marks(&[mark(1, 0, 10.0, 20.0), mark(2, 3, 30.0, 40.0), mark(3, 0, 50.0, 60.0)]);
        let kinds: Vec<(MarkKind, i32, [f32; 2])> = got.iter().map(|m| (m.kind, m.world, m.at)).collect();
        assert_eq!(kinds, [(MarkKind::QuestOffer, 0, [10.0, 20.0]), (MarkKind::QuestStep, 3, [30.0, 40.0]), (MarkKind::QuestStep, 0, [50.0, 60.0])]);
        assert_eq!(got[0].tip, card(&mark(1, 0, 10.0, 20.0)), "its card for a tooltip");
        assert!(mini_marks(&[]).is_empty());
    }

    /// A giver's "!" is gold, a step's "?" pale, a hunt's warmer; the card
    /// names the NPC, then each quest with its levels, its line and its
    /// detail; the control: an empty detail adds no line.
    #[test]
    fn the_card_is_the_marks_quests() {
        assert_eq!(look(1), ("!", TAKE));
        assert_eq!(look(2), ("?", STEP));
        assert_eq!(look(3), ("?", HUNT));
        let mut m = mark(1, 0, 0.0, 0.0);
        m.quests.push(MarkQuest { name: "Kellina's ring".into(), levels: (4, 4), line: "Find my ring.".into(), detail: String::new() });
        let lines = card(&m);
        let text: Vec<&str> = lines.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(text, ["Malsis", "The pincers (levels 1-10)", "You there, will you prove your worth?", "430 XP, Smoldering Aura", "Kellina's ring (level 4)", "Find my ring."]);
        assert_eq!([lines[0].1, lines[1].1, lines[2].1, lines[3].1], [NPC, NAME, LINE, DETAIL]);
    }
}
