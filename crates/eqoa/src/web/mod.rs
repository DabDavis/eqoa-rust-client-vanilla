//! The browser build. The page (`web/index.html`) hands over the player's
//! own disc image (a `File`: nothing is uploaded, and no game data is
//! served), and this runs the same client on its canvas from the game's boot
//! (`boot`, the `pregame`: the loading screens, the licence and the title,
//! Station's login screen, the server list, character select and creation,
//! the world), with wgpu on WebGPU or, without it, WebGL2. A page can open
//! neither a UDP socket nor a TCP connection, so both go through a WebSocket
//! relay (`relay`), behind the pregame's `link`; and its disc reads are
//! asynchronous, so the pregame's files, the world entry's and a world's zones
//! are read as they are asked for and finish on later frames (`disc`).
//!
//! The start menu reads CLIENT's data and the HUD SUPPORT's, which the disc
//! holds only encrypted: the page's workers (`web/unpack.js`) run this same
//! module's `unpack_module` on them, the disc's own DNAS library in crate
//! `dnas`, while the boot's pictures show (`unpack_modules`).
//!
//! The sound is the native client's (`sound`, SNDDRV in `snd`), its driver
//! mixed on this thread a little ahead of the page's audio clock and played
//! by an AudioWorklet (`window.eqoaAudio`, main.js; web/sound.js), the music
//! read off the disc whole before it starts (`bgm`).
//!
//! What goes wrong is the page's to show (web/notices.js): a panic or a
//! lost graphics device stops the client, and the page says so (`crash`);
//! what only takes a part away (a module that would not unpack, a disc
//! other than Frontiers, no sound) is a notice (`problem`).

mod bgm;
mod disc;
pub mod pad;
mod relay;

use crate::{sky, viewer};
use disc::WebDisc;
use esf::Library;
use std::cell::RefCell;
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

/// The motors of the gamepads at `pads` (the Gamepad API's indices: those
/// this window reads), `window.eqoaRumble(strong, weak, ms, pads)`, main.js:
/// the Gamepad API's dual-rumble.
pub fn rumble(strong: f64, weak: f64, ms: u32, pads: &[u32]) {
    let Some(w) = web_sys::window() else { return };
    if let Ok(f) = js_sys::Reflect::get(&w, &"eqoaRumble".into())
        && let Some(f) = f.dyn_ref::<js_sys::Function>()
    {
        let list: js_sys::Array = pads.iter().map(|&i| JsValue::from(i)).collect();
        let _ = f.call4(&JsValue::NULL, &strong.into(), &weak.into(), &(ms as f64).into(), &JsValue::from(list));
    }
}

/// main.js's `window.eqoaWindows`: this page's place among the site's pages
/// open in this browser (a Web Lock) and the places held (`multibox`).
fn windows_js() -> Option<JsValue> {
    let w = web_sys::window()?;
    js_sys::Reflect::get(&w, &"eqoaWindows".into()).ok().filter(|v| v.is_object())
}

/// This page's place, from 0: unknown while main.js takes it, then the
/// place, or none for good (a browser without Web Locks, every place taken).
pub fn window_place() -> Option<Option<usize>> {
    let w = windows_js()?;
    if !js_sys::Reflect::get(&w, &"ready".into()).ok()?.is_truthy() {
        return None;
    }
    let place = js_sys::Reflect::get(&w, &"place".into()).ok().and_then(|v| v.as_f64()).unwrap_or(-1.0);
    Some((place >= 0.0).then_some(place as usize))
}

/// The places the site's pages hold now, as main.js last looked (twice a
/// second).
pub fn window_places() -> Vec<usize> {
    let Some(live) = windows_js().and_then(|w| js_sys::Reflect::get(&w, &"live".into()).ok()) else { return Vec::new() };
    js_sys::Array::from(&live).iter().filter_map(|v| v.as_f64()).filter(|v| *v >= 0.0).map(|v| v as usize).collect()
}

/// Whether the page's document has the focus: its window is the one in
/// front (Controller Focus's Window).
pub fn has_focus() -> bool {
    web_sys::window().and_then(|w| w.document()).and_then(|d| d.has_focus().ok()).unwrap_or(true)
}

/// Whether the page is on a touch-first device (`window.eqoaTouchFirst`:
/// its main pointer is coarse), so the touch screen's pad shows at once.
pub fn touch_first() -> bool {
    let Some(w) = web_sys::window() else { return false };
    js_sys::Reflect::get(&w, &"eqoaTouchFirst".into())
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call0(&JsValue::NULL).ok())
        .is_some_and(|v| v.is_truthy())
}

