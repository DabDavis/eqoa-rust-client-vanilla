//! The player's disc image in a browser: a `File` the page hands over
//! (nothing is uploaded or downloaded), read a slice at a time. Reads there
//! are asynchronous, so a whole file is fetched before the game reads it, and
//! anything that walks the image synchronously (the ISO directory, a world
//! file's record headers) runs against an [`iso::Sparse`] cache that is
//! filled between attempts.

use iso::{Entry, Sparse};
use js_sys::Uint8Array;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

pub struct WebDisc {
    file: web_sys::File,
    len: u64,
    pub entries: Vec<Entry>,
    /// What the synchronous walks have needed so far.
    pub sparse: Sparse,
}

fn err(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}

/// `len` bytes at `off` of the image.
pub(super) async fn read_blob(file: &web_sys::File, off: u64, len: usize) -> Result<Vec<u8>, String> {
    let end = (off + len as u64).min(file.size() as u64);
    let blob = file.slice_with_f64_and_f64(off as f64, end as f64).map_err(err)?;
    let buf = JsFuture::from(blob.array_buffer()).await.map_err(err)?;
    Ok(Uint8Array::new(&buf).to_vec())
}

/// A file's extent in the image, as `esf::ReadAt`, over the cache: a read
/// of bytes not fetched yet is `WouldBlock`.
struct Window<'a> {
    sparse: &'a mut Sparse,
    base: u64,
    len: u64,
}

impl esf::ReadAt for Window<'_> {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
        if off + buf.len() as u64 > self.len {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        iso::Source::read_at(self.sparse, self.base + off, buf)
    }
}

/// Zone reads finished and not yet handed over: (zone, its bytes).
type Done = Rc<RefCell<VecDeque<(usize, std::io::Result<Vec<u8>>)>>>;

/// A deferred world's zones (`stream::ZoneSource`): each read is a slice of
/// the image, finished on a later frame.
pub struct WebZones {
    file: web_sys::File,
    base: u64,
    done: Done,
}

impl crate::stream::ZoneSource for WebZones {
    fn request(&mut self, zi: usize, off: u64, len: usize) {
        let (file, done, at) = (self.file.clone(), self.done.clone(), self.base + off);
        wasm_bindgen_futures::spawn_local(async move {
            let r = read_blob(&file, at, len).await.map_err(std::io::Error::other);
            done.borrow_mut().push_back((zi, r));
        });
    }

    fn poll(&mut self) -> Option<(usize, std::io::Result<Vec<u8>>)> {
        self.done.borrow_mut().pop_front()
    }
}

impl WebDisc {
    pub async fn open(file: web_sys::File) -> Result<Self, String> {
        let len = file.size() as u64;
        let mut d = WebDisc { file, len, entries: Vec::new(), sparse: Sparse::new(len) };
        d.entries = d.drive(iso::list).await.map_err(|e| format!("not an EQOA disc image: {e}"))?;
        Ok(d)
    }

    /// Another reader of the same image, with a cache of its own (a task
    /// that reads while others do).
    pub fn copy(&self) -> WebDisc {
        WebDisc { file: self.file.clone(), len: self.len, entries: self.entries.clone(), sparse: Sparse::new(self.len) }
    }

    /// The player's image (the music reads it as it plays, `bgm`).
    pub fn file(&self) -> web_sys::File {
        self.file.clone()
    }

    /// `len` bytes at `off` of the image.
    pub async fn read_at(&self, off: u64, len: usize) -> Result<Vec<u8>, String> {
        read_blob(&self.file, off, len).await
    }

    /// Runs `f` against a file's extent until it stops asking for bytes the
    /// cache lacks.
    async fn drive_window<T>(&mut self, e: &Entry, mut f: impl FnMut(&mut dyn esf::ReadAt) -> Result<T, esf::Error>) -> Result<T, String> {
        let (base, len) = (e.lba as u64 * iso::SECTOR, e.size as u64);
        loop {
            let r = f(&mut Window { sparse: &mut self.sparse, base, len });
            match r {
                Err(esf::Error::Io(x)) if x.kind() == std::io::ErrorKind::WouldBlock => {
                    for (off, n) in std::mem::take(&mut self.sparse.missing) {
                        let b = self.read_at(off, n as usize).await?;
                        self.sparse.fill(off, &b);
                    }
                }
                Err(x) => return Err(format!("{}: {x}", e.path)),
                Ok(v) => return Ok(v),
            }
        }
    }

