//! A merc's inventory and spell windows (go-eqoa-server's 0x7E72-0x7E77,
//! `handlers/mercwin.go`; the server owner's of 2026-11-15: "the merc
//! inventory should pop up like player inventory menu also show the merc
//! equipped gear. players can replace it and/or give them new equipment like a
//! player", and "also merc spell menu should be like players also with spell
//! book and 10 spells"). The server offers them at each world entry; this
//! client answers 1 and is sent the merc's whole state whenever it changes,
//! with a flag to open its window when the player asks (the merc's menu's
//! Inventory and Spells, `/merc <name> inv`, `/merc <name> spells`). Every move
//! is asked and the server judges it (the player's gates, two-handers, a full
//! pack, NO TRADE and LORE): nothing is changed here until the state comes
//! back. All little-endian; a name is a u8 byte count and its UTF-8 bytes.

use crate::dump::{Ability, NetItem, read_ability, read_net_item};
use crate::stream::{Reader, Writer};

/// The offer both ways: from the server a u8, 1, the layout of the states;
/// the answer a u8, the layout read (1).
pub const OP_MERC_WINDOW_OFFER: u16 = 0x7E72;
/// The inventory window's asks ([`ask`]).
pub const OP_MERC_WINDOW_ASK: u16 = 0x7E73;
/// The merc's inventory and figures ([`MercWindow`]).
pub const OP_MERC_WINDOW_STATE: u16 = 0x7E74;
/// An ask's answer: the ask's opcode's low byte, the ask, 1 done or 0 refused.
pub const OP_MERC_WINDOW_RESULT: u16 = 0x7E75;
/// The spell window's asks ([`spells_ask`]).
pub const OP_MERC_SPELLS_ASK: u16 = 0x7E76;
/// The merc's book and its ten slots ([`MercSpells`]).
pub const OP_MERC_SPELLS_STATE: u16 = 0x7E77;

/// The layout the server offers, and the spell window's state's.
pub const LAYOUT: u8 = 1;
/// The inventory state's layout this client answers the offer with: 1's with
/// the record kind [`USE`] (go-eqoa-server's petition 3285). A server that
/// knows only 1 sends 1, which reads the same.
pub const LAYOUT_USE: u8 = 2;

/// A state's flags: open its window; the hire is gone (or, the spell
/// window's, the merc not out): close it; the spell window's ten set by the
/// player (else they are the merc's default).
pub const OPEN: u8 = 1;
pub const GONE: u8 = 2;
pub const SET: u8 = 4;

/// A record's kind: carried, worn, an heirloom worn where the merc wears
/// nothing of its own (shown only; its id 0).
pub const CARRIED: u8 = 0;
pub const WORN: u8 = 1;
pub const HEIRLOOM: u8 = 2;
/// The owner's potion stock (an alchemist's window): its id the item's, its
/// count the record's stack.
pub const STOCK: u8 = 3;
/// Carried, and a meal or a grocer's drink the merc can be given to eat or
/// drink now ([`ASK_USE`]); layout 2's (layout 1 sends it [`CARRIED`]).
pub const USE: u8 = 4;

/// The inventory window's asks.
pub const ASK_OPEN: u8 = 1;
pub const ASK_GIVE: u8 = 2;
pub const ASK_EQUIP: u8 = 3;
pub const ASK_UNEQUIP: u8 = 4;
pub const ASK_TAKE: u8 = 5;
/// From the owner's potion stock into the pack: the item's id and how many
/// (0 a stack).
pub const ASK_STOCK: u8 = 6;
/// The window is down: no stock's change is sent it.
pub const ASK_CLOSE: u8 = 7;
/// The merc eats or drinks a carried meal or drink ([`USE`]) now.
pub const ASK_USE: u8 = 8;

/// The spell window's asks.
pub const SPELLS_OPEN: u8 = 1;
pub const SPELLS_SET: u8 = 2;
pub const SPELLS_CLEAR: u8 = 3;
pub const SPELLS_DEFAULT: u8 = 4;

/// The spell window's slots: a player's ten.
pub const SLOTS: usize = 10;

/// A record the merc holds or wears.
#[derive(Clone, Debug, Default)]
pub struct MercRecord {
    /// its id, which the asks name (0 an heirloom)
    pub id: u32,
    pub kind: u8,
    /// as 0x003B carries it: its equip slot the worn slot, -1 carried
    pub item: NetItem,
}

