//! The quest guide (the user: "for quest tracker and map/minimap icons,
//! etc. let it be built into client also, so sandstorm can benefit without
//! server additions, though may not be as accurate"): on a server that offers
//! no quest marks or tracker of its own (go-eqoa-server's 0x7E03 and 0x7E05,
//! `net::session::OP_QUEST_MARKERS`; Sandstorm offers neither), the client
//! makes the same ones (`map::quests` on the map and the minimap, `tracker`
//! under it) out of what it knows: the journal's lines (CLIENT's eight,
//! `questlog::Quests`), the player's class, race, level, place and items, and
//! a guide compiled in (`data`, written by `tools/gen_quest_guide.py`: the 746
//! class quests of levels 1-20 of EQOA-Data's city guides as go-eqoa-server's
//! quest files hold them, and where their people stand on go-eqoa-server).
//! Every card and tracked quest it makes says "(guide)"; a server that offers
//! its own is believed alone, and the guide stays off (`Play::guide_on`).
//!
//! - A journal line is a step of a quest when it names that step's people or
//!   things (`match_line`). Sandstorm words its journal its own way ("Pick up
//!   Militia bracers from Merchant Galosh.", "Return to Nothard.", "Bring
//!   Norgram 2 slashed orc pawn belts ."), so the line's words are held to
//!   the step's NPC or creature (by its own name, not a title: Nothard for
//!   Commander Nothard; a long word one letter off still counts), its items
//!   (plurals as their singulars) and what it hands over, among the quests of
//!   the player's class, race and level; the player's home city, the quest
//!   of a line that just left (Sandstorm puts a quest's next step's line in
//!   as the last goes) and a chain's next quest weigh in on a tie. Within
//!   the quest, a loot step whose drops are all held, or a step whose
//!   handed-over item is, is behind the player: the step after it is the one
//!   to do. A line naming nothing the guide knows well enough stays text.
//! - The marks (`Estimate::marks`, as the server's: `map::quests`): a "?" at
//!   each matched step's NPC (the two nearest of its spawns), at a kill or
//!   loot step's creature and at a reach step's place; a "!" at the givers of
//!   the quests the player may take: its class and race, its level within
//!   the quest's, every quest it requires finished, neither in the journal
//!   nor finished itself, and of the player's home city unless a chain begun
//!   elsewhere (every city's first quests would be a "!" in every town, and a
//!   Sandstorm character's history is not known). At most 40, the steps'
//!   first, then the player's world, nearest first.
//! - The tracker (`Estimate::tracker`): each line's quest and its step's
//!   objectives: a show, give or loot step's items held of those needed (the
//!   inventory's, worn ones too: "Militia Bracer: 1/1", "Show to Commander
//!   Nothard"), a kill step's kills the client saw by the player's hand
//!   ("(seen)": the server's own count is not sent), else the game's line.
//! - What it keeps of a character (`Kept`, `settings::quest_guide`): the
//!   quests it finished (the game's "You have finished a quest!" as a matched
//!   line has left the journal and no line of its quest is in it; and a
//!   matched quest's chain before it), its home city once a line named one of
//!   that city's quests, and the kills seen.

mod data;
#[cfg(test)]
mod tests;

use net::dump::NetItem;
use net::session::{MarkQuest, QuestGoal, QuestMark, TrackedQuest};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;

pub use data::{CITIES, HOMES, NPCS, QUESTS};

/// What the guide's own cards and quests say after their names.
pub const GUIDE: &str = crate::tracker::GUIDE;
/// What the game says as a quest's last step is done (0x0046, Sandstorm's
/// and go-eqoa-server's alike).
pub const FINISHED: &str = "You have finished a quest!";

/// The marks at most, a mark's quests and a name's spawns marked, as the
/// server's (go-eqoa-server's questmarks.go), and the texts' bytes.
const MAX_MARKS: usize = 40;
const MAX_MARK_QUESTS: usize = 4;
const MAX_SPOTS: usize = 2;
const MAX_TEXT: usize = 140;
/// How long a line gone from the journal is still its quest's (the next
/// step's line comes in with the last one's going), on the session's clock.
const GONE_MS: u32 = 10_000;
/// How long a creature the player (or its pet) hit is the player's kill
/// when it dies.
const HIT_MS: u32 = 60_000;
/// How often the estimate is made again (the nearest spawns move with the
/// player; the items held change).
const EVERY_MS: u32 = 500;
/// A word in this many NPCs' names or fewer (or items') names one well
/// enough alone.
const RARE: u32 = 3;
/// The least a step's names must score to be matched, and the score that
/// needs no rare word.
const LEAST: f32 = 2.0;
const SURE: f32 = 4.0;

/// An NPC or a creature the quests name, and its spawns: world
/// (`gWorldInfo`'s index), x, z.
pub struct Npc {
    pub name: &'static str,
    pub places: &'static [(u8, f32, f32)],
}

/// What a step asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Talk,
    Show,
    Give,
    Kill,
    Loot,
    Reach,
}

/// One step of a quest.
pub struct Step {
    pub kind: Kind,
    /// its NPC or creature (`NPCS`), `NONE` for a reach step
    pub target: u16,
    /// the items it needs and how many (a show or give step's; a loot
    /// step's to take)
    pub items: &'static [(&'static str, u16)],
    /// the items it hands over once done
    pub gets: &'static [&'static str],
    /// a kill step's kills
    pub count: u16,
    /// a give step's tunar
    pub tunar: u32,
    /// a reach step's place: world, x, z
    pub at: Option<(u8, f32, f32)>,
    /// the guide's journal line for it
    pub journal: &'static str,
}

