//! The pregame in the window (`viewer.rs`): its 2D layer (the files' UI
//! surfaces and the ArialUI font on `render::ui`, the loading pictures as
//! full-frame textures), the frame drawn once a frame of the game's and kept
//! for the frames between (as the HUD is, `frames.rs`), its sounds and music
//! on the client's sound device, and the hand-off to `Play` at the world
//! entry: the world's files read meanwhile (natively on a thread) under
//! CLIENT's loading screen, the session pumped and its events kept for the
//! game (`Session::held`).
//!
//! Headless, for tests: `EQOA_KEYS` types on the keyboard by script
//! (`enter@3000;down@3400;text player@4000`: a key's name, or `text` and
//! what to type, at ms on the client's clock), `EQOA_SHOTS` writes PNGs of
//! what was drawn (`3000=eula.png,9000=title.png`, under `EQOA_OFFSCREEN`).

use super::{Entered, Music, Pregame};
use crate::ui::UiAssets;
use render::ui::{Quad, Ui};
use render::{Renderer, wgpu};
use std::collections::HashMap;

/// The files whose surfaces the 2D layer draws.
const UI_FILES: &[&str] = &["UI.CSF", "SETUP.CSF", "ADDART.CSF", "STATUI.CSF", "PREGAME.CSF"];

/// The world's library and walk, as `main.rs`'s `world_library` builds them.
pub type World = (esf::Library, crate::viewer::Source);

/// How a page reads the world entry's files (`set_world_loader`).
pub type WorldLoader = Box<dyn Fn(&::net::session::Player) -> super::link::Later<World>>;

thread_local! {
    static WORLD_LOADER: std::cell::RefCell<Option<WorldLoader>> = const { std::cell::RefCell::new(None) };
}

/// Makes `f` the way the world entry's files are read (a page's).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn set_world_loader(f: WorldLoader) {
    WORLD_LOADER.with(|c| *c.borrow_mut() = Some(f));
}

/// The world's files being read for `Play` (natively on a thread).
pub struct WorldLoad {
    pub entered: Box<Entered>,
    reads: super::link::Later<World>,
}

/// What the window does next.
pub enum Done {
    Stay,
    /// the world is read: play it
    Play(Box<Entered>, esf::Library, crate::viewer::Source),
    /// the window closes
    Quit,
}

pub struct Host {
    pub pre: Pregame,
    assets: Option<UiAssets>,
    /// the loading pictures on the layer, by number
    pictures: HashMap<i32, usize>,
    /// music playing: (playback, the device's sound) by the stage's tag
    playing: HashMap<&'static str, (i32, i32)>,
    keys: Vec<(i64, i32, bool)>,
    /// keys a pointer pressed, let go a frame later
    pointer_held: Vec<i32>,
    pub world: Option<WorldLoad>,
    /// CLIENT's loading screen's gears (UI.CSF's `Loading_Gears`) on the GPU,
    /// once looked for
    gears: Option<Option<(crate::loading::Gears, Vec<render::GpuModel>)>>,
}

impl Host {
    pub fn new(pre: Pregame) -> Self {
        Host {
            pre,
            assets: None,
            pictures: HashMap::new(),
            playing: HashMap::new(),
            keys: key_script(),
            pointer_held: Vec::new(),
            world: None,
            gears: None,
        }
    }

    /// The layer and its surfaces: made the first time, then the files read
    /// since the last frame put up.
    fn layer(&mut self, renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) {
        if renderer.ui.is_none() || self.assets.is_none() {
            let mut ui = Ui::new(device, format, render::DEPTH_FORMAT);
            let mut assets = UiAssets::default();
            assets.add_white(&mut ui, device, queue);
            renderer.ui = Some(ui);
            self.assets = Some(assets);
            self.pictures.clear();
            // every file already read goes up again
            let n = self.pre.res.lib.files.len();
            self.upload(renderer, device, queue, 0..n);
        }
        let fresh = self.pre.res.take_fresh();
        for i in fresh {
            self.upload(renderer, device, queue, i..i + 1);
        }
    }

    fn upload(&mut self, renderer: &mut Renderer, device: &wgpu::Device, queue: &wgpu::Queue, files: std::ops::Range<usize>) {
        let (Some(ui), Some(assets)) = (renderer.ui.as_mut(), self.assets.as_mut()) else { return };
        for i in files {
            let (name, c) = &self.pre.res.lib.files[i];
            if UI_FILES.iter().any(|f| f.eq_ignore_ascii_case(name)) {
                assets.add_surfaces(ui, device, queue, c);
            }
            if assets.font.is_none() {
                let id = vi::ui::font_id() as i32;
                if let Some(f) = (0..c.nodes.len()).filter(|&n| c.nodes[n].ty == 0x7000).find_map(|n| esf::font::read(c, n).filter(|f| f.id == id)) {
                    assets.font = Some(ui.add_font(device, queue, &f));
                }
            }
        }
    }

    /// A key the window took (the game's code).
    pub fn key(&mut self, key: i32, down: bool) {
        self.pre.key(key, down);
    }

