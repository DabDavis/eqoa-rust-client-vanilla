//! `VIEffectVolume` and `VIBoid` (decomp `vieffectvolume.cpp`), and
//! `VIEffectVolumeSprite::Raster` (`vieffectvolumesprite.cpp`): a box of
//! weather placed in the world (ESF 0xC300). Its type says what fills it:
//! falling leaves, light shafts, ground fog, a dust storm or a flock of
//! boids, `m_density` how much, `[m_p34, m_p38]` the hours it shows (with a
//! five-minute ramp), `m_p30` how far it is seen (fading over its last
//! quarter), `m_p28` how strong it is.
//!
//! The leaves, shafts, fog and dust are textured quads the raster draws as
//! immediate-mode fans (`BeginTriFan`, six vertices: both faces of the
//! quad); they are recorded here as [`Fan`]s in the volume's own space, with
//! the raster state the game hands `VIRFan` (colour, normal) at each. The
//! flock's boids are copies of a sprite (a `VICSprite`) steered by Reynolds'
//! rules; they are recorded as the sprite and model-view each is rastered at.
//! The volume's state runs on the wall clock (`VITimer::GetTime`, `now`
//! here), at most 60 steps a second and 0.1 s a step, whatever the frame.

use crate::csprite::Rand;
use crate::fpu::{F, Model, f};
use crate::geom::{BBox, Matrix44, Vect4};
use crate::particle::{Color, rotate_hpr};
use crate::vect::Vect3;

pub const NONE: i32 = 0;
pub const LEAVES: i32 = 1;
pub const SHAFTS: i32 = 2;
pub const FOG: i32 = 3;
pub const DUST: i32 = 4;
pub const FLOCK: i32 = 5;

/// `VIBOIDRAND`: `((float)rand() - (float)rand()) * 2^-31`.
fn boidrand<M: Model>(rand: &mut Rand) -> F<M> {
    let a = F::<M>::from_int(rand.rand());
    let b = F::<M>::from_int(rand.rand());
    (a - b) * f(4.65661287e-10)
}

/// `(float)((rand() * 131) & 0xFF) / 255.0f`
fn rand255<M: Model>(rand: &mut Rand) -> F<M> {
    F::from_int(rand.rand().wrapping_mul(131) & 0xFF) / f(255.0)
}

fn rot<M: Model>(axis: usize, a: F<M>) -> Matrix44<M> {
    let (s, c) = (a.sinf(), a.cosf());
    let mut m = Matrix44::identity();
    match axis {
        // RotateX
        0 => {
            m.m[1].y = c;
            m.m[1].z = s;
            m.m[2].y = -s;
            m.m[2].z = c;
        }
        // RotateY
        1 => {
            m.m[0].x = c;
            m.m[0].z = -s;
            m.m[2].x = s;
            m.m[2].z = c;
        }
        // RotateZ
        _ => {
            m.m[0].x = c;
            m.m[0].y = s;
            m.m[1].x = -s;
            m.m[1].y = c;
        }
    }
    m
}

fn transpose<M: Model>(a: &Matrix44<M>) -> Matrix44<M> {
    let g = |r: &Vect4<M>, i: usize| [r.x, r.y, r.z, r.w][i];
    let col = |i: usize| Vect4::new(g(&a.m[0], i), g(&a.m[1], i), g(&a.m[2], i), g(&a.m[3], i));
    Matrix44 { m: [col(0), col(1), col(2), col(3)] }
}

/// A transposed rigid transform with its translation rebuilt by hand, as
/// `Render` and `VIEffectVolumeSprite::Raster` both write it: the inverse.
fn rebuild<M: Model>(a: &Matrix44<M>) -> Matrix44<M> {
    let mut b = transpose(a);
    let x = b.m[0].w * b.m[0].x + b.m[1].w * b.m[1].x + b.m[2].w * b.m[2].x;
    let y = b.m[0].w * b.m[0].y + b.m[1].w * b.m[1].y + b.m[2].w * b.m[2].y;
    let z = b.m[0].w * b.m[0].z + b.m[1].w * b.m[1].z + b.m[2].w * b.m[2].z;
    b.m[0].w = F::ZERO;
    b.m[1].w = F::ZERO;
    b.m[2].w = F::ZERO;
    b.m[3].x = -x;
    b.m[3].y = -y;
    b.m[3].z = -z;
    b
}

