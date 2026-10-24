//! Which disc image the client reads, and the player's pick of it.
//!
//! A disc is known by the program it starts, as its SYSTEM.CNF names it
//! (`boot_program`, `describe`): a page says so as its image is picked, and
//! natively the same check stands between the file dialog and the game.
//!
//! Natively the game from its boot (`eqoa`, `eqoa boot`, `eqoa <image>`)
//! takes the image named on the command line, else `$EQOA_ISO`, else the one
//! kept from the last pick (`settings::disc`: `disc.opt` in the config
//! directory), else the checkout's `run/eqoa.iso` (`start`). With none of
//! them the player picks it in the desktop's own file dialog, and it is kept
//! (`settle`). The dialogs run inside the window's event loop, before the
//! window is made (`viewer`'s `resumed`), where every platform's toolkit
//! expects a modal dialog; before the loop, a Mac would show it with no
//! application running.

/// The disc this client is made for, by the program it starts.
pub const FRONTIERS: &str = "SLUS_207.44";

/// The other EverQuest Online Adventures discs, by the program each starts,
/// and what is missing with it: measured, each disc through the page
/// (2026-10-20). The first and the 2002-09-18 beta start the same program.
const OTHER_DISCS: &[(&str, &str, &str)] = &[
    ("SLUS_204.70", "EverQuest Online Adventures (USA)", "the client cannot enter the world"),
    ("SLUS_280.21", "an EverQuest Online Adventures beta (USA)", "the client cannot enter the world"),
    ("SCES_516.48", "EverQuest Online Adventures (Europe)", "there is no HUD and no start menu"),
    ("SCES_517.25", "EverQuest Online Adventures (Europe, En/Fr/De)", "there is no HUD and no start menu"),
    ("SCED_517.28", "the EverQuest Online Adventures demo (Europe)", "there is no HUD and no start menu"),
    ("SLUS_290.63", "the Frontiers beta (USA)", "there is no HUD and no start menu"),
];

/// The image a native run reads when no command names one: `run/eqoa.iso`,
/// the checkout's link to the disc.
pub const CHECKOUT: &str = "run/eqoa.iso";

/// The program a disc starts, out of its SYSTEM.CNF (`BOOT2 =
/// cdrom0:\SLUS_207.44;1` is `SLUS_207.44`): which disc this is.
pub fn boot_program(cnf: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(cnf);
    let line = text.lines().find(|l| l.trim_start().to_ascii_uppercase().starts_with("BOOT2"))?;
    let file = line.rsplit(['\\', '/', ':']).next()?.trim();
    Some(file.split(';').next()?.trim().to_string()).filter(|f| !f.is_empty())
}

/// Whether a disc that starts `boot` is the one this client is made for,
/// and a line for the player saying which disc it is.
pub fn describe(boot: Option<&str>) -> (bool, String) {
    const WANT: &str = "The client is made for EverQuest Online Adventures: Frontiers (USA), SLUS-20744";
    // SLUS_207.44 as the disc's label prints it, SLUS-20744
    let label = |b: &str| b.replace('_', "-").replace('.', "");
    match boot {
        Some(FRONTIERS) => (true, "EverQuest Online Adventures: Frontiers (USA), SLUS-20744.".into()),
        Some(b) => match OTHER_DISCS.iter().find(|d| d.0.eq_ignore_ascii_case(b)) {
            Some((_, name, missing)) => (false, format!("This is {name}, {}. {WANT}: with this disc {missing}.", label(b))),
            None => (false, format!("This disc starts {}, which is not Frontiers. {WANT}.", label(b))),
        },
        None => (false, format!("This image has no SYSTEM.CNF, so it is not a PlayStation 2 disc. {WANT}.")),
    }
}

/// The image a boot took (`boot_disc`, `settle`): `disc_path` answers it
/// from then on.
static TAKEN: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// The image this run took, if a boot took one.
pub fn taken() -> Option<String> {
    TAKEN.get().cloned()
}