    /// A click at frame point `p`: the keys the window under it asks for,
    /// pressed now and let go on the next frame.
    pub fn click(&mut self, p: [f32; 2]) {
        for k in self.pre.point(p) {
            if k == crate::pointer::MOVED {
                continue;
            }
            self.pre.key(k, true);
            self.pointer_held.push(k);
        }
    }

    /// One frame drawn: the pregame's frame on one of the game's (its quads
    /// kept for the frames between), the sounds, and what comes next.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        size: (u32, u32),
        now: i64,
        game_frame: bool,
        sound: Option<&mut crate::sound::Sound>,
    ) -> Done {
        for k in std::mem::take(&mut self.pointer_held) {
            self.pre.key(k, false);
        }
        // EQOA_KEYS: the script's keys due
        while let Some(&(t, k, down)) = self.keys.first() {
            if t > now {
                break;
            }
            self.keys.remove(0);
            self.pre.key(k, down);
        }
        self.layer(renderer, device, queue, format);
        let mut cues = Vec::new();
        let mut music = Vec::new();
        if let Some(w) = self.world.as_mut() {
            // the world's files coming: the session kept alive, the loading screen up
            let events = w.entered.session.pump();
            w.entered.session.held.extend(events);
        }
        if game_frame {
            // the layer out of the renderer while the windows draw on it, so a
            // window's 3D can use the renderer meanwhile (`Ctx::gpu`)
            let mut layer = renderer.ui.take().unwrap();
            let ui = &mut layer;
            ui.begin(size);
            let assets = self.assets.as_ref().unwrap();
            if let Some(n) = self.pre.loading_picture() {
                // the launcher's picture, put straight into the frame
                if !self.pictures.contains_key(&n)
                    && let Some(rgba) = self.pre.res.picture(n)
                {
                    let t = ui.add_texture(device, queue, super::boot::WIDTH as u32, super::boot::HEIGHT as u32, &rgba);
                    self.pictures.insert(n, t);
                }
                if let Some(&tex) = self.pictures.get(&n) {
                    let (w, h) = (super::boot::WIDTH as f32, super::boot::HEIGHT as f32);
                    ui.push(Quad { tex, blend: 0, pos: [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]], uv: [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], color: [1.0; 4], clip: None });
                }
                self.pre.update(now);
            } else {
                if let Some(v) = self.pre.res.vui_mut() {
                    v.time = now;
                }
                let mut r = assets.raster(ui);
                if self.world.is_some() || self.pre.client_loading() {
                    // `LoadingProgress`'s 2D half (the gears are drawn by the window)
                    crate::loading::draw_2d(self.pre.res.vui(), &mut r, true);
                    self.pre.update(now);
                } else {
                    let gpu = crate::play::Gpu { renderer: &mut *renderer, device, queue };
                    self.pre.frame(now, game_frame, &mut r, &mut cues, &mut music, Some(gpu));
                }
            }
            ui.end();
            ui.keep();
            renderer.ui = Some(layer);
        } else {
            if let Some(ui) = renderer.ui.as_mut() {
                ui.again();
            }
            self.pre.update(now);
        }
        // CLIENT's loading screen's gears, turning by the clock
        if self.loading_screen() {
            if self.gears.is_none() && self.pre.res.container("UI.CSF").is_some() {
                self.gears = Some(crate::loading::Gears::load(&self.pre.res.lib).map(|g| {
                    let textures = render::library_textures(&self.pre.res.lib);
                    let models = g.parts.iter().map(|(_, m)| renderer.upload(device, queue, m, &[render::IDENTITY], &textures)).collect();
                    (g, models)
                }));
            }
            if let Some(Some((g, models))) = self.gears.as_mut() {
                for (m, gpu) in g.pose(now).iter().zip(models.iter()) {
                    renderer.set_instances(queue, gpu, &[m.to_cols()]);
                }
            }
        }
        // the sounds
        if let Some(s) = sound {
            s.take_support();
            for c in &cues {
                s.cue(c);
            }
            // the title's Options: the classes' volumes and the output, as play's apply them
            for up in self.pre.setup.take_sound() {
                match up {
                    crate::options::OptionsUp::Volume(cls, v) => s.dev.set_sound_class_volume(cls, v.0),
                    crate::options::OptionsUp::Output(stereo) => s.dev.set_output_mode(stereo as i32),
                    crate::options::OptionsUp::Preview(t) => s.preview = Some(t),
                    crate::options::OptionsUp::PreviewEnd => s.end_preview(),
                    crate::options::OptionsUp::Apply => {}
                }
            }
            for m in music {
                self.music(s, m);
            }
            s.frame_alone();
        }
        // the world entry: its files read, then play
        if self.world.is_none()
            && let Some(e) = self.pre.take_entered()
        {
            self.begin_world(e);
        }
        if let Some(done) = self.world_done() {
            return done;
        }
        if self.pre.quit {
            return Done::Quit;
        }
        Done::Stay
    }

    fn music(&mut self, s: &mut crate::sound::Sound, m: Music) {
        match m {
            Music::Xm { tag, id, repeat, volume } => {
                if let Some(old) = self.playing.remove(tag) {
                    s.stop_xm(old);
                }
                let found = self.pre.res.lib.lookup(id as i32);
                if let Some(r) = found
                    && let Some(p) = s.play_xm_in(self.pre.res.lib.file(r.file), id, repeat, 127, volume)
                {
                    self.playing.insert(tag, p);
                }
            }
            Music::Stop { tag } => {
                if let Some(old) = self.playing.remove(tag) {
                    s.stop_xm(old);
                }
            }
            Music::Bgm { slot, path } => {
                s.dev.play_bgm(slot as usize, &path);
            }
            Music::StopBgm { slot } => s.dev.stop_bgm(slot as usize),
        }
    }

    /// The pregame's music stopped (the world begins).
    pub fn stop_music(&mut self, s: &mut crate::sound::Sound) {
        for (_, p) in self.playing.drain() {
            s.stop_xm(p);
        }
        s.dev.stop_all_bgm();
    }

    /// The world entry: its files read for `Play` (`main.rs`'s
    /// `world_library`), natively on a thread.
    fn begin_world(&mut self, e: Box<Entered>) {
        let reads = match WORLD_LOADER.with(|c| c.borrow().as_ref().map(|f| f(&e.player))) {
            Some(l) => l,
            #[cfg(not(target_arch = "wasm32"))]
            None => {
                let player = e.player.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    let r = crate::world_library(&player).map_err(|e| e.to_string());
                    let _ = tx.send(r);
                });
                super::link::Later::Thread(rx)
            }
            #[cfg(target_arch = "wasm32")]
            None => super::link::Later::ready(Err("no way to read the world's files".into())),
        };
        self.world = Some(WorldLoad { entered: e, reads });
    }

    fn world_done(&mut self) -> Option<Done> {
        let r = self.world.as_mut()?.reads.poll()?;
        let w = self.world.take()?;
        match r {
            Ok((lib, src)) => Some(Done::Play(w.entered, lib, src)),
            Err(e) => {
                crate::say(&format!("the world could not be read: {e}"));
                #[cfg(target_arch = "wasm32")]
                crate::web::crash("The world could not be read", &e);
                Some(Done::Quit)
            }
        }
    }

    /// CLIENT's loading screen is up: its files being read, or the world's.
    pub fn loading_screen(&self) -> bool {
        self.world.is_some() || self.pre.client_loading()
    }

    /// The loading screen's gears, to draw through `loading::view`.
    pub fn gears(&self) -> Vec<&render::GpuModel> {
        match &self.gears {
            Some(Some((_, models))) if self.loading_screen() => models.iter().collect(),
            _ => Vec::new(),
        }
    }
}

