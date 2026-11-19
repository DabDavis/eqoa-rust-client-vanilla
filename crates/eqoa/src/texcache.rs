//! A texture pack's files made ready for the GPU off the frame, kept in
//! memory within a budget and on the disk between runs (the owner's of
//! 2026-11-18: "for texture packs lets look at PCSX2 how they do texture packs
//! like precaching and we can make a better version"; PCSX2's own, read
//! against this, the maintainer's notes).
//!
//! - **Prepared** (`prepare`): a file's whole level chain, as `texpack`'s
//!   `with_levels` makes a stand-in's (its own levels while each is half the
//!   one before, then each half the last, down to one texel), less the
//!   levels past the pack's largest side (`render::PackLimits::max_side`).
//!   A surface's stand-in is a run of it (`Prepared::slice`), so one file
//!   read serves every surface it stands in for. A DDS of BCn blocks whose
//!   chain goes to one texel is kept as its blocks, never decoded, on a card
//!   that draws BCn (`render::packed::compressed_textures`): its levels are
//!   slices of the file mapped read-only. A PNG (and any other DDS) is
//!   decoded and its chain made, then, with Pack Compression on (or Auto, a
//!   pack larger than the card's budget), each level compressed to BC3 (a
//!   quarter of the RGBA: texpresso's range fit; BC3's alpha block keeps the
//!   GS's 0x80 exact).
//! - **On the disk** (`Disk`): a prepared file that cost a decode is written
//!   to the cache folder (`dir`: `eqoa-rs/texpack` in the platform's cache
//!   directory, never the pack's folder), keyed by the file's path, size and
//!   time of change and the form wanted (largest side, compressed or not); a
//!   later run maps it read-only (`memmap2`) instead of decoding the PNG
//!   again. Mapped, its bytes are the system's page cache: every client
//!   open on the computer reads the one copy. One client writes at a time
//!   (`.writer.lock`, held for the run by the first that writes); a file is
//!   written beside its name and renamed over it, so a reader never sees one
//!   half written, and one that does not read whole is made again.
//! - **In memory** (`Ram`): the prepared files by path, within a budget
//!   (`ram_budget`), a priority each: drawn, the zone's coming in, the
//!   precache's. A file asked for lets go of the least recently used of
//!   those no more wanted than it; the precache's stop where the budget is
//!   full of what was drawn.
//! - **The workers** (`Queue`): a pool of threads taking the most wanted
//!   first, so a texture a frame is drawing jumps every zone's and the
//!   precache's.

use render::packed::{Bytes, Format, Level, Packed};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// A pack's file made ready (`prepare`): its level chain from level `skip`
/// of the file's base, in one format.
#[derive(Debug)]
pub struct Prepared {
    pub packed: Arc<Packed>,
    /// The file's base's size (its level 0).
    pub base: (u32, u32),
    /// How many of the file's first levels the chain leaves out (larger than
    /// the pack's largest side).
    pub skip: u32,
    /// How it came: read off the disk cache, kept as the file's own blocks,
    /// or decoded (and compressed) now.
    pub how: How,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    Cached,
    Blocks,
    Decoded,
}

impl Prepared {
    pub fn bytes(&self) -> u64 {
        self.packed.bytes()
    }

    /// Whether its bytes are a mapped file's (the page cache's, shared).
    pub fn shared(&self) -> bool {
        self.packed.levels.iter().all(|l| l.data.shared())
    }

    /// The stand-in for a surface that keeps `want` levels of a stand-in of
    /// this base (`texpack::level_count`), at most `max_side` a side: the
    /// file's levels from the first within it, `want` less those dropped
    /// (one at least), as `texpack::fit(with_levels(..))` gives them.
    /// `None` where the chain does not hold them (made for a larger side).
    pub fn slice(&self, want: usize, max_side: u32) -> Option<Packed> {
        let k = crate::texpack::over(self.base, max_side) as u32;
        let start = k.checked_sub(self.skip)? as usize;
        let count = want.saturating_sub(k as usize).max(1);
        let levels = self.packed.levels.get(start..start + count)?.to_vec();
        Some(Packed { format: self.packed.format, levels, alpha_min: self.packed.alpha_min })
    }
}

/// What a file is made ready as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Want {
    /// The pack's largest side (`render::PackLimits::max_side`).
    pub max_side: u32,
    /// A decoded one compressed to BC3 (Pack Compression).
    pub compress: bool,
    /// The card draws BCn: a DDS's blocks kept as they are.
    pub bc: bool,
}

/// A level chain's levels as RGBA8 (BCn decoded as `texpack::read_dds`
/// decodes it).
pub fn images(p: &Packed) -> Vec<esf::Image> {
    p.levels.iter().map(|l| crate::texpack::image_of(l.width, l.height, rgba_of(p.format, l))).collect()
}

/// A level's texels as RGBA8.
pub fn rgba_of(format: Format, l: &Level) -> Vec<u8> {
    if !format.compressed() {
        return l.data.to_vec();
    }
    let (w, h) = (l.width, l.height);
    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
    let bb = format.block_bytes() as usize;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let mut px = [0u8; 64];
    for by in 0..bh {
        for bx in 0..bw {
            let src = &l.data[((by * bw + bx) as usize) * bb..][..bb];
            match format {
                Format::Bc1 => bcdec_rs::bc1(src, &mut px, 16),
                Format::Bc2 => bcdec_rs::bc2(src, &mut px, 16),
                Format::Bc3 => bcdec_rs::bc3(src, &mut px, 16),
                _ => bcdec_rs::bc7(src, &mut px, 16),
            }
            for y in 0..4 {
                for x in 0..4 {
                    let (tx, ty) = (bx * 4 + x, by * 4 + y);
                    if tx < w && ty < h {
                        let d = ((ty * w + tx) * 4) as usize;
                        rgba[d..d + 4].copy_from_slice(&px[((y * 4 + x) * 4) as usize..][..4]);
                    }
                }
            }
        }
    }
    rgba
}

