//! Time of day: `VIScene::InitTimeOfDay`'s 24 hourly entries and
//! `SetupTimeOfDayLights`' interpolation (decomp `viscene.cpp`). Each hour has
//! an ambient colour and a sun colour for a CLEAR sky (`[0]`) and an
//! OVERCAST one (`[1]`), and the direction the sun's light travels,
//! (−sin 15h°, cos 15h°, 0): straight down at noon. The overcast weight is
//! the atmosphere's (`GetOvercastInterp`) times the terrain profiles' (see
//! [`overcast_weight`]); a lightning flash whitens the sun ([`TimeOfDay::flash`]).
//! The placed point lights never light anything: `VIRaster::MaxPointLights`
//! is 1, which the personal light takes.

use crate::fpu::{F, Model, f, vimax, vimin};
use crate::world::ProfileBlend;
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
    /// Sky and fog colour, clear (the sun's) and overcast (a grey that peaks
    /// at noon).
    pub d: [[Color<M>; 2]; 24],
}

/// `VITerrainProfile`: fog near and far, a tint, the sky type, the overcast
/// weight, the ambient-music index and the Lavastorm weight.
#[derive(Clone, Copy, Debug)]
pub struct Profile {
    pub near: f32,
    pub far: f32,
    pub color: [f32; 4],
    pub sky: i32,
    pub overcast: f32,
    pub music: i32,
    pub lavastorm: f32,
}

const fn b(r: u8, g: u8, bl: u8) -> [f32; 4] {
    // SetColorFrom: c × (1/255), which `f32` arithmetic here would round
    // differently; `profile_color` redoes it in the model.
    [r as f32, g as f32, bl as f32, -1.0]
}
const U: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

const fn p(near: f32, far: f32, color: [f32; 4], sky: i32, overcast: f32, music: i32, lavastorm: f32) -> Profile {
    Profile { near, far, color, sky, overcast, music, lavastorm }
}

/// `VIWorld::SetProfiles`, generated from the decomp's source. A colour with
/// a negative alpha is bytes (alpha 255), taken over 255 in the model.
pub const PROFILES: [Profile; 22] = [
    p(250.0, 800.0, U, 1, 1.0, 9, 0.0),
    p(0.0, 400.0, b(210, 230, 180), 1, 1.0, 4, 0.0),
    p(250.0, 800.0, b(227, 247, 247), 0, 1.0, 6, 0.0),
    p(250.0, 800.0, b(233, 218, 166), 0, 1.0, 3, 0.0),
    p(150.0, 800.0, U, 2, 0.5, 5, 0.0),
    p(0.0, 400.0, b(90, 140, 65), 0, 1.0, 7, 0.0),
    p(0.0, 350.0, b(140, 170, 120), 2, 0.5, 10, 0.0),
    p(250.0, 800.0, U, 1, 1.0, 2, 0.0),
    p(250.0, 800.0, U, 1, 1.0, 1, 0.0),
    p(0.0, 400.0, b(210, 230, 180), 1, 1.0, 1, 0.0),
    p(250.0, 800.0, b(227, 247, 247), 0, 1.0, 1, 0.0),
    p(250.0, 800.0, b(233, 218, 166), 0, 1.0, 1, 0.0),
    p(150.0, 800.0, U, 2, 0.5, 1, 0.0),
    p(0.0, 400.0, b(90, 140, 65), 0, 1.0, 1, 0.0),
    p(0.0, 350.0, b(140, 170, 120), 2, 0.5, 1, 0.0),
    p(250.0, 800.0, U, 1, 1.0, 8, 0.0),
    p(250.0, 800.0, U, 1, 1.0, 1, 0.0),
    p(0.0, 800.0, b(255, 170, 170), 0, 1.0, 3, 0.0),
    p(0.0, 550.0, b(110, 140, 30), 1, 1.0, 8, 0.0),
    p(0.0, 600.0, b(255, 170, 170), 2, 1.0, 8, 1.0),
    p(250.0, 800.0, b(175, 200, 240), 1, 1.0, 6, 0.0),
    p(0.0, 600.0, b(140, 170, 160), 1, 1.0, 7, 0.0),
];

/// A profile's tint as `SetProfiles` leaves it.
pub fn profile_color<M: Model>(i: i32) -> [F<M>; 4] {
    let c = PROFILES.get(i as usize).map_or(U, |p| p.color);
    if c[3] < 0.0 {
        let k = f(1.0 / 255.0);
        [F::new(c[0]) * k, F::new(c[1]) * k, F::new(c[2]) * k, F::new(255.0) * k]
    } else {
        c.map(F::new)
    }
}

