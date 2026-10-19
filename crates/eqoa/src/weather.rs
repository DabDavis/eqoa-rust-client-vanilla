//! The weather and the sky in the client: `vi::atmosphere` driven as the
//! game drives it each frame (`VIClient::Process`: `SetSky`, then
//! `VIScene::Render`: the atmosphere's `Update`, the lights' `GetLightning`,
//! `SetSkyColor`, `SetLavastormIntensity`, `RenderSky`, `RenderWeather`),
//! its sky materials onto the sky's models (`sky.rs`), its billboards into
//! `render::billboard` (the stars and lightning patches as the sky's, the
//! rain, snow, splashes and the Lavastorm's fireballs in the world), and the
//! rain and thunder on the sound device. The server sets the weather
//! (0x00C5 fades, 0x00C7 is instant); `EQOA_WEATHER=t[,fade]` sets it at
//! the start (0 clear, 1-3 rain light to heavy, 4-6 snow light to heavy; see
//! `vi::atmosphere::WX_CLEAR`), instant unless `fade`. The Lavastorm comes
//! with the terrain (its profiles' weight), not with the weather.
//!
//! What differs from the game: the Lavastorm's fireballs ride a particle
//! system of their own rather than the scene's.

use crate::sky::Sprites;
use esf::Library;
use render::billboard::{Batch, Quad};
use vi::atmosphere::{self, Atmosphere, Camera, Material, Raster};
use vi::coll::{CollRay, EnvRay};
use vi::csprite::Rand;
use vi::fpu::{Ee, F};
use vi::geom::{Matrix44, Plane};
use vi::hsprite::hash_ref_id;
use vi::particle::{Billboard, Definition, System};
use vi::scene::{Filter, FilterKind};
use vi::vect::Vect3;
use vi::world::World;

/// The materials `Init` makes, in its order: rain, snow, splash, star,
/// lightning patch; their texture, blend mode and Z test (none writes Z).
const MATERIALS: [(&str, usize, bool); 5] = [("rain.tga", 3, true), ("snow.tga", 3, true), ("splash.tga", 1, true), ("star.tga", 1, false), ("lightning_patch.tga", 1, false)];

/// FX.CSF's Lavastorm: the fireball's head and its impact.
const HEAD: &str = "P:\\studio\\eqps2\\team\\brandon\\head.esf";
const IMPACT: &str = "P:\\studio\\eqps2\\team\\brandon\\impact.esf";

/// The scene, as the weather asks it: everything collides
/// (`WeatherFilter` filters nothing).
struct Rays<'a>(&'a mut World<Ee>);

impl atmosphere::Scene<Ee> for Rays<'_> {
    fn collide_ray(&mut self, r: &mut CollRay<Ee>) -> bool {
        let mut f = Filter::new(FilterKind::Scene, -1);
        self.0.collide_ray(r, &mut f)
    }
    fn collide_env(&mut self, r: &mut EnvRay<Ee>) -> bool {
        self.0.collide_env(r)
    }
}

/// The sound device, as the atmosphere calls it; with none, handles that
/// play nothing (so the atmosphere's state runs as it would).
struct Voices<'a>(Option<&'a mut crate::sound::Sound>, &'a mut i32);

impl atmosphere::Sound for Voices<'_> {
    fn play(&mut self, sound: i32) -> i32 {
        match &mut self.0 {
            Some(s) => s.dev.play(sound, 127, 0),
            None => {
                *self.1 += 1;
                *self.1
            }
        }
    }
    fn stop(&mut self, h: i32) {
        if let Some(s) = &mut self.0 {
            s.dev.stop(h);
        }
    }
    fn set_volume(&mut self, h: i32, v: f32) {
        if let Some(s) = &mut self.0 {
            s.dev.set_volume(h, v);
        }
    }
    fn set_pan(&mut self, h: i32, p: f32) {
        if let Some(s) = &mut self.0 {
            s.dev.set_pan(h, p);
        }
    }
    fn set_pitch(&mut self, h: i32, p: i32) {
        if let Some(s) = &mut self.0 {
            s.dev.set_pitch(h, p);
        }
    }
}

pub struct Weather {
    pub atm: Atmosphere<Ee>,
    rand: Rand,
    /// the fireballs' emitters
    sys: System<Ee>,
    tex: [i32; 5],
    /// `SetSky`'s first call fades (instantly), as entering a world does
    first: bool,
    sounds_attached: bool,
    silent_handles: i32,
    /// this frame's lightning flash, when there is one
    pub flash: Option<F<Ee>>,
    /// the scene clock: the caller's, kept from running backwards when the
    /// caller's restarts (a world change), as the game's never does
    last: i64,
    offset: i64,
}