/// One quest.
pub struct Quest {
    pub key: &'static str,
    pub name: &'static str,
    /// its city (`CITIES`)
    pub city: u8,
    /// a bit a class that may take it (WAR 0 .. ALC 14), none: any
    pub classes: u16,
    /// a bit a race, none: any
    pub races: u16,
    pub levels: (u8, u8),
    /// the quests (`QUESTS`) to have finished first
    pub requires: &'static [u16],
    /// who offers it (`NPCS`)
    pub giver: u16,
    /// the first line of the giver's offer, and the rewards in all
    pub offer: &'static str,
    pub rewards: &'static str,
    pub steps: &'static [Step],
}

/// A race and class's home cities (`CITIES`), where the game starts it.
pub struct Home {
    pub race: u8,
    pub class: u8,
    pub cities: &'static [u8],
}

/// A step with no NPC or creature.
pub const NONE: u16 = u16::MAX;

/// A step, the table's shorthand.
#[allow(clippy::too_many_arguments)]
const fn st(kind: Kind, target: u16, items: &'static [(&'static str, u16)], gets: &'static [&'static str], count: u16, tunar: u32, at: Option<(u8, f32, f32)>, journal: &'static str) -> Step {
    Step { kind, target, items, gets, count, tunar, at, journal }
}

// ---- words ---------------------------------------------------------------------------

/// The words of `text` as the guide compares them: lower case, of letters and
/// digits (a backtick or apostrophe inside a word kept, as '), a possessive's
/// "'s" dropped, each stemmed so a plural meets its singular (`stem`).
pub fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars().chain(std::iter::once(' ')) {
        let c = if matches!(c, '`' | '\u{2018}' | '\u{2019}') { '\'' } else { c };
        if c.is_alphanumeric() || (c == '\'' && !cur.is_empty()) {
            cur.extend(c.to_lowercase());
        } else if !cur.is_empty() {
            let mut w = std::mem::take(&mut cur);
            if w.ends_with("'s") {
                w.truncate(w.len() - 2);
            }
            while w.ends_with('\'') {
                w.pop();
            }
            if !w.is_empty() {
                out.push(stem(&w));
            }
        }
    }
    out
}

/// A word without its plural's ending and a last "e", so "belts" and "belt",
/// "flies" and "fly", "wolves" and "wolf", "boxes" and "box", "horses" and
/// "horse" meet (a key to compare by, not a word to show).
fn stem(w: &str) -> String {
    let n = w.len();
    let mut s = if n > 4 && w.ends_with("ies") {
        format!("{}y", &w[..n - 3])
    } else if n > 4 && w.ends_with("lves") {
        format!("{}lf", &w[..n - 4])
    } else if n > 3 && w.ends_with("es") {
        w[..n - 2].to_string()
    } else if n > 3 && w.ends_with('s') && !w.ends_with("ss") && !w.ends_with("us") && !w.ends_with("is") {
        w[..n - 1].to_string()
    } else {
        w.to_string()
    };
    if s.len() > 3 && s.ends_with('e') {
        s.pop();
    }
    s
}

/// Two words alike: the same, or both of six letters or more, the same
/// first letter and one letter apart (Sandstorm's "Norgram" for Norgam).
fn same(a: &str, b: &str) -> bool {
    a == b || (a.len() >= 6 && b.len() >= 6 && a.as_bytes()[0] == b.as_bytes()[0] && one_apart(a, b))
}

/// `a` and `b` one edit apart (a letter changed, put in or taken out).
fn one_apart(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (s, l) = if a.len() <= b.len() { (&a, &b) } else { (&b, &a) };
    if l.len() - s.len() > 1 {
        return false;
    }
    let p = s.iter().zip(l.iter()).take_while(|(x, y)| x == y).count();
    if s.len() == l.len() { s[p + 1..] == l[p + 1..] } else { s[p..] == l[p + 1..] }
}

/// Words that say what a person is, not who (a name is known by its other
/// words: Nothard, not Commander), with every word beginning two names or
/// more (`build`: Captain, Guard, Orc).
const TITLES: &[&str] = &[
    "acolyte", "alchemist", "apprentice", "archivist", "armorer", "baron", "bartender", "banker", "blacksmith", "brother", "captain", "chief", "cleric", "clerk", "coachman", "commander",
    "constable", "corporal", "deputy", "dragoon", "druid", "elder", "enchanter", "father", "fisherman", "fletcher", "foreman", "gatekeeper", "general", "grand", "guard", "guardsman",
    "guildmaster", "harbormaster", "high", "initiate", "innkeeper", "jeweler", "keeper", "king", "knight", "lady", "lieutenant", "lord", "mage", "magistrate", "magus", "marshal",
    "master", "mayor", "merchant", "mistress", "monk", "mother", "necrologist", "necromancer", "officer", "overseer", "paladin", "priest", "priestess", "prince", "princess", "queen",
    "ranger", "rogue", "sage", "scholar", "scribe", "sentry", "sergeant", "shaman", "sheriff", "sir", "sister", "smith", "spiritmaster", "tailor", "templar", "trainer", "warden",
    "warrior", "watchman", "waysender", "weaponsmith", "wizard",
];
/// Words a name leaves out.
const NAME_STOP: &[&str] = &["a", "an", "the", "of"];
/// Words of a journal line that name nothing (a verb, a number).
const STOP: &[&str] = &[
    "a", "an", "the", "of", "to", "and", "in", "on", "at", "by", "for", "from", "with", "into", "onto", "up", "out", "back", "me", "my", "your", "you", "him", "her", "his", "them",
    "their", "it", "its", "this", "that", "these", "those", "some", "is", "are", "be", "who", "what", "where", "has", "have", "i", "he", "she", "we", "they", "go", "get", "see", "ask",
    "tell", "speak", "talk", "return", "bring", "take", "give", "show", "find", "kill", "slay", "collect", "gather", "deliver", "pick", "meet", "one", "two", "three", "four",
    "five", "six", "seven", "eight", "nine", "ten",
];

