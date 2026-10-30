//! The entities on state channels 0x00-0x17: `VIClient::ChannelEntity` and
//! the two readers (`VIEntity::ReadUpdate`, `VICreatureEntity::ReadUpdate`,
//! decomp `modules/client/src/{viclient,vientity,vicreatureentity}.cpp`),
//! and `VIClient::Extrapolate`'s blend from where an entity was drawn to
//! where its newest update puts it.
//!
//! CLIENT keeps 24 entity slots, the state channels 0x00-0x17
//! (`m_entities[24]`; `ProcessStateChannel` hands only `type < 24` to
//! `ChannelEntity`). The client keeps up to [`MAX_SLOTS`] on a server that
//! offers them (go-eqoa-server's 0x7E00, `session::OP_ENTITY_VIEW`): slot n travels on
//! [`channel_of_slot`]`(n)`, slots 24-63 on 0x18-0x3F and 64-203 on
//! 0x6C-0xF7, every DRDP state channel CLIENT reads as nothing else (0x40 is
//! the group, 0x42 the stats, 0x43 the effects, 0x44-0x6B `Channel44`'s
//! records). How many it reads is [`Entities::limit`].

use crate::stream::Reader;

/// CLIENT's entity slots, channels 0x00-0x17.
pub const RETAIL_SLOTS: usize = 24;
/// Every slot a state channel can carry: 0x00-0x3F and 0x6C-0xF7.
pub const MAX_SLOTS: usize = 0x40 + (0xF8 - 0x6C);
pub const TYPE_ENTITY: u8 = 129;
pub const TYPE_CREATURE: u8 = 130;

/// The entity slot state channel `ch` carries: 0x00-0x3F are slots 0-63,
/// 0x6C-0xF7 slots 64-203; none for the others.
pub fn slot_of_channel(ch: u8) -> Option<usize> {
    match ch {
        0x00..=0x3F => Some(ch as usize),
        0x6C..=0xF7 => Some(ch as usize - 0x6C + 0x40),
        _ => None,
    }
}

/// The state channel entity slot `slot` travels on.
pub fn channel_of_slot(slot: usize) -> Option<u8> {
    match slot {
        0..0x40 => Some(slot as u8),
        0x40..MAX_SLOTS => Some((slot - 0x40 + 0x6C) as u8),
        _ => None,
    }
}

/// The live game's look (go-eqoa-server's 0x7E0C, [`crate::session::OP_LIVE_LOOK`]):
/// SOE's live CLIENT (eqoa-live-1500, in the Corsten and Lostologist save
/// states) reads a colour (`VIColor32`: r, g, b, a) for each held item, each
/// armour slot and the robe where the disc's reads a tint, and per armour
/// slot the variant its ArmorGroup is picked by (`GetArmorGroupResourceID`,
/// 0-3; the Frontiers disc's art has none, so it picks nothing). Its
/// `ApplyLook` hands each colour to `SetArmorSlot` / `SetRobe` / `SetHelm`,
/// which with no ArmorGroup make it the slot's layer colour, where the disc's
/// take `GetTintColor(tint)`; nothing draws the held items' colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LiveColors {
    /// The three attach items' (primary, secondary, shield), `+0x296`.
    pub items: [[u8; 4]; 3],
    /// Chest, bracer, gloves, legs, boots, helm (the armour sets' order), `+0x2A2`.
    pub armor: [[u8; 4]; 6],
    /// Each slot's ArmorGroup variant, `+0x2BE`.
    pub variants: [u8; 6],
    /// The robe's, `+0x2BA`.
    pub robe: [u8; 4],
}

/// The live game's entity channel: 201 bytes (the disc's 162).
pub const LIVE_STATE_SIZE: usize = 201;