impl Weather {
    /// The atmosphere at scene time `time`, its resources out of SKY.CSF (in
    /// `lib`) and FX.CSF (added to `lib` for the fireballs' textures).
    pub fn new(lib: &mut Library, time: i64) -> Weather {
        let mut rand = Rand(1);
        let mut atm = Atmosphere::<Ee>::new(time, &mut rand);
        let mut sys = System::new(time, 2000);
        let id = |s: &str| hash_ref_id(s);
        if lib.lookup(id(HEAD)).is_none()
            && let Ok(c) = crate::disc(None).and_then(|mut d| Ok(esf::Container::open(d.read("DATA2/FX.CSF")?)?))
        {
            lib.add("DATA2/FX.CSF", c);
        }
        let def = |lib: &Library, name: &str| {
            let r = lib.lookup(id(name))?;
            let c = lib.file(r.file);
            let d = c.child(r.node, 0xC020)?;
            Definition::<Ee>::parse(c.body(d), c.node(d).version as i32)
        };
        let lava = match (def(lib, HEAD), def(lib, IMPACT)) {
            (Some(mut h), Some(i)) => {
                Atmosphere::tune_fireball_head(&mut h);
                (sys.add_definition(h) as i32, sys.add_definition(i) as i32)
            }
            _ => (-1, -1),
        };
        // the sky's sprites: their palettes, a material per batch
        let models = crate::sky::models(lib);
        let has360 = lib.lookup(id("EQOA_Skydome_360")).is_some();
        if let Some(ms) = &models {
            for (k, m) in ms.iter().enumerate() {
                let pal = m.batches.iter().map(|b| Material::new(&b.layers.iter().map(|l| atmosphere::Layer { texture: l.texture, blend: l.blend, color: l.color }).collect::<Vec<_>>())).collect();
                atm.pals.insert(k as i32, pal);
            }
        }
        // SetResources: skyTex[1] is the dome's own first layer
        let own = models.as_ref().and_then(|ms| ms[Sprites::Sky as usize].batches.first()).and_then(|b| b.layers.first()).map_or(-1, |l| l.texture);
        atm.set_resources(
            Sprites::Sun as i32,
            Sprites::Corona as i32,
            Sprites::Moon as i32,
            [Sprites::Sky as i32, if has360 { Sprites::Sky360 as i32 } else { -1 }],
            [id("PS2EQSky_Sunny.tga"), own, id("PS2EQSky_DarkClouds.tga")],
            -1,
            -1,
            lava,
        );
        if models.is_none() {
            atm.initialised = false;
        }
        let mut w = Weather { atm, rand, sys, tex: MATERIALS.map(|m| id(m.0)), first: true, sounds_attached: false, silent_handles: 0, flash: None, last: time, offset: 0 };
        if let Ok(v) = std::env::var("EQOA_WEATHER") {
            let mut it = v.split(',');
            let t = it.next().and_then(|t| t.trim().parse().ok()).unwrap_or(0);
            let fade = it.next().is_some_and(|f| f.trim() == "fade");
            w.atm.set_weather(t, !fade);
        }
        w
    }

    /// A weather byte off the wire (`VIClient::SetWeather`).
    pub fn set_weather(&mut self, w: u8, instant: bool) {
        self.atm.set_weather(atmosphere::weather_from_wire(w), instant);
    }

    pub fn describe(&self) -> String {
        let a = &self.atm;
        format!(
            "weather {} -> {} ({:.2}), intensity {:.2}, mix {:.2}, sky {} -> {}, overcast {:.2}, lavastorm {:.2}, {} splashes, {} fireballs, ground under {} of {} drops, thunder in {:.1} s, flash {:.2}",
            a.p5fc,
            a.p600,
            a.p608.0,
            a.intensity.0,
            a.effect_mix.0,
            a.p60c,
            a.p610,
            a.overcast_interp().0,
            a.lavastorm.0,
            a.num_splashes(),
            a.fireballs().count(),
            a.particles.iter().filter(|p| p.ground != F::ZERO).count(),
            a.particles.len(),
            a.p1198.0,
            a.pf48.0
        ) + &format!(", dome {:?}", a.pals.get(&a.sky_sprite).and_then(|p| p.first()).map(|m| m.used().iter().map(|l| (l.texture, l.blend, l.color)).collect::<Vec<_>>()))
    }