/// `list`'s words stemmed (`stem`), as a line's are compared.
fn stemmed(list: &[&str]) -> std::collections::HashSet<String> {
    list.iter().map(|w| stem(w)).collect()
}

/// Whether a (stemmed) word names nothing (`STOP`).
fn stop(w: &str) -> bool {
    static SET: OnceLock<std::collections::HashSet<String>> = OnceLock::new();
    SET.get_or_init(|| stemmed(STOP)).contains(w)
}

/// A name as the guide knows it: the words that say who (all of them, or one
/// no other name has, must be in a line) and its titles (a little more).
struct Name {
    key: Vec<String>,
    titles: Vec<String>,
}

/// The guide's words, made once: each NPC's name, each item's, each step's
/// journal line, and how many NPCs' names and items' each word is in (Nothard
/// names three people: a "Nothard" says one of them as well as any).
struct Index {
    npcs: Vec<Name>,
    items: HashMap<&'static str, Vec<String>>,
    npc_df: HashMap<String, u32>,
    item_df: HashMap<String, u32>,
    /// each quest's steps' journal lines: every word, and those that name
    journals: Vec<Vec<(Vec<String>, Vec<String>)>>,
}

fn index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(build)
}

fn build() -> Index {
    let (name_stop, titles) = (stemmed(NAME_STOP), stemmed(TITLES));
    let toks: Vec<Vec<String>> = NPCS.iter().map(|n| words(n.name).into_iter().filter(|w| !name_stop.contains(w)).collect()).collect();
    // a word that begins two names of two words or more is a title
    let mut first: HashMap<&str, u32> = HashMap::new();
    for t in toks.iter().filter(|t| t.len() > 1) {
        *first.entry(t[0].as_str()).or_default() += 1;
    }
    let title = |w: &str| titles.contains(w) || first.get(w).is_some_and(|&n| n >= 2);
    let npcs: Vec<Name> = toks
        .iter()
        .map(|t| {
            let (mut key, mut titles): (Vec<String>, Vec<String>) = t.iter().cloned().partition(|w| !title(w));
            if key.is_empty()
                && let Some(last) = titles.pop()
            {
                key.push(last);
            }
            Name { key, titles }
        })
        .collect();
    let mut items = HashMap::new();
    for s in QUESTS.iter().flat_map(|q| q.steps) {
        for n in s.items.iter().map(|&(n, _)| n).chain(s.gets.iter().copied()) {
            items.entry(n).or_insert_with(|| content(&words(n)));
        }
    }
    let count = |keys: &mut dyn Iterator<Item = &Vec<String>>| {
        let mut df: HashMap<String, u32> = HashMap::new();
        for key in keys {
            let mut seen: Vec<&String> = key.iter().collect();
            seen.sort();
            seen.dedup();
            for w in seen {
                *df.entry(w.clone()).or_default() += 1;
            }
        }
        df
    };
    let npc_df = count(&mut npcs.iter().map(|n| &n.key));
    let item_df = count(&mut items.values());
    let journals = QUESTS
        .iter()
        .map(|q| {
            q.steps
                .iter()
                .map(|s| {
                    let w = words(s.journal);
                    let c = content(&w);
                    (w, c)
                })
                .collect()
        })
        .collect();
    Index { npcs, items, npc_df, item_df, journals }
}

/// The words that name something.
fn content(w: &[String]) -> Vec<String> {
    w.iter().filter(|w| !stop(w)).cloned().collect()
}

impl Index {
    /// How many NPCs' names, or items', word `w` is in.
    fn in_npcs(&self, w: &str) -> u32 {
        self.npc_df.get(w).copied().unwrap_or(0)
    }

    fn in_items(&self, w: &str) -> u32 {
        self.item_df.get(w).copied().unwrap_or(0)
    }

    /// How well `line` names NPC `n`, and by a rare word or not: all of its
    /// key words there (or one no other name has), 2 each, and half a point
    /// for each title.
    fn name_score(&self, n: &Name, line: &[String]) -> (f32, bool) {
        let has = |w: &String| line.iter().any(|l| same(w, l));
        let hit: Vec<&String> = n.key.iter().filter(|w| has(w)).collect();
        let all = !hit.is_empty() && hit.len() == n.key.len();
        let own = hit.iter().any(|w| self.in_npcs(w) <= 1);
        if !all && !own {
            return (0.0, false);
        }
        let titles = n.titles.iter().filter(|w| has(w)).count();
        (2.0 * hit.len() as f32 + 0.5 * titles as f32, hit.iter().any(|w| self.in_npcs(w) <= RARE))
    }

