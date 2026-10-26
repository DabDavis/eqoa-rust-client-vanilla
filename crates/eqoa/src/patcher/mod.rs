//! SETUP's Verant patcher (`decomp/modules/setup/src`, 454 of 462 functions
//! byte-exact; `claude-notes/decomp-setup-patcher-20261016.md`): after the
//! title's Play and before STATION, the server's manifest is fetched from
//! its patch port (7000, the path `data\psvrpath.dat` names), every file it
//! names is judged against the card's copy (by SIZE, unless the distribution
//! says UseCRC) and fetched whole when it differs (each must be gzipped and
//! carry the manifest's CRC once inflated), the Delete entries are marked
//! after the downloads, and the card is written back only when all of it
//! succeeded. Here the card is the server's own folder (`servers::folder`),
//! so each server keeps its own files (the user's word, 2026-10-23).
//!
//! The pieces: `http` (VUrl and Http_get), `image` (VIPatchImage, the
//! folder), `tree` (the downloaders and the distribution info), and here
//! `PatcherAPI_Impl` (`Api`) and `VISetup::RunPatcher`'s patch half (`run`)
//! with its window states, frame counts and bar. The PS2 runs it all on one
//! thread while its window draws; so does this (natively, `pregame::link`),
//! the window reading `Status` each frame. DNAS, before it on a PS2, is not
//! this client's to run.

pub mod http;
pub mod image;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
pub mod replay;
pub mod tree;
#[cfg(target_arch = "wasm32")]
pub mod web;

use image::Images;
use std::sync::{Arc, Mutex};

thread_local! {
    /// The lines of a play `replay` may throw away, held until it is known
    /// to stand (`replay::play`); None: each line said as it comes.
    static HELD: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

/// A line of the patcher's: said, or held while a page's play runs.
fn say(s: &str) {
    let held = HELD.with(|h| h.borrow_mut().as_mut().map(|v| v.push(s.to_string())).is_some());
    if !held {
        crate::say(s);
    }
}

/// The patch port SETUP passes `Initialize`.
pub const PORT: u16 = 7000;

/// `data\psvrpath.dat` on the retail disc, when the disc's cannot be read.
pub const MANIFEST: &str = "patch/eqoa/eqoa-frontierslive-update.xml.gz";

/// `VIWndPatcher::m_state` (setup.h), as RunPatcher sets it: the status
/// words after "Status:" (803).
pub mod state {
    /// the window draws nothing of its own (cancelled, or before the start)
    pub const CANCELLED: i32 = -1;
    /// 757 "Connecting to update server..."
    pub const CONNECTING: i32 = 0;
    /// 754 "Checking for updates..."
    pub const CHECKING: i32 = 1;
    /// 768 "Error while updating.", and the box 769
    pub const FAILED: i32 = 2;
    /// 812 "Updating" (OnFile, once the manifest is in)
    pub const UPDATING: i32 = 3;
    /// 764 "Done.", and the choices
    pub const DONE: i32 = 4;
    /// 792 "Reading game data"
    pub const READING: i32 = 5;
    /// 814 "Writing game data"
    pub const WRITING: i32 = 6;
    /// 768, and the box 787 (a module failed DNAS; never here)
    pub const AUTH_FAILED: i32 = 7;
}

/// What the patcher tells its owner (`PatcherAPI`'s virtuals; VISetup
/// overrides OnFile, OnTick and OnError, and answers 1 to the rest).
pub trait Callbacks {
    fn on_file(&mut self, _name: &str) {}
    fn on_tick(&mut self, _n: i32) {}
    fn on_error(&mut self, _code: i32, _text: &str) {}
    fn on_finish(&mut self) {}
    fn on_status_info(&mut self, _text: &str) {}
    /// VISetup's m_x218B0: the manifest is being read (OnFile then leaves
    /// the window's state alone)
    fn set_initializing(&mut self, _on: bool) {}
}

/// The platform's HTTP: connects to `host:port`, sends `request`, and hands
/// each piece of the reply to `piece` until it says the reply is whole (or
/// the far end closes). Natively a socket on the patcher's thread.
pub trait Wire {
    fn exchange(&mut self, host: &str, port: u16, request: &[u8], piece: &mut dyn FnMut(&[u8]) -> bool) -> Result<(), http::Error>;
}

/// `PatcherAPI_Impl`'s state (patcherapi_impl.cpp): the manifest's tree, the
/// card's images, the byte counts the bar is made of.
pub struct Api<'a> {
    pub cb: &'a mut dyn Callbacks,
    pub wire: &'a mut dyn Wire,
    pub images: Images,
    pub tree: tree::Tree,
    /// +0x008: the `<VerantPatcher>`'s Version (the `<Product>`'s is never read)
    pub version: i32,
    pub product_name: Option<String>,
    /// +0x010: "host:7000", as SETUP passes it
    pub server: String,
    pub port: u16,
    /// +0x3C4: the product's release notes
    pub changes: Vec<(i16, String)>,
    /// +0x3C8: the product's distribution info
    pub dist: Option<usize>,
    /// m_cancel and gPatcherCancel
    pub cancel: bool,
    pub bytes_done: i64,
    pub bytes_total: i64,
    pub quick_scan: bool,
    /// the last fetch's failure, for the owner (a server that does not answer)
    pub last_error: Option<http::Error>,
}