/// `EQOA_KEYS`: `name@ms` presses (held 100 ms), `text abc@ms` types.
fn key_script() -> Vec<(i64, i32, bool)> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(s) = std::env::var("EQOA_KEYS") {
        return parse_keys(&s);
    }
    Vec::new()
}

/// The keyboard's codes by name, as `VIKeyboard::Read` gives them.
pub fn key_code(name: &str) -> Option<i32> {
    Some(match name {
        "enter" => 13,
        "esc" | "escape" => 27,
        "backspace" => 8,
        "tab" => 9,
        "space" => 32,
        "left" => 17,
        "right" => 18,
        "up" => 19,
        "down" => 20,
        "pageup" => 21,
        "pagedown" => 22,
        "end" => 23,
        "home" => 24,
        _ => return None,
    })
}

pub fn parse_keys(s: &str) -> Vec<(i64, i32, bool)> {
    let mut out = Vec::new();
    for part in s.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let Some((what, at)) = part.rsplit_once('@') else { continue };
        let Ok(at) = at.trim().parse::<i64>() else { continue };
        if let Some(text) = what.strip_prefix("text ") {
            for (i, c) in text.chars().enumerate() {
                let t = at + i as i64 * 80;
                out.push((t, c as i32, true));
                out.push((t + 40, c as i32, false));
            }
        } else if let Some(k) = key_code(what.trim()) {
            out.push((at, k, true));
            out.push((at + 100, k, false));
        }
    }
    out.sort_by_key(|e| e.0);
    out
}

/// `EQOA_SHOTS`: `ms=file.png`, comma-separated (the window takes them).
pub fn shot_script() -> Vec<(i64, String)> {
    #[cfg(not(target_arch = "wasm32"))]
    if let Ok(s) = std::env::var("EQOA_SHOTS") {
        return s.split(',').filter_map(|p| p.split_once('=')).filter_map(|(t, f)| Some((t.trim().parse().ok()?, f.trim().to_string()))).collect();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_script_presses_and_types_in_time() {
        let k = parse_keys("enter@3000; text ab@4000;down@3500;nonsense@1");
        assert_eq!(k, vec![(3000, 13, true), (3100, 13, false), (3500, 20, true), (3600, 20, false), (4000, 97, true), (4040, 97, false), (4080, 98, true), (4120, 98, false)]);
        // the control: an unknown key name is not a key
        assert_eq!(key_code("nonsense"), None);
    }
}