/// [`OP_MERC_WINDOW_STATE`]: the merc, its figures and what it holds.
#[derive(Clone, Debug, Default)]
pub struct MercWindow {
    pub flags: u8,
    /// its hire's slot (1-3), which the asks name
    pub slot: u8,
    pub name: String,
    pub class: u8,
    pub race: u8,
    pub level: u8,
    /// hit points, their most, power, its most, AC
    pub hp: u32,
    pub max_hp: u32,
    pub power: u32,
    pub max_power: u32,
    pub ac: u32,
    /// STR STA AGI DEX WIS INT CHA
    pub stats: [u32; 7],
    /// the records it holds at most
    pub most: u8,
    pub records: Vec<MercRecord>,
}

fn name(r: &mut Reader) -> String {
    let n = r.u8() as usize;
    String::from_utf8_lossy(r.bytes(n)).into_owned()
}

impl MercWindow {
    /// The state's body past the opcode; none for another layout or a short
    /// one.
    pub fn read(b: &[u8]) -> Option<MercWindow> {
        let mut r = Reader::new(b);
        if !(LAYOUT..=LAYOUT_USE).contains(&r.u8()) {
            return None;
        }
        let mut s = MercWindow { flags: r.u8(), slot: r.u8(), name: name(&mut r), class: r.u8(), race: r.u8(), level: r.u8(), ..Default::default() };
        s.hp = r.u32();
        s.max_hp = r.u32();
        s.power = r.u32();
        s.max_power = r.u32();
        s.ac = r.u32();
        for v in &mut s.stats {
            *v = r.u32();
        }
        s.most = r.u8();
        for _ in 0..r.u8() {
            if r.error {
                break;
            }
            let (id, kind) = (r.u32(), r.u8());
            s.records.push(MercRecord { id, kind, item: read_net_item(&mut r) });
        }
        (!r.error).then_some(s)
    }
}

/// A spell of the merc's book as the spell window lists it.
#[derive(Clone, Debug, Default)]
pub struct MercSpell {
    /// the ability record (its display index its place in the book; valid and
    /// slot while it is on one of the ten)
    pub ability: Ability,
    /// what it casts it for: "hit", "heal", "buff", "pet", "potion"...
    pub use_: String,
    /// turned off in its menu
    pub off: bool,
}

/// [`OP_MERC_SPELLS_STATE`]: the merc's book and its ten.
#[derive(Clone, Debug, Default)]
pub struct MercSpells {
    pub flags: u8,
    pub slot: u8,
    pub name: String,
    pub class: u8,
    pub level: u8,
    pub spells: Vec<MercSpell>,
    /// the ten slots' spell ids, 0 empty
    pub slots: [u32; SLOTS],
}

impl MercSpells {
    /// The state's body past the opcode; none for another layout or a short
    /// one.
    pub fn read(b: &[u8]) -> Option<MercSpells> {
        let mut r = Reader::new(b);
        if r.u8() != LAYOUT {
            return None;
        }
        let mut s = MercSpells { flags: r.u8(), slot: r.u8(), name: name(&mut r), class: r.u8(), level: r.u8(), ..Default::default() };
        for _ in 0..r.u8() {
            if r.error {
                break;
            }
            let ability = read_ability(&mut r);
            let use_ = name(&mut r);
            let off = r.u8() != 0;
            s.spells.push(MercSpell { ability, use_, off });
        }
        for v in &mut s.slots {
            *v = r.u32();
        }
        (!r.error).then_some(s)
    }

    /// The ten set by the player (else the merc's default).
    pub fn set(&self) -> bool {
        self.flags & SET != 0
    }
}

/// An inventory window's ask of the merc of `slot` (0 the player's only
/// one): `op`, then its fields (`ASK_GIVE` the pack record's key, `ASK_STOCK`
/// the item's id, the others the merc's record id; `ASK_EQUIP` an equip slot
/// after it, -1 the first of its wear; `ASK_STOCK` how many after it, 0 a
/// stack).
pub fn ask(slot: u8, op: u8, id: u32, equip: i8) -> Vec<u8> {
    let mut w = Writer::op(OP_MERC_WINDOW_ASK);
    w.u8(slot).u8(op);
    match op {
        ASK_GIVE | ASK_UNEQUIP | ASK_TAKE | ASK_USE => {
            w.u32(id);
        }
        ASK_EQUIP => {
            w.u32(id).u8(equip as u8);
        }
        ASK_STOCK => {
            w.u32(id).u32(equip.max(0) as u32);
        }
        _ => {}
    }
    w.data
}

