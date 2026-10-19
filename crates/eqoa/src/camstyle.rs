//! The walk camera's styles and presets, as `VIClient` keeps them:
//! `SetCameraStyle` (0 first person at the race's eye height, 1 and 3 the two
//! orbits, 2 the fixed follow), `InitCameraStyle` (the P key's cycle),
//! `ToggleCameraStyle` (the first world entry's), the presets a camera is
//! saved to and put back from (`SaveCameraPreset`, `ApplyCameraPreset`), and
//! `ApplyRaceCamera`, the start menu's framing of the character from
//! `gRaceCameras` (CLIENT data, read out of `run/client.bin`).

use crate::client_image::ClientImage;
use vi::camera::WalkCamera;
use vi::fpu::{Ee, F};
use vi::scene::Scene;

/// `VIRaceCamera`, the fields the play camera reads.
#[derive(Clone, Copy, Debug)]
pub struct RaceCamera {
    pub race: i32,
    pub target_height: f32,
    pub look_at_height_above: f32,
    pub pitch: f32,
    pub distance: f32,
    /// `m_p34`: the eye height first person and style 3's look-at
    pub p34: f32,
}

/// `gRaceCameras` and `gRaceCamerasEx`, 21 records of 56 bytes each.
pub fn race_cameras(img: &ClientImage) -> [Vec<RaceCamera>; 2] {
    let f = |va: u32| f32::from_bits(img.i32_at(va) as u32);
    [0x00CDD8A0u32, 0x00CDDD38].map(|t| {
        (0..21u32)
            .map(|i| {
                let r = t + 56 * i;
                RaceCamera { race: img.i32_at(r), target_height: f(r + 0x24), look_at_height_above: f(r + 0x28), pitch: f(r + 0x2C), distance: f(r + 0x30), p34: f(r + 0x34) }
            })
            .collect()
    })
}

/// `GetRaceCamera(race, expansion)`: the expansion's table with its bit 1;
/// out of range is race 0; the LAST record of the race.
pub fn race_camera(tables: &[Vec<RaceCamera>; 2], race: i32, expansion: u32) -> Option<RaceCamera> {
    let race = if (0..21).contains(&race) { race } else { 0 };
    tables[(expansion & 2 != 0) as usize].iter().rev().find(|c| c.race == race).copied()
}

/// `VICameraPreset`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Preset {
    pub mode: i32,
    pub target_height: F<Ee>,
    pub look_at_height_above: F<Ee>,
    pub pitch: F<Ee>,
    pub distance: F<Ee>,
    pub angle: F<Ee>,
}

/// `SaveCameraPreset`.
pub fn save(cam: &WalkCamera<Ee>) -> Preset {
    Preset {
        mode: cam.camera_mode,
        target_height: cam.target_height,
        look_at_height_above: cam.look_at_height_above,
        pitch: cam.last.orbit.y,
        distance: cam.last.dist,
        angle: cam.last.orbit.x,
    }
}

/// `ApplyCameraPreset`.
pub fn apply(cam: &mut WalkCamera<Ee>, p: &Preset) {
    cam.set_camera_mode(p.mode);
    cam.target_height = p.target_height;
    cam.look_at_height_above = p.look_at_height_above;
    cam.set_orbit_pitch_angle(p.pitch);
    cam.set_orbit_distance(p.distance);
    cam.set_orbit_angle(p.angle);
}

