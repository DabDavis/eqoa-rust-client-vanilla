//! The viewer, on wgpu + winit: one model at a time (orbit camera) or a zone
//! (fly camera). `shot` renders the same view offscreen to a PNG.
//!
//! Models: drag to orbit, scroll to zoom, ←/→ for the previous/next model.
//! Zones: drag to look, WASD to move, Q/E down/up, Shift ×5.
//! Walking: WASD is the left stick, ←/→ orbit and ↑/↓ zoom (the right stick),
//! drag orbits too. Esc quits.

use crate::scene::{self, Scene};
use crate::stream::{self, Streamer};
use crate::walker::{Controls, Placing, Spawn, Walker};
use esf::{Library, Ref};
use glam::Vec3;
use render::{GpuModel, Renderer, View, wgpu};
use std::collections::HashSet;
use std::sync::Arc;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

type Error = Box<dyn std::error::Error>;

/// The hour of day: `EQOA_HOUR` if set, else 10:00 (InitTimeOfDay's) moving
/// at the game's 24 hours an hour from when the viewer started.
fn hour(start: Instant) -> f32 {
    match std::env::var("EQOA_HOUR").ok().and_then(|h| h.parse::<f32>().ok()) {
        Some(h) => h,
        None => (10.0 + start.elapsed().as_secs_f32() / 150.0) % 24.0,
    }
}

/// The hour of a game: `EQOA_HOUR` if set; else the server's, as
/// `VIClient::UpdateTimeOfDay` keeps it (`Session::time_of_day`: 0x0013's
/// clock B on the local timer); else, before the server set it, `hour`'s.
pub fn game_hour(play: Option<&crate::play::Play>, start: Instant) -> f32 {
    if std::env::var_os("EQOA_HOUR").is_none()
        && let Some(h) = play.and_then(|p| p.session.time_of_day())
    {
        return h.0;
    }
    hour(start)
}

/// A frame's lights, fog and sky out of the time of day, for the camera at
/// `eye` in `world` (its terrain profiles and env flags), if any, under the
/// atmosphere's `weather`: its overcast (scaled by the profiles' for the
/// lights), its lightning flash, and the sky colour and Lavastorm handed back
/// to it.
pub fn lit(
    proj: glam::Mat4,
    view: glam::Mat4,
    hour_now: f32,
    eye: Option<(Vec3, &vi::world::World<vi::fpu::Ee>)>,
    mut weather: Option<&mut crate::weather::Weather>,
) -> (View, Option<crate::sky::SkyFrame>) {
    let view_proj = proj * view;
    let mut rot = view;
    rot.w_axis = glam::Vec4::W;
    use vi::fpu::{Ee, F};
    let tod = vi::tod::TimeOfDay::<Ee>::new();
    let hour = F::new(hour_now);
    let blend = eye.and_then(|(at, w)| w.grid.as_ref().map(|g| (g.profile_blend(&vi::vect::Vect3::of(at.x, at.y, at.z)), at, w)));
    let o = weather.as_deref().map_or(F::ZERO, |w| w.atm.overcast_interp());
    let k = blend.as_ref().map_or(F::ZERO, |(b, _, _)| vi::tod::overcast_weight(o, b));
    let (amb, mut sun, mut dir) = tod.lights(hour, k);
    let outdoors = weather.as_deref().map_or(F::ONE, |w| w.outdoors.intensity);
    if let Some(flash) = weather.as_deref_mut().and_then(|w| w.lightning()) {
        (sun, dir) = tod.flash(&sun, flash, outdoors);
    }
    let rgb = |c: [F<Ee>; 3]| Vec3::new(c[0].0, c[1].0, c[2].0);
    let mut v = View {
        view_proj,
        // the camera, which the models are drawn relative to
        eye: view.inverse().w_axis.truncate(),
        sun_dir: Vec3::from(dir.to_array()),
        sun: rgb(sun),
        ambient: rgb(amb),
        fog: None,
        point: None,
        clear: Vec3::new(0.18, 0.2, 0.24),
        sky_view_proj: None,
    };
    let mut sky = None;
    if let Some((b, at, w)) = &blend {
        let at = vi::vect::Vect3::of(at.x, at.y, at.z);
        let flags = w.env_flags(&at);
        let fog = tod.fog(hour, o, b, flags, &sun);
        let color = Vec3::new(fog.color[0].0, fog.color[1].0, fog.color[2].0);
        // the distance fog through the Extended page's Fog; inside a liquid
        // (env flags 1, 2, 4, 8) the game's own
        v.fog = if flags & 0xF != 0 {
            Some((fog.near.0, fog.far.0, color))
        } else {
            crate::fog::distances(crate::settings::fog(), fog.near.0, fog.far.0).map(|(n, f)| (n, f, color))
        };
        let c = tod.sky(hour, o, b);
        v.clear = Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32) / 255.0;
        // the sky through a far plane of its own: the world's is the fog's
        v.sky_view_proj = Some(crate::fog::with_far(proj, crate::fog::SKY_FAR) * rot);
        if let Some(wx) = weather {
            wx.sky_colour(c, vi::tod::lavastorm_weight(b));
            let a = &wx.atm;
            sky = Some(crate::sky::SkyFrame {
                plane: a.p618,
                lights: a.p618 == 0 && a.overcast != 0,
                rot: vi::atmosphere::SkyRotations { sun: a.sky_rot, corona: a.corona_rot, moon: a.moon_rot },
                pals: (0..5).map(|k| a.pals.get(&k).cloned().unwrap_or_default()).collect(),
            });
        }
    }
    (v, sky)
}

/// What is on screen.
pub enum Source {
    Models { list: Vec<(i32, Ref)>, index: usize },
    Zone { file: usize, zone: esf::NodeId, index: usize },
    /// A zone with the character in it, walking: at `spawn` when a server
    /// gave the place.
    Walk { file: usize, index: usize, character: Ref, spawn: Option<Spawn> },
}

/// The human (`claude-notes`: model ids by race; the wire value is the DictID).
pub const HUMAN: i32 = 0x70D898C6;

impl Source {
    pub fn scene(&self, lib: &Library) -> Scene {
        match self {
            Source::Models { list, index, .. } => {
                let (id, r) = list[*index];
                scene::model(id, lib.build_model(r).unwrap_or_default())
            }
            Source::Zone { file, zone, index } => scene::zone(lib, *file, *zone, *index),
            // Only for the spawn point: a walk's zones are streamed. (A world
            // left on the disc must have this zone in already.)
            Source::Walk { file, index, .. } => match lib.zone(*file, *index) {
                Some((zf, z)) => scene::zone(lib, zf, z, *index),
                None => scene::zones(lib, *file, &[], &format!("zone {index} (not in)"), false),
            },
        }
    }

