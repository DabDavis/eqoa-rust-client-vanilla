//! Texture packs and texture dumps, PCSX2's: one pack works in PCSX2 and here
//! (the user, 2026-11-08: "Vanilla loads PCSX2 replacement packs as they are
//! (same folder layout and file names, PNG and DDS, HD sizes) and can dump
//! its textures under PCSX2's names, so one pack works in both. An Extended
//! page row turns packs on and off.").
//!
//! Where (`place`): `<serial>/replacements` and `<serial>/dumps`, as PCSX2
//! keeps them in its own (`EmuFolders::Textures`), in a `textures` folder
//! beside the program, where a player finds it (the user, 2026-11-09: "lets
//! put the dumps/replacements in a folder next to the binary, so players
//! can find it easy"; `program_folder`): the program's own folder, a Mac
//! app's the folder holding the `.app` (not the bundle's inside), an
//! AppImage's the folder of `$APPIMAGE` (not its read-only mount). Where the
//! game cannot write there (Program Files, /usr/bin, a read-only disk) it is
//! `textures` in the config directory (`settings::config_dir`:
//! `$EQOA_CONFIG`, else `eqoa-rs` in the platform's), as it was before
//! 2026-11-09, and the Extended page says so; on Android, which has no
//! program folder of a player's, always. The config directory's from before
//! is moved beside the program the first time it is wanted (one rename: on
//! the same disk only), else read after the new one (`resolve`). The updater
//! never touches it (it renames the program's own file and its `.old`s). The
//! serial is the Frontiers disc's, `SLUS-20744`, and a texture off the 2002
//! disc (the 2002 art, `era`) the 2002 disc's, `SLUS-20470`. A pack's folder
//! is read at any depth, its names in any case, as PCSX2 reads it
//! (`ReloadReplacementMap`); a run that keeps no settings (the test
//! commands) has no folder, so nothing changes a test's picture.
//! Extended > Textures > Texture Folder shows the Frontiers disc's folder in
//! the system's file manager (`open_folder`).
//!
//! A file is named as PCSX2 names the texture (`esf::pcsx2`): every name it
//! can give it is looked for, the whole mip chain's first, then the base's
//! alone, then each farther chain's (`GsTexture::names`), and the first
//! found stands in for the texture. PNG (8 bits, RGB or RGBA; a picture
//! without alpha takes 0x80, the GS's 1.0, as PCSX2 gives it) and DDS (BC1,
//! BC2, BC3 and BC7, the uncompressed forms PCSX2 reads, its own mip levels)
//! of any size: a larger one is drawn with its own levels, made as each half
//! the one before where the file has none, and sampled that many levels
//! farther on (`render::level_shift`), so it adds detail near and keeps the
//! game's texel density in the distance. Its bytes are the GS's, as the
//! game's own are (an alpha of 0x80 is the GS's 1.0).
//!
//! Dumps: each texture made while Dump Textures is on is written to `dumps`
//! under every name PCSX2 gives it (each picture its first level, as PCSX2's
//! dump of that name is: what PCSX2 writes as it meets the texture near and
//! far, with Mipmapping on or off), byte for byte the picture PCSX2 dumps; a
//! file there already is left as it is. A thread writes them.
//!
//! Off (the default) nothing is set and every texture is the game's own,
//! byte for byte. A change to either row makes every texture again
//! (`generation`): the world's zones, the sky, the creatures, the effects
//! and the 2D layer (`App::retexture`).

use esf::pcsx2::Name;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// The 2002 disc's serial (EverQuest Online Adventures, SLUS-20470), whose
/// folder a texture off that disc is looked for and dumped in.
pub const SERIAL_2002: &str = "SLUS-20470";

/// The serial whose folder holds a texture off the disc of `era`.
pub fn serial(era: esf::Era) -> &'static str {
    match era {
        esf::Era::Frontiers => esf::pcsx2::SERIAL,
        esf::Era::Y2002 => SERIAL_2002,
    }
}

// ---- where the folders are ------------------------------------------------------

/// What this run's `textures` folder is beside (`Place::beside`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub enum Beside {
    /// The program, by its name as a player finds it (`eqoa.exe`, a Mac's
    /// `Eqoa.app`, an `.AppImage`).
    Program(String),
    /// The settings, in the config directory (`config`, its folder's name:
    /// `eqoa-rs`), the program's folder (`program`'s) being one the game
    /// cannot write to.
    Settings { program: String, config: String },
    /// The settings, on a platform with no program folder of a player's
    /// (Android).
    Only,
}

/// A name as the Extended page's notes say it, which have room for about 30
/// characters beside Back: as it is up to 12 (`eqoa.exe`, `Eqoa.app`), else
/// `long` (the release's bare download's, an AppImage's).
fn shown<'a>(name: &'a str, long: &'a str) -> &'a str {
    if name.chars().count() <= 12 { name } else { long }
}

impl Beside {
    /// The folder as the note on Texture Packs says it, after "12 in": the
    /// program's `textures` ("eqoa.exe's textures"), or the settings'
    /// ("eqoa-rs's textures"); on Android, as before 2026-11-09.
    pub fn packs(&self) -> String {
        match self {
            Beside::Program(n) => format!("{}'s textures", shown(n, "the game")),
            Beside::Settings { config, .. } if config.chars().count() <= 12 => format!("{config}'s textures"),
            Beside::Settings { .. } => "the settings' textures".to_string(),
            Beside::Only => format!("textures/{}", esf::pcsx2::SERIAL),
        }
    }

    /// What the note on Texture Folder says: where it is ("Beside
    /// eqoa.exe"), or why it is the settings' instead ("eqoa.exe's folder is
    /// read-only").
    pub fn folder(&self) -> String {
        match self {
            Beside::Program(n) => format!("Beside {}", shown(n, "the game")),
            Beside::Settings { program, .. } if program.chars().count() <= 12 => format!("{program}'s folder is read-only"),
            Beside::Settings { .. } => "The game's folder is read-only".to_string(),
            Beside::Only => "In the settings".to_string(),
        }
    }
}

/// This run's textures folders (`place`, `resolve`).
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub struct Place {
    /// The `textures` folder: each disc's packs read here first and its
    /// dumps written here.
    pub dir: PathBuf,
    /// Another read after it: the config directory's from before, where it
    /// could not be moved beside the program (another disk), or, the
    /// program's folder being one the game cannot write to, one there.
    pub also: Option<PathBuf>,
    pub beside: Beside,
    /// The config directory's from before, moved to `dir` as this was found.
    pub moved: Option<PathBuf>,
    /// What went otherwise (for the log): why that one was not moved, or
    /// why the program's folder cannot be written to.
    pub why: Option<String>,
}

/// The folder a player finds the program at `exe` in, and its name there:
/// the program's own folder; on a Mac (`mac`) an app's
/// (`<name>.app/Contents/MacOS/<exe>`) the folder holding the app, not the
/// bundle's inside; an AppImage's (`appimage`: `$APPIMAGE`, and `$APPDIR`,
/// the read-only mount it runs from) the folder of the image, when the
/// program runs from inside that mount (one started by an AppImage, and so
/// given its variables, is not one).
#[cfg(not(target_arch = "wasm32"))]
pub fn program_folder(exe: &Path, appimage: Option<(&Path, &Path)>, mac: bool) -> Option<(PathBuf, String)> {
    let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    if let Some((image, mount)) = appimage
        && image.is_absolute()
        && mount.is_absolute()
        && exe.starts_with(mount)
        && let (Some(dir), Some(n)) = (image.parent(), name(image))
    {
        return Some((dir.to_path_buf(), n));
    }
    if mac {
        let up = |p: &Path, want: &str| p.parent().filter(|d| d.file_name().is_some_and(|n| n == want)).map(Path::to_path_buf);
        if let Some(app) = up(exe, "MacOS").and_then(|m| up(&m, "Contents")).and_then(|c| c.parent().map(Path::to_path_buf))
            && app.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"))
            && let (Some(dir), Some(n)) = (app.parent(), name(&app))
        {
            return Some((dir.to_path_buf(), n));
        }
    }
    Some((exe.parent()?.to_path_buf(), name(exe)?))
}

/// The running program's folder and name (`program_folder`): its path
/// with links resolved (on Linux the system's already is; a Mac's may be a
/// link's), and an AppImage's variables.
#[cfg(all(not(test), not(target_arch = "wasm32")))]
fn program() -> Option<(PathBuf, String)> {
    let exe = std::env::current_exe().ok()?;
    // (not on Windows: its resolved paths are `\\?\` ones, which Explorer
    // does not take)
    #[cfg(unix)]
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let appimage = std::env::var_os("APPIMAGE").zip(std::env::var_os("APPDIR")).map(|(i, m)| {
        let m = PathBuf::from(m);
        (PathBuf::from(i), std::fs::canonicalize(&m).unwrap_or(m))
    });
    program_folder(&exe, appimage.as_ref().map(|(i, m)| (i.as_path(), m.as_path())), cfg!(target_os = "macos"))
}

/// Whether the game can write in `dir`, made when it is not there: a file
/// written in it and taken away, the one test every refusal answers
/// (permissions, a read-only disk or mount, Windows' protected folders); a
/// folder made for the test and then refused a file is taken away again.
#[cfg(not(target_arch = "wasm32"))]
fn writable(dir: &Path) -> std::io::Result<()> {
    let made = !dir.is_dir();
    std::fs::create_dir_all(dir)?;
    let probe = dir.join(".eqoa-write-test");
    let r = std::fs::write(&probe, b"");
    if r.is_ok() {
        // (a file a scanner holds a moment longer is still a written one)
        let _ = std::fs::remove_file(&probe);
    } else if made {
        let _ = std::fs::remove_dir(dir);
    }
    r
}

