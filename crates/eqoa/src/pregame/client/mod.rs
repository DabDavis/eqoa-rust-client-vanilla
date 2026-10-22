//! CLIENT before the world: `VIClient`'s pregame half (`InitBody`,
//! `ShowLoginWindow`, `Connect`, `ConnectShard`, `CharacterList`,
//! `SelectCharacter`, `Disconnect`, `Msg0777Done`...) and its windows: the
//! server list (`server.rs`, `VIWndServerSelect`), character select
//! (`select.rs`, `VIWndCharacterSelect`, with Frontiers' customise,
//! `cust.rs`), the disconnect notice (`notice.rs`, `VIWndDisconnect`), and
//! character creation (`create/`, the creation port's).
//!
//! As the game runs them: once a frame the session's messages are taken
//! (`PumpMessages`) and the window on top draws (`m_p2BD44->OnDraw`); a key
//! goes to the window that has the focus (`m_p2BD40`). The windows' calls on
//! `VIClient` are requests (`Req`) done after their draw or key, in order.
//!
//! Left out, each where it would be: `VIWndInitFromMc` (the memory card's
//! check and the options it loads: the client's are `settings.rs`'s) and the
//! card's checks before a shard is connected to.

pub mod create;
pub mod cust;
pub mod notice;
pub mod races;
pub mod select;
pub mod server;

use super::{Ctx, Entered, Login, Music, Next, Shared, res::Res};
use crate::camstyle::RaceCamera;
use crate::pregame::stage::Stage;
use net::session::{Event, Session};
use vi::ui::{Vui, wide};

/// What `VIClient::LoadWorld` ("PreGame Load") reads after the base files.
pub const FILES: &[&str] = &[
    "DATA2/PREGAME.CSF",
    "DATA2/ARIAL.CSF",
    "DATA2/ARENA.CSF",
    "DATA2/ITEM.CSF",
    "DATA2/CHARCUST.CSF",
    "DATA2/CHARFACE.CSF",
    "DATA2/CHARSEL1.CSF",
    "DATA2/CHARSEL2.CSF",
    "DATA2/CHARSEL3.CSF",
    "DATA2/CHARSEL4.CSF",
    "DATA2/DEBUG.CSF",
    "DATA2/FX.CSF",
    "DATA2/SKY.CSF",
];

/// `VIGetExpansionFlags()`: what a Frontiers disc owns (SLUS's
/// `VISetExpansionFlags(3)`), which `ExpansionFlags()` ANDs with the
/// server's 0x07D1 mask.
pub const DISC_EXPANSION: u32 = 3;

/// A shard as the list shows it (`VIElem_BFC798`): its name, `m_x40` (the
/// server recommends it), whether it can be connected to (`m_id` not -1),
/// its endpoint.
#[derive(Clone, Debug, PartialEq)]
pub struct ShardEntry {
    pub name: Vec<u16>,
    pub recommended: bool,
    pub valid: bool,
    pub endpoint: u64,
}

impl ShardEntry {
    fn of(s: &net::session::Shard) -> Self {
        ShardEntry { name: wide(&s.name), recommended: s.recommended != 0, valid: s.endpoint != u64::MAX, endpoint: s.endpoint }
    }
}

/// `VICharRecord`: a character as 0x002C lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharRecord {
    pub name: String,
    /// -1: being made (`CreateCharacter`'s entry, until the list comes)
    pub server_id: i64,
    pub model: u32,
    pub class: i32,
    pub race: i32,
    pub level: i32,
    pub hair_color: i32,
    pub hair_length: i32,
    pub hair_style: i32,
    pub face: i32,
    /// `m_robe` (-1 none), `m_p40` its tint
    pub robe: i32,
    pub robe_tint: u8,
    /// the three held items' DictIDs, and their attack types (`m_p50`)
    pub weapons: [u32; 3],
    pub attack: [u8; 3],
    /// the six armour sets (`m_armor`) and their tints (`m_p59`)
    pub armor: [u8; 6],
    pub tints: [u8; 6],
}

