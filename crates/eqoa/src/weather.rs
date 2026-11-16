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
//!
//! Not the game's: a server's regional weather (`weather_zones`, board entry
//! #1550), sampled at the camera each frame into `regional` (Options' Weather
//! applied): the sand's blowing dust (`sanddust`, drawn as strips),
//! the roof over the eye and a wall upwind (probed every 100 ms), and the
//! wind's bed on the sound. Its fog is `viewer::lit`'s.
//! `EQOA_WEATHER_PREVIEW=kind[,strength]` (as `/weatherpreview`'s words, a
//! comma for the space) previews one with no server, for shots.

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
    /// `CalcOutdoorsIntensity`'s probe
    pub outdoors: vi::world::Outdoors<Ee>,
    /// The server's regional weather at the camera this frame, through
    /// Options' Weather (`weather_zones`; `set_regional`)
    pub regional: crate::weather_zones::Sample,
    /// `EQOA_WEATHER_PREVIEW`'s, standing in when no server's is given
    preview: Option<crate::weather_zones::Preview>,
    /// a roof within 120 over the eye, and a wall within 8 upwind of it,
    /// probed every 100 ms while there is regional weather
    pub roof: bool,
    pub upwind_wall: bool,
    /// the sandstorm's blowing dust (`sanddust`)
    pub dust: crate::sanddust::Dust,
    probed: Option<i64>,
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
                // each layer its own UV mapping (`Layer::uv`) as made
                let pal = m
                    .batches
                    .iter()
                    .map(|b| Material::new(&b.layers.iter().enumerate().map(|(j, l)| atmosphere::Layer { texture: l.texture, blend: l.blend, color: l.color, uv: j as i32 }).collect::<Vec<_>>()))
                    .collect();
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
        let preview = std::env::var("EQOA_WEATHER_PREVIEW").ok().and_then(|v| crate::weather_zones::Preview::parse(&v.replace(',', " "))).flatten();
        let mut w = Weather {
            atm,
            rand,
            sys,
            tex: MATERIALS.map(|m| id(m.0)),
            first: true,
            sounds_attached: false,
            silent_handles: 0,
            flash: None,
            last: time,
            offset: 0,
            outdoors: Default::default(),
            regional: Default::default(),
            preview,
            roof: false,
            upwind_wall: false,
            dust: Default::default(),
            probed: None,
        };
        if let Some(b) = preview.and_then(|p| p.retail_byte()) {
            w.atm.set_weather(atmosphere::weather_from_wire(b), true);
        }
        if let Ok(v) = std::env::var("EQOA_WEATHER") {
            let mut it = v.split(',');
            let t = it.next().and_then(|t| t.trim().parse().ok()).unwrap_or(0);
            let fade = it.next().is_some_and(|f| f.trim() == "fade");
            w.atm.set_weather(t, !fade);
        }
        w
    }

    /// This frame's regional weather: the server's sample at the camera
    /// (`Play::weather_sample`), else `EQOA_WEATHER_PREVIEW`'s, else none;
    /// through Options' Weather.
    pub fn set_regional(&mut self, server: Option<crate::weather_zones::Sample>, now_ms: i64) {
        let s = server.or_else(|| self.preview.map(|p| p.sample(now_ms))).unwrap_or_default();
        self.regional = s.scaled(crate::settings::client().weather);
    }

    /// A weather byte off the wire (`VIClient::SetWeather`).
    pub fn set_weather(&mut self, w: u8, instant: bool) {
        self.atm.set_weather(atmosphere::weather_from_wire(w), instant);
    }

    /// `VIScene::SetSky(cam, 1)`: the sky of the profiles where `eye` stands,
    /// at once, as `Respawn` and `Msg000D` set it.
    pub fn sky_now(&mut self, world: &World<Ee>, eye: &vi::camera::Eye<Ee>) {
        if let Some(g) = &world.grid {
            self.atm.set_sky(&g.profile_blend(&eye.position), true);
            self.first = false;
        }
    }

    /// The atmosphere's orders as the messages gave them (`Play::weather`),
    /// taken in their order: each weather byte, and each sky set at once
    /// where the player now stands. While another world is still to be
    /// entered (`pending`) a sky's order waits, and every order after it, for
    /// that world, as the console sets it only once `EnterWorld` has read it.
    pub fn take(&mut self, ops: &mut Vec<crate::play::Atmos>, world: &World<Ee>, eye: &vi::camera::Eye<Ee>, pending: bool) {
        use crate::play::Atmos;
        let mut done = 0;
        for &op in ops.iter() {
            match op {
                Atmos::Weather(w, instant) => self.set_weather(w, instant),
                Atmos::SkyNow if pending => break,
                Atmos::SkyNow => self.sky_now(world, eye),
            }
            done += 1;
        }
        ops.drain(..done);
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
            + &format!(", dust {} grains {} wisps, ground {:?}", self.dust.grain_count, self.dust.wisp_count, self.dust.ground)
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
        // CalcOutdoorsIntensity, ahead of the atmosphere's Update
        let o = self.outdoors.update(world, &eye.position, time);
        let mut sound = sound;
        if !self.sounds_attached
            && let Some(s) = sound.as_deref()
        {
            self.atm.p1188 = s.sound_named("rain_lt");
            self.atm.p118c = s.sound_named("lightning_far02");
            self.sounds_attached = true;
        }
        // the regional weather's roof and wall, every 100 ms while there is any
        let r = self.regional;
        if r.intensity() <= 0.0 {
            (self.roof, self.upwind_wall, self.probed, self.dust.ground) = (false, false, None, None);
        } else if self.probed.is_none_or(|t| time - t >= 100 || time < t) {
            self.probed = Some(time);
            let p = eye.position;
            let mut up = CollRay::default();
            up.init(p, Vect3::new(p.x, p.y + F::new(120.0), p.z));
            self.roof = atmosphere::Scene::collide_ray(&mut Rays(world), &mut up);
            // the ground under the eye, which the dust skims
            let mut down = CollRay::default();
            down.init(p, Vect3::new(p.x, p.y - F::new(200.0), p.z));
            self.dust.ground = atmosphere::Scene::collide_ray(&mut Rays(world), &mut down).then(|| down.hit_point().y.0);
            let len = r.wind[0].hypot(r.wind[1]);
            self.upwind_wall = len > 0.01 && {
                let mut side = CollRay::default();
                side.init(p, Vect3::new(p.x - F::new(r.wind[0] / len * 8.0), p.y, p.z - F::new(r.wind[1] / len * 8.0)));
                atmosphere::Scene::collide_ray(&mut Rays(world), &mut side)
            };
        }
        if let Some(s) = sound.as_deref_mut() {
            s.weather = self.atm.intensity;
            s.outdoors = o;
            let fx = s.dev.sound_class_volume(0);
            s.set_wind(crate::weather_zones::wind_target(&r, fx, self.roof || self.upwind_wall));
        }
        let cam = camera(eye);
        let mut voices = Voices(sound, &mut self.silent_handles);
        self.atm.update(&mut Rays(world), &mut voices, &mut self.sys, &cam, time, F::new(hour), o, &mut self.rand);
        // the sand's drops on the sampled sand and wind (none at 0)
        let (e, d) = (eye.position, eye.direction);
        self.dust.update([e.x.0, e.y.0, e.z.0], [d.x.0, d.y.0, d.z.0], r.sand, r.wind, time);
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
        let mut out = self.batches(&world, crate::settings::client().weather);
        for (id, bbs) in self.sys.render(time) {
            let def = self.sys.emitter(id).def;
            let Some(d) = self.sys.defs[def].as_ref() else { continue };
            out.push(Batch { texture: d.texture as i32, blend: d.blend.clamp(0, 4) as usize, ztest: d.ztest, zwrite: d.zwrite, quads: quads(&bbs) });
        }
        (self.batches(&sky, crate::settings::SHARE_FULL), out)
    }

    /// The sandstorm's dust as strips seen from `eye`, in `color` (the
    /// storm's fog colour): for the renderer's strips, after the frame's own.
    pub fn dust_strips(&self, eye: glam::Vec3, color: [f32; 3]) -> Vec<render::billboard::Strips> {
        self.dust.strips(eye.to_array(), color, self.tex[1])
    }

    /// The raster's runs as batches, through the options' Weather at `share`
    /// (`weathered`).
    fn batches(&self, r: &Raster<Ee>, share: u8) -> Vec<Batch> {
        r.runs
            .iter()
            .filter(|(_, b)| !b.is_empty())
            .map(|(m, bbs)| {
                let k = (*m).clamp(0, MATERIALS.len() as i32 - 1) as usize;
                Batch { texture: self.tex[k], blend: MATERIALS[k].1, ztest: MATERIALS[k].2, zwrite: false, quads: weathered(quads(bbs), k, share) }
            })
            .filter(|b| !b.quads.is_empty())
            .collect()
    }
}

