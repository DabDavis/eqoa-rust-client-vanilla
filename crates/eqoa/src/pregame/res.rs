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
        }
        self.art_due = true;
        self.relayer = true;
    }

    /// The art to make what is drawn again in, once: when it changed and its
    /// 2002 files (if any are being read) are all in.
    pub fn take_art_change(&mut self) -> Option<Era> {
        (self.art_due && self.pending_2002.is_empty()).then(|| {
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

    /// The files read since the last frame, put where they go.
    pub fn poll(&mut self) {
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
                self.disc_words = StringTable::parse(&bytes);
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
        self.server_words = bytes.and_then(|b| StringTable::parse(&b));
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
}
