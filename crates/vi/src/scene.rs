//! What the walk and camera code ask of the world: `VIScene`'s clock, its
//! sphere and environment-ray collides, and `GetEnvFlags`.
//!
//! The game answers these by walking the world BSP, the zone BSP, the rooms and
//! their actors (`viscene.cpp`, `vizone.cpp`, `vicollide.cpp`; already ported
//! to Go and checked against the console in `go-eqoa-vi/vicoll`). That walk is
//! the next port; until then a client supplies its own implementation.

use crate::coll::{CollSphere, EnvRay};
use crate::fpu::Model;
use crate::vect::Vect3;

/// Which `VISceneFilter` subclass a collide runs with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterKind {
    /// `VISceneFilter`: skips only `actor`.
    Scene,
    /// `VIWalkFloorFilter`: also skips every `VICSprite` (players and NPCs),
    /// so a floor probe lands on the terrain under them.
    WalkFloor,
    /// `VIWalkCameraFilter`: skips `actor` and every `VICSprite`.
    WalkCamera,
}

#[derive(Clone, Copy, Debug)]
pub struct Filter {
    pub kind: FilterKind,
    /// The actor to ignore (the character's own); -1 for none.
    pub actor: i32,
    /// Filled by the collide: the actor hit, or -1 for world geometry.
    pub hit_actor: i32,
    pub hit_sprite: i32,
}

impl Filter {
    /// `Init(actor)`.
    pub fn new(kind: FilterKind, actor: i32) -> Self {
        Filter { kind, actor, hit_actor: -1, hit_sprite: -1 }
    }
}

pub trait Scene<M: Model> {
    /// `VIScene::GetTime`, in milliseconds.
    fn time(&self) -> i64;
    /// Sweep `s`; on a hit, set its hit time, index and normal, fill
    /// `filter.hit_actor`, and return true.
    fn collide_sphere(&mut self, s: &mut CollSphere<M>, filter: &mut Filter) -> bool;
    /// `Collide(VIEnvRay &)`: the first crossing of a liquid in `ray.mask`.
    fn collide_env(&mut self, ray: &mut EnvRay<M>) -> bool;
    /// `GetEnvFlags`: 1 water, 2 slime, 4 lava, 8 mud (and `!0` outside the
    /// world, as the game's does).
    fn env_flags(&self, at: &Vect3<M>) -> u32;
}