fn sub<M: Model>(a: &Vect3<M>, b: &Vect3<M>) -> Vect3<M> {
    Vect3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

fn scale<M: Model>(v: &Vect3<M>, k: F<M>) -> Vect3<M> {
    Vect3::new(v.x * k, v.y * k, v.z * k)
}

/// `GlobalInit`'s cards: a falling leaf's two swing poses (30 steps each),
/// the fog's eight rolls and the dust's sixty. Four corners a card.
#[derive(Clone, Debug)]
pub struct Cards<M: Model> {
    pub fall: [[[Vect3<M>; 4]; 30]; 2],
    pub fog: [[Vect3<M>; 4]; 8],
    pub dust: [[Vect3<M>; 4]; 60],
    /// What lies past `sDustCard`: a mote born on frame 60 (`rand() & 0x3C`
    /// reaches 60) is drawn once with `sDustCard[60]`, out of bounds. Zero
    /// here; what the console has there is not known.
    pub past: [Vect3<M>; 4],
}

impl<M: Model> Default for Cards<M> {
    fn default() -> Self {
        Self::new()
    }
}

impl<M: Model> Cards<M> {
    pub fn new() -> Self {
        let z = Vect3::default();
        let mut c = Cards { fall: [[[z; 4]; 30]; 2], fog: [[z; 4]; 8], dust: [[z; 4]; 60], past: [z; 4] };
        let square = || [Vect3::of(-0.5, 0.5, 0.0), Vect3::of(0.5, 0.5, 0.0), Vect3::of(0.5, -0.5, 0.0), Vect3::of(-0.5, -0.5, 0.0)];
        let pi = f::<M>(3.1415927);
        for i in 0..8 {
            let m = rot(2, F::<M>::from_int(i) / f(8.0) * f(2.0) * pi);
            c.fog[i as usize] = square().map(|q| m.mul_point(&q));
        }
        for i in 0..60 {
            let m = rot(2, F::<M>::from_int(-i) / f(60.0) * f(2.0) * pi);
            c.dust[i as usize] = square().map(|q| m.mul_point(&q));
        }
        let card = |h: F<M>| {
            let (a, b) = (f::<M>(0.1), -h);
            [Vect3::new(-a, b, -a), Vect3::new(a, b, -a), Vect3::new(a, b, a), Vect3::new(-a, b, a)]
        };
        for i in 0..30 {
            let mut h = f::<M>(0.5);
            let mut v = card(h);
            let a = F::<M>::from_int(i) / f(30.0) * f(2.0) * pi;
            let mut m = rot(1, a);
            for p in v.iter_mut() {
                *p = m.mul_point(p);
            }
            m = rot(0, a.sinf() * f(0.4));
            for p in v.iter_mut() {
                *p = m.mul_point(p);
            }
            m = rot(2, -a.cosf() * f(0.4));
            c.fall[0][i as usize] = v.map(|p| {
                let mut q = m.mul_point(&p);
                q.y = q.y + h;
                q
            });
            h = f(0.4);
            v = card(h);
            m = rot(0, a.sinf() * h);
            for p in v.iter_mut() {
                *p = m.mul_point(p);
            }
            m = rot(1, a.sinf() * f(0.23));
            c.fall[1][i as usize] = v.map(|p| {
                let mut q = m.mul_point(&p);
                q.y = q.y + h;
                q
            });
        }
        c
    }
}

/// A falling leaf: dead (0), falling (1) or settling (2).
#[derive(Clone, Copy, Debug, Default)]
pub struct Leaf<M: Model> {
    pub state: i32,
    pub side: i32,
    pub wobble: F<M>,
    pub pos: Vect3<M>,
    pub floor: F<M>,
    pub scale: F<M>,
    pub life: F<M>,
}

/// A light shaft: dead (0) or shining (1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Shaft<M: Model> {
    pub state: i32,
    pub pos: Vect3<M>,
    pub width: F<M>,
    pub height: F<M>,
    pub age: F<M>,
    pub life: F<M>,
}

/// A puff of ground fog: dead (0) or lingering (1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Puff<M: Model> {
    pub state: i32,
    pub pos: Vect3<M>,
    pub card: i32,
    pub size: F<M>,
    pub height: F<M>,
    pub age: F<M>,
    pub life: F<M>,
}

/// A mote of dust: dead (0) or blowing (1).
#[derive(Clone, Copy, Debug, Default)]
pub struct Mote<M: Model> {
    pub state: i32,
    pub pos: Vect3<M>,
    pub frame: i32,
    pub base: i32,
    pub size_x: F<M>,
    pub size_y: F<M>,
    pub age: F<M>,
    pub rate: F<M>,
    pub life: F<M>,
    pub inv_life: F<M>,
    pub end: Vect3<M>,
}

/// `VIBoid`.
#[derive(Clone, Copy, Debug)]
pub struct Boid<M: Model> {
    pub sprite: i32,
    pub hpr: Vect3<M>,
    /// turn rate, damped by 0.94 a step
    pub p10: F<M>,
    pub dir: Vect3<M>,
    pub pos: Vect3<M>,
    pub sep: Vect3<M>,
    pub align: Vect3<M>,
    pub coh: Vect3<M>,
    pub goal: Vect3<M>,
    pub count: i32,
    pub xform: Matrix44<M>,
    /// the locomotion last set (0 or 1), -1 none
    pub pa0: i32,
    /// turn and climb gains
    pub pa4: F<M>,
    pub pa8: F<M>,
}

impl<M: Model> Boid<M> {
    /// `Init`. `Set(x, y, z)`'s three draws are evaluated last argument
    /// first, as the oracle's compiler does (C leaves the order open).
    pub fn new(rand: &mut Rand) -> Self {
        let r = |rand: &mut Rand, k: f32| boidrand::<M>(rand) * f(k);
        let (hz, hy, hx) = (r(rand, 0.2), r(rand, 0.2), r(rand, 0.2));
        let hpr = Vect3::new(hx, hy, hz);
        let (pz, py, px) = (r(rand, 3.0), r(rand, 3.0), r(rand, 3.0));
        let pos = Vect3::new(px, py, pz);
        let pa4 = r(rand, 0.3) + F::ONE;
        let pa8 = r(rand, 0.3) + F::ONE;
        let z = Vect3::default();
        Boid { sprite: -1, hpr, p10: F::ZERO, dir: Vect3::of(0.0, 0.0, 1.0), pos, sep: z, align: z, coh: z, goal: z, count: 0, xform: Matrix44::identity(), pa0: -1, pa4, pa8 }
    }

