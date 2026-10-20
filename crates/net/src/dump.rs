//! The MemoryDump (0x000D), read as `VIClient::Msg000D` reads it: the world,
//! `VIPlayer::Read`, the quick-chat sets, the quests, the inventory and the
//! bank (with the auction's lots and bids), a corpse's loot, the abilities,
//! the speed, the crafting slots and the Combat Mastery state.
//!
//! Every string limit is the decomp's `ReadString` size: a narrow string
//! fits when its length is under the buffer, a wide one when it is under
//! half the buffer's bytes; one that does not fit is skipped and read empty.
//! Names are the decomp's where it has them.

use crate::stream::Reader;

/// `VIPlayer::Read` (VIClient+0xC2DC).
#[derive(Clone, Debug, Default)]
pub struct Player {
    pub id: i64,
    pub name: String,
    pub class: i64,
    pub race: i64,
    pub level: i64,
    pub exp: i64,
    pub p2c: i64,
    pub p30: u8,
    pub p34: i64,
    pub p38: i64,
    pub p40: i64,
    pub p44: i64,
    pub world: i64,
    pub pos: [f32; 3],
    /// Heading, pitch, roll.
    pub hpr: [f32; 3],
}

/// One quick-chat entry (`VIRec388`).
#[derive(Clone, Debug, Default)]
pub struct TextEntry {
    pub id: i64,
    pub text: String,
    pub label: String,
}

/// One quick-chat direction (`VIRec6A`): always four entries.
#[derive(Clone, Debug, Default)]
pub struct TextSet {
    pub id: i64,
    pub entries: Vec<TextEntry>,
}

/// `VIItemStat`.
#[derive(Clone, Copy, Debug, Default)]
pub struct ItemStat {
    pub id: i64,
    pub value: i64,
}

/// `VIItem`'s stream form (`operator>>(VIMemStream &, VIItem &)`).
#[derive(Clone, Debug, Default)]
pub struct Item {
    pub id: i64,
    pub value: i64,
    pub p08: i64,
    pub icon: i64,
    pub icon2: i64,
    pub wear_slot: i64,
    pub p18: i64,
    pub no_trade: i64,
    pub rent: i64,
    pub p24: i64,
    pub attack_type: i64,
    pub damage: i64,
    pub range: i64,
    pub level_req: i64,
    pub max_stack: i64,
    pub max_hp: i64,
    pub durability: i64,
    pub class_mask: i64,
    pub race_mask: i64,
    pub p4c: i64,
    pub lore: i64,
    pub p58: i64,
    pub p54: i64,
    pub name: String,
    pub desc: String,
    pub stats: Vec<ItemStat>,
}

/// `ReadNetItem`: the slot's six words, then the item.
#[derive(Clone, Debug, Default)]
pub struct NetItem {
    pub quantity: i64,
    pub cur_hp: i64,
    pub charges: i64,
    pub equip_slot: i64,
    pub quick_slot: i64,
    pub inv_index: u32,
    pub item: Item,
}

/// `VIItemList::Read`.
#[derive(Clone, Debug, Default)]
pub struct ItemList {
    pub p04: u32,
    pub items: Vec<NetItem>,
}

/// An auction lot (`VIAuctionItem`).
#[derive(Clone, Debug, Default)]
pub struct AuctionItem {
    pub lot_id: i64,
    pub quantity: i64,
    pub cur_hp: i64,
    pub charges: i64,
    pub p2f4: i64,
    pub p2f8: i64,
    pub p2fc: i64,
    pub seller: String,
    pub item: Item,
}

/// A CHARACTER_BIDS row (`VIDbCharBid`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Bid {
    pub character_id: i64,
    pub auction_id: i64,
    pub x08: i64,
    pub x0c: i64,
    pub x10: i64,
    pub x14: u8,
}

/// `VIBankControl::Read`.
#[derive(Clone, Debug, Default)]
pub struct Bank {
    pub items: ItemList,
    pub lots: Vec<AuctionItem>,
    pub bids: Vec<Bid>,
    pub p78cc: bool,
}

