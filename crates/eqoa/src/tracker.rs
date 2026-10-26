//! The quest tracker (the user: "add quest progress hud item similar to WoW
//! below minimap above chat bar, transparent background. like if quest says
//! obtain 3 bat eyes, have 0/3"): the quests under way a server sends
//! (`net::session::OP_QUEST_PROGRESS_DATA`, go-eqoa-server's: none from
//! Sandstorm), in the HUD's frame under the minimap (`map::mini::FRAME`) and
//! above the chat box at its tallest, out at a wide window's right and up a
//! tall one's top with the quick menus as the minimap is. No backing: the
//! words outlined so they read over the world, each quest's name in gold, its
//! objectives under it ("Bat Eye: 0/3"), one counted whole dimmed; as many
//! quests as fit, whole, then "+N more". A click on it opens the quest
//! journal (`Hud`'s `Pick::Quests`). Extended's Quest Tracker turns it off;
//! it is up where the minimap could be (`Play::tracker_up`), so never under
//! Hide UI, with the map up or a window open.

use crate::ui::UiRaster;
use net::session::TrackedQuest;
use render::ui::{Anchor, Vertical};

/// Its column in the HUD's frame (`Anchor::Right`, `Vertical::Top`): the
/// quick menu's name plate's left (`map::mini::PLATE`) to the minimap's
/// right, from under the minimap to above the chat box with five lines.
pub const COLUMN: [i32; 4] = [430, 168, 586, 336];
/// Its words' size, of the HUD's font, and the objectives' indent.
pub const SCALE: f32 = 0.8;
const INDENT: f32 = 6.0;
/// A quest's name, an objective, one counted whole, "+N more"; the outline.
pub const NAME: [f32; 4] = [1.0, 0.82, 0.25, 1.0];
pub const GOAL: [f32; 4] = [0.96, 0.95, 0.9, 1.0];
pub const DONE: [f32; 4] = [0.6, 0.6, 0.56, 1.0];
pub const MORE: [f32; 4] = [0.85, 0.85, 0.8, 1.0];
const OUTLINE: [f32; 4] = [0.0, 0.0, 0.0, 0.85];

/// A line of the tracker: its words, colour and whether it is indented (an
/// objective).
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    pub color: [f32; 4],
    pub indent: bool,
}

/// An objective's words: "Bat Eye: 0/3", or as they are with nothing to
/// count ("Speak with Kellina").
pub fn goal_text(text: &str, have: u16, need: u16) -> String {
    if need == 0 { text.to_string() } else { format!("{text}: {}/{need}", have.min(need)) }
}

/// `text` no wider than `most` by `width`, cut at a character with "...".
fn fit(text: &str, most: f32, width: &impl Fn(&str) -> f32) -> String {
    if width(text) <= most {
        return text.to_string();
    }
    let mut cut: Vec<char> = text.chars().collect();
    while !cut.is_empty() {
        cut.pop();
        let s: String = cut.iter().collect::<String>().trim_end().to_string() + "...";
        if width(&s) <= most {
            return s;
        }
    }
    String::new()
}

/// The tracker's lines in `rows` rows at most, each no wider than `most`
/// by `width`: the quests whole (a name, its objectives) in their order
/// while they fit, then "+N more" for the rest (a row kept for it).
pub fn lines(quests: &[TrackedQuest], rows: usize, most: f32, width: impl Fn(&str) -> f32) -> Vec<Line> {
    let mut out = Vec::new();
    for (i, q) in quests.iter().enumerate() {
        let need = 1 + q.goals.len();
        let left = quests.len() - i - 1;
        let room = rows.saturating_sub(out.len());
        if need > room || (left > 0 && need + 1 > room) {
            let more = quests.len() - i;
            if out.len() < rows {
                out.push(Line { text: format!("+{more} more"), color: MORE, indent: false });
            }
            return out;
        }
        out.push(Line { text: fit(&q.name, most, &width), color: NAME, indent: false });
        for g in &q.goals {
            let color = if g.done() { DONE } else { GOAL };
            out.push(Line { text: fit(&goal_text(&g.text, g.have, g.need), most - INDENT, &width), color, indent: true });
        }
    }
    out
}

