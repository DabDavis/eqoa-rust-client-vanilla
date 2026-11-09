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
//! round it). The server's `Compose` is the other copy of this: a change here is
//! one there, or no world builds.

use net::serverworlds::{Part, World};
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// A server's world held here, ready to load.
#[derive(Clone, Debug)]
pub struct Held {
    pub id: u8,
    /// "WORLD10.ESF": what the world table names it (`file_of`).
    pub file: String,
    pub path: PathBuf,
    pub parts: Vec<Part>,
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
                    if let Ok(mut g) = HELD.write() {
                        g.retain(|x| x.id != h.id);
                        g.push(h);
                    }
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
    let held = || Held { id: w.id, file: file.clone(), path: path.clone(), parts: w.parts.clone() };
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

/// A disc file's bytes, by position.
struct Src {
    f: std::fs::File,
    base: u64,
    len: u64,
}

impl Src {
    fn read(&self, off: u64, n: usize) -> Result<Vec<u8>, String> {
        if off + n as u64 > self.len {
            return Err(format!("a read at {off} of {n} runs past {}", self.len));
        }
        let mut b = vec![0u8; n];
        iso::read_exact_at(&self.f, self.base + off, &mut b).map_err(|e| e.to_string())?;
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
struct SrcWorld {
    src: Src,
    zones: Vec<(u64, u64)>,
    proxies: Vec<Vec<u8>>,
    rows: i32,
    cols: i32,
    regions: Vec<u8>,
}

impl SrcWorld {
    fn open(src: Src) -> Result<Self, String> {
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

/// A zone in the making.
struct Piece {
    world: usize,
    zone: usize,
    rec: Vec<u8>,
    proxy: Vec<u8>,
    col: i32,
    row: i32,
}

/// The world `w` names, built off the disc image at `disc`: its zones'
/// records checked against their CRCs on the server's disc, and the file's.
pub fn compose(disc: &Path, w: &World) -> Result<Vec<u8>, String> {
    let image = iso::Iso::open(disc).map_err(|e| format!("{}: {e}", disc.display()))?;
    let mut worlds: Vec<(String, SrcWorld)> = Vec::new();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut cells = std::collections::HashMap::new();
    for (i, p) in w.parts.iter().enumerate() {
        let name = p.from.to_uppercase();
        let wi = match worlds.iter().position(|(n, _)| *n == name) {
            Some(k) => k,
            None => {
                let e = image.entries().iter().find(|e| e.path.rsplit('/').next().is_some_and(|f| f.eq_ignore_ascii_case(&name))).ok_or_else(|| format!("{name} is not on this disc"))?;
                let f = iso::open_file(disc).map_err(|e| e.to_string())?;
                let src = Src { f, base: u64::from(e.lba) * iso::SECTOR, len: u64::from(e.size) };
                worlds.push((name.clone(), SrcWorld::open(src).map_err(|e| format!("{name}: {e}"))?));
                worlds.len() - 1
            }
        };
        let sw = &worlds[wi].1;
        let zi = usize::from(p.zone);
        let &(at, len) = sw.zones.get(zi).ok_or_else(|| format!("{name} has no zone {zi}"))?;
        let rec = sw.src.read(at, len as usize)?;
        if rec.len() as u64 != u64::from(p.size) || crc32(&rec) != p.crc {
            return Err(format!("{name} zone {zi} is not the server's (this disc's: {} bytes, CRC {:08x})", rec.len(), crc32(&rec)));
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
        pieces.push(Piece { world: wi, zone: zi, rec, proxy, col, row });
    }
    let cols = pieces.iter().map(|p| p.col + 1).max().unwrap_or(0);
    let rows = pieces.iter().map(|p| p.row + 1).max().unwrap_or(0);

    let mut out: Vec<u8> = Vec::with_capacity(pieces.iter().map(|p| p.rec.len()).sum::<usize>() + 4096);
    fn begin(out: &mut Vec<u8>, ty: u16, version: i16, count: u32) -> usize {
        let at = out.len();
        out.extend(ty.to_le_bytes());
        out.extend(version.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(count.to_le_bytes());
        at
    }
    fn end(out: &mut [u8], at: usize) {
        let n = (out.len() - at - 12) as u32;
        out[at + 4..at + 8].copy_from_slice(&n.to_le_bytes());
    }
    out.extend(b"FJBO");
    out.extend(1u32.to_le_bytes());
    out.extend(43855u32.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(32u64.to_le_bytes());
    out.extend((-1i64).to_le_bytes());
    let root = begin(&mut out, 0x8000, 0, 2);
    let world = begin(&mut out, 0x8100, 0, pieces.len() as u32);
    for p in &mut pieces {
        let at = out.len() as u64;
        let from = worlds[p.world].1.zones[p.zone].0;
        let delta = at as i64 - from as i64;
        let mut rec = std::mem::take(&mut p.rec);
        let base_at = rebase(&mut rec, delta)?;
        out.extend(&rec);
        p.proxy[0..8].copy_from_slice(&at.to_le_bytes());
        p.proxy[8..16].copy_from_slice(&(at + base_at).to_le_bytes());
    }
    end(&mut out, world);
    let base = begin(&mut out, 0x8200, 2, 4);
    let h = begin(&mut out, 0x8220, 1, 0);
    let f = |o: &mut Vec<u8>, v: f32| o.extend(v.to_bits().to_le_bytes());
    out.extend((pieces.len() as u32).to_le_bytes());
    f(&mut out, 0.0);
    f(&mut out, -1000.0);
    f(&mut out, 0.0);
    f(&mut out, (cols * CELL) as f32);
    f(&mut out, 1000.0);
    f(&mut out, (rows * CELL) as f32);
    out.extend((rows as u32).to_le_bytes());
    out.extend((cols as u32).to_le_bytes());
    out.extend((SUBDIV as u32).to_le_bytes());
    f(&mut out, CELL as f32);
    f(&mut out, 1000.0);
    f(&mut out, 0.0);
    f(&mut out, 1000.0);
    end(&mut out, h);
    let px = begin(&mut out, 0x8210, 2, 0);
    out.extend((pieces.len() as u32).to_le_bytes());
    for p in &pieces {
        out.extend(&p.proxy);
    }
    end(&mut out, px);
    let t = begin(&mut out, 0x8230, 0, 0);
    out.extend(world_tree(&pieces));
    end(&mut out, t);
    let g = begin(&mut out, 0x8240, 0, 0);
    out.extend(regions(&pieces, &worlds, rows, cols));
    end(&mut out, g);
    end(&mut out, base);
    end(&mut out, root);
    if out.len() as u64 != u64::from(w.size) || crc32(&out) != w.crc {
        return Err(format!("built {} bytes, CRC {:08x}: not the server's {} bytes, CRC {:08x}", out.len(), crc32(&out), w.size, w.crc));
    }
    Ok(out)
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
fn regions(pieces: &[Piece], worlds: &[(String, SrcWorld)], rows: i32, cols: i32) -> Vec<u8> {
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
        let part = |from: &str, zone: u16, size: u32, crc: u32, col: u8, row: u8| Part { from: from.into(), zone, size, crc, col, row, name: String::new(), race: 0 };
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
    }

    /// The Homelands as go-eqoa-server's recipe has them (tools/worlds/
    /// homelands.json, built 2026-11-08): nineteen zones of two worlds, each
    /// in its own cell, so the BSP's every split and both worlds' profiles;
    /// built here, it is the server's file byte for byte. Skipped without the
    /// disc.
    #[test]
    fn the_homelands_build_as_the_server_built_them() {
        let Some(disc) = disc() else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let parts = [
            ("ODUS.ESF", 8, 4236947, 0x8354D60F, 3, 1),
            ("ODUS.ESF", 19, 8409752, 0x91047E6F, 2, 3),
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
            ("ODUS.ESF", 28, 7848248, 0x4454CF3C, 5, 4),
        ];
        let parts = parts.iter().map(|&(from, zone, size, crc, col, row)| Part { from: from.into(), zone, size, crc, col, row, name: String::new(), race: 0 }).collect();
        let w = World { id: 10, name: "the Homelands".into(), size: 133406681, crc: 0xBC6C27E3, parts };
        let b = compose(&disc, &w).expect("the server's world");
        assert_eq!((b.len(), crc32(&b)), (133406681, 0xBC6C27E3));
    }
}