/// Where a boot's disc comes from, before any dialog.
#[derive(Debug, PartialEq)]
pub enum Start {
    /// this image as it is: `$EQOA_ISO`'s, the kept one, the checkout's
    Ready(String),
    /// this image, named on the command line: checked as a pick is, and kept
    Check(String),
    /// none: the player picks one, told why when the kept one cannot be used
    Pick { note: Option<String> },
}

/// The order a boot takes its disc in: `arg` (the command line's), `env`
/// (`$EQOA_ISO`), `kept` (the last pick) when `usable` says it opens, then
/// `CHECKOUT` when it opens; else the player picks one.
pub fn start(arg: Option<&str>, env: Option<String>, kept: Option<String>, usable: impl Fn(&str) -> Result<(), String>) -> Start {
    if let Some(a) = arg {
        return Start::Check(a.to_string());
    }
    if let Some(e) = env {
        return Start::Ready(e);
    }
    let mut note = None;
    if let Some(k) = kept {
        match usable(&k) {
            Ok(()) => return Start::Ready(k),
            Err(e) => note = Some(format!("The disc image chosen last time cannot be used: {e}.")),
        }
    }
    if usable(CHECKOUT).is_ok() {
        return Start::Ready(CHECKOUT.into());
    }
    Start::Pick { note }
}

/// What a picked image is: Frontiers (`Take`), another PlayStation 2 disc,
/// which the player may still play with (`Ask`, the page's warning), or no
/// disc at all (`Refuse`); each with the line saying so.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Take(String),
    Ask(String),
    Refuse(String),
}

