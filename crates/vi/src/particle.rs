//! The particle system: `VIParticleAttributes`, `VIParticleMotif`,
//! `VIParticleDefinition(Ex)`, `VIParticleEmitter` and `VIParticleSystem`,
//! line by line from the decomp (`viparticle*.cpp`), and the ESF definition
//! (0xC000) as `VIESFParse::ParseParticleDefinition` reads it.
//!
//! A definition is a set of attributes (birth rate, life, speed, size, a
//! 32-step colour gradient, the spread angles and offsets of a nozzle) plus
//! named motifs a spell can blend in. An emitter spawns particles from a
//! shared pool of 2,000 each time it is updated; the system's `render` moves,
//! ages and colours every live one and hands it out as a billboard, as the
//! game hands them to `VIRaster::BeginBillboards`.
//!
//! The random numbers are newlib's `rand()`, two per `VIRAND` (left operand
//! first, as the EE calls them), passed in so a test can seed them.

use crate::csprite::Rand;
use crate::fpu::{F, Model};
use crate::geom::{BBox, Matrix44};
use crate::vect::Vect3;
use std::collections::VecDeque;

/// `VIColor32F`
#[derive(Clone, Copy, Debug, Default)]
pub struct Color<M: Model> {
    pub r: F<M>,
    pub g: F<M>,
    pub b: F<M>,
    pub a: F<M>,
}

/// `VIMatrix33`, rows.
#[derive(Clone, Copy, Debug)]
pub struct Matrix33<M: Model> {
    pub m: [Vect3<M>; 3],
}

impl<M: Model> Matrix33<M> {
    pub fn identity() -> Self {
        let (o, z) = (F::ONE, F::ZERO);
        Matrix33 { m: [Vect3::new(o, z, z), Vect3::new(z, o, z), Vect3::new(z, z, o)] }
    }
}

/// `VIVect3 operator*(const VIVect3 &, const VIMatrix33 &)`: a row vector.
fn vm33<M: Model>(v: &Vect3<M>, m: &Matrix33<M>) -> Vect3<M> {
    let mut r = Vect3::new(m.m[0].x * v.x, m.m[0].y * v.x, m.m[0].z * v.x);
    r.x = r.x + m.m[1].x * v.y;
    r.y = r.y + m.m[1].y * v.y;
    r.z = r.z + m.m[1].z * v.y;
    r.x = r.x + m.m[2].x * v.z;
    r.y = r.y + m.m[2].y * v.z;
    r.z = r.z + m.m[2].z * v.z;
    r
}

/// `VIMatrix44::RotateHPR`
fn rotate_hpr<M: Model>(v: &Vect3<M>) -> Matrix44<M> {
    let (si, ci) = (v.x.sinf(), v.x.cosf());
    let (sj, cj) = (v.y.sinf(), v.y.cosf());
    let (sh, ch) = (v.z.sinf(), v.z.cosf());
    let mut m = Matrix44::identity();
    m.m[0].x = ch * ci + sh * sj * si;
    m.m[0].y = sh * cj;
    m.m[0].z = -ch * si + sh * sj * ci;
    m.m[1].x = -sh * ci + ch * sj * si;
    m.m[1].y = ch * cj;
    m.m[1].z = sh * si + ch * sj * ci;
    m.m[2].x = cj * si;
    m.m[2].y = -sj;
    m.m[2].z = cj * ci;
    m
}

/// `VIRAND`: `((float)rand() - (float)rand()) * 2^-31`, a signed unit random.
fn virand<M: Model>(rand: &mut Rand) -> F<M> {
    let a = F::<M>::from_int(rand.rand());
    let b = F::<M>::from_int(rand.rand());
    (a - b) * F::new(4.65661287e-10)
}

fn nonzero<M: Model>(v: &Vect3<M>) -> i32 {
    (v.x != F::ZERO || v.y != F::ZERO || v.z != F::ZERO) as i32
}

/// `VIParticleAttributes`. Every setter bumps `version`.
#[derive(Clone, Debug)]
pub struct Attributes<M: Model> {
    pub friction: F<M>,
    pub birthrate: F<M>,
    pub birthrate_var: F<M>,
    pub lifespan: F<M>,
    pub lifespan_var: F<M>,
    pub velocity: F<M>,
    pub velocity_var: F<M>,
    pub unk1c: Vect3<M>,
    pub start_size: F<M>,
    pub start_size_var: F<M>,
    pub end_size: F<M>,
    pub end_size_var: F<M>,
    pub gradient: [Color<M>; 32],
    pub start_color_var: Color<M>,
    pub end_color_var: Color<M>,
    pub inherit_velocity: F<M>,
    pub delta_spawn: F<M>,
    pub gradient_repeat: F<M>,
    pub inner_offset: Vect3<M>,
    pub inner_hpr_var: Vect3<M>,
    pub outer_offset: Vect3<M>,
    pub outer_hpr_var: Vect3<M>,
    pub nozzle_axis: Vect3<M>,
    pub nozzle_hpr_var: Vect3<M>,
    pub inner_offset_set: i32,
    pub inner_hpr_var_set: i32,
    pub outer_offset_set: i32,
    pub outer_hpr_var_set: i32,
    pub nozzle_hpr_var_set: i32,
    pub pattern: Option<Vec<Vect3<M>>>,
    /// `m_unk2C8`: a motif's blend factor this frame
    pub factor: F<M>,
    pub version: i32,
    pub gravity: F<M>,
}

