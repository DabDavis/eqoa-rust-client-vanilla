//! Which quests the quest tracker shows (Heirloom petition 3013, Cucumber:
//! "The ability to choose which quests in your quest log you track"). Every
//! quest is tracked until the player untracks it in the quest journal (□ on
//! the selected quest, `questlog`); an untracked one drops off the tracker
//! (`tracker`) and reads grey in the journal. The set is the character's own,
//! kept as the auto slots are (`autocast`): one file for every window, a
//! line a character (`<server id>:<name>` then the untracked quests' names,
//! tab-separated), read again before each write.
//!
//! A quest is known by its name as the tracker has it (the server's
//! `OP_QUEST_PROGRESS_DATA`: the journal's quests in its order, so the
//! journal's quest `i` is the tracker's `i`): the journal's own line changes
//! with each step, its name does not. A quest that leaves the journal
//! (0x007D: done, or dropped) leaves the set, so taking it again tracks it.

use std::collections::BTreeSet;

/// The file the sets are kept in.
pub const FILE: &str = "questtrack.opt";

/// The character's untracked quests.
#[derive(Default, Debug)]
pub struct Untracked {
    who: Option<(i64, String)>,
    off: BTreeSet<String>,
}

/// A name as a line keeps it: no tab or line break in it.
fn clean(name: &str) -> String {
    name.chars().map(|c| if matches!(c, '\t' | '\n' | '\r') { ' ' } else { c }).collect::<String>().trim().to_string()
}

/// The character's untracked quests out of the file's `text`.
pub fn kept_of(text: &str, server_id: i64, name: &str) -> BTreeSet<String> {
    let me = format!("{server_id}:{name}");
    let mut out = BTreeSet::new();
    for line in text.lines() {
        let mut parts = line.split('\t');
        if parts.next() == Some(me.as_str()) {
            out.extend(parts.filter(|p| !p.trim().is_empty()).map(|p| p.to_string()));
        }
    }
    out
}

/// The file with this character's line made `off` (dropped when empty), the
/// others' as they are.
pub fn with_kept(text: &str, server_id: i64, name: &str, off: &BTreeSet<String>) -> String {
    let me = format!("{server_id}:{name}");
    let mut out: String = text.lines().filter(|l| l.split('\t').next() != Some(me.as_str()) && !l.trim().is_empty()).map(|l| format!("{l}\n")).collect();
    if !off.is_empty() {
        out.push_str(&me);
        for q in off {
            out.push('\t');
            out.push_str(q);
        }
        out.push('\n');
    }
    out
}

fn read_file() -> String {
    crate::settings::store_read(FILE).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
}

impl Untracked {
    /// The character played: its set read when it is another.
    pub fn character(&mut self, server_id: i64, name: &str) {
        if self.who.as_ref().is_some_and(|(i, n)| *i == server_id && n == name) {
            return;
        }
        *self = Untracked { who: Some((server_id, name.to_string())), off: kept_of(&read_file(), server_id, name) };
    }

    fn keep(&self) {
        if let Some((id, name)) = &self.who {
            let text = with_kept(&read_file(), *id, name, &self.off);
            if let Err(e) = crate::settings::store_write(FILE, text.as_bytes()) {
                crate::say(&format!("the tracked quests were not kept: {e}"));
            }
        }
    }

    /// Whether the quest named `quest` is on the tracker.
    pub fn tracked(&self, quest: &str) -> bool {
        !self.off.contains(&clean(quest))
    }

    /// The quest named `quest` tracked or not, the other way; kept. Whether it
    /// is tracked now.
    pub fn toggle(&mut self, quest: &str) -> bool {
        let q = clean(quest);
        if q.is_empty() {
            return true;
        }
        let now = !self.off.remove(&q);
        if now {
            self.off.insert(q);
        }
        self.keep();
        !now
    }

    /// The quest named `quest` left the journal: out of the set (kept), so
    /// taken again it is tracked.
    pub fn gone(&mut self, quest: &str) {
        if self.off.remove(&clean(quest)) {
            self.keep();
        }
    }

    /// The tracker's quests less the untracked ones.
    pub fn filter(&self, quests: Vec<net::session::TrackedQuest>) -> Vec<net::session::TrackedQuest> {
        if self.off.is_empty() {
            return quests;
        }
        quests.into_iter().filter(|q| self.tracked(&q.name)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::session::TrackedQuest;

    fn q(name: &str) -> TrackedQuest {
        TrackedQuest { name: name.into(), goals: Vec::new(), ability: false }
    }

    /// A character's line written and read back, another's left as it was;
    /// an empty set drops the line. The control: another character reads
    /// none of it.
    #[test]
    fn the_untracked_quests_round_trip() {
        let other = "7:Bo\tRat Tails\n";
        let off: BTreeSet<String> = ["Bat Eyes".to_string(), "The Orc Pawn".to_string()].into();
        let text = with_kept(other, 12, "Cucumber", &off);
        assert_eq!(text, "7:Bo\tRat Tails\n12:Cucumber\tBat Eyes\tThe Orc Pawn\n");
        assert_eq!(kept_of(&text, 12, "Cucumber"), off, "round trip");
        assert_eq!(kept_of(&text, 7, "Bo"), ["Rat Tails".to_string()].into(), "the other's kept");
        assert!(kept_of(&text, 12, "Bo").is_empty(), "the control: another character");
        assert_eq!(with_kept(&text, 12, "Cucumber", &BTreeSet::new()), other, "none untracked: no line");
    }

    /// All tracked by default; untracked ones off the tracker, tracked again
    /// back; a quest gone from the journal leaves the set.
    #[test]
    fn the_tracker_shows_the_tracked_ones() {
        crate::settings::testing::reset(false);
        let mut u = Untracked::default();
        u.character(12, "Cucumber");
        let all = vec![q("Bat Eyes"), q("The Orc Pawn"), q("Speak with Kellina")];
        assert_eq!(u.filter(all.clone()), all, "every quest tracked at first");
        assert!(!u.toggle("The Orc Pawn"), "untracked");
        let names = |v: Vec<TrackedQuest>| v.into_iter().map(|q| q.name).collect::<Vec<_>>();
        assert_eq!(names(u.filter(all.clone())), ["Bat Eyes", "Speak with Kellina"]);
        // kept: the character read again (another first) has it
        u.character(7, "Bo");
        assert_eq!(u.filter(all.clone()), all, "another character's");
        u.character(12, "Cucumber");
        assert!(!u.tracked("The Orc Pawn"), "kept for the character");
        assert!(u.toggle("The Orc Pawn"), "tracked again");
        assert_eq!(u.filter(all.clone()), all);
        u.toggle("Bat Eyes");
        u.gone("Bat Eyes");
        assert!(u.tracked("Bat Eyes"), "done or dropped: out of the set");
        u.character(7, "Bo");
        u.character(12, "Cucumber");
        assert!(u.tracked("Bat Eyes"), "and kept so");
    }
}
