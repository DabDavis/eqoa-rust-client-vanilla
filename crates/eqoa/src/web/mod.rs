//! The browser build. The page (`web/index.html`) hands over the player's
//! own disc image (a `File`: nothing is uploaded, and no game data is
//! served) and the login, and this runs the same client on its canvas, with
//! wgpu on WebGPU or, without it, WebGL2. A page can open neither a UDP
//! socket nor a TCP connection, so both go through a WebSocket relay
//! (`relay`); and its disc reads are asynchronous, so the files the game
//! reads synchronously are fetched before it starts (`disc`).

mod disc;
mod relay;

use crate::{play, sky, viewer};
use disc::WebDisc;
use esf::Library;
use net::session::{Event, Session};
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

pub fn log(s: &str) {
    web_sys::console::log_1(&JsValue::from_str(s));
}

/// A line for the page's status (`window.eqoaStatus`) and the console.
pub fn status(s: &str) {
    log(s);
    let Some(w) = web_sys::window() else { return };
    if let Ok(f) = js_sys::Reflect::get(&w, &"eqoaStatus".into())
        && let Some(f) = f.dyn_ref::<js_sys::Function>()
    {
        let _ = f.call1(&JsValue::NULL, &JsValue::from_str(s));
    }
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
            status(&format!("{what} · {} fps · {:.0} MB", n.wrapping_sub(last), mem / 1048576.0));
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
    let instance = wgpu::util::new_instance_with_webgpu_detection(wgpu::InstanceDescriptor::new_without_display_handle()).await;
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

async fn add(lib: &mut Library, disc: &WebDisc, name: &str) -> Result<usize, String> {
    status(&format!("reading {name}"));
    let b = disc.read(name).await?;
    let c = esf::Container::open(b).map_err(|e| format!("{name}: {e}"))?;
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
        let disc = WebDisc::open(file).await?;
        let name = a.first().ok_or("which file?")?;
        let mut lib = Library::new();
        let main = add(&mut lib, &disc, name).await?;
        for s in SHARED {
            if !s.eq_ignore_ascii_case(name) {
                add(&mut lib, &disc, s).await?;
            }
        }
        let src = match &a[1..] {
            [z, n] if z == "walk" => {
                add(&mut lib, &disc, "CHAR.ESF").await?;
                let index: usize = n.parse().map_err(|_| "walk <zone number>")?;
                lib.file(main).zones().get(index).ok_or("no such zone")?;
                let character = lib.lookup(viewer::HUMAN).ok_or("no human model")?;
                viewer::Source::Walk { file: main, index, character, spawn: None }
            }
            [z, n] if z == "zone" => {
                let index: usize = n.parse().map_err(|_| "zone <zone number>")?;
                let zone = *lib.file(main).zones().get(index).ok_or("no such zone")?;
                viewer::Source::Zone { file: main, zone, index }
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

/// What `opts` names (`relay`, `server`, `authPort`, `gamePort`, `character`).
fn opt(opts: &JsValue, k: &str) -> Option<String> {
    js_sys::Reflect::get(opts, &k.into()).ok().and_then(|v| v.as_string()).filter(|v| !v.is_empty())
}

/// The session from its hello to the world: the first shard, the character
/// named (else the first), pumped every 16 ms.
async fn enter_world(mut s: Session, want: Option<String>) -> Result<(Session, net::session::Player, Box<net::dump::MemoryDump>), String> {
    let start = web_time::Instant::now();
    while start.elapsed().as_secs() < 30 {
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
                    let pick = list.iter().find(|c| want.as_deref() == Some(c.name.as_str())).or(list.first()).cloned();
                    match pick {
                        Some(c) => {
                            status(&format!("entering the world as {}", c.name));
                            s.select_character(&c);
                        }
                        None => return Err("this account has no characters yet (make one with another client)".into()),
                    }
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
        sleep(16).await;
    }
    Err("no world entry within 30 s".into())
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

/// `eqoa play` in the page: log in through the relay, enter the world, and
/// play on the canvas.
#[wasm_bindgen]
pub async fn play(file: web_sys::File, user: String, password: String, opts: JsValue) -> Result<(), JsValue> {
    let r: Result<(), String> = async {
        let relay = opt(&opts, "relay").ok_or("no relay address")?;
        let server_name = opt(&opts, "server").ok_or("no server address")?;
        let server = relay::parse_ip(&server_name).ok_or_else(|| format!("{server_name}: not an IPv4 address"))?;
        let auth_port = opt(&opts, "authPort").and_then(|p| p.parse().ok()).unwrap_or(9735);
        let game_port = opt(&opts, "gamePort").and_then(|p| p.parse().ok()).unwrap_or(10070);
        status("reading the disc");
        let disc = WebDisc::open(file).await?;
        // What every world needs, before the session starts its clock.
        let mut common = Vec::new();
        for name in ["CHAR.ESF", "ITEM.ESF", "ARIAL.CSF", "UI.CSF", "ARIALUI.CSF", "ITEMICON.ESF", "SPELLFX.CSF", "FX.CSF"] {
            status(&format!("reading {name}"));
            let c = esf::Container::open(disc.read(name).await?).map_err(|e| format!("{name}: {e}"))?;
            common.push((name, c));
        }
        let mut shared = Vec::new();
        for name in SHARED {
            status(&format!("reading {name}"));
            shared.push((*name, esf::Container::open(disc.read(name).await?).map_err(|e| format!("{name}: {e}"))?));
        }
        status(&format!("logging in as {user}"));
        let l = relay::login(&relay, server, auth_port, &user, &password).await?;
        let (sock, local) = relay::RelayUdp::open(&relay).await?;
        let s = Session::connect_with(Box::new(sock), local, (server, game_port), &user, l.token).ok_or("could not open the game connection")?;
        let (s, player, dump) = enter_world(s, opt(&opts, "character")).await?;
        // The world the dump names, by the path's last part, read while the
        // session is kept answered.
        let session = Rc::new(RefCell::new(Some(s)));
        let alive = Rc::new(RefCell::new(true));
        keep_pumping(session.clone(), alive.clone());
        let world = player.world_path.rsplit(['\\', '/']).next().unwrap_or("tunaria.esf").to_uppercase();
        let mut lib = Library::new();
        let loaded = add(&mut lib, &disc, &world).await;
        *alive.borrow_mut() = false;
        let main = loaded?;
        for (name, c) in shared {
            lib.add(name, c);
        }
        for (name, c) in common {
            lib.add(name, c);
        }
        let s = session.borrow_mut().take().ok_or("lost the session")?;
        let [x, _, z] = player.pos;
        let index = vi::world::World::<vi::fpu::Ee>::zone_at(lib.file(main), x, z).ok_or("no zone at the player's position")?;
        let character = lib.lookup(viewer::HUMAN).ok_or("no human model")?;
        let src = viewer::Source::Walk { file: main, index, character, spawn: Some((x, z, player.hpr[0])) };
        let mut play = play::Play::new(s, player, dump);
        play.words = disc.read("WORDS.TXT").await.ok().and_then(|b| vi::strtab::StringTable::parse(&b));
        let pending = gfx(canvas()?).await?;
        status("in the world");
        rate("in the world");
        viewer::run_with_gfx(lib, src, Some(play), Some(pending)).map_err(|e| e.to_string())
    }
    .await;
    r.map_err(js)
}