impl<M: Model> Default for Attributes<M> {
    /// The constructor: all zero, then the setters' defaults.
    fn default() -> Self {
        let z = F::ZERO;
        let zc = Color::default();
        let mut a = Attributes {
            friction: z,
            birthrate: z,
            birthrate_var: z,
            lifespan: z,
            lifespan_var: z,
            velocity: z,
            velocity_var: z,
            unk1c: Vect3::default(),
            start_size: z,
            start_size_var: z,
            end_size: z,
            end_size_var: z,
            gradient: [zc; 32],
            start_color_var: zc,
            end_color_var: zc,
            inherit_velocity: z,
            delta_spawn: z,
            gradient_repeat: z,
            inner_offset: Vect3::default(),
            inner_hpr_var: Vect3::default(),
            outer_offset: Vect3::default(),
            outer_hpr_var: Vect3::default(),
            nozzle_axis: Vect3::default(),
            nozzle_hpr_var: Vect3::default(),
            inner_offset_set: 0,
            inner_hpr_var_set: 0,
            outer_offset_set: 0,
            outer_hpr_var_set: 0,
            nozzle_hpr_var_set: 0,
            pattern: None,
            factor: z,
            version: 0,
            gravity: z,
        };
        let f = F::new;
        a.set_floats([f(0.0), f(100.0), f(0.0), f(0.5), f(0.0), f(2.0), f(0.0), f(0.2), f(0.0), f(0.2), f(0.0)]);
        a.start_color_var = Color::default();
        a.version += 1;
        a.end_color_var = Color::default();
        a.version += 1;
        a.inherit_velocity = z;
        a.version += 1;
        a.delta_spawn = z;
        a.version += 1;
        a.set_nozzle_axis(Vect3::of(0.0, 1.0, 0.0));
        a.set_vec(VecField::NozzleHprVar, Vect3::of(1.0, 1.0, 1.0));
        a.gradient_repeat = F::ONE;
        a.version += 1;
        a.set_gravity_on(0);
        for i in 0..32 {
            let t = F::ONE - F::<M>::from_int(i) / F::new(31.0);
            a.gradient[i as usize] = Color { r: F::ONE, g: t, b: F::ZERO, a: t };
            a.version += 1;
        }
        a
    }
}

/// The five vector setters that keep an "is set" flag.
#[derive(Clone, Copy)]
pub enum VecField {
    InnerOffset,
    InnerHprVar,
    OuterOffset,
    OuterHprVar,
    NozzleHprVar,
}

impl<M: Model> Attributes<M> {
    /// The eleven scalar setters from friction to end size variance, in file
    /// order (the parser's order).
    fn set_floats(&mut self, v: [F<M>; 11]) {
        let [fr, br, brv, ls, lsv, ve, vev, ss, ssv, es, esv] = v;
        self.friction = fr;
        self.birthrate = br;
        self.birthrate_var = brv;
        self.lifespan = ls;
        self.lifespan_var = lsv;
        self.velocity = ve;
        self.velocity_var = vev;
        self.start_size = ss;
        self.start_size_var = ssv;
        self.end_size = es;
        self.end_size_var = esv;
        self.version += 11;
    }

    pub fn set_vec(&mut self, which: VecField, v: Vect3<M>) {
        let set = nonzero(&v);
        match which {
            VecField::InnerOffset => (self.inner_offset, self.inner_offset_set) = (v, set),
            VecField::InnerHprVar => (self.inner_hpr_var, self.inner_hpr_var_set) = (v, set),
            VecField::OuterOffset => (self.outer_offset, self.outer_offset_set) = (v, set),
            VecField::OuterHprVar => (self.outer_hpr_var, self.outer_hpr_var_set) = (v, set),
            VecField::NozzleHprVar => (self.nozzle_hpr_var, self.nozzle_hpr_var_set) = (v, set),
        }
        self.version += 1;
    }

    /// `SetNozzleAxis`: no flag, and it normalises what it is handed.
    pub fn set_nozzle_axis(&mut self, v: Vect3<M>) {
        self.nozzle_axis = v;
        self.nozzle_axis.normalize();
        self.version += 1;
    }

    pub fn set_gravity_on(&mut self, on: i32) {
        self.gravity = if on != 0 { F::new(-5.0) } else { F::ZERO };
        self.version += 1;
    }

