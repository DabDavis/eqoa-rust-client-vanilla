//! SETUP (`setup_main`, `VISetup`, SETUP `visetup.cpp`): the legal screen,
//! the licence and the title, as `VISetup::Process` runs them a frame at a
//! time (30 a second), and the windows the title opens.
//!
//! `Process`'s order: the window manager hands the keys to the window with
//! the focus and draws the root window and its children (`VIWnd::OnDraw`:
//! itself, then each visible child), then the fade (`VIFader::Update`) over
//! everything. A window gives its focus back (`SetFocus(0)`) when it is
//! done, and `Process` moves on from there by `m_state`: the legal screen
//! (state 0) to the licence (state 2), the licence to the intro movie and
//! the title (state 3), and the title's Play to `RunPatcher`.
//!
//! Play runs the patcher (`RunPatcher`'s patch half, `patcher`, in its
//! window, `patchwnd`) against the server Network Setup keeps, into that
//! server's own folder (`servers`), then fades out to STATION; a patch server
//! that does not answer plays the folder as it stands (a notice says so when
//! it has a patch). Natively: a page cannot reach the patch port, and a run
//! that keeps nothing (the test commands) patches nothing.
//!
//! Left out, each where it would be: the controller screen (no analog pad:
//! the keyboard is a computer's), the language screen (feature flag 0x400,
//! which SLUS does not set: it sets 0x300), the intro movie (`movies\eqoa.pss`:
//! the user's "later"), the memory card's checks (MCERR), and Play's
//! network steps before the patch: the card's network configurations
//! (`VIWndConnect`) and DNAS. The
//! title's Network Setup is the client's own page (`netsetup`), and its
//! Options window is the client's `options::WndOptions`, which is the same
//! code as SETUP's (CLIENT's `viwndoptions.cpp` differs only where it
//! closes: `ApplyOptions` there, the focus back to the title here).
//!
//! Back from STATION (its Disconnect) SLUS starts over, so `start` begins
//! again at the legal screen.

pub mod eula;
pub mod fader;
pub mod legal;
pub mod netsetup;
pub mod patchwnd;
pub mod splash;
pub mod splashfx;

use super::{Ctx, Music, Next, Shared, res::Res};
use crate::options::{GameOptions, OptionsUp, WndOptions};
use crate::textlist::{Message, PROMPT_OK};
use vi::ui::wide;

/// What `VISetup::Init` reads after the base files (UI.CSF, ARIALUI.CSF):
/// SETUP.CSF (the painting, the splash effects' atlases, the title's music),
/// ADDART.CSF, and the licence (`VIWndEula::Init`); and the patch's manifest
/// path (`RunPatcher`'s `data\psvrpath.dat`).
pub const FILES: &[&str] = &["DATA2/SETUP.CSF", "DATA2/ADDART.CSF", "DATA/EULA.TXT", "DATA/PSVRPATH.DAT"];

/// The client's words for Play with no server (the game's for a missing
/// network configuration are 783 and 984).
const NO_SERVER: &str = "No server is set. Select Network Setup to enter the address of the server to play on.";

/// The window manager's root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Root {
    Legal,
    Eula,
    Title,
}

/// The window with the focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Legal,
    Eula,
    Title,
    Options,
    NetSetup,
    /// the title's message box (Play with no server)
    Message,
    /// the patch's window (`RunPatcher`)
    Patcher,
}

pub struct Setup {
    /// `m_state`: 0 the intro screens, 2 the movie, 3 the title (4: done,
    /// `setup_main` returned)
    pub state: i32,
    pub root: Root,
    /// `mFocus`; None: given back, which `Process` moves on from
    pub focus: Option<Focus>,
    legal: legal::Legal,
    eula: eula::Eula,
    splash: splash::Splash,
    fx: splashfx::SplashFx,
    fader: fader::Fader,
    /// `VIGetGameOptions()`, as `Init(1)` loads it
    pub options: GameOptions,
    options_wnd: WndOptions,
    options_visible: bool,
    /// the Options asked for from outside the game's keys (`ask_options`)
    options_asked: bool,
    message: Message,
    net: netsetup::NetSetup,
    net_visible: bool,
    /// the patch running or run (`RunPatcher`): its status, and the window
    patch: Option<crate::patcher::Shared>,
    pub patch_wnd: patchwnd::PatchWnd,
    /// the patch's end has been read
    patch_ended: bool,
    /// how a patch starts (`link::patch`; a test's own)
    patch_start: fn(&str, &str) -> Option<crate::patcher::Shared>,
    /// the keys since the last frame, for the window with the focus
    keys: Vec<(i32, bool)>,
    /// the Controls page's capture, for the next frame
    captures: Vec<Capture>,
    /// what the sound device is asked besides cues and music: the classes'
    /// volumes and the output mode (`take_sound`)
    sound: Vec<OptionsUp>,
    /// `Init`'s half that needs the clock has run
    began: bool,
    /// the title's music is playing (`m_ambient`)
    ambient: bool,
    /// Play: `RunPatcher`'s closing fade under way
    leaving: bool,
    /// the last key came from a keyboard (`gKeyboardAttached`, for the
    /// edit box's face)
    keyboard: bool,
    /// the edit box's face was set by this window
    face_set: bool,
    now: i64,
}