impl<'a> Api<'a> {
    pub fn new(cb: &'a mut dyn Callbacks, wire: &'a mut dyn Wire, images: Images) -> Api<'a> {
        Api { cb, wire, images, tree: tree::Tree::default(), version: 0, product_name: None, server: String::new(), port: PORT, changes: Vec::new(), dist: None, cancel: false, bytes_done: 0, bytes_total: 0, quick_scan: false, last_error: None }
    }

    /// The download's URL as both downloaders build it: a path that starts
    /// with a slash (or a backslash) goes straight after the port; every
    /// backslash becomes a slash.
    fn url(server: &str, port: i32, path: &str) -> String {
        let u = if let Some(rest) = path.strip_prefix(['/', '\\']) { format!("http://{server}:{port}/{rest}") } else { format!("http://{server}:{port}/{path}") };
        u.replace('\\', "/")
    }

    /// The bytes-downloaded tick (`OnBytesDownloaded`'s, inline wherever the
    /// count moves).
    fn tick(&mut self) {
        if self.bytes_total > 100 {
            self.cb.on_tick((self.bytes_done / (self.bytes_total / 100)) as i32);
        } else if self.bytes_total != 0 {
            self.cb.on_tick((self.bytes_done * 100 / self.bytes_total) as i32);
        }
    }

    /// `DownloadMemoryFile` / `DownloadToMemory`: `path` fetched into `size`
    /// bytes, as text (inflated, or as it came); None when the fetch failed.
    pub fn download_memory_file(&mut self, size: usize, server: Option<&str>, port: i32, path: Option<&str>, compressed: bool) -> Option<Vec<u8>> {
        let (Some(server), Some(path)) = (server, path) else { return None };
        if size == 0 {
            return None;
        }
        if self.cancel {
            return None;
        }
        let url = Self::url(server, port, path);
        // the first OnFile names the buffer, which reads "Hello"
        self.cb.on_file("Hello");
        let mut moved = 0usize;
        let r = http::get(self.wire, &url, size, &mut |n| moved += n);
        match r {
            Ok(body) => {
                self.last_error = None;
                Some(http::memory_text(&body, compressed, size))
            }
            Err(e) => {
                self.cb.on_error(e.code, &e.text);
                self.last_error = Some(e);
                None
            }
        }
    }

    /// `DownloadFile`: `path` fetched into the image named after `file`
    /// (cleared first, written whatever came: a failed fetch leaves it
    /// empty, which its CRC then refuses). The URL takes Initialize's port.
    pub fn download_file(&mut self, server: Option<&str>, path: Option<&str>, file: Option<&str>, compressed: bool, download_size: i64, total_size: i64) -> i32 {
        if self.cancel {
            return 100;
        }
        let (Some(path), Some(file)) = (path, file) else {
            self.cb.on_error(60, "Null path requested for download.\n");
            return 0;
        };
        let url = Self::url(server.unwrap_or("(null)"), self.port as i32, path);
        let file = file.replace('\\', "/");
        let started = download_size + total_size != 0;
        if started {
            self.cb.on_file(&file);
        }
        let saved = self.bytes_done;
        if let Some(im) = self.images.find(&file) {
            im.data.clear();
        }
        let total = self.bytes_total;
        let mut got = 0i64;
        let mut ticks: Vec<i64> = Vec::new();
        let r = http::get(self.wire, &url, 0x20_0000, &mut |n| {
            got += n as i64;
            ticks.push(got);
        });
        // the bar moves with each piece (`OnBytesDownloaded`)
        if total != 0 {
            for g in ticks {
                self.bytes_done = saved + g;
                self.tick();
            }
        }
        let (bytes, ok) = match r {
            Ok(body) => (http::file_bytes(&body, compressed, 0x20_0000), Ok(body.len())),
            Err(e) => (Vec::new(), Err(e)),
        };
        self.images.write(&file, bytes);
        match ok {
            Ok(n) => {
                self.last_error = None;
                if started {
                    self.bytes_done = saved + n as i64;
                    if total < self.bytes_done {
                        self.cb.on_tick(100);
                    } else {
                        self.tick();
                    }
                }
                0
            }
            Err(e) => {
                // Get_thread leaves an aborted fetch's status at 8: the error
                // is said and 100 returned
                self.bytes_done = saved;
                if started {
                    self.cb.on_tick(0);
                }
                self.cb.on_error(e.code, &e.text);
                self.last_error = Some(e);
                100
            }
        }
    }

    /// `Initialize` (0x00583278): the manifest at `file` on `server`
    /// ("host:7000") into 0x19000 bytes, and the tree built from it. A
    /// server rebuilding its manifest answers a page titled
    /// `_rerun_xml_script_`: the PS2 fetches again until it goes away and,
    /// a shipped bug, reports error 43 when that fetch SUCCEEDS, so such a
    /// manifest always fails the patch.
    pub fn initialize(&mut self, server: &str, port: u16, file: &str) -> bool {
        self.server = server.to_string();
        self.port = port;
        let Some(mut mem) = self.download_memory_file(0x19000, Some(server), port as i32, Some(file), true) else { return false };
        let mut ok = true;
        let mut told = false;
        while String::from_utf8_lossy(&mem).contains("<TITLE>_rerun_xml_script_</TITLE>") {
            self.cb.on_status_info(if told { "." } else { "Updating server files... Please be patient..." });
            told = true;
            match self.download_memory_file(0x19000, Some(server), port as i32, Some(file), true) {
                Some(m) => mem = m,
                None => return false,
            }
            self.cb.on_error(43, "Error opening remote xml file.\nCould not access patcher update file.  Please verify your network connection.  If the problem persists, please contact station.sony.com.\n");
            ok = false;
        }
        let root = if ok { crate::pregame::station::xml::parse(&mem).filter(|r| r.name.eq_ignore_ascii_case("VerantPatcher")) } else { None };
        self.continue_initialization(root)
    }

    /// `ContinueInitialization`: `<VerantPatcher><Product>` and nothing else.
    fn continue_initialization(&mut self, root: Option<crate::pregame::station::xml::Element>) -> bool {
        let Some(root) = root else {
            self.cb.on_error(41, "Document of the wrong type, VerantPatcher expected\n");
            return false;
        };
        self.version = tree::attr_long(&root, "Version");
        let Some(product) = root.children.first() else {
            self.cb.on_error(41, "No Product element found\n");
            return false;
        };
        if !product.name.eq_ignore_ascii_case("Product") {
            self.cb.on_error(41, "Outer node of the wrong type, Product expected\n");
            return false;
        }
        self.product_name = tree::attr(product, "Name");
        // InitializeChanges: every <Changes> of the product
        for c in product.children.iter().filter(|c| c.name.eq_ignore_ascii_case("Changes")) {
            self.set_changes_file(c);
        }
        // StartInitializeThread: the tree, then the needs rolled up
        let info = tree::Tree::initialize_product(self, product);
        let d = self.tree.infos[info].downloader;
        self.tree.calculate(d);
        self.dist = Some(info);
        self.cb.on_status_info("\n");
        true
    }

    /// The product's `SetChangesFile` (0x00583AF0): the release notes, plain
    /// unless the element says Compressed, into 0x1F400 bytes.
    fn set_changes_file(&mut self, e: &crate::pregame::station::xml::Element) {
        let server = tree::attr(e, "Server").unwrap_or_else(|| self.server.clone());
        let port = match tree::attr_long(e, "Port") {
            0 => self.port as i32,
            p => p,
        };
        let Some(remote) = tree::attr(e, "RemotePath").filter(|r| !r.is_empty()) else { return };
        if server.is_empty() {
            return;
        }
        if let Some(text) = self.download_memory_file(0x1F400, Some(&server), port, Some(&remote), tree::attr_bool(e, "Compressed")) {
            tree::insert_changes(&mut self.changes, tree::attr_long(e, "Version") as i16, String::from_utf8_lossy(&text).into_owned());
        }
    }

    /// `DownloadAll(0)` (`Download` and `DownloadRoutine`): the totals summed
    /// (what is left to fetch is the bar's 100%), then the product's tree.
    pub fn download_all(&mut self) -> bool {
        self.cancel = false;
        let Some(info) = self.dist else { return false };
        let d = self.tree.infos[info].downloader;
        self.bytes_done = 0;
        self.tree.reset_scan(d, 3);
        let _total = self.tree.size(d, 0, false);
        self.tree.reset_scan(d, 2);
        self.bytes_total = self.tree.size(d, 0, true);
        let ok = tree::Tree::download(self, d, 0) == 0;
        if !ok {
            say("patcher: PatcherAPI_Impl::DownloadRoutine successFalse 1");
        }
        self.tree.reset_scan(d, 2);
        if self.tree.size(d, 0, true) == 0 {
            self.cb.on_tick(100);
        }
        if !self.cancel {
            self.cb.on_finish();
        }
        ok
    }

    /// `DeleteOldFiles`: every Delete entry marks its image.
    pub fn delete_old_files(&mut self) -> bool {
        let Some(info) = self.dist else { return false };
        let d = self.tree.infos[info].downloader;
        tree::Tree::delete_old(self, d);
        true
    }

    /// `GetProductChanges`' text: the first of the product's release notes.
    pub fn product_changes(&self) -> String {
        self.changes.first().map(|c| c.1.clone()).unwrap_or_default()
    }
}