    /// For a walk: the character in the middle of the zone, on the game's own
    /// collision world (a world left on the disc has the zones round the
    /// spawn read first, `stream::fill`; one read whole has every zone), and
    /// a streamer for the zones to read and draw.
    pub fn walker(&self, lib: &mut Library, s: &Scene) -> Option<(Walker, esf::Model, Streamer)> {
        let Source::Walk { file, character, spawn: given, .. } = self else { return None };
        // `EQOA_SPAWN=x,z[,heading[,y]]` picks the spot; else the server's
        // place, else near the middle. With a height the character stands
        // there as `Respawn` places it (on the floor within 3 of it, not the
        // first floor a probe from above meets: Neriak's plaza is under a
        // roof).
        let c = (s.bounds.0 + s.bounds.1) * 0.5;
        let spawn: Vec<f32> = std::env::var("EQOA_SPAWN")
            .ok()
            .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect())
            .unwrap_or_default();
        let spawn = match (spawn.as_slice(), given) {
            ([x, z], _) => Spawn::ground(*x, *z, 0.0),
            ([x, z, h], _) => Spawn::ground(*x, *z, *h),
            ([x, z, h, y, ..], _) => Spawn { pos: [*x, *y, *z], heading: *h, placing: Placing::Respawn },
            (_, Some(at)) => *at,
            _ => Spawn::ground(c.x - 40.0, c.z + 40.0, 0.0),
        };
        let [x, _, z] = spawn.pos;
        let mut source = stream::source_for(lib, *file);
        let mut pending = Vec::new();
        if let Some(src) = source.as_mut() {
            let t = Instant::now();
            pending = stream::fill(lib, *file, x, z, src.as_mut());
            eprintln!("zones round the spawn read in {:.2?}{}", t.elapsed(), if pending.is_empty() { String::new() } else { format!(", {} still coming", pending.len()) });
        }
        let t = Instant::now();
        let world = vi::world::World::load(lib, *file)?;
        eprintln!(
            "collision world: {} zones ({} in), {} buffers, {} actors, in {:.2?}",
            world.zones.len(),
            world.loaded.iter().filter(|&&l| l).count(),
            world.buffers.len(),
            world.actors.len(),
            t.elapsed()
        );
        let mut w = Walker::arrive(world, spawn);
        w.set_character(lib, *character);
        Some((w, lib.build_model(*character)?, Streamer::new(*file, source, pending)))
    }

    pub fn camera(&self, bounds: (Vec3, Vec3)) -> Camera {
        match self {
            Source::Models { .. } => Camera::Orbit { yaw: 0.6, pitch: 0.3, zoom: 1.0 },
            // Above the middle of the zone, looking down across it.
            Source::Zone { .. } | Source::Walk { .. } => {
                let c = (bounds.0 + bounds.1) * 0.5;
                Camera::Fly { pos: Vec3::new(c.x, bounds.1.y + 150.0, bounds.0.z - 100.0), yaw: 0.0, pitch: -0.5 }
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Camera {
    Orbit { yaw: f32, pitch: f32, zoom: f32 },
    Fly { pos: Vec3, yaw: f32, pitch: f32 },
}

fn direction(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(yaw.sin() * pitch.cos(), pitch.sin(), yaw.cos() * pitch.cos())
}

impl Camera {
    /// Y is up (checked by eye on the human model, 0x70D898C6).
    pub fn view(&self, bounds: (Vec3, Vec3), aspect: f32) -> View {
        let fov = 45f32.to_radians();
        let (eye, target, near, far) = match *self {
            Camera::Orbit { yaw, pitch, zoom } => {
                let center = (bounds.0 + bounds.1) * 0.5;
                let radius = ((bounds.1 - bounds.0).length() * 0.5).max(0.01);
                let dist = radius * 2.6 * zoom;
                (center + direction(yaw, pitch) * dist, center, dist * 0.01, dist * 10.0)
            }
            Camera::Fly { pos, yaw, pitch } => (pos, pos + direction(yaw, pitch), 0.5, 6000.0),
        };
        let proj = glam::camera::rh::proj::directx::perspective(fov, aspect, near, far);
        lit(proj, glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y), hour(Instant::now()), None, None).0
    }

    fn drag(&mut self, dx: f32, dy: f32) {
        match self {
            Camera::Orbit { yaw, pitch, .. } => {
                *yaw -= dx * 0.01;
                *pitch = (*pitch - dy * 0.01).clamp(-1.5, 1.5);
            }
            Camera::Fly { yaw, pitch, .. } => {
                *yaw -= dx * 0.005;
                *pitch = (*pitch - dy * 0.005).clamp(-1.5, 1.5);
            }
        }
    }
}

fn upload(renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, lib: &Library, s: &Scene) -> Vec<GpuModel> {
    let textures = render::library_textures(lib);
    s.parts
        .iter()
        .zip(&s.statics)
        .map(|((m, mats), st)| {
            let gm = renderer.upload(device, queue, m, mats, &textures);
            renderer.set_static_lights(queue, &gm, st);
            gm
        })
        .collect()
}

/// A world change in a playshot: the world file replaced, and a walker and
/// streamer for it at the arrival.
fn change_world(lib: &mut Library, file: &str, at: Spawn) -> Result<(Walker, Streamer), Error> {
    let c = stream::open_file(std::path::Path::new(&crate::disc_path(None)), file)?;
    lib.replace(0, file, c);
    let index = vi::world::World::<vi::fpu::Ee>::zone_at(lib.file(0), at.pos[0], at.pos[2]).ok_or("no zone at the arrival")?;
    stream::load_zone_now(lib, 0, index)?;
    let character = lib.lookup(HUMAN).ok_or("no human model")?;
    let src = Source::Walk { file: 0, index, character, spawn: Some(at) };
    let s = src.scene(lib);
    let (w, _, st) = src.walker(lib, &s).ok_or("no walker")?;
    Ok((w, st))
}

/// The font name plates use (`VICSprite::FontResourceID`, "Arial"), if a
/// loaded file has it.
fn arial(lib: &Library) -> Option<esf::font::Font> {
    let id = vi::hsprite::hash_ref_id("Arial");
    if let Some(r) = lib.lookup(id)
        && lib.file(r.file).node(r.node).ty == 0x7000
    {
        return esf::font::read(lib.file(r.file), r.node);
    }
    lib.files.iter().find_map(|(_, c)| {
        (0..c.nodes.len()).filter(|&i| c.nodes[i].ty == 0x7000).find_map(|i| esf::font::read(c, i).filter(|f| f.id == id))
    })
}

fn report(renderer: &Renderer) {
    if !renderer.missing_textures.is_empty() {
        eprintln!("{} texture DictIDs resolve to nothing in the loaded files", renderer.missing_textures.len());
    }
}

/// A scripted walk for `shot`: hold `controls` for `ms` of game time.
#[derive(Clone, Copy, Debug)]
pub struct Script {
    pub ms: i64,
    pub controls: Controls,
}

/// Renders one view to a PNG, with no window.
pub fn shot(
    lib: &mut Library,
    source: &Source,
    cam: Option<Camera>,
    script: Option<Script>,
    (w, h): (u32, u32),
    out: &str,
) -> Result<(), Error> {
    shot_with(lib, source, cam, script, (w, h), out, None)
}

/// `shot`, playing on a server: the script's time passes in REAL time,
/// 16 ms a frame, with the session pumped every frame.
pub fn shot_with(
    lib: &mut Library,
    source: &Source,
    cam: Option<Camera>,
    script: Option<Script>,
    (w, h): (u32, u32),
    out: &str,
    mut play: Option<crate::play::Play>,
) -> Result<(), Error> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default()))?;
    let mut renderer = Renderer::new(&device, &queue, render::TARGET_FORMAT);
    renderer.scroll_time = Some(script.as_ref().map_or(0, |s| s.ms) as f32 / 1000.0);
    if let Some(f) = arial(lib) {
        renderer.set_font(&device, &queue, &f);
    }
    if let Some(p) = play.as_mut() {
        p.attach_ui(&mut renderer, &device, &queue, render::TARGET_FORMAT, lib);
    }
    let s = source.scene(lib);
    let walking = matches!(source, Source::Walk { .. });
    let mut gpu = if walking { Vec::new() } else { upload(&mut renderer, &device, &queue, lib, &s) };
    let mut streamed = None;
    let mut batches: Vec<render::billboard::Batch> = Vec::new();
    let mut billboard_view: Option<(glam::Mat4, glam::Mat4)> = None;
    let mut sky_billboards: Option<(glam::Mat4, glam::Mat4, Vec<render::billboard::Batch>)> = None;
    // the effect volumes round the walk (`fx.rs`)
    let mut fx: Option<crate::fx::Fx> = None;
    let cam = cam.unwrap_or_else(|| source.camera(s.bounds));
    let aspect = w as f32 / h as f32;
    let mut view = cam.view(s.bounds, aspect);
    if let Some((mut walker, body, mut streamer)) = source.walker(lib, &s) {
        // 60 frames a second of game time, the controls held throughout.
        let script = script.unwrap_or(Script { ms: 0, controls: Controls::default() });
        let mut t = 0;
        let mut point = None;
        let say = std::env::var("EQOA_SAY").ok();
        let tab = std::env::var_os("EQOA_TAB").is_some();
        let mut parts = crate::particles::Particles::new(&walker.scene, 0);
        let mut held_fx: Option<crate::particles::Held> = None;
        fx = crate::fx::enabled().then(crate::fx::Fx::new);
        // `EQOA_SPELL=n`: SPELLFX's n-th effect, cast by the character at a
        // stand-in 6 ahead of it at EQOA_AT (offline, no server).
        let mut trial = std::env::var("EQOA_SPELL").ok().and_then(|v| v.parse::<usize>().ok()).filter(|_| play.is_none()).map(|n| {
            // and ITEM.ESF, where the hands' projectiles are
            for f in ["SPELLFX.CSF", "FX.CSF", "ITEM.ESF"] {
                if lib.files.iter().all(|(name, _)| name != f)
                    && let Ok(b) = crate::disc(None).and_then(|mut d| Ok(d.read(f)?))
                    && let Ok(c) = esf::Container::open(b)
                {
                    lib.add(f, c);
                }
            }
            (crate::spells::Spells::load(lib, 0), n, vi::csprite::Rand(1))
        });
        // `EQOA_RANGED=7` (a bow), `9` (a crossbow) or `10:<DictID>` (that
        // item thrown): the trial's caster holds it in its second hand and
        // nothing in its first, as a bow is worn, so a ranged swing's
        // projectile flies (`spells::hand_projectiles`)
        let ranged: Option<(i32, u32)> = std::env::var("EQOA_RANGED").ok().and_then(|v| {
            let (ty, id) = v.split_once(':').unwrap_or((&v, "0"));
            let id = id.strip_prefix("0x").map_or_else(|| id.parse().ok(), |h| u32::from_str_radix(h, 16).ok())?;
            Some((ty.parse().ok()?, id))
        });
        let mut weather = crate::weather::Weather::new(lib, 0);
        // `EQOA_AT=ms`: when the target (and then Q, attack) steps happen.
        let at: i64 = std::env::var("EQOA_AT").ok().and_then(|v| v.parse().ok()).map_or(2000, |v: i64| v / 16 * 16);
        let attack = std::env::var_os("EQOA_ATTACK").is_some();
        let trace_walk: Option<i64> = std::env::var("EQOA_TRACE_WALK").ok().and_then(|v| v.parse::<i64>().ok()).map(|v| (v / 16 * 16).max(16));
        let walk = walk_script();
        // `EQOA_FRAME_MS=n`: frames of n ms instead of 16 (a display's rate:
        // 6 is about 165 a second, 33 the console's 30)
        let frame_ms: i64 = std::env::var("EQOA_FRAME_MS").ok().and_then(|v| v.parse().ok()).filter(|&v: &i64| v > 0).unwrap_or(16);
        // the game's frames among them (`frames.rs`), and how many since `at`
        let mut frames = crate::frames::Frames::default();
        let mut game_frames = 0usize;
        let mut hud_drawn = false;
        // `EQOA_PAD`: a pad by script, through the game's pad path;
        // `EQOA_MOUSE`: the mouse's, through the window's
        let pad_script = crate::input::Script::from_env();
        let mut console = crate::input::Console::new();
        let mut pad_edge = crate::binds::PadEdge::default();
        let mouse_script = crate::mouse::script();
        let mut mouse = crate::mouse::Mouse::default();
        let mut last_view: Option<glam::Mat4> = None;
        // `EQOA_TOUCH=1` and `EQOA_TOUCHES`: the touch screen's pad
        let touch_script = crate::touch::script();
        let mut touch = crate::touch::TouchPad::new(touch_first());
        while t < script.ms {
            t += frame_ms;
            let game_frame = frames.due(t as f64);
            let controls = match &walk {
                Some(segs) => segs.iter().find(|s| t <= s.1).map_or(Controls::default(), |s| s.0),
                None => script.controls,
            };
            let mut sticks = controls.axes();
            // `EQOA_MAP=ms`: the map opened then (M)
            if let Some(p) = play.as_mut()
                && std::env::var("EQOA_MAP").ok().and_then(|v| v.parse::<i64>().ok()).is_some_and(|ms| ms / 16 * 16 == t)
            {
                p.map.show();
            }
            touch.window = play.as_ref().is_some_and(|p| p.window_up() || p.map.open);
            touch.dpad = play.as_mut().is_some_and(|p| p.quick_dpad());
            for (_, id, phase, at) in touch_script.iter().filter(|e| e.0 == t) {
                touch.touch(*id, *phase, *at, Instant::now(), (w, h));
            }
            // `EQOA_TAP=<name>@<ms>`: a tap where that creature is drawn
            if let Ok(spec) = std::env::var("EQOA_TAP")
                && let Some((name, ms)) = spec.rsplit_once('@')
                && ms.parse::<i64>().is_ok_and(|ms| ms / 16 * 16 == t)
                && let (Some(p), Some(vp)) = (play.as_ref(), last_view)
                && let Some((x, y)) = p.screen_of(name, (w, h), vp)
            {
                eprintln!("tap {name} at ({x:.0}, {y:.0})");
                touch.touch(99, crate::touch::Phase::Started, [x as f32, y as f32], Instant::now(), (w, h));
                touch.touch(99, crate::touch::Phase::Ended, [x as f32, y as f32], Instant::now(), (w, h));
            }
            if pad_script.is_some() || touch.used {
                let dead = play.as_ref().is_some_and(|p| p.is_dead());
                let mut report = pad_script.as_ref().map_or_else(Default::default, |s| s.at(t));
                // the script stands for a controller: the player's own
                // buttons (`binds`), and a capture's, as the window's
                let capturing = play.as_ref().is_some_and(|p| p.capturing().is_some());
                let (fields, took) = crate::settings::with_binds(|b| pad_edge.frame(report.buttons, capturing, b));
                report.buttons = fields;
                if let (Some(i), Some(p)) = (took, play.as_mut()) {
                    eprintln!("pad: button {} taken by the Controls page", crate::binds::BUTTONS[i as usize].0);
                    p.capture_button(i);
                }
                report.merge(&touch.report());
                let keys = console.poll(report, true, false, dead);
                if let Some(p) = play.as_mut() {
                    p.pad_mode(console.mode());
                    for (k, down) in keys {
                        eprintln!("pad key {k} {}", if down { "down" } else { "up" });
                        p.set_key(k, down, &mut walker, aspect);
                    }
                }
                if !dead {
                    for (a, p) in sticks.iter_mut().zip(console.sticks()) {
                        if p * p > *a * *a {
                            *a = p;
                        }
                    }
                }
            }
            for c in touch.take_clicks() {
                if let (Some(p), Some(vp)) = (play.as_mut(), last_view) {
                    let mode = p.mode;
                    if p.point(c, (w, h), true) {
                        eprintln!("touch tap: {c:?} on a window (mode {mode})");
                        continue;
                    }
                    let id = p.creature_at(c.at(), (w, h), vp);
                    eprintln!("touch tap: {c:?} on {id:?}");
                    p.click(id, !matches!(c, crate::mouse::Click::Target(..)));
                }
            }
            for (_, step) in mouse_script.iter().filter(|s| s.0 == t) {
                // `click @<name>`: at the creature called that, where it is drawn
                let mut step = step.clone();
                if let Some((verb, name)) = step.split_once(" @")
                    && let (Some(p), Some(vp)) = (play.as_ref(), last_view)
                {
                    match p.screen_of(name, (w, h), vp) {
                        Some((x, y)) => step = format!("{verb} {x:.0} {y:.0}"),
                        None => eprintln!("mouse {step}: not on screen"),
                    }
                }
                let click = mouse.play(&step, (w, h), Instant::now());
                if let (Some(c), Some(p), Some(vp)) = (click, play.as_mut(), last_view) {
                    let mode = p.mode;
                    if p.point(c, (w, h), false) {
                        eprintln!("mouse {step}: {c:?} on a window (mode {mode})");
                        continue;
                    }
                    let id = p.creature_at(c.at(), (w, h), vp);
                    eprintln!("mouse {step}: {:?} on {id:?}", c);
                    p.click(id, !matches!(c, crate::mouse::Click::Target(..)));
                }
            }
            // the pointer as a window holds it for a drag (`mouse::Grab`)
            let look_free = !play.as_ref().is_some_and(|p| p.is_dead()) && play.as_ref().is_none_or(|p| p.look_free());
            if let Some(on) = mouse.grab_change(look_free) {
                mouse.set_grab(if on { crate::mouse::Grab::Held } else { crate::mouse::Grab::Free });
                eprintln!("mouse: pointer {}", if on { "held" } else { "let go" });
            }
            // the pointers' looks, taken every frame, applied only in play
            // with the camera free
            let (tx, ty, pinch) = touch.take_look();
            if let Some(p) = play.as_mut().filter(|p| p.map.open) {
                // the map up: a finger's drag and pinch, the wheel, its; the
                // pad's sticks go to it (`map_frame`), the walk stands
                p.map.touch(tx, ty, pinch);
                p.map.wheel(mouse.take_wheel(), None);
                mouse.discard();
                sticks = [0.0; 4];
            } else if look_free {
                let inverted = play.as_ref().is_some_and(|p| p.options.inverted != 0);
                let frame_s = frame_ms as f32 / 1000.0;
                let (look, zoom) = mouse.look(&mut walker, inverted, frame_s);
                if let Some(p) = play.as_mut() {
                    p.wheel_style(&mut walker, zoom, &mut mouse.back);
                }
                let feel = crate::mouse::look(&mut walker, tx, ty, pinch, inverted, frame_s);
                for i in 0..2 {
                    for m in [look[i], feel[i]] {
                        if m * m > sticks[i] * sticks[i] {
                            sticks[i] = m;
                        }
                    }
                }
                // both mouse buttons: run forward (`Mouse::running`)
                if mouse.running() {
                    sticks[3] = -1.0;
                }
            } else {
                // a window up: the wheel and a finger's drag are its up and down
                let lines = mouse.take_wheel() + crate::touch::drag_lines(ty, (w, h));
                mouse.discard();
                if let Some(p) = play.as_mut() {
                    p.window_wheel(lines);
                }
            }
            let mut client = play.as_ref().filter(|p| !p.is_dead()).map(|p| p.control_client()).unwrap_or_default();
            client.steer = look_free && mouse.steer(walker.cam.camera_mode);
            let changed = walker.frame_with(sticks, t, &client, game_frame);
            if let Some(p) = play.as_mut() {
                p.controls_changed(changed);
            }
            // `EQOA_TRACE_WALK=ms`: the body and the camera every that often
            if let Some(every) = trace_walk
                && t / every != (t - frame_ms) / every
            {
                let v = walker.cam.walk.phys.velocity;
                eprintln!("walk {t} ms: {} vel ({:.4}, {:.4}, {:.4}) orbit {:.4} sticks {sticks:?} anim {:?}", walker.describe(), v.x.0, v.y.0, v.z.0, walker.cam.cur.orbit.x.0, walker.sprite.as_ref().map(|s| s.action));
            }
            let (proj, v) = walker.view(aspect);
            last_view = Some(proj * v);
            // the zones it walks toward, read as the game's loader reads them
            streamer.fetch(Vec3::from(walker.cam.walk.phys.position.to_array()), &walker.scene, lib);
            if let Some(first) = streamer.install(lib, &mut walker.scene) {
                parts.add_from(&walker.scene, first);
            }
            if let Some(p) = &mut play {
                // `EQOA_SAY=<text>[@ms][;<text>@ms...]`: each line said at its
                // time (a second in without one).
                if let Some(lines) = &say {
                    for spec in lines.split(';') {
                        let (line, ms) = match spec.rsplit_once('@') {
                            Some((l, ms)) if ms.parse::<i64>().is_ok() => (l, ms.parse::<i64>().unwrap() / 16 * 16),
                            _ => (spec, 1008),
                        };
                        if t == ms {
                            p.chat_line(line);
                        }
                    }
                }
                // `EQOA_TAB`: Tab two seconds in.
                // `EQOA_TARGET=<name>[@ms][;<name>@ms...]`: that creature
                // targeted then (two seconds in without a time).
                if let Ok(spec) = std::env::var("EQOA_TARGET") {
                    for part in spec.split(';') {
                        let (name, ms) = match part.rsplit_once('@') {
                            Some((n, ms)) if ms.parse::<i64>().is_ok() => (n, ms.parse::<i64>().unwrap() / 16 * 16),
                            _ => (part, at),
                        };
                        if t == ms {
                            let found = p.target_by_name(name);
                            eprintln!("target {name}: {found}");
                            if !found {
                                eprintln!("  in view: {}", p.entity_names().join(", "));
                            }
                        }
                    }
                }
                // `EQOA_PET=<name>`: that creature made the pet then (as 0x007E).
                if t == at
                    && let Ok(name) = std::env::var("EQOA_PET")
                {
                    eprintln!("pet {name}: {}", p.pet_by_name(&name));
                }
                // `EQOA_P=<ms>`: the P key then (first person and back)
                if std::env::var("EQOA_P").ok().and_then(|v| v.parse::<i64>().ok()).is_some_and(|ms| ms / 16 * 16 == t) {
                    p.camera_style_key(&mut walker);
                }
                if t == at && tab {
                    let (proj, v) = walker.view(aspect);
                    let dir = Vec3::from(walker.eye.direction.to_array());
                    p.cycle_target(false, walker.eye_position(), dir, proj * v);
                }
                // `EQOA_BOX=text|option|...`: a dialog box then; `EQOA_BOX_KEYS`
                // (u, d, e, x) pressed one a frame after it.
                if t == at
                    && let Ok(spec) = std::env::var("EQOA_BOX")
                {
                    p.fake_dialog_box(&spec);
                }
                if t > at
                    && let Ok(keys) = std::env::var("EQOA_BOX_KEYS")
                    && let Some(k) = keys.chars().nth(((t - at) / 16 - 1) as usize)
                {
                    use crate::dialog::Key as D;
                    let k = match k {
                        'u' => Some(D::Up),
                        'd' => Some(D::Down),
                        'e' => Some(D::Enter),
                        'x' => Some(D::Escape),
                        _ => None,
                    };
                    if let Some(k) = k {
                        p.dialog_key(k);
                    }
                }
                // `EQOA_HUD_KEYS` (u, d, l, r, e): the HUD's quick-menu keys, one a
                // frame from then on.
                if t > at
                    && let Ok(keys) = std::env::var("EQOA_HUD_KEYS")
                    && let Some(k) = keys.chars().nth(((t - at) / 16 - 1) as usize)
                {
                    use crate::hudmenu::Key as H;
                    let k = match k {
                        'u' => Some(H::Up),
                        'd' => Some(H::Down),
                        'l' => Some(H::Left),
                        'r' => Some(H::Right),
                        'e' => Some(H::Use),
                        _ => None,
                    };
                    if let Some(k) = k {
                        p.hud_key(k);
                    }
                }
                // `EQOA_START_KEYS` (o open, u d l r e x, s square, c circle, t R1, any
                // other character a pause):
                // the start menu's
                // keys, the spell book's while it is up, each held for a frame of
                // the game's (pressed, then released on the next: the menus'
                // auto-repeat pulses once a frame of the game's while a key is down).
                if t > at && game_frame {
                    game_frames += 1;
                }
                let step = game_frames.saturating_sub(1);
                if t > at
                    && game_frame
                    && let Ok(keys) = std::env::var("EQOA_START_KEYS")
                    && let Some(k) = keys.chars().nth(step / 2)
                {
                    let down = step.is_multiple_of(2);
                    use crate::startmenu::keys as K;
                    let k = match k {
                        'o' => {
                            if down {
                                p.open_start_menu(true);
                            }
                            None
                        }
                        'u' => Some(K::UP),
                        'd' => Some(K::DOWN),
                        'l' => Some(K::LEFT),
                        'r' => Some(K::RIGHT),
                        'e' => Some(K::CROSS),
                        'x' => Some(K::ESCAPE),
                        's' => Some(133),
                        'c' => Some(135),
                        't' => Some(142),
                        _ => None,
                    };
                    let k = if keys.chars().nth(step / 2) == Some('a') { Some(113) } else { k };
                    if let Some(k) = k {
                        if p.search_open() {
                            p.search_key(k, down);
                        } else if p.bank_owns() {
                            p.bank_key(k, down);
                        } else if p.npcw_owns() {
                            p.npcw_key(k, down);
                        } else if p.support_owns() {
                            p.support_key(k, down);
                        } else if p.mode == 5 {
                            p.book_key(k, down);
                        } else if p.mode == 35 {
                            p.who_key(k, down);
                        } else if p.char_window() {
                            p.char_key(k, down);
                        } else if p.mail_owns() {
                            p.mail_key(k, down);
                        } else if p.panel_up() {
                            p.panel_key(k, down);
                        } else {
                            p.start_key(k, down);
                        }
                    }
                }
                // `EQOA_MAIL_TEXT=a|b|c`: the mail and tell forms' fields, one
                // into each editor as it opens.
                p.mail_script_step();
                // `EQOA_CS_TEXT`: the CS request's text, into its editor.
                p.support_script_step();
                // `EQOA_EDIT_TEXT`: typed into the edit box, a character a frame
                // once it is up, then Enter; the same into the quick-chat
                // editor's own (`EQOA_QUICK_TEXT`, each `|`-separated piece
                // into the next one that opens).
                if p.mode == 2
                    && let Ok(text) = std::env::var("EQOA_EDIT_TEXT")
                {
                    let k = p.edit_typed;
                    p.edit_typed += 1;
                    match text.chars().nth(k) {
                        Some(c) => p.edit_key(c as i32, true),
                        None if k == text.chars().count() => p.edit_key(13, true),
                        None => {}
                    }
                }
                if let Ok(text) = std::env::var("EQOA_QUICK_TEXT") {
                    p.quick_script_step(&text);
                }
                // `EQOA_QUESTS=a|b|...`: quests as if 0x007C had brought them.
                if t == at
                    && let Ok(q) = std::env::var("EQOA_QUESTS")
                {
                    p.fake_quests(&q);
                }
                // `EQOA_DAMAGE=<attacker>,<amount>,<defender>@ms;...`: 0x00DBs
                // then (self, target or a hex id).
                // `EQOA_DIE=ms`: the player dies then, as 0x005E says.
                if let Ok(v) = std::env::var("EQOA_DIE")
                    && let Ok(ms) = v.parse::<i64>()
                    && t == ms / 16 * 16
                {
                    p.fake_death(&mut walker);
                }
                if let Ok(spec) = std::env::var("EQOA_DAMAGE") {
                    for part in spec.split(';') {
                        if let Some((hit, ms)) = part.rsplit_once('@')
                            && let Ok(ms) = ms.parse::<i64>()
                            && t == ms / 16 * 16
                            && let [a, n, d] = hit.split(',').collect::<Vec<_>>()[..]
                            && let Ok(n) = n.parse::<i64>()
                        {
                            p.fake_damage(a, n, d, walker.cam.camera_mode);
                        }
                    }
                }
                // `EQOA_EFFECTS=<icon>:<name>|...`: effects as if channel 0x43
                // had brought them.
                if t == at
                    && let Ok(fx) = std::env::var("EQOA_EFFECTS")
                {
                    p.fake_effects(&fx);
                }
                // `EQOA_INVITE`: the target invited just after targeting
                // (`DoAction(0x07010000)`); `EQOA_ACCEPT`: any message box
                // answered Yes (left, then X).
                if t == at + 96 && std::env::var_os("EQOA_INVITE").is_some() {
                    p.group_action(0x0701_0000);
                }
                // `EQOA_ACTION=<hex>@<ms>[;<hex>@<ms>...]`: a start-menu
                // entry's message then (`DoAction`; 0B060000 asks the target
                // to trade, 0B0E0006 is Assist Target).
                if let Ok(spec) = std::env::var("EQOA_ACTION") {
                    for part in spec.split(';') {
                        if let Some((a, ms)) = part.split_once('@')
                            && let (Ok(a), Ok(ms)) = (u32::from_str_radix(a.trim().trim_start_matches("0x"), 16), ms.trim().parse::<i64>())
                            && t == ms / 16 * 16
                        {
                            p.do_action(a, 0);
                        }
                    }
                }
                // `EQOA_GROUP_ACTION=<hex>@<ms>`: a start-menu group action then.
                if let Ok(spec) = std::env::var("EQOA_GROUP_ACTION")
                    && let Some((a, ms)) = spec.split_once('@')
                    && let (Ok(a), Ok(ms)) = (u32::from_str_radix(a.trim_start_matches("0x"), 16), ms.parse::<i64>())
                    && t == ms / 16 * 16
                {
                    p.group_action(a);
                }
                if p.mode == 14 && std::env::var_os("EQOA_ACCEPT").is_some() {
                    p.dialog_key(crate::dialog::Key::Left);
                    p.dialog_key(crate::dialog::Key::Enter);
                }
                // `EQOA_GUILD=create <name>@ms;invite <name>@ms`: the guild's
                // Create (past the client's group and tunar checks, which a
                // two-account test cannot meet) and Invite sent then.
                if let Ok(spec) = std::env::var("EQOA_GUILD") {
                    for part in spec.split(';') {
                        if let Some((cmd, ms)) = part.rsplit_once('@')
                            && let Ok(ms) = ms.parse::<i64>()
                            && t == ms / 16 * 16
                        {
                            p.guild_test(cmd);
                        }
                    }
                }
                // and a buddy request's box (mode 17), a guild invitation's (20)
                if matches!(p.mode, 17 | 20) && std::env::var_os("EQOA_ACCEPT").is_some() {
                    use crate::startmenu::keys as K;
                    for k in [K::LEFT, K::CROSS] {
                        p.panel_key(k, true);
                        p.panel_key(k, false);
                    }
                }
                // `EQOA_INTERACT[=ms]`: Q just after targeting, or at that time.
                if let Ok(v) = std::env::var("EQOA_INTERACT")
                    && t == v.parse::<i64>().map_or(at + 96, |ms| ms / 16 * 16)
                {
                    p.interact_with_target();
                }
                // `EQOA_ATTACK`: attack held from just after that to the end.
                if t == at + 96 && attack {
                    p.attack_press();
                }
                let gpu = crate::play::Gpu { renderer: &mut renderer, device: &device, queue: &queue };
                if let Some(model) = p.frame(&mut walker, &*lib, gpu, t, game_frame, None)
                    && let Some(r) = lib.lookup(model as i32)
                {
                    eprintln!("the server names the player's model {model:#010x}");
                    walker.set_character(lib, r);
                }
                // the HUD, once SUPPORT's atlas has come
                p.attach_ui(&mut renderer, &device, &queue, render::TARGET_FORMAT, lib);
                for (w, instant) in p.weather.drain(..) {
                    weather.set_weather(w, instant);
                }
                // The HUD steps each of the game's frames (its glows, the
                // name's scroll).
                if let Some(u) = renderer.ui.as_mut() {
                    let (proj, v) = walker.view(w as f32 / h as f32);
                    p.hud(u, &walker, t, proj * v, (w, h), game_frame);
                    hud_drawn |= game_frame;
                }
                // TrackTarget too (the rings' clock); the last frame's are drawn
                let (proj, v) = walker.view(w as f32 / h as f32);
                p.draw_rings(&mut renderer, &device, &queue, &walker, proj * v, lib, game_frame);
                // the map (`map.rs`): its build, and its view while up
                p.map_frame(&walker, lib, &device, &queue, render::TARGET_FORMAT, (w, h), frame_ms as f32 / 1000.0, console.sticks());
                // A teleport into another world: that world's file in place
                // of this one, and the walk begun again there.
                if let Some((file, at)) = p.take_world_change() {
                    match change_world(lib, &file, at) {
                        Ok((w, s2)) => {
                            walker = w;
                            streamer = s2;
                            // the particles named the old world's actors
                            parts = crate::particles::Particles::new(&walker.scene, t);
                            fx = crate::fx::enabled().then(crate::fx::Fx::new);
                        }
                        Err(e) => eprintln!("world change to {file}: {e}"),
                    }
                    p.world_entered(&mut walker);
                }
                // under the loading screen the zones go up at once (the
                // shot's own go up at its end), and it comes down once they are
                if p.loading() {
                    let at = Vec3::from(walker.cam.walk.phys.position.to_array());
                    streamer.update(at, &mut walker.scene, lib, stream::Gpu { renderer: &mut renderer, device: &device, queue: &queue }, None);
                    let settled = p.loading_at().is_some_and(|a| streamer.settled(a, &walker.scene, lib));
                    p.loading_step(&mut walker, settled);
                }
                std::thread::sleep(std::time::Duration::from_millis(frame_ms as u64));
            }
            // UpdateSprite runs every frame in the game.
            let (self_m, _) = walker.pose();
            let (proj, v) = walker.view(aspect);
            batches = parts.frame(&walker.scene, lib, walker.eye_position(), proj * v, t);
            if let Some((sp, n, rand)) = trial.as_mut() {
                use crate::spells::{Body, Cast, SELF};
                use vi::fpu::{Ee, F};
                use vi::geom::Matrix44;
                use vi::vect::Vect3;
                let phys = &walker.cam.walk.phys;
                let heading = phys.angles.x;
                let actor = Matrix44::<Ee>::transform(&Vect3::new(heading, F::ZERO, F::ZERO), F::ONE, &phys.smooth_location());
                let ahead = actor.mul_point(&Vect3::of(0.0, 0.0, 6.0));
                let target = Matrix44::<Ee>::transform(&Vect3::new(heading, F::ZERO, F::ZERO), F::ONE, &ahead);
                let anims = &walker.anims;
                let (sprite, centre) = match walker.sprite.as_mut() {
                    Some(cs) => {
                        let c = cs.h.calc_trans_about_center(&actor, anims);
                        (Some((cs, anims, &mut *rand)), c)
                    }
                    None => (None, actor),
                };
                let (attack, projectile) = ranged.map_or(([0, 0], [None; 2]), |(ty, id)| ([11, ty], [None, Some(crate::spells::second_model(ty, id))]));
                let mut bodies = vec![Body { actor: SELF, sprite, transform: actor, centre, attack, projectile }, Body { actor: 1, sprite: None, transform: target, centre: target, attack: [0, 0], projectile: [None; 2] }];
                let mut casts = Vec::new();
                if t == at && let Some(effect) = sp.nth(*n) {
                    eprintln!("spell effect {n} ({effect:#010x}) cast");
                    casts.push((Cast { effect, time: 1500, caster: 0, target: 1, targets: Vec::new() }, SELF, 1, Vec::new()));
                }
                sp.frame(&mut bodies, &casts, SELF, Some(&mut walker.cam.effect), t, &walker.eye, lib, None);
                batches.append(&mut sp.batches);
                renderer.strips = std::mem::take(&mut sp.strips);
            }
            if let Some(sp) = play.as_mut().and_then(|p| p.spells.as_mut()) {
                batches.append(&mut sp.batches);
                renderer.strips = std::mem::take(&mut sp.strips);
            }
            // the particles of the items in hands (an epic weapon's)
            if let Some(p) = play.as_ref() {
                let held = held_fx.get_or_insert_with(|| crate::particles::Held::new(t));
                batches.extend(held.frame(lib, &p.in_hand(lib, &walker, self_m), &walker.eye.position, t));
            }
            // the weapon trails (VITrailFxRender) after them
            if let Some(p) = play.as_mut() {
                renderer.strips.append(&mut p.trail_strips);
            }
            // the atmosphere every frame; the last frame's lights and render
            // are the shot's
            weather.frame(&mut walker.scene, None, &walker.eye, t, game_hour(play.as_ref(), Instant::now()));
            // the effect volumes, stepped as the scene rasters them
            if let Some(fx) = fx.as_mut() {
                fx.stream(Vec3::from(walker.cam.walk.phys.position.to_array()), &walker.scene, lib);
                let cam = crate::fx::camera(walker.eye.position, walker.eye.direction);
                fx.frame(&walker.scene, lib, &cam, proj * v, walker.eye_position(), t, game_hour(play.as_ref(), Instant::now()));
            }
            if t < script.ms {
                let _ = lit(proj, v, game_hour(play.as_ref(), Instant::now()), Some((walker.eye_position(), &walker.scene)), Some(&mut weather));
                if let Some(p) = play.as_mut() {
                    point = p.point_light(&walker, t, game_hour(Some(p), Instant::now()), Some(&weather));
                }
                let _ = weather.render(&walker.eye);
            }
        }
        if script.ms == 0 {
            weather.frame(&mut walker.scene, None, &walker.eye, 0, game_hour(play.as_ref(), Instant::now()));
        }
        eprintln!("{}", parts.describe());
        eprintln!("{}", weather.describe());
        if let Some((sp, _, _)) = &trial {
            eprintln!("{}", sp.describe());
        }
        if let Some(h) = &held_fx {
            eprintln!("{}", h.describe());
        }
        let body = match play.as_ref().and_then(|p| p.self_model).and_then(|m| lib.lookup(m as i32)).and_then(|r| lib.build_model(r)) {
            Some(m) => m,
            None => body,
        };
        let (model, palette) = walker.pose();
        eprintln!("after {} ms: {}", script.ms, walker.describe());
        let textures = render::library_textures(lib);
        let mut g = renderer.upload(&device, &queue, &body, &[model], &textures);
        renderer.set_palette(&queue, &g, &palette);
        crate::play::place_body(&mut g, &walker, model);
        if let Some(p) = play.as_mut().filter(|p| p.has_self()) {
            p.place_self(&walker, model, &palette, &renderer, &queue);
            g.visible = false;
        }
        gpu.push(g);
        // the trial's projectiles where the last frame left them
        if let Some((sp, _, _)) = &trial {
            for (id, m) in &sp.flying {
                if let Some(model) = lib.lookup(*id as i32).and_then(|r| lib.build_model(r)) {
                    gpu.push(renderer.upload(&device, &queue, &model, &[m.to_cols()], &textures));
                }
            }
        }
        let at = Vec3::from(walker.cam.walk.phys.position.to_array());
        streamer.update(at, &mut walker.scene, lib, stream::Gpu { renderer: &mut renderer, device: &device, queue: &queue }, None);
        streamer.animate(&mut walker.scene, &renderer, &queue);
        eprintln!("{}", streamer.describe());
        let (mut proj, mut v) = walker.view(aspect);
        let mut shot_eye = walker.eye_position();
        let mut shot_dir = walker.eye.direction;
        // `EQOA_SHOT_EYE=dx,dy,dz`: look at the character from that offset
        // instead of through the game's camera (for checking poses).
        let eye: Vec<f32> = std::env::var("EQOA_SHOT_EYE")
            .ok()
            .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect())
            .unwrap_or_default();
        // `EQOA_SHOT_AT=x,y,z`: look at that point (from the EYE offset) instead.
        let at: Vec<f32> = std::env::var("EQOA_SHOT_AT")
            .ok()
            .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect())
            .unwrap_or_default();
        if let [dx, dy, dz] = eye.as_slice() {
            let p = match at.as_slice() {
                [x, y, z] => Vec3::new(*x, *y, *z),
                _ => Vec3::from(walker.cam.walk.phys.position.to_array()) + Vec3::Y,
            };
            let at = p + Vec3::new(*dx, *dy, *dz);
            let far = crate::fog::far_plane(&walker.scene, at, crate::settings::fog());
            proj = glam::camera::rh::proj::directx::perspective(40f32.to_radians(), aspect, 0.1, far);
            shot_eye = at;
            shot_dir = vi::vect::Vect3::of(p.x - at.x, p.y - at.y, p.z - at.z);
            v = glam::camera::rh::view::look_at_mat4(at, p, Vec3::Y);
        }
        billboard_view = Some((v, proj));
        let shot_hour = game_hour(play.as_ref(), Instant::now());
        let server = std::env::var_os("EQOA_HOUR").is_none() && play.as_ref().is_some_and(|p| p.session.clock_b.ready);
        eprintln!("hour {shot_hour:.2}{}", if server { " (the server's)" } else { "" });
        let mut frame = lit(proj, v, shot_hour, Some((shot_eye, &walker.scene)), Some(&mut weather));
        frame.0.point = point;
        let (sky_bb, world_bb) = weather.render(&walker.eye);
        batches.extend(world_bb);
        let mut rot = v;
        rot.w_axis = glam::Vec4::W;
        sky_billboards = Some((rot, proj, sky_bb));
        if let (Some(f), Some(at)) = (&frame.1, crate::sky::upload(&mut renderer, &device, &queue, lib, &mut gpu)) {
            let textures = render::library_textures(lib);
            crate::sky::update(&mut renderer, &device, &queue, &mut gpu, at, f, &textures);
        }
        view = frame.0;
        // the volumes as the shot's camera sees them (the walk's clock does
        // not move: nothing steps again)
        if let Some(fx) = fx.as_mut() {
            fx.stream(Vec3::from(walker.cam.walk.phys.position.to_array()), &walker.scene, lib);
            let cam = crate::fx::camera(vi::vect::Vect3::of(shot_eye.x, shot_eye.y, shot_eye.z), shot_dir);
            fx.frame(&walker.scene, lib, &cam, view.view_proj, view.eye, t, game_hour(play.as_ref(), Instant::now()));
            fx.upload(&mut renderer, &device, &queue, lib);
            eprintln!("{}", fx.describe());
        }
        streamed = Some(streamer);
        if let Some(p) = &mut play {
            if let Some(u) = renderer.ui.as_mut() {
                // the last of the game's frames' HUD (made now if none came)
                p.hud(u, &walker, t, view.view_proj, (w, h), !hud_drawn);
                touch.textures(u, &device, &queue);
                p.draw_touch(u, &touch, (w, h));
            }
            let plates = p.plates();
            if let Some(t) = renderer.text.as_mut() {
                t.plates = crate::play::labels(&plates, shot_eye, view.view_proj, t);
                t.labels = crate::play::chat_labels(p, t, (w, h));
            }
        }
        if let Some(p) = &play {
            eprintln!("play: {}", p.describe());
            for e in p.entities.slots.iter().flatten() {
                let name = e.creature.as_ref().map(|c| c.name.as_str()).unwrap_or("");
                eprintln!("  entity {:#x} {name:24} model {:#010x} at ({:.1}, {:.1}, {:.1})", e.id, e.model, e.pos[0], e.pos[1], e.pos[2]);
            }
        }
    }
    // the loading screen in place of the scene: its gears through its own view
    let loading = play.as_ref().and_then(|p| p.loading_scene((w, h)));
    // the name plates are the scene's: none under the loading screen
    if loading.is_some()
        && let Some(t) = renderer.text.as_mut()
    {
        t.plates.clear();
    }
    let refs: Vec<render::Draw> = match &loading {
        Some((_, gears)) => gears.iter().map(|&m| render::Draw::Model(m)).collect(),
        None => gpu
            .iter()
            .map(render::Draw::Model)
            .chain(streamed.iter().flat_map(|s| s.draws()))
            .chain(play.iter().flat_map(|p| p.models()).map(render::Draw::Model))
            .chain(fx.iter().flat_map(|f| f.models()).map(render::Draw::Model))
            .collect(),
    };
    let textures = render::library_textures(lib);
    let particles = billboard_view.filter(|_| loading.is_none()).map(|(v, p)| (v, p, batches.as_slice(), &textures as &render::Textures));
    let sky = sky_billboards.as_ref().filter(|_| loading.is_none()).map(|(v, p, b)| (*v, *p, b.as_slice(), &textures as &render::Textures));
    // the options' Brightness (kept only under `$EQOA_CONFIG` for a shot)
    renderer.curve.set(crate::settings::curve(crate::settings::client().brightness));
    let mut rgba = render::capture_with_sky(&device, &queue, &mut renderer, w, h, loading.as_ref().map_or(&view, |l| &l.0), &refs, particles, sky);
    // the map up covers the frame: the picture is the map's
    if let Some(p) = play.as_ref().filter(|p| p.map.open) {
        rgba = crate::map::tool::capture(&p.map, &device, &queue, (w, h))?;
        eprintln!("{}", p.map.describe());
    }
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out)?), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgba)?;
    eprintln!("{out}: {} ({cam:?})", s.summary);
    report(&renderer);
    Ok(())
}

