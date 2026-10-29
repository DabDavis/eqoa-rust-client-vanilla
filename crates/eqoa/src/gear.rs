//! SOE's final live client's weapon sets, and the layout they come with: what
//! `c:/eqoa-live-1500`'s CLIENT does where the Frontiers disc's does otherwise,
//! read off that build where two retail save states hold it resident (CLIENT at
//! 0x012AC400 in one; `claude-notes/live-toolbars-gear-sets-20261024.md`). That
//! build has no decomp, so each function here names the live address it is
//! read from and the tests hold it to what the code there does.
//!
//! A server that keeps sets says so by sending them (0x0021, at world entry):
//! from then on the HUD's carousel has a fourth menu, the four sets and bare
//! hands (`hudmenu`), the item quick slots take a potion (type 2) on 0-1 and a
//! gem (type 7) on 2-3 where the disc's take a gem on 2 and a weapon on 3
//! (`inventory::quick_slot`), and the item menu's Prepare on a weapon makes a
//! set (`itemmenu`). A server that does not (Sandstorm, every server built for
//! the disc) leaves the disc's layout.

use net::dump::{ItemList, NetItem};
use vi::ui::icon_id;

/// The sets (`VIClient+0x14CC4`, four records of 40 bytes); the HUD menu's
/// fifth entry is bare hands.
pub const SETS: usize = 4;
/// The menu entry, and the 0x0022, that empties the hands.
pub const BARE_HANDS: i64 = 4;
/// The HUD menu's entries: the sets and bare hands (`gHudMenu3`'s count).
pub const ENTRIES: usize = SETS + 1;
/// What using a set's HUD entry sends (`SetGearMenuItem`'s `m_msg`; the live
/// `VIWndGame::OnMessage` answers it with `UseGearSet`).
pub const MSG: u32 = 0x020C_0000;
/// The most of a name the editor takes (`SetMaxLength(16)`, the terminator
/// counted) and the client keeps from the server (a 32-byte buffer).
pub const NAME_MAX: usize = 16;

/// One set: the main hand's and the off hand's items by `VIItem::m_id` (the
/// item record's first field; -1 none), and its name.
#[derive(Clone, Debug, PartialEq)]
pub struct Set {
    pub main: i64,
    pub off: i64,
    pub name: String,
}

impl Default for Set {
    fn default() -> Self {
        Set { main: -1, off: -1, name: String::new() }
    }
}

/// The four sets, and whether the server keeps them (it sent one).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gear {
    pub on: bool,
    pub sets: [Set; SETS],
}

/// Word `id`, the live client's own where the table's is "UNUSED" or empty
/// (`livewords`: 159 is bare hands, 160 the set name editor's title).
pub fn word(words: &vi::strtab::StringTable, id: i32, language: i32) -> String {
    crate::livewords::word(words, id, language)
}

/// `FindInvItemById` (0x012D6B98): the first inventory record with that
/// `VIItem::m_id`.
fn find(inv: &ItemList, id: i64) -> Option<&NetItem> {
    inv.items.iter().find(|it| it.item.id == id)
}

impl Gear {
    /// 0x0021 from the server (the live dispatcher's case at 0x012CADEC:
    /// u8 set, a name read into 32 bytes, u32, u32, then `SetGearSet`
    /// 0x012C49E8, which keeps sets 0-3): the sets are kept, so the live
    /// layout is on.
    pub fn set(&mut self, i: u8, name: String, main: u32, off: u32) {
        self.on = true;
        if let Some(s) = self.sets.get_mut(i as usize) {
            *s = Set { main: main as i32 as i64, off: off as i32 as i64, name };
        }
    }

    /// `IsGearSetEmpty` (0x012C4A48): the set has no main hand.
    pub fn empty(&self, i: i64) -> bool {
        usize::try_from(i).ok().and_then(|i| self.sets.get(i)).is_none_or(|s| s.main == -1)
    }

    /// `GetGearSetName` (0x012C4A70): bare hands is word 159, a set its name,
    /// an empty set none.
    pub fn name(&self, i: i64, words: &vi::strtab::StringTable, language: i32) -> String {
        if i == BARE_HANDS {
            return word(words, 159, language);
        }
        match usize::try_from(i).ok().and_then(|i| self.sets.get(i)) {
            Some(s) if s.main != -1 => s.name.clone(),
            _ => String::new(),
        }
    }

