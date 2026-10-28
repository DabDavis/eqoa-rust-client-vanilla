//! `VIInfoText` (CLIENT `viinfotext.cpp`), the inventory half: the
//! descriptions the item menu shows of an item (its stats, charges, stack,
//! masks, level, flags and text), built out of WORDS.TXT as the game's
//! `snprintf16`/`strcat16` chains build them, and the four predicates that
//! sort the inventory into the item menu's tabs. `StatWithItem` (what a
//! stat would be with the item worn) is here too, over a copy of the list.

use crate::inventory;
use net::dump::{AuctionItem, Item, ItemList, NetItem};
use vi::strtab::StringTable;

/// `sStatWords` (`vikeyflash.cpp`): (word, stat).
const STAT_WORDS: [(i32, i64); 19] = [(226, 0), (227, 1), (228, 2), (229, 3), (230, 4), (231, 5), (232, 6), (234, 8), (496, 10), (240, 12), (241, 13), (242, 14), (447, 24), (451, 25), (450, 26), (452, 27), (449, 23), (448, 22), (1203, 28)];

/// `sWearSlotWords` (`viwearslot.cpp`): (word, wear slot).
const WEAR_WORDS: [(i32, i64); 28] = [
    (302, -1), (327, 1), (329, 3), (330, 4), (331, 5), (332, 6), (333, 7), (405, 8), (335, 9), (336, 10), (337, 11), (338, 12), (339, 13), (340, 14),
    (341, 15), (404, 19), (403, 2), (398, 16), (399, 17), (402, 18), (1204, 20), (1206, 21), (1550, 22), (1549, 23), (1552, 24), (1548, 25), (1553, 26), (1551, 27),
];

/// `GetRace(r)->m_shortStrID` for races 0-9, `GetClass(c)->m_shortStrID`.
const RACE_SHORT: [i32; 10] = [317, 318, 319, 320, 321, 322, 323, 324, 325, 1191];
const CLASS_SHORT: [i32; 15] = [303, 304, 305, 306, 307, 308, 309, 310, 311, 312, 313, 314, 315, 316, 1190];

/// The LAST entry for the id (`FindStatWord`, `FindWearSlotWord`,
/// `FindAttackWord`: no break).
fn find(table: &[(i32, i64)], id: i64) -> Option<i32> {
    table.iter().rev().find(|e| e.1 == id).map(|e| e.0)
}

/// `FindWearSlotWord(slot)`: the wear slot's word (`sWearSlotWords`).
pub fn wear_slot_word(slot: i64) -> Option<i32> {
    find(&WEAR_WORDS, slot)
}

/// `FindAttackWord(id)`: the attack type's word in `gAttackWords` (CLIENT's
/// data, `StartData::attack_words`).
pub fn attack_word(attack_words: &[(i32, i64)], id: i64) -> Option<i32> {
    find(attack_words, id)
}

/// What the describers read off the client.
pub struct Info<'a> {
    pub words: &'a StringTable,
    pub inv: &'a ItemList,
    /// `GetHP`, `GetMaxHP`, `GetPower`, `GetMaxPower`
    pub hp: [i32; 4],
    /// `m_player.m_baseStat`
    pub base: &'a [i32; 29],
    pub class: i64,
    pub level: i64,
    /// `gAttackWords` (CLIENT's data): (word, attack type)
    pub attack_words: &'a [(i32, i64)],
}