    /// How well `line` names an item (its words `key`): all of them, 2 each;
    /// two or more with its last (the thing it is), 1 each.
    fn item_score(&self, key: &[String], line: &[String]) -> (f32, bool) {
        let has = |w: &String| line.iter().any(|l| same(w, l));
        let hit = key.iter().filter(|w| has(w)).count();
        let head = key.last().is_some_and(has);
        let v = if hit > 0 && hit == key.len() {
            2.0 * hit as f32
        } else if hit >= 2 && head {
            hit as f32
        } else {
            0.0
        };
        (v, v > 0.0 && (hit >= 2 || key.iter().any(|w| has(w) && self.in_items(w) <= RARE)))
    }

    /// How well a line (its words, and those that name) names step `si` of
    /// quest `qi`: its NPC, its items and what it hands over, its own
    /// journal's words a little; the same line as the guide's journal far
    /// more. Whether it counts (`LEAST` with a rare word, or `SURE`).
    fn step_score(&self, qi: usize, si: usize, all: &[String], line: &[String]) -> f32 {
        let s = &QUESTS[qi].steps[si];
        let (mut v, mut rare) = (0.0, false);
        if let Some(n) = self.npcs.get(s.target as usize) {
            let (a, b) = self.name_score(n, line);
            v += a;
            // a creature's name is a kind ("farmer", "Large Rat"): enough
            // alone only when all of it is there, two words or more
            rare |= if matches!(s.kind, Kind::Kill | Kind::Loot) { a > 0.0 && n.key.len() + n.titles.len() >= 2 && n.titles.iter().all(|w| line.iter().any(|l| same(w, l))) } else { b };
        }
        for n in s.items.iter().map(|&(n, _)| n).chain(s.gets.iter().copied()) {
            let (a, b) = self.item_score(&self.items[n], line);
            v += a;
            rare |= b;
        }
        let (jall, jcontent) = &self.journals[qi][si];
        if !jall.is_empty() && jall == all {
            return 20.0 + v;
        }
        if v >= SURE || (v >= LEAST && rare) {
            v + (jcontent.iter().filter(|w| line.contains(w)).count() as f32 * 0.25).min(1.0)
        } else {
            0.0
        }
    }
}

// ---- the match -----------------------------------------------------------------------

/// Who the journal is matched for.
pub struct Who<'a> {
    pub class: i64,
    pub race: i64,
    pub level: i64,
    /// the player's home cities (`CITIES`), the likeliest first
    pub cities: &'a [u8],
    pub finished: &'a BTreeSet<String>,
    /// quests whose line just left the journal
    pub after: &'a [usize],
}

/// A line matched: its quest (`QUESTS`), the step it names, the step to do
/// (a later one when that one is behind the player: `behind`) and how well.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Match {
    pub quest: usize,
    pub named: usize,
    pub step: usize,
    pub score: f32,
}

fn bit(mask: u16, i: i64) -> bool {
    mask == 0 || ((0..16).contains(&i) && mask & (1 << i) != 0)
}

/// Whether the player may have quest `q` at all: its class and race, a
/// level it can be taken at.
fn may_have(q: &Quest, who: &Who) -> bool {
    bit(q.classes, who.class) && bit(q.races, who.race) && who.level >= q.levels.0 as i64
}

/// Every quest quest `qi` needs finished first, all the way back.
fn chain(qi: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut todo: Vec<usize> = QUESTS[qi].requires.iter().map(|&r| r as usize).collect();
    while let Some(r) = todo.pop() {
        if !out.contains(&r) {
            out.push(r);
            todo.extend(QUESTS[r].requires.iter().map(|&r| r as usize));
        }
    }
    out
}

/// A line's words, and those that name.
struct Line {
    all: Vec<String>,
    named: Vec<String>,
}

impl Line {
    fn of(text: &str) -> Line {
        let all = words(text);
        let named = content(&all);
        Line { all, named }
    }
}

/// The step of quest `qi` that `line` names best and the one to do, by how
/// well each step scored; None when none counted.
fn steps_of(qi: usize, line: &Line, held: &dyn Fn(&str) -> u32) -> Option<(usize, usize, f32)> {
    let ix = index();
    let scores: Vec<f32> = (0..QUESTS[qi].steps.len()).map(|si| ix.step_score(qi, si, &line.all, &line.named)).collect();
    let top = scores.iter().copied().fold(0.0f32, f32::max);
    if top <= 0.0 {
        return None;
    }
    let q = &QUESTS[qi];
    // of the steps named as well, a hand-in whose items are all held (the
    // player is at it), else the first not behind the player
    let tied: Vec<usize> = (0..scores.len()).filter(|&k| scores[k] >= top - 1e-3).collect();
    let ready = |s: &Step| matches!(s.kind, Kind::Show | Kind::Give) && !s.items.is_empty() && s.items.iter().all(|&(n, c)| held(n) >= c as u32);
    let first = tied.iter().copied().find(|&k| ready(&q.steps[k])).or_else(|| tied.iter().copied().find(|&k| !behind(&q.steps[k], held))).unwrap_or(tied[tied.len() - 1]);
    let mut step = first;
    while step + 1 < q.steps.len() && behind(&q.steps[step], held) {
        step += 1;
    }
    Some((first, step, top))
}

/// A step behind the player: a loot step's drops all held, or what a step
/// hands over held (the militia bracer from Merchant Galosh).
fn behind(s: &Step, held: &dyn Fn(&str) -> u32) -> bool {
    (s.kind == Kind::Loot && !s.items.is_empty() && s.items.iter().all(|&(n, c)| held(n) >= c as u32)) || (!s.gets.is_empty() && s.gets.iter().all(|n| held(n) >= 1))
}

