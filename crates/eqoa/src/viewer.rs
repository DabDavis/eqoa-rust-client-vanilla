//! The viewer, on wgpu + winit: one model at a time (orbit camera) or a zone
//! (fly camera). `shot` renders the same view offscreen to a PNG.
//!
//! Models: drag to orbit, scroll to zoom, ←/→ for the previous/next model.
//! Zones: drag to look, WASD to move, Q/E down/up, Shift ×5.
//! Walking: WASD is the left stick, ←/→ orbit and ↑/↓ zoom (the right stick),
//! drag orbits too. Esc quits.

use crate::scene::{self, Scene};
use crate::stream::{self, Streamer};
use crate::walker::{Controls, Walker};
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

/// A frame's lights, fog and sky out of the time of day, for the camera at
/// `eye` in `world` (its terrain profiles and env flags), if any, under the
/// atmosphere's `weather`: its overcast (scaled by the profiles' for the
/// lights), its lightning flash, and the sky colour and Lavastorm handed back
/// to it.
pub fn lit(
    proj: glam::Mat4,
    view: glam::Mat4,
    start: Instant,
    eye: Option<(Vec3, &vi::world::World<vi::fpu::Ee>)>,
    mut weather: Option<&mut crate::weather::Weather>,
) -> (View, Option<crate::sky::SkyFrame>) {
    let view_proj = proj * view;
    let mut rot = view;
    rot.w_axis = glam::Vec4::W;
    use vi::fpu::{Ee, F};
    let tod = vi::tod::TimeOfDay::<Ee>::new();
    let hour = F::new(hour(start));
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
        let fog = tod.fog(hour, o, b, w.env_flags(&at), &sun);
        v.fog = Some((fog.near.0, fog.far.0, Vec3::new(fog.color[0].0, fog.color[1].0, fog.color[2].0)));
        let c = tod.sky(hour, o, b);
        v.clear = Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32) / 255.0;
        v.sky_view_proj = Some(proj * rot);
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
    /// A zone with the character in it, walking.
    Walk { file: usize, index: usize, character: Ref, spawn: Option<(f32, f32, f32)> },
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
        // `EQOA_SPAWN=x,z[,heading]` picks the spot; else near the middle.
        let c = (s.bounds.0 + s.bounds.1) * 0.5;
        let spawn: Vec<f32> = std::env::var("EQOA_SPAWN")
            .ok()
            .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect())
            .unwrap_or_default();
        let (x, z, h) = match (spawn.as_slice(), given) {
            ([x, z], _) => (*x, *z, 0.0),
            ([x, z, h, ..], _) => (*x, *z, *h),
            (_, Some(at)) => *at,
            _ => (c.x - 40.0, c.z + 40.0, 0.0),
        };
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
        let mut w = Walker::new(world, x, z, h);
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
        lit(proj, glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y), Instant::now(), None, None).0
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
fn change_world(lib: &mut Library, file: &str, pos: [f32; 3], heading: f32) -> Result<(Walker, Streamer), Error> {
    let c = stream::open_file(std::path::Path::new(&crate::disc_path(None)), file)?;
    lib.replace(0, file, c);
    let index = vi::world::World::<vi::fpu::Ee>::zone_at(lib.file(0), pos[0], pos[2]).ok_or("no zone at the arrival")?;
    stream::load_zone_now(lib, 0, index)?;
    let character = lib.lookup(HUMAN).ok_or("no human model")?;
    let src = Source::Walk { file: 0, index, character, spawn: Some((pos[0], pos[2], heading)) };
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
        // `EQOA_SPELL=n`: SPELLFX's n-th effect, cast by the character at a
        // stand-in 6 ahead of it at EQOA_AT (offline, no server).
        let mut trial = std::env::var("EQOA_SPELL").ok().and_then(|v| v.parse::<usize>().ok()).filter(|_| play.is_none()).map(|n| {
            for f in ["SPELLFX.CSF", "FX.CSF"] {
                if lib.files.iter().all(|(name, _)| name != f)
                    && let Ok(b) = crate::disc(None).and_then(|mut d| Ok(d.read(f)?))
                    && let Ok(c) = esf::Container::open(b)
                {
                    lib.add(f, c);
                }
            }
            (crate::spells::Spells::load(lib, 0), n, vi::csprite::Rand(1))
        });
        let mut weather = crate::weather::Weather::new(lib, 0);
        // `EQOA_AT=ms`: when the target (and then Q, attack) steps happen.
        let at: i64 = std::env::var("EQOA_AT").ok().and_then(|v| v.parse().ok()).map_or(2000, |v: i64| v / 16 * 16);
        let attack = std::env::var_os("EQOA_ATTACK").is_some();
        let trace_walk: Option<i64> = std::env::var("EQOA_TRACE_WALK").ok().and_then(|v| v.parse::<i64>().ok()).map(|v| (v / 16 * 16).max(16));
        let walk = walk_script();
        // `EQOA_PAD`: a pad by script, through the game's pad path;
        // `EQOA_MOUSE`: the mouse's, through the window's
        let pad_script = crate::input::Script::from_env();
        let mut console = crate::input::Console::new();
        let mouse_script = crate::mouse::script();
        let mut mouse = crate::mouse::Mouse::default();
        let mut last_view: Option<glam::Mat4> = None;
        // `EQOA_TOUCH=1` and `EQOA_TOUCHES`: the touch screen's pad
        let touch_script = crate::touch::script();
        let mut touch = crate::touch::TouchPad::new(touch_first());
        while t < script.ms {
            t += 16;
            let controls = match &walk {
                Some(segs) => segs.iter().find(|s| t <= s.1).map_or(Controls::default(), |s| s.0),
                None => script.controls,
            };
            let mut sticks = controls.axes();
            touch.window = play.as_ref().is_some_and(|p| p.mode != 0);
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
            // the pointers' looks, taken every frame, applied only in play
            // with the camera free
            let (tx, ty, pinch) = touch.take_look();
            if !play.as_ref().is_some_and(|p| p.is_dead()) && play.as_ref().is_none_or(|p| p.look_free()) {
                let inverted = play.as_ref().is_some_and(|p| p.options.inverted != 0);
                let look = mouse.look(&mut walker, inverted);
                let feel = crate::mouse::look(&mut walker, tx, ty, pinch, inverted);
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
            let client = play.as_ref().filter(|p| !p.is_dead()).map(|p| p.control_client()).unwrap_or_default();
            if walker.frame_with(sticks, t, &client)
                && let Some(p) = play.as_mut()
            {
                p.toggle_autorun();
            }
            // `EQOA_TRACE_WALK=ms`: the body and the camera every that often
            if let Some(every) = trace_walk
                && t % every == 0
            {
                eprintln!("walk {t} ms: {} orbit {:.4} sticks {sticks:?} anim {:?}", walker.describe(), walker.cam.cur.orbit.x.0, walker.sprite.as_ref().map(|s| s.action));
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
                // `EQOA_TARGET=<name>`: that creature targeted two seconds in.
                if t == at
                    && let Ok(name) = std::env::var("EQOA_TARGET")
                {
                    let found = p.target_by_name(&name);
                    eprintln!("target {name}: {found}");
                    if !found {
                        eprintln!("  in view: {}", p.entity_names().join(", "));
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
                // keys, the spell book's while it is up, each held for a frame (pressed, then released on the next:
                // the menus' auto-repeat pulses while a key is down).
                let step = ((t - at) / 16 - 1) as usize;
                if t > at
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
                        if p.bank_owns() {
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
                // once it is up, then Enter.
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
                // `EQOA_ACTION=<hex>@<ms>`: a start-menu entry's message then
                // (`DoAction`; 0B060000 asks the target to trade).
                if let Ok(spec) = std::env::var("EQOA_ACTION")
                    && let Some((a, ms)) = spec.split_once('@')
                    && let (Ok(a), Ok(ms)) = (u32::from_str_radix(a.trim_start_matches("0x"), 16), ms.parse::<i64>())
                    && t == ms / 16 * 16
                {
                    p.do_action(a, 0);
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
                if let Some(model) = p.frame(&mut walker, &*lib, gpu, t, None)
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
                // The HUD steps each frame (its glows, the name's scroll).
                if let Some(u) = renderer.ui.as_mut() {
                    let (proj, v) = walker.view(w as f32 / h as f32);
                    p.draw_hud(u, &walker, t, proj * v, (w, h));
                }
                // TrackTarget too (the rings' clock); the last frame's are drawn
                let (proj, v) = walker.view(w as f32 / h as f32);
                p.draw_rings(&mut renderer, &device, &queue, &walker, proj * v, lib);
                // A teleport into another world: that world's file in place
                // of this one, and the walk begun again there.
                if let Some((file, pos, heading)) = p.take_world_change() {
                    match change_world(lib, &file, pos, heading) {
                        Ok((w, s2)) => {
                            walker = w;
                            streamer = s2;
                            // the particles named the old world's actors
                            parts = crate::particles::Particles::new(&walker.scene, t);
                        }
                        Err(e) => eprintln!("world change to {file}: {e}"),
                    }
                    p.world_entered(&mut walker);
                }
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
            // UpdateSprite runs every frame in the game.
            walker.pose();
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
                let mut bodies = vec![Body { actor: SELF, sprite, transform: actor, centre, attack: [0, 0] }, Body { actor: 1, sprite: None, transform: target, centre: target, attack: [0, 0] }];
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
            // the weapon trails (VITrailFxRender) after them
            if let Some(p) = play.as_mut() {
                renderer.strips.append(&mut p.trail_strips);
            }
            // the atmosphere every frame; the last frame's lights and render
            // are the shot's
            weather.frame(&mut walker.scene, None, &walker.eye, t, hour(Instant::now()));
            if t < script.ms {
                let _ = lit(proj, v, Instant::now(), Some((walker.eye_position(), &walker.scene)), Some(&mut weather));
                if let Some(p) = play.as_mut() {
                    point = p.point_light(&walker, t, hour(Instant::now()), Some(&weather));
                }
                let _ = weather.render(&walker.eye);
            }
        }
        if script.ms == 0 {
            weather.frame(&mut walker.scene, None, &walker.eye, 0, hour(Instant::now()));
        }
        eprintln!("{}", parts.describe());
        eprintln!("{}", weather.describe());
        if let Some((sp, _, _)) = &trial {
            eprintln!("{}", sp.describe());
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
        if let Some(p) = play.as_ref().filter(|p| p.has_self()) {
            p.place_self(&walker, model, &palette, &renderer, &queue);
            g.visible = false;
        }
        gpu.push(g);
        let at = Vec3::from(walker.cam.walk.phys.position.to_array());
        streamer.update(at, &mut walker.scene, lib, stream::Gpu { renderer: &mut renderer, device: &device, queue: &queue }, None);
        streamer.animate(&mut walker.scene, &renderer, &queue);
        eprintln!("{}", streamer.describe());
        let (mut proj, mut v) = walker.view(aspect);
        let mut shot_eye = walker.eye_position();
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
            proj = glam::camera::rh::proj::directx::perspective(40f32.to_radians(), aspect, 0.1, 3000.0);
            let at = p + Vec3::new(*dx, *dy, *dz);
            shot_eye = at;
            v = glam::camera::rh::view::look_at_mat4(at, p, Vec3::Y);
        }
        billboard_view = Some((v, proj));
        let mut frame = lit(proj, v, Instant::now(), Some((shot_eye, &walker.scene)), Some(&mut weather));
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
        streamed = Some(streamer);
        if let Some(p) = &mut play {
            if let Some(u) = renderer.ui.as_mut() {
                p.draw_hud(u, &walker, t, view.view_proj, (w, h));
                touch.textures(u, &device, &queue);
                p.draw_touch(u, &touch, (w, h));
            }
            let plates = p.plates();
            if let Some(t) = renderer.text.as_mut() {
                t.labels = crate::play::labels(&plates, shot_eye, view.view_proj, t);
                let chat = crate::play::chat_labels(p, t, (w, h));
                t.labels.extend(chat);
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
    let refs: Vec<render::Draw> = gpu
        .iter()
        .map(render::Draw::Model)
        .chain(streamed.iter().flat_map(|s| s.draws()))
        .chain(play.iter().flat_map(|p| p.models()).map(render::Draw::Model))
        .collect();
    let textures = render::library_textures(lib);
    let particles = billboard_view.map(|(v, p)| (v, p, batches.as_slice(), &textures as &render::Textures));
    let sky = sky_billboards.as_ref().map(|(v, p, b)| (*v, *p, b.as_slice(), &textures as &render::Textures));
    let rgba = render::capture_with_sky(&device, &queue, &mut renderer, w, h, &view, &refs, particles, sky);
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out)?), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgba)?;
    eprintln!("{out}: {} ({cam:?})", s.summary);
    report(&renderer);
    Ok(())
}

/// The texture an `EQOA_OFFSCREEN` run draws into.
fn offscreen_target(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

/// Whether the touch screen's pad shows from the start: in a page on a
/// touch-first device (`window.eqoaTouchFirst`), or with `EQOA_TOUCH=1`.
fn touch_first() -> bool {
    #[cfg(target_arch = "wasm32")]
    if crate::web::touch_first() {
        return true;
    }
    std::env::var("EQOA_TOUCH").is_ok_and(|v| v == "1")
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
    const NAMES: [&str; 14] = [
        "stream", "walk", "play", "pose", "zones", "sound", "weather", "particles", "lights", "acquire", "hud", "encode", "submit",
        "gpu wait",
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
    batches: Vec<render::billboard::Batch>,
    /// The sky and the weather (made once, kept across worlds as the
    /// scene's is), and this frame's sky billboards.
    weather: Option<crate::weather::Weather>,
    sky_batches: Vec<render::billboard::Batch>,
    /// Index in `Gfx::models` of the character.
    body_at: usize,
    /// Where the sky's models start in `Gfx::models`.
    sky_at: Option<usize>,
    /// A world change under way: the world's file, the arrival and heading,
    /// and its reads (`stream::open_world`), entered once they are in.
    entering: Option<(String, [f32; 3], f32, crate::stream::Arrival)>,
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
    pad_script: Option<crate::input::Script>,
    /// The mouse in play (`mouse.rs`), and the last frame's view (what a
    /// click picks through).
    mouse: crate::mouse::Mouse,
    last_view: Option<glam::Mat4>,
    /// The touch screen's pad (`touch.rs`).
    touch: crate::touch::TouchPad,
}

pub fn run(lib: Library, source: Source) -> Result<(), Error> {
    run_with(lib, source, None)
}

/// `run`, with a game session to play on.
pub fn run_with(lib: Library, source: Source, play: Option<crate::play::Play>) -> Result<(), Error> {
    run_with_gfx(lib, source, play, None)
}

/// `run_with` on a surface made beforehand. In a browser the loop is handed
/// to the page and this returns at once.
pub fn run_with_gfx(lib: Library, source: Source, play: Option<crate::play::Play>, pending: Option<PendingGfx>) -> Result<(), Error> {
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
        batches: Vec::new(),
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
        pad_script: crate::input::Script::from_env(),
        mouse: crate::mouse::Mouse::default(),
        last_view: None,
        touch: crate::touch::TouchPad::new(touch_first()),
    };
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(app);
    }
    #[cfg(not(target_arch = "wasm32"))]
    event_loop.run_app(&mut app)?;
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
            g.window.set_title(&format!("eqoa — {}", self.summary));
            g.window.request_redraw();
        }
    }

    /// A world change in the window, once its reads are in
    /// (`stream::open_world`): the file replaces the world's, the zones read
    /// round the arrival go in, and `load` begins the walk again there.
    fn enter_world(&mut self, file: &str, c: esf::Container, zones: Vec<(usize, Vec<u8>)>, pos: [f32; 3], heading: f32) -> Result<(), Error> {
        self.lib.replace(0, file, c);
        for (zi, b) in zones {
            stream::install(&mut self.lib, 0, zi, b)?;
        }
        let index = vi::world::World::<vi::fpu::Ee>::zone_at(self.lib.file(0), pos[0], pos[2]).ok_or("no zone at the arrival")?;
        stream::load_zone_now(&mut self.lib, 0, index)?;
        let Source::Walk { character, .. } = self.source else { return Err("not walking".into()) };
        self.source = Source::Walk { file: 0, index, character, spawn: Some((pos[0], pos[2], heading)) };
        self.load();
        Ok(())
    }

    fn step(&mut self, by: isize) {
        if let Source::Models { list, index, .. } = &mut self.source {
            *index = (*index as isize + by).rem_euclid(list.len() as isize) as usize;
            self.load();
        }
    }

    /// The walker's controls from the held keys (`UpdateControls`'s keyboard
    /// half): W/S forward, A/D strafe, ←/→ (or J/L) orbit, ↑/↓ (or I/K) zoom.
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
        let either = |x: f32, y: f32| if x != 0.0 { x } else { y };
        Controls { fwd: k("w", "s"), strafe: k("d", "a"), orbit: either(k("right", "left"), k("l", "j")), pitch: either(k("down", "up"), k("k", "i")) }
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
        if self.touch.used {
            report.merge(&self.touch.report());
            connected = true;
        }
        let dead = self.play.as_ref().is_some_and(|p| p.is_dead());
        let keys = self.console.poll(report, connected, motors, dead);
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
        let mut a = self.controls().axes();
        // the pointers' looks, taken every frame, applied only in play with
        // the camera free
        let (tx, ty, pinch) = self.touch.take_look();
        let dead = self.play.as_ref().is_some_and(|p| p.is_dead());
        let free = !dead && self.play.as_ref().is_none_or(|p| p.look_free());
        let inverted = self.play.as_ref().is_some_and(|p| p.options.inverted != 0);
        let (look, feel) = match self.walker.as_mut() {
            Some(w) if free => (self.mouse.look(w, inverted), crate::mouse::look(w, tx, ty, pinch, inverted)),
            _ => {
                // a window up: the wheel and a finger's drag are its up and down
                let size = self.gfx.as_ref().map_or((640, 448), |g| (g.config.width, g.config.height));
                let lines = self.mouse.take_wheel() + crate::touch::drag_lines(ty, size);
                self.mouse.discard();
                if let Some(p) = self.play.as_mut() {
                    p.window_wheel(lines);
                }
                ([0.0; 2], [0.0; 2])
            }
        };
        if dead {
            return a;
        }
        // both mouse buttons: run forward (`Mouse::running`)
        if free && self.mouse.running() {
            a[3] = -1.0;
        }
        let pad = self.console.sticks();
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

    /// What a report from the page says of the client (web/report.js): the
    /// frames, the surface, the files in the library and play's own.
    #[cfg(target_arch = "wasm32")]
    fn report_state(&self) -> String {
        use std::fmt::Write;
        let mut o = String::new();
        let _ = writeln!(o, "frames {}, clock {:.1} s", FRAMES.load(std::sync::atomic::Ordering::Relaxed), self.clock.elapsed().as_secs_f64());
        if let Some(g) = &self.gfx {
            let _ = writeln!(o, "surface {}x{} {:?}", g.config.width, g.config.height, g.config.format);
        }
        let _ = writeln!(o, "files: {}", self.lib.files.iter().map(|f| f.0.as_str()).collect::<Vec<_>>().join(", "));
        // the controls: a pad's sticks as they came (before the host's dead
        // zone) and as the game reads them, and the camera's mode
        let mode = self.walker.as_ref().map_or(-1, |w| w.cam.camera_mode);
        let _ = writeln!(o, "sticks rx ry lx ly: pad {:.3?} (dead zone {}), the game's {:.4?}; camera mode {mode}", self.pads.raw, self.pads.dead_zone, self.console.sticks());
        if let Some(e) = &self.entering {
            let _ = writeln!(o, "entering {} at {:?}", e.0, e.1);
        }
        if let (Some(p), Some(w)) = (&self.play, &self.walker) {
            o.push_str(&p.report(w.cam.walk.phys.position.to_array()));
        }
        o
    }

    /// The renderer on a configured surface (or none: an offscreen target of
    /// the configuration's size), then the scene.
    fn attach(&mut self, window: Arc<Window>, surface: Option<wgpu::Surface<'static>>, config: wgpu::SurfaceConfiguration, device: wgpu::Device, queue: wgpu::Queue) {
        let mut renderer = Renderer::new(&device, &queue, config.format);
        if let Some(f) = arial(&self.lib) {
            renderer.set_font(&device, &queue, &f);
        }
        if let Some(p) = self.play.as_mut() {
            p.attach_ui(&mut renderer, &device, &queue, config.format, &self.lib);
        }
        let depth = render::depth_view(&device, config.width, config.height);
        let offscreen = surface.is_none().then(|| offscreen_target(&device, &config));
        self.gfx = Some(Gfx { window, surface, offscreen, in_flight: None, device, queue, config, depth, renderer, models: Vec::new() });
        self.load();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        if let Some(p) = self.pending.take() {
            let window = Arc::new(el.create_window(p.attrs).expect("window"));
            self.attach(window, Some(p.surface), p.config, p.device, p.queue);
            return;
        }
        let window = Arc::new(el.create_window(Window::default_attributes().with_title("eqoa")).expect("window"));
        // `EQOA_OFFSCREEN=WxH`: drawn on the adapter the environment names
        // into a texture of that size, never presented. A GPU with no display
        // to present on (an X server without DRI3) measures as it plays.
        if let Some((w, h)) = std::env::var("EQOA_OFFSCREEN").ok().and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
        }) {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
            let adapter = pollster::block_on(instance.request_adapter(&Default::default())).expect("adapter");
            eprintln!("offscreen {w}x{h} on {:?}", adapter.get_info().name);
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
        surface.configure(&device, &config);
        self.attach(window, Some(surface), config, device, queue);
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if (!self.held.is_empty() || self.walker.is_some())
            && let Some(g) = &self.gfx
        {
            g.window.request_redraw();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            // Quit, answered yes in the start menu (`m_p2EC28`), or the
            // server ended the connection.
            _ if self.play.as_ref().is_some_and(|p| p.quit || p.disconnected.is_some()) => el.exit(),
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
            WindowEvent::KeyboardInput { event, .. } => {
                // a keyboard in use (`gKeyboardAttached`): the edit boxes'
                // compact face, with the options' Soft Keyboard on; the
                // touch screen's pad goes
                if event.state == ElementState::Pressed {
                    self.touch.hide();
                    if let Some(p) = self.play.as_mut() {
                        p.keyboard = true;
                    }
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
                    // the who list (mode 35): Q asks again
                    if p.mode == 35 {
                        let k = match &event.logical_key {
                            Key::Named(NamedKey::ArrowUp) => Some(K::UP),
                            Key::Named(NamedKey::ArrowDown) => Some(K::DOWN),
                            Key::Named(NamedKey::Enter) => Some(K::CROSS),
                            Key::Character(c) if c.eq_ignore_ascii_case("e") => Some(K::CROSS),
                            Key::Character(c) if c.eq_ignore_ascii_case("q") => Some(113),
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
                    // the options, buddy and ignore lists (modes 34, 17/18,
                    // 26): arrows, Enter or E, Escape
                    if p.panel_up() {
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
                        (None, Key::Character(c)) if c.eq_ignore_ascii_case("q") => {
                            p.interact_with_target();
                            return;
                        }
                        // P: InitCameraStyle (first person and back).
                        (None, Key::Character(c)) if c.eq_ignore_ascii_case("p") => {
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
                    if event.state == ElementState::Pressed && !event.repeat {
                        p.attack_press();
                    } else if event.state == ElementState::Released {
                        p.attack_release();
                    }
                    return;
                }
                // The d-pad (arrows) and X (E) in play: the HUD's quick menus
                // (`VIWndHUDMenu`: up/down the selection, left/right the menu,
                // E uses it). The camera is on I/J/K/L then, a keyboard's
                // right stick.
                if let Some(p) = self.play.as_mut()
                    && p.mode == 0
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
                // Tab: the next target (`CycleTarget`).
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::Tab)
                    && let (Some(p), Some(w), Some(g)) = (self.play.as_mut(), self.walker.as_ref(), self.gfx.as_ref())
                {
                    let (proj, v) = w.view(g.config.width as f32 / g.config.height as f32);
                    let dir = Vec3::from(w.eye.direction.to_array());
                    p.cycle_target(false, w.eye_position(), dir, proj * v);
                    return;
                }
                let name = match &event.logical_key {
                    Key::Named(NamedKey::Shift) => Some("shift".to_string()),
                    Key::Named(NamedKey::ArrowLeft) => Some("left".to_string()),
                    Key::Named(NamedKey::ArrowRight) => Some("right".to_string()),
                    Key::Named(NamedKey::ArrowUp) => Some("up".to_string()),
                    Key::Named(NamedKey::ArrowDown) => Some("down".to_string()),
                    Key::Character(c) => Some(c.to_lowercase()),
                    _ => None,
                };
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
            // a release that may never come: let go of what is held
            WindowEvent::Focused(false) | WindowEvent::CursorLeft { .. } => self.mouse.release_all(),
            WindowEvent::MouseInput { state, button, .. } if matches!(button, MouseButton::Left | MouseButton::Right) => {
                let down = state == ElementState::Pressed;
                if self.walker.is_some() {
                    if let Some(c) = self.mouse.button(button == MouseButton::Right, down, Instant::now()) {
                        self.click(c, false);
                    }
                } else if button == MouseButton::Left {
                    self.drag = down.then_some(self.cursor);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                self.mouse.moved(position.x, position.y);
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
                if self.walker.is_some() {
                    self.mouse.wheel(d, steps);
                } else if let Camera::Orbit { zoom, .. } = &mut self.camera {
                    *zoom = (*zoom * 0.9f32.powf(d)).clamp(0.05, 20.0);
                }
                if let Some(g) = &self.gfx {
                    g.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                // A page on WebGL2 draws at most 30 frames a second
                // (`web::frame_due`): an animation frame before the next is
                // due is skipped whole, the game included, so its clocks see
                // longer frames rather than fewer ticks.
                #[cfg(target_arch = "wasm32")]
                if !crate::web::frame_due() {
                    if let Some(g) = &self.gfx {
                        g.window.request_redraw();
                    }
                    return;
                }
                FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let frame_start = Instant::now();
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
                if let Some((file, pos, heading)) = self.play.as_mut().and_then(|p| p.take_world_change()) {
                    let reads = stream::open_world(&file, pos[0], pos[2]);
                    self.entering = Some((file, pos, heading, reads));
                }
                if let Some(r) = self.entering.as_ref().and_then(|e| e.3.borrow_mut().take()) {
                    let (file, pos, heading, _) = self.entering.take().unwrap();
                    match r.map_err(Error::from).and_then(|(c, zones)| self.enter_world(&file, c, zones, pos, heading)) {
                        Ok(()) => crate::say(&format!("entered {file} at {pos:?}")),
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
                self.touch.window = self.play.as_ref().is_some_and(|p| p.mode != 0);
                mark(&mut self.perf, 0);
                let sticks = self.sticks();
                let now = self.clock.elapsed().as_millis() as i64;
                let Some(g) = &mut self.gfx else { return };
                let body_at = self.body_at;
                let clock = self.clock;
                let streamer = &mut self.streamer;
                let sound = &mut self.sound;
                let walker_sounds = &mut self.walker_sounds;
                let particles = &mut self.particles;
                let batches = &mut self.batches;
                let weather = &mut self.weather;
                let sky_batches = &mut self.sky_batches;
                let mut bb_view = None;
                let play = &mut self.play;
                let lib = &self.lib;
                let perf = &mut self.perf;
                let console = &mut self.console;
                let pads = &mut self.pads;
                let walk_view = self.walker.as_mut().map(|w| {
                    // UpdateControls, on the sticks, as the client stands
                    let client = play.as_ref().filter(|p| !p.is_dead()).map(|p| p.control_client()).unwrap_or_default();
                    if w.frame_with(sticks, now, &client)
                        && let Some(p) = play.as_mut()
                    {
                        p.toggle_autorun();
                    }
                    mark(perf, 1);
                    if let Some(p) = play {
                        let gpu = crate::play::Gpu { renderer: &mut g.renderer, device: &g.device, queue: &g.queue };
                        // The server named the player's model: dress the walker in it.
                        if let Some(model) = p.frame(w, lib, gpu, now, sound.as_mut())
                            && let Some(r) = lib.lookup(model as i32)
                            && let Some(m) = lib.build_model(r)
                        {
                            w.set_character(lib, r);
                            let textures = render::library_textures(lib);
                            let (mat, _) = w.pose();
                            if body_at < g.models.len() {
                                g.models[body_at] = g.renderer.upload(&g.device, &g.queue, &m, &[mat], &textures);
                            }
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
                    if let Some(body) = g.models.get(body_at) {
                        g.renderer.set_instances(&g.queue, body, &[m]);
                        g.renderer.set_palette(&g.queue, body, &palette);
                    }
                    // Dressed as the server says: the plain body gives way.
                    if let Some(p) = play.as_ref().filter(|p| p.has_self()) {
                        p.place_self(w, m, &palette, &g.renderer, &g.queue);
                        if let Some(body) = g.models.get_mut(body_at) {
                            body.visible = false;
                        }
                    }
                    mark(perf, 3);
                    if let Some(s) = streamer {
                        // One zone a frame, as the game's loader takes them.
                        let at = Vec3::from(w.cam.walk.phys.position.to_array());
                        let gpu = stream::Gpu { renderer: &mut g.renderer, device: &g.device, queue: &g.queue };
                        s.update(at, &mut w.scene, lib, gpu, Some(1));
                        s.animate(&mut w.scene, &g.renderer, &g.queue);
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
                        s.frame(&mut w.scene, &w.eye, hour(clock), now);
                    }
                    match play {
                        Some(p) => g.window.set_title(&format!("eqoa — {} — {} — {}", p.player.name, w.describe(), p.describe())),
                        None => g.window.set_title(&format!("eqoa — walking {}", w.describe())),
                    }
                    mark(perf, 5);
                    if let Some(wx) = weather.as_mut() {
                        wx.frame(&mut w.scene, sound.as_mut(), &w.eye, now, hour(clock));
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
                    // the weapon trails (VITrailFxRender) after them
                    if let Some(p) = play.as_mut() {
                        g.renderer.strips.append(&mut p.trail_strips);
                    }
                    mark(perf, 7);
                    let mut lights = lit(proj, v, clock, Some((w.eye_position(), &w.scene)), weather.as_mut());
                    if let Some(p) = play.as_mut() {
                        lights.0.point = p.point_light(w, now, hour(clock), weather.as_ref());
                    }
                    if let Some(wx) = weather.as_mut() {
                        let (sky, world) = wx.render(&w.eye);
                        *sky_batches = sky;
                        batches.extend(world);
                        bb_view = Some((v, proj));
                    }
                    mark(perf, 8);
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
                        p.draw_hud(u, w, self.clock.elapsed().as_millis() as i64, view.view_proj, (g.config.width, g.config.height));
                        self.touch.textures(u, &g.device, &g.queue);
                        p.draw_touch(u, &self.touch, (g.config.width, g.config.height));
                    }
                    p.draw_rings(&mut g.renderer, &g.device, &g.queue, w, view.view_proj, &self.lib);
                    let plates = p.plates();
                    if let Some(t) = g.renderer.text.as_mut() {
                        t.labels = crate::play::labels(&plates, w.eye_position(), view.view_proj, t);
                        let chat = crate::play::chat_labels(p, t, (g.config.width, g.config.height));
                        t.labels.extend(chat);
                    }
                }
                mark(&mut self.perf, 10);
                let target = match (&frame, &g.offscreen) {
                    (Some(f), _) => f.texture.create_view(&Default::default()),
                    (None, Some(t)) => t.create_view(&Default::default()),
                    (None, None) => return,
                };
                let mut enc = g.device.create_command_encoder(&Default::default());
                let refs: Vec<render::Draw> = g
                    .models
                    .iter()
                    .map(render::Draw::Model)
                    .chain(self.streamer.iter().flat_map(|s| s.draws()))
                    .chain(self.play.iter().flat_map(|p| p.models()).map(render::Draw::Model))
                    .collect();
                g.renderer.draw(&g.queue, &mut enc, &target, &g.depth, &view, &refs);
                if let Some((v, p)) = bb_view {
                    let textures = render::library_textures(&self.lib);
                    let mut rot = v;
                    rot.w_axis = glam::Vec4::W;
                    g.renderer.draw_sky_billboards(&g.device, &g.queue, &mut enc, &target, &g.depth, rot, p, &self.sky_batches, &textures);
                    g.renderer.draw_billboards(&g.device, &g.queue, &mut enc, &target, &g.depth, v, p, &self.batches, &textures);
                }
                g.renderer.draw_text(&g.device, &mut enc, &target, &g.depth, (g.config.width, g.config.height));
                mark(&mut self.perf, 11);
                let sent = g.queue.submit([enc.finish()]);
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
                // The page's picture of this frame, if it asked, and its
                // report's state of the client (web/report.js).
                #[cfg(target_arch = "wasm32")]
                {
                    crate::web::frame_drawn();
                    crate::web::note_state(|| self.report_state());
                }
            }
            _ => {}
        }
    }
}
