//! The pregame: what the PS2 shows from the disc's boot to the world, in the
//! order SLUS runs its scenes (`_root`, `claude-notes/loadexec-scene-probe-20260811.md`):
//!
//! 1. **the launcher's loading screens** (`boot`): `VIDisplayLoadingScreen`'s
//!    pictures while the modules load (here: while SUPPORT's image and the UI
//!    files come);
//! 2. **SETUP** (`setup`): `setup_main` / `VISetup::Process`, the legal
//!    screen, the licence, the title (Play, Network Setup, Options);
//! 3. **STATION** (`station`): `station_main` / `VIStation`, STATION.PAK's
//!    pages (the Station login, Play EverQuest, the news) through
//!    `VIWndGenericRenderer`;
//! 4. **CLIENT** (`client`): `client_main`'s windows before the world, the
//!    server list, character select and creation; then the world entry,
//!    which `Play` takes from there (`Next::World`).
//!
//! Back the other way as SLUS goes: STATION's Disconnect brings the title
//! back (`station_main` returns 1 and SLUS starts over), CLIENT's Back on the
//! server list brings STATION's login back (`client_main` returns 0).
//!
//! What the PS2 does here that a computer does not need is left out, each
//! step where it would be: the memory card's checks (MCERR, `VIWndInitFromMc`),
//! the network adaptor's configuration (`VIWndConnect`) and DNAS. The patcher
//! runs after the title's Play (`setup`, `patcher`), into the server's own
//! folder (`servers`).
//! The title's Network Setup, which on the console starts Sony's network
//! utility (NTGUI2.ELF), is where the player enters the server's address
//! (the client's own page, the user's choice).
//!
//! The windows are the decomp's, ported as the start menu's are: each draws
//! through `vi::ui` (`Vui`, `Window`) onto the 2D layer, takes the game's key
//! codes (the pad's 128.. in `VIConsolePad`'s order, the keyboard's own:
//! 13 Enter, 27 Escape, 8 Backspace, 17-20 the cursor keys, a character as
//! itself) and plays its sounds as `Cue`s. What the game counts in frames
//! steps on `frames.rs`'s clock (`Ctx::game_frame`).

pub mod boot;
pub mod figure;
pub mod client;
pub mod files;
pub mod host;
pub mod link;
pub mod res;
pub mod setup;
pub mod stage;
pub mod station;
pub mod wide;

use std::collections::VecDeque;

/// Where the pregame stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// the launcher's loading screens
    Boot,
    Setup,
    Station,
    Client,
}

/// What a stage tells the pregame after its frame.
pub enum Next {
    Stay,
    /// back to the title (STATION's Disconnect)
    Setup,
    /// on to STATION (the title's Play), or back to it (CLIENT's Back)
    Station,
    /// on to CLIENT, logged in (`Shared::login`)
    Client,
    /// in the world: `Play` takes it from here
    World(Box<Entered>),
}

/// The world entry, as `VIClient` has it after the MemoryDump.
pub struct Entered {
    pub session: net::session::Session,
    pub player: net::session::Player,
    pub dump: Box<net::dump::MemoryDump>,
}

/// The Station login's result, as `station_main` hands it to SLUS and SLUS to
/// `client_main`: the account's name and the password, AES-encrypted under
/// STATION's fixed key (`net::auth::udp_token`), which the game's UDP login
/// sends in its place.
#[derive(Clone, Debug)]
pub struct Login {
    pub user: String,
    pub token: [u8; 32],
}

/// What the stages hand each other.
#[derive(Clone, Default)]
pub struct Shared {
    /// The server Network Setup keeps (`crate::settings::server`): the host
    /// STATION logs in at (TCP 9735), the news comes from (HTTP 7000) and
    /// CLIENT's session starts at (UDP 10070).
    pub server: Option<String>,
    pub login: Option<Login>,
}

/// The ports, as a server answers on them (`EQOA_AUTH_PORT`,
/// `EQOA_GAME_PORT`, `EQOA_NEWS_PORT`: a test server on others).
pub fn port(var: &str, default: u16) -> u16 {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(p) = std::env::var(var).ok().and_then(|p| p.parse().ok()) {
        return p;
    }
    // a page: `?auth_port=`, `?game_port=`... (`web::page_port`)
    #[cfg(target_arch = "wasm32")]
    if let Some(p) = crate::web::page_port(var) {
        return p;
    }
    let _ = var;
    default
}

/// Whether a test server's port was given for `var` (`port`): natively its
/// environment variable, in a page its `?` value.
pub fn port_given(var: &str) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    return std::env::var_os(var).is_some();
    #[cfg(target_arch = "wasm32")]
    return crate::web::page_port(var).is_some();
}

