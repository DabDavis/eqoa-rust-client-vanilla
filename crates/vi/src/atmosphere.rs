//! `VIAtmosphere`: the sky and the weather (decomp `viatmosphere.cpp`), line
//! by line, and `VIScene::SetSky`'s sky type.
//!
//! The sky is a dome sprite, a sun, a corona and a moon, drawn with the
//! camera's rotation only (`RenderSky` zeroes the view's translation), and
//! 128 stars at night. The weather is 200 particles in a 10-unit box that
//! follows the camera, each dropped to the ground (or a liquid surface) by a
//! ray, rain (and its splashes) or snow by the effect mix; the Lavastorm drops
//! fireballs. A change of weather or sky type cross-fades over 30 seconds
//! (`ProcessTransitions`): the sun, corona and moon fade, the dome blends a
//! third layer in and out, the intensity follows a seven-step table, and the
//! rain's two voices are kept going. Rain also thunders, every 8 seconds or
//! so, and under a dark sky each clap is a lightning flash (`GetLightning`,
//! which also whitens the sun in the time of day) that lights 14 patches of
//! the sky.
//!
//! What the atmosphere asks of the scene (the two rays, the sound device, the
//! particle system for the fireballs) comes in through traits; what it sets
//! on the raster (billboards per material, the sky sprites' material layers,
//! the sprites it rasters) is recorded in [`Raster`] and in the material
//! mirrors it owns. Newlib's `rand()` is passed in.

use crate::coll::{CollRay, EnvRay};
use crate::csprite::Rand;
use crate::fpu::{F, Model, f};
use crate::geom::{Matrix44, Plane, Vect4};
use crate::particle::{Billboard, Color, Definition, System, rotate_hpr};
use crate::tod::PROFILES;
use crate::vect::Vect3;
use crate::world::ProfileBlend;
use std::collections::HashMap;

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
    let mut sun = rotate_hpr(&Vect3::new(z, z, w));
    sun.m[3].x = w.sinf() * f(35.0);
    sun.m[3].y = w.cosf() * f(-35.0);
    let mut t = Matrix44::identity();
    t.m[0].x = sky_scale;
    t.m[1].y = sky_scale;
    t.m[2].z = sky_scale;
    let corona = Matrix44::mul(&t, &sun);
    let mut moon = rotate_hpr(&Vect3::new(z, z, hour * f(0.2617993) + f(3.14159274)));
    moon.m[3].x = -sun.m[3].x;
    moon.m[3].y = -sun.m[3].y;
    SkyRotations { sun, corona, moon }
}

/// `VIScene::SetSky`: the sky type of the profile with the heaviest weight
/// (the first, on a tie). 0 clear, 1 overcast (the dome's own texture), 2 and
/// up dark clouds.
pub fn sky_type<M: Model>(b: &ProfileBlend<M>) -> i32 {
    let mut k = 0;
    for j in 1..4 {
        if b.weight[k] < b.weight[j] {
            k = j;
        }
    }
    PROFILES.get(b.index[k] as usize).map_or(0, |p| p.sky)
}

/// `VIScene::SetSky`'s choice and its plane of sky: the heaviest profile's
/// sky type, and plane 1 (the 360° dome, no sun, moon or stars) for profile 20.
pub fn sky_profile<M: Model>(b: &ProfileBlend<M>) -> (i32, i32) {
    let mut best = F::<M>::ZERO;
    let mut k = 0;
    for j in 0..4 {
        if j == 0 || best < b.weight[j] {
            best = b.weight[j];
            k = j;
        }
    }
    let sky = PROFILES.get(b.index[k] as usize).map_or(0, |p| p.sky);
    (sky, (b.index[k] == 20) as i32)
}

/// `VIAtmosphere::WXTYPE`, the wire's weather byte as `VIClient::SetWeather`
/// maps it (anything past 6 is clear). The decomp's `vi.h` names 1..6 rain,
/// snow, sleet, sandstorm, Lavastorm, thunderstorm; what the code does with
/// them says otherwise: `ProcessTransitions` makes 1-3 rain and 4-6 snow
/// (`st` 1 and 2: the effect mix, the material, the splashes, the thunder)
/// at intensities 0.15, 0.5 and 1 each, and the client's own debug command
/// sets 0, 2 and 5. The Lavastorm is not a weather at all: it is the terrain
/// profiles' (`SetLavastormIntensity`).
pub const WX_CLEAR: i32 = 0;
pub const WX_RAIN_LIGHT: i32 = 1;
pub const WX_RAIN: i32 = 2;
pub const WX_RAIN_HEAVY: i32 = 3;
pub const WX_SNOW_LIGHT: i32 = 4;
pub const WX_SNOW: i32 = 5;
pub const WX_SNOW_HEAVY: i32 = 6;

pub fn weather_from_wire(w: u8) -> i32 {
    if w <= 6 { w as i32 } else { WX_CLEAR }
}

/// One layer of a sky sprite's material, as the atmosphere reads and writes
/// it (`VIMaterial`'s texture, blend mode and colour).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    pub texture: i32,
    pub blend: i32,
    pub color: [u8; 4],
}

/// A material's layers: four slots, `n` in use. Shrinking keeps what the
/// slots held (`SetNumLayers` only releases their textures); growing resets
/// the new ones to textureless, white, opaque, blend 0. A slot `Init` never
/// set holds whatever the allocation held (`CopyLayer` reads one when the
/// plane of sky changes to a one-layer dome mid-fade); here it is zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    pub layers: [Layer; 4],
    pub n: usize,
}

impl Material {
    pub const BLANK: Layer = Layer { texture: -1, blend: 0, color: [255; 4] };
    pub const UNSET: Layer = Layer { texture: 0, blend: 0, color: [0; 4] };

    pub fn new(layers: &[Layer]) -> Self {
        let mut m = Material { layers: [Self::UNSET; 4], n: layers.len().min(4) };
        m.layers[..m.n].copy_from_slice(&layers[..m.n]);
        m
    }

    pub fn used(&self) -> &[Layer] {
        &self.layers[..self.n]
    }

    /// `SetNumLayers`
    pub fn set_num_layers(&mut self, n: usize) {
        if n >= 4 {
            return;
        }
        for i in self.n..n {
            self.layers[i] = Self::BLANK;
        }
        self.n = n;
    }

