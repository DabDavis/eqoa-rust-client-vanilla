//! The sky's placement: `VIAtmosphere::Update`'s sun, corona and moon
//! matrices and `VIScene::SetSky`'s sky type (decomp `viatmosphere.cpp`,
//! `viscene.cpp`). The sky is drawn with the camera's rotation only
//! (`RenderSky` zeroes the view's translation), so these are relative to the
//! eye. Not ported: transitions between sky types (`ProcessTransitions`
//! fades over 30 s; here the sky is always in its settled state), weather,
//! lightning, stars and the Lavastorm.

use crate::fpu::{F, Model, f};
use crate::geom::Matrix44;
use crate::tod::PROFILES;
use crate::vect::Vect3;
use crate::world::ProfileBlend;

/// The model matrices `RenderSky` puts before the view for the sun, the
/// corona and the moon.
#[derive(Clone, Copy, Debug)]
pub struct SkyRotations<M: Model> {
    pub sun: Matrix44<M>,
    pub corona: Matrix44<M>,
    pub moon: Matrix44<M>,
}

/// `VIAtmosphere::Update` for `hour`: the sun rolls 15° an hour about z at
/// 35 units, overhead at noon; the moon opposite. `sky_scale` (1 under a
/// settled clear sky) sizes the corona.
pub fn sky_rotations<M: Model>(hour: F<M>, sky_scale: F<M>) -> SkyRotations<M> {
    let w = hour * f(0.2617993);
    let z = F::ZERO;
    let sun = Matrix44::transform(&Vect3::new(z, z, w), f(1.0), &Vect3::new(w.sinf() * f(35.0), w.cosf() * f(-35.0), z));
    let corona = Matrix44::mul(&Matrix44::transform(&Vect3::default(), sky_scale, &Vect3::default()), &sun);
    let m = hour * f(0.2617993) + f(3.14159274);
    let moon = Matrix44::transform(&Vect3::new(z, z, m), f(1.0), &Vect3::new(-sun.m[3].x, -sun.m[3].y, z));
    SkyRotations { sun, corona, moon }
}

/// `VIScene::SetSky`: the sky type of the profile with the heaviest weight
/// (the first, on a tie). 0 sunny, 1 the dome's own texture, 2 and up dark
/// clouds (and no sun, moon or corona: `m_overcast` ends 0).
pub fn sky_type<M: Model>(b: &ProfileBlend<M>) -> i32 {
    let mut k = 0;
    for j in 1..4 {
        if b.weight[k] < b.weight[j] {
            k = j;
        }
    }
    PROFILES.get(b.index[k] as usize).map_or(0, |p| p.sky)
}
