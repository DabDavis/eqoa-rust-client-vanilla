//! The characters' blob shadows, as `VIScene` draws them: `vi::shadow`
//! (`RenderShadow`, held to the decomp by eqoa-port's `oracle_shadow`) over
//! the walker's world (`World::query_intersection_actors`: the ground and the
//! placed props under each, as the default `VIActorFilter` takes them), drawn
//! by `render::shadow` under `SetSky`'s material.
//!
//! The queue is `RasterActor`'s: every character the game rasters this frame
//! (a creature drawn, the player while its own actor is shown: not in first
//! person, `Play::self_shown`), in view (its actor's bound inside the frustum,
//! `CullByTop`) and within 50 of the eye (`d2 < m_farPlane`), at most 128, in
//! the order the game updates their actors (the entity slots, the player in
//! its own). A skin-hidden (invisible) character keeps its shadow: the game
//! hides its skins, not its actor. The eye is the one every other
//! `RasterActor` distance goes by here (`lod::eye`: drawn in to the game's
//! camera ceiling), so zoomed out past the game's camera the player's own
//! shadow stays. No shadow.tga in the library (FX.CSF's), no material
//! (`m_p1880` -1), no shadows.
//!
//! Drawn after the particle system's billboards and before the weather's, as
//! `VIScene::Render` draws `RenderShadows` after `VIParticleSystem::Render`
//! (`viewer`, `render::billboard`'s late slot). The options' Shadows off
//! draws none.

use esf::Library;
use glam::{Mat4, Vec3, Vec4};
use render::wgpu;
use vi::csprite::CSprite;
use vi::flora::QueryTriList;
use vi::fpu::{Ee, F};
use vi::geom::{BBox, Matrix44, Sphere};
use vi::shadow::{MAX_SHADOWS, ShadowScene, TEXTURE, render_shadow, within};
use vi::vect::Vect3;
use vi::world::World;

/// A character `RasterActor` rasters this frame, as `RenderShadow` reads it:
/// its actor's transform (`GetActorTransform`) and place, each slot's world
/// transform (`CalcSlotWorldTransform`: the slot's node under the centre
/// transform; None where the sprite lacks the slot, which is the actor's own
/// transform) and its actor's bound (for `CullByTop`; None: not culled).
#[derive(Clone, Debug)]
pub struct Caster {
    pub transform: Matrix44<Ee>,
    pub location: Vect3<Ee>,
    pub slots: [Option<Matrix44<Ee>>; 9],
    pub bound: Option<Sphere<Ee>>,
}

impl Caster {
    /// A posed CSprite's caster: `transform` its actor's, `centre` the
    /// centre transform it was drawn with (`CalcTransAboutCenter`).
    pub fn of(transform: Matrix44<Ee>, cs: &CSprite<Ee>, centre: &Matrix44<Ee>, bound: Option<Sphere<Ee>>) -> Self {
        let mut slots = [None; 9];
        for (k, s) in slots.iter_mut().enumerate() {
            let node = usize::try_from(cs.node_ids[k]).ok().and_then(|n| cs.h.nodes.get(n));
            *s = node.map(|n| Matrix44::mul(&n.world, centre));
        }
        Caster { transform, location: transform.m[3].xyz(), slots, bound }
    }

    /// `CalcSlotWorldTransform(slot)`: the slot's world transform, or the
    /// actor's own where the sprite lacks the slot.
    pub fn slot(&self, slot: i32) -> Matrix44<Ee> {
        self.slots.get(slot as usize).copied().flatten().unwrap_or(self.transform)
    }
}

/// The walker's own body as a caster (the player's actor: its heading, at
/// its drawn place, `Walker::pose`'s centre transform), its bound off
/// `model`'s sprite in the walker's world when given; None without a
/// CSprite. The pose is the frame's when the walker has been posed.
pub fn walker_caster(walker: &mut crate::walker::Walker, lib: &Library, model: Option<u32>) -> Option<Caster> {
    let actor = Matrix44::transform(&Vect3::new(walker.cam.walk.phys.angles.x, F::ZERO, F::ZERO), F::ONE, &walker.cam.walk.phys.smooth_location());
    let bound = model.and_then(|m| lib.with_view(lib.view_for(m as i32), || lib.lookup(m as i32))).and_then(|r| walker.scene.find_sprite(lib, r)).map(|s| {
        let b = walker.scene.sprites[s].bsphere;
        Sphere { c: actor.mul_point(&b.c), r: b.r }
    });
    let cs = walker.sprite.as_mut()?;
    let centre = cs.h.calc_trans_about_center(&actor, &walker.anims);
    Some(Caster::of(actor, cs, &centre, bound))
}

