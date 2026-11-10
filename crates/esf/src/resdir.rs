//! A resource file's own directory.
//!
//! A resource file (CHAR.ESF, the creatures) is an OBJF whose root record
//! (0xA000) holds two: its objects (0xA010, each object a child) and a table
//! of them (0x9000),
//!
//! ```text
//! count i32 | count x { offset u64 | size u32 | DictID u32 }
//! ```
//!
//! each object's record by its place in the file (its header's offset, and
//! the header and body's length together), sorted by DictID. The game opens
//! such a file by its table alone (`VIESFParse::ParseResourceFile(path, 1,
//! ..)`: `ParseResourceDirObj` skips the objects and `ParseResourceTable`
//! reads the table) and then streams one object at a time out of it:
//! `VILoader::Find` binary-searches the table, the object's bytes are read,
//! and they are parsed as a file of their own (`VIObjFile::OpenObject`; here
//! [`Container::parse_records`](crate::Container::parse_records)). This is
//! that, through a [`ReadAt`]: the file's header, three record headers and
//! the table are read (CHAR.ESF: 9.4 KB of its 148 MB), never an object.
//!
//! `tests/resdir.rs` holds it to the whole file's parse: every entry is the
//! record the dictionary names for its id, and every sprite the whole parse
//! finds is an entry.

use crate::{Error, ReadAt, i32le, i64le, u16le};

/// The file type a resource file's header names (`VIObjFile::FileType`), as
/// `ParseResourceFile` asks of it.
const FILE_TYPE: u32 = 0xAB4F;
const ROOT: u16 = 0xA000;
const OBJECTS: u16 = 0xA010;
const TABLE: u16 = 0x9000;

/// `VIResourceElem`: one object's record in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceElem {
    /// the offset of its record's header in the file
    pub offset: u64,
    /// its header and body's length together
    pub size: u32,
    /// its DictID, the search key
    pub id: u32,
}

/// `VIResourceDir`: a resource file's table, as the file has it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResourceDir {
    pub elems: Vec<ResourceElem>,
    /// the file's length: an entry past it names nothing
    pub len: u64,
}

/// A record's header at `pos`: its type and its body's length.
fn header(src: &mut dyn ReadAt, pos: u64, len: u64) -> Result<(u16, u64), Error> {
    if pos.checked_add(12).is_none_or(|end| end > len) {
        return Err(Error::Corrupt(format!("a record header at {pos} of {len} bytes")));
    }
    let mut h = [0u8; 12];
    src.read_at(pos, &mut h)?;
    let size = i32le(&h, 4);
    if size < 0 || pos + 12 + size as u64 > len {
        return Err(Error::Corrupt(format!("the record at {pos}: {size} bytes of body in a file of {len}")));
    }
    Ok((u16le(&h, 0), size as u64))
}

impl ResourceDir {
    /// `VIESFParse::ParseResourceFile` with its objects skipped: the table of
    /// the resource file behind `src`. A file that is no resource file (a
    /// CSF, a world, a file with no table) is `NotAContainer` or `Corrupt`; a
    /// read that fails is `Error::Io` with nothing kept (`WouldBlock`: the
    /// bytes are not there yet, and the call is made again once they are).
    pub fn read(src: &mut dyn ReadAt) -> Result<ResourceDir, Error> {
        let len = src.len();
        if len < 24 {
            return Err(Error::NotAContainer("too small for an OBJF header".into()));
        }
        let mut head = [0u8; 24];
        src.read_at(0, &mut head)?;
        if &head[..4] != b"FJBO" {
            return Err(Error::NotAContainer("missing OBJF magic".into()));
        }
        let ty = i32le(&head, 8) as u32;
        if ty != FILE_TYPE {
            return Err(Error::NotAContainer(format!("file type {ty:04X}")));
        }
        let start = i64le(&head, 16);
        if start < 0 || start as u64 > len {
            return Err(Error::Corrupt(format!("object tree offset {start}")));
        }
        // `ParseResourceDir`: the root; `ParseResourceDirObj`: its objects,
        // skipped (`ReadEnd`), then `ParseResourceTable`
        let root = start as u64;
        match header(src, root, len)? {
            (ROOT, _) => {}
            (ty, _) => return Err(Error::NotAContainer(format!("no resource directory: the root is {ty:04X}"))),
        }
        let objects = root + 12;
        let table = match header(src, objects, len)? {
            (OBJECTS, size) => objects + 12 + size,
            (ty, _) => return Err(Error::NotAContainer(format!("no resource directory: {ty:04X} where the objects are"))),
        };
        let size = match header(src, table, len)? {
            (TABLE, size) => size as usize,
            (ty, _) => return Err(Error::NotAContainer(format!("no resource directory: {ty:04X} where the table is"))),
        };
        let mut body = vec![0u8; size];
        src.read_at(table + 12, &mut body)?;
        let n = if size >= 4 { i32le(&body, 0) } else { -1 };
        if n < 0 || 4 + 16 * n as usize > size {
            return Err(Error::Corrupt(format!("a resource table of {n} entries in {size} bytes")));
        }
        let elems = (0..n as usize)
            .map(|k| {
                let e = &body[4 + 16 * k..20 + 16 * k];
                ResourceElem { offset: i64le(e, 0) as u64, size: i32le(e, 8) as u32, id: i32le(e, 12) as u32 }
            })
            .collect();
        Ok(ResourceDir { elems, len })
    }

    /// `VILoader::Find`: the object `id` names, by binary search (the table is
    /// sorted by DictID; one that is not finds what the game would): its
    /// record's offset in the file and its length. None too for an entry that
    /// reaches past the file or is too short to be a record.
    pub fn find(&self, id: u32) -> Option<(u64, usize)> {
        let (mut lo, mut hi) = (0i64, self.elems.len() as i64 - 1);
        while lo <= hi {
            let mid = (lo + hi) >> 1;
            let e = &self.elems[mid as usize];
            if e.id == id {
                let fits = e.size >= 12 && e.offset.checked_add(e.size as u64).is_some_and(|end| end <= self.len);
                return fits.then_some((e.offset, e.size as usize));
            }
            if id < e.id {
                hi = mid - 1;
            } else {
                lo = mid + 1;
            }
        }
        None
    }
}

/// Whether `rec`, read by its entry, is the sprite record `id` names: a
/// record of type `ty` whose header's length is the entry's, and whose first
/// child (its header record) begins with the id, as the dictionary registers
/// it (`ID_IN_HEADER`). What a directory says is checked before it is used.
pub fn is_sprite(rec: &[u8], ty: u16, id: u32) -> bool {
    rec.len() >= 28 && u16le(rec, 0) == ty && i32le(rec, 4) as i64 == rec.len() as i64 - 12 && i32le(rec, 8) != 0 && i32le(rec, 16) >= 4 && i32le(rec, 24) as u32 == id
}
