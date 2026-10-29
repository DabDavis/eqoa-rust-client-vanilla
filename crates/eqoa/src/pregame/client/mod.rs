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
    /// The live game's record's colours (go-eqoa-server's live look): its
    /// held items', armour slots' and robe's, and the slots' ArmorGroup
    /// variants where the disc's has tints; then no tint is sent.
    pub colors: Option<net::entity::LiveColors>,
}

impl CharRecord {
    /// `CharacterList`'s read: the zigzags, then the 32 raw bytes (`m_robe`
    /// as an int, `m_p40`, `m_weapons` three ints, `m_p50`, `m_armor`,
    /// `m_p59`).
    pub fn of(c: &net::session::Character) -> Self {
        let g = |i: usize| c.gear.get(i).copied().unwrap_or(0);
        let word = |o: usize| i32::from_le_bytes([g(o), g(o + 1), g(o + 2), g(o + 3)]);
        if c.gear.len() >= net::session::LIVE_GEAR_SIZE {
            return CharRecord::of_live(c);
        }
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
            colors: None,
        }
    }

    /// The live game's record (its `CharacterList`, Corsten's 0x012CC018):
    /// the zigzags, then `m_robe`, the three held items, their attack types,
    /// the armour sets, six ArmorGroup variants, the held items' colours, the
    /// armour slots' and the robe's (r, g, b, a each); no robe tint.
    fn of_live(c: &net::session::Character) -> Self {
        let g = |i: usize| c.gear.get(i).copied().unwrap_or(0);
        let word = |o: usize| i32::from_le_bytes([g(o), g(o + 1), g(o + 2), g(o + 3)]);
        let rgba = |o: usize| [g(o), g(o + 1), g(o + 2), g(o + 3)];
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
            robe_tint: 0,
            weapons: [word(4) as u32, word(8) as u32, word(12) as u32],
            attack: [g(16), g(17), g(18)],
            armor: std::array::from_fn(|k| g(19 + k)),
            tints: [0; 6],
            colors: Some(net::entity::LiveColors {
                variants: std::array::from_fn(|k| g(25 + k)),
                items: std::array::from_fn(|k| rgba(31 + 4 * k)),
                armor: std::array::from_fn(|k| rgba(43 + 4 * k)),
                robe: rgba(67),
            }),
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
}

impl<'a> UiCx<'a> {
    /// A context with nothing but the device (a test's).
    #[cfg(test)]
    pub fn bare(vui: &'a Vui, r: &'a mut dyn crate::ui::Layout) -> Self {
        UiCx { vui, r, now: 0, state: 20, status: Vec::new(), home_shard: Vec::new(), expansion: DISC_EXPANSION, cues: Vec::new(), music: Vec::new(), gpu: None, lib: None, race_cams: None }
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
    /// The art swapped (F9): the stage and the figures to be made again in
    /// it, at the next frame with the GPU.
    remake: bool,
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
            remake: false,
        }
    }
}

impl Client {
    pub fn new() -> Self {
        Self::default()
    }

