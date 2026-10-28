//! The characters' blob shadows, as `VIScene` queues and draws them (decomp
//! `viscene.cpp`: `RasterActor`'s queue, `RenderShadows`, `RenderShadow`; the
//! material `SetSky` makes), held to the decomp by eqoa-port's
//! `oracle_shadow` (`tests/oracle_shadow.rs`).
//!
//! `RasterActor` queues an actor it rasters whose sprite is a `VICSprite`
//! (type 5) while its distance squared from the eye is below `m_farPlane`
//! (2500, set once by `Clear` and never changed: 50 units), at most 128 a
//! frame; `Render` clears the queue first and draws it after the particles
//! (`RenderShadows`, each in turn).
//!
//! `RenderShadow` bounds six of the skeleton's slots (`SLOTS`: the last two,
//! 0 and 1, pulled halfway to the actor's origin) in the actor's own frame,
//! inflates that box by 0.2 in x and z, takes it back into the world (the
//! box round its eight corners) and stretches it from 1 below the actor's
//! feet to 0.5 above. Every scene triangle facing within 80 degrees of up in
//! it, cut to it (`VIQueryTriList`, at most 200: `VIScene::Init`'s), is drawn
//! again: lifted 0.04, the local box mapped onto UV, white with alpha
//! `(1 - clamp(feet - y, 0, 1)) * 0.65`, a fan of three, under the shadow
//! material at Z bias 15. The query's filter is a default `VIActorFilter`,
//! which filters no actor: the rooms' own geometry and the placed props
//! take a shadow (a character's sprite has no triangles to give).

use crate::flora::{QueryTri, QueryTriList};
use crate::fpu::{F, Model, f, vimax, vimin};
use crate::geom::{BBox, Matrix44};
use crate::pick::Color32F;
use crate::vect::{Vect2, Vect3};

/// `m_farPlane`: an actor is queued while its distance squared from the eye
/// is below it.
pub const FAR_PLANE: f32 = 2500.0;

/// `m_shadows`: the queue's 128; a frame's actors past them are not queued.
pub const MAX_SHADOWS: usize = 128;

/// `m_queryTris.Init(200, ...)` in `VIScene::Init`: a shadow's triangles.
pub const MAX_TRIS: usize = 200;

/// `RenderShadow`'s six slots (`VICSpriteNodeID`), in its order; the last
/// two are halved in the actor's frame.
pub const SLOTS: [i32; 6] = [5, 6, 4, 7, 0, 1];

/// `SetZBias(15)`: `VIRaster::SetZBias` makes it 15 x 166 in the GS's
/// screen Z.
pub const Z_BIAS: i32 = 15;

/// The material's texture (`VIHashResourceID("shadow.tga")`); none in the
/// dictionary, no shadows (`m_p1880` stays -1).
pub const TEXTURE: &str = "shadow.tga";

/// `VIScene::SetSky`'s shadow material: one layer, texture fill, modulated,
/// blend mode 3 (`(Cs - Cd) As + Cd`), LOD bias 1.0, no Z write, Z test,
/// no dither, wrap mode 3 (`ConstructTextPass` makes it CLAMP on both axes).
pub const BLEND: usize = 3;
pub const LOD_BIAS: f32 = 1.0;
pub const ZWRITE: bool = false;
pub const ZTEST: bool = true;

/// What `RenderShadow` reads of the scene.
pub trait ShadowScene<M: Model> {
    /// `GetActorSpritePtr(actor)` is not null.
    fn has_sprite(&self, actor: i32) -> bool;
    /// `GetActorTransform`: `Transform(hpr, scale, location)`.
    fn actor_transform(&self, actor: i32) -> Matrix44<M>;
    /// `GetActorLocation`.
    fn actor_location(&self, actor: i32) -> Vect3<M>;
    /// `VICSprite::CalcSlotWorldTransform(slot, actor)`: the slot's node under
    /// the centre transform, or the actor's transform when the sprite lacks
    /// the slot.
    fn slot_world_transform(&self, actor: i32, slot: i32) -> Matrix44<M>;
    /// `QueryIntersection(box, VIActorFilter(), list)`.
    fn query(&mut self, b: &BBox<M>, l: &mut QueryTriList<M>);
}

