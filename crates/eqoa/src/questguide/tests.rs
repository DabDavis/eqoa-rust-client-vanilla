use super::*;

const WAR: i64 = 0;
const WIZ: i64 = 13;
const BRD: i64 = 5;
const HUM: i64 = 0;

fn quest(key: &str) -> usize {
    QUESTS.iter().position(|q| q.key == key).unwrap_or_else(|| panic!("no quest {key}"))
}

fn npc(name: &str) -> &'static Npc {
    NPCS.iter().find(|n| n.name == name).unwrap_or_else(|| panic!("no NPC {name}"))
}

fn item(name: &str, quantity: i64, worn: bool) -> NetItem {
    let mut it = NetItem { quantity, equip_slot: if worn { 4 } else { -1 }, ..Default::default() };
    it.item.name = name.into();
    it
}

/// Who a Freeport warrior of `level` is, with `finished` done and `after`
/// just gone from the journal.
fn warrior<'a>(level: i64, finished: &'a BTreeSet<String>, after: &'a [usize]) -> Who<'a> {
    Who { class: WAR, race: HUM, level, cities: &[1], finished, after }
}

fn none(_: &str) -> u32 {
    0
}

fn matched(line: &str, who: &Who, held: &dyn Fn(&str) -> u32) -> Option<(&'static str, usize)> {
    match_line(line, who, held).map(|m| (QUESTS[m.quest].key, m.step))
}

/// The words a line is compared by: lower case, plurals as singulars, a
/// possessive and a backtick as the game spells a Teir'Dal name.
#[test]
fn a_line_s_words_meet_their_singulars() {
    assert_eq!(words("Bring Norgram 2 slashed orc pawn belts ."), ["bring", "norgram", "2", "slashed", "orc", "pawn", "belt"]);
    assert_eq!(words("Ronks's ledger"), words("Ronks ledger"));
    assert_eq!(words("Dragoon V`Ghen"), words("dragoon v'ghen"));
    for (a, b) in [("flies", "fly"), ("wolves", "wolf"), ("boxes", "box"), ("horses", "horse"), ("bracers", "bracer"), ("glasses", "glass")] {
        assert_eq!(words(a), words(b), "{a} and {b}");
    }
    assert_ne!(words("belts"), words("bolts"), "the control: another word");
    assert!(same("norgram", "norgam"), "a long name one letter off");
    assert!(!same("norgxyz", "norgam") && !same("zole", "zola"), "the control: two letters off, or a short word");
}

/// Sandstorm's own journal lines for a Freeport warrior's first quests
/// (`../claude-notes/quests-sandstorm-20261025.md`), worded as the guide's
/// never are, each found as its quest's step; a step whose handed-over item
/// is held, or a loot step whose drops all are, gives way to the next.
#[test]
fn sandstorm_s_lines_are_the_guide_s_steps() {
    let done = BTreeSet::new();
    let w1 = warrior(1, &done, &[]);
    let bracer = |n: &str| (n == "Militia Bracer") as u32;
    assert_eq!(matched("Pick up Militia bracers from Merchant Galosh.", &w1, &none), Some(("1-20/freeport/warrior/L1", 0)), "to Galosh");
    assert_eq!(matched("Pick up Militia bracers from Merchant Galosh.", &w1, &bracer), Some(("1-20/freeport/warrior/L1", 1)), "the bracer held: show it to Nothard");
    let w2 = warrior(2, &done, &[]);
    assert_eq!(matched("Speak with Spiritmaster Zole outside the guild hall.", &w2, &none), Some(("1-20/freeport/warrior/L2", 0)));
    assert_eq!(matched("Speak with Coachman Ronks.", &w2, &none), Some(("1-20/freeport/warrior/L2", 1)));
    // "Nothard" alone names Commander Nothard; the quest whose line just
    // went is the one it goes on
    let l2 = [quest("1-20/freeport/warrior/L2")];
    assert_eq!(matched("Return to Nothard.", &warrior(2, &done, &l2), &none), Some(("1-20/freeport/warrior/L2", 2)));
    // "Norgram" (sic) one letter off Captain Norgam; the belts held, the
    // hand-in is next
    let w3 = warrior(3, &done, &[]);
    let belts = |n: &str| if n == "Slashed Pawn Belt" { 2 } else { 0 };
    assert_eq!(matched("Bring Norgram 2 slashed orc pawn belts .", &w3, &none), Some(("1-20/freeport/warrior/L3", 0)), "to the orc pawns");
    assert_eq!(matched("Bring Norgram 2 slashed orc pawn belts .", &w3, &belts), Some(("1-20/freeport/warrior/L3", 1)), "both held: to Norgam");
    let one = |n: &str| (n == "Slashed Pawn Belt") as u32;
    assert_eq!(matched("Bring Norgram 2 slashed orc pawn belts .", &w3, &one), Some(("1-20/freeport/warrior/L3", 0)), "the control: one of two");
}