    /// A container off the disc: a world (an OBJF whose first record is
    /// 0x8000) with its zones left there (`Container::open_deferred`, as
    /// `stream::open_file` opens one natively), anything else whole.
    pub async fn container(&mut self, name: &str) -> Result<esf::Container, String> {
        let e = self.find(name).ok_or_else(|| format!("{name}: not on the disc"))?.clone();
        let world = self
            .drive_window(&e, |w| {
                let mut h = [0u8; 24];
                if w.len() < 36 {
                    return Ok(false);
                }
                w.read_at(0, &mut h).map_err(esf::Error::Io)?;
                if &h[..4] != b"FJBO" {
                    return Ok(false);
                }
                let start = i64::from_le_bytes(h[16..24].try_into().unwrap());
                let mut r = [0u8; 2];
                if start < 0 || start as u64 + 2 > w.len() {
                    return Ok(false);
                }
                w.read_at(start as u64, &mut r).map_err(esf::Error::Io)?;
                Ok(u16::from_le_bytes(r) == 0x8000)
            })
            .await?;
        if !world {
            return esf::Container::open(self.read(name).await?).map_err(|x| format!("{name}: {x}"));
        }
        let c = self.drive_window(&e, |w| esf::Container::open_deferred(w)).await?;
        // what the walk over the headers fetched is not needed again
        self.sparse = Sparse::new(self.len);
        Ok(c)
    }

    /// `stream::open_world` in a page (`stream::set_world_opener`): on a
    /// reader of its own, the world file opened with its zones left on the
    /// disc and the zones round the arrival read, as the first world was
    /// (`container`, `fill`).
    pub fn world_opener(&self) -> crate::stream::Opener {
        let (file, len, entries) = (self.file.clone(), self.len, self.entries.clone());
        Box::new(move |name, x, z| {
            let out: crate::stream::Arrival = Default::default();
            // off the disc of the art drawn when that disc has the world
            // (`era::world_era`: Tunaria off the 2002 disc), and saying so
            let d2 = (crate::era::world_era(name) == esf::Era::Y2002).then(super::disc_2002_copy).flatten();
            let frontiers = WebDisc { file: file.clone(), len, entries: entries.clone(), sparse: Sparse::new(len) };
            let (o, name) = (out.clone(), name.to_string());
            wasm_bindgen_futures::spawn_local(async move {
                let t = web_time::Instant::now();
                let r = async {
                    // the 2002 disc's world where it places the arrival
                    // (`super::places`), else Frontiers'
                    let mut off_2002 = None;
                    if let Some(mut d2) = d2 {
                        let c = d2.container(&name).await?;
                        match super::places(&c, x, z) {
                            true => off_2002 = Some((d2, c)),
                            false => super::log(&format!("art: the 2002 disc's {name} places no zone at the arrival: Frontiers' world instead")),
                        }
                    }
                    let era = if off_2002.is_some() { esf::Era::Y2002 } else { esf::Era::Frontiers };
                    let mut frontiers = frontiers;
                    let (d, c) = match off_2002 {
                        Some(both) => both,
                        None => {
                            let c = frontiers.container(&name).await?;
                            (frontiers.copy(), c)
                        }
                    };
                    let mut lib = esf::Library::new();
                    let f = lib.add(&name, c);
                    let base = d.find(&name).ok_or("the world left the disc")?.lba as u64 * iso::SECTOR;
                    let mut zones = Vec::new();
                    for (zi, off, n) in crate::stream::fill_set(&lib, f, x, z) {
                        zones.push((zi, d.read_at(base + off, n).await?));
                    }
                    // the 2002 disc's world with Frontiers' as its twin, for
                    // its effect volumes and streamed audio (`stream::wants_twin`),
                    // its zones read with the arrival's
                    let (mut twin, mut twin_zones) = (None, Vec::new());
                    if crate::stream::wants_twin(era) {
                        let c = frontiers.container(&name).await?;
                        let tbase = frontiers.find(&name).ok_or("the world left the disc")?.lba as u64 * iso::SECTOR;
                        let t = lib.add("twin", c);
                        for &(zi, _) in &zones {
                            if let Some((off, n)) = lib.zone_range(t, zi) {
                                twin_zones.push((zi, frontiers.read_at(tbase + off, n).await?));
                            }
                        }
                        twin = Some(std::mem::replace(&mut lib.files[t].1, esf::Container::empty()));
                    }
                    super::log(&format!("{name} opened ({era:?}), {} zones read round the arrival ({} ms)", zones.len(), t.elapsed().as_millis()));
                    Ok::<_, String>(crate::stream::Opened { container: std::mem::replace(&mut lib.files[f].1, esf::Container::empty()), zones, era, twin, twin_zones })
                }
                .await;
                *o.borrow_mut() = Some(r);
            });
            out
        })
    }

