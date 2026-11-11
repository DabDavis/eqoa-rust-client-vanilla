//! The disc's art that draws everywhere: a sprite only some zones of a world
//! hold, read off the player's own disc and added to the in-game library, so
//! a creature whose body it is (an NPC's model) draws in every world and
//! every zone, where before it drew only while a zone holding it was loaded.
//!
//! The pieces are a table ([`PIECES`]): a DictID and where its record is (a
//! disc file, and the zone of it when it is a world). Each is one record,
//! whole, with what it registers inside it (its palette's surfaces), cut out
//! of its zone's resource table as the zone's bytes hold it; the records
//! together are a container of their own ([`NAME`]) added after the game's
//! files and the server's patch's art, so it registers only what nothing
//! before it has. A zone holding the same record (Highpass's) ranks at its
//! world's place, ahead of every file, and its own record answers there: the
//! same bytes. go-eqoa-server's `cmd/artbuild` puts the same record into the
//! patch's ss-art.csf for a PS2 (`internal/worlds/art.go`).
//!
//! The records are read once per run and kept ([`cached`]): natively off the
//! image ([`read_native`]), in a page off the player's file (`web`).

use std::sync::Mutex;

/// A sprite of the disc that draws everywhere, and where its record is.
pub struct Piece {
    pub sprite: u32,
    /// The disc file holding it.
    pub from: &'static str,
    /// Its zone (index among the world's zones) when `from` is a world.
    pub zone: usize,
}

/// The pieces, in the order they register.
pub const PIECES: &[Piece] = &[
    // Highpass's bulletin board with papers on it: the Mercenary Recruitment
    // and Real Estate Listings boards (go-eqoa-server's npcs.modelid
    // 638497839), held by 23 Tunaria zones and no other file.
    Piece { sprite: 0x260E_B42F, from: "TUNARIA.ESF", zone: 80 },
];

/// The library's name for the container of the pieces' records.
pub const NAME: &str = "DISCART";

/// The record that registers `sprite` in a zone's records (its header and
/// body as the world file holds them, `Library::zone_range`): its header and
/// body, whole.
pub fn cut(zone: Vec<u8>, sprite: u32) -> Result<Vec<u8>, String> {
    let c = esf::Container::parse_records(zone);
    let n = c.lookup(sprite as i32).ok_or_else(|| format!("{sprite:#010x} is not in the zone"))?;
    let node = c.node(n);
    let (from, to) = (node.body - 12, node.body + node.size);
    Ok(c.data[from..to].to_vec())
}

/// The pieces' records as one container.
pub fn container(records: &[Vec<u8>]) -> esf::Container {
    let mut c = esf::Container::with_capacity(records.iter().map(Vec::len).sum());
    for r in records {
        c.append_records(r);
    }
    c
}

/// The pieces' records, kept once read (a world change, a new login: the same
/// disc).
static KEPT: Mutex<Option<Vec<Vec<u8>>>> = Mutex::new(None);

/// The kept records, if read.
pub fn cached() -> Option<Vec<Vec<u8>>> {
    KEPT.lock().ok()?.clone()
}

/// Keeps `records` for the run.
pub fn keep(records: Vec<Vec<u8>>) {
    if let Ok(mut k) = KEPT.lock() {
        *k = Some(records);
    }
}

/// Adds the pieces' records to `lib` (after the files it already has: what
/// they register wins). A library that has them already is left as it is.
pub fn add(lib: &mut esf::Library, records: &[Vec<u8>]) -> Option<usize> {
    if records.is_empty() || lib.file_named(NAME).is_some() {
        return None;
    }
    Some(lib.add(NAME, container(records)))
}

/// The pieces' records off the image at `iso_path`, each cut out of its
/// zone; a piece that cannot be read is said and left out.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_native(iso_path: &std::path::Path) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut worlds: Vec<(&str, esf::Container)> = Vec::new();
    for p in PIECES {
        let r = (|| -> Result<Vec<u8>, String> {
            let e = iso::Iso::open(iso_path).map_err(|e| e.to_string())?.find(p.from).cloned().ok_or_else(|| format!("{}: not on the disc", p.from))?;
            let base = e.lba as u64 * iso::SECTOR;
            if !worlds.iter().any(|w| w.0 == p.from) {
                let mut src = esf::Window::new(iso::open_file(iso_path).map_err(|e| e.to_string())?, base, e.size as u64);
                worlds.push((p.from, esf::Container::open_deferred(&mut src).map_err(|e| e.to_string())?));
            }
            let w = &worlds.iter().find(|w| w.0 == p.from).unwrap().1;
            let z = *w.zones().get(p.zone).ok_or_else(|| format!("{} has no zone {}", p.from, p.zone))?;
            let (off, len) = w.node(z).deferred.ok_or_else(|| format!("{} zone {} is not on the disc", p.from, p.zone))?;
            let mut b = vec![0u8; len];
            iso::read_exact_at(&std::fs::File::open(iso_path).map_err(|e| e.to_string())?, base + off, &mut b).map_err(|e| e.to_string())?;
            cut(b, p.sprite)
        })();
        match r {
            Ok(b) => out.push(b),
            Err(e) => eprintln!("discart: {:#010x}: {e}", p.sprite),
        }
    }
    out
}

