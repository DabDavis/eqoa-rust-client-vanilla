//! EQOA's asset containers.
//!
//! An `.ESF` is an OBJF file: the magic `"FJBO"` (OBJF reversed), a header
//! whose i64 at +16 is the offset of a forest of records, each
//!
//! ```text
//! type u16 | version i16 | size i32 | numSubObjects u32 | body[size]
//! ```
//!
//! with its children packed at the start of its body. A `.CSF` is the same file
//! zlib-compressed in blocks. Records with a dictionary id (a DictID) are how
//! the game names art across files.
//!
//! Ported from the Blender importer's `esf.py`, which was checked against
//! `go-eqoa-esf-tools-v2`; `tests/surfaces.rs` holds this crate to the same
//! per-surface CRC fixtures. The game's own parser is `viobjfile.cpp` and
//! `viesfparse.cpp` in the decomp, and that is the authority where they differ.

use std::collections::HashMap;
use std::io::Read;

mod library;
pub(crate) mod mesh;
mod surface;
pub mod font;
pub mod sound;
pub mod zone;
pub use library::{Era, Library, Ref};
pub use mesh::{Batch, Layer, Model, ModelRef, PrimBuffer, VertexList};
pub use surface::{Image, Surface};

pub mod types {
    pub const SURFACE: u16 = 0x1000;
    pub const MATERIAL: u16 = 0x1100;
    pub const MATERIAL_ARRAY: u16 = 0x1101;
    pub const MATPAL: u16 = 0x1110;
    pub const MATPAL_HDR: u16 = 0x1111;
    pub const PRIMBUF: u16 = 0x1200;
    pub const SKINPRIMBUF: u16 = 0x1210;
    /// `ParseFloraPrimBuffer`'s, in a flora sprite (`FLORASPRITE`).
    pub const FLORAPRIMBUF: u16 = 0x1230;
    /// `VIFloraSprite`: a header (0x2F01: id, box), a palette, a 0x1230.
    pub const FLORASPRITE: u16 = 0x2F00;
    pub const SIMPLESPRITE: u16 = 0x2000;
    pub const SIMPLESPRITE_HDR: u16 = 0x2001;
    pub const HSPRITE: u16 = 0x2200;
    pub const HSPRITE_HDR: u16 = 0x2210;
    pub const HSPRITE_ARRAY: u16 = 0x2220;
    pub const HSPRITE_HIERARCHY: u16 = 0x2400;
    pub const SUBSPRITE: u16 = 0x2310;
    pub const SUBSPRITE_HDR: u16 = 0x2311;
    pub const SKINSUBSPRITE: u16 = 0x2320;
    pub const SKINSUBSPRITE2: u16 = 0x2321;
    pub const CSPRITE: u16 = 0x2700;
    pub const CSPRITE_HDR: u16 = 0x2710;
    pub const CSPRITE_ARRAY: u16 = 0x2800;
    pub const LODSPRITE: u16 = 0x2A00;
    pub const LODSPRITE_HDR: u16 = 0x2A10;
    pub const LODSPRITE_ARRAY: u16 = 0x2A20;
    pub const CSPRITE_VARIANT: u16 = 0x2A40;
    pub const CSPRITE_VARIANT_MESHES: u16 = 0x2A60;
    pub const GROUPSPRITE: u16 = 0x2C00;
    pub const GROUPSPRITE_HDR: u16 = 0x2C10;
    pub const GROUPSPRITE_MEMBERS: u16 = 0x2C30;
    pub const RESDIR2: u16 = 0xA010;
}
use types::*;

/// Which records register a DictID, read off the decomp's 27 `m_dict->Add`
/// sites (`viesfparse.cpp`). Two shapes:
///
/// - the id is the first word of the record's OWN body (`Read(id)` straight
///   after the dispatcher's `ReadBegin`): surfaces, point sprites and lights,
///   streamed audio, animations, the CSprite ref map, flora PrimBuffers, and
///   PrimBuffers from version 2 (`if (version >= 2) Read(id)`);
/// - the id is the first word of the record's FIRST CHILD, a header, and the
///   PARENT is what gets registered: every other sprite, palette, sound,
///   particle, font and spell record.
const ID_IN_BODY: &[u16] = &[SURFACE, 0x2D00, 0x2B00, 0x2E00, 0x2600, 0x5000, 0x1230];
const ID_IN_BODY_FROM_V2: &[u16] = &[PRIMBUF, SKINPRIMBUF];
const ID_IN_HEADER: &[u16] = &[
    SIMPLESPRITE, HSPRITE, CSPRITE, LODSPRITE, CSPRITE_VARIANT, GROUPSPRITE, 0x2F00, SUBSPRITE, SKINSUBSPRITE,
    0xB100, 0xC100, 0xC300, MATPAL, 0x7000, 0xB000, 0xB030, 0xC200, 0xC000,
];

