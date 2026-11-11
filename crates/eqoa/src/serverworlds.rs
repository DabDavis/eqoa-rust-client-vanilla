//! A server's own worlds, built off the player's disc (go-eqoa-server's
//! `internal/worlds`, `cmd/worldbuild`; `net::serverworlds`): a world made of
//! the disc's own zones, each copied whole where it stands on the disc into a
//! world file of its own. The server sends what makes one (each zone's world
//! file and index, its CRC on the disc, the built file's CRC), never the zones:
//! the file is built here from this player's disc, the same bytes as the
//! server's build of its own (both CRCs checked), and kept in the cache
//! (`worlds/`, by its CRC) for the next time. The server is told which it holds
//! and sends the player into those alone; one that does not build (another
//! disc, a CRC that differs) is left out, and the server keeps the player on
//! the disc's worlds, as it does a PS2.
//!
//! The file is the shape of every retail world, the records and versions of
//! TUNARIA.ESF: the OBJF header ("FJBO", 1, 43855, 0, 32, -1), the root 0x8000
//! v0 of two, the world 0x8100 v0 of the zones (each 0x3000 record verbatim, its
//! ResourceTable 0x9000 and ZoneStaticTable 0x32C0 file offsets moved with it,
//! as a PS2 seeks to them; this client reads neither), and the world base 0x8200
//! v2: its header 0x8220 v1, the zones' proxies 0x8210 v2 (the disc's, their
//! offsets moved), the BSP 0x8230 v0 over the zones' cells, and the terrain
//! profiles 0x8240 v0 (each zone's own world's over its cell and the sub-cell
//! round it). A zone may be given extras, the disc's art placed in it as actors
//! of its own, its art copied in off the disc when the zone lacks it
//! ([`extras`]). The server's `Compose` is the other copy of this: a change here
//! is one there, or no world builds.
//!
//! The composer reads the disc through a [`Disc`] and writes through a
//! [`Sink`], a zone's record at a time (read, checked against its CRC, its
//! offsets moved or its extras added, written), so it never holds the world:
//! natively the disc image on disk into a `Vec` that is written to the cache
//! whole, as before (`compose`); in a page the image the player picked, read in
//! a worker, into a file of the origin's private storage as it is built
//! (`web::worlds`). The extras' art is read through the same `Disc`.

use net::serverworlds::{Part, World};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

mod extras;

/// A server's world held here, ready to load.
#[derive(Clone, Debug)]
pub struct Held {
    pub id: u8,
    /// "WORLD10.ESF": what the world table names it (`file_of`).
    pub file: String,
    pub path: PathBuf,
    pub parts: Vec<Part>,
    /// What players are told it is (the server's `World::name`).
    pub name: String,
}

static HELD: RwLock<Vec<Held>> = RwLock::new(Vec::new());

/// Takes the server's worlds from now on: the session builds the ones a
/// server lists (`net::session::set_world_builder`).
pub fn install() {
    net::session::set_world_builder(build_all);
}

/// The worlds of this session's server held here, forgotten (a new server).
pub fn forget() {
    if let Ok(mut h) = HELD.write() {
        h.clear();
    }
}

fn held() -> Vec<Held> {
    HELD.read().map(|h| h.clone()).unwrap_or_default()
}

/// `h` held from now on, in place of one of its id (a page's builder,
/// `web::worlds`, as `build_all` natively).
pub fn keep(h: Held) {
    if let Ok(mut g) = HELD.write() {
        g.retain(|x| x.id != h.id);
        g.push(h);
    }
}

/// The server's worlds held here, by id: (id, its name), the map's places
/// (`map::places`).
pub fn held_worlds() -> Vec<(u8, String)> {
    let mut v: Vec<(u8, String)> = held().into_iter().map(|h| (h.id, h.name)).collect();
    v.sort();
    v
}

/// The file the world table names for world `world` when it is one of the
/// server's held here.
pub fn file_of(world: i64) -> Option<String> {
    held().into_iter().find(|h| i64::from(h.id) == world).map(|h| h.file)
}

/// Where the server's world file `name` (any case) is kept; or, for the
/// viewer and tests, in `$EQOA_WORLDS_DIR` (a folder of built worlds,
/// `eqoa shot WORLD10.ESF zone 0 a.png`).
pub fn path_of(name: &str) -> Option<PathBuf> {
    if let Some(h) = held().into_iter().find(|h| h.file.eq_ignore_ascii_case(name)) {
        return Some(h.path);
    }
    let dir = std::env::var_os("EQOA_WORLDS_DIR")?;
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.is_file() && p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)))
}

/// The name of the zone in cell (`col`, `row`) of world `world`, when that is
/// one of the server's held here.
pub fn zone_name(world: i64, col: i32, row: i32) -> Option<String> {
    let h = held().into_iter().find(|h| i64::from(h.id) == world)?;
    h.parts.iter().find(|p| i32::from(p.col) == col && i32::from(p.row) == row).map(|p| p.name.clone())
}

/// The name of the zone a server's race starts in, when it is a world of the
/// server's held here (the create pages' starting place).
pub fn home_of(race: i32) -> Option<String> {
    held().into_iter().flat_map(|h| h.parts).find(|p| p.race != 0 && i32::from(p.race) == race).map(|p| p.name)
}

/// Where built worlds are kept: `$EQOA_CACHE/worlds`, else `eqoa-rs/worlds`
/// in the platform's cache directory.
fn cache_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("EQOA_CACHE") {
        return Some(PathBuf::from(d).join("worlds"));
    }
    let home = || std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        home().map(|h| h.join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| home().map(|h| h.join(".cache")))
    };
    base.map(|b| b.join("eqoa-rs").join("worlds"))
}

/// What the session's builder does with a server's list: on a thread of its
/// own, each world found in the cache or built off the disc, and the ids of
/// those held answered.
fn build_all(worlds: &[World]) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let worlds = worlds.to_vec();
    forget();
    std::thread::spawn(move || {
        let disc = PathBuf::from(crate::disc_path(None));
        let mut ids = Vec::new();
        for w in &worlds {
            match hold(&disc, w) {
                Ok(h) => {
                    eprintln!("server worlds: world {} {:?} ({} zones) held at {}", w.id, w.name, w.parts.len(), h.path.display());
                    ids.push(w.id);
                    keep(h);
                }
                Err(e) => eprintln!("server worlds: world {} {:?} not held: {e}", w.id, w.name),
            }
        }
        let _ = tx.send(ids);
    });
    rx
}