/// `RenderShadow`'s scene: the casters and the walker's world.
struct Scene<'a> {
    world: &'a mut World<Ee>,
    casters: &'a [Caster],
}

impl ShadowScene<Ee> for Scene<'_> {
    fn has_sprite(&self, _: i32) -> bool {
        true
    }
    fn actor_transform(&self, actor: i32) -> Matrix44<Ee> {
        self.casters[actor as usize].transform
    }
    fn actor_location(&self, actor: i32) -> Vect3<Ee> {
        self.casters[actor as usize].location
    }
    fn slot_world_transform(&self, actor: i32, slot: i32) -> Matrix44<Ee> {
        self.casters[actor as usize].slot(slot)
    }
    fn query(&mut self, b: &BBox<Ee>, l: &mut QueryTriList<Ee>) {
        self.world.query_intersection_actors(b, l);
    }
}

/// `RasterActor`'s queue over `casters`, in their order: in view (the bound
/// inside every plane of `planes`, `particles::frustum`'s) and within 50 of
/// `eye`, at most 128.
pub fn queue(casters: &[Caster], eye: &Vect3<Ee>, planes: &[Vec4; 6]) -> Vec<usize> {
    let in_view = |b: &Option<Sphere<Ee>>| {
        b.as_ref().is_none_or(|b| {
            let c = Vec3::new(b.c.x.0, b.c.y.0, b.c.z.0);
            planes.iter().all(|p| p.dot(c.extend(1.0)) >= -b.r.0)
        })
    };
    casters.iter().enumerate().filter(|(_, c)| in_view(&c.bound) && within(&c.location, eye)).map(|(k, _)| k).take(MAX_SHADOWS).collect()
}

/// The shadows of a frame.
#[derive(Default)]
pub struct Shadows {
    /// shadow.tga's DictID, while the library has it
    texture: Option<i32>,
    /// this frame's vertices, three a triangle, relative to `eye`
    verts: Vec<render::shadow::Vertex>,
    eye: Vec3,
    /// where the queue measured from (`lod::eye`: the game's camera)
    game_eye: Vec3,
    /// how many were queued and drawn, and their triangles (`describe`)
    queued: usize,
    drawn: usize,
    tris: usize,
    /// The batches before the weather's (`viewer`): the shadows are drawn
    /// after them.
    pub particles_end: usize,
}

impl Shadows {
    pub fn new() -> Self {
        Self::default()
    }

    /// `RenderShadows` over this frame's `casters`: the queue (`queue`, from
    /// the eye `eye`, in `view_proj`'s view), each shadow made over `world`,
    /// its vertices relative to `rel` (the view's eye, as the frame draws
    /// them). Nothing when the library has no shadow.tga.
    pub fn frame(&mut self, world: &mut World<Ee>, lib: &Library, casters: &[Caster], eye: &Vect3<Ee>, view_proj: Mat4, rel: Vec3) {
        self.verts.clear();
        self.eye = rel;
        self.game_eye = Vec3::new(eye.x.0, eye.y.0, eye.z.0);
        let id = vi::hash::hash_resource_id(TEXTURE) as i32;
        self.texture = lib.surface(id).is_some().then_some(id);
        let q = queue(casters, eye, &crate::particles::frustum(view_proj));
        self.queued = q.len();
        self.drawn = 0;
        self.tris = 0;
        if self.texture.is_none() {
            return;
        }
        let mut sc = Scene { world, casters };
        let r = rel.as_dvec3();
        for k in q {
            let Some(s) = render_shadow(&mut sc, 0, k as i32) else { continue };
            self.drawn += 1;
            self.tris += s.tris.len();
            for t in &s.tris {
                for v in t {
                    let at = glam::DVec3::new(v.at.x.0 as f64, v.at.y.0 as f64, v.at.z.0 as f64) - r;
                    self.verts.push(render::shadow::Vertex { at: at.as_vec3().to_array(), uv: [v.uv.x.0, v.uv.y.0], alpha: v.color.a.0 });
                }
            }
        }
    }

    /// The frame's shadows to the GPU, nearer the camera by `SetZBias(15)`
    /// under `proj` as the game's camera pulls them (from the queue's eye,
    /// which the view's is that far behind); none: nothing is drawn.
    pub fn upload(&self, renderer: &mut render::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, lib: &Library, proj: &Mat4) {
        let far = proj.w_axis.z / (proj.z_axis.z + 1.0);
        let k = render::shadow::z_bias_reciprocal(vi::shadow::Z_BIAS, GAME_NEAR, far);
        let bias = render::shadow::Bias { clip_z: proj.w_axis.z, k, behind: self.eye.distance(self.game_eye) };
        let texture = self.texture.unwrap_or(0);
        renderer.set_shadows(device, queue, texture, &self.verts, self.eye, bias, self.particles_end, &render::library_textures(lib));
    }

