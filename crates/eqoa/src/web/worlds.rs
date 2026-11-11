//! A server's own worlds in a page (`serverworlds`; natively built on a thread
//! into the cache directory). The page builds each one off the player's disc
//! in a worker (web/worlds.js, the client's own module instantiated again) into
//! a file of the origin's private storage (OPFS, `worlds/world<id>-<crc>.ESF`,
//! the name the cache gives it natively), streamed: the composer
//! (`serverworlds::compose_into`) reads a zone's record off the disc at a time
//! (`FileReaderSync` over the image the player picked), checks it against its
//! CRC, moves its offsets and writes it to the file (a
//! `FileSystemSyncAccessHandle`), and the file is then checked against the
//! server's size and CRC, read back. Neither the worker nor the game ever holds
//! the world (133 MB for the Homelands): the worker's heap holds one zone's
//! record, and goes with the worker.
//!
//! A later visit checks the kept file by its size and CRC, in the worker again,
//! and does no more. The game then opens it as it opens a world of the disc
//! (`WebDisc::of_file`: its zones left in it, read as the walk nears them,
//! `stream`), at a login there (`super::world_loader`), at a move into it
//! (`WebDisc::world_opener`) and for its zones' reads (`WebDisc::zone_sources`).
//!
//! The session takes the server's offer only where this can be done
//! (`usable`: the origin's private storage there, and workers): elsewhere (a
//! page on plain http to another host, a private window without the storage)
//! it answers as before, and a character of a server's world starts in its
//! mainland city. While a world is being built a notice at the page's top says
//! how far it has come, and a character played meanwhile whose login needs it
//! (the server says which: `net::session::Session::pick_needs_worlds`) waits
//! for it (`pregame::client`), so the server places it by what the page holds;
//! any other goes in at once, and the build goes on while it plays (the
//! session asks for the list again in the world when it went in before it).

use super::disc::WebDisc;
use crate::serverworlds::{self, Disc, Held};
use net::serverworlds::World;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

thread_local! {
    /// The player's disc, which the builder reads (`install`).
    static DISC: RefCell<Option<web_sys::File>> = const { RefCell::new(None) };
    /// The server's worlds held in the origin's storage, each its file there,
    /// by the place `Held::path` names.
    static FILES: RefCell<HashMap<String, web_sys::File>> = RefCell::new(HashMap::new());
}

/// The name a world is kept under: its id and CRC, as natively.
fn kept_name(w: &World) -> String {
    format!("world{}-{:08x}.ESF", w.id, w.crc)
}

/// main.js's `window.eqoaWorlds`.
fn page_worlds() -> Option<JsValue> {
    let w = web_sys::window()?;
    js_sys::Reflect::get(&w, &"eqoaWorlds".into()).ok().filter(|v| v.is_object())
}

fn err(e: JsValue) -> String {
    e.dyn_ref::<js_sys::Error>().map(|e| String::from(e.message())).or_else(|| e.as_string()).unwrap_or_else(|| format!("{e:?}"))
}

/// Whether this page can keep a server's world: the origin's private storage
/// answers (none on plain http to another host; none in some private
/// windows) and workers run (`eqoaWorlds.usable`, main.js).
pub async fn usable() -> bool {
    let Some(w) = page_worlds() else { return false };
    let Some(f) = js_sys::Reflect::get(&w, &"usable".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()) else { return false };
    let Ok(p) = f.call0(&w) else { return false };
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&p)).await.is_ok_and(|v| v.is_truthy())
}

/// The session takes a server's offer of its worlds from now on and builds
/// them off `disc` (`net::session::set_world_builder`).
pub fn install(disc: web_sys::File) {
    DISC.with(|d| *d.borrow_mut() = Some(disc));
    net::session::set_world_builder(build_all);
}

/// A server's world held, as a disc of that one file (`WebDisc::of_file`):
/// the world file `name` (any case) when it is one of the server's held
/// here.
pub fn disc_of(name: &str) -> Option<WebDisc> {
    let path = serverworlds::path_of(name)?;
    let file = FILES.with(|f| f.borrow().get(&path.to_string_lossy().into_owned()).cloned())?;
    Some(WebDisc::of_file(file, name))
}

