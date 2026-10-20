//! The browser build. The page (`web/index.html`) hands over the player's
//! own disc image (a `File`: nothing is uploaded, and no game data is
//! served) and the login, and this runs the same client on its canvas, with
//! wgpu on WebGPU or, without it, WebGL2. A page can open neither a UDP
//! socket nor a TCP connection, so both go through a WebSocket relay
//! (`relay`); and its disc reads are asynchronous, so the files the game
//! reads synchronously are fetched before it starts, and a world's zones
//! as the walk comes near them (`disc`).
//!
//! Between the login and the world the page is the character select and
//! creation screen: the list goes to `window.eqoaCharacters`, and the page
//! answers through the exports `choose_character`, `create_character`,
//! `delete_character` and `random_name` (the game's `SelectCharacter`,
//! `CreateCharacter`, `DeleteCharacter` and `Send0012`).
//!
//! The start menu reads CLIENT's data and the HUD SUPPORT's, which the disc
//! holds only encrypted: the page's workers (`web/unpack.js`) run this same
//! module's `unpack_module` on them, the disc's own DNAS library in crate
//! `dnas`, while the session goes on (`unpack_modules`).
//!
//! The sound is the native client's (`sound`, SNDDRV in `snd`), its driver
//! mixed on this thread a little ahead of the page's audio clock and played
//! by an AudioWorklet (`window.eqoaAudio`, main.js; web/sound.js), the music
//! read off the disc whole before it starts (`bgm`).

mod bgm;
mod disc;
mod relay;

use crate::{play, sky, viewer};
use disc::WebDisc;
use esf::Library;
use net::session::{Character, Event, NewChar, Session};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

pub fn log(s: &str) {
    web_sys::console::log_1(&JsValue::from_str(s));
}

/// Calls `window.<name>(arg)` if the page defines it.
fn call_page(name: &str, arg: &JsValue) {
    let Some(w) = web_sys::window() else { return };
    if let Ok(f) = js_sys::Reflect::get(&w, &name.into())
        && let Some(f) = f.dyn_ref::<js_sys::Function>()
    {
        let _ = f.call1(&JsValue::NULL, arg);
    }
}

/// The page's sound output (`window.eqoaAudio.clock`, main.js): 48 kHz
/// frames it has played, or `None` while it is not running.
pub fn audio_clock() -> Option<f64> {
    let w = web_sys::window()?;
    let a = js_sys::Reflect::get(&w, &"eqoaAudio".into()).ok()?;
    let f = js_sys::Reflect::get(&a, &"clock".into()).ok()?.dyn_into::<js_sys::Function>().ok()?;
    f.call0(&a).ok()?.as_f64().filter(|&c| c >= 0.0)
}

/// Hands the output interleaved 48 kHz stereo frames
/// (`window.eqoaAudio.push`).
pub fn audio_push(pcm: &[i16]) {
    let Some(w) = web_sys::window() else { return };
    let Ok(a) = js_sys::Reflect::get(&w, &"eqoaAudio".into()) else { return };
    if let Ok(f) = js_sys::Reflect::get(&a, &"push".into())
        && let Some(f) = f.dyn_ref::<js_sys::Function>()
    {
        let _ = f.call1(&a, &js_sys::Int16Array::from(pcm));
    }
}

/// A line for the page's status (`window.eqoaStatus`) and the console.
pub fn status(s: &str) {
    log(s);
    call_page("eqoaStatus", &JsValue::from_str(s));
}

/// `main`, in a browser: the page calls the exports below.
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Once a second, the frame rate and the wasm heap on the status line,
/// after `what`.
fn rate(what: &'static str) {
    wasm_bindgen_futures::spawn_local(async move {
        let mut last = viewer::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
        loop {
            sleep(1000).await;
            let n = viewer::FRAMES.load(std::sync::atomic::Ordering::Relaxed);
            let mem = wasm_bindgen::memory().dyn_into::<js_sys::WebAssembly::Memory>().map_or(0.0, |m| m.buffer().dyn_into::<js_sys::ArrayBuffer>().map_or(0.0, |b| b.byte_length() as f64));
            let [lead, dry] = &crate::audio::PAGE_SOUND;
            let lead = lead.load(std::sync::atomic::Ordering::Relaxed);
            let sound = if lead > 0 { format!(" · sound +{lead} ms, dry {}", dry.load(std::sync::atomic::Ordering::Relaxed)) } else { String::new() };
            status(&format!("{what} · {} fps · {:.0} MB{sound}", n.wrapping_sub(last), mem / 1048576.0));
            last = n;
        }
    });
}