    /// One attribute block of the file (`ParseParticleDefinition`'s loop body,
    /// past a motif's name). Returns the bytes read.
    fn parse(&mut self, d: &[u8], version: i32) -> Option<usize> {
        let mut p = 0usize;
        let mut f = || -> Option<F<M>> {
            let v = F::new(f32::from_le_bytes(d.get(p..p + 4)?.try_into().ok()?));
            p += 4;
            Some(v)
        };
        let mut v11 = [F::ZERO; 11];
        for x in v11.iter_mut() {
            *x = f()?;
        }
        let (inherit, spawn) = (f()?, f()?);
        let mut col = || -> Option<Color<M>> { Some(Color { r: f()?, g: f()?, b: f()?, a: f()? }) };
        let scv = col()?;
        let ecv = col()?;
        let mut grad = [Color::default(); 32];
        for g in grad.iter_mut() {
            *g = col()?;
        }
        drop(col);
        let repeat = f()?;
        let mut vecs = [Vect3::default(); 6];
        for v in vecs.iter_mut() {
            *v = Vect3::new(f()?, f()?, f()?);
        }
        let gravity = if version > 0 { Some(f()?.bits() as i32) } else { None };
        drop(f);
        // the setters, in the parser's order
        self.set_floats(v11);
        self.inherit_velocity = inherit;
        self.version += 1;
        self.delta_spawn = spawn;
        self.version += 1;
        self.start_color_var = scv;
        self.version += 1;
        self.end_color_var = ecv;
        self.version += 1;
        for (k, g) in grad.into_iter().enumerate() {
            self.gradient[k] = g;
            self.version += 1;
        }
        self.gradient_repeat = repeat;
        self.version += 1;
        self.set_vec(VecField::InnerOffset, vecs[0]);
        self.set_vec(VecField::InnerHprVar, vecs[1]);
        self.set_vec(VecField::OuterOffset, vecs[2]);
        self.set_vec(VecField::OuterHprVar, vecs[3]);
        self.set_nozzle_axis(vecs[4]);
        self.set_vec(VecField::NozzleHprVar, vecs[5]);
        self.set_gravity_on(gravity.unwrap_or(0));
        Some(p)
    }
}

/// `VIParticleMotif`: a named set of attributes a spell blends toward.
#[derive(Clone, Debug)]
pub struct Motif<M: Model> {
    pub attrs: Attributes<M>,
    pub name_hash: u32,
}

/// `VIParticleDefinition`.
#[derive(Clone, Debug)]
pub struct Definition<M: Model> {
    pub attrs: Attributes<M>,
    /// 0 none, 1..4 the material's alpha-blend modes
    pub blend: i32,
    pub zwrite: bool,
    pub ztest: bool,
    /// 1 whole sheet, 2 its halves, 3 three quadrants, 4 four
    pub tex_config: i32,
    /// the texture's DictID, 0 none
    pub texture: u32,
    pub motifs: Vec<Motif<M>>,
}

impl<M: Model> Default for Definition<M> {
    fn default() -> Self {
        Definition { attrs: Attributes::default(), blend: 0, zwrite: false, ztest: true, tex_config: 1, texture: 0, motifs: Vec::new() }
    }
}

impl<M: Model> Definition<M> {
    /// `ParseParticleDefinition` over a definition object's body (`version`
    /// its object version): the texture, blend, z write, z test, texture
    /// configuration and motif count, then the definition's attributes and
    /// each motif's (a 32-byte name first).
    pub fn parse(body: &[u8], version: i32) -> Option<Self> {
        let i = |o: usize| -> Option<i32> { Some(i32::from_le_bytes(body.get(o..o + 4)?.try_into().ok()?)) };
        let mut d = Definition::default();
        d.texture = i(0)? as u32;
        d.blend = i(4)?;
        d.zwrite = i(8)? != 0;
        d.ztest = i(12)? != 0;
        d.tex_config = i(16)?;
        let n = i(20)?;
        let mut p = 24usize;
        p += d.attrs.parse(body.get(p..)?, version)?;
        for _ in 0..n.max(0) {
            let name = body.get(p..p + 32)?;
            let end = name.iter().position(|&b| b == 0).unwrap_or(32);
            let name_hash = crate::hash::hash_resource_id(&String::from_utf8_lossy(&name[..end]));
            p += 32;
            let mut attrs = Attributes::default();
            p += attrs.parse(body.get(p..)?, version)?;
            d.motifs.push(Motif { attrs, name_hash });
        }
        Some(d)
    }