/// An NPC named in two steps of a quest (a talk, then the hand-in): the
/// talk, unless the hand-in's items are all held; the control one short.
#[test]
fn a_hand_in_ready_is_the_step() {
    let (qi, talk, give) = QUESTS
        .iter()
        .enumerate()
        .find_map(|(qi, q)| {
            let g = q.steps.iter().position(|s| s.kind == Kind::Give && !s.items.is_empty() && s.tunar == 0)?;
            let t = q.steps[..g].iter().position(|s| s.kind == Kind::Talk && s.target == q.steps[g].target && s.gets.is_empty())?;
            Some((qi, t, g))
        })
        .expect("a quest with a talk and a hand-in to one NPC");
    let q = &QUESTS[qi];
    let line = format!("Go to {}.", NPCS[q.steps[give].target as usize].name);
    let class = (0..15).find(|&c| bit(q.classes, c)).unwrap();
    let finished: BTreeSet<String> = chain(qi).into_iter().map(|r| QUESTS[r].key.to_string()).collect();
    let who = Who { class, race: HUM, level: q.levels.0 as i64, cities: &[q.city], finished: &finished, after: &[] };
    let items = q.steps[give].items;
    let all = |n: &str| items.iter().find(|i| i.0 == n).map_or(0, |i| i.1 as u32);
    let short = |n: &str| items.iter().find(|i| i.0 == n).map_or(0, |i| i.1 as u32 - 1);
    assert_eq!(matched(&line, &who, &all), Some((q.key, give)), "{line}: the items held");
    assert_eq!(matched(&line, &who, &short), Some((q.key, talk)), "the control: one short");
}

/// The directive's case: a line naming Commander Nothard and a militia
/// bracer is the Freeport warrior's first quest, its show step; for a class
/// whose quests these are not, it is not that quest.
#[test]
fn nothard_and_a_militia_bracer_are_the_first_warrior_quest() {
    let done = BTreeSet::new();
    let line = "Show your Militia Bracer to Commander Nothard.";
    assert_eq!(matched(line, &warrior(1, &done, &[]), &none), Some(("1-20/freeport/warrior/L1", 1)));
    let wizard = Who { class: WIZ, ..warrior(1, &done, &[]) };
    assert_ne!(matched(line, &wizard, &none).map(|m| m.0), Some("1-20/freeport/warrior/L1"), "the control: a wizard has no such quest");
}

/// A line naming nobody the guide knows stays text: a veteran's journal
/// (the PS2 captures' Mcgregor, a bard of 51), and a line of words alone;
/// the control: a line of the guide's own words is matched.
#[test]
fn a_line_naming_nobody_known_stays_text() {
    let done = BTreeSet::new();
    let bard = Who { class: BRD, level: 51, ..warrior(51, &done, &[]) };
    for line in ["Go to your hometown and meet the fish farmer.", "Slay Parathior and return to Crikston Gundlefop with Chunk of Parathior.", "Talk to the man by the gate."] {
        assert_eq!(matched(line, &bard, &none), None, "{line}");
        assert_eq!(matched(line, &warrior(20, &done, &[]), &none), None, "{line}, for a warrior");
    }
    let q = &QUESTS[quest("1-20/freeport/warrior/L2")];
    assert_eq!(matched(q.steps[1].journal, &warrior(2, &done, &[]), &none), Some((q.key, 1)), "the control: the guide's own line");
}

