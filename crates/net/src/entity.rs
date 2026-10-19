//! The entities on state channels 0x00-0x17: `VIClient::ChannelEntity` and
//! the two readers (`VIEntity::ReadUpdate`, `VICreatureEntity::ReadUpdate`,
//! decomp `modules/client/src/{viclient,vientity,vicreatureentity}.cpp`),
//! and `VIClient::Extrapolate`'s blend from where an entity was drawn to
//! where its newest update puts it.

use crate::stream::Reader;

pub const SLOTS: usize = 24;
pub const TYPE_ENTITY: u8 = 129;
pub const TYPE_CREATURE: u8 = 130;

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
    pub name: String,
    pub level: u8,
    pub race: u8,
    pub class: u8,
    pub npc_flags: u16,
}

#[derive(Clone, Debug, Default)]
pub struct Entity {
    pub id: u32,
    pub kind: u8,
    pub pos: [f32; 3],
    /// Heading and pitch (roll is 0).
    pub hpr: [f32; 3],
    pub world: i8,
    pub model: u32,
    pub model2: u32,
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
    fn read_update(&mut self, r: &mut Reader) {
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
        r.u8();
        r.u8();
        self.model = r.u32();
        self.model2 = r.u32();
        self.scale = r.f32();
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
            c.armor_tint.copy_from_slice(&pad6(r.bytes(6)));
            c.hair = [r.u8(), r.u8(), r.u8()];
            c.face = r.u8();
            c.robe = r.u8();
            c.robe_tint = r.u8();
            r.u32();
            r.u8();
            let name = r.bytes(24);
            c.name = String::from_utf8_lossy(&name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())]).into_owned();
            c.level = r.u8();
            r.u8();
            r.u8();
            r.u8();
            c.race = r.u8();
            c.class = r.u8();
            c.npc_flags = r.u16();
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

/// The 24 slots.
#[derive(Default)]
pub struct Entities {
    pub slots: [Option<Entity>; SLOTS],
}

impl Entities {
    /// `VIClient::ChannelEntity` for slot `ch` at time `now`. Nothing is read
    /// before the player's own id is known; a message under 5 bytes empties
    /// the slot; a different id or type, or flag 0, replaces it (flag 0 then
    /// leaves it empty). Types other than 129/130 crash the PS2; they are
    /// dropped here.
    pub fn channel(&mut self, ch: usize, data: &[u8], self_id: Option<u64>, now: u32, rate: u32) {
        let Some(self_id) = self_id else { return };
        if ch >= SLOTS {
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
        e.read_update(&mut r);
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