/// Whether the page holds the pointer (`document.pointerLockElement`): the
/// mouse look's lock (`mouse::Grab`), which the browser ends itself on
/// Escape.
pub fn pointer_locked() -> bool {
    web_sys::window().and_then(|w| w.document()).and_then(|d| d.pointer_lock_element()).is_some()
}

/// Whether a connected gamepad can rumble (`window.eqoaCanRumble`).
pub fn can_rumble() -> bool {
    let Some(w) = web_sys::window() else { return false };
    js_sys::Reflect::get(&w, &"eqoaCanRumble".into())
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
        .and_then(|f| f.call0(&JsValue::NULL).ok())
        .is_some_and(|v| v.is_truthy())
}

/// `main`, in a browser: the page calls the exports below. A panic goes to
/// the console and to the page's notices: the client stops there, and a
/// player without the console open would see only a frame that no longer
/// moves.
pub fn start() {
    std::panic::set_hook(Box::new(|info| {
        console_error_panic_hook::hook(info);
        crash("The client stopped", &info.to_string());
    }));
}

/// The client has stopped (`title`; `detail` says why): the page's notice
/// (`window.eqoaCrash`, web/notices.js), which asks for a reload.
pub(crate) fn crash(title: &str, detail: &str) {
    let a = js_sys::Array::of2(&title.into(), &detail.into());
    call_page("eqoaCrash", &a);
}

/// Something the player should know went wrong, short of a crash: a notice
/// on the page (`window.eqoaProblem`). `key` names it, so the same problem
/// again replaces its notice.
pub fn problem(key: &str, text: &str) {
    log(text);
    call_page("eqoaProblem", &js_sys::Array::of2(&key.into(), &text.into()));
}

/// The server ended the connection in the world (`Play::disconnected`): the
/// page goes back to its login screen with CLIENT's text for the reason
/// (`window.eqoaDisconnected`), as the console's VIWndDisconnect shows it.
pub fn disconnected(text: &str) {
    log(text);
    call_page("eqoaDisconnected", &JsValue::from_str(text));
}

/// Stops the client with a panic, as a bug would: how the page's crash
/// notice is checked (`eqoaClient.crashTest()` in the page).
#[wasm_bindgen]
pub fn crash_test() {
    panic!("crash_test: the client stopped on purpose");
}

/// What the disc image `file` is (`discs::describe`): `[ok, line]`, ok when
/// it is the Frontiers disc the client is made for. The page's login card
/// shows the line when the image is picked.
#[wasm_bindgen]
pub async fn disc_id(file: web_sys::File) -> Result<JsValue, JsValue> {
    let disc = WebDisc::open(file).await.map_err(|e| JsValue::from_str(&e))?;
    let (ok, line) = crate::discs::describe(disc.boot().await.as_deref());
    Ok(js_sys::Array::of2(&ok.into(), &line.into()).into())
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
            // the status line only: once a second in the console would bury
            // everything else there
            let line = format!("{what} · {} fps · {:.0} MB{sound}", n.wrapping_sub(last), mem / 1048576.0);
            call_page("eqoaStatus", &JsValue::from_str(&line));
            last = n;
        }
    });
}

/// Whether this animation frame is drawn under the options' Frame Rate
/// (`settings::frame_due`): a page on WebGL2 keeps 30 frames a second unless
/// the player chose otherwise (WebGL2 costs about twice the CPU of WebGPU;
/// 2026-10-20, a tester's ask), `?uncapped` in the page's address lifting
/// that default; WebGPU draws every animation frame unless capped.
pub fn frame_due() -> bool {
    crate::settings::frame_due()
}

/// Whether the page's address has `?name` (or `&name`) in its query.
pub fn page_flag(name: &str) -> bool {
    web_sys::window().and_then(|w| w.location().search().ok()).is_some_and(|q| q.split(['?', '&']).any(|p| p == name))
}

