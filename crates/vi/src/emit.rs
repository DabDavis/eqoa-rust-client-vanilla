//! The sound emitters around the listener: `VIScene::QueryEmittersAtLocation`
//! and the sprites' `QueryEmitters`, plus `VICamera`'s view matrix, which the
//! query hands down.
//!
//! The query takes the actors of the room the camera is in whose bounding
//! sphere holds the camera, walks each one's sprite (a group's members, with
//! their own transforms), and keeps the 32 emitters of the asked-for type with
//! the largest key, in the order of a `VIMultiMap<float, ...>`: ascending, an
//! equal key after the ones already there, the smallest dropped when it
//! overflows. A plain sprite's key is its radius over its distance; a
//! streamed-audio box's is its priority (1, 2, 3) less a ten-thousandth of
//! the listener's distance from the box's centre.

use crate::fpu::{F, Model};
use crate::geom::Matrix44;
use crate::vect::Vect3;
use crate::world::{SpriteKind, World};

/// One emitter found: the key, the actor, the sprite (a group's member when
/// it was in a group) and the sprite's object-to-view matrix.
#[derive(Clone, Debug)]
pub struct Emitter<M: Model> {
    pub key: F<M>,
    pub actor: usize,
    pub sprite: usize,
    pub matrix: Matrix44<M>,
}

/// `VICamera::CalcAxes` and the view half of `VICamera::Update`: the axes as
/// columns, the translation as three dot products. Also the forward vector
/// (`m_dir`).
pub fn camera_view<M: Model>(pos: &Vect3<M>, dir: &Vect3<M>) -> (Matrix44<M>, Vect3<M>) {
    let mut fwd = *dir;
    fwd.normalize();
    let u = Vect3::new(F::ZERO, F::ONE, F::ZERO);
    let mut right = Vect3::cross(&fwd, &u);
    if right.normalize() == 0 {
        let u = Vect3::new(F::ONE, F::ZERO, F::ZERO);
        right = Vect3::cross(&fwd, &u);
        right.normalize();
    }
    let up = Vect3::cross(&right, &fwd);
    let mut v = Matrix44::identity();
    v.m[0].x = right.x;
    v.m[0].y = up.x;
    v.m[0].z = fwd.x;
    v.m[0].w = F::ZERO;
    v.m[1].x = right.y;
    v.m[1].y = up.y;
    v.m[1].z = fwd.y;
    v.m[1].w = F::ZERO;
    v.m[2].x = right.z;
    v.m[2].y = up.z;
    v.m[2].z = fwd.z;
    v.m[2].w = F::ZERO;
    v.m[3].x = -pos.x * v.m[0].x + -pos.y * v.m[1].x + -pos.z * v.m[2].x;
    v.m[3].y = -pos.x * v.m[0].y + -pos.y * v.m[1].y + -pos.z * v.m[2].y;
    v.m[3].z = -pos.x * v.m[0].z + -pos.y * v.m[1].z + -pos.z * v.m[2].z;
    v.m[3].w = F::ONE;
    (v, fwd)
}

struct Query<M: Model> {
    ty: i32,
    limit: usize,
    a: usize,
    b: usize,
    map: Vec<Emitter<M>>,
}

impl<M: Model> Query<M> {
    fn insert(&mut self, key: F<M>, matrix: Matrix44<M>) {
        let at = self.map.iter().position(|e| key < e.key).unwrap_or(self.map.len());
        self.map.insert(at, Emitter { key, actor: self.a, sprite: self.b, matrix });
        if self.limit < self.map.len() {
            self.map.remove(0);
        }
    }
}

