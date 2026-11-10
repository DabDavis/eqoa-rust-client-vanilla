//! The pregame's files off the disc, asked for by their paths on it
//! (`DATA2/SETUP.CSF`) and handed back whole once read: natively on a thread
//! of their own, so the window keeps drawing (the console draws its loading
//! screen meanwhile), in a page by the page's own reader (`set_reader`),
//! whose reads finish on later frames.
//!
//! And objects out of a resource file (CHAR.ESF's bodies, `cut`), read as
//! the console streams them: found in the file's own directory
//! (`esf::ResourceDir`, the game's `VILoader::Find`) and read one record at
//! a time, each parsed as it comes, off the frame's thread (natively a
//! thread, in a page its reader's task between frames, `set_cutter`); never
//! the file whole, but when it has no directory to go by.

use std::cell::RefCell;
use std::rc::Rc;

/// The files of one request, in the order asked, or why they could not be read.
pub type Files = Result<Vec<(String, Vec<u8>)>, String>;

/// A request a page's reader answers into: filled on a later frame.
pub type Slot = Rc<RefCell<Option<Files>>>;

/// A page's reader: starts reading the paths and fills the slot it returns.
pub type Reader = Box<dyn Fn(&[String]) -> Slot>;

thread_local! {
    static READER: RefCell<Option<Reader>> = const { RefCell::new(None) };
}

/// Makes `r` the way the pregame's files are read (a page's).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn set_reader(r: Reader) {
    READER.with(|c| *c.borrow_mut() = Some(r));
}

/// Files being read.
pub enum Pending {
    Slot(Slot),
    #[cfg(not(target_arch = "wasm32"))]
    Thread(std::sync::mpsc::Receiver<Files>),
    /// in hand already (a page's refusal)
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Done(Option<Files>),
}

impl Pending {
    /// The files, once, when they have been read.
    pub fn poll(&mut self) -> Option<Files> {
        match self {
            Pending::Slot(s) => s.borrow_mut().take(),
            #[cfg(not(target_arch = "wasm32"))]
            Pending::Thread(rx) => match rx.try_recv() {
                Ok(f) => Some(f),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("the reading thread stopped".into())),
            },
            Pending::Done(f) => f.take(),
        }
    }
}

/// Starts reading `paths` off the disc.
pub fn read(paths: &[&str]) -> Pending {
    let paths: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    if let Some(s) = READER.with(|c| c.borrow().as_ref().map(|f| f(&paths))) {
        return Pending::Slot(s);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let iso = crate::disc_path(None);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let go = || -> Files {
                let mut d = iso::Iso::open(&iso).map_err(|e| format!("{iso}: {e}"))?;
                paths.iter().map(|p| d.read(p).map(|b| (p.clone(), b)).map_err(|e| format!("{p}: {e}"))).collect()
            };
            let _ = tx.send(go());
        });
        Pending::Thread(rx)
    }
    #[cfg(target_arch = "wasm32")]
    Pending::Done(Some(Err(format!("no reader for {paths:?}"))))
}

/// The record type `cut` takes: a character sprite (a body).
const BODY: u16 = 0x2700;

/// Objects asked of a resource file on a disc (`cut`): the disc (the one the
/// game runs off, or the 2002 disc for its art, `era`), the file as the game
/// names it (the 2002 disc's is found by its last part, wherever that disc
/// keeps it), and the bodies wanted by DictID, a group a container, each
/// group's in the order their records go into it.
#[derive(Clone, Debug, PartialEq)]
pub struct CutAsk {
    pub era: esf::Era,
    pub path: String,
    pub groups: Vec<Vec<u32>>,
}

/// How a cut's records were found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Found {
    /// by the file's own directory, each record read alone
    Directory,
    /// in the whole file, read and parsed (a file with no directory)
    Whole,
}

/// What came of a `CutAsk`: each group's container (its bodies' records, each
/// whole, header and all, in the group's order; None for a group the file has
/// none of), how they were found, and the reads made of the disc for them.
pub struct Cut {
    pub groups: Vec<Option<esf::Container>>,
    pub found: Found,
    pub reads: u32,
    pub bytes: u64,
}

pub type CutResult = Result<Cut, String>;

/// A cut a page's reader answers into: filled on a later frame.
pub type CutSlot = Rc<RefCell<Option<CutResult>>>;