    /// `Apply`: the transform from the angles and the position, and the
    /// heading off its third row.
    pub fn apply(&mut self) {
        self.xform = rotate_hpr(&self.hpr);
        self.xform.m[3].x = self.pos.x;
        self.xform.m[3].y = self.pos.y;
        self.xform.m[3].z = self.pos.z;
        self.dir = Vect3::new(self.xform.m[2].x, self.xform.m[2].y, self.xform.m[2].z);
    }
}

#[derive(Clone, Debug, Default)]
pub enum Particles<M: Model> {
    #[default]
    None,
    Leaves(Vec<Leaf<M>>),
    Shafts(Vec<Shaft<M>>),
    Fog(Vec<Puff<M>>),
    Dust(Vec<Mote<M>>),
    Flock(Vec<Boid<M>>),
}

/// What the volume asks of the scene and the raster.
pub trait Host<M: Model> {
    /// `AttachSurface`'s material: one textured layer of `surface`, blended
    /// as `blend`, Z tested, not written. Its id.
    fn create_material(&mut self, surface: i32, blend: i32) -> i32;
    fn release_material(&mut self, m: i32);
    /// `VIScene::CopySprite(i, 0)`, a boid's own copy of the flock's sprite.
    fn copy_sprite(&mut self, sprite: i32) -> i32;
    fn release_sprite(&mut self, sprite: i32);
    /// `VICSprite::SetLocomotion(anim)` on a boid's sprite.
    fn set_locomotion(&mut self, sprite: i32, anim: i32);
    /// `VIScene::m_p1FE0`, the hour.
    fn hour(&self) -> F<M>;
}

/// One immediate-mode fan: the material set (-1: the raster's solid-fill
/// UI material), the colour and normal in the raster at the time, and the
/// six (uv, vertex) pairs, in the volume's space.
#[derive(Clone, Copy, Debug)]
pub struct Fan<M: Model> {
    pub material: i32,
    pub color: Color<M>,
    pub normal: Vect3<M>,
    pub verts: [([F<M>; 2], Vect3<M>); 6],
}

/// What a volume put on the raster: its fans and its boids' sprites (with
/// the model-view each is rastered at). `normal` is the raster's normal,
/// which a fan inherits unless the volume sets it.
#[derive(Clone, Debug, Default)]
pub struct Draw<M: Model> {
    pub fans: Vec<Fan<M>>,
    pub sprites: Vec<(i32, Matrix44<M>)>,
    pub normal: Vect3<M>,
}

impl<M: Model> Draw<M> {
    /// The six-vertex fan every renderer here draws: v0 v1 v2 v3 v2 v1.
    fn quad(&mut self, material: i32, color: Color<M>, v: &[Vect3<M>; 4]) {
        let uv = |u: f32, w: f32| [f(u), f(w)];
        self.fans.push(Fan {
            material,
            color,
            normal: self.normal,
            verts: [(uv(0.0, 0.0), v[0]), (uv(1.0, 0.0), v[1]), (uv(1.0, 1.0), v[2]), (uv(0.0, 1.0), v[3]), (uv(1.0, 1.0), v[2]), (uv(1.0, 0.0), v[1])],
        });
    }
}

/// The camera, as the volume reads it.
#[derive(Clone, Copy, Debug)]
pub struct Camera<M: Model> {
    pub position: Vect3<M>,
    pub dir: Vect3<M>,
    pub view: Matrix44<M>,
    /// `m_world`, the camera's own transform (view's inverse)
    pub world: Matrix44<M>,
}

/// `VIEffectVolume`.
#[derive(Clone, Debug)]
pub struct Volume<M: Model> {
    pub time: i64,
    pub bbox: BBox<M>,
    pub reseed: bool,
    pub ty: i32,
    /// strength
    pub p28: F<M>,
    pub density: F<M>,
    /// seen out to
    pub p30: F<M>,
    /// the hours it shows, from and to
    pub p34: F<M>,
    pub p38: F<M>,
    pub flags: u32,
    pub surface: i32,
    pub sprite: i32,
    pub dirty: bool,
    pub material: i32,
    pub particles: Particles<M>,
    /// 1 inside the hour window, 0 outside (0 until the first render: the
    /// game reads it uninitialised)
    pub day_night: F<M>,
    /// 1 near, 0 past `p30` (likewise)
    pub fade: F<M>,
    /// a flock's waypoint, and seconds to the next
    pub wind: Vect3<M>,
    pub p6c: F<M>,
}

impl<M: Model> Default for Volume<M> {
    /// The constructor.
    fn default() -> Self {
        Volume {
            time: 0,
            bbox: BBox::default(),
            reseed: true,
            ty: NONE,
            p28: F::ONE,
            density: F::ONE,
            p30: f(20.0),
            p34: F::ZERO,
            p38: F::ZERO,
            flags: 0,
            surface: -1,
            sprite: -1,
            dirty: true,
            material: -1,
            particles: Particles::None,
            day_night: F::ZERO,
            fade: F::ZERO,
            wind: Vect3::default(),
            p6c: F::ZERO,
        }
    }
}

