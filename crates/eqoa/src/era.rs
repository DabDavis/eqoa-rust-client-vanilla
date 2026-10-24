//! The 2002 disc's art (the client's own: F9, and the Extended page's Art):
//! the game drawn as EverQuest Online Adventures was before Frontiers, out of
//! the player's own disc of it, EverQuest Online Adventures (USA), SLUS-20470
//! (November 2002), beside the Frontiers disc the game runs off and plays by.
//!
//! The two discs name their art alike (a DictID is the hash of the art's
//! name), so a 2002 file stands in for its Frontiers namesake in the library
//! (`esf::Library::stand_in`) and the library answers lookups as the 2002 disc
//! would while its view is 2002's: every race's body, face, hair, armour,
//! helm and robe, and every creature the 2002 disc has, is its 2002 art; what
//! only Frontiers has (its new creatures, worlds and icons) is Frontiers'.
//! Tunaria itself comes off the 2002 disc (the walk's world swapped in place,
//! `viewer`), its ground the same and its lighting, props and textures 2002's,
//! with no radial flora or effect volumes (Frontiers added both). Measured,
//! file by file: `claude-notes/era-2002-art-20261024.md`.
//!
//! Only the art changes: the game is Frontiers' (its code, words, menus and
//! rules), the server's patch is read as it is, and the 2002 disc is read for
//! its containers and loading pictures alone.
//!
//! The disc: natively `$EQOA_2002_ISO`, else the one the player picked
//! (`settings::disc_2002`, asked for by F9 with none); in a page the file the
//! player hands its second input (`set_page_disc`). The European discs (2003)
//! already carry nearly all of Frontiers' art and the 2002 betas lay their
//! files out apart, so only the USA disc is taken.

use esf::{Container, Era, Library};
use std::cell::RefCell;
use std::rc::Rc;

/// The program the USA disc of 2002 starts (SYSTEM.CNF's BOOT2).
pub const USA_2002: &str = "SLUS_204.70";

/// A file only the USA disc has of the two discs that start `USA_2002` (the
/// 2002-09-18 beta starts it too, its files in DATA alone).
const USA_2002_MARK: &str = "DATA2/CHARCUST.CSF";

/// The worlds the 2002 disc has: Tunaria alone (Frontiers added the others).
const WORLDS_2002: &[&str] = &["TUNARIA.ESF"];

/// The art in effect: Frontiers' or the 2002 disc's, as the player last chose
/// it (`settings::art`).
pub fn current() -> Era {
    crate::settings::art()
}

/// Chooses the art and keeps the choice (`settings::set_art`).
pub fn set(era: Era) {
    if let Err(e) = crate::settings::set_art(era) {
        crate::say(&format!("art: the choice was not kept: {e}"));
    }
}

/// The other art from `era`'s: what F9 turns to.
pub fn other(era: Era) -> Era {
    match era {
        Era::Frontiers => Era::Y2002,
        Era::Y2002 => Era::Frontiers,
    }
}

/// A line for the player: which art is drawn now, and what F9 does.
pub fn line(era: Era) -> &'static str {
    match era {
        Era::Y2002 => "Art: the 2002 disc's (F9: Frontiers')",
        Era::Frontiers => "Art: Frontiers' (F9: the 2002 disc's)",
    }
}

/// Whether a disc that starts `boot`, and has `marked` (the USA disc's
/// DATA2/CHARCUST.CSF), is one the 2002 art can come off; else why not, for
/// the player.
pub fn judge(boot: Option<&str>, marked: bool) -> Result<(), String> {
    const WANT: &str = "the 2002 art is on EverQuest Online Adventures (USA), SLUS-20470";
    match boot {
        Some(USA_2002) if marked => Ok(()),
        Some(USA_2002) => Err(format!("This is a 2002 beta of EverQuest Online Adventures, its files laid out apart: {WANT}.")),
        Some(crate::discs::FRONTIERS) => Err(format!("This is the Frontiers disc the game runs off: {WANT}.")),
        Some("SLUS_280.21") => Err(format!("This is a 2002 beta of EverQuest Online Adventures, its files laid out apart: {WANT}.")),
        Some("SCES_516.48" | "SCES_517.25" | "SCED_517.28") => Err(format!("This European disc of 2003 already has nearly all of Frontiers' art: {WANT}.")),
        Some("SLUS_290.63") => Err(format!("This is the Frontiers beta: {WANT}.")),
        Some(b) => Err(format!("This disc starts {}, which is not EverQuest Online Adventures: {WANT}.", b.replace('_', "-").replace('.', ""))),
        None => Err(format!("This image has no SYSTEM.CNF, so it is not a PlayStation 2 disc: {WANT}.")),
    }
}

