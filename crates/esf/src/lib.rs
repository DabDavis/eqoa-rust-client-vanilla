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

mod mesh;
mod surface;
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
    pub const SIMPLESPRITE: u16 = 0x2000;
    pub const SIMPLESPRITE_HDR: u16 = 0x2001;
    pub const HSPRITE: u16 = 0x2200;
    pub const HSPRITE_HDR: u16 = 0x2210;
    pub const HSPRITE_ARRAY: u16 = 0x2220;
    pub const HSPRITE_HIERARCHY: u16 = 0x2400;
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

/// Types whose body starts with a DictID.
const HAS_DICTID: &[u16] = &[
    SURFACE, MATPAL_HDR, PRIMBUF, 0x1220, 0x1230, SIMPLESPRITE_HDR, HSPRITE_HDR,
    SKINSUBSPRITE, 0x5000, CSPRITE_HDR, LODSPRITE_HDR, GROUPSPRITE_HDR, 0x2E00, 0x2F01,
    0x4200, 0xB101, 0xC010, 0xC101, 0xC210, 0xC310,
];
/// Header types whose DictID names their PARENT, not themselves.
const IS_ID_CONTAINER: &[u16] = &[
    MATPAL_HDR, SIMPLESPRITE_HDR, HSPRITE_HDR, CSPRITE_HDR, GROUPSPRITE_HDR, 0x2F01,
    0xB101, 0xC010, 0xC101, 0xC210, 0xC310,
];

const CSF_HEADER_SIZE: usize = 40;

#[derive(Debug)]
pub enum Error {
    NotAContainer(String),
    Corrupt(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::NotAContainer(s) => write!(f, "not an EQOA container: {s}"),
            Error::Corrupt(s) => write!(f, "corrupt container: {s}"),
        }
    }
}

impl std::error::Error for Error {}

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
}

/// A parsed container. It owns its (decompressed) bytes; nodes are offsets
/// into them.
pub struct Container {
    pub data: Vec<u8>,
    pub nodes: Vec<Node>,
    pub roots: Vec<NodeId>,
    /// DictID → the record it names. The first registration wins.
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
        let mut c = Container {
            data,
            nodes: Vec::new(),
            roots: Vec::new(),
            dict: HashMap::new(),
            surfaces: Vec::new(),
        };
        c.walk(start as usize);
        Ok(c)
    }

    /// Iterative walk with an explicit stack (a 43k-record file nests deeply).
    /// A record's children begin at its body; once its child count is used up
    /// the cursor jumps to body+size, skipping bytes the children did not cover.
    fn walk(&mut self, mut pos: usize) {
        let end = self.data.len();
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
            if ty == 0 && stack.is_empty() {
                break;
            }
            let body = pos + 12;
            if size < 0 || body + size as usize > end {
                break;
            }
            let size = size as usize;
            let id = self.nodes.len();
            let parent = stack.last().map(|t| t.2);
            self.nodes.push(Node { ty, version, body, size, parent, children: Vec::new() });
            match parent {
                Some(p) => {
                    stack.last_mut().unwrap().0 -= 1;
                    self.nodes[p].children.push(id);
                }
                None => self.roots.push(id),
            }
            if HAS_DICTID.contains(&ty) && size >= 4 {
                let dict_id = i32le(&self.data, body);
                if dict_id != 0 {
                    let target = match parent {
                        Some(p) if IS_ID_CONTAINER.contains(&ty) => p,
                        _ => id,
                    };
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