impl CharRecord {
    /// `CharacterList`'s read: the zigzags, then the 32 raw bytes (`m_robe`
    /// as an int, `m_p40`, `m_weapons` three ints, `m_p50`, `m_armor`,
    /// `m_p59`).
    pub fn of(c: &net::session::Character) -> Self {
        let g = |i: usize| c.gear.get(i).copied().unwrap_or(0);
        let word = |o: usize| i32::from_le_bytes([g(o), g(o + 1), g(o + 2), g(o + 3)]);
        CharRecord {
            name: c.name.clone(),
            server_id: c.server_id,
            model: c.model as u32,
            class: c.class as i32,
            race: c.race as i32,
            level: c.level as i32,
            hair_color: c.hair_color as i32,
            hair_length: c.hair_length as i32,
            hair_style: c.hair_style as i32,
            face: c.face as i32,
            robe: word(0),
            robe_tint: g(4),
            weapons: [word(5) as u32, word(9) as u32, word(13) as u32],
            attack: [g(17), g(18), g(19)],
            armor: std::array::from_fn(|k| g(20 + k)),
            tints: std::array::from_fn(|k| g(26 + k)),
        }
    }

    /// The session's form of it, with the look as it stands (0x002A sends
    /// the server id and the face and hair).
    fn character(&self) -> net::session::Character {
        net::session::Character {
            name: self.name.clone(),
            server_id: self.server_id,
            model: self.model as i64,
            class: self.class as i64,
            race: self.race as i64,
            level: self.level as i64,
            hair_color: self.hair_color as i64,
            hair_length: self.hair_length as i64,
            hair_style: self.hair_style as i64,
            face: self.face as i64,
            gear: Vec::new(),
        }
    }
}

/// What a window asks of `VIClient`.
#[derive(Clone, Debug, PartialEq)]
pub enum Req {
    /// `ConnectRandomShard`: a connection for the list
    ConnectForList,
    /// `ConnectShard(id)`
    ConnectShard(u64),
    /// `m_p4F0EC = code; Disconnect(reason, quiet)`
    Disconnect { reason: i32, quiet: bool, code: i32 },
    /// `Msg0777Done(mode)`
    Done(i32),
    /// the server list's message closed: `Msg0777Done` by `DisconnectKind`
    DoneByKind,
    /// `SetHomeShardName` and the options saved
    Home(Vec<u16>),
    /// `SelectCharacter(i)`
    Select(usize),
    /// `DeleteCharacter(i)`
    Delete(usize),
    /// `SetCharLook(i, face, hair style, hair length, hair colour)`
    Look(usize, [i32; 4]),
    /// `Msg0777`: back to the list, quietly
    Msg0777,
}

/// The window on top (`m_p2BD44`) and the one with the keys (`m_p2BD40`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Wnd {
    #[default]
    None,
    Server,
    Select,
    Create,
    Notice,
}

/// What a window's draw reads of `VIClient` and draws with.
pub struct UiCx<'a> {
    pub vui: &'a Vui,
    pub r: &'a mut dyn crate::ui::Layout,
    /// `m_p07568`, the client's clock (ms)
    pub now: i64,
    /// `m_p4F0E8`
    pub state: i32,
    /// `ConnectStatusString()`
    pub status: Vec<u16>,
    /// game.opt's home shard (`HomeShardName`)
    pub home_shard: Vec<u16>,
    /// `ExpansionFlags()`
    pub expansion: u32,
    pub cues: Vec<i32>,
    pub music: Vec<Music>,
    pub gpu: Option<crate::play::Gpu<'a>>,
    pub lib: Option<&'a esf::Library>,
    pub race_cams: Option<&'a [Vec<RaceCamera>; 2]>,
    pub game_frame: bool,
}

impl<'a> UiCx<'a> {
    /// A context with nothing but the device (a test's).
    #[cfg(test)]
    pub fn bare(vui: &'a Vui, r: &'a mut dyn crate::ui::Layout) -> Self {
        UiCx { vui, r, now: 0, state: 20, status: Vec::new(), home_shard: Vec::new(), expansion: DISC_EXPANSION, cues: Vec::new(), music: Vec::new(), gpu: None, lib: None, race_cams: None, game_frame: true }
    }
}

/// `VIClient::Connect(host, port)`'s socket and first sends: the session to
/// the server's login endpoint (`EQOA_GAME_PORT` another port), opened
/// through `link` (natively the name resolved on a thread, in a page the
/// relay's).
fn connect(server: &str, login: &Login) -> crate::pregame::link::Opening {
    crate::pregame::link::session(server, super::port("EQOA_GAME_PORT", super::GAME_PORT), &login.user, login.token)
}

/// `Connect`'s failures as `Disconnect`'s reasons: the name not resolved
/// is 2 ("Server not found. (DNS error)"), anything else 5 ("Network
/// Error.").
fn open_failure(e: &str) -> i32 {
    let e = e.to_ascii_lowercase();
    if e.contains("no address") || e.contains("lookup") || e.contains("resolve") || e.contains("name or service") { 2 } else { 5 }
}

