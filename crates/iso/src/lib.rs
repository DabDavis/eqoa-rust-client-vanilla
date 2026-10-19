//! ISO 9660 reader for the PS2 disc.
//!
//! The game's assets are ESF/CSF containers inside a plain ISO 9660 image, so a
//! walk of the primary volume descriptor's directory tree names every file on
//! any region or revision of the disc. Reads are seek-and-read: listing a 3 GB
//! image touches a few kilobytes. Ported from the Blender importer's `iso.py`.

use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub const SECTOR: u64 = 2048;
const PVD_LBA: u64 = 16;
const PVD_ROOT_OFFSET: usize = 156;
const DIR_RECORD_MIN: usize = 34;
const FLAG_DIRECTORY: u8 = 0x02;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    NotIso,
    BlockSize(u16),
    NotFound(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::NotIso => write!(f, "no ISO 9660 primary volume descriptor at LBA 16"),
            Error::BlockSize(n) => write!(f, "unsupported logical block size {n}"),
            Error::NotFound(p) => write!(f, "{p}: not on the disc"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

/// One file on the disc. `path` uses `/` and has the `;1` version stripped.
#[derive(Clone, Debug)]
pub struct Entry {
    pub path: String,
    pub lba: u32,
    pub size: u32,
}

pub struct Iso {
    file: File,
    entries: Vec<Entry>,
}

fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

impl Iso {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let mut file = File::open(path)?;
        let mut pvd = vec![0u8; SECTOR as usize];
        read_at(&mut file, PVD_LBA * SECTOR, &mut pvd).map_err(|_| Error::NotIso)?;
        if pvd[0] != 1 || &pvd[1..6] != b"CD001" {
            return Err(Error::NotIso);
        }
        let block = u16::from_le_bytes([pvd[128], pvd[129]]);
        if block as u64 != SECTOR {
            return Err(Error::BlockSize(block));
        }
        let root = &pvd[PVD_ROOT_OFFSET..PVD_ROOT_OFFSET + DIR_RECORD_MIN];
        let (lba, size) = (u32le(root, 2), u32le(root, 10));
        let mut entries = Vec::new();
        walk(&mut file, lba, size, "", &mut entries, 0, &mut HashSet::new());
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Iso { file, entries })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Finds a file by path, ignoring case (the disc's names are upper case and
    /// the game asks for them in mixed case). A bare name with no directory
    /// matches a file of that name anywhere, if exactly one has it.
    pub fn find(&self, path: &str) -> Option<&Entry> {
        let want = path.trim_start_matches(['/', '\\']).replace('\\', "/");
        if let Some(e) = self.entries.iter().find(|e| e.path.eq_ignore_ascii_case(&want)) {
            return Some(e);
        }
        if want.contains('/') {
            return None;
        }
        let mut named = self
            .entries
            .iter()
            .filter(|e| e.path.rsplit('/').next().is_some_and(|n| n.eq_ignore_ascii_case(&want)));
        match (named.next(), named.next()) {
            (Some(e), None) => Some(e),
            _ => None,
        }
    }

    pub fn read(&mut self, path: &str) -> Result<Vec<u8>, Error> {
        let e = self.find(path).ok_or_else(|| Error::NotFound(path.into()))?.clone();
        self.read_entry(&e)
    }

    /// Whole sectors from `lba`, as a drive reads them (CDSTREAM's reads):
    /// past the end of the image the buffer is left as zeros.
    pub fn read_sectors(&mut self, lba: u32, buf: &mut [u8]) -> Result<(), Error> {
        buf.fill(0);
        let off = lba as u64 * SECTOR;
        let len = self.file.metadata()?.len();
        if off >= len {
            return Ok(());
        }
        let n = ((len - off) as usize).min(buf.len());
        read_at(&mut self.file, off, &mut buf[..n])?;
        Ok(())
    }

    pub fn read_entry(&mut self, e: &Entry) -> Result<Vec<u8>, Error> {
        let mut buf = vec![0u8; e.size as usize];
        read_at(&mut self.file, e.lba as u64 * SECTOR, &mut buf)?;
        Ok(buf)
    }
}

fn read_at(f: &mut File, off: u64, buf: &mut [u8]) -> io::Result<()> {
    f.seek(SeekFrom::Start(off))?;
    f.read_exact(buf)
}

/// Appends every file under one directory extent. `seen` guards against a
/// malformed image whose records loop.
fn walk(
    f: &mut File,
    lba: u32,
    size: u32,
    prefix: &str,
    out: &mut Vec<Entry>,
    depth: u32,
    seen: &mut HashSet<u32>,
) {
    if depth > 16 || !seen.insert(lba) {
        return;
    }
    let mut ext = vec![0u8; size as usize];
    if read_at(f, lba as u64 * SECTOR, &mut ext).is_err() {
        return;
    }
    let mut pos = 0usize;
    while pos + DIR_RECORD_MIN <= ext.len() {
        let len = ext[pos] as usize;
        if len == 0 {
            // A record never straddles a sector: zero is padding to the next one.
            pos = (pos / SECTOR as usize + 1) * SECTOR as usize;
            continue;
        }
        if pos + len > ext.len() || len < DIR_RECORD_MIN {
            break;
        }
        let rec = &ext[pos..pos + len];
        pos += len;
        let name_len = rec[32] as usize;
        if 33 + name_len > len {
            break;
        }
        let raw = &rec[33..33 + name_len];
        // "." and ".." are the single bytes 0x00 and 0x01.
        if name_len == 1 && raw[0] <= 1 {
            continue;
        }
        let is_dir = rec[25] & FLAG_DIRECTORY != 0;
        let mut name = String::from_utf8_lossy(raw).into_owned();
        if let (false, Some(semi)) = (is_dir, name.rfind(';')) {
            name.truncate(semi);
        }
        let path = format!("{prefix}{name}");
        if is_dir {
            walk(f, u32le(rec, 2), u32le(rec, 10), &format!("{path}/"), out, depth + 1, seen);
        } else {
            out.push(Entry { path, lba: u32le(rec, 2), size: u32le(rec, 10) });
        }
    }
}