pub const AUTH_PORT: u16 = 9735;
pub const GAME_PORT: u16 = 10070;

/// What a stage's frame has to draw and sound with.
pub struct Ctx<'a> {
    /// ms on the client's clock (`VITimer::GetTime`'s place)
    pub now: i64,
    /// one of the game's frames, 30 a second (`frames.rs`): what the game
    /// counts in frames steps only on these
    pub game_frame: bool,
    pub vui: &'a vi::ui::Vui,
    /// the 2D layer, the 640x448 frame (a wider window shows past its sides)
    pub r: &'a mut dyn crate::ui::Layout,
    /// the files read so far
    pub res: &'a res::Res,
    pub shared: &'a mut Shared,
    /// sounds to play (`VIUI::SoundPlay`'s ids, or names)
    pub cues: &'a mut Vec<crate::sound::Cue>,
    /// music to start and stop
    pub music: &'a mut Vec<Music>,
    /// the GPU, for a window's 3D (character select's, creation's): the
    /// models it uploads and poses itself, drawn as its `scene` says
    pub gpu: Option<crate::play::Gpu<'a>>,
}

/// A window's 3D this frame (character select's preview, creation's model):
/// its models (uploaded and posed by the window through `Ctx::gpu`), the
/// view they are drawn through, and the viewport they fill (frame pixels,
/// inclusive, as `VIRaster::Viewport` takes them). It is drawn over the 2D
/// layer's quads made before the window's `Layout::mark_split`, and under
/// the rest.
pub struct Scene<'a> {
    pub view: render::View,
    pub viewport: [i32; 4],
    pub models: Vec<&'a render::GpuModel>,
}

