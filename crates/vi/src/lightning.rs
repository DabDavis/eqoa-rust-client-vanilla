//! `VILightning`, `VIBolt` and `VIBoltHarmonic` (decomp `vilightning.cpp`): a
//! bolt between two points, three harmonics of interpolated noise summed along
//! its seventeen points, up to three branches off it, and a three-frame
//! after-image ring. `render` hands out what the game hands `VIRaster`: per
//! bolt per frame of the ring, a colour and eight tri-strips (four offsets,
//! two halves), each vertex a UV and a point.
//!
//! The material is `lightning.tga`, additive, Z tested, no Z write
//! (`VILightningFxInit`).

use crate::csprite::Rand;
use crate::fpu::{F, Model};
use crate::geom::Matrix44;
use crate::particle::Color;
use crate::vect::Vect3;

/// One harmonic: two sets of 17 samples and a phase between them.
#[derive(Clone, Debug)]
pub struct Harmonic<M: Model> {
    samples: [[Vect3<M>; 17]; 2],
    phase: F<M>,
    rate: F<M>,
    amplitude: F<M>,
}

impl<M: Model> Harmonic<M> {
    fn new(amplitude: f32, rate: f32, rand: &mut Rand) -> Self {
        let mut h = Harmonic { samples: [[Vect3::default(); 17]; 2], phase: F::ZERO, rate: F::new(rate), amplitude: F::new(amplitude) };
        h.generate(0, rand);
        h.generate(1, rand);
        h
    }

    fn generate(&mut self, which: usize, rand: &mut Rand) {
        let a = self.amplitude;
        for i in 0..17 {
            let x = F::<M>::from_int(((rand.rand() / 131) & 255) - 127) / F::new(128.0) * a;
            let y = F::<M>::from_int(((rand.rand() / 131) & 255) - 127) / F::new(128.0) * a;
            self.samples[which][i] = Vect3::new(x, y, F::ZERO);
        }
    }

    fn sample(&self, i: usize) -> Vect3<M> {
        let t = (self.phase * F::new(3.1415927) + F::new(1.5707964)).sinf() * F::new(0.5) + F::new(0.5);
        let (s0, s1) = (self.samples[0][i], self.samples[1][i]);
        let k = F::ONE - t;
        let d = Vect3::new(s1.x - s0.x, s1.y - s0.y, s1.z - s0.z);
        Vect3::new(d.x * k + s0.x, d.y * k + s0.y, d.z * k + s0.z)
    }

    fn update(&mut self, dt: i32, rand: &mut Rand) {
        let t = F::<M>::from_int(dt) * F::new(0.001);
        if F::ONE <= self.phase {
            self.phase = F::ZERO;
            self.samples[0] = self.samples[1];
            self.generate(1, rand);
            return;
        }
        self.phase = self.phase + t * self.rate;
        self.phase = if F::ONE < self.phase { F::ONE } else { self.phase };
    }
}

/// `VIBolt`
#[derive(Clone, Debug)]
pub struct Bolt<M: Model> {
    harmonics: [Harmonic<M>; 3],
    pub points: [Vect3<M>; 17],
}

impl<M: Model> Bolt<M> {
    fn new(rand: &mut Rand) -> Self {
        let harmonics = Self::harmonics(rand);
        Bolt { harmonics, points: [Vect3::default(); 17] }
    }

    fn harmonics(rand: &mut Rand) -> [Harmonic<M>; 3] {
        let a = Harmonic::new(0.57, 0.64, rand);
        let b = Harmonic::new(0.15, 10.24, rand);
        let c = Harmonic::new(0.08, 44.0, rand);
        [a, b, c]
    }

    fn reset(&mut self, rand: &mut Rand) {
        self.harmonics = Self::harmonics(rand);
    }

