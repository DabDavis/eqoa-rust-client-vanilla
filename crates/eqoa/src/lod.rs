//! What of a character the game draws at a distance: `VILODSprite::Raster`'s
//! level of a skinned LOD body (the player races' models: 40 of CHAR.ESF's 568
//! CSprites, the human and the barbarian among them, carry a near mesh to 10
//! and a far one to 100), `VIHSprite::Raster`'s level of its skeleton's
//! animation (`anim_level`: every player race's skeleton has levels at 5, 15
//! and 40, and the nodes past a level take their rest frame), and
//! `VIScene::RasterActor`'s off distance, past which an actor is not drawn at
//! all (`VICSprite::CalcLODOffDist`: an LOD body's last level, else
//! `VISprite`'s 500). All on the EE's floats, as the game computes them.
//!
//! The client's own, beside them. Its follow orbit zooms out past the
//! game's 8.0 (to 11, or as far as 50 by the options' Camera Distance), and
//! the levels and the off distances are measured from where the game's
//! camera would be at its own ceiling (`eye`: the eye drawn in along its
//! line to 8.0 from the orbit's pivot), so zooming further only moves the
//! camera back: the player's own body keeps the near level it always had
//! on the console (at the client's 11 a barbarian, nearer 11 than 10 from
//! the camera, lost its muscle: F3 20261021-001920), and characters are
//! drawn as far round the player, at the same levels, as the console drew
//! them at its farthest. The skeletons' animation levels are measured from
//! there too: from the camera itself, a player zoomed out to 50 saw 22 of a
//! human's 40 nodes (27 of a barbarian's 43) of their own body stop moving,
//! the arms and legs held at their rest frames (F3 pass 11: "the LoD off
//! setting only disable the mid level LoD, not when zoomed far out"). The
//! options' Level of Detail off draws every LOD body's near level out to its
//! off distance (`chosen`) and poses every skeleton at the nearest level
//! (`anim_level`), the player's own and every NPC's, at any zoom. The
//! options' NPC/PC Distance draws every character at least that far
//! (`view_dist`), an LOD body past its last level at that level (the game's
//! 100 changes nothing: no character's off distance is nearer).
//!
//! The name plates go by the same eye: `VINameSprite::Raster`'s 22 (or the
//! options' Name Distance) is measured from it, so zooming out keeps the
//! names the console showed round the player, and one size for every plate
//! comes from the zoom (`plate_scale`).

use vi::camera::WalkCamera;
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
use vi::hsprite::HSprite;
use vi::vect::Vect3;

/// `VISprite::VISprite`'s off distance, which a CSprite keeps unless its body
/// is an LOD sprite.
pub const OFF_DIST: f32 = 500.0;

/// `VILODSprite::Raster`: the level drawn of an LOD body whose levels reach
/// `dists` (`m_levels[i].m_dist`), `d` being the translation of its model-view
/// (the body's centre less the camera): the first level whose distance squared
/// is not below the translation's length squared, or `None` past the last,
/// where nothing is drawn.
pub fn level(dists: &[f32], d: &Vect3<Ee>) -> Option<u8> {
    let d2 = d.x * d.x + d.y * d.y + d.z * d.z;
    let i = dists.iter().take_while(|&&l| F::<Ee>::new(l) * F::new(l) < d2).count();
    (i < dists.len()).then_some(i as u8)
}

/// `VICSprite::CalcLODOffDist`: a CSprite's off distance, its LOD body's
/// (the last level's distance) when it has one.
pub fn off_dist(dists: &[f32]) -> f32 {
    dists.last().copied().unwrap_or(OFF_DIST)
}

/// The distance a character is drawn to: its off distance (`off_dist`), or
/// the options' NPC/PC Distance where that is farther.
pub fn view_dist(dists: &[f32]) -> f32 {
    off_dist(dists).max(crate::settings::entity_view() as f32)
}

/// `VIScene::RasterActor`: whether an actor at `at` is drawn from a camera at
/// `eye`, its sprite's off distance being `off`.
pub fn within(at: &Vect3<Ee>, eye: &Vect3<Ee>, off: f32) -> bool {
    let (dx, dy, dz) = (at.x - eye.x, at.y - eye.y, at.z - eye.z);
    let d2 = dx * dx + dy * dy + dz * dz;
    d2 < F::new(off) * F::new(off)
}

