//! The player's inventory as CLIENT keeps it (`VIItemList`, `viclient.cpp`)
//! and the messages that change it: 0x003B `AddInvItem`, 0x003C `Msg003C`,
//! 0x003D `Op003D`, 0x003E `Msg003E`, 0x003F `Msg003F` (with `CanWearIn`'s
//! table and the slots a worn item clears), 0x0040 `Msg0040`, 0x000A
//! `Msg000A` and 0x0060 `Msg0060`. The list is the MemoryDump's own
//! (`dump.inventory`), changed in place. A message naming a slot out of the
//! list is dropped (the game asserts).

use net::dump::{ItemList, NetItem};

/// The inventory's capacity (`AddItem`'s 40).
pub const CAPACITY: usize = 40;

/// `CanWearIn(wear, slot)` (`viclientutil.cpp`): whether an item of wear
/// slot `wear` goes into equip slot `slot`.
pub fn can_wear_in(wear: i64, slot: i64) -> bool {
    match wear {
        1 => slot == 0,
        3 => slot == 2 || slot == 3,
        4 => slot == 4,
        5 => slot == 5,
        6 => slot == 6 || slot == 7,
        7 => slot == 8,
        8 => slot == 9 || slot == 10,
        9 => slot == 11,
        10 => slot == 12,
        11 => slot == 13,
        12 => slot == 14,
        13 => slot == 17,
        14 => slot == 14 || slot == 15,
        15 => slot == 16,
        16 | 17 => slot == 18,
        18 => slot == 19 || slot == 20,
        19 => slot == 1,
        2 => slot == 21,
        20 => slot == 14,
        21 => slot == 15,
        22..=27 => slot == 19,
        _ => false,
    }
}

/// `FitsSlot3`: what the third quick slot takes (by wear slot).
pub fn fits_slot3(it: &NetItem) -> bool {
    matches!(it.item.wear_slot, 12 | 14 | 15 | 16 | 18 | 20)
}

/// `VIClient::ItemIcon`: the expansion's icon when there is one.
pub fn item_icon(it: &NetItem, expansion_flags: u32) -> i64 {
    if expansion_flags & 2 != 0 && it.item.icon2 != 0 { it.item.icon2 } else { it.item.icon }
}

/// `VIPlayer::CanUse`: the class and race masks, then the level.
pub fn can_use(it: &NetItem, class: i64, race: i64, level: i64) -> bool {
    let (c, r) = (1i64 << class.clamp(0, 31), 1i64 << race.clamp(0, 31));
    it.item.class_mask & c != 0 && it.item.race_mask & r != 0 && level >= it.item.level_req
}

/// `VIItemList::Took`: `n` off the stack; at none the rest move up.
pub fn took(l: &mut ItemList, slot: usize, n: i64) {
    let Some(it) = l.items.get_mut(slot) else { return };
    if it.quantity < n {
        return;
    }
    it.quantity -= n;
    if it.quantity == 0 {
        l.items.remove(slot);
    }
}

/// `VIItemList::Op003D`: the item at `a` moved to `b`, the ones between
/// shifting over.
pub fn op003d(l: &mut ItemList, a: usize, b: usize) {
    if a >= l.items.len() || b >= l.items.len() {
        return;
    }
    let it = l.items.remove(a);
    l.items.insert(b, it);
}

/// `VIItemList::Find`: the slot of inventory id `id`.
pub fn find(l: &ItemList, id: u32) -> Option<usize> {
    l.items.iter().position(|it| it.inv_index == id)
}

/// `VIItemList::Unequip`: whatever is in equip slot `equip` comes out.
pub fn unequip(l: &mut ItemList, equip: i64) {
    for it in &mut l.items {
        if it.equip_slot == equip {
            it.equip_slot = -1;
        }
    }
}

/// `VIItemList::AddItem(item, limit, capacity)`: stacked onto the same item
/// up to `limit` (its max stack), else a new slot with the next inventory
/// id; the count that went in.
pub fn add_item(l: &mut ItemList, item: &NetItem, limit: i64, capacity: usize) -> i64 {
    if limit >= 2 {
        if let Some(at) = l.items.iter().position(|it| it.item.id == item.item.id) {
            let have = l.items[at].quantity;
            return if have + item.quantity > limit {
                l.items[at].quantity = limit;
                limit - have
            } else {
                l.items[at].quantity = have + item.quantity;
                item.quantity
            };
        }
        if l.items.len() >= capacity {
            return 0;
        }
        let mut it = item.clone();
        it.inv_index = l.p04;
        l.p04 += 1;
        let n = item.quantity.min(limit);
        it.quantity = n;
        l.items.push(it);
        return n;
    }
    if l.items.len() < capacity {
        let mut it = item.clone();
        it.quantity = 1;
        it.inv_index = l.p04;
        l.p04 += 1;
        l.items.push(it);
        return 1;
    }
    0
}