/// The notice at the page's top that says how far the build has come
/// (`eqoaNotices.notice`, notices.js); none clears it.
fn notice(text: Option<&str>) {
    let Some(w) = web_sys::window() else { return };
    let Ok(n) = js_sys::Reflect::get(&w, &"eqoaNotices".into()) else { return };
    if let Ok(f) = js_sys::Reflect::get(&n, &"notice".into())
        && let Some(f) = f.dyn_ref::<js_sys::Function>()
    {
        let _ = f.call3(&n, &"server-worlds".into(), &"".into(), &text.unwrap_or("").into());
    }
}

/// What the session's builder does with a server's list in a page: each world
/// found kept in the origin's storage or built there off the disc, one after
/// another, then the ids of those held answered.
fn build_all(worlds: &[World]) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let worlds = worlds.to_vec();
    serverworlds::forget();
    let disc = DISC.with(|d| d.borrow().clone());
    wasm_bindgen_futures::spawn_local(async move {
        let mut ids = Vec::new();
        for w in &worlds {
            let t = web_time::Instant::now();
            super::log(&format!("server worlds: world {} {:?} ({} zones): the copy kept in this browser checked, else built off the disc", w.id, w.name, w.parts.len()));
            let r = match &disc {
                Some(d) => hold(d, w).await,
                None => Err(("no disc".to_string(), false)),
            };
            match r {
                Ok((file, how, heap)) => {
                    let name = kept_name(w);
                    let path = PathBuf::from(format!("opfs:worlds/{name}"));
                    super::log(&format!(
                        "server worlds: world {} {:?} ({} zones) {how} in {} ms (the worker's heap {:.0} MB), held in this browser's storage, worlds/{name}",
                        w.id,
                        w.name,
                        w.parts.len(),
                        t.elapsed().as_millis(),
                        heap / 1048576.0
                    ));
                    FILES.with(|f| f.borrow_mut().insert(path.to_string_lossy().into_owned(), file));
                    serverworlds::keep(Held { id: w.id, file: format!("WORLD{}.ESF", w.id), path, parts: w.parts.clone(), name: w.name.clone() });
                    ids.push(w.id);
                }
                Err((e, quiet)) => {
                    super::log(&format!("server worlds: world {} {:?} not held: {e}", w.id, w.name));
                    // another disc's lands are not the server's: said plainly
                    let why = if e.contains("not the server's") { "your disc's lands are not the ones it is made of".to_string() } else { e };
                    if !quiet {
                        super::problem(
                            "server-worlds",
                            &format!("{} could not be built from your disc in this browser ({why}). A character from there starts in its home city.", title_name(&w.name)),
                        );
                    }
                }
            }
        }
        notice(None);
        let _ = tx.send(ids);
    });
    rx
}

/// "the Homelands" at the start of a line.
fn title_name(name: &str) -> String {
    let mut c = name.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_else(|| "The server's world".into())
}

/// World `w` held: the page's worker checks the copy kept in the origin's
/// storage, or builds it off `disc` there (`eqoaWorlds.build`, main.js), and
/// says how far it has come (the notice); the kept file, which of the two
/// was done, and the worker's heap at its end. An error says why, and
/// whether it is only that this browser cannot (`quiet`: nothing to tell).
async fn hold(disc: &web_sys::File, w: &World) -> Result<(web_sys::File, String, f64), (String, bool)> {
    let plain = |e: &str| (e.to_string(), false);
    let page = page_worlds().ok_or_else(|| plain("the page has no eqoaWorlds"))?;
    let build = js_sys::Reflect::get(&page, &"build".into()).ok().and_then(|f| f.dyn_into::<js_sys::Function>().ok()).ok_or_else(|| plain("the page has no eqoaWorlds.build"))?;
    let list = js_sys::Uint8Array::from(&net::serverworlds::encode(std::slice::from_ref(w))[..]);
    let (what, mb) = (w.name.clone(), f64::from(w.size) / 1048576.0);
    // the notice goes up for a build (seconds) at once, for a check of the
    // kept copy (a fraction of one) only once it has taken a while
    let started = web_time::Instant::now();
    let progress = Closure::<dyn FnMut(String, f64, f64)>::new(move |phase: String, done: f64, total: f64| {
        let pc = if total > 0.0 { (done / total * 100.0).floor() as i32 } else { 0 };
        let text = match phase.as_str() {
            "build" => format!("Building {what} from your disc: {pc}% ({:.0} of {mb:.0} MB). A character who is there waits for it; any other can play now.", done / 1048576.0),
            "verify" => format!("Checking {what}, built from your disc: {pc}%."),
            _ if started.elapsed().as_millis() < 1500 => return,
            _ => format!("Checking {what} kept in this browser: {pc}%."),
        };
        notice(Some(&text));
    });
    let args = js_sys::Array::of5(disc, &list, &JsValue::from(kept_name(w)), &JsValue::from(f64::from(w.size)), progress.as_ref());
    let p = build.apply(&page, &args).map_err(|e| (err(e), false))?;
    let v = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::resolve(&p)).await.map_err(|e| {
        let quiet = js_sys::Reflect::get(&e, &"quiet".into()).is_ok_and(|q| q.is_truthy());
        (err(e), quiet)
    });
    drop(progress);
    let v = v?;
    let file = js_sys::Reflect::get(&v, &"file".into()).ok().and_then(|f| f.dyn_into::<web_sys::File>().ok()).ok_or_else(|| plain("the worker kept no file"))?;
    if file.size() as u64 != u64::from(w.size) {
        return Err(plain(&format!("the kept file is {} bytes, not {}", file.size(), w.size)));
    }
    let how = js_sys::Reflect::get(&v, &"how".into()).ok().and_then(|h| h.as_string()).unwrap_or_default();
    let heap = js_sys::Reflect::get(&v, &"heap".into()).ok().and_then(|h| h.as_f64()).unwrap_or(0.0);
    Ok((file, how, heap))
}