/// `VICreatureEntity`'s part of the update.
#[derive(Clone, Debug, Default)]
pub struct Creature {
    pub velocity: [f32; 3],
    pub angular_vel: [f32; 3],
    pub accel: [f32; 3],
    pub angular_accel: [f32; 3],
    pub floor: f32,
    pub target_heading: f32,
    pub state: u8,
    pub target: u32,
    pub walk_flags: u32,
    pub items: [u32; 3],
    pub items_over: [u32; 3],
    pub item_types: [u8; 2],
    pub armor: [u8; 6],
    pub armor_tint: [u8; 6],
    pub hair: [u8; 3],
    pub face: u8,
    pub robe: u8,
    pub robe_tint: u8,
    /// The live game's colours, when the channel was the live game's (then
    /// `armor_tint` and `robe_tint` are 0: it carries none).
    pub colors: Option<LiveColors>,
    pub name: String,
    pub level: u8,
    /// +0x255: runs its own walk physics (players) rather than being carried
    /// along its velocity.
    pub p255: u8,
    /// +0x256 and +0x299: pick the name plate's colour (`ApplyLook`).
    pub p256: u8,
    pub p299: u8,
    pub race: u8,
    pub class: u8,
    pub npc_flags: u16,
    /// +0x263: in combat (`Byte263`): the battle music plays while the
    /// player's is set.
    pub p263: u8,
    /// +0x262: `Flag4` (& 4) and `Flag8` (& 8), the bars' two status icons.
    pub flags262: u8,
    /// +0x264: a player that can be invited (`InviteEntity` refuses 0).
    pub p264: u8,
}

#[derive(Clone, Debug, Default)]
pub struct Entity {
    pub id: u32,
    pub kind: u8,
    pub pos: [f32; 3],
    /// Heading and pitch (roll is 0).
    pub hpr: [f32; 3],
    pub world: i8,
    /// +0x18 (byte 26): bit 0 selectable, bit 1 a corpse.
    pub p18: u8,
    /// +0x19 (byte 27): `TargetInRange` refuses a target where it is 0.
    pub p19: u8,
    pub model: u32,
    /// `m_model2`: when not 0 the body drawn in `m_model`'s place
    /// (`GetModel`).
    pub model2: u32,
    /// The body the game draws (the character cache's model): `GetModel()`
    /// as the actor was made with it, then as `Changed` sets it after each
    /// update, which is only when it differs from the update's OLD `m_model`
    /// (`ReadUpdate` keeps `int oldModel = m_model`, not `GetModel()`). So a
    /// `model2` set is drawn at once, and one cleared to 0 stays drawn until
    /// `model` itself changes or the entity is made again.
    pub drawn_model: u32,
    pub scale: f32,
    pub creature: Option<Creature>,
    /// `Extrapolate`: where it was drawn minus where it now is, run down to
    /// nothing by `stamp`.
    pub vel: [f32; 3],
    pub rot: [f32; 3],
    pub stamp: u32,
    /// Drawn this frame.
    pub actor_pos: [f32; 3],
    pub actor_hpr: [f32; 3],
    /// `m_extrapolated`: the scene time the position was last carried to.
    pub extrapolated: i64,
}

/// `VIEntity::Dequantize`: a signed 24-bit big-endian value over 128.
fn dequantize(v: &[u8]) -> f32 {
    let fixed = ((v[0] as i8 as i32) << 16) | ((v[1] as i32) << 8) | v[2] as i32;
    fixed as f32 / 128.0
}

/// `DequantizeVect`: `n` signed bytes, each times range/128.
fn dequantize_vect(v: &[u8], range: f32) -> [f32; 3] {
    let k = range * (1.0 / 128.0);
    let mut out = [0f32; 3];
    for (o, &b) in out.iter_mut().zip(v) {
        *o = b as i8 as f32 * k;
    }
    out
}

fn wrap(mut a: f32) -> f32 {
    let pi = std::f32::consts::PI;
    while a < -pi {
        a += 2.0 * pi;
    }
    while a >= pi {
        a -= 2.0 * pi;
    }
    a
}

impl Entity {
    /// `VIEntity::GetModel`: `model2` when it is not 0, else `model`.
    pub fn model(&self) -> u32 {
        if self.model2 != 0 { self.model2 } else { self.model }
    }