/// An opened image judged as the 2002 disc (`judge`).
pub fn judge_image<S: iso::Source>(d: &mut iso::Iso<S>) -> Result<(), String> {
    let boot = d.read("SYSTEM.CNF").ok().and_then(|c| crate::discs::boot_program(&c));
    judge(boot.as_deref(), d.find(USA_2002_MARK).is_some())
}

thread_local! {
    /// A page's 2002 disc is in hand (`set_page_disc`).
    static PAGE_DISC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// How a page asks the player for its 2002 disc (`set_asker`).
    static ASKER: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
    /// How a page reads the 2002 disc's files (`set_reader`).
    static READER: RefCell<Option<Reader>> = const { RefCell::new(None) };
}

/// The 2002 disc image natively: `$EQOA_2002_ISO`, else the one the player
/// picked.
#[cfg(not(target_arch = "wasm32"))]
pub fn disc() -> Option<String> {
    std::env::var("EQOA_2002_ISO").ok().filter(|s| !s.is_empty()).or_else(crate::settings::disc_2002)
}

/// Whether a 2002 disc is at hand: natively one named or picked (judged when
/// it is read), in a page one the page was handed.
pub fn available() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    if disc().is_some() {
        return true;
    }
    PAGE_DISC.with(|c| c.get())
}

/// A page has (or no longer has) its 2002 disc.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn set_page_disc(present: bool) {
    PAGE_DISC.with(|c| c.set(present));
}

/// How a page asks the player for its 2002 disc (its second input).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn set_asker(f: Box<dyn Fn()>) {
    ASKER.with(|c| *c.borrow_mut() = Some(f));
}

/// The player asked for the 2002 art with no 2002 disc at hand: natively the
/// desktop's file dialog (the pick judged and kept, `discs::pick_2002`), in a
/// page the page's own ask. Whether a disc is at hand after.
pub fn ask() -> bool {
    if ASKER.with(|c| c.borrow().as_ref().map(|f| f()).is_some()) {
        return available();
    }
    #[cfg(not(target_arch = "wasm32"))]
    match crate::discs::pick_2002() {
        Ok(path) => {
            if let Err(e) = crate::settings::set_disc_2002(&path) {
                crate::say(&format!("art: the 2002 disc's place was not kept: {e}"));
            }
            crate::say(&format!("art: the 2002 disc is {path}"));
        }
        Err(e) => crate::say(&format!("art: {e}")),
    }
    available()
}

/// Whether the 2002 disc has the world `name` (its file's last part).
pub fn has_world(name: &str) -> bool {
    let n = crate::pregame::res::name_of(name);
    WORLDS_2002.iter().any(|w| w.eq_ignore_ascii_case(n))
}

/// The disc the world `name` is read off now: the 2002 disc's while its art
/// is drawn and it has that world (Tunaria), else Frontiers'.
pub fn world_era(name: &str) -> Era {
    if current() == Era::Y2002 && available() && has_world(name) { Era::Y2002 } else { Era::Frontiers }
}

/// The image a file off the disc of `era` is read from natively: the 2002
/// disc's, or the one the game runs off.
#[cfg(not(target_arch = "wasm32"))]
pub fn image_of(era: Era) -> Option<String> {
    match era {
        Era::Y2002 => disc(),
        Era::Frontiers => Some(crate::disc_path(None)),
    }
}

/// Files off the 2002 disc, each by the name it was asked for (any path: it
/// is found by its last part, wherever that disc keeps it, and one the disc
/// lacks is left out) and its bytes; or why the disc could not be read.
pub type Files = Result<Vec<(String, Vec<u8>)>, String>;

/// A page's read of them: filled on a later frame.
pub type Slot = Rc<RefCell<Option<Files>>>;

/// A page's reader: starts reading the files `names` names (as `Files`) and
/// fills the slot it returns.
pub type Reader = Box<dyn Fn(Vec<String>) -> Slot>;

/// Makes `r` the way the 2002 disc's files are read (a page's).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub fn set_reader(r: Reader) {
    READER.with(|c| *c.borrow_mut() = Some(r));
}

/// Files off the 2002 disc being read (`read`).
pub enum Reading {
    Slot(Slot),
    #[cfg(not(target_arch = "wasm32"))]
    Thread(std::sync::mpsc::Receiver<Files>),
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Done(Option<Files>),
}