    fn update(&mut self, dt: i32, a: &Vect3<M>, b: &Vect3<M>, m: &Matrix44<M>, rand: &mut Rand) {
        for h in &mut self.harmonics {
            h.update(dt, rand);
        }
        let d = Vect3::new(b.x - a.x, b.y - a.y, b.z - a.z);
        for i in 0..17 {
            let k = F::<M>::from_int(i as i32) * F::new(0.0625);
            self.points[i] = Vect3::new(d.x * k + a.x, d.y * k + a.y, d.z * k + a.z);
        }
        let pass = |this: &mut Self, h: usize, n: usize, step: usize, last_open: bool| {
            for i in 0..n {
                let start = i * step;
                let end = start + step;
                let t = m.mul_point(&this.harmonics[h].sample(i));
                let mut d = this.points[end];
                if !last_open || i < n - 1 {
                    d = Vect3::new(d.x + t.x, d.y + t.y, d.z + t.z);
                }
                let s = this.points[start];
                d = Vect3::new(d.x - s.x, d.y - s.y, d.z - s.z);
                for j in start..=end {
                    let k = F::<M>::from_int((j - start) as i32) / F::from_int((end - start) as i32);
                    this.points[j] = Vect3::new(d.x * k + s.x, d.y * k + s.y, d.z * k + s.z);
                }
            }
        };
        pass(self, 0, 4, 4, true);
        pass(self, 1, 8, 2, false);
        for i in 1..16 {
            let t = m.mul_point(&self.harmonics[2].sample(i));
            let p = &mut self.points[i];
            *p = Vect3::new(p.x + t.x, p.y + t.y, p.z + t.z);
        }
    }
}

/// One tri-strip vertex: (u, v) and the point.
pub type StripVertex<M> = ([F<M>; 2], Vect3<M>);

/// What `Render` draws: a colour, then the strips in it.
#[derive(Clone, Debug)]
pub struct BoltDraw<M: Model> {
    pub color: Color<M>,
    pub strips: Vec<Vec<StripVertex<M>>>,
}

/// `VILightning`
#[derive(Clone, Debug)]
pub struct Lightning<M: Model> {
    start: Vect3<M>,
    end: Vect3<M>,
    matrix: Matrix44<M>,
    strike_timer: i32,
    bolt_count: i32,
    bolts: Vec<Bolt<M>>,
    color: Color<M>,
    max_branch_count: i32,
    last_charge: F<M>,
    charge: F<M>,
    ring_timer: i32,
    ring_head: i32,
    ring_count: i32,
    branches: [(i32, [[Vect3<M>; 17]; 4]); 3],
}

impl<M: Model> Lightning<M> {
    /// The constructor: its four bolts each draw their harmonics' samples.
    pub fn new(rand: &mut Rand) -> Self {
        let bolts = (0..4).map(|_| Bolt::new(rand)).collect();
        let o = F::ONE;
        Lightning {
            start: Vect3::default(),
            end: Vect3::default(),
            matrix: Matrix44 { m: [Default::default(); 4] },
            strike_timer: 0,
            bolt_count: 0,
            bolts,
            color: Color { r: o, g: o, b: o, a: o },
            max_branch_count: 0,
            last_charge: F::ZERO,
            charge: F::ZERO,
            ring_timer: 0,
            ring_head: -1,
            ring_count: 0,
            branches: [(0, [[Vect3::default(); 17]; 4]); 3],
        }
    }

    pub fn set_location(&mut self, a: &Vect3<M>, b: &Vect3<M>) {
        self.start = *a;
        self.end = *b;
        let mut up = Vect3::of(0.0, 1.0, 0.0);
        let mut d = Vect3::new(b.x - a.x, b.y - a.y, b.z - a.z);
        let cross = |a: &Vect3<M>, b: &Vect3<M>| Vect3::new(a.y * b.z - a.z * b.y, -a.x * b.z + a.z * b.x, a.x * b.y - a.y * b.x);
        let mut right = cross(&up, &d);
        up = cross(&d, &right);
        right.normalize();
        up.normalize();
        d.normalize();
        let z = F::ZERO;
        self.matrix.m[0] = crate::geom::Vect4::new(right.x, right.y, right.z, z);
        self.matrix.m[1] = crate::geom::Vect4::new(up.x, up.y, up.z, z);
        self.matrix.m[2] = crate::geom::Vect4::new(d.x, d.y, d.z, z);
        self.matrix.m[3] = crate::geom::Vect4::new(z, z, z, F::ONE);
    }

    pub fn charge(&self) -> F<M> {
        self.charge
    }
    pub fn set_charge(&mut self, c: F<M>) {
        self.charge = c;
    }
    pub fn set_color(&mut self, c: Color<M>) {
        self.color = c;
    }
    pub fn set_max_branch_count(&mut self, n: i32) {
        self.max_branch_count = n.min(3);
    }