impl Info<'_> {
    fn w(&self, id: i32) -> &str {
        self.words.get(id, 0)
    }

    fn item(&self, i: i32) -> Option<&NetItem> {
        if i < 0 { None } else { self.inv.items.get(i as usize) }
    }

    fn name_line(&self, it: &NetItem) -> String {
        format!("{}\n \n", it.item.name)
    }

    fn charges(&self, d: &mut String, v: i64) {
        *d += &format!("{}: {v}", self.w(297));
    }

    fn stack(&self, d: &mut String, v: i64, max: i64) {
        *d += &format!("{}: {v}/{max}", self.w(491));
    }

    fn hit_points(&self, d: &mut String, v: i64, max: i64) {
        *d += &format!("{}: {v}/{max}", self.w(233));
    }

    fn level(&self, d: &mut String, v: i64) {
        *d += &format!("{}: {v}", self.w(503));
    }

    /// `AddEquipSlot`.
    fn equip_slot(&self, d: &mut String, v: i64) {
        match wear_slot_word(v) {
            Some(w) => *d += &format!("{}: {}", self.w(490), self.w(w)),
            None => *d += &format!("{}: {v}", self.w(490)),
        }
    }

    /// `AddAttackType`.
    fn attack_type(&self, d: &mut String, v: i64) {
        match attack_word(self.attack_words, v) {
            Some(w) => *d += &format!("{}: {}", self.w(497), self.w(w)),
            None => *d += &format!("{}: {v}", self.w(497)),
        }
    }

    /// `AddRaceMask` / `AddClassMask`: ALL, NONE, or each short name and a
    /// space.
    fn mask(&self, d: &mut String, title: i32, mask: i64, short: &[i32]) {
        let bits = short.len();
        let all = (0..bits).all(|b| mask & (1 << b) != 0);
        let none = (0..bits).all(|b| mask & (1 << b) == 0);
        *d += self.w(title);
        *d += ": ";
        if all {
            *d += self.w(301);
        } else if none {
            *d += self.w(302);
        } else {
            for (b, &w) in short.iter().enumerate() {
                if (mask >> b) & 1 != 0 {
                    *d += self.w(w);
                    *d += " ";
                }
            }
        }
    }

    /// The no-trade / no-rent / lore words and their line.
    fn flags(&self, d: &mut String, it: &NetItem, newline_first: bool) {
        self.item_flags(d, &it.item, newline_first);
    }

    /// `flags` on the item record itself (an auction lot's).
    fn item_flags(&self, d: &mut String, it: &Item, newline_first: bool) {
        let any = it.no_trade != 0 || it.rent == 0 || it.lore != 0;
        if newline_first && any {
            *d += "\n";
        }
        if it.no_trade > 0 {
            *d += &format!("{} ", self.w(553));
        }
        if it.rent == 0 {
            *d += &format!("{} ", self.w(554));
        }
        if it.lore > 0 {
            *d += self.w(555);
        }
        if !newline_first && any {
            *d += "\n";
        }
    }

    /// `AddStats(index, source -1 or 0, all)`: a line a stat, "Str: 5", or
    /// for source 0 against the stat with it worn, "Str: 5 -> 7"; six at
    /// most unless `all`.
    fn stats(&self, d: &mut String, i: i32, source: i32, all: bool) {
        let Some(it) = self.item(i) else { return };
        for (n, s) in it.item.stats.iter().enumerate() {
            if !all && n >= 6 {
                break;
            }
            match find(&STAT_WORDS, s.id) {
                Some(w) if source == -1 => *d += &format!("{}: {}\n", self.w(w), s.value),
                Some(w) => *d += &format!("{}: {} -> {}\n", self.w(w), s.value, self.stat_with_item(i as usize, s.id)),
                None => *d += &format!("{}: {}\n", s.id, s.value),
            }
        }
    }

    /// `VIClient::StatWithItem(index, stat, 0)`: the base stat plus every
    /// worn item's, with item `index` worn where `EquipSlotFor` puts it.
    pub fn stat_with_item(&self, index: usize, stat: i64) -> i64 {
        let mut list = self.inv.clone();
        if index < list.items.len() {
            let slot = equip_slot_for(&list, &list.items[index], self.class, self.level);
            inventory::equip(&mut list, index, slot);
        }
        let mut total = self.base.get(stat as usize).copied().unwrap_or(0) as i64;
        for it in &list.items {
            for s in &it.item.stats {
                if s.id == stat && it.equip_slot != -1 {
                    total += s.value;
                }
            }
        }
        total
    }

    /// `DescribeInvUsable`: an item with charges.
    pub fn inv_usable(&self, i: i32) -> String {
        let Some(it) = self.item(i) else { return String::new() };
        let mut d = self.name_line(it);
        if it.charges > 0 {
            self.charges(&mut d, it.charges);
            d += "\n";
        }
        if it.item.max_stack >= 2 {
            self.stack(&mut d, it.quantity, it.item.max_stack);
            d += "\n";
        }
        self.mask(&mut d, 289, it.item.race_mask, &RACE_SHORT);
        d += "\n";
        self.mask(&mut d, 288, it.item.class_mask, &CLASS_SHORT);
        d += "\n";
        self.level(&mut d, it.item.level_req);
        d += "\n";
        self.flags(&mut d, it, false);
        d += &it.item.desc;
        d
    }

    /// `DescribeInvEquipment`: `DescribeInvItem(i, 1)`, in full.
    pub fn inv_equipment(&self, i: i32) -> String {
        let Some(it) = self.item(i) else { return String::new() };
        let mut d = self.name_line(it);
        if it.item.damage > 0 {
            d += &format!("{}: {} ", self.w(493), it.item.damage);
        }
        if it.item.attack_type != 0 {
            self.attack_type(&mut d, it.item.attack_type);
            d += "\n";
        }
        if it.item.wear_slot != -1 {
            self.equip_slot(&mut d, it.item.wear_slot);
            d += "\n";
        }
        if is_ranged(it) && it.item.range > 0 {
            d += &format!("{}: {}\n", self.w(504), it.item.range);
        }
        if it.item.max_hp != 0 {
            self.hit_points(&mut d, it.cur_hp, it.item.max_hp);
            d += "\n";
        }
        if it.item.durability != 0 {
            d += &format!("{}: {}\n", self.w(495), it.item.durability);
        }
        if it.item.max_stack >= 2 {
            self.stack(&mut d, it.quantity, it.item.max_stack);
            d += "\n";
        }
        self.mask(&mut d, 289, it.item.race_mask, &RACE_SHORT);
        d += "\n";
        self.mask(&mut d, 288, it.item.class_mask, &CLASS_SHORT);
        if it.item.level_req > 0 && (it.item.wear_slot != -1 || it.item.p4c != 0) {
            d += "\n";
            self.level(&mut d, it.item.level_req);
        }
        self.flags(&mut d, it, true);
        d
    }

    /// `DescribeInvStats`: the name, every stat bonus, the text.
    pub fn inv_stats(&self, i: i32) -> String {
        let Some(it) = self.item(i) else { return String::new() };
        let mut d = self.name_line(it);
        self.stats(&mut d, i, -1, true);
        if !it.item.stats.is_empty() {
            d += " \n";
        }
        d += &it.item.desc;
        d
    }

    /// `DescribeInvMisc`: charges, masks and level only with a type.
    pub fn inv_misc(&self, i: i32) -> String {
        let Some(it) = self.item(i) else { return String::new() };
        let typed = it.item.p4c != 0;
        let mut d = self.name_line(it);
        if it.charges > 0 && typed {
            self.charges(&mut d, it.charges);
            d += "\n";
        }
        if it.item.max_stack >= 2 {
            self.stack(&mut d, it.quantity, it.item.max_stack);
            d += "\n";
        }
        if typed {
            self.mask(&mut d, 289, it.item.race_mask, &RACE_SHORT);
            d += "\n";
            self.mask(&mut d, 288, it.item.class_mask, &CLASS_SHORT);
            if it.item.level_req > 0 {
                d += "\n";
                self.level(&mut d, it.item.level_req);
            }
            d += "\n";
        }
        self.flags(&mut d, it, false);
        d += &it.item.desc;
        d
    }

    /// `DescribeInvCharged`: the stack and the hit points.
    pub fn inv_charged(&self, i: i32) -> String {
        let Some(it) = self.item(i) else { return String::new() };
        let mut d = self.name_line(it);
        if it.item.max_stack >= 2 {
            self.stack(&mut d, it.quantity, it.item.max_stack);
            d += "\n";
        }
        if it.item.max_hp > 0 {
            self.hit_points(&mut d, it.cur_hp, it.item.max_hp);
            d += "\n";
        }
        self.flags(&mut d, it, false);
        d += &it.item.desc;
        d
    }

    /// `DescribeInvChargedShort`: the same, short, ending "Inspect for more
    /// info" (492).
    pub fn inv_charged_short(&self, i: i32) -> String {
        let Some(it) = self.item(i) else { return String::new() };
        let mut d = self.name_line(it);
        if it.item.max_stack >= 2 {
            self.stack(&mut d, it.quantity, it.item.max_stack);
            d += "\n";
        }
        if it.item.max_hp > 0 {
            self.hit_points(&mut d, it.cur_hp, it.item.max_hp);
            d += "\n";
        }
        d += self.w(492);
        d
    }

    /// `DescribeInvConsumable`: the player's HP and power, then the item.
    pub fn inv_consumable(&self, i: i32) -> String {
        let Some(it) = self.item(i) else { return String::new() };
        let mut d = format!("{} {}/{}\n", self.w(233), self.hp[0], self.hp[1]);
        d += &format!("{} {}/{}\n \n", self.w(235), self.hp[2], self.hp[3]);
        d += &it.item.name;
        d += "\n \n";
        if it.charges > 0 {
            self.charges(&mut d, it.charges);
            d += "\n";
        }
        if it.item.max_stack >= 2 {
            self.stack(&mut d, it.quantity, it.item.max_stack);
            d += "\n";
        }
        d += self.w(492);
        d
    }

    /// `DescribeInvEquipmentShort`: slot, hit points, durability, damage,
    /// attack, range, stack, six stat lines against what is worn, 492.
    pub fn inv_equipment_short(&self, i: i32) -> String {
        let Some(it) = self.item(i) else { return String::new() };
        let mut d = self.name_line(it);
        self.equip_slot(&mut d, it.item.wear_slot);
        d += "\n";
        self.hit_points(&mut d, it.cur_hp, it.item.max_hp);
        d += "\n";
        d += &format!("{}: {}\n", self.w(495), it.item.durability);
        if it.item.damage > 0 {
            d += &format!("{}: {}\n", self.w(493), it.item.damage);
        }
        if it.item.attack_type != 0 {
            self.attack_type(&mut d, it.item.attack_type);
            d += "\n";
        }
        if is_ranged(it) && it.item.range > 0 {
            d += &format!("{}: {}\n", self.w(504), it.item.range);
        }
        if it.item.max_stack >= 2 {
            self.stack(&mut d, it.quantity, it.item.max_stack);
            d += "\n";
        }
        self.stats(&mut d, i, 0, true);
        d += self.w(492);
        d
    }

    /// `DescribeInvFood`: the food value and the stack.
    pub fn inv_food(&self, i: i32) -> String {
        let Some(it) = self.item(i) else { return String::new() };
        let mut d = self.name_line(it);
        if it.item.p58 > 0 {
            d += &format!("{}: {}\n", self.w(11), it.item.p58);
        }
        if it.item.max_stack >= 2 {
            self.stack(&mut d, it.quantity, it.item.max_stack);
            d += "\n";
        }
        d += self.w(492);
        d
    }

    /// `DescribeLootItem`: a corpse's item, its name; the masks and level
    /// for something typed or wearable; the flags.
    pub fn loot_item(&self, it: &NetItem) -> String {
        let mut d = it.item.name.clone();
        if it.item.p4c != 0 || it.item.wear_slot != -1 {
            d += "\n";
            self.mask(&mut d, 289, it.item.race_mask, &RACE_SHORT);
            d += "\n";
            self.mask(&mut d, 288, it.item.class_mask, &CLASS_SHORT);
            d += "\n";
            self.level(&mut d, it.item.level_req);
        }
        self.flags(&mut d, it, true);
        d
    }

    /// `DescribeItem(dst, size, item)` (0x00C06C50): a record the caller
    /// holds (the fishing window's catch), as `DescribeLootItem` writes a
    /// corpse's but with the flags' words on the record's own nonzero
    /// fields (`m_noTrade`, `m_lore`), where the corpse's ask `> 0`.
    pub fn describe_item(&self, it: &NetItem) -> String {
        let it = &it.item;
        let mut d = it.name.clone();
        if it.p4c != 0 || it.wear_slot != -1 {
            d += "\n";
            self.mask(&mut d, 289, it.race_mask, &RACE_SHORT);
            d += "\n";
            self.mask(&mut d, 288, it.class_mask, &CLASS_SHORT);
            d += "\n";
            self.level(&mut d, it.level_req);
        }
        if it.no_trade != 0 || it.rent == 0 || it.lore != 0 {
            d += "\n";
        }
        if it.no_trade != 0 {
            d += &format!("{} ", self.w(553));
        }
        if it.rent == 0 {
            d += &format!("{} ", self.w(554));
        }
        if it.lore != 0 {
            d += self.w(555);
        }
        d
    }

    /// `DescribeInvItem(i, flags)` over a record, and `DescribeMerchantItem`
    /// (`merchant`: the hit points as the maximum alone, never the stack or
    /// the charges' flag). `flags & 1`: the blank line after the name, the
    /// stack instead of the maximum stack, no charges.
    fn list_item(&self, it: &NetItem, flags: i32, merchant: bool) -> String {
        let full = flags & 1 != 0;
        let mut d = format!("{}\n", it.item.name);
        if full {
            d += " \n";
        }
        if it.item.damage > 0 {
            d += &format!("{}: {} ", self.w(493), it.item.damage);
        }
        if it.item.attack_type != 0 {
            self.attack_type(&mut d, it.item.attack_type);
            d += "\n";
        }
        if it.item.wear_slot != -1 {
            self.equip_slot(&mut d, it.item.wear_slot);
            d += "\n";
        }
        if is_ranged(it) && it.item.range > 0 {
            d += &format!("{}: {}\n", self.w(504), it.item.range);
        }
        if merchant {
            if it.item.max_hp != 0 {
                d += &format!("{}: {}\n", self.w(233), it.item.max_hp);
            }
        } else if it.item.max_hp != 0 {
            self.hit_points(&mut d, it.cur_hp, it.item.max_hp);
            d += "\n";
        }
        if it.item.durability != 0 {
            d += &format!("{}: {}\n", self.w(495), it.item.durability);
        }
        if full {
            if it.item.max_stack >= 2 {
                self.stack(&mut d, it.quantity, it.item.max_stack);
                d += "\n";
            }
        } else if it.item.max_stack >= 2 {
            d += &format!("{}: {}\n", self.w(502), it.item.max_stack);
        }
        if it.charges > 0 && !full {
            self.charges(&mut d, it.charges);
            d += "\n";
        }
        self.mask(&mut d, 289, it.item.race_mask, &RACE_SHORT);
        d += "\n";
        self.mask(&mut d, 288, it.item.class_mask, &CLASS_SHORT);
        if it.item.level_req > 0 && (it.item.wear_slot != -1 || it.item.p4c != 0) {
            d += "\n";
            self.level(&mut d, it.item.level_req);
        }
        self.flags(&mut d, it, true);
        d
    }

    /// `DescribeInvItem(i, flags)`.
    pub fn inv_item(&self, i: i32, flags: i32) -> String {
        self.item(i).map_or_else(String::new, |it| self.list_item(it, flags, false))
    }

    /// `DescribeBankItem`: the bank's record, the lines `DescribeInvItem(i,
    /// 0)` gives an inventory one (name, damage, attack, slot, range, hit
    /// points, durability, the maximum stack, charges, masks, level, flags).
    pub fn bank_item(&self, it: &NetItem) -> String {
        self.list_item(it, 0, false)
    }

    /// `DescribeMerchantItem`.
    pub fn merchant_item(&self, it: &NetItem) -> String {
        self.list_item(it, 0, true)
    }

    /// `DescribeTradeInvItem` / `DescribeTradeOffer` (the same lines):
    /// attack, slot, hit points, masks, level, flags.
    pub fn trade_item(&self, it: &NetItem) -> String {
        let mut d = format!("{}\n", it.item.name);
        if it.item.attack_type != 0 {
            self.attack_type(&mut d, it.item.attack_type);
            d += "\n";
        }
        if it.item.wear_slot != -1 {
            self.equip_slot(&mut d, it.item.wear_slot);
            d += "\n";
        }
        if it.item.max_hp != 0 {
            self.hit_points(&mut d, it.cur_hp, it.item.max_hp);
            d += "\n";
        }
        self.mask(&mut d, 289, it.item.race_mask, &RACE_SHORT);
        d += "\n";
        self.mask(&mut d, 288, it.item.class_mask, &CLASS_SHORT);
        if it.item.level_req > 0 && (it.item.wear_slot != -1 || it.item.p4c != 0) {
            d += "\n";
            self.level(&mut d, it.item.level_req);
        }
        self.flags(&mut d, it, true);
        d
    }

    /// `DescribeAuctionItem`: an auction lot by what its item is
    /// (`IsWearable`, `IsMisc`, `IsUsable`, `IsCharged`, in that order),
    /// nothing when it is none of them. Unlike the inventory's describers
    /// these read the lot's own record: its count, hit points and charges.
    pub fn auction_item(&self, lot: &AuctionItem) -> String {
        let it = &lot.item;
        if wearable(it) {
            self.auction_equipment(lot)
        } else if misc(it) {
            self.auction_misc(lot)
        } else if usable(it) {
            self.auction_usable(lot)
        } else if charged(it) {
            self.auction_charged(lot)
        } else {
            String::new()
        }
    }

    /// `AddStats(0, 8, 1, item)`: every stat of the lot's item, "Str: 5",
    /// or its number when no word names it.
    fn auction_stats(&self, d: &mut String, it: &Item) {
        for s in &it.stats {
            match find(&STAT_WORDS, s.id) {
                Some(w) => *d += &format!("{}: {}\n", self.w(w), s.value),
                None => *d += &format!("{}: {}\n", s.id, s.value),
            }
        }
    }

    /// `DescribeAuctionEquipment`.
    fn auction_equipment(&self, lot: &AuctionItem) -> String {
        let it = &lot.item;
        let mut d = format!("{}\n\n", it.name);
        if it.damage > 0 {
            d += &format!("{}: {}", self.w(493), it.damage);
            d += " ";
        }
        if it.attack_type != 0 {
            self.attack_type(&mut d, it.attack_type);
            d += "\n";
        }
        if it.wear_slot != -1 {
            self.equip_slot(&mut d, it.wear_slot);
            d += "\n";
        }
        if ranged(it) && it.range > 0 {
            d += &format!("{}: {}", self.w(504), it.range);
            d += "\n";
        }
        if it.max_hp > 0 {
            self.hit_points(&mut d, lot.cur_hp, it.max_hp);
            d += "\n";
        }
        if it.durability != 0 {
            d += &format!("{}: {}", self.w(495), it.durability);
            d += "\n";
        }
        if it.max_stack >= 2 {
            self.stack(&mut d, lot.quantity, it.max_stack);
            d += "\n";
        }
        if lot.charges > 0 {
            self.charges(&mut d, lot.charges);
            d += "\n";
        }
        self.mask(&mut d, 289, it.race_mask, &RACE_SHORT);
        d += "\n";
        self.mask(&mut d, 288, it.class_mask, &CLASS_SHORT);
        d += "\n";
        self.level(&mut d, it.level_req);
        d += "\n";
        self.item_flags(&mut d, it, false);
        if !it.stats.is_empty() {
            self.auction_stats(&mut d, it);
            d += " \n";
        }
        d += &it.desc;
        d
    }

    /// `DescribeAuctionMisc`: charges, masks and level only with a type.
    fn auction_misc(&self, lot: &AuctionItem) -> String {
        let it = &lot.item;
        let mut d = format!("{}\n\n", it.name);
        if it.max_stack >= 2 {
            self.stack(&mut d, lot.quantity, it.max_stack);
            d += "\n";
        }
        if it.p4c != 0 {
            if lot.charges > 0 {
                self.charges(&mut d, lot.charges);
                d += "\n";
            }
            self.mask(&mut d, 289, it.race_mask, &RACE_SHORT);
            d += "\n";
            self.mask(&mut d, 288, it.class_mask, &CLASS_SHORT);
            d += "\n";
            self.level(&mut d, it.level_req);
            d += "\n";
        }
        self.item_flags(&mut d, it, false);
        d += &it.desc;
        d
    }

    /// `DescribeAuctionUsable`.
    fn auction_usable(&self, lot: &AuctionItem) -> String {
        let it = &lot.item;
        let mut d = format!("{}\n\n", it.name);
        if it.max_stack >= 2 {
            self.stack(&mut d, lot.quantity, it.max_stack);
            d += "\n";
        }
        if lot.charges > 0 {
            self.charges(&mut d, lot.charges);
            d += "\n";
        }
        self.mask(&mut d, 289, it.race_mask, &RACE_SHORT);
        d += "\n";
        self.mask(&mut d, 288, it.class_mask, &CLASS_SHORT);
        d += "\n";
        self.level(&mut d, it.level_req);
        d += "\n";
        self.item_flags(&mut d, it, false);
        d += &it.desc;
        d
    }

    /// `DescribeAuctionCharged`: the stack and the hit points.
    fn auction_charged(&self, lot: &AuctionItem) -> String {
        let it = &lot.item;
        let mut d = format!("{}\n\n", it.name);
        if it.max_stack >= 2 {
            self.stack(&mut d, lot.quantity, it.max_stack);
            d += "\n";
        }
        if it.max_hp > 0 {
            self.hit_points(&mut d, lot.cur_hp, it.max_hp);
            d += "\n";
        }
        self.item_flags(&mut d, it, false);
        d += &it.desc;
        d
    }

    /// `GetStatTotal`: the base stat plus every worn item's.
    pub fn stat_total(&self, stat: i64) -> i64 {
        stat_total(self.base, self.inv, stat)
    }

    /// `StatWithItem(index, stat, 1)`: the stat with a merchant's item worn,
    /// over a list of only the worn items (merged, keeping their keys) with
    /// the item added (`AddItem`, the list's own counter from 0). The key
    /// found can be a worn item's, as shipped. `None` when the item would
    /// not go in (the caller says word 823); the total then is
    /// `GetStatTotal`.
    pub fn stat_with_merchant_item(&self, src: &NetItem, stat: i64) -> (i64, bool) {
        let mut list = ItemList::default();
        for it in self.inv.items.iter().filter(|it| it.equip_slot != -1) {
            merge(&mut list, it, it.item.max_stack, inventory::CAPACITY);
        }
        let (n, mut key) = add_item_key(&mut list, src, src.item.max_stack, inventory::CAPACITY);
        if n == 0 {
            // FindFullStack
            match list.items.iter().find(|it| src.item.id == it.item.id && src.item.max_stack == it.quantity && src.item.max_stack > 1) {
                Some(it) => key = it.inv_index,
                None => return (self.stat_total(stat), false),
            }
        }
        let Some(slot) = inventory::find(&list, key) else { return (self.stat_total(stat), true) };
        let at = equip_slot_for(&list, src, self.class, self.level);
        inventory::equip(&mut list, slot, at);
        let mut total = self.base.get(stat as usize).copied().unwrap_or(0) as i64;
        for it in list.items.iter().filter(|it| it.equip_slot != -1) {
            total += it.item.stats.iter().filter(|s| s.id == stat).map(|s| s.value).sum::<i64>();
        }
        (total, true)
    }
}