impl<M: Model> Volume<M> {
    /// `Init(b)`.
    pub fn init(&mut self, b: &BBox<M>, host: &mut dyn Host<M>) {
        self.bbox = *b;
        self.reseed = true;
        self.set_type(NONE, f(-1.0), host);
        self.dirty = true;
        self.p28 = F::ONE;
        self.density = F::ONE;
        self.p30 = f(20.0);
        self.p34 = F::ZERO;
        self.p38 = F::ZERO;
        self.flags = 0;
        self.time = 0;
        self.wind = Vect3::default();
        self.p6c = F::ZERO;
    }

    /// `Free`: the surface and sprite let go, the particles freed.
    pub fn free(&mut self, host: &mut dyn Host<M>) {
        self.detach_surface(host);
        self.sprite = -1;
        self.free_particles(host);
    }

    pub fn set_type(&mut self, t: i32, density: F<M>, host: &mut dyn Host<M>) {
        self.free(host);
        self.ty = t;
        if density >= F::ZERO {
            self.density = density;
        }
        self.dirty = true;
    }

    pub fn on_resize(&mut self, b: &BBox<M>) {
        self.bbox = *b;
        self.reseed = true;
        self.dirty = true;
    }

    pub fn attach_sprite(&mut self, i: i32) -> i32 {
        if i == -1 {
            return -1;
        }
        self.sprite = i;
        0
    }

    /// `AttachSurface`: a material of the surface, blended by the type.
    pub fn attach_surface(&mut self, surf: i32, host: &mut dyn Host<M>) -> i32 {
        if surf == -1 {
            return -1;
        }
        if self.surface != -1 {
            self.detach_surface(host);
        }
        self.surface = surf;
        let blend = match self.ty {
            LEAVES | FOG | DUST => 3,
            SHAFTS => 1,
            // NONE sets 0; a flock keeps Init's
            _ => 0,
        };
        self.material = host.create_material(surf, blend);
        if self.material < 0 {
            self.surface = -1;
            return -1;
        }
        0
    }

    pub fn detach_surface(&mut self, host: &mut dyn Host<M>) {
        if self.material != -1 {
            host.release_material(self.material);
            self.material = -1;
        }
        self.surface = -1;
    }

    fn free_particles(&mut self, host: &mut dyn Host<M>) {
        if let Particles::Flock(b) = &self.particles {
            for x in b {
                if x.sprite != -1 {
                    host.release_sprite(x.sprite);
                }
            }
        }
        self.particles = Particles::None;
    }

    fn size(&self) -> Vect3<M> {
        sub(&self.bbox.max, &self.bbox.min)
    }

    fn alloc_particles(&mut self, rand: &mut Rand, host: &mut dyn Host<M>) {
        let s = self.size();
        self.free_particles(host);
        let count = |c: F<M>| (c.0 as i32).max(1) as usize;
        self.particles = match self.ty {
            LEAVES => Particles::Leaves(vec![Leaf::default(); count(s.x * s.y * f(0.4) * self.density)]),
            SHAFTS => Particles::Shafts(vec![Shaft::default(); count(s.x * f(0.5) * self.density)]),
            FOG => Particles::Fog(vec![Puff::default(); count(s.x * f(20.0) * self.density)]),
            DUST => Particles::Dust(vec![Mote::default(); count(s.x * f(20.0) * self.density)]),
            FLOCK => {
                let n = (self.density * f(8.0)).0 as i32 + 1;
                Particles::Flock((0..n).map(|_| Boid::new(rand)).collect())
            }
            _ => return,
        };
        self.reseed = true;
    }

    /// `Process(dt)` at wall-clock `now` (ms; `dt` is not read).
    pub fn process(&mut self, now: i64, rand: &mut Rand, host: &mut dyn Host<M>) {
        let mut t = F::<M>::from_int(now.wrapping_sub(self.time) as i32) * f(0.001);
        if self.dirty {
            self.alloc_particles(rand, host);
            self.dirty = false;
        }
        if t >= f(0.016666668) {
            self.time = now;
            if t > f(0.1) {
                t = f(0.1);
            }
            match self.ty {
                LEAVES => self.process_leaves(t, rand),
                SHAFTS => self.process_shafts(t, rand),
                FOG => self.process_fog(t, rand),
                DUST => self.process_dust(t, rand),
                FLOCK => self.process_flock(t, rand, host),
                _ => {}
            }
        }
    }

