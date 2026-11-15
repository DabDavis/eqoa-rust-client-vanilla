//! A merc's inventory window, the player's own item menu over what the merc
//! holds (the server owner's of 2026-11-15: "the merc inventory should pop up
//! like player inventory menu also show the merc equipped gear. players can
//! replace it and/or give them new equipment like a player"; go-eqoa-server's
//! 0x7E72-0x7E75, `net::mercwin`). The window is `itemmenu::ItemMenu` with
//! three tabs of its own in place of the game's four: Worn (what the merc
//! wears, its own and, where it wears nothing of its own, its heirlooms),
//! Carried (what it holds), Your Pack (what the player may give it); the
//! description beside the list is the merc's name, level, hit points, power,
//! AC and stats over the selected item's. Each item's popup is the game's
//! (Equip, Unequip, Inspect) with Take Back and Give, vanilla's own. Every move
//! is asked of the server, which judges it by the player's rules and answers
//! with the merc's new state; nothing here changes what it holds. The
//! player's own inventory has Give to the merc on each item not worn while
//! the server offers the windows and the player's merc is out
//! (`with_give`). A child of nothing: `play::mercplay` ties it to the session.

use crate::client_image::{Menu, MenuEntry, Menus};
use crate::infotext;
use net::dump::{ItemList, NetItem};
use net::mercwin::{CARRIED, HEIRLOOM, MercWindow, WORN};

/// The popups' messages of the client's own (none of the game's): Take Back,
/// and Give (the merc's window's, and the inventory's Give to the merc).
pub const MSG_TAKE: u32 = 0x7E73_0005;
pub const MSG_GIVE: u32 = 0x7E73_0002;
/// Their words, vanilla's own (`wndlist::row_word`).
pub const WORD_TAKE: i32 = -20;
pub const WORD_GIVE: i32 = -21;
pub const WORD_GIVE_TO: i32 = -22;

/// The game's popup rows: Equip, Unequip, Inspect.
const MSG_USE: u32 = 0x0203_0000;
const MSG_INSPECT: u32 = 0x0202_0000;
const WORD_EQUIP: i32 = 121;
const WORD_UNEQUIP: i32 = 154;
const WORD_INSPECT: i32 = 122;

/// The tabs' labels.
pub const TABS: [&str; 3] = ["Worn", "Carried", "Your Pack"];

/// The rows' text.
pub fn word(word: i32) -> Option<&'static str> {
    match word {
        WORD_TAKE => Some("Take Back"),
        WORD_GIVE => Some("Give"),
        WORD_GIVE_TO => Some("Give to Merc"),
        _ => None,
    }
}

/// Where a row of the window's list comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Src {
    /// the merc's record (its id, 0 an heirloom; its kind)
    Merc { id: u32, kind: u8 },
    /// the player's pack's record, by its key
    Pack { key: u32 },
}

/// The window's list: the merc's records, worn then carried, then the
/// player's pack's not worn, each with where it comes from; and the merc's
/// base stats for the description's sums (its stats less what it wears).
pub struct View {
    pub list: ItemList,
    pub src: Vec<Src>,
    pub base: [i32; 29],
}

/// The window's list of the merc's state and the player's pack.
pub fn view(m: &MercWindow, pack: &ItemList) -> View {
    let mut list = ItemList::default();
    let mut src = Vec::new();
    for kind in [WORN, HEIRLOOM, CARRIED] {
        for r in m.records.iter().filter(|r| r.kind == kind) {
            list.items.push(r.item.clone());
            src.push(Src::Merc { id: r.id, kind: r.kind });
        }
    }
    for it in pack.items.iter().filter(|it| it.equip_slot == -1) {
        let mut it = it.clone();
        it.quick_slot = -1;
        list.items.push(it.clone());
        src.push(Src::Pack { key: it.inv_index });
    }
    let mut base = [0i32; 29];
    for (k, v) in m.stats.iter().enumerate() {
        let worn: i64 = list.items.iter().filter(|it| it.equip_slot != -1).flat_map(|it| &it.item.stats).filter(|s| s.id == k as i64).map(|s| s.value).sum();
        base[k] = (*v as i64 - worn) as i32;
    }
    View { list, src, base }
}