pub struct Client {
    session: Option<Session>,
    making: Option<crate::pregame::link::Opening>,
    /// `m_p4F0E8`: 20 connecting, 21 connected and listed, else the last
    /// disconnect's reason
    state: i32,
    /// `m_p4F0EC`: the server's code with a disconnect, -1 none
    code: i32,
    /// `m_p00058`: 0 waiting for the list, 1 listed, 2 to a shard, 3 the
    /// characters, 4 entering, -1 disconnected
    phase: i32,
    /// `m_p4F130`: character select has been up since the list (a
    /// disconnect then puts up the notice)
    select_up: bool,
    shards: Vec<ShardEntry>,
    pub chars: Vec<CharRecord>,
    top: Wnd,
    active: Wnd,
    /// a focus change waiting for the next draw (295 sizes windows)
    focus: Option<(Wnd, Wnd)>,
    server_wnd: server::ServerSelect,
    select: select::CharSelect,
    notice: notice::Notice,
    pub create: create::Create,
    /// the character screens' 3D, made once ARENA.CSF and the GPU are here
    stage: Option<Stage>,
    race_cams: Option<[Vec<RaceCamera>; 2]>,
    /// sounds and requests from keys, done at the next frame
    cues: Vec<i32>,
    reqs: Vec<Req>,
    now: i64,
    next: Option<Next>,
    server: Option<String>,
    login: Option<Login>,
    entered: Option<Box<Entered>>,
}

impl Default for Client {
    fn default() -> Self {
        Client {
            session: None,
            making: None,
            state: 20,
            code: -1,
            phase: -1,
            select_up: false,
            shards: Vec::new(),
            chars: Vec::new(),
            top: Wnd::None,
            active: Wnd::None,
            focus: None,
            server_wnd: Default::default(),
            select: Default::default(),
            notice: Default::default(),
            create: create::Create::new(),
            stage: None,
            race_cams: None,
            cues: Vec::new(),
            reqs: Vec::new(),
            now: 0,
            next: None,
            server: None,
            login: None,
            entered: None,
        }
    }
}

impl Client {
    pub fn new() -> Self {
        Self::default()
    }

    /// `client_main` with the login STATION handed SLUS: `InitBody`, then
    /// (the card's check left out) `ShowLoginWindow`.
    pub fn start(&mut self, _res: &mut Res, shared: &mut Shared) {
        self.server = shared.server.clone();
        self.login = shared.login.clone();
        self.session = None;
        self.making = None;
        self.state = 20;
        self.code = -1;
        self.phase = -1;
        self.next = None;
        self.entered = None;
        self.chars.clear();
        self.show_login_window();
    }

    /// Back from the world, which the window has dropped (`VIClient::Process`
    /// and the notice):
    ///
    /// - `None`, the start menu's logout (`m_p2EC28`: 0x09B0 was sent,
    ///   `Disconnect(9, 1)`, `Msg0777Done(2)`): the server list again, its
    ///   connection made anew after its 50 frames;
    /// - `Some(reason)`, the connection lost in the world (the reason as
    ///   `Event::Disconnected` gives it, which is `m_p4F0E8`'s:
    ///   `session.rs` maps a DRDP disconnect's byte 14 to 16, 1 to 1, 15 to
    ///   15, any other to 10, and the dispatcher's own cases 3..19):
    ///   `Disconnect(reason, 0)` puts up the notice (`VIWndDisconnect`),
    ///   whose Ok goes by `DisconnectKind`: 2 the server list, 3 STATION's
    ///   login (`frame` returns `Next::Station`), 4 the title
    ///   (`Next::Setup`), 0 nowhere.
    ///
    /// The Client stage must be the pregame's stage again when this is
    /// called (its files are still in `res`).
    pub fn back_from_world(&mut self, res: &mut Res, shared: &mut Shared, disconnect: Option<i32>) {
        let _ = res;
        self.server = shared.server.clone();
        self.login = shared.login.clone();
        self.session = None;
        self.making = None;
        self.next = None;
        self.entered = None;
        self.code = -1;
        // the world was entered from character select (`m_p4F130` stays 1)
        self.select_up = true;
        self.active = Wnd::None;
        let mut music = Vec::new();
        match disconnect {
            None => {
                self.disconnect(9, true, &mut music);
                self.done(2);
            }
            Some(reason) => self.disconnect(reason, false, &mut music),
        }
    }