    /// `VIParticleDefinitionEx::BlendMotifs`: the definition's attributes,
    /// moved toward each motif by its factor; the flags recomputed.
    fn blend_motifs(&self, out: &mut Attributes<M>) {
        *out = self.attrs.clone();
        let a = &self.attrs;
        for m in &self.motifs {
            let f = m.attrs.factor;
            if f == F::ZERO {
                continue;
            }
            let b = &m.attrs;
            let lerp = |o: &mut F<M>, x: F<M>, y: F<M>| *o = *o + (x - y) * f;
            let lerp3 = |o: &mut Vect3<M>, x: &Vect3<M>, y: &Vect3<M>| {
                o.x = o.x + (x.x - y.x) * f;
                o.y = o.y + (x.y - y.y) * f;
                o.z = o.z + (x.z - y.z) * f;
            };
            let lerpc = |o: &mut Color<M>, x: &Color<M>, y: &Color<M>| {
                o.r = o.r + (x.r - y.r) * f;
                o.g = o.g + (x.g - y.g) * f;
                o.b = o.b + (x.b - y.b) * f;
                o.a = o.a + (x.a - y.a) * f;
            };
            lerp(&mut out.friction, b.friction, a.friction);
            lerp(&mut out.birthrate, b.birthrate, a.birthrate);
            lerp(&mut out.birthrate_var, b.birthrate_var, a.birthrate_var);
            lerp(&mut out.lifespan, b.lifespan, a.lifespan);
            lerp(&mut out.lifespan_var, b.lifespan_var, a.lifespan_var);
            lerp(&mut out.velocity, b.velocity, a.velocity);
            lerp(&mut out.velocity_var, b.velocity_var, a.velocity_var);
            lerp3(&mut out.unk1c, &b.unk1c, &a.unk1c);
            lerp(&mut out.start_size, b.start_size, a.start_size);
            lerp(&mut out.start_size_var, b.start_size_var, a.start_size_var);
            lerp(&mut out.end_size, b.end_size, a.end_size);
            lerp(&mut out.end_size_var, b.end_size_var, a.end_size_var);
            lerp(&mut out.gravity, b.gravity, a.gravity);
            for i in 0..32 {
                lerpc(&mut out.gradient[i], &b.gradient[i], &a.gradient[i]);
            }
            lerpc(&mut out.start_color_var, &b.start_color_var, &a.start_color_var);
            lerpc(&mut out.end_color_var, &b.end_color_var, &a.end_color_var);
            lerp(&mut out.inherit_velocity, b.inherit_velocity, a.inherit_velocity);
            lerp(&mut out.delta_spawn, b.delta_spawn, a.delta_spawn);
            lerp(&mut out.gradient_repeat, b.gradient_repeat, a.gradient_repeat);
            lerp3(&mut out.inner_offset, &b.inner_offset, &a.inner_offset);
            lerp3(&mut out.inner_hpr_var, &b.inner_hpr_var, &a.inner_hpr_var);
            lerp3(&mut out.outer_offset, &b.outer_offset, &a.outer_offset);
            lerp3(&mut out.outer_hpr_var, &b.outer_hpr_var, &a.outer_hpr_var);
            lerp3(&mut out.nozzle_axis, &b.nozzle_axis, &a.nozzle_axis);
            lerp3(&mut out.nozzle_hpr_var, &b.nozzle_hpr_var, &a.nozzle_hpr_var);
        }
        out.inner_offset_set = nonzero(&out.inner_offset);
        out.inner_hpr_var_set = nonzero(&out.inner_hpr_var);
        out.outer_offset_set = nonzero(&out.outer_offset);
        out.outer_hpr_var_set = nonzero(&out.outer_hpr_var);
        out.nozzle_hpr_var_set = nonzero(&out.nozzle_hpr_var);
    }
}

/// `VIParticle`
#[derive(Clone, Copy, Debug, Default)]
pub struct Particle<M: Model> {
    pub age: F<M>,
    pub age_rate: F<M>,
    pub friction: F<M>,
    pub grad_repeat: F<M>,
    pub pos: Vect3<M>,
    pub vel: Vect3<M>,
    pub size: F<M>,
    pub growth: F<M>,
    pub gravity: F<M>,
    pub frame: i32,
}

/// `VIMotifBlender`: a motif of the emitter's definition and how far toward
/// it the emitter's attributes are blended.
#[derive(Clone, Copy, Debug)]
pub struct Blender<M: Model> {
    pub motif: usize,
    pub factor: F<M>,
    pub changed: bool,
}

/// `VIParticleEmitter`
#[derive(Clone, Debug)]
pub struct Emitter<M: Model> {
    pub def: usize,
    pub attributes: Attributes<M>,
    blend_version: i32,
    /// the live particles, in list order (indices into the system's pool)
    pub particles: Vec<usize>,
    /// `m_unk4C`: the fraction of a birth carried to the next frame
    carry: F<M>,
    pub direction: Matrix33<M>,
    pub location: Vect3<M>,
    pub last_location: Vect3<M>,
    have_location: bool,
    pub attractor: Option<(Vect3<M>, F<M>)>,
    pub space: Matrix44<M>,
    pub static_bounds: BBox<M>,
    pub dyn_bounds: BBox<M>,
    /// in link order; a blender deleted leaves its place empty
    pub blenders: Vec<Option<Blender<M>>>,
}

/// `VIBillboard`, as `Render` fills it.
#[derive(Clone, Copy, Debug)]
pub struct Billboard<M: Model> {
    pub location: Vect3<M>,
    /// the frame << 2: the first of its four corner UVs
    pub frame: i32,
    pub width: F<M>,
    pub height: F<M>,
    pub color: Color<M>,
}