// ---- the worker's half (web/worlds.js): synchronous reads and writes, which
// only a dedicated worker has

/// The one world the page's list names (`net::serverworlds::encode` of it).
fn world_of(list: &[u8]) -> Result<World, JsValue> {
    net::serverworlds::read(list).and_then(|mut w| w.pop()).ok_or_else(|| JsValue::from_str("not a server's world"))
}

/// A file read synchronously in the worker (`FileReaderSync`): the player's
/// disc image (`serverworlds::Disc`, `iso::Source`), whose reads of a zone's
/// size say how far the build has come, or a world kept in the origin's
/// storage.
struct SyncFile {
    file: web_sys::File,
    reader: web_sys::FileReaderSync,
    len: u64,
    entries: Vec<iso::Entry>,
    /// bytes read in reads of a megabyte or more, and the total expected
    read: Cell<f64>,
    total: f64,
    /// told `("build", read, total)` after each (the disc's alone)
    progress: Option<js_sys::Function>,
}

fn io(e: JsValue) -> std::io::Error {
    std::io::Error::other(err(e))
}

impl SyncFile {
    fn new(file: web_sys::File, total: f64, progress: Option<&js_sys::Function>) -> Result<Self, JsValue> {
        let reader = web_sys::FileReaderSync::new()?;
        Ok(SyncFile { len: file.size() as u64, file, reader, entries: Vec::new(), read: Cell::new(0.0), total, progress: progress.cloned() })
    }

    fn read(&self, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
        let end = off.checked_add(buf.len() as u64).filter(|&e| e <= self.len).ok_or(std::io::ErrorKind::UnexpectedEof)?;
        let blob = self.file.slice_with_f64_and_f64(off as f64, end as f64).map_err(io)?;
        let a = js_sys::Uint8Array::new(self.reader.read_as_array_buffer(&blob).map_err(io)?.as_ref());
        if a.length() as usize != buf.len() {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        a.copy_to(buf);
        if let Some(p) = &self.progress
            && buf.len() >= 1 << 20
        {
            self.read.set(self.read.get() + buf.len() as f64);
            let _ = p.call3(&JsValue::NULL, &"build".into(), &self.read.get().into(), &self.total.into());
        }
        Ok(())
    }
}

impl iso::Source for SyncFile {
    fn size(&mut self) -> std::io::Result<u64> {
        Ok(self.len)
    }
    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.read(off, buf)
    }
}

impl Disc for SyncFile {
    fn file(&self, name: &str) -> Option<(u64, u64)> {
        let e = self.entries.iter().find(|e| e.path.rsplit('/').next().is_some_and(|f| f.eq_ignore_ascii_case(name)))?;
        Some((u64::from(e.lba) * iso::SECTOR, u64::from(e.size)))
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.read(off, buf)
    }
}

/// How much of a kept world is read at a time to check its CRC.
const CHUNK: usize = serverworlds::CHECK_STEP;