    /// The stage left (Back to STATION, or the title): the connection goes.
    pub fn stop(&mut self) {
        if let Some(s) = self.session.as_mut() {
            s.flush();
        }
        self.session = None;
        self.making = None;
        self.top = Wnd::None;
        self.active = Wnd::None;
    }

    /// `ShowLoginWindow`: the server list up again, from the top.
    fn show_login_window(&mut self) {
        self.select_up = false;
        self.shards.clear();
        self.set_active(Wnd::Server);
        self.top = Wnd::Server;
    }

    /// `SetActiveWindow(w)`: the windows hear it (295) at the next draw.
    fn set_active(&mut self, w: Wnd) {
        let old = self.active;
        // the window losing the keys lets go of what it showed
        if old == Wnd::Select && !matches!(w, Wnd::Select) {
            self.select.lost_focus();
        }
        self.focus = Some((old, w));
        self.active = w;
    }

    /// `ExpansionFlags()`.
    fn expansion(&self) -> u32 {
        DISC_EXPANSION & self.session.as_ref().map_or(0, |s| s.expansion)
    }

    /// `DisconnectKind()`: 2 back to the list, 3 to STATION, 4 to the title.
    pub fn disconnect_kind(state: i32) -> i32 {
        match state {
            3 | 4 | 8 | 9 => 3,
            0 | 1 | 2 | 6 | 7 | 10 | 13 | 15 | 17 | 19 => 2,
            5 | 11 | 12 | 14 | 16 | 18 => 4,
            _ => 0,
        }
    }

    /// `ConnectStatusString()`: the words for the state; 0 and 10 with the
    /// server's code when there is one.
    pub fn status_string(vui: &Vui, state: i32, code: i32) -> Vec<u16> {
        let w = |id: i32| vui.words.get(id, vui.language).to_string();
        let s = match state {
            0 if code != -1 => format!("{} ({}: {code})", w(272), w(819)),
            10 if code != -1 => format!("{} ({code})", w(667)),
            20 => w(758),
            21 => w(822),
            s => net::session::disconnect_word(s).map(w).unwrap_or_default(),
        };
        wide(&s)
    }

    /// `Disconnect(reason, quiet)`: the music stops, the connection goes;
    /// with character select up (and not quiet) the notice comes up.
    fn disconnect(&mut self, reason: i32, quiet: bool, music: &mut Vec<Music>) {
        music.push(Music::StopBgm { slot: 0 });
        music.push(Music::StopBgm { slot: 1 });
        self.phase = -1;
        self.state = reason;
        if !quiet && self.select_up && self.active != Wnd::Notice && reason != 21 {
            // `Notify`
            self.set_active(Wnd::Notice);
            self.top = Wnd::Notice;
        }
        if let Some(s) = self.session.as_mut() {
            s.flush();
        }
        self.session = None;
        self.making = None;
    }

    /// `Msg0777Done(mode)`.
    fn done(&mut self, mode: i32) {
        match mode {
            // `client_main` returns 0: SLUS runs STATION again
            3 => {
                self.next = Some(Next::Station);
                self.stop();
            }
            // the world (none here) let go, the list again
            2 => self.show_login_window(),
            // `client_main` returns -1: SLUS starts over
            4 => {
                self.next = Some(Next::Setup);
                self.stop();
            }
            _ => {}
        }
    }

    /// `CharacterList`: the characters (eight at most), creation closed,
    /// character select up.
    fn character_list(&mut self, list: &[net::session::Character]) {
        let was_creating = self.top == Wnd::Create;
        let old: Vec<i64> = self.chars.iter().map(|c| c.server_id).collect();
        self.chars = list.iter().take(8).map(CharRecord::of).collect();
        self.select_up = true;
        self.top = Wnd::Select;
        self.set_active(Wnd::Select);
        // `CreateCharacter` put the selection on the new character's slot
        if was_creating && let Some(i) = self.chars.iter().position(|c| !old.contains(&c.server_id)) {
            self.select.sel = i as i32;
        }
    }