/// A page's reader of cuts: starts on the ask and fills the slot it returns.
pub type CutReader = Box<dyn Fn(CutAsk) -> CutSlot>;

thread_local! {
    static CUTTER: RefCell<Option<CutReader>> = const { RefCell::new(None) };
}

/// Makes `r` the way cuts are read (a page's).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn set_cutter(r: CutReader) {
    CUTTER.with(|c| *c.borrow_mut() = Some(r));
}

/// A cut being read.
pub enum PendingCut {
    Slot(CutSlot),
    #[cfg(not(target_arch = "wasm32"))]
    Thread(std::sync::mpsc::Receiver<CutResult>),
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Done(Option<CutResult>),
}

impl PendingCut {
    /// The cut, once, when it has been read.
    pub fn poll(&mut self) -> Option<CutResult> {
        match self {
            PendingCut::Slot(s) => s.borrow_mut().take(),
            #[cfg(not(target_arch = "wasm32"))]
            PendingCut::Thread(rx) => match rx.try_recv() {
                Ok(c) => Some(c),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("the reading thread stopped".into())),
            },
            PendingCut::Done(c) => c.take(),
        }
    }
}

/// Starts cutting what `ask` asks for off its disc.
pub fn cut(ask: CutAsk) -> PendingCut {
    if let Some(s) = CUTTER.with(|c| c.borrow().as_ref().map(|f| f(ask.clone()))) {
        return PendingCut::Slot(s);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(cut_now(&ask));
        });
        PendingCut::Thread(rx)
    }
    #[cfg(target_arch = "wasm32")]
    PendingCut::Done(Some(Err(format!("no reader for {}", ask.path))))
}

/// `ask` cut now, natively: the disc's image opened (the 2002 disc judged
/// first, as every read of it is), the file found on it and read through its
/// directory (`cut_from`). A 2002 disc without the file has none of the
/// bodies (Frontiers' stay); the disc the game runs off must have it.
#[cfg(not(target_arch = "wasm32"))]
pub fn cut_now(ask: &CutAsk) -> CutResult {
    let image = crate::era::image_of(ask.era).ok_or("no 2002 disc image (F9 asks for it)")?;
    let mut d = iso::Iso::open(&image).map_err(|e| format!("{image}: {e}"))?;
    let name = match ask.era {
        esf::Era::Y2002 => {
            crate::era::judge_image(&mut d)?;
            super::res::name_of(&ask.path)
        }
        esf::Era::Frontiers => ask.path.as_str(),
    };
    let Some(e) = d.find(name).cloned() else {
        return match ask.era {
            esf::Era::Y2002 => Ok(Cut { groups: ask.groups.iter().map(|_| None).collect(), found: Found::Directory, reads: 0, bytes: 0 }),
            esf::Era::Frontiers => Err(format!("{name}: not on the disc")),
        };
    };
    let file = iso::open_file(&image).map_err(|e| format!("{image}: {e}"))?;
    let mut src = esf::Window::new(file, e.lba as u64 * iso::SECTOR, e.size as u64);
    cut_from(&mut src, &ask.groups).map_err(|x| format!("{name}: {x}"))
}

/// The bodies `groups` name cut out of the resource file behind `src`: by
/// its directory, each record read alone, in the file's order (a drive seeks
/// the least) and put in the groups'; a file with no directory, or one whose
/// directory names what is not there, is read whole and cut as it always was
/// (`cut_whole`).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub fn cut_from(src: &mut dyn esf::ReadAt, groups: &[Vec<u32>]) -> CutResult {
    let dir = match esf::ResourceDir::read(src) {
        Ok(d) => Some(d),
        Err(esf::Error::Io(e)) => return Err(e.to_string()),
        Err(_) => None,
    };
    if let Some(dir) = dir {
        let mut c = Cutter::new(&dir, groups);
        let mut reads: Vec<(usize, u64, usize)> = c.plan().collect();
        reads.sort_by_key(|r| r.1);
        let mut sound = true;
        for (k, off, len) in reads {
            let mut rec = vec![0u8; len];
            src.read_at(off, &mut rec).map_err(|e| e.to_string())?;
            if c.put(k, rec).is_err() {
                sound = false;
                break;
            }
        }
        if sound {
            return Ok(c.finish());
        }
    }
    let whole = esf::read_all(src).map_err(|e| e.to_string())?;
    cut_whole(whole, groups)
}