/// The texture an `EQOA_OFFSCREEN` run draws into (read back for
/// `EQOA_SHOTS`).
fn offscreen_target(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// `EQOA_SHOTS`: the shots due at `now` taken off the offscreen target,
/// each once.
fn shoot(shots: &mut Vec<(i64, String)>, g: &Gfx, now: i64, what: &str) {
    let due: Vec<String> = shots.iter().filter(|s| s.0 <= now).map(|s| s.1.clone()).collect();
    if due.is_empty() {
        return;
    }
    shots.retain(|s| s.0 > now);
    let Some(t) = &g.offscreen else {
        crate::say("EQOA_SHOTS needs EQOA_OFFSCREEN");
        return;
    };
    let rgba = read_back(&g.device, &g.queue, t);
    for f in due {
        match write_png(&f, g.config.width, g.config.height, &rgba) {
            Ok(()) => crate::say(&format!("shot {f} at {now} ms ({what})")),
            Err(e) => crate::say(&format!("shot {f}: {e}")),
        }
    }
}

/// A texture's RGBA8 rows, read back (waits for the GPU).
fn read_back(device: &wgpu::Device, queue: &wgpu::Queue, t: &wgpu::Texture) -> Vec<u8> {
    let (w, h) = (t.width(), t.height());
    let row = (w * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("shot"), size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        t.as_image_copy(),
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |r| r.expect("map the shot"));
    device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
    let data = buf.slice(..).get_mapped_range().expect("the shot's bytes");
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h as usize {
        out.extend_from_slice(&data[y * row as usize..y * row as usize + w as usize * 4]);
    }
    out
}

fn write_png(path: &str, w: u32, h: u32, rgba: &[u8]) -> Result<(), Error> {
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(path)?), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(rgba)?;
    Ok(())
}

/// `EQOA_REPORT_TEST=<note>@<ms>` (natively): the report's dialog opened
/// then (or the crash report it offers, already open), the note typed and
/// sent two seconds after, the picture taken between (`EQOA_REPORT_NOPIC`:
/// the picture turned off before it is sent, Tab to its row and Space, as a
/// player would); `EQOA_CRASH_TEST=<ms>`: a panic then, as a bug would (a
/// crash report for the next start). `now`: ms on the client's clock.
#[cfg(not(target_arch = "wasm32"))]
fn report_test(now: i64) {
    use std::sync::atomic::{AtomicU8, Ordering};
    static STEP: AtomicU8 = AtomicU8::new(0);
    if let Some(at) = std::env::var("EQOA_CRASH_TEST").ok().and_then(|v| v.parse::<i64>().ok())
        && now >= at
    {
        panic!("EQOA_CRASH_TEST: the client stopped on purpose at {now} ms");
    }
    let Ok(spec) = std::env::var("EQOA_REPORT_TEST") else { return };
    let Some((note, at)) = spec.rsplit_once('@') else { return };
    let Ok(at) = at.parse::<i64>() else { return };
    match STEP.load(Ordering::Relaxed) {
        0 if now >= at => {
            crate::report::open();
            STEP.store(1, Ordering::Relaxed);
        }
        1 if now >= at + 2000 => {
            crate::report::key(crate::report::Key::Text(note.to_string()));
            if std::env::var_os("EQOA_REPORT_NOPIC").is_some() {
                for k in [crate::report::Key::Tab, crate::report::Key::Tab, crate::report::Key::Text(" ".into())] {
                    crate::report::key(k);
                }
            }
            crate::report::key(crate::report::Key::Enter);
            STEP.store(2, Ordering::Relaxed);
        }
        _ => {}
    }
}

/// Whether the report's dialog is open (natively; a page's is its own).
fn report_open() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return crate::report::is_open();
    #[cfg(target_arch = "wasm32")]
    false
}

/// The report's picture of this frame, if its dialog wants one: the frame
/// read back before it is presented (where the surface lets it be copied),
/// as RGBA.
#[cfg(not(target_arch = "wasm32"))]
fn report_shot(g: &Gfx, frame: Option<&wgpu::SurfaceTexture>) {
    if !crate::report::wants_shot() {
        return;
    }
    let Some(t) = frame.map(|f| &f.texture).or(g.offscreen.as_ref()).filter(|t| t.usage().contains(wgpu::TextureUsages::COPY_SRC)) else { return };
    let mut rgba = read_back(&g.device, &g.queue, t);
    if matches!(t.format(), wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb) {
        for px in rgba.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
    }
    crate::report::set_shot(t.width(), t.height(), &rgba);
}

/// A key as the game's keyboard gives it (`VIKeyboard::Read`): 13 Enter,
/// 27 Escape, 8 Backspace, 9 Tab, 17-20 the cursor keys (left, right, up,
/// down), 21-24 Page Up, Page Down, End, Home; a character as itself.
fn pregame_key(event: &KeyEvent) -> Option<i32> {
    Some(match &event.logical_key {
        Key::Named(NamedKey::Enter) => 13,
        Key::Named(NamedKey::Escape) => 27,
        Key::Named(NamedKey::Backspace) => 8,
        Key::Named(NamedKey::Tab) => 9,
        Key::Named(NamedKey::Space) => 32,
        Key::Named(NamedKey::ArrowLeft) => 17,
        Key::Named(NamedKey::ArrowRight) => 18,
        Key::Named(NamedKey::ArrowUp) => 19,
        Key::Named(NamedKey::ArrowDown) => 20,
        Key::Named(NamedKey::PageUp) => 21,
        Key::Named(NamedKey::PageDown) => 22,
        Key::Named(NamedKey::End) => 23,
        Key::Named(NamedKey::Home) => 24,
        Key::Character(s) => {
            let c = s.chars().next()?;
            if !c.is_ascii() || c.is_ascii_control() {
                return None;
            }
            c as i32
        }
        _ => return None,
    })
}

/// Whether the touch screen's pad shows from the start: in a page on a
/// touch-first device (`window.eqoaTouchFirst`), or with `EQOA_TOUCH=1`.
/// Whether a key pressed before the world is the art's (`era`): F9, or the
/// key its row was moved onto on the Controls page (`binds`); F9 moved off
/// is not.
fn is_art_key(k: &Key) -> bool {
    let Some(name) = crate::binds::name_of(k) else { return false };
    match crate::settings::with_binds(|b| b.alias(&name)) {
        crate::binds::Alias::Same => name == "f9",
        crate::binds::Alias::To(to) => to == "f9",
        _ => false,
    }
}

fn touch_first() -> bool {
    #[cfg(target_arch = "wasm32")]
    if crate::web::touch_first() {
        return true;
    }
    std::env::var("EQOA_TOUCH").is_ok_and(|v| v == "1")
}

/// A key's name in the held set (`App::held`): the cursor block's and
/// Shift by name, a character as itself in lower case.
fn held_name(k: &Key) -> Option<String> {
    Some(match k {
        Key::Named(NamedKey::Shift) => "shift".to_string(),
        Key::Named(NamedKey::ArrowLeft) => "left".to_string(),
        Key::Named(NamedKey::ArrowRight) => "right".to_string(),
        Key::Named(NamedKey::ArrowUp) => "up".to_string(),
        Key::Named(NamedKey::ArrowDown) => "down".to_string(),
        Key::Named(NamedKey::PageUp) => "pageup".to_string(),
        Key::Named(NamedKey::PageDown) => "pagedown".to_string(),
        Key::Named(NamedKey::End) => "end".to_string(),
        Key::Named(NamedKey::Home) => "home".to_string(),
        Key::Character(c) => c.to_lowercase(),
        _ => return None,
    })
}

/// `EQOA_WALK=fwd,strafe,orbit,ms[;...]`: each set of controls held for its
/// ms, one after another, then none (`shot`'s script, in the window too).
fn walk_script() -> Option<Vec<(Controls, i64)>> {
    std::env::var("EQOA_WALK").ok().map(|w| {
        let mut end = 0;
        w.split(';')
            .filter_map(|seg| {
                let v: Vec<f32> = seg.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                (v.len() == 4).then(|| {
                    end += v[3] as i64;
                    (Controls { fwd: v[0], strafe: v[1], orbit: v[2], pitch: 0.0 }, end)
                })
            })
            .collect()
    })
}

/// `Perf::mark`, when measuring.
fn mark(perf: &mut Option<Perf>, phase: usize) {
    if let Some(p) = perf {
        p.mark(phase);
    }
}

/// Frames drawn so far (the browser page's rate readout).
pub static FRAMES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// A surface and device made before the event loop: a browser's, where the
/// adapter is the canvas's own (WebGL2) and asking for it is asynchronous.
/// `attrs` puts the window on the same canvas.
pub struct PendingGfx {
    pub attrs: winit::window::WindowAttributes,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

struct Gfx {
    window: Arc<Window>,
    /// None under `EQOA_OFFSCREEN`: frames go to `offscreen` and are never
    /// presented.
    surface: Option<wgpu::Surface<'static>>,
    offscreen: Option<wgpu::Texture>,
    /// The last offscreen frame's submission, waited for before the next is
    /// sent (one frame in flight, as a swap chain keeps it).
    in_flight: Option<wgpu::SubmissionIndex>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    renderer: Renderer,
    models: Vec<GpuModel>,
    /// The window's title as last set (`title`).
    title: String,
}

/// The window's title (F3 20261023-045744, Liav: several windows told
/// apart): "EQOA <n>", `n` this window's place among the client's windows
/// open, from 1 (`multibox::place`, the order Controller Focus's Auto reads
/// the controllers in), and in the world " - <character>". It never changes
/// while a window stays where it is.
fn title(name: Option<&str>) -> String {
    let n = crate::multibox::place() + 1;
    match name.filter(|s| !s.is_empty()) {
        Some(name) => format!("EQOA {n} - {name}"),
        None => format!("EQOA {n}"),
    }
}

impl Gfx {
    /// Names the window `t` (`title`), only when that is new; in a page the
    /// tab too.
    fn set_title(&mut self, t: String) {
        if self.title != t {
            self.window.set_title(&t);
            #[cfg(target_arch = "wasm32")]
            crate::web::set_title(&t);
            self.title = t;
        }
    }
}

/// `EQOA_PERF=1` (in a page, `?perf`): each frame's CPU time by phase,
/// summed over five seconds and printed; `EQOA_QUIT=<s>` ends the run after
/// that many seconds.
struct Perf {
    since: Instant,
    mark: Instant,
    frames: u32,
    worst: f64,
    /// seconds per phase since `since`
    total: [f64; Perf::NAMES.len()],
    /// the last frame's models and draw calls
    drawn: (u32, u32),
}

impl Perf {
    const NAMES: [&str; 15] = [
        "stream", "walk", "play", "pose", "zones", "sound", "weather", "particles", "lights", "acquire", "hud", "encode", "submit",
        "gpu wait", "volumes",
    ];

    fn from_env() -> Option<Perf> {
        #[cfg(target_arch = "wasm32")]
        let on = crate::web::page_flag("perf");
        #[cfg(not(target_arch = "wasm32"))]
        let on = std::env::var_os("EQOA_PERF").is_some();
        on.then(|| {
            let now = Instant::now();
            Perf { since: now, mark: now, frames: 0, worst: 0.0, total: [0.0; Perf::NAMES.len()], drawn: (0, 0) }
        })
    }

    fn begin(&mut self) {
        self.mark = Instant::now();
    }

    /// The time since the last mark goes to phase `i`.
    fn mark(&mut self, i: usize) {
        let now = Instant::now();
        self.total[i] += (now - self.mark).as_secs_f64();
        self.mark = now;
    }