/// The least alpha of a chain's texels.
fn alpha_min(format: Format, levels: &[Level]) -> u8 {
    levels.iter().map(|l| rgba_of(format, l).chunks_exact(4).map(|p| p[3]).min().unwrap_or(255)).min().unwrap_or(255)
}

/// `levels` (a file's, the base first) as a whole chain: its own while each
/// is half the one before, then each half the last (`texpack::half`), down
/// to one texel a side.
pub fn whole_chain(mut levels: Vec<esf::Image>) -> Vec<esf::Image> {
    let base = (levels[0].width, levels[0].height);
    let chain = 32 - base.0.max(base.1).leading_zeros() as usize;
    let mut own = 1;
    while own < levels.len() && (levels[own].width, levels[own].height) == ((base.0 >> own).max(1), (base.1 >> own).max(1)) {
        own += 1;
    }
    levels.truncate(own.min(chain));
    while levels.len() < chain {
        let next = crate::texpack::half(levels.last().unwrap());
        levels.push(next);
    }
    levels
}

/// A level compressed to BC3 (texpresso's range fit: the quick one; its
/// alpha block keeps a constant alpha such as the GS's 0x80 exactly).
#[cfg(not(target_arch = "wasm32"))]
pub fn bc3(l: &esf::Image) -> Vec<u8> {
    let f = texpresso::Format::Bc3;
    let mut out = vec![0u8; f.compressed_size(l.width as usize, l.height as usize)];
    let params = texpresso::Params { algorithm: texpresso::Algorithm::RangeFit, ..Default::default() };
    f.compress(&l.rgba, l.width as usize, l.height as usize, params, &mut out);
    out
}

/// A page has neither the encoder nor a card that draws BCn
/// (`from_images` never asks it there).
#[cfg(target_arch = "wasm32")]
pub fn bc3(_l: &esf::Image) -> Vec<u8> {
    unreachable!("no BCn in a page")
}

/// A file read whole (a page has no files to map).
#[cfg(target_arch = "wasm32")]
fn map_file(path: &Path) -> Result<Arc<dyn AsRef<[u8]> + Send + Sync>, String> {
    std::fs::read(path).map(|v| Arc::new(v) as Arc<dyn AsRef<[u8]> + Send + Sync>).map_err(|e| e.to_string())
}

/// A file read whole, mapped read-only where it can be (natively), else
/// read into memory.
#[cfg(not(target_arch = "wasm32"))]
fn map_file(path: &Path) -> Result<Arc<dyn AsRef<[u8]> + Send + Sync>, String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    // SAFETY: mapped read-only; a file changed under the mapping changes its
    // bytes, never its length as mapped, and every reader of it checks
    // lengths and sizes before it reads (`Packed::valid`, `dds_layout`)
    match unsafe { memmap2::Mmap::map(&f) } {
        Ok(m) => Ok(Arc::new(m)),
        Err(_) => std::fs::read(path).map(|v| Arc::new(v) as Arc<dyn AsRef<[u8]> + Send + Sync>).map_err(|e| e.to_string()),
    }
}

/// A pack's file made ready (module doc): off the disk cache where it has
/// it, else as a DDS's own blocks (`want.bc`, a chain to one texel, its base
/// within the largest side a whole number of blocks), else decoded, its
/// chain made, compressed to BC3 if `want.compress` (and `want.bc`), and
/// written to the disk cache.
pub fn prepare(path: &Path, want: Want, disk: Option<&Disk>) -> Result<Prepared, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if let Some(p) = disk.and_then(|d| d.load(path, &meta, want)) {
        return Ok(p);
    }
    let dds = crate::texpack::loader(path) == Some("dds");
    let levels = if dds {
        let bytes = map_file(path)?;
        if want.bc
            && let Some(p) = dds_blocks(&bytes, want.max_side)
        {
            return Ok(p);
        }
        crate::texpack::read_dds(&(*bytes).as_ref())?
    } else {
        vec![crate::texpack::read_png(&std::fs::read(path).map_err(|e| e.to_string())?)?]
    };
    let p = from_images(levels, want);
    if let Some(d) = disk {
        d.store(path, &meta, want, &p);
    }
    Ok(p)
}

/// A decoded file's levels made ready (`prepare`).
pub fn from_images(levels: Vec<esf::Image>, want: Want) -> Prepared {
    let base = (levels[0].width, levels[0].height);
    let mut chain = whole_chain(levels);
    let skip = crate::texpack::over(base, want.max_side).min(chain.len() - 1);
    chain.drain(..skip);
    let amin = chain.iter().flat_map(|l| l.rgba.chunks_exact(4).map(|p| p[3])).min().unwrap_or(255);
    let compress = cfg!(not(target_arch = "wasm32")) && want.compress && want.bc && chain[0].width % 4 == 0 && chain[0].height % 4 == 0;
    let (format, levels) = if compress {
        (Format::Bc3, chain.iter().map(|l| Level { width: l.width, height: l.height, data: Bytes::owned(bc3(l)) }).collect())
    } else {
        (Format::Rgba8, chain.into_iter().map(|l| Level { width: l.width, height: l.height, data: Bytes::owned(l.rgba) }).collect())
    };
    Prepared { packed: Arc::new(Packed { format, levels, alpha_min: amin }), base, skip: skip as u32, how: How::Decoded }
}

