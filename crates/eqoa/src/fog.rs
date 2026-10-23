//! The game's fog and its far plane, and the Extended page's Fog.
//!
//! `SetupTimeOfDayFog` fogs linearly from the terrain profiles' near to their
//! far (`vi::tod::fog`), and `VIWalkCamera::SetCamera`, as `VIClient::Process`
//! calls it every frame, makes the profiles' far the camera's far plane
//! (`VIScene::CalcFarPlane`, `Grid::far_plane`): the world is all fog by the
//! far distance and nothing past it is drawn, so the scenery fades out into
//! the sky instead of standing against it in the fog's colour. The sky is
//! drawn through its own far plane (`SKY_FAR`), as before.
//!
//! Fog (the client's, 0 to `STEPS`, the game's at `GAME`) changes the
//! distance fog only; inside water, lava, slime or mud the game's own fog
//! stays. Below the game's it thins: the fog begins nearer the far distance
//! (all of it at the far plane still, so nothing ends in a hard edge), and
//! at 0 it is off and the world is drawn as far as it is loaded. Above, it
//! thickens: the near and far distances, and the far plane with them, come
//! in, to half at `STEPS`.

use glam::{Mat4, Vec3};
use vi::fpu::Ee;
use vi::vect::Vect3;
use vi::world::World;

/// Fog's steps (`ClientOptions::fog`).
pub const STEPS: u8 = 10;
/// The game's fog, the middle step.
pub const GAME: u8 = 5;
/// The far plane with no fog, and the sky's: the client's before it drew the
/// game's.
pub const SKY_FAR: f32 = 6000.0;

/// How much nearer the distances come above the game's, a step.
const THICKEN: f32 = 0.1;

/// The far plane at `eye` through Fog `level`: the game's (`CalcFarPlane`),
/// that brought in above the game's step, `SKY_FAR` with the fog off or no
/// profile grid.
pub fn far_plane(world: &World<Ee>, eye: Vec3, level: u8) -> f32 {
    match world.grid.as_ref() {
        Some(g) if level > 0 => {
            let far = g.far_plane(&Vect3::of(eye.x, eye.y, eye.z)).0;
            far * scale(level)
        }
        _ => SKY_FAR,
    }
}

/// The distance fog through Fog `level`, from the game's `near` and `far`:
/// none at 0; at the game's step exactly the game's.
pub fn distances(level: u8, near: f32, far: f32) -> Option<(f32, f32)> {
    match level {
        0 => None,
        GAME => Some((near, far)),
        l if l < GAME => Some((far - (far - near) * l as f32 / GAME as f32, far)),
        l => Some((near * scale(l), far * scale(l))),
    }
}

/// The distances' scale above the game's step (1 at and below it).
fn scale(level: u8) -> f32 {
    if level > GAME { 1.0 - THICKEN * (level.min(STEPS) - GAME) as f32 } else { 1.0 }
}

/// `proj`, a right-handed perspective with a [0, 1] depth range (glam's
/// `perspective`), with its far plane moved to `far`: its near plane kept.
pub fn with_far(proj: Mat4, far: f32) -> Mat4 {
    let (a, b) = (proj.z_axis.z, proj.w_axis.z);
    // a = f / (n - f) and b = n f / (n - f), so n = b / a
    if a == 0.0 {
        return proj;
    }
    let near = b / a;
    let mut m = proj;
    m.z_axis.z = far / (near - far);
    m.w_axis.z = near * far / (near - far);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `with_far` is the perspective glam builds with that far plane, the
    /// rest of it untouched; an unchanged far gives the matrix back.
    #[test]
    fn a_far_plane_moved_is_the_perspective_with_it() {
        let p = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 16.0 / 9.0, 0.3, 6000.0);
        let want = glam::camera::rh::proj::directx::perspective(45f32.to_radians(), 16.0 / 9.0, 0.3, 800.0);
        let got = with_far(p, 800.0);
        for (g, w) in got.to_cols_array().iter().zip(want.to_cols_array()) {
            assert!((g - w).abs() <= w.abs() * 1e-5 + 1e-6, "{got:?}\n{want:?}");
        }
        let back = with_far(got, 6000.0);
        for (g, w) in back.to_cols_array().iter().zip(p.to_cols_array()) {
            assert!((g - w).abs() <= w.abs() * 1e-5 + 1e-6);
        }
        // a point at 700 is inside the 800 plane, one at 900 past it (the
        // control: the old plane keeps both)
        let at = |m: Mat4, d: f32| {
            let c = m * glam::Vec4::new(0.0, 0.0, -d, 1.0);
            c.z / c.w
        };
        assert!(at(got, 700.0) < 1.0 && at(got, 900.0) > 1.0);
        assert!(at(p, 900.0) < 1.0);
    }

    /// The game's step is the game's distances bit for bit; below it the
    /// fog begins later and still ends at the far distance; above it both
    /// come in; 0 is none.
    #[test]
    fn the_fog_steps() {
        let (near, far) = (250.0f32, 800.0f32);
        let (n, f) = distances(GAME, near, far).unwrap();
        assert_eq!((n.to_bits(), f.to_bits()), (near.to_bits(), far.to_bits()));
        assert_eq!(distances(0, near, far), None);
        let mut last = near;
        for l in (1..GAME).rev() {
            let (n, f) = distances(l, near, far).unwrap();
            assert_eq!(f, far, "thinner fog still ends at the far plane");
            assert!(n > last && n < far, "step {l}: {n}");
            last = n;
        }
        let mut last = far;
        for l in GAME + 1..=STEPS {
            let (n, f) = distances(l, near, far).unwrap();
            assert!(f < last && n < near, "step {l}: {n}..{f}");
            last = f;
        }
        assert_eq!(distances(STEPS, near, far), Some((125.0, 400.0)));
        // the control: a step that thinned by moving the far plane out would
        // leave the world ending unfogged at the plane
        assert!(distances(1, near, far).unwrap().1 <= far);
    }
}