/// A spell window's ask: `SPELLS_SET` puts spell `id` (0 none) on slot `at`.
pub fn spells_ask(slot: u8, op: u8, at: u8, id: u32) -> Vec<u8> {
    let mut w = Writer::op(OP_MERC_SPELLS_ASK);
    w.u8(slot).u8(op);
    if op == SPELLS_SET {
        w.u8(at).u32(id);
    }
    w.data
}

/// Builders of the server's bytes, for this crate's tests and the game's
/// (`eqoa`'s merc windows).
#[doc(hidden)]
pub mod testing {
    use super::*;

    /// A record as go-eqoa-server's `writeItem` writes it (`ReadNetItem`'s order).
    pub fn net_item(w: &mut Writer, it: &NetItem) {
        w.signed(it.quantity).signed(it.cur_hp).signed(it.charges).signed(it.equip_slot).signed(it.quick_slot).u32(it.inv_index);
        let i = &it.item;
        for v in [
            i.id, i.value, i.p08, i.icon, i.icon2, i.wear_slot, i.p18, i.no_trade, i.rent, i.p24, i.attack_type, i.damage, i.range, i.level_req,
            i.max_stack, i.max_hp, i.durability, i.class_mask, i.race_mask, i.p4c, i.lore, i.p58, i.p54,
        ] {
            w.signed(v);
        }
        w.wide(&i.name).wide(&i.desc).signed(i.stats.len() as i64);
        for s in &i.stats {
            w.signed(s.id).signed(s.value);
        }
    }

    /// An ability as go-eqoa-server's `writeAbilityRecord` writes it.
    pub fn ability(w: &mut Writer, a: &Ability) {
        for v in [a.id, a.display_index, a.valid, a.slot, a.lockout, a.p14, a.level] {
            w.signed(v);
        }
        w.u32(a.range.to_bits());
        for v in [a.cast_time, a.power_cost, a.icon_back, a.icon_front, a.scope, a.cooldown, a.equip_req] {
            w.signed(v);
        }
        w.wide(&a.name).wide(&a.desc).signed(0);
    }

    fn put_name(w: &mut Writer, s: &str) {
        w.u8(s.len() as u8).bytes(s.as_bytes());
    }