/// `GetStatTotal` over base stats `base` and list `inv`: the base stat plus
/// every worn item's.
pub fn stat_total(base: &[i32; 29], inv: &ItemList, stat: i64) -> i64 {
    let mut total = base.get(stat as usize).copied().unwrap_or(0) as i64;
    for it in inv.items.iter().filter(|it| it.equip_slot != -1) {
        total += it.item.stats.iter().filter(|s| s.id == stat).map(|s| s.value).sum::<i64>();
    }
    total
}

/// `FindStatWord`.
pub fn stat_word(stat: i64) -> Option<i32> {
    find(&STAT_WORDS, stat)
}

/// `VIItemList::Merge`: as `AddItem`, but a new record keeps its own key.
pub fn merge(l: &mut ItemList, item: &NetItem, limit: i64, capacity: usize) -> i64 {
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
        let n = item.quantity.min(limit);
        it.quantity = n;
        l.items.push(it);
        return n;
    }
    if l.items.len() < capacity {
        let mut it = item.clone();
        it.quantity = 1;
        l.items.push(it);
        return 1;
    }
    0
}

/// `VIItemList::AddItem`: how many went in, and the key it hands back in
/// `item.m_invIndex` (the stack's when it went onto one; the item's own
/// when nothing went in).
pub fn add_item_key(l: &mut ItemList, item: &NetItem, limit: i64, capacity: usize) -> (i64, u32) {
    let stack = if limit >= 2 { l.items.iter().position(|it| it.item.id == item.item.id) } else { None };
    let before = l.items.len();
    let n = inventory::add_item(l, item, limit, capacity);
    let key = match stack {
        Some(at) => l.items[at].inv_index,
        None if l.items.len() > before => l.items[before].inv_index,
        None => item.inv_index,
    };
    (n, key)
}