/// A character's body as the camera at `eye` sees it this frame: the level
/// of its LOD body (if it has one) for its centre transform `centre`, under
/// the options' Level of Detail and NPC/PC Distance. Whether it is
/// drawn at all is `within`'s, at the actor's place (to `view_dist`).
pub fn place(body: &mut render::GpuModel, centre: &Matrix44<Ee>, eye: &Vect3<Ee>) {
    body.level = chosen(&body.lod_dists, &from_eye(centre, eye), crate::settings::client().lod, view_dist(&body.lod_dists));
}

/// The level drawn of a body whose levels reach `dists`, `d` from the
/// camera: `VILODSprite::Raster`'s with Level of Detail on (`lod`), and past
/// the last level, nearer than `view`, the last (the client's: the game's
/// draws nothing there, and `view` at its off distance changes nothing);
/// off, the near one at any distance (past the distance `within` hides it).
/// A body with no levels has none either way.
pub fn chosen(dists: &[f32], d: &Vect3<Ee>, lod: bool, view: f32) -> Option<u8> {
    if !lod {
        return (!dists.is_empty()).then_some(0);
    }
    level(dists, d).or_else(|| (!dists.is_empty() && within(d, &Vect3::default(), view)).then(|| dists.len() as u8 - 1))
}

/// The game's zoom-out ceiling for the follow orbit (`SetCameraStyle(3)`
/// stores 8.0), which the client's goes past.
pub const GAME_CEILING: f32 = 8.0;

/// The follow orbit's pivot: the player's place raised by the target height
/// and the height offset, as `MoveCamera` places the eye.
fn pivot(cam: &WalkCamera<Ee>) -> Vect3<Ee> {
    let p = &cam.walk.phys;
    let mut pivot = p.position;
    pivot.y = pivot.y + cam.target_height + p.height_offset;
    pivot
}

/// How far the follow orbit's camera (mode 3) at `eye` is from its pivot:
/// the zoom; None in the other modes (whose ceilings are the game's own).
pub fn orbit_distance(cam: &WalkCamera<Ee>, eye: &Vect3<Ee>) -> Option<F<Ee>> {
    (cam.camera_mode == 3).then(|| eye.distance(&pivot(cam)))
}

/// Where the levels and off distances are measured from: the camera's
/// `eye`, but in the follow orbit (mode 3) no farther from the orbit's pivot
/// (`pivot`) than the game's ceiling, along the same line. Within the
/// ceiling, and in the other modes (whose ceilings are the game's own), the
/// eye itself.
pub fn eye(cam: &WalkCamera<Ee>, eye: &Vect3<Ee>) -> Vect3<Ee> {
    let Some(len) = orbit_distance(cam, eye) else { return *eye };
    let ceiling = F::new(GAME_CEILING);
    if len <= ceiling {
        return *eye;
    }
    let pivot = pivot(cam);
    let k = ceiling / len;
    Vect3::new(pivot.x + (eye.x - pivot.x) * k, pivot.y + (eye.y - pivot.y) * k, pivot.z + (eye.z - pivot.z) * k)
}

/// `VIHSprite::Raster`'s `Process(CalcLODLevelIndex(modelview))`: the level
/// a skeleton is posed at for an actor whose transform is `actor`, seen from
/// `eye` (`lod::eye`'s, as the meshes' levels are: the model view's
/// translation is the actor's place less the camera's, its length the
/// distance); `ProcessPlayList` gives each node at or past it its rest frame
/// (every player race's levels are at 5, 15 and 40: of a human's 40 nodes 2
/// rest from 5, 14 from 15 and 22 from 40). With the options' Level of Detail
/// off (`lod` false) the nearest, -1, at any distance, as the game poses one
/// nearer than its first level.
pub fn anim_level(h: &HSprite<Ee>, actor: &Matrix44<Ee>, eye: &Vect3<Ee>, lod: bool) -> i32 {
    if !lod {
        return -1;
    }
    let mut rel = *actor;
    rel.m[3].x -= eye.x;
    rel.m[3].y -= eye.y;
    rel.m[3].z -= eye.z;
    h.calc_lod_level_index(&rel)
}

/// The least a name plate is drawn at against the game's size
/// (`plate_scale`), at the farthest zoom.
pub const PLATE_FLOOR: f32 = 0.5;

