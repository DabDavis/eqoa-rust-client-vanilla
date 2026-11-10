//! The pregame's resources: the files each stage reads (the containers into
//! one library, in the order read, as each module's `VICSFFile::Parse` adds
//! them to its scene's dictionary; the text files kept as they are), and the
//! UI device over them, `Vui` (WORDS.TXT, TEXTURES.TXT and SUPPORT's atlas),
//! once all three are here.
//!
//! The 2D layer's textures are the window's (`viewer.rs`): the containers
//! read since it last looked are in `fresh`, for their surfaces to go up.

use super::files;
use esf::Era;
use std::collections::{HashMap, HashSet};
use vi::strtab::StringTable;
use vi::ui::Vui;

/// What every module reads first: the game's words and texture names, the
/// UI's art and its font (`VISetup::Init`: ArialUI.csf, then ui.csf).
pub const BASE: &[&str] = &["DATA/WORDS.TXT", "DATA/TEXTURES.TXT", "DATA2/ARIALUI.CSF", "DATA2/UI.CSF"];

#[derive(Default)]
pub struct Res {
    /// the containers read, by name (the disc path's last part), in order
    pub lib: esf::Library,
    /// the other files, by their disc path
    raw: HashMap<String, Vec<u8>>,
    /// every path asked for, read or not
    asked: HashSet<String>,
    /// every path read
    read: HashSet<String>,
    pending: Vec<files::Pending>,
    /// the game's words: the disc's, or the server's (`set_server_words`)
    pub words: Option<StringTable>,
    /// the disc's WORDS.TXT, and the server's when its patch put one in its
    /// folder (`servers::words`)
    disc_words: Option<StringTable>,
    server_words: Option<StringTable>,
    pub textures: Option<StringTable>,
    vui: Option<Vui>,
    /// library files read since the window last took them (`take_fresh`)
    fresh: Vec<usize>,
    /// why a file could not be read (the last)
    pub error: Option<String>,
    /// The art drawn (`era`, F9): Frontiers', or the 2002 disc's, whose
    /// copies of the art files are read beside the disc's (`set_art`).
    art: Era,
    /// The 2002 disc's copies: each path asked of it, each answered (read,
    /// not on that disc, or its read failed), the reads under way, its
    /// containers waiting for their namesake, its other files (the loading
    /// pictures) by the path they stand for.
    asked_2002: HashSet<String>,
    read_2002: HashSet<String>,
    pending_2002: Vec<(Vec<String>, crate::era::Reading)>,
    waiting_2002: Vec<(String, esf::Container)>,
    raw_2002: HashMap<String, Vec<u8>>,
    /// The 2D layer is to be made again, every surface put up in the view's
    /// order (`take_relayer`): the art changed, or a 2002 file stood in for
    /// one whose surfaces are up already.
    relayer: bool,
    /// The art changed: what is drawn is made again in it once its files are
    /// in (`take_art_change`).
    art_due: bool,
    /// A server's races' bodies (`want_bodies`): each asked for once; some
    /// being cut out of CHAR.ESF; some just put in the library
    /// (`take_bodies_in`).
    bodies_asked: HashSet<u32>,
    bodies_reading: Option<(Vec<u32>, files::PendingCut)>,
    /// when the cut under way was asked for (`bodytrace`)
    bodies_since: Option<web_time::Instant>,
    bodies_in: bool,
    /// Each bodies' file in the library and the bodies cut into it; in the
    /// 2002 art the 2002 disc's copies of them (`want_bodies_2002`): the files
    /// asked of it, and those being cut out of its CHAR.ESF.
    bodies_files: Vec<(usize, Vec<u32>)>,
    bodies_asked_2002: HashSet<usize>,
    bodies_reading_2002: Option<(Vec<usize>, files::PendingCut)>,
}

/// The name a server's races' bodies go into the library under.
pub const BODIES: &str = "CHAR.ESF (bodies)";

/// The file the bodies are cut out of, on either disc.
pub const BODIES_FILE: &str = "DATA/CHAR.ESF";

/// A server's races' bodies out of CHAR.ESF read whole (`file`): each body's
/// CSprite record (0x2700, header and all) whole, together one container of
/// those records alone; every other record let go. A body's record holds its
/// skeleton, meshes, materials and surfaces: built from it alone it is the
/// whole file's (`tests::a_cut_body_is_the_whole_files`). None when none of
/// them is there. What the pregame did until 2026-11-10 (148 MB read and
/// parsed for a few bodies), and what its cut by the file's directory
/// (`files::cut`) is held to, byte for byte.
#[cfg(test)]
pub fn cut_bodies(file: Vec<u8>, ids: &[u32]) -> Option<esf::Container> {
    files::cut_whole(file, &[ids.to_vec()]).ok()?.groups.pop()?
}