    /// `ReadUpdate`: the disc's CLIENT's, or with `live` the live game's
    /// (`VICreatureEntity::ReadUpdate`, Corsten's 0x013D2870), which reads
    /// [`LiveColors`] after the armour sets in place of the tints, and no
    /// robe tint.
    fn read_update(&mut self, r: &mut Reader, live: bool) {
        // ReadUpdate: what `Changed` is handed
        let (old_world, old_model) = (self.world, self.model);
        for i in 0..3 {
            self.pos[i] = dequantize(r.bytes(3));
        }
        self.hpr = [0.0; 3];
        for i in 0..2 {
            self.hpr[i] = r.u8() as i8 as f32 * (std::f32::consts::PI / 128.0);
        }
        // ReadState
        self.world = r.u8() as i8;
        r.u64();
        self.p18 = r.u8();
        self.p19 = r.u8();
        self.model = r.u32();
        self.model2 = r.u32();
        self.scale = r.f32();
        // Changed: another dimension makes the actor again (`OnWorldLoaded`,
        // `CreateActor` with `GetModel()`); else the body follows `GetModel()`
        // only when that differs from the old `m_model`
        let model = self.model();
        if old_world != self.world || old_model != model {
            self.drawn_model = model;
        }
        if self.kind == TYPE_CREATURE {
            let mut c = Creature {
                velocity: dequantize_vect(r.bytes(3), 21.0),
                angular_vel: { let mut v = r.bytes(2).to_vec(); v.push(0); dequantize_vect(&v, 8.0) },
                accel: dequantize_vect(r.bytes(3), 63.0),
                angular_accel: { let mut v = r.bytes(2).to_vec(); v.push(0); dequantize_vect(&v, 32.0) },
                floor: r.f32(),
                target_heading: r.f32(),
                state: r.u8(),
                target: r.u32(),
                walk_flags: r.u32(),
                ..Default::default()
            };
            for i in 0..3 {
                c.items[i] = r.u32();
            }
            for i in 0..3 {
                c.items_over[i] = r.u32();
            }
            c.item_types = [r.u8(), r.u8()];
            c.armor.copy_from_slice(&pad6(r.bytes(6)));
            if live {
                // four single-byte reads each, r first
                let rgba = |r: &mut Reader| [r.u8(), r.u8(), r.u8(), r.u8()];
                let mut l = LiveColors::default();
                for i in 0..3 {
                    l.items[i] = rgba(r);
                }
                for i in 0..6 {
                    l.armor[i] = rgba(r);
                    l.variants[i] = r.u8();
                }
                l.robe = rgba(r);
                c.colors = Some(l);
                c.hair = [r.u8(), r.u8(), r.u8()];
                c.face = r.u8();
                c.robe = r.u8();
            } else {
                c.armor_tint.copy_from_slice(&pad6(r.bytes(6)));
                c.hair = [r.u8(), r.u8(), r.u8()];
                c.face = r.u8();
                c.robe = r.u8();
                c.robe_tint = r.u8();
            }
            r.u32();
            r.u8();
            let name = r.bytes(24);
            c.name = String::from_utf8_lossy(&name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())]).into_owned();
            c.level = r.u8();
            c.p255 = r.u8();
            c.p256 = r.u8();
            r.u8();
            c.race = r.u8();
            c.class = r.u8();
            c.npc_flags = r.u16();
            r.u32();
            // m_p260, m_p261, m_flags262, m_p263, m_p264
            r.u8();
            r.u8();
            c.flags262 = r.u8();
            c.p263 = r.u8();
            c.p264 = r.u8();
            c.p299 = r.u8();
            self.creature = Some(c);
        }
    }

    /// `VIClient::Extrapolate` for one entity at `now`.
    pub fn extrapolate(&mut self, now: u32, rate: u32) {
        let mut t = self.stamp.wrapping_sub(now) as i32 as f32 / (rate * 2) as f32;
        if t < 0.0 {
            t = 0.0;
        }
        for i in 0..3 {
            self.actor_pos[i] = self.pos[i] + self.vel[i] * t;
            self.actor_hpr[i] = self.hpr[i] + self.rot[i] * t;
        }
    }
}

fn pad6(b: &[u8]) -> [u8; 6] {
    let mut o = [0u8; 6];
    o[..b.len().min(6)].copy_from_slice(&b[..b.len().min(6)]);
    o
}