    /// The server list or character select up and waiting on the player,
    /// none of their boxes up: `Pregame::quiet`.
    pub fn quiet(&self) -> bool {
        self.focus.is_none()
            && self.entered.is_none()
            && match (self.phase, self.active) {
                (1, Wnd::Server) => self.server_wnd.quiet(),
                (3, Wnd::Select) => self.select.quiet(),
                _ => false,
            }
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

    /// The art swapped (F9, `era`), its files in: the stage, character
    /// select's figure (and customise's) and creation's made again in it at
    /// the next frame.
    pub fn art_changed(&mut self) {
        self.remake = true;
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
        self.chars = list.iter().take(8).map(CharRecord::of).collect();
        // creation's form and pages start over; the selection stays where
        // it was (`CreateCharacter` left it on the new character's slot)
        self.create.reset();
        self.select_up = true;
        self.top = Wnd::Select;
        self.set_active(Wnd::Select);
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
        // the art swapped: what is made of its files made again in it
        if self.remake
            && let Some(g) = cx.gpu.as_mut()
        {
            self.remake = false;
            if let Some(st) = self.stage.as_mut() {
                st.remake(&cx.res.lib, g);
            }
            self.select.remake(&cx.res.lib, g, cx.now);
            self.create.remake(&cx.res.lib, g, cx.now);
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

    /// What creation said: back out of its first page (`BackToLogin`,
    /// creation reset itself), or Create (`CreateCharacter`).
    fn create_out(&mut self, out: create::Out) {
        match out {
            create::Out::Stay => {}
            create::Out::Back => {
                self.top = Wnd::Select;
                self.set_active(Wnd::Select);
            }
            create::Out::Done => {
                let f = self.create.sent().clone();
                self.create_character(&f);
            }
        }
    }

    /// `CreateCharacter`, once creation has sent the form (0x002B): the form
    /// becomes the next entry with no server id yet (its race's model by
    /// sex, no robe, level 1, the rest zero), selected and shown, and
    /// `ShowWord(413)` (empty: the pending states end). The server's list
    /// replaces it; a refusal takes it away.
    fn create_character(&mut self, f: &net::session::NewChar) {
        let rec = CharRecord {
            server_id: -1,
            model: create::tables::get_race(f.race).model_id(f.sex),
            class: f.class,
            race: f.race,
            level: 1,
            hair_color: f.hair_color,
            hair_length: f.hair_length,
            hair_style: f.hair_style,
            face: f.face,
            robe: -1,
            ..Default::default()
        };
        self.select.sel = self.chars.len() as i32;
        self.chars.push(rec);
        self.top = Wnd::Select;
        self.set_active(Wnd::Select);
        self.select.show_text_none();
    }

    /// A refusal of 0x002B (0x002E 269, 0x0065 828, 0x002F 829): the word in
    /// select's box and `m_charCount--` (`CreateCharacter`'s entry goes;
    /// the console counts down whatever is last).
    fn refused(&mut self, vui: &Vui, id: i32) {
        self.select.show_word(vui, id);
        self.chars.pop();
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
                    self.phase = 3;
                    self.state = 21;
                    self.character_list(list);
                }
                // 0x0012: `m_wndCharCreate->SetName`, whatever is up
                Event::RandomName(_) => {
                    let out = self.create.event(e);
                    self.create_out(out);
                }
                Event::CreateRefused(id) => self.refused(cx.vui, *id),
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
                    // `EQOA_GAME_PORT` (a page's `?game_port=`): a test server's list names the default port
                    let ep = if super::port_given("EQOA_GAME_PORT") { (ep & !0xFFFF_0000) | (super::port("EQOA_GAME_PORT", super::GAME_PORT) as u64) << 16 } else { ep };
                    // the hello the server's patch makes it (`s2`: the live S2's), else retail's
                    if let Some(s) = self.session.as_mut() {
                        s.connect_shard(ep, crate::s2::get().shard_hello().as_deref());
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

    /// An editor has the keys (creation's name).
    pub fn typing(&self) -> bool {
        self.active == Wnd::Create && self.create.typing()
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

    /// The live game's record (go-eqoa-server's `entity.LiveGear` for its
    /// test player: robe 2, the primary 0x11223344 of type 3, sets 4 4 3 2 1 4,
    /// chest d10000ff, boots 5a5a78ff, robe 008221ff, the primary 0000ffff)
    /// read as the live CLIENT's `CharacterList` reads it, and dressed with
    /// its colours on the select screen. The control: the disc's record has
    /// none, and dresses in its tints.
    #[test]
    fn a_live_record_reads_its_colours() {
        let hex = "020000004433221100000000000000000300000404030201040000000000000000ffffffffffffffffffffd10000ffffffffffffffffffffffffff5a5a78ffffffffff008221ff";
        let g: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
        assert_eq!(g.len(), net::session::LIVE_GEAR_SIZE);
        let r = CharRecord::of(&character("Tinctoria", 9, g));
        assert_eq!((r.robe, r.robe_tint, r.weapons, r.attack), (2, 0, [0x11223344, 0, 0], [3, 0, 0]));
        assert_eq!((r.armor, r.tints), ([4, 4, 3, 2, 1, 4], [0; 6]));
        let l = r.colors.expect("the live record's colours");
        assert_eq!(l.armor, [[0xD1, 0, 0, 0xFF], [0xFF; 4], [0xFF; 4], [0xFF; 4], [0x5A, 0x5A, 0x78, 0xFF], [0xFF; 4]]);
        assert_eq!((l.robe, l.items[0], l.variants), ([0, 0x82, 0x21, 0xFF], [0, 0, 0xFF, 0xFF], [0; 6]));
        let look = select::look_of(&r, true);
        assert_eq!((look.armor_color, look.robe_color), (Some(l.armor), Some(l.robe)));
        let disc = CharRecord::of(&character("Plain", 8, vec![0; net::session::DISC_GEAR_SIZE]));
        assert!(disc.colors.is_none() && select::look_of(&disc, true).armor_color.is_none(), "the control: the disc's record");
    }

    /// Where a box of the client's own may stand (`update`'s question): the
    /// server list and character select up and waiting, none of their
    /// boxes up; not creation, not while connecting, not before; and the
    /// pregame's title menu, not its Network Setup nor the boot.
    #[test]
    fn quiet_is_a_list_waiting_on_the_player() {
        let mut c = Client::new();
        assert!(!c.quiet(), "the control: nothing up yet");
        (c.phase, c.top, c.active) = (1, Wnd::Server, Wnd::Server);
        assert!(c.quiet(), "the server list");
        c.server_wnd.connecting = true;
        assert!(!c.quiet(), "connecting to a shard");
        c.server_wnd.connecting = false;
        (c.phase, c.top, c.active) = (3, Wnd::Select, Wnd::Select);
        assert!(c.quiet(), "character select");
        c.select.set_pending();
        assert!(!c.quiet(), "its busy box up");
        let mut c = Client::new();
        (c.phase, c.top, c.active) = (3, Wnd::Create, Wnd::Create);
        assert!(!c.quiet(), "creation (a name is typed there)");
        (c.phase, c.active) = (4, Wnd::Select);
        assert!(!c.quiet(), "entering the world");
        let mut p = crate::pregame::Pregame::new(None);
        assert!(!p.quiet(), "the boot's pictures");
        p.stage = crate::pregame::Stage::Setup;
        p.setup.focus = Some(crate::pregame::setup::Focus::Title);
        assert!(p.quiet(), "the title's menu");
        p.setup.focus = Some(crate::pregame::setup::Focus::NetSetup);
        assert!(!p.quiet(), "Network Setup (an address is typed there)");
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
        let shards = [net::session::Shard { name: "Marr's Fist".into(), recommended: 1, endpoint: 77, language: 0 }];
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
        let reqs = std::mem::take(&mut e.reqs);
        e.run(reqs, &mut Vec::new());
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

    /// A session nothing answers (creation's pages send on it).
    fn quiet_session() -> Session {
        struct Nowhere;
        impl net::session::Datagrams for Nowhere {
            fn send_to(&mut self, _: &[u8], _: [u8; 4], _: u16) {}
            fn recv_from(&mut self) -> Option<(Vec<u8>, [u8; 4], u16)> {
                None
            }
        }
        Session::connect_with(Box::new(Nowhere), ([127, 0, 0, 1], 5000), ([127, 0, 0, 1], 10070), "test", [0; 32]).unwrap()
    }

    /// `CreateCharacter`: the form creation sent becomes the next entry with
    /// no server id yet (its race's model by sex, no robe, level 1, nothing
    /// worn), selected, select up, its pending states ended (`ShowWord(413)`).
    /// A refusal shows its word and takes the entry away; creation's back
    /// adds none.
    #[test]
    fn a_created_character_waits_in_the_list() {
        let mut c = Client::new();
        c.chars = vec![CharRecord::of(&character("A", 1, vec![7; 32]))];
        (c.top, c.active) = (Wnd::Create, Wnd::Create);
        c.select.set_pending();
        let form = net::session::NewChar { name: "Brakk".into(), race: 4, class: 9, sex: 1, hair_color: 2, hair_length: 1, hair_style: 3, face: 2, city: 0, stats: [0; 7] };
        c.create_character(&form);
        assert_eq!(c.chars.len(), 2);
        let n = &c.chars[1];
        assert_eq!((n.server_id, n.race, n.class, n.level, n.robe), (-1, 4, 9, 1, -1));
        assert_eq!(n.model, vi::hash::hash_resource_id("DwarfFemale"));
        assert_eq!((n.hair_color, n.hair_length, n.hair_style, n.face), (2, 1, 3, 2));
        assert_eq!((n.armor, n.weapons, n.name.as_str()), ([0; 6], [0; 3], ""));
        assert_eq!((c.select.sel, c.top, c.active), (1, Wnd::Select, Wnd::Select));
        assert_eq!(c.select.pending(), (false, false, false), "ShowWord(413): nothing pending, no box");
        let vui = crate::pointer::testing::vui();
        c.refused(&vui, 269);
        assert_eq!(c.chars.len(), 1);
        assert_eq!(c.chars[0].name, "A", "the listed character stays");
        assert!(c.select.pending().2, "the refusal's word in the box");
        // the control: back out of creation leaves the list as it was
        (c.top, c.active) = (Wnd::Create, Wnd::Create);
        c.create_out(create::Out::Back);
        assert_eq!((c.chars.len(), c.top), (1, Wnd::Select));
    }

    /// The server's list resets creation's form (`CharacterList`'s `Reset`)
    /// and leaves the selection where it was (on the slot `CreateCharacter`
    /// gave the new character).
    #[test]
    fn the_list_resets_creation_and_keeps_the_selection() {
        let mut c = Client::new();
        let mut s = quiet_session();
        let v = create::tests_vui();
        let mut r = crate::pointer::testing::NullRaster;
        let res = crate::pregame::res::Res::new();
        let mut shared = Shared::default();
        let (mut cues, mut music) = (Vec::new(), Vec::new());
        c.create.open();
        for (t, k) in [(0, None), (40, Some(crate::pointer::key::CROSS))] {
            if let Some(k) = k {
                c.create.key(k, true);
                c.create.key(k, false);
            }
            let mut cx = Ctx { now: t, game_frame: true, vui: &v, r: &mut r, res: &res, shared: &mut shared, cues: &mut cues, music: &mut music, gpu: None };
            c.create.frame(&mut cx, &mut s, None);
        }
        assert_eq!(c.create.current().race, 6, "the control: the race page's choice is in the form");
        c.select.sel = 1;
        c.phase = 3;
        c.character_list(&[character("A", 1, vec![0; 32]), character("B", 2, vec![0; 32])]);
        assert_eq!(c.create.current(), &create::blank_form());
        assert_eq!((c.select.sel, c.top, c.chars.len()), (1, Wnd::Select, 2));
    }
}
