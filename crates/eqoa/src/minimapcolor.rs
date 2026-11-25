//! Minimap Colors (Extended > Minimap Colors; Heirloom petition 3092: "colour
//! the friendly NPCs on the minimap by role: spell merchants (optionally by
//! class), blacksmiths, general merchants, bankers, trainers, coachmen, quest
//! givers"): a friendly NPC's square on the minimap in its role's colour, chosen
//! from Number Colors' palette (`numcolor::PALETTE`); Default, its name's colour
//! as before. Roles, by what the client knows of an NPC (`role`):
//!
//! - Coachmen: the name, "Coachman ..." or "Coachwoman ..." (go-eqoa-server's
//!   `IsCoachman`);
//! - Bankers: the bank flag (CLIENT's `m_npcFlags` 2, which opens the bank), or
//!   a name "Banker ...";
//! - Spell Merchants: a merchant the server names as one
//!   (`net::session::OP_NPC_ROLES`: what he sells, and the class whose scrolls
//!   they mostly are), or a merchant with "Spell" in its name (an archivist); on
//!   a server that names none, the name alone. Spell Merchants Shown, My Class:
//!   only those of the player's class (or of none named), the others
//!   Merchants;
//! - Smiths: the blacksmith flag (8, the repair window), or a merchant whose
//!   name starts as a smith's does (Smithy, Blacksmith, Armorer, Weaponsmith,
//!   Shieldsmith, Bowyer ...);
//! - Trainers: a name starting as a guild master's or a trainer's
//!   (Guildmaster, Guild Master, Trainer, Training, Master, Rehabilitator);
//! - Quest Givers: an NPC with a quest mark over its head, from a server that
//!   sends them (`Session::head_mark`, the Quest Helper's Quest Markers);
//! - Merchants: any other with the merchant flag (1).
//!
//! The first that fits wins, in that order. A friendly NPC is one with the NPC
//! bit (`npc_flags` 0x80) and no owner (not a pet's or a player's colour).

use crate::numcolor::{Choice, paint};

/// The roles, in the page's order.
pub const ROLES: usize = 7;
pub const LABELS: [&str; ROLES] = ["Spell Merchants", "Smiths", "Merchants", "Bankers", "Trainers", "Coachmen", "Quest Givers"];
pub const KEYS: [&str; ROLES] = ["spells", "smiths", "merchants", "bankers", "trainers", "coachmen", "quest_givers"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Spells = 0,
    Smiths = 1,
    Merchants = 2,
    Bankers = 3,
    Trainers = 4,
    Coachmen = 5,
    QuestGivers = 6,
}

impl Role {
    /// Its line under the name in the minimap's tooltip.
    pub fn label(self) -> &'static str {
        ["Spell Merchant", "Smith", "Merchant", "Banker", "Trainer", "Coachman", "Quest Giver"][self as usize]
    }
}

/// The Extended items of the rows (300 on, not the next free: sibling lanes
/// take those), then Spell Merchants Shown's.
pub const ITEMS: [i32; ROLES] = [300, 301, 302, 303, 304, 305, 306];
pub const SPELLS_SHOWN_ITEM: i32 = 307;

/// The row of an Extended item, none for another's.
pub fn row_of_item(item: i32) -> Option<usize> {
    ITEMS.iter().position(|&i| i == item)
}

/// Each role's colour until chosen, every one its own and none the friendly
/// NPCs' tan: Purple, Orange, Yellow, Cyan, Pink, Light Blue, White
/// (`numcolor::PALETTE`'s places, from 1).
pub const DEFAULTS: [Choice; ROLES] = [6, 3, 2, 9, 5, 8, 1];

/// The class names, for the tooltip's "Spell Merchant (Wizard)".
const CLASSES: [&str; 15] = ["Warrior", "Ranger", "Paladin", "Shadowknight", "Monk", "Bard", "Rogue", "Druid", "Shaman", "Cleric", "Magician", "Necromancer", "Enchanter", "Wizard", "Alchemist"];

/// A class's name, none for 255 or past the list.
pub fn class_name(class: u8) -> Option<&'static str> {
    CLASSES.get(class as usize).copied()
}

/// The words a smith's name starts with.
const SMITHS: [&str; 9] = ["Smithy", "Smith", "Blacksmith", "Armorer", "Armourer", "Weaponsmith", "Shieldsmith", "Bowyer", "Fletcher"];
/// The words a trainer's or a guild master's name starts with.
const TRAINERS: [&str; 6] = ["Guildmaster", "Trainer", "Training", "Master", "Rehabilitator", "Guild"];

/// What the client knows of an NPC for its role.
pub struct Npc<'a> {
    pub name: &'a str,
    pub npc_flags: u16,
    /// its name colour's kind (`p256`): 0 or 1 an NPC's, 2 a player's, 3 a pet's
    pub p256: u8,
    /// the server named it a spell merchant, with the class it mostly sells
    /// for (255 none)
    pub spell_class: Option<u8>,
    /// a quest mark over its head
    pub quest_mark: bool,
}