pub async fn sleep(ms: i32) {
    let p = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
}

fn canvas() -> Result<web_sys::HtmlCanvasElement, String> {
    let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    doc.get_element_by_id("eqoa").ok_or("no canvas #eqoa")?.dyn_into().map_err(|_| "#eqoa is not a canvas".into())
}

/// The device on the canvas, made before the event loop: WebGPU where the
/// browser has a working adapter, else WebGL2 (whose adapter is the canvas's
/// own context, so the surface comes first).
async fn gfx(canvas: web_sys::HtmlCanvasElement) -> Result<viewer::PendingGfx, String> {
    use winit::platform::web::WindowAttributesExtWebSys;
    let dpr = web_sys::window().map_or(1.0, |w| w.device_pixel_ratio());
    let (w, h) = (((canvas.client_width() as f64) * dpr).max(1.0) as u32, ((canvas.client_height() as f64) * dpr).max(1.0) as u32);
    canvas.set_width(w);
    canvas.set_height(h);
    // `?webgl` in the page's address: WebGL2 even where WebGPU works (a test,
    // and a way round a WebGPU that misbehaves).
    let webgl = web_sys::window().and_then(|w| w.location().search().ok()).is_some_and(|q| q.split(['?', '&']).any(|p| p == "webgl"));
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    let instance = if webgl {
        desc.backends = wgpu::Backends::GL;
        wgpu::Instance::new(desc)
    } else {
        wgpu::util::new_instance_with_webgpu_detection(desc).await
    };
    let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone())).map_err(|e| format!("surface: {e}"))?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions { compatible_surface: Some(&surface), ..Default::default() })
        .await
        .map_err(|e| format!("no graphics adapter: {e}"))?;
    let info = adapter.get_info();
    status(&format!("graphics: {} ({:?})", info.name, info.backend));
    let limits = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor { required_limits: limits, ..Default::default() })
        .await
        .map_err(|e| format!("device: {e}"))?;
    let mut config = surface.get_default_config(&adapter, w, h).ok_or("the surface cannot be configured")?;
    // The bytes go out as the GS's do: no sRGB encode (render::TARGET_FORMAT).
    config.format = config.format.remove_srgb_suffix();
    surface.configure(&device, &config);
    let attrs = winit::window::Window::default_attributes().with_canvas(Some(canvas)).with_title("eqoa");
    Ok(viewer::PendingGfx { attrs, surface, config, device, queue })
}

/// Files every scene loads after its own (main.rs's `SHARED`).
const SHARED: &[&str] = &["CHARCUST.CSF", "CHARFACE.CSF", sky::FILE];

async fn add(lib: &mut Library, disc: &mut WebDisc, name: &str) -> Result<usize, String> {
    status(&format!("reading {name}"));
    let c = disc.container(name).await?;
    Ok(lib.add(name, c))
}

fn js(e: String) -> JsValue {
    status(&format!("error: {e}"));
    JsValue::from_str(&e)
}