/// `VIParticleSystem`
pub struct System<M: Model> {
    pool: Vec<Particle<M>>,
    free: VecDeque<usize>,
    pub defs: Vec<Option<Definition<M>>>,
    /// by id; an id is never reused (a spell's blender may outlive its
    /// emitter and must not find another in its place)
    pub emitters: Vec<Option<Box<Emitter<M>>>>,
    /// emitter ids in list order (`m_emitters`)
    order: Vec<usize>,
    pub time: i64,
    dt_history: [F<M>; 8],
    pub delta_time: F<M>,
}

impl<M: Model> System<M> {
    /// `Init(time, raster, max)`: the pool, and a history of 1/30 s frames.
    pub fn new(time: i64, max: usize) -> Self {
        System {
            pool: vec![Particle::default(); max],
            free: (0..max).collect(),
            defs: Vec::new(),
            emitters: Vec::new(),
            order: Vec::new(),
            time,
            dt_history: [F::ONE / F::new(30.0); 8],
            delta_time: F::ZERO,
        }
    }

    pub fn add_definition(&mut self, d: Definition<M>) -> usize {
        self.defs.push(Some(d));
        self.defs.len() - 1
    }

    pub fn particle(&self, i: usize) -> &Particle<M> {
        &self.pool[i]
    }

    /// `VIParticleEmitter::Create`: the definition's attributes, at the
    /// origin facing +z (identity), appended to the system's list.
    pub fn create_emitter(&mut self, def: usize) -> usize {
        let attrs = self.defs[def].as_ref().map(|d| d.attrs.clone()).unwrap_or_default();
        let mut e = Emitter {
            def,
            attributes: attrs,
            blend_version: 0,
            particles: Vec::new(),
            carry: F::ZERO,
            direction: Matrix33::identity(),
            location: Vect3::default(),
            last_location: Vect3::default(),
            have_location: false,
            attractor: None,
            space: Matrix44::identity(),
            static_bounds: BBox::default(),
            dyn_bounds: BBox::default(),
            blenders: Vec::new(),
        };
        e.recompute_static_bbox();
        let id = self.emitters.len();
        self.emitters.push(Some(Box::new(e)));
        self.order.push(id);
        id
    }

    /// `VIParticleEmitter::Destroy`: its particles back to the pool.
    pub fn destroy_emitter(&mut self, id: usize) {
        if let Some(e) = self.emitters.get_mut(id).and_then(|e| e.take()) {
            self.free.extend(e.particles);
            self.order.retain(|&k| k != id);
        }
    }

    /// `VIParticleEmitter::Purge`
    pub fn purge(&mut self, id: usize) {
        if let Some(Some(e)) = self.emitters.get_mut(id) {
            self.free.extend(e.particles.drain(..));
        }
    }

    pub fn emitter(&mut self, id: usize) -> &mut Emitter<M> {
        self.emitters[id].as_mut().unwrap()
    }

    /// The emitter, if it has not been destroyed.
    pub fn get(&self, id: usize) -> Option<&Emitter<M>> {
        self.emitters.get(id).and_then(|e| e.as_deref())
    }

    pub fn get_mut(&mut self, id: usize) -> Option<&mut Emitter<M>> {
        self.emitters.get_mut(id).and_then(|e| e.as_deref_mut())
    }

    /// `ParticleCount`, 0 for a destroyed emitter.
    pub fn particle_count(&self, id: usize) -> usize {
        self.get(id).map_or(0, |e| e.particles.len())
    }

    /// `LinkMotifBlender(hash, b)` on an emitter, by its definition's motif.
    pub fn link_blender(&mut self, id: usize, hash: u32) -> Option<usize> {
        let def = self.get(id)?.def;
        let d = self.defs.get(def)?.as_ref()?.clone();
        self.get_mut(id)?.link_blender(&d, hash)
    }

