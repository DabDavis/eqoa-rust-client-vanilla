//! Zones: terrain patches and placed actors.
//!
//! A world (0x8100) holds zones (0x3000). A zone's layout lives in two places,
//! neither of them the sprite records a model listing walks:
//!
//! - **Terrain** is the SubSprites under ZoneResources (0x3100). Their vertices
//!   are quantised against the zone's **pre-translation table** (0x3250 under
//!   0x3200): each vertex names a coordinate island, and without the table every
//!   patch collapses onto one 2 km square.
//! - **Props** are ZoneActor records (0x6000), 36 bytes each: a sprite DictID,
//!   position, rotation (only `rot.x` drives yaw), scale and an RGBA colour. The
//!   sprite is authored at the origin; the actor says where it stands.
//!
//! Ported from the Blender importer's `zone.py` (from `go-eqoa-esf-tools-v2`'s
//! `zone.go`); the game's parser is `VIESFParse`'s zone half in the decomp.

use crate::types::SUBSPRITE;
use crate::{Container, NodeId, f32le, i32le};

pub const WORLD: u16 = 0x8100;
pub const ZONE: u16 = 0x3000;
pub const ZONE_RESOURCES: u16 = 0x3100;
pub const ZONE_BASE: u16 = 0x3200;
pub const ZONE_PRETRANS: u16 = 0x3250;
pub const ZONE_ROOM_ACTORS: u16 = 0x3270;
pub const ZONE_ROOM_ACTORS2: u16 = 0x3280;
pub const ZONE_ACTORS: u16 = 0x3290;
pub const ZONE_ACTOR: u16 = 0x6000;
pub const ZONE_ROOM_ACTORS3: u16 = 0x6040;

#[derive(Clone, Debug)]
pub struct Actor {
    /// The room's real id (ZoneRoomActors2), not a positional counter.
    pub room: i32,
    /// Its slot in the room's actor list, which a static lighting names.
    pub slot: usize,
    pub sprite: i32,
    pub pos: [f32; 3],
    pub rot: [f32; 3],
    pub scale: f32,
    pub color: [u8; 4],
}

impl Actor {
    /// Column-major model matrix: scale (if positive), yaw by `rot[0]` with
    /// x' = z·sin + x·cos, z' = z·cos − x·sin, then translate.
    pub fn matrix(&self) -> [[f32; 4]; 4] {
        let s = if self.scale > 0.0 { self.scale } else { 1.0 };
        let (sin, cos) = self.rot[0].sin_cos();
        [
            [s * cos, 0.0, -s * sin, 0.0],
            [0.0, s, 0.0, 0.0],
            [s * sin, 0.0, s * cos, 0.0],
            [self.pos[0], self.pos[1], self.pos[2], 1.0],
        ]
    }
}

impl Container {
    /// Every zone of every world in this file, in record order.
    pub fn zones(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.roots.iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            if self.node(n).ty == WORLD {
                out.extend(self.node(n).children.iter().filter(|&&z| self.node(z).ty == ZONE));
            } else {
                stack.extend(self.node(n).children.iter().rev());
            }
        }
        out
    }

    /// The zone's coordinate islands.
    pub fn pre_translations(&self, zone: NodeId) -> Vec<[f32; 3]> {
        let Some(pt) = self.child(zone, ZONE_BASE).and_then(|b| self.child(b, ZONE_PRETRANS)) else {
            return Vec::new();
        };
        let (d, body) = (&self.data, self.node(pt).body);
        let n = i32le(d, body);
        if !(0..=4096).contains(&n) || body + 4 + n as usize * 12 > d.len() {
            return Vec::new();
        }
        (0..n as usize)
            .map(|i| {
                let p = body + 4 + i * 12;
                [f32le(d, p), f32le(d, p + 4), f32le(d, p + 8)]
            })
            .collect()
    }

    /// The terrain patches (SubSprites under ZoneResources).
    pub fn terrain_patches(&self, zone: NodeId) -> Vec<NodeId> {
        self.child(zone, ZONE_RESOURCES).map_or_else(Vec::new, |r| {
            self.node(r).children.iter().copied().filter(|&s| self.node(s).ty == SUBSPRITE).collect()
        })
    }

    /// The zone's baked lighting (0x32B0, `ParseZoneStaticLightings`): per
    /// (room id, actor slot), the colour buffers (0x1220), each its colours,
    /// in the order the actor's simple sprites take them.
    pub fn static_lightings(&self, zone: NodeId) -> std::collections::HashMap<(i32, usize), Vec<Vec<[u8; 4]>>> {
        let d = &self.data;
        let mut out = std::collections::HashMap::new();
        let Some(sl) = self.child(zone, 0x32B0) else { return out };
        for &rl in &self.node(sl).children {
            let (Some(room), Some(list)) = (self.child(rl, 0x32A0), self.child(rl, 0x6030)) else { continue };
            let room = i32le(d, self.node(room).body);
            for &l in &self.node(list).children {
                let Some(obj) = self.child(l, 0x6020) else { continue };
                let slot = i32le(d, self.node(obj).body);
                let mut colours = Vec::new();
                for &b in self.node(l).children.iter().filter(|&&b| self.node(b).ty == 0x1220) {
                    let n = self.node(b);
                    let k = i32le(d, n.body + 4).max(0) as usize;
                    colours.push((0..k).filter(|i| 12 + 4 * i <= n.size).map(|i| {
                        let o = n.body + 8 + 4 * i;
                        [d[o], d[o + 1], d[o + 2], d[o + 3]]
                    }).collect());
                }
                if let Ok(slot) = usize::try_from(slot) {
                    out.insert((room, slot), colours);
                }
            }
        }
        out
    }

    pub fn actors(&self, zone: NodeId) -> Vec<Actor> {
        let d = &self.data;
        let mut out = Vec::new();
        let Some(za) = self.child(zone, ZONE_ACTORS) else { return out };
        for &ra in &self.node(za).children {
            if self.node(ra).ty != ZONE_ROOM_ACTORS {
                continue;
            }
            let room = self.child(ra, ZONE_ROOM_ACTORS2).map_or(-1, |r| i32le(d, self.node(r).body));
            let Some(ra3) = self.child(ra, ZONE_ROOM_ACTORS3) else { continue };
            for (slot, &a) in self.node(ra3).children.iter().enumerate() {
                let n = self.node(a);
                if n.ty != ZONE_ACTOR || n.size < 36 {
                    continue;
                }
                let o = n.body;
                let f = |k: usize| f32le(d, o + k);
                out.push(Actor {
                    room,
                    slot,
                    sprite: i32le(d, o),
                    pos: [f(4), f(8), f(12)],
                    rot: [f(16), f(20), f(24)],
                    scale: f(28),
                    color: [d[o + 32], d[o + 33], d[o + 34], d[o + 35]],
                });
            }
        }
        out
    }
}
