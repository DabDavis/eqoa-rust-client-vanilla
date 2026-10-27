//! The updater: a build CI made follows the one rolling `dev` release on
//! GitHub (`.github/workflows/dev.yml`) and puts a newer build there in its
//! own place when the game starts, before the title.
//!
//! CI bakes the channel and the commit in (`EQOA_UPDATE_CHANNEL`,
//! `EQOA_BUILD_COMMIT`); a build made anywhere else has neither and never
//! updates itself. The game as players start it (`eqoa`, `eqoa boot`, `eqoa
//! <image>`; not a scripted command, nor with `EQOA_OFFSCREEN` or
//! `EQOA_NO_UPDATE` set) starts a thread (`start`) that reads the release's
//! `update.txt`: the commit it was built from and, a line a platform, the
//! program's file, its SHA-256 and its size. Another commit's program is
//! downloaded beside this one (`<exe>.new`), held to that size and hash, and
//! put in this one's place by two renames, the running program aside to
//! `<exe>.old` first: a running program is never written to, which Windows
//! refuses, and a failure between the two puts the old one back. The boot's
//! pictures show meanwhile; the boot waits for the thread before the title
//! (`settled`, at most `WAIT`, a line on the picture while it downloads), and
//! with the new program in place it closes the window (`staged`), after which
//! `relaunch_if_requested` starts the new program with the same arguments. The
//! next start removes `<exe>.old`. A failure anywhere leaves the program as it
//! was, says so in the log, and the game goes on.
//!
//! While the game runs the thread goes on asking (`watch`): `FIRST` after the
//! start, then every `EVERY`, the same `update.txt` (the release's, never the
//! game server's, so a player hears of a build on any server). Another
//! commit's program is put in place as at the start, or found there already
//! (another window of the game put it there, or the start's own download came
//! after the title); the program the update started (`EQOA_UPDATED`) watches
//! too, without the start's check. Then the game asks the player, in its own
//! Yes/No box (`Offer`, `VIWndMBox`'s; it starts on No), whether to restart
//! now: it comes up only while nothing else is on the screen and nothing is
//! being typed (`Play::quiet`, `Pregame::quiet`), and takes only its own keys
//! (Enter, Escape, left and right; the pad's ✕, △ and the d-pad's left and
//! right) and clicks on its buttons, so a key meant for the game is never
//! taken; a window or an edit box of the game's put up meanwhile puts it
//! away until it can come back. Yes logs out as the start menu's Quit does
//! (0x09B0), the window closes, and `relaunch_if_requested` starts the
//! program in place; No, or the box left `BOX_MS`, asks again after `LATER`.
//! Each window of several asks its own player. A page asks its player itself
//! (`web/main.js`: `version.json`, Reload / Later).
//!
//! Testing: `EQOA_UPDATE_URL=<base>` reads `<base>update.txt` and the files
//! beside it instead of the release (a server of one's own, `python3 -m
//! http.server`), offscreen too, and a build given `EQOA_UPDATE_CHANNEL=dev
//! EQOA_BUILD_COMMIT=<commit>` follows it; `EQOA_UPDATE_EVERY=<s>` asks
//! the release again after `s` seconds and every `s` after, and again after
//! a No three times that.

use std::time::Duration;

/// The channel CI built this for (`dev`); none for any other build.
pub const CHANNEL: Option<&str> = option_env!("EQOA_UPDATE_CHANNEL");
/// The commit CI built this from (its `GITHUB_SHA`).
pub const COMMIT: Option<&str> = option_env!("EQOA_BUILD_COMMIT");

/// This platform's name in `update.txt`: the programs `dev.yml` builds.
pub const PLATFORM: Option<&str> = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    Some("linux-x86_64")
} else if cfg!(all(windows, target_arch = "x86_64")) {
    Some("windows-x86_64")
} else if cfg!(target_os = "macos") {
    Some("macos")
} else {
    None
};

/// How long the boot waits for the updater before the title, from its
/// start: a check takes a second or two and a download a few more on most
/// lines; past this the game goes on, and a download still coming is put in
/// place for the next start.
pub const WAIT: Duration = Duration::from_secs(30);

/// How long after the start the game asks the release again (`watch`),
/// then how often; after a No it asks the player again no sooner than
/// `LATER`.
pub const FIRST: Duration = Duration::from_secs(3 * 60);
pub const EVERY: Duration = Duration::from_secs(10 * 60);
pub const LATER: Duration = Duration::from_secs(30 * 60);

/// How long the question stands with nothing done in it before it counts
/// as a Later (ms; each input puts it off, and the last ten seconds are
/// counted down by its choices).
pub const BOX_MS: i64 = 180_000;

/// `FIRST`, `EVERY` and `LATER`, or a test's (`EQOA_UPDATE_EVERY`'s
/// seconds, the last three times them).
pub fn intervals(test: Option<&str>) -> (Duration, Duration, Duration) {
    match test.and_then(|s| s.trim().parse::<u64>().ok()).filter(|&s| s > 0) {
        Some(s) => (Duration::from_secs(s), Duration::from_secs(s), Duration::from_secs(3 * s)),
        None => (FIRST, EVERY, LATER),
    }
}

/// One platform's program in `update.txt`.
#[derive(Clone, Debug, PartialEq)]
pub struct Build {
    pub platform: String,
    pub file: String,
    pub sha256: String,
    pub size: u64,
}

/// `update.txt`, as `dev.yml` writes it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Manifest {
    pub version: String,
    pub commit: String,
    pub builds: Vec<Build>,
}

impl Manifest {
    /// Its lines: `version <name>`, `commit <sha>` and `build <platform>
    /// <file> <sha256> <size>`; any other line, or a build line that does not
    /// read, is passed over.
    pub fn parse(s: &str) -> Manifest {
        let mut m = Manifest::default();
        for line in s.lines() {
            let w: Vec<&str> = line.split_whitespace().collect();
            match w.as_slice() {
                ["version", v] => m.version = v.to_string(),
                ["commit", c] => m.commit = c.to_string(),
                ["build", platform, file, sha, size] if sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()) => {
                    if let Ok(size) = size.parse() {
                        m.builds.push(Build { platform: platform.to_string(), file: file.to_string(), sha256: sha.to_ascii_lowercase(), size });
                    }
                }
                _ => {}
            }
        }
        m
    }

    pub fn build(&self, platform: &str) -> Option<&Build> {
        self.builds.iter().find(|b| b.platform == platform)
    }
}

/// Where the updater stands.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// asking the release
    Checking,
    /// the release's program coming: the bytes so far, of
    Downloading { version: String, done: u64, total: u64 },
    /// nothing to do, or it failed (said in the log): the game goes on
    Done,
    /// the release's program (`version`) is in place: the boot closes the
    /// window for `relaunch_if_requested`
    Staged { version: String },
}

impl State {
    /// Whether the boot may go on to the title, `waited` after the updater
    /// started.
    pub fn settled(&self, waited: Duration) -> bool {
        !matches!(self, State::Checking | State::Downloading { .. }) || waited >= WAIT
    }

    /// The line the boot's picture carries.
    pub fn line(&self) -> Option<String> {
        match self {
            State::Downloading { version, done, total } => Some(format!("Downloading the update ({version}): {}%", done * 100 / (*total).max(1))),
            State::Staged { version } => Some(format!("Starting the update ({version})")),
            _ => None,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::{relaunch_if_requested, start};

/// The boot found the release's program in place and closes the window:
/// `relaunch_if_requested` starts it after (natively; a page updates itself).
pub fn request_relaunch() {
    #[cfg(not(target_arch = "wasm32"))]
    native::request_relaunch();
}

/// Whether the boot may go on to the title: no updater this run, it is
/// done, a program is in place, or it has had `WAIT`.
pub fn settled() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return native::with(|s, waited| s.settled(waited)).unwrap_or(true);
    #[cfg(target_arch = "wasm32")]
    true
}

/// A new program is in place: the boot closes the window, to start it.
pub fn staged() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return native::with(|s, _| matches!(s, State::Staged { .. })).unwrap_or(false);
    #[cfg(target_arch = "wasm32")]
    false
}

/// The line the boot's picture carries while the update downloads.
pub fn line() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    return native::with(|s, _| s.line()).flatten();
    #[cfg(target_arch = "wasm32")]
    None
}