    /// `GetGearSetIcon` (0x012C4B18), a UI image's DictID or -1: bare hands
    /// "Fist"; a set whose main hand is a ranged weapon (attack type 7, 9 or
    /// 10) "weapons_ranged"; else by the main hand's wear slot: a primary or
    /// one-handed weapon "sword", or "weapons_melee" with a one-handed weapon
    /// or an item held in the off hand, a two-hander "sword", an item held
    /// "Wand", a fishing pole or a tool "trade_skills", anything else none.
    /// An item not in the inventory has none.
    pub fn icon(&self, i: i64, inv: &ItemList) -> i32 {
        if i == BARE_HANDS {
            return icon_id("Fist") as i32;
        }
        let Some(s) = usize::try_from(i).ok().and_then(|i| self.sets.get(i)).filter(|s| s.main != -1) else { return -1 };
        let Some(a) = find(inv, s.main) else { return -1 };
        if matches!(a.item.attack_type, 7 | 9 | 10) {
            return icon_id("weapons_ranged") as i32;
        }
        let (name, off) = match a.item.wear_slot {
            12 | 14 => ("sword", true),
            15 => ("sword", false),
            18 => ("Wand", false),
            20 | 22..=27 => ("trade_skills", false),
            _ => return -1,
        };
        if off && s.off != -1 && find(inv, s.off).is_some_and(|b| matches!(b.item.wear_slot, 14 | 18)) {
            return icon_id("weapons_melee") as i32;
        }
        icon_id(name) as i32
    }

    /// `GetGearSetBadge` (0x012C4C78): "defence_active" under a set whose off
    /// hand is a shield (wear slot 13), else -1; bare hands has none.
    pub fn badge(&self, i: i64, inv: &ItemList) -> i32 {
        let Some(s) = usize::try_from(i).ok().and_then(|i| self.sets.get(i)) else { return -1 };
        if s.main == -1 || s.off == -1 {
            return -1;
        }
        if find(inv, s.off).is_some_and(|b| b.item.wear_slot == 13) { icon_id("defence_active") as i32 } else { -1 }
    }

    /// What the item menu's set panel shows this frame (`View`).
    pub fn view(&self, inv: &ItemList, words: &vi::strtab::StringTable, language: i32) -> View {
        let all = |f: &dyn Fn(i64) -> i32| std::array::from_fn(|k| f(k as i64));
        View {
            on: self.on,
            names: std::array::from_fn(|k| self.name(k as i64, words, language)),
            icons: all(&|k| self.icon(k, inv)),
            badges: all(&|k| self.badge(k, inv)),
            empty: std::array::from_fn(|k| self.empty(k as i64)),
        }
    }
}

/// The sets as the item menu draws and picks them, each frame's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub on: bool,
    pub names: [String; ENTRIES],
    pub icons: [i32; ENTRIES],
    pub badges: [i32; ENTRIES],
    pub empty: [bool; SETS],
}