/// `eqoa view` in the page: `args` is `<file> [model]`, `<world> zone <n>` or
/// `<world> walk <n>`.
#[wasm_bindgen]
pub async fn view(file: web_sys::File, args: String) -> Result<(), JsValue> {
    let a: Vec<String> = args.split_whitespace().map(String::from).collect();
    let r: Result<(), String> = async {
        let mut disc = WebDisc::open(file).await?;
        crate::stream::set_source_factory(disc.zone_sources());
        let name = a.first().ok_or("which file?")?;
        let mut lib = Library::new();
        let main = add(&mut lib, &mut disc, name).await?;
        for s in SHARED {
            if !s.eq_ignore_ascii_case(name) {
                add(&mut lib, &mut disc, s).await?;
            }
        }
        // A zone of a deferred world, read now (its neighbours come as the walk does).
        async fn zone(disc: &WebDisc, lib: &mut Library, main: usize, index: usize) -> Result<(), String> {
            if lib.zone(main, index).is_some() {
                return Ok(());
            }
            let (off, len) = lib.zone_range(main, index).ok_or("no such zone")?;
            let e = disc.find(&lib.files[main].0).ok_or("the world left the disc")?.clone();
            let b = disc.read_at(e.lba as u64 * iso::SECTOR + off, len).await?;
            crate::stream::install(lib, main, index, b)
        }
        let src = match &a[1..] {
            [z, n] if z == "walk" => {
                add(&mut lib, &mut disc, "CHAR.ESF").await?;
                let index: usize = n.parse().map_err(|_| "walk <zone number>")?;
                zone(&disc, &mut lib, main, index).await?;
                let character = lib.lookup(viewer::HUMAN).ok_or("no human model")?;
                viewer::Source::Walk { file: main, index, character, spawn: None }
            }
            [z, n] if z == "zone" => {
                let index: usize = n.parse().map_err(|_| "zone <zone number>")?;
                zone(&disc, &mut lib, main, index).await?;
                let (file, zone) = lib.zone(main, index).ok_or("no such zone")?;
                viewer::Source::Zone { file, zone, index }
            }
            [] | [_] => {
                let list = crate::scene::list_models(&lib, main);
                let index = match a.get(1) {
                    None => 0,
                    Some(m) => match m.strip_prefix("0x") {
                        Some(hex) => {
                            let id = u32::from_str_radix(hex, 16).map_err(|_| "bad DictID")? as i32;
                            list.iter().position(|(d, _)| *d == id).ok_or("no model with that DictID")?
                        }
                        None => m.parse::<usize>().ok().filter(|&i| i < list.len()).ok_or("no such model")?,
                    },
                };
                if list.is_empty() {
                    return Err(format!("{name} has no models"));
                }
                viewer::Source::Models { list, index }
            }
            _ => return Err("expected [model], zone <n> or walk <n>".into()),
        };
        let pending = gfx(canvas()?).await?;
        status("running");
        rate("viewing");
        viewer::run_with_gfx(lib, src, None, Some(pending)).map_err(|e| e.to_string())
    }
    .await;
    r.map_err(js)
}

/// One of the disc's DNAS modules unpacked by the disc's own library
/// (`dnas::unpack`): `UTIL.REL` and the `.AUT` in, the un-relocated SNR1
/// module out. For the page's worker (`web/unpack.js`); ~300M interpreted
/// instructions, so never on the page's own thread.
#[wasm_bindgen]
pub fn unpack_module(util: &[u8], aut: &[u8]) -> Result<Vec<u8>, JsValue> {
    dnas::unpack(util, aut).map_err(|e| JsValue::from_str(&e.to_string()))
}

/// CLIENT and SUPPORT (the start menu, the HUD): `UTIL.REL` and the two
/// `.AUT`s read off the disc here, each unpacked by the page
/// (`window.eqoaUnpack`: a worker of its own, or the page's copy from a
/// visit before) while the session goes on, and kept (`modules::put`) for
/// play to take up when it comes.
async fn unpack_modules(disc: &WebDisc) {
    use crate::modules::{self, Module};
    let util = match disc.read("UTIL.REL").await {
        Ok(u) => u,
        Err(e) => return log(&format!("no start menu and no HUD: {e}")),
    };
    for m in Module::ALL {
        if modules::get(m).is_some() {
            continue;
        }
        let aut = match disc.read(&format!("{}.AUT", m.name())).await {
            Ok(a) => a,
            Err(e) => {
                log(&format!("no {}: {e}", m.gives()));
                continue;
            }
        };
        let util = util.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let t = web_time::Instant::now();
            let r = async {
                let w = web_sys::window().ok_or("no window")?;
                let f = js_sys::Reflect::get(&w, &"eqoaUnpack".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()).ok_or("the page has no eqoaUnpack")?;
                let p = f.call3(&JsValue::NULL, &m.name().into(), &js_sys::Uint8Array::from(&util[..]), &js_sys::Uint8Array::from(&aut[..])).map_err(err)?;
                let v = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&p)).await.map_err(err)?;
                let image = js_sys::Uint8Array::new(&v).to_vec();
                modules::check(m, &image).map_err(String::from)?;
                Ok::<_, String>(image)
            }
            .await;
            match r {
                Ok(image) => {
                    log(&format!("{} in {:.1} s (unpacked off the disc, or the page's copy from before): {} is in", m.name(), t.elapsed().as_secs_f64(), m.gives()));
                    modules::put(m, image);
                }
                Err(e) => log(&format!("no {}: {} {e}", m.gives(), m.name())),
            }
        });
    }
}