    fn process_leaves(&mut self, dt: F<M>, rand: &mut Rand) {
        if self.day_night == F::ZERO || self.fade == F::ZERO {
            return;
        }
        let s = self.size();
        let reseed = self.reseed;
        let Particles::Leaves(p) = &mut self.particles else { return };
        for l in p.iter_mut() {
            match l.state {
                0 => {
                    l.state = 1;
                    l.pos.x = s.x * f(0.5) * boidrand(rand);
                    l.pos.y = if reseed { s.y * f(0.5) * boidrand(rand) } else { s.y * f(0.5) };
                    l.pos.z = s.z * f(0.5) * boidrand(rand);
                    l.scale = rand255::<M>(rand) * f(0.8) + f(0.5);
                    l.floor = -s.y * f(0.5);
                    l.life = F::ONE;
                    l.wobble = F::from_int(rand.rand() % 30);
                    l.side = rand.rand() & 1;
                }
                1 => {
                    l.pos.y = l.pos.y - l.scale * dt;
                    l.wobble = l.wobble + dt * f(30.0);
                    if f(30.0) <= l.wobble {
                        l.wobble = F::ZERO;
                    }
                    if l.pos.y < l.floor {
                        l.state = 2;
                    }
                }
                2 => {
                    if F::ONE <= l.wobble {
                        l.wobble = l.wobble + dt * f(30.0);
                        if f(30.0) <= l.wobble {
                            l.wobble = F::ZERO;
                        }
                    } else {
                        l.life = l.life - dt * f(0.6);
                    }
                    if l.life < F::ZERO {
                        l.state = 0;
                    }
                }
                _ => {}
            }
        }
        self.reseed = false;
    }

    fn process_shafts(&mut self, dt: F<M>, rand: &mut Rand) {
        if self.day_night == F::ZERO || self.fade == F::ZERO {
            return;
        }
        let s = self.size();
        let p28 = self.p28;
        let Particles::Shafts(p) = &mut self.particles else { return };
        for x in p.iter_mut() {
            match x.state {
                0 => {
                    x.state = 1;
                    x.pos.x = s.x * f(0.5) * boidrand(rand);
                    x.pos.y = s.y * f(0.5);
                    x.pos.z = s.z * f(0.5) * boidrand(rand);
                    x.width = (rand255::<M>(rand) * f(0.4) + f(1.5)) * f(0.5);
                    x.height = s.y * (rand255::<M>(rand) * f(0.2) + f(0.8));
                    x.age = F::ZERO;
                    let r = rand255::<M>(rand);
                    x.life = (F::ONE - p28) * f(60.0) + f(4.0) + r * ((F::ONE - p28) * f(10.0) + f(2.0));
                }
                1 => {
                    x.age = x.age + dt;
                    if x.life <= x.age {
                        x.state = 0;
                    }
                }
                _ => {}
            }
        }
        self.reseed = false;
    }

    fn process_fog(&mut self, dt: F<M>, rand: &mut Rand) {
        if self.day_night == F::ZERO || self.fade == F::ZERO {
            return;
        }
        let s = self.size();
        let reseed = self.reseed;
        let Particles::Fog(p) = &mut self.particles else { return };
        for x in p.iter_mut() {
            match x.state {
                0 => {
                    x.state = 1;
                    x.size = (rand255::<M>(rand) + f(2.0)) * f(2.0);
                    x.height = s.y * (rand255::<M>(rand) * f(0.15) + f(0.85));
                    x.pos.x = s.x * f(0.5) * boidrand(rand);
                    x.pos.y = F::ZERO;
                    x.pos.z = s.z * f(0.5) * boidrand(rand);
                    x.card = rand.rand() & 7;
                    x.life = rand255::<M>(rand) * f(30.0) + f(10.0);
                    x.age = if reseed { x.life * rand255::<M>(rand) } else { F::ZERO };
                }
                1 => {
                    x.age = x.age + dt;
                    if x.life <= x.age {
                        x.state = 0;
                    }
                }
                _ => {}
            }
        }
        self.reseed = false;
    }

    fn process_dust(&mut self, dt: F<M>, rand: &mut Rand) {
        if self.day_night == F::ZERO || self.fade == F::ZERO {
            return;
        }
        let s = self.size();
        let (p28, reseed) = (self.p28, self.reseed);
        let Particles::Dust(p) = &mut self.particles else { return };
        for x in p.iter_mut() {
            match x.state {
                0 => {
                    x.state = 1;
                    x.size_x = (rand255::<M>(rand) + f(2.0)) * f(2.0) * (p28 * f(0.5) + f(0.5));
                    x.size_y = (rand255::<M>(rand) + f(2.0)) * f(2.0) * (p28 * f(0.5) + f(0.5));
                    x.pos.x = -s.x * f(0.5) - s.x * f(0.25) * rand255::<M>(rand);
                    x.pos.y = s.y * f(0.5) * boidrand(rand);
                    x.pos.z = s.z * f(0.5) * boidrand(rand);
                    x.end.x = s.x * f(0.5) + s.x * f(0.25) * rand255::<M>(rand);
                    x.end.y = x.pos.y;
                    x.end.z = x.pos.z;
                    x.frame = rand.rand() & 0x3C;
                    x.base = x.frame;
                    x.rate = rand255::<M>(rand) * f(0.5) + f(0.5);
                    x.life = (rand255::<M>(rand) * f(0.7) + F::ONE) * (F::ONE - p28 + F::ONE);
                    x.inv_life = F::ONE / x.life;
                    x.age = if reseed { x.life * rand255::<M>(rand) } else { F::ZERO };
                }
                1 => {
                    x.age = x.age + dt;
                    x.frame = ((x.age * x.inv_life * f(60.0) * x.rate).0 as i32 + x.base) % 60;
                    if x.life <= x.age {
                        x.state = 0;
                    }
                }
                _ => {}
            }
        }
        self.reseed = false;
    }