/// The quests to take ("!", kind 1): the player's class, its level within
/// the quest's, every quest it needs finished, not under way nor finished,
/// of its home city; each control the same with that one thing changed.
#[test]
fn a_quest_to_take_is_the_player_s() {
    let nothard = npc("Commander Nothard");
    let at = [nothard.places[0].1, nothard.places[0].2];
    let takes = |class: i64, level: i64, finished: &[&str], journal: &[&str], cities: &[u8]| -> Vec<String> {
        let journal: Vec<String> = journal.iter().map(|s| s.to_string()).collect();
        let kept = Kept { finished: finished.iter().map(|s| s.to_string()).collect(), ..Default::default() };
        let input = Input { journal: &journal, class, race: HUM, level, world: 0, at, items: &[], tunar: 0 };
        let who = Who { class, race: HUM, level, cities, finished: &kept.finished, after: &[] };
        let matches: Vec<Option<Match>> = journal.iter().map(|l| match_line(l, &who, &none)).collect();
        estimate(&input, &matches, &kept, cities).marks.into_iter().filter(|m| m.kind == 1).flat_map(|m| m.quests.into_iter().map(|q| q.name)).collect()
    };
    let name = |key: &str| QUESTS[quest(key)].name.to_string();
    let (l1, l2, l3) = (name("1-20/freeport/warrior/L1"), name("1-20/freeport/warrior/L2"), name("1-20/freeport/warrior/L3"));
    // a new warrior: the first quest, not the second (it needs the first)
    let new = takes(WAR, 1, &[], &[], &[1]);
    assert!(new.contains(&l1) && !new.contains(&l2), "{new:?}");
    // the first finished, at level 2: the second, and the first no more
    let two = takes(WAR, 2, &["1-20/freeport/warrior/L1"], &[], &[1]);
    assert!(two.contains(&l2) && !two.contains(&l1) && !two.contains(&l3), "{two:?}");
    assert!(!takes(WAR, 1, &["1-20/freeport/warrior/L1"], &[], &[1]).contains(&l2), "the level: the second is for level 2");
    // a wizard: none of the warrior's
    let wiz = takes(WIZ, 1, &[], &[], &[1]);
    assert!(!wiz.contains(&l1) && !wiz.is_empty(), "the class: {wiz:?}");
    // under way: no "!" for it
    assert!(!takes(WAR, 1, &[], &["Pick up Militia bracers from Merchant Galosh."], &[1]).contains(&l1), "in the journal");
    // another city's first quest only for its own
    let qeynos = name("1-20/qeynos/warrior/L1");
    assert!(!new.contains(&qeynos), "Qeynos's first quest is not a Freeport warrior's");
    assert!(takes(WAR, 1, &[], &[], &[9]).contains(&qeynos), "the control: a Qeynos warrior's");
}

/// The marks are labelled the guide's, the steps' first ("?" at Galosh for
/// the line, kind 2), the giver's "!" after; a hunt is kind 3 at the
/// creature's two nearest spawns.
#[test]
fn the_marks_are_the_guide_s_at_the_steps() {
    let galosh = npc("Merchant Galosh");
    let at = [galosh.places[0].1, galosh.places[0].2];
    let journal = vec!["Pick up Militia bracers from Merchant Galosh.".to_string(), "Bring Norgram 2 slashed orc pawn belts .".to_string()];
    let kept = Kept::default();
    let input = Input { journal: &journal, class: WAR, race: HUM, level: 3, world: 0, at, items: &[], tunar: 0 };
    let who = Who { class: WAR, race: HUM, level: 3, cities: &[1], finished: &kept.finished, after: &[] };
    let matches: Vec<Option<Match>> = journal.iter().map(|l| match_line(l, &who, &none)).collect();
    let e = estimate(&input, &matches, &kept, &[1]);
    assert_eq!(e.marks[0].kind, 2);
    assert_eq!(e.marks[0].npc, format!("Merchant Galosh{GUIDE}"));
    assert_eq!(e.marks[0].quests[0].line, journal[0], "the game's own line on its card");
    let hunts: Vec<&QuestMark> = e.marks.iter().filter(|m| m.kind == 3).collect();
    assert!(!hunts.is_empty() && hunts.len() <= MAX_SPOTS && hunts.iter().all(|m| m.npc == format!("Orc Pawn{GUIDE}")), "{hunts:?}");
    assert_eq!(hunts[0].quests[0].detail, "Slashed Pawn Belt: 0 of 2");
    let first_take = e.marks.iter().position(|m| m.kind == 1).unwrap_or(e.marks.len());
    assert!(e.marks[first_take..].iter().all(|m| m.kind == 1), "the steps first");
    assert!(e.marks.iter().all(|m| m.npc.ends_with(GUIDE)), "every card the guide's");
}