const CSF_HEADER_SIZE: usize = 40;

#[derive(Debug)]
pub enum Error {
    NotAContainer(String),
    Corrupt(String),
    /// A read through a [`ReadAt`] failed; `WouldBlock` means the bytes are
    /// not there yet, and the whole call may be made again once they are.
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::NotAContainer(s) => write!(f, "not an EQOA container: {s}"),
            Error::Corrupt(s) => write!(f, "corrupt container: {s}"),
            Error::Io(e) => write!(f, "reading the container: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// Random access to a file's bytes, for opening a world without reading it
/// whole. A read whose bytes are not available yet fails with
/// `ErrorKind::WouldBlock` (the browser fetches them, then makes the whole
/// call again), so nothing reading through one keeps state across a failure.
pub trait ReadAt {
    fn len(&self) -> u64;
    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> std::io::Result<()>;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl ReadAt for [u8] {
    fn len(&self) -> u64 {
        <[u8]>::len(self) as u64
    }
    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
        let o = usize::try_from(off).ok().filter(|&o| o <= <[u8]>::len(self) && buf.len() <= <[u8]>::len(self) - o);
        let o = o.ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))?;
        buf.copy_from_slice(&self[o..o + buf.len()]);
        Ok(())
    }
}

impl ReadAt for Vec<u8> {
    fn len(&self) -> u64 {
        self.as_slice().len() as u64
    }
    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.as_mut_slice().read_at(off, buf)
    }
}

/// `len` bytes of `inner` from `base` on, as a file of their own: a file
/// inside the disc image (its extent at `lba * 2048`).
pub struct Window<R> {
    pub inner: R,
    pub base: u64,
    pub len: u64,
}

impl<R> Window<R> {
    pub fn new(inner: R, base: u64, len: u64) -> Self {
        Window { inner, base, len }
    }
}

impl<R: std::io::Read + std::io::Seek> ReadAt for Window<R> {
    fn len(&self) -> u64 {
        self.len
    }
    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
        if off.checked_add(buf.len() as u64).is_none_or(|end| end > self.len) {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        self.inner.seek(std::io::SeekFrom::Start(self.base + off))?;
        self.inner.read_exact(buf)
    }
}

/// The whole of a source, for what is read whole (a CSF, a file that is not
/// a world).
pub fn read_all(src: &mut dyn ReadAt) -> std::io::Result<Vec<u8>> {
    let n = usize::try_from(src.len()).map_err(|_| std::io::Error::from(std::io::ErrorKind::OutOfMemory))?;
    let mut b = vec![0u8; n];
    src.read_at(0, &mut b)?;
    Ok(b)
}