/// A DDS of BCn blocks kept as its blocks (slices of `bytes`): only where
/// its own chain goes down to one texel and the first level within
/// `max_side` is a whole number of blocks; else `None` (decoded instead).
fn dds_blocks(bytes: &Arc<dyn AsRef<[u8]> + Send + Sync>, max_side: u32) -> Option<Prepared> {
    use crate::texpack::DdsFmt;
    let (fmt, levels) = crate::texpack::dds_layout((**bytes).as_ref()).ok()?;
    let format = match fmt {
        DdsFmt::Bc(1) => Format::Bc1,
        DdsFmt::Bc(2) => Format::Bc2,
        DdsFmt::Bc(3) => Format::Bc3,
        DdsFmt::Bc(7) => Format::Bc7,
        _ => return None,
    };
    let base = (levels[0].width, levels[0].height);
    let chain = 32 - base.0.max(base.1).leading_zeros() as usize;
    if levels.len() < chain {
        return None;
    }
    let skip = crate::texpack::over(base, max_side).min(chain - 1);
    let mut out = Vec::new();
    for l in &levels[skip..chain] {
        let len = format.level_bytes(l.width, l.height) as usize;
        // a padded row (a pitch the header gave) copied packed
        let data = if l.row == format.row_bytes(l.width) {
            Bytes::slice_of(bytes.clone(), l.at, len)?
        } else {
            let src = (**bytes).as_ref();
            let rb = format.row_bytes(l.width) as usize;
            Bytes::owned((0..l.rows as usize).flat_map(|r| src[l.at + r * l.row as usize..][..rb].iter().copied()).collect())
        };
        out.push(Level { width: l.width, height: l.height, data });
    }
    let packed = Packed { format, alpha_min: alpha_min(format, &out), levels: out };
    packed.valid().then(|| Prepared { packed: Arc::new(packed), base, skip: skip as u32, how: How::Blocks })
}

// ---- the disk cache --------------------------------------------------------------

/// The cache files' first bytes, and the form they are in.
const MAGIC: &[u8; 4] = b"EQTC";
const VERSION: u32 = 1;
const HEADER: usize = 56;
const LEVEL: usize = 24;

/// The cache folder (module doc): `$EQOA_TEXCACHE`; else `texpack` beside
/// `$EQOA_CACHE` (an Android app's cache); else `eqoa-rs/texpack` in the
/// platform's cache directory (`$XDG_CACHE_HOME` or `~/.cache`,
/// `~/Library/Caches` on a Mac, `%LOCALAPPDATA%` on Windows). None in a
/// page, and in a test unless it gives one.
#[cfg(all(not(test), not(target_arch = "wasm32")))]
pub fn dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("EQOA_TEXCACHE") {
        return Some(PathBuf::from(d));
    }
    if let Some(d) = std::env::var_os("EQOA_CACHE") {
        return Some(PathBuf::from(d).join("texpack"));
    }
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".cache")))
    };
    base.map(|b| b.join("eqoa-rs").join("texpack"))
}

#[cfg(all(not(test), target_arch = "wasm32"))]
pub fn dir() -> Option<PathBuf> {
    None
}

#[cfg(test)]
pub fn dir() -> Option<PathBuf> {
    testing::DIR.with(|d| d.borrow().clone())
}

#[cfg(test)]
pub mod testing {
    use std::cell::RefCell;
    use std::path::PathBuf;
    thread_local! {
        pub static DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }
    /// This thread's cache folder (`dir`).
    pub fn set_dir(d: Option<PathBuf>) {
        DIR.with(|x| *x.borrow_mut() = d);
    }
}

/// A file's time of change in nanoseconds since 1970 (0 where the system
/// does not say).
fn mtime(meta: &std::fs::Metadata) -> u64 {
    meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos() as u64)
}

/// The disk cache in its folder (module doc).
pub struct Disk {
    dir: PathBuf,
    /// The writer's lock, once this client has it (`Some(None)`: tried and
    /// another client has it).
    writer: Mutex<Option<Option<std::fs::File>>>,
    /// Files written and read this run.
    pub written: AtomicU64,
    pub read: AtomicU64,
}

impl Disk {
    /// The cache in `dir`, made if it is not there.
    pub fn open(dir: PathBuf) -> Option<Disk> {
        std::fs::create_dir_all(&dir).ok()?;
        Some(Disk { dir, writer: Mutex::new(None), written: AtomicU64::new(0), read: AtomicU64::new(0) })
    }

    /// The cache's file for a pack's file at `path` in the form `want`: its
    /// path's XXH3, and `-bc` for the compressed form, so that both forms
    /// are kept side by side (windows opening and closing move Auto's
    /// choice, `texpack`: neither form is written over the other).
    pub fn file_for(&self, path: &Path, want: Want) -> PathBuf {
        let form = if want.compress && want.bc { "-bc" } else { "" };
        self.dir.join(format!("{:016x}{form}.eqtc", path_hash(path)))
    }

    /// Whether this client writes the cache: the writer's lock taken, held
    /// for the run (`File::try_lock`, which the system lets go however the
    /// program ends); a client that finds it held never writes and asks
    /// again only after a minute.
    pub fn writer(&self) -> bool {
        let mut w = self.writer.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(Some(_)) = w.as_ref() {
            return true;
        }
        if w.is_some() {
            return false;
        }
        let got = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(self.dir.join(".writer.lock")).ok().filter(|f| f.try_lock().is_ok());
        let ok = got.is_some();
        *w = Some(got);
        ok
    }

    /// The prepared file the cache holds for `path` (as it is now, `meta`)
    /// in the form `want`, mapped read-only; `None` where it has none, an
    /// older one, one of another form, or one that does not read whole.
    pub fn load(&self, path: &Path, meta: &std::fs::Metadata, want: Want) -> Option<Prepared> {
        let file = self.file_for(path, want);
        let bytes = map_file(&file).ok()?;
        let p = parse(&bytes, path_hash(path), meta.len(), mtime(meta), want)?;
        self.read.fetch_add(1, Ordering::Relaxed);
        Some(p)
    }