/// What a stage asks of the sound device besides its cues.
#[derive(Clone, Debug, PartialEq)]
pub enum Music {
    /// an XM out of the files read (`VIStation::PlayMusic`, SETUP's
    /// `StartAmbientSoundtrack`), by its name's DictID, looped or not, at a
    /// volume (0..1); `tag` names it for `Stop`
    Xm { tag: &'static str, id: u32, repeat: bool, volume: f32 },
    Stop { tag: &'static str },
    /// `VISoundDevice::PlayBgm(slot, path)`: a streamed track off the disc
    /// (`bgm\\xxx.bgm`), and `StopBgm(slot)`
    Bgm { slot: i32, path: String },
    StopBgm { slot: i32 },
}

/// The pregame.
pub struct Pregame {
    pub stage: Stage,
    pub res: res::Res,
    pub shared: Shared,
    pub setup: setup::Setup,
    pub station: station::Station,
    pub client: client::Client,
    /// The loading screens: the picture up, and since when.
    pub rotation: boot::Rotation,
    pub picture: Option<(i32, i64)>,
    /// What the boot waits for before the title: the launcher's steps taken.
    boot_step: Option<boot::Step>,
    /// The stage whose files are being read before it starts, and since when.
    entering: Option<Stage>,
    entering_since: i64,
    /// The keyboard's events not yet read: `VIKeyboard::Read` hands the
    /// window manager one a frame.
    keyboard: VecDeque<(i32, bool)>,
    /// A world entry, once.
    entered: Option<Box<Entered>>,
    /// Back from the world (`back`): CLIENT starts by `back_from_world`
    /// with the disconnect's reason (None: the logout), not by `start`.
    back: Option<Option<i32>>,
    /// The release's build is in place (`update`): the window closes for it.
    pub relaunch: bool,
}

impl Pregame {
    /// The pregame from the top: the boot's first picture, its files asked for.
    pub fn new(server: Option<String>) -> Pregame {
        let mut p = Pregame {
            stage: Stage::Boot,
            res: res::Res::new(),
            shared: Shared { server: server.or_else(crate::settings::server), login: None },
            setup: setup::Setup::new(),
            station: station::Station::new(),
            client: client::Client::new(),
            rotation: boot::Rotation::default(),
            picture: None,
            boot_step: None,
            entering: None,
            entering_since: 0,
            keyboard: VecDeque::new(),
            entered: None,
            back: None,
            relaunch: false,
        };
        p.res.want(res::BASE);
        p
    }

    /// Back from the world (`VIClient::Msg0777Done`): the start menu's
    /// logout (`disconnect` None) or the connection lost (its reason). CLIENT
    /// reads its pregame files again (`LoadWorld`, under its own loading
    /// screen), then the server list, or the disconnect's notice first
    /// (`Client::back_from_world`). The account and the server carry over.
    pub fn back(shared: Shared, disconnect: Option<i32>, now: i64) -> Pregame {
        let mut p = Pregame::new(shared.server.clone());
        p.res.set_server_words(shared.server.as_deref().and_then(crate::servers::words));
        p.shared = shared;
        p.back = Some(disconnect);
        p.stage = Stage::Client;
        p.boot_step = Some(boot::Step::Setup);
        p.enter(Stage::Client, now);
        p
    }

    /// The picture `VIDisplayLoadingScreen` shows at this step.
    fn show_picture(&mut self, step: boot::Step, now: i64) {
        let n = self.rotation.show(step.next());
        self.picture = Some((n, now));
        self.boot_step = Some(step);
        self.res.want(&[&boot::path(n)]);
    }

    /// Whether the picture up has been up long enough.
    fn held(&self, now: i64) -> bool {
        self.picture.is_none_or(|(_, at)| now - at >= boot::HOLD_MS)
    }

    /// The loading picture to draw this frame, if one is up (its RGBA is
    /// in `res` once read).
    pub fn loading_picture(&self) -> Option<i32> {
        match self.stage {
            Stage::Boot => self.picture.map(|p| p.0),
            _ if self.entering.is_some() => self.picture.map(|p| p.0),
            _ => None,
        }
    }

    /// A key the game reads: the pad's (`VIPad` through the window manager,
    /// at once) or the keyboard's (one a frame, `frame`).
    pub fn key(&mut self, key: i32, down: bool) {
        if (128..148).contains(&key) {
            self.dispatch(key, down);
        } else {
            self.keyboard.push_back((key, down));
        }
    }

    fn dispatch(&mut self, key: i32, down: bool) {
        if self.entering.is_some() {
            return;
        }
        match self.stage {
            Stage::Boot => {}
            Stage::Setup => self.setup.key(key, down),
            Stage::Station => self.station.key(key, down),
            Stage::Client => self.client.key(key, down),
        }
    }

    /// An edit box has the keys (Network Setup's address, Station's boxes,
    /// creation's name): what a phone's or a tablet's own keyboard types
    /// into, in a page (`web::want_text`).
    pub fn typing(&self) -> bool {
        self.entering.is_none()
            && match self.stage {
                Stage::Boot => false,
                Stage::Setup => self.setup.typing(),
                Stage::Station => self.station.typing(),
                Stage::Client => self.client.typing(),
            }
    }

    /// A pointer's click at frame point `p`: the keys to press, from the
    /// window under it (`pointer`).
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        if self.entering.is_some() {
            return Vec::new();
        }
        match self.stage {
            Stage::Boot => Vec::new(),
            Stage::Setup => self.setup.point(p),
            Stage::Station => self.station.point(p),
            Stage::Client => self.client.point(p),
        }
    }

    /// The files read since the last frame go into `res`; then the stage
    /// moves on if its files are in.
    pub fn update(&mut self, now: i64) {
        self.res.poll();
        if self.stage == Stage::Boot {
            // VILauncher::Init's two calls, _root's, then SETUP's Init
            match self.boot_step {
                None => self.show_picture(boot::Step::Launcher, now),
                Some(boot::Step::Launcher) if self.held(now) => self.show_picture(boot::Step::Util, now),
                Some(boot::Step::Util) if self.held(now) && self.res.support_in() => self.show_picture(boot::Step::Support, now),
                // and the updater (`update`), before the title: a build of
                // the release's put in place closes the window for it
                Some(boot::Step::Support) if self.held(now) && self.res.ready(res::BASE) && crate::update::settled() => {
                    if crate::update::staged() {
                        crate::update::request_relaunch();
                        self.relaunch = true;
                    } else {
                        self.show_picture(boot::Step::Setup, now);
                        self.enter(Stage::Setup, now);
                    }
                }
                _ => {}
            }
        }
        if let Some(s) = self.entering
            && self.res.ready(res::BASE)
            && self.res.ready(self.files_of(s))
            && self.held(now)
            && now - self.entering_since >= boot::HOLD_MS
            && self.res.vui_ready()
        {
            self.entering = None;
            self.picture = None;
            self.stage = s;
            match s {
                Stage::Setup => self.setup.start(&mut self.res, &mut self.shared),
                Stage::Station => self.station.start(&mut self.res, &mut self.shared),
                Stage::Client => match self.back.take() {
                    Some(disconnect) => self.client.back_from_world(&mut self.res, &mut self.shared, disconnect),
                    None => self.client.start(&mut self.res, &mut self.shared),
                },
                Stage::Boot => {}
            }
        }
    }

    fn files_of(&self, s: Stage) -> &'static [&'static str] {
        match s {
            Stage::Boot => res::BASE,
            Stage::Setup => setup::FILES,
            Stage::Station => station::FILES,
            Stage::Client => client::FILES,
        }
    }