    fn process_flock(&mut self, dt: F<M>, rand: &mut Rand, host: &mut dyn Host<M>) {
        if self.day_night == F::ZERO || self.fade == F::ZERO {
            return;
        }
        let sprite = self.sprite;
        let s = self.size();
        let Particles::Flock(b) = &mut self.particles else { return };
        if sprite != -1 && b[0].sprite == -1 {
            for x in b.iter_mut() {
                x.sprite = host.copy_sprite(sprite);
            }
        }
        if self.p6c <= F::ZERO {
            self.p6c = F::<M>::from_int(rand.rand() / 131 % 9) + f(3.0);
            self.wind.x = s.x * f(0.5) * boidrand(rand);
            self.wind.y = s.y * f(0.5) * boidrand(rand);
            self.wind.z = s.z * f(0.5) * boidrand(rand);
        }
        let n = b.len();
        for x in b.iter_mut() {
            x.sep.zero();
            x.align.zero();
            x.coh.zero();
            x.count = 0;
            x.goal.zero();
        }
        for i in 0..n {
            for j in i + 1..n {
                let d = sub(&b[i].pos, &b[j].pos);
                let dist = d.magnitude();
                if dist < f(2.0) && F::ZERO < dist {
                    let inv = F::ONE / dist;
                    let t = scale(&d, F::ONE / (dist * dist));
                    b[i].sep += t;
                    b[j].sep -= t;
                    let mut t = b[j].dir;
                    t *= inv;
                    b[i].align += t;
                    let mut t = b[i].dir;
                    t *= inv;
                    b[j].align += t;
                    let pj = b[j].pos;
                    b[i].coh += pj;
                    let pi = b[i].pos;
                    b[j].coh += pi;
                    b[i].count += 1;
                    b[j].count += 1;
                }
            }
            let x = &mut b[i];
            if x.count != 0 {
                x.coh *= F::ONE / F::from_int(x.count);
                let d = sub(&x.coh, &x.pos);
                let mag = d.magnitude();
                if F::ZERO < mag {
                    x.coh = scale(&d, F::ONE / mag);
                } else {
                    x.coh.zero();
                }
                x.align *= F::ONE / F::from_int(x.count);
                let mag = x.align.magnitude();
                if F::ZERO < mag {
                    let inv = F::ONE / mag;
                    x.align *= inv;
                    let d = sub(&x.align, &x.dir);
                    x.align = scale(&d, inv);
                }
            }
            x.goal = sub(&self.wind, &x.pos);
        }
        let pi = f::<M>(3.1415927);
        for x in b.iter_mut() {
            let mut a = Vect3::default();
            a += x.sep;
            a += x.align;
            a += x.coh;
            a += x.goal;
            let mut h = Vect3::new(a.x, F::ZERO, a.z);
            let c = x.dir;
            if F::ZERO < h.magnitude() {
                h.normalize();
            }
            let mut dot = h.x * c.x + h.y * c.y + h.z * c.z;
            let h = Vect3::new(h.y * c.z - h.z * c.y, -h.x * c.z + h.z * c.x, h.x * c.y - h.y * c.x);
            dot = (F::ONE - dot) * f(0.5) * f(0.0067);
            if f(0.05) < h.y {
                x.p10 = x.p10 - dot;
            } else if h.y < f(-0.05) {
                x.p10 = x.p10 + dot;
            }
            x.p10 = x.p10 * f(0.94);
            x.hpr.z = -x.p10 * f(9.0);
            x.hpr.x = x.hpr.x + (x.p10 + x.hpr.y * f(0.01)) * x.pa4 * dt * f(30.0);
            if F::ZERO < a.y {
                x.hpr.y = x.hpr.y - dt * f(0.25) * x.pa8;
            } else {
                x.hpr.y = x.hpr.y + dt * f(0.25) * x.pa8;
            }
            let lo = -pi * f(0.15);
            let hi = pi * f(0.15);
            let m = if lo > x.hpr.y { lo } else { x.hpr.y };
            x.hpr.y = if m < hi { m } else { hi };
            let mut v = x.dir;
            v *= dt * (x.hpr.y * f(2.4) + f(5.0)) * f(0.6);
            x.pos += v;
            if x.hpr.y < F::ZERO {
                if x.pa0 != 0 {
                    x.pa0 = 0;
                    host.set_locomotion(x.sprite, 1);
                }
            } else if x.pa0 != 1 {
                x.pa0 = 1;
                host.set_locomotion(x.sprite, 0);
            }
            x.apply();
        }
        self.p6c = self.p6c - dt;
    }

