//! The collision primitives the walk code sweeps: `VICollRay`, `VICollSphere`
//! (decomp `vicollray.cpp`) and `VIEnvRay` (`vienvray.cpp`). Laid out as the
//! EE lays them out (0x58, 0x60 bytes). The scene walk that fills in a hit is
//! not here; see [`crate::scene::Scene`].

use crate::fpu::{F, Model, f};
use crate::vect::Vect3;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CollRay<M: Model> {
    pub start: Vect3<M>,
    pub end: Vect3<M>,
    pub delta: Vect3<M>,
    /// Unit.
    pub dir: Vect3<M>,
    pub length: F<M>,
    pub inv_length: F<M>,
    /// A genuine second copy of `dir`, which `Skitter` steers by.
    pub dir2: Vect3<M>,
    pub hit_time: F<M>,
    /// -1: no hit.
    pub hit_idx: i32,
    pub hit_normal: Vect3<M>,
}

impl<M: Model> CollRay<M> {
    fn precalc(&mut self) {
        self.delta = self.end;
        self.delta -= self.start;
        self.dir = self.delta;
        let l = self.dir.magnitude();
        self.length = l;
        if l != F::ZERO {
            let inv = F::ONE / l;
            self.inv_length = inv;
            self.dir *= inv;
        } else {
            self.inv_length = F::ZERO;
        }
    }

    pub fn init(&mut self, start: Vect3<M>, end: Vect3<M>) -> i32 {
        self.start = start;
        self.end = end;
        self.precalc();
        self.hit_idx = -1;
        self.hit_time = F::ONE;
        self.hit_normal.zero();
        self.dir2 = self.dir;
        0
    }

    /// Slide along what was hit: project the remaining travel onto the hit
    /// plane (and onto `v`'s perpendicular), nudge off the surface by 0.01,
    /// and restart the ray from the hit point. Returns 1 when there is travel
    /// left to sweep.
    pub fn skitter(&mut self, v: &Vect3<M>) -> i32 {
        let mut hit = 0;
        if self.hit_time < F::ONE {
            let mut t1 = self.hit_normal;
            let d1 = -v.dot(&self.hit_normal);
            t1.add_scaled(v, d1);
            if t1.normalize() != 0 {
                let mut t0 = self.dir2;
                let d2 = -t1.dot(&self.dir2);
                t0.add_scaled(&t1, d2);
                if t0.normalize() != 0 {
                    t0.add_scaled(&t1, f(0.01));
                    if t0.normalize() != 0 {
                        let d3 = -v.dot(&t0);
                        t0.add_scaled(v, d3);
                        let dist = (F::ONE - self.hit_time) * t0.dot(&self.dir2) * self.length;
                        if F::ZERO < dist {
                            let hp = self.hit_point();
                            hit = 1;
                            self.start = hp;
                            self.end = t0;
                            self.end *= dist;
                            self.end += self.start;
                            self.precalc();
                            self.hit_time = F::ONE;
                            self.hit_idx = -1;
                            self.hit_normal.zero();
                        }
                    }
                }
            }
        }
        hit
    }

    pub fn hit_point(&self) -> Vect3<M> {
        let mut p = self.delta;
        p.x = p.x * self.hit_time + self.start.x;
        p.y = p.y * self.hit_time + self.start.y;
        p.z = p.z * self.hit_time + self.start.z;
        p
    }

    pub fn hit_distance(&self) -> F<M> {
        self.hit_time * self.length
    }
}

/// A ray with a radius. Its `Init`/`Precalc` repeat the ray's, with the radius
/// appended.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CollSphere<M: Model> {
    pub ray: CollRay<M>,
    pub radius: F<M>,
    pub inv_radius: F<M>,
}

impl<M: Model> CollSphere<M> {
    pub fn init(&mut self, start: Vect3<M>, end: Vect3<M>, radius: F<M>) -> i32 {
        let r = &mut self.ray;
        r.start = start;
        r.end = end;
        self.radius = radius;
        r.precalc();
        self.inv_radius = if self.radius == F::ZERO { F::ZERO } else { F::ONE / self.radius };
        r.hit_idx = -1;
        r.hit_time = F::ONE;
        r.hit_normal.zero();
        r.dir2 = r.dir;
        0
    }

    pub fn skitter(&mut self, v: &Vect3<M>) -> i32 {
        self.ray.skitter(v)
    }

    pub fn hit_point(&self) -> Vect3<M> {
        self.ray.hit_point()
    }
}

/// `VITransEnvRay`: a segment asking where a liquid surface is (`mask`: 1
/// water, 2 slime, 4 lava, 8 mud, as `GetEnvFlags` returns them).
#[derive(Clone, Copy, Debug, Default)]
pub struct EnvRay<M: Model> {
    pub start: Vect3<M>,
    pub end: Vect3<M>,
    pub delta: Vect3<M>,
    pub length: F<M>,
    pub inv_length: F<M>,
    pub mask: u32,
    pub hit_time: F<M>,
}

impl<M: Model> EnvRay<M> {
    pub fn init(&mut self, a: Vect3<M>, b: Vect3<M>, mask: u32) -> i32 {
        self.start = a;
        self.end = b;
        self.delta = self.end;
        self.delta -= self.start;
        // NormalizeMagn: the length, and delta left as a unit vector.
        let mut d = (self.delta.x * self.delta.x + self.delta.y * self.delta.y) + self.delta.z * self.delta.z;
        if d != F::ZERO {
            d = d.sqrt();
            let r = F::ONE / d;
            self.delta *= r;
        }
        self.length = d;
        self.inv_length = if self.length != F::ZERO { F::ONE / self.length } else { F::ZERO };
        self.mask = mask;
        self.hit_time = F::ONE;
        0
    }

    pub fn hit_point(&self) -> Vect3<M> {
        let mut p = self.end;
        p -= self.start;
        p *= self.hit_time;
        p += self.start;
        p
    }
}