impl Reading {
    /// The files, once, when they have been read.
    pub fn poll(&mut self) -> Option<Files> {
        match self {
            Reading::Slot(s) => s.borrow_mut().take(),
            #[cfg(not(target_arch = "wasm32"))]
            Reading::Thread(rx) => match rx.try_recv() {
                Ok(a) => Some(a),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("the reading thread stopped".into())),
            },
            Reading::Done(a) => a.take(),
        }
    }
}

/// Starts reading the files `names` names off the 2002 disc (as `Files`):
/// natively on a thread, in a page through its reader.
pub fn read(names: Vec<String>) -> Reading {
    if let Some(s) = READER.with(|c| c.borrow().as_ref().map(|f| f(names.clone()))) {
        return Reading::Slot(s);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(read_files_now(&names));
        });
        Reading::Thread(rx)
    }
    #[cfg(target_arch = "wasm32")]
    Reading::Done(Some(Err("no reader for the 2002 disc".into())))
}

/// The files `names` names read off the 2002 disc now (natively), the disc
/// judged first.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_files_now(names: &[String]) -> Files {
    let path = disc().ok_or("no 2002 disc image (F9 asks for it)")?;
    let mut d = iso::Iso::open(&path).map_err(|e| format!("{path}: {e}"))?;
    judge_image(&mut d)?;
    let mut out = Vec::new();
    for name in names {
        let bare = crate::pregame::res::name_of(name);
        if d.find(bare).is_none() {
            continue;
        }
        out.push((name.clone(), d.read(bare).map_err(|e| format!("{bare}: {e}"))?));
    }
    Ok(out)
}

/// The 2002 disc's containers, each the library's name it stands in for and
/// the container; or why the disc could not be read.
pub type Art = Result<Vec<(String, Container)>, String>;

/// Files opened as containers.
fn open_all(files: Vec<(String, Vec<u8>)>) -> Art {
    files.into_iter().map(|(n, b)| Container::open(b).map(|c| (n.clone(), c)).map_err(|e| format!("{}: {e}", crate::pregame::res::name_of(&n)))).collect()
}

/// The 2002 disc's containers being read (`load`).
pub enum Loading {
    /// a page's: opened as they come, on the game's thread
    Files(Reading),
    /// natively: read and opened on a thread
    #[cfg(not(target_arch = "wasm32"))]
    Thread(std::sync::mpsc::Receiver<Art>),
}

impl Loading {
    /// The containers, once, when they have been read.
    pub fn poll(&mut self) -> Option<Art> {
        match self {
            Loading::Files(r) => r.poll().map(|f| f.and_then(open_all)),
            #[cfg(not(target_arch = "wasm32"))]
            Loading::Thread(rx) => match rx.try_recv() {
                Ok(a) => Some(a),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("the reading thread stopped".into())),
            },
        }
    }
}

/// Starts reading the 2002 disc's copies of the files `names` names (the
/// library's names, `names_of`), as containers: natively read and opened on
/// a thread, in a page read through its reader and opened as they come.
pub fn load(names: Vec<String>) -> Loading {
    if READER.with(|c| c.borrow().is_some()) {
        return Loading::Files(read(names));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(read_now(&names));
        });
        Loading::Thread(rx)
    }
    #[cfg(target_arch = "wasm32")]
    Loading::Files(Reading::Done(Some(Err("no reader for the 2002 disc".into()))))
}

/// The 2002 disc's copies of `names` read and opened now (natively: a world's
/// loader thread reads them with the world's files). A name the disc does not
/// have is left out.
#[cfg(not(target_arch = "wasm32"))]
pub fn read_now(names: &[String]) -> Art {
    read_files_now(names).and_then(open_all)
}

/// The files of `lib` the 2002 disc may have a copy of: every container but
/// a world (Tunaria comes over whole, `viewer`) and its zones, and none that
/// already has one.
pub fn names_of(lib: &Library) -> Vec<String> {
    (0..lib.files.len())
        .filter(|&f| lib.stands_for(f).is_none() && lib.stand_in_of(f).is_none() && lib.era_of(f) == Era::Frontiers)
        .filter(|&f| !lib.files[f].0.contains('#') && lib.file(f).zones().is_empty() && !has_world(&lib.files[f].0))
        .map(|f| lib.files[f].0.clone())
        .collect()
}