/// Draws the tracker in the HUD's frame; the frame rectangle its lines
/// cover (for the pointer), or None when nothing was drawn.
pub fn draw(ui: &mut UiRaster, quests: &[TrackedQuest]) -> Option<[i32; 4]> {
    let font = ui.assets.font?;
    if quests.is_empty() {
        return None;
    }
    ui.ui.anchor = Anchor::Right;
    ui.ui.vertical = Vertical::Top;
    let h = ui.ui.fonts[font].height as f32 * SCALE;
    let step = h + 1.0;
    let rows = ((COLUMN[3] - COLUMN[1]) as f32 / step).floor() as usize;
    let most = (COLUMN[2] - COLUMN[0]) as f32;
    let shown = {
        let f = &ui.ui.fonts[font];
        lines(quests, rows, most, |s| f.string_width(&vi::ui::wide(s)) as f32 * SCALE)
    };
    let mut y = COLUMN[1] as f32;
    let mut right = COLUMN[0] as f32;
    for l in &shown {
        let x = COLUMN[0] as f32 + if l.indent { INDENT } else { 0.0 };
        let s = vi::ui::wide(&l.text);
        // outlined: the words in black a pixel round, then in their colour
        for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)] {
            ui.ui.text_scaled(font, x + dx, y + dy, SCALE, OUTLINE, &s);
        }
        ui.ui.text_scaled(font, x, y, SCALE, l.color, &s);
        right = right.max(x + ui.ui.fonts[font].string_width(&s) as f32 * SCALE);
        y += step;
    }
    (!shown.is_empty()).then(|| [COLUMN[0], COLUMN[1], right.ceil() as i32, y.ceil() as i32])
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::session::QuestGoal;

    fn quest(name: &str, goals: &[(&str, u16, u16)]) -> TrackedQuest {
        TrackedQuest { name: name.into(), goals: goals.iter().map(|&(t, have, need)| QuestGoal { text: t.into(), have, need }).collect() }
    }

    /// Each quest's name, then its objectives indented, counted ("Bat Eye:
    /// 0/3") or as they are (nothing to count), one counted whole dimmed; the
    /// control: one not yet whole is not.
    #[test]
    fn the_lines_are_the_quests_counted() {
        let qs = [quest("Bat eyes", &[("Bat Eye", 1, 3), ("Give to Kellina", 0, 0)]), quest("Pawns", &[("Orc Pawn", 3, 3)])];
        let got = lines(&qs, 20, 1000.0, |s| s.len() as f32);
        let text: Vec<&str> = got.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(text, ["Bat eyes", "Bat Eye: 1/3", "Give to Kellina", "Pawns", "Orc Pawn: 3/3"]);
        assert_eq!(got.iter().map(|l| l.indent).collect::<Vec<_>>(), [false, true, true, false, true]);
        assert_eq!([got[0].color, got[1].color, got[4].color], [NAME, GOAL, DONE], "the whole one dimmed");
        assert_eq!(goal_text("Bat Eye", 5, 3), "Bat Eye: 3/3", "never more than needed");
    }

    /// Short of room the quests that fit go whole and "+N more" counts the
    /// rest, in its own row; the control: with room for all, no such line. A
    /// line wider than the column is cut with "...".
    #[test]
    fn what_does_not_fit_is_counted() {
        let qs = [quest("One", &[("a", 0, 1)]), quest("Two", &[("b", 0, 1), ("c", 0, 1)]), quest("Three", &[("d", 0, 1)])];
        let text = |rows| lines(&qs, rows, 1000.0, |s: &str| s.len() as f32).into_iter().map(|l| l.text).collect::<Vec<_>>();
        assert_eq!(text(7), ["One", "a: 0/1", "Two", "b: 0/1", "c: 0/1", "Three", "d: 0/1"], "all of them");
        assert_eq!(text(6), ["One", "a: 0/1", "Two", "b: 0/1", "c: 0/1", "+1 more"]);
        assert_eq!(text(4), ["One", "a: 0/1", "+2 more"], "Two is three rows and the count's");
        assert_eq!(text(1), ["+3 more"]);
        let cut = lines(&[quest("A very long quest name indeed", &[])], 5, 12.0, |s: &str| s.len() as f32);
        assert_eq!(cut[0].text, "A very lo...");
    }
}
