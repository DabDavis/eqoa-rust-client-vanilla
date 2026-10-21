//! The map's cache: each zone's tile (`t<zone>.png`, 512 across) and its
//! overview cell (`c<zone>.png`, 128), kept under the world's key
//! (`cells::cache_key`) so a world's map is drawn once on a machine. Natively
//! a directory, `eqoa-rs/map/<key>` in the platform's cache directory (as the
//! unpacked modules are kept, `modules.rs`; `$EQOA_MAP_CACHE` in its place);
//! in a page the origin's private storage (OPFS), `map/<key>`, through the
//! page's `eqoaMapGet`/`eqoaMapPut`/`eqoaMapList` (main.js). Reads answer on
//! a later frame in a page, at once natively: both come back through `poll`.
//! A write is whole or not there (a file renamed into place; OPFS's writable
//! commits on close), so a run cut short leaves no half picture.

/// What came back from the cache.
#[derive(Debug, PartialEq)]
pub enum Got {
    /// a file's bytes, `None` when it is not kept
    File(String, Option<Vec<u8>>),
    /// every name kept under the key
    List(Vec<String>),
}

pub trait Store {
    /// Starts reading `name`.
    fn get(&mut self, name: &str);
    /// Keeps `bytes` as `name`.
    fn put(&mut self, name: &str, bytes: Vec<u8>);
    /// Starts listing what is kept.
    fn list(&mut self);
    /// One answer, if any has come.
    fn poll(&mut self) -> Option<Got>;
    /// Where it keeps things, for the log.
    fn describe(&self) -> String;
}

/// The store for the world `key` on this platform; `None` where nothing can
/// be kept (no cache directory).
pub fn open(key: &str) -> Option<Box<dyn Store>> {
    #[cfg(target_arch = "wasm32")]
    {
        Some(Box::new(web::Opfs::new(key)))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let root = root()?;
        prune(&root, key);
        Some(Box::new(Dir::new(root.join(key))))
    }
}

/// Where the map's worlds are kept natively: `$EQOA_MAP_CACHE`, else
/// `eqoa-rs/map` beside the unpacked modules' `eqoa-rs/dnas`.
#[cfg(not(target_arch = "wasm32"))]
fn root() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Some(d) = std::env::var_os("EQOA_MAP_CACHE") {
        return Some(PathBuf::from(d));
    }
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".cache")))
    };
    base.map(|b| b.join("eqoa-rs").join("map"))
}

/// The same world's older pictures (another build of the map, or of the
/// world's file) let go: directories named for its file but not `key`.
#[cfg(not(target_arch = "wasm32"))]
fn prune(root: &std::path::Path, key: &str) {
    let Some((stem, _)) = key.rsplit_once('-') else { return };
    let Ok(dirs) = std::fs::read_dir(root) else { return };
    for d in dirs.flatten() {
        let name = d.file_name().to_string_lossy().to_string();
        if name != key && name.rsplit_once('-').is_some_and(|(s, h)| s == stem && h.len() == 16) {
            let _ = std::fs::remove_dir_all(d.path());
        }
    }
}

/// A directory, read and written at once.
#[cfg(not(target_arch = "wasm32"))]
pub struct Dir {
    dir: std::path::PathBuf,
    done: std::collections::VecDeque<Got>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Dir {
    pub fn new(dir: std::path::PathBuf) -> Self {
        Dir { dir, done: Default::default() }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Store for Dir {
    fn get(&mut self, name: &str) {
        self.done.push_back(Got::File(name.to_string(), std::fs::read(self.dir.join(name)).ok()));
    }

    fn put(&mut self, name: &str, bytes: Vec<u8>) {
        let (p, tmp) = (self.dir.join(name), self.dir.join(format!("{name}.tmp")));
        let kept = std::fs::create_dir_all(&self.dir).and_then(|()| std::fs::write(&tmp, &bytes)).and_then(|()| std::fs::rename(&tmp, &p));
        if let Err(e) = kept {
            eprintln!("map: {} not kept ({e})", p.display());
        }
    }

    fn list(&mut self) {
        let names = std::fs::read_dir(&self.dir).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| !n.ends_with(".tmp")).collect()).unwrap_or_default();
        self.done.push_back(Got::List(names));
    }

    fn poll(&mut self) -> Option<Got> {
        self.done.pop_front()
    }

    fn describe(&self) -> String {
        self.dir.display().to_string()
    }
}

/// In memory (for tests): answers come out of `poll` in the order asked.
#[cfg(test)]
#[derive(Default)]
pub struct Mem {
    pub files: std::collections::BTreeMap<String, Vec<u8>>,
    due: std::collections::VecDeque<Got>,
}

#[cfg(test)]
impl Store for Mem {
    fn get(&mut self, name: &str) {
        self.due.push_back(Got::File(name.to_string(), self.files.get(name).cloned()));
    }
    fn put(&mut self, name: &str, bytes: Vec<u8>) {
        self.files.insert(name.to_string(), bytes);
    }
    fn list(&mut self) {
        self.due.push_back(Got::List(self.files.keys().cloned().collect()));
    }
    fn poll(&mut self) -> Option<Got> {
        self.due.pop_front()
    }
    fn describe(&self) -> String {
        "memory".into()
    }
}

/// The page's private storage, through main.js.
#[cfg(target_arch = "wasm32")]
mod web {
    use super::{Got, Store};
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use wasm_bindgen::{JsCast, JsValue};