/// The quest and step journal line `line` is, for `who`, with `held` the
/// items the player holds: the best named of the quests it may have, a tie
/// to its home city's, a quest whose line just left, a chain's next quest
/// after one it finished, the later quest (nearer its level); a finished
/// one only when nothing else is named.
pub fn match_line(line: &str, who: &Who, held: &dyn Fn(&str) -> u32) -> Option<Match> {
    let line = Line::of(line);
    let mut best: Option<(f32, Match)> = None;
    for (qi, q) in QUESTS.iter().enumerate() {
        if !may_have(q, who) {
            continue;
        }
        let Some((named, step, score)) = steps_of(qi, &line, held) else { continue };
        let mut rank = score;
        if who.cities.contains(&q.city) {
            rank += 1.0;
        }
        if who.after.contains(&qi) {
            rank += 4.0;
        }
        // a chain's next quest by what it finished; with nothing of the chain
        // known, the quest nearest the player's level
        if who.finished.contains(q.key) {
            rank -= 6.0;
        } else if !q.requires.is_empty() && q.requires.iter().all(|&r| who.finished.contains(QUESTS[r as usize].key)) {
            rank += 0.5;
        }
        rank += q.levels.0 as f32 * 0.01;
        if best.as_ref().is_none_or(|(b, _)| rank > *b) {
            best = Some((rank, Match { quest: qi, named, step, score }));
        }
    }
    best.map(|(_, m)| m)
}

/// The player's home cities (`CITIES`), the likeliest first: the one kept,
/// else where the game starts its race and class (a human's two, the nearer
/// to `at` first), else the city whose chain for its class begins nearest.
pub fn homes(kept: Option<u8>, race: i64, class: i64, world: i32, at: [f32; 2]) -> Vec<u8> {
    if let Some(c) = kept {
        return vec![c];
    }
    // how far the city's first quest for the class is given from `at`
    let far = |city: u8| {
        QUESTS
            .iter()
            .filter(|q| q.city == city && q.requires.is_empty() && bit(q.classes, class))
            .flat_map(|q| NPCS[q.giver as usize].places)
            .filter(|p| p.0 as i32 == world)
            .map(|p| (p.1 - at[0]).hypot(p.2 - at[1]))
            .fold(f32::INFINITY, f32::min)
    };
    let mut out: Vec<u8> = HOMES.iter().find(|h| h.race as i64 == race && h.class as i64 == class).map(|h| h.cities.to_vec()).unwrap_or_default();
    if out.is_empty() {
        out = (0..CITIES.len() as u8).filter(|&c| far(c).is_finite()).collect();
        out.sort_by(|&a, &b| far(a).total_cmp(&far(b)));
        out.truncate(1);
    }
    out.sort_by(|&a, &b| far(a).total_cmp(&far(b)));
    out
}

// ---- items ---------------------------------------------------------------------------

/// How many of item `name` the player holds (worn ones too, as the server
/// counts them): by its name in any case, else every word of the guide's in
/// the item's (Sandstorm's names may say more: "orc" in a belt's).
pub fn held(items: &[NetItem], name: &str) -> u32 {
    let n = |it: &NetItem| it.quantity.max(1) as u32;
    let exact: u32 = items.iter().filter(|it| it.item.name.trim().eq_ignore_ascii_case(name)).map(n).sum();
    if exact > 0 {
        return exact;
    }
    let key = content(&words(name));
    if key.is_empty() {
        return 0;
    }
    items
        .iter()
        .filter(|it| {
            let w = words(&it.item.name);
            key.iter().all(|k| w.iter().any(|x| same(k, x)))
        })
        .map(n)
        .sum()
}

/// Item `name` as the player's inventory spells it, when it holds one.
fn shown(items: &[NetItem], name: &str) -> String {
    let key = content(&words(name));
    items
        .iter()
        .find(|it| it.item.name.trim().eq_ignore_ascii_case(name) || (!key.is_empty() && key.iter().all(|k| words(&it.item.name).iter().any(|x| same(k, x)))))
        .map_or_else(|| name.to_string(), |it| it.item.name.trim().to_string())
}

// ---- the estimate --------------------------------------------------------------------

/// What the estimate is made from.
pub struct Input<'a> {
    /// the journal's lines, in its order
    pub journal: &'a [String],
    pub class: i64,
    pub race: i64,
    pub level: i64,
    /// the player's world (`gWorldInfo`'s index) and place (x, z)
    pub world: i32,
    pub at: [f32; 2],
    /// the inventory, worn items too
    pub items: &'a [NetItem],
    pub tunar: i64,
}

/// The marks and the tracker the guide made.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Estimate {
    pub marks: Vec<QuestMark>,
    pub tracker: Vec<TrackedQuest>,
}

/// What the guide keeps of a character between runs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kept {
    /// its home city (`CITIES`), once a journal line named one of its quests
    pub city: Option<u8>,
    /// the quests it finished, by key
    pub finished: BTreeSet<String>,
    /// the kills seen for a kill step: (quest key, step) -> kills
    pub kills: BTreeMap<(String, u8), u16>,
}

/// `s` within `n` bytes, cut at a character and marked (the server's `clip`).
fn clip(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.len() <= n {
        return s.to_string();
    }
    let mut cut = n - 3;
    while cut > 0 && !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}...", s[..cut].trim_end())
}