/// A thrown value as text.
fn err(e: JsValue) -> String {
    e.dyn_ref::<js_sys::Error>().map(|e| String::from(e.message())).or_else(|| e.as_string()).unwrap_or_else(|| format!("{e:?}"))
}

/// What `opts` names (`relay`, `server`, `authPort`, `gamePort`, `character`).
fn opt(opts: &JsValue, k: &str) -> Option<String> {
    js_sys::Reflect::get(opts, &k.into()).ok().and_then(|v| v.as_string()).filter(|v| !v.is_empty())
}

/// What the character screen asks of the session.
enum Cmd {
    Choose(i64),
    Create(NewChar),
    Delete(i64),
    RandomName(i32, i32),
}

thread_local! {
    static CMDS: RefCell<VecDeque<Cmd>> = const { RefCell::new(VecDeque::new()) };
}

fn push(c: Cmd) {
    CMDS.with(|q| q.borrow_mut().push_back(c));
}

/// Play the character with this server id (`SelectCharacter`).
#[wasm_bindgen]
pub fn choose_character(server_id: f64) {
    push(Cmd::Choose(server_id as i64));
}

/// Remove the character with this server id (`DeleteCharacter`, 0x002D).
#[wasm_bindgen]
pub fn delete_character(server_id: f64) {
    push(Cmd::Delete(server_id as i64));
}

/// Ask the server for a name for this race and sex (`Send0012`); it comes
/// back to `window.eqoaRandomName`.
#[wasm_bindgen]
pub fn random_name(race: i32, sex: i32) {
    push(Cmd::RandomName(race, sex));
}

/// Make a character (`CreateCharacter`, 0x002B) from the form: `name`,
/// `race`, `city`, `sex`, `class`, `face`, `hairStyle`, `hairLength`,
/// `hairColor` and `stats` (the seven points spent, Strength first).
#[wasm_bindgen]
pub fn create_character(form: JsValue) -> Result<(), JsValue> {
    let num = |k: &str| js_sys::Reflect::get(&form, &k.into()).ok().and_then(|v| v.as_f64()).map(|v| v as i32);
    let name = js_sys::Reflect::get(&form, &"name".into()).ok().and_then(|v| v.as_string()).unwrap_or_default();
    let mut stats = [0i32; 7];
    if let Ok(a) = js_sys::Reflect::get(&form, &"stats".into())
        && let Ok(a) = a.dyn_into::<js_sys::Array>()
    {
        for (i, s) in stats.iter_mut().enumerate() {
            *s = a.get(i as u32).as_f64().unwrap_or(0.0) as i32;
        }
    }
    let need = |k: &str| num(k).ok_or_else(|| JsValue::from_str(&format!("the form has no {k}")));
    push(Cmd::Create(NewChar {
        name,
        race: need("race")?,
        class: need("class")?,
        sex: need("sex")?,
        hair_color: need("hairColor")?,
        hair_length: need("hairLength")?,
        hair_style: need("hairStyle")?,
        face: need("face")?,
        city: num("city").unwrap_or(0),
        stats,
    }));
    Ok(())
}

/// The list as the page reads it.
fn characters_js(list: &[Character]) -> JsValue {
    let a = js_sys::Array::new();
    for c in list {
        let o = js_sys::Object::new();
        let set = |k: &str, v: JsValue| {
            let _ = js_sys::Reflect::set(&o, &k.into(), &v);
        };
        set("name", c.name.as_str().into());
        set("serverId", (c.server_id as f64).into());
        set("race", (c.race as f64).into());
        set("class", (c.class as f64).into());
        set("level", (c.level as f64).into());
        a.push(&o);
    }
    a.into()
}

