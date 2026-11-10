//! A resource file's own directory (`esf::ResourceDir`, the game's
//! `VIResourceDir`) against the same file read and parsed whole: every entry
//! is the record the whole file's dictionary names for its id, at its place
//! and of its length; every sprite the whole parse finds is an entry; an
//! object read by its entry and parsed alone is the whole parse's subtree;
//! and only a few kilobytes are read for it. A file that is no resource file
//! is refused, and bytes not there yet pass through for the read to be made
//! again. Each check has a control that must fail.

use esf::{Container, Error, ReadAt, ResourceDir, Window};
use std::fs::File;

/// A source that counts what is read through it.
struct Counting<R> {
    inner: R,
    bytes: u64,
    reads: u64,
}

impl<R: ReadAt> ReadAt for Counting<R> {
    fn len(&self) -> u64 {
        self.inner.len()
    }
    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.bytes += buf.len() as u64;
        self.reads += 1;
        self.inner.read_at(off, buf)
    }
}

/// The Frontiers disc (`$EQOA_ISO`, else `run/eqoa.iso`), and the 2002 disc
/// when there is one (`$EQOA_2002_ISO`, else the USA disc under
/// `../decomp-builds/discs/eqoa-usa/`).
fn discs() -> Vec<(&'static str, std::path::PathBuf)> {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out = Vec::new();
    let frontiers = std::env::var_os("EQOA_ISO").map(std::path::PathBuf::from).unwrap_or_else(|| root.join("run/eqoa.iso"));
    if frontiers.exists() {
        out.push(("Frontiers", frontiers));
    }
    let y2002 = std::env::var_os("EQOA_2002_ISO").map(std::path::PathBuf::from).or_else(|| {
        std::fs::read_dir(root.join("../decomp-builds/discs/eqoa-usa")).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")))
    });
    if let Some(p) = y2002.filter(|p| p.exists()) {
        out.push(("2002", p));
    }
    out
}

/// The whole parse's nodes under `n` (and `n`), in the order parsed, as (type,
/// version, body's length, children, the body's place past `n`'s header).
fn subtree(c: &Container, n: usize, count: usize) -> Vec<(u16, i16, usize, usize, usize)> {
    let base = c.node(n).body - 12;
    (n..n + count).map(|k| c.node(k)).map(|x| (x.ty, x.version, x.size, x.children.len(), x.body - base)).collect()
}

/// CHAR.ESF's directory on each disc at hand against the whole file. Needs
/// the disc (`run/eqoa.iso`); says so and passes without.
#[test]
fn the_directory_is_the_whole_files() {
    let discs = discs();
    if discs.is_empty() {
        eprintln!("SKIP: no disc (set EQOA_ISO)");
        return;
    }
    for (which, path) in discs {
        let mut disc = iso::Iso::open(&path).unwrap();
        let e = disc.find("CHAR.ESF").expect("CHAR.ESF on the disc").clone();
        let mut src = Counting { inner: Window::new(File::open(&path).unwrap(), e.lba as u64 * iso::SECTOR, e.size as u64), bytes: 0, reads: 0 };
        let dir = ResourceDir::read(&mut src).unwrap_or_else(|x| panic!("{which}: {x}"));
        assert!(src.reads <= 5 && src.bytes < 64 * 1024, "{which}: {} reads, {} bytes for the directory", src.reads, src.bytes);
        eprintln!("{which}: CHAR.ESF {} bytes, its directory {} entries in {} reads of {} bytes", e.size, dir.elems.len(), src.reads, src.bytes);
        assert_eq!(dir.len, e.size as u64);
        let whole = Container::open(disc.read_entry(&e).unwrap()).unwrap();
        // sorted by id, as `VILoader::Find` needs
        assert!(dir.elems.windows(2).all(|w| w[0].id < w[1].id), "{which}: sorted by DictID");
        // every entry: the record the dictionary names for its id, a child of
        // the objects' record, at its place and of its length
        let objects = whole.child(whole.roots[0], 0xA010).expect("the objects' record");
        let (mut sprites, mut simple) = (0, 0);
        for el in &dir.elems {
            let n = whole.lookup(el.id as i32).unwrap_or_else(|| panic!("{which}: {:08X} in the directory, not in the file", el.id));
            let node = whole.node(n);
            assert_eq!(node.parent, Some(objects), "{which}: {:08X}", el.id);
            assert_eq!(((node.body - 12) as u64, 12 + node.size), (el.offset, el.size as usize), "{which}: {:08X}", el.id);
            assert_eq!(dir.find(el.id), Some((el.offset, el.size as usize)));
            match node.ty {
                0x2700 => sprites += 1,
                0x2000 => simple += 1,
                ty => panic!("{which}: {:08X} is a {ty:04X}", el.id),
            }
        }
        // every sprite the whole parse finds is an entry, and every object
        let in_file = whole.nodes.iter().filter(|n| n.ty == 0x2700).count();
        assert_eq!((sprites, sprites + simple), (in_file, whole.node(objects).children.len()), "{which}");
        assert!(sprites > 300, "{which}: {sprites} sprites");
        // the controls: an id no object has, a surface's id (in the file's
        // dictionary, not an object), and an id one off an object's
        let surface = whole.dict.iter().find(|&(_, &n)| whole.node(n).ty == 0x1000).map(|(&id, _)| id as u32).expect("a surface");
        assert!(dir.elems.iter().all(|el| el.id != surface));
        for id in [0x1234_5678, surface, dir.elems[0].id.wrapping_sub(1), 0] {
            assert_eq!(dir.find(id), None, "{which}: {id:08X}");
        }
        // an object read by its entry is the whole file's bytes, the sprite
        // its id names, and parsed alone the whole parse's subtree
        let mut read = 0u64;
        for el in dir.elems.iter().step_by(7).chain(dir.elems.last()) {
            let (off, size) = dir.find(el.id).unwrap();
            let mut rec = vec![0u8; size];
            src.inner.read_at(off, &mut rec).unwrap();
            read += size as u64;
            assert_eq!(rec, whole.data[off as usize..off as usize + size], "{which}: {:08X}", el.id);
            let n = whole.lookup(el.id as i32).unwrap();
            let ty = whole.node(n).ty;
            assert!(esf::resdir::is_sprite(&rec, ty, el.id), "{which}: {:08X}", el.id);
            assert!(!esf::resdir::is_sprite(&rec, ty, el.id ^ 1) && !esf::resdir::is_sprite(&rec, ty ^ 0x0800, el.id) && !esf::resdir::is_sprite(&rec[..size - 1], ty, el.id), "the controls");
            let alone = Container::parse_records(rec);
            assert_eq!(alone.roots, [0]);
            assert_eq!(subtree(&alone, 0, alone.nodes.len()), subtree(&whole, n, alone.nodes.len()), "{which}: {:08X}", el.id);
            assert_eq!(alone.lookup(el.id as i32), Some(0));
        }
        assert!(read < e.size as u64 / 4, "{which}: the sample read {read} bytes");
    }
}