/// The folders for a program found in `program` (its folder and the name
/// shown for it, `program_folder`; none on Android) and settings kept in
/// `config`. The first time, `config`'s `textures` is moved beside the
/// program, where none is there yet: one rename, so only on the same disk
/// (another's is refused), and only a folder itself (a link to one is left,
/// as a relative one would point elsewhere moved); one not moved is read
/// after the new one, which takes the dumps. Where the program's folder
/// cannot be written to (`writable`), `config`'s is the folder, as before,
/// and one beside the program is read after it.
#[cfg(not(target_arch = "wasm32"))]
pub fn resolve(program: Option<(&Path, &str)>, config: &Path) -> Place {
    let old = config.join("textures");
    let Some((folder, name)) = program else {
        return Place { dir: old, also: None, beside: Beside::Only, moved: None, why: None };
    };
    let new = folder.join("textures");
    if new == old {
        return Place { dir: new, also: None, beside: Beside::Program(name.to_string()), moved: None, why: None };
    }
    let (mut moved, mut why) = (None, None);
    let a_folder = std::fs::symlink_metadata(&old).is_ok_and(|m| m.is_dir());
    if a_folder && std::fs::symlink_metadata(&new).is_err() {
        match std::fs::rename(&old, &new) {
            Ok(()) => moved = Some(old.clone()),
            Err(e) => why = Some(format!("{} was not moved there ({e})", old.display())),
        }
    }
    match writable(&new) {
        Ok(()) => Place { also: old.is_dir().then(|| old.clone()), dir: new, beside: Beside::Program(name.to_string()), moved, why },
        Err(e) => {
            let config = config.file_name().map_or_else(|| "the settings".to_string(), |n| n.to_string_lossy().into_owned());
            let why = Some(format!("{} cannot be written to ({e})", folder.display()));
            Place { also: new.is_dir().then(|| new.clone()), dir: old, beside: Beside::Settings { program: name.to_string(), config }, moved: None, why }
        }
    }
}

/// This run's folders (`resolve`): natively, while it keeps its settings
/// (none in a page, nor in a run that keeps none, the test commands'),
/// found the first time they are wanted (Texture Packs or Dump Textures
/// on, or the Texture Folder row chosen) and kept for the run.
#[cfg(all(not(test), not(target_arch = "wasm32")))]
pub fn place() -> Option<Place> {
    static FOUND: Mutex<Option<(PathBuf, Place)>> = Mutex::new(None);
    let config = crate::settings::config_dir()?;
    let mut found = FOUND.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((c, p)) = found.as_ref()
        && *c == config
    {
        return Some(p.clone());
    }
    let program = if cfg!(target_os = "android") { None } else { program() };
    let p = resolve(program.as_ref().map(|(d, n)| (d.as_path(), n.as_str())), &config);
    if let Some(from) = &p.moved {
        crate::say(&format!("textures: {} moved to {}, beside the program", from.display(), p.dir.display()));
    }
    match (&p.beside, &p.why) {
        (Beside::Settings { .. }, Some(why)) => crate::say(&format!("textures: {why}: the textures folder is {} instead", p.dir.display())),
        (_, Some(why)) => crate::say(&format!("textures: {why}: it is read after {}", p.dir.display())),
        _ => {}
    }
    *found = Some((config, p.clone()));
    Some(p)
}

#[cfg(all(not(test), target_arch = "wasm32"))]
pub fn place() -> Option<Place> {
    None
}

/// A test's folders (`testing::set_place`): none unless it gives some.
#[cfg(test)]
pub fn place() -> Option<Place> {
    testing::PLACE.with(|p| p.borrow().clone())
}

/// `textures/<serial>`, where the serial's packs and dumps are (`place`);
/// none in a page or a run that keeps no settings.
pub fn root(era: esf::Era) -> Option<PathBuf> {
    place().map(|p| p.dir.join(serial(era)))
}

/// Extended > Textures > Texture Folder: the folder the Frontiers disc's
/// packs go in and its dumps are written to (`textures/SLUS-20744`, its
/// `replacements` made in it as PCSX2 makes it) shown in the system's file
/// manager (Explorer, the Finder, the desktop's through xdg-open). False
/// with no folder (a run that keeps no settings) or nothing to show it.
pub fn open_folder() -> bool {
    let Some(dir) = root(esf::Era::Frontiers) else { return false };
    if child(&dir, "replacements").is_none() {
        let _ = std::fs::create_dir_all(dir.join("replacements"));
    }
    show(&dir)
}

/// `dir` in the system's file manager.
#[cfg(all(not(test), not(target_arch = "wasm32"), not(target_os = "android")))]
fn show(dir: &Path) -> bool {
    use std::process::{Command, Stdio};
    let program: PathBuf = if cfg!(windows) {
        std::env::var_os("SystemRoot").map_or_else(|| "explorer.exe".into(), |r| PathBuf::from(r).join("explorer.exe"))
    } else if cfg!(target_os = "macos") {
        "open".into()
    } else {
        "xdg-open".into()
    };
    // none of this program's streams: its stderr is the log's pipe
    // (`logfile`), which a file manager it starts would outlive
    match Command::new(&program).arg(dir).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() {
        Ok(mut c) => {
            crate::say(&format!("textures: {} shown", dir.display()));
            // waited for, so that it does not linger as a zombie
            let _ = std::thread::Builder::new().name("texture folder".into()).spawn(move || {
                let _ = c.wait();
            });
            true
        }
        Err(e) => {
            crate::say(&format!("textures: {} did not show {} ({e})", program.display(), dir.display()));
            false
        }
    }
}

/// Nothing shows a folder in a page or an Android app.
#[cfg(all(not(test), any(target_arch = "wasm32", target_os = "android")))]
fn show(_dir: &Path) -> bool {
    false
}

/// A test's: the folder noted (`testing::shown`), shown if it is there.
#[cfg(test)]
fn show(dir: &Path) -> bool {
    testing::SHOWN.with(|s| s.borrow_mut().push(dir.to_path_buf()));
    dir.is_dir()
}

#[cfg(test)]
pub mod testing {
    use super::Place;
    use std::cell::RefCell;
    use std::path::PathBuf;

    thread_local! {
        pub static PLACE: RefCell<Option<Place>> = const { RefCell::new(None) };
        pub static SHOWN: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    }

    /// This thread's folders (`place`), and none shown yet.
    pub fn set_place(p: Option<Place>) {
        PLACE.with(|x| *x.borrow_mut() = p);
        SHOWN.with(|s| s.borrow_mut().clear());
    }

    /// The folders `open_folder` showed on this thread.
    pub fn shown() -> Vec<PathBuf> {
        SHOWN.with(|s| s.borrow().clone())
    }
}

/// `dir`'s child named `name` in any case (PCSX2 warns of a folder named in
/// the wrong case, which a case-sensitive disk does not find): the exact one
/// first.
fn child(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_dir() {
        return Some(exact);
    }
    std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.is_dir() && p.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(name)))
}

/// A pack's files by the name each gives (`Name::parse`), and the CLUT
/// hashes among them, which a texture's are looked up by first.
#[derive(Default)]
pub struct Index {
    pub files: HashMap<Name, PathBuf>,
    cluts: HashSet<u64>,
}

impl Index {
    /// Every PNG and DDS under `dir`, at any depth, whose name PCSX2 reads;
    /// of two with one name the first in path order.
    pub fn scan(dir: &Path) -> Index {
        let mut paths = Vec::new();
        // a folder's depth kept, so a link back up does not go round for ever
        let mut dirs = vec![(dir.to_path_buf(), 0)];
        while let Some((d, depth)) = dirs.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if depth < 16 {
                        dirs.push((p, depth + 1));
                    }
                } else if loader(&p).is_some() {
                    paths.push(p);
                }
            }
        }
        paths.sort();
        let mut ix = Index::default();
        for p in paths {
            let Some(n) = p.file_name().and_then(|f| Name::parse(&f.to_string_lossy())) else { continue };
            ix.cluts.insert(n.clut_hash);
            ix.files.entry(n).or_insert(p);
        }
        ix
    }

    /// Every PNG and DDS under each of `dirs` (`scan`), the first folder's
    /// over a later one's of the same name.
    pub fn scan_all(dirs: impl IntoIterator<Item = PathBuf>) -> Index {
        let mut ix = Index::default();
        for d in dirs {
            for (n, p) in Index::scan(&d).files {
                ix.cluts.insert(n.clut_hash);
                ix.files.entry(n).or_insert(p);
            }
        }
        ix
    }

    /// The file standing in for a surface, and the level of it the file's
    /// name stands for: of `names` (`GsTexture::names`, in its order) the
    /// first a file has.
    fn find<'a>(&'a self, names: &[(Name, usize)]) -> Option<(&'a Path, Name, usize)> {
        names.iter().find_map(|&(n, level)| self.files.get(&n).map(|p| (p.as_path(), n, level)))
    }
}

/// What a file is read as: by its extension, `png` or `dds` in any case.
fn loader(p: &Path) -> Option<&'static str> {
    let e = p.extension()?.to_string_lossy().to_ascii_lowercase();
    match e.as_str() {
        "png" => Some("png"),
        "dds" => Some("dds"),
        _ => None,
    }
}

/// A stand-in found for a surface: its levels, the base first.
struct Found {
    levels: Vec<esf::Image>,
}

