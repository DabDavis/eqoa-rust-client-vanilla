//! Time of day: `VIScene::InitTimeOfDay`'s 24 hourly entries and
//! `SetupTimeOfDayLights`' interpolation (decomp `viscene.cpp`). Each hour has
//! an ambient colour and a sun colour for a CLEAR sky (`[0]`) and an
//! OVERCAST one (`[1]`), and the direction the sun's light travels,
//! (−sin 15h°, cos 15h°, 0): straight down at noon. Not ported: the weather
//! that sets the overcast weight, the terrain profiles that scale it, the
//! lightning flash, the personal light and the point lights.

use crate::fpu::{F, Model, f};
use crate::vect::Vect3;

pub type Color<M> = [F<M>; 3];

#[derive(Clone, Debug)]
pub struct TimeOfDay<M: Model> {
    /// Ambient, clear and overcast.
    pub a: [[Color<M>; 2]; 24],
    /// The sun's light direction.
    pub b: [Vect3<M>; 24],
    /// Sun colour, clear and overcast.
    pub c: [[Color<M>; 2]; 24],
}

fn rgb<M: Model>(r: u8, g: u8, b: u8) -> Color<M> {
    // SetColorFrom: c / 255.
    [r, g, b].map(|v| F::new(v as f32) * f(1.0 / 255.0))
}

impl<M: Model> TimeOfDay<M> {
    /// `InitTimeOfDay`.
    pub fn new() -> Self {
        const AMB: [(u8, u8, u8); 24] = [
            (75, 75, 100), (75, 75, 100), (75, 75, 100), (75, 75, 100), (75, 75, 100), (75, 75, 100),
            (85, 85, 85), (85, 85, 85), (95, 95, 95), (100, 100, 100), (100, 100, 100), (102, 102, 102),
            (102, 102, 102), (100, 100, 100), (100, 100, 100), (100, 100, 100), (95, 95, 95), (75, 75, 100),
            (75, 75, 100), (75, 75, 100), (75, 75, 100), (75, 75, 100), (75, 75, 100), (75, 75, 100),
        ];
        const SUN: [(u8, u8, u8); 24] = [
            (70, 0, 190), (80, 0, 220), (85, 0, 240), (90, 0, 255), (120, 55, 240), (170, 55, 240),
            (255, 175, 55), (255, 210, 100), (255, 248, 160), (255, 250, 200), (255, 250, 210), (255, 250, 230),
            (255, 250, 240), (255, 250, 230), (255, 250, 210), (255, 250, 200), (255, 248, 160), (255, 210, 100),
            (255, 175, 55), (170, 55, 240), (120, 55, 240), (90, 0, 255), (85, 0, 240), (80, 0, 220),
        ];
        let z = [F::ZERO; 3];
        let mut t = TimeOfDay { a: [[z; 2]; 24], b: [Vect3::default(); 24], c: [[z; 2]; 24] };
        for h in 0..24 {
            t.a[h][0] = rgb(AMB[h].0, AMB[h].1, AMB[h].2);
            t.c[h][0] = rgb(SUN[h].0, SUN[h].1, SUN[h].2);
            // Overcast ambient: 90% of clear's red, grey; overcast sun: none.
            let r = t.a[h][0][0] * f(0.9);
            t.a[h][1] = [r, r, r];
            t.c[h][1] = z;
            let a = F::<M>::new(h as f32) * f(15.0) * f(0.0174532924);
            t.b[h] = Vect3::new(-a.sinf(), a.cosf(), F::ZERO);
        }
        t
    }

    /// `SetupTimeOfDayLights`' ambient, sun colour and sun direction for
    /// `hour` (0..24) at overcast weight `k` (0 clear).
    pub fn lights(&self, hour: F<M>, k: F<M>) -> (Color<M>, Color<M>, Vect3<M>) {
        let hour = crate::fpu::clamp(hour, F::ZERO, f(23.99999));
        let h = hour.0 as i32 as usize;
        let t = hour - F::new(h as f32);
        let next = if h + 1 == 24 { 0 } else { h + 1 };
        let lerp = |a: &Color<M>, b: &Color<M>, t: F<M>| [0, 1, 2].map(|i| (b[i] - a[i]) * t + a[i]);
        let blend = |tab: &[[Color<M>; 2]; 24]| {
            let x0 = lerp(&tab[h][0], &tab[next][0], t);
            let x1 = lerp(&tab[h][1], &tab[next][1], t);
            lerp(&x0, &x1, k)
        };
        let mut dir = self.b[next];
        dir -= self.b[h];
        dir *= t;
        dir += self.b[h];
        dir.normalize();
        (blend(&self.a), blend(&self.c), dir)
    }
}

impl<M: Model> Default for TimeOfDay<M> {
    fn default() -> Self {
        Self::new()
    }
}