/// One of an ability's records (`VIRec144_BE08C8`).
#[derive(Clone, Debug, Default)]
pub struct AbilityPart {
    pub p00: i64,
    pub p04: i64,
    pub p08: i64,
    pub p0c: i64,
    pub name: String,
}

/// `VIAbility::Read`.
#[derive(Clone, Debug, Default)]
pub struct Ability {
    pub id: i64,
    pub display_index: i64,
    pub valid: i64,
    pub slot: i64,
    pub lockout: i64,
    pub p14: i64,
    pub level: i64,
    pub range: f32,
    pub cast_time: i64,
    pub power_cost: i64,
    pub icon_back: i64,
    pub icon_front: i64,
    pub scope: i64,
    pub cooldown: i64,
    pub equip_req: i64,
    pub name: String,
    pub desc: String,
    pub parts: Vec<AbilityPart>,
}

/// `VIClientPart3834C::m_slots`: one crafting slot.
#[derive(Clone, Copy, Debug, Default)]
pub struct CraftSlot(pub [u32; 4]);

/// The stat channel (0x42, 233 bytes): `VIClient::ReadStats` into
/// `VIPlayer`. Indices 0-6 STR STA AGI DEX WIS INT CHA, 7 HP, 8 max HP,
/// 9 power, 10 max power.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub stat: [i32; 29],
    pub base: [i32; 29],
    pub p30: u8,
}

pub fn read_stats(r: &mut Reader) -> Stats {
    let mut s = Stats::default();
    for v in &mut s.stat {
        *v = r.u32() as i32;
    }
    for v in &mut s.base {
        *v = r.u32() as i32;
    }
    s.p30 = r.u8();
    s
}

/// The active effects (state channel 0x43): `VIClient`'s `m_effectCount`,
/// `m_effectIcon` (an icon's DictID) and `m_effectName`.
#[derive(Clone, Debug, Default)]
pub struct Effects {
    pub count: i32,
    pub icon: [u32; 8],
    pub name: [Vec<u16>; 8],
}

/// `VIClient::Channel43`: the count, at most 8, then a 132-byte record for
/// each, the icon and the name (`ReadString(name, 64)`: a wide string of
/// at most 31 characters, an empty one for a longer). As `VIMemStream`
/// reads: a read past the end leaves what it would have written as it was,
/// and every read after one has failed fails too.
pub fn read_effects(e: &mut Effects, data: &[u8]) {
    let mut s = MemStream { data, pos: 0, error: false };
    if let Some(v) = s.u32() {
        e.count = v as i32;
    }
    e.count = if e.count < 9 { e.count } else { 8 };
    for i in 0..e.count.max(0) as usize {
        if let Some(v) = s.u32() {
            e.icon[i] = v;
        }
        match s.u32() {
            // n + 1 <= MaxBytes / 2, unsigned
            Some(n) if n.wrapping_add(1) <= 32 => match s.take((n as usize).saturating_mul(2)) {
                Some(b) => e.name[i] = b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect(),
                // the characters are not read, and dst[n] = 0 cuts what was there
                None => e.name[i].truncate(n as usize),
            },
            Some(n) => {
                s.take((n as usize).saturating_mul(2));
                e.name[i].clear();
            }
            // The length is not read, and the console goes on with whatever
            // its slot held: undefined, so the name is left as it was.
            None => {}
        }
        s.pos = 4 + (i + 1) * 132;
    }
}

/// `VIMemStream::Read`/`Skip`: nothing taken past the end, and nothing
/// after an error.
struct MemStream<'a> {
    data: &'a [u8],
    pos: usize,
    error: bool,
}

impl<'a> MemStream<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if !self.error && self.data.len() as i64 - self.pos as i64 >= n as i64 {
            self.pos += n;
            Some(&self.data[self.pos - n..self.pos])
        } else {
            self.error = true;
            None
        }
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }
}