    /// `Render(cam, m)`: `m` takes the world into the volume's space,
    /// `modelview` (the raster's) the volume's space to the camera. Sets the
    /// distance fade and the hour gain, then draws the type's fans (or the
    /// boids' sprites) into `out`.
    #[allow(clippy::too_many_arguments)]
    pub fn render(&mut self, cam: &Camera<M>, modelview: &Matrix44<M>, m: &Matrix44<M>, cards: &Cards<M>, rand: &mut Rand, host: &mut dyn Host<M>, out: &mut Draw<M>) {
        if self.dirty {
            self.alloc_particles(rand, host);
            self.dirty = false;
        }
        let b = rebuild(&cam.view);
        let c = Matrix44::mul(modelview, &b);
        let mut p = cam.position;
        p -= Vect3::new(c.m[3].x, c.m[3].y, c.m[3].z);
        let dist = p.magnitude();
        let range = self.p30;
        let near = range * f(0.75);
        if dist < near {
            self.fade = F::ONE;
        } else {
            if range < dist {
                self.fade = F::ZERO;
                return;
            }
            self.fade = F::ONE - (dist - near) / (range * f(0.25));
            let v = if F::ZERO > self.fade { F::ZERO } else { self.fade };
            self.fade = if v < F::ONE { v } else { F::ONE };
        }
        let span = (self.p34 - self.p38).abs();
        let (t0, t1) = (self.p34, self.p38);
        let ramp = f::<M>(0.083333336);
        if span <= f(0.25) {
            self.day_night = if t0 < t1 { F::ZERO } else { F::ONE };
        } else if f(23.75) <= span {
            self.day_night = if t0 < t1 { F::ONE } else { F::ZERO };
        } else {
            let mut h = host.hour();
            let mut e = t1;
            if e < t0 {
                if h < e {
                    h = h + f(24.0);
                }
                e = e + f(24.0);
            }
            if t0 <= h && h <= e {
                self.day_night = F::ONE;
            } else {
                if e + ramp < h {
                    h = h - f(24.0);
                } else if h < t0 - ramp {
                    h = h + f(24.0);
                }
                self.day_night = if t0 - ramp < h && h <= t0 {
                    (h - (t0 - ramp)) / ramp
                } else if e <= h && h < e + ramp {
                    (e + ramp - h) / ramp
                } else {
                    F::ZERO
                };
                let v = if F::ZERO > self.day_night { F::ZERO } else { self.day_night };
                self.day_night = if v < F::ONE { v } else { F::ONE };
            }
        }
        match self.ty {
            LEAVES => self.render_leaves(cards, out),
            SHAFTS => self.render_shafts(cam, m, host, out),
            FOG => self.render_fog(cam, m, cards, out),
            DUST => self.render_dust(cam, m, cards, out),
            FLOCK => self.render_flock(modelview, out),
            _ => {}
        }
    }

    /// `PrepareBillboardTransform(a, b, &out)`: `b`'s rotation, its rows
    /// normalised, after `a` into `out`; and the rotation itself.
    fn billboard(a: &Matrix44<M>, b: &Matrix44<M>) -> (Matrix44<M>, Matrix44<M>) {
        let mut m = *b;
        m.m[3].x = F::ZERO;
        m.m[3].y = F::ZERO;
        m.m[3].z = F::ZERO;
        let n = |r: &Vect4<M>| F::ONE / ((r.x * r.x + r.y * r.y) + r.z * r.z).sqrt();
        let s = [n(&m.m[0]), n(&m.m[1]), n(&m.m[2])];
        for (k, r) in m.m.iter_mut().take(3).enumerate() {
            r.x = r.x * s[k];
            r.y = r.y * s[k];
            r.z = r.z * s[k];
        }
        (Matrix44::mul(a, &m), m)
    }

    fn render_fog(&mut self, cam: &Camera<M>, m: &Matrix44<M>, cards: &Cards<M>, out: &mut Draw<M>) {
        if self.day_night == F::ZERO || self.fade == F::ZERO {
            return;
        }
        let Particles::Fog(puffs) = &self.particles else { return };
        let (mut a, _) = Self::billboard(&cam.world, m);
        let k = (F::ONE - self.density) * f(0.5) + f(0.5);
        for p in puffs.iter().filter(|p| p.state != 0) {
            let card = &cards.fog[p.card as usize];
            let fk = (p.age / p.life * f(3.1415927)).sinf() * k;
            let mut c = *card;
            for q in c.iter_mut() {
                q.x = q.x * p.size;
            }
            for q in c.iter_mut() {
                q.y = q.y * p.height;
            }
            a.m[3].x = p.pos.x;
            a.m[3].y = p.pos.y + fk * f(1.5);
            a.m[3].z = p.pos.z;
            let v = c.map(|q| a.mul_point(&q));
            let alpha = self.p28 * fk * self.fade * self.day_night;
            out.quad(self.material, Color { r: F::ONE, g: F::ONE, b: F::ONE, a: alpha }, &v);
        }
    }

    fn render_dust(&mut self, cam: &Camera<M>, m: &Matrix44<M>, cards: &Cards<M>, out: &mut Draw<M>) {
        if self.day_night == F::ZERO || self.fade == F::ZERO {
            return;
        }
        let Particles::Dust(motes) = &self.particles else { return };
        let (mut a, _) = Self::billboard(&cam.world, m);
        let k = (F::ONE - self.density) * f(0.5) + f(0.5);
        for p in motes.iter().filter(|p| p.state != 0) {
            let card = cards.dust.get(p.frame as usize).unwrap_or(&cards.past);
            let fk = (p.age * p.inv_life * f(3.1415927)).sinf() * k;
            let mut d = p.end;
            d -= p.pos;
            d *= p.age * p.inv_life;
            d += p.pos;
            let mut c = *card;
            for q in c.iter_mut() {
                q.x = q.x * p.size_x;
            }
            for q in c.iter_mut() {
                q.y = q.y * p.size_y;
            }
            a.m[3].x = d.x;
            a.m[3].y = d.y;
            a.m[3].z = d.z;
            let v = c.map(|q| a.mul_point(&q));
            let alpha = self.p28 * fk * self.fade * self.day_night;
            out.normal = Vect3::of(0.0, 1.0, 0.0);
            out.quad(self.material, Color { r: F::ZERO, g: F::ZERO, b: F::ZERO, a: alpha }, &v);
        }
    }

