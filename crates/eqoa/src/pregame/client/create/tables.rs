//! The tables character creation reads: SUPPORT's race and class
//! descriptors (`gRaces`, `s_classes`: `../races.rs`, re-exported here),
//! their readers (`IsValidRaceClassCityCombo(Expansion)`, `IsStatHighlighted`),
//! the pages' own lists (the race page's `sRaces`, the class page's `sClasses`
//! and `sStats`, the review's `sHomes`), and what CLIENT keeps as data: the
//! races' default looks (`gRaceInfo`/`gRaceInfoEx`, `GetRaceInfo`), read at run
//! time out of CLIENT's image (`client_image`). The race cameras' creation
//! framings are `camstyle`'s, as play's and character select's are.

use crate::client_image::ClientImage;
use net::creation::Creation;
use std::cell::RefCell;
use std::rc::Rc;

pub use super::super::races::{get_class, get_race, CLASSES, RACES};

thread_local! {
    /// What the server said creation takes past the game's own
    /// (`net::creation`, the session's `creation`): empty from a server that
    /// never offers it, so every table below is the game's alone.
    static OFFER: RefCell<Rc<Creation>> = RefCell::new(Rc::new(Creation::default()));
}

/// Makes `c` what the tables read (the pregame, from its session's; None: the
/// game's alone). True when it changed.
pub fn set_offer(c: Option<&Creation>) -> bool {
    OFFER.with(|o| {
        let mut o = o.borrow_mut();
        let new = c.cloned().unwrap_or_default();
        if **o == new {
            return false;
        }
        *o = Rc::new(new);
        true
    })
}

/// What the server said creation takes, as the tables read it.
pub fn offer() -> Rc<Creation> {
    OFFER.with(|o| o.borrow().clone())
}

/// The server's race of this id, if it told of one.
pub fn server_race(race: i32) -> Option<net::creation::Race> {
    offer().race(race).cloned()
}

/// The game's race whose stage place, cameras, music and home a race takes:
/// a server's race's own (`like`), else the race itself.
pub fn like(race: i32) -> i32 {
    server_race(race).map_or(race, |r| r.like)
}

/// A race's bonuses (STR STA AGI DEX WIS INT CHA): a server's race's, else
/// gRaces'.
pub fn bonus(race: i32) -> [i32; 7] {
    server_race(race).map_or(get_race(race).bonus, |r| r.bonus)
}

/// A race's name, wide: a server's race's own words, else WORDS.TXT's.
pub fn race_name(vui: &vi::ui::Vui, race: i32) -> Vec<u16> {
    match server_race(race) {
        Some(r) => vi::ui::wide(&r.name),
        None => vi::ui::wide(vui.words.get(get_race(race).name, vui.language)),
    }
}

/// A race page row's description, wide: a server's race's own, else its word.
pub fn race_desc(vui: &vi::ui::Vui, row: &RaceRow) -> Vec<u16> {
    match server_race(row.race).filter(|_| row.desc_word < 0) {
        Some(r) => vi::ui::wide(&r.desc),
        None => vi::ui::wide(vui.words.get(row.desc_word, vui.language)),
    }
}

/// The body a race's character of `sex` wears: a server's race's (CHAR.ESF),
/// else gRaces' model.
pub fn model_of(race: i32, sex: i32) -> u32 {
    server_race(race).map_or_else(|| get_race(race).model_id(sex), |r| r.body_for(sex))
}