    /// `p` (made from `path` as it is now, `meta`, in the form `want`)
    /// written to the cache, if this client is its writer: beside its name,
    /// then renamed over it. Whether it was.
    pub fn store(&self, path: &Path, meta: &std::fs::Metadata, want: Want, p: &Prepared) -> bool {
        if !self.writer() {
            return false;
        }
        let file = self.file_for(path, want);
        let tmp = file.with_extension(format!("eqtc.{}.part", std::process::id()));
        let go = || -> std::io::Result<()> {
            use std::io::Write;
            let mut w = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
            w.write_all(&encode(p, path_hash(path), meta.len(), mtime(meta), want))?;
            for l in &p.packed.levels {
                w.write_all(&l.data)?;
            }
            // (no fsync: a file is checked whole as it is read, and a cache
            // lost to a crash is only made again)
            w.into_inner().map_err(|e| e.into_error())?;
            std::fs::rename(&tmp, &file)
        };
        match go() {
            Ok(()) => {
                self.written.fetch_add(1, Ordering::Relaxed);
                true
            }
            Err(_) => {
                let _ = std::fs::remove_file(&tmp);
                false
            }
        }
    }

    /// The bytes and count of the cache's files.
    pub fn size(&self) -> (u64, usize) {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return (0, 0) };
        rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "eqtc")).filter_map(|e| e.metadata().ok()).fold((0, 0), |(b, n), m| (b + m.len(), n + 1))
    }

    /// Every cache file taken away (Rebuild Pack Cache): how many. One
    /// another client has mapped stays on Windows until it lets go; it is
    /// made again when it no longer matches anyway.
    pub fn clear(&self) -> usize {
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return 0 };
        rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "eqtc") || p.to_string_lossy().ends_with(".part")).filter(|p| std::fs::remove_file(p).is_ok()).count()
    }
}

fn path_hash(path: &Path) -> u64 {
    xxhash_rust::xxh3::xxh3_64(path.to_string_lossy().as_bytes())
}

/// A cache file's header and level table (`Disk::store`), its levels'
/// bytes after it in order.
fn encode(p: &Prepared, hash: u64, len: u64, time: u64, want: Want) -> Vec<u8> {
    let n = p.packed.levels.len();
    let mut h = Vec::with_capacity(HEADER + n * LEVEL);
    h.extend_from_slice(MAGIC);
    h.extend_from_slice(&VERSION.to_le_bytes());
    h.extend_from_slice(&len.to_le_bytes());
    h.extend_from_slice(&time.to_le_bytes());
    h.extend_from_slice(&hash.to_le_bytes());
    h.extend_from_slice(&want.max_side.to_le_bytes());
    h.extend_from_slice(&[p.packed.format.code(), p.packed.alpha_min, want.compress as u8, 0]);
    h.extend_from_slice(&p.base.0.to_le_bytes());
    h.extend_from_slice(&p.base.1.to_le_bytes());
    h.extend_from_slice(&p.skip.to_le_bytes());
    h.extend_from_slice(&(n as u32).to_le_bytes());
    let mut at = (HEADER + n * LEVEL) as u64;
    for l in &p.packed.levels {
        h.extend_from_slice(&l.width.to_le_bytes());
        h.extend_from_slice(&l.height.to_le_bytes());
        h.extend_from_slice(&at.to_le_bytes());
        h.extend_from_slice(&(l.data.len() as u64).to_le_bytes());
        at += l.data.len() as u64;
    }
    h
}

/// A cache file read (`encode`'s), its levels slices of `bytes`: `None`
/// unless it is this version's, of the file as it is now (`hash`, `len`,
/// `time`), in the form `want`, and whole.
fn parse(bytes: &Arc<dyn AsRef<[u8]> + Send + Sync>, hash: u64, len: u64, time: u64, want: Want) -> Option<Prepared> {
    let b = (**bytes).as_ref();
    let u32_at = |at: usize| Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?));
    let u64_at = |at: usize| Some(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?));
    if b.get(..4)? != MAGIC || u32_at(4)? != VERSION || u64_at(8)? != len || u64_at(16)? != time || u64_at(24)? != hash || u32_at(32)? != want.max_side {
        return None;
    }
    let (format, amin, compress) = (Format::from_code(b[36])?, b[37], b[38] != 0);
    // the form: compressed as asked (a BC3 only where the card draws it)
    if compress != want.compress || (format.compressed() && !want.bc) {
        return None;
    }
    let (base, skip, n) = ((u32_at(40)?, u32_at(44)?), u32_at(48)?, u32_at(52)? as usize);
    if n == 0 || n > 32 {
        return None;
    }
    let mut levels = Vec::with_capacity(n);
    for k in 0..n {
        let at = HEADER + k * LEVEL;
        let (w, h, off, size) = (u32_at(at)?, u32_at(at + 4)?, u64_at(at + 8)? as usize, u64_at(at + 16)? as usize);
        levels.push(Level { width: w, height: h, data: Bytes::slice_of(bytes.clone(), off, size)? });
    }
    let packed = Packed { format, levels, alpha_min: amin };
    let first = (base.0 >> skip).max(1) == packed.levels[0].width && (base.1 >> skip).max(1) == packed.levels[0].height;
    (first && packed.valid()).then(|| Prepared { packed: Arc::new(packed), base, skip, how: How::Cached })
}

// ---- in memory ---------------------------------------------------------------------

/// How much a prepared file is wanted, the least first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Prio {
    /// The precache's: every file of the pack, as there is room.
    Background,
    /// A zone's coming in (`texpack::prefetch`).
    Zone,
    /// A texture being made for a frame.
    Drawn,
}

struct RamEntry {
    prep: Arc<Prepared>,
    bytes: u64,
    prio: Prio,
    stamp: u64,
}