/// The session from its hello to the world: the first shard, then the
/// character screen, which the page drives (a character named in the
/// options is played at once). Pumped every 16 ms.
async fn enter_world(mut s: Session, mut want: Option<String>, words: Option<&vi::strtab::StringTable>) -> Result<(Session, net::session::Player, Box<net::dump::MemoryDump>), String> {
    let start = web_time::Instant::now();
    let mut chars: Vec<Character> = Vec::new();
    let mut listed = false;
    loop {
        if !listed && start.elapsed().as_secs() > 30 {
            return Err("no character list within 30 s".into());
        }
        let events = s.pump();
        for (k, e) in events.iter().enumerate() {
            match e {
                Event::ShardList(list) => {
                    log(&format!("shards: {list:?}"));
                    if let Some(sh) = list.first() {
                        status(&format!("joining {}", sh.name));
                        s.connect_shard(sh.endpoint);
                    }
                }
                Event::Characters(list) => {
                    chars = list.clone();
                    listed = true;
                    if let Some(c) = want.take().and_then(|w| list.iter().find(|c| c.name.eq_ignore_ascii_case(&w)).cloned()) {
                        status(&format!("entering the world as {}", c.name));
                        s.select_character(&c);
                    } else {
                        status(&format!("{} character{}", list.len(), if list.len() == 1 { "" } else { "s" }));
                        call_page("eqoaCharacters", &characters_js(list));
                    }
                }
                Event::RandomName(n) => call_page("eqoaRandomName", &JsValue::from_str(n)),
                Event::CreateRefused(word) => {
                    let text = words.map(|w| w.get(*word, 0).to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| "Character create failed".into());
                    call_page("eqoaRefused", &JsValue::from_str(&text));
                }
                Event::WorldEntry { player, dump, .. } => {
                    let p = player.clone();
                    let d = dump.clone();
                    s.held = events[k + 1..].to_vec();
                    return Ok((s, p, d));
                }
                Event::Disconnected(r) => return Err(format!("disconnected (reason {r})")),
                _ => {}
            }
        }
        while let Some(c) = CMDS.with(|q| q.borrow_mut().pop_front()) {
            match c {
                Cmd::Choose(id) => match chars.iter().find(|c| c.server_id == id) {
                    Some(c) => {
                        status(&format!("entering the world as {}", c.name));
                        call_page("eqoaEntering", &JsValue::from_str(&c.name));
                        s.select_character(&c.clone());
                    }
                    None => call_page("eqoaRefused", &JsValue::from_str("that character is not on the list")),
                },
                Cmd::Create(nc) => {
                    status(&format!("creating {}", nc.name));
                    s.create_character(&nc);
                }
                Cmd::Delete(id) => {
                    status("deleting a character");
                    s.delete_character(id);
                }
                Cmd::RandomName(race, sex) => s.random_name(race, sex),
            }
        }
        sleep(16).await;
    }
}

/// Keeps a session answered while the page reads files: every 50 ms a
/// pump, its events kept for the game.
fn keep_pumping(s: Rc<RefCell<Option<Session>>>, alive: Rc<RefCell<bool>>) {
    wasm_bindgen_futures::spawn_local(async move {
        while *alive.borrow() {
            if let Some(s) = s.borrow_mut().as_mut() {
                let ev = s.pump();
                s.held.extend(ev);
            }
            sleep(50).await;
        }
    });
}