    fn end(&mut self, frame_start: Instant) {
        self.frames += 1;
        self.worst = self.worst.max(frame_start.elapsed().as_secs_f64());
        let span = self.since.elapsed().as_secs_f64();
        if span < 5.0 {
            return;
        }
        let n = self.frames as f64;
        let cpu: f64 = self.total.iter().sum();
        let phases: Vec<String> = Perf::NAMES
            .iter()
            .zip(&self.total)
            .filter(|(_, t)| **t * 1e3 / n >= 0.005)
            .map(|(name, t)| format!("{name} {:.2}", t * 1e3 / n))
            .collect();
        crate::say(&format!(
            "perf: {:.1} fps, cpu {:.2} ms/frame (worst {:.1}), {} models in {} draws: {}",
            n / span,
            cpu * 1e3 / n,
            self.worst * 1e3,
            self.drawn.0,
            self.drawn.1,
            phases.join(", ")
        ));
        *self = Perf { since: Instant::now(), mark: Instant::now(), frames: 0, worst: 0.0, total: [0.0; Perf::NAMES.len()], drawn: (0, 0) };
    }
}

struct App {
    lib: Library,
    source: Source,
    bounds: (Vec3, Vec3),
    summary: String,
    camera: Camera,
    drag: Option<(f64, f64)>,
    cursor: (f64, f64),
    held: HashSet<String>,
    last: Instant,
    walker: Option<Walker>,
    /// On a server: the session and the entities it brings.
    play: Option<crate::play::Play>,
    /// The walk's zones, streamed around the character.
    streamer: Option<Streamer>,
    /// The game's sound, once walking (`EQOA_AUDIO=0`: driven, not heard).
    sound: Option<crate::sound::Sound>,
    /// The walking character's sounds, shared while it is the character.
    walker_sounds: Vec<u32>,
    /// The world's particles, and this frame's billboards.
    particles: Option<crate::particles::Particles>,
    /// The particles of the items in creatures' hands.
    held_fx: Option<crate::particles::Held>,
    batches: Vec<render::billboard::Batch>,
    /// The effect volumes round the walk (`fx.rs`).
    fx: Option<crate::fx::Fx>,
    /// The sky and the weather (made once, kept across worlds as the
    /// scene's is), and this frame's sky billboards.
    weather: Option<crate::weather::Weather>,
    sky_batches: Vec<render::billboard::Batch>,
    /// Index in `Gfx::models` of the character.
    body_at: usize,
    /// Where the sky's models start in `Gfx::models`.
    sky_at: Option<usize>,
    /// A world change under way: the world's file, the arrival, and its
    /// reads (`stream::open_world`), entered once they are in.
    entering: Option<(String, Spawn, crate::stream::Arrival)>,
    clock: Instant,
    gfx: Option<Gfx>,
    /// Made before the loop (a browser's), taken when it resumes.
    pending: Option<PendingGfx>,
    display: winit::event_loop::OwnedDisplayHandle,
    perf: Option<Perf>,
    /// `EQOA_QUIT`: when the run ends (s on `clock`)
    quit_at: Option<f64>,
    walk: Option<Vec<(Controls, i64)>>,
    /// The pad the game reads (`VIConsole`'s), the controllers behind it,
    /// and `EQOA_PAD`'s script.
    console: crate::input::Console,
    pads: crate::input::Gamepads,
    /// macOS: ⌘ held (`ModifiersChanged`), for ⌘, as a key (`macos::ask`)
    #[cfg(target_os = "macos")]
    super_held: bool,
    pad_script: Option<crate::input::Script>,
    /// The player's own keys and buttons (`binds`) at the input's edge: what
    /// each key pressed was made, and the controllers' buttons a capture took.
    keys_edge: crate::binds::KeyEdge,
    pad_edge: crate::binds::PadEdge,
    /// the sticks `UpdateControls` got last frame: the pad's, the keys', the
    /// mouse's and the touch screen's (for a report)
    sticks_used: [f32; 4],
    /// The mouse in play (`mouse.rs`), and the last frame's view (what a
    /// click picks through).
    mouse: crate::mouse::Mouse,
    last_view: Option<glam::Mat4>,
    /// The touch screen's pad (`touch.rs`).
    touch: crate::touch::TouchPad,
    /// The game's frames among those drawn (`frames.rs`): what the game
    /// counts in frames steps on these.
    frames: crate::frames::Frames,
    /// When the last frame was drawn, and how long it was (s): a mouse's
    /// drag pushes the stick for a time, not a frame (`mouse::look`).
    drawn_at: Option<Instant>,
    frame_s: f32,
    /// The game before the world (`pregame`): the loading screens, the
    /// title, Station's login, the server list and character select; gone
    /// once the world is entered.
    host: Option<crate::pregame::host::Host>,
    /// In a world entered from the game's own boot: the account and the
    /// server, for the way back (a logout, a lost connection).
    booted: Option<crate::pregame::Shared>,
    /// `EQOA_SHOTS` (`ms=file.png,...`): what was drawn then, as PNGs, under
    /// `EQOA_OFFSCREEN` (the pregame's frames and play's alike).
    shots: Vec<(i64, String)>,
    /// Why the loop ended before a window opened (natively: no disc image
    /// was picked), for `run_app` to return.
    failed: Option<String>,
    /// The 2002 disc's files being read for its art (`era`, F9), put in the
    /// library and drawn when they come.
    era_loading: Option<crate::era::Loading>,
}

pub fn run(lib: Library, source: Source) -> Result<(), Error> {
    run_with(lib, source, None)
}

/// `run`, with a game session to play on.
pub fn run_with(lib: Library, source: Source, play: Option<crate::play::Play>) -> Result<(), Error> {
    run_with_gfx(lib, source, play, None)
}

/// The game from its boot (`pregame`): the window opens on the launcher's
/// loading screen and plays on into the world.
pub fn run_boot(server: Option<String>) -> Result<(), Error> {
    run_boot_gfx(server, None)
}

/// `run_boot` on a surface made beforehand (a page's).
pub fn run_boot_gfx(server: Option<String>, pending: Option<PendingGfx>) -> Result<(), Error> {
    let host = crate::pregame::host::Host::new(crate::pregame::Pregame::new(server), 0);
    run_app(Library::new(), Source::Models { list: Vec::new(), index: 0 }, None, pending, Some(host))
}

/// `run_with` on a surface made beforehand. In a browser the loop is handed
/// to the page and this returns at once.
pub fn run_with_gfx(lib: Library, source: Source, play: Option<crate::play::Play>, pending: Option<PendingGfx>) -> Result<(), Error> {
    run_app(lib, source, play, pending, None)
}

fn run_app(lib: Library, source: Source, play: Option<crate::play::Play>, pending: Option<PendingGfx>, host: Option<crate::pregame::host::Host>) -> Result<(), Error> {
    let event_loop = EventLoop::new()?;
    let display = event_loop.owned_display_handle();
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut app = App {
        lib,
        source,
        bounds: (Vec3::ZERO, Vec3::ONE),
        summary: String::new(),
        camera: Camera::Orbit { yaw: 0.0, pitch: 0.0, zoom: 1.0 },
        drag: None,
        cursor: (0.0, 0.0),
        held: HashSet::new(),
        last: Instant::now(),
        walker: None,
        play,
        streamer: None,
        sound: None,
        walker_sounds: Vec::new(),
        particles: None,
        held_fx: None,
        batches: Vec::new(),
        fx: None,
        weather: None,
        sky_batches: Vec::new(),
        body_at: 0,
        sky_at: None,
        entering: None,
        clock: Instant::now(),
        gfx: None,
        pending,
        display,
        perf: Perf::from_env(),
        quit_at: std::env::var("EQOA_QUIT").ok().and_then(|s| s.parse().ok()),
        walk: walk_script(),
        console: crate::input::Console::new(),
        pads: crate::input::Gamepads::new(),
        #[cfg(target_os = "macos")]
        super_held: false,
        pad_script: crate::input::Script::from_env(),
        keys_edge: Default::default(),
        pad_edge: Default::default(),
        sticks_used: [0.0; 4],
        mouse: crate::mouse::Mouse::default(),
        last_view: None,
        touch: crate::touch::TouchPad::new(touch_first()),
        frames: crate::frames::Frames::default(),
        drawn_at: None,
        frame_s: 1.0 / 60.0,
        host,
        booted: None,
        shots: crate::pregame::host::shot_script(),
        failed: None,
        era_loading: None,
    };
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(app);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        event_loop.run_app(&mut app)?;
        if let Some(e) = app.failed.take() {
            return Err(e.into());
        }
    }
    Ok(())
}

impl App {
    /// (Re)builds the scene for the current source and uploads it.
    fn load(&mut self) {
        let s = self.source.scene(&self.lib);
        eprintln!("{}", s.summary);
        self.bounds = s.bounds;
        self.summary = s.summary.clone();
        if !matches!(self.camera, Camera::Orbit { .. }) || matches!(self.source, Source::Zone { .. }) || self.gfx.is_none() {
            self.camera = self.source.camera(s.bounds);
        }
        let walker = self.source.walker(&mut self.lib, &s);
        if let Some(g) = &mut self.gfx {
            g.models = if walker.is_some() { Vec::new() } else { upload(&mut g.renderer, &g.device, &g.queue, &self.lib, &s) };
            self.streamer = None;
            self.walker = walker.map(|(w, body, mut streamer)| {
                let textures = render::library_textures(&self.lib);
                let mut w = w;
                let (model, _) = w.pose();
                self.body_at = g.models.len();
                g.models.push(g.renderer.upload(&g.device, &g.queue, &body, &[model], &textures));
                let at = Vec3::from(w.cam.walk.phys.position.to_array());
                let gpu = stream::Gpu { renderer: &mut g.renderer, device: &g.device, queue: &g.queue };
                streamer.update(at, &mut w.scene, &self.lib, gpu, None);
                eprintln!("{}", streamer.describe());
                self.streamer = Some(streamer);
                self.particles = Some(crate::particles::Particles::new(&w.scene, 0));
                self.fx = crate::fx::enabled().then(crate::fx::Fx::new);
                w
            });
            self.sky_at = None;
            if self.walker.is_some() {
                self.sky_at = crate::sky::upload(&mut g.renderer, &g.device, &g.queue, &self.lib, &mut g.models);
                if self.weather.is_none() {
                    self.weather = Some(crate::weather::Weather::new(&mut self.lib, 0));
                }
                match &mut self.sound {
                    Some(s) => s.world_changed(),
                    None => self.sound = crate::sound::make(std::path::Path::new(&crate::disc_path(None))),
                }
            }
            // On a server the game's clock runs on through a world change, as
            // VITimer does: what plays was started on it (the creatures'
            // animations), and going back would put them before their start.
            // The walk skips the gap since its spawn, as the console's does.
            if self.play.is_none() {
                self.clock = Instant::now();
            }
            report(&g.renderer);
            g.set_title(title(self.play.as_ref().map(|p| p.player.name.as_str())));
            g.window.request_redraw();
        }
    }

    /// A world change in the window, once its reads are in
    /// (`stream::open_world`): the file replaces the world's, the zones read
    /// round the arrival go in, and `load` begins the walk again there.
    fn enter_world(&mut self, file: &str, c: esf::Container, zones: Vec<(usize, Vec<u8>)>, at: Spawn) -> Result<(), Error> {
        self.lib.replace(0, file, c);
        for (zi, b) in zones {
            stream::install(&mut self.lib, 0, zi, b)?;
        }
        let index = vi::world::World::<vi::fpu::Ee>::zone_at(self.lib.file(0), at.pos[0], at.pos[2]).ok_or("no zone at the arrival")?;
        stream::load_zone_now(&mut self.lib, 0, index)?;
        let Source::Walk { character, .. } = self.source else { return Err("not walking".into()) };
        self.source = Source::Walk { file: 0, index, character, spawn: Some(at) };
        self.load();
        Ok(())
    }

    /// F9 (`era`): the art swapped between Frontiers' and the 2002 disc's,
    /// wherever the window is, and the choice kept. With no 2002 disc at hand
    /// it is asked for first (natively the file dialog, in a page the
    /// page's), and nothing changes without one.
    fn toggle_era(&mut self) {
        let want = crate::era::other(crate::era::current());
        if want == esf::Era::Y2002 && !crate::era::available() && !crate::era::ask() {
            return;
        }
        // what was held is let go: a dialog may have taken the release
        self.held.clear();
        self.keys_edge.clear();
        crate::era::set(want);
        crate::say(&format!("art: {}", crate::era::line(want)));
        self.apply_era();
    }

    /// The art `era::current` names, drawn: in the world (or a model
    /// viewer) the library's view turned and what is drawn made again in it,
    /// the 2002 disc's files read first when the library has none of them
    /// yet (`poll_era` comes back here once they are in); before the world,
    /// the pregame's (`Host::art_changed`).
    fn apply_era(&mut self) {
        let want = crate::era::current();
        if let Some(h) = self.host.as_mut() {
            h.art_changed(want);
        }
        if self.host.is_some() {
            return;
        }
        if want == esf::Era::Y2002 && !self.lib.has_2002() && crate::era::available() {
            if self.era_loading.is_none() {
                self.era_loading = Some(crate::era::load(crate::era::names_of(&self.lib)));
            }
            return;
        }
        if self.lib.view() == want {
            return;
        }
        self.lib.set_view(want);
        self.rebuild_art(want);
    }

    /// The 2002 disc's files, once read: stood in for their namesakes, then
    /// the art applied.
    fn poll_era(&mut self) {
        let Some(r) = self.era_loading.as_mut().and_then(|l| l.poll()) else { return };
        self.era_loading = None;
        match r {
            Ok(art) => {
                let n = crate::era::install(&mut self.lib, art);
                crate::say(&format!("art: {n} of the 2002 disc's files stand in for Frontiers'"));
                self.apply_era();
            }
            Err(e) => crate::say(&format!("art: the 2002 disc could not be read: {e}")),
        }
    }

    /// What is drawn made again in the art `era`, the library's view already
    /// turned: the creatures as they are next applied (`Play::art_changed`),
    /// the world (`swap_world_art`); a model viewer's scene at once.
    fn rebuild_art(&mut self, era: esf::Era) {
        if let Some(p) = self.play.as_mut() {
            p.art_changed(era);
        }
        if self.walker.is_some() {
            self.swap_world_art(era);
        } else {
            self.load();
        }
    }

    /// The world itself in the art `era`: Tunaria off the disc of that art
    /// (the walk kept where it is). Not yet: the world stays Frontiers'.
    fn swap_world_art(&mut self, _era: esf::Era) {}

    fn step(&mut self, by: isize) {
        if let Source::Models { list, index, .. } = &mut self.source {
            *index = (*index as isize + by).rem_euclid(list.len() as isize) as usize;
            self.load();
        }
    }

    /// The keyboard's axes `UpdateControls` reads (`m_keys[17..=24]`, the
    /// codes `VIKeyboard::Read` gives the cursor block: 17 left, 18 right,
    /// 19 up, 20 down, 21 page up, 22 page down, 23 end, 24 home), held or
    /// not: Home and End orbit the follow camera, Page Up and Page Down pitch
    /// or zoom. The arrows, which walk and turn in the game, stay 0: here
    /// WASD walks and the arrows are the quick menus' d-pad (the user's
    /// choice, 2026-10-20; the game has W/S/A/D on the quick menus).
    fn game_keys(&self) -> [f32; 8] {
        if self.play.as_ref().is_some_and(|p| p.is_dead()) {
            return [0.0; 8];
        }
        let h = |k: &str| self.held.contains(k) as i32 as f32;
        [0.0, 0.0, 0.0, 0.0, h("pageup"), h("pagedown"), h("end"), h("home")]
    }

    /// The walker's controls from the held keys (a pad's sticks): W/S
    /// forward, A/D strafe, the client's own; I/J/K/L a right stick (orbit
    /// and zoom), which the game's keyboard leaves free (its own camera keys
    /// are Home, End, Page Up and Page Down, `game_keys`).
    fn controls(&self) -> Controls {
        // dead: `UpdateControls` is not run
        if self.play.as_ref().is_some_and(|p| p.is_dead()) {
            return Controls::default();
        }
        if let Some(segs) = &self.walk {
            let t = self.clock.elapsed().as_millis() as i64;
            return segs.iter().find(|s| t <= s.1).map_or(Controls::default(), |s| s.0);
        }
        let k = |a: &str, b: &str| self.held.contains(a) as i32 as f32 - self.held.contains(b) as i32 as f32;
        Controls { fwd: k("w", "s"), strafe: k("d", "a"), orbit: k("l", "j"), pitch: k("k", "i") }
    }

    /// The game's UI sounds asked for outside a window of its own (the
    /// update's question's).
    #[cfg(not(target_arch = "wasm32"))]
    fn ui_cues(&mut self, cues: Vec<i32>) {
        if let Some(s) = self.sound.as_mut() {
            for c in cues {
                s.cue(&crate::sound::Cue::Ui(c));
            }
        }
    }

    /// The pad's keys pressed for the update's question while it is up
    /// (its own: ✕, △, the d-pad) taken by it, and a stick's up and down
    /// its list's; the rest, and every release, go on.
    #[cfg(not(target_arch = "wasm32"))]
    fn offer_pad(&mut self, keys: Vec<(i32, bool)>) -> Vec<(i32, bool)> {
        if !crate::update::offer_shown() {
            return keys;
        }
        let now = self.clock.elapsed().as_millis() as i64;
        let s = self.console.sticks();
        crate::update::offer_stick(if s[3].abs() >= s[1].abs() { s[3] } else { s[1] }, self.frame_s, now);
        let mut rest = Vec::new();
        for (k, down) in keys {
            // a stick leaving or coming back to rest (128-131) is the list's
            // scrolling, not a key for what is behind
            if (128..=131).contains(&k) {
                continue;
            }
            match down.then(|| crate::update::offer_key(k, now)).flatten() {
                Some(cues) => self.ui_cues(cues),
                None => rest.push((k, down)),
            }
        }
        rest
    }

    /// A press on the update's question (a choice, or its list to drag):
    /// true when it took it.
    #[cfg(not(target_arch = "wasm32"))]
    fn offer_click(&mut self) -> bool {
        let Some(g) = &self.gfx else { return false };
        let p = render::ui::frame_from_window((g.config.width, g.config.height), [self.cursor.0 as f32, self.cursor.1 as f32]);
        let Some(cues) = crate::update::offer_click(p, self.clock.elapsed().as_millis() as i64) else { return false };
        self.ui_cues(cues);
        true
    }

    /// The pointer moved: the update's question's list dragged, if a press
    /// began a drag on it. True while it does.
    #[cfg(not(target_arch = "wasm32"))]
    fn offer_drag(&mut self) -> bool {
        let Some(g) = &self.gfx else { return false };
        let p = render::ui::frame_from_window((g.config.width, g.config.height), [self.cursor.0 as f32, self.cursor.1 as f32]);
        crate::update::offer_drag(p, self.clock.elapsed().as_millis() as i64)
    }

    /// The pad, once a frame: every source as libpad's one port
    /// (`VIPad::Read`), its mode to the client (`UpdateControls`' opening),
    /// and `PadKeys`' keys to the window that has them (`SetKey`).
    fn poll_pad(&mut self) {
        let (mut report, mut connected, motors) = self.pads.poll();
        // a controller in use: the touch screen's pad goes
        if report.buttons != 0 || report.sticks.iter().any(|v| v.abs() > 0.5) {
            self.touch.hide();
        }
        if let Some(s) = &self.pad_script {
            report.merge(&s.at(self.clock.elapsed().as_millis() as i64));
            connected = true;
        }
        // the player's own buttons (`binds`, the client's): the controllers'
        // (and a script's, which stands for one) on the game's fields; while
        // the Controls page waits for a button the one pressed is its, and
        // the game sees nothing of it. The touch screen's pad draws the
        // game's glyphs, and stays as it is.
        let capturing = self.play.as_ref().is_some_and(|p| p.capturing().is_some());
        let (fields, took) = crate::settings::with_binds(|b| self.pad_edge.frame(report.buttons, capturing, b));
        report.buttons = fields;
        if let (Some(i), Some(p)) = (took, self.play.as_mut()) {
            p.capture_button(i);
        }
        if self.touch.used {
            report.merge(&self.touch.report());
            connected = true;
        }
        let dead = self.play.as_ref().is_some_and(|p| p.is_dead());
        let keys = self.console.poll(report, connected, motors, dead);
        #[cfg(not(target_arch = "wasm32"))]
        let keys = self.offer_pad(keys);
        // `EQOA_PAD_TRACE=1`: every pad key, as the game gets it
        if crate::input::trace() {
            for &(k, down) in &keys {
                crate::say(&format!("[pad] {} {}", crate::input::key_name(k), if down { "down" } else { "up" }));
            }
        }
        let aspect = self.gfx.as_ref().map_or(4.0 / 3.0, |g| g.config.width as f32 / g.config.height as f32);
        if let (Some(p), Some(w)) = (self.play.as_mut(), self.walker.as_mut()) {
            p.pad_mode(self.console.mode());
            for (k, down) in keys {
                p.set_key(k, down, w, aspect);
            }
        }
    }

    /// The sticks `UpdateControls` reads: the keyboard's (`controls`), the
    /// pad's and the mouse's drag (`Mouse::look`, which moves an orbiting
    /// camera itself), axis by axis the one further from rest.
    fn sticks(&mut self) -> [f32; 4] {
        // the map up (`map.rs`): a finger's drag and pinch are its, the walk
        // stands (the pad's sticks go to it in `Play::map_frame`)
        if let Some(p) = self.play.as_mut().filter(|p| p.map.open) {
            let (tx, ty, pinch) = self.touch.take_look();
            p.map.touch(tx, ty, pinch);
            self.mouse.discard();
            return [0.0; 4];
        }
        let mut a = self.controls().axes();
        // the pointers' looks, taken every frame, applied only in play with
        // the camera free
        let (tx, ty, pinch) = self.touch.take_look();
        let dead = self.play.as_ref().is_some_and(|p| p.is_dead());
        // the update's question up (`update`) is a window too: no look
        let offer = crate::update::offer_shown();
        let free = !dead && !offer && self.play.as_ref().is_none_or(|p| p.look_free());
        let aim = !dead && !offer && self.play.as_ref().is_some_and(|p| p.aiming());
        let inverted = self.play.as_ref().is_some_and(|p| p.options.inverted != 0);
        let ((look, zoom), feel) = match self.walker.as_mut() {
            Some(w) if free => (self.mouse.look(w, inverted, self.frame_s), crate::mouse::look(w, tx, ty, pinch, inverted, self.frame_s)),
            // the manual camera (L1 or R1 held): the mouse's and a finger's
            // drags look with it (the client's, `mouse::aim`); the wheel is
            // nothing
            Some(w) if aim => {
                let (dx, dy) = self.mouse.take_look();
                crate::mouse::aim(&mut w.cam, dx as f32 + tx, dy as f32 + ty, inverted);
                (([0.0; 2], crate::mouse::Zoom::None), [0.0; 2])
            }
            _ => {
                // a window up: the wheel and a finger's drag are its up and down
                let size = self.gfx.as_ref().map_or((640, 448), |g| (g.config.width, g.config.height));
                let lines = self.mouse.take_wheel() + crate::touch::drag_lines(ty, size);
                self.mouse.discard();
                if let Some(p) = self.play.as_mut() {
                    p.window_wheel(lines);
                }
                (([0.0; 2], crate::mouse::Zoom::None), [0.0; 2])
            }
        };
        // the wheel past the closest zoom: first person, and back
        if let (Some(p), Some(w)) = (self.play.as_mut(), self.walker.as_mut()) {
            p.wheel_style(w, zoom, &mut self.mouse.back);
        }
        if dead {
            return a;
        }
        // both mouse buttons: run forward (`Mouse::running`)
        if free && self.mouse.running() {
            a[3] = -1.0;
        }
        // (and its list has the pad's sticks: the walk does not)
        let pad = if offer { [0.0; 4] } else { self.console.sticks() };
        for (i, a) in a.iter_mut().enumerate() {
            let (m, t) = if i < 2 { (look[i], feel[i]) } else { (0.0, 0.0) };
            for v in [pad[i], m, t] {
                if v * v > *a * *a {
                    *a = v;
                }
            }
        }
        a
    }