    /// `VIParticleEmitter::Update`: one frame of births.
    pub fn update(&mut self, id: usize, rand: &mut Rand) {
        let Some(e) = self.emitters.get_mut(id).and_then(|e| e.as_mut()) else { return };
        let Some(def) = self.defs.get_mut(e.def).and_then(|d| d.as_mut()) else { return };
        e.blend_motifs(def);
        let a = &e.attributes;
        let dt = self.delta_time;
        let inv_dt = F::ONE / dt;
        let mut n = (a.birthrate + virand::<M>(rand) * a.birthrate_var) * dt;
        if a.delta_spawn != F::ZERO {
            n = n + e.location.distance(&e.last_location) * a.delta_spawn;
        }
        n = n + e.carry;
        let count = n.0 as i32;
        e.carry = n - F::from_int(count);
        let step = F::ONE / F::from_int(count);
        let mut at = e.last_location;
        let mut d = Vect3::new(e.location.x - e.last_location.x, e.location.y - e.last_location.y, e.location.z - e.last_location.z);
        d = Vect3::new(d.x * step, d.y * step, d.z * step);
        let (first, num) = match def.tex_config {
            1 => (0, 1),
            2 => (5, 2),
            3 => (1, 3),
            4 => (1, 4),
            _ => (0, 1),
        };
        let mut i = 0;
        while i < count {
            let Some(pi) = self.free.pop_front() else { break };
            let a = &e.attributes;
            let mut p = Particle::default();
            if let Some(pat) = &a.pattern {
                p.pos = pat[(rand.rand() % pat.len() as i32) as usize];
                p.pos = vm33(&p.pos, &e.direction);
                p.pos = Vect3::new(p.pos.x + at.x, p.pos.y + at.y, p.pos.z + at.z);
            } else {
                p.pos = at;
            }
            at = Vect3::new(at.x + d.x, at.y + d.y, at.z + d.z);
            e.particles.push(pi);
            p.age = F::ZERO;
            p.age_rate = a.lifespan + virand::<M>(rand) * a.lifespan_var;
            let life = if p.age_rate < F::ZERO {
                F::ZERO
            } else if p.age_rate < F::new(1000.0) {
                p.age_rate
            } else {
                F::new(1000.0)
            };
            p.age_rate = F::ONE / life;
            p.friction = a.friction;
            p.gravity = a.gravity;
            p.frame = if first != 0 { first + rand.rand() % num } else { 0 };
            p.grad_repeat = a.gradient_repeat;
            p.size = (a.start_size + virand::<M>(rand) * a.start_size_var) * F::new(0.5);
            p.growth = ((a.end_size + virand::<M>(rand) * a.end_size_var) * F::new(0.5) - p.size) * p.age_rate;
            let mut off = Vect3::<M>::default();
            let mut dir = a.nozzle_axis;
            if a.nozzle_hpr_var_set != 0 {
                let b = dir;
                let h = a.nozzle_hpr_var.x * virand::<M>(rand);
                let t = a.nozzle_hpr_var.y * virand::<M>(rand);
                let r = a.nozzle_hpr_var.z * virand::<M>(rand);
                let m = rotate_hpr(&Vect3::new(h, t, r));
                dir = m.mul_point(&b);
            }
            if (a.outer_hpr_var_set | a.outer_offset_set) != 0 {
                let mut av = off;
                av = Vect3::new(av.x + a.outer_offset.x, av.y + a.outer_offset.y, av.z + a.outer_offset.z);
                let b = dir;
                let h = a.outer_hpr_var.x * virand::<M>(rand);
                let t = a.outer_hpr_var.y * virand::<M>(rand);
                let r = a.outer_hpr_var.z * virand::<M>(rand);
                let m = rotate_hpr(&Vect3::new(h, t, r));
                off = m.mul_point(&av);
                dir = m.mul_point(&b);
            }
            if (a.inner_hpr_var_set | a.inner_offset_set) != 0 {
                let mut av = off;
                av = Vect3::new(av.x + a.inner_offset.x, av.y + a.inner_offset.y, av.z + a.inner_offset.z);
                let b = dir;
                let h = a.inner_hpr_var.x * virand::<M>(rand);
                let t = a.inner_hpr_var.y * virand::<M>(rand);
                let r = a.inner_hpr_var.z * virand::<M>(rand);
                let m = rotate_hpr(&Vect3::new(h, t, r));
                off = m.mul_point(&av);
                dir = m.mul_point(&b);
            }
            dir = vm33(&dir, &e.direction);
            off = vm33(&off, &e.direction);
            p.pos = Vect3::new(p.pos.x + off.x, p.pos.y + off.y, p.pos.z + off.z);
            let speed = a.velocity + virand::<M>(rand) * a.velocity_var;
            dir = Vect3::new(dir.x * speed, dir.y * speed, dir.z * speed);
            p.vel = dir;
            if a.inherit_velocity != F::ZERO {
                let mut b = Vect3::new(e.location.x - e.last_location.x, e.location.y - e.last_location.y, e.location.z - e.last_location.z);
                b = Vect3::new(b.x * inv_dt, b.y * inv_dt, b.z * inv_dt);
                let k = a.inherit_velocity;
                b = Vect3::new(b.x * k, b.y * k, b.z * k);
                p.vel = Vect3::new(p.vel.x + b.x, p.vel.y + b.y, p.vel.z + b.z);
            }
            self.pool[pi] = p;
            i += 1;
        }
    }