    pub fn update(&mut self, dt: i32, rand: &mut Rand) {
        self.strike_timer = self.strike_timer.wrapping_sub(dt);
        self.ring_timer = self.ring_timer.wrapping_sub(dt).max(0);
        if self.strike_timer <= 0 {
            for i in 1..4 {
                if (i as i32) < self.bolt_count {
                    self.bolts[i].reset(rand);
                }
            }
            let r = rand.rand() % 351;
            self.strike_timer = r.max(70);
            self.bolt_count = rand.rand() % (self.max_branch_count + 1) + 1;
            if self.bolt_count == 1 {
                self.strike_timer >>= 2;
            }
        }
        for i in 0..self.bolt_count.max(0) as usize {
            if i == 0 {
                let (s, e, m) = (self.start, self.end, self.matrix);
                self.bolts[0].update(dt, &s, &e, &m, rand);
            } else {
                let (s, e, m) = (self.bolts[0].points[i * 3], self.end, self.matrix);
                self.bolts[i].update(dt, &s, &e, &m, rand);
            }
        }
        self.last_charge = self.charge;
        self.charge = self.charge - F::<M>::from_int(dt.wrapping_mul(5)) * F::new(0.001);
        self.charge = if F::ZERO > self.charge { F::ZERO } else { self.charge };
    }

    /// `Render`: this frame's bolts into the ring, then the ring's frames,
    /// newest first, each fainter.
    pub fn render(&mut self) -> Vec<BoltDraw<M>> {
        if self.ring_timer == 0 {
            self.ring_head = (self.ring_head + 1) % 3;
            self.ring_timer = 30;
            self.ring_count = (self.ring_count + 1).min(3);
        }
        let h = self.ring_head as usize;
        self.branches[h].0 = self.bolt_count;
        for i in 0..self.bolt_count.max(0) as usize {
            self.branches[h].1[i] = self.bolts[i].points;
        }
        let mut out = Vec::new();
        for i in 0..self.ring_count {
            let b = &self.branches[((self.ring_head - i + 3) % 3) as usize];
            for j in 0..b.0.max(0) as usize {
                out.push(self.bolt_vertices(&b.1[j], 3 - i));
            }
        }
        out
    }

    fn bolt_vertices(&self, pts: &[Vect3<M>; 17], n: i32) -> BoltDraw<M> {
        let f = F::<M>::from_int(n) / F::new(3.0);
        let c = self.color;
        let color = Color { r: c.r * f * self.last_charge, g: c.g * f * self.last_charge, b: c.b * f * self.last_charge, a: F::ONE };
        let mut strips = Vec::new();
        let mut off = Vect3::<M>::default();
        let uv = |u: F<M>, v: f32| [u, F::new(v)];
        for i in 0..4 {
            match i {
                0 => off = Vect3::of(0.0, 0.04, 0.0),
                1 => off = Vect3::of(0.0, -0.04, 0.0),
                2 => off = self.matrix.mul_point(&Vect3::of(0.04, 0.0, 0.0)),
                _ => off = Vect3::new(-off.x, -off.y, -off.z),
            }
            let sub = |p: &Vect3<M>| Vect3::new(p.x - off.x, p.y - off.y, p.z - off.z);
            let add = |p: &Vect3<M>| Vect3::new(p.x + off.x, p.y + off.y, p.z + off.z);
            let mut s = vec![(uv(F::ZERO, 1.0), pts[0]), (uv(F::ZERO, 0.0), pts[0])];
            for (j, p) in pts.iter().enumerate().take(9).skip(1) {
                let u = F::from_int(j as i32);
                s.push((uv(u, 1.0), sub(p)));
                s.push((uv(u, 0.0), add(p)));
            }
            strips.push(s);
            let mut s = vec![(uv(F::ZERO, 1.0), sub(&pts[8])), (uv(F::ZERO, 0.0), add(&pts[8]))];
            for (j, p) in pts.iter().enumerate().skip(8) {
                if j == 16 {
                    s.push((uv(F::new(8.0), 1.0), *p));
                    s.push((uv(F::new(8.0), 0.0), *p));
                } else {
                    let u = F::from_int(j as i32 - 8);
                    s.push((uv(u, 1.0), sub(p)));
                    s.push((uv(u, 0.0), add(p)));
                }
            }
            strips.push(s);
        }
        BoltDraw { color, strips }
    }
}