/// The tracker: a show step's item held of those needed, then whom to show
/// it to; a loot step's drops held; a talk step the game's own line; a line
/// matched to nothing its text, unlabelled; the guide's quests labelled.
#[test]
fn the_tracker_counts_what_is_held() {
    let journal = vec!["Show your Militia Bracer to Commander Nothard.".to_string(), "Go to your hometown and meet the fish farmer.".to_string()];
    let items = [item("Militia Bracer", 1, true)];
    let kept = Kept::default();
    let input = Input { journal: &journal, class: WAR, race: HUM, level: 1, world: 0, at: [0.0, 0.0], items: &items, tunar: 0 };
    let who = Who { class: WAR, race: HUM, level: 1, cities: &[1], finished: &kept.finished, after: &[] };
    let have = |n: &str| held(&items, n);
    let matches: Vec<Option<Match>> = journal.iter().map(|l| match_line(l, &who, &have)).collect();
    let e = estimate(&input, &matches, &kept, &[1]);
    assert_eq!(e.tracker.len(), 2);
    assert_eq!(e.tracker[0].name, format!("The Militia Bracer{GUIDE}"));
    assert_eq!(e.tracker[0].goals, [QuestGoal { text: "Militia Bracer".into(), have: 1, need: 1 }, QuestGoal { text: "Show to Commander Nothard".into(), have: 0, need: 0 }], "worn, it counts");
    assert_eq!(e.tracker[1], TrackedQuest { name: journal[1].clone(), goals: Vec::new() }, "the game's line as it is");
    // the control: not held, 0 of 1
    let bare = Input { items: &[], ..input };
    assert_eq!(estimate(&bare, &matches, &kept, &[1]).tracker[0].goals[0].have, 0);
    // a loot step's drops; a talk step's line
    let journal = vec!["Bring Norgram 2 slashed orc pawn belts .".to_string(), "Speak with Coachman Ronks.".to_string()];
    let items = [item("Slashed Pawn Belt", 1, false)];
    let input = Input { journal: &journal, level: 3, items: &items, ..bare };
    let who = Who { level: 3, ..who };
    let matches: Vec<Option<Match>> = journal.iter().map(|l| match_line(l, &who, &have)).collect();
    let e = estimate(&input, &matches, &kept, &[1]);
    assert_eq!(e.tracker[0].goals, [QuestGoal { text: "Slashed Pawn Belt".into(), have: 1, need: 2 }]);
    assert_eq!(e.tracker[1].goals, [QuestGoal { text: "Speak with Coachman Ronks.".into(), have: 0, need: 0 }]);
}

/// Items held: by name in any case, a stack's count and every stack's,
/// worn ones, and a name with more words (Sandstorm's) holding all of the
/// guide's; the controls another item and one word short.
#[test]
fn items_held_are_counted() {
    let items = [item("slashed pawn belt", 2, false), item("Slashed Pawn Belt", 1, false), item("Militia Bracer", 1, true), item("Leather Belt", 5, false)];
    assert_eq!(held(&items, "Slashed Pawn Belt"), 3, "any case, every stack");
    assert_eq!(held(&items, "Militia Bracer"), 1, "worn");
    assert_eq!(held(&[item("Slashed Orc Pawn Belt", 2, false)], "Slashed Pawn Belt"), 2, "more words in the game's name");
    assert_eq!(held(&items, "Bat Wing"), 0, "the control: none");
    assert_eq!(held(&[item("Pawn Belt", 4, false)], "Slashed Pawn Belt"), 0, "the control: a word short");
    assert_eq!(shown(&[item("Slashed Orc Pawn Belt", 2, false)], "Slashed Pawn Belt"), "Slashed Orc Pawn Belt", "the game's name shown");
}