    /// `VIParticleSystem::Render`: the frame time (capped at 0.1 s, smoothed
    /// over eight frames); then every live particle of every emitter, in list
    /// order, is emitted as a billboard and moved, slowed, pulled, aged and
    /// grown, and freed once its age reaches 1. Returns each emitter's
    /// billboards.
    pub fn render(&mut self, time: i64) -> Vec<(usize, Vec<Billboard<M>>)> {
        let mut dt = F::<M>::from_int(time.wrapping_sub(self.time) as i32) * F::new(0.001);
        self.time = time;
        dt = if F::new(0.1) < dt { F::new(0.1) } else { dt };
        let mut sum = F::ZERO;
        for i in 0..7 {
            sum = sum + self.dt_history[i];
        }
        sum = (sum + dt) * F::new(0.125);
        for i in 0..7 {
            self.dt_history[7 - i] = self.dt_history[6 - i];
        }
        self.dt_history[0] = dt;
        self.delta_time = sum;
        let dts = self.delta_time;
        let mut out = Vec::new();
        for k in 0..self.order.len() {
            let id = self.order[k];
            let Some(e) = self.emitters[id].as_mut() else { continue };
            if e.particles.is_empty() {
                continue;
            }
            let big = F::new(1.0e7);
            e.dyn_bounds.min = Vect3::new(big, big, big);
            e.dyn_bounds.max = Vect3::new(-big, -big, -big);
            let mut bbs = Vec::with_capacity(e.particles.len());
            let mut keep = Vec::with_capacity(e.particles.len());
            for &pi in &e.particles {
                let p = &mut self.pool[pi];
                let v = Vect3::new(p.vel.x * dts, p.vel.y * dts, p.vel.z * dts);
                let slow = F::ONE - p.friction * dts;
                p.vel = Vect3::new(p.vel.x * slow, p.vel.y * slow, p.vel.z * slow);
                let gi = ((p.age * F::new(31.0) * p.grad_repeat).0 as i32) & 31;
                bbs.push(Billboard {
                    location: e.space.mul_point(&p.pos),
                    frame: p.frame << 2,
                    width: p.size,
                    height: p.size,
                    color: e.attributes.gradient[gi as usize],
                });
                p.pos = Vect3::new(p.pos.x + v.x, p.pos.y + v.y, p.pos.z + v.z);
                let b = &mut e.dyn_bounds;
                if p.pos.x < b.min.x {
                    b.min.x = p.pos.x;
                }
                if p.pos.y < b.min.y {
                    b.min.y = p.pos.y;
                }
                if p.pos.z < b.min.z {
                    b.min.z = p.pos.z;
                }
                if p.pos.x > b.max.x {
                    b.max.x = p.pos.x;
                }
                if p.pos.y > b.max.y {
                    b.max.y = p.pos.y;
                }
                if p.pos.z > b.max.z {
                    b.max.z = p.pos.z;
                }
                if let Some((at, kk)) = e.attractor {
                    let mut w = Vect3::new(at.x - p.pos.x, at.y - p.pos.y, at.z - p.pos.z);
                    w.normalize();
                    let s = kk * dts;
                    p.vel = Vect3::new(p.vel.x + w.x * s, p.vel.y + w.y * s, p.vel.z + w.z * s);
                }
                p.vel.y = p.vel.y + p.gravity * dts;
                p.age = p.age + p.age_rate * dts;
                p.size = p.size + p.growth * dts;
                if F::ONE <= p.age {
                    self.free.push_back(pi);
                } else {
                    keep.push(pi);
                }
            }
            e.particles = keep;
            out.push((id, bbs));
        }
        out
    }
}

impl<M: Model> Emitter<M> {
    /// `m_unk4C`: the fraction of a birth carried to the next frame.
    pub fn carry(&self) -> F<M> {
        self.carry
    }

    /// `SetLocation`: the last one kept, and both the same the first time.
    pub fn set_location(&mut self, v: Vect3<M>) {
        self.last_location = self.location;
        self.location = v;
        if !self.have_location {
            self.have_location = true;
            self.last_location = self.location;
        }
    }

    /// `SetDirection(const VIVect3 &)`: a basis around the vector.
    pub fn set_direction_vec(&mut self, v: &Vect3<M>) {
        let mut up = Vect3::of(0.0, 1.0, 0.0);
        if (v.x * up.x + v.y * up.y + v.z * up.z).abs() > F::new(0.999000013) {
            up = Vect3::of(0.0, 0.0, 1.0);
        }
        let cross = |a: &Vect3<M>, b: &Vect3<M>| Vect3::new(a.y * b.z - a.z * b.y, -a.x * b.z + a.z * b.x, a.x * b.y - a.y * b.x);
        let mut a = cross(&up, v);
        up = cross(v, &a);
        let mut c = *v;
        a.normalize();
        up.normalize();
        c.normalize();
        self.direction = Matrix33 { m: [a, up, c] };
    }

    /// `BlendMotifs`: the blenders' factors into the motifs; a change of any,
    /// or of a version, re-blends the attributes and the static box.
    fn blend_motifs(&mut self, def: &mut Definition<M>) {
        let mut changed = false;
        let mut version = 0i32;
        for m in &mut def.motifs {
            m.attrs.factor = F::ZERO;
        }
        for b in self.blenders.iter_mut().flatten() {
            if b.changed {
                b.changed = false;
                changed = true;
            }
            if let Some(m) = def.motifs.get_mut(b.motif) {
                m.attrs.factor = b.factor;
                if b.factor != F::ZERO {
                    version = version.wrapping_add(m.attrs.version);
                }
            }
        }
        version = version.wrapping_add(def.attrs.version);
        if changed || version != self.blend_version {
            def.blend_motifs(&mut self.attributes);
            self.recompute_static_bbox();
            self.blend_version = version;
        }
    }

