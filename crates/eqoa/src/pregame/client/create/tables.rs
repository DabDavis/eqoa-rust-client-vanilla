//! The tables character creation reads: SUPPORT's race and class
//! descriptors (`gRaces`, `s_classes`: `../races.rs`, re-exported here),
//! their readers (`IsValidRaceClassCityCombo(Expansion)`, `IsStatHighlighted`),
//! the pages' own lists (the race page's `sRaces`, the class page's `sClasses`
//! and `sStats`, the review's `sHomes`), and what CLIENT keeps as data: the
//! races' default looks (`gRaceInfo`/`gRaceInfoEx`, `GetRaceInfo`), read at run
//! time out of CLIENT's image (`client_image`). The race cameras' creation
//! framings are `camstyle`'s, as play's and character select's are.

use crate::client_image::ClientImage;

pub use super::super::races::{get_class, get_race, CLASSES, RACES};

/// `IsStatHighlighted`.
pub fn stat_highlighted(c: i32, stat: i32) -> bool {
    (0..7).contains(&stat) && (0..15).contains(&c) && (CLASSES[c as usize].stat_mask >> stat) & 1 != 0
}

/// `IsValidRaceClassCityCombo`: a human's city picks its own mask (East 1,
/// West 2). A city outside 0..2 for a human returns the game's
/// uninitialised `ok`; here it is refused.
pub fn valid_combo(race: i32, c: i32, city: i32) -> bool {
    if !(0..21).contains(&race) || !(0..15).contains(&c) {
        return false;
    }
    let mask = if race == 0 {
        match city {
            0 => RACES[0].class_mask,
            1 => 0x7E69,
            2 => 0x76F7,
            _ => return false,
        }
    } else {
        RACES[race as usize].class_mask
    };
    (mask >> c) & 1 != 0
}

/// `IsValidRaceClassCityComboExpansion`: and the class's and the race's
/// expansions owned.
pub fn valid_combo_expansion(race: i32, c: i32, city: i32, owned: u32) -> bool {
    if !valid_combo(race, c, city) {
        return false;
    }
    let need = get_class(c).expansion | get_race(race).expansion;
    owned & need == need
}

/// The race page's list (`sRaces`, 0x00CE3E98): a race, its city (the
/// humans split by city), the city's word and the description's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaceRow {
    pub race: i32,
    pub city: i32,
    pub city_word: i32,
    pub desc_word: i32,
}

pub const RACE_ROWS: [RaceRow; 11] = [
    RaceRow { race: 6, city: 0, city_word: -1, desc_word: 373 },
    RaceRow { race: 2, city: 0, city_word: -1, desc_word: 369 },
    RaceRow { race: 4, city: 0, city_word: -1, desc_word: 371 },
    RaceRow { race: 1, city: 0, city_word: -1, desc_word: 368 },
    RaceRow { race: 8, city: 0, city_word: -1, desc_word: 375 },
    RaceRow { race: 3, city: 0, city_word: -1, desc_word: 370 },
    RaceRow { race: 7, city: 0, city_word: -1, desc_word: 374 },
    RaceRow { race: 0, city: 1, city_word: 459, desc_word: 455 },
    RaceRow { race: 0, city: 2, city_word: 460, desc_word: 454 },
    RaceRow { race: 9, city: 0, city_word: -1, desc_word: 376 },
    RaceRow { race: 5, city: 0, city_word: -1, desc_word: 372 },
];

/// `VIWndCharCreateRace::BuildList`: the races the owned expansions allow
/// (a race of no expansion always).
pub fn race_list(owned: u32) -> Vec<RaceRow> {
    RACE_ROWS.iter().copied().filter(|r| owned & get_race(r.race).expansion != 0 || get_race(r.race).expansion == 0).collect()
}

/// The class page's list (`sClasses`, 0x00CE3490): a class and its
/// description's word, alphabetical.
pub const CLASS_ROWS: [(i32, i32); 15] = [(14, 394), (5, 385), (9, 389), (7, 387), (12, 392), (10, 390), (4, 384), (11, 391), (2, 382), (1, 381), (6, 386), (3, 383), (8, 388), (0, 380), (13, 393)];

/// `VIWndCharCreateClass::BuildList`: the classes whose expansion is all
/// owned.
pub fn class_list(owned: u32) -> Vec<(i32, i32)> {
    CLASS_ROWS.iter().copied().filter(|&(c, _)| owned & get_class(c).expansion == get_class(c).expansion).collect()
}

/// `sStats` (0x00CE3508): each stat with its name's and description's words.
pub const STAT_ROWS: [(usize, i32, i32); 7] = [(0, 248, 202), (1, 249, 203), (2, 250, 204), (3, 251, 205), (4, 252, 206), (5, 253, 207), (6, 254, 208)];