/// The wear slots Prepare makes a set of (the live `AssignSlot`'s jump table
/// at 0x013FCE20, wear 12-27): a primary, one-handed or two-handed weapon, a
/// bow, an item held, a fishing pole and the tools; not a shield, a thrown
/// weapon, gloves or bait.
pub fn preparable(wear: i64) -> bool {
    matches!(wear, 12 | 14 | 15 | 16 | 18 | 20 | 22..=27)
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::dump::Item;

    fn item(id: i64, wear: i64, attack: i64) -> NetItem {
        NetItem { equip_slot: -1, quick_slot: -1, item: Item { id, wear_slot: wear, attack_type: attack, ..Default::default() }, ..Default::default() }
    }

    fn gear(sets: &[(i64, i64)]) -> Gear {
        let mut g = Gear::default();
        for (i, &(a, b)) in sets.iter().enumerate() {
            g.set(i as u8, format!("set {i}"), a as i32 as u32, b as i32 as u32);
        }
        g
    }

    /// A 0x0021 turns the layout on and keeps sets 0-3, the ids signed as the
    /// client keeps them (-1 none); set 4 and up are not kept (the control).
    #[test]
    fn a_set_from_the_server_turns_the_layout_on() {
        let mut g = Gear::default();
        assert!(!g.on && g.empty(0));
        g.set(2, "Tank".into(), 100, u32::MAX);
        assert!(g.on);
        assert_eq!(g.sets[2], Set { main: 100, off: -1, name: "Tank".into() });
        g.set(4, "Nope".into(), 7, 8);
        assert_eq!(g.sets.iter().filter(|s| s.main != -1).count(), 1, "set 4 is not a record");
        assert!(!g.empty(2) && g.empty(0) && g.empty(4) && g.empty(-1));
    }

    /// The icons by the main hand's attack type and wear slot, and the off
    /// hand's (`GetGearSetIcon`'s jump table at 0x013EAEA0), every wear slot
    /// 11-28 with nothing in the off hand; the badge only for a shield in it.
    #[test]
    fn icons_follow_the_live_rules() {
        let inv = ItemList {
            items: vec![
                item(1, 12, 1), item(2, 14, 3), item(3, 15, 2), item(4, 16, 7), item(5, 18, 0), item(6, 20, 0),
                item(7, 13, 0), item(8, 17, 10), item(9, 25, 0), item(10, 19, 0), item(11, 14, 9),
            ],
            ..Default::default()
        };
        let one = |main: i64, off: i64| gear(&[(main, off)]);
        let id = |s: &str| icon_id(s) as i32;
        assert_eq!(one(1, -1).icon(0, &inv), id("sword"), "a primary");
        assert_eq!(one(1, 2).icon(0, &inv), id("weapons_melee"), "with a one-hander in the off hand");
        assert_eq!(one(1, 5).icon(0, &inv), id("weapons_melee"), "with an item held in it");
        assert_eq!(one(1, 7).icon(0, &inv), id("sword"), "with a shield: still a sword");
        assert_eq!(one(1, 7).badge(0, &inv), id("defence_active"), "badged");
        assert_eq!(one(1, 2).badge(0, &inv), -1, "no badge without a shield");
        assert_eq!(one(3, 2).icon(0, &inv), id("sword"), "a two-hander never looks at the off hand");
        assert_eq!(one(4, -1).icon(0, &inv), id("weapons_ranged"), "a bow (attack 7)");
        assert_eq!(one(8, -1).icon(0, &inv), id("weapons_ranged"), "thrown (attack 10), before its wear slot");
        assert_eq!(one(11, -1).icon(0, &inv), id("weapons_ranged"), "attack 9 on a one-hander");
        assert_eq!(one(5, -1).icon(0, &inv), id("Wand"));
        assert_eq!(one(6, -1).icon(0, &inv), id("trade_skills"), "a fishing pole");
        assert_eq!(one(9, -1).icon(0, &inv), id("trade_skills"), "a tool");
        for none in [7, 10] {
            assert_eq!(one(none, -1).icon(0, &inv), -1, "item {none}: a shield or gloves as the main hand");
        }
        assert_eq!(one(99, -1).icon(0, &inv), -1, "an item not carried");
        assert_eq!(one(-1, 2).icon(0, &inv), -1, "an empty set");
        assert_eq!(Gear::default().icon(BARE_HANDS, &inv), id("Fist"));
        assert_eq!(Gear::default().badge(BARE_HANDS, &inv), -1);
    }

    /// The disc's WORDS.TXT has "UNUSED" at 159 and 160: the live words stand
    /// in there, and never over a word the table has (the control).
    #[test]
    fn the_live_words_fill_the_unused_ones() {
        let mut lines = vec!["x"; 158];
        lines.extend(["UNUSED", "UNUSED"]);
        lines.extend(vec!["y"; 272]);
        lines.push("Prepare");
        let t = vi::strtab::StringTable::parse(lines.join("\r\n").as_bytes()).unwrap();
        assert_eq!((t.get(159, 0), t.get(433, 0)), ("UNUSED", "Prepare"), "rows count from 1");
        assert_eq!(word(&t, 159, 0), "bare hands");
        assert_eq!(word(&t, 160, 0), "NAME CONFIGURATION");
        assert_eq!(word(&t, 433, 0), "Prepare", "a word the table has is its own");
        let mut live = lines.clone();
        live[158] = "bare hands (a server's own)";
        let t2 = vi::strtab::StringTable::parse(live.join("\n").as_bytes()).unwrap();
        assert_eq!(word(&t2, 159, 0), "bare hands (a server's own)", "a WORDS.TXT that has it");
        assert_eq!(Gear::default().name(BARE_HANDS, &t, 0), "bare hands");
        assert_eq!(gear(&[(5, -1)]).name(0, &t, 0), "set 0");
        assert_eq!(gear(&[(-1, -1)]).name(0, &t, 0), "", "an empty set has no name");
    }
}