/// What a surface is known by in `Pack::found`: its DictID, its disc, where
/// its palette is in its container's bytes and how long they are (one
/// DictID is one record in a view, but a file's own surfaces are also read
/// straight off it: `UiAssets`).
type Key = (i32, bool, usize, usize);

fn key(d: &[u8], s: &esf::Surface, era: esf::Era) -> Key {
    (s.dict_id, era == esf::Era::Y2002, s.pal_off, d.len())
}

/// The options' Texture Packs and Dump Textures, as set (`sync`): the packs'
/// files by name for each disc, the stand-ins found so far, and the dumper.
pub struct Pack {
    generation: u64,
    packs: Option<[Index; 2]>,
    found: Mutex<HashMap<Key, Option<Arc<Found>>>>,
    dumper: Option<Dumper>,
}

/// Each `Pack` made, from 1 (its key bits: `render::LibraryTextures::key`).
static MADE: AtomicU64 = AtomicU64::new(0);

impl Pack {
    /// The packs' folders read (`load`) and the dumps' made ready (`dump`)
    /// in this run's (`place`); `None` when it has none.
    pub fn new(load: bool, dump: bool) -> Option<Pack> {
        if !(load || dump) {
            return None;
        }
        Some(Pack::at(&place()?, load, dump))
    }

    /// The packs read and the dumps made ready in `place`'s folders: each
    /// disc's `replacements` in `dir`, then in `also` (the first's file
    /// standing for a name both have), its `dumps` in `dir` alone.
    fn at(place: &Place, load: bool, dump: bool) -> Pack {
        let eras = [esf::Era::Frontiers, esf::Era::Y2002];
        let packs = load.then(|| {
            eras.map(|era| {
                let root = place.dir.join(serial(era));
                // made for the player to find, as PCSX2 makes it on a dump
                if era == esf::Era::Frontiers && child(&root, "replacements").is_none() {
                    let _ = std::fs::create_dir_all(root.join("replacements"));
                }
                let roots = std::iter::once(root).chain(place.also.iter().map(|a| a.join(serial(era))));
                Index::scan_all(roots.filter_map(|r| child(&r, "replacements")))
            })
        });
        let dumper = dump
            .then(|| {
                Dumper::new(eras.map(|era| {
                    let r = place.dir.join(serial(era));
                    Some(child(&r, "dumps").unwrap_or_else(|| r.join("dumps")))
                }))
            })
            .flatten();
        Pack { generation: (MADE.fetch_add(1, Ordering::Relaxed) + 1) & 0xFF_FFFF, packs, found: Mutex::new(HashMap::new()), dumper }
    }

    /// How many replacements the packs hold (for the disc of `era`).
    pub fn count(&self, era: esf::Era) -> usize {
        self.packs.as_ref().map_or(0, |p| p[(era == esf::Era::Y2002) as usize].files.len())
    }

    /// How many files have been dumped.
    pub fn dumped(&self) -> usize {
        self.dumper.as_ref().map_or(0, |d| d.written.load(Ordering::Relaxed))
    }

    /// The stand-in for a surface, if a pack has one: looked up once (a
    /// texture's CLUT first, then its names), read and given its levels.
    fn find(&self, d: &[u8], s: &esf::Surface, era: esf::Era) -> Option<Arc<Found>> {
        let packs = self.packs.as_ref()?;
        let k = key(d, s, era);
        if let Some(f) = self.found.lock().unwrap_or_else(|e| e.into_inner()).get(&k) {
            return f.clone();
        }
        let ix = &packs[(era == esf::Era::Y2002) as usize];
        let f = s.gs_clut_hash(d).filter(|h| ix.cluts.contains(h)).and_then(|_| {
            let t = s.gs(d)?;
            let names = t.names();
            let (path, name, _) = ix.find(&names)?;
            match read(path) {
                Ok(levels) => {
                    let levels = with_levels(levels, s);
                    // `EQOA_TEXTURES_LOG`: each stand-in said as it is read
                    if std::env::var_os("EQOA_TEXTURES_LOG").is_some() {
                        crate::say(&format!("textures: {:08x} drawn as {} ({}x{})", s.dict_id as u32, name.file_name(0), levels[0].width, levels[0].height));
                    }
                    Some(Arc::new(Found { levels }))
                }
                Err(e) => {
                    crate::say(&format!("textures: {}: {e}", path.display()));
                    None
                }
            }
        });
        self.found.lock().unwrap_or_else(|e| e.into_inner()).insert(k, f.clone());
        f
    }

    /// The levels to draw for a surface (the pack's, or `None` for its own;
    /// its record's bytes are `d`), dumping it first while Dump Textures is on.
    pub fn levels_for(&self, d: &[u8], s: &esf::Surface, era: esf::Era) -> Option<Vec<esf::Image>> {
        if let Some(dumper) = &self.dumper {
            dumper.dump(d, s, era);
        }
        self.find(d, s, era).map(|f| f.levels.clone())
    }
}

impl render::Pack for Pack {
    fn levels(&self, c: &esf::Container, s: &esf::Surface, era: esf::Era) -> Option<Vec<esf::Image>> {
        self.levels_for(&c.data, s, era)
    }

    /// A stand-in's texture, and while dumping every texture (so that each
    /// is made again, and dumped, when the row turns it on), is keyed apart.
    fn replaces(&self, _key: u64, c: &esf::Container, s: &esf::Surface, era: esf::Era) -> bool {
        self.dumper.is_some() || self.find(&c.data, s, era).is_some()
    }

    fn generation(&self) -> u64 {
        self.generation
    }
}

/// A picture for the dumps' writer: its file, its size and its RGBA.
type Job = (PathBuf, u32, u32, Vec<u8>);

/// The dumps' folders (by disc) and the thread that writes them.
struct Dumper {
    dirs: [Option<PathBuf>; 2],
    tx: Mutex<std::sync::mpsc::Sender<Job>>,
    /// The names dumped (or found on the disk) this run.
    seen: Mutex<HashSet<Name>>,
    written: Arc<AtomicUsize>,
    /// Pictures handed to the writer and not yet written.
    pending: Arc<AtomicUsize>,
}

impl Dumper {
    /// The writer started, the Frontiers disc's folder made (and beside it
    /// the replacements', as PCSX2 makes both); the 2002 disc's is made at
    /// its first dump.
    fn new(dirs: [Option<PathBuf>; 2]) -> Option<Dumper> {
        let d = dirs[0].as_ref()?;
        if !d.is_dir() {
            let _ = std::fs::create_dir_all(d);
            if let Some(up) = d.parent()
                && child(up, "replacements").is_none()
            {
                let _ = std::fs::create_dir_all(up.join("replacements"));
            }
        }
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        let written = Arc::new(AtomicUsize::new(0));
        let pending = Arc::new(AtomicUsize::new(0));
        let (count, left) = (written.clone(), pending.clone());
        std::thread::Builder::new()
            .name("texture dumps".into())
            .spawn(move || {
                for (path, w, h, rgba) in rx {
                    if !path.exists() {
                        if let Some(dir) = path.parent().filter(|d| !d.is_dir()) {
                            let _ = std::fs::create_dir_all(dir);
                        }
                        match write_png(&path, w, h, &rgba) {
                            Ok(()) => {
                                count.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(e) => eprintln!("textures: {}: {e}", path.display()),
                        }
                    }
                    left.fetch_sub(1, Ordering::Relaxed);
                }
            })
            .ok()?;
        Some(Dumper { dirs, tx: Mutex::new(tx), seen: Mutex::new(HashSet::new()), written, pending })
    }

    /// Every name PCSX2 gives the surface's texture, each with its picture
    /// (its first level's), to the writer: the names not dumped yet.
    fn dump(&self, d: &[u8], s: &esf::Surface, era: esf::Era) {
        let Some(dir) = &self.dirs[(era == esf::Era::Y2002) as usize] else { return };
        let Some(t) = s.gs(d) else { return };
        for (name, level) in t.names() {
            if !self.seen.lock().unwrap_or_else(|e| e.into_inner()).insert(name) {
                continue;
            }
            let path = dir.join(name.file_name(0));
            if path.exists() {
                continue;
            }
            let l = &t.levels[level];
            self.pending.fetch_add(1, Ordering::Relaxed);
            if self.tx.lock().unwrap_or_else(|e| e.into_inner()).send((path, l.width, l.height, t.rgba(level))).is_err() {
                self.pending.fetch_sub(1, Ordering::Relaxed);
            }
        }
    }
}

/// An RGBA8 PNG, as PCSX2's `SavePNGImage` writes a dump (its default
/// compression, level 1, is the fast one).
pub fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    // written beside it under this program's own name, then moved over it:
    // a dump cut off is never left as the file, and two windows dumping the
    // same texture do not write into one another's
    let tmp = path.with_extension(format!("png.{}.part", std::process::id()));
    let go = || -> Result<(), Box<dyn std::error::Error>> {
        let mut enc = png::Encoder::new(std::io::BufWriter::new(std::fs::File::create(&tmp)?), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        enc.write_header()?.write_image_data(rgba)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    };
    go().map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

// ---- what the options set ----------------------------------------------------

/// The pack set (`sync`), and the rows it was made for.
struct State {
    applied: (bool, bool),
    pack: Option<Arc<Pack>>,
}

static STATE: Mutex<State> = Mutex::new(State { applied: (false, false), pack: None });

/// Moves on each time what stands in for the game's textures changes: what
/// is drawn is made again then (`App::retexture`).
static GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

/// The pack set, if any.
pub fn pack() -> Option<Arc<Pack>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner()).pack.clone()
}