    /// `stream::set_source_factory`'s argument: every deferred world's zones
    /// read as slices of this image.
    pub fn zone_sources(&self) -> crate::stream::Factory {
        let d = WebDisc { file: self.file.clone(), len: self.len, entries: self.entries.clone(), sparse: Sparse::new(0) };
        Box::new(move |lib, f| {
            // the world's own disc's: Tunaria off the 2002 disc (`era`)
            let name = &lib.files[f].0;
            let z = match lib.era_of(f) {
                esf::Era::Y2002 => super::disc_2002_copy().and_then(|d2| d2.zone_source(name)),
                esf::Era::Frontiers => d.zone_source(name),
            };
            z.map(|z| Box::new(z) as Box<dyn crate::stream::ZoneSource>)
        })
    }

    /// Where a deferred world's zones come from (`stream::set_source_factory`).
    pub fn zone_source(&self, name: &str) -> Option<WebZones> {
        let e = self.find(name)?;
        Some(WebZones { file: self.file.clone(), base: e.lba as u64 * iso::SECTOR, done: Default::default() })
    }

    /// `stream::fill`, awaited (`VIScene::StreamFillCache`): every zone a walk
    /// at (x, z) needs before it starts, read and installed.
    /// The twin of the 2002 disc's world in file `file`, off this image,
    /// Frontiers' (`esf::Library::set_twin`), and what its zones `zones` add
    /// (`stream::add_twin_zone`): Frontiers' effect volumes and streamed
    /// audio over the 2002 disc's Tunaria, as natively.
    pub async fn twin(&mut self, lib: &mut esf::Library, file: usize, zones: &[usize]) -> Result<(), String> {
        let name = lib.files[file].0.clone();
        let c = self.container(&name).await?;
        let t = lib.set_twin(file, Some(c), esf::Era::Frontiers).ok_or("no twin")?;
        let base = self.find(&name).ok_or_else(|| format!("{name}: not on the disc"))?.lba as u64 * iso::SECTOR;
        for &zi in zones {
            let Some((off, n)) = lib.zone_range(t, zi) else { continue };
            let b = self.read_at(base + off, n).await?;
            crate::stream::add_twin_zone(lib, file, zi, b);
        }
        Ok(())
    }

    pub async fn fill(&self, lib: &mut esf::Library, file: usize, x: f32, z: f32) -> Result<usize, String> {
        let name = lib.files[file].0.clone();
        let e = self.find(&name).ok_or_else(|| format!("{name}: not on the disc"))?.clone();
        let want = crate::stream::fill_set(lib, file, x, z);
        for &(zi, off, len) in &want {
            let b = self.read_at(e.lba as u64 * iso::SECTOR + off, len).await?;
            crate::stream::install(lib, file, zi, b)?;
        }
        Ok(want.len())
    }

    pub fn find(&self, path: &str) -> Option<&Entry> {
        let want = path.trim_start_matches(['/', '\\']).replace('\\', "/");
        if let Some(e) = self.entries.iter().find(|e| e.path.eq_ignore_ascii_case(&want)) {
            return Some(e);
        }
        if want.contains('/') {
            return None;
        }
        let mut named = self.entries.iter().filter(|e| e.path.rsplit('/').next().is_some_and(|n| n.eq_ignore_ascii_case(&want)));
        match (named.next(), named.next()) {
            (Some(e), None) => Some(e),
            _ => None,
        }
    }

    /// A whole file off the disc, by path (case ignored; a bare name matches
    /// a file anywhere when exactly one has it, as `iso::Iso::find`).
    pub async fn read(&self, path: &str) -> Result<Vec<u8>, String> {
        let e = self.find(path).ok_or_else(|| format!("{path}: not on the disc"))?.clone();
        self.read_at(e.lba as u64 * iso::SECTOR, e.size as usize).await
    }

    /// The program the disc starts, as its SYSTEM.CNF names it
    /// (`discs::boot_program`): which disc this is.
    pub async fn boot(&self) -> Option<String> {
        crate::discs::boot_program(&self.read("SYSTEM.CNF").await.ok()?)
    }

    /// Runs a synchronous walk against the cache, fetching what it lacks,
    /// until it stops asking.
    pub async fn drive<T>(&mut self, mut f: impl FnMut(&mut Sparse) -> Result<T, iso::Error>) -> Result<T, iso::Error> {
        loop {
            match f(&mut self.sparse) {
                Err(iso::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    for (off, n) in std::mem::take(&mut self.sparse.missing) {
                        let b = self.read_at(off, n as usize).await.map_err(|e| iso::Error::Io(std::io::Error::other(e)))?;
                        self.sparse.fill(off, &b);
                    }
                }
                r => return r,
            }
        }
    }
}