    /// A window state's body (past the opcode) of the records given.
    pub fn window_body(flags: u8, slot: u8, name: &str, records: &[MercRecord]) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(LAYOUT).u8(flags).u8(slot);
        put_name(&mut w, name);
        w.u8(0).u8(2).u8(30);
        for v in [800, 817, 1400, 1480, 240, 85, 131, 65, 102, 55, 149, 70] {
            w.u32(v);
        }
        w.u8(40).u8(records.len() as u8);
        for r in records {
            w.u32(r.id).u8(r.kind);
            net_item(&mut w, &r.item);
        }
        w.data
    }

    /// A spells state's body of the spells (id, level, slot or -1) and slots.
    pub fn spells_body(flags: u8, spells: &[(i64, i64, i64)], slots: [u32; SLOTS]) -> Vec<u8> {
        let mut w = Writer::default();
        w.u8(LAYOUT).u8(flags).u8(1);
        put_name(&mut w, "Kel");
        w.u8(13).u8(30).u8(spells.len() as u8);
        for (i, &(id, level, slot)) in spells.iter().enumerate() {
            let a = Ability { id, display_index: i as i64, valid: (slot >= 0) as i64, slot, p14: 1, level, name: format!("Spell {id}"), ..Default::default() };
            ability(&mut w, &a);
            put_name(&mut w, "hit");
            w.u8(0);
        }
        for s in slots {
            w.u32(s);
        }
        w.data
    }

    pub fn item(id: u32, kind: u8, name: &str, wear: i64, equip: i64) -> MercRecord {
        let mut it = NetItem { quantity: 1, equip_slot: equip, quick_slot: -1, inv_index: id, ..Default::default() };
        it.item.id = 900000 + id as i64;
        it.item.name = name.into();
        it.item.wear_slot = wear;
        it.item.class_mask = -1;
        it.item.race_mask = -1;
        it.item.max_stack = 1;
        MercRecord { id, kind, item: it }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::*;
    use super::*;

    /// The window's state read back whole, each record's item too; a short
    /// one, or another layout, none.
    #[test]
    fn the_window_state_reads() {
        let recs = vec![item(7, WORN, "Mithril Breastplate", 5, 5), item(0, HEIRLOOM, "Heirloom head", 1, 0), item(9, CARRIED, "Buckler", 13, -1)];
        let b = window_body(OPEN, 2, "Kel", &recs);
        let s = MercWindow::read(&b).expect("read");
        assert_eq!((s.flags, s.slot, s.name.as_str(), s.class, s.race, s.level), (OPEN, 2, "Kel", 0, 2, 30));
        assert_eq!((s.hp, s.max_hp, s.power, s.max_power, s.ac), (800, 817, 1400, 1480, 240));
        assert_eq!(s.stats, [85, 131, 65, 102, 55, 149, 70]);
        assert_eq!(s.most, 40);
        let got: Vec<_> = s.records.iter().map(|r| (r.id, r.kind, r.item.item.name.clone(), r.item.equip_slot, r.item.item.wear_slot, r.item.inv_index)).collect();
        let want: Vec<_> = recs.iter().map(|r| (r.id, r.kind, r.item.item.name.clone(), r.item.equip_slot, r.item.item.wear_slot, r.item.inv_index)).collect();
        assert_eq!(got, want);
        assert!(MercWindow::read(&b[..b.len() - 1]).is_none(), "short");
        let mut other = b.clone();
        other[0] = LAYOUT_USE;
        assert!(MercWindow::read(&other).is_some(), "layout 2 (the kind USE) reads too");
        other[0] = 3;
        assert!(MercWindow::read(&other).is_none(), "another layout");
    }

    /// The spells state read back: the book's abilities, their uses, the ten.
    #[test]
    fn the_spells_state_reads() {
        let mut slots = [0; SLOTS];
        slots[3] = 950;
        let b = spells_body(OPEN | SET, &[(950, 10, 3), (951, 20, -1)], slots);
        let s = MercSpells::read(&b).expect("read");
        assert!(s.set() && s.flags & OPEN != 0);
        assert_eq!((s.name.as_str(), s.class, s.level, s.slots), ("Kel", 13, 30, slots));
        assert_eq!(s.spells.len(), 2);
        assert_eq!((s.spells[0].ability.id, s.spells[0].ability.valid, s.spells[0].ability.slot, s.spells[0].use_.as_str()), (950, 1, 3, "hit"));
        assert_eq!((s.spells[1].ability.display_index, s.spells[1].ability.valid, s.spells[1].ability.slot), (1, 0, -1));
        assert!(MercSpells::read(&b[..b.len() - 2]).is_none(), "short");
    }

    /// The asks' bytes, as the server reads them.
    #[test]
    fn the_asks_are_built() {
        assert_eq!(ask(1, ASK_OPEN, 0, 0), vec![0x73, 0x7E, 1, 1]);
        assert_eq!(ask(0, ASK_GIVE, 0x0102_0304, 0), vec![0x73, 0x7E, 0, 2, 4, 3, 2, 1]);
        assert_eq!(ask(2, ASK_EQUIP, 7, -1), vec![0x73, 0x7E, 2, 3, 7, 0, 0, 0, 0xFF]);
        assert_eq!(ask(2, ASK_EQUIP, 7, 17), vec![0x73, 0x7E, 2, 3, 7, 0, 0, 0, 17]);
        assert_eq!(ask(1, ASK_UNEQUIP, 7, 0), vec![0x73, 0x7E, 1, 4, 7, 0, 0, 0]);
        assert_eq!(ask(1, ASK_TAKE, 7, 0), vec![0x73, 0x7E, 1, 5, 7, 0, 0, 0]);
        assert_eq!(ask(1, ASK_STOCK, 0x0101, 0), vec![0x73, 0x7E, 1, 6, 1, 1, 0, 0, 0, 0, 0, 0]);
        assert_eq!(ask(1, ASK_STOCK, 0x0101, 1), vec![0x73, 0x7E, 1, 6, 1, 1, 0, 0, 1, 0, 0, 0]);
        assert_eq!(ask(1, ASK_CLOSE, 0, 0), vec![0x73, 0x7E, 1, 7]);
        assert_eq!(ask(1, ASK_USE, 7, 0), vec![0x73, 0x7E, 1, 8, 7, 0, 0, 0]);
        assert_eq!(spells_ask(1, SPELLS_SET, 3, 950), vec![0x76, 0x7E, 1, 2, 3, 0xB6, 3, 0, 0]);
        assert_eq!(spells_ask(1, SPELLS_CLEAR, 0, 0), vec![0x76, 0x7E, 1, 3]);
        assert_eq!(spells_ask(1, SPELLS_DEFAULT, 0, 0), vec![0x76, 0x7E, 1, 4]);
    }
}