/// Every body of the server's races (male and female), and every body their
/// options offer (`raceopts`), for the pregame to read (`res::Res::want_bodies`:
/// CHARSEL's files hold the game's races alone).
pub fn bodies() -> Vec<u32> {
    let mut out: Vec<u32> = offer().races.iter().flat_map(|r| r.body).filter(|&b| b != 0).collect();
    if let Some(o) = crate::raceopts::options() {
        out.extend(o.races.iter().flat_map(|(_, rows)| rows).filter(|r| r.kind == net::raceopts::Kind::Body).flat_map(|r| r.male.iter().chain(&r.female)).map(|s| s.value).filter(|&b| b != 0));
        // a texture row of one step, laid always, names a body (the Wisp's wisp:
        // `wisp`), which the pregame must have to draw it
        let always = |r: &&net::raceopts::Row| r.kind == net::raceopts::Kind::Texture && r.male.len() == 1 && r.female.len() <= 1;
        out.extend(o.races.iter().flat_map(|(_, rows)| rows).filter(always).flat_map(|r| r.male.iter().chain(&r.female)).map(|s| s.value).filter(|&b| b != 0));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// The game's own body for `model` where it is a server's race's (its `like`
/// race's of the body's sex), or another body of one its options offer: the
/// stand-in a screen whose library lacks the body draws. None for any other
/// model.
pub fn stand_in(model: u32) -> Option<u32> {
    let o = offer();
    if let Some(r) = o.races.iter().find(|r| r.body[0] == model || r.body[1] == model) {
        let sex = if r.body[1] == model && r.body[0] != model { 1 } else { 0 };
        return Some(get_race(r.like).model_id(sex));
    }
    // a body a race's options offer: its race's like race's, of the sex whose
    // steps hold it
    let opts = crate::raceopts::options()?;
    opts.races.iter().find_map(|(id, rows)| {
        let r = o.race(*id)?;
        let row = rows.iter().find(|row| row.kind == net::raceopts::Kind::Body && row.male.iter().chain(&row.female).any(|s| s.value == model))?;
        let sex = if row.male.iter().any(|s| s.value == model) { 0 } else { 1 };
        Some(get_race(r.like).model_id(sex))
    })
}

/// The body a form's character wears and the look its parts take: its race's
/// options' steps as the form holds them (`raceopts`), else its race's own body
/// and no look.
pub fn chosen(form: &net::session::NewChar) -> (u32, Option<net::raceopts::Look>) {
    let own = model_of(form.race, form.sex);
    let Some(o) = crate::raceopts::options() else { return (own, None) };
    let rows = o.rows(form.race);
    if rows.is_empty() {
        return (own, None);
    }
    let steps: Vec<usize> = rows.iter().map(|r| form.options.iter().find(|(k, _)| *k == r.key).map_or(0, |(_, s)| *s as usize)).collect();
    let model = crate::raceopts::body_of_steps(rows, form.sex, &steps).unwrap_or(own);
    (model, Some(crate::raceopts::look_of_steps(rows, form.sex, &steps, model)))
}

/// `IsStatHighlighted`.
pub fn stat_highlighted(c: i32, stat: i32) -> bool {
    (0..7).contains(&stat) && (0..15).contains(&c) && (CLASSES[c as usize].stat_mask >> stat) & 1 != 0
}

/// `IsValidRaceClassCityCombo`: a human's city picks its own mask (East 1,
/// West 2). A city outside 0..2 for a human returns the game's
/// uninitialised `ok`; here it is refused. A server's race (`offer`) has its
/// own classes, and where the server opens every class to every race any race
/// it or the game has takes any class.
pub fn valid_combo(race: i32, c: i32, city: i32) -> bool {
    if !(0..15).contains(&c) {
        return false;
    }
    // a server's race by its own number, whatever it is (go-eqoa-server's go
    // to 30): the game's table stops at 20, and Orc (21) to Gargoyle (29) had
    // no class to take
    let o = offer();
    if let Some(r) = o.race(race) {
        return o.all_classes || r.has_class(c);
    }
    if !(0..21).contains(&race) {
        return false;
    }
    if o.all_classes && (0..10).contains(&race) && (race != 0 || (1..=2).contains(&city)) {
        return true;
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
    let need = get_class(c).expansion | get_race(like(race)).expansion;
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

/// The game's rows' names, in `RACE_ROWS`' order: where a server's race goes
/// among them (the list is alphabetical).
const RACE_ROW_NAMES: [&str; 11] = ["Barbarian", "Dark Elf", "Dwarf", "Elf", "Erudite", "Gnome", "Halfling", "Human", "Human", "Ogre", "Troll"];

/// `VIWndCharCreateRace::BuildList`: the races the owned expansions allow
/// (a race of no expansion always), and each race the server has past the
/// game's (`offer`) in its alphabetical place, as its `like` race's
/// expansion allows (a row with no description word: its own text).
pub fn race_list(owned: u32) -> Vec<RaceRow> {
    let allowed = |race: i32| owned & get_race(race).expansion != 0 || get_race(race).expansion == 0;
    let mut out: Vec<(String, RaceRow)> = RACE_ROWS.iter().zip(RACE_ROW_NAMES).filter(|(r, _)| allowed(r.race)).map(|(r, n)| (n.to_ascii_lowercase(), *r)).collect();
    let o = offer();
    let mut extra: Vec<&net::creation::Race> = o.races.iter().filter(|r| allowed(r.like) && !(0..10).contains(&r.id)).collect();
    extra.sort_by_key(|r| r.name.to_ascii_lowercase());
    for r in extra {
        let key = r.name.to_ascii_lowercase();
        let at = out.iter().position(|(n, _)| *n > key).unwrap_or(out.len());
        out.insert(at, (key, RaceRow { race: r.id, city: 0, city_word: -1, desc_word: -1 }));
    }
    out.into_iter().map(|(_, r)| r).collect()
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
pub(crate) mod tests {

    /// The pregame reads the body a texture row of one step names (the Wisp's
    /// wisp) beside the options' bodies; with no options (the control), neither.
    #[test]
    fn the_pregame_reads_the_wisps_body() {
        use net::raceopts::{Kind, Options, Part, Row, Step};
        let step = |v: u32| Step { name: "s".into(), value: v, size: 1000 };
        let rows = vec![
            Row { key: "body".into(), label: "Body".into(), kind: Kind::Body, parts: vec![], male: vec![step(0x70D8_98C6)], female: vec![step(0x0D53_7335)] },
            Row { key: "wisp".into(), label: "Wisp".into(), kind: Kind::Texture, parts: vec![(0x70D8_98C6, Part { entries: vec![2], rects: vec![] })], male: vec![Step { size: 0, ..step(0x8143_DC70) }], female: vec![] },
            Row { key: "hair".into(), label: "Hair".into(), kind: Kind::Texture, parts: vec![], male: vec![step(0), step(0x98F8_6489)], female: vec![] },
        ];
        crate::raceopts::set_options(None);
        assert!(!bodies().contains(&0x8143_DC70), "the control: no options");
        crate::raceopts::set_options(Some(&Options { races: vec![(31, rows)] }));
        let b = bodies();
        assert!(b.contains(&0x8143_DC70) && b.contains(&0x70D8_98C6) && b.contains(&0x0D53_7335));
        assert!(!b.contains(&0x98F8_6489), "a hair row's textures are no bodies");
        crate::raceopts::set_options(None);
    }
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

    /// The races a server tells of (go-eqoa-server's `CreationRaces`): the
    /// froglok (a human of the east's), a hill giant (an ogre's), a kerran (a
    /// western human's, its own classes and description), a goblin (a dark elf's).
    pub(crate) fn servers_offer(all_classes: bool) -> Creation {
        let race = |id: i32, like: i32, city: i32, name: &str, mask: i32, body: [u32; 2], bonus: [i32; 7]| net::creation::Race {
            id,
            like,
            city,
            bonus,
            class_mask: mask,
            body,
            name: name.into(),
            short: name[..3].to_ascii_uppercase(),
            desc: format!("{name} folk."),
        };
        Creation {
            all_classes,
            races: vec![
                race(10, 0, 1, "Froglok", 0x7E69, [0x141B_FE0C; 2], [5; 7]),
                race(11, 9, 0, "Hill Giant", 0x0909, [0xFFB1_EA2C; 2], [25, 20, -5, -5, 0, 0, 0]),
                race(12, 0, 2, "Kerran", 0x0003, [0x2217_6258, 0xB749_BF57], [5, 5, 15, 10, 0, 0, 0]),
                race(13, 2, 0, "Goblin", 0x7E49, [0x6C13_08E0, 0], [0, 5, 10, 15, 0, 5, 0]),
            ],
        }
    }

    /// A server's race past the game's twenty takes its classes from the offer as
    /// any server race does (every class where the server opens them all, else
    /// its own mask); the control: with no offer, race 21 is refused, as before.
    #[test]
    fn a_server_race_past_twenty_has_classes() {
        let mut o = servers_offer(true);
        let mut far = o.races[2].clone();
        far.id = 29;
        far.class_mask = 0x0005;
        let mut orc = o.races[1].clone();
        orc.id = 21;
        o.races.push(far);
        o.races.push(orc);
        set_offer(Some(&o));
        assert_eq!((0..15).filter(|&c| valid_combo(29, c, 0)).count(), 15, "every class, the server opens them all");
        assert_eq!((0..15).filter(|&c| valid_combo(21, c, 0)).count(), 15);
        o.all_classes = false;
        set_offer(Some(&o));
        assert_eq!((0..15).filter(|&c| valid_combo(29, c, 0)).collect::<Vec<_>>(), vec![0, 2], "its own mask");
        set_offer(None);
        assert!(!valid_combo(21, 0, 0) && !valid_combo(29, 0, 0), "with no offer, past twenty is no race");
    }

    /// A server's races take their alphabetical places in the race page's list
    /// (the froglok among them, after Erudite), as their `like` race's
    /// expansion allows; the control: with no offer the list is the game's.
    #[test]
    fn a_servers_races_join_the_list_in_their_places() {
        set_offer(Some(&servers_offer(false)));
        let ids: Vec<i32> = race_list(3).iter().map(|r| r.race).collect();
        assert_eq!(ids, vec![6, 2, 4, 1, 8, 10, 3, 13, 7, 11, 0, 0, 12, 9, 5]);
        assert!(race_list(3).iter().filter(|r| r.race >= 10).all(|r| r.desc_word == -1 && r.city == 0));
        // the hill giant passes as an ogre, which needs Frontiers
        assert!(!race_list(1).iter().any(|r| r.race == 11 || r.race == 9));
        assert!(!set_offer(Some(&servers_offer(false))), "the same offer again changes nothing");
        assert!(set_offer(None));
        assert_eq!(race_list(3).len(), 11, "the control: the game's alone");
    }

    /// A server's race has its own classes, bonuses, body and stage race; with
    /// every class open any race the server or the game has takes any class (a
    /// human by its city); the control: with no offer the game's masks.
    #[test]
    fn a_servers_races_classes_bonuses_and_bodies() {
        set_offer(Some(&servers_offer(false)));
        assert!(valid_combo(12, 1, 0) && !valid_combo(12, 2, 0), "the kerran's own: warrior, ranger");
        assert!(valid_combo(11, 3, 0) && !valid_combo(11, 13, 0), "the giant's: the ogre's");
        assert!(!valid_combo(5, 13, 0), "a troll wizard: not without every class");
        assert_eq!((bonus(11), bonus(6)), ([25, 20, -5, -5, 0, 0, 0], [15, 10, 0, 5, 0, 5, 0]));
        assert_eq!((like(11), like(10), like(6)), (9, 0, 6));
        assert_eq!((model_of(12, 1), model_of(13, 1), model_of(0, 0)), (0xB749_BF57, 0x6C13_08E0, get_race(0).model_id(0)));
        assert_eq!(stand_in(0xB749_BF57), Some(get_race(0).model_id(1)), "the kerran woman: a human woman");
        assert_eq!(stand_in(0xFFB1_EA2C), Some(get_race(9).model_id(0)), "the giant: an ogre");
        assert_eq!(stand_in(get_race(0).model_id(0)), None, "the game's own body: no stand-in");
        assert!(valid_combo_expansion(11, 0, 0, 3) && !valid_combo_expansion(11, 0, 0, 1), "the giant needs the ogre's Frontiers");
        set_offer(Some(&servers_offer(true)));
        assert!(valid_combo(5, 13, 0) && valid_combo(11, 13, 0) && valid_combo(12, 2, 0) && valid_combo(0, 8, 1) && valid_combo(0, 3, 2));
        assert!(!valid_combo(5, 15, 0) && !valid_combo(0, 8, 3), "no such class or city");
        // an empty slot of gRaces takes every class, as the game's own check does
        // (its mask is -1); the race page never lists one
        assert!(valid_combo(14, 0, 0) && !race_list(3).iter().any(|r| r.race == 14));
        set_offer(None);
        assert!(!valid_combo(5, 13, 0) && valid_combo(5, 8, 0), "the control: the game's masks");
        assert_eq!(bonus(11), [0; 7], "no server: an empty slot");
    }

    #[test]
    fn the_models_are_the_races_hashes() {
        assert_eq!(get_race(0).model_id(0), vi::hash::hash_resource_id("HumanMale"));
        assert_eq!(get_race(9).model_id(1), vi::hash::hash_resource_id("OgreFemale"));
        assert_eq!(get_race(99).name, 183, "out of range is the human");
        assert_eq!(sex_word(1), 164);
    }
}