/// The slots: [`MAX_SLOTS`] of them, the first `limit` read.
pub struct Entities {
    pub slots: Vec<Option<Entity>>,
    /// How many slots are read: CLIENT's 24, or as many as a server that
    /// offers more and the player's NPC/PC Display have agreed.
    pub limit: usize,
    /// The server told this client the live game's look is in force
    /// (`Session::live_look`): a channel of [`LIVE_STATE_SIZE`] bytes or more
    /// is then read as the live CLIENT reads it; any other as the disc's, as
    /// every channel is on a server that never offered it (Sandstorm).
    pub live_look: bool,
}

impl Default for Entities {
    fn default() -> Self {
        Entities { slots: vec![None; MAX_SLOTS], limit: RETAIL_SLOTS, live_look: false }
    }
}

impl Entities {
    /// Reads the first `n` slots (within CLIENT's 24 and [`MAX_SLOTS`]);
    /// what was on the others is gone (the server is told, and empties them).
    pub fn set_limit(&mut self, n: usize) {
        self.limit = n.clamp(RETAIL_SLOTS, MAX_SLOTS);
        for s in &mut self.slots[self.limit..] {
            *s = None;
        }
    }

    /// `VIClient::ChannelEntity` for slot `ch` at time `now`. Nothing is read
    /// before the player's own id is known, nor for a slot past `limit`; a
    /// message under 5 bytes empties the slot; a different id or type, or flag
    /// 0, replaces it (flag 0 then leaves it empty). Types other than 129/130
    /// crash the PS2; they are dropped here.
    /// `sync` runs after the read, as `VICreatureEntity::Sync` does inside
    /// `ReadUpdate` (the ground snap), before the blend is measured.
    pub fn channel(&mut self, ch: usize, data: &[u8], self_id: Option<u64>, now: u32, rate: u32, mut sync: impl FnMut(&mut Entity)) {
        let Some(self_id) = self_id else { return };
        if ch >= self.limit {
            return;
        }
        if data.len() < 5 {
            self.slots[ch] = None;
            return;
        }
        let mut r = Reader::new(data);
        let (flag, id, kind) = (r.u8(), r.u32(), r.u8());
        if let Some(e) = &self.slots[ch]
            && (e.kind != kind || e.id != id || flag == 0)
        {
            self.slots[ch] = None;
            if flag == 0 {
                return;
            }
        }
        if kind != TYPE_ENTITY && kind != TYPE_CREATURE {
            return;
        }
        let created = self.slots[ch].is_none();
        let e = self.slots[ch].get_or_insert_with(|| Entity { id, kind, ..Default::default() });
        e.id = id;
        let old_vel = e.creature.as_ref().map(|c| c.velocity);
        e.read_update(&mut r, self.live_look && data.len() >= LIVE_STATE_SIZE);
        sync(e);
        if created {
            e.vel = [0.0; 3];
            e.rot = [0.0; 3];
            e.actor_pos = e.pos;
            e.actor_hpr = e.hpr;
        } else {
            for i in 0..3 {
                e.vel[i] = e.actor_pos[i] - e.pos[i];
                e.rot[i] = wrap(e.actor_hpr[i] - e.hpr[i]);
            }
        }
        e.stamp = now.wrapping_add(rate * 2);
        if e.creature.as_ref().map(|c| c.velocity) == old_vel && old_vel.is_some() {
            e.vel = [0.0; 3];
        }
        let _ = self_id;
    }