/// The options' Texture Packs and Dump Textures carried out, when they
/// changed: the packs read and the dumps made ready, set for the renderer
/// (`render::set_pack`), and `generation` moved on. Called each frame.
pub fn sync() {
    let c = crate::settings::client();
    let want = (c.texture_packs, c.dump_textures);
    let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if st.applied == want {
        return;
    }
    st.applied = want;
    let pack = Pack::new(want.0, want.1).map(Arc::new);
    if let (Some(p), Some(place)) = (&pack, place()) {
        let at = |dir: &Path, what: &str| dir.join(esf::pcsx2::SERIAL).join(what).display().to_string();
        if want.0 {
            let also = place.also.as_ref().map(|a| format!(", then {}", at(a, "replacements"))).unwrap_or_default();
            crate::say(&format!("textures: {} replacements in {}{also}", p.count(esf::Era::Frontiers), at(&place.dir, "replacements")));
        }
        if want.1 {
            crate::say(&format!("textures: dumping to {}", at(&place.dir, "dumps")));
        }
    }
    render::set_pack(pack.clone().map(|p| p as Arc<dyn render::Pack>));
    st.pack = pack;
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// Waits (up to ten seconds) for the dumps handed to the writer to be
/// written, as the program ends: its thread does not outlive it.
pub fn flush() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(d) = pack().and_then(|p| p.dumper.as_ref().map(|d| d.pending.clone())) else { return };
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while d.load(Ordering::Relaxed) > 0 && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}

/// The base level to draw for a surface the 2D layer takes (`UiAssets`):
/// the pack's, else its own (`Surface::decode`).
pub fn image(c: &esf::Container, s: &esf::Surface, era: esf::Era) -> Option<esf::Image> {
    if let Some(l) = pack().and_then(|p| p.levels_for(&c.data, s, era)).and_then(|l| l.into_iter().next()) {
        return Some(l);
    }
    s.decode(&c.data)
}

/// Extended's rows of Textures (`options`) with a note of their own: Dump
/// Textures and Texture Folder (Texture Packs' is every other row's).
pub const ROW_DUMPS: i32 = 51;
pub const ROW_FOLDER: i32 = 55;

/// What the Extended page's Textures says beside Back, its row `item`
/// chosen: on Dump Textures, while it is on, how many textures have been
/// dumped; on Texture Folder where the folder is, or why it is the
/// settings' (`Beside::folder`); on Texture Packs, while it is on, how many
/// replacements the packs hold and where (`Beside::packs`).
pub fn note(item: i32) -> Option<String> {
    let c = crate::settings::client();
    if item == ROW_DUMPS {
        return c.dump_textures.then(|| format!("{} dumped", pack().map_or(0, |p| p.dumped())));
    }
    if item == ROW_FOLDER {
        return Some(place().map_or_else(|| "No folder here".to_string(), |p| p.beside.folder()));
    }
    if !c.texture_packs {
        return None;
    }
    let Some(place) = place() else { return Some("No folder here".to_string()) };
    // (none read yet in the frame it was turned on: `sync` reads them next)
    Some(match pack().map_or(0, |p| p.count(esf::Era::Frontiers)) {
        0 => format!("None in {}", place.beside.packs()),
        n => format!("{n} in {}", place.beside.packs()),
    })
}

// ---- reading a replacement -----------------------------------------------------

/// A replacement file's levels, the base first, as RGBA8.
pub fn read(path: &Path) -> Result<Vec<esf::Image>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    match loader(path) {
        Some("png") => read_png(&bytes).map(|i| vec![i]),
        Some("dds") => read_dds(&bytes),
        _ => Err("not a PNG or DDS".into()),
    }
}

/// An image whose alpha flag is set as `esf::Image`'s is: some texel's alpha
/// is not 255.
fn image_of(width: u32, height: u32, rgba: Vec<u8>) -> esf::Image {
    let alpha = rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 255);
    esf::Image { width, height, rgba, alpha }
}

/// A PNG as RGBA8: RGBA as it is, RGB with alpha 0x80 (PCSX2's `PNGLoader`:
/// "make opaque" in the GS's terms); grey, a palette and 16 bits, which
/// PCSX2 reads as nothing, made 8-bit RGB(A) first.
pub fn read_png(bytes: &[u8]) -> Result<esf::Image, String> {
    let mut d = png::Decoder::new(std::io::Cursor::new(bytes));
    d.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = d.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![0; r.output_buffer_size().ok_or("too large")?];
    let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let (w, h) = (info.width, info.height);
    let n = (w * h) as usize;
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 0x80]).collect(),
        png::ColorType::GrayscaleAlpha => px.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 0x80]).collect(),
        png::ColorType::Indexed => return Err("a palette PNG left unexpanded".into()),
    };
    if w == 0 || h == 0 || rgba.len() != n * 4 {
        return Err("empty or short".into());
    }
    Ok(image_of(w, h, rgba))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// A DDS's levels as RGBA8, as PCSX2's `DDSLoader` reads the file: BC1