/// `IsRangedItem` / `VIInfoText::IsRanged`: attack types 7-10.
pub fn is_ranged(it: &NetItem) -> bool {
    ranged(&it.item)
}

/// `IsCharged`: a stack with a count, not type 8, not worn.
pub fn is_charged(it: &NetItem) -> bool {
    charged(&it.item)
}

/// `IsWearable`.
pub fn is_wearable(it: &NetItem) -> bool {
    wearable(&it.item)
}

/// `IsMisc`.
pub fn is_misc(it: &NetItem) -> bool {
    misc(&it.item)
}

/// `IsUsable`.
pub fn is_usable(it: &NetItem) -> bool {
    usable(&it.item)
}

// The predicates on the item record (`VIInfoText::Is*` take a `VIItem`, so
// an auction lot's item too).
fn ranged(it: &Item) -> bool {
    (7..=10).contains(&it.attack_type)
}

fn charged(it: &Item) -> bool {
    it.p54 > 0 && it.p4c != 8 && it.wear_slot == -1
}

fn wearable(it: &Item) -> bool {
    it.wear_slot != -1
}

fn misc(it: &Item) -> bool {
    !matches!(it.p4c, 7 | 2 | 5 | 8) && it.p54 == 0 && it.wear_slot == -1
}

fn usable(it: &Item) -> bool {
    matches!(it.p4c, 7 | 2 | 5 | 8)
}