/// The verdict on an image at `path` whose SYSTEM.CNF is `cnf` (None: the
/// image has none), or which did not open (`Err`, the reason).
pub fn judge(path: &str, cnf: Result<Option<Vec<u8>>, String>) -> Verdict {
    let cnf = match cnf {
        Ok(c) => c,
        Err(e) => return Verdict::Refuse(format!("{path} is not a disc image the client can read: {e}.")),
    };
    let boot = cnf.as_deref().and_then(boot_program);
    let (ok, line) = describe(boot.as_deref());
    match (ok, boot) {
        (true, _) => Verdict::Take(line),
        (false, Some(_)) => Verdict::Ask(line),
        (false, None) => Verdict::Refuse(line),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{boot_disc, pick_2002, settle, unsettled};

/// A page's disc is the image the page hands over (`web`): none to find.
#[cfg(target_arch = "wasm32")]
pub fn boot_disc(_arg: Option<&str>) -> Option<String> {
    None
}

/// The native half: the start's resolution, the dialogs and the keeping.
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    /// What a boot still has to ask the player (`boot_disc`, `settle`).
    static ASK: Mutex<Option<Start>> = Mutex::new(None);

    /// The dialogs' title.
    const TITLE: &str = "EverQuest Online Adventures";
    /// The file dialog's question.
    const CHOOSE: &str = "Choose your EverQuest Online Adventures: Frontiers disc image (SLUS-20744)";

    /// Whether `path` is an image that opens: its ISO 9660 directory read.
    fn usable(path: &str) -> Result<(), String> {
        match iso::Iso::open(path) {
            Ok(_) => Ok(()),
            Err(iso::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => Err(format!("{path} is not there any more")),
            Err(e) => Err(format!("{path}: {}", reason(&e))),
        }
    }

    /// An `iso` error for a player: what the file is instead.
    fn reason(e: &iso::Error) -> String {
        match e {
            iso::Error::NotIso => "it is not an ISO 9660 image (an archive, .zip, .7z or .rar, is unpacked first)".into(),
            iso::Error::BlockSize(n) => format!("its sectors are {n} bytes, not a DVD image's 2,048"),
            e => e.to_string(),
        }
    }

    /// The image at `path`, judged: opened, and its SYSTEM.CNF read.
    pub(super) fn judge_file(path: &str) -> Verdict {
        let cnf = iso::Iso::open(path).map_err(|e| reason(&e)).map(|mut d| d.read("SYSTEM.CNF").ok());
        judge(path, cnf)
    }

    /// The boot's disc as far as it can be had without asking: the image
    /// when one is at hand (taken), else None, the question kept for
    /// `settle`.
    pub fn boot_disc(arg: Option<&str>) -> Option<String> {
        match start(arg, std::env::var("EQOA_ISO").ok(), crate::settings::disc(), usable) {
            Start::Ready(p) => {
                let _ = TAKEN.set(p.clone());
                Some(p)
            }
            ask => {
                *ASK.lock().unwrap() = Some(ask);
                None
            }
        }
    }

    /// Whether a boot still has to ask for its disc (`settle`).
    pub fn unsettled() -> bool {
        ASK.lock().unwrap().is_some()
    }

    /// The boot's question answered, before the window opens: the image the
    /// command line named, or else the player's pick in the desktop's file
    /// dialog, judged (`judge`) until it is one to play with; kept for the
    /// next time and taken. Err when the player cancels the dialog, when no
    /// dialog could open, or when one is needed with no screen to show it on
    /// (`EQOA_OFFSCREEN`).
    pub fn settle() -> Result<String, String> {
        const NONE: &str = "no disc image was chosen. The client reads your own EverQuest Online Adventures: Frontiers disc image (SLUS-20744): start it again to choose it, or name it, eqoa /path/to/image.iso";
        let Some(ask) = ASK.lock().unwrap().take() else {
            return taken().ok_or_else(|| NONE.to_string());
        };
        let (mut named, mut note) = match ask {
            Start::Ready(p) => return Ok(p),
            Start::Check(p) => (Some(p), None),
            Start::Pick { note } => (None, note),
        };
        // `EQOA_OFFSCREEN=WxH` (as `viewer` reads it): no screen to show a
        // dialog on, so what would need one ends the run with its line instead
        let offscreen = std::env::var("EQOA_OFFSCREEN").ok().and_then(|v| v.split_once('x').map(|(w, h)| w.parse::<u32>().is_ok() && h.parse::<u32>().is_ok()));
        let screen = offscreen != Some(true);
        let unseen = |line: &str| format!("{line} No screen to ask on (EQOA_OFFSCREEN): set EQOA_ISO.");
        if let Some(n) = &note {
            crate::say(n);
        }
        loop {
            let path = match named.take() {
                Some(p) => PathBuf::from(p),
                None if !screen => return Err(unseen(note.as_deref().unwrap_or("No disc image is at hand."))),
                None => pick(note.as_deref()).ok_or(NONE)?,
            };
            // kept whole, so it opens from wherever the next run starts
            let path = std::path::absolute(&path).unwrap_or(path);
            let text = path.to_str().map(str::to_string);
            let verdict = match &text {
                Some(t) => judge_file(t),
                None => Verdict::Refuse(format!("The client cannot open {} (its name is not Unicode): rename it or move it.", path.display())),
            };
            match verdict {
                Verdict::Take(line) => crate::say(&format!("disc: {line}")),
                Verdict::Ask(line) | Verdict::Refuse(line) if !screen => return Err(unseen(&line)),
                Verdict::Ask(line) => {
                    if !confirm(&format!("{line}\n\nPlay with it anyway?")) {
                        note = Some(line);
                        continue;
                    }
                }
                Verdict::Refuse(line) => {
                    tell(&line);
                    note = Some(line);
                    continue;
                }
            }
            // (a name that is not Unicode was refused above)
            let Some(text) = text else { continue };
            if let Err(e) = crate::settings::set_disc(&text) {
                crate::say(&format!("the disc image's place was not kept: {e}"));
            }
            let _ = TAKEN.set(text.clone());
            return Ok(text);
        }
    }

    /// The desktop's file dialog: the player's image, or None (cancelled, or
    /// no dialog could open). `note` says why it asks again.
    fn pick(note: Option<&str>) -> Option<PathBuf> {
        let title = match note {
            Some(n) => format!("{n} {CHOOSE}"),
            None => CHOOSE.to_string(),
        };
        let mut d = rfd::FileDialog::new().set_title(&title);
        // a Mac's open panel takes every filter's types as one list, with no
        // "All files" to switch to: there the check after the pick does it
        if !cfg!(target_os = "macos") {
            d = d.add_filter("PlayStation 2 disc image", &["iso", "ISO", "bin", "BIN", "img", "IMG"]).add_filter("All files", &["*"]);
        }
        // where the kept one was, when its folder is still there
        if let Some(dir) = crate::settings::disc().and_then(|k| Path::new(&k).parent().map(Path::to_path_buf)).filter(|d| d.is_dir()) {
            d = d.set_directory(dir);
        }
        d.pick_file()
    }

    /// The 2002 disc for F9's art (`era`), in the desktop's file dialog,
    /// judged (`era::judge_image`) until it is the USA disc of 2002: its path,
    /// or why there is none (cancelled, no dialog, no screen).
    pub fn pick_2002() -> Result<String, String> {
        const CHOOSE_2002: &str = "Choose your EverQuest Online Adventures (USA) disc image (SLUS-20470), for its 2002 art";
        let offscreen = std::env::var("EQOA_OFFSCREEN").ok().and_then(|v| v.split_once('x').map(|(w, h)| w.parse::<u32>().is_ok() && h.parse::<u32>().is_ok()));
        if offscreen == Some(true) {
            return Err("no 2002 disc image, and no screen to ask on (EQOA_OFFSCREEN): set EQOA_2002_ISO".into());
        }
        let mut note: Option<String> = None;
        loop {
            let title = match &note {
                Some(n) => format!("{n} {CHOOSE_2002}"),
                None => CHOOSE_2002.to_string(),
            };
            let mut d = rfd::FileDialog::new().set_title(&title);
            if !cfg!(target_os = "macos") {
                d = d.add_filter("PlayStation 2 disc image", &["iso", "ISO", "bin", "BIN", "img", "IMG"]).add_filter("All files", &["*"]);
            }
            if let Some(dir) = crate::settings::disc_2002().or_else(crate::settings::disc).and_then(|k| Path::new(&k).parent().map(Path::to_path_buf)).filter(|d| d.is_dir()) {
                d = d.set_directory(dir);
            }
            let path = d.pick_file().ok_or("no 2002 disc image was chosen")?;
            let path = std::path::absolute(&path).unwrap_or(path);
            let Some(text) = path.to_str().map(str::to_string) else {
                let line = format!("The client cannot open {} (its name is not Unicode): rename it or move it.", path.display());
                tell(&line);
                note = Some(line);
                continue;
            };
            let verdict = iso::Iso::open(&text).map_err(|e| format!("{text}: {}", reason(&e))).and_then(|mut i| crate::era::judge_image(&mut i));
            match verdict {
                Ok(()) => return Ok(text),
                Err(line) => {
                    tell(&line);
                    note = Some(line);
                }
            }
        }
    }

    /// Why a pick was refused: said, and shown in a box the player closes
    /// (on Linux zenity's, when it is there).
    fn tell(line: &str) {
        crate::say(line);
        let _ = rfd::MessageDialog::new().set_level(rfd::MessageLevel::Error).set_title(TITLE).set_description(line).set_buttons(rfd::MessageButtons::Ok).show();
    }

    /// A question with Yes and No: true for Yes. No box (Linux without
    /// zenity) is a No.
    fn confirm(text: &str) -> bool {
        crate::say(text);
        matches!(
            rfd::MessageDialog::new().set_level(rfd::MessageLevel::Warning).set_title(TITLE).set_description(text).set_buttons(rfd::MessageButtons::YesNo).show(),
            rfd::MessageDialogResult::Yes
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CNF: &[u8] = b"BOOT2 = cdrom0:\\SLUS_207.44;1\r\nVER = 1.00\r\nVMODE = NTSC\r\n";

    /// SYSTEM.CNF's BOOT2 line names the program, however it is spaced or
    /// cased; a CNF without one names none.
    #[test]
    fn the_program_out_of_system_cnf() {
        assert_eq!(boot_program(CNF).as_deref(), Some("SLUS_207.44"));
        assert_eq!(boot_program(b"boot2=cdrom0:\\SCES_516.48;1\n").as_deref(), Some("SCES_516.48"));
        assert_eq!(boot_program(b"  BOOT2 = cdrom0:\\DATA\\SLUS_204.70;1").as_deref(), Some("SLUS_204.70"));
        assert_eq!(boot_program(b"VMODE = NTSC\n"), None, "no BOOT2 line");
        assert_eq!(boot_program(b"BOOT2 = cdrom0:\\;1\n"), None, "a BOOT2 naming nothing");
    }

    /// Frontiers is taken, another PlayStation 2 disc asked about (as the
    /// page warns), an image with no SYSTEM.CNF or one that did not open
    /// refused; each with its line.
    #[test]
    fn a_picked_image_is_judged_by_its_program() {
        assert!(matches!(judge("a.iso", Ok(Some(CNF.to_vec()))), Verdict::Take(l) if l.contains("SLUS-20744")));
        match judge("eu.iso", Ok(Some(b"BOOT2 = cdrom0:\\SCES_516.48;1\n".to_vec()))) {
            Verdict::Ask(l) => assert!(l.contains("Europe") && l.contains("no HUD"), "{l}"),
            v => panic!("the European disc: {v:?}"),
        }
        match judge("other.iso", Ok(Some(b"BOOT2 = cdrom0:\\SCUS_971.34;1\n".to_vec()))) {
            Verdict::Ask(l) => assert!(l.contains("SCUS-97134") && l.contains("not Frontiers"), "{l}"),
            v => panic!("another game: {v:?}"),
        }
        assert!(matches!(judge("data.iso", Ok(None)), Verdict::Refuse(l) if l.contains("no SYSTEM.CNF")));
        assert!(matches!(judge("game.zip", Err("it is not an ISO 9660 image".into())), Verdict::Refuse(l) if l.contains("game.zip") && l.contains("ISO 9660")));
    }

    /// The command line's image first, then `$EQOA_ISO`, the kept pick and
    /// the checkout's link; a kept image that cannot be used is passed over
    /// with a note for the dialog, and with none the player is asked.
    #[test]
    fn a_boot_takes_its_disc_in_order() {
        let all = |_: &str| Ok(());
        let none = |p: &str| Err(format!("{p} is not there any more"));
        let kept = || Some("/games/eqoa frontiers.iso".to_string());
        assert_eq!(start(Some("x.iso"), Some("env.iso".into()), kept(), all), Start::Check("x.iso".into()));
        assert_eq!(start(None, Some("env.iso".into()), kept(), all), Start::Ready("env.iso".into()));
        assert_eq!(start(None, None, kept(), all), Start::Ready("/games/eqoa frontiers.iso".into()));
        assert_eq!(start(None, None, None, all), Start::Ready(CHECKOUT.into()));
        assert_eq!(start(None, None, None, none), Start::Pick { note: None });
        // the kept one gone: the checkout's link when it is there, else a pick saying why
        let only_checkout = |p: &str| if p == CHECKOUT { Ok(()) } else { Err(format!("{p} is not there any more")) };
        assert_eq!(start(None, None, kept(), only_checkout), Start::Ready(CHECKOUT.into()));
        match start(None, None, kept(), none) {
            Start::Pick { note: Some(n) } => assert!(n.contains("/games/eqoa frontiers.iso") && n.contains("not there"), "{n}"),
            s => panic!("{s:?}"),
        }
    }

    /// The disc itself, through the native path the dialog's pick takes:
    /// Frontiers is taken (needs `run/eqoa.iso`); a file that is not an
    /// image, or is not there, is refused.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn the_disc_judged_off_the_file() {
        let not = concat!(env!("CARGO_MANIFEST_DIR"), "/src/discs.rs");
        assert!(matches!(native::judge_file(not), Verdict::Refuse(l) if l.contains("not an ISO 9660 image")), "a source file is no disc");
        assert!(matches!(native::judge_file("/nonexistent/eqoa.iso"), Verdict::Refuse(_)));
        let iso = concat!(env!("CARGO_MANIFEST_DIR"), "/../../run/eqoa.iso");
        if !std::path::Path::new(iso).exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        assert!(matches!(native::judge_file(iso), Verdict::Take(_)));
    }
}