/// One vertex as `RenderShadow` hands it to the raster: `Color`, `UV`,
/// `Vertex`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Vertex<M: Model> {
    pub color: Color32F<M>,
    pub uv: Vect2<M>,
    pub at: Vect3<M>,
}

/// One actor's shadow: `SetMaterial(material)`, `SetZBias(15)`, a fan of
/// three a triangle (`tris`), `SetZBias(0)`; and what made it.
#[derive(Clone, Debug)]
pub struct Shadow<M: Model> {
    pub material: i32,
    /// The six slots in the actor's frame, bounded and inflated: what the
    /// UVs map.
    pub local: BBox<M>,
    /// The box queried, in the world.
    pub query: BBox<M>,
    /// The query's triangles (`m_queryTris` after it).
    pub found: Vec<QueryTri<M>>,
    pub tris: Vec<[Vertex<M>; 3]>,
}

/// `RenderShadow(actor)` under `material` (`m_p1880`): None, and nothing
/// drawn, when there is no material or the actor no sprite.
pub fn render_shadow<M: Model>(sc: &mut impl ShadowScene<M>, material: i32, actor: i32) -> Option<Shadow<M>> {
    if material == -1 {
        return None;
    }
    if !sc.has_sprite(actor) {
        return None;
    }
    let to_world = sc.actor_transform(actor);
    // its return is not read: a singular transform leaves the rows so far
    let (to_local, _) = Matrix44::invert_partial(&to_world);
    let mut pts = [Vect3::<M>::default(); 6];
    for (i, &slot) in SLOTS.iter().enumerate() {
        let node = sc.slot_world_transform(actor, slot);
        let v = Vect3::new(node.m[3].x, node.m[3].y, node.m[3].z);
        pts[i] = to_local.mul_point(&v);
        if i >= 4 {
            pts[i] *= f(0.5);
        }
    }
    // bb.Init(6, pts)
    let mut bb = BBox::<M>::default();
    for (i, p) in pts.iter().enumerate() {
        bb.bound(i == 0, p);
    }
    let mut e = Vect3::new(f(0.2), F::ZERO, f(0.2));
    bb.min -= e;
    e.set(f(0.2), F::ZERO, f(0.2));
    bb.max += e;
    let mut bx = bb.transform(&to_world);
    let v = sc.actor_location(actor);
    bx.max.y = v.y + f(0.5);
    bx.min.y = v.y - F::ONE;
    e.set(F::ZERO, F::ONE, F::ZERO);
    let mut list = QueryTriList::new(MAX_TRIS, &bx, e, f(80.0));
    sc.query(&bx, &mut list);
    let mut tris = Vec::with_capacity(list.tris.len());
    let m = &to_local.m;
    for t in &list.tris {
        let sx = F::ONE / (bb.max.x - bb.min.x);
        let sz = F::ONE / (bb.max.z - bb.min.z);
        let mut p = [t.v0, t.v1, t.v2];
        for q in p.iter_mut() {
            q.y = q.y + f(0.04);
        }
        let vert = |q: &Vect3<M>| {
            let u = (q.x * m[0].x + q.y * m[1].x + q.z * m[2].x + m[3].x - bb.min.x) * sx;
            let w = (q.x * m[0].z + q.y * m[1].z + q.z * m[2].z + m[3].z - bb.min.z) * sz;
            let d = v.y - q.y;
            let d = vimin(vimax(F::ZERO, d), F::ONE);
            Vertex { color: Color32F { r: F::ONE, g: F::ONE, b: F::ONE, a: (F::ONE - d) * f(0.65) }, uv: Vect2::new(u, w), at: *q }
        };
        tris.push([vert(&p[0]), vert(&p[1]), vert(&p[2])]);
    }
    Some(Shadow { material, local: bb, query: bx, found: list.tris, tris })
}

/// `RasterActor`'s test for the queue: a `VICSprite`'s actor at `at` from
/// the eye at `eye`, `d2 < m_farPlane` (the distance squared summed x, y, z
/// as the game sums it).
pub fn within<M: Model>(at: &Vect3<M>, eye: &Vect3<M>) -> bool {
    let (dx, dy, dz) = (at.x - eye.x, at.y - eye.y, at.z - eye.z);
    dx * dx + dy * dy + dz * dz < f(FAR_PLANE)
}