/// The patch window's state, written by `run` and read by the window each
/// frame.
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub state: i32,
    /// the bar, 0..1
    pub progress: f32,
    /// the product's release notes as the server sent them
    pub notes: String,
    /// files were written: the notes open by themselves at Done
    pub changed: bool,
    /// `run` has returned
    pub done: bool,
    /// why it failed, for the log and a report
    pub err: Option<String>,
    /// the patch server never answered (its name or its port): the server's
    /// folder is played as it stands (the client's, not the PS2's)
    pub unreachable: bool,
}

pub type Shared = Arc<Mutex<Status>>;

/// What `run` is told: where to patch from and into.
#[derive(Clone, Debug)]
pub struct Config {
    /// the server the player set (Network Setup)
    pub host: String,
    pub port: u16,
    /// `data\psvrpath.dat`'s line
    pub manifest: String,
    /// one of RunPatcher's MainLoop()s, which it counts between the steps so
    /// each status can be read (the PS2's frame is two vsyncs); 0 straight
    /// through
    pub frame_ms: u64,
}

/// The server's folder as the patch sees it: read whole, then written back
/// (and its record) only when the patch finished.
pub trait Folder {
    fn read(&mut self, progress: &mut dyn FnMut(f32)) -> Result<Images, String>;
    fn write(&mut self, images: &Images, progress: &mut dyn FnMut(f32)) -> Result<bool, String>;
    fn record(&mut self, r: &crate::servers::Record);
}