    /// `CopyLayer(src, dst)`
    pub fn copy_layer(&mut self, src: usize, dst: usize) {
        if src != dst {
            self.layers[dst] = self.layers[src];
        }
    }
}

/// A sprite's material palette.
pub type Pal = Vec<Material>;

/// What the atmosphere asks of the scene: the two rays its weather and its
/// fireballs drop (`Collide(VICollRay &, WeatherFilter &)`, which accepts
/// everything, and `Collide(VIEnvRay &)` for a liquid surface). Each sets its
/// ray's hit time and answers whether it hit.
pub trait Scene<M: Model> {
    fn collide_ray(&mut self, r: &mut CollRay<M>) -> bool;
    fn collide_env(&mut self, r: &mut EnvRay<M>) -> bool;
}

/// The sound device, as the atmosphere calls it (handles, `Play(s, 127, 0)`).
pub trait Sound {
    fn play(&mut self, sound: i32) -> i32;
    fn stop(&mut self, h: i32);
    fn set_volume(&mut self, h: i32, v: f32);
    fn set_pan(&mut self, h: i32, p: f32);
    fn set_pitch(&mut self, h: i32, p: i32);
}

/// The camera the atmosphere is handed: where it is and looks, its view
/// matrix, and its frustum's plane 0 (the near plane).
#[derive(Clone, Copy, Debug)]
pub struct Camera<M: Model> {
    pub position: Vect3<M>,
    pub dir: Vect3<M>,
    pub view: Matrix44<M>,
    pub near: Plane<M>,
}

/// What a frame put on the raster: each `SetMaterial` run's billboards, the
/// sky sprites rastered (with their model-views), and how often the fog was
/// turned off.
#[derive(Clone, Debug, Default)]
pub struct Raster<M: Model> {
    pub runs: Vec<(i32, Vec<Billboard<M>>)>,
    pub sprites: Vec<(i32, Matrix44<M>)>,
    pub fog_off: usize,
}