    /// One frame: the session's messages, then the window on top draws.
    pub fn frame(&mut self, cx: &mut Ctx) -> Next {
        self.now = cx.now;
        // the race cameras, once CLIENT's image is here
        if self.race_cams.is_none()
            && let Some(img) = crate::modules::get(crate::modules::Module::Client).and_then(|i| crate::client_image::ClientImage::from_bytes(i.to_vec()))
        {
            self.race_cams = Some(crate::camstyle::race_cameras(&img));
        }
        // the stage, once ARENA.CSF and the GPU are here (`LoadWorld`'s
        // walk camera limits set as it sets them)
        if self.stage.is_none()
            && let Some(g) = cx.gpu.as_mut()
            && let Some(mut st) = Stage::new(&cx.res.lib, g)
        {
            let f = vi::fpu::F::<vi::fpu::Ee>::new;
            st.cam.min_orbit_distance = f(0.5);
            st.cam.max_orbit_distance = f(25.0);
            st.cam.min_collision_dist = f(3.5);
            self.stage = Some(st);
        }
        let mut music = Vec::new();
        self.pump(&mut music, cx);
        if let Some(e) = self.entered.take() {
            return Next::World(e);
        }
        // the requests and sounds keys made since the last frame
        let mut cues = std::mem::take(&mut self.cues);
        let reqs = std::mem::take(&mut self.reqs);
        self.run(reqs, &mut music);
        let status = Client::status_string(cx.vui, self.state, self.code);
        let expansion = self.expansion();
        let home_shard = crate::settings::home_shard().map(|s| wide(&s)).unwrap_or_default();
        let gpu = cx.gpu.take();
        let mut ui = UiCx {
            vui: cx.vui,
            r: &mut *cx.r,
            now: cx.now,
            state: self.state,
            status,
            home_shard,
            expansion,
            cues: Vec::new(),
            music: Vec::new(),
            gpu,
            lib: Some(&cx.res.lib),
            race_cams: self.race_cams.as_ref(),
            game_frame: cx.game_frame,
        };
        let mut reqs = Vec::new();
        // 295: the focus that moved since the last frame
        if let Some((from, to)) = self.focus.take() {
            match to {
                Wnd::Server => self.server_wnd.focus(&ui, &self.shards),
                Wnd::Select if from != Wnd::Select => self.select.focus(&mut ui),
                Wnd::Notice => self.notice.focus(&ui),
                _ => {}
            }
        }
        let mut created = None;
        match self.top {
            Wnd::Server => self.server_wnd.draw(&mut ui, &self.shards, &mut reqs),
            Wnd::Select => self.select.draw(&mut ui, &self.chars, self.stage.as_mut(), &mut reqs),
            Wnd::Notice => self.notice.draw(&mut ui),
            Wnd::Create => {
                created = Some(match self.session.as_mut() {
                    Some(s) => {
                        let mut ccx = Ctx { now: cx.now, game_frame: cx.game_frame, vui: cx.vui, r: &mut *ui.r, res: cx.res, shared: &mut *cx.shared, cues: &mut *cx.cues, music: &mut music, gpu: ui.gpu.take() };
                        self.create.frame(&mut ccx, s, self.stage.as_mut())
                    }
                    None => create::Out::Back,
                });
            }
            Wnd::None => {}
        }
        cues.append(&mut ui.cues);
        music.append(&mut ui.music);
        drop(ui);
        if let Some(out) = created {
            self.create_out(out);
        }
        self.run(reqs, &mut music);
        cx.cues.extend(cues.into_iter().map(crate::sound::Cue::Ui));
        cx.music.extend(music);
        self.next.take().unwrap_or(Next::Stay)
    }

    /// What creation said.
    fn create_out(&mut self, out: create::Out) {
        match out {
            create::Out::Stay => {}
            create::Out::Back | create::Out::Done => {
                self.top = Wnd::Select;
                self.set_active(Wnd::Select);
            }
        }
    }