    /// `RecomputeStaticBBox`: twelve test particles at full reach, swung to
    /// the spread angles' extremes, and half the largest particle around.
    pub fn recompute_static_bbox(&mut self) {
        let a = &self.attributes;
        let mut sb = BBox::<M>::default();
        let s0 = a.start_size + a.start_size_var.abs();
        let s1 = a.end_size + a.end_size_var.abs();
        let life = a.lifespan + a.lifespan_var.abs();
        let speed = a.velocity + a.velocity_var.abs();
        let half = (if s0 > s1 { s0 } else { s1 }) * F::new(0.5);
        let ang = [F::new(-1.0), F::new(-0.33), F::new(0.33), F::ONE];
        let pi = F::new(3.1415927);
        let vmin = |a: F<M>, b: F<M>| if a < b { a } else { b };
        let vmax = |a: F<M>, b: F<M>| if a > b { a } else { b };
        let rot = |axis: usize, var: &Vect3<M>, k: F<M>, lit_first: bool| {
            let pick = [var.x, var.y, var.z][axis];
            // the nozzle arms read `ang * pi`, the others `pi * ang`: the
            // same product
            let x = if lit_first { vmin(pick, pi) * k } else { vmin(pick, pi) * k };
            let mut v = Vect3::<M>::default();
            match axis {
                0 => v.x = x,
                1 => v.y = x,
                _ => v.z = x,
            }
            rotate_hpr(&v)
        };
        for i in 0..12usize {
            let mut base = Vect3::<M>::default();
            let ls = life * speed;
            let mut dir = Vect3::new(a.nozzle_axis.x * ls, a.nozzle_axis.y * ls, a.nozzle_axis.z * ls);
            let k = ang[i & 3];
            if a.nozzle_hpr_var_set != 0 {
                let q = dir;
                dir = rot(i % 3, &a.nozzle_hpr_var, k, true).mul_point(&q);
            }
            if (a.outer_hpr_var_set | a.outer_offset_set) != 0 {
                let p = Vect3::new(base.x + a.outer_offset.x, base.y + a.outer_offset.y, base.z + a.outer_offset.z);
                let q = dir;
                let m = rot((i + 4) % 3, &a.outer_hpr_var, k, false);
                base = m.mul_point(&p);
                dir = m.mul_point(&q);
            }
            if (a.inner_hpr_var_set | a.inner_offset_set) != 0 {
                let p = Vect3::new(base.x + a.inner_offset.x, base.y + a.inner_offset.y, base.z + a.inner_offset.z);
                let q = dir;
                let m = rot((i + 8) % 3, &a.inner_hpr_var, k, false);
                base = m.mul_point(&p);
                dir = m.mul_point(&q);
            }
            let e = Vect3::new(base.x + dir.x, base.y + dir.y, base.z + dir.z);
            sb.min.x = vmin(e.x, sb.min.x);
            sb.min.y = vmin(e.y, sb.min.y);
            sb.min.z = vmin(e.z, sb.min.z);
            sb.max.x = vmax(sb.max.x, e.x);
            sb.max.y = vmax(sb.max.y, e.y);
            sb.max.z = vmax(sb.max.z, e.z);
            sb.min.x = vmin(base.x, sb.min.x);
            sb.min.y = vmin(base.y, sb.min.y);
            sb.min.z = vmin(base.z, sb.min.z);
            sb.max.x = vmax(sb.max.x, base.x);
            sb.max.y = vmax(sb.max.y, base.y);
            sb.max.z = vmax(sb.max.z, base.z);
        }
        sb.min = Vect3::new(sb.min.x - half, sb.min.y - half, sb.min.z - half);
        sb.max = Vect3::new(sb.max.x + half, sb.max.y + half, sb.max.z + half);
        self.static_bounds = sb;
    }

    /// `LinkMotifBlender`: a new blender on a motif of the definition, by
    /// its name's hash, at factor 0.
    pub fn link_blender(&mut self, def: &Definition<M>, hash: u32) -> Option<usize> {
        let motif = def.motifs.iter().position(|m| m.name_hash == hash)?;
        self.blenders.push(Some(Blender { motif, factor: F::ZERO, changed: true }));
        Some(self.blenders.len() - 1)
    }

    /// `~VIMotifBlender`: out of the emitter's list.
    pub fn unlink_blender(&mut self, b: usize) {
        if let Some(x) = self.blenders.get_mut(b) {
            *x = None;
        }
    }

    pub fn blender_factor(&self, b: usize) -> Option<F<M>> {
        self.blenders.get(b).and_then(|x| x.as_ref()).map(|x| x.factor)
    }

    /// `VIMotifBlender::SetFactor`: a change marks it.
    pub fn set_blender_factor(&mut self, b: usize, f: F<M>) {
        let Some(b) = self.blenders[b].as_mut() else { return };
        if b.factor != f {
            b.factor = f;
            b.changed = true;
        }
    }
}