/// The updater as a report says it: this build, and what the last look at
/// the release found (put in place, the release's already, or why not), so a
/// player still on an old build after many starts says why (F3 crash
/// 20261026-124849 came from dev-ffb50c3 a day and twenty builds later).
#[cfg(not(target_arch = "wasm32"))]
pub fn describe() -> String {
    let this = COMMIT.map_or_else(|| "local".to_string(), |c| format!("dev-{}", &c[..c.len().min(7)]));
    format!("update: this is {this}; {}", native::found().unwrap_or_else(|| native::off().to_string()))
}

/// The release's version of a program in place other than this running
/// one (`watch`): the game offers to restart into it.
pub fn ready() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    return native::ready();
    #[cfg(target_arch = "wasm32")]
    None
}

// ---- what is new -------------------------------------------------------------

/// The release's copy of the update notes, beside `update.txt`: dev.yml (and
/// notes.yml, for an edit to the notes alone) uploads UPDATE-NOTES.md under
/// this name before `update.txt`, so a game that reads the new manifest
/// finds the notes that go with it.
pub const NOTES_FILE: &str = "update-notes.md";

/// This build's own update notes, as they were when it was built: the
/// sections the release's have and these lack are what is new.
#[cfg(not(target_arch = "wasm32"))]
const OWN_NOTES: &str = include_str!("../../../UPDATE-NOTES.md");

/// A section of the update notes: `## <date> · <title>` and its bullets (a
/// bullet's wrapped lines joined), as plain text (`plain`).
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub title: String,
    pub bullets: Vec<String>,
}

/// What the question says is new in the build it offers.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum WhatsNew {
    /// the release's sections this build's notes lack, newest first: one a
    /// build the player did not have
    Sections(Vec<Section>),
    /// the release's notes read, and nothing in them this build lacks
    Nothing,
    /// the release's notes could not be read
    #[default]
    Unread,
}

/// Markdown made plain: `**` and backticks out.
pub fn plain(s: &str) -> String {
    s.replace("**", "").replace('`', "")
}

/// An UPDATE-NOTES.md's sections, newest first as the file has them: a
/// `## ` heading starts one; `- ` starts a bullet and a line indented under
/// it carries it on; any other line is a paragraph of its own; what comes
/// before the first heading is the file's preface.
pub fn sections(md: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut open = false;
    for line in md.lines() {
        let t = line.trim_end();
        if let Some(h) = t.strip_prefix("## ") {
            out.push(Section { title: plain(h.trim()), bullets: Vec::new() });
            open = false;
            continue;
        }
        let Some(s) = out.last_mut() else { continue };
        if t.trim().is_empty() {
            open = false;
        } else if let Some(b) = t.strip_prefix("- ") {
            s.bullets.push(plain(b.trim()));
            open = true;
        } else if open && t.starts_with(' ') {
            if let Some(last) = s.bullets.last_mut() {
                last.push(' ');
                last.push_str(&plain(t.trim()));
            }
        } else {
            s.bullets.push(plain(t.trim()));
            open = true;
        }
    }
    out
}

/// The release's sections whose heading this build's notes do not have, in
/// the release's order (newest first).
pub fn whats_new(release: &str, own: &str) -> WhatsNew {
    let had: std::collections::HashSet<String> = sections(own).into_iter().map(|s| s.title).collect();
    let new: Vec<Section> = sections(release).into_iter().filter(|s| !had.contains(&s.title)).collect();
    if new.is_empty() { WhatsNew::Nothing } else { WhatsNew::Sections(new) }
}

/// What is new in the program in place (`ready`), looked for with it.
pub fn ready_notes() -> Option<WhatsNew> {
    #[cfg(not(target_arch = "wasm32"))]
    return native::notes();
    #[cfg(target_arch = "wasm32")]
    None
}

// ---- the question ----------------------------------------------------------

/// The game's moment, as the question reads it each frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct Moment {
    /// nothing else on the screen, nothing being typed, no fight, the
    /// controls let go: the question may come up (once it has been so for
    /// `QUIET_MS`)
    pub quiet: bool,
    /// nothing else on the screen and nothing being typed: it may stay up
    pub plain: bool,
    /// in the world
    pub world: bool,
    /// the clock the window opens and times out on (ms)
    pub now: i64,
}

/// How long the moment stays quiet before the question comes up (ms): a
/// player who has just let go of the controls may be about to take them up
/// again.
pub const QUIET_MS: i64 = 2_000;

#[cfg(not(target_arch = "wasm32"))]
/// The question once a newer program is in place (`ready`), with what is
/// new in it (`updatewnd::NotesWnd`, starting on Later): up only once the
/// moment has been quiet `QUIET_MS`, taking only its own input.
pub struct Offer {
    wnd: crate::updatewnd::NotesWnd,
    shown: bool,
    quiet_since: Option<i64>,
    /// a Later, or the window left `BOX_MS`: not up again before this (ms)
    later: Option<i64>,
    later_ms: i64,
    /// when the window, left alone, counts as a Later (each input puts it off)
    timeout: i64,
    /// Restart now: the program in place is to start
    restart: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl Offer {
    pub fn new(later: Duration) -> Offer {
        Offer { wnd: Default::default(), shown: false, quiet_since: None, later: None, later_ms: later.as_millis() as i64, timeout: 0, restart: false }
    }

    /// A frame: up when a program is `ready` (with `notes`, what is new in
    /// it), the moment quiet for `QUIET_MS` and no Later standing; put away
    /// (and asked again when it can be) when a window of the game's or an
    /// edit box came up meanwhile.
    pub fn frame(&mut self, ready: bool, notes: Option<&WhatsNew>, m: Moment) {
        if self.restart {
            return;
        }
        self.quiet_since = if m.quiet { Some(self.quiet_since.unwrap_or(m.now)) } else { None };
        if self.shown {
            if !m.plain || !ready {
                self.shown = false;
                self.wnd.release();
            }
            return;
        }
        if ready && self.quiet_since.is_some_and(|t| m.now - t >= QUIET_MS) && self.later.is_none_or(|t| m.now >= t) {
            self.wnd.open(notes.cloned().unwrap_or_default(), m.world);
            self.timeout = m.now + BOX_MS;
            self.shown = true;
        }
    }

    pub fn shown(&self) -> bool {
        self.shown
    }

    /// Restart now was answered.
    pub fn restart(&self) -> bool {
        self.restart
    }

    /// Whether a key is one the window takes, the game's code: `VIKeyboard`'s
    /// 13 Enter, 27 Escape, 17 to 20 the cursor keys, 21 to 24 Page Up, Page
    /// Down, End, Home; the pad's 134 ✕, 132 △ and 136 to 139 the d-pad.
    pub fn takes(key: i32) -> bool {
        matches!(key, 13 | 27 | 17..=24 | 134 | 132 | 136..=139)
    }

    /// A key pressed: true when the window took it (it is up, and the key is
    /// one of its own).
    pub fn key(&mut self, key: i32, now: i64) -> bool {
        if !self.shown || !Self::takes(key) {
            return false;
        }
        let a = self.wnd.key(key, now);
        self.answer(a, now);
        true
    }

    /// A press at frame point `p`: on a choice, that choice; on the list, a
    /// drag begun. True when it was the window's.
    pub fn click(&mut self, p: [f32; 2], now: i64) -> bool {
        if !self.shown {
            return false;
        }
        match self.wnd.press(p) {
            Some(a) => {
                self.answer(a, now);
                true
            }
            None => false,
        }
    }

    /// The pointer moved while pressed: true while it drags the list.
    pub fn drag(&mut self, p: [f32; 2], now: i64) -> bool {
        let on = self.shown && self.wnd.drag_to(p);
        if on {
            self.timeout = now + BOX_MS;
        }
        on
    }

    /// The press let go: true when it was the list's drag.
    pub fn release(&mut self) -> bool {
        self.wnd.release()
    }

    /// The wheel's `lines` (up positive): the list's while the window is up.
    pub fn wheel(&mut self, lines: f32, now: i64) -> bool {
        if !self.shown {
            return false;
        }
        if lines != 0.0 {
            self.wnd.wheel(lines);
            self.timeout = now + BOX_MS;
        }
        true
    }

    /// A stick's up and down (down positive) held `dt` seconds: the list's.
    pub fn stick(&mut self, y: f32, dt: f32, now: i64) {
        if self.shown && y != 0.0 {
            self.wnd.stick(y, dt);
            self.timeout = now + BOX_MS;
        }
    }

    /// The window over the frame (the 2D layer's, frame pixels), and its time
    /// running out (a Later).
    pub fn draw(&mut self, vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster, now: i64) {
        if !self.shown {
            return;
        }
        self.wnd.draw(vui, r, self.timeout - now);
        if now >= self.timeout {
            self.answer(Some(crate::updatewnd::Answer::Later), now);
        }
    }

    fn answer(&mut self, a: Option<crate::updatewnd::Answer>, now: i64) {
        self.timeout = now + BOX_MS;
        match a {
            Some(crate::updatewnd::Answer::Restart) => {
                self.shown = false;
                self.restart = true;
            }
            Some(crate::updatewnd::Answer::Later) => {
                self.shown = false;
                self.later = Some(now + self.later_ms);
            }
            None => {}
        }
    }

    /// The sounds the window asked for since the last call.
    pub fn take_cues(&mut self) -> Vec<i32> {
        self.wnd.take_cues()
    }
}

/// The game's one question (natively; a page asks its own, `web/main.js`).
#[cfg(not(target_arch = "wasm32"))]
static OFFER: std::sync::Mutex<Option<Offer>> = std::sync::Mutex::new(None);

/// `f` on the question, made the first time.
#[cfg(not(target_arch = "wasm32"))]
fn with_offer<T>(f: impl FnOnce(&mut Offer) -> T) -> T {
    let mut o = OFFER.lock().unwrap_or_else(|e| e.into_inner());
    f(o.get_or_insert_with(|| Offer::new(intervals(std::env::var("EQOA_UPDATE_EVERY").ok().as_deref()).2)))
}

/// Whether the question may come up, a program being `ready`: only with
/// Update Notices on (the Extended page, `settings::update_notices`); off,
/// the program in place starts the next time the game does, unasked, and a
/// question up is put away.
#[cfg(not(target_arch = "wasm32"))]
pub fn asks(ready: bool) -> bool {
    ready && crate::settings::update_notices()
}

/// Each frame there is a UI to draw on (the game's screens' or the HUD's):
/// the window up or away by the moment.
pub fn offer_frame(m: Moment) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let ready = ready().is_some();
        if ready && !asks(ready) && !QUIET_SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
            crate::say("update: Update Notices are off: the program in place starts the next time the game does");
        }
        let notes = ready_notes();
        with_offer(|o| o.frame(asks(ready), notes.as_ref(), m));
    }
    #[cfg(target_arch = "wasm32")]
    let _ = m;
}