/// The prepared files kept in memory by key (a file's path), within a
/// budget (module doc).
pub struct Ram<K> {
    entries: HashMap<K, RamEntry>,
    used: u64,
    clock: u64,
    /// How many were let go for room, and how many the budget refused.
    pub evicted: u64,
    pub refused: u64,
}

impl<K> Default for Ram<K> {
    fn default() -> Self {
        Ram { entries: HashMap::new(), used: 0, clock: 0, evicted: 0, refused: 0 }
    }
}

impl<K: std::hash::Hash + Eq + Clone> Ram<K> {
    /// The prepared file under `key`, its use stamped and its priority
    /// raised to `prio` if it was less.
    pub fn get(&mut self, key: &K, prio: Prio) -> Option<Arc<Prepared>> {
        self.clock += 1;
        let e = self.entries.get_mut(key)?;
        e.stamp = self.clock;
        e.prio = e.prio.max(prio);
        Some(e.prep.clone())
    }

    pub fn contains(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }

    /// `prep` kept under `key` at `prio`: room made in `budget` by letting go
    /// of the least recently used of those no more wanted than it (the least
    /// wanted first); where that is not enough a drawn one is kept anyway
    /// (a frame wants it now) and any other refused. Whether it was kept.
    pub fn insert(&mut self, key: K, prep: Arc<Prepared>, prio: Prio, budget: u64) -> bool {
        self.remove(&key);
        let bytes = prep.bytes();
        if self.used + bytes > budget {
            let mut order: Vec<(Prio, u64, K)> = self.entries.iter().filter(|(_, e)| e.prio <= prio).map(|(k, e)| (e.prio, e.stamp, k.clone())).collect();
            order.sort_by_key(|(p, s, _)| (*p, *s));
            let freeable: u64 = order.iter().map(|(_, _, k)| self.entries[k].bytes).sum();
            if prio != Prio::Drawn && self.used - freeable + bytes > budget {
                self.refused += 1;
                return false;
            }
            for (_, _, k) in order {
                if self.used + bytes <= budget {
                    break;
                }
                self.remove(&k);
                self.evicted += 1;
            }
        }
        self.clock += 1;
        self.used += bytes;
        self.entries.insert(key, RamEntry { prep, bytes, prio, stamp: self.clock });
        true
    }

    pub fn remove(&mut self, key: &K) -> bool {
        match self.entries.remove(key) {
            Some(e) => {
                self.used -= e.bytes;
                true
            }
            None => false,
        }
    }

    pub fn used(&self) -> u64 {
        self.used
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The bytes of those that are a mapped file's (the page cache's,
    /// shared with every client).
    pub fn shared(&self) -> u64 {
        self.entries.values().filter(|e| e.prep.shared()).map(|e| e.bytes).sum()
    }

    /// How many are compressed (BCn).
    pub fn compressed(&self) -> usize {
        self.entries.values().filter(|e| e.prep.packed.format.compressed()).count()
    }

}

/// The system's memory, where it says it.
pub fn system_memory() -> Option<u64> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let s = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = s.lines().find(|l| l.starts_with("MemTotal:"))?.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb * 1024)
    }
    #[cfg(target_os = "macos")]
    {
        let mut v: u64 = 0;
        let mut n = std::mem::size_of::<u64>();
        // SAFETY: hw.memsize is a u64, written into `v` of that size
        let r = unsafe { libc::sysctlbyname(c"hw.memsize".as_ptr(), (&raw mut v).cast(), &raw mut n, std::ptr::null_mut(), 0) };
        (r == 0).then_some(v)
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        // SAFETY: a zeroed MEMORYSTATUSEX with its length set, filled by the call
        let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        (unsafe { GlobalMemoryStatusEx(&mut m) } != 0).then_some(m.ullTotalPhys)
    }
    #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos", windows)))]
    {
        None
    }
}

/// The memory a pack's prepared files may take: an eighth of the system's,
/// 256 MB to 2 GB (1 GB where it does not say), split between the game
/// windows open (`sharers`: a mapped file's bytes are one copy for all of
/// them, a decoded one's each window's own), 256 MB at least.
/// `EQOA_PACK_RAM_MB` sets it.
pub fn ram_budget(sharers: u64) -> u64 {
    if let Some(mb) = std::env::var("EQOA_PACK_RAM_MB").ok().and_then(|v| v.parse::<u64>().ok()) {
        return mb << 20;
    }
    let all = system_memory().map_or(1 << 30, |m| (m / 8).clamp(256 << 20, 2 << 30));
    (all / sharers.max(1)).max(256 << 20)
}

// ---- the workers -------------------------------------------------------------------

/// Jobs by how much each is wanted, the most wanted taken first (`pop`),
/// and in the order they came within a priority.
pub struct Queue<J> {
    inner: Mutex<(Vec<VecDeque<J>>, bool)>,
    cv: Condvar,
}

impl<J> Default for Queue<J> {
    fn default() -> Self {
        Queue { inner: Mutex::new(((0..3).map(|_| VecDeque::new()).collect(), false)), cv: Condvar::new() }
    }
}

impl<J> Queue<J> {
    pub fn push(&self, job: J, prio: Prio) {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        q.0[prio as usize].push_back(job);
        self.cv.notify_one();
    }

    /// The most wanted job, waiting for one; `None` once closed.
    pub fn pop(&self) -> Option<(J, Prio)> {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if q.1 {
                return None;
            }
            for p in [Prio::Drawn, Prio::Zone, Prio::Background] {
                if let Some(j) = q.0[p as usize].pop_front() {
                    return Some((j, p));
                }
            }
            q = self.cv.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }

    /// The most wanted job if there is one now.
    #[cfg(test)]
    pub fn try_pop(&self) -> Option<(J, Prio)> {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        [Prio::Drawn, Prio::Zone, Prio::Background].into_iter().find_map(|p| q.0[p as usize].pop_front().map(|j| (j, p)))
    }

    /// The jobs of `prio` dropped (the precache's, once memory is full).
    pub fn drop_all(&self, prio: Prio) -> usize {
        let mut q = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let n = q.0[prio as usize].len();
        q.0[prio as usize].clear();
        n
    }

    #[cfg(test)]
    pub fn len(&self, prio: Prio) -> usize {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).0[prio as usize].len()
    }

    /// No more jobs: every worker's `pop` ends.
    pub fn close(&self) {
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).1 = true;
        self.cv.notify_all();
    }
}

/// The calling worker made less urgent than the game's thread (Linux and
/// Android: a thread's nice of 10), so that decoding a pack never takes a
/// frame's time from the game on a machine with few cores.
pub fn lower_priority() {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // SAFETY: setpriority on the calling thread (who 0 is the caller's
        // own task on Linux), no memory touched
        unsafe {
            libc::setpriority(libc::PRIO_PROCESS, 0, 10);
        }
    }
}

/// How many workers a pack reads with: the processors less one for the
/// game, 1 to 4.
pub fn workers() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(1)).clamp(1, 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> esf::Image {
        let rgba = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).flat_map(|(x, y)| f(x, y)).collect();
        crate::texpack::image_of(w, h, rgba)
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("eqoa-texcache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    const WANT: Want = Want { max_side: 1024, compress: false, bc: false };

    /// A surface's run of a prepared chain is the levels `texpack`'s
    /// `fit(with_levels(..))` gives it, texel for texel: at its size, past
    /// the largest side, wanting more levels than the file has and fewer.
    #[test]
    fn a_slice_is_what_fit_with_levels_gives() {
        let base = img(256, 128, |x, y| [(x * 7) as u8, (y * 3) as u8, (x ^ y) as u8, 0x80]);
        let own2 = vec![base.clone(), crate::texpack::half(&base)];
        for (max, want) in [(1024, 9), (1024, 3), (64, 9), (64, 2), (32, 1), (4, 9)] {
            let p = from_images(own2.clone(), Want { max_side: max, ..WANT });
            let got = images(&p.slice(want, max).unwrap());
            // texpack's own path: the surface's count taken as given
            let mut lv = own2.clone();
            while lv.len() < want {
                let h = crate::texpack::half(lv.last().unwrap());
                lv.push(h);
            }
            lv.truncate(want.max(1));
            let expect = crate::texpack::fit(lv, max);
            assert_eq!(got.len(), expect.len(), "max {max} want {want}");
            for (a, b) in got.iter().zip(&expect) {
                assert_eq!((a.width, a.height, &a.rgba), (b.width, b.height, &b.rgba), "max {max} want {want}");
            }
        }
        // the control: a chain made for a smaller side has no run for a
        // larger one's first level
        let small = from_images(own2, Want { max_side: 64, ..WANT });
        assert!(small.slice(9, 1024).is_none());
    }

    /// The disk cache: a file written is read back mapped, level for level;
    /// the file changed (its size or time), another form asked for, or the
    /// cache file cut short, and it is not (made again instead).
    #[test]
    fn the_disk_cache_keyed_by_size_time_and_form() {
        let t = temp("disk");
        let src = t.join("a.png");
        let pic = img(64, 32, |x, y| [x as u8, y as u8, 9, 0x80]);
        crate::texpack::write_png(&src, 64, 32, &pic.rgba).unwrap();
        let disk = Disk::open(t.join("cache")).unwrap();
        let p = prepare(&src, WANT, Some(&disk)).unwrap();
        assert_eq!(p.how, How::Decoded);
        assert_eq!(disk.written.load(Ordering::Relaxed), 1);
        let again = prepare(&src, WANT, Some(&disk)).unwrap();
        assert_eq!(again.how, How::Cached, "read off the cache");
        assert!(again.shared(), "mapped");
        let px = |p: &Prepared| images(&p.packed).into_iter().map(|i| (i.width, i.height, i.rgba)).collect::<Vec<_>>();
        assert_eq!(px(&again), px(&p));
        assert_eq!(again.packed.alpha_min, 0x80);
        // another form: made again
        assert_eq!(prepare(&src, Want { max_side: 32, ..WANT }, Some(&disk)).unwrap().how, How::Decoded);
        // the file changed: one byte longer (a new PNG), made again
        let pic2 = img(64, 32, |x, y| [x as u8, y as u8, 10, 0x80]);
        std::thread::sleep(std::time::Duration::from_millis(20));
        crate::texpack::write_png(&src, 64, 32, &pic2.rgba).unwrap();
        let p2 = prepare(&src, WANT, Some(&disk)).unwrap();
        assert_eq!(p2.how, How::Decoded, "the file changed");
        assert_eq!(images(&p2.packed)[0].rgba[2], 10);
        assert_eq!(prepare(&src, WANT, Some(&disk)).unwrap().how, How::Cached);
        // the compressed form kept beside the RGBA one, neither over the other
        let bc = Want { compress: true, bc: true, ..WANT };
        assert_eq!(prepare(&src, bc, Some(&disk)).unwrap().how, How::Decoded);
        assert_eq!(prepare(&src, bc, Some(&disk)).unwrap().how, How::Cached);
        assert_eq!(prepare(&src, WANT, Some(&disk)).unwrap().how, How::Cached, "the RGBA form still there");
        assert_eq!(disk.size().1, 2);
        let _ = std::fs::remove_file(disk.file_for(&src, bc));
        // cut short: not read
        let f = disk.file_for(&src, WANT);
        let len = std::fs::metadata(&f).unwrap().len();
        std::fs::OpenOptions::new().write(true).open(&f).unwrap().set_len(len - 1).unwrap();
        assert_eq!(prepare(&src, WANT, Some(&disk)).unwrap().how, How::Decoded, "a cache file cut short is made again");
        assert_eq!(disk.clear(), 1);
        assert_eq!(disk.size(), (0, 0));
        let _ = std::fs::remove_dir_all(&t);
    }

    /// One writer: a second cache on the same folder (another client) finds
    /// the lock held and writes nothing, but reads what the first wrote.
    #[test]
    fn one_writer_and_every_reader() {
        let t = temp("writer");
        let src = t.join("b.png");
        let pic = img(16, 16, |x, _| [x as u8, 0, 0, 0x80]);
        crate::texpack::write_png(&src, 16, 16, &pic.rgba).unwrap();
        let first = Disk::open(t.join("cache")).unwrap();
        let second = Disk::open(t.join("cache")).unwrap();
        assert!(first.writer());
        assert!(!second.writer(), "held by the first");
        let other = t.join("c.png");
        crate::texpack::write_png(&other, 16, 16, &pic.rgba).unwrap();
        prepare(&other, WANT, Some(&second)).unwrap();
        assert_eq!(second.written.load(Ordering::Relaxed), 0, "the second writes nothing");
        prepare(&src, WANT, Some(&first)).unwrap();
        assert_eq!(prepare(&src, WANT, Some(&second)).unwrap().how, How::Cached, "and reads the first's");
        let _ = std::fs::remove_dir_all(&t);
    }

    /// A PNG compressed: BC3, a quarter of the RGBA, its alpha of 0x80 kept
    /// exactly and its colours near; a base not a whole number of blocks is
    /// left RGBA, as is every one where the card has no BCn (the control).
    #[test]
    fn compressed_to_bc3() {
        let pic = img(128, 64, |x, y| [(x * 2) as u8, (y * 4) as u8, 100, 0x80]);
        let on = Want { compress: true, bc: true, ..WANT };
        let p = from_images(vec![pic.clone()], on);
        assert_eq!(p.packed.format, Format::Bc3);
        let rgba = from_images(vec![pic.clone()], WANT);
        assert_eq!(p.packed.levels[0].data.len() * 4, rgba.packed.levels[0].data.len());
        let back = images(&p.packed);
        assert!(back[0].rgba.chunks_exact(4).all(|px| px[3] == 0x80), "alpha exact");
        let err = back[0].rgba.iter().zip(&pic.rgba).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(err <= 12, "colours near: {err}");
        assert_eq!(from_images(vec![img(6, 6, |_, _| [0, 0, 0, 0x80])], on).packed.format, Format::Rgba8, "not whole blocks");
        assert_eq!(from_images(vec![pic], Want { bc: false, ..on }).packed.format, Format::Rgba8, "no BCn on the card");
    }

    /// A BC DDS with its chain to one texel is kept as its blocks (not
    /// decoded), the levels past the largest side left out; one without its
    /// whole chain is decoded (the control).
    #[test]
    fn a_bc_dds_kept_as_its_blocks() {
        let t = temp("dds");
        // a BC1 64 x 64 with 7 levels, each block one colour
        let mut body = Vec::new();
        for k in 0..7u32 {
            let s = (64u32 >> k).max(1);
            for _ in 0..(s.div_ceil(4) * s.div_ceil(4)) {
                body.extend_from_slice(&[0xFF, 0xFF, 0, 0, 0, 0, 0, 0]);
            }
        }
        let dds = |mips: u32, body: &[u8]| {
            let mut b = Vec::new();
            b.extend_from_slice(b"DDS ");
            let mut h = [0u32; 31];
            h[0] = 124;
            h[1] = 0x1007 | 0x0002_0000;
            h[2] = 64;
            h[3] = 64;
            h[6] = mips;
            h[18] = 32;
            h[19] = 4;
            h[20] = u32::from_le_bytes(*b"DXT1");
            for v in h {
                b.extend_from_slice(&v.to_le_bytes());
            }
            b.extend_from_slice(body);
            b
        };
        let whole = t.join("w.dds");
        std::fs::write(&whole, dds(7, &body)).unwrap();
        let p = prepare(&whole, Want { max_side: 32, compress: false, bc: true }, None).unwrap();
        assert_eq!((p.how, p.packed.format, p.skip, p.packed.size()), (How::Blocks, Format::Bc1, 1, (32, 32, 6)));
        assert!(p.shared(), "slices of the mapped file");
        assert_eq!(p.packed.alpha_min, 255);
        let short = t.join("s.dds");
        std::fs::write(&short, dds(3, &body)).unwrap();
        let q = prepare(&short, Want { max_side: 32, compress: false, bc: true }, None).unwrap();
        assert_eq!((q.how, q.packed.format), (How::Decoded, Format::Rgba8), "three levels of seven: decoded, its chain made");
        assert_eq!(prepare(&whole, Want { max_side: 32, compress: false, bc: false }, None).unwrap().packed.format, Format::Rgba8, "no BCn on the card");
        let _ = std::fs::remove_dir_all(&t);
    }

    fn prep(bytes: usize) -> Arc<Prepared> {
        let levels = vec![Level { width: 1, height: (bytes / 4) as u32, data: Bytes::owned(vec![0; bytes]) }];
        Arc::new(Prepared { packed: Arc::new(Packed { format: Format::Rgba8, levels, alpha_min: 0 }), base: (1, 1), skip: 0, how: How::Decoded })
    }

    /// The memory's budget: a drawn file lets go of the least recently used
    /// precached ones first, then zones', then drawn ones; a precache file
    /// never lets go of a drawn or zone one (refused instead), nor a zone's
    /// of a drawn one; a drawn one is kept even past the budget.
    #[test]
    fn memory_by_priority_then_use() {
        const MB: usize = 1 << 20;
        let budget = 4 * MB as u64;
        let mut ram: Ram<u32> = Ram::default();
        assert!(ram.insert(1, prep(MB), Prio::Background, budget));
        assert!(ram.insert(2, prep(MB), Prio::Background, budget));
        assert!(ram.insert(3, prep(MB), Prio::Zone, budget));
        assert!(ram.insert(4, prep(MB), Prio::Drawn, budget));
        ram.get(&1, Prio::Background);
        // a drawn one: 2 goes (the least recently used precached), not 1
        assert!(ram.insert(6, prep(MB), Prio::Drawn, budget));
        assert!(!ram.contains(&2) && ram.contains(&1) && ram.contains(&3));
        // a precached one can only let go of a precached one
        assert!(ram.insert(7, prep(MB), Prio::Background, budget), "1 goes for it");
        assert!(!ram.contains(&1));
        assert!(!ram.insert(8, prep(2 * MB), Prio::Background, budget), "refused: only 7 is as little wanted");
        assert!(ram.contains(&7));
        // a zone's lets go of precached and zones', never drawn
        assert!(!ram.insert(9, prep(4 * MB), Prio::Zone, budget));
        assert!(ram.insert(10, prep(2 * MB), Prio::Zone, budget));
        assert!(ram.contains(&4) && ram.contains(&6) && !ram.contains(&7) && !ram.contains(&3));
        // a drawn one past the budget is kept anyway
        assert!(ram.insert(11, prep(8 * MB), Prio::Drawn, budget));
        assert!(ram.used() >= 8 * MB as u64);
        assert_eq!(ram.refused, 2);
    }

    /// The queue gives the most wanted first, in order within each.
    #[test]
    fn the_queue_takes_the_most_wanted_first() {
        let q: Queue<u32> = Queue::default();
        q.push(1, Prio::Background);
        q.push(2, Prio::Zone);
        q.push(3, Prio::Background);
        q.push(4, Prio::Drawn);
        q.push(5, Prio::Zone);
        let got: Vec<u32> = std::iter::from_fn(|| q.try_pop().map(|(j, _)| j)).collect();
        assert_eq!(got, vec![4, 2, 5, 1, 3]);
        q.push(6, Prio::Background);
        assert_eq!(q.drop_all(Prio::Background), 1);
        q.close();
        assert!(q.pop().is_none());
    }
}

/// `EQOA_BENCH_PACK=<replacements folder> cargo test --release -p eqoa
/// texcache::bench -- --ignored --nocapture`: every file of a pack made
/// ready on `workers()` threads as the frame's reader made it before
/// 2026-11-18 (decoded, its levels made, one thread) and as now: decoded and
/// written to a disk cache, then read off it (mapped), and compressed to
/// BC3 then read off it; each pass's time, and the memory the pack's
/// textures take in each form.
#[cfg(test)]
mod bench {
    use super::*;

    fn files(dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).filter(|p| crate::texpack::loader(p).is_some()).collect();
        v.sort();
        v
    }

    /// `f` over `paths` on `n` threads: the seconds, and the bytes it gave.
    fn pass(paths: &[PathBuf], n: usize, f: impl Fn(&Path) -> u64 + Sync) -> (f64, u64) {
        let t = std::time::Instant::now();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let total = AtomicU64::new(0);
        std::thread::scope(|s| {
            for _ in 0..n {
                s.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(p) = paths.get(i) else { break };
                        total.fetch_add(f(p), Ordering::Relaxed);
                    }
                });
            }
        });
        (t.elapsed().as_secs_f64(), total.load(Ordering::Relaxed))
    }

    #[test]
    #[ignore]
    fn bench() {
        let Some(dir) = std::env::var_os("EQOA_BENCH_PACK").map(PathBuf::from) else { return };
        let paths = files(&dir);
        let cache = std::env::temp_dir().join(format!("eqoa-texbench-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&cache);
        let n = workers();
        let mb = |b: u64| b as f64 / 1048576.0;
        println!("{} files, {} workers", paths.len(), n);
        let before = pass(&paths, 1, |p| {
            let l = crate::texpack::fit(crate::texpack::read(p).unwrap(), 2048);
            l.iter().map(|i| i.rgba.len() as u64).sum()
        });
        println!("before (one reader, decoded each run): {:.2} s, {:.0} MB RGBA (base levels only)", before.0, mb(before.1));
        let nodisk = pass(&paths, n, |p| prepare(p, Want { max_side: 2048, compress: false, bc: true }, None).unwrap().bytes());
        println!("now without a disk cache: {:.2} s ({n} workers, decode + whole chain)", nodisk.0);
        for (name, want) in [("RGBA", Want { max_side: 2048, compress: false, bc: true }), ("BC3", Want { max_side: 2048, compress: true, bc: true })] {
            let disk = Disk::open(cache.join(name)).unwrap();
            let first = pass(&paths, n, |p| prepare(p, want, Some(&disk)).unwrap().bytes());
            let again = pass(&paths, n, |p| {
                let q = prepare(p, want, Some(&disk)).unwrap();
                assert_eq!(q.how, How::Cached);
                // touched as an upload reads it
                q.packed.levels.iter().map(|l| {
                    std::hint::black_box(l.data.iter().step_by(4096).fold(0u8, |a, &b| a ^ b));
                    l.data.len() as u64
                }).sum()
            });
            let (disk_bytes, _) = disk.size();
            println!("{name}: first run {:.2} s ({} workers, decode{} + write), later runs {:.2} s (mapped off the cache); {:.0} MB in memory/GPU with levels, cache {:.0} MB on disk", first.0, n, if want.compress { " + BC3" } else { "" }, again.0, mb(first.1), mb(disk_bytes));
        }
        let _ = std::fs::remove_dir_all(&cache);
    }
}