enum Capture {
    Key(String),
    Button(u8),
}

impl Default for Setup {
    fn default() -> Self {
        Self::new()
    }
}

impl Setup {
    pub fn new() -> Self {
        Setup {
            state: 0,
            root: Root::Legal,
            focus: Some(Focus::Legal),
            legal: Default::default(),
            eula: Default::default(),
            splash: Default::default(),
            fx: splashfx::SplashFx::new(net::session::EXPANSION_FLAGS),
            fader: Default::default(),
            options: GameOptions::default(),
            options_wnd: Default::default(),
            options_visible: false,
            options_asked: false,
            message: Default::default(),
            net: Default::default(),
            net_visible: false,
            patch: None,
            patch_wnd: Default::default(),
            patch_ended: false,
            patch_start: crate::pregame::link::patch,
            keys: Vec::new(),
            captures: Vec::new(),
            sound: Vec::new(),
            began: false,
            ambient: false,
            leaving: false,
            keyboard: true,
            face_set: false,
            now: 0,
        }
    }

    /// `VISetup::Init(1)`: the options loaded, the windows made, the legal
    /// screen the root with the focus. Its clock's half (the fade, the
    /// focus's notice) is the first frame's.
    pub fn start(&mut self, res: &mut Res, _shared: &mut Shared) {
        self.options = crate::settings::game_options();
        self.eula.init(res.raw("DATA/EULA.TXT").unwrap_or_default());
        self.splash.init();
        self.state = 0;
        self.root = Root::Legal;
        self.focus = Some(Focus::Legal);
        self.options_visible = false;
        self.message.visible = false;
        self.net_visible = false;
        self.patch = None;
        self.patch_ended = false;
        self.keys.clear();
        self.began = false;
        self.leaving = false;
    }

    /// The art swapped (F9, `era`): the title's effects as that disc's
    /// SETUP draws them (`splashfx::flags`): the 2002 disc's painting with
    /// the original game's birds, Frontiers' with its island layer.
    pub fn art_changed(&mut self, era: esf::Era) {
        self.fx = splashfx::SplashFx::new(splashfx::flags(era));
    }

    /// A key the game reads, for the window with the focus at the next frame.
    pub fn key(&mut self, key: i32, down: bool) {
        self.keys.push((key, down));
    }