/// Art the 2002 view keeps Frontiers': the HUD atlas's sheet (UI.CSF's
/// "sheet1"), whose rects SUPPORT's table names. The 2002 sheet is the same
/// art palette-quantised again (88% of its texels within 8 levels), laid out
/// otherwise where Frontiers added art (the Alchemist's flask, the keyboard's
/// help, the DNAS mark): Frontiers' table over it would cut the wrong art.
pub fn kept() -> Vec<i32> {
    vec![vi::ui::icon_id("sheet1") as i32]
}

/// Each of the 2002 disc's containers in place, standing in for the file of
/// its name: one whose bytes are its namesake's is left out (ITEM.ESF, the
/// arena, the fonts' Arial: the same on both discs), and the art `kept`
/// names stays Frontiers'. How many stand in.
pub fn install(lib: &mut Library, art: Vec<(String, Container)>) -> usize {
    lib.keep_frontiers(&kept());
    let mut n = 0;
    for (name, c) in art {
        let Some(f) = (0..lib.files.len()).find(|&f| lib.stands_for(f).is_none() && lib.files[f].0 == name) else { continue };
        if lib.file(f).data == c.data {
            continue;
        }
        lib.stand_in(f, c);
        n += 1;
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The USA disc is taken; every other EverQuest Online Adventures disc
    /// is refused with why, and so is anything else.
    #[test]
    fn only_the_usa_disc_of_2002() {
        assert_eq!(judge(Some("SLUS_204.70"), true), Ok(()));
        // the control: the same program without the USA disc's DATA2 is the beta
        assert!(judge(Some("SLUS_204.70"), false).unwrap_err().contains("beta"));
        assert!(judge(Some("SLUS_207.44"), true).unwrap_err().contains("Frontiers disc"));
        assert!(judge(Some("SCES_516.48"), true).unwrap_err().contains("European"));
        assert!(judge(Some("SLUS_290.63"), true).unwrap_err().contains("Frontiers beta"));
        assert!(judge(Some("SCUS_971.34"), true).unwrap_err().contains("SCUS-97134"));
        assert!(judge(None, true).unwrap_err().contains("no SYSTEM.CNF"));
        for b in [Some("SLUS_207.44"), Some("SCED_517.28"), None] {
            assert!(judge(b, true).unwrap_err().contains("SLUS-20470"), "{b:?}: says which disc");
        }
    }

    /// The world's files the 2002 disc may stand in for: containers, not the
    /// world and its zones, not one already stood in for.
    #[test]
    fn names_of_a_world_library() {
        let mut lib = Library::new();
        let empty = || {
            let mut d = b"FJBO".to_vec();
            d.resize(24, 0);
            d[16..24].copy_from_slice(&24i64.to_le_bytes());
            Container::parse(d).unwrap()
        };
        lib.add("TUNARIA.ESF", empty());
        lib.add("CHARCUST.CSF", empty());
        lib.add("DATA2/SKY.CSF", empty());
        lib.add("CHAR.ESF", empty());
        assert_eq!(names_of(&lib), ["CHARCUST.CSF", "DATA2/SKY.CSF", "CHAR.ESF"]);
        lib.stand_in(3, empty());
        assert_eq!(names_of(&lib), ["CHARCUST.CSF", "DATA2/SKY.CSF"], "CHAR.ESF has its stand-in");
        assert!(has_world("TUNARIA.ESF") && has_world("tunaria.esf") && !has_world("ODUS.ESF"));
    }

    /// A 2002 file the same as its namesake is not stood in; one that is
    /// other art is.
    #[test]
    fn install_skips_the_unchanged() {
        let record = |id: u32, fill: u8| {
            let mut d = b"FJBO".to_vec();
            d.resize(24, 0);
            d[16..24].copy_from_slice(&24i64.to_le_bytes());
            let mut body = id.to_le_bytes().to_vec();
            body.extend([fill; 16]);
            d.extend_from_slice(&0x1000u16.to_le_bytes());
            d.extend_from_slice(&0i16.to_le_bytes());
            d.extend_from_slice(&(body.len() as i32).to_le_bytes());
            d.extend_from_slice(&0u32.to_le_bytes());
            d.extend_from_slice(&body);
            Container::parse(d).unwrap()
        };
        let mut lib = Library::new();
        lib.add("ITEM.ESF", record(1, 1));
        lib.add("CHAR.ESF", record(2, 2));
        let n = install(&mut lib, vec![("ITEM.ESF".into(), record(1, 1)), ("CHAR.ESF".into(), record(2, 9)), ("NONE.ESF".into(), record(3, 3))]);
        assert_eq!(n, 1);
        assert_eq!((lib.stand_in_of(0), lib.stand_in_of(1)), (None, Some(2)));
    }
}