    /// `PumpMessages`: the session made (a page's comes later), then its
    /// events as `ProcessMessage` and `ProcessMessageFC` take them.
    fn pump(&mut self, music: &mut Vec<Music>, cx: &Ctx) {
        if let Some(r) = self.making.as_mut().and_then(|o| o.poll()) {
            self.making = None;
            match r {
                Ok(s) => self.session = Some(s),
                Err(e) => {
                    crate::say(&format!("the server list's connection: {e}"));
                    self.disconnect(open_failure(&e), false, music);
                }
            }
        }
        let Some(s) = self.session.as_mut() else { return };
        let events = s.pump();
        for (k, e) in events.iter().enumerate() {
            match e {
                Event::ShardList(list) if matches!(self.phase, 0 | 1) => {
                    self.shards = list.iter().map(ShardEntry::of).collect();
                    if self.phase == 0 {
                        self.phase = 1;
                        self.state = 21;
                    }
                }
                Event::Characters(list) if matches!(self.phase, 2 | 3) => {
                    if self.create.is_open() {
                        let _ = self.create.event(e);
                    }
                    self.phase = 3;
                    self.state = 21;
                    self.character_list(list);
                }
                Event::RandomName(_) if self.create.is_open() => {
                    let out = self.create.event(e);
                    self.create_out(out);
                }
                Event::CreateRefused(id) => {
                    if self.create.is_open() {
                        let out = self.create.event(e);
                        self.create_out(out);
                    } else {
                        // `m_charCount--`: `CreateCharacter`'s entry goes
                        if self.chars.last().is_some_and(|c| c.server_id == -1) {
                            self.chars.pop();
                        }
                        self.select.show_word(cx.vui, *id);
                    }
                }
                Event::Opcode { op, .. } => match op {
                    // `CharSelectMsg`
                    0x07F2 | 0x07F4 => self.char_select_msg(cx.vui, 1662),
                    0x07F0 | 0x07F1 | 0x07F3 => self.char_select_msg(cx.vui, 676),
                    0x0777 => self.reqs.push(Req::Msg0777),
                    _ => {}
                },
                Event::WorldEntry { player, dump, .. } => {
                    let (player, dump) = (player.clone(), dump.clone());
                    let mut s = self.session.take().unwrap();
                    s.held = events[k + 1..].to_vec();
                    self.entered = Some(Box::new(Entered { session: s, player, dump }));
                    return;
                }
                Event::Disconnected(r) => {
                    self.disconnect(*r, false, music);
                    return;
                }
                _ => {}
            }
        }
    }

    /// `CharSelectMsg(id)`: character select up with the word.
    fn char_select_msg(&mut self, vui: &Vui, id: i32) {
        self.phase = 3;
        self.top = Wnd::Select;
        self.set_active(Wnd::Select);
        self.select.show_word(vui, id);
    }

    /// The windows' requests, in order.
    fn run(&mut self, reqs: Vec<Req>, music: &mut Vec<Music>) {
        for q in reqs {
            match q {
                Req::ConnectForList => {
                    // `Connect`: m_p00058 0, the state 20, the hello and 0x0904
                    self.phase = 0;
                    self.state = 20;
                    self.session = None;
                    if let (Some(server), Some(login)) = (self.server.clone(), self.login.clone()) {
                        self.making = Some(connect(&server, &login));
                    } else {
                        self.disconnect(2, false, music);
                    }
                }
                Req::ConnectShard(ep) => {
                    self.phase = 2;
                    self.select.connecting();
                    self.state = 20;
                    let mut ep = ep;
                    // `EQOA_GAME_PORT`: a test server's list names the default port
                    #[cfg(not(target_arch = "wasm32"))]
                    if std::env::var_os("EQOA_GAME_PORT").is_some() {
                        ep = (ep & !0xFFFF_0000) | (super::port("EQOA_GAME_PORT", super::GAME_PORT) as u64) << 16;
                    }
                    if let Some(s) = self.session.as_mut() {
                        s.connect_shard(ep);
                    }
                }
                Req::Disconnect { reason, quiet, code } => {
                    self.code = code;
                    self.disconnect(reason, quiet, music);
                }
                Req::Done(m) => self.done(m),
                Req::DoneByKind => {
                    let k = Client::disconnect_kind(self.state);
                    self.done(if k == 2 { 3 } else { k });
                }
                Req::Home(name) => {
                    if let Err(e) = crate::settings::set_home_shard(&String::from_utf16_lossy(&name)) {
                        crate::say(&format!("the home server was not kept: {e}"));
                    }
                }
                Req::Select(i) => self.select_character(i),
                Req::Delete(i) => {
                    // `DeleteCharacter`: 0x002D, and the list closes up
                    if let (Some(c), Some(s)) = (self.chars.get(i), self.session.as_mut()) {
                        s.delete_character(c.server_id);
                    }
                    if i < self.chars.len() {
                        self.chars.remove(i);
                    }
                }
                Req::Look(i, [face, style, length, color]) => {
                    // `SetCharLook`: Frontiers only
                    if self.expansion() & 2 != 0 && i < self.chars.len() && self.chars.len() < 9 {
                        let c = &mut self.chars[i];
                        (c.face, c.hair_style, c.hair_length, c.hair_color) = (face, style, length, color);
                    }
                }
                Req::Msg0777 => {
                    // `Msg0777`: select hidden, a quiet disconnect, the list again
                    self.code = -1;
                    self.disconnect(0, true, music);
                    self.done(2);
                }
            }
        }
    }