/// `VIClientPart4EC58::ReadState`: Combat Mastery.
#[derive(Clone, Debug, Default)]
pub struct Mastery {
    pub p41c: u32,
    pub x0c: i64,
    pub x10: i64,
    pub x14: i64,
    pub x08: i64,
    pub speed: f32,
    pub stats: [i64; 29],
    pub bonuses: [i64; 7],
}

/// The whole MemoryDump.
#[derive(Clone, Debug, Default)]
pub struct MemoryDump {
    /// `m_p0662C`: the world is not on the disc, so `world_path` is loaded.
    pub world_not_on_disc: bool,
    pub world_path: String,
    pub player: Player,
    pub text_sets: Vec<TextSet>,
    pub p2bb60: u32,
    pub p2bb0c: u32,
    pub quests: Vec<String>,
    pub inventory: ItemList,
    pub bank: Bank,
    /// A corpse to loot, when the player has one.
    pub corpse: Option<ItemList>,
    pub abilities: Vec<Ability>,
    pub speed: f32,
    pub craft_p78c0: u32,
    pub craft_slots: [CraftSlot; 6],
    pub mastery: Mastery,
}

fn narrow(r: &mut Reader, max: usize) -> String {
    String::from_utf8_lossy(&r.string(max)).into_owned()
}

/// A wide `ReadString(buf, bytes)`.
fn wide(r: &mut Reader, bytes: usize) -> String {
    r.wide(bytes / 2)
}

fn f32x3(r: &mut Reader) -> [f32; 3] {
    [r.f32(), r.f32(), r.f32()]
}

/// A zigzag count; a negative one reads as none (`VIArray::Init` refuses it).
fn count(r: &mut Reader) -> usize {
    r.signed().max(0) as usize
}

pub fn read_player(r: &mut Reader) -> Player {
    Player {
        id: r.signed(),
        name: narrow(r, 24),
        class: r.signed(),
        race: r.signed(),
        level: r.signed(),
        exp: r.signed(),
        p2c: r.signed(),
        p30: r.u8(),
        p34: r.signed(),
        p38: r.signed(),
        p40: r.signed(),
        p44: r.signed(),
        world: r.signed(),
        pos: f32x3(r),
        hpr: f32x3(r),
    }
}

pub fn read_item(r: &mut Reader) -> Item {
    let mut f = [0i64; 23];
    for v in &mut f {
        *v = r.signed();
    }
    let name = wide(r, 128);
    let desc = wide(r, 512);
    let n = count(r);
    let mut stats = Vec::new();
    for _ in 0..n {
        if r.error {
            break;
        }
        stats.push(ItemStat { id: r.signed(), value: r.signed() });
    }
    Item {
        id: f[0],
        value: f[1],
        p08: f[2],
        icon: f[3],
        icon2: f[4],
        wear_slot: f[5],
        p18: f[6],
        no_trade: f[7],
        rent: f[8],
        p24: f[9],
        attack_type: f[10],
        damage: f[11],
        range: f[12],
        level_req: f[13],
        max_stack: f[14],
        max_hp: f[15],
        durability: f[16],
        class_mask: f[17],
        race_mask: f[18],
        p4c: f[19],
        lore: f[20],
        p58: f[21],
        p54: f[22],
        name,
        desc,
        stats,
    }
}

pub fn read_net_item(r: &mut Reader) -> NetItem {
    NetItem {
        quantity: r.signed(),
        cur_hp: r.signed(),
        charges: r.signed(),
        equip_slot: r.signed(),
        quick_slot: r.signed(),
        inv_index: r.u32(),
        item: read_item(r),
    }
}

pub fn read_item_list(r: &mut Reader) -> ItemList {
    let n = count(r);
    let p04 = r.u32();
    let mut items = Vec::new();
    for _ in 0..n {
        if r.error {
            break;
        }
        items.push(read_net_item(r));
    }
    ItemList { p04, items }
}