impl<M: Model> World<M> {
    /// `VIScene::QueryEmittersAtLocation` for sprites of type `ty` (9 a sound,
    /// 12 a streamed-audio box), at most `limit` of them. `None` when the
    /// camera is outside the world, or a query failed (a streamed-audio
    /// priority outside 0..2).
    pub fn query_emitters_at_location(&mut self, pos: &Vect3<M>, view: &Matrix44<M>, ty: i32, limit: usize) -> Option<Vec<Emitter<M>>> {
        let (z, leaf) = self.find_leaf(pos)?;
        let room = self.zones[z].leaves[leaf].0;
        let mut q = Query { ty, limit, a: 0, b: 0, map: Vec::new() };
        let actors = self.zones[z].rooms.get(room.max(0) as usize).map(|r| r.actors.clone()).unwrap_or_default();
        let mut ok = true;
        for a in actors {
            let actor = &mut self.actors[a];
            if actor.stamp80 == self.stamp || actor.flags & 36 != 0 {
                continue;
            }
            actor.stamp80 = self.stamp;
            let Some(sprite) = actor.sprite else { continue };
            if !point_in_solid(&actor.bound.c, actor.bound.r, pos) {
                continue;
            }
            q.a = a;
            q.b = sprite;
            let m = Matrix44::mul(&actor.object_to_world, view);
            if !self.sprite_emitters(sprite, &m, &mut q) {
                ok = false;
                break;
            }
        }
        if !ok {
            return None;
        }
        self.stamp += 1;
        Some(q.map)
    }

    fn sprite_emitters(&self, sprite: usize, m: &Matrix44<M>, q: &mut Query<M>) -> bool {
        let sp = &self.sprites[sprite];
        match &sp.kind {
            // VIGroupSprite::QueryEmitters: each member at its own transform
            SpriteKind::Group(members) => {
                for &(ms, angles, scale, t) in members {
                    let mem = Matrix44::transform(&Vect3::of(angles[0], angles[1], angles[2]), F::new(scale), &Vect3::of(t[0], t[1], t[2]));
                    let out = Matrix44::mul(&mem, m);
                    q.b = ms;
                    if !self.sprite_emitters(ms, &out, q) {
                        return false;
                    }
                }
                true
            }
            // VIStreamAudioSprite::QueryEmitters: the listener inside the box
            SpriteKind::Stream { priority, .. } => {
                if q.ty != sp.sprite_type {
                    return true;
                }
                let inv = Matrix44::invert_affine(m);
                let mut p = Vect3::new(inv.m[3].x, inv.m[3].y, inv.m[3].z);
                if contains(&sp.bbox.min, &sp.bbox.max, &p) {
                    let mut k = match priority {
                        0 => F::ONE,
                        1 => F::new(2.0),
                        2 => F::new(3.0),
                        _ => return false,
                    };
                    let half = F::new(0.5);
                    p.x = p.x - (sp.bbox.min.x + sp.bbox.max.x) * half;
                    p.y = p.y - (sp.bbox.min.y + sp.bbox.max.y) * half;
                    p.z = p.z - (sp.bbox.min.z + sp.bbox.max.z) * half;
                    k = k - p.magnitude() * F::new(0.0001);
                    q.insert(k, *m);
                }
                true
            }
            // (VIHSprite's override poses the sprite and asks its attachments;
            // no retail file attaches an emitter to one)
            SpriteKind::H(_) => true,
            // VISprite::QueryEmitters: the sphere over the distance
            _ => {
                if q.ty == sp.sprite_type {
                    let p = Vect3::new(m.m[3].x, m.m[3].y, m.m[3].z);
                    let s = (m.m[0].x * m.m[0].x + m.m[0].y * m.m[0].y + m.m[0].z * m.m[0].z).sqrt();
                    let t = sp.bsphere.r * s;
                    let d = (p.x * p.x + p.y * p.y + p.z * p.z).sqrt();
                    let k = t * (F::ONE / d);
                    q.insert(k, *m);
                }
                true
            }
        }
    }
}

/// `VISphere::PointInSolid`
fn point_in_solid<M: Model>(c: &Vect3<M>, r: F<M>, v: &Vect3<M>) -> bool {
    let d = Vect3::new(v.x - c.x, v.y - c.y, v.z - c.z);
    d.x * d.x + d.y * d.y + d.z * d.z <= r * r
}

/// `VIBBox::Contains`
fn contains<M: Model>(min: &Vect3<M>, max: &Vect3<M>, p: &Vect3<M>) -> bool {
    min.x <= p.x && p.x <= max.x && min.y <= p.y && p.y <= max.y && min.z <= p.z && p.z <= max.z
}