    /// `SelectCharacter(i)`: past the list (fewer than eight), creation;
    /// else the character played (0x002A with its look).
    fn select_character(&mut self, i: usize) {
        if i >= self.chars.len() && self.chars.len() < 8 {
            self.select.show_text_none();
            self.create.open();
            self.top = Wnd::Create;
            self.set_active(Wnd::Create);
        } else if let Some(c) = self.chars.get(i).filter(|c| c.server_id != -1) {
            self.phase = 4;
            let c = c.character();
            if let Some(s) = self.session.as_mut() {
                s.select_character(&c);
            }
        }
    }

    /// A key: to the window that has it.
    pub fn key(&mut self, key: i32, down: bool) {
        let now = self.now;
        match self.active {
            Wnd::Server => self.server_wnd.key(key, down, now, &self.shards, &mut self.cues, &mut self.reqs),
            Wnd::Select => self.select.key(key, down, now, &self.chars, self.expansion(), &mut self.cues, &mut self.reqs),
            Wnd::Notice => {
                if down && self.notice.key(key, &mut self.cues) {
                    let k = Client::disconnect_kind(self.state);
                    self.reqs.push(Req::Done(k));
                }
            }
            Wnd::Create => self.create.key(key, down),
            Wnd::None => {}
        }
    }

    /// A pointer at frame point `p`: the keys the window under it asks for.
    pub fn point(&mut self, p: [f32; 2]) -> Vec<i32> {
        let mut keys = match self.active {
            Wnd::Server => self.server_wnd.point(p),
            Wnd::Select => self.select.point(p, self.chars.len()),
            Wnd::Notice => self.notice.point(p),
            Wnd::Create => self.create.point(p),
            Wnd::None => Vec::new(),
        };
        // a selection moved under the pointer sounds as the d-pad's does
        if keys.contains(&crate::pointer::MOVED) {
            self.cues.push(11);
            keys.retain(|&k| k != crate::pointer::MOVED);
        }
        keys
    }

    pub fn typing(&self) -> bool {
        self.active == Wnd::Create && self.create.typing()
    }