/// A record's header and body.
fn record(ty: u16, children: u32, body: &[u8]) -> Vec<u8> {
    let mut r = ty.to_le_bytes().to_vec();
    r.extend(0i16.to_le_bytes());
    r.extend((body.len() as i32).to_le_bytes());
    r.extend(children.to_le_bytes());
    r.extend(body);
    r
}

/// A resource file of sprites (`(id, filler)`, a 0x2700 with its 0x2710
/// header each), its table as `table` makes it of each object's (offset,
/// size, id).
fn resource_file(objects: &[(u32, usize)], table: impl Fn(Vec<(u64, u32, u32)>) -> Vec<u8>) -> Vec<u8> {
    let mut objs = Vec::new();
    let mut places = Vec::new();
    // the file's header, the root's and the objects' come before
    let base = 24 + 12 + 12;
    for &(id, fill) in objects {
        let mut hdr = id.to_le_bytes().to_vec();
        hdr.extend(vec![0xEE; fill]);
        let rec = record(0x2700, 1, &record(0x2710, 0, &hdr));
        places.push(((base + objs.len()) as u64, rec.len() as u32, id));
        objs.extend(rec);
    }
    let mut root = record(0xA010, objects.len() as u32, &objs);
    root.extend(record(0x9000, 0, &table(places)));
    let mut f = b"FJBO".to_vec();
    f.extend(1u32.to_le_bytes());
    f.extend(0xAB4Fu32.to_le_bytes());
    f.extend(0u32.to_le_bytes());
    f.extend(24i64.to_le_bytes());
    f.extend(record(0xA000, 2, &root));
    f
}

fn table_of(entries: &[(u64, u32, u32)]) -> Vec<u8> {
    let mut t = (entries.len() as i32).to_le_bytes().to_vec();
    for &(off, size, id) in entries {
        t.extend(off.to_le_bytes());
        t.extend(size.to_le_bytes());
        t.extend(id.to_le_bytes());
    }
    t
}

/// A made resource file: its directory names each object where the whole
/// parse puts it, ids above 0x7FFFFFFF sort last (the search is unsigned, as
/// `VILoader::Find`'s), and an entry that reaches past the file or is no
/// record names nothing.
#[test]
fn a_made_resource_file() {
    let objects = [(0x0000_0010, 5), (0x4000_0000, 0), (0x9000_0001, 40), (0xFFB1_EA2C, 9)];
    let file = resource_file(&objects, |p| table_of(&p));
    let dir = ResourceDir::read(&mut file.clone()).unwrap();
    let whole = Container::open(file.clone()).unwrap();
    assert_eq!(dir.elems.len(), 4);
    for &(id, _) in &objects {
        let (off, size) = dir.find(id).unwrap_or_else(|| panic!("{id:08X}"));
        let n = whole.lookup(id as i32).unwrap();
        assert_eq!((off as usize, size), (whole.node(n).body - 12, 12 + whole.node(n).size), "{id:08X}");
        assert!(esf::resdir::is_sprite(&file[off as usize..off as usize + size], 0x2700, id));
    }
    assert_eq!(dir.find(0x4000_0001), None, "the control: no such object");
    // an entry past the file's end, and one too short to be a record
    let bad = resource_file(&objects, |mut p| {
        p[1].0 = 1 << 40;
        p[2].1 = 11;
        table_of(&p)
    });
    let dir = ResourceDir::read(&mut bad.clone()).unwrap();
    assert!(dir.find(0x0000_0010).is_some() && dir.find(0xFFB1_EA2C).is_some());
    assert_eq!((dir.find(0x4000_0000), dir.find(0x9000_0001)), (None, None));
}