/// `operator>>(VIMemStream &, VIAuctionItem &)`: seven zigzag words, the
/// seller, the item.
pub fn read_auction_item(r: &mut Reader) -> AuctionItem {
    AuctionItem {
        lot_id: r.signed(),
        quantity: r.signed(),
        cur_hp: r.signed(),
        charges: r.signed(),
        p2f4: r.signed(),
        p2f8: r.signed(),
        p2fc: r.signed(),
        seller: narrow(r, 24),
        item: read_item(r),
    }
}

/// `operator>>(VIMemStream &, VIDbCharBid &)`: the key's two words, three
/// more, a byte.
pub fn read_bid(r: &mut Reader) -> Bid {
    Bid { character_id: r.signed(), auction_id: r.signed(), x08: r.signed(), x0c: r.signed(), x10: r.signed(), x14: r.u8() }
}

fn read_bank(r: &mut Reader) -> Bank {
    let items = read_item_list(r);
    let mut lots = Vec::new();
    for _ in 0..count(r) {
        if r.error {
            break;
        }
        lots.push(read_auction_item(r));
    }
    let mut bids = Vec::new();
    for _ in 0..count(r) {
        if r.error {
            break;
        }
        bids.push(read_bid(r));
    }
    let p78cc = r.u8() != 0;
    Bank { items, lots, bids, p78cc }
}

pub fn read_ability(r: &mut Reader) -> Ability {
    let mut a = Ability {
        id: r.signed(),
        display_index: r.signed(),
        valid: r.signed(),
        slot: r.signed(),
        lockout: r.signed(),
        p14: r.signed(),
        level: r.signed(),
        range: r.f32(),
        cast_time: r.signed(),
        power_cost: r.signed(),
        icon_back: r.signed(),
        icon_front: r.signed(),
        scope: r.signed(),
        cooldown: r.signed(),
        equip_req: r.signed(),
        name: wide(r, 128),
        desc: wide(r, 256),
        parts: Vec::new(),
    };
    for _ in 0..count(r) {
        if r.error {
            break;
        }
        a.parts.push(AbilityPart { p00: r.signed(), p04: r.signed(), p08: r.signed(), p0c: r.signed(), name: wide(r, 128) });
    }
    a
}

fn read_mastery(r: &mut Reader) -> Mastery {
    let mut m = Mastery { p41c: r.u32(), x0c: r.signed(), x10: r.signed(), x14: r.signed(), x08: r.signed(), speed: r.f32(), ..Default::default() };
    for v in &mut m.stats {
        *v = r.signed();
    }
    for v in &mut m.bonuses {
        *v = r.signed();
    }
    m
}

/// `Msg000D`, from after the opcode. `r.error` says whether it read short.
pub fn read(r: &mut Reader) -> MemoryDump {
    let mut d = MemoryDump { world_not_on_disc: r.u8() != 0, world_path: narrow(r, 256), player: read_player(r), ..Default::default() };
    for _ in 0..count(r) {
        if r.error {
            break;
        }
        let id = r.signed();
        let entries = (0..4).map(|_| TextEntry { id: r.signed(), text: wide(r, 256), label: wide(r, 128) }).collect();
        d.text_sets.push(TextSet { id, entries });
    }
    d.p2bb60 = r.u32();
    d.p2bb0c = r.u32();
    let quests = r.u32() as i32;
    for _ in 0..quests.max(0) {
        if r.error {
            break;
        }
        d.quests.push(wide(r, 256));
    }
    d.inventory = read_item_list(r);
    d.bank = read_bank(r);
    if r.u8() != 0 {
        d.corpse = Some(read_item_list(r));
    }
    for _ in 0..count(r) {
        if r.error {
            break;
        }
        d.abilities.push(read_ability(r));
    }
    d.speed = r.f32();
    d.craft_p78c0 = r.u32();
    for s in &mut d.craft_slots {
        *s = CraftSlot([r.u32(), r.u32(), r.u32(), r.u32()]);
    }
    d.mastery = read_mastery(r);
    d
}