/// `SetCameraStyle(style)`.
pub fn set_style<S: Scene<Ee>>(cam: &mut WalkCamera<Ee>, style: i32, rc: Option<RaceCamera>, scene: &mut S) {
    let f = F::<Ee>::new;
    let half_pi = f(1.570_796_4);
    let p34 = rc.map_or(f(1.68), |r| f(r.p34));
    match style {
        2 => {
            cam.target_height = f(1.68);
            cam.min_orbit_distance = f(1.5);
            cam.max_orbit_distance = f(25.0);
            cam.min_collision_dist = f(3.5);
            cam.set_orbit_distance(f(3.38));
            cam.walk.set_pitch_range(f(-0.785_398_2), half_pi);
            cam.walk.turn_rate = half_pi;
            cam.walk.init_intermediate();
            cam.look_at_height_above = f(1.68);
            cam.set_camera_mode(2);
        }
        3 => {
            cam.target_height = f(1.68);
            cam.min_orbit_distance = f(1.5);
            cam.max_orbit_distance = f(8.0);
            cam.min_collision_dist = f(3.5);
            cam.set_orbit_distance(f(3.38));
            cam.walk.set_pitch_range(f(-0.785_398_2), half_pi);
            cam.set_def_orbit_pitch_angle(f(0.261_799_4));
            cam.walk.turn_rate = f(6.283_185_5);
            cam.walk.init_intermediate();
            cam.look_at_height_above = p34;
            cam.set_camera_mode(3);
            let a = cam.walk.committed.angles.x + f(std::f32::consts::PI);
            cam.choose_orbit_angle(a, scene);
        }
        1 => {
            cam.target_height = f(1.1);
            cam.min_orbit_distance = f(1.5);
            cam.max_orbit_distance = f(25.0);
            cam.min_collision_dist = f(3.5);
            cam.walk.set_pitch_range(-half_pi, half_pi);
            cam.set_def_orbit_pitch_angle(f(0.436_332_32));
            cam.walk.turn_rate = half_pi;
            cam.walk.init_intermediate();
            cam.set_camera_mode(1);
            let a = cam.walk.committed.angles.x + f(std::f32::consts::PI);
            cam.choose_orbit_angle(a, scene);
        }
        _ => {
            // 0, and anything else: first person (at the race's eye height
            // only for 0)
            cam.walk.set_pitch_range(-half_pi, half_pi);
            cam.walk.turn_rate = half_pi;
            cam.walk.init_intermediate();
            cam.set_camera_mode(0);
            if style == 0 {
                cam.look_at_height = p34;
            }
        }
    }
}

/// `InitCameraStyle`: the P key's cycle (third person and the fixed follow
/// to first person, first person to the option's orbit), then saved.
pub fn cycle<S: Scene<Ee>>(cam: &mut WalkCamera<Ee>, fixed_option: bool, rc: Option<RaceCamera>, scene: &mut S) -> Preset {
    let style = match cam.camera_mode {
        0 => {
            if fixed_option {
                2
            } else {
                3
            }
        }
        _ => 0,
    };
    set_style(cam, style, rc, scene);
    save(cam)
}

/// `ToggleCameraStyle`: the option's orbit, from behind, saved.
pub fn toggle<S: Scene<Ee>>(cam: &mut WalkCamera<Ee>, fixed_option: bool, rc: Option<RaceCamera>, scene: &mut S) -> Preset {
    set_style(cam, if fixed_option { 2 } else { 3 }, rc, scene);
    let a = cam.walk.committed.angles.x + F::new(std::f32::consts::PI);
    cam.set_orbit_angle(a);
    save(cam)
}

/// `ApplyRaceCamera`: the camera saved, then framed on the character by
/// the race's record (orbit mode 1, from the best angle), halted.
pub fn apply_race<S: Scene<Ee>>(cam: &mut WalkCamera<Ee>, rc: Option<RaceCamera>, scene: &mut S) -> Option<Preset> {
    let rc = rc?;
    let saved = save(cam);
    let f = F::<Ee>::new;
    cam.target_height = f(rc.target_height);
    cam.look_at_height_above = f(rc.look_at_height_above);
    cam.set_orbit_pitch_angle(f(rc.pitch));
    cam.set_orbit_distance(f(rc.distance));
    cam.set_camera_mode(1);
    let a = cam.walk.committed.angles.x;
    cam.choose_orbit_angle(a, scene);
    cam.halt();
    Some(saved)
}
