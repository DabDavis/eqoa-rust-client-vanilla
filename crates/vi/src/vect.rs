//! `VIVect2` / `VIVect3`, the parts the walk code uses (decomp `vivect.cpp`).
//! The default constructor zeroes (`Default`), as the guest's does.

use crate::fpu::{F, Model, f};
use std::ops::{AddAssign, MulAssign, SubAssign};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vect2<M: Model> {
    pub x: F<M>,
    pub y: F<M>,
}

impl<M: Model> Vect2<M> {
    pub fn new(x: F<M>, y: F<M>) -> Self {
        Vect2 { x, y }
    }

    /// `sqrtf(x*x + y*y)`.
    pub fn magnitude(&self) -> F<M> {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn normalize(&mut self) -> i32 {
        let d = self.x * self.x + self.y * self.y;
        if d != F::ZERO {
            let r = F::ONE / d.sqrt();
            self.x = self.x * r;
            self.y = self.y * r;
            return 1;
        }
        0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vect3<M: Model> {
    pub x: F<M>,
    pub y: F<M>,
    pub z: F<M>,
}

impl<M: Model> Vect3<M> {
    pub fn new(x: F<M>, y: F<M>, z: F<M>) -> Self {
        Vect3 { x, y, z }
    }

    pub fn of(x: f32, y: f32, z: f32) -> Self {
        Vect3 { x: f(x), y: f(y), z: f(z) }
    }

    pub fn set(&mut self, x: F<M>, y: F<M>, z: F<M>) {
        *self = Vect3 { x, y, z };
    }

    /// `Zero()`: three `sw $zero`, so a -0.0 comes back +0.0.
    pub fn zero(&mut self) {
        *self = Self::default();
    }

    /// `sqrtf((x*x + y*y) + z*z)`.
    /// `VIVect3::Distance`
    pub fn distance(&self, v: &Self) -> F<M> {
        let (x, y, z) = (self.x - v.x, self.y - v.y, self.z - v.z);
        ((x * x + y * y) + z * z).sqrt()
    }

    pub fn magnitude(&self) -> F<M> {
        ((self.x * self.x + self.y * self.y) + self.z * self.z).sqrt()
    }

    pub fn normalize(&mut self) -> i32 {
        let d = (self.x * self.x + self.y * self.y) + self.z * self.z;
        if d != F::ZERO {
            let r = F::ONE / d.sqrt();
            self.x = self.x * r;
            self.y = self.y * r;
            self.z = self.z * r;
            return 1;
        }
        0
    }

    /// `VIVect3::HPR(h, p, r)`, the setter: the unit direction (heading,
    /// pitch) points in. Roll is never read; `cosf(p)` is called twice.
    pub fn hpr(&mut self, h: F<M>, p: F<M>, _r: F<M>) {
        self.x = h.sinf() * p.cosf();
        self.y = -p.sinf();
        self.z = h.cosf() * p.cosf();
    }

    /// `VIVect3::HPR(float*, float*, float*) const`, the reader. Roll is
    /// stored as zero.
    pub fn get_hpr(&self) -> (F<M>, F<M>, F<M>) {
        let h = self.x.atan2f(self.z);
        let p = -self.y.atan2f((self.x * self.x + self.z * self.z).sqrt());
        (h, p, F::ZERO)
    }

    /// The inline `Cross(out, a, b)` of `viwalkcontrol.cpp`, term by term (the
    /// operand order decides the sign of a zero).
    pub fn cross(a: &Self, b: &Self) -> Self {
        Vect3 {
            x: a.y * b.z - a.z * b.y,
            y: -a.x * b.z + a.z * b.x,
            z: a.x * b.y - a.y * b.x,
        }
    }

    pub fn dot(&self, b: &Self) -> F<M> {
        self.x * b.x + self.y * b.y + self.z * b.z
    }

    /// The inline `AddVec(accum, dir, k)`: `accum += dir * k`, a component at
    /// a time.
    pub fn add_scaled(&mut self, dir: &Self, k: F<M>) {
        self.x += dir.x * k;
        self.y += dir.y * k;
        self.z += dir.z * k;
    }

    /// `SubVec`, `AddVec`'s mirror.
    pub fn sub_scaled(&mut self, dir: &Self, k: F<M>) {
        self.x -= dir.x * k;
        self.y -= dir.y * k;
        self.z -= dir.z * k;
    }

    pub fn to_array(self) -> [f32; 3] {
        [self.x.0, self.y.0, self.z.0]
    }
}

impl<M: Model> AddAssign for Vect3<M> {
    fn add_assign(&mut self, o: Self) {
        self.x += o.x;
        self.y += o.y;
        self.z += o.z;
    }
}

impl<M: Model> SubAssign for Vect3<M> {
    fn sub_assign(&mut self, o: Self) {
        self.x -= o.x;
        self.y -= o.y;
        self.z -= o.z;
    }
}

impl<M: Model> MulAssign<F<M>> for Vect3<M> {
    fn mul_assign(&mut self, k: F<M>) {
        self.x *= k;
        self.y *= k;
        self.z *= k;
    }
}