    /// The 3D drawn this frame (character select's preview, customise's,
    /// creation's model).
    pub fn scene(&self) -> Option<super::Scene<'_>> {
        let st = self.stage.as_ref()?;
        match self.top {
            Wnd::Select => self.select.scene(st),
            Wnd::Create => self.create.scene(st),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn character(name: &str, id: i64, gear: Vec<u8>) -> net::session::Character {
        net::session::Character { name: name.into(), server_id: id, model: 0x70D898C6, class: 11, race: 2, level: 7, hair_color: 1, hair_length: 2, hair_style: 3, face: 1, gear }
    }

    /// `CharacterList`'s raw 32 bytes: the robe as an int, its tint, three
    /// item ints, their attack types, six armour sets, six tints.
    #[test]
    fn a_record_reads_its_gear() {
        let mut g = Vec::new();
        g.extend((-1i32).to_le_bytes());
        g.push(4);
        for w in [0x11223344u32, 0, 0x55667788] {
            g.extend(w.to_le_bytes());
        }
        g.extend([11, 0, 5]);
        g.extend([1, 2, 3, 4, 5, 6]);
        g.extend([7, 8, 9, 10, 11, 12]);
        assert_eq!(g.len(), 32);
        let r = CharRecord::of(&character("Vanseldk", 9, g));
        assert_eq!((r.robe, r.robe_tint), (-1, 4));
        assert_eq!(r.weapons, [0x11223344, 0, 0x55667788]);
        assert_eq!(r.attack, [11, 0, 5]);
        assert_eq!((r.armor, r.tints), ([1, 2, 3, 4, 5, 6], [7, 8, 9, 10, 11, 12]));
        // the control: a short gear reads zeros, not a panic
        let s = CharRecord::of(&character("X", 1, vec![1, 2]));
        assert_eq!((s.robe, s.weapons), (0x0201, [0; 3]));
        // 0x002A takes the look as it stands
        let mut t = r.clone();
        (t.face, t.hair_color) = (3, 7);
        let c = t.character();
        assert_eq!((c.server_id, c.face, c.hair_style, c.hair_length, c.hair_color), (9, 3, 3, 2, 7));
    }

    /// `DisconnectKind` and `Msg0777Done`: a refused login goes to STATION,
    /// a lost connection to the list, a network error to the title.
    #[test]
    fn a_disconnect_goes_where_its_kind_says() {
        assert_eq!([3, 8, 0, 10, 5, 12, 20, 21].map(Client::disconnect_kind), [3, 3, 2, 2, 4, 4, 0, 0]);
        let mut c = Client::new();
        c.done(3);
        assert!(matches!(c.next, Some(Next::Station)));
        let mut c = Client::new();
        c.done(4);
        assert!(matches!(c.next, Some(Next::Setup)));
        let mut c = Client::new();
        c.done(2);
        assert!(c.next.is_none() && c.top == Wnd::Server && c.active == Wnd::Server);
        // the control: mode 1 does nothing
        let mut c = Client::new();
        c.done(1);
        assert!(c.next.is_none() && c.top == Wnd::None);
    }

    /// The list and the characters as `ProcessMessageFC` and 0x002C take
    /// them: the list only while waiting for it, the characters only after
    /// a shard was connected to; character select then up.
    #[test]
    fn the_list_then_the_characters() {
        let mut c = Client::new();
        c.show_login_window();
        c.phase = 0;
        let shards = vec![net::session::Shard { name: "Marr's Fist".into(), recommended: 1, endpoint: 77, language: 0 }];
        c.shards = shards.iter().map(ShardEntry::of).collect();
        assert!(c.shards[0].valid && c.shards[0].recommended);
        c.phase = 2;
        c.character_list(&[character("A", 1, vec![0; 32]), character("B", 2, vec![0; 32])]);
        assert_eq!((c.top, c.active, c.chars.len()), (Wnd::Select, Wnd::Select, 2));
        assert!(c.select_up);
        // more than eight are cut to eight
        let nine: Vec<_> = (0..9).map(|i| character("N", i, vec![0; 32])).collect();
        c.character_list(&nine);
        assert_eq!(c.chars.len(), 8);
        // an invalid shard (no endpoint) is not connectable
        let s = ShardEntry::of(&net::session::Shard { name: "Gone".into(), recommended: 1, endpoint: u64::MAX, language: 0 });
        assert!(!s.valid);
    }

    /// `Connect`'s two failures: the name, 2; the socket, 5.
    #[test]
    fn a_failed_opening_is_dns_or_network() {
        assert_eq!(open_failure("eqoa.invalid: no address"), 2);
        assert_eq!(open_failure("eqoa.invalid: failed to lookup address information: Name or service not known"), 2);
        assert_eq!(open_failure("Address already in use (os error 98)"), 5, "the control");
        assert_eq!(Client::disconnect_kind(2), 2);
        assert_eq!(Client::disconnect_kind(5), 4);
    }

    /// `back_from_world`: a logout goes straight to the list, quietly; a
    /// lost connection puts up the notice first, whose Ok goes by the kind.
    #[test]
    fn back_from_the_world() {
        let (mut res, mut shared) = (Res::new(), Shared::default());
        let mut c = Client::new();
        c.back_from_world(&mut res, &mut shared, None);
        assert_eq!((c.top, c.active, c.state), (Wnd::Server, Wnd::Server, 9));
        let mut d = Client::new();
        d.back_from_world(&mut res, &mut shared, Some(10));
        assert_eq!((d.top, d.active, d.state), (Wnd::Notice, Wnd::Notice, 10));
        d.key(134, true);
        assert_eq!(d.reqs, vec![Req::Done(2)], "a lost connection: the list");
        let mut e = Client::new();
        e.back_from_world(&mut res, &mut shared, Some(3));
        e.key(134, true);
        e.run(std::mem::take(&mut e.reqs), &mut Vec::new());
        assert!(matches!(e.next, Some(Next::Station)), "a refused login: STATION");
        // the control: a key other than Ok leaves the notice up
        let mut f = Client::new();
        f.back_from_world(&mut res, &mut shared, Some(10));
        f.key(132, true);
        assert!(f.reqs.is_empty() && f.top == Wnd::Notice);
    }

    /// `SelectCharacter`: an empty slot opens creation; a character sends
    /// 0x002A; a record being made (-1) does nothing.
    #[test]
    fn selecting_an_empty_slot_opens_creation() {
        let mut c = Client::new();
        c.chars = vec![CharRecord::of(&character("A", 1, vec![0; 32]))];
        c.select_character(3);
        assert_eq!((c.top, c.active), (Wnd::Create, Wnd::Create));
        assert!(c.create.is_open());
        let mut d = Client::new();
        d.chars = vec![CharRecord { server_id: -1, ..Default::default() }];
        d.select_character(0);
        assert_eq!((d.top, d.phase), (Wnd::None, -1), "the control: a record being made is not played");
        d.chars[0].server_id = 5;
        d.select_character(0);
        assert_eq!(d.phase, 4);
    }
}