/// World `w` held: its file in the cache when one of its CRC is there, else
/// built off the disc and kept.
fn hold(disc: &Path, w: &World) -> Result<Held, String> {
    let dir = cache_dir().ok_or("no cache directory")?;
    let file = format!("WORLD{}.ESF", w.id);
    let path = dir.join(format!("world{}-{:08x}.ESF", w.id, w.crc));
    let held = || Held { id: w.id, file: file.clone(), path: path.clone(), parts: w.parts.clone(), name: w.name.clone() };
    if let Ok(b) = std::fs::read(&path)
        && b.len() as u64 == u64::from(w.size)
        && crc32(&b) == w.crc
    {
        return Ok(held());
    }
    let b = compose(disc, w)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let tmp = path.with_extension("part");
    std::fs::write(&tmp, &b).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(held())
}

/// CRC-32 (IEEE), as the server's `hash/crc32`.
pub fn crc32(b: &[u8]) -> u32 {
    crc32_update(0, b)
}

/// The CRC-32 of what `crc` was the CRC of, then `b`: `crc32_update(crc32(a),
/// b)` is the CRC of `a` and `b` together, so a file is checked a piece at a
/// time (a page's 133 MB world, never held). Eight bytes a step (slicing by
/// eight), bit for bit the byte-at-a-time table's.
pub fn crc32_update(crc: u32, b: &[u8]) -> u32 {
    let mut c = !crc;
    let mut words = b.chunks_exact(8);
    for w in &mut words {
        let lo = c ^ u32::from_le_bytes([w[0], w[1], w[2], w[3]]);
        let hi = u32::from_le_bytes([w[4], w[5], w[6], w[7]]);
        c = CRC_TABLES[7][(lo & 0xFF) as usize]
            ^ CRC_TABLES[6][((lo >> 8) & 0xFF) as usize]
            ^ CRC_TABLES[5][((lo >> 16) & 0xFF) as usize]
            ^ CRC_TABLES[4][(lo >> 24) as usize]
            ^ CRC_TABLES[3][(hi & 0xFF) as usize]
            ^ CRC_TABLES[2][((hi >> 8) & 0xFF) as usize]
            ^ CRC_TABLES[1][((hi >> 16) & 0xFF) as usize]
            ^ CRC_TABLES[0][(hi >> 24) as usize];
    }
    for &x in words.remainder() {
        c = CRC_TABLES[0][((c ^ x as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    !c
}

/// The IEEE polynomial's tables: `[0]` the byte-at-a-time one, `[k]` a byte
/// followed by k zero bytes.
const CRC_TABLES: [[u32; 256]; 8] = {
    let mut t = [[0u32; 256]; 8];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[0][i] = c;
        i += 1;
    }
    let mut i = 0;
    while i < 256 {
        let mut k = 1;
        while k < 8 {
            t[k][i] = (t[k - 1][i] >> 8) ^ t[0][(t[k - 1][i] & 0xFF) as usize];
            k += 1;
        }
        i += 1;
    }
    t
};

const CELL: i32 = 2000;
const SUBDIV: i32 = 4;
const LEAF: u32 = 0x4000_0000;

fn rd16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn rd32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn rd64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
fn rdf(b: &[u8], o: usize) -> f32 {
    f32::from_bits(rd32(b, o))
}

/// A record's header: type, version, body size, records under it, where.
#[derive(Clone, Copy, Debug)]
struct Hdr {
    ty: u16,
    version: i16,
    size: u64,
    count: u32,
    at: u64,
}

/// What a world is built off: the player's disc, its files found by name and
/// read by their place in the image. Natively the image on disk ([`compose`]);
/// in a page the image the player picked, read in a worker (`web::worlds`).
pub trait Disc {
    /// Where the file named `name` (its last part, any case) is in the image:
    /// its offset and length.
    fn file(&self, name: &str) -> Option<(u64, u64)>;
    /// `buf.len()` bytes at `off` of the image.
    fn read_at(&self, off: u64, buf: &mut [u8]) -> std::io::Result<()>;
}

/// Where a world is built into, as it is built ([`compose_into`]): a `Vec`
/// natively, in a page a file in the browser's storage.
pub trait Sink {
    /// How many bytes have been written.
    fn len(&self) -> u64;
    /// `b` written after them.
    fn put(&mut self, b: &[u8]) -> Result<(), String>;
    /// `b` written over the bytes at `at` (a record's size, once its body is
    /// in).
    fn patch(&mut self, at: u64, b: &[u8]) -> Result<(), String>;
    /// The CRC-32 ([`crc32`]) of everything written.
    fn crc(&mut self) -> Result<u32, String>;
}

impl Sink for Vec<u8> {
    fn len(&self) -> u64 {
        Vec::len(self) as u64
    }
    fn put(&mut self, b: &[u8]) -> Result<(), String> {
        self.extend_from_slice(b);
        Ok(())
    }
    fn patch(&mut self, at: u64, b: &[u8]) -> Result<(), String> {
        let at = usize::try_from(at).map_err(|e| e.to_string())?;
        self.get_mut(at..at + b.len()).ok_or("a patch past the end")?.copy_from_slice(b);
        Ok(())
    }
    fn crc(&mut self) -> Result<u32, String> {
        Ok(crc32(self))
    }
}

/// A file a world is written into at offsets ([`FileSink`]): a page's file
/// in the origin's private storage (`web::worlds`, its
/// `FileSystemSyncAccessHandle`).
pub trait FileAt {
    /// `b` written at `at`, the file growing as it must.
    fn write_at(&mut self, at: u64, b: &[u8]) -> Result<(), String>;
    /// `b.len()` bytes read at `at`.
    fn read_at(&mut self, at: u64, b: &mut [u8]) -> Result<(), String>;
}

/// A [`Sink`] into a [`FileAt`], so a world is built straight into a file and
/// never held: small writes gathered up to `gather` bytes, larger ones (a
/// zone's record) written as they come, a patch made in what is gathered or
/// in the file; the CRC read back off the file [`CHECK_STEP`] bytes at a time,
/// `step(done, total)` after each.
pub struct FileSink<F: FileAt> {
    pub file: F,
    len: u64,
    /// what was written after `len - buf.len()`, not yet in the file
    buf: Vec<u8>,
    gather: usize,
    step: Box<dyn FnMut(u64, u64)>,
}

/// How much of a file is read at a time to check its CRC ([`FileSink`]).
pub const CHECK_STEP: usize = 8 << 20;

impl<F: FileAt> FileSink<F> {
    pub fn new(file: F, gather: usize, step: Box<dyn FnMut(u64, u64)>) -> Self {
        FileSink { file, len: 0, buf: Vec::new(), gather, step }
    }

    /// What is gathered written to the file.
    pub fn flush(&mut self) -> Result<(), String> {
        if !self.buf.is_empty() {
            self.file.write_at(self.len - self.buf.len() as u64, &self.buf)?;
            self.buf.clear();
        }
        Ok(())
    }
}

impl<F: FileAt> Sink for FileSink<F> {
    fn len(&self) -> u64 {
        self.len
    }

    fn put(&mut self, b: &[u8]) -> Result<(), String> {
        if self.buf.len() + b.len() > self.gather {
            self.flush()?;
        }
        if b.len() > self.gather {
            self.file.write_at(self.len, b)?;
        } else {
            self.buf.extend_from_slice(b);
        }
        self.len += b.len() as u64;
        Ok(())
    }

    fn patch(&mut self, at: u64, b: &[u8]) -> Result<(), String> {
        if at + b.len() as u64 > self.len {
            return Err("a patch past the end".into());
        }
        let gathered = self.len - self.buf.len() as u64;
        if at >= gathered {
            let o = (at - gathered) as usize;
            self.buf[o..o + b.len()].copy_from_slice(b);
            return Ok(());
        }
        // in the file already (a record's size, before its body): what is
        // gathered goes first when the patch reaches into it
        if at + b.len() as u64 > gathered {
            self.flush()?;
        }
        self.file.write_at(at, b)
    }

    fn crc(&mut self) -> Result<u32, String> {
        self.flush()?;
        let mut chunk = vec![0u8; CHECK_STEP.min(self.len as usize)];
        let (mut crc, mut at) = (0u32, 0u64);
        while at < self.len {
            let n = CHECK_STEP.min((self.len - at) as usize);
            self.file.read_at(at, &mut chunk[..n])?;
            crc = crc32_update(crc, &chunk[..n]);
            at += n as u64;
            (self.step)(at, self.len);
        }
        Ok(crc)
    }
}

/// The disc image on disk, as `iso` lists it.
struct Image {
    f: std::fs::File,
    entries: Vec<iso::Entry>,
}

impl Disc for Image {
    fn file(&self, name: &str) -> Option<(u64, u64)> {
        let e = self.entries.iter().find(|e| e.path.rsplit('/').next().is_some_and(|f| f.eq_ignore_ascii_case(name)))?;
        Some((u64::from(e.lba) * iso::SECTOR, u64::from(e.size)))
    }
    fn read_at(&self, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
        iso::read_exact_at(&self.f, off, buf)
    }
}

/// A disc file's bytes, by position.
struct Src<'a> {
    d: &'a dyn Disc,
    base: u64,
    len: u64,
}

impl Src<'_> {
    fn read(&self, off: u64, n: usize) -> Result<Vec<u8>, String> {
        if off + n as u64 > self.len {
            return Err(format!("a read at {off} of {n} runs past {}", self.len));
        }
        let mut b = vec![0u8; n];
        self.d.read_at(self.base + off, &mut b).map_err(|e| e.to_string())?;
        Ok(b)
    }

    fn hdr(&self, at: u64, limit: u64) -> Result<Hdr, String> {
        if at + 12 > limit {
            return Err(format!("a record header at {at} runs past {limit}"));
        }
        let b = self.read(at, 12)?;
        hdr_of(&b, at, limit)
    }

    fn children(&self, h: &Hdr) -> Result<Vec<Hdr>, String> {
        let (mut p, end) = (h.at + 12, h.at + 12 + h.size);
        let mut out = Vec::new();
        for _ in 0..h.count {
            let c = self.hdr(p, end)?;
            p = c.at + 12 + c.size;
            out.push(c);
        }
        Ok(out)
    }
}

fn hdr_of(b: &[u8], at: u64, limit: u64) -> Result<Hdr, String> {
    let size = rd32(b, 4) as i32;
    let count = rd32(b, 8) as i32;
    if size < 0 || count < 0 || at + 12 + size as u64 > limit {
        return Err(format!("record {:#x} at {at}: size {size}, {count} records", rd16(b, 0)));
    }
    Ok(Hdr { ty: rd16(b, 0), version: rd16(b, 2) as i16, size: size as u64, count: count as u32, at })
}

/// A world file of the disc, as far as building needs it.
struct SrcWorld<'a> {
    src: Src<'a>,
    zones: Vec<(u64, u64)>,
    proxies: Vec<Vec<u8>>,
    rows: i32,
    cols: i32,
    regions: Vec<u8>,
}

