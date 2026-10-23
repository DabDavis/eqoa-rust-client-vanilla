//! The walk camera's styles and presets, as `VIClient` keeps them:
//! `SetCameraStyle` (0 first person at the race's eye height, 1 and 3 the two
//! orbits, 2 the fixed follow), `InitCameraStyle` (the P key's cycle),
//! `ToggleCameraStyle` (the first world entry's), the presets a camera is
//! saved to and put back from (`SaveCameraPreset`, `ApplyCameraPreset`), and
//! `ApplyRaceCamera`, the start menu's framing of the character from
//! `gRaceCameras` (CLIENT data, read out of `run/client.bin`).
//!
//! The client's own, beside them: the follow orbit zooms out to 11.0 where
//! the game stops at 8.0 (`FOLLOW_MAX_DISTANCE`), and the mouse wheel goes
//! into first person past the closest zoom and back out (`zoom_in`,
//! `zoom_out`).

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
    /// +0x04..+0x10: character select's framing (`ApplyCamera`): target
    /// height, look-at height above it, default pitch, distance
    pub review: [f32; 4],
    /// +0x14..+0x20: customise's and the appearance page's
    pub cust: [f32; 4],
}

/// `gRaceCameras` and `gRaceCamerasEx`, 21 records of 56 bytes each.
pub fn race_cameras(img: &ClientImage) -> [Vec<RaceCamera>; 2] {
    let f = |va: u32| f32::from_bits(img.i32_at(va) as u32);
    [0x00CDD8A0u32, 0x00CDDD38].map(|t| {
        (0..21u32)
            .map(|i| {
                let r = t + 56 * i;
                RaceCamera {
                    race: img.i32_at(r),
                    target_height: f(r + 0x24),
                    look_at_height_above: f(r + 0x28),
                    pitch: f(r + 0x2C),
                    distance: f(r + 0x30),
                    p34: f(r + 0x34),
                    review: [f(r + 0x04), f(r + 0x08), f(r + 0x0C), f(r + 0x10)],
                    cust: [f(r + 0x14), f(r + 0x18), f(r + 0x1C), f(r + 0x20)],
                }
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

/// The follow orbit's (style 3's) zoom-out ceiling: the client's 11.0
/// where the game stores 8.0 (`set_style`), by default; the options'
/// Camera Distance sets it from 8 to 50 (`settings::camera_distance`).
pub const FOLLOW_MAX_DISTANCE: f32 = 11.0;

/// `SetCameraStyle(style)`.
pub fn set_style<S: Scene<Ee>>(cam: &mut WalkCamera<Ee>, style: i32, rc: Option<RaceCamera>, scene: &mut S) {
    let f = F::<Ee>::new;
    let half_pi = f(std::f32::consts::FRAC_PI_2);
    let p34 = rc.map_or(f(1.68), |r| f(r.p34));
    match style {
        2 => {
            cam.target_height = f(1.68);
            cam.min_orbit_distance = f(1.5);
            cam.max_orbit_distance = f(25.0);
            cam.min_collision_dist = f(3.5);
            cam.set_orbit_distance(f(3.38));
            cam.walk.set_pitch_range(f(-std::f32::consts::FRAC_PI_4), half_pi);
            cam.walk.turn_rate = half_pi;
            cam.walk.init_intermediate();
            cam.look_at_height_above = f(1.68);
            cam.set_camera_mode(2);
        }
        3 => {
            cam.target_height = f(1.68);
            cam.min_orbit_distance = f(1.5);
            // The client's, not the game's: SetCameraStyle(3) stores 8.0, and
            // on a big modern screen that felt cramped (F3 report
            // 20261020-193543), so the follow orbit zooms out 11.0, about 40%
            // further; walls still pull it in (`min_collision_dist` and the
            // sweep in `move_camera` are the game's). The QOL ELF raised the
            // same ceiling on the PS2 (claude-notes/qol-patches-20260816.md,
            // the cave on `VIWalkCamera::Process` storing +0x264).
            cam.max_orbit_distance = f(crate::settings::camera_distance());
            cam.min_collision_dist = f(3.5);
            cam.set_orbit_distance(f(3.38));
            cam.walk.set_pitch_range(f(-std::f32::consts::FRAC_PI_4), half_pi);
            cam.set_def_orbit_pitch_angle(f(0.261_799_4));
            cam.walk.turn_rate = f(std::f32::consts::TAU);
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

/// The Extended page's Camera Turn Speed (`settings::camera_turn`, percent
/// of the game's), the client's own: how fast the stick and the keys turn
/// the view. In the orbiting cameras (1 and 3) the orbit's push
/// (`angle_accel`, `InitCameraIntermediate`'s from the default orbit speed,
/// a quarter turn a second); in the two that look out of the body (0 and 2)
/// the body's turn (`turn_rate`, `SetCameraStyle`'s quarter turn a second,
/// and its push `turn_accel`, `InitIntermediate`'s, which `UpdateGame`
/// recomputes from the rate every frame). The body's own turn in the orbits
/// stays the game's. At 100 the game's values, bit for bit. Run every frame,
/// as Camera Distance's ceiling is, so a slider moved applies at once; the
/// mouse and the touch screen look by the pixel and are not changed.
pub fn turn_speed(cam: &mut WalkCamera<Ee>, percent: u16) {
    let f = F::<Ee>::new;
    let scaled = |v: F<Ee>| if percent == crate::settings::TURN_GAME { v } else { v * f(percent as f32 / 100.0) };
    let dt = F::new(cam.walk.tick_ms as f32) * f(0.001);
    let d = cam.angle_damping.powf(dt);
    let speed = scaled(cam.def_orbit_angle);
    cam.angle_accel = (speed - speed * d) / (d * dt);
    if matches!(cam.camera_mode, 0 | 2) {
        let rate = scaled(f(std::f32::consts::FRAC_PI_2));
        let d = cam.walk.turn_damping.powf(dt);
        cam.walk.turn_rate = rate;
        cam.walk.turn_accel = (rate - rate * d) / (d * dt);
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

/// What the wheel's way into first person leaves to come back to (the
/// client's own: `mouse::Zoom`, `Play::wheel_style`): the camera as
/// `SaveCameraPreset` keeps it, and mode 3's default pitch (the mouse's
/// tilt, which its preset's pitch does not hold near a wall).
#[derive(Clone, Copy, Debug)]
pub struct ZoomBack {
    pub preset: Preset,
    pub def_pitch: F<Ee>,
}

/// The wheel past the closest zoom: into first person as P's cycle goes
/// there (style 0); what to come back to.
pub fn zoom_in<S: Scene<Ee>>(cam: &mut WalkCamera<Ee>, rc: Option<RaceCamera>, scene: &mut S) -> ZoomBack {
    let back = ZoomBack { preset: save(cam), def_pitch: cam.def_orbit_pitch };
    set_style(cam, 0, rc, scene);
    back
}

/// The wheel out of first person: back to `back`'s style (behind the
/// player at a clear angle, as `SetCameraStyle` chooses), its distance and
/// its tilt.
pub fn zoom_out<S: Scene<Ee>>(cam: &mut WalkCamera<Ee>, back: &ZoomBack, rc: Option<RaceCamera>, scene: &mut S) {
    set_style(cam, back.preset.mode, rc, scene);
    match back.preset.mode {
        3 => {
            cam.set_def_orbit_pitch_angle(back.def_pitch);
            cam.set_orbit_distance(back.preset.distance);
        }
        1 => {
            cam.set_orbit_pitch_angle(back.preset.pitch);
            cam.set_orbit_distance(back.preset.distance);
        }
        _ => {}
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{PI, TAU};
    use vi::vect::Vect3;

    /// A scene with nothing in it, its clock the test's.
    struct Clock(i64);

    impl Scene<Ee> for Clock {
        fn time(&self) -> i64 {
            self.0
        }
        fn collide_sphere(&mut self, _: &mut vi::coll::CollSphere<Ee>, _: &mut vi::scene::Filter) -> bool {
            false
        }
        fn collide_env(&mut self, _: &mut vi::coll::EnvRay<Ee>) -> bool {
            false
        }
        fn env_flags(&self, _: &Vect3<Ee>) -> u32 {
            0
        }
    }

    /// The play camera in camera style `style`.
    fn styled(style: i32) -> (WalkCamera<Ee>, Clock) {
        let mut scene = Clock(0);
        let mut cam = WalkCamera::<Ee>::default();
        cam.init(Vect3::of(100.0, 0.0, 100.0), Vect3::of(0.5, 0.0, 0.0), &mut scene);
        set_style(&mut cam, style, None, &mut scene);
        (cam, scene)
    }

    /// What the turn speed sets, as bits: the orbit's push, the body's turn
    /// rate and its push.
    fn turns(cam: &WalkCamera<Ee>) -> [u32; 3] {
        [cam.angle_accel.bits(), cam.walk.turn_rate.bits(), cam.walk.turn_accel.bits()]
    }

    /// At the game's own speed every style keeps the game's numbers bit for
    /// bit, and gets them back after another speed; another speed changes
    /// them (the control), but never the body's own turn in the orbits.
    #[test]
    fn the_games_turn_speed_is_the_games_bit_for_bit() {
        for style in 0..4 {
            let (mut cam, _) = styled(style);
            let game = turns(&cam);
            turn_speed(&mut cam, 100);
            assert_eq!(turns(&cam), game, "style {style}");
            turn_speed(&mut cam, 175);
            let fast = turns(&cam);
            assert_ne!(fast[0], game[0], "style {style}: the orbit's push");
            if matches!(style, 0 | 2) {
                assert_ne!(fast[1..], game[1..], "style {style}: the body's turn");
            } else {
                assert_eq!(fast[1..], game[1..], "style {style}: the body's own turn stays the game's");
            }
            turn_speed(&mut cam, 100);
            assert_eq!(turns(&cam), game, "style {style}: back to the game's");
        }
    }

    /// How far the view turns in two seconds of the game's frames with the
    /// stick held over: the orbit's angle in the orbiting styles (1 and 3),
    /// the body's heading in the two that look out of it (0 and 2).
    fn turned(style: i32, percent: u16) -> f32 {
        let (mut cam, mut scene) = styled(style);
        let orbit = matches!(style, 1 | 3);
        let angle = |c: &WalkCamera<Ee>| if orbit { c.cur.orbit.x.0 } else { c.walk.phys.angles.x.0 };
        let (mut total, mut last) = (0.0f32, angle(&cam));
        for _ in 0..60 {
            turn_speed(&mut cam, percent);
            if orbit {
                cam.orbit_right(F::ONE);
            } else {
                cam.walk.turn_left(F::ONE);
            }
            scene.0 += 33;
            cam.process(&mut scene);
            let a = angle(&cam);
            let d = a - last;
            total += if d > PI { d - TAU } else if d < -PI { d + TAU } else { d };
            last = a;
        }
        total.abs()
    }

    /// Camera Turn Speed turns the view that much faster or slower, by the
    /// stick and the keys alike (both are these inputs), in every style.
    #[test]
    fn the_turn_speed_scales_the_views_turn() {
        for style in 0..4 {
            let game = turned(style, 100);
            assert!(game > 1.0, "style {style}: the game's turns ({game})");
            let twice = turned(style, 200) / game;
            assert!((1.9..2.1).contains(&twice), "style {style}: twice the game's, {twice}");
            let half = turned(style, 50) / game;
            assert!((0.45..0.55).contains(&half), "style {style}: half the game's, {half}");
        }
    }
}