    /// One frame, before the lights: `SetSky` for the camera's terrain
    /// profiles, then `Update` (the clock, the sky's rotations, the weather
    /// field, the transitions and the rain's and thunder's voices). Leaves
    /// the weather's intensity on `sound` for its track (`SetSoundTrack`).
    pub fn frame(&mut self, world: &mut World<Ee>, sound: Option<&mut crate::sound::Sound>, eye: &vi::camera::Eye<Ee>, time: i64, hour: f32) {
        if time + self.offset < self.last {
            self.offset = self.last - time;
        }
        let time = time + self.offset;
        self.last = time;
        if let Some(g) = &world.grid {
            let b = g.profile_blend(&eye.position);
            self.atm.set_sky(&b, self.first);
            self.first = false;
        }
        let mut sound = sound;
        if !self.sounds_attached
            && let Some(s) = sound.as_deref()
        {
            self.atm.p1188 = s.sound_named("rain_lt");
            self.atm.p118c = s.sound_named("lightning_far02");
            self.sounds_attached = true;
        }
        if let Some(s) = sound.as_deref_mut() {
            s.weather = self.atm.intensity;
        }
        let cam = camera(eye);
        let mut voices = Voices(sound, &mut self.silent_handles);
        self.atm.update(&mut Rays(world), &mut voices, &mut self.sys, &cam, time, F::new(hour), F::ONE, &mut self.rand);
    }

    /// The lights' half (`SetupTimeOfDayLights`' `GetLightning`): the flash,
    /// kept for the frame.
    pub fn lightning(&mut self) -> Option<F<Ee>> {
        self.flash = self.atm.get_lightning(&mut self.rand);
        self.flash
    }

    /// The sky's half of the frame (`SetupTimeOfDaySky`): its colour and the
    /// Lavastorm's intensity from the profiles.
    pub fn sky_colour(&mut self, rgb: [u8; 3], lavastorm: F<Ee>) {
        self.atm.set_sky_color(rgb);
        self.atm.set_lavastorm_intensity(lavastorm, &mut self.sys);
    }

    /// `RenderSky` and `RenderWeather`, and the fireballs' particles (at the
    /// last `frame`'s time): (the sky's billboards, drawn behind the world
    /// through the sky's view; the world's).
    pub fn render(&mut self, eye: &vi::camera::Eye<Ee>) -> (Vec<Batch>, Vec<Batch>) {
        let time = self.last;
        let cam = camera(eye);
        let mut sky = Raster::default();
        self.atm.render_sky(&cam, &mut sky, &mut self.rand);
        let mut world = Raster::default();
        self.atm.render_weather(&cam, &mut world);
        let mut out = self.batches(&world);
        for (id, bbs) in self.sys.render(time) {
            let def = self.sys.emitter(id).def;
            let Some(d) = self.sys.defs[def].as_ref() else { continue };
            out.push(Batch { texture: d.texture as i32, blend: d.blend.clamp(0, 4) as usize, ztest: d.ztest, zwrite: d.zwrite, quads: quads(&bbs) });
        }
        (self.batches(&sky), out)
    }

    fn batches(&self, r: &Raster<Ee>) -> Vec<Batch> {
        r.runs
            .iter()
            .filter(|(_, b)| !b.is_empty())
            .map(|(m, bbs)| {
                let k = (*m).clamp(0, 4) as usize;
                Batch { texture: self.tex[k], blend: MATERIALS[k].1, ztest: MATERIALS[k].2, zwrite: false, quads: quads(bbs) }
            })
            .collect()
    }
}

fn quads(bbs: &[Billboard<Ee>]) -> Vec<Quad> {
    bbs.iter()
        .map(|b| Quad { center: [b.location.x.0, b.location.y.0, b.location.z.0], half: [b.width.0, b.height.0], frame: b.frame, color: [b.color.r.0, b.color.g.0, b.color.b.0, b.color.a.0] })
        .collect()
}

/// The camera the atmosphere sees: where the eye is and looks, and the
/// frustum's near plane (normal out of the frustum, 0.3 ahead of the eye,
/// as the client's projection has it). The view matrix only places the sky
/// sprites, which `sky.rs` places itself.
fn camera(eye: &vi::camera::Eye<Ee>) -> Camera<Ee> {
    let mut d = eye.direction;
    d.normalize();
    let n = Vect3::new(-d.x, -d.y, -d.z);
    let p = eye.position;
    let at = (p.x * d.x + p.y * d.y + p.z * d.z) + F::new(0.3);
    Camera { position: p, dir: d, view: Matrix44::identity(), near: Plane { n, d: at } }
}
