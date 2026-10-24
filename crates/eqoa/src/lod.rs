//! What of a character the game draws at a distance: `VILODSprite::Raster`'s
//! level of a skinned LOD body (the player races' models: 40 of CHAR.ESF's 568
//! CSprites, the human and the barbarian among them, carry a near mesh to 10
//! and a far one to 100), and `VIScene::RasterActor`'s off distance, past which
//! an actor is not drawn at all (`VICSprite::CalcLODOffDist`: an LOD body's
//! last level, else `VISprite`'s 500). Both on the EE's floats, as the game
//! computes them.
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
//! them at its farthest. The options' Level of Detail off draws every LOD
//! body's near level out to its off distance (`chosen`). The options' NPC/PC
//! View Distance draws every character at least that far (`view_dist`), an
//! LOD body past its last level at that level (the game's 100 changes
//! nothing: no character's off distance is nearer).

use vi::camera::WalkCamera;
use vi::fpu::{Ee, F};
use vi::geom::Matrix44;
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
/// the options' NPC/PC View Distance where that is farther.
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
/// the options' Level of Detail and NPC/PC View Distance. Whether it is
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

/// Where the levels and off distances are measured from: the camera's
/// `eye`, but in the follow orbit (mode 3) no farther from the orbit's pivot
/// than the game's ceiling, along the same line. The pivot is the player's
/// place raised by the target height and the height offset, as
/// `MoveCamera` places the eye. Within the ceiling, and in the other modes
/// (whose ceilings are the game's own), the eye itself.
pub fn eye(cam: &WalkCamera<Ee>, eye: &Vect3<Ee>) -> Vect3<Ee> {
    if cam.camera_mode != 3 {
        return *eye;
    }
    let p = &cam.walk.phys;
    let mut pivot = p.position;
    pivot.y = pivot.y + cam.target_height + p.height_offset;
    let len = eye.distance(&pivot);
    let ceiling = F::new(GAME_CEILING);
    if len <= ceiling {
        return *eye;
    }
    let k = ceiling / len;
    Vect3::new(pivot.x + (eye.x - pivot.x) * k, pivot.y + (eye.y - pivot.y) * k, pivot.z + (eye.z - pivot.z) * k)
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

    /// NPC/PC View Distance: at the game's 100 a body's levels and off
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