    /// The mouse look's hold on the pointer (`mouse::Grab`): taken while a
    /// held button drags in play with the camera free, given back when the
    /// drag ends or the look is taken (a window, the start menu, a death);
    /// once a frame (`frame`) a page's lock is looked at, which the browser
    /// ends itself on Escape.
    fn grab_pointer(&mut self, frame: bool) {
        let dead = self.play.as_ref().is_some_and(|p| p.is_dead());
        let free = self.walker.is_some() && !dead && self.play.as_ref().is_none_or(|p| p.look_free() || p.aiming()) && !report_open() && !crate::update::offer_shown();
        let Some(g) = &self.gfx else { return };
        #[cfg(target_arch = "wasm32")]
        if frame && self.mouse.lock_seen(crate::web::pointer_locked(), Instant::now()) {
            crate::mouse::give_pointer(&g.window, self.mouse.home());
        }
        let _ = frame;
        match self.mouse.grab_change(free) {
            Some(true) => {
                let grab = crate::mouse::take_pointer(&g.window).unwrap_or(crate::mouse::Grab::Refused);
                self.mouse.set_grab(grab);
            }
            Some(false) => crate::mouse::give_pointer(&g.window, self.mouse.home()),
            None => {}
        }
    }

    /// A click in play (a finger's tap with `finger`): a window takes it
    /// first (`Play::point`); else the creature under it targeted (through
    /// the last frame's view), and with a double or right click talked to.
    fn click(&mut self, c: crate::mouse::Click, finger: bool) {
        let (Some(p), Some(g), Some(vp)) = (self.play.as_mut(), self.gfx.as_ref(), self.last_view) else { return };
        let size = (g.config.width, g.config.height);
        if p.point(c, size, finger) {
            return;
        }
        p.click(p.creature_at(c.at(), size, vp), !matches!(c, crate::mouse::Click::Target(..)));
    }

    /// Fly-camera movement from the held keys, scaled by frame time.
    fn fly(&mut self) -> bool {
        let dt = self.last.elapsed().as_secs_f32().min(0.1);
        self.last = Instant::now();
        if self.walker.is_some() {
            return false;
        }
        let Camera::Fly { pos, yaw, pitch } = &mut self.camera else { return false };
        let fwd = direction(*yaw, *pitch);
        let right = fwd.cross(Vec3::Y).normalize_or_zero();
        let mut v = Vec3::ZERO;
        for k in &self.held {
            v += match k.as_str() {
                "w" => fwd,
                "s" => -fwd,
                "d" => right,
                "a" => -right,
                "e" => Vec3::Y,
                "q" => -Vec3::Y,
                _ => Vec3::ZERO,
            };
        }
        let speed = if self.held.contains("shift") { 400.0 } else { 80.0 };
        *pos += v * speed * dt;
        v != Vec3::ZERO
    }

    /// What a report says of the client (web/report.js's in a page, natively
    /// `report`'s): the frames, the surface, the files in the library and
    /// play's own.
    fn report_state(&self) -> String {
        use std::fmt::Write;
        let mut o = String::new();
        let _ = writeln!(o, "frames {}, clock {:.1} s", FRAMES.load(std::sync::atomic::Ordering::Relaxed), self.clock.elapsed().as_secs_f64());
        if let Some(g) = &self.gfx {
            let _ = writeln!(o, "surface {}x{} {:?}", g.config.width, g.config.height, g.config.format);
        }
        let _ = writeln!(o, "files: {}", self.lib.files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>().join(", "));
        let _ = writeln!(o, "options, extended: {}", crate::settings::describe());
        let _ = writeln!(o, "keys and buttons: {}", crate::settings::with_binds(|b| b.describe()));
        let _ = writeln!(o, "the server's patch: {}", crate::servers::report_line(self.booted.as_ref().and_then(|s| s.server.as_deref())));
        // the sound: what plays, the ambient track and the terrain profile
        // it was chosen by, and the two classes' volumes (Fx, Music)
        if let Some(s) = &self.sound {
            let _ = writeln!(o, "sound: {}", s.describe());
        }
        // the controls: a pad's sticks as they came (before the host's dead
        // zone), as the game reads them and with the keys, the mouse and the
        // touch screen's merged in (what `UpdateControls` got), the camera's
        // mode; each pad as the browser gives it; what else is held
        let mode = self.walker.as_ref().map_or(-1, |w| w.cam.camera_mode);
        let _ = writeln!(
            o,
            "sticks rx ry lx ly: pad {:.3?} (dead zone {}), the game's {:.4?}, with the keys, mouse and touch {:.4?}; camera mode {mode}",
            self.pads.raw,
            self.pads.dead_zone,
            self.console.sticks(),
            self.sticks_used
        );
        #[cfg(target_arch = "wasm32")]
        for d in &self.pads.described {
            let _ = writeln!(o, "  {d}");
        }
        let mut keys: Vec<&str> = self.held.iter().map(String::as_str).collect();
        keys.sort_unstable();
        let _ = writeln!(o, "keys held [{}]; mouse {}; touch pad {}", keys.join(", "), self.mouse.describe(), if self.touch.used { "in use" } else { "unused" });
        // the body and the camera, for a report of either moving on its own
        if let Some(w) = &self.walker {
            let (p, c) = (&w.cam.walk.phys, &w.cam.cur);
            let anim = w.sprite.as_ref().map(|s| [s.cur[0].anim, s.cur[1].anim]);
            let v = p.velocity;
            let _ = writeln!(
                o,
                "body: heading {:.4} (target {:.4}), velocity [{:.3}, {:.3}, {:.3}], flags {:#x}, loco {}, anim {anim:?}; camera: orbit {:.4}, pitch {:.4}, dist {:.2}, flags {:#x}",
                p.angles.x.0,
                p.target_heading.0,
                v.x.0,
                v.y.0,
                v.z.0,
                p.flags,
                p.loco_state,
                c.orbit.x.0,
                c.orbit.y.0,
                c.dist.0,
                c.flags
            );
            let cam = &w.cam;
            let _ = writeln!(o, "camera: distance {:.2} of {:.2} (chosen {:.2}), pitch {:.3} (default {:.3})", cam.cur.dist.0, cam.max_orbit_distance.0, w.reach.chosen(cam).0, cam.cur.orbit.y.0, cam.def_orbit_pitch.0);
        }
        if let Some(e) = &self.entering {
            let _ = writeln!(o, "entering {} at {:?}", e.0, e.1.pos);
        }
        // the hour the lights and the effect volumes take, and the volumes
        let _ = writeln!(o, "hour {:.2}{}; {}", game_hour(self.play.as_ref(), self.clock), if self.play.as_ref().is_some_and(|p| p.session.clock_b.ready) { " (the server's)" } else { "" }, self.fx.as_ref().map_or("effect volumes off".into(), |f| f.describe()));
        if let (Some(p), Some(w)) = (&self.play, &self.walker) {
            o.push_str(&p.report(w.cam.walk.phys.position.to_array()));
        }
        o
    }

    /// The renderer on a configured surface (or none: an offscreen target of
    /// the configuration's size), then the scene.
    fn attach(&mut self, window: Arc<Window>, surface: Option<wgpu::Surface<'static>>, config: wgpu::SurfaceConfiguration, device: wgpu::Device, queue: wgpu::Queue) {
        // in front or not as it opens: a window opened behind another (the
        // system can refuse a program the front) has no `Focused` to say so
        self.pads.focused = window.has_focus();
        // a device lost closes the window for a start again (`gpulost`; a
        // page's is `web::start`'s)
        #[cfg(not(target_arch = "wasm32"))]
        crate::gpulost::watch(&device);
        let mut renderer = Renderer::new(&device, &queue, config.format);
        if let Some(f) = arial(&self.lib) {
            renderer.set_font(&device, &queue, &f);
        }
        if let Some(p) = self.play.as_mut() {
            p.attach_ui(&mut renderer, &device, &queue, config.format, &self.lib);
        }
        let depth = render::depth_view(&device, config.width, config.height);
        let offscreen = surface.is_none().then(|| offscreen_target(&device, &config));
        self.gfx = Some(Gfx { window, surface, offscreen, in_flight: None, device, queue, config, depth, renderer, models: Vec::new(), title: "EQOA".into() });
        // before the world there is nothing to load: the pregame draws
        if self.host.is_some() {
            if let Some(g) = &self.gfx {
                g.window.request_redraw();
            }
            return;
        }
        self.load();
    }

    /// The pad before the world: every source as libpad's one port, its keys
    /// to the pregame's window (`VIWindowManager::Update`'s 128..).
    fn poll_pad_host(&mut self) {
        let (mut report, mut connected, motors) = self.pads.poll();
        if report.buttons != 0 || report.sticks.iter().any(|v| v.abs() > 0.5) {
            self.touch.hide();
        }
        if let Some(s) = &self.pad_script {
            report.merge(&s.at(self.clock.elapsed().as_millis() as i64));
            connected = true;
        }
        // the player's own buttons, and a capture's for the title's Controls page
        let capturing = self.host.as_ref().is_some_and(|h| h.pre.setup.capturing().is_some());
        let (fields, took) = crate::settings::with_binds(|b| self.pad_edge.frame(report.buttons, capturing, b));
        report.buttons = fields;
        if let (Some(i), Some(h)) = (took, self.host.as_mut()) {
            h.pre.setup.capture_button(i);
        }
        if self.touch.used {
            report.merge(&self.touch.report());
            connected = true;
        }
        let keys = self.console.poll(report, connected, motors, false);
        #[cfg(not(target_arch = "wasm32"))]
        let keys = self.offer_pad(keys);
        let Some(h) = self.host.as_mut() else { return };
        for (k, down) in keys {
            if crate::input::trace() {
                crate::say(&format!("[pad] {} {}", crate::input::key_name(k), if down { "down" } else { "up" }));
            }
            h.key(k, down);
        }
    }