    /// `Extrapolate` for every entity but the player.
    pub fn extrapolate(&mut self, self_id: Option<u64>, now: u32, rate: u32) {
        for e in self.slots.iter_mut().flatten() {
            if Some(e.id as u64) != self_id.map(|s| s & 0xffff_ffff) {
                e.extrapolate(now, rate);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every slot has a channel of its own, CLIENT's 24 their own 0x00-0x17,
    /// and no slot is a channel CLIENT reads as something else (0x40-0x6B) or
    /// a DRDP transport (0xF8 and up).
    #[test]
    fn slots_and_channels() {
        assert_eq!(MAX_SLOTS, 204);
        let mut seen = [None; 256];
        for slot in 0..MAX_SLOTS {
            let ch = channel_of_slot(slot).unwrap();
            if slot < RETAIL_SLOTS {
                assert_eq!(ch as usize, slot, "CLIENT's slot {slot}");
            }
            assert!(!(0x40..0x6C).contains(&ch) && ch < 0xF8, "slot {slot} on {ch:#x}");
            assert_eq!(seen[ch as usize].replace(slot), None, "two slots on {ch:#x}");
            assert_eq!(slot_of_channel(ch), Some(slot));
        }
        for ch in 0..=255u8 {
            assert_eq!(slot_of_channel(ch).is_some(), seen[ch as usize].is_some(), "{ch:#x}");
        }
        for ch in [0x40, 0x42, 0x43, 0x44, 0x6B, 0xF8, 0xFB] {
            assert_eq!(slot_of_channel(ch), None, "{ch:#x} carries no slot");
        }
        assert_eq!(channel_of_slot(MAX_SLOTS), None);
    }

    fn creature(id: u32) -> Vec<u8> {
        let mut v = vec![0u8; 162];
        v[0] = 1;
        v[1..5].copy_from_slice(&id.to_le_bytes());
        v[5] = TYPE_CREATURE;
        v
    }

    /// The body drawn (`drawn_model`) as `Changed` keeps it: `GetModel()`
    /// (`model2` when not 0) when the actor is made, then only where that
    /// differs from the update's OLD `m_model`. So a `model2` set is drawn at
    /// once, one cleared stays drawn until the model itself changes, and an
    /// entity from another dimension is made again (`OnWorldLoaded`). The
    /// steps where `GetModel()` read afresh would differ are the clears.
    #[test]
    fn the_body_drawn_follows_changed() {
        let (human, elf, rat) = (0x70D8_98C6u32, 0x156B_9CA9u32, 0x1234_5678u32);
        let state = |model: u32, model2: u32, world: i8| {
            let mut v = creature(500);
            v[17] = world as u8;
            v[28..32].copy_from_slice(&model.to_le_bytes());
            v[32..36].copy_from_slice(&model2.to_le_bytes());
            v
        };
        let mut e = Entities::default();
        let mut step = |model, model2, world| {
            e.channel(3, &state(model, model2, world), Some(7), 0, 100, |_| {});
            let x = e.slots[3].as_ref().unwrap();
            (x.model(), x.drawn_model)
        };
        assert_eq!(step(human, rat, 0), (rat, rat), "made with GetModel()");
        assert_eq!(step(human, rat, 0), (rat, rat));
        assert_eq!(step(human, 0, 0), (human, rat), "model2 cleared: still drawn (the old model is the new GetModel())");
        assert_eq!(step(elf, 0, 0), (elf, elf), "the model changed");
        assert_eq!(step(elf, rat, 0), (rat, rat), "model2 set: at once");
        assert_eq!(step(elf, 0, 0), (elf, rat), "cleared again: still drawn");
        assert_eq!(step(elf, 0, 2), (elf, elf), "another dimension: made again");
        assert_eq!(step(0, 0, 2), (0, 0), "model 0: nothing");
    }

    /// go-eqoa-server's `entity.LiveChannel` for a dyed player (its
    /// `livelook_test.go` player: chest d10000ff, boots 5a5a78ff, robe
    /// 008221ff, the primary 0000ffff) and the disc's channel of the same
    /// player (`SerializePlayerUpdate`).
    const LIVE_DYED: &str = "01a186010082003200000a000096000000000000000000000000017f00000000000000000000803f00000000000000000000fffa5842c2ca21c000000000000000000044332211887766550000000000000000000000000000000003000404030201040000ffffffffffffffffffffd10000ff00ffffffff00ffffffff00ffffffff005a5a78ff00ffffffff00008221ff0102030202060000000054696e63746f7269610000000000000000000000000000002a000201030d0000ffffffff00180000010074737271";
    const DISC_DYED: &str = "01a186010082003200000a000096000000000000000000000000017f00000000000000000000803f00000000000000000000fffa5842c2ca21c00000000000000000004433221188776655000000000000000000000000000000000300040403020104000000000000010203020201060000000054696e63746f7269610000000000000000000000000000002a000201030d0000ffffffff00180000010074737271";

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    /// The live game's channel, as the server sends it to a client of the
    /// live look, reads as the live CLIENT's ReadUpdate reads it: the entity
    /// the disc's channel of the same player reads as, its colours, its
    /// variants, and no tints. The controls: the same bytes before the look
    /// is in force are the disc's reading (the name lands elsewhere), and a
    /// channel of the disc's 162 bytes is the disc's whatever is in force (a
    /// server that never offered the look, Sandstorm).
    #[test]
    fn the_live_channel() {
        let (live, disc) = (unhex(LIVE_DYED), unhex(DISC_DYED));
        assert_eq!((live.len(), disc.len()), (LIVE_STATE_SIZE, 162));
        let read = |data: &[u8], live_look: bool| {
            let mut e = Entities { live_look, ..Default::default() };
            e.channel(1, data, Some(7), 0, 100, |_| {});
            e.slots[1].clone().unwrap()
        };
        let (l, d) = (read(&live, true), read(&disc, true));
        let (lc, dc) = (l.creature.clone().unwrap(), d.creature.clone().unwrap());
        assert_eq!(lc.name, "Tinctoria");
        assert_eq!((lc.level, lc.race, lc.class, lc.hair, lc.face, lc.robe), (42, 3, 13, [1, 2, 3], 2, 2));
        assert_eq!((l.pos, l.model, l.scale), (d.pos, d.model, d.scale));
        assert_eq!((lc.name.as_str(), lc.level, lc.race, lc.class, lc.npc_flags, lc.items, lc.armor), (dc.name.as_str(), dc.level, dc.race, dc.class, dc.npc_flags, dc.items, dc.armor));
        assert_eq!((lc.hair, lc.face, lc.robe, lc.p255, lc.p256, lc.flags262, lc.p264, lc.p299), (dc.hair, dc.face, dc.robe, dc.p255, dc.p256, dc.flags262, dc.p264, dc.p299));
        let colors = lc.colors.unwrap();
        assert_eq!(colors.armor, [[0xD1, 0, 0, 0xFF], [0xFF; 4], [0xFF; 4], [0xFF; 4], [0x5A, 0x5A, 0x78, 0xFF], [0xFF; 4]]);
        assert_eq!(colors.robe, [0x00, 0x82, 0x21, 0xFF]);
        assert_eq!(colors.items, [[0, 0, 0xFF, 0xFF], [0xFF; 4], [0xFF; 4]]);
        assert_eq!(colors.variants, [0; 6]);
        assert_eq!((lc.armor_tint, lc.robe_tint), ([0; 6], 0), "the live channel carries no tints");
        assert!(dc.colors.is_none(), "the disc's 162 bytes are the disc's, the live look in force or not");
        assert_eq!(dc.robe_tint, 1, "the disc's channel's robe tint (the server's byte 110)");

        let not_yet = read(&live, false).creature.unwrap();
        assert!(not_yet.colors.is_none() && not_yet.name != "Tinctoria", "the control: before the look is in force, read as the disc's");
    }

    /// A slot past the limit is not read (CLIENT's `type < 24` until a server
    /// offers more); the limit raised, it is; lowered again, what was past it
    /// is gone and what is within it stays.
    #[test]
    fn the_limit() {
        let mut e = Entities::default();
        let read = |e: &mut Entities, slot: usize| e.channel(slot, &creature(1000 + slot as u32), Some(7), 0, 100, |_| {});
        assert_eq!(e.limit, RETAIL_SLOTS);
        read(&mut e, 23);
        read(&mut e, 24);
        read(&mut e, 100);
        assert!(e.slots[23].is_some());
        assert!(e.slots[24].is_none() && e.slots[100].is_none(), "past CLIENT's 24 before any offer");

        e.set_limit(128);
        read(&mut e, 24);
        read(&mut e, 100);
        read(&mut e, 128);
        assert_eq!(e.slots[100].as_ref().map(|e| e.id), Some(1100));
        assert!(e.slots[24].is_some() && e.slots[128].is_none(), "slot 128 is past a limit of 128");

        e.set_limit(64);
        assert!(e.slots[23].is_some() && e.slots[24].is_some(), "within the limit: kept");
        assert!(e.slots[100].is_none(), "past the limit: gone");
        e.set_limit(0);
        assert_eq!(e.limit, RETAIL_SLOTS, "never fewer than CLIENT's");
        assert!(e.slots[24].is_none());
        e.set_limit(1000);
        assert_eq!(e.limit, MAX_SLOTS);
    }
}