/// An NPC's role, none for one with no role or not friendly; `mine_only` the
/// player's `class` for Spell Merchants Shown's My Class.
pub fn role(n: &Npc, mine_only: Option<u8>) -> Option<Role> {
    if n.npc_flags & 0x80 == 0 || n.p256 >= 2 {
        return None;
    }
    let first = n.name.split_whitespace().next().unwrap_or("");
    let merchant = n.npc_flags & 1 != 0;
    if n.name.starts_with("Coachman ") || n.name.starts_with("Coachwoman ") {
        return Some(Role::Coachmen);
    }
    if n.npc_flags & 2 != 0 || first == "Banker" {
        return Some(Role::Bankers);
    }
    let spell = n.spell_class.or_else(|| (merchant && n.name.contains("Spell")).then_some(255));
    if let Some(class) = spell {
        match mine_only {
            Some(mine) if class != 255 && class != mine => {}
            _ => return Some(Role::Spells),
        }
    }
    if n.npc_flags & 8 != 0 || (merchant && SMITHS.contains(&first)) {
        return Some(Role::Smiths);
    }
    if TRAINERS.contains(&first) || n.name.starts_with("Guild Master") {
        return Some(Role::Trainers);
    }
    if n.quest_mark {
        return Some(Role::QuestGivers);
    }
    merchant.then_some(Role::Merchants)
}

/// The colour a role's square is drawn in with the rows `choices`, none (its
/// name's colour) for Default.
pub fn rgba(role: Role, choices: &[Choice; ROLES]) -> Option<[f32; 4]> {
    paint(choices[role as usize]).map(|p| [p.rgb[0] as f32 / 255.0, p.rgb[1] as f32 / 255.0, p.rgb[2] as f32 / 255.0, 1.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn npc(name: &str, npc_flags: u16) -> Npc<'_> {
        Npc { name, npc_flags, p256: 0, spell_class: None, quest_mark: false }
    }

    /// Each role by what the client knows, the first that fits winning; a
    /// creature (no NPC bit), a player and a pet have none, and a friendly NPC
    /// with nothing to say for itself none.
    #[test]
    fn roles_by_what_the_client_knows() {
        let cases: [(Npc, Option<Role>); 16] = [
            (npc("Coachman Ronks", 0x180), Some(Role::Coachmen)),
            (npc("Coachwoman Darva", 0x180), Some(Role::Coachmen)),
            (npc("Banker Lars", 0x82), Some(Role::Bankers)),
            (npc("The Troupe's Treasurer", 0x82), Some(Role::Bankers)),
            (Npc { spell_class: Some(13), ..npc("Merchant Timmerin", 0x81) }, Some(Role::Spells)),
            (npc("Spell Archivist", 0x81), Some(Role::Spells)),
            (npc("Blacksmith Yiggins", 0x88), Some(Role::Smiths)),
            (npc("Armorer Wheats", 0x81), Some(Role::Smiths)),
            (npc("Bowyer Fillkner", 0x81), Some(Role::Smiths)),
            (npc("Guildmaster Ferris", 0x80), Some(Role::Trainers)),
            (npc("Rehabilitator Ama", 0x80), Some(Role::Trainers)),
            (Npc { quest_mark: true, ..npc("Dilana", 0x80) }, Some(Role::QuestGivers)),
            (npc("Merchant Perry", 0x81), Some(Role::Merchants)),
            (npc("Barmaid Terese", 0x80), None),
            (npc("orc pawn", 0), None),
            (Npc { p256: 3, ..npc("Merchant Pet", 0x81) }, None),
        ];
        for (n, want) in &cases {
            assert_eq!(role(n, None), *want, "{}", n.name);
        }
        // the control: the same merchant unnamed by the server is a merchant
        assert_eq!(role(&npc("Merchant Timmerin", 0x81), None), Some(Role::Merchants));
        // a coachman with the merchant flag is still a coachman; a quest giver
        // who sells is a quest giver before a merchant
        assert_eq!(role(&npc("Coachman Ronks", 0x81), None), Some(Role::Coachmen));
        assert_eq!(role(&Npc { quest_mark: true, ..npc("Merchant Risa", 0x81) }, None), Some(Role::QuestGivers));
    }

    /// Spell Merchants Shown, My Class: another class's spell merchant is a
    /// merchant; the player's own, and one of no class named, are spell
    /// merchants. All: every one.
    #[test]
    fn my_class_leaves_the_others_merchants() {
        let wiz = Npc { spell_class: Some(13), ..npc("Merchant Timmerin", 0x81) };
        let unknown = Npc { spell_class: Some(255), ..npc("Merchant Who", 0x81) };
        assert_eq!(role(&wiz, Some(13)), Some(Role::Spells));
        assert_eq!(role(&wiz, Some(12)), Some(Role::Merchants));
        assert_eq!(role(&unknown, Some(12)), Some(Role::Spells));
        assert_eq!(role(&wiz, None), Some(Role::Spells));
    }

    /// The defaults: every role coloured, each its own colour; Default draws
    /// the name's colour (none).
    #[test]
    fn every_role_its_own_colour() {
        let d = DEFAULTS;
        let all: Vec<_> = (0..ROLES).map(|i| paint(d[i]).expect("coloured by default").rgb).collect();
        for (i, c) in all.iter().enumerate() {
            assert_eq!(all.iter().filter(|x| *x == c).count(), 1, "{}'s colour its own", LABELS[i]);
        }
        assert_eq!(rgba(Role::Bankers, &d), Some([0.0, 240.0 / 255.0, 240.0 / 255.0, 1.0]));
        assert_eq!(rgba(Role::Bankers, &[0; ROLES]), None, "Default: the name's colour");
        assert_eq!(ITEMS.map(row_of_item), core::array::from_fn(Some));
        assert_eq!(row_of_item(SPELLS_SHOWN_ITEM), None);
        assert_eq!((class_name(13), class_name(255)), (Some("Wizard"), None));
    }
}
