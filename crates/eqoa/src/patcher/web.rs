//! The patcher's platform in a page: a page reaches no patch port (7000 is
//! plain HTTP, and an https page may not fetch over http), so the files are
//! fetched over the page's own scheme from the server's host, the paths the
//! manifest names as the patch port serves them (`https://<host>/<path>`,
//! the server's web server serving its patch port's files), and the folder
//! is the origin's storage (`servers::page`).
//!
//! A fetch in a page cannot wait inside `run`, so `run` is played over what
//! was fetched (`replay`) until it asks for nothing new; only that last
//! play writes the folder.

use super::image::{self, Image, Images};
use super::replay::{self, Fetched};
use super::{Config, Shared, state};
use std::path::PathBuf;
use wasm_bindgen::JsCast;

/// Plays of `run` before the patch is given up: the manifest, its files,
/// and room for a tree that asks for its files one at a time.
const PLAYS: usize = 12;

/// How long a fetch waits for the server's answer: natively 10 s to connect
/// to its patch port (`native::Tcp`); a fetch would otherwise wait as long
/// as the browser tries a host that drops its packets, the window on
/// Connecting all that time (Chromium on Linux: 185 s, measured).
const ANSWER_MS: i32 = 10_000;

/// The server's folder in the page's storage; what `run` writes is held
/// until the play it came from is known to be the last.
struct Page {
    dir: PathBuf,
    written: Option<Images>,
    record: Option<crate::servers::Record>,
}

impl super::Folder for Page {
    fn read(&mut self, progress: &mut dyn FnMut(f32)) -> Result<Images, String> {
        let mut names: Vec<String> = crate::servers::page::names(&self.dir).into_iter().filter(|n| image::is_card_file(n)).collect();
        names.sort();
        let mut images = Images::default();
        let n = names.len().max(1);
        for (i, name) in names.into_iter().enumerate() {
            let mut data = crate::servers::page::read(&self.dir, &name).ok_or_else(|| format!("{name}: could not be read"))?;
            data.truncate(0x40_0000);
            let crc = image::crc32(&data);
            images.list.insert(0, Image { name, data, crc, dirty: false, deleted: false });
            progress((i + 1) as f32 / n as f32);
        }
        Ok(images)
    }

    fn write(&mut self, images: &Images, progress: &mut dyn FnMut(f32)) -> Result<bool, String> {
        progress(1.0);
        if !images.changed() {
            return Ok(false);
        }
        if let Some(im) = images.list.iter().find(|im| !image::safe_name(&im.name)) {
            return Err(format!("refusing to write {:?}: not a plain file name", im.name));
        }
        self.written = Some(images.clone());
        Ok(true)
    }

    fn record(&mut self, r: &crate::servers::Record) {
        self.record = Some(r.clone());
    }
}

impl Page {
    /// The last play's writes into the storage, as `image::write_folder`
    /// writes a folder.
    fn keep(&self) -> Result<(), String> {
        for im in self.written.iter().flat_map(|i| &i.list) {
            if im.dirty && !im.deleted {
                crate::say(&format!("patcher: POST PATCH: Writing (new/modified) file ({}), CRC = {}", im.name, im.crc));
                crate::servers::page::write(&self.dir, &im.name, &im.data)?;
            }
            if im.deleted {
                crate::say(&format!("patcher: POST PATCH: Deleting file ({})", im.name));
                crate::servers::page::remove(&self.dir, &im.name);
            }
        }
        if let Some(r) = &self.record
            && let Err(e) = crate::servers::write_record(&self.dir, r)
        {
            crate::say(&format!("patcher: the record was not kept: {e}"));
        }
        Ok(())
    }
}

/// `patcher::run` against the server at `host` into its folder, on a task
/// of its own; the window reads the status it returns each frame, as
/// natively.
pub fn start(host: &str, manifest: &str) -> Shared {
    let st: Shared = Default::default();
    let cfg = Config { host: host.to_string(), port: super::PORT, manifest: manifest.to_string(), frame_ms: 0 };
    let out = st.clone();
    wasm_bindgen_futures::spawn_local(async move { patch(&cfg, &out).await });
    st
}

async fn patch(cfg: &Config, st: &Shared) {
    let base = base(&cfg.host);
    let mut got = Fetched::new();
    for _ in 0..PLAYS {
        let mut folder = Page { dir: crate::servers::folder(&cfg.host), written: None, record: None };
        let (mut s, missing, said) = replay::play(cfg, &got, &mut folder);
        if missing.is_empty() {
            for line in &said {
                crate::say(line);
            }
            if s.state == state::DONE
                && let Err(e) = folder.keep()
            {
                crate::say(&format!("patcher: writing the server's folder: {e}"));
                s.state = state::FAILED;
                s.err = Some(format!("writing the server's folder: {e}"));
            }
            *st.lock().unwrap() = s;
            return;
        }
        st.lock().unwrap().state = if got.is_empty() { state::CONNECTING } else { state::UPDATING };
        // every path this play asked for, fetched at once
        for p in &missing {
            crate::say(&format!("patcher: fetching {base}/{p}"));
        }
        let fetches: Vec<_> = missing.into_iter().map(|p| (fetch(&format!("{base}/{p}")), p)).collect();
        for (f, p) in fetches {
            got.insert(p, f.await);
        }
    }
    crate::say("patcher: the patch kept asking for more files; given up");
    let mut s = st.lock().unwrap();
    s.state = state::FAILED;
    s.err = Some("the patch kept asking for more files".into());
    s.done = true;
}

/// Where a page fetches the server's patch files: its host over the page's
/// own scheme (an https page may fetch over https only).
fn base(host: &str) -> String {
    let http = web_sys::window().and_then(|w| w.location().protocol().ok()).is_some_and(|p| p == "http:");
    format!("{}://{host}", if http { "http" } else { "https" })
}

/// A GET of `url` begun now (every fetch of a play runs at once), the
/// browser's copy revalidated: its status and body, or why there is none
/// (no answer in `ANSWER_MS` among them).
fn fetch(url: &str) -> impl std::future::Future<Output = Result<(u16, Vec<u8>), String>> + use<> {
    let begun = web_sys::window().map(|w| {
        let init = web_sys::RequestInit::new();
        init.set_cache(web_sys::RequestCache::NoCache);
        let answer = w.fetch_with_str_and_init(url, &init);
        // rejects with undefined, where a failed fetch rejects with an error
        let late = js_sys::Promise::new(&mut |_, reject| {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&reject, ANSWER_MS);
        });
        js_sys::Promise::race(&js_sys::Array::of2(&answer, &late))
    });
    let url = url.to_string();
    async move {
        let p = begun.ok_or("no window")?;
        let r = wasm_bindgen_futures::JsFuture::from(p).await.map_err(|e| {
            if e.is_undefined() { format!("{url}: no answer in {} s", ANSWER_MS / 1000) } else { format!("{url}: {e:?}") }
        })?;
        let r: web_sys::Response = r.dyn_into().map_err(|_| format!("{url}: not a response"))?;
        let body = r.array_buffer().map_err(|e| format!("{url}: {e:?}"))?;
        let body = wasm_bindgen_futures::JsFuture::from(body).await.map_err(|e| format!("{url}: {e:?}"))?;
        Ok((r.status(), js_sys::Uint8Array::new(&body).to_vec()))
    }
}