/// Every name plate's size this frame against the game's (`VINameSprite`
/// laid out on the frame's 448 lines, the HUD's scale), from the zoom (F3
/// pass 11, the user's choice: one size for every name, as the map's dart
/// keeps one): the game's while the follow orbit's camera is no farther from
/// its pivot than the game's ceiling (`orbit_distance`), smaller as it pulls
/// back past it, in proportion, to `PLATE_FLOOR` at Camera Distance's
/// farthest; in the other camera modes (whose ceilings are the game's) the
/// game's. `k` is the window's lines to the frame's 448: a plate is never
/// smaller than the font's own texels one to a window pixel, under which the
/// nearest texel drops strokes (at 448 lines or fewer, the game's size at
/// every zoom).
pub fn plate_scale(cam: &WalkCamera<Ee>, eye: &Vect3<Ee>, k: f32) -> f32 {
    orbit_distance(cam, eye).map_or(1.0, |d| plate_scale_at(d.0, k))
}

/// `plate_scale` for a camera `d` from the orbit's pivot.
pub fn plate_scale_at(d: f32, k: f32) -> f32 {
    let floor = PLATE_FLOOR.max(1.0 / k.max(f32::MIN_POSITIVE)).min(1.0);
    let far = crate::settings::CAMERA_MAX as f32;
    let t = ((d - GAME_CEILING) / (far - GAME_CEILING)).clamp(0.0, 1.0);
    1.0 - (1.0 - floor) * t
}