    /// One frame before the world: the pregame's windows on the 2D layer
    /// over a black frame (the loading screen's gears while the world's files
    /// come), then the world entered once they are in.
    fn host_frame(&mut self, el: &ActiveEventLoop, game_frame: bool) {
        self.poll_pad_host();
        if self.sound.is_none() {
            self.sound = crate::sound::make(std::path::Path::new(&crate::disc_path(None)));
        }
        let now = self.clock.elapsed().as_millis() as i64;
        let Some(g) = &mut self.gfx else { return };
        let Some(host) = self.host.as_mut() else { return };
        let size = (g.config.width, g.config.height);
        let done = host.frame(&mut g.renderer, &g.device, &g.queue, g.config.format, size, now, game_frame, self.sound.as_mut());
        // the update's question, then the report's dialog, over the game's
        // screens (`update`, `report`)
        #[cfg(not(target_arch = "wasm32"))]
        report_test(now);
        #[cfg(not(target_arch = "wasm32"))]
        if let (Some((vui, assets)), Some(ui)) = (host.ui_parts(), g.renderer.ui.as_mut()) {
            let mut r = assets.raster(ui);
            r.ui.anchor = render::ui::Anchor::Middle;
            let quiet = host.pre.quiet() && !crate::report::is_open();
            crate::update::offer_frame(crate::update::Moment { quiet, plain: quiet, world: false, now });
            crate::update::offer_draw(vui, &mut r, now);
            crate::report::draw(vui, &mut r);
        }
        g.set_title(title(None));
        let frame = match &g.surface {
            Some(surface) => match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => Some(f),
                _ => {
                    surface.configure(&g.device, &g.config);
                    return;
                }
            },
            None => None,
        };
        let target = match (&frame, &g.offscreen) {
            (Some(f), _) => f.texture.create_view(&Default::default()),
            (None, Some(t)) => t.create_view(&Default::default()),
            (None, None) => return,
        };
        let mut enc = g.device.create_command_encoder(&Default::default());
        // the frame cleared black (`SetClearColor(0, 0, 0, 1)`), with CLIENT's
        // loading screen's gears in its corner while it is up
        let gears: Vec<render::Draw> = host.gears().into_iter().map(render::Draw::Model).collect();
        let view = if gears.is_empty() { render::View { clear: Vec3::ZERO, ..render::View::daylight(glam::Mat4::IDENTITY) } } else { crate::loading::view(size) };
        g.renderer.draw(&g.queue, &mut enc, &target, &g.depth, &view, &gears);
        // a window's 3D (character select's preview, creation's model): the
        // 2D made before it under it, the rest over it
        let split = g.renderer.ui.as_ref().and_then(|u| u.split.map(|s| (s, u.quads.len())));
        match (split, host.pre.scene()) {
            (Some((s, n)), Some(scene)) => {
                g.renderer.draw_ui(&g.device, &mut enc, &target, &g.depth, size, 0..s);
                // the pixels a 2D quad over the viewport covers: a plate over it hides all of it
                let [x, y, w, h] = crate::pregame::stage::viewport_pixels(size, scene.viewport);
                if w > 0 && h > 0 {
                    let draws: Vec<render::Draw> = scene.models.iter().map(|&m| render::Draw::Model(m)).collect();
                    g.renderer.draw_viewport(&g.queue, &mut enc, &target, &g.depth, &scene.view, &draws, [x, y, w, h]);
                }
                g.renderer.draw_ui(&g.device, &mut enc, &target, &g.depth, size, s..n);
            }
            _ => g.renderer.draw_text(&g.device, &mut enc, &target, &g.depth, size),
        }
        g.queue.submit([enc.finish()]);
        #[cfg(not(target_arch = "wasm32"))]
        report_shot(g, frame.as_ref());
        shoot(&mut self.shots, g, now, &host.pre.describe());
        match frame {
            Some(f) => g.queue.present(f),
            None => {
                let _ = g.device.poll(wgpu::PollType::wait_indefinitely());
            }
        }
        // The page's picture of this frame, if it asked, and the report's
        // state of the client (`report`): which of the game's screens.
        crate::report::frame_drawn();
        crate::report::note_state(|| format!("pregame: {}\nsurface {}x{} {:?}\noptions, extended: {}\nkeys and buttons: {}\nthe server's patch: {}", host.pre.describe(), size.0, size.1, g.config.format, crate::settings::describe(), crate::settings::with_binds(|b| b.describe()), crate::servers::report_line(host.pre.shared.server.as_deref())));
        match done {
            crate::pregame::host::Done::Stay => {}
            crate::pregame::host::Done::Quit => el.exit(),
            crate::pregame::host::Done::Play(entered, lib, src) => self.begin_play(entered, lib, src),
        }
    }

    /// Back from the world to the game's own screens (`Msg0777Done`): the
    /// world let go (`UnloadWorld`), and the pregame at CLIENT again, its
    /// files read anew under its loading screen, then the server list (a
    /// logout) or the lost connection's notice.
    fn back_to_pregame(&mut self, shared: crate::pregame::Shared) {
        let disconnect = self.play.as_ref().and_then(|p| if p.quit { None } else { p.disconnected });
        crate::say(&match disconnect {
            None => "logged out: back to the server list".to_string(),
            Some(r) => format!("disconnected (reason {r}): back to the game's screens"),
        });
        self.play = None;
        self.walker = None;
        self.streamer = None;
        self.particles = None;
        self.held_fx = None;
        self.fx = None;
        self.batches.clear();
        self.sky_batches.clear();
        self.entering = None;
        self.sky_at = None;
        if let Some(s) = self.sound.as_mut() {
            s.world_changed();
            s.battle_music(None);
            s.release_sounds(&self.walker_sounds);
        }
        self.walker_sounds.clear();
        if let Some(g) = &mut self.gfx {
            g.models.clear();
            g.renderer.ui = None;
            if let Some(t) = g.renderer.text.as_mut() {
                t.labels.clear();
                t.plates.clear();
            }
            g.renderer.strips.clear();
            g.renderer.set_overlay(&g.device, &g.queue, glam::Mat4::IDENTITY, &[], &|_| None);
            g.window.request_redraw();
        }
        self.lib = Library::new();
        self.source = Source::Models { list: Vec::new(), index: 0 };
        self.grab_pointer(false);
        let now = self.clock.elapsed().as_millis() as i64;
        self.host = Some(crate::pregame::host::Host::new(crate::pregame::Pregame::back(shared, disconnect, now), now));
    }

    /// The world entered from the pregame: its library and walk (`main.rs`'s
    /// `world_library`), and `Play` on the session, as `eqoa play` starts
    /// them.
    fn begin_play(&mut self, entered: Box<crate::pregame::Entered>, lib: Library, src: Source) {
        let crate::pregame::Entered { session, player, dump } = *entered;
        crate::say(&format!("in the world as {} after the pregame", player.name));
        let mut play = crate::play::Play::new(session, player, dump);
        play.booted = true;
        // the game's words and the HUD's texture names, read with the pregame's files
        if let Some(h) = &self.host {
            // the server's folder's copy first (`s2`: the patcher's)
            play.words = crate::s2::get().words().or_else(|| h.pre.res.words.clone());
            play.ui_textures = h.pre.res.textures.clone();
        }
        if let (Some(h), Some(s)) = (self.host.as_mut(), self.sound.as_mut()) {
            h.stop_music(s);
        }
        self.booted = self.host.take().map(|h| h.pre.shared);
        self.host = None;
        self.lib = lib;
        self.source = src;
        self.play = Some(play);
        if let Some(g) = &mut self.gfx {
            // the name plates' font, as `attach` sets it
            if let Some(f) = arial(&self.lib) {
                g.renderer.set_font(&g.device, &g.queue, &f);
            }
            // the pregame's 2D layer goes; the HUD makes its own
            g.renderer.ui = None;
        }
        self.load();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        // macOS: Settings… (⌘,) in the app menu winit made (`macos`), once
        #[cfg(target_os = "macos")]
        crate::macos::add_settings_item();
        if self.gfx.is_some() {
            return;
        }
        // natively, the game from its boot with no disc image at hand: the
        // player picks it before the window opens (`discs::settle`), and its
        // CLIENT and SUPPORT start coming off it
        #[cfg(not(target_arch = "wasm32"))]
        if self.host.is_some() && crate::discs::unsettled() {
            match crate::discs::settle() {
                Ok(iso) => {
                    crate::modules::load(std::path::Path::new(&iso));
                    // the pregame asked for its first files before there was
                    // a disc to read them off: it begins again, on this one
                    if let Some(h) = self.host.take() {
                        self.host = Some(crate::pregame::host::Host::new(crate::pregame::Pregame::new(h.pre.shared.server), 0));
                    }
                }
                Err(e) => {
                    self.failed = Some(e);
                    el.exit();
                    return;
                }
            }
        }
        if let Some(p) = self.pending.take() {
            let window = Arc::new(el.create_window(p.attrs).expect("window"));
            self.attach(window, Some(p.surface), p.config, p.device, p.queue);
            return;
        }
        let window = Arc::new(el.create_window(Window::default_attributes().with_title("EQOA")).expect("window"));
        // `EQOA_OFFSCREEN=WxH`: drawn on the adapter the environment names
        // into a texture of that size, never presented. A GPU with no display
        // to present on (an X server without DRI3) measures as it plays.
        if let Some((w, h)) = std::env::var("EQOA_OFFSCREEN").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
        }) {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
            let adapter = pollster::block_on(instance.request_adapter(&Default::default())).expect("adapter");
            let info = adapter.get_info();
            eprintln!("offscreen {w}x{h} on {:?}", info.name);
            #[cfg(not(target_arch = "wasm32"))]
            crate::report::set_gpu(format!("{} ({:?})", info.name, info.backend));
            let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).expect("device");
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: render::TARGET_FORMAT,
                width: w,
                height: h,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: wgpu::CompositeAlphaMode::Opaque,
                view_formats: Vec::new(),
                color_space: wgpu::SurfaceColorSpace::Auto,
            };
            self.attach(window, None, config, device, queue);
            return;
        }
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle_from_env(Box::new(self.display.clone())));
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .expect("adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).expect("device");
        let size = window.inner_size();
        let mut config = surface.get_default_config(&adapter, size.width.max(1), size.height.max(1)).expect("surface config");
        // The bytes go out as the GS's do: no sRGB encode (render::TARGET_FORMAT).
        config.format = config.format.remove_srgb_suffix();
        // a report's picture is read off the frame before it is presented
        // (`report_shot`), where the surface lets it be copied
        #[cfg(not(target_arch = "wasm32"))]
        {
            if surface.get_capabilities(&adapter).usages.contains(wgpu::TextureUsages::COPY_SRC) {
                config.usage |= wgpu::TextureUsages::COPY_SRC;
            }
            let info = adapter.get_info();
            crate::report::set_gpu(format!("{} ({:?})", info.name, info.backend));
        }
        // At the display's rate: the default is the driver's first present
        // mode, Immediate on RADV, which drew 2,300-2,600 frames a second in
        // STATION; the Frame Rate's caps still apply beneath it.
        config.present_mode = wgpu::PresentMode::AutoVsync;
        surface.configure(&device, &config);
        self.attach(window, Some(surface), config, device, queue);
    }

    /// The mouse's own motion: the look while it holds the pointer.
    fn device_event(&mut self, _: &ActiveEventLoop, _: winit::event::DeviceId, event: winit::event::DeviceEvent) {
        if let winit::event::DeviceEvent::MouseMotion { delta } = event {
            self.mouse.motion(delta.0, delta.1);
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if (!self.held.is_empty() || self.walker.is_some() || self.host.is_some())
            && let Some(g) = &self.gfx
        {
            // the options' Frame Rate natively: under a cap the loop sleeps
            // until the next frame is due instead of asking for frames it
            // would skip (a page skips animation frames: `web::frame_due`)
            #[cfg(not(target_arch = "wasm32"))]
            {
                use winit::event_loop::ControlFlow;
                if let Some(d) = crate::settings::frame_wait() {
                    _el.set_control_flow(ControlFlow::WaitUntil(Instant::now() + d));
                    return;
                }
                _el.set_control_flow(ControlFlow::Wait);
            }
            g.window.request_redraw();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        // the graphics device lost: nothing drawn or uploaded on it again,
        // the window closes and the game starts again (`gpulost`; its test
        // loses it here, between two events), logged out first as the
        // update's restart is (a login still held would refuse the next)
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(g) = &self.gfx {
                crate::gpulost::test(&g.device, self.clock.elapsed().as_millis() as i64);
            }
            if crate::gpulost::lost() {
                if let Some(p) = self.play.as_mut() {
                    p.log_out();
                }
                el.exit();
                return;
            }
        }
        match event {
            WindowEvent::CloseRequested => el.exit(),
            // Quit, answered yes in the start menu (`m_p2EC28`), or the
            // server ended the connection: back to the game's own screens
            // when it was booted (`Msg0777Done`), else the window closes.
            _ if self.play.as_ref().is_some_and(|p| p.quit || p.disconnected.is_some()) => match self.booted.take() {
                // the update's restart (`update`): the window closes for the
                // program in place
                Some(_) if crate::update::restarting() => el.exit(),
                Some(shared) => self.back_to_pregame(shared),
                None => el.exit(),
            },
            WindowEvent::Resized(s) if s.width > 0 && s.height > 0 => {
                let Some(g) = &mut self.gfx else { return };
                let Some(surface) = &g.surface else { return };
                g.config.width = s.width;
                g.config.height = s.height;
                surface.configure(&g.device, &g.config);
                g.depth = render::depth_view(&g.device, s.width, s.height);
                g.window.request_redraw();
            }
            // F11: full screen (borderless, on the window's monitor) and back.
            // A page leaves F11 to the browser (web/main.js).
            // macOS: ⌘ held or not, and ⌘, (the settings' key) when no menu
            // item took it, asking as Settings… does (`macos`)
            #[cfg(target_os = "macos")]
            WindowEvent::ModifiersChanged(m) => self.super_held = m.state().super_key(),
            #[cfg(target_os = "macos")]
            WindowEvent::KeyboardInput { event, .. } if self.super_held && event.state == ElementState::Pressed && matches!(&event.logical_key, Key::Character(c) if c == ",") => {
                crate::macos::ask();
            }
            WindowEvent::KeyboardInput { event: KeyEvent { logical_key: Key::Named(NamedKey::F11), state: ElementState::Pressed, repeat: false, .. }, .. } => {
                if let Some(g) = &self.gfx {
                    let full = g.window.fullscreen().is_some();
                    g.window.set_fullscreen((!full).then_some(Fullscreen::Borderless(None)));
                }
            }
            WindowEvent::Touch(t) => {
                use winit::event::TouchPhase as T;
                let phase = match t.phase {
                    T::Started => crate::touch::Phase::Started,
                    T::Moved => crate::touch::Phase::Moved,
                    T::Ended => crate::touch::Phase::Ended,
                    T::Cancelled => crate::touch::Phase::Cancelled,
                };
                let size = self.gfx.as_ref().map_or((1, 1), |g| (g.config.width, g.config.height));
                if self.walker.is_some() {
                    self.touch.touch(t.id, phase, [t.location.x as f32, t.location.y as f32], Instant::now(), size);
                    if let Some(p) = self.play.as_mut() {
                        p.keyboard = false;
                    }
                }
            }
            // The report's dialog (`report`): F3 opens it, over the game's
            // screens or the world, once there is a UI to draw it on; while
            // it is open it has the keyboard and the clicks.
            #[cfg(not(target_arch = "wasm32"))]
            WindowEvent::KeyboardInput { event, .. } if crate::report::is_open() || crate::report::is_report_key(&event.logical_key) => {
                if event.state == ElementState::Pressed {
                    if crate::report::is_open() {
                        if let Some(k) = crate::report::key_of(&event) {
                            crate::report::key(k);
                        }
                    } else if !event.repeat && (self.host.as_ref().is_some_and(|h| h.ui_parts().is_some()) || self.play.as_ref().is_some_and(|p| p.ui_parts().is_some())) {
                        crate::report::open();
                        // what was held is let go: the game would not see it go
                        self.held.clear();
                        self.keys_edge.clear();
                        self.grab_pointer(false);
                    }
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            WindowEvent::MouseInput { state, .. } if crate::report::is_open() => {
                if state == ElementState::Pressed
                    && let Some(g) = &self.gfx
                {
                    let size = (g.config.width, g.config.height);
                    crate::report::click(render::ui::frame_from_window(size, [self.cursor.0 as f32, self.cursor.1 as f32]));
                }
            }
            // The update's question (`update`): its own keys pressed while it
            // is up, and a click on its buttons; their releases and the rest
            // go on to the game.
            #[cfg(not(target_arch = "wasm32"))]
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed && crate::update::offer_shown() && pregame_key(&event).is_some_and(crate::update::Offer::takes) => {
                if !event.repeat
                    && let Some(k) = pregame_key(&event)
                {
                    let cues = crate::update::offer_key(k, self.clock.elapsed().as_millis() as i64);
                    self.ui_cues(cues.unwrap_or_default());
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } if crate::update::offer_shown() && self.offer_click() => {}
            #[cfg(not(target_arch = "wasm32"))]
            WindowEvent::MouseInput { state: ElementState::Released, button: MouseButton::Left, .. } if crate::update::offer_release() => {}
            #[cfg(not(target_arch = "wasm32"))]
            WindowEvent::MouseWheel { delta, .. } if crate::update::offer_shown() => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                crate::update::offer_wheel(lines, self.clock.elapsed().as_millis() as i64);
            }
            // Before the world: the pregame's keys, the game's codes
            // (`VIKeyboard::Read`'s), and its clicks.
            WindowEvent::KeyboardInput { event, .. } if self.host.is_some() => {
                self.touch.hide();
                // F9 (`era`), as the Controls page has it: the art swapped
                if event.state == ElementState::Pressed && !event.repeat && is_art_key(&event.logical_key) && self.host.as_ref().is_some_and(|h| h.pre.setup.capturing().is_none()) {
                    self.toggle_era();
                    return;
                }
                // the title's Options' Controls page waiting for a key: it is
                // the page's, as play's is
                let host = self.host.as_mut().unwrap();
                if host.pre.setup.capturing().is_some()
                    && event.state == ElementState::Pressed
                    && !event.repeat
                    && let Some(name) = crate::binds::name_of(&event.logical_key)
                {
                    host.pre.setup.capture_key(&name);
                    self.keys_edge.hold_back(&name);
                    return;
                }
                if let Some(k) = pregame_key(&event) {
                    let down = event.state == ElementState::Pressed;
                    // a held key repeats only where it types
                    let typing = matches!(k, 8 | 32..=126);
                    if !event.repeat || typing {
                        self.host.as_mut().unwrap().key(k, down);
                    }
                }
            }
            WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } if self.host.is_some() => {
                let Some(g) = &self.gfx else { return };
                let size = (g.config.width, g.config.height);
                let p = render::ui::frame_from_window(size, [self.cursor.0 as f32, self.cursor.1 as f32]);
                self.host.as_mut().unwrap().click(p);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // Escape while the mouse look holds the pointer: it is let go
                // (as a browser ends its lock on Escape), and nothing more
                if event.state == ElementState::Pressed && event.logical_key == Key::Named(NamedKey::Escape) && self.mouse.escape() {
                    self.grab_pointer(false);
                    return;
                }
                // a keyboard in use (`gKeyboardAttached`): the edit boxes'
                // compact face, with the options' Soft Keyboard on; the
                // touch screen's pad goes
                if event.state == ElementState::Pressed {
                    self.touch.hide();
                    if let Some(p) = self.play.as_mut() {
                        p.keyboard = true;
                    }
                }
                // The Controls page waiting for a key (`bindpage`): the key
                // pressed is its, as it is, and nothing else sees it or its
                // release.
                if let Some(p) = self.play.as_mut()
                    && p.capturing().is_some()
                    && event.state == ElementState::Pressed
                    && !event.repeat
                    && let Some(name) = crate::binds::name_of(&event.logical_key)
                {
                    p.capture_key(&name);
                    self.keys_edge.hold_back(&name);
                    return;
                }
                // The player's own keys (`binds`, the client's, at the edge):
                // a key a row was moved onto is that row's default key to all
                // below, the row's old key nothing; a key given to △'s row
                // (which the game has no key for) presses △ as the pad's
                // does; text typed into an edit box is never remapped; a key
                // lets go as what it was pressed as.
                let event = match crate::binds::name_of(&event.logical_key) {
                    Some(name) => {
                        use crate::binds::Sent;
                        let typing = self.play.as_mut().is_some_and(|p| p.keyboard_typing());
                        let down = event.state == ElementState::Pressed;
                        match crate::settings::with_binds(|b| self.keys_edge.key(&name, down, event.repeat, typing, b)) {
                            Some(Sent::Key(to)) if to == name => event,
                            Some(Sent::Key(to)) => {
                                let mut e = event.clone();
                                e.logical_key = crate::binds::key_of(&to);
                                e.text = None;
                                e
                            }
                            Some(Sent::Button(b)) => {
                                let aspect = self.gfx.as_ref().map_or(4.0 / 3.0, |g| g.config.width as f32 / g.config.height as f32);
                                if !event.repeat
                                    && let (Some(p), Some(w), Some(k)) = (self.play.as_mut(), self.walker.as_mut(), crate::binds::pad_key(b))
                                {
                                    p.set_key(k, down, w, aspect);
                                    p.keyboard = true;
                                }
                                return;
                            }
                            None => return,
                        }
                    }
                    None => event,
                };
                // a key let go is let go of, whichever window takes the
                // event: an arrow held to walk and let go over the start
                // menu would walk on after it
                if event.state == ElementState::Released
                    && let Some(n) = held_name(&event.logical_key)
                {
                    self.held.remove(&n);
                }
                // F9 (`era`): the art swapped, whatever window is up
                if event.state == ElementState::Pressed && !event.repeat && event.logical_key == Key::Named(NamedKey::F9) {
                    self.toggle_era();
                    return;
                }
                // The map (`map.rs`), while it is up, takes every key.
                if let Some(p) = self.play.as_mut().filter(|p| p.map.open) {
                    let name = match &event.logical_key {
                        Key::Named(NamedKey::Escape) => Some("escape".to_string()),
                        Key::Named(NamedKey::Backspace) => Some("backspace".to_string()),
                        Key::Named(NamedKey::Enter) => Some("enter".to_string()),
                        Key::Named(NamedKey::Space) => Some("space".to_string()),
                        k => held_name(k),
                    };
                    if let Some(n) = name {
                        p.map.keyboard_key(&n, event.state == ElementState::Pressed, event.repeat);
                    }
                    return;
                }
                // The edit box (mode 2) takes the keyboard: characters type,
                // Backspace, Escape (cancel), Enter (done), the arrows move the
                // caret (with the soft keyboard showing, its cursor; Insert
                // presses its key).
                if let Some(p) = self.play.as_mut()
                    && p.mode == 2
                {
                    let down = event.state == ElementState::Pressed;
                    let soft = !p.edit_pad_only();
                    let named = match &event.logical_key {
                        Key::Named(NamedKey::Backspace) => Some(8),
                        Key::Named(NamedKey::Escape) => Some(27),
                        Key::Named(NamedKey::Enter) => Some(13),
                        Key::Named(NamedKey::ArrowLeft) => Some(if soft { 136 } else { 17 }),
                        Key::Named(NamedKey::ArrowRight) => Some(if soft { 137 } else { 18 }),
                        Key::Named(NamedKey::ArrowUp) => Some(if soft { 138 } else { 19 }),
                        Key::Named(NamedKey::ArrowDown) => Some(if soft { 139 } else { 20 }),
                        Key::Named(NamedKey::Insert) => Some(134),
                        _ => None,
                    };
                    if let Some(k) = named {
                        p.edit_key(k, down);
                    } else if down && let Some(t) = &event.text {
                        for c in t.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()) {
                            p.edit_key(c as i32, true);
                        }
                    }
                    return;
                }
                // The quick-chat menus (mode 30): the arrows are the game's
                // cursor keys (17-20), its directions, held to edit (a key's
                // repeat would start the hold again, so it is dropped); Enter
                // (13) is the chat line over a menu and X in the editor, as
                // is E; Escape or Backspace (27) and Z (122) close a menu or
                // back out of the editor. The editor's edit box takes the
                // keyboard as the edit box does.
                if let Some(p) = self.play.as_mut()
                    && p.typing.is_none()
                    && p.mode == 30
                {
                    let down = event.state == ElementState::Pressed;
                    if p.quick_typing() {
                        let soft = !p.edit_pad_only();
                        let named = match &event.logical_key {
                            Key::Named(NamedKey::Backspace) => Some(8),
                            Key::Named(NamedKey::Escape) => Some(27),
                            Key::Named(NamedKey::Enter) => Some(13),
                            Key::Named(NamedKey::ArrowLeft) => Some(if soft { 136 } else { 17 }),
                            Key::Named(NamedKey::ArrowRight) => Some(if soft { 137 } else { 18 }),
                            Key::Named(NamedKey::ArrowUp) => Some(if soft { 138 } else { 19 }),
                            Key::Named(NamedKey::ArrowDown) => Some(if soft { 139 } else { 20 }),
                            Key::Named(NamedKey::Insert) => Some(134),
                            _ => None,
                        };
                        if let Some(k) = named {
                            p.quick_key(k, down);
                        } else if down && let Some(t) = &event.text {
                            for c in t.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()) {
                                p.quick_key(c as i32, true);
                            }
                        }
                        return;
                    }
                    let k = match &event.logical_key {
                        Key::Named(NamedKey::ArrowUp) => Some(19),
                        Key::Named(NamedKey::ArrowLeft) => Some(17),
                        Key::Named(NamedKey::ArrowRight) => Some(18),
                        Key::Named(NamedKey::ArrowDown) => Some(20),
                        Key::Named(NamedKey::Enter) => Some(13),
                        Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(134),
                        Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Backspace) => Some(27),
                        Key::Character(c) if c.eq_ignore_ascii_case("z") => Some(122),
                        _ => None,
                    };
                    if let Some(k) = k {
                        if !(down && event.repeat) {
                            p.quick_key(k, down);
                        }
                        return;
                    }
                }
                // A server's player search (`searchwnd`), while it is up: the
                // arrows, Enter or E (X: Search, a tell), Escape or Backspace
                // (it closes), Q (square: an invite), ` and Tab (L1 and R1:
                // the column sorted by). Every other key goes to it too, for
                // nothing, as S2 takes `VIWndGame`'s keys.
                if let Some(p) = self.play.as_mut()
                    && p.typing.is_none()
                    && p.search_open()
                {
                    use crate::startmenu::keys as K;
                    let down = event.state == ElementState::Pressed;
                    let k = match &event.logical_key {
                        Key::Named(NamedKey::ArrowUp) => Some(K::UP),
                        Key::Named(NamedKey::ArrowDown) => Some(K::DOWN),
                        Key::Named(NamedKey::ArrowLeft) => Some(K::LEFT),
                        Key::Named(NamedKey::ArrowRight) => Some(K::RIGHT),
                        Key::Named(NamedKey::Enter) => Some(K::CROSS),
                        Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(K::CROSS),
                        Key::Character(c) if c.eq_ignore_ascii_case("q") => Some(133),
                        Key::Character(c) if c.as_str() == "`" || c.as_str() == "~" => Some(140),
                        Key::Named(NamedKey::Tab) => Some(142),
                        Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Backspace) => Some(K::ESCAPE),
                        _ => None,
                    };
                    if let Some(k) = k
                        && !(down && event.repeat)
                    {
                        p.search_key(k, down);
                    }
                    return;
                }
                // The bank (mode 11): the arrows, Enter or E (X), Escape
                // (triangle); the auction form's editor takes the keyboard.
                if let Some(p) = self.play.as_mut()
                    && p.typing.is_none()
                    && p.bank_owns()
                {
                    let down = event.state == ElementState::Pressed;
                    if p.bank_typing() {
                        let named = match &event.logical_key {
                            Key::Named(NamedKey::Backspace) => Some(8),
                            Key::Named(NamedKey::Escape) => Some(27),
                            Key::Named(NamedKey::Enter) => Some(13),
                            Key::Named(NamedKey::ArrowLeft) => Some(17),
                            Key::Named(NamedKey::ArrowRight) => Some(18),
                            _ => None,
                        };
                        if let Some(k) = named {
                            p.bank_key(k, down);
                        } else if down && let Some(t) = &event.text {
                            for c in t.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()) {
                                p.bank_key(c as i32, true);
                            }
                        }
                        return;
                    }
                    use crate::startmenu::keys as K;
                    let k = match &event.logical_key {
                        Key::Named(NamedKey::ArrowUp) => Some(K::UP),
                        Key::Named(NamedKey::ArrowDown) => Some(K::DOWN),
                        Key::Named(NamedKey::ArrowLeft) => Some(K::LEFT),
                        Key::Named(NamedKey::ArrowRight) => Some(K::RIGHT),
                        Key::Named(NamedKey::Enter) => Some(K::CROSS),
                        Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(K::CROSS),
                        Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Backspace) => Some(K::ESCAPE),
                        _ => None,
                    };
                    if let Some(k) = k
                        && !(down && event.repeat)
                    {
                        p.bank_key(k, down);
                    }
                    return;
                }
                // The corpse, merchant and trade windows (modes 1, 10, 15,
                // 16): the arrows, Enter or E (X), Escape (triangle), Q
                // (accept a trade), Space (the trade's chat line, which then
                // takes the keyboard as the edit box does).
                if let Some(p) = self.play.as_mut()
                    && p.typing.is_none()
                    && p.npcw_owns()
                {
                    let down = event.state == ElementState::Pressed;
                    if p.npcw_typing() {
                        let named = match &event.logical_key {
                            Key::Named(NamedKey::Backspace) => Some(8),
                            Key::Named(NamedKey::Escape) => Some(27),
                            Key::Named(NamedKey::Enter) => Some(13),
                            Key::Named(NamedKey::ArrowLeft) => Some(17),
                            Key::Named(NamedKey::ArrowRight) => Some(18),
                            _ => None,
                        };
                        if let Some(k) = named {
                            p.npcw_key(k, down);
                        } else if down && let Some(t) = &event.text {
                            for c in t.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()) {
                                p.npcw_key(c as i32, true);
                            }
                        }
                        return;
                    }
                    use crate::startmenu::keys as K;
                    let k = match &event.logical_key {
                        Key::Named(NamedKey::ArrowUp) => Some(K::UP),
                        Key::Named(NamedKey::ArrowDown) => Some(K::DOWN),
                        Key::Named(NamedKey::ArrowLeft) => Some(K::LEFT),
                        Key::Named(NamedKey::ArrowRight) => Some(K::RIGHT),
                        Key::Named(NamedKey::Enter) => Some(K::CROSS),
                        Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(K::CROSS),
                        Key::Character(c) if c.eq_ignore_ascii_case("q") => Some(113),
                        Key::Named(NamedKey::Space) => Some(32),
                        Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Backspace) => Some(K::ESCAPE),
                        _ => None,
                    };
                    if let Some(k) = k
                        && !(down && event.repeat)
                    {
                        p.npcw_key(k, down);
                    }
                    return;
                }
                // The blacksmith and the customer-service windows (modes 19,
                // 31, 32): the arrows, Enter or E (X), Escape (triangle), Q
                // (square: the inbox again); while the text's editor is up
                // it takes the keyboard (Enter a newline, Tab done).
                if let Some(p) = self.play.as_mut()
                    && p.typing.is_none()
                    && p.support_owns()
                {
                    let down = event.state == ElementState::Pressed;
                    if p.support_typing() {
                        let named = match &event.logical_key {
                            Key::Named(NamedKey::Backspace) => Some(8),
                            Key::Named(NamedKey::Escape) => Some(27),
                            Key::Named(NamedKey::Enter) => Some(13),
                            Key::Named(NamedKey::Tab) => Some(143),
                            Key::Named(NamedKey::ArrowLeft) => Some(17),
                            Key::Named(NamedKey::ArrowRight) => Some(18),
                            Key::Named(NamedKey::ArrowUp) => Some(19),
                            Key::Named(NamedKey::ArrowDown) => Some(20),
                            _ => None,
                        };
                        if let Some(k) = named {
                            p.support_key(k, down);
                        } else if down && let Some(t) = &event.text {
                            for c in t.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()) {
                                p.support_key(c as i32, true);
                            }
                        }
                        return;
                    }
                    use crate::startmenu::keys as K;
                    let k = match &event.logical_key {
                        Key::Named(NamedKey::ArrowUp) => Some(K::UP),
                        Key::Named(NamedKey::ArrowDown) => Some(K::DOWN),
                        Key::Named(NamedKey::ArrowLeft) => Some(K::LEFT),
                        Key::Named(NamedKey::ArrowRight) => Some(K::RIGHT),
                        Key::Named(NamedKey::Enter) => Some(K::CROSS),
                        Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(K::CROSS),
                        Key::Character(c) if c.eq_ignore_ascii_case("q") => Some(133),
                        Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Backspace) => Some(K::ESCAPE),
                        _ => None,
                    };
                    if let Some(k) = k
                        && !(down && event.repeat)
                    {
                        p.support_key(k, down);
                    }
                    return;
                }
                // The mail and tell windows (modes 23, 24, 28, 29, and the buddy
                // list's compose): the arrows, Enter or E (X), Escape
                // (triangle), Q (square: the inbox again); while a field's
                // editor is up it takes the keyboard (Enter a newline in the
                // body, Tab done).
                if let Some(p) = self.play.as_mut()
                    && p.typing.is_none()
                    && p.mail_owns()
                {
                    let down = event.state == ElementState::Pressed;
                    if p.mail_typing() {
                        let named = match &event.logical_key {
                            Key::Named(NamedKey::Backspace) => Some(8),
                            Key::Named(NamedKey::Escape) => Some(27),
                            Key::Named(NamedKey::Enter) => Some(13),
                            Key::Named(NamedKey::Tab) => Some(143),
                            Key::Named(NamedKey::ArrowLeft) => Some(17),
                            Key::Named(NamedKey::ArrowRight) => Some(18),
                            Key::Named(NamedKey::ArrowUp) => Some(19),
                            Key::Named(NamedKey::ArrowDown) => Some(20),
                            _ => None,
                        };
                        if let Some(k) = named {
                            p.mail_key(k, down);
                        } else if down && let Some(t) = &event.text {
                            for c in t.chars().filter(|c| c.is_ascii() && !c.is_ascii_control()) {
                                p.mail_key(c as i32, true);
                            }
                        }
                        return;
                    }
                    use crate::startmenu::keys as K;
                    let k = match &event.logical_key {
                        Key::Named(NamedKey::ArrowUp) => Some(K::UP),
                        Key::Named(NamedKey::ArrowDown) => Some(K::DOWN),
                        Key::Named(NamedKey::ArrowLeft) => Some(K::LEFT),
                        Key::Named(NamedKey::ArrowRight) => Some(K::RIGHT),
                        Key::Named(NamedKey::Enter) => Some(K::CROSS),
                        Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(K::CROSS),
                        Key::Character(c) if c.eq_ignore_ascii_case("q") => Some(133),
                        Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Backspace) => Some(K::ESCAPE),
                        _ => None,
                    };
                    if let Some(k) = k
                        && !(down && event.repeat)
                    {
                        p.mail_key(k, down);
                    }
                    return;
                }
                // The start menu (mode 13) takes the arrows, Enter or E (X) and
                // Escape (back), down and up (its auto-repeat); Escape in play
                // opens it (`OnKeyDown` 27).
                if let Some(p) = self.play.as_mut()
                    && p.typing.is_none()
                    && p.start.is_some()
                {
                    use crate::startmenu::keys as K;
                    let down = event.state == ElementState::Pressed;
                    if p.mode == 13 {
                        let k = match &event.logical_key {
                            Key::Named(NamedKey::ArrowUp) => Some(K::UP),
                            Key::Named(NamedKey::ArrowDown) => Some(K::DOWN),
                            Key::Named(NamedKey::ArrowLeft) => Some(K::LEFT),
                            Key::Named(NamedKey::ArrowRight) => Some(K::RIGHT),
                            Key::Named(NamedKey::Enter) => Some(K::CROSS),
                            Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(K::CROSS),
                            Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Backspace) => Some(K::ESCAPE),
                            _ => None,
                        };
                        if let Some(k) = k
                            && !(down && event.repeat)
                        {
                            p.start_key(k, down);
                        }
                        return;
                    }
                    // the spell book (mode 5): its arrows include the tabs
                    if p.mode == 5 {
                        let k = match &event.logical_key {
                            Key::Named(NamedKey::ArrowUp) => Some(K::UP),
                            Key::Named(NamedKey::ArrowDown) => Some(K::DOWN),
                            Key::Named(NamedKey::ArrowLeft) => Some(136),
                            Key::Named(NamedKey::ArrowRight) => Some(137),
                            Key::Named(NamedKey::Enter) => Some(K::CROSS),
                            Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(K::CROSS),
                            Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Backspace) => Some(K::ESCAPE),
                            _ => None,
                        };
                        if let Some(k) = k
                            && !(down && event.repeat)
                        {
                            p.book_key(k, down);
                        }
                        return;
                    }
                    // the who list (mode 35): Q asks again, C (the windows'
                    // ○) sorts by the next column
                    if p.mode == 35 {
                        let k = match &event.logical_key {
                            Key::Named(NamedKey::ArrowUp) => Some(K::UP),
                            Key::Named(NamedKey::ArrowDown) => Some(K::DOWN),
                            Key::Named(NamedKey::Enter) => Some(K::CROSS),
                            Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(K::CROSS),
                            Key::Character(c) if c.eq_ignore_ascii_case("q") => Some(113),
                            Key::Character(c) if c.eq_ignore_ascii_case("c") => Some(135),
                            Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Backspace) => Some(K::ESCAPE),
                            _ => None,
                        };
                        if let Some(k) = k
                            && !(down && event.repeat)
                        {
                            p.who_key(k, down);
                        }
                        return;
                    }
                    // the character windows (the quest journal, the trainer,
                    // class mastery, factions): the arrows, Enter or E, Escape
                    if p.char_window() {
                        let k = match &event.logical_key {
                            Key::Named(NamedKey::ArrowUp) => Some(K::UP),
                            Key::Named(NamedKey::ArrowDown) => Some(K::DOWN),
                            Key::Named(NamedKey::ArrowLeft) => Some(136),
                            Key::Named(NamedKey::ArrowRight) => Some(137),
                            Key::Named(NamedKey::Enter) => Some(K::CROSS),
                            Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(K::CROSS),
                            Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Backspace) => Some(K::ESCAPE),
                            // square (Class Mastery's points) and R1 (its other list)
                            Key::Character(c) if c.eq_ignore_ascii_case("q") => Some(133),
                            // circle (the factions' next page)
                            Key::Character(c) if c.eq_ignore_ascii_case("c") => Some(135),
                            Key::Named(NamedKey::Tab) => Some(142),
                            _ => None,
                        };
                        if let Some(k) = k
                            && !(down && event.repeat)
                        {
                            p.char_key(k, down);
                        }
                        return;
                    }
                    // the options, buddy and ignore lists and the guild's
                    // roster (modes 34, 17/18, 26, 20/21): arrows, Enter or
                    // E, Escape; C (the windows' ○) the roster's sort
                    if p.panel_up() {
                        let k = match &event.logical_key {
                            Key::Named(NamedKey::ArrowUp) => Some(K::UP),
                            Key::Named(NamedKey::ArrowDown) => Some(K::DOWN),
                            Key::Named(NamedKey::ArrowLeft) => Some(K::LEFT),
                            Key::Named(NamedKey::ArrowRight) => Some(K::RIGHT),
                            Key::Named(NamedKey::Enter) => Some(K::CROSS),
                            Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(K::CROSS),
                            Key::Character(c) if c.eq_ignore_ascii_case("c") => Some(135),
                            Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Backspace) => Some(K::ESCAPE),
                            _ => None,
                        };
                        if let Some(k) = k
                            && !(down && event.repeat)
                        {
                            p.panel_key(k, down);
                        }
                        return;
                    }
                    if p.mode == 0 && down && event.logical_key == Key::Named(NamedKey::Escape) {
                        p.open_start_menu(true);
                        return;
                    }
                }
                // The game's keys (`VIWndGame::OnKeyDown`): Enter opens the
                // edit box R3 opens, `/` one begun with it, R one to `/reply`,
                // T one to `/tell` (`Play::chat_key`). Before the HUD is up
                // (no SUPPORT yet) a plain line stands in: keys type into it,
                // Enter sends it (`ChatLine`) and Escape drops it.
                if let Some(p) = self.play.as_mut()
                    && event.state == ElementState::Pressed
                {
                    let open = |p: &mut crate::play::Play, held: &mut HashSet<String>, s: &str| {
                        held.clear();
                        if p.hud_ready() {
                            p.chat_key(s, s.is_empty() || s == "/");
                        } else {
                            p.typing = Some(s.to_string());
                        }
                    };
                    // The manual camera: the chat line's keys end it first
                    // (`ResetView`, `OnKeyDown` 13, 47, 82, 84), then open it
                    if p.mode == 38 && p.typing.is_none() {
                        let chat = match &event.logical_key {
                            Key::Named(NamedKey::Enter) => true,
                            Key::Character(c) => c.as_str() == "/" || c.eq_ignore_ascii_case("t") || (c.eq_ignore_ascii_case("r") && !p.reply_name.is_empty()),
                            _ => false,
                        };
                        if chat && let Some(w) = self.walker.as_mut() {
                            p.reset_view(w);
                        }
                    }
                    // A dialog up takes the arrows, Enter and Escape.
                    if p.typing.is_none() && p.mode != 0 {
                        use crate::dialog::Key as D;
                        let k = match &event.logical_key {
                            Key::Named(NamedKey::ArrowUp) => Some(D::Up),
                            Key::Named(NamedKey::ArrowDown) => Some(D::Down),
                            Key::Named(NamedKey::Enter) => Some(D::Enter),
                            Key::Named(NamedKey::Escape) => Some(D::Escape),
                            Key::Named(NamedKey::ArrowLeft) => Some(D::Left),
                            Key::Named(NamedKey::ArrowRight) => Some(D::Right),
                            _ => None,
                        };
                        if let Some(k) = k
                            && p.dialog_key(k)
                        {
                            return;
                        }
                    }
                    match (&mut p.typing, &event.logical_key) {
                        (None, Key::Named(NamedKey::Enter)) => {
                            open(p, &mut self.held, "");
                            return;
                        }
                        // Q: InteractWithTarget.
                        (None, Key::Character(c)) if p.mode == 0 && c.eq_ignore_ascii_case("q") => {
                            p.interact_with_target();
                            return;
                        }
                        // ` and ~ (96, 126): L1's cycle, the player, the
                        // group, the pet (`CycleTarget(1)`), and held, the
                        // manual camera (`Play::target_key`); a key's repeat
                        // is nothing (the PS2's keyboard has none, so a key
                        // held is one press)
                        (None, Key::Character(c)) if p.mode == 0 && (c.as_str() == "`" || c.as_str() == "~") => {
                            if !event.repeat
                                && let (Some(w), Some(g)) = (self.walker.as_ref(), self.gfx.as_ref())
                            {
                                let (proj, v) = w.view(g.config.width as f32 / g.config.height as f32);
                                let dir = Vec3::from(w.eye.direction.to_array());
                                p.target_key(true, w.eye_position(), dir, proj * v);
                            }
                            return;
                        }
                        // Z (90, 122): the quick-chat menus, as L2.
                        (None, Key::Character(c)) if p.mode == 0 && c.eq_ignore_ascii_case("z") => {
                            p.open_quick_chat();
                            return;
                        }
                        // F1: the Options window's Controls page (the client's
                        // own; F1 makes no code in the game's keyboard table).
                        (None, Key::Named(NamedKey::F1)) if p.mode == 0 && !event.repeat => {
                            p.open_controls();
                            return;
                        }
                        // M: the map (the client's own; free in the game's
                        // keyboard table, `map.rs`; dead, it closes again).
                        (None, Key::Character(c)) if p.mode == 0 && c.eq_ignore_ascii_case("m") && !event.repeat => {
                            p.map.show();
                            return;
                        }
                        // X (88, 120): autorun (`ToggleP74`), as L3.
                        (None, Key::Character(c)) if p.mode == 0 && c.eq_ignore_ascii_case("x") => {
                            p.toggle_autorun();
                            return;
                        }
                        // 1 to 6: `SelectTarget`, the player, the group's
                        // others, the pet.
                        // (`OnKeyDown` takes them in the manual camera too)
                        (None, Key::Character(c)) if (p.mode == 0 || p.mode == 38) && matches!(c.as_str(), "1" | "2" | "3" | "4" | "5" | "6") => {
                            p.select_target(c.as_bytes()[0] as i32 - 48);
                            return;
                        }
                        // P: InitCameraStyle (first person and back), in
                        // play only (`OnKeyDown` 80 asks for mode 0).
                        (None, Key::Character(c)) if p.mode == 0 && c.eq_ignore_ascii_case("p") => {
                            if let Some(w) = self.walker.as_mut() {
                                p.camera_style_key(w);
                            }
                            return;
                        }
                        (None, Key::Character(c)) if c.as_str() == "/" => {
                            open(p, &mut self.held, "/");
                            return;
                        }
                        (None, Key::Character(c)) if c.eq_ignore_ascii_case("r") && !p.reply_name.is_empty() => {
                            open(p, &mut self.held, "/reply ");
                            return;
                        }
                        (None, Key::Character(c)) if c.eq_ignore_ascii_case("t") => {
                            open(p, &mut self.held, "/tell ");
                            return;
                        }
                        (Some(t), Key::Named(NamedKey::Enter)) => {
                            let line = std::mem::take(t);
                            p.typing = None;
                            if !line.is_empty() {
                                p.chat_line(&line);
                            }
                            return;
                        }
                        (Some(_), Key::Named(NamedKey::Escape)) => {
                            p.typing = None;
                            return;
                        }
                        (Some(t), Key::Named(NamedKey::Backspace)) => {
                            t.pop();
                            return;
                        }
                        (Some(t), _) => {
                            if let Some(s) = &event.text {
                                t.push_str(s);
                            }
                            return;
                        }
                        _ => {}
                    }
                }
                if self.play.as_ref().is_some_and(|p| p.typing.is_some()) {
                    return;
                }
                // C (67, 99) held: the chat box's history, as R2 holds it
                // (mode 33), its up and down the arrows (19, 20); let go,
                // play again (`OnKeyDown`/`OnKeyUp` 67, `VIWndChat`).
                if let Some(p) = self.play.as_mut()
                    && (p.mode == 0 || p.mode == 33)
                {
                    let down = event.state == ElementState::Pressed;
                    let k = match &event.logical_key {
                        Key::Character(c) if c.eq_ignore_ascii_case("c") => Some(99),
                        Key::Named(NamedKey::ArrowUp) if p.mode == 33 => Some(19),
                        Key::Named(NamedKey::ArrowDown) if p.mode == 33 => Some(20),
                        _ => None,
                    };
                    if let Some(k) = k
                        && !(down && event.repeat)
                    {
                        p.chat_history_key(k, down);
                        return;
                    }
                }
                // Q released: the target's info window closes (`OnKeyUp` 81).
                if let (Some(p), Key::Character(c)) = (self.play.as_mut(), &event.logical_key)
                    && c.eq_ignore_ascii_case("q")
                    && event.state == ElementState::Released
                {
                    p.info_up = false;
                    return;
                }
                // Space: attack, held (`ToggleAutoAttack` / `MaybeStopAutoAttack`).
                if let (Some(p), Key::Named(NamedKey::Space)) = (self.play.as_mut(), &event.logical_key) {
                    if event.state == ElementState::Pressed && !event.repeat && p.mode == 0 {
                        p.attack_press();
                    } else if event.state == ElementState::Released {
                        p.attack_release();
                    }
                    return;
                }
                // The HUD's quick menus in play on the arrows, the d-pad
                // (`VIWndHUDMenu`: up and down the selection, left and right
                // the menu), and E uses it (as circle). The game's keyboard
                // has them on W/S and A/D and walks on the arrows; here WASD
                // walks (`controls`), the user's choice.
                if let Some(p) = self.play.as_mut()
                    && (p.mode == 0 || p.mode == 38)
                {
                    use crate::hudmenu::Key as H;
                    let k = match &event.logical_key {
                        Key::Named(NamedKey::ArrowUp) => Some(H::Up),
                        Key::Named(NamedKey::ArrowDown) => Some(H::Down),
                        Key::Named(NamedKey::ArrowLeft) => Some(H::Left),
                        Key::Named(NamedKey::ArrowRight) => Some(H::Right),
                        Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(H::Use),
                        _ => None,
                    };
                    if let Some(k) = k {
                        if event.state == ElementState::Pressed {
                            p.hud_key(k);
                        }
                        return;
                    }
                }
                // F5-F8: the start menu's group page (not ported yet): invite
                // the target, remove it, disband, leave.
                if event.state == ElementState::Pressed
                    && let Some(p) = self.play.as_mut()
                    && p.mode == 0
                {
                    let action = match &event.logical_key {
                        Key::Named(NamedKey::F5) => Some(0x0701_0000),
                        Key::Named(NamedKey::F6) => Some(0x0704_0000),
                        Key::Named(NamedKey::F7) => Some(0x0705_0000),
                        Key::Named(NamedKey::F8) => Some(0x0706_0000),
                        _ => None,
                    };
                    if let Some(a) = action {
                        p.group_action(a);
                        return;
                    }
                }
                // Tab: the next target (`CycleTarget`), and held, the manual
                // camera (`Play::target_key`; its repeat is nothing, as `).
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::Tab)
                    && let (Some(p), Some(w), Some(g)) = (self.play.as_mut(), self.walker.as_ref(), self.gfx.as_ref())
                    && p.mode == 0
                {
                    if !event.repeat {
                        let (proj, v) = w.view(g.config.width as f32 / g.config.height as f32);
                        let dir = Vec3::from(w.eye.direction.to_array());
                        p.target_key(false, w.eye_position(), dir, proj * v);
                    }
                    return;
                }
                // Tab, ` or ~ let go (`OnKeyUp` 9, 96, 126): the manual camera
                // ended, or the hold given up
                if event.state == ElementState::Released
                    && (event.logical_key == Key::Named(NamedKey::Tab) || matches!(&event.logical_key, Key::Character(c) if c.as_str() == "`" || c.as_str() == "~"))
                    && let (Some(p), Some(w)) = (self.play.as_mut(), self.walker.as_mut())
                {
                    p.target_key_up(w);
                    return;
                }
                let name = held_name(&event.logical_key);
                if event.state == ElementState::Pressed {
                    match event.logical_key {
                        Key::Named(NamedKey::Escape) => el.exit(),
                        Key::Named(NamedKey::ArrowRight) if self.walker.is_none() => self.step(1),
                        Key::Named(NamedKey::ArrowLeft) if self.walker.is_none() => self.step(-1),
                        _ => {}
                    }
                    if let Some(n) = name {
                        if self.held.is_empty() {
                            self.last = Instant::now();
                        }
                        self.held.insert(n);
                    }
                } else if let Some(n) = name {
                    self.held.remove(&n);
                }
                if let Some(g) = &self.gfx {
                    g.window.request_redraw();
                }
            }
            // Walking, the mouse is `mouse.rs`'s: a drag looks round, the
            // wheel zooms, a click targets and a double or right click talks.
            // a release that may never come: let go of what is held (a key
            // let go while the report's dialog or another window has the
            // keys would walk or turn the player on for good)
            // which window is in front: Controller Focus's Window reads the
            // controllers only there
            WindowEvent::Focused(true) => self.pads.focused = true,
            WindowEvent::Focused(false) => {
                self.pads.focused = false;
                self.mouse.release_all();
                self.held.clear();
                self.keys_edge.clear();
                self.grab_pointer(false);
                if let Some(p) = self.play.as_mut() {
                    p.map.let_go();
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.mouse.release_all();
                self.grab_pointer(false);
                if let Some(p) = self.play.as_mut() {
                    p.map.let_go();
                }
            }
            WindowEvent::MouseInput { state, button, .. } if matches!(button, MouseButton::Left | MouseButton::Right) => {
                let down = state == ElementState::Pressed;
                // the map up: a drag pans it, a click is its (`map.rs`)
                let at = [self.cursor.0 as f32, self.cursor.1 as f32];
                if let Some(c) = self.play.as_mut().filter(|p| p.map.open).map(|p| p.map.mouse_button(button == MouseButton::Right, down, at)) {
                    if let Some(c) = c {
                        self.click(c, false);
                    }
                    return;
                }
                if self.walker.is_some() {
                    // the left button on a quick-chat choice holds its
                    // direction while it is down (`Play::point_press`), and
                    // what it would click is not a click
                    let mut held = false;
                    if button == MouseButton::Left
                        && let (Some(p), Some(g)) = (self.play.as_mut(), self.gfx.as_ref())
                    {
                        held = if down { p.point_press(self.cursor, (g.config.width, g.config.height)) } else { p.point_release() };
                    }
                    if let Some(c) = self.mouse.button(button == MouseButton::Right, down, Instant::now())
                        && !held
                    {
                        self.click(c, false);
                    }
                    self.grab_pointer(false);
                } else if button == MouseButton::Left {
                    self.drag = down.then_some(self.cursor);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                // the update's question's list dragged (`update`)
                #[cfg(not(target_arch = "wasm32"))]
                if self.offer_drag() {
                    return;
                }
                if let Some(p) = self.play.as_mut().filter(|p| p.map.open) {
                    p.map.mouse_moved([position.x as f32, position.y as f32]);
                }
                self.mouse.moved(position.x, position.y);
                // a drag begun: the pointer taken, within the press's
                // gesture; a confined cursor put back to the middle
                if self.walker.is_some() {
                    self.grab_pointer(false);
                    if let Some(g) = &self.gfx
                        && let Some(at) = self.mouse.recentre((g.config.width, g.config.height))
                    {
                        crate::mouse::put_pointer(&g.window, at);
                    }
                }
                if self.walker.is_none()
                    && let Some((x, y)) = self.drag
                {
                    self.camera.drag((position.x - x) as f32, (position.y - y) as f32);
                    self.drag = Some(self.cursor);
                    if let Some(g) = &self.gfx {
                        g.window.request_redraw();
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // a window takes a notch (a line; a page's hundred pixels)
                // as one step
                let (d, steps) = match delta {
                    MouseScrollDelta::LineDelta(_, y) => (y, y.clamp(-1.0, 1.0)),
                    MouseScrollDelta::PixelDelta(p) => (p.y as f32 / 40.0, p.y as f32 / 100.0),
                };
                if let Some(p) = self.play.as_mut().filter(|p| p.map.open) {
                    p.map.wheel(steps, Some([self.cursor.0 as f32, self.cursor.1 as f32]));
                } else if self.walker.is_some() {
                    self.mouse.wheel(d, steps);
                } else if let Camera::Orbit { zoom, .. } = &mut self.camera {
                    *zoom = (*zoom * 0.9f32.powf(d)).clamp(0.05, 20.0);
                }
                if let Some(g) = &self.gfx {
                    g.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                // Sound in Background (the Extended page): off, this window
                // is heard only while it is the one in front (has the focus:
                // a minimised window has not; one merely covered by another
                // while it has it is still the player's)
                crate::audio::set_heard(crate::settings::background_sound() || self.pads.in_front());
                // macOS: Settings… (⌘,): the Options where the player is, in
                // the world the Options window, at the title its Options
                #[cfg(target_os = "macos")]
                if crate::macos::take() {
                    match self.play.as_mut() {
                        Some(p) => p.open_options(),
                        None => {
                            if let Some(h) = self.host.as_mut() {
                                h.pre.ask_options();
                            }
                        }
                    }
                }
                // A page on WebGL2 draws at most 30 frames a second
                // (`web::frame_due`): an animation frame before the next is
                // due is skipped whole, the game included, so its clocks see
                // longer frames rather than fewer ticks.
                #[cfg(target_arch = "wasm32")]
                if !crate::web::frame_due() {
                    // the sound is still fed (`Sound::feed`)
                    if let Some(s) = self.sound.as_mut() {
                        s.feed();
                    }
                    if let Some(g) = &self.gfx {
                        g.window.request_redraw();
                    }
                    return;
                }
                // natively the same cap, the options' Frame Rate: a frame
                // asked for early (a resize, the wheel) waits for its time
                // (`about_to_wait` sleeps until it is due)
                #[cfg(not(target_arch = "wasm32"))]
                if !crate::settings::frame_due() {
                    return;
                }
                FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                // the 2002 disc's files, read since (`era`)
                self.poll_era();
                let frame_start = Instant::now();
                // one of the game's frames, 30 a second whatever the
                // display's rate: what the game counts in frames (the HUD and
                // its windows, the rings, the death camera, the shake) steps
                // only on these (`frames.rs`)
                let game_frame = self.frames.due(self.clock.elapsed().as_secs_f64() * 1000.0);
                if let Some(at) = self.drawn_at.replace(frame_start) {
                    self.frame_s = frame_start.duration_since(at).as_secs_f32().min(0.1);
                }
                // before the world: the pregame's frame
                if self.host.is_some() {
                    if self.quit_at.is_some_and(|q| self.clock.elapsed().as_secs_f64() >= q) {
                        el.exit();
                        return;
                    }
                    self.host_frame(el, game_frame);
                    return;
                }
                if let Some(p) = &mut self.perf {
                    p.begin();
                }
                if self.quit_at.is_some_and(|q| self.clock.elapsed().as_secs_f64() >= q) {
                    el.exit();
                    return;
                }
                // A teleport into another world: its reads begun (a page's take
                // frames, the old world played meanwhile under the black
                // screen), and the walk begun again there once they are in.
                if let Some((file, at)) = self.play.as_mut().and_then(|p| p.take_world_change()) {
                    let reads = stream::open_world(&file, at.pos[0], at.pos[2]);
                    self.entering = Some((file, at, reads));
                }
                if let Some(r) = self.entering.as_ref().and_then(|e| e.2.borrow_mut().take()) {
                    let (file, at, _) = self.entering.take().unwrap();
                    match r.map_err(Error::from).and_then(|(c, zones)| self.enter_world(&file, c, zones, at)) {
                        Ok(()) => crate::say(&format!("entered {file} at {:?}", at.pos)),
                        Err(e) => {
                            crate::say(&format!("world change to {file}: {e}"));
                            #[cfg(target_arch = "wasm32")]
                            crate::web::problem("world", &format!("The world change to {file} failed: {e}"));
                        }
                    }
                    if let (Some(p), Some(w)) = (self.play.as_mut(), self.walker.as_mut()) {
                        p.world_entered(w);
                    }
                }
                self.fly();
                // Zones whose bytes have come: into the library and the collision world.
                if let (Some(s), Some(w)) = (self.streamer.as_mut(), self.walker.as_mut())
                    && let Some(first) = s.install(&mut self.lib, &mut w.scene)
                    && let Some(p) = self.particles.as_mut()
                {
                    p.add_from(&w.scene, first);
                }
                self.poll_pad();
                for c in self.touch.take_clicks() {
                    self.click(c, true);
                }
                self.touch.window = self.play.as_ref().is_some_and(|p| p.window_up() || p.map.open);
                self.touch.dpad = self.play.as_mut().is_some_and(|p| p.quick_dpad());
                mark(&mut self.perf, 0);
                self.grab_pointer(true);
                let sticks = self.sticks();
                self.sticks_used = sticks;
                // the mouse steering the body (`controls::Client::steer`)
                let look_free = !self.play.as_ref().is_some_and(|p| p.is_dead()) && self.play.as_ref().is_none_or(|p| p.look_free());
                let steer = look_free && self.walker.as_ref().is_some_and(|w| self.mouse.steer(w.cam.camera_mode));
                let game_keys = self.game_keys();
                let now = self.clock.elapsed().as_millis() as i64;
                let Some(g) = &mut self.gfx else { return };
                let body_at = self.body_at;
                let clock = self.clock;
                let streamer = &mut self.streamer;
                let sound = &mut self.sound;
                let walker_sounds = &mut self.walker_sounds;
                let particles = &mut self.particles;
                let held_fx = &mut self.held_fx;
                let batches = &mut self.batches;
                let fx = &mut self.fx;
                let weather = &mut self.weather;
                let sky_batches = &mut self.sky_batches;
                let mut bb_view = None;
                let play = &mut self.play;
                let lib = &self.lib;
                let perf = &mut self.perf;
                let console = &mut self.console;
                let pads = &mut self.pads;
                let walk_view = self.walker.as_mut().map(|w| {
                    // UpdateControls, on the sticks and the keyboard's axes,
                    // as the client stands
                    let mut client = play.as_ref().filter(|p| !p.is_dead()).map(|p| p.control_client()).unwrap_or_default();
                    client.keys = game_keys;
                    client.steer = steer;
                    let changed = w.frame_with(sticks, now, &client, game_frame);
                    if let Some(p) = play.as_mut() {
                        p.controls_changed(changed);
                    }
                    mark(perf, 1);
                    if let Some(p) = play {
                        let gpu = crate::play::Gpu { renderer: &mut g.renderer, device: &g.device, queue: &g.queue };
                        // The server named the player's model: dress the walker in it.
                        // Built in its model's art (`era`): the 2002 disc's
                        // whole while it is drawn and has that model.
                        if let Some(model) = p.frame(w, lib, gpu, now, game_frame, sound.as_mut()) {
                            lib.with_view(lib.view_for(model as i32), || {
                                if let Some(r) = lib.lookup(model as i32)
                                    && let Some(m) = lib.build_model(r)
                                {
                                    w.set_character(lib, r);
                                    let textures = render::library_textures(lib);
                                    let (mat, _) = w.pose();
                                    if body_at < g.models.len() {
                                        g.models[body_at] = g.renderer.upload(&g.device, &g.queue, &m, &[mat], &textures);
                                    }
                                }
                            });
                        }
                        for (x, instant) in p.weather.drain(..) {
                            if let Some(wx) = weather.as_mut() {
                                wx.set_weather(x, instant);
                            }
                        }
                        // the HUD, once SUPPORT's atlas has come
                        p.attach_ui(&mut g.renderer, &g.device, &g.queue, g.config.format, lib);
                        // the spell effects' vibrate events (`Rumble`)
                        let vibration = p.options.vibration != 0;
                        for (motor, level, ms) in p.rumbles.drain(..) {
                            console.rumble(motor, level, ms, vibration);
                        }
                    }
                    // VIConsole::Process: the motors as they are now
                    if let Some((act, ms)) = console.process(now) {
                        pads.rumble(act, ms);
                    }
                    mark(perf, 2);
                    if let Some(s) = sound.as_mut() {
                        let ids = w.sprite.as_ref().map(|cs| cs.sounds()).unwrap_or_default();
                        if ids != *walker_sounds {
                            s.share_sounds(lib, &ids);
                            s.release_sounds(walker_sounds);
                            *walker_sounds = ids;
                        }
                    }
                    let (m, palette) = match sound.as_mut() {
                        Some(s) => w.pose_sounding(Some(&mut s.sink())),
                        None => w.pose(),
                    };
                    if let Some(body) = g.models.get_mut(body_at) {
                        g.renderer.set_instances(&g.queue, body, &[m]);
                        g.renderer.set_palette(&g.queue, body, &palette);
                        crate::play::place_body(body, w, m);
                    }
                    // Dressed as the server says: the plain body gives way.
                    if let Some(p) = play.as_mut().filter(|p| p.has_self()) {
                        p.place_self(w, m, &palette, &g.renderer, &g.queue);
                        if let Some(body) = g.models.get_mut(body_at) {
                            body.visible = false;
                        }
                    }
                    mark(perf, 3);
                    let loading = play.as_ref().is_some_and(|p| p.loading());
                    if let Some(s) = streamer.as_mut() {
                        // One zone a frame, as the game's loader takes them;
                        // under the loading screen all at once.
                        let at = Vec3::from(w.cam.walk.phys.position.to_array());
                        let gpu = stream::Gpu { renderer: &mut g.renderer, device: &g.device, queue: &g.queue };
                        s.update(at, &mut w.scene, lib, gpu, if loading { None } else { Some(1) });
                        s.animate(&mut w.scene, &g.renderer, &g.queue);
                    }
                    // the loading screen down once the zones round the arrival are in and drawn
                    if loading && let Some(p) = play.as_mut() {
                        let settled = p.loading_at().is_some_and(|a| streamer.as_ref().is_none_or(|s| s.settled(a, &w.scene, lib)));
                        p.loading_step(w, settled);
                    }
                    mark(perf, 4);
                    let cues: Vec<crate::sound::Cue> = play.as_mut().map(|p| std::mem::take(&mut p.cues)).unwrap_or_default();
                    if let Some(s) = sound {
                        for c in &cues {
                            s.cue(c);
                        }
                        if let Some(p) = play.as_ref() {
                            s.battle_music(p.self_fighting());
                        }
                        // the game's sound on the game's frames, the output
                        // fed between (`pregame::host`'s `sound_frame`)
                        if game_frame {
                            s.frame(&mut w.scene, &w.eye, game_hour(play.as_ref(), clock), now);
                        } else {
                            s.feed();
                        }
                    }
                    g.set_title(title(play.as_ref().map(|p| p.player.name.as_str())));
                    mark(perf, 5);
                    if let Some(wx) = weather.as_mut() {
                        wx.frame(&mut w.scene, sound.as_mut(), &w.eye, now, game_hour(play.as_ref(), clock));
                    }
                    mark(perf, 6);
                    let (proj, v) = w.view(g.config.width as f32 / g.config.height as f32);
                    if let Some(ps) = particles.as_mut() {
                        *batches = ps.frame(&w.scene, lib, w.eye_position(), proj * v, now);
                        bb_view = Some((v, proj));
                    }
                    // the spell effects' particles after the world's, their lightning before
                    if let Some(sp) = play.as_mut().and_then(|p| p.spells.as_mut()) {
                        batches.append(&mut sp.batches);
                        g.renderer.strips = std::mem::take(&mut sp.strips);
                    }
                    // the particles of the items in hands (an epic weapon's)
                    if let Some(p) = play.as_ref() {
                        let held = held_fx.get_or_insert_with(|| crate::particles::Held::new(now));
                        batches.extend(held.frame(lib, &p.in_hand(lib, w, m), &w.eye.position, now));
                    }
                    // the weapon trails (VITrailFxRender) after them
                    if let Some(p) = play.as_mut() {
                        g.renderer.strips.append(&mut p.trail_strips);
                    }
                    mark(perf, 7);
                    let hour_now = game_hour(play.as_ref(), clock);
                    let mut lights = lit(proj, v, hour_now, Some((w.eye_position(), &w.scene)), weather.as_mut());
                    if let Some(p) = play.as_mut() {
                        lights.0.point = p.point_light(w, now, hour_now, weather.as_ref());
                    }
                    if let Some(wx) = weather.as_mut() {
                        let (sky, world) = wx.render(&w.eye);
                        *sky_batches = sky;
                        batches.extend(world);
                        bb_view = Some((v, proj));
                    }
                    mark(perf, 8);
                    // the effect volumes (`VIScene::Render`'s deferred actors)
                    if let Some(fx) = fx.as_mut() {
                        fx.stream(Vec3::from(w.cam.walk.phys.position.to_array()), &w.scene, lib);
                        let cam = crate::fx::camera(w.eye.position, w.eye.direction);
                        fx.frame(&w.scene, lib, &cam, proj * v, lights.0.eye, now, hour_now);
                        fx.upload(&mut g.renderer, &g.device, &g.queue, lib);
                    }
                    mark(perf, 14);
                    lights
                });
                let frame = match &g.surface {
                    Some(surface) => match surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => Some(f),
                        _ => {
                            surface.configure(&g.device, &g.config);
                            return;
                        }
                    },
                    None => None,
                };
                mark(&mut self.perf, 9);
                let view = match walk_view {
                    Some((v, sky)) => {
                        if let (Some(f), Some(at)) = (sky, self.sky_at) {
                            let textures = render::library_textures(&self.lib);
                            crate::sky::update(&mut g.renderer, &g.device, &g.queue, &mut g.models, at, &f, &textures);
                        }
                        v
                    }
                    None => self.camera.view(self.bounds, g.config.width as f32 / g.config.height as f32),
                };
                self.last_view = Some(view.view_proj);
                if let (Some(p), Some(w)) = (&mut self.play, &self.walker) {
                    if let Some(u) = g.renderer.ui.as_mut() {
                        p.hud(u, w, self.clock.elapsed().as_millis() as i64, view.view_proj, (g.config.width, g.config.height), game_frame);
                        self.touch.textures(u, &g.device, &g.queue);
                        p.draw_touch(u, &self.touch, (g.config.width, g.config.height));
                        // the update's question (its Yes logs out: the window
                        // closes) and the report's dialog over the HUD and the
                        // touch pad (`update`, `report`)
                        #[cfg(not(target_arch = "wasm32"))]
                        report_test(self.clock.elapsed().as_millis() as i64);
                        #[cfg(not(target_arch = "wasm32"))]
                        {
                            let now = self.clock.elapsed().as_millis() as i64;
                            let plain = p.plain() && !crate::report::is_open();
                            let still = self.console.sticks().iter().all(|v| v.abs() < 0.05);
                            let quiet = plain && p.quiet() && self.held.is_empty() && still;
                            crate::update::offer_frame(crate::update::Moment { quiet, plain, world: true, now });
                            if crate::update::restarting() {
                                p.log_out();
                            }
                        }
                        #[cfg(not(target_arch = "wasm32"))]
                        if let Some((vui, assets)) = p.ui_parts() {
                            let mut r = assets.raster(u);
                            r.ui.anchor = render::ui::Anchor::Middle;
                            crate::update::offer_draw(vui, &mut r, self.clock.elapsed().as_millis() as i64);
                            crate::report::draw(vui, &mut r);
                        }
                    }
                    p.draw_rings(&mut g.renderer, &g.device, &g.queue, w, view.view_proj, &self.lib, game_frame);
                    // the map (`map.rs`): its build behind the game, and its view while up
                    p.map_frame(w, &self.lib, &g.device, &g.queue, g.config.format, (g.config.width, g.config.height), self.frame_s, self.console.sticks());
                    let plates = p.plates();
                    if let Some(t) = g.renderer.text.as_mut() {
                        t.plates = crate::play::labels(&plates, w.eye_position(), view.view_proj, t);
                        t.labels = crate::play::chat_labels(p, t, (g.config.width, g.config.height));
                    }
                }
                mark(&mut self.perf, 10);
                let target = match (&frame, &g.offscreen) {
                    (Some(f), _) => f.texture.create_view(&Default::default()),
                    (None, Some(t)) => t.create_view(&Default::default()),
                    (None, None) => return,
                };
                let mut enc = g.device.create_command_encoder(&Default::default());
                // the loading screen in place of the scene: its gears through its own view
                let loading = self.play.as_ref().and_then(|p| p.loading_scene((g.config.width, g.config.height)));
                let refs: Vec<render::Draw> = match &loading {
                    Some((_, gears)) => gears.iter().map(|&m| render::Draw::Model(m)).collect(),
                    None => g
                        .models
                        .iter()
                        .map(render::Draw::Model)
                        .chain(self.streamer.iter().flat_map(|s| s.draws()))
                        .chain(self.play.iter().flat_map(|p| p.models()).map(render::Draw::Model))
                        .chain(self.fx.iter().flat_map(|f| f.models()).map(render::Draw::Model))
                        .collect(),
                };
                // the options' Brightness: the scene through its curve (none
                // at the default), the labels and the HUD after it as drawn
                g.renderer.curve.set(crate::settings::curve(crate::settings::client().brightness));
                let scene = g.renderer.curve.scene_target(&g.device, (g.config.width, g.config.height));
                let st = scene.as_ref().unwrap_or(&target);
                g.renderer.draw(&g.queue, &mut enc, st, &g.depth, loading.as_ref().map_or(&view, |l| &l.0), &refs);
                let textures = render::library_textures(&self.lib);
                // under the loading screen no sky, volumes or particles
                let bb_view = bb_view.filter(|_| loading.is_none());
                if let Some((v, p)) = bb_view {
                    let mut rot = v;
                    rot.w_axis = glam::Vec4::W;
                    g.renderer.draw_sky_billboards(&g.device, &g.queue, &mut enc, st, &g.depth, rot, p, &self.sky_batches, &textures);
                }
                // the effect volumes' fans, then the particles over them,
                // all in the scene (the Brightness curve takes them too)
                if loading.is_none() {
                    g.renderer.draw_fans(&g.queue, &mut enc, st, &g.depth, &view);
                    // the name plates where VIScene::Render draws them: the
                    // particles and the rest after, over them
                    g.renderer.draw_plates(&g.device, &mut enc, st, &g.depth, (g.config.width, g.config.height));
                }
                if let Some((v, p)) = bb_view {
                    g.renderer.draw_billboards(&g.device, &g.queue, &mut enc, st, &g.depth, v, p, &self.batches, &textures);
                }
                g.renderer.curve.apply(&g.queue, &mut enc, &target);
                g.renderer.draw_text(&g.device, &mut enc, &target, &g.depth, (g.config.width, g.config.height));
                if let Some(p) = &self.play {
                    p.map.draw(&g.device, &mut enc, &target, &g.depth, (g.config.width, g.config.height));
                }
                mark(&mut self.perf, 11);
                let sent = g.queue.submit([enc.finish()]);
                #[cfg(not(target_arch = "wasm32"))]
                report_shot(g, frame.as_ref());
                if !self.shots.is_empty() {
                    let what = self.play.as_ref().map_or_else(|| "viewer".to_string(), |p| format!("play as {}", p.player.name));
                    shoot(&mut self.shots, g, self.clock.elapsed().as_millis() as i64, &what);
                }
                mark(&mut self.perf, 12);
                match frame {
                    Some(f) => g.queue.present(f),
                    None => {
                        if let Some(prev) = g.in_flight.replace(sent) {
                            let _ = g.device.poll(wgpu::PollType::Wait { submission_index: Some(prev), timeout: None });
                        }
                    }
                }
                mark(&mut self.perf, 13);
                if let Some(p) = &mut self.perf {
                    p.drawn = g.renderer.drawn.get();
                    p.end(frame_start);
                }
                // The page's picture of this frame, if it asked, and the
                // report's state of the client (`report`).
                crate::report::frame_drawn();
                crate::report::note_state(|| self.report_state());
            }
            _ => {}
        }
    }
}