/// The home cities: where the game starts a race and class (a human
/// warrior Freeport's or Qeynos's, the nearer first), the one kept first.
#[test]
fn the_home_city_is_the_race_s_and_class_s() {
    let city = |k: &str| CITIES.iter().position(|&c| c == k).unwrap() as u8;
    let nothard = npc("Commander Nothard").places[0];
    assert_eq!(homes(None, HUM, WAR, 0, [nothard.1, nothard.2]), [city("freeport"), city("qeynos")]);
    let qeynos = &NPCS[QUESTS[quest("1-20/qeynos/warrior/L1")].giver as usize].places[0];
    assert_eq!(homes(None, HUM, WAR, 0, [qeynos.1, qeynos.2]), [city("qeynos"), city("freeport")], "the control: by Qeynos");
    assert_eq!(homes(Some(city("qeynos")), HUM, WAR, 0, [nothard.1, nothard.2]), [city("qeynos")], "the kept one");
    assert_eq!(homes(None, 2, WAR, 0, [0.0, 0.0]), [city("neriak-nektulos")], "a dark elf");
}

/// The record: each character's line read back as it was kept, another's
/// left alone, a character keeping nothing writing none.
#[test]
fn the_record_is_kept_per_character() {
    let kept = Kept {
        city: Some(1),
        finished: ["1-20/freeport/warrior/L1".to_string(), "1-20/freeport/warrior/L2".to_string()].into(),
        kills: [(("1-20/freeport/warrior/L3".to_string(), 0u8), 1u16)].into(),
    };
    let text = with_kept("7:Other city=qeynos\n", 12, "Halvardin", &kept);
    assert_eq!(kept_of(&text, 12, "Halvardin"), kept);
    assert_eq!(kept_of(&text, 7, "Other").city, Some(9), "another's line stays");
    assert_eq!(kept_of(&text, 13, "Halvardin"), Kept::default(), "the control: another character of the name");
    assert_eq!(with_kept(&text, 12, "Halvardin", &Kept::default()), "7:Other city=qeynos\n", "nothing kept, no line");
    let junk = kept_of("12:Halvardin done=nope,1-20/freeport/warrior/L1 city=nowhere kills=x\n", 12, "Halvardin");
    assert_eq!(junk, Kept { finished: ["1-20/freeport/warrior/L1".to_string()].into(), ..Default::default() }, "what does not read is left out");
}

/// A game as Sandstorm runs it: a quest's last line goes and the game says
/// "You have finished a quest!": the quest is finished and kept; a middle
/// step's line replaced by the next's is not (the control); a quest matched
/// says the chain before it is done.
#[test]
fn a_finished_quest_is_kept() {
    crate::settings::testing::reset(false);
    let mut g = Guide::default();
    g.character(12, "Halvardin");
    let input = |journal: &'static [&'static str], level: i64| (journal.iter().map(|s| s.to_string()).collect::<Vec<_>>(), level);
    let run = |g: &mut Guide, (journal, level): (Vec<String>, i64), now: u32| {
        let i = Input { journal: &journal, class: WAR, race: HUM, level, world: 0, at: [25486.0, 15546.0], items: &[], tunar: 0 };
        g.frame(&i, &|_| None, now);
    };
    let notice = |g: &mut Guide, (journal, level): (Vec<String>, i64), now: u32| {
        let i = Input { journal: &journal, class: WAR, race: HUM, level, world: 0, at: [25486.0, 15546.0], items: &[], tunar: 0 };
        g.finished_notice(&i, now);
    };
    run(&mut g, input(&["Speak with Spiritmaster Zole outside the guild hall."], 2), 1000);
    assert_eq!(g.kept.finished, ["1-20/freeport/warrior/L1".to_string()].into(), "the chain before it");
    assert_eq!(g.kept.city, Some(1));
    // a middle step: the line replaced by the next one's, then the notice
    notice(&mut g, input(&["Speak with Coachman Ronks."], 2), 1500);
    assert!(!g.kept.finished.contains("1-20/freeport/warrior/L2"), "the control: under way");
    run(&mut g, input(&["Speak with Coachman Ronks."], 2), 1600);
    run(&mut g, input(&["Return to Nothard."], 2), 2200);
    assert_eq!(g.estimate.tracker[0].name, format!("Bound to Freeport{GUIDE}"), "Nothard after Ronks is the same quest");
    // the last line goes, then the notice
    notice(&mut g, input(&[], 3), 3000);
    assert!(g.kept.finished.contains("1-20/freeport/warrior/L2"));
    let kept = crate::settings::quest_guide();
    assert!(kept.starts_with("12:Halvardin city=freeport done=1-20/freeport/warrior/L1,1-20/freeport/warrior/L2"), "{kept}");
    // read back by the next run
    let mut h = Guide::default();
    h.character(12, "Halvardin");
    assert_eq!(h.kept, g.kept);
}