/// A file of the origin's storage open for writing in the worker: its
/// `FileSystemSyncAccessHandle`, whose reads and writes at an offset are
/// synchronous (`serverworlds::FileSink` builds a world into it).
struct Handle(web_sys::FileSystemSyncAccessHandle);

impl serverworlds::FileAt for Handle {
    fn write_at(&mut self, at: u64, b: &[u8]) -> Result<(), String> {
        let o = web_sys::FileSystemReadWriteOptions::new();
        o.set_at(at as f64);
        let n = self.0.write_with_u8_array_and_options(b, &o).map_err(err)?;
        if n as usize != b.len() {
            return Err(format!("{n} of {} bytes written (is the browser's storage full?)", b.len()));
        }
        Ok(())
    }

    fn read_at(&mut self, at: u64, b: &mut [u8]) -> Result<(), String> {
        let o = web_sys::FileSystemReadWriteOptions::new();
        o.set_at(at as f64);
        let n = self.0.read_with_u8_array_and_options(b, &o).map_err(err)?;
        if n as usize != b.len() {
            return Err(format!("{n} of {} bytes read back at {at}", b.len()));
        }
        Ok(())
    }
}

/// Small writes are gathered to this size before they go to the file.
const GATHER: usize = 1 << 20;

/// The worker's check of a world kept in the origin's storage (`kept`, a
/// File of it): the size and CRC of the world the list names, read a piece
/// at a time; `progress("check", done, total)` as it goes.
#[wasm_bindgen]
pub fn world_kept(kept: web_sys::File, list: &[u8], progress: &js_sys::Function) -> Result<bool, JsValue> {
    let w = world_of(list)?;
    if kept.size() as u64 != u64::from(w.size) {
        return Ok(false);
    }
    let f = SyncFile::new(kept, 0.0, None)?;
    let mut chunk = vec![0u8; CHUNK.min(f.len as usize)];
    let (mut crc, mut at) = (0u32, 0u64);
    while at < f.len {
        let n = CHUNK.min((f.len - at) as usize);
        // a read that fails (a file the browser let go of) is a copy to build again
        if f.read(at, &mut chunk[..n]).is_err() {
            return Ok(false);
        }
        crc = serverworlds::crc32_update(crc, &chunk[..n]);
        at += n as u64;
        let _ = progress.call3(&JsValue::NULL, &"check".into(), &(at as f64).into(), &(f.len as f64).into());
    }
    Ok(crc == w.crc)
}

/// The worker's build of the world the list names off the disc image
/// `disc` into the file `handle` is open on (emptied first): the composer
/// streamed into it, a zone at a time (`serverworlds::compose_into`), its
/// CRC checked; `progress("build" | "verify", done, total)` as it goes. An
/// error says why it is not the server's world (another disc, the storage
/// full), and what was written is the caller's to remove.
#[wasm_bindgen]
pub fn world_build(disc: web_sys::File, list: &[u8], handle: web_sys::FileSystemSyncAccessHandle, progress: &js_sys::Function) -> Result<(), JsValue> {
    let w = world_of(list)?;
    let total = w.parts.iter().map(|p| f64::from(p.size)).sum();
    let mut d = SyncFile::new(disc, total, Some(progress))?;
    d.entries = iso::list(&mut d).map_err(|e| JsValue::from_str(&format!("not an EQOA disc image: {e}")))?;
    handle.truncate_with_f64(0.0)?;
    let p = progress.clone();
    let step = Box::new(move |done: u64, total: u64| {
        let _ = p.call3(&JsValue::NULL, &"verify".into(), &(done as f64).into(), &(total as f64).into());
    });
    let mut sink = serverworlds::FileSink::new(Handle(handle), GATHER, step);
    serverworlds::compose_into(&d, &w, &mut sink).map_err(|e| JsValue::from_str(&e))?;
    sink.file.0.flush()?;
    Ok(())
}

/// The worker's wasm heap now (its peak: a heap only grows), in bytes.
#[wasm_bindgen]
pub fn heap_bytes() -> f64 {
    wasm_bindgen::memory().dyn_into::<js_sys::WebAssembly::Memory>().map_or(0.0, |m| m.buffer().dyn_into::<js_sys::ArrayBuffer>().map_or(0.0, |b| b.byte_length() as f64))
}