impl<M: Model> Raster<M> {
    fn material(&mut self, m: i32) {
        self.runs.push((m, Vec::new()));
    }
    fn bb(&mut self, b: Billboard<M>) {
        self.runs.last_mut().unwrap().1.push(b);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct WeatherParticle<M: Model> {
    pub location: Vect3<M>,
    pub color: Color<M>,
    /// the ground under it
    pub ground: F<M>,
    /// 1 when the ground is a liquid surface
    pub liquid: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Splash<M: Model> {
    pub location: Vect3<M>,
    pub bright: F<M>,
    pub frame: i32,
}

/// A Lavastorm fireball: where it falls from (40 above where it lands),
/// where it lands, how far down it is, how fast, whether it has landed, and
/// its two emitters (in the scene's particle system).
#[derive(Clone, Copy, Debug, Default)]
pub struct Fireball<M: Model> {
    pub from: Vect3<M>,
    pub to: Vect3<M>,
    pub t: F<M>,
    pub speed: F<M>,
    pub landed: i32,
    pub e0: usize,
    pub e1: usize,
}

static NOISE: [u8; 600] = include!("atmosphere_noise.in");
static STAR_NOISE: [u8; 384] = include!("atmosphere_stars.in");

/// `(float)((rand() * 131) & 0xFF) / 255.0f`
fn rand255<M: Model>(rand: &mut Rand) -> F<M> {
    F::from_int(rand.rand().wrapping_mul(131) & 0xFF) / f(255.0)
}

/// `((float)rand() - (float)rand()) * 2^-31`
fn virand<M: Model>(rand: &mut Rand) -> F<M> {
    let a = F::<M>::from_int(rand.rand());
    let b = F::<M>::from_int(rand.rand());
    (a - b) * f(4.65661287e-10)
}

/// `((float)rand() - (float)rand()) / 2147483648.0f`
fn virand_div<M: Model>(rand: &mut Rand) -> F<M> {
    let a = F::<M>::from_int(rand.rand());
    let b = F::<M>::from_int(rand.rand());
    (a - b) / f(2147483648.0)
}

fn add3<M: Model>(a: &Vect3<M>, b: &Vect3<M>) -> Vect3<M> {
    Vect3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

fn vmin<M: Model>(a: F<M>, b: F<M>) -> F<M> {
    if a < b { a } else { b }
}

fn vmax<M: Model>(a: F<M>, b: F<M>) -> F<M> {
    if a > b { a } else { b }
}

/// `VIAtmosphere`. Field names follow the decomp's where it names them; the
/// rest keep its offsets.
#[derive(Clone, Debug)]
pub struct Atmosphere<M: Model> {
    pub range: F<M>,
    pub num_particles: usize,
    pub particles: Vec<WeatherParticle<M>>,
    pub particle_scale: F<M>,
    pool_splash: Vec<Splash<M>>,
    /// active splash nodes, in list order, and the free ones
    splash_list: Vec<usize>,
    splash_free: Vec<usize>,
    pub camera: Vect3<M>,
    pub intensity: F<M>,
    pub lavastorm: F<M>,
    pub p50: [Vect3<M>; 2],
    pub p68: [Vect3<M>; 2],
    pub effect_mix: F<M>,
    pub p84: F<M>,
    pub p88: [F<M>; 2],
    pub p90: [F<M>; 2],
    pub p98: [F<M>; 2],
    pub mat: [i32; 2],
    pub pa8: [Vect3<M>; 64],
    pub p3a8: [F<M>; 64],
    /// `DeltaTime()`: the last tick, in seconds
    pub p4a8: F<M>,
    pub initialised: bool,
    pub sky_tex: [i32; 3],
    pub star_mat: i32,
    pub sky_sprite: i32,
    /// the two planes of sky: "sky" and "EQOA_Skydome_360" (-1 none)
    pub planes: [i32; 2],
    pub sun: i32,
    pub corona: i32,
    pub moon: i32,
    pub sky_rot: Matrix44<M>,
    pub corona_rot: Matrix44<M>,
    pub moon_rot: Matrix44<M>,
    pub sky_scale: F<M>,
    /// 1 while the sun, corona and moon are drawn
    pub overcast: i32,
    pub p5c8: i64,
    pub p5d0: i64,
    /// the sky's current and pending state (0 clear, 1 dark), its texture
    pub p5ec: i32,
    pub p5f0: i32,
    pub p5f4: i32,
    pub p5f8: i32,
    /// the weather: current, pending, and the one asked for during a change
    pub p5fc: i32,
    pub p600: i32,
    pub p604: i32,
    /// the change's progress, 0 -> 1; 0 settled
    pub p608: F<M>,
    /// the default sky: current, pending, asked for during a change
    pub p60c: i32,
    pub p610: i32,
    pub p614: i32,
    /// the plane of sky
    pub p618: i32,
    /// the stars' brightness
    pub p61c: F<M>,
    pub stars: [Vect4<M>; 128],
    pub pe20: [i8; 128],
    pub pea0: [F<M>; 32],
    pub star_tex: i32,
    pub pf24: F<M>,
    pub pf28: F<M>,
    pub pf2c: F<M>,
    pub pf30: Vect3<M>,
    pub pf3c: Vect3<M>,
    /// the lightning flash this frame
    pub pf48: F<M>,
    pub pf4c: i32,
    pub pf50: i32,
    pub patches: [Vect4<M>; 32],
    pub splash_mat: i32,
    pub splash_tex: i32,
    fireballs: Vec<Fireball<M>>,
    /// active fireball nodes, in list order, and the free ones
    fireball_list: Vec<usize>,
    fireball_free: Vec<usize>,
    /// the Lavastorm's two particle definitions (head, impact), -1 none
    pub p1180: i32,
    pub p1184: i32,
    /// the rain sound and the thunder, and the rain's two playbacks
    pub p1188: i32,
    pub p118c: i32,
    pub p1190: i32,
    pub p1194: i32,
    /// the thunder's clock, its period, the flash's time, length, weight
    pub p1198: F<M>,
    pub p119c: F<M>,
    pub p11a0: F<M>,
    pub p11a4: F<M>,
    pub p11a8: F<M>,
    /// the sky sprites' material palettes, by sprite
    pub pals: HashMap<i32, Pal>,
    /// `s_skyType`, `VIScene::SetSky`'s static: the sky type last seen
    pub seen_sky: i32,
}

impl<M: Model> Atmosphere<M> {
    /// `VIAtmosphere::Init(time, raster, n)`: the 200 weather particles, the
    /// wind table, the stars, the materials (numbered 0..5 in the order the
    /// raster makes them: rain, snow, splash, star, lightning patch).
    pub fn new(time: i64, rand: &mut Rand) -> Self {
        let z = F::ZERO;
        let v0 = Vect3::default();
        let m0 = Matrix44::identity();
        let mut a = Atmosphere {
            range: f(10.0),
            num_particles: 200,
            particles: Vec::new(),
            particle_scale: F::ONE,
            pool_splash: Vec::new(),
            splash_list: Vec::new(),
            splash_free: Vec::new(),
            camera: v0,
            intensity: z,
            lavastorm: z,
            p50: [v0; 2],
            p68: [v0; 2],
            effect_mix: z,
            p84: z,
            p88: [f(0.1); 2],
            p90: [z; 2],
            p98: [z; 2],
            mat: [0, 1],
            pa8: [v0; 64],
            p3a8: [z; 64],
            p4a8: z,
            initialised: false,
            sky_tex: [-1; 3],
            star_mat: 3,
            sky_sprite: -1,
            planes: [-1; 2],
            sun: -1,
            corona: -1,
            moon: -1,
            sky_rot: m0,
            corona_rot: m0,
            moon_rot: m0,
            sky_scale: F::ONE,
            overcast: 1,
            p5c8: 0,
            p5d0: time,
            p5ec: 0,
            p5f0: 0,
            p5f4: -1,
            p5f8: -1,
            p5fc: 0,
            p600: 0,
            p604: 0,
            p608: z,
            p60c: 1,
            p610: 1,
            p614: 1,
            p618: 0,
            p61c: F::ONE,
            stars: [Vect4::default(); 128],
            pe20: [0; 128],
            pea0: [z; 32],
            star_tex: -1,
            pf24: z,
            pf28: z,
            pf2c: z,
            pf30: v0,
            pf3c: v0,
            pf48: z,
            pf4c: -1,
            pf50: 4,
            patches: [Vect4::default(); 32],
            splash_mat: 2,
            splash_tex: -1,
            fireballs: vec![Fireball::default(); 32],
            fireball_list: Vec::new(),
            fireball_free: (0..32).collect(),
            p1180: -1,
            p1184: -1,
            p1188: -1,
            p118c: -1,
            p1190: -1,
            p1194: -1,
            p1198: z,
            p119c: z,
            p11a0: z,
            p11a4: z,
            p11a8: z,
            pals: HashMap::new(),
            seen_sky: 0,
        };
        for i in 0..64 {
            let ang = F::<M>::from_int(i) * f(6.2831855) / f(64.0) * f(2.0);
            a.pa8[i as usize] = Vect3::new(ang.sinf(), ang.cosf(), ang.cosf());
            a.p3a8[i as usize] = rand255::<M>(rand) + F::ONE;
        }
        let range = a.range;
        for i in 0..a.num_particles {
            let al = virand::<M>(rand) * f(0.08) + f(0.1);
            let n = |k: usize| F::<M>::from_int(NOISE[i * 3 + k] as i32) / f(255.0);
            let half = range * f(0.5);
            a.particles.push(WeatherParticle {
                location: Vect3::new(n(0) * range - half, n(1) * range - half, n(2) * range - half),
                color: Color { r: F::ONE, g: F::ONE, b: F::ONE, a: al },
                ground: z,
                liquid: 0,
            });
        }
        for i in 0..128 {
            let s = |k: usize| F::<M>::from_int(128 - STAR_NOISE[i * 3 + k] as i32) / f(128.0);
            let m = rotate_hpr(&Vect3::new(s(0) * f(3.1415927) * f(2.3), s(1) * f(0.49) * f(3.1415927), s(2) * f(0.49) * f(3.1415927)));
            a.stars[i] = m.apply(&Vect4::new(z, f(100.0), z, F::ONE));
            a.stars[i].w = F::from_int((rand.rand() / 131) & 0xFF) / f(255.0) * f(0.5) + f(0.3);
            a.pe20[i] = ((rand.rand() / 131) & 0x1F) as i8;
        }
        for i in 0..32 {
            a.pea0[i] = ((F::<M>::from_int(i as i32) / f(16.0) * f(3.1415927)).sinf() * f(0.5) + f(0.5)) * f(0.4) + f(0.5);
        }
        a.pool_splash = vec![Splash::default(); a.num_particles];
        a.splash_free = (0..a.num_particles).collect();
        a.p88[0] = f(0.095);
        a.p98[0] = f(0.8);
        a.p90[0] = z;
        a.p50[0] = Vect3::of(0.0025, 0.008, 0.0025);
        a.p68[0] = Vect3::of(0.0, -0.21042, 0.0);
        a.p88[1] = f(0.05);
        a.p90[1] = z;
        a.p98[1] = z;
        a.p50[1] = Vect3::of(0.0106, 0.01723, 0.0106);
        a.p68[1] = Vect3::of(0.0, -0.02199, 0.0);
        a
    }

    /// What `SetResources` does to the Lavastorm's head definition: a spawn
    /// every 7.5 units of travel, and 60% of its lifespan.
    pub fn tune_fireball_head(d: &mut Definition<M>) {
        d.attrs.delta_spawn = f(7.5);
        d.attrs.version += 1;
        d.attrs.lifespan = d.attrs.lifespan * f(0.6);
        d.attrs.version += 1;
    }

    /// The tail of `SetResources`: the sprites, textures and sounds it found
    /// (the materials' own setup is the renderer's), and the atmosphere ready.
    /// The plane of sky starts as "sky" (the loop leaves it at plane 0).
    #[allow(clippy::too_many_arguments)]
    pub fn set_resources(&mut self, sun: i32, corona: i32, moon: i32, planes: [i32; 2], sky_tex: [i32; 3], rain: i32, thunder: i32, lavastorm: (i32, i32)) {
        self.sun = sun;
        self.corona = corona;
        self.moon = moon;
        self.planes = planes;
        self.sky_sprite = planes[0];
        self.sky_tex = sky_tex;
        self.p1188 = rain;
        self.p118c = thunder;
        (self.p1180, self.p1184) = lavastorm;
        self.initialised = true;
    }

    pub fn num_splashes(&self) -> usize {
        self.splash_list.len()
    }

    /// The fireballs falling or burning out, in list order.
    pub fn fireballs(&self) -> impl Iterator<Item = &Fireball<M>> {
        self.fireball_list.iter().map(|&k| &self.fireballs[k])
    }

    pub fn effect_mix(&self) -> F<M> {
        self.effect_mix
    }
    pub fn intensity(&self) -> F<M> {
        self.intensity
    }
    pub fn lavastorm_intensity(&self) -> F<M> {
        if self.p1180 == -1 || self.p1184 == -1 { F::ZERO } else { self.lavastorm }
    }

    pub fn set_intensity(&mut self, v: F<M>) {
        if v > F::ZERO && self.intensity == F::ZERO {
            self.clear_all_splashes();
        }
        self.intensity = v;
    }

    pub fn set_lavastorm_intensity(&mut self, v: F<M>, sys: &mut System<M>) {
        if self.p1180 == -1 || self.p1184 == -1 {
            return;
        }
        if v > F::ZERO && self.lavastorm == F::ZERO {
            self.clear_all_fireballs(sys);
        }
        self.lavastorm = v;
    }

    /// `VIScene::SetSky(cam, fade)` for the profile blend at the camera: a
    /// new sky type (or any, with `fade`, which despite its name makes the
    /// change instant) goes to `SetDefaultSky`, and profile 20 takes the 360°
    /// plane of sky. The client calls it every frame with `fade` 0, and with
    /// 1 on entering a world.
    pub fn set_sky(&mut self, b: &ProfileBlend<M>, fade: bool) {
        let (sky, plane) = sky_profile(b);
        if sky != self.seen_sky || fade {
            self.set_default_sky(sky, fade);
        }
        self.set_plane_of_sky(plane);
        self.seen_sky = sky;
    }

    pub fn set_plane_of_sky(&mut self, i: i32) {
        if self.planes[1] == -1 {
            return;
        }
        self.p618 = i;
        self.sky_sprite = self.planes[i as usize];
    }

    fn clear_all_splashes(&mut self) {
        if !self.initialised || self.splash_list.is_empty() {
            return;
        }
        self.splash_list.clear();
        self.splash_free = (0..self.num_particles).collect();
    }

    fn clear_all_fireballs(&mut self, sys: &mut System<M>) {
        if !self.initialised || self.fireball_list.is_empty() {
            return;
        }
        for &k in &self.fireball_list {
            sys.destroy_emitter(self.fireballs[k].e0);
            sys.destroy_emitter(self.fireballs[k].e1);
        }
        self.fireball_list.clear();
        self.fireball_free = (0..32).collect();
    }

    fn add_splash(&mut self, p: Vect3<M>, rand: &mut Rand) {
        if self.splash_free.is_empty() {
            return;
        }
        let n = self.splash_free.remove(0);
        let s = &mut self.pool_splash[n];
        s.location = p;
        s.frame = 0;
        s.bright = rand255::<M>(rand) * f(0.2) + f(0.282);
        self.splash_list.push(n);
    }

    pub fn reset_lightning(&mut self, rand: &mut Rand) {
        self.p11a8 = rand255::<M>(rand);
        self.p11a8 = vmin(vmax(f(0.1), self.p11a8), f(0.8));
        self.p1198 = f(8.0);
        self.p119c = self.p11a8 * f(6.0);
        self.p11a4 = rand255::<M>(rand) * f(0.6) + f(0.6);
        self.recompute_lightning_patches(rand);
    }

    pub fn overcast_interp(&self) -> F<M> {
        if self.p5ec == 0 {
            if self.p5f0 == 0 {
                return F::ZERO;
            }
            if self.p5f0 == 1 {
                return self.p608;
            }
        }
        if self.p5ec == 1 && self.p5f0 == 0 {
            return F::ONE - self.p608;
        }
        F::ONE
    }

    /// `SetWeather(t, instant)`: during a change the request waits.
    pub fn set_weather(&mut self, t: i32, instant: bool) -> i32 {
        if !self.initialised {
            return -1;
        }
        if self.p608 != F::ZERO {
            self.p604 = t;
        } else if self.p5fc != t {
            self.p608 = if instant { F::ONE } else { f(1.0e-05) };
            self.p604 = t;
            self.p600 = t;
            if t == 0 {
                self.p5f0 = (self.p60c == 2) as i32;
                self.p5f8 = self.sky_tex[self.p60c.clamp(0, 2) as usize];
            } else {
                self.p5f0 = 1;
                self.p5f8 = self.sky_tex[2];
            }
        }
        0
    }

    /// `SetDefaultSky(t, instant)`
    pub fn set_default_sky(&mut self, t: i32, instant: bool) -> i32 {
        if !self.initialised {
            return -1;
        }
        if self.p608 != F::ZERO {
            self.p614 = t;
        } else if self.p60c != t {
            self.p608 = if instant { F::ONE } else { f(1.0e-05) };
            self.p614 = t;
            self.p610 = t;
            if self.p5fc > 0 {
                self.p60c = t;
                self.p608 = F::ZERO;
                return 0;
            }
            if t == 0 {
                self.p5f0 = 0;
                self.p5f8 = self.sky_tex[0];
            } else if t == 1 {
                self.p5f0 = 0;
                self.p5f8 = self.sky_tex[1];
            } else {
                self.p5f0 = 1;
                self.p5f8 = self.sky_tex[2];
            }
        }
        0
    }

    pub fn recompute_lightning_patches(&mut self, rand: &mut Rand) {
        let pi = f(3.14159274);
        for i in 0..14 {
            let s = |rand: &mut Rand| F::<M>::from_int(128 - ((rand.rand() / 137) & 0xFF)) / f(128.0);
            let h = s(rand) * f(0.45) * pi;
            let p = s(rand) * f(0.32) * pi;
            let r = s(rand) * f(0.32) * pi;
            let m = rotate_hpr(&Vect3::new(h, p, r));
            self.patches[i] = m.apply(&Vect4::new(F::ZERO, f(100.0), F::ZERO, F::ONE));
            self.patches[i].w = F::from_int((rand.rand() / 131) & 0xFF) / f(255.0) * f(20.0) + f(70.0);
        }
    }

    /// `GetLightning`: the flash this frame (also kept for the patches), when
    /// the sky is dark and a flash is under way.
    pub fn get_lightning(&mut self, rand: &mut Rand) -> Option<F<M>> {
        if self.p5ec != 1 || self.p5f0 != 1 {
            self.p11a0 = F::ZERO;
            return None;
        }
        let t = self.p11a0;
        if t == F::ZERO {
            self.pf48 = F::ZERO;
            return None;
        }
        self.p11a0 = t + self.p4a8;
        let mut b;
        if self.p11a0 < f(0.2) {
            b = self.p11a0 / f(0.2);
            b = b + virand_div::<M>(rand) * f(0.2);
        } else {
            b = (self.p11a0 - f(0.2)) / (self.p11a4 - f(0.2));
            b = b + virand_div::<M>(rand) * f(0.3);
            if (rand.rand() & 0xF) < 2 {
                b = rand255::<M>(rand) * f(0.2) + f(0.8);
            }
        }
        if self.p11a0 >= self.p11a4 {
            self.p11a0 = F::ZERO;
        }
        let h = self.p11a4 * f(0.5);
        if t <= h && h < self.p11a0 && (rand.rand() & 0xF) < 6 {
            self.recompute_lightning_patches(rand);
        }
        let c = vmin(vmax(F::ZERO, b), F::ONE);
        let mut b = c * (F::ONE - self.p11a8);
        b = b * f(0.8);
        self.pf48 = b;
        Some(b)
    }

    /// `Update(scene, camera, time, hour, outdoors)`: the clock, the sky's
    /// rotations, the weather, then the transitions.
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, sc: &mut dyn Scene<M>, snd: &mut dyn Sound, sys: &mut System<M>, cam: &Camera<M>, time: i64, hour: F<M>, vol: F<M>, rand: &mut Rand) {
        if !self.initialised {
            return;
        }
        self.p5d0 = self.p5c8;
        self.p5c8 = time;
        self.p4a8 = F::from_int(time.wrapping_sub(self.p5d0) as i32) * f(0.001);
        let r = sky_rotations(hour, self.sky_scale);
        self.sky_rot = r.sun;
        self.corona_rot = r.corona;
        self.moon_rot = r.moon;
        self.process_weather(sc, sys, cam, rand);
        self.process_transitions(snd, hour, vol, rand);
    }

    fn pal_alpha(&mut self, sprite: i32, a: impl Fn() -> u8) {
        if let Some(p) = self.pals.get_mut(&sprite) {
            for e in p.iter_mut() {
                let n = e.n;
                for l in e.layers[..n].iter_mut() {
                    l.color[3] = a();
                }
            }
        }
    }

    fn dome(&mut self) -> Option<&mut Material> {
        self.pals.get_mut(&self.sky_sprite).and_then(|p| p.first_mut())
    }

    fn process_transitions(&mut self, snd: &mut dyn Sound, hour: F<M>, vol: F<M>, rand: &mut Rand) {
        let night = |hour: F<M>| {
            if hour < f(6.0) {
                F::ONE - hour / f(6.0)
            } else if f(18.0) < hour {
                (hour - f(18.0)) / f(6.0)
            } else {
                F::ZERO
            }
        };
        if self.p608 != F::ZERO {
            self.p61c = night(hour);
            self.p608 = self.p608 + self.p4a8 * f(1.0 / 30.0);
            self.p608 = vmin(self.p608, F::ONE);
            let alpha: i32 = if self.p5ec == 0 && self.p5f0 == 1 {
                self.p61c = self.p61c * (F::ONE - self.p608);
                255 - (self.p608 * f(255.0)).0 as i32
            } else if self.p5ec == 1 && self.p5f0 == 0 {
                self.p61c = self.p61c * self.p608;
                (self.p608 * f(255.0)).0 as i32
            } else if self.p5ec != 0 && self.p5f0 != 0 {
                self.p61c = F::ZERO;
                0
            } else {
                255
            };
            self.overcast = (alpha != 0) as i32;
            let fa = F::<M>::from_int(alpha);
            self.pal_alpha(self.sun, || alpha as u8);
            self.pal_alpha(self.corona, || (255 - (255 - alpha) * 2).max(0) as u8);
            self.pal_alpha(self.moon, || alpha as u8);
            self.sky_scale = fa / f(255.0);

            if self.p5fc > 0 || self.p600 > 0 {
                let lv = [0.0, 0.15, 0.5, 1.0, 0.15, 0.5, 1.0].map(f::<M>);
                let mut st = [0; 2];
                if self.p5fc > 0 {
                    st[0] = 1;
                }
                if self.p5fc >= 4 {
                    st[0] = 2;
                }
                if self.p600 > 0 {
                    st[1] = 1;
                }
                if self.p600 >= 4 {
                    st[1] = 2;
                }
                self.effect_mix = match (st[0], st[1]) {
                    (1, 2) => self.p608,
                    (2, 1) => F::ONE - self.p608,
                    (1, 1) => F::ZERO,
                    (2, 2) => F::ONE,
                    (0, 1) => F::ZERO,
                    (0, _) => F::ONE,
                    (1, _) => F::ZERO,
                    _ => F::ONE,
                };
                let a = lv[self.p5fc.clamp(0, 6) as usize];
                let b = lv[self.p600.clamp(0, 6) as usize];
                self.set_intensity(a + (b - a) * self.p608);
                if self.p1190 == -1 {
                    self.p1190 = snd.play(self.p1188);
                    self.reset_lightning(rand);
                }
                if self.p1194 == -1 {
                    snd.set_pan(self.p1190, -1.0);
                    self.p1194 = snd.play(self.p1188);
                    snd.set_pan(self.p1194, 1.0);
                    snd.set_pitch(self.p1194, 4176);
                }
                let v = (self.intensity * (F::ONE - self.effect_mix) * f(0.5) * vol).0;
                if self.p1190 != 0 {
                    snd.set_volume(self.p1190, v);
                }
                if self.p1194 != 0 {
                    snd.set_volume(self.p1194, v);
                }
            } else {
                self.set_intensity(F::ZERO);
                // `if (m_p1190)`: a stopped handle is -1, which is true
                if self.p1190 != 0 {
                    snd.stop(self.p1190);
                    self.p1190 = -1;
                }
                if self.p1194 != 0 {
                    snd.stop(self.p1194);
                    self.p1194 = -1;
                }
            }

            let p = self.p608;
            let new_tex = self.p5f8;
            if p < f(0.5) {
                if let Some(m) = self.dome() {
                    m.layers[0].blend = 3;
                    if m.n != 3 {
                        m.set_num_layers(3);
                        m.copy_layer(1, 2);
                        m.copy_layer(0, 1);
                        m.layers[1].texture = new_tex;
                    }
                    m.layers[1].color[3] = (p * f(2.0) * f(255.0)).0 as i32 as u8;
                    m.layers[2].color[3] = ((f(0.5) - p) * f(2.0) * f(64.0)).0 as i32 as u8;
                }
            } else if p < F::ONE
                && let Some(m) = self.dome()
            {
                if m.n != 2 {
                    m.copy_layer(1, 0);
                    m.copy_layer(2, 1);
                    m.set_num_layers(2);
                    m.layers[1].texture = new_tex;
                }
                m.layers[1].color[3] = ((p - f(0.5)) * f(2.0) * f(64.0)).0 as i32 as u8;
            }
            if self.p608 == F::ONE {
                if let Some(m) = self.dome() {
                    m.layers[0].texture = new_tex;
                    m.layers[1].texture = new_tex;
                    m.layers[0].color[3] = 255;
                    m.layers[1].color[3] = 64;
                }
                self.p5ec = self.p5f0;
                self.p5f4 = self.p5f8;
                self.p5fc = self.p600;
                self.p608 = F::ZERO;
                self.p60c = self.p610;
                if self.p614 != self.p60c {
                    self.set_default_sky(self.p614, false);
                }
                if self.p604 != self.p5fc {
                    self.set_weather(self.p604, false);
                }
            }
        } else if self.p5ec == 0 {
            self.p61c = night(hour);
            self.sky_scale = F::ONE;
        } else {
            self.p61c = F::ZERO;
            self.sky_scale = F::ZERO;
        }

        if self.intensity != F::ZERO && self.effect_mix < F::ONE {
            if self.p1198 < self.p119c {
                self.p119c = F::ZERO;
                self.p11a0 = f(1.0e-5);
            }
            self.p1198 = self.p1198 - self.p4a8;
            if self.p1198 <= F::ZERO {
                let v = ((F::ONE - self.p11a8) * self.intensity * (F::ONE - self.effect_mix) * f(0.9) * vol).0;
                let h = snd.play(self.p118c);
                snd.set_volume(h, v);
                snd.set_pan(h, -1.0);
                let h = snd.play(self.p118c);
                snd.set_volume(h, v);
                snd.set_pan(h, 1.0);
                snd.set_pitch(h, 2496);
                self.reset_lightning(rand);
            }
            let v = (self.intensity * (F::ONE - self.effect_mix) * f(0.5) * vol).0;
            if self.p1190 != 0 {
                snd.set_volume(self.p1190, v);
            }
            if self.p1194 != 0 {
                snd.set_volume(self.p1194, v);
            }
        } else if self.intensity == F::ZERO {
            self.p11a0 = F::ZERO;
            self.p1198 = F::ZERO;
            self.p119c = F::ZERO;
        }
    }

    /// Drop a weather particle's column to the ground (or a liquid surface
    /// above it) and put it at a random height below the box's top.
    fn drop_particle(&mut self, i: usize, sc: &mut dyn Scene<M>, rand: &mut Rand) {
        let p = self.particles[i].location;
        let mut a = p;
        a.y = a.y + f(5000.0);
        let mut b = p;
        b.y = b.y - f(5000.0);
        a = add3(&a, &self.camera);
        b = add3(&b, &self.camera);
        let mut ray = CollRay::default();
        ray.init(a, b);
        let mut env = EnvRay::default();
        env.init(a, b, 15);
        if sc.collide_ray(&mut ray) {
            let mut h = ray.hit_point();
            self.particles[i].liquid = 0;
            if sc.collide_env(&mut env) {
                let e = env.hit_point();
                if e.y > h.y {
                    self.particles[i].liquid = 1;
                    h = e;
                }
            }
            self.particles[i].ground = h.y;
            self.particles[i].location.y = h.y - self.camera.y;
            let half = self.range * f(0.5);
            self.particles[i].location.y = half + (self.particles[i].location.y - half) * rand255::<M>(rand);
        }
    }

    fn process_weather(&mut self, sc: &mut dyn Scene<M>, sys: &mut System<M>, cam: &Camera<M>, rand: &mut Rand) {
        if F::ZERO < self.lavastorm {
            self.process_lavastorm(sc, sys, cam, rand);
        }
        if self.intensity == F::ZERO {
            return;
        }
        let c = Vect3::new(cam.dir.x * f(5.0), cam.dir.y * f(5.0), cam.dir.z * f(5.0));
        let mut d = add3(&c, &cam.position);
        let a = d;
        d -= self.camera;
        let n = (self.intensity * F::from_int(self.num_particles as i32)).0 as i32;
        let k = (F::<M>::from_int(n) * self.effect_mix).0 as i32;
        self.camera = a;
        if self.range < d.x || self.range < d.y || self.range < d.z {
            d = Vect3::default();
            for i in 0..self.num_particles {
                self.drop_particle(i, sc, rand);
            }
        }
        let mut j = self.p84.0 as i32;
        self.p84 = self.p84 + self.p4a8 * f(10.0);
        let rate = self.p4a8 / f(1.0 / 60.0);
        if f(64.0) < self.p84 {
            self.p84 = self.p84 - f(64.0);
        }
        let half = self.range * f(0.5);
        for i in 0..self.num_particles {
            let sel = ((i as i32) < k) as usize;
            let w = self.pa8[((j + i as i32) & 0x3F) as usize];
            let p = &mut self.particles[i].location;
            p.x = p.x + (w.x * self.p50[sel].x + self.p68[sel].x) * rate;
            p.y = p.y + (w.y * self.p50[sel].y + self.p68[sel].y * self.p3a8[i & 0x3F]) * rate;
            p.z = p.z + (w.z * self.p50[sel].z + self.p68[sel].z) * rate;
            p.y = p.y - d.y;
            let mut wrapped = false;
            if p.y < -half {
                p.y = p.y + self.range;
            } else if half <= p.y {
                p.y = p.y - self.range;
            }
            p.x = p.x - d.x;
            p.z = p.z - d.z;
            if p.x < -half {
                wrapped = true;
                p.x = p.x + self.range;
            } else if half <= p.x {
                wrapped = true;
                p.x = p.x - self.range;
            }
            if p.z < -half {
                wrapped = true;
                p.z = p.z + self.range;
            } else if half <= p.z {
                wrapped = true;
                p.z = p.z - self.range;
            }
            if wrapped {
                self.drop_particle(i, sc, rand);
            }
            let wp = self.particles[i];
            if wp.location.y + self.camera.y < wp.ground {
                if sel == 0 && (i as i32) < n {
                    let at = Vect3::new(wp.location.x + self.camera.x, wp.ground + f(0.08), wp.location.z + self.camera.z);
                    self.add_splash(at, rand);
                }
                if wp.liquid == 1 {
                    let r = self.range;
                    self.particles[i].location.x = rand255::<M>(rand) * r - r * f(0.5);
                    self.particles[i].location.z = rand255::<M>(rand) * r - r * f(0.5);
                }
                self.particles[i].location.y = vmax(wp.ground - self.camera.y, half);
            }
            j += 29;
        }
    }

    fn process_lavastorm(&mut self, sc: &mut dyn Scene<M>, sys: &mut System<M>, cam: &Camera<M>, rand: &mut Rand) {
        if (self.fireball_list.len() as i32) < (self.lavastorm * f(32.0)).0 as i32 {
            let mut a = Vect3::new(virand::<M>(rand) * f(50.0), f(5000.0), virand::<M>(rand) * f(50.0));
            if (a.x.0 as f64).abs() < 10.0 && (a.z.0 as f64).abs() < 10.0 {
                a.x = a.x + if a.x < F::ZERO { f(-30.0) } else { f(30.0) };
                a.z = a.z + if a.z < F::ZERO { f(-30.0) } else { f(30.0) };
            }
            let cp = cam.position;
            a = Vect3::new(a.x + cp.x, a.y + cp.y, a.z + cp.z);
            let mut b = a;
            b.y = b.y - f(10000.0);
            let mut ray = CollRay::default();
            ray.init(a, b);
            let mut env = EnvRay::default();
            env.init(a, b, 15);
            if sc.collide_ray(&mut ray) {
                let mut h = ray.hit_point();
                if sc.collide_env(&mut env) {
                    let e = env.hit_point();
                    if h.y < e.y {
                        h = e;
                    }
                }
                if !self.fireball_free.is_empty() {
                    let k = self.fireball_free.remove(0);
                    self.fireball_list.push(k);
                    let mut fb = Fireball { to: h, from: h, t: F::ZERO, ..Default::default() };
                    fb.from.y = fb.from.y + f(40.0);
                    fb.speed = (rand255::<M>(rand) * f(0.6) + f(0.7)) * f(2.5);
                    fb.landed = 0;
                    fb.e0 = sys.create_emitter(self.p1180 as usize);
                    sys.emitter(fb.e0).set_location(fb.from);
                    sys.emitter(fb.e0).set_location(fb.from);
                    fb.e1 = sys.create_emitter(self.p1184 as usize);
                    sys.emitter(fb.e1).set_location(fb.to);
                    sys.emitter(fb.e1).set_location(fb.to);
                    self.fireballs[k] = fb;
                }
            }
        }
        let list = self.fireball_list.clone();
        for k in list {
            let dt = sys.delta_time;
            let fb = &mut self.fireballs[k];
            fb.t = fb.t + dt * f(0.15) * fb.speed;
            if F::ONE <= fb.t {
                if fb.landed == 0 {
                    fb.landed = 1;
                    sys.update(fb.e1, rand);
                } else if sys.emitter(fb.e1).particles.is_empty() {
                    sys.destroy_emitter(fb.e0);
                    sys.destroy_emitter(fb.e1);
                    self.fireball_list.retain(|&x| x != k);
                    self.fireball_free.push(k);
                }
            } else {
                let mut d = Vect3::new(fb.to.x - fb.from.x, fb.to.y - fb.from.y, fb.to.z - fb.from.z);
                d *= fb.t;
                d += fb.from;
                let e0 = fb.e0;
                sys.emitter(e0).set_location(d);
                sys.update(e0, rand);
            }
        }
    }

    /// `SetSkyColor`: every layer of the dome's palette takes the colour,
    /// keeping its alpha.
    pub fn set_sky_color(&mut self, c: [u8; 3]) {
        if !self.initialised {
            return;
        }
        if let Some(p) = self.pals.get_mut(&self.sky_sprite) {
            for e in p.iter_mut() {
                let n = e.n;
                for l in e.layers[..n].iter_mut() {
                    l.color = [c[0], c[1], c[2], l.color[3]];
                }
            }
        }
    }

    /// `RenderSky`: the dome at the view's rotation; under a clear sky on
    /// plane 0 the sun, corona and moon at theirs; then the stars and the
    /// lightning patches.
    pub fn render_sky(&mut self, cam: &Camera<M>, r: &mut Raster<M>, rand: &mut Rand) {
        if !self.initialised {
            return;
        }
        let mut mv = cam.view;
        mv.m[3].x = F::ZERO;
        mv.m[3].y = F::ZERO;
        mv.m[3].z = F::ZERO;
        r.sprites.push((self.sky_sprite, mv));
        if self.p618 == 0 && self.overcast != 0 {
            r.sprites.push((self.sun, Matrix44::mul(&self.sky_rot, &mv)));
            r.sprites.push((self.corona, Matrix44::mul(&self.corona_rot, &mv)));
            r.sprites.push((self.moon, Matrix44::mul(&self.moon_rot, &mv)));
        }
        if self.p618 == 0 {
            self.render_stars(r, rand);
        }
    }

    fn render_stars(&mut self, r: &mut Raster<M>, rand: &mut Rand) {
        let one = |b: F<M>| Color { r: b, g: b, b, a: F::ONE };
        if F::ZERO < self.p61c {
            r.fog_off += 1;
            r.material(self.star_mat);
            if self.pf28 <= F::ZERO {
                if self.pf2c == F::ZERO {
                    let a = rand255::<M>(rand) * f(3.14159274) * f(3.9);
                    self.pf30 = Vect3::new(a.sinf() * f(100.0), f(100.0), a.cosf() * f(100.0));
                    let a = rand255::<M>(rand) * f(3.14159274) * f(3.9);
                    self.pf3c = Vect3::new(a.sinf() * f(100.0), F::ZERO, a.cosf() * f(100.0));
                    self.pf2c = f(1.0e-05);
                }
                if F::ZERO < self.pf2c {
                    self.pf2c = self.pf2c + self.p4a8 * f(0.2);
                    let mut b = virand_div::<M>(rand) * f(0.3) + f(0.7);
                    if self.pf2c < f(0.2) {
                        b = b * (self.pf2c * f(5.0));
                    } else {
                        b = b * (F::ONE - (self.pf2c - f(0.2)) * f(1.25));
                    }
                    b = vmin(vmax(F::ZERO, b), F::ONE);
                    let mut loc = self.pf3c;
                    loc -= self.pf30;
                    loc *= self.pf2c;
                    loc += self.pf30;
                    r.bb(Billboard { location: loc, frame: 0, width: f(0.5), height: f(0.5), color: one(b) });
                    if F::ONE <= self.pf2c {
                        self.pf2c = F::ZERO;
                        self.pf28 = F::from_int((rand.rand() & 0xF) + 10);
                    }
                }
            } else {
                self.pf28 = self.pf28 - self.p4a8;
            }
            let idx = (self.pf24.0 as i32) as i8 as i32;
            self.pf24 = self.pf24 + self.p4a8 * f(15.0);
            if f(32.0) < self.pf24 {
                self.pf24 = self.pf24 - f(32.0);
            }
            for i in 0..128 {
                let b = self.pea0[((idx + self.pe20[i] as i32) & 0x1F) as usize] * self.p61c;
                let s = self.stars[i];
                r.bb(Billboard { location: Vect3::new(s.x, s.y, s.z), frame: 0, width: s.w, height: s.w, color: one(b) });
            }
        }
        if F::ZERO < self.pf48 {
            r.fog_off += 1;
            r.material(self.pf50);
            for i in 0..14 {
                let p = self.patches[i];
                let mut b = self.pf48 * f(0.45);
                b = vmin(b, f(0.15));
                r.bb(Billboard { location: Vect3::new(p.x, p.y, p.z), frame: 0, width: p.w, height: p.w, color: one(b) });
            }
        }
    }

    /// `RenderWeather`: the particles at the camera's box (a particle closer
    /// than 1.5 behind the near plane is a zero-sized quad), rain and snow
    /// split by the effect mix, each with its material and size; then the
    /// splashes.
    pub fn render_weather(&mut self, cam: &Camera<M>, r: &mut Raster<M>) {
        if !self.initialised || self.intensity == F::ZERO {
            return;
        }
        let np = self.num_particles as i32;
        let mut n = (self.intensity * F::from_int(np)).0 as i32;
        n = n.min(np).max(0);
        let k = (F::<M>::from_int(n) * self.effect_mix).0 as i32;
        let pl = cam.near;
        let v = pl.n;
        let mut w = Vect3::new(pl.n.x, F::ZERO, pl.n.z);
        w.normalize();
        let aspect = F::ONE - (v.x * w.x + v.y * w.y + v.z * w.z);
        let run = |a: &Self, r: &mut Raster<M>, mat: i32, sz_i: usize, var_i: usize, first: i32, last: i32| {
            r.material(mat);
            let sz = a.p88[sz_i] * a.particle_scale;
            let wide = sz - sz * a.p90[var_i] * aspect;
            let high = sz - sz * a.p98[var_i] * aspect;
            for j in first..last {
                let p = &a.particles[j as usize];
                let loc = add3(&p.location, &a.camera);
                let (mut ww, mut hh) = (wide, high);
                if f(-1.5) < pl.distance(&loc) {
                    ww = F::ZERO;
                    hh = F::ZERO;
                }
                r.bb(Billboard { location: loc, frame: 0, width: ww, height: hh, color: p.color });
            }
        };
        if k > 0 && k < n {
            for i in 0..2usize {
                let (first, last) = if i == 1 { (k, n) } else { (0, k) };
                run(self, r, self.mat[1 - i], 1 - i, i, first, last);
            }
        } else {
            let s = (k != 0) as usize;
            run(self, r, self.mat[s], s, s, 0, n);
        }
        self.render_splashes(r);
    }

    fn render_splashes(&mut self, r: &mut Raster<M>) {
        r.material(self.splash_mat);
        let list = self.splash_list.clone();
        for n in list {
            let s = &mut self.pool_splash[n];
            r.bb(Billboard {
                location: s.location,
                frame: (s.frame + 1) * 4,
                width: f(0.08),
                height: f(0.08),
                color: Color { r: s.bright, g: s.bright, b: s.bright, a: s.bright },
            });
            s.frame += 1;
            if s.frame == 4 {
                self.splash_list.retain(|&x| x != n);
                self.splash_free.push(n);
            }
        }
    }
}