impl<'a> SrcWorld<'a> {
    fn open(src: Src<'a>) -> Result<Self, String> {
        let head = src.read(0, 32)?;
        if &head[..4] != b"FJBO" {
            return Err("not an OBJF file".into());
        }
        let root = src.hdr(rd64(&head, 16), src.len)?;
        if root.ty != 0x8000 {
            return Err(format!("its first record is {:#x}", root.ty));
        }
        let mut w = SrcWorld { zones: Vec::new(), proxies: Vec::new(), rows: 0, cols: 0, regions: Vec::new(), src };
        let mut base = None;
        for c in w.src.children(&root)? {
            match c.ty {
                0x8100 => {
                    for z in w.src.children(&c)? {
                        if z.ty == 0x3000 {
                            w.zones.push((z.at, 12 + z.size));
                        }
                    }
                }
                0x8200 => base = Some(c),
                _ => {}
            }
        }
        let base = base.ok_or("no world base")?;
        for c in w.src.children(&base)? {
            let body = w.src.read(c.at + 12, c.size as usize)?;
            match c.ty {
                0x8220 => {
                    if c.version == 0 || body.len() < 56 {
                        return Err(format!("world header v{} of {} bytes", c.version, body.len()));
                    }
                    w.rows = rd32(&body, 28) as i32;
                    w.cols = rd32(&body, 32) as i32;
                    if rd32(&body, 36) as i32 != SUBDIV {
                        return Err("a world header of another sub-division".into());
                    }
                }
                0x8210 => {
                    let n = rd32(&body, 0) as i32;
                    if n < 0 || 4 + n as usize * 56 > body.len() {
                        return Err(format!("{n} proxies in {} bytes", body.len()));
                    }
                    w.proxies = (0..n as usize).map(|i| body[4 + i * 56..4 + (i + 1) * 56].to_vec()).collect();
                }
                0x8240 => w.regions = body,
                _ => {}
            }
        }
        Ok(w)
    }

    /// The proxy whose record offset is the zone's (by matching, never by
    /// index: a patched world's went stale).
    fn proxy_of(&self, zi: usize) -> Result<Vec<u8>, String> {
        let at = self.zones[zi].0;
        self.proxies.iter().find(|p| rd64(p, 0) == at).cloned().ok_or_else(|| format!("no proxy names zone {zi}"))
    }

