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

/// A test's disc, as a boot would have taken it (`crate::disc_path`): the
/// checkout's own, whatever directory the test runs in.
#[cfg(test)]
pub fn take_for_tests(path: &std::path::Path) {
    let _ = TAKEN.set(path.to_string_lossy().into_owned());
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
        let game_mode = game_mode();
        if game_mode {
            crate::say("session: Steam's game mode (gamescope): no desktop dialogs, zenity's instead");
        }
        match start(arg, std::env::var("EQOA_ISO").ok(), crate::settings::disc(), usable) {
            Start::Ready(p) => {
                let _ = TAKEN.set(p.clone());
                Some(p)
            }
            // where a Steam Deck keeps its games, before a dialog
            Start::Pick { note } if game_mode => {
                let home = std::env::var_os("HOME").map(PathBuf::from);
                match find_frontiers(&deck_places(home.as_deref(), Path::new("/run/media"))) {
                    Some(p) => {
                        if let Some(n) = &note {
                            crate::say(n);
                        }
                        crate::say(&format!("disc: {p}, found where a Steam Deck keeps its games"));
                        if let Err(e) = crate::settings::set_disc(&p) {
                            crate::say(&format!("the disc image's place was not kept: {e}"));
                        }
                        let _ = TAKEN.set(p.clone());
                        Some(p)
                    }
                    None => {
                        *ASK.lock().unwrap() = Some(Start::Pick { note });
                        None
                    }
                }
            }
            ask => {
                *ASK.lock().unwrap() = Some(ask);
                None
            }
        }
    }

    /// Steam's game mode: SteamOS's gamescope session, or a game run in
    /// gamescope. It shows the windows of the program Steam started and of
    /// the programs that one starts, never a desktop portal's: the portal's
    /// file dialog (rfd's) is not seen there and the game waits on it as if
    /// hung (a Steam Deck player, 2026-10-25: "works flawlessly from desktop
    /// but seems to hang from steam"), where zenity's, the game's own child,
    /// is seen. A disc chosen in the desktop and kept on a USB drive is not
    /// there in game mode either: it mounts the microSD card, not a drive.
    pub fn game_mode() -> bool {
        cfg!(target_os = "linux")
            && (std::env::var_os("GAMESCOPE_WAYLAND_DISPLAY").is_some() || std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|p| p.eq_ignore_ascii_case("gamescope"))))
    }

    /// Where a Steam Deck keeps a disc image, in the order looked through:
    /// the home folder and its Downloads, Desktop, Documents, Games and ROMs
    /// folders and EmuDeck's `Emulation/roms/ps2`, then the same on each card
    /// or drive under `media` (`/run/media/<card>`, `/run/media/<user>/<card>`).
    /// Only the folders that are there.
    pub(super) fn deck_places(home: Option<&Path>, media: &Path) -> Vec<PathBuf> {
        const SUBS: [&str; 10] = ["", "Downloads", "Desktop", "Documents", "Games", "games", "ROMs", "roms", "Emulation/roms/ps2", "Emulation/roms/PS2"];
        let dirs = |d: &Path| -> Vec<PathBuf> {
            let mut v: Vec<PathBuf> = std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
            v.sort();
            v
        };
        let mut roots: Vec<PathBuf> = home.into_iter().map(Path::to_path_buf).collect();
        for card in dirs(media) {
            let below = dirs(&card);
            roots.push(card);
            roots.extend(below);
        }
        let mut out = Vec::new();
        for r in roots {
            for s in SUBS {
                let p = if s.is_empty() { r.clone() } else { r.join(s) };
                if p.is_dir() && !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        out
    }

    /// The first Frontiers image (`judge_file`'s Take) among the image files
    /// in `places` and in their folders one down (not the hidden ones), by
    /// name; at most 48 images opened, none smaller than 100 MB.
    pub(super) fn find_frontiers(places: &[PathBuf]) -> Option<String> {
        const LOOKS: usize = 48;
        const SMALLEST: u64 = 100 << 20;
        let image = |p: &Path| {
            p.extension().and_then(|e| e.to_str()).is_some_and(|e| ["iso", "bin", "img"].iter().any(|x| e.eq_ignore_ascii_case(x))) && p.metadata().is_ok_and(|m| m.is_file() && m.len() >= SMALLEST)
        };
        let listed = |d: &Path| -> Vec<PathBuf> {
            let mut v: Vec<PathBuf> = std::fs::read_dir(d).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| !p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.'))).collect();
            v.sort();
            v
        };
        let mut looked = 0;
        for place in places {
            let entries = listed(place);
            let mut files: Vec<PathBuf> = entries.iter().filter(|p| image(p)).cloned().collect();
            for d in entries.iter().filter(|p| p.is_dir()) {
                files.extend(listed(d).into_iter().filter(|p| image(p)));
            }
            for f in files {
                if looked == LOOKS {
                    return None;
                }
                looked += 1;
                if let Some(t) = f.to_str()
                    && matches!(judge_file(t), Verdict::Take(_))
                {
                    return Some(t.to_string());
                }
            }
        }
        None
    }

    /// zenity's file dialog, which Steam's game mode shows (`game_mode`):
    /// the image picked, None when cancelled; Err when zenity did not start.
    fn zenity_pick(title: &str, dir: Option<&Path>) -> std::io::Result<Option<PathBuf>> {
        let mut c = std::process::Command::new("zenity");
        c.args(["--file-selection", "--title", title, "--file-filter", "PlayStation 2 disc image | *.iso *.ISO *.bin *.BIN *.img *.IMG", "--file-filter", "All files | *"]);
        if let Some(d) = dir {
            let mut s = d.as_os_str().to_os_string();
            s.push("/");
            c.arg("--filename").arg(s);
        }
        let out = c.output()?;
        // 1 is Cancel; anything else said, with what zenity wrote
        if !out.status.success() {
            if out.status.code() != Some(1) {
                let why = String::from_utf8_lossy(&out.stderr);
                crate::say(&format!("disc: zenity's dialog ended {}: {}", out.status, why.lines().last().unwrap_or("")));
            }
            return Ok(None);
        }
        let path = String::from_utf8_lossy(&out.stdout).trim_end_matches(['\n', '\r']).to_string();
        Ok((!path.is_empty()).then(|| PathBuf::from(path)))
    }

    /// A file dialog for a disc image: zenity's in Steam's game mode
    /// (`game_mode`) while it is there, else the desktop's (rfd's: on Linux
    /// the portal's), its filters the disc images' and all files', opening
    /// in `dir`; the image picked, or None.
    fn file_dialog(title: &str, dir: Option<PathBuf>) -> Option<PathBuf> {
        if game_mode() {
            match zenity_pick(title, dir.as_deref()) {
                Ok(p) => return p,
                Err(e) => crate::say(&format!("disc: no zenity in Steam's game mode ({e}): the desktop's dialog, which it may not show")),
            }
        }
        let mut d = rfd::FileDialog::new().set_title(title);
        // a Mac's open panel takes every filter's types as one list, with no
        // "All files" to switch to: there the check after the pick does it
        if !cfg!(target_os = "macos") {
            d = d.add_filter("PlayStation 2 disc image", &["iso", "ISO", "bin", "BIN", "img", "IMG"]).add_filter("All files", &["*"]);
        }
        if let Some(dir) = dir {
            d = d.set_directory(dir);
        }
        d.pick_file()
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
        // where the kept one was, when its folder is still there
        let dir = crate::settings::disc().and_then(|k| Path::new(&k).parent().map(Path::to_path_buf)).filter(|d| d.is_dir());
        file_dialog(&title, dir)
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
            let dir = crate::settings::disc_2002().or_else(crate::settings::disc).and_then(|k| Path::new(&k).parent().map(Path::to_path_buf)).filter(|d| d.is_dir());
            let path = file_dialog(&title, dir).ok_or("no 2002 disc image was chosen")?;
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

    /// Steam's game mode finds the disc where a Steam Deck keeps its games:
    /// EmuDeck's `Emulation/roms/ps2` on the microSD card (under /run/media,
    /// one folder or two down), its name any case, passing over what is not
    /// Frontiers (a file that is not an image, however large; one too small
    /// to be a disc). The controls: the same image in a folder not looked
    /// through, and in a hidden folder, is not found; with no places, none.
    #[cfg(unix)]
    #[test]
    fn game_mode_finds_the_disc_where_a_deck_keeps_it() {
        let iso = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../run/eqoa.iso"));
        if !iso.exists() {
            eprintln!("SKIP: no run/eqoa.iso");
            return;
        }
        let iso = std::fs::canonicalize(&iso).unwrap();
        let root = std::env::temp_dir().join(format!("eqoa-deck-places-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home/deck");
        let media = root.join("run/media");
        let card = media.join("deck/SD card");
        for d in [home.join("Downloads"), home.join(".hidden"), home.join("elsewhere/deeper/still"), card.join("Emulation/roms/ps2")] {
            std::fs::create_dir_all(&d).unwrap();
        }
        // a large file that is not an image (sparse), and a small "image"
        let big = std::fs::File::create(home.join("Downloads/big.iso")).unwrap();
        big.set_len(200 << 20).unwrap();
        std::fs::write(home.join("Downloads/tiny.ISO"), b"not a disc").unwrap();
        let places = native::deck_places(Some(&home), &media);
        assert!(places.contains(&home.join("Downloads")) && places.contains(&card.join("Emulation/roms/ps2")), "{places:?}");
        assert_eq!(native::find_frontiers(&places), None, "nothing that is Frontiers yet");
        std::os::unix::fs::symlink(&iso, card.join("Emulation/roms/ps2/EverQuest Online Adventures - Frontiers (USA).ISO")).unwrap();
        let found = native::find_frontiers(&native::deck_places(Some(&home), &media));
        assert_eq!(found.as_deref(), card.join("Emulation/roms/ps2/EverQuest Online Adventures - Frontiers (USA).ISO").to_str());
        // the controls: hidden, or deeper than a place's folders, is not looked at
        std::fs::remove_file(card.join("Emulation/roms/ps2/EverQuest Online Adventures - Frontiers (USA).ISO")).unwrap();
        std::os::unix::fs::symlink(&iso, home.join(".hidden/eqoa.iso")).unwrap();
        std::os::unix::fs::symlink(&iso, home.join("elsewhere/deeper/still/eqoa.iso")).unwrap();
        assert_eq!(native::find_frontiers(&native::deck_places(Some(&home), &media)), None);
        assert_eq!(native::find_frontiers(&[]), None);
        let _ = std::fs::remove_dir_all(&root);
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