/// The page's `?name=value`, if it has one.
pub fn page_value(name: &str) -> Option<String> {
    let q = web_sys::window()?.location().search().ok()?;
    q.split(['?', '&']).find_map(|p| p.strip_prefix(name)?.strip_prefix('=').map(str::to_string))
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
    let webgl = page_flag("webgl");
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
    if info.backend == wgpu::Backend::Gl && !page_flag("uncapped") {
        crate::settings::set_platform_rate(crate::settings::FrameRate::Thirty);
    }
    // the rate the page keeps (the options' Frame Rate, else the platform's)
    match crate::settings::frame_rate() {
        crate::settings::FrameRate::Thirty => status("drawn at 30 frames a second (Options, Extended, Frame Rate lifts it; ?uncapped does on WebGL2)"),
        crate::settings::FrameRate::Sixty => status("drawn at 60 frames a second (Options, Extended, Frame Rate)"),
        crate::settings::FrameRate::Display => {}
    }
    let limits = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor { required_limits: limits, ..Default::default() })
        .await
        .map_err(|e| format!("device: {e}"))?;
    // A device lost (a driver reset, the GPU process gone) draws nothing more:
    // the client has stopped. `Destroyed` is the page closing.
    device.set_device_lost_callback(|reason, message| {
        if !matches!(reason, wgpu::DeviceLostReason::Destroyed) {
            crash("The graphics stopped", &format!("the graphics device was lost ({reason:?}): {message}"));
        }
    });
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
/// play to take up when it comes. An image that fails `modules::check` is
/// unpacked once more past the page's copy, in case that copy is what is
/// wrong. On the Frontiers disc a module that still does not come is a
/// problem for the page to show; on another disc it is expected, and the
/// disc's own notice says so.
async fn unpack_modules(disc: &WebDisc, frontiers: bool) {
    use crate::modules::{self, Module};
    let missing = move |m: Module, why: &str| {
        let gives = m.gives();
        let text = format!("{}{} is missing ({}: {why}).", gives[..1].to_uppercase(), &gives[1..], m.name());
        if frontiers { problem(&format!("module-{}", m.name()), &text) } else { log(&text) }
    };
    let util = match disc.read("UTIL.REL").await {
        Ok(u) => u,
        Err(e) => {
            for m in Module::ALL {
                missing(m, &format!("UTIL.REL: {e}"));
            }
            return;
        }
    };
    for m in Module::ALL {
        if modules::get(m).is_some() {
            continue;
        }
        let aut = match disc.read(&format!("{}.AUT", m.name())).await {
            Ok(a) => a,
            Err(e) => {
                missing(m, &e);
                continue;
            }
        };
        let util = util.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let t = web_time::Instant::now();
            let unpack = |fresh: bool| {
                let (util, aut) = (util.clone(), aut.clone());
                async move {
                    let w = web_sys::window().ok_or("no window")?;
                    let f = js_sys::Reflect::get(&w, &"eqoaUnpack".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()).ok_or("the page has no eqoaUnpack")?;
                    let args = js_sys::Array::of4(&m.name().into(), &js_sys::Uint8Array::from(&util[..]), &js_sys::Uint8Array::from(&aut[..]), &fresh.into());
                    let p = f.apply(&JsValue::NULL, &args).map_err(err)?;
                    let v = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&p)).await.map_err(err)?;
                    let image = js_sys::Uint8Array::new(&v).to_vec();
                    modules::check(m, &image).map_err(String::from)?;
                    Ok::<_, String>(image)
                }
            };
            let r = match unpack(false).await {
                Err(e) if frontiers => {
                    log(&format!("{}: {e}; unpacking it again, past the page's copy", m.name()));
                    unpack(true).await
                }
                r => r,
            };
            match r {
                Ok(image) => {
                    log(&format!("{} in {:.1} s (unpacked off the disc, or the page's copy from before): {} is in", m.name(), t.elapsed().as_secs_f64(), m.gives()));
                    modules::put(m, image);
                }
                Err(e) => missing(m, &e),
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

/// `eqoa` in the page: the game from its boot on the canvas (`pregame`):
/// the loading screens, the licence and the title, Station's login, the
/// server list, character select and creation, then the world. The page has
/// only handed over the disc image and the relay; `opts.server` is the
/// server to play on when the title's Network Setup keeps none (the page's
/// own host, as the relay resolves it).
#[wasm_bindgen]
pub async fn boot(file: web_sys::File, opts: JsValue) -> Result<(), JsValue> {
    let r: Result<(), String> = async {
        let relay = opt(&opts, "relay").ok_or("no relay address")?;
        status("reading the disc");
        let disc = WebDisc::open(file).await?;
        crate::stream::set_source_factory(disc.zone_sources());
        crate::stream::set_world_opener(disc.world_opener());
        let (frontiers, which) = crate::discs::describe(disc.boot().await.as_deref());
        if !frontiers {
            problem("disc", &which);
        }
        // CLIENT and SUPPORT in their workers while the boot's pictures show
        {
            let d = disc.copy();
            wasm_bindgen_futures::spawn_local(async move { unpack_modules(&d, frontiers).await });
        }
        // the pregame's files, a request at a time off the image
        let d = disc.copy();
        crate::pregame::files::set_reader(Box::new(move |paths| {
            let slot: crate::pregame::files::Slot = Default::default();
            let (out, d, paths) = (slot.clone(), d.copy(), paths.to_vec());
            wasm_bindgen_futures::spawn_local(async move {
                let mut files = Vec::new();
                let mut r = Ok(());
                for p in &paths {
                    match d.read(p).await {
                        Ok(b) => files.push((p.clone(), b)),
                        Err(e) => {
                            r = Err(e);
                            break;
                        }
                    }
                }
                *out.borrow_mut() = Some(r.map(|_| files));
            });
            slot
        }));
        // Station's connection, the news and the game session, through the relay
        let (rt, ru) = (relay.clone(), relay.clone());
        crate::pregame::link::set_platform(crate::pregame::link::Platform {
            stream: Box::new(move |host, port| Box::new(relay::RelayTcp::open(&rt, host, port))),
            session: Box::new(move |host, port, user, token| relay::session(&ru, host, port, user, token)),
        });
        // the world entry's files, as `world_library` reads them natively
        crate::pregame::host::set_world_loader(world_loader(&disc));
        // the sound's own files, then the device (`Sound::with`)
        let mut sound_files = std::collections::HashMap::new();
        for path in ["DATA/AMBTRACK.ESF", "DATA2/UI.CSF", "DATA2/FX.CSF", "DATA2/SKY.CSF", "DATA/UISOUND.TXT"] {
            if let Ok(b) = disc.read(path).await {
                sound_files.insert(path.to_string(), b);
            }
        }
        let bgm = crate::bgm::Bgm::new(&disc.entries);
        let fetch = {
            let (bgm, file) = (bgm.clone(), disc.file());
            Box::new(move || bgm::fetch(&bgm, &file))
        };
        match crate::sound::Sound::with(&mut |p| sound_files.remove(p), crate::audio::Audio::web(Box::new(bgm), fetch)) {
            Some(s) => crate::sound::give(s),
            None => problem("sound", "No sound: the sound device did not start (VISoundDevice::Init failed)."),
        }
        let pending = gfx(canvas()?).await?;
        status("");
        call_page("eqoaBooted", &JsValue::NULL);
        rate("booted");
        let server = crate::settings::server().or_else(|| opt(&opts, "server"));
        viewer::run_boot_gfx(server, Some(pending)).map_err(|e| e.to_string())
    }
    .await;
    r.map_err(js)
}

/// The world entry's files in a page (`pregame::host::set_world_loader`):
/// the world file with the zones round the player read, the character art
/// every scene keeps, then CHAR.ESF and the rest, in `world_library`'s
/// order, on a reader of its own.
fn world_loader(disc: &WebDisc) -> crate::pregame::host::WorldLoader {
    let d0 = disc.copy();
    Box::new(move |player| {
        let slot: Rc<RefCell<Option<Result<crate::pregame::host::World, String>>>> = Default::default();
        let (out, mut d, player) = (slot.clone(), d0.copy(), player.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let r = async {
                let world = player.world_path.rsplit(['\\', '/']).next().unwrap_or("tunaria.esf").to_uppercase();
                let mut lib = Library::new();
                let main = add(&mut lib, &mut d, &world).await?;
                let [x, _, z] = player.pos;
                let n = d.fill(&mut lib, main, x, z).await?;
                log(&format!("{n} zones read round the spawn"));
                for name in SHARED.iter().copied().chain(["CHAR.ESF", "ITEM.ESF", "ARIAL.CSF", "UI.CSF", "ARIALUI.CSF", "ITEMICON.ESF", "SPELLFX.CSF", "FX.CSF", "ADDART.CSF"]) {
                    add(&mut lib, &mut d, name).await?;
                }
                let index = vi::world::World::<vi::fpu::Ee>::zone_at(lib.file(main), x, z).ok_or("no zone at the player's position")?;
                let character = lib.lookup(viewer::HUMAN).ok_or("no human model")?;
                Ok::<_, String>((lib, viewer::Source::Walk { file: main, index, character, spawn: Some((x, z, player.hpr[0])) }))
            }
            .await;
            *out.borrow_mut() = Some(r);
        });
        crate::pregame::link::Later::Slot(slot)
    })
}