/// The 2002 disc's copy of a bodies' file (`c`: the same bodies cut out of
/// that disc's CHAR.ESF) stood in for it (`f`, its index in `lib`), as the
/// world's CHAR.ESF has its copy stand in for it (`era::install`), so the
/// 2002 art draws a server's race's body in character select and creation as
/// the world draws it (the 2002 disc's skeleton, goblin, lizardman, brownie,
/// gnoll, orc and minotaur are its own; until 2026-11-08 the pregame drew
/// Frontiers', whose helm nodes differ: a skeleton's helm showed in
/// character select and not in the world). A copy that is its file's bytes
/// stands in for nothing. Whether it stands in.
fn stand_in_cut(lib: &mut esf::Library, f: usize, c: esf::Container) -> bool {
    if c.data == lib.file(f).data {
        return false;
    }
    lib.keep_frontiers(&crate::era::kept());
    lib.stand_in(f, c);
    true
}

/// `stand_in_cut` for each of `files` (each one's index in `lib` and the
/// bodies cut into it) out of the 2002 disc's CHAR.ESF read whole (`bytes`):
/// how many stand in. A file the 2002 disc has none of the bodies of is left
/// out. What the pregame did until 2026-11-10, and what the cut by that
/// disc's directory is held to.
#[cfg(test)]
pub fn stand_in_bodies(lib: &mut esf::Library, files: &[(usize, Vec<u32>)], bytes: Vec<u8>) -> usize {
    let groups: Vec<Vec<u32>> = files.iter().map(|(_, ids)| ids.clone()).collect();
    let Ok(cut) = files::cut_whole(bytes, &groups) else { return 0 };
    files.iter().zip(cut.groups).filter_map(|(&(f, _), c)| c.map(|c| stand_in_cut(lib, f, c))).filter(|&stood| stood).count()
}