/// What is no resource file is refused, each for its reason, and nothing is
/// read past what says so. The control is the same file as made.
#[test]
fn what_is_no_resource_file_is_refused() {
    let objects = [(7u32, 3usize), (9, 0)];
    let good = resource_file(&objects, |p| table_of(&p));
    assert!(ResourceDir::read(&mut good.clone()).is_ok(), "the control");
    let refused = |f: Vec<u8>| match ResourceDir::read(&mut f.clone()) {
        Err(Error::NotAContainer(s)) | Err(Error::Corrupt(s)) => s,
        Err(Error::Io(e)) => panic!("an I/O error: {e}"),
        Ok(d) => panic!("read {} entries", d.elems.len()),
    };
    refused(good[..20].to_vec());
    let mut f = good.clone();
    f[0] = b'X';
    assert!(refused(f).contains("magic"));
    // a CSF (compressed): no place in it can be read by position
    assert!(refused(vec![0x78; 4096]).contains("magic"));
    let mut f = good.clone();
    f[8] = 0x50;
    assert!(refused(f).contains("file type"));
    let mut f = good.clone();
    f[16..24].copy_from_slice(&(1i64 << 40).to_le_bytes());
    assert!(refused(f).contains("offset"));
    // a world's root, not a resource directory's
    let mut f = good.clone();
    f[24..26].copy_from_slice(&0x8000u16.to_le_bytes());
    assert!(refused(f).contains("root is 8000"));
    let mut f = good.clone();
    f[36..38].copy_from_slice(&0x2700u16.to_le_bytes());
    assert!(refused(f).contains("where the objects are"));
    // the objects' record claims more than the file holds
    let mut f = good.clone();
    f[40..44].copy_from_slice(&(1i32 << 30).to_le_bytes());
    refused(f);
    // no table after the objects, a table cut short, and a count its bytes do not hold
    let at = good.len() - (12 + 4 + 16 * objects.len());
    let mut f = good.clone();
    f[at..at + 2].copy_from_slice(&0x9001u16.to_le_bytes());
    assert!(refused(f).contains("where the table is"));
    refused(good[..good.len() - 1].to_vec());
    let mut f = good.clone();
    f[at + 12..at + 16].copy_from_slice(&3i32.to_le_bytes());
    assert!(refused(f).contains("3 entries"));
    let mut f = good.clone();
    f[at + 12..at + 16].copy_from_slice(&(-1i32).to_le_bytes());
    refused(f);
}

/// A source whose bytes past `have` are not there yet (a browser's disc).
struct Partial {
    data: Vec<u8>,
    have: usize,
    asked: Vec<(u64, usize)>,
}

impl ReadAt for Partial {
    fn len(&self) -> u64 {
        self.data.len() as u64
    }
    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> std::io::Result<()> {
        self.asked.push((off, buf.len()));
        if off as usize + buf.len() > self.have {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }
        buf.copy_from_slice(&self.data[off as usize..off as usize + buf.len()]);
        Ok(())
    }
}

/// Bytes not there yet come back as `WouldBlock`, for the caller to fetch
/// them and read again from the start (a page's disc); with them all there
/// the directory is the same. The control: any other failure is not a wait.
#[test]
fn bytes_not_there_yet_are_waited_for() {
    let file = resource_file(&[(3, 100), (5, 100), (8, 100)], |p| table_of(&p));
    let whole = ResourceDir::read(&mut file.clone()).unwrap();
    let mut src = Partial { data: file.clone(), have: 0, asked: Vec::new() };
    let mut waits = 0;
    let dir = loop {
        match ResourceDir::read(&mut src) {
            Ok(d) => break d,
            Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // what the read lacked is fetched
                let &(off, n) = src.asked.last().unwrap();
                src.have = src.have.max(off as usize + n);
                waits += 1;
            }
            Err(e) => panic!("{e}"),
        }
    };
    assert_eq!(dir, whole);
    assert!((2..=5).contains(&waits), "{waits} waits");
    // no read of the objects themselves: the file's header, three record
    // headers and the table
    let table_at = file.len() - (4 + 16 * 3);
    assert!(src.asked.iter().all(|&(off, n)| off as usize + n <= 48 || off as usize >= table_at - 12), "{:?}", src.asked);
    // the control: a read that fails for good
    struct Broken;
    impl ReadAt for Broken {
        fn len(&self) -> u64 {
            4096
        }
        fn read_at(&mut self, _: u64, _: &mut [u8]) -> std::io::Result<()> {
            Err(std::io::ErrorKind::PermissionDenied.into())
        }
    }
    assert!(matches!(ResourceDir::read(&mut Broken), Err(Error::Io(e)) if e.kind() == std::io::ErrorKind::PermissionDenied));
}
