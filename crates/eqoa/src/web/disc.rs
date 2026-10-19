//! The player's disc image in a browser: a `File` the page hands over
//! (nothing is uploaded or downloaded), read a slice at a time. Reads there
//! are asynchronous, so a whole file is fetched before the game reads it, and
//! anything that walks the image synchronously (the ISO directory, a world
//! file's record headers) runs against an [`iso::Sparse`] cache that is
//! filled between attempts.

use iso::{Entry, Sparse};
use js_sys::Uint8Array;
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

impl WebDisc {
    pub async fn open(file: web_sys::File) -> Result<Self, String> {
        let len = file.size() as u64;
        let mut d = WebDisc { file, len, entries: Vec::new(), sparse: Sparse::new(len) };
        d.entries = d.drive(iso::list).await.map_err(|e| format!("not an EQOA disc image: {e}"))?;
        Ok(d)
    }

    /// `len` bytes at `off` of the image.
    pub async fn read_at(&self, off: u64, len: usize) -> Result<Vec<u8>, String> {
        let end = (off + len as u64).min(self.len);
        let blob = self.file.slice_with_f64_and_f64(off as f64, end as f64).map_err(err)?;
        let buf = JsFuture::from(blob.array_buffer()).await.map_err(err)?;
        Ok(Uint8Array::new(&buf).to_vec())
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
