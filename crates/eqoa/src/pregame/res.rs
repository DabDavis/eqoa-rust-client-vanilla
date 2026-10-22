//! The pregame's resources: the files each stage reads (the containers into
//! one library, in the order read, as each module's `VICSFFile::Parse` adds
//! them to its scene's dictionary; the text files kept as they are), and the
//! UI device over them, `Vui` (WORDS.TXT, TEXTURES.TXT and SUPPORT's atlas),
//! once all three are here.
//!
//! The 2D layer's textures are the window's (`viewer.rs`): the containers
//! read since it last looked are in `fresh`, for their surfaces to go up.

use super::files;
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
    pub words: Option<StringTable>,
    pub textures: Option<StringTable>,
    vui: Option<Vui>,
    /// library files read since the window last took them (`take_fresh`)
    fresh: Vec<usize>,
    /// why a file could not be read (the last)
    pub error: Option<String>,
}

/// A file's name in the library: its disc path's last part (`SETUP.CSF`).
pub fn name_of(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn is_container(path: &str) -> bool {
    let p = path.to_ascii_uppercase();
    p.ends_with(".CSF") || p.ends_with(".ESF")
}

impl Res {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks for the files not asked for yet.
    pub fn want(&mut self, paths: &[&str]) {
        let new: Vec<&str> = paths.iter().copied().filter(|p| self.asked.insert(p.to_string())).collect();
        if !new.is_empty() {
            self.pending.push(files::read(&new));
        }
    }

    /// Whether every one of `paths` has been read.
    pub fn ready(&self, paths: &[&str]) -> bool {
        paths.iter().all(|p| self.read.contains(*p))
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
        self.make_vui();
    }

    /// A file read: a container into the library, anything else kept.
    pub fn put(&mut self, path: &str, bytes: Vec<u8>) {
        if is_container(path) {
            match esf::Container::open(bytes) {
                Ok(c) => {
                    let i = self.lib.add(name_of(path).to_ascii_uppercase(), c);
                    self.fresh.push(i);
                }
                Err(e) => {
                    crate::say(&format!("pregame: {path}: {e}"));
                    self.error = Some(format!("{path}: {e}"));
                }
            }
        } else {
            if path.eq_ignore_ascii_case("DATA/WORDS.TXT") {
                self.words = StringTable::parse(&bytes);
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

    /// A container, by its name (`SETUP.CSF`).
    pub fn container(&self, name: &str) -> Option<&esf::Container> {
        self.lib.files.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, c)| c)
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

    pub fn vui_ready(&self) -> bool {
        self.vui.is_some()
    }

    pub fn vui(&self) -> Option<&Vui> {
        self.vui.as_ref()
    }

    pub fn vui_mut(&mut self) -> Option<&mut Vui> {
        self.vui.as_mut()
    }

    /// A loading picture's RGBA, once its file is read.
    pub fn picture(&self, n: i32) -> Option<Vec<u8>> {
        self.raw(&super::boot::path(n)).and_then(super::boot::decode)
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
}
