//! The race and class tables (SUPPORT's `virace.cpp` and `viclass.cpp`,
//! decomp phase 61), as character select and creation read them: a race's
//! names, its two models, its select place in the arena (`m_selectX/Z`), its
//! music, the classes it may be; a class's names and the stats the creation
//! page highlights. `gRaces` is built at static-init time (its model words
//! are `VIHashResourceID` calls) and `s_classes` is initialised data: both
//! are the public decomp's constants, written out here as there.

/// `VIRaceDef`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RaceDef {
    /// `m_nameStrID`, `m_shortStrID`: WORDS.TXT rows (-1 an empty slot)
    pub name: i32,
    pub short: i32,
    /// `m_model[sex]`: the names `VIHashResourceID` hashes (empty: 0)
    pub model: [&'static str; 2],
    /// STR STA AGI DEX WIS INT CHA
    pub bonus: [i32; 7],
    /// bit N: class N allowed (-1 an empty slot)
    pub class_mask: i32,
    /// `m_selectX`, `m_selectZ`: where character select and creation stand
    /// the race in ARENA.CSF's world
    pub select: [f32; 2],
    /// `m_bgm`: the race's music (`PlayBgm(0, ...)`)
    pub bgm: &'static str,
    /// the expansion bits the race needs
    pub expansion: u32,
}

const EMPTY_RACE: RaceDef = RaceDef { name: -1, short: -1, model: ["", ""], bonus: [0; 7], class_mask: -1, select: [0.0, 0.0], bgm: "", expansion: 0 };

const fn race(name: i32, short: i32, model: [&'static str; 2], bonus: [i32; 7], class_mask: i32, select: [f32; 2], bgm: &'static str, expansion: u32) -> RaceDef {
    RaceDef { name, short, model, bonus, class_mask, select, bgm, expansion }
}

/// `gRaces[21]`.
pub const RACES: [RaceDef; 21] = [
    race(183, 317, ["HumanMale", "HumanFemale"], [5, 5, 5, 5, 5, 5, 5], 0x7EFF, [0.0, -26.0], "bgm\\qeynos.bgm", 0),
    race(185, 318, ["ElfMale", "ElfFemale"], [0, 0, 0, 10, 10, 5, 10], 0x76E6, [0.0, 0.0], "bgm\\elves.bgm", 0),
    race(186, 319, ["DarkElfMale", "DarkElfFemale"], [0, 5, 0, 10, 5, 15, 0], 0x7E49, [-13.0, 0.0], "bgm\\darkelve.bgm", 0),
    race(187, 320, ["GnomeMale", "GnomeFemale"], [0, 0, 10, 10, 0, 15, 0], 0x7E41, [13.0, 0.0], "bgm\\gnomes.bgm", 0),
    race(188, 321, ["DwarfMale", "DwarfFemale"], [10, 10, 0, 5, 10, 0, 0], 0x0245, [-13.0, -13.0], "bgm\\dwarves.bgm", 0),
    race(189, 322, ["TrollMale", "TrollFemale"], [20, 15, 0, 0, 0, 0, 0], 0x0109, [-13.0, 13.0], "bgm\\trolls.bgm", 0),
    race(191, 323, ["BarbarianMale", "BarbarianFemale"], [15, 10, 0, 5, 0, 5, 0], 0x0141, [13.0, -13.0], "bgm\\barbaria.bgm", 0),
    race(192, 324, ["HalflingMale", "HalflingFemale"], [0, 10, 10, 15, 0, 0, 0], 0x02C1, [0.0, -13.0], "bgm\\halfling.bgm", 0),
    race(193, 325, ["EruditeMale", "EruditeFemale"], [0, 0, 0, 5, 10, 20, 0], 0x7E0C, [0.0, 13.0], "bgm\\erudites.bgm", 0),
    race(190, 1191, ["OgreMale", "OgreFemale"], [20, 15, 0, 0, 0, 0, 0], 0x0909, [13.0, 13.0], "bgm\\ogre.bgm", 2),
    EMPTY_RACE,
    EMPTY_RACE,
    EMPTY_RACE,
    EMPTY_RACE,
    EMPTY_RACE,
    EMPTY_RACE,
    EMPTY_RACE,
    EMPTY_RACE,
    EMPTY_RACE,
    EMPTY_RACE,
    EMPTY_RACE,
];

/// `GetRace(r)`: out of range is race 0, never none.
pub fn get_race(r: i32) -> &'static RaceDef {
    &RACES[if (0..21).contains(&r) { r as usize } else { 0 }]
}

/// `m_model[sex]` as the game holds it: the name's `VIHashResourceID` (0 for
/// an empty slot's).
pub fn model_id(r: &RaceDef, sex: usize) -> u32 {
    match r.model[sex.min(1)] {
        "" => 0,
        n => vi::hash::hash_resource_id(n),
    }
}

