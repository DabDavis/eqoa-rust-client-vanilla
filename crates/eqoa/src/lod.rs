//! What of a character the game draws at a distance: `VILODSprite::Raster`'s
//! level of a skinned LOD body (the player races' models: 40 of CHAR.ESF's 568
//! CSprites, the human and the barbarian among them, carry a near mesh to 10
//! and a far one to 100), and `VIScene::RasterActor`'s off distance, past which
//! an actor is not drawn at all (`VICSprite::CalcLODOffDist`: an LOD body's
//! last level, else `VISprite`'s 500). Both on the EE's floats, as the game
//! computes them.

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

/// `VIScene::RasterActor`: whether an actor at `at` is drawn from a camera at
/// `eye`, its sprite's off distance being `off`.
pub fn within(at: &Vect3<Ee>, eye: &Vect3<Ee>, off: f32) -> bool {
    let (dx, dy, dz) = (at.x - eye.x, at.y - eye.y, at.z - eye.z);
    let d2 = dx * dx + dy * dy + dz * dz;
    d2 < F::new(off) * F::new(off)
}

/// A character's body as the camera at `eye` sees it this frame: the level
/// of its LOD body (if it has one) for its centre transform `centre`. Whether
/// it is drawn at all is `within`'s, at the actor's place.
pub fn place(body: &mut render::GpuModel, centre: &Matrix44<Ee>, eye: &Vect3<Ee>) {
    body.level = level(&body.lod_dists, &from_eye(centre, eye));
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