/// `eqoa play` in the page: log in through the relay, the character screen,
/// the world, and play on the canvas.
#[wasm_bindgen]
pub async fn play(file: web_sys::File, user: String, password: String, opts: JsValue) -> Result<(), JsValue> {
    CMDS.with(|q| q.borrow_mut().clear());
    let r: Result<(), String> = async {
        let relay = opt(&opts, "relay").ok_or("no relay address")?;
        let server_name = opt(&opts, "server").ok_or("no server address")?;
        let server = relay::parse_ip(&server_name).ok_or_else(|| format!("{server_name}: not an IPv4 address"))?;
        let auth_port = opt(&opts, "authPort").and_then(|p| p.parse().ok()).unwrap_or(9735);
        let game_port = opt(&opts, "gamePort").and_then(|p| p.parse().ok()).unwrap_or(10070);
        status("reading the disc");
        let mut disc = WebDisc::open(file).await?;
        crate::stream::set_source_factory(disc.zone_sources());
        crate::stream::set_world_opener(disc.world_opener());
        unpack_modules(&disc).await;
        // What every world needs, before the session starts its clock.
        let mut common = Vec::new();
        for name in ["CHAR.ESF", "ITEM.ESF", "ARIAL.CSF", "UI.CSF", "ARIALUI.CSF", "ITEMICON.ESF", "SPELLFX.CSF", "FX.CSF", "ADDART.CSF"] {
            status(&format!("reading {name}"));
            common.push((name, disc.container(name).await?));
        }
        let mut shared = Vec::new();
        for name in SHARED {
            status(&format!("reading {name}"));
            shared.push((*name, disc.container(name).await?));
        }
        // The sound's own files: the ambient tracks, the resident waves, the
        // UI sounds' names (`Sound::with`).
        let mut sound_files = std::collections::HashMap::new();
        for path in ["DATA/AMBTRACK.ESF", "DATA2/UI.CSF", "DATA2/FX.CSF", "DATA2/SKY.CSF", "DATA/UISOUND.TXT"] {
            if let Ok(b) = disc.read(path).await {
                sound_files.insert(path.to_string(), b);
            }
        }
        let words = disc.read("WORDS.TXT").await.ok().and_then(|b| vi::strtab::StringTable::parse(&b));
        // The HUD's texture names (its atlas is SUPPORT's, `unpack_modules`).
        let textures = disc.read("TEXTURES.TXT").await.ok().and_then(|b| vi::strtab::StringTable::parse(&b));
        status(&format!("logging in as {user}"));
        let l = relay::login(&relay, server, auth_port, &user, &password).await?;
        let (sock, local) = relay::RelayUdp::open(&relay).await?;
        let s = Session::connect_with(Box::new(sock), local, (server, game_port), &user, l.token).ok_or("could not open the game connection")?;
        let (s, player, dump) = enter_world(s, opt(&opts, "character"), words.as_ref()).await?;
        // The world the dump names, by the path's last part, opened with its
        // zones on the disc and the ones round the player read, while the
        // session is kept answered.
        let session = Rc::new(RefCell::new(Some(s)));
        let alive = Rc::new(RefCell::new(true));
        keep_pumping(session.clone(), alive.clone());
        let world = player.world_path.rsplit(['\\', '/']).next().unwrap_or("tunaria.esf").to_uppercase();
        let mut lib = Library::new();
        let [x, _, z] = player.pos;
        let loaded = async {
            let main = add(&mut lib, &mut disc, &world).await?;
            status("reading the zones round you");
            let n = disc.fill(&mut lib, main, x, z).await?;
            log(&format!("{n} zones read round the spawn"));
            Ok::<usize, String>(main)
        }
        .await;
        *alive.borrow_mut() = false;
        let main = loaded?;
        for (name, c) in shared {
            lib.add(name, c);
        }
        for (name, c) in common {
            lib.add(name, c);
        }
        let s = session.borrow_mut().take().ok_or("lost the session")?;
        let index = vi::world::World::<vi::fpu::Ee>::zone_at(lib.file(main), x, z).ok_or("no zone at the player's position")?;
        let character = lib.lookup(viewer::HUMAN).ok_or("no human model")?;
        let src = viewer::Source::Walk { file: main, index, character, spawn: Some((x, z, player.hpr[0])) };
        let mut play = play::Play::new(s, player, dump);
        play.words = words;
        play.ui_textures = textures;
        // The sound, left for the viewer (`sound::make`): the driver on this
        // thread for the page's AudioWorklet, the music off the disc.
        let bgm = crate::bgm::Bgm::new(&disc.entries);
        let fetch = {
            let (bgm, file) = (bgm.clone(), disc.file());
            Box::new(move || bgm::fetch(&bgm, &file))
        };
        match crate::sound::Sound::with(&mut |p| sound_files.remove(p), crate::audio::Audio::web(Box::new(bgm), fetch)) {
            Some(s) => crate::sound::give(s),
            None => log("no sound: VISoundDevice::Init failed"),
        }
        let pending = gfx(canvas()?).await?;
        status("in the world");
        call_page("eqoaInWorld", &JsValue::NULL);
        rate("in the world");
        viewer::run_with_gfx(lib, src, Some(play), Some(pending)).map_err(|e| e.to_string())
    }
    .await;
    r.map_err(js)
}