/// `VIClassDef`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClassDef {
    /// `m_nameStrID` (165..179), `m_shortStrID`
    pub name: i32,
    pub short: i32,
    /// bit N: stat N highlighted on the creation page
    pub stat_mask: i32,
    /// STR STA AGI DEX WIS INT CHA
    pub bonus: [i32; 7],
    pub expansion: u32,
}

const fn class(name: i32, short: i32, stat_mask: i32, bonus: [i32; 7], expansion: u32) -> ClassDef {
    ClassDef { name, short, stat_mask, bonus, expansion }
}

/// `s_classes[15]`: WAR RAN PAL SK MNK BRD RGE DRD SHA CLR MAG NEC ENC WIZ ALC.
pub const CLASSES: [ClassDef; 15] = [
    class(165, 303, 3, [10, 15, 0, 0, -5, -10, -10], 0),
    class(166, 304, 12, [0, 0, 10, 15, -5, -10, -10], 0),
    class(167, 305, 3, [10, 15, 0, 0, -5, -10, -10], 0),
    class(168, 306, 3, [10, 15, 0, 0, -10, -5, -10], 0),
    class(169, 307, 10, [0, 10, 0, 15, -5, -10, -10], 0),
    class(170, 308, 6, [-5, 10, 15, 0, -10, -10, 0], 0),
    class(171, 309, 12, [0, 0, 10, 15, -10, -10, -5], 0),
    class(172, 310, 24, [0, -10, -10, 10, 15, -5, 0], 0),
    class(173, 311, 18, [-10, 10, 0, -5, 15, -10, 0], 0),
    class(174, 312, 80, [0, 0, -5, -10, 15, -10, 10], 0),
    class(175, 313, 36, [-10, 0, 10, -5, 0, 15, -10], 0),
    class(176, 314, 34, [0, 10, -10, 0, -5, 15, -10], 0),
    class(177, 315, 96, [-10, -10, 0, 0, -5, 15, 10], 0),
    class(178, 316, 40, [0, -5, 0, 10, -10, 15, -10], 0),
    class(179, 1190, 48, [-10, 0, -5, 0, 10, 15, -10], 2),
];

/// `GetClass(c)`: out of range is class 0.
pub fn get_class(c: i32) -> &'static ClassDef {
    &CLASSES[if (0..15).contains(&c) { c as usize } else { 0 }]
}

/// `IsStatHighlighted(c, stat)`.
pub fn is_stat_highlighted(c: i32, stat: i32) -> bool {
    (0..7).contains(&stat) && (0..15).contains(&c) && (CLASSES[c as usize].stat_mask >> stat) & 1 != 0
}

/// `IsValidRaceClassCityCombo(race, c, city)`: the city is a human's only
/// (0, and the menu's East 1 and West 2, each with its own classes). None
/// where the game returns an uninitialised word: a human with a city outside
/// 0..2.
pub fn is_valid_race_class_city_combo(race: i32, c: i32, city: i32) -> Option<bool> {
    if !(0..21).contains(&race) || !(0..15).contains(&c) {
        return Some(false);
    }
    let mask = match (race, city) {
        (0, 0) => RACES[0].class_mask,
        (0, 1) => 0x7E69,
        (0, 2) => 0x76F7,
        (0, _) => return None,
        _ => RACES[race as usize].class_mask,
    };
    Some((mask >> c) & 1 != 0)
}