/// The bodies `groups` name cut out of a whole resource file (`file`, its
/// bytes): the file parsed, and each group's bodies' records copied out of
/// it, as the dictionary names them. The way of a file with no directory, and
/// what the directory's way is held to.
pub fn cut_whole(file: Vec<u8>, groups: &[Vec<u32>]) -> CutResult {
    let bytes = file.len() as u64;
    let c = esf::Container::open(file).map_err(|e| e.to_string())?;
    let cut = |ids: &Vec<u32>| {
        let mut out = Vec::new();
        for &id in ids {
            let Some(&n) = c.dict.get(&(id as i32)) else { continue };
            let node = c.node(n);
            if node.ty == BODY && node.body >= 12 {
                out.extend_from_slice(&c.data[node.body - 12..node.body + node.size]);
            }
        }
        (!out.is_empty()).then(|| esf::Container::parse_records(out))
    };
    Ok(Cut { groups: groups.iter().map(cut).collect(), found: Found::Whole, reads: 1, bytes })
}

/// A cut under way by a file's directory: the reads to make, and each
/// group's container as its records come. The same on every platform: what
/// reads the disc (a thread's file natively, a page's task) asks `plan` what
/// to read and `put`s each read's bytes.
pub struct Cutter {
    /// each read: the group its record goes in, the body's id, the record's
    /// offset in the file and its length
    plan: Vec<(usize, u32, u64, usize)>,
    /// how many of the plan's records are in their containers (they go in
    /// the plan's order)
    placed: usize,
    /// records read ahead of their turn
    ahead: std::collections::HashMap<usize, Vec<u8>>,
    out: Vec<esf::Container>,
    reads: u32,
    bytes: u64,
}

impl Cutter {
    /// The cut of `groups` out of the file `dir` is the directory of: a read
    /// for each body the directory names, in the groups' order (one it does
    /// not name is not in the file).
    pub fn new(dir: &esf::ResourceDir, groups: &[Vec<u32>]) -> Cutter {
        let mut plan = Vec::new();
        let mut room = vec![0usize; groups.len()];
        for (g, ids) in groups.iter().enumerate() {
            for &id in ids {
                if let Some((off, len)) = dir.find(id) {
                    plan.push((g, id, off, len));
                    room[g] += len;
                }
            }
        }
        Cutter { plan, placed: 0, ahead: Default::default(), out: room.into_iter().map(esf::Container::with_capacity).collect(), reads: 0, bytes: 0 }
    }

    /// The reads: each one's number, its offset in the file and its length.
    pub fn plan(&self) -> impl Iterator<Item = (usize, u64, usize)> + '_ {
        self.plan.iter().enumerate().map(|(k, &(_, _, off, len))| (k, off, len))
    }

    /// The bytes of read `k`. A body's record (a character sprite of that
    /// id, `esf::resdir::is_sprite`) goes into its group's container, in the
    /// plan's order (one read ahead of its turn waits for it); a whole record
    /// of another kind is left out, as the whole file's cut leaves it. Bytes
    /// that are not the record the directory named are an error: the
    /// directory is not to be gone by.
    pub fn put(&mut self, k: usize, rec: Vec<u8>) -> Result<(), String> {
        let &(_, id, off, len) = self.plan.get(k).ok_or("no such read")?;
        let whole = rec.len() == len && len >= 12 && i32::from_le_bytes([rec[4], rec[5], rec[6], rec[7]]) as i64 == len as i64 - 12;
        let body = whole && u16::from_le_bytes([rec[0], rec[1]]) == BODY;
        if !whole || (body && !esf::resdir::is_sprite(&rec, BODY, id)) {
            return Err(format!("the directory's {id:08X} at {off} is not that record"));
        }
        self.reads += 1;
        self.bytes += len as u64;
        self.ahead.insert(k, if body { rec } else { Vec::new() });
        while let Some(rec) = self.ahead.remove(&self.placed) {
            if !rec.is_empty() {
                self.out[self.plan[self.placed].0].append_records(&rec);
            }
            self.placed += 1;
        }
        Ok(())
    }

    /// The cut, every read `put`.
    pub fn finish(self) -> Cut {
        debug_assert_eq!(self.placed, self.plan.len(), "a read was not put");
        Cut { groups: self.out.into_iter().map(|c| (!c.data.is_empty()).then_some(c)).collect(), found: Found::Directory, reads: self.reads, bytes: self.bytes }
    }
}