    /// A pointer at frame point `p`: the keys the window with the focus asks for.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        match self.focus {
            Some(Focus::Legal) => self.legal.point(p),
            Some(Focus::Eula) => self.eula.point(p),
            Some(Focus::Title) => self.splash.point(p, self.now),
            Some(Focus::Options) => self.options_wnd.point(p),
            Some(Focus::NetSetup) => self.net.point(p),
            Some(Focus::Message) => self.message.point(p).into_iter().collect(),
            Some(Focus::Patcher) => self.patch_wnd.point(p),
            None => Vec::new(),
        }
    }

    /// An edit box has the keyboard (Network Setup's).
    pub fn typing(&self) -> bool {
        self.net_visible && self.net.editing
    }

    /// What the Options window's Controls page waits for, if anything: the
    /// window then wants the next key's name (`capture_key`) or button's
    /// bit (`capture_button`), as play's does.
    pub fn capturing(&self) -> Option<crate::bindpage::Column> {
        if self.focus == Some(Focus::Options) { self.options_wnd.capturing() } else { None }
    }

    pub fn capture_key(&mut self, name: &str) {
        self.captures.push(Capture::Key(name.to_string()));
    }

    pub fn capture_button(&mut self, phys: u8) {
        self.captures.push(Capture::Button(phys));
    }

    /// The sound device's requests besides cues and music, as play's
    /// Options window makes them: the classes' volumes (the title's at its
    /// start, and the Sound page's), the output mode.
    pub fn take_sound(&mut self) -> Vec<OptionsUp> {
        std::mem::take(&mut self.sound)
    }

    /// `Init`'s clocked half: the fade from black over half a second, the
    /// splash effects' reset, the focus's notice to the legal screen.
    fn begin(&mut self, now: i64) {
        self.fader.init(1.0, now);
        self.fader.begin(0.0, 0.5);
        self.fx.reset(now);
        self.legal.focus(now);
        self.began = true;
    }

    /// The key for the window with the focus (`VIWindowManager::Update`).
    fn dispatch(&mut self, cx: &mut Ctx, k: i32, down: bool) {
        if down {
            self.keyboard = k < 128;
        }
        let now = cx.now;
        let mut cues = Vec::new();
        match self.focus {
            Some(Focus::Legal) if down => self.legal.key(k, now),
            Some(Focus::Eula) => {
                if !down {
                    self.eula.key_up();
                } else if self.eula.key(cx.vui, cx.r, k, now, &mut cues) {
                    self.focus = None;
                }
            }
            Some(Focus::Title) if down => match self.splash.key(k, now, &mut cues) {
                Some(splash::Pick::Play) => self.connect(cx),
                Some(splash::Pick::NetworkSetup) => {
                    self.net.open(cx.vui, cx.r, cx.shared.server.as_deref());
                    self.net_visible = true;
                    self.focus = Some(Focus::NetSetup);
                }
                Some(splash::Pick::Options) => {
                    self.options_wnd.open(cx.vui, cx.r, &self.options);
                    self.options_visible = true;
                    self.focus = Some(Focus::Options);
                }
                None => {}
            },
            Some(Focus::Options) => {
                if !down {
                    self.options_wnd.release(k);
                } else {
                    let mut ups = Vec::new();
                    self.options_wnd.key(cx.vui, cx.r, &mut self.options, k, now, &mut cues, &mut ups);
                    self.options_up(cx, ups);
                }
            }
            Some(Focus::NetSetup) => {
                if !down {
                    self.net.key_up(k);
                } else {
                    match self.net.key(cx.vui, cx.r, k, now, &mut cues) {
                        netsetup::NetUp::Close => {
                            self.net_visible = false;
                            self.focus = Some(Focus::Title);
                        }
                        // Done is the page's submit: the address kept, and
                        // back to the title (F3 20261026-202044: only
                        // Triangle left the page)
                        netsetup::NetUp::Kept(addr) => {
                            if let Err(e) = crate::settings::set_server(addr.as_deref().unwrap_or("")) {
                                crate::say(&format!("the server's address was not kept: {e}"));
                            }
                            cx.shared.server = addr;
                            self.net_visible = false;
                            self.focus = Some(Focus::Title);
                        }
                        netsetup::NetUp::None => {}
                    }
                }
            }
            // 307: the box closed, the focus back to the title
            Some(Focus::Message) if down && self.message.key(k, now, &mut cues).is_some() => {
                self.focus = Some(Focus::Title);
            }
            Some(Focus::Patcher) if down => match self.patch_wnd.key(cx.vui, cx.r, k, now, &mut cues) {
                Some(patchwnd::Up::Play) => self.go_on(cx),
                Some(patchwnd::Up::Restart | patchwnd::Up::Cancelled) => self.restart_connect(cx),
                None => {}
            },
            _ => {}
        }
        cx.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
    }

    /// The Options window's requests: the sound's, its battle music's
    /// preview (BGM slot 0, as `VIBattleMusic` plays it), and its close
    /// (`SetVisible(false)`, the focus back to the title).
    fn options_up(&mut self, cx: &mut Ctx, ups: Vec<OptionsUp>) {
        for up in ups {
            match up {
                OptionsUp::Volume(..) | OptionsUp::Output(_) => self.sound.push(up),
                OptionsUp::Preview(t) if t > 0 => cx.music.push(Music::Bgm { slot: 0, path: format!("bgm\\battle{t:02}.bgm") }),
                OptionsUp::Preview(_) | OptionsUp::PreviewEnd => cx.music.push(Music::StopBgm { slot: 0 }),
                OptionsUp::Apply => {
                    self.options_visible = false;
                    self.focus = Some(Focus::Title);
                    cx.music.push(Music::StopBgm { slot: 0 });
                }
            }
        }
    }

    /// `VIWndSplash::Connect`, the PS2's steps left out (the card, its
    /// network configurations): with no server the message box says to set
    /// one; with one the title gives the focus up (`Process` then runs
    /// `RunPatcher`).
    fn connect(&mut self, cx: &mut Ctx) {
        if cx.shared.server.is_none() {
            let title = wide(cx.vui.words.get(767, cx.vui.language));
            self.message.setup(cx.vui, cx.r, &title, &wide(NO_SERVER), PROMPT_OK, [320, 246], 400);
            self.focus = Some(Focus::Message);
        } else {
            self.focus = None;
        }
    }

    /// `StartAmbientSoundtrack`: SETUP.CSF's `splash_screen`, looped.
    fn start_music(&mut self, cx: &mut Ctx) {
        self.stop_music(cx);
        cx.music.push(Music::Xm { tag: "title", id: vi::hash::hash_resource_id("splash_screen"), repeat: true, volume: 1.0 });
        self.ambient = true;
    }

    /// `StopAmbientSoundtrack`.
    fn stop_music(&mut self, cx: &mut Ctx) {
        if self.ambient {
            cx.music.push(Music::Stop { tag: "title" });
            self.ambient = false;
        }
    }

    /// One frame of `VISetup::Process`.
    /// macOS's Settings… (⌘,): the title's Options at the next frame, if the
    /// title's menu has the keys then.
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    pub fn ask_options(&mut self) {
        self.options_asked = true;
    }

    pub fn frame(&mut self, cx: &mut Ctx) -> Next {
        self.now = cx.now;
        if !self.began {
            self.begin(cx.now);
            self.focus_arrived(cx);
        }
        for (k, down) in std::mem::take(&mut self.keys) {
            self.dispatch(cx, k, down);
        }
        // macOS's Settings… (⌘,): the title's Options, as its menu's Options
        // opens them, when the title's menu has the keys; asked anywhere else
        // (the licence, Network Setup, the patch) it is let go
        if std::mem::take(&mut self.options_asked) && self.focus == Some(Focus::Title) {
            self.options_wnd.open(cx.vui, cx.r, &self.options);
            self.options_visible = true;
            self.focus = Some(Focus::Options);
        }
        for c in std::mem::take(&mut self.captures) {
            let mut cues = Vec::new();
            match c {
                Capture::Key(name) => self.options_wnd.capture_key(cx.vui, &name, &mut cues),
                Capture::Button(b) => self.options_wnd.capture_button(cx.vui, b, &mut cues),
            }
            cx.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        }
        self.edit_face();
        // the root and its children (`VIWindowManager::Update`, `VIWnd::OnDraw`)
        let mut cues = Vec::new();
        match self.root {
            Root::Legal => {
                if self.legal.draw(cx.vui, cx.r, cx.now) && self.focus == Some(Focus::Legal) {
                    self.focus = None;
                }
            }
            Root::Eula => self.eula.draw(cx.vui, cx.r, cx.now, &mut cues),
            Root::Title => {
                let focused = self.focus == Some(Focus::Title);
                if self.splash.draw(cx.vui, cx.r, &mut self.fx, cx.now, focused) {
                    // the card's check passed: the classes' volumes
                    self.sound.push(OptionsUp::Volume(0, self.options.fx_volume));
                    self.sound.push(OptionsUp::Volume(1, self.options.music_volume));
                }
                if self.options_visible {
                    let mut ups = Vec::new();
                    self.options_wnd.draw(cx.vui, cx.r, &self.options, &mut ups);
                    cues.extend(self.options_wnd.take_cues());
                    self.options_up(cx, ups);
                }
                if self.message.visible {
                    self.message.draw(cx.vui, cx.r);
                }
                if self.net_visible {
                    self.net.draw(cx.vui, cx.r, cx.now, &mut cues);
                }
                if self.patch.is_some() {
                    self.follow_patch(cx, &mut cues);
                    self.patch_wnd.draw(cx.vui, cx.r, &mut cues);
                }
            }
        }
        cx.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        self.fader.update(cx.r, cx.now);
        if self.leaving {
            // `RunPatcher`'s end: the fade out, then on (`Free` stops the music)
            if self.fader.alpha() == 1.0 {
                // SETUP is done (`setup_main` returns): nothing moves on
                // again until `start`
                self.leaving = false;
                self.state = 4;
                self.stop_music(cx);
                self.restore_face();
                return Next::Station;
            }
            return Next::Stay;
        }
        if self.focus.is_none() {
            self.moved_on(cx);
        }
        Next::Stay
    }

    /// `Process` after a window gave the focus up.
    fn moved_on(&mut self, cx: &mut Ctx) {
        let state = self.state;
        match state {
            0 => {
                // the legal screen done: the licence
                self.state = 2;
                self.root = Root::Eula;
                self.focus = Some(Focus::Eula);
                self.focus_arrived(cx);
            }
            2 => {
                // the licence accepted. Here the game plays the intro movie
                // (`VIMovieStartup(640, 448)`, `VIMoviePlay("movies\\eqoa.pss")`,
                // stoppable with START, X or Escape): left out, the user's
                // "later". Then `Free` and `Init(0)`: the fade from black,
                // the effects and the title from the top.
                self.fader.init(1.0, cx.now);
                self.fader.begin(0.0, 0.5);
                self.fx.reset(cx.now);
                self.splash.init();
                self.state = 3;
                self.fader.begin(1.0, 0.0);
                self.root = Root::Title;
                self.focus = Some(Focus::Title);
                self.focus_arrived(cx);
                self.fader.begin(0.0, 0.5);
                self.start_music(cx);
            }
            // `m_splash.Selection()`: Play gives the focus up (Connect);
            // `RunPatcher` follows, or (nothing to patch with) its closing
            // fade. (Network Setup's fade out to Sony's utility is the
            // client's page here.)
            3 if self.splash.sel == 0 && !self.leaving && self.patch.is_none() && !self.run_patcher(cx) => {
                self.fader.begin(1.0, 0.5);
                self.leaving = true;
            }
            _ => {}
        }
    }

    /// `RunPatcher`: the patch started on its thread, its window with the
    /// focus. False when there is none to run (no server, a page, a run
    /// that keeps nothing).
    fn run_patcher(&mut self, cx: &mut Ctx) -> bool {
        let Some(host) = cx.shared.server.clone() else { return false };
        let manifest = psvrpath(cx.res);
        let Some(st) = (self.patch_start)(&host, &manifest) else { return false };
        self.patch = Some(st);
        self.patch_ended = false;
        self.patch_wnd.open(cx.vui, cx.r);
        self.focus = Some(Focus::Patcher);
        true
    }

    /// The patch's status into its window each frame; at its end the notes
    /// (opened by themselves when files changed), or, the patch server not
    /// answering, the notice (a folder with a patch) or straight on (none).
    fn follow_patch(&mut self, cx: &mut Ctx, cues: &mut Vec<i32>) {
        let Some(st) = self.patch.as_ref() else { return };
        if self.patch_ended {
            return;
        }
        let s = st.lock().unwrap().clone();
        self.patch_wnd.state = s.state;
        self.patch_wnd.progress = s.progress;
        if !s.done {
            return;
        }
        self.patch_ended = true;
        let host = cx.shared.server.clone().unwrap_or_default();
        if s.unreachable {
            match crate::servers::record(&host) {
                Some(r) => {
                    let text = format!("The update server did not answer. Playing with the files {host} sent on {} UTC.", crate::servers::stamp(r.when));
                    self.patch_wnd.notice(cx.vui, cx.r, &text);
                }
                None => {
                    crate::say(&format!("patcher: {host}'s patch server did not answer, and its folder has no patch: playing as the disc is"));
                    self.go_on(cx);
                }
            }
        } else if s.state == crate::patcher::state::DONE {
            if !s.notes.is_empty() {
                self.patch_wnd.append_message(&wide(&s.notes));
            }
            if s.changed {
                self.patch_wnd.show_patch_message(cx.vui, cx.r, cues);
            }
        }
    }

    /// The patch window's Play (or the notice's OK): `RunPatcher`'s closing
    /// fade, then STATION.
    fn go_on(&mut self, cx: &mut Ctx) {
        let _ = cx;
        self.focus = None;
        self.fader.begin(1.0, 0.5);
        self.leaving = true;
    }

    /// `RestartConnect`: back to the title, the patch window put away (a
    /// failed patch's Restart, which reboots a PS2; the cancel box's Yes).
    fn restart_connect(&mut self, cx: &mut Ctx) {
        self.patch = None;
        self.patch_ended = false;
        self.patch_wnd = Default::default();
        self.focus = Some(Focus::Title);
        self.focus_arrived(cx);
    }

    /// `OnMessage` 295 to the window the focus came to from outside it.
    fn focus_arrived(&mut self, cx: &mut Ctx) {
        match self.focus {
            Some(Focus::Legal) => self.legal.focus(cx.now),
            Some(Focus::Eula) => self.eula.focus(cx.vui, cx.r),
            Some(Focus::Title) => self.splash.focus(cx.vui, cx.r),
            _ => {}
        }
    }

    /// The edit box's face (`sPadOnly`, which `VIWndEdit::OnDrawSelf` sets
    /// each frame from `gKeyboardAttached` and the options' Soft Keyboard):
    /// set while Network Setup's box is open, the client's default
    /// (compact) put back after.
    fn edit_face(&mut self) {
        if self.typing() {
            crate::editbox::set_pad_only(self.keyboard && self.options.compact != 0);
            self.face_set = true;
        } else {
            self.restore_face();
        }
    }

    fn restore_face(&mut self) {
        if self.face_set {
            crate::editbox::set_pad_only(true);
            self.face_set = false;
        }
    }
}