/// The fog `SetupTimeOfDayFog` hands `VIRaster::SetFog`: near, far, colour.
#[derive(Clone, Copy, Debug)]
pub struct Fog<M: Model> {
    pub near: F<M>,
    pub far: F<M>,
    pub color: [F<M>; 4],
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
        let mut t = TimeOfDay { a: [[z; 2]; 24], b: [Vect3::default(); 24], c: [[z; 2]; 24], d: [[z; 2]; 24] };
        for h in 0..24 {
            t.a[h][0] = rgb(AMB[h].0, AMB[h].1, AMB[h].2);
            t.c[h][0] = rgb(SUN[h].0, SUN[h].1, SUN[h].2);
            // Overcast ambient: 90% of clear's red, grey; overcast sun: none.
            let r = t.a[h][0][0] * f(0.9);
            t.a[h][1] = [r, r, r];
            t.c[h][1] = z;
            t.d[h][0] = t.c[h][0];
            // Overcast sky: grey, 1 − |h − 12|/12 at 70%, at least 0.2.
            let k = F::<M>::new((h as i32 - 12) as f32) / f(12.0);
            let g = vimax((f(1.0) - k.abs()) * f(0.7), f(0.2));
            t.d[h][1] = [g, g, g];
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

/// `SetupTimeOfDayLights`' `k`: the atmosphere's overcast `o` times the
/// blend of the profiles' overcast weights.
pub fn overcast_weight<M: Model>(o: F<M>, b: &ProfileBlend<M>) -> F<M> {
    let w = |k: usize| F::<M>::new(PROFILES.get(b.index[k] as usize).unwrap_or(&PROFILES[0]).overcast) * b.weight[k];
    o * (((w(0) + w(1)) + w(2)) + w(3))
}

/// `SetupTimeOfDaySky`'s Lavastorm intensity: the blend of the profiles'.
pub fn lavastorm_weight<M: Model>(b: &ProfileBlend<M>) -> F<M> {
    let w = |k: usize| F::<M>::new(PROFILES.get(b.index[k] as usize).unwrap_or(&PROFILES[0]).lavastorm) * b.weight[k];
    ((w(0) + w(1)) + w(2)) + w(3)
}

impl<M: Model> TimeOfDay<M> {
    /// `SetupTimeOfDayLights` under a lightning flash: the sun whitened by
    /// `flash` (red and green at 90%) times `outdoors` (`m_soundTrack.m_p30`),
    /// and its light from hour 10's direction.
    pub fn flash(&self, sun: &Color<M>, flash: F<M>, outdoors: F<M>) -> (Color<M>, Vect3<M>) {
        let c = [
            sun[0] + (F::ONE - sun[0]) * flash * f(0.9) * outdoors,
            sun[1] + (F::ONE - sun[1]) * flash * f(0.9) * outdoors,
            sun[2] + (F::ONE - sun[2]) * flash * outdoors,
        ];
        (c, self.b[10])
    }

    fn hour_split(hour: F<M>) -> (usize, usize, F<M>) {
        let h = hour.0 as i32 as usize;
        let t = hour - F::new(h as f32);
        (h, if h + 1 == 24 { 0 } else { h + 1 }, t)
    }

    /// The hour's `m_d` blended to overcast weight `o`, times the profile
    /// blend's tint: `SetupTimeOfDaySky`'s sky colour and the plain fog's.
    fn tinted(&self, hour: F<M>, o: F<M>, b: &ProfileBlend<M>) -> [F<M>; 3] {
        let (h, next, t) = Self::hour_split(hour);
        let mut p = [F::ZERO; 3];
        for k in 0..4 {
            let c = profile_color::<M>(b.index[k]);
            for i in 0..3 {
                p[i] = if k == 0 { c[i] * b.weight[0] } else { p[i] + c[i] * b.weight[k] };
            }
        }
        [0, 1, 2].map(|i| {
            let x0 = (self.d[next][0][i] - self.d[h][0][i]) * t + self.d[h][0][i];
            let x1 = (self.d[next][1][i] - self.d[h][1][i]) * t + self.d[h][1][i];
            ((x1 - x0) * o + x0) * p[i]
        })
    }

    /// `SetupTimeOfDaySky`'s colour, as the bytes it sets.
    pub fn sky(&self, hour: F<M>, o: F<M>, b: &ProfileBlend<M>) -> [u8; 3] {
        self.tinted(hour, o, b).map(|c| (c * f(255.0) + f(0.5)).0 as i32 as u8)
    }

    /// `SetupTimeOfDayFog`. `flags` are the camera's env flags, `sun` the sun
    /// colour `SetupTimeOfDayLights` left (`m_p1FE4`), `cap` the streaming
    /// radius the far plane may not pass (`m_p2A58[2]`, 900).
    pub fn fog(&self, hour: F<M>, o: F<M>, b: &ProfileBlend<M>, flags: u32, sun: &Color<M>) -> Fog<M> {
        // CalcTimeOfDayClipPlanes.
        let mut near = F::ZERO;
        let mut far = F::ZERO;
        for k in 0..4 {
            let pr = PROFILES.get(b.index[k] as usize).unwrap_or(&PROFILES[0]);
            if k == 0 {
                near = F::new(pr.near) * b.weight[0];
                far = F::new(pr.far) * b.weight[0];
            } else {
                near = near + F::new(pr.near) * b.weight[k];
                far = far + F::new(pr.far) * b.weight[k];
            }
        }
        // m_p1FE4 is the sun colour with alpha 0, so its floored alpha is 0.4.
        let lit = || sun.map(|c| vimax(c, f(0.4)));
        let a = f(0.4);
        let one = f(1.0);
        let (color, cap) = if flags & 1 != 0 {
            let p = lit();
            ([p[0] * F::ZERO, p[1] * f(0.5), p[2] * one, a * one], Some(f(20.0)))
        } else if flags & 4 != 0 {
            ([one, F::ZERO, F::ZERO, one], Some(f(20.0)))
        } else if flags & 2 != 0 {
            let p = lit();
            ([p[0] * f(0.18), p[1] * f(0.56), p[2] * f(0.313), a], Some(f(20.0)))
        } else if flags & 8 != 0 {
            let p = lit();
            let k = f(1.0 / 255.0);
            let c = [70.0, 65.0, 50.0].map(|v| F::<M>::new(v) * k);
            ([p[0] * c[0], p[1] * c[1], p[2] * c[2], a * (F::new(255.0) * k)], Some(f(5.0)))
        } else {
            let c = self.tinted(hour, o, b);
            ([c[0], c[1], c[2], F::ZERO], None)
        };
        if let Some(cap) = cap {
            near = F::ZERO;
            far = vimin(cap, far);
        }
        Fog { near, far: vimin(f(900.0), far), color }
    }
}

impl<M: Model> Default for TimeOfDay<M> {
    fn default() -> Self {
        Self::new()
    }
}


/// `VIScene`'s personal light (`m_personalLight*`, `m_p2A38`, `m_p2A40`):
/// the light the client hangs 3 above the player (`SetPersonalLight(actor,
/// 20, (1, 0.69, 0.36))`), its radius ramped in `SetupTimeOfDayLights`.
/// A point light as the raster takes it: position, range, colour.
pub type PointLight<M> = (Vect3<M>, F<M>, [F<M>; 4]);

#[derive(Clone, Copy, Debug)]
pub struct PersonalLight<M: Model> {
    /// `VIPersonalLightMode`: 0 by the hour, 1 on, 2 off
    pub mode: i32,
    pub at: Vect3<M>,
    pub max_radius: F<M>,
    pub color: [F<M>; 4],
    last: i64,
    pub radius: F<M>,
}

impl<M: Model> Default for PersonalLight<M> {
    fn default() -> Self {
        let f = F::new;
        PersonalLight { mode: 0, at: Vect3::default(), max_radius: f(20.0), color: [f(1.0), f(0.69), f(0.36), f(1.0)], last: 0, radius: F::ZERO }
    }
}

impl<M: Model> PersonalLight<M> {
    /// `SetupTimeOfDayLights`' part: the radius grows 2 a second (on, night
    /// before 6 or after 16, or rain outdoors under a heavy overcast) or
    /// shrinks, clamped to the maximum; the light while it is above 0.
    /// `rain` is the atmosphere's intensity, `outdoors` the probe's, `o`
    /// `GetOvercastInterp`.
    pub fn step(&mut self, now: i64, hour: F<M>, rain: F<M>, outdoors: F<M>, o: F<M>) -> Option<PointLight<M>> {
        if !(0..2).contains(&self.mode) {
            return None;
        }
        let dt = now - self.last;
        let step = if dt > 0 { F::from_int(dt as i32) * F::new(0.002) } else { F::ZERO };
        let f = F::new;
        if self.mode == 1 || hour < f(6.0) || f(16.0) < hour || (F::ZERO < rain && f(0.05) < outdoors && f(0.5) < o) {
            self.radius = self.radius + step;
        } else {
            self.radius = self.radius - step;
        }
        self.radius = vimin(self.max_radius, vimax(F::ZERO, self.radius));
        self.last = now;
        (F::ZERO < self.radius).then_some((self.at, self.radius, self.color))
    }
}