/// The model-view translation `VILODSprite::Raster` reads, for a body whose
/// centre transform (`CalcTransAboutCenter`) is `centre`: its translation less
/// the camera's position (the view's rotation keeps the length).
pub fn from_eye(centre: &Matrix44<Ee>, eye: &Vect3<Ee>) -> Vect3<Ee> {
    Vect3::new(centre.m[3].x - eye.x, centre.m[3].y - eye.y, centre.m[3].z - eye.z)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: [f32; 2] = [10.0, 100.0];

    fn at(x: f32) -> Vect3<Ee> {
        Vect3::of(x, 0.0, 0.0)
    }

    /// The near mesh up to 10 (10 itself included: the test is `dist² < d²`),
    /// the far one up to 100, nothing past it; and the same for any direction.
    #[test]
    fn levels_by_distance() {
        assert_eq!(level(&BODY, &at(0.0)), Some(0));
        assert_eq!(level(&BODY, &at(10.0)), Some(0));
        assert_eq!(level(&BODY, &at(10.001)), Some(1));
        assert_eq!(level(&BODY, &at(100.0)), Some(1));
        assert_eq!(level(&BODY, &at(100.01)), None);
        assert_eq!(level(&BODY, &Vect3::of(6.0, 6.0, 6.0)), Some(1), "|(6,6,6)| = 10.4");
        assert_eq!(level(&BODY, &Vect3::of(5.0, -5.0, 5.0)), Some(0), "|(5,-5,5)| = 8.7");
        // a body with no levels draws none
        assert_eq!(level(&[], &at(1.0)), None);
    }

    /// The control: the levels counted from the other end, or with `<=`,
    /// differ where the rule says they must.
    #[test]
    fn the_boundary_belongs_to_the_nearer_level() {
        let le = |d: f32| BODY.iter().take_while(|&&l| l * l <= d * d).count();
        assert_ne!(le(10.0), level(&BODY, &at(10.0)).unwrap() as usize);
        assert_eq!(level(&BODY, &at(9.0)), Some(0));
    }

    /// Level of Detail off: the near level at any distance (past 100 the
    /// actor is not drawn at all: `within`); on, the game's. The control is
    /// the game's rule at 50.
    #[test]
    fn level_of_detail_off_keeps_the_near_level() {
        for x in [0.0, 10.001, 50.0, 100.0, 150.0] {
            assert_eq!(chosen(&BODY, &at(x), false, 100.0), Some(0), "{x}");
        }
        assert_eq!(chosen(&BODY, &at(50.0), true, 100.0), Some(1));
        assert_eq!(chosen(&BODY, &at(150.0), true, 100.0), None);
        assert_eq!(chosen(&[], &at(5.0), false, 100.0), None, "no levels, none");
    }

    /// NPC/PC Distance: at the game's 100 a body's levels and off
    /// distance are the game's (the control: none past 100, a sprite with no
    /// LOD body still to 500); farther, the far level out to it and nothing
    /// past it, and a sprite with no LOD body to whichever is farther.
    #[test]
    fn the_view_distance_draws_farther() {
        crate::settings::testing::reset(false);
        let set = |d: u16| crate::settings::set_client(crate::settings::ClientOptions { entity_view: d, ..Default::default() });
        set(100);
        assert_eq!((view_dist(&BODY), view_dist(&[])), (100.0, OFF_DIST), "the game's");
        assert_eq!(chosen(&BODY, &at(100.01), true, view_dist(&BODY)), None);
        set(600);
        assert_eq!((view_dist(&BODY), view_dist(&[])), (600.0, 600.0));
        assert_eq!(chosen(&BODY, &at(50.0), true, 600.0), Some(1), "within the levels, the game's");
        assert_eq!(chosen(&BODY, &at(5.0), true, 600.0), Some(0));
        assert_eq!(chosen(&BODY, &at(450.0), true, 600.0), Some(1), "past them, the far level");
        assert_eq!(chosen(&BODY, &at(600.0), true, 600.0), None, "at the distance is out, as `within`");
        assert_eq!(chosen(&[], &at(450.0), true, 600.0), None, "no levels, none");
        set(300);
        assert_eq!(view_dist(&[]), OFF_DIST, "no nearer than the game's 500");
        let eye = Vect3::of(25000.0, 60.0, 16000.0);
        assert!(within(&Vect3::of(25299.0, 60.0, 16000.0), &eye, view_dist(&BODY)));
        assert!(!within(&Vect3::of(25301.0, 60.0, 16000.0), &eye, view_dist(&BODY)));
        crate::settings::testing::reset(false);
    }

    /// The follow orbit pulled out past the game's 8.0: the levels are
    /// measured from 8.0 along the same line, so the player's own body (its
    /// centre a little under the pivot) keeps the near level at 11 and at
    /// 50, and a character 60 ahead stays within its off distance of 100;
    /// within 8, and in the fixed follow camera (mode 2, whose 25 is the
    /// game's), the eye itself. The control: the eye itself at 11 gives the
    /// far level, and at 50 the character ahead is not drawn.
    #[test]
    fn levels_are_measured_from_the_games_farthest_camera() {
        let mut cam = WalkCamera::<Ee> { camera_mode: 3, target_height: F::new(1.68), ..Default::default() };
        cam.walk.phys.position = Vect3::of(1000.0, 50.0, 2000.0);
        let pivot = Vect3::of(1000.0, 51.68, 2000.0);
        // behind the player (-z), a little above
        let back = |d: f32| {
            let (s, c) = (0.26f32.sin(), 0.26f32.cos());
            Vect3::of(1000.0, 51.68 + d * s, 2000.0 - d * c)
        };
        let centre = Vect3::of(1000.0, 51.0, 2000.0);
        let ahead = Vect3::of(1000.0, 51.0, 2060.0);
        for d in [11.0, 50.0] {
            let e = eye(&cam, &back(d));
            assert!((e.distance(&pivot).0 - 8.0).abs() < 1e-3, "{d}: drawn in to 8");
            let own = Vect3::new(centre.x - e.x, centre.y - e.y, centre.z - e.z);
            assert_eq!(level(&BODY, &own), Some(0), "{d}: the player's own body, near");
            assert!(within(&ahead, &e, off_dist(&BODY)), "{d}: 60 ahead is drawn");
        }
        let near = back(5.0);
        assert_eq!(eye(&cam, &near).to_array(), near.to_array(), "within the ceiling: the eye");
        cam.camera_mode = 2;
        assert_eq!(eye(&cam, &back(20.0)).to_array(), back(20.0).to_array(), "mode 2: the eye");
        // the control: from the eye itself
        let own = |e: Vect3<Ee>| Vect3::new(centre.x - e.x, centre.y - e.y, centre.z - e.z);
        assert_eq!(level(&BODY, &own(back(11.0))), Some(1));
        assert!(!within(&ahead, &back(50.0), off_dist(&BODY)));
    }

    /// A skeleton's level (`anim_level`) as `CalcLODLevelIndex` counts: -1
    /// nearer than its first level, then one more a level passed (a level's
    /// own distance is past it: `dist² < d²` breaks the count); with Level of
    /// Detail off, -1 at any distance. The control: the puppets' old
    /// `process(0)` is wrong both near and far.
    #[test]
    fn a_skeletons_level_by_distance() {
        let mut h = HSprite::<Ee>::new(0);
        h.lod_dist = [5.0, 15.0, 40.0].map(F::new).to_vec();
        let eye = Vect3::of(100.0, 10.0, 200.0);
        let actor = |x: f32| Matrix44::transform(&Vect3::default(), F::ONE, &Vect3::of(100.0 + x, 10.0, 200.0));
        let cases = [(0.0, -1), (4.9, -1), (5.0, 0), (14.9, 0), (15.0, 1), (39.9, 1), (40.0, 2), (500.0, 2)];
        for (x, want) in cases {
            assert_eq!(anim_level(&h, &actor(x), &eye, true), want, "{x}");
            assert_eq!(anim_level(&h, &actor(x), &eye, false), -1, "{x}: Level of Detail off");
        }
        assert!(cases.iter().any(|&(_, want)| want != 0), "the control: a fixed 0 must fail");
    }

    /// The player's own skeleton in the follow orbit zoomed out to the
    /// client's 11, 25 and 50: measured from `eye` (drawn in to 8 from the
    /// pivot) it keeps the console's level at its farthest, 0 (5 to 15 from
    /// the camera), as at 8 itself. The control: from the camera itself it
    /// is 1 at 25 and 2 at 50, where 14 and 22 of a human's 40 nodes stop
    /// moving.
    #[test]
    fn zoomed_out_the_players_skeleton_keeps_its_level() {
        let mut h = HSprite::<Ee>::new(0);
        h.lod_dist = [5.0, 15.0, 40.0].map(F::new).to_vec();
        let mut cam = WalkCamera::<Ee> { camera_mode: 3, target_height: F::new(1.68), ..Default::default() };
        cam.walk.phys.position = Vect3::of(1000.0, 50.0, 2000.0);
        let actor = Matrix44::transform(&Vect3::default(), F::ONE, &cam.walk.phys.position);
        let back = |d: f32| {
            let (s, c) = (0.26f32.sin(), 0.26f32.cos());
            Vect3::of(1000.0, 51.68 + d * s, 2000.0 - d * c)
        };
        for d in [8.0, 11.0, 25.0, 50.0] {
            assert_eq!(anim_level(&h, &actor, &eye(&cam, &back(d)), true), 0, "{d}");
        }
        assert_eq!(anim_level(&h, &actor, &back(25.0), true), 1, "the control: from the camera at 25");
        assert_eq!(anim_level(&h, &actor, &back(50.0), true), 2, "the control: from the camera at 50");
    }

    /// One plate size from the zoom (`plate_scale_at`): the game's within the
    /// game's ceiling, then smaller in proportion to the floor at Camera
    /// Distance's 50, never under the font's texels one to a pixel (a
    /// 448-line window keeps the game's size, a 720-line one goes no lower
    /// than 448/720); the other camera modes keep the game's. The control:
    /// a plate sized as the camera's distance (8/d) would be under the
    /// floor at 50.
    #[test]
    fn one_plate_size_from_the_zoom() {
        let k = 1080.0 / 448.0;
        assert_eq!((plate_scale_at(2.0, k), plate_scale_at(GAME_CEILING, k)), (1.0, 1.0), "the game's camera");
        assert!((plate_scale_at(50.0, k) - PLATE_FLOOR).abs() < 1e-6, "the floor at the farthest");
        assert!((plate_scale_at(29.0, k) - 0.75).abs() < 1e-6, "halfway, halfway down");
        let all: Vec<f32> = (8..=50).map(|d| plate_scale_at(d as f32, k)).collect();
        assert!(all.windows(2).all(|w| w[1] < w[0]), "smaller all the way out: {all:?}");
        assert_eq!(plate_scale_at(50.0, 1.0), 1.0, "448 lines: the game's at any zoom");
        assert!((plate_scale_at(50.0, 720.0 / 448.0) - 448.0 / 720.0).abs() < 1e-6, "720 lines: one texel a pixel");
        let mut cam = WalkCamera::<Ee> { camera_mode: 3, target_height: F::new(1.68), ..Default::default() };
        cam.walk.phys.position = Vect3::of(1000.0, 50.0, 2000.0);
        let far = Vect3::of(1000.0, 51.68, 1950.0);
        assert!((plate_scale(&cam, &far, k) - PLATE_FLOOR).abs() < 1e-3, "the orbit at 50");
        cam.camera_mode = 2;
        assert_eq!(plate_scale(&cam, &far, k), 1.0, "mode 2: the game's");
        assert!(GAME_CEILING / 50.0 < plate_scale_at(50.0, k), "the control must fail");
    }

    #[test]
    fn off_distance() {
        assert_eq!(off_dist(&BODY), 100.0);
        assert_eq!(off_dist(&[]), OFF_DIST);
        let eye = Vect3::of(25000.0, 60.0, 16000.0);
        assert!(within(&Vect3::of(25099.0, 60.0, 16000.0), &eye, 100.0));
        assert!(!within(&Vect3::of(25100.0, 60.0, 16000.0), &eye, 100.0), "at the distance is out: d² < off²");
        assert!(!within(&Vect3::of(25000.0, 60.0, 16101.0), &eye, 100.0));
        assert!(within(&Vect3::of(25000.0, 60.0, 16101.0), &eye, OFF_DIST));
    }
}