/// The program in place said to wait for the next start (`offer_frame`),
/// once a run.
#[cfg(not(target_arch = "wasm32"))]
static QUIET_SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the window is up (it takes its own input).
pub fn offer_shown() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return OFFER.lock().ok().is_some_and(|o| o.as_ref().is_some_and(Offer::shown));
    #[cfg(target_arch = "wasm32")]
    false
}

/// A key pressed, the game's code (`Offer::takes`): the sounds to play when
/// the window took it. A Restart now makes the program in place the one to
/// start.
pub fn offer_key(key: i32, now: i64) -> Option<Vec<i32>> {
    #[cfg(not(target_arch = "wasm32"))]
    return answered(|o| o.key(key, now));
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (key, now);
        None
    }
}

/// A press at frame point `p`: the sounds to play when the window took it.
pub fn offer_click(p: [f32; 2], now: i64) -> Option<Vec<i32>> {
    #[cfg(not(target_arch = "wasm32"))]
    return answered(|o| o.click(p, now));
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (p, now);
        None
    }
}

/// The pointer at `p` while pressed: true while it drags the window's list.
pub fn offer_drag(p: [f32; 2], now: i64) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return with_offer(|o| o.drag(p, now));
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (p, now);
        false
    }
}

/// What a point is on of the question (`offer_part`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub enum OfferPart {
    /// its list, which a press drags
    List,
    /// elsewhere on its window (its choices among it)
    Window,
}

/// What frame point `p` is on of the question while it is up: a finger put
/// down there is the question's, not the touch screen's pad's.
pub fn offer_part(p: [f32; 2]) -> Option<OfferPart> {
    #[cfg(not(target_arch = "wasm32"))]
    return with_offer(|o| o.shown.then(|| o.wnd.part(p)).flatten());
    #[cfg(target_arch = "wasm32")]
    {
        let _ = p;
        None
    }
}

/// The press let go: true when it was the window's drag.
pub fn offer_release() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return with_offer(Offer::release);
    #[cfg(target_arch = "wasm32")]
    false
}

/// The wheel's lines (up positive): true when the window took them (it is
/// up).
pub fn offer_wheel(lines: f32, now: i64) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return with_offer(|o| o.wheel(lines, now));
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (lines, now);
        false
    }
}

/// A stick's up and down (down positive) held `dt` seconds, while the window
/// is up.
pub fn offer_stick(y: f32, dt: f32, now: i64) {
    #[cfg(not(target_arch = "wasm32"))]
    with_offer(|o| o.stick(y, dt, now));
    #[cfg(target_arch = "wasm32")]
    let _ = (y, dt, now);
}

/// The window over the frame, when it is up.
pub fn offer_draw(vui: &vi::ui::Vui, r: &mut dyn vi::ui::Raster, now: i64) {
    #[cfg(not(target_arch = "wasm32"))]
    answered(|o| {
        o.draw(vui, r, now);
        true
    });
    #[cfg(target_arch = "wasm32")]
    let _ = (vui, r, now);
}

/// `f` on the question, and a Restart now it brought made good: the program
/// in place to start (`native::accept`); the window's sounds when `f` took
/// its input.
#[cfg(not(target_arch = "wasm32"))]
fn answered(f: impl FnOnce(&mut Offer) -> bool) -> Option<Vec<i32>> {
    let (took, yes, cues) = with_offer(|o| {
        let was = o.restart;
        let took = f(o);
        (took, o.restart && !was, o.take_cues())
    });
    if yes {
        if native::accept() {
            crate::say("update: restarting into the program in place");
        } else {
            with_offer(|o| o.restart = false);
        }
    }
    took.then_some(cues)
}