/// (DXT1), BC2 (DXT2, DXT3), BC3 (DXT4, DXT5) and BC7 by their FourCC or a
/// DX10 header's format; A8R8G8B8, X8R8G8B8 (alpha 0xFF), A8B8G8R8,
/// X8B8G8R8 (alpha 0x80) and R8G8B8 (alpha 0xFF) uncompressed; its own mip
/// levels while the file holds them whole.
pub fn read_dds(b: &[u8]) -> Result<Vec<esf::Image>, String> {
    let bad = |s: &str| Err(format!("DDS: {s}"));
    if u32_at(b, 0) != Some(0x2053_4444) {
        return bad("no DDS magic");
    }
    let hdr = |i: usize| u32_at(b, 4 + i * 4).unwrap_or(0);
    let (size, flags, height, width, pitch, mips) = (hdr(0), hdr(1), hdr(2), hdr(3), hdr(4), hdr(6));
    if size < 124 || b.len() < 128 {
        return bad("short header");
    }
    if width == 0 || width >= 32768 || height == 0 || height >= 32768 || flags & 0x0080_0000 != 0 {
        return bad("not a 2D picture PCSX2 reads");
    }
    // the pixel format: +76 in the header
    let pf = |i: usize| u32_at(b, 4 + 72 + i * 4).unwrap_or(0);
    let (pf_size, pf_flags, fourcc, bits, rm, gm, bm, am) = (pf(0), pf(1), pf(2), pf(3), pf(4), pf(5), pf(6), pf(7));
    let mut at = 128;
    #[derive(Clone, Copy, PartialEq)]
    enum Fmt {
        Bc(u8),
        Rgba,
        Bgra,
        Bgrx,
        Rgbx,
        Bgr,
    }
    let fmt = if pf_flags & 4 != 0 {
        let mut dx10 = 0;
        if fourcc == u32::from_le_bytes(*b"DX10") {
            let (format, dim, array) = (u32_at(b, 128).unwrap_or(0), u32_at(b, 132).unwrap_or(0), u32_at(b, 140).unwrap_or(0));
            if dim != 3 || array != 1 {
                return bad("not one 2D picture");
            }
            dx10 = format;
            at += 20;
        }
        match (&fourcc.to_le_bytes(), dx10) {
            (b"DXT1", _) | (_, 71) => Fmt::Bc(1),
            (b"DXT2", _) | (b"DXT3", _) | (_, 74) => Fmt::Bc(2),
            (b"DXT4", _) | (b"DXT5", _) | (_, 77) => Fmt::Bc(3),
            (_, 98) => Fmt::Bc(7),
            _ => return bad("a compression PCSX2 does not read"),
        }
    } else {
        // `DDSPixelFormatMatches`: size, flags, FourCC, bit count, masks
        let m = |f: u32, n: u32, r: u32, g: u32, bb: u32, a: u32| (pf_size, pf_flags, fourcc, bits, rm, gm, bm, am) == (32, f, 0, n, r, g, bb, a);
        if m(0x41, 32, 0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000) {
            Fmt::Bgra
        } else if m(0x40, 32, 0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0) {
            Fmt::Bgrx
        } else if m(0x40, 32, 0x0000_00ff, 0x0000_ff00, 0x00ff_0000, 0) {
            Fmt::Rgbx
        } else if m(0x40, 24, 0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0) {
            Fmt::Bgr
        } else if m(0x41, 32, 0x0000_00ff, 0x0000_ff00, 0x00ff_0000, 0xff00_0000) {
            Fmt::Rgba
        } else {
            return bad("a pixel format PCSX2 does not read");
        }
    };
    let (block, bytes_per_block) = match fmt {
        Fmt::Bc(1) => (4u32, 8u32),
        Fmt::Bc(_) => (4, 16),
        Fmt::Bgr => (1, 3),
        _ => (1, 4),
    };
    if block > 1 && (width % block != 0 || height % block != 0) {
        return bad("a compressed base not a whole number of blocks");
    }
    // the levels the header names (none: the base alone; 0 with the flag:
    // the whole chain), each read while the file holds it whole
    let count = if flags & 0x0002_0000 != 0 {
        if mips != 0 { mips } else { 32 - width.max(height).leading_zeros() }
    } else {
        1
    };
    let mut out = Vec::new();
    for level in 0..count.max(1) {
        let (w, h) = ((width >> level).max(1), (height >> level).max(1));
        let (bw, bh) = (w.div_ceil(block).max(1), h.div_ceil(block).max(1));
        // the base's pitch from the header when it gives both flags
        let row = if level == 0 && flags & 0x8 != 0 && flags & 0x0008_0000 != 0 {
            if pitch < bytes_per_block {
                return bad("its pitch");
            }
            pitch
        } else {
            bw * bytes_per_block
        };
        let Some(data) = b.get(at..at + (row * bh) as usize) else {
            if level == 0 {
                return bad("short of its base");
            }
            break;
        };
        at += (row * bh) as usize;
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        match fmt {
            Fmt::Bc(k) => {
                let mut px = [0u8; 64];
                for by in 0..bh {
                    for bx in 0..bw {
                        let src = &data[(by * row + bx * bytes_per_block) as usize..][..bytes_per_block as usize];
                        match k {
                            1 => bcdec_rs::bc1(src, &mut px, 16),
                            2 => bcdec_rs::bc2(src, &mut px, 16),
                            3 => bcdec_rs::bc3(src, &mut px, 16),
                            _ => bcdec_rs::bc7(src, &mut px, 16),
                        }
                        for y in 0..4 {
                            for x in 0..4 {
                                let (tx, ty) = (bx * 4 + x, by * 4 + y);
                                if tx < w && ty < h {
                                    let d = ((ty * w + tx) * 4) as usize;
                                    rgba[d..d + 4].copy_from_slice(&px[((y * 4 + x) * 4) as usize..][..4]);
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                for y in 0..h {
                    for x in 0..w {
                        let s = &data[(y * row + x * bytes_per_block) as usize..];
                        let d = ((y * w + x) * 4) as usize;
                        rgba[d..d + 4].copy_from_slice(&match fmt {
                            Fmt::Rgba => [s[0], s[1], s[2], s[3]],
                            Fmt::Bgra => [s[2], s[1], s[0], s[3]],
                            Fmt::Bgrx => [s[2], s[1], s[0], 0xFF],
                            Fmt::Rgbx => [s[0], s[1], s[2], 0x80],
                            _ => [s[2], s[1], s[0], 0xFF],
                        });
                    }
                }
            }
        }
        out.push(image_of(w, h, rgba));
    }
    Ok(out)
}

/// A replacement's levels for a surface: as many as the surface keeps
/// (`Surface::levels`) and the levels it is larger by (`render::level_shift`,
/// rounded; fewer for a smaller one), down to one texel a side at most; the
/// file's own while it has them, each after them half the one before (the
/// mean of each 2 x 2, as the levels on the disc are made).
pub fn with_levels(mut levels: Vec<esf::Image>, s: &esf::Surface) -> Vec<esf::Image> {
    let base = (levels[0].width, levels[0].height);
    let shift = render::level_shift(base, Some((s.width.max(1) as u32, s.height.max(1) as u32)));
    let chain = 32 - base.0.max(base.1).leading_zeros() as usize;
    let want = ((s.levels() as f32 + shift).round().max(1.0) as usize).min(chain);
    // a level of the file's that is not half the one before ends its own
    let mut own = 1;
    while own < levels.len() && own < want && (levels[own].width, levels[own].height) == ((base.0 >> own).max(1), (base.1 >> own).max(1)) {
        own += 1;
    }
    levels.truncate(own);
    while levels.len() < want {
        let next = half(levels.last().unwrap());
        levels.push(next);
    }
    levels
}

/// A level half the size of `l` each way (one texel at least): each texel
/// the mean of the 2 x 2 (or 2 x 1) it covers, rounded.
fn half(l: &esf::Image) -> esf::Image {
    let (w, h) = ((l.width / 2).max(1), (l.height / 2).max(1));
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let xs = [(2 * x).min(l.width - 1), (2 * x + 1).min(l.width - 1)];
            let ys = [(2 * y).min(l.height - 1), (2 * y + 1).min(l.height - 1)];
            for c in 0..4 {
                let mut sum = 0u32;
                for &yy in &ys {
                    for &xx in &xs {
                        sum += l.rgba[((yy * l.width + xx) * 4 + c) as usize] as u32;
                    }
                }
                rgba[((y * w + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
            }
        }
    }
    image_of(w, h, rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A surface record's body: `w` x `h` at 8 bits with `levels` levels,
    /// each a row length and its rows; palette entry i is (i, 255 - i, i / 2,
    /// 255), level k's texel (x, y) is x * 3 + y * 5 + k.
    fn record(w: usize, h: usize, levels: usize) -> Vec<u8> {
        let mut d = Vec::new();
        for v in [0x1234, w as i32, h as i32, 1, levels as i32] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&256i32.to_le_bytes());
        for i in 0..256u32 {
            d.extend_from_slice(&[i as u8, 255 - i as u8, (i / 2) as u8, 255]);
        }
        let (mut lw, mut lh) = (w, h);
        for k in 0..levels {
            d.extend_from_slice(&(lw as i32).to_le_bytes());
            for y in 0..lh {
                for x in 0..lw {
                    d.push((x * 3 + y * 5 + k) as u8);
                }
            }
            (lw, lh) = ((lw / 2).max(1), (lh / 2).max(1));
        }
        d
    }

    /// `record`'s surface in a container of its own (a 0x1000 record:
    /// type, version, size, children, then the body).
    fn surface(w: usize, h: usize, levels: usize) -> (esf::Container, esf::Surface) {
        let body = record(w, h, levels);
        let mut d = Vec::new();
        d.extend_from_slice(&0x1000u16.to_le_bytes());
        d.extend_from_slice(&0i16.to_le_bytes());
        d.extend_from_slice(&(body.len() as i32).to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&body);
        let c = esf::Container::parse_records(d);
        let s = c.surfaces[0].clone();
        (c, s)
    }

    fn png_bytes(w: u32, h: u32, color: png::ColorType, depth: png::BitDepth, px: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, w, h);
            e.set_color(color);
            e.set_depth(depth);
            e.write_header().unwrap().write_image_data(px).unwrap();
        }
        out
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("eqoa-texpack-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A PNG as PCSX2's loader takes it: RGBA as it is, RGB with alpha 0x80
    /// (the GS's 1.0); grey and 16 bits made 8-bit RGB(A) first. The control:
    /// RGB's alpha is not 255.
    #[test]
    fn a_png_as_pcsx2_reads_it() {
        let rgba = read_png(&png_bytes(2, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[1, 2, 3, 4, 5, 6, 7, 255])).unwrap();
        assert_eq!((rgba.width, rgba.height, rgba.rgba.as_slice(), rgba.alpha), (2, 1, &[1u8, 2, 3, 4, 5, 6, 7, 255][..], true));
        let rgb = read_png(&png_bytes(2, 1, png::ColorType::Rgb, png::BitDepth::Eight, &[1, 2, 3, 4, 5, 6])).unwrap();
        assert_eq!(rgb.rgba, vec![1, 2, 3, 0x80, 4, 5, 6, 0x80], "RGB: alpha 0x80, as PCSX2 makes it opaque");
        assert_ne!(rgb.rgba[3], 255, "the control: not 255");
        let grey = read_png(&png_bytes(1, 1, png::ColorType::Grayscale, png::BitDepth::Eight, &[9])).unwrap();
        assert_eq!(grey.rgba, vec![9, 9, 9, 0x80]);
        let ga = read_png(&png_bytes(1, 1, png::ColorType::GrayscaleAlpha, png::BitDepth::Eight, &[9, 7])).unwrap();
        assert_eq!(ga.rgba, vec![9, 9, 9, 7]);
        let deep = read_png(&png_bytes(1, 1, png::ColorType::Rgb, png::BitDepth::Sixteen, &[0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC])).unwrap();
        assert_eq!(deep.rgba, vec![0x12, 0x56, 0x9A, 0x80], "16 bits: the high byte");
        assert!(read_png(b"not a png").is_err());
    }

    /// A DDS header (`DDS_HEADER`, 124 bytes after the magic) for `w` x `h`,
    /// `mips` levels named when not 0, and the pixel format given.
    fn dds(w: u32, h: u32, mips: u32, pf: [u32; 8], dx10: Option<u32>, body: &[u8]) -> Vec<u8> {
        let mut b = b"DDS ".to_vec();
        let flags = 0x1007 | if mips > 0 { 0x0002_0000 } else { 0 };
        for v in [124, flags, h, w, 0, 0, mips] {
            b.extend_from_slice(&u32::to_le_bytes(v));
        }
        b.extend_from_slice(&[0; 44]);
        for v in pf {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&[0; 20]);
        if let Some(f) = dx10 {
            for v in [f, 3, 0, 1, 0] {
                b.extend_from_slice(&u32::to_le_bytes(v));
            }
        }
        b.extend_from_slice(body);
        b
    }

    /// The DDS forms PCSX2 reads, as it reads them: A8R8G8B8 swapped to RGBA,
    /// X8R8G8B8 alpha 0xFF, X8B8G8R8 alpha 0x80, R8G8B8 alpha 0xFF, A8B8G8R8
    /// as it is; a BC1 block and its mip levels; a DX10 header's BC7. The
    /// controls: an unknown format, a volume, a compressed base not of whole
    /// blocks, and a base the file is short of, are refused.
    #[test]
    fn a_dds_as_pcsx2_reads_it() {
        let px = [1u8, 2, 3, 4, 5, 6, 7, 8];
        let argb = [32, 0x41, 0, 32, 0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0xff00_0000];
        assert_eq!(read_dds(&dds(2, 1, 0, argb, None, &px)).unwrap()[0].rgba, vec![3, 2, 1, 4, 7, 6, 5, 8]);
        let xrgb = [32, 0x40, 0, 32, 0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0];
        assert_eq!(read_dds(&dds(2, 1, 0, xrgb, None, &px)).unwrap()[0].rgba, vec![3, 2, 1, 0xFF, 7, 6, 5, 0xFF]);
        let xbgr = [32, 0x40, 0, 32, 0x0000_00ff, 0x0000_ff00, 0x00ff_0000, 0];
        assert_eq!(read_dds(&dds(2, 1, 0, xbgr, None, &px)).unwrap()[0].rgba, vec![1, 2, 3, 0x80, 5, 6, 7, 0x80]);
        let bgr = [32, 0x40, 0, 24, 0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0];
        assert_eq!(read_dds(&dds(2, 1, 0, bgr, None, &px[..6])).unwrap()[0].rgba, vec![3, 2, 1, 0xFF, 6, 5, 4, 0xFF]);
        let abgr = [32, 0x41, 0, 32, 0x0000_00ff, 0x0000_ff00, 0x00ff_0000, 0xff00_0000];
        assert_eq!(read_dds(&dds(2, 1, 0, abgr, None, &px)).unwrap()[0].rgba, px.to_vec());
        // BC1: a block of one colour (both ends pure red, 0xF800), 8 x 4 and
        // its two levels after it (4 x 2 and 2 x 1, a block each)
        let red = [0x00u8, 0xF8, 0x00, 0xF8, 0, 0, 0, 0];
        let dxt1 = [32, 4, u32::from_le_bytes(*b"DXT1"), 0, 0, 0, 0, 0];
        let body: Vec<u8> = red.iter().copied().cycle().take(8 * 4).collect();
        let l = read_dds(&dds(8, 4, 3, dxt1, None, &body)).unwrap();
        assert_eq!(l.iter().map(|i| (i.width, i.height)).collect::<Vec<_>>(), vec![(8, 4), (4, 2), (2, 1)]);
        assert!(l.iter().all(|i| i.rgba.chunks(4).all(|p| p == [255, 0, 0, 255])), "pure red, opaque");
        // a level the file is short of ends the list
        assert_eq!(read_dds(&dds(8, 4, 3, dxt1, None, &body[..24])).unwrap().len(), 2);
        // the flag with a count of 0: the whole chain (8 x 4 to 1 x 1)
        let whole: Vec<u8> = red.iter().copied().cycle().take(8 * 5).collect();
        assert_eq!(read_dds(&dds(8, 4, 0, dxt1, None, &whole)).unwrap().len(), 1, "no flag: the base alone");
        let mut flagged = dds(8, 4, 0, dxt1, None, &whole);
        flagged[4 + 4 + 2] |= 0x02; // DDSD_MIPMAPCOUNT (0x20000), its count 0
        assert_eq!(read_dds(&flagged).unwrap().len(), 4, "8x4, 4x2, 2x1, 1x1");
        // BC7 through a DX10 header (format 98)
        let dx10 = [32, 4, u32::from_le_bytes(*b"DX10"), 0, 0, 0, 0, 0];
        let bc7 = read_dds(&dds(4, 4, 0, dx10, Some(98), &[0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])).unwrap();
        assert_eq!((bc7[0].width, bc7[0].height, bc7[0].rgba.len()), (4, 4, 64));
        // the controls
        let unknown = [32, 4, u32::from_le_bytes(*b"ATI2"), 0, 0, 0, 0, 0];
        assert!(read_dds(&dds(4, 4, 0, unknown, None, &[0; 16])).is_err());
        let mut volume = dds(2, 1, 0, abgr, None, &px);
        volume[4 + 4 + 2] |= 0x80; // DDSD_DEPTH
        assert!(read_dds(&volume).is_err());
        assert!(read_dds(&dds(6, 4, 0, dxt1, None, &[0; 16])).is_err(), "6 wide: not whole blocks");
        assert!(read_dds(&dds(8, 4, 0, dxt1, None, &red)).is_err(), "short of its base");
    }

    /// A pack's folder read as PCSX2 reads it: at any depth, `.png` and
    /// `.dds` in any case, names PCSX2 reads only (a dumped level's
    /// `-mip<n>` and other files passed over), of two with one name the
    /// first in path order.
    #[test]
    fn a_pack_folder_as_pcsx2_reads_it() {
        let d = temp("scan");
        std::fs::create_dir_all(d.join("a/b")).unwrap();
        for f in ["1-2-00001993.png", "a/3-4-00001993.PNG", "a/b/5-00000001.dds", "a/b/1-2-00001993.png", "6-7-00001993-mip1.png", "readme.txt", "8-9-00001993.jpg"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        let ix = Index::scan(&d);
        let mut names: Vec<String> = ix.files.keys().map(|n| n.file_name(0)).collect();
        names.sort();
        assert_eq!(names, vec!["1-2-00001993.png", "3-4-00001993.png", "5-00000001.png"]);
        let n = Name::parse("1-2-00001993.png").unwrap();
        assert_eq!(ix.files[&n], d.join("1-2-00001993.png"), "the first in path order");
        assert!(ix.cluts.contains(&2) && ix.cluts.contains(&4));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A replacement's levels: a larger one gets the levels it is larger by
    /// (a 256 for a 64 with three: five, down to 16), the file's own first;
    /// one the surface's size its three; a smaller its fewer. The control:
    /// each level is half the one before, the mean of its 2 x 2.
    #[test]
    fn a_replacements_levels() {
        let (_, s) = surface(64, 64, 3);
        assert_eq!(s.levels(), 3);
        let flat = |w: u32, v: u8| image_of(w, w, vec![v; (w * w * 4) as usize]);
        let sizes = |l: &[esf::Image]| l.iter().map(|i| i.width).collect::<Vec<_>>();
        assert_eq!(sizes(&with_levels(vec![flat(256, 9)], &s)), vec![256, 128, 64, 32, 16]);
        assert_eq!(sizes(&with_levels(vec![flat(64, 9)], &s)), vec![64, 32, 16]);
        assert_eq!(sizes(&with_levels(vec![flat(32, 9)], &s)), vec![32, 16]);
        // a DDS's own levels kept (their values), the rest made after them
        let own = with_levels(vec![flat(256, 1), flat(128, 2), flat(64, 3)], &s);
        assert_eq!(sizes(&own), vec![256, 128, 64, 32, 16]);
        assert_eq!((own[1].rgba[0], own[2].rgba[0], own[3].rgba[0]), (2, 3, 3), "its own, then made from its last");
        // a level of the file's not half the one before is not taken as one
        assert_eq!(with_levels(vec![flat(256, 1), flat(100, 2)], &s)[1].rgba[0], 1);
        let l = image_of(2, 2, vec![0, 0, 0, 0, 2, 2, 2, 2, 4, 4, 4, 4, 255, 255, 255, 255]);
        assert_eq!(half(&l).rgba, vec![65, 65, 65, 65], "(0 + 2 + 4 + 255 + 2) / 4");
        let level_shift = render::level_shift;
        assert_eq!((level_shift((256, 256), Some((64, 64))), level_shift((64, 64), Some((64, 64))), level_shift((32, 32), Some((64, 64)))), (2.0, 0.0, -1.0));
        assert_eq!(level_shift((128, 64), Some((64, 64))), 1.0, "the larger ratio");
        assert_eq!(level_shift((128, 128), None), 0.0, "no size known: none");
    }

    /// A surface's stand-in is the first of its names a pack has, in
    /// `GsTexture::names`' order (the whole chain's before the base's alone);
    /// a pack none of whose files has the surface's CLUT reads nothing. The
    /// control: with neither name, none.
    #[test]
    fn a_stand_in_by_its_names_in_order() {
        let (c, s) = surface(32, 32, 2);
        let d = &c.data;
        let t = s.gs(d).unwrap();
        let names = t.names();
        assert_eq!(names.len(), 3, "the chain, the base, the chain from level 1");
        let dir = temp("find");
        let pic = |name: &Name, v: u8| {
            let p = dir.join(name.file_name(0));
            std::fs::write(&p, png_bytes(32, 32, png::ColorType::Rgba, png::BitDepth::Eight, &vec![v; 32 * 32 * 4])).unwrap();
            p
        };
        let pack = |files: Vec<(Name, PathBuf)>| {
            let mut ix = Index::default();
            for (n, p) in files {
                ix.cluts.insert(n.clut_hash);
                ix.files.insert(n, p);
            }
            Pack { generation: 1, packs: Some([ix, Index::default()]), found: Mutex::new(HashMap::new()), dumper: None }
        };
        let (chain, base) = (pic(&names[0].0, 10), pic(&names[1].0, 20));
        let both = pack(vec![(names[0].0, chain.clone()), (names[1].0, base.clone())]);
        let got = both.levels_for(d, &s, esf::Era::Frontiers).unwrap();
        assert_eq!((got[0].rgba[0], got.len()), (10, 2), "the chain's, its two levels");
        assert!(both.levels_for(d, &s, esf::Era::Y2002).is_none(), "the 2002 disc's folder has none");
        let only_base = pack(vec![(names[1].0, base)]);
        assert_eq!(only_base.levels_for(d, &s, esf::Era::Frontiers).unwrap()[0].rgba[0], 20);
        let other = Name { clut_hash: names[0].0.clut_hash ^ 1, ..names[0].0 };
        assert!(pack(vec![(other, chain)]).levels_for(d, &s, esf::Era::Frontiers).is_none(), "the control: another CLUT");
        assert!(pack(vec![]).levels_for(d, &s, esf::Era::Frontiers).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A dump writes every name of the texture, each its first level's
    /// picture as PCSX2 dumps it (its CLUT's colours), and leaves a file
    /// there already as it is.
    #[test]
    fn a_dump_writes_every_name() {
        let (c, s) = surface(32, 32, 2);
        let d = &c.data;
        let t = s.gs(d).unwrap();
        let dir = temp("dump");
        let kept = dir.join(t.names()[1].0.file_name(0));
        std::fs::write(&kept, b"mine").unwrap();
        let dumper = Dumper::new([Some(dir.clone()), None]).unwrap();
        dumper.dump(d, &s, esf::Era::Frontiers);
        dumper.dump(d, &s, esf::Era::Frontiers);
        dumper.dump(d, &s, esf::Era::Y2002);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while dumper.pending.load(Ordering::Relaxed) > 0 && std::time::Instant::now() < until {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(dumper.written.load(Ordering::Relaxed), 2, "two of three names, once each");
        assert_eq!(std::fs::read(&kept).unwrap(), b"mine", "a file there already left as it is");
        for (n, level) in t.names() {
            if n == t.names()[1].0 {
                continue;
            }
            let img = read_png(&std::fs::read(dir.join(n.file_name(0))).unwrap()).unwrap();
            assert_eq!((img.width, img.rgba), (t.levels[level].width, t.rgba(level)), "{}", n.file_name(0));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- where the folders are ----

    /// The folder a player finds the program in: its own; a Mac app's the
    /// folder holding the app; an AppImage's the image's folder while the
    /// program runs from inside its mount. The controls: the same app's
    /// path off a Mac, a `Contents/MacOS` in no `.app`, a program outside
    /// the AppImage's mount (one it started), and an image or a mount not
    /// named absolutely, are each the program's own folder.
    #[test]
    fn the_folder_a_player_finds_the_program_in() {
        let p = |s: &str| PathBuf::from(s);
        let got = |exe: &str, appimage: Option<(&str, &str)>, mac: bool| {
            let appimage = appimage.map(|(i, m)| (p(i), p(m)));
            program_folder(&p(exe), appimage.as_ref().map(|(i, m)| (i.as_path(), m.as_path())), mac)
        };
        let want = |d: &str, n: &str| Some((p(d), n.to_string()));
        assert_eq!(got("/home/x/eqoa-vanilla-linux-x86_64/eqoa", None, false), want("/home/x/eqoa-vanilla-linux-x86_64", "eqoa"), "the release's archive");
        assert_eq!(got("/home/x/Downloads/eqoa-vanilla-linux-x86_64", None, false), want("/home/x/Downloads", "eqoa-vanilla-linux-x86_64"), "its bare program");
        assert_eq!(got("/Users/x/eqoa-vanilla-macos/eqoa", None, true), want("/Users/x/eqoa-vanilla-macos", "eqoa"), "the release's Mac program, no app");
        let app = "/Users/x/Games/Eqoa.app/Contents/MacOS/eqoa";
        assert_eq!(got(app, None, true), want("/Users/x/Games", "Eqoa.app"), "a Mac app: the folder holding it");
        assert_eq!(got(app, None, false), want("/Users/x/Games/Eqoa.app/Contents/MacOS", "eqoa"), "the control: off a Mac");
        assert_eq!(got("/Users/x/Games/Eqoa/Contents/MacOS/eqoa", None, true), want("/Users/x/Games/Eqoa/Contents/MacOS", "eqoa"), "the control: no .app");
        let image = Some(("/home/x/Apps/Eqoa-x86_64.AppImage", "/tmp/.mount_EqoaAb"));
        assert_eq!(got("/tmp/.mount_EqoaAb/usr/bin/eqoa", image, false), want("/home/x/Apps", "Eqoa-x86_64.AppImage"), "an AppImage: the image's folder");
        assert_eq!(got("/home/x/eqoa/eqoa", image, false), want("/home/x/eqoa", "eqoa"), "the control: started by an AppImage, not in one");
        assert_eq!(got("/tmp/.mount_EqoaAb/usr/bin/eqoa", Some(("Eqoa.AppImage", "/tmp/.mount_EqoaAb")), false), want("/tmp/.mount_EqoaAb/usr/bin", "eqoa"), "an image named relatively");
        assert_eq!(got("/tmp/.mount_EqoaAb/usr/bin/eqoa", Some(("/home/x/Eqoa.AppImage", "")), false), want("/tmp/.mount_EqoaAb/usr/bin", "eqoa"), "an empty mount");
    }

    /// A test's program folder and config directory, under one of its own:
    /// `game` and `config`.
    fn folders(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let d = temp(name);
        let (game, config) = (d.join("game"), d.join("config"));
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(&config).unwrap();
        (d, game, config)
    }

    /// A pack's file in the `textures` folder `t`, at `rel` under the
    /// Frontiers disc's replacements.
    fn put(t: &Path, rel: &str, b: &[u8]) -> PathBuf {
        let p = t.join(esf::pcsx2::SERIAL).join("replacements").join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b).unwrap();
        p
    }

    /// `dir`'s mode set (Unix), and whether this test can be refused at all
    /// (not as root, whom permissions do not stop).
    #[cfg(unix)]
    fn mode(dir: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[cfg(unix)]
    fn refused(dir: &Path) -> bool {
        let probe = dir.join("probe");
        let wrote = std::fs::write(&probe, b"").is_ok();
        let _ = std::fs::remove_file(&probe);
        if wrote {
            eprintln!("skipped: {} was written to with its write permission off (root?)", dir.display());
        }
        !wrote
    }

    /// Beside the program: a `textures` made there, none in the settings;
    /// the settings' from before moved there whole, a pack and dumps in it
    /// (one rename), and found as it is the next time. The control: one
    /// beside the program already (the player's, or another copy of the
    /// game's) moves nothing, and the old one is read after it.
    #[test]
    fn beside_the_program_and_the_old_folder_moved_there() {
        let (d, game, config) = folders("beside");
        let p = resolve(Some((&game, "eqoa.exe")), &config);
        assert_eq!(p, Place { dir: game.join("textures"), also: None, beside: Beside::Program("eqoa.exe".into()), moved: None, why: None });
        assert!(game.join("textures").is_dir(), "made for the player to find");
        assert!(!config.join("textures").exists(), "none in the settings");
        assert_eq!(std::fs::read_dir(game.join("textures")).unwrap().count(), 0, "the write test's file taken away");
        let _ = std::fs::remove_dir_all(&d);

        let (d, game, config) = folders("moved");
        let (old, new) = (config.join("textures"), game.join("textures"));
        let pack = put(&old, "a/1-2-00001993.png", b"pack");
        let dump = old.join(esf::pcsx2::SERIAL).join("dumps").join("3-4-00001993.png");
        std::fs::create_dir_all(dump.parent().unwrap()).unwrap();
        std::fs::write(&dump, b"dump").unwrap();
        let p = resolve(Some((&game, "eqoa")), &config);
        assert_eq!((&p.dir, &p.also, &p.moved, &p.why), (&new, &None, &Some(old.clone()), &None));
        let there = |f: &Path| new.join(f.strip_prefix(&old).unwrap());
        assert_eq!((std::fs::read(there(&pack)).unwrap(), std::fs::read(there(&dump)).unwrap()), (b"pack".to_vec(), b"dump".to_vec()));
        assert!(!old.exists(), "moved, not copied");
        assert_eq!(resolve(Some((&game, "eqoa")), &config), Place { moved: None, ..p }, "the next start: as it is now");
        let _ = std::fs::remove_dir_all(&d);

        let (d, game, config) = folders("both");
        let (old, new) = (config.join("textures"), game.join("textures"));
        let kept = put(&old, "1-2-00001993.png", b"old");
        put(&new, "5-6-00001993.png", b"new");
        let p = resolve(Some((&game, "eqoa")), &config);
        assert_eq!((&p.dir, &p.also, &p.moved), (&new, &Some(old.clone()), &None), "the control: one there already");
        assert_eq!(std::fs::read(&kept).unwrap(), b"old", "left where it is");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A program folder the game cannot write to (Program Files, /usr/bin,
    /// a read-only mount): the settings' folder, as before, said so, with a
    /// `textures` beside the program read after it; with none there, none
    /// made. The control: the same folder writable is the one beside it.
    #[cfg(unix)]
    #[test]
    fn a_program_folder_that_cannot_be_written_to() {
        let (d, game, config) = folders("readonly");
        let (old, new) = (config.join("textures"), game.join("textures"));
        let mine = put(&old, "3-4-00001993.png", b"mine");
        put(&new, "1-2-00001993.png", b"theirs");
        mode(&new, 0o555);
        mode(&game, 0o555);
        if refused(&new) {
            let p = resolve(Some((&game, "eqoa.exe")), &config);
            assert_eq!((&p.dir, &p.also, &p.moved), (&old, &Some(new.clone()), &None));
            assert_eq!(p.beside, Beside::Settings { program: "eqoa.exe".into(), config: "config".into() });
            assert!(p.why.as_deref().is_some_and(|w| w.contains("cannot be written to")), "{:?}", p.why);
            assert_eq!((p.beside.packs(), p.beside.folder()), ("config's textures".to_string(), "eqoa.exe's folder is read-only".to_string()));
            assert_eq!(std::fs::read(&mine).unwrap(), b"mine", "nothing moved");
            // a read-only folder with none beside the program
            let bare = d.join("bare");
            std::fs::create_dir_all(&bare).unwrap();
            mode(&bare, 0o555);
            let p = resolve(Some((&bare, "eqoa")), &config);
            assert_eq!((&p.dir, &p.also), (&old, &None));
            assert!(!bare.join("textures").exists(), "none made there");
            mode(&bare, 0o755);
        }
        mode(&game, 0o755);
        mode(&new, 0o755);
        let p = resolve(Some((&game, "eqoa.exe")), &config);
        assert_eq!((&p.dir, &p.beside), (&new, &Beside::Program("eqoa.exe".into())), "the control: writable");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The settings' folder that cannot be moved (on another disk, where a
    /// rename is refused): read after the new one, which takes the dumps.
    /// Refused here by its config directory's permissions, and, where the
    /// machine has a RAM disk on another device (/dev/shm), by another
    /// disk; a link to a folder is never moved (a relative one would point
    /// elsewhere). The control: the refusal lifted, it is moved.
    #[cfg(unix)]
    #[test]
    fn an_old_folder_not_moved_is_read_after_the_new() {
        use std::os::unix::fs::MetadataExt;
        let (d, game, config) = folders("kept");
        let (old, new) = (config.join("textures"), game.join("textures"));
        put(&old, "1-2-00001993.png", b"old");
        mode(&config, 0o555);
        if refused(&config) {
            let p = resolve(Some((&game, "eqoa")), &config);
            assert_eq!((&p.dir, &p.also, &p.moved), (&new, &Some(old.clone()), &None));
            assert!(p.why.as_deref().is_some_and(|w| w.contains("was not moved")), "{:?}", p.why);
            let pack = Pack::at(&p, true, true);
            assert_eq!(pack.count(esf::Era::Frontiers), 1, "its pack read");
            assert_eq!(pack.dumper.as_ref().unwrap().dirs[0], Some(new.join(esf::pcsx2::SERIAL).join("dumps")), "dumps to the new one");
        }
        mode(&config, 0o755);
        let _ = std::fs::remove_dir_all(&new);
        let p = resolve(Some((&game, "eqoa")), &config);
        assert_eq!((&p.also, &p.moved), (&None, &Some(old.clone())), "the control: moved");
        let _ = std::fs::remove_dir_all(&d);

        // another disk
        let shm = Path::new("/dev/shm");
        let other = shm.join(format!("eqoa-texpack-disk-{}", std::process::id()));
        let devices = std::fs::metadata(shm).ok().zip(std::fs::metadata(std::env::temp_dir()).ok()).map(|(a, b)| (a.dev(), b.dev()));
        if devices.is_some_and(|(a, b)| a != b) && std::fs::create_dir_all(other.join("config")).is_ok() {
            let (d, game, _) = folders("disk");
            let config = other.join("config");
            put(&config.join("textures"), "1-2-00001993.png", b"old");
            let p = resolve(Some((&game, "eqoa")), &config);
            assert_eq!((&p.dir, &p.also, &p.moved), (&game.join("textures"), &Some(config.join("textures")), &None), "another disk");
            assert!(p.why.as_deref().is_some_and(|w| w.contains("was not moved")), "{:?}", p.why);
            let _ = std::fs::remove_dir_all(&other);
            let _ = std::fs::remove_dir_all(&d);
        } else {
            eprintln!("no second disk to hand: another disk's refusal not shown");
        }

        // a link
        let (d, game, config) = folders("link");
        let real = d.join("packs");
        put(&real, "1-2-00001993.png", b"linked");
        std::os::unix::fs::symlink(&real, config.join("textures")).unwrap();
        let p = resolve(Some((&game, "eqoa")), &config);
        assert_eq!((&p.also, &p.moved), (&Some(config.join("textures")), &None), "a link left");
        assert_eq!(Pack::at(&p, true, false).count(esf::Era::Frontiers), 1, "and read");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Android (no program folder of a player's): the settings' folder, as
    /// before, nothing made until it is wanted; settings kept in the
    /// program's own folder (`$EQOA_CONFIG` there): the one folder.
    #[test]
    fn android_and_settings_in_the_programs_folder() {
        let (d, game, config) = folders("only");
        assert_eq!(resolve(None, &config), Place { dir: config.join("textures"), also: None, beside: Beside::Only, moved: None, why: None });
        assert!(!config.join("textures").exists());
        assert_eq!(Beside::Only.packs(), "textures/SLUS-20744", "as the page said before");
        put(&game.join("textures"), "1-2-00001993.png", b"here");
        let p = resolve(Some((&game, "eqoa")), &game);
        assert_eq!((&p.dir, &p.also, &p.moved), (&game.join("textures"), &None, &None));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The packs of `dir` read before `also`'s: a name both have is `dir`'s
    /// file, one `also` alone has its, each disc's; dumps go to `dir`
    /// alone. The control: without `also` its files are not read.
    #[test]
    fn packs_read_in_both_folders_and_dumps_written_in_the_first() {
        let (d, game, config) = folders("two");
        let (new, old) = (game.join("textures"), config.join("textures"));
        let mine = put(&new, "1-2-00001993.png", b"new");
        put(&old, "1-2-00001993.png", b"old");
        let theirs = put(&old, "x/3-4-00001993.png", b"old alone");
        let y2002 = old.join(SERIAL_2002).join("replacements").join("7-8-00001993.png");
        std::fs::create_dir_all(y2002.parent().unwrap()).unwrap();
        std::fs::write(&y2002, b"2002").unwrap();
        let place = Place { dir: new.clone(), also: Some(old.clone()), beside: Beside::Program("eqoa".into()), moved: None, why: None };
        let pack = Pack::at(&place, true, true);
        let packs = pack.packs.as_ref().unwrap();
        let file = |era: usize, n: &str| packs[era].files.get(&Name::parse(n).unwrap()).cloned();
        assert_eq!(file(0, "1-2-00001993.png"), Some(mine), "the new folder's");
        assert_eq!(file(0, "3-4-00001993.png"), Some(theirs), "the old folder's own");
        assert_eq!(file(1, "7-8-00001993.png"), Some(y2002), "the 2002 disc's, there too");
        let dirs = &pack.dumper.as_ref().unwrap().dirs;
        assert_eq!(dirs, &[Some(new.join(esf::pcsx2::SERIAL).join("dumps")), Some(new.join(SERIAL_2002).join("dumps"))]);
        assert!(new.join(esf::pcsx2::SERIAL).join("dumps").is_dir() && !old.join(esf::pcsx2::SERIAL).join("dumps").exists());
        let alone = Pack::at(&Place { also: None, ..place }, true, false);
        assert_eq!((alone.count(esf::Era::Frontiers), alone.count(esf::Era::Y2002)), (1, 0), "the control: the new folder's alone");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// What Extended's Textures says beside Back, row by row, with a folder
    /// beside the program, one in the settings (the program's read-only),
    /// and none (a run that keeps no settings); Texture Folder shows the
    /// Frontiers disc's folder, made with its replacements as PCSX2 makes
    /// it. The control: with no folder it shows none.
    #[test]
    fn what_the_textures_page_says_and_shows() {
        // Texture Packs' row
        const PACKS: i32 = 50;
        crate::settings::testing::reset(false);
        let d = temp("note");
        let set = |texture_packs: bool, dump_textures: bool| crate::settings::set_client(crate::settings::ClientOptions { texture_packs, dump_textures, ..crate::settings::client() });
        testing::set_place(None);
        assert_eq!(note(ROW_FOLDER).as_deref(), Some("No folder here"));
        assert!(!open_folder() && testing::shown().is_empty(), "the control: none to show");
        set(true, false);
        assert_eq!(note(PACKS).as_deref(), Some("No folder here"));
        let beside = Place { dir: d.join("textures"), also: None, beside: Beside::Program("eqoa.exe".into()), moved: None, why: None };
        testing::set_place(Some(beside.clone()));
        assert_eq!(note(PACKS).as_deref(), Some("None in eqoa.exe's textures"));
        assert_eq!(note(ROW_FOLDER).as_deref(), Some("Beside eqoa.exe"));
        assert_eq!(note(ROW_DUMPS), None, "Dump Textures off");
        set(false, true);
        assert_eq!((note(PACKS), note(ROW_DUMPS).as_deref()), (None, Some("0 dumped")));
        assert!(open_folder());
        let serial = d.join("textures").join(esf::pcsx2::SERIAL);
        assert_eq!(testing::shown(), vec![serial.clone()]);
        assert!(serial.join("replacements").is_dir());
        let settings = Place { beside: Beside::Settings { program: "eqoa.exe".into(), config: "eqoa-rs".into() }, ..beside };
        testing::set_place(Some(settings));
        set(true, false);
        assert_eq!(note(PACKS).as_deref(), Some("None in eqoa-rs's textures"));
        assert_eq!(note(ROW_FOLDER).as_deref(), Some("eqoa.exe's folder is read-only"));
        // a long name (the release's bare download's, an AppImage's) as the
        // game, so that the note fits
        let long = "eqoa-vanilla-windows-x86_64.exe".to_string();
        assert_eq!((Beside::Program(long.clone()).packs(), Beside::Program(long.clone()).folder()), ("the game's textures".to_string(), "Beside the game".to_string()));
        let config = "a-config-folder-of-a-test".to_string();
        assert_eq!(Beside::Settings { program: long, config }.packs(), "the settings' textures");
        assert_eq!(Beside::Settings { program: "Eqoa-x86_64.AppImage".into(), config: "eqoa-rs".into() }.folder(), "The game's folder is read-only");
        testing::set_place(None);
        crate::settings::testing::reset(false);
        let _ = std::fs::remove_dir_all(&d);
    }
}
