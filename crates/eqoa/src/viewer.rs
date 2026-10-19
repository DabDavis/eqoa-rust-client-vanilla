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
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

type Error = Box<dyn std::error::Error>;

/// The hour of day: `EQOA_HOUR` if set, else 10:00 (InitTimeOfDay's) moving
/// at the game's 24 hours an hour from when the viewer started.
fn hour(start: Instant) -> f32 {
    match std::env::var("EQOA_HOUR").ok().and_then(|h| h.parse::<f32>().ok()) {
        Some(h) => h,
        None => (10.0 + start.elapsed().as_secs_f32() / 150.0) % 24.0,
    }
}

/// A frame's lights out of the time of day (clear sky).
/// A frame's lights, fog and sky out of the time of day (clear sky), for the
/// camera at `eye` in `world` (its terrain profile and env flags), if any.
pub fn lit(
    proj: glam::Mat4,
    view: glam::Mat4,
    start: Instant,
    eye: Option<(Vec3, &vi::world::World<vi::fpu::Ee>)>,
) -> (View, Option<crate::sky::SkyFrame>) {
    let view_proj = proj * view;
    let mut rot = view;
    rot.w_axis = glam::Vec4::W;
    use vi::fpu::{Ee, F};
    let tod = vi::tod::TimeOfDay::<Ee>::new();
    let hour = F::new(hour(start));
    let (amb, sun, dir) = tod.lights(hour, F::ZERO);
    let rgb = |c: [F<Ee>; 3]| Vec3::new(c[0].0, c[1].0, c[2].0);
    let mut v = View {
        view_proj,
        sun_dir: Vec3::from(dir.to_array()),
        sun: rgb(sun),
        ambient: rgb(amb),
        fog: None,
        clear: Vec3::new(0.18, 0.2, 0.24),
        sky_view_proj: None,
    };
    let mut sky = None;
    if let Some((at, w)) = eye
        && let Some(grid) = &w.grid
    {
        let at = vi::vect::Vect3::of(at.x, at.y, at.z);
        let b = grid.profile_blend(&at);
        let fog = tod.fog(hour, F::ZERO, &b, w.env_flags(&at), &sun);
        v.fog = Some((fog.near.0, fog.far.0, Vec3::new(fog.color[0].0, fog.color[1].0, fog.color[2].0)));
        let c = tod.sky(hour, F::ZERO, &b);
        v.clear = Vec3::new(c[0] as f32, c[1] as f32, c[2] as f32) / 255.0;
        v.sky_view_proj = Some(proj * rot);
        sky = Some(crate::sky::SkyFrame {
            ty: vi::atmosphere::sky_type(&b),
            rgb: v.clear,
            rot: vi::atmosphere::sky_rotations(hour, F::new(1.0)),
        });
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
            // Only for the spawn point: a walk's zones are streamed.
            Source::Walk { file, index, .. } => {
                let c = lib.file(*file);
                scene::zone(lib, *file, c.zones()[*index], *index)
            }
        }
    }

    /// For a walk: the character in the middle of the zone, on the game's own
    /// collision world (the whole world file's, not just this zone's), and a
    /// streamer for the zones to draw.
    pub fn walker(&self, lib: &Library, s: &Scene) -> Option<(Walker, esf::Model, Streamer)> {
        let Source::Walk { file, character, spawn: given, .. } = self else { return None };
        let t = Instant::now();
        let world = vi::world::World::load(lib, *file)?;
        eprintln!(
            "collision world: {} zones, {} buffers, {} actors, in {:.2?}",
            world.zones.len(),
            world.buffers.len(),
            world.actors.len(),
            t.elapsed()
        );
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
        let mut w = Walker::new(world, x, z, h);
        w.set_character(lib, *character);
        Some((w, lib.build_model(*character)?, Streamer::new(*file)))
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
        lit(proj, glam::camera::rh::view::look_at_mat4(eye, target, Vec3::Y), Instant::now(), None).0
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
    s.parts.iter().map(|(m, mats)| renderer.upload(device, queue, m, mats, &textures)).collect()
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
    lib: &Library,
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
    lib: &Library,
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
    let s = source.scene(lib);
    let walking = matches!(source, Source::Walk { .. });
    let mut gpu = if walking { Vec::new() } else { upload(&mut renderer, &device, &queue, lib, &s) };
    let mut streamed = None;
    let cam = cam.unwrap_or_else(|| source.camera(s.bounds));
    let aspect = w as f32 / h as f32;
    let mut view = cam.view(s.bounds, aspect);
    if let Some((mut walker, body, mut streamer)) = source.walker(lib, &s) {
        // 60 frames a second of game time, the controls held throughout.
        let script = script.unwrap_or(Script { ms: 0, controls: Controls::default() });
        let mut t = 0;
        while t < script.ms {
            t += 16;
            walker.frame(script.controls, t);
            if let Some(p) = &mut play {
                let gpu = crate::play::Gpu { renderer: &mut renderer, device: &device, queue: &queue };
                if let Some(model) = p.frame(&mut walker, lib, gpu, t)
                    && let Some(r) = lib.lookup(model as i32)
                {
                    eprintln!("the server names the player's model {model:#010x}");
                    walker.set_character(lib, r);
                }
                std::thread::sleep(std::time::Duration::from_millis(16));
            }
            // UpdateSprite runs every frame in the game.
            walker.pose();
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
        let (proj, v) = walker.view(aspect);
        let mut frame = lit(proj, v, Instant::now(), Some((walker.eye_position(), &walker.scene)));
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
            let proj = glam::camera::rh::proj::directx::perspective(40f32.to_radians(), aspect, 0.1, 3000.0);
            let at = p + Vec3::new(*dx, *dy, *dz);
            frame = lit(proj, glam::camera::rh::view::look_at_mat4(at, p, Vec3::Y), Instant::now(), Some((at, &walker.scene)));
        }
        if let (Some(f), Some(at)) = (&frame.1, crate::sky::upload(&mut renderer, &device, &queue, lib, &mut gpu)) {
            crate::sky::update(&renderer, &queue, &mut gpu, at, f);
        }
        view = frame.0;
        streamed = Some(streamer);
        if let Some(p) = &play {
            eprintln!("play: {}", p.describe());
            for e in p.entities.slots.iter().flatten() {
                let name = e.creature.as_ref().map(|c| c.name.as_str()).unwrap_or("");
                eprintln!("  entity {:#x} {name:24} model {:#010x} at ({:.1}, {:.1}, {:.1})", e.id, e.model, e.pos[0], e.pos[1], e.pos[2]);
            }
        }
    }
    let refs: Vec<&GpuModel> = gpu.iter().chain(streamed.iter().flat_map(|s| s.models())).chain(play.iter().flat_map(|p| p.models())).collect();
    let rgba = render::capture(&device, &queue, &renderer, w, h, &view, &refs);
    let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(out)?), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgba)?;
    eprintln!("{out}: {} ({cam:?})", s.summary);
    report(&renderer);
    Ok(())
}

struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    renderer: Renderer,
    models: Vec<GpuModel>,
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
    /// Index in `Gfx::models` of the character.
    body_at: usize,
    /// Where the sky's models start in `Gfx::models`.
    sky_at: Option<usize>,
    clock: Instant,
    gfx: Option<Gfx>,
    display: winit::event_loop::OwnedDisplayHandle,
}

pub fn run(lib: Library, source: Source) -> Result<(), Error> {
    run_with(lib, source, None)
}

/// `run`, with a game session to play on.
pub fn run_with(lib: Library, source: Source, play: Option<crate::play::Play>) -> Result<(), Error> {
    let event_loop = EventLoop::new()?;
    let display = event_loop.owned_display_handle();
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
        body_at: 0,
        sky_at: None,
        clock: Instant::now(),
        gfx: None,
        display,
    };
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
        let walker = self.source.walker(&self.lib, &s);
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
                w
            });
            self.sky_at = None;
            if self.walker.is_some() {
                self.sky_at = crate::sky::upload(&mut g.renderer, &g.device, &g.queue, &self.lib, &mut g.models);
            }
            self.clock = Instant::now();
            report(&g.renderer);
            g.window.set_title(&format!("eqoa — {}", self.summary));
            g.window.request_redraw();
        }
    }

    fn step(&mut self, by: isize) {
        if let Source::Models { list, index, .. } = &mut self.source {
            *index = (*index as isize + by).rem_euclid(list.len() as isize) as usize;
            self.load();
        }
    }

    /// The walker's controls from the held keys (`UpdateControls`'s keyboard
    /// half): W/S forward, A/D strafe, ←/→ orbit, ↑/↓ zoom.
    fn controls(&self) -> Controls {
        let k = |a: &str, b: &str| self.held.contains(a) as i32 as f32 - self.held.contains(b) as i32 as f32;
        Controls { fwd: k("w", "s"), strafe: k("d", "a"), orbit: k("right", "left"), pitch: k("down", "up") }
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
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let window = Arc::new(el.create_window(Window::default_attributes().with_title("eqoa")).expect("window"));
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
        let renderer = Renderer::new(&device, &queue, config.format);
        let depth = render::depth_view(&device, config.width, config.height);
        self.gfx = Some(Gfx { window, surface, device, queue, config, depth, renderer, models: Vec::new() });
        self.load();
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
            WindowEvent::Resized(s) if s.width > 0 && s.height > 0 => {
                let Some(g) = &mut self.gfx else { return };
                g.config.width = s.width;
                g.config.height = s.height;
                g.surface.configure(&g.device, &g.config);
                g.depth = render::depth_view(&g.device, s.width, s.height);
                g.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. } => {
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
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                self.drag = (state == ElementState::Pressed).then_some(self.cursor);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x, position.y);
                if let Some((x, y)) = self.drag {
                    if let Some(w) = &mut self.walker {
                        // The right stick, by mouse: a drag is an orbit push.
                        let k = vi::fpu::F::new(((position.x - x) as f32 * 0.05).clamp(-1.0, 1.0));
                        w.cam.orbit_right(k);
                    }
                    self.camera.drag((position.x - x) as f32, (position.y - y) as f32);
                    self.drag = Some(self.cursor);
                    if let Some(g) = &self.gfx {
                        g.window.request_redraw();
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                if let Camera::Orbit { zoom, .. } = &mut self.camera {
                    *zoom = (*zoom * 0.9f32.powf(d)).clamp(0.05, 20.0);
                }
                if let Some(g) = &self.gfx {
                    g.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                self.fly();
                let controls = self.controls();
                let now = self.clock.elapsed().as_millis() as i64;
                let Some(g) = &mut self.gfx else { return };
                let body_at = self.body_at;
                let clock = self.clock;
                let streamer = &mut self.streamer;
                let play = &mut self.play;
                let lib = &self.lib;
                let walk_view = self.walker.as_mut().map(|w| {
                    w.frame(controls, now);
                    if let Some(p) = play {
                        let gpu = crate::play::Gpu { renderer: &mut g.renderer, device: &g.device, queue: &g.queue };
                        // The server named the player's model: dress the walker in it.
                        if let Some(model) = p.frame(w, lib, gpu, now)
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
                    }
                    let (m, palette) = w.pose();
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
                    if let Some(s) = streamer {
                        // One zone a frame, as the game's loader takes them.
                        let at = Vec3::from(w.cam.walk.phys.position.to_array());
                        let gpu = stream::Gpu { renderer: &mut g.renderer, device: &g.device, queue: &g.queue };
                        s.update(at, &mut w.scene, lib, gpu, Some(1));
                        s.animate(&mut w.scene, &g.renderer, &g.queue);
                    }
                    match play {
                        Some(p) => g.window.set_title(&format!("eqoa — {} — {} — {}", p.player.name, w.describe(), p.describe())),
                        None => g.window.set_title(&format!("eqoa — walking {}", w.describe())),
                    }
                    let (proj, v) = w.view(g.config.width as f32 / g.config.height as f32);
                    lit(proj, v, clock, Some((w.eye_position(), &w.scene)))
                });
                let frame = match g.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                    _ => {
                        g.surface.configure(&g.device, &g.config);
                        return;
                    }
                };
                let view = match walk_view {
                    Some((v, sky)) => {
                        if let (Some(f), Some(at)) = (sky, self.sky_at) {
                            crate::sky::update(&g.renderer, &g.queue, &mut g.models, at, &f);
                        }
                        v
                    }
                    None => self.camera.view(self.bounds, g.config.width as f32 / g.config.height as f32),
                };
                let target = frame.texture.create_view(&Default::default());
                let mut enc = g.device.create_command_encoder(&Default::default());
                let refs: Vec<&GpuModel> = g
                    .models
                    .iter()
                    .chain(self.streamer.iter().flat_map(|s| s.models()))
                    .chain(self.play.iter().flat_map(|p| p.models()))
                    .collect();
                g.renderer.draw(&g.queue, &mut enc, &target, &g.depth, &view, &refs);
                g.queue.submit([enc.finish()]);
                g.queue.present(frame);
            }
            _ => {}
        }
    }
}
