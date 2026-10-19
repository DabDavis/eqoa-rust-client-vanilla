//! The character on the ground: the ported `VIWalkCamera` (crate `vi`, on the
//! console's float model), set up and driven the way CLIENT's
//! `VIClient::UpdateControls` drives it in third person (camera mode 3):
//!
//!   fwd = -left.y, strafe = left.x  ->  ViewRelativeMove(camera dir, fwd, strafe)
//!   right.y                         ->  OrbitIn(-pitch)
//!   right.x                         ->  OrbitRight(orbit)
//!   then VIWalkCamera::Process, which steps everything in 32 ms ticks.
//!
//! The scene it collides with is the game's own: `vi::world::World`, the
//! world and zone BSPs, rooms, collision buffers and placed actors, loaded
//! from the world file (`VIScene::Collide` and `GetEnvFlags`, ported).

use glam::{Mat4, Vec3};
use vi::camera::{Eye, WalkCamera};
use vi::coll::CollSphere;
use vi::fpu::{Ee, F};
use vi::scene::{Filter, FilterKind};
use vi::vect::Vect3;
use vi::character::{AnimCache, load_csprite};
use vi::csprite::{CSprite, Rand};
use vi::geom::Matrix44;
use vi::hsprite::AnimScene;
use vi::world::World;

/// One frame's controls, as `UpdateControls` reads them: each in -1..1.
#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    pub fwd: f32,
    pub strafe: f32,
    pub orbit: f32,
    pub pitch: f32,
}

pub struct Walker {
    pub cam: WalkCamera<Ee>,
    pub scene: World<Ee>,
    pub eye: Eye<Ee>,
    /// The character, animated: the game's VICSprite over the clips it names.
    pub sprite: Option<CSprite<Ee>>,
    pub anims: AnimScene<Ee>,
    rand: Rand,
}

impl Walker {
    /// Stand the character at (x, z) on the terrain, facing `heading`, with
    /// the camera behind it.
    pub fn new(scene: World<Ee>, x: f32, z: f32, heading: f32) -> Self {
        let mut scene = scene;
        let y = ground(&mut scene, x, z).unwrap_or(0.0);
        let mut cam = WalkCamera::<Ee>::default();
        let pos = Vect3::of(x, y, z);
        let angles = Vect3::of(heading, 0.0, 0.0);
        cam.init(pos, angles, &mut scene);
        // What a camera preset does (`VIClient::ApplyCameraPreset`): mode,
        // pitch, distance, angle. The preset's own numbers are not read yet;
        // these are the walk camera's defaults.
        cam.set_camera_mode(3);
        let pitch = cam.def_orbit_pitch;
        cam.set_orbit_pitch_angle(pitch);
        let dist = cam.def_orbit_distance;
        cam.set_orbit_distance(dist);
        cam.set_orbit_angle_behind();
        let eye = cam.process(&mut scene);
        Walker { cam, scene, eye, sprite: None, anims: AnimScene::default(), rand: Rand(1) }
    }

    /// Loads the character as a VICSprite from `r` (a 0x2700 record).
    pub fn set_character(&mut self, lib: &esf::Library, r: esf::Ref) {
        self.anims.time = self.scene.time;
        let mut cache = AnimCache::new();
        self.sprite = load_csprite(lib, r, &mut self.anims, &mut cache, &mut self.rand);
        match &self.sprite {
            Some(s) => eprintln!("character: {} nodes, {} clips, skeleton type {}", s.h.nodes.len(), self.anims.anims.len(), s.skel_type),
            None => eprintln!("character: not a CSprite with a play list; drawn in its bind pose"),
        }
    }

    /// The actor's transform: the body's render position, turned to its
    /// heading (`VIMatrix44::Transform(hpr, 1, pos)`).
    fn actor(&self) -> Matrix44<Ee> {
        let p = self.cam.walk.phys.smooth_location();
        let hpr = Vect3::new(self.cam.walk.phys.angles.x, F::ZERO, F::ZERO);
        Matrix44::transform(&hpr, F::ONE, &p)
    }

    /// The character's model matrix and skin palette for this frame:
    /// `UpdateSprite`, then `Process` at the LOD for its distance, then the
    /// centre transform and `SetBlendMatrices`.
    pub fn pose(&mut self) -> ([[f32; 4]; 4], Vec<[[f32; 4]; 4]>) {
        let actor = self.actor();
        let Some(cs) = self.sprite.as_mut() else { return (actor.to_cols(), Vec::new()) };
        self.anims.time = self.scene.time;
        self.cam.walk.update_sprite(cs, &self.anims, &mut self.rand);
        let mut rel = actor;
        rel.m[3].x -= self.eye.position.x;
        rel.m[3].y -= self.eye.position.y;
        rel.m[3].z -= self.eye.position.z;
        let lod = cs.h.calc_lod_level_index(&rel);
        cs.h.process(lod, &self.anims);
        let model = cs.h.calc_trans_about_center(&actor, &self.anims);
        cs.h.set_blend_matrices();
        (model.to_cols(), cs.h.blend.iter().map(|m| m.to_cols()).collect())
    }