/// `sHomes` (0x00CE4270): each race's home city word, the humans' by city.
pub const HOMES: [(i32, i32, i32); 11] = [(6, 0, 364), (8, 0, 366), (2, 0, 360), (4, 0, 608), (1, 0, 1125), (3, 0, 361), (7, 0, 365), (0, 1, 358), (0, 2, 357), (5, 0, 363), (9, 0, 1187)];

/// `GetSexWord`: 163 Male, 164 Female.
pub fn sex_word(sex: i32) -> i32 {
    if sex == 1 { 164 } else { 163 }
}

/// `VIRaceInfo`: a race's default face, hair style, length and colour, by
/// sex.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RaceInfo {
    pub male: [i32; 4],
    pub female: [i32; 4],
}

const RACE_INFO: [u32; 2] = [0x00CD_E428, 0x00CD_E720];

/// `GetRaceInfo(race, expansion)`: out of range the plain table's entry 0,
/// else the expansion's table (bit 1).
pub fn race_info(img: &ClientImage, race: i32, expansion: u32) -> RaceInfo {
    let r = if (0..21).contains(&race) { RACE_INFO[(expansion & 2 != 0) as usize] + 36 * race as u32 } else { RACE_INFO[0] };
    let w = |o: u32| img.i32_at(r + o);
    RaceInfo { male: [w(4), w(8), w(12), w(16)], female: [w(20), w(24), w(28), w(32)] }
}

/// `CapitalizeName` (`vicharacter.cpp`): upper-case the name, then
/// lower-case all but its first letter.
pub fn capitalize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for (i, c) in name.chars().enumerate() {
        out.push(if i == 0 { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_race_list_keeps_its_display_order_and_the_human_cities() {
        let all = race_list(3);
        let names: Vec<(i32, i32)> = all.iter().map(|r| (r.race, r.city)).collect();
        assert_eq!(names, vec![(6, 0), (2, 0), (4, 0), (1, 0), (8, 0), (3, 0), (7, 0), (0, 1), (0, 2), (9, 0), (5, 0)]);
        // the ogre needs Frontiers (expansion bit 1): the control
        let old = race_list(1);
        assert_eq!(old.len(), 10);
        assert!(old.iter().all(|r| r.race != 9));
    }

    #[test]
    fn the_class_masks_and_the_human_cities() {
        // a barbarian may be a warrior, rogue or shaman
        let ok: Vec<i32> = (0..15).filter(|&c| valid_combo(6, c, 0)).collect();
        assert_eq!(ok, vec![0, 6, 8]);
        // the eastern humans' mask is 0x7E69, the western 0x76F7
        assert!(valid_combo(0, 3, 1) && !valid_combo(0, 3, 2), "a shadowknight is eastern");
        assert!(valid_combo(0, 1, 2) && !valid_combo(0, 1, 1), "a ranger is western");
        // the alchemist needs the expansion: the control
        assert!(valid_combo(0, 14, 1) && !valid_combo_expansion(0, 14, 1, 1) && valid_combo_expansion(0, 14, 1, 3));
        assert!(!valid_combo(0, 0, 3), "no fourth city");
        assert!(!valid_combo(21, 0, 0) && !valid_combo(0, 15, 0));
        // the alchemist leaves the old game's class list
        assert_eq!(class_list(1).len(), 14);
        assert_eq!(class_list(3).len(), 15);
    }

    #[test]
    fn the_stats_are_the_base_the_race_and_the_class() {
        // a barbarian rogue: the reference picture's 75 70 70 80 50 55 55
        let (r, c) = (get_race(6), get_class(6));
        let shown: Vec<i32> = (0..7).map(|s| 60 + r.bonus[s] + c.bonus[s]).collect();
        assert_eq!(shown, vec![75, 70, 70, 80, 50, 55, 55]);
        // every race's bonuses sum to 35
        assert!(RACES.iter().take(10).all(|r| r.bonus.iter().sum::<i32>() == 35));
        assert!(stat_highlighted(0, 0) && stat_highlighted(0, 1) && !stat_highlighted(0, 2));
        assert!(!stat_highlighted(0, 7) && !stat_highlighted(15, 0));
    }

    #[test]
    fn a_name_is_capitalised() {
        assert_eq!(capitalize_name("aRWEN"), "Arwen");
        assert_eq!(capitalize_name("x"), "X");
        assert_ne!(capitalize_name("arwen"), "arwen", "the control");
    }

    #[test]
    fn the_models_are_the_races_hashes() {
        assert_eq!(get_race(0).model_id(0), vi::hash::hash_resource_id("HumanMale"));
        assert_eq!(get_race(9).model_id(1), vi::hash::hash_resource_id("OgreFemale"));
        assert_eq!(get_race(99).name, 183, "out of range is the human");
        assert_eq!(sex_word(1), 164);
    }
}