/// `Msg003E`: a charge used (the item gone with its last), or the item gone;
/// a worn one sounds (5 a weapon, 1 else).
pub fn use_charge(l: &mut ItemList, slot: usize, charge: bool) -> Option<i32> {
    let it = l.items.get(slot)?.clone();
    if charge {
        l.items[slot].charges -= 1;
        if l.items[slot].charges == 0 {
            took(l, slot, 1);
        }
    } else {
        took(l, slot, 1);
    }
    (it.item.wear_slot != -1).then_some(if it.item.attack_type != 0 { 5 } else { 1 })
}

/// `Msg003F`: item `i` into equip slot `slot` when it fits, and out of the
/// slots it covers; its sound (5 armour, 1 a weapon).
pub fn equip(l: &mut ItemList, i: usize, slot: i64) -> Option<i32> {
    let wear = l.items.get(i)?.item.wear_slot;
    if !can_wear_in(wear, slot) {
        return None;
    }
    unequip(l, slot);
    let off: &[i64] = match wear {
        6 => &[8],
        7 => &[7, 6],
        12 | 20 => &[16, 18, 19],
        13 => &[15, 16, 18, 20],
        14 | 21 => {
            if slot == 15 {
                &[16, 18, 17, 20]
            } else {
                &[16, 18, 19]
            }
        }
        18 => {
            if slot == 20 {
                &[16, 18, 17, 15]
            } else {
                &[16, 18, 14]
            }
        }
        15 => &[14, 15, 17, 18, 19, 20],
        16 => &[14, 15, 17, 16, 19, 20],
        22..=27 => &[16, 18, 14],
        _ => &[],
    };
    for &s in off {
        unequip(l, s);
    }
    l.items[i].equip_slot = slot;
    Some(if l.items[i].item.attack_type == 0 { 5 } else { 1 })
}

/// `Msg0040`: equip slot `slot` emptied, with the sound of what was in it.
pub fn take_off(l: &mut ItemList, slot: i64) -> Option<i32> {
    let s = l.items.iter().find(|it| it.equip_slot == slot).map(|it| if it.item.attack_type == 0 { 5 } else { 1 });
    unequip(l, slot);
    s
}

/// `Msg000A`: item `slot` onto quick slot `pos` (0-1 a type-2 item, 2 a
/// type-7, 3 what `FitsSlot3` takes), taking it off whatever had it. (The
/// game's `pos == 255` clear can never pass after `pos < 4`.) With SOE's live
/// layout (`live`, `gear`) a type-7 item goes on 3 as on 2 and nothing else
/// does (its `Msg000A`, 0x012D3550).
pub fn quick_slot(l: &mut ItemList, slot: usize, pos: u8, live: bool) {
    if pos >= 4 {
        return;
    }
    let pos = pos as i64;
    let prev = l.items.iter().position(|it| it.quick_slot == pos);
    let Some(it) = l.items.get(slot) else { return };
    let fits = if live { (it.item.p4c == 2 && pos < 2) || (it.item.p4c == 7 && (2..4).contains(&pos)) } else { (it.item.p4c == 2 && pos < 2) || (it.item.p4c == 7 && pos == 2) || (fits_slot3(it) && pos == 3) };
    if fits {
        if let Some(p) = prev {
            l.items[p].quick_slot = -1;
        }
        l.items[slot].quick_slot = pos;
    }
}

/// `VIItemList::Op0060`: an item's hit points (none below 0).
pub fn set_hp(l: &mut ItemList, slot: usize, hp: i64) {
    if let Some(it) = l.items.get_mut(slot) {
        it.cur_hp = hp.max(0);
    }
}