/// "2 Slashed Pawn Belt", or the name alone for one (the server's `countOf`).
fn count_of(n: u16, name: &str) -> String {
    if n == 1 { name.to_string() } else { format!("{n} {name}") }
}

/// NPC `target`'s spawns (none for `NONE`).
fn places(target: u16) -> &'static [(u8, f32, f32)] {
    NPCS.get(target as usize).map_or(&[], |n| n.places)
}

/// The two nearest of `places` to `at` in `world` (another world's after).
fn nearest(places: &[(u8, f32, f32)], world: i32, at: [f32; 2]) -> Vec<((u8, f32, f32), f32)> {
    let mut out: Vec<((u8, f32, f32), f32)> = places.iter().map(|&p| (p, if p.0 as i32 == world { (p.1 - at[0]).hypot(p.2 - at[1]) } else { -1.0 })).collect();
    out.sort_by(|a, b| (a.1 < 0.0).cmp(&(b.1 < 0.0)).then(a.1.total_cmp(&b.1)));
    out.truncate(MAX_SPOTS);
    out
}

/// The marks and the tracker for `input`, with each journal line's match
/// (`matches`, in the journal's order) and what is kept of the character
/// (its home city first in `cities`).
pub fn estimate(input: &Input, matches: &[Option<Match>], kept: &Kept, cities: &[u8]) -> Estimate {
    let have = |n: &str| held(input.items, n);
    let mut marks: Vec<(QuestMark, f32)> = Vec::new();
    let mut add = |kind: u8, npc: &str, places: &[(u8, f32, f32)], mq: MarkQuest| {
        for (p, d) in nearest(places, input.world, input.at) {
            let at = marks.iter().position(|(m, _)| m.kind == kind && m.world == p.0 && m.x == p.1 && m.z == p.2);
            let k = at.unwrap_or_else(|| {
                marks.push((QuestMark { kind, world: p.0, x: p.1, z: p.2, npc: clip(&format!("{npc}{GUIDE}"), MAX_TEXT), quests: Vec::new() }, d));
                marks.len() - 1
            });
            if marks[k].0.quests.len() < MAX_MARK_QUESTS {
                marks[k].0.quests.push(mq.clone());
            }
        }
    };
    let mut tracker = Vec::new();
    let mut under_way: Vec<usize> = Vec::new();
    for (line, m) in input.journal.iter().zip(matches.iter().chain(std::iter::repeat(&None))) {
        let Some(m) = m.filter(|m| !under_way.contains(&m.quest)) else {
            tracker.push(TrackedQuest { name: line.clone(), goals: Vec::new() });
            continue;
        };
        under_way.push(m.quest);
        let q = &QUESTS[m.quest];
        let s = &q.steps[m.step];
        let target = NPCS.get(s.target as usize).map_or("", |n| n.name);
        // the step's words: the game's own line for the step it names, the
        // guide's for one after it
        let said = if m.step == m.named { line.clone() } else { s.journal.to_string() };
        let mq = |detail: String| MarkQuest { name: clip(q.name, MAX_TEXT), levels: q.levels, line: clip(&said, MAX_TEXT), detail: clip(&detail, MAX_TEXT) };
        let mut goals = Vec::new();
        let u16_of = |v: i64| v.clamp(0, u16::MAX as i64) as u16;
        match s.kind {
            Kind::Show | Kind::Give => {
                for &(n, c) in s.items {
                    goals.push(QuestGoal { text: shown(input.items, n), have: u16_of(have(n) as i64), need: c });
                }
                if s.tunar > 0 {
                    goals.push(QuestGoal { text: "Tunar".into(), have: u16_of(input.tunar), need: u16_of(s.tunar as i64) });
                }
                let verb = if s.kind == Kind::Show { "Show to" } else { "Give to" };
                goals.push(QuestGoal { text: format!("{verb} {target}"), have: 0, need: 0 });
                let needs: Vec<String> = s.items.iter().map(|&(n, c)| count_of(c, &shown(input.items, n))).chain((s.tunar > 0).then(|| format!("{} tunar", s.tunar))).collect();
                add(2, target, places(s.target), mq(needs.join(", ")));
            }
            Kind::Loot => {
                if let Some(&(n, c)) = s.items.first() {
                    let got = u16_of(have(n) as i64);
                    goals.push(QuestGoal { text: shown(input.items, n), have: got, need: c });
                    add(3, target, places(s.target), mq(format!("{}: {} of {c}", shown(input.items, n), got.min(c))));
                }
            }
            Kind::Kill => {
                let need = s.count.max(1);
                let seen = kept.kills.get(&(q.key.to_string(), m.step as u8)).copied().unwrap_or(0);
                goals.push(QuestGoal { text: format!("{target} (seen)"), have: seen, need });
                add(3, target, places(s.target), mq(format!("{target}: {} of {need} killed (seen)", seen.min(need))));
            }
            Kind::Talk => {
                let text = if m.step == m.named { line.clone() } else { format!("Speak with {target}") };
                goals.push(QuestGoal { text, have: 0, need: 0 });
                add(2, target, places(s.target), mq(String::new()));
            }
            Kind::Reach => {
                goals.push(QuestGoal { text: said.clone(), have: 0, need: 0 });
                if let Some(p) = s.at {
                    add(2, s.journal, &[p], mq(String::new()));
                }
            }
        }
        tracker.push(TrackedQuest { name: format!("{}{GUIDE}", q.name), goals });
    }
    // the quests to take: the player's own, at its level, the quests they
    // need finished, of its home city or a chain begun elsewhere
    let home = cities.first().copied();
    let who = Who { class: input.class, race: input.race, level: input.level, cities, finished: &kept.finished, after: &[] };
    for (qi, q) in QUESTS.iter().enumerate() {
        let open = may_have(q, &who)
            && input.level <= q.levels.1 as i64
            && !kept.finished.contains(q.key)
            && !under_way.contains(&qi)
            && q.requires.iter().all(|&r| kept.finished.contains(QUESTS[r as usize].key))
            && (home.is_none_or(|h| h == q.city) || !q.requires.is_empty());
        if open {
            let giver = &NPCS[q.giver as usize];
            add(1, giver.name, giver.places, MarkQuest { name: clip(q.name, MAX_TEXT), levels: q.levels, line: q.offer.to_string(), detail: q.rewards.to_string() });
        }
    }
    // the steps' first, then the player's world, nearest first
    marks.sort_by(|a, b| (a.0.kind == 1).cmp(&(b.0.kind == 1)).then((a.1 < 0.0).cmp(&(b.1 < 0.0))).then(a.1.total_cmp(&b.1)));
    marks.truncate(MAX_MARKS);
    Estimate { marks: marks.into_iter().map(|(m, _)| m).collect(), tracker }
}