/// Restart now was answered: the game logs out and the window closes, for
/// the program in place to start (`relaunch_if_requested`).
pub fn restarting() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return OFFER.lock().ok().is_some_and(|o| o.as_ref().is_some_and(Offer::restart));
    #[cfg(target_arch = "wasm32")]
    false
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{Build, COMMIT, CHANNEL, Manifest, NOTES_FILE, OWN_NOTES, PLATFORM, State, WAIT, WhatsNew, whats_new};
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::{Duration, Instant, SystemTime};

    /// The release this build follows.
    const RELEASE: &str = "https://github.com/DabDavis/eqoa-rust-client-vanilla/releases/download/dev/";

    struct Shared {
        state: State,
        since: Instant,
        /// the program's path as it started (after the swap, Linux names a
        /// running program by its new name, `<exe>.old`)
        exe: PathBuf,
        relaunch: bool,
        /// the release's version of a program in place other than this
        /// running one, found while the game runs (`watch`), and what is new
        /// in it
        ready: Option<String>,
        notes: Option<WhatsNew>,
        /// what the last look found, for a report (`describe`)
        found: Option<String>,
    }

    static UPDATER: Mutex<Option<Shared>> = Mutex::new(None);

    pub(super) fn with<T>(f: impl FnOnce(&State, Duration) -> T) -> Option<T> {
        UPDATER.lock().ok()?.as_ref().map(|s| f(&s.state, s.since.elapsed()))
    }

    fn set(state: State) {
        if let Ok(mut u) = UPDATER.lock()
            && let Some(s) = u.as_mut()
        {
            s.state = state;
        }
    }

    /// What the last look found, said (`describe`); none before one ended.
    pub(super) fn found() -> Option<String> {
        let u = UPDATER.lock().ok()?;
        let s = u.as_ref()?;
        Some(s.found.clone().unwrap_or_else(|| format!("looking ({:.0} s)", s.since.elapsed().as_secs_f32())))
    }

    /// Why this run does not look (no updater was made: `start`).
    pub(super) fn off() -> &'static str {
        if CHANNEL.is_none() || COMMIT.is_none() {
            "a build of its own (no release to follow)"
        } else if PLATFORM.is_none() {
            "no release build for this platform"
        } else if std::env::var_os("EQOA_NO_UPDATE").is_some() {
            "off (EQOA_NO_UPDATE)"
        } else {
            "off for a scripted run"
        }
    }

    fn note_found(what: String) {
        if let Ok(mut u) = UPDATER.lock()
            && let Some(s) = u.as_mut()
        {
            s.found = Some(what);
        }
    }

    /// The updater's thread, when this build follows a channel and the run
    /// is a player's (the module's doc): the start's check, then the watch.
    pub fn start() {
        clean_up();
        let (Some(_), Some(commit), Some(platform)) = (CHANNEL, COMMIT, PLATFORM) else { return };
        // the program an update just started does not ask at its start again
        // (it watches, as any does)
        let updated = std::env::var("EQOA_UPDATED").ok();
        if let Some(from) = &updated {
            crate::say(&format!("update: this is the release's build now ({from})"));
        }
        // (a server of one's own is a test of the updater: it runs offscreen too)
        let test = std::env::var("EQOA_UPDATE_URL").ok();
        if std::env::var_os("EQOA_NO_UPDATE").is_some() || (std::env::var_os("EQOA_OFFSCREEN").is_some() && test.is_none()) {
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let base = test.unwrap_or_else(|| RELEASE.to_string());
        let every = super::intervals(std::env::var("EQOA_UPDATE_EVERY").ok().as_deref());
        let state = if updated.is_some() { State::Done } else { State::Checking };
        let found = updated.as_ref().map(|v| format!("started by the update to {v}"));
        *UPDATER.lock().unwrap() = Some(Shared { state, since: Instant::now(), exe: exe.clone(), relaunch: false, ready: None, notes: None, found });
        let spawned = std::thread::Builder::new().name("update".into()).spawn(move || {
            // this program's own bytes, before anything is put in its place
            let own = sha256_file(&exe).ok();
            if updated.is_none() {
                let state = match update(&base, &exe, commit, platform, &set) {
                    Ok(Some(version)) => {
                        crate::say(&format!("update: {version} is in place"));
                        note_found(format!("the start put {version} in place"));
                        State::Staged { version }
                    }
                    Ok(None) => {
                        note_found("the start found nothing to put in place (the release is this build, or its program is in place)".to_string());
                        State::Done
                    }
                    Err(e) => {
                        crate::say(&format!("update: {e}"));
                        note_found(format!("the start's look failed: {e}"));
                        State::Done
                    }
                };
                // in place after the boot went on without it: offered now
                if let State::Staged { version } = &state
                    && with(|_, waited| waited >= WAIT).unwrap_or(false)
                {
                    offer(&base, version.clone());
                }
                set(state);
            }
            watch(&base, &exe, commit, platform, own.as_deref(), every);
        });
        if spawned.is_err() {
            set(State::Done);
        }
    }

    /// The version of a program in place other than this one (`watch`).
    pub(super) fn ready() -> Option<String> {
        UPDATER.lock().ok()?.as_ref()?.ready.clone()
    }

    /// What is new in the program in place, looked for with it.
    pub(super) fn notes() -> Option<WhatsNew> {
        UPDATER.lock().ok()?.as_ref()?.notes.clone()
    }

    /// The program in place (`version`) offered, with what is new in it: the
    /// release's notes, read beside update.txt, set against this build's own
    /// (`whats_new`). A read that failed is tried again at the next look.
    fn offer(base: &str, version: String) {
        let (same, unread) = UPDATER
            .lock()
            .ok()
            .and_then(|u| u.as_ref().map(|s| (s.ready.as_deref() == Some(version.as_str()), matches!(s.notes, None | Some(WhatsNew::Unread)))))
            .unwrap_or((false, true));
        if same && !unread {
            return;
        }
        let notes = match fetch_notes(base) {
            Ok(text) => whats_new(&text, OWN_NOTES),
            Err(e) => {
                if !same {
                    crate::say(&format!("update: what is new in it: {e}"));
                }
                WhatsNew::Unread
            }
        };
        if !same {
            let what = match &notes {
                WhatsNew::Sections(s) => format!("{} new section(s) of notes", s.len()),
                WhatsNew::Nothing => "no notes new to this build".to_string(),
                WhatsNew::Unread => "its notes not read".to_string(),
            };
            crate::say(&format!("update: {version} is in place: the game offers to restart into it, Update Notices on ({what})"));
        }
        if let Ok(mut u) = UPDATER.lock()
            && let Some(s) = u.as_mut()
        {
            s.ready = Some(version);
            s.notes = Some(notes);
        }
    }

    /// The release's update notes (`NOTES_FILE`, beside update.txt).
    pub(super) fn fetch_notes(base: &str) -> Result<String, String> {
        let url = format!("{base}{NOTES_FILE}");
        agent()
            .get(&url)
            .call()
            .map_err(|e| format!("{url}: {e}"))?
            .body_mut()
            .with_config()
            .limit(1024 * 1024)
            .read_to_string()
            .map_err(|e| format!("{url}: {e}"))
    }

    /// The player said Yes to the program in place: it is what
    /// `relaunch_if_requested` starts once the window closes. False when
    /// there is none (nothing to restart into).
    pub(super) fn accept() -> bool {
        let Ok(mut u) = UPDATER.lock() else { return false };
        let Some(s) = u.as_mut() else { return false };
        let Some(version) = s.ready.clone() else { return false };
        s.state = State::Staged { version };
        s.relaunch = true;
        true
    }

    /// The release asked again while the game runs (the module's doc):
    /// `first` after the start, then every `every`; a program other than
    /// this one in place is `ready`. A failure is said once until it
    /// changes (offline, every look would say it).
    fn watch(base: &str, exe: &Path, commit: &str, platform: &str, own: Option<&str>, (first, every, _): (Duration, Duration, Duration)) {
        let mut said = String::new();
        std::thread::sleep(first);
        loop {
            match watch_once(base, exe, commit, platform, own) {
                Ok(Some(version)) => {
                    note_found(format!("{version} is in place, for a restart"));
                    offer(base, version);
                    said.clear();
                }
                Ok(None) => {
                    note_found("the release is this build".to_string());
                    said.clear();
                }
                Err(e) => {
                    note_found(format!("the last look failed: {e}"));
                    if e != said {
                        crate::say(&format!("update: {e}"));
                        said = e;
                    }
                }
            }
            std::thread::sleep(every);
        }
    }

    /// One look at the release while the game runs: the release's version
    /// when a program other than this running one is in place now, put
    /// there by this look as the start's puts it, or there already (another
    /// window's, or the start's own download that came after the title);
    /// else None. `own` is this program's SHA-256 as it started: the
    /// release's program with this one's bytes (another commit's name on
    /// the same build) is no update, or the game would offer to restart
    /// into itself for good.
    pub(super) fn watch_once(base: &str, exe: &Path, commit: &str, platform: &str, own: Option<&str>) -> Result<Option<String>, String> {
        Ok(match look(base, exe, commit, platform, &|_| {})? {
            Look::Current | Look::Busy => None,
            Look::Staged(version) => Some(version),
            Look::InPlace { version, sha256 } => own.filter(|o| *o != sha256).map(|_| version),
        })
    }

    pub(super) fn request_relaunch() {
        if let Ok(mut u) = UPDATER.lock()
            && let Some(s) = u.as_mut()
        {
            s.relaunch = true;
        }
    }

    /// Once the window and the sound device are closed (main's `boot`): the
    /// program the update put in place, started with this one's arguments,
    /// in its place (on Linux and macOS the same process).
    pub fn relaunch_if_requested() {
        let Some((exe, version)) = UPDATER.lock().ok().and_then(|u| {
            let s = u.as_ref()?;
            let State::Staged { version } = &s.state else { return None };
            s.relaunch.then(|| (s.exe.clone(), version.clone()))
        }) else {
            return;
        };
        // stderr as it was before the log's pipe: the new program would
        // inherit a pipe nothing reads any more; and this window's place
        // (multiboxing) let go for it
        crate::report::restore_stderr();
        crate::multibox::leave();
        let mut c = std::process::Command::new(&exe);
        c.args(std::env::args_os().skip(1)).env("EQOA_UPDATED", &version);
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let e = c.exec();
            crate::say(&format!("update: {} did not start: {e}", exe.display()));
        }
        #[cfg(not(unix))]
        if let Err(e) = c.spawn() {
            crate::say(&format!("update: {} did not start: {e}", exe.display()));
        }
    }

    /// The programs updates moved aside, removed (`asides`): on Windows one
    /// may still be running, for a moment or as another window of the game
    /// that has not restarted yet, and a later start removes it then.
    pub fn clean_up() {
        if let Ok(exe) = std::env::current_exe() {
            for p in asides(&exe) {
                remove(&p);
            }
        }
    }

    /// Where the running program goes aside for the new one: `<exe>.old`,
    /// or the first `<exe>.old-<n>` free when that cannot be removed. On
    /// Windows nothing removes or replaces a program while it runs, and
    /// another window of the game may still be running the one the last
    /// update moved aside (F3 20261027-054237: every look failed "Access is
    /// denied" on the rename onto it).
    pub(super) fn aside(exe: &Path) -> PathBuf {
        let old = sibling(exe, "old");
        if remove(&old) {
            return old;
        }
        (1..100).map(|n| sibling(exe, &format!("old-{n}"))).find(|p| remove(p)).unwrap_or(old)
    }

    /// Every program an update moved aside beside `exe` (`aside`'s names).
    pub(super) fn asides(exe: &Path) -> Vec<PathBuf> {
        let (Some(dir), Some(name)) = (exe.parent(), exe.file_name().and_then(|n| n.to_str())) else { return Vec::new() };
        let Ok(list) = std::fs::read_dir(dir) else { return Vec::new() };
        let old = format!("{name}.old");
        list.filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_str().is_some_and(|f| f == old || f.strip_prefix(&old).and_then(|r| r.strip_prefix('-')).is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))))
            .map(|e| e.path())
            .collect()
    }

    /// `path` removed, or not there to begin with. Windows removes no
    /// read-only file, so one is made writable first (a program a player
    /// marked read-only was moved aside so, and stayed for good).
    pub(super) fn remove(path: &Path) -> bool {
        match std::fs::symlink_metadata(path) {
            Err(e) => e.kind() == std::io::ErrorKind::NotFound,
            Ok(m) if m.is_dir() => false,
            Ok(m) => {
                #[cfg(windows)]
                if m.permissions().readonly() {
                    let mut p = m.permissions();
                    p.set_readonly(false);
                    let _ = std::fs::set_permissions(path, p);
                }
                #[cfg(not(windows))]
                let _ = m;
                std::fs::remove_file(path).is_ok()
            }
        }
    }

    /// `<exe>.<ext>` beside the program (`eqoa.exe.new`).
    pub(super) fn sibling(exe: &Path, ext: &str) -> PathBuf {
        let mut name = exe.file_name().unwrap_or_default().to_os_string();
        name.push(".");
        name.push(ext);
        exe.with_file_name(name)
    }

    fn agent() -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(120)))
            .user_agent(format!("eqoa-vanilla/{}", COMMIT.unwrap_or("local")))
            .build()
            .into()
    }

    /// What a look at the release found (`look`).
    #[derive(Clone, Debug, PartialEq)]
    pub(super) enum Look {
        /// the release is this build's commit
        Current,
        /// the release's program (its SHA-256 `sha256`) is the one in place
        /// already: another window put it there, or it is this one under
        /// another commit's name
        InPlace { version: String, sha256: String },
        /// another window of the game is putting it in place
        Busy,
        /// downloaded and put in place by this look
        Staged(String),
    }

    /// The check and, when the release is another commit's, its program
    /// downloaded and put in place: the release's version, or None when
    /// there was nothing to do.
    pub(super) fn update(base: &str, exe: &Path, commit: &str, platform: &str, progress: &dyn Fn(State)) -> Result<Option<String>, String> {
        Ok(match look(base, exe, commit, platform, progress)? {
            Look::Staged(version) => Some(version),
            _ => None,
        })
    }

    /// `update`'s check and download, saying what it found.
    fn look(base: &str, exe: &Path, commit: &str, platform: &str, progress: &dyn Fn(State)) -> Result<Look, String> {
        let agent = agent();
        let url = format!("{base}update.txt");
        let text = agent
            .get(&url)
            .call()
            .map_err(|e| format!("{url}: {e}"))?
            .body_mut()
            .with_config()
            .limit(64 * 1024)
            .read_to_string()
            .map_err(|e| format!("{url}: {e}"))?;
        let m = Manifest::parse(&text);
        if m.commit.is_empty() {
            return Err(format!("{url} names no commit"));
        }
        if m.commit.eq_ignore_ascii_case(commit) {
            return Ok(Look::Current);
        }
        let b = m.build(platform).ok_or_else(|| format!("{url} has no {platform} build"))?;
        let version = if m.version.is_empty() { m.commit.clone() } else { m.version.clone() };
        // the release's program in place already
        if sha256_file(exe).is_ok_and(|h| h == b.sha256) {
            return Ok(Look::InPlace { version, sha256: b.sha256.clone() });
        }
        let Some(_lock) = Lock::take(exe) else {
            // another window of the game is updating it
            return Ok(Look::Busy);
        };
        let new = sibling(exe, "new");
        progress(State::Downloading { version: version.clone(), done: 0, total: b.size });
        download(&agent, &format!("{base}{}", b.file), &new, b, &|done| progress(State::Downloading { version: version.clone(), done, total: b.size }))?;
        swap(exe, &new)?;
        Ok(Look::Staged(version))
    }

    /// The file at `url` into `to`, held to the build's size and SHA-256;
    /// nothing left there when it is not.
    fn download(agent: &ureq::Agent, url: &str, to: &Path, b: &Build, progress: &dyn Fn(u64)) -> Result<(), String> {
        let r = (|| -> Result<(), String> {
            let mut resp = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
            let mut body = resp.body_mut().with_config().limit(b.size + 1).reader();
            let mut f = std::fs::File::create(to).map_err(|e| format!("{}: {e}", to.display()))?;
            let mut h = Sha256::new();
            let mut buf = vec![0u8; 64 * 1024];
            let mut done = 0u64;
            let mut shown = 0u64;
            loop {
                let n = body.read(&mut buf).map_err(|e| format!("{url}: {e}"))?;
                if n == 0 {
                    break;
                }
                h.update(&buf[..n]);
                f.write_all(&buf[..n]).map_err(|e| format!("{}: {e}", to.display()))?;
                done += n as u64;
                if done - shown >= 256 * 1024 {
                    progress(done);
                    shown = done;
                }
            }
            f.sync_all().map_err(|e| format!("{}: {e}", to.display()))?;
            if done != b.size {
                return Err(format!("{url}: {done} bytes, where update.txt says {}", b.size));
            }
            if hex(&h.finalize()) != b.sha256 {
                return Err(format!("{url}: its SHA-256 is not the one update.txt names"));
            }
            Ok(())
        })();
        if r.is_err() {
            let _ = std::fs::remove_file(to);
        }
        r
    }

    /// The new program in the old one's place: the old aside (`aside`: a
    /// running program may be renamed, never written to), the new one to
    /// its name, with its mode on Linux and macOS; the old one put back when
    /// the second rename fails. (Windows' one permission is read-only, which
    /// is not carried over: a read-only program aside is one no update could
    /// remove.)
    pub(super) fn swap(exe: &Path, new: &Path) -> Result<(), String> {
        #[cfg(unix)]
        if let Ok(m) = std::fs::metadata(exe) {
            let _ = std::fs::set_permissions(new, m.permissions());
        }
        let old = aside(exe);
        std::fs::rename(exe, &old).map_err(|e| format!("{}: {e}", exe.display()))?;
        if let Err(e) = std::fs::rename(new, exe) {
            let _ = std::fs::rename(&old, exe);
            let _ = std::fs::remove_file(new);
            return Err(format!("{}: {e}", exe.display()));
        }
        Ok(())
    }

    pub(super) fn sha256_file(path: &Path) -> std::io::Result<String> {
        let mut f = std::fs::File::open(path)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(hex(&h.finalize()))
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    /// `<exe>.lock`, held while one window of the game updates the program:
    /// another started with it leaves the update to it. One left by a
    /// program that died is taken after ten minutes.
    struct Lock(PathBuf);

    impl Lock {
        fn take(exe: &Path) -> Option<Lock> {
            let path = sibling(exe, "lock");
            let stale = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok()).is_some_and(|age| age > Duration::from_secs(600));
            if stale {
                let _ = std::fs::remove_file(&path);
            }
            std::fs::OpenOptions::new().write(true).create_new(true).open(&path).ok().map(|_| Lock(path))
        }
    }

    impl Drop for Lock {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::native::{aside, asides, remove, sha256_file, sibling, swap, update, watch_once};
    use super::*;
    use std::io::{Read, Write};
    use std::path::PathBuf;

    const OLD: &[u8] = b"the program as it is";
    const NEW: &[u8] = b"the release's program, a commit later";

    fn sha(b: &[u8]) -> String {
        use sha2::Digest;
        sha2::Sha256::digest(b).iter().map(|x| format!("{x:02x}")).collect()
    }

    /// A directory of its own in the temporary directory, emptied.
    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("eqoa-update-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A server on a port of its own answering `files` by path (404 for any
    /// other), each request on a connection of its own.
    fn serve(files: Vec<(&'static str, Vec<u8>)>) -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for s in l.incoming() {
                let Ok(mut s) = s else { continue };
                let mut req = Vec::new();
                let mut b = [0u8; 1];
                while !req.ends_with(b"\r\n\r\n") && s.read(&mut b).is_ok_and(|n| n == 1) {
                    req.push(b[0]);
                }
                let line = String::from_utf8_lossy(&req).lines().next().unwrap_or_default().to_string();
                let path = line.split_whitespace().nth(1).unwrap_or("/").trim_start_matches('/').to_string();
                match files.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => {
                        let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
                        let _ = s.write_all(body);
                    }
                    None => {
                        let _ = write!(s, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
        });
        base
    }

    fn manifest(commit: &str, sha: &str, size: usize) -> Vec<u8> {
        format!(
            "version dev-{}\ncommit {commit}\nbuild linux-x86_64 eqoa-vanilla-linux-x86_64 {sha} {size}\nbuild windows-x86_64 eqoa-vanilla-windows-x86_64.exe {sha} {size}\nbuild macos eqoa-vanilla-macos {sha} {size}\n",
            &commit[..7]
        )
        .into_bytes()
    }

    const MINE: &str = "1111111111111111111111111111111111111111";
    const THEIRS: &str = "2222222222222222222222222222222222222222";

    /// Its lines as dev.yml writes them; a line that does not read, a short
    /// hash and unknown lines passed over; no such platform is none.
    #[test]
    fn the_manifest_reads_as_ci_writes_it() {
        let text = String::from_utf8(manifest(THEIRS, &sha(NEW), NEW.len())).unwrap() + "build linux-x86_64 other abc 12\nbuild ios x zz 1\nsomething else\n";
        let m = Manifest::parse(&text);
        assert_eq!((m.version.as_str(), m.commit.as_str(), m.builds.len()), ("dev-2222222", THEIRS, 3));
        assert_eq!(m.build("windows-x86_64").map(|b| (b.file.as_str(), b.size)), Some(("eqoa-vanilla-windows-x86_64.exe", NEW.len() as u64)));
        assert_eq!(m.build("linux-x86_64").unwrap().sha256, sha(NEW), "the first linux line, not the one with a short hash");
        assert!(m.build("freebsd").is_none());
    }

    /// Another commit's program downloaded and put in place, the old one
    /// aside; the same commit, the same program and a release that does not
    /// answer leave it as it is.
    #[test]
    fn another_commits_program_is_put_in_place() {
        let d = dir("swap");
        let exe = d.join("eqoa");
        std::fs::write(&exe, OLD).unwrap();
        let base = serve(vec![("update.txt", manifest(THEIRS, &sha(NEW), NEW.len())), ("eqoa-vanilla-linux-x86_64", NEW.to_vec())]);
        // the control: this program's own commit
        assert_eq!(update(&base, &exe, THEIRS, "linux-x86_64", &|_| {}), Ok(None));
        assert_eq!(std::fs::read(&exe).unwrap(), OLD);
        let seen = std::sync::Mutex::new(Vec::new());
        assert_eq!(update(&base, &exe, MINE, "linux-x86_64", &|s| seen.lock().unwrap().push(s)), Ok(Some("dev-2222222".into())));
        assert_eq!(std::fs::read(&exe).unwrap(), NEW);
        assert_eq!(std::fs::read(sibling(&exe, "old")).unwrap(), OLD);
        assert!(!sibling(&exe, "new").exists() && !sibling(&exe, "lock").exists());
        assert_eq!(seen.lock().unwrap().first(), Some(&State::Downloading { version: "dev-2222222".into(), done: 0, total: NEW.len() as u64 }));
        // the program is the release's already (another commit, the same bytes)
        assert_eq!(update(&base, &exe, MINE, "linux-x86_64", &|_| {}), Ok(None));
        // nothing answers
        let gone = "http://127.0.0.1:9/";
        assert!(update(gone, &exe, MINE, "linux-x86_64", &|_| {}).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), NEW);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A download that is not the program update.txt promised (another
    /// hash, another size, no such file) leaves the program as it is and
    /// nothing beside it.
    #[test]
    fn a_download_that_is_not_the_promised_one_changes_nothing() {
        for (name, m, file) in [
            ("hash", manifest(THEIRS, &sha(b"something else"), NEW.len()), NEW.to_vec()),
            ("size", manifest(THEIRS, &sha(NEW), NEW.len() + 1), NEW.to_vec()),
            ("long", manifest(THEIRS, &sha(NEW), NEW.len() - 1), NEW.to_vec()),
        ] {
            let d = dir(name);
            let exe = d.join("eqoa.exe");
            std::fs::write(&exe, OLD).unwrap();
            let base = serve(vec![("update.txt", m), ("eqoa-vanilla-windows-x86_64.exe", file)]);
            assert!(update(&base, &exe, MINE, "windows-x86_64", &|_| {}).is_err(), "{name}");
            assert_eq!(std::fs::read(&exe).unwrap(), OLD, "{name}");
            assert!(!sibling(&exe, "new").exists() && !sibling(&exe, "old").exists(), "{name}");
            let _ = std::fs::remove_dir_all(&d);
        }
        let d = dir("missing");
        let exe = d.join("eqoa");
        std::fs::write(&exe, OLD).unwrap();
        let base = serve(vec![("update.txt", manifest(THEIRS, &sha(NEW), NEW.len()))]);
        assert!(update(&base, &exe, MINE, "macos", &|_| {}).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), OLD);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Another window updating the program (its lock) leaves it alone.
    #[test]
    fn a_second_window_leaves_the_update_to_the_first() {
        let d = dir("lock");
        let exe = d.join("eqoa");
        std::fs::write(&exe, OLD).unwrap();
        std::fs::write(sibling(&exe, "lock"), b"").unwrap();
        let base = serve(vec![("update.txt", manifest(THEIRS, &sha(NEW), NEW.len())), ("eqoa-vanilla-linux-x86_64", NEW.to_vec())]);
        assert_eq!(update(&base, &exe, MINE, "linux-x86_64", &|_| {}), Ok(None));
        assert_eq!(std::fs::read(&exe).unwrap(), OLD);
        std::fs::remove_file(sibling(&exe, "lock")).unwrap();
        assert!(update(&base, &exe, MINE, "linux-x86_64", &|_| {}).unwrap().is_some(), "the control: unlocked, it updates");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The second rename failing puts the old program back.
    #[test]
    fn a_failed_swap_puts_the_program_back() {
        let d = dir("rollback");
        let exe = d.join("eqoa");
        std::fs::write(&exe, OLD).unwrap();
        assert!(swap(&exe, &d.join("not-there")).is_err());
        assert_eq!(std::fs::read(&exe).unwrap(), OLD);
        assert!(!sibling(&exe, "old").exists());
        assert_eq!(sha256_file(&exe).unwrap(), sha(OLD));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The program the last update moved aside, still there and not to be
    /// removed (on Windows another window still running it; here a
    /// directory in its place, which neither a removal nor a rename takes):
    /// the running one goes aside under the next free name and the update
    /// goes through. (Before: the rename onto it failed, every look.)
    #[test]
    fn a_program_aside_still_running_leaves_the_update_another_name() {
        let d = dir("busyold");
        let exe = d.join("eqoa.exe");
        std::fs::write(&exe, OLD).unwrap();
        let held = sibling(&exe, "old");
        std::fs::create_dir(&held).unwrap();
        std::fs::write(held.join("running"), b"another window").unwrap();
        let base = serve(vec![("update.txt", manifest(THEIRS, &sha(NEW), NEW.len())), ("eqoa-vanilla-windows-x86_64.exe", NEW.to_vec())]);
        assert_eq!(update(&base, &exe, MINE, "windows-x86_64", &|_| {}), Ok(Some("dev-2222222".into())));
        assert_eq!(std::fs::read(&exe).unwrap(), NEW);
        assert_eq!(std::fs::read(sibling(&exe, "old-1")).unwrap(), OLD);
        assert!(held.join("running").exists(), "the held one is left alone");
        // the next update while it still runs: the free name's file is
        // removed and used again
        std::fs::write(&exe, b"a third program").unwrap();
        assert_eq!(update(&base, &exe, MINE, "windows-x86_64", &|_| {}), Ok(Some("dev-2222222".into())));
        assert_eq!(std::fs::read(sibling(&exe, "old-1")).unwrap(), b"a third program");
        assert!(!sibling(&exe, "old-2").exists());
        // the control: free again, `<exe>.old` itself
        std::fs::remove_dir_all(&held).unwrap();
        assert_eq!(aside(&exe), held);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The start's clean-up takes every program aside, and nothing else
    /// beside the program.
    #[test]
    fn the_programs_aside_are_found_by_their_names() {
        let d = dir("asides");
        let exe = d.join("eqoa.exe");
        for f in ["eqoa.exe", "eqoa.exe.old", "eqoa.exe.old-1", "eqoa.exe.old-12", "eqoa.exe.older", "eqoa.exe.old-x", "eqoa.exe.old-", "eqoa.exe.new", "eqoa.exe.lock", "other.exe.old"] {
            std::fs::write(d.join(f), f).unwrap();
        }
        let mut found: Vec<String> = asides(&exe).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        found.sort();
        assert_eq!(found, ["eqoa.exe.old", "eqoa.exe.old-1", "eqoa.exe.old-12"]);
        for p in asides(&exe) {
            assert!(remove(&p));
        }
        assert!(asides(&exe).is_empty() && exe.exists() && d.join("eqoa.exe.older").exists());
        assert!(remove(&d.join("not-there")), "nothing there is removed already");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The boot waits while the updater checks or downloads, for `WAIT` at
    /// most; its line says how far the download is.
    #[test]
    fn the_boot_waits_for_the_updater_at_most_its_while() {
        let down = State::Downloading { version: "dev-2222222".into(), done: 512, total: 1024 };
        assert!(!State::Checking.settled(Duration::from_secs(1)) && !down.settled(Duration::from_secs(29)));
        assert!(down.settled(WAIT) && State::Done.settled(Duration::ZERO));
        assert!(State::Staged { version: "v".into() }.settled(Duration::ZERO));
        assert_eq!(down.line().as_deref(), Some("Downloading the update (dev-2222222): 50%"));
        assert_eq!(State::Checking.line(), None);
    }

    /// While the game runs: another commit's program put in place is
    /// offered, as is one another window put there first; the release's
    /// program with this one's bytes (another commit's name on the same
    /// build), this commit's own, and a window still updating are not.
    #[test]
    fn the_watch_offers_a_program_other_than_this_one() {
        let d = dir("watch");
        let exe = d.join("eqoa");
        std::fs::write(&exe, OLD).unwrap();
        let base = serve(vec![("update.txt", manifest(THEIRS, &sha(NEW), NEW.len())), ("eqoa-vanilla-linux-x86_64", NEW.to_vec())]);
        let own = sha(OLD);
        // this commit's own: nothing
        assert_eq!(watch_once(&base, &exe, THEIRS, "linux-x86_64", Some(&own)), Ok(None));
        // another window updating it: nothing yet
        std::fs::write(sibling(&exe, "lock"), b"").unwrap();
        assert_eq!(watch_once(&base, &exe, MINE, "linux-x86_64", Some(&own)), Ok(None));
        assert_eq!(std::fs::read(&exe).unwrap(), OLD);
        std::fs::remove_file(sibling(&exe, "lock")).unwrap();
        // put in place by the look, and offered
        assert_eq!(watch_once(&base, &exe, MINE, "linux-x86_64", Some(&own)), Ok(Some("dev-2222222".into())));
        assert_eq!(std::fs::read(&exe).unwrap(), NEW);
        // in place already (another window's, or this one's look before): offered again, nothing fetched
        std::fs::remove_file(sibling(&exe, "old")).unwrap();
        assert_eq!(watch_once(&base, &exe, MINE, "linux-x86_64", Some(&own)), Ok(Some("dev-2222222".into())));
        assert!(!sibling(&exe, "old").exists(), "nothing was swapped");
        // the control: this program IS the release's bytes under another commit's name
        assert_eq!(watch_once(&base, &exe, MINE, "linux-x86_64", Some(&sha(NEW))), Ok(None));
        // nor when this program's own bytes could not be read
        assert_eq!(watch_once(&base, &exe, MINE, "linux-x86_64", None), Ok(None));
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The watch's times, and a test's shorter ones; a bad value is none.
    #[test]
    fn the_watch_asks_now_and_then() {
        assert_eq!(intervals(None), (FIRST, EVERY, LATER));
        assert_eq!(intervals(Some("5")), (Duration::from_secs(5), Duration::from_secs(5), Duration::from_secs(15)));
        assert_eq!(intervals(Some("0")), (FIRST, EVERY, LATER));
        assert_eq!(intervals(Some("soon")), (FIRST, EVERY, LATER));
        assert!(FIRST < EVERY && EVERY < LATER);
    }

    fn quiet(now: i64) -> Moment {
        Moment { quiet: true, plain: true, world: true, now }
    }

    /// The moment quiet from `t`, and `QUIET_MS` later.
    fn settle(o: &mut Offer, ready: bool, t: i64) {
        o.frame(ready, None, quiet(t));
        o.frame(ready, None, quiet(t + QUIET_MS));
    }

    /// The question comes up only with a program ready and the moment quiet
    /// for `QUIET_MS`; it starts on Later, so Enter alone (or ✕) is a Later,
    /// which keeps it down until `LATER`; left then Enter restarts.
    #[test]
    fn the_question_starts_on_later_and_comes_back() {
        let later = 1000;
        let mut o = Offer::new(Duration::from_millis(later as u64));
        settle(&mut o, false, 0);
        assert!(!o.shown(), "nothing ready");
        o.frame(true, None, Moment { quiet: false, ..quiet(0) });
        o.frame(true, None, Moment { quiet: false, ..quiet(QUIET_MS) });
        assert!(!o.shown(), "not quiet");
        let t = 10_000;
        o.frame(true, None, quiet(t));
        o.frame(true, None, quiet(t + QUIET_MS - 1));
        assert!(!o.shown(), "quiet, not for long enough");
        assert!(!o.key(13, t), "a key while it is down is not its");
        let t = t + QUIET_MS;
        o.frame(true, None, quiet(t));
        assert!(o.shown());
        // keys not its own go by
        assert!(!o.key(119, t) && !o.key(32, t) && !o.key(144, t) && o.shown());
        // Enter on Later
        assert!(o.key(13, t));
        assert!(!o.shown() && !o.restart());
        o.frame(true, None, quiet(t + later - 1));
        assert!(!o.shown(), "not again before LATER");
        o.frame(true, None, quiet(t + later));
        assert!(o.shown(), "again after it");
        // the pad: the d-pad's left to Restart now, ✕
        assert!(o.key(136, t + later) && o.shown());
        assert!(o.key(134, t + later));
        assert!(o.restart() && !o.shown());
        o.frame(true, None, quiet(t + later + 10_000));
        assert!(!o.shown(), "restarting: no more questions");
        // △ is a Later too
        let mut o = Offer::new(Duration::from_millis(later as u64));
        settle(&mut o, true, 0);
        assert!(o.key(132, QUIET_MS) && !o.shown() && !o.restart());
    }

    /// A window or an edit box of the game's put up meanwhile puts the
    /// question away, unanswered: it is back once the moment has been quiet
    /// again (where a Later keeps it down).
    #[test]
    fn a_window_of_the_games_puts_it_away_unanswered() {
        let mut o = Offer::new(LATER);
        settle(&mut o, true, 0);
        assert!(o.shown());
        o.frame(true, None, Moment { quiet: false, plain: true, ..quiet(QUIET_MS + 5) });
        assert!(o.shown(), "a fight starting keeps it up");
        o.frame(true, None, Moment { quiet: false, plain: false, ..quiet(QUIET_MS + 10) });
        assert!(!o.shown(), "a window up puts it away");
        settle(&mut o, true, QUIET_MS + 20);
        assert!(o.shown(), "back once quiet again: it was not answered");
        // the control: a Later keeps it down
        o.key(27, 2 * QUIET_MS + 30);
        settle(&mut o, true, 2 * QUIET_MS + 40);
        assert!(!o.shown());
    }

    /// Update Notices off (the Extended page): a program ready never brings
    /// the question up, and one up goes away unanswered (no Later, no
    /// restart); on again, it comes up at the next quiet moment. The control:
    /// with them on, the same moments bring it up.
    #[test]
    fn update_notices_off_never_ask() {
        crate::settings::testing::reset(false);
        let mut o = Offer::new(LATER);
        settle(&mut o, asks(true), 0);
        assert!(o.shown(), "the control: on, it asks");
        crate::settings::set_client(crate::settings::ClientOptions { update_notices: false, ..Default::default() });
        o.frame(asks(true), None, quiet(QUIET_MS + 5));
        assert!(!o.shown() && !o.restart(), "turned off: put away, unanswered");
        settle(&mut o, asks(true), QUIET_MS + 10);
        settle(&mut o, asks(true), 10 * QUIET_MS);
        assert!(!o.shown(), "off: never again");
        let mut fresh = Offer::new(LATER);
        settle(&mut fresh, asks(true), 0);
        assert!(!fresh.shown(), "off from the start: never asked");
        crate::settings::set_client(crate::settings::ClientOptions::default());
        settle(&mut o, asks(true), 20 * QUIET_MS);
        assert!(o.shown(), "on again: asked at the next quiet moment");
        assert!(!asks(false), "nothing ready, nothing asked");
        crate::settings::testing::reset(false);
    }

    /// Left standing it counts as a Later; any input puts that off.
    #[test]
    fn left_standing_it_is_a_later() {
        use crate::pointer::testing::{NullRaster, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let mut o = Offer::new(Duration::from_millis(500));
        settle(&mut o, true, 0);
        let t = QUIET_MS;
        o.draw(&vui, &mut r, t + BOX_MS - 1);
        assert!(o.shown(), "the control: not yet");
        assert!(o.key(139, t + BOX_MS - 1), "a scroll");
        o.draw(&vui, &mut r, t + BOX_MS + 1);
        assert!(o.shown(), "put off by it");
        let t = t + 2 * BOX_MS;
        o.draw(&vui, &mut r, t);
        assert!(!o.shown() && !o.restart());
        o.frame(true, None, quiet(t + 100));
        assert!(!o.shown(), "a Later's wait");
        o.frame(true, None, quiet(t + 600));
        assert!(o.shown());
    }

    /// A press on a choice takes it (a pointer, `pointer`): Restart now's
    /// restarts, Later's is a Later; a press on the list is the window's (a
    /// drag) and answers nothing; anywhere else is not the window's.
    #[test]
    fn a_press_on_its_choices_and_its_list() {
        use crate::pointer::testing::{NullRaster, vui};
        let (vui, mut r) = (vui(), NullRaster);
        let mid = |b: [i32; 4]| [(b[0] + b[2]) as f32 / 2.0, (b[1] + b[3]) as f32 / 2.0];
        let mut o = Offer::new(LATER);
        settle(&mut o, true, 0);
        assert!(!o.click([320.0, 380.0], QUIET_MS), "nothing drawn yet: nowhere to press");
        o.draw(&vui, &mut r, QUIET_MS);
        assert!(!o.click([2.0, 2.0], QUIET_MS) && o.shown(), "the control: off the window");
        let [restart, later] = o.wnd.choices();
        assert!(o.click(mid(later), QUIET_MS));
        assert!(!o.shown() && !o.restart(), "Later's choice");
        let mut o = Offer::new(LATER);
        settle(&mut o, true, 0);
        o.draw(&vui, &mut r, QUIET_MS);
        assert!(o.click(mid(o.wnd.list_rect()), QUIET_MS) && o.shown() && !o.restart(), "the list: a drag, no answer");
        assert!(o.release() && !o.release(), "the drag let go once");
        assert!(o.click(mid(restart), QUIET_MS));
        assert!(o.restart(), "Restart now's choice");
    }

    /// The window's keys are the game's: Enter, Escape, the cursor keys,
    /// Page Up and Down, Home and End; the pad's ✕, △ and the d-pad.
    #[test]
    fn the_question_takes_only_its_own_keys() {
        for k in [13, 27, 17, 18, 19, 20, 21, 22, 23, 24, 134, 132, 136, 137, 138, 139] {
            assert!(Offer::takes(k), "{k}");
        }
        for k in [8, 9, 32, 97, 119, 128, 131, 133, 135, 140, 144] {
            assert!(!Offer::takes(k), "{k}");
        }
    }

    /// The wheel and a stick are the window's while it is up, and nobody's
    /// while it is down.
    #[test]
    fn the_wheel_and_a_stick_are_its_while_it_is_up() {
        let mut o = Offer::new(LATER);
        assert!(!o.wheel(1.0, 0), "down: not its");
        settle(&mut o, true, 0);
        assert!(o.wheel(-1.0, QUIET_MS), "up: its");
        o.stick(1.0, 0.1, QUIET_MS);
        assert!(o.shown());
    }

    const NOTES_V3: &str = "# Update notes\n\nWhat each build changed.\n\n## 2026-10-25 · Third\n- c1 **bold**\n  carried on `code`\n- c2\n\n## 2026-10-24 · Second\n- b1\n\n## 2026-10-23 · First\n- a1\n";
    const NOTES_V1: &str = "# Update notes\n\n## 2026-10-23 · First\n- a1\n";

    /// The sections as the file has them: the headings, the bullets with
    /// their wrapped lines joined, the markdown out, the preface passed over.
    #[test]
    fn the_notes_read_as_sections() {
        let s = sections(NOTES_V3);
        assert_eq!(s.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["2026-10-25 · Third", "2026-10-24 · Second", "2026-10-23 · First"]);
        assert_eq!(s[0].bullets, ["c1 bold carried on code", "c2"]);
        assert_eq!(sections("a preface\n- and no heading"), Vec::<Section>::new(), "the control: no heading, no section");
    }

    /// What is new: the release's sections this build lacks, newest first,
    /// several when builds were skipped; none of them new is Nothing.
    #[test]
    fn whats_new_is_what_this_build_lacks() {
        let WhatsNew::Sections(s) = whats_new(NOTES_V3, NOTES_V1) else { panic!("sections") };
        assert_eq!(s.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["2026-10-25 · Third", "2026-10-24 · Second"], "two builds skipped: both");
        let v2 = "## 2026-10-24 · Second\n- b1\n\n## 2026-10-23 · First\n- a1\n";
        let WhatsNew::Sections(s) = whats_new(NOTES_V3, v2) else { panic!("sections") };
        assert_eq!(s.len(), 1, "one build behind: the newest alone");
        assert_eq!(whats_new(NOTES_V1, NOTES_V1), WhatsNew::Nothing, "the control: nothing new");
        assert_eq!(whats_new("", NOTES_V1), WhatsNew::Nothing, "an empty file: nothing");
    }

    /// This build's own notes (UPDATE-NOTES.md, built in) read as dated
    /// sections, newest first, each with bullets and no markdown left.
    #[test]
    fn this_builds_own_notes_read() {
        let s = sections(OWN_NOTES);
        assert!(s.len() >= 5, "{}", s.len());
        assert!(s.iter().all(|s| !s.bullets.is_empty() && s.title.starts_with("20")));
        assert!(s.windows(2).all(|w| w[0].title[..10] >= w[1].title[..10]), "newest first");
        assert!(s.iter().flat_map(|s| &s.bullets).all(|b| !b.contains("**") && !b.contains('`')));
    }

    /// The notes come from beside update.txt; a release without them is
    /// an error (the question then says where they are).
    #[test]
    fn the_notes_come_from_beside_the_manifest() {
        use super::native::fetch_notes;
        let base = serve(vec![(NOTES_FILE, NOTES_V3.as_bytes().to_vec())]);
        assert_eq!(fetch_notes(&base).as_deref(), Ok(NOTES_V3));
        let none = serve(vec![]);
        assert!(fetch_notes(&none).is_err(), "the control: none there");
    }
}