/// The description's first lines: the merc's name, level and class, hit
/// points, power, AC and stats.
pub fn header(m: &MercWindow, class_name: &str) -> String {
    let s = m.stats;
    format!(
        "{}\nLevel {} {class_name}\nHP {}/{}  Power {}/{}\nAC {}\nSTR {}  STA {}  AGI {}\nDEX {}  WIS {}  INT {}\nCHA {}",
        m.name, m.level, m.hp, m.max_hp, m.power, m.max_power, m.ac, s[0], s[1], s[2], s[3], s[4], s[5], s[6]
    )
}

fn entry(word: i32, msg: u32, id: i32) -> MenuEntry {
    MenuEntry { word, msg, id, icon: -1, icon2: -1, number: -1, ..Default::default() }
}

/// The three tabs' menus (and an empty fourth, never shown), each row's
/// popup: worn, Unequip, Inspect, Take Back (an heirloom Inspect alone);
/// carried, Equip (an item it may wear), Inspect, Take Back; the pack's, Give
/// and Inspect. `can_wear` is the merc's class, race and level's
/// (`inventory::can_use`); one it may not wear is badged as the game badges
/// the player's (140).
pub fn build(v: &View, count: usize, icon: &dyn Fn(&NetItem) -> i32, can_wear: &dyn Fn(&NetItem) -> bool) -> (Menus, [usize; 4]) {
    let mut menus = Menus::default();
    let mut out = [0; 4];
    for (tab, slot) in out.iter_mut().enumerate() {
        let mut entries = vec![MenuEntry::default(); count.max(v.list.items.len())];
        let mut n = 0;
        for (k, (it, src)) in v.list.items.iter().zip(&v.src).enumerate() {
            let id = k as i32;
            let shown = match (tab, src) {
                (0, Src::Merc { kind, .. }) => *kind != CARRIED,
                (1, Src::Merc { kind, .. }) => *kind == CARRIED,
                (2, Src::Pack { .. }) => true,
                _ => false,
            };
            if !shown {
                continue;
            }
            let mut e = MenuEntry { icon: icon(it), number: it.quantity as i32, x08: -1, word: -1, msg: 0, id, icon2: -1, sub: None };
            let sub = match src {
                Src::Merc { kind: HEIRLOOM, .. } => {
                    e.icon2 = 138;
                    vec![entry(WORD_INSPECT, MSG_INSPECT, id)]
                }
                Src::Merc { kind: WORN, .. } => {
                    e.icon2 = 138;
                    vec![entry(WORD_UNEQUIP, MSG_USE, id), entry(WORD_INSPECT, MSG_INSPECT, id), entry(WORD_TAKE, MSG_TAKE, id)]
                }
                Src::Merc { .. } => {
                    let mut rows = Vec::new();
                    if infotext::is_wearable(it) {
                        if !can_wear(it) {
                            e.icon2 = 140;
                        }
                        rows.push(entry(WORD_EQUIP, MSG_USE, id));
                    }
                    rows.extend([entry(WORD_INSPECT, MSG_INSPECT, id), entry(WORD_TAKE, MSG_TAKE, id)]);
                    rows
                }
                Src::Pack { .. } => vec![entry(WORD_GIVE, MSG_GIVE, id), entry(WORD_INSPECT, MSG_INSPECT, id)],
            };
            e.sub = Some(menus.push(Menu { x00: -1, x04: -1, entries: sub }));
            if n < entries.len() {
                entries[n] = e;
            }
            n += 1;
        }
        *slot = menus.push(Menu { x00: -1, x04: -1, entries });
    }
    (menus, out)
}

/// What a move of the window asks the server: the ask and its record (the
/// merc's id, or the pack's key).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ask {
    Give(u32),
    Equip(u32),
    Unequip(u32),
    Take(u32),
}