// ---- the guide, as a game keeps it ---------------------------------------------------

/// The guide for one character: its kept record, the journal as last
/// matched, what just left it, the creatures the player hit, and the
/// estimate last made.
#[derive(Default)]
pub struct Guide {
    /// whose record it keeps: the character's server id and name
    who: Option<(i64, String)>,
    pub kept: Kept,
    /// the journal last matched: each line and its quest
    lines: Vec<(String, Option<usize>)>,
    /// quests whose line left the journal, and when
    gone: Vec<(usize, u32)>,
    /// the creatures the player or its pet hit, and when
    hits: Vec<(u32, u32)>,
    /// the player's home cities as last worked out
    cities: Vec<u8>,
    pub estimate: Estimate,
    made: Option<u32>,
}

/// Whether a line the game said (0x0046) is its quest-finished notice.
pub fn is_finished(text: &[u16]) -> bool {
    String::from_utf16_lossy(text).trim() == FINISHED
}

impl Guide {
    /// The character the guide keeps for (its record read when it is
    /// another), its id on the server and its name.
    pub fn character(&mut self, server_id: i64, name: &str) {
        if self.who.as_ref().is_some_and(|(i, n)| *i == server_id && n == name) {
            return;
        }
        *self = Guide { who: Some((server_id, name.to_string())), kept: kept_of(&crate::settings::quest_guide(), server_id, name), ..Default::default() };
    }

    /// Keeps the record (when there is a character).
    fn keep(&self) {
        if let Some((id, name)) = &self.who {
            let text = with_kept(&crate::settings::quest_guide(), *id, name, &self.kept);
            crate::settings::save_quest_guide(&text);
        }
    }

    /// The player (or its pet) hit entity `id` (0x00DB).
    pub fn hit(&mut self, id: u32, now: u32) {
        self.hits.retain(|&(i, _)| i != id);
        self.hits.push((id, now));
    }

    fn who<'a>(&'a self, input: &Input, after: &'a [usize]) -> Who<'a> {
        Who { class: input.class, race: input.race, level: input.level, cities: &self.cities, finished: &self.kept.finished, after }
    }

    /// The game's "You have finished a quest!": a quest whose matched line
    /// left the journal (since the last frame, or lately) and has none in it
    /// now is finished.
    pub fn finished_notice(&mut self, input: &Input, now: u32) {
        let mut done: Vec<usize> = self.lines.iter().filter(|(t, _)| !input.journal.contains(t)).filter_map(|(_, q)| *q).collect();
        done.extend(self.gone.iter().filter(|&&(_, t)| now.wrapping_sub(t) <= GONE_MS).map(|&(q, _)| q));
        done.sort_unstable();
        done.dedup();
        let items = input.items;
        let have = |n: &str| held(items, n);
        let mut changed = false;
        for q in done {
            let after = [q];
            let who = self.who(input, &after);
            let still = input.journal.iter().any(|l| match self.lines.iter().find(|(t, _)| t == l) {
                Some((_, m)) => *m == Some(q),
                None => match_line(l, &who, &have).is_some_and(|m| m.quest == q),
            });
            if !still {
                changed |= self.kept.finished.insert(QUESTS[q].key.to_string());
                self.kept.kills.retain(|(k, _), _| k != QUESTS[q].key);
            }
        }
        if changed {
            self.keep();
            self.made = None;
        }
    }

    /// A creature the player killed (`name`): a kill more for each kill step
    /// in the journal of its kind.
    fn killed(&mut self, name: &str, matches: &[Option<Match>]) {
        let mut changed = false;
        for m in matches.iter().flatten() {
            let q = &QUESTS[m.quest];
            let s = &q.steps[m.step];
            if s.kind == Kind::Kill && NPCS.get(s.target as usize).is_some_and(|n| n.name.trim().eq_ignore_ascii_case(name.trim())) {
                let k = self.kept.kills.entry((q.key.to_string(), m.step as u8)).or_default();
                *k = k.saturating_add(1);
                changed = true;
            }
        }
        if changed {
            self.keep();
            self.made = None;
        }
    }