/// VISetup as the patcher's callbacks: OnFile turns the status to
/// "Updating" for four frames once the manifest is in; OnTick is the bar.
struct Setup<'s> {
    st: &'s Shared,
    frame_ms: u64,
    /// m_x218B0: the manifest is being read; OnFile leaves the state alone
    initializing: bool,
    /// m_progress: how much of the card was read
    read_done: f32,
    /// m_x218AC: how much of it was written back
    write_done: f32,
}

impl Setup<'_> {
    fn frames(&self, n: u64) {
        if self.frame_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(n * self.frame_ms));
        }
    }

    fn set_state(&self, s: i32) {
        self.st.lock().unwrap().state = s;
    }
}

impl Callbacks for Setup<'_> {
    fn on_file(&mut self, name: &str) {
        say(&format!("patcher: On File: {name}"));
        if !self.initializing {
            self.set_state(state::UPDATING);
            self.frames(4);
        }
    }

    /// The bar: reading the card is the first 20%, the download the next
    /// 60% and writing the card the last 20%.
    fn on_tick(&mut self, n: i32) {
        let mut p = self.read_done * 0.2;
        if n != 0 {
            p += n as f32 / 100.0 * 0.6;
        }
        if n == 100 {
            p += self.write_done * 0.2;
        }
        self.st.lock().unwrap().progress = p.clamp(0.0, 1.0);
    }

    fn on_error(&mut self, code: i32, text: &str) {
        say(&format!("patcher: ERROR CODE {code} / '{}'", text.trim_end()));
    }

    fn set_initializing(&mut self, on: bool) {
        self.initializing = on;
    }
}