    fn region(&self, r: i32, c: i32) -> u8 {
        let (wide, high) = (self.cols * SUBDIV, self.rows * SUBDIV);
        if r < 0 || c < 0 || r >= high || c >= wide {
            return 0;
        }
        self.regions.get((r * wide + c) as usize).copied().unwrap_or(0)
    }
}

/// A zone in the making: its record stays on the disc until it is written.
struct Piece {
    world: usize,
    zone: usize,
    proxy: Vec<u8>,
    col: i32,
    row: i32,
}

/// The world `w` names, built off the disc image at `disc`: its zones'
/// records checked against their CRCs on the server's disc, and the file's
/// ([`compose_into`], into memory).
pub fn compose(disc: &Path, w: &World) -> Result<Vec<u8>, String> {
    let image = iso::Iso::open(disc).map_err(|e| format!("{}: {e}", disc.display()))?;
    let f = iso::open_file(disc).map_err(|e| e.to_string())?;
    let image = Image { f, entries: image.entries().to_vec() };
    let mut out = Vec::with_capacity(w.size as usize);
    compose_into(&image, w, &mut out)?;
    Ok(out)
}

/// The world `w` names, built off `disc` into `out` as it is built: the
/// zones' places read first (their proxies), then each zone's record read,
/// checked against its CRC on the server's disc, its offsets moved and
/// written, one at a time; then the world's base, and the file checked
/// against the server's size and CRC. A world that is not the server's is an
/// error, with what was written left in `out`.
pub fn compose_into(disc: &dyn Disc, w: &World, out: &mut dyn Sink) -> Result<(), String> {
    let mut worlds: Vec<(String, SrcWorld)> = Vec::new();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut cells = std::collections::HashMap::new();
    let not_the_servers = |name: &str, zi: usize, rec: &[u8]| format!("{name} zone {zi} is not the server's (this disc's: {} bytes, CRC {:08x})", rec.len(), crc32(rec));
    for (i, p) in w.parts.iter().enumerate() {
        let name = p.from.to_uppercase();
        let wi = match worlds.iter().position(|(n, _)| *n == name) {
            Some(k) => k,
            None => {
                let (base, len) = disc.file(&name).ok_or_else(|| format!("{name} is not on this disc"))?;
                let src = Src { d: disc, base, len };
                worlds.push((name.clone(), SrcWorld::open(src).map_err(|e| format!("{name}: {e}"))?));
                worlds.len() - 1
            }
        };
        let sw = &worlds[wi].1;
        let zi = usize::from(p.zone);
        let &(at, len) = sw.zones.get(zi).ok_or_else(|| format!("{name} has no zone {zi}"))?;
        // its record is read when it is written; one of another size is not the server's now
        if len != u64::from(p.size) {
            return Err(not_the_servers(&name, zi, &sw.src.read(at, len as usize)?));
        }
        let proxy = sw.proxy_of(zi)?;
        let bx: Vec<f32> = (0..6).map(|k| rdf(&proxy, 32 + 4 * k)).collect();
        let col = (f64::from(bx[0] + bx[3]) / 2.0 / f64::from(CELL)).floor() as i32;
        let row = (f64::from(bx[2] + bx[5]) / 2.0 / f64::from(CELL)).floor() as i32;
        if !(0..=255).contains(&col) || !(0..=255).contains(&row) {
            return Err(format!("{name} zone {zi}: cell ({col}, {row})"));
        }
        if let Some(j) = cells.insert((col, row), i) {
            return Err(format!("parts {j} and {i} both stand in cell ({col}, {row})"));
        }
        pieces.push(Piece { world: wi, zone: zi, proxy, col, row });
    }
    let cols = pieces.iter().map(|p| p.col + 1).max().unwrap_or(0);
    let rows = pieces.iter().map(|p| p.row + 1).max().unwrap_or(0);

    fn begin(out: &mut dyn Sink, ty: u16, version: i16, count: u32) -> Result<u64, String> {
        let at = out.len();
        let mut h = Vec::with_capacity(12);
        h.extend(ty.to_le_bytes());
        h.extend(version.to_le_bytes());
        h.extend(0u32.to_le_bytes());
        h.extend(count.to_le_bytes());
        out.put(&h)?;
        Ok(at)
    }
    fn end(out: &mut dyn Sink, at: u64) -> Result<(), String> {
        let n = (out.len() - at - 12) as u32;
        out.patch(at + 4, &n.to_le_bytes())
    }
    let mut head = Vec::with_capacity(32);
    head.extend(b"FJBO");
    head.extend(1u32.to_le_bytes());
    head.extend(43855u32.to_le_bytes());
    head.extend(0u32.to_le_bytes());
    head.extend(32u64.to_le_bytes());
    head.extend((-1i64).to_le_bytes());
    // the extras' art, read through the disc before a zone is written
    let mut arts: std::collections::HashMap<(String, u16), extras::Art> = std::collections::HashMap::new();
    for e in w.parts.iter().flat_map(|p| &p.extras).filter(|e| !e.from.is_empty()) {
        let key = (e.from.to_uppercase(), e.zone);
        if !arts.contains_key(&key) {
            let a = art(disc, &key.0, key.1).map_err(|err| format!("{} zone {}: {err}", key.0, key.1))?;
            arts.insert(key, a);
        }
    }
    out.put(&head)?;
    let root = begin(out, 0x8000, 0, 2)?;
    let world = begin(out, 0x8100, 0, pieces.len() as u32)?;
    // a piece a part, in the parts' order
    for (p, part) in pieces.iter_mut().zip(&w.parts) {
        let (name, sw) = (&worlds[p.world].0, &worlds[p.world].1);
        let (from, len) = sw.zones[p.zone];
        let mut rec = sw.src.read(from, len as usize)?;
        if crc32(&rec) != part.crc {
            return Err(not_the_servers(name, p.zone, &rec));
        }
        let at = out.len();
        if part.extras.is_empty() {
            let delta = at as i64 - from as i64;
            let base_at = rebase(&mut rec, delta)?;
            out.put(&rec)?;
            p.proxy[0..8].copy_from_slice(&at.to_le_bytes());
            p.proxy[8..16].copy_from_slice(&(at + base_at).to_le_bytes());
        } else {
            let z = extras::with_extras(&rec, from as i64, at as i64, &part.extras, &arts).map_err(|e| format!("{name} zone {}: {e}", p.zone))?;
            out.put(&z.rec)?;
            p.proxy[0..8].copy_from_slice(&at.to_le_bytes());
            p.proxy[8..16].copy_from_slice(&(at + z.base_at as u64).to_le_bytes());
            p.proxy[16..20].copy_from_slice(&(z.base_len as u32).to_le_bytes());
        }
    }
    end(out, world)?;
    let base = begin(out, 0x8200, 2, 4)?;
    let h = begin(out, 0x8220, 1, 0)?;
    let mut b = Vec::new();
    let f = |o: &mut Vec<u8>, v: f32| o.extend(v.to_bits().to_le_bytes());
    b.extend((pieces.len() as u32).to_le_bytes());
    f(&mut b, 0.0);
    f(&mut b, -1000.0);
    f(&mut b, 0.0);
    f(&mut b, (cols * CELL) as f32);
    f(&mut b, 1000.0);
    f(&mut b, (rows * CELL) as f32);
    b.extend((rows as u32).to_le_bytes());
    b.extend((cols as u32).to_le_bytes());
    b.extend((SUBDIV as u32).to_le_bytes());
    f(&mut b, CELL as f32);
    f(&mut b, 1000.0);
    f(&mut b, 0.0);
    f(&mut b, 1000.0);
    out.put(&std::mem::take(&mut b))?;
    end(out, h)?;
    let px = begin(out, 0x8210, 2, 0)?;
    b.extend((pieces.len() as u32).to_le_bytes());
    for p in &pieces {
        b.extend(&p.proxy);
    }
    out.put(&std::mem::take(&mut b))?;
    end(out, px)?;
    let t = begin(out, 0x8230, 0, 0)?;
    out.put(&world_tree(&pieces))?;
    end(out, t)?;
    let g = begin(out, 0x8240, 0, 0)?;
    out.put(&regions(&pieces, &worlds, rows, cols))?;
    end(out, g)?;
    end(out, base)?;
    end(out, root)?;
    let crc = out.crc()?;
    if out.len() != u64::from(w.size) || crc != w.crc {
        return Err(format!("built {} bytes, CRC {:08x}: not the server's {} bytes, CRC {:08x}", out.len(), crc, w.size, w.crc));
    }
    Ok(())
}