/// `data\psvrpath.dat`'s first line (up to a CR, an LF or a space; at most
/// 255 bytes), the retail path when the disc's is not there.
pub fn psvrpath(res: &Res) -> String {
    match res.raw("DATA/PSVRPATH.DAT") {
        Some(b) => {
            let b = &b[..b.len().min(255)];
            let end = b.iter().position(|&c| matches!(c, b'\r' | b'\n' | b' ' | 0)).unwrap_or(b.len());
            String::from_utf8_lossy(&b[..end]).into_owned()
        }
        None => crate::patcher::MANIFEST.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pointer::key;
    use crate::pointer::testing::{NullRaster, vui};

    struct Run {
        s: Setup,
        res: Res,
        shared: Shared,
        vui: vi::ui::Vui,
        cues: Vec<crate::sound::Cue>,
        music: Vec<Music>,
        now: i64,
    }

    impl Run {
        fn new(server: Option<&str>) -> Run {
            crate::settings::testing::reset(false);
            let mut res = Res::new();
            res.put("DATA/EULA.TXT", (0..40).map(|k| format!("term {k}\n")).collect::<String>().into_bytes());
            let mut shared = Shared { server: server.map(str::to_string), login: None };
            let mut s = Setup::new();
            s.start(&mut res, &mut shared);
            Run { s, res, shared, vui: vui(), cues: Vec::new(), music: Vec::new(), now: 0 }
        }

        /// One of SETUP's frames, 33 ms on.
        fn frame(&mut self) -> Next {
            self.now += 33;
            let mut r = NullRaster;
            let mut cx = Ctx { now: self.now, game_frame: true, vui: &self.vui, r: &mut r, res: &self.res, shared: &mut self.shared, cues: &mut self.cues, music: &mut self.music, gpu: None };
            self.s.frame(&mut cx)
        }

        fn frames(&mut self, n: usize) -> Vec<Next> {
            (0..n).map(|_| self.frame()).filter(|n| !matches!(n, Next::Stay)).collect()
        }

        fn press(&mut self, k: i32) {
            self.s.key(k, true);
            self.frame();
            self.s.key(k, false);
            self.frame();
        }
    }

    /// The order: the legal screen 7.5 s, the licence until accepted, then
    /// the title with its music; Play with a server fades out and goes on.
    #[test]
    fn legal_then_the_licence_then_the_title_then_play() {
        let mut run = Run::new(Some("192.0.2.1"));
        run.frames(200);
        assert_eq!(run.s.root, Root::Legal, "6.6 s in: still the legal screen");
        run.frames(40);
        assert_eq!((run.s.root, run.s.state), (Root::Eula, 2), "past 7.5 s: the licence");
        run.press(13);
        assert_eq!(run.s.root, Root::Eula, "declining keeps the licence");
        run.press(13);
        run.press(18);
        run.press(13);
        assert_eq!((run.s.root, run.s.state), (Root::Title, 3), "accepted: the title (the movie left out)");
        assert!(run.music.iter().any(|m| matches!(m, Music::Xm { tag: "title", repeat: true, .. })), "its music");
        run.frames(30);
        assert!(run.s.take_sound().iter().any(|u| matches!(u, OptionsUp::Volume(0, _))), "ready: the classes' volumes");
        run.press(key::CROSS);
        let after = run.frames(30);
        assert!(matches!(after.as_slice(), [Next::Station]), "a half-second fade, then Station");
        assert!(run.music.contains(&Music::Stop { tag: "title" }), "the music stopped");
    }

    /// Play with no server says so and stays; Network Setup keeps one and
    /// Play then goes on.
    #[test]
    fn play_needs_a_server_and_network_setup_keeps_one() {
        let mut run = Run::new(None);
        run.s.state = 2;
        run.s.focus = None;
        run.frames(32);
        assert_eq!(run.s.root, Root::Title);
        run.press(13);
        assert_eq!(run.s.focus, Some(Focus::Message), "no server: the box");
        assert!(run.frames(30).is_empty(), "and it stays");
        run.press(13);
        assert_eq!(run.s.focus, Some(Focus::Title), "the box closed");
        run.press(18);
        run.press(13);
        assert_eq!(run.s.focus, Some(Focus::NetSetup));
        run.press(13);
        assert!(run.s.typing(), "X on the row: the edit box");
        for ch in "eqoa.example.com".chars() {
            run.press(ch as i32);
        }
        run.press(13);
        assert_eq!(run.shared.server.as_deref(), Some("eqoa.example.com"));
        assert_eq!(crate::settings::server().as_deref(), Some("eqoa.example.com"), "kept in server.opt");
        assert_eq!(run.s.focus, Some(Focus::Title), "Done: back at the title");
        run.press(17);
        run.press(13);
        assert!(matches!(run.frames(30).as_slice(), [Next::Station]));
        crate::settings::testing::reset(false);
    }

    /// Done in Network Setup's edit box is the page's submit (F3
    /// 20261026-202044: "there isn't a proper submission. You need to press
    /// triangle"): the address kept (server.opt and the session's) and the
    /// title back with the keys, a blank keeping none the same way. The
    /// controls: a refused address keeps the page, its message box up and
    /// the address as it was (the box closed, the page still has the keys,
    /// and Triangle goes back); Escape in the edit box cancels it, the page
    /// staying.
    #[test]
    fn done_in_network_setup_keeps_the_address_and_goes_back() {
        let mut run = Run::new(Some("192.0.2.1"));
        run.s.state = 2;
        run.s.focus = None;
        run.frames(32);
        assert_eq!(run.s.focus, Some(Focus::Title));
        run.press(18);
        // the title's Network Setup (its choice stays there), X on its row,
        // the address typed over the old one, then `done`
        let submit = |run: &mut Run, typed: &str, done: i32| {
            run.press(13);
            assert_eq!(run.s.focus, Some(Focus::NetSetup), "the page");
            run.press(13);
            assert!(run.s.typing(), "the edit box");
            for _ in 0..20 {
                run.press(8);
            }
            for ch in typed.chars() {
                run.press(ch as i32);
            }
            run.press(done);
        };
        submit(&mut run, "eqoa.example.com", 13);
        assert_eq!((run.s.focus, run.s.net_visible, run.s.typing()), (Some(Focus::Title), false, false), "Enter: kept, and back at the title");
        assert_eq!(run.shared.server.as_deref(), Some("eqoa.example.com"));
        assert_eq!(crate::settings::server().as_deref(), Some("eqoa.example.com"), "kept in server.opt");

        // refused: the page stays, nothing kept
        submit(&mut run, "300.1.1.1", 13);
        assert_eq!((run.s.focus, run.s.net_visible), (Some(Focus::NetSetup), true), "the control: a refused address keeps the page");
        assert_eq!(run.shared.server.as_deref(), Some("eqoa.example.com"));
        run.press(13);
        assert_eq!(run.s.focus, Some(Focus::NetSetup), "the box closed, the page still up");
        run.press(key::TRIANGLE);
        assert_eq!(run.s.focus, Some(Focus::Title), "Triangle goes back");

        // a blank, by R2 (a click on the legend's Done): none kept, back too
        submit(&mut run, "", key::R2);
        assert_eq!((run.s.focus, run.s.net_visible), (Some(Focus::Title), false));
        assert_eq!((run.shared.server.as_deref(), crate::settings::server()), (None, None));

        // the control: Escape in the edit box cancels, the page staying
        run.press(13);
        run.press(13);
        run.press(b'x' as i32);
        run.press(27);
        assert_eq!((run.s.focus, run.s.typing()), (Some(Focus::NetSetup), false), "the control: cancelled, the page up");
        assert_eq!(run.shared.server, None);
        crate::settings::testing::reset(false);
    }

    thread_local! {
        static STATUS: std::cell::RefCell<Option<crate::patcher::Shared>> = const { std::cell::RefCell::new(None) };
    }

    /// The test's patch: the status the test holds.
    fn started(_: &str, _: &str) -> Option<crate::patcher::Shared> {
        STATUS.with(|s| s.borrow().clone())
    }

    fn set(st: &crate::patcher::Shared, f: impl FnOnce(&mut crate::patcher::Status)) {
        f(&mut st.lock().unwrap());
    }

    /// Play runs the patch in its window: its status followed, the notes
    /// opened by themselves when files changed, Done's Play then on to
    /// STATION; a failure's Restart goes back to the title (the control: no
    /// STATION), and a patch server that does not answer, with nothing kept,
    /// goes straight on.
    #[test]
    fn play_runs_the_patch_in_its_window() {
        use crate::patcher::state;
        let st: crate::patcher::Shared = Default::default();
        STATUS.with(|s| *s.borrow_mut() = Some(st.clone()));
        let title = |run: &mut Run| {
            run.s.patch_start = started;
            run.s.state = 2;
            run.s.focus = None;
            run.frames(32);
        };
        let mut run = Run::new(Some("192.0.2.1"));
        title(&mut run);
        set(&st, |s| s.state = state::CHECKING);
        run.press(13);
        assert_eq!(run.s.focus, Some(Focus::Patcher), "Play: the patch's window");
        run.frames(3);
        assert_eq!(run.s.patch_wnd.state, state::CHECKING, "its status followed");
        assert!(run.frames(30).is_empty(), "nothing goes on while it runs");
        set(&st, |s| {
            s.state = state::DONE;
            s.progress = 1.0;
            s.notes = "New this week.".into();
            s.changed = true;
            s.done = true;
        });
        run.frames(2);
        assert_eq!(run.s.patch_wnd.notes.text, wide("New this week."), "the notes in");
        run.press(key::TRIANGLE);
        assert_eq!(run.s.patch_wnd.sel, 1, "they opened by themselves, Play behind them");
        run.press(key::CROSS);
        assert!(matches!(run.frames(30).as_slice(), [Next::Station]), "Play: the fade, then STATION");

        // a failure: Restart is the title again
        set(&st, |s| *s = crate::patcher::Status { state: state::FAILED, done: true, ..Default::default() });
        let mut run = Run::new(Some("192.0.2.1"));
        title(&mut run);
        run.press(13);
        run.frames(2);
        run.press(key::CROSS);
        assert_eq!(run.s.focus, Some(Focus::Title), "Restart: the title");
        assert!(run.frames(30).is_empty(), "the control: no STATION");

        // a patch server that never answered, nothing kept: straight on
        set(&st, |s| *s = crate::patcher::Status { state: state::CONNECTING, done: true, unreachable: true, ..Default::default() });
        let mut run = Run::new(Some("192.0.2.1"));
        title(&mut run);
        run.press(13);
        assert!(matches!(run.frames(30).as_slice(), [Next::Station]));
        crate::settings::testing::reset(false);
    }

    /// Options opens over the title with the client's window, and Triangle
    /// on its menu gives the title its focus back.
    /// macOS's Settings… opens the title's Options when the title's menu has
    /// the keys, as its Options entry does; the control: asked while the
    /// licence has them, nothing opens, and the asking is not kept for later.
    #[test]
    fn settings_from_outside_open_the_titles_options() {
        let mut run = Run::new(Some("192.0.2.1"));
        run.frames(5);
        assert_ne!(run.s.focus, Some(Focus::Title), "the legal screen first");
        run.s.ask_options();
        run.frame();
        assert!(!run.s.options_visible, "not over the legal screen");
        run.s.state = 2;
        run.s.focus = None;
        run.frames(32);
        assert_eq!(run.s.focus, Some(Focus::Title));
        assert!(!run.s.options_visible, "the earlier asking was let go");
        run.s.ask_options();
        run.frame();
        assert_eq!(run.s.focus, Some(Focus::Options));
        assert!(run.s.options_visible);
    }

    #[test]
    fn options_opens_and_triangle_comes_back() {
        let mut run = Run::new(Some("192.0.2.1"));
        run.s.state = 2;
        run.s.focus = None;
        run.frames(32);
        run.press(18);
        run.press(18);
        run.press(13);
        assert_eq!(run.s.focus, Some(Focus::Options));
        run.press(key::TRIANGLE);
        assert_eq!(run.s.focus, Some(Focus::Title));
        // the control: the title's keys do not reach it while Options is up
        run.press(18);
        run.press(13);
        run.press(17);
        assert_eq!(run.s.focus, Some(Focus::Options));
        assert_eq!(run.s.splash.sel, 2, "the title's choice did not move under Options");
    }
}