/// `IsValidRaceClassCityComboExpansion`: the combination, and the race's and
/// the class's expansion bits all owned.
pub fn is_valid_combo_expansion(race: i32, c: i32, city: i32, owned: u32) -> bool {
    if is_valid_race_class_city_combo(race, c, city) != Some(true) {
        return false;
    }
    let need = get_class(c).expansion | get_race(race).expansion;
    owned & need == need
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The decomp's source next to this repository (`EQOA_DECOMP`, else
    /// `../decomp` from the repository's root), if it is there.
    fn decomp_file(name: &str) -> Option<String> {
        let root = std::env::var("EQOA_DECOMP").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../decomp"));
        std::fs::read_to_string(root.join("src").join(name)).ok()
    }

    /// A C initialiser's tokens in order: numbers (as f64), strings, and
    /// `EMPTY_RACE` as itself; comments and punctuation dropped.
    fn tokens(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut it = src.char_indices().peekable();
        let b = src.as_bytes();
        while let Some((i, c)) = it.next() {
            if c == '/' && b.get(i + 1) == Some(&b'/') {
                while it.peek().is_some_and(|&(_, c)| c != '\n') {
                    it.next();
                }
            } else if c == '"' {
                let mut s = String::new();
                while let Some((_, c)) = it.next() {
                    if c == '\\' {
                        if let Some((_, n)) = it.next() {
                            s.push(n);
                        }
                    } else if c == '"' {
                        break;
                    } else {
                        s.push(c);
                    }
                }
                out.push(format!("\"{s}\""));
            } else if c.is_ascii_digit() || (c == '-' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
                let mut s = c.to_string();
                while let Some(&(_, n)) = it.peek() {
                    if n.is_ascii_alphanumeric() || n == '.' {
                        s.push(n);
                        it.next();
                    } else {
                        break;
                    }
                }
                let s = s.trim_end_matches('f');
                let v = if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) { i64::from_str_radix(h, 16).unwrap() as f64 } else { s.parse::<f64>().unwrap() };
                out.push(format!("{v}"));
            } else if c.is_ascii_alphabetic() || c == '_' {
                let mut s = c.to_string();
                while let Some(&(_, n)) = it.peek() {
                    if n.is_ascii_alphanumeric() || n == '_' {
                        s.push(n);
                        it.next();
                    } else {
                        break;
                    }
                }
                if s == "EMPTY_RACE" {
                    out.push(s);
                }
            }
        }
        out
    }

    fn race_tokens(r: &RaceDef) -> Vec<String> {
        if *r == EMPTY_RACE {
            return vec!["EMPTY_RACE".into()];
        }
        let mut v = vec![format!("{}", r.name as f64), format!("{}", r.short as f64), format!("\"{}\"", r.model[0]), format!("\"{}\"", r.model[1])];
        v.extend(r.bonus.iter().map(|b| format!("{}", *b as f64)));
        v.push(format!("{}", r.class_mask as f64));
        v.extend(r.select.iter().map(|s| format!("{}", *s as f64)));
        v.push(format!("\"{}\"", r.bgm));
        v.push(format!("{}", r.expansion as f64));
        v
    }

    fn class_tokens(c: &ClassDef) -> Vec<String> {
        let mut v = vec![format!("{}", c.name as f64), format!("{}", c.short as f64), format!("{}", c.stat_mask as f64)];
        v.extend(c.bonus.iter().map(|b| format!("{}", *b as f64)));
        v.push(format!("{}", c.expansion as f64));
        v
    }

    /// The tables against the decomp's own initialisers, token for token
    /// (the race's, `gRaces[21] = { ... }`; the class's, `s_classes[15]`).
    #[test]
    fn the_tables_are_the_decomps() {
        let (Some(r), Some(c)) = (decomp_file("virace.cpp"), decomp_file("viclass.cpp")) else {
            eprintln!("skipped: the decomp's virace.cpp/viclass.cpp are not beside the repository (EQOA_DECOMP)");
            return;
        };
        let body = |src: &str, head: &str| {
            let at = src.find(head).unwrap();
            let open = at + src[at..].find('{').unwrap();
            let close = open + src[open..].find("};").unwrap();
            src[open + 1..close].to_string()
        };
        let want = tokens(&body(&r, "gRaces[21] ="));
        let ours: Vec<String> = RACES.iter().flat_map(race_tokens).collect();
        assert_eq!(ours, want, "gRaces");
        let want = tokens(&body(&c, "s_classes[15] ="));
        let ours: Vec<String> = CLASSES.iter().flat_map(class_tokens).collect();
        assert_eq!(ours, want, "s_classes");
        // the control: one select place moved is seen
        let mut moved = RACES;
        moved[4].select[1] = -12.0;
        let ours: Vec<String> = moved.iter().flat_map(race_tokens).collect();
        assert_ne!(ours, tokens(&body(&r, "gRaces[21] =")), "a changed table must not pass");
    }

    #[test]
    fn out_of_range_is_entry_zero_and_the_human_cities_have_their_classes() {
        assert_eq!(get_race(21).name, 183);
        assert_eq!(get_race(-1).name, 183);
        assert_eq!(get_class(15).name, 165);
        // East (1) has no Ranger (bit 1), West (2) has one; the race's own mask for city 0
        assert_eq!(is_valid_race_class_city_combo(0, 1, 1), Some(false));
        assert_eq!(is_valid_race_class_city_combo(0, 1, 2), Some(true));
        assert_eq!(is_valid_race_class_city_combo(0, 1, 0), Some(true));
        assert_eq!(is_valid_race_class_city_combo(0, 0, 3), None, "shipped: uninitialised");
        // the Alchemist needs expansion bit 1 (2), the Ogre too
        assert!(!is_valid_combo_expansion(4, 14, 0, 1) && is_valid_combo_expansion(2, 14, 0, 3));
        assert!(!is_valid_combo_expansion(9, 0, 0, 1) && is_valid_combo_expansion(9, 0, 0, 2));
        // a class past the 15 is never valid; an empty race slot allows nothing it does not ask for
        assert_eq!(is_valid_race_class_city_combo(2, 15, 0), Some(false));
        assert!(is_stat_highlighted(0, 0) && is_stat_highlighted(0, 1) && !is_stat_highlighted(0, 2) && !is_stat_highlighted(0, 7));
        assert_eq!(model_id(get_race(0), 0), vi::hash::hash_resource_id("HumanMale"));
        assert_eq!(model_id(&RACES[12], 1), 0);
    }
}