/// The pieces' records for the run: kept, else read off the image now.
#[cfg(not(target_arch = "wasm32"))]
pub fn records(iso_path: &std::path::Path) -> Vec<Vec<u8>> {
    if let Some(r) = cached() {
        return r;
    }
    let r = read_native(iso_path);
    if r.len() == PIECES.len() {
        keep(r.clone());
    }
    r
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::path::Path;

    fn iso() -> Option<std::path::PathBuf> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        p.is_file().then_some(p)
    }

    /// The board's record comes off the disc whole (56,147 bytes, as
    /// go-eqoa-server's artbuild cut it), and once added it resolves to a
    /// model whose every texture the library has; a library without it (the
    /// control) does not resolve the board. Needs the disc.
    #[test]
    fn the_board_off_the_disc_resolves() {
        let Some(iso) = iso() else {
            eprintln!("SKIP: no disc");
            return;
        };
        let t = std::time::Instant::now();
        let r = read_native(&iso);
        eprintln!("read {} records ({} bytes) in {} ms", r.len(), r.iter().map(Vec::len).sum::<usize>(), t.elapsed().as_millis());
        assert_eq!(r.len(), PIECES.len());
        assert_eq!(r[0].len(), 56_147);
        let board = PIECES[0].sprite as i32;
        let mut lib = esf::Library::new();
        lib.add("CHAR.ESF", crate::stream::open_file(&iso, "CHAR.ESF").unwrap());
        assert!(lib.lookup(board).is_none(), "the control: CHAR.ESF has no board");
        let f = add(&mut lib, &r).unwrap();
        assert!(add(&mut lib, &r).is_none(), "added once");
        let at = lib.lookup(board).expect("the board resolves");
        assert_eq!(at.file, f);
        let m = lib.build_model(at).expect("the board builds");
        assert!(m.triangles > 0 && m.missing == 0);
        let textures: Vec<i32> = m.batches.iter().map(|b| b.texture()).filter(|&t| t != 0).collect();
        assert!(!textures.is_empty());
        // every texture is had: the record's own, or a file before it
        // registered the same DictID as the same bytes (CHAR.ESF has some)
        let ours = lib.file(f);
        let bytes = |c: &esf::Container, n: esf::NodeId| {
            let n = c.node(n);
            c.data[n.body - 12..n.body + n.size].to_vec()
        };
        for t in textures {
            assert!(lib.surface(t).is_some(), "texture {t:#x}");
            let at = lib.lookup(t).unwrap();
            if at.file != f {
                assert_eq!(bytes(lib.file(at.file), at.node), bytes(ours, ours.lookup(t).unwrap()), "{t:#x} as other bytes");
            }
        }
    }

    /// In Tunaria with Highpass's zone loaded the zone's own record answers
    /// (it ranks at its world's place, ahead of every file) and it is the
    /// same bytes as ours: nothing is registered twice, whichever is first.
    #[test]
    fn highpass_s_zone_answers_its_own_same_record() {
        let Some(iso) = iso() else {
            eprintln!("SKIP: no disc");
            return;
        };
        let r = read_native(&iso);
        let board = PIECES[0].sprite as i32;
        let mut lib = esf::Library::new();
        let w = lib.add("TUNARIA.ESF", crate::stream::open_file(&iso, "TUNARIA.ESF").unwrap());
        let ours = add(&mut lib, &r).unwrap();
        assert_eq!(lib.lookup(board).unwrap().file, ours, "no zone loaded yet: ours");
        let (off, len) = lib.zone_range(w, 80).unwrap();
        let e = iso::Iso::open(&iso).unwrap().find("TUNARIA.ESF").cloned().unwrap();
        let mut b = vec![0u8; len];
        iso::read_exact_at(&std::fs::File::open(&iso).unwrap(), e.lba as u64 * iso::SECTOR + off, &mut b).unwrap();
        let z = lib.add_zone(w, 80, esf::Container::parse_records(b)).unwrap();
        let at = lib.lookup(board).unwrap();
        assert_eq!(at.file, z, "the zone ranks ahead");
        let n = lib.file(z).node(at.node);
        assert_eq!(&lib.file(z).data[n.body - 12..n.body + n.size], &r[0][..]);
    }
}