/// `VIClient::EquipSlotFor`: where the item would be worn (the second of a
/// pair when the first is taken; a second primary only for the classes
/// that dual-wield, from level 20).
pub fn equip_slot_for(l: &ItemList, it: &NetItem, class: i64, level: i64) -> i64 {
    let taken = |s: i64| l.items.iter().any(|x| x.equip_slot == s);
    let pair = |a: i64, b: i64| {
        if it.equip_slot == a || it.equip_slot == b {
            it.equip_slot
        } else if taken(a) {
            b
        } else {
            a
        }
    };
    match it.item.wear_slot {
        1 => 0,
        3 => pair(2, 3),
        4 => 4,
        5 => 5,
        6 => pair(6, 7),
        7 => 8,
        19 => 1,
        8 => pair(9, 10),
        2 => 21,
        9 => 11,
        10 => 12,
        11 => 13,
        13 => 17,
        14 => {
            if it.equip_slot == 14 || it.equip_slot == 15 {
                it.equip_slot
            } else if taken(14) && level >= 20 && matches!(class, 0 | 6 | 4 | 1 | 5) {
                15
            } else {
                14
            }
        }
        15 => 16,
        16 => 18,
        18 => pair(19, 20),
        12 | 20 => 14,
        21 => 15,
        22..=27 => 19,
        _ => -1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use net::dump::ItemStat;

    /// Words as `wN` for WORDS.TXT line N, so a description says which.
    fn numbered() -> StringTable {
        let lines: Vec<String> = (1..=2000).map(|n| format!("w{n}")).collect();
        StringTable::parse(lines.join("\n").as_bytes()).unwrap()
    }

    /// A lot of 3 with 40 hit points left and 2 charges: the lot's own
    /// record, which the describers read where the inventory's read a slot.
    fn lot(item: Item) -> AuctionItem {
        AuctionItem { lot_id: 9, quantity: 3, cur_hp: 40, charges: 2, item, ..Default::default() }
    }

    /// `DescribeAuctionItem` by the lot's kind, as the four describers write
    /// it: a blank line after the name, the lot's count, hit points and
    /// charges, the stats after the flags; a lot of no kind, nothing. A worn
    /// charm of a usable type is equipment (`IsWearable` first).
    #[test]
    fn an_auction_lot_is_described_by_its_kind() {
        let words = numbered();
        let (inv, base) = (ItemList::default(), [0; 29]);
        let info = Info { words: &words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 0, attack_words: &[(500, 1)] };
        let stats = vec![ItemStat { id: 0, value: 5 }, ItemStat { id: 99, value: 7 }];
        let sword = Item { name: "Sword".into(), damage: 12, attack_type: 1, wear_slot: 14, max_hp: 80, durability: 5, race_mask: 0x3FF, class_mask: 0b101, level_req: 30, no_trade: 1, rent: 1, lore: 1, stats, desc: "A blade.".into(), ..Default::default() };
        assert_eq!(info.auction_item(&lot(sword)), "Sword\n\nw493: 12 w497: w500\nw490: w340\nw233: 40/80\nw495: 5\nw297: 2\nw289: w301\nw288: w303 w305 \nw503: 30\nw553 w555\nw226: 5\n99: 7\n \nA blade.");
        // a bow: its range (attack types 7-10), an attack type no word names,
        // no race or class, and a lot that may not be rented
        let bow = Item { name: "Bow".into(), attack_type: 8, wear_slot: 16, range: 40, ..Default::default() };
        assert_eq!(info.auction_item(&lot(bow)), "Bow\n\nw497: 8\nw490: w398\nw504: 40\nw297: 2\nw289: w302\nw288: w302\nw503: 0\nw554 \n");
        // misc: the stack; untyped, nothing else
        let gem = Item { name: "Gem".into(), wear_slot: -1, max_stack: 20, rent: 1, desc: "Shiny.".into(), ..Default::default() };
        assert_eq!(info.auction_item(&lot(gem)), "Gem\n\nw491: 3/20\nShiny.");
        // typed misc: charges, masks and level too
        let rune = Item { name: "Rune".into(), wear_slot: -1, p4c: 3, race_mask: 0x3FF, class_mask: 0x7FFF, level_req: 10, rent: 1, ..Default::default() };
        assert_eq!(info.auction_item(&lot(rune)), "Rune\n\nw297: 2\nw289: w301\nw288: w301\nw503: 10\n");
        // usable (type 2): the stack, the charges, masks, level, lore
        let potion = Item { name: "Potion".into(), wear_slot: -1, p4c: 2, max_stack: 5, race_mask: 0x3FF, class_mask: 0x7FFF, level_req: 1, rent: 1, lore: 1, desc: "Heals.".into(), ..Default::default() };
        assert_eq!(info.auction_item(&lot(potion)), "Potion\n\nw491: 3/5\nw297: 2\nw289: w301\nw288: w301\nw503: 1\nw555\nHeals.");
        // charged (a count, not type 8, not worn): the stack and hit points
        let arrows = Item { name: "Arrows".into(), wear_slot: -1, p54: 5, max_stack: 50, max_hp: 100, rent: 1, ..Default::default() };
        assert_eq!(info.auction_item(&lot(arrows)), "Arrows\n\nw491: 3/50\nw233: 40/100\n");
        // worn and of a usable type: equipment
        let charm = Item { name: "Charm".into(), wear_slot: 8, p4c: 2, rent: 1, ..Default::default() };
        assert_eq!(info.auction_item(&lot(charm)), "Charm\n\nw490: w405\nw297: 2\nw289: w302\nw288: w302\nw503: 0\n");
        // none of the four (a negative count): nothing
        let odd = Item { name: "Odd".into(), wear_slot: -1, p54: -1, ..Default::default() };
        assert_eq!(info.auction_item(&lot(odd)), "");
    }

    fn it(wear: i64, p4c: i64, p54: i64) -> NetItem {
        let mut n = NetItem { quantity: 1, equip_slot: -1, quick_slot: -1, ..Default::default() };
        n.item.wear_slot = wear;
        n.item.p4c = p4c;
        n.item.p54 = p54;
        n
    }

    #[test]
    fn the_tabs_partition_by_type() {
        // a worn thing, a potion (2), a charged stack, a plain thing
        let items = [it(1, 0, 0), it(-1, 2, 0), it(-1, 0, 5), it(-1, 0, 0), it(-1, 8, 5)];
        let tab = |f: fn(&NetItem) -> bool| items.iter().map(f).collect::<Vec<_>>();
        assert_eq!(tab(is_wearable), [true, false, false, false, false]);
        assert_eq!(tab(is_usable), [false, true, false, false, true]);
        assert_eq!(tab(is_charged), [false, false, true, false, false], "type 8 is not charged");
        assert_eq!(tab(is_misc), [false, false, false, true, false]);
    }

    #[test]
    fn a_stat_with_the_item_worn() {
        let mut ring = it(8, 0, 0);
        ring.item.stats = vec![ItemStat { id: 0, value: 5 }];
        let mut worn = it(8, 0, 0);
        worn.equip_slot = 9;
        worn.item.stats = vec![ItemStat { id: 0, value: 2 }];
        let inv = ItemList { p04: 0, items: vec![worn, ring] };
        assert_eq!(equip_slot_for(&inv, &inv.items[1], 0, 1), 10, "the second ring slot");
        let words = StringTable::default();
        let base = [10; 29];
        let info = Info { words: &words, inv: &inv, hp: [0; 4], base: &base, class: 0, level: 1, attack_words: &[] };
        assert_eq!(info.stat_with_item(1, 0), 17);
        assert_eq!(info.stat_with_item(1, 1), 10);
        // a second primary needs level 20 and a dual-wielding class
        let mut a = it(14, 0, 0);
        a.equip_slot = 14;
        let inv = ItemList { p04: 0, items: vec![a, it(14, 0, 0)] };
        assert_eq!(equip_slot_for(&inv, &inv.items[1], 0, 20), 15);
        assert_eq!(equip_slot_for(&inv, &inv.items[1], 9, 20), 14);
        assert_eq!(equip_slot_for(&inv, &inv.items[1], 0, 19), 14);
    }
}