/// `FindInvByQuickSlot`.
pub fn find_by_quick_slot(l: &ItemList, slot: i64) -> Option<usize> {
    l.items.iter().position(|it| it.quick_slot == slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: i64, wear: i64, p4c: i64, qty: i64, inv: u32) -> NetItem {
        let mut it = NetItem { quantity: qty, equip_slot: -1, quick_slot: -1, inv_index: inv, ..Default::default() };
        it.item.id = id;
        it.item.wear_slot = wear;
        it.item.p4c = p4c;
        it
    }

    #[test]
    fn stacking_and_taking() {
        let mut l = ItemList { p04: 10, items: vec![] };
        assert_eq!(add_item(&mut l, &item(7, -1, 2, 3, 0), 5, CAPACITY), 3);
        assert_eq!(add_item(&mut l, &item(7, -1, 2, 4, 0), 5, CAPACITY), 2, "stacks to the limit");
        assert_eq!((l.items.len(), l.items[0].quantity, l.items[0].inv_index), (1, 5, 10));
        assert_eq!(add_item(&mut l, &item(7, -1, 2, 1, 0), 1, CAPACITY), 1, "limit 1: a slot of its own");
        assert_eq!((l.items.len(), l.items[1].inv_index, l.p04), (2, 11, 12));
        took(&mut l, 0, 9);
        assert_eq!(l.items[0].quantity, 5, "more than there is: nothing taken");
        took(&mut l, 0, 5);
        assert_eq!(l.items.len(), 1);
        assert_eq!(find(&l, 11), Some(0));
    }

    #[test]
    fn moving_an_item() {
        let mut l = ItemList { p04: 0, items: (0..5).map(|k| item(k, -1, 0, 1, k as u32)).collect() };
        op003d(&mut l, 1, 3);
        assert_eq!(l.items.iter().map(|i| i.item.id).collect::<Vec<_>>(), vec![0, 2, 3, 1, 4]);
        op003d(&mut l, 3, 0);
        assert_eq!(l.items.iter().map(|i| i.item.id).collect::<Vec<_>>(), vec![1, 0, 2, 3, 4]);
    }

    #[test]
    fn equipping_clears_what_it_covers() {
        // a robe (wear 15) in slot 16 takes 14, 15, 17-20 off
        let mut l = ItemList { p04: 0, items: vec![item(1, 12, 0, 1, 0), item(2, 15, 0, 1, 1), item(3, 1, 0, 1, 2)] };
        l.items[0].equip_slot = 14;
        l.items[2].equip_slot = 0;
        assert_eq!(equip(&mut l, 1, 16), Some(5));
        assert_eq!((l.items[0].equip_slot, l.items[1].equip_slot, l.items[2].equip_slot), (-1, 16, 0));
        assert_eq!(equip(&mut l, 2, 3), None, "wear 1 does not go in 3");
        assert_eq!(take_off(&mut l, 16), Some(5));
        assert_eq!(l.items[1].equip_slot, -1);
    }

    #[test]
    fn quick_slots_by_type() {
        let mut l = ItemList { p04: 0, items: vec![item(1, -1, 2, 1, 0), item(2, -1, 7, 1, 1), item(3, 14, 0, 1, 2)] };
        quick_slot(&mut l, 0, 1, false);
        quick_slot(&mut l, 1, 1, false);
        assert_eq!((l.items[0].quick_slot, l.items[1].quick_slot), (1, -1), "type 7 only on 2");
        quick_slot(&mut l, 1, 2, false);
        quick_slot(&mut l, 2, 3, false);
        assert_eq!((l.items[1].quick_slot, l.items[2].quick_slot), (2, 3));
        quick_slot(&mut l, 0, 3, false);
        assert_eq!(l.items[0].quick_slot, 1, "a type-2 item does not fit 3");
        assert_eq!(find_by_quick_slot(&l, 3), Some(2));
    }

    /// SOE's live layout (`gear`): a gem (type 7) on 3 as on 2, which takes
    /// the weapon the disc's rule put there off it; a weapon fits none and a
    /// potion still only 0-1 (the controls).
    #[test]
    fn the_live_layout_takes_gems_on_3() {
        let mut l = ItemList { p04: 0, items: vec![item(1, -1, 2, 1, 0), item(2, -1, 7, 1, 1), item(3, 14, 0, 1, 2)] };
        quick_slot(&mut l, 2, 3, false);
        assert_eq!(l.items[2].quick_slot, 3, "the disc's: a weapon on 3");
        quick_slot(&mut l, 1, 3, true);
        assert_eq!((l.items[1].quick_slot, l.items[2].quick_slot), (3, -1), "a gem on 3, the weapon off it");
        quick_slot(&mut l, 2, 3, true);
        assert_eq!(l.items[2].quick_slot, -1, "no weapon on 3 in the live layout");
        quick_slot(&mut l, 0, 2, true);
        assert_eq!(l.items[0].quick_slot, -1, "a potion not on 2");
        quick_slot(&mut l, 0, 1, true);
        assert_eq!(l.items[0].quick_slot, 1);
    }
}
