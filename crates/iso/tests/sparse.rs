//! The browser's way to the disc: a `Sparse` cache that a caller fills
//! between attempts, driven to the same listing and bytes as the file.

use iso::{Iso, Source, Sparse};
use std::io;

fn image() -> Option<std::fs::File> {
    let path = std::env::var("EQOA_ISO").unwrap_or_else(|_| "../../run/eqoa.iso".into());
    match std::fs::File::open(&path) {
        Ok(f) => Some(f),
        Err(_) => {
            eprintln!("SKIPPED: no disc image at {path}");
            None
        }
    }
}

/// Runs `f` against the cache, fetching what it lacks from `file`, until it
/// stops asking; returns its result and the number of fetches.
fn drive<T>(file: &mut std::fs::File, s: &mut Sparse, mut f: impl FnMut(&mut Sparse) -> Result<T, iso::Error>) -> (Result<T, iso::Error>, usize) {
    let mut fetches = 0;
    loop {
        match f(s) {
            Err(iso::Error::Io(e)) if e.kind() == io::ErrorKind::WouldBlock => {
                for (off, n) in std::mem::take(&mut s.missing) {
                    let mut b = vec![0u8; n as usize];
                    file.read_at(off, &mut b).unwrap();
                    s.fill(off, &b);
                    fetches += 1;
                }
            }
            r => return (r, fetches),
        }
    }
}

#[test]
fn listing_through_the_cache_equals_the_file() {
    let Some(mut f) = image() else { return };
    let direct = Iso::from_source(f.try_clone().unwrap()).unwrap();
    let mut s = Sparse::new(f.size().unwrap());
    let (entries, fetches) = drive(&mut f, &mut s, iso::list);
    let entries = entries.unwrap();
    assert_eq!(entries.len(), direct.entries().len());
    for (a, b) in entries.iter().zip(direct.entries()) {
        assert_eq!((&a.path, a.lba, a.size), (&b.path, b.lba, b.size));
    }
    eprintln!("{} files in {fetches} fetches, {} bytes held", entries.len(), s.held());
    assert!(s.held() < 4 << 20, "listing should touch little of the image");
}

#[test]
fn reads_through_the_cache_equal_the_file() {
    let Some(mut f) = image() else { return };
    let mut direct = Iso::from_source(f.try_clone().unwrap()).unwrap();
    let e = direct.find("WORDS.TXT").unwrap().clone();
    let want = direct.read_entry(&e).unwrap();
    let mut s = Sparse::new(f.size().unwrap());
    // a read that straddles chunks and starts mid-chunk
    let off = e.lba as u64 * iso::SECTOR + 1000;
    let (got, _) = drive(&mut f, &mut s, |s| {
        let mut b = vec![0u8; 200_000.min(e.size as usize - 1000)];
        s.read_at(off, &mut b)?;
        Ok(b)
    });
    assert_eq!(got.unwrap(), want[1000..1000 + 200_000.min(e.size as usize - 1000)]);
    // negative control: a chunk filled with the wrong bytes shows
    let mut bad = Sparse::new(f.size().unwrap());
    let (k, _) = (off / Sparse::CHUNK * Sparse::CHUNK, 0);
    bad.fill(k, &vec![0xAA; Sparse::CHUNK as usize]);
    let (got, _) = drive(&mut f, &mut bad, |s| {
        let mut b = vec![0u8; 64];
        s.read_at(off, &mut b)?;
        Ok(b)
    });
    assert_ne!(got.unwrap(), want[1000..1064]);
}