/// The ask the item menu's answer `o` makes of the window's list, if any:
/// Equip on a carried record, Unequip on a worn one, Take Back, Give.
pub fn ask_of(o: &crate::itemmenu::ItemOut, v: &View) -> Option<Ask> {
    use crate::itemmenu::ItemOut;
    let src = |i: i32| v.src.get(usize::try_from(i).ok()?).copied();
    match *o {
        ItemOut::UseItem(i, _) => match src(i)? {
            Src::Merc { id, kind: CARRIED } => Some(Ask::Equip(id)),
            Src::Merc { id, kind: WORN } => Some(Ask::Unequip(id)),
            _ => None,
        },
        ItemOut::Custom(MSG_TAKE, i) => match src(i)? {
            Src::Merc { id, kind } if kind != HEIRLOOM => Some(Ask::Take(id)),
            _ => None,
        },
        ItemOut::Custom(MSG_GIVE, i) => match src(i)? {
            Src::Pack { key } => Some(Ask::Give(key)),
            _ => None,
        },
        _ => None,
    }
}

/// The ask's bytes, of the merc of `slot`.
pub fn ask_bytes(a: Ask, slot: u8) -> Vec<u8> {
    use net::mercwin::{ASK_EQUIP, ASK_GIVE, ASK_TAKE, ASK_UNEQUIP, ask};
    match a {
        Ask::Give(key) => ask(slot, ASK_GIVE, key, 0),
        Ask::Equip(id) => ask(slot, ASK_EQUIP, id, -1),
        Ask::Unequip(id) => ask(slot, ASK_UNEQUIP, id, 0),
        Ask::Take(id) => ask(slot, ASK_TAKE, id, 0),
    }
}