    /// Lets go of what was drawn (Shadows turned off).
    pub fn clear(&mut self, renderer: &mut render::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, lib: &Library) {
        self.verts.clear();
        self.queued = 0;
        self.drawn = 0;
        self.tris = 0;
        renderer.set_shadows(device, queue, 0, &[], self.eye, render::shadow::Bias::default(), 0, &render::library_textures(lib));
    }

    pub fn describe(&self) -> String {
        format!("shadows: {} queued, {} drawn, {} triangles{}", self.queued, self.drawn, self.tris, if self.texture.is_none() { " (no shadow.tga)" } else { "" })
    }
}

/// The game camera's near plane (`VIClient`'s `m_camera.Init(.., 0.1f, 900.0f)`),
/// which the GS's screen Z spans from: the Z bias is a shift of 1/w over it.
pub const GAME_NEAR: f32 = 0.1;

#[cfg(test)]
mod tests {
    use super::*;

    fn caster(at: [f32; 3], r: f32) -> Caster {
        let m = Matrix44::transform(&Vect3::of(0.0, 0.0, 0.0), F::ONE, &Vect3::of(at[0], at[1], at[2]));
        Caster { transform: m, location: m.m[3].xyz(), slots: [None; 9], bound: Some(Sphere { c: Vect3::of(at[0], at[1] + 1.0, at[2]), r: F::new(r) }) }
    }

    /// `RasterActor`'s queue: within 50 of the eye (the distance squared
    /// under 2500, height and all), in view, in the casters' order, 128 at
    /// most. The controls: one 50.01 away, one 30 across but 45 above, and
    /// one behind the camera are not queued; with no view, the one behind is.
    #[test]
    fn the_queue_is_raster_actors() {
        let eye = Vect3::of(100.0, 10.0, 100.0);
        // a view down -z from the eye: everything with z < 100 + r is in it
        let planes = [Vec4::new(0.0, 0.0, -1.0, 100.0), Vec4::new(0.0, 0.0, 0.0, 1.0), Vec4::new(0.0, 0.0, 0.0, 1.0), Vec4::new(0.0, 0.0, 0.0, 1.0), Vec4::new(0.0, 0.0, 0.0, 1.0), Vec4::new(0.0, 0.0, 0.0, 1.0)];
        let cs = vec![
            caster([100.0, 10.0, 60.0], 1.0),
            caster([100.0, 10.0, 49.99], 1.0),
            caster([100.0, 10.0, 49.0], 1.0),
            caster([100.0, 10.0 + 45.0, 70.0], 1.0),
            caster([100.0, 10.0, 130.0], 1.0),
            caster([130.0, 30.0, 70.0], 1.0),
        ];
        // 40, 50.01, 51, sqrt(2025 + 900) = 54.1, behind (z 130), and the
        // last sqrt(900 + 400 + 900) = 46.9
        assert_eq!(queue(&cs, &eye, &planes), vec![0, 5]);
        let all = [Vec4::new(0.0, 0.0, 0.0, 1.0); 6];
        assert_eq!(queue(&cs, &eye, &all), vec![0, 4, 5], "the control: no view, the one behind is queued");
        let many: Vec<Caster> = (0..200).map(|k| caster([100.0, 10.0, 99.0 - k as f32 * 0.2], 1.0)).collect();
        let q = queue(&many, &eye, &planes);
        assert_eq!((q.len(), q[0], q[127]), (128, 0, 127), "the first 128 in order");
    }

    /// A slot the sprite lacks is the actor's own transform
    /// (`CalcSlotWorldTransform`'s else); one it has, its node's.
    #[test]
    fn a_missing_slot_is_the_actors_transform() {
        let mut c = caster([5.0, 6.0, 7.0], 1.0);
        c.slots[4] = Some(Matrix44::transform(&Vect3::of(0.0, 0.0, 0.0), F::ONE, &Vect3::of(5.0, 7.3, 7.0)));
        assert_eq!(c.slot(5).m[3].y.0, 6.0);
        assert_eq!(c.slot(4).m[3].y.0, 7.3);
        assert_eq!(c.slot(9).m[3].y.0, 6.0, "past the table: the actor's");
    }
}