    /// Every frame the guide is on: the creatures hit that died since
    /// (`dead`: an entity's name while it is a corpse) counted, the journal
    /// matched again when it changed, and the estimate made again when
    /// anything did or half a second went by.
    pub fn frame(&mut self, input: &Input, dead: &dyn Fn(u32) -> Option<String>, now: u32) {
        let fresh = self.lines.len() != input.journal.len() || self.lines.iter().zip(input.journal).any(|((t, _), l)| t != l);
        if !fresh && self.made.is_some_and(|t| now.wrapping_sub(t) < EVERY_MS) {
            return;
        }
        self.made = Some(now);
        self.cities = homes(self.kept.city, input.race, input.class, input.world, input.at);
        // the lines gone: their quests, for the line that follows
        for (t, q) in &self.lines {
            if let Some(q) = q
                && !input.journal.contains(t)
            {
                self.gone.push((*q, now));
            }
        }
        self.gone.retain(|&(_, t)| now.wrapping_sub(t) <= GONE_MS);
        let after: Vec<usize> = self.gone.iter().map(|&(q, _)| q).collect();
        let items = input.items;
        let have = |n: &str| held(items, n);
        // a line keeps the quest it was matched to while it is there
        let mut lines = Vec::with_capacity(input.journal.len());
        let mut matches = Vec::with_capacity(input.journal.len());
        let mut learned = false;
        for l in input.journal {
            let kept = self.lines.iter().find(|(t, _)| t == l).map(|(_, q)| *q);
            let m = match kept {
                Some(Some(q)) => steps_of(q, &Line::of(l), &have).map(|(named, step, score)| Match { quest: q, named, step, score }),
                _ => {
                    let who = self.who(input, &after);
                    let m = match_line(l, &who, &have);
                    learned |= m.is_some();
                    m
                }
            };
            lines.push((l.clone(), m.map(|m| m.quest)));
            matches.push(m);
        }
        self.lines = lines;
        // a quest under way says the chain before it is done, and the city
        if learned {
            let mut changed = false;
            for m in matches.iter().flatten() {
                for r in chain(m.quest) {
                    changed |= self.kept.finished.insert(QUESTS[r].key.to_string());
                }
                if self.kept.city != Some(QUESTS[m.quest].city) {
                    self.kept.city = Some(QUESTS[m.quest].city);
                    changed = true;
                }
            }
            // kills kept for a quest no longer in the journal go
            let keys: Vec<&str> = matches.iter().flatten().map(|m| QUESTS[m.quest].key).collect();
            let before = self.kept.kills.len();
            self.kept.kills.retain(|(k, _), _| keys.contains(&k.as_str()));
            changed |= self.kept.kills.len() != before;
            if changed {
                self.keep();
                self.cities = homes(self.kept.city, input.race, input.class, input.world, input.at);
            }
        }
        // the creatures hit that are corpses now
        let mut kills = Vec::new();
        self.hits.retain(|&(id, t)| {
            if now.wrapping_sub(t) > HIT_MS {
                return false;
            }
            match dead(id) {
                Some(name) => {
                    kills.push(name);
                    false
                }
                None => true,
            }
        });
        for name in kills {
            self.killed(&name, &matches);
        }
        self.made = Some(now);
        self.estimate = estimate(input, &matches, &self.kept, &self.cities);
    }
}

// ---- the kept record -----------------------------------------------------------------

/// A character's record in the guide's file (`settings::quest_guide`): a line
/// `<server id>:<name>`, then `city=<city>`, `done=<key>,<key>` and
/// `kills=<key>#<step>=<n>,...`, each when there is one.
fn kept_of(text: &str, server_id: i64, name: &str) -> Kept {
    let me = format!("{server_id}:{name}");
    let mut k = Kept::default();
    let Some(line) = text.lines().find(|l| l.split_whitespace().next() == Some(me.as_str())) else { return k };
    for f in line.split_whitespace().skip(1) {
        match f.split_once('=') {
            Some(("city", c)) => k.city = CITIES.iter().position(|&x| x == c).map(|i| i as u8),
            Some(("done", d)) => k.finished = d.split(',').filter(|s| QUESTS.iter().any(|q| q.key == *s)).map(str::to_string).collect(),
            Some(("kills", d)) => {
                for e in d.split(',') {
                    if let Some((ks, n)) = e.split_once('=')
                        && let Some((key, step)) = ks.rsplit_once('#')
                        && let (Ok(step), Ok(n)) = (step.parse::<u8>(), n.parse::<u16>())
                    {
                        k.kills.insert((key.to_string(), step), n);
                    }
                }
            }
            _ => {}
        }
    }
    k
}

/// `text` (the guide's file) with the character's line made `kept`'s (put
/// last; gone when it keeps nothing).
fn with_kept(text: &str, server_id: i64, name: &str, kept: &Kept) -> String {
    let me = format!("{server_id}:{name}");
    let mut out: String = text.lines().filter(|l| !l.trim().is_empty() && l.split_whitespace().next() != Some(me.as_str())).map(|l| format!("{l}\n")).collect();
    if *kept != Kept::default() {
        let mut line = me;
        if let Some(c) = kept.city {
            line.push_str(&format!(" city={}", CITIES[c as usize]));
        }
        if !kept.finished.is_empty() {
            line.push_str(&format!(" done={}", kept.finished.iter().cloned().collect::<Vec<_>>().join(",")));
        }
        if !kept.kills.is_empty() {
            line.push_str(&format!(" kills={}", kept.kills.iter().map(|((k, s), n)| format!("{k}#{s}={n}")).collect::<Vec<_>>().join(",")));
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}