/// Kills seen: a creature the player hit that dies counts for a kill step
/// of its kind in the journal; the controls one not hit, another kind, and
/// a hit long ago.
#[test]
fn kills_seen_by_the_player_s_hand_are_counted() {
    crate::settings::testing::reset(false);
    let (qi, si) = QUESTS.iter().enumerate().find_map(|(qi, q)| q.steps.iter().position(|s| s.kind == Kind::Kill && s.count > 1).map(|si| (qi, si))).expect("a kill step");
    let q = &QUESTS[qi];
    let s = &q.steps[si];
    let target = NPCS[s.target as usize].name;
    let class = (0..15).find(|&c| bit(q.classes, c)).unwrap();
    let journal = vec![s.journal.to_string()];
    let input = Input { journal: &journal, class, race: HUM, level: q.levels.0 as i64, world: 0, at: [0.0, 0.0], items: &[], tunar: 0 };
    let mut g = Guide::default();
    g.character(1, "Hunter");
    g.frame(&input, &|_| None, 1000);
    let count = |g: &Guide| g.estimate.tracker[0].goals[0].clone();
    assert_eq!(count(&g), QuestGoal { text: format!("{target} (seen)"), have: 0, need: s.count });
    // hit, then dead: one
    g.hit(7, 1100);
    g.frame(&input, &|id| (id == 7).then(|| target.to_string()), 2000);
    assert_eq!(count(&g).have, 1);
    // a corpse never hit, and one of another kind hit: none more
    g.hit(8, 2100);
    g.frame(&input, &|id| match id {
        8 => Some("Somebody Else".to_string()),
        9 => Some(target.to_string()),
        _ => None,
    }, 3000);
    assert_eq!(count(&g).have, 1, "the control: not the player's, not its kind");
    // a hit too long ago
    g.hit(10, 3100);
    g.frame(&input, &|_| None, 3700);
    g.frame(&input, &|id| (id == 10).then(|| target.to_string()), 3100 + HIT_MS + 1000);
    assert_eq!(count(&g).have, 1, "the control: hit a minute before");
    assert!(crate::settings::quest_guide().contains(&format!("kills={}#{si}=1", q.key)), "kept");
}

/// The guide is small enough to match a journal at once: every line of a
/// full journal against every quest within a few milliseconds' work, and
/// the table whole (every quest's giver and steps named and placed).
#[test]
fn the_table_is_whole() {
    for q in &QUESTS {
        assert!(!q.steps.is_empty() && (q.giver as usize) < NPCS.len(), "{}", q.key);
        for s in q.steps {
            assert!(s.target == NONE || (s.target as usize) < NPCS.len(), "{}", q.key);
            assert!(s.kind != Kind::Reach || s.at.is_some() || s.target != NONE, "{}: a reach step with no place", q.key);
        }
        for &r in q.requires {
            assert!((r as usize) < QUESTS.len());
        }
    }
    assert!(NPCS.iter().all(|n| !n.places.is_empty()), "every name placed");
}