    fn render_shafts(&mut self, cam: &Camera<M>, m: &Matrix44<M>, host: &mut dyn Host<M>, out: &mut Draw<M>) {
        if self.day_night == F::ZERO || self.fade == F::ZERO {
            return;
        }
        let mut lean = F::ZERO;
        let h = host.hour();
        if self.flags & 2 != 0 {
            lean = if h < f(9.0) {
                f(-1.0)
            } else if h < f(15.0) {
                (h - f(9.0)) / f(3.0) + f(-1.0)
            } else {
                F::ONE
            };
            self.day_night = if h < f(9.0) {
                F::ZERO
            } else if h < f(10.0) {
                h - f(9.0)
            } else if h < f(14.0) {
                F::ONE
            } else if h < f(15.0) {
                F::ONE - (h - f(14.0))
            } else {
                F::ZERO
            };
            if self.day_night == F::ZERO {
                return;
            }
        }
        let Particles::Shafts(shafts) = &self.particles else { return };
        let dir = cam.dir;
        let q = (dir.z / -dir.x).atanf();
        let b = if F::ZERO < dir.x { rot(1, q - f(1.5707964)) } else { rot(1, q + f(1.5707964)) };
        let (a, c) = Self::billboard(&b, m);
        let t = if self.flags & 2 != 0 { c.mul_point(&Vect3::new(lean, F::ZERO, F::ZERO)) } else { Vect3::default() };
        let alpha = self.fade * self.day_night * ((F::ONE - self.density) * f(0.5) + f(0.5));
        for p in shafts.iter().filter(|p| p.state != 0) {
            let z = F::ZERO;
            let mut v = [
                a.mul_point(&Vect3::new(-p.width, z, z)),
                a.mul_point(&Vect3::new(p.width, z, z)),
                a.mul_point(&Vect3::new(p.width, -p.height, z)),
                a.mul_point(&Vect3::new(-p.width, -p.height, z)),
            ];
            for x in v.iter_mut() {
                *x += p.pos;
            }
            if self.flags & 2 != 0 {
                v[2] += scale(&t, p.height);
                v[3] += scale(&t, p.height);
            }
            let alpha = (p.age / p.life * f(3.1415927)).sinf() * alpha;
            out.quad(self.material, Color { r: F::ONE, g: F::ONE, b: F::ONE, a: alpha }, &v);
        }
    }

    fn render_leaves(&mut self, cards: &Cards<M>, out: &mut Draw<M>) {
        if self.day_night == F::ZERO || self.fade == F::ZERO {
            return;
        }
        let Particles::Leaves(leaves) = &self.particles else { return };
        let n = (self.day_night * F::from_int(leaves.len() as i32)).0 as i32;
        for p in leaves.iter().take(n.max(0) as usize).filter(|p| p.state != 0) {
            let c = &cards.fall[p.side as usize][p.wobble.0 as i32 as usize];
            let mut v = *c;
            for x in v.iter_mut() {
                *x += p.pos;
            }
            out.normal = Vect3::of(0.0, 1.0, 0.0);
            out.quad(self.material, Color { r: F::ZERO, g: F::ZERO, b: F::ZERO, a: p.life * self.fade }, &v);
        }
    }

    fn render_flock(&mut self, modelview: &Matrix44<M>, out: &mut Draw<M>) {
        if self.day_night == F::ZERO || self.fade == F::ZERO {
            return;
        }
        let Particles::Flock(b) = &self.particles else { return };
        for x in b.iter().filter(|x| x.sprite != -1) {
            out.sprites.push((x.sprite, Matrix44::mul(&x.xform, modelview)));
        }
    }
}

/// `VIEffectVolumeSprite`: the volume and the box it was last built for.
#[derive(Clone, Debug, Default)]
pub struct VolumeSprite<M: Model> {
    pub bbox: BBox<M>,
    pub local_bbox: BBox<M>,
    pub volume: Volume<M>,
}

impl<M: Model> VolumeSprite<M> {
    /// `Raster(cam, sc, r, flags)` with bit 1 set, under the raster's
    /// model-view (the volume's space to the camera): re-seeded when the box
    /// has drifted more than a metre, then one `Process` and a `Render` in
    /// the volume's own space.
    #[allow(clippy::too_many_arguments)]
    pub fn raster(&mut self, cam: &Camera<M>, modelview: &Matrix44<M>, now: i64, cards: &Cards<M>, rand: &mut Rand, host: &mut dyn Host<M>, out: &mut Draw<M>) {
        let a = sub(&self.bbox.max, &self.bbox.min);
        let b = sub(&self.local_bbox.max, &self.local_bbox.min);
        let d = [a.x - b.x, a.y - b.y, a.z - b.z];
        if d.iter().any(|&x| x > F::ONE) || d.iter().any(|&x| x < f(-1.0)) {
            self.local_bbox = self.bbox;
            self.volume.on_resize(&self.local_bbox);
        }
        let q = rebuild(&cam.view);
        let w = Matrix44::mul(modelview, &q);
        let q = rebuild(&w);
        self.volume.process(now, rand, host);
        self.volume.render(cam, modelview, &q, cards, rand, host, out);
    }
}