/// A run of material `k`'s billboards (`MATERIALS`) as the options' Weather
/// draws it at `share`: of the rain's and the snow's drops (0, 1) that share,
/// each by its place in the run, which is its particle's (`RenderWeather`
/// draws every particle, in order, at every frame); the splashes (2) that much
/// as bright (an added blend); the stars and the lightning (3, 4), and every
/// run at all of it, as they are: the game's.
fn weathered(mut q: Vec<Quad>, k: usize, share: u8) -> Vec<Quad> {
    if share >= crate::settings::SHARE_FULL {
        return q;
    }
    match k {
        0 | 1 => {
            let mut at = 0u64;
            q.retain(|_| {
                at += 1;
                crate::settings::share_keeps(at, share)
            });
        }
        2 => q.iter_mut().for_each(|b| b.color[3] *= share as f32 / crate::settings::SHARE_FULL as f32),
        _ => {}
    }
    q
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::Atmos;
    use vi::camera::Eye;

    /// The library with the sky (and FX.CSF, so `new` reads nothing off a
    /// disc of its own) and Tunaria round Freeport's shore, and that world;
    /// None without `run/eqoa.iso`.
    fn sky_and_world() -> Option<(Library, World<Ee>)> {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return None;
        }
        let mut lib = Library::new();
        let w = lib.add("TUNARIA.ESF", crate::stream::open_file(&iso, "TUNARIA.ESF").unwrap());
        let mut disc = iso::Iso::open(&iso).unwrap();
        for f in ["DATA2/SKY.CSF", "DATA2/FX.CSF"] {
            lib.add(f, esf::Container::open(disc.read(f).unwrap()).unwrap());
        }
        let mut src = crate::stream::FileZones::open(&iso, "TUNARIA.ESF").unwrap();
        crate::stream::fill(&mut lib, w, 25275.0, 15766.0, &mut src);
        let world = World::load(&lib, w)?;
        Some((lib, world))
    }

    /// Weather at all of it is every billboard as the raster gave it; below,
    /// fewer drops of rain and of snow (the same ones at each frame: by their
    /// place) and fainter splashes; the stars and the lightning kept whole.
    /// The control: a drop list at half is not the whole list.
    #[test]
    fn weather_keeps_its_share_of_the_drops() {
        let drops: Vec<Quad> = (0..1000).map(|k| Quad { center: [k as f32, 0.0, 0.0], half: [0.1, 0.4], frame: 0, color: [1.0, 1.0, 1.0, 0.5] }).collect();
        let xs = |v: &[Quad]| v.iter().map(|q| q.center[0].to_bits()).collect::<Vec<_>>();
        for k in 0..5 {
            let w = weathered(drops.clone(), k, 100);
            assert_eq!((xs(&w), w.iter().map(|q| q.color[3]).collect::<Vec<_>>()), (xs(&drops), vec![0.5; 1000]), "material {k} at all of it");
        }
        for k in [0, 1] {
            let half = weathered(drops.clone(), k, 50);
            assert!(half.len() > 400 && half.len() < 600, "the control: {}", half.len());
            assert_eq!(xs(&half), xs(&weathered(drops.clone(), k, 50)), "the same drops each frame");
            assert!(weathered(drops.clone(), k, 0).is_empty(), "none at 0");
        }
        let splash = weathered(drops.clone(), 2, 30);
        assert_eq!((splash.len(), splash[0].color[3]), (1000, 0.5 * 0.3), "the splashes fainter");
        for k in [3, 4] {
            assert_eq!(xs(&weathered(drops.clone(), k, 0)), xs(&drops), "the sky's own, kept");
        }
    }

    fn eye() -> Eye<Ee> {
        Eye { position: Vect3::of(25275.0, 60.0, 15766.0), direction: Vect3::of(0.0, 0.0, 1.0) }
    }

    /// A port's sky (`Respawn`'s `SetSky(m_camera, 1)`) changes at once:
    /// the change under way is done at the next step, so the weather the
    /// server sends next fades in from there. The control: the sky the frame
    /// sets (fade 0), which is all a port had: a change faded over 30 s,
    /// behind which that weather waits.
    #[test]
    fn a_ports_sky_is_set_at_once_and_the_weather_after_it_is_not_held() {
        let Some((mut lib, world)) = sky_and_world() else { return };
        let mut now = Weather::new(&mut lib, 0);
        let mut faded = Weather::new(&mut lib, 0);
        assert!(now.atm.initialised, "the sky's sprites are in");
        // a sky none of the profiles here has, shown and last seen, as after
        // a port from afar
        for w in [&mut now, &mut faded] {
            w.atm.p60c = 9;
            w.atm.seen_sky = 9;
        }
        now.take(&mut vec![Atmos::SkyNow, Atmos::Weather(2, false)], &world, &eye(), false);
        assert_eq!(now.atm.p608, F::ONE, "the sky at once");
        assert_eq!(now.atm.p604, 2, "the weather after it is the next change");
        let b = world.grid.as_ref().unwrap().profile_blend(&eye().position);
        faded.atm.set_sky(&b, false);
        faded.set_weather(2, false);
        assert!(faded.atm.p608 > F::ZERO && faded.atm.p608 < F::ONE, "the control: the sky fades");
        assert_eq!((faded.atm.p604, faded.atm.p600), (2, 0), "the control: the weather waits behind it");
    }

    /// The orders go in the messages' order; while another world is still
    /// to be entered a sky's order, and every order after it, waits for it.
    /// The control: with no world to wait for, all of them are taken.
    #[test]
    fn the_orders_after_a_ports_sky_wait_for_its_world() {
        let Some((mut lib, world)) = sky_and_world() else { return };
        let mut w = Weather::new(&mut lib, 0);
        let mut ops = vec![Atmos::Weather(1, true), Atmos::SkyNow, Atmos::Weather(0, false)];
        w.take(&mut ops, &world, &eye(), true);
        assert_eq!(ops, [Atmos::SkyNow, Atmos::Weather(0, false)], "the sky and what follows it wait");
        assert_eq!((w.atm.p600, w.atm.p608), (1, F::ONE), "the weather before it is taken, at once");
        w.take(&mut ops, &world, &eye(), false);
        assert!(ops.is_empty(), "the control: the world entered, all are taken");
        assert_eq!(w.atm.p604, 0, "the last weather is the next change");
    }
}