/// A file's name in the library: its disc path's last part (`SETUP.CSF`).
pub fn name_of(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn is_container(path: &str) -> bool {
    let p = path.to_ascii_uppercase();
    p.ends_with(".CSF") || p.ends_with(".ESF")
}

/// Whether a file is art the 2002 disc has its own copy of (`era`): a
/// container, or a loading picture. The words, the texture names, the
/// licence and STATION's pages stay the disc's the game runs off.
pub fn is_art(path: &str) -> bool {
    is_container(path) || path.to_ascii_uppercase().ends_with(".16")
}

impl Res {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks for the files not asked for yet (in the 2002 art, the 2002
    /// disc's copies of the art among them too).
    pub fn want(&mut self, paths: &[&str]) {
        let new: Vec<&str> = paths.iter().copied().filter(|p| self.asked.insert(p.to_string())).collect();
        if !new.is_empty() {
            self.pending.push(files::read(&new));
            if self.art == Era::Y2002 {
                self.want_2002(&new);
            }
        }
    }

    /// Whether every one of `paths` has been read: in the 2002 art, with the
    /// 2002 disc's copies of the art among them answered.
    pub fn ready(&self, paths: &[&str]) -> bool {
        paths.iter().all(|p| self.read.contains(*p)) && (!self.reads_2002() || paths.iter().filter(|p| is_art(p)).all(|p| self.read_2002.contains(*p)))
    }

    /// Whether the 2002 disc's copies are read: in its art, with a 2002 disc
    /// at hand (none: its art is Frontiers', there is nothing to stand in).
    fn reads_2002(&self) -> bool {
        self.art == Era::Y2002 && crate::era::available()
    }

    /// The 2002 disc's copies of the art among `paths`, those not asked of
    /// it yet.
    fn want_2002(&mut self, paths: &[&str]) {
        if !self.reads_2002() {
            return;
        }
        let new: Vec<String> = paths.iter().copied().filter(|p| is_art(p) && self.asked_2002.insert(p.to_string())).map(str::to_string).collect();
        if !new.is_empty() {
            self.pending_2002.push((new.clone(), crate::era::read(new)));
        }
    }

    /// The art drawn now.
    pub fn art(&self) -> Era {
        self.art
    }

    /// Draws in `era` from now on: the library's view turned, and in the
    /// 2002 art the 2002 disc's copies asked of every art file asked for so
    /// far (and of every one asked later). What is drawn is made again once
    /// they are in (`take_art_change`), the 2D layer at once and again as
    /// they come (`take_relayer`).
    pub fn set_art(&mut self, era: Era) {
        if era == self.art {
            return;
        }
        self.art = era;
        self.lib.set_view(era);
        if era == Era::Y2002 {
            let all: Vec<String> = self.asked.iter().cloned().collect();
            self.want_2002(&all.iter().map(String::as_str).collect::<Vec<_>>());
            self.want_bodies_2002();
        }
        self.art_due = true;
        self.relayer = true;
    }

    /// The textures changed (a texture pack turned on or off, `texpack`):
    /// what is drawn made again in the art as it is, once its 2002 files (if
    /// any are being read) are all in, and the 2D layer with it, as for an
    /// art swapped (`take_art_change`, `take_relayer`).
    pub fn retexture(&mut self) {
        self.art_due = true;
        self.relayer = true;
    }

    /// The art to make what is drawn again in, once: when it changed and its
    /// 2002 files (if any are being read) are all in.
    pub fn take_art_change(&mut self) -> Option<Era> {
        (self.art_due && self.pending_2002.is_empty() && self.bodies_reading_2002.is_none()).then(|| {
            self.art_due = false;
            self.art
        })
    }

    /// Whether the 2D layer is to be made again, once.
    pub fn take_relayer(&mut self) -> bool {
        std::mem::take(&mut self.relayer)
    }

    /// A file off the 2002 disc, by the path it stands for: a container
    /// waits for its namesake (`stand_in_waiting`), anything else is kept.
    fn put_2002(&mut self, path: &str, bytes: Vec<u8>) {
        if is_container(path) {
            match esf::Container::open(bytes) {
                Ok(c) => {
                    self.waiting_2002.push((name_of(path).to_ascii_uppercase(), c));
                    self.stand_in_waiting();
                }
                Err(e) => crate::say(&format!("pregame: the 2002 disc's {path}: {e}")),
            }
        } else {
            self.raw_2002.insert(path.to_string(), bytes);
        }
    }

    /// The 2002 containers whose namesakes are in, stood in for them as the
    /// world's are (`era::install`: one the same bytes as its namesake is let
    /// go, the art `era::kept` names stays Frontiers').
    fn stand_in_waiting(&mut self) {
        let waiting = std::mem::take(&mut self.waiting_2002);
        for (name, c) in waiting {
            if (0..self.lib.files.len()).any(|f| self.lib.stands_for(f).is_none() && self.lib.files[f].0 == name) {
                if crate::era::install(&mut self.lib, vec![(name, c)]) > 0 {
                    self.relayer = true;
                }
            } else {
                self.waiting_2002.push((name, c));
            }
        }
    }

    /// Asks for the bodies among `ids` the library lacks (a server's races',
    /// `create::tables::bodies`): cut out of CHAR.ESF by its own directory
    /// (`files::cut`: each body's record read alone, off the frame's thread),
    /// one cut at a time, and kept as `BODIES`, a file a cut. Each is asked
    /// for once.
    pub fn want_bodies(&mut self, ids: &[u32]) {
        if self.bodies_reading.is_some() {
            return;
        }
        let new: Vec<u32> = ids.iter().copied().filter(|&id| id != 0 && self.lib.lookup(id as i32).is_none() && self.bodies_asked.insert(id)).collect();
        if !new.is_empty() {
            super::bodytrace::note(|| format!("{} bodies asked for", new.len()));
            self.bodies_since = Some(web_time::Instant::now());
            let ask = files::CutAsk { era: Era::Frontiers, path: BODIES_FILE.into(), groups: vec![new.clone()] };
            self.bodies_reading = Some((new, files::cut(ask)));
        }
    }

    /// Whether bodies came in (or the 2002 disc's copies of them stood in)
    /// since the last call: the figures made of what they changed are made
    /// again (`figure::Figure::refresh`).
    pub fn take_bodies_in(&mut self) -> bool {
        std::mem::take(&mut self.bodies_in)
    }

    /// The bodies' cut, once it is done: put in the library.
    fn poll_bodies(&mut self) {
        let Some((_, p)) = self.bodies_reading.as_mut() else { return };
        let Some(r) = p.poll() else { return };
        let (ids, _) = self.bodies_reading.take().expect("reading");
        let since = self.bodies_since.map_or(0.0, |t| t.elapsed().as_secs_f64() * 1e3);
        match r {
            Ok(mut cut) => {
                super::bodytrace::note(|| format!("{} bodies cut out of CHAR.ESF ({:?}: {} reads, {:.1} MB), {since:.1} ms after they were asked for", ids.len(), cut.found, cut.reads, cut.bytes as f64 / 1048576.0));
                match cut.groups.pop().flatten() {
                    Some(c) => {
                        let took = super::bodytrace::watch();
                        let mb = c.data.len() as f64 / 1048576.0;
                        let f = self.lib.add(BODIES, c);
                        super::bodytrace::note(|| format!("their file ({mb:.1} MB) put in the library in {:.1} ms on the frame's thread", took()));
                        self.bodies_files.push((f, ids));
                        self.bodies_in = true;
                    }
                    None => crate::say(&format!("pregame: CHAR.ESF has none of the bodies {ids:08X?}")),
                }
            }
            Err(e) => crate::say(&format!("pregame: the bodies: {e}")),
        }
        self.want_bodies_2002();
    }

    /// In the 2002 art (a 2002 disc at hand), the 2002 disc's copies of the
    /// bodies' files not asked of it yet: the same bodies cut out of its
    /// CHAR.ESF, one cut at a time, for `stand_in_cut`.
    fn want_bodies_2002(&mut self) {
        if !self.reads_2002() || self.bodies_reading_2002.is_some() {
            return;
        }
        let new: Vec<usize> = self.bodies_files.iter().map(|&(f, _)| f).filter(|&f| self.bodies_asked_2002.insert(f)).collect();
        if !new.is_empty() {
            let groups = self.bodies_files.iter().filter(|(f, _)| new.contains(f)).map(|(_, ids)| ids.clone()).collect();
            let ask = files::CutAsk { era: Era::Y2002, path: BODIES_FILE.into(), groups };
            self.bodies_reading_2002 = Some((new, files::cut(ask)));
        }
    }

    /// The 2002 disc's copies of the bodies' files asked of it, once they are
    /// cut: each stood in for its file (`stand_in_cut`; a disc without them,
    /// or a read that failed, leaves Frontiers'), the figures made of them
    /// made again.
    fn poll_bodies_2002(&mut self) {
        let Some((_, r)) = self.bodies_reading_2002.as_mut() else { return };
        let Some(read) = r.poll() else { return };
        let (asked, _) = self.bodies_reading_2002.take().expect("reading");
        match read {
            Ok(cut) => {
                let took = super::bodytrace::watch();
                let files: Vec<usize> = self.bodies_files.iter().map(|&(f, _)| f).filter(|f| asked.contains(f)).collect();
                let (found, reads, mb) = (cut.found, cut.reads, cut.bytes as f64 / 1048576.0);
                let mut n = 0;
                for (f, c) in files.iter().zip(cut.groups) {
                    if c.is_some_and(|c| stand_in_cut(&mut self.lib, *f, c)) {
                        n += 1;
                    }
                }
                super::bodytrace::note(|| format!("the 2002 disc's bodies ({found:?}: {reads} reads, {mb:.1} MB): {n} of {} files stood in for, in {:.1} ms on the frame's thread", files.len(), took()));
                if n > 0 {
                    self.bodies_in = true;
                }
            }
            Err(e) => crate::say(&format!("pregame: the 2002 disc's bodies: {e}")),
        }
        // files that came in meanwhile
        self.want_bodies_2002();
    }

    /// The files read since the last frame, put where they go.
    pub fn poll(&mut self) {
        self.poll_bodies();
        self.poll_bodies_2002();
        let mut arrived = Vec::new();
        self.pending.retain_mut(|p| match p.poll() {
            Some(r) => {
                arrived.push(r);
                false
            }
            None => true,
        });
        for r in arrived {
            match r {
                Ok(list) => {
                    for (path, bytes) in list {
                        self.put(&path, bytes);
                    }
                }
                Err(e) => {
                    crate::say(&format!("pregame: {e}"));
                    self.error = Some(e);
                }
            }
        }
        // the 2002 disc's copies, as they come: each path answered, read or
        // not (one its disc lacks, or a read that failed, is Frontiers')
        let mut done = Vec::new();
        self.pending_2002.retain_mut(|(names, r)| match r.poll() {
            Some(f) => {
                done.push((std::mem::take(names), f));
                false
            }
            None => true,
        });
        for (names, files) in done {
            match files {
                Ok(list) => {
                    for (path, bytes) in list {
                        self.put_2002(&path, bytes);
                    }
                }
                Err(e) => crate::say(&format!("pregame: the 2002 disc: {e}")),
            }
            self.read_2002.extend(names);
        }
        self.make_vui();
    }

    /// A file read: a container into the library, anything else kept.
    pub fn put(&mut self, path: &str, bytes: Vec<u8>) {
        if is_container(path) {
            match esf::Container::open(bytes) {
                Ok(c) => {
                    let i = self.lib.add(name_of(path).to_ascii_uppercase(), c);
                    self.fresh.push(i);
                    // its 2002 copy, if that came first
                    self.stand_in_waiting();
                }
                Err(e) => {
                    crate::say(&format!("pregame: {path}: {e}"));
                    self.error = Some(format!("{path}: {e}"));
                }
            }
        } else {
            if path.eq_ignore_ascii_case("DATA/WORDS.TXT") {
                self.disc_words = crate::spelling::words(&bytes);
                self.words = self.server_words.clone().or_else(|| self.disc_words.clone());
            }
            if path.eq_ignore_ascii_case("DATA/TEXTURES.TXT") {
                self.textures = StringTable::parse(&bytes);
            }
            self.raw.insert(path.to_string(), bytes);
        }
        self.read.insert(path.to_string());
    }

    /// A file that is not a container, by its disc path.
    pub fn raw(&self, path: &str) -> Option<&[u8]> {
        self.raw.get(path).map(Vec::as_slice)
    }

    /// A container, by its name (`SETUP.CSF`), as the art drawn has it (in
    /// the 2002 art, the 2002 disc's copy where it stands in).
    pub fn container(&self, name: &str) -> Option<&esf::Container> {
        self.lib.file_named(name).map(|f| self.lib.file(f))
    }

    /// The library files read since the last call: their surfaces go up.
    pub fn take_fresh(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.fresh)
    }

    /// SUPPORT's image is here (natively unpacked on its thread, in a page in
    /// its worker, `modules`).
    pub fn support_in(&self) -> bool {
        crate::modules::get(crate::modules::Module::Support).is_some()
    }

    /// `VIUI::Init`'s tables: once the words, the texture names and
    /// SUPPORT's atlas are all here.
    fn make_vui(&mut self) {
        if self.vui.is_some() {
            return;
        }
        let (Some(w), Some(t)) = (&self.words, &self.textures) else { return };
        let Some(img) = crate::modules::get(crate::modules::Module::Support) else { return };
        match vi::ui::read_atlas(&img) {
            Some(atlas) => self.vui = Some(Vui::new(w.clone(), t.clone(), atlas)),
            None => {
                if self.error.is_none() {
                    crate::say("pregame: SUPPORT's image has no atlas (not the Frontiers build)");
                    self.error = Some("SUPPORT's image is not the Frontiers build's".into());
                }
            }
        }
    }

    /// The server's WORDS.TXT for its session (its patch's, out of its
    /// folder), or with None the disc's again: what the windows and the
    /// world read from here on. A server's words never outlive its session.
    pub fn set_server_words(&mut self, bytes: Option<Vec<u8>>) {
        self.server_words = bytes.and_then(|b| crate::spelling::words(&b));
        self.words = self.server_words.clone().or_else(|| self.disc_words.clone());
        if let (Some(v), Some(w)) = (self.vui.as_mut(), self.words.as_ref()) {
            v.words = w.clone();
        }
    }

    pub fn vui_ready(&self) -> bool {
        self.vui.is_some()
    }

    pub fn vui(&self) -> Option<&Vui> {
        self.vui.as_ref()
    }

    pub fn vui_mut(&mut self) -> Option<&mut Vui> {
        self.vui.as_mut()
    }

    /// A loading picture's RGBA, once its file is read: in the 2002 art the
    /// 2002 disc's, once it is in (Frontiers' where that disc has none).
    pub fn picture(&self, n: i32) -> Option<Vec<u8>> {
        let path = super::boot::path(n);
        if self.reads_2002() {
            if !self.read_2002.contains(&path) {
                return None;
            }
            if let Some(b) = self.raw_2002.get(&path) {
                return super::boot::decode(b);
            }
        }
        self.raw(&path).and_then(super::boot::decode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_go_where_they_belong() {
        let mut r = Res::new();
        r.put("DATA/EULA.TXT", b"licence".to_vec());
        assert_eq!(r.raw("DATA/EULA.TXT"), Some(&b"licence"[..]));
        assert!(r.ready(&["DATA/EULA.TXT"]));
        assert!(!r.ready(&["DATA/EULA.TXT", "DATA2/SETUP.CSF"]));
        // a container that does not parse is said, not kept
        r.put("DATA2/BROKEN.CSF", b"not an objf".to_vec());
        assert!(r.container("BROKEN.CSF").is_none());
        assert!(r.error.is_some());
        assert_eq!(name_of("DATA2/SETUP.CSF"), "SETUP.CSF");
    }

    /// A container of surfaces (`(id, fill)`), as a CSF's bytes would open.
    fn objf(surfaces: &[(u32, u8)]) -> Vec<u8> {
        let mut d = b"FJBO".to_vec();
        d.resize(24, 0);
        d[16..24].copy_from_slice(&24i64.to_le_bytes());
        for &(id, fill) in surfaces {
            let mut body = id.to_le_bytes().to_vec();
            body.extend([fill; 16]);
            d.extend_from_slice(&0x1000u16.to_le_bytes());
            d.extend_from_slice(&0i16.to_le_bytes());
            d.extend_from_slice(&(body.len() as i32).to_le_bytes());
            d.extend_from_slice(&0u32.to_le_bytes());
            d.extend_from_slice(&body);
        }
        d
    }

    /// The reads asked of the 2002 disc (`era::set_reader`), each answered
    /// when the test says.
    type Asked = std::rc::Rc<std::cell::RefCell<Vec<(Vec<String>, crate::era::Slot)>>>;

    fn reader_2002() -> Asked {
        crate::era::set_page_disc(true);
        let asked: Asked = Default::default();
        let a = asked.clone();
        crate::era::set_reader(Box::new(move |names| {
            let slot: crate::era::Slot = Default::default();
            a.borrow_mut().push((names, slot.clone()));
            slot
        }));
        // the disc's own files come as the test puts them
        super::files::set_reader(Box::new(|_| Default::default()));
        asked
    }

    /// In the 2002 art each art file's 2002 copy is asked for (not the words,
    /// not STATION's pages), the screens wait for it, a container stands in
    /// for its namesake (one that came first waits for it), one of the same
    /// bytes does not, a loading picture is the 2002 disc's once it is in and
    /// Frontiers' where that disc has none; the art's change is due once the
    /// reads are in, the layer made again as stand-ins come. The control:
    /// back in Frontiers' art nothing waits and the pictures are Frontiers'.
    #[test]
    fn the_2002_copies_stand_in_and_the_pictures_wait() {
        let asked = reader_2002();
        let mut r = Res::new();
        let ui = "DATA2/UI.CSF";
        let pic = super::super::boot::path(1);
        let err3 = "DATA3/ERROR3.16";
        r.want(&[ui, "DATA/WORDS.TXT", &pic, err3]);
        assert!(asked.borrow().is_empty(), "Frontiers' art asks the 2002 disc for nothing");
        let frontiers_pic = vec![0x1fu8; super::super::boot::WIDTH * super::super::boot::HEIGHT * 2];
        r.put(ui, objf(&[(1, 1), (2, 2)]));
        r.put("DATA/WORDS.TXT", b"one\n".to_vec());
        r.put(&pic, frontiers_pic.clone());
        r.put(err3, frontiers_pic.clone());
        assert!(r.ready(&[ui, &pic]));
        r.take_relayer();
        // the 2002 art: the art files asked of the 2002 disc, the words not
        r.set_art(Era::Y2002);
        let names: Vec<String> = asked.borrow().iter().flat_map(|a| a.0.clone()).collect();
        let mut want: Vec<String> = [ui, pic.as_str(), err3].map(String::from).to_vec();
        want.sort();
        let mut got = names.clone();
        got.sort();
        assert_eq!(got, want);
        assert!(!r.ready(&[ui]) && r.ready(&["DATA/WORDS.TXT"]), "the screens wait for the 2002 copies");
        assert!(r.picture(1).is_none(), "no Frontiers picture while the 2002 one comes");
        assert!(r.take_art_change().is_none(), "the art changes once its files are in");
        // a file asked later is asked of both discs
        r.want(&["DATA2/SETUP.CSF"]);
        assert_eq!(asked.borrow().last().unwrap().0, vec!["DATA2/SETUP.CSF".to_string()]);
        // the answers: UI.CSF other art, the picture, no ERROR3 on that disc;
        // SETUP.CSF's copy before its namesake, the same bytes
        let y_pic = vec![0u8; super::super::boot::WIDTH * super::super::boot::HEIGHT * 2];
        for (names, slot) in asked.borrow().iter() {
            let files: Vec<(String, Vec<u8>)> = names
                .iter()
                .filter_map(|n| match n.as_str() {
                    "DATA2/UI.CSF" => Some((n.clone(), objf(&[(1, 9), (3, 3)]))),
                    "DATA2/SETUP.CSF" => Some((n.clone(), objf(&[(7, 7)]))),
                    n if n.ends_with("LOADING1.16") => Some((n.to_string(), y_pic.clone())),
                    _ => None,
                })
                .collect();
            *slot.borrow_mut() = Some(Ok(files));
        }
        r.poll();
        assert!(r.ready(&[ui, &pic, err3]));
        assert!(r.take_relayer(), "a stand-in came for a file already up");
        let f = r.lib.file_named("UI.CSF").unwrap();
        assert_eq!(r.lib.stands_for(f), Some(0), "UI.CSF is the 2002 disc's in its art");
        assert_eq!(r.lib.in_view(), vec![f, 0]);
        assert_eq!(r.picture(1).unwrap()[..4], [0, 0, 0, 255], "the 2002 disc's picture");
        assert_eq!(r.raw(err3).map(|b| b.len()), Some(frontiers_pic.len()));
        assert!(r.ready(&[err3]), "a file the 2002 disc lacks is answered: Frontiers'");
        assert_eq!(r.take_art_change(), Some(Era::Y2002));
        assert_eq!(r.take_art_change(), None, "once");
        // SETUP.CSF's copy came first and waits; its namesake the same bytes
        r.put("DATA2/SETUP.CSF", objf(&[(7, 7)]));
        assert_eq!(r.lib.stand_in_of(r.lib.files.iter().position(|(n, _)| n == "SETUP.CSF").unwrap()), None, "the same bytes stand in for nothing");
        // the control: Frontiers' art again
        r.set_art(Era::Frontiers);
        assert_eq!(r.lib.file_named("UI.CSF"), Some(0));
        assert_eq!(r.picture(1).unwrap()[..4], [248, 192, 56, 255], "Frontiers' picture");
        assert_eq!(r.take_art_change(), Some(Era::Frontiers));
        crate::era::set_page_disc(false);
    }

    /// A server's words stand for its session and give way to the disc's
    /// again; the control: the disc's before and after.
    #[test]
    fn a_servers_words_last_its_session() {
        let mut r = Res::new();
        r.put("DATA/WORDS.TXT", b"disc one\ndisc two\n".to_vec());
        assert_eq!(r.words.as_ref().unwrap().get(2, 0), "disc two");
        r.set_server_words(Some(b"server one\nserver two\n".to_vec()));
        assert_eq!(r.words.as_ref().unwrap().get(2, 0), "server two");
        r.set_server_words(None);
        assert_eq!(r.words.as_ref().unwrap().get(2, 0), "disc two");
        // set before the disc's arrive (a world left, the files read again)
        let mut r = Res::new();
        r.set_server_words(Some(b"server one\nserver two\n".to_vec()));
        r.put("DATA/WORDS.TXT", b"disc one\ndisc two\n".to_vec());
        assert_eq!(r.words.as_ref().unwrap().get(2, 0), "server two");
    }

    /// A server's races' bodies cut out of CHAR.ESF build as the whole file
    /// builds them, every texture there, and their sprites load (skeleton and
    /// animations: `actor_sprite`); a body not asked for is not in the cut, and
    /// one CHAR.ESF lacks leaves nothing. Needs the disc (`run/eqoa.iso`).
    #[test]
    fn a_cut_body_is_the_whole_files() {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut d) = iso::Iso::open(iso.to_str().unwrap()) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let bytes = d.read("CHAR.ESF").unwrap();
        // the froglok, the hill giant, a kerran woman, the kappa
        let ids = [0x141B_FE0Cu32, 0xFFB1_EA2C, 0xB749_BF57, 0x31A1_5F17];
        let mut cut = esf::Library::new();
        cut.add(BODIES, cut_bodies(bytes.clone(), &ids).expect("cut"));
        let mut whole = esf::Library::new();
        whole.add("CHAR.ESF", esf::Container::open(bytes.clone()).unwrap());
        for id in ids {
            let (a, b) = (cut.lookup(id as i32).expect("in the cut"), whole.lookup(id as i32).unwrap());
            let (ma, mb) = (cut.build_model(a).unwrap(), whole.build_model(b).unwrap());
            assert_eq!((ma.pos.len(), ma.batches.len(), ma.triangles), (mb.pos.len(), mb.batches.len(), mb.triangles), "{id:08X}");
            let textures: Vec<i32> = ma.batches.iter().flat_map(|b| b.layers.iter().map(|l| l.texture)).filter(|&t| t != 0).collect();
            assert!(!textures.is_empty() && textures.iter().all(|&t| cut.surface(t).is_some()), "{id:08X}: a texture outside the cut");
            let mut anims = vi::hsprite::AnimScene::default();
            let mut rand = vi::csprite::Rand(1);
            let sprite = crate::pregame::figure::actor_sprite(&cut, a, &mut anims, &mut rand).expect("its sprite");
            let whole_sprite = crate::pregame::figure::actor_sprite(&whole, b, &mut vi::hsprite::AnimScene::default(), &mut vi::csprite::Rand(1)).unwrap();
            assert_eq!(sprite.node_ids, whole_sprite.node_ids, "{id:08X}: its skeleton");
        }
        // the controls: one not asked for is not there, one CHAR.ESF lacks leaves none
        assert!(cut.lookup(0x2217_6258).is_none(), "the kerran man was not asked for");
        assert!(cut_bodies(bytes, &[0x1234_5678]).is_none());
    }

    /// In the 2002 disc's art a server's race's body is drawn in character
    /// select and creation as the world draws it: the bodies cut out of each
    /// disc's CHAR.ESF, the 2002 cut standing in for Frontiers'
    /// (`stand_in_bodies`), against both whole files as the world has them
    /// (the 2002 CHAR.ESF standing in for Frontiers', `era::install`). Per
    /// body (every one go-eqoa-server's races wear, and the three skeletons
    /// its Skeleton race's options offer, which `create::tables::bodies` cuts
    /// with them) and art: the same record off the same disc, the same model
    /// and skeleton, and a player's helm on the same node
    /// (`slotless::helm_node`). The human that dresses the skeletons is cut in
    /// beside them, where character select has it out of CHARSEL3.CSF (whose
    /// 2002 copy stands in as every pregame file's does): it is not one of the
    /// bodies, and its own helm node is 5 in Frontiers' art and 6 in the 2002
    /// disc's. The control: without the stand-in (the pregame until
    /// 2026-11-08) only the 2002 art differed, on the bodies the 2002 disc has
    /// its own of, and with them the helm nodes of the four skeletons
    /// (Frontiers' each name node 5, the 2002 disc's none) and the brownie's
    /// (seated on its node of the human's helm node's name, `slotless::
    /// HEAD_SEATS`: 5 on Frontiers' skeleton, 6 on the 2002 disc's); the
    /// goblin's and the orc's are the same in both (the 2002 disc's name a helm
    /// node, and a player's helm on Frontiers' hangs on the node of that name).
    /// A cut that is its
    /// file's bytes (the hill giant is the same on both discs) or has none of
    /// the bodies (no kerran on the 2002 disc) stands in for nothing. Needs
    /// both discs (`run/eqoa.iso`, and `$EQOA_2002_ISO` or the USA disc under
    /// `../decomp-builds/discs/eqoa-usa/`; without them it passes having
    /// checked nothing, and says so only under `--nocapture`).
    #[test]
    fn the_2002_art_draws_a_races_body_as_the_world_does() {
        let iso = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../run/eqoa.iso");
        let Ok(mut d) = iso::Iso::open(iso.to_str().unwrap()) else {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        };
        let y2002 = std::env::var_os("EQOA_2002_ISO").map(std::path::PathBuf::from).or_else(|| {
            let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../decomp-builds/discs/eqoa-usa");
            std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("iso")))
        });
        let Some(mut d2) = y2002.and_then(|p| iso::Iso::open(p).ok()) else {
            eprintln!("SKIP: no 2002 disc (set EQOA_2002_ISO)");
            return;
        };
        let (bf, by) = (d.read("CHAR.ESF").unwrap(), d2.read("CHAR.ESF").unwrap());
        let (goblin, orc, skeleton, hill_giant, kerran) = (0x6C13_08E0u32, 0x8214_A32Du32, 0x6AF9_8A81u32, 0xFFB1_EA2Cu32, 0x2217_6258u32);
        // the skeleton's options: old, mossy and frost bone
        let options = [0x45A8_B064u32, 0x198B_AA64, 0x648D_FC11];
        let mut bodies = vec![0x141B_FE0C, hill_giant, kerran, 0xB749_BF57, goblin, 0x3151_DF13, 0x8EFD_12EA, 0x9112_20E2, 0x31A1_5F17, 0x4376_EB52, 0x94BA_77CD];
        bodies.extend([0x7A85_646C, 0xCF41_5561, orc, 0xCFC4_78EE, 0xC5CB_F4CF, 0xBADC_25C9, 0xBE10_226F, 0x70EF_CE05, skeleton, 0x4A4D_88FB, 0xB4D2_D132]);
        bodies.extend(options);
        let human = vi::hash::hash_resource_id("HumanMale");
        let ids: Vec<u32> = bodies.iter().copied().chain([human]).collect();
        let mut world = esf::Library::new();
        let f = world.add("CHAR.ESF", esf::Container::open(bf.clone()).unwrap());
        world.stand_in(f, esf::Container::open(by.clone()).unwrap());
        let mut before = esf::Library::new();
        before.add(BODIES, cut_bodies(bf.clone(), &ids).unwrap());
        let mut pregame = esf::Library::new();
        let g = pregame.add(BODIES, cut_bodies(bf.clone(), &ids).unwrap());
        assert_eq!(stand_in_bodies(&mut pregame, &[(g, ids.clone())], by.clone()), 1);
        let drawn = |lib: &esf::Library, id: u32| {
            lib.with_view(lib.view_for(id as i32), || {
                let r = lib.lookup(id as i32).expect("there");
                let m = lib.build_model(r).unwrap();
                let cs = crate::pregame::figure::actor_sprite(lib, r, &mut vi::hsprite::AnimScene::default(), &mut vi::csprite::Rand(1)).unwrap();
                (lib.era_of(r.file), m.pos.len(), m.triangles, cs.h.nodes.len(), cs.node_ids, crate::slotless::helm_node(lib, id, &cs, true))
            })
        };
        let mut differed = Vec::new();
        for (era, own) in [(Era::Frontiers, 5), (Era::Y2002, 6)] {
            for lib in [&world, &before, &pregame] {
                lib.set_view(era);
            }
            for &id in &bodies {
                let (w, b) = (drawn(&world, id), drawn(&before, id));
                assert_eq!(drawn(&pregame, id), w, "{era:?}: {id:08X}");
                if b != w {
                    differed.push((era, id, b.5, w.5));
                }
            }
            // the human beside them, as the world has it
            assert_eq!(drawn(&pregame, human), drawn(&world, human), "{era:?}: the human");
            assert_eq!(drawn(&world, human).5, own, "{era:?}: the human's own helm node");
        }
        // the control: the pregame as it was
        assert!(differed.iter().all(|d| d.0 == Era::Y2002), "{differed:08X?}");
        assert_eq!(differed.len(), 10, "the seven race bodies the 2002 disc has its own of, and the three options: {differed:08X?}");
        // (a player's helm on the goblin's and the orc's Frontiers bodies hangs on
        // their node of the human's helm node's name, `slotless::HEAD_SEATS`, the
        // node the 2002 disc's own name for it: the same in both)
        let helms: Vec<(u32, i32, i32)> = differed.iter().filter(|d| d.2 != d.3).map(|d| (d.1, d.2, d.3)).collect();
        let brownie = 0x4376_EB52u32;
        let mut want = vec![(brownie, 5, 6), (skeleton, 5, 6)];
        want.extend(options.map(|o| (o, 5, 6)));
        assert_eq!(helms, want);
        assert!(differed.iter().any(|d| d.1 == goblin) && differed.iter().any(|d| d.1 == orc), "the goblin's and orc's bodies are the 2002 disc's own");
        // a cut of its file's bytes, and one of none of them
        for one in [hill_giant, kerran] {
            let mut lib = esf::Library::new();
            let h = lib.add(BODIES, cut_bodies(bf.clone(), &[one]).unwrap());
            assert_eq!(stand_in_bodies(&mut lib, &[(h, vec![one])], by.clone()), 0, "{one:08X}");
            assert!(!lib.has_2002());
        }
    }
}