/// The records an extra's art is taken from, read through `disc`: zone
/// `zone` of the disc's world file `name`, or the whole of any other file
/// (`extras::Art`).
fn art(disc: &dyn Disc, name: &str, zone: u16) -> Result<extras::Art, String> {
    let (base, len) = disc.file(name).ok_or_else(|| format!("{name} is not on this disc"))?;
    match SrcWorld::open(Src { d: disc, base, len }) {
        Ok(sw) => {
            let &(at, n) = sw.zones.get(usize::from(zone)).ok_or_else(|| format!("{name} has {} zones, not {}", sw.zones.len(), usize::from(zone) + 1))?;
            let rec = sw.src.read(at, n as usize)?;
            Ok(extras::Art::new(extras::parse_nodes(&rec, 0, rec.len() as i64)?))
        }
        Err(_) => extras::Art::whole(Src { d: disc, base, len }.read(0, len as usize)?),
    }
}

/// The zone record's file offsets moved by `delta` (ResourceTable's,
/// ZoneStaticTable's; -1 stays), and where its ZoneBase record is in it.
fn rebase(rec: &mut [u8], delta: i64) -> Result<u64, String> {
    let limit = rec.len() as u64;
    let zone = hdr_of(rec, 0, limit)?;
    if zone.ty != 0x3000 {
        return Err(format!("record {:#x}, not a zone", zone.ty));
    }
    let kids = |rec: &[u8], h: &Hdr| -> Result<Vec<Hdr>, String> {
        let (mut p, end) = (h.at + 12, h.at + 12 + h.size);
        let mut out = Vec::new();
        for _ in 0..h.count {
            if p + 12 > end {
                return Err("a record header runs past its parent".into());
            }
            let c = hdr_of(&rec[p as usize..p as usize + 12], p, end)?;
            p = c.at + 12 + c.size;
            out.push(c);
        }
        Ok(out)
    };
    let zb = kids(rec, &zone)?.into_iter().find(|c| c.ty == 0x3200).ok_or("no zone base")?;
    let mut moved = 0;
    for c in kids(rec, &zb)? {
        let body = &mut rec[(c.at + 12) as usize..(c.at + 12 + c.size) as usize];
        match c.ty {
            0x9000 => {
                let n = rd32(body, 0) as i32;
                if n < 0 || 4 + 16 * n as usize > body.len() {
                    return Err(format!("resource table: {n} entries in {} bytes", body.len()));
                }
                for i in 0..n as usize {
                    let o = 4 + 16 * i;
                    let v = rd64(body, o) as i64 + delta;
                    body[o..o + 8].copy_from_slice(&v.to_le_bytes());
                    moved += 1;
                }
            }
            0x32C0 => {
                let n = rd32(body, 0) as i32;
                if n < 0 || 4 + 24 * n as usize != body.len() {
                    return Err(format!("static table: {n} rooms in {} bytes", body.len()));
                }
                for i in 0..n as usize {
                    for o in [4 + 24 * i, 4 + 24 * i + 12] {
                        let v = rd64(body, o) as i64;
                        if v >= 0 {
                            body[o..o + 8].copy_from_slice(&(v + delta).to_le_bytes());
                            moved += 1;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if moved == 0 {
        return Err("no resource table to move".into());
    }
    Ok(zb.at)
}

/// The world's BSP: split at cell edges, the wider spread of columns or rows
/// first, the edge nearest the middle; one zone is one node with both leaves
/// that zone. A node's front is where n.p + d >= 0.
fn world_tree(pieces: &[Piece]) -> Vec<u8> {
    struct Node {
        n: [f32; 3],
        d: f32,
        front: u32,
        back: u32,
    }
    let mut nodes: Vec<Node> = Vec::new();
    let mut leaves: Vec<u32> = Vec::new();
    if pieces.len() == 1 {
        nodes.push(Node { n: [-1.0, 0.0, 0.0], d: ((pieces[0].col * 2 + 1) * CELL / 2) as f32, front: LEAF, back: LEAF | 1 });
        leaves = vec![0, 0];
    } else {
        fn build(set: &[usize], pieces: &[Piece], nodes: &mut Vec<Node>, leaves: &mut Vec<u32>) -> u32 {
            if set.len() == 1 {
                leaves.push(set[0] as u32);
                return LEAF | (leaves.len() as u32 - 1);
            }
            let spread = |f: &dyn Fn(&Piece) -> i32| {
                let v: Vec<i32> = set.iter().map(|&i| f(&pieces[i])).collect();
                (*v.iter().min().unwrap(), *v.iter().max().unwrap())
            };
            let (cl, ch) = spread(&|p| p.col);
            let (rl, rh) = spread(&|p| p.row);
            let on_x = ch - cl >= rh - rl;
            let cell = |i: usize| if on_x { pieces[i].col } else { pieces[i].row };
            let mut s = set.to_vec();
            s.sort_by(|&a, &b| cell(a).cmp(&cell(b)).then(a.cmp(&b)));
            let (mut k, mut best) = (0, usize::MAX);
            for j in 1..s.len() {
                if cell(s[j]) == cell(s[j - 1]) {
                    continue;
                }
                let d = (2 * j).abs_diff(s.len());
                if d < best {
                    k = j;
                    best = d;
                }
            }
            let edge = (cell(s[k]) * CELL) as f32;
            let me = nodes.len();
            nodes.push(Node { n: if on_x { [-1.0, 0.0, 0.0] } else { [0.0, 0.0, -1.0] }, d: edge, front: 0, back: 0 });
            let front = build(&s[..k], pieces, nodes, leaves);
            let back = build(&s[k..], pieces, nodes, leaves);
            nodes[me].front = front;
            nodes[me].back = back;
            me as u32
        }
        let all: Vec<usize> = (0..pieces.len()).collect();
        build(&all, pieces, &mut nodes, &mut leaves);
    }
    let mut b = Vec::new();
    b.extend((nodes.len() as u32).to_le_bytes());
    b.extend((leaves.len() as u32).to_le_bytes());
    for n in &nodes {
        for v in n.n {
            b.extend(v.to_bits().to_le_bytes());
        }
        b.extend(n.d.to_bits().to_le_bytes());
        b.extend(n.front.to_le_bytes());
        b.extend(n.back.to_le_bytes());
    }
    for l in leaves {
        b.extend(l.to_le_bytes());
    }
    b
}

/// The terrain profiles: each zone's own world's over its cell and the
/// sub-cell round it, a cell's own over any neighbour's margin; 0 elsewhere.
fn regions(pieces: &[Piece], worlds: &[(String, SrcWorld<'_>)], rows: i32, cols: i32) -> Vec<u8> {
    let (wide, high) = (cols * SUBDIV, rows * SUBDIV);
    let mut out = vec![0u8; (wide * high).max(0) as usize];
    let mut put = |p: &Piece, margin: i32| {
        let sw = &worlds[p.world].1;
        for r in p.row * SUBDIV - margin..(p.row + 1) * SUBDIV + margin {
            for c in p.col * SUBDIV - margin..(p.col + 1) * SUBDIV + margin {
                if r >= 0 && c >= 0 && r < high && c < wide {
                    out[(r * wide + c) as usize] = sw.region(r, c);
                }
            }
        }
    };
    for p in pieces {
        put(p, 1);
    }
    for p in pieces {
        put(p, 0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc() -> Option<PathBuf> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        p.exists().then_some(p)
    }

    /// The CRC is the server's (`hash/crc32`, IEEE): its check value.
    #[test]
    fn crc32_is_ieee() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    /// The bytes of a test, the same every run (xorshift).
    fn noise(n: usize, seed: u32) -> Vec<u8> {
        let mut x = seed | 1;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect()
    }

    /// Eight bytes a step is the byte-at-a-time table's CRC (the one this
    /// file had, the server's) for every length up to 300 and every split of
    /// it in two: a file checked a piece at a time is checked as a whole.
    /// The control: one bit flipped is another CRC.
    #[test]
    fn crc32_in_pieces_is_crc32_whole() {
        fn bytewise(b: &[u8]) -> u32 {
            let mut table = [0u32; 256];
            for (i, t) in table.iter_mut().enumerate() {
                let mut c = i as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
                }
                *t = c;
            }
            !b.iter().fold(!0u32, |c, &x| table[((c ^ x as u32) & 0xFF) as usize] ^ (c >> 8))
        }
        let b = noise(300, 7);
        for n in 0..=b.len() {
            let whole = bytewise(&b[..n]);
            assert_eq!(crc32(&b[..n]), whole, "{n} bytes");
            for k in 0..=n {
                assert_eq!(crc32_update(crc32(&b[..k]), &b[k..n]), whole, "{n} bytes split at {k}");
            }
        }
        let mut flipped = b.clone();
        flipped[123] ^= 4;
        assert_ne!(crc32(&flipped), crc32(&b), "the control: a bit flipped");
    }

    /// A file a test writes into (`FileAt`), counting its writes.
    #[derive(Default)]
    struct VecFile {
        b: Vec<u8>,
        writes: usize,
    }

    impl FileAt for VecFile {
        fn write_at(&mut self, at: u64, b: &[u8]) -> Result<(), String> {
            let at = at as usize;
            if self.b.len() < at + b.len() {
                self.b.resize(at + b.len(), 0);
            }
            self.b[at..at + b.len()].copy_from_slice(b);
            self.writes += 1;
            Ok(())
        }
        fn read_at(&mut self, at: u64, b: &mut [u8]) -> Result<(), String> {
            let at = at as usize;
            b.copy_from_slice(self.b.get(at..at + b.len()).ok_or("past the end")?);
            Ok(())
        }
    }

    /// Writes and patches as the composer makes them (small heads, a record
    /// bigger than what is gathered, sizes patched once their bodies are in,
    /// some still gathered, some in the file, one reaching across both) leave
    /// a `FileSink`'s file the bytes a `Vec` holds, and its CRC, read back, is
    /// theirs; a record goes to the file as it comes, never gathered. The
    /// control: the same writes with one patch left out are another file.
    #[test]
    fn a_file_sink_is_a_vec_written_into_a_file() {
        fn script(out: &mut dyn Sink, skip: bool) {
            out.put(b"FJBOhead").unwrap();
            let root = out.len();
            out.put(&[0u8; 12]).unwrap();
            for k in 0..3u8 {
                out.put(&noise(5000 + 300 * usize::from(k), u32::from(k) + 1)).unwrap();
                out.put(&[k; 7]).unwrap();
            }
            let tail = out.len();
            out.put(&[0u8; 12]).unwrap();
            out.put(&noise(40, 9)).unwrap();
            // in what is gathered, then in the file, then across both
            out.patch(tail + 4, &44u32.to_le_bytes()).unwrap();
            if !skip {
                out.patch(root + 4, &123u32.to_le_bytes()).unwrap();
            }
            // the last record's end and the 7 bytes gathered after it
            out.patch(tail - 10, &noise(10, 3)).unwrap();
        }
        let mut v = Vec::new();
        script(&mut v, false);
        let steps = std::rc::Rc::new(std::cell::Cell::new(0));
        let s2 = steps.clone();
        let mut f = FileSink::new(VecFile::default(), 64, Box::new(move |_, _| s2.set(s2.get() + 1)));
        script(&mut f, false);
        assert_eq!(f.crc().unwrap(), crc32(&v));
        assert_eq!(f.file.b, v);
        assert!(steps.get() >= 1, "the read-back said how far it came");
        assert!(f.file.writes < 20, "{} writes: small ones gathered", f.file.writes);
        assert!(f.patch(v.len() as u64 - 2, &[0; 4]).is_err(), "a patch past the end");
        let mut g = FileSink::new(VecFile::default(), 64, Box::new(|_, _| {}));
        script(&mut g, true);
        assert_ne!(g.crc().unwrap(), crc32(&v), "the control: a patch left out");
    }

    /// A one-zone world (Tunaria's zone 115, Aviak Village) built here is the
    /// server's build of the same recipe byte for byte: its size and CRC are
    /// go-eqoa-server's `cmd/worldbuild` output's (2026-11-08). A zone whose CRC
    /// is not this disc's is refused (the control), as is a second zone in a
    /// taken cell. Skipped without the disc (`run/eqoa.iso`).
    #[test]
    fn a_world_builds_as_the_server_built_it() {
        let Some(disc) = disc() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let part = |from: &str, zone: u16, size: u32, crc: u32, col: u8, row: u8| Part { from: from.into(), zone, size, crc, col, row, name: String::new(), race: 0, extras: vec![] };
        let aviak = part("TUNARIA.ESF", 115, 5_879_151, 0x7257_AE33, 4, 10);
        let one = World { id: 10, name: "the Homelands".into(), size: 5_880_303, crc: 0xB6D9_4C34, parts: vec![aviak.clone()] };
        let b = compose(&disc, &one).expect("the server's world");
        assert_eq!((b.len(), crc32(&b)), (5_880_303, 0xB6D9_4C34));
        let mut wrong = one.clone();
        wrong.parts[0].crc ^= 1;
        assert!(compose(&disc, &wrong).unwrap_err().contains("not the server's"), "the control: another disc's zone");
        let mut twice = one.clone();
        twice.parts.push(aviak);
        assert!(compose(&disc, &twice).unwrap_err().contains("both stand in cell"), "one zone a cell");
        // built straight into a file, as a page builds it (`web::worlds`):
        // the same bytes, a zone's record written as it came
        let image = iso::Iso::open(&disc).unwrap();
        let src = Image { f: iso::open_file(&disc).unwrap(), entries: image.entries().to_vec() };
        let mut f = FileSink::new(VecFile::default(), 1 << 20, Box::new(|_, _| {}));
        compose_into(&src, &one, &mut f).expect("the server's world, into a file");
        assert_eq!(f.file.b, b);
        let mut g = FileSink::new(VecFile::default(), 1 << 20, Box::new(|_, _| {}));
        assert!(compose_into(&src, &wrong, &mut g).unwrap_err().contains("not the server's"), "the control: into a file too");
    }

    /// A zone given extras built here is the server's build of the same
    /// recipe byte for byte (go-eqoa-server's TestComposeExtrasOffTheDisc,
    /// 2026-11-10: Aviak Village given its own tree, nothing copied, and two
    /// of The Vastly Deep's trees off Odus, copied in, one scaled and turned,
    /// one of a light of its own). The control: an extra whose art is not
    /// where it is said to be is refused. Skipped without the disc.
    #[test]
    fn a_zone_with_extras_builds_as_the_server_built_it() {
        let Some(disc) = disc() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let e = |sprite: u32, from: &str, zone: u16, at: [f32; 3], hpr: [f32; 3], scale: f32, color: [u8; 4]| net::serverworlds::Extra { sprite, from: from.into(), zone, at, hpr, scale, color };
        let extras = vec![
            e(0x194F_A7CB, "", 0, [9300.0, 84.0, 21300.0], [0.5, 0.0, 0.0], 1.0, [0, 0, 0, 255]),
            e(0xF017_8AAC, "ODUS.ESF", 22, [9250.0, 84.0, 21350.0], [1.25, 0.1, 0.0], 2.5, [0, 0, 0, 255]),
            e(0x5698_614C, "ODUS.ESF", 22, [8100.0, 90.0, 20100.0], [0.0; 3], 1.0, [10, 20, 30, 200]),
        ];
        let aviak = Part { from: "TUNARIA.ESF".into(), zone: 115, size: 5_879_151, crc: 0x7257_AE33, col: 4, row: 10, name: String::new(), race: 0, extras };
        let w = World { id: 10, name: "the Homelands".into(), size: 6_139_437, crc: 0x25A4_139A, parts: vec![aviak] };
        let b = compose(&disc, &w).expect("the server's world");
        assert_eq!((b.len(), crc32(&b)), (6_139_437, 0x25A4_139A));
        let mut wrong = w.clone();
        wrong.parts[0].extras[1].sprite = 0x1234_5678;
        assert!(compose(&disc, &wrong).err().is_some_and(|e| e.contains("not in its art")), "the control: art not where it is said to be");
    }

    /// Every world built beside a server in `$EQOA_WORLDS_DIR` (its
    /// `worldN.json`, as `cmd/worldbuild` writes it: the recipe, extras and
    /// all, and the file's size and CRC) built here off the disc is the
    /// server's file byte for byte: the check that the two composers agree on
    /// whatever recipe the server builds. Skipped without both.
    #[test]
    fn the_server_worlds_beside_us_build_here() {
        let (Some(disc), Some(dir)) = (disc(), std::env::var_os("EQOA_WORLDS_DIR")) else {
            eprintln!("SKIP: no run/eqoa.iso or EQOA_WORLDS_DIR");
            return;
        };
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).expect("the worlds folder").flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let Some(w) = name.strip_prefix("world").and_then(|r| r.strip_suffix(".json")).and_then(|n| n.parse::<u8>().ok()) else { continue };
            let j: serde_json::Value = serde_json::from_slice(&std::fs::read(entry.path()).unwrap()).unwrap();
            let world = built_world(w, &j);
            let b = compose(&disc, &world).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!((b.len() as u32, crc32(&b)), (world.size, world.crc), "{name}");
            seen += 1;
        }
        eprintln!("{seen} server worlds built as the server built them");
    }

    /// A world as the server's `worldN.json` has it, as the list it is sent.
    fn built_world(id: u8, j: &serde_json::Value) -> World {
        let u = |v: &serde_json::Value| v.as_u64().unwrap_or(0);
        // a float as the server's (Go's) JSON wrote a float32: its text read
        // as an f32 (through an f64 it could round twice)
        let f = |v: &serde_json::Value| v.as_number().and_then(|n| n.to_string().parse::<f32>().ok()).unwrap_or(0.0);
        let zones = j["zones"].as_array().cloned().unwrap_or_default();
        let parts = j["parts"].as_array().unwrap();
        World {
            id,
            name: j["name"].as_str().unwrap_or("").into(),
            size: u(&j["size"]) as u32,
            crc: u(&j["crc"]) as u32,
            parts: parts
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let z = zones.get(i).cloned().unwrap_or_default();
                    let extras = z["extras"]
                        .as_array()
                        .map(|list| {
                            list.iter()
                                .map(|e| {
                                    let sprite = match &e["sprite"] {
                                        serde_json::Value::String(s) => u32::from_str_radix(s.trim_start_matches("0x").trim_start_matches("0X"), 16).unwrap(),
                                        v => u(v) as u32,
                                    };
                                    let v3 = |k: &str| [f(&e[k][0]), f(&e[k][1]), f(&e[k][2])];
                                    let scale = if e["scale"].is_null() || f(&e["scale"]) == 0.0 { 1.0 } else { f(&e["scale"]) };
                                    let color = e["color"].as_array().map_or([0, 0, 0, 255], |c| [0, 1, 2, 3].map(|k| u(&c[k]) as u8));
                                    let from = e["from"].as_str().unwrap_or("").to_string();
                                    net::serverworlds::Extra { sprite, zone: if from.is_empty() { 0 } else { u(&e["zone"]) as u16 }, from, at: v3("at"), hpr: v3("hpr"), scale, color }
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Part {
                        from: p["from"].as_str().unwrap().into(),
                        zone: u(&p["zone"]) as u16,
                        size: u(&p["size"]) as u32,
                        crc: u(&p["crc"]) as u32,
                        col: u(&p["col"]) as u8,
                        row: u(&p["row"]) as u8,
                        name: z["name"].as_str().unwrap_or("").into(),
                        race: u(&z["race"]) as u8,
                        extras,
                    }
                })
                .collect(),
        }
    }

    /// The Homelands as go-eqoa-server's recipe has them (tools/worlds/
    /// homelands.json, merged 2026-11-10): twenty-one zones of three worlds,
    /// each in its own cell, so the BSP's every split and all three worlds'
    /// profiles; built here without the ridge of rocks along their edges (the
    /// recipe's extraFiles), it is the server's file of the same zones byte
    /// for byte. With `$EQOA_WORLDS_DIR` holding the server's `world10.json`
    /// of these zones, the world as served, its 1,861 rocks too, is pinned as
    /// well (the server's `worldbuild`: 147,041,330 bytes, CRC 5fa4f6a2).
    /// Skipped without the disc.
    #[test]
    fn the_homelands_build_as_the_server_built_them() {
        let Some(disc) = disc() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let parts = [
            ("ODUS.ESF", 8, 4236947, 0x8354D60F, 3, 1),
            ("ODUS.ESF", 19, 8409752, 0x91047E6F, 2, 3),
            ("RATHE.ESF", 15, 6521318, 0xABD0428A, 2, 4),
            ("TUNARIA.ESF", 68, 7417256, 0x9330B29B, 9, 6),
            ("TUNARIA.ESF", 115, 5879151, 0x7257AE33, 4, 10),
            ("TUNARIA.ESF", 160, 6574889, 0x18B97D99, 7, 15),
            ("ODUS.ESF", 7, 5769073, 0xFD84B785, 2, 1),
            ("TUNARIA.ESF", 103, 6244148, 0x2CC37CE5, 5, 9),
            ("TUNARIA.ESF", 54, 6245081, 0x4EE9A0A0, 8, 5),
            ("TUNARIA.ESF", 49, 8004149, 0xB2F96BBF, 3, 5),
            ("TUNARIA.ESF", 65, 8548970, 0x42E1A52C, 6, 6),
            ("TUNARIA.ESF", 42, 7288745, 0x0BA8BD96, 9, 4),
            ("ODUS.ESF", 27, 7830256, 0x4FF407C3, 4, 4),
            ("TUNARIA.ESF", 143, 7443987, 0xBAC867D8, 6, 13),
            ("ODUS.ESF", 22, 6846934, 0xA6AF6EFE, 5, 3),
            ("ODUS.ESF", 14, 5318740, 0xC66ECF86, 3, 2),
            ("ODUS.ESF", 13, 8099924, 0x60531D03, 2, 2),
            ("TUNARIA.ESF", 82, 6426546, 0x422D8CC6, 10, 7),
            ("TUNARIA.ESF", 157, 8968801, 0x4E29CED4, 12, 14),
            ("RATHE.ESF", 16, 5808555, 0x39C43319, 3, 4),
            ("ODUS.ESF", 28, 7848248, 0x4454CF3C, 5, 4),
        ];
        let parts: Vec<Part> = parts.iter().map(|&(from, zone, size, crc, col, row)| Part { from: from.into(), zone, size, crc, col, row, name: String::new(), race: 0, extras: vec![] }).collect();
        let w = World { id: 10, name: "the Homelands".into(), size: 145736722, crc: 0x776848A5, parts: parts.clone() };
        let b = compose(&disc, &w).expect("the server's world");
        assert_eq!((b.len(), crc32(&b)), (145736722, 0x776848A5));
        drop(b);
        // the world as served, rocks and all, when the server's build of
        // these zones is beside us
        let Some(j) = std::env::var_os("EQOA_WORLDS_DIR").and_then(|d| std::fs::read(Path::new(&d).join("world10.json")).ok()) else {
            eprintln!("SKIP (the rocks): no world10.json in EQOA_WORLDS_DIR");
            return;
        };
        let served = built_world(10, &serde_json::from_slice(&j).unwrap());
        let same = |a: &Part, b: &Part| (&a.from, a.zone, a.size, a.crc, a.col, a.row) == (&b.from, b.zone, b.size, b.crc, b.col, b.row);
        if served.parts.len() != parts.len() || !served.parts.iter().zip(&parts).all(|(a, b)| same(a, b)) {
            eprintln!("SKIP (the rocks): EQOA_WORLDS_DIR's world10.json is of other zones");
            return;
        }
        assert_eq!(served.parts.iter().map(|p| p.extras.len()).sum::<usize>(), 1861, "the ridge's rocks");
        let b = compose(&disc, &served).expect("the server's world, rocks and all");
        assert_eq!((b.len(), crc32(&b)), (147041330, 0x5FA4F6A2));
    }
}