    /// One frame: the controls, then the clock moved to `now_ms` and stepped.
    pub fn frame(&mut self, c: Controls, now_ms: i64) {
        let f = F::<Ee>::new;
        let dir = self.eye.direction;
        self.cam.walk.view_relative_move(&dir, f(c.fwd), f(c.strafe));
        self.cam.orbit_in(f(-c.pitch));
        self.cam.orbit_right(f(c.orbit));
        self.scene.time = now_ms;
        self.eye = self.cam.process(&mut self.scene);
    }


    /// The projection and the view matrix through the game's camera.
    pub fn view(&self, aspect: f32) -> (Mat4, Mat4) {
        let eye = Vec3::from(self.eye.position.to_array());
        let dir = Vec3::from(self.eye.direction.to_array());
        let proj = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), aspect, 0.3, 6000.0);
        (proj, glam::camera::rh::view::look_to_mat4(eye, dir.normalize_or(Vec3::Z), Vec3::Y))
    }

    /// Where the camera is.
    pub fn eye_position(&self) -> Vec3 {
        Vec3::from(self.eye.position.to_array())
    }

    pub fn describe(&self) -> String {
        let env = self.scene.env_flags(&self.cam.walk.phys.position);
        let p = &self.cam.walk.phys;
        format!(
            "at ({:.1}, {:.1}, {:.1}) heading {:.2} loco {} flags {:#06x} env {env:#x} cam dist {:.2} anim {:?}",
            p.position.x.0,
            p.position.y.0,
            p.position.z.0,
            p.angles.x.0,
            p.loco_state,
            p.flags,
            self.cam.cur.dist.0,
            self.sprite.as_ref().map(|s| [s.cur[0].anim, s.cur[1].anim])
        )
    }
}

/// The ground under (x, z): a floor probe from high above, as CalcFloor drops
/// one.
pub fn ground(w: &mut World<Ee>, x: f32, z: f32) -> Option<f32> {
    let mut s = CollSphere::<Ee>::default();
    s.init(Vect3::of(x, 2000.0, z), Vect3::of(x, -2000.0, z), F::new(0.001));
    let mut f = Filter::new(FilterKind::WalkFloor, -1);
    w.collide_sphere(&mut s, &mut f).then(|| s.hit_point().y.0)
}

/// HSprite placements drawn posed: one group per (skeleton, node attachment),
/// instanced over its placements. The game shares one skeleton between every
/// placement of a sprite, so each is evaluated once a frame.
pub struct AnimatedProps {
    pub groups: Vec<PropGroup>,
}

pub struct PropGroup {
    pub hsprite: usize,
    pub node: usize,
    pub model: esf::Model,
    pub actors: Vec<Matrix44<Ee>>,
}

impl AnimatedProps {
    pub fn new(world: &World<Ee>, lib: &esf::Library, placements: &[(esf::Ref, Matrix44<Ee>)]) -> Self {
        let mut groups: Vec<PropGroup> = Vec::new();
        for (r, m) in placements {
            let Some(h) = world.hsprite_of(*r) else { continue };
            for &(sp, node) in &world.hsprites[h].1 {
                if node < 0 {
                    continue;
                }
                let node = node as usize;
                if let Some(g) = groups.iter_mut().find(|g| g.hsprite == h && g.node == node) {
                    g.actors.push(*m);
                } else if let Some(model) = lib.build_model(world.sprite_refs[sp]) {
                    groups.push(PropGroup { hsprite: h, node, model, actors: vec![*m] });
                }
            }
        }
        AnimatedProps { groups }
    }

    /// This frame's instance matrices per group: `node.world × centre × actor`
    /// (`VIHSprite::Raster`).
    pub fn matrices(&self, world: &mut World<Ee>) -> Vec<Vec<[[f32; 4]; 4]>> {
        world.anims.time = world.time;
        let mut posed: std::collections::HashMap<usize, Matrix44<Ee>> = std::collections::HashMap::new();
        for g in &self.groups {
            if let std::collections::hash_map::Entry::Vacant(e) = posed.entry(g.hsprite) {
                let h = &mut world.hsprites[g.hsprite].0;
                h.process(-1, &world.anims);
                e.insert(h.calc_trans_about_center(&Matrix44::identity(), &world.anims));
            }
        }
        self.groups
            .iter()
            .map(|g| {
                let centre = posed[&g.hsprite];
                let node = Matrix44::mul(&world.hsprites[g.hsprite].0.nodes[g.node].world, &centre);
                g.actors.iter().map(|a| Matrix44::mul(&node, a).to_cols()).collect()
            })
            .collect()
    }
}