    pub struct Opfs {
        key: String,
        done: Rc<RefCell<VecDeque<Got>>>,
    }

    impl Opfs {
        pub fn new(key: &str) -> Self {
            // the same world's older pictures let go (main.js's eqoaMapPrune)
            let _ = call("eqoaMapPrune", &[key.into()]);
            Opfs { key: key.to_string(), done: Default::default() }
        }
    }

    /// The page's function `name` called with `args`: its promise.
    fn call(name: &str, args: &[JsValue]) -> Option<js_sys::Promise> {
        let w = web_sys::window()?;
        let f = js_sys::Reflect::get(&w, &name.into()).ok()?.dyn_into::<js_sys::Function>().ok()?;
        let a: js_sys::Array = args.iter().collect();
        f.apply(&JsValue::NULL, &a).ok()?.dyn_into::<js_sys::Promise>().ok()
    }

    impl Store for Opfs {
        fn get(&mut self, name: &str) {
            let (done, name) = (self.done.clone(), name.to_string());
            let p = call("eqoaMapGet", &[self.key.as_str().into(), name.as_str().into()]);
            wasm_bindgen_futures::spawn_local(async move {
                let bytes = match p {
                    Some(p) => wasm_bindgen_futures::JsFuture::from(p).await.ok().filter(|v| !v.is_null() && !v.is_undefined()).map(|v| js_sys::Uint8Array::new(&v).to_vec()),
                    None => None,
                };
                done.borrow_mut().push_back(Got::File(name, bytes));
            });
        }

        fn put(&mut self, name: &str, bytes: Vec<u8>) {
            let a = js_sys::Uint8Array::from(bytes.as_slice());
            if let Some(p) = call("eqoaMapPut", &[self.key.as_str().into(), name.into(), a.into()]) {
                wasm_bindgen_futures::spawn_local(async move {
                    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
                });
            }
        }

        fn list(&mut self) {
            let done = self.done.clone();
            let p = call("eqoaMapList", &[self.key.as_str().into()]);
            wasm_bindgen_futures::spawn_local(async move {
                let names = match p {
                    Some(p) => wasm_bindgen_futures::JsFuture::from(p).await.ok().map(|v| js_sys::Array::from(&v).iter().filter_map(|n| n.as_string()).collect()).unwrap_or_default(),
                    None => Vec::new(),
                };
                done.borrow_mut().push_back(Got::List(names));
            });
        }

        fn poll(&mut self) -> Option<Got> {
            self.done.borrow_mut().pop_front()
        }

        fn describe(&self) -> String {
            format!("the page's storage, map/{}", self.key)
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    /// A directory keeps what is put whole (no `.tmp` left, none listed),
    /// answers a name it lacks with none, and an older picture of the same
    /// world is let go when the world's key changes while another world's
    /// stays. The control: a name never put.
    #[test]
    fn a_directory_keeps_and_lets_go() {
        let root = std::env::temp_dir().join(format!("eqoa-map-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let old = root.join("tunaria-0000000000000001");
        let other = root.join("odus-0000000000000001");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        prune(&root, "tunaria-0000000000000002");
        assert!(!old.exists() && other.exists());
        let mut s = Dir::new(root.join("tunaria-0000000000000002"));
        s.put("t3.png", vec![1, 2, 3]);
        s.get("t3.png");
        s.get("t4.png");
        s.list();
        assert_eq!(s.poll(), Some(Got::File("t3.png".into(), Some(vec![1, 2, 3]))));
        assert_eq!(s.poll(), Some(Got::File("t4.png".into(), None)), "the control: never put");
        assert_eq!(s.poll(), Some(Got::List(vec!["t3.png".into()])));
        assert_eq!(s.poll(), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