pub(crate) fn f32le(b: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

pub(crate) fn i32le(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn i64le(b: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

/// A CSF is recognised by NOT being an uncompressed OBJF.
pub fn is_csf(data: &[u8]) -> bool {
    data.len() >= 4 && &data[..4] != b"FJBO"
}

/// Inflates a CSF: a 40-byte header (block count at +4, block table offset as
/// an i64 at +24), then N blocks, each an 8-byte length and a zlib stream.
pub fn decompress_csf(data: &[u8]) -> Result<Vec<u8>, Error> {
    if data.len() < CSF_HEADER_SIZE {
        return Err(Error::NotAContainer(format!("{} bytes", data.len())));
    }
    let blocks = i32le(data, 4) as u32;
    let offset = i64le(data, 24);
    if !(CSF_HEADER_SIZE as i64..=data.len() as i64).contains(&offset) || blocks == 0 || blocks > 1 << 20 {
        return Err(Error::NotAContainer("no OBJF magic and no usable CSF header".into()));
    }
    let mut out = Vec::new();
    let mut pos = offset as usize;
    for b in 0..blocks {
        if pos + 8 > data.len() {
            return Err(Error::Corrupt(format!("CSF block {b}: truncated size")));
        }
        let size = (i64le(data, pos) as u64 & 0xFFFF_FFFF) as usize;
        pos += 8;
        if pos + size > data.len() {
            return Err(Error::Corrupt(format!("CSF block {b}: size exceeds file")));
        }
        flate2::read::ZlibDecoder::new(&data[pos..pos + size])
            .read_to_end(&mut out)
            .map_err(|e| {
                if b == 0 {
                    Error::NotAContainer(format!("first CSF block is not zlib ({e})"))
                } else {
                    Error::Corrupt(format!("CSF block {b}: {e}"))
                }
            })?;
        pos += size;
    }
    Ok(out)
}

/// Index of a record in [`Container::nodes`].
pub type NodeId = usize;

#[derive(Clone, Debug)]
pub struct Node {
    pub ty: u16,
    pub version: i16,
    /// Offset of the body in [`Container::data`].
    pub body: usize,
    pub size: usize,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// A zone left on the disc by [`Container::open_deferred`]: the file
    /// offset of its record HEADER and the header and body's length together
    /// (what [`Container::parse_records`] takes). It has no children and no
    /// bytes here until its library loads it ([`Library::add_zone`]).
    pub deferred: Option<(u64, usize)>,
}

/// The records `open_deferred` walks into without keeping their bytes: the
/// world file's root and the world (whose body is every zone).
const DESCEND: &[u16] = &[0x8000, zone::WORLD];

/// A parsed container. It owns its (decompressed) bytes; nodes are offsets
/// into them.
pub struct Container {
    pub data: Vec<u8>,
    pub nodes: Vec<Node>,
    pub roots: Vec<NodeId>,
    /// DictID → the record it names (a sprite, not its header). The first
    /// registration wins.
    pub dict: HashMap<i32, NodeId>,
    pub surfaces: Vec<Surface>,
}

impl Container {
    /// Opens a container's file bytes, inflating a CSF first.
    pub fn open(file: Vec<u8>) -> Result<Self, Error> {
        let data = if is_csf(&file) { decompress_csf(&file)? } else { file };
        Self::parse(data)
    }

    pub fn parse(data: Vec<u8>) -> Result<Self, Error> {
        if data.len() < 24 {
            return Err(Error::NotAContainer("too small for an OBJF header".into()));
        }
        if &data[..4] != b"FJBO" {
            return Err(Error::NotAContainer("missing OBJF magic".into()));
        }
        let start = i64le(&data, 16);
        if start < 0 || start as usize > data.len() {
            return Err(Error::Corrupt(format!("object tree offset {start}")));
        }
        let mut c = Container::with_data(data);
        let end = c.data.len();
        c.walk_range(start as usize, end, None);
        Ok(c)
    }

    /// A container with no records: what a world change leaves in place of
    /// the old world's zones, so every other file keeps its index.
    pub fn empty() -> Self {
        Container::with_data(Vec::new())
    }

    fn with_data(data: Vec<u8>) -> Self {
        Container { data, nodes: Vec::new(), roots: Vec::new(), dict: HashMap::new(), surfaces: Vec::new() }
    }

    /// Records with no OBJF header: one zone's header and body as
    /// [`Node::deferred`] names them (its roots are that one zone). The
    /// nodes, DictIDs and surfaces are what [`Container::parse`] makes of the
    /// same subtree inside the whole file.
    pub fn parse_records(data: Vec<u8>) -> Self {
        let mut c = Container::with_data(data);
        let end = c.data.len();
        c.walk_range(0, end, None);
        c
    }

    /// Opens an OBJF file through `src` with its zones left on the disc: the
    /// file's root and its world (0x8000, 0x8100) are walked header by header
    /// and keep no bytes, every zone (0x3000) in a world becomes a
    /// [`Node::deferred`] stub, and every other record is read whole and
    /// parsed as [`Container::parse`] parses it. For TUNARIA.ESF that is
    /// 997 MB on the disc and the 18 KB of the world base read. A CSF (small,
    /// and compressed in blocks) is read whole and opened as before. A
    /// `WouldBlock` read comes back as `Error::Io` with nothing kept, for the
    /// caller to fetch the bytes and call again.
    pub fn open_deferred(src: &mut dyn ReadAt) -> Result<Self, Error> {
        let len = src.len();
        let mut head = [0u8; 24];
        if len < 24 {
            return Self::open(read_all(src)?);
        }
        src.read_at(0, &mut head)?;
        if &head[..4] != b"FJBO" {
            return Self::open(read_all(src)?);
        }
        let start = i64le(&head, 16);
        if start < 0 || start as u64 > len {
            return Err(Error::Corrupt(format!("object tree offset {start}")));
        }
        let mut c = Container::with_data(Vec::new());
        // the records walked into: (children left, file offset of the end of the body, node)
        let mut stack: Vec<(u32, u64, NodeId)> = Vec::new();
        let mut pos = start as u64;
        loop {
            while let Some(&(0, body_end, _)) = stack.last() {
                stack.pop();
                pos = pos.max(body_end);
            }
            if pos + 12 > len {
                break;
            }
            let mut h = [0u8; 12];
            src.read_at(pos, &mut h)?;
            let ty = u16le(&h, 0);
            let version = u16le(&h, 2) as i16;
            let size = i32le(&h, 4);
            let n_sub = i32le(&h, 8) as u32;
            if ty == 0 && stack.is_empty() {
                break;
            }
            let body = pos + 12;
            if size < 0 || body + size as u64 > len {
                break;
            }
            let size = size as usize;
            let parent = stack.last().map(|t| t.2);
            let in_world = parent.is_some_and(|p| c.nodes[p].ty == zone::WORLD);
            if DESCEND.contains(&ty) && n_sub > 0 {
                let id = c.attach(Node { ty, version, body: c.data.len(), size: 0, parent, children: Vec::new(), deferred: None });
                if let Some(t) = stack.last_mut() {
                    t.0 -= 1;
                }
                stack.push((n_sub, body + size as u64, id));
                pos = body;
            } else if ty == zone::ZONE && in_world {
                c.attach(Node { ty, version, body: c.data.len(), size: 0, parent, children: Vec::new(), deferred: Some((pos, 12 + size)) });
                if let Some(t) = stack.last_mut() {
                    t.0 -= 1;
                }
                pos = body + size as u64;
            } else {
                let mut rec = vec![0u8; 12 + size];
                src.read_at(pos, &mut rec)?;
                let o = c.data.len();
                c.data.extend_from_slice(&rec);
                c.walk_range(o, o + 12 + size, parent);
                if let Some(t) = stack.last_mut() {
                    t.0 -= 1;
                }
                pos = body + size as u64;
            }
        }
        Ok(c)
    }

    /// Adds a node under its parent (or as a root), returning its id.
    fn attach(&mut self, n: Node) -> NodeId {
        let id = self.nodes.len();
        match n.parent {
            Some(p) => self.nodes[p].children.push(id),
            None => self.roots.push(id),
        }
        self.nodes.push(n);
        id
    }

    /// Iterative walk with an explicit stack (a 43k-record file nests deeply)
    /// over the records in `data[pos..end]`, which hang under `outer` (or are
    /// roots). A record's children begin at its body; once its child count is
    /// used up the cursor jumps to body+size, skipping bytes the children did
    /// not cover. A zero type at the top of a file ends it.
    fn walk_range(&mut self, mut pos: usize, end: usize, outer: Option<NodeId>) {
        // (children left, end of body, node)
        let mut stack: Vec<(u32, usize, NodeId)> = Vec::new();
        loop {
            while let Some(&(0, body_end, _)) = stack.last() {
                stack.pop();
                pos = pos.max(body_end);
            }
            if pos + 12 > end {
                break;
            }
            let d = &self.data;
            let ty = u16le(d, pos);
            let version = u16le(d, pos + 2) as i16;
            let size = i32le(d, pos + 4);
            let n_sub = i32le(d, pos + 8) as u32;
            if ty == 0 && stack.is_empty() && outer.is_none() {
                break;
            }
            let body = pos + 12;
            if size < 0 || body + size as usize > end {
                break;
            }
            let size = size as usize;
            let parent = stack.last().map(|t| t.2).or(outer);
            if let Some(t) = stack.last_mut() {
                t.0 -= 1;
            }
            let id = self.attach(Node { ty, version, body, size, parent, children: Vec::new(), deferred: None });
            // First registration wins (`ParseSurfaceObj` and its siblings
            // `Find` the id before they `Add` it).
            let named = if ID_IN_BODY.contains(&ty) || (ID_IN_BODY_FROM_V2.contains(&ty) && version >= 2) {
                Some(id)
            } else {
                parent.filter(|&p| self.nodes[p].children.len() == 1 && ID_IN_HEADER.contains(&self.nodes[p].ty))
            };
            if let Some(target) = named.filter(|_| size >= 4) {
                let dict_id = i32le(&self.data, body);
                if dict_id != 0 {
                    self.dict.entry(dict_id).or_insert(target);
                }
            }
            if ty == SURFACE {
                self.surfaces.push(Surface::read(&self.data, body));
            }
            if n_sub > 0 {
                stack.push((n_sub, body + size, id));
                pos = body;
            } else {
                pos = body + size;
            }
        }
    }

    /// Whether any zone of this file is still on the disc.
    pub fn is_deferred(&self) -> bool {
        self.nodes.iter().any(|n| n.deferred.is_some())
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    pub fn body(&self, id: NodeId) -> &[u8] {
        let n = &self.nodes[id];
        &self.data[n.body..n.body + n.size]
    }

    /// The first child of `id` with type `ty`.
    pub fn child(&self, id: NodeId, ty: u16) -> Option<NodeId> {
        self.nodes[id].children.iter().copied().find(|&c| self.nodes[c].ty == ty)
    }

    pub fn lookup(&self, dict_id: i32) -> Option<NodeId> {
        self.dict.get(&dict_id).copied()
    }

    /// The surface a DictID names, if this container holds it.
    pub fn surface(&self, dict_id: i32) -> Option<Surface> {
        let id = self.lookup(dict_id)?;
        (self.node(id).ty == SURFACE).then(|| Surface::read(&self.data, self.node(id).body))
    }
}
