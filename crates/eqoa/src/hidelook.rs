//! Hide Look and Show Look on a worn item (the server owner, 2026-11-11: "lets
//! add a hide display option to equipped armor/weapon, still equipped, but no
//! model displayed"; go-eqoa-server's 0x7E6C-0x7E6E, `net::hidelook`). On a
//! server that offers it, the item menu's Equipment tab gives a worn item with
//! a look (armour, a robe, a weapon, a shield, a held item, a cape) a row right
//! after Unequip: Hide Look while it shows, Show Look while it is hidden. The
//! server keeps the choice for the character's slot and sends every client,
//! this one's own figure too, that slot as bare; the item stays worn, its stats
//! and swing as ever. On any other server the menu is the game's, row for row.

use crate::client_image::{MenuEntry, Menus};
use net::dump::ItemList;
use net::hidelook::State;

/// The item menu's row while the look shows: hide it.
pub const MSG_HIDE_LOOK: u32 = 0x0218_0000;
/// The item menu's row while the look is hidden: show it.
pub const MSG_SHOW_LOOK: u32 = 0x0219_0000;
/// The rows' words, vanilla's own (`wndlist::row_word`).
pub const WORD_HIDE_LOOK: i32 = -10;
pub const WORD_SHOW_LOOK: i32 = -11;

/// The rows' text.
pub fn word(word: i32) -> Option<&'static str> {
    match word {
        WORD_HIDE_LOOK => Some("Hide Look"),
        WORD_SHOW_LOOK => Some("Show Look"),
        _ => None,
    }
}

/// The menus as `itemmenu::build_linked` made them, with Hide Look or Show Look
/// after the first row (Unequip) of each worn item's popup in the Equipment
/// tab (`tabs[1]`) whose slot has a look, where the server offered it.
pub fn with_rows(built: (Menus, [usize; 4]), inv: &ItemList, state: &State) -> (Menus, [usize; 4]) {
    let (mut menus, tabs) = built;
    if !state.offered {
        return (menus, tabs);
    }
    let rows: Vec<(usize, i32)> = menus.list[tabs[1]].entries.iter().filter_map(|e| Some((e.sub?, e.id))).collect();
    for (sub, id) in rows {
        let Some(it) = usize::try_from(id).ok().and_then(|k| inv.items.get(k)) else { continue };
        let slot = i32::try_from(it.equip_slot).unwrap_or(-1);
        if slot == -1 || !net::hidelook::shown(slot) {
            continue;
        }
        let (word, msg) = if state.is_hidden(slot) { (WORD_SHOW_LOOK, MSG_SHOW_LOOK) } else { (WORD_HIDE_LOOK, MSG_HIDE_LOOK) };
        let entries = &mut menus.list[sub].entries;
        let at = 1.min(entries.len());
        entries.insert(at, MenuEntry { word, msg, id, icon: -1, icon2: -1, number: -1, ..Default::default() });
    }
    (menus, tabs)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A worn chest piece gets Hide Look right after Unequip, a hidden helm Show
    // Look, a worn ring and an item carried nothing; with no offer the menus
    // are the game's.
    #[test]
    fn the_rows_go_on_worn_items_with_a_look() {
        let mut inv = ItemList::default();
        for (name, wear, slot) in [("Plate", 5, 5), ("Crown", 1, 0), ("Ring", 3, 2), ("Carried", 5, -1)] {
            let mut it = net::dump::NetItem { quantity: 1, equip_slot: slot, quick_slot: -1, ..Default::default() };
            it.item.name = name.into();
            it.item.wear_slot = wear;
            inv.items.push(it);
        }
        let build = || crate::itemmenu::build_linked(&inv, [30, 30, 30, 40], &|_| -1, &|_| true, false);
        let game = build();
        let off = with_rows(build(), &inv, &State::default());
        let popups = |b: &(Menus, [usize; 4])| -> Vec<Vec<(u32, i32)>> {
            b.0.list[b.1[1]].entries.iter().filter_map(|e| e.sub).map(|s| b.0.list[s].entries.iter().map(|x| (x.msg, x.word)).collect()).collect()
        };
        assert_eq!(popups(&off), popups(&game), "no offer: the game's menus");
        let on = with_rows(build(), &inv, &State { offered: true, hidden: 1 << 0 });
        let tab: Vec<i32> = on.0.list[on.1[1]].entries.iter().filter(|e| e.sub.is_some()).map(|e| e.id).collect();
        for (k, id) in tab.iter().enumerate() {
            let rows = &popups(&on)[k];
            let plain = &popups(&game)[k];
            match inv.items[*id as usize].item.name.as_str() {
                "Plate" => assert_eq!(rows[1], (MSG_HIDE_LOOK, WORD_HIDE_LOOK)),
                "Crown" => assert_eq!(rows[1], (MSG_SHOW_LOOK, WORD_SHOW_LOOK)),
                _ => assert_eq!(rows, plain, "{}: no row", inv.items[*id as usize].item.name),
            }
            if rows != plain {
                assert_eq!(rows.len(), plain.len() + 1);
                assert_eq!(rows[0], plain[0], "after Unequip");
            }
        }
    }
}