/// `VISetup::RunPatcher`'s patch half (0x00568A28, from `SetState(-1)`):
/// the folder read, the manifest fetched, every file brought in line, the
/// deletes marked, the release notes taken, and the folder written back,
/// the window's state and bar moving as the console's do, frame counts
/// included. It always ends with `Status::done`; on a failure nothing is
/// written.
pub fn run(cfg: &Config, wire: &mut dyn Wire, folder: &mut dyn Folder, st: &Shared) {
    let mut s = Setup { st, frame_ms: cfg.frame_ms, initializing: false, read_done: 0.0, write_done: 0.0 };
    let fail = |st: &Shared, msg: String, unreachable: bool| {
        say(&format!("patcher: {msg}"));
        let mut g = st.lock().unwrap();
        if !unreachable {
            g.state = state::FAILED;
        }
        g.err = Some(msg);
        g.unreachable = unreachable;
        g.done = true;
    };
    s.set_state(state::CANCELLED);
    s.frames(20);
    s.set_state(state::READING);
    s.frames(10);
    say("patcher: reading patch data");
    let images = {
        let mut read_done = 0.0;
        let r = folder.read(&mut |f| {
            read_done = f;
        });
        s.read_done = read_done;
        s.on_tick(0);
        r
    };
    let images = match images {
        Ok(i) => i,
        Err(e) => return fail(st, format!("reading the server's folder: {e}"), false),
    };
    s.set_state(state::CONNECTING);
    s.frames(32);
    say(&format!("patcher: Patching with XML: {}", cfg.manifest));
    let mut cb = Setup { st, frame_ms: cfg.frame_ms, initializing: true, read_done: s.read_done, write_done: 0.0 };
    let mut api = Api::new(&mut cb, wire, images);
    let server = format!("{}:{}", cfg.host, cfg.port);
    let ok = api.initialize(&server, cfg.port, &cfg.manifest);
    if !ok {
        let unreachable = api.last_error.as_ref().is_some_and(http::Error::unreachable);
        return fail(st, "Patcher (Initialize) Failed".into(), unreachable);
    }
    api.cb.set_initializing(false);
    st.lock().unwrap().state = state::CHECKING;
    s.frames(32);
    if !api.download_all() {
        return fail(st, "Patcher (DownloadAll) Failed".into(), false);
    }
    if !api.delete_old_files() {
        return fail(st, "Patcher (DeleteOldFiles) Failed".into(), false);
    }
    let notes = api.product_changes();
    let images = std::mem::take(&mut api.images);
    let version = api.version;
    drop(api);
    st.lock().unwrap().state = state::WRITING;
    s.frames(10);
    let mut write_done = 0.0;
    let changed = match folder.write(&images, &mut |f| write_done = f) {
        Ok(c) => c,
        Err(e) => return fail(st, format!("writing the server's folder: {e}"), false),
    };
    s.write_done = write_done;
    s.on_tick(100);
    let mut files: Vec<crate::servers::Kept> = images.list.iter().filter(|im| !im.deleted).map(|im| crate::servers::Kept { name: im.name.clone(), crc: im.crc, size: im.data.len() as u64 }).collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    let when = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    folder.record(&crate::servers::Record { server: cfg.host.clone(), manifest: cfg.manifest.clone(), when, files });
    say(&format!("patcher: {} version {version}: done ({})", cfg.host, if changed { "files changed" } else { "nothing to change" }));
    let mut g = st.lock().unwrap();
    g.progress = 1.0;
    g.state = state::DONE;
    g.notes = notes;
    g.changed = changed;
    g.done = true;
}

#[cfg(test)]
mod tests;
