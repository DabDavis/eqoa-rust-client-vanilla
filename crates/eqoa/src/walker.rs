//! The character on the ground: the ported `VIWalkCamera` (crate `vi`, on the
//! console's float model), set up and driven the way CLIENT's
//! `VIClient::UpdateControls` drives it in third person (camera mode 3):
//!
//!   fwd = -left.y, strafe = left.x  ->  ViewRelativeMove(camera dir, fwd, strafe)
//!   right.y                         ->  OrbitIn(-pitch)
//!   right.x                         ->  OrbitRight(orbit)
//!   then VIWalkCamera::Process, which steps everything in 32 ms ticks.
//!
//! The scene it collides with is [`crate::standin::TerrainScene`], a stand-in.

use crate::standin::TerrainScene;
use glam::{Mat4, Vec3};
use vi::camera::{Eye, WalkCamera};
use vi::fpu::{Ee, F};
use vi::vect::Vect3;

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
    pub scene: TerrainScene,
    pub eye: Eye<Ee>,
}

impl Walker {
    /// Stand the character at (x, z) on the terrain, facing `heading`, with
    /// the camera behind it.
    pub fn new(scene: TerrainScene, x: f32, z: f32, heading: f32) -> Self {
        let mut scene = scene;
        let y = scene.ground(x, z).unwrap_or(0.0);
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
        Walker { cam, scene, eye }
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

    /// The character's model matrix: the body's render position, turned to
    /// its heading (HPR's heading turns +Z toward +X).
    pub fn body_matrix(&self) -> [[f32; 4]; 4] {
        let p = self.cam.walk.phys.smooth_location().to_array();
        let h = self.cam.walk.phys.angles.x.0;
        (Mat4::from_translation(Vec3::from(p)) * Mat4::from_rotation_y(h)).to_cols_array_2d()
    }

    pub fn view(&self, aspect: f32) -> Mat4 {
        let eye = Vec3::from(self.eye.position.to_array());
        let dir = Vec3::from(self.eye.direction.to_array());
        let proj = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), aspect, 0.3, 6000.0);
        proj * glam::camera::rh::view::look_to_mat4(eye, dir.normalize_or(Vec3::Z), Vec3::Y)
    }

    pub fn describe(&self) -> String {
        let p = &self.cam.walk.phys;
        format!(
            "at ({:.1}, {:.1}, {:.1}) heading {:.2} loco {} flags {:#06x} cam dist {:.2}",
            p.position.x.0,
            p.position.y.0,
            p.position.z.0,
            p.angles.x.0,
            p.loco_state,
            p.flags,
            self.cam.cur.dist.0
        )
    }
}