    /// Begins the stage `s`: its files asked for, the loading picture its
    /// module's `Init` shows up meanwhile (STATION's: a random one; CLIENT
    /// has its own loading screen, `client`).
    fn enter(&mut self, s: Stage, now: i64) {
        self.res.want(self.files_of(s));
        match s {
            Stage::Station => self.show_picture(boot::Step::Station, now),
            // SLUS started over (STATION's Disconnect, a disconnect's restart):
            // SETUP's Init shows the next picture while it loads
            Stage::Setup if self.stage != Stage::Boot => self.show_picture(boot::Step::Setup, now),
            _ => {}
        }
        self.entering = Some(s);
        self.entering_since = now;
    }

    /// One frame drawn: the stage's windows (once its files are in), and
    /// what it asks for next.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(&mut self, cx_now: i64, game_frame: bool, r: &mut dyn crate::ui::Layout, cues: &mut Vec<crate::sound::Cue>, music: &mut Vec<Music>, gpu: Option<crate::play::Gpu>) {
        self.update(cx_now);
        // the keyboard's next event, read once a frame of the game's
        if game_frame && let Some((k, down)) = self.keyboard.pop_front() {
            self.dispatch(k, down);
        }
        if self.entering.is_some() || self.stage == Stage::Boot {
            return;
        }
        let Some(vui) = self.res.vui() else { return };
        let mut cx = Ctx { now: cx_now, game_frame, vui, r, res: &self.res, shared: &mut self.shared, cues, music, gpu };
        let next = match self.stage {
            Stage::Setup => self.setup.frame(&mut cx),
            Stage::Station => self.station.frame(&mut cx),
            Stage::Client => self.client.frame(&mut cx),
            Stage::Boot => Next::Stay,
        };
        match next {
            Next::Stay => {}
            Next::Setup => {
                self.station.stop();
                self.client.stop();
                // back at the title: the disc's words and retail's rules
                // until a server's patch
                self.res.set_server_words(None);
                crate::s2::set(crate::s2::Rules::retail());
                self.enter(Stage::Setup, cx_now);
            }
            Next::Station => {
                if self.stage == Stage::Client {
                    self.client.stop();
                } else {
                    // past the patch: the server's words, if its folder has
                    // them, and its S2's rules (`s2`)
                    let words = self.shared.server.as_deref().and_then(crate::servers::words);
                    self.res.set_server_words(words);
                    match self.shared.server.as_deref() {
                        Some(host) => crate::s2::for_server(host),
                        None => crate::s2::set(crate::s2::Rules::retail()),
                    }
                }
                self.enter(Stage::Station, cx_now);
            }
            Next::Client => self.enter(Stage::Client, cx_now),
            Next::World(e) => self.entered = Some(e),
        }
    }

    /// The art swapped (F9, `era`), its files in: what the stages keep made of
    /// them is made again in it (the title's effects, character select's and
    /// creation's figures and stage). The 2D layer is the window's (`host`).
    pub fn art_changed(&mut self, era: esf::Era) {
        self.setup.art_changed(era);
        self.client.art_changed();
    }

    /// CLIENT's files are being read (`LoadWorld`, "PreGame Load"): CLIENT's
    /// own loading screen is up (`LoadingProgress`), not the launcher's.
    pub fn client_loading(&self) -> bool {
        self.entering == Some(Stage::Client)
    }

    /// The world entry, once it has come.
    pub fn take_entered(&mut self) -> Option<Box<Entered>> {
        self.entered.take()
    }

    /// The 3D the stage wants drawn this frame (character select's and
    /// creation's scene), under the 2D layer's later quads.
    pub fn scene(&self) -> Option<Scene<'_>> {
        (self.stage == Stage::Client && self.entering.is_none()).then(|| self.client.scene()).flatten()
    }

    /// macOS's Settings… (⌘,): SETUP's title's Options (`setup::ask_options`);
    /// STATION, the server list and character select have none of the game's.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn ask_options(&mut self) {
        if self.stage == Stage::Setup {
            self.setup.ask_options();
        }
    }

    /// One of the game's screens up and waiting on the player, nothing being
    /// typed and nothing loading or connecting: where a box of the client's
    /// own may stand (`update`'s question). The title's menu, STATION's page
    /// (not its editor), the server list and character select.
    pub fn quiet(&self) -> bool {
        self.entering.is_none()
            && self.entered.is_none()
            && match self.stage {
                Stage::Setup => self.setup.focus == Some(setup::Focus::Title),
                Stage::Station => self.station.quiet(),
                Stage::Client => self.client.quiet(),
                Stage::Boot => false,
            }
    }

    /// For the window's title: where the pregame stands.
    pub fn describe(&self) -> String {
        match (self.stage, self.entering) {
            (_, Some(s)) => format!("loading ({s:?})"),
            (s, None) => format!("{s:?}"),
        }
    }
}