/// The player's own item menus as `itemmenu::build_linked` made them, with
/// Give to Merc after Inspect in each popup of an item not worn, where `on`
/// (the server offered the windows and the player's merc is out).
pub fn with_give(built: (Menus, [usize; 4]), inv: &ItemList, on: bool) -> (Menus, [usize; 4]) {
    let (mut menus, tabs) = built;
    if !on {
        return (menus, tabs);
    }
    let mut seen = Vec::new();
    for &t in &tabs {
        let rows: Vec<(usize, i32)> = menus.list[t].entries.iter().filter_map(|e| Some((e.sub?, e.id))).collect();
        for (sub, id) in rows {
            if seen.contains(&sub) {
                continue;
            }
            seen.push(sub);
            let Some(it) = usize::try_from(id).ok().and_then(|k| inv.items.get(k)) else { continue };
            if it.equip_slot != -1 {
                continue;
            }
            let entries = &mut menus.list[sub].entries;
            let at = entries.iter().position(|e| e.msg == MSG_INSPECT).map_or(entries.len(), |k| k + 1);
            entries.insert(at, entry(WORD_GIVE_TO, MSG_GIVE, id));
        }
    }
    (menus, tabs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::itemmenu::ItemOut;
    use net::mercwin::testing::item;

    fn state() -> MercWindow {
        let mut chest = item(7, WORN, "Mithril Breastplate", 5, 5);
        chest.item.item.stats.push(net::dump::ItemStat { id: 0, value: 10 });
        MercWindow {
            name: "Kel".into(),
            level: 30,
            stats: [85, 131, 65, 102, 55, 149, 70],
            records: vec![item(9, CARRIED, "Buckler", 13, -1), chest, item(0, HEIRLOOM, "Heirloom head", 1, 0), item(11, CARRIED, "Lucky Stone", -1, -1)],
            ..Default::default()
        }
    }

    fn pack() -> ItemList {
        let mut l = ItemList::default();
        for (key, name, equip) in [(40, "Greatsword", -1), (41, "Worn Ring", 9)] {
            let mut it = NetItem { quantity: 1, equip_slot: equip, quick_slot: -1, inv_index: key, ..Default::default() };
            it.item.name = name.into();
            it.item.wear_slot = 15;
            l.items.push(it);
        }
        l
    }

    fn names(v: &View, b: &(Menus, [usize; 4]), tab: usize) -> Vec<(String, Vec<i32>)> {
        b.0.list[b.1[tab]]
            .entries
            .iter()
            .filter(|e| e.id >= 0)
            .map(|e| (v.list.items[e.id as usize].item.name.clone(), b.0.list[e.sub.unwrap()].entries.iter().map(|x| x.word).collect()))
            .collect()
    }

    /// The tabs: Worn the merc's worn and its heirloom (Unequip, Inspect, Take
    /// Back; the heirloom Inspect alone), Carried what it holds (Equip on what
    /// it may wear), Your Pack the pack's not worn (Give, Inspect). The base
    /// stats are its stats less what it wears.
    #[test]
    fn the_tabs_and_their_popups() {
        let v = view(&state(), &pack());
        let b = build(&v, 40, &|_| -1, &|_| true);
        assert_eq!(
            names(&v, &b, 0),
            vec![("Mithril Breastplate".into(), vec![WORD_UNEQUIP, WORD_INSPECT, WORD_TAKE]), ("Heirloom head".into(), vec![WORD_INSPECT])]
        );
        assert_eq!(names(&v, &b, 1), vec![("Buckler".into(), vec![WORD_EQUIP, WORD_INSPECT, WORD_TAKE]), ("Lucky Stone".into(), vec![WORD_INSPECT, WORD_TAKE])]);
        assert_eq!(names(&v, &b, 2), vec![("Greatsword".into(), vec![WORD_GIVE, WORD_INSPECT])], "the worn ring is not offered");
        assert!(names(&v, &b, 3).is_empty());
        assert_eq!(v.base[0], 75, "STR 85 less the breastplate's 10");
        let not = build(&v, 40, &|_| -1, &|it| it.item.name != "Buckler");
        let buckler = not.0.list[not.1[1]].entries.iter().find(|e| e.id >= 0).unwrap();
        assert_eq!(buckler.icon2, 140, "one it may not wear is badged");
    }

    /// The asks the window makes: Equip a carried record, Unequip a worn
    /// one, Take Back one of its own, Give a pack record by its key. The
    /// controls: Take Back on an heirloom, Give on the merc's own, Equip on a
    /// pack record ask nothing.
    #[test]
    fn the_asks_of_the_popups() {
        let v = view(&state(), &pack());
        let at = |name: &str| v.list.items.iter().position(|it| it.item.name == name).unwrap() as i32;
        assert_eq!(ask_of(&ItemOut::UseItem(at("Buckler"), 0), &v), Some(Ask::Equip(9)));
        assert_eq!(ask_of(&ItemOut::UseItem(at("Mithril Breastplate"), 0), &v), Some(Ask::Unequip(7)));
        assert_eq!(ask_of(&ItemOut::Custom(MSG_TAKE, at("Lucky Stone")), &v), Some(Ask::Take(11)));
        assert_eq!(ask_of(&ItemOut::Custom(MSG_GIVE, at("Greatsword")), &v), Some(Ask::Give(40)));
        assert_eq!(ask_of(&ItemOut::Custom(MSG_TAKE, at("Heirloom head")), &v), None);
        assert_eq!(ask_of(&ItemOut::Custom(MSG_GIVE, at("Buckler")), &v), None);
        assert_eq!(ask_of(&ItemOut::UseItem(at("Greatsword"), 0), &v), None);
        assert_eq!(ask_of(&ItemOut::Custom(MSG_TAKE, 99), &v), None);
        assert_eq!(ask_bytes(Ask::Equip(9), 2), vec![0x73, 0x7E, 2, 3, 9, 0, 0, 0, 0xFF]);
        assert_eq!(ask_bytes(Ask::Give(40), 1), vec![0x73, 0x7E, 1, 2, 40, 0, 0, 0]);
    }

    /// The player's own inventory: Give to Merc after Inspect on an item not
    /// worn, while on; off, the menus are the game's.
    #[test]
    fn give_to_merc_in_the_inventory() {
        let inv = pack();
        let built = || crate::itemmenu::build_linked(&inv, [40, 40, 40, 40], &|_| -1, &|_| true, false);
        let rows = |b: &(Menus, [usize; 4])| -> Vec<Vec<i32>> {
            b.0.list[b.1[1]].entries.iter().filter_map(|e| e.sub).map(|s| b.0.list[s].entries.iter().map(|x| x.word).collect()).collect()
        };
        let off = with_give(built(), &inv, false);
        assert_eq!(rows(&off), rows(&built()), "off: the game's");
        let on = with_give(built(), &inv, true);
        let r = rows(&on);
        assert_eq!(r[0][..3], [WORD_EQUIP, WORD_INSPECT, WORD_GIVE_TO], "the greatsword: Give to Merc after Inspect");
        assert!(!r[1].contains(&WORD_GIVE_TO), "the worn ring: none");
    }
}
